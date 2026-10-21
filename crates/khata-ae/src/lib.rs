//! United Arab Emirates compliance pack.
//!
//! Implements the VAT items of `docs/research/mena-compliance.md` §1 (see
//! also `docs/uae-vat.md`):
//! - TRN validation: 15 digits, with the TIN (first 10 digits) exposed; no
//!   checksum, because none is published ([`trn`]);
//! - the seven emirates, with stable codes and their VAT 201 boxes
//!   ([`emirates`]);
//! - VAT on a line: one "VAT" component at the class's rate (5% standard
//!   from 1 Jan 2018, 0% zero-rated and exempt classes), exports zero-rated,
//!   designated zones taxed unless marked out of scope ([`vat`]);
//! - tax invoice numbers under ER Art 59(1)(d): unique across all years,
//!   at most 35 characters of `[A-Za-z0-9/-]` (a KhataOS choice; see
//!   [`vat::MAX_INVOICE_NUMBER_LEN`]);
//! - January to December years by default;
//! - the VAT 201 return from outward and inward documents ([`returns`]),
//!   with the reverse charge (Boxes 3 and 10) and customs imports (Boxes 6,
//!   7 and 10) marked on tax classes ([`vat::Mechanism`]).
//!
//! Not modelled yet: the profit margin scheme, tourist refunds, input-tax
//! blocking and apportionment, the FTA Audit File and Corporate Tax.
//!
//! E-invoicing (Peppol PINT AE) is not in this crate: the document and its
//! rules are in `khata-en16931` (`pint`), and the desktop app's `peppol.rs`
//! builds it from the books and sends it through an Accredited Service
//! Provider.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`vat::default_rates`] is a starting list.

pub mod calendar;
pub mod emirates;
pub mod returns;
pub mod trn;
pub mod vat;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "AE", name: "United Arab Emirates", dialing_code: "+971" };

/// UAE dirham, 100 fils. The symbol is the ISO code: the research doesn't
/// cover the currency sign, and "AED" is what invoices print.
pub const CURRENCY: Currency = Currency { code: "AED", symbol: "AED", minor_units: 2 };

/// Books default to calendar years. UAE law fixes no single year: VAT
/// periods are quarterly or monthly as the FTA assigns them (research
/// §1.4), and a Corporate Tax period is the company's financial year, any
/// 12 months (FDL 47/2022 Art 57, research §2). A company can still change
/// its books' first month.
pub const FY_START_MONTH: u32 = 1;

/// The UAE pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct Uae;

impl CompliancePack for Uae {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The seven emirates, in VAT 201 order.
    fn regions(&self) -> Vec<Region> {
        emirates::EMIRATES.iter().map(|e| Region { code: e.code, name: e.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "TRN"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        trn::validate(id)
    }

    /// Unique among every number already issued in the series, in any year.
    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        vat::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        vat::line_tax(line, rates)
    }
    fn calendar_defaults(&self) -> Option<khata_compliance::CalendarDefaults> {
        Some(calendar::defaults())
    }

    fn due_dates(&self, company: &khata_compliance::CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<khata_compliance::DueDate> {
        calendar::due_dates(company, from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use khata_compliance::{PlaceOfSupply, Supply};
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// Everything an app needs goes through the trait object.
    #[test]
    fn works_as_a_trait_object() {
        let pack: &dyn CompliancePack = &Uae;
        assert_eq!((pack.country().code, pack.country().dialing_code), ("AE", "+971"));
        assert_eq!((pack.currency().code, pack.currency().minor_units, pack.tax_id_label()), ("AED", 2, "TRN"));
        let fy = pack.financial_year(d(2027, 1, 15));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2027, 1, 1), d(2027, 12, 31), "2027".into()));
        let trn = pack.validate_tax_id("100-1234-5670-0003").unwrap();
        assert_eq!((trn.value.as_str(), trn.entity_id.as_deref(), trn.region), ("100123456700003", Some("1001234567"), None));

        let issued = [IssuedDocument { number: "7", date: d(2026, 4, 2) }];
        assert!(pack.validate_invoice_number("7", d(2026, 12, 31), &issued).is_err());
        let next_year = pack.validate_invoice_number("7", d(2027, 1, 1), &issued);
        assert_eq!(
            next_year,
            Err(InvoiceNumberError::Duplicate { number: "7".into(), year: "2026".into() }),
            "unlike India, a new year may not reuse it"
        );

        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "STANDARD".into(),
            taxable_value: dec!(50000),
            supply: Supply {
                supplier_region: "DXB".into(),
                place_of_supply: PlaceOfSupply::Region("AUH".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &vat::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "VAT", dec!(2500)));
    }

    #[test]
    fn regions_are_the_seven_emirates() {
        let regions = Uae.regions();
        let codes: Vec<_> = regions.iter().map(|r| r.code).collect();
        assert_eq!(codes, ["AUH", "DXB", "SHJ", "AJM", "UAQ", "RAK", "FUJ"]);
        assert!(regions.contains(&Region { code: "RAK", name: "Ras Al Khaimah" }));
    }

    #[test]
    fn calendar_years_by_default() {
        let fy = Uae.financial_year(d(2028, 2, 29));
        assert_eq!((fy.start, fy.end), (d(2028, 1, 1), d(2028, 12, 31)));
        assert_eq!(Uae.financial_year(d(2026, 12, 31)).label(), "2026");
    }
}

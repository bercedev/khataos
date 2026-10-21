//! Singapore compliance pack.
//!
//! Implements the GST items of `docs/research/singapore-compliance.md` (see
//! also `docs/singapore-gst.md`):
//! - GST registration numbers: a UEN in one of its three shapes, or an
//!   IRAS-issued `M2-1234567-K` number; the format only, because no check
//!   letter algorithm is published ([`gst_no`]);
//! - no tax regions: GST is one rate across Singapore, so
//!   [`CompliancePack::regions`] is empty;
//! - GST on a line: one "GST" component at the class's rate on the document
//!   date (the standard rate's history from 3% in 1994 to 9% from 1 Jan
//!   2024; zero-rated, exempt and out-of-scope classes), exports zero-rated
//!   ([`gst`]);
//! - tax invoice numbers: unique across all years, at most 35 characters of
//!   `[A-Za-z0-9/-]` (a KhataOS choice; IRAS only asks for an identifying
//!   number; see [`gst::MAX_INVOICE_NUMBER_LEN`]);
//! - January to December years by default;
//! - the GST F5 return from outward and inward documents ([`returns`]),
//!   with the reverse charge (Boxes 1, 5-7 and 14) and imports with GST
//!   paid at customs (Boxes 5 and 7) marked on tax classes
//!   ([`gst::Mechanism`]).
//!
//! Not modelled yet: OVR, MES, A3PL and IGDS imports, the
//! Gross Margin Scheme, bad debt relief, tourist refunds, input tax
//! disallowance and apportionment, and corporate income tax.
//!
//! InvoiceNow (Peppol PINT SG) is not in this crate: the document and its
//! rules are in `khata-en16931` (`pint`), and the desktop app's `peppol.rs`
//! builds it from the books and sends it through an InvoiceNow Access Point.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`gst::default_rates`] is a starting list.

pub mod calendar;
pub mod gst;
pub mod gst_no;
pub mod returns;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "SG", name: "Singapore", dialing_code: "+65" };

/// Singapore dollar, 100 cents. Printed "S$", the usual local sign; GST
/// figures must be shown in SGD even on foreign-currency invoices (research
/// §4 [PRIMARY]).
pub const CURRENCY: Currency = Currency { code: "SGD", symbol: "S$", minor_units: 2 };

/// Books default to calendar years. Singapore fixes no single year: a
/// company chooses its own financial year end (its first year may run up to
/// 18 months, research §6 [SECONDARY]), and GST periods are usually
/// calendar quarters whatever the FYE. A company can change its books'
/// first month.
pub const FY_START_MONTH: u32 = 1;

/// The Singapore pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct Singapore;

impl CompliancePack for Singapore {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// None: GST has no regions, and the app uses the country code as the
    /// region on both sides of a supply.
    fn regions(&self) -> Vec<Region> {
        Vec::new()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "GST Reg No"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        gst_no::validate(id)
    }

    /// Unique among every number already issued in the series, in any year.
    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        gst::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        gst::line_tax(line, rates)
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
        let pack: &dyn CompliancePack = &Singapore;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("SG", "Singapore", "+65"));
        assert_eq!((pack.currency().code, pack.currency().minor_units, pack.tax_id_label()), ("SGD", 2, "GST Reg No"));
        assert!(pack.regions().is_empty(), "GST has no regions");
        let fy = pack.financial_year(d(2027, 1, 15));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2027, 1, 1), d(2027, 12, 31), "2027".into()));
        let uen = pack.validate_tax_id("200912345k").unwrap();
        assert_eq!((uen.value.as_str(), uen.entity_id.as_deref(), uen.region), ("200912345K", Some("200912345K"), None));
        assert!(pack.validate_tax_id("2009123456").is_err());

        let issued = [IssuedDocument { number: "7", date: d(2026, 4, 2) }];
        assert_eq!(
            pack.validate_invoice_number("7", d(2027, 1, 1), &issued),
            Err(InvoiceNumberError::Duplicate { number: "7".into(), year: "2026".into() }),
            "no yearly reset"
        );

        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "STANDARD".into(),
            taxable_value: dec!(50000),
            supply: Supply {
                supplier_region: "SG".into(),
                place_of_supply: PlaceOfSupply::Region("SG".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &gst::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "GST", dec!(4500)));
    }

    #[test]
    fn calendar_years_by_default() {
        let fy = Singapore.financial_year(d(2028, 2, 29));
        assert_eq!((fy.start, fy.end), (d(2028, 1, 1), d(2028, 12, 31)));
        assert_eq!(Singapore.financial_year(d(2026, 12, 31)).label(), "2026");
    }
}

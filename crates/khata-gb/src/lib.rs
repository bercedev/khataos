//! United Kingdom compliance pack.
//!
//! Implements the VAT items of `docs/research/uk-compliance.md` (see also
//! `docs/uk-vat.md`):
//! - VAT registration numbers: 9 digits with HMRC's Mod 97 / Mod 97-55
//!   check digits, 12-digit branch numbers, GD/HA numbers, `GB` and `XI`
//!   prefixes ([`vrn`]);
//! - the four nations as address regions, Northern Ireland marked for
//!   goods trade with the EU ([`nations`]);
//! - VAT on a line: one "VAT" component at the class's rate (20% standard
//!   from 4 Jan 2011, 5% reduced, 0% zero-rated and exempt), exports
//!   zero-rated ([`vat`]);
//! - VAT invoice numbers under VAT Regulations 1995 reg 14(1)(a): unique
//!   across all years, at most 35 characters of `[A-Za-z0-9/-]` (a KhataOS
//!   choice; see [`vat::MAX_INVOICE_NUMBER_LEN`]);
//! - April to March years by default ([`FY_START_MONTH`]);
//! - the 9-box VAT return from outward and inward documents ([`returns`])
//!   and its Making Tax Digital API body ([`mtd`]), with the
//!   fraud-prevention headers ([`fraud`]); the desktop app submits it
//!   (`mtd_client`).
//!
//! - reverse charges (imported services, the domestic reverse charge of
//!   Notice 735 and construction), import VAT paid at the border and
//!   postponed import VAT, marked on tax classes ([`vat::Mechanism`]);
//! - the Flat Rate Scheme and the Cash Accounting Scheme ([`schemes`]).
//!
//! Not modelled yet: acquisition VAT (Box 2), the Annual Accounting
//! Scheme's interim payments, partial exemption, blocked input tax,
//! simplified and modified invoices, and Companies House / Corporation Tax
//! filings.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`vat::default_rates`] is a starting list.

pub mod calendar;
pub mod fraud;
pub mod mtd;
pub mod nations;
pub mod returns;
pub mod schemes;
pub mod vat;
pub mod vrn;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "GB", name: "United Kingdom", dialing_code: "+44" };

/// Pound sterling, 100 pence. VAT invoices must state the total VAT "in
/// sterling" (VAT Regulations 1995 reg 14(1)(l) [PRIMARY]).
pub const CURRENCY: Currency = Currency { code: "GBP", symbol: "£", minor_units: 2 };

/// Books default to April–March years. UK law fixes no single year for a
/// business: a company's accounting reference date is the last day of the
/// month of its incorporation anniversary and can be changed (Companies
/// House guidance), and VAT periods are usually quarters set by HMRC. April
/// is the start of the Corporation Tax "financial year" (1 April to 31
/// March, HMRC CTM01405) and a common company year end, so it is the
/// default; a company changes its books' first month to match its own
/// year. The individual tax year (6 April to 5 April) does not start on
/// the 1st and is not the default. See research §2.
pub const FY_START_MONTH: u32 = 4;

/// The UK pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnitedKingdom;

impl CompliancePack for UnitedKingdom {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// England, Scotland, Wales and Northern Ireland.
    fn regions(&self) -> Vec<Region> {
        nations::NATIONS.iter().map(|n| Region { code: n.code, name: n.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "VAT Reg No"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        vrn::validate(id)
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
        let pack: &dyn CompliancePack = &UnitedKingdom;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("GB", "United Kingdom", "+44"));
        assert_eq!((pack.currency().code, pack.currency().symbol, pack.currency().minor_units), ("GBP", "£", 2));
        assert_eq!(pack.tax_id_label(), "VAT Reg No");
        let fy = pack.financial_year(d(2027, 1, 15));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 4, 1), d(2027, 3, 31), "2026-27".into()));
        let vrn = pack.validate_tax_id("GB 562 2359 45").unwrap();
        assert_eq!((vrn.value.as_str(), vrn.entity_id.as_deref(), vrn.region), ("GB562235945", Some("562235945"), None));
        assert!(pack.validate_tax_id("GB 562 2359 46").is_err());

        let issued = [IssuedDocument { number: "7", date: d(2026, 4, 2) }];
        assert_eq!(
            pack.validate_invoice_number("7", d(2027, 4, 1), &issued),
            Err(InvoiceNumberError::Duplicate { number: "7".into(), year: "2026-27".into() }),
            "no yearly reset"
        );

        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "STANDARD".into(),
            taxable_value: dec!(1000),
            supply: Supply {
                supplier_region: "ENG".into(),
                place_of_supply: PlaceOfSupply::Region("SCT".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &vat::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "VAT", dec!(200)));
    }

    #[test]
    fn regions_are_the_four_nations() {
        let regions = UnitedKingdom.regions();
        let codes: Vec<_> = regions.iter().map(|r| r.code).collect();
        assert_eq!(codes, ["ENG", "SCT", "WLS", "NIR"]);
        assert!(regions.contains(&Region { code: "NIR", name: "Northern Ireland" }));
    }

    #[test]
    fn april_to_march_years_by_default() {
        let fy = UnitedKingdom.financial_year(d(2026, 3, 31));
        assert_eq!((fy.start, fy.end), (d(2025, 4, 1), d(2026, 3, 31)));
        assert_eq!(UnitedKingdom.financial_year(d(2026, 4, 1)).label(), "2026-27");
    }
}

//! United States compliance pack.
//!
//! Implements the items of `docs/research/us-compliance.md` (see also
//! `docs/us-sales-tax.md`):
//! - the 50 states and DC as regions, by USPS code ([`states`]);
//! - TINs: EIN `NN-NNNNNNN` (format only: no public check digit), and the
//!   SSN / ITIN a payee gives on Form W-9 ([`ein`]);
//! - sales tax on a line from **jurisdictions and rates the company
//!   defines** (state + county + city + district components), because no
//!   app can ship the rates of 13,000-odd local jurisdictions
//!   ([`sales_tax`]);
//! - invoice numbers unique in the series, any year (a KhataOS rule: no
//!   federal invoice-numbering law applies to sales tax);
//! - calendar years by default ([`FY_START_MONTH`]);
//! - a sales tax liability report by jurisdiction and state, with an
//!   economic nexus early warning for the states with a sourced threshold
//!   ([`returns`], [`nexus`]);
//! - the Form 1099 vendor list ([`form1099`]);
//! - use tax the business self-assesses on untaxed purchases from vendors
//!   outside its state ([`use_tax`]);
//! - customers' resale and exemption certificates, their effect on the
//!   sales tax report and an expiring-certificates report ([`exemptions`]).
//!
//! Not modelled: marketplace facilitator sales, origin-based sourcing rules, state-specific return
//! forms and e-filing, address-to-jurisdiction lookup (rooftop geocoding),
//! payroll taxes and income tax.

pub mod calendar;
pub mod ein;
pub mod exemptions;
pub mod form1099;
pub mod nexus;
pub mod returns;
pub mod sales_tax;
pub mod states;
pub mod use_tax;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "US", name: "United States", dialing_code: "+1" };

/// US dollar, 100 cents.
pub const CURRENCY: Currency = Currency { code: "USD", symbol: "$", minor_units: 2 };

/// Books default to calendar years. A business may adopt a calendar or a
/// fiscal tax year (IRS Publication 538); the company changes the books'
/// first month to match its own.
pub const FY_START_MONTH: u32 = 1;

/// Longest invoice number accepted (a KhataOS choice, as for the UK).
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain (a KhataOS choice).
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// The US pack. It holds no company data.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnitedStates;

impl CompliancePack for UnitedStates {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The 50 states and DC.
    fn regions(&self) -> Vec<Region> {
        states::STATES.iter().map(|s| Region { code: s.code, name: s.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "EIN"
    }

    /// EIN, SSN or ITIN format.
    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        ein::validate(id)
    }

    /// Not empty, at most 35 of letters, digits, `-` and `/`, and unique
    /// in the series in any year (case ignored).
    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        if number.is_empty() {
            return Err(InvoiceNumberError::Empty);
        }
        if let Some(found) = number.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '/')) {
            return Err(InvoiceNumberError::InvalidCharacter { number: number.to_owned(), found, allowed: ALLOWED_CHARACTERS });
        }
        if number.len() > MAX_INVOICE_NUMBER_LEN {
            return Err(InvoiceNumberError::TooLong { number: number.to_owned(), max: MAX_INVOICE_NUMBER_LEN, found: number.len() });
        }
        if let Some(clash) = issued.iter().find(|d| d.number.eq_ignore_ascii_case(number)) {
            return Err(InvoiceNumberError::Duplicate { number: number.to_owned(), year: self.financial_year(clash.date).label() });
        }
        Ok(())
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        sales_tax::line_tax(line, rates)
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

    #[test]
    fn works_as_a_trait_object() {
        let pack: &dyn CompliancePack = &UnitedStates;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("US", "United States", "+1"));
        assert_eq!((pack.currency().code, pack.currency().minor_units), ("USD", 2));
        assert_eq!(pack.tax_id_label(), "EIN");
        assert_eq!(pack.regions().len(), 51);
        let fy = pack.financial_year(d(2026, 10, 20));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 1, 1), d(2026, 12, 31), "2026".into()));
        assert_eq!(pack.validate_tax_id("123456789").unwrap().value, "12-3456789");

        let issued = [IssuedDocument { number: "INV-7", date: d(2025, 12, 1) }];
        assert_eq!(
            pack.validate_invoice_number("inv-7", d(2026, 1, 2), &issued),
            Err(InvoiceNumberError::Duplicate { number: "inv-7".into(), year: "2025".into() })
        );
        assert!(matches!(pack.validate_invoice_number("INV 8", d(2026, 1, 2), &[]), Err(InvoiceNumberError::InvalidCharacter { .. })));
        assert!(matches!(pack.validate_invoice_number(&"9".repeat(36), d(2026, 1, 2), &[]), Err(InvoiceNumberError::TooLong { .. })));
        assert_eq!(pack.validate_invoice_number("", d(2026, 1, 2), &[]), Err(InvoiceNumberError::Empty));

        let mut rates = RateSchedule::default();
        rates.set("CA", d(2017, 1, 1), dec!(7.25)).unwrap();
        rates.set("CA-LA/DISTRICT", d(2017, 1, 1), dec!(2.25)).unwrap();
        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "TAXABLE".into(),
            taxable_value: dec!(200),
            supply: Supply {
                supplier_region: "CA".into(),
                place_of_supply: PlaceOfSupply::Region("CA-LA".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &rates).unwrap();
        assert_eq!((t.components.len(), t.total()), (2, dec!(19)));
    }
}

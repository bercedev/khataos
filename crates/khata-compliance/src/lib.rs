//! The country compliance-pack interface.
//!
//! `khata-core` never matches on a country. Everything that differs by
//! jurisdiction (financial year, currency, tax-ID formats, document-number
//! rules, tax computation) sits behind [`CompliancePack`], one crate per
//! country (`khata-in` for India, later UAE and KSA). The core never depends
//! on this crate; apps pick the pack for a company's country and call it.
//!
//! The shape follows the shared contract in
//! `docs/research/mena-compliance.md` §8: pluggable tax-ID validators that
//! expose derived parts, a declared numbering rule, supply attributes
//! modelled once and mapped by each pack, and rates that come from
//! effective-dated data rather than code.

pub mod calendar;
pub mod rates;

use chrono::{Datelike, Months, NaiveDate};
use khata_core::Amount;
use rust_decimal::Decimal;
use serde::Serialize;
use thiserror::Error;

pub use calendar::{CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, ProfileField, Rule, Shift, StateFiling};
pub use rates::{RateChange, RateError, RateSchedule};

/// The country a pack covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Country {
    /// ISO 3166-1 alpha-2, as stored in the company profile ("IN").
    pub code: &'static str,
    pub name: &'static str,
    /// International dialling prefix ("+91").
    pub dialing_code: &'static str,
}

/// The currency the books are kept in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Currency {
    /// ISO 4217 ("INR").
    pub code: &'static str,
    pub symbol: &'static str,
    /// Decimal places of the minor unit (2 for paise; 3 for KWD, BHD, OMR).
    pub minor_units: u32,
}

/// A tax region inside a country: an Indian state or union territory, a UAE
/// emirate. Codes are the ones the tax authority uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Region {
    pub code: &'static str,
    pub name: &'static str,
}

/// A financial year, both ends inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct FinancialYear {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl FinancialYear {
    /// The twelve-month year that starts on the 1st of `start_month` and
    /// contains `date` (April for India, January for a calendar year).
    pub fn containing(date: NaiveDate, start_month: u32) -> Self {
        assert!((1..=12).contains(&start_month), "month {start_month} out of range");
        let year = if date.month() >= start_month { date.year() } else { date.year() - 1 };
        let start = NaiveDate::from_ymd_opt(year, start_month, 1).expect("the 1st of a month exists");
        let end = start
            .checked_add_months(Months::new(12))
            .and_then(|d| d.pred_opt())
            .expect("dates in range");
        Self { start, end }
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    /// "2026-27" for a year spanning two calendar years, "2026" otherwise.
    pub fn label(&self) -> String {
        if self.start.year() == self.end.year() {
            self.start.year().to_string()
        } else {
            format!("{}-{:02}", self.start.year(), self.end.year().rem_euclid(100))
        }
    }
}

/// A tax ID that passed validation, with the parts it encodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaxIdDetails {
    /// Normalised form to store (trimmed, upper case).
    pub value: String,
    /// The region the registration belongs to, when the ID encodes one.
    pub region: Option<Region>,
    /// The underlying entity identifier (PAN inside a GSTIN, TIN inside a
    /// UAE TRN), when the ID encodes one.
    pub entity_id: Option<String>,
}

/// Why a tax ID is invalid. Positions are 1-based, as a user counts them.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaxIdError {
    #[error("it is empty")]
    Empty,
    #[error("it must be {expected} characters long, not {found}")]
    Length { expected: usize, found: usize },
    #[error("character {position} must be {expected}")]
    Format { position: usize, expected: &'static str },
    #[error("{code:?} is not a known {kind}")]
    UnknownRegion { kind: &'static str, code: String },
    #[error("the check character should be {expected}, not {found}")]
    CheckCharacter { expected: char, found: char },
}

/// A document number already issued in the same series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssuedDocument<'a> {
    pub number: &'a str,
    pub date: NaiveDate,
}

/// Why an invoice (or credit/debit note) number breaks the country's rule.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvoiceNumberError {
    #[error("an invoice number is required")]
    Empty,
    #[error("invoice numbers may have at most {max} characters; {number:?} has {found}")]
    TooLong { number: String, max: usize, found: usize },
    #[error("invoice numbers may only use {allowed}; {number:?} contains {found:?}")]
    InvalidCharacter { number: String, found: char, allowed: &'static str },
    #[error("invoice number {number:?} is already used in financial year {year}")]
    Duplicate { number: String, year: String },
}

/// Where a supply is consumed, for place-of-supply rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PlaceOfSupply {
    /// A region of the supplier's country (a state code for India).
    Region(String),
    /// Outside the country: an export.
    Abroad,
}

/// The supply attributes tax depends on, modelled once for every country;
/// each pack maps them to its own rules (MENA research §8, point 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Supply {
    /// The supplier's registered region (the state of the company's GSTIN).
    pub supplier_region: String,
    pub place_of_supply: PlaceOfSupply,
    /// The recipient is in a special economic or free zone (an Indian SEZ
    /// unit or developer, a UAE designated zone).
    pub special_zone: bool,
    /// A zero-rated supply made without paying tax (India: an export or SEZ
    /// supply under LUT or bond). Only valid for exports and zone supplies.
    pub without_payment: bool,
}

/// One invoice line to tax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaxLine {
    /// The document date; it picks the rate in force.
    pub date: NaiveDate,
    /// What the rate schedule is keyed by: an HSN/SAC code, or a category.
    pub tax_class: String,
    /// Value after discounts, before tax. Negative on credit notes.
    pub taxable_value: Amount,
    pub supply: Supply,
}

/// One tax on a line (CGST, SGST, UTGST, IGST, VAT…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaxComponent {
    pub tax: &'static str,
    /// Percentage, e.g. 9 for 9%.
    pub percent: Decimal,
    pub amount: Amount,
}

/// The taxes on one line. Empty for a zero-rated supply without payment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LineTax {
    pub components: Vec<TaxComponent>,
}

impl LineTax {
    pub fn total(&self) -> Amount {
        self.components.iter().map(|c| c.amount).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaxError {
    #[error("no tax rate for {class:?} on {date}")]
    NoRate { class: String, date: NaiveDate },
    #[error("{code:?} is not a known {kind}")]
    UnknownRegion { kind: &'static str, code: String },
    #[error("only exports and special-zone supplies can be made without paying tax")]
    NotZeroRated,
    #[error("amounts are too large")]
    Overflow,
}

/// A country's compliance rules. Implementations hold no company data:
/// rates and issued numbers are passed in, so one pack serves every company.
pub trait CompliancePack: Send + Sync {
    fn country(&self) -> Country;

    fn currency(&self) -> Currency;

    /// Tax regions (states, emirates) in the authority's order.
    fn regions(&self) -> Vec<Region>;

    /// The financial year that contains `date`.
    fn financial_year(&self, date: NaiveDate) -> FinancialYear;

    /// What the country calls a party's tax registration ("GSTIN", "TRN").
    fn tax_id_label(&self) -> &'static str;

    /// Check a party tax ID offline and return its normalised form and the
    /// parts it encodes.
    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError>;

    /// Check a new invoice number against the legal rule and the numbers
    /// already issued in its series. Callers may pass only the issued
    /// documents whose number could clash: those equal to `number` when
    /// ASCII case is ignored and every character other than an ASCII letter
    /// or digit counts as the same (the desktop app does). Use `issued`
    /// only to look for a clash.
    fn validate_invoice_number(
        &self,
        number: &str,
        date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError>;

    /// Tax on one line, with the rate in force on the line's date.
    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError>;

    /// The usual filing pattern of the main tax return, for the compliance
    /// calendar's settings. `None`: the pack has no calendar yet.
    fn calendar_defaults(&self) -> Option<CalendarDefaults> {
        None
    }

    /// Statutory due dates falling in `from..=to` for a company, estimated
    /// from the published rules ([`calendar`]). Empty when the pack has no
    /// calendar yet.
    fn due_dates(&self, _company: &CalendarCompany, _from: NaiveDate, _to: NaiveDate) -> Vec<DueDate> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn april_to_march_years() {
        let fy = FinancialYear::containing(d(2026, 3, 31), 4);
        assert_eq!((fy.start, fy.end), (d(2025, 4, 1), d(2026, 3, 31)));
        assert_eq!(fy.label(), "2025-26");
        let next = FinancialYear::containing(d(2026, 4, 1), 4);
        assert_eq!((next.start, next.end, next.label()), (d(2026, 4, 1), d(2027, 3, 31), "2026-27".into()));
        assert!(next.contains(d(2027, 3, 31)) && !next.contains(d(2026, 3, 31)));
        assert_eq!(FinancialYear::containing(d(2099, 12, 31), 4).label(), "2099-00");
    }

    #[test]
    fn calendar_years_and_leap_days() {
        let fy = FinancialYear::containing(d(2028, 2, 29), 1);
        assert_eq!((fy.start, fy.end, fy.label()), (d(2028, 1, 1), d(2028, 12, 31), "2028".into()));
        let july = FinancialYear::containing(d(2027, 6, 30), 7);
        assert_eq!((july.start, july.end), (d(2026, 7, 1), d(2027, 6, 30)));
    }

    #[test]
    fn line_tax_totals_its_components() {
        let t = LineTax {
            components: vec![
                TaxComponent { tax: "CGST", percent: Decimal::new(9, 0), amount: Decimal::new(9000, 2) },
                TaxComponent { tax: "SGST", percent: Decimal::new(9, 0), amount: Decimal::new(9000, 2) },
            ],
        };
        assert_eq!(t.total(), Decimal::new(18000, 2));
        assert_eq!(LineTax::default().total(), Amount::ZERO);
    }
}

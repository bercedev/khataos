//! Netherlands compliance pack.
//!
//! Implements the btw items of `docs/research/netherlands-compliance.md`
//! (see also `docs/netherlands-vat.md`):
//! - tax identifiers: the btw-identificatienummer ("NL" + nine digits +
//!   "B" + two digits) with the MOD 97-10 check of the 2020 btw-id or the
//!   elfproef of the omzetbelastingnummer ([`btw_id`]); a VAT identifier of
//!   another EU member state is accepted on its shape;
//! - the 12 provinces as address regions (ISO 3166-2:NL, [`provincies`]);
//! - btw on a line: one "BTW" component at the class's rate (21% / 9%,
//!   19% / 6% before 1 Oct 2012 / 1 Jan 2019), 0% for ZERO, EXEMPT and the
//!   "btw verlegd" REVERSE class, supplies abroad 0% ([`btw`]);
//! - invoice numbers under art. 35a Wet OB: given once, across all years,
//!   at most 35 characters of `[A-Za-z0-9/-]` (a KhataOS choice);
//! - calendar years by default ([`FY_START_MONTH`]);
//! - the aangifte omzetbelasting rubrieken 1a to 5g from outward and
//!   inward documents ([`returns`]).
//!
//! Not modelled yet: filing (Mijn Belastingdienst Zakelijk or SBR /
//! Digipoort), the ICP-opgaaf, the KOR, privégebruik (1d), the OSS (3c),
//! margin schemes and partial deduction. E-invoices (Peppol BIS / NLCIUS)
//! come from the shared `khata-en16931` crate.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`btw::default_rates`] is a starting list.

pub mod calendar;
pub mod btw;
pub mod btw_id;
pub mod provincies;
pub mod returns;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "NL", name: "Netherlands", dialing_code: "+31" };

/// Euro, 100 cents.
pub const CURRENCY: Currency = Currency { code: "EUR", symbol: "€", minor_units: 2 };

/// Books default to calendar years: the boekjaar is usually the calendar
/// year, though a company's articles may fix another 12-month year
/// (research §8). A company with a different year changes its books' first
/// month.
pub const FY_START_MONTH: u32 = 1;

/// The longest VAT identifier body after the two-letter prefix in the EU
/// (12 characters [SECONDARY: VIES]).
const EU_VAT_MAX: usize = 12;

/// A VAT identifier of another EU member state, on its shape only: an EU
/// prefix (EL for Greece) and 2 to 12 letters or digits.
fn foreign_eu_vat(value: &str) -> Option<TaxIdDetails> {
    let (prefix, body) = value.split_at(value.len().min(2));
    let eu = prefix != "NL" && (prefix == "EL" || (provincies::in_eu(prefix) && prefix != "GR"));
    let shape = (2..=EU_VAT_MAX).contains(&body.len()) && body.bytes().all(|b| b.is_ascii_alphanumeric());
    (eu && shape).then(|| TaxIdDetails { value: value.to_owned(), region: None, entity_id: None })
}

/// Validate a Dutch btw-id or omzetbelastingnummer, or another EU state's
/// VAT identifier (shape only).
pub fn validate_tax_id(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = btw_id::normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if btw_id::looks_like(&value) {
        return btw_id::validate(&value);
    }
    if let Some(d) = foreign_eu_vat(&value) {
        return Ok(d);
    }
    btw_id::validate(&value)
}

/// The Netherlands pack. It holds no company data, so one value serves
/// every company.
#[derive(Debug, Clone, Copy, Default)]
pub struct Netherlands;

impl CompliancePack for Netherlands {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The 12 provinces.
    fn regions(&self) -> Vec<Region> {
        provincies::PROVINCIES.iter().map(|p| Region { code: p.code, name: p.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "Btw-id"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        validate_tax_id(id)
    }

    /// Given once among every number already issued in the series, in any year.
    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        btw::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        btw::line_tax(line, rates)
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
        let pack: &dyn CompliancePack = &Netherlands;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("NL", "Netherlands", "+31"));
        assert_eq!((pack.currency().code, pack.currency().symbol, pack.currency().minor_units), ("EUR", "€", 2));
        assert_eq!(pack.tax_id_label(), "Btw-id");
        let fy = pack.financial_year(d(2026, 10, 21));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 1, 1), d(2026, 12, 31), "2026".into()));
        let id = pack.validate_tax_id("NL 0024.55.799.B11").unwrap();
        assert_eq!((id.value.as_str(), id.entity_id.as_deref()), ("NL002455799B11", Some("002455799")));
        assert!(pack.validate_tax_id("NL002455799B12").is_err());
        assert_eq!(pack.regions().len(), 12);

        let issued = [IssuedDocument { number: "7", date: d(2025, 12, 1) }];
        assert_eq!(
            pack.validate_invoice_number("7", d(2026, 1, 2), &issued),
            Err(InvoiceNumberError::Duplicate { number: "7".into(), year: "2025".into() })
        );
        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "STANDARD".into(),
            taxable_value: dec!(1000),
            supply: Supply {
                supplier_region: "NH".into(),
                place_of_supply: PlaceOfSupply::Region("ZH".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &btw::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "BTW", dec!(210)));
    }

    #[test]
    fn tax_ids_of_every_kind() {
        assert_eq!(validate_tax_id("004495445B01").unwrap().value, "NL004495445B01");
        assert_eq!(validate_tax_id("DE 136 695 976").unwrap().value, "DE136695976");
        assert_eq!(validate_tax_id("FR40303265045").unwrap().value, "FR40303265045");
        assert!(validate_tax_id("GR094259216").is_err(), "Greece uses EL");
        assert!(validate_tax_id("GB562235945").is_err(), "the UK is not in the EU");
        assert!(validate_tax_id("12345").is_err());
        assert_eq!(validate_tax_id(""), Err(TaxIdError::Empty));
    }
}

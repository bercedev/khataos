//! France compliance pack.
//!
//! Implements the TVA items of `docs/research/france-compliance.md` (see
//! also `docs/france-vat.md`):
//! - tax identifiers: the numéro de TVA intracommunautaire ("FR", a key,
//!   the SIREN) with the key formula and the SIREN's Luhn check, the SIREN
//!   and the SIRET on their own ([`siren`]); a VAT identifier of another EU
//!   member state is accepted on its shape;
//! - the 18 régions as address regions (ISO 3166-2:FR), with the overseas
//!   régions' TVA territories ([`regions`]);
//! - TVA on a line: one "TVA" component at the class's rate (20% / 10% /
//!   5.5% / 2.1%, 19.6% and 7% before 2014; 8.5% / 2.1% in Guadeloupe,
//!   Martinique and La Réunion; none in Guyane and Mayotte), 0% for ZERO,
//!   EXEMPT and the autoliquidation REVERSE class, supplies abroad and
//!   between metropolitan France and the overseas régions 0% ([`tva`]);
//! - invoice numbers "basé[s] sur une séquence chronologique et continue":
//!   given once, across all years, at most 35 characters of
//!   `[A-Za-z0-9/-]` (a KhataOS choice);
//! - calendar years by default ([`FY_START_MONTH`]);
//! - the CA3 (3310-CA3) lines from outward and inward documents
//!   ([`returns`]).
//!
//! E-invoices (the reform: receiving for everyone from 1 Sep 2026, issuing
//! from 1 Sep 2026 / 1 Sep 2027) are Factur-X, UBL or CII through a
//! plateforme agréée; the documents come from the shared `khata-en16931`
//! crate (its Factur-X writer). Not modelled yet: the PA connection and
//! e-reporting, EDI / EFI filing, the CA12, the franchise en base
//! thresholds, the DEB / DES, the FEC export and NF525.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`tva::default_rates`] is a starting list.

pub mod calendar;
pub mod regions;
pub mod returns;
pub mod siren;
pub mod tva;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "FR", name: "France", dialing_code: "+33" };

/// Euro, 100 cents.
pub const CURRENCY: Currency = Currency { code: "EUR", symbol: "€", minor_units: 2 };

/// Books default to calendar years. The exercice comptable is 12 months
/// with a closing date the business chooses (research §8); 31 December is
/// the most common, and the micro-entrepreneur's. A company with another
/// year changes its books' first month.
pub const FY_START_MONTH: u32 = 1;

/// The longest VAT identifier body after the two-letter prefix in the EU
/// (12 characters [SECONDARY: VIES]).
const EU_VAT_MAX: usize = 12;

/// A VAT identifier of another EU member state, on its shape only: an EU
/// prefix (EL for Greece) and 2 to 12 letters or digits.
fn foreign_eu_vat(value: &str) -> Option<TaxIdDetails> {
    let (prefix, body) = value.split_at(value.len().min(2));
    let eu = prefix != "FR" && (prefix == "EL" || (regions::in_eu(prefix) && prefix != "GR"));
    let shape = (2..=EU_VAT_MAX).contains(&body.len()) && body.bytes().all(|b| b.is_ascii_alphanumeric());
    (eu && shape).then(|| TaxIdDetails { value: value.to_owned(), region: None, entity_id: None })
}

/// Validate a French TVA number, a SIREN (nine digits) or SIRET (14
/// digits) for a party without one, or another EU state's VAT identifier
/// (shape only).
pub fn validate_tax_id(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = siren::normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if siren::looks_like(&value) {
        return siren::validate(&value);
    }
    if value.bytes().all(|b| b.is_ascii_digit()) {
        return match value.len() {
            siren::SIRET_LEN => siren::validate_siret(&value),
            _ => siren::validate_siren(&value),
        };
    }
    foreign_eu_vat(&value).map_or_else(|| siren::validate(&value), Ok)
}

/// The France pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct France;

impl CompliancePack for France {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The 13 metropolitan and five overseas régions.
    fn regions(&self) -> Vec<Region> {
        regions::REGIONS.iter().map(|r| Region { code: r.code, name: r.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "N° TVA"
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
        tva::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        tva::line_tax(line, rates)
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
        let pack: &dyn CompliancePack = &France;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("FR", "France", "+33"));
        assert_eq!((pack.currency().code, pack.currency().symbol, pack.currency().minor_units), ("EUR", "€", 2));
        assert_eq!(pack.tax_id_label(), "N° TVA");
        let fy = pack.financial_year(d(2026, 10, 21));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 1, 1), d(2026, 12, 31), "2026".into()));
        let id = pack.validate_tax_id("FR 44 732 829 320").unwrap();
        assert_eq!((id.value.as_str(), id.entity_id.as_deref()), ("FR44732829320", Some("732829320")));
        assert!(pack.validate_tax_id("FR45732829320").is_err());
        assert_eq!(pack.regions().len(), 18);

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
                supplier_region: "IDF".into(),
                place_of_supply: PlaceOfSupply::Region("BRE".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &tva::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "TVA", dec!(200)));
    }

    #[test]
    fn tax_ids_of_every_kind() {
        assert_eq!(validate_tax_id("303 265 045").unwrap().value, "303265045", "a SIREN");
        assert_eq!(validate_tax_id("732 829 320 00074").unwrap().entity_id.as_deref(), Some("732829320"), "a SIRET");
        assert!(validate_tax_id("732829321").is_err());
        assert_eq!(validate_tax_id("FR32000000330").unwrap().value, "FR32000000330", "Monaco");
        assert_eq!(validate_tax_id("DE 136 695 976").unwrap().value, "DE136695976");
        assert_eq!(validate_tax_id("NL004495445B01").unwrap().value, "NL004495445B01");
        assert!(validate_tax_id("GB562235945").is_err(), "the UK is not in the EU");
        assert!(validate_tax_id("12345").is_err());
        assert_eq!(validate_tax_id(""), Err(TaxIdError::Empty));
    }
}

//! Spain compliance pack.
//!
//! Implements the IVA items of `docs/research/spain-compliance.md` (see
//! also `docs/spain-vat.md`):
//! - tax identifiers: the NIF in all its forms (DNI, NIE, K/L/M and
//!   entities) with its check character, the "ES" intra-EU VAT number, and
//!   another EU member state's VAT identifier on its shape ([`nif`]);
//! - the 17 autonomous communities and Ceuta and Melilla as address
//!   regions (ISO 3166-2:ES), with Canarias, Ceuta and Melilla outside the
//!   IVA territory ([`comunidades`]);
//! - IVA on a line: one "IVA" component at the class's rate (21% / 10% /
//!   4%, 18% / 8% before 1 Sep 2012), 0% for ZERO, EXEMPT and the
//!   inversión del sujeto pasivo REVERSE class, supplies abroad 0% ([`iva`]);
//! - invoice numbers under RD 1619/2012 art. 6: unique in their series
//!   across all years, at most 60 characters (VeriFactu's NumSerieFactura),
//!   with a correlative check for numbers typed by hand;
//! - calendar years by default ([`FY_START_MONTH`]);
//! - the modelo 303 casillas from outward and inward documents
//!   ([`returns`]);
//! - VeriFactu billing records: the registro de alta and de anulación,
//!   their SHA-256 huella chain and the QR code URL ([`verifactu`]);
//!   generated only, never sent.
//!
//! Not modelled yet: filing (the AEAT's sede electrónica), the SII,
//! VeriFactu submission and the "no VERI*FACTU" signed mode, TicketBAI and
//! Batuz (País Vasco), the recargo de equivalencia, the special schemes,
//! the prorrata, the modelo 349 and 390, and IGIC / IPSI.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`iva::default_rates`] is a starting list.

pub mod calendar;
pub mod comunidades;
pub mod iva;
pub mod nif;
pub mod returns;
pub mod verifactu;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "ES", name: "Spain", dialing_code: "+34" };

/// Euro, 100 cents.
pub const CURRENCY: Currency = Currency { code: "EUR", symbol: "€", minor_units: 2 };

/// Books default to calendar years: the período impositivo of the
/// Impuesto sobre Sociedades is the company's ejercicio económico, at most
/// 12 months, usually the calendar year (Ley 27/2014 art. 27, research
/// §9). A company with a different year changes its books' first month;
/// IVA periods are calendar quarters or months either way.
pub const FY_START_MONTH: u32 = 1;

/// The longest VAT identifier body after the two-letter prefix in the EU
/// (12 characters [SECONDARY: VIES]).
const EU_VAT_MAX: usize = 12;

/// A VAT identifier of another EU member state, on its shape only: an EU
/// prefix (EL for Greece) and 2 to 12 letters or digits.
fn foreign_eu_vat(value: &str) -> Option<TaxIdDetails> {
    let (prefix, body) = value.split_at(value.len().min(2));
    let eu = prefix != "ES" && (prefix == "EL" || (comunidades::in_eu(prefix) && prefix != "GR"));
    let shape = (2..=EU_VAT_MAX).contains(&body.len()) && body.bytes().all(|b| b.is_ascii_alphanumeric());
    (eu && shape).then(|| TaxIdDetails { value: value.to_owned(), region: None, entity_id: None })
}

/// Validate a Spanish NIF (with or without "ES"), or another EU state's
/// VAT identifier (shape only).
pub fn validate_tax_id(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = nif::normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    // No other EU state's identifier has nine characters.
    if value.len() == nif::LEN || value.starts_with(nif::PREFIX) {
        return nif::validate(&value);
    }
    foreign_eu_vat(&value).map_or_else(|| nif::validate(&value), Ok)
}

/// The Spain pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spain;

impl CompliancePack for Spain {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The autonomous communities, Ceuta and Melilla.
    fn regions(&self) -> Vec<Region> {
        comunidades::COMUNIDADES.iter().map(|c| Region { code: c.code, name: c.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "NIF"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        validate_tax_id(id)
    }

    /// Unique among every number already issued in the series, in any year.
    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        iva::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        iva::line_tax(line, rates)
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
        let pack: &dyn CompliancePack = &Spain;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("ES", "Spain", "+34"));
        assert_eq!((pack.currency().code, pack.currency().symbol, pack.currency().minor_units), ("EUR", "€", 2));
        assert_eq!(pack.tax_id_label(), "NIF");
        let fy = pack.financial_year(d(2026, 10, 21));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 1, 1), d(2026, 12, 31), "2026".into()));
        let id = pack.validate_tax_id("ES B-58378431").unwrap();
        assert_eq!((id.value.as_str(), id.entity_id.as_deref()), ("B58378431", Some("B58378431")));
        assert!(pack.validate_tax_id("B58378432").is_err());
        assert_eq!(pack.regions().len(), 19);

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
                supplier_region: "MD".into(),
                place_of_supply: PlaceOfSupply::Region("AN".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &iva::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "IVA", dec!(210)));
    }

    #[test]
    fn tax_ids_of_every_kind() {
        assert_eq!(validate_tax_id("54362315-K").unwrap().value, "54362315K");
        assert_eq!(validate_tax_id("ESX2482300W").unwrap().value, "X2482300W");
        assert_eq!(validate_tax_id("DE 136 695 976").unwrap().value, "DE136695976");
        assert_eq!(validate_tax_id("FR40303265045").unwrap().value, "FR40303265045");
        assert_eq!(validate_tax_id("NL004495445B01").unwrap().value, "NL004495445B01");
        assert!(validate_tax_id("GB562235945").is_err(), "the UK is not in the EU");
        assert!(validate_tax_id("ES54362315T").is_err());
        assert!(validate_tax_id("12345").is_err());
        assert_eq!(validate_tax_id(""), Err(TaxIdError::Empty));
    }
}

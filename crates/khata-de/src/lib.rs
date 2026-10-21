//! Germany compliance pack.
//!
//! Implements the USt items of `docs/research/germany-compliance.md` (see
//! also `docs/germany-vat.md`):
//! - tax identifiers: the USt-IdNr ("DE" + nine digits, ISO 7064 MOD 11,10
//!   check digit, [`ustid`]); the Steuernummer in the printed or 13-digit
//!   ELSTER form with the state's check digit, accepted instead
//!   ([`steuernummer`]); a VAT identifier of
//!   another EU member state is accepted on its shape;
//! - the 16 Bundesländer as address regions (ISO 3166-2:DE, [`laender`]);
//! - USt on a line: one "USt" component at the class's rate (19% / 7%,
//!   16% / 5% from 1 Jul to 31 Dec 2020), 0% for ZERO, EXEMPT and the §13b
//!   REVERSE class, supplies abroad 0% ([`ust`]);
//! - invoice numbers under §14 Abs. 4 Nr. 4 UStG: given once, across all
//!   years, at most 35 characters of `[A-Za-z0-9/-]` (a KhataOS choice);
//! - calendar years by default ([`FY_START_MONTH`]);
//! - the UStVA Kennzahlen from outward and inward documents ([`returns`]);
//! - XRechnung / EN 16931 invoices from the books ([`xrechnung`], with the
//!   syntax writers of `khata-en16931`), and Leitweg-ID checks
//!   ([`leitweg`]).
//!
//! Not modelled yet: ELSTER submission (ERiC), the Zusammenfassende
//! Meldung, the annual return, Kleinunternehmer limits (§19), Istversteuerung,
//! Dauerfristverlängerung, margin schemes and GoBD export (GDPdU / DSFinV).
//!
//! ZUGFeRD / Factur-X PDFs (profile EN 16931, the CII XML embedded in a
//! PDF/A-3) are not in this crate: `khata-en16931` (`facturx`) writes them
//! and the desktop app's `peppol.rs` (`facturx_pdf`) builds them from the
//! books. They follow EN 16931, not XRechnung, so a German public buyer
//! still needs an XRechnung.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`ust::default_rates`] is a starting list.

pub mod calendar;
pub mod laender;
pub mod leitweg;
pub mod returns;
pub mod steuernummer;
pub mod ust;
pub mod ustid;
pub mod xrechnung;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "DE", name: "Germany", dialing_code: "+49" };

/// Euro, 100 cents.
pub const CURRENCY: Currency = Currency { code: "EUR", symbol: "€", minor_units: 2 };

/// Books default to calendar years: the Wirtschaftsjahr is the calendar
/// year unless a business entered in the Handelsregister chooses another
/// (§4a Abs. 1 EStG; changing it needs the Finanzamt's consent). A company
/// with a different year changes its books' first month.
pub const FY_START_MONTH: u32 = 1;

/// The longest VAT identifier body after the two-letter prefix in the EU
/// (12 characters, Cyprus/France/Netherlands… [SECONDARY: VIES]).
const EU_VAT_MAX: usize = 12;

/// A VAT identifier of another EU member state, on its shape only: an EU
/// prefix (EL for Greece) and 2 to 12 letters or digits.
fn foreign_eu_vat(value: &str) -> Option<TaxIdDetails> {
    let (prefix, body) = value.split_at(value.len().min(2));
    let eu = prefix != "DE" && (prefix == "EL" || (laender::in_eu(prefix) && prefix != "GR"));
    let shape = (2..=EU_VAT_MAX).contains(&body.len()) && body.bytes().all(|b| b.is_ascii_alphanumeric());
    (eu && shape).then(|| TaxIdDetails { value: value.to_owned(), region: None, entity_id: None })
}

/// Validate a German USt-IdNr or Steuernummer, or another EU state's VAT
/// identifier (shape only).
pub fn validate_tax_id(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = ustid::normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if ustid::looks_like(&value) {
        return ustid::validate(&value);
    }
    if steuernummer::looks_like(&value) {
        return steuernummer::validate(id);
    }
    if let Some(d) = foreign_eu_vat(&value) {
        return Ok(d);
    }
    if value.bytes().all(|b| b.is_ascii_digit()) {
        return steuernummer::validate(id);
    }
    ustid::validate(&value)
}

/// The Germany pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct Germany;

impl CompliancePack for Germany {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The 16 Bundesländer.
    fn regions(&self) -> Vec<Region> {
        laender::LAENDER.iter().map(|l| Region { code: l.code, name: l.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "USt-IdNr"
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
        ust::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        ust::line_tax(line, rates)
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
        let pack: &dyn CompliancePack = &Germany;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("DE", "Germany", "+49"));
        assert_eq!((pack.currency().code, pack.currency().symbol, pack.currency().minor_units), ("EUR", "€", 2));
        assert_eq!(pack.tax_id_label(), "USt-IdNr");
        let fy = pack.financial_year(d(2026, 10, 20));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 1, 1), d(2026, 12, 31), "2026".into()));
        let id = pack.validate_tax_id("DE 136 695 976").unwrap();
        assert_eq!((id.value.as_str(), id.entity_id.as_deref()), ("DE136695976", Some("136695976")));
        assert!(pack.validate_tax_id("DE136695977").is_err());
        assert_eq!(pack.regions().len(), 16);

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
                supplier_region: "BY".into(),
                place_of_supply: PlaceOfSupply::Region("BE".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &ust::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "USt", dec!(190)));
    }

    #[test]
    fn tax_ids_of_every_kind() {
        assert_eq!(validate_tax_id("400/8150/8159").unwrap().value, "40081508159");
        assert_eq!(validate_tax_id("9198081508152").unwrap().region.map(|r| r.code), Some("BY"));
        assert!(validate_tax_id("9198081508153").is_err());
        assert_eq!(validate_tax_id("FR 40 303265045").unwrap().value, "FR40303265045");
        assert_eq!(validate_tax_id("ATU13585627").unwrap().value, "ATU13585627");
        assert_eq!(validate_tax_id("EL094259216").unwrap().value, "EL094259216");
        assert!(validate_tax_id("GR094259216").is_err(), "Greece uses EL");
        assert!(validate_tax_id("GB562235945").is_err(), "the UK is not in the EU");
        assert!(validate_tax_id("12345").is_err());
        assert_eq!(validate_tax_id(""), Err(TaxIdError::Empty));
    }
}

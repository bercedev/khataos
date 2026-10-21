//! Impuesto sobre el Valor Añadido (IVA) on an invoice line, IVA
//! treatments, rounding and invoice numbers.
//!
//! Sources: `docs/research/spain-compliance.md` §2 and §3 (Ley 37/1992
//! art. 90 and 91 for the rates, art. 84.Uno.2º for inversión del sujeto
//! pasivo, art. 20 exemptions; Real Decreto 1619/2012 art. 6 for invoice
//! numbers and series). Not modelled yet, and so never applied silently:
//! - the recargo de equivalencia (retail traders) and the special schemes
//!   (simplificado, agricultura, bienes usados, criterio de caja);
//! - the prorrata (partial deduction);
//! - Canarias, Ceuta and Melilla, outside the IVA territory: a company
//!   there gets an error, and a supply to them is 0% (an export, or not
//!   located in the IVA territory);
//! - the conditions of an intra-EU supply or export: a supply is 0% as soon
//!   as the app marks it [`PlaceOfSupply::Abroad`].

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::comunidades;

/// The one tax component on a Spanish line.
pub const IVA: &str = "IVA";

/// The tipo general, 21% since 1 Sep 2012 (art. 90; 18% from 1 Jul 2010).
/// Only [`default_rates`] uses these: [`line_tax`] always takes the rate
/// from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(21, 0, 0, false, 0);
/// The tipo reducido, 10% since 1 Sep 2012 (art. 91.Uno; 8% from 1 Jul 2010).
pub const REDUCED_PERCENT: Decimal = Decimal::from_parts(10, 0, 0, false, 0);
/// The tipo superreducido, 4% (art. 91.Dos).
pub const SUPER_REDUCED_PERCENT: Decimal = Decimal::from_parts(4, 0, 0, false, 0);
/// The tipo general from 1 Jul 2010 to 31 Aug 2012.
pub const STANDARD_2010_PERCENT: Decimal = Decimal::from_parts(18, 0, 0, false, 0);
/// The tipo reducido from 1 Jul 2010 to 31 Aug 2012.
pub const REDUCED_2010_PERCENT: Decimal = Decimal::from_parts(8, 0, 0, false, 0);

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

/// The first day of the default list (the 18% / 8% rates took effect);
/// books for supplies before then are out of scope.
pub fn list_start() -> NaiveDate {
    date(2010, 7, 1)
}

/// The day the 21% and 10% rates took effect.
pub fn rates_2012() -> NaiveDate {
    date(2012, 9, 1)
}

/// Suggested tax classes for a new company's rate list.
pub const STANDARD_CLASS: &str = "STANDARD";
/// The 10% rate (most food, hospitality, passenger transport, housing…).
/// Any class whose name starts with this is at the tipo reducido.
pub const REDUCED_CLASS: &str = "REDUCED";
/// The 4% rate (bread, milk, fruit, books, medicines…).
pub const SUPER_REDUCED_CLASS: &str = "SUPERREDUCED";
/// 0%: used only for the temporary 0% on basic foods (2023–2024).
pub const ZERO_CLASS: &str = "ZERO";
/// Exempt supplies (art. 20: medical care, education, financial services,
/// letting of housing…). Any class whose name starts with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";
/// Supplies where the buyer is the taxpayer ("inversión del sujeto
/// pasivo", art. 84.Uno.2º: construction works, scrap, gold, mobile
/// phones…). Any class whose name starts with this is reverse-charged.
pub const REVERSE_CLASS: &str = "REVERSE";

/// The rate list a new Spanish company can start from: 18% then 21%, 8%
/// then 10%, 4%; 0% for ZERO, EXEMPT and REVERSE.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    let start = list_start();
    r.set(STANDARD_CLASS, start, STANDARD_2010_PERCENT).expect("valid default rate");
    r.set(STANDARD_CLASS, rates_2012(), STANDARD_PERCENT).expect("valid default rate");
    r.set(REDUCED_CLASS, start, REDUCED_2010_PERCENT).expect("valid default rate");
    r.set(REDUCED_CLASS, rates_2012(), REDUCED_PERCENT).expect("valid default rate");
    r.set(SUPER_REDUCED_CLASS, start, SUPER_REDUCED_PERCENT).expect("valid default rate");
    for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
        r.set(class, start, Decimal::ZERO).expect("valid default rate");
    }
    r
}

/// How a supply is treated for IVA, which decides its modelo 303 casilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    /// Taxed at a positive rate: casillas 01–09 (4%, 10%, 21%).
    Taxed,
    /// Taxed at 0% (the 2023–2024 basic-food measure): casillas 150–152.
    Zero,
    /// Exempt (art. 20): not in the devengado casillas.
    Exempt,
    /// Inversión del sujeto pasivo inside Spain: the seller reports casilla
    /// 122; the buyer self-assesses in 12/13 and deducts in 28/29.
    ReverseCharge,
    /// To or from another EU member state: the seller reports casilla 59;
    /// the buyer self-assesses in 10/11 and deducts it.
    IntraEu,
    /// Goods out of the EU: casilla 60. On a purchase: services from
    /// outside the EU, self-assessed in 12/13.
    Export,
    /// Not located in the IVA territory: casilla 120 (59 when sold to an EU
    /// business).
    OutOfScope,
}

impl Treatment {
    /// The line carries IVA.
    pub fn taxed(self) -> bool {
        self == Treatment::Taxed
    }
}

fn squash(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, "REVERSE…" reverse-charged; any other
/// class at 0% is zero-rated, any positive rate taxed.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class = squash(class);
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if class.starts_with(REVERSE_CLASS) {
        Treatment::ReverseCharge
    } else if rate.is_zero() {
        Treatment::Zero
    } else {
        Treatment::Taxed
    }
}

fn known(code: &str) -> Result<&'static comunidades::Comunidad, TaxError> {
    comunidades::comunidad(code).ok_or_else(|| TaxError::UnknownRegion { kind: "autonomous community code", code: code.to_owned() })
}

/// IVA on a value at a rate, before rounding.
pub fn exact_iva(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round to the cent, half away from zero: commercial rounding, and what
/// EN 16931 asks for the VAT breakdown (research §3 on the rounding rule).
pub fn round_cents(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn iva(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: IVA, percent, amount: round_cents(exact_iva(value, percent)?) }] })
}

/// IVA on one line: one "IVA" component, rounded to the cent.
///
/// - Both communities must be known codes. A company in Canarias, Ceuta or
///   Melilla gets an error: IGIC and IPSI are not modelled.
/// - A supply to Canarias, Ceuta or Melilla is 0% (an export, or not
///   located in the IVA territory).
/// - Otherwise the rate is the class's rate on the line's date; the
///   community never changes it.
/// - A "REVERSE…" class is 0% whatever its rate on file; a supply abroad
///   ([`PlaceOfSupply::Abroad`]) is 0%.
/// - `special_zone` changes nothing; `without_payment` on a domestic supply
///   is an error.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    let from = known(&supply.supplier_region)?;
    if !from.territory.iva() {
        return Err(TaxError::UnknownRegion {
            kind: "community of the IVA territory (Canarias: IGIC, Ceuta and Melilla: IPSI, not modelled)",
            code: from.code.to_owned(),
        });
    }
    let to = match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return iva(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known(code)?,
    };
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    if !to.territory.iva() || squash(&line.tax_class).starts_with(REVERSE_CLASS) {
        return iva(Decimal::ZERO, line.taxable_value);
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    iva(rate, line.taxable_value)
}

/// Longest invoice number accepted: VeriFactu's NumSerieFactura holds at
/// most 60 characters, series included (Orden HAC/1177/2024 record
/// layout, research §6).
pub const MAX_INVOICE_NUMBER_LEN: usize = 60;

/// What an invoice number may contain. A KhataOS choice, not a legal
/// rule: letters, digits, "-" and "/". Spaces are refused.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Invoice numbers (RD 1619/2012 art. 6.1.a: "número y, en su caso,
/// serie. La numeración de las facturas dentro de cada serie será
/// correlativa"): not empty, at most [`MAX_INVOICE_NUMBER_LEN`] characters
/// of [`ALLOWED_CHARACTERS`], and unique among every number already issued
/// in the series, whatever its year. KhataOS numbers each voucher type in
/// its own gap-free series, so "correlativa" holds by construction; a
/// number typed by hand can be checked with [`correlative_gap`].
/// Uniqueness ignores case. `year_of` names the year of a clash.
pub fn validate_invoice_number(
    number: &str,
    issued: &[IssuedDocument<'_>],
    year_of: impl Fn(NaiveDate) -> FinancialYear,
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
        return Err(InvoiceNumberError::Duplicate { number: number.to_owned(), year: year_of(clash.date).label() });
    }
    Ok(())
}

/// A number split into its series (everything before the trailing digits)
/// and its sequence number.
pub fn split_series(number: &str) -> Option<(&str, u64)> {
    let digits = number.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let (series, n) = number.split_at(number.len() - digits);
    n.parse().ok().map(|n| (series, n))
}

/// Why a number does not follow its series correlatively: the series'
/// last number is not one less, or an earlier number is dated after it.
/// `None` when it follows (or starts a new series). A warning, not a
/// refusal: KhataOS's own numbering never breaks it.
pub fn correlative_gap(number: &str, date: NaiveDate, issued: &[IssuedDocument<'_>]) -> Option<String> {
    let (series, n) = split_series(number)?;
    let same: Vec<(u64, NaiveDate)> = issued
        .iter()
        .filter_map(|d| split_series(d.number).filter(|(s, _)| s.eq_ignore_ascii_case(series)).map(|(_, k)| (k, d.date)))
        .collect();
    let (last, last_date) = same.iter().max_by_key(|(k, _)| *k).copied()?;
    if n != last + 1 {
        return Some(format!("series {series:?} continues at {}, not {n}", last + 1));
    }
    (last_date > date).then(|| format!("number {} of series {series:?} is dated {last_date}, after {date}", last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use khata_compliance::Supply;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn supply(from: &str, to: Option<&str>) -> Supply {
        Supply {
            supplier_region: from.into(),
            place_of_supply: to.map_or(PlaceOfSupply::Abroad, |c| PlaceOfSupply::Region(c.into())),
            special_zone: false,
            without_payment: false,
        }
    }

    fn line(class: &str, value: Amount, on: NaiveDate, supply: Supply) -> TaxLine {
        TaxLine { date: on, tax_class: class.into(), taxable_value: value, supply }
    }

    fn split(t: &LineTax) -> Vec<(&'static str, Decimal, Amount)> {
        t.components.iter().map(|c| (c.tax, c.percent, c.amount)).collect()
    }

    #[test]
    fn default_rates_by_date() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2010, 6, 30)), None, "not seeded before July 2010");
        assert_eq!(r.rate(STANDARD_CLASS, d(2012, 8, 31)), Some(dec!(18)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2012, 9, 1)), Some(dec!(21)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2011, 1, 1)), Some(dec!(8)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2026, 1, 1)), Some(dec!(10)));
        assert_eq!(r.rate(SUPER_REDUCED_CLASS, d(2026, 1, 1)), Some(dec!(4)));
        assert_eq!(r.rate("SUPERREDUCED-PAN", d(2026, 1, 1)), Some(dec!(4)), "not taken for REDUCED");
        for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
            assert_eq!(r.rate(class, d(2026, 1, 1)), Some(dec!(0)), "{class}");
        }
    }

    #[test]
    fn lines_carry_one_iva_component() {
        let rates = default_rates();
        let t = line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("MD", Some("CT"))), &rates).unwrap();
        assert_eq!(split(&t), [(IVA, dec!(21), dec!(210))]);
        let t = line_tax(&line("reduced", dec!(1000), d(2026, 5, 1), supply("es-an", Some("PV"))), &rates).unwrap();
        assert_eq!(split(&t), [(IVA, dec!(10), dec!(100))]);
        let t = line_tax(&line("SUPERREDUCED", dec!(12.50), d(2026, 5, 1), supply("MD", Some("MD"))), &rates).unwrap();
        assert_eq!(split(&t), [(IVA, dec!(4), dec!(0.50))]);
        let t = line_tax(&line("STANDARD", dec!(1000), d(2012, 8, 31), supply("MD", Some("MD"))), &rates).unwrap();
        assert_eq!(split(&t), [(IVA, dec!(18), dec!(180))], "the rate follows the date");
    }

    #[test]
    fn outside_the_iva_territory_reverse_charge_and_abroad() {
        let rates = default_rates();
        for to in ["CN", "CE", "ML"] {
            assert_eq!(line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("MD", Some(to))), &rates).unwrap().total(), dec!(0), "{to}");
        }
        assert!(matches!(
            line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("CN", Some("CN"))), &rates),
            Err(TaxError::UnknownRegion { .. })
        ));
        for class in ["REVERSE", "reverse-obra", "EXEMPT", "ZERO"] {
            let t = line_tax(&line(class, dec!(1000), d(2026, 5, 1), supply("MD", Some("MD"))), &rates).unwrap();
            assert_eq!(split(&t), [(IVA, dec!(0), dec!(0))], "{class}");
        }
        assert_eq!(line_tax(&line("NO-SUCH", dec!(1), d(2026, 5, 1), supply("MD", None)), &rates).unwrap().total(), dec!(0));
        let mut s = supply("MD", Some("MD"));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), s), &rates), Err(TaxError::NotZeroRated));
        assert!(matches!(line_tax(&line("8471", dec!(1), d(2026, 1, 1), supply("MD", Some("MD"))), &rates), Err(TaxError::NoRate { .. })));
    }

    #[test]
    fn invoice_numbers_and_series() {
        let calendar = |date: NaiveDate| FinancialYear::containing(date, 1);
        assert_eq!(validate_invoice_number("2026-A/000123", &[], calendar), Ok(()));
        assert_eq!(validate_invoice_number(&"9".repeat(60), &[], calendar), Ok(()));
        assert!(matches!(validate_invoice_number(&"9".repeat(61), &[], calendar), Err(InvoiceNumberError::TooLong { max: 60, .. })));
        assert!(matches!(validate_invoice_number("A 1", &[], calendar), Err(InvoiceNumberError::InvalidCharacter { .. })));
        let issued = [IssuedDocument { number: "A-7", date: d(2026, 3, 1) }, IssuedDocument { number: "R-1", date: d(2026, 3, 9) }];
        assert!(matches!(validate_invoice_number("a-7", &issued, calendar), Err(InvoiceNumberError::Duplicate { .. })));
        assert_eq!(split_series("2026-A/000123"), Some(("2026-A/", 123)));
        assert_eq!(split_series("ABC"), None);
        assert_eq!(correlative_gap("A-8", d(2026, 3, 2), &issued), None);
        assert_eq!(correlative_gap("B-1", d(2026, 3, 2), &issued), None, "a new series");
        assert!(correlative_gap("A-9", d(2026, 3, 2), &issued).unwrap().contains("continues at 8"));
        assert!(correlative_gap("R-2", d(2026, 3, 2), &issued).unwrap().contains("after"));
    }
}

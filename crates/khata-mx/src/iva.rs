//! IVA (impuesto al valor agregado) on an invoice line, treatments, and
//! invoice numbers.
//!
//! Sources: `docs/research/mexico-compliance.md` §1 (Ley del IVA arts. 1,
//! 1-A, 2-A, 9, 15, 20, 29; the border-region stimulus decree). Not
//! modelled, and so never applied silently:
//! - IVA and ISR withholdings (retenciones) on the line: they depend on
//!   the customer, so they come from the customer's `RET:` class in
//!   [`crate::retenciones`], not from this line tax;
//! - IEPS (special excise) and its interaction with the IVA base;
//! - the conditions of the 8% border stimulus (registration in the
//!   padrón, place of delivery in the region, excluded goods): choosing
//!   the [`BORDER_CLASS`] is the user's statement that they apply;
//! - proportional crediting (acreditamiento proporcional) when a business
//!   makes both taxed and exempt supplies.

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::states;

/// The one tax component on a Mexican line.
pub const IVA: &str = "IVA";

/// The general rate (LIVA art. 1, 16% since 1 Jan 2010).
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(16, 0, 0, false, 0);

/// The border-region stimulus rate: a credit of 50% of the IVA, so 8%
/// effective (the SAT catalogue's TasaOCuota 0.080000, "IVA Crédito
/// aplicado del 50%" [HIGH]). Northern region decree DOF 31 Dec 2018,
/// southern DOF 30 Dec 2020; extended to 31 Dec 2025 (DOF 24 Dec 2024)
/// and to **31 Dec 2026** (DOF 31 Dec 2025) [MEDIUM]. No extension for
/// 2027 was found as of 2026-10-20, so [`default_rates`] returns BORDER to
/// 16% from 1 Jan 2027; if a new decree extends it, delete that change.
pub const BORDER_PERCENT: Decimal = Decimal::from_parts(8, 0, 0, false, 0);

/// Suggested tax classes.
pub const STANDARD_CLASS: &str = "STANDARD";
/// Supplies in the border region under the stimulus decree (8%).
pub const BORDER_CLASS: &str = "BORDER";
/// Tasa 0% (LIVA art. 2-A: most food, medicines, and others; exports
/// under art. 29).
pub const ZERO_CLASS: &str = "ZERO";
/// Exempt supplies (LIVA arts. 9, 15, 20): no IVA and no right to credit
/// the IVA paid on related costs. Any class starting with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";

/// The day 16% took effect (the LIVA reform of DOF 7 Dec 2009 raised the
/// general rate from 15%) [UNCERTAIN: not re-checked for this pack];
/// [`default_rates`] starts here.
pub fn standard_rate_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2010, 1, 1).expect("valid date")
}

/// The day the northern border decree took effect (1 Jan 2019); the
/// southern region followed on 1 Jan 2021. [`default_rates`] uses the
/// earlier date for the single BORDER class.
pub fn border_rate_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2019, 1, 1).expect("valid date")
}

/// The day after the last extension of the border decrees runs out
/// (31 Dec 2026).
pub fn border_rate_end() -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, 1).expect("valid date")
}

/// The rate list a new Mexican company can start from.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    r.set(STANDARD_CLASS, standard_rate_start(), STANDARD_PERCENT).expect("valid rate");
    r.set(BORDER_CLASS, border_rate_start(), BORDER_PERCENT).expect("valid rate");
    r.set(BORDER_CLASS, border_rate_end(), STANDARD_PERCENT).expect("valid rate");
    r.set(ZERO_CLASS, standard_rate_start(), Decimal::ZERO).expect("valid rate");
    r.set(EXEMPT_CLASS, standard_rate_start(), Decimal::ZERO).expect("valid rate");
    r
}

/// How a line is treated. On a CFDI, taxed and zero-rated lines carry a
/// `Traslado` with `TipoFactor="Tasa"`; exempt lines carry
/// `TipoFactor="Exento"` with no rate or amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    /// A positive rate (16% or 8%).
    Taxed,
    /// Tasa 0%.
    ZeroRated,
    Exempt,
}

/// The treatment from the class and the rate in force: a class named
/// "EXEMPT…" is exempt; any other class at 0% is zero-rated.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class: String = class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if rate.is_zero() {
        Treatment::ZeroRated
    } else {
        Treatment::Taxed
    }
}

fn known_entity(code: &str) -> Result<(), TaxError> {
    states::entity(code).map(|_| ()).ok_or_else(|| TaxError::UnknownRegion { kind: "Mexican state code", code: code.to_owned() })
}

/// Round to the centavo, half away from zero.
pub fn round_centavos(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn iva(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    let exact = value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)?;
    Ok(LineTax { components: vec![TaxComponent { tax: IVA, percent, amount: round_centavos(exact) }] })
}

/// IVA on one line: one "IVA" component at the class's rate on the line's
/// date. Exports ([`PlaceOfSupply::Abroad`]) are 0% (LIVA art. 29). Both
/// entities must be known codes; the entity never changes the rate.
/// `without_payment` on a domestic supply is an error; `special_zone`
/// (IMMEX, recinto fiscalizado) changes nothing.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    known_entity(&supply.supplier_region)?;
    match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return iva(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_entity(code)?,
    }
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    iva(rate, line.taxable_value)
}

/// Longest invoice number. Anexo 20 limits Folio to 40 characters and
/// Serie to 25 (`[^|]`) [HIGH: cfdv40.xsd]; KhataOS puts the voucher
/// number in Folio (letters and digits only).
pub const MAX_INVOICE_NUMBER_LEN: usize = 40;

/// What an invoice number may contain (a KhataOS choice within Anexo 20's
/// Folio pattern).
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Folio numbers: not empty, at most [`MAX_INVOICE_NUMBER_LEN`] of
/// [`ALLOWED_CHARACTERS`], and unique in the series in any year (the SAT's
/// UUID identifies a CFDI; the folio is the issuer's own control, which
/// KhataOS keeps unique).
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

#[cfg(test)]
mod tests {
    use super::*;
    use khata_compliance::Supply;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn line(class: &str, value: Amount, to: Option<&str>) -> TaxLine {
        TaxLine {
            date: d(2026, 5, 1),
            tax_class: class.into(),
            taxable_value: value,
            supply: Supply {
                supplier_region: "JAL".into(),
                place_of_supply: to.map_or(PlaceOfSupply::Abroad, |c| PlaceOfSupply::Region(c.into())),
                special_zone: false,
                without_payment: false,
            },
        }
    }

    fn split(t: &LineTax) -> Vec<(&'static str, Decimal, Amount)> {
        t.components.iter().map(|c| (c.tax, c.percent, c.amount)).collect()
    }

    #[test]
    fn default_rates_and_treatments() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2009, 12, 31)), None);
        assert_eq!(r.rate(STANDARD_CLASS, d(2026, 1, 1)), Some(dec!(16)));
        assert_eq!(r.rate(BORDER_CLASS, d(2026, 1, 1)), Some(dec!(8)));
        assert_eq!(r.rate(BORDER_CLASS, d(2018, 12, 31)), None);
        assert_eq!(r.rate(BORDER_CLASS, d(2026, 12, 31)), Some(dec!(8)));
        assert_eq!(r.rate(BORDER_CLASS, d(2027, 1, 1)), Some(dec!(16)), "the decree ends with 2026 unless extended");
        assert_eq!(treatment("EXEMPT", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::ZeroRated);
        assert_eq!(treatment("BORDER", dec!(8)), Treatment::Taxed);
        assert_eq!(treatment(" exempt rent", dec!(16)), Treatment::Exempt, "the name wins");
    }

    #[test]
    fn lines_carry_one_iva_component() {
        let r = default_rates();
        assert_eq!(split(&line_tax(&line("STANDARD", dec!(1000), Some("CMX")), &r).unwrap()), [(IVA, dec!(16), dec!(160))]);
        assert_eq!(split(&line_tax(&line("BORDER", dec!(1000), Some("BCN")), &r).unwrap()), [(IVA, dec!(8), dec!(80))]);
        assert_eq!(line_tax(&line("ZERO", dec!(1000), Some("JAL")), &r).unwrap().total(), dec!(0));
        assert_eq!(line_tax(&line("STANDARD", dec!(1000), None), &r).unwrap().total(), dec!(0), "exports are 0%");
        assert_eq!(line_tax(&line("STANDARD", dec!(0.03125), Some("JAL")), &r).unwrap().total(), dec!(0.01), "0.005 → 0.01");
        assert!(matches!(line_tax(&line("STANDARD", dec!(1), Some("DF")), &r), Err(TaxError::UnknownRegion { .. })));
        assert!(matches!(line_tax(&line("8471", dec!(1), Some("JAL")), &r), Err(TaxError::NoRate { .. })));
        let mut l = line("STANDARD", dec!(1), Some("JAL"));
        l.supply.without_payment = true;
        assert_eq!(line_tax(&l, &r), Err(TaxError::NotZeroRated));
    }

    #[test]
    fn folios_are_unique_and_short() {
        let cal = |date: NaiveDate| FinancialYear::containing(date, 1);
        assert_eq!(validate_invoice_number("A-1", &[], cal), Ok(()));
        assert!(matches!(validate_invoice_number(&"9".repeat(41), &[], cal), Err(InvoiceNumberError::TooLong { .. })));
        assert!(matches!(validate_invoice_number("A 1", &[], cal), Err(InvoiceNumberError::InvalidCharacter { .. })));
        let issued = [IssuedDocument { number: "A-1", date: d(2025, 12, 31) }];
        assert_eq!(
            validate_invoice_number("a-1", &issued, cal),
            Err(InvoiceNumberError::Duplicate { number: "a-1".into(), year: "2025".into() })
        );
    }
}

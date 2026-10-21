//! Taxe sur la valeur ajoutée (TVA) on an invoice line, TVA treatments,
//! rounding and invoice numbers.
//!
//! Sources: `docs/research/france-compliance.md` §2 and §3 (CGI art. 278
//! normal rate, art. 278 bis / 278-0 bis reduced rates, art. 279
//! intermediate rate, art. 281 quater ff. the 2.1% rate, art. 294 and 296
//! overseas, art. 283 autoliquidation, art. 242 nonies A annexe II invoice
//! mentions, art. 293 B franchise en base). Not modelled yet, and so never
//! applied silently:
//! - the franchise en base (art. 293 B): a company under it charges no
//!   TVA; give its lines the EXEMPT class (the invoice then says "TVA non
//!   applicable, art. 293 B du CGI") and file no CA3;
//! - Corse's own rates (art. 297): set them up as classes of their own
//!   (`CORSE-13`, …);
//! - the margin schemes (art. 297 A ff.), TVA sur les encaissements timing
//!   and partial deduction (coefficient de déduction);
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

use crate::regions::{self, Territory};

/// The one tax component on a French line.
pub const TVA: &str = "TVA";

/// The taux normal, 20% since 1 Jan 2014 (art. 278 CGI; 19.6% before).
/// Only [`default_rates`] uses these: [`line_tax`] always takes the rate
/// from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(20, 0, 0, false, 0);
/// The taux intermédiaire, 10% since 1 Jan 2014 (art. 279; 7% in 2012–2013).
pub const INTERMEDIATE_PERCENT: Decimal = Decimal::from_parts(10, 0, 0, false, 0);
/// The taux réduit, 5.5% (art. 278-0 bis, 278 bis).
pub const REDUCED_PERCENT: Decimal = Decimal::from_parts(55, 0, 0, false, 1);
/// The taux particulier, 2.1% (art. 281 quater ff.).
pub const SPECIAL_PERCENT: Decimal = Decimal::from_parts(21, 0, 0, false, 1);
/// The taux normal before 2014.
pub const STANDARD_2000_PERCENT: Decimal = Decimal::from_parts(196, 0, 0, false, 1);
/// The taux intermédiaire of 2012–2013.
pub const INTERMEDIATE_2012_PERCENT: Decimal = Decimal::from_parts(7, 0, 0, false, 0);
/// The DOM taux normal, 8.5% (art. 296 CGI: Guadeloupe, Martinique, La Réunion).
pub const DOM_STANDARD_PERCENT: Decimal = Decimal::from_parts(85, 0, 0, false, 1);
/// The DOM taux réduit, 2.1% (art. 296).
pub const DOM_REDUCED_PERCENT: Decimal = Decimal::from_parts(21, 0, 0, false, 1);

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

/// The first day of the default list: the taux intermédiaire was created
/// on 1 Jan 2012. Books for supplies before 2012 are out of scope.
pub fn list_start() -> NaiveDate {
    date(2012, 1, 1)
}

/// The day the 20% and 10% rates took effect.
pub fn rates_2014() -> NaiveDate {
    date(2014, 1, 1)
}

/// Suggested tax classes for a new company's rate list.
pub const STANDARD_CLASS: &str = "STANDARD";
/// The 10% rate (restaurants, passenger transport, renovation works…).
/// Any class whose name starts with this is at the taux intermédiaire.
pub const INTERMEDIATE_CLASS: &str = "INTERMEDIATE";
/// The 5.5% rate (most food, energy-saving works, books…).
pub const REDUCED_CLASS: &str = "REDUCED";
/// The 2.1% rate (reimbursable medicines, press, certain shows…).
pub const SPECIAL_CLASS: &str = "SPECIAL";
/// 0%: not a French rate; kept for supplies the app cannot place abroad
/// but that bear no TVA with a right to deduct.
pub const ZERO_CLASS: &str = "ZERO";
/// Exempt supplies (art. 261 ff.: medical care, education, financial
/// services, letting of bare land…) and a company under the franchise en
/// base. Any class whose name starts with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";
/// Supplies where the buyer pays the TVA ("autoliquidation", art. 283:
/// subcontracting in construction (283-2 nonies), gas and electricity,
/// scrap…). Any class whose name starts with this is reverse-charged.
pub const REVERSE_CLASS: &str = "REVERSE";
/// Prefix of the classes looked up first for a company in Guadeloupe,
/// Martinique or La Réunion: `DOM-STANDARD` is the 8.5% rate there.
pub const DOM_PREFIX: &str = "DOM-";

/// The rate list a new French company can start from: 19.6% then 20%, 7%
/// then 10%, 5.5% and 2.1%; the DOM rates 8.5% and 2.1% under `DOM-`
/// classes; 0% for ZERO, EXEMPT and REVERSE.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    let start = list_start();
    r.set(STANDARD_CLASS, start, STANDARD_2000_PERCENT).expect("valid default rate");
    r.set(STANDARD_CLASS, rates_2014(), STANDARD_PERCENT).expect("valid default rate");
    r.set(INTERMEDIATE_CLASS, start, INTERMEDIATE_2012_PERCENT).expect("valid default rate");
    r.set(INTERMEDIATE_CLASS, rates_2014(), INTERMEDIATE_PERCENT).expect("valid default rate");
    r.set(REDUCED_CLASS, start, REDUCED_PERCENT).expect("valid default rate");
    r.set(SPECIAL_CLASS, start, SPECIAL_PERCENT).expect("valid default rate");
    r.set(&format!("{DOM_PREFIX}{STANDARD_CLASS}"), start, DOM_STANDARD_PERCENT).expect("valid default rate");
    r.set(&format!("{DOM_PREFIX}{REDUCED_CLASS}"), start, DOM_REDUCED_PERCENT).expect("valid default rate");
    for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
        r.set(class, start, Decimal::ZERO).expect("valid default rate");
    }
    r
}

/// How a supply is treated for TVA, which decides its CA3 line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    /// Taxed at a positive rate: base in A1, tax in 08 / 09 / 9B / 10 / 11…
    Taxed,
    /// 0% without being exempt (not a French rate): E2.
    Zero,
    /// Exempt (art. 261 ff.): E2 on the seller's side.
    Exempt,
    /// Autoliquidation inside France: the seller reports E2, the buyer
    /// self-assesses (B4 or A2 and the rate line) and deducts it.
    ReverseCharge,
    /// To or from another EU member state: goods sold → F2; services sold
    /// to EU businesses → E2. On a purchase: goods → B2 (acquisitions),
    /// services → A3, self-assessed.
    IntraEu,
    /// Goods out of the EU or to the overseas régions: E1. On a purchase:
    /// services from outside the EU → B4, self-assessed.
    Export,
    /// Not taxable in France (place of supply abroad): E2 when sold to an
    /// EU business, else left out.
    OutOfScope,
}

impl Treatment {
    /// The line carries TVA.
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

fn known_region(code: &str) -> Result<&'static regions::Region, TaxError> {
    regions::region(code).ok_or_else(|| TaxError::UnknownRegion { kind: "région code", code: code.to_owned() })
}

/// TVA on a value at a rate, before rounding.
pub fn exact_tva(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round to the cent, half away from zero. The CGI prescribes no rounding
/// for invoice lines [UNCERTAIN]; commercial rounding is the general
/// practice and what EN 16931 asks for the VAT breakdown.
pub fn round_cents(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn tva(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: TVA, percent, amount: round_cents(exact_tva(value, percent)?) }] })
}

/// The rate of a class for a supply made in a région: in Guadeloupe,
/// Martinique and La Réunion the `DOM-` class first (`DOM-STANDARD` for
/// `STANDARD`); no fallback to the metropolitan rate, which would be wrong
/// there.
pub fn rate_for(class: &str, territory: Territory, on: NaiveDate, rates: &RateSchedule) -> Result<Decimal, TaxError> {
    let key = match territory {
        Territory::Dom if !squash(class).starts_with(DOM_PREFIX) => format!("{DOM_PREFIX}{}", class.trim()),
        _ => class.to_owned(),
    };
    rates.rate(&key, on).ok_or(TaxError::NoRate { class: key, date: on })
}

/// TVA on one line: one "TVA" component, rounded to the cent.
///
/// - Both régions must be known codes.
/// - A supply made in Guyane or Mayotte (the supplier there) bears no TVA.
/// - A supply between metropolitan France and an overseas région, or
///   between two overseas régions, is an export or import: 0%.
/// - Inside metropolitan France the rate is the class's rate on the line's
///   date; inside one DOM région it is the `DOM-` class's rate.
/// - A "REVERSE…" class is 0% whatever its rate on file (autoliquidation);
///   a supply abroad ([`PlaceOfSupply::Abroad`]) is 0%.
/// - `special_zone` changes nothing; `without_payment` on a domestic supply
///   is an error.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    let from = known_region(&supply.supplier_region)?;
    let to = match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return tva(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_region(code)?,
    };
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    if from.territory == Territory::NoTva {
        return tva(Decimal::ZERO, line.taxable_value);
    }
    let same_territory = match from.territory {
        Territory::Metropole => to.territory == Territory::Metropole,
        _ => from.code == to.code,
    };
    if !same_territory || squash(&line.tax_class).starts_with(REVERSE_CLASS) {
        return tva(Decimal::ZERO, line.taxable_value);
    }
    let rate = rate_for(&line.tax_class, from.territory, line.date, rates)?;
    tva(rate, line.taxable_value)
}

/// Longest invoice number accepted. The CGI (art. 242 nonies A, I-1° annexe
/// II) asks for "un numéro unique basé sur une séquence chronologique et
/// continue" and sets no length. 35 is a KhataOS choice, as for Germany and
/// the UK (UN/EDIFACT data element 1004), which keeps the number usable in
/// EN 16931 and Factur-X documents.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain. A KhataOS choice, not a legal
/// rule: letters, digits, "-" and "/". Spaces are refused.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Invoice and credit-note numbers: not empty, at most
/// [`MAX_INVOICE_NUMBER_LEN`] characters of [`ALLOWED_CHARACTERS`], and
/// unique among every number already issued in the series, whatever its
/// year. Several series are allowed when each is chronological and
/// continuous (BOFiP BOI-TVA-DECLA-30-20-20); KhataOS numbers each voucher
/// type in its own gap-free series, so "continue" holds by construction.
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
        assert_eq!(r.rate(STANDARD_CLASS, d(2011, 12, 31)), None, "not seeded before 2012");
        assert_eq!(r.rate(STANDARD_CLASS, d(2013, 12, 31)), Some(dec!(19.6)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2014, 1, 1)), Some(dec!(20)));
        assert_eq!(r.rate(INTERMEDIATE_CLASS, d(2013, 6, 1)), Some(dec!(7)));
        assert_eq!(r.rate(INTERMEDIATE_CLASS, d(2026, 6, 1)), Some(dec!(10)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2026, 6, 1)), Some(dec!(5.5)));
        assert_eq!(r.rate(SPECIAL_CLASS, d(2026, 6, 1)), Some(dec!(2.1)));
        assert_eq!(r.rate("DOM-STANDARD", d(2026, 6, 1)), Some(dec!(8.5)));
        assert_eq!(r.rate("DOM-REDUCED", d(2026, 6, 1)), Some(dec!(2.1)));
        for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
            assert_eq!(r.rate(class, d(2026, 1, 1)), Some(dec!(0)), "{class}");
        }
    }

    #[test]
    fn metropolitan_lines_carry_one_tva_component() {
        let rates = default_rates();
        let t = line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("IDF", Some("PAC"))), &rates).unwrap();
        assert_eq!(split(&t), [(TVA, dec!(20), dec!(200))]);
        let t = line_tax(&line("intermediate", dec!(1000), d(2026, 5, 1), supply("fr-ara", Some("20R"))), &rates).unwrap();
        assert_eq!(split(&t), [(TVA, dec!(10), dec!(100))], "Corse is metropolitan");
        let t = line_tax(&line("REDUCED", dec!(99.99), d(2026, 5, 1), supply("IDF", Some("IDF"))), &rates).unwrap();
        assert_eq!(split(&t), [(TVA, dec!(5.5), dec!(5.50))], "5.49945 → 5.50");
        let t = line_tax(&line("STANDARD", dec!(1000), d(2013, 5, 1), supply("IDF", Some("IDF"))), &rates).unwrap();
        assert_eq!(split(&t), [(TVA, dec!(19.6), dec!(196))], "the rate follows the date");
    }

    #[test]
    fn overseas_regions() {
        let rates = default_rates();
        let t = line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("974", Some("974"))), &rates).unwrap();
        assert_eq!(split(&t), [(TVA, dec!(8.5), dec!(85))], "La Réunion: the DOM rate");
        let t = line_tax(&line("DOM-REDUCED", dec!(1000), d(2026, 5, 1), supply("971", Some("971"))), &rates).unwrap();
        assert_eq!(split(&t), [(TVA, dec!(2.1), dec!(21))]);
        assert_eq!(
            line_tax(&line("INTERMEDIATE", dec!(1000), d(2026, 5, 1), supply("972", Some("972"))), &rates),
            Err(TaxError::NoRate { class: "DOM-INTERMEDIATE".into(), date: d(2026, 5, 1) }),
            "no silent fallback to the metropolitan rate"
        );
        for (from, to) in [("IDF", "974"), ("974", "IDF"), ("971", "972"), ("973", "973"), ("976", "IDF")] {
            let t = line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply(from, Some(to))), &rates).unwrap();
            assert_eq!(t.total(), dec!(0), "{from} → {to}");
        }
    }

    #[test]
    fn reverse_charge_exempt_abroad_and_errors() {
        let rates = default_rates();
        for class in ["REVERSE", "reverse-btp", "EXEMPT", "ZERO"] {
            let t = line_tax(&line(class, dec!(1000), d(2026, 5, 1), supply("IDF", Some("IDF"))), &rates).unwrap();
            assert_eq!(split(&t), [(TVA, dec!(0), dec!(0))], "{class}");
        }
        assert_eq!(line_tax(&line("NO-SUCH", dec!(1000), d(2026, 5, 1), supply("IDF", None)), &rates).unwrap().total(), dec!(0));
        let bad = TaxError::UnknownRegion { kind: "région code", code: "75".into() };
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), supply("75", Some("IDF"))), &rates), Err(bad));
        let mut s = supply("IDF", Some("IDF"));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), s), &rates), Err(TaxError::NotZeroRated));
        assert_eq!(line_tax(&line("STANDARD", Amount::MAX, d(2026, 1, 1), supply("IDF", Some("IDF"))), &rates), Err(TaxError::Overflow));
    }

    #[test]
    fn treatments_and_invoice_numbers() {
        assert_eq!(treatment("STANDARD", dec!(20)), Treatment::Taxed);
        assert_eq!(treatment(" exempt santé ", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("REVERSE", dec!(20)), Treatment::ReverseCharge);
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::Zero);
        let calendar = |date: NaiveDate| FinancialYear::containing(date, 1);
        assert_eq!(validate_invoice_number("FA-2026/0001", &[], calendar), Ok(()));
        assert!(matches!(validate_invoice_number("FA 1", &[], calendar), Err(InvoiceNumberError::InvalidCharacter { .. })));
        let issued = [IssuedDocument { number: "FA-1", date: d(2025, 12, 31) }];
        assert_eq!(
            validate_invoice_number("fa-1", &issued, calendar),
            Err(InvoiceNumberError::Duplicate { number: "fa-1".into(), year: "2025".into() })
        );
    }
}

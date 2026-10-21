//! Umsatzsteuer (USt) on an invoice line, USt treatments, rounding and
//! invoice numbers.
//!
//! Sources: `docs/research/germany-compliance.md` §2, §3 and §5 (UStG §12 rates,
//! §28 Abs. 1 and 2 for the second half of 2020, §13b reverse charge,
//! §14 Abs. 4 invoice contents, §14a notes, §19 Kleinunternehmer). Not
//! modelled yet, and so never applied silently:
//! - the Kleinunternehmer scheme (§19): a company under it charges no USt;
//!   give its lines the EXEMPT class (the invoice then carries the §19
//!   note, see [`crate::xrechnung`]) and keep it out of the UStVA;
//! - margin schemes (§25, §25a), Istversteuerung timing (§20) and partial
//!   input-tax deduction (§15 Abs. 4);
//! - the conditions of an intra-EU supply or export (Gelangensbestätigung,
//!   Ausfuhrnachweis, the buyer's valid USt-IdNr): a supply is tax-free as
//!   soon as the app marks it [`PlaceOfSupply::Abroad`].

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::laender;

/// The one tax component on a German line.
pub const UST: &str = "USt";

/// The standard rate, 19% (§12 Abs. 1 UStG) since 1 Jan 2007, except
/// 1 Jul to 31 Dec 2020 (16%, §28 Abs. 1 UStG). Only [`default_rates`]
/// uses these: [`line_tax`] always takes the rate from the company's
/// schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(19, 0, 0, false, 0);
/// The reduced rate, 7% (§12 Abs. 2 UStG), 5% from 1 Jul to 31 Dec 2020
/// (§28 Abs. 2 UStG).
pub const REDUCED_PERCENT: Decimal = Decimal::from_parts(7, 0, 0, false, 0);
/// The temporary standard rate of the second half of 2020.
pub const STANDARD_2020_PERCENT: Decimal = Decimal::from_parts(16, 0, 0, false, 0);
/// The temporary reduced rate of the second half of 2020.
pub const REDUCED_2020_PERCENT: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

/// The day the standard rate became 19%; the default list starts here
/// (books for supplies before 2007 are out of scope).
pub fn standard_rate_start() -> NaiveDate {
    date(2007, 1, 1)
}

/// The temporary rates of the Zweites Corona-Steuerhilfegesetz: from
/// 1 Jul 2020 to 31 Dec 2020 inclusive.
pub fn temporary_2020() -> (NaiveDate, NaiveDate) {
    (date(2020, 7, 1), date(2020, 12, 31))
}

/// Suggested tax classes for a new company's rate list.
pub const STANDARD_CLASS: &str = "STANDARD";
/// Reduced-rated supplies (§12 Abs. 2, Anlage 2: most food, books, local
/// transport…). Any class whose name starts with this is reduced.
pub const REDUCED_CLASS: &str = "REDUCED";
/// 0% supplies that are taxable (§12 Abs. 3: photovoltaic systems from
/// 1 Jan 2023).
pub const ZERO_CLASS: &str = "ZERO";
/// Tax-free supplies (§4 UStG: Nr. 8 to 29 financial, rental, medical…).
/// Any class whose name starts with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";
/// Supplies where the recipient owes the tax (§13b UStG): 0% on the
/// invoice, which must say "Steuerschuldnerschaft des
/// Leistungsempfängers" (§14a Abs. 5). Any class whose name starts with
/// this is reverse-charged.
pub const REVERSE_CLASS: &str = "REVERSE";

/// The rate list a new German company can start from: 19% and 7% from
/// 2007, with 16% and 5% for the second half of 2020; 0% for ZERO, EXEMPT
/// and REVERSE.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    let (from, to) = temporary_2020();
    let after = to.succ_opt().expect("valid date");
    for (class, normal, temporary) in
        [(STANDARD_CLASS, STANDARD_PERCENT, STANDARD_2020_PERCENT), (REDUCED_CLASS, REDUCED_PERCENT, REDUCED_2020_PERCENT)]
    {
        r.set(class, standard_rate_start(), normal).expect("valid default rate");
        r.set(class, from, temporary).expect("valid default rate");
        r.set(class, after, normal).expect("valid default rate");
    }
    for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
        r.set(class, standard_rate_start(), Decimal::ZERO).expect("valid default rate");
    }
    r
}

/// How a supply is treated for USt, which decides its UStVA line and its
/// EN 16931 VAT category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    /// 19% (or 16% in 2H 2020): Kz 81 (or 35/36).
    Standard,
    /// 7% (or 5% in 2H 2020): Kz 86 (or 35/36).
    Reduced,
    /// Taxable at 0% (§12 Abs. 3).
    Zero,
    /// Tax-free without input-tax deduction (§4 Nr. 8 ff.): Kz 48.
    Exempt,
    /// The recipient owes the tax (§13b): the supplier reports Kz 60; a
    /// recipient self-assesses (Kz 46/47 or 84/85, deducts Kz 67).
    ReverseCharge,
    /// Tax-free intra-EU supply of goods to a buyer with a USt-IdNr
    /// (§4 Nr. 1 Buchst. b, §6a): Kz 41. On a purchase: an intra-EU
    /// acquisition (Kz 89/93, deducted in Kz 61).
    IntraEu,
    /// Tax-free export of goods outside the EU (§4 Nr. 1 Buchst. a, §6):
    /// Kz 43.
    Export,
    /// Not taxable in Germany (place of supply abroad, §3a): Kz 21 for
    /// services to EU businesses (§18b), else Kz 45.
    OutOfScope,
}

impl Treatment {
    /// Standard or reduced: the line carries USt.
    pub fn taxed(self) -> bool {
        matches!(self, Treatment::Standard | Treatment::Reduced)
    }
}

fn squash(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, "REVERSE…" reverse-charged; any other
/// class at 0% is zero-rated, "REDUCED…" is reduced, and any other
/// positive rate is standard. Intra-EU supplies, exports and supplies
/// abroad are decided by where the party is (the app), not by the class.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class = squash(class);
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if class.starts_with(REVERSE_CLASS) {
        Treatment::ReverseCharge
    } else if rate.is_zero() {
        Treatment::Zero
    } else if class.starts_with(REDUCED_CLASS) {
        Treatment::Reduced
    } else {
        Treatment::Standard
    }
}

fn known_land(code: &str) -> Result<(), TaxError> {
    laender::land(code).map(|_| ()).ok_or_else(|| TaxError::UnknownRegion { kind: "Bundesland code", code: code.to_owned() })
}

/// USt on a value at a rate, before rounding.
pub fn exact_ust(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round to the cent, half away from zero (kaufmännische Rundung). The
/// UStG prescribes no rounding method for invoice lines [UNCERTAIN];
/// commercial rounding is the general practice, and it is what EN 16931
/// asks for the VAT breakdown (BR-CO-17).
pub fn round_cents(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn ust(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: UST, percent, amount: round_cents(exact_ust(value, percent)?) }] })
}

/// USt on one line: one "USt" component, rounded to the cent.
///
/// - Inside Germany, the rate is the class's rate on the line's date from
///   the company's schedule. Both Länder must be known codes; the Land
///   never changes the rate.
/// - A "REVERSE…" class is 0% whatever its rate on file: the recipient
///   owes the tax (§13b Abs. 5).
/// - A supply abroad ([`PlaceOfSupply::Abroad`]) is 0%: an intra-EU supply
///   or export (tax-free) or a supply whose place is abroad (not taxable).
///   The conditions are not checked.
/// - `special_zone` changes nothing (free ports are not modelled).
/// - `without_payment` on a domestic supply is an error.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    known_land(&supply.supplier_region)?;
    match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return ust(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_land(code)?,
    }
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    if squash(&line.tax_class).starts_with(REVERSE_CLASS) {
        return ust(Decimal::ZERO, line.taxable_value);
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    ust(rate, line.taxable_value)
}

/// Longest invoice number accepted. §14 Abs. 4 Nr. 4 UStG asks only for
/// "eine fortlaufende Nummer mit einer oder mehreren Zahlenreihen, die zur
/// Identifizierung der Rechnung vom Rechnungsaussteller einmalig vergeben
/// wird" and sets no length. 35 is a KhataOS choice, as for the UK and the
/// UAE (UN/EDIFACT data element 1004), which keeps the number usable in
/// EN 16931 documents and by trading partners.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain. A KhataOS choice, not a legal
/// rule: letters, digits, "-" and "/". Spaces are refused.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Invoice and credit-note numbers (§14 Abs. 4 Nr. 4): not empty, at most
/// [`MAX_INVOICE_NUMBER_LEN`] characters of [`ALLOWED_CHARACTERS`], and
/// unique among every number already issued in the series, whatever its
/// year: the law asks for a number given "einmalig" (once). Several series
/// are allowed ("einer oder mehreren Zahlenreihen"); KhataOS numbers each
/// voucher type in its own gap-free series, so "fortlaufend" holds by
/// construction. Uniqueness ignores case. `year_of` names the year of a
/// clash.
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
    fn default_rates_with_the_2020_reduction() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2006, 12, 31)), None, "16% before 2007: not seeded");
        assert_eq!(r.rate(STANDARD_CLASS, d(2007, 1, 1)), Some(dec!(19)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2020, 6, 30)), Some(dec!(19)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2020, 7, 1)), Some(dec!(16)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2020, 12, 31)), Some(dec!(16)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2021, 1, 1)), Some(dec!(19)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2020, 9, 1)), Some(dec!(5)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2026, 1, 1)), Some(dec!(7)));
        for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
            assert_eq!(r.rate(class, d(2026, 1, 1)), Some(dec!(0)), "{class}");
        }
    }

    #[test]
    fn standard_and_reduced_lines_carry_one_ust_component() {
        let rates = default_rates();
        let t = line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("BY", Some("NW"))), &rates).unwrap();
        assert_eq!(split(&t), [(UST, dec!(19), dec!(190))]);
        let t = line_tax(&line("reduced", dec!(1000), d(2026, 5, 1), supply("be", Some("DE-HH"))), &rates).unwrap();
        assert_eq!(split(&t), [(UST, dec!(7), dec!(70))]);
        let t = line_tax(&line("STANDARD", dec!(1000), d(2020, 11, 30), supply("BY", Some("BY"))), &rates).unwrap();
        assert_eq!(split(&t), [(UST, dec!(16), dec!(160))], "the rate follows the date");
    }

    #[test]
    fn reverse_charge_exempt_and_abroad_are_zero() {
        let mut rates = default_rates();
        rates.set("REVERSE-BAU", d(2007, 1, 1), dec!(19)).unwrap();
        for class in ["REVERSE", "reverse-bau", "EXEMPT", "ZERO"] {
            let t = line_tax(&line(class, dec!(1000), d(2026, 5, 1), supply("BY", Some("BY"))), &rates).unwrap();
            assert_eq!(split(&t), [(UST, dec!(0), dec!(0))], "{class}");
        }
        let t = line_tax(&line("NO-SUCH", dec!(1000), d(2026, 5, 1), supply("BY", None)), &rates).unwrap();
        assert_eq!(t.total(), dec!(0), "abroad needs no rate on file");
    }

    #[test]
    fn errors() {
        let bad = |code: &str| TaxError::UnknownRegion { kind: "Bundesland code", code: code.into() };
        let r = default_rates();
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), supply("XX", Some("BY"))), &r), Err(bad("XX")));
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), supply("BY", Some(""))), &r), Err(bad("")));
        let mut s = supply("BY", Some("BY"));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), s), &r), Err(TaxError::NotZeroRated));
        assert!(matches!(line_tax(&line("8471", dec!(1), d(2026, 1, 1), supply("BY", Some("BY"))), &r), Err(TaxError::NoRate { .. })));
        assert_eq!(line_tax(&line("STANDARD", Amount::MAX, d(2026, 1, 1), supply("BY", Some("BY"))), &r), Err(TaxError::Overflow));
    }

    #[test]
    fn rounding_to_the_cent() {
        let rates = default_rates();
        let ust_on = |v: Decimal| line_tax(&line("STANDARD", v, d(2026, 1, 1), supply("BY", Some("BY"))), &rates).unwrap().total();
        assert_eq!(ust_on(dec!(0.50)), dec!(0.10), "0.095 → 0.10");
        assert_eq!(ust_on(dec!(-0.50)), dec!(-0.10), "credit notes mirror");
        assert_eq!(ust_on(dec!(0.02)), dec!(0.00));
    }

    #[test]
    fn treatments_from_class_names_and_rates() {
        assert_eq!(treatment("STANDARD", dec!(19)), Treatment::Standard);
        assert_eq!(treatment("Reduced books", dec!(7)), Treatment::Reduced);
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::Zero);
        assert_eq!(treatment(" exempt rent ", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("REVERSE", dec!(19)), Treatment::ReverseCharge, "the name wins over a rate");
        assert_eq!(treatment("2716", dec!(7)), Treatment::Standard, "a 7% rate alone does not make it reduced");
        assert!(Treatment::Reduced.taxed() && !Treatment::ReverseCharge.taxed() && !Treatment::IntraEu.taxed());
    }

    fn calendar(date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, 1)
    }

    #[test]
    fn invoice_numbers() {
        let ok = |n: &str| validate_invoice_number(n, &[], calendar);
        assert_eq!(ok("RE-2026/0001"), Ok(()));
        assert_eq!(ok(&"9".repeat(35)), Ok(()));
        assert!(matches!(ok(&"9".repeat(36)), Err(InvoiceNumberError::TooLong { max: 35, found: 36, .. })));
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["RE 1", "RE_1", "RE#1", "RÄ-1", "RE.1"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
        let issued = [IssuedDocument { number: "RE-1", date: d(2025, 12, 31) }];
        assert_eq!(
            validate_invoice_number("re-1", &issued, calendar),
            Err(InvoiceNumberError::Duplicate { number: "re-1".into(), year: "2025".into() }),
            "once, across years"
        );
    }
}

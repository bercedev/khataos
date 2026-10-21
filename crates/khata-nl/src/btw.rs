//! Omzetbelasting (btw) on an invoice line, btw treatments, rounding and
//! invoice numbers.
//!
//! Sources: `docs/research/netherlands-compliance.md` §2 and §3 (Wet op de
//! omzetbelasting 1968 art. 9 and Tabel I for the rates, art. 12 lid 2 and
//! 3 and art. 24ba for reverse charge, art. 35a for invoice contents,
//! art. 25 for the KOR). Not modelled yet, and so never applied silently:
//! - the kleineondernemersregeling (KOR): a company under it charges no
//!   btw; give its lines the EXEMPT class and file no aangifte;
//! - the margin schemes (art. 28b ff.), privégebruik (rubriek 1d) and
//!   partial input-tax deduction (pro rata);
//! - the conditions of an intra-EU supply or export (the buyer's valid
//!   btw-id, transport evidence): a supply is 0% as soon as the app marks
//!   it [`PlaceOfSupply::Abroad`].

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::provincies;

/// The one tax component on a Dutch line.
pub const BTW: &str = "BTW";

/// The general rate ("algemeen tarief"), 21% since 1 Oct 2012 (19% from
/// 1 Jan 2001). Only [`default_rates`] uses these: [`line_tax`] always
/// takes the rate from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(21, 0, 0, false, 0);
/// The reduced rate ("verlaagd tarief", Tabel I), 9% since 1 Jan 2019 (6%
/// before).
pub const REDUCED_PERCENT: Decimal = Decimal::from_parts(9, 0, 0, false, 0);
/// The general rate before 1 Oct 2012.
pub const STANDARD_2001_PERCENT: Decimal = Decimal::from_parts(19, 0, 0, false, 0);
/// The reduced rate before 1 Jan 2019.
pub const REDUCED_1986_PERCENT: Decimal = Decimal::from_parts(6, 0, 0, false, 0);

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

/// The first day of the default list (the general rate became 19% then;
/// books for supplies before 2001 are out of scope).
pub fn list_start() -> NaiveDate {
    date(2001, 1, 1)
}

/// The general rate became 21%.
pub fn standard_21_start() -> NaiveDate {
    date(2012, 10, 1)
}

/// The reduced rate became 9%.
pub fn reduced_9_start() -> NaiveDate {
    date(2019, 1, 1)
}

/// Suggested tax classes for a new company's rate list.
pub const STANDARD_CLASS: &str = "STANDARD";
/// Reduced-rated supplies (Tabel I: food, medicines, books, …). Any class
/// whose name starts with this is reduced.
pub const REDUCED_CLASS: &str = "REDUCED";
/// Taxable at 0% (Tabel II: exports, intra-EU supplies, bunker supplies…)
/// when the app does not already know it from the party.
pub const ZERO_CLASS: &str = "ZERO";
/// Exempt supplies (art. 11: medical care, education, financial services,
/// rental of immovable property…), and a company under the KOR. Any class
/// whose name starts with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";
/// Supplies where the btw is shifted to the buyer ("btw verlegd", art. 12
/// lid 2 and 3, art. 24ba Uitvoeringsbeschikking: subcontracting in
/// construction, scrap metal, mobile phones and chips…). Any class whose
/// name starts with this is reverse-charged.
pub const REVERSE_CLASS: &str = "REVERSE";

/// The rate list a new Dutch company can start from: 19% then 21%, 6% then
/// 9%; 0% for ZERO, EXEMPT and REVERSE.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    r.set(STANDARD_CLASS, list_start(), STANDARD_2001_PERCENT).expect("valid default rate");
    r.set(STANDARD_CLASS, standard_21_start(), STANDARD_PERCENT).expect("valid default rate");
    r.set(REDUCED_CLASS, list_start(), REDUCED_1986_PERCENT).expect("valid default rate");
    r.set(REDUCED_CLASS, reduced_9_start(), REDUCED_PERCENT).expect("valid default rate");
    for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
        r.set(class, list_start(), Decimal::ZERO).expect("valid default rate");
    }
    r
}

/// How a supply is treated for btw, which decides its rubriek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    /// Taxed at a positive rate: rubriek 1a (21%), 1b (9%) or 1c (other).
    Taxed,
    /// Taxed at 0% in the Netherlands: rubriek 1e.
    Zero,
    /// Exempt (art. 11): not on the aangifte.
    Exempt,
    /// Btw verlegd within the Netherlands: the seller reports rubriek 1e,
    /// the buyer self-assesses in 2a (and deducts it in 5b).
    ReverseCharge,
    /// To or from another EU member state: the seller reports rubriek 3b
    /// (goods to a buyer with a btw-id, and services to EU businesses); the
    /// buyer self-assesses in 4b.
    IntraEu,
    /// Goods out of the EU (uitvoer): rubriek 3a. On a purchase: services
    /// (or goods under an art. 23 licence) from outside the EU, rubriek 4a.
    Export,
    /// Not taxable in the Netherlands and not on the aangifte (services to
    /// customers outside the EU).
    OutOfScope,
}

impl Treatment {
    /// The line carries btw.
    pub fn taxed(self) -> bool {
        self == Treatment::Taxed
    }
}

fn squash(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, "REVERSE…" reverse-charged; any other
/// class at 0% is zero-rated, and any positive rate is taxed. Intra-EU
/// supplies and exports are decided by where the party is (the app).
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

fn known_provincie(code: &str) -> Result<(), TaxError> {
    provincies::provincie(code).map(|_| ()).ok_or_else(|| TaxError::UnknownRegion { kind: "province code", code: code.to_owned() })
}

/// Btw on a value at a rate, before rounding.
pub fn exact_btw(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round to the cent, half away from zero. The Wet OB prescribes no
/// rounding for invoice lines [UNCERTAIN]; commercial rounding is the
/// general practice and what EN 16931 asks for the VAT breakdown.
pub fn round_cents(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn btw(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: BTW, percent, amount: round_cents(exact_btw(value, percent)?) }] })
}

/// Btw on one line: one "BTW" component, rounded to the cent.
///
/// - Inside the Netherlands, the rate is the class's rate on the line's
///   date from the company's schedule. Both provinces must be known codes;
///   the province never changes the rate.
/// - A "REVERSE…" class is 0% whatever its rate on file: the buyer owes
///   the btw ("btw verlegd").
/// - A supply abroad ([`PlaceOfSupply::Abroad`]) is 0%. The conditions are
///   not checked.
/// - `special_zone` changes nothing; `without_payment` on a domestic supply
///   is an error.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    known_provincie(&supply.supplier_region)?;
    match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return btw(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_provincie(code)?,
    }
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    if squash(&line.tax_class).starts_with(REVERSE_CLASS) {
        return btw(Decimal::ZERO, line.taxable_value);
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    btw(rate, line.taxable_value)
}

/// Longest invoice number accepted. Art. 35a lid 1 onderdeel b Wet OB asks
/// for "een opeenvolgend nummer, met een of meer reeksen, waardoor de
/// factuur eenduidig wordt geïdentificeerd" and sets no length. 35 is a
/// KhataOS choice, as for Germany and the UK (UN/EDIFACT data element
/// 1004), which keeps the number usable in EN 16931 documents.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain. A KhataOS choice, not a legal
/// rule: letters, digits, "-" and "/". Spaces are refused.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Invoice and credit-note numbers (art. 35a lid 1 onderdeel b): not empty,
/// at most [`MAX_INVOICE_NUMBER_LEN`] characters of [`ALLOWED_CHARACTERS`],
/// and unique among every number already issued in the series, whatever
/// its year: the number must identify the invoice "eenduidig". Several
/// series are allowed ("een of meer reeksen"); KhataOS numbers each voucher
/// type in its own gap-free series, so "opeenvolgend" holds by
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
    fn default_rates_by_date() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2000, 12, 31)), None, "not seeded before 2001");
        assert_eq!(r.rate(STANDARD_CLASS, d(2012, 9, 30)), Some(dec!(19)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2012, 10, 1)), Some(dec!(21)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2018, 12, 31)), Some(dec!(6)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2019, 1, 1)), Some(dec!(9)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2026, 7, 1)), Some(dec!(9)));
        for class in [ZERO_CLASS, EXEMPT_CLASS, REVERSE_CLASS] {
            assert_eq!(r.rate(class, d(2026, 1, 1)), Some(dec!(0)), "{class}");
        }
    }

    #[test]
    fn taxed_lines_carry_one_btw_component() {
        let rates = default_rates();
        let t = line_tax(&line("STANDARD", dec!(1000), d(2026, 5, 1), supply("NH", Some("ZH"))), &rates).unwrap();
        assert_eq!(split(&t), [(BTW, dec!(21), dec!(210))]);
        let t = line_tax(&line("reduced food", dec!(1000), d(2026, 5, 1), supply("nl-ut", Some("GE"))), &rates).unwrap();
        assert_eq!(split(&t), [(BTW, dec!(9), dec!(90))]);
        let t = line_tax(&line("REDUCED", dec!(1000), d(2018, 5, 1), supply("UT", Some("UT"))), &rates).unwrap();
        assert_eq!(split(&t), [(BTW, dec!(6), dec!(60))], "the rate follows the date");
    }

    #[test]
    fn reverse_charge_exempt_and_abroad_are_zero() {
        let mut rates = default_rates();
        rates.set("REVERSE-BOUW", d(2001, 1, 1), dec!(21)).unwrap();
        for class in ["REVERSE", "reverse-bouw", "EXEMPT", "ZERO"] {
            let t = line_tax(&line(class, dec!(1000), d(2026, 5, 1), supply("NH", Some("NH"))), &rates).unwrap();
            assert_eq!(split(&t), [(BTW, dec!(0), dec!(0))], "{class}");
        }
        let t = line_tax(&line("NO-SUCH", dec!(1000), d(2026, 5, 1), supply("NH", None)), &rates).unwrap();
        assert_eq!(t.total(), dec!(0), "abroad needs no rate on file");
    }

    #[test]
    fn errors_and_rounding() {
        let bad = |code: &str| TaxError::UnknownRegion { kind: "province code", code: code.into() };
        let r = default_rates();
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), supply("BQ", Some("NH"))), &r), Err(bad("BQ")));
        let mut s = supply("NH", Some("NH"));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1), d(2026, 1, 1), s), &r), Err(TaxError::NotZeroRated));
        assert!(matches!(line_tax(&line("8471", dec!(1), d(2026, 1, 1), supply("NH", Some("NH"))), &r), Err(TaxError::NoRate { .. })));
        assert_eq!(line_tax(&line("STANDARD", Amount::MAX, d(2026, 1, 1), supply("NH", Some("NH"))), &r), Err(TaxError::Overflow));
        let on = |v: Decimal| line_tax(&line("STANDARD", v, d(2026, 1, 1), supply("NH", Some("NH"))), &r).unwrap().total();
        assert_eq!(on(dec!(0.50)), dec!(0.11), "0.105 → 0.11");
        assert_eq!(on(dec!(-0.50)), dec!(-0.11), "credit notes mirror");
    }

    #[test]
    fn treatments_from_class_names_and_rates() {
        assert_eq!(treatment("STANDARD", dec!(21)), Treatment::Taxed);
        assert_eq!(treatment("Reduced", dec!(9)), Treatment::Taxed);
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::Zero);
        assert_eq!(treatment(" exempt zorg ", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("REVERSE", dec!(21)), Treatment::ReverseCharge, "the name wins over a rate");
        assert!(Treatment::Taxed.taxed() && !Treatment::Zero.taxed() && !Treatment::IntraEu.taxed());
    }

    #[test]
    fn invoice_numbers() {
        let calendar = |date: NaiveDate| FinancialYear::containing(date, 1);
        let ok = |n: &str| validate_invoice_number(n, &[], calendar);
        assert_eq!(ok("F-2026/0001"), Ok(()));
        assert!(matches!(ok(&"9".repeat(36)), Err(InvoiceNumberError::TooLong { max: 35, found: 36, .. })));
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["F 1", "F_1", "F#1", "F.1"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
        let issued = [IssuedDocument { number: "F-1", date: d(2025, 12, 31) }];
        assert_eq!(
            validate_invoice_number("f-1", &issued, calendar),
            Err(InvoiceNumberError::Duplicate { number: "f-1".into(), year: "2025".into() }),
            "once, across years"
        );
    }
}

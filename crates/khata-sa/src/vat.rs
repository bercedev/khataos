//! VAT on an invoice line, VAT treatments and categories, and tax invoice
//! numbers.
//!
//! Sources: `docs/research/mena-compliance.md` §5 (VAT Law and Implementing
//! Regulations) and §6.2 (ZATCA XML Implementation Standard v1.2: VAT
//! categories, VATEX codes and rounding). Not modelled yet, and so never
//! applied silently:
//! - the reverse charge on imported services, and VAT paid at customs;
//! - the profit margin scheme;
//! - private healthcare and education to citizens (VAT borne by the state),
//!   other than as a zero-rated class the user names with its VATEX code;
//! - the conditions for zero-rating an export (evidence, time limits): an
//!   export is taken as zero-rated as soon as it is marked
//!   [`PlaceOfSupply::Abroad`];
//! - restricted (blocked) input tax.

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::regions;

/// The one tax component on a KSA line.
pub const VAT: &str = "VAT";

/// The standard rate from 1 Jul 2020 (Royal Decree A/638, research §5
/// [SECONDARY]). Only [`default_rates`] uses it: [`line_tax`] always takes
/// the rate from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(15, 0, 0, false, 0);

/// The standard rate from 1 Jan 2018 to 30 Jun 2020 (research §5: "5%
/// history from 1 Jan 2018").
pub const FIRST_STANDARD_PERCENT: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

/// The day KSA VAT started.
pub fn vat_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2018, 1, 1).expect("valid date")
}

/// The day the standard rate went up to 15%.
pub fn fifteen_percent_from() -> NaiveDate {
    NaiveDate::from_ymd_opt(2020, 7, 1).expect("valid date")
}

/// Suggested tax classes for a new company's rate list, as in the UAE pack.
/// A class may carry a VATEX code after its name ("ZERO VATEX-SA-35",
/// "EXEMPT VATEX-SA-29"): the e-invoice then prints that exemption reason.
pub const STANDARD_CLASS: &str = "STANDARD";
/// Zero-rated supplies: exports, international transport, qualifying
/// medicines, metals, and the rest of the VATEX-SA-32 to -36 list.
pub const ZERO_RATED_CLASS: &str = "ZERO";
/// Exempt supplies: financial services, life insurance, residential real
/// estate (VATEX-SA-29, -29-7, -30). Any class whose name starts with this
/// is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";
/// Supplies outside the scope of VAT (ZATCA category "O"). Any class whose
/// name, without spaces, starts with this is out of scope.
pub const OUT_OF_SCOPE_CLASS: &str = "OUTOFSCOPE";

/// The rate list a new KSA company can start from: 5% standard from 1 Jan
/// 2018, 15% from 1 Jul 2020, and 0% zero-rated, exempt and out of scope.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    r.set(STANDARD_CLASS, vat_start(), FIRST_STANDARD_PERCENT).expect("valid default rate");
    r.set(STANDARD_CLASS, fifteen_percent_from(), STANDARD_PERCENT).expect("valid default rate");
    for class in [ZERO_RATED_CLASS, EXEMPT_CLASS, OUT_OF_SCOPE_CLASS] {
        r.set(class, vat_start(), Decimal::ZERO).expect("valid default rate");
    }
    r
}

/// How a supply is treated for VAT. They go to different lines of the VAT
/// return and carry different ZATCA category codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    Standard,
    ZeroRated,
    Exempt,
    OutOfScope,
}

impl Treatment {
    /// The ZATCA VAT category code (UNCL5305 subset, research §6.2): `S`,
    /// `Z`, `E` or `O`.
    pub fn category(self) -> &'static str {
        match self {
            Treatment::Standard => "S",
            Treatment::ZeroRated => "Z",
            Treatment::Exempt => "E",
            Treatment::OutOfScope => "O",
        }
    }
}

fn squeeze(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, "OUT OF SCOPE…" out of scope, any other
/// class at 0% zero-rated, and a positive rate standard.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class = squeeze(class);
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if class.starts_with(OUT_OF_SCOPE_CLASS) {
        Treatment::OutOfScope
    } else if rate.is_zero() {
        Treatment::ZeroRated
    } else {
        Treatment::Standard
    }
}

/// A VAT exemption reason code (VATEX) and its meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Vatex {
    pub code: &'static str,
    /// The category it belongs to (`Z`, `E` or `O`).
    pub category: &'static str,
    /// What the code covers, in KhataOS's words from research §6.2. The
    /// e-invoice prints this as the exemption reason (BT-121); ZATCA's code
    /// list has the official English and Arabic texts, which should replace
    /// these before production [UNCERTAIN].
    pub meaning: &'static str,
}

const fn x(code: &'static str, category: &'static str, meaning: &'static str) -> Vatex {
    Vatex { code, category, meaning }
}

/// The VATEX codes listed in research §6.2 (XML standard §11.2.4
/// [PRIMARY]).
pub const VATEX: &[Vatex] = &[
    x("VATEX-SA-29", "E", "Financial services"),
    x("VATEX-SA-29-7", "E", "Life insurance services"),
    x("VATEX-SA-30", "E", "Real estate transactions"),
    x("VATEX-SA-32", "Z", "Export of goods"),
    x("VATEX-SA-33", "Z", "Export of services"),
    x("VATEX-SA-34-1", "Z", "International transport of goods"),
    x("VATEX-SA-34-2", "Z", "International transport of passengers"),
    x("VATEX-SA-34-3", "Z", "Services directly connected to international transport of passengers"),
    x("VATEX-SA-34-4", "Z", "Supply of a qualifying means of transport"),
    x("VATEX-SA-34-5", "Z", "Services relating to goods or passenger transportation"),
    x("VATEX-SA-35", "Z", "Medicines and medical equipment"),
    x("VATEX-SA-36", "Z", "Qualifying metals"),
    x("VATEX-SA-EDU", "Z", "Private education to citizens"),
    x("VATEX-SA-HEA", "Z", "Private healthcare to citizens"),
    x("VATEX-SA-MLTRY", "Z", "Supply of qualified military goods"),
    x("VATEX-SA-OOS", "O", "Not subject to VAT"),
];

/// The VATEX code a tax class names ("EXEMPT VATEX-SA-29" →
/// VATEX-SA-29), when it is a known one.
pub fn vatex_in_class(class: &str) -> Option<&'static Vatex> {
    let class = squeeze(class);
    let at = class.find("VATEX-SA-")?;
    let code = &class[at..];
    VATEX.iter().find(|v| v.code == code)
}

/// Look up a VATEX code.
pub fn vatex(code: &str) -> Option<&'static Vatex> {
    VATEX.iter().find(|v| v.code.eq_ignore_ascii_case(code.trim()))
}

fn known_region(code: &str) -> Result<(), TaxError> {
    regions::region(code)
        .map(|_| ())
        .ok_or_else(|| TaxError::UnknownRegion { kind: "region code", code: code.to_owned() })
}

/// How VAT is accounted for on a line, beyond its rate: marked with a
/// prefix on the line's tax class (on the stock item, or on the service or
/// tax ledger), because the core model has no field for it. The rest of the
/// class is the rate class as usual: "RC:STANDARD" is the reverse charge at
/// the STANDARD rate. Same convention as the UAE and UK packs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Mechanism {
    /// The supplier charges VAT (or the line is zero-rated or exempt).
    #[default]
    Normal,
    /// "RC:": services (and goods) bought from a supplier abroad, whose VAT
    /// the business accounts for under the reverse charge (IR Art 47):
    /// "imports subject to VAT accounted for through the reverse charge".
    ReverseCharge,
    /// "IMPORT:": goods imported with VAT paid at customs: the line's value
    /// goes in "imports subject to VAT paid at customs"; the VAT on the
    /// customs declaration comes from a VAT ledger classed "IMPORT".
    Import,
}

/// The class prefix of [`Mechanism::ReverseCharge`].
pub const REVERSE_CHARGE_PREFIX: &str = "RC:";
/// The class prefix of [`Mechanism::Import`].
pub const IMPORT_PREFIX: &str = "IMPORT:";

/// A tax class split into its mechanism and its rate class, compared without
/// spaces and case ("rc: standard" is "RC:STANDARD"). A bare "RC" or
/// "IMPORT" (no rate class) means the STANDARD class.
pub fn mechanism(class: &str) -> (Mechanism, String) {
    let key: String = class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    for (prefix, m) in [(REVERSE_CHARGE_PREFIX, Mechanism::ReverseCharge), (IMPORT_PREFIX, Mechanism::Import)] {
        let bare = &prefix[..prefix.len() - 1];
        if key == bare {
            return (m, STANDARD_CLASS.to_owned());
        }
        if let Some(rest) = key.strip_prefix(prefix) {
            return (m, if rest.is_empty() { STANDARD_CLASS.to_owned() } else { rest.to_owned() });
        }
    }
    (Mechanism::Normal, key)
}

/// VAT on a value at a rate, before rounding.
pub fn exact_vat(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round a SAR amount to the halala (2 places), half up: ZATCA's rule for
/// amounts (XML standard §10, research §6.2 [PRIMARY]). Negative amounts
/// (credit notes) round symmetrically, away from zero at the midpoint.
pub fn round_halalas(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn vat(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax {
        components: vec![TaxComponent { tax: VAT, percent, amount: round_halalas(exact_vat(value, percent)?) }],
    })
}

/// VAT on one line: one "VAT" component, rounded half up to the halala.
///
/// - Inside the Kingdom, the rate is the class's rate on the line's date
///   from the company's schedule (15% standard from 1 Jul 2020, 5% before;
///   0% for zero-rated, exempt and out-of-scope classes). Both regions must
///   be known codes ([`crate::regions`]); the region never changes the rate.
/// - An export ([`PlaceOfSupply::Abroad`]) is zero-rated: VAT 0%, whatever
///   the class. The export conditions are not checked.
/// - KSA has no designated-zone rule in the research, so `special_zone`
///   changes nothing, and `without_payment` on a domestic supply is an
///   error: a domestic supply either carries VAT or is zero-rated or exempt
///   by its class.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    known_region(&supply.supplier_region)?;
    match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return vat(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_region(code)?,
    }
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    if mechanism(&line.tax_class).0 != Mechanism::Normal {
        // The supplier's invoice carries no VAT: the business accounts for
        // it on the return (reverse charge) or at customs (import).
        return Ok(LineTax::default());
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    vat(rate, line.taxable_value)
}

/// Longest invoice number accepted. The research asks only for a unique
/// sequential number (IR Art 53, research §5 [SECONDARY]) and gives no
/// length. 35 is the same KhataOS choice as the UAE pack (the UN/EDIFACT
/// document-number length), and keeps the number on one printed line.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain: a KhataOS choice, as in the UAE
/// pack. ZATCA's file name replaces every non-alphanumeric character of the
/// number with "-" (XML standard §14, research §6.2), so "A/1" and "A-1"
/// would share a file name: KhataOS refuses that clash by checking
/// uniqueness on the file-name form too.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// The invoice number as it appears in a ZATCA file name.
pub fn file_name_form(number: &str) -> String {
    number.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Tax invoice, credit note and debit note numbers: not empty, at most
/// [`MAX_INVOICE_NUMBER_LEN`] characters of [`ALLOWED_CHARACTERS`], and
/// unique among every number already issued in the series, whatever its
/// year (no yearly reset), ignoring case and treating "/" and "-" alike (see
/// [`ALLOWED_CHARACTERS`]). `year_of` names the year of a clash.
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
    let key = file_name_form(number);
    if let Some(clash) = issued.iter().find(|d| file_name_form(d.number).eq_ignore_ascii_case(&key)) {
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

    fn line(class: &str, value: Amount, supply: Supply) -> TaxLine {
        TaxLine { date: d(2026, 5, 1), tax_class: class.into(), taxable_value: value, supply }
    }

    fn split(t: &LineTax) -> Vec<(&'static str, Decimal, Amount)> {
        t.components.iter().map(|c| (c.tax, c.percent, c.amount)).collect()
    }

    #[test]
    fn mechanisms_come_from_class_prefixes() {
        assert_eq!(mechanism("RC:STANDARD"), (Mechanism::ReverseCharge, "STANDARD".into()));
        assert_eq!(mechanism(" import "), (Mechanism::Import, "STANDARD".into()));
        assert_eq!(mechanism("ZERO VATEX-SA-35"), (Mechanism::Normal, "ZEROVATEX-SA-35".into()));
    }

    #[test]
    fn default_rates_are_five_then_fifteen_percent() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2017, 12, 31)), None, "no VAT before 2018");
        assert_eq!(r.rate(STANDARD_CLASS, d(2018, 1, 1)), Some(dec!(5)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2020, 6, 30)), Some(dec!(5)));
        assert_eq!(r.rate(STANDARD_CLASS, d(2020, 7, 1)), Some(dec!(15)));
        for class in [ZERO_RATED_CLASS, EXEMPT_CLASS, OUT_OF_SCOPE_CLASS] {
            assert_eq!(r.rate(class, d(2026, 1, 1)), Some(dec!(0)), "{class}");
        }
    }

    #[test]
    fn standard_rated_lines_carry_fifteen_percent() {
        let t = line_tax(&line("STANDARD", dec!(1000), supply("SA-01", Some("SA-01"))), &default_rates()).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(15), dec!(150))]);
        let t = line_tax(&line("STANDARD", dec!(1000), supply("SA-01", Some("sa-04"))), &default_rates()).unwrap();
        assert_eq!(t.total(), dec!(150), "another region changes nothing");
        let mut old = line("STANDARD", dec!(1000), supply("SA-01", Some("SA-01")));
        old.date = d(2020, 6, 30);
        assert_eq!(line_tax(&old, &default_rates()).unwrap().total(), dec!(50), "5% before July 2020");
    }

    #[test]
    fn rounding_is_half_up_to_the_halala() {
        let rates = default_rates();
        let tax = |v: Amount| line_tax(&line("STANDARD", v, supply("SA-01", Some("SA-01"))), &rates).unwrap().total();
        assert_eq!(tax(dec!(0.10)), dec!(0.02), "0.015 → 0.02");
        assert_eq!(tax(dec!(-0.10)), dec!(-0.02), "credit notes round symmetrically");
        assert_eq!(tax(dec!(0.03)), dec!(0), "0.0045 → 0.00");
        assert_eq!(tax(dec!(1999.90)), dec!(299.99), "299.985 → 299.99");
        assert_eq!(tax(dec!(33.33)), dec!(5.00), "4.9995 → 5.00");
    }

    #[test]
    fn zero_exempt_and_out_of_scope_classes_are_zero_percent() {
        let mut rates = default_rates();
        rates.set("EXEMPT VATEX-SA-29", d(2018, 1, 1), dec!(0)).unwrap();
        for class in ["ZERO", "EXEMPT", "exempt vatex-sa-29", "Out of scope"] {
            let t = line_tax(&line(class, dec!(1000), supply("SA-02", Some("SA-02"))), &rates).unwrap();
            assert_eq!(split(&t), [(VAT, dec!(0), dec!(0))], "{class}");
        }
    }

    #[test]
    fn exports_are_zero_rated_and_domestic_supplies_cannot_skip_vat() {
        let t = line_tax(&line("STANDARD", dec!(1000), supply("SA-01", None)), &default_rates()).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(0), dec!(0))]);
        assert_eq!(line_tax(&line("NO-SUCH", dec!(1), supply("SA-01", None)), &default_rates()).unwrap().total(), dec!(0));
        let mut s = supply("SA-01", Some("SA-01"));
        s.special_zone = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(100), s.clone()), &default_rates()).unwrap().total(), dec!(15));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(100), s), &default_rates()), Err(TaxError::NotZeroRated));
    }

    #[test]
    fn unknown_regions_missing_rates_and_overflow_are_errors() {
        let bad = |code: &str| TaxError::UnknownRegion { kind: "region code", code: code.into() };
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("DXB", Some("SA-01"))), &default_rates()), Err(bad("DXB")));
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("SA-01", Some("SA-13"))), &default_rates()), Err(bad("SA-13")));
        assert!(matches!(
            line_tax(&line("8471", dec!(1), supply("SA-01", Some("SA-01"))), &default_rates()),
            Err(TaxError::NoRate { .. })
        ));
        assert_eq!(
            line_tax(&line("STANDARD", Amount::MAX, supply("SA-01", Some("SA-01"))), &default_rates()),
            Err(TaxError::Overflow)
        );
    }

    #[test]
    fn treatments_categories_and_vatex_codes() {
        assert_eq!(treatment("STANDARD", dec!(15)), Treatment::Standard);
        assert_eq!(treatment("ZERO VATEX-SA-35", dec!(0)), Treatment::ZeroRated);
        assert_eq!(treatment(" exempt ", dec!(15)), Treatment::Exempt, "the name wins over a wrong rate");
        assert_eq!(treatment("out of scope", dec!(0)), Treatment::OutOfScope);
        let cats: Vec<_> =
            [Treatment::Standard, Treatment::ZeroRated, Treatment::Exempt, Treatment::OutOfScope].map(Treatment::category).into();
        assert_eq!(cats, ["S", "Z", "E", "O"]);
        assert_eq!(vatex_in_class("Zero VATEX-SA-35").map(|v| (v.code, v.category)), Some(("VATEX-SA-35", "Z")));
        assert_eq!(vatex_in_class("EXEMPT vatex-sa-29-7").map(|v| v.code), Some("VATEX-SA-29-7"));
        assert_eq!(vatex_in_class("ZERO"), None);
        assert_eq!(vatex_in_class("ZERO VATEX-SA-99"), None, "unknown codes are not guessed");
        assert_eq!(vatex("vatex-sa-oos").map(|v| v.category), Some("O"));
        for v in VATEX {
            assert!(["Z", "E", "O"].contains(&v.category) && v.code.starts_with("VATEX-SA-"));
        }
    }

    fn calendar(date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, 1)
    }

    #[test]
    fn invoice_numbers_follow_the_kept_rules() {
        let ok = |n: &str| validate_invoice_number(n, &[], calendar);
        assert_eq!(ok("1"), Ok(()));
        assert_eq!(ok("SINV/2026/000001"), Ok(()));
        assert_eq!(ok(&"A".repeat(35)), Ok(()));
        assert!(matches!(ok(&"A".repeat(36)), Err(InvoiceNumberError::TooLong { max: 35, found: 36, .. })));
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["INV 1", "INV#1", "INV_1", "فاتورة1", "INV١"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
    }

    #[test]
    fn invoice_numbers_are_unique_across_years_and_file_names() {
        let issued = [IssuedDocument { number: "INV/1", date: d(2024, 12, 31) }];
        assert_eq!(
            validate_invoice_number("inv/1", &issued, calendar),
            Err(InvoiceNumberError::Duplicate { number: "inv/1".into(), year: "2024".into() })
        );
        assert!(validate_invoice_number("INV-1", &issued, calendar).is_err(), "same ZATCA file name");
        assert_eq!(validate_invoice_number("INV-2", &issued, calendar), Ok(()));
        assert_eq!(file_name_form("SINV/2026/1"), "SINV-2026-1");
    }
}

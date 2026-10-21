//! GST on an invoice line, GST treatments, and tax invoice numbers.
//!
//! Sources: `docs/research/singapore-compliance.md` §1 (rate history), §3
//! (treatments) and §4 (invoices). Not modelled yet, and so never applied
//! silently:
//! - who must apply the reverse charge (an RC business) and which goods are
//!   low-value: the user marks the lines ([`Mechanism`]); OVR and customer
//!   accounting are not modelled;
//! - the Gross Margin Scheme, tourist refunds, bad debt relief;
//! - transitional rules for supplies spanning a rate change: the rate is
//!   the one in force on the document date;
//! - the conditions for zero-rating (export evidence, the s21(3) list of
//!   international services): a supply is taken as zero-rated as soon as it
//!   is marked [`PlaceOfSupply::Abroad`] or has a 0% class.

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

/// The one tax component on a Singapore line.
pub const GST: &str = "GST";

/// The standard rate since 1 Jan 2024 (IRAS rate table [PRIMARY]). Only
/// [`default_rates`] uses the history: [`line_tax`] always takes the rate
/// from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(9, 0, 0, false, 0);

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

/// The day GST started (1 Apr 1994).
pub fn gst_start() -> NaiveDate {
    date(1994, 4, 1)
}

/// The standard rate's history, from IRAS's rate table [PRIMARY]: 3% from
/// 1 Apr 1994, 4% from 1 Jan 2003, 5% from 1 Jan 2004, 7% from 1 Jul 2007,
/// 8% from 1 Jan 2023, 9% from 1 Jan 2024.
pub fn standard_rate_history() -> [(NaiveDate, Decimal); 6] {
    [
        (gst_start(), Decimal::new(3, 0)),
        (date(2003, 1, 1), Decimal::new(4, 0)),
        (date(2004, 1, 1), Decimal::new(5, 0)),
        (date(2007, 7, 1), Decimal::new(7, 0)),
        (date(2023, 1, 1), Decimal::new(8, 0)),
        (date(2024, 1, 1), STANDARD_PERCENT),
    ]
}

/// Suggested tax classes for a new company's rate list. A company may also
/// key rates by HS code; these names only matter to [`treatment`].
pub const STANDARD_CLASS: &str = "STANDARD";
/// Zero-rated supplies: exports of goods and international services under
/// s21(3) of the GST Act.
pub const ZERO_RATED_CLASS: &str = "ZERO";
/// Exempt supplies: financial services, sale and lease of residential
/// property, investment precious metals, digital payment tokens. Any class
/// whose name starts with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";
/// Out-of-scope supplies: goods sold overseas-to-overseas, overseas goods
/// sold in a Free Trade Zone or Zero GST Warehouse, private transactions.
/// They carry no GST and are not reported in the GST F5. Any class whose
/// name starts with this is out of scope.
pub const OUT_OF_SCOPE_CLASS: &str = "OUT-OF-SCOPE";

/// The rate list a new Singapore company can start from: the standard
/// rate's full history, and 0% zero-rated, exempt and out-of-scope classes
/// from 1 Apr 1994.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    for (from, percent) in standard_rate_history() {
        r.set(STANDARD_CLASS, from, percent).expect("valid default rate");
    }
    for class in [ZERO_RATED_CLASS, EXEMPT_CLASS, OUT_OF_SCOPE_CLASS] {
        r.set(class, gst_start(), Decimal::ZERO).expect("valid default rate");
    }
    r
}

/// How a supply is treated for GST. Zero-rated, exempt and out-of-scope
/// supplies all carry no GST, but they go to different GST F5 boxes (2, 3,
/// and none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    Standard,
    ZeroRated,
    Exempt,
    OutOfScope,
}

/// A class compared without spaces, hyphens and case, so "out of scope",
/// "OUT-OF-SCOPE" and "Out-of-Scope FTZ" are all out of scope.
fn class_key(class: &str) -> String {
    class.chars().filter(|c| !(c.is_whitespace() || *c == '-')).collect::<String>().to_ascii_uppercase()
}

fn is_out_of_scope(class: &str) -> bool {
    class_key(class).starts_with("OUTOFSCOPE")
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, one named "OUT-OF-SCOPE…" (or "OUT OF
/// SCOPE…") is out of scope, any other class at 0% is zero-rated, and a positive rate is
/// standard. A rate is only one number, so the class name is the only way
/// to tell the three 0% treatments apart.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class = class_key(class);
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if class.starts_with("OUTOFSCOPE") {
        Treatment::OutOfScope
    } else if rate.is_zero() {
        Treatment::ZeroRated
    } else {
        Treatment::Standard
    }
}

/// How GST is accounted for on a line, beyond its rate: marked with a
/// prefix on the line's tax class (on the stock item, or on the service or
/// tax ledger), because the core model has no field for it. The rest of the
/// class is the rate class as usual: "RC:STANDARD" is the reverse charge at
/// the STANDARD rate. Same convention as the UAE and UK packs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Mechanism {
    /// The supplier charges GST (or the line is zero-rated or exempt).
    #[default]
    Normal,
    /// "RC:": an RC business accounts for GST on imported services and
    /// low-value goods under the reverse charge (IRAS e-Tax Guide "GST:
    /// Reverse Charge"): value in Boxes 1, 5 and 14, GST in Boxes 6 and 7.
    ReverseCharge,
    /// "IMPORT:": goods imported with GST paid to Singapore Customs: the
    /// line's value goes in Box 5; the GST on the import permit is claimed
    /// in Box 7 through a GST ledger classed "IMPORT".
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

/// GST on a value at a rate, before rounding.
pub fn exact_gst(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round an SGD amount to the cent. IRAS lets the total GST be rounded "to
/// the nearest cent" (research §4 [PRIMARY]); the direction of a half cent
/// is not stated [UNCERTAIN], so this rounds half away from zero, the
/// commercial convention.
pub fn round_cents(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// The GST on a whole document from its lines' (value, percent), rounded
/// once, as IRAS describes rounding the total GST. [`line_tax`] rounds each
/// line because the books post a figure per line; the two can differ by a
/// few cents on a long invoice, and an e-invoice (InvoiceNow) should carry
/// this figure.
pub fn document_gst(lines: &[(Amount, Decimal)]) -> Result<Amount, TaxError> {
    let mut total = Amount::ZERO;
    for (value, percent) in lines {
        total = total.checked_add(exact_gst(*value, *percent)?).ok_or(TaxError::Overflow)?;
    }
    Ok(round_cents(total))
}

fn gst(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: GST, percent, amount: round_cents(exact_gst(value, percent)?) }] })
}

/// GST on one line: one "GST" component, rounded to the cent.
///
/// - Singapore has no tax regions: the region codes in the supply are not
///   used.
/// - A class named "OUT-OF-SCOPE…" carries no GST component at all.
/// - An export ([`PlaceOfSupply::Abroad`]) is zero-rated: GST 0%, whatever
///   the class. The export conditions are not checked.
/// - Otherwise the rate is the class's rate on the line's date from the
///   company's schedule (9% standard from 1 Jan 2024; 0% for zero-rated and
///   exempt classes).
/// - `without_payment` on a supply to a special zone is the user's
///   statement that it is out of scope (overseas goods sold within a Free
///   Trade Zone); the line then has no GST component. On any other local
///   supply it is an error.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    if is_out_of_scope(&line.tax_class) || mechanism(&line.tax_class).0 != Mechanism::Normal {
        // Out of scope, or reverse charge / import: the invoice carries no
        // GST (the business accounts for it on the return or at customs).
        return Ok(LineTax::default());
    }
    if supply.place_of_supply == PlaceOfSupply::Abroad {
        return gst(Decimal::ZERO, line.taxable_value);
    }
    if supply.without_payment {
        return if supply.special_zone { Ok(LineTax::default()) } else { Err(TaxError::NotZeroRated) };
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    gst(rate, line.taxable_value)
}

/// Longest invoice number accepted. IRAS asks only for "an invoice
/// identifying number" (research §4 [PRIMARY]) and sets no limit. 35 is a
/// KhataOS choice, as in the UAE pack: the length of a document number in
/// UN/EDIFACT (data element 1004), which keeps numbers safe for e-invoice
/// chains (InvoiceNow) and on one printed line.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain. Also a KhataOS choice, not a legal
/// rule: letters and digits with "-" and "/" as separators. Spaces are
/// refused: "INV 1" and "INV1" would look like different numbers while
/// meaning the same invoice.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Tax invoice, credit and debit note numbers: not empty, at most
/// [`MAX_INVOICE_NUMBER_LEN`] characters of [`ALLOWED_CHARACTERS`], and
/// unique among every number already issued in the series, whatever its
/// year. An identifying number must identify one invoice, so KhataOS never
/// lets a number come back in a later year. Uniqueness ignores case.
/// `year_of` names the year of a clash in the error.
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

    fn supply(abroad: bool) -> Supply {
        Supply {
            supplier_region: "SG".into(),
            place_of_supply: if abroad { PlaceOfSupply::Abroad } else { PlaceOfSupply::Region("SG".into()) },
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
        assert_eq!(mechanism("rc"), (Mechanism::ReverseCharge, "STANDARD".into()));
        assert_eq!(mechanism("Import: Standard"), (Mechanism::Import, "STANDARD".into()));
        assert_eq!(mechanism("ZERO"), (Mechanism::Normal, "ZERO".into()));
        let rates = default_rates();
        let line = TaxLine {
            date: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            tax_class: "RC:STANDARD".into(),
            taxable_value: Decimal::ONE_HUNDRED,
            supply: khata_compliance::Supply {
                supplier_region: String::new(),
                place_of_supply: PlaceOfSupply::Region(String::new()),
                special_zone: false,
                without_payment: false,
            },
        };
        assert!(line_tax(&line, &rates).unwrap().components.is_empty());
    }

    #[test]
    fn default_rates_follow_the_iras_history() {
        let r = default_rates();
        let at = |y, m, day| r.rate(STANDARD_CLASS, d(y, m, day));
        assert_eq!(at(1994, 3, 31), None, "no GST before 1 Apr 1994");
        assert_eq!(at(1994, 4, 1), Some(dec!(3)));
        assert_eq!(at(2002, 12, 31), Some(dec!(3)));
        assert_eq!(at(2003, 1, 1), Some(dec!(4)));
        assert_eq!(at(2004, 1, 1), Some(dec!(5)));
        assert_eq!(at(2007, 6, 30), Some(dec!(5)));
        assert_eq!(at(2007, 7, 1), Some(dec!(7)));
        assert_eq!(at(2022, 12, 31), Some(dec!(7)));
        assert_eq!(at(2023, 1, 1), Some(dec!(8)));
        assert_eq!(at(2023, 12, 31), Some(dec!(8)));
        assert_eq!(at(2024, 1, 1), Some(dec!(9)));
        assert_eq!(at(2030, 1, 1), Some(dec!(9)));
        for class in [ZERO_RATED_CLASS, EXEMPT_CLASS, OUT_OF_SCOPE_CLASS] {
            assert_eq!(r.rate(class, d(2026, 1, 1)), Some(dec!(0)), "{class}");
        }
        assert_eq!(STANDARD_PERCENT, dec!(9));
    }

    #[test]
    fn standard_rated_lines_take_the_rate_on_their_date() {
        let rates = default_rates();
        let mut l = line("STANDARD", dec!(1000), supply(false));
        assert_eq!(split(&line_tax(&l, &rates).unwrap()), [(GST, dec!(9), dec!(90))]);
        l.date = d(2023, 12, 31);
        assert_eq!(split(&line_tax(&l, &rates).unwrap()), [(GST, dec!(8), dec!(80))], "old invoices keep 8%");
        l.date = d(2022, 6, 1);
        assert_eq!(line_tax(&l, &rates).unwrap().total(), dec!(70));
        l.date = d(1994, 3, 31);
        assert_eq!(line_tax(&l, &rates), Err(TaxError::NoRate { class: "STANDARD".into(), date: d(1994, 3, 31) }));
    }

    #[test]
    fn regions_are_not_used() {
        let mut s = supply(false);
        s.supplier_region = String::new();
        s.place_of_supply = PlaceOfSupply::Region("anything".into());
        assert_eq!(line_tax(&line("STANDARD", dec!(100), s), &default_rates()).unwrap().total(), dec!(9));
    }

    #[test]
    fn zero_rated_exempt_and_out_of_scope_classes() {
        let rates = default_rates();
        for class in ["ZERO", "EXEMPT", "exempt residential"] {
            let mut rates = rates.clone();
            rates.set("EXEMPTRESIDENTIAL", d(1994, 4, 1), dec!(0)).unwrap();
            let t = line_tax(&line(class, dec!(1000), supply(false)), &rates).unwrap();
            assert_eq!(split(&t), [(GST, dec!(0), dec!(0))], "{class}");
        }
        for class in ["OUT-OF-SCOPE", "out-of-scope ftz", "OUT-OF-SCOPE-NO-RATE"] {
            assert_eq!(line_tax(&line(class, dec!(1000), supply(false)), &rates).unwrap(), LineTax::default(), "{class}");
            assert_eq!(line_tax(&line(class, dec!(1000), supply(true)), &rates).unwrap(), LineTax::default(), "{class} abroad");
        }
    }

    #[test]
    fn exports_are_zero_rated_whatever_the_class() {
        let t = line_tax(&line("STANDARD", dec!(1000), supply(true)), &default_rates()).unwrap();
        assert_eq!(split(&t), [(GST, dec!(0), dec!(0))]);
        let t = line_tax(&line("NO-SUCH-CLASS", dec!(1000), supply(true)), &default_rates()).unwrap();
        assert_eq!(t.total(), dec!(0), "an export needs no rate on file");
        let mut s = supply(true);
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1000), s), &default_rates()).unwrap().total(), dec!(0));
    }

    #[test]
    fn free_trade_zones_are_taxed_unless_marked_out_of_scope() {
        let mut s = supply(false);
        s.special_zone = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(200), s.clone()), &default_rates()).unwrap().total(), dec!(18));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(200), s), &default_rates()).unwrap(), LineTax::default());
        let mut local = supply(false);
        local.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1), local), &default_rates()), Err(TaxError::NotZeroRated));
    }

    #[test]
    fn hs_code_classes_work_too() {
        let mut rates = default_rates();
        let l = line("8471", dec!(100), supply(false));
        assert!(matches!(line_tax(&l, &rates), Err(TaxError::NoRate { .. })));
        rates.set("84", d(2024, 1, 1), dec!(9)).unwrap();
        assert_eq!(line_tax(&l, &rates).unwrap().total(), dec!(9));
        assert_eq!(line_tax(&line("STANDARD", Amount::MAX, supply(false)), &rates), Err(TaxError::Overflow));
    }

    #[test]
    fn lines_round_to_the_cent_and_documents_round_once() {
        let rates = default_rates();
        // 9% of 0.05 = 0.0045 → 0.00; of 0.06 = 0.0054 → 0.01; 9% of 0.50 = 0.045 → 0.05.
        let tax = |v| line_tax(&line("STANDARD", v, supply(false)), &rates).unwrap().total();
        assert_eq!(tax(dec!(0.05)), dec!(0));
        assert_eq!(tax(dec!(0.06)), dec!(0.01));
        assert_eq!(tax(dec!(0.50)), dec!(0.05), "a half cent rounds up");
        assert_eq!(tax(dec!(-0.50)), dec!(-0.05), "and away from zero on credit notes");
        // Ten lines of 0.06: 0.10 when each line is rounded, 0.05 at document level.
        assert_eq!(document_gst(&vec![(dec!(0.06), dec!(9)); 10]), Ok(dec!(0.05)));
        assert_eq!(document_gst(&[(dec!(1999.99), dec!(9)), (dec!(5), dec!(0))]), Ok(dec!(180.00)));
        assert_eq!(document_gst(&[(Amount::MAX, dec!(9)), (Amount::MAX, dec!(9))]), Err(TaxError::Overflow));
    }

    #[test]
    fn treatments_come_from_class_names_and_rates() {
        assert_eq!(treatment("STANDARD", dec!(9)), Treatment::Standard);
        assert_eq!(treatment("8471", dec!(9)), Treatment::Standard);
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::ZeroRated);
        assert_eq!(treatment("9983", dec!(0)), Treatment::ZeroRated, "any other 0% class is zero-rated");
        assert_eq!(treatment(" exempt dpt ", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("EXEMPT", dec!(9)), Treatment::Exempt, "the name wins over a wrong rate");
        assert_eq!(treatment("Out-of-scope", dec!(0)), Treatment::OutOfScope);
        assert_eq!(treatment("out of scope", dec!(0)), Treatment::OutOfScope, "spaces or hyphens");
        assert_eq!(treatment("OUTPUT", dec!(9)), Treatment::Standard);
    }

    fn calendar(date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, 1)
    }

    #[test]
    fn invoice_numbers_follow_the_kept_rules() {
        let ok = |n: &str| validate_invoice_number(n, &[], calendar);
        assert_eq!(ok("1"), Ok(()));
        assert_eq!(ok("2026/1"), Ok(()));
        assert_eq!(ok("TI-2026-000001"), Ok(()));
        assert_eq!(ok(&"A".repeat(35)), Ok(()));
        let long = "A".repeat(36);
        assert_eq!(ok(&long), Err(InvoiceNumberError::TooLong { number: long.clone(), max: 35, found: 36 }));
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["INV 1", " INV1", "INV#1", "INV_1", "INV.1", "发票1"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
    }

    #[test]
    fn invoice_numbers_are_unique_across_years() {
        let issued = [IssuedDocument { number: "INV-1", date: d(2024, 12, 31) }];
        assert_eq!(
            validate_invoice_number("inv-1", &issued, calendar),
            Err(InvoiceNumberError::Duplicate { number: "inv-1".into(), year: "2024".into() })
        );
        assert_eq!(validate_invoice_number("INV-2", &issued, calendar), Ok(()));
    }
}

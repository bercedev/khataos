//! VAT on an invoice line, VAT treatments, and tax invoice numbers.
//!
//! Sources: `docs/research/mena-compliance.md` §1.2 and §1.3 (Federal
//! Decree-Law No. 8 of 2017 on VAT, as amended, and its Executive
//! Regulation). Reverse charge and imports are marked on the tax class
//! ([`Mechanism`], "RC:" and "IMPORT:" prefixes) and worked out by the
//! VAT 201 ([`crate::returns`]); the goods-type rules of the domestic
//! reverse charge (who must apply it) are the user's call. Not modelled
//! yet, and so never applied silently:
//! - the profit margin scheme (Art 43);
//! - out-of-scope supplies, other than goods inside designated zones that
//!   the user marks as such (see [`line_tax`]);
//! - the conditions for zero-rating an export (evidence of export, time
//!   limits): an export is taken as zero-rated as soon as it is marked
//!   [`PlaceOfSupply::Abroad`].

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::emirates;

/// The one tax component on a UAE line.
pub const VAT: &str = "VAT";

/// The standard rate since VAT started on 1 Jan 2018 (VAT Law Art 3
/// [SECONDARY]). Only [`default_rates`] uses it: [`line_tax`] always takes
/// the rate from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

/// The day UAE VAT started.
pub fn vat_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2018, 1, 1).expect("valid date")
}

/// Suggested tax classes for a new company's rate list. A company may also
/// key rates by HS code; these names only matter to [`treatment`].
pub const STANDARD_CLASS: &str = "STANDARD";
/// Zero-rated supplies (VAT Law Art 45): exports, international transport,
/// qualifying education and healthcare, first supply of new residential
/// buildings, and the rest of the Art 45 list.
pub const ZERO_RATED_CLASS: &str = "ZERO";
/// Exempt supplies (VAT Law Art 46): certain financial services, residential
/// leases, bare land, local passenger transport. Any class whose name starts
/// with this is exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";

/// The rate list a new UAE company can start from: 5% standard, 0% zero
/// rated and 0% exempt, all from 1 Jan 2018.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    for (class, percent) in
        [(STANDARD_CLASS, STANDARD_PERCENT), (ZERO_RATED_CLASS, Decimal::ZERO), (EXEMPT_CLASS, Decimal::ZERO)]
    {
        r.set(class, vat_start(), percent).expect("valid default rate");
    }
    r
}

/// How a supply is treated for VAT. Zero-rated and exempt supplies both
/// carry no VAT, but they go to different VAT 201 boxes (4 and 5), and only
/// zero-rated supplies keep the right to recover input tax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    Standard,
    ZeroRated,
    Exempt,
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, any other class at 0% is zero-rated, and
/// a positive rate is standard. A rate is only one number, so this naming
/// convention is the only way to tell exempt from zero-rated.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class: String = class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if rate.is_zero() {
        Treatment::ZeroRated
    } else {
        Treatment::Standard
    }
}

/// How VAT is accounted for on a line, beyond its rate: marked with a prefix
/// on the line's tax class (on the stock item, or on the service ledger),
/// because the core model has no field for it. The rest of the class is the
/// rate class as usual: "RC:STANDARD" is the reverse charge at the STANDARD
/// rate, "IMPORT:STANDARD" an import through UAE Customs at that rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Mechanism {
    /// The supplier charges VAT (or the line is zero-rated or exempt).
    #[default]
    Normal,
    /// "RC:": the recipient accounts for the VAT (VAT Law Art 48): services
    /// and goods from abroad that are not imported through UAE Customs, and
    /// the domestic reverse charge (electronic devices, precious metals and
    /// stones, hydrocarbons, metal scrap; research §1.2). On a purchase:
    /// VAT 201 Box 3 (due) and Box 10 (recoverable). On a sale: the supplier
    /// charges no VAT (see `returns`).
    ReverseCharge,
    /// "IMPORT:": goods imported through UAE Customs, whose VAT a registered
    /// importer accounts for on the return: Box 6 (pre-filled by the FTA
    /// from the customs declaration) and Box 10.
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

fn known_emirate(code: &str) -> Result<(), TaxError> {
    emirates::emirate(code)
        .map(|_| ())
        .ok_or_else(|| TaxError::UnknownRegion { kind: "emirate code", code: code.to_owned() })
}

/// VAT on a value at a rate, before rounding.
pub fn exact_vat(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round an AED amount to the fil (2 places), half away from zero.
/// The rounding direction for UAE VAT is not stated in the research
/// [UNCERTAIN]; half away from zero is the commercial convention.
pub fn round_fils(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// The VAT on a whole document from its lines' (value, percent), rounded
/// once. The MoF e-invoicing guidelines (§12.2 [PRIMARY]) round only at the
/// invoice total, so an e-invoice should use this figure; [`line_tax`]
/// rounds each line because the tax invoice shows VAT per line (ER Art
/// 59(1)(h)) and the books post it. The two can differ by a few fils on a
/// long invoice: reconciling them is left to the e-invoice serialiser.
pub fn document_vat(lines: &[(Amount, Decimal)]) -> Result<Amount, TaxError> {
    let mut total = Amount::ZERO;
    for (value, percent) in lines {
        total = total.checked_add(exact_vat(*value, *percent)?).ok_or(TaxError::Overflow)?;
    }
    Ok(round_fils(total))
}

fn vat(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: VAT, percent, amount: round_fils(exact_vat(value, percent)?) }] })
}

/// VAT on one line: one "VAT" component, rounded to the fil.
///
/// - Inside the UAE, the rate is the class's rate on the line's date from
///   the company's schedule (5% standard, 0% for zero-rated and exempt
///   classes). Both emirates must be known codes; the emirate never changes
///   the rate.
/// - An export ([`PlaceOfSupply::Abroad`]) is zero-rated (Art 45): VAT 0%,
///   whatever the class. The export conditions are not checked.
/// - A recipient in a designated zone (`special_zone`) is taxed like any
///   other UAE supply. Supplies of goods within or between designated zones
///   can be outside the scope of VAT when the conditions of Art 51 and ER
///   Arts 50 and 51 are met, but services are generally supplied in the UAE
///   and the rules turn on facts this model doesn't hold (research §1.2
///   [SECONDARY]). The user asserts that a zone supply is out of scope by
///   also setting `without_payment`; the line then has no VAT component.
/// - `without_payment` on any other UAE supply is an error: a UAE supply
///   either carries VAT or is zero-rated or exempt by its class.
/// - A class marked "RC:" or "IMPORT:" ([`mechanism`]) has no VAT
///   component: the invoice carries no VAT, the recipient accounts for it on
///   the VAT 201 (the app works it out from the rate class).
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    known_emirate(&supply.supplier_region)?;
    match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return vat(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_emirate(code)?,
    }
    if supply.without_payment {
        return if supply.special_zone { Ok(LineTax::default()) } else { Err(TaxError::NotZeroRated) };
    }
    if mechanism(&line.tax_class).0 != Mechanism::Normal {
        // The supplier's invoice carries no VAT: the recipient accounts
        // for it on the return.
        return Ok(LineTax::default());
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    vat(rate, line.taxable_value)
}

/// Longest invoice number accepted. No UAE limit is published (ER Art
/// 59(1)(d) only asks for a sequential or unique number that identifies the
/// invoice and its order [SECONDARY]). 35 is a KhataOS choice: it is the
/// length of a document number in UN/EDIFACT (data element 1004), which
/// many trading partners' systems still use, and it keeps the number on one
/// printed line. Revisit if PINT AE or the FTA sets a limit.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain. Also a KhataOS choice, not a legal
/// rule: letters and digits with "-" and "/" as separators, the set India's
/// Rule 46 allows, which every system in an e-invoice chain can carry.
/// Spaces are refused: "INV 1" and "INV1" would look like different numbers
/// while meaning the same invoice.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// Tax invoice and tax credit note numbers (ER Arts 59(1)(d), 60): not
/// empty, at most [`MAX_INVOICE_NUMBER_LEN`] characters of
/// [`ALLOWED_CHARACTERS`], and unique among every number already issued in
/// the series, whatever its year: the UAE rule asks for a number that
/// identifies the invoice, with no yearly reset (unlike India's Rule 46).
/// Uniqueness ignores case. `year_of` names the year of a clash in the
/// error.
pub fn validate_invoice_number(
    number: &str,
    issued: &[IssuedDocument<'_>],
    year_of: impl Fn(NaiveDate) -> FinancialYear,
) -> Result<(), InvoiceNumberError> {
    if number.is_empty() {
        return Err(InvoiceNumberError::Empty);
    }
    if let Some(found) = number.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '/')) {
        return Err(InvoiceNumberError::InvalidCharacter {
            number: number.to_owned(),
            found,
            allowed: ALLOWED_CHARACTERS,
        });
    }
    if number.len() > MAX_INVOICE_NUMBER_LEN {
        return Err(InvoiceNumberError::TooLong {
            number: number.to_owned(),
            max: MAX_INVOICE_NUMBER_LEN,
            found: number.len(),
        });
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

    fn line(class: &str, value: Amount, supply: Supply) -> TaxLine {
        TaxLine { date: d(2026, 5, 1), tax_class: class.into(), taxable_value: value, supply }
    }

    fn split(t: &LineTax) -> Vec<(&'static str, Decimal, Amount)> {
        t.components.iter().map(|c| (c.tax, c.percent, c.amount)).collect()
    }

    #[test]
    fn default_rates_are_five_percent_from_2018() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2017, 12, 31)), None, "no VAT before 2018");
        assert_eq!(r.rate(STANDARD_CLASS, d(2018, 1, 1)), Some(dec!(5)));
        assert_eq!(r.rate(ZERO_RATED_CLASS, d(2026, 1, 1)), Some(dec!(0)));
        assert_eq!(r.rate(EXEMPT_CLASS, d(2026, 1, 1)), Some(dec!(0)));
        assert_eq!(STANDARD_PERCENT, dec!(5));
    }

    #[test]
    fn standard_rated_lines_carry_one_vat_component() {
        let t = line_tax(&line("STANDARD", dec!(1000), supply("DXB", Some("DXB"))), &default_rates()).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(5), dec!(50))]);
        // Another emirate changes nothing.
        let t = line_tax(&line("STANDARD", dec!(1000), supply("DXB", Some("auh"))), &default_rates()).unwrap();
        assert_eq!(t.total(), dec!(50));
    }

    #[test]
    fn zero_rated_and_exempt_classes_are_zero_percent() {
        for class in ["ZERO", "EXEMPT", "exempt-finance"] {
            let mut rates = default_rates();
            rates.set("EXEMPT-FINANCE", d(2018, 1, 1), dec!(0)).unwrap();
            let t = line_tax(&line(class, dec!(1000), supply("SHJ", Some("SHJ"))), &rates).unwrap();
            assert_eq!(split(&t), [(VAT, dec!(0), dec!(0))], "{class}");
        }
    }

    #[test]
    fn exports_are_zero_rated_whatever_the_class() {
        let t = line_tax(&line("STANDARD", dec!(1000), supply("DXB", None)), &default_rates()).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(0), dec!(0))]);
        let t = line_tax(&line("NO-SUCH-CLASS", dec!(1000), supply("DXB", None)), &default_rates()).unwrap();
        assert_eq!(t.total(), dec!(0), "an export needs no rate on file");
        let mut s = supply("DXB", None);
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1000), s), &default_rates()).unwrap().total(), dec!(0));
    }

    #[test]
    fn designated_zones_are_standard_rated_unless_marked_out_of_scope() {
        let mut s = supply("DXB", Some("DXB"));
        s.special_zone = true;
        let t = line_tax(&line("STANDARD", dec!(200), s.clone()), &default_rates()).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(5), dec!(10))], "conservative: taxed");
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(200), s), &default_rates()).unwrap(), LineTax::default());
    }

    #[test]
    fn mainland_supplies_cannot_skip_vat() {
        let mut s = supply("DXB", Some("SHJ"));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(1), s), &default_rates()), Err(TaxError::NotZeroRated));
    }

    #[test]
    fn rates_follow_the_invoice_date_and_must_exist() {
        let mut l = line("STANDARD", dec!(100), supply("AJM", Some("AJM")));
        l.date = d(2017, 12, 31);
        assert_eq!(
            line_tax(&l, &default_rates()),
            Err(TaxError::NoRate { class: "STANDARD".into(), date: d(2017, 12, 31) })
        );
        l.date = d(2018, 1, 1);
        assert_eq!(line_tax(&l, &default_rates()).unwrap().total(), dec!(5));
        l.tax_class = "8471".into();
        assert!(matches!(line_tax(&l, &default_rates()), Err(TaxError::NoRate { .. })));
        let mut rates = default_rates();
        rates.set("84", d(2018, 1, 1), dec!(5)).unwrap();
        assert_eq!(line_tax(&l, &rates).unwrap().total(), dec!(5), "HS-code classes work too");
    }

    #[test]
    fn unknown_emirates_and_overflow_are_errors() {
        let bad = |code: &str| TaxError::UnknownRegion { kind: "emirate code", code: code.into() };
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("27", Some("DXB"))), &default_rates()), Err(bad("27")));
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("DXB", Some("XYZ"))), &default_rates()), Err(bad("XYZ")));
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("", None)), &default_rates()), Err(bad("")));
        assert_eq!(
            line_tax(&line("STANDARD", Amount::MAX, supply("DXB", Some("DXB"))), &default_rates()),
            Err(TaxError::Overflow)
        );
    }

    #[test]
    fn lines_round_to_the_fil_and_documents_round_once() {
        // 5% of 0.10 = 0.005 → 0.01; of −0.10 → −0.01 (credit notes).
        let rates = default_rates();
        assert_eq!(line_tax(&line("STANDARD", dec!(0.10), supply("DXB", Some("DXB"))), &rates).unwrap().total(), dec!(0.01));
        assert_eq!(line_tax(&line("STANDARD", dec!(-0.10), supply("DXB", Some("DXB"))), &rates).unwrap().total(), dec!(-0.01));
        assert_eq!(line_tax(&line("STANDARD", dec!(0.09), supply("DXB", Some("DXB"))), &rates).unwrap().total(), dec!(0));
        // Ten lines of 0.10: 0.10 when each line is rounded, 0.05 at document level.
        let lines = vec![(dec!(0.10), dec!(5)); 10];
        assert_eq!(document_vat(&lines), Ok(dec!(0.05)));
        assert_eq!(document_vat(&[(dec!(999.99), dec!(5)), (dec!(0.01), dec!(0))]), Ok(dec!(50.00)));
        assert_eq!(document_vat(&[(Amount::MAX, dec!(5)), (Amount::MAX, dec!(5))]), Err(TaxError::Overflow));
    }

    #[test]
    fn treatments_come_from_class_names_and_rates() {
        assert_eq!(treatment("STANDARD", dec!(5)), Treatment::Standard);
        assert_eq!(treatment("8471", dec!(5)), Treatment::Standard);
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::ZeroRated);
        assert_eq!(treatment("9983", dec!(0)), Treatment::ZeroRated, "any 0% class is zero-rated");
        assert_eq!(treatment("Exempt", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment(" exempt finance ", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("EXEMPT", dec!(5)), Treatment::Exempt, "the name wins over a wrong rate");
    }

    #[test]
    fn mechanisms_come_from_class_prefixes() {
        assert_eq!(mechanism("RC:STANDARD"), (Mechanism::ReverseCharge, "STANDARD".into()));
        assert_eq!(mechanism(" rc : zero "), (Mechanism::ReverseCharge, "ZERO".into()));
        assert_eq!(mechanism("RC"), (Mechanism::ReverseCharge, "STANDARD".into()), "bare prefix: standard");
        assert_eq!(mechanism("IMPORT:"), (Mechanism::Import, "STANDARD".into()));
        assert_eq!(mechanism("import:8471"), (Mechanism::Import, "8471".into()));
        assert_eq!(mechanism("STANDARD"), (Mechanism::Normal, "STANDARD".into()));
        assert_eq!(mechanism("RCX"), (Mechanism::Normal, "RCX".into()), "only the prefix with its colon");
        let rates = default_rates();
        for class in ["RC:STANDARD", "IMPORT:STANDARD"] {
            let t = line_tax(&line(class, dec!(1000), supply("DXB", Some("DXB"))), &rates).unwrap();
            assert!(t.components.is_empty(), "{class}: the invoice carries no VAT");
        }
        assert!(line_tax(&line("RC:STANDARD", dec!(1), supply("XXX", Some("DXB"))), &rates).is_err(), "emirates still checked");
    }

    fn calendar(date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, 1)
    }

    #[test]
    fn invoice_numbers_follow_the_kept_rules() {
        let ok = |n: &str| validate_invoice_number(n, &[], calendar);
        assert_eq!(ok("1"), Ok(()));
        assert_eq!(ok("2026/1"), Ok(()), "KhataOS's default yearly series");
        assert_eq!(ok("TI-DXB/2026/000001"), Ok(()));
        let longest = "A".repeat(35);
        assert_eq!(ok(&longest), Ok(()));
        let long = "A".repeat(36);
        assert_eq!(ok(&long), Err(InvoiceNumberError::TooLong { number: long.clone(), max: 35, found: 36 }));
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["INV 1", " INV1", "INV#1", "INV_1", "INV.1", "فاتورة1", "INV١"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
    }

    #[test]
    fn invoice_numbers_are_unique_across_years() {
        let issued = [
            IssuedDocument { number: "INV-1", date: d(2024, 12, 31) },
            IssuedDocument { number: "INV-2", date: d(2026, 1, 5) },
        ];
        assert_eq!(
            validate_invoice_number("inv-1", &issued, calendar),
            Err(InvoiceNumberError::Duplicate { number: "inv-1".into(), year: "2024".into() }),
            "no yearly reset: an old number cannot come back"
        );
        assert!(validate_invoice_number("INV-2", &issued, calendar).is_err());
        assert_eq!(validate_invoice_number("INV-3", &issued, calendar), Ok(()));
    }
}

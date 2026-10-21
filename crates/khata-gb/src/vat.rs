//! VAT on an invoice line, VAT treatments, rounding, and invoice numbers.
//!
//! Sources: `docs/research/uk-compliance.md` §1.3 and §1.4 (VAT Act 1994
//! ss 2, 29A, 30, 31; VAT Regulations 1995 reg 14; VAT Notice 700 §3, §16,
//! §17). The reverse charges and import VAT are marked on the tax class
//! ([`Mechanism`]); who must apply the domestic reverse charge (the £5,000
//! de minimis, end-user notices) is the user's call. The Flat Rate and Cash
//! Accounting schemes are in [`crate::schemes`]. Not modelled yet, and so
//! never applied silently:
//! - margin schemes and the Annual Accounting Scheme's interim payments;
//! - supplies outside the scope of VAT (they should not be on a VAT invoice);
//! - the conditions for zero-rating an export (evidence, time limits): an
//!   export is zero-rated as soon as it is marked [`PlaceOfSupply::Abroad`].

use chrono::NaiveDate;
use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError,
    TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::nations;

/// The one tax component on a UK line.
pub const VAT: &str = "VAT";

/// The standard rate (VATA 1994 s2(1); 20% for supplies from 4 Jan 2011,
/// Notice 700 §3.3.1 [PRIMARY]). Only [`default_rates`] uses it:
/// [`line_tax`] always takes the rate from the company's schedule.
pub const STANDARD_PERCENT: Decimal = Decimal::from_parts(20, 0, 0, false, 0);

/// The reduced rate (VATA 1994 s29A; 5% since 1 Sep 1997, Notice 700
/// §3.3.1 [PRIMARY]).
pub const REDUCED_PERCENT: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

/// The day the standard rate became 20%. The default list starts here:
/// books for supplies before 2011 are out of scope.
pub fn standard_rate_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2011, 1, 4).expect("valid date")
}

/// Suggested tax classes for a new company's rate list. A company may also
/// key rates by commodity code; these names only matter to [`treatment`].
pub const STANDARD_CLASS: &str = "STANDARD";
/// Reduced-rated supplies (VATA Sch 7A): domestic fuel and power,
/// children's car seats… Any class whose name starts with this is reduced.
pub const REDUCED_CLASS: &str = "REDUCED";
/// Zero-rated supplies (VATA s30, Sch 8): most food, children's clothes,
/// books, exports of goods…
pub const ZERO_RATED_CLASS: &str = "ZERO";
/// Exempt supplies (VATA s31, Sch 9): financial and property transactions,
/// postage stamps, insurance… Any class whose name starts with this is
/// exempt.
pub const EXEMPT_CLASS: &str = "EXEMPT";

/// The rate list a new UK company can start from: 20% standard, 5%
/// reduced, 0% zero-rated and 0% exempt, all from 4 Jan 2011.
pub fn default_rates() -> RateSchedule {
    let mut r = RateSchedule::default();
    for (class, percent) in [
        (STANDARD_CLASS, STANDARD_PERCENT),
        (REDUCED_CLASS, REDUCED_PERCENT),
        (ZERO_RATED_CLASS, Decimal::ZERO),
        (EXEMPT_CLASS, Decimal::ZERO),
    ] {
        r.set(class, standard_rate_start(), percent).expect("valid default rate");
    }
    r
}

/// How a supply is treated for VAT. Every treatment's value goes in Box 6
/// (or 7); only zero-rated, reduced and standard supplies keep the right to
/// recover input tax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Treatment {
    Standard,
    Reduced,
    ZeroRated,
    Exempt,
}

impl Treatment {
    /// Standard or reduced: the line carries VAT.
    pub fn taxed(self) -> bool {
        matches!(self, Treatment::Standard | Treatment::Reduced)
    }
}

/// The treatment of a line from its tax class and the rate in force: a
/// class named "EXEMPT…" is exempt, any other class at 0% is zero-rated, a
/// class named "REDUCED…" is reduced, and any other positive rate is
/// standard. A rate is only one number, so names are the only way to tell
/// exempt from zero-rated.
pub fn treatment(class: &str, rate: Decimal) -> Treatment {
    let class: String = class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    if class.starts_with(EXEMPT_CLASS) {
        Treatment::Exempt
    } else if rate.is_zero() {
        Treatment::ZeroRated
    } else if class.starts_with(REDUCED_CLASS) {
        Treatment::Reduced
    } else {
        Treatment::Standard
    }
}

/// How VAT is accounted for on a line, beyond its rate: marked with a prefix
/// on the line's tax class (stock item, service ledger or VAT ledger),
/// because the core model has no field for it. The rest of the class is the
/// rate class: "DRC:STANDARD" is the domestic reverse charge at the
/// STANDARD rate. Same convention as the UAE, Singapore and Saudi packs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Mechanism {
    /// The supplier charges VAT (or the line is zero-rated or exempt).
    #[default]
    Normal,
    /// "RC:": services bought from a supplier belonging outside the UK
    /// (Notice 741A §5.2): the customer puts the VAT in Boxes 1 and 4 and
    /// the value in Boxes 6 and 7.
    ReverseCharge,
    /// "DRC:": the domestic reverse charge (Notice 735: mobile phones and
    /// chips of £5,000 or more per invoice, wholesale gas and electricity,
    /// telecoms, emissions allowances; and CIS construction services). The
    /// customer: VAT in Boxes 1 and 4, value in Box 7 only (§8.5). The
    /// supplier: value in Box 6, no VAT (§7.7).
    DomesticReverseCharge,
    /// "IMPORT:": goods imported with VAT paid at the border (the C79
    /// certificate is the evidence). The value goes in Box 7; the import VAT
    /// is reclaimed in Box 4, from a VAT ledger classed "IMPORT".
    Import,
    /// "IMPORT:PVA" (or "PVA"): postponed import VAT from the monthly
    /// statement (MPIVS), posted to VAT ledgers classed "IMPORT:PVA": Box 1
    /// and Box 4.
    PostponedImport,
}

/// The class prefix of [`Mechanism::ReverseCharge`].
pub const REVERSE_CHARGE_PREFIX: &str = "RC:";
/// The class prefix of [`Mechanism::DomesticReverseCharge`].
pub const DOMESTIC_REVERSE_CHARGE_PREFIX: &str = "DRC:";
/// The class prefix of [`Mechanism::Import`].
pub const IMPORT_PREFIX: &str = "IMPORT:";
/// The class of [`Mechanism::PostponedImport`].
pub const POSTPONED_IMPORT_CLASS: &str = "IMPORT:PVA";

/// A tax class split into its mechanism and its rate class, compared without
/// spaces and case. A bare prefix ("RC", "DRC:", "IMPORT") means the
/// STANDARD class.
pub fn mechanism(class: &str) -> (Mechanism, String) {
    let key: String = class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    if key == POSTPONED_IMPORT_CLASS || key == "PVA" || key.starts_with("IMPORT:PVA:") || key.starts_with("PVA:") {
        let base = key.rsplit_once("PVA:").map(|(_, b)| b).filter(|b| !b.is_empty()).unwrap_or(STANDARD_CLASS);
        return (Mechanism::PostponedImport, base.to_owned());
    }
    for (prefix, m) in [
        (DOMESTIC_REVERSE_CHARGE_PREFIX, Mechanism::DomesticReverseCharge),
        (REVERSE_CHARGE_PREFIX, Mechanism::ReverseCharge),
        (IMPORT_PREFIX, Mechanism::Import),
    ] {
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

impl Mechanism {
    /// The customer works out the VAT itself at the rate class.
    pub fn self_accounted(self) -> bool {
        matches!(self, Mechanism::ReverseCharge | Mechanism::DomesticReverseCharge)
    }
}

fn known_nation(code: &str) -> Result<(), TaxError> {
    nations::nation(code).map(|_| ()).ok_or_else(|| TaxError::UnknownRegion { kind: "UK nation code", code: code.to_owned() })
}

/// VAT on a value at a rate, before rounding.
pub fn exact_vat(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Round to the nearest penny, half away from zero. Notice 700 §17.5.1
/// allows line VAT "to the nearest 1p" [PRIMARY]; it does not say which way
/// a half-penny goes [UNCERTAIN], so the commercial convention is used.
pub fn round_pence(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// The total VAT on a whole invoice from its lines' (value, percent),
/// rounded **down** to a whole penny: "You may round down the total VAT
/// payable on all goods and services shown on a VAT invoice to a whole
/// penny" (Notice 700 §17.5 [PRIMARY]; not for retailers, §17.6). Down
/// means toward zero, so a credit note's VAT is not inflated either.
/// [`line_tax`] instead rounds each line to the nearest penny (§17.5.1),
/// because the books post VAT per line; the two can differ by a few pence.
pub fn document_vat(lines: &[(Amount, Decimal)]) -> Result<Amount, TaxError> {
    let mut total = Amount::ZERO;
    for (value, percent) in lines {
        total = total.checked_add(exact_vat(*value, *percent)?).ok_or(TaxError::Overflow)?;
    }
    Ok(total.round_dp_with_strategy(2, RoundingStrategy::ToZero))
}

fn vat(percent: Decimal, value: Amount) -> Result<LineTax, TaxError> {
    Ok(LineTax { components: vec![TaxComponent { tax: VAT, percent, amount: round_pence(exact_vat(value, percent)?) }] })
}

/// VAT on one line: one "VAT" component, rounded to the nearest penny.
///
/// - Inside the UK, the rate is the class's rate on the line's date from
///   the company's schedule. Both nations must be known codes; the nation
///   never changes the rate.
/// - An export ([`PlaceOfSupply::Abroad`]) is zero-rated: VAT 0%, whatever
///   the class. The conditions (evidence of export) are not checked, and
///   services supplied abroad (place-of-supply rules) are treated the same.
/// - `special_zone` changes nothing: freeport and customs-site reliefs are
///   not modelled.
/// - `without_payment` on a UK supply is an error: a UK supply carries VAT
///   or is zero-rated or exempt by its class.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    known_nation(&supply.supplier_region)?;
    match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return vat(Decimal::ZERO, line.taxable_value),
        PlaceOfSupply::Region(code) => known_nation(code)?,
    }
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    if mechanism(&line.tax_class).0 != Mechanism::Normal {
        // The invoice carries no VAT: under a reverse charge the customer
        // accounts for it (Notice 735 §7.6.1 asks the invoice to say so);
        // import VAT is paid at the border or postponed.
        return Ok(LineTax::default());
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    vat(rate, line.taxable_value)
}

/// Longest invoice number accepted. No UK limit is published (reg 14(1)(a)
/// only asks for "a sequential number based on one or more series which
/// uniquely identifies the document" [PRIMARY]). 35 is a KhataOS choice, as
/// for the UAE: the UN/EDIFACT document-number length (data element 1004),
/// which keeps the number usable by trading partners and on one line.
pub const MAX_INVOICE_NUMBER_LEN: usize = 35;

/// What an invoice number may contain. A KhataOS choice, not a legal rule:
/// letters and digits with "-" and "/" as separators. Spaces are refused:
/// "INV 1" and "INV1" would look like different numbers.
pub const ALLOWED_CHARACTERS: &str = "letters, digits, hyphen (-) and slash (/)";

/// VAT invoice and credit note numbers (reg 14(1)(a)): not empty, at most
/// [`MAX_INVOICE_NUMBER_LEN`] characters of [`ALLOWED_CHARACTERS`], and
/// unique among every number already issued in the series, whatever its
/// year: the rule asks for a number that "uniquely identifies the
/// document", with no yearly reset. A business may run several series
/// ("one or more series"); KhataOS numbers each voucher type in its own
/// series, gap-free and in order, so "sequential" holds by construction.
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
    fn mechanisms_come_from_class_prefixes() {
        assert_eq!(mechanism("RC:STANDARD"), (Mechanism::ReverseCharge, "STANDARD".into()));
        assert_eq!(mechanism("drc: reduced"), (Mechanism::DomesticReverseCharge, "REDUCED".into()));
        assert_eq!(mechanism("DRC"), (Mechanism::DomesticReverseCharge, "STANDARD".into()));
        assert_eq!(mechanism("IMPORT:"), (Mechanism::Import, "STANDARD".into()));
        assert_eq!(mechanism("import:pva"), (Mechanism::PostponedImport, "STANDARD".into()));
        assert_eq!(mechanism("PVA"), (Mechanism::PostponedImport, "STANDARD".into()));
        assert_eq!(mechanism("IMPORT:PVA:REDUCED"), (Mechanism::PostponedImport, "REDUCED".into()));
        assert_eq!(mechanism("ZERO"), (Mechanism::Normal, "ZERO".into()));
        assert!(Mechanism::DomesticReverseCharge.self_accounted() && !Mechanism::Import.self_accounted());
        for class in ["DRC:STANDARD", "RC:STANDARD", "IMPORT:STANDARD"] {
            let t = line_tax(&line(class, dec!(1000), supply("ENG", Some("ENG"))), &default_rates()).unwrap();
            assert!(t.components.is_empty(), "{class}: no VAT on the invoice");
        }
    }

    #[test]
    fn default_rates_from_4_january_2011() {
        let r = default_rates();
        assert_eq!(r.rate(STANDARD_CLASS, d(2011, 1, 3)), None, "17.5% before: not seeded");
        assert_eq!(r.rate(STANDARD_CLASS, d(2011, 1, 4)), Some(dec!(20)));
        assert_eq!(r.rate(REDUCED_CLASS, d(2026, 1, 1)), Some(dec!(5)));
        assert_eq!(r.rate(ZERO_RATED_CLASS, d(2026, 1, 1)), Some(dec!(0)));
        assert_eq!(r.rate(EXEMPT_CLASS, d(2026, 1, 1)), Some(dec!(0)));
        assert_eq!((STANDARD_PERCENT, REDUCED_PERCENT), (dec!(20), dec!(5)));
    }

    #[test]
    fn standard_and_reduced_lines_carry_one_vat_component() {
        let rates = default_rates();
        let t = line_tax(&line("STANDARD", dec!(1000), supply("ENG", Some("SCT"))), &rates).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(20), dec!(200))]);
        let t = line_tax(&line("reduced", dec!(1000), supply("WLS", Some("nir"))), &rates).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(5), dec!(50))], "the nation never changes the rate");
    }

    #[test]
    fn zero_rated_and_exempt_classes_are_zero_percent() {
        for class in ["ZERO", "EXEMPT", "exempt-finance"] {
            let mut rates = default_rates();
            rates.set("EXEMPT-FINANCE", d(2011, 1, 4), dec!(0)).unwrap();
            let t = line_tax(&line(class, dec!(1000), supply("ENG", Some("ENG"))), &rates).unwrap();
            assert_eq!(split(&t), [(VAT, dec!(0), dec!(0))], "{class}");
        }
    }

    #[test]
    fn exports_are_zero_rated_whatever_the_class() {
        let t = line_tax(&line("STANDARD", dec!(1000), supply("ENG", None)), &default_rates()).unwrap();
        assert_eq!(split(&t), [(VAT, dec!(0), dec!(0))]);
        let t = line_tax(&line("NO-SUCH-CLASS", dec!(1000), supply("NIR", None)), &default_rates()).unwrap();
        assert_eq!(t.total(), dec!(0), "an export needs no rate on file");
    }

    #[test]
    fn uk_supplies_cannot_skip_vat_and_zones_change_nothing() {
        let mut s = supply("ENG", Some("ENG"));
        s.special_zone = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(10), s.clone()), &default_rates()).unwrap().total(), dec!(2));
        s.without_payment = true;
        assert_eq!(line_tax(&line("STANDARD", dec!(10), s), &default_rates()), Err(TaxError::NotZeroRated));
    }

    #[test]
    fn rates_follow_the_invoice_date_and_must_exist() {
        let mut l = line("STANDARD", dec!(100), supply("ENG", Some("ENG")));
        l.date = d(2010, 12, 31);
        assert_eq!(line_tax(&l, &default_rates()), Err(TaxError::NoRate { class: "STANDARD".into(), date: d(2010, 12, 31) }));
        let mut rates = default_rates();
        rates.set("STANDARD", d(2008, 12, 1), dec!(15)).unwrap();
        rates.set("STANDARD", d(2010, 1, 1), dec!(17.5)).unwrap();
        assert_eq!(line_tax(&l, &rates).unwrap().total(), dec!(17.50), "history kept when entered");
        l.tax_class = "8471".into();
        assert!(matches!(line_tax(&l, &default_rates()), Err(TaxError::NoRate { .. })));
    }

    #[test]
    fn unknown_nations_and_overflow_are_errors() {
        let bad = |code: &str| TaxError::UnknownRegion { kind: "UK nation code", code: code.into() };
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("LDN", Some("ENG"))), &default_rates()), Err(bad("LDN")));
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("ENG", Some("GB-SCT"))), &default_rates()), Err(bad("GB-SCT")));
        assert_eq!(line_tax(&line("STANDARD", dec!(1), supply("", None)), &default_rates()), Err(bad("")));
        assert_eq!(line_tax(&line("STANDARD", Amount::MAX, supply("ENG", Some("ENG"))), &default_rates()), Err(TaxError::Overflow));
    }

    #[test]
    fn lines_round_to_the_nearest_penny_and_invoices_may_round_down() {
        let rates = default_rates();
        let vat_on = |v: Decimal| line_tax(&line("STANDARD", v, supply("ENG", Some("ENG"))), &rates).unwrap().total();
        assert_eq!(vat_on(dec!(0.025)), dec!(0.01), "0.005 → 0.01");
        assert_eq!(vat_on(dec!(-0.025)), dec!(-0.01), "credit notes mirror");
        assert_eq!(vat_on(dec!(0.02)), dec!(0), "0.004 → 0.00");
        assert_eq!(vat_on(dec!(1999.99)), dec!(400.00), "399.998 → 400.00");
        // Invoice level: 399.998 rounds down to 399.99 (§17.5).
        assert_eq!(document_vat(&[(dec!(1999.99), dec!(20))]), Ok(dec!(399.99)));
        assert_eq!(document_vat(&[(dec!(-1999.99), dec!(20))]), Ok(dec!(-399.99)));
        assert_eq!(document_vat(&[(dec!(0.07), dec!(20)), (dec!(0.07), dec!(5)), (dec!(9), dec!(0))]), Ok(dec!(0.01)));
        assert_eq!(document_vat(&[(Amount::MAX, dec!(20)), (Amount::MAX, dec!(20))]), Err(TaxError::Overflow));
    }

    #[test]
    fn treatments_come_from_class_names_and_rates() {
        assert_eq!(treatment("STANDARD", dec!(20)), Treatment::Standard);
        assert_eq!(treatment("8471", dec!(20)), Treatment::Standard);
        assert_eq!(treatment("Reduced fuel", dec!(5)), Treatment::Reduced);
        assert_eq!(treatment("2716", dec!(5)), Treatment::Standard, "a 5% rate alone does not make it reduced");
        assert_eq!(treatment("ZERO", dec!(0)), Treatment::ZeroRated);
        assert_eq!(treatment("REDUCED", dec!(0)), Treatment::ZeroRated, "a 0% rate is zero-rated");
        assert_eq!(treatment(" exempt insurance ", dec!(0)), Treatment::Exempt);
        assert_eq!(treatment("EXEMPT", dec!(20)), Treatment::Exempt, "the name wins over a wrong rate");
        assert!(Treatment::Reduced.taxed() && Treatment::Standard.taxed() && !Treatment::ZeroRated.taxed());
    }

    fn april(date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, 4)
    }

    #[test]
    fn invoice_numbers_follow_the_kept_rules() {
        let ok = |n: &str| validate_invoice_number(n, &[], april);
        assert_eq!(ok("1"), Ok(()));
        assert_eq!(ok("2026-27/1"), Ok(()));
        assert_eq!(ok(&"A".repeat(35)), Ok(()));
        let long = "A".repeat(36);
        assert_eq!(ok(&long), Err(InvoiceNumberError::TooLong { number: long.clone(), max: 35, found: 36 }));
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["INV 1", "INV#1", "INV_1", "INV.1", "£1"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
    }

    #[test]
    fn invoice_numbers_are_unique_across_years() {
        let issued = [IssuedDocument { number: "INV-1", date: d(2025, 3, 31) }];
        assert_eq!(
            validate_invoice_number("inv-1", &issued, april),
            Err(InvoiceNumberError::Duplicate { number: "inv-1".into(), year: "2024-25".into() })
        );
        assert_eq!(validate_invoice_number("INV-2", &issued, april), Ok(()));
    }
}

//! Place of supply, the CGST/SGST/UTGST/IGST split, and invoice numbers.

use khata_compliance::{
    FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, PlaceOfSupply, RateSchedule, Supply, TaxComponent,
    TaxError, TaxLine,
};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::gstin;
use crate::states::{self, Kind};

pub const CGST: &str = "CGST";
pub const SGST: &str = "SGST";
pub const UTGST: &str = "UTGST";
pub const IGST: &str = "IGST";

/// Rule 46(b): "not exceeding sixteen characters".
pub const MAX_INVOICE_NUMBER_LEN: usize = 16;

/// Which law a supply is taxed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nature {
    /// CGST + SGST, or CGST + UTGST in a union territory without a legislature.
    IntraState { union_territory: bool },
    /// IGST.
    InterState,
}

/// The default place of supply for a party (IGST Act ss.10 and 12): the
/// state in the recipient's GSTIN when registered, else the state on record,
/// else the supplier's own state (over-the-counter supplies, and services to
/// an unregistered person with no address on record). An invoice can
/// override it; the caller stores the result on the invoice.
pub fn default_place_of_supply(recipient_gstin: Option<&str>, recipient_state: Option<&str>, supplier_state: &str) -> String {
    recipient_gstin
        .and_then(|g| gstin::validate(g).ok())
        .and_then(|d| d.region)
        .map(|r| r.code)
        .or(recipient_state)
        .unwrap_or(supplier_state)
        .to_owned()
}

fn known_state(code: &str) -> Result<&'static states::State, TaxError> {
    states::state(code)
        .filter(|s| s.kind != Kind::CentreJurisdiction)
        .ok_or_else(|| TaxError::UnknownRegion { kind: "GST state code", code: code.to_owned() })
}

/// Intra-State or inter-State (IGST Act ss.7, 8). Supplies to or by an SEZ
/// unit or developer, and exports, are always inter-State.
pub fn nature(supply: &Supply) -> Result<Nature, TaxError> {
    let from = known_state(&supply.supplier_region)?;
    let to = match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return Ok(Nature::InterState),
        PlaceOfSupply::Region(code) => known_state(code)?,
    };
    if supply.special_zone || from.code != to.code {
        return Ok(Nature::InterState);
    }
    Ok(Nature::IntraState { union_territory: to.kind == Kind::UnionTerritory })
}

fn component(tax: &'static str, percent: Decimal, value: Amount) -> Result<TaxComponent, TaxError> {
    let amount = value
        .checked_mul(percent)
        .and_then(|a| a.checked_div(Decimal::ONE_HUNDRED))
        .ok_or(TaxError::Overflow)?
        .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    Ok(TaxComponent { tax, percent, amount })
}

/// Tax on one line. The schedule holds the full GST rate (the IGST rate);
/// an intra-State supply splits it in half between CGST and SGST/UTGST. Each
/// component is rounded to the paisa, half away from zero.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let nature = nature(&line.supply)?;
    let zero_rated = line.supply.special_zone || line.supply.place_of_supply == PlaceOfSupply::Abroad;
    if line.supply.without_payment {
        return if zero_rated { Ok(LineTax::default()) } else { Err(TaxError::NotZeroRated) };
    }
    let rate = rates
        .rate(&line.tax_class, line.date)
        .ok_or_else(|| TaxError::NoRate { class: line.tax_class.clone(), date: line.date })?;
    let value = line.taxable_value;
    let components = match nature {
        Nature::InterState => vec![component(IGST, rate, value)?],
        Nature::IntraState { union_territory } => {
            let half = rate / Decimal::TWO;
            let local = if union_territory { UTGST } else { SGST };
            vec![component(CGST, half, value)?, component(local, half, value)?]
        }
    };
    Ok(LineTax { components })
}

/// Rule 46(b) (and Rule 53 for credit and debit notes): at most 16
/// characters, only letters, digits, hyphen and slash, unique within the
/// financial year. Uniqueness ignores case, as the GST portal does.
pub fn validate_invoice_number(
    number: &str,
    year: FinancialYear,
    issued: &[IssuedDocument<'_>],
) -> Result<(), InvoiceNumberError> {
    if number.is_empty() {
        return Err(InvoiceNumberError::Empty);
    }
    if let Some(found) = number.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '/')) {
        return Err(InvoiceNumberError::InvalidCharacter {
            number: number.to_owned(),
            found,
            allowed: "letters, digits, hyphen (-) and slash (/)",
        });
    }
    if number.len() > MAX_INVOICE_NUMBER_LEN {
        return Err(InvoiceNumberError::TooLong {
            number: number.to_owned(),
            max: MAX_INVOICE_NUMBER_LEN,
            found: number.len(),
        });
    }
    if issued.iter().any(|d| year.contains(d.date) && d.number.eq_ignore_ascii_case(number)) {
        return Err(InvoiceNumberError::Duplicate { number: number.to_owned(), year: year.label() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
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

    fn line(value: Amount, supply: Supply) -> TaxLine {
        TaxLine { date: d(2026, 5, 1), tax_class: "8471".into(), taxable_value: value, supply }
    }

    fn rates() -> RateSchedule {
        let mut r = RateSchedule::default();
        r.set("8471", d(2017, 7, 1), dec!(18)).unwrap();
        r.set("6109", d(2017, 7, 1), dec!(12)).unwrap();
        r.set("6109", d(2025, 9, 22), dec!(5)).unwrap();
        r
    }

    fn split(t: &LineTax) -> Vec<(&'static str, Decimal, Amount)> {
        t.components.iter().map(|c| (c.tax, c.percent, c.amount)).collect()
    }

    #[test]
    fn place_of_supply_defaults() {
        assert_eq!(default_place_of_supply(Some("29AAGCB7383J1Z4"), Some("27"), "27"), "29", "GSTIN state wins");
        assert_eq!(default_place_of_supply(None, Some("33"), "27"), "33", "unregistered: state on record");
        assert_eq!(default_place_of_supply(None, None, "27"), "27", "nothing known: supplier's state");
        assert_eq!(default_place_of_supply(Some("29AAGCB7383J1Z5"), Some("33"), "27"), "33", "an invalid GSTIN is not trusted");
    }

    #[test]
    fn same_state_is_cgst_plus_sgst() {
        let t = line_tax(&line(dec!(10000), supply("27", Some("27"))), &rates()).unwrap();
        assert_eq!(split(&t), [(CGST, dec!(9), dec!(900)), (SGST, dec!(9), dec!(900))]);
        assert_eq!(t.total(), dec!(1800));
    }

    #[test]
    fn union_territories_without_a_legislature_use_utgst() {
        for ut in ["04", "26", "31", "35", "38", "97"] {
            let t = line_tax(&line(dec!(100), supply(ut, Some(ut))), &rates()).unwrap();
            assert_eq!(split(&t), [(CGST, dec!(9), dec!(9)), (UTGST, dec!(9), dec!(9))], "{ut}");
        }
        for with_legislature in ["07", "34", "01"] {
            let t = line_tax(&line(dec!(100), supply(with_legislature, Some(with_legislature))), &rates()).unwrap();
            assert_eq!(t.components[1].tax, SGST, "{with_legislature}");
        }
    }

    #[test]
    fn different_states_are_igst() {
        let t = line_tax(&line(dec!(10000), supply("27", Some("29"))), &rates()).unwrap();
        assert_eq!(split(&t), [(IGST, dec!(18), dec!(1800))]);
        // A UT on either side doesn't change that.
        let t = line_tax(&line(dec!(100), supply("04", Some("03"))), &rates()).unwrap();
        assert_eq!(split(&t), [(IGST, dec!(18), dec!(18))]);
    }

    #[test]
    fn sez_supplies_are_inter_state_even_within_a_state() {
        let mut s = supply("27", Some("27"));
        s.special_zone = true;
        assert_eq!(nature(&s), Ok(Nature::InterState));
        assert_eq!(split(&line_tax(&line(dec!(1000), s.clone()), &rates()).unwrap()), [(IGST, dec!(18), dec!(180))]);
        s.without_payment = true;
        assert_eq!(line_tax(&line(dec!(1000), s), &rates()).unwrap(), LineTax::default(), "under LUT: no tax");
    }

    #[test]
    fn exports_pay_igst_or_go_under_lut() {
        let mut s = supply("27", None);
        assert_eq!(split(&line_tax(&line(dec!(1000), s.clone()), &rates()).unwrap()), [(IGST, dec!(18), dec!(180))]);
        s.without_payment = true;
        assert!(line_tax(&line(dec!(1000), s), &rates()).unwrap().components.is_empty());
    }

    #[test]
    fn domestic_supplies_cannot_skip_tax() {
        let mut s = supply("27", Some("29"));
        s.without_payment = true;
        assert_eq!(line_tax(&line(dec!(1000), s), &rates()), Err(TaxError::NotZeroRated));
    }

    #[test]
    fn rates_follow_the_invoice_date() {
        let mut l = line(dec!(1000), supply("27", Some("27")));
        l.tax_class = "61091000".into();
        l.date = d(2025, 9, 21);
        assert_eq!(line_tax(&l, &rates()).unwrap().total(), dec!(120), "12% before GST 2.0");
        l.date = d(2025, 9, 22);
        let t = line_tax(&l, &rates()).unwrap();
        assert_eq!(split(&t), [(CGST, dec!(2.5), dec!(25)), (SGST, dec!(2.5), dec!(25))], "5% from 22 Sep 2025");
        l.tax_class = "9983".into();
        assert_eq!(line_tax(&l, &rates()), Err(TaxError::NoRate { class: "9983".into(), date: d(2025, 9, 22) }));
    }

    #[test]
    fn each_component_rounds_to_the_paisa() {
        // 18% of 999.99 = 179.9982; 9% = 89.9991 → 90.00 each half.
        let t = line_tax(&line(dec!(999.99), supply("27", Some("27"))), &rates()).unwrap();
        assert_eq!(split(&t), [(CGST, dec!(9), dec!(90.00)), (SGST, dec!(9), dec!(90.00))]);
        let t = line_tax(&line(dec!(999.99), supply("27", Some("29"))), &rates()).unwrap();
        assert_eq!(t.total(), dec!(180.00));
        // Exactly half a paisa rounds away from zero, on credit notes too.
        let t = line_tax(&line(dec!(0.25), supply("27", Some("29"))), &rates()).unwrap();
        assert_eq!(t.total(), dec!(0.05), "0.045 → 0.05");
        let t = line_tax(&line(dec!(-0.25), supply("27", Some("29"))), &rates()).unwrap();
        assert_eq!(t.total(), dec!(-0.05));
    }

    #[test]
    fn unknown_states_and_overflow_are_errors() {
        let bad = TaxError::UnknownRegion { kind: "GST state code", code: "40".into() };
        assert_eq!(line_tax(&line(dec!(1), supply("27", Some("40"))), &rates()), Err(bad.clone()));
        assert_eq!(nature(&supply("40", Some("27"))), Err(bad));
        assert!(matches!(nature(&supply("99", Some("27"))), Err(TaxError::UnknownRegion { .. })), "99 is not a place");
        assert_eq!(line_tax(&line(Amount::MAX, supply("27", Some("29"))), &rates()), Err(TaxError::Overflow));
    }

    fn fy(y: i32) -> FinancialYear {
        FinancialYear::containing(d(y, 4, 1), 4)
    }

    #[test]
    fn invoice_numbers_follow_rule_46() {
        let ok = |n: &str| validate_invoice_number(n, fy(2026), &[]);
        assert_eq!(ok("1"), Ok(()));
        assert_eq!(ok("INV/2026-27/0001"), Ok(()), "16 characters, slash and hyphen");
        assert_eq!(
            ok("INV/2026-27/00001"),
            Err(InvoiceNumberError::TooLong { number: "INV/2026-27/00001".into(), max: 16, found: 17 })
        );
        assert_eq!(ok(""), Err(InvoiceNumberError::Empty));
        for bad in ["INV 1", "INV#1", "INV_1", "INV.1", "ИНВ1"] {
            assert!(matches!(ok(bad), Err(InvoiceNumberError::InvalidCharacter { .. })), "{bad}");
        }
    }

    #[test]
    fn invoice_numbers_are_unique_per_financial_year() {
        let issued = [
            IssuedDocument { number: "INV-1", date: d(2026, 3, 31) },
            IssuedDocument { number: "INV-2", date: d(2026, 4, 1) },
        ];
        assert_eq!(validate_invoice_number("INV-1", fy(2026), &issued), Ok(()), "INV-1 was last year");
        assert_eq!(
            validate_invoice_number("inv-2", fy(2026), &issued),
            Err(InvoiceNumberError::Duplicate { number: "inv-2".into(), year: "2026-27".into() })
        );
        assert!(validate_invoice_number("INV-1", fy(2025), &issued).is_err());
    }
}

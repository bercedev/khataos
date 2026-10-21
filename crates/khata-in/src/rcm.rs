//! Reverse charge, blocked credit and ITC reversals: the rules, on plain
//! values (the app supplies the documents and the payments).
//!
//! Sources are in `docs/research/india-gst-advanced.md`:
//! - reverse charge: CGST s.9(3), 9(4) (IGST s.5(3), 5(4)); the recipient
//!   issues a self-invoice for supplies from an unregistered supplier
//!   (s.31(3)(f)) **within 30 days of receipt** (Rule 47A, from 1 Nov 2024);
//! - blocked credit: s.17(5). Its tax stays in GSTR-3B 4(A) and is reversed
//!   in 4(B)(1) (Circular 170/02/2022-GST para 4.3(B), (E));
//! - Rule 37: ITC on a forward-charge supply not paid for within 180 days
//!   of the invoice is reversed, proportionate to the amount not paid, in
//!   the GSTR-3B for the tax period after the 180 days, in 4(B)(2)
//!   (Circular 170 para 4.3(C)); it is re-availed on payment;
//! - Rule 42: common credit on inputs and input services split between
//!   taxable and exempt supplies (D1 = E/F × C2, D2 = 5% of C2).
//!
//! **How a ledger or item is flagged** (no change to the core model): its
//! tax class (`tax_class`, the HSN/SAC field) carries a prefix:
//! `RCM:9965` is taxed on reverse charge, `BLOCKED:8703` is a blocked
//! credit, and `RCM:BLOCKED:9964` is both. Prefixes may come in either
//! order; the rate is looked up on the code after them. None of this is
//! legal advice.

use chrono::{Datelike, Duration, NaiveDate};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::returns::Amounts;

/// Tax class prefix: tax on this supply is paid by the recipient.
pub const RCM_PREFIX: &str = "RCM:";
/// Tax class prefix: input tax credit is blocked (CGST s.17(5)).
pub const BLOCKED_PREFIX: &str = "BLOCKED:";

/// Rule 47A: days from receipt of the supply to issue the self-invoice.
pub const SELF_INVOICE_DAYS: i64 = 30;

/// Second proviso to CGST s.16(2), Rule 37: days to pay the supplier.
pub const RULE37_DAYS: i64 = 180;

/// A tax class with its flags taken off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassFlags<'a> {
    /// The HSN/SAC code or tax category the rate is looked up on.
    pub class: &'a str,
    pub reverse_charge: bool,
    pub blocked: bool,
}

/// Split a tax class into its code and flags. Case and spaces are ignored
/// in the prefixes ("rcm: 9965" works); the code keeps its own spelling.
pub fn parse_class(raw: &str) -> ClassFlags<'_> {
    let mut rest = raw.trim_start();
    let mut flags = ClassFlags { class: rest, reverse_charge: false, blocked: false };
    loop {
        let upper = rest.get(..BLOCKED_PREFIX.len().min(rest.len())).unwrap_or_default().to_ascii_uppercase();
        if upper.starts_with(RCM_PREFIX) {
            flags.reverse_charge = true;
            rest = rest[RCM_PREFIX.len()..].trim_start();
        } else if upper.starts_with(BLOCKED_PREFIX) {
            flags.blocked = true;
            rest = rest[BLOCKED_PREFIX.len()..].trim_start();
        } else {
            break;
        }
    }
    flags.class = rest;
    flags
}

/// The tax class with flags put back, in the canonical order
/// (`RCM:BLOCKED:<code>`). An empty code with no flags is empty.
pub fn class_with_flags(class: &str, reverse_charge: bool, blocked: bool) -> String {
    let code = parse_class(class).class;
    let mut out = String::new();
    if reverse_charge {
        out.push_str(RCM_PREFIX);
    }
    if blocked {
        out.push_str(BLOCKED_PREFIX);
    }
    out.push_str(code);
    out
}

/// Rule 47A: the last day to issue a self-invoice for a supply received
/// on `received` (30 days from the date of receipt).
pub fn self_invoice_due(received: NaiveDate) -> NaiveDate {
    received + Duration::days(SELF_INVOICE_DAYS)
}

/// The financial year label "26-27" for a date (April to March).
pub fn fy_label(d: NaiveDate) -> String {
    let start = if d.month() >= 4 { d.year() } else { d.year() - 1 };
    format!("{:02}-{:02}", start.rem_euclid(100), (start + 1).rem_euclid(100))
}

/// A self-invoice number in its own series: "SI/26-27/14". At most 16
/// characters of letters, digits, '/' and '-' (Rule 46(b)) up to n = 99999.
pub fn self_invoice_number(date: NaiveDate, n: u32) -> String {
    format!("SI/{}/{n}", fy_label(date))
}

/// One forward-charge purchase for the Rule 37 test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpaidPurchase {
    /// The supplier's invoice date (the 180 days run from it).
    pub invoice_date: NaiveDate,
    /// Invoice value: taxable value plus tax.
    pub value: Decimal,
    /// What is still unpaid on `as_at`.
    pub unpaid: Decimal,
    /// ITC availed on the invoice.
    pub itc: Amounts,
}

/// The ITC to reverse under Rule 37 for one purchase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rule37Reversal {
    /// The last of the 180 days.
    pub period_ends: NaiveDate,
    /// The first day after the 180 days. The reversal goes in the GSTR-3B
    /// for the tax period after the 180 days (Rule 37(1)); this is taken as
    /// the return for the month that contains this day [UNCERTAIN].
    pub reverse_from: NaiveDate,
    /// Unpaid share of the invoice value, 0 to 1.
    pub share: Decimal,
    /// ITC to reverse, by tax ("proportionate to the amount not paid").
    pub reversal: Amounts,
}

/// Rule 37 (as substituted by Notif. 19/2022-CT and amended by 26/2022-CT,
/// w.e.f. 1 Oct 2022): when more than 180 days have passed since the
/// invoice on `as_at` and something is unpaid, the ITC proportionate to
/// the unpaid amount is reversed. `None`: nothing to reverse (paid, within
/// 180 days, or no value). Not for reverse-charge supplies; the caller
/// leaves those out.
pub fn rule37(p: &UnpaidPurchase, as_at: NaiveDate) -> Option<Rule37Reversal> {
    let period_ends = p.invoice_date + Duration::days(RULE37_DAYS);
    if as_at <= period_ends || p.unpaid <= Decimal::ZERO || p.value <= Decimal::ZERO {
        return None;
    }
    let share = (p.unpaid / p.value).min(Decimal::ONE);
    let r = |a: Decimal| (a * share).round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let reversal = Amounts { taxable: r(p.itc.taxable), igst: r(p.itc.igst), cgst: r(p.itc.cgst), sgst: r(p.itc.sgst), cess: r(p.itc.cess) };
    if reversal.tax().is_zero() {
        return None;
    }
    Some(Rule37Reversal { period_ends, reverse_from: period_ends + Duration::days(1), share, reversal })
}

/// Rule 42(1) inputs for one tax (IGST, CGST, SGST/UTGST or cess are
/// worked out separately, clause (l)).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Rule42Input {
    /// T: total input tax on inputs and input services in the period.
    pub total: Decimal,
    /// T1: used exclusively for non-business purposes.
    pub non_business: Decimal,
    /// T2: used exclusively for exempt supplies.
    pub exempt_only: Decimal,
    /// T3: blocked under s.17(5).
    pub blocked: Decimal,
    /// T4: used exclusively for taxable (including zero-rated) supplies.
    pub taxable_only: Decimal,
    /// E: value of exempt supplies in the period.
    pub exempt_turnover: Decimal,
    /// F: total turnover in the State in the period.
    pub total_turnover: Decimal,
    /// Common inputs are used partly for non-business purposes (D2 applies).
    pub non_business_use: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rule42 {
    /// C1 = T − (T1 + T2 + T3): credited to the electronic credit ledger.
    pub c1: Decimal,
    /// C2 = C1 − T4: common credit.
    pub c2: Decimal,
    /// D1 = (E ÷ F) × C2: attributable to exempt supplies.
    pub d1: Decimal,
    /// D2 = 5% of C2 when common inputs are partly for non-business use.
    pub d2: Decimal,
    /// C3 = C2 − (D1 + D2): eligible common credit.
    pub c3: Decimal,
    /// D1 + D2: reversed in GSTR-3B 4(B)(1) (Circular 170 para 4.3(B)).
    pub reverse: Decimal,
}

/// Rule 42(1) for one tax. The annual true-up (Rule 42(2)) and the
/// real-estate variant are not covered. E/F with no turnover in the
/// period uses the last period with turnover (proviso to clause (i)); the
/// caller supplies those figures. Amounts are rounded to the paisa.
pub fn rule42(i: &Rule42Input) -> Result<Rule42, &'static str> {
    if i.total_turnover <= Decimal::ZERO {
        return Err("total turnover (F) must be more than zero; use the last period with turnover");
    }
    if i.exempt_turnover < Decimal::ZERO || i.exempt_turnover > i.total_turnover {
        return Err("exempt turnover (E) must be between zero and the total turnover (F)");
    }
    let r = |a: Decimal| a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let c1 = i.total - (i.non_business + i.exempt_only + i.blocked);
    let c2 = c1 - i.taxable_only;
    let d1 = r(i.exempt_turnover / i.total_turnover * c2);
    let d2 = if i.non_business_use { r(c2 * Decimal::new(5, 2)) } else { Decimal::ZERO };
    Ok(Rule42 { c1, c2, d1, d2, c3: c2 - (d1 + d2), reverse: d1 + d2 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn classes_carry_flags_in_any_order() {
        assert_eq!(parse_class("9965"), ClassFlags { class: "9965", reverse_charge: false, blocked: false });
        assert_eq!(parse_class("RCM:9965"), ClassFlags { class: "9965", reverse_charge: true, blocked: false });
        assert_eq!(parse_class("blocked: rcm:996601"), ClassFlags { class: "996601", reverse_charge: true, blocked: true });
        assert_eq!(parse_class("BLOCKED:"), ClassFlags { class: "", reverse_charge: false, blocked: true });
        assert_eq!(class_with_flags("BLOCKED:RCM:9965", true, true), "RCM:BLOCKED:9965");
        assert_eq!(class_with_flags("RCM:9965", false, false), "9965");
        assert!(class_with_flags("99654321", true, true).len() <= 24, "fits the 24-character tax class");
    }

    #[test]
    fn self_invoices_are_due_in_30_days_and_numbered_by_year() {
        assert_eq!(self_invoice_due(d(2026, 5, 10)), d(2026, 6, 9));
        assert_eq!(self_invoice_number(d(2026, 5, 10), 7), "SI/26-27/7");
        assert_eq!(self_invoice_number(d(2027, 3, 31), 99999), "SI/26-27/99999");
        assert!(self_invoice_number(d(2027, 3, 31), 99999).len() <= crate::gst::MAX_INVOICE_NUMBER_LEN);
        assert_eq!(fy_label(d(2000, 1, 1)), "99-00");
    }

    #[test]
    fn rule37_reverses_the_unpaid_share_after_180_days() {
        let p = UnpaidPurchase {
            invoice_date: d(2026, 1, 1),
            value: dec!(11800),
            unpaid: dec!(5900),
            itc: Amounts { taxable: dec!(10000), igst: dec!(1800), ..Amounts::default() },
        };
        assert_eq!(rule37(&p, d(2026, 6, 30)), None, "day 180 is still in time");
        let r = rule37(&p, d(2026, 7, 1)).unwrap();
        assert_eq!((r.period_ends, r.reverse_from, r.share), (d(2026, 6, 30), d(2026, 7, 1), dec!(0.5)));
        assert_eq!((r.reversal.igst, r.reversal.taxable), (dec!(900), dec!(5000)));
        assert_eq!(rule37(&UnpaidPurchase { unpaid: dec!(0), ..p.clone() }, d(2027, 1, 1)), None);
        let over = rule37(&UnpaidPurchase { unpaid: dec!(20000), ..p }, d(2027, 1, 1)).unwrap();
        assert_eq!(over.reversal.igst, dec!(1800), "never more than the credit");
    }

    #[test]
    fn rule42_splits_common_credit() {
        let r = rule42(&Rule42Input {
            total: dec!(100000),
            non_business: dec!(5000),
            exempt_only: dec!(10000),
            blocked: dec!(5000),
            taxable_only: dec!(30000),
            exempt_turnover: dec!(250000),
            total_turnover: dec!(1000000),
            non_business_use: true,
        })
        .unwrap();
        assert_eq!((r.c1, r.c2, r.d1, r.d2, r.c3, r.reverse), (dec!(80000), dec!(50000), dec!(12500), dec!(2500), dec!(35000), dec!(15000)));
        assert!(rule42(&Rule42Input::default()).is_err());
        assert!(rule42(&Rule42Input { exempt_turnover: dec!(2), total_turnover: dec!(1), ..Rule42Input::default() }).is_err());
    }
}

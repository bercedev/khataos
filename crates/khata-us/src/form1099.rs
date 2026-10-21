//! The Form 1099 vendor list: payments to each payee in a calendar year
//! that reach the reporting threshold, from neutral payment records.
//!
//! Sources: `docs/research/us-compliance.md` §6 (IRC §§ 6041, 6041A; the
//! One Big Beautiful Bill Act, Pub. L. 119-21; the IRS instructions for
//! Forms 1099-MISC and 1099-NEC). In short:
//! - **1099-NEC** box 1 (box 1a from 2026) reports nonemployee compensation (fees,
//!   commissions, services by people who are not employees) of
//!   [`threshold`] or more in the year;
//! - **1099-MISC** box 1 rents and box 3 other income use the same
//!   threshold; box 2 royalties use $10;
//! - the general threshold was **$600** for payments made through
//!   31 Dec 2025 and is **$2,000** for payments made after 31 Dec 2025,
//!   indexed for inflation after 2026 ([`threshold`] returns $2,000 for
//!   2027 on with a warning to check the indexed figure);
//! - the payer collects the payee's TIN on **Form W-9** before paying;
//!   without one, backup withholding applies (24%);
//! - payments to corporations are generally not reported (legal services
//!   are an exception), and payments by card or a
//!   payment app are reported by the processor on Form 1099-K, not by the
//!   payer (third-party networks above $20,000 and 200 transactions, after
//!   the Act's s70432).
//!
//! The Act is s70433 (`$600` → `$2,000` in IRC s6041(a), for payments
//! after 31 Dec 2025; s6041A(a)(2) follows; indexed under new s6041(h)
//! for calendar years after 2026, rounded to $100) [HIGH].
//!
//! KhataOS does not know whether a payee is a corporation or how a
//! payment was made: the list is every payee at or above the threshold,
//! and the user decides who gets a form. Nothing is filed (no IRIS / FIRE
//! file is produced).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Serialize;

/// The 1099 box a payment belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum PaymentKind {
    /// 1099-NEC box 1.
    Nonemployee,
    /// 1099-MISC box 1.
    Rents,
    /// 1099-MISC box 2.
    Royalties,
    /// 1099-MISC box 3.
    OtherIncome,
}

impl PaymentKind {
    /// The form and box for payments made in `year`: the 2026
    /// instructions renamed 1099-NEC box 1 to box 1a (boxes 1b and 13a
    /// are for cash tips) [HIGH: i1099mec Rev. 12/2026].
    pub fn form(self, year: i32) -> &'static str {
        match self {
            PaymentKind::Nonemployee if year >= 2026 => "1099-NEC box 1a",
            PaymentKind::Nonemployee => "1099-NEC box 1",
            PaymentKind::Rents => "1099-MISC box 1",
            PaymentKind::Royalties => "1099-MISC box 2",
            PaymentKind::OtherIncome => "1099-MISC box 3",
        }
    }
}

/// The threshold for a kind of payment made in `year`, and whether the
/// figure needs checking (inflation indexing after 2026 not yet known).
pub fn threshold(kind: PaymentKind, year: i32) -> (Decimal, bool) {
    match kind {
        PaymentKind::Royalties => (Decimal::from(10), false),
        _ if year <= 2025 => (Decimal::from(600), false),
        _ => (Decimal::from(2000), year > 2026),
    }
}

/// One payment to a payee. Negative amounts (refunds from the payee)
/// reduce the total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Payment {
    /// The payee's id in the app (a ledger id).
    pub party_id: String,
    pub party_name: String,
    /// The payee's TIN (EIN, SSN or ITIN) as stored, if any.
    pub tin: Option<String>,
    pub date: NaiveDate,
    pub kind: PaymentKind,
    pub amount: Decimal,
    pub cancelled: bool,
}

/// One payee and box that reaches the threshold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Form1099Row {
    pub party_id: String,
    pub party_name: String,
    pub tin: Option<String>,
    pub kind: PaymentKind,
    pub form: &'static str,
    pub total: Decimal,
    pub payments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Form1099Report {
    pub year: i32,
    /// The general (NEC / rents / other income) threshold used.
    pub threshold: Decimal,
    pub rows: Vec<Form1099Row>,
    /// How many payee/box totals stayed below their threshold.
    pub below: usize,
    pub warnings: Vec<String>,
}

/// A payee/box running total: (name, TIN, total, payments).
type Running = (String, Option<String>, Decimal, usize);

/// Payees whose payments in `year` reach the threshold, by name.
pub fn form1099_report(payments: &[Payment], year: i32) -> Form1099Report {
    let mut totals: BTreeMap<(String, PaymentKind), Running> = BTreeMap::new();
    for p in payments.iter().filter(|p| !p.cancelled && p.date.year() == year) {
        let e = totals.entry((p.party_id.clone(), p.kind)).or_insert_with(|| (p.party_name.clone(), None, Decimal::ZERO, 0));
        e.2 += p.amount;
        e.3 += 1;
        if let Some(t) = p.tin.as_deref().filter(|t| !t.trim().is_empty()) {
            e.1 = Some(t.trim().to_owned());
        }
    }
    let (general, check) = threshold(PaymentKind::Nonemployee, year);
    let mut warnings = Vec::new();
    if check {
        warnings.push(format!(
            "the $2,000 threshold is indexed for inflation after 2026; check the IRS figure for {year}"
        ));
    }
    let mut rows = Vec::new();
    let mut below = 0;
    for ((party_id, kind), (party_name, tin, total, count)) in totals {
        if total < threshold(kind, year).0 {
            below += 1;
            continue;
        }
        if tin.is_none() {
            warnings.push(format!("{party_name}: no TIN on file; request a Form W-9 (backup withholding applies without one)"));
        }
        rows.push(Form1099Row { party_id, party_name, tin, kind, form: kind.form(year), total, payments: count });
    }
    rows.sort_by_key(|r| (r.party_name.to_lowercase(), r.kind));
    if !rows.is_empty() {
        warnings.push(
            "payments to corporations (except for legal services) and payments by card or payment app (Form 1099-K) are generally not reported on 1099-NEC/MISC; KhataOS cannot tell them apart".into(),
        );
    }
    Form1099Report { year, threshold: general, rows, below, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn pay(id: &str, tin: Option<&str>, date: NaiveDate, kind: PaymentKind, amount: Decimal) -> Payment {
        Payment { party_id: id.into(), party_name: format!("Payee {id}"), tin: tin.map(str::to_owned), date, kind, amount, cancelled: false }
    }

    #[test]
    fn thresholds_by_year() {
        assert_eq!(threshold(PaymentKind::Nonemployee, 2025), (dec!(600), false));
        assert_eq!(threshold(PaymentKind::Rents, 2026), (dec!(2000), false));
        assert_eq!(threshold(PaymentKind::Nonemployee, 2027), (dec!(2000), true));
        assert_eq!(threshold(PaymentKind::Royalties, 2026), (dec!(10), false));
    }

    #[test]
    fn payees_at_or_over_the_threshold() {
        let nec = PaymentKind::Nonemployee;
        let mut cancelled = pay("c", Some("12-3456789"), d(2026, 3, 1), nec, dec!(5000));
        cancelled.cancelled = true;
        let payments = vec![
            pay("a", Some("12-3456789"), d(2026, 1, 5), nec, dec!(1500)),
            pay("a", None, d(2026, 6, 5), nec, dec!(500)),
            pay("b", None, d(2026, 2, 1), nec, dec!(1999.99)),
            pay("b", None, d(2025, 12, 31), nec, dec!(800)),
            pay("d", None, d(2026, 2, 1), nec, dec!(2500)),
            pay("d", None, d(2026, 2, 2), PaymentKind::Royalties, dec!(10)),
            cancelled,
        ];
        let r = form1099_report(&payments, 2026);
        let rows: Vec<_> = r.rows.iter().map(|r| (r.party_id.as_str(), r.form, r.total, r.tin.as_deref())).collect();
        assert_eq!(rows, [
            ("a", "1099-NEC box 1a", dec!(2000), Some("12-3456789")),
            ("d", "1099-NEC box 1a", dec!(2500), None),
            ("d", "1099-MISC box 2", dec!(10), None),
        ]);
        assert_eq!((r.threshold, r.below), (dec!(2000), 1));
        assert!(r.warnings.iter().any(|w| w.contains("Payee d: no TIN")));
        // The same payee reached $600 in 2025.
        let r25 = form1099_report(&payments, 2025);
        assert_eq!((r25.rows.len(), r25.rows[0].total, r25.threshold, r25.rows[0].form), (1, dec!(800), dec!(600), "1099-NEC box 1"));
    }
}

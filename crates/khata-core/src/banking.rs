//! Banking: bank reconciliation.
//!
//! Each bank line on a voucher can carry the date the bank cleared it
//! ([`crate::Entry::bank_date`]). Reconciling a bank ledger as at a date
//! starts from the balance as per the company's books and backs out every
//! line the bank had not cleared by then, giving the balance the bank
//! statement should show.

use chrono::NaiveDate;
use serde::Serialize;

use crate::{Amount, Book, LedgerId, VoucherId, VoucherKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankLine {
    pub date: NaiveDate,
    pub voucher: VoucherId,
    pub kind: VoucherKind,
    pub number: String,
    /// The other ledger(s) on the voucher.
    pub particulars: String,
    /// Money into the bank (bank ledger debited).
    pub deposit: Amount,
    /// Money out of the bank (bank ledger credited).
    pub withdrawal: Amount,
    pub bank_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BankReconciliation {
    pub ledger: LedgerId,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Lines dated in `[from, to]`, plus older lines still uncleared at `to`.
    pub lines: Vec<BankLine>,
    /// Balance as per the books at `to`, debit (money in the bank) positive.
    pub balance_per_books: Amount,
    /// Deposits recorded but not cleared by the bank by `to`.
    pub uncleared_deposits: Amount,
    /// Withdrawals (e.g. cheques issued) not cleared by the bank by `to`.
    pub uncleared_withdrawals: Amount,
    /// What the bank statement should show at `to`.
    pub balance_per_bank: Amount,
}

/// Bank Reconciliation Statement for a bank (or cash) ledger.
pub fn bank_reconciliation(book: &Book, ledger: LedgerId, from: NaiveDate, to: NaiveDate) -> Option<BankReconciliation> {
    book.ledger(ledger)?;
    let balance_per_books = book.balances(Some(to)).get(&ledger).copied().unwrap_or_default();
    let mut vs: Vec<_> = book
        .active_vouchers()
        .filter(|v| v.date <= to && v.entries.iter().any(|e| e.ledger == ledger))
        .collect();
    vs.sort_by_key(|v| (v.date, v.id));
    let mut lines = Vec::new();
    let (mut uncleared_deposits, mut uncleared_withdrawals) = (Amount::ZERO, Amount::ZERO);
    for v in vs {
        let mine: Vec<_> = v.entries.iter().filter(|e| e.ledger == ledger).collect();
        let net: Amount = mine.iter().map(|e| e.signed()).sum();
        let bank_date = mine.iter().filter_map(|e| e.bank_date).max();
        let cleared = mine.iter().all(|e| e.bank_date.is_some_and(|d| d <= to));
        if !cleared {
            if net > Amount::ZERO { uncleared_deposits += net } else { uncleared_withdrawals -= net }
        }
        if v.date < from && cleared {
            continue;
        }
        let others: Vec<&str> = v
            .entries
            .iter()
            .filter(|e| e.ledger != ledger)
            .filter_map(|e| book.ledger(e.ledger).map(|l| l.name.as_str()))
            .collect();
        lines.push(BankLine {
            date: v.date,
            voucher: v.id,
            kind: v.kind,
            number: v.number.clone(),
            particulars: match others.as_slice() {
                [one] => (*one).to_owned(),
                [] => String::new(),
                _ => format!("{} (as per details)", others[0]),
            },
            deposit: net.max(Amount::ZERO),
            withdrawal: (-net).max(Amount::ZERO),
            bank_date,
        });
    }
    Some(BankReconciliation {
        ledger,
        from,
        to,
        lines,
        balance_per_books,
        uncleared_deposits,
        uncleared_withdrawals,
        balance_per_bank: balance_per_books - uncleared_deposits + uncleared_withdrawals,
    })
}

/// One line of a bank statement, as imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StatementLine {
    pub date: NaiveDate,
    pub description: String,
    /// Cheque or reference number, when the bank gives one.
    #[serde(default)]
    pub reference: String,
    /// Money into the account.
    pub deposit: Amount,
    /// Money out of the account.
    pub withdrawal: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementMatch {
    /// Index into the statement lines.
    pub line: usize,
    /// The uncleared voucher it matches, if any.
    pub voucher: Option<VoucherId>,
    /// Days between the voucher date and the bank date (bank later: positive).
    pub days_apart: i64,
    /// The statement's reference matches the voucher's number or narration.
    pub reference_matched: bool,
}

/// Days a bank may take to clear a cheque or transfer for a match.
const MATCH_WINDOW_DAYS: i64 = 15;

/// Pair statement lines with uncleared bank lines of the same amount and
/// direction, dated within the window before (or a few days after) the
/// bank date. A matching cheque/reference number wins, then the closest
/// date. Each voucher is used at most once.
pub fn match_statement(book: &Book, ledger: LedgerId, lines: &[StatementLine]) -> Vec<StatementMatch> {
    let mut open: Vec<(VoucherId, NaiveDate, Amount, String)> = book
        .active_vouchers()
        .filter_map(|v| {
            let mine: Vec<_> = v.entries.iter().filter(|e| e.ledger == ledger).collect();
            if mine.is_empty() || mine.iter().all(|e| e.bank_date.is_some()) {
                return None;
            }
            let net: Amount = mine.iter().map(|e| e.signed()).sum();
            Some((v.id, v.date, net, format!("{} {}", v.number, v.narration).to_lowercase()))
        })
        .collect();
    let mut found: Vec<Option<StatementMatch>> = vec![None; lines.len()];
    // First the lines whose reference names a voucher, then the rest, so a
    // cheque number is never taken by an unreferenced line of equal amount.
    for by_ref_only in [true, false] {
        for (i, s) in lines.iter().enumerate() {
            if found[i].is_some() {
                continue;
            }
            let net = s.deposit - s.withdrawal;
            let reference = s.reference.trim().trim_start_matches('0').to_lowercase();
            let best = open
                .iter()
                .enumerate()
                .filter(|(_, (_, date, amount, _))| {
                    let days = (s.date - *date).num_days();
                    *amount == net && !net.is_zero() && (-3..=MATCH_WINDOW_DAYS).contains(&days)
                })
                .map(|(k, (_, date, _, text))| {
                    let by_ref = !reference.is_empty() && text.contains(&reference);
                    (k, by_ref, (s.date - *date).num_days().abs())
                })
                .filter(|(_, by_ref, _)| *by_ref || !by_ref_only)
                .min_by_key(|(_, by_ref, days)| (!by_ref, *days));
            if let Some((k, by_ref, _)) = best {
                let (id, date, ..) = open.remove(k);
                found[i] = Some(StatementMatch { line: i, voucher: Some(id), days_apart: (s.date - date).num_days(), reference_matched: by_ref });
            }
        }
    }
    let out: Vec<StatementMatch> = found
        .into_iter()
        .enumerate()
        .map(|(i, m)| m.unwrap_or(StatementMatch { line: i, voucher: None, days_apart: 0, reference_matched: false }))
        .collect();
    out
}

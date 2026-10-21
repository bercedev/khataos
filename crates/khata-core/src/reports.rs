//! Core reports: Day Book, ledger statement, Profit & Loss, Balance Sheet.
//!
//! Reports are pure functions over a [`Book`]; the UI renders them and
//! drills from each row into its voucher (interaction spec rules R1–R5).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::chart::{GroupId, LedgerId, Nature};
use crate::voucher::{BillKind, DrCr, Voucher, VoucherId, VoucherKind};
use crate::{Amount, Book};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayBookRow {
    pub date: NaiveDate,
    pub voucher: VoucherId,
    pub kind: VoucherKind,
    pub number: String,
    /// The ledger shown in the Particulars column.
    pub particulars: String,
    pub debit: Amount,
    pub credit: Amount,
    /// Listed with zero amounts, as in Tally's Day Book.
    pub cancelled: bool,
}

/// The ledger a voucher is "about": for Payment, the ledger paid; for
/// Receipt, the ledger received from; for Sales, the party debited; and so on.
fn particulars_entry(v: &Voucher) -> Option<LedgerId> {
    let side = match v.kind {
        VoucherKind::Receipt | VoucherKind::Purchase | VoucherKind::CreditNote => DrCr::Cr,
        _ => DrCr::Dr,
    };
    v.entries.iter().find(|e| e.side == side).or(v.entries.first()).map(|e| e.ledger)
}

/// Vouchers dated within `[from, to]`, in date then entry order.
pub fn day_book(book: &Book, from: NaiveDate, to: NaiveDate) -> Vec<DayBookRow> {
    let mut vs: Vec<&Voucher> = book.vouchers().filter(|v| v.date >= from && v.date <= to).collect();
    vs.sort_by_key(|v| (v.date, v.id));
    vs.into_iter()
        .map(|v| {
            let (total, _) = v.totals();
            let particulars = particulars_entry(v)
                .and_then(|l| book.ledger(l))
                .map(|l| l.name.clone())
                .unwrap_or_default();
            let credit_side = matches!(v.kind, VoucherKind::Receipt | VoucherKind::Purchase | VoucherKind::CreditNote);
            let total = if v.cancelled { Amount::ZERO } else { total };
            DayBookRow {
                date: v.date,
                voucher: v.id,
                kind: v.kind,
                number: v.number.clone(),
                particulars,
                debit: if credit_side { Amount::ZERO } else { total },
                credit: if credit_side { total } else { Amount::ZERO },
                cancelled: v.cancelled,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementRow {
    pub date: NaiveDate,
    pub voucher: VoucherId,
    pub kind: VoucherKind,
    pub number: String,
    /// The other ledger(s) on the voucher.
    pub particulars: String,
    pub debit: Amount,
    pub credit: Amount,
    /// Running balance after this row, debit positive.
    pub balance: Amount,
    /// Net foreign amount of this row (debit positive), for a ledger kept
    /// in a foreign currency or a line that carried one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forex: Option<Amount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerStatement {
    pub ledger: LedgerId,
    pub opening: Amount,
    pub rows: Vec<StatementRow>,
    pub closing: Amount,
    /// For a ledger kept in a foreign currency: the currency and its
    /// opening and closing balances in it (debit positive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forex: Option<ForexBalances>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForexBalances {
    pub currency: String,
    pub opening: Amount,
    pub closing: Amount,
}

/// Net foreign amount of a voucher's lines on `ledger` in `currency`, if any carry one.
fn forex_net(v: &Voucher, ledger: LedgerId, currency: Option<&str>) -> Option<Amount> {
    let mut any = false;
    let mut net = Amount::ZERO;
    for e in v.entries.iter().filter(|e| e.ledger == ledger) {
        let Some(fx) = e.forex.as_ref().filter(|f| currency.is_none_or(|c| f.currency == c)) else { continue };
        any = true;
        net += if e.side == DrCr::Dr { fx.amount } else { -fx.amount };
    }
    any.then_some(net)
}

/// Ledger Vouchers report for `[from, to]`.
pub fn ledger_statement(book: &Book, ledger: LedgerId, from: NaiveDate, to: NaiveDate) -> Option<LedgerStatement> {
    let opening = book.ledger(ledger)?.opening
        + book
            .active_vouchers()
            .filter(|v| v.date < from)
            .flat_map(|v| &v.entries)
            .filter(|e| e.ledger == ledger)
            .map(|e| e.signed())
            .sum::<Amount>();
    let mut vs: Vec<&Voucher> = book
        .active_vouchers()
        .filter(|v| v.date >= from && v.date <= to && v.entries.iter().any(|e| e.ledger == ledger))
        .collect();
    vs.sort_by_key(|v| (v.date, v.id));
    let currency = book.ledger(ledger)?.currency.clone();
    let forex_opening: Amount = book
        .active_vouchers()
        .filter(|v| v.date < from)
        .filter_map(|v| forex_net(v, ledger, currency.as_deref()))
        .sum();
    let mut forex_closing = forex_opening;
    let mut balance = opening;
    let rows = vs
        .into_iter()
        .map(|v| {
            let net: Amount = v.entries.iter().filter(|e| e.ledger == ledger).map(|e| e.signed()).sum();
            balance += net;
            let others: Vec<&str> = v
                .entries
                .iter()
                .filter(|e| e.ledger != ledger)
                .filter_map(|e| book.ledger(e.ledger).map(|l| l.name.as_str()))
                .collect();
            let particulars = match others.as_slice() {
                [one] => (*one).to_owned(),
                [] => String::new(),
                _ => format!("{} (as per details)", others[0]),
            };
            let forex = forex_net(v, ledger, currency.as_deref());
            forex_closing += forex.unwrap_or_default();
            StatementRow {
                date: v.date,
                voucher: v.id,
                kind: v.kind,
                number: v.number.clone(),
                particulars,
                debit: net.max(Amount::ZERO),
                credit: (-net).max(Amount::ZERO),
                balance,
                forex,
            }
        })
        .collect();
    let forex = currency.map(|currency| ForexBalances { currency, opening: forex_opening, closing: forex_closing });
    Some(LedgerStatement { ledger, opening, rows, closing: balance, forex })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupTotal {
    pub group: GroupId,
    pub name: String,
    /// Shown positive on its natural side.
    pub amount: Amount,
    /// Part of the trading account (sales, purchases, direct income/expense).
    pub affects_gross_profit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfitAndLoss {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub incomes: Vec<GroupTotal>,
    pub expenses: Vec<GroupTotal>,
    /// Stock on hand at the start of the period (debit side of trading).
    pub opening_stock: Amount,
    /// Stock on hand at the end of the period (credit side of trading).
    pub closing_stock: Amount,
    /// Trading result (sales, purchases, direct income/expense).
    pub gross_profit: Amount,
    /// Negative for a loss.
    pub net_profit: Amount,
}

/// Net movement per ledger from vouchers within `[from, to]`.
fn movements(book: &Book, from: NaiveDate, to: NaiveDate) -> BTreeMap<LedgerId, Amount> {
    let mut net = BTreeMap::new();
    for v in book.active_vouchers().filter(|v| v.date >= from && v.date <= to) {
        for e in &v.entries {
            *net.entry(e.ledger).or_default() += e.signed();
        }
    }
    net
}

/// Totals by primary group for ledgers of the given natures.
fn by_primary_group(book: &Book, balances: &BTreeMap<LedgerId, Amount>, natures: &[Nature]) -> Vec<GroupTotal> {
    let mut totals: BTreeMap<GroupId, Amount> = BTreeMap::new();
    for (&ledger, &bal) in balances {
        let Some(primary) = book.ledger(ledger).and_then(|l| book.primary_group(l.group)) else { continue };
        if natures.contains(&primary.nature) {
            *totals.entry(primary.id).or_default() += bal;
        }
    }
    let mut out: Vec<GroupTotal> = totals
        .into_iter()
        .filter(|(_, amt)| !amt.is_zero())
        .map(|(id, amt)| {
            let g = book.group(id).expect("primary group exists");
            // Debit-natured groups show debit balances positive, others credit.
            let sign = if matches!(g.nature, Nature::Assets | Nature::Expenses) { amt } else { -amt };
            GroupTotal { group: id, name: g.name.clone(), amount: sign, affects_gross_profit: g.affects_gross_profit }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn profit_and_loss(book: &Book, from: NaiveDate, to: NaiveDate) -> ProfitAndLoss {
    let mv = movements(book, from, to);
    let incomes = by_primary_group(book, &mv, &[Nature::Income]);
    let expenses = by_primary_group(book, &mv, &[Nature::Expenses]);
    let trading = |rows: &[GroupTotal]| -> Amount {
        rows.iter()
            .filter(|r| book.group(r.group).is_some_and(|g| g.affects_gross_profit))
            .map(|r| r.amount)
            .sum()
    };
    let opening_stock = crate::inventory::opening_stock(book, from);
    let closing_stock = crate::inventory::closing_stock(book, Some(to));
    let stock = closing_stock - opening_stock;
    let gross_profit = trading(&incomes) - trading(&expenses) + stock;
    let net_profit = incomes.iter().map(|r| r.amount).sum::<Amount>() - expenses.iter().map(|r| r.amount).sum::<Amount>()
        + stock;
    ProfitAndLoss { from, to, incomes, expenses, opening_stock, closing_stock, gross_profit, net_profit }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BalanceSheet {
    pub as_at: NaiveDate,
    pub liabilities: Vec<GroupTotal>,
    pub assets: Vec<GroupTotal>,
    /// Value of stock on hand at `as_at` (weighted average cost).
    pub closing_stock: Amount,
    /// Accumulated profit (negative for loss) up to `as_at`.
    pub profit_and_loss: Amount,
    /// Non-zero when opening balances do not tally; shown on the liabilities side.
    pub opening_difference: Amount,
    pub total_liabilities: Amount,
    pub total_assets: Amount,
}

pub fn balance_sheet(book: &Book, as_at: NaiveDate) -> BalanceSheet {
    let balances = book.balances(Some(as_at));
    let liabilities = by_primary_group(book, &balances, &[Nature::Liabilities]);
    let assets = by_primary_group(book, &balances, &[Nature::Assets]);
    let profit_and_loss = -by_primary_group(book, &balances, &[Nature::Income, Nature::Expenses])
        .iter()
        .map(|r| {
            let g = book.group(r.group).expect("exists");
            if g.nature == Nature::Expenses { r.amount } else { -r.amount }
        })
        .sum::<Amount>();
    // Stock gained since the books began is profit; the opening stock itself
    // is part of the opening balances.
    let closing_stock = crate::inventory::closing_stock(book, Some(as_at));
    let profit_and_loss = profit_and_loss + closing_stock - crate::inventory::opening_stock_value(book);
    let opening_difference = book.opening_difference();
    let total_liabilities =
        liabilities.iter().map(|r| r.amount).sum::<Amount>() + profit_and_loss + opening_difference;
    let total_assets = assets.iter().map(|r| r.amount).sum::<Amount>() + closing_stock;
    BalanceSheet {
        as_at,
        liabilities,
        assets,
        closing_stock,
        profit_and_loss,
        opening_difference,
        total_liabilities,
        total_assets,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutstandingBill {
    pub ledger: LedgerId,
    pub party: String,
    /// Bill reference; "On Account" / "Opening Balance" for unallocated amounts.
    pub reference: String,
    /// Date the bill was raised (None for opening balances).
    pub date: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    /// Signed: positive = receivable (debit), negative = payable (credit).
    pub amount: Amount,
    /// Days past the due date (or bill date when there is no due date).
    pub overdue_days: i64,
    /// The bill in its foreign currency (signed like `amount`), when it
    /// was raised or settled with foreign amounts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forex: Option<(String, Amount)>,
}

/// Whether a ledger sits under Sundry Debtors or Sundry Creditors.
fn is_party(book: &Book, ledger: LedgerId) -> bool {
    let party_groups = [GroupId::default_named("Sundry Debtors"), GroupId::default_named("Sundry Creditors")];
    let mut group = book.ledger(ledger).map(|l| l.group);
    while let Some(id) = group {
        if party_groups.contains(&id) {
            return true;
        }
        group = book.group(id).and_then(|g| g.parent);
    }
    false
}

/// Open bills of party ledgers as at a date, bill by bill.
pub fn outstanding_bills(book: &Book, as_at: NaiveDate) -> Vec<OutstandingBill> {
    type Key = (LedgerId, String);
    let mut bills: BTreeMap<Key, (Option<NaiveDate>, Option<NaiveDate>, Amount)> = BTreeMap::new();
    let mut foreign: BTreeMap<Key, (String, Amount)> = BTreeMap::new();
    let mut add_fx = |key: &Key, currency: &str, signed: Amount| {
        let slot = foreign.entry(key.clone()).or_insert_with(|| (currency.to_owned(), Amount::ZERO));
        if slot.0 == currency {
            slot.1 += signed;
        }
    };
    for l in book.ledgers().filter(|l| (!l.opening.is_zero() || !l.opening_bills.is_empty()) && is_party(book, l.id)) {
        // Opening bills by name; the rest of the opening balance as one line.
        for b in &l.opening_bills {
            let slot = bills.entry((l.id, b.reference.clone())).or_insert((b.date, b.due, Amount::ZERO));
            slot.2 += b.amount;
        }
        let rest = l.opening - l.opening_bills.iter().map(|b| b.amount).sum::<Amount>();
        if !rest.is_zero() {
            bills.insert((l.id, "Opening Balance".into()), (None, None, rest));
        }
    }
    let mut vs: Vec<&Voucher> = book.active_vouchers().filter(|v| v.date <= as_at).collect();
    vs.sort_by_key(|v| (v.date, v.id));
    for v in vs {
        for e in &v.entries {
            if e.bills.is_empty() {
                if is_party(book, e.ledger) {
                    let key = (e.ledger, "On Account".to_owned());
                    if let Some(fx) = &e.forex {
                        add_fx(&key, &fx.currency, if e.side == DrCr::Dr { fx.amount } else { -fx.amount });
                    }
                    let slot = bills.entry(key).or_insert((Some(v.date), None, Amount::ZERO));
                    slot.2 += e.signed();
                }
                continue;
            }
            for b in &e.bills {
                let signed = if e.side == DrCr::Dr { b.amount } else { -b.amount };
                let reference = if b.kind == BillKind::OnAccount { "On Account".to_owned() } else { b.reference.clone() };
                if let (Some(fx), Some(f)) = (&e.forex, b.forex_amount) {
                    add_fx(&(e.ledger, reference.clone()), &fx.currency, if e.side == DrCr::Dr { f } else { -f });
                }
                let slot = bills.entry((e.ledger, reference)).or_insert((Some(v.date), b.due, Amount::ZERO));
                if matches!(b.kind, BillKind::New | BillKind::Advance) && slot.1.is_none() {
                    slot.1 = b.due;
                }
                slot.2 += signed;
            }
        }
    }
    let mut out: Vec<OutstandingBill> = bills
        .into_iter()
        .filter(|(_, (_, _, amt))| !amt.is_zero())
        .map(|((ledger, reference), (date, due, amount))| {
            let from = due.or(date);
            let forex = foreign.remove(&(ledger, reference.clone()));
            OutstandingBill {
                ledger,
                party: book.ledger(ledger).map(|l| l.name.clone()).unwrap_or_default(),
                reference,
                date,
                due,
                amount,
                overdue_days: from.map_or(0, |d| (as_at - d).num_days().max(0)),
                forex,
            }
        })
        .collect();
    out.sort_by(|a, b| (a.party.as_str(), a.date).cmp(&(b.party.as_str(), b.date)));
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgeingRow {
    pub ledger: LedgerId,
    pub party: String,
    /// Outstanding by days overdue: 0–30, 31–60, 61–90, over 90. Shown positive.
    pub buckets: [Amount; 4],
    pub total: Amount,
}

/// Receivables (`receivable = true`) or payables ageing per party.
pub fn ageing(book: &Book, as_at: NaiveDate, receivable: bool) -> Vec<AgeingRow> {
    let mut rows: BTreeMap<LedgerId, AgeingRow> = BTreeMap::new();
    for b in outstanding_bills(book, as_at) {
        let amount = if receivable { b.amount } else { -b.amount };
        if amount <= Amount::ZERO {
            continue;
        }
        let bucket = match b.overdue_days {
            0..=30 => 0,
            31..=60 => 1,
            61..=90 => 2,
            _ => 3,
        };
        let row = rows.entry(b.ledger).or_insert_with(|| AgeingRow {
            ledger: b.ledger,
            party: b.party.clone(),
            buckets: [Amount::ZERO; 4],
            total: Amount::ZERO,
        });
        row.buckets[bucket] += amount;
        row.total += amount;
    }
    let mut out: Vec<AgeingRow> = rows.into_values().collect();
    out.sort_by(|a, b| a.party.cmp(&b.party));
    out
}

/// Payable bills older than `days` from the bill date: candidates for the
/// MSME payment rule (India: s.43B(h) — 15 days, or up to 45 days by
/// written agreement). Which suppliers are MSEs is the caller's filter.
pub fn overdue_payables(book: &Book, as_at: NaiveDate, days: i64) -> Vec<OutstandingBill> {
    outstanding_bills(book, as_at)
        .into_iter()
        .filter(|b| b.amount.is_sign_negative() && b.date.is_some_and(|d| (as_at - d).num_days() > days))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voucher::Entry;
    use chrono::{DateTime, Utc};
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_760_000_000, 0).unwrap()
    }

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 4, day).unwrap()
    }

    /// The same scenario as the TallyPrime study: HDFC opening 2,50,000 Dr
    /// against capital, then rent of 25,000 paid from the bank.
    fn demo() -> (Book, LedgerId, LedgerId, LedgerId, LedgerId, LedgerId) {
        let mut b = Book::new("KhataOS Demo Traders");
        let g = |b: &Book, n: &str| b.group_by_name(n).unwrap().id;
        let bank = b.create_ledger(now(), "u", "HDFC Bank", g(&b, "Bank Accounts")).unwrap();
        let capital = b.create_ledger(now(), "u", "Owner's Capital", g(&b, "Capital Account")).unwrap();
        let rent = b.create_ledger(now(), "u", "Office Rent", g(&b, "Indirect Expenses")).unwrap();
        let sales = b.create_ledger(now(), "u", "Sales", g(&b, "Sales Accounts")).unwrap();
        let party = b.create_ledger(now(), "u", "Sharma Enterprises", g(&b, "Sundry Debtors")).unwrap();
        b.set_opening_balance(now(), "u", bank, dec!(250000)).unwrap();
        b.set_opening_balance(now(), "u", capital, dec!(-250000)).unwrap();
        let mut post = |kind, num: &str, day, entries| {
            b.post_voucher(
                now(),
                "u",
                Voucher { id: VoucherId::new(), kind, number: num.into(), date: d(day), entries, narration: String::new(), cancelled: false, series: None, stock: Vec::new() },
            )
            .unwrap();
        };
        post(VoucherKind::Payment, "1", 1, vec![Entry::dr(rent, dec!(25000)), Entry::cr(bank, dec!(25000))]);
        post(VoucherKind::Sales, "1", 2, vec![Entry::dr(party, dec!(40000)), Entry::cr(sales, dec!(40000))]);
        post(VoucherKind::Receipt, "1", 3, vec![Entry::dr(bank, dec!(15000)), Entry::cr(party, dec!(15000))]);
        (b, bank, capital, rent, sales, party)
    }

    #[test]
    fn day_book_lists_vouchers_with_particulars() {
        let (b, ..) = demo();
        let rows = day_book(&b, d(1), d(30));
        let summary: Vec<_> = rows.iter().map(|r| (r.kind, r.particulars.as_str(), r.debit, r.credit)).collect();
        assert_eq!(
            summary,
            [
                (VoucherKind::Payment, "Office Rent", dec!(25000), dec!(0)),
                (VoucherKind::Sales, "Sharma Enterprises", dec!(40000), dec!(0)),
                (VoucherKind::Receipt, "Sharma Enterprises", dec!(0), dec!(15000)),
            ]
        );
        assert_eq!(day_book(&b, d(1), d(1)).len(), 1);
    }

    #[test]
    fn ledger_statement_runs_balance_from_opening() {
        let (b, bank, ..) = demo();
        let s = ledger_statement(&b, bank, d(1), d(30)).unwrap();
        assert_eq!(s.opening, dec!(250000));
        let bals: Vec<_> = s.rows.iter().map(|r| r.balance).collect();
        assert_eq!(bals, [dec!(225000), dec!(240000)]);
        assert_eq!(s.closing, dec!(240000));
        assert_eq!(s.rows[0].particulars, "Office Rent");
        // Opening for a later window includes earlier vouchers.
        assert_eq!(ledger_statement(&b, bank, d(2), d(30)).unwrap().opening, dec!(225000));
    }

    #[test]
    fn profit_and_loss_and_balance_sheet_tally() {
        let (b, ..) = demo();
        let pl = profit_and_loss(&b, d(1), d(30));
        assert_eq!(pl.gross_profit, dec!(40000));
        assert_eq!(pl.net_profit, dec!(15000));

        let bs = balance_sheet(&b, d(30));
        assert_eq!(bs.profit_and_loss, dec!(15000));
        assert_eq!(bs.opening_difference, dec!(0));
        assert_eq!(bs.total_assets, dec!(265000)); // bank 2,40,000 + debtor 25,000
        assert_eq!(bs.total_liabilities, bs.total_assets);
    }

    #[test]
    fn opening_difference_keeps_balance_sheet_balanced() {
        let (mut b, _, capital, ..) = demo();
        b.set_opening_balance(now(), "u", capital, dec!(-200000)).unwrap();
        let bs = balance_sheet(&b, d(30));
        assert_eq!(bs.opening_difference, dec!(50000));
        assert_eq!(bs.total_liabilities, bs.total_assets);
    }

    #[test]
    fn replay_rebuilds_identical_books() {
        let (b, bank, ..) = demo();
        let replayed = Book::replay(b.name.clone(), b.audit().clone()).unwrap();
        assert_eq!(replayed.trial_balance(), b.trial_balance());
        assert_eq!(replayed.ledger(bank).unwrap().opening, dec!(250000));
        assert_eq!(replayed.vouchers().count(), 3);
        assert_eq!(replayed.next_voucher_number(VoucherKind::Sales, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()), "2");
    }
    #[test]
    fn bill_wise_outstanding_ageing_and_msme() {
        use crate::voucher::BillKind;
        let mut b = Book::new("Demo");
        let g = |b: &Book, n: &str| b.group_by_name(n).unwrap().id;
        let bank = b.create_ledger(now(), "u", "Bank", g(&b, "Bank Accounts")).unwrap();
        let sales = b.create_ledger(now(), "u", "Sales", g(&b, "Sales Accounts")).unwrap();
        let purchase = b.create_ledger(now(), "u", "Purchase", g(&b, "Purchase Accounts")).unwrap();
        let debtor = b.create_ledger(now(), "u", "Sharma Enterprises", g(&b, "Sundry Debtors")).unwrap();
        let supplier = b.create_ledger(now(), "u", "Gupta Steel (MSE)", g(&b, "Sundry Creditors")).unwrap();
        let date = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let mut post = |kind, num: &str, on, entries| {
            b.post_voucher(now(), "u", Voucher { id: VoucherId::new(), kind, number: num.into(), date: on, entries, narration: String::new(), cancelled: false, series: None, stock: Vec::new() })
                .unwrap();
        };
        // Two invoices to the debtor, 30-day credit; one part-paid.
        post(VoucherKind::Sales, "S1", date(4, 1), vec![
            Entry::dr(debtor, dec!(10000)).bill(BillKind::New, "S1", dec!(10000), Some(date(5, 1))),
            Entry::cr(sales, dec!(10000)),
        ]);
        post(VoucherKind::Sales, "S2", date(6, 1), vec![
            Entry::dr(debtor, dec!(5000)).bill(BillKind::New, "S2", dec!(5000), Some(date(7, 1))),
            Entry::cr(sales, dec!(5000)),
        ]);
        post(VoucherKind::Receipt, "R1", date(6, 10), vec![
            Entry::dr(bank, dec!(7000)),
            Entry::cr(debtor, dec!(7000))
                .bill(BillKind::Against, "S1", dec!(6000), None)
                .bill(BillKind::OnAccount, "", dec!(1000), None),
        ]);
        // A supplier bill from an MSE, unpaid.
        post(VoucherKind::Purchase, "P1", date(5, 1), vec![
            Entry::dr(purchase, dec!(20000)),
            Entry::cr(supplier, dec!(20000)).bill(BillKind::New, "GS/101", dec!(20000), None),
        ]);

        let as_at = date(7, 15);
        let open = outstanding_bills(&b, as_at);
        let summary: Vec<_> = open.iter().map(|o| (o.reference.as_str(), o.amount, o.overdue_days)).collect();
        assert_eq!(summary, [("GS/101", dec!(-20000), 75), ("S1", dec!(4000), 75), ("S2", dec!(5000), 14), ("On Account", dec!(-1000), 35)]);

        let rec = ageing(&b, as_at, true);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].buckets, [dec!(5000), dec!(0), dec!(4000), dec!(0)]);
        let pay = ageing(&b, as_at, false);
        assert_eq!(pay.iter().map(|r| (r.party.as_str(), r.total)).collect::<Vec<_>>(), [("Gupta Steel (MSE)", dec!(20000)), ("Sharma Enterprises", dec!(1000))]);

        let msme = overdue_payables(&b, as_at, 45);
        assert_eq!(msme.len(), 1);
        assert_eq!(msme[0].reference, "GS/101");
    }

    #[test]
    fn bill_allocations_must_add_up() {
        use crate::voucher::{BillKind, VoucherError};
        let (mut b, bank, _, _, sales, party) = demo();
        let bad = Voucher {
            id: VoucherId::new(),
            kind: VoucherKind::Sales,
            number: "9".into(),
            date: d(5),
            entries: vec![Entry::dr(party, dec!(100)).bill(BillKind::New, "X", dec!(90), None), Entry::cr(sales, dec!(100))],
            narration: String::new(), cancelled: false, series: None, stock: Vec::new(),
        };
        assert_eq!(b.post_voucher(now(), "u", bad), Err(crate::BookError::Voucher(VoucherError::BillsMismatch { index: 0 })));
        let _ = bank;
    }

}

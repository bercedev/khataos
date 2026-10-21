//! Ratio Analysis and the figures behind the Dashboard.
//!
//! Everything here is derived from the same balances as the Balance Sheet
//! and Profit & Loss, located through the default groups (Current Assets,
//! Sundry Debtors, Sales Accounts…), so sub-groups a user creates under
//! them are counted too.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::Serialize;

use crate::chart::{GroupId, LedgerId};
use crate::reports::profit_and_loss;
use crate::{Amount, Book};

/// Whether `ledger` sits under the default group `name` (at any depth).
fn under(book: &Book, ledger: LedgerId, name: &str) -> bool {
    let target = GroupId::default_named(name);
    let mut group = book.ledger(ledger).map(|l| l.group);
    for _ in 0..=book.groups().count() {
        let Some(id) = group else { return false };
        if id == target {
            return true;
        }
        group = book.group(id).and_then(|g| g.parent);
    }
    false
}

/// Signed (debit positive) total of `balances` for ledgers under `name`.
fn total(book: &Book, balances: &BTreeMap<LedgerId, Amount>, name: &str) -> Amount {
    balances.iter().filter(|(l, _)| under(book, **l, name)).map(|(_, a)| *a).sum()
}

/// Net movement per ledger from vouchers dated within `[from, to]`.
fn movements(book: &Book, from: NaiveDate, to: NaiveDate) -> BTreeMap<LedgerId, Amount> {
    let mut net = BTreeMap::new();
    for v in book.active_vouchers().filter(|v| v.date >= from && v.date <= to) {
        for e in &v.entries {
            *net.entry(e.ledger).or_default() += e.signed();
        }
    }
    net
}

fn ratio(n: Amount, d: Amount) -> Option<Amount> {
    if d.is_zero() { None } else { n.checked_div(d).map(|r| r.round_dp(2)) }
}

fn pct(n: Amount, d: Amount) -> Option<Amount> {
    ratio(n * Amount::ONE_HUNDRED, d)
}

/// Ratio Analysis, laid out like Tally's: principal groups (amounts) and
/// principal ratios. A ratio is `None` when its denominator is zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RatioAnalysis {
    pub from: NaiveDate,
    pub to: NaiveDate,
    // Principal groups, as at `to` (balances) or over the period (flows).
    pub working_capital: Amount,
    pub cash_in_hand: Amount,
    pub bank_accounts: Amount,
    pub bank_od: Amount,
    pub sundry_debtors: Amount,
    pub sundry_creditors: Amount,
    pub sales: Amount,
    pub purchases: Amount,
    pub stock_in_hand: Amount,
    pub net_profit: Amount,
    pub working_capital_turnover: Option<Amount>,
    pub inventory_turnover: Option<Amount>,
    // Principal ratios.
    pub current_ratio: Option<Amount>,
    pub quick_ratio: Option<Amount>,
    pub debt_equity: Option<Amount>,
    pub gross_profit_pct: Option<Amount>,
    pub net_profit_pct: Option<Amount>,
    pub operating_cost_pct: Option<Amount>,
    /// Average days a sale stays unpaid: debtors ÷ sales × days in period.
    pub receivable_days: Option<Amount>,
    pub return_on_investment_pct: Option<Amount>,
    pub return_on_working_capital_pct: Option<Amount>,
}

pub fn ratio_analysis(book: &Book, from: NaiveDate, to: NaiveDate) -> RatioAnalysis {
    let bal = book.balances(Some(to));
    let mv = movements(book, from, to);
    let pl = profit_and_loss(book, from, to);
    let stock = pl.closing_stock;

    let current_assets = total(book, &bal, "Current Assets") + stock;
    let current_liabilities = -total(book, &bal, "Current Liabilities");
    let working_capital = current_assets - current_liabilities;
    let sales = -total(book, &mv, "Sales Accounts");
    let purchases = total(book, &mv, "Purchase Accounts");
    let debtors = total(book, &bal, "Sundry Debtors");
    let loans = -total(book, &bal, "Loans (Liability)");
    let capital = -total(book, &bal, "Capital Account");
    let net_profit = pl.net_profit;
    let days = Amount::from((to - from).num_days() + 1);

    RatioAnalysis {
        from,
        to,
        working_capital,
        cash_in_hand: total(book, &bal, "Cash-in-Hand"),
        bank_accounts: total(book, &bal, "Bank Accounts"),
        bank_od: -(total(book, &bal, "Bank OD A/c") + total(book, &bal, "Bank OCC A/c")),
        sundry_debtors: debtors,
        sundry_creditors: -total(book, &bal, "Sundry Creditors"),
        sales,
        purchases,
        stock_in_hand: stock,
        net_profit,
        working_capital_turnover: ratio(sales, working_capital),
        inventory_turnover: ratio(sales, stock),
        current_ratio: ratio(current_assets, current_liabilities),
        quick_ratio: ratio(current_assets - stock, current_liabilities),
        debt_equity: ratio(loans, capital + net_profit),
        gross_profit_pct: pct(pl.gross_profit, sales),
        net_profit_pct: pct(net_profit, sales),
        operating_cost_pct: pct(sales - net_profit, sales),
        receivable_days: ratio(debtors * days, sales).map(|d| d.round_dp(0)),
        return_on_investment_pct: pct(net_profit, capital + net_profit),
        return_on_working_capital_pct: pct(net_profit, working_capital),
    }
}

/// One month of activity for the Dashboard charts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MonthPoint {
    /// First day of the month.
    pub month: NaiveDate,
    pub sales: Amount,
    pub purchases: Amount,
    /// Money into cash and bank ledgers.
    pub inflow: Amount,
    /// Money out of cash and bank ledgers.
    pub outflow: Amount,
    /// Cash and bank balance at the month's end.
    pub cash_bank_balance: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NamedAmount {
    pub ledger: LedgerId,
    pub name: String,
    pub amount: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpenseShare {
    pub name: String,
    pub amount: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dashboard {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub cash_in_hand: Amount,
    pub bank_balance: Amount,
    pub receivables: Amount,
    pub payables: Amount,
    pub sales: Amount,
    pub purchases: Amount,
    pub gross_profit: Amount,
    pub net_profit: Amount,
    pub closing_stock: Amount,
    /// Every month of the period, oldest first.
    pub months: Vec<MonthPoint>,
    /// Largest debit balances under Sundry Debtors (up to 5).
    pub top_debtors: Vec<NamedAmount>,
    /// Largest credit balances under Sundry Creditors (up to 5).
    pub top_creditors: Vec<NamedAmount>,
    /// Expenses for the period by primary group, largest first.
    pub expenses: Vec<ExpenseShare>,
}

fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1 exists")
}

fn next_month(d: NaiveDate) -> NaiveDate {
    d.checked_add_months(chrono::Months::new(1)).unwrap_or(NaiveDate::MAX)
}

fn top(book: &Book, bal: &BTreeMap<LedgerId, Amount>, group: &str, sign: Amount) -> Vec<NamedAmount> {
    let mut rows: Vec<NamedAmount> = bal
        .iter()
        .filter(|(l, a)| under(book, **l, group) && **a * sign > Amount::ZERO)
        .filter_map(|(l, a)| Some(NamedAmount { ledger: *l, name: book.ledger(*l)?.name.clone(), amount: *a * sign }))
        .collect();
    rows.sort_by(|a, b| b.amount.cmp(&a.amount).then_with(|| a.name.cmp(&b.name)));
    rows.truncate(5);
    rows
}

/// Key figures and month-by-month series for `[from, to]`.
pub fn dashboard(book: &Book, from: NaiveDate, to: NaiveDate) -> Dashboard {
    let bal = book.balances(Some(to));
    let pl = profit_and_loss(book, from, to);
    let cash_bank: Vec<LedgerId> = book.ledgers().filter(|l| book.is_cash_or_bank(l.id)).map(|l| l.id).collect();
    let opening_cash: Amount = {
        let before = from.pred_opt().map(|d| book.balances(Some(d)));
        cash_bank
            .iter()
            .map(|l| match &before {
                Some(b) => b.get(l).copied().unwrap_or_default(),
                None => book.ledger(*l).map_or(Amount::ZERO, |x| x.opening),
            })
            .sum()
    };

    let mut months = Vec::new();
    let mut running = opening_cash;
    let mut start = month_start(from);
    while start <= to && months.len() < 120 {
        let end = next_month(start).pred_opt().unwrap_or(to).min(to);
        let lo = start.max(from);
        let mv = movements(book, lo, end);
        let (mut inflow, mut outflow) = (Amount::ZERO, Amount::ZERO);
        for v in book.active_vouchers().filter(|v| v.date >= lo && v.date <= end) {
            // Contra between two cash/bank ledgers is neither in nor out.
            let net: Amount = v.entries.iter().filter(|e| cash_bank.contains(&e.ledger)).map(|e| e.signed()).sum();
            if net > Amount::ZERO { inflow += net } else { outflow -= net }
        }
        running += inflow - outflow;
        months.push(MonthPoint {
            month: start,
            sales: -total(book, &mv, "Sales Accounts"),
            purchases: total(book, &mv, "Purchase Accounts"),
            inflow,
            outflow,
            cash_bank_balance: running,
        });
        start = next_month(start);
    }

    let mut expenses: Vec<ExpenseShare> =
        pl.expenses.iter().map(|g| ExpenseShare { name: g.name.clone(), amount: g.amount }).collect();
    expenses.sort_by_key(|e| std::cmp::Reverse(e.amount));

    let mv = movements(book, from, to);
    Dashboard {
        from,
        to,
        cash_in_hand: total(book, &bal, "Cash-in-Hand"),
        bank_balance: total(book, &bal, "Bank Accounts"),
        receivables: total(book, &bal, "Sundry Debtors"),
        payables: -total(book, &bal, "Sundry Creditors"),
        sales: -total(book, &mv, "Sales Accounts"),
        purchases: total(book, &mv, "Purchase Accounts"),
        gross_profit: pl.gross_profit,
        net_profit: pl.net_profit,
        closing_stock: pl.closing_stock,
        months,
        top_debtors: top(book, &bal, "Sundry Debtors", Amount::ONE),
        top_creditors: top(book, &bal, "Sundry Creditors", -Amount::ONE),
        expenses,
    }
}

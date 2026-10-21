//! Vouchers: the only way money moves between ledgers.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::forex::Forex;
use crate::inventory::StockLine;
use crate::{Amount, LedgerId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VoucherId(pub Uuid);

impl VoucherId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for VoucherId {
    fn default() -> Self {
        Self::new()
    }
}

/// Voucher types, mirroring the keys accountants already press
/// (F4 Contra, F5 Payment, F6 Receipt, F7 Journal, F8 Sales, F9 Purchase…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VoucherKind {
    Contra,
    Payment,
    Receipt,
    Journal,
    Sales,
    Purchase,
    CreditNote,
    DebitNote,
    /// Stock moved without buying or selling: transfers between godowns,
    /// materials consumed and goods produced.
    StockJournal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrCr {
    Dr,
    Cr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub ledger: LedgerId,
    pub side: DrCr,
    /// Always positive; the side carries the sign.
    pub amount: Amount,
    /// Bill-by-bill allocation of this line (party ledgers). When present,
    /// the allocations add up to `amount`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bills: Vec<BillAllocation>,
    /// Stock items behind this line (item invoice). When present, their
    /// amounts add up to `amount`. Under a debit the stock comes in, under
    /// a credit it goes out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<StockLine>,
    /// Date the bank cleared this line (bank reconciliation). Only for
    /// bank ledgers; `None` until reconciled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_date: Option<NaiveDate>,
    /// The line in a foreign currency: its foreign amount and the rate of
    /// exchange. `amount` (base currency) is still what is posted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forex: Option<Forex>,
}

/// How a line relates to a party's bills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BillKind {
    /// Raises a new bill (e.g. a sales invoice for a debtor).
    New,
    /// Settles part or all of an existing bill.
    Against,
    /// Money received or paid before the bill exists.
    Advance,
    /// Not tied to any bill.
    OnAccount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillAllocation {
    pub kind: BillKind,
    /// Bill reference, usually the invoice number. Empty for on-account.
    pub reference: String,
    /// Always positive; the line's side carries the sign.
    pub amount: Amount,
    /// Due date for new bills (bill date + credit period).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<NaiveDate>,
    /// The allocation in the line's foreign currency, so a bill's outstanding
    /// is known in both currencies. On a line with a foreign amount, either
    /// every allocation has one (adding up to it) or none does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forex_amount: Option<Amount>,
}

impl Entry {
    pub fn dr(ledger: LedgerId, amount: Amount) -> Self {
        Self { ledger, side: DrCr::Dr, amount, bills: Vec::new(), items: Vec::new(), bank_date: None, forex: None }
    }

    pub fn cr(ledger: LedgerId, amount: Amount) -> Self {
        Self { ledger, side: DrCr::Cr, amount, bills: Vec::new(), items: Vec::new(), bank_date: None, forex: None }
    }

    /// Attach a bill allocation (builder style).
    pub fn bill(mut self, kind: BillKind, reference: &str, amount: Amount, due: Option<NaiveDate>) -> Self {
        self.bills.push(BillAllocation { kind, reference: reference.to_owned(), amount, due, forex_amount: None });
        self
    }

    /// Give the line a foreign amount and rate (builder style).
    pub fn fx(mut self, currency: &str, amount: Amount, rate: Amount) -> Self {
        self.forex = Some(Forex { currency: currency.to_owned(), amount, rate });
        self
    }

    /// Attach a bill allocation with its foreign amount (builder style).
    pub fn fx_bill(mut self, kind: BillKind, reference: &str, amount: Amount, foreign: Amount, due: Option<NaiveDate>) -> Self {
        self.bills.push(BillAllocation { kind, reference: reference.to_owned(), amount, due, forex_amount: Some(foreign) });
        self
    }

    /// Attach a stock item line (builder style).
    pub fn item(mut self, line: StockLine) -> Self {
        self.items.push(line);
        self
    }

    /// Amount with sign: debit positive, credit negative.
    pub fn signed(&self) -> Amount {
        match self.side {
            DrCr::Dr => self.amount,
            DrCr::Cr => -self.amount,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voucher {
    pub id: VoucherId,
    pub kind: VoucherKind,
    pub number: String,
    pub date: NaiveDate,
    pub entries: Vec<Entry>,
    pub narration: String,
    /// Cancelled vouchers keep their number (invoice series stay gapless)
    /// and their lines for the record, but affect no balance or report.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancelled: bool,
    /// A named number series within the voucher type, such as a custom
    /// voucher type brought over from Tally ("Sales GST", "Cash Sales"),
    /// each numbered on its own. A number is unique per type, series and
    /// series period, so two series of one type can each have a "1".
    /// `None`: the type's own series.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// Stock Journal lines: stock out of (consumed, transferred from) and
    /// into (produced, transferred to) a location, without a ledger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stock: Vec<StockMove>,
}

/// One Stock Journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockMove {
    /// Into stock (produced, transferred to); out of stock otherwise.
    pub inward: bool,
    #[serde(flatten)]
    pub line: StockLine,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VoucherError {
    #[error("a voucher needs at least one debit and one credit entry")]
    TooFewEntries,
    #[error("entry {index} has a non-positive amount")]
    NonPositiveAmount { index: usize },
    #[error("debits ({dr}) do not equal credits ({cr})")]
    Unbalanced { dr: Amount, cr: Amount },
    #[error("amounts are too large")]
    Overflow,
    #[error("bill allocations on entry {index} do not add up to its amount")]
    BillsMismatch { index: usize },
    #[error("stock items on entry {index} do not add up to its amount, or have a non-positive quantity")]
    ItemsMismatch { index: usize },
    #[error("a Stock Journal needs stock lines, and only a Stock Journal can have them")]
    StockLines,
    #[error("the foreign amount on entry {index} is not valid: it needs a currency code, an amount and a rate above zero, and its bills' foreign amounts must add up to it")]
    Forex { index: usize },
}

impl Voucher {
    pub fn totals(&self) -> (Amount, Amount) {
        self.checked_totals().unwrap_or((Amount::MAX, Amount::MAX))
    }

    /// Totals, or `None` if they overflow the decimal range.
    pub fn checked_totals(&self) -> Option<(Amount, Amount)> {
        self.entries.iter().try_fold((Amount::ZERO, Amount::ZERO), |(dr, cr), e| match e.side {
            DrCr::Dr => Some((dr.checked_add(e.amount)?, cr)),
            DrCr::Cr => Some((dr, cr.checked_add(e.amount)?)),
        })
    }

    /// Structural double-entry checks that hold in every country.
    pub fn validate(&self) -> Result<(), VoucherError> {
        // A cancelled voucher may keep its number without any lines, as
        // Tally keeps one (its lines are dropped when it is cancelled).
        if self.cancelled && self.entries.is_empty() && self.stock.is_empty() {
            return Ok(());
        }
        if (self.kind == VoucherKind::StockJournal) == self.stock.is_empty() {
            return Err(VoucherError::StockLines);
        }
        if self.stock.iter().any(|m| m.line.qty <= Amount::ZERO || m.line.rate < Amount::ZERO || m.line.amount < Amount::ZERO) {
            return Err(VoucherError::StockLines);
        }
        // A Stock Journal may move stock only; any ledger lines (e.g. an
        // additional cost) must still balance.
        if self.kind == VoucherKind::StockJournal && self.entries.is_empty() {
            return Ok(());
        }
        let has_dr = self.entries.iter().any(|e| e.side == DrCr::Dr);
        let has_cr = self.entries.iter().any(|e| e.side == DrCr::Cr);
        if !has_dr || !has_cr {
            return Err(VoucherError::TooFewEntries);
        }
        if let Some(index) = self.entries.iter().position(|e| e.amount <= Amount::ZERO) {
            return Err(VoucherError::NonPositiveAmount { index });
        }
        let (dr, cr) = self.checked_totals().ok_or(VoucherError::Overflow)?;
        if dr != cr {
            return Err(VoucherError::Unbalanced { dr, cr });
        }
        for (index, e) in self.entries.iter().enumerate() {
            let allocated = e
                .bills
                .iter()
                .try_fold(Amount::ZERO, |acc, b| acc.checked_add(b.amount))
                .ok_or(VoucherError::Overflow)?;
            if !e.bills.is_empty() && (allocated != e.amount || e.bills.iter().any(|b| b.amount <= Amount::ZERO)) {
                return Err(VoucherError::BillsMismatch { index });
            }
            let items = e
                .items
                .iter()
                .try_fold(Amount::ZERO, |acc, l| acc.checked_add(l.amount))
                .ok_or(VoucherError::Overflow)?;
            let bad_line = e.items.iter().any(|l| l.qty <= Amount::ZERO || l.rate < Amount::ZERO || l.amount < Amount::ZERO);
            if !e.items.is_empty() && (items != e.amount || bad_line) {
                return Err(VoucherError::ItemsMismatch { index });
            }
            if !forex_ok(e) {
                return Err(VoucherError::Forex { index });
            }
        }
        Ok(())
    }
}

/// A line's foreign amount is well formed, and its bills' foreign amounts
/// (all or none of them) add up to it.
fn forex_ok(e: &Entry) -> bool {
    let with = e.bills.iter().filter(|b| b.forex_amount.is_some()).count();
    let Some(fx) = &e.forex else { return with == 0 };
    if !crate::forex::is_code(&fx.currency) || fx.amount <= Amount::ZERO || fx.rate <= Amount::ZERO {
        return false;
    }
    if with == 0 {
        return true;
    }
    let amounts: Option<Vec<Amount>> = e.bills.iter().map(|b| b.forex_amount.filter(|a| *a > Amount::ZERO)).collect();
    amounts.is_some_and(|a| a.iter().try_fold(Amount::ZERO, |acc, x| acc.checked_add(*x)) == Some(fx.amount))
}

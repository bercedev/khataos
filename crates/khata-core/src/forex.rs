//! Foreign currency (see `docs/multi-currency.md`).
//!
//! The books are kept in one base currency, the pack's. A party or bank
//! ledger may be in a foreign currency, and a voucher line may carry its
//! foreign amount and the rate of exchange used ([`Forex`]). The base amount
//! on the line is still what is posted: every voucher balances in the base
//! currency, and the foreign amount is extra data that reports and invoices
//! show beside it.
//!
//! - The currency master is a built-in ISO 4217 list ([`builtin`]) that a
//!   company can extend or override ([`Book::save_currency`]).
//! - Rates are remembered per currency per date: those the user enters on a
//!   voucher, and those set in the company's rate table ([`Book::fx_rate`]).
//!   Nothing is fetched from the internet; an app that wants online rates
//!   asks the user first and stores them with [`Book::set_fx_rate`].
//! - When a foreign bill is settled at a different rate, [`settle`] books the
//!   party line at the bill's carrying amount and puts the difference on a
//!   Forex Gain/Loss line in the same voucher (realised gain or loss).
//! - [`revaluation`] works out the unrealised difference on foreign balances
//!   at closing rates, as a journal to review, never posted by itself.
//!
//! All new fields are optional and skipped when empty, so vouchers, ledgers
//! and profiles written before this module serialise exactly as they did and
//! old audit trails replay and verify unchanged.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::chart::{Ledger, LedgerId};
use crate::profile::Profile;
use crate::voucher::{BillKind, DrCr, Entry, Voucher, VoucherId, VoucherKind};
use crate::{Amount, Book, BookError};

/// A currency in the currency master.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Currency {
    /// ISO 4217 code ("USD"): three capital letters.
    pub code: String,
    /// Printed before amounts ("$").
    pub symbol: String,
    /// Formal name ("US Dollar").
    pub name: String,
    /// Decimal places of the minor unit (2 for cents; 0 for yen; 3 for fils).
    pub decimals: u32,
}

/// The foreign side of a voucher line: `amount` units of `currency` at
/// `rate` base units each. The line's own amount (base currency) is what
/// is posted; it is normally `amount × rate` rounded to the base currency,
/// except on a settlement, where it is the bill's carrying amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forex {
    pub currency: String,
    /// Always positive, like the line's amount.
    pub amount: Amount,
    /// Base currency per one unit of `currency`.
    pub rate: Amount,
}

/// Name of the ledger realised gains and losses are posted to when a
/// company has not picked one (it is created under Indirect Expenses).
pub const FOREX_LEDGER: &str = "Forex Gain/Loss";

/// Most decimal places a currency may have.
pub const MAX_DECIMALS: u32 = 4;
/// Longest currency symbol and name accepted.
pub const MAX_SYMBOL: usize = 8;
pub const MAX_NAME: usize = 40;

/// Whether `code` has the shape of an ISO 4217 code: three capital letters.
pub fn is_code(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase())
}

/// (code, symbol, formal name, decimals): currencies an Indian, Gulf,
/// European or Asian business is likely to invoice in. Decimals follow ISO
/// 4217. Symbols that would be ambiguous on an invoice use the code.
const BUILTIN: &[(&str, &str, &str, u32)] = &[
    ("AED", "AED", "UAE Dirham", 2),
    ("AUD", "A$", "Australian Dollar", 2),
    ("BDT", "৳", "Bangladeshi Taka", 2),
    ("BHD", "BHD", "Bahraini Dinar", 3),
    ("BRL", "R$", "Brazilian Real", 2),
    ("CAD", "C$", "Canadian Dollar", 2),
    ("CHF", "CHF", "Swiss Franc", 2),
    ("CNY", "CN¥", "Chinese Yuan", 2),
    ("CZK", "Kč", "Czech Koruna", 2),
    ("DKK", "kr.", "Danish Krone", 2),
    ("EGP", "E£", "Egyptian Pound", 2),
    ("EUR", "€", "Euro", 2),
    ("GBP", "£", "Pound Sterling", 2),
    ("HKD", "HK$", "Hong Kong Dollar", 2),
    ("IDR", "Rp", "Indonesian Rupiah", 2),
    ("ILS", "₪", "Israeli New Shekel", 2),
    ("INR", "₹", "Indian Rupee", 2),
    ("JOD", "JOD", "Jordanian Dinar", 3),
    ("JPY", "¥", "Japanese Yen", 0),
    ("KES", "KSh", "Kenyan Shilling", 2),
    ("KRW", "₩", "South Korean Won", 0),
    ("KWD", "KWD", "Kuwaiti Dinar", 3),
    ("LKR", "LKR", "Sri Lankan Rupee", 2),
    ("MXN", "MX$", "Mexican Peso", 2),
    ("MYR", "RM", "Malaysian Ringgit", 2),
    ("NGN", "₦", "Nigerian Naira", 2),
    ("NOK", "kr", "Norwegian Krone", 2),
    ("NPR", "NPR", "Nepalese Rupee", 2),
    ("NZD", "NZ$", "New Zealand Dollar", 2),
    ("OMR", "OMR", "Omani Rial", 3),
    ("PHP", "₱", "Philippine Peso", 2),
    ("PKR", "PKR", "Pakistani Rupee", 2),
    ("PLN", "zł", "Polish Złoty", 2),
    ("QAR", "QAR", "Qatari Riyal", 2),
    ("SAR", "SAR", "Saudi Riyal", 2),
    ("SEK", "kr", "Swedish Krona", 2),
    ("SGD", "S$", "Singapore Dollar", 2),
    ("THB", "฿", "Thai Baht", 2),
    ("TRY", "₺", "Turkish Lira", 2),
    ("TWD", "NT$", "New Taiwan Dollar", 2),
    ("USD", "$", "US Dollar", 2),
    ("VND", "₫", "Vietnamese Đồng", 0),
    ("ZAR", "R", "South African Rand", 2),
];

/// The built-in currency master, by code.
pub fn builtin() -> impl Iterator<Item = Currency> {
    BUILTIN.iter().map(|&(code, symbol, name, decimals)| Currency {
        code: code.to_owned(),
        symbol: symbol.to_owned(),
        name: name.to_owned(),
        decimals,
    })
}

/// A built-in currency.
pub fn builtin_currency(code: &str) -> Option<Currency> {
    builtin().find(|c| c.code == code)
}

/// A rate of exchange remembered for a currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RememberedRate {
    /// The date the rate was entered for.
    pub date: NaiveDate,
    pub rate: Amount,
    /// Taken from a voucher line rather than the rate table.
    pub from_voucher: bool,
}

impl Book {
    /// A currency from the company's master: its own definition, else the
    /// built-in one.
    pub fn currency(&self, code: &str) -> Option<Currency> {
        self.profile().and_then(|p| p.currencies.get(code).cloned()).or_else(|| builtin_currency(code))
    }

    /// The whole currency master: built-in currencies with the company's
    /// own definitions in place of (or added to) them, by code.
    pub fn currencies(&self) -> Vec<Currency> {
        let mut all: BTreeMap<String, Currency> = builtin().map(|c| (c.code.clone(), c)).collect();
        if let Some(p) = self.profile() {
            all.extend(p.currencies.iter().map(|(k, c)| (k.clone(), c.clone())));
        }
        all.into_values().collect()
    }

    /// Add a currency to the company's master, or change a built-in one's
    /// symbol, name or decimals.
    pub fn save_currency(&mut self, at: DateTime<Utc>, user: &str, c: Currency) -> Result<(), BookError> {
        let mut p = self.profile().cloned().ok_or(BookError::Currency("set the country first".into()))?;
        p.currencies.insert(c.code.clone(), c);
        self.save_profile(at, user, p)
    }

    /// Set (or, with `None`, remove) the rate of exchange for one unit of
    /// `code` on a date, in the company's rate table.
    pub fn set_fx_rate(&mut self, at: DateTime<Utc>, user: &str, code: &str, date: NaiveDate, rate: Option<Amount>) -> Result<(), BookError> {
        let mut p = self.profile().cloned().ok_or(BookError::Currency("set the country first".into()))?;
        match rate {
            Some(r) => {
                p.fx_rates.entry(code.to_owned()).or_default().insert(date, r);
            }
            None => {
                if let Some(rates) = p.fx_rates.get_mut(code) {
                    rates.remove(&date);
                    if rates.is_empty() {
                        p.fx_rates.remove(code);
                    }
                }
            }
        }
        self.save_profile(at, user, p)
    }

    /// The rate of exchange to offer for `code` on `on`: the latest one on
    /// or before that date, from the rate table or from a voucher line in
    /// that currency (the last one entered on the day). On the same date the
    /// rate table wins: it is the rate the company set on purpose.
    pub fn fx_rate(&self, code: &str, on: NaiveDate) -> Option<RememberedRate> {
        let table = self
            .profile()
            .and_then(|p| p.fx_rates.get(code))
            .and_then(|r| r.range(..=on).next_back())
            .map(|(&date, &rate)| RememberedRate { date, rate, from_voucher: false });
        let used = self
            .active_vouchers()
            .filter(|v| v.date <= on)
            .filter_map(|v| {
                let fx = v.entries.iter().filter_map(|e| e.forex.as_ref()).rfind(|f| f.currency == code)?;
                Some(((v.date, v.id), fx.rate))
            })
            .max_by_key(|(key, _)| *key)
            .map(|((date, _), rate)| RememberedRate { date, rate, from_voucher: true });
        match (table, used) {
            (Some(t), Some(u)) => Some(if u.date > t.date { u } else { t }),
            (t, u) => t.or(u),
        }
    }
}

/// A profile's currency master and rate table must be well formed.
pub(crate) fn check_profile(p: &Profile) -> Result<(), BookError> {
    let bad = |m: String| Err(BookError::Currency(m));
    for (code, c) in &p.currencies {
        if *code != c.code || !is_code(code) {
            return bad(format!("{code:?} is not an ISO 4217 currency code"));
        }
        let symbol = c.symbol.trim();
        if symbol.is_empty() || symbol.chars().count() > MAX_SYMBOL || c.name.trim().is_empty() || c.name.chars().count() > MAX_NAME {
            return bad(format!("{code} needs a symbol of 1 to {MAX_SYMBOL} characters and a name of 1 to {MAX_NAME}"));
        }
        if c.decimals > MAX_DECIMALS {
            return bad(format!("{code} can have at most {MAX_DECIMALS} decimal places"));
        }
    }
    for (code, rates) in &p.fx_rates {
        if !is_code(code) {
            return bad(format!("{code:?} is not an ISO 4217 currency code"));
        }
        if rates.values().any(|r| *r <= Amount::ZERO) {
            return bad(format!("rates of exchange for {code} must be more than zero"));
        }
    }
    Ok(())
}

/// A ledger's currency must look like an ISO code, and changing it must not
/// leave voucher lines in another foreign currency on it.
pub(crate) fn check_ledger(book: &Book, l: &Ledger) -> Result<(), BookError> {
    let Some(code) = &l.currency else { return Ok(()) };
    if !is_code(code) {
        return Err(BookError::Currency(format!("{code:?} is not an ISO 4217 currency code")));
    }
    let clash = book.vouchers().flat_map(|v| &v.entries).any(|e| {
        e.ledger == l.id && e.forex.as_ref().is_some_and(|f| f.currency != *code)
    });
    if clash {
        return Err(BookError::Currency(format!("{} has vouchers in another currency, so it cannot be in {code}", l.name)));
    }
    Ok(())
}

/// A line on a ledger kept in a foreign currency may only carry that
/// currency. (Base-only lines are allowed: a gain/loss or revaluation.)
pub(crate) fn check_voucher(book: &Book, v: &Voucher) -> Result<(), BookError> {
    for e in &v.entries {
        let (Some(fx), Some(l)) = (&e.forex, book.ledger(e.ledger)) else { continue };
        if l.currency.as_ref().is_some_and(|c| *c != fx.currency) {
            return Err(BookError::Currency(format!("{} is kept in {}, not {}", l.name, l.currency.as_deref().unwrap_or(""), fx.currency)));
        }
    }
    Ok(())
}

/// What a bill stands at, in both currencies (signed: debit positive).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Open {
    base: Amount,
    foreign: Amount,
}

/// Open foreign bills of party lines, except those of voucher `skip` (the
/// one being settled, when it is altered): (ledger, reference) → currency
/// and what is open. Only bills raised with a foreign amount count.
fn open_foreign_bills(book: &Book, skip: VoucherId) -> BTreeMap<(LedgerId, String), (String, Open)> {
    let mut open: BTreeMap<(LedgerId, String), (String, Open)> = BTreeMap::new();
    let mut vs: Vec<&Voucher> = book.active_vouchers().filter(|v| v.id != skip).collect();
    vs.sort_by_key(|v| (v.date, v.id));
    // Bills first raised in a foreign currency.
    for v in &vs {
        for e in &v.entries {
            let Some(fx) = &e.forex else { continue };
            for b in e.bills.iter().filter(|b| b.kind == BillKind::New && b.forex_amount.is_some()) {
                open.entry((e.ledger, b.reference.clone())).or_insert_with(|| (fx.currency.clone(), Open::default()));
            }
        }
    }
    for v in &vs {
        for e in &v.entries {
            for b in &e.bills {
                let Some((currency, o)) = open.get_mut(&(e.ledger, b.reference.clone())) else { continue };
                let sign = if e.side == DrCr::Dr { Amount::ONE } else { -Amount::ONE };
                o.base += sign * b.amount;
                let same = e.forex.as_ref().is_some_and(|f| f.currency == *currency);
                if let (true, Some(f)) = (same, b.forex_amount) {
                    o.foreign += sign * f;
                }
            }
        }
    }
    open
}

/// One foreign bill settled by a voucher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settlement {
    pub ledger: LedgerId,
    pub reference: String,
    pub currency: String,
    /// Foreign amount settled.
    pub foreign: Amount,
    /// What that part of the bill stood at in the books (its carrying amount).
    pub carrying: Amount,
    /// What it was settled for, at the voucher's rate.
    pub settled: Amount,
    /// Realised gain (negative: loss), in the base currency.
    pub gain: Amount,
}

/// Book realised forex gain or loss on the foreign bills `v` settles.
///
/// For each Agst Ref allocation with a foreign amount on a bill raised in
/// that currency, the allocation (and its line) is set to the bill's
/// carrying amount: the open base amount in proportion to the foreign
/// amount settled, all of it when the bill is settled in full, so the bill
/// closes in both currencies. The difference from what the line was posted
/// at (foreign amount × the voucher's rate) goes to `gain_loss` as one
/// base-only line, merged with any base-only line already on that ledger.
/// The voucher stays balanced; calling this twice changes nothing more.
pub fn settle(book: &Book, v: &mut Voucher, gain_loss: LedgerId, base_decimals: u32) -> Vec<Settlement> {
    let mut open = open_foreign_bills(book, v.id);
    let mut out = Vec::new();
    let mut diff = Amount::ZERO;
    for e in v.entries.iter_mut().filter(|e| e.ledger != gain_loss) {
        let Some(fx) = e.forex.clone() else { continue };
        let before = e.signed();
        let sign = if e.side == DrCr::Dr { Amount::ONE } else { -Amount::ONE };
        let mut changed = false;
        for b in e.bills.iter_mut().filter(|b| b.kind == BillKind::Against) {
            let Some(f) = b.forex_amount else { continue };
            let Some((currency, o)) = open.get_mut(&(e.ledger, b.reference.clone())) else { continue };
            // Only a settlement: the line must reduce what is open.
            if *currency != fx.currency || o.foreign.is_zero() || o.foreign.is_sign_positive() == (sign > Amount::ZERO) {
                continue;
            }
            let carrying = if f == o.foreign.abs() {
                o.base.abs()
            } else {
                (f * o.base.abs() / o.foreign.abs()).round_dp(base_decimals)
            };
            if carrying <= Amount::ZERO {
                continue;
            }
            out.push(Settlement {
                ledger: e.ledger,
                reference: b.reference.clone(),
                currency: fx.currency.clone(),
                foreign: f,
                carrying,
                settled: b.amount,
                // Money in for a receivable (Cr), out for a payable (Dr).
                gain: if e.side == DrCr::Cr { b.amount - carrying } else { carrying - b.amount },
            });
            o.base += sign * carrying;
            o.foreign += sign * f;
            b.amount = carrying;
            changed = true;
        }
        if changed {
            e.amount = e.bills.iter().map(|b| b.amount).sum();
            diff += before - e.signed();
        }
    }
    if out.is_empty() {
        return out;
    }
    // One base-only line on the gain/loss ledger, merged with any there.
    let mut net = diff;
    v.entries.retain(|e| {
        let merge = e.ledger == gain_loss && e.forex.is_none() && e.bills.is_empty() && e.items.is_empty();
        if merge {
            net += e.signed();
        }
        !merge
    });
    if !net.is_zero() {
        v.entries.push(if net > Amount::ZERO { Entry::dr(gain_loss, net) } else { Entry::cr(gain_loss, -net) });
    }
    out
}

/// A foreign balance restated at a closing rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Revaluation {
    pub ledger: LedgerId,
    pub name: String,
    pub currency: String,
    /// Balance in the foreign currency (debit positive).
    pub foreign: Amount,
    /// Balance in the books, base currency (debit positive).
    pub book: Amount,
    pub rate: Amount,
    /// `foreign × rate`, rounded to the base currency.
    pub revalued: Amount,
    /// `revalued − book`: a debit to the ledger when positive.
    pub difference: Amount,
}

/// Foreign balances of ledgers kept in a foreign currency as at a date,
/// restated at `rates` (base per unit, by currency code). Ledgers whose
/// currency has no rate are left out. Nothing is posted: see
/// [`revaluation_journal`].
pub fn revaluation(book: &Book, as_at: NaiveDate, rates: &BTreeMap<String, Amount>, base_decimals: u32) -> Vec<Revaluation> {
    let balances = book.balances(Some(as_at));
    let mut foreign: BTreeMap<LedgerId, Amount> = BTreeMap::new();
    for v in book.active_vouchers().filter(|v| v.date <= as_at) {
        for e in &v.entries {
            let Some(fx) = &e.forex else { continue };
            if book.ledger(e.ledger).and_then(|l| l.currency.as_ref()) == Some(&fx.currency) {
                let sign = if e.side == DrCr::Dr { Amount::ONE } else { -Amount::ONE };
                *foreign.entry(e.ledger).or_default() += sign * fx.amount;
            }
        }
    }
    let mut out: Vec<Revaluation> = book
        .ledgers()
        .filter_map(|l| {
            let currency = l.currency.clone()?;
            let rate = *rates.get(&currency)?;
            let f = foreign.get(&l.id).copied().unwrap_or_default();
            let b = balances.get(&l.id).copied().unwrap_or(l.opening);
            let revalued = (f * rate).round_dp(base_decimals);
            Some(Revaluation { ledger: l.id, name: l.name.clone(), currency, foreign: f, book: b, rate, revalued, difference: revalued - b })
        })
        .filter(|r| !(r.foreign.is_zero() && r.book.is_zero()))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A Journal that books `lines` (unrealised gain or loss) against
/// `gain_loss`, for the user to review and post; `None` when nothing moves.
pub fn revaluation_journal(as_at: NaiveDate, number: String, lines: &[Revaluation], gain_loss: LedgerId) -> Option<Voucher> {
    let mut entries: Vec<Entry> = lines
        .iter()
        .filter(|r| !r.difference.is_zero())
        .map(|r| if r.difference > Amount::ZERO { Entry::dr(r.ledger, r.difference) } else { Entry::cr(r.ledger, -r.difference) })
        .collect();
    let net: Amount = entries.iter().map(Entry::signed).sum();
    if entries.is_empty() {
        return None;
    }
    if !net.is_zero() {
        entries.push(if net > Amount::ZERO { Entry::cr(gain_loss, net) } else { Entry::dr(gain_loss, -net) });
    }
    Some(Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Journal,
        number,
        date: as_at,
        entries,
        narration: format!("Revaluation of foreign currency balances at closing rates as at {as_at}"),
        cancelled: false,
        series: None,
        stock: Vec::new(),
    })
}

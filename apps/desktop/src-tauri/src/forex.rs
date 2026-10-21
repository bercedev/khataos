//! Foreign currency on the desktop (see `docs/multi-currency.md`): the
//! currency master, a ledger's currency, remembered rates of exchange,
//! foreign amounts on voucher input, realised gain/loss on settlement and
//! the period-end revaluation suggestion.
//!
//! Rates are never fetched from the internet. [`RateSource`] is the hook
//! for an online source: an app that offers one must ask the user first,
//! then store what it got with [`set_fx_rate`] so it is in the audit trail.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use khata_core::forex::{self as core_fx, Currency, FOREX_LEDGER, Forex};
use khata_core::{Amount, Book, BookError, GroupId, LedgerId, UserRole, Voucher, VoucherKind};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, amt, err, group_is_party, now, open_mut, open_ref, os_user, pack, parse_amount, parse_date, parse_uuid, require};

/// The foreign side of a voucher line, from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForexInput {
    pub currency: String,
    /// Foreign amount, positive.
    pub amount: String,
    /// Base currency per unit.
    pub rate: String,
    /// Foreign amount of each bill allocation, in order. Empty: shared out
    /// in proportion to the bills' base amounts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bills: Vec<String>,
}

/// The foreign side of a saved line, for the frontend.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ForexView {
    pub currency: String,
    pub amount: String,
    pub rate: String,
    /// Foreign amount of each bill allocation, in order (empty if none).
    pub bills: Vec<String>,
}

pub fn forex_view(e: &khata_core::Entry) -> Option<ForexView> {
    let fx = e.forex.as_ref()?;
    Some(ForexView {
        currency: fx.currency.clone(),
        amount: amt(fx.amount),
        rate: fx.rate.normalize().to_string(),
        bills: e.bills.iter().filter_map(|b| b.forex_amount.map(amt)).collect(),
    })
}

/// Decimal places of the company's base currency (the pack's).
pub fn base_decimals(book: &Book) -> u32 {
    pack(book).map_or(2, |p| p.currency().minor_units)
}

/// The base currency's ISO code, once a country is set.
pub fn base_code(book: &Book) -> Option<&'static str> {
    pack(book).map(|p| p.currency().code)
}

/// Turn a line's foreign input into its [`Forex`] and fill the bills'
/// foreign amounts.
pub fn entry_forex(book: &Book, f: &ForexInput, bills: &mut [khata_core::BillAllocation]) -> ApiResult<Forex> {
    let currency = f.currency.trim().to_ascii_uppercase();
    if book.currency(&currency).is_none() {
        return Err(format!("{currency:?} is not in the currency master"));
    }
    if Some(currency.as_str()) == base_code(book) {
        return Err(format!("{currency} is the company's own currency"));
    }
    let amount = parse_amount(&f.amount)?;
    let rate = parse_amount(&f.rate)?;
    if amount <= Amount::ZERO || rate <= Amount::ZERO {
        return Err("a foreign amount needs an amount and a rate of exchange above zero".into());
    }
    let decimals = book.currency(&currency).map_or(2, |c| c.decimals);
    if amount.normalize().scale() > decimals {
        return Err(format!("{currency} amounts have at most {decimals} decimal places"));
    }
    if !bills.is_empty() {
        if f.bills.is_empty() {
            // In proportion to the base amounts; the last bill takes the rounding.
            let base: Amount = bills.iter().map(|b| b.amount).sum();
            let mut left = amount;
            let n = bills.len();
            for (i, b) in bills.iter_mut().enumerate() {
                let share = if i + 1 == n || base.is_zero() { left } else { (amount * b.amount / base).round_dp(decimals) };
                b.forex_amount = Some(share);
                left -= share;
            }
        } else if f.bills.len() == bills.len() {
            for (b, a) in bills.iter_mut().zip(&f.bills) {
                b.forex_amount = Some(parse_amount(a)?);
            }
        } else {
            return Err("give a foreign amount for every bill, or for none".into());
        }
    }
    Ok(Forex { currency, amount, rate })
}

/// The company's Forex Gain/Loss ledger: the one named so, if any.
pub fn gain_loss_ledger(book: &Book) -> Option<LedgerId> {
    book.ledger_by_name(FOREX_LEDGER).map(|l| l.id)
}

/// Book the realised gain or loss of the foreign bills `v` settles (see
/// [`core_fx::settle`]), creating the Forex Gain/Loss ledger under Indirect
/// Expenses the first time one is needed.
pub fn settle_voucher(book: &mut Book, v: &mut Voucher) -> ApiResult<Vec<core_fx::Settlement>> {
    if !v.entries.iter().any(|e| e.forex.is_some() && !e.bills.is_empty()) {
        return Ok(Vec::new());
    }
    let decimals = base_decimals(book);
    let gl = match gain_loss_ledger(book) {
        Some(id) => id,
        None => {
            // Only create the ledger when there is something to post to it.
            let mut probe = v.clone();
            let found = core_fx::settle(book, &mut probe, LedgerId::new(), decimals);
            if found.iter().all(|s| s.gain.is_zero()) {
                *v = probe;
                return Ok(found);
            }
            let group = GroupId::default_named("Indirect Expenses");
            book.create_ledger(now(), &os_user(), FOREX_LEDGER, group).map_err(err)?
        }
    };
    Ok(core_fx::settle(book, v, gl, decimals))
}

/// Whether entry `index` of a saved voucher is the gain/loss line that
/// [`settle_voucher`] books (the frontend leaves it out when altering).
pub fn is_gain_line(book: &Book, v: &Voucher, index: usize) -> bool {
    let e = &v.entries[index];
    let settles = v.entries.iter().any(|x| x.forex.is_some() && x.bills.iter().any(|b| b.kind == khata_core::BillKind::Against && b.forex_amount.is_some()));
    settles && Some(e.ledger) == gain_loss_ledger(book) && e.forex.is_none() && e.bills.is_empty() && e.items.is_empty()
}

// ------------------------------------------------------------ invoice

/// Countries whose invoices in a foreign currency must also show the tax
/// in the local currency (India, UAE, KSA, UK, Singapore; see the doc).
const TAX_IN_BASE: &[&str] = &["IN", "AE", "SA", "GB", "SG"];

/// A sales invoice or note to a party kept in a foreign currency prints in
/// that currency: every amount of `doc` is restated from the lines' foreign
/// amounts (or at the voucher's rate where a line has none), the total is
/// the party's foreign amount, and the currency section carries the rate,
/// the base-currency total and, where the law asks, the tax in the base
/// currency. Other vouchers are left as they are.
/// Structured e-invoices (ZATCA, PINT, XRechnung, Factur-X, CFDI, VeriFactu)
/// are written in the base currency today. A voucher in a foreign currency prints in that
/// currency, so its XML would not match the printed invoice: refuse it until
/// the document and tax currencies are written separately (docs/multi-currency.md).
pub fn refuse_foreign_einvoice(v: &Voucher, what: &str) -> ApiResult<()> {
    match v.entries.iter().find_map(|e| e.forex.as_ref()) {
        Some(fx) => Err(format!(
            "{what} for an invoice in {} is not supported yet: the e-invoice would be in the base currency while the printed invoice is in {}",
            fx.currency, fx.currency
        )),
        None => Ok(()),
    }
}

pub fn invoice_currency(book: &Book, v: &Voucher, doc: &mut khata_print::InvoiceDoc) {
    let Some(base) = base_code(book) else { return };
    let party = v.entries.iter().find(|e| {
        e.forex.is_some() && book.ledger(e.ledger).is_some_and(|l| l.currency.is_some() && group_is_party(book, l.group))
    });
    let Some((party, fx)) = party.and_then(|e| Some((e, e.forex.as_ref()?))) else { return };
    let decimals = book.currency(&fx.currency).map_or(2, |c| c.decimals);
    let rate = fx.rate;
    // Base amount → foreign amount, from the other lines (items shared out).
    let mut known: Vec<(Amount, Amount)> = Vec::new();
    for e in v.entries.iter().filter(|e| !std::ptr::eq(*e, party)) {
        let Some(f) = e.forex.as_ref().filter(|f| f.currency == fx.currency) else { continue };
        known.push((e.amount, f.amount));
        let mut left = f.amount;
        for (i, l) in e.items.iter().enumerate() {
            let share = if i + 1 == e.items.len() || e.amount.is_zero() { left } else { (f.amount * l.amount / e.amount).round_dp(decimals) };
            left -= share;
            known.push((l.amount, share));
        }
    }
    let mut convert = |a: Amount| -> Amount {
        match known.iter().position(|(b, _)| *b == a.abs()) {
            Some(i) => {
                let f = known.remove(i).1;
                if a.is_sign_negative() { -f } else { f }
            }
            None => (a / rate).round_dp(decimals),
        }
    };
    let base_tax: Amount = doc.taxes.iter().map(|(_, a)| *a).sum();
    for l in &mut doc.lines {
        l.amount = convert(l.amount);
        l.rate = l.rate.map(|r| (r / rate).round_dp(decimals.max(2)));
    }
    for (_, a) in doc.taxes.iter_mut().chain(doc.others.iter_mut()) {
        *a = convert(*a);
    }
    doc.total = fx.amount;
    // The company's convention, with the invoice currency's names ("US Dollars … Only" from India).
    doc.total_in_words = crate::invoice::words(book, fx.amount, Some(&fx.currency), false);
    let asks = book.country().is_some_and(|c| TAX_IN_BASE.contains(&c));
    doc.currency = Some(khata_print::CurrencySection {
        code: fx.currency.clone(),
        base_code: base.to_owned(),
        rate,
        base_total: party.amount,
        base_tax: (asks && !doc.taxes.is_empty()).then_some(base_tax),
    });
}

// ------------------------------------------------------------ master

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CurrencyDto {
    pub code: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u32,
}

impl From<Currency> for CurrencyDto {
    fn from(c: Currency) -> Self {
        Self { code: c.code, symbol: c.symbol, name: c.name, decimals: c.decimals }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CurrenciesDto {
    /// The company's own currency (the pack's), once a country is set.
    pub base: Option<CurrencyDto>,
    pub currencies: Vec<CurrencyDto>,
}

pub fn list_currencies(slot: &Slot) -> ApiResult<CurrenciesDto> {
    let book = open_ref(slot)?;
    let base = base_code(book).map(|code| {
        book.currency(code).map_or_else(
            || CurrencyDto { code: code.to_owned(), symbol: code.to_owned(), name: code.to_owned(), decimals: base_decimals(book) },
            CurrencyDto::from,
        )
    });
    Ok(CurrenciesDto { base, currencies: book.currencies().into_iter().map(CurrencyDto::from).collect() })
}

/// Add a currency to the master, or change a built-in one.
pub fn save_currency(slot: &mut Slot, code: &str, symbol: &str, name: &str, decimals: u32) -> ApiResult<CurrenciesDto> {
    {
        let (store, book) = open_mut(slot)?;
        require(book, UserRole::Accountant)?;
        let c = Currency { code: code.trim().to_ascii_uppercase(), symbol: symbol.trim().to_owned(), name: name.trim().to_owned(), decimals };
        book.save_currency(now(), &os_user(), c).map_err(err)?;
        store.save(book).map_err(err)?;
    }
    list_currencies(slot)
}

/// Keep a party or bank ledger in a foreign currency (`""` or the base
/// currency: back to the base currency).
pub fn set_ledger_currency(slot: &mut Slot, ledger_id: &str, code: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    let id = LedgerId(parse_uuid(ledger_id, "ledger")?);
    let mut l = book.ledger(id).ok_or("unknown ledger")?.clone();
    let code = code.trim().to_ascii_uppercase();
    l.currency = if code.is_empty() || Some(code.as_str()) == base_code(book) {
        None
    } else {
        if book.currency(&code).is_none() {
            return Err(format!("{code:?} is not in the currency master"));
        }
        if !group_is_party(book, l.group) && !book.is_cash_or_bank(l.id) {
            return Err("only party and bank ledgers can be kept in a foreign currency".into());
        }
        Some(code)
    };
    book.alter_ledger(now(), &os_user(), l).map_err(|e| match e {
        BookError::Currency(m) => m,
        e => e.to_string(),
    })?;
    store.save(book).map_err(err)?;
    Ok(())
}

// ------------------------------------------------------------- rates

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RateDto {
    pub currency: String,
    pub date: NaiveDate,
    pub rate: String,
    /// Remembered from a voucher rather than the rate table.
    pub from_voucher: bool,
}

/// The rate to offer for `code` on `date`: the last one entered on or
/// before it (rate table or voucher). `None` when there is none yet.
pub fn fx_rate(slot: &Slot, code: &str, date: &str) -> ApiResult<Option<RateDto>> {
    let book = open_ref(slot)?;
    let code = code.trim().to_ascii_uppercase();
    Ok(book.fx_rate(&code, parse_date(date)?).map(|r| RateDto { currency: code, date: r.date, rate: r.rate.normalize().to_string(), from_voucher: r.from_voucher }))
}

/// Set (empty `rate`: remove) a rate in the company's rate table.
pub fn set_fx_rate(slot: &mut Slot, code: &str, date: &str, rate: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    let code = code.trim().to_ascii_uppercase();
    if book.currency(&code).is_none() {
        return Err(format!("{code:?} is not in the currency master"));
    }
    let rate = if rate.trim().is_empty() { None } else { Some(parse_amount(rate)?) };
    book.set_fx_rate(now(), &os_user(), &code, parse_date(date)?, rate).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

/// Where rates could come from besides the user. KhataOS ships none that
/// goes online: rates are typed in (or remembered). An implementation that
/// fetches from a central bank or the customs notification must run only
/// after the user agrees, and its rates are stored with [`set_fx_rate`].
pub trait RateSource {
    /// Rates for `codes` on `date`, base currency per unit.
    fn rates(&self, base: &str, codes: &[String], date: NaiveDate) -> Result<BTreeMap<String, Amount>, String>;
}

// ------------------------------------------------------- revaluation

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RevaluationDto {
    pub ledger_id: String,
    pub name: String,
    pub currency: String,
    pub foreign: String,
    pub book: String,
    pub rate: String,
    pub revalued: String,
    /// Debit to the ledger when positive; the other side is Forex Gain/Loss.
    pub difference: String,
}

/// Closing rates: those given, else the remembered rate on `as_at`.
fn closing_rates(book: &Book, as_at: NaiveDate, given: &BTreeMap<String, String>) -> ApiResult<BTreeMap<String, Amount>> {
    let mut rates = BTreeMap::new();
    for code in book.ledgers().filter_map(|l| l.currency.clone()) {
        let rate = match given.get(&code).filter(|r| !r.trim().is_empty()) {
            Some(r) => Some(parse_amount(r)?),
            None => book.fx_rate(&code, as_at).map(|r| r.rate),
        };
        if let Some(r) = rate {
            rates.insert(code, r);
        }
    }
    Ok(rates)
}

/// Foreign balances restated at closing rates, for review. Nothing is posted.
pub fn revaluation(slot: &Slot, as_at: &str, rates: &BTreeMap<String, String>) -> ApiResult<Vec<RevaluationDto>> {
    let book = open_ref(slot)?;
    let as_at = parse_date(as_at)?;
    let rates = closing_rates(book, as_at, rates)?;
    Ok(core_fx::revaluation(book, as_at, &rates, base_decimals(book))
        .into_iter()
        .map(|r| RevaluationDto {
            ledger_id: r.ledger.0.to_string(),
            name: r.name,
            currency: r.currency,
            foreign: amt(r.foreign),
            book: amt(r.book),
            rate: r.rate.normalize().to_string(),
            revalued: amt(r.revalued),
            difference: amt(r.difference),
        })
        .collect())
}

/// Post the revaluation journal the user reviewed. Returns its id, or
/// `None` when nothing needed restating.
pub fn post_revaluation(slot: &mut Slot, as_at: &str, rates: &BTreeMap<String, String>) -> ApiResult<Option<String>> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    let as_at = parse_date(as_at)?;
    let rates = closing_rates(book, as_at, rates)?;
    let lines = core_fx::revaluation(book, as_at, &rates, base_decimals(book));
    if lines.iter().all(|r| r.difference.is_zero()) {
        return Ok(None);
    }
    let gl = match gain_loss_ledger(book) {
        Some(id) => id,
        None => book.create_ledger(now(), &os_user(), FOREX_LEDGER, GroupId::default_named("Indirect Expenses")).map_err(err)?,
    };
    let number = book.next_voucher_number(VoucherKind::Journal, as_at);
    let Some(j) = core_fx::revaluation_journal(as_at, number, &lines, gl) else { return Ok(None) };
    let id = book.post_voucher(now(), &os_user(), j).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(Some(id.0.to_string()))
}

pub mod cmd {
    //! Tauri commands for foreign currency (kept here so `commands.rs` stays small).
    use std::collections::BTreeMap;

    use tauri::State;

    use super::*;
    use crate::AppState;

    fn lock<'a>(state: &'a State<'_, AppState>) -> ApiResult<std::sync::MutexGuard<'a, Slot>> {
        state.company.lock().map_err(|_| "company state is poisoned".to_owned())
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_list_currencies(state: State<'_, AppState>) -> ApiResult<CurrenciesDto> {
        list_currencies(&*lock(&state)?)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_save_currency(state: State<'_, AppState>, code: String, symbol: String, name: String, decimals: u32) -> ApiResult<CurrenciesDto> {
        save_currency(&mut *lock(&state)?, &code, &symbol, &name, decimals)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_set_ledger_currency(state: State<'_, AppState>, ledger_id: String, code: String) -> ApiResult<()> {
        set_ledger_currency(&mut *lock(&state)?, &ledger_id, &code)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_rate(state: State<'_, AppState>, code: String, date: String) -> ApiResult<Option<RateDto>> {
        super::fx_rate(&*lock(&state)?, &code, &date)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_set_rate(state: State<'_, AppState>, code: String, date: String, rate: String) -> ApiResult<()> {
        set_fx_rate(&mut *lock(&state)?, &code, &date, &rate)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_revaluation(state: State<'_, AppState>, as_at: String, rates: BTreeMap<String, String>) -> ApiResult<Vec<RevaluationDto>> {
        revaluation(&*lock(&state)?, &as_at, &rates)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn fx_post_revaluation(state: State<'_, AppState>, as_at: String, rates: BTreeMap<String, String>) -> ApiResult<Option<String>> {
        post_revaluation(&mut *lock(&state)?, &as_at, &rates)
    }
}

#[cfg(test)]
mod tests {
    use khata_core::{DrCr, VoucherId};

    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher};
    use crate::api::{EntryInput, create_party_ledger, get_voucher, ledger_statement, outstanding, post_voucher};
    use crate::tax::{set_company_tax, set_ledger_tax};

    fn fx(currency: &str, amount: &str, rate: &str) -> Option<ForexInput> {
        Some(ForexInput { currency: currency.into(), amount: amount.into(), rate: rate.into(), bills: Vec::new() })
    }

    fn with_bill(mut e: EntryInput, kind: khata_core::BillKind, reference: &str) -> EntryInput {
        e.bills = vec![crate::api::BillInput { kind, reference: reference.into(), amount: e.amount.clone(), due: None }];
        e
    }

    /// An Indian exporter with a US customer (USD) and an INR bank.
    fn exporter(name: &str) -> (crate::api::tests::TempFile, Slot, String, String, String) {
        let (f, mut slot) = indian_company(name);
        let debtors = group_id(&slot, "Sundry Debtors");
        let acme = create_party_ledger(&mut slot, "Acme Inc", &debtors, "", None, None, None).unwrap().ledger.id;
        set_ledger_currency(&mut slot, &acme, "usd").unwrap();
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "");
        let sales = ledger(&mut slot, "Export Sales", "Sales Accounts", "");
        (f, slot, acme, bank, sales)
    }

    #[test]
    fn ledgers_take_a_currency_from_the_master() {
        let (_f, mut slot, acme, _bank, sales) = exporter("fx-ledger");
        let book = open_ref(&slot).unwrap();
        assert_eq!(book.ledger(LedgerId(parse_uuid(&acme, "l").unwrap())).unwrap().currency.as_deref(), Some("USD"));
        assert!(set_ledger_currency(&mut slot, &sales, "USD").unwrap_err().contains("party and bank"));
        assert!(set_ledger_currency(&mut slot, &acme, "XYZ").unwrap_err().contains("currency master"));
        // The company's own currency is no foreign currency.
        set_ledger_currency(&mut slot, &acme, "INR").unwrap();
        assert!(crate::masters::get_ledger(&slot, &acme).unwrap().currency.is_none());
        let c = list_currencies(&slot).unwrap();
        assert_eq!(c.base.unwrap().code, "INR");
        assert!(c.currencies.iter().any(|c| c.code == "AED" && c.decimals == 2));
        save_currency(&mut slot, "XAU", "XAU", "Gold (troy ounce)", 4).unwrap();
        assert!(list_currencies(&slot).unwrap().currencies.iter().any(|c| c.code == "XAU"));
    }

    #[test]
    fn e_invoices_refuse_a_foreign_currency_voucher_until_supported() {
        let (_f, mut slot, acme, _bank, sales) = exporter("fx-einvoice");
        let mut party = entry(&acme, DrCr::Dr, "80000");
        party.forex = fx("USD", "1000", "80");
        let mut income = entry(&sales, DrCr::Cr, "80000");
        income.forex = fx("USD", "1000", "80");
        let usd = post_voucher(&mut slot, voucher("Sales", "2026-04-10", vec![party, income])).unwrap();
        let inr = post_voucher(&mut slot, voucher("Sales", "2026-04-11", vec![entry(&acme, DrCr::Dr, "100"), entry(&sales, DrCr::Cr, "100")])).unwrap();
        let book = open_ref(&slot).unwrap();
        let v = |id: &str| book.voucher(VoucherId(parse_uuid(id, "v").unwrap())).unwrap();
        assert!(refuse_foreign_einvoice(v(&usd.id), "A ZATCA e-invoice").unwrap_err().contains("in USD"));
        assert!(refuse_foreign_einvoice(v(&inr.id), "A ZATCA e-invoice").is_ok());
    }

    #[test]
    fn a_dollar_invoice_and_its_settlement_book_the_realised_gain() {
        let (_f, mut slot, acme, bank, sales) = exporter("fx-settle");
        let mut party = entry(&acme, DrCr::Dr, "80000");
        party.forex = fx("USD", "1000", "80");
        let mut income = entry(&sales, DrCr::Cr, "80000");
        income.forex = fx("USD", "1000", "80");
        let inv = post_voucher(&mut slot, voucher("Sales", "2026-04-10", vec![with_bill(party, khata_core::BillKind::New, "EXP-1"), income])).unwrap();
        let open = outstanding(&slot, "2026-06-30", "receivable", None).unwrap();
        assert_eq!((open[0].amount.as_str(), open[0].forex_currency.as_deref(), open[0].forex_amount.as_deref()), ("80000", Some("USD"), Some("1000")));
        let v = get_voucher(&slot, &inv.id).unwrap();
        assert_eq!(v.entries[0].forex.as_ref().unwrap().bills, ["1000"]);
        // The rate is remembered for the next voucher.
        assert_eq!(fx_rate(&slot, "USD", "2026-05-02").unwrap().unwrap().rate, "80");

        // $1,000 received at 83: the bank gets ₹83,000, the bill closes at ₹80,000.
        let mut paid = entry(&acme, DrCr::Cr, "83000");
        paid.forex = fx("USD", "1000", "83");
        let r = post_voucher(&mut slot, voucher("Receipt", "2026-05-02", vec![entry(&bank, DrCr::Dr, "83000"), with_bill(paid, khata_core::BillKind::Against, "EXP-1")])).unwrap();
        let book = open_ref(&slot).unwrap();
        let gl = gain_loss_ledger(book).expect("created on the first gain");
        assert_eq!(book.balances(None)[&gl], Amount::from(-3000));
        assert!(outstanding(&slot, "2026-06-30", "all", None).unwrap().is_empty());
        let view = get_voucher(&slot, &r.id).unwrap();
        assert_eq!(view.entries.iter().filter(|e| e.forex_gain).count(), 1, "the gain line is marked");
        let s = ledger_statement(&slot, &acme, "2026-04-01", "2026-06-30").unwrap();
        let fx_bal = s.statement.forex.unwrap();
        assert_eq!((fx_bal.currency.as_str(), fx_bal.closing), ("USD", Amount::ZERO));

        // Altered to 82 (sent without the old gain line, as the voucher screen does).
        let mut paid = entry(&acme, DrCr::Cr, "82000");
        paid.forex = fx("USD", "1000", "82");
        crate::api::alter_voucher(&mut slot, &r.id, voucher("Receipt", "2026-05-02", vec![entry(&bank, DrCr::Dr, "82000"), with_bill(paid, khata_core::BillKind::Against, "EXP-1")])).unwrap();
        let book = open_ref(&slot).unwrap();
        assert_eq!(book.balances(None)[&gl], Amount::from(-2000));
        assert_eq!(book.voucher(VoucherId(parse_uuid(&r.id, "v").unwrap())).unwrap().entries.len(), 3);
    }

    #[test]
    fn foreign_input_is_checked() {
        let (_f, mut slot, acme, _bank, sales) = exporter("fx-input");
        let post = |slot: &mut Slot, f: Option<ForexInput>| {
            let mut party = entry(&acme, DrCr::Dr, "100");
            party.forex = f;
            post_voucher(slot, voucher("Sales", "2026-04-10", vec![party, entry(&sales, DrCr::Cr, "100")]))
        };
        assert!(post(&mut slot, fx("INR", "1", "100")).unwrap_err().contains("own currency"));
        assert!(post(&mut slot, fx("ZZZ", "1", "100")).unwrap_err().contains("currency master"));
        assert!(post(&mut slot, fx("USD", "1.234", "100")).unwrap_err().contains("decimal places"));
        assert!(post(&mut slot, fx("USD", "1", "0")).unwrap_err().contains("above zero"));
        assert!(post(&mut slot, fx("EUR", "1", "100")).unwrap_err().contains("kept in USD"));
        post(&mut slot, fx("usd", "1.25", "80")).unwrap();
    }

    #[test]
    fn a_dollar_invoice_prints_in_dollars_with_the_tax_in_rupees() {
        let (_f, mut slot, acme, _bank, sales) = exporter("fx-print");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        let igst = ledger(&mut slot, "Output IGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &igst, "IGST", khata_core::TaxSide::Output).unwrap();
        // $1,000 of services with IGST at 18% paid on export: tax on the INR value.
        let mut party = entry(&acme, DrCr::Dr, "97940");
        party.forex = fx("USD", "1180", "83");
        let mut income = entry(&sales, DrCr::Cr, "83000");
        income.forex = fx("USD", "1000", "83");
        let mut tax = entry(&igst, DrCr::Cr, "14940");
        tax.forex = fx("USD", "180", "83");
        let inv = post_voucher(&mut slot, voucher("Sales", "2026-05-02", vec![party, income, tax])).unwrap();
        let book = open_ref(&slot).unwrap();
        let v = book.voucher(VoucherId(parse_uuid(&inv.id, "v").unwrap())).unwrap();
        let doc = crate::invoice::invoice_doc(book, v);
        assert_eq!(doc.total, Amount::from(1180));
        assert_eq!(doc.lines[0].amount, Amount::from(1000));
        assert_eq!(doc.taxes[0].1, Amount::from(180));
        assert!(doc.total_in_words.contains("US Dollars"), "{}", doc.total_in_words);
        let c = doc.currency.unwrap();
        assert_eq!((c.code.as_str(), c.base_code.as_str(), c.rate, c.base_total, c.base_tax), ("USD", "INR", Amount::from(83), Amount::from(97940), Some(Amount::from(14940))));
        // A rupee invoice prints as before.
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        let local = post_voucher(&mut slot, voucher("Sales", "2026-05-03", vec![entry(&sales, DrCr::Cr, "100"), entry(&walk_in, DrCr::Dr, "100")])).unwrap();
        let book = open_ref(&slot).unwrap();
        assert!(crate::invoice::invoice_doc(book, book.voucher(VoucherId(parse_uuid(&local.id, "v").unwrap())).unwrap()).currency.is_none());
    }

    #[test]
    fn revaluation_is_suggested_and_posted_only_when_asked() {
        let (_f, mut slot, acme, _bank, sales) = exporter("fx-reval");
        let mut party = entry(&acme, DrCr::Dr, "80000");
        party.forex = fx("USD", "1000", "80");
        post_voucher(&mut slot, voucher("Sales", "2026-04-10", vec![party, entry(&sales, DrCr::Cr, "80000")])).unwrap();
        set_fx_rate(&mut slot, "USD", "2026-06-30", "82.75").unwrap();
        let rows = revaluation(&slot, "2026-06-30", &BTreeMap::new()).unwrap();
        assert_eq!((rows[0].rate.as_str(), rows[0].difference.as_str()), ("82.75", "2750"));
        assert!(gain_loss_ledger(open_ref(&slot).unwrap()).is_none(), "nothing posted by looking");
        let given = BTreeMap::from([("USD".to_owned(), "81".to_owned())]);
        assert_eq!(revaluation(&slot, "2026-06-30", &given).unwrap()[0].difference, "1000");
        let id = post_revaluation(&mut slot, "2026-06-30", &BTreeMap::new()).unwrap().unwrap();
        let v = get_voucher(&slot, &id).unwrap();
        assert_eq!((v.kind, v.entries.len()), (VoucherKind::Journal, 2));
        assert_eq!(post_revaluation(&mut slot, "2026-06-30", &BTreeMap::new()).unwrap(), None, "already restated");
    }
}

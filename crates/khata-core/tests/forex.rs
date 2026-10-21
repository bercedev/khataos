//! Foreign currency: posting, settlement gain/loss, revaluation, remembered
//! rates, and the audit-trail compatibility of all of it.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::forex::{self, Currency, FOREX_LEDGER};
use khata_core::reports::{ledger_statement, outstanding_bills};
use khata_core::{AuditEntry, AuditLog, BillKind, Book, BookError, Entry, LedgerId, Voucher, VoucherError, VoucherId, VoucherKind};
use rust_decimal_macros::dec;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

fn voucher(kind: VoucherKind, number: &str, date: NaiveDate, entries: Vec<Entry>) -> Voucher {
    Voucher { id: VoucherId::new(), kind, number: number.into(), date, entries, narration: String::new(), cancelled: false, series: None, stock: Vec::new() }
}

struct Co {
    book: Book,
    acme: LedgerId,
    supplier: LedgerId,
    bank: LedgerId,
    sales: LedgerId,
    purchase: LedgerId,
    gain_loss: LedgerId,
}

/// An Indian company with a US customer and a German supplier.
fn company() -> Co {
    let mut book = Book::new("Exporters");
    book.set_country(now(), "owner", "IN").unwrap();
    let g = |b: &Book, n: &str| b.group_by_name(n).unwrap().id;
    let acme = book.create_ledger(now(), "owner", "Acme Inc", g(&book, "Sundry Debtors")).unwrap();
    let supplier = book.create_ledger(now(), "owner", "Berlin GmbH", g(&book, "Sundry Creditors")).unwrap();
    let bank = book.create_ledger(now(), "owner", "HDFC Bank", g(&book, "Bank Accounts")).unwrap();
    let sales = book.create_ledger(now(), "owner", "Export Sales", g(&book, "Sales Accounts")).unwrap();
    let purchase = book.create_ledger(now(), "owner", "Imports", g(&book, "Purchase Accounts")).unwrap();
    let gain_loss = book.create_ledger(now(), "owner", FOREX_LEDGER, g(&book, "Indirect Expenses")).unwrap();
    for (id, code) in [(acme, "USD"), (supplier, "EUR")] {
        let l = khata_core::Ledger { currency: Some(code.into()), ..book.ledger(id).unwrap().clone() };
        book.alter_ledger(now(), "owner", l).unwrap();
    }
    Co { book, acme, supplier, bank, sales, purchase, gain_loss }
}

/// Sales invoice EXP-1: $1,000 at 80 = ₹80,000.
fn invoice(c: &mut Co) -> VoucherId {
    let v = voucher(VoucherKind::Sales, "1", d(4, 10), vec![
        Entry::dr(c.acme, dec!(80000)).fx("USD", dec!(1000), dec!(80)).fx_bill(BillKind::New, "EXP-1", dec!(80000), dec!(1000), Some(d(5, 10))),
        Entry::cr(c.sales, dec!(80000)).fx("USD", dec!(1000), dec!(80)),
    ]);
    c.book.post_voucher(now(), "clerk", v).unwrap()
}

/// A receipt of `usd` against EXP-1 at `rate`, as the app builds it before settlement.
fn receipt(c: &Co, number: &str, usd: rust_decimal::Decimal, rate: rust_decimal::Decimal) -> Voucher {
    let base = (usd * rate).round_dp(2);
    voucher(VoucherKind::Receipt, number, d(5, 2), vec![
        Entry::dr(c.bank, base),
        Entry::cr(c.acme, base).fx("USD", usd, rate).fx_bill(BillKind::Against, "EXP-1", base, usd, None),
    ])
}

// ------------------------------------------------- audit-trail compatibility

fn fixture() -> Vec<AuditEntry> {
    serde_json::from_str(include_str!("fixtures/pre_forex_trail.json")).unwrap()
}

/// `tests/fixtures/pre_forex_trail.json` was written by KhataOS before
/// foreign currency existed (ledgers, a party bill, an altered and a
/// cancelled voucher). It must still replay, verify and keep its hashes.
#[test]
fn a_trail_written_before_foreign_currency_still_replays_and_verifies() {
    let entries = fixture();
    let raw = include_str!("fixtures/pre_forex_trail.json");
    assert!(!raw.contains("forex") && !raw.contains("currency") && !raw.contains("fx_rates"), "the fixture predates the new fields");
    let log = AuditLog::from(entries.clone());
    assert_eq!(log.verify(), Ok(()), "every stored hash still matches");
    let book = Book::replay("Old Format Traders", log).unwrap();
    assert_eq!(book.audit().entries(), entries.as_slice(), "replay keeps every entry, hashes included");
    assert_eq!(book.audit().entries().last().unwrap().hash, "2e795dbc787e5b0433a8225f12132ef0be8a9f111a0891d2d20c28c767c9b7ad");
    // Old records serialise exactly as they were stored, so the next Alter's
    // `before` matches: the new optional fields add nothing to old JSON.
    for e in &entries {
        let Some(after) = &e.after else { continue };
        let (kind, _) = e.entity.split_once(':').unwrap();
        let again = match kind {
            "voucher" => serde_json::to_value(serde_json::from_value::<Voucher>(after.clone()).unwrap()).unwrap(),
            "ledger" => serde_json::to_value(serde_json::from_value::<khata_core::Ledger>(after.clone()).unwrap()).unwrap(),
            "profile" => serde_json::to_value(serde_json::from_value::<khata_core::Profile>(after.clone()).unwrap()).unwrap(),
            _ => continue,
        };
        assert_eq!(&again, after, "entry {} round-trips byte for byte", e.seq);
    }
    let bill = &outstanding_bills(&book, d(6, 30))[0];
    assert_eq!((bill.reference.as_str(), bill.amount, bill.forex.clone()), ("EXP-1", dec!(80000), None));
}

#[test]
fn an_old_trail_takes_new_foreign_entries_and_replays_again() {
    let mut book = Book::replay("Old Format Traders", AuditLog::from(fixture())).unwrap();
    // Alter an old ledger and an old voucher with the new code.
    let party = book.ledger_by_name("Acme Inc (USA)").unwrap().clone();
    book.alter_ledger(now(), "owner", khata_core::Ledger { currency: Some("USD".into()), ..party.clone() }).unwrap();
    let old = book.vouchers().find(|v| v.kind == VoucherKind::Sales).unwrap().clone();
    let mut v = old.clone();
    v.narration = "Export of services (LUT), USD invoice".into();
    book.alter_voucher(now(), "owner", v).unwrap();
    book.set_fx_rate(now(), "owner", "USD", d(4, 1), Some(dec!(83.5))).unwrap();
    let replayed = Book::replay("Old Format Traders", AuditLog::from(book.audit().entries().to_vec())).unwrap();
    assert_eq!(replayed.audit().verify(), Ok(()));
    assert_eq!(replayed.audit().entries()[..10], fixture()[..], "the old part of the trail is untouched");
    assert_eq!(replayed.ledger(party.id).unwrap().currency.as_deref(), Some("USD"));
    assert_eq!(replayed.fx_rate("USD", d(4, 2)).unwrap().rate, dec!(83.5));
}

#[test]
fn vouchers_without_foreign_amounts_keep_their_old_json() {
    let c = company();
    let v = voucher(VoucherKind::Payment, "1", d(4, 1), vec![Entry::dr(c.sales, dec!(10)).bill(BillKind::OnAccount, "", dec!(10), None), Entry::cr(c.bank, dec!(10))]);
    let text = serde_json::to_string(&v).unwrap();
    assert!(!text.contains("forex"), "{text}");
    let l = serde_json::to_string(c.book.ledger(c.bank).unwrap()).unwrap();
    assert!(!l.contains("currency"), "{l}");
}

// ------------------------------------------------------------- posting

#[test]
fn a_foreign_invoice_posts_in_base_and_keeps_its_foreign_amounts() {
    let mut c = company();
    let id = invoice(&mut c);
    // The books balance in rupees; the dollars ride along.
    assert_eq!(c.book.balances(None)[&c.acme], dec!(80000));
    assert_eq!(c.book.voucher(id).unwrap().entries[0].forex.as_ref().unwrap().amount, dec!(1000));
    let bills = outstanding_bills(&c.book, d(6, 30));
    assert_eq!(bills.len(), 1);
    assert_eq!((bills[0].amount, bills[0].forex.clone()), (dec!(80000), Some(("USD".into(), dec!(1000)))));
    let s = ledger_statement(&c.book, c.acme, d(4, 1), d(6, 30)).unwrap();
    assert_eq!(s.rows[0].forex, Some(dec!(1000)));
    let fx = s.forex.unwrap();
    assert_eq!((fx.currency.as_str(), fx.opening, fx.closing), ("USD", dec!(0), dec!(1000)));
    // A base ledger's statement has no foreign balances.
    assert!(ledger_statement(&c.book, c.bank, d(4, 1), d(6, 30)).unwrap().forex.is_none());
    // The rate is remembered for the next voucher.
    let r = c.book.fx_rate("USD", d(4, 20)).unwrap();
    assert_eq!((r.date, r.rate, r.from_voucher), (d(4, 10), dec!(80), true));
    assert!(c.book.fx_rate("USD", d(4, 9)).is_none(), "not before it was used");
    assert!(c.book.fx_rate("EUR", d(4, 20)).is_none());
}

#[test]
fn foreign_amounts_are_checked() {
    let mut c = company();
    let sales = c.sales;
    let bad = |c: &mut Co, entries: Vec<Entry>| c.book.post_voucher(now(), "clerk", voucher(VoucherKind::Sales, "9", d(4, 1), entries)).unwrap_err();
    // Not a currency code, a zero rate, bills that don't add up in dollars.
    for e in [
        Entry::dr(c.acme, dec!(100)).fx("usd", dec!(1), dec!(100)),
        Entry::dr(c.acme, dec!(100)).fx("USD", dec!(1), dec!(0)),
        Entry::dr(c.acme, dec!(100)).fx("USD", dec!(1), dec!(100)).fx_bill(BillKind::New, "X", dec!(100), dec!(2), None),
        Entry::dr(c.acme, dec!(100)).fx_bill(BillKind::New, "X", dec!(100), dec!(1), None),
    ] {
        assert_eq!(bad(&mut c, vec![e, Entry::cr(sales, dec!(100))]), BookError::Voucher(VoucherError::Forex { index: 0 }));
    }
    // Half the bills with a foreign amount, half without.
    let mixed = Entry::dr(c.acme, dec!(200)).fx("USD", dec!(2), dec!(100))
        .fx_bill(BillKind::New, "X", dec!(100), dec!(2), None)
        .bill(BillKind::New, "Y", dec!(100), None);
    assert_eq!(bad(&mut c, vec![mixed, Entry::cr(sales, dec!(200))]), BookError::Voucher(VoucherError::Forex { index: 0 }));
    // A dollar ledger takes no euros.
    let euros = Entry::dr(c.acme, dec!(100)).fx("EUR", dec!(1), dec!(100));
    assert!(matches!(bad(&mut c, vec![euros, Entry::cr(sales, dec!(100))]), BookError::Currency(m) if m.contains("kept in USD")));
    // A base-only line on a foreign ledger is fine (a gain/loss or revaluation).
    c.book.post_voucher(now(), "clerk", voucher(VoucherKind::Journal, "1", d(4, 1), vec![Entry::dr(c.acme, dec!(5)), Entry::cr(c.gain_loss, dec!(5))])).unwrap();
    // Once it has dollar vouchers, the ledger can't switch to euros.
    invoice(&mut c);
    let l = khata_core::Ledger { currency: Some("EUR".into()), ..c.book.ledger(c.acme).unwrap().clone() };
    assert!(matches!(c.book.alter_ledger(now(), "owner", l), Err(BookError::Currency(_))));
    let l = khata_core::Ledger { currency: Some("US$".into()), ..c.book.ledger(c.bank).unwrap().clone() };
    assert!(matches!(c.book.alter_ledger(now(), "owner", l), Err(BookError::Currency(_))));
}

// ---------------------------------------------------------- settlement

#[test]
fn settling_at_a_higher_rate_books_a_realised_gain() {
    let mut c = company();
    invoice(&mut c);
    let mut r = receipt(&c, "1", dec!(1000), dec!(83));
    let s = forex::settle(&c.book, &mut r, c.gain_loss, 2);
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].carrying, s[0].settled, s[0].gain), (dec!(80000), dec!(83000), dec!(3000)));
    // Party at the carrying amount, the gain credited, the bank as received.
    assert_eq!(r.entries[1].amount, dec!(80000));
    assert_eq!(r.entries[1].forex.as_ref().unwrap().rate, dec!(83), "the rate of the voucher is kept");
    let gl = r.entries.iter().find(|e| e.ledger == c.gain_loss).unwrap();
    assert_eq!((gl.side, gl.amount), (khata_core::DrCr::Cr, dec!(3000)));
    // Settling twice changes nothing more.
    let again = r.clone();
    forex::settle(&c.book, &mut r, c.gain_loss, 2);
    assert_eq!(r, again);
    c.book.post_voucher(now(), "clerk", r).unwrap();
    assert!(outstanding_bills(&c.book, d(6, 30)).is_empty(), "closed in rupees and in dollars");
    assert_eq!(c.book.balances(None)[&c.gain_loss], dec!(-3000));
    assert_eq!(c.book.balances(None)[&c.bank], dec!(83000));
}

#[test]
fn part_payments_at_a_lower_rate_book_a_loss_and_leave_the_rest_open() {
    let mut c = company();
    invoice(&mut c);
    let mut r = receipt(&c, "1", dec!(400), dec!(78.5));
    let s = forex::settle(&c.book, &mut r, c.gain_loss, 2);
    assert_eq!((s[0].carrying, s[0].settled, s[0].gain), (dec!(32000), dec!(31400), dec!(-600)));
    let gl = r.entries.iter().find(|e| e.ledger == c.gain_loss).unwrap();
    assert_eq!((gl.side, gl.amount), (khata_core::DrCr::Dr, dec!(600)));
    c.book.post_voucher(now(), "clerk", r).unwrap();
    let open = &outstanding_bills(&c.book, d(6, 30))[0];
    assert_eq!((open.amount, open.forex.clone()), (dec!(48000), Some(("USD".into(), dec!(600)))));
    // The rest at 81: the bill's whole remaining carrying amount closes.
    let mut r = receipt(&c, "2", dec!(600), dec!(81));
    let s = forex::settle(&c.book, &mut r, c.gain_loss, 2);
    assert_eq!((s[0].carrying, s[0].gain), (dec!(48000), dec!(600)));
    c.book.post_voucher(now(), "clerk", r).unwrap();
    assert!(outstanding_bills(&c.book, d(6, 30)).is_empty());
    assert_eq!(c.book.balances(None)[&c.gain_loss], dec!(0), "a loss of 600 and a gain of 600");
}

#[test]
fn paying_a_foreign_supplier_at_a_higher_rate_is_a_loss() {
    let mut c = company();
    let bill = voucher(VoucherKind::Purchase, "1", d(4, 5), vec![
        Entry::dr(c.purchase, dec!(90000)),
        Entry::cr(c.supplier, dec!(90000)).fx("EUR", dec!(1000), dec!(90)).fx_bill(BillKind::New, "B-7", dec!(90000), dec!(1000), None),
    ]);
    c.book.post_voucher(now(), "clerk", bill).unwrap();
    let mut pay = voucher(VoucherKind::Payment, "1", d(5, 5), vec![
        Entry::dr(c.supplier, dec!(92500)).fx("EUR", dec!(1000), dec!(92.5)).fx_bill(BillKind::Against, "B-7", dec!(92500), dec!(1000), None),
        Entry::cr(c.bank, dec!(92500)),
    ]);
    let s = forex::settle(&c.book, &mut pay, c.gain_loss, 2);
    assert_eq!(s[0].gain, dec!(-2500));
    c.book.post_voucher(now(), "clerk", pay).unwrap();
    assert_eq!(c.book.balances(None)[&c.gain_loss], dec!(2500), "a loss is a debit");
    assert!(outstanding_bills(&c.book, d(6, 30)).is_empty());
}

#[test]
fn altering_a_settlement_rebooks_its_gain() {
    let mut c = company();
    invoice(&mut c);
    let mut r = receipt(&c, "1", dec!(1000), dec!(83));
    forex::settle(&c.book, &mut r, c.gain_loss, 2);
    let id = c.book.post_voucher(now(), "clerk", r.clone()).unwrap();
    // The app sends the altered receipt without its old gain line, at the new rate.
    let mut altered = receipt(&c, "1", dec!(1000), dec!(82));
    altered.id = id;
    let s = forex::settle(&c.book, &mut altered, c.gain_loss, 2);
    assert_eq!(s[0].gain, dec!(2000), "the voucher's own bill allocation is left out");
    c.book.alter_voucher(now(), "owner", altered).unwrap();
    assert_eq!(c.book.balances(None)[&c.gain_loss], dec!(-2000));
    // A receipt for a base-currency bill is left alone.
    let mut plain = voucher(VoucherKind::Receipt, "2", d(5, 3), vec![Entry::dr(c.bank, dec!(10)), Entry::cr(c.sales, dec!(10))]);
    assert!(forex::settle(&c.book, &mut plain, c.gain_loss, 2).is_empty());
    assert_eq!(plain.entries.len(), 2);
}

// --------------------------------------------- master, rates, revaluation

#[test]
fn the_currency_master_and_rate_table_live_in_the_profile() {
    let mut c = company();
    assert_eq!(c.book.currency("USD").unwrap().symbol, "$");
    assert_eq!(c.book.currency("KWD").unwrap().decimals, 3);
    assert_eq!(c.book.currency("JPY").unwrap().decimals, 0);
    assert!(c.book.currency("XYZ").is_none());
    let x = Currency { code: "XYZ".into(), symbol: "X".into(), name: "Example".into(), decimals: 2 };
    c.book.save_currency(now(), "owner", x.clone()).unwrap();
    assert_eq!(c.book.currency("XYZ"), Some(x));
    assert!(c.book.currencies().iter().any(|c| c.code == "XYZ") && c.book.currencies().len() > 40);
    let bad = Currency { code: "ABC".into(), symbol: "A".into(), name: "Bad".into(), decimals: 9 };
    assert!(matches!(c.book.save_currency(now(), "owner", bad), Err(BookError::Currency(_))));
    assert!(matches!(c.book.set_fx_rate(now(), "owner", "USD", d(4, 1), Some(dec!(-1))), Err(BookError::Currency(_))));
    // The table and vouchers both remember rates; on one day the table wins.
    c.book.set_fx_rate(now(), "owner", "USD", d(4, 10), Some(dec!(80.25))).unwrap();
    invoice(&mut c);
    let r = c.book.fx_rate("USD", d(4, 10)).unwrap();
    assert_eq!((r.rate, r.from_voucher), (dec!(80.25), false));
    c.book.set_fx_rate(now(), "owner", "USD", d(4, 10), None).unwrap();
    assert_eq!(c.book.fx_rate("USD", d(4, 10)).unwrap().rate, dec!(80));
    let replayed = Book::replay("Exporters", AuditLog::from(c.book.audit().entries().to_vec())).unwrap();
    assert_eq!(replayed.currency("XYZ").unwrap().name, "Example");
}

#[test]
fn revaluation_suggests_a_journal_for_unrealised_differences() {
    let mut c = company();
    invoice(&mut c);
    let rates = BTreeMap::from([("USD".to_owned(), dec!(82.5))]);
    let lines = forex::revaluation(&c.book, d(6, 30), &rates, 2);
    assert_eq!(lines.len(), 1, "only ledgers in a currency with a rate");
    let r = &lines[0];
    assert_eq!((r.foreign, r.book, r.revalued, r.difference), (dec!(1000), dec!(80000), dec!(82500), dec!(2500)));
    let j = forex::revaluation_journal(d(6, 30), "R-1".into(), &lines, c.gain_loss).unwrap();
    assert!(c.book.balances(None).get(&c.gain_loss).is_none_or(|b| b.is_zero()), "nothing posted by itself");
    c.book.post_voucher(now(), "owner", j).unwrap();
    assert_eq!(c.book.balances(None)[&c.acme], dec!(82500));
    assert_eq!(c.book.balances(None)[&c.gain_loss], dec!(-2500), "an unrealised gain");
    assert!(forex::revaluation(&c.book, d(6, 30), &rates, 2)[0].difference.is_zero());
    assert!(forex::revaluation_journal(d(6, 30), "R-2".into(), &forex::revaluation(&c.book, d(6, 30), &rates, 2), c.gain_loss).is_none());
}

#[test]
fn a_trail_with_foreign_amounts_replays_to_the_same_books() {
    let mut c = company();
    invoice(&mut c);
    let mut r = receipt(&c, "1", dec!(1000), dec!(83));
    forex::settle(&c.book, &mut r, c.gain_loss, 2);
    c.book.post_voucher(now(), "clerk", r).unwrap();
    let replayed = Book::replay("Exporters", AuditLog::from(c.book.audit().entries().to_vec())).unwrap();
    assert_eq!(replayed.trial_balance(), c.book.trial_balance());
    assert_eq!(outstanding_bills(&replayed, d(6, 30)), outstanding_bills(&c.book, d(6, 30)));
    assert_eq!(replayed.vouchers().collect::<Vec<_>>(), c.book.vouchers().collect::<Vec<_>>());
}

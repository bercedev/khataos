//! Two devices and an external viewer syncing through a real server.

use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{Book, Entry, Voucher, VoucherId, VoucherKind};
use khata_store::Store;
use khata_sync::{Remote, SyncError, SyncOutcome, sync};

use rust_decimal_macros::dec;
use serde_json::{Value, json};

const ADMIN: &str = "test-admin-key-0123456789abcdef";

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("khata-sync-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    let _ = std::fs::remove_file(&p);
    p
}

/// Start a server on a free port in a background runtime; returns its URL.
fn start_server(name: &str) -> String {
    let state = khata_server::AppState::open(khata_server::Config { data_dir: tmp(name), admin_key: ADMIN.into() })
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(listener, khata_server::router(state)).await.unwrap();
        });
    });
    format!("http://{addr}")
}

fn post(url: &str, token: &str, body: Value) -> Value {
    let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
    agent
        .post(url)
        .header("authorization", &format!("Bearer {token}"))
        .send_json(body)
        .unwrap()
        .body_mut()
        .read_json()
        .unwrap()
}

fn pay(book: &mut Book, number: &str, amount: rust_decimal::Decimal) {
    let rent = book.ledger_by_name("Rent").unwrap().id;
    let bank = book.ledger_by_name("HDFC Bank").unwrap().id;
    book.post_voucher(
        now(),
        "clerk",
        Voucher {
            id: VoucherId::new(),
            kind: VoucherKind::Payment,
            number: number.into(),
            date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            entries: vec![Entry::dr(rent, amount), Entry::cr(bank, amount)],
            narration: String::new(), cancelled: false, series: None, stock: Vec::new(),
        },
    )
    .unwrap();
}

#[test]
fn two_devices_and_a_viewer() {
    let url = start_server("server");
    let created = post(&format!("{url}/v1/companies"), ADMIN, json!({ "name": "Demo Traders" }));
    let company_id = created["company_id"].as_str().unwrap().to_owned();
    let owner = Remote { url: url.clone(), company_id: company_id.clone(), token: created["owner_token"].as_str().unwrap().into() };

    // Device A: create books and push.
    let path_a = tmp("a.khata");
    let mut store_a = Store::create(&path_a, "Demo Traders").unwrap();
    let mut book_a = Book::new("Demo Traders");
    book_a.create_ledger(now(), "owner", "HDFC Bank", book_a.group_by_name("Bank Accounts").unwrap().id).unwrap();
    book_a.create_ledger(now(), "owner", "Rent", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    pay(&mut book_a, "1", dec!(25000));
    assert_eq!(sync(&mut store_a, &mut book_a, &owner).unwrap(), SyncOutcome::Pushed(3));
    assert_eq!(sync(&mut store_a, &mut book_a, &owner).unwrap(), SyncOutcome::UpToDate);

    // Device B: an accountant on a new machine pulls everything.
    let share = post(
        &format!("{url}/v1/companies/{company_id}/shares"),
        &owner.token,
        json!({ "role": "accountant", "label": "staff laptop", "expires_in_days": null }),
    );
    let accountant = Remote { token: share["token"].as_str().unwrap().into(), ..owner.clone() };
    let path_b = tmp("b.khata");
    let mut store_b = Store::create(&path_b, "Demo Traders").unwrap();
    let mut book_b = Book::new("Demo Traders");
    assert_eq!(sync(&mut store_b, &mut book_b, &accountant).unwrap(), SyncOutcome::Pulled(3));
    assert_eq!(book_b.trial_balance(), book_a.trial_balance());

    // B posts; A pulls it.
    pay(&mut book_b, "2", dec!(1000));
    assert_eq!(sync(&mut store_b, &mut book_b, &accountant).unwrap(), SyncOutcome::Pushed(1));
    assert_eq!(sync(&mut store_a, &mut book_a, &owner).unwrap(), SyncOutcome::Pulled(1));
    assert_eq!(book_a.vouchers().count(), 2);

    // Both edit offline → conflict, and nothing is lost or rewritten.
    pay(&mut book_a, "3", dec!(10));
    pay(&mut book_b, "4", dec!(20));
    assert_eq!(sync(&mut store_b, &mut book_b, &accountant).unwrap(), SyncOutcome::Pushed(1));
    assert!(matches!(sync(&mut store_a, &mut book_a, &owner), Err(SyncError::Conflict { local: 1, remote: 1 })));
    assert_eq!(store_a.len().unwrap(), 5, "A's offline voucher is still stored locally");

    // An external viewer can read but not write.
    let share = post(
        &format!("{url}/v1/companies/{company_id}/shares"),
        &owner.token,
        json!({ "role": "viewer", "label": "auditor", "expires_in_days": 7 }),
    );
    let viewer = Remote { token: share["token"].as_str().unwrap().into(), ..owner.clone() };
    let path_v = tmp("v.khata");
    let mut store_v = Store::create(&path_v, "Demo Traders").unwrap();
    let mut book_v = Book::new("Demo Traders");
    assert_eq!(sync(&mut store_v, &mut book_v, &viewer).unwrap(), SyncOutcome::Pulled(5));
    pay(&mut book_v, "99", dec!(1));
    assert!(matches!(sync(&mut store_v, &mut book_v, &viewer), Err(SyncError::Server { status: 403, .. })));
}

#[test]
fn a_file_stays_linked_to_its_company() {
    let url = start_server("linked");
    let a = post(&format!("{url}/v1/companies"), ADMIN, json!({ "name": "A" }));
    let b = post(&format!("{url}/v1/companies"), ADMIN, json!({ "name": "B" }));
    let remote = |c: &Value| Remote {
        url: url.clone(),
        company_id: c["company_id"].as_str().unwrap().into(),
        token: c["owner_token"].as_str().unwrap().into(),
    };
    let path = tmp("linked.khata");
    let mut store = Store::create(&path, "A").unwrap();
    let mut book = Book::new("A");
    book.create_ledger(now(), "o", "Cash box", book.group_by_name("Cash-in-Hand").unwrap().id).unwrap();
    sync(&mut store, &mut book, &remote(&a)).unwrap();
    assert!(matches!(sync(&mut store, &mut book, &remote(&b)), Err(SyncError::WrongCompany { .. })));
}

#[test]
fn offline_conflict_is_rebased_without_losing_anything() {
    let url = start_server("rebase");
    let created = post(&format!("{url}/v1/companies"), ADMIN, json!({ "name": "Demo Traders" }));
    let company_id = created["company_id"].as_str().unwrap().to_owned();
    let owner = Remote { url: url.clone(), company_id: company_id.clone(), token: created["owner_token"].as_str().unwrap().into() };

    let path_a = tmp("rebase-a.khata");
    let mut store_a = Store::create(&path_a, "Demo Traders").unwrap();
    let mut book_a = Book::new("Demo Traders");
    book_a.create_ledger(now(), "owner", "HDFC Bank", book_a.group_by_name("Bank Accounts").unwrap().id).unwrap();
    book_a.create_ledger(now(), "owner", "Rent", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    sync(&mut store_a, &mut book_a, &owner).unwrap();

    let path_b = tmp("rebase-b.khata");
    let mut store_b = Store::create(&path_b, "Demo Traders").unwrap();
    let mut book_b = Book::new("Demo Traders");
    sync(&mut store_b, &mut book_b, &owner).unwrap();

    // Both enter Payment No. 1 offline; B syncs first.
    pay(&mut book_a, "1", dec!(100));
    book_a.create_ledger(now(), "owner", "Electricity", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    pay(&mut book_b, "1", dec!(200));
    sync(&mut store_b, &mut book_b, &owner).unwrap();
    assert!(matches!(sync(&mut store_a, &mut book_a, &owner), Err(SyncError::Conflict { .. })));
    drop(store_a);

    let r = khata_sync::rebase(&path_a, &owner).unwrap();
    assert_eq!(r.renumbered, [(VoucherKind::Payment, "1".to_owned(), "2".to_owned())]);
    assert!(r.needs_attention.is_empty());
    assert_eq!(r.pushed, 2);
    assert_eq!(r.book.vouchers().count(), 2);
    assert!(r.book.ledger_by_name("Electricity").is_some());

    // The pre-rebase file is kept untouched as an archive.
    let archived = Store::open(&r.archived).unwrap();
    assert_eq!(archived.len().unwrap(), 4, "2 ledgers + A's payment + Electricity");

    // B pulls A's rebased work.
    assert_eq!(sync(&mut store_b, &mut book_b, &owner).unwrap(), SyncOutcome::Pulled(2));
    assert_eq!(book_b.trial_balance(), r.book.trial_balance());
}

/// A dollar invoice and its settlement (with the realised gain line) made on
/// one device arrive intact on another, outstanding in both currencies.
#[test]
fn foreign_currency_vouchers_sync_between_devices() {
    use khata_core::{BillKind, Ledger, forex};
    let url = start_server("forex-server");
    let created = post(&format!("{url}/v1/companies"), ADMIN, json!({ "name": "Exporters" }));
    let remote = Remote {
        url: url.clone(),
        company_id: created["company_id"].as_str().unwrap().to_owned(),
        token: created["owner_token"].as_str().unwrap().into(),
    };
    let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    let path_a = tmp("forex-a.khata");
    let mut store_a = Store::create(&path_a, "Exporters").unwrap();
    let mut a = Book::new("Exporters");
    a.set_country(now(), "owner", "IN").unwrap();
    let acme = a.create_ledger(now(), "owner", "Acme Inc", a.group_by_name("Sundry Debtors").unwrap().id).unwrap();
    a.alter_ledger(now(), "owner", Ledger { currency: Some("USD".into()), ..a.ledger(acme).unwrap().clone() }).unwrap();
    let sales = a.create_ledger(now(), "owner", "Export Sales", a.group_by_name("Sales Accounts").unwrap().id).unwrap();
    let v = |kind, number: &str, entries| Voucher { id: VoucherId::new(), kind, number: number.into(), date: d(4, 10), entries, narration: String::new(), cancelled: false, series: None, stock: Vec::new() };
    a.post_voucher(now(), "clerk", v(VoucherKind::Sales, "1", vec![
        Entry::dr(acme, dec!(80000)).fx("USD", dec!(1000), dec!(80)).fx_bill(BillKind::New, "EXP-1", dec!(80000), dec!(1000), None),
        Entry::cr(sales, dec!(80000)),
    ]))
    .unwrap();
    assert!(matches!(sync(&mut store_a, &mut a, &remote).unwrap(), SyncOutcome::Pushed(_)));

    // Device B pulls, takes a part payment at a better rate and pushes it back.
    let path_b = tmp("forex-b.khata");
    let mut store_b = Store::create(&path_b, "Exporters").unwrap();
    let mut b = Book::new("Exporters");
    assert!(matches!(sync(&mut store_b, &mut b, &remote).unwrap(), SyncOutcome::Pulled(_)));
    let bank = b.create_ledger(now(), "clerk", "HDFC Bank", b.group_by_name("Bank Accounts").unwrap().id).unwrap();
    let gl = b.create_ledger(now(), "clerk", forex::FOREX_LEDGER, b.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    let mut r = v(VoucherKind::Receipt, "1", vec![
        Entry::dr(bank, dec!(41500)),
        Entry::cr(acme, dec!(41500)).fx("USD", dec!(500), dec!(83)).fx_bill(BillKind::Against, "EXP-1", dec!(41500), dec!(500), None),
    ]);
    assert_eq!(forex::settle(&b, &mut r, gl, 2)[0].gain, dec!(1500));
    b.post_voucher(now(), "clerk", r).unwrap();
    assert!(matches!(sync(&mut store_b, &mut b, &remote).unwrap(), SyncOutcome::Pushed(3)));
    assert!(matches!(sync(&mut store_a, &mut a, &remote).unwrap(), SyncOutcome::Pulled(3)));
    assert_eq!(a.trial_balance(), b.trial_balance());
    let open = khata_core::reports::outstanding_bills(&a, d(6, 30));
    assert_eq!((open[0].amount, open[0].forex.clone()), (dec!(40000), Some(("USD".into(), dec!(500)))));
    // And it survives reopening from disk.
    let reopened = Store::open(&path_a).unwrap().load().unwrap();
    assert_eq!(reopened.trial_balance(), a.trial_balance());
}

//! End-to-end encrypted sync through a blind relay.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{Book, Entry, Voucher, VoucherId, VoucherKind};
use khata_store::Store;
use khata_sync::{SyncError, SyncOutcome, create_relay_company, move_relay, rebase_sealed, sync_sealed};
use rust_decimal_macros::dec;

fn tmp(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("khata-relay-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    let _ = std::fs::remove_file(&p);
    p
}

fn start_relay(name: &str) -> (String, std::path::PathBuf) {
    let dir = tmp(name);
    let state = khata_server::AppState::open(khata_server::Config { data_dir: dir.clone(), admin_key: "admin-key-0123456789abcdef-xyz".into() }).unwrap();
    state.set_open_relay(true);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async move {
            axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), khata_server::router(state)).await.unwrap();
        });
    });
    (format!("http://{addr}"), dir)
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

#[test]
fn relay_never_sees_the_books_and_cannot_be_read_without_the_key() {
    let (url, dir) = start_relay("blind");
    let (remote, key) = create_relay_company(&url, None).unwrap();

    let path_a = tmp("a.khata");
    let mut store_a = Store::create(&path_a, "Secret Traders").unwrap();
    let mut book_a = Book::new("Secret Traders");
    let bank = book_a.create_ledger(now(), "owner", "Kotak Bank", book_a.group_by_name("Bank Accounts").unwrap().id).unwrap();
    let rent = book_a.create_ledger(now(), "owner", "Office Rent", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    book_a.post_voucher(now(), "owner", Voucher {
        id: VoucherId::new(), kind: VoucherKind::Payment, number: "1".into(),
        date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        entries: vec![Entry::dr(rent, dec!(98765)), Entry::cr(bank, dec!(98765))],
        narration: "April".into(), cancelled: false, series: None, stock: Vec::new(),
    }).unwrap();
    assert_eq!(sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap(), SyncOutcome::Pushed(3));

    // Nothing readable reached the relay's disk. Amounts are looked for as JSON
    // strings ("98765"): bare digits also occur by chance in the hex tokens the
    // relay stores, which made this check fail now and then.
    for f in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(f.unwrap().path()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Kotak") && !text.contains("\"98765\"") && !text.contains("Office Rent"));
    }

    // A second device with the invite (remote + key) gets identical books.
    let path_b = tmp("b.khata");
    let mut store_b = Store::create(&path_b, "Secret Traders").unwrap();
    let mut book_b = Book::new("Secret Traders");
    assert_eq!(sync_sealed(&mut store_b, &mut book_b, &remote, &key).unwrap(), SyncOutcome::Pulled(3));
    assert_eq!(book_b.trial_balance(), book_a.trial_balance());

    // The wrong key cannot read the relay's copy.
    let path_c = tmp("c.khata");
    let mut store_c = Store::create(&path_c, "x").unwrap();
    let mut book_c = Book::new("x");
    let wrong = khata_seal::CompanyKey::generate().unwrap();
    assert!(matches!(sync_sealed(&mut store_c, &mut book_c, &remote, &wrong), Err(SyncError::InvalidFromServer(_))));
}

fn payment(book: &Book, number: &str, amount: rust_decimal::Decimal) -> Voucher {
    let bank = book.ledger_by_name("Kotak Bank").unwrap().id;
    let rent = book.ledger_by_name("Office Rent").unwrap().id;
    Voucher {
        id: VoucherId::new(), kind: VoucherKind::Payment, number: number.into(),
        date: NaiveDate::from_ymd_opt(2026, 4, 2).unwrap(),
        entries: vec![Entry::dr(rent, amount), Entry::cr(bank, amount)],
        narration: String::new(), cancelled: false, series: None, stock: Vec::new(),
    }
}

#[test]
fn offline_edits_on_two_devices_rebase_through_the_relay() {
    let (url, dir) = start_relay("rebase");
    let (remote, key) = create_relay_company(&url, None).unwrap();

    let path_a = tmp("ra.khata");
    let mut store_a = Store::create(&path_a, "Traders").unwrap();
    let mut book_a = Book::new("Traders");
    book_a.create_ledger(now(), "owner", "Kotak Bank", book_a.group_by_name("Bank Accounts").unwrap().id).unwrap();
    book_a.create_ledger(now(), "owner", "Office Rent", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap();

    let path_b = tmp("rb.khata");
    let mut store_b = Store::create(&path_b, "Traders").unwrap();
    let mut book_b = Book::new("Traders");
    sync_sealed(&mut store_b, &mut book_b, &remote, &key).unwrap();

    // Both devices post voucher 1 while offline; A syncs first.
    book_a.post_voucher(now(), "owner", payment(&book_a, "1", dec!(500))).unwrap();
    sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap();
    book_b.post_voucher(now(), "accountant", payment(&book_b, "1", dec!(700))).unwrap();
    assert!(matches!(sync_sealed(&mut store_b, &mut book_b, &remote, &key), Err(SyncError::Conflict { .. })));

    drop(store_b);
    let r = rebase_sealed(&path_b, &remote, &key).unwrap();
    assert_eq!(r.pushed, 1);
    assert_eq!(r.renumbered, vec![(VoucherKind::Payment, "1".to_owned(), "2".to_owned())]);
    assert!(r.needs_attention.is_empty());
    assert!(r.archived.exists(), "the pre-rebase file is kept");

    // A pulls B's renumbered voucher; both books agree, and still nothing
    // readable reached the relay.
    assert_eq!(sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap(), SyncOutcome::Pulled(1));
    assert_eq!(book_a.trial_balance(), r.book.trial_balance());
    for f in std::fs::read_dir(&dir).unwrap() {
        let text = String::from_utf8_lossy(&std::fs::read(f.unwrap().path()).unwrap()).into_owned();
        assert!(!text.contains("Kotak") && !text.contains("Office Rent"));
    }
}

/// Issue an Accountant invite's token for a relay company (as the owner).
fn accountant_token(remote: &khata_sync::Remote) -> String {
    let resp = ureq::post(&format!("{}/v1/companies/{}/shares", remote.url, remote.company_id))
        .header("authorization", &format!("Bearer {}", remote.token))
        .send_json(serde_json::json!({ "role": "accountant", "label": "Ravi" }))
        .unwrap();
    let v: serde_json::Value = resp.into_body().read_json().unwrap();
    v["token"].as_str().unwrap().to_owned()
}

#[test]
fn a_company_moves_to_another_relay_with_a_new_key() {
    let (old_url, old_dir) = start_relay("move-old");
    let (new_url, new_dir) = start_relay("move-new");
    let (old, old_key) = create_relay_company(&old_url, None).unwrap();

    let path_a = tmp("ma.khata");
    let mut store_a = Store::create(&path_a, "Moving Traders").unwrap();
    let mut book_a = Book::new("Moving Traders");
    book_a.create_ledger(now(), "owner", "Kotak Bank", book_a.group_by_name("Bank Accounts").unwrap().id).unwrap();
    book_a.create_ledger(now(), "owner", "Office Rent", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    book_a.post_voucher(now(), "owner", payment(&book_a, "1", dec!(43210))).unwrap();
    sync_sealed(&mut store_a, &mut book_a, &old, &old_key).unwrap();

    // A colleague works through the old relay with an Accountant invite.
    let colleague = khata_sync::Remote { token: accountant_token(&old), ..old.clone() };
    let path_b = tmp("mb.khata");
    let mut store_b = Store::create(&path_b, "Moving Traders").unwrap();
    let mut book_b = Book::new("Moving Traders");
    sync_sealed(&mut store_b, &mut book_b, &colleague, &old_key).unwrap();

    // Moving needs a fully synced file, and a different relay.
    book_a.post_voucher(now(), "owner", payment(&book_a, "2", dec!(100))).unwrap();
    assert!(matches!(
        move_relay(&mut store_a, &mut book_a, &old, &old_key, &new_url, None),
        Err(SyncError::NotSynced { local: 1, remote: 0 })
    ));
    sync_sealed(&mut store_a, &mut book_a, &old, &old_key).unwrap();
    assert_eq!(sync_sealed(&mut store_b, &mut book_b, &colleague, &old_key).unwrap(), SyncOutcome::Pulled(1));
    book_b.post_voucher(now(), "accountant", payment(&book_b, "3", dec!(200))).unwrap();
    sync_sealed(&mut store_b, &mut book_b, &colleague, &old_key).unwrap();
    assert!(matches!(
        move_relay(&mut store_a, &mut book_a, &old, &old_key, &new_url, None),
        Err(SyncError::NotSynced { local: 0, remote: 1 })
    ));
    assert_eq!(sync_sealed(&mut store_a, &mut book_a, &old, &old_key).unwrap(), SyncOutcome::Pulled(1));
    assert!(matches!(
        move_relay(&mut store_a, &mut book_a, &old, &old_key, &format!("{old_url}/"), None),
        Err(SyncError::SameRelay(_))
    ));
    let moved = move_relay(&mut store_a, &mut book_a, &old, &old_key, &new_url, None).unwrap();
    assert_eq!(moved.entries, book_a.audit().entries().len() as u64);
    assert_eq!(moved.revoked, Some(1), "the colleague's old access is revoked");
    assert_ne!(moved.remote.company_id, old.company_id);
    assert_ne!(moved.key, old_key, "a new key");
    // The owner's file now syncs with the new relay.
    assert_eq!(sync_sealed(&mut store_a, &mut book_a, &moved.remote, &moved.key).unwrap(), SyncOutcome::UpToDate);
    // Old invites stop working.
    book_b.post_voucher(now(), "accountant", payment(&book_b, "4", dec!(300))).unwrap();
    assert!(matches!(
        sync_sealed(&mut store_b, &mut book_b, &colleague, &old_key),
        Err(SyncError::Server { status: 401, .. })
    ));

    // A device invited to the new relay gets identical books.
    let joined = khata_sync::Remote { token: accountant_token(&moved.remote), ..moved.remote.clone() };
    let path_c = tmp("mc.khata");
    let mut store_c = Store::create(&path_c, "Moving Traders").unwrap();
    let mut book_c = Book::new("Moving Traders");
    assert_eq!(
        sync_sealed(&mut store_c, &mut book_c, &joined, &moved.key).unwrap(),
        SyncOutcome::Pulled(moved.entries)
    );
    assert_eq!(book_c.audit().entries(), book_a.audit().entries());
    assert_eq!(book_c.trial_balance(), book_a.trial_balance());
    // The old key cannot read the new relay's copy.
    let mut store_d = Store::create(tmp("md.khata"), "x").unwrap();
    let mut book_d = Book::new("x");
    assert!(matches!(
        sync_sealed(&mut store_d, &mut book_d, &joined, &old_key),
        Err(SyncError::InvalidFromServer(_))
    ));

    // Nothing readable on either relay's disk (amounts as JSON strings, as above).
    for dir in [&old_dir, &new_dir] {
        for f in std::fs::read_dir(dir).unwrap() {
            let text = String::from_utf8_lossy(&std::fs::read(f.unwrap().path()).unwrap()).into_owned();
            assert!(!text.contains("Kotak") && !text.contains("Office Rent") && !text.contains("\"43210\""));
        }
    }
}

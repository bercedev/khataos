//! Shared records (notes, the invoice design, tax settings…) syncing beside
//! the books: through a self-hosted server, through a blind relay, with an
//! older server, and with an older client. See `docs/sync-settings.md`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use khata_core::Book;
use khata_store::Store;
use khata_sync::{Remote, RecordsOutcome, SyncError, SyncOutcome, create_relay_company, sync, sync_all, sync_records, sync_sealed};
use serde_json::{Value, json};

const ADMIN: &str = "records-admin-key-0123456789abcdef";

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("khata-records-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    let _ = std::fs::remove_file(&p);
    p
}

struct Server {
    url: String,
    dir: PathBuf,
    /// While false the server answers like one from before shared records:
    /// no `records` routes.
    records: Arc<AtomicBool>,
}

fn start(name: &str, open_relay: bool) -> Server {
    use axum::response::IntoResponse;
    let dir = tmp(name);
    let state = khata_server::AppState::open(khata_server::Config { data_dir: dir.clone(), admin_key: ADMIN.into() }).unwrap();
    state.set_open_relay(open_relay);
    let records = Arc::new(AtomicBool::new(true));
    let flag = records.clone();
    let app = khata_server::router(state).layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let on = flag.load(Ordering::Relaxed);
            async move {
                if !on && req.uri().path().ends_with("/records") {
                    return axum::http::StatusCode::NOT_FOUND.into_response();
                }
                next.run(req).await
            }
        },
    ));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new().unwrap().block_on(async move {
            axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), app).await.unwrap();
        });
    });
    Server { url: format!("http://{addr}"), dir, records }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).build().into()
}

fn post(url: &str, token: &str, body: Value) -> (u16, Value) {
    let mut resp = agent().post(url).header("authorization", &format!("Bearer {token}")).send_json(body).unwrap();
    (resp.status().as_u16(), resp.body_mut().read_json().unwrap_or(Value::Null))
}

fn get(url: &str, token: &str) -> (u16, Value) {
    let mut resp = agent().get(url).header("authorization", &format!("Bearer {token}")).call().unwrap();
    (resp.status().as_u16(), resp.body_mut().read_json().unwrap_or(Value::Null))
}

/// A company on `server` with the owner's remote.
fn company(server: &Server) -> Remote {
    let (_, created) = post(&format!("{}/v1/companies", server.url), ADMIN, json!({ "name": "Desk Traders" }));
    Remote {
        url: server.url.clone(),
        company_id: created["company_id"].as_str().unwrap().into(),
        token: created["owner_token"].as_str().unwrap().into(),
    }
}

fn share(remote: &Remote, role: &str) -> Remote {
    let (_, v) = post(&format!("{}/v1/companies/{}/shares", remote.url, remote.company_id), &remote.token, json!({ "role": role, "label": role }));
    Remote { token: v["token"].as_str().unwrap().into(), ..remote.clone() }
}

fn device(name: &str) -> (Store, Book) {
    let mut book = Book::new("Desk Traders");
    book.create_ledger(now(), "owner", "HDFC Bank", book.group_by_name("Bank Accounts").unwrap().id).unwrap();
    (Store::create(tmp(name), "Desk Traders").unwrap(), book)
}

fn empty_device(name: &str) -> (Store, Book) {
    (Store::create(tmp(name), "Desk Traders").unwrap(), Book::new("Desk Traders"))
}

/// A 300 KB logo as invoice_design.rs stores it (base64 in JSON).
fn logo() -> String {
    let bytes: Vec<u8> = (0..300 * 1024).map(|i| (i * 7 % 251) as u8).collect();
    let b64: String = bytes.chunks(3).map(|c| format!("{:02x}{:02x}", c[0], c.get(1).copied().unwrap_or(0))).collect();
    json!({ "kind": "png", "data": format!("LOGO-MARKER-{b64}") }).to_string()
}

#[test]
fn notes_designs_and_a_logo_sync_between_two_devices_through_a_server() {
    let server = start("server", false);
    let owner = company(&server);
    let (mut store_a, mut book_a) = device("a.khata");
    store_a.set_record("desk.note.0199a", Some(r#"{"title":"Ask Mehta for the TDS certificate"}"#), "asha").unwrap();
    store_a.set_record("invoice.design", Some(r#"{"accent":"0055aa"}"#), "asha").unwrap();
    store_a.set_record("invoice.logo", Some(&logo()), "asha").unwrap();
    store_a.set_record("vat_gb.schemes", Some(r#"{"cash_accounting":true}"#), "asha").unwrap();
    let trail_before = book_a.audit().entries().to_vec();

    let (books, records) = sync_all(&mut store_a, &mut book_a, &owner, None).unwrap();
    assert_eq!(books, SyncOutcome::Pushed(trail_before.len() as u64));
    assert_eq!(records, RecordsOutcome { sent: 4, supported: true, ..Default::default() });
    assert_eq!(book_a.audit().entries(), &trail_before[..], "records add nothing to the trail");

    // The accountant's copy gets books and records alike.
    let accountant = share(&owner, "accountant");
    let (mut store_b, mut book_b) = empty_device("b.khata");
    let (_, got) = sync_all(&mut store_b, &mut book_b, &accountant, None).unwrap();
    assert_eq!(got.received, 4);
    assert_eq!(store_b.record("invoice.logo").unwrap(), Some(logo()));
    assert_eq!(store_b.record("desk.note.0199a").unwrap().as_deref(), Some(r#"{"title":"Ask Mehta for the TDS certificate"}"#));
    assert_eq!(book_b.audit().entries(), book_a.audit().entries());
    assert!(book_b.audit().verify().is_ok());

    // The accountant ticks the note done; the owner sees it.
    store_b.set_record("desk.note.0199a", Some(r#"{"title":"Ask Mehta for the TDS certificate","done":true}"#), "mehta").unwrap();
    assert_eq!(sync_all(&mut store_b, &mut book_b, &accountant, None).unwrap().1.sent, 1);
    assert_eq!(sync_all(&mut store_a, &mut book_a, &owner, None).unwrap(), (SyncOutcome::UpToDate, RecordsOutcome { received: 1, supported: true, ..Default::default() }));
    assert!(store_a.record("desk.note.0199a").unwrap().unwrap().contains("\"done\":true"));
    // Nothing is re-sent once in step.
    assert_eq!(sync_all(&mut store_a, &mut book_a, &owner, None).unwrap().1, RecordsOutcome { supported: true, ..Default::default() });
    assert_eq!(store_a.pending_records(&owner.url).unwrap(), 0);
}

#[test]
fn offline_edits_of_one_note_last_writer_wins_and_a_deleted_note_never_comes_back() {
    let server = start("conflicts", false);
    let owner = company(&server);
    let accountant = share(&owner, "accountant");
    let (mut store_a, mut book_a) = device("ca.khata");
    store_a.set_record("desk.note.1", Some("v1"), "asha").unwrap();
    store_a.set_record("desk.note.2", Some("keep me"), "asha").unwrap();
    sync(&mut store_a, &mut book_a, &owner).unwrap();
    let (mut store_b, mut book_b) = empty_device("cb.khata");
    sync(&mut store_b, &mut book_b, &accountant).unwrap();

    // Both edit note 2 offline; B's edit is later and wins on both.
    store_a.set_record_at("desk.note.2", Some("A's edit"), "asha", 2_000_000_000_000).unwrap();
    store_b.set_record_at("desk.note.2", Some("B's edit"), "mehta", 2_000_000_000_500).unwrap();
    // A deletes note 1 while B, offline, edits it even later.
    store_a.delete_record_forever("desk.note.1", "asha").unwrap();
    store_b.set_record_at("desk.note.1", Some("edited after the deletion"), "mehta", 3_000_000_000_000).unwrap();

    sync(&mut store_a, &mut book_a, &owner).unwrap();
    sync(&mut store_b, &mut book_b, &accountant).unwrap();
    sync(&mut store_a, &mut book_a, &owner).unwrap();
    for s in [&store_a, &store_b] {
        assert_eq!(s.record("desk.note.2").unwrap().as_deref(), Some("B's edit"));
        assert_eq!(s.record("desk.note.1").unwrap(), None, "the deleted note stays deleted");
    }
    assert_eq!(store_a.all_records().unwrap(), store_b.all_records().unwrap(), "both copies converge");
    // A third device joining later never sees the deleted note either.
    let (mut store_c, mut book_c) = empty_device("cc.khata");
    sync(&mut store_c, &mut book_c, &accountant).unwrap();
    assert_eq!(store_c.record("desk.note.1").unwrap(), None);
    assert_eq!(store_c.all_records().unwrap(), store_a.all_records().unwrap());
}

#[test]
fn viewers_read_records_but_cannot_change_them_on_the_server() {
    let server = start("viewer", false);
    let owner = company(&server);
    let (mut store_a, mut book_a) = device("va.khata");
    store_a.set_record("desk.calendar", Some(r#"{"audit":true}"#), "asha").unwrap();
    sync(&mut store_a, &mut book_a, &owner).unwrap();

    let viewer = share(&owner, "viewer");
    let (mut store_v, mut book_v) = empty_device("vv.khata");
    let (_, r) = sync_all(&mut store_v, &mut book_v, &viewer, None).unwrap();
    assert_eq!(r.received, 1);
    // Even if a viewer's copy changes a record, the server keeps it out.
    store_v.set_record("desk.calendar", Some(r#"{"audit":false}"#), "auditor").unwrap();
    let (_, r) = sync_all(&mut store_v, &mut book_v, &viewer, None).unwrap();
    assert!(r.read_only && r.sent == 0);
    let raw = json!({ "records": [store_v.record_version("desk.calendar").unwrap()] });
    assert_eq!(post(&format!("{}/v1/companies/{}/records", owner.url, owner.company_id), &viewer.token, raw).0, 403);
    sync(&mut store_a, &mut book_a, &owner).unwrap();
    assert_eq!(store_a.record("desk.calendar").unwrap().as_deref(), Some(r#"{"audit":true}"#));
}

#[test]
fn the_relay_sees_nothing_readable_in_shared_records() {
    let relay = start("relay", true);
    let (remote, key) = create_relay_company(&relay.url, None).unwrap();
    let (mut store_a, mut book_a) = device("ra.khata");
    store_a.set_record("desk.note.0199b", Some(r#"{"title":"Secret merger with Kotak"}"#), "asha").unwrap();
    store_a.set_record("invoice.voucher.0199c", Some(r#"{"ship_to_name":"Gupta Exports","incoterms":"FOB"}"#), "asha").unwrap();
    store_a.set_record("invoice.logo", Some(&logo()), "asha").unwrap();
    // The largest record a device may make (an SVG logo with its PNG) fits the relay's limits.
    let big = "B".repeat(khata_store::records::MAX_RECORD_VALUE);
    store_a.set_record("invoice.signature", Some(&big), "asha").unwrap();
    let (_, r) = sync_all(&mut store_a, &mut book_a, &remote, Some(&key)).unwrap();
    assert_eq!(r.sent, 4);

    for f in std::fs::read_dir(&relay.dir).unwrap() {
        let text = String::from_utf8_lossy(&std::fs::read(f.unwrap().path()).unwrap()).into_owned();
        for secret in ["Secret merger", "Kotak", "Gupta", "FOB", "LOGO-MARKER", "desk.note", "invoice.", "asha", "BBBBBBBB"] {
            assert!(!text.contains(secret), "{secret} reached the relay");
        }
    }

    let (mut store_b, mut book_b) = empty_device("rb.khata");
    let (_, r) = sync_all(&mut store_b, &mut book_b, &remote, Some(&key)).unwrap();
    assert_eq!(r.received, 4);
    assert_eq!(store_b.record("invoice.signature").unwrap().map(|v| v.len()), Some(big.len()));
    assert_eq!(store_b.record("invoice.logo").unwrap(), Some(logo()));
    assert_eq!(store_b.all_records().unwrap(), store_a.all_records().unwrap());

    // A deletion syncs through the relay too.
    store_b.delete_record_forever("desk.note.0199b", "mehta").unwrap();
    sync_sealed(&mut store_b, &mut book_b, &remote, &key).unwrap();
    sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap();
    assert_eq!(store_a.record("desk.note.0199b").unwrap(), None);

    // Without the key nothing can be read: the records don't open, and the
    // books refuse to sync.
    let (mut store_c, mut book_c) = empty_device("rc.khata");
    let wrong = khata_seal::CompanyKey::generate().unwrap();
    let r = sync_records(&mut store_c, &remote, Some(&wrong)).unwrap();
    assert_eq!((r.received, r.ignored), (0, 5));
    assert!(store_c.all_records().unwrap().is_empty());
    assert!(matches!(sync_all(&mut store_c, &mut book_c, &remote, Some(&wrong)), Err(SyncError::InvalidFromServer(_))));

    // A blob the relay (or anyone with a token) made up is ignored; it
    // holds up neither the other records nor the books.
    assert_eq!(post(&format!("{}/v1/relay/{}/records", remote.url, remote.company_id), &remote.token, json!({ "records": ["abcd"] })).0, 200);
    store_a.set_record("desk.note.0199d", Some("after the junk"), "asha").unwrap();
    sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap();
    let (mut store_d, mut book_d) = empty_device("rd.khata");
    let (_, r) = sync_all(&mut store_d, &mut book_d, &remote, Some(&key)).unwrap();
    assert_eq!(r.ignored, 1);
    assert_eq!(store_d.record("desk.note.0199d").unwrap().as_deref(), Some("after the junk"));
    assert_eq!(book_d.audit().entries(), book_a.audit().entries());
}

#[test]
fn rebasing_and_moving_relays_keep_the_shared_records() {
    let relay = start("rebase-old", true);
    let other = start("rebase-new", true);
    let (remote, key) = create_relay_company(&relay.url, None).unwrap();
    let path_a = tmp("ma.khata");
    let mut store_a = Store::create(&path_a, "Desk Traders").unwrap();
    let mut book_a = Book::new("Desk Traders");
    book_a.create_ledger(now(), "owner", "HDFC Bank", book_a.group_by_name("Bank Accounts").unwrap().id).unwrap();
    sync_sealed(&mut store_a, &mut book_a, &remote, &key).unwrap();
    let (mut store_b, mut book_b) = empty_device("mb.khata");
    sync_sealed(&mut store_b, &mut book_b, &remote, &key).unwrap();

    // Both change the books offline: a conflict. The records still sync.
    book_b.create_ledger(now(), "b", "Rent", book_b.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    sync_sealed(&mut store_b, &mut book_b, &remote, &key).unwrap();
    book_a.create_ledger(now(), "a", "Salaries", book_a.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    store_a.set_record("desk.note.9", Some("made during a conflict"), "asha").unwrap();
    assert!(matches!(sync_sealed(&mut store_a, &mut book_a, &remote, &key), Err(SyncError::Conflict { .. })));
    assert_eq!(sync_records(&mut store_b, &remote, Some(&key)).unwrap().received, 1);

    drop(store_a);
    let r = khata_sync::rebase_sealed(&path_a, &remote, &key).unwrap();
    let mut store_a = r.store;
    let mut book_a = r.book;
    assert_eq!(store_a.record("desk.note.9").unwrap().as_deref(), Some("made during a conflict"));

    // Moving to another relay re-seals the records under the new key.
    let moved = khata_sync::move_relay(&mut store_a, &mut book_a, &remote, &key, &other.url, None).unwrap();
    let (mut store_c, mut book_c) = empty_device("mc.khata");
    let (_, got) = sync_all(&mut store_c, &mut book_c, &moved.remote, Some(&moved.key)).unwrap();
    assert_eq!(got.received, 1);
    assert_eq!(store_c.record("desk.note.9").unwrap().as_deref(), Some("made during a conflict"));
    let _ = std::fs::remove_file(r.archived);
}

#[test]
fn an_older_server_syncs_the_books_and_keeps_the_records_waiting() {
    let server = start("old-server", false);
    server.records.store(false, Ordering::Relaxed);
    let owner = company(&server);
    let (mut store_a, mut book_a) = device("oa.khata");
    store_a.set_record("desk.note.1", Some("waiting for the server's update"), "asha").unwrap();
    let (books, records) = sync_all(&mut store_a, &mut book_a, &owner, None).unwrap();
    assert!(matches!(books, SyncOutcome::Pushed(_)), "the books sync as before");
    assert!(!records.supported);
    assert_eq!(store_a.pending_records(&owner.url).unwrap(), 1, "nothing is lost");
    assert_eq!(store_a.record_cursor(&owner.url).unwrap().supported, Some(false));

    // Once the server is updated, the next sync sends them.
    server.records.store(true, Ordering::Relaxed);
    let (_, records) = sync_all(&mut store_a, &mut book_a, &owner, None).unwrap();
    assert_eq!((records.supported, records.sent), (true, 1));
    assert_eq!(store_a.pending_records(&owner.url).unwrap(), 0);
}

#[test]
fn an_older_client_syncs_the_books_unchanged_with_a_newer_server() {
    let server = start("old-client", false);
    let owner = company(&server);
    let (mut store_a, mut book_a) = device("na.khata");
    store_a.set_record("desk.note.1", Some("only newer apps see this"), "asha").unwrap();
    sync(&mut store_a, &mut book_a, &owner).unwrap();

    // An older client speaks protocol 1: pull and push of entries only.
    let (status, pulled) = get(&format!("{}/v1/companies/{}/pull?from=0", owner.url, owner.company_id), &owner.token);
    assert_eq!(status, 200);
    assert_eq!(pulled["protocol"], 2, "newer servers say which protocol they speak; older clients ignore it");
    let entries: Vec<khata_core::AuditEntry> = serde_json::from_value(pulled["entries"].clone()).unwrap();
    assert_eq!(entries, book_a.audit().entries(), "the trail and its hashes are exactly the books'");
    let mut old_book = Book::replay("Desk Traders", khata_core::AuditLog::from(entries)).unwrap();
    old_book.create_ledger(now(), "old app", "Rent", old_book.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    let len = book_a.audit().entries().len();
    let new = &old_book.audit().entries()[len..];
    let (status, pushed) = post(&format!("{}/v1/companies/{}/push", owner.url, owner.company_id), &owner.token, json!({ "base_len": len, "entries": new }));
    assert_eq!((status, pushed["len"].as_u64()), (200, Some(len as u64 + 1)));
    // The newer client pulls the older client's entry, and keeps its records.
    assert_eq!(sync(&mut store_a, &mut book_a, &owner).unwrap(), SyncOutcome::Pulled(1));
    assert_eq!(store_a.record("desk.note.1").unwrap().as_deref(), Some("only newer apps see this"));
}

#[test]
fn device_local_and_credential_keys_never_leave_the_device() {
    let secrets = [
        ("sync.token", "SECRET-SYNC-TOKEN"),
        ("sync.url", "SECRET-SYNC-URL"),
        ("share.key", "SECRET-RELAY-KEY"),
        ("share.role", "SECRET-ROLE"),
        ("package.sent_to", "SECRET-PACKAGE-CURSOR"),
        ("zatca.device", "SECRET-ZATCA-CSID"),
        ("zatca.chain", "SECRET-ZATCA-CHAIN"),
        ("zatca.queue", "SECRET-ZATCA-QUEUE"),
        ("verifactu.chain", "SECRET-VERIFACTU-CHAIN"),
        ("tds.file_sequence", "SECRET-TDS-SEQUENCE"),
    ];
    let seed = |store: &Store| {
        for (k, v) in secrets {
            store.set_setting(k, Some(v)).unwrap();
            assert!(store.set_record(k, Some(v), "x").is_err(), "{k} can't become a shared record");
        }
        store.set_record("desk.note.1", Some("shared note"), "asha").unwrap();
    };
    let leaked = |text: &str| secrets.iter().find(|(_, v)| text.contains(v)).map(|(k, _)| *k);

    // Self-hosted (or office LAN) server.
    let server = start("secrets-server", true);
    let owner = company(&server);
    let (mut store_a, mut book_a) = device("sa.khata");
    seed(&store_a);
    sync(&mut store_a, &mut book_a, &owner).unwrap();
    // A forged device-local record is refused by the server…
    let forged = json!({ "records": [{ "key": "sync.token", "value": "x", "at": 1, "author": "x", "device": "x" }] });
    assert_eq!(post(&format!("{}/v1/companies/{}/records", owner.url, owner.company_id), &owner.token, forged).0, 422);
    let (_, pulled) = get(&format!("{}/v1/companies/{}/records?since=0", owner.url, owner.company_id), &owner.token);
    assert_eq!(leaked(&pulled.to_string()), None);
    assert_eq!(pulled["records"].as_array().unwrap().len(), 1);

    // Blind relay.
    let (relay, key) = create_relay_company(&server.url, None).unwrap();
    let (mut store_r, mut book_r) = device("sr.khata");
    seed(&store_r);
    sync_sealed(&mut store_r, &mut book_r, &relay, &key).unwrap();
    // …and one sealed with the right key is ignored by every device.
    let forged = khata_store::Record { key: "share.key".into(), value: Some("planted".into()), at: 1, author: "x".into(), device: "x".into(), forever: false };
    let blob = hex::encode(khata_seal::seal(&key, &khata_seal::record_context(&relay.company_id), &serde_json::to_vec(&forged).unwrap()).unwrap());
    assert_eq!(post(&format!("{}/v1/relay/{}/records", relay.url, relay.company_id), &relay.token, json!({ "records": [blob] })).0, 200);
    let (mut store_j, mut book_j) = empty_device("sj.khata");
    sync_sealed(&mut store_j, &mut book_j, &relay, &key).unwrap();
    assert_eq!(store_j.record("desk.note.1").unwrap().as_deref(), Some("shared note"));
    assert!(store_j.all_records().unwrap().iter().all(|r| !khata_store::records::is_device_local(&r.key)));
    assert_eq!(store_j.setting("share.key").unwrap(), None);

    // Everything the server and relay hold on disk.
    for f in std::fs::read_dir(&server.dir).unwrap() {
        let text = String::from_utf8_lossy(&std::fs::read(f.unwrap().path()).unwrap()).into_owned();
        assert_eq!(leaked(&text), None);
    }

    // Accountant file (.khatapkg), decrypted.
    let pkg = khata_package::export_with_records(&book_a, 0, &store_a.all_records().unwrap(), "pass phrase").unwrap();
    assert_eq!(leaked(&String::from_utf8_lossy(&pkg)), None);
    let (_, entries, records) = khata_package::open_with_records(&pkg, "pass phrase").unwrap();
    assert_eq!(leaked(&serde_json::to_string(&(entries, &records)).unwrap()), None);
    assert_eq!(records.len(), 1);
}

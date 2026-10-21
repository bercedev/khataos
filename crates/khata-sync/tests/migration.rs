//! Whole-system scenario: a business migrates from Tally, syncs to its own
//! server, and its external CA reviews the books read-only.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{Book, reports};
use khata_store::Store;
use khata_sync::{Remote, SyncOutcome, sync};
use serde_json::{Value, json};

const ADMIN: &str = "test-admin-key-0123456789abcdef";
const MASTERS: &[u8] = include_bytes!("../../khata-import-tally/tests/fixtures/masters.xml");
const VOUCHERS: &[u8] = include_bytes!("../../khata-import-tally/tests/fixtures/vouchers.xml");

fn tmp(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("khata-migration-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    let _ = std::fs::remove_file(&p);
    p
}

fn start_server() -> String {
    let state = khata_server::AppState::open(khata_server::Config { data_dir: tmp("server"), admin_key: ADMIN.into() })
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

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).build().into()
}

fn post(url: &str, token: &str, body: Value) -> Value {
    agent().post(url).header("authorization", &format!("Bearer {token}")).send_json(body).unwrap().body_mut().read_json().unwrap()
}

fn get(url: &str, token: &str) -> (u16, Value) {
    let mut r = agent().get(url).header("authorization", &format!("Bearer {token}")).call().unwrap();
    (r.status().as_u16(), r.body_mut().read_json().unwrap_or(Value::Null))
}

#[test]
fn tally_migration_to_shared_books() {
    let now = DateTime::<Utc>::from_timestamp(1_760_000_000, 0).unwrap();

    // 1. Migrate: import the Tally masters and Day Book exports into a new company file.
    let mut book = Book::new("Shah & Sons");
    let report = khata_import_tally::import_all([MASTERS, VOUCHERS], &mut book, now, "owner").unwrap();
    assert!(report.vouchers_posted > 0, "{report:?}");
    assert_eq!(book.opening_difference(), rust_decimal::Decimal::ZERO, "opening balances tally");
    let path = tmp("shah.khata");
    let mut store = Store::create(&path, &book.name).unwrap();
    store.save(&book).unwrap();

    // 2. Share: create the company on the self-hosted server and push.
    let url = start_server();
    let created = post(&format!("{url}/v1/companies"), ADMIN, json!({ "name": "Shah & Sons" }));
    let company_id = created["company_id"].as_str().unwrap().to_owned();
    let owner = Remote { url: url.clone(), company_id: company_id.clone(), token: created["owner_token"].as_str().unwrap().into() };
    assert!(matches!(sync(&mut store, &mut book, &owner).unwrap(), SyncOutcome::Pushed(_)));

    // 3. The CA gets 30-day read-only access and sees the same figures.
    let share = post(
        &format!("{url}/v1/companies/{company_id}/shares"),
        &owner.token,
        json!({ "role": "viewer", "label": "Mehta & Co. (CA)", "expires_in_days": 30 }),
    );
    let ca = share["token"].as_str().unwrap();
    let (status, tb) = get(&format!("{url}/v1/companies/{company_id}/reports/trial-balance"), ca);
    assert_eq!(status, 200);
    assert_eq!(tb["rows"].as_array().unwrap().len(), book.trial_balance().len());

    let as_at = NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();
    let (_, bs) = get(&format!("{url}/v1/companies/{company_id}/reports/balance-sheet?as_at={as_at}"), ca);
    let local = reports::balance_sheet(&book, as_at);
    assert_eq!(bs["total_assets"], json!(local.total_assets.to_string()));
    assert_eq!(bs["total_assets"], bs["total_liabilities"], "balance sheet tallies on the server");

    // The CA cannot change anything.
    let (status, _) = (
        agent()
            .post(&format!("{url}/v1/companies/{company_id}/push"))
            .header("authorization", &format!("Bearer {ca}"))
            .send_json(json!({ "base_len": 0, "entries": [] }))
            .unwrap()
            .status()
            .as_u16(),
        (),
    );
    assert_eq!(status, 403);
}

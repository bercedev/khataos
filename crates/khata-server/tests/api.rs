use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, NaiveDate, Utc};
use http_body_util::BodyExt;
use khata_core::{AuditAction, BillKind, Book, Entry, Voucher, VoucherId, VoucherKind};
use khata_server::protocol::*;
use khata_server::{AppState, Config, Role, router};
use rust_decimal_macros::dec;
use serde_json::{Value, json};
use tower::ServiceExt;

const ADMIN: &str = "test-admin-key-0123456789abcdef";

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn app(name: &str) -> Router {
    let dir = std::env::temp_dir().join(format!("khata-server-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    router(AppState::open(Config { data_dir: dir, admin_key: ADMIN.into() }).unwrap())
}

async fn call(app: &Router, method: &str, uri: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(body.map_or(Body::empty(), |b| Body::from(b.to_string())))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn new_company(app: &Router) -> (String, String) {
    let (s, v) = call(app, "POST", "/v1/companies", ADMIN, Some(json!({ "name": "Demo Traders" }))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let c: CompanyCreated = serde_json::from_value(v).unwrap();
    (c.company_id.to_string(), c.owner_token)
}

async fn share(app: &Router, company: &str, owner: &str, role: Role, days: Option<u32>) -> (String, String) {
    let body = json!({ "role": role, "label": "Mehta & Co.", "expires_in_days": days });
    let (s, v) = call(app, "POST", &format!("/v1/companies/{company}/shares"), owner, Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let c: ShareCreated = serde_json::from_value(v).unwrap();
    (c.id.to_string(), c.token)
}

/// A device's local books with one rent payment.
fn device_books() -> Book {
    let mut b = Book::new("Demo Traders");
    let bank = b.create_ledger(now(), "clerk", "HDFC Bank", b.group_by_name("Bank Accounts").unwrap().id).unwrap();
    let rent = b.create_ledger(now(), "clerk", "Rent", b.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    b.post_voucher(
        now(),
        "clerk",
        Voucher {
            id: VoucherId::new(),
            kind: VoucherKind::Payment,
            number: "1".into(),
            date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            entries: vec![Entry::dr(rent, dec!(25000)), Entry::cr(bank, dec!(25000))],
            narration: String::new(), cancelled: false, stock: Vec::new(),
        },
    )
    .unwrap();
    b
}

fn push_body(base_len: u64, book: &Book) -> Value {
    json!({ "base_len": base_len, "entries": book.audit().entries()[base_len as usize..] })
}

#[tokio::test]
async fn team_sync_round_trip() {
    let app = app("roundtrip");
    let (company, owner) = new_company(&app).await;
    let book = device_books();

    let (s, v) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["len"], 3);

    // A second device pulls and rebuilds identical books.
    let (s, v) = call(&app, "GET", &format!("/v1/companies/{company}/pull?from=0"), &owner, None).await;
    assert_eq!(s, StatusCode::OK);
    let pulled: Pulled = serde_json::from_value(v).unwrap();
    let log = serde_json::from_value(json!({ "entries": pulled.entries })).unwrap();
    let replica = Book::replay(pulled.company, log).unwrap();
    assert_eq!(replica.trial_balance(), book.trial_balance());

    // Incremental pull returns only what's new.
    let (_, v) = call(&app, "GET", &format!("/v1/companies/{company}/pull?from=2"), &owner, None).await;
    assert_eq!(v["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn external_viewer_reads_but_cannot_write() {
    let app = app("viewer");
    let (company, owner) = new_company(&app).await;
    call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &device_books()))).await;
    let (_, viewer) = share(&app, &company, &owner, Role::Viewer, Some(30)).await;

    let (s, v) = call(&app, "GET", &format!("/v1/companies/{company}/reports/trial-balance"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["rows"].as_array().unwrap().len(), 2);
    assert_eq!(v["opening_stock"], "0");

    let (s, _) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &viewer, Some(push_body(3, &device_books()))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/shares"), &viewer, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "only the owner manages access");
}

#[tokio::test]
async fn expired_and_revoked_tokens_stop_working() {
    let app = app("expiry");
    let (company, owner) = new_company(&app).await;
    let (_, expired) = share(&app, &company, &owner, Role::Viewer, Some(0)).await;
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/pull"), &expired, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (id, accountant) = share(&app, &company, &owner, Role::Accountant, None).await;
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/pull"), &accountant, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&app, "DELETE", &format!("/v1/companies/{company}/shares/{id}"), &owner, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/pull"), &accountant, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (_, v) = call(&app, "GET", &format!("/v1/companies/{company}/shares"), &owner, None).await;
    let revoked: Vec<bool> = v.as_array().unwrap().iter().map(|s| s["revoked"].as_bool().unwrap()).collect();
    assert_eq!(revoked, [false, false, true], "owner, expired viewer, revoked accountant");

    // The owner cannot lock themselves out.
    let own = v[0]["id"].as_str().unwrap();
    let (s, _) = call(&app, "DELETE", &format!("/v1/companies/{company}/shares/{own}"), &owner, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/pull"), &owner, None).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn tokens_are_scoped_to_their_company() {
    let app = app("scope");
    let (a, owner_a) = new_company(&app).await;
    let (b, _) = new_company(&app).await;
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{b}/pull"), &owner_a, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{a}/pull"), "khk_not_a_token", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, "POST", "/v1/companies", "wrong-admin", Some(json!({ "name": "X" }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn stale_push_conflicts() {
    let app = app("conflict");
    let (company, owner) = new_company(&app).await;
    let book = device_books();
    call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;
    let (s, v) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(v["server_len"], 3);
}

#[tokio::test]
async fn tampered_client_cannot_push_unbalanced_voucher() {
    let app = app("tamper");
    let (company, owner) = new_company(&app).await;
    let book = device_books();
    call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;

    // Forge an entry with a valid hash chain but an unbalanced voucher,
    // bypassing the client-side checks in Book::post_voucher.
    let rent = book.ledger_by_name("Rent").unwrap().id;
    let bank = book.ledger_by_name("HDFC Bank").unwrap().id;
    let bad = Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Payment,
        number: "2".into(),
        date: NaiveDate::from_ymd_opt(2026, 4, 2).unwrap(),
        entries: vec![Entry::dr(rent, dec!(500)), Entry::cr(bank, dec!(5))],
        narration: String::new(), cancelled: false, stock: Vec::new(),
    };
    let mut log = book.audit().clone();
    log.record(now(), "mallory", AuditAction::Create, format!("voucher:{}", bad.id.0), None, Some(json!(bad)));
    // Even local replay refuses it now; push the raw forged entries.
    assert!(Book::replay("Demo Traders", log.clone()).is_err());
    let forged = log.entries()[3..].to_vec();

    let (s, v) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(json!({ "base_len": 3, "entries": forged }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert!(v["error"].as_str().unwrap().contains("do not equal"));

    // Breaking the hash chain is rejected too.
    let mut entries = forged;
    entries[0].user = "someone-else".into();
    let (s, _) = call(
        &app,
        "POST",
        &format!("/v1/companies/{company}/push"),
        &owner,
        Some(json!({ "base_len": 3, "entries": entries })),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn viewer_reads_reports_and_portal_is_served() {
    let app = app("reports");
    let (company, owner) = new_company(&app).await;
    call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &device_books()))).await;
    let (_, viewer) = share(&app, &company, &owner, Role::Viewer, Some(30)).await;

    let (s, v) = call(&app, "GET", &format!("/v1/companies/{company}/reports/day-book?from=2026-04-01&to=2026-04-30"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v[0]["particulars"], "Rent");
    assert_eq!(v[0]["debit"], "25000");

    let (s, v) = call(&app, "GET", &format!("/v1/companies/{company}/reports/profit-and-loss?from=2026-04-01&to=2027-03-31"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["net_profit"], "-25000");

    let (s, v) = call(&app, "GET", &format!("/v1/companies/{company}/reports/balance-sheet?as_at=2027-03-31"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["total_assets"], v["total_liabilities"]);

    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/reports/day-book?from=2026-04-01&to=2026-04-30"), "nope", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let resp = app.clone().oneshot(Request::get("/portal").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let csp = resp.headers()["content-security-policy"].to_str().unwrap().to_owned();
    assert!(csp.contains("frame-ancestors 'none'") && csp.contains("script-src 'sha256-"), "{csp}");
    assert!(!csp.contains("unsafe-inline"));
    assert_eq!(resp.headers()["x-frame-options"], "DENY");
    let html = String::from_utf8(resp.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(html.contains("Read-only accountant access"));
}


#[tokio::test]
async fn forged_reference_is_rejected_and_server_keeps_serving() {
    // Review finding 1: a balanced voucher pointing at ledgers that do not
    // exist used to be stored and then crash every report.
    let app = app("dangling");
    let (company, owner) = new_company(&app).await;
    let mut book = device_books();
    call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;
    let ghost_a = khata_core::LedgerId::new();
    let ghost_b = khata_core::LedgerId::new();
    let v = Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Journal,
        number: "1".into(),
        date: NaiveDate::from_ymd_opt(2026, 4, 2).unwrap(),
        entries: vec![Entry::dr(ghost_a, dec!(1)), Entry::cr(ghost_b, dec!(1))],
        narration: String::new(), cancelled: false, stock: Vec::new(),
    };
    let mut log = book.audit().clone();
    log.record(now(), "mallory", AuditAction::Create, format!("voucher:{}", v.id.0), None, Some(json!(v)));
    let entries = log.entries()[3..].to_vec();
    let (s, v) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(json!({ "base_len": 3, "entries": entries }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert!(v["error"].as_str().unwrap().contains("unknown ledger"));

    // Nothing was stored; reports and other pushes still work.
    let (s, _) = call(&app, "GET", &format!("/v1/companies/{company}/reports/trial-balance"), &owner, None).await;
    assert_eq!(s, StatusCode::OK);
    let rent = book.ledger_by_name("Rent").unwrap().id;
    let bank = book.ledger_by_name("HDFC Bank").unwrap().id;
    book.post_voucher(now(), "clerk", Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Payment,
        number: "2".into(),
        date: NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(),
        entries: vec![Entry::dr(rent, dec!(10)), Entry::cr(bank, dec!(10))],
        narration: String::new(), cancelled: false, stock: Vec::new(),
    }).unwrap();
    let (s, v) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(3, &book))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

#[tokio::test]
async fn limits_are_enforced() {
    let app = app("limits");
    let (company, owner) = new_company(&app).await;
    let body = json!({ "role": "viewer", "label": "x", "expires_in_days": 4000000000u64 });
    let (s, _) = call(&app, "POST", &format!("/v1/companies/{company}/shares"), &owner, Some(body)).await;
    assert!(s.is_client_error(), "huge expiry must not panic");
    let long = "x".repeat(500);
    let (s, _) = call(&app, "POST", "/v1/companies", ADMIN, Some(json!({ "name": long }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn source_link_is_offered_for_agpl() {
    let app = app("source");
    let resp = app.clone().oneshot(Request::get("/source").body(Body::empty()).unwrap()).await.unwrap();
    assert!(resp.status().is_redirection(), "{}", resp.status());
    assert!(resp.headers()["location"].to_str().unwrap().starts_with("https://"));
}

// ------------------------------------------------------------------ Portal v2

/// GET with a bearer token; returns status, headers and the raw body.
async fn raw(app: &Router, uri: &str, token: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let req = Request::get(uri).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// Books with a debtor and a supplier tracked bill by bill:
/// - bank 1,00,000 Dr against capital (opening);
/// - 1 Apr rent 25,000 paid from the bank;
/// - 5 Apr sale of 50,000 to `Sharma, "Ltd"`, bill INV-1 due 5 May;
/// - 10 Apr purchase of 30,000 from Gupta Supplies, bill P-7 due 10 May;
/// - 20 Apr 20,000 received from Sharma against INV-1.
fn party_books() -> Book {
    let mut b = Book::new("Demo Traders");
    let g = |b: &Book, n: &str| b.group_by_name(n).unwrap().id;
    let bank = b.create_ledger(now(), "clerk", "HDFC Bank", g(&b, "Bank Accounts")).unwrap();
    let capital = b.create_ledger(now(), "clerk", "Capital", g(&b, "Capital Account")).unwrap();
    let rent = b.create_ledger(now(), "clerk", "Rent", g(&b, "Indirect Expenses")).unwrap();
    let sales = b.create_ledger(now(), "clerk", "Sales", g(&b, "Sales Accounts")).unwrap();
    let purchases = b.create_ledger(now(), "clerk", "Purchases", g(&b, "Purchase Accounts")).unwrap();
    let sharma = b.create_ledger(now(), "clerk", "Sharma, \"Ltd\"", g(&b, "Sundry Debtors")).unwrap();
    let gupta = b.create_ledger(now(), "clerk", "Gupta Supplies", g(&b, "Sundry Creditors")).unwrap();
    b.set_opening_balance(now(), "clerk", bank, dec!(100000)).unwrap();
    b.set_opening_balance(now(), "clerk", capital, dec!(-100000)).unwrap();
    let mut post = |kind, number: &str, date, entries| {
        let v = Voucher { id: VoucherId::new(), kind, number: number.into(), date, entries, narration: String::new(), cancelled: false, stock: Vec::new() };
        b.post_voucher(now(), "clerk", v).unwrap();
    };
    post(VoucherKind::Payment, "1", d(2026, 4, 1), vec![Entry::dr(rent, dec!(25000)), Entry::cr(bank, dec!(25000))]);
    post(
        VoucherKind::Sales,
        "1",
        d(2026, 4, 5),
        vec![
            Entry::dr(sharma, dec!(50000)).bill(BillKind::New, "INV-1", dec!(50000), Some(d(2026, 5, 5))),
            Entry::cr(sales, dec!(50000)),
        ],
    );
    post(
        VoucherKind::Purchase,
        "1",
        d(2026, 4, 10),
        vec![
            Entry::dr(purchases, dec!(30000)),
            Entry::cr(gupta, dec!(30000)).bill(BillKind::New, "P-7", dec!(30000), Some(d(2026, 5, 10))),
        ],
    );
    post(
        VoucherKind::Receipt,
        "1",
        d(2026, 4, 20),
        vec![Entry::dr(bank, dec!(20000)), Entry::cr(sharma, dec!(20000)).bill(BillKind::Against, "INV-1", dec!(20000), None)],
    );
    b
}

/// A company holding `party_books()`, plus a viewer token.
async fn party_company(name: &str) -> (Router, String, String, String, Book) {
    let app = app(name);
    let (company, owner) = new_company(&app).await;
    let book = party_books();
    let (s, v) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, viewer) = share(&app, &company, &owner, Role::Viewer, Some(30)).await;
    (app, company, owner, viewer, book)
}

#[tokio::test]
async fn new_endpoints_require_a_token_and_allow_viewers() {
    let (app, company, _, viewer, book) = party_company("v2-auth").await;
    let sharma = book.ledger_by_name("Sharma, \"Ltd\"").unwrap().id.0;
    let ca = book.group_by_name("Current Assets").unwrap().id.0;
    let base = format!("/v1/companies/{company}");
    let uris = [
        format!("{base}/ledgers"),
        format!("{base}/ledgers/{sharma}/statement?from=2026-04-01&to=2026-06-30"),
        format!("{base}/groups/{ca}/summary?as_at=2026-06-30"),
        format!("{base}/reports/outstanding?as_at=2026-06-30"),
        format!("{base}/reports/ageing?as_at=2026-06-30&receivable=false"),
        format!("{base}/reports/msme-overdue?as_at=2026-06-30"),
        format!("{base}/reports/audit-trail"),
        format!("{base}/reports/audit-trail?format=csv"),
        format!("{base}/reports/trial-balance?format=csv"),
        format!("{base}/reports/trial-balance?as_at=2026-06-30&format=pdf-html"),
        format!("{base}/reports/day-book?from=2026-04-01&to=2026-06-30&format=csv"),
        format!("{base}/reports/day-book?from=2026-04-01&to=2026-06-30&format=pdf-html"),
        format!("{base}/reports/profit-and-loss?from=2026-04-01&to=2027-03-31&format=csv"),
        format!("{base}/reports/profit-and-loss?from=2026-04-01&to=2027-03-31&format=pdf-html"),
        format!("{base}/reports/balance-sheet?as_at=2027-03-31&format=csv"),
        format!("{base}/reports/balance-sheet?as_at=2027-03-31&format=pdf-html"),
        format!("{base}/ledgers/{sharma}/statement?from=2026-04-01&to=2026-06-30&format=pdf-html"),
        format!("{base}/ledgers/{sharma}/statement?from=2026-04-01&to=2026-06-30&format=csv"),
        format!("{base}/ledgers?format=csv"),
        format!("{base}/groups/{ca}/summary?as_at=2026-06-30&format=csv"),
        format!("{base}/reports/outstanding?as_at=2026-06-30&format=csv"),
        format!("{base}/reports/ageing?as_at=2026-06-30&format=csv"),
        format!("{base}/reports/msme-overdue?as_at=2026-06-30&format=csv"),
        format!("{base}/reports/stock-summary?from=2026-04-01&to=2026-06-30"),
        format!("{base}/reports/stock-summary?from=2026-04-01&to=2026-06-30&format=csv"),
        format!("{base}/reports/ratios?from=2026-04-01&to=2026-06-30"),
        format!("{base}/reports/ratios?from=2026-04-01&to=2026-06-30&format=csv"),
    ];
    for uri in &uris {
        let (s, _, _) = raw(&app, uri, "khk_wrong").await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{uri}");
        let req = Request::get(uri).body(Body::empty()).unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED, "no token: {uri}");
        let (s, _, body) = raw(&app, uri, &viewer).await;
        assert_eq!(s, StatusCode::OK, "{uri}: {body}");
    }

    // A token for another company is refused too.
    let (other, other_owner) = new_company(&app).await;
    let _ = other;
    let (s, _, _) = raw(&app, &format!("{base}/ledgers"), &other_owner).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ledgers_statement_and_group_drill_down() {
    let (app, company, _, viewer, book) = party_company("v2-drill").await;
    let base = format!("/v1/companies/{company}");

    let (s, v) = call(&app, "GET", &format!("{base}/ledgers?as_at=2026-06-30"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let balances: Vec<(&str, &str, &str)> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["name"].as_str().unwrap(), l["group_name"].as_str().unwrap(), l["balance"].as_str().unwrap()))
        .collect();
    assert_eq!(
        balances,
        [
            ("Capital", "Capital Account", "-100000"),
            ("Gupta Supplies", "Sundry Creditors", "-30000"),
            ("HDFC Bank", "Bank Accounts", "95000"),
            ("Purchases", "Purchase Accounts", "30000"),
            ("Rent", "Indirect Expenses", "25000"),
            ("Sales", "Sales Accounts", "-50000"),
            ("Sharma, \"Ltd\"", "Sundry Debtors", "30000"),
        ]
    );

    // Balance Sheet row → group summary: sub-groups with everything below them.
    let (_, bs) = call(&app, "GET", &format!("{base}/reports/balance-sheet?as_at=2026-06-30"), &viewer, None).await;
    let ca = bs["assets"].as_array().unwrap().iter().find(|g| g["name"] == "Current Assets").unwrap();
    assert_eq!(ca["amount"], "125000");
    let (s, g) = call(&app, "GET", &format!("{base}/groups/{}/summary?as_at=2026-06-30", ca["group"].as_str().unwrap()), &viewer, None).await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["name"], "Current Assets");
    assert_eq!(g["nature"], "Assets");
    assert_eq!(g["total"], ca["amount"], "drill-down agrees with the Balance Sheet");
    let rows: Vec<(&str, &str, &str)> = g["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["kind"].as_str().unwrap(), r["name"].as_str().unwrap(), r["amount"].as_str().unwrap()))
        .collect();
    assert_eq!(rows, [("group", "Bank Accounts", "95000"), ("group", "Sundry Debtors", "30000")]);

    // Sub-group → ledger, shown on the natural (credit) side for liabilities.
    let creditors = book.group_by_name("Sundry Creditors").unwrap().id.0;
    let (_, g) = call(&app, "GET", &format!("{base}/groups/{creditors}/summary?as_at=2026-06-30"), &viewer, None).await;
    assert_eq!(g["rows"][0]["kind"], "ledger");
    assert_eq!(g["rows"][0]["name"], "Gupta Supplies");
    assert_eq!(g["rows"][0]["amount"], "30000");
    assert_eq!(g["rows"][0]["id"], book.ledger_by_name("Gupta Supplies").unwrap().id.0.to_string());

    // P&L row → group summary over the same period.
    let (_, pl) = call(&app, "GET", &format!("{base}/reports/profit-and-loss?from=2026-04-01&to=2027-03-31"), &viewer, None).await;
    assert_eq!(pl["net_profit"], "-5000");
    let ind = pl["expenses"].as_array().unwrap().iter().find(|g| g["name"] == "Indirect Expenses").unwrap();
    let (_, g) = call(
        &app,
        "GET",
        &format!("{base}/groups/{}/summary?from=2026-04-01&to=2027-03-31", ind["group"].as_str().unwrap()),
        &viewer,
        None,
    )
    .await;
    assert_eq!(g["total"], ind["amount"]);
    assert_eq!(g["rows"][0]["name"], "Rent");
    // Movements only: nothing for a period before the rent was paid.
    let (_, g) = call(
        &app,
        "GET",
        &format!("{base}/groups/{}/summary?from=2025-04-01&to=2026-03-31", ind["group"].as_str().unwrap()),
        &viewer,
        None,
    )
    .await;
    assert_eq!(g["rows"].as_array().unwrap().len(), 0);
    assert_eq!(g["total"], "0");

    let (s, _) = call(&app, "GET", &format!("{base}/groups/{creditors}/summary?as_at=2026-06-30&from=2026-04-01"), &viewer, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "mixing as_at and a period is ambiguous");
    let (s, _) = call(&app, "GET", &format!("{base}/groups/{}/summary", uuid_like()), &viewer, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Ledger → statement with running balance.
    let sharma = book.ledger_by_name("Sharma, \"Ltd\"").unwrap().id.0;
    let (s, st) = call(&app, "GET", &format!("{base}/ledgers/{sharma}/statement?from=2026-04-01&to=2026-06-30"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["name"], "Sharma, \"Ltd\"");
    assert_eq!(st["opening"], "0");
    assert_eq!(st["closing"], "30000");
    let rows: Vec<(&str, &str, &str, &str)> = st["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["particulars"].as_str().unwrap(), r["debit"].as_str().unwrap(), r["credit"].as_str().unwrap(), r["balance"].as_str().unwrap()))
        .collect();
    assert_eq!(rows, [("Sales", "50000", "0", "50000"), ("HDFC Bank", "0", "20000", "30000")]);
    // Opening carries earlier vouchers forward.
    let (_, st) = call(&app, "GET", &format!("{base}/ledgers/{sharma}/statement?from=2026-04-10&to=2026-06-30"), &viewer, None).await;
    assert_eq!(st["opening"], "50000");
    assert_eq!(st["rows"].as_array().unwrap().len(), 1);

    let (s, _) = call(&app, "GET", &format!("{base}/ledgers/{}/statement?from=2026-04-01&to=2026-06-30", uuid_like()), &viewer, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

fn uuid_like() -> String {
    "0190a000-0000-7000-8000-000000000000".into()
}

#[tokio::test]
async fn outstanding_ageing_and_msme_overdue() {
    let (app, company, _, viewer, _) = party_company("v2-bills").await;
    let base = format!("/v1/companies/{company}");

    let (s, v) = call(&app, "GET", &format!("{base}/reports/outstanding?as_at=2026-06-30"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let bills: Vec<(&str, &str, &str, i64)> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["party"].as_str().unwrap(), b["reference"].as_str().unwrap(), b["amount"].as_str().unwrap(), b["overdue_days"].as_i64().unwrap()))
        .collect();
    assert_eq!(bills, [("Gupta Supplies", "P-7", "-30000", 51), ("Sharma, \"Ltd\"", "INV-1", "30000", 56)]);

    let (_, v) = call(&app, "GET", &format!("{base}/reports/outstanding?as_at=2026-06-30&side=receivable"), &viewer, None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["reference"], "INV-1");
    let (_, v) = call(&app, "GET", &format!("{base}/reports/outstanding?as_at=2026-06-30&side=payable"), &viewer, None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["reference"], "P-7");
    // Before the receipt, the whole invoice is open.
    let (_, v) = call(&app, "GET", &format!("{base}/reports/outstanding?as_at=2026-04-15&side=receivable"), &viewer, None).await;
    assert_eq!(v[0]["amount"], "50000");

    // Ageing: 56 and 51 days past due both fall in 31–60.
    let (_, v) = call(&app, "GET", &format!("{base}/reports/ageing?as_at=2026-06-30"), &viewer, None).await;
    assert_eq!(v[0]["party"], "Sharma, \"Ltd\"");
    assert_eq!(v[0]["buckets"], json!(["0", "30000", "0", "0"]));
    assert_eq!(v[0]["total"], "30000");
    let (_, v) = call(&app, "GET", &format!("{base}/reports/ageing?as_at=2026-06-30&receivable=false"), &viewer, None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["party"], "Gupta Supplies");
    assert_eq!(v[0]["buckets"], json!(["0", "30000", "0", "0"]));

    // MSME: 81 days since the 10 Apr bill; flagged at the default 45 days, not at 90.
    let (_, v) = call(&app, "GET", &format!("{base}/reports/msme-overdue?as_at=2026-06-30"), &viewer, None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["party"], "Gupta Supplies");
    assert_eq!(v[0]["amount"], "-30000");
    let (_, v) = call(&app, "GET", &format!("{base}/reports/msme-overdue?as_at=2026-06-30&days=90"), &viewer, None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
    let (_, v) = call(&app, "GET", &format!("{base}/reports/msme-overdue?as_at=2026-05-20"), &viewer, None).await;
    assert_eq!(v.as_array().unwrap().len(), 0, "40 days is within the 45-day window");
    let (s, _) = call(&app, "GET", &format!("{base}/reports/msme-overdue?as_at=2026-06-30&days=99999"), &viewer, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "GET", &format!("{base}/reports/outstanding"), &viewer, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "as_at is required");

    // CSV: payables as positive amounts with a side column.
    let (s, h, body) = raw(&app, &format!("{base}/reports/msme-overdue?as_at=2026-06-30&format=csv"), &viewer).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["content-disposition"], "attachment; filename=\"demo-traders_msme-overdue-45-days-2026-06-30.csv\"");
    assert_eq!(
        body,
        "Party,Reference,Bill Date,Due Date,Side,Amount,Days Past Due\r\nGupta Supplies,P-7,2026-04-10,2026-05-10,Payable,30000,51\r\n"
    );
    let (_, _, body) = raw(&app, &format!("{base}/reports/ageing?as_at=2026-06-30&format=csv"), &viewer).await;
    assert_eq!(
        body,
        "Party,0-30 days,31-60 days,61-90 days,Over 90 days,Total\r\n\"Sharma, \"\"Ltd\"\"\",0,30000,0,0,30000\r\n"
    );
    let (s, _, _) = raw(&app, &format!("{base}/reports/outstanding?as_at=2026-06-30&format=pdf-html"), &viewer).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no print layout for bills yet");
}

#[tokio::test]
async fn csv_downloads_quote_and_name_files() {
    let (app, company, _, viewer, book) = party_company("v2-csv").await;
    let base = format!("/v1/companies/{company}");

    let (s, h, body) = raw(&app, &format!("{base}/reports/trial-balance?as_at=2026-06-30&format=csv"), &viewer).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["content-type"], "text/csv; charset=utf-8");
    assert_eq!(h["content-disposition"], "attachment; filename=\"demo-traders_trial-balance-2026-06-30.csv\"");
    assert_eq!(h["cache-control"], "no-store");
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert!(body.starts_with("Particulars,Debit,Credit\r\n"), "{body}");
    assert!(body.contains("\r\n\"Sharma, \"\"Ltd\"\"\",30000,0\r\n"), "RFC 4180 quoting: {body}");
    assert!(body.contains("\r\nHDFC Bank,95000,0\r\n"), "{body}");
    assert_eq!(body.lines().count(), 8, "header and seven ledgers");
    // As at an earlier date only earlier vouchers count.
    let (_, _, early) = raw(&app, &format!("{base}/reports/trial-balance?as_at=2026-04-02&format=csv"), &viewer).await;
    assert!(early.contains("\r\nHDFC Bank,75000,0\r\n"), "{early}");
    assert!(!early.contains("Sharma"));

    let (_, h, body) = raw(&app, &format!("{base}/reports/day-book?from=2026-04-01&to=2026-04-30&format=csv"), &viewer).await;
    assert_eq!(h["content-disposition"], "attachment; filename=\"demo-traders_day-book-2026-04-01-to-2026-04-30.csv\"");
    assert_eq!(body.lines().next().unwrap(), "Date,Particulars,Vch Type,Vch No.,Debit,Credit,Cancelled");
    assert!(body.contains("\r\n2026-04-05,\"Sharma, \"\"Ltd\"\"\",Sales,1,50000,0,\r\n"), "{body}");
    assert_eq!(body.lines().count(), 5);

    let sharma = book.ledger_by_name("Sharma, \"Ltd\"").unwrap().id.0;
    let (_, h, body) = raw(&app, &format!("{base}/ledgers/{sharma}/statement?from=2026-04-01&to=2026-06-30&format=csv"), &viewer).await;
    assert_eq!(
        h["content-disposition"],
        "attachment; filename=\"demo-traders_ledger-sharma-ltd-2026-04-01-to-2026-06-30.csv\""
    );
    assert_eq!(
        body,
        "Date,Particulars,Vch Type,Vch No.,Debit,Credit,Balance\r\n\
         2026-04-01,Opening Balance,,,,,0\r\n\
         2026-04-05,Sales,Sales,1,50000,0,50000\r\n\
         2026-04-20,HDFC Bank,Receipt,1,0,20000,30000\r\n\
         2026-06-30,Closing Balance,,,,,30000\r\n"
    );

    let (_, h, body) = raw(&app, &format!("{base}/reports/profit-and-loss?from=2026-04-01&to=2027-03-31&format=csv"), &viewer).await;
    assert_eq!(h["content-disposition"], "attachment; filename=\"demo-traders_profit-and-loss-2026-04-01-to-2027-03-31.csv\"");
    assert!(body.contains("\r\nResult,Net Profit,-5000\r\n"), "{body}");
    assert!(body.contains("\r\nResult,Gross Profit,20000\r\n"), "{body}");

    let (_, _, body) = raw(&app, &format!("{base}/reports/balance-sheet?as_at=2027-03-31&format=csv"), &viewer).await;
    assert!(body.contains("\r\nLiabilities,Profit & Loss A/c,-5000\r\n"), "{body}");
    assert!(body.contains("\r\nAssets,Total,125000\r\n"), "{body}");
    assert!(body.contains("\r\nLiabilities,Total,125000\r\n"), "{body}");

    let (_, _, body) = raw(&app, &format!("{base}/ledgers?as_at=2026-06-30&format=csv"), &viewer).await;
    assert!(body.contains("\r\nGupta Supplies,Sundry Creditors,30000,Cr\r\n"), "{body}");
    assert!(body.contains("\r\n\"Sharma, \"\"Ltd\"\"\",Sundry Debtors,30000,Dr\r\n"), "{body}");

    // Unknown formats are rejected rather than silently served as JSON.
    let (s, _, _) = raw(&app, &format!("{base}/reports/trial-balance?format=xlsx"), &viewer).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// `sha256-…` CSP source for some bytes (standard base64).
fn csp_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let digest = Sha256::digest(bytes);
    let mut out = String::new();
    for chunk in digest.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= chunk.len() { A[((n >> (18 - 6 * i)) & 63) as usize] as char } else { '=' });
        }
    }
    format!("sha256-{out}")
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> &'a str {
    let start = s.find(open).unwrap() + open.len();
    &s[start..start + s[start..].find(close).unwrap()]
}

#[tokio::test]
async fn print_html_is_escaped_locked_down_and_printable_from_the_portal() {
    let (app, company, _, viewer, book) = party_company("v2-print").await;
    let base = format!("/v1/companies/{company}");

    let (s, h, html) = raw(&app, &format!("{base}/reports/trial-balance?as_at=2026-06-30&format=pdf-html"), &viewer).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["content-type"], "text/html; charset=utf-8");
    assert_eq!(h["content-disposition"], "attachment; filename=\"demo-traders_trial-balance-2026-06-30.html\"");
    assert!(html.contains("Trial Balance") && html.contains("As at 30-Jun-2026"), "{html}");
    assert!(html.contains("Sharma, &quot;Ltd&quot;"), "names are HTML-escaped");
    assert!(html.contains("95,000.00"), "Indian grouping");
    let print_csp = h["content-security-policy"].to_str().unwrap().to_owned();
    let style = csp_hash(between(&html, "<style>", "</style>").as_bytes());
    assert!(print_csp.contains(&style), "{print_csp}");
    assert!(print_csp.starts_with("default-src 'none'") && !print_csp.contains("script-src"), "{print_csp}");

    let sharma = book.ledger_by_name("Sharma, \"Ltd\"").unwrap().id.0;
    for (uri, title) in [
        (format!("{base}/reports/day-book?from=2026-04-01&to=2026-04-30&format=pdf-html"), "Day Book"),
        (format!("{base}/reports/profit-and-loss?from=2026-04-01&to=2027-03-31&format=pdf-html"), "Profit &amp; Loss A/c"),
        (format!("{base}/reports/balance-sheet?as_at=2027-03-31&format=pdf-html"), "Balance Sheet"),
        (format!("{base}/ledgers/{sharma}/statement?from=2026-04-01&to=2026-06-30&format=pdf-html"), "Ledger: Sharma, &quot;Ltd&quot;"),
    ] {
        let (s, h, html) = raw(&app, &uri, &viewer).await;
        assert_eq!(s, StatusCode::OK, "{uri}");
        assert!(html.contains(title), "{uri}: {html}");
        assert!(h["content-disposition"].to_str().unwrap().ends_with(".html\""), "{uri}");
        assert_eq!(h["content-security-policy"], print_csp.as_str());
    }

    // The portal opens the print HTML from a blob: URL, which inherits the
    // portal's CSP: its style-src must allow the print stylesheet, and the
    // page's own script and style must match their hashes.
    let resp = app.clone().oneshot(Request::get("/portal").body(Body::empty()).unwrap()).await.unwrap();
    let csp = resp.headers()["content-security-policy"].to_str().unwrap().to_owned();
    let page = String::from_utf8(resp.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(csp.contains(&style), "{csp}");
    assert!(csp.contains(&format!("script-src '{}'", csp_hash(between(&page, "<script>", "</script>").as_bytes()))), "{csp}");
    assert!(csp.contains(&csp_hash(between(&page, "<style>", "</style>").as_bytes())), "{csp}");
    assert!(!csp.contains("unsafe"), "{csp}");
    assert_eq!(page.matches("<script>").count(), 1, "one inline script, covered by the hash");
    assert_eq!(page.matches("<style>").count(), 1);
    assert!(!page.contains("innerHTML") && !page.contains("insertAdjacentHTML") && !page.contains("document.write"));
    assert!(!page.contains(" style=") && !page.contains(" onclick="), "inline attributes would be blocked by the CSP");
    // Top-level script bindings may not shadow the browser's non-configurable
    // globals: `const top = …` is a SyntaxError and the whole page dies.
    for global in ["top", "window", "document", "location"] {
        for decl in ["const", "let", "function", "class"] {
            assert!(!page.contains(&format!("\n{decl} {global} ")), "{decl} {global}");
            assert!(!page.contains(&format!("\n{decl} {global}(")), "{decl} {global}(");
        }
    }
    for feature in ["Download PDF", "Download CSV", "Audit trail CSV", "msme-overdue", "groups/", "/statement", "href=\"/source\""] {
        assert!(page.contains(feature), "portal is missing {feature}");
    }
}

#[tokio::test]
async fn audit_trail_export_lists_every_entry_with_its_pusher() {
    let (app, company, owner, viewer, mut book) = party_company("v2-audit").await;
    let base = format!("/v1/companies/{company}");
    let first_len = book.audit().entries().len();

    // An accountant (the company's CA) pushes one more voucher.
    let (_, accountant) = share(&app, &company, &owner, Role::Accountant, None).await;
    let rent = book.ledger_by_name("Rent").unwrap().id;
    let bank = book.ledger_by_name("HDFC Bank").unwrap().id;
    book.post_voucher(now(), "ca, \"Mehta\"", Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Payment,
        number: "2".into(),
        date: d(2026, 5, 1),
        entries: vec![Entry::dr(rent, dec!(1000)), Entry::cr(bank, dec!(1000))],
        narration: String::new(),
        cancelled: false, stock: Vec::new(),
    })
    .unwrap();
    let (s, v) = call(&app, "POST", &format!("{base}/push"), &accountant, Some(push_body(first_len as u64, &book))).await;
    assert_eq!(s, StatusCode::OK, "{v}");

    let (s, v) = call(&app, "GET", &format!("{base}/reports/audit-trail"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let lines = v.as_array().unwrap();
    assert_eq!(lines.len(), book.audit().entries().len());
    for (line, e) in lines.iter().zip(book.audit().entries()) {
        assert_eq!(line["seq"], e.seq);
        assert_eq!(line["hash"], e.hash);
        assert_eq!(line["entity"], e.entity);
    }
    assert_eq!(lines[0]["pushed_by"], "owner");
    assert_eq!(lines[0]["action"], "Create");
    assert_eq!(lines[first_len - 1]["pushed_by"], "owner");
    assert_eq!(lines[first_len]["pushed_by"], "Mehta & Co.");
    assert_eq!(lines[first_len]["user"], "ca, \"Mehta\"");
    assert!(lines[first_len]["received_at"].is_string());

    let (s, h, csv) = raw(&app, &format!("{base}/reports/audit-trail?format=csv"), &viewer).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["content-disposition"], "attachment; filename=\"demo-traders_audit-trail.csv\"");
    let mut rows = csv.split("\r\n");
    assert_eq!(rows.next().unwrap(), "Seq,At (UTC),User,Action,Entity,Pushed By,Received At (UTC),Hash");
    assert_eq!(csv.matches("\r\n").count(), book.audit().entries().len() + 1);
    let last = book.audit().entries().last().unwrap();
    assert!(
        csv.contains(&format!("\r\n{},2025-10-09T08:53:20+00:00,\"ca, \"\"Mehta\"\"\",Create,{},Mehta & Co.,", last.seq, last.entity)),
        "{csv}"
    );
    assert!(csv.ends_with(&format!(",{}\r\n", last.hash)));

    let (s, _, _) = raw(&app, &format!("{base}/reports/audit-trail?format=pdf-html"), &viewer).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// Foreign-currency ledgers and vouchers (with a realised gain) push, pull
/// and rebuild identically; a forged line in the wrong currency is refused
/// by the same rules as on the device.
#[tokio::test]
async fn foreign_currency_vouchers_sync_and_wrong_currencies_are_rejected() {
    let app = app("forex");
    let (company, owner) = new_company(&app).await;
    let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    let mut book = Book::new("Demo Traders");
    book.set_country(now(), "owner", "IN").unwrap();
    let acme = book.create_ledger(now(), "owner", "Acme Inc", book.group_by_name("Sundry Debtors").unwrap().id).unwrap();
    let l = khata_core::Ledger { currency: Some("USD".into()), ..book.ledger(acme).unwrap().clone() };
    book.alter_ledger(now(), "owner", l).unwrap();
    let bank = book.create_ledger(now(), "owner", "HDFC Bank", book.group_by_name("Bank Accounts").unwrap().id).unwrap();
    let sales = book.create_ledger(now(), "owner", "Export Sales", book.group_by_name("Sales Accounts").unwrap().id).unwrap();
    let gl = book.create_ledger(now(), "owner", khata_core::forex::FOREX_LEDGER, book.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    let v = |kind, date, entries| Voucher { id: VoucherId::new(), kind, number: "1".into(), date, entries, narration: String::new(), cancelled: false, stock: Vec::new() };
    book.post_voucher(now(), "clerk", v(VoucherKind::Sales, d(4, 10), vec![
        Entry::dr(acme, dec!(80000)).fx("USD", dec!(1000), dec!(80)).fx_bill(BillKind::New, "EXP-1", dec!(80000), dec!(1000), None),
        Entry::cr(sales, dec!(80000)),
    ]))
    .unwrap();
    let mut receipt = v(VoucherKind::Receipt, d(5, 2), vec![
        Entry::dr(bank, dec!(83000)),
        Entry::cr(acme, dec!(83000)).fx("USD", dec!(1000), dec!(83)).fx_bill(BillKind::Against, "EXP-1", dec!(83000), dec!(1000), None),
    ]);
    khata_core::forex::settle(&book, &mut receipt, gl, 2);
    book.post_voucher(now(), "clerk", receipt).unwrap();
    book.set_fx_rate(now(), "owner", "USD", d(5, 31), Some(dec!(83.4))).unwrap();

    let (s, v2) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(push_body(0, &book))).await;
    assert_eq!(s, StatusCode::OK, "{v2}");
    let len = book.audit().entries().len() as u64;
    assert_eq!(v2["len"], len);
    let (_, pulled) = call(&app, "GET", &format!("/v1/companies/{company}/pull?from=0"), &owner, None).await;
    let pulled: Pulled = serde_json::from_value(pulled).unwrap();
    let replica = Book::replay(pulled.company, khata_core::AuditLog::from(pulled.entries)).unwrap();
    assert_eq!(replica.trial_balance(), book.trial_balance());
    assert_eq!(replica.vouchers().collect::<Vec<_>>(), book.vouchers().collect::<Vec<_>>());
    assert_eq!(replica.fx_rate("USD", d(6, 1)).unwrap().rate, dec!(83.4));

    // A forged euro line on the dollar ledger, with a valid hash chain.
    let mut bad = v(VoucherKind::Sales, d(5, 3), vec![Entry::dr(acme, dec!(90)).fx("EUR", dec!(1), dec!(90)), Entry::cr(sales, dec!(90))]);
    bad.number = "2".into();
    let mut log = book.audit().clone();
    log.record(now(), "mallory", AuditAction::Create, format!("voucher:{}", bad.id.0), None, Some(json!(bad)));
    let forged = log.entries()[len as usize..].to_vec();
    let (s, err) = call(&app, "POST", &format!("/v1/companies/{company}/push"), &owner, Some(json!({ "base_len": len, "entries": forged }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    assert!(err["error"].as_str().unwrap().contains("kept in USD"), "{err}");
}

fn record(key: &str, value: Option<&str>, at: i64, device: &str) -> Value {
    json!({ "key": key, "value": value, "at": at, "author": "asha", "device": device })
}

#[tokio::test]
async fn shared_records_merge_last_writer_wins_and_only_writers_push() {
    let app = app("records");
    let (company, owner) = new_company(&app).await;
    let (_, accountant) = share(&app, &company, &owner, Role::Accountant, None).await;
    let (_, viewer) = share(&app, &company, &owner, Role::Viewer, Some(30)).await;
    let uri = format!("/v1/companies/{company}/records");

    let body = json!({ "records": [record("desk.note.1", Some("v1"), 10, "a"), record("invoice.design", Some("{}"), 10, "a")] });
    let (s, v) = call(&app, "POST", &uri, &owner, Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let pushed: RecordsPushed = serde_json::from_value(v).unwrap();
    assert_eq!(pushed, RecordsPushed { rev: 2, changed: 2, protocol: PROTOCOL_VERSION });

    // An older version changes nothing; a newer one replaces it; a deletion
    // forever wins over a later edit.
    let body = json!({ "records": [record("desk.note.1", Some("stale"), 5, "b"), record("invoice.design", Some("{\"accent\":1}"), 11, "b")] });
    let (_, v) = call(&app, "POST", &uri, &accountant, Some(body)).await;
    assert_eq!((v["rev"].as_u64(), v["changed"].as_u64()), (Some(3), Some(1)));
    let gone = json!({ "records": [{ "key": "desk.note.1", "value": null, "at": 12, "author": "asha", "device": "a", "forever": true }] });
    assert_eq!(call(&app, "POST", &uri, &owner, Some(gone)).await.0, StatusCode::OK);
    let late = json!({ "records": [record("desk.note.1", Some("edited offline"), 99, "b")] });
    assert_eq!(call(&app, "POST", &uri, &accountant, Some(late)).await.1["changed"], 0);

    // Viewers read but cannot write.
    let (s, v) = call(&app, "GET", &format!("{uri}?since=0"), &viewer, None).await;
    assert_eq!(s, StatusCode::OK);
    let pulled: PulledRecords = serde_json::from_value(v).unwrap();
    assert_eq!((pulled.rev, pulled.more, pulled.protocol), (4, false, PROTOCOL_VERSION));
    let by_key: Vec<(&str, Option<&str>)> = pulled.records.iter().map(|r| (r.record.key.as_str(), r.record.value.as_deref())).collect();
    assert_eq!(by_key, [("invoice.design", Some("{\"accent\":1}")), ("desk.note.1", None)]);
    let body = json!({ "records": [record("desk.note.2", Some("x"), 1, "v")] });
    assert_eq!(call(&app, "POST", &uri, &viewer, Some(body)).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "GET", &format!("{uri}?since=0"), "not-a-token", None).await.0, StatusCode::UNAUTHORIZED);

    // Device-local keys and malformed records are refused whole.
    for bad in [record("sync.token", Some("secret"), 1, "a"), record("share.key", Some("k"), 1, "a"), record("zatca.device", Some("{}"), 1, "a"), record("../x", Some("x"), 1, "a"), record("desk.note.3", Some("x"), -1, "a")] {
        let (s, v) = call(&app, "POST", &uri, &owner, Some(json!({ "records": [record("desk.note.4", Some("ok"), 1, "a"), bad] }))).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    }
    let (_, v) = call(&app, "GET", &format!("{uri}?since=4"), &owner, None).await;
    assert_eq!(v["records"].as_array().unwrap().len(), 0, "nothing of a refused push was kept");

    // Records are not books: the trail is untouched.
    let (_, v) = call(&app, "GET", &format!("/v1/companies/{company}/pull?from=0"), &owner, None).await;
    assert_eq!((v["entries"].as_array().unwrap().len(), v["protocol"].as_u64()), (0, Some(u64::from(PROTOCOL_VERSION))));
}

#[tokio::test]
async fn record_pulls_page_by_count_and_bytes() {
    let app = app("records-paging");
    let (company, owner) = new_company(&app).await;
    let uri = format!("/v1/companies/{company}/records");
    let big = "x".repeat(1024 * 1024);
    let list: Vec<Value> = (0..6).map(|i| record(&format!("invoice.voucher.{i}"), Some(&big), 1, "a")).collect();
    assert_eq!(call(&app, "POST", &uri, &owner, Some(json!({ "records": list }))).await.0, StatusCode::OK);
    let (mut since, mut pages, mut seen) = (0, 0, 0);
    loop {
        let (_, v) = call(&app, "GET", &format!("{uri}?since={since}"), &owner, None).await;
        let page: PulledRecords = serde_json::from_value(v).unwrap();
        pages += 1;
        seen += page.records.len();
        since = page.records.last().map_or(since, |r| r.rev);
        if !page.more {
            break;
        }
    }
    assert_eq!((seen, pages), (6, 2), "4 MB of values per page");
}

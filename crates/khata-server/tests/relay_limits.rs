//! Abuse limits of an open relay: sign-ups per client address per day,
//! bytes per relay company, and bytes in all.

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use khata_server::{AppState, Config, RelayLimits, SharedState, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const ADMIN: &str = "test-admin-key-0123456789abcdef";
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn relay(name: &str, limits: RelayLimits) -> (SharedState, Router) {
    let dir = std::env::temp_dir().join(format!("khata-relay-limits-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let state = AppState::open(Config { data_dir: dir, admin_key: ADMIN.into() }).unwrap();
    state.set_open_relay(true);
    state.set_relay_limits(limits);
    (state.clone(), router(state))
}

/// A request as it arrives from `peer` (None: no peer address, as when the
/// router is called directly), with optional extra headers.
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    peer: Option<&str>,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut req = req.body(body.map_or(Body::empty(), |b| Body::from(b.to_string()))).unwrap();
    if let Some(p) = peer {
        req.extensions_mut().insert(ConnectInfo(p.parse::<SocketAddr>().unwrap()));
    }
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn sign_up(app: &Router, peer: Option<&str>, headers: &[(&str, &str)]) -> StatusCode {
    call(app, "POST", "/v1/relay", None, peer, headers, None).await.0
}

/// Create a relay company: (id, owner token).
async fn company(app: &Router) -> (String, String) {
    let (s, v) = call(app, "POST", "/v1/relay", Some(ADMIN), None, &[], None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    (v["company_id"].as_str().unwrap().to_owned(), v["owner_token"].as_str().unwrap().to_owned())
}

/// `n` sealed-looking entries from `from` on; each stores 128 + `blob_hex`
/// bytes (two hashes and the blob).
fn entries(from: u64, n: u64, blob_hex: usize) -> Value {
    let hash = |seq: u64| format!("{:064x}", seq + 1);
    let list: Vec<Value> = (from..from + n)
        .map(|seq| {
            json!({
                "seq": seq,
                "prev_hash": if seq == 0 { GENESIS.to_owned() } else { hash(seq - 1) },
                "hash": hash(seq),
                "blob": "ab".repeat(blob_hex / 2),
            })
        })
        .collect();
    json!({ "base_len": from, "entries": list })
}

async fn push(app: &Router, id: &str, token: &str, from: u64, n: u64, blob_hex: usize) -> (StatusCode, Value) {
    call(app, "POST", &format!("/v1/relay/{id}/push"), Some(token), None, &[], Some(entries(from, n, blob_hex))).await
}

async fn stored(app: &Router, id: &str, token: &str) -> usize {
    let (s, v) = call(app, "GET", &format!("/v1/relay/{id}/pull?from=0"), Some(token), None, &[], None).await;
    assert_eq!(s, StatusCode::OK);
    v["entries"].as_array().unwrap().len()
}

#[tokio::test]
async fn sign_ups_are_limited_per_client_address_per_day() {
    let (_, app) = relay("per-ip", RelayLimits { max_new_per_ip_day: 2, ..RelayLimits::default() });
    let a = Some("203.0.113.7:50000");
    assert_eq!(sign_up(&app, a, &[]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, Some("203.0.113.7:50001"), &[]).await, StatusCode::OK, "same address, other port");
    let (s, v) = call(&app, "POST", "/v1/relay", None, a, &[], None).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(v["error"].as_str().unwrap().contains("at most 2 new companies a day"), "{v}");

    // Another address has its own allowance; the operator's admin key has none.
    assert_eq!(sign_up(&app, Some("198.51.100.1:1"), &[]).await, StatusCode::OK);
    let (s, _) = call(&app, "POST", "/v1/relay", Some(ADMIN), a, &[], None).await;
    assert_eq!(s, StatusCode::OK);

    // IPv6 clients are counted per /64.
    assert_eq!(sign_up(&app, Some("[2001:db8:1:2::1]:1"), &[]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, Some("[2001:db8:1:2::ffff]:1"), &[]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, Some("[2001:db8:1:2:abcd::9]:1"), &[]).await, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(sign_up(&app, Some("[2001:db8:1:3::1]:1"), &[]).await, StatusCode::OK);

    // Without a peer address (router called directly) everyone shares one bucket.
    assert_eq!(sign_up(&app, None, &[]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, None, &[]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, None, &[]).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn forwarded_for_counts_only_behind_a_trusted_proxy() {
    let proxy = Some("127.0.0.1:40000");
    // Not trusted: X-Forwarded-For is ignored, so a client cannot dodge the
    // limit by sending a different header each time.
    let (_, app) = relay("xff-untrusted", RelayLimits { max_new_per_ip_day: 1, ..RelayLimits::default() });
    assert_eq!(sign_up(&app, proxy, &[("x-forwarded-for", "192.0.2.1")]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, proxy, &[("x-forwarded-for", "192.0.2.2")]).await, StatusCode::TOO_MANY_REQUESTS);

    // Trusted: the last hop (added by the proxy) is the client.
    let (_, app) =
        relay("xff-trusted", RelayLimits { max_new_per_ip_day: 1, trust_proxy: true, ..RelayLimits::default() });
    assert_eq!(sign_up(&app, proxy, &[("x-forwarded-for", "192.0.2.1")]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, proxy, &[("x-forwarded-for", "192.0.2.2")]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, proxy, &[("x-forwarded-for", "192.0.2.1")]).await, StatusCode::TOO_MANY_REQUESTS);
    // A client-supplied first hop doesn't help: the proxy appended the real one.
    assert_eq!(
        sign_up(&app, proxy, &[("x-forwarded-for", "10.9.9.9, 192.0.2.1")]).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // No header: the peer address counts.
    assert_eq!(sign_up(&app, proxy, &[]).await, StatusCode::OK);
    assert_eq!(sign_up(&app, proxy, &[]).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_relay_company_cannot_store_more_than_its_limit() {
    // Each entry stores 128 + 400 = 528 bytes.
    let (_, app) = relay("per-company", RelayLimits { max_bytes_per_company: 1000, ..RelayLimits::default() });
    let (id, token) = company(&app).await;
    assert_eq!(push(&app, &id, &token, 0, 1, 400).await.0, StatusCode::OK);
    let (s, v) = push(&app, &id, &token, 1, 1, 400).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(v["error"].as_str().unwrap().contains("storage limit"), "{v}");
    // The whole push was refused; what was stored is unchanged.
    assert_eq!(stored(&app, &id, &token).await, 1);
    // A smaller entry still fits, and other companies have their own allowance.
    assert_eq!(push(&app, &id, &token, 1, 1, 200).await.0, StatusCode::OK);
    let (other, other_token) = company(&app).await;
    assert_eq!(push(&app, &other, &other_token, 0, 1, 400).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_relay_as_a_whole_cannot_store_more_than_its_limit() {
    let limits = RelayLimits { max_bytes_per_company: 0, max_total_bytes: Some(1056), ..RelayLimits::default() };
    let (state, app) = relay("total", limits);
    let (a, a_token) = company(&app).await;
    let (b, b_token) = company(&app).await;
    assert_eq!(push(&app, &a, &a_token, 0, 2, 400).await.0, StatusCode::OK, "exactly at the limit");
    let (s, v) = push(&app, &b, &b_token, 0, 1, 400).await;
    assert_eq!(s, StatusCode::INSUFFICIENT_STORAGE);
    assert!(v["error"].as_str().unwrap().contains("relay is full"), "{v}");
    assert_eq!(stored(&app, &b, &b_token).await, 0);
    // No new companies on a full relay, not even with sign-ups open.
    assert_eq!(sign_up(&app, Some("192.0.2.9:1"), &[]).await, StatusCode::INSUFFICIENT_STORAGE);

    // Usage is kept on disk: a restart doesn't reset it.
    drop(app);
    let dir = std::env::temp_dir().join(format!("khata-relay-limits-{}-total", std::process::id()));
    drop(state);
    let state = AppState::open(Config { data_dir: dir, admin_key: ADMIN.into() }).unwrap();
    state.set_relay_limits(limits);
    let app = router(state);
    assert_eq!(push(&app, &b, &b_token, 0, 1, 400).await.0, StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(stored(&app, &a, &a_token).await, 2);
}

#[tokio::test]
async fn sealed_records_count_towards_the_limits_and_viewers_cannot_push_them() {
    let (_, app) = relay("records", RelayLimits { max_bytes_per_company: 1000, ..RelayLimits::default() });
    let (id, token) = company(&app).await;
    let records = |hex: usize| json!({ "records": ["cd".repeat(hex / 2)] });
    let uri = format!("/v1/relay/{id}/records");
    let (s, v) = call(&app, "POST", &uri, Some(&token), None, &[], Some(records(600))).await;
    assert_eq!((s, v["rev"].as_u64(), v["protocol"].as_u64()), (StatusCode::OK, Some(1), Some(2)));
    // Entries and records share the company's allowance.
    assert_eq!(push(&app, &id, &token, 0, 1, 400).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    let (s, v) = call(&app, "POST", &uri, Some(&token), None, &[], Some(records(600))).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(v["error"].as_str().unwrap().contains("storage limit"), "{v}");

    // Not hex, or larger than a sealed record can be: refused.
    let (s, _) = call(&app, "POST", &uri, Some(&token), None, &[], Some(json!({ "records": ["not hex"] }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (big, big_token) = company(&app).await;
    let huge = json!({ "records": ["ab".repeat(khata_server::relay::MAX_RECORD_HEX / 2 + 1)] });
    let (s, _) = call(&app, "POST", &format!("/v1/relay/{big}/records"), Some(&big_token), None, &[], Some(huge)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // A viewer pulls the sealed records but cannot add any.
    let (s, v) = call(&app, "POST", &format!("/v1/companies/{id}/shares"), Some(&token), None, &[], Some(json!({ "role": "viewer", "label": "auditor" }))).await;
    assert_eq!(s, StatusCode::OK);
    let viewer = v["token"].as_str().unwrap().to_owned();
    let (s, v) = call(&app, "GET", &format!("{uri}?since=0"), Some(&viewer), None, &[], None).await;
    assert_eq!((s, v["records"].as_array().map(Vec::len)), (StatusCode::OK, Some(1)));
    assert_eq!(v["records"][0]["rev"], 1);
    let (s, _) = call(&app, "POST", &uri, Some(&viewer), None, &[], Some(records(10))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = call(&app, "GET", &format!("{uri}?since=1"), Some(&viewer), None, &[], None).await;
    assert_eq!((s, v["records"].as_array().map(Vec::len), v["more"].as_bool()), (StatusCode::OK, Some(0), Some(false)));
}

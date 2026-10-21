//! Blind relay: end-to-end encrypted companies.
//!
//! For sharing over the internet without anyone hosting their own server, a
//! managed relay stores each company's audit entries *sealed*
//! ([`khata_seal::SealedEntry`]) with a key that only the company's devices
//! and invite codes hold. The relay sees entry numbers and the hash chain —
//! enough to keep one consistent order — but no names, amounts or ledgers.
//! Devices validate everything they pull with the same rules as local edits,
//! so a relay cannot forge entries either (it cannot produce a valid seal).
//!
//! Relay companies use the same tokens and roles as normal companies. If the
//! operator enables open sign-up, anyone may create a relay company without
//! the admin key: there is nothing readable to protect.
//!
//! A relay open to the public is limited ([`RelayLimits`]): new companies per
//! client address per day, bytes stored per company, and optionally bytes
//! stored in all. Limits only refuse new data; nothing stored is ever
//! changed or removed.
//!
//! **Shared records** (notes, the invoice design…) are sealed too, one blob
//! per version, and appended to their own log (`/v1/relay/{id}/records`).
//! The relay sees neither keys nor values, so it cannot merge them: it
//! hands every blob to every device, and devices merge (last writer wins).
//! A replayed old blob changes nothing. Blobs count towards the same
//! storage limits.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{Extensions, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use khata_seal::SealedEntry;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{self, Role};
use crate::{ApiError, SharedState, lock};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS relay_companies (
    id         TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS relay_entries (
    company   TEXT NOT NULL,
    seq       INTEGER NOT NULL,
    prev_hash TEXT NOT NULL,
    hash      TEXT NOT NULL,
    blob      TEXT NOT NULL,
    PRIMARY KEY (company, seq)
);
CREATE TRIGGER IF NOT EXISTS relay_no_update BEFORE UPDATE ON relay_entries
BEGIN SELECT RAISE(ABORT, 'relay trail is append-only'); END;
CREATE TRIGGER IF NOT EXISTS relay_no_delete BEFORE DELETE ON relay_entries
BEGIN SELECT RAISE(ABORT, 'relay trail is append-only'); END;
CREATE TABLE IF NOT EXISTS relay_records (
    company TEXT NOT NULL,
    rev     INTEGER NOT NULL,
    blob    TEXT NOT NULL,
    PRIMARY KEY (company, rev)
);
CREATE TRIGGER IF NOT EXISTS relay_records_no_update BEFORE UPDATE ON relay_records
BEGIN SELECT RAISE(ABORT, 'relay records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS relay_records_no_delete BEFORE DELETE ON relay_records
BEGIN SELECT RAISE(ABORT, 'relay records are append-only'); END;
CREATE TABLE IF NOT EXISTS relay_usage (
    company TEXT PRIMARY KEY,
    bytes   INTEGER NOT NULL
);
";

/// Create the relay tables, and count what relays from before
/// `relay_usage` existed already store (once per company).
pub(crate) fn init(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)?;
    conn.execute(
        "INSERT INTO relay_usage (company, bytes)
         SELECT c.id, COALESCE((SELECT SUM(length(e.blob) + length(e.hash) + length(e.prev_hash))
                                FROM relay_entries e WHERE e.company = c.id), 0)
         FROM relay_companies c WHERE c.id NOT IN (SELECT company FROM relay_usage)",
        [],
    )?;
    Ok(())
}

/// Largest sealed entry accepted (hex characters).
const MAX_BLOB_HEX: usize = 512 * 1024;
/// Largest sealed record accepted (hex characters): a record of
/// [`khata_store::records::MAX_RECORD_VALUE`] sealed, with room for its JSON.
pub const MAX_RECORD_HEX: usize = 2 * (khata_store::records::MAX_RECORD_VALUE + 16 * 1024);
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const MIB: u64 = 1024 * 1024;

/// Abuse limits for a relay that anyone may sign up to. Set from the
/// environment in `main.rs`; the defaults suit a small public relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayLimits {
    /// New relay companies per client address per day (UTC) without the
    /// admin key. 0 = no limit.
    pub max_new_per_ip_day: u32,
    /// Bytes one relay company may store. 0 = no limit.
    pub max_bytes_per_company: u64,
    /// Bytes all relay companies together may store.
    pub max_total_bytes: Option<u64>,
    /// Take the client address from `X-Forwarded-For` (set by a reverse
    /// proxy such as Caddy or Cloudflare Tunnel). Only when the server is
    /// reachable through that proxy alone, or clients could pick any address.
    pub trust_proxy: bool,
}

impl Default for RelayLimits {
    fn default() -> Self {
        Self { max_new_per_ip_day: 20, max_bytes_per_company: 1024 * MIB, max_total_bytes: None, trust_proxy: false }
    }
}

/// Relay limits and today's sign-up counts (in memory: a restart forgets
/// them, which only ever errs on the side of letting people in).
#[derive(Default)]
pub(crate) struct Guard {
    pub(crate) limits: Mutex<RelayLimits>,
    /// (day number, sign-ups per client address that day).
    signups: Mutex<(i64, HashMap<String, u32>)>,
}

/// A refusal with its own status code (limits), or any other API error.
pub(crate) enum RelayError {
    Api(ApiError),
    Limit(StatusCode, String),
}

impl From<ApiError> for RelayError {
    fn from(e: ApiError) -> Self {
        Self::Api(e)
    }
}

impl From<rusqlite::Error> for RelayError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Api(e.into())
    }
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        match self {
            Self::Api(e) => e.into_response(),
            Self::Limit(status, error) => (status, Json(serde_json::json!({ "error": error }))).into_response(),
        }
    }
}

/// Who is signing up, for the per-address limit. IPv6 clients are counted
/// per /64, the block one connection usually gets. Without a peer address
/// (tests calling the router directly) everyone shares one bucket.
fn client_bucket(limits: &RelayLimits, headers: &HeaderMap, ext: &Extensions) -> String {
    let forwarded = limits
        .trust_proxy
        .then(|| {
            // The proxy appends the address it saw: the last entry is the
            // only one a client cannot choose.
            let last = headers.get_all("x-forwarded-for").iter().filter_map(|v| v.to_str().ok()).next_back()?;
            last.rsplit(',').next()?.trim().parse::<IpAddr>().ok()
        })
        .flatten();
    let ip = forwarded.or_else(|| ext.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip()));
    match ip {
        Some(IpAddr::V6(v6)) if v6.to_ipv4_mapped().is_none() => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Some(IpAddr::V6(v6)) => v6.to_ipv4_mapped().map_or_else(String::new, |v4| v4.to_string()),
        Some(IpAddr::V4(v4)) => v4.to_string(),
        None => "unknown".into(),
    }
}

/// Count a sign-up from `bucket`, refusing it past today's limit.
fn take_signup(guard: &Guard, bucket: String, max: u32) -> Result<(), RelayError> {
    if max == 0 {
        return Ok(());
    }
    let today = Utc::now().timestamp().div_euclid(86_400);
    let mut signups = lock(&guard.signups);
    if signups.0 != today {
        *signups = (today, HashMap::new());
    }
    let n = signups.1.entry(bucket).or_insert(0);
    if *n >= max {
        return Err(RelayError::Limit(
            StatusCode::TOO_MANY_REQUESTS,
            format!("this relay accepts at most {max} new companies a day from one address; try again tomorrow"),
        ));
    }
    *n += 1;
    Ok(())
}

fn total_used(conn: &rusqlite::Connection) -> Result<u64, RelayError> {
    Ok(conn.query_row("SELECT COALESCE(SUM(bytes), 0) FROM relay_usage", [], |r| r.get::<_, i64>(0))? as u64)
}

fn full_relay() -> RelayError {
    RelayError::Limit(
        StatusCode::INSUFFICIENT_STORAGE,
        "this relay is full and accepts no more data; move the company to another relay, or ask the relay's operator".into(),
    )
}

/// What one sealed entry costs in storage.
fn stored_size(e: &SealedEntry) -> u64 {
    (e.blob.len() + e.hash.len() + e.prev_hash.len()) as u64
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayCreated {
    pub company_id: Uuid,
    pub owner_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPush {
    pub base_len: u64,
    pub entries: Vec<SealedEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPulled {
    pub entries: Vec<SealedEntry>,
    pub more: bool,
    /// [`crate::protocol::PROTOCOL_VERSION`]; absent from older relays.
    #[serde(default)]
    pub protocol: u32,
}

/// Sealed shared records a device pushes: hex of
/// `khata_seal::seal(key, record_context(company), record JSON)`.
#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPushRecords {
    pub records: Vec<String>,
}

/// One sealed record and the revision the relay gave it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SealedRecord {
    pub rev: u64,
    pub blob: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayRecordsPushed {
    pub rev: u64,
    pub protocol: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayPulledRecords {
    pub records: Vec<SealedRecord>,
    pub more: bool,
    pub protocol: u32,
}

pub(crate) async fn create(
    State(state): State<SharedState>,
    headers: HeaderMap,
    ext: Extensions,
) -> Result<Json<RelayCreated>, RelayError> {
    let limits = *lock(&state.relay.limits);
    // With the admin key, the operator creates companies without limits.
    if auth::require_admin(&state, &headers).is_err() {
        if !state.open_relay.load(Ordering::Relaxed) {
            return Err(ApiError::Unauthorized.into());
        }
        take_signup(&state.relay, client_bucket(&limits, &headers, &ext), limits.max_new_per_ip_day)?;
    }
    let company_id = Uuid::now_v7();
    {
        let mut conn = lock(&state.registry);
        if let Some(max) = limits.max_total_bytes
            && total_used(&conn)? >= max
        {
            return Err(full_relay());
        }
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO relay_companies (id, created_at) VALUES (?1, ?2)",
            params![company_id.to_string(), Utc::now().timestamp()],
        )?;
        tx.execute("INSERT INTO relay_usage (company, bytes) VALUES (?1, 0)", [company_id.to_string()])?;
        tx.commit()?;
    }
    let (_, owner_token) = auth::issue(&state, company_id, Role::Owner, "owner", None)?;
    Ok(Json(RelayCreated { company_id, owner_token }))
}

fn is_relay(state: &SharedState, id: Uuid) -> Result<bool, ApiError> {
    Ok(lock(&state.registry)
        .query_row("SELECT 1 FROM relay_companies WHERE id = ?1", [id.to_string()], |_| Ok(()))
        .optional()?
        .is_some())
}

fn tip(conn: &rusqlite::Connection, id: Uuid) -> Result<(u64, String), ApiError> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT seq, hash FROM relay_entries WHERE company = ?1 ORDER BY seq DESC LIMIT 1",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.map_or((0, GENESIS.to_owned()), |(seq, hash)| (seq as u64 + 1, hash)))
}

pub(crate) async fn push(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<RelayPush>,
) -> Result<Json<crate::protocol::Pushed>, RelayError> {
    let caller = auth::authorize(&state, &headers, id)?;
    if !caller.role.can_write() {
        return Err(ApiError::Forbidden.into());
    }
    if !is_relay(&state, id)? {
        return Err(ApiError::NotFound.into());
    }
    if req.entries.len() > crate::routes::MAX_PUSH_ENTRIES {
        return Err(ApiError::BadRequest("too many entries".into()).into());
    }
    let limits = *lock(&state.relay.limits);
    let mut conn = lock(&state.registry);
    let (len, mut prev) = tip(&conn, id)?;
    if req.base_len != len {
        return Err(ApiError::Conflict { server_len: len }.into());
    }
    // The relay cannot read entries, but it keeps the chain continuous.
    for (i, e) in req.entries.iter().enumerate() {
        let hex64 = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        if e.seq != len + i as u64 || e.prev_hash != prev || !hex64(&e.hash) {
            return Err(ApiError::Rejected(format!("entry {} does not continue the trail", e.seq)).into());
        }
        if e.blob.len() > MAX_BLOB_HEX || !e.blob.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ApiError::BadRequest(format!("entry {} is not a sealed blob", e.seq)).into());
        }
        prev = e.hash.clone();
    }
    // Storage limits: refuse the whole push; what is stored stays as it is.
    let adding: u64 = req.entries.iter().map(stored_size).sum();
    check_room(&conn, &limits, id, adding)?;
    let tx = conn.transaction()?;
    for e in &req.entries {
        tx.execute(
            "INSERT INTO relay_entries (company, seq, prev_hash, hash, blob) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id.to_string(), e.seq as i64, e.prev_hash, e.hash, e.blob],
        )?;
    }
    tx.execute(
        "INSERT INTO relay_usage (company, bytes) VALUES (?1, ?2)
         ON CONFLICT(company) DO UPDATE SET bytes = bytes + excluded.bytes",
        params![id.to_string(), adding as i64],
    )?;
    tx.commit()?;
    drop(conn);
    let new_len = len + req.entries.len() as u64;
    if new_len > len {
        auth::log_push(&state, id, caller, len, new_len)?;
    }
    Ok(Json(crate::protocol::Pushed { len: new_len }))
}

#[derive(Deserialize)]
pub(crate) struct PullQuery {
    #[serde(default)]
    from: u64,
}

pub(crate) async fn pull(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PullQuery>,
    headers: HeaderMap,
) -> Result<Json<RelayPulled>, ApiError> {
    auth::authorize(&state, &headers, id)?;
    if !is_relay(&state, id)? {
        return Err(ApiError::NotFound);
    }
    let conn = lock(&state.registry);
    let page = crate::routes::PULL_PAGE as i64;
    let mut stmt = conn.prepare(
        "SELECT seq, prev_hash, hash, blob FROM relay_entries WHERE company = ?1 AND seq >= ?2 ORDER BY seq LIMIT ?3",
    )?;
    let mut entries: Vec<SealedEntry> = stmt
        .query_map(params![id.to_string(), q.from as i64, page + 1], |r| {
            Ok(SealedEntry { seq: r.get::<_, i64>(0)? as u64, prev_hash: r.get(1)?, hash: r.get(2)?, blob: r.get(3)? })
        })?
        .collect::<Result<_, _>>()?;
    let more = entries.len() as i64 > page;
    entries.truncate(page as usize);
    Ok(Json(RelayPulled { entries, more, protocol: crate::protocol::PROTOCOL_VERSION }))
}

/// Check that adding `adding` bytes keeps the company and the relay within
/// their storage limits.
fn check_room(conn: &rusqlite::Connection, limits: &RelayLimits, id: Uuid, adding: u64) -> Result<(), RelayError> {
    if adding == 0 {
        return Ok(());
    }
    let used: u64 = conn
        .query_row("SELECT bytes FROM relay_usage WHERE company = ?1", [id.to_string()], |r| r.get::<_, i64>(0))
        .optional()?
        .unwrap_or(0) as u64;
    let max = limits.max_bytes_per_company;
    if max > 0 && used + adding > max {
        return Err(RelayError::Limit(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "this company has reached the relay's storage limit of {} MB; move it to another relay, or ask the relay's operator",
                max / MIB
            ),
        ));
    }
    if let Some(max) = limits.max_total_bytes
        && total_used(conn)? + adding > max
    {
        return Err(full_relay());
    }
    Ok(())
}

pub(crate) async fn push_records(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<RelayPushRecords>,
) -> Result<Json<RelayRecordsPushed>, RelayError> {
    let caller = auth::authorize(&state, &headers, id)?;
    if !caller.role.can_write() {
        return Err(ApiError::Forbidden.into());
    }
    if !is_relay(&state, id)? {
        return Err(ApiError::NotFound.into());
    }
    if req.records.len() > crate::records::MAX_PUSH_RECORDS {
        return Err(ApiError::BadRequest("too many records".into()).into());
    }
    for (i, b) in req.records.iter().enumerate() {
        if b.is_empty() || b.len() > MAX_RECORD_HEX || !b.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(ApiError::BadRequest(format!("record {i} is not a sealed blob")).into());
        }
    }
    let limits = *lock(&state.relay.limits);
    let mut conn = lock(&state.registry);
    let adding: u64 = req.records.iter().map(|b| b.len() as u64).sum();
    check_room(&conn, &limits, id, adding)?;
    let before: u64 = conn.query_row(
        "SELECT COALESCE(MAX(rev), 0) FROM relay_records WHERE company = ?1",
        [id.to_string()],
        |r| r.get::<_, i64>(0),
    )? as u64;
    let tx = conn.transaction()?;
    for (i, blob) in req.records.iter().enumerate() {
        tx.execute(
            "INSERT INTO relay_records (company, rev, blob) VALUES (?1, ?2, ?3)",
            params![id.to_string(), (before + 1 + i as u64) as i64, blob],
        )?;
    }
    tx.execute(
        "INSERT INTO relay_usage (company, bytes) VALUES (?1, ?2)
         ON CONFLICT(company) DO UPDATE SET bytes = bytes + excluded.bytes",
        params![id.to_string(), adding as i64],
    )?;
    tx.commit()?;
    drop(conn);
    let rev = before + req.records.len() as u64;
    if rev > before {
        crate::records::log_push(&state, id, caller, before, rev)?;
    }
    Ok(Json(RelayRecordsPushed { rev, protocol: crate::protocol::PROTOCOL_VERSION }))
}

pub(crate) async fn pull_records(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<crate::records::SinceQuery>,
    headers: HeaderMap,
) -> Result<Json<RelayPulledRecords>, ApiError> {
    auth::authorize(&state, &headers, id)?;
    if !is_relay(&state, id)? {
        return Err(ApiError::NotFound);
    }
    let conn = lock(&state.registry);
    let mut stmt = conn.prepare("SELECT rev, blob FROM relay_records WHERE company = ?1 AND rev > ?2 ORDER BY rev")?;
    let mut rows = stmt.query(params![id.to_string(), q.since as i64])?;
    let (mut records, mut bytes, mut more) = (Vec::new(), 0, false);
    while let Some(row) = rows.next()? {
        let blob: String = row.get(1)?;
        if records.len() >= crate::records::PULL_RECORDS || (!records.is_empty() && bytes + blob.len() > 2 * crate::records::PULL_RECORD_BYTES) {
            more = true;
            break;
        }
        bytes += blob.len();
        records.push(SealedRecord { rev: row.get::<_, i64>(0)? as u64, blob });
    }
    Ok(Json(RelayPulledRecords { records, more, protocol: crate::protocol::PROTOCOL_VERSION }))
}

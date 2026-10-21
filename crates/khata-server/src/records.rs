//! Shared records (`/v1/companies/{id}/records`): the company's
//! non-accounting data, beside the audit trail.
//!
//! The server merges each pushed version into the company file's `records`
//! table with the same rule as every device ([`khata_store::Record::wins_over`]),
//! so all copies converge whatever order pushes arrive in. Records are not
//! books: nothing here touches the trail or its hashes.
//!
//! Owners and accountants push, viewers only pull. Keys that belong to one
//! computer (sync and relay credentials, e-invoicing device chains) are
//! refused, whatever a client sends ([`khata_store::records::check_key`]).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::Utc;
use rusqlite::params;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{self, Caller};
use crate::protocol::{PROTOCOL_VERSION, PulledRecords, PushRecords, RecordsPushed, RevRecord};
use crate::{ApiError, AppState, SharedState, lock};

/// Most records in one push.
pub const MAX_PUSH_RECORDS: usize = 1_000;
/// Most records in one pull page…
pub const PULL_RECORDS: usize = 500;
/// …and the values' bytes a page stops at (one record is always sent).
pub const PULL_RECORD_BYTES: usize = 4 * 1024 * 1024;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS record_log (
    company     TEXT NOT NULL,
    token_id    TEXT NOT NULL,
    from_rev    INTEGER NOT NULL,
    to_rev      INTEGER NOT NULL,
    received_at INTEGER NOT NULL
);
";

pub(crate) fn init(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)
}

/// Record which token moved the company's records from `from_rev` to `to_rev`.
pub(crate) fn log_push(state: &AppState, company: Uuid, caller: Caller, from_rev: u64, to_rev: u64) -> Result<(), ApiError> {
    lock(&state.registry).execute(
        "INSERT INTO record_log (company, token_id, from_rev, to_rev, received_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![company.to_string(), caller.token_id.to_string(), from_rev as i64, to_rev as i64, Utc::now().timestamp()],
    )?;
    Ok(())
}

#[derive(Deserialize)]
pub(crate) struct SinceQuery {
    #[serde(default)]
    pub(crate) since: u64,
}

pub(crate) async fn push(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<PushRecords>,
) -> Result<Json<RecordsPushed>, ApiError> {
    let caller = auth::authorize(&state, &headers, id)?;
    if !caller.role.can_write() {
        return Err(ApiError::Forbidden);
    }
    if req.records.len() > MAX_PUSH_RECORDS {
        return Err(ApiError::BadRequest(format!("at most {MAX_PUSH_RECORDS} records per push")));
    }
    for r in &req.records {
        r.check().map_err(ApiError::Rejected)?;
    }
    state.with_company(id, |c| {
        let before = c.store.record_rev()?;
        let changed = c.store.merge_records(&req.records)?.len() as u64;
        let rev = c.store.record_rev()?;
        if rev > before {
            log_push(&state, id, caller, before, rev)?;
        }
        Ok(Json(RecordsPushed { rev, changed, protocol: PROTOCOL_VERSION }))
    })
}

pub(crate) async fn pull(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<SinceQuery>,
    headers: HeaderMap,
) -> Result<Json<PulledRecords>, ApiError> {
    auth::authorize(&state, &headers, id)?;
    state.with_company(id, |c| {
        let (page, more) = c.store.records_since(q.since, PULL_RECORDS, PULL_RECORD_BYTES)?;
        Ok(Json(PulledRecords {
            records: page.into_iter().map(|(rev, record)| RevRecord { rev, record }).collect(),
            rev: c.store.record_rev()?,
            more,
            protocol: PROTOCOL_VERSION,
        }))
    })
}

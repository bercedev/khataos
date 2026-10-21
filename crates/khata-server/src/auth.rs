//! Bearer tokens scoped to a company and a role.

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ApiError, AppState, lock};

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tokens (
    id         TEXT PRIMARY KEY,
    hash       TEXT NOT NULL UNIQUE,
    company    TEXT NOT NULL,
    role       TEXT NOT NULL,
    label      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked    INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS push_log (
    company     TEXT NOT NULL,
    token_id    TEXT NOT NULL,
    from_seq    INTEGER NOT NULL,
    to_seq      INTEGER NOT NULL,
    received_at INTEGER NOT NULL
);
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Accountant,
    Viewer,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Accountant => "accountant",
            Role::Viewer => "viewer",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Role::Owner),
            "accountant" => Some(Role::Accountant),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    pub fn can_write(self) -> bool {
        matches!(self, Role::Owner | Role::Accountant)
    }
}

pub(crate) fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ")
}

pub(crate) fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    match bearer(headers) {
        Some(t) if hash_token(t) == state.admin_key_hash => Ok(()),
        _ => Err(ApiError::Unauthorized),
    }
}

/// A verified caller: the token's id and the role it grants.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Caller {
    pub(crate) token_id: Uuid,
    pub(crate) role: Role,
}

/// The caller a request's token identifies on `company`.
pub(crate) fn authorize(state: &AppState, headers: &HeaderMap, company: Uuid) -> Result<Caller, ApiError> {
    let token = bearer(headers).ok_or(ApiError::Unauthorized)?;
    let conn = lock(&state.registry);
    let row: Option<(String, String, Option<i64>, bool)> = conn
        .query_row(
            "SELECT id, role, expires_at, revoked FROM tokens WHERE hash = ?1 AND company = ?2",
            params![hash_token(token), company.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let (id, role, expires_at, revoked) = row.ok_or(ApiError::Unauthorized)?;
    if revoked || expires_at.is_some_and(|t| t <= Utc::now().timestamp()) {
        return Err(ApiError::Unauthorized);
    }
    Ok(Caller {
        token_id: id.parse().map_err(|_| ApiError::Unauthorized)?,
        role: Role::parse(&role).ok_or(ApiError::Unauthorized)?,
    })
}

/// Record which token pushed entries `from_seq..to_seq`.
pub(crate) fn log_push(state: &AppState, company: Uuid, caller: Caller, from_seq: u64, to_seq: u64) -> Result<(), ApiError> {
    lock(&state.registry).execute(
        "INSERT INTO push_log (company, token_id, from_seq, to_seq, received_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![company.to_string(), caller.token_id.to_string(), from_seq as i64, to_seq as i64, Utc::now().timestamp()],
    )?;
    Ok(())
}

/// One accepted push: entries `from_seq..to_seq`, the label of the token
/// that pushed them (empty if the token row is gone) and the server time.
pub(crate) struct PushRecord {
    pub(crate) from_seq: u64,
    pub(crate) to_seq: u64,
    pub(crate) label: String,
    pub(crate) received_at: i64,
}

/// Every push recorded for `company`, oldest first.
pub(crate) fn push_log(state: &AppState, company: Uuid) -> Result<Vec<PushRecord>, ApiError> {
    let conn = lock(&state.registry);
    let mut stmt = conn.prepare(
        "SELECT p.from_seq, p.to_seq, COALESCE(t.label, ''), p.received_at FROM push_log p
         LEFT JOIN tokens t ON t.id = p.token_id WHERE p.company = ?1 ORDER BY p.from_seq",
    )?;
    let rows = stmt
        .query_map([company.to_string()], |r| {
            Ok(PushRecord {
                from_seq: r.get::<_, i64>(0)?.max(0) as u64,
                to_seq: r.get::<_, i64>(1)?.max(0) as u64,
                label: r.get(2)?,
                received_at: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Issue a new token; returns `(share_id, token)`. The token is shown once.
pub(crate) fn issue(
    state: &AppState,
    company: Uuid,
    role: Role,
    label: &str,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(Uuid, String), ApiError> {
    let id = Uuid::now_v7();
    let token = format!("khk_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    lock(&state.registry).execute(
        "INSERT INTO tokens (id, hash, company, role, label, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.to_string(),
            hash_token(&token),
            company.to_string(),
            role.as_str(),
            label,
            Utc::now().timestamp(),
            expires_at.map(|t| t.timestamp()),
        ],
    )?;
    Ok((id, token))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShareInfo {
    pub id: Uuid,
    pub role: Role,
    pub label: String,
    pub expires_at: Option<i64>,
    pub revoked: bool,
}

pub(crate) fn list(state: &AppState, company: Uuid) -> Result<Vec<ShareInfo>, ApiError> {
    let conn = lock(&state.registry);
    let mut stmt = conn.prepare(
        "SELECT id, role, label, expires_at, revoked FROM tokens WHERE company = ?1 ORDER BY created_at, id",
    )?;
    let rows = stmt
        .query_map([company.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .filter_map(Result::ok)
        .filter_map(|(id, role, label, expires_at, revoked)| {
            Some(ShareInfo { id: id.parse().ok()?, role: Role::parse(&role)?, label, expires_at, revoked })
        })
        .collect();
    Ok(rows)
}

pub(crate) fn revoke(state: &AppState, company: Uuid, share: Uuid) -> Result<(), ApiError> {
    let n = lock(&state.registry).execute(
        "UPDATE tokens SET revoked = 1 WHERE id = ?1 AND company = ?2",
        params![share.to_string(), company.to_string()],
    )?;
    if n == 0 { Err(ApiError::NotFound) } else { Ok(()) }
}

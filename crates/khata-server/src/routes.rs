//! HTTP routes (`/v1/...`).

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use khata_store::Store;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{self, Role, ShareInfo};
use crate::protocol::*;
use crate::views;
use crate::{ApiError, SharedState};

/// Largest request body accepted (a push of a few thousand entries).
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
/// Most entries accepted in one push.
pub const MAX_PUSH_ENTRIES: usize = 10_000;
/// Most entries returned by one pull; clients page with `from`.
pub const PULL_PAGE: usize = 5_000;
const MAX_NAME_LEN: usize = 200;
const MAX_SHARE_DAYS: u32 = 3_650;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/v1/health", get(|| async { "ok" }))
        .route("/v1/companies", post(create_company))
        .route("/v1/companies/{id}/push", post(push))
        .route("/v1/relay", post(crate::relay::create))
        .route("/v1/relay/{id}/push", post(crate::relay::push))
        .route("/v1/relay/{id}/pull", get(crate::relay::pull))
        .route("/v1/companies/{id}/pull", get(pull))
        .route("/v1/companies/{id}/records", get(crate::records::pull).post(crate::records::push))
        .route("/v1/relay/{id}/records", get(crate::relay::pull_records).post(crate::relay::push_records))
        .route("/v1/companies/{id}/shares", post(create_share).get(list_shares))
        .route("/v1/companies/{id}/shares/{share}", delete(revoke_share))
        .route("/v1/companies/{id}/reports/trial-balance", get(views::trial_balance))
        .route("/v1/companies/{id}/reports/day-book", get(views::day_book))
        .route("/v1/companies/{id}/reports/profit-and-loss", get(views::profit_and_loss))
        .route("/v1/companies/{id}/reports/balance-sheet", get(views::balance_sheet))
        .route("/v1/companies/{id}/reports/outstanding", get(views::outstanding))
        .route("/v1/companies/{id}/reports/ageing", get(views::ageing))
        .route("/v1/companies/{id}/reports/msme-overdue", get(views::msme_overdue))
        .route("/v1/companies/{id}/reports/audit-trail", get(views::audit_trail))
        .route("/v1/companies/{id}/reports/stock-summary", get(views::stock_summary))
        .route("/v1/companies/{id}/reports/ratios", get(views::ratios))
        .route("/v1/companies/{id}/ledgers", get(views::ledgers))
        .route("/v1/companies/{id}/ledgers/{ledger}/statement", get(views::ledger_statement))
        .route("/v1/companies/{id}/groups/{group}/summary", get(views::group))
        .route("/portal", get(crate::portal::page))
        .route("/source", get(source))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// AGPL-3.0 section 13: offer network users the corresponding source.
/// Operators running a modified server set `KHATA_SOURCE_URL` to theirs.
async fn source() -> axum::response::Redirect {
    let url = std::env::var("KHATA_SOURCE_URL").unwrap_or_else(|_| env!("CARGO_PKG_REPOSITORY").to_owned());
    axum::response::Redirect::to(&url)
}

fn check_name(what: &str, s: &str) -> Result<(), ApiError> {
    if s.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("{what} is empty")));
    }
    if s.chars().count() > MAX_NAME_LEN {
        return Err(ApiError::BadRequest(format!("{what} is longer than {MAX_NAME_LEN} characters")));
    }
    Ok(())
}

async fn create_company(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateCompany>,
) -> Result<Json<CompanyCreated>, ApiError> {
    auth::require_admin(&state, &headers)?;
    check_name("company name", &req.name)?;
    let company_id = Uuid::now_v7();
    Store::create(state.company_path(company_id), req.name.trim())?;
    let (_, owner_token) = auth::issue(&state, company_id, Role::Owner, "owner", None)?;
    Ok(Json(CompanyCreated { company_id, owner_token }))
}

async fn push(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<Push>,
) -> Result<Json<Pushed>, ApiError> {
    let caller = auth::authorize(&state, &headers, id)?;
    if !caller.role.can_write() {
        return Err(ApiError::Forbidden);
    }
    if req.entries.len() > MAX_PUSH_ENTRIES {
        return Err(ApiError::BadRequest(format!("at most {MAX_PUSH_ENTRIES} entries per push")));
    }
    if req.entries.iter().any(|e| e.user.chars().count() > MAX_NAME_LEN) {
        return Err(ApiError::BadRequest("user name too long".into()));
    }
    state.with_company(id, |c| {
        let len = c.book.audit().entries().len() as u64;
        if req.base_len != len {
            return Err(ApiError::Conflict { server_len: len });
        }
        // Validate on a copy with the same rules as local edits; the cached
        // books change only once everything is stored.
        let mut next = c.book.clone();
        for e in &req.entries {
            next.apply_entry(e).map_err(|err| ApiError::Rejected(format!("entry {}: {err}", e.seq)))?;
        }
        c.store.append(&req.entries)?;
        c.book = next;
        let new_len = c.book.audit().entries().len() as u64;
        if new_len > len {
            auth::log_push(&state, id, caller, len, new_len)?;
        }
        Ok(Json(Pushed { len: new_len }))
    })
}

#[derive(Deserialize)]
struct PullQuery {
    #[serde(default)]
    from: u64,
}

async fn pull(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<PullQuery>,
    headers: HeaderMap,
) -> Result<Json<Pulled>, ApiError> {
    auth::authorize(&state, &headers, id)?;
    state.with_company(id, |c| {
        let all = c.book.audit().entries();
        let start = (q.from as usize).min(all.len());
        let end = (start + PULL_PAGE).min(all.len());
        Ok(Json(Pulled {
            company: c.book.name.clone(),
            entries: all[start..end].to_vec(),
            more: end < all.len(),
            protocol: PROTOCOL_VERSION,
        }))
    })
}

async fn create_share(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<CreateShare>,
) -> Result<Json<ShareCreated>, ApiError> {
    if auth::authorize(&state, &headers, id)?.role != Role::Owner {
        return Err(ApiError::Forbidden);
    }
    if req.role == Role::Owner {
        return Err(ApiError::BadRequest("owner access cannot be shared".into()));
    }
    check_name("label", &req.label)?;
    let expires = match req.expires_in_days {
        Some(d) if d > MAX_SHARE_DAYS => {
            return Err(ApiError::BadRequest(format!("expiry is limited to {MAX_SHARE_DAYS} days")));
        }
        Some(d) => Some(Utc::now() + Duration::days(d.into())),
        None => None,
    };
    let (id, token) = auth::issue(&state, id, req.role, req.label.trim(), expires)?;
    Ok(Json(ShareCreated { id, token }))
}

async fn list_shares(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Vec<ShareInfo>>, ApiError> {
    if auth::authorize(&state, &headers, id)?.role != Role::Owner {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(auth::list(&state, id)?))
}

async fn revoke_share(
    State(state): State<SharedState>,
    Path((id, share)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<(), ApiError> {
    let caller = auth::authorize(&state, &headers, id)?;
    if caller.role != Role::Owner {
        return Err(ApiError::Forbidden);
    }
    // Revoking the access you are using would lock the owner out.
    if caller.token_id == share {
        return Err(ApiError::BadRequest("you cannot revoke your own access".into()));
    }
    auth::revoke(&state, id, share)
}

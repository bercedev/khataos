use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("missing or invalid token")]
    Unauthorized,
    #[error("this token's role does not allow that")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("trail moved on: server has {server_len} entries")]
    Conflict { server_len: u64 },
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    /// Details are logged on the server, never sent to clients.
    #[error("internal error")]
    Internal,
}

impl ApiError {
    pub(crate) fn internal(detail: impl std::fmt::Display) -> Self {
        eprintln!("khata-server internal error: {detail}");
        Self::Internal
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(e)
    }
}

impl From<khata_store::StoreError> for ApiError {
    fn from(e: khata_store::StoreError) -> Self {
        match e {
            khata_store::StoreError::Book(b) => Self::Rejected(b.to_string()),
            khata_store::StoreError::Diverged(seq) => Self::Rejected(format!("entry {seq} does not continue the trail")),
            khata_store::StoreError::Record(m) => Self::Rejected(m),
            other => Self::internal(other),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let mut body = json!({ "error": self.to_string() });
        if let Self::Conflict { server_len } = self {
            body["server_len"] = json!(server_len);
        }
        (status, Json(body)).into_response()
    }
}

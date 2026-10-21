//! Blocking client for Tally's XML server (feature `http`).
//!
//! Enable it in TallyPrime under F1 Help → Settings → Connectivity (Client/
//! Server configuration: "Server" or "Both", port 9000), with the company
//! loaded.

use thiserror::Error;

use crate::request::ExportRequest;

/// Where Tally listens by default.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:9000";

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("could not reach Tally: {0}")]
    Http(#[from] ureq::Error),
}

/// POST `request` to Tally and return the raw response bytes, ready for
/// [`crate::import`]. The request is sent as UTF-16 LE, which Tally
/// answers in kind, so non-ASCII names (₹, regional scripts) survive.
pub fn fetch(endpoint: &str, request: &ExportRequest) -> Result<Vec<u8>, FetchError> {
    let body: Vec<u8> = request.to_xml().encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut response = ureq::post(endpoint).header("Content-Type", "text/xml;charset=utf-16").send(&body[..])?;
    // Day Book exports of a busy company easily exceed ureq's default cap.
    Ok(response.body_mut().with_config().limit(u64::MAX).read_to_vec()?)
}

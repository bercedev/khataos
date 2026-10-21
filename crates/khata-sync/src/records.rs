//! Syncing the company's shared records (notes, the invoice design, tax
//! settings…) beside the audit trail. See `docs/sync-settings.md`.
//!
//! One round: push what changed here since the last push, then pull what
//! changed on the remote since the last pull, merging each version last
//! writer wins ([`khata_store::Record::wins_over`]). Through a relay every
//! version travels sealed with the company key, keys and values alike.
//!
//! - **Older server** (no `records` route): nothing is lost; the records
//!   stay pending here and the books sync as before
//!   ([`RecordsOutcome::supported`] is `false`).
//! - **Viewer** access: the server refuses pushes (403); pulling goes on.
//! - Records that fail [`khata_store::Record::check`] (a device-local key,
//!   say) are never sent, and are ignored when received, as are sealed
//!   blobs that don't open with the company key (made up by a relay or a
//!   token holder: they can't carry data, and must not stop the channel).
//!   A wrong key still fails the sync: the books can't be opened either.

use khata_seal::CompanyKey;
use khata_store::{Record, RecordCursor, Store};
use serde::Deserialize;

use crate::{Remote, SyncError, agent};

/// What a sync did with the shared records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordsOutcome {
    /// Versions sent.
    pub sent: u64,
    /// Versions received that changed something here.
    pub received: u64,
    /// The remote takes shared records (protocol 2). `false`: an older
    /// server; only the books synced.
    pub supported: bool,
    /// The remote refused this device's changes (viewer access).
    pub read_only: bool,
    /// Versions received that were ignored: not valid records, or sealed
    /// blobs that don't open with the company key.
    pub ignored: u64,
}

/// Bytes of record values per push request, under the server's 8 MiB body
/// limit (one record larger than this goes alone).
const PUSH_BATCH_BYTES: usize = 4 * 1024 * 1024;
/// Records per push request (the server takes up to 1 000).
const PUSH_BATCH_RECORDS: usize = 500;

enum Reply<T> {
    Ok(T),
    /// No such route: a server from before shared records.
    Unsupported,
    Forbidden,
}

fn base(remote: &Remote, key: Option<&CompanyKey>) -> String {
    let url = remote.url.trim_end_matches('/');
    match key {
        Some(_) => format!("{url}/v1/relay/{}/records", remote.company_id),
        None => format!("{url}/v1/companies/{}/records", remote.company_id),
    }
}

fn reply<T: for<'de> Deserialize<'de>>(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Reply<T>, SyncError> {
    let mut resp = resp.map_err(|e| SyncError::Network(e.to_string()))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return resp.body_mut().read_json().map(Reply::Ok).map_err(|e| SyncError::Network(e.to_string()));
    }
    let message = resp.body_mut().read_json::<crate::ErrorBody>().ok().map(|b| b.error);
    match (status, message) {
        // An unknown route answers 404 without a KhataOS error body.
        (404, None) | (405, _) => Ok(Reply::Unsupported),
        (403, _) => Ok(Reply::Forbidden),
        (status, message) => Err(SyncError::Server { status, message: message.unwrap_or_default() }),
    }
}

fn seal(key: &CompanyKey, company_id: &str, r: &Record) -> Result<String, SyncError> {
    let plain = serde_json::to_vec(r).expect("record serializes");
    khata_seal::seal(key, &khata_seal::record_context(company_id), &plain)
        .map(hex::encode)
        .map_err(|e| SyncError::Network(e.to_string()))
}

/// The record in a sealed blob; `None` when it isn't hex, doesn't open
/// with the company key, or isn't a record.
fn open(key: &CompanyKey, company_id: &str, blob: &str) -> Option<Record> {
    let sealed = hex::decode(blob).ok()?;
    let plain = khata_seal::open(key, &khata_seal::record_context(company_id), &sealed).ok()?;
    serde_json::from_slice(&plain).ok()
}

/// Push one batch of versions.
fn push_batch(remote: &Remote, key: Option<&CompanyKey>, batch: &[Record]) -> Result<Reply<()>, SyncError> {
    let body = match key {
        Some(k) => serde_json::json!({ "records": batch.iter().map(|r| seal(k, &remote.company_id, r)).collect::<Result<Vec<_>, _>>()? }),
        None => serde_json::json!({ "records": batch }),
    };
    let resp = agent().post(&base(remote, key)).header("authorization", &format!("Bearer {}", remote.token)).send_json(body);
    Ok(match reply::<serde_json::Value>(resp)? {
        Reply::Ok(_) => Reply::Ok(()),
        Reply::Unsupported => Reply::Unsupported,
        Reply::Forbidden => Reply::Forbidden,
    })
}

#[derive(Deserialize)]
struct PlainPage {
    records: Vec<PlainRecord>,
    #[serde(default)]
    more: bool,
}

#[derive(Deserialize)]
struct PlainRecord {
    rev: u64,
    #[serde(flatten)]
    record: Record,
}

#[derive(Deserialize)]
struct SealedPage {
    records: Vec<SealedRecord>,
    #[serde(default)]
    more: bool,
}

#[derive(Deserialize)]
struct SealedRecord {
    rev: u64,
    blob: String,
}

/// One page after `since`: `(rev, record)` pairs (`None` for a blob that
/// doesn't open) and whether more follow.
#[allow(clippy::type_complexity)]
fn pull_page(remote: &Remote, key: Option<&CompanyKey>, since: u64) -> Result<Reply<(Vec<(u64, Option<Record>)>, bool)>, SyncError> {
    let resp = agent()
        .get(&format!("{}?since={since}", base(remote, key)))
        .header("authorization", &format!("Bearer {}", remote.token))
        .call();
    Ok(match key {
        Some(k) => match reply::<SealedPage>(resp)? {
            Reply::Ok(p) => Reply::Ok((p.records.iter().map(|s| (s.rev, open(k, &remote.company_id, &s.blob))).collect(), p.more)),
            Reply::Unsupported => Reply::Unsupported,
            Reply::Forbidden => Reply::Forbidden,
        },
        None => match reply::<PlainPage>(resp)? {
            Reply::Ok(p) => Reply::Ok((p.records.into_iter().map(|r| (r.rev, Some(r.record))).collect(), p.more)),
            Reply::Unsupported => Reply::Unsupported,
            Reply::Forbidden => Reply::Forbidden,
        },
    })
}

/// Sync the shared records of `store` with `remote` (sealed with `key`
/// through a relay). The cursor is saved as it goes, so a failure part-way
/// loses nothing.
pub fn sync_records(store: &mut Store, remote: &Remote, key: Option<&CompanyKey>) -> Result<RecordsOutcome, SyncError> {
    crate::check_remote(remote)?;
    if let Some((linked, _)) = store.sync_state(&remote.url)?
        && linked != remote.company_id
    {
        return Err(SyncError::WrongCompany { linked, requested: remote.company_id.clone() });
    }
    let mut cursor = store.record_cursor(&remote.url)?;
    if cursor.company_id != remote.company_id {
        cursor = RecordCursor { company_id: remote.company_id.clone(), ..RecordCursor::default() };
    }
    let mut out = RecordsOutcome { supported: true, ..RecordsOutcome::default() };
    let unsupported = |store: &Store, mut cursor: RecordCursor| -> Result<RecordsOutcome, SyncError> {
        cursor.supported = Some(false);
        store.set_record_cursor(&remote.url, &cursor)?;
        Ok(RecordsOutcome::default())
    };

    // Push what changed here, in batches.
    let (pending, _) = store.records_since(cursor.pushed, usize::MAX, usize::MAX)?;
    let mut rest: &[(u64, Record)] = &pending;
    while !rest.is_empty() {
        let mut n = 0;
        let mut bytes = 0;
        for (_, r) in rest {
            let size = r.value.as_ref().map_or(0, String::len) + r.key.len() + 128;
            if n > 0 && (n == PUSH_BATCH_RECORDS || bytes + size > PUSH_BATCH_BYTES) {
                break;
            }
            bytes += size;
            n += 1;
        }
        let (batch, later) = rest.split_at(n);
        // `check` never fails for what the store wrote; it is the last guard
        // that nothing device-local is ever sent.
        let send: Vec<Record> = batch.iter().map(|(_, r)| r.clone()).filter(|r| r.check().is_ok()).collect();
        let sent = if send.is_empty() { Reply::Ok(()) } else { push_batch(remote, key, &send)? };
        match sent {
            Reply::Ok(()) => {
                out.sent += send.len() as u64;
                cursor.pushed = batch.last().map_or(cursor.pushed, |(rev, _)| *rev);
                store.set_record_cursor(&remote.url, &cursor)?;
            }
            Reply::Unsupported => return unsupported(store, cursor),
            Reply::Forbidden => {
                out.read_only = true;
                break;
            }
        }
        rest = later;
    }

    // Pull what changed there.
    loop {
        let (page, more) = match pull_page(remote, key, cursor.pulled)? {
            Reply::Ok(p) => p,
            Reply::Unsupported => return unsupported(store, cursor),
            Reply::Forbidden => return Err(SyncError::Server { status: 403, message: String::new() }),
        };
        let Some(last) = page.last().map(|(rev, _)| *rev) else { break };
        let n = page.len() as u64;
        let valid: Vec<Record> = page.into_iter().filter_map(|(_, r)| r).filter(|r| r.check().is_ok()).collect();
        out.ignored += n - valid.len() as u64;
        out.received += store.merge_records(&valid)?.len() as u64;
        cursor.pulled = last.max(cursor.pulled);
        store.set_record_cursor(&remote.url, &cursor)?;
        if !more {
            break;
        }
    }
    // Everything changed here since the push came from this remote.
    if !out.read_only {
        cursor.pushed = store.record_rev()?;
    }
    cursor.supported = Some(true);
    store.set_record_cursor(&remote.url, &cursor)?;
    Ok(out)
}

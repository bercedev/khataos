//! Sync a local company with a `khata-server`.
//!
//! The store remembers how much of the trail is shared with each remote
//! (`synced_len`). A sync is a fast-forward in one direction:
//! - only this device has new entries → push them;
//! - only the server has new entries → pull and append them;
//! - both moved on → [`SyncError::Conflict`]. History is never rewritten;
//!   resolving concurrent offline edits is a separate step (see
//!   `docs/architecture.md`, "Open design items").
//!
//! Every sync also exchanges the company's **shared records** (notes, the
//! invoice design, tax settings…) through a second channel that merges
//! last writer wins and never conflicts ([`records`],
//! `docs/sync-settings.md`). They go first, so they reach colleagues even
//! while the books are in conflict.

mod records;

use std::path::{Path, PathBuf};

use khata_core::{AuditAction, AuditEntry, Book, BookError, Voucher, VoucherKind};
use khata_store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use records::{RecordsOutcome, sync_records};

/// Where and as whom to sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remote {
    /// Server base URL, e.g. `https://books.example.in`.
    pub url: String,
    pub company_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    UpToDate,
    Pushed(u64),
    Pulled(u64),
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("network error: {0}")]
    Network(String),
    #[error("server said {status}: {message}")]
    Server { status: u16, message: String },
    #[error("this file is linked to company {linked}, not {requested}")]
    WrongCompany { linked: String, requested: String },
    #[error("both this device ({local} new) and the server ({remote} new) have changes")]
    Conflict { local: u64, remote: u64 },
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Book(#[from] BookError),
    #[error("refusing to sync: {0}")]
    InsecureRemote(String),
    #[error("this file has fewer entries than were synced; was an older backup restored?")]
    BehindSyncState,
    #[error("the server sent entries that are not valid books: {0}")]
    InvalidFromServer(String),
    #[error("cannot replace the company file: {0}")]
    File(String),
    #[error("sync first: this device has {local} change(s) not on the relay, and the relay has {remote} not on this device")]
    NotSynced { local: u64, remote: u64 },
    #[error("the company already syncs with {0}; choose a different relay")]
    SameRelay(String),
}

#[derive(Deserialize)]
struct Pulled {
    entries: Vec<AuditEntry>,
    #[serde(default)]
    more: bool,
}

/// Whether `host` is this machine or a private office-network address
/// (RFC 1918 IPv4, a 100.64/10 overlay such as Tailscale, IPv6 unique-local /
/// link-local, or a `.local` mDNS name).
/// Plain http is accepted only there — the Tally-style "one PC acts as
/// server" LAN setup — never across the internet.
fn is_local_network(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".local") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        // 100.64.0.0/10 is carrier-grade NAT space, used by Tailscale and
        // similar WireGuard overlays that already encrypt the link.
        Ok(std::net::IpAddr::V4(ip)) => {
            let [a, b, ..] = ip.octets();
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || (a == 100 && (64..128).contains(&b))
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

/// The same check [`sync`] applies before any request: `https://`, or plain
/// `http://` only on this computer or the office network, and a UUID company
/// id. For callers that build a remote before syncing (invite codes).
pub fn validate_remote(remote: &Remote) -> Result<(), SyncError> {
    check_remote(remote)
}

/// Reject remotes that would leak the token or inject path segments.
fn check_remote(remote: &Remote) -> Result<(), SyncError> {
    let url = remote.url.trim_end_matches('/');
    let plain_local = url.strip_prefix("http://").is_some_and(|rest| {
        let authority = rest.split('/').next().unwrap_or("");
        let host = if authority.starts_with('[') {
            authority.split(']').next().map(|h| format!("{h}]")).unwrap_or_default()
        } else {
            authority.rsplit_once(':').map_or(authority, |(h, _)| h).to_owned()
        };
        !authority.contains('@') && is_local_network(&host)
    });
    if !url.starts_with("https://") && !plain_local {
        return Err(SyncError::InsecureRemote(
            "the server URL must use https:// (http only on this computer or the office network)".into(),
        ));
    }
    let id = &remote.company_id;
    let is_uuid = id.len() == 36
        && id.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() });
    if !is_uuid {
        return Err(SyncError::InsecureRemote("company id is not a UUID".into()));
    }
    Ok(())
}

#[derive(Serialize)]
struct Push<'a> {
    base_len: u64,
    entries: &'a [AuditEntry],
}

#[derive(Deserialize)]
struct Pushed {
    len: u64,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

fn agent() -> ureq::Agent {
    // A server that is switched off must fail fast, not hang the app.
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        // …and a server that accepts but stalls must not hold the books.
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into()
}

fn read<T: for<'de> Deserialize<'de>>(mut resp: ureq::http::Response<ureq::Body>) -> Result<T, SyncError> {
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return resp.body_mut().read_json().map_err(|e| SyncError::Network(e.to_string()));
    }
    let message = resp.body_mut().read_json::<ErrorBody>().map(|b| b.error).unwrap_or_default();
    Err(SyncError::Server { status, message })
}

/// All entries from `from` on, following the server's pages.
fn pull(remote: &Remote, from: u64) -> Result<Vec<AuditEntry>, SyncError> {
    let mut out = Vec::new();
    loop {
        let at = from + out.len() as u64;
        let url = format!("{}/v1/companies/{}/pull?from={at}", remote.url.trim_end_matches('/'), remote.company_id);
        let resp = agent()
            .get(&url)
            .header("authorization", &format!("Bearer {}", remote.token))
            .call()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        let page = read::<Pulled>(resp)?;
        let done = !page.more || page.entries.is_empty();
        out.extend(page.entries);
        if done {
            return Ok(out);
        }
    }
}

fn push(remote: &Remote, base_len: u64, entries: &[AuditEntry]) -> Result<u64, SyncError> {
    let url = format!("{}/v1/companies/{}/push", remote.url.trim_end_matches('/'), remote.company_id);
    let resp = agent()
        .post(&url)
        .header("authorization", &format!("Bearer {}", remote.token))
        .send_json(Push { base_len, entries })
        .map_err(|e| SyncError::Network(e.to_string()))?;
    Ok(read::<Pushed>(resp)?.len)
}

/// What a rebase did with this device's offline changes.
pub struct Rebased {
    pub store: Store,
    pub book: Book,
    /// The previous company file, kept unchanged for the audit trail.
    pub archived: PathBuf,
    /// Vouchers whose number was taken on the server: `(kind, old, new)`.
    pub renumbered: Vec<(VoucherKind, String, String)>,
    /// Changes that could not be applied on top of the server's books
    /// (e.g. altering a voucher someone else deleted), with the reason.
    /// They remain in the archived file.
    pub needs_attention: Vec<(u64, String)>,
    pub pushed: u64,
}

/// Resolve a [`SyncError::Conflict`]: rebuild the company from the server's
/// trail, re-apply this device's unsynced changes on top as new validated
/// operations, renumber vouchers whose numbers were taken, and push.
///
/// The file at `path` is never modified: it is renamed to
/// `<name>.conflict-<unix-time>.khata` and a new file takes its place.
pub fn rebase(path: &Path, remote: &Remote) -> Result<Rebased, SyncError> {
    rebase_with(path, remote, None)
}

/// [`rebase`] for a company that syncs through an end-to-end encrypted relay.
pub fn rebase_sealed(path: &Path, remote: &Remote, key: &khata_seal::CompanyKey) -> Result<Rebased, SyncError> {
    rebase_with(path, remote, Some(key))
}

fn rebase_with(path: &Path, remote: &Remote, key: Option<&khata_seal::CompanyKey>) -> Result<Rebased, SyncError> {
    check_remote(remote)?;
    let store = Store::open(path)?;
    let synced = match store.sync_state(&remote.url)? {
        Some((linked, _)) if linked != remote.company_id => {
            return Err(SyncError::WrongCompany { linked, requested: remote.company_id.clone() });
        }
        Some((_, len)) => len,
        None => 0,
    };
    let pending = store.entries_from(synced)?;
    let name = store.company_name()?;

    let server_entries = match key {
        Some(k) => relay_pull(remote, k, 0)?,
        None => pull(remote, 0)?,
    };
    let server_len = server_entries.len() as u64;
    let mut book = Book::replay(name.clone(), khata_core::AuditLog::from(server_entries)).map_err(|e| SyncError::InvalidFromServer(e.to_string()))?;
    // The server's trail must start with what this file already synced.
    let local_synced = store.entries_from(0)?;
    let shared = &local_synced[..(synced as usize).min(local_synced.len())];
    if book.audit().entries().get(..shared.len()) != Some(shared) {
        return Err(SyncError::InvalidFromServer("history differs from what was synced before".into()));
    }

    let mut renumbered = Vec::new();
    let mut needs_attention = Vec::new();
    for e in &pending {
        match book.reapply(e) {
            Ok(()) => {}
            Err(BookError::DuplicateVoucherNumber(old)) if e.action == AuditAction::Create => {
                let mut v: Voucher = e.after.clone().and_then(|a| serde_json::from_value(a).ok()).ok_or(
                    BookError::UnreplayableEntry(e.seq),
                )?;
                let new = book.next_voucher_number(v.kind, v.date);
                v.number = new.clone();
                let mut renumbered_entry = e.clone();
                renumbered_entry.after = Some(serde_json::to_value(&v).expect("voucher serializes"));
                match book.reapply(&renumbered_entry) {
                    Ok(()) => renumbered.push((v.kind, old, new)),
                    Err(err) => needs_attention.push((e.seq, err.to_string())),
                }
            }
            Err(err) => needs_attention.push((e.seq, err.to_string())),
        }
    }

    let fresh = reserve(path, "rebasing")?;
    let mut new_store = Store::create(&fresh, &name)?;
    new_store.save(&book)?;
    new_store.set_sync_state(&remote.url, &remote.company_id, server_len)?;
    // Shared records aren't rebased: they come along as they are, and the
    // sync below sends any not yet on the server.
    new_store.merge_records(&store.all_records()?)?;
    drop(new_store);
    drop(store);

    let archived = reserve(path, "conflict")?;
    std::fs::rename(path, &archived).map_err(|e| SyncError::File(format!("cannot archive: {e}")))?;
    if let Err(e) = std::fs::rename(&fresh, path) {
        // Put the original back so the company is never left missing.
        let _ = std::fs::rename(&archived, path);
        return Err(SyncError::File(format!("cannot replace: {e}")));
    }

    let mut store = Store::open(path)?;
    let outcome = match key {
        Some(k) => sync_sealed(&mut store, &mut book, remote, k)?,
        None => sync(&mut store, &mut book, remote)?,
    };
    let pushed = match outcome {
        SyncOutcome::Pushed(n) => n,
        _ => 0,
    };
    Ok(Rebased { store, book, archived, renumbered, needs_attention, pushed })
}

/// Atomically reserve a new, unique sibling file name such as
/// `books.conflict-1760000000-1.khata` (never overwrites an existing file,
/// never follows a planted symlink).
fn reserve(path: &Path, tag: &str) -> Result<PathBuf, SyncError> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("company");
    for n in 0..1000 {
        let candidate = path.with_file_name(format!("{stem}.{tag}-{stamp}-{n}.khata"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(SyncError::File(e.to_string())),
        }
    }
    Err(SyncError::File("no free file name".into()))
}

/// Sync `book` (backed by `store`) with `remote`, the shared records
/// first. On a pull, `book` is replaced by the reloaded, verified books.
pub fn sync(store: &mut Store, book: &mut Book, remote: &Remote) -> Result<SyncOutcome, SyncError> {
    sync_all(store, book, remote, None).map(|(books, _)| books)
}

/// [`sync`] or (with the company `key`) [`sync_sealed`], also saying what
/// happened to the shared records. A books conflict is still
/// [`SyncError::Conflict`]; the records have synced by then.
pub fn sync_all(
    store: &mut Store,
    book: &mut Book,
    remote: &Remote,
    key: Option<&khata_seal::CompanyKey>,
) -> Result<(SyncOutcome, RecordsOutcome), SyncError> {
    check_remote(remote)?;
    store.save(book)?;
    // Unreachable or the wrong company: the books would fail the same way.
    // Any other trouble with the records must not hold up the books; it is
    // reported once they have synced.
    let records = match sync_records(store, remote, key) {
        Err(e @ (SyncError::Network(_) | SyncError::WrongCompany { .. } | SyncError::InsecureRemote(_))) => return Err(e),
        other => other,
    };
    let books = match key {
        Some(k) => sync_books_sealed(store, book, remote, k)?,
        None => sync_books(store, book, remote)?,
    };
    Ok((books, records?))
}

fn sync_books(store: &mut Store, book: &mut Book, remote: &Remote) -> Result<SyncOutcome, SyncError> {
    check_remote(remote)?;
    store.save(book)?;
    let synced = match store.sync_state(&remote.url)? {
        Some((linked, _)) if linked != remote.company_id => {
            return Err(SyncError::WrongCompany { linked, requested: remote.company_id.clone() });
        }
        Some((_, len)) => len,
        None => 0,
    };
    let local_len = book.audit().entries().len() as u64;
    let local_new = local_len.checked_sub(synced).ok_or(SyncError::BehindSyncState)?;
    let incoming = pull(remote, synced)?;

    match (local_new, incoming.len() as u64) {
        (0, 0) => Ok(SyncOutcome::UpToDate),
        (n, 0) => {
            let len = push(remote, synced, &book.audit().entries()[synced as usize..]).map_err(|e| match e {
                SyncError::Server { status: 409, .. } => SyncError::Conflict { local: n, remote: 1 },
                other => other,
            })?;
            store.set_sync_state(&remote.url, &remote.company_id, len)?;
            Ok(SyncOutcome::Pushed(n))
        }
        (0, m) => {
            // Validate before anything touches the append-only file.
            let mut next = book.clone();
            for e in &incoming {
                next.apply_entry(e).map_err(|err| SyncError::InvalidFromServer(format!("entry {}: {err}", e.seq)))?;
            }
            store.append(&incoming)?;
            *book = next;
            store.set_sync_state(&remote.url, &remote.company_id, synced + m)?;
            Ok(SyncOutcome::Pulled(m))
        }
        (n, m) => Err(SyncError::Conflict { local: n, remote: m }),
    }
}

// ---------------------------------------------------------------------------
// End-to-end encrypted sync through a managed relay.

/// Create an end-to-end encrypted company on a relay. Returns a [`Remote`]
/// for the owner and a fresh key; both go into invite codes, the key never
/// to the relay.
pub fn create_relay_company(url: &str, admin_key: Option<&str>) -> Result<(Remote, khata_seal::CompanyKey), SyncError> {
    #[derive(Deserialize)]
    struct Created {
        company_id: String,
        owner_token: String,
    }
    // Refuse plain http across the internet before anything is sent.
    let nil = "00000000-0000-0000-0000-000000000000".to_owned();
    check_remote(&Remote { url: url.to_owned(), company_id: nil, token: String::new() })?;
    let mut req = agent().post(&format!("{}/v1/relay", url.trim_end_matches('/')));
    if let Some(k) = admin_key {
        req = req.header("authorization", &format!("Bearer {k}"));
    }
    let resp = req.send_empty().map_err(|e| SyncError::Network(e.to_string()))?;
    let c: Created = read(resp)?;
    let remote = Remote { url: url.trim_end_matches('/').to_owned(), company_id: c.company_id, token: c.owner_token };
    check_remote(&remote)?;
    Ok((remote, khata_seal::CompanyKey::generate().map_err(|e| SyncError::Network(e.to_string()))?))
}

fn seal_entry(key: &khata_seal::CompanyKey, company_id: &str, e: &AuditEntry) -> Result<khata_seal::SealedEntry, SyncError> {
    let plain = serde_json::to_vec(e).expect("entry serializes");
    let blob = khata_seal::seal(key, &khata_seal::entry_context(company_id, e.seq), &plain)
        .map_err(|err| SyncError::Network(err.to_string()))?;
    Ok(khata_seal::SealedEntry { seq: e.seq, prev_hash: e.prev_hash.clone(), hash: e.hash.clone(), blob: hex::encode(blob) })
}

fn open_entry(key: &khata_seal::CompanyKey, company_id: &str, s: &khata_seal::SealedEntry) -> Result<AuditEntry, SyncError> {
    let bad = |why: &str| SyncError::InvalidFromServer(format!("entry {}: {why}", s.seq));
    let blob = hex::decode(&s.blob).map_err(|_| bad("not hex"))?;
    let plain = khata_seal::open(key, &khata_seal::entry_context(company_id, s.seq), &blob)
        .map_err(|_| bad("cannot be decrypted with this company's key"))?;
    let e: AuditEntry = serde_json::from_slice(&plain).map_err(|_| bad("not an audit entry"))?;
    if e.seq != s.seq || e.hash != s.hash || e.prev_hash != s.prev_hash {
        return Err(bad("metadata does not match its contents"));
    }
    Ok(e)
}

fn relay_pull(remote: &Remote, key: &khata_seal::CompanyKey, from: u64) -> Result<Vec<AuditEntry>, SyncError> {
    #[derive(Deserialize)]
    struct Page {
        entries: Vec<khata_seal::SealedEntry>,
        #[serde(default)]
        more: bool,
    }
    let mut out = Vec::new();
    loop {
        let at = from + out.len() as u64;
        let url = format!("{}/v1/relay/{}/pull?from={at}", remote.url.trim_end_matches('/'), remote.company_id);
        let resp = agent()
            .get(&url)
            .header("authorization", &format!("Bearer {}", remote.token))
            .call()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        let page: Page = read(resp)?;
        let done = !page.more || page.entries.is_empty();
        for s in &page.entries {
            out.push(open_entry(key, &remote.company_id, s)?);
        }
        if done {
            return Ok(out);
        }
    }
}

/// Sealed hex per push request, well under the server's 8 MiB body limit.
const PUSH_BATCH_BYTES: usize = 4 * 1024 * 1024;
/// Entries per push request (the server takes up to 10 000).
const PUSH_BATCH_ENTRIES: usize = 2_000;

/// Seal and push `entries` on top of `base_len`, in batches the server
/// accepts. `progress` hears the relay's length after each batch, so a
/// failure part-way loses nothing already sent.
fn relay_push(
    remote: &Remote,
    key: &khata_seal::CompanyKey,
    base_len: u64,
    entries: &[AuditEntry],
    mut progress: impl FnMut(u64) -> Result<(), SyncError>,
) -> Result<u64, SyncError> {
    let url = format!("{}/v1/relay/{}/push", remote.url.trim_end_matches('/'), remote.company_id);
    let mut len = base_len;
    let mut rest = entries;
    while !rest.is_empty() {
        let mut batch = Vec::new();
        let mut bytes = 0;
        for e in rest {
            let s = seal_entry(key, &remote.company_id, e)?;
            if !batch.is_empty() && (bytes + s.blob.len() > PUSH_BATCH_BYTES || batch.len() == PUSH_BATCH_ENTRIES) {
                break;
            }
            bytes += s.blob.len();
            batch.push(s);
        }
        let resp = agent()
            .post(&url)
            .header("authorization", &format!("Bearer {}", remote.token))
            .send_json(serde_json::json!({ "base_len": len, "entries": batch }))
            .map_err(|e| SyncError::Network(e.to_string()))?;
        len = read::<Pushed>(resp)?.len;
        progress(len)?;
        rest = &rest[batch.len()..];
    }
    Ok(len)
}

/// Like [`sync`], through a relay that only ever sees sealed entries and
/// sealed shared records.
pub fn sync_sealed(
    store: &mut Store,
    book: &mut Book,
    remote: &Remote,
    key: &khata_seal::CompanyKey,
) -> Result<SyncOutcome, SyncError> {
    sync_all(store, book, remote, Some(key)).map(|(books, _)| books)
}

fn sync_books_sealed(
    store: &mut Store,
    book: &mut Book,
    remote: &Remote,
    key: &khata_seal::CompanyKey,
) -> Result<SyncOutcome, SyncError> {
    check_remote(remote)?;
    store.save(book)?;
    let synced = match store.sync_state(&remote.url)? {
        Some((linked, _)) if linked != remote.company_id => {
            return Err(SyncError::WrongCompany { linked, requested: remote.company_id.clone() });
        }
        Some((_, len)) => len,
        None => 0,
    };
    let local_len = book.audit().entries().len() as u64;
    let local_new = local_len.checked_sub(synced).ok_or(SyncError::BehindSyncState)?;
    let incoming = relay_pull(remote, key, synced)?;
    match (local_new, incoming.len() as u64) {
        (0, 0) => Ok(SyncOutcome::UpToDate),
        (n, 0) => {
            let pending = &book.audit().entries()[synced as usize..];
            relay_push(remote, key, synced, pending, |len| {
                store.set_sync_state(&remote.url, &remote.company_id, len).map_err(SyncError::from)
            })
            .map_err(|e| match e {
                SyncError::Server { status: 409, .. } => SyncError::Conflict { local: n, remote: 1 },
                other => other,
            })?;
            Ok(SyncOutcome::Pushed(n))
        }
        (0, m) => {
            let mut next = book.clone();
            for e in &incoming {
                next.apply_entry(e).map_err(|err| SyncError::InvalidFromServer(format!("entry {}: {err}", e.seq)))?;
            }
            store.append(&incoming)?;
            *book = next;
            store.set_sync_state(&remote.url, &remote.company_id, synced + m)?;
            Ok(SyncOutcome::Pulled(m))
        }
        (n, m) => Err(SyncError::Conflict { local: n, remote: m }),
    }
}

/// A company moved to another relay by [`move_relay`].
#[derive(Debug)]
pub struct Moved {
    /// The owner's link to the company on the new relay.
    pub remote: Remote,
    /// The company's new end-to-end key.
    pub key: khata_seal::CompanyKey,
    /// Entries now on the new relay (the whole trail).
    pub entries: u64,
    /// Colleagues' access revoked on the old relay; `None` when it couldn't
    /// be reached after the copy (their old invites then still open the old,
    /// frozen copy until revoked there).
    pub revoked: Option<usize>,
}

/// Move a relay company to another relay, for example from a relay hosted
/// for a while by someone else to the client's own.
///
/// The file must be fully synced with `from` (nothing to send, nothing to
/// receive). The company is created on the relay at `to_url` (with
/// `admin_key` if that relay doesn't accept open sign-ups) and the whole
/// trail is sealed again for it — the seal binds each entry to its company
/// id — under a **new key**:
/// - invite codes carry the relay address, company id and token, so every
///   colleague needs a new code anyway; a new key costs nothing extra;
/// - whoever held the old key (old invite codes left in chats, people whose
///   access was revoked, the old relay's copy if that key ever leaks) cannot
///   read anything written after the move.
///
/// Afterwards colleagues' access on the old relay is revoked, so their old
/// invites stop syncing instead of quietly diverging; anything they pushed
/// to the old relay meanwhile is carried over first. The old relay keeps
/// its sealed copy (it is append-only) and only the owner's token still
/// opens it. On success this file syncs with the returned remote; the
/// caller stores the new remote and key.
pub fn move_relay(
    store: &mut Store,
    book: &mut Book,
    from: &Remote,
    key: &khata_seal::CompanyKey,
    to_url: &str,
    admin_key: Option<&str>,
) -> Result<Moved, SyncError> {
    check_remote(from)?;
    let to_url = to_url.trim().trim_end_matches('/');
    if to_url.eq_ignore_ascii_case(from.url.trim_end_matches('/')) {
        return Err(SyncError::SameRelay(to_url.to_owned()));
    }
    store.save(book)?;
    let synced = match store.sync_state(&from.url)? {
        Some((linked, _)) if linked != from.company_id => {
            return Err(SyncError::WrongCompany { linked, requested: from.company_id.clone() });
        }
        Some((_, len)) => len,
        None => 0,
    };
    let local_new = (book.audit().entries().len() as u64).checked_sub(synced).ok_or(SyncError::BehindSyncState)?;
    let incoming = relay_pull(from, key, synced)?.len() as u64;
    if local_new > 0 || incoming > 0 {
        return Err(SyncError::NotSynced { local: local_new, remote: incoming });
    }

    // Shared records: take the old relay's latest first.
    sync_records(store, from, Some(key))?;

    let (to, new_key) = create_relay_company(to_url, admin_key)?;
    let mut len = relay_push(&to, &new_key, 0, book.audit().entries(), |_| Ok(()))?;
    // …and seal every record again for the new relay.
    sync_records(store, &to, Some(&new_key))?;

    // Freeze the old copy: colleagues' old access stops working there.
    let revoked = revoke_colleagues(from).ok();
    // Records a colleague changed on the old relay meanwhile come along too.
    if sync_records(store, from, Some(key)).is_ok_and(|r| r.received > 0) {
        sync_records(store, &to, Some(&new_key))?;
    }
    // Entries a colleague pushed to the old relay before that are carried
    // over: applied here like a pull, then sent to the new relay.
    if let Ok(late) = relay_pull(from, key, synced)
        && !late.is_empty()
    {
        let mut next = book.clone();
        for e in &late {
            next.apply_entry(e).map_err(|err| SyncError::InvalidFromServer(format!("entry {}: {err}", e.seq)))?;
        }
        store.append(&late)?;
        *book = next;
        store.set_sync_state(&from.url, &from.company_id, synced + late.len() as u64)?;
        len = relay_push(&to, &new_key, len, &late, |_| Ok(()))?;
    }
    store.set_sync_state(&to.url, &to.company_id, len)?;
    Ok(Moved { remote: to, key: new_key, entries: len, revoked })
}

/// Revoke every access to `remote`'s company except the owner's own.
fn revoke_colleagues(remote: &Remote) -> Result<usize, SyncError> {
    #[derive(Deserialize)]
    struct Share {
        id: String,
        role: String,
        revoked: bool,
    }
    let base = format!("{}/v1/companies/{}/shares", remote.url.trim_end_matches('/'), remote.company_id);
    let auth = format!("Bearer {}", remote.token);
    let resp = agent().get(&base).header("authorization", &auth).call().map_err(|e| SyncError::Network(e.to_string()))?;
    let shares: Vec<Share> = read(resp)?;
    let mut n = 0;
    for s in shares.iter().filter(|s| s.role != "owner" && !s.revoked) {
        let resp = agent()
            .delete(&format!("{base}/{}", s.id))
            .header("authorization", &auth)
            .call()
            .map_err(|e| SyncError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(SyncError::Server { status, message: String::new() });
        }
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(url: &str, company_id: &str) -> Remote {
        Remote { url: url.into(), company_id: company_id.into(), token: "t".into() }
    }

    const ID: &str = "0199f1c2-7a3b-7c4d-8e5f-0123456789ab";

    #[test]
    fn remotes_must_be_https_except_localhost() {
        assert!(check_remote(&remote("https://books.example.in", ID)).is_ok());
        assert!(check_remote(&remote("http://localhost:8787", ID)).is_ok());
        assert!(check_remote(&remote("http://127.0.0.1:8787/", ID)).is_ok());
        assert!(matches!(check_remote(&remote("http://books.example.in", ID)), Err(SyncError::InsecureRemote(_))));
        assert!(matches!(check_remote(&remote("http://localhost.evil.com", ID)), Err(SyncError::InsecureRemote(_))));
    }

    #[test]
    fn plain_http_only_on_the_office_network() {
        for ok in ["http://192.168.1.20:9090", "http://10.0.0.5:9090/", "http://172.20.1.1", "http://accounts-pc.local:9090", "http://[fd12::1]:9090", "http://100.101.102.103:9090"] {
            assert!(check_remote(&remote(ok, ID)).is_ok(), "{ok}");
        }
        for bad in ["http://8.8.8.8:9090", "http://172.32.0.1", "http://evil.com", "http://user@192.168.1.20", "http://192.168.1.20.evil.com", "http://100.128.0.1", "http://100.63.255.1"] {
            assert!(matches!(check_remote(&remote(bad, ID)), Err(SyncError::InsecureRemote(_))), "{bad}");
        }
    }

    #[test]
    fn company_ids_cannot_inject_paths() {
        for bad in ["../../v1/companies", "0199f1c2-7a3b-7c4d-8e5f-0123456789a/", "", "x"] {
            assert!(matches!(check_remote(&remote("https://b.example", bad)), Err(SyncError::InsecureRemote(_))), "{bad}");
        }
    }
}

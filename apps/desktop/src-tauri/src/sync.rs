//! Sync with a KhataOS server: per-file settings, "Sync now" and rebase.
//!
//! Settings live in the company file's device-local `settings` table (not the
//! audit trail), so the token never leaves this machine through sync. The
//! company's shared records (notes, invoice design, tax settings) sync with
//! the books (`docs/sync-settings.md`).

use std::path::Path;

use chrono::Local;
use khata_store::Store;
use khata_sync::{Remote, SyncError, SyncOutcome};
use serde::Serialize;

use crate::api::{ApiResult, Slot, err};

pub const URL: &str = "sync.url";
pub const COMPANY: &str = "sync.company_id";
pub const TOKEN: &str = "sync.token";
/// Human-readable result of the last attempt, shown in the title strip.
pub const LAST: &str = "sync.last";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SyncSettings {
    pub url: String,
    pub company_id: String,
    /// The token itself is never sent back to the webview.
    pub has_token: bool,
    pub last_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SyncResult {
    /// "up_to_date", "pushed", "pulled" or "conflict".
    pub status: String,
    pub count: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Renumbered {
    pub kind: khata_core::VoucherKind,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Attention {
    pub seq: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RebaseReport {
    pub archived: String,
    pub renumbered: Vec<Renumbered>,
    pub needs_attention: Vec<Attention>,
    pub pushed: u64,
}

/// The open company for syncing. Unlike `api::open_mut` this works for
/// read-only (Viewer) files too: a sync only appends what the server
/// already accepted.
pub(crate) fn slot_mut(slot: &mut Slot) -> ApiResult<(&mut Store, &mut khata_core::Book)> {
    slot.as_mut()
        .map(|(s, b)| (s, b))
        .ok_or_else(|| "no company is open".to_owned())
}

fn store_ref(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref()
        .map(|(s, _)| s)
        .ok_or_else(|| "no company is open".to_owned())
}

pub fn settings(slot: &Slot) -> ApiResult<SyncSettings> {
    let store = store_ref(slot)?;
    let get = |k| store.setting(k).map_err(err);
    Ok(SyncSettings {
        url: get(URL)?.unwrap_or_default(),
        company_id: get(COMPANY)?.unwrap_or_default(),
        has_token: get(TOKEN)?.is_some_and(|t| !t.is_empty()),
        last_status: get(LAST)?,
    })
}

/// Save settings. An empty `token` keeps the stored one.
pub fn set_settings(
    slot: &Slot,
    url: &str,
    company_id: &str,
    token: &str,
) -> ApiResult<SyncSettings> {
    let store = store_ref(slot)?;
    let url = url.trim().trim_end_matches('/');
    let company_id = company_id.trim();
    if !is_uuid(company_id) {
        return Err(
            "the company id must be the UUID your server gave you (like 0190f3c4-…-…)".into(),
        );
    }
    // Same rule as khata-sync: https, or plain http only on this computer or
    // the office network (the "one PC acts as server" setup).
    let candidate = Remote {
        url: url.to_owned(),
        company_id: company_id.to_owned(),
        token: String::new(),
    };
    if url.len() <= "https://".len() || khata_sync::validate_remote(&candidate).is_err() {
        return Err(
            "the server URL must start with https:// (http:// is allowed only on this computer or the office network)"
                .into(),
        );
    }
    store.set_setting(URL, Some(url)).map_err(err)?;
    store.set_setting(COMPANY, Some(company_id)).map_err(err)?;
    if !token.trim().is_empty() {
        store.set_setting(TOKEN, Some(token.trim())).map_err(err)?;
    }
    settings(slot)
}

fn is_uuid(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// What to tell the user for each sync failure.
pub(crate) fn explain(e: &SyncError) -> String {
    match e {
        SyncError::Network(m) => format!("couldn't reach the sync server (network error: {m})"),
        SyncError::Server { status: 401 | 403, .. } => "the server refused this token; check Sync settings".into(),
        SyncError::Server { status, message } => format!("the server said {status}: {message}"),
        SyncError::WrongCompany { .. } => format!("{e}; check the company id in Sync settings"),
        SyncError::InsecureRemote(m) => format!("refusing to sync: {m}"),
        SyncError::BehindSyncState => {
            "this file is older than what was already synced (was a backup restored?); open the latest file or ask for help".into()
        }
        SyncError::InvalidFromServer(m) => format!("the server sent books that don't check out, so nothing was changed: {m}"),
        SyncError::File(m) => format!("couldn't replace the company file: {m}"),
        other => other.to_string(),
    }
}

pub(crate) fn remote(store: &Store) -> ApiResult<Remote> {
    let get = |k| {
        store
            .setting(k)
            .map_err(err)
            .map(|v| v.filter(|s| !s.is_empty()))
    };
    match (get(URL)?, get(COMPANY)?, get(TOKEN)?) {
        (Some(url), Some(company_id), Some(token)) => Ok(Remote {
            url,
            company_id,
            token,
        }),
        _ => Err(
            "sync isn't set up for this company yet: open Company (Alt+K) → Sync settings".into(),
        ),
    }
}

pub(crate) fn stamp(store: &Store, message: &str) {
    let line = format!("{message} · {}", Local::now().format("%d-%b %H:%M"));
    let _ = store.set_setting(LAST, Some(&line));
}

/// `(status, count, message)` for what a sync did: the books first, else
/// the notes and settings (shared records) that moved.
pub(crate) fn outcome(books: SyncOutcome, records: &khata_sync::RecordsOutcome) -> (&'static str, u64, String) {
    match books {
        SyncOutcome::Pushed(n) => ("pushed", n, format!("Sent {n} change(s)")),
        SyncOutcome::Pulled(n) => ("pulled", n, format!("Received {n} change(s)")),
        SyncOutcome::UpToDate if records.received > 0 => {
            ("pulled", records.received, format!("Received {} update(s) to notes and settings", records.received))
        }
        SyncOutcome::UpToDate if records.sent > 0 => ("pushed", records.sent, format!("Sent {} update(s) to notes and settings", records.sent)),
        SyncOutcome::UpToDate => ("up_to_date", 0, "Up to date".into()),
    }
}

/// Push or pull. Both sides having changes is reported as `conflict`, not an
/// error, so the UI can offer a rebase.
pub fn sync_now(slot: &mut Slot) -> ApiResult<SyncResult> {
    let (store, book) = slot_mut(slot)?;
    let remote = remote(store)?;
    let result = match crate::lan::sync_any(store, book, &remote) {
        Ok((books, records)) => {
            let (status, count, message) = outcome(books, &records);
            SyncResult {
                status: status.into(),
                count,
                message,
            }
        }
        Err(SyncError::Conflict { local, remote }) => SyncResult {
            status: "conflict".into(),
            count: local,
            message: format!("Conflict: {local} change(s) here, {remote} on the server"),
        },
        Err(e) => {
            stamp(store, "Sync failed");
            return Err(explain(&e));
        }
    };
    stamp(store, &result.message);
    Ok(result)
}

/// Resolve a conflict: rebuild from the server and re-apply this device's
/// changes on top. The old file is archived next to the new one.
pub fn rebase(slot: &mut Slot, path: &Path) -> ApiResult<RebaseReport> {
    let (store, _) = slot_mut(slot)?;
    let remote = remote(store)?;
    let key = crate::lan::company_key(store)?;
    let carried = store.settings().map_err(err)?;
    // Release the file before it is renamed and replaced.
    *slot = None;
    let rebased = match &key {
        Some(key) => khata_sync::rebase_sealed(path, &remote, key),
        None => khata_sync::rebase(path, &remote),
    };
    match rebased {
        Ok(r) => {
            for (k, v) in carried.iter().filter(|(k, _)| k != LAST) {
                r.store.set_setting(k, Some(v)).map_err(err)?;
            }
            let report = RebaseReport {
                archived: r.archived.display().to_string(),
                renumbered: r
                    .renumbered
                    .into_iter()
                    .map(|(kind, old, new)| Renumbered { kind, old, new })
                    .collect(),
                needs_attention: r
                    .needs_attention
                    .into_iter()
                    .map(|(seq, reason)| Attention { seq, reason })
                    .collect(),
                pushed: r.pushed,
            };
            stamp(
                &r.store,
                &format!("Rebased, sent {} change(s)", report.pushed),
            );
            *slot = Some((r.store, r.book));
            Ok(report)
        }
        Err(e) => {
            // Put the untouched original back in place.
            let store = Store::open(path).map_err(err)?;
            let book = store.load().map_err(err)?;
            stamp(&store, "Rebase failed");
            *slot = Some((store, book));
            Err(explain(&e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, ledger, voucher};
    use crate::api::{self, create_company, post_voucher};
    use khata_core::{Book, DrCr};
    use serde_json::{Value, json};

    const ADMIN: &str = "desktop-test-admin-key-0123456789";
    const C1: &str = "0190f3c4-1111-7aaa-8bbb-0123456789ab";
    const C2: &str = "0190f3c4-2222-7aaa-8bbb-0123456789ab";

    fn start_server(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "khataos-desktop-server-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let state = khata_server::AppState::open(khata_server::Config {
            data_dir: dir,
            admin_key: ADMIN.into(),
        })
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    axum::serve(listener, khata_server::router(state))
                        .await
                        .unwrap();
                });
        });
        format!("http://{addr}")
    }

    fn post(url: &str, token: &str, body: Value) -> Value {
        let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
        agent
            .post(url)
            .header("authorization", &format!("Bearer {token}"))
            .send_json(body)
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap()
    }

    #[test]
    fn settings_round_trip_and_hide_the_token() {
        let f = TempFile::new("sync-settings");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        assert!(sync_now(&mut slot).unwrap_err().contains("isn't set up"));
        assert!(set_settings(&slot, "ftp://x", C1, "t").is_err());
        assert!(
            set_settings(&slot, "http://books.example.in", C1, "t")
                .unwrap_err()
                .contains("https://")
        );
        assert!(set_settings(&slot, "http://localhost.evil.com", C1, "t").is_err());
        assert!(
            set_settings(&slot, "http://localhost:8080", C1, "t").is_ok(),
            "plain http on this computer"
        );
        assert!(
            set_settings(&slot, "http://192.168.1.20:9090", C1, "t").is_ok(),
            "plain http on the office network"
        );
        assert!(set_settings(&slot, "http://8.8.8.8:9090", C1, "t").is_err());
        assert!(
            set_settings(&slot, "https://x", "c1", "t")
                .unwrap_err()
                .contains("UUID")
        );
        let s = set_settings(&slot, "https://books.example.in/", C1, "secret").unwrap();
        assert_eq!(
            (s.url.as_str(), s.company_id.as_str(), s.has_token),
            ("https://books.example.in", C1, true)
        );
        // A blank token keeps the stored one.
        assert!(
            set_settings(&slot, "https://books.example.in", C2, "")
                .unwrap()
                .has_token
        );
        drop(slot);
        let mut reopened = None;
        api::open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(settings(&reopened).unwrap().company_id, C2);
        assert_eq!(
            serde_json::to_value(settings(&reopened).unwrap())
                .unwrap()
                .get("token"),
            None
        );
    }

    #[test]
    fn network_failure_is_reported_and_recorded() {
        let f = TempFile::new("sync-offline");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        set_settings(&slot, "http://127.0.0.1:9", C1, "t").unwrap();
        assert!(
            sync_now(&mut slot)
                .unwrap_err()
                .contains("couldn't reach the sync server")
        );
        let info = api::company_info(&slot, None).unwrap();
        assert!(info.sync_status.unwrap().starts_with("Sync failed"));
    }

    #[test]
    fn push_conflict_and_rebase_through_the_command_layer() {
        let url = start_server("rebase");
        let created = post(
            &format!("{url}/v1/companies"),
            ADMIN,
            json!({ "name": "Demo Traders" }),
        );
        let company_id = created["company_id"].as_str().unwrap().to_owned();
        let token = created["owner_token"].as_str().unwrap().to_owned();

        // This device: two ledgers, synced.
        let f = TempFile::new("sync-device-a");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Traders").unwrap();
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "");
        let rent = ledger(&mut slot, "Rent", "Indirect Expenses", "");
        set_settings(&slot, &url, &company_id, &token).unwrap();
        assert_eq!(sync_now(&mut slot).unwrap().status, "pushed");
        assert_eq!(sync_now(&mut slot).unwrap().status, "up_to_date");

        // Another device pulls, then posts Payment No. 1 first.
        let other = TempFile::new("sync-device-b");
        let mut store_b = Store::create(&other.0, "Demo Traders").unwrap();
        let mut book_b = Book::new("Demo Traders");
        let remote = Remote {
            url: url.clone(),
            company_id: company_id.clone(),
            token: token.clone(),
        };
        khata_sync::sync(&mut store_b, &mut book_b, &remote).unwrap();
        let (b_rent, b_bank) = (
            book_b.ledger_by_name("Rent").unwrap().id,
            book_b.ledger_by_name("HDFC Bank").unwrap().id,
        );
        book_b
            .post_voucher(
                api::now(),
                "b",
                khata_core::Voucher {
                    id: khata_core::VoucherId::new(),
                    kind: khata_core::VoucherKind::Payment,
                    number: "1".into(),
                    date: chrono::NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                    entries: vec![
                        khata_core::Entry::dr(b_rent, 200.into()),
                        khata_core::Entry::cr(b_bank, 200.into()),
                    ],
                    narration: String::new(),
                    cancelled: false, series: None, stock: Vec::new(),
                },
            )
            .unwrap();
        khata_sync::sync(&mut store_b, &mut book_b, &remote).unwrap();

        // Meanwhile this device posts its own Payment No. 1 offline.
        post_voucher(
            &mut slot,
            voucher(
                "Payment",
                "2026-04-02",
                vec![entry(&rent, DrCr::Dr, "100"), entry(&bank, DrCr::Cr, "100")],
            ),
        )
        .unwrap();
        let r = sync_now(&mut slot).unwrap();
        assert_eq!(r.status, "conflict");

        let report = rebase(&mut slot, &f.0).unwrap();
        assert_eq!(
            report.renumbered,
            [Renumbered {
                kind: khata_core::VoucherKind::Payment,
                old: "1".into(),
                new: "2".into()
            }]
        );
        assert!(report.needs_attention.is_empty());
        assert_eq!(report.pushed, 1);
        assert!(Path::new(&report.archived).is_file(), "old file archived");
        // The reloaded company has both payments and keeps its sync settings.
        assert_eq!(api::company_info(&slot, None).unwrap().voucher_count, 2);
        assert!(settings(&slot).unwrap().has_token);
        assert_eq!(sync_now(&mut slot).unwrap().status, "up_to_date");
        let _ = std::fs::remove_file(&report.archived);
    }
}

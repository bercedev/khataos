//! Accountant files (`.khatapkg`): sharing books with no server and no
//! internet connection between the two sides.
//!
//! Share (Alt+M) → "Send to accountant (file)" writes the entries the other
//! side doesn't have yet (or everything), encrypted with a passphrase, so it
//! can travel by WhatsApp, e-mail or USB stick. Import (Alt+O) → "Open
//! accountant file" verifies every entry before anything changes: into the
//! open company, into a company file that came from earlier packages, or
//! into a new company file.
//!
//! A file also carries the company's shared records changed since the last
//! file (notes, invoice design, tax settings; `docs/sync-settings.md`),
//! merged last writer wins on the other side.

use std::path::{Path, PathBuf};

use khata_package::PackageError;
use khata_store::Store;
use serde::Serialize;

use crate::api::{self, ApiResult, Slot, err};

/// Entries sent so far by this file (the next package starts here).
pub const SENT_TO: &str = "package.sent_to";
/// The package identity of the books (hash of the first audit entry).
pub const PACKAGE_ID: &str = "package.company_id";
/// Shared records sent so far by this file (a revision of its `records`).
pub const RECORDS_SENT_TO: &str = "package.records_sent_to";
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MIN_PASSPHRASE: usize = 8;

pub fn explain(e: &PackageError) -> String {
    match e {
        PackageError::NotAPackage => "this isn't a KhataOS accountant file (.khatapkg), or it is damaged".into(),
        PackageError::Seal(_) => "wrong passphrase, or the file was changed on the way. Check the passphrase with the sender".into(),
        PackageError::WrongCompany { ours, theirs } => {
            format!("this file is for {theirs}, not {ours}. Shut this company and open the file again")
        }
        PackageError::Gap { from, have } => format!(
            "an earlier file is missing: this one starts at entry {from} and your books have {have}. \
             Ask the sender for the earlier file, or for a full file"
        ),
        PackageError::Diverged { at } => format!(
            "both sides changed the books since they were last in step (from entry {at}). \
             Use sync, or send your changes to the other side first and ask for a new file"
        ),
        PackageError::Invalid { seq, reason } => {
            format!("entry {seq} in the file doesn't check out ({reason}); nothing was changed")
        }
        PackageError::Empty => "these books have no entries to send yet".into(),
    }
}

fn get(store: &Store, key: &str) -> Option<String> {
    store.setting(key).ok().flatten().filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Sent {
    pub path: String,
    pub from_seq: u64,
    pub to_seq: u64,
    pub entries: u64,
    /// Notes and settings (shared records) in the file.
    pub records: u64,
}

/// Where the next "new entries only" file starts.
pub fn next_from(slot: &Slot) -> ApiResult<u64> {
    let (store, _) = slot.as_ref().ok_or("no company is open")?;
    Ok(get(store, SENT_TO)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}

/// Write a package of the open company to `path`: the entries after the
/// last file sent, or everything when `full`.
pub fn send(slot: &Slot, path: &Path, passphrase: &str, full: bool) -> ApiResult<Sent> {
    if passphrase.chars().count() < MIN_PASSPHRASE {
        return Err(format!(
            "use a passphrase of at least {MIN_PASSPHRASE} characters"
        ));
    }
    let (store, book) = slot.as_ref().ok_or("no company is open")?;
    let len = book.audit().entries().len() as u64;
    let from = if full { 0 } else { next_from(slot)?.min(len) };
    let records_from = if full {
        0
    } else {
        get(store, RECORDS_SENT_TO).and_then(|v| v.parse().ok()).unwrap_or(0)
    };
    let (records, _) = store
        .records_since(records_from, usize::MAX, usize::MAX)
        .map_err(err)?;
    let records: Vec<khata_store::Record> = records.into_iter().map(|(_, r)| r).collect();
    if from >= len && len > 0 && records.is_empty() {
        return Err(format!(
            "no new entries since the last file you sent (it had entries up to {len}). Choose Everything to send the whole books again"
        ));
    }
    let bytes = khata_package::export_with_records(book, from, &records, passphrase)
        .map_err(|e| explain(&e))?;
    std::fs::write(path, bytes).map_err(|e| format!("cannot save {}: {e}", path.display()))?;
    store
        .set_setting(SENT_TO, Some(&len.to_string()))
        .map_err(err)?;
    store
        .set_setting(
            RECORDS_SENT_TO,
            Some(&store.record_rev().map_err(err)?.to_string()),
        )
        .map_err(err)?;
    if let Some(id) = khata_package::company_id(book) {
        store.set_setting(PACKAGE_ID, Some(id)).map_err(err)?;
    }
    Ok(Sent {
        path: path.display().to_string(),
        from_seq: from,
        to_seq: len,
        entries: len - from,
        records: records.len() as u64,
    })
}

/// Merge a package's shared records into `store`. What came from the
/// sender needn't go back to it, so when nothing here was waiting to be
/// sent, the next file starts after these.
fn merge_records(store: &mut Store, records: &[khata_store::Record]) -> ApiResult<u64> {
    let sent_to: u64 = get(store, RECORDS_SENT_TO)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let in_step = sent_to >= store.record_rev().map_err(err)?;
    let changed = store.merge_records(records).map_err(err)?.len() as u64;
    if in_step {
        let rev = store.record_rev().map_err(err)?;
        store
            .set_setting(RECORDS_SENT_TO, Some(&rev.to_string()))
            .map_err(err)?;
    }
    Ok(changed)
}

fn read(path: &Path) -> ApiResult<Vec<u8>> {
    let meta =
        std::fs::metadata(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err("this file is too large to be a KhataOS accountant file".into());
    }
    std::fs::read(path).map_err(|e| format!("cannot open {}: {e}", path.display()))
}

/// What a package holds, readable before the passphrase is asked for.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PackageInfo {
    pub company: String,
    pub from_seq: u64,
    pub to_seq: u64,
    pub entries: u64,
    /// The open company is the one in the file.
    pub for_open_company: bool,
}

pub fn inspect(slot: &Slot, path: &Path) -> ApiResult<PackageInfo> {
    let h = khata_package::inspect(&read(path)?).map_err(|e| explain(&e))?;
    let for_open_company = slot
        .as_ref()
        .is_some_and(|(_, b)| khata_package::company_id(b) == Some(h.company_id.as_str()));
    Ok(PackageInfo {
        company: h.company,
        from_seq: h.from_seq,
        entries: h.to_seq.saturating_sub(h.from_seq),
        to_seq: h.to_seq,
        for_open_company,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Received {
    pub info: api::CompanyInfo,
    pub added: u64,
    pub already_had: u64,
    /// Notes and settings (shared records) that changed here.
    pub records: u64,
    /// A new company file was made from the package.
    pub created: bool,
    /// Another company than the one open before is now open.
    pub switched: bool,
}

/// A company file in `dir` whose books are the package's company.
fn find_by_package_id(dir: &Path, id: &str) -> Option<PathBuf> {
    api::list_company_files(dir).into_iter().find_map(|f| {
        let store = Store::open(&f.path).ok()?;
        (get(&store, PACKAGE_ID)? == id).then(|| PathBuf::from(f.path))
    })
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let file = api::company_file_name(name);
    let stem = file.trim_end_matches(".khata");
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(&file)
            } else {
                dir.join(format!("{stem}-{n}.khata"))
            }
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Apply the package to the books in `slot`, all or nothing, then merge
/// its shared records. Returns what the books took and how many records
/// changed.
fn apply_open(
    slot: &mut Slot,
    bytes: &[u8],
    passphrase: &str,
) -> ApiResult<(khata_package::Applied, u64)> {
    let (store, book) = api::open_mut(slot)?;
    let (applied, records) =
        khata_package::apply_with_records(book, bytes, passphrase).map_err(|e| explain(&e))?;
    if let Err(e) = store.save(book) {
        // Keep memory and file in step.
        *book = store.load().map_err(err)?;
        return Err(format!("cannot save the received entries: {e}"));
    }
    if let Some(id) = khata_package::company_id(book) {
        store.set_setting(PACKAGE_ID, Some(id)).map_err(err)?;
    }
    let changed = merge_records(store, &records)?;
    Ok((applied, changed))
}

/// [`receive_locked`], dropping the company lock (tests).
#[cfg(test)]
pub fn receive(
    slot: &mut Slot,
    open_path: Option<&Path>,
    dir: &Path,
    path: &Path,
    passphrase: &str,
) -> ApiResult<(Received, PathBuf)> {
    receive_locked(slot, None, open_path, dir, path, passphrase).map(|(r, p, _)| (r, p))
}

/// Open an accountant file: into the open company if it is the one in the
/// file, else into the matching company file in `dir`, else (a full file)
/// as a new company in `dir`. Returns the company now open, its path and,
/// when it switched company, the new company's lock (`held` is the company
/// whose lock this window holds).
pub fn receive_locked(
    slot: &mut Slot,
    held: Option<&Path>,
    open_path: Option<&Path>,
    dir: &Path,
    path: &Path,
    passphrase: &str,
) -> ApiResult<(Received, PathBuf, Option<api::CompanyLock>)> {
    let bytes = read(path)?;
    let h = khata_package::inspect(&bytes).map_err(|e| explain(&e))?;
    let is_open = slot
        .as_ref()
        .is_some_and(|(_, b)| khata_package::company_id(b) == Some(h.company_id.as_str()));
    if is_open {
        let target = open_path.map(Path::to_path_buf).unwrap_or_default();
        let (a, records) = apply_open(slot, &bytes, passphrase)?;
        let info = api::company_info(slot, Some(&target))?;
        return Ok((
            Received {
                info,
                added: a.added,
                already_had: a.already_had,
                records,
                created: false,
                switched: false,
            },
            target,
            None,
        ));
    }
    std::fs::create_dir_all(dir).map_err(err)?;
    if let Some(target) = find_by_package_id(dir, &h.company_id) {
        let lock = api::CompanyLock::for_switch(held, &target)?;
        // Check the passphrase and the entries before switching company.
        let mut probe = Store::open(&target).map_err(err)?.load().map_err(err)?;
        khata_package::apply(&mut probe, &bytes, passphrase).map_err(|e| explain(&e))?;
        api::open_company(slot, &target)?;
        let (a, records) = apply_open(slot, &bytes, passphrase)?;
        let info = api::company_info(slot, Some(&target))?;
        return Ok((
            Received {
                info,
                added: a.added,
                already_had: a.already_had,
                records,
                created: false,
                switched: true,
            },
            target,
            lock,
        ));
    }
    if h.from_seq > 0 {
        return Err(format!(
            "this file holds later changes of {} (from entry {}). Open that company first, or ask the sender for a full file",
            h.company, h.from_seq
        ));
    }
    let (book, records) =
        khata_package::receive_new_with_records(&bytes, passphrase).map_err(|e| explain(&e))?;
    let target = unique_path(dir, &book.name);
    let lock = api::CompanyLock::acquire(&target)?;
    let mut store = Store::create(&target, &book.name).map_err(err)?;
    store.save(&book).map_err(err)?;
    let records = merge_records(&mut store, &records)?;
    store
        .set_setting(PACKAGE_ID, Some(&h.company_id))
        .map_err(err)?;
    // Everything received counts as already with the sender.
    store
        .set_setting(SENT_TO, Some(&h.to_seq.to_string()))
        .map_err(err)?;
    let added = book.audit().entries().len() as u64;
    drop(store);
    api::open_company(slot, &target)?;
    let info = api::company_info(slot, Some(&target))?;
    Ok((
        Received {
            info,
            added,
            already_had: 0,
            records,
            created: true,
            switched: true,
        },
        target,
        Some(lock),
    ))
}

pub mod cmd {

    use tauri::{AppHandle, Manager};

    use super::*;
    use crate::AppState;

    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> ApiResult<T> + Send + 'static,
    ) -> ApiResult<T> {
        tauri::async_runtime::spawn_blocking(f)
            .await
            .map_err(|e| e.to_string())?
    }

    fn slot(state: &AppState) -> std::sync::MutexGuard<'_, Slot> {
        state
            .company
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Passphrase stretching is deliberately slow, so these run off the main thread.
    #[tauri::command(rename_all = "snake_case")]
    pub async fn package_send(
        app: AppHandle,
        path: String,
        passphrase: String,
        full: bool,
    ) -> ApiResult<Sent> {
        blocking(move || {
            let local = crate::phone_files::local_output(&app, &path, "khatapkg")?;
            let sent = super::send(&slot(&app.state::<AppState>()), &local, &passphrase, full)?;
            crate::phone_files::finish_output(&app, &path, &local)?;
            Ok(sent)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn package_next_from(state: tauri::State<'_, AppState>) -> ApiResult<u64> {
        super::next_from(&slot(&state))
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn package_inspect(
        app: AppHandle,
        state: tauri::State<'_, AppState>,
        path: String,
    ) -> ApiResult<PackageInfo> {
        let local = crate::phone_files::local_input(&app, &path, &["khatapkg"], MAX_FILE_BYTES)?;
        super::inspect(&slot(&state), &local)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn package_receive(
        app: AppHandle,
        path: String,
        passphrase: String,
    ) -> ApiResult<Received> {
        blocking(move || {
            let state = app.state::<AppState>();
            let dir =
                api::default_company_dir(&app.path().app_data_dir().map_err(|e| e.to_string())?);
            let open_path = state.path.lock().ok().and_then(|p| p.clone());
            let (received, target) = {
                let mut slot = slot(&state);
                let mut held = state.company_lock.lock().map_err(|_| "company state is poisoned".to_owned())?;
                let local = crate::phone_files::local_input(&app, &path, &["khatapkg"], MAX_FILE_BYTES)?;
                let (received, target, lock) = super::receive_locked(
                    &mut slot,
                    held.as_ref().map(api::CompanyLock::company),
                    open_path.as_deref(),
                    &dir,
                    &local,
                    &passphrase,
                )?;
                if let Some(lock) = lock {
                    *held = Some(lock);
                }
                (received, target)
            };
            if let Ok(mut p) = state.path.lock() {
                *p = Some(target);
            }
            Ok(received)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, demo, entry, voucher};
    use crate::api::{post_voucher, trial_balance};
    use khata_core::DrCr;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("khataos-pkg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const PASS: &str = "correct horse battery";

    #[test]
    fn send_and_receive_through_the_command_layer() {
        let f = TempFile::new("pkg-owner");
        let (mut owner, ..) = demo(&f);
        let out = temp_dir("out");
        assert!(
            send(&owner, &out.join("x.khatapkg"), "short", false)
                .unwrap_err()
                .contains("at least")
        );

        // First file: everything.
        let first = out.join("demo-1.khatapkg");
        let sent = send(&owner, &first, PASS, false).unwrap();
        assert_eq!(sent.from_seq, 0);
        assert!(
            send(&owner, &out.join("again.khatapkg"), PASS, false)
                .unwrap_err()
                .contains("no new entries")
        );

        // The accountant has nothing open: a new company file is made.
        let books = temp_dir("accountant");
        let mut acct = None;
        let info = inspect(&acct, &first).unwrap();
        assert_eq!(
            (info.company.as_str(), info.from_seq, info.for_open_company),
            ("Demo Traders", 0, false)
        );
        assert!(
            receive(&mut acct, None, &books, &first, "wrong passphrase")
                .unwrap_err()
                .contains("wrong passphrase")
        );
        assert!(
            acct.is_none() && api::list_company_files(&books).is_empty(),
            "nothing created on a wrong passphrase"
        );
        let (r, acct_path) = receive(&mut acct, None, &books, &first, PASS).unwrap();
        assert!(r.created && r.switched);
        assert_eq!(r.added, sent.to_seq);
        assert_eq!(
            trial_balance(&acct, None).unwrap(),
            trial_balance(&owner, None).unwrap()
        );

        // Second file: only the new payment; applied to the open company.
        let ids = |slot: &Slot, name: &str| {
            api::list_ledgers(slot, name, "all")
                .unwrap()
                .into_iter()
                .find(|l| l.name == name)
                .unwrap()
                .id
        };
        let (rent, bank) = (ids(&owner, "Office Rent"), ids(&owner, "HDFC Bank"));
        post_voucher(
            &mut owner,
            voucher(
                "Payment",
                "2026-04-05",
                vec![entry(&rent, DrCr::Dr, "700"), entry(&bank, DrCr::Cr, "700")],
            ),
        )
        .unwrap();
        let second = out.join("demo-2.khatapkg");
        let sent2 = send(&owner, &second, PASS, false).unwrap();
        assert_eq!((sent2.from_seq, sent2.entries), (sent.to_seq, 1));
        assert!(inspect(&acct, &second).unwrap().for_open_company);
        let (r2, _) = receive(&mut acct, Some(&acct_path), &books, &second, PASS).unwrap();
        assert_eq!(
            (r2.added, r2.already_had, r2.created, r2.switched),
            (1, 0, false, false)
        );
        assert_eq!(
            trial_balance(&acct, None).unwrap(),
            trial_balance(&owner, None).unwrap()
        );
        // Applying it again changes nothing.
        let (r3, _) = receive(&mut acct, Some(&acct_path), &books, &second, PASS).unwrap();
        assert_eq!((r3.added, r3.already_had), (0, 1));

        // With nothing open, a later file finds the company it belongs to.
        acct = None;
        post_voucher(
            &mut owner,
            voucher(
                "Payment",
                "2026-04-06",
                vec![entry(&rent, DrCr::Dr, "1"), entry(&bank, DrCr::Cr, "1")],
            ),
        )
        .unwrap();
        let third = out.join("demo-3.khatapkg");
        send(&owner, &third, PASS, false).unwrap();
        let (r4, p4) = receive(&mut acct, None, &books, &third, PASS).unwrap();
        assert_eq!(
            (r4.added, r4.created, r4.switched, p4),
            (1, false, true, acct_path.clone())
        );
        assert_eq!(
            trial_balance(&acct, None).unwrap(),
            trial_balance(&owner, None).unwrap()
        );
    }

    #[test]
    fn notes_and_the_invoice_design_travel_in_accountant_files() {
        use crate::desk::{self, NoteInput};
        let f = TempFile::new("pkg-records-owner");
        let (mut owner, ..) = demo(&f);
        let out = temp_dir("records-out");
        let note = desk::save_note(
            &mut owner,
            &NoteInput { title: "Send the March bank statement".into(), ..NoteInput::default() },
        )
        .unwrap();
        let design = khata_print::InvoiceDesign { footer: "Thank you".into(), ..Default::default() };
        crate::invoice_design::set_design(&mut owner, design.clone()).unwrap();
        // Secrets of this computer stay here.
        owner.as_ref().unwrap().0.set_setting(crate::sync::TOKEN, Some("SECRET-TOKEN-123")).unwrap();
        let first = out.join("first.khatapkg");
        let sent = send(&owner, &first, PASS, false).unwrap();
        assert_eq!(sent.records, 2);
        assert!(!String::from_utf8_lossy(&std::fs::read(&first).unwrap()).contains("March bank"));

        let books = temp_dir("records-accountant");
        let mut acct = None;
        let (r, _) = receive(&mut acct, None, &books, &first, PASS).unwrap();
        assert_eq!((r.created, r.records), (true, 2));
        assert_eq!(desk::notes(&acct, None, None).unwrap()[0].title, note.title);
        assert_eq!(crate::invoice_design::branding(&acct).design, design);
        assert_eq!(acct.as_ref().unwrap().0.setting(crate::sync::TOKEN).unwrap(), None);
        // What came in isn't sent back: the accountant has nothing new.
        assert!(send(&acct, &out.join("back.khatapkg"), PASS, false).unwrap_err().contains("no new entries"));

        // Only a note changes: a file with no new entries still carries it,
        // and a deletion too; the next file has nothing left to send.
        desk::set_note_done(&mut owner, &note.id, true).unwrap();
        let second = out.join("second.khatapkg");
        let sent = send(&owner, &second, PASS, false).unwrap();
        assert_eq!((sent.entries, sent.records), (0, 1));
        assert!(send(&owner, &out.join("third.khatapkg"), PASS, false).unwrap_err().contains("no new entries"));
        let (r, _) = receive(&mut acct, None, &books, &second, PASS).unwrap();
        assert_eq!((r.added, r.records), (0, 1));
        assert!(desk::notes(&acct, None, None).unwrap()[0].done);
        let (r, _) = receive(&mut acct, None, &books, &second, PASS).unwrap();
        assert_eq!(r.records, 0, "the same file twice changes nothing");

        desk::delete_note(&mut acct, &note.id).unwrap();
        let back = out.join("back.khatapkg");
        assert_eq!(send(&acct, &back, PASS, false).unwrap().records, 1);
        receive(&mut owner, Some(&f.0), &books, &back, PASS).unwrap();
        assert!(desk::notes(&owner, None, None).unwrap().is_empty());
    }

    #[test]
    fn gaps_divergence_and_wrong_files_are_explained() {
        let f = TempFile::new("pkg-gap");
        let (mut owner, ..) = demo(&f);
        let out = temp_dir("gap-out");
        let books = temp_dir("gap-books");
        let first = out.join("1.khatapkg");
        send(&owner, &first, PASS, false).unwrap();
        let ids = |slot: &Slot, name: &str| {
            api::list_ledgers(slot, name, "all")
                .unwrap()
                .into_iter()
                .find(|l| l.name == name)
                .unwrap()
                .id
        };
        let (rent, bank) = (ids(&owner, "Office Rent"), ids(&owner, "HDFC Bank"));
        let pay = |slot: &mut Slot, amt: &str| {
            let (r, b) = (ids(slot, "Office Rent"), ids(slot, "HDFC Bank"));
            post_voucher(
                slot,
                voucher(
                    "Payment",
                    "2026-04-07",
                    vec![entry(&r, DrCr::Dr, amt), entry(&b, DrCr::Cr, amt)],
                ),
            )
            .unwrap();
        };
        let _ = (rent, bank);
        pay(&mut owner, "10");
        let second = out.join("2.khatapkg");
        send(&owner, &second, PASS, false).unwrap();
        pay(&mut owner, "20");
        let third = out.join("3.khatapkg");
        send(&owner, &third, PASS, false).unwrap();

        // Later changes with no company to apply them to.
        let mut none = None;
        assert!(
            receive(&mut none, None, &books, &third, PASS)
                .unwrap_err()
                .contains("full file")
        );

        let mut acct = None;
        let (_, p) = receive(&mut acct, None, &books, &first, PASS).unwrap();
        // Skipping file 2.
        assert!(
            receive(&mut acct, Some(&p), &books, &third, PASS)
                .unwrap_err()
                .contains("earlier file is missing")
        );
        // Both sides changed.
        pay(&mut acct, "30");
        assert!(
            receive(&mut acct, Some(&p), &books, &second, PASS)
                .unwrap_err()
                .contains("both sides changed")
        );
        // Not a package at all.
        let junk = out.join("junk.khatapkg");
        std::fs::write(&junk, b"hello").unwrap();
        assert!(
            inspect(&acct, &junk)
                .unwrap_err()
                .contains("isn't a KhataOS accountant file")
        );
    }
}

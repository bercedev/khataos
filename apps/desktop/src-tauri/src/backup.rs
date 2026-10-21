//! Backup and restore (Data menu, `Alt+Y`): verified `.khatabak` files
//! made by `crates/khata-backup`, and automatic backups.
//!
//! - **Backup** writes a consistent snapshot of the open company to the
//!   company's backup folder (by default `<Documents>/KhataOS Backups/<Company>/`,
//!   remembered per computer), optionally sealed with a password, and says
//!   it worked only after the file verified.
//! - **Restore** verifies a backup first, then makes it a new company file
//!   (the default), or replaces the open company: that needs the company
//!   name typed, keeps the replaced file as
//!   `<name>.before-restore-<time>.khata` and respects the company lock.
//! - **Automatic backups** run when the company closes and once a day while
//!   it is open, when the books changed since the last backup, keeping the
//!   newest N in the folder. One is also taken before every risky action:
//!   a Tally import into a company that has books, a restore that replaces,
//!   a sync rebase and moving to another relay.
//!
//! The settings and the last backup's details live in `backup.json` in the
//! app's data folder, per company file: they belong to this computer, and
//! nothing about backups is written into the company file.
//!
//! The company lock is held only while the snapshot is taken (`VACUUM INTO`
//! of the open file into a work folder); the backup is then written and
//! verified from that copy, so the window keeps working meanwhile.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Duration, Local, Utc};
use khata_backup::{BackupError, Summary};
use khata_core::UserRole;
use khata_store::Store;
use serde::{Deserialize, Serialize};

use crate::api::{self, ApiResult, CompanyLock, Slot, err};

/// Backup settings and the last backup, per company file, in app data.
pub const CONFIG_FILE: &str = "backup.json";
/// How many automatic backups a company keeps by default.
pub const DEFAULT_KEEP: u32 = 10;
/// The desk and home screen remind after this many days without a backup.
pub const REMIND_DAYS: i64 = 7;
const DAILY: Duration = Duration::hours(24);
/// Largest backup a phone copies in from a picked file.
pub const MAX_BACKUP_BYTES: u64 = 1024 * 1024 * 1024;

/// Where backups and work files go on this computer.
#[derive(Debug, Clone)]
pub struct Env {
    /// The app's data folder (`backup.json`).
    pub app_data: PathBuf,
    /// Default backup folders are made inside this, one per company.
    pub backups_root: PathBuf,
    /// Private work files (snapshots, decrypted copies): never a cloud folder.
    pub scratch: PathBuf,
}

/// A company's backup settings on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// `None`: the default folder.
    pub folder: Option<String>,
    /// Automatic backups on or off (backups before risky actions are always taken).
    pub automatic: bool,
    /// When the company closes.
    pub on_close: bool,
    /// Once a day while the company is open.
    pub daily: bool,
    /// Automatic backups kept in the folder; older ones are deleted.
    pub keep: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { folder: None, automatic: true, on_close: true, daily: true, keep: DEFAULT_KEEP }
    }
}

/// The last backup made on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Last {
    pub at: DateTime<Utc>,
    /// Where it was written (a path, or on a phone the place picked).
    pub path: String,
    pub automatic: bool,
    pub encrypted: bool,
    pub size: u64,
    pub entries: u64,
    pub last_hash: Option<String>,
    /// The company file's records revision when it was made.
    pub records_rev: u64,
    /// "manual", "close", "daily", or the risky action it came before.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CompanyEntry {
    #[serde(default)]
    prefs: Prefs,
    #[serde(default)]
    last: Option<Last>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    companies: BTreeMap<String, CompanyEntry>,
}

/// One writer of `backup.json` at a time in this process.
static CONFIG: Mutex<()> = Mutex::new(());

fn key(path: &Path) -> String {
    api::canonical(path).display().to_string()
}

fn load(env: &Env) -> Config {
    let file = env.app_data.join(CONFIG_FILE);
    match std::fs::read(&file) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            eprintln!("{} is unreadable ({e}); using the default backup settings", file.display());
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

fn save(env: &Env, config: &Config) -> ApiResult<()> {
    std::fs::create_dir_all(&env.app_data).map_err(err)?;
    let file = env.app_data.join(CONFIG_FILE);
    let tmp = env.app_data.join(format!("{CONFIG_FILE}.tmp"));
    let json = serde_json::to_vec_pretty(config).map_err(err)?;
    std::fs::write(&tmp, json).map_err(|e| format!("cannot save the backup settings: {e}"))?;
    std::fs::rename(&tmp, &file).map_err(|e| format!("cannot save the backup settings: {e}"))
}

fn entry(env: &Env, path: &Path) -> CompanyEntry {
    let _guard = CONFIG.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    load(env).companies.get(&key(path)).cloned().unwrap_or_default()
}

fn update(env: &Env, path: &Path, f: impl FnOnce(&mut CompanyEntry)) -> ApiResult<()> {
    let _guard = CONFIG.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut config = load(env);
    f(config.companies.entry(key(path)).or_default());
    save(env, &config)
}

/// A company name as a folder name: characters no file system takes are
/// replaced, and it is never empty.
pub fn folder_name(company: &str) -> String {
    let cleaned: String =
        company.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { ' ' } else { c }).collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let name: String = words.join(" ").trim_matches('.').trim().chars().take(80).collect();
    if name.is_empty() { "Company".into() } else { name }
}

/// The company's backup folder: the one picked, else
/// `<backups root>/<Company>`.
pub fn folder(env: &Env, prefs: &Prefs, company: &str) -> PathBuf {
    match prefs.folder.as_deref().filter(|f| !f.trim().is_empty()) {
        Some(f) => PathBuf::from(f),
        None => env.backups_root.join(folder_name(company)),
    }
}

/// What the books are now, to tell whether they changed since a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Live {
    entries: u64,
    last_hash: Option<String>,
    records_rev: u64,
}

fn live(store: &Store) -> ApiResult<Live> {
    Ok(Live { entries: store.len().map_err(err)?, last_hash: store.last_hash().map_err(err)?, records_rev: store.record_rev().map_err(err)? })
}

/// Whether the books or the shared records changed since `last` (with no
/// backup yet: whether there is anything to lose).
fn changed_since(last: Option<&Last>, live: &Live) -> bool {
    match last {
        None => live.entries > 0 || live.records_rev > 0,
        Some(l) => l.entries != live.entries || l.last_hash != live.last_hash || l.records_rev != live.records_rev,
    }
}

// ------------------------------------------------------------- status

/// The Data menu's and the home screen's view of backups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusDto {
    pub prefs: Prefs,
    /// The folder backups go to now.
    pub folder: String,
    pub default_folder: String,
    pub last: Option<Last>,
    /// The books changed since the last backup.
    pub changed_since: bool,
    /// No backup for [`REMIND_DAYS`] days (or never) and the books changed.
    pub remind: bool,
    /// The signed-in user may change the settings (Accountant or Owner).
    pub can_change: bool,
}

fn status_of(store: &Store, book: &khata_core::Book, env: &Env, path: &Path, now: DateTime<Utc>) -> ApiResult<StatusDto> {
    let e = entry(env, path);
    let live = live(store)?;
    let changed = changed_since(e.last.as_ref(), &live);
    let stale = e.last.as_ref().is_none_or(|l| now - l.at >= Duration::days(REMIND_DAYS));
    Ok(StatusDto {
        folder: folder(env, &e.prefs, &book.name).display().to_string(),
        default_folder: folder(env, &Prefs::default(), &book.name).display().to_string(),
        remind: changed && stale && live.entries > 0,
        changed_since: changed,
        can_change: api::require(book, UserRole::Accountant).is_ok(),
        prefs: e.prefs,
        last: e.last,
    })
}

pub fn status(slot: &Slot, env: &Env, path: &Path) -> ApiResult<StatusDto> {
    let book = api::open_ref(slot)?;
    let (store, _) = slot.as_ref().expect("checked");
    status_of(store, book, env, path, Utc::now())
}

/// The desk's backup reminder, when one is due.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReminderDto {
    /// The last backup, if there was one.
    pub last: Option<DateTime<Utc>>,
    /// Days since it.
    pub days: Option<i64>,
}

pub fn reminder(slot: &Slot, env: &Env, path: &Path, now: DateTime<Utc>) -> Option<ReminderDto> {
    let (store, book) = slot.as_ref()?;
    let s = status_of(store, book, env, path, now).ok()?;
    s.remind.then(|| ReminderDto { last: s.last.as_ref().map(|l| l.at), days: s.last.as_ref().map(|l| (now - l.at).num_days()) })
}

pub fn set_prefs(slot: &Slot, env: &Env, path: &Path, prefs: Prefs) -> ApiResult<StatusDto> {
    let book = api::open_ref(slot)?;
    api::require(book, UserRole::Accountant)?;
    if !(1..=100).contains(&prefs.keep) {
        return Err("keep between 1 and 100 automatic backups".into());
    }
    let mut prefs = prefs;
    prefs.folder = prefs.folder.map(|f| f.trim().to_owned()).filter(|f| !f.is_empty());
    if let Some(f) = &prefs.folder {
        let dir = Path::new(f);
        if !dir.is_absolute() {
            return Err(format!("choose a full folder path, not {f}"));
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot use the folder {f}: {e}"))?;
    }
    update(env, path, |e| e.prefs = prefs)?;
    status(slot, env, path)
}

// ------------------------------------------------------------- backing up

/// A backup that was written and verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Done {
    /// Where it is (a path, or on a phone the place picked).
    pub path: String,
    pub file: String,
    pub folder: String,
    pub size: u64,
    pub summary: Summary,
    /// Older automatic backups deleted to keep the newest N.
    pub deleted: Vec<String>,
}

/// Where a backup goes.
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    /// A new file in the company's backup folder.
    Folder,
    /// A file the user picked: written at `local`, shown as `shown` (a
    /// phone writes locally first, then copies to the picked place).
    File { local: &'a Path, shown: &'a str },
}

/// A snapshot of the open company in the work folder, taken under the
/// company lock; deleted when dropped.
struct Detached {
    path: PathBuf,
    live: Live,
    company: String,
}

impl Drop for Detached {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn detach(store: &Store, env: &Env) -> ApiResult<Detached> {
    std::fs::create_dir_all(&env.scratch).map_err(err)?;
    let path = env.scratch.join(format!("khataos-live-{}.tmp", uuid::Uuid::new_v4().simple()));
    let live = live(store)?;
    let company = store.company_name().map_err(err)?;
    store.snapshot_to(&path).map_err(|e| format!("cannot take a snapshot of the books: {e}"))?;
    Ok(Detached { path, live, company })
}

fn explain(e: BackupError) -> String {
    e.to_string()
}

/// Write and verify a backup from a detached snapshot, rotate automatic
/// backups, and remember it as the company's last backup.
fn finish(
    d: &Detached,
    env: &Env,
    company_path: &Path,
    target: Target<'_>,
    password: Option<&str>,
    automatic: bool,
    reason: &str,
) -> ApiResult<Done> {
    let prefs = entry(env, company_path).prefs;
    let dir = folder(env, &prefs, &d.company);
    let slug = khata_backup::slug(&d.company);
    let now = Utc::now();
    let (dest, shown) = match target {
        Target::Folder => {
            std::fs::create_dir_all(&dir).map_err(|e| format!("cannot use the backup folder {}: {e}", dir.display()))?;
            let p = khata_backup::next_path(&dir, &slug, now.with_timezone(&Local).naive_local(), automatic);
            let shown = p.display().to_string();
            (p, shown)
        }
        Target::File { local, shown } => (local.to_owned(), shown.to_owned()),
    };
    let source = Store::open(&d.path).map_err(err)?;
    let opts = khata_backup::Options { automatic, password, app_version: env!("CARGO_PKG_VERSION"), now, scratch: &env.scratch };
    let w = khata_backup::write(&source, &dest, &opts).map_err(explain)?;
    drop(source);
    let deleted = if automatic && matches!(target, Target::Folder) {
        khata_backup::rotate(&dir, &slug, prefs.keep as usize, Some(&dest))
            .map_err(|e| format!("the backup was saved, but older automatic backups couldn't be removed: {e}"))?
    } else {
        Vec::new()
    };
    // The backup holds exactly what the books were when the snapshot was taken.
    let last = Last {
        at: now,
        path: shown.clone(),
        automatic,
        encrypted: password.is_some(),
        size: w.size,
        entries: d.live.entries,
        last_hash: d.live.last_hash.clone(),
        records_rev: d.live.records_rev,
        reason: reason.to_owned(),
    };
    update(env, company_path, |e| e.last = Some(last))?;
    let (file, folder) = match target {
        Target::Folder => (dest.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(), dir.display().to_string()),
        Target::File { shown, .. } => (shown.rsplit(['/', '\\']).next().unwrap_or(shown).to_owned(), String::new()),
    };
    Ok(Done { path: shown, file, folder, size: w.size, summary: w.summary, deleted: deleted.iter().map(|p| p.display().to_string()).collect() })
}

/// Back up the open company now (Data → Backup).
pub fn back_up(slot: &Mutex<Slot>, env: &Env, path: &Path, target: Target<'_>, password: Option<&str>) -> ApiResult<Done> {
    let d = {
        let slot = slot.lock().map_err(|_| "company state is poisoned".to_owned())?;
        api::open_ref(&slot)?;
        detach(&slot.as_ref().expect("checked").0, env)?
    };
    finish(&d, env, path, target, password, false, "manual")
}

/// The automatic backup taken before a risky action, whatever the
/// automatic setting, when the books hold anything. The action must not
/// go ahead when this fails.
pub fn before_risky(slot: &Slot, env: &Env, path: &Path, what: &str) -> ApiResult<Option<Done>> {
    let Some((store, _)) = slot.as_ref() else { return Ok(None) };
    let now = live(store)?;
    if now.entries == 0 && now.records_rev == 0 {
        return Ok(None);
    }
    let fail = |e: String| format!("KhataOS couldn't back up the company first, so nothing was changed: {e}");
    let d = detach(store, env).map_err(fail)?;
    finish(&d, env, path, Target::Folder, None, true, what).map(Some).map_err(fail)
}

/// Tally import into the open company (at `path`): when it already has
/// books, an automatic backup is taken first, and the import doesn't start
/// if that fails. A new, empty company needs none.
pub fn import_tally(slot: &mut Slot, env: &Env, path: Option<&Path>, files: &[String]) -> ApiResult<(crate::export::ImportSummary, Option<Done>)> {
    api::open_mut(slot)?;
    let safety = match path {
        Some(path) => before_risky(slot, env, path, "Tally import")?,
        None => None,
    };
    crate::export::import_tally(slot, files).map(|s| (s, safety))
}

/// An automatic backup to run when a company closes.
#[derive(Debug, Clone)]
pub struct CloseJob {
    env: Env,
    path: PathBuf,
}

/// The backup due as the company in `slot` (at `path`) closes: automatic
/// backups on, "when the company closes" on, and the books changed.
pub fn due_on_close(slot: &Slot, env: &Env, path: &Path) -> Option<CloseJob> {
    let (store, _) = slot.as_ref()?;
    let e = entry(env, path);
    let due = e.prefs.automatic && e.prefs.on_close && changed_since(e.last.as_ref(), &live(store).ok()?);
    due.then(|| CloseJob { env: env.clone(), path: path.to_owned() })
}

impl CloseJob {
    /// Run after the company closed: the file is reopened read-only.
    pub fn run(self) -> ApiResult<Done> {
        let d = {
            let store = Store::open_read_only(&self.path).map_err(err)?;
            detach(&store, &self.env)?
        };
        finish(&d, &self.env, &self.path, Target::Folder, None, true, "close")
    }

    /// Run on a thread of its own, logging the outcome.
    pub fn spawn(self) {
        std::thread::spawn(move || log("on close", self.run()));
    }
}

/// The daily automatic backup, when due: on, the last one at least a day
/// old (or none), and the books changed since.
pub fn daily(slot: &Mutex<Slot>, env: &Env, path: &Path, now: DateTime<Utc>) -> ApiResult<Option<Done>> {
    let d = {
        let slot = slot.lock().map_err(|_| "company state is poisoned".to_owned())?;
        let Some((store, _)) = slot.as_ref() else { return Ok(None) };
        let e = entry(env, path);
        let old = e.last.as_ref().is_none_or(|l| now - l.at >= DAILY);
        if !(e.prefs.automatic && e.prefs.daily && old && changed_since(e.last.as_ref(), &live(store)?)) {
            return Ok(None);
        }
        detach(store, env)?
    };
    finish(&d, env, path, Target::Folder, None, true, "daily").map(Some)
}

fn log(what: &str, r: ApiResult<Done>) {
    match r {
        Ok(d) => eprintln!("automatic backup ({what}): {} ({} bytes)", d.path, d.size),
        Err(e) => eprintln!("automatic backup ({what}) failed: {e}"),
    }
}

// --------------------------------------------------------------- restore

/// How a backup relates to the open company.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    NoCompanyOpen,
    /// A backup of another company.
    OtherCompany,
    /// The same books, entry for entry.
    Same,
    /// The open books have later entries the backup lacks.
    Older,
    /// The backup has entries the open books lack.
    Newer,
    /// Both changed since they were last the same.
    Diverged,
}

/// What a backup holds, after it verified, before anything is restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub summary: Summary,
    pub file: String,
    pub size: u64,
    pub relation: Relation,
    /// Entries in the open books that the backup doesn't have.
    pub later_entries: u64,
    /// The open company can be replaced by this backup (same company,
    /// signed in as Owner, not a Viewer copy).
    pub can_replace: bool,
    /// The open company's name.
    pub open_company: Option<String>,
}

fn relation(slot: &Slot, backup: &khata_core::Book) -> (Relation, u64) {
    let Some((_, book)) = slot.as_ref() else { return (Relation::NoCompanyOpen, 0) };
    let ours = book.audit().entries();
    let theirs = backup.audit().entries();
    if ours.first().map(|e| &e.hash) != theirs.first().map(|e| &e.hash) {
        return (Relation::OtherCompany, 0);
    }
    let common = ours.len().min(theirs.len());
    if common > 0 && ours[common - 1].hash != theirs[common - 1].hash {
        return (Relation::Diverged, 0);
    }
    match ours.len().cmp(&theirs.len()) {
        std::cmp::Ordering::Equal => (Relation::Same, 0),
        std::cmp::Ordering::Greater => (Relation::Older, (ours.len() - theirs.len()) as u64),
        std::cmp::Ordering::Less => (Relation::Newer, 0),
    }
}

fn can_replace(slot: &Slot, relation: Relation) -> bool {
    let Some((store, book)) = slot.as_ref() else { return false };
    relation != Relation::OtherCompany
        && relation != Relation::NoCompanyOpen
        && crate::lan::check_writable(store).is_ok()
        && api::require(book, UserRole::Owner).is_ok()
}

fn open_backup(env: &Env, backup: &Path, password: Option<&str>) -> ApiResult<khata_backup::Opened> {
    khata_backup::open(backup, password.filter(|p| !p.is_empty()), &env.scratch).map_err(explain)
}

/// What a verified backup holds and how it relates to the open company.
fn preview(slot: &Slot, opened: khata_backup::Opened, shown: &str, size: u64) -> Preview {
    let (relation, later_entries) = relation(slot, &opened.book);
    Preview {
        file: shown.rsplit(['/', '\\']).next().unwrap_or(shown).to_owned(),
        size,
        can_replace: can_replace(slot, relation),
        open_company: slot.as_ref().map(|(_, b)| b.name.clone()),
        relation,
        later_entries,
        summary: opened.summary,
    }
}

/// Verify a backup and say what it holds (Data → Restore, first step).
pub fn inspect(slot: &Mutex<Slot>, env: &Env, backup: &Path, shown: &str, password: Option<&str>) -> ApiResult<Preview> {
    // Verify first, without the company lock; then compare.
    let opened = open_backup(env, backup, password)?;
    let size = std::fs::metadata(backup).map(|m| m.len()).unwrap_or(0);
    let slot = slot.lock().map_err(|_| "company state is poisoned".to_owned())?;
    Ok(preview(&slot, opened, shown, size))
}

/// Whether a backup needs a password.
pub fn encrypted(backup: &Path) -> ApiResult<bool> {
    Ok(khata_backup::kind(backup).map_err(explain)? == khata_backup::Kind::Sealed)
}

/// A free company file name in `dir` for a restored copy:
/// `<company>-restored.khata`, then `-restored-2`…
pub fn restored_path(dir: &Path, company: &str) -> PathBuf {
    let stem = khata_backup::slug(company);
    (1..)
        .map(|n| dir.join(if n == 1 { format!("{stem}-restored.khata") } else { format!("{stem}-restored-{n}.khata") }))
        .find(|p| !p.exists() && !khata_backup::partial_path(p).exists())
        .expect("a free name")
}

/// Restore a backup as a new company file in `dir` (never over anything).
/// Returns its path; the caller opens it.
pub fn restore_new(env: &Env, dir: &Path, backup: &Path, password: Option<&str>) -> ApiResult<PathBuf> {
    let opened = open_backup(env, backup, password)?;
    let dest = restored_path(dir, &opened.summary.company);
    opened.restore_to(&dest).map_err(explain)?;
    Ok(dest)
}

/// What a replacing restore did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replaced {
    pub info: api::CompanyInfo,
    /// The replaced company file, kept as `<name>.before-restore-<time>.khata`.
    pub kept: String,
    /// The automatic backup taken first.
    pub safety_backup: Option<String>,
}

/// `<stem>.before-restore-<YYYY-MM-DD_HHMMSS>.khata` beside `path`.
pub fn kept_path(path: &Path, at: chrono::NaiveDateTime) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "company".into());
    let base = format!("{stem}.before-restore-{}", at.format("%Y-%m-%d_%H%M%S"));
    let dir = path.parent().unwrap_or(Path::new(""));
    (1..)
        .map(|n| dir.join(if n == 1 { format!("{base}.khata") } else { format!("{base}-{n}.khata") }))
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Replace the open company (at `path`) with a backup of the same company.
///
/// Needs the company name typed as `confirm`, Owner rights and the
/// company's lock: refused while another KhataOS window or user holds it.
/// An automatic backup is taken first; the replaced file is kept beside it
/// as `<name>.before-restore-<time>.khata` (renamed, never overwritten);
/// on any failure it is put back and reopened.
pub fn restore_replace(
    slot: &mut Slot,
    held: &mut Option<CompanyLock>,
    env: &Env,
    path: &Path,
    backup: &Path,
    password: Option<&str>,
    confirm: &str,
) -> ApiResult<Replaced> {
    let (store, book) = api::open_mut(slot)?;
    api::require(book, UserRole::Owner)?;
    let _ = store;
    let name = book.name.clone();
    if !confirm.trim().eq_ignore_ascii_case(name.trim()) && confirm.trim() != name.trim() {
        return Err(format!("type the company name, {name}, to replace it"));
    }
    // This window's lock, or the lock taken now: never while another holds it.
    let ours = held.as_ref().is_some_and(|l| l.company() == api::canonical(path));
    let taken = if ours { None } else { Some(CompanyLock::acquire(path)?) };

    let opened = open_backup(env, backup, password)?;
    let (rel, _) = relation(slot, &opened.book);
    if rel == Relation::OtherCompany {
        return Err(format!(
            "this backup is of {}, not {name}: restore it as a new company file instead",
            opened.summary.company
        ));
    }
    let safety = before_risky(slot, env, path, "restore")?;

    // Close the books, then move the file aside: renamed, never deleted.
    *slot = None;
    let kept = kept_path(path, Local::now().naive_local());
    let reopen = |slot: &mut Slot| api::open_company(slot, path);
    if let Err(e) = std::fs::rename(path, &kept) {
        reopen(slot)?;
        return Err(format!("cannot move the current company file aside, so nothing was changed: {e}"));
    }
    if let Err(e) = opened.restore_to(path) {
        let back = std::fs::rename(&kept, path);
        reopen(slot)?;
        return Err(match back {
            Ok(()) => format!("{}; the company was left as it was", explain(e)),
            Err(b) => format!("{}; the previous company file is kept at {} ({b})", explain(e), kept.display()),
        });
    }
    let info = reopen(slot)?;
    if let Some(lock) = taken {
        *held = Some(lock);
    }
    Ok(Replaced { info, kept: kept.display().to_string(), safety_backup: safety.map(|d| d.path) })
}

// --------------------------------------------------------------- the app

/// Starts the daily check: every 10 minutes, a backup of the open company
/// when one is due ([`daily`]).
pub fn start_scheduler(app: tauri::AppHandle) {
    use tauri::Manager;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(90));
        loop {
            if let Ok(env) = cmd::env(&app) {
                let state = app.state::<crate::AppState>();
                let path = state.path.lock().ok().and_then(|p| p.clone());
                if let Some(path) = path {
                    match daily(&state.company, &env, &path, Utc::now()) {
                        Ok(Some(d)) => log("daily", Ok(d)),
                        Ok(None) => {}
                        Err(e) => log("daily", Err(e)),
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(600));
        }
    });
}

pub mod cmd {
    //! Tauri commands for the Data menu.
    use tauri::{AppHandle, Manager, State};

    use super::*;
    use crate::AppState;

    /// This device's folders. Desktop: backups in `<Documents>/KhataOS
    /// Backups`. Phones: in the app's own storage (uninstalling deletes
    /// it, so Backup there saves through the system's save dialog).
    pub fn env(app: &AppHandle) -> ApiResult<Env> {
        let app_data = app.path().app_data_dir().map_err(err)?;
        #[cfg(mobile)]
        let backups_root = app_data.join("backups");
        #[cfg(not(mobile))]
        let backups_root = app
            .path()
            .document_dir()
            .map(|d| d.join("KhataOS Backups"))
            .unwrap_or_else(|_| app_data.join("backups"));
        let scratch = app.path().app_cache_dir().map(|d| d.join("backup-work")).unwrap_or_else(|_| std::env::temp_dir().join("khataos-backup-work"));
        Ok(Env { app_data, backups_root, scratch })
    }

    fn open_path(state: &AppState) -> ApiResult<PathBuf> {
        state.path.lock().ok().and_then(|p| p.clone()).ok_or_else(|| "no company is open".to_owned())
    }

    fn slot(state: &AppState) -> std::sync::MutexGuard<'_, Slot> {
        state.company.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn blocking<T: Send + 'static>(f: impl FnOnce() -> ApiResult<T> + Send + 'static) -> ApiResult<T> {
        tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn backup_status(app: AppHandle, state: State<'_, AppState>) -> ApiResult<StatusDto> {
        let path = open_path(&state)?;
        status(&slot(&state), &env(&app)?, &path)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn backup_set_prefs(app: AppHandle, state: State<'_, AppState>, prefs: Prefs) -> ApiResult<StatusDto> {
        let path = open_path(&state)?;
        set_prefs(&slot(&state), &env(&app)?, &path, prefs)
    }

    /// Back up now: into the backup folder, or (`path`, from a save dialog;
    /// phones) to a picked file.
    #[tauri::command(rename_all = "snake_case")]
    pub async fn backup_now(app: AppHandle, path: Option<String>, password: Option<String>) -> ApiResult<Done> {
        blocking(move || {
            let env = env(&app)?;
            let state = app.state::<AppState>();
            let company = open_path(&state)?;
            let password = password.filter(|p| !p.is_empty());
            match path.filter(|p| !p.trim().is_empty()) {
                None => back_up(&state.company, &env, &company, Target::Folder, password.as_deref()),
                Some(picked) => {
                    let local = crate::phone_files::local_output(&app, &picked, khata_backup::EXTENSION)?;
                    let done = back_up(&state.company, &env, &company, Target::File { local: &local, shown: &picked }, password.as_deref())?;
                    crate::phone_files::finish_output(&app, &picked, &local)?;
                    Ok(done)
                }
            }
        })
        .await
    }

    fn local_backup(app: &AppHandle, path: &str) -> ApiResult<PathBuf> {
        crate::phone_files::local_input(app, path, &[khata_backup::EXTENSION], MAX_BACKUP_BYTES)
    }

    /// Whether the picked backup needs a password.
    #[tauri::command(rename_all = "snake_case")]
    pub async fn backup_encrypted(app: AppHandle, path: String) -> ApiResult<bool> {
        blocking(move || encrypted(&local_backup(&app, &path)?)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn backup_inspect(app: AppHandle, path: String, password: Option<String>) -> ApiResult<Preview> {
        blocking(move || {
            let env = env(&app)?;
            let local = local_backup(&app, &path)?;
            inspect(&app.state::<AppState>().company, &env, &local, &path, password.as_deref())
        })
        .await
    }

    /// What a restore did: a new company file to open, or the open company
    /// replaced (and reopened).
    #[derive(Debug, Clone, Serialize)]
    pub struct Restored {
        pub path: String,
        /// The replaced company, reopened; `None` for a new company file.
        pub info: Option<api::CompanyInfo>,
        pub kept: Option<String>,
        pub safety_backup: Option<String>,
    }

    /// `mode`: "new" (a new company file, the default) or "replace" (the
    /// open company; `confirm` is its name, typed).
    #[tauri::command(rename_all = "snake_case")]
    pub async fn backup_restore(
        app: AppHandle,
        path: String,
        password: Option<String>,
        mode: String,
        confirm: Option<String>,
    ) -> ApiResult<Restored> {
        blocking(move || {
            let env = env(&app)?;
            let local = local_backup(&app, &path)?;
            let state = app.state::<AppState>();
            if mode == "replace" {
                let company = open_path(&state)?;
                let mut slot = slot(&state);
                let mut held = state.company_lock.lock().map_err(|_| "company state is poisoned".to_owned())?;
                let r = restore_replace(&mut slot, &mut held, &env, &company, &local, password.as_deref(), confirm.as_deref().unwrap_or(""))?;
                crate::security::sign_out();
                return Ok(Restored { path: company.display().to_string(), info: Some(r.info), kept: Some(r.kept), safety_backup: r.safety_backup });
            }
            let dir = api::default_company_dir(&env.app_data);
            let made = restore_new(&env, &dir, &local, password.as_deref())?;
            Ok(Restored { path: made.display().to_string(), info: None, kept: None, safety_backup: None })
        })
        .await
    }
}

#[cfg(test)]
mod tests;

//! Backups of a company: `.khatabak` files.
//!
//! A backup is made while the company is open, from a consistent snapshot
//! (`VACUUM INTO`, never a raw copy of a live file), and is checked before
//! anyone is told it worked: it is opened read-only, its audit trail is
//! replayed and verified, and its entry count and last hash are compared
//! with the books it came from ([`write`]).
//!
//! **What is in a backup:** the company row, the whole hash-chained audit
//! trail (every master, voucher, user and setting of the books) and the
//! shared records (notes, invoice design, tax settings and authority
//! receipts; `docs/sync-settings.md`). **What is not:** the device-local
//! `settings` table (sync and relay credentials, cursors, a device's ZATCA
//! and VeriFactu chains) and the sync cursors. Credentials kept in app data
//! (IRP, PAC, ZATCA keys, MTD tokens) were never in the company file.
//!
//! **Format.** A `.khatabak` file is one of:
//!
//! - **plain:** an SQLite company file, as written by [`write`], with one
//!   extra table, `backup_info` (when it was made, entry count, last hash
//!   and a SHA-256 digest of the trail's hashes and the records). Renamed
//!   to `.khata`, a copy opens in any KhataOS: nothing is locked inside a
//!   format only this crate reads.
//! - **sealed** (a password was given): the same SQLite file, encrypted with
//!   `khata-seal` (Argon2id key from the password, XChaCha20-Poly1305) in
//!   1 MiB blocks. Each block is bound to the header, its position and
//!   whether it is the last, so blocks can't be dropped, reordered or cut
//!   off. The header holds only the format, salt and block size: not even
//!   the company name is readable without the password.
//!
//! Any change to the data a backup carries is caught: the trail by its hash
//! chain and `hash` column, the records by the digest, the file structure by
//! `PRAGMA integrity_check`, and every byte of a sealed backup by the
//! cipher's tag.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use khata_core::Book;
use khata_seal::CompanyKey;
use khata_store::{Store, StoreError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The backup file extension.
pub const EXTENSION: &str = "khatabak";
/// Shortest password for a sealed backup.
pub const MIN_PASSWORD: usize = 8;

const FORMAT: &str = "khatabak/1";
const SEALED_FORMAT: &str = "khatabak-sealed/1";
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
const SEALED_MAGIC: &[u8; 16] = b"KhataOS sealed\n\0";
const CHUNK: usize = 1 << 20;
const MAX_CHUNK: usize = 16 << 20;
const MAX_HEADER: u32 = 4096;
/// khata-seal's version byte, nonce and tag.
const SEAL_OVERHEAD: usize = 1 + 24 + 16;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("this isn't a KhataOS backup (.khatabak)")]
    NotABackup,
    #[error("this is a company file (.khata), not a backup: open it from the company list instead")]
    CompanyFile,
    #[error("this backup is protected by a password: enter it to open the backup")]
    NeedsPassword,
    #[error("wrong password, or the backup was changed or damaged after it was made")]
    WrongPassword,
    #[error("the backup is damaged: {0}. Nothing was taken from it; use another backup")]
    Damaged(String),
    #[error("the backup doesn't match the books it was made from ({0}), so it was not kept")]
    Mismatch(String),
    #[error("use a password of at least {MIN_PASSWORD} characters")]
    WeakPassword,
    #[error("{0}")]
    Io(String),
}

fn io(what: &str, path: &Path) -> impl FnOnce(io::Error) -> BackupError {
    let shown = format!("{what} {}", path.display());
    move |e| BackupError::Io(format!("cannot {shown}: {e}"))
}

fn store_err(e: StoreError) -> BackupError {
    BackupError::Io(e.to_string())
}

/// What a backup holds, read from the backup itself after it verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub company: String,
    /// Identity of the books: the hash of their first entry.
    pub company_id: Option<String>,
    /// Audit trail entries.
    pub entries: u64,
    pub last_hash: Option<String>,
    /// Shared records (notes, invoice design, tax settings), deletions included.
    pub records: u64,
    pub vouchers: u64,
    pub first_voucher: Option<NaiveDate>,
    pub last_voucher: Option<NaiveDate>,
    /// When the backup was made.
    pub made_at: DateTime<Utc>,
    /// Made by an automatic backup (on close, daily, before a risky action).
    pub automatic: bool,
    /// Sealed with a password.
    pub encrypted: bool,
    /// The KhataOS version that made it.
    pub app_version: String,
}

/// How a backup is made.
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    pub automatic: bool,
    /// Seal the backup with this password.
    pub password: Option<&'a str>,
    pub app_version: &'a str,
    pub now: DateTime<Utc>,
    /// A private folder for work files. Never the destination folder: a
    /// cloud-synced folder would upload half-made files.
    pub scratch: &'a Path,
}

/// A backup that was written and verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// Bytes on disk.
    pub size: u64,
    pub summary: Summary,
}

/// A work file, deleted when dropped.
struct Temp(PathBuf);

impl Temp {
    fn new(dir: &Path, what: &str) -> Self {
        Self(dir.join(format!("khataos-{what}-{}.tmp", uuid::Uuid::new_v4().simple())))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Where a file is written before it is renamed into place.
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".partial");
    PathBuf::from(p)
}

/// SHA-256 over the company name, every entry's hash and every record:
/// together with the hash chain, it covers all the data a backup carries.
fn digest(store: &Store) -> Result<String, StoreError> {
    let mut h = Sha256::new();
    h.update(b"khatabak-digest/1\0");
    h.update(store.company_name()?.as_bytes());
    h.update([0]);
    for hash in store.hashes()? {
        h.update(hash.as_bytes());
        h.update(b"\n");
    }
    h.update([0]);
    for r in store.all_records()? {
        h.update(serde_json::to_vec(&r)?);
        h.update(b"\n");
    }
    Ok(hex::encode(h.finalize()))
}

/// Write a verified backup of the company in `source` to `dest`, which
/// must not exist yet.
///
/// The snapshot is taken in `opts.scratch` and cleaned of device-local
/// data there; only the finished file goes to `dest`'s folder, first as
/// `<dest>.partial`, renamed into place once it verified. Verification
/// opens that file read-only (decrypting a sealed one with the password),
/// replays and verifies the trail, checks the digest, and compares the
/// entry count and last hash with `source`: at least every entry the books
/// had when the backup started, and exactly the trail `source` has up to
/// there. On any failure nothing is left at `dest`.
pub fn write(source: &Store, dest: &Path, opts: &Options<'_>) -> Result<Written, BackupError> {
    if opts.password.is_some_and(|p| p.chars().count() < MIN_PASSWORD) {
        return Err(BackupError::WeakPassword);
    }
    if dest.exists() {
        return Err(BackupError::Io(format!("{} already exists", dest.display())));
    }
    let had = source.len().map_err(store_err)?;
    fs::create_dir_all(opts.scratch).map_err(io("create", opts.scratch))?;
    if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(io("create the folder", dir))?;
    }

    let snapshot = Temp::new(opts.scratch, "snapshot");
    source.snapshot_to(&snapshot.0).map_err(|e| BackupError::Io(format!("cannot take a snapshot of the books: {e}")))?;
    {
        let snap = Store::open(&snapshot.0).map_err(store_err)?;
        let company = snap.company_name().map_err(store_err)?;
        let entries = snap.len().map_err(store_err)?.to_string();
        let last = snap.last_hash().map_err(store_err)?.unwrap_or_default();
        let records = snap.all_records().map_err(store_err)?.len().to_string();
        let digest = digest(&snap).map_err(store_err)?;
        let made_at = opts.now.to_rfc3339();
        snap.set_backup_info(&[
            ("format", FORMAT),
            ("company", &company),
            ("made_at", &made_at),
            ("entries", &entries),
            ("last_hash", &last),
            ("records", &records),
            ("automatic", if opts.automatic { "1" } else { "0" }),
            ("app_version", opts.app_version),
            ("digest", &digest),
        ])
        .map_err(store_err)?;
    }

    let sealed;
    let body = match opts.password {
        Some(password) => {
            sealed = Temp::new(opts.scratch, "sealed");
            seal_file(&snapshot.0, &sealed.0, password)?;
            &sealed.0
        }
        None => &snapshot.0,
    };

    let partial = partial_path(dest);
    let _ = fs::remove_file(&partial);
    let placed = (|| {
        copy_synced(body, &partial)?;
        let opened = open(&partial, opts.password, opts.scratch)?;
        let s = &opened.summary;
        if s.entries < had {
            return Err(BackupError::Mismatch(format!("it has {} entries, the books had {had}", s.entries)));
        }
        if s.entries > 0 && source.hash_at(s.entries - 1).map_err(store_err)? != s.last_hash {
            return Err(BackupError::Mismatch("its last entry is not the books' entry at that place".into()));
        }
        Ok(opened.summary.clone())
    })();
    let summary = match placed {
        Ok(s) => s,
        Err(e) => {
            let _ = fs::remove_file(&partial);
            return Err(e);
        }
    };
    if dest.exists() {
        let _ = fs::remove_file(&partial);
        return Err(BackupError::Io(format!("{} already exists", dest.display())));
    }
    fs::rename(&partial, dest).map_err(io("save", dest))?;
    let size = fs::metadata(dest).map_err(io("read", dest))?.len();
    Ok(Written { path: dest.to_owned(), size, summary })
}

fn copy_synced(from: &Path, to: &Path) -> Result<(), BackupError> {
    let mut r = File::open(from).map_err(io("read", from))?;
    let mut w = File::create_new(to).map_err(io("write", to))?;
    io::copy(&mut r, &mut w).map_err(io("write", to))?;
    w.sync_all().map_err(io("write", to))
}

/// Plain (an SQLite file) or sealed with a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Plain,
    Sealed,
}

/// Tell a plain backup from a sealed one by its first bytes.
pub fn kind(path: &Path) -> Result<Kind, BackupError> {
    let mut head = [0u8; 16];
    let mut f = File::open(path).map_err(io("open", path))?;
    let mut got = 0;
    while got < head.len() {
        match f.read(&mut head[got..]).map_err(io("read", path))? {
            0 => return Err(BackupError::NotABackup),
            n => got += n,
        }
    }
    match &head {
        SQLITE_MAGIC => Ok(Kind::Plain),
        SEALED_MAGIC => Ok(Kind::Sealed),
        _ => Err(BackupError::NotABackup),
    }
}

/// A backup that opened and verified: its summary, its books, and a plain
/// copy to restore from (a work file for a sealed backup, deleted on drop).
pub struct Opened {
    pub summary: Summary,
    pub book: Book,
    plain: PathBuf,
    _work: Option<Temp>,
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opened").field("summary", &self.summary).finish_non_exhaustive()
    }
}

/// Open and verify a backup: decrypt a sealed one with `password` (into
/// `scratch`), then check its structure, replay and verify its trail and
/// check its digest. Nothing is changed.
pub fn open(path: &Path, password: Option<&str>, scratch: &Path) -> Result<Opened, BackupError> {
    match kind(path)? {
        Kind::Plain => {
            let (summary, book) = verify_plain(path, false)?;
            Ok(Opened { summary, book, plain: path.to_owned(), _work: None })
        }
        Kind::Sealed => {
            let password = password.filter(|p| !p.is_empty()).ok_or(BackupError::NeedsPassword)?;
            fs::create_dir_all(scratch).map_err(io("create", scratch))?;
            let work = Temp::new(scratch, "opened");
            unseal_file(path, &work.0, password)?;
            let (summary, book) = verify_plain(&work.0, true)?;
            Ok(Opened { summary, book, plain: work.0.clone(), _work: Some(work) })
        }
    }
}

fn damaged(what: impl Into<String>) -> BackupError {
    BackupError::Damaged(what.into())
}

fn verify_plain(path: &Path, encrypted: bool) -> Result<(Summary, Book), BackupError> {
    let store = Store::open_read_only(path).map_err(|e| match e {
        StoreError::NotACompany => BackupError::NotABackup,
        other => damaged(format!("it can't be read as a database ({other})")),
    })?;
    store.integrity_check().map_err(|_| damaged("its database structure doesn't check out"))?;
    let info: BTreeMap<String, String> = store.backup_info().map_err(store_err)?.into_iter().collect();
    match info.get("format").map(String::as_str) {
        Some(FORMAT) => {}
        Some(_) => return Err(BackupError::Io("this backup was made by a newer KhataOS: update KhataOS to restore it".into())),
        None => return Err(BackupError::CompanyFile),
    }
    let field = |k: &str| info.get(k).cloned().ok_or_else(|| damaged(format!("its {k} is missing")));
    let book = store.load_verified().map_err(|e| match e {
        StoreError::Damaged(seq) => damaged(format!("entry {seq} of the audit trail doesn't check out")),
        StoreError::Book(b) => damaged(format!("the audit trail doesn't replay ({b})")),
        other => damaged(other.to_string()),
    })?;
    if digest(&store).map_err(store_err)? != field("digest")? {
        return Err(damaged("its notes and settings don't match the checksum taken when it was made"));
    }
    let entries = book.audit().entries().len() as u64;
    let last_hash = book.audit().entries().last().map(|e| e.hash.clone());
    if field("entries")? != entries.to_string() || field("last_hash")? != last_hash.clone().unwrap_or_default() {
        return Err(damaged("its audit trail is not the one it was made with"));
    }
    if field("company")? != book.name {
        return Err(damaged("its company name doesn't match"));
    }
    let made_at = DateTime::parse_from_rfc3339(&field("made_at")?).map_err(|_| damaged("its date is unreadable"))?.with_timezone(&Utc);
    let dates = || book.vouchers().map(|v| v.date);
    let summary = Summary {
        company: book.name.clone(),
        company_id: book.audit().entries().first().map(|e| e.hash.clone()),
        entries,
        last_hash,
        records: store.all_records().map_err(store_err)?.len() as u64,
        vouchers: book.vouchers().count() as u64,
        first_voucher: dates().min(),
        last_voucher: dates().max(),
        made_at,
        automatic: info.get("automatic").is_some_and(|v| v == "1"),
        encrypted,
        app_version: info.get("app_version").cloned().unwrap_or_default(),
    };
    Ok((summary, book))
}

impl Opened {
    /// Put the backup's books at `dest` (which must not exist) as an
    /// ordinary company file: written beside it as `<dest>.partial`,
    /// checked again, then renamed into place.
    pub fn restore_to(&self, dest: &Path) -> Result<(), BackupError> {
        if dest.exists() {
            return Err(BackupError::Io(format!("{} already exists", dest.display())));
        }
        if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(io("create the folder", dir))?;
        }
        let partial = partial_path(dest);
        let _ = fs::remove_file(&partial);
        let done = (|| {
            copy_synced(&self.plain, &partial)?;
            let store = Store::open(&partial).map_err(store_err)?;
            store.clear_backup_info().map_err(store_err)?;
            let book = store.load_verified().map_err(|e| damaged(e.to_string()))?;
            if book.audit().entries().last().map(|e| &e.hash) != self.summary.last_hash.as_ref() {
                return Err(damaged("the restored copy differs from the backup"));
            }
            Ok(())
        })();
        if let Err(e) = done {
            let _ = fs::remove_file(&partial);
            return Err(e);
        }
        fs::rename(&partial, dest).map_err(io("save", dest))
    }
}

// ------------------------------------------------------------- sealing

#[derive(Serialize, Deserialize)]
struct SealedHeader {
    format: String,
    /// Hex salt for the password key (Argon2id).
    salt: String,
    /// Plain bytes per block.
    chunk: u32,
}

/// The context a block is sealed with: the header, its index and whether
/// it is the last, so blocks can't be moved, dropped or cut off.
fn chunk_context(header: &[u8], index: u64, last: bool) -> Vec<u8> {
    let mut c = Vec::with_capacity(header.len() + 16);
    c.extend_from_slice(header);
    c.extend_from_slice(b"\0block");
    c.extend_from_slice(&index.to_be_bytes());
    c.push(u8::from(last));
    c
}

fn seal_file(plain: &Path, out: &Path, password: &str) -> Result<(), BackupError> {
    let salt = khata_seal::new_salt().map_err(|e| BackupError::Io(e.to_string()))?;
    let key = CompanyKey::from_passphrase(password, &salt).map_err(|e| BackupError::Io(e.to_string()))?;
    let header = serde_json::to_vec(&SealedHeader { format: SEALED_FORMAT.into(), salt: hex::encode(salt), chunk: CHUNK as u32 })
        .expect("header serializes");
    let total = fs::metadata(plain).map_err(io("read", plain))?.len();
    let mut r = BufReader::new(File::open(plain).map_err(io("read", plain))?);
    let file = File::create_new(out).map_err(io("write", out))?;
    let mut w = BufWriter::new(file);
    let werr = io("write", out);
    let result = (|| -> io::Result<()> {
        w.write_all(SEALED_MAGIC)?;
        w.write_all(&(header.len() as u32).to_be_bytes())?;
        w.write_all(&header)?;
        let mut buf = vec![0u8; CHUNK];
        let mut done = 0u64;
        for index in 0u64.. {
            let n = read_full(&mut r, &mut buf)?;
            done += n as u64;
            let last = n < CHUNK || done >= total;
            let sealed = khata_seal::seal(&key, &chunk_context(&header, index, last), &buf[..n]).map_err(io::Error::other)?;
            w.write_all(&(sealed.len() as u32).to_be_bytes())?;
            w.write_all(&sealed)?;
            if last {
                break;
            }
        }
        w.flush()?;
        w.get_ref().sync_all()
    })();
    result.map_err(werr)
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..])? {
            0 => break,
            n => got += n,
        }
    }
    Ok(got)
}

fn unseal_file(path: &Path, out: &Path, password: &str) -> Result<(), BackupError> {
    let cut = || damaged("it is cut short");
    let mut r = BufReader::new(File::open(path).map_err(io("open", path))?);
    let mut magic = [0u8; 16];
    r.read_exact(&mut magic).map_err(|_| BackupError::NotABackup)?;
    if &magic != SEALED_MAGIC {
        return Err(BackupError::NotABackup);
    }
    let mut len = [0u8; 4];
    r.read_exact(&mut len).map_err(|_| cut())?;
    let header_len = u32::from_be_bytes(len);
    if header_len == 0 || header_len > MAX_HEADER {
        return Err(damaged("its header is unreadable"));
    }
    let mut header = vec![0u8; header_len as usize];
    r.read_exact(&mut header).map_err(|_| cut())?;
    let h: SealedHeader = serde_json::from_slice(&header).map_err(|_| damaged("its header is unreadable"))?;
    if h.format != SEALED_FORMAT {
        return Err(BackupError::Io("this backup was made by a newer KhataOS: update KhataOS to restore it".into()));
    }
    let salt: [u8; 16] = hex::decode(&h.salt).ok().and_then(|s| s.try_into().ok()).ok_or_else(|| damaged("its header is unreadable"))?;
    let chunk = h.chunk as usize;
    if chunk == 0 || chunk > MAX_CHUNK {
        return Err(damaged("its header is unreadable"));
    }
    let key = CompanyKey::from_passphrase(password, &salt).map_err(|e| BackupError::Io(e.to_string()))?;
    let mut w = BufWriter::new(File::create_new(out).map_err(io("write", out))?);
    let mut sealed = Vec::with_capacity(chunk + SEAL_OVERHEAD);
    for index in 0u64.. {
        r.read_exact(&mut len).map_err(|_| cut())?;
        let n = u32::from_be_bytes(len) as usize;
        if !(SEAL_OVERHEAD..=chunk + SEAL_OVERHEAD).contains(&n) {
            return Err(damaged(format!("block {index} has a wrong length")));
        }
        sealed.resize(n, 0);
        r.read_exact(&mut sealed).map_err(|_| cut())?;
        let (plain, last) = match khata_seal::open(&key, &chunk_context(&header, index, false), &sealed) {
            Ok(p) => (p, false),
            Err(_) => match khata_seal::open(&key, &chunk_context(&header, index, true), &sealed) {
                Ok(p) => (p, true),
                Err(_) if index == 0 => return Err(BackupError::WrongPassword),
                Err(_) => return Err(damaged(format!("block {index} was changed or damaged"))),
            },
        };
        w.write_all(&plain).map_err(io("write", out))?;
        if last {
            let mut extra = [0u8; 1];
            if r.read(&mut extra).map_err(io("read", path))? != 0 {
                return Err(damaged("there are extra bytes after its end"));
            }
            break;
        }
    }
    w.flush().map_err(io("write", out))?;
    Ok(())
}

// ------------------------------------------------------ names, rotation

/// The file-name part for a company: "Sharma & Sons" → "sharma-sons".
pub fn slug(company: &str) -> String {
    let mut slug = String::new();
    for c in company.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { "company".to_owned() } else { slug.to_owned() }
}

/// `<slug>_<YYYY-MM-DD_HHMM>.khatabak`; an automatic backup ends in
/// `_auto.khatabak`, and a second backup in the same minute gets `-2`.
pub fn file_name(slug: &str, at: NaiveDateTime, automatic: bool, n: u32) -> String {
    let dup = if n > 1 { format!("-{n}") } else { String::new() };
    let auto = if automatic { "_auto" } else { "" };
    format!("{slug}_{}{dup}{auto}.{EXTENSION}", at.format("%Y-%m-%d_%H%M"))
}

/// The path for a new backup in `dir`: [`file_name`] with the next
/// same-minute number after any backup (or `.partial`) of that minute
/// already there, so a new backup always sorts after the older ones.
pub fn next_path(dir: &Path, slug: &str, at: NaiveDateTime, automatic: bool) -> PathBuf {
    let minute = NaiveDateTime::parse_from_str(&at.format("%Y-%m-%d_%H%M").to_string(), "%Y-%m-%d_%H%M").unwrap_or(at);
    let taken = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| {
                    let name = e.file_name().to_str()?.to_owned();
                    let name = name.strip_suffix(".partial").unwrap_or(&name);
                    parse_name(name, slug).filter(|(t, _, a)| *t == minute && *a == automatic).map(|(_, n, _)| n)
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    (taken + 1..)
        .map(|n| dir.join(file_name(slug, at, automatic, n)))
        .find(|p| !p.exists() && !partial_path(p).exists())
        .expect("a free name")
}

/// A backup file name of `slug` made by KhataOS, manual or automatic:
/// its minute, same-minute number and whether it is automatic.
pub fn parse_name(name: &str, slug: &str) -> Option<(NaiveDateTime, u32, bool)> {
    let rest = name.strip_prefix(slug)?.strip_prefix('_')?.strip_suffix(&format!(".{EXTENSION}"))?;
    let (rest, automatic) = match rest.strip_suffix("_auto") {
        Some(r) => (r, true),
        None => (rest, false),
    };
    if rest.len() < 15 || !rest.is_char_boundary(15) {
        return None;
    }
    let (stamp, tail) = rest.split_at(15);
    let at = NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H%M").ok()?;
    if at.format("%Y-%m-%d_%H%M").to_string() != stamp {
        return None;
    }
    let n = if tail.is_empty() {
        1
    } else {
        let digits = tail.strip_prefix('-')?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.starts_with('0') {
            return None;
        }
        let n: u32 = digits.parse().ok()?;
        if n < 2 {
            return None;
        }
        n
    };
    Some((at, n, automatic))
}

/// When `name` is an automatic backup of `slug` made by KhataOS, its time
/// and same-minute number. Anything else, however close, is `None`.
pub fn parse_automatic(name: &str, slug: &str) -> Option<(NaiveDateTime, u32)> {
    parse_name(name, slug).filter(|(_, _, automatic)| *automatic).map(|(at, n, _)| (at, n))
}

/// Delete the oldest automatic backups of `slug` in `dir`, keeping the
/// newest `keep` (at least one). `just_made`, the backup written a moment
/// ago, always stays, whatever the clock said. Only regular files whose
/// names match [`parse_automatic`] exactly are touched: manual backups,
/// other companies' backups and any other file stay. Returns what was
/// deleted.
pub fn rotate(dir: &Path, slug: &str, keep: usize, just_made: Option<&Path>) -> io::Result<Vec<PathBuf>> {
    let mut found: Vec<(bool, NaiveDateTime, u32, PathBuf)> = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        let Some(name) = e.file_name().to_str().map(str::to_owned) else { continue };
        let Some((at, n)) = parse_automatic(&name, slug) else { continue };
        // Not a symlink, not a folder.
        if fs::symlink_metadata(e.path()).map(|m| m.is_file()).unwrap_or(false) {
            let newest = just_made.is_some_and(|j| j.file_name() == Some(e.file_name().as_os_str()));
            found.push((newest, at, n, e.path()));
        }
    }
    found.sort_by_key(|a| std::cmp::Reverse((a.0, a.1, a.2)));
    let mut deleted = Vec::new();
    for (_, _, _, path) in found.into_iter().skip(keep.max(1)) {
        fs::remove_file(&path)?;
        deleted.push(path);
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests;

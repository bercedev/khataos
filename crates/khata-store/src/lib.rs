//! Local-first storage for a company's books.
//!
//! A company is one SQLite file. The only table that matters is `audit`:
//! every change to the books is an entry in the hash-chained audit trail,
//! and loading a company replays it ([`Book::replay`]). Triggers make the
//! table append-only at the database level, so the edit log cannot be
//! altered or switched off even by opening the file directly — the
//! "cannot be disabled" requirement of India's Companies (Accounts)
//! Rule 3(1) and GST Rule 56(8).
//!
//! Sync to a server later ships the same entries: appending entries with
//! `seq` greater than what the other side has.
//!
//! The company's non-accounting data (notes, invoice design, tax-scheme
//! settings…) is kept in a second table, `records`, and syncs as a
//! last-writer-wins channel beside the trail ([`records`]). The `settings`
//! table stays device-local and never syncs.

pub mod records;

use std::path::Path;

use khata_core::{AuditEntry, AuditLog, Book, BookError};
use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

pub use records::Record;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Book(#[from] BookError),
    #[error("stored trail diverges from the books at entry {0}")]
    Diverged(u64),
    #[error("not a KhataOS company file")]
    NotACompany,
    #[error("{0}")]
    Record(String),
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS company (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    name TEXT NOT NULL,
    schema_version INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS audit (
    seq   INTEGER PRIMARY KEY,
    hash  TEXT NOT NULL UNIQUE,
    entry TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_state (
    remote     TEXT PRIMARY KEY,
    company_id TEXT NOT NULL,
    synced_len INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS records (
    key     TEXT PRIMARY KEY,
    value   TEXT,
    at      INTEGER NOT NULL,
    author  TEXT NOT NULL,
    device  TEXT NOT NULL,
    forever INTEGER NOT NULL DEFAULT 0,
    rev     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS records_rev ON records (rev);
CREATE TABLE IF NOT EXISTS record_sync (
    remote     TEXT PRIMARY KEY,
    company_id TEXT NOT NULL,
    pulled     INTEGER NOT NULL,
    pushed     INTEGER NOT NULL,
    supported  INTEGER
);
CREATE TRIGGER IF NOT EXISTS audit_no_update BEFORE UPDATE ON audit
BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
CREATE TRIGGER IF NOT EXISTS audit_no_delete BEFORE DELETE ON audit
BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
";

/// 2: shared records moved out of `settings` into `records`.
const SCHEMA_VERSION: i64 = 2;
/// This file's random device id for the records it writes (device-local).
const DEVICE_KEY: &str = "records.device";

pub struct Store {
    conn: Connection,
}

/// How far the shared records are in step with one remote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordCursor {
    pub company_id: String,
    /// The remote's revision this file has pulled up to.
    pub pulled: u64,
    /// This file's revision it has pushed up to.
    pub pushed: u64,
    /// Whether the remote took shared records at the last sync (`None`:
    /// not tried yet). An older server doesn't.
    pub supported: Option<bool>,
}

fn row_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    Ok(Record {
        key: r.get(0)?,
        value: r.get(1)?,
        at: r.get(2)?,
        author: r.get(3)?,
        device: r.get(4)?,
        forever: r.get::<_, i64>(5)? != 0,
    })
}

const RECORD_COLUMNS: &str = "key, value, at, author, device, forever";

impl Store {
    /// Create a new company file.
    pub fn create(path: impl AsRef<Path>, company: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT INTO company (id, name, schema_version) VALUES (1, ?1, ?2)",
            params![company, SCHEMA_VERSION],
        )?;
        Ok(Self { conn })
    }

    /// Open an existing company file. A file from before shared records
    /// (schema 1) has its shared rows moved from `settings` to `records`
    /// once; the audit trail is not touched.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        let mut store = Self { conn };
        store.company_name()?;
        store.migrate()?;
        Ok(store)
    }

    /// Schema 1 → 2: move the [`records::SHARED_KEYS`] rows out of
    /// `settings`. They get clock 0, so any edit made after the upgrade
    /// wins over them; between two migrated copies the device id decides.
    fn migrate(&mut self) -> Result<(), StoreError> {
        let version: i64 = self.conn.query_row("SELECT schema_version FROM company WHERE id = 1", [], |r| r.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        let shared: Vec<(String, String)> = self.settings()?.into_iter().filter(|(k, _)| records::is_shared_key(k)).collect();
        let device = if shared.is_empty() { String::new() } else { self.device_id()? };
        let tx = self.conn.transaction()?;
        for (key, value) in &shared {
            let rev: i64 = tx.query_row("SELECT COALESCE(MAX(rev), 0) + 1 FROM records", [], |r| r.get(0))?;
            tx.execute(
                "INSERT OR IGNORE INTO records (key, value, at, author, device, forever, rev) VALUES (?1, ?2, 0, '', ?3, 0, ?4)",
                params![key, value, device, rev],
            )?;
            tx.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        }
        tx.execute("UPDATE company SET schema_version = ?1 WHERE id = 1", [SCHEMA_VERSION])?;
        tx.commit()?;
        Ok(())
    }

    pub fn company_name(&self) -> Result<String, StoreError> {
        self.conn
            .query_row("SELECT name FROM company WHERE id = 1", [], |r| r.get(0))
            .optional()?
            .ok_or(StoreError::NotACompany)
    }

    /// Number of audit entries stored. Sequence numbers run 0, 1, 2… with no
    /// gaps (`save` refuses anything else), so this is the last one plus one:
    /// a primary-key lookup instead of counting every row on each save.
    pub fn len(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(seq) + 1, 0) FROM audit", [], |r| r.get::<_, i64>(0))? as u64)
    }

    pub fn is_empty(&self) -> Result<bool, StoreError> {
        Ok(self.len()? == 0)
    }

    /// Load and verify the books by replaying the stored trail.
    pub fn load(&self) -> Result<Book, StoreError> {
        let mut stmt = self.conn.prepare("SELECT entry FROM audit ORDER BY seq")?;
        let entries = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str::<AuditEntry>(&row?)?))
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(Book::replay(self.company_name()?, AuditLog::from(entries))?)
    }

    /// How much of the trail is known to be on `remote`: `(company_id, synced_len)`.
    pub fn sync_state(&self, remote: &str) -> Result<Option<(String, u64)>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT company_id, synced_len FROM sync_state WHERE remote = ?1", [remote], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
            })
            .optional()?)
    }

    pub fn set_sync_state(&self, remote: &str, company_id: &str, synced_len: u64) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO sync_state (remote, company_id, synced_len) VALUES (?1, ?2, ?3)
             ON CONFLICT(remote) DO UPDATE SET company_id = excluded.company_id, synced_len = excluded.synced_len",
            params![remote, company_id, synced_len as i64],
        )?;
        Ok(())
    }

    /// A device-local setting (sync server, preferences). Settings are not
    /// part of the books or the audit trail and are never synced; the
    /// company's shared data is in [`Store::record`].
    pub fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    /// Set (or, with `None`, remove) a device-local setting.
    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<(), StoreError> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => self.conn.execute("DELETE FROM settings WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// Every setting, sorted by key (to carry settings over to a new file).
    pub fn settings(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>().map_err(Into::into)
    }

    // ------------------------------------------------------ shared records

    /// This file's random device id, made on first use (device-local).
    pub fn device_id(&self) -> Result<String, StoreError> {
        if let Some(id) = self.setting(DEVICE_KEY)? {
            return Ok(id);
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.set_setting(DEVICE_KEY, Some(&id))?;
        Ok(id)
    }

    /// The current value of a shared record (`None` when absent or deleted).
    pub fn record(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.record_version(key)?.and_then(|r| r.value))
    }

    /// The stored version of a key, tombstones included.
    pub fn record_version(&self, key: &str) -> Result<Option<Record>, StoreError> {
        version_in(&self.conn, key)
    }

    /// Live records whose key starts with `prefix`, sorted by key.
    pub fn records(&self, prefix: &str) -> Result<Vec<(String, String)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT key, value FROM records WHERE value IS NOT NULL AND substr(key, 1, length(?1)) = ?1 ORDER BY key",
        )?;
        stmt.query_map([prefix], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Set (or, with `None`, delete) a shared record as `author`. Refuses
    /// device-local keys, and a key deleted forever.
    pub fn set_record(&self, key: &str, value: Option<&str>, author: &str) -> Result<Record, StoreError> {
        self.write_record(key, value, false, author, records::now_ms())
    }

    /// [`Store::set_record`] at a given clock (tests, imports).
    pub fn set_record_at(&self, key: &str, value: Option<&str>, author: &str, now_ms: i64) -> Result<Record, StoreError> {
        self.write_record(key, value, false, author, now_ms)
    }

    /// Delete a record so that no later or concurrent edit brings it back
    /// (a note: an offline edit elsewhere loses to the deletion).
    pub fn delete_record_forever(&self, key: &str, author: &str) -> Result<Record, StoreError> {
        self.write_record(key, None, true, author, records::now_ms())
    }

    fn write_record(&self, key: &str, value: Option<&str>, forever: bool, author: &str, now_ms: i64) -> Result<Record, StoreError> {
        let old = self.record_version(key)?;
        if old.as_ref().is_some_and(|o| o.forever && o.value.is_none()) {
            if value.is_none() {
                return Ok(old.expect("checked"));
            }
            return Err(StoreError::Record(format!("{key} was deleted")));
        }
        // Nothing changes: no new version to sync (clearing what isn't
        // there, or saving the same value again).
        let same = match &old {
            Some(o) => o.value.as_deref() == value && o.forever == forever,
            None => value.is_none() && !forever,
        };
        if same {
            records::check_key(key).map_err(StoreError::Record)?;
            return Ok(old.unwrap_or_else(|| Record {
                key: key.to_owned(),
                value: None,
                at: 0,
                author: author.to_owned(),
                device: String::new(),
                forever: false,
            }));
        }
        let at = now_ms.max(old.as_ref().map_or(0, |o| o.at.saturating_add(1)));
        let rec = Record { key: key.to_owned(), value: value.map(str::to_owned), at, author: author.to_owned(), device: self.device_id()?, forever };
        rec.check().map_err(StoreError::Record)?;
        put_in(&self.conn, &rec)?;
        Ok(rec)
    }

    /// Merge versions from elsewhere (a sync, an accountant file): each
    /// replaces the stored one only if it [wins](Record::wins_over).
    /// All-or-nothing; refuses any record that fails [`Record::check`].
    /// Returns the versions that changed something here.
    pub fn merge_records(&mut self, incoming: &[Record]) -> Result<Vec<Record>, StoreError> {
        for r in incoming {
            r.check().map_err(StoreError::Record)?;
        }
        let tx = self.conn.transaction()?;
        let mut changed = Vec::new();
        for r in incoming {
            if version_in(&tx, &r.key)?.is_none_or(|o| r.wins_over(&o)) {
                put_in(&tx, r)?;
                changed.push(r.clone());
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// The latest revision of the records here (0 when there are none).
    pub fn record_rev(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(rev), 0) FROM records", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Versions changed after revision `rev`, oldest first, as
    /// `(revision, record)`: at most `limit` of them, and past the first
    /// only while their values stay within `max_bytes`. The flag says more
    /// follow.
    pub fn records_since(&self, rev: u64, limit: usize, max_bytes: usize) -> Result<(Vec<(u64, Record)>, bool), StoreError> {
        let mut stmt = self.conn.prepare(&format!("SELECT {RECORD_COLUMNS}, rev FROM records WHERE rev > ?1 ORDER BY rev"))?;
        let mut rows = stmt.query([rev as i64])?;
        let mut out = Vec::new();
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let r = row_record(row)?;
            let size = r.value.as_ref().map_or(0, String::len) + r.key.len() + 64;
            if out.len() >= limit || (!out.is_empty() && bytes + size > max_bytes) {
                return Ok((out, true));
            }
            bytes += size;
            out.push((row.get::<_, i64>(6)? as u64, r));
        }
        Ok((out, false))
    }

    /// Every record, tombstones included, sorted by key (to carry them to a
    /// new file, or into a full accountant file).
    pub fn all_records(&self) -> Result<Vec<Record>, StoreError> {
        let mut stmt = self.conn.prepare(&format!("SELECT {RECORD_COLUMNS} FROM records ORDER BY key"))?;
        stmt.query_map([], row_record)?.collect::<Result<_, _>>().map_err(Into::into)
    }

    /// How far the records are in step with `remote` (defaults when never synced).
    pub fn record_cursor(&self, remote: &str) -> Result<RecordCursor, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT company_id, pulled, pushed, supported FROM record_sync WHERE remote = ?1", [remote], |r| {
                Ok(RecordCursor {
                    company_id: r.get(0)?,
                    pulled: r.get::<_, i64>(1)? as u64,
                    pushed: r.get::<_, i64>(2)? as u64,
                    supported: r.get::<_, Option<i64>>(3)?.map(|v| v != 0),
                })
            })
            .optional()?
            .unwrap_or_default())
    }

    pub fn set_record_cursor(&self, remote: &str, c: &RecordCursor) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO record_sync (remote, company_id, pulled, pushed, supported) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(remote) DO UPDATE SET company_id = excluded.company_id, pulled = excluded.pulled,
                pushed = excluded.pushed, supported = excluded.supported",
            params![remote, c.company_id, c.pulled as i64, c.pushed as i64, c.supported.map(i64::from)],
        )?;
        Ok(())
    }

    /// Records changed here and not yet sent to `remote`.
    pub fn pending_records(&self, remote: &str) -> Result<u64, StoreError> {
        let pushed = self.record_cursor(remote)?.pushed;
        Ok(self.conn.query_row("SELECT COUNT(*) FROM records WHERE rev > ?1", [pushed as i64], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Entries with `seq` greater than or equal to `from`, in order.
    pub fn entries_from(&self, from: u64) -> Result<Vec<AuditEntry>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT entry FROM audit WHERE seq >= ?1 ORDER BY seq")?;
        stmt.query_map([from as i64], |r| r.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str::<AuditEntry>(&row?)?))
            .collect()
    }

    /// Append raw entries that continue the stored trail (used by sync).
    /// The caller has already verified them; this checks only continuity.
    pub fn append(&mut self, entries: &[AuditEntry]) -> Result<u64, StoreError> {
        let stored = self.len()?;
        let tx = self.conn.transaction()?;
        for (i, e) in entries.iter().enumerate() {
            if e.seq != stored + i as u64 {
                return Err(StoreError::Diverged(e.seq));
            }
            tx.execute(
                "INSERT INTO audit (seq, hash, entry) VALUES (?1, ?2, ?3)",
                params![e.seq as i64, e.hash, serde_json::to_string(e)?],
            )?;
        }
        tx.commit()?;
        Ok(entries.len() as u64)
    }

    /// Append every audit entry the store does not have yet, in one
    /// transaction. Refuses if the stored prefix differs from the books.
    pub fn save(&mut self, book: &Book) -> Result<u64, StoreError> {
        let stored = self.len()?;
        let entries = book.audit().entries();
        if stored > 0 {
            let last = (stored - 1) as usize;
            let hash: String =
                self.conn.query_row("SELECT hash FROM audit WHERE seq = ?1", [last as i64], |r| r.get(0))?;
            if entries.get(last).map(|e| &e.hash) != Some(&hash) {
                return Err(StoreError::Diverged(last as u64));
            }
        }
        let tx = self.conn.transaction()?;
        let mut added = 0;
        for e in entries.iter().skip(stored as usize) {
            tx.execute(
                "INSERT INTO audit (seq, hash, entry) VALUES (?1, ?2, ?3)",
                params![e.seq as i64, e.hash, serde_json::to_string(e)?],
            )?;
            added += 1;
        }
        tx.commit()?;
        Ok(added)
    }
}

fn version_in(conn: &Connection, key: &str) -> Result<Option<Record>, StoreError> {
    Ok(conn.query_row(&format!("SELECT {RECORD_COLUMNS} FROM records WHERE key = ?1"), [key], row_record).optional()?)
}

/// Store a version under the next revision.
fn put_in(conn: &Connection, r: &Record) -> Result<(), StoreError> {
    let rev: i64 = conn.query_row("SELECT COALESCE(MAX(rev), 0) + 1 FROM records", [], |row| row.get(0))?;
    conn.execute(
        "INSERT INTO records (key, value, at, author, device, forever, rev) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, at = excluded.at, author = excluded.author,
            device = excluded.device, forever = excluded.forever, rev = excluded.rev",
        params![r.key, r.value, r.at, r.author, r.device, i64::from(r.forever), rev],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};
    use khata_core::{Entry, Voucher, VoucherId, VoucherKind};
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_760_000_000, 0).unwrap()
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("khataos-test-{}-{name}.khata", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn sample() -> Book {
        let mut b = Book::new("Demo Traders");
        let bank = b.create_ledger(now(), "u", "HDFC Bank", b.group_by_name("Bank Accounts").unwrap().id).unwrap();
        let rent = b.create_ledger(now(), "u", "Rent", b.group_by_name("Indirect Expenses").unwrap().id).unwrap();
        b.set_opening_balance(now(), "u", bank, dec!(1000)).unwrap();
        b.post_voucher(
            now(),
            "u",
            Voucher {
                id: VoucherId::new(),
                kind: VoucherKind::Payment,
                number: "1".into(),
                date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                entries: vec![Entry::dr(rent, dec!(100)), Entry::cr(bank, dec!(100))],
                narration: "April rent".into(), cancelled: false, stock: Vec::new(),
            },
        )
        .unwrap();
        b
    }

    #[test]
    fn round_trips_books_through_sqlite() {
        let path = tmp("roundtrip");
        let book = sample();
        let mut store = Store::create(&path, &book.name).unwrap();
        assert_eq!(store.save(&book).unwrap(), 4);
        assert_eq!(store.save(&book).unwrap(), 0, "saving again appends nothing");
        drop(store);

        let loaded = Store::open(&path).unwrap().load().unwrap();
        assert_eq!(loaded.name, "Demo Traders");
        assert_eq!(loaded.trial_balance(), book.trial_balance());
        assert_eq!(loaded.audit().entries(), book.audit().entries());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn incremental_saves_append_only_new_entries() {
        let path = tmp("incremental");
        let mut book = sample();
        let mut store = Store::create(&path, &book.name).unwrap();
        store.save(&book).unwrap();
        let rent = book.ledger_by_name("Rent").unwrap().id;
        book.set_opening_balance(now(), "u", rent, dec!(5)).unwrap();
        assert_eq!(store.save(&book).unwrap(), 1);
        assert_eq!(store.len().unwrap(), 5);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn database_refuses_to_edit_or_delete_history() {
        let path = tmp("appendonly");
        let book = sample();
        let mut store = Store::create(&path, &book.name).unwrap();
        store.save(&book).unwrap();
        let upd = store.conn.execute("UPDATE audit SET entry = '{}' WHERE seq = 0", []);
        let del = store.conn.execute("DELETE FROM audit", []);
        assert!(upd.unwrap_err().to_string().contains("append-only"));
        assert!(del.unwrap_err().to_string().contains("append-only"));
        assert_eq!(store.len().unwrap(), 4);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_to_save_a_diverged_history() {
        let path = tmp("diverged");
        let mut store = Store::create(&path, "Demo Traders").unwrap();
        store.save(&sample()).unwrap();
        // A different company history with the same length must not be appended to.
        let other = sample();
        assert!(matches!(store.save(&other), Err(StoreError::Diverged(_))));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn settings_persist_outside_the_audit_trail() {
        let path = tmp("settings");
        let store = Store::create(&path, "Demo Traders").unwrap();
        assert_eq!(store.setting("sync.url").unwrap(), None);
        store.set_setting("sync.url", Some("https://books.example.in")).unwrap();
        store.set_setting("sync.url", Some("https://other.example.in")).unwrap();
        store.set_setting("sync.token", Some("secret")).unwrap();
        store.set_setting("sync.token", None).unwrap();
        assert_eq!(store.len().unwrap(), 0, "settings are not audit entries");
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.setting("sync.url").unwrap().as_deref(), Some("https://other.example.in"));
        assert_eq!(store.settings().unwrap(), [("sync.url".to_owned(), "https://other.example.in".to_owned())]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn records_last_writer_wins_and_a_note_deleted_forever_stays_deleted() {
        let path = tmp("records");
        let mut a = Store::create(&path, "Demo Traders").unwrap();
        let b_path = tmp("records-b");
        let mut b = Store::create(&b_path, "Demo Traders").unwrap();
        assert_ne!(a.device_id().unwrap(), b.device_id().unwrap());

        // A clock behind the stored version still moves forward.
        let v1 = a.set_record_at("desk.note.1", Some("first"), "asha", 5_000).unwrap();
        let v2 = a.set_record_at("desk.note.1", Some("second"), "asha", 10).unwrap();
        assert_eq!(v2.at, v1.at + 1);
        assert_eq!(a.record("desk.note.1").unwrap().as_deref(), Some("second"));
        assert_eq!(a.len().unwrap(), 0, "records are not audit entries");
        assert!(a.set_record("sync.token", Some("secret"), "asha").is_err(), "device-local keys never become records");

        // B edits the same note offline later, A deletes it forever: the deletion wins both ways.
        b.merge_records(&a.all_records().unwrap()).unwrap();
        let edit = b.set_record_at("desk.note.1", Some("edited offline"), "ravi", 9_000_000).unwrap();
        let gone = a.delete_record_forever("desk.note.1", "asha").unwrap();
        assert!(a.merge_records(std::slice::from_ref(&edit)).unwrap().is_empty());
        assert_eq!(b.merge_records(std::slice::from_ref(&gone)).unwrap().len(), 1);
        for s in [&a, &b] {
            assert_eq!(s.record("desk.note.1").unwrap(), None);
            assert!(s.set_record("desk.note.1", Some("again"), "x").is_err());
        }
        assert_eq!(a.all_records().unwrap(), b.all_records().unwrap());

        // A plain deletion (an image removed) can be set again later.
        a.set_record("invoice.logo", Some("png"), "asha").unwrap();
        a.set_record("invoice.logo", None, "asha").unwrap();
        assert_eq!(a.record("invoice.logo").unwrap(), None);
        a.set_record("invoice.logo", Some("png2"), "asha").unwrap();
        assert_eq!(a.records("invoice.").unwrap(), [("invoice.logo".to_owned(), "png2".to_owned())]);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(b_path).unwrap();
    }

    #[test]
    fn merging_converges_in_any_order() {
        let p1 = tmp("converge-1");
        let p2 = tmp("converge-2");
        let mut s1 = Store::create(&p1, "D").unwrap();
        let mut s2 = Store::create(&p2, "D").unwrap();
        let w = |key: &str, v: Option<&str>, at: i64, device: &str| Record { key: key.into(), value: v.map(Into::into), at, author: "u".into(), device: device.into(), forever: false };
        let writes = [w("desk.calendar", Some("a"), 5, "d1"), w("desk.calendar", Some("b"), 5, "d2"), w("desk.calendar", Some("c"), 4, "d3"), w("invoice.design", None, 7, "d1")];
        for x in &writes {
            s1.merge_records(std::slice::from_ref(x)).unwrap();
        }
        for x in writes.iter().rev() {
            s2.merge_records(std::slice::from_ref(x)).unwrap();
        }
        assert_eq!(s1.all_records().unwrap(), s2.all_records().unwrap());
        assert_eq!(s1.record("desk.calendar").unwrap().as_deref(), Some("b"));
        assert!(s1.merge_records(&[w("share.key", Some("k"), 1, "d")]).is_err());
        std::fs::remove_file(p1).unwrap();
        std::fs::remove_file(p2).unwrap();
    }

    #[test]
    fn revisions_page_and_cursors_count_what_is_pending() {
        let path = tmp("revs");
        let s = Store::create(&path, "D").unwrap();
        for i in 0..5 {
            s.set_record(&format!("desk.note.{i}"), Some(&"x".repeat(100)), "u").unwrap();
        }
        assert_eq!(s.record_rev().unwrap(), 5);
        let (page, more) = s.records_since(0, 3, usize::MAX).unwrap();
        assert_eq!((page.len(), more, page[0].0), (3, true, 1));
        let (page, more) = s.records_since(3, 10, 1).unwrap();
        assert_eq!((page.len(), more), (1, true), "at least one, then within the byte budget");
        assert_eq!(s.pending_records("https://r").unwrap(), 5);
        let c = RecordCursor { company_id: "c".into(), pulled: 9, pushed: 4, supported: Some(true) };
        s.set_record_cursor("https://r", &c).unwrap();
        assert_eq!(s.record_cursor("https://r").unwrap(), c);
        assert_eq!(s.pending_records("https://r").unwrap(), 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn an_older_company_file_moves_its_shared_settings_once_and_keeps_its_trail() {
        let path = tmp("migrate");
        let book = sample();
        let mut store = Store::create(&path, &book.name).unwrap();
        store.save(&book).unwrap();
        // What a file written by an older KhataOS holds.
        store.conn.execute("UPDATE company SET schema_version = 1", []).unwrap();
        store.conn.execute("DELETE FROM records", []).unwrap();
        for (k, v) in [("desk.note.0199", "{\"title\":\"Call CA\"}"), ("vat_gb.schemes", "{}"), ("sync.token", "secret"), ("share.key", "abcd"), ("zatca.chain", "{}")] {
            store.set_setting(k, Some(v)).unwrap();
        }
        let hashes: Vec<String> = store.entries_from(0).unwrap().into_iter().map(|e| e.hash).collect();
        drop(store);

        let store = Store::open(&path).unwrap();
        assert_eq!(store.record("desk.note.0199").unwrap().as_deref(), Some("{\"title\":\"Call CA\"}"));
        assert_eq!(store.record("vat_gb.schemes").unwrap().as_deref(), Some("{}"));
        assert_eq!(store.record_version("vat_gb.schemes").unwrap().unwrap().at, 0);
        let left: Vec<String> = store.settings().unwrap().into_iter().map(|(k, _)| k).collect();
        assert_eq!(left, ["records.device", "share.key", "sync.token", "zatca.chain"], "device-local rows stay settings");
        let loaded = store.load().unwrap();
        assert_eq!(loaded.audit().entries().iter().map(|e| e.hash.clone()).collect::<Vec<_>>(), hashes, "the trail is unchanged");
        assert!(loaded.audit().verify().is_ok());
        drop(store);
        // Opening again changes nothing.
        let store = Store::open(&path).unwrap();
        assert_eq!(store.record_rev().unwrap(), 2);
        std::fs::remove_file(path).unwrap();
    }
}

//! Sharing books without a server: encrypted update files.
//!
//! "Send to accountant" writes a `.khatapkg` file holding the audit entries
//! the accountant does not have yet, encrypted with a passphrase. It can go
//! over WhatsApp, e-mail or a cloud drive — none of them can read it. The
//! accountant opens it with the passphrase; KhataOS verifies every entry
//! with the same rules as local edits ([`Book::apply_entry`]) before
//! anything changes, so a damaged or forged file is rejected whole.
//!
//! Packages are incremental and idempotent: entries the receiver already
//! has are skipped, and a gap (an earlier package not yet applied) is
//! reported instead of guessed at.
//!
//! A package can also carry the company's **shared records** (notes, the
//! invoice design, tax settings…; `docs/sync-settings.md`), sealed in their
//! own `records` field with the same key. The header and the entries are
//! exactly as before, so an older KhataOS still opens the file and applies
//! the books (it skips the records it doesn't know). The receiver merges
//! records last writer wins, so applying a package twice changes nothing.

use khata_core::{AuditEntry, AuditLog, Book, BookError};
use khata_seal::{CompanyKey, SealError};
pub use khata_store::Record;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const FORMAT: &str = "khata-package/1";

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("not a KhataOS package file")]
    NotAPackage,
    #[error(transparent)]
    Seal(#[from] SealError),
    #[error("this package is for {theirs:?}, not this company ({ours:?})")]
    WrongCompany { ours: String, theirs: String },
    #[error("an earlier package is missing: this one starts at entry {from}, the books have {have}")]
    Gap { from: u64, have: u64 },
    #[error("these books and the package have both changed since they were last in step (from entry {at})")]
    Diverged { at: u64 },
    #[error("entry {seq} is not valid: {reason}")]
    Invalid { seq: u64, reason: BookError },
    #[error("the books have no entries to send")]
    Empty,
}

/// Readable without the passphrase, so the app can show what a file is
/// before asking for it. Also authenticated: it is the encryption context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub format: String,
    pub company: String,
    /// Identity of the books: hash of their first audit entry.
    pub company_id: String,
    pub from_seq: u64,
    pub to_seq: u64,
    /// Hex salt for the passphrase key.
    pub salt: String,
}

#[derive(Serialize, Deserialize)]
struct File {
    header: Header,
    /// Hex of the sealed JSON array of entries.
    body: String,
    /// Hex of the sealed JSON array of shared records; absent in packages
    /// from before shared records, and when there are none to send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    records: Option<String>,
}

/// The records' encryption context: the header's, plus a tag, so the
/// records can't stand in for the entries or another package's records.
fn records_context(header_context: &[u8]) -> Vec<u8> {
    let mut c = header_context.to_vec();
    c.extend_from_slice(b"\0records");
    c
}

/// What [`apply`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub added: u64,
    pub already_had: u64,
}

/// Identity of a company's books (hash of the first entry).
pub fn company_id(book: &Book) -> Option<&str> {
    book.audit().entries().first().map(|e| e.hash.as_str())
}

/// Package every entry from `from_seq` on, sealed with `passphrase`.
pub fn export(book: &Book, from_seq: u64, passphrase: &str) -> Result<Vec<u8>, PackageError> {
    export_with_records(book, from_seq, &[], passphrase)
}

/// [`export`], also carrying shared `records` (those failing
/// [`Record::check`], such as device-local keys, are left out).
pub fn export_with_records(book: &Book, from_seq: u64, records: &[Record], passphrase: &str) -> Result<Vec<u8>, PackageError> {
    let entries = book.audit().entries();
    let id = company_id(book).ok_or(PackageError::Empty)?.to_owned();
    let start = (from_seq as usize).min(entries.len());
    let salt = khata_seal::new_salt()?;
    let header = Header {
        format: FORMAT.into(),
        company: book.name.clone(),
        company_id: id,
        from_seq: start as u64,
        to_seq: entries.len() as u64,
        salt: hex::encode(salt),
    };
    let key = CompanyKey::from_passphrase(passphrase, &salt)?;
    let context = serde_json::to_vec(&header).expect("header serializes");
    let plain = serde_json::to_vec(&entries[start..]).expect("entries serialize");
    let sealed = khata_seal::seal(&key, &context, &plain)?;
    let shared: Vec<&Record> = records.iter().filter(|r| r.check().is_ok()).collect();
    let records = if shared.is_empty() {
        None
    } else {
        let plain = serde_json::to_vec(&shared).expect("records serialize");
        Some(hex::encode(khata_seal::seal(&key, &records_context(&context), &plain)?))
    };
    let file = File { header, body: hex::encode(sealed), records };
    Ok(serde_json::to_vec_pretty(&file).expect("file serializes"))
}

/// Read a package's header without the passphrase.
pub fn inspect(bytes: &[u8]) -> Result<Header, PackageError> {
    let file: File = serde_json::from_slice(bytes).map_err(|_| PackageError::NotAPackage)?;
    if file.header.format != FORMAT {
        return Err(PackageError::NotAPackage);
    }
    Ok(file.header)
}

/// Decrypt a package's entries.
pub fn open(bytes: &[u8], passphrase: &str) -> Result<(Header, Vec<AuditEntry>), PackageError> {
    open_with_records(bytes, passphrase).map(|(h, e, _)| (h, e))
}

/// Decrypt a package's entries and its shared records (none in older
/// packages). Records failing [`Record::check`] are dropped.
pub fn open_with_records(bytes: &[u8], passphrase: &str) -> Result<(Header, Vec<AuditEntry>, Vec<Record>), PackageError> {
    let file: File = serde_json::from_slice(bytes).map_err(|_| PackageError::NotAPackage)?;
    if file.header.format != FORMAT {
        return Err(PackageError::NotAPackage);
    }
    let salt: [u8; 16] = hex::decode(&file.header.salt)
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or(PackageError::NotAPackage)?;
    let key = CompanyKey::from_passphrase(passphrase, &salt)?;
    let sealed = hex::decode(&file.body).map_err(|_| PackageError::NotAPackage)?;
    let context = serde_json::to_vec(&file.header).expect("header serializes");
    let plain = khata_seal::open(&key, &context, &sealed)?;
    let entries: Vec<AuditEntry> = serde_json::from_slice(&plain).map_err(|_| PackageError::NotAPackage)?;
    let expected = file.header.to_seq.saturating_sub(file.header.from_seq);
    if entries.len() as u64 != expected || entries.first().is_some_and(|e| e.seq != file.header.from_seq) {
        return Err(PackageError::NotAPackage);
    }
    let records = match &file.records {
        None => Vec::new(),
        Some(hex_records) => {
            let sealed = hex::decode(hex_records).map_err(|_| PackageError::NotAPackage)?;
            let plain = khata_seal::open(&key, &records_context(&context), &sealed)?;
            let all: Vec<Record> = serde_json::from_slice(&plain).map_err(|_| PackageError::NotAPackage)?;
            all.into_iter().filter(|r| r.check().is_ok()).collect()
        }
    };
    Ok((file.header, entries, records))
}

/// Start a company from a package that begins at entry 0 (the first
/// package an accountant receives).
pub fn receive_new(bytes: &[u8], passphrase: &str) -> Result<Book, PackageError> {
    receive_new_with_records(bytes, passphrase).map(|(b, _)| b)
}

/// [`receive_new`], also returning the package's shared records for the
/// new company file.
pub fn receive_new_with_records(bytes: &[u8], passphrase: &str) -> Result<(Book, Vec<Record>), PackageError> {
    let (header, entries, records) = open_with_records(bytes, passphrase)?;
    if header.from_seq != 0 {
        return Err(PackageError::Gap { from: header.from_seq, have: 0 });
    }
    let book = Book::replay(header.company.clone(), AuditLog::from(entries)).map_err(|reason| PackageError::Invalid { seq: 0, reason })?;
    if company_id(&book) != Some(header.company_id.as_str()) {
        return Err(PackageError::NotAPackage);
    }
    Ok((book, records))
}

/// Apply a package to existing books. All-or-nothing: on any error `book`
/// is unchanged.
pub fn apply(book: &mut Book, bytes: &[u8], passphrase: &str) -> Result<Applied, PackageError> {
    apply_with_records(book, bytes, passphrase).map(|(a, _)| a)
}

/// [`apply`], also returning the package's shared records for the caller
/// to merge into the company file (`Store::merge_records`) once the books
/// are saved.
pub fn apply_with_records(book: &mut Book, bytes: &[u8], passphrase: &str) -> Result<(Applied, Vec<Record>), PackageError> {
    let (header, entries, records) = open_with_records(bytes, passphrase)?;
    let ours = company_id(book).unwrap_or_default().to_owned();
    if ours != header.company_id {
        return Err(PackageError::WrongCompany { ours: book.name.clone(), theirs: header.company.clone() });
    }
    let have = book.audit().entries().len() as u64;
    if header.from_seq > have {
        return Err(PackageError::Gap { from: header.from_seq, have });
    }
    let mut next = book.clone();
    let mut already_had = 0;
    for e in &entries {
        match next.audit().entries().get(e.seq as usize) {
            Some(existing) if existing == e => already_had += 1,
            Some(_) => return Err(PackageError::Diverged { at: e.seq }),
            None => next.apply_entry(e).map_err(|reason| PackageError::Invalid { seq: e.seq, reason })?,
        }
    }
    let added = next.audit().entries().len() as u64 - have;
    *book = next;
    Ok((Applied { added, already_had }, records))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};
    use khata_core::{AuditAction, Entry, Voucher, VoucherId, VoucherKind};
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_760_000_000, 0).unwrap()
    }

    fn pay(book: &mut Book, number: &str, amount: rust_decimal::Decimal) {
        let rent = book.ledger_by_name("Rent").unwrap().id;
        let bank = book.ledger_by_name("Bank").unwrap().id;
        book.post_voucher(
            now(),
            "owner",
            Voucher {
                id: VoucherId::new(),
                kind: VoucherKind::Payment,
                number: number.into(),
                date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                entries: vec![Entry::dr(rent, amount), Entry::cr(bank, amount)],
                narration: String::new(),
                cancelled: false, series: None, stock: Vec::new(),
            },
        )
        .unwrap();
    }

    fn books() -> Book {
        let mut b = Book::new("Shah & Sons");
        b.create_ledger(now(), "owner", "Bank", b.group_by_name("Bank Accounts").unwrap().id).unwrap();
        b.create_ledger(now(), "owner", "Rent", b.group_by_name("Indirect Expenses").unwrap().id).unwrap();
        pay(&mut b, "1", dec!(25000));
        b
    }

    #[test]
    fn first_package_creates_the_accountants_copy() {
        let owner = books();
        let pkg = export(&owner, 0, "pass phrase 1").unwrap();
        let h = inspect(&pkg).unwrap();
        assert_eq!((h.company.as_str(), h.from_seq, h.to_seq), ("Shah & Sons", 0, 3));
        let text = String::from_utf8_lossy(&pkg);
        assert!(!text.contains("Rent") && !text.contains("25000"), "contents are encrypted");

        let ca = receive_new(&pkg, "pass phrase 1").unwrap();
        assert_eq!(ca.trial_balance(), owner.trial_balance());
        assert!(matches!(receive_new(&pkg, "wrong"), Err(PackageError::Seal(SealError::Decrypt))));
    }

    #[test]
    fn incremental_packages_apply_in_order_and_are_idempotent() {
        let mut owner = books();
        let mut ca = receive_new(&export(&owner, 0, "p").unwrap(), "p").unwrap();
        pay(&mut owner, "2", dec!(10));
        pay(&mut owner, "3", dec!(20));
        let pkg = export(&owner, 3, "p").unwrap();
        assert_eq!(apply(&mut ca, &pkg, "p").unwrap(), Applied { added: 2, already_had: 0 });
        assert_eq!(apply(&mut ca, &pkg, "p").unwrap(), Applied { added: 0, already_had: 2 }, "re-applying is harmless");
        assert_eq!(ca.trial_balance(), owner.trial_balance());
        // Overlapping full export also fine.
        assert_eq!(apply(&mut ca, &export(&owner, 0, "p").unwrap(), "p").unwrap().added, 0);
    }

    #[test]
    fn gaps_other_companies_and_divergence_are_refused() {
        let mut owner = books();
        let mut ca = receive_new(&export(&owner, 0, "p").unwrap(), "p").unwrap();
        pay(&mut owner, "2", dec!(10));
        pay(&mut owner, "3", dec!(20));
        let later = export(&owner, 4, "p").unwrap();
        assert!(matches!(apply(&mut ca, &later, "p"), Err(PackageError::Gap { from: 4, have: 3 })));

        let other = books();
        let mut other_copy = receive_new(&export(&other, 0, "p").unwrap(), "p").unwrap();
        assert!(matches!(apply(&mut other_copy, &export(&owner, 3, "p").unwrap(), "p"), Err(PackageError::WrongCompany { .. })));

        pay(&mut ca, "9", dec!(1)); // the CA edited their copy too
        let before = ca.audit().entries().len();
        assert!(matches!(apply(&mut ca, &export(&owner, 3, "p").unwrap(), "p"), Err(PackageError::Diverged { at: 3 })));
        assert_eq!(ca.audit().entries().len(), before, "nothing applied");
    }

    #[test]
    fn forged_entries_are_rejected_whole() {
        let owner = books();
        let mut ca = receive_new(&export(&owner, 0, "p").unwrap(), "p").unwrap();
        // A sender with the passphrase but a tampered app: unbalanced voucher.
        let mut forged = owner.clone();
        let rent = forged.ledger_by_name("Rent").unwrap().id;
        let bank = forged.ledger_by_name("Bank").unwrap().id;
        let bad = Voucher {
            id: VoucherId::new(),
            kind: VoucherKind::Payment,
            number: "2".into(),
            date: NaiveDate::from_ymd_opt(2026, 4, 2).unwrap(),
            entries: vec![Entry::dr(rent, dec!(500)), Entry::cr(bank, dec!(5))],
            narration: String::new(),
            cancelled: false, series: None, stock: Vec::new(),
        };
        let mut log = forged.audit().clone();
        log.record(now(), "x", AuditAction::Create, format!("voucher:{}", bad.id.0), None, Some(serde_json::json!(bad)));
        // Build a package around the forged trail by hand.
        let salt = khata_seal::new_salt().unwrap();
        let header = Header {
            format: FORMAT.into(),
            company: forged.name.clone(),
            company_id: company_id(&forged).unwrap().to_owned(),
            from_seq: 3,
            to_seq: 4,
            salt: hex::encode(salt),
        };
        let key = CompanyKey::from_passphrase("p", &salt).unwrap();
        let sealed = khata_seal::seal(&key, &serde_json::to_vec(&header).unwrap(), &serde_json::to_vec(&log.entries()[3..]).unwrap()).unwrap();
        let bytes = serde_json::to_vec(&File { header, body: hex::encode(sealed), records: None }).unwrap();
        forged = ca.clone();
        assert!(matches!(apply(&mut ca, &bytes, "p"), Err(PackageError::Invalid { seq: 3, .. })));
        assert_eq!(ca.trial_balance(), forged.trial_balance());
    }

    #[test]
    fn header_tampering_breaks_decryption() {
        let owner = books();
        let pkg = export(&owner, 0, "p").unwrap();
        let mut file: serde_json::Value = serde_json::from_slice(&pkg).unwrap();
        file["header"]["company"] = "Someone Else".into();
        let tampered = serde_json::to_vec(&file).unwrap();
        assert!(matches!(open(&tampered, "p"), Err(PackageError::Seal(SealError::Decrypt))));
        assert!(matches!(inspect(b"{}"), Err(PackageError::NotAPackage)));
    }

    fn note(key: &str, value: &str, at: i64) -> Record {
        Record { key: key.into(), value: Some(value.into()), at, author: "owner".into(), device: "d1".into(), forever: false }
    }

    #[test]
    fn shared_records_travel_sealed_and_merge_idempotently() {
        let owner = books();
        let records = [
            note("desk.note.1", "Ask the CA about advance tax", 10),
            note("invoice.logo", "LOGO-BYTES", 11),
            note("sync.token", "SECRET-TOKEN", 12),
        ];
        let pkg = export_with_records(&owner, 0, &records, "p").unwrap();
        let text = String::from_utf8_lossy(&pkg);
        assert!(!text.contains("advance tax") && !text.contains("LOGO-BYTES") && !text.contains("desk.note"), "records are encrypted");
        assert!(!text.contains("SECRET-TOKEN"));
        let (book, got) = receive_new_with_records(&pkg, "p").unwrap();
        assert_eq!(book.trial_balance(), owner.trial_balance());
        assert_eq!(got, records[..2], "device-local keys never travel");
        let mut ca = book;
        let (a, again) = apply_with_records(&mut ca, &pkg, "p").unwrap();
        assert_eq!((a.added, again.len()), (0, 2), "the same records again: merging them changes nothing");
        // A package with records but no new entries.
        let pkg = export_with_records(&owner, 3, &[note("desk.note.1", "done", 20)], "p").unwrap();
        assert_eq!(inspect(&pkg).unwrap().to_seq, 3);
        assert_eq!(apply_with_records(&mut ca, &pkg, "p").unwrap().1.len(), 1);
    }

    #[test]
    fn older_and_newer_packages_open_both_ways() {
        let owner = books();
        // Without records the file is byte-for-byte the old format.
        let plain = export(&owner, 0, "p").unwrap();
        assert!(!String::from_utf8_lossy(&plain).contains("records"));
        assert_eq!(open_with_records(&plain, "p").unwrap().2, Vec::new());
        // An older KhataOS reads a newer file as it always did: header and
        // entries unchanged, the records field unknown to it and skipped.
        #[derive(Deserialize)]
        struct OldFile {
            header: Header,
            body: String,
        }
        let newer = export_with_records(&owner, 0, &[note("desk.note.1", "x", 1)], "p").unwrap();
        let old: OldFile = serde_json::from_slice(&newer).unwrap();
        assert_eq!(old.header.format, "khata-package/1");
        let salt: [u8; 16] = hex::decode(&old.header.salt).unwrap().try_into().unwrap();
        let key = CompanyKey::from_passphrase("p", &salt).unwrap();
        let plain = khata_seal::open(&key, &serde_json::to_vec(&old.header).unwrap(), &hex::decode(old.body).unwrap()).unwrap();
        let entries: Vec<AuditEntry> = serde_json::from_slice(&plain).unwrap();
        assert_eq!(entries, owner.audit().entries());
        // Tampering with the records is caught like tampering with entries.
        let mut file: serde_json::Value = serde_json::from_slice(&newer).unwrap();
        let mut r = file["records"].as_str().unwrap().to_owned();
        r.replace_range(r.len() - 2.., "00");
        file["records"] = r.into();
        assert!(matches!(open_with_records(&serde_json::to_vec(&file).unwrap(), "p"), Err(PackageError::Seal(_))));
    }
}

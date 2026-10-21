//! Shared records: the company's non-accounting data that syncs.
//!
//! Notes and to-dos, "filed" marks, the compliance calendar profile, the
//! invoice design and images, per-voucher print details, tax-scheme
//! settings and authority receipts belong to the company, not to one
//! computer, but they are not entries in the books. They live in the
//! company file's `records` table, next to (not in) the hash-chained audit
//! trail, and sync as a second channel: one row per key, **last writer
//! wins** (see `docs/sync-settings.md`).
//!
//! - Each write carries a clock (`at`, milliseconds), its author and the
//!   device it was made on. A write's clock is at least one more than the
//!   version it replaced, so an edit made after seeing someone's change
//!   always wins over it, whatever the computers' clocks say.
//! - A deletion is a tombstone. A deletion `forever` (a note) wins over any
//!   write, so an edit made offline elsewhere never brings a deleted note
//!   back.
//! - Merging takes the greater of two versions in one total order, so every
//!   copy that has seen the same writes holds the same rows, in any order.
//!
//! The device-local `settings` table (sync and relay credentials, cursors,
//! a device's e-invoicing chain…) is never part of this: [`is_device_local`]
//! keys are refused here, on every sync path and by the server.

use serde::{Deserialize, Serialize};

/// Largest value one record may hold: a 300 KB logo, its 1.2 MB PNG
/// rendition, both in base64, with room to spare.
pub const MAX_RECORD_VALUE: usize = 3 * 1024 * 1024;
/// Longest key.
pub const MAX_KEY_LEN: usize = 200;
const MAX_AUTHOR_LEN: usize = 200;
const MAX_DEVICE_LEN: usize = 64;
/// Clocks stay below 2^53 so they survive JSON in any language.
const MAX_AT: i64 = 1 << 53;

/// Keys (or key prefixes, ending in `.`) of the company data that syncs.
/// Existing company files keep these in `settings`; opening one moves them
/// into `records` once ([`crate::Store::open`]).
pub const SHARED_KEYS: &[&str] = &[
    // Accountant's desk (apps/desktop/src-tauri/src/desk.rs).
    "desk.note.",
    "desk.filed.",
    "desk.calendar",
    // Invoice design (invoice_design.rs).
    "invoice.design",
    "invoice.logo",
    "invoice.signature",
    "invoice.voucher.",
    // Tax settings and records of the company.
    "vat_gb.schemes",
    "gst.gstr1_filed.",
    "tds.tan",
    "tds.deductor",
    "tds.challan.",
    "us.exempt.",
    "us.use_tax.jurisdiction",
    "mx.emisor",
    "zatca.seller",
    // What a tax authority answered about the company's documents.
    "irp.irn.",
    "mx.cfdi.",
    "mtd.receipt.",
];

/// Keys (or prefixes) that belong to one computer and must never leave it:
/// sync and relay credentials and cursors, accountant-file cursors, a
/// device's ZATCA EGS unit (CSID, invoice chain, queue) and VeriFactu
/// installation chain, and the TDS file counter. Refused as records.
pub const DEVICE_LOCAL_KEYS: &[&str] = &[
    "sync.",
    "share.",
    "package.",
    "records.",
    "zatca.device",
    "zatca.chain",
    "zatca.doc.",
    "zatca.submission.",
    "zatca.queue",
    "verifactu.",
    "tds.file_sequence",
];

fn matches(key: &str, list: &[&str]) -> bool {
    list.iter().any(|p| if p.ends_with('.') { key.starts_with(p) } else { key == *p || key.starts_with(&format!("{p}.")) })
}

/// A key that syncs as a shared record.
pub fn is_shared_key(key: &str) -> bool {
    matches(key, SHARED_KEYS) && !is_device_local(key)
}

/// A key that must stay on this computer.
pub fn is_device_local(key: &str) -> bool {
    matches(key, DEVICE_LOCAL_KEYS)
}

/// One shared record: the latest version of a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub key: String,
    /// `None`: deleted (a tombstone).
    pub value: Option<String>,
    /// Last-writer-wins clock: milliseconds since 1970, at least one more
    /// than the version this one replaced.
    pub at: i64,
    /// Who made the change (the app's user name).
    pub author: String,
    /// The company file it was made in (a random id per file).
    pub device: String,
    /// A deletion no later write undoes (notes).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forever: bool,
}

impl Record {
    /// Total order of versions: a deletion forever first, then the clock,
    /// then the device, author and value as tie-breakers.
    fn rank(&self) -> (bool, i64, &str, &str, Option<&str>) {
        (self.forever && self.value.is_none(), self.at, &self.device, &self.author, self.value.as_deref())
    }

    /// Whether this version replaces `other` when the two meet.
    pub fn wins_over(&self, other: &Record) -> bool {
        self.rank() > other.rank()
    }

    /// Whether this is a tombstone.
    pub fn is_deleted(&self) -> bool {
        self.value.is_none()
    }

    /// Check a record before it is stored or sent: a well-formed key that
    /// is not device-local, sizes within limits, a sane clock.
    pub fn check(&self) -> Result<(), String> {
        check_key(&self.key)?;
        if self.value.as_ref().is_some_and(|v| v.len() > MAX_RECORD_VALUE) {
            return Err(format!("{}: the value is larger than {} MB", self.key, MAX_RECORD_VALUE / (1024 * 1024)));
        }
        if !(0..MAX_AT).contains(&self.at) {
            return Err(format!("{}: the clock is out of range", self.key));
        }
        if self.author.chars().count() > MAX_AUTHOR_LEN || self.device.len() > MAX_DEVICE_LEN {
            return Err(format!("{}: the author or device is too long", self.key));
        }
        if self.forever && self.value.is_some() {
            return Err(format!("{}: only a deletion can be forever", self.key));
        }
        Ok(())
    }
}

/// A key a record may have: 1 to 200 of `A-Z a-z 0-9 . _ : - #` (HMRC
/// period keys such as `#001` have a `#`), and not one of
/// [`DEVICE_LOCAL_KEYS`].
pub fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > MAX_KEY_LEN || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-#".contains(&b)) {
        return Err(format!("{key:?} is not a record key"));
    }
    if is_device_local(key) {
        return Err(format!("{key} belongs to one computer and is never shared"));
    }
    Ok(())
}

/// Milliseconds since 1970 by this computer's clock.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(key: &str, value: Option<&str>, at: i64, device: &str) -> Record {
        Record { key: key.into(), value: value.map(Into::into), at, author: "u".into(), device: device.into(), forever: false }
    }

    #[test]
    fn keys_are_classified() {
        for k in ["desk.note.0199", "desk.calendar", "invoice.logo", "invoice.voucher.x", "vat_gb.schemes", "irp.irn.x", "zatca.seller", "mtd.receipt.sandbox.#001"] {
            assert!(is_shared_key(k), "{k}");
            assert!(check_key(k).is_ok(), "{k}");
        }
        for k in ["sync.token", "sync.url", "share.key", "share.role", "package.sent_to", "zatca.device", "zatca.chain", "zatca.doc.x", "zatca.queue", "verifactu.chain", "verifactu.doc.x", "tds.file_sequence", "records.device"] {
            assert!(is_device_local(k), "{k}");
            assert!(!is_shared_key(k), "{k}");
            assert!(check_key(k).is_err(), "{k}");
        }
        assert!(!is_shared_key("desk.calendarx") && !is_shared_key("invoice.designs"));
        assert!(check_key("a/b").is_err() && check_key("").is_err() && check_key(&"x".repeat(201)).is_err());
    }

    #[test]
    fn the_order_is_total_and_a_deletion_forever_wins() {
        let a = rec("k", Some("a"), 10, "d1");
        let b = rec("k", Some("b"), 11, "d1");
        assert!(b.wins_over(&a) && !a.wins_over(&b));
        let tie = rec("k", Some("a"), 10, "d2");
        assert!(tie.wins_over(&a) ^ a.wins_over(&tie), "ties break by device");
        assert!(!a.wins_over(&a.clone()), "a version doesn't replace itself");
        let gone = Record { forever: true, ..rec("k", None, 1, "d1") };
        assert!(gone.wins_over(&b) && !b.wins_over(&gone), "a later offline edit doesn't undo the deletion");
        let plain_delete = rec("k", None, 12, "d1");
        assert!(plain_delete.wins_over(&b) && rec("k", Some("c"), 13, "d1").wins_over(&plain_delete));
    }

    #[test]
    fn records_are_checked() {
        assert!(rec("desk.note.1", Some("x"), 1, "d").check().is_ok());
        assert!(rec("sync.token", Some("x"), 1, "d").check().is_err());
        assert!(rec("desk.note.1", Some(&"x".repeat(MAX_RECORD_VALUE + 1)), 1, "d").check().is_err());
        assert!(rec("desk.note.1", Some("x"), -1, "d").check().is_err());
        assert!(rec("desk.note.1", Some("x"), MAX_AT, "d").check().is_err());
        assert!(Record { forever: true, ..rec("desk.note.1", Some("x"), 1, "d") }.check().is_err());
    }
}

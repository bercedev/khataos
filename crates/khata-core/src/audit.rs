//! Append-only, hash-chained audit trail.
//!
//! Every create/alter/delete is recorded with before/after snapshots. Each
//! entry's hash covers the previous entry's hash, so editing or removing any
//! past record breaks the chain. This satisfies "edit log that cannot be
//! disabled" rules such as India's Companies (Accounts) Rule 3(1), and is the
//! basis for tamper-evident sharing with external accountants.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    Create,
    Alter,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub user: String,
    pub action: AuditAction,
    /// e.g. "voucher:<uuid>" or "ledger:<uuid>".
    pub entity: String,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub prev_hash: String,
    pub hash: String,
}

const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

/// A trail read back from storage or the network, not yet checked (as when
/// deserialized): [`crate::Book::replay`] verifies every entry. Cheaper than
/// a round trip through `serde_json::Value`, which copies the whole trail.
impl From<Vec<AuditEntry>> for AuditLog {
    fn from(entries: Vec<AuditEntry>) -> Self {
        Self { entries }
    }
}

impl AuditLog {
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn record(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        action: AuditAction,
        entity: String,
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    ) -> &AuditEntry {
        let seq = self.entries.len() as u64;
        let prev_hash = self.entries.last().map_or(GENESIS.to_owned(), |e| e.hash.clone());
        let mut entry = AuditEntry {
            seq,
            at,
            user: user.to_owned(),
            action,
            entity,
            before,
            after,
            prev_hash,
            hash: String::new(),
        };
        entry.hash = entry_hash(&entry);
        self.entries.push(entry);
        self.entries.last().expect("just pushed")
    }

    /// Build the entry that would come next, without appending it.
    pub fn next_entry(
        &self,
        at: DateTime<Utc>,
        user: &str,
        action: AuditAction,
        entity: String,
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    ) -> AuditEntry {
        let seq = self.entries.len() as u64;
        let prev_hash = self.entries.last().map_or(GENESIS.to_owned(), |e| e.hash.clone());
        let mut entry = AuditEntry { seq, at, user: user.to_owned(), action, entity, before, after, prev_hash, hash: String::new() };
        entry.hash = entry_hash(&entry);
        entry
    }

    /// Check that `e` correctly continues this trail (seq, link, hash).
    pub fn check_next(&self, e: &AuditEntry) -> Result<(), u64> {
        let prev = self.entries.last().map_or(GENESIS, |l| l.hash.as_str());
        if e.seq != self.entries.len() as u64 || e.prev_hash != prev || e.hash != entry_hash(e) {
            return Err(e.seq);
        }
        Ok(())
    }

    /// Append an entry already validated with [`Self::check_next`].
    pub(crate) fn push_checked(&mut self, e: AuditEntry) {
        debug_assert_eq!(self.check_next(&e), Ok(()));
        self.entries.push(e);
    }

    /// Returns the sequence number of the first broken link, if any.
    pub fn verify(&self) -> Result<(), u64> {
        let mut prev = GENESIS.to_owned();
        for (i, e) in self.entries.iter().enumerate() {
            if e.seq != i as u64 || e.prev_hash != prev || e.hash != entry_hash(e) {
                return Err(i as u64);
            }
            prev = e.hash.clone();
        }
        Ok(())
    }
}

fn entry_hash(e: &AuditEntry) -> String {
    let body = serde_json::json!({
        "seq": e.seq,
        "at": e.at,
        "user": e.user,
        "action": e.action,
        "entity": e.entity,
        "before": e.before,
        "after": e.after,
        "prev_hash": e.prev_hash,
    });
    hex::encode(Sha256::digest(body.to_string().as_bytes()))
}

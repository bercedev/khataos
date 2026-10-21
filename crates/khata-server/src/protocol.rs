//! Request and response bodies, shared with clients.

use khata_core::AuditEntry;
use khata_store::Record;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Role;

/// The sync protocol this server speaks:
/// - 1: the audit trail (`push`, `pull`);
/// - 2: also the company's shared records (`records`): notes, the invoice
///   design and other non-accounting data, last writer wins.
///
/// Sent in pull answers; clients that predate it ignore the field, and a
/// client finding no `records` route (an older server) syncs the books only.
pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCompany {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompanyCreated {
    pub company_id: Uuid,
    /// Shown once; the server keeps only its hash.
    pub owner_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Push {
    /// Trail length the client built on. Must equal the server's length.
    pub base_len: u64,
    pub entries: Vec<AuditEntry>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pushed {
    pub len: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pulled {
    pub company: String,
    pub entries: Vec<AuditEntry>,
    /// More entries follow; pull again from `from + entries.len()`.
    #[serde(default)]
    pub more: bool,
    /// [`PROTOCOL_VERSION`]; absent (0) from servers before version 2.
    #[serde(default)]
    pub protocol: u32,
}

/// Shared records a device changed (`POST …/records`).
#[derive(Debug, Serialize, Deserialize)]
pub struct PushRecords {
    pub records: Vec<Record>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordsPushed {
    /// The server's latest record revision.
    pub rev: u64,
    /// Records that changed something on the server (the others were older
    /// than what it had).
    pub changed: u64,
    pub protocol: u32,
}

/// A record as the server holds it, with the revision it got there.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevRecord {
    pub rev: u64,
    #[serde(flatten)]
    pub record: Record,
}

/// `GET …/records?since=<rev>`: versions changed after `since`, oldest first.
#[derive(Debug, Serialize, Deserialize)]
pub struct PulledRecords {
    pub records: Vec<RevRecord>,
    /// The server's latest record revision.
    pub rev: u64,
    /// More follow; pull again from the last record's `rev`.
    #[serde(default)]
    pub more: bool,
    pub protocol: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateShare {
    pub role: Role,
    /// Who it is for, e.g. "Mehta & Co. (statutory auditor)".
    pub label: String,
    /// Days until the token stops working; `None` = no expiry.
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShareCreated {
    pub id: Uuid,
    /// Shown once; the server keeps only its hash.
    pub token: String,
}

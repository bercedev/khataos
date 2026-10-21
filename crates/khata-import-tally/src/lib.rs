//! Import from TallyPrime / Tally.ERP 9 XML.
//!
//! Reads the XML that Tally writes from Export → XML (Data Interchange) and
//! returns from its XML-over-HTTP server, and replays it into a [`Book`]:
//! groups, ledgers with opening balances, units, stock groups, stock items
//! with opening stock, godowns, and accounting vouchers with their stock
//! item lines by godown and batch (item invoices). This is
//! for interoperability and migration; it works from Tally's published XML
//! envelope only.
//!
//! Import is forgiving by design. A bad voucher (unknown ledger, unbalanced,
//! duplicate number, …) is recorded in [`ImportReport::skipped`] and the
//! import carries on; only input that is not a Tally envelope at all aborts
//! with an [`ImportError`]. Nothing is written to the book until the whole
//! document has parsed.
//!
//! Sign convention: Tally amounts are signed **negative = debit**
//! (`ISDEEMEDPOSITIVE` is `Yes` on those entries). KhataOS is debit
//! positive, so every amount is negated on the way in. The same holds for
//! stock lines: a line Tally signs negative (a debit, as on a purchase or a
//! sales return) brings stock in; a positive one (sales, purchase return)
//! sends it out. Opening stock values are made positive whatever their
//! sign.

mod apply;
pub mod parse;
pub mod request;
mod text;

#[cfg(feature = "http")]
pub mod http;

use chrono::{DateTime, Utc};
use khata_core::{Amount, Book, BookError};
use thiserror::Error;

pub use parse::{
    TallyBatch, TallyData, TallyEntry, TallyGodown, TallyGroup, TallyLedger, TallyStockGroup, TallyStockItem,
    TallyStockLine, TallyUnit, TallyVoucher, TallyVoucherType, parse,
};
pub use request::ExportRequest;

/// Why a whole document could not be imported.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("the input is empty")]
    Empty,
    #[error("not a Tally XML envelope (root element is <{0}>)")]
    NotTallyXml(String),
    /// `position` is a byte offset into the decoded, cleaned UTF-8 text.
    #[error("malformed XML at byte {position}: {message}")]
    Xml { position: u64, message: String },
    #[error("Tally reported an error: {0}")]
    TallyStatus(String),
}

/// What kind of Tally object a [`Skipped`] entry refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Group,
    Ledger,
    Voucher,
    Unit,
    StockGroup,
    StockItem,
    Godown,
    /// Stock detail of an object that *was* imported (a voucher posted
    /// without its stock lines, godown/batch detail merged, …).
    Inventory,
}

/// Why one object was left out of the import.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SkipReason {
    #[error("parent group {0:?} was not found")]
    UnknownParentGroup(String),
    #[error("custom primary groups are not supported; create it under a default group")]
    CustomPrimaryGroup,
    #[error("ledger has no parent group")]
    NoParentGroup,
    #[error("a ledger with this name already exists; kept the existing one")]
    DuplicateLedger,
    #[error("amount {0:?} is not a number")]
    InvalidAmount(String),
    #[error("date {0:?} is not a valid date")]
    InvalidDate(String),
    #[error("voucher has no date")]
    MissingDate,
    #[error("voucher type {0:?} is unknown")]
    UnknownVoucherType(String),
    #[error("voucher type {0:?} is not an accounting voucher KhataOS imports")]
    UnsupportedVoucherType(String),
    #[error("cancelled voucher")]
    Cancelled,
    #[error("optional voucher")]
    Optional,
    #[error("ledger {0:?} is not in the import or the book")]
    UnknownLedger(String),
    #[error("voucher has no ledger amounts")]
    NoEntries,
    #[error("debits ({dr}) do not equal credits ({cr})")]
    Unbalanced { dr: Amount, cr: Amount },
    #[error("voucher number {0:?} is already used for this voucher type")]
    DuplicateNumber(String),
    #[error("contra vouchers may only use cash and bank ledgers")]
    ContraNonCash,
    #[error("compound units aren't imported yet; only simple units are")]
    CompoundUnit,
    #[error("its unit {0:?} is a compound unit, which isn't imported yet")]
    CompoundItemUnit(String),
    #[error("stock item has no unit of measure")]
    NoUnit,
    #[error("parent stock group {0:?} was not found")]
    UnknownParentStockGroup(String),
    #[error("stock group parents form a loop through {0:?}")]
    StockGroupCycle(String),
    #[error("a stock item with this name already exists; kept the existing one")]
    DuplicateStockItem,
    #[error("opening quantity {0} is negative; the item was created without opening stock")]
    NegativeOpeningStock(String),
    #[error("{0} vouchers aren't imported yet; stock moves only through Sales, Purchase, Credit Note and Debit Note")]
    InventoryVoucherType(String),
    #[error("posted without its stock lines: {0}")]
    StockLinesDropped(String),
    #[error("godown parents form a loop through {0:?}")]
    GodownCycle(String),
    #[error("parent godown {0:?} was not found")]
    UnknownParentGodown(String),
    #[error("opening stock by godown and batch was merged into one opening balance at the main location")]
    OpeningByGodownMerged,
    #[error("batch allocations that did not add up to their line were merged into it at the main location")]
    BatchDetailMerged,
    #[error(transparent)]
    Rejected(BookError),
}

/// One object that was not imported, and why.
#[derive(Debug, PartialEq, Eq)]
pub struct Skipped {
    pub kind: ItemKind,
    /// Group, ledger, unit or stock name; for vouchers "type number on date".
    pub name: String,
    pub reason: SkipReason,
}

/// Outcome of an import.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Custom groups created in the book.
    pub groups_created: usize,
    /// Groups matched to a default group or one already in the book.
    pub groups_matched: usize,
    pub ledgers_created: usize,
    /// Ledgers given a non-zero opening balance.
    pub opening_balances: usize,
    pub vouchers_posted: usize,
    /// Vouchers posted without a Tally number, numbered by KhataOS.
    pub vouchers_numbered: usize,
    pub units_created: usize,
    pub stock_groups_created: usize,
    pub stock_items_created: usize,
    pub godowns_created: usize,
    /// Stock item lines posted on vouchers.
    pub inventory_lines: usize,
    pub skipped: Vec<Skipped>,
}

impl ImportReport {
    /// Skipped objects of one kind.
    pub fn skipped_of(&self, kind: ItemKind) -> impl Iterator<Item = &Skipped> {
        self.skipped.iter().filter(move |s| s.kind == kind)
    }
}

/// Parse Tally XML (UTF-8 or UTF-16) and import it into `into`.
pub fn import(xml: &[u8], into: &mut Book, at: DateTime<Utc>, user: &str) -> Result<ImportReport, ImportError> {
    import_all([xml], into, at, user)
}

/// Import several documents as one, e.g. a masters export and a Day Book
/// export. Voucher types and ledger aliases from one document then apply to
/// vouchers in another; KhataOS keeps neither, so importing the files one
/// by one would lose them. Every document is parsed before the book is
/// touched.
pub fn import_all<'a>(
    docs: impl IntoIterator<Item = &'a [u8]>,
    into: &mut Book,
    at: DateTime<Utc>,
    user: &str,
) -> Result<ImportReport, ImportError> {
    let mut data = TallyData::default();
    for xml in docs {
        data.extend(parse(xml)?);
    }
    Ok(import_data(data, into, at, user))
}

/// Import already-parsed data: masters first (groups in dependency order,
/// then ledgers and opening balances, units, stock groups, stock items),
/// then vouchers.
pub fn import_data(data: TallyData, into: &mut Book, at: DateTime<Utc>, user: &str) -> ImportReport {
    apply::apply(data, into, at, user)
}

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
//! Bill-wise details come too: bill allocations on voucher lines and the
//! opening bills of party ledgers, so outstandings and ageing match Tally.
//! So do party GSTINs, states and PANs, the tax of GST duty ledgers, HSN/SAC
//! codes and their rates, and bank reconciliation dates. Each custom
//! voucher type ("Sales GST", "Cash Sales") keeps its name as the voucher's
//! number series, so types that Tally numbers on their own don't clash.
//!
//! Import is forgiving by design. A bad voucher (unknown ledger, unbalanced,
//! duplicate number, …) is recorded in [`ImportReport::skipped`] and the
//! import carries on; anything imported with a caveat is a [`Note`], and
//! checks of the result (the Day Book's dates, the opening difference,
//! closing balances against Tally's) are in the report too. Only input that
//! is not a Tally envelope at all aborts with an [`ImportError`]. Nothing is
//! written to the book until every document has parsed.
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
mod report;
pub mod request;
mod text;

#[cfg(feature = "http")]
pub mod http;

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{Amount, Book, BookError};
use thiserror::Error;

pub use parse::{
    TallyBalance, TallyBatch, TallyBill, TallyData, TallyEntry, TallyGodown, TallyGroup, TallyGst, TallyLedger,
    TallyParty, TallyStockGroup, TallyStockItem, TallyStockLine, TallyUnit, TallyVoucher, TallyVoucherType, parse,
};
pub use report::{BalanceCheck, BalanceDifference, BillCounts, GstCounts, Note, NoteKind, TypeCount, voucher_kind_label};
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
    #[error("the export marks it as deleted")]
    Deleted,
    #[error("ledger {0:?} is not in the import or the book")]
    UnknownLedger(String),
    #[error("voucher has no ledger amounts")]
    NoEntries,
    #[error("debits ({dr}) do not equal credits ({cr})")]
    Unbalanced { dr: Amount, cr: Amount },
    #[error("voucher number {0:?} is already used for this voucher type")]
    DuplicateNumber(String),
    #[error("voucher number {0:?} of this type is already in the company, from before this import; it looks imported already")]
    AlreadyInBooks(String),
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

impl SkipReason {
    /// The reason without names or amounts, for a shareable summary.
    pub fn summary(&self) -> &'static str {
        match self {
            Self::UnknownParentGroup(_) => "parent group not found",
            Self::CustomPrimaryGroup => "custom primary group",
            Self::NoParentGroup => "no parent group",
            Self::DuplicateLedger => "ledger already exists",
            Self::InvalidAmount(_) => "amount is not a number",
            Self::InvalidDate(_) => "date is not valid",
            Self::MissingDate => "no date",
            Self::UnknownVoucherType(_) => "voucher type unknown",
            Self::UnsupportedVoucherType(_) => "voucher type not imported",
            Self::Cancelled => "cancelled",
            Self::Optional => "optional voucher",
            Self::Deleted => "marked as deleted",
            Self::UnknownLedger(_) => "ledger not found",
            Self::NoEntries => "no ledger amounts",
            Self::Unbalanced { .. } => "debits and credits differ",
            Self::DuplicateNumber(_) => "voucher number already used",
            Self::AlreadyInBooks(_) => "voucher already in the company",
            Self::ContraNonCash => "contra with a ledger that is not cash or bank",
            Self::CompoundUnit => "compound unit",
            Self::CompoundItemUnit(_) => "item with a compound unit",
            Self::NoUnit => "no unit",
            Self::UnknownParentStockGroup(_) => "stock group not found",
            Self::StockGroupCycle(_) => "stock groups form a loop",
            Self::DuplicateStockItem => "stock item already exists",
            Self::NegativeOpeningStock(_) => "negative opening stock",
            Self::InventoryVoucherType(_) => "stock voucher type not imported yet",
            Self::StockLinesDropped(_) => "posted without stock lines",
            Self::GodownCycle(_) => "godowns form a loop",
            Self::UnknownParentGodown(_) => "parent godown not found",
            Self::OpeningByGodownMerged => "opening stock by godown merged",
            Self::BatchDetailMerged => "batch detail merged",
            Self::Rejected(_) => "refused by the books",
        }
    }
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
    /// Vouchers posted as cancelled, keeping their numbers (not counted in
    /// `vouchers_posted`).
    pub vouchers_cancelled: usize,
    /// Vouchers by Tally voucher type name, as written in the files.
    pub by_type: BTreeMap<String, TypeCount>,
    pub bills: BillCounts,
    pub gst: GstCounts,
    /// First and last date of the vouchers posted.
    pub first_date: Option<NaiveDate>,
    pub last_date: Option<NaiveDate>,
    /// [`Book::opening_difference`] after the import: zero when the opening
    /// balances tally.
    pub opening_difference: Amount,
    /// Closing balances compared with those in the files, when the files
    /// carry any (a Trial Balance exported as XML, or `CLOSINGBALANCE` on
    /// ledger masters).
    pub balances: BalanceCheck,
    /// Things imported with a caveat, and checks to look at.
    pub notes: Vec<Note>,
    pub skipped: Vec<Skipped>,
}

impl ImportReport {
    /// Skipped objects of one kind.
    pub fn skipped_of(&self, kind: ItemKind) -> impl Iterator<Item = &Skipped> {
        self.skipped.iter().filter(move |s| s.kind == kind)
    }

    /// Notes of one kind.
    pub fn notes_of(&self, kind: NoteKind) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(move |n| n.kind == kind)
    }

    /// Whether anything suggests balances may differ from Tally's.
    pub fn balance_warnings(&self) -> usize {
        self.notes.iter().filter(|n| n.kind.affects_balances()).count()
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

//! Country-neutral accounting core.
//!
//! Nothing in this crate knows about a specific country's tax law. Country
//! rules live in compliance packs (see `khata-compliance`) so the same engine
//! runs on desktop, server and mobile for every jurisdiction.

pub mod analysis;
pub mod audit;
pub mod banking;
pub mod book;
pub mod chart;
pub mod forex;
pub mod format;
pub mod inventory;
pub mod numbering;
mod number_index;
pub mod profile;
pub mod reports;
pub mod voucher;
pub mod words;

pub use audit::{AuditAction, AuditEntry, AuditLog};
pub use book::{Book, BookError, TrialBalanceRow};
pub use forex::{Currency, Forex};
pub use chart::{DeducteeKind, Group, GroupId, Ledger, LedgerId, LedgerTax, LedgerTds, Mailing, Nature, OpeningBill, TaxSide, TdsTerms};
pub use inventory::{Godown, GodownId, Quantity, StockGroup, StockGroupId, StockItem, StockItemId, StockLine, Unit, UnitId};
pub use numbering::{NumberSeries, Restart};
pub use profile::{CompanyUser, Profile, UserRole};
pub use voucher::{BillAllocation, BillKind, DrCr, Entry, StockMove, Voucher, VoucherError, VoucherId, VoucherKind};

/// Monetary amounts are always exact decimals, never floats.
pub type Amount = rust_decimal::Decimal;

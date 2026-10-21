//! A company's books: chart of accounts, vouchers and the audit trail.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use thiserror::Error;

use crate::audit::{AuditAction, AuditEntry, AuditLog};
use crate::chart::{DEFAULT_ALIASES, DEFAULT_GROUPS, Group, GroupId, Ledger, LedgerId, Nature};
use crate::inventory::{Godown, GodownId, MAX_BATCH, StockGroup, StockGroupId, StockItem, StockItemId, Unit, UnitId};
use crate::number_index::{NumberIndex, numbering_basis};
use crate::numbering::NumberSeries;
use crate::profile::{CompanyUser, PROFILE_ENTITY, Profile, UserRole};
use crate::voucher::{Voucher, VoucherError, VoucherId, VoucherKind};
use crate::Amount;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BookError {
    #[error(transparent)]
    Voucher(#[from] VoucherError),
    #[error("unknown group")]
    UnknownGroup,
    #[error("unknown ledger {0:?}")]
    UnknownLedger(LedgerId),
    #[error("unknown voucher")]
    UnknownVoucher,
    #[error("a ledger named {0:?} already exists")]
    DuplicateLedger(String),
    #[error("a group named {0:?} already exists")]
    DuplicateGroup(String),
    #[error("voucher number {0:?} is already used for this voucher type")]
    DuplicateVoucherNumber(String),
    #[error("contra vouchers may only use cash and bank ledgers")]
    ContraNonCashLedger,
    #[error("ledger is used by vouchers and cannot be deleted")]
    LedgerInUse,
    #[error("audit trail is broken at entry {0}")]
    BrokenAuditTrail(u64),
    #[error("audit entry {0} cannot be replayed")]
    UnreplayableEntry(u64),
    #[error("entry {seq}: {reason}")]
    InvalidEntry { seq: u64, reason: Box<BookError> },
    #[error("audit entry {0} names a different record than it contains")]
    EntityMismatch(u64),
    #[error("audit entry {0} does not match the current state of the record")]
    StaleBefore(u64),
    #[error("audit entry {0} creates a record that already exists")]
    AlreadyExists(u64),
    #[error("audit entry {0} creates a group that does not inherit its parent's behaviour")]
    GroupNotInherited(u64),
    #[error("group has sub-groups or ledgers, or is a primary group, and cannot be deleted")]
    GroupInUse,
    #[error("a cancelled voucher cannot be altered")]
    VoucherCancelled,
    #[error("{0:?} is not an ISO 3166-1 alpha-2 country code")]
    InvalidCountry(String),
    #[error("a group cannot be moved under itself or under a group of a different kind")]
    InvalidGroupParent,
    #[error("a unit {0:?} already exists")]
    DuplicateUnit(String),
    #[error("a stock group or item named {0:?} already exists")]
    DuplicateStockName(String),
    #[error("unknown unit")]
    UnknownUnit,
    #[error("unknown stock group")]
    UnknownStockGroup,
    #[error("unknown stock item")]
    UnknownStockItem,
    #[error("in use by stock items or vouchers, so it cannot be deleted")]
    StockInUse,
    #[error("stock items can only be on Sales, Purchase, Credit Note and Debit Note vouchers")]
    ItemsNotAllowed,
    #[error("quantity {0} has more decimal places than its unit allows")]
    QuantityPrecision(String),
    #[error("a bank date can only be set on cash and bank ledgers, on or after the voucher date")]
    InvalidBankDate,
    #[error("invalid numbering: {0}")]
    InvalidNumbering(String),
    #[error("unknown godown")]
    UnknownGodown,
    #[error("a godown named {0:?} already exists")]
    DuplicateGodown(String),
    #[error("batch names can have at most 40 characters")]
    InvalidBatch,
    #[error("invalid tax setting: {0}")]
    InvalidTax(String),
    #[error("{0}")]
    InvalidUsers(String),
    #[error("invalid statement setting: {0}")]
    InvalidStatementSetting(String),
    #[error("{0}")]
    Currency(String),
    #[error("an opening bill needs a name of at most 100 characters, an amount, and a due date on or after its date")]
    InvalidOpeningBill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrialBalanceRow {
    pub ledger: LedgerId,
    pub name: String,
    pub debit: Amount,
    pub credit: Amount,
}

#[derive(Debug, Default, Clone)]
pub struct Book {
    pub name: String,
    groups: BTreeMap<GroupId, Group>,
    ledgers: BTreeMap<LedgerId, Ledger>,
    vouchers: BTreeMap<VoucherId, Voucher>,
    profile: Option<Profile>,
    units: BTreeMap<UnitId, Unit>,
    stock_groups: BTreeMap<StockGroupId, StockGroup>,
    stock_items: BTreeMap<StockItemId, StockItem>,
    godowns: BTreeMap<GodownId, Godown>,
    audit: AuditLog,
    /// Voucher numbers by series period: derived from `vouchers` and the
    /// numbering settings, never stored.
    numbers: NumberIndex,
}

impl Book {
    /// A new company seeded with the default primary groups.
    pub fn new(name: impl Into<String>) -> Self {
        let mut book = Self { name: name.into(), ..Self::default() };
        for &(name, nature, affects_gross_profit, is_cash_or_bank, parent) in DEFAULT_GROUPS {
            let parent = parent.map(|p| book.group_by_name(p).expect("parents listed first").id);
            let id = GroupId::default_named(name);
            let aliases =
                DEFAULT_ALIASES.iter().filter(|(g, _)| *g == name).map(|(_, a)| (*a).to_owned()).collect();
            book.groups.insert(
                id,
                Group { id, name: name.to_owned(), parent, nature, aliases, affects_gross_profit, is_cash_or_bank },
            );
        }
        book
    }

    /// Rebuild a company by replaying its audit trail. The trail is the
    /// storage format: every state change is in it, hash-chained. Each entry
    /// is checked with exactly the rules local edits obey ([`Self::apply_entry`]),
    /// so a trail from another device or a server cannot smuggle in state that
    /// this app would never have produced.
    pub fn replay(name: impl Into<String>, log: AuditLog) -> Result<Self, BookError> {
        let mut book = Self::new(name);
        for e in log.entries() {
            book.apply_entry(e)
                .map_err(|reason| BookError::InvalidEntry { seq: e.seq, reason: Box::new(reason) })?;
        }
        Ok(book)
    }

    /// Validate an audit entry against the current state and, if it is a
    /// legal next step, apply it and append it to the trail. This is the
    /// single gate for every change: local edits, replay, rebase and sync.
    ///
    /// Checks: hash-chain link and hash; `entity` names the payload's id;
    /// Create targets a new id, Alter/Delete an existing record whose
    /// current state equals `before`; references exist; names and voucher
    /// numbers are unique; vouchers obey the double-entry rules; nothing
    /// in use is deleted.
    pub fn apply_entry(&mut self, e: &AuditEntry) -> Result<(), BookError> {
        self.audit.check_next(e).map_err(BookError::BrokenAuditTrail)?;
        let bad = || BookError::UnreplayableEntry(e.seq);
        let (kind, id) = e.entity.split_once(':').ok_or_else(bad)?;
        let payload = |v: &Option<serde_json::Value>| v.clone().ok_or_else(bad);
        let check_id = |payload_id: String| if payload_id == id { Ok(()) } else { Err(BookError::EntityMismatch(e.seq)) };
        let check_before = |current: serde_json::Value| {
            if e.before.as_ref() == Some(&current) { Ok(()) } else { Err(BookError::StaleBefore(e.seq)) }
        };
        let no_after = || if e.after.is_none() { Ok(()) } else { Err(bad()) };
        match (kind, e.action) {
            ("group", AuditAction::Create) => {
                if e.before.is_some() {
                    return Err(bad());
                }
                let g: Group = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                if self.groups.contains_key(&g.id) {
                    return Err(BookError::AlreadyExists(e.seq));
                }
                // Custom groups always hang off an existing group and inherit
                // its behaviour; parents must exist first, so no cycles.
                let parent = g.parent.and_then(|p| self.groups.get(&p)).ok_or(BookError::UnknownGroup)?;
                if (g.nature, g.affects_gross_profit, g.is_cash_or_bank)
                    != (parent.nature, parent.affects_gross_profit, parent.is_cash_or_bank)
                {
                    return Err(BookError::GroupNotInherited(e.seq));
                }
                if self.group_by_name(&g.name).is_some() || g.aliases.iter().any(|a| self.group_by_name(a).is_some()) {
                    return Err(BookError::DuplicateGroup(g.name));
                }
                self.groups.insert(g.id, g);
            }
            ("group", AuditAction::Alter) => {
                let g: Group = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                let current = self.groups.get(&g.id).ok_or(BookError::UnknownGroup)?;
                check_before(json(current))?;
                let behaviour = |x: &Group| (x.nature, x.affects_gross_profit, x.is_cash_or_bank);
                match (current.parent, g.parent) {
                    // Primary groups stay primary and keep their behaviour.
                    (None, None) if behaviour(current) == behaviour(&g) => {}
                    (Some(_), Some(p)) => {
                        let parent = self.groups.get(&p).ok_or(BookError::UnknownGroup)?;
                        if self.is_within(p, g.id) || behaviour(parent) != behaviour(&g) || behaviour(current) != behaviour(&g) {
                            return Err(BookError::InvalidGroupParent);
                        }
                    }
                    _ => return Err(BookError::InvalidGroupParent),
                }
                let clash = |n: &str| self.groups.values().any(|x| x.id != g.id && (x.name.eq_ignore_ascii_case(n) || x.aliases.iter().any(|a| a.eq_ignore_ascii_case(n))));
                if g.name.trim().is_empty() || clash(&g.name) || g.aliases.iter().any(|a| clash(a)) {
                    return Err(BookError::DuplicateGroup(g.name));
                }
                self.groups.insert(g.id, g);
            }
            ("group", AuditAction::Delete) => {
                no_after()?;
                let g: Group = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                let current = self.groups.get(&g.id).ok_or(BookError::UnknownGroup)?;
                check_before(json(current))?;
                let in_use = current.parent.is_none()
                    || self.groups.values().any(|x| x.parent == Some(g.id))
                    || self.ledgers.values().any(|l| l.group == g.id);
                if in_use {
                    return Err(BookError::GroupInUse);
                }
                self.groups.remove(&g.id);
            }
            ("ledger", AuditAction::Create | AuditAction::Alter) => {
                let l: Ledger = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(l.id.0.to_string())?;
                match (e.action, self.ledgers.get(&l.id)) {
                    (AuditAction::Create, None) if e.before.is_none() => {}
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => check_before(json(current))?,
                    (_, None) => return Err(BookError::UnknownLedger(l.id)),
                }
                if !self.groups.contains_key(&l.group) {
                    return Err(BookError::UnknownGroup);
                }
                if self.ledgers.values().any(|x| x.id != l.id && x.name.eq_ignore_ascii_case(&l.name)) {
                    return Err(BookError::DuplicateLedger(l.name));
                }
                if l.tax_id_override.is_some() && l.tax_id.is_none() {
                    return Err(bad());
                }
                if l.tax_class.as_ref().is_some_and(|c| c.trim().is_empty() || c.chars().count() > crate::profile::MAX_TAX_CLASS) {
                    return Err(BookError::InvalidTax("tax class must be 1 to 24 characters".into()));
                }
                if let Some(m) = &l.mailing {
                    let long = m.address.chars().count() > 400
                        || [&m.postal_code, &m.phone, &m.email].iter().any(|f| f.chars().count() > 120);
                    if long || m.country.as_ref().is_some_and(|c| !Profile::is_valid_country(c)) {
                        return Err(BookError::InvalidTax("mailing details are too long or the country code is not valid".into()));
                    }
                }
                if let Some(t) = &l.tax {
                    let duties = GroupId::default_named("Duties & Taxes");
                    if !self.is_within(l.group, duties) {
                        return Err(BookError::InvalidTax("only ledgers under Duties & Taxes collect a tax".into()));
                    }
                    if t.component.trim().is_empty() || t.component.len() > 16 {
                        return Err(BookError::InvalidTax("the tax name must be 1 to 16 characters".into()));
                    }
                }
                if l.pan.as_ref().is_some_and(|p| p.len() != crate::chart::PAN_LEN || !p.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())) {
                    return Err(BookError::InvalidTax("a PAN has 10 capital letters and digits".into()));
                }
                if let Some(t) = &l.tds {
                    self.check_tds(&l, t)?;
                }
                if let Some(t) = &l.tcs {
                    self.check_tcs(&l, t)?;
                }
                crate::forex::check_ledger(self, &l)?;
                let bad_bill = |b: &crate::chart::OpeningBill| {
                    b.reference.trim().is_empty()
                        || b.reference.chars().count() > crate::chart::MAX_BILL_REFERENCE
                        || b.amount.is_zero()
                        || b.date.zip(b.due).is_some_and(|(d, due)| due < d)
                };
                if l.opening_bills.iter().any(bad_bill) {
                    return Err(BookError::InvalidOpeningBill);
                }
                // Contra vouchers and bank dates need a cash or bank ledger;
                // moving one out of those groups would invalidate them.
                if e.action == AuditAction::Alter && !self.group_is_cash_or_bank(l.group) {
                    let needs_cash_bank = self.vouchers.values().any(|v| {
                        v.entries.iter().any(|x| x.ledger == l.id && (v.kind == VoucherKind::Contra || x.bank_date.is_some()))
                    });
                    if needs_cash_bank {
                        return Err(BookError::ContraNonCashLedger);
                    }
                }
                self.ledgers.insert(l.id, l);
            }
            ("ledger", AuditAction::Delete) => {
                no_after()?;
                let l: Ledger = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(l.id.0.to_string())?;
                let current = self.ledgers.get(&l.id).ok_or(BookError::UnknownLedger(l.id))?;
                check_before(json(current))?;
                if self.vouchers.values().any(|v| v.entries.iter().any(|x| x.ledger == l.id)) {
                    return Err(BookError::LedgerInUse);
                }
                self.ledgers.remove(&l.id);
            }
            ("voucher", AuditAction::Create | AuditAction::Alter) => {
                let v: Voucher = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(v.id.0.to_string())?;
                let replacing = match (e.action, self.vouchers.get(&v.id)) {
                    (AuditAction::Create, None) if e.before.is_none() => None,
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => {
                        check_before(json(current))?;
                        if current.cancelled {
                            return Err(BookError::VoucherCancelled);
                        }
                        Some(v.id)
                    }
                    (_, None) => return Err(BookError::UnknownVoucher),
                };
                self.check_voucher(&v, replacing)?;
                crate::forex::check_voucher(self, &v)?;
                self.store_voucher(v);
            }
            ("voucher", AuditAction::Delete) => {
                no_after()?;
                let v: Voucher = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(v.id.0.to_string())?;
                let current = self.vouchers.get(&v.id).ok_or(BookError::UnknownVoucher)?;
                check_before(json(current))?;
                self.remove_voucher(v.id);
            }
            ("unit", AuditAction::Create | AuditAction::Alter) => {
                let u: Unit = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(u.id.0.to_string())?;
                match (e.action, self.units.get(&u.id)) {
                    (AuditAction::Create, None) if e.before.is_none() => {}
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => check_before(json(current))?,
                    (_, None) => return Err(BookError::UnknownUnit),
                }
                if u.symbol.trim().is_empty() || u.decimals > 4 {
                    return Err(bad());
                }
                if self.units.values().any(|x| x.id != u.id && x.symbol.eq_ignore_ascii_case(&u.symbol)) {
                    return Err(BookError::DuplicateUnit(u.symbol));
                }
                // Fewer decimals must not invalidate quantities already entered.
                if self.stock_items.values().filter(|i| i.unit == u.id).any(|i| self.item_quantities(i.id).any(|q| q.normalize().scale() > u.decimals)) {
                    return Err(BookError::QuantityPrecision(u.symbol));
                }
                self.units.insert(u.id, u);
            }
            ("unit", AuditAction::Delete) => {
                no_after()?;
                let u: Unit = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(u.id.0.to_string())?;
                let current = self.units.get(&u.id).ok_or(BookError::UnknownUnit)?;
                check_before(json(current))?;
                if self.stock_items.values().any(|i| i.unit == u.id) {
                    return Err(BookError::StockInUse);
                }
                self.units.remove(&u.id);
            }
            ("stockgroup", AuditAction::Create | AuditAction::Alter) => {
                let g: StockGroup = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                match (e.action, self.stock_groups.get(&g.id)) {
                    (AuditAction::Create, None) if e.before.is_none() => {}
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => check_before(json(current))?,
                    (_, None) => return Err(BookError::UnknownStockGroup),
                }
                if let Some(p) = g.parent {
                    if !self.stock_groups.contains_key(&p) {
                        return Err(BookError::UnknownStockGroup);
                    }
                    if self.stock_group_within(p, g.id) {
                        return Err(BookError::InvalidGroupParent);
                    }
                }
                self.check_stock_name(&g.name, g.id.0)?;
                self.stock_groups.insert(g.id, g);
            }
            ("stockgroup", AuditAction::Delete) => {
                no_after()?;
                let g: StockGroup = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                let current = self.stock_groups.get(&g.id).ok_or(BookError::UnknownStockGroup)?;
                check_before(json(current))?;
                if self.stock_groups.values().any(|x| x.parent == Some(g.id)) || self.stock_items.values().any(|i| i.group == Some(g.id)) {
                    return Err(BookError::StockInUse);
                }
                self.stock_groups.remove(&g.id);
            }
            ("stockitem", AuditAction::Create | AuditAction::Alter) => {
                let i: StockItem = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(i.id.0.to_string())?;
                match (e.action, self.stock_items.get(&i.id)) {
                    (AuditAction::Create, None) if e.before.is_none() => {}
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => check_before(json(current))?,
                    (_, None) => return Err(BookError::UnknownStockItem),
                }
                let unit = self.units.get(&i.unit).ok_or(BookError::UnknownUnit)?;
                if i.group.is_some_and(|g| !self.stock_groups.contains_key(&g)) {
                    return Err(BookError::UnknownStockGroup);
                }
                if i.opening_qty < Amount::ZERO || i.opening_value < Amount::ZERO {
                    return Err(bad());
                }
                let qty_scale_ok = |q: &crate::Quantity| q.normalize().scale() <= unit.decimals;
                if !qty_scale_ok(&i.opening_qty) || !self.item_quantities(i.id).all(|q| qty_scale_ok(&q)) {
                    return Err(BookError::QuantityPrecision(i.opening_qty.to_string()));
                }
                self.check_stock_name(&i.name, i.id.0)?;
                if i.tax_class.as_ref().is_some_and(|c| c.trim().is_empty() || c.chars().count() > crate::profile::MAX_TAX_CLASS) {
                    return Err(BookError::InvalidTax("tax class must be 1 to 24 characters".into()));
                }
                self.stock_items.insert(i.id, i);
            }
            ("stockitem", AuditAction::Delete) => {
                no_after()?;
                let i: StockItem = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(i.id.0.to_string())?;
                let current = self.stock_items.get(&i.id).ok_or(BookError::UnknownStockItem)?;
                check_before(json(current))?;
                if self.item_quantities(i.id).next().is_some() {
                    return Err(BookError::StockInUse);
                }
                self.stock_items.remove(&i.id);
            }
            ("godown", AuditAction::Create | AuditAction::Alter) => {
                let g: Godown = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                match (e.action, self.godowns.get(&g.id)) {
                    (AuditAction::Create, None) if e.before.is_none() => {}
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => check_before(json(current))?,
                    (_, None) => return Err(BookError::UnknownGodown),
                }
                if let Some(p) = g.parent {
                    if !self.godowns.contains_key(&p) {
                        return Err(BookError::UnknownGodown);
                    }
                    if self.godown_within(p, g.id) {
                        return Err(BookError::InvalidGroupParent);
                    }
                }
                if g.name.trim().is_empty() || self.godowns.values().any(|x| x.id != g.id && x.name.eq_ignore_ascii_case(&g.name)) {
                    return Err(BookError::DuplicateGodown(g.name));
                }
                self.godowns.insert(g.id, g);
            }
            ("godown", AuditAction::Delete) => {
                no_after()?;
                let g: Godown = serde_json::from_value(payload(&e.before)?).map_err(|_| bad())?;
                check_id(g.id.0.to_string())?;
                let current = self.godowns.get(&g.id).ok_or(BookError::UnknownGodown)?;
                check_before(json(current))?;
                let used = self.godowns.values().any(|x| x.parent == Some(g.id))
                    || self.all_stock_lines().any(|l| l.godown == Some(g.id));
                if used {
                    return Err(BookError::StockInUse);
                }
                self.godowns.remove(&g.id);
            }
            ("profile", AuditAction::Create | AuditAction::Alter) => {
                if e.entity != PROFILE_ENTITY {
                    return Err(bad());
                }
                let p: Profile = serde_json::from_value(payload(&e.after)?).map_err(|_| bad())?;
                match (e.action, &self.profile) {
                    (AuditAction::Create, None) if e.before.is_none() => {}
                    (AuditAction::Create, _) => return Err(BookError::AlreadyExists(e.seq)),
                    (_, Some(current)) => check_before(json(current))?,
                    (_, None) => return Err(bad()),
                }
                if !Profile::is_valid_country(&p.country) {
                    return Err(BookError::InvalidCountry(p.country));
                }
                self.check_numbering(&p)?;
                Self::check_tax_profile(&p)?;
                Self::check_users(&p)?;
                Self::check_statement_settings(&p)?;
                crate::forex::check_profile(&p)?;
                let renumbered = numbering_basis(self.profile.as_ref()) != numbering_basis(Some(&p));
                self.profile = Some(p);
                if renumbered {
                    self.numbers = NumberIndex::build(self.vouchers.values(), self.profile.as_ref());
                }
            }
            _ => return Err(bad()),
        }
        self.audit.push_checked(e.clone());
        Ok(())
    }

    /// Record and apply a change made on this copy of the books.
    fn commit(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        action: AuditAction,
        entity: String,
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    ) -> Result<(), BookError> {
        let entry = self.audit.next_entry(at, user, action, entity, before, after);
        self.apply_entry(&entry)
    }

    /// Re-apply a change recorded on another copy of these books (another
    /// device's audit entry) as a new, fully validated operation, keeping
    /// record ids, author and time. `before` is taken from this copy's
    /// current state. Used to rebase offline edits onto the server's trail;
    /// the original entry stays in that device's archive.
    pub fn reapply(&mut self, entry: &AuditEntry) -> Result<(), BookError> {
        let bad = || BookError::UnreplayableEntry(entry.seq);
        let (kind, id) = entry.entity.split_once(':').ok_or_else(bad)?;
        let current = match (kind, entry.action) {
            (_, AuditAction::Create) => None,
            ("group", _) => self.groups.values().find(|g| g.id.0.to_string() == id).map(json),
            ("ledger", _) => self.ledgers.values().find(|l| l.id.0.to_string() == id).map(json),
            ("voucher", _) => self.vouchers.values().find(|v| v.id.0.to_string() == id).map(json),
            ("profile", _) => self.profile.as_ref().map(json),
            ("unit", _) => self.units.values().find(|u| u.id.0.to_string() == id).map(json),
            ("stockgroup", _) => self.stock_groups.values().find(|g| g.id.0.to_string() == id).map(json),
            ("stockitem", _) => self.stock_items.values().find(|i| i.id.0.to_string() == id).map(json),
            ("godown", _) => self.godowns.values().find(|g| g.id.0.to_string() == id).map(json),
            _ => return Err(bad()),
        };
        if entry.action != AuditAction::Create && current.is_none() {
            return Err(match kind {
                "voucher" => BookError::UnknownVoucher,
                "group" => BookError::UnknownGroup,
                _ => bad(),
            });
        }
        self.commit(entry.at, &entry.user, entry.action, entry.entity.clone(), current, entry.after.clone())
    }

    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    /// Company-wide settings, once they have been set.
    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// The country whose compliance pack applies, once it has been picked.
    pub fn country(&self) -> Option<&str> {
        self.profile.as_ref().map(|p| p.country.as_str())
    }

    /// Set the company's country (ISO 3166-1 alpha-2). Audited like any
    /// other change; the first call creates the profile.
    pub fn set_country(&mut self, at: DateTime<Utc>, user: &str, country: &str) -> Result<(), BookError> {
        let after = Profile { country: country.to_owned(), ..self.profile.clone().unwrap_or_default() };
        self.save_profile(at, user, after)
    }

    /// Set the first month of the financial year (1–12), which yearly
    /// numbering follows. Needs the country to be set first.
    pub fn set_fy_start_month(&mut self, at: DateTime<Utc>, user: &str, month: u32) -> Result<(), BookError> {
        let current = self.profile.clone().ok_or(BookError::InvalidNumbering("set the country first".into()))?;
        self.save_profile(at, user, Profile { fy_start_month: Some(month), ..current })
    }

    /// Set how a voucher type is numbered. Refused if vouchers already
    /// saved would then share a number within one series period.
    pub fn set_numbering(&mut self, at: DateTime<Utc>, user: &str, kind: VoucherKind, series: NumberSeries) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidNumbering("set the country first".into()))?;
        if series == NumberSeries::default() {
            p.numbering.remove(&kind);
        } else {
            p.numbering.insert(kind, series);
        }
        self.save_profile(at, user, p)
    }

    pub(crate) fn save_profile(&mut self, at: DateTime<Utc>, user: &str, after: Profile) -> Result<(), BookError> {
        let (action, before) = match &self.profile {
            Some(p) => (AuditAction::Alter, Some(json(p))),
            None => (AuditAction::Create, None),
        };
        self.commit(at, user, action, PROFILE_ENTITY.to_owned(), before, Some(json(&after)))
    }

    /// The numbering series of a voucher type (plain, never restarting,
    /// unless the company set one).
    pub fn numbering(&self, kind: VoucherKind) -> NumberSeries {
        self.profile.as_ref().and_then(|p| p.numbering.get(&kind)).cloned().unwrap_or_default()
    }

    /// First month of the financial year; January until a country sets it.
    pub fn fy_start_month(&self) -> u32 {
        self.profile.as_ref().and_then(|p| p.fy_start_month).unwrap_or(1)
    }

    /// Which series period a voucher number belongs to, under `profile`.
    fn series_key(profile: Option<&Profile>, kind: VoucherKind, date: NaiveDate) -> Option<NaiveDate> {
        let month = profile.and_then(|p| p.fy_start_month).unwrap_or(1);
        let series = profile.and_then(|p| p.numbering.get(&kind)).cloned().unwrap_or_default();
        series.period(date, month).map(|(start, _)| start)
    }

    /// A new profile must be well formed and must not make two saved
    /// vouchers share a number in one series period.
    fn check_numbering(&self, p: &Profile) -> Result<(), BookError> {
        if p.fy_start_month.is_some_and(|m| !(1..=12).contains(&m)) {
            return Err(BookError::InvalidNumbering("the financial year must start in month 1 to 12".into()));
        }
        if let Some((kind, _)) = p.numbering.iter().find(|(_, s)| !s.is_valid()) {
            return Err(BookError::InvalidNumbering(format!(
                "the {kind:?} prefix or suffix is too long or uses an unknown token"
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        for v in self.vouchers.values() {
            let key = (v.kind, series_name(v), Self::series_key(Some(p), v.kind, v.date), v.number.to_lowercase());
            if !seen.insert(key) {
                return Err(BookError::InvalidNumbering(format!(
                    "{:?} number {:?} is used more than once in the same period",
                    v.kind, v.number
                )));
            }
        }
        Ok(())
    }

    pub fn groups(&self) -> impl Iterator<Item = &Group> {
        self.groups.values()
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(&id)
    }

    pub fn ledgers(&self) -> impl Iterator<Item = &Ledger> {
        self.ledgers.values()
    }

    pub fn ledger_by_name(&self, name: &str) -> Option<&Ledger> {
        self.ledgers.values().find(|l| l.name.eq_ignore_ascii_case(name))
    }

    pub fn vouchers(&self) -> impl Iterator<Item = &Voucher> {
        self.vouchers.values()
    }

    /// The top-level group a group belongs to.
    pub fn primary_group(&self, mut id: GroupId) -> Option<&Group> {
        // Bounded: a corrupt parent cycle yields None instead of spinning.
        for _ in 0..=self.groups.len() {
            let g = self.groups.get(&id)?;
            match g.parent {
                Some(p) => id = p,
                None => return Some(g),
            }
        }
        None
    }

    /// Next free number for a voucher type dated `date`, in its series:
    /// "1", "2", … by default, or e.g. "26-27/14" with a yearly series.
    pub fn next_voucher_number(&self, kind: VoucherKind, date: NaiveDate) -> String {
        let series = self.numbering(kind);
        let month = self.fy_start_month();
        let period = series.period(date, month).map(|(start, _)| start);
        let max = self.numbers.max_running(kind, period).unwrap_or(0);
        series.format(max + 1, date, month)
    }

    /// Vouchers of `kind` whose number matches `number` loosely: ignoring
    /// ASCII case and treating every character other than an ASCII letter
    /// or digit as the same, so "26-27/1" also finds "26/27-1". Cancelled
    /// vouchers are included (they keep their numbers). In id order, like
    /// [`Self::vouchers`]. This is a superset of any stricter comparison,
    /// for duplicate checks that would otherwise scan every voucher.
    pub fn vouchers_numbered(&self, kind: VoucherKind, number: &str) -> impl Iterator<Item = &Voucher> {
        self.numbers.same_number(kind, number).filter_map(|id| self.vouchers.get(&id))
    }

    /// Store a checked voucher (new or replacing one with its id), keeping
    /// the number index in step.
    fn store_voucher(&mut self, v: Voucher) {
        if let Some(old) = self.vouchers.get(&v.id) {
            self.numbers.remove(old, self.profile.as_ref());
        }
        self.numbers.insert(&v, self.profile.as_ref());
        self.vouchers.insert(v.id, v);
    }

    #[cfg(test)]
    pub(crate) fn number_index(&self) -> &NumberIndex {
        &self.numbers
    }

    fn remove_voucher(&mut self, id: VoucherId) {
        if let Some(old) = self.vouchers.remove(&id) {
            self.numbers.remove(&old, self.profile.as_ref());
        }
    }

    /// Set a ledger's opening balance (signed: debit positive).
    pub fn set_opening_balance(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        id: LedgerId,
        opening: Amount,
    ) -> Result<(), BookError> {
        let current = self.ledgers.get(&id).ok_or(BookError::UnknownLedger(id))?;
        let after = Ledger { opening, ..current.clone() };
        self.commit(at, user, AuditAction::Alter, format!("ledger:{}", id.0), Some(json(current)), Some(json(&after)))
    }

    /// Sum of all opening balances (debit positive). Non-zero means the
    /// opening trial balance does not tally (interaction spec rule Ma7).
    pub fn opening_difference(&self) -> Amount {
        // Opening stock is an asset in the opening trial balance too.
        self.ledgers.values().map(|l| l.opening).sum::<Amount>() + crate::inventory::opening_stock_value(self)
    }

    /// Find a group by its name or one of its aliases.
    pub fn group_by_name(&self, name: &str) -> Option<&Group> {
        self.groups.values().find(|g| {
            g.name.eq_ignore_ascii_case(name) || g.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
        })
    }

    /// Groups whose name or alias contains `query` anywhere, case-insensitively,
    /// sorted by name — the list-picker filter (interaction spec rule P3).
    pub fn search_groups(&self, query: &str) -> Vec<&Group> {
        let q = query.to_lowercase();
        let mut hits: Vec<&Group> = self
            .groups
            .values()
            .filter(|g| {
                g.name.to_lowercase().contains(&q) || g.aliases.iter().any(|a| a.to_lowercase().contains(&q))
            })
            .collect();
        hits.sort_by(|a, b| a.name.cmp(&b.name));
        hits
    }

    pub fn ledger(&self, id: LedgerId) -> Option<&Ledger> {
        self.ledgers.get(&id)
    }

    pub fn voucher(&self, id: VoucherId) -> Option<&Voucher> {
        self.vouchers.get(&id)
    }

    /// Create a sub-group under `parent`. Like a Tally sub-group it inherits
    /// its parent's nature and gross-profit / cash-or-bank behaviour. Names
    /// must not clash with any existing group name or alias.
    pub fn create_group(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        name: &str,
        parent: GroupId,
    ) -> Result<GroupId, BookError> {
        let p = self.groups.get(&parent).ok_or(BookError::UnknownGroup)?;
        let group = Group {
            id: GroupId::new(),
            name: name.to_owned(),
            parent: Some(parent),
            nature: p.nature,
            aliases: Vec::new(),
            affects_gross_profit: p.affects_gross_profit,
            is_cash_or_bank: p.is_cash_or_bank,
        };
        let id = group.id;
        self.commit(at, user, AuditAction::Create, format!("group:{}", id.0), None, Some(json(&group)))?;
        Ok(id)
    }

    pub fn create_ledger(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        name: &str,
        group: GroupId,
    ) -> Result<LedgerId, BookError> {
        let ledger = Ledger {
            id: LedgerId::new(),
            name: name.to_owned(),
            group,
            opening: Amount::ZERO,
            credit_days: None,
            tax_id: None,
            tax_id_override: None,
            tax: None,
            mailing: None,
            tax_class: None,
            pan: None,
            tds: None,
            tcs: None,
            currency: None,
            opening_bills: Vec::new(),
        };
        let id = ledger.id;
        self.commit(at, user, AuditAction::Create, format!("ledger:{}", id.0), None, Some(json(&ledger)))?;
        Ok(id)
    }

    pub fn delete_ledger(&mut self, at: DateTime<Utc>, user: &str, id: LedgerId) -> Result<(), BookError> {
        let current = self.ledgers.get(&id).ok_or(BookError::UnknownLedger(id))?;
        self.commit(at, user, AuditAction::Delete, format!("ledger:{}", id.0), Some(json(current)), None)
    }

    pub fn post_voucher(&mut self, at: DateTime<Utc>, user: &str, voucher: Voucher) -> Result<VoucherId, BookError> {
        let id = voucher.id;
        self.commit(at, user, AuditAction::Create, format!("voucher:{}", id.0), None, Some(json(&voucher)))?;
        Ok(id)
    }

    pub fn alter_voucher(&mut self, at: DateTime<Utc>, user: &str, voucher: Voucher) -> Result<(), BookError> {
        let current = self.vouchers.get(&voucher.id).ok_or(BookError::UnknownVoucher)?;
        let entity = format!("voucher:{}", voucher.id.0);
        self.commit(at, user, AuditAction::Alter, entity, Some(json(current)), Some(json(&voucher)))
    }

    /// Cancel a voucher: it keeps its number (so invoice series have no
    /// gaps) and its lines for the record, but stops affecting balances.
    pub fn cancel_voucher(&mut self, at: DateTime<Utc>, user: &str, id: VoucherId) -> Result<(), BookError> {
        let current = self.vouchers.get(&id).ok_or(BookError::UnknownVoucher)?;
        if current.cancelled {
            return Err(BookError::VoucherCancelled);
        }
        let after = Voucher { cancelled: true, ..current.clone() };
        self.commit(at, user, AuditAction::Alter, format!("voucher:{}", id.0), Some(json(current)), Some(json(&after)))
    }

    /// Set a party's default credit period in days (`None` clears it).
    pub fn set_credit_days(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        id: LedgerId,
        credit_days: Option<u32>,
    ) -> Result<(), BookError> {
        let current = self.ledgers.get(&id).ok_or(BookError::UnknownLedger(id))?;
        let after = Ledger { credit_days, ..current.clone() };
        self.commit(at, user, AuditAction::Alter, format!("ledger:{}", id.0), Some(json(current)), Some(json(&after)))
    }

    /// Set a party's tax registration (`None` clears it). `override_reason`
    /// records why an ID that failed validation was kept; it is dropped
    /// along with the ID.
    pub fn set_tax_id(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        id: LedgerId,
        tax_id: Option<String>,
        override_reason: Option<String>,
    ) -> Result<(), BookError> {
        let current = self.ledgers.get(&id).ok_or(BookError::UnknownLedger(id))?;
        let tax_id_override = override_reason.filter(|_| tax_id.is_some());
        let after = Ledger { tax_id, tax_id_override, ..current.clone() };
        self.commit(at, user, AuditAction::Alter, format!("ledger:{}", id.0), Some(json(current)), Some(json(&after)))
    }

    /// Vouchers that count: everything except cancelled vouchers.
    pub fn active_vouchers(&self) -> impl Iterator<Item = &Voucher> {
        self.vouchers.values().filter(|v| !v.cancelled)
    }

    /// The narration of the latest voucher (by date, then entry order) that
    /// has one, optionally only of `kind` and only with a line for `ledger`:
    /// what `Ctrl+R` and `Alt+R` bring back into a new voucher.
    pub fn last_narration(&self, kind: Option<VoucherKind>, ledger: Option<LedgerId>) -> Option<&str> {
        let mut best: Option<&Voucher> = None;
        // Ids are UUIDv7, so the map runs in entry order: on the same date the later entry wins.
        for v in self.active_vouchers().filter(|v| {
            !v.narration.trim().is_empty()
                && kind.is_none_or(|k| v.kind == k)
                && ledger.is_none_or(|l| v.entries.iter().any(|e| e.ledger == l))
        }) {
            if best.is_none_or(|b| v.date >= b.date) {
                best = Some(v);
            }
        }
        best.map(|v| v.narration.as_str())
    }

    pub fn delete_voucher(&mut self, at: DateTime<Utc>, user: &str, id: VoucherId) -> Result<(), BookError> {
        let current = self.vouchers.get(&id).ok_or(BookError::UnknownVoucher)?;
        self.commit(at, user, AuditAction::Delete, format!("voucher:{}", id.0), Some(json(current)), None)
    }

    /// Signed balance (debit positive) of every ledger: opening plus all
    /// vouchers dated on or before `as_at` (all vouchers when `None`).
    pub fn balances(&self, as_at: Option<NaiveDate>) -> BTreeMap<LedgerId, Amount> {
        let mut net: BTreeMap<LedgerId, Amount> = self.ledgers.values().map(|l| (l.id, l.opening)).collect();
        for v in self.active_vouchers().filter(|v| as_at.is_none_or(|d| v.date <= d)) {
            for e in &v.entries {
                *net.entry(e.ledger).or_default() += e.signed();
            }
        }
        net
    }

    /// Closing balance of each ledger with a non-zero balance, by name.
    pub fn trial_balance(&self) -> Vec<TrialBalanceRow> {
        let net = self.balances(None);
        let mut rows: Vec<_> = net
            .into_iter()
            .filter(|(_, bal)| !bal.is_zero())
            .filter_map(|(ledger, bal)| {
                Some(TrialBalanceRow {
                    ledger,
                    name: self.ledgers.get(&ledger)?.name.clone(),
                    debit: bal.max(Amount::ZERO),
                    credit: (-bal).max(Amount::ZERO),
                })
            })
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows
    }

    pub fn nature_of(&self, ledger: LedgerId) -> Option<Nature> {
        let group = self.ledgers.get(&ledger)?.group;
        Some(self.groups.get(&group)?.nature)
    }

    fn check_voucher(&self, voucher: &Voucher, replacing: Option<VoucherId>) -> Result<(), BookError> {
        voucher.validate()?;
        let trading = matches!(voucher.kind, VoucherKind::Sales | VoucherKind::Purchase | VoucherKind::CreditNote | VoucherKind::DebitNote);
        for e in &voucher.entries {
            if !self.ledgers.contains_key(&e.ledger) {
                return Err(BookError::UnknownLedger(e.ledger));
            }
            if !e.items.is_empty() && !trading {
                return Err(BookError::ItemsNotAllowed);
            }
            if e.bank_date.is_some_and(|d| d < voucher.date) || (e.bank_date.is_some() && !self.is_cash_or_bank(e.ledger)) {
                return Err(BookError::InvalidBankDate);
            }
        }
        for l in voucher.entries.iter().flat_map(|e| &e.items).chain(voucher.stock.iter().map(|m| &m.line)) {
            let item = self.stock_items.get(&l.item).ok_or(BookError::UnknownStockItem)?;
            let decimals = self.units.get(&item.unit).map_or(0, |u| u.decimals);
            if l.qty.normalize().scale() > decimals {
                return Err(BookError::QuantityPrecision(l.qty.to_string()));
            }
            if l.godown.is_some_and(|g| !self.godowns.contains_key(&g)) {
                return Err(BookError::UnknownGodown);
            }
            if l.batch.as_ref().is_some_and(|b| b.trim().is_empty() || b.chars().count() > MAX_BATCH) {
                return Err(BookError::InvalidBatch);
            }
        }
        if voucher.kind == VoucherKind::Contra && !voucher.entries.iter().all(|e| self.is_cash_or_bank(e.ledger)) {
            return Err(BookError::ContraNonCashLedger);
        }
        // Unique within the type's series period (the whole life of the
        // company unless numbering restarts).
        if voucher.series.as_deref().is_some_and(|s| s.trim().is_empty() || s.chars().count() > MAX_SERIES_NAME) {
            return Err(BookError::InvalidNumbering(format!("a series name has 1 to {MAX_SERIES_NAME} characters")));
        }
        let key = Self::series_key(self.profile.as_ref(), voucher.kind, voucher.date);
        let series = series_name(voucher);
        let clash = self.vouchers_numbered(voucher.kind, &voucher.number).any(|v| {
            Some(v.id) != replacing
                && v.number.eq_ignore_ascii_case(&voucher.number)
                && series_name(v) == series
                && Self::series_key(self.profile.as_ref(), v.kind, v.date) == key
        });
        if clash {
            return Err(BookError::DuplicateVoucherNumber(voucher.number.clone()));
        }
        Ok(())
    }

    /// Whether a ledger sits under a cash-in-hand or bank group, including
    /// Bank OD A/c and Bank OCC A/c (see [`Self::group_is_cash_or_bank`]).
    pub fn is_cash_or_bank(&self, ledger: LedgerId) -> bool {
        self.ledgers.get(&ledger).is_some_and(|l| self.group_is_cash_or_bank(l.group))
    }

    /// Whether a group is, or sits under, a group whose ledgers count as
    /// cash or bank: Contra vouchers, payment and receipt accounts and bank
    /// dates may use them. Besides the groups flagged `is_cash_or_bank`
    /// (Cash-in-Hand, Bank Accounts), the default Bank OD A/c and Bank OCC
    /// A/c count too, as they do in Tally: an overdraft or cash credit
    /// account is a bank account that runs a credit balance. Those two keep
    /// their Loans (Liability) nature and their stored flag, so company
    /// files written before stay exactly as they were.
    pub fn group_is_cash_or_bank(&self, group: GroupId) -> bool {
        let bank_loans = [GroupId::default_named("Bank OD A/c"), GroupId::default_named("Bank OCC A/c")];
        let mut group = Some(group);
        for _ in 0..=self.groups.len() {
            let Some(g) = group.and_then(|id| self.groups.get(&id)) else { return false };
            if g.is_cash_or_bank || bank_loans.contains(&g.id) {
                return true;
            }
            group = g.parent;
        }
        false
    }
}

impl Book {
    /// Whether group `id` is `ancestor` or sits somewhere below it.
    fn is_within(&self, mut id: GroupId, ancestor: GroupId) -> bool {
        for _ in 0..=self.groups.len() {
            if id == ancestor {
                return true;
            }
            match self.groups.get(&id).and_then(|g| g.parent) {
                Some(p) => id = p,
                None => return false,
            }
        }
        true
    }

    fn godown_within(&self, mut id: GodownId, ancestor: GodownId) -> bool {
        for _ in 0..=self.godowns.len() {
            if id == ancestor {
                return true;
            }
            match self.godowns.get(&id).and_then(|g| g.parent) {
                Some(p) => id = p,
                None => return false,
            }
        }
        true
    }

    pub fn godowns(&self) -> impl Iterator<Item = &Godown> {
        self.godowns.values()
    }

    pub fn godown(&self, id: GodownId) -> Option<&Godown> {
        self.godowns.get(&id)
    }

    /// Create or alter a godown (by id).
    pub fn save_godown(&mut self, at: DateTime<Utc>, user: &str, godown: Godown) -> Result<GodownId, BookError> {
        let id = godown.id;
        let (action, before) = self.godowns.get(&id).map_or((AuditAction::Create, None), |c| (AuditAction::Alter, Some(json(c))));
        self.commit(at, user, action, format!("godown:{}", id.0), before, Some(json(&godown)))?;
        Ok(id)
    }

    pub fn delete_godown(&mut self, at: DateTime<Utc>, user: &str, id: GodownId) -> Result<(), BookError> {
        let current = self.godowns.get(&id).ok_or(BookError::UnknownGodown)?;
        self.commit(at, user, AuditAction::Delete, format!("godown:{}", id.0), Some(json(current)), None)
    }

    /// The company's own tax registration and region (`None` clears).
    pub fn set_company_tax(&mut self, at: DateTime<Utc>, user: &str, tax_id: Option<String>, region: Option<String>) -> Result<(), BookError> {
        let current = self.profile.clone().ok_or(BookError::InvalidTax("set the country first".into()))?;
        self.save_profile(at, user, Profile { tax_id, tax_region: region, ..current })
    }

    /// Set (or, with `None`, remove) the rate of a tax class from a date.
    pub fn set_tax_rate(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        class: &str,
        from: NaiveDate,
        percent: Option<Amount>,
    ) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidTax("set the country first".into()))?;
        let class = normalise_tax_class(class);
        match percent {
            Some(r) => {
                p.tax_rates.entry(class).or_default().insert(from, r);
            }
            None => {
                if let Some(changes) = p.tax_rates.get_mut(&class) {
                    changes.remove(&from);
                    if changes.is_empty() {
                        p.tax_rates.remove(&class);
                    }
                }
            }
        }
        self.save_profile(at, user, p)
    }

    /// Set the rates of several tax classes at once, as `(class, from,
    /// percent)`, in one audited change (an import sets hundreds).
    pub fn set_tax_rates(&mut self, at: DateTime<Utc>, user: &str, rates: &[(String, NaiveDate, Amount)]) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidTax("set the country first".into()))?;
        if rates.is_empty() {
            return Ok(());
        }
        for (class, from, percent) in rates {
            p.tax_rates.entry(normalise_tax_class(class)).or_default().insert(*from, *percent);
        }
        self.save_profile(at, user, p)
    }

    /// The rate of a tax class set from exactly `from`, if any.
    pub fn tax_rate_from(&self, class: &str, from: NaiveDate) -> Option<Amount> {
        self.profile.as_ref()?.tax_rates.get(&normalise_tax_class(class))?.get(&from).copied()
    }

    /// Digits of HSN/SAC the company must show (4, 6 or 8; `None` unsets).
    pub fn set_hsn_digits(&mut self, at: DateTime<Utc>, user: &str, digits: Option<u8>) -> Result<(), BookError> {
        if digits.is_some_and(|d| ![4, 6, 8].contains(&d)) {
            return Err(BookError::InvalidTax("HSN codes need 4, 6 or 8 digits".into()));
        }
        let current = self.profile.clone().ok_or(BookError::InvalidTax("set the country first".into()))?;
        self.save_profile(at, user, Profile { hsn_digits: digits, ..current })
    }

    /// The company's mailing address (several lines; empty clears).
    pub fn set_company_address(&mut self, at: DateTime<Utc>, user: &str, address: &str) -> Result<(), BookError> {
        let current = self.profile.clone().ok_or(BookError::InvalidTax("set the country first".into()))?;
        self.save_profile(at, user, Profile { address: address.trim().to_owned(), ..current })
    }

    /// Add or replace a user (by name, ignoring case).
    pub fn save_user(&mut self, at: DateTime<Utc>, user: &str, u: CompanyUser) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidUsers("set the country first".into()))?;
        match p.users.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&u.name)) {
            Some(x) => *x = u,
            None => p.users.push(u),
        }
        self.save_profile(at, user, p)
    }

    /// Remove a user. Removing the last one turns sign-in off.
    pub fn remove_user(&mut self, at: DateTime<Utc>, user: &str, name: &str) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidUsers("set the country first".into()))?;
        let before = p.users.len();
        p.users.retain(|x| !x.name.eq_ignore_ascii_case(name));
        if p.users.len() == before {
            return Err(BookError::InvalidUsers(format!("there is no user {name:?}")));
        }
        self.save_profile(at, user, p)
    }

    /// Map a group to a statutory statement line (India: a Schedule III
    /// line code); `None` removes the mapping. The code is opaque to the
    /// core, which checks only its shape: the pack validates its meaning.
    pub fn set_schedule3_mapping(&mut self, at: DateTime<Utc>, user: &str, group: GroupId, code: Option<&str>) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidStatementSetting("set the country first".into()))?;
        match code {
            Some(c) => {
                if !self.groups.contains_key(&group) {
                    return Err(BookError::UnknownGroup);
                }
                p.schedule3_map.insert(group, c.to_owned());
            }
            None => {
                p.schedule3_map.remove(&group);
            }
        }
        self.save_profile(at, user, p)
    }

    /// Mark a party as a micro or small enterprise (or clear the mark).
    pub fn set_msme_party(&mut self, at: DateTime<Utc>, user: &str, ledger: LedgerId, msme: bool) -> Result<(), BookError> {
        let mut p = self.profile.clone().ok_or(BookError::InvalidStatementSetting("set the country first".into()))?;
        if msme {
            if !self.ledgers.contains_key(&ledger) {
                return Err(BookError::UnknownLedger(ledger));
            }
            p.msme_parties.insert(ledger);
        } else {
            p.msme_parties.remove(&ledger);
        }
        self.save_profile(at, user, p)
    }

    /// The statement format (pack-numbered, 1 to 9) and rounding unit code;
    /// `None` returns either to the pack's default.
    pub fn set_schedule3_format(&mut self, at: DateTime<Utc>, user: &str, division: Option<u8>, rounding: Option<&str>) -> Result<(), BookError> {
        let current = self.profile.clone().ok_or(BookError::InvalidStatementSetting("set the country first".into()))?;
        self.save_profile(
            at,
            user,
            Profile { schedule3_division: division, schedule3_rounding: rounding.map(str::to_owned), ..current },
        )
    }

    fn check_statement_settings(p: &Profile) -> Result<(), BookError> {
        let bad = |what: String| Err(BookError::InvalidStatementSetting(what));
        if let Some(c) = p.schedule3_map.values().find(|c| !crate::profile::is_statement_code(c)) {
            return bad(format!("{c:?} is not a line code"));
        }
        if p.schedule3_division.is_some_and(|d| !(1..=9).contains(&d)) {
            return bad("the format number must be 1 to 9".into());
        }
        if let Some(r) = p.schedule3_rounding.as_deref().filter(|r| !crate::profile::is_statement_code(r)) {
            return bad(format!("{r:?} is not a rounding code"));
        }
        Ok(())
    }

    /// Users of the company (empty: no sign-in).
    pub fn users(&self) -> &[CompanyUser] {
        self.profile.as_ref().map_or(&[], |p| p.users.as_slice())
    }

    fn check_users(p: &Profile) -> Result<(), BookError> {
        let mut seen = std::collections::BTreeSet::new();
        for u in &p.users {
            let name = u.name.trim();
            if name.is_empty() || name.chars().count() > 40 || name != u.name {
                return Err(BookError::InvalidUsers("user names must be 1 to 40 characters, without leading or trailing spaces".into()));
            }
            if !seen.insert(name.to_lowercase()) {
                return Err(BookError::InvalidUsers(format!("{name:?} is listed twice")));
            }
            if !u.password_hash.starts_with("$argon2id$") {
                return Err(BookError::InvalidUsers(format!("{name:?} has no valid password hash")));
            }
        }
        if !p.users.is_empty() && !p.users.iter().any(|u| u.role == UserRole::Owner && !u.disabled) {
            return Err(BookError::InvalidUsers("at least one active Owner is needed".into()));
        }
        Ok(())
    }

    fn check_tax_profile(p: &Profile) -> Result<(), BookError> {
        if p.hsn_digits.is_some_and(|d| ![4, 6, 8].contains(&d)) {
            return Err(BookError::InvalidTax("HSN codes need 4, 6 or 8 digits".into()));
        }
        if p.address.chars().count() > crate::profile::MAX_ADDRESS {
            return Err(BookError::InvalidTax("the address is too long".into()));
        }
        for (class, changes) in &p.tax_rates {
            if class.is_empty() || class.chars().count() > crate::profile::MAX_TAX_CLASS || *class != normalise_tax_class(class) {
                return Err(BookError::InvalidTax(format!("tax class {class:?} is not valid")));
            }
            if changes.values().any(|r| r.is_sign_negative() || *r > Amount::ONE_HUNDRED) {
                return Err(BookError::InvalidTax(format!("rates for {class} must be between 0 and 100%")));
            }
        }
        Ok(())
    }

    fn stock_group_within(&self, mut id: StockGroupId, ancestor: StockGroupId) -> bool {
        for _ in 0..=self.stock_groups.len() {
            if id == ancestor {
                return true;
            }
            match self.stock_groups.get(&id).and_then(|g| g.parent) {
                Some(p) => id = p,
                None => return false,
            }
        }
        true
    }

    /// Stock groups and items share one name space, as in Tally.
    fn check_stock_name(&self, name: &str, own: uuid::Uuid) -> Result<(), BookError> {
        let taken = self.stock_groups.values().any(|g| g.id.0 != own && g.name.eq_ignore_ascii_case(name))
            || self.stock_items.values().any(|i| i.id.0 != own && i.name.eq_ignore_ascii_case(name));
        if name.trim().is_empty() || taken {
            return Err(BookError::DuplicateStockName(name.to_owned()));
        }
        Ok(())
    }

    /// Quantities of every voucher line (cancelled vouchers too) for an item.
    fn item_quantities(&self, item: StockItemId) -> impl Iterator<Item = crate::Quantity> + '_ {
        self.all_stock_lines().filter(move |l| l.item == item).map(|l| l.qty)
    }

    /// Every stock line on every voucher (cancelled ones too).
    fn all_stock_lines(&self) -> impl Iterator<Item = &crate::StockLine> + '_ {
        self.vouchers
            .values()
            .flat_map(|v| v.entries.iter().flat_map(|e| &e.items).chain(v.stock.iter().map(|m| &m.line)))
    }

    pub fn units(&self) -> impl Iterator<Item = &Unit> {
        self.units.values()
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.get(&id)
    }

    pub fn stock_groups(&self) -> impl Iterator<Item = &StockGroup> {
        self.stock_groups.values()
    }

    pub fn stock_group(&self, id: StockGroupId) -> Option<&StockGroup> {
        self.stock_groups.get(&id)
    }

    pub fn stock_items(&self) -> impl Iterator<Item = &StockItem> {
        self.stock_items.values()
    }

    pub fn stock_item(&self, id: StockItemId) -> Option<&StockItem> {
        self.stock_items.get(&id)
    }

    /// Rename a group, change its aliases, or move it under another group
    /// of the same kind. Primary groups can be renamed but not moved.
    pub fn alter_group(&mut self, at: DateTime<Utc>, user: &str, group: Group) -> Result<(), BookError> {
        let current = self.groups.get(&group.id).ok_or(BookError::UnknownGroup)?;
        self.commit(at, user, AuditAction::Alter, format!("group:{}", group.id.0), Some(json(current)), Some(json(&group)))
    }

    pub fn delete_group(&mut self, at: DateTime<Utc>, user: &str, id: GroupId) -> Result<(), BookError> {
        let current = self.groups.get(&id).ok_or(BookError::UnknownGroup)?;
        self.commit(at, user, AuditAction::Delete, format!("group:{}", id.0), Some(json(current)), None)
    }

    /// TDS settings: codes of a sensible size, a payable tag only on a duty
    /// ledger that collects no other tax, and a whole certificate.
    fn check_tds(&self, l: &Ledger, t: &crate::chart::LedgerTds) -> Result<(), BookError> {
        use crate::chart::{LedgerTds, MAX_TDS_CERTIFICATE, MAX_TDS_NATURE};
        let code_ok = |c: &str| !c.trim().is_empty() && c.chars().count() <= MAX_TDS_NATURE;
        match t {
            LedgerTds::Payable { payable_for } => {
                if !self.is_within(l.group, GroupId::default_named("Duties & Taxes")) {
                    return Err(BookError::InvalidTax("only ledgers under Duties & Taxes hold TDS payable".into()));
                }
                if l.tax.is_some() {
                    return Err(BookError::InvalidTax("a TDS payable ledger cannot also collect another tax".into()));
                }
                if !code_ok(payable_for) {
                    return Err(BookError::InvalidTax("the TDS nature must be 1 to 16 characters".into()));
                }
            }
            LedgerTds::Deductible(d) => {
                if d.nature.is_none() && d.deductee.is_none() {
                    return Err(BookError::InvalidTax("TDS needs a nature of payment or a deductee type".into()));
                }
                if d.nature.as_deref().is_some_and(|c| !code_ok(c)) {
                    return Err(BookError::InvalidTax("the TDS nature must be 1 to 16 characters".into()));
                }
                let has_certificate = d.certificate.is_some() || d.valid_from.is_some() || d.valid_to.is_some();
                if has_certificate && d.lower_rate.is_none() {
                    return Err(BookError::InvalidTax("a lower-deduction certificate needs its rate".into()));
                }
                if d.lower_rate.is_some_and(|r| r.is_sign_negative() || r > Amount::ONE_HUNDRED) {
                    return Err(BookError::InvalidTax("a TDS rate is 0 to 100 percent".into()));
                }
                if d.certificate.as_deref().is_some_and(|c| c.trim().is_empty() || c.chars().count() > MAX_TDS_CERTIFICATE) {
                    return Err(BookError::InvalidTax("the certificate number must be 1 to 40 characters".into()));
                }
                if d.valid_from.zip(d.valid_to).is_some_and(|(from, to)| from > to) {
                    return Err(BookError::InvalidTax("the certificate ends before it starts".into()));
                }
                if let Some(n) = &d.non_resident {
                    use crate::chart::{MAX_FOREIGN_TIN, MAX_REMITTANCE_CODE};
                    if n.rate.is_sign_negative() || n.rate > Amount::ONE_HUNDRED {
                        return Err(BookError::InvalidTax("a TDS rate is 0 to 100 percent".into()));
                    }
                    let short = |c: &Option<String>, max: usize| {
                        c.as_deref().is_none_or(|c| !c.trim().is_empty() && c.chars().count() <= max)
                    };
                    if !short(&n.country, MAX_REMITTANCE_CODE) || !short(&n.remittance, MAX_REMITTANCE_CODE) {
                        return Err(BookError::InvalidTax("country and remittance codes are 1 to 4 characters".into()));
                    }
                    if !short(&n.foreign_tin, MAX_FOREIGN_TIN) {
                        return Err(BookError::InvalidTax("the foreign tax number must be 1 to 25 characters".into()));
                    }
                }
            }
        }
        Ok(())
    }

    /// TCS settings: the same shape rules as TDS (a payable tag only on a
    /// duty ledger that collects nothing else; a whole certificate).
    fn check_tcs(&self, l: &Ledger, t: &crate::chart::LedgerTcs) -> Result<(), BookError> {
        use crate::chart::{LedgerTcs, MAX_TDS_CERTIFICATE, MAX_TDS_NATURE};
        let code_ok = |c: &str| !c.trim().is_empty() && c.chars().count() <= MAX_TDS_NATURE;
        match t {
            LedgerTcs::Payable { payable_for } => {
                if !self.is_within(l.group, GroupId::default_named("Duties & Taxes")) {
                    return Err(BookError::InvalidTax("only ledgers under Duties & Taxes hold TCS payable".into()));
                }
                if l.tax.is_some() || matches!(l.tds, Some(crate::chart::LedgerTds::Payable { .. })) {
                    return Err(BookError::InvalidTax("a TCS payable ledger cannot also hold another tax".into()));
                }
                if !code_ok(payable_for) {
                    return Err(BookError::InvalidTax("the TCS nature must be 1 to 16 characters".into()));
                }
            }
            LedgerTcs::Collectible(c) => {
                if c.nature.is_none() && c.collectee.is_none() && !c.declaration && !c.excluded && !c.non_resident {
                    return Err(BookError::InvalidTax("TCS needs a nature of collection or the collectee's terms".into()));
                }
                if c.nature.as_deref().is_some_and(|x| !code_ok(x)) {
                    return Err(BookError::InvalidTax("the TCS nature must be 1 to 16 characters".into()));
                }
                let has_certificate = c.certificate.is_some() || c.valid_from.is_some() || c.valid_to.is_some();
                if has_certificate && c.lower_rate.is_none() {
                    return Err(BookError::InvalidTax("a lower-collection certificate needs its rate".into()));
                }
                if c.lower_rate.is_some_and(|r| r.is_sign_negative() || r > Amount::ONE_HUNDRED) {
                    return Err(BookError::InvalidTax("a TCS rate is 0 to 100 percent".into()));
                }
                if c.certificate.as_deref().is_some_and(|x| x.trim().is_empty() || x.chars().count() > MAX_TDS_CERTIFICATE) {
                    return Err(BookError::InvalidTax("the certificate number must be 1 to 40 characters".into()));
                }
                if c.valid_from.zip(c.valid_to).is_some_and(|(from, to)| from > to) {
                    return Err(BookError::InvalidTax("the certificate ends before it starts".into()));
                }
                if c.permanent_establishment && !c.non_resident {
                    return Err(BookError::InvalidTax("a permanent establishment is recorded only for a non-resident".into()));
                }
            }
        }
        Ok(())
    }

    /// Replace a ledger's details (name, group, opening, credit days, tax ID).
    pub fn alter_ledger(&mut self, at: DateTime<Utc>, user: &str, ledger: Ledger) -> Result<(), BookError> {
        let current = self.ledgers.get(&ledger.id).ok_or(BookError::UnknownLedger(ledger.id))?;
        self.commit(at, user, AuditAction::Alter, format!("ledger:{}", ledger.id.0), Some(json(current)), Some(json(&ledger)))
    }

    /// Create or alter a unit of measure (by id).
    pub fn save_unit(&mut self, at: DateTime<Utc>, user: &str, unit: Unit) -> Result<UnitId, BookError> {
        let id = unit.id;
        let (action, before) = self.units.get(&id).map_or((AuditAction::Create, None), |c| (AuditAction::Alter, Some(json(c))));
        self.commit(at, user, action, format!("unit:{}", id.0), before, Some(json(&unit)))?;
        Ok(id)
    }

    pub fn delete_unit(&mut self, at: DateTime<Utc>, user: &str, id: UnitId) -> Result<(), BookError> {
        let current = self.units.get(&id).ok_or(BookError::UnknownUnit)?;
        self.commit(at, user, AuditAction::Delete, format!("unit:{}", id.0), Some(json(current)), None)
    }

    /// Create or alter a stock group (by id).
    pub fn save_stock_group(&mut self, at: DateTime<Utc>, user: &str, group: StockGroup) -> Result<StockGroupId, BookError> {
        let id = group.id;
        let (action, before) =
            self.stock_groups.get(&id).map_or((AuditAction::Create, None), |c| (AuditAction::Alter, Some(json(c))));
        self.commit(at, user, action, format!("stockgroup:{}", id.0), before, Some(json(&group)))?;
        Ok(id)
    }

    pub fn delete_stock_group(&mut self, at: DateTime<Utc>, user: &str, id: StockGroupId) -> Result<(), BookError> {
        let current = self.stock_groups.get(&id).ok_or(BookError::UnknownStockGroup)?;
        self.commit(at, user, AuditAction::Delete, format!("stockgroup:{}", id.0), Some(json(current)), None)
    }

    /// Create or alter a stock item (by id).
    pub fn save_stock_item(&mut self, at: DateTime<Utc>, user: &str, item: StockItem) -> Result<StockItemId, BookError> {
        let id = item.id;
        let (action, before) =
            self.stock_items.get(&id).map_or((AuditAction::Create, None), |c| (AuditAction::Alter, Some(json(c))));
        self.commit(at, user, action, format!("stockitem:{}", id.0), before, Some(json(&item)))?;
        Ok(id)
    }

    pub fn delete_stock_item(&mut self, at: DateTime<Utc>, user: &str, id: StockItemId) -> Result<(), BookError> {
        let current = self.stock_items.get(&id).ok_or(BookError::UnknownStockItem)?;
        self.commit(at, user, AuditAction::Delete, format!("stockitem:{}", id.0), Some(json(current)), None)
    }

    /// Bank reconciliation: record (or clear, with `None`) the date the bank
    /// cleared `ledger`'s line(s) on a voucher. Audited as a voucher alter.
    pub fn set_bank_date(
        &mut self,
        at: DateTime<Utc>,
        user: &str,
        voucher: VoucherId,
        ledger: LedgerId,
        date: Option<NaiveDate>,
    ) -> Result<(), BookError> {
        let current = self.vouchers.get(&voucher).ok_or(BookError::UnknownVoucher)?;
        if !current.entries.iter().any(|e| e.ledger == ledger) {
            return Err(BookError::UnknownLedger(ledger));
        }
        let mut after = current.clone();
        for e in after.entries.iter_mut().filter(|e| e.ledger == ledger) {
            e.bank_date = date;
        }
        self.commit(at, user, AuditAction::Alter, format!("voucher:{}", voucher.0), Some(json(current)), Some(json(&after)))
    }
}

/// Longest name of a voucher's number series ([`Voucher::series`]).
pub const MAX_SERIES_NAME: usize = 60;

/// A voucher's named series as compared for duplicate numbers: ignoring
/// case, `None` for the type's own series.
fn series_name(v: &Voucher) -> Option<String> {
    v.series.as_deref().map(|s| s.trim().to_lowercase())
}

/// Tax classes compare without spaces and case ("8471 30" is "847130").
pub fn normalise_tax_class(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_uppercase).collect()
}

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("model types always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voucher::Entry;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_760_000_000, 0).unwrap()
    }

    fn setup() -> (Book, LedgerId, LedgerId, LedgerId) {
        let mut book = Book::new("Demo Traders");
        let cash = book.group_by_name("Cash-in-Hand").unwrap().id;
        let bank = book.group_by_name("Bank Accounts").unwrap().id;
        let sales = book.group_by_name("Sales Accounts").unwrap().id;
        let c = book.create_ledger(now(), "admin", "Cash", cash).unwrap();
        let b = book.create_ledger(now(), "admin", "HDFC Bank", bank).unwrap();
        let s = book.create_ledger(now(), "admin", "Sales", sales).unwrap();
        (book, c, b, s)
    }

    fn voucher(kind: VoucherKind, number: &str, entries: Vec<Entry>) -> Voucher {
        Voucher {
            id: VoucherId::new(),
            kind,
            number: number.into(),
            date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            entries,
            narration: String::new(), cancelled: false, series: None, stock: Vec::new(),
        }
    }

    #[test]
    fn rejects_unbalanced_voucher() {
        let (mut book, c, _, s) = setup();
        let v = voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(100)), Entry::cr(s, dec!(99.99))]);
        assert_eq!(
            book.post_voucher(now(), "admin", v),
            Err(BookError::Voucher(VoucherError::Unbalanced { dr: dec!(100), cr: dec!(99.99) }))
        );
    }

    #[test]
    fn contra_only_between_cash_and_bank() {
        let (mut book, c, b, s) = setup();
        let ok = voucher(VoucherKind::Contra, "1", vec![Entry::dr(b, dec!(500)), Entry::cr(c, dec!(500))]);
        book.post_voucher(now(), "admin", ok).unwrap();
        let bad = voucher(VoucherKind::Contra, "2", vec![Entry::dr(c, dec!(5)), Entry::cr(s, dec!(5))]);
        assert_eq!(book.post_voucher(now(), "admin", bad), Err(BookError::ContraNonCashLedger));
    }

    #[test]
    fn voucher_numbers_unique_per_kind() {
        let (mut book, c, _, s) = setup();
        let a = voucher(VoucherKind::Sales, "INV-1", vec![Entry::dr(c, dec!(1)), Entry::cr(s, dec!(1))]);
        let b = voucher(VoucherKind::Sales, "inv-1", vec![Entry::dr(c, dec!(1)), Entry::cr(s, dec!(1))]);
        book.post_voucher(now(), "admin", a).unwrap();
        assert!(matches!(book.post_voucher(now(), "admin", b), Err(BookError::DuplicateVoucherNumber(_))));
    }

    #[test]
    fn trial_balance_balances() {
        let (mut book, c, b, s) = setup();
        book.post_voucher(now(), "u", voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(1180)), Entry::cr(s, dec!(1180))]))
            .unwrap();
        book.post_voucher(now(), "u", voucher(VoucherKind::Contra, "1", vec![Entry::dr(b, dec!(1000)), Entry::cr(c, dec!(1000))]))
            .unwrap();
        let tb = book.trial_balance();
        let dr: Amount = tb.iter().map(|r| r.debit).sum();
        let cr: Amount = tb.iter().map(|r| r.credit).sum();
        assert_eq!(dr, cr);
        assert_eq!(tb.iter().find(|r| r.name == "Cash").unwrap().debit, dec!(180));
    }

    #[test]
    fn edits_are_logged_and_tampering_is_detected() {
        let (mut book, c, _, s) = setup();
        let mut v = voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(10)), Entry::cr(s, dec!(10))]);
        book.post_voucher(now(), "clerk", v.clone()).unwrap();
        v.entries = vec![Entry::dr(c, dec!(12)), Entry::cr(s, dec!(12))];
        book.alter_voucher(now(), "clerk", v.clone()).unwrap();
        book.delete_voucher(now(), "manager", v.id).unwrap();

        let log = book.audit().entries();
        assert_eq!(log.len(), 3 + 3, "3 ledger creates + create/alter/delete");
        assert_eq!(log[4].action, AuditAction::Alter);
        assert!(log[4].before.is_some() && log[4].after.is_some());
        assert_eq!(book.audit().verify(), Ok(()));

        let mut tampered = book.audit().clone();
        let json = serde_json::to_string(&tampered).unwrap().replace("\"12\"", "\"13\"");
        tampered = serde_json::from_str(&json).unwrap();
        assert_eq!(tampered.verify(), Err(4));
    }

    #[test]
    fn default_chart_has_groups_tally_users_expect() {
        let book = Book::new("Demo");
        for name in ["Bank OCC A/c", "Bank OD A/c", "Branch / Divisions", "Sundry Debtors", "Suspense Account"] {
            assert!(book.group_by_name(name).is_some(), "missing {name}");
        }
        assert_eq!(book.group_by_name("Expenses (Direct)").unwrap().name, "Direct Expenses");
    }

    #[test]
    fn group_search_matches_substrings_and_aliases() {
        let book = Book::new("Demo");
        let names: Vec<_> = book.search_groups("direct").iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Direct Expenses", "Direct Incomes", "Indirect Expenses", "Indirect Incomes"]);
        let names: Vec<_> = book.search_groups("income (").iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Direct Incomes", "Indirect Incomes"]);
    }

    #[test]
    fn custom_group_inherits_from_parent() {
        let mut book = Book::new("Demo");
        let banks = book.group_by_name("Bank Accounts").unwrap().id;
        let g = book.create_group(now(), "admin", "Current Accounts", banks).unwrap();
        let group = book.group(g).unwrap();
        assert_eq!(group.parent, Some(banks));
        assert_eq!(group.nature, Nature::Assets);
        assert!(group.is_cash_or_bank && !group.affects_gross_profit);
        assert_eq!(book.primary_group(g).unwrap().name, "Current Assets");

        let direct = book.group_by_name("Direct Expenses").unwrap().id;
        let freight = book.create_group(now(), "admin", "Freight", direct).unwrap();
        assert!(book.group(freight).unwrap().affects_gross_profit);
        assert_eq!(book.group(freight).unwrap().nature, Nature::Expenses);

        let entry = &book.audit().entries()[0];
        assert_eq!(entry.entity, format!("group:{}", g.0));
        assert_eq!(entry.action, AuditAction::Create);
    }

    #[test]
    fn custom_group_rejects_clashes_and_unknown_parents() {
        let mut book = Book::new("Demo");
        let banks = book.group_by_name("Bank Accounts").unwrap().id;
        book.create_group(now(), "admin", "Current Accounts", banks).unwrap();
        assert_eq!(
            book.create_group(now(), "admin", "current accounts", banks),
            Err(BookError::DuplicateGroup("current accounts".into()))
        );
        assert!(matches!(book.create_group(now(), "admin", "Expenses (Direct)", banks), Err(BookError::DuplicateGroup(_))));
        assert_eq!(book.create_group(now(), "admin", "Orphans", GroupId::new()), Err(BookError::UnknownGroup));
        assert!(book.audit().entries().len() == 1, "rejected creates are not logged");
    }

    #[test]
    fn replay_restores_custom_groups() {
        let mut book = Book::new("Demo");
        let banks = book.group_by_name("Bank Accounts").unwrap().id;
        let current = book.create_group(now(), "admin", "Current Accounts", banks).unwrap();
        let nested = book.create_group(now(), "admin", "Salary Accounts", current).unwrap();
        let hdfc = book.create_ledger(now(), "admin", "HDFC Salary", nested).unwrap();
        let cash_group = book.group_by_name("Cash-in-Hand").unwrap().id;
        let cash = book.create_ledger(now(), "admin", "Cash", cash_group).unwrap();
        book.post_voucher(now(), "u", voucher(VoucherKind::Contra, "1", vec![Entry::dr(hdfc, dec!(5)), Entry::cr(cash, dec!(5))]))
            .unwrap();

        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        assert_eq!(replayed.group(nested), book.group(nested));
        assert_eq!(replayed.group_by_name("Salary Accounts").unwrap().parent, Some(current));
        assert_eq!(replayed.groups().count(), book.groups().count());
        assert!(replayed.is_cash_or_bank(hdfc));
        assert_eq!(replayed.trial_balance(), book.trial_balance());
    }

    #[test]
    fn replay_applies_group_deletes() {
        let mut book = Book::new("Demo");
        let banks = book.group_by_name("Bank Accounts").unwrap().id;
        let g = book.create_group(now(), "admin", "Temp", banks).unwrap();
        let before = json(book.group(g).unwrap());
        let mut log = book.audit().clone();
        log.record(now(), "admin", AuditAction::Delete, format!("group:{}", g.0), Some(before), None);
        let replayed = Book::replay("Demo", log).unwrap();
        assert!(replayed.group(g).is_none());
        assert!(replayed.group_by_name("Temp").is_none());
    }

    #[test]
    fn reapply_rebases_another_devices_changes() {
        let (base, c, _, s) = setup();
        let common = base.audit().entries().len();
        // Two devices start from the same trail and diverge offline.
        let mut server = Book::replay("Demo", base.audit().clone()).unwrap();
        let mut device = Book::replay("Demo", base.audit().clone()).unwrap();
        server.post_voucher(now(), "a", voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(5)), Entry::cr(s, dec!(5))])).unwrap();
        let rent = device.create_ledger(now(), "b", "Rent", device.group_by_name("Indirect Expenses").unwrap().id).unwrap();
        device.post_voucher(now(), "b", voucher(VoucherKind::Payment, "1", vec![Entry::dr(rent, dec!(9)), Entry::cr(c, dec!(9))])).unwrap();
        let clash = voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(7)), Entry::cr(s, dec!(7))]);
        device.post_voucher(now(), "b", clash).unwrap();

        let pending = &device.audit().entries()[common..];
        server.reapply(&pending[0]).unwrap(); // new ledger keeps its id
        server.reapply(&pending[1]).unwrap(); // payment referencing it
        assert!(matches!(server.reapply(&pending[2]), Err(BookError::DuplicateVoucherNumber(_))));

        assert_eq!(server.ledger(rent).unwrap().name, "Rent");
        assert_eq!(server.audit().verify(), Ok(()));
        let last = server.audit().entries().last().unwrap();
        assert_eq!(last.user, "b", "author is preserved");
        // The rebased trail replays to the same books.
        let again = Book::replay("Demo", server.audit().clone()).unwrap();
        assert_eq!(again.trial_balance(), server.trial_balance());
    }

    #[test]
    fn ledger_in_use_cannot_be_deleted() {
        let (mut book, c, _, s) = setup();
        book.post_voucher(now(), "u", voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(1)), Entry::cr(s, dec!(1))]))
            .unwrap();
        assert_eq!(book.delete_ledger(now(), "u", s), Err(BookError::LedgerInUse));
    }
    /// Forge one entry with a valid hash chain on top of a real trail and
    /// return the replay result.
    fn forge(book: &Book, action: AuditAction, entity: String, before: Option<serde_json::Value>, after: Option<serde_json::Value>) -> Result<Book, BookError> {
        let mut log = book.audit().clone();
        log.record(now(), "mallory", action, entity, before, after);
        Book::replay("Demo", log)
    }

    fn rejected(r: Result<Book, BookError>) -> BookError {
        match r {
            Err(BookError::InvalidEntry { reason, .. }) => *reason,
            other => panic!("forged entry was accepted: {:?}", other.map(|b| b.audit().entries().len())),
        }
    }

    #[test]
    fn replay_rejects_forged_entries_that_pass_the_hash_chain() {
        let (book, c, _, s) = setup();
        let ghost = LedgerId::new();
        let v = voucher(VoucherKind::Sales, "9", vec![Entry::dr(ghost, dec!(1)), Entry::cr(s, dec!(1))]);
        let e = rejected(forge(&book, AuditAction::Create, format!("voucher:{}", v.id.0), None, Some(json(&v))));
        assert_eq!(e, BookError::UnknownLedger(ghost), "dangling ledger reference");

        // A group that is its own parent.
        let gid = GroupId::new();
        let g = Group { id: gid, name: "Loop".into(), parent: Some(gid), nature: Nature::Assets, aliases: vec![], affects_gross_profit: false, is_cash_or_bank: false };
        assert_eq!(rejected(forge(&book, AuditAction::Create, format!("group:{}", gid.0), None, Some(json(&g)))), BookError::UnknownGroup);

        // Entity id differs from the payload id.
        let v = voucher(VoucherKind::Sales, "9", vec![Entry::dr(c, dec!(1)), Entry::cr(s, dec!(1))]);
        let other = VoucherId::new();
        assert!(matches!(rejected(forge(&book, AuditAction::Create, format!("voucher:{}", other.0), None, Some(json(&v)))), BookError::EntityMismatch(_)));

        // Overwriting a default group with a Create.
        let cash = book.group_by_name("Cash-in-Hand").unwrap().clone();
        let hijack = Group { name: "Mine".into(), ..cash.clone() };
        assert!(matches!(rejected(forge(&book, AuditAction::Create, format!("group:{}", cash.id.0), None, Some(json(&hijack)))), BookError::AlreadyExists(_)));

        // Deleting a primary group, or one with ledgers.
        assert_eq!(rejected(forge(&book, AuditAction::Delete, format!("group:{}", cash.id.0), Some(json(&cash)), None)), BookError::GroupInUse);

        // Alter whose `before` does not match the current record.
        let ledger = book.ledger(c).unwrap().clone();
        let fake_before = Ledger { name: "Not the real name".into(), ..ledger.clone() };
        let after = Ledger { opening: dec!(1000000), ..ledger.clone() };
        assert!(matches!(rejected(forge(&book, AuditAction::Alter, format!("ledger:{}", c.0), Some(json(&fake_before)), Some(json(&after)))), BookError::StaleBefore(_)));

        // Duplicate voucher number and contra rule are enforced on replay too.
        let mut b2 = book.clone();
        b2.post_voucher(now(), "u", voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(1)), Entry::cr(s, dec!(1))])).unwrap();
        let dup = voucher(VoucherKind::Sales, "1", vec![Entry::dr(c, dec!(2)), Entry::cr(s, dec!(2))]);
        assert!(matches!(rejected(forge(&b2, AuditAction::Create, format!("voucher:{}", dup.id.0), None, Some(json(&dup)))), BookError::DuplicateVoucherNumber(_)));
        let contra = voucher(VoucherKind::Contra, "1", vec![Entry::dr(c, dec!(2)), Entry::cr(s, dec!(2))]);
        assert_eq!(rejected(forge(&book, AuditAction::Create, format!("voucher:{}", contra.id.0), None, Some(json(&contra)))), BookError::ContraNonCashLedger);

        // Deleting a ledger that vouchers use.
        let sales = b2.ledger(s).unwrap().clone();
        assert_eq!(rejected(forge(&b2, AuditAction::Delete, format!("ledger:{}", s.0), Some(json(&sales)), None)), BookError::LedgerInUse);

        // Decimal overflow is an error, not a panic.
        let huge = voucher(VoucherKind::Sales, "8", vec![Entry::dr(c, Amount::MAX), Entry::dr(c, Amount::MAX), Entry::cr(s, Amount::MAX)]);
        assert_eq!(rejected(forge(&book, AuditAction::Create, format!("voucher:{}", huge.id.0), None, Some(json(&huge)))), BookError::Voucher(VoucherError::Overflow));
    }

    #[test]
    fn apply_entry_rejects_broken_links() {
        let (book, ..) = setup();
        let mut copy = Book::replay("Demo", AuditLog::default()).unwrap();
        let mut skipped = book.audit().entries()[1].clone();
        assert_eq!(copy.apply_entry(&skipped), Err(BookError::BrokenAuditTrail(1)));
        skipped.seq = 0;
        assert_eq!(copy.apply_entry(&skipped), Err(BookError::BrokenAuditTrail(0)), "wrong prev_hash / hash");
    }

    #[test]
    fn cancelled_vouchers_keep_their_number_but_not_their_effect() {
        let (mut book, c, _, s) = setup();
        let v = voucher(VoucherKind::Sales, "INV-1", vec![Entry::dr(c, dec!(100)), Entry::cr(s, dec!(100))]);
        let id = book.post_voucher(now(), "u", v).unwrap();
        book.cancel_voucher(now(), "u", id).unwrap();
        assert!(book.trial_balance().is_empty(), "no effect on balances");
        assert!(book.voucher(id).unwrap().cancelled);
        // The number stays taken, and the voucher cannot be altered or re-cancelled.
        let again = voucher(VoucherKind::Sales, "INV-1", vec![Entry::dr(c, dec!(1)), Entry::cr(s, dec!(1))]);
        assert!(matches!(book.post_voucher(now(), "u", again), Err(BookError::DuplicateVoucherNumber(_))));
        let mut edit = book.voucher(id).unwrap().clone();
        edit.cancelled = false;
        assert_eq!(book.alter_voucher(now(), "u", edit), Err(BookError::VoucherCancelled));
        assert_eq!(book.cancel_voucher(now(), "u", id), Err(BookError::VoucherCancelled));
        // It survives replay.
        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        assert!(replayed.voucher(id).unwrap().cancelled);
        assert!(replayed.trial_balance().is_empty());
    }

    #[test]
    fn credit_days_are_audited() {
        let (mut book, c, ..) = setup();
        book.set_credit_days(now(), "u", c, Some(30)).unwrap();
        assert_eq!(book.ledger(c).unwrap().credit_days, Some(30));
        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        assert_eq!(replayed.ledger(c).unwrap().credit_days, Some(30));
    }

    #[test]
    fn tax_ids_and_overrides_are_audited() {
        let (mut book, c, ..) = setup();
        let gstin = Some("27AAPFU0939F1ZV".to_owned());
        book.set_tax_id(now(), "u", c, gstin.clone(), None).unwrap();
        book.set_tax_id(now(), "manager", c, Some("27AAPFU0939F1ZX".into()), Some("UIN from the portal".into())).unwrap();
        let last = book.audit().entries().last().unwrap();
        assert_eq!(last.user, "manager");
        assert_eq!(last.after.as_ref().unwrap()["tax_id_override"], "UIN from the portal");
        assert_eq!(last.before.as_ref().unwrap()["tax_id"], "27AAPFU0939F1ZV");
        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        assert_eq!(replayed.ledger(c), book.ledger(c));

        // Clearing the ID clears its override too.
        book.set_tax_id(now(), "u", c, None, Some("ignored".into())).unwrap();
        assert_eq!((book.ledger(c).unwrap().tax_id.clone(), book.ledger(c).unwrap().tax_id_override.clone()), (None, None));

        // An override without an ID cannot be smuggled in on replay.
        let ledger = book.ledger(c).unwrap().clone();
        let forged = Ledger { tax_id_override: Some("x".into()), ..ledger.clone() };
        let entity = format!("ledger:{}", c.0);
        assert!(matches!(rejected(forge(&book, AuditAction::Alter, entity, Some(json(&ledger)), Some(json(&forged)))), BookError::UnreplayableEntry(_)));
    }

    #[test]
    fn ledgers_without_tax_fields_keep_their_old_json() {
        // Trails written before tax IDs existed must still match `before`.
        let (book, c, ..) = setup();
        let v = json(book.ledger(c).unwrap());
        assert!(v.get("tax_id").is_none() && v.get("tax_id_override").is_none());
    }

    #[test]
    fn pan_and_tds_settings_are_audited_and_checked() {
        use crate::chart::{DeducteeKind, LedgerTds, TdsTerms};
        let (mut book, c, ..) = setup();
        let creditors = book.group_by_name("Sundry Creditors").unwrap().id;
        let duties = book.group_by_name("Duties & Taxes").unwrap().id;
        let expenses = book.group_by_name("Indirect Expenses").unwrap().id;
        let party = book.create_ledger(now(), "u", "Mehta Builders", creditors).unwrap();
        let labour = book.create_ledger(now(), "u", "Contract Labour", expenses).unwrap();
        let payable = book.create_ledger(now(), "u", "TDS Payable 194C", duties).unwrap();

        // Old JSON has neither field; new ledgers do not write them either.
        let v = json(book.ledger(party).unwrap());
        assert!(v.get("pan").is_none() && v.get("tds").is_none());

        let terms = TdsTerms {
            deductee: Some(DeducteeKind::Firm),
            lower_rate: Some(dec!(0.5)),
            certificate: Some("LDC123".into()),
            valid_from: NaiveDate::from_ymd_opt(2026, 4, 1),
            valid_to: NaiveDate::from_ymd_opt(2027, 3, 31),
            ..TdsTerms::default()
        };
        let p = Ledger { pan: Some("AAPFU0939F".into()), tds: Some(LedgerTds::Deductible(terms.clone())), ..book.ledger(party).unwrap().clone() };
        book.alter_ledger(now(), "u", p).unwrap();
        let l = Ledger { tds: Some(LedgerTds::Deductible(TdsTerms { nature: Some("contract".into()), ..TdsTerms::default() })), ..book.ledger(labour).unwrap().clone() };
        book.alter_ledger(now(), "u", l).unwrap();
        let t = Ledger { tds: Some(LedgerTds::Payable { payable_for: "contract".into() }), ..book.ledger(payable).unwrap().clone() };
        book.alter_ledger(now(), "u", t).unwrap();
        let v = json(book.ledger(party).unwrap());
        assert_eq!((v["pan"].as_str(), v["tds"]["kind"].as_str(), v["tds"]["deductee"].as_str()), (Some("AAPFU0939F"), Some("Deductible"), Some("Firm")));
        assert_eq!(json(book.ledger(payable).unwrap())["tds"], serde_json::json!({ "kind": "Payable", "payable_for": "contract" }));
        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        for id in [party, labour, payable] {
            assert_eq!(replayed.ledger(id), book.ledger(id));
        }

        let base = book.ledger(party).unwrap().clone();
        let refuse = |book: &mut Book, l: Ledger| match book.alter_ledger(now(), "u", l) {
            Err(BookError::InvalidTax(why)) => why,
            other => panic!("{other:?}"),
        };
        // PAN: only length and characters are the core's business.
        for bad in ["AAPFU0939", "aapfu0939f", "AAPFU-939F", "AAPFU0939FF"] {
            assert!(refuse(&mut book, Ledger { pan: Some(bad.into()), ..base.clone() }).contains("PAN"), "{bad}");
        }
        // A payable tag only under Duties & Taxes, and not on a GST ledger.
        let tag = Some(LedgerTds::Payable { payable_for: "contract".into() });
        assert!(refuse(&mut book, Ledger { tds: tag.clone(), ..base.clone() }).contains("Duties & Taxes"));
        let gst = crate::chart::LedgerTax { component: "IGST".into(), side: crate::chart::TaxSide::Input };
        let both = Ledger { tax: Some(gst), ..book.ledger(payable).unwrap().clone() };
        assert!(refuse(&mut book, both).contains("another tax"));
        let empty = Ledger { tds: Some(LedgerTds::Payable { payable_for: " ".into() }), ..book.ledger(payable).unwrap().clone() };
        assert!(refuse(&mut book, empty).contains("1 to 16"));
        // Deductible terms must say something and hold together.
        let with = |t: TdsTerms| Ledger { tds: Some(LedgerTds::Deductible(t)), ..base.clone() };
        assert!(refuse(&mut book, with(TdsTerms::default())).contains("nature of payment or a deductee"));
        let no_rate = TdsTerms { lower_rate: None, ..terms.clone() };
        assert!(refuse(&mut book, with(no_rate)).contains("needs its rate"));
        let too_high = TdsTerms { lower_rate: Some(dec!(101)), ..terms.clone() };
        assert!(refuse(&mut book, with(too_high)).contains("0 to 100"));
        let backwards = TdsTerms { valid_to: NaiveDate::from_ymd_opt(2026, 3, 31), ..terms.clone() };
        assert!(refuse(&mut book, with(backwards)).contains("ends before"));
        let long = TdsTerms { nature: Some("x".repeat(17)), ..terms };
        assert!(refuse(&mut book, with(long)).contains("1 to 16"));
        // Nothing above reached the trail.
        assert_eq!(book.ledger(party), Some(&base));
        assert!(book.ledger(c).unwrap().tds.is_none());
    }

    #[test]
    fn tcs_and_non_resident_settings_are_audited_and_checked() {
        use crate::chart::{DeducteeKind, LedgerTcs, LedgerTds, NonResidentTerms, TcsTerms, TdsTerms};
        let (mut book, ..) = setup();
        let debtors = book.group_by_name("Sundry Debtors").unwrap().id;
        let duties = book.group_by_name("Duties & Taxes").unwrap().id;
        let sales = book.group_by_name("Sales Accounts").unwrap().id;
        let buyer = book.create_ledger(now(), "u", "Kabadi Traders", debtors).unwrap();
        let scrap = book.create_ledger(now(), "u", "Scrap Sales", sales).unwrap();
        let payable = book.create_ledger(now(), "u", "TCS Payable", duties).unwrap();

        // Old JSON has no tcs field, and new ledgers do not write one.
        assert!(json(book.ledger(buyer).unwrap()).get("tcs").is_none());
        let old: Ledger = serde_json::from_value(json(book.ledger(buyer).unwrap())).unwrap();
        assert_eq!(old.tcs, None);

        let terms = TcsTerms {
            collectee: Some(DeducteeKind::Firm),
            lower_rate: Some(dec!(0.5)),
            certificate: Some("LCC1".into()),
            declaration: true,
            ..TcsTerms::default()
        };
        // A party can be a deductee and a collectee at once.
        let tds = Some(LedgerTds::Deductible(TdsTerms { deductee: Some(DeducteeKind::Firm), ..TdsTerms::default() }));
        let b = Ledger { tcs: Some(LedgerTcs::Collectible(terms.clone())), tds, ..book.ledger(buyer).unwrap().clone() };
        book.alter_ledger(now(), "u", b).unwrap();
        let s = Ledger { tcs: Some(LedgerTcs::Collectible(TcsTerms { nature: Some("scrap".into()), ..TcsTerms::default() })), ..book.ledger(scrap).unwrap().clone() };
        book.alter_ledger(now(), "u", s).unwrap();
        let p = Ledger { tcs: Some(LedgerTcs::Payable { payable_for: "all".into() }), ..book.ledger(payable).unwrap().clone() };
        book.alter_ledger(now(), "u", p).unwrap();
        let v = json(book.ledger(buyer).unwrap());
        assert_eq!((v["tcs"]["kind"].as_str(), v["tcs"]["declaration"].as_bool(), v["tcs"].get("excluded")), (Some("Collectible"), Some(true), None));
        assert_eq!(json(book.ledger(payable).unwrap())["tcs"], serde_json::json!({ "kind": "Payable", "payable_for": "all" }));
        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        for id in [buyer, scrap, payable] {
            assert_eq!(replayed.ledger(id), book.ledger(id));
        }

        let refuse = |book: &mut Book, l: Ledger| match book.alter_ledger(now(), "u", l) {
            Err(BookError::InvalidTax(why)) => why,
            other => panic!("{other:?}"),
        };
        let base = book.ledger(buyer).unwrap().clone();
        let with = |t: TcsTerms| Ledger { tcs: Some(LedgerTcs::Collectible(t)), ..base.clone() };
        assert!(refuse(&mut book, with(TcsTerms::default())).contains("nature of collection"));
        assert!(refuse(&mut book, with(TcsTerms { lower_rate: None, ..terms.clone() })).contains("needs its rate"));
        assert!(refuse(&mut book, with(TcsTerms { lower_rate: Some(dec!(-1)), ..terms.clone() })).contains("0 to 100"));
        assert!(refuse(&mut book, with(TcsTerms { permanent_establishment: true, ..terms.clone() })).contains("non-resident"));
        assert!(refuse(&mut book, with(TcsTerms { nature: Some("x".repeat(17)), ..terms })).contains("1 to 16"));
        let tag = Some(LedgerTcs::Payable { payable_for: "scrap".into() });
        assert!(refuse(&mut book, Ledger { tcs: tag.clone(), ..base.clone() }).contains("Duties & Taxes"));
        let both = Ledger { tds: Some(LedgerTds::Payable { payable_for: "all".into() }), ..book.ledger(payable).unwrap().clone() };
        assert!(refuse(&mut book, both).contains("another tax"));

        // Non-resident terms on a TDS deductee: rate and code sizes only.
        let nr = |n: NonResidentTerms| Ledger {
            tds: Some(LedgerTds::Deductible(TdsTerms { nature: Some("non_resident".into()), non_resident: Some(n), ..TdsTerms::default() })),
            ..base.clone()
        };
        let good = NonResidentTerms { rate: dec!(10), treaty: true, country: Some("267".into()), remittance: Some("21".into()), foreign_tin: Some("12-3456789".into()) };
        book.alter_ledger(now(), "u", nr(good.clone())).unwrap();
        assert_eq!(json(book.ledger(buyer).unwrap())["tds"]["non_resident"]["treaty"], serde_json::json!(true));
        assert!(refuse(&mut book, nr(NonResidentTerms { rate: dec!(120), ..good.clone() })).contains("0 to 100"));
        assert!(refuse(&mut book, nr(NonResidentTerms { country: Some("12345".into()), ..good.clone() })).contains("1 to 4"));
        assert!(refuse(&mut book, nr(NonResidentTerms { foreign_tin: Some(" ".into()), ..good })).contains("1 to 25"));
    }

    #[test]
    fn country_lives_in_the_trail() {
        let (mut book, ..) = setup();
        assert_eq!(book.country(), None);
        book.set_country(now(), "u", "IN").unwrap();
        book.set_country(now(), "u", "AE").unwrap();
        assert_eq!(book.country(), Some("AE"));
        let log = book.audit().entries();
        assert_eq!((log[log.len() - 2].action, log[log.len() - 1].action), (AuditAction::Create, AuditAction::Alter));
        assert_eq!(log[log.len() - 1].entity, "profile:company");
        assert_eq!(Book::replay("Demo", book.audit().clone()).unwrap().country(), Some("AE"));
        assert_eq!(book.set_country(now(), "u", "India"), Err(BookError::InvalidCountry("India".into())));
        assert_eq!(book.set_country(now(), "u", "in"), Err(BookError::InvalidCountry("in".into())));
    }

    #[test]
    fn profile_entries_are_checked_on_replay() {
        let (mut book, ..) = setup();
        let p = Profile { country: "IN".into(), ..Profile::default() };
        // Alter before any profile exists, and a profile under another id.
        assert!(matches!(rejected(forge(&book, AuditAction::Alter, PROFILE_ENTITY.into(), Some(json(&p)), Some(json(&p)))), BookError::UnreplayableEntry(_)));
        assert!(matches!(rejected(forge(&book, AuditAction::Create, "profile:other".into(), None, Some(json(&p)))), BookError::UnreplayableEntry(_)));
        // A second Create, and an Alter with a stale `before`.
        book.set_country(now(), "u", "IN").unwrap();
        assert!(matches!(rejected(forge(&book, AuditAction::Create, PROFILE_ENTITY.into(), None, Some(json(&p)))), BookError::AlreadyExists(_)));
        let stale = Profile { country: "AE".into(), ..Profile::default() };
        assert!(matches!(rejected(forge(&book, AuditAction::Alter, PROFILE_ENTITY.into(), Some(json(&stale)), Some(json(&stale)))), BookError::StaleBefore(_)));
    }

    #[test]
    fn reapply_rebases_a_country_change() {
        let (base, ..) = setup();
        let mut server = Book::replay("Demo", base.audit().clone()).unwrap();
        let mut device = Book::replay("Demo", base.audit().clone()).unwrap();
        device.set_country(now(), "b", "IN").unwrap();
        server.reapply(device.audit().entries().last().unwrap()).unwrap();
        assert_eq!(server.country(), Some("IN"));
    }

    #[test]
    fn schedule3_settings_live_in_the_trail_and_survive_replay() {
        let (mut book, cash, ..) = setup();
        let loans = GroupId::default_named("Secured Loans");
        let ghost = GroupId::new();
        assert!(matches!(book.set_schedule3_mapping(now(), "u", loans, Some("x")), Err(BookError::InvalidStatementSetting(_))), "no profile yet");
        book.set_country(now(), "u", "IN").unwrap();
        book.set_schedule3_mapping(now(), "u", loans, Some("short_term_borrowings:current_maturities")).unwrap();
        book.set_schedule3_mapping(now(), "u", GroupId::default_named("Sundry Creditors"), Some("trade_payables")).unwrap();
        book.set_msme_party(now(), "u", cash, true).unwrap();
        book.set_schedule3_format(now(), "u", Some(2), Some("lakhs")).unwrap();
        assert_eq!(book.set_schedule3_mapping(now(), "u", ghost, Some("trade_payables")), Err(BookError::UnknownGroup));
        assert!(matches!(book.set_schedule3_mapping(now(), "u", loans, Some("Short Term")), Err(BookError::InvalidStatementSetting(_))));
        assert!(matches!(book.set_schedule3_format(now(), "u", Some(0), None), Err(BookError::InvalidStatementSetting(_))));
        assert_eq!(book.set_msme_party(now(), "u", LedgerId::new(), true).map_err(|e| matches!(e, BookError::UnknownLedger(_))), Err(true));
        book.set_schedule3_mapping(now(), "u", GroupId::default_named("Sundry Creditors"), None).unwrap();

        let replayed = Book::replay("Demo", book.audit().clone()).unwrap();
        let p = replayed.profile().unwrap();
        assert_eq!(p, book.profile().unwrap());
        assert_eq!(p.schedule3_map.len(), 1);
        assert_eq!(p.schedule3_map[&loans], "short_term_borrowings:current_maturities");
        assert!(p.msme_parties.contains(&cash));
        assert_eq!((p.schedule3_division, p.schedule3_rounding.as_deref()), (Some(2), Some("lakhs")));
        // Unset fields stay out of the trail, so older profiles read the same.
        let plain = serde_json::to_value(Profile { country: "IN".into(), ..Profile::default() }).unwrap();
        assert!(plain.get("schedule3_map").is_none() && plain.get("msme_parties").is_none());

        // A forged profile with a malformed code is refused on replay.
        let mut bad = p.clone();
        bad.schedule3_map.insert(loans, "DROP TABLE".into());
        assert!(matches!(
            rejected(forge(&book, AuditAction::Alter, PROFILE_ENTITY.into(), Some(json(p)), Some(json(&bad)))),
            BookError::InvalidStatementSetting(_)
        ));
    }

}

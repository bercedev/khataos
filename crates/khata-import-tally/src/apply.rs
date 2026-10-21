//! Replaying parsed Tally objects into a [`Book`].

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use khata_core::inventory::MAX_BATCH;
use khata_core::{
    Amount, Book, BookError, Entry, Godown, GodownId, GroupId, LedgerId, Quantity, StockGroup, StockGroupId, StockItem,
    StockItemId, StockLine, Unit, UnitId, Voucher, VoucherError, VoucherId, VoucherKind,
};

use crate::parse::{TallyData, TallyGroup, TallyStockLine, TallyVoucher};
use crate::text::normalize;
use crate::{ImportReport, ItemKind, SkipReason, Skipped};

/// Tally names for predefined groups that differ from the KhataOS default
/// name beyond case and punctuation, as `(tally, khataos)`.
const GROUP_NAMES: &[(&str, &str)] = &[
    ("Suspense A/c", "Suspense Account"),
    ("Suspense", "Suspense Account"),
    ("Retained Earnings", "Reserves & Surplus"),
    ("Bank Overdraft", "Bank OD A/c"),
    ("Bank Overdraft A/c", "Bank OD A/c"),
    ("Bank OCC", "Bank OCC A/c"),
    ("Sales Account", "Sales Accounts"),
    ("Purchase Account", "Purchase Accounts"),
    ("Branch/Divisions", "Branch / Divisions"),
];

/// Tally's predefined accounting voucher types.
const BASE_TYPES: &[(&str, VoucherKind)] = &[
    ("Contra", VoucherKind::Contra),
    ("Payment", VoucherKind::Payment),
    ("Receipt", VoucherKind::Receipt),
    ("Journal", VoucherKind::Journal),
    ("Sales", VoucherKind::Sales),
    ("Purchase", VoucherKind::Purchase),
    ("Credit Note", VoucherKind::CreditNote),
    ("Debit Note", VoucherKind::DebitNote),
];

/// Tally's voucher types that move stock without (or beyond) Sales,
/// Purchase, Credit Note and Debit Note.
const INVENTORY_TYPES: &[&str] = &[
    "Stock Journal",
    "Manufacturing Journal",
    "Delivery Note",
    "Receipt Note",
    "Rejections In",
    "Rejections Out",
    "Physical Stock",
    "Material In",
    "Material Out",
];

/// Tally's predefined non-accounting (or not yet supported) voucher types.
const UNSUPPORTED_TYPES: &[&str] = &[
    "Memorandum",
    "Reversing Journal",
    "Job Work In Order",
    "Job Work Out Order",
    "Sales Order",
    "Purchase Order",
    "Payroll",
    "Attendance",
];

/// The ledger Tally keeps at Primary for accumulated profit; KhataOS files
/// it under Reserves & Surplus so its opening balance still tallies.
const PROFIT_AND_LOSS: &str = "profitlossac";

struct Importer<'a> {
    book: &'a mut Book,
    at: DateTime<Utc>,
    user: &'a str,
    report: ImportReport,
    /// Normalized Tally group name or alias → group in the book.
    groups: HashMap<String, GroupId>,
    /// Lower-cased ledger name or alias → ledger in the book.
    ledgers: HashMap<String, LedgerId>,
    /// Normalized voucher type name → its parent type name.
    voucher_types: HashMap<String, String>,
    /// Lower-cased unit symbol → unit in the book.
    units: HashMap<String, UnitId>,
    /// Lower-cased symbols of compound units (not imported).
    compound_units: HashSet<String>,
    /// Lower-cased unit symbol → most decimal places its quantities use.
    seen_decimals: HashMap<String, u32>,
    /// Lower-cased stock group name or alias → stock group in the book.
    stock_groups: HashMap<String, StockGroupId>,
    /// Lower-cased stock item name or alias → stock item in the book.
    stock_items: HashMap<String, StockItemId>,
    /// Lower-cased godown name or alias → godown in the book.
    godowns: HashMap<String, GodownId>,
    /// Items whose batch allocations were merged into their line.
    merged_detail: Vec<String>,
}

/// A named master in a hierarchy (stock group, godown).
struct TreeNode<'a> {
    name: &'a str,
    aliases: &'a [String],
    parent: Option<&'a str>,
}

enum TreeSkip {
    UnknownParent(String),
    Cycle(String),
    Rejected(BookError),
}

/// Create `nodes` under their parents once those exist, so parents may
/// appear anywhere in the file. A node already in the book (`existing`) is
/// reused. Returns how many were created and what was left out.
fn tree<Id: Copy>(
    book: &mut Book,
    nodes: &[TreeNode<'_>],
    map: &mut HashMap<String, Id>,
    existing: impl Fn(&Book, &str) -> Option<Id>,
    create: impl Fn(&mut Book, &str, Option<Id>) -> Result<Id, BookError>,
) -> (usize, Vec<(String, TreeSkip)>) {
    let remember = |map: &mut HashMap<String, Id>, n: &TreeNode<'_>, id| {
        for name in std::iter::once(n.name).chain(n.aliases.iter().map(String::as_str)) {
            map.entry(name.to_lowercase()).or_insert(id);
        }
    };
    let mut created = 0;
    let mut skipped = Vec::new();
    let mut pending: Vec<&TreeNode<'_>> = Vec::new();
    for n in nodes {
        match existing(book, n.name) {
            Some(id) => remember(map, n, id),
            None => pending.push(n),
        }
    }
    loop {
        let before = pending.len();
        let mut waiting = Vec::new();
        for n in pending {
            let parent = match n.parent {
                None => None,
                Some(p) => match map.get(&p.to_lowercase()) {
                    Some(&id) => Some(id),
                    None => {
                        waiting.push(n);
                        continue;
                    }
                },
            };
            if let Some(&id) = map.get(&n.name.to_lowercase()) {
                remember(map, n, id);
                continue;
            }
            match create(book, n.name, parent) {
                Ok(id) => {
                    remember(map, n, id);
                    created += 1;
                }
                Err(e) => skipped.push((n.name.to_owned(), TreeSkip::Rejected(e))),
            }
        }
        pending = waiting;
        if pending.is_empty() || pending.len() == before {
            break;
        }
    }
    // Whatever is left waits on a missing parent or on a loop.
    let stuck: HashSet<String> = pending.iter().map(|n| n.name.to_lowercase()).collect();
    for n in pending {
        let parent = n.parent.unwrap_or_default().to_owned();
        let why = if stuck.contains(&parent.to_lowercase()) {
            TreeSkip::Cycle(parent)
        } else {
            TreeSkip::UnknownParent(parent)
        };
        skipped.push((n.name.to_owned(), why));
    }
    (created, skipped)
}

pub(crate) fn apply(mut data: TallyData, book: &mut Book, at: DateTime<Utc>, user: &str) -> ImportReport {
    let mut im = Importer {
        book,
        at,
        user,
        report: ImportReport::default(),
        groups: HashMap::new(),
        ledgers: HashMap::new(),
        voucher_types: data
            .voucher_types
            .iter()
            .filter_map(|t| Some((normalize(&t.name), t.parent.clone()?)))
            .collect(),
        units: HashMap::new(),
        compound_units: HashSet::new(),
        seen_decimals: seen_decimals(&data),
        stock_groups: HashMap::new(),
        stock_items: HashMap::new(),
        godowns: HashMap::new(),
        merged_detail: Vec::new(),
    };
    im.report.skipped.append(&mut data.rejected);
    im.groups(&data.groups);
    im.ledgers(&data);
    im.units(&data);
    im.stock_groups(&data);
    im.stock_items(&data);
    im.godowns(&data);
    // Numbered vouchers first, so numbers KhataOS hands out to unnumbered
    // ones can never take a number Tally already used.
    let (numbered, unnumbered): (Vec<_>, Vec<_>) = data.vouchers.iter().partition(|v| !v.number.is_empty());
    for v in numbered.into_iter().chain(unnumbered) {
        im.voucher(v);
    }
    for name in std::mem::take(&mut im.merged_detail) {
        im.skip(ItemKind::Inventory, name, SkipReason::BatchDetailMerged);
    }
    im.report
}

/// Decimal places a quantity needs, capped at the 4 KhataOS allows.
fn decimals(q: Quantity) -> u32 {
    q.normalize().scale().min(4)
}

/// Per unit symbol, the most decimal places used by quantities in the data
/// (item openings and voucher lines), for units Tally gave no
/// `DECIMALPLACES`.
fn seen_decimals(data: &TallyData) -> HashMap<String, u32> {
    let mut item_unit: HashMap<String, String> = HashMap::new();
    let mut out: HashMap<String, u32> = HashMap::new();
    for i in &data.stock_items {
        let Some(unit) = i.unit.as_deref().map(str::to_lowercase) else {
            continue;
        };
        for name in std::iter::once(&i.name).chain(&i.aliases) {
            item_unit.entry(name.to_lowercase()).or_insert_with(|| unit.clone());
        }
        let d = out.entry(unit).or_default();
        *d = (*d).max(decimals(i.opening_qty));
    }
    for l in data.vouchers.iter().flat_map(|v| &v.inventory) {
        if let Some(unit) = item_unit.get(&l.item.to_lowercase()) {
            let d = out.entry(unit.clone()).or_default();
            *d = l.batches.iter().map(|b| decimals(b.qty)).fold((*d).max(decimals(l.qty)), u32::max);
        }
    }
    out
}

impl Importer<'_> {
    fn skip(&mut self, kind: ItemKind, name: impl Into<String>, reason: SkipReason) {
        self.report.skipped.push(Skipped { kind, name: name.into(), reason });
    }

    /// A KhataOS default group matching a Tally predefined group name.
    fn default_group(&self, name: &str) -> Option<GroupId> {
        let n = normalize(name);
        let n = GROUP_NAMES.iter().find(|(t, _)| normalize(t) == n).map_or(n, |(_, k)| normalize(k));
        self.book
            .groups()
            .filter(|g| g.id == GroupId::default_named(&g.name))
            .find(|g| normalize(&g.name) == n || g.aliases.iter().any(|a| normalize(a) == n))
            .map(|g| g.id)
    }

    fn resolve_group(&self, name: &str) -> Option<GroupId> {
        self.groups
            .get(&normalize(name))
            .copied()
            .or_else(|| self.book.group_by_name(name).map(|g| g.id))
            .or_else(|| self.default_group(name))
    }

    fn remember_group(&mut self, g: &TallyGroup, id: GroupId) {
        for name in std::iter::once(&g.name).chain(&g.aliases) {
            self.groups.entry(normalize(name)).or_insert(id);
        }
    }

    /// Predefined groups map onto KhataOS defaults; custom groups are
    /// created under their parent once it exists, so parents may appear
    /// anywhere in the file.
    fn groups(&mut self, groups: &[TallyGroup]) {
        let mut pending: Vec<&TallyGroup> = Vec::new();
        for g in groups {
            let reserved = g.reserved.as_deref().and_then(|r| self.default_group(r));
            match reserved.or_else(|| self.default_group(&g.name)) {
                Some(id) => {
                    self.remember_group(g, id);
                    self.report.groups_matched += 1;
                }
                None if g.parent.is_none() => self.skip(ItemKind::Group, &g.name, SkipReason::CustomPrimaryGroup),
                None => pending.push(g),
            }
        }
        loop {
            let before = pending.len();
            let mut waiting = Vec::new();
            for g in pending {
                let parent = g.parent.as_deref().expect("custom primary groups were skipped");
                // Only resolve against groups that already exist in the
                // book or were imported, never a sibling still pending.
                let Some(parent_id) = self.resolve_group(parent) else {
                    waiting.push(g);
                    continue;
                };
                if let Some(existing) = self.book.group_by_name(&g.name).map(|x| x.id) {
                    self.remember_group(g, existing);
                    self.report.groups_matched += 1;
                    continue;
                }
                match self.book.create_group(self.at, self.user, &g.name, parent_id) {
                    Ok(id) => {
                        self.remember_group(g, id);
                        self.report.groups_created += 1;
                    }
                    Err(e) => self.skip(ItemKind::Group, &g.name, SkipReason::Rejected(e)),
                }
            }
            pending = waiting;
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        for g in pending {
            let parent = g.parent.clone().unwrap_or_default();
            self.skip(ItemKind::Group, &g.name, SkipReason::UnknownParentGroup(parent));
        }
    }

    fn ledgers(&mut self, data: &TallyData) {
        for l in &data.ledgers {
            if let Some(existing) = self.book.ledger_by_name(&l.name).map(|x| x.id) {
                self.ledgers.insert(l.name.to_lowercase(), existing);
                self.skip(ItemKind::Ledger, &l.name, SkipReason::DuplicateLedger);
                continue;
            }
            let group = match l.parent.as_deref() {
                Some(parent) => {
                    self.resolve_group(parent).ok_or_else(|| SkipReason::UnknownParentGroup(parent.to_owned()))
                }
                None if normalize(&l.name) == PROFIT_AND_LOSS
                    || l.reserved.as_deref().is_some_and(|r| normalize(r) == PROFIT_AND_LOSS) =>
                {
                    self.default_group("Reserves & Surplus").ok_or(SkipReason::NoParentGroup)
                }
                None => Err(SkipReason::NoParentGroup),
            };
            let group = match group {
                Ok(g) => g,
                Err(reason) => {
                    self.skip(ItemKind::Ledger, &l.name, reason);
                    continue;
                }
            };
            let id = match self.book.create_ledger(self.at, self.user, &l.name, group) {
                Ok(id) => id,
                Err(e) => {
                    self.skip(ItemKind::Ledger, &l.name, SkipReason::Rejected(e));
                    continue;
                }
            };
            self.report.ledgers_created += 1;
            self.ledgers.insert(l.name.to_lowercase(), id);
            if !l.opening.is_zero() {
                self.book.set_opening_balance(self.at, self.user, id, l.opening).expect("ledger was just created");
                self.report.opening_balances += 1;
            }
        }
        // Aliases last, so they never shadow a real ledger name.
        for l in &data.ledgers {
            if let Some(&id) = self.ledgers.get(&l.name.to_lowercase()) {
                for a in &l.aliases {
                    self.ledgers.entry(a.to_lowercase()).or_insert(id);
                }
            }
        }
    }

    /// Simple units are created (or matched to a unit already in the book by
    /// symbol); compound units are reported and skipped.
    fn units(&mut self, data: &TallyData) {
        for u in &data.units {
            let key = u.name.to_lowercase();
            if u.compound {
                self.compound_units.insert(key);
                self.skip(ItemKind::Unit, &u.name, SkipReason::CompoundUnit);
                continue;
            }
            if self.units.contains_key(&key) {
                continue;
            }
            let decimals = u.decimals.unwrap_or_else(|| self.seen_decimals.get(&key).copied().unwrap_or(0)).min(4);
            if let Err(e) = self.unit(&u.name, &u.formal_name, decimals) {
                self.skip(ItemKind::Unit, &u.name, SkipReason::Rejected(e));
            }
        }
    }

    /// The unit with this symbol: imported, already in the book, or created.
    fn unit(&mut self, symbol: &str, formal_name: &str, decimals: u32) -> Result<UnitId, BookError> {
        let key = symbol.to_lowercase();
        if let Some(&id) = self.units.get(&key) {
            return Ok(id);
        }
        let existing = self.book.units().find(|u| u.symbol.eq_ignore_ascii_case(symbol)).map(|u| u.id);
        let id = match existing {
            Some(id) => id,
            None => {
                let unit =
                    Unit { id: UnitId::new(), symbol: symbol.to_owned(), name: formal_name.to_owned(), decimals };
                let id = self.book.save_unit(self.at, self.user, unit)?;
                self.report.units_created += 1;
                id
            }
        };
        self.units.insert(key, id);
        Ok(id)
    }

    /// Stock groups in dependency order; "Primary" is the top level.
    fn stock_groups(&mut self, data: &TallyData) {
        let nodes: Vec<TreeNode<'_>> = data
            .stock_groups
            .iter()
            .map(|g| TreeNode { name: &g.name, aliases: &g.aliases, parent: g.parent.as_deref() })
            .collect();
        let (at, user) = (self.at, self.user);
        let (created, skipped) = tree(
            self.book,
            &nodes,
            &mut self.stock_groups,
            |book, name| book.stock_groups().find(|g| g.name.eq_ignore_ascii_case(name)).map(|g| g.id),
            |book, name, parent| {
                book.save_stock_group(at, user, StockGroup { id: StockGroupId::new(), name: name.to_owned(), parent })
            },
        );
        self.report.stock_groups_created += created;
        for (name, why) in skipped {
            let reason = match why {
                TreeSkip::UnknownParent(p) => SkipReason::UnknownParentStockGroup(p),
                TreeSkip::Cycle(p) => SkipReason::StockGroupCycle(p),
                TreeSkip::Rejected(e) => SkipReason::Rejected(e),
            };
            self.skip(ItemKind::StockGroup, name, reason);
        }
    }

    /// Godown masters in dependency order, then any godown that voucher
    /// lines name but the masters did not include (created at the top).
    fn godowns(&mut self, data: &TallyData) {
        let referenced: Vec<String> = data
            .vouchers
            .iter()
            .flat_map(|v| &v.inventory)
            .flat_map(|l| &l.batches)
            .filter_map(|b| b.godown.clone())
            .filter(|name| !data.godowns.iter().any(|g| g.name.eq_ignore_ascii_case(name)))
            .collect();
        let nodes: Vec<TreeNode<'_>> = data
            .godowns
            .iter()
            .map(|g| TreeNode { name: &g.name, aliases: &g.aliases, parent: g.parent.as_deref() })
            .chain(referenced.iter().map(|name| TreeNode { name, aliases: &[], parent: None }))
            .collect();
        let (at, user) = (self.at, self.user);
        let (created, skipped) = tree(
            self.book,
            &nodes,
            &mut self.godowns,
            |book, name| book.godowns().find(|g| g.name.eq_ignore_ascii_case(name)).map(|g| g.id),
            |book, name, parent| {
                book.save_godown(at, user, Godown { id: GodownId::new(), name: name.to_owned(), parent })
            },
        );
        self.report.godowns_created += created;
        for (name, why) in skipped {
            let reason = match why {
                TreeSkip::UnknownParent(p) => SkipReason::UnknownParentGodown(p),
                TreeSkip::Cycle(p) => SkipReason::GodownCycle(p),
                TreeSkip::Rejected(e) => SkipReason::Rejected(e),
            };
            self.skip(ItemKind::Godown, name, reason);
        }
    }

    fn stock_items(&mut self, data: &TallyData) {
        for i in &data.stock_items {
            let key = i.name.to_lowercase();
            let existing = self.book.stock_items().find(|x| x.name.eq_ignore_ascii_case(&i.name)).map(|x| x.id);
            if let Some(id) = existing.or_else(|| self.stock_items.get(&key).copied()) {
                self.stock_items.entry(key).or_insert(id);
                self.skip(ItemKind::StockItem, &i.name, SkipReason::DuplicateStockItem);
                continue;
            }
            let group = match i.parent.as_deref() {
                None => None,
                Some(p) => match self.stock_groups.get(&p.to_lowercase()) {
                    Some(&id) => Some(id),
                    None => {
                        self.skip(ItemKind::StockItem, &i.name, SkipReason::UnknownParentStockGroup(p.to_owned()));
                        continue;
                    }
                },
            };
            let Some(symbol) = i.unit.as_deref() else {
                self.skip(ItemKind::StockItem, &i.name, SkipReason::NoUnit);
                continue;
            };
            if self.compound_units.contains(&symbol.to_lowercase()) {
                self.skip(ItemKind::StockItem, &i.name, SkipReason::CompoundItemUnit(symbol.to_owned()));
                continue;
            }
            // A unit the masters did not include is created from the data.
            let seen = self.seen_decimals.get(&symbol.to_lowercase()).copied().unwrap_or(0);
            let unit = match self.unit(symbol, "", seen) {
                Ok(id) => id,
                Err(e) => {
                    self.skip(ItemKind::StockItem, &i.name, SkipReason::Rejected(e));
                    continue;
                }
            };
            let negative = i.opening_qty < Quantity::ZERO;
            let (opening_qty, opening_value) =
                if negative { (Quantity::ZERO, Amount::ZERO) } else { (i.opening_qty, i.opening_value) };
            let item = StockItem {
                id: StockItemId::new(),
                name: i.name.clone(),
                group,
                unit,
                opening_qty,
                opening_value,
                tax_class: i.hsn.clone(),
                costing: costing_of(i.costing.as_deref()),
            };
            match self.book.save_stock_item(self.at, self.user, item) {
                Ok(id) => {
                    self.report.stock_items_created += 1;
                    for name in std::iter::once(&i.name).chain(&i.aliases) {
                        self.stock_items.entry(name.to_lowercase()).or_insert(id);
                    }
                    if negative {
                        let reason = SkipReason::NegativeOpeningStock(i.opening_qty.to_string());
                        self.skip(ItemKind::Inventory, &i.name, reason);
                    }
                    if i.godown_detail && !negative {
                        self.skip(ItemKind::Inventory, &i.name, SkipReason::OpeningByGodownMerged);
                    }
                }
                Err(e) => self.skip(ItemKind::StockItem, &i.name, SkipReason::Rejected(e)),
            }
        }
    }

    fn merged(&mut self, item: &str) {
        if !self.merged_detail.iter().any(|n| n.eq_ignore_ascii_case(item)) {
            self.merged_detail.push(item.to_owned());
        }
    }

    fn resolve_stock_item(&self, name: &str) -> Option<StockItemId> {
        self.stock_items
            .get(&name.to_lowercase())
            .copied()
            .or_else(|| self.book.stock_items().find(|i| i.name.eq_ignore_ascii_case(name)).map(|i| i.id))
    }

    fn resolve_ledger(&self, name: &str) -> Option<LedgerId> {
        self.book.ledger_by_name(name).map(|l| l.id).or_else(|| self.ledgers.get(&name.to_lowercase()).copied())
    }

    /// Follow a custom voucher type's parents to a predefined type.
    fn voucher_kind(&self, type_name: &str) -> Result<VoucherKind, SkipReason> {
        let mut name = type_name.to_owned();
        for _ in 0..16 {
            let n = normalize(&name);
            if let Some((_, kind)) = BASE_TYPES.iter().find(|(t, _)| normalize(t) == n) {
                return Ok(*kind);
            }
            if let Some(t) = INVENTORY_TYPES.iter().find(|t| normalize(t) == n) {
                return Err(SkipReason::InventoryVoucherType((*t).to_owned()));
            }
            if UNSUPPORTED_TYPES.iter().any(|t| normalize(t) == n) {
                return Err(SkipReason::UnsupportedVoucherType(type_name.to_owned()));
            }
            match self.voucher_types.get(&n) {
                Some(parent) if normalize(parent) != n => name = parent.clone(),
                _ => break,
            }
        }
        Err(SkipReason::UnknownVoucherType(type_name.to_owned()))
    }

    fn voucher(&mut self, v: &TallyVoucher) {
        let label = {
            let mut l = v.type_name.clone();
            if !v.number.is_empty() {
                l.push_str(&format!(" {}", v.number));
            }
            format!("{l} on {}", v.date.format("%Y-%m-%d"))
        };
        let (voucher, mut dropped, merged) = match self.build_voucher(v) {
            Ok(built) => built,
            Err(reason) => return self.skip(ItemKind::Voucher, label, reason),
        };
        let lines: usize = voucher.entries.iter().map(|e| e.items.len()).sum();
        let posted = match self.book.post_voucher(self.at, self.user, voucher.clone()) {
            // Stock lines the book will not take: keep the accounting.
            Err(e) if lines > 0 => {
                let mut plain = voucher;
                for entry in &mut plain.entries {
                    entry.items.clear();
                }
                let posted = self.book.post_voucher(self.at, self.user, plain).map(|_| 0);
                if posted.is_ok() {
                    dropped = Some(e.to_string());
                }
                posted
            }
            posted => posted.map(|_| lines),
        };
        match posted {
            Ok(lines) => {
                self.report.vouchers_posted += 1;
                self.report.vouchers_numbered += usize::from(v.number.is_empty());
                self.report.inventory_lines += lines;
                if lines > 0 {
                    for item in merged {
                        self.merged(&item);
                    }
                }
                if let Some(why) = dropped {
                    self.skip(ItemKind::Inventory, label, SkipReason::StockLinesDropped(why));
                }
            }
            Err(e) => self.skip(ItemKind::Voucher, label, rejected(e)),
        }
    }

    /// The voucher to post, why its stock lines were left off (if they
    /// were), and the items whose batch allocations were merged. Stock lines
    /// never cause a voucher to be skipped.
    fn build_voucher(&self, v: &TallyVoucher) -> Result<(Voucher, Option<String>, Vec<String>), SkipReason> {
        if v.cancelled {
            return Err(SkipReason::Cancelled);
        }
        if v.optional {
            return Err(SkipReason::Optional);
        }
        let kind = self.voucher_kind(&v.type_name)?;
        let mut signed = Vec::with_capacity(v.entries.len());
        for e in &v.entries {
            let ledger = self.resolve_ledger(&e.ledger).ok_or_else(|| SkipReason::UnknownLedger(e.ledger.clone()))?;
            signed.push((ledger, e.amount));
        }
        let plain = || signed.iter().filter_map(|&(ledger, amount)| entry(ledger, amount)).collect::<Vec<_>>();
        let (entries, dropped, merged) = match &v.inventory_problem {
            Some(problem) => (plain(), Some(problem.clone()), Vec::new()),
            None if v.inventory.is_empty() => (plain(), None, Vec::new()),
            None => match self.with_stock(&signed, &v.inventory) {
                Ok((entries, merged)) => (entries, None, merged),
                Err(why) => (plain(), Some(why), Vec::new()),
            },
        };
        if entries.is_empty() {
            return Err(SkipReason::NoEntries);
        }
        let number = if v.number.is_empty() { self.book.next_voucher_number(kind, v.date) } else { v.number.clone() };
        let voucher = Voucher {
            id: VoucherId::new(),
            kind,
            number,
            date: v.date,
            entries,
            narration: v.narration.clone(),
            cancelled: false, stock: Vec::new(),
        };
        Ok((voucher, dropped, merged))
    }

    /// The book's lines for one Tally line: one per batch allocation when
    /// the allocations carry godown or batch detail and add up to the line;
    /// otherwise the whole line at the main location (`true`: detail was
    /// merged).
    fn split(&self, item: StockItemId, l: &TallyStockLine) -> (Vec<StockLine>, bool) {
        let whole = || {
            let mut line = StockLine::new(item, l.qty, l.rate);
            line.amount = l.amount.abs();
            vec![line]
        };
        if !l.has_godown_detail() {
            return (whole(), false);
        }
        let godown = |name: &str| self.godowns.get(&name.to_lowercase()).copied();
        let fits = l.batches.iter().all(|b| {
            b.qty > Quantity::ZERO
                && b.batch.as_deref().is_none_or(|n| n.trim().chars().count() <= MAX_BATCH)
                && b.godown.as_deref().is_none_or(|g| godown(g).is_some())
        }) && l.batches.iter().map(|b| b.qty).sum::<Quantity>() == l.qty
            && l.batches.iter().map(|b| b.amount).sum::<Amount>() == l.amount
            && l.batches.iter().map(|b| b.amount.abs()).sum::<Amount>() == l.amount.abs();
        if !fits {
            return (whole(), true);
        }
        let lines = l
            .batches
            .iter()
            .map(|b| {
                let mut line = StockLine::new(item, b.qty.normalize(), b.rate.unwrap_or(l.rate));
                line.amount = b.amount.abs();
                if let Some(id) = b.godown.as_deref().and_then(godown) {
                    line = line.at(id);
                }
                if let Some(name) = &b.batch {
                    line = line.batch(name.trim());
                }
                line
            })
            .collect();
        (lines, false)
    }

    /// Ledger entries with the stock lines on the entry of the ledger each
    /// line is booked to (the sales or purchase ledger). Several entries of
    /// such a ledger become one entry carrying all its lines. `Err` says why
    /// the lines cannot be kept.
    fn with_stock(
        &self,
        signed: &[(LedgerId, Amount)],
        lines: &[TallyStockLine],
    ) -> Result<(Vec<Entry>, Vec<String>), String> {
        let mut by_ledger: Vec<(LedgerId, Vec<(&TallyStockLine, StockLine)>)> = Vec::new();
        let mut merged_detail = Vec::new();
        for l in lines {
            let item = self
                .resolve_stock_item(&l.item)
                .ok_or_else(|| format!("stock item {:?} is not in the import or the book", l.item))?;
            let ledger_name = match l.ledgers.as_slice() {
                [one] => one,
                [] => {
                    return Err(format!("{:?} is not booked to a sales or purchase ledger", l.item));
                }
                many => {
                    return Err(format!("{:?} is split across {} ledgers", l.item, many.len()));
                }
            };
            let ledger = self
                .resolve_ledger(ledger_name)
                .ok_or_else(|| format!("ledger {ledger_name:?} of {:?} is not in the import or the book", l.item))?;
            if l.qty.is_zero() {
                return Err(format!("{:?} has no quantity", l.item));
            }
            let (split, merged) = self.split(item, l);
            if merged {
                merged_detail.push(l.item.clone());
            }
            let pos = by_ledger.iter().position(|(id, _)| *id == ledger).unwrap_or_else(|| {
                by_ledger.push((ledger, Vec::new()));
                by_ledger.len() - 1
            });
            by_ledger[pos].1.extend(split.into_iter().map(|line| (l, line)));
        }
        // Merge the entries of ledgers that carry stock, keeping the first
        // one's position.
        let mut merged: Vec<(LedgerId, Amount, bool)> = Vec::new();
        for &(ledger, amount) in signed {
            let stock = by_ledger.iter().any(|(id, _)| *id == ledger);
            match merged.iter_mut().find(|(id, _, s)| stock && *s && *id == ledger) {
                Some(m) => m.1 += amount,
                None => merged.push((ledger, amount, stock)),
            }
        }
        let mut entries = Vec::with_capacity(merged.len());
        for (ledger, amount, stock) in merged {
            let Some(mut e) = entry(ledger, amount) else {
                if stock {
                    return Err("the stock lines' ledger line nets to zero".into());
                }
                continue;
            };
            if stock {
                let (_, ls) = by_ledger.iter().find(|(id, _)| *id == ledger).expect("ledger carries stock");
                for (l, line) in ls {
                    if l.inward() != (amount > Amount::ZERO) {
                        return Err(format!("{:?} moves stock the opposite way to its ledger line", l.item));
                    }
                    e = e.item(line.clone());
                }
                let total: Amount = e.items.iter().map(|l| l.amount).sum();
                if total != e.amount {
                    return Err(format!("item amounts ({total}) do not add up to their ledger line ({})", e.amount));
                }
            }
            entries.push(e);
        }
        if let Some((ledger, _)) = by_ledger.iter().find(|(id, _)| !signed.iter().any(|(l, _)| l == id)) {
            let name = self.book.ledger(*ledger).map(|l| l.name.clone()).unwrap_or_default();
            return Err(format!("ledger {name:?} of the stock lines has no amount on the voucher"));
        }
        Ok((entries, merged_detail))
    }
}

/// A KhataOS entry for a debit-positive amount; `None` for zero.
fn entry(ledger: LedgerId, amount: Amount) -> Option<Entry> {
    if amount > Amount::ZERO {
        Some(Entry::dr(ledger, amount))
    } else if amount < Amount::ZERO {
        Some(Entry::cr(ledger, -amount))
    } else {
        None
    }
}

fn rejected(e: BookError) -> SkipReason {
    match e {
        BookError::Voucher(VoucherError::Unbalanced { dr, cr }) => SkipReason::Unbalanced { dr, cr },
        BookError::DuplicateVoucherNumber(n) => SkipReason::DuplicateNumber(n),
        BookError::ContraNonCashLedger => SkipReason::ContraNonCash,
        e => SkipReason::Rejected(e),
    }
}

/// Tally's costing method, where KhataOS has the same one; average cost
/// otherwise (Tally's default too).
fn costing_of(method: Option<&str>) -> khata_core::inventory::Costing {
    use khata_core::inventory::Costing;
    match method.map(|m| m.to_ascii_lowercase().replace(['.', ' '], "")) {
        Some(m) if m == "fifo" || m == "fifoperpetual" => Costing::Fifo,
        Some(m) if m.starts_with("lastpurchase") => Costing::LastPurchase,
        _ => Costing::AverageCost,
    }
}

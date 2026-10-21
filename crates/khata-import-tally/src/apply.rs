//! Replaying parsed Tally objects into a [`Book`].

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::book::{MAX_SERIES_NAME, normalise_tax_class};
use khata_core::chart::MAX_BILL_REFERENCE;
use khata_core::inventory::MAX_BATCH;
use khata_core::numbering::fy_start;
use khata_core::{
    Amount, BillAllocation, BillKind, Book, BookError, Entry, Godown, GodownId, GroupId, Ledger, LedgerId, LedgerTax,
    Mailing, NumberSeries, OpeningBill, Quantity, Restart, StockGroup, StockGroupId, StockItem, StockItemId, StockLine,
    TaxSide, Unit, UnitId, Voucher, VoucherError, VoucherId, VoucherKind,
};

use crate::parse::{TallyBill, TallyData, TallyEntry, TallyGroup, TallyGst, TallyLedger, TallyStockLine, TallyVoucher};
use crate::report::{BalanceDifference, Note, NoteKind, TypeCount, voucher_kind_label};
use crate::text::{normalize, tally_date, tally_due};
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

/// GST came into force on this date; rates without an "applicable from"
/// date apply from it.
const GST_START: (i32, u32, u32) = (2017, 7, 1);

/// A Day Book whose first voucher is more days than this after the start
/// of its financial year gets a warning.
const LATE_START_DAYS: i64 = 31;

/// Longest postal and contact fields a ledger's mailing details take.
const MAX_ADDRESS: usize = 400;
const MAX_CONTACT: usize = 120;
/// Longest tax class (HSN/SAC) the core takes.
const MAX_TAX_CLASS: usize = 24;
/// Deepest group tree walked; real charts are a few levels deep.
const MAX_DEPTH: usize = 64;

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
    /// Vouchers posted by this import, to tell a number Tally repeated from
    /// one already in the company before.
    posted: HashSet<VoucherId>,
    /// Lower-cased duty ledger name → (used on sales, used on purchases).
    tax_usage: HashMap<String, (bool, bool)>,
    /// GST rates by HSN/SAC: `(normalized class, from)` → (class, rate, master).
    rates: HashMap<(String, NaiveDate), (String, Amount, String)>,
    /// Counts for the summary notes written at the end.
    cost_centre_lines: usize,
    bank_dates_dropped: usize,
}

/// A ledger line ready to post: debit-positive amount, bills and bank date.
#[derive(Clone)]
struct Line {
    ledger: LedgerId,
    amount: Amount,
    bills: Vec<BillAllocation>,
    bank_date: Option<NaiveDate>,
}

/// What [`Importer::build_voucher`] made: the voucher, why its stock lines
/// were left off (if they were), items whose batch detail was merged, and
/// notes to keep once it posts.
struct Built {
    voucher: Voucher,
    dropped: Option<String>,
    merged: Vec<String>,
    notes: Vec<Note>,
}

/// Why posting failed.
enum PostError {
    Book(BookError),
    /// The number was in the company before this import began.
    AlreadyInBooks(String),
}

impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Book(e) => e.fmt(f),
            Self::AlreadyInBooks(n) => SkipReason::AlreadyInBooks(n.clone()).fmt(f),
        }
    }
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
        posted: HashSet::new(),
        tax_usage: HashMap::new(),
        rates: HashMap::new(),
        cost_centre_lines: 0,
        bank_dates_dropped: 0,
    };
    im.report.skipped.append(&mut data.rejected);
    im.groups(&data.groups);
    im.tax_usage(&data);
    im.ledgers(&data);
    im.units(&data);
    im.stock_groups(&data);
    im.stock_items(&data);
    im.tax_rates();
    im.godowns(&data);
    // Numbered vouchers first, so numbers KhataOS hands out to unnumbered
    // ones can never take a number Tally already used.
    let (numbered, unnumbered): (Vec<_>, Vec<_>) = data.vouchers.iter().partition(|v| !v.number.is_empty());
    im.yearly_numbering(&numbered);
    for v in numbered.into_iter().chain(unnumbered) {
        im.voucher(v);
    }
    for name in std::mem::take(&mut im.merged_detail) {
        im.skip(ItemKind::Inventory, name, SkipReason::BatchDetailMerged);
    }
    im.summary_notes();
    im.checks(&data);
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

fn note(kind: NoteKind, name: impl Into<String>, detail: impl Into<String>) -> Note {
    Note { kind, name: name.into(), detail: detail.into(), count: 1 }
}

/// A voucher's name in the report: "Sales GST 12 on 2024-04-01".
fn label(v: &TallyVoucher) -> String {
    let mut l = v.type_name.clone();
    if !v.number.is_empty() {
        l.push_str(&format!(" {}", v.number));
    }
    format!("{l} on {}", v.date.format("%Y-%m-%d"))
}

/// `s` trimmed to at most `max` characters, and whether it was cut.
fn cut(s: &str, max: usize) -> (String, bool) {
    let t = s.trim();
    if t.chars().count() <= max { (t.to_owned(), false) } else { (t.chars().take(max).collect(), true) }
}

/// The GST state code for a Tally state name ("Maharashtra" → "27").
fn state_code(name: &str) -> Option<&'static str> {
    // Older and alternative spellings Tally has used.
    const OTHER_NAMES: &[(&str, &str)] = &[
        ("orissa", "21"),
        ("pondicherry", "34"),
        ("uttaranchal", "05"),
        ("newdelhi", "07"),
        ("nctofdelhi", "07"),
        ("dadraandnagarhaveli", "26"),
        ("damananddiu", "26"),
        ("andamanandnicobar", "35"),
    ];
    let key = |s: &str| normalize(&s.replace('&', " and "));
    let n = key(name);
    if n.is_empty() {
        return None;
    }
    khata_in::states::STATES
        .iter()
        .filter(|s| !s.legacy)
        .find(|s| key(s.name) == n)
        .map(|s| s.code)
        .or_else(|| OTHER_NAMES.iter().find(|(o, _)| *o == n).map(|(_, c)| *c))
}

/// The GST component a Tally duty head collects.
fn component(head: &str) -> Option<&'static str> {
    match normalize(head).as_str() {
        "integratedtax" | "igst" => Some("IGST"),
        "centraltax" | "cgst" => Some("CGST"),
        "statetax" | "sgst" => Some("SGST"),
        "uttax" | "utgst" | "unionterritorytax" => Some("UTGST"),
        h if h.contains("cess") => Some("CESS"),
        _ => None,
    }
}

/// A credit period in whole days ("30 Days"), not a fixed due date.
fn credit_days(raw: &str) -> Option<u32> {
    let lower = raw.trim().to_ascii_lowercase();
    lower.trim_end_matches('s').trim_end_matches("day").trim().parse().ok()
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

    /// Group `id` and the groups above it.
    fn ancestors(&self, id: GroupId) -> impl Iterator<Item = GroupId> + '_ {
        let mut next = Some(id);
        std::iter::from_fn(move || {
            let id = next?;
            next = self.book.group(id).and_then(|g| g.parent);
            Some(id)
        })
        .take(MAX_DEPTH)
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

    /// Which GST duty ledgers the vouchers use on sales and on purchases,
    /// to tell input tax ledgers from output ones.
    fn tax_usage(&mut self, data: &TallyData) {
        let duty: HashSet<String> = data
            .ledgers
            .iter()
            .filter(|l| l.party.duty_head.is_some())
            .flat_map(|l| std::iter::once(&l.name).chain(&l.aliases))
            .map(|n| n.to_lowercase())
            .collect();
        if duty.is_empty() {
            return;
        }
        for v in &data.vouchers {
            let (outward, inward) = match self.voucher_kind(&v.type_name) {
                Ok(VoucherKind::Sales | VoucherKind::CreditNote) => (true, false),
                Ok(VoucherKind::Purchase | VoucherKind::DebitNote) => (false, true),
                _ => continue,
            };
            for e in &v.entries {
                let name = e.ledger.to_lowercase();
                if duty.contains(&name) {
                    let slot = self.tax_usage.entry(name).or_default();
                    slot.0 |= outward;
                    slot.1 |= inward;
                }
            }
        }
    }

    fn ledgers(&mut self, data: &TallyData) {
        // One pass over the book instead of a search per ledger.
        let existing: HashMap<String, LedgerId> = self.book.ledgers().map(|l| (l.name.to_lowercase(), l.id)).collect();
        for l in &data.ledgers {
            if let Some(&id) = existing.get(&l.name.to_lowercase()) {
                self.ledgers.insert(l.name.to_lowercase(), id);
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
            self.ledger_details(l, id);
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

    /// Opening balance, opening bills, party and tax details of a ledger
    /// just created, in one change. If the book refuses a detail, the
    /// ledger keeps its opening balance only, with a note.
    fn ledger_details(&mut self, l: &TallyLedger, id: LedgerId) {
        let created = self.book.ledger(id).expect("ledger was just created").clone();
        let mut notes = Vec::new();
        let mut ledger = Ledger { opening: l.opening, ..created.clone() };
        self.party_details(l, &mut ledger, &mut notes);
        self.opening_bills(l, &mut ledger, &mut notes);
        if ledger != created {
            if let Err(e) = self.book.alter_ledger(self.at, self.user, ledger.clone()) {
                notes = vec![note(NoteKind::DetailRefused, &l.name, format!("{e}; the ledger was kept with its opening balance only"))];
                ledger = Ledger { opening: l.opening, ..created };
                if !l.opening.is_zero() {
                    self.book.set_opening_balance(self.at, self.user, id, l.opening).expect("ledger was just created");
                }
            }
        }
        self.report.opening_balances += usize::from(!ledger.opening.is_zero());
        let g = &mut self.report.gst;
        g.gstins += usize::from(ledger.tax_id.is_some());
        g.pans += usize::from(ledger.pan.is_some());
        g.duty_ledgers += usize::from(ledger.tax.is_some());
        g.ledger_classes += usize::from(ledger.tax_class.is_some());
        if let Some(m) = &ledger.mailing {
            g.states += usize::from(m.region.is_some());
            g.addresses += usize::from(!m.address.is_empty());
        }
        if !ledger.opening_bills.is_empty() {
            self.report.bills.opening += ledger.opening_bills.len();
            self.report.bills.opening_ledgers += 1;
        }
        self.report.notes.extend(notes);
    }

    /// GSTIN, PAN, state and address, credit period, the tax a duty ledger
    /// collects, and the HSN/SAC code of a sales or service ledger.
    fn party_details(&mut self, l: &TallyLedger, ledger: &mut Ledger, notes: &mut Vec<Note>) {
        let p = &l.party;
        let gstin = p.gstin.as_deref().map(|g| g.split_whitespace().collect::<String>().to_ascii_uppercase());
        if let Some(raw) = gstin.filter(|g| !g.is_empty() && g != "NA") {
            match khata_in::gstin::validate(&raw) {
                Ok(_) => ledger.tax_id = Some(raw),
                Err(e) => {
                    let (gstin, _) = cut(&raw, 20);
                    notes.push(note(NoteKind::GstinInvalid, &l.name, format!("GSTIN {gstin}: {e}")));
                    ledger.tax_id_override = Some(format!("Imported from Tally; {e}"));
                    ledger.tax_id = Some(gstin);
                }
            }
        }
        if let Some(pan) = p.pan.as_deref().map(|x| x.trim().to_ascii_uppercase()).filter(|x| !x.is_empty()) {
            if pan.len() == 10 && pan.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
                ledger.pan = Some(pan);
            } else {
                notes.push(note(NoteKind::PanInvalid, &l.name, format!("PAN {pan:?} is not 10 letters and digits")));
            }
        }
        let mut m = Mailing::default();
        if let Some(state) = p.state.as_deref().filter(|s| !s.trim().is_empty()) {
            match state_code(state) {
                Some(code) => m.region = Some(code.to_owned()),
                None => notes.push(note(NoteKind::StateUnknown, &l.name, format!("state {state:?} is not a GST state"))),
            }
        }
        let mut shortened = Vec::new();
        let mut take = |value: &str, max: usize, what: &str| {
            let (v, was_cut) = cut(value, max);
            if was_cut {
                shortened.push(what.to_owned());
            }
            v
        };
        m.address = take(&p.address.join("\n"), MAX_ADDRESS, "address");
        m.postal_code = take(p.pincode.as_deref().unwrap_or(""), MAX_CONTACT, "PIN code");
        m.phone = take(p.phone.as_deref().unwrap_or(""), MAX_CONTACT, "phone");
        m.email = take(p.email.as_deref().unwrap_or(""), MAX_CONTACT, "email");
        m.special_zone = p.sez;
        if !shortened.is_empty() {
            notes.push(note(NoteKind::DetailNotKept, &l.name, format!("shortened to fit: {}", shortened.join(", "))));
        }
        if let Some(country) = p.country.as_deref().filter(|c| !c.trim().is_empty() && normalize(c) != "india") {
            notes.push(note(NoteKind::DetailNotKept, &l.name, format!("country {country:?} was not kept; set it under Alter Ledger")));
        }
        if m != Mailing::default() {
            ledger.mailing = Some(m);
        }
        if let Some(days) = p.credit_period.as_deref().and_then(credit_days).filter(|d| *d > 0) {
            ledger.credit_days = Some(days);
        }
        self.duty_tax(l, ledger, notes);
        // An HSN/SAC code on a sales, purchase or service ledger; a duty
        // ledger's GST details describe the tax, not a supply.
        let Some(gst) = l.gst.as_ref().filter(|_| ledger.tax.is_none()) else { return };
        match gst.hsn.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
            Some(hsn) if hsn.chars().count() <= MAX_TAX_CLASS => {
                ledger.tax_class = Some(hsn.to_owned());
                self.rate(hsn, gst, &l.name);
            }
            Some(hsn) => {
                notes.push(note(NoteKind::DetailNotKept, &l.name, format!("HSN/SAC {hsn:?} is longer than {MAX_TAX_CLASS} characters")));
            }
            None => {
                if let Some(c) = gst.classification.as_deref().filter(|_| gst.rate.is_none()) {
                    notes.push(note(NoteKind::GstClassification, &l.name, format!("its GST details come from the classification {c:?}")));
                }
            }
        }
    }

    /// Tag a GST duty ledger with the tax it collects, and on which side.
    fn duty_tax(&self, l: &TallyLedger, ledger: &mut Ledger, notes: &mut Vec<Note>) {
        let p = &l.party;
        let gst_type = p.tax_type.as_deref().map(normalize);
        let Some(head) = p.duty_head.as_deref().filter(|_| gst_type.as_deref().is_none_or(|t| t == "gst")) else {
            if gst_type.as_deref() == Some("gst") {
                notes.push(note(NoteKind::DutyLedgerNotTagged, &l.name, "a GST ledger without a duty head (CGST, SGST, IGST or cess)"));
            }
            return;
        };
        let Some(component) = component(head) else {
            notes.push(note(NoteKind::DutyLedgerNotTagged, &l.name, format!("duty head {head:?} is not CGST, SGST, UTGST, IGST or cess")));
            return;
        };
        let duties = GroupId::default_named("Duties & Taxes");
        if !self.ancestors(ledger.group).any(|g| g == duties) {
            notes.push(note(NoteKind::DutyLedgerNotTagged, &l.name, format!("it collects {component} but is not under Duties & Taxes")));
            return;
        }
        let name = normalize(&l.name);
        let by_name = if ["input", "itc", "credit", "receivable"].iter().any(|w| name.contains(w)) {
            Some(TaxSide::Input)
        } else if ["output", "payable"].iter().any(|w| name.contains(w)) {
            Some(TaxSide::Output)
        } else {
            None
        };
        let usage = std::iter::once(&l.name)
            .chain(&l.aliases)
            .filter_map(|n| self.tax_usage.get(&n.to_lowercase()))
            .fold((false, false), |a, b| (a.0 | b.0, a.1 | b.1));
        let side = match (by_name, usage) {
            (Some(side), _) => side,
            (None, (true, false)) => TaxSide::Output,
            (None, (false, true)) => TaxSide::Input,
            (None, (outward, inward)) => {
                let why = if outward && inward { "it is used on both sales and purchases" } else { "no sale or purchase uses it" };
                notes.push(note(
                    NoteKind::TaxSideGuessed,
                    &l.name,
                    format!("{why}, so it was tagged as input {component}. GST reports still count it by voucher type; on a sales invoice it is output tax. Set the side under Alter Ledger if needed"),
                ));
                TaxSide::Input
            }
        };
        ledger.tax = Some(LedgerTax { component: component.to_owned(), side });
    }

    /// Remember an HSN/SAC code's GST rate, to set once all masters are in.
    fn rate(&mut self, hsn: &str, gst: &TallyGst, master: &str) {
        let Some(rate) = gst.rate.filter(|r| *r >= Amount::ZERO && *r <= Amount::ONE_HUNDRED) else { return };
        let start = NaiveDate::from_ymd_opt(GST_START.0, GST_START.1, GST_START.2).expect("valid date");
        let from = gst.from.unwrap_or(start).max(start);
        let key = (normalise_tax_class(hsn), from);
        match self.rates.get(&key) {
            None => {
                self.rates.insert(key, (hsn.to_owned(), rate, master.to_owned()));
            }
            Some((_, kept, first)) if *kept != rate => {
                let detail = format!("HSN/SAC {hsn} is {rate}% here but {kept}% on {first}; kept {kept}%");
                self.report.notes.push(note(NoteKind::TaxRateConflict, master, detail));
            }
            Some(_) => {}
        }
    }

    /// Set every HSN/SAC rate the masters gave, in one change. A rate the
    /// company already has for that code and date is kept.
    fn tax_rates(&mut self) {
        if self.rates.is_empty() {
            return;
        }
        let mut list: Vec<_> = std::mem::take(&mut self.rates).into_iter().collect();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        let mut rates: Vec<(String, NaiveDate, Amount)> = Vec::new();
        for ((_, from), (class, rate, master)) in list {
            match self.book.tax_rate_from(&class, from) {
                Some(existing) if existing != rate => {
                    let detail = format!("HSN/SAC {class} is {rate}% in Tally but the company already has {existing}%; kept {existing}%");
                    self.report.notes.push(note(NoteKind::TaxRateConflict, master, detail));
                }
                Some(_) => {}
                None => rates.push((class, from, rate)),
            }
        }
        let n = rates.len();
        let why = if self.book.profile().is_none() {
            Some("set the company's country (India) and financial year before importing, so HSN/SAC rates can be kept".to_owned())
        } else {
            self.book.set_tax_rates(self.at, self.user, &rates).err().map(|e| e.to_string())
        };
        match why {
            None => self.report.gst.rates += n,
            Some(detail) => self.report.notes.push(Note { kind: NoteKind::TaxRatesNotKept, name: String::new(), detail, count: n }),
        }
    }

    /// The bills a ledger's opening balance is made of.
    fn opening_bills(&self, l: &TallyLedger, ledger: &mut Ledger, notes: &mut Vec<Note>) {
        let mut bills = Vec::new();
        let mut problems: Vec<String> = l.bill_problem.iter().cloned().collect();
        for b in l.opening_bills.iter().filter(|b| !b.amount.is_zero()) {
            let (reference, was_cut) = cut(&b.name, MAX_BILL_REFERENCE);
            if reference.is_empty() {
                problems.push(format!("a bill of {} has no name and was left out", b.amount.abs()));
                continue;
            }
            if was_cut {
                problems.push(format!("bill name {:?} was shortened", b.name));
            }
            let due = match (b.credit_period.as_deref(), b.date) {
                (Some(cp), Some(date)) => tally_due(cp, date),
                (Some(cp), None) => tally_date(cp),
                _ => None,
            }
            .filter(|d| b.date.is_none_or(|bd| *d >= bd));
            bills.push(OpeningBill { reference, date: b.date, due, amount: b.amount, advance: b.advance });
        }
        let total: Amount = bills.iter().map(|b| b.amount).sum();
        if !bills.is_empty() && total != l.opening {
            problems.push(format!(
                "its bills add up to {total}, its opening balance is {}; the rest shows as Opening Balance in outstandings",
                l.opening
            ));
        }
        if !problems.is_empty() {
            notes.push(note(NoteKind::OpeningBills, &l.name, problems.join("; ")));
        }
        ledger.opening_bills = bills;
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
        let masters: HashSet<String> = data.godowns.iter().map(|g| g.name.to_lowercase()).collect();
        let referenced: Vec<String> = data
            .vouchers
            .iter()
            .flat_map(|v| &v.inventory)
            .flat_map(|l| &l.batches)
            .filter_map(|b| b.godown.clone())
            .filter(|name| !masters.contains(&name.to_lowercase()))
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
        // One pass over the book instead of a search per item.
        let existing: HashMap<String, StockItemId> =
            self.book.stock_items().map(|x| (x.name.to_lowercase(), x.id)).collect();
        for i in &data.stock_items {
            let key = i.name.to_lowercase();
            if let Some(id) = existing.get(&key).copied().or_else(|| self.stock_items.get(&key).copied()) {
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
                    match (&i.gst, i.hsn.as_deref()) {
                        (Some(gst), Some(hsn)) if gst.rate.is_some() => self.rate(hsn, gst, &i.name),
                        (Some(gst), _) if gst.rate.is_none() => {
                            if let Some(c) = &gst.classification {
                                let detail = format!("its GST details come from the classification {c:?}");
                                self.report.notes.push(note(NoteKind::GstClassification, &i.name, detail));
                            }
                        }
                        _ => {}
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

    /// The importer's own map first: it holds every imported or reused
    /// ledger, so the book's search (a scan) only runs for the rest.
    fn resolve_ledger(&self, name: &str) -> Option<LedgerId> {
        self.ledgers.get(&name.to_lowercase()).copied().or_else(|| self.book.ledger_by_name(name).map(|l| l.id))
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

    /// A custom voucher type's own number series: its name, unless it is
    /// the predefined type itself.
    fn series(type_name: &str, kind: VoucherKind) -> Option<String> {
        let base = BASE_TYPES.iter().find(|(_, k)| *k == kind).map_or("", |(t, _)| t);
        let (name, _) = cut(type_name, MAX_SERIES_NAME);
        (!name.is_empty() && normalize(&name) != normalize(base)).then_some(name)
    }

    /// Tally often restarts numbering every financial year. When the same
    /// number comes back in a later year within one voucher type and
    /// series, and the company's numbering for that type never restarts,
    /// make it restart yearly, as Tally's did.
    fn yearly_numbering(&mut self, vouchers: &[&TallyVoucher]) {
        let month = self.book.fy_start_month();
        let mut years: HashMap<(VoucherKind, Option<String>, String), BTreeSet<NaiveDate>> = HashMap::new();
        for v in vouchers.iter().filter(|v| !v.optional && !v.deleted) {
            let Ok(kind) = self.voucher_kind(&v.type_name) else { continue };
            let series = Self::series(&v.type_name, kind).map(|s| s.to_lowercase());
            years.entry((kind, series, v.number.to_lowercase())).or_default().insert(fy_start(v.date, month));
        }
        let kinds: BTreeSet<VoucherKind> = years.iter().filter(|(_, y)| y.len() > 1).map(|((k, ..), _)| *k).collect();
        for kind in kinds {
            let series = self.book.numbering(kind);
            if series.restart != Restart::Never {
                continue;
            }
            let label = voucher_kind_label(kind);
            let yearly = NumberSeries { restart: Restart::Yearly, ..series };
            let detail = match self.book.set_numbering(self.at, self.user, kind, yearly) {
                Ok(()) => format!("{label} numbers repeat each financial year in Tally, so {label} numbering now restarts each financial year"),
                Err(e) => format!("{label} numbers repeat each financial year in Tally, but numbering could not be set to restart yearly ({e}); repeats get a new number"),
            };
            self.report.notes.push(note(NoteKind::YearlyNumbering, label, detail));
        }
    }

    fn count_type(&mut self, v: &TallyVoucher, kind: Option<VoucherKind>) -> &mut TypeCount {
        let name = if v.type_name.trim().is_empty() { "(no type)".to_owned() } else { v.type_name.trim().to_owned() };
        let slot = self.report.by_type.entry(name).or_default();
        if slot.kind.is_none() {
            slot.kind = kind;
        }
        slot
    }

    fn voucher(&mut self, v: &TallyVoucher) {
        let label = label(v);
        let kind = self.voucher_kind(&v.type_name).ok();
        let built = if v.deleted {
            Err(SkipReason::Deleted)
        } else if v.optional {
            Err(SkipReason::Optional)
        } else if v.cancelled {
            self.build_cancelled(v)
        } else {
            self.build_voucher(v, &label)
        };
        let Built { voucher, mut dropped, merged, mut notes } = match built {
            Ok(built) => built,
            Err(reason) => {
                self.count_type(v, kind).skipped += 1;
                return self.skip(ItemKind::Voucher, label, reason);
            }
        };
        let cancelled = voucher.cancelled;
        let lines: usize = voucher.entries.iter().map(|e| e.items.len()).sum();
        let posted = match self.post(voucher.clone(), v, &label, &mut notes) {
            // Bills or bank dates the book will not take: keep the rest.
            Err(PostError::Book(e @ (BookError::Voucher(VoucherError::BillsMismatch { .. }) | BookError::InvalidBankDate))) => {
                let mut plain = voucher.clone();
                for entry in &mut plain.entries {
                    entry.bills.clear();
                    entry.bank_date = None;
                }
                notes.retain(|n| n.kind != NoteKind::BillsDropped);
                notes.push(note(NoteKind::BillsDropped, &label, format!("{e}; posted without its bills and bank dates")));
                self.post(plain, v, &label, &mut notes)
            }
            other => other,
        };
        let posted = match posted {
            Ok(_) => Ok(lines),
            // Stock lines the book will not take: keep the accounting.
            Err(PostError::Book(e)) if lines > 0 => {
                let mut plain = voucher;
                for entry in &mut plain.entries {
                    entry.items.clear();
                }
                let posted = self.post(plain, v, &label, &mut notes).map(|_| 0);
                if posted.is_ok() {
                    dropped = Some(e.to_string());
                }
                posted
            }
            Err(e) => Err(e),
        };
        match posted {
            Ok(lines) => {
                let slot = self.count_type(v, kind);
                if cancelled {
                    slot.cancelled += 1;
                    self.report.vouchers_cancelled += 1;
                } else {
                    slot.posted += 1;
                    self.report.vouchers_posted += 1;
                    self.report.first_date = Some(self.report.first_date.map_or(v.date, |d| d.min(v.date)));
                    self.report.last_date = Some(self.report.last_date.map_or(v.date, |d| d.max(v.date)));
                }
                self.report.vouchers_numbered += usize::from(v.number.is_empty());
                self.report.inventory_lines += lines;
                if lines > 0 {
                    for item in merged {
                        self.merged(&item);
                    }
                }
                if let Some(why) = dropped {
                    self.skip(ItemKind::Inventory, label.clone(), SkipReason::StockLinesDropped(why));
                }
                if v.post_dated && !cancelled {
                    notes.push(note(
                        NoteKind::PostDated,
                        &label,
                        "Tally keeps a post-dated voucher out of its books until its date. KhataOS posts it on its date, so reports up to an earlier date leave it out, but balances without a date include it",
                    ));
                }
                self.report.notes.extend(notes);
            }
            Err(e) => {
                self.count_type(v, kind).skipped += 1;
                let reason = match e {
                    PostError::AlreadyInBooks(n) => SkipReason::AlreadyInBooks(n),
                    PostError::Book(e) => rejected(e),
                };
                self.skip(ItemKind::Voucher, label, reason);
            }
        }
    }

    /// Post a voucher. When Tally used its number twice in one series and
    /// period, the repeat gets a new number ("12 (2)") and a note; when the
    /// number was already in the company before this import, it is refused
    /// as a likely second import of the same file.
    fn post(&mut self, voucher: Voucher, v: &TallyVoucher, label: &str, notes: &mut Vec<Note>) -> Result<VoucherId, PostError> {
        let number = voucher.number.clone();
        let mut attempt = voucher;
        match self.book.post_voucher(self.at, self.user, attempt.clone()) {
            Ok(id) => {
                self.posted.insert(id);
                return Ok(id);
            }
            Err(BookError::DuplicateVoucherNumber(_)) if !v.number.is_empty() => {}
            Err(e) => return Err(PostError::Book(e)),
        }
        let series = |x: &Voucher| x.series.as_deref().map(str::to_lowercase);
        let from_before = self.book.vouchers_numbered(attempt.kind, &number).any(|o| {
            !self.posted.contains(&o.id) && o.number.eq_ignore_ascii_case(&number) && series(o) == series(&attempt)
        });
        if from_before {
            return Err(PostError::AlreadyInBooks(number));
        }
        for k in 2..1000 {
            attempt.number = format!("{number} ({k})");
            match self.book.post_voucher(self.at, self.user, attempt.clone()) {
                Err(BookError::DuplicateVoucherNumber(_)) => continue,
                Ok(id) => {
                    self.posted.insert(id);
                    let detail = format!(
                        "Tally used number {number} more than once in this voucher type and year; this one is now {}",
                        attempt.number
                    );
                    notes.push(note(NoteKind::Renumbered, label, detail));
                    return Ok(id);
                }
                Err(e) => return Err(PostError::Book(e)),
            }
        }
        Err(PostError::Book(BookError::DuplicateVoucherNumber(number)))
    }

    /// A cancelled voucher keeps its number. Its lines are kept for the
    /// record when they can be read; otherwise it is kept without lines,
    /// as Tally keeps it.
    fn build_cancelled(&self, v: &TallyVoucher) -> Result<Built, SkipReason> {
        let kind = self.voucher_kind(&v.type_name)?;
        let mut entries = Vec::with_capacity(v.entries.len());
        for e in &v.entries {
            match self.resolve_ledger(&e.ledger) {
                Some(ledger) => entries.extend(entry(ledger, e.amount)),
                None => {
                    entries.clear();
                    break;
                }
            }
        }
        let mut voucher = Voucher {
            id: VoucherId::new(),
            kind,
            number: if v.number.is_empty() { self.book.next_voucher_number(kind, v.date) } else { v.number.clone() },
            date: v.date,
            entries,
            narration: v.narration.clone(),
            cancelled: true,
            series: Self::series(&v.type_name, kind),
            stock: Vec::new(),
        };
        if voucher.validate().is_err() {
            voucher.entries.clear();
        }
        Ok(Built { voucher, dropped: None, merged: Vec::new(), notes: Vec::new() })
    }

    /// A voucher line's bill allocations in KhataOS form, or why they can't
    /// be kept. `amount` is the line's, debit positive.
    fn bills(e: &TallyEntry, date: NaiveDate) -> Result<Vec<BillAllocation>, String> {
        if let Some(problem) = &e.bill_problem {
            return Err(problem.clone());
        }
        let mut out = Vec::with_capacity(e.bills.len());
        for b in e.bills.iter().filter(|b| !b.amount.is_zero()) {
            let kind = bill_kind(b)?;
            if b.amount.is_sign_negative() != e.amount.is_sign_negative() {
                return Err("its bill allocations mix debits and credits".into());
            }
            let (reference, was_cut) = cut(&b.name, MAX_BILL_REFERENCE);
            if was_cut {
                return Err(format!("bill name {:?} is too long", b.name));
            }
            if reference.is_empty() && kind != BillKind::OnAccount {
                return Err(format!("a {} bill has no name", b.kind));
            }
            let due = match kind {
                BillKind::New | BillKind::Advance => b.credit_period.as_deref().and_then(|cp| tally_due(cp, date)),
                _ => None,
            };
            out.push(BillAllocation { kind, reference, amount: b.amount.abs(), due, forex_amount: None });
        }
        let total: Amount = out.iter().map(|b| b.amount).sum();
        if !out.is_empty() && total != e.amount.abs() {
            return Err(format!("its bills add up to {total}, not the line's {}", e.amount.abs()));
        }
        Ok(out)
    }

    /// The voucher to post, why its stock lines were left off (if they
    /// were), the items whose batch allocations were merged, and notes.
    /// Stock lines, bills and bank dates never cause a voucher to be skipped.
    fn build_voucher(&mut self, v: &TallyVoucher, label: &str) -> Result<Built, SkipReason> {
        let kind = self.voucher_kind(&v.type_name)?;
        let mut resolved = Vec::with_capacity(v.entries.len());
        for e in &v.entries {
            let ledger = self.resolve_ledger(&e.ledger).ok_or_else(|| SkipReason::UnknownLedger(e.ledger.clone()))?;
            resolved.push((ledger, e));
        }
        let mut notes = Vec::new();
        let mut lines = Vec::with_capacity(resolved.len());
        for (ledger, e) in resolved {
            let bills = Self::bills(e, v.date).unwrap_or_else(|why| {
                notes.push(note(NoteKind::BillsDropped, label, format!("{}: {why}; posted without its bills", e.ledger)));
                Vec::new()
            });
            let bank_date = e.bank_date.filter(|d| *d >= v.date && self.book.is_cash_or_bank(ledger));
            self.bank_dates_dropped += usize::from(e.bank_date.is_some() && bank_date.is_none());
            self.cost_centre_lines += usize::from(e.cost_centres);
            lines.push(Line { ledger, amount: e.amount, bills, bank_date });
        }
        let plain = || lines.iter().filter_map(line_entry).collect::<Vec<_>>();
        let (entries, dropped, merged) = match &v.inventory_problem {
            Some(problem) => (plain(), Some(problem.clone()), Vec::new()),
            None if v.inventory.is_empty() => (plain(), None, Vec::new()),
            None => match self.with_stock(&lines, &v.inventory, label, &mut notes) {
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
            cancelled: false,
            series: Self::series(&v.type_name, kind),
            stock: Vec::new(),
        };
        Ok(Built { voucher, dropped, merged, notes })
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
        signed: &[Line],
        lines: &[TallyStockLine],
        label: &str,
        notes: &mut Vec<Note>,
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
        // one's position. Bills on such a ledger don't follow the merge.
        let mut merged: Vec<(Line, bool)> = Vec::new();
        for line in signed {
            let stock = by_ledger.iter().any(|(id, _)| *id == line.ledger);
            match merged.iter_mut().find(|(m, s)| stock && *s && m.ledger == line.ledger) {
                Some((m, _)) => {
                    m.amount += line.amount;
                    m.bills.extend(line.bills.iter().cloned());
                    m.bank_date = None;
                }
                None => merged.push((line.clone(), stock)),
            }
        }
        let mut entries = Vec::with_capacity(merged.len());
        for (mut line, stock) in merged {
            if stock && !line.bills.is_empty() {
                let name = self.book.ledger(line.ledger).map(|l| l.name.clone()).unwrap_or_default();
                notes.push(note(NoteKind::BillsDropped, label, format!("{name}: bills on a line that carries stock items are not kept")));
                line.bills.clear();
            }
            let Some(mut e) = line_entry(&line) else {
                if stock {
                    return Err("the stock lines' ledger line nets to zero".into());
                }
                continue;
            };
            if stock {
                let (_, ls) = by_ledger.iter().find(|(id, _)| *id == line.ledger).expect("ledger carries stock");
                for (l, sl) in ls {
                    if l.inward() != (line.amount > Amount::ZERO) {
                        return Err(format!("{:?} moves stock the opposite way to its ledger line", l.item));
                    }
                    e = e.item(sl.clone());
                }
                let total: Amount = e.items.iter().map(|l| l.amount).sum();
                if total != e.amount {
                    return Err(format!("item amounts ({total}) do not add up to their ledger line ({})", e.amount));
                }
            }
            entries.push(e);
        }
        if let Some((ledger, _)) = by_ledger.iter().find(|(id, _)| !signed.iter().any(|l| l.ledger == *id)) {
            let name = self.book.ledger(*ledger).map(|l| l.name.clone()).unwrap_or_default();
            return Err(format!("ledger {name:?} of the stock lines has no amount on the voucher"));
        }
        Ok((entries, merged_detail))
    }

    /// Notes that cover many lines at once, and the bill counts.
    fn summary_notes(&mut self) {
        let mut counts = self.report.bills;
        for b in self.posted.iter().filter_map(|id| self.book.voucher(*id)).flat_map(|v| &v.entries).flat_map(|e| &e.bills) {
            match b.kind {
                BillKind::New => counts.new_ref += 1,
                BillKind::Against => counts.against += 1,
                BillKind::Advance => counts.advance += 1,
                BillKind::OnAccount => counts.on_account += 1,
            }
        }
        self.report.bills = counts;
        if self.cost_centre_lines > 0 {
            self.report.notes.push(Note {
                kind: NoteKind::CostCentres,
                name: String::new(),
                detail: format!(
                    "{} ledger lines had cost centre allocations; the ledger amounts are imported, the cost centre split is not",
                    self.cost_centre_lines
                ),
                count: self.cost_centre_lines,
            });
        }
        if self.bank_dates_dropped > 0 {
            self.report.notes.push(Note {
                kind: NoteKind::BankDateDropped,
                name: String::new(),
                detail: format!(
                    "{} bank dates were before their voucher's date, or on a ledger that isn't a bank ledger here",
                    self.bank_dates_dropped
                ),
                count: self.bank_dates_dropped,
            });
        }
    }

    /// Checks that tell whether the books match Tally's: the opening
    /// balances, the Day Book's dates, and the closing balances Tally gave.
    fn checks(&mut self, data: &TallyData) {
        let d = self.book.opening_difference();
        self.report.opening_difference = d;
        if !d.is_zero() {
            self.report.notes.push(note(
                NoteKind::OpeningDifference,
                "",
                format!(
                    "The opening balances differ by {} {}. Tally shows the same figure as \"Difference in opening balances\" when its own opening balances don't agree; if Tally shows none, a ledger or stock item with an opening balance was skipped",
                    d.abs(),
                    dr_cr(d)
                ),
            ));
        }
        if self.report.vouchers_posted > 0 {
            // A Day Book alone is fine when the company already has its
            // opening balances (masters imported before).
            if data.ledgers.is_empty() && self.book.ledgers().all(|l| l.opening.is_zero()) {
                self.report.notes.push(note(
                    NoteKind::NoLedgerMasters,
                    "",
                    "The files have vouchers but no ledger masters, and no ledger here has an opening balance. Import the masters export together with the Day Book",
                ));
            } else if self.report.ledgers_created > 0 && data.ledgers.iter().all(|l| l.opening.is_zero()) {
                self.report.notes.push(note(
                    NoteKind::NoOpeningBalances,
                    "",
                    "No ledger in the masters has an opening balance. That is right only if the company's books begin with the first voucher",
                ));
            }
        }
        if let Some(first) = self.report.first_date {
            // A company that hasn't set its year yet: Tally's default in India.
            let month = self.book.profile().and_then(|p| p.fy_start_month).unwrap_or(4);
            let start = fy_start(first, month);
            let gap = (first - start).num_days();
            if gap > LATE_START_DAYS {
                let (first, start) = (first.format("%d-%b-%Y"), start.format("%d-%b-%Y"));
                self.report.notes.push(note(
                    NoteKind::DayBookStartsLate,
                    "",
                    format!(
                        "The first voucher is dated {first}, {gap} days after its financial year began on {start}. Tally's ledger opening balances are as at the books beginning date. If the books begin before {first}, export the Day Book from the books beginning date: otherwise closing balances here differ from Tally's, though the Trial Balance still agrees"
                    ),
                ));
            }
        }
        self.compare_balances(data);
    }

    /// Compare closing balances with those the files carry: a Trial Balance
    /// exported as XML (by ledger or by group), and `CLOSINGBALANCE` on
    /// ledger masters.
    fn compare_balances(&mut self, data: &TallyData) {
        let given: Vec<(&str, Amount)> = data
            .ledgers
            .iter()
            .filter_map(|l| Some((l.name.as_str(), l.closing?)))
            .chain(data.balances.iter().map(|b| (b.name.as_str(), b.closing)))
            .collect();
        if given.is_empty() {
            return;
        }
        let balances = self.book.balances(None);
        // Each group's total: every ledger's balance added to its groups.
        let mut group_totals: HashMap<GroupId, Amount> = HashMap::new();
        for l in self.book.ledgers() {
            let bal = balances.get(&l.id).copied().unwrap_or_default();
            for g in self.ancestors(l.group) {
                *group_totals.entry(g).or_default() += bal;
            }
        }
        let mut seen = HashSet::new();
        for (name, tally) in given {
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            let ours = match self.resolve_ledger(name) {
                Some(id) => Some(balances.get(&id).copied().unwrap_or_default()),
                None => self.resolve_group(name).map(|g| group_totals.get(&g).copied().unwrap_or_default()),
            };
            let Some(ours) = ours else {
                self.report.notes.push(note(
                    NoteKind::BalanceNotFound,
                    name,
                    "Tally gave a closing balance for it, but no ledger or group here has that name",
                ));
                continue;
            };
            self.report.balances.compared += 1;
            if ours == tally {
                self.report.balances.matched += 1;
            } else {
                self.report.balances.differences.push(BalanceDifference { name: name.to_owned(), tally, khataos: ours });
                self.report.notes.push(note(
                    NoteKind::BalanceDiffers,
                    name,
                    format!("Tally {} {}, KhataOS {} {}", tally.abs(), dr_cr(tally), ours.abs(), dr_cr(ours)),
                ));
            }
        }
    }
}

fn dr_cr(a: Amount) -> &'static str {
    if a < Amount::ZERO { "Cr" } else { "Dr" }
}

/// KhataOS's kind of a Tally bill allocation.
fn bill_kind(b: &TallyBill) -> Result<BillKind, String> {
    match normalize(&b.kind).as_str() {
        "newref" | "new" => Ok(BillKind::New),
        "agstref" | "againstref" | "agst" => Ok(BillKind::Against),
        "advance" => Ok(BillKind::Advance),
        "onaccount" => Ok(BillKind::OnAccount),
        _ => Err(format!("bill type {:?} is not New Ref, Agst Ref, Advance or On Account", b.kind)),
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

/// The entry for a line, with its bills and bank date.
fn line_entry(line: &Line) -> Option<Entry> {
    let mut e = entry(line.ledger, line.amount)?;
    e.bills = line.bills.clone();
    e.bank_date = line.bank_date;
    Some(e)
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

//! Inventory: units of measure, stock groups and stock items.
//!
//! Stock moves only through item lines on vouchers ([`StockLine`], attached
//! to a voucher's sales / purchase ledger entry, as in an item invoice). A
//! line under a debit entry brings stock in (purchases, sales returns); a
//! line under a credit entry sends it out (sales, purchase returns).
//!
//! Stock is valued at weighted average cost: opening value plus everything
//! bought, divided by opening quantity plus everything bought. Closing stock
//! flows into Profit & Loss and the Balance Sheet, as when accounts and
//! inventory are integrated in Tally.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::voucher::DrCr;
use crate::{Amount, Book};

/// Quantities are exact decimals too (kg, litres, metres…).
pub type Quantity = rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnitId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StockGroupId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StockItemId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GodownId(pub Uuid);

macro_rules! new_id {
    ($t:ty) => {
        impl $t {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }
        impl Default for $t {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}
new_id!(UnitId);
new_id!(StockGroupId);
new_id!(StockItemId);
new_id!(GodownId);

/// A place stock is kept (warehouse, shop, branch). Lines without a godown
/// are at the company's main location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Godown {
    pub id: GodownId,
    pub name: String,
    /// `None`: top level.
    pub parent: Option<GodownId>,
}

/// Longest batch name accepted.
pub const MAX_BATCH: usize = 40;

/// A unit of measure, e.g. `Nos`, `kg`, `ltr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    /// Short symbol shown next to quantities.
    pub symbol: String,
    /// Formal name, e.g. "Numbers", "Kilograms".
    #[serde(default)]
    pub name: String,
    /// Decimal places a quantity in this unit may have (0–4).
    #[serde(default)]
    pub decimals: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockGroup {
    pub id: StockGroupId,
    pub name: String,
    /// `None` for a top-level stock group.
    pub parent: Option<StockGroupId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockItem {
    pub id: StockItemId,
    pub name: String,
    /// `None`: listed under "Primary".
    pub group: Option<StockGroupId>,
    pub unit: UnitId,
    /// Quantity on hand on the books-beginning date.
    #[serde(default)]
    pub opening_qty: Quantity,
    /// Value of the opening quantity.
    #[serde(default)]
    pub opening_value: Amount,
    /// What the tax rate is looked up by: an HSN/SAC code or a category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_class: Option<String>,
    /// How closing stock of this item is valued.
    #[serde(default, skip_serializing_if = "Costing::is_default")]
    pub costing: Costing,
}

/// Stock valuation method, chosen per item as in Tally.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Costing {
    /// Weighted average cost of everything available in the period.
    #[default]
    AverageCost,
    /// First in, first out: what is left is valued at the latest purchases.
    Fifo,
    /// At the rate of the most recent purchase (or other stock coming in).
    LastPurchase,
}

impl Costing {
    fn is_default(&self) -> bool {
        *self == Costing::AverageCost
    }
}

/// One item on a voucher line: quantity × rate = amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockLine {
    pub item: StockItemId,
    pub qty: Quantity,
    pub rate: Amount,
    pub amount: Amount,
    /// Where the stock comes in or goes out; `None`: main location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub godown: Option<GodownId>,
    /// Batch or lot name, for items tracked by batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<String>,
}

impl StockLine {
    pub fn new(item: StockItemId, qty: Quantity, rate: Amount) -> Self {
        Self { item, qty, rate, amount: (qty * rate).round_dp(2), godown: None, batch: None }
    }

    /// Put the line at a godown (builder style).
    pub fn at(mut self, godown: GodownId) -> Self {
        self.godown = Some(godown);
        self
    }

    /// Give the line a batch (builder style).
    pub fn batch(mut self, name: &str) -> Self {
        self.batch = Some(name.to_owned());
        self
    }
}

/// Opening, in, out and closing of one item over a period.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StockMovement {
    pub opening_qty: Quantity,
    pub opening_value: Amount,
    pub inward_qty: Quantity,
    pub inward_value: Amount,
    pub outward_qty: Quantity,
    /// Sales value of outward lines (what customers paid, not cost).
    pub outward_value: Amount,
    pub closing_qty: Quantity,
    /// Closing quantity at weighted average cost.
    pub closing_value: Amount,
}

impl StockMovement {
    /// Weighted average cost per unit of everything available in the period.
    pub fn average_cost(&self) -> Amount {
        let qty = self.opening_qty + self.inward_qty;
        if qty <= Quantity::ZERO { Amount::ZERO } else { (self.opening_value + self.inward_value) / qty }
    }
}

/// Every active item line dated within `[from, to]` (all dates when `None`),
/// with its direction: `true` = inward.
fn lines(book: &Book, from: Option<NaiveDate>, to: Option<NaiveDate>) -> impl Iterator<Item = (NaiveDate, bool, &StockLine)> {
    book.active_vouchers()
        .filter(move |v| from.is_none_or(|f| v.date >= f) && to.is_none_or(|t| v.date <= t))
        .flat_map(|v| {
            let invoiced = v.entries.iter().flat_map(move |e| e.items.iter().map(move |l| (v.date, e.side == DrCr::Dr, l)));
            let journal = v.stock.iter().map(move |m| (v.date, m.inward, &m.line));
            invoiced.chain(journal)
        })
}

/// Movement of every stock item over `[from, to]`. Movements before `from`
/// are folded into the opening at average cost.
pub fn movements(book: &Book, from: NaiveDate, to: NaiveDate) -> BTreeMap<StockItemId, StockMovement> {
    let mut out: BTreeMap<StockItemId, StockMovement> = book
        .stock_items()
        .map(|i| (i.id, StockMovement { opening_qty: i.opening_qty, opening_value: i.opening_value, ..Default::default() }))
        .collect();
    // Before the period: closing position becomes the period's opening.
    let mut before: BTreeMap<StockItemId, StockMovement> = BTreeMap::new();
    let earlier = from.pred_opt().into_iter().flat_map(|end| lines(book, None, Some(end)));
    for (_, inward, l) in earlier {
        let m = before.entry(l.item).or_default();
        if inward {
            m.inward_qty += l.qty;
            m.inward_value += l.amount;
        } else {
            m.outward_qty += l.qty;
        }
    }
    for (id, b) in before {
        let Some(m) = out.get_mut(&id) else { continue };
        let all = StockMovement { inward_qty: b.inward_qty, inward_value: b.inward_value, ..m.clone() };
        let qty = m.opening_qty + b.inward_qty - b.outward_qty;
        m.opening_value = (qty * all.average_cost()).round_dp(2);
        m.opening_qty = qty;
    }
    for (_, inward, l) in lines(book, Some(from), Some(to)) {
        let Some(m) = out.get_mut(&l.item) else { continue };
        if inward {
            m.inward_qty += l.qty;
            m.inward_value += l.amount;
        } else {
            m.outward_qty += l.qty;
            m.outward_value += l.amount;
        }
    }
    for m in out.values_mut() {
        m.closing_qty = m.opening_qty + m.inward_qty - m.outward_qty;
        m.closing_value = (m.closing_qty * m.average_cost()).round_dp(2);
    }
    // FIFO and last-purchase items: value the opening and closing quantity
    // from the item's own history instead of the period average.
    let others: Vec<&StockItem> = book.stock_items().filter(|i| i.costing != Costing::AverageCost).collect();
    if !others.is_empty() {
        let mut history: BTreeMap<StockItemId, Vec<(NaiveDate, bool, Quantity, Amount)>> = BTreeMap::new();
        for (date, inward, l) in lines(book, None, Some(to)) {
            history.entry(l.item).or_default().push((date, inward, l.qty, l.amount));
        }
        for i in others {
            let h = history.get(&i.id).map_or(&[][..], Vec::as_slice);
            let Some(m) = out.get_mut(&i.id) else { continue };
            if let Some(end) = from.pred_opt() {
                m.opening_value = value_on_hand(i, h, end, m.opening_qty);
            }
            m.closing_value = value_on_hand(i, h, to, m.closing_qty);
        }
    }
    out
}

/// Value of `qty` on hand at the end of `end` for a FIFO or last-purchase
/// item, from its history (date order: opening first, then each line).
fn value_on_hand(item: &StockItem, history: &[(NaiveDate, bool, Quantity, Amount)], end: NaiveDate, qty: Quantity) -> Amount {
    if qty <= Quantity::ZERO {
        return Amount::ZERO;
    }
    let inward = || history.iter().filter(move |(d, inward, ..)| *inward && *d <= end);
    let rate_of = |q: Quantity, v: Amount| if q.is_zero() { Amount::ZERO } else { v / q };
    match item.costing {
        Costing::AverageCost => Amount::ZERO,
        Costing::LastPurchase => {
            let rate = inward()
                .next_back()
                .map(|(_, _, q, v)| rate_of(*q, *v))
                .unwrap_or_else(|| rate_of(item.opening_qty, item.opening_value));
            (qty * rate).round_dp(2)
        }
        Costing::Fifo => {
            // What is left is the newest layers: walk back from the latest.
            let mut layers: Vec<(Quantity, Amount)> = vec![(item.opening_qty, item.opening_value)];
            layers.extend(inward().map(|(_, _, q, v)| (*q, *v)));
            let mut left = qty;
            let mut value = Amount::ZERO;
            for (q, v) in layers.iter().rev() {
                if left <= Quantity::ZERO {
                    break;
                }
                let take = left.min(*q);
                value += take * rate_of(*q, *v);
                left -= take;
            }
            // More on hand than ever came in (stock found): at the oldest rate.
            if left > Quantity::ZERO {
                value += left * layers.first().map_or(Amount::ZERO, |(q, v)| rate_of(*q, *v));
            }
            value.round_dp(2)
        }
    }
}

/// Value of all stock on hand at the end of `as_at` (all dates when `None`).
pub fn closing_stock(book: &Book, as_at: Option<NaiveDate>) -> Amount {
    if book.stock_items().next().is_none() {
        return Amount::ZERO;
    }
    let to = as_at.unwrap_or(NaiveDate::MAX);
    movements(book, NaiveDate::MIN, to).values().map(|m| m.closing_value).sum()
}

/// Value of all stock on hand at the start of `from`.
pub fn opening_stock(book: &Book, from: NaiveDate) -> Amount {
    if book.stock_items().next().is_none() {
        return Amount::ZERO;
    }
    movements(book, from, from).values().map(|m| m.opening_value).sum()
}

/// Sum of the stock items' own opening values (part of the opening trial balance).
pub fn opening_stock_value(book: &Book) -> Amount {
    book.stock_items().map(|i| i.opening_value).sum()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StockSummaryRow {
    pub item: StockItemId,
    pub name: String,
    /// Stock group path from the top ("Raw Material / Steel"); empty for Primary.
    pub group: String,
    pub unit: String,
    #[serde(flatten)]
    pub movement: StockMovement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StockSummary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// By group path, then item name.
    pub rows: Vec<StockSummaryRow>,
    pub opening_value: Amount,
    pub inward_value: Amount,
    pub outward_value: Amount,
    pub closing_value: Amount,
}

/// Stock group path of a group, top first.
pub fn stock_group_path(book: &Book, group: Option<StockGroupId>) -> String {
    let mut names = Vec::new();
    let mut at = group;
    for _ in 0..=book.stock_groups().count() {
        let Some(g) = at.and_then(|id| book.stock_group(id)) else { break };
        names.push(g.name.as_str());
        at = g.parent;
    }
    names.reverse();
    names.join(" / ")
}

/// Stock Summary for `[from, to]`: every item's opening, inward, outward
/// and closing quantity and value.
pub fn stock_summary(book: &Book, from: NaiveDate, to: NaiveDate) -> StockSummary {
    let mv = movements(book, from, to);
    let mut rows: Vec<StockSummaryRow> = book
        .stock_items()
        .map(|i| StockSummaryRow {
            item: i.id,
            name: i.name.clone(),
            group: stock_group_path(book, i.group),
            unit: book.unit(i.unit).map(|u| u.symbol.clone()).unwrap_or_default(),
            movement: mv.get(&i.id).cloned().unwrap_or_default(),
        })
        .collect();
    rows.sort_by_key(|r| (r.group.to_lowercase(), r.name.to_lowercase()));
    let total = |f: fn(&StockMovement) -> Amount| rows.iter().map(|r| f(&r.movement)).sum();
    StockSummary {
        from,
        to,
        opening_value: total(|m| m.opening_value),
        inward_value: total(|m| m.inward_value),
        outward_value: total(|m| m.outward_value),
        closing_value: total(|m| m.closing_value),
        rows,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StockVoucherRow {
    pub date: NaiveDate,
    pub voucher: crate::VoucherId,
    pub kind: crate::VoucherKind,
    pub number: String,
    /// The party (or other ledger) on the voucher.
    pub particulars: String,
    pub inward_qty: Quantity,
    pub inward_value: Amount,
    pub outward_qty: Quantity,
    pub outward_value: Amount,
    /// Quantity on hand after this row.
    pub closing_qty: Quantity,
}

/// Stock Item Vouchers: the movements of one item in `[from, to]` (the
/// Stock Summary drill-down).
pub fn item_vouchers(book: &Book, item: StockItemId, from: NaiveDate, to: NaiveDate) -> Option<(StockMovement, Vec<StockVoucherRow>)> {
    book.stock_item(item)?;
    let summary = movements(book, from, to).remove(&item).unwrap_or_default();
    let mut vs: Vec<&crate::Voucher> = book
        .active_vouchers()
        .filter(|v| v.date >= from && v.date <= to && v.entries.iter().any(|e| e.items.iter().any(|l| l.item == item)))
        .collect();
    vs.sort_by_key(|v| (v.date, v.id));
    let mut qty = summary.opening_qty;
    let rows = vs
        .into_iter()
        .map(|v| {
            let mut row = StockVoucherRow {
                date: v.date,
                voucher: v.id,
                kind: v.kind,
                number: v.number.clone(),
                particulars: v
                    .entries
                    .iter()
                    .find(|e| e.items.is_empty())
                    .and_then(|e| book.ledger(e.ledger))
                    .map(|l| l.name.clone())
                    .unwrap_or_default(),
                inward_qty: Quantity::ZERO,
                inward_value: Amount::ZERO,
                outward_qty: Quantity::ZERO,
                outward_value: Amount::ZERO,
                closing_qty: Quantity::ZERO,
            };
            for e in &v.entries {
                for l in e.items.iter().filter(|l| l.item == item) {
                    if e.side == DrCr::Dr {
                        row.inward_qty += l.qty;
                        row.inward_value += l.amount;
                    } else {
                        row.outward_qty += l.qty;
                        row.outward_value += l.amount;
                    }
                }
            }
            qty += row.inward_qty - row.outward_qty;
            row.closing_qty = qty;
            row
        })
        .collect();
    Some((summary, rows))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocationRow {
    /// `None`: main location.
    pub godown: Option<GodownId>,
    pub godown_name: String,
    /// `None` when the item is not tracked by batch.
    pub batch: Option<String>,
    pub item: StockItemId,
    pub item_name: String,
    pub unit: String,
    pub qty: Quantity,
    /// Quantity at the item's weighted average cost.
    pub value: Amount,
}

/// Stock on hand at the end of `as_at` by godown and batch: Godown Summary
/// and Batch Summary. Opening stock sits at the main location.
pub fn stock_by_location(book: &Book, as_at: NaiveDate) -> Vec<LocationRow> {
    let cost: BTreeMap<StockItemId, Amount> =
        movements(book, NaiveDate::MIN, as_at).into_iter().map(|(id, m)| (id, m.average_cost())).collect();
    let mut qty: BTreeMap<(Option<GodownId>, Option<String>, StockItemId), Quantity> = BTreeMap::new();
    for i in book.stock_items().filter(|i| !i.opening_qty.is_zero()) {
        *qty.entry((None, None, i.id)).or_default() += i.opening_qty;
    }
    for (_, inward, l) in lines(book, None, Some(as_at)) {
        let q = qty.entry((l.godown, l.batch.clone(), l.item)).or_default();
        if inward { *q += l.qty } else { *q -= l.qty }
    }
    let mut rows: Vec<LocationRow> = qty
        .into_iter()
        .filter(|(_, q)| !q.is_zero())
        .filter_map(|((godown, batch, item), q)| {
            let i = book.stock_item(item)?;
            Some(LocationRow {
                godown,
                godown_name: godown.and_then(|g| book.godown(g)).map_or_else(|| "Main Location".to_owned(), |g| g.name.clone()),
                batch,
                item,
                item_name: i.name.clone(),
                unit: book.unit(i.unit).map(|u| u.symbol.clone()).unwrap_or_default(),
                qty: q,
                value: (q * cost.get(&item).copied().unwrap_or_default()).round_dp(2),
            })
        })
        .collect();
    rows.sort_by_key(|r| (r.godown.is_some(), r.godown_name.to_lowercase(), r.item_name.to_lowercase(), r.batch.clone()));
    rows
}

/// The rate this item was last bought (`inward`) or sold at, preferring the
/// same party, for the rate a new invoice line starts with.
pub fn last_rate(book: &Book, item: StockItemId, inward: bool, party: Option<crate::LedgerId>) -> Option<Amount> {
    let mut best: Option<(bool, NaiveDate, crate::VoucherId, Amount)> = None;
    for v in book.active_vouchers() {
        let same_party = party.is_some_and(|p| v.entries.iter().any(|e| e.ledger == p));
        for e in v.entries.iter().filter(|e| (e.side == DrCr::Dr) == inward) {
            for l in e.items.iter().filter(|l| l.item == item) {
                let key = (same_party, v.date, v.id, l.rate);
                if best.as_ref().is_none_or(|b| (key.0, key.1, key.2) > (b.0, b.1, b.2)) {
                    best = Some(key);
                }
            }
        }
    }
    best.map(|b| b.3)
}

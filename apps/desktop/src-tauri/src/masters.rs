//! Altering and deleting masters (groups, ledgers) and the inventory
//! masters: units of measure, stock groups and stock items.
//!
//! Every change is one audited operation on the book, validated by the core
//! exactly like a sync from another device.

use std::collections::BTreeMap;

use khata_core::inventory::{self, Costing, stock_group_path};
use khata_core::numbering::{MAX_AFFIX, MAX_WIDTH};
use khata_core::{
    Amount, Godown, GodownId, GroupId, LedgerId, NumberSeries, Quantity, Restart, StockGroup, StockGroupId, StockItem, StockItemId,
    Unit, UnitId, VoucherKind,
};
use khata_core::UserRole;
use serde::{Deserialize, Serialize};

use crate::api::{
    self, ApiResult, CreatedLedger, GroupDto, LedgerDto, Slot, amt, err, group_dto, group_is_party, ledger_dto,
    now, open_mut, open_ref, os_user, parse_amount, parse_uuid, party_tax_id,
};

// ------------------------------------------------------------ groups

pub fn get_group(slot: &Slot, id: &str) -> ApiResult<GroupDto> {
    let book = open_ref(slot)?;
    let g = book.group(GroupId(parse_uuid(id, "group")?)).ok_or("unknown group")?;
    Ok(group_dto(book, g))
}

/// Rename a group, change its aliases, or move it under another group of
/// the same kind. `parent_id` empty keeps the current parent (a primary
/// group stays primary).
pub fn alter_group(slot: &mut Slot, id: &str, name: &str, parent_id: &str, aliases: &[String]) -> ApiResult<GroupDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let id = GroupId(parse_uuid(id, "group")?);
    let mut g = book.group(id).ok_or("unknown group")?.clone();
    let name = name.trim();
    if name.is_empty() {
        return Err("group name is required".into());
    }
    g.name = name.to_owned();
    g.aliases = aliases.iter().map(|a| a.trim().to_owned()).filter(|a| !a.is_empty()).collect();
    if parent_id.trim().is_empty() {
        // Keep the current parent.
    } else if g.parent.is_some() {
        g.parent = Some(GroupId(parse_uuid(parent_id, "parent group")?));
    } else {
        return Err("a primary group can be renamed but not moved under another group".into());
    }
    book.alter_group(now(), &os_user(), g).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(group_dto(book, book.group(id).expect("just altered")))
}

pub fn delete_group(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    book.delete_group(now(), &os_user(), GroupId(parse_uuid(id, "group")?)).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

// ------------------------------------------------------------ ledgers

pub fn get_ledger(slot: &Slot, id: &str) -> ApiResult<LedgerDto> {
    let book = open_ref(slot)?;
    let l = book.ledger(LedgerId(parse_uuid(id, "ledger")?)).ok_or("unknown ledger")?;
    Ok(ledger_dto(book, l, &book.balances(None)))
}

#[derive(Debug, Clone, Deserialize)]
pub struct LedgerInput {
    pub name: String,
    pub group_id: String,
    #[serde(default)]
    pub opening: String,
    #[serde(default)]
    pub credit_days: Option<u32>,
    #[serde(default)]
    pub tax_id: Option<String>,
    #[serde(default)]
    pub tax_id_override: Option<String>,
    /// Duty ledgers: the tax collected ("CGST"); empty clears it. Absent:
    /// keep the ledger's current tax type.
    #[serde(default)]
    pub tax_component: Option<String>,
    #[serde(default)]
    pub tax_side: Option<khata_core::TaxSide>,
    /// Postal and contact details; absent keeps the current ones, an
    /// all-empty value clears them.
    #[serde(default)]
    pub mailing: Option<khata_core::Mailing>,
    /// HSN/SAC or tax category for a sales or service ledger; absent keeps,
    /// empty clears.
    #[serde(default)]
    pub tax_class: Option<String>,
    /// PAN (India); absent keeps, empty clears.
    #[serde(default)]
    pub pan: Option<String>,
    /// TDS settings (India); absent keeps, null clears.
    #[serde(default, deserialize_with = "crate::tds::some")]
    pub tds: Option<Option<khata_core::LedgerTds>>,
}

/// Alter a ledger in one audited step. Credit period and tax ID only stay
/// on party ledgers; an invalid tax ID needs a reason, as on create.
pub fn alter_ledger(slot: &mut Slot, id: &str, input: &LedgerInput) -> ApiResult<CreatedLedger> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let id = LedgerId(parse_uuid(id, "ledger")?);
    let mut l = book.ledger(id).ok_or("unknown ledger")?.clone();
    let name = input.name.trim();
    if name.is_empty() {
        return Err("ledger name is required".into());
    }
    l.name = name.to_owned();
    l.group = GroupId(parse_uuid(&input.group_id, "group")?);
    if book.group(l.group).is_none() {
        return Err("unknown group".into());
    }
    l.opening = if input.opening.trim().is_empty() { Amount::ZERO } else { parse_amount(&input.opening)? };
    let party = group_is_party(book, l.group);
    l.credit_days = input.credit_days.filter(|_| party);
    let unchanged = l.tax_id.as_deref().map(str::trim) == input.tax_id.as_deref().map(str::trim) && l.tax_id.is_some();
    if !unchanged {
        let tax = party_tax_id(book, l.group, input.tax_id.as_deref(), input.tax_id_override.as_deref())?;
        (l.tax_id, l.tax_id_override) = match tax {
            Some((t, reason)) => (Some(t), reason),
            None => (None, None),
        };
    } else if !party {
        return Err("only party ledgers (Sundry Debtors and Sundry Creditors) have a tax registration".into());
    }
    // Group and tax type change in one audited step, so a ledger can move
    // out of Duties & Taxes and drop its tax type together.
    if let Some(component) = &input.tax_component {
        let component = component.trim().to_ascii_uppercase();
        l.tax = match (component.is_empty(), input.tax_side) {
            (true, _) => None,
            (false, Some(side)) => Some(khata_core::LedgerTax { component, side }),
            (false, None) => return Err("say whether the tax is on sales (output) or purchases (input)".into()),
        };
    }
    if let Some(c) = &input.tax_class {
        l.tax_class = Some(khata_core::book::normalise_tax_class(c)).filter(|c| !c.is_empty());
    }
    if let Some(m) = &input.mailing {
        l.mailing = (*m != khata_core::Mailing::default()).then(|| m.clone());
    }
    crate::tds::apply_ledger_input(book, &mut l, input.pan.as_deref(), input.tds.as_ref())?;
    book.alter_ledger(now(), &os_user(), l).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(CreatedLedger {
        ledger: ledger_dto(book, book.ledger(id).expect("just altered"), &book.balances(None)),
        opening_difference: amt(book.opening_difference()),
    })
}

pub fn delete_ledger(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    book.delete_ledger(now(), &os_user(), LedgerId(parse_uuid(id, "ledger")?)).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

// ------------------------------------------------------------ voucher types

/// A voucher type's numbering, with the next number it would give today.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NumberingDto {
    pub kind: VoucherKind,
    pub restart: Restart,
    pub prefix: String,
    pub suffix: String,
    pub width: u8,
    /// Next number for a voucher dated today.
    pub next_number: String,
    /// Vouchers of this type already saved.
    pub used: usize,
}

fn numbering_dto(book: &khata_core::Book, kind: VoucherKind) -> NumberingDto {
    let s = book.numbering(kind);
    NumberingDto {
        kind,
        restart: s.restart,
        prefix: s.prefix,
        suffix: s.suffix,
        width: s.width,
        next_number: book.next_voucher_number(kind, api::today()),
        used: book.vouchers().filter(|v| v.kind == kind).count(),
    }
}

pub fn list_numbering(slot: &Slot) -> ApiResult<Vec<NumberingDto>> {
    let book = open_ref(slot)?;
    Ok(VOUCHER_KINDS.iter().map(|k| numbering_dto(book, *k)).collect())
}

const VOUCHER_KINDS: [VoucherKind; 8] = [
    VoucherKind::Contra,
    VoucherKind::Payment,
    VoucherKind::Receipt,
    VoucherKind::Journal,
    VoucherKind::Sales,
    VoucherKind::Purchase,
    VoucherKind::CreditNote,
    VoucherKind::DebitNote,
];

/// Set a voucher type's series. Refused when saved vouchers would then
/// share a number in one period, or the country isn't set yet.
pub fn set_numbering(slot: &mut Slot, kind: VoucherKind, restart: Restart, prefix: &str, suffix: &str, width: u8) -> ApiResult<NumberingDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Owner)?;
    let series = NumberSeries { restart, prefix: prefix.trim().to_owned(), suffix: suffix.trim().to_owned(), width };
    if !series.is_valid() {
        return Err(format!(
            "prefix and suffix can have at most {MAX_AFFIX} characters and only the tokens {{FY}}, {{YYYY}}, {{YY}} and {{MM}}; \
             padding is at most {MAX_WIDTH} digits"
        ));
    }
    book.set_numbering(now(), &os_user(), kind, series).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(numbering_dto(book, kind))
}

// ------------------------------------------------------------ godowns

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GodownDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub parent_name: Option<String>,
}

fn godown_dto(book: &khata_core::Book, g: &Godown) -> GodownDto {
    GodownDto {
        id: g.id.0.to_string(),
        name: g.name.clone(),
        parent_id: g.parent.map(|p| p.0.to_string()),
        parent_name: g.parent.and_then(|p| book.godown(p)).map(|p| p.name.clone()),
    }
}

pub fn list_godowns(slot: &Slot, query: &str) -> ApiResult<Vec<GodownDto>> {
    let book = open_ref(slot)?;
    let q = query.trim().to_lowercase();
    let mut out: Vec<GodownDto> =
        book.godowns().filter(|g| g.name.to_lowercase().contains(&q)).map(|g| godown_dto(book, g)).collect();
    out.sort_by_key(|g| g.name.to_lowercase());
    Ok(out)
}

/// Create (`id` empty) or alter a godown; `parent_id` empty: top level.
pub fn save_godown(slot: &mut Slot, id: &str, name: &str, parent_id: &str) -> ApiResult<GodownDto> {
    let (store, book) = open_mut(slot)?;
    if !id.trim().is_empty() {
        crate::api::require(book, UserRole::Accountant)?;
    }
    let id = optional_id(id, "godown")?.map_or_else(GodownId::new, GodownId);
    let godown = Godown { id, name: name.trim().to_owned(), parent: optional_id(parent_id, "parent godown")?.map(GodownId) };
    book.save_godown(now(), &os_user(), godown).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(godown_dto(book, book.godown(id).expect("just saved")))
}

pub fn delete_godown(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    book.delete_godown(now(), &os_user(), GodownId(parse_uuid(id, "godown")?)).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

// ------------------------------------------------------------ units

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UnitDto {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u32,
}

fn unit_dto(u: &Unit) -> UnitDto {
    UnitDto { id: u.id.0.to_string(), symbol: u.symbol.clone(), name: u.name.clone(), decimals: u.decimals }
}

pub fn list_units(slot: &Slot) -> ApiResult<Vec<UnitDto>> {
    let mut out: Vec<UnitDto> = open_ref(slot)?.units().map(unit_dto).collect();
    out.sort_by_key(|u| u.symbol.to_lowercase());
    Ok(out)
}

/// Create (`id` empty) or alter a unit.
pub fn save_unit(slot: &mut Slot, id: &str, symbol: &str, name: &str, decimals: u32) -> ApiResult<UnitDto> {
    let (store, book) = open_mut(slot)?;
    if !id.trim().is_empty() {
        crate::api::require(book, UserRole::Accountant)?;
    }
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return Err("unit symbol is required".into());
    }
    if decimals > 4 {
        return Err("a unit can have at most 4 decimal places".into());
    }
    let id = if id.trim().is_empty() { UnitId::new() } else { UnitId(parse_uuid(id, "unit")?) };
    let unit = Unit { id, symbol: symbol.to_owned(), name: name.trim().to_owned(), decimals };
    book.save_unit(now(), &os_user(), unit).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(unit_dto(book.unit(id).expect("just saved")))
}

pub fn delete_unit(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    book.delete_unit(now(), &os_user(), UnitId(parse_uuid(id, "unit")?)).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

// ------------------------------------------------------------ stock groups

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockGroupDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    /// Full path of the parent ("Raw Material / Steel"); empty: Primary.
    pub parent_name: String,
}

fn stock_group_dto(book: &khata_core::Book, g: &StockGroup) -> StockGroupDto {
    StockGroupDto {
        id: g.id.0.to_string(),
        name: g.name.clone(),
        parent_id: g.parent.map(|p| p.0.to_string()),
        parent_name: stock_group_path(book, g.parent),
    }
}

pub fn list_stock_groups(slot: &Slot, query: &str) -> ApiResult<Vec<StockGroupDto>> {
    let book = open_ref(slot)?;
    let q = query.trim().to_lowercase();
    let mut out: Vec<StockGroupDto> = book
        .stock_groups()
        .filter(|g| g.name.to_lowercase().contains(&q))
        .map(|g| stock_group_dto(book, g))
        .collect();
    out.sort_by_key(|g| g.name.to_lowercase());
    Ok(out)
}

fn optional_id(s: &str, what: &str) -> ApiResult<Option<uuid::Uuid>> {
    if s.trim().is_empty() { Ok(None) } else { parse_uuid(s, what).map(Some) }
}

/// Create (`id` empty) or alter a stock group; `parent_id` empty: Primary.
pub fn save_stock_group(slot: &mut Slot, id: &str, name: &str, parent_id: &str) -> ApiResult<StockGroupDto> {
    let (store, book) = open_mut(slot)?;
    if !id.trim().is_empty() {
        crate::api::require(book, UserRole::Accountant)?;
    }
    let id = optional_id(id, "stock group")?.map_or_else(StockGroupId::new, StockGroupId);
    let group = StockGroup {
        id,
        name: name.trim().to_owned(),
        parent: optional_id(parent_id, "parent stock group")?.map(StockGroupId),
    };
    book.save_stock_group(now(), &os_user(), group).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(stock_group_dto(book, book.stock_group(id).expect("just saved")))
}

pub fn delete_stock_group(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    book.delete_stock_group(now(), &os_user(), StockGroupId(parse_uuid(id, "stock group")?)).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

// ------------------------------------------------------------ stock items

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockItemDto {
    pub id: String,
    pub name: String,
    pub group_id: Option<String>,
    /// Stock group path; empty: Primary.
    pub group_name: String,
    pub unit_id: String,
    pub unit: String,
    /// Decimal places quantities may have.
    pub decimals: u32,
    pub opening_qty: String,
    pub opening_value: String,
    /// Opening value ÷ opening quantity.
    pub opening_rate: String,
    /// On hand now (all vouchers).
    pub closing_qty: String,
    pub closing_value: String,
    pub tax_class: Option<String>,
    pub costing: Costing,
}

fn qty(q: Quantity) -> String {
    q.normalize().to_string()
}

fn item_dto(book: &khata_core::Book, i: &StockItem, now: &BTreeMap<StockItemId, inventory::StockMovement>) -> StockItemDto {
    let unit = book.unit(i.unit);
    let m = now.get(&i.id).cloned().unwrap_or_default();
    StockItemDto {
        id: i.id.0.to_string(),
        name: i.name.clone(),
        group_id: i.group.map(|g| g.0.to_string()),
        group_name: stock_group_path(book, i.group),
        unit_id: i.unit.0.to_string(),
        unit: unit.map(|u| u.symbol.clone()).unwrap_or_default(),
        decimals: unit.map_or(0, |u| u.decimals),
        opening_qty: qty(i.opening_qty),
        opening_value: amt(i.opening_value),
        opening_rate: amt(if i.opening_qty.is_zero() { Amount::ZERO } else { (i.opening_value / i.opening_qty).round_dp(2) }),
        closing_qty: qty(m.closing_qty),
        closing_value: amt(m.closing_value),
        tax_class: i.tax_class.clone(),
        costing: i.costing,
    }
}

fn all_movements(book: &khata_core::Book) -> BTreeMap<StockItemId, inventory::StockMovement> {
    inventory::movements(book, chrono::NaiveDate::MIN, chrono::NaiveDate::MAX)
}

pub fn list_stock_items(slot: &Slot, query: &str) -> ApiResult<Vec<StockItemDto>> {
    let book = open_ref(slot)?;
    let q = query.trim().to_lowercase();
    let mv = all_movements(book);
    let mut out: Vec<StockItemDto> = book
        .stock_items()
        .filter(|i| i.name.to_lowercase().contains(&q))
        .map(|i| item_dto(book, i, &mv))
        .collect();
    out.sort_by_key(|i| i.name.to_lowercase());
    Ok(out)
}

pub fn get_stock_item(slot: &Slot, id: &str) -> ApiResult<StockItemDto> {
    let book = open_ref(slot)?;
    let i = book.stock_item(StockItemId(parse_uuid(id, "stock item")?)).ok_or("unknown stock item")?;
    Ok(item_dto(book, i, &all_movements(book)))
}

#[derive(Debug, Clone, Deserialize)]
pub struct StockItemInput {
    /// Empty: create.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Empty: Primary.
    #[serde(default)]
    pub group_id: String,
    pub unit_id: String,
    #[serde(default)]
    pub opening_qty: String,
    /// Per unit; the value is quantity × rate unless `opening_value` is given.
    #[serde(default)]
    pub opening_rate: String,
    #[serde(default)]
    pub opening_value: String,
    /// HSN/SAC code or tax category; empty: none.
    #[serde(default)]
    pub tax_class: String,
    /// Valuation method; absent keeps the item's current one (average cost
    /// for a new item).
    #[serde(default)]
    pub costing: Option<Costing>,
}

fn decimal_or_zero(s: &str) -> ApiResult<Amount> {
    if s.trim().is_empty() { Ok(Amount::ZERO) } else { parse_amount(s) }
}

pub fn save_stock_item(slot: &mut Slot, input: &StockItemInput) -> ApiResult<StockItemDto> {
    let (store, book) = open_mut(slot)?;
    if !input.id.trim().is_empty() {
        crate::api::require(book, UserRole::Accountant)?;
    }
    let id = optional_id(&input.id, "stock item")?.map_or_else(StockItemId::new, StockItemId);
    let opening_qty: Quantity = decimal_or_zero(&input.opening_qty)?;
    let opening_value = if input.opening_value.trim().is_empty() {
        (opening_qty * decimal_or_zero(&input.opening_rate)?).round_dp(2)
    } else {
        parse_amount(&input.opening_value)?
    };
    let item = StockItem {
        id,
        name: input.name.trim().to_owned(),
        group: optional_id(&input.group_id, "stock group")?.map(StockGroupId),
        unit: UnitId(parse_uuid(&input.unit_id, "unit")?),
        opening_qty,
        opening_value,
        tax_class: Some(khata_core::book::normalise_tax_class(&input.tax_class)).filter(|c| !c.is_empty()),
        costing: input.costing.or_else(|| book.stock_item(id).map(|i| i.costing)).unwrap_or_default(),
    };
    book.save_stock_item(now(), &os_user(), item).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(item_dto(book, book.stock_item(id).expect("just saved"), &all_movements(book)))
}

pub fn delete_stock_item(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    book.delete_stock_item(now(), &os_user(), StockItemId(parse_uuid(id, "stock item")?)).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use khata_core::DrCr;

    use super::*;
    use crate::api::tests::{TempFile, demo, entry, group_id, voucher};
    use crate::api::{StockLineInput, get_voucher, open_company, post_voucher, profit_and_loss};
    use crate::insights;

    #[test]
    fn masters_alter_and_survive_a_reopen() {
        let f = TempFile::new("masters-alter");
        let (mut slot, bank, _, rent, _, party) = demo(&f);

        // Rename and move a ledger, keep its tax ID rules.
        let rent_view = get_ledger(&slot, &rent).unwrap();
        let input = LedgerInput {
            name: "Rent – Office".into(),
            group_id: group_id(&slot, "Direct Expenses"),
            opening: String::new(),
            credit_days: Some(30),
            tax_id: None,
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: None,
            tax_class: None,
            pan: None,
            tds: None,
        };
        let altered = alter_ledger(&mut slot, &rent, &input).unwrap();
        assert_eq!((altered.ledger.name.as_str(), altered.ledger.credit_days), ("Rent – Office", None), "credit days are for parties only");
        assert_ne!(altered.ledger.group_id, rent_view.group_id);
        let bad = LedgerInput { tax_id: Some("XYZ".into()), ..input };
        assert!(alter_ledger(&mut slot, &rent, &bad).unwrap_err().contains("party"));

        // A bank ledger with vouchers can't leave the bank groups' rules behind
        // if it has bank dates; a used ledger can't be deleted.
        assert!(delete_ledger(&mut slot, &party).is_err());

        // Rename a default group; the party check still works by id.
        let debtors = group_id(&slot, "Sundry Debtors");
        alter_group(&mut slot, &debtors, "Trade Receivables", "", &["Debtors".into()]).unwrap();
        assert!(get_ledger(&slot, &party).unwrap().is_party);
        let creditors = group_id(&slot, "Sundry Creditors");
        assert!(alter_group(&mut slot, &debtors, "Trade Receivables", &creditors, &[]).is_err(), "different kind");
        let capital = group_id(&slot, "Capital Account");
        assert!(alter_group(&mut slot, &capital, "Capital", &creditors, &[]).is_err(), "primary stays primary");

        let _ = bank;
        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(get_ledger(&reopened, &rent).unwrap().name, "Rent – Office");
        assert_eq!(get_group(&reopened, &debtors).unwrap().aliases, ["Debtors"]);
    }

    #[test]
    fn item_invoice_moves_stock_and_reaches_the_reports() {
        let f = TempFile::new("masters-stock");
        let (mut slot, bank, _, _, sales, party) = demo(&f);
        let nos = save_unit(&mut slot, "", "Nos", "Numbers", 0).unwrap();
        let hw = save_stock_group(&mut slot, "", "Hardware", "").unwrap();
        let item = save_stock_item(&mut slot, &StockItemInput {
            id: String::new(),
            name: "Drill".into(),
            group_id: hw.id.clone(),
            unit_id: nos.id.clone(),
            opening_qty: "4".into(),
            opening_rate: "2500".into(),
            opening_value: String::new(),
            tax_class: String::new(),
            costing: None,
        })
        .unwrap();
        assert_eq!((item.opening_value.as_str(), item.group_name.as_str()), ("10000", "Hardware"));

        let mut line = entry(&sales, DrCr::Cr, "9000");
        line.items = vec![StockLineInput { item_id: item.id.clone(), qty: "3".into(), rate: "3000".into(), amount: String::new(), godown_id: String::new(), batch: String::new() }];
        let posted = post_voucher(&mut slot, voucher("Sales", "2026-04-10", vec![entry(&party, DrCr::Dr, "9000"), line])).unwrap();
        let view = get_voucher(&slot, &posted.id).unwrap();
        let shown = &view.entries.iter().find(|e| !e.items.is_empty()).unwrap().items[0];
        assert_eq!((shown.item_name.as_str(), shown.unit.as_str(), shown.qty.as_str()), ("Drill", "Nos", "3"));

        let summary = insights::stock_summary(&slot, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(summary.closing_value, Amount::from(2500));
        assert_eq!(get_stock_item(&slot, &item.id).unwrap().closing_qty, "1");
        let pl = profit_and_loss(&slot, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!((pl.opening_stock, pl.closing_stock), (Amount::from(10000), Amount::from(2500)));
        assert!(delete_stock_item(&mut slot, &item.id).is_err(), "in use");
        assert!(delete_unit(&mut slot, &nos.id).is_err(), "in use");

        // Bank reconciliation through the command layer.
        let brs = insights::bank_reconciliation(&slot, &bank, "2026-04-01", "2026-04-30").unwrap();
        let rent_payment = &brs.statement.lines[0];
        assert_eq!(brs.statement.balance_per_bank, Amount::from(250000));
        insights::set_bank_date(&mut slot, &rent_payment.voucher.0.to_string(), &bank, "2026-04-03").unwrap();
        let brs = insights::bank_reconciliation(&slot, &bank, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(brs.statement.balance_per_bank, brs.statement.balance_per_books);
        assert!(insights::bank_reconciliation(&slot, &party, "2026-04-01", "2026-04-30").is_err());

        // Altering the voucher keeps its bank date.
        let paid = get_voucher(&slot, &rent_payment.voucher.0.to_string()).unwrap();
        assert_eq!(paid.entries.iter().find(|e| e.ledger_id == bank).unwrap().bank_date.map(|d| d.to_string()).as_deref(), Some("2026-04-03"));
        let r = insights::ratio_analysis(&slot, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(r.sales, Amount::from(49000));
        let d = insights::dashboard(&slot, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(d.months.len(), 1);
    }

    #[test]
    fn stock_journal_through_the_command_layer() {
        use crate::api::{StockMoveInput, VoucherInput};
        let f = TempFile::new("masters-journal");
        let (mut slot, ..) = demo(&f);
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap();
        let item = |slot: &mut Slot, name: &str, qty: &str| {
            save_stock_item(slot, &StockItemInput {
                id: String::new(),
                name: name.into(),
                group_id: String::new(),
                unit_id: nos.id.clone(),
                opening_qty: qty.into(),
                opening_rate: "10".into(),
                opening_value: String::new(),
                tax_class: String::new(),
                costing: None,
            })
            .unwrap()
            .id
        };
        let part = item(&mut slot, "Part", "10");
        let product = item(&mut slot, "Product", "");
        let godown = save_godown(&mut slot, "", "Workshop", "").unwrap();
        let line = |item: &str, qty: &str, rate: &str, godown_id: &str| StockLineInput {
            item_id: item.into(),
            qty: qty.into(),
            rate: rate.into(),
            amount: String::new(),
            godown_id: godown_id.into(),
            batch: String::new(),
        };
        let posted = post_voucher(&mut slot, VoucherInput {
            stock: vec![
                StockMoveInput { inward: false, line: line(&part, "4", "10", "") },
                StockMoveInput { inward: true, line: line(&product, "1", "55", &godown.id) },
            ],
            ..voucher("StockJournal", "2026-04-12", Vec::new())
        })
        .unwrap();
        assert_eq!(posted.number, "1");
        let v = get_voucher(&slot, &posted.id).unwrap();
        assert_eq!(v.stock.len(), 2);
        assert_eq!(v.stock[1].line.godown_name.as_deref(), Some("Workshop"));
        assert_eq!(get_stock_item(&slot, &part).unwrap().closing_qty, "6");
        assert_eq!(get_stock_item(&slot, &product).unwrap().closing_value, "55");
        assert!(delete_godown(&mut slot, &godown.id).is_err(), "in use");
    }

    #[test]
    fn trial_balance_balances_with_opening_stock() {
        let f = TempFile::new("masters-tb-stock");
        let (mut slot, ..) = demo(&f);
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap();
        save_stock_item(&mut slot, &StockItemInput {
            id: String::new(),
            name: "Fan".into(),
            group_id: String::new(),
            unit_id: nos.id,
            opening_qty: "10".into(),
            opening_rate: "2000".into(),
            opening_value: String::new(),
            tax_class: String::new(),
            costing: None,
        })
        .unwrap();
        let tb = crate::api::trial_balance(&slot, None).unwrap();
        assert_eq!(tb.opening_stock, "20000");
        // The opening stock is the only opening difference, and the TB now
        // carries it as a debit, so both sides agree once it is entered.
        assert_eq!(tb.opening_difference, "20000");
        let html = crate::export::report_html(&slot, &crate::export::ReportSpec::TrialBalance { as_at: "2026-04-30".into() }).unwrap();
        assert!(html.contains("Opening Stock"));
    }
}

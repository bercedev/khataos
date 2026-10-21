//! e-Invoice (INV-01) and e-way bill JSON from saved vouchers, for upload
//! through the government portals' offline or bulk tools. The JSON shapes
//! and checks live in `khata_in::einvoice` and `khata_in::ewaybill`; this
//! module turns vouchers into their documents. Nothing here talks to an IRP
//! or the e-way bill system (see `docs/einvoice.md`).
//!
//! Which vouchers: Sales → invoice (INV), Credit Note → credit note (CRN)
//! for e-invoices; Sales (outward) and Purchase (inward) for e-way bills.
//! KhataOS's Debit Note is a purchase return, so it is not an outward debit
//! note and is not e-invoiced.
//!
//! A voucher's lines split as for GST returns (`gst.rs`): the party line
//! carries the total; stock lines are the items; other ledgers on the
//! opposite side are items without an HSN (so they fail the e-invoice
//! check: ledgers have no HSN/SAC yet) or, on an e-way bill, other value;
//! a ledger named "Round off" is the round-off; other ledgers on the
//! party's side are discounts, spread over the items by amount; tax ledgers
//! give the tax, spread over the items by amount × the rate on file for
//! each item's HSN (or by amount when a rate is missing).
//!
//! Addresses: the company's comes from its profile address (PIN = the last
//! six-digit number); the party's from its ledger mailing details. What is
//! missing can be supplied per call ([`BuyerDetails`], [`EwayDetails`]);
//! nothing given there is saved to the books.

use std::collections::{BTreeMap, HashMap};

use khata_compliance::{CompliancePack, RateSchedule};
use khata_core::{Book, DrCr, Ledger, Voucher, VoucherId, VoucherKind};
use khata_in::einvoice::{self, Address, DocErrors, DocType, Document, Item, Party, SupplyType, parse_address};
use khata_in::ewaybill::{self, Direction, EwayBill, Place, SubSupply, TransMode, TransactionType, Transport, VehicleType};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::api::{ApiResult, Slot, err, group_is_party, open_ref, pack, parse_date, parse_uuid};

/// What the e-invoice needs about the buyer that the books may lack. Every
/// field is optional: an empty one falls back to the party ledger (name,
/// GSTIN, mailing details).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BuyerDetails {
    pub legal_name: String,
    /// Replaces the ledger's street lines (with `address2`).
    pub address1: String,
    pub address2: String,
    /// City or town.
    pub location: String,
    pub pin: String,
    /// Two-digit GST state code of the address.
    pub state_code: String,
    /// Two-digit state code; default: the GSTIN's state, else the mailing
    /// state.
    pub place_of_supply: String,
    /// `B2B`, `SEZWP`, `SEZWOP`, `EXPWP`, `EXPWOP` or `DEXP`. Empty: an
    /// export when the party's mailing country is not India (with or
    /// without IGST as booked), else B2B.
    pub supply_type: String,
    pub reverse_charge: bool,
    pub igst_on_intra: bool,
}

/// What an e-way bill needs beyond the voucher: the other party's address
/// when the ledger lacks it, and Part B. Every field is optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EwayDetails {
    /// The other party (consignee on a sale, consignor on a purchase).
    pub name: String,
    pub address1: String,
    pub address2: String,
    pub location: String,
    pub pin: String,
    pub state_code: String,
    /// Where the goods actually leave from / go to, when not the billing
    /// state.
    pub actual_from_state: String,
    pub actual_to_state: String,
    /// The supplier's invoice number and date on a purchase (the voucher's
    /// own number is used when empty).
    pub doc_no: String,
    pub doc_date: String,
    /// Default: tax invoice.
    pub doc_type: Option<ewaybill::DocType>,
    /// Default: supply.
    pub sub_supply: Option<SubSupply>,
    pub transaction_type: Option<TransactionType>,
    pub distance_km: Option<u32>,
    pub trans_mode: Option<TransMode>,
    pub transporter_id: String,
    pub transporter_name: String,
    pub trans_doc_no: String,
    pub trans_doc_date: String,
    pub vehicle_no: String,
    pub over_dimensional: bool,
}

fn label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Sales",
        VoucherKind::Purchase => "Purchase",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        _ => "Voucher",
    }
}

/// The party line, as in `gst.rs`: a debtor/creditor, else the cash or bank
/// line on the party's side, else the first line on that side.
fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = match v.kind {
        VoucherKind::Sales | VoucherKind::DebitNote => DrCr::Dr,
        _ => DrCr::Cr,
    };
    let is_party = |e: &khata_core::Entry| book.ledger(e.ledger).is_some_and(|l| group_is_party(book, l.group));
    v.entries
        .iter()
        .position(is_party)
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

/// The GST unit code for a unit symbol, or `None` when it has no mapping
/// (`gst::uqc` falls back to OTH, which an e-invoice should not get by
/// accident).
fn mapped_uqc(symbol: &str) -> Option<String> {
    let code = crate::gst::uqc(symbol);
    (code != "OTH" || symbol.trim().eq_ignore_ascii_case("oth")).then(|| code.to_owned())
}

/// A voucher's items, round-off and total, with tax spread over the items.
struct Lines {
    /// Items, each flagged true when it comes from a stock line.
    items: Vec<(Item, bool)>,
    round_off: Decimal,
    total: Decimal,
    problems: Vec<String>,
}

fn rates(book: &Book) -> ApiResult<RateSchedule> {
    let mut rates = RateSchedule::default();
    if let Some(p) = book.profile() {
        for (class, changes) in &p.tax_rates {
            for (from, r) in changes {
                rates.set(class, *from, *r).map_err(err)?;
            }
        }
    }
    Ok(rates)
}

fn lines(book: &Book, v: &Voucher, party: Option<usize>, rates: &RateSchedule) -> Lines {
    let party_side = party.map(|i| v.entries[i].side);
    let mut out = Lines {
        items: Vec::new(),
        round_off: Decimal::ZERO,
        total: party.map_or_else(|| v.totals().0, |i| v.entries[i].amount),
        problems: Vec::new(),
    };
    let mut known: Vec<Option<Decimal>> = Vec::new();
    let mut tax: BTreeMap<&'static str, Decimal> = BTreeMap::new();
    let mut discount = Decimal::ZERO;
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.as_str()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            let slot = match t.component.to_ascii_uppercase().as_str() {
                "IGST" => "igst",
                "CGST" => "cgst",
                "SGST" | "UTGST" => "sgst",
                c if c.contains("CESS") => "cess",
                c => {
                    out.problems.push(format!("{ledger_name} collects {c}, which is not GST"));
                    continue;
                }
            };
            *tax.entry(slot).or_default() += signed;
        } else if ledger_name.to_lowercase().contains("round") {
            out.round_off += signed;
        } else if on_party_side {
            // A discount (or other deduction) on the party's side.
            discount += e.amount;
        } else if e.items.is_empty() {
            // A service or sales ledger line: its own HSN/SAC, when set.
            let hsn = crate::gst::class_flags(ledger.and_then(|l| l.tax_class.as_deref())).0;
            known.push(hsn.as_deref().and_then(|c| rates.rate(c, v.date)));
            out.items.push((
                Item { description: ledger_name.to_owned(), hsn, gross: e.amount, unit_price: None, ..Item::default() },
                false,
            ));
        } else {
            for l in &e.items {
                let item = book.stock_item(l.item);
                let hsn = crate::gst::class_flags(item.and_then(|i| i.tax_class.as_deref())).0;
                known.push(hsn.as_deref().and_then(|c| rates.rate(c, v.date)));
                out.items.push((
                    Item {
                        description: item.map(|i| i.name.clone()).unwrap_or_default(),
                        hsn,
                        qty: Some(l.qty),
                        unit: item.and_then(|i| book.unit(i.unit)).and_then(|u| mapped_uqc(&u.symbol)),
                        unit_price: Some(l.rate),
                        gross: l.amount,
                        ..Item::default()
                    },
                    true,
                ));
            }
        }
    }
    let items = &mut out.items;
    if items.is_empty() {
        return out;
    }
    // Discounts by amount; the last item takes the rounding.
    let gross: Decimal = items.iter().map(|(i, _)| i.gross).sum();
    if !discount.is_zero() && !gross.is_zero() {
        let mut left = discount;
        let n = items.len();
        for (k, (it, _)) in items.iter_mut().enumerate() {
            let share = if k + 1 == n { left } else { (discount * it.gross / gross).round_dp(2) };
            it.discount = share;
            left -= share;
        }
    }
    // Tax weights: assessable value × rate when every item has a rate.
    let rated = known.iter().all(Option::is_some);
    let weights: Vec<Decimal> = items
        .iter()
        .zip(&known)
        .map(|((it, _), r)| if rated { it.assessable() * r.unwrap_or_default() } else { it.assessable() })
        .collect();
    let weight_sum: Decimal = weights.iter().sum();
    let last = weights.iter().rposition(|w| !w.is_zero());
    for (slot, total) in tax {
        let mut left = total;
        for (k, (it, _)) in items.iter_mut().enumerate() {
            let share = match last {
                Some(l) if k == l => left,
                Some(_) if !weight_sum.is_zero() => (total * weights[k] / weight_sum).round_dp(2),
                _ => Decimal::ZERO,
            };
            left -= share;
            match slot {
                "igst" => it.igst = share,
                "cgst" => it.cgst = share,
                "sgst" => it.sgst = share,
                _ => it.cess = share,
            }
        }
        if last.is_none() && !total.is_zero() {
            out.problems.push("the items add up to zero, so the tax cannot be spread over them".into());
        }
    }
    // Rates: from the rate list, else worked out from the amounts.
    for ((it, _), r) in items.iter_mut().zip(&known) {
        let base = it.assessable();
        if base.is_zero() {
            continue;
        }
        it.cess_rate = (it.cess * Decimal::ONE_HUNDRED / base).round_dp(2);
        it.gst_rate = match r {
            Some(r) => *r,
            None => {
                let (rate, standard) = khata_in::returns::snap_rate(it.tax() * Decimal::ONE_HUNDRED / base);
                if !standard {
                    out.problems.push(format!(
                        "{}: no rate on file for its HSN/SAC, and the tax booked gives {}%, which is not a GST rate",
                        it.description,
                        rate.normalize()
                    ));
                }
                rate
            }
        };
    }
    out
}

struct Company {
    gstin: String,
    party: Party,
}

fn company(book: &Book) -> ApiResult<(Company, &'static dyn CompliancePack)> {
    if book.country() != Some("IN") {
        return Err("e-invoices and e-way bills are for companies in India".into());
    }
    let pack = pack(book).ok_or("pick the company's country first")?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let gstin = profile.tax_id.clone().ok_or("set the company's GSTIN first (Company → Tax registration)")?;
    Ok((
        Company {
            party: Party {
                gstin: Some(gstin.clone()),
                legal_name: book.name.clone(),
                address: parse_address(&profile.address),
                state_code: profile.tax_region.clone(),
            },
            gstin,
        },
        pack,
    ))
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// The party's address from its mailing details, with per-call overrides.
fn party_address(ledger: Option<&Ledger>, address1: &str, address2: &str, location: &str, pin: &str) -> Address {
    let mailing = ledger.and_then(|l| l.mailing.as_ref());
    let mut a = mailing.map(|m| parse_address(&m.address)).unwrap_or_default();
    if let Some(p) = mailing.and_then(|m| non_empty(&m.postal_code)) {
        a.pin = Some(p);
    }
    if let Some(l1) = non_empty(address1) {
        a.line1 = l1;
        a.line2 = address2.trim().to_owned();
    }
    if let Some(l) = non_empty(location) {
        a.location = l;
    }
    if let Some(p) = non_empty(pin) {
        a.pin = Some(p);
    }
    a
}

/// The party's state: the override, else its mailing state, else its
/// GSTIN's state.
fn party_state(pack: &dyn CompliancePack, ledger: Option<&Ledger>, given: &str) -> Option<String> {
    non_empty(given)
        .or_else(|| ledger.and_then(|l| l.mailing.as_ref()).and_then(|m| m.region.as_deref()).and_then(non_empty))
        .or_else(|| ledger.and_then(|l| crate::tax::party_region(pack, l)))
}

fn party_gstin(ledger: Option<&Ledger>) -> Option<String> {
    ledger.and_then(|l| l.tax_id.as_deref()).and_then(non_empty)
}

fn abroad(ledger: Option<&Ledger>) -> bool {
    ledger
        .and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .is_some_and(|c| !c.trim().is_empty() && !c.trim().eq_ignore_ascii_case("IN"))
}

/// The e-invoice document for a voucher, and problems found while reading
/// the books (on top of what `einvoice::validate` checks).
fn document(book: &Book, v: &Voucher, buyer: &BuyerDetails) -> ApiResult<(Document, Vec<String>)> {
    let (company, pack) = company(book)?;
    let doc_type = match v.kind {
        VoucherKind::Sales => DocType::Invoice,
        VoucherKind::CreditNote => DocType::CreditNote,
        _ => return Err("e-invoices are made from Sales and Credit Note vouchers".into()),
    };
    if v.cancelled {
        return Err(format!("{} {} is cancelled", label(v.kind), v.number));
    }
    let party = party_line(book, v);
    let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
    let lines = lines(book, v, party, &rates(book)?);
    let taxed = lines.items.iter().any(|(i, _)| !i.tax().is_zero());
    let sez = ledger.and_then(|l| l.mailing.as_ref()).is_some_and(|m| m.special_zone);
    let supply_type = match non_empty(&buyer.supply_type) {
        Some(code) => SupplyType::parse(&code).ok_or_else(|| format!("{code:?} is not an e-invoice supply type"))?,
        None if sez && taxed => SupplyType::SezWithPayment,
        None if sez => SupplyType::SezWithoutPayment,
        None if abroad(ledger) && taxed => SupplyType::ExportWithPayment,
        None if abroad(ledger) => SupplyType::ExportWithoutPayment,
        None => SupplyType::B2b,
    };
    let state = party_state(pack, ledger, &buyer.state_code);
    let doc = Document {
        label: format!("{} {}", label(v.kind), v.number),
        supply_type,
        doc_type,
        number: v.number.clone(),
        date: v.date,
        reverse_charge: buyer.reverse_charge,
        igst_on_intra: buyer.igst_on_intra,
        seller: company.party,
        buyer: Party {
            gstin: party_gstin(ledger).filter(|_| !supply_type.is_export()),
            legal_name: non_empty(&buyer.legal_name).or_else(|| ledger.map(|l| l.name.clone())).unwrap_or_default(),
            address: party_address(ledger, &buyer.address1, &buyer.address2, &buyer.location, &buyer.pin),
            state_code: state.clone(),
        },
        place_of_supply: non_empty(&buyer.place_of_supply)
            .or_else(|| ledger.and_then(|l| crate::tax::party_region(pack, l)))
            .or(state),
        items: lines.items.into_iter().map(|(i, _)| i).collect(),
        round_off: lines.round_off,
        total: lines.total,
    };
    Ok((doc, lines.problems))
}

fn build(doc: &Document, extra: Vec<String>, hsn_digits: usize) -> Result<serde_json::Value, DocErrors> {
    if extra.is_empty() {
        return einvoice::to_json(doc, hsn_digits);
    }
    let mut problems = extra;
    problems.extend(einvoice::validate(doc, hsn_digits));
    Err(DocErrors { document: doc.label.clone(), problems })
}

fn voucher<'a>(book: &'a Book, id: &str) -> ApiResult<&'a Voucher> {
    book.voucher(VoucherId(parse_uuid(id, "voucher")?)).ok_or_else(|| "unknown voucher".into())
}

fn check_digits(hsn_digits: usize) -> ApiResult<()> {
    if matches!(hsn_digits, 4 | 6 | 8) {
        Ok(())
    } else {
        Err("the minimum HSN length is 4 or 6 digits (by the previous year's turnover)".into())
    }
}

/// The INV-01 JSON for one Sales or Credit Note voucher. `hsn_digits` is the
/// minimum HSN length: 4 up to ₹5 crore AATO, 6 above.
pub fn einvoice_json(slot: &Slot, voucher_id: &str, buyer: &BuyerDetails, hsn_digits: usize) -> ApiResult<String> {
    check_digits(hsn_digits)?;
    let book = open_ref(slot)?;
    let (doc, extra) = document(book, voucher(book, voucher_id)?, buyer)?;
    let json = build(&doc, extra, hsn_digits).map_err(err)?;
    serde_json::to_string_pretty(&json).map_err(err)
}

/// INV-01 JSON for every B2B, SEZ, deemed-export and export Sales or Credit
/// Note voucher in a period, as one array for bulk upload. Vouchers to
/// unregistered buyers in India (B2C) and cancelled ones are skipped.
/// `buyers` gives per-party details by ledger id. Nothing is produced
/// unless every document passes; the error lists each failing one.
pub fn einvoice_bulk_json(
    slot: &Slot,
    from: &str,
    to: &str,
    buyers: &HashMap<String, BuyerDetails>,
    hsn_digits: usize,
) -> ApiResult<String> {
    check_digits(hsn_digits)?;
    let book = open_ref(slot)?;
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    if from > to {
        return Err("the period ends before it starts".into());
    }
    let mut vouchers: Vec<&Voucher> = book
        .vouchers()
        .filter(|v| matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote))
        .filter(|v| !v.cancelled && v.date >= from && v.date <= to)
        .collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    let none = BuyerDetails::default();
    let mut docs = Vec::new();
    let mut errors = Vec::new();
    for v in vouchers {
        let Some(ledger) = party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger)) else { continue };
        let details = buyers.get(&ledger.id.0.to_string()).unwrap_or(&none);
        let (doc, extra) = document(book, v, details)?;
        if doc.supply_type == SupplyType::B2b && party_gstin(Some(ledger)).is_none() {
            continue;
        }
        if extra.is_empty() {
            docs.push(doc);
        } else if let Err(e) = build(&doc, extra, hsn_digits) {
            errors.push(e);
        }
    }
    match einvoice::bulk_json(&docs, hsn_digits) {
        Ok(_) if !errors.is_empty() => Err(errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")),
        Ok(json) if json.as_array().is_some_and(Vec::is_empty) => Err("no B2B, SEZ or export documents in this period".into()),
        Ok(json) => serde_json::to_string_pretty(&json).map_err(err),
        Err(more) => {
            errors.extend(more);
            Err(errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))
        }
    }
}

/// The e-way bill bulk JSON (one bill) for a Sales (outward) or Purchase
/// (inward) voucher. Part B is optional; see [`EwayDetails`].
pub fn ewaybill_json(slot: &Slot, voucher_id: &str, details: &EwayDetails, hsn_digits: usize) -> ApiResult<String> {
    check_digits(hsn_digits)?;
    let book = open_ref(slot)?;
    let v = voucher(book, voucher_id)?;
    let direction = match v.kind {
        VoucherKind::Sales => Direction::Outward,
        VoucherKind::Purchase => Direction::Inward,
        _ => return Err("e-way bills are made here from Sales and Purchase vouchers".into()),
    };
    if v.cancelled {
        return Err(format!("{} {} is cancelled", label(v.kind), v.number));
    }
    let (company, pack) = company(book)?;
    let party = party_line(book, v);
    let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
    let lines = lines(book, v, party, &rates(book)?);
    let other = Place {
        gstin: party_gstin(ledger),
        name: non_empty(&details.name).or_else(|| ledger.map(|l| l.name.clone())).unwrap_or_default(),
        address: party_address(ledger, &details.address1, &details.address2, &details.location, &details.pin),
        state_code: party_state(pack, ledger, &details.state_code),
        actual_state_code: None,
    };
    let own = Place {
        gstin: company.party.gstin,
        name: company.party.legal_name,
        address: company.party.address,
        state_code: company.party.state_code,
        actual_state_code: None,
    };
    let (mut from, mut to) = match direction {
        Direction::Outward => (own, other),
        Direction::Inward => (other, own),
    };
    from.actual_state_code = non_empty(&details.actual_from_state);
    to.actual_state_code = non_empty(&details.actual_to_state);
    // Lines that are not stock (freight, services) are other value.
    let mut other_value = lines.round_off;
    let mut items = Vec::new();
    for (it, stock) in lines.items {
        if stock {
            items.push(it);
        } else {
            other_value += it.total();
        }
    }
    let bill = EwayBill {
        label: format!("{} {}", label(v.kind), v.number),
        user_gstin: company.gstin,
        direction,
        sub_supply: details.sub_supply.unwrap_or(SubSupply::Supply),
        doc_type: details.doc_type.unwrap_or(ewaybill::DocType::Invoice),
        number: non_empty(&details.doc_no).unwrap_or_else(|| v.number.clone()),
        date: match non_empty(&details.doc_date) {
            Some(d) => parse_date(&d)?,
            None => v.date,
        },
        from,
        to,
        transaction_type: details.transaction_type.unwrap_or_default(),
        items,
        other_value,
        total: lines.total,
        transport: Transport {
            mode: details.trans_mode,
            distance_km: details.distance_km,
            transporter_id: details.transporter_id.trim().to_owned(),
            transporter_name: details.transporter_name.trim().to_owned(),
            doc_no: details.trans_doc_no.trim().to_owned(),
            doc_date: non_empty(&details.trans_doc_date).map(|d| parse_date(&d)).transpose()?,
            vehicle_no: details.vehicle_no.clone(),
            vehicle_type: if details.over_dimensional { VehicleType::OverDimensional } else { VehicleType::Regular },
        },
    };
    let json = match ewaybill::to_json(std::slice::from_ref(&bill), hsn_digits) {
        Ok(j) if lines.problems.is_empty() => j,
        Ok(_) => return Err(DocErrors { document: bill.label, problems: lines.problems }.to_string()),
        Err(mut e) => {
            e[0].problems.splice(0..0, lines.problems);
            return Err(e[0].to_string());
        }
    };
    serde_json::to_string_pretty(&json).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher as voucher_input};
    use crate::api::{StockLineInput, cancel_voucher, create_party_ledger, post_voucher};
    use crate::masters::{LedgerInput, StockItemInput, alter_ledger, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::{Mailing, TaxSide};
    use serde_json::Value;

    struct Setup {
        slot: Slot,
        _file: crate::api::tests::TempFile,
        buyer: String,
        local: String,
        walk_in: String,
        foreign: String,
        supplier: String,
        sales: String,
        purchase: String,
        tax: BTreeMap<&'static str, String>,
        laptop: String,
    }

    fn items(item_id: &str, qty: &str, rate: &str) -> Vec<StockLineInput> {
        vec![StockLineInput {
            item_id: item_id.into(),
            qty: qty.into(),
            rate: rate.into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }]
    }

    fn set_mailing(slot: &mut Slot, id: &str, group: &str, tax_id: Option<&str>, m: Mailing) {
        let l = crate::masters::get_ledger(slot, id).unwrap();
        alter_ledger(slot, id, &LedgerInput {
            name: l.name,
            group_id: group_id(slot, group),
            opening: String::new(),
            credit_days: None,
            tax_id: tax_id.map(str::to_owned),
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: Some(m),
            tax_class: None,
            pan: None,
            tds: None,
        })
        .unwrap();
    }

    /// A valid Maharashtra GSTIN for a second party.
    fn mumbai_gstin() -> String {
        format!("27AAGCB7383J1Z{}", khata_in::gstin::check_character("27AAGCB7383J1Z").unwrap())
    }

    fn setup(name: &str) -> Setup {
        let (file, mut slot) = indian_company(name);
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_company_address(&mut slot, "12 MG Road\nCamp\nPune 411001").unwrap();
        set_tax_rate(&mut slot, "847130", "2026-04-01", "18").unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let party = |slot: &mut Slot, name: &str, group: &str, gstin: Option<&str>| {
            create_party_ledger(slot, name, group, "", None, gstin, None).unwrap().ledger.id
        };
        let buyer = party(&mut slot, "Bengaluru Co", &debtors, Some("29AAGCB7383J1Z4"));
        set_mailing(&mut slot, &buyer, "Sundry Debtors", Some("29AAGCB7383J1Z4"), Mailing {
            address: "4 Residency Road\nShanti Nagar\nBengaluru".into(),
            region: Some("29".into()),
            postal_code: "560025".into(),
            ..Mailing::default()
        });
        // Registered in Maharashtra, but no mailing details in the books.
        let local = party(&mut slot, "Mumbai Co", &debtors, Some(mumbai_gstin().as_str()));
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        let foreign = party(&mut slot, "Acme GmbH", &debtors, None);
        set_mailing(&mut slot, &foreign, "Sundry Debtors", None, Mailing {
            address: "Hauptstrasse 5\nBerlin".into(),
            country: Some("DE".into()),
            ..Mailing::default()
        });
        let supplier = party(&mut slot, "KA Supplier", &creditors, Some("29AAGCB7383J1Z4"));
        set_mailing(&mut slot, &supplier, "Sundry Creditors", Some("29AAGCB7383J1Z4"), Mailing {
            address: "Plot 7, Peenya, Bengaluru 560058".into(),
            region: Some("29".into()),
            ..Mailing::default()
        });
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let mut tax = BTreeMap::new();
        for (name, component, side) in [
            ("Output CGST", "CGST", TaxSide::Output),
            ("Output SGST", "SGST", TaxSide::Output),
            ("Output IGST", "IGST", TaxSide::Output),
            ("Input IGST", "IGST", TaxSide::Input),
        ] {
            let id = ledger(&mut slot, name, "Duties & Taxes", "");
            set_ledger_tax(&mut slot, &id, component, side).unwrap();
            tax.insert(name, id);
        }
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap();
        let laptop = save_stock_item(&mut slot, &StockItemInput {
            id: String::new(),
            name: "Laptop".into(),
            group_id: String::new(),
            unit_id: nos.id,
            opening_qty: "10".into(),
            opening_rate: "30000".into(),
            opening_value: String::new(),
            tax_class: "847130".into(),
            costing: None,
        })
        .unwrap()
        .id;
        Setup { slot, _file: file, buyer, local, walk_in, foreign, supplier, sales, purchase, tax, laptop }
    }

    /// Post a Sales or Credit Note of `qty` laptops at `rate` (net value
    /// `net`) with the given tax lines.
    fn sale(s: &mut Setup, kind: &str, date: &str, party: &str, (net, qty, rate): (&str, &str, &str), taxes: &[(&str, &str)]) -> String {
        let (party_side, other) = if kind == "Sales" { (DrCr::Dr, DrCr::Cr) } else { (DrCr::Cr, DrCr::Dr) };
        let mut line = entry(&s.sales, other, net);
        line.items = items(&s.laptop, qty, rate);
        let mut entries = vec![line];
        let mut total: Decimal = net.parse().unwrap();
        for (ledger, amount) in taxes {
            entries.push(entry(&s.tax[ledger], other, amount));
            total += amount.parse::<Decimal>().unwrap();
        }
        entries.insert(0, entry(party, party_side, &total.to_string()));
        post_voucher(&mut s.slot, voucher_input(kind, date, entries)).unwrap().id
    }

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn b2b_invoice_and_credit_note_from_the_books() {
        let mut s = setup("einvoice-b2b");
        let buyer = s.buyer.clone();
        let id = sale(&mut s, "Sales", "2026-05-02", &buyer, ("100000", "2", "50000"), &[("Output IGST", "18000")]);
        let j = parse(&einvoice_json(&s.slot, &id, &BuyerDetails::default(), 6).unwrap());
        assert_eq!(j["TranDtls"]["SupTyp"], "B2B");
        assert_eq!(j["DocDtls"]["Typ"], "INV");
        assert_eq!(j["DocDtls"]["Dt"], "02/05/2026");
        let seller = &j["SellerDtls"];
        assert_eq!((seller["Gstin"].as_str(), seller["LglNm"].as_str(), seller["Loc"].as_str()), (Some("27AAPFU0939F1ZV"), Some("Demo"), Some("Pune")));
        assert_eq!((seller["Pin"].as_u64(), seller["Stcd"].as_str()), (Some(411001), Some("27")));
        let b = &j["BuyerDtls"];
        assert_eq!((b["Gstin"].as_str(), b["Pos"].as_str(), b["Stcd"].as_str()), (Some("29AAGCB7383J1Z4"), Some("29"), Some("29")));
        assert_eq!((b["Addr1"].as_str(), b["Addr2"].as_str(), b["Loc"].as_str(), b["Pin"].as_u64()), (Some("4 Residency Road"), Some("Shanti Nagar"), Some("Bengaluru"), Some(560025)));
        let it = &j["ItemList"][0];
        assert_eq!((it["PrdDesc"].as_str(), it["HsnCd"].as_str(), it["Unit"].as_str()), (Some("Laptop"), Some("847130"), Some("NOS")));
        assert_eq!((it["Qty"].as_f64(), it["UnitPrice"].as_f64(), it["GstRt"].as_f64(), it["IgstAmt"].as_f64()), (Some(2.0), Some(50000.0), Some(18.0), Some(18000.0)));
        assert_eq!(j["ValDtls"]["TotInvVal"].as_f64(), Some(118000.0));

        let cn = sale(&mut s, "CreditNote", "2026-05-10", &buyer, ("50000", "1", "50000"), &[("Output IGST", "9000")]);
        let j = parse(&einvoice_json(&s.slot, &cn, &BuyerDetails::default(), 6).unwrap());
        assert_eq!(j["DocDtls"]["Typ"], "CRN");
        assert_eq!((j["ValDtls"]["IgstVal"].as_f64(), j["ValDtls"]["TotInvVal"].as_f64()), (Some(9000.0), Some(59000.0)));
    }

    #[test]
    fn missing_party_details_come_from_the_call_or_are_reported() {
        let mut s = setup("einvoice-missing");
        let local = s.local.clone();
        let id = sale(&mut s, "Sales", "2026-05-03", &local, ("40000", "1", "40000"), &[("Output CGST", "3600"), ("Output SGST", "3600")]);
        let e = einvoice_json(&s.slot, &id, &BuyerDetails::default(), 6).unwrap_err();
        assert!(e.starts_with("Sales 26-27/1: "), "{e}");
        for needle in ["buyer's address", "buyer's city", "buyer's PIN code is missing"] {
            assert!(e.contains(needle), "missing {needle:?} in {e}");
        }
        let given = BuyerDetails {
            address1: "Nariman Point".into(),
            location: "Mumbai".into(),
            pin: "400021".into(),
            ..BuyerDetails::default()
        };
        let j = parse(&einvoice_json(&s.slot, &id, &given, 6).unwrap());
        assert_eq!((j["BuyerDtls"]["Pos"].as_str(), j["BuyerDtls"]["Stcd"].as_str()), (Some("27"), Some("27")), "state from the GSTIN");
        assert_eq!((j["ItemList"][0]["CgstAmt"].as_f64(), j["ItemList"][0]["SgstAmt"].as_f64()), (Some(3600.0), Some(3600.0)));
        assert!(einvoice_json(&s.slot, &id, &given, 5).unwrap_err().contains("4 or 6"));
        // An unmapped unit and a short HSN are caught.
        let hrs = save_unit(&mut s.slot, "", "hrs", "", 0).unwrap();
        let svc = save_stock_item(&mut s.slot, &StockItemInput {
            id: String::new(),
            name: "Repairs kit".into(),
            group_id: String::new(),
            unit_id: hrs.id,
            opening_qty: String::new(),
            opening_rate: String::new(),
            opening_value: String::new(),
            tax_class: "8471".into(),
            costing: None,
        })
        .unwrap();
        let buyer = s.buyer.clone();
        let mut line = entry(&s.sales, DrCr::Cr, "1000");
        line.items = items(&svc.id, "1", "1000");
        let bad = post_voucher(&mut s.slot, voucher_input("Sales", "2026-05-04", vec![
            entry(&buyer, DrCr::Dr, "1180"),
            line,
            entry(&s.tax["Output IGST"], DrCr::Cr, "180"),
        ]))
        .unwrap()
        .id;
        let e = einvoice_json(&s.slot, &bad, &BuyerDetails::default(), 6).unwrap_err();
        assert!(e.contains("8471 has 4 digits") && e.contains("no GST unit code"), "{e}");
    }

    #[test]
    fn exports_and_bulk_skip_b2c() {
        let mut s = setup("einvoice-bulk");
        let (buyer, walk_in, foreign) = (s.buyer.clone(), s.walk_in.clone(), s.foreign.clone());
        sale(&mut s, "Sales", "2026-05-02", &buyer, ("100000", "2", "50000"), &[("Output IGST", "18000")]);
        sale(&mut s, "Sales", "2026-05-03", &walk_in, ("40000", "1", "40000"), &[("Output CGST", "3600"), ("Output SGST", "3600")]);
        let export = sale(&mut s, "Sales", "2026-05-04", &foreign, ("60000", "1", "60000"), &[]);
        let cancelled = sale(&mut s, "Sales", "2026-05-05", &buyer, ("50000", "1", "50000"), &[("Output IGST", "9000")]);
        cancel_voucher(&mut s.slot, &cancelled).unwrap();
        sale(&mut s, "Sales", "2026-06-01", &buyer, ("50000", "1", "50000"), &[("Output IGST", "9000")]);

        let j = parse(&einvoice_json(&s.slot, &export, &BuyerDetails::default(), 6).unwrap());
        assert_eq!(j["TranDtls"]["SupTyp"], "EXPWOP");
        assert_eq!((j["BuyerDtls"]["Gstin"].as_str(), j["BuyerDtls"]["Pos"].as_str()), (Some("URP"), Some("96")));
        assert_eq!(j["BuyerDtls"]["Loc"], "Berlin");

        let all = parse(&einvoice_bulk_json(&s.slot, "2026-05-01", "2026-05-31", &HashMap::new(), 6).unwrap());
        let docs: Vec<_> = all.as_array().unwrap().iter().map(|d| (d["DocDtls"]["No"].as_str().unwrap().to_owned(), d["TranDtls"]["SupTyp"].as_str().unwrap().to_owned())).collect();
        assert_eq!(docs, [("26-27/1".to_owned(), "B2B".to_owned()), ("26-27/3".to_owned(), "EXPWOP".to_owned())], "no B2C, no cancelled");

        // Per-party details apply in bulk; a bad one fails the whole file.
        let mut buyers = HashMap::new();
        buyers.insert(buyer.clone(), BuyerDetails { pin: "12345".into(), ..BuyerDetails::default() });
        let e = einvoice_bulk_json(&s.slot, "2026-05-01", "2026-05-31", &buyers, 6).unwrap_err();
        assert!(e.contains("Sales 26-27/1") && e.contains("\"12345\" is not six digits"), "{e}");
        assert!(einvoice_bulk_json(&s.slot, "2026-07-01", "2026-07-31", &HashMap::new(), 6).unwrap_err().contains("no B2B"));
    }

    #[test]
    fn eway_bills_for_sales_and_purchases() {
        let mut s = setup("ewaybill");
        let buyer = s.buyer.clone();
        let id = sale(&mut s, "Sales", "2026-05-02", &buyer, ("100000", "2", "50000"), &[("Output IGST", "18000")]);
        let e = ewaybill_json(&s.slot, &id, &EwayDetails::default(), 6).unwrap_err();
        assert!(e.contains("distance"), "{e}");
        let details = EwayDetails {
            distance_km: Some(840),
            trans_mode: Some(TransMode::Road),
            vehicle_no: "MH 12 AB 1234".into(),
            ..EwayDetails::default()
        };
        let j = parse(&ewaybill_json(&s.slot, &id, &details, 6).unwrap());
        let b = &j["billLists"][0];
        assert_eq!((b["supplyType"].as_str(), b["docNo"].as_str(), b["userGstin"].as_str()), (Some("O"), Some("26-27/1"), Some("27AAPFU0939F1ZV")));
        assert_eq!((b["fromPincode"].as_u64(), b["toPincode"].as_u64(), b["toStateCode"].as_u64()), (Some(411001), Some(560025), Some(29)));
        assert_eq!((b["igstValue"].as_f64(), b["totInvValue"].as_f64(), b["vehicleNo"].as_str()), (Some(18000.0), Some(118000.0), Some("MH12AB1234")));
        assert_eq!(b["itemList"][0]["igstRate"].as_f64(), Some(18.0));

        // A purchase: inward, from the supplier, with the supplier's number.
        let supplier = s.supplier.clone();
        let mut line = entry(&s.purchase, DrCr::Dr, "30000");
        line.items = items(&s.laptop, "1", "30000");
        let p = post_voucher(&mut s.slot, voucher_input("Purchase", "2026-05-05", vec![
            entry(&supplier, DrCr::Cr, "35400"),
            line,
            entry(&s.tax["Input IGST"], DrCr::Dr, "5400"),
        ]))
        .unwrap()
        .id;
        let details = EwayDetails { distance_km: Some(840), doc_no: "KA/778".into(), doc_date: "2026-05-04".into(), ..EwayDetails::default() };
        let j = parse(&ewaybill_json(&s.slot, &p, &details, 6).unwrap());
        let b = &j["billLists"][0];
        assert_eq!((b["supplyType"].as_str(), b["docNo"].as_str(), b["docDate"].as_str()), (Some("I"), Some("KA/778"), Some("04/05/2026")));
        assert_eq!((b["fromGstin"].as_str(), b["fromPincode"].as_u64(), b["fromPlace"].as_str()), (Some("29AAGCB7383J1Z4"), Some(560058), Some("Bengaluru")));
        assert_eq!(b["toGstin"], "27AAPFU0939F1ZV");

        let cn = sale(&mut s, "CreditNote", "2026-05-10", &buyer, ("50000", "1", "50000"), &[("Output IGST", "9000")]);
        assert!(ewaybill_json(&s.slot, &cn, &details, 6).unwrap_err().contains("Sales and Purchase"));
    }
}

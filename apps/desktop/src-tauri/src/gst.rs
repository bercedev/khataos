//! GST returns from the books: GSTR-1 and GSTR-3B for a period, and the
//! draft GSTR-1 JSON. The rules live in `khata_in::returns`; this module
//! turns vouchers into its documents.
//!
//! Which vouchers feed which side (KhataOS records a sales return as a
//! Credit Note and a purchase return as a Debit Note):
//! - outward (GSTR-1; GSTR-3B 3.1 and 3.2): Sales → invoice, Credit Note →
//!   credit note;
//! - inward (GSTR-3B 4(A)(5) ITC): Purchase → invoice, Debit Note → the
//!   supplier's credit note (it reduces ITC).
//!
//! Other voucher types are left out, even when they post to tax ledgers (a
//! warning says so). A voucher's lines split as on the printed invoice
//! (`invoice.rs`): the party line carries the total; item lines and other
//! ledgers on the opposite side are the taxable value (a ledger named
//! "Round off" excepted); tax ledgers give the tax by component. The tax is
//! spread over the lines by taxable value × the rate on file for each
//! line's HSN, or by taxable value alone when a rate is missing (the rate is
//! then worked out from the amounts).
//!
//! Reverse charge and blocked credit (`khata_in::rcm`): a line whose item or
//! ledger tax class starts `RCM:` is a reverse-charge line, `BLOCKED:` a
//! blocked credit; the code after the prefix is the HSN/SAC. On a purchase,
//! output-tax ledgers carry the tax owed on reverse charge (spread over the
//! reverse-charge lines) and input-tax ledgers the credit (less that tax,
//! spread over the other lines).
//!
//! GSTR-1A candidates (`gstr1_amendments`): the user marks a GSTR-1 period
//! filed (a shared record of the company holding the time); outward vouchers of
//! that period created, altered, cancelled or deleted after that time are
//! listed from the audit trail with their values before and after.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use khata_compliance::RateSchedule;
use khata_core::{BillKind, Book, DrCr, LedgerId, TaxSide, UserRole, Voucher, VoucherKind};
use khata_in::rcm;
use khata_in::returns::{self, Amounts, DocKind, DocLine, Gstr1, Gstr3b, InwardDoc, OutwardDoc, Period};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, group_is_party, open_mut, open_ref, pack, parse_date};

/// Unit quantity code (UQC) for a unit symbol, as the GST portal lists
/// them; `OTH` when the symbol is not a common one.
pub fn uqc(symbol: &str) -> &'static str {
    match symbol.trim().trim_end_matches('.').to_ascii_lowercase().as_str() {
        "nos" | "no" | "nos." | "number" | "numbers" => "NOS",
        "kg" | "kgs" | "kilogram" | "kilograms" => "KGS",
        "g" | "gm" | "gms" | "gram" | "grams" => "GMS",
        "ltr" | "ltrs" | "l" | "litre" | "litres" | "liter" | "liters" => "LTR",
        "mtr" | "mtrs" | "m" | "metre" | "metres" | "meter" | "meters" => "MTR",
        "pcs" | "pc" | "piece" | "pieces" => "PCS",
        "box" | "boxes" => "BOX",
        "doz" | "dozen" => "DOZ",
        "set" | "sets" => "SET",
        "bag" | "bags" => "BAG",
        "btl" | "bottle" | "bottles" => "BTL",
        "pair" | "pairs" | "prs" => "PRS",
        _ => "OTH",
    }
}

/// What else a purchase voucher tells (for GSTR-2B and Rule 37).
pub(crate) struct PurchaseInfo {
    /// Bill references on the supplier's line (often the supplier's invoice number).
    pub refs: Vec<String>,
    /// The supplier's line: the invoice value.
    pub value: Option<Decimal>,
    pub party: Option<LedgerId>,
}

/// What the books give the return builders.
pub(crate) struct Docs {
    pub outward: Vec<OutwardDoc>,
    pub inward: Vec<InwardDoc>,
    /// Alongside `inward`.
    pub inward_info: Vec<PurchaseInfo>,
    pub period: Period,
    pub gstin: Option<String>,
    pub state: String,
    pub warnings: Vec<String>,
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

/// The party line: a debtor/creditor, else the cash or bank line on the
/// party's side, else the first line on that side (as on the invoice print).
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

/// Spread `tax` (by slot) over the lines `idx` of `out`: by taxable value ×
/// the rate on file when every line has one, else by taxable value.
fn spread(
    out: &mut [DocLine],
    known: &[Option<Decimal>],
    idx: &[usize],
    tax: &BTreeMap<&'static str, Decimal>,
    name: &str,
    warnings: &mut Vec<String>,
) {
    let total_tax: Decimal = tax.values().sum();
    if idx.is_empty() {
        if !total_tax.is_zero() {
            warnings.push(format!("{name} has tax but no taxable lines; its tax is left out"));
        }
        return;
    }
    // Weights: taxable value × rate when every line has a rate on file.
    let rated = idx.iter().all(|&i| known[i].is_some())
        && idx.iter().any(|&i| !(known[i].unwrap_or_default() * out[i].taxable_value).is_zero());
    let weights: Vec<Decimal> = idx
        .iter()
        .map(|&i| if rated { out[i].taxable_value * known[i].unwrap_or_default() } else { out[i].taxable_value })
        .collect();
    let weight_sum: Decimal = weights.iter().sum();
    if rated {
        for &i in idx {
            out[i].rate = known[i];
        }
        let expected = weight_sum / Decimal::ONE_HUNDRED;
        let booked = total_tax - tax.get("cess").copied().unwrap_or_default();
        if (expected - booked).abs() > Decimal::ONE {
            warnings.push(format!(
                "{name}: GST booked is {}, but the rates on file give {}",
                booked.normalize(),
                expected.round_dp(2).normalize()
            ));
        }
    }
    let last = weights.iter().rposition(|w| !w.is_zero());
    for (slot, total) in tax {
        let mut left = *total;
        for (k, &i) in idx.iter().enumerate() {
            let share = match last {
                Some(j) if k == j => left,
                Some(_) if !weight_sum.is_zero() => (*total * weights[k] / weight_sum).round_dp(2),
                _ => Decimal::ZERO,
            };
            left -= share;
            let l = &mut out[i];
            match *slot {
                "igst" => l.igst = share,
                "cgst" => l.cgst = share,
                "sgst" => l.sgst = share,
                _ => l.cess = share,
            }
        }
        if last.is_none() && !total.is_zero() {
            warnings.push(format!("{name}: its lines add up to zero, so its tax is left out"));
        }
    }
}

/// A tax class split into its HSN/SAC and the reverse-charge and blocked flags.
pub(crate) fn class_flags(class: Option<&str>) -> (Option<String>, bool, bool) {
    match class {
        None => (None, false, false),
        Some(c) => {
            let f = rcm::parse_class(c);
            (Some(f.class.to_owned()).filter(|c| !c.is_empty()), f.reverse_charge, f.blocked)
        }
    }
}

/// One voucher's lines, with tax spread over them. `inward`: a purchase or
/// purchase return, where output-tax ledgers carry tax owed on reverse charge.
fn lines(book: &Book, v: &Voucher, party: Option<usize>, rates: &RateSchedule, inward: bool, warnings: &mut Vec<String>) -> Vec<DocLine> {
    let name = format!("{} {}", label(v.kind), v.number);
    let party_side = party.map(|i| v.entries[i].side);
    let mut out: Vec<DocLine> = Vec::new();
    let mut known: Vec<Option<Decimal>> = Vec::new();
    let mut tax: BTreeMap<&'static str, Decimal> = BTreeMap::new();
    // Inward only: output-tax ledgers (the tax owed on reverse charge), signed.
    let mut owed: BTreeMap<&'static str, Decimal> = BTreeMap::new();
    let mut outward_rcm = false;
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
                    warnings.push(format!("{name}: {ledger_name} collects {c}, which is not GST; it is left out"));
                    continue;
                }
            };
            let map = if inward && t.side == TaxSide::Output { &mut owed } else { &mut tax };
            *map.entry(slot).or_default() += signed;
            continue;
        }
        let mut push = |class: Option<&str>, line: DocLine, out: &mut Vec<DocLine>, known: &mut Vec<Option<Decimal>>| {
            let (hsn, reverse_charge, blocked) = class_flags(class);
            outward_rcm |= reverse_charge && !inward;
            known.push(hsn.as_deref().and_then(|c| rates.rate(c, v.date)));
            out.push(DocLine { hsn, reverse_charge: reverse_charge && inward, blocked: blocked && inward, ..line });
        };
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                push(
                    item.and_then(|i| i.tax_class.as_deref()),
                    DocLine {
                        uqc: item.and_then(|i| book.unit(i.unit)).map(|u| uqc(&u.symbol).to_owned()),
                        qty: Some(if on_party_side { -l.qty } else { l.qty }),
                        taxable_value: if on_party_side { -l.amount } else { l.amount },
                        ..DocLine::default()
                    },
                    &mut out,
                    &mut known,
                );
            }
        } else if ledger_name.to_lowercase().contains("round") {
            // Round-off is part of the invoice value, not of the taxable value.
        } else if on_party_side {
            warnings.push(format!(
                "{name}: {ledger_name} is on the party's side; it is not deducted from the taxable value"
            ));
        } else {
            // A service or sales ledger line: its own HSN/SAC, when set.
            push(ledger.and_then(|l| l.tax_class.as_deref()), DocLine { taxable_value: signed, ..DocLine::default() }, &mut out, &mut known);
        }
    }
    if outward_rcm {
        warnings.push(format!(
            "{name} has lines on reverse charge; outward supplies on reverse charge are not marked as such in GSTR-1 (check this invoice)"
        ));
    }
    let all: Vec<usize> = (0..out.len()).collect();
    let (rc, fwd): (Vec<usize>, Vec<usize>) = all.iter().copied().partition(|&i| out[i].reverse_charge);
    // The tax owed is posted on the supplier's side, so it comes in negative.
    let liability: BTreeMap<&'static str, Decimal> = owed.iter().filter(|(_, a)| !a.is_zero()).map(|(k, a)| (*k, -*a)).collect();
    if rc.is_empty() {
        if !liability.is_empty() {
            warnings.push(format!(
                "{name} posts output tax on a purchase but none of its lines is on reverse charge (RCM: before the HSN/SAC); it is left out of 3.1(d)"
            ));
        }
        spread(&mut out, &known, &all, &tax, &name, warnings);
    } else if liability.is_empty() {
        warnings.push(format!(
            "{name} has lines on reverse charge but posts no output tax for them; the input tax is taken as the tax on reverse charge"
        ));
        spread(&mut out, &known, &all, &tax, &name, warnings);
    } else {
        spread(&mut out, &known, &rc, &liability, &name, warnings);
        // The credit booked less the credit on reverse charge (taken as the tax owed).
        let mut rest = tax.clone();
        for (k, a) in &liability {
            *rest.entry(*k).or_default() -= *a;
        }
        let credit: Decimal = tax.values().sum();
        let owed_total: Decimal = liability.values().sum();
        if fwd.is_empty() {
            if !credit.is_zero() && (credit - owed_total).abs() > Decimal::ONE {
                warnings.push(format!(
                    "{name}: input tax booked ({}) is not the tax on reverse charge ({}); the credit is taken as the tax owed",
                    credit.normalize(),
                    owed_total.normalize()
                ));
            }
        } else {
            if rest.values().any(|a| a.is_sign_negative() && !a.is_zero()) {
                warnings.push(format!(
                    "{name} mixes reverse-charge and other lines and its input tax is less than the tax on reverse charge"
                ));
            }
            spread(&mut out, &known, &fwd, &rest, &name, warnings);
        }
    }
    out
}

fn collect(book: &Book, from: &str, to: &str) -> ApiResult<Docs> {
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    collect_period(book, period)
}

/// The rates on file as a schedule.
pub(crate) fn rate_schedule(book: &Book) -> ApiResult<RateSchedule> {
    let mut rates = RateSchedule::default();
    for (class, changes) in book.profile().map(|p| &p.tax_rates).into_iter().flatten() {
        for (from, r) in changes {
            rates.set(class, *from, *r).map_err(err)?;
        }
    }
    Ok(rates)
}

/// The outward and inward documents dated in `period`.
pub(crate) fn collect_period(book: &Book, period: Period) -> ApiResult<Docs> {
    if book.country() != Some("IN") {
        return Err("GST returns are for companies in India".into());
    }
    let pack = pack(book).ok_or("pick the company's country first")?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let state = profile
        .tax_region
        .clone()
        .ok_or("set the company's GSTIN and state first (Company → Tax registration)")?;
    let rates = rate_schedule(book)?;
    let state_name = pack.regions().into_iter().find(|r| r.code == state).map_or(state.clone(), |r| r.name.to_owned());
    let mut d = Docs {
        outward: Vec::new(),
        inward: Vec::new(),
        inward_info: Vec::new(),
        period,
        gstin: profile.tax_id.clone(),
        state: state.clone(),
        warnings: Vec::new(),
    };
    let mut assumed_local = 0;
    let mut other_kinds = 0;
    let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| period.contains(v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let outward = match v.kind {
            VoucherKind::Sales | VoucherKind::CreditNote => true,
            VoucherKind::Purchase | VoucherKind::DebitNote => false,
            _ => {
                let taxed = v.entries.iter().any(|e| book.ledger(e.ledger).is_some_and(|l| l.tax.is_some()));
                other_kinds += usize::from(taxed && !v.cancelled);
                continue;
            }
        };
        let party = party_line(book, v);
        let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
        let party_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        let tax_id = ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty());
        let region = ledger.and_then(|l| crate::tax::party_region(pack, l));
        let lines = lines(book, v, party, &rates, !outward, &mut d.warnings);
        let id = v.id.0.to_string();
        if outward {
            if region.is_none() && !v.cancelled {
                assumed_local += 1;
            }
            d.outward.push(OutwardDoc {
                id,
                kind: if v.kind == VoucherKind::Sales { DocKind::Invoice } else { DocKind::CreditNote },
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_gstin: tax_id,
                place_of_supply: region.unwrap_or_else(|| state.clone()),
                value: party.map(|i| v.entries[i].amount),
                cancelled: v.cancelled,
                lines,
            });
        } else {
            let refs = party
                .map(|i| v.entries[i].bills.iter().filter(|b| b.kind == BillKind::New).map(|b| b.reference.clone()).collect())
                .unwrap_or_default();
            d.inward_info.push(PurchaseInfo {
                refs,
                value: party.map(|i| v.entries[i].amount),
                party: party.map(|i| v.entries[i].ledger),
            });
            d.inward.push(InwardDoc {
                id,
                kind: if v.kind == VoucherKind::Purchase { DocKind::Invoice } else { DocKind::CreditNote },
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_gstin: tax_id,
                cancelled: v.cancelled,
                lines,
            });
        }
    }
    if assumed_local > 0 {
        d.warnings.push(format!(
            "{assumed_local} sales or credit note(s) have no valid party GSTIN to give a state; they are taken as within {state_name}"
        ));
    }
    if other_kinds > 0 {
        d.warnings.push(format!(
            "{other_kinds} journal or other voucher(s) post to tax ledgers; they are not in these returns"
        ));
    }
    Ok(d)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Gstr1Dto {
    pub gstin: Option<String>,
    pub state: String,
    #[serde(flatten)]
    pub ret: Gstr1,
}

/// GSTR-1 for a period: sections, summary and warnings.
pub fn gstr1(slot: &Slot, from: &str, to: &str) -> ApiResult<Gstr1Dto> {
    let d = collect(open_ref(slot)?, from, to)?;
    let mut ret = returns::gstr1(&d.outward, d.period, &d.state);
    ret.warnings.splice(0..0, d.warnings);
    Ok(Gstr1Dto { gstin: d.gstin, state: d.state, ret })
}

/// The draft GSTR-1 JSON (not validated against the portal schema; see
/// `khata_in::returns::gstr1_json`).
pub fn gstr1_json(slot: &Slot, from: &str, to: &str) -> ApiResult<String> {
    let d = collect(open_ref(slot)?, from, to)?;
    let gstin = d.gstin.ok_or("set the company's GSTIN first (Company → Tax registration)")?;
    let ret = returns::gstr1(&d.outward, d.period, &d.state);
    serde_json::to_string_pretty(&returns::gstr1_json(&ret, &gstin)).map_err(err)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Gstr3bDto {
    pub gstin: Option<String>,
    pub state: String,
    #[serde(flatten)]
    pub ret: Gstr3b,
    /// Total output tax (3.1(a) + 3.1(b)) and total ITC, for the footer.
    pub output: Amounts,
}

/// GSTR-3B for a period, with Rule 37 reversals falling due in it
/// (`gst_rcm::rule37_rows`) in 4(B)(2).
pub fn gstr3b(slot: &Slot, from: &str, to: &str) -> ApiResult<Gstr3bDto> {
    let book = open_ref(slot)?;
    let d = collect(book, from, to)?;
    let mut ret = returns::gstr3b(&d.outward, &d.inward, d.period, &d.state);
    ret.warnings.splice(0..0, d.warnings);
    let mut rule37 = Amounts::default();
    for r in crate::gst_rcm::rule37_rows(book, d.period.to)?.rows.iter().filter(|r| d.period.contains(r.reverse_from)) {
        for (sum, a) in [(&mut rule37.igst, r.reversal.igst), (&mut rule37.cgst, r.reversal.cgst), (&mut rule37.sgst, r.reversal.sgst), (&mut rule37.cess, r.reversal.cess)] {
            *sum += a;
        }
    }
    if !rule37.tax().is_zero() {
        ret = returns::with_rule37(ret, &rule37);
        ret.warnings.push("4(B)(2) includes ITC reversed under Rule 37 (suppliers unpaid after 180 days); see the Rule 37 report".into());
    }
    let output = Amounts {
        taxable: ret.outward_taxable.taxable + ret.zero_rated.taxable,
        igst: ret.outward_taxable.igst + ret.zero_rated.igst,
        cgst: ret.outward_taxable.cgst + ret.zero_rated.cgst,
        sgst: ret.outward_taxable.sgst + ret.zero_rated.sgst,
        cess: ret.outward_taxable.cess + ret.zero_rated.cess,
    };
    Ok(Gstr3bDto { gstin: d.gstin, state: d.state, ret, output })
}

/// Save a return or e-invoice file the user asked for: GSTR-1, e-invoice
/// and e-way bill JSON, or ZATCA XML (a .json or .xml file only).
pub fn write_json(path: &str, contents: &str) -> ApiResult<()> {
    let p = std::path::Path::new(path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json") || e.eq_ignore_ascii_case("xml")) {
        return Err("save this file with a .json or .xml name".into());
    }
    std::fs::write(p, contents).map_err(|e| format!("cannot write {path}: {e}"))
}

// ------------------------------------------------------- GSTR-1A candidates

/// Shared-record key prefix for "GSTR-1 filed" markers:
/// `gst.gstr1_filed.YYYY-MM` → the time it was marked (RFC 3339). Not in
/// the books; synced with the company (`docs/sync-settings.md`).
const FILED_KEY: &str = "gst.gstr1_filed.";

/// "2026-05", "05-2026" or the portal's "052026" → (2026, 5).
fn parse_month(period: &str) -> ApiResult<(i32, u32)> {
    let p = period.trim();
    let sep = |i: usize| p.as_bytes().get(i).is_some_and(|c| *c == b'-' || *c == b'/');
    let (y, m) = if p.len() == 7 && sep(4) {
        (p[..4].parse::<i32>().ok(), p[5..].parse::<u32>().ok())
    } else if p.len() == 7 && sep(2) {
        (p[3..].parse::<i32>().ok(), p[..2].parse::<u32>().ok())
    } else if p.len() == 6 && p.bytes().all(|c| c.is_ascii_digit()) {
        (p[2..].parse::<i32>().ok(), p[..2].parse::<u32>().ok())
    } else {
        (None, None)
    };
    match (y, m) {
        (Some(y), Some(m)) if (1..=12).contains(&m) && (2017..=2100).contains(&y) => Ok((y, m)),
        _ => Err(format!("{period:?} is not a return period (expected YYYY-MM, e.g. 2026-05)")),
    }
}

fn month_bounds((y, m): (i32, u32)) -> Period {
    let from = NaiveDate::from_ymd_opt(y, m, 1).expect("valid month");
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) };
    Period { from, to: next.expect("valid month").pred_opt().expect("not the first day ever") }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FiledPeriodDto {
    /// YYYY-MM.
    pub period: String,
    pub filed_at: DateTime<Utc>,
}

fn store_of(slot: &Slot) -> ApiResult<&khata_store::Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

/// Periods marked as filed, oldest first.
pub fn gstr1_filed_periods(slot: &Slot) -> ApiResult<Vec<FiledPeriodDto>> {
    open_ref(slot)?;
    let mut out: Vec<FiledPeriodDto> = store_of(slot)?
        .records(FILED_KEY)
        .map_err(err)?
        .into_iter()
        .filter_map(|(k, v)| {
            let period = k.strip_prefix(FILED_KEY)?.to_owned();
            let filed_at = DateTime::parse_from_rfc3339(&v).ok()?.with_timezone(&Utc);
            Some(FiledPeriodDto { period, filed_at })
        })
        .collect();
    out.sort_by(|a, b| a.period.cmp(&b.period));
    Ok(out)
}

/// Mark a GSTR-1 period as filed now (`filed`), or clear the mark.
/// Changes to its outward vouchers after this time are GSTR-1A candidates.
pub fn gstr1_mark_filed(slot: &mut Slot, period: &str, filed: bool) -> ApiResult<Vec<FiledPeriodDto>> {
    let (y, m) = parse_month(period)?;
    {
        let (store, book) = open_mut(slot)?;
        crate::api::require(book, UserRole::Accountant)?;
        if book.country() != Some("IN") {
            return Err("GST returns are for companies in India".into());
        }
        let key = format!("{FILED_KEY}{y:04}-{m:02}");
        let value = filed.then(|| crate::api::now().to_rfc3339());
        store.set_record(&key, value.as_deref(), &crate::api::os_user()).map_err(err)?;
    }
    gstr1_filed_periods(slot)
}

/// A voucher's GSTR-1 figures at one point in time.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocSnapshot {
    pub number: String,
    pub date: NaiveDate,
    pub party: String,
    pub party_gstin: Option<String>,
    /// The party line (invoice value).
    pub value: Decimal,
    pub taxable: Decimal,
    pub tax: Decimal,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AmendmentDto {
    /// The filed period (YYYY-MM) the document belongs to.
    pub period: String,
    pub filed_at: DateTime<Utc>,
    pub voucher_id: String,
    pub kind: VoucherKind,
    /// "added", "altered", "cancelled" or "deleted" since filing.
    pub change: String,
    /// The last change.
    pub changed_at: DateTime<Utc>,
    pub user: String,
    /// As filed (`None`: added after filing).
    pub before: Option<DocSnapshot>,
    /// Now (`None`: deleted).
    pub after: Option<DocSnapshot>,
}

fn snapshot(book: &Book, v: &Voucher, rates: &RateSchedule) -> DocSnapshot {
    let party = party_line(book, v);
    let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
    let mut ignored = Vec::new();
    let lines = lines(book, v, party, rates, false, &mut ignored);
    let taxable: Decimal = lines.iter().map(|l| l.taxable_value).sum();
    let tax: Decimal = lines.iter().map(|l| l.igst + l.cgst + l.sgst + l.cess).sum();
    DocSnapshot {
        number: v.number.clone(),
        date: v.date,
        party: ledger.map(|l| l.name.clone()).unwrap_or_default(),
        party_gstin: ledger.and_then(|l| l.tax_id.clone()),
        value: party.map_or(Decimal::ZERO, |i| v.entries[i].amount),
        taxable,
        tax,
        cancelled: v.cancelled,
    }
}

/// The voucher with bank dates cleared, to tell real changes from a bank
/// reconciliation.
fn without_bank_dates(v: &Voucher) -> Voucher {
    let mut v = v.clone();
    for e in &mut v.entries {
        e.bank_date = None;
    }
    v
}

/// Outward documents (Sales, Credit Notes) of filed GSTR-1 periods changed
/// after the period was marked filed: candidates for GSTR-1A (Rule 59(1)
/// proviso, Notif. 12/2024-CT) or, after GSTR-3B, for amendment in a later
/// GSTR-1. One row per voucher and period: as filed, and as it is now.
pub fn gstr1_amendments(slot: &Slot) -> ApiResult<Vec<AmendmentDto>> {
    let filed = gstr1_filed_periods(slot)?;
    let book = open_ref(slot)?;
    let rates = rate_schedule(book)?;
    let outward = |v: &Voucher| matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote);
    let mut out = Vec::new();
    for f in &filed {
        let Ok(month) = parse_month(&f.period) else { continue };
        let period = month_bounds(month);
        // voucher id → (first `before` after filing, last `after`, last entry).
        let mut changed: BTreeMap<String, (Option<Voucher>, Option<Voucher>, &khata_core::AuditEntry)> = BTreeMap::new();
        for e in book.audit().entries().iter().filter(|e| e.at > f.filed_at) {
            let Some(id) = e.entity.strip_prefix("voucher:") else { continue };
            let parse = |j: &Option<serde_json::Value>| j.clone().and_then(|j| serde_json::from_value::<Voucher>(j).ok());
            let (before, after) = (parse(&e.before), parse(&e.after));
            let relevant = [&before, &after].into_iter().flatten().any(|v| outward(v) && period.contains(v.date));
            if !relevant {
                continue;
            }
            changed
                .entry(id.to_owned())
                .and_modify(|c| {
                    c.1 = after.clone();
                    c.2 = e;
                })
                .or_insert((before, after, e));
        }
        for (id, (before, after, last)) in changed {
            let same = match (&before, &after) {
                (Some(b), Some(a)) => without_bank_dates(b) == without_bank_dates(a),
                _ => false,
            };
            if same {
                continue;
            }
            let change = match (&before, &after) {
                (None, _) => "added",
                (_, None) => "deleted",
                (Some(b), Some(a)) if a.cancelled && !b.cancelled => "cancelled",
                _ => "altered",
            };
            let kind = after.as_ref().or(before.as_ref()).map_or(VoucherKind::Sales, |v| v.kind);
            out.push(AmendmentDto {
                period: f.period.clone(),
                filed_at: f.filed_at,
                voucher_id: id,
                kind,
                change: change.into(),
                changed_at: last.at,
                user: last.user.clone(),
                before: before.as_ref().map(|v| snapshot(book, v, &rates)),
                after: after.as_ref().map(|v| snapshot(book, v, &rates)),
            });
        }
    }
    out.sort_by(|a, b| (&a.period, a.changed_at).cmp(&(&b.period, b.changed_at)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher};
    use crate::api::{StockLineInput, cancel_voucher, create_party_ledger, post_voucher};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

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

    #[test]
    fn uqc_maps_common_symbols() {
        assert_eq!([uqc("Nos"), uqc("kg"), uqc("Ltr"), uqc("mtr"), uqc("pcs"), uqc("Box"), uqc("hrs")], [
            "NOS", "KGS", "LTR", "MTR", "PCS", "BOX", "OTH"
        ]);
    }

    #[test]
    fn returns_from_a_small_indian_company() {
        let (_f, mut slot) = indian_company("gst-returns");
        assert!(gstr1(&slot, "2026-05-01", "2026-05-31").unwrap_err().contains("GSTIN and state"));
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_tax_rate(&mut slot, "8471", "2026-04-01", "18").unwrap();

        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let buyer = create_party_ledger(&mut slot, "Bengaluru Co", &debtors, "", None, Some("29AAGCB7383J1Z4"), None)
            .unwrap()
            .ledger
            .id;
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        let supplier = create_party_ledger(&mut slot, "KA Supplier", &creditors, "", None, Some("29AAGCB7383J1Z4"), None)
            .unwrap()
            .ledger
            .id;
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
        let line = |ledger: &str, side: DrCr, amount: &str, qty: &str, rate: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(&laptop, qty, rate);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();

        // B2B, inter-State.
        let b2b = post(&mut slot, "Sales", "2026-05-02", vec![
            entry(&buyer, DrCr::Dr, "118000"),
            line(&sales, DrCr::Cr, "100000", "2", "50000"),
            entry(&tax["Output IGST"], DrCr::Cr, "18000"),
        ]);
        // B2C small, within Maharashtra.
        post(&mut slot, "Sales", "2026-05-03", vec![
            entry(&walk_in, DrCr::Dr, "47200"),
            line(&sales, DrCr::Cr, "40000", "1", "40000"),
            entry(&tax["Output CGST"], DrCr::Cr, "3600"),
            entry(&tax["Output SGST"], DrCr::Cr, "3600"),
        ]);
        let cancelled = post(&mut slot, "Sales", "2026-05-04", vec![
            entry(&walk_in, DrCr::Dr, "47200"),
            line(&sales, DrCr::Cr, "40000", "1", "40000"),
            entry(&tax["Output CGST"], DrCr::Cr, "3600"),
            entry(&tax["Output SGST"], DrCr::Cr, "3600"),
        ]);
        cancel_voucher(&mut slot, &cancelled.id).unwrap();
        // A sales return from the registered buyer.
        post(&mut slot, "CreditNote", "2026-05-10", vec![
            entry(&buyer, DrCr::Cr, "59000"),
            line(&sales, DrCr::Dr, "50000", "1", "50000"),
            entry(&tax["Output IGST"], DrCr::Dr, "9000"),
        ]);
        // A purchase and a purchase return.
        post(&mut slot, "Purchase", "2026-05-05", vec![
            entry(&supplier, DrCr::Cr, "35400"),
            line(&purchase, DrCr::Dr, "30000", "1", "30000"),
            entry(&tax["Input IGST"], DrCr::Dr, "5400"),
        ]);
        post(&mut slot, "DebitNote", "2026-05-12", vec![
            entry(&supplier, DrCr::Dr, "11800"),
            line(&purchase, DrCr::Cr, "10000", "1", "10000"),
            entry(&tax["Input IGST"], DrCr::Cr, "1800"),
        ]);
        // Outside the period.
        post(&mut slot, "Sales", "2026-06-01", vec![
            entry(&buyer, DrCr::Dr, "59000"),
            line(&sales, DrCr::Cr, "50000", "1", "50000"),
            entry(&tax["Output IGST"], DrCr::Cr, "9000"),
        ]);

        let r = gstr1(&slot, "2026-05-01", "2026-05-31").unwrap();
        assert_eq!((r.gstin.as_deref(), r.state.as_str(), r.ret.fp.as_str()), (Some("27AAPFU0939F1ZV"), "27", "052026"));
        assert_eq!(r.ret.b2b.len(), 1);
        let inv = &r.ret.b2b[0];
        assert_eq!((inv.id.as_str(), inv.value, inv.place_of_supply.as_str()), (b2b.id.as_str(), dec!(118000), "29"));
        assert_eq!((inv.items[0].rate, inv.items[0].amounts.igst), (dec!(18), dec!(18000)));
        assert_eq!(r.ret.b2cs.len(), 1);
        assert_eq!((r.ret.b2cs[0].amounts.taxable, r.ret.b2cs[0].amounts.cgst), (dec!(40000), dec!(3600)));
        assert_eq!(r.ret.cdnr.len(), 1);
        assert_eq!(r.ret.cdnr[0].totals().igst, dec!(9000));
        let hsn: Vec<_> = r.ret.hsn_b2b.iter().map(|h| (h.hsn.as_str(), h.uqc.as_str(), h.qty, h.amounts.taxable)).collect();
        assert_eq!(hsn, [("847130", "NOS", dec!(1), dec!(50000))]);
        assert_eq!(r.ret.hsn_b2c[0].amounts.taxable, dec!(40000));
        let docs: Vec<_> = r.ret.docs_issued.iter().map(|d| (d.doc_num, d.total, d.cancelled)).collect();
        assert_eq!(docs, [(1, 3, 1), (5, 1, 0)]);
        assert!(r.ret.warnings.iter().any(|w| w.contains("taken as within Maharashtra")), "{:?}", r.ret.warnings);
        assert!(!r.ret.warnings.iter().any(|w| w.contains("HSN") || w.contains("rate")), "{:?}", r.ret.warnings);

        let j: serde_json::Value = serde_json::from_str(&gstr1_json(&slot, "2026-05-01", "2026-05-31").unwrap()).unwrap();
        assert_eq!((j["gstin"].as_str(), j["fp"].as_str()), (Some("27AAPFU0939F1ZV"), Some("052026")));
        assert_eq!(j["b2b"][0]["ctin"], "29AAGCB7383J1Z4");
        assert_eq!(j["b2b"][0]["inv"][0]["idt"], "02-05-2026");

        let g = gstr3b(&slot, "2026-05-01", "2026-05-31").unwrap();
        assert_eq!(g.ret.outward_taxable.taxable, dec!(90000));
        assert_eq!(g.ret.itc.igst, dec!(3600), "purchase IGST less the purchase return");
        let net: Vec<_> = g.ret.net.iter().map(|n| (n.component.as_str(), n.net)).collect();
        assert_eq!(net, [("IGST", dec!(5400)), ("CGST", dec!(3600)), ("SGST/UTGST", dec!(3600)), ("Cess", dec!(0))]);
        assert_eq!(g.output.igst, dec!(9000));

        let dir = std::env::temp_dir();
        assert!(write_json(&dir.join("gstr1.csv").to_string_lossy(), "{}").is_err());
    }

    #[test]
    fn gstr1a_candidates_follow_the_audit_trail() {
        let (_f, mut slot) = indian_company("gst-1a");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let cgst = ledger(&mut slot, "Output CGST", "Duties & Taxes", "");
        let sgst = ledger(&mut slot, "Output SGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &cgst, "CGST", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &sgst, "SGST", TaxSide::Output).unwrap();
        let sale = |date: &str, taxable: u32| {
            let tax = taxable * 9 / 100;
            voucher("Sales", date, vec![
                entry(&walk_in, DrCr::Dr, &(taxable + 2 * tax).to_string()),
                entry(&sales, DrCr::Cr, &taxable.to_string()),
                entry(&cgst, DrCr::Cr, &tax.to_string()),
                entry(&sgst, DrCr::Cr, &tax.to_string()),
            ])
        };
        let a = post_voucher(&mut slot, sale("2026-05-02", 1000)).unwrap();
        let b = post_voucher(&mut slot, sale("2026-05-03", 1000)).unwrap();
        let june = post_voucher(&mut slot, sale("2026-06-03", 1000)).unwrap();
        assert!(gstr1_mark_filed(&mut slot, "2026-13", true).is_err());
        let filed = gstr1_mark_filed(&mut slot, "052026", true).unwrap();
        assert_eq!(filed.iter().map(|f| f.period.as_str()).collect::<Vec<_>>(), ["2026-05"]);
        assert!(gstr1_amendments(&slot).unwrap().is_empty(), "nothing changed since filing");

        crate::api::alter_voucher(&mut slot, &a.id, sale("2026-05-02", 2000)).unwrap();
        cancel_voucher(&mut slot, &b.id).unwrap();
        let c = post_voucher(&mut slot, sale("2026-05-30", 500)).unwrap();
        crate::api::alter_voucher(&mut slot, &june.id, sale("2026-06-03", 3000)).unwrap();

        let rows = gstr1_amendments(&slot).unwrap();
        let what: Vec<_> = rows.iter().map(|r| (r.voucher_id.as_str(), r.change.as_str())).collect();
        assert_eq!(what, [(a.id.as_str(), "altered"), (b.id.as_str(), "cancelled"), (c.id.as_str(), "added")]);
        let before = rows[0].before.as_ref().unwrap();
        let after = rows[0].after.as_ref().unwrap();
        assert_eq!((before.value, before.taxable, before.tax), (dec!(1180), dec!(1000), dec!(180)));
        assert_eq!((after.value, after.taxable, after.tax), (dec!(2360), dec!(2000), dec!(360)));
        assert!(rows[2].before.is_none() && rows[1].after.as_ref().unwrap().cancelled);

        assert!(gstr1_mark_filed(&mut slot, "2026-05", false).unwrap().is_empty());
        assert!(gstr1_amendments(&slot).unwrap().is_empty());
    }

    #[test]
    fn missing_rates_are_worked_out_from_the_amounts() {
        let (_f, mut slot) = indian_company("gst-derived");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        let services = ledger(&mut slot, "Consulting", "Sales Accounts", "");
        let round = ledger(&mut slot, "Round Off", "Indirect Expenses", "");
        let cgst = ledger(&mut slot, "Output CGST", "Duties & Taxes", "");
        let sgst = ledger(&mut slot, "Output SGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &cgst, "CGST", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &sgst, "SGST", TaxSide::Output).unwrap();
        post_voucher(&mut slot, voucher("Sales", "2026-05-02", vec![
            entry(&walk_in, DrCr::Dr, "1180"),
            entry(&services, DrCr::Cr, "1000.40"),
            entry(&cgst, DrCr::Cr, "90.04"),
            entry(&sgst, DrCr::Cr, "90.04"),
            entry(&round, DrCr::Dr, "0.48"),
        ]))
        .unwrap();
        let r = gstr1(&slot, "2026-05-01", "2026-05-31").unwrap().ret;
        assert_eq!((r.b2cs[0].rate, r.b2cs[0].amounts.taxable), (dec!(18), dec!(1000.40)));
        assert!(r.warnings.iter().any(|w| w.contains("without an HSN/SAC code")), "{:?}", r.warnings);
        assert!(!r.warnings.iter().any(|w| w.contains("party's side")), "round-off is not an adjustment: {:?}", r.warnings);
    }
}

//! UK 9-box VAT return and its MTD JSON from the books. The rules live in
//! `khata_gb::returns`, `khata_gb::schemes` and `khata_gb::mtd`; this module
//! turns vouchers into their documents.
//!
//! Which vouchers feed which side (KhataOS records a sales return as a
//! Credit Note and a purchase return as a Debit Note):
//! - outward (Boxes 1, 6, 8): Sales → invoice, Credit Note → credit note;
//! - inward (Boxes 4, 7, 9): Purchase → invoice, Debit Note → the
//!   supplier's credit note (it reduces Boxes 4 and 7).
//!
//! Other voucher types are left out, even when they post to VAT ledgers (a
//! warning says so), except postings to VAT ledgers classed "IMPORT" (import
//! VAT paid at the border, Box 4) or "IMPORT:PVA" (postponed import VAT
//! from the monthly statement: the output side's credits in Box 1, the input
//! side's debits in Box 4), on any voucher. A voucher's lines split as in
//! `vat_lines.rs`; a line's class may carry "RC:" (imported services),
//! "DRC:" (domestic reverse charge) or "IMPORT:" (imported goods), see
//! `khata_gb::vat::Mechanism`.
//!
//! Where parties are: a party whose mailing country is outside the UK VAT
//! area (not GB or the Isle of Man) is abroad; a sale to it with no VAT is
//! an export (zero-rated). The company is in Northern Ireland when its
//! nation is NIR or its VAT number has the `XI` prefix; then goods sold to,
//! or bought from, a party in an EU member state go in Box 8 or Box 9.
//!
//! **Schemes.** The Flat Rate Scheme and the Cash Accounting Scheme change
//! the figures, so their settings belong to the company. The core profile
//! has no place for them, so they are a shared record of the company
//! (`vat_gb.schemes`, [`vat_gb_schemes`]): outside the audit trail, and
//! synced with co-workers and the accountant like the books
//! (`docs/sync-settings.md`). Cash accounting (and the flat
//! rate scheme's cash-based turnover) needs bill-wise details: payments
//! go to the invoices they are allocated to (Agst Ref or Advance with the
//! invoice's reference); unallocated payments, credit notes and on-account
//! amounts go to the party's oldest open invoices first (Notice 731 §5.7),
//! with a warning; a sale for cash (the party line is a cash or bank
//! ledger) is paid on its date.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use khata_compliance::RateSchedule;
use khata_core::{BillKind, Book, DrCr, LedgerId, TaxSide, Voucher, VoucherKind};
use khata_gb::returns::{self, DocKind, DocLine, InwardDoc, OutwardDoc, Period, VatReturn};
use khata_gb::schemes::{self, Credit, FlatRate, FlatRateDetails, Payment, Sector};
use khata_gb::vat::{Mechanism, Treatment};
use khata_gb::{mtd, nations, vat, vrn};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, group_is_party, open_mut, open_ref, parse_date};
use crate::vat_lines;

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

/// The UK's classes and treatments for [`vat_lines`].
struct UkRules<'a> {
    rates: &'a RateSchedule,
}

impl vat_lines::Rules for UkRules<'_> {
    type Treatment = Treatment;
    type Mechanism = Mechanism;
    const COMPONENT: &'static str = vat::VAT;

    fn classify(&self, class: Option<&str>, date: NaiveDate) -> (Mechanism, Option<(Treatment, Decimal)>) {
        let Some(c) = class else { return (Mechanism::Normal, None) };
        let (m, base) = vat::mechanism(c);
        let key = if m == Mechanism::Normal { c.to_owned() } else { base };
        (m, self.rates.rate(&key, date).map(|r| (vat::treatment(&key, r), r)))
    }
    fn standard(&self) -> Treatment {
        Treatment::Standard
    }
    fn zero_rated(&self) -> Treatment {
        Treatment::ZeroRated
    }
    fn taxed(&self, t: Treatment) -> bool {
        t.taxed()
    }
    fn self_accounted(&self, m: Mechanism) -> bool {
        m.self_accounted()
    }
    fn ledger_mechanism(&self, class: &str) -> Option<Mechanism> {
        let m = vat::mechanism(class).0;
        matches!(m, Mechanism::Import | Mechanism::PostponedImport).then_some(m)
    }
}

/// Import VAT posted to VAT ledgers classed "IMPORT" or "IMPORT:PVA" on a
/// voucher, as a document with one line per mechanism.
fn import_vat_doc(v: &Voucher, postings: &[vat_lines::LedgerTax<Mechanism>], warnings: &mut Vec<String>) -> Option<InwardDoc> {
    let mut lines = Vec::new();
    let paid: Decimal = postings.iter().filter(|p| p.mechanism == Mechanism::Import).map(|p| p.debit).sum();
    if !paid.is_zero() {
        lines.push(DocLine { treatment: Treatment::Standard, goods: true, taxable_value: Decimal::ZERO, vat: paid, mechanism: Mechanism::Import, capital: false });
    }
    let pva = postings.iter().filter(|p| p.mechanism == Mechanism::PostponedImport);
    let due: Decimal = pva.clone().filter(|p| p.side == TaxSide::Output).map(|p| -p.debit).sum();
    let reclaimed: Decimal = pva.filter(|p| p.side == TaxSide::Input).map(|p| p.debit).sum();
    if !due.is_zero() || !reclaimed.is_zero() {
        if due != reclaimed {
            warnings.push(format!(
                "{}: postponed import VAT due is {} but {} is reclaimed; both are reported as the VAT due (post the statement's VAT to an output and an input ledger classed IMPORT:PVA, and adjust Box 4 by hand if part is not reclaimable)",
                vat_lines::voucher_name(v),
                due.normalize(),
                reclaimed.normalize()
            ));
        }
        lines.push(DocLine { treatment: Treatment::Standard, goods: true, taxable_value: Decimal::ZERO, vat: due, mechanism: Mechanism::PostponedImport, capital: false });
    }
    (!lines.is_empty()).then(|| InwardDoc {
        id: format!("{}-import-vat", v.id.0),
        kind: DocKind::Invoice,
        number: v.number.clone(),
        date: v.date,
        party_name: "HMRC (import VAT)".into(),
        party_vrn: None,
        ni_eu_goods: false,
        cancelled: v.cancelled,
        lines,
    })
}

/// What the books give the return builder.
struct Docs {
    outward: Vec<OutwardDoc>,
    inward: Vec<InwardDoc>,
    /// Which voucher, party line and party ledger each document comes from.
    origin: BTreeMap<String, (khata_core::VoucherId, Option<usize>)>,
    period: Period,
    vrn: Option<String>,
    northern_ireland: bool,
    warnings: Vec<String>,
}

/// The documents of the period's vouchers; with `history`, also of every
/// earlier voucher (cash accounting pays old invoices), whose warnings are
/// dropped.
fn collect(book: &Book, from: &str, to: &str, history: bool) -> ApiResult<Docs> {
    if book.country() != Some(khata_gb::COUNTRY.code) {
        return Err("the UK VAT return is for companies in the United Kingdom".into());
    }
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let mut rates = RateSchedule::default();
    for (class, changes) in &profile.tax_rates {
        for (from, r) in changes {
            rates.set(class, *from, *r).map_err(err)?;
        }
    }
    let rules = UkRules { rates: &rates };
    let company_vrn = profile.tax_id.clone().filter(|t| !t.trim().is_empty());
    let northern_ireland = profile.tax_region.as_deref().is_some_and(nations::is_northern_ireland)
        || company_vrn.as_deref().is_some_and(vrn::is_xi);
    let mut d = Docs {
        outward: Vec::new(),
        inward: Vec::new(),
        origin: BTreeMap::new(),
        period,
        vrn: company_vrn,
        northern_ireland,
        warnings: Vec::new(),
    };
    let mut other_kinds = 0;
    let mut vouchers: Vec<&Voucher> =
        book.vouchers().filter(|v| v.date <= period.to && (history || period.from <= v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let mut scratch = Vec::new();
        let warnings = if period.contains(v.date) { &mut d.warnings } else { &mut scratch };
        let outward = match v.kind {
            VoucherKind::Sales | VoucherKind::CreditNote => true,
            VoucherKind::Purchase | VoucherKind::DebitNote => false,
            _ => {
                let postings = vat_lines::ledger_tax(&rules, book, v);
                d.inward.extend(import_vat_doc(v, &postings, warnings));
                let taxed = v.entries.iter().any(|e| {
                    book.ledger(e.ledger).is_some_and(|l| {
                        l.tax.is_some() && l.tax_class.as_deref().and_then(|c| vat_lines::Rules::ledger_mechanism(&rules, c)).is_none()
                    })
                });
                other_kinds += usize::from(taxed && !v.cancelled && period.contains(v.date));
                continue;
            }
        };
        let party = party_line(book, v);
        let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
        let party_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        let country = ledger.and_then(|l| l.mailing.as_ref()).and_then(|m| m.country.as_deref()).filter(|c| !c.trim().is_empty());
        let abroad = country.is_some_and(|c| !nations::in_uk_vat_area(c));
        let split = vat_lines::split(&rules, book, v, party, !outward, outward && abroad, warnings);
        d.inward.extend(import_vat_doc(v, &split.ledger_tax, warnings));
        let lines: Vec<DocLine> = split
            .lines
            .into_iter()
            .map(|l| DocLine {
                treatment: l.treatment,
                goods: l.goods,
                taxable_value: l.value,
                vat: l.tax,
                mechanism: l.mechanism,
                capital: l.capital,
            })
            .collect();
        if !outward
            && abroad
            && !v.cancelled
            && lines.iter().all(|l| l.mechanism == Mechanism::Normal && l.vat.is_zero())
            && lines.iter().any(|l| l.treatment.taxed() && !l.taxable_value.is_zero())
        {
            warnings.push(format!(
                "{} {} from {party_name} (abroad) has no VAT: if it is a service under the reverse charge, give its lines an RC: class; imported goods an IMPORT: class",
                label(v.kind),
                v.number
            ));
        }
        let ni_eu_goods = northern_ireland && country.is_some_and(nations::in_eu);
        let id = v.id.0.to_string();
        d.origin.insert(id.clone(), (v.id, party));
        if outward {
            d.outward.push(OutwardDoc {
                id,
                kind: if v.kind == VoucherKind::Sales { DocKind::Invoice } else { DocKind::CreditNote },
                number: v.number.clone(),
                date: v.date,
                party_name,
                ni_eu_goods,
                cancelled: v.cancelled,
                lines,
            });
        } else {
            d.inward.push(InwardDoc {
                id,
                kind: if v.kind == VoucherKind::Purchase { DocKind::Invoice } else { DocKind::CreditNote },
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_vrn: ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty()),
                ni_eu_goods,
                cancelled: v.cancelled,
                lines,
            });
        }
    }
    if other_kinds > 0 {
        d.warnings.push(format!(
            "{other_kinds} journal or other voucher(s) post to tax ledgers; they are not in this return"
        ));
    }
    Ok(d)
}

// ------------------------------------------------------------ schemes

const SCHEMES_KEY: &str = "vat_gb.schemes";

/// The company's VAT accounting schemes: a shared record of the company
/// (`vat_gb.schemes`), synced with co-workers and the accountant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UkVatSchemes {
    /// The Flat Rate Scheme (Notice 733); `None`: not on it.
    pub flat_rate: Option<FlatRate>,
    /// The Cash Accounting Scheme (Notice 731). Not with the Flat Rate
    /// Scheme, which has its own cash-based turnover.
    pub cash_accounting: bool,
    /// The Annual Accounting Scheme (Notice 732): one return a year with
    /// interim payments. Only a note: pick the whole year as the period.
    pub annual_accounting: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UkVatSchemesDto {
    pub settings: UkVatSchemes,
    /// HMRC's flat rates by sector, for the dialog.
    pub sectors: &'static [Sector],
    pub limited_cost_percent: Decimal,
}

fn read_schemes(slot: &Slot) -> ApiResult<UkVatSchemes> {
    let store = slot.as_ref().map(|(s, _)| s).ok_or("no company is open")?;
    match store.record(SCHEMES_KEY).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("the VAT scheme settings are damaged: {e}")),
        None => Ok(UkVatSchemes::default()),
    }
}

/// The company's VAT schemes and the sector list.
pub fn vat_gb_schemes(slot: &Slot) -> ApiResult<UkVatSchemesDto> {
    let book = open_ref(slot)?;
    if book.country() != Some(khata_gb::COUNTRY.code) {
        return Err("VAT schemes are for companies in the United Kingdom".into());
    }
    Ok(UkVatSchemesDto {
        settings: read_schemes(slot)?,
        sectors: schemes::SECTORS,
        limited_cost_percent: schemes::LIMITED_COST_PERCENT,
    })
}

/// Save the company's VAT schemes (owner only).
pub fn set_vat_gb_schemes(slot: &mut Slot, settings: &UkVatSchemes) -> ApiResult<UkVatSchemesDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, khata_core::UserRole::Owner)?;
    if book.country() != Some(khata_gb::COUNTRY.code) {
        return Err("VAT schemes are for companies in the United Kingdom".into());
    }
    let mut clean = settings.clone();
    if let Some(f) = clean.flat_rate.as_mut() {
        if clean.cash_accounting {
            return Err("the Cash Accounting Scheme cannot be used with the Flat Rate Scheme (Notice 731 §2.8): choose the flat rate scheme's cash-based turnover instead".into());
        }
        f.sector = f.sector.trim().to_owned();
        if !(f.percent > Decimal::ZERO && f.percent < Decimal::ONE_HUNDRED) {
            return Err("the flat rate is a percentage between 0 and 100".into());
        }
    }
    let json = serde_json::to_string(&clean).map_err(err)?;
    store.set_record(SCHEMES_KEY, Some(&json), &crate::api::os_user()).map_err(err)?;
    vat_gb_schemes(slot)
}

/// Payments and credit notes set against the documents' invoices, from the
/// bill-wise details (see the module notes).
fn settlements(book: &Book, d: &Docs, warnings: &mut Vec<String>) -> (Vec<Payment>, Vec<Credit>) {
    struct Open {
        id: String,
        date: NaiveDate,
        /// Still to pay.
        left: Decimal,
    }
    // Invoices by party ledger, oldest first, and by (ledger, reference).
    let mut by_party: BTreeMap<LedgerId, Vec<Open>> = BTreeMap::new();
    let mut by_ref: BTreeMap<(LedgerId, String), usize> = BTreeMap::new();
    let mut invoice_vouchers = std::collections::BTreeSet::new();
    let mut credit_docs: BTreeMap<khata_core::VoucherId, String> = BTreeMap::new();
    let mut payments = Vec::new();
    let mut credits = Vec::new();
    let invoices = d
        .outward
        .iter()
        .filter(|o| o.kind != DocKind::CreditNote && !o.cancelled)
        .map(|o| o.id.clone())
        .chain(d.inward.iter().filter(|i| i.kind != DocKind::CreditNote && !i.cancelled).map(|i| i.id.clone()));
    for id in invoices {
        let Some(&(vid, Some(pi))) = d.origin.get(&id) else { continue };
        let Some(v) = book.voucher(vid) else { continue };
        let e = &v.entries[pi];
        invoice_vouchers.insert(vid);
        if book.is_cash_or_bank(e.ledger) {
            // A cash sale or purchase: paid on its date.
            payments.push(Payment { doc_id: id, date: v.date, amount: e.amount });
            continue;
        }
        by_party.entry(e.ledger).or_default().push(Open { id, date: v.date, left: e.amount });
    }
    for list in by_party.values_mut() {
        list.sort_by_key(|o| o.date);
    }
    for (ledger, list) in &by_party {
        for (i, o) in list.iter().enumerate() {
            let Some(&(vid, Some(pi))) = d.origin.get(&o.id) else { continue };
            let Some(v) = book.voucher(vid) else { continue };
            for b in v.entries[pi].bills.iter().filter(|b| b.kind == BillKind::New && !b.reference.trim().is_empty()) {
                by_ref.insert((*ledger, b.reference.trim().to_uppercase()), i);
            }
            by_ref.entry((*ledger, v.number.trim().to_uppercase())).or_insert(i);
        }
    }
    for o in d.outward.iter().filter(|o| o.kind == DocKind::CreditNote) {
        if let Some(&(vid, _)) = d.origin.get(&o.id) {
            credit_docs.insert(vid, o.id.clone());
        }
    }
    for i in d.inward.iter().filter(|i| i.kind == DocKind::CreditNote) {
        if let Some(&(vid, _)) = d.origin.get(&i.id) {
            credit_docs.insert(vid, i.id.clone());
        }
    }
    let mut fifo_used: BTreeMap<LedgerId, usize> = BTreeMap::new();
    let mut unmatched: Vec<String> = Vec::new();
    let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| v.date <= d.period.to && !v.cancelled).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        if invoice_vouchers.contains(&v.id) {
            continue;
        }
        let credit = credit_docs.get(&v.id);
        for e in &v.entries {
            let Some(list) = by_party.get_mut(&e.ledger) else { continue };
            // The side that pays the party's invoices: the opposite of the invoice's party line.
            let Some(first) = list.first() else { continue };
            let Some(&(fv, Some(fpi))) = d.origin.get(&first.id) else { continue };
            let invoice_side = book.voucher(fv).map(|x| x.entries[fpi].side);
            let sign = if Some(e.side) == invoice_side { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
            let mut left = e.amount;
            let mut apply = |idx: usize, amount: Decimal, list: &mut Vec<Open>| {
                list[idx].left -= amount;
                match credit {
                    Some(cid) => credits.push(Credit { invoice_id: list[idx].id.clone(), credit_id: cid.clone() }),
                    None => payments.push(Payment { doc_id: list[idx].id.clone(), date: v.date, amount: amount * sign }),
                }
            };
            for b in e.bills.iter().filter(|b| matches!(b.kind, BillKind::Against | BillKind::Advance)) {
                let key = (e.ledger, b.reference.trim().to_uppercase());
                match by_ref.get(&key) {
                    Some(&idx) => {
                        apply(idx, b.amount, list);
                        left -= b.amount;
                    }
                    None if b.kind == BillKind::Advance => {
                        if d.period.contains(v.date) {
                            unmatched.push(format!("{} {} ({}: no invoice yet)", v.number, b.reference, b.amount.normalize()));
                        }
                        left -= b.amount;
                    }
                    None => {}
                }
            }
            if left <= Decimal::ZERO || sign.is_sign_negative() {
                if sign.is_sign_negative() && d.period.contains(v.date) && credit.is_none() {
                    unmatched.push(format!("{} (a refund of {})", v.number, e.amount.normalize()));
                }
                continue;
            }
            // Unallocated: the oldest open invoices first (Notice 731 §5.7).
            let mut used = false;
            for idx in 0..list.len() {
                if left <= Decimal::ZERO {
                    break;
                }
                let open = list[idx].left;
                if open <= Decimal::ZERO {
                    continue;
                }
                let take = open.min(left);
                apply(idx, take, list);
                left -= take;
                used = true;
                if credit.is_some() {
                    // A credit note sets against one invoice.
                    left = Decimal::ZERO;
                }
            }
            if used && d.period.contains(v.date) {
                *fifo_used.entry(e.ledger).or_default() += 1;
            }
            if left > Decimal::ZERO && d.period.contains(v.date) {
                unmatched.push(format!("{} ({} left over)", v.number, left.normalize()));
            }
        }
    }
    for (ledger, n) in fifo_used {
        let name = book.ledger(ledger).map(|l| l.name.clone()).unwrap_or_default();
        warnings.push(format!(
            "Cash accounting: {n} payment(s) or credit note(s) with {name} are not allocated to bills; they were set against the oldest open invoices (Notice 731 §5.7). Allocate them bill-wise if that is wrong"
        ));
    }
    if !unmatched.is_empty() {
        warnings.push(format!(
            "Cash accounting: these payments are not matched to an invoice, so their VAT is left out: {}",
            unmatched.join(", ")
        ));
    }
    (payments, credits)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VatReturnGbDto {
    /// The company's VAT registration number as stored.
    pub vrn: Option<String>,
    /// The company trades goods from Northern Ireland (nation NIR or an XI
    /// number): Boxes 8 and 9 can be non-zero.
    pub northern_ireland: bool,
    /// "standard", "flat_rate" or "cash".
    pub scheme: &'static str,
    /// How the flat rate figures were reached (Flat Rate Scheme only).
    pub flat_rate: Option<FlatRateDetails>,
    /// The company is on the Annual Accounting Scheme (a note only).
    pub annual_accounting: bool,
    #[serde(flatten)]
    pub ret: VatReturn,
}

fn build(slot: &Slot, from: &str, to: &str) -> ApiResult<VatReturnGbDto> {
    let book = open_ref(slot)?;
    let settings = if book.country() == Some(khata_gb::COUNTRY.code) { read_schemes(slot)? } else { UkVatSchemes::default() };
    let cash_turnover = settings.flat_rate.as_ref().is_some_and(|f| f.cash_turnover);
    let history = settings.cash_accounting || cash_turnover;
    let mut d = collect(book, from, to, history)?;
    let mut warnings = std::mem::take(&mut d.warnings);
    let (outward, inward) = if history {
        let (payments, credits) = settlements(book, &d, &mut warnings);
        let (o, i, w) = schemes::cash_basis(&d.outward, &d.inward, &payments, &credits, d.period);
        warnings.extend(w);
        if cash_turnover { (o, d.inward.clone()) } else { (o, i) }
    } else {
        (d.outward.clone(), d.inward.clone())
    };
    let (scheme, mut ret, flat_rate) = match &settings.flat_rate {
        Some(f) => {
            let (r, x) = schemes::flat_rate_return(&outward, &inward, d.period, f);
            ("flat_rate", r, Some(x))
        }
        None if settings.cash_accounting => ("cash", returns::vat_return(&outward, &inward, d.period), None),
        None => ("standard", returns::vat_return(&outward, &inward, d.period), None),
    };
    ret.warnings.splice(0..0, warnings);
    if settings.annual_accounting {
        ret.warnings.push(
            "Annual Accounting Scheme: the return covers the whole accounting year and is due 2 months after it ends; Box 5 less the interim payments made is the balance to pay (Notice 732)"
                .to_owned(),
        );
    }
    Ok(VatReturnGbDto { vrn: d.vrn, northern_ireland: d.northern_ireland, scheme, flat_rate, annual_accounting: settings.annual_accounting, ret })
}

/// The 9-box VAT return for a period: boxes and warnings, under the
/// company's VAT scheme.
pub fn vat_return_gb(slot: &Slot, from: &str, to: &str) -> ApiResult<VatReturnGbDto> {
    build(slot, from, to)
}

/// The MTD VAT API body for the period's return (`periodKey` from HMRC's
/// obligations). Nothing is sent, and `finalised` is `false`: the user's
/// declaration belongs to whatever submits it.
pub fn mtd_vat_json(slot: &Slot, from: &str, to: &str, period_key: &str) -> ApiResult<String> {
    let dto = build(slot, from, to)?;
    mtd::vat_return_json(&dto.ret, period_key, false).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, indian_company, ledger, voucher};
    use crate::api::{StockLineInput, cancel_voucher, check_tax_id, create_company, create_party_ledger, post_voucher, set_country};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{InvoiceTaxInput, InvoiceTaxLine, invoice_tax, set_company_tax, set_ledger_tax, set_tax_rate};
    use chrono::Datelike;
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

    /// HMRC's worked examples (VAT Software Developer Forum, 2009).
    const VRN: &str = "GB 562 2359 45";
    const SUPPLIER_VRN: &str = "562235987";

    fn uk_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Ltd").unwrap();
        set_country(&mut slot, Some(&f.0), "GB").unwrap();
        (f, slot)
    }

    fn items(item_id: &str, amount: &str) -> Vec<StockLineInput> {
        vec![StockLineInput {
            item_id: item_id.into(),
            qty: "1".into(),
            rate: amount.into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }]
    }

    fn item(slot: &mut Slot, name: &str, unit: &str, class: &str) -> String {
        let input: StockItemInput =
            serde_json::from_value(serde_json::json!({ "name": name, "unit_id": unit, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    fn set_party_country(slot: &mut Slot, id: &str, name: &str, group: &str, country: &str) {
        let input: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": name,
            "group_id": group_id(slot, group),
            "mailing": { "country": country },
        }))
        .unwrap();
        crate::masters::alter_ledger(slot, id, &input).unwrap();
    }

    #[test]
    fn a_uk_company_checks_vat_numbers_and_charges_20_percent() {
        let f = TempFile::new("gb-vat");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Ltd").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "gb").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("GB", "GBP", "VAT Reg No"));
        assert_eq!((info.fy_start.month(), info.fy_start.day()), (4, 1), "April years");

        let ok = check_tax_id(&slot, " gb 562 2359 45 ").unwrap();
        assert_eq!((ok.value.as_str(), ok.error, ok.region, ok.entity_id.as_deref()), ("GB562235945", None, None, Some("562235945")));
        let bad = check_tax_id(&slot, "GB 562 2359 46").unwrap();
        assert!(bad.error.as_deref().is_some_and(|e| e.contains("modulus 97")), "{:?}", bad.error);

        assert!(set_company_tax(&mut slot, VRN, "LDN").is_err(), "the nation must be known");
        let tax = set_company_tax(&mut slot, VRN, "ENG").unwrap();
        assert_eq!((tax.label.as_str(), tax.region_name.as_deref(), tax.regions.len()), ("VAT Reg No", Some("England"), 4));
        for (class, percent) in [("STANDARD", "20"), ("REDUCED", "5"), ("ZERO", "0"), ("EXEMPT", "0")] {
            set_tax_rate(&mut slot, class, "2011-01-04", percent).unwrap();
        }
        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Glasgow Ltd", &debtors, "", None, Some(SUPPLIER_VRN), None).unwrap().ledger.id;
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        let t = invoice_tax(&slot, &InvoiceTaxInput {
            kind: VoucherKind::Sales,
            date: "2026-05-01".into(),
            party_id: buyer,
            lines: vec![
                InvoiceTaxLine { item_id: String::new(), ledger_id: String::new(), tax_class: "STANDARD".into(), taxable_value: "1999.99".into() },
                InvoiceTaxLine { item_id: String::new(), ledger_id: String::new(), tax_class: "REDUCED".into(), taxable_value: "100".into() },
            ],
        })
        .unwrap();
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.place_of_supply.as_deref(), Some("ENG"), "a VRN gives no nation: the company's own");
        let rows: Vec<_> = t.rows.iter().map(|r| (r.component.as_str(), r.percent.as_str(), r.amount.as_str())).collect();
        assert_eq!(rows, [("VAT", "20", "400"), ("VAT", "5", "5")], "399.998 rounds to 400.00");

        // Invoice numbers are unique across years.
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let sale = |date: &str, number: &str| crate::api::VoucherInput {
            number: Some(number.into()),
            ..voucher("Sales", date, vec![entry(&cash, DrCr::Dr, "120"), entry(&sales, DrCr::Cr, "120")])
        };
        post_voucher(&mut slot, sale("2027-03-31", "INV-1")).unwrap();
        assert!(post_voucher(&mut slot, sale("2027-04-01", "inv-1")).unwrap_err().contains("already used"));
    }

    #[test]
    fn nine_box_return_and_mtd_json_from_a_small_company() {
        let (_f, mut slot) = uk_company("gb-return");
        set_company_tax(&mut slot, VRN, "ENG").unwrap();
        for (class, percent) in [("STANDARD", "20"), ("REDUCED", "5"), ("ZERO", "0"), ("EXEMPT", "0")] {
            set_tax_rate(&mut slot, class, "2011-01-04", percent).unwrap();
        }
        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let buyer = create_party_ledger(&mut slot, "Leeds Retail", &debtors, "", None, None, None).unwrap().ledger.id;
        let supplier =
            create_party_ledger(&mut slot, "Cardiff Wholesale", &creditors, "", None, Some(SUPPLIER_VRN), None).unwrap().ledger.id;
        let unregistered = ledger(&mut slot, "Market Stall", "Sundry Creditors", "");
        let foreign = ledger(&mut slot, "New York Inc", "Sundry Debtors", "");
        set_party_country(&mut slot, &foreign, "New York Inc", "Sundry Debtors", "US");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Input VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "VAT", TaxSide::Input).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &nos, "STANDARD");
        let bread = item(&mut slot, "Bread", &nos, "ZERO");
        let stamps = item(&mut slot, "Stamps", &nos, "EXEMPT");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, amount);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();

        // A 20% sale with zero-rated and exempt lines.
        let mut mixed =
            vec![entry(&buyer, DrCr::Dr, "13250.50"), line(&sales, DrCr::Cr, "10000", &laptop), entry(&output, DrCr::Cr, "2000")];
        mixed[1].items.extend(items(&bread, "1000"));
        mixed[1].items.extend(items(&stamps, "250.50"));
        mixed[1].amount = "11250.50".into();
        post(&mut slot, "Sales", "2026-04-10", mixed);
        let cancelled = post(&mut slot, "Sales", "2026-04-11", vec![
            entry(&buyer, DrCr::Dr, "1200"),
            line(&sales, DrCr::Cr, "1000", &laptop),
            entry(&output, DrCr::Cr, "200"),
        ]);
        cancel_voucher(&mut slot, &cancelled.id).unwrap();
        // A sales return, and an export with no VAT.
        post(&mut slot, "CreditNote", "2026-05-02", vec![
            entry(&buyer, DrCr::Cr, "1200"),
            line(&sales, DrCr::Dr, "1000", &laptop),
            entry(&output, DrCr::Dr, "200"),
        ]);
        post(&mut slot, "Sales", "2026-05-03", vec![entry(&foreign, DrCr::Dr, "4000"), line(&sales, DrCr::Cr, "4000", &laptop)]);
        // Purchases, a purchase return, and VAT from an unregistered seller.
        post(&mut slot, "Purchase", "2026-05-05", vec![
            entry(&supplier, DrCr::Cr, "7200"),
            line(&purchase, DrCr::Dr, "6000", &laptop),
            entry(&input, DrCr::Dr, "1200"),
        ]);
        post(&mut slot, "DebitNote", "2026-05-12", vec![
            entry(&supplier, DrCr::Dr, "1200"),
            line(&purchase, DrCr::Cr, "1000", &laptop),
            entry(&input, DrCr::Cr, "200"),
        ]);
        post(&mut slot, "Purchase", "2026-05-20", vec![
            entry(&unregistered, DrCr::Cr, "120"),
            entry(&purchase, DrCr::Dr, "100"),
            entry(&input, DrCr::Dr, "20"),
        ]);
        // Outside the quarter.
        post(&mut slot, "Sales", "2026-07-01", vec![
            entry(&buyer, DrCr::Dr, "1200"),
            line(&sales, DrCr::Cr, "1000", &laptop),
            entry(&output, DrCr::Cr, "200"),
        ]);

        let r = vat_return_gb(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!((r.vrn.as_deref(), r.northern_ireland), (Some("GB562235945"), false));
        let b = &r.ret;
        assert_eq!((b.vat_due_sales, b.vat_due_acquisitions, b.total_vat_due), (dec!(1800), dec!(0), dec!(1800)));
        assert_eq!((b.vat_reclaimed, b.net_vat_due), (dec!(1000), dec!(800)));
        // Box 6: 11250.50 − 1000 + 4000 export = 14250.50 → 14250. Box 7: 6000 − 1000 + 100.
        assert_eq!((b.total_sales_ex_vat, b.total_purchases_ex_vat), (dec!(14250), dec!(5100)));
        assert_eq!((b.goods_supplied_to_eu, b.goods_acquired_from_eu), (dec!(0), dec!(0)));
        assert!(b.warnings.iter().any(|w| w.contains("no supplier VAT number")), "{:?}", b.warnings);
        assert!(!b.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", b.warnings);

        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["vat_due_sales"].as_str(), j["net_vat_due"].as_str()), (Some("1800"), Some("800")));

        assert!(mtd_vat_json(&slot, "2026-04-01", "2026-06-30", "26A").unwrap_err().contains("period key"));
        let json = mtd_vat_json(&slot, "2026-04-01", "2026-06-30", "26A1").unwrap();
        let m: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(m["periodKey"], "26A1");
        assert_eq!((m["vatDueSales"].as_f64(), m["totalVatDue"].as_f64()), (Some(1800.0), Some(1800.0)));
        assert_eq!((m["vatReclaimedCurrPeriod"].as_f64(), m["netVatDue"].as_f64()), (Some(1000.0), Some(800.0)));
        assert_eq!((m["totalValueSalesExVAT"].as_i64(), m["totalValuePurchasesExVAT"].as_i64()), (Some(14250), Some(5100)));
        assert_eq!(m["finalised"], false, "the user's declaration is not KhataOS's to make");
        assert!(json.contains("\"vatDueSales\": 1800.00,"), "{json}");
    }

    #[test]
    fn northern_ireland_goods_to_and_from_the_eu() {
        let (_f, mut slot) = uk_company("gb-ni");
        set_company_tax(&mut slot, "XI 562 2359 87", "NIR").unwrap();
        set_tax_rate(&mut slot, "ZERO", "2011-01-04", "0").unwrap();
        let dublin = ledger(&mut slot, "Dublin Ltd", "Sundry Debtors", "");
        set_party_country(&mut slot, &dublin, "Dublin Ltd", "Sundry Debtors", "IE");
        let lyon = ledger(&mut slot, "Lyon SARL", "Sundry Creditors", "");
        set_party_country(&mut slot, &lyon, "Lyon SARL", "Sundry Creditors", "FR");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let freight = ledger(&mut slot, "Consulting", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let goods = item(&mut slot, "Widget", &nos, "ZERO");
        let mut sold = entry(&sales, DrCr::Cr, "3000");
        sold.items = items(&goods, "3000");
        post_voucher(&mut slot, voucher("Sales", "2026-05-01", vec![entry(&dublin, DrCr::Dr, "3500"), sold, entry(&freight, DrCr::Cr, "500")]))
            .unwrap();
        let mut bought = entry(&purchase, DrCr::Dr, "1200.80");
        bought.items = items(&goods, "1200.80");
        post_voucher(&mut slot, voucher("Purchase", "2026-05-02", vec![entry(&lyon, DrCr::Cr, "1200.80"), bought])).unwrap();

        let r = vat_return_gb(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert!(r.northern_ireland);
        assert_eq!((r.ret.total_sales_ex_vat, r.ret.goods_supplied_to_eu), (dec!(3500), dec!(3000)), "the service stays out of Box 8");
        assert_eq!((r.ret.total_purchases_ex_vat, r.ret.goods_acquired_from_eu), (dec!(1200), dec!(1200)));
        assert!(r.ret.warnings.iter().any(|w| w.contains("Box 2")), "{:?}", r.ret.warnings);
        let m: serde_json::Value = serde_json::from_str(&mtd_vat_json(&slot, "2026-04-01", "2026-06-30", "26A1").unwrap()).unwrap();
        assert_eq!((m["totalValueGoodsSuppliedExVAT"].as_i64(), m["totalAcquisitionsExVAT"].as_i64()), (Some(3000), Some(1200)));
    }

    #[test]
    fn the_uk_return_is_only_for_uk_companies() {
        let (_f, slot) = indian_company("gb-return-india");
        assert!(vat_return_gb(&slot, "2026-04-01", "2026-06-30").unwrap_err().contains("United Kingdom"));
        let (_g, slot) = uk_company("gb-return-dates");
        assert!(vat_return_gb(&slot, "2026-06-30", "2026-04-01").unwrap_err().contains("before it starts"));
    }

    fn alter(slot: &mut Slot, id: &str, fields: serde_json::Value) {
        let input: crate::masters::LedgerInput = serde_json::from_value(fields).unwrap();
        crate::masters::alter_ledger(slot, id, &input).unwrap();
    }

    /// A UK company with rates and VAT ledgers: (output, input).
    fn vat_books(slot: &mut Slot) -> (String, String) {
        set_company_tax(slot, VRN, "ENG").unwrap();
        for (class, percent) in [("STANDARD", "20"), ("REDUCED", "5"), ("ZERO", "0"), ("EXEMPT", "0")] {
            set_tax_rate(slot, class, "2011-01-04", percent).unwrap();
        }
        let output = ledger(slot, "Output VAT", "Duties & Taxes", "");
        let input = ledger(slot, "Input VAT", "Duties & Taxes", "");
        set_ledger_tax(slot, &output, "VAT", TaxSide::Output).unwrap();
        set_ledger_tax(slot, &input, "VAT", TaxSide::Input).unwrap();
        (output, input)
    }

    fn boxes(r: &VatReturn) -> [Decimal; 7] {
        [r.vat_due_sales, r.total_vat_due, r.vat_reclaimed, r.net_vat_due, r.total_sales_ex_vat, r.total_purchases_ex_vat, r.goods_supplied_to_eu]
    }

    #[test]
    fn reverse_charges_and_import_vat_from_the_books() {
        let (_f, mut slot) = uk_company("gb-rc");
        let (output, input) = vat_books(&mut slot);
        let creditors = group_id(&slot, "Sundry Creditors");
        let duties = group_id(&slot, "Duties & Taxes");
        let expenses = group_id(&slot, "Indirect Expenses");
        let saas = ledger(&mut slot, "Cloud Inc", "Sundry Creditors", "");
        alter(&mut slot, &saas, serde_json::json!({ "name": "Cloud Inc", "group_id": creditors, "mailing": { "country": "US" } }));
        let builder = create_party_ledger(&mut slot, "Brick Ltd", &creditors, "", None, Some(SUPPLIER_VRN), None).unwrap().ledger.id;
        let maker = ledger(&mut slot, "Shenzhen Maker", "Sundry Creditors", "");
        alter(&mut slot, &maker, serde_json::json!({ "name": "Shenzhen Maker", "group_id": creditors, "mailing": { "country": "CN" } }));
        let software = ledger(&mut slot, "Software", "Indirect Expenses", "");
        alter(&mut slot, &software, serde_json::json!({ "name": "Software", "group_id": expenses, "tax_class": "RC:STANDARD" }));
        let works = ledger(&mut slot, "Building works", "Indirect Expenses", "");
        alter(&mut slot, &works, serde_json::json!({ "name": "Building works", "group_id": expenses, "tax_class": "DRC:STANDARD" }));
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let machine = item(&mut slot, "Machine", &nos, "IMPORT:STANDARD");
        let pva_out = ledger(&mut slot, "PVA output VAT", "Duties & Taxes", "");
        let pva_in = ledger(&mut slot, "PVA input VAT", "Duties & Taxes", "");
        let c79 = ledger(&mut slot, "Import VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &pva_out, "VAT", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &pva_in, "VAT", TaxSide::Input).unwrap();
        set_ledger_tax(&mut slot, &c79, "VAT", TaxSide::Input).unwrap();
        for (id, name, class) in [(&pva_out, "PVA output VAT", "IMPORT:PVA"), (&pva_in, "PVA input VAT", "IMPORT:PVA"), (&c79, "Import VAT", "IMPORT")] {
            alter(&mut slot, id, serde_json::json!({ "name": name, "group_id": duties, "tax_class": class }));
        }
        let bank = ledger(&mut slot, "Barclays", "Bank Accounts", "");
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();
        post(&mut slot, "Purchase", "2026-04-02", vec![entry(&saas, DrCr::Cr, "1000"), entry(&software, DrCr::Dr, "1000"), entry(&input, DrCr::Dr, "200"), entry(&output, DrCr::Cr, "200")]);
        post(&mut slot, "Purchase", "2026-04-03", vec![entry(&builder, DrCr::Cr, "5000"), entry(&works, DrCr::Dr, "5000"), entry(&input, DrCr::Dr, "1000"), entry(&output, DrCr::Cr, "1000")]);
        let mut goods = entry(&purchase, DrCr::Dr, "800");
        goods.items = items(&machine, "800");
        post(&mut slot, "Purchase", "2026-04-04", vec![entry(&maker, DrCr::Cr, "800"), goods]);
        // The monthly postponed import VAT statement, as a journal.
        post(&mut slot, "Journal", "2026-05-12", vec![entry(&pva_in, DrCr::Dr, "400"), entry(&pva_out, DrCr::Cr, "400")]);
        // Import VAT paid at the border (C79).
        post(&mut slot, "Payment", "2026-04-05", vec![entry(&c79, DrCr::Dr, "160"), entry(&bank, DrCr::Cr, "160")]);

        let r = vat_return_gb(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!(r.scheme, "standard");
        assert_eq!(boxes(&r.ret), [dec!(1600), dec!(1600), dec!(1760), dec!(-160), dec!(1000), dec!(6800), dec!(0)], "{:?}", r.ret.warnings);
        assert!(!r.ret.warnings.iter().any(|w| w.contains("journal or other") || w.contains("output VAT (book")), "{:?}", r.ret.warnings);
    }

    #[test]
    fn a_domestic_reverse_charge_sale_prints_the_notice_735_wording() {
        let (_f, mut slot) = uk_company("gb-drc-print");
        let (output, _) = vat_books(&mut slot);
        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Phone Shop Ltd", &debtors, "", None, Some(SUPPLIER_VRN), None).unwrap().ledger.id;
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let phones = item(&mut slot, "Mobile phones", &nos, "DRC:STANDARD");
        let plain = item(&mut slot, "Cases", &nos, "STANDARD");
        let mut line = entry(&sales, DrCr::Cr, "6000");
        line.items = items(&phones, "6000");
        let drc = post_voucher(&mut slot, voucher("Sales", "2026-04-10", vec![entry(&buyer, DrCr::Dr, "6000"), line])).unwrap();
        let html = crate::invoice::voucher_html(&slot, &drc.id).unwrap();
        assert!(html.contains("Reverse charge: Customer to pay the VAT to HMRC."), "{html}");

        let mut line = entry(&sales, DrCr::Cr, "100");
        line.items = items(&plain, "100");
        let normal = post_voucher(&mut slot, voucher("Sales", "2026-04-11", vec![entry(&buyer, DrCr::Dr, "120"), line, entry(&output, DrCr::Cr, "20")])).unwrap();
        assert!(!crate::invoice::voucher_html(&slot, &normal.id).unwrap().contains("Reverse charge"));
    }

    #[test]
    fn flat_rate_scheme_settings_and_return() {
        let (_f, mut slot) = uk_company("gb-frs");
        let (output, input) = vat_books(&mut slot);
        let s = vat_gb_schemes(&slot).unwrap();
        assert!(s.settings.flat_rate.is_none() && s.sectors.len() > 50);
        let frs = FlatRate { sector: " Computer and IT consultancy or data processing ".into(), percent: dec!(14.5), registered_on: None, limited_cost: schemes::LimitedCost::No, cash_turnover: false };
        let both = UkVatSchemes { flat_rate: Some(frs.clone()), cash_accounting: true, annual_accounting: false };
        assert!(set_vat_gb_schemes(&mut slot, &both).unwrap_err().contains("cannot be used with the Flat Rate"));
        let bad = UkVatSchemes { flat_rate: Some(FlatRate { percent: dec!(0), ..frs.clone() }), ..UkVatSchemes::default() };
        assert!(set_vat_gb_schemes(&mut slot, &bad).is_err());
        let saved = set_vat_gb_schemes(&mut slot, &UkVatSchemes { flat_rate: Some(frs), ..UkVatSchemes::default() }).unwrap();
        assert_eq!(saved.settings.flat_rate.as_ref().unwrap().sector, "Computer and IT consultancy or data processing");

        let buyer = ledger(&mut slot, "Leeds Retail", "Sundry Debtors", "");
        let creditors = group_id(&slot, "Sundry Creditors");
        let supplier = create_party_ledger(&mut slot, "Cardiff Wholesale", &creditors, "", None, Some(SUPPLIER_VRN), None).unwrap().ledger.id;
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let stationery = ledger(&mut slot, "Stationery", "Indirect Expenses", "");
        let laptops = ledger(&mut slot, "Computers", "Fixed Assets", "");
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();
        post(&mut slot, "Sales", "2026-04-10", vec![entry(&buyer, DrCr::Dr, "12000"), entry(&sales, DrCr::Cr, "10000"), entry(&output, DrCr::Cr, "2000")]);
        post(&mut slot, "Purchase", "2026-04-11", vec![entry(&supplier, DrCr::Cr, "600"), entry(&stationery, DrCr::Dr, "500"), entry(&input, DrCr::Dr, "100")]);
        post(&mut slot, "Purchase", "2026-04-12", vec![entry(&supplier, DrCr::Cr, "2400"), entry(&laptops, DrCr::Dr, "2000"), entry(&input, DrCr::Dr, "400")]);
        let r = vat_return_gb(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!(r.scheme, "flat_rate");
        let x = r.flat_rate.as_ref().unwrap();
        assert_eq!((x.turnover, x.flat_rate_vat, x.capital_vat), (dec!(12000), dec!(1740.00), dec!(400)));
        assert_eq!(boxes(&r.ret), [dec!(1740.00), dec!(1740.00), dec!(400.00), dec!(1340.00), dec!(12000), dec!(2000), dec!(0)]);
        // The MTD body follows the scheme.
        let json = mtd_vat_json(&slot, "2026-04-01", "2026-06-30", "26A1").unwrap();
        assert!(json.contains("1740"), "{json}");
    }

    #[test]
    fn cash_accounting_follows_payments_and_bills() {
        let (_f, mut slot) = uk_company("gb-cash");
        let (output, input) = vat_books(&mut slot);
        set_vat_gb_schemes(&mut slot, &UkVatSchemes { cash_accounting: true, ..UkVatSchemes::default() }).unwrap();
        let buyer = ledger(&mut slot, "Leeds Retail", "Sundry Debtors", "");
        let creditors = group_id(&slot, "Sundry Creditors");
        let supplier = create_party_ledger(&mut slot, "Cardiff Wholesale", &creditors, "", None, Some(SUPPLIER_VRN), None).unwrap().ledger.id;
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let stationery = ledger(&mut slot, "Stationery", "Indirect Expenses", "");
        let bank = ledger(&mut slot, "Barclays", "Bank Accounts", "");
        let bill = |kind: BillKind, reference: &str, amount: &str| crate::api::BillInput { kind, reference: reference.into(), amount: amount.into(), due: None };
        let post = |slot: &mut Slot, kind: &str, date: &str, number: &str, entries| {
            post_voucher(slot, crate::api::VoucherInput { number: Some(number.into()), ..voucher(kind, date, entries) }).unwrap()
        };
        // March: an invoice from the last quarter, paid on account in April.
        let mut dr = entry(&buyer, DrCr::Dr, "1200");
        dr.bills = vec![bill(BillKind::New, "INV-1", "1200")];
        post(&mut slot, "Sales", "2026-03-20", "INV-1", vec![dr, entry(&sales, DrCr::Cr, "1000"), entry(&output, DrCr::Cr, "200")]);
        let mut dr = entry(&buyer, DrCr::Dr, "2400");
        dr.bills = vec![bill(BillKind::New, "INV-2", "2400")];
        post(&mut slot, "Sales", "2026-04-02", "INV-2", vec![dr, entry(&sales, DrCr::Cr, "2000"), entry(&output, DrCr::Cr, "400")]);
        // Half of INV-2, against its bill.
        let mut cr = entry(&buyer, DrCr::Cr, "1200");
        cr.bills = vec![bill(BillKind::Against, "INV-2", "1200")];
        post(&mut slot, "Receipt", "2026-04-20", "R-1", vec![entry(&bank, DrCr::Dr, "1200"), cr]);
        // On account: the oldest open invoice, INV-1.
        post(&mut slot, "Receipt", "2026-04-21", "R-2", vec![entry(&bank, DrCr::Dr, "1200"), entry(&buyer, DrCr::Cr, "1200")]);
        // A cash sale: paid on its date.
        post(&mut slot, "Sales", "2026-04-22", "INV-3", vec![entry(&bank, DrCr::Dr, "120"), entry(&sales, DrCr::Cr, "100"), entry(&output, DrCr::Cr, "20")]);
        // An unpaid purchase: nothing to reclaim yet.
        post(&mut slot, "Purchase", "2026-04-23", "P-1", vec![entry(&supplier, DrCr::Cr, "600"), entry(&stationery, DrCr::Dr, "500"), entry(&input, DrCr::Dr, "100")]);

        let r = vat_return_gb(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!(r.scheme, "cash");
        assert_eq!(boxes(&r.ret), [dec!(420.00), dec!(420.00), dec!(0), dec!(420.00), dec!(2100), dec!(0), dec!(0)], "{:?}", r.ret.warnings);
        assert!(r.ret.warnings.iter().any(|w| w.contains("oldest open invoices") && w.contains("Leeds Retail")), "{:?}", r.ret.warnings);
    }
}

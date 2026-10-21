//! Reverse charge and ITC reversals from the books (India): the register of
//! reverse-charge purchases with their self-invoices (Rule 47A), the
//! self-invoice print, the Rule 37 report (suppliers unpaid after 180
//! days), the Rule 42 calculator, and the flags on ledgers and items.
//!
//! **The flags** live in the tax class (HSN/SAC field) of an expense or
//! purchase ledger, or of a stock item, as a prefix: `RCM:` reverse charge,
//! `BLOCKED:` blocked credit under s.17(5) (`khata_in::rcm`). Nothing in the
//! core model changes; `set_gst_flags` adds or removes the prefixes.
//!
//! **Self-invoice numbers** are a series of their own, "SI/26-27/1", worked
//! out from the books, not stored: the reverse-charge purchases from
//! unregistered suppliers in a financial year, in the order they were
//! entered (the audit trail). A voucher with a self-invoice should be
//! cancelled rather than deleted, or later numbers move up.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use khata_core::reports;
use khata_core::{AuditAction, Book, LedgerId, StockItemId, TaxSide, UserRole, VoucherId, VoucherKind};
use khata_core::format::{Grouping, amount_in_words};
use khata_in::rcm::{self, Rule42, Rule42Input, UnpaidPurchase};
use khata_in::returns::{Amounts, DocKind, InwardDoc, Period};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, now, open_mut, open_ref, os_user, parse_amount, parse_date, parse_uuid};
use crate::gst::{PurchaseInfo, collect_period};

/// The earliest date the books are read from (GST began 1 Jul 2017).
fn gst_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2017, 7, 1).expect("valid date")
}

fn sum_lines(d: &InwardDoc, keep: impl Fn(&khata_in::returns::DocLine) -> bool) -> Amounts {
    let mut a = Amounts::default();
    for l in d.lines.iter().filter(|l| keep(l)) {
        let o = Amounts::of_line(l);
        a.taxable += o.taxable;
        a.igst += o.igst;
        a.cgst += o.cgst;
        a.sgst += o.sgst;
        a.cess += o.cess;
    }
    a
}

// ------------------------------------------------------ reverse charge

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RcmRow {
    pub voucher_id: String,
    pub kind: VoucherKind,
    pub number: String,
    pub date: NaiveDate,
    pub party: String,
    pub party_gstin: Option<String>,
    /// Reverse-charge lines only: taxable value and the tax owed.
    pub amounts: Amounts,
    /// Credit on them is blocked (s.17(5)).
    pub blocked: bool,
    /// The supplier is unregistered: the company issues a self-invoice
    /// (s.31(3)(f)).
    pub needs_self_invoice: bool,
    /// "SI/26-27/3" when a self-invoice is needed.
    pub self_invoice: Option<String>,
    /// Rule 47A: within 30 days of receipt (the voucher date is taken as
    /// the date of receipt).
    pub self_invoice_due: Option<NaiveDate>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RcmRegisterDto {
    pub rows: Vec<RcmRow>,
    /// 3.1(d) for the period (purchase returns reduce it).
    pub total: Amounts,
    pub warnings: Vec<String>,
}

/// Each voucher's position in the order of entry (its Create in the trail).
fn entry_order(book: &Book) -> BTreeMap<String, u64> {
    book.audit()
        .entries()
        .iter()
        .filter(|e| e.action == AuditAction::Create)
        .filter_map(|e| e.entity.strip_prefix("voucher:").map(|id| (id.to_owned(), e.seq)))
        .collect()
}

/// Self-invoice numbers by voucher id, for the financial years `period`
/// touches (worked out over whole years, so numbers don't depend on it).
fn self_invoice_numbers(book: &Book, docs: &[InwardDoc], kinds: &BTreeMap<String, VoucherKind>) -> BTreeMap<String, String> {
    let order = entry_order(book);
    let mut by_year: BTreeMap<String, Vec<(u64, &InwardDoc)>> = BTreeMap::new();
    for d in docs {
        let needs = kinds.get(&d.id) == Some(&VoucherKind::Purchase)
            && d.lines.iter().any(|l| l.reverse_charge)
            && d.party_gstin.as_deref().is_none_or(|g| g.trim().is_empty());
        if needs {
            by_year.entry(rcm::fy_label(d.date)).or_default().push((order.get(&d.id).copied().unwrap_or(u64::MAX), d));
        }
    }
    let mut out = BTreeMap::new();
    for (_, mut list) in by_year {
        list.sort_by_key(|(seq, d)| (*seq, d.date));
        for (n, (_, d)) in list.into_iter().enumerate() {
            out.insert(d.id.clone(), rcm::self_invoice_number(d.date, n as u32 + 1));
        }
    }
    out
}

fn voucher_kinds(book: &Book) -> BTreeMap<String, VoucherKind> {
    book.vouchers().map(|v| (v.id.0.to_string(), v.kind)).collect()
}

/// The financial year around `d` (April to March).
fn fy_of(d: NaiveDate) -> Period {
    let from = crate::api::fy_start(d);
    let to = NaiveDate::from_ymd_opt(from.year() + 1, 3, 31).expect("valid date");
    Period { from, to }
}

/// Purchases and purchase returns with reverse-charge lines in a period.
pub fn rcm_register(slot: &Slot, from: &str, to: &str) -> ApiResult<RcmRegisterDto> {
    let book = open_ref(slot)?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    // Whole financial years, so self-invoice numbers are the same whatever the period.
    let years = Period { from: fy_of(period.from).from, to: fy_of(period.to).to };
    let docs = collect_period(book, years)?;
    let kinds = voucher_kinds(book);
    let numbers = self_invoice_numbers(book, &docs.inward, &kinds);
    let mut rows = Vec::new();
    let mut total = Amounts::default();
    for d in docs.inward.iter().filter(|d| period.contains(d.date) && d.lines.iter().any(|l| l.reverse_charge)) {
        let amounts = sum_lines(d, |l| l.reverse_charge);
        if !d.cancelled {
            let sign = if d.kind == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
            total.taxable += amounts.taxable * sign;
            total.igst += amounts.igst * sign;
            total.cgst += amounts.cgst * sign;
            total.sgst += amounts.sgst * sign;
            total.cess += amounts.cess * sign;
        }
        let self_invoice = numbers.get(&d.id).cloned();
        rows.push(RcmRow {
            voucher_id: d.id.clone(),
            kind: kinds.get(&d.id).copied().unwrap_or(VoucherKind::Purchase),
            number: d.number.clone(),
            date: d.date,
            party: d.party_name.clone(),
            party_gstin: d.party_gstin.clone(),
            amounts,
            blocked: d.lines.iter().any(|l| l.reverse_charge && l.blocked),
            needs_self_invoice: self_invoice.is_some(),
            self_invoice_due: self_invoice.as_ref().map(|_| rcm::self_invoice_due(d.date)),
            self_invoice,
            cancelled: d.cancelled,
        });
    }
    // Only this voucher's own warnings matter here, not the whole years'.
    let warnings = docs.warnings.into_iter().filter(|w| w.contains("reverse charge") || w.contains("output tax on a purchase")).collect();
    Ok(RcmRegisterDto { rows, total, warnings })
}

/// The self-invoice (Rule 46 invoice issued by the recipient under
/// s.31(3)(f)) for a reverse-charge purchase from an unregistered
/// supplier, as print-ready HTML.
pub fn self_invoice_html(slot: &Slot, voucher_id: &str) -> ApiResult<String> {
    let book = open_ref(slot)?;
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    if v.kind != VoucherKind::Purchase {
        return Err("a self-invoice is printed from a Purchase on reverse charge".into());
    }
    let year = fy_of(v.date);
    let docs = collect_period(book, year)?;
    let kinds = voucher_kinds(book);
    let numbers = self_invoice_numbers(book, &docs.inward, &kinds);
    let id = v.id.0.to_string();
    let number = numbers.get(&id).cloned().ok_or(
        "this purchase needs no self-invoice: none of its lines is on reverse charge (RCM: before the HSN/SAC), or the supplier has a GSTIN and issues the invoice",
    )?;
    let mut doc = crate::invoice::invoice_doc(book, v);
    doc.title = "Self Invoice (Reverse Charge)".into();
    doc.number = number;
    // Service lines print their ledger's SAC too (a self-invoice needs it, Rule 46(g)).
    let classes: BTreeMap<String, Option<String>> = v
        .entries
        .iter()
        .filter_map(|e| book.ledger(e.ledger))
        .map(|l| (l.name.clone(), l.tax_class.clone()))
        .collect();
    for l in &mut doc.lines {
        let class = l.tax_class.clone().or_else(|| classes.get(&l.description).cloned().flatten());
        l.tax_class = crate::gst::class_flags(class.as_deref()).0;
    }
    // Only the tax owed: the output-tax ledgers' lines.
    doc.taxes = v
        .entries
        .iter()
        .filter_map(|e| {
            let l = book.ledger(e.ledger)?;
            (l.tax.as_ref()?.side == TaxSide::Output).then(|| (format!("{} (payable on reverse charge)", l.name), e.amount))
        })
        .collect();
    let taxable: Decimal = doc.lines.iter().map(|l| l.amount).sum();
    let tax: Decimal = doc.taxes.iter().map(|(_, a)| *a).sum();
    let others: Decimal = doc.others.iter().map(|(_, a)| *a).sum();
    doc.total = taxable + tax + others;
    doc.total_in_words = amount_in_words(doc.total, Grouping::Indian, "Rupees", "Paise").trim().to_owned();
    let due = rcm::self_invoice_due(v.date).format("%-d-%b-%Y");
    let note = format!(
        "Tax is payable on reverse charge: Yes (CGST Act s.9(3)/9(4)). Self-invoice issued by the recipient under s.31(3)(f) for a supply from an unregistered supplier; to be issued within 30 days of receipt (Rule 47A), by {due}. Purchase voucher {}.",
        v.number
    );
    doc.narration = if doc.narration.trim().is_empty() { note } else { format!("{note} {}", doc.narration) };
    let address = book.profile().map(|p| p.address.clone()).unwrap_or_default();
    let h = khata_print::Header { company: &book.name, address: &address, grouping: Grouping::Indian };
    let html = khata_print::invoice_html(&h, &doc);
    // The "Bill to" box holds the supplier on a self-invoice.
    Ok(html.replacen("<div class=\"label\">Bill to</div>", "<div class=\"label\">Supplier (unregistered)</div>", 1))
}

// ----------------------------------------------------------- Rule 37

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rule37Row {
    pub voucher_id: String,
    pub number: String,
    pub date: NaiveDate,
    pub party: String,
    pub party_gstin: Option<String>,
    pub reference: String,
    /// Invoice value (the supplier's line).
    pub value: Decimal,
    pub unpaid: Decimal,
    /// Days from the invoice to `as_at`.
    pub days: i64,
    pub period_ends: NaiveDate,
    /// The first day after the 180 days; the reversal is reported in the
    /// GSTR-3B for the period containing it [UNCERTAIN].
    pub reverse_from: NaiveDate,
    /// ITC availed on the invoice.
    pub itc: Amounts,
    /// ITC to reverse (proportionate to the unpaid share), 4(B)(2).
    pub reversal: Amounts,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rule37Dto {
    pub as_at: NaiveDate,
    pub rows: Vec<Rule37Row>,
    pub total: Amounts,
    pub warnings: Vec<String>,
}

/// Purchases (forward charge, from registered suppliers, credit not
/// blocked) whose bill is still unpaid more than 180 days after the
/// invoice, as at `as_at`, with the ITC to reverse under Rule 37. Unpaid
/// amounts come from the bill-wise details (New Ref) on the supplier line;
/// purchases without them are counted in a warning.
pub(crate) fn rule37_rows(book: &Book, as_at: NaiveDate) -> ApiResult<Rule37Dto> {
    let cutoff = as_at - chrono::Duration::days(rcm::RULE37_DAYS);
    let mut out = Rule37Dto { as_at, rows: Vec::new(), total: Amounts::default(), warnings: Vec::new() };
    if cutoff < gst_start() {
        return Ok(out);
    }
    let docs = collect_period(book, Period { from: gst_start(), to: cutoff })?;
    let open: BTreeMap<(LedgerId, String), Decimal> = reports::outstanding_bills(book, as_at)
        .into_iter()
        .map(|b| ((b.ledger, b.reference), b.amount))
        .collect();
    let mut no_bills = 0;
    for (d, info) in docs.inward.iter().zip(&docs.inward_info) {
        if d.kind != DocKind::Invoice || d.cancelled || d.party_gstin.as_deref().is_none_or(|g| g.trim().is_empty()) {
            continue;
        }
        let itc = sum_lines(d, |l| !l.reverse_charge && !l.blocked);
        if itc.tax().is_zero() {
            continue;
        }
        let PurchaseInfo { refs, value, party } = info;
        let (Some(value), Some(party)) = (value, party) else { continue };
        if refs.is_empty() {
            no_bills += 1;
            continue;
        }
        for reference in refs {
            // Payable bills are negative.
            let unpaid = open.get(&(*party, reference.clone())).map_or(Decimal::ZERO, |a| (-*a).max(Decimal::ZERO));
            let p = UnpaidPurchase { invoice_date: d.date, value: *value, unpaid, itc };
            let Some(r) = rcm::rule37(&p, as_at) else { continue };
            for (sum, a) in [
                (&mut out.total.igst, r.reversal.igst),
                (&mut out.total.cgst, r.reversal.cgst),
                (&mut out.total.sgst, r.reversal.sgst),
                (&mut out.total.cess, r.reversal.cess),
            ] {
                *sum += a;
            }
            out.rows.push(Rule37Row {
                voucher_id: d.id.clone(),
                number: d.number.clone(),
                date: d.date,
                party: d.party_name.clone(),
                party_gstin: d.party_gstin.clone(),
                reference: reference.clone(),
                value: *value,
                unpaid,
                days: (as_at - d.date).num_days(),
                period_ends: r.period_ends,
                reverse_from: r.reverse_from,
                itc,
                reversal: Amounts { taxable: Decimal::ZERO, ..r.reversal },
            });
        }
    }
    if no_bills > 0 {
        out.warnings.push(format!(
            "{no_bills} purchase(s) older than 180 days have no bill-wise details (New Ref) on the supplier's line, so whether they are paid is not known"
        ));
    }
    Ok(out)
}

pub fn rule37_report(slot: &Slot, as_at: &str) -> ApiResult<Rule37Dto> {
    rule37_rows(open_ref(slot)?, parse_date(as_at)?)
}

// ----------------------------------------------------------- Rule 42

#[derive(Debug, Clone, Deserialize)]
pub struct Rule42Strings {
    pub total: String,
    #[serde(default)]
    pub non_business: String,
    #[serde(default)]
    pub exempt_only: String,
    #[serde(default)]
    pub blocked: String,
    #[serde(default)]
    pub taxable_only: String,
    pub exempt_turnover: String,
    pub total_turnover: String,
    #[serde(default)]
    pub non_business_use: bool,
}

/// Rule 42 for one tax, from typed amounts.
pub fn rule42_calc(input: &Rule42Strings) -> ApiResult<Rule42> {
    let i = Rule42Input {
        total: parse_amount(&input.total)?,
        non_business: parse_amount(&input.non_business)?,
        exempt_only: parse_amount(&input.exempt_only)?,
        blocked: parse_amount(&input.blocked)?,
        taxable_only: parse_amount(&input.taxable_only)?,
        exempt_turnover: parse_amount(&input.exempt_turnover)?,
        total_turnover: parse_amount(&input.total_turnover)?,
        non_business_use: input.non_business_use,
    };
    rcm::rule42(&i).map_err(str::to_owned)
}

// ------------------------------------------------------------- flags

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GstFlagRow {
    /// "ledger" or "item".
    pub kind: String,
    pub id: String,
    pub name: String,
    /// The HSN/SAC without flags.
    pub class: Option<String>,
    pub reverse_charge: bool,
    pub blocked: bool,
}

/// Ledgers that can carry a tax class (not parties, cash, bank or tax
/// ledgers) and stock items, with their flags.
pub fn gst_flag_list(slot: &Slot) -> ApiResult<Vec<GstFlagRow>> {
    let book = open_ref(slot)?;
    let row = |kind: &str, id: String, name: &str, class: Option<&str>| {
        let (code, reverse_charge, blocked) = crate::gst::class_flags(class);
        GstFlagRow { kind: kind.into(), id, name: name.to_owned(), class: code, reverse_charge, blocked }
    };
    let mut out: Vec<GstFlagRow> = book
        .ledgers()
        .filter(|l| l.tax.is_none() && !crate::api::group_is_party(book, l.group) && !book.is_cash_or_bank(l.id))
        .map(|l| row("ledger", l.id.0.to_string(), &l.name, l.tax_class.as_deref()))
        .collect();
    out.sort_by_key(|r| r.name.to_lowercase());
    let mut items: Vec<GstFlagRow> =
        book.stock_items().map(|i| row("item", i.id.0.to_string(), &i.name, i.tax_class.as_deref())).collect();
    items.sort_by_key(|r| r.name.to_lowercase());
    out.extend(items);
    Ok(out)
}

/// Set the reverse-charge and blocked-credit flags on a ledger's or stock
/// item's tax class (keeping its HSN/SAC).
pub fn set_gst_flags(slot: &mut Slot, kind: &str, id: &str, reverse_charge: bool, blocked: bool) -> ApiResult<GstFlagRow> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    if book.country() != Some("IN") {
        return Err("reverse charge and blocked credit are Indian GST flags".into());
    }
    let flagged = |class: Option<&str>| {
        let code = class.map(|c| rcm::parse_class(c).class.to_owned()).unwrap_or_default();
        Some(rcm::class_with_flags(&code, reverse_charge, blocked)).filter(|c| !c.is_empty())
    };
    let (name, class) = match kind {
        "ledger" => {
            let mut l = book.ledger(LedgerId(parse_uuid(id, "ledger")?)).ok_or("unknown ledger")?.clone();
            l.tax_class = flagged(l.tax_class.as_deref());
            let out = (l.name.clone(), l.tax_class.clone());
            book.alter_ledger(now(), &os_user(), l).map_err(err)?;
            out
        }
        "item" => {
            let mut i = book.stock_item(StockItemId(parse_uuid(id, "stock item")?)).ok_or("unknown stock item")?.clone();
            i.tax_class = flagged(i.tax_class.as_deref());
            let out = (i.name.clone(), i.tax_class.clone());
            book.save_stock_item(now(), &os_user(), i).map_err(err)?;
            out
        }
        other => return Err(format!("unknown kind {other:?} (expected ledger or item)")),
    };
    store.save(book).map_err(err)?;
    let (code, reverse_charge, blocked) = crate::gst::class_flags(class.as_deref());
    Ok(GstFlagRow { kind: kind.into(), id: id.trim().to_owned(), name, class: code, reverse_charge, blocked })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher};
    use crate::api::{BillInput, cancel_voucher, create_party_ledger, post_voucher};
    use crate::tax::{InvoiceTaxInput, InvoiceTaxLine, invoice_tax, set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::{BillKind, DrCr};
    use rust_decimal_macros::dec;

    struct Co {
        slot: Slot,
        _f: crate::api::tests::TempFile,
        tax: BTreeMap<&'static str, String>,
        legal: String,
        goods: String,
        car: String,
        advocate: String,
        supplier: String,
    }

    fn company(name: &str) -> Co {
        let (f, mut slot) = indian_company(name);
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_tax_rate(&mut slot, "9982", "2017-07-01", "18").unwrap();
        set_tax_rate(&mut slot, "8703", "2017-07-01", "28").unwrap();
        set_tax_rate(&mut slot, "8471", "2017-07-01", "18").unwrap();
        let creditors = group_id(&slot, "Sundry Creditors");
        let advocate = ledger(&mut slot, "Advocate", "Sundry Creditors", "");
        let gstin = format!("27AAGCB7383J1Z{}", khata_in::gstin::check_character("27AAGCB7383J1Z").unwrap());
        let supplier = create_party_ledger(&mut slot, "MH Supplier", &creditors, "", None, Some(&gstin), None).unwrap().ledger.id;
        let mut tax = BTreeMap::new();
        for (name, component, side) in [
            ("Input CGST", "CGST", TaxSide::Input),
            ("Input SGST", "SGST", TaxSide::Input),
            ("Output CGST", "CGST", TaxSide::Output),
            ("Output SGST", "SGST", TaxSide::Output),
        ] {
            let id = ledger(&mut slot, name, "Duties & Taxes", "");
            set_ledger_tax(&mut slot, &id, component, side).unwrap();
            tax.insert(name, id);
        }
        let legal = ledger(&mut slot, "Legal Fees", "Indirect Expenses", "");
        let goods = ledger(&mut slot, "Purchases", "Purchase Accounts", "");
        let car = ledger(&mut slot, "Motor Car", "Fixed Assets", "");
        Co { slot, _f: f, tax, legal, goods, car, advocate, supplier }
    }

    fn set_class(slot: &mut Slot, id: &str, class: &str) {
        let book = &mut slot.as_mut().unwrap().1;
        let mut l = book.ledger(LedgerId(parse_uuid(id, "ledger").unwrap())).unwrap().clone();
        l.tax_class = Some(class.into());
        book.alter_ledger(now(), "test", l).unwrap();
    }

    #[test]
    fn reverse_charge_purchases_flow_to_3b_and_self_invoices() {
        let mut c = company("gst-rcm");
        set_class(&mut c.slot, &c.legal, "9982");
        let row = set_gst_flags(&mut c.slot, "ledger", &c.legal, true, false).unwrap();
        assert_eq!((row.class.as_deref(), row.reverse_charge, row.blocked), (Some("9982"), true, false));
        assert!(gst_flag_list(&c.slot).unwrap().iter().any(|r| r.id == c.legal && r.reverse_charge));
        assert!(!gst_flag_list(&c.slot).unwrap().iter().any(|r| r.id == c.advocate), "parties are not listed");

        // invoice_tax gives the four rows; post them as the voucher screen does.
        let t = invoice_tax(&c.slot, &InvoiceTaxInput {
            kind: VoucherKind::Purchase,
            date: "2026-05-04".into(),
            party_id: c.advocate.clone(),
            lines: vec![InvoiceTaxLine { item_id: String::new(), ledger_id: c.legal.clone(), tax_class: String::new(), taxable_value: "10000".into() }],
        })
        .unwrap();
        let mut entries = vec![entry(&c.advocate, DrCr::Cr, "10000"), entry(&c.legal, DrCr::Dr, "10000")];
        for r in &t.rows {
            let amount = parse_amount(&r.amount).unwrap();
            let side = if amount < Decimal::ZERO { DrCr::Cr } else { DrCr::Dr };
            entries.push(entry(r.ledger_id.as_ref().unwrap(), side, &amount.abs().to_string()));
        }
        let first = post_voucher(&mut c.slot, voucher("Purchase", "2026-05-04", entries.clone())).unwrap();
        let second = post_voucher(&mut c.slot, voucher("Purchase", "2026-04-20", entries)).unwrap();

        let g = crate::gst::gstr3b(&c.slot, "2026-05-01", "2026-05-31").unwrap();
        assert_eq!((g.ret.reverse_charge_inward.taxable, g.ret.reverse_charge_inward.cgst), (dec!(10000), dec!(900)));
        assert_eq!(g.ret.itc_reverse_charge.sgst, dec!(900));
        assert_eq!(g.ret.itc.cgst, dec!(0), "nothing in 4(A)(5)");
        let cgst = g.ret.net.iter().find(|n| n.component == "CGST").unwrap();
        assert_eq!((cgst.output, cgst.itc, cgst.net, cgst.reverse_charge), (dec!(900), dec!(900), dec!(0), dec!(900)));
        assert!(!g.ret.warnings.iter().any(|w| w.contains("reverse charge")), "{:?}", g.ret.warnings);

        // The register numbers self-invoices in the order of entry, not by date.
        let r = rcm_register(&c.slot, "2026-04-01", "2026-05-31").unwrap();
        let si: Vec<_> = r.rows.iter().map(|x| (x.voucher_id.as_str(), x.self_invoice.as_deref())).collect();
        assert_eq!(si, [(second.id.as_str(), Some("SI/26-27/2")), (first.id.as_str(), Some("SI/26-27/1"))]);
        assert_eq!(r.rows[1].self_invoice_due, NaiveDate::from_ymd_opt(2026, 6, 3));
        assert_eq!(r.total.cgst, dec!(1800));
        let may = rcm_register(&c.slot, "2026-05-01", "2026-05-31").unwrap();
        assert_eq!(may.rows[0].self_invoice.as_deref(), Some("SI/26-27/1"), "the same number whatever the period");

        let html = self_invoice_html(&c.slot, &first.id).unwrap();
        for text in ["Self Invoice (Reverse Charge)", "SI/26-27/1", "Supplier (unregistered)", "Advocate", "Rule 47A", "9982", "Output CGST (payable on reverse charge)", "11,800.00"] {
            assert!(html.contains(text), "{text} missing");
        }
        assert!(!html.contains("RCM:"), "the flag is not printed");
        assert!(!html.contains("Input CGST"), "the credit is not on the self-invoice");

        // A registered supplier issues its own invoice: no self-invoice.
        set_class(&mut c.slot, &c.goods, "RCM:8471");
        let reg = post_voucher(&mut c.slot, voucher("Purchase", "2026-05-06", vec![
            entry(&c.supplier, DrCr::Cr, "1000"),
            entry(&c.goods, DrCr::Dr, "1000"),
            entry(&c.tax["Input CGST"], DrCr::Dr, "90"),
            entry(&c.tax["Input SGST"], DrCr::Dr, "90"),
            entry(&c.tax["Output CGST"], DrCr::Cr, "90"),
            entry(&c.tax["Output SGST"], DrCr::Cr, "90"),
        ]))
        .unwrap();
        let r = rcm_register(&c.slot, "2026-05-01", "2026-05-31").unwrap();
        let row = r.rows.iter().find(|x| x.voucher_id == reg.id).unwrap();
        assert!(!row.needs_self_invoice && row.self_invoice.is_none());
        assert!(self_invoice_html(&c.slot, &reg.id).unwrap_err().contains("needs no self-invoice"));
        // Cancelling keeps the numbers.
        cancel_voucher(&mut c.slot, &second.id).unwrap();
        assert_eq!(rcm_register(&c.slot, "2026-05-01", "2026-05-31").unwrap().rows[0].self_invoice.as_deref(), Some("SI/26-27/1"));
    }

    #[test]
    fn blocked_credit_is_reversed_in_4b1() {
        let mut c = company("gst-blocked");
        set_class(&mut c.slot, &c.car, "8703");
        set_gst_flags(&mut c.slot, "ledger", &c.car, false, true).unwrap();
        post_voucher(&mut c.slot, voucher("Purchase", "2026-05-04", vec![
            entry(&c.supplier, DrCr::Cr, "128000"),
            entry(&c.car, DrCr::Dr, "100000"),
            entry(&c.tax["Input CGST"], DrCr::Dr, "14000"),
            entry(&c.tax["Input SGST"], DrCr::Dr, "14000"),
        ]))
        .unwrap();
        let g = crate::gst::gstr3b(&c.slot, "2026-05-01", "2026-05-31").unwrap().ret;
        assert_eq!((g.itc.cgst, g.itc_reversed_permanent.cgst, g.itc_net.cgst), (dec!(14000), dec!(14000), dec!(0)));
    }

    #[test]
    fn rule37_lists_bills_unpaid_after_180_days() {
        let mut c = company("gst-rule37");
        set_class(&mut c.slot, &c.goods, "8471");
        let bill = |reference: &str, amount: &str| BillInput { kind: BillKind::New, reference: reference.into(), amount: amount.into(), due: None };
        let mut party = entry(&c.supplier, DrCr::Cr, "11800");
        party.bills = vec![bill("B-1", "11800")];
        post_voucher(&mut c.slot, voucher("Purchase", "2026-01-10", vec![
            party,
            entry(&c.goods, DrCr::Dr, "10000"),
            entry(&c.tax["Input CGST"], DrCr::Dr, "900"),
            entry(&c.tax["Input SGST"], DrCr::Dr, "900"),
        ]))
        .unwrap();
        // Half paid against the bill.
        let bank = ledger(&mut c.slot, "Bank", "Bank Accounts", "100000");
        let mut paid = entry(&c.supplier, DrCr::Dr, "5900");
        paid.bills = vec![BillInput { kind: BillKind::Against, ..bill("B-1", "5900") }];
        post_voucher(&mut c.slot, voucher("Payment", "2026-03-01", vec![paid, entry(&bank, DrCr::Cr, "5900")])).unwrap();
        // No bill-wise details: counted in a warning.
        post_voucher(&mut c.slot, voucher("Purchase", "2026-01-12", vec![
            entry(&c.supplier, DrCr::Cr, "1180"),
            entry(&c.goods, DrCr::Dr, "1000"),
            entry(&c.tax["Input CGST"], DrCr::Dr, "90"),
            entry(&c.tax["Input SGST"], DrCr::Dr, "90"),
        ]))
        .unwrap();

        assert!(rule37_report(&c.slot, "2026-07-09").unwrap().rows.is_empty(), "day 180");
        let r = rule37_report(&c.slot, "2026-07-31").unwrap();
        assert_eq!(r.rows.len(), 1);
        let row = &r.rows[0];
        assert_eq!((row.reference.as_str(), row.unpaid, row.reversal.cgst, row.reversal.sgst), ("B-1", dec!(5900), dec!(450), dec!(450)));
        assert_eq!(row.reverse_from, NaiveDate::from_ymd_opt(2026, 7, 10).unwrap());
        assert!(r.warnings[0].contains("1 purchase(s)"), "{:?}", r.warnings);
        // It goes to 4(B)(2) of the July return.
        let g = crate::gst::gstr3b(&c.slot, "2026-07-01", "2026-07-31").unwrap().ret;
        assert_eq!((g.itc_reversed_other.cgst, g.itc_net.cgst), (dec!(450), dec!(-450)));
        assert!(crate::gst::gstr3b(&c.slot, "2026-08-01", "2026-08-31").unwrap().ret.itc_reversed_other.cgst.is_zero());
    }

    #[test]
    fn rule42_from_strings() {
        let r = rule42_calc(&Rule42Strings {
            total: "1,00,000".into(),
            non_business: String::new(),
            exempt_only: String::new(),
            blocked: String::new(),
            taxable_only: "20000".into(),
            exempt_turnover: "100000".into(),
            total_turnover: "400000".into(),
            non_business_use: false,
        })
        .unwrap();
        assert_eq!((r.c2, r.d1, r.reverse), (dec!(80000), dec!(20000), dec!(20000)));
    }
}

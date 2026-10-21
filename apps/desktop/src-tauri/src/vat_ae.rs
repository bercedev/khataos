//! UAE VAT 201 from the books. The rules live in `khata_ae::returns`; this
//! module turns vouchers into its documents.
//!
//! Which vouchers feed which side (KhataOS records a sales return as a
//! Credit Note and a purchase return as a Debit Note):
//! - outward (Boxes 1 to 5): Sales → invoice, Credit Note → credit note;
//! - inward (Box 9): Purchase → invoice, Debit Note → the supplier's credit
//!   note (it reduces recoverable VAT).
//!
//! Other voucher types are left out, even when they post to VAT ledgers (a
//! warning says so). A voucher's lines split as in `gst.rs`: the party line
//! carries the total; item lines and other ledgers on the opposite side are
//! the value (a ledger named "Round off" excepted); ledgers whose type of
//! tax is VAT give the VAT.
//!
//! Each line's treatment comes from its stock item's (or service ledger's)
//! tax class and the rate on file for the voucher date
//! (`khata_ae::vat::treatment`: a class named "EXEMPT…" is exempt, any other
//! 0% class zero-rated, else standard). A line without a class or rate is
//! taken as standard-rated. The voucher's VAT is spread over its
//! standard-rated lines by value.
//!
//! **Reverse charge and imports** are marked on the class, without any core
//! change: "RC:STANDARD" (reverse charge at the STANDARD rate) and
//! "IMPORT:STANDARD" (goods imported through UAE Customs), see
//! `khata_ae::vat::Mechanism`. On a purchase, the VAT of such a line is
//! worked out at its rate class (the supplier charged none) and goes in
//! Box 3 or Box 6/7, and in Box 10; the voucher should self-account it (Dr
//! input VAT, Cr output VAT), and a warning compares what it posts to
//! output VAT ledgers. On a sale, an "RC:" line is a domestic reverse-charge
//! supply: its value, without VAT. A purchase from a party abroad with no
//! VAT and no marked line gets a reminder.
//!
//! Simplifications: every standard-rated supply is put in the Box 1 row of
//! the company's own emirate (KhataOS keeps one establishment per company);
//! a sale with no VAT to a party whose mailing address is abroad is an
//! export (zero-rated); other exports need a 0% class; and the rest are
//! listed in `khata_ae::returns`.

use khata_ae::returns::{self, DocKind, DocLine, InwardDoc, OutwardDoc, Period, Vat201};
use khata_ae::vat::{self, Mechanism, Treatment};
use khata_compliance::RateSchedule;
use khata_core::{Book, DrCr, TaxSide, Voucher, VoucherKind};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, group_is_party, open_ref, parse_date};

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

/// One raw line: value, mechanism, and the treatment and rate when its
/// rate class has a rate on file.
type RawLine = (Decimal, Mechanism, Option<(Treatment, Decimal)>);

/// The mechanism and the treatment and rate of a tax class on a date.
fn classify(class: Option<&str>, rates: &RateSchedule, date: chrono::NaiveDate) -> (Mechanism, Option<(Treatment, Decimal)>) {
    match class {
        None => (Mechanism::Normal, None),
        Some(c) => {
            let (m, base) = vat::mechanism(c);
            (m, rates.rate(&base, date).map(|r| (vat::treatment(&base, r), r)))
        }
    }
}

/// One voucher's lines, with treatments and VAT.
///
/// Lines whose class is marked "RC:" or "IMPORT:" carry the VAT the
/// business accounts for itself, worked out at their rate class; the VAT
/// the voucher posts is spread over the other standard-rated lines. A
/// purchase should self-account that VAT (Dr input VAT, Cr output VAT): the
/// output VAT it posts is checked against what is worked out.
fn lines(book: &Book, v: &Voucher, party: Option<usize>, rates: &RateSchedule, warnings: &mut Vec<String>) -> Vec<DocLine> {
    let name = format!("{} {}", label(v.kind), v.number);
    let party_side = party.map(|i| v.entries[i].side);
    let inward = matches!(v.kind, VoucherKind::Purchase | VoucherKind::DebitNote);
    let mut raw: Vec<RawLine> = Vec::new();
    let mut vat_total = Decimal::ZERO;
    // Output VAT a purchase posts (credits less debits): the self-accounted reverse charge.
    let mut self_output = Decimal::ZERO;
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.as_str()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if t.component.eq_ignore_ascii_case(vat::VAT) {
                vat_total += signed;
                if t.side == TaxSide::Output {
                    self_output += if e.side == DrCr::Cr { e.amount } else { -e.amount };
                }
            } else {
                warnings.push(format!("{name}: {ledger_name} collects {}, which is not VAT; it is left out", t.component));
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let class = book.stock_item(l.item).and_then(|i| i.tax_class.clone());
                let (m, k) = classify(class.as_deref(), rates, v.date);
                raw.push((if on_party_side { -l.amount } else { l.amount }, m, k));
            }
        } else if ledger_name.to_lowercase().contains("round") {
            // Round-off is part of the invoice value, not of the supply.
        } else if on_party_side {
            warnings.push(format!("{name}: {ledger_name} is on the party's side; it is not deducted from the value"));
        } else {
            let class = ledger.and_then(|l| l.tax_class.clone());
            let (m, k) = classify(class.as_deref(), rates, v.date);
            raw.push((signed, m, k));
        }
    }
    if raw.is_empty() {
        if !vat_total.is_zero() {
            warnings.push(format!("{name} has VAT but no lines to put it on; its VAT is left out"));
        }
        return Vec::new();
    }
    let mut out: Vec<DocLine> = raw
        .iter()
        .map(|(value, m, k)| DocLine {
            treatment: k.map_or(Treatment::Standard, |(t, _)| t),
            taxable_value: *value,
            vat: Decimal::ZERO,
            mechanism: *m,
        })
        .collect();
    // Self-accounted lines: VAT at the rate class (none on a sale: the
    // recipient accounts for it).
    let mut self_vat = Decimal::ZERO;
    for (l, (value, m, k)) in out.iter_mut().zip(&raw) {
        if *m == Mechanism::Normal {
            continue;
        }
        match k {
            Some((Treatment::Standard, r)) if inward => {
                l.vat = vat::round_fils(*value * r / Decimal::ONE_HUNDRED);
                self_vat += l.vat;
            }
            Some(_) => {}
            None => warnings.push(format!(
                "{name}: a reverse-charge or import line has no rate on file for its class; its VAT is taken as 0"
            )),
        }
    }
    if inward && (self_vat - self_output).abs() > Decimal::ONE {
        warnings.push(format!(
            "{name}: the reverse charge worked out is VAT {}, but the voucher posts {} to output VAT (book Dr input VAT, Cr output VAT for it)",
            self_vat.normalize(),
            self_output.normalize()
        ));
    }
    let normal = |l: &DocLine| l.mechanism == Mechanism::Normal;
    if !vat_total.is_zero() && !out.iter().any(|l| normal(l) && l.treatment == Treatment::Standard) {
        if out.iter().any(normal) {
            warnings.push(format!("{name} has VAT but only zero-rated or exempt lines; it is reported as standard-rated"));
            for l in out.iter_mut().filter(|l| normal(l)) {
                l.treatment = Treatment::Standard;
            }
        } else {
            warnings.push(format!("{name} posts VAT {} beyond its reverse charge; it is left out", vat_total.normalize()));
            return out;
        }
    }
    let standard: Vec<usize> = (0..out.len()).filter(|&i| normal(&out[i]) && out[i].treatment == Treatment::Standard).collect();
    if vat_total.is_zero()
        && raw.iter().zip(&out).any(|((v, m, k), l)| *m == Mechanism::Normal && k.is_none() && l.treatment == Treatment::Standard && !v.is_zero())
    {
        warnings.push(format!(
            "{name} has no VAT and lines without a tax class or rate; they are reported as standard-rated (give them a ZERO or EXEMPT class if they are not)"
        ));
    }
    // Check the VAT against the rates on file when every standard line has one.
    if standard.iter().all(|&i| raw[i].2.is_some()) && !standard.is_empty() {
        let expected: Decimal = standard.iter().map(|&i| raw[i].0 * raw[i].2.map_or(Decimal::ZERO, |(_, r)| r)).sum::<Decimal>()
            / Decimal::ONE_HUNDRED;
        if (expected - vat_total).abs() > Decimal::ONE {
            warnings.push(format!(
                "{name}: VAT booked is {}, but the rates on file give {}",
                vat_total.normalize(),
                expected.round_dp(2).normalize()
            ));
        }
    }
    // Spread the VAT by value; the last standard line takes what is left.
    let weight: Decimal = standard.iter().map(|&i| out[i].taxable_value).sum();
    let last = standard.iter().rev().copied().find(|&i| !out[i].taxable_value.is_zero());
    let mut left = vat_total;
    for &i in &standard {
        let share = match last {
            Some(k) if i == k => left,
            Some(_) if !weight.is_zero() => (vat_total * out[i].taxable_value / weight).round_dp(2),
            _ => Decimal::ZERO,
        };
        left -= share;
        out[i].vat = share;
    }
    if last.is_none() && !vat_total.is_zero() {
        warnings.push(format!("{name}: its lines add up to zero, so its VAT is left out"));
    }
    out
}

/// What the books give the return builder.
struct Docs {
    outward: Vec<OutwardDoc>,
    inward: Vec<InwardDoc>,
    period: Period,
    trn: Option<String>,
    emirate: String,
    warnings: Vec<String>,
}

fn collect(book: &Book, from: &str, to: &str) -> ApiResult<Docs> {
    if book.country() != Some(khata_ae::COUNTRY.code) {
        return Err("VAT 201 is for companies in the United Arab Emirates".into());
    }
    let profile = book.profile().ok_or("pick the company's country first")?;
    let emirate = profile
        .tax_region
        .clone()
        .ok_or("set the company's emirate first (Company → Tax registration)")?;
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
    let mut d = Docs {
        outward: Vec::new(),
        inward: Vec::new(),
        period,
        trn: profile.tax_id.clone(),
        emirate: emirate.clone(),
        warnings: Vec::new(),
    };
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
        let mut lines = lines(book, v, party, &rates, &mut d.warnings);
        // A sale to a party whose address is outside the UAE, charged no
        // VAT, is an export: zero-rated (Box 4).
        let abroad = ledger.and_then(|l| l.mailing.as_ref()).and_then(|m| m.country.as_deref()).is_some_and(|c| c != "AE");
        if outward && abroad && lines.iter().all(|l| l.vat.is_zero()) {
            for l in lines.iter_mut().filter(|l| l.treatment == Treatment::Standard && l.mechanism == Mechanism::Normal) {
                l.treatment = Treatment::ZeroRated;
            }
        }
        // A purchase from abroad with no VAT and nothing marked: likely a
        // reverse charge the user has not marked.
        if !outward
            && abroad
            && !v.cancelled
            && lines.iter().all(|l| l.mechanism == Mechanism::Normal && l.vat.is_zero())
            && lines.iter().any(|l| l.treatment == Treatment::Standard && !l.taxable_value.is_zero())
        {
            d.warnings.push(format!(
                "{} {} from {party_name} (abroad) has no VAT: if it is an imported service or goods, give its lines an RC: or IMPORT: class",
                label(v.kind),
                v.number
            ));
        }
        let id = v.id.0.to_string();
        if outward {
            d.outward.push(OutwardDoc {
                id,
                kind: if v.kind == VoucherKind::Sales { DocKind::Invoice } else { DocKind::CreditNote },
                number: v.number.clone(),
                date: v.date,
                party_name,
                emirate: emirate.clone(),
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
                party_trn: ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty()),
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Vat201Dto {
    pub trn: Option<String>,
    /// The company's emirate code; its Box 1 row holds every standard-rated supply.
    pub emirate: String,
    #[serde(flatten)]
    pub ret: Vat201,
}

/// VAT 201 for a period: boxes and warnings.
pub fn vat201(slot: &Slot, from: &str, to: &str) -> ApiResult<Vat201Dto> {
    let d = collect(open_ref(slot)?, from, to)?;
    let mut ret = returns::vat201(&d.outward, &d.inward, d.period);
    ret.warnings.splice(0..0, d.warnings);
    Ok(Vat201Dto { trn: d.trn, emirate: d.emirate, ret })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, uae_company, voucher};
    use crate::api::{StockLineInput, cancel_voucher, create_party_ledger, post_voucher};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

    const TRN: &str = "100123456700003";
    const SUPPLIER_TRN: &str = "100987654300003";

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

    fn item(slot: &mut Slot, name: &str, unit: &str, class: &str) -> String {
        // Built from JSON so fields added later with defaults don't break it.
        let input: StockItemInput =
            serde_json::from_value(serde_json::json!({ "name": name, "unit_id": unit, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    #[test]
    fn vat201_from_a_small_dubai_company() {
        let (_f, mut slot) = uae_company("vat201");
        assert!(vat201(&slot, "2026-04-01", "2026-06-30").unwrap_err().contains("emirate"));
        set_company_tax(&mut slot, TRN, "DXB").unwrap();
        for (class, percent) in [("STANDARD", "5"), ("ZERO", "0"), ("EXEMPT", "0")] {
            set_tax_rate(&mut slot, class, "2018-01-01", percent).unwrap();
        }
        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let buyer = create_party_ledger(&mut slot, "Abu Dhabi Co", &debtors, "", None, Some(TRN), None).unwrap().ledger.id;
        let supplier =
            create_party_ledger(&mut slot, "Sharjah Supplier", &creditors, "", None, Some(SUPPLIER_TRN), None).unwrap().ledger.id;
        let unregistered = ledger(&mut slot, "Small Trader", "Sundry Creditors", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Input VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "VAT", TaxSide::Input).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &nos, "STANDARD");
        let export = item(&mut slot, "Export kit", &nos, "ZERO");
        let flat = item(&mut slot, "Residential lease", &nos, "EXEMPT");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, "1", amount);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();

        // Standard-rated, zero-rated and exempt lines on one invoice.
        let mut mixed = vec![
            entry(&buyer, DrCr::Dr, "13500"),
            line(&sales, DrCr::Cr, "10000", &laptop),
            entry(&output, DrCr::Cr, "500"),
        ];
        mixed[1].items.extend(items(&export, "1", "2000"));
        mixed[1].items.extend(items(&flat, "1", "1000"));
        mixed[1].amount = "13000".into();
        post(&mut slot, "Sales", "2026-04-10", mixed);
        let cancelled = post(&mut slot, "Sales", "2026-04-11", vec![
            entry(&buyer, DrCr::Dr, "1050"),
            line(&sales, DrCr::Cr, "1000", &laptop),
            entry(&output, DrCr::Cr, "50"),
        ]);
        cancel_voucher(&mut slot, &cancelled.id).unwrap();
        // A sales return.
        post(&mut slot, "CreditNote", "2026-05-02", vec![
            entry(&buyer, DrCr::Cr, "2100"),
            line(&sales, DrCr::Dr, "2000", &laptop),
            entry(&output, DrCr::Dr, "100"),
        ]);
        // Purchases, a purchase return, and VAT from a supplier without a TRN.
        post(&mut slot, "Purchase", "2026-05-05", vec![
            entry(&supplier, DrCr::Cr, "6300"),
            line(&purchase, DrCr::Dr, "6000", &laptop),
            entry(&input, DrCr::Dr, "300"),
        ]);
        post(&mut slot, "DebitNote", "2026-05-12", vec![
            entry(&supplier, DrCr::Dr, "1050"),
            line(&purchase, DrCr::Cr, "1000", &laptop),
            entry(&input, DrCr::Cr, "50"),
        ]);
        post(&mut slot, "Purchase", "2026-05-20", vec![
            entry(&unregistered, DrCr::Cr, "105"),
            entry(&purchase, DrCr::Dr, "100"),
            entry(&input, DrCr::Dr, "5"),
        ]);
        // Outside the quarter.
        post(&mut slot, "Sales", "2026-07-01", vec![
            entry(&buyer, DrCr::Dr, "1050"),
            line(&sales, DrCr::Cr, "1000", &laptop),
            entry(&output, DrCr::Cr, "50"),
        ]);

        let r = vat201(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!((r.trn.as_deref(), r.emirate.as_str()), (Some(TRN), "DXB"));
        let box1: Vec<_> = r.ret.standard_rated.iter().map(|e| (e.box_id, e.amounts.amount, e.amounts.vat)).collect();
        assert_eq!(box1[1], ("1b", dec!(8000), dec!(400)), "Dubai: 10000 − 2000 returned");
        assert!(box1.iter().filter(|b| b.0 != "1b").all(|b| b.1.is_zero() && b.2.is_zero()));
        assert_eq!((r.ret.zero_rated, r.ret.exempt), (dec!(2000), dec!(1000)));
        assert_eq!((r.ret.total_supplies.amount, r.ret.total_due), (dec!(11000), dec!(400)));
        assert_eq!((r.ret.standard_expenses.amount, r.ret.standard_expenses.vat), (dec!(5000), dec!(250)));
        assert_eq!((r.ret.total_recoverable, r.ret.net), (dec!(250), dec!(150)));
        assert!(r.ret.warnings.iter().any(|w| w.contains("no supplier TRN")), "{:?}", r.ret.warnings);
        assert!(!r.ret.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", r.ret.warnings);

        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["standard_rated"][1]["box"].as_str(), j["net"].as_str()), (Some("1b"), Some("150")));
    }

    #[test]
    fn vat201_is_only_for_uae_companies() {
        let (_f, slot) = indian_company("vat201-india");
        assert!(vat201(&slot, "2026-04-01", "2026-06-30").unwrap_err().contains("United Arab Emirates"));
    }

    #[test]
    fn unclassified_lines_are_standard_and_wrong_vat_is_flagged() {
        let (_f, mut slot) = uae_company("vat201-plain");
        set_company_tax(&mut slot, "", "SHJ").unwrap();
        set_tax_rate(&mut slot, "STANDARD", "2018-01-01", "5").unwrap();
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        let services = ledger(&mut slot, "Consulting", "Sales Accounts", "");
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &nos, "STANDARD");
        post_voucher(&mut slot, voucher("Sales", "2026-02-02", vec![
            entry(&walk_in, DrCr::Dr, "1050"),
            entry(&services, DrCr::Cr, "1000"),
            entry(&output, DrCr::Cr, "50"),
        ]))
        .unwrap();
        post_voucher(&mut slot, voucher("Sales", "2026-02-03", vec![entry(&walk_in, DrCr::Dr, "300"), entry(&services, DrCr::Cr, "300")]))
            .unwrap();
        let mut wrong = entry(&services, DrCr::Cr, "1000");
        wrong.items = items(&laptop, "1", "1000");
        post_voucher(&mut slot, voucher("Sales", "2026-02-04", vec![entry(&walk_in, DrCr::Dr, "1080"), wrong, entry(&output, DrCr::Cr, "80")]))
            .unwrap();
        let r = vat201(&slot, "2026-01-01", "2026-03-31").unwrap();
        let sharjah = &r.ret.standard_rated[2];
        assert_eq!((sharjah.box_id, sharjah.amounts.amount, sharjah.amounts.vat), ("1c", dec!(2300), dec!(130)));
        assert!(r.ret.warnings.iter().any(|w| w.contains("without a tax class")), "{:?}", r.ret.warnings);
        assert!(r.ret.warnings.iter().any(|w| w.contains("VAT booked is 80, but the rates on file give 50")), "{:?}", r.ret.warnings);
        assert_eq!(r.ret.warnings.iter().filter(|w| w.contains("tax class")).count(), 1, "only the untaxed voucher");
    }

    #[test]
    fn untaxed_sales_to_parties_abroad_are_exports() {
        let (_f, mut slot) = uae_company("vat201-export");
        set_company_tax(&mut slot, "", "DXB").unwrap();
        let buyer = ledger(&mut slot, "Mumbai Importers", "Sundry Debtors", "");
        let local = ledger(&mut slot, "Deira Trading", "Sundry Debtors", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let abroad: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": "Mumbai Importers",
            "group_id": group_id(&slot, "Sundry Debtors"),
            "mailing": { "country": "IN" },
        }))
        .unwrap();
        crate::masters::alter_ledger(&mut slot, &buyer, &abroad).unwrap();
        post_voucher(&mut slot, voucher("Sales", "2026-02-02", vec![entry(&buyer, DrCr::Dr, "4000"), entry(&sales, DrCr::Cr, "4000")])).unwrap();
        post_voucher(&mut slot, voucher("Sales", "2026-02-03", vec![entry(&local, DrCr::Dr, "1000"), entry(&sales, DrCr::Cr, "1000")])).unwrap();
        let r = vat201(&slot, "2026-01-01", "2026-03-31").unwrap();
        assert_eq!(r.ret.zero_rated, dec!(4000), "the export");
        assert_eq!(r.ret.standard_rated[1].amounts.amount, dec!(1000), "the local sale stays standard-rated (Dubai)");
    }

    /// Alter a ledger from JSON fields (name and group kept by the caller).
    fn alter(slot: &mut Slot, id: &str, fields: serde_json::Value) {
        let input: crate::masters::LedgerInput = serde_json::from_value(fields).unwrap();
        crate::masters::alter_ledger(slot, id, &input).unwrap();
    }

    #[test]
    fn reverse_charge_and_customs_imports_fill_boxes_3_6_and_10() {
        let (_f, mut slot) = uae_company("vat201-rc");
        set_company_tax(&mut slot, TRN, "DXB").unwrap();
        set_tax_rate(&mut slot, "STANDARD", "2018-01-01", "5").unwrap();
        let creditors = group_id(&slot, "Sundry Creditors");
        let aws = ledger(&mut slot, "Cloud Ltd", "Sundry Creditors", "");
        alter(&mut slot, &aws, serde_json::json!({ "name": "Cloud Ltd", "group_id": creditors, "mailing": { "country": "IE" } }));
        let maker = ledger(&mut slot, "Shenzhen Maker", "Sundry Creditors", "");
        alter(&mut slot, &maker, serde_json::json!({ "name": "Shenzhen Maker", "group_id": creditors, "mailing": { "country": "CN" } }));
        let expenses = group_id(&slot, "Indirect Expenses");
        let cloud = ledger(&mut slot, "Cloud services", "Indirect Expenses", "");
        alter(&mut slot, &cloud, serde_json::json!({ "name": "Cloud services", "group_id": expenses, "tax_class": "rc: standard" }));
        let other = ledger(&mut slot, "Consulting abroad", "Indirect Expenses", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Input VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "VAT", TaxSide::Input).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let machine = item(&mut slot, "Machine", &nos, "IMPORT:STANDARD");
        // Self-accounted: Dr input VAT, Cr output VAT.
        post_voucher(&mut slot, voucher("Purchase", "2026-04-05", vec![
            entry(&aws, DrCr::Cr, "2000"),
            entry(&cloud, DrCr::Dr, "2000"),
            entry(&input, DrCr::Dr, "100"),
            entry(&output, DrCr::Cr, "100"),
        ]))
        .unwrap();
        // Not self-accounted in the books: still reported, with a warning.
        post_voucher(&mut slot, voucher("Purchase", "2026-04-06", vec![entry(&aws, DrCr::Cr, "1000"), entry(&cloud, DrCr::Dr, "1000")])).unwrap();
        let mut goods = entry(&purchase, DrCr::Dr, "10000");
        goods.items = items(&machine, "1", "10000");
        post_voucher(&mut slot, voucher("Purchase", "2026-05-01", vec![
            entry(&maker, DrCr::Cr, "10000"),
            goods,
            entry(&input, DrCr::Dr, "500"),
            entry(&output, DrCr::Cr, "500"),
        ]))
        .unwrap();
        // From abroad, no VAT, nothing marked: a reminder.
        post_voucher(&mut slot, voucher("Purchase", "2026-05-02", vec![entry(&maker, DrCr::Cr, "300"), entry(&other, DrCr::Dr, "300")])).unwrap();

        let r = vat201(&slot, "2026-04-01", "2026-06-30").unwrap().ret;
        assert_eq!((r.reverse_charge_supplies.amount, r.reverse_charge_supplies.vat), (dec!(3000), dec!(150)));
        assert_eq!((r.imports.amount, r.imports.vat), (dec!(10000), dec!(500)));
        assert_eq!((r.reverse_charge_recovery.amount, r.reverse_charge_recovery.vat), (dec!(13000), dec!(650)));
        assert_eq!(r.standard_expenses.vat, dec!(0), "no supplier TRN: the unmarked purchase stays out of Box 9");
        assert_eq!((r.total_due, r.total_recoverable, r.net), (dec!(650), dec!(650), dec!(0)));
        let w = &r.warnings;
        assert_eq!(w.iter().filter(|w| w.contains("posts 0 to output VAT")).count(), 1, "{w:?}");
        assert!(w.iter().any(|w| w.contains("Purchase 4") || w.contains("RC: or IMPORT:")), "{w:?}");
        assert!(!w.iter().any(|w| w.contains("rates on file")), "{w:?}");
    }
}

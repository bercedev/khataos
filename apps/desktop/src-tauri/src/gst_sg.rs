//! Singapore GST F5 from the books. The rules live in `khata_sg::returns`;
//! this module turns vouchers into its documents, as `vat_ae.rs` does for
//! the UAE VAT 201.
//!
//! Which vouchers feed which side (KhataOS records a sales return as a
//! Credit Note and a purchase return as a Debit Note):
//! - outward (Boxes 1 to 4, 6 and 13): Sales → invoice, Credit Note →
//!   credit note;
//! - inward (Boxes 5 and 7): Purchase → invoice, Debit Note → the
//!   supplier's credit note (it reduces the input tax claimed).
//!
//! Other voucher types are left out, even when they post to GST ledgers (a
//! warning says so). A voucher's lines split as in `gst.rs`: the party line
//! carries the total; item lines and other ledgers on the opposite side are
//! the value (a ledger named "Round off" excepted); ledgers whose type of
//! tax is GST give the GST.
//!
//! Each line's treatment comes from its stock item's tax class and the rate
//! on file for the voucher date (`khata_sg::gst::treatment`: "EXEMPT…" is
//! exempt, "OUT-OF-SCOPE…" out of scope, any other 0% class zero-rated,
//! else standard). A line without a class or rate is taken as
//! standard-rated. The voucher's GST is spread over its standard-rated
//! lines by value.
//!
//! A sale with no GST to a party whose mailing address is outside
//! Singapore is an export (zero-rated). The rest of the simplifications are
//! listed in `khata_sg::returns`.

use khata_compliance::RateSchedule;
use khata_core::{Book, DrCr, Voucher, VoucherKind};
use khata_sg::gst::{self, Mechanism, Treatment};
use khata_sg::returns::{self, DocKind, DocLine, GstF5, InwardDoc, OutwardDoc, Period};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, group_is_party, open_ref, parse_date};
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

/// Singapore's classes and treatments for [`vat_lines`].
struct SgRules<'a> {
    rates: &'a RateSchedule,
}

impl vat_lines::Rules for SgRules<'_> {
    type Treatment = Treatment;
    type Mechanism = Mechanism;
    const COMPONENT: &'static str = gst::GST;

    fn classify(&self, class: Option<&str>, date: chrono::NaiveDate) -> (Mechanism, Option<(Treatment, Decimal)>) {
        let Some(c) = class else { return (Mechanism::Normal, None) };
        let (m, base) = gst::mechanism(c);
        let key = if m == Mechanism::Normal { c.to_owned() } else { base };
        (m, self.rates.rate(&key, date).map(|r| (gst::treatment(&key, r), r)))
    }
    fn standard(&self) -> Treatment {
        Treatment::Standard
    }
    fn zero_rated(&self) -> Treatment {
        Treatment::ZeroRated
    }
    fn taxed(&self, t: Treatment) -> bool {
        t == Treatment::Standard
    }
    fn self_accounted(&self, m: Mechanism) -> bool {
        m == Mechanism::ReverseCharge
    }
    fn ledger_mechanism(&self, class: &str) -> Option<Mechanism> {
        (gst::mechanism(class).0 == Mechanism::Import).then_some(Mechanism::Import)
    }
}

/// One voucher's lines, with treatments and GST (see `vat_lines`).
fn lines(
    book: &Book,
    v: &Voucher,
    party: Option<usize>,
    rules: &SgRules<'_>,
    export: bool,
    warnings: &mut Vec<String>,
) -> (Vec<DocLine>, Vec<vat_lines::LedgerTax<Mechanism>>) {
    let inward = matches!(v.kind, VoucherKind::Purchase | VoucherKind::DebitNote);
    let split = vat_lines::split(rules, book, v, party, inward, export, warnings);
    let lines = split
        .lines
        .into_iter()
        .map(|l| DocLine { treatment: l.treatment, taxable_value: l.value, gst: l.tax, mechanism: l.mechanism })
        .collect();
    (lines, split.ledger_tax)
}

/// Import GST paid at customs, posted to GST ledgers classed IMPORT on any
/// voucher: a document with one line carrying the GST, for Box 7.
fn import_gst_doc(v: &Voucher, postings: &[vat_lines::LedgerTax<Mechanism>]) -> Option<InwardDoc> {
    let gst: Decimal = postings.iter().filter(|p| p.mechanism == Mechanism::Import).map(|p| p.debit).sum();
    (!gst.is_zero()).then(|| InwardDoc {
        id: format!("{}-import-gst", v.id.0),
        kind: DocKind::Invoice,
        number: v.number.clone(),
        date: v.date,
        party_name: "Singapore Customs".into(),
        party_gst_no: None,
        cancelled: v.cancelled,
        lines: vec![DocLine { treatment: Treatment::Standard, taxable_value: Decimal::ZERO, gst, mechanism: Mechanism::Import }],
    })
}

/// What the books give the return builder.
struct Docs {
    outward: Vec<OutwardDoc>,
    inward: Vec<InwardDoc>,
    period: Period,
    gst_no: Option<String>,
    warnings: Vec<String>,
}

fn collect(book: &Book, from: &str, to: &str) -> ApiResult<Docs> {
    let country = khata_sg::COUNTRY.code;
    if book.country() != Some(country) {
        return Err("GST F5 is for companies in Singapore".into());
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
    let mut d =
        Docs { outward: Vec::new(), inward: Vec::new(), period, gst_no: profile.tax_id.clone(), warnings: Vec::new() };
    if d.gst_no.is_none() {
        d.warnings.push("The company has no GST registration number (Company → Tax registration)".into());
    }
    let rules = SgRules { rates: &rates };
    let mut other_kinds = 0;
    let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| period.contains(v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let outward = match v.kind {
            VoucherKind::Sales | VoucherKind::CreditNote => true,
            VoucherKind::Purchase | VoucherKind::DebitNote => false,
            _ => {
                let postings = vat_lines::ledger_tax(&rules, book, v);
                d.inward.extend(import_gst_doc(v, &postings));
                let taxed = v.entries.iter().any(|e| {
                    book.ledger(e.ledger).is_some_and(|l| {
                        l.tax.is_some() && l.tax_class.as_deref().and_then(|c| vat_lines::Rules::ledger_mechanism(&rules, c)).is_none()
                    })
                });
                other_kinds += usize::from(taxed && !v.cancelled);
                continue;
            }
        };
        let party = party_line(book, v);
        let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
        let party_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        // A sale to a party whose address is outside Singapore, charged no
        // GST, is an export: zero-rated (Box 2).
        let abroad = ledger
            .and_then(|l| l.mailing.as_ref())
            .and_then(|m| m.country.as_deref())
            .is_some_and(|c| !c.trim().is_empty() && !c.trim().eq_ignore_ascii_case(country));
        let (lines, postings) = lines(book, v, party, &rules, outward && abroad, &mut d.warnings);
        d.inward.extend(import_gst_doc(v, &postings));
        if !outward
            && abroad
            && !v.cancelled
            && lines.iter().all(|l| l.mechanism == Mechanism::Normal && l.gst.is_zero())
            && lines.iter().any(|l| l.treatment == Treatment::Standard && !l.taxable_value.is_zero())
        {
            d.warnings.push(format!(
                "{} {} from {party_name} (abroad) has no GST: if it is an imported service or low-value goods under the reverse charge, or goods imported through customs, give its lines an RC: or IMPORT: class",
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
                party_gst_no: ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty()),
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
pub struct GstF5Dto {
    /// The company's GST registration number.
    pub gst_no: Option<String>,
    #[serde(flatten)]
    pub ret: GstF5,
}

/// GST F5 for a period: boxes and warnings.
pub fn gst_f5(slot: &Slot, from: &str, to: &str) -> ApiResult<GstF5Dto> {
    let d = collect(open_ref(slot)?, from, to)?;
    let mut ret = returns::gst_f5(&d.outward, &d.inward, d.period);
    ret.warnings.splice(0..0, d.warnings);
    Ok(GstF5Dto { gst_no: d.gst_no, ret })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, indian_company, ledger, voucher};
    use crate::api::{StockLineInput, cancel_voucher, check_tax_id, create_company, create_party_ledger, post_voucher, set_country};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{InvoiceTaxInput, InvoiceTaxLine, company_tax, invoice_tax, set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

    const GST_NO: &str = "201912345K";
    const SUPPLIER_GST_NO: &str = "M2-1234567-K";

    fn sg_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Pte Ltd").unwrap();
        set_country(&mut slot, Some(&f.0), "SG").unwrap();
        (f, slot)
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

    fn item(slot: &mut Slot, name: &str, unit: &str, class: &str) -> String {
        // Built from JSON so fields added later with defaults don't break it.
        let input: StockItemInput =
            serde_json::from_value(serde_json::json!({ "name": name, "unit_id": unit, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    #[test]
    fn a_singapore_company_has_no_regions_and_charges_nine_percent() {
        let f = TempFile::new("sg");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Pte Ltd").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "sg").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("SG", "SGD", "GST Reg No"));
        assert_eq!(chrono::Datelike::month(&info.fy_start), 1, "calendar years");

        let ok = check_tax_id(&slot, " m2-1234567-k ").unwrap();
        assert_eq!((ok.value.as_str(), ok.error, ok.region, ok.entity_id), ("M21234567K", None, None, None));
        let uen = check_tax_id(&slot, GST_NO).unwrap();
        assert_eq!((uen.error, uen.entity_id.as_deref()), (None, Some(GST_NO)), "a UEN is the entity ID");
        assert!(check_tax_id(&slot, "20191234").unwrap().error.is_some());

        let tax = set_company_tax(&mut slot, GST_NO, "").unwrap();
        assert_eq!((tax.label.as_str(), tax.tax_id.as_deref(), tax.region, tax.regions.len()), ("GST Reg No", Some(GST_NO), None, 0));
        assert!(set_company_tax(&mut slot, GST_NO, "01").is_err(), "there are no regions to pick");
        assert_eq!(company_tax(&slot).unwrap().region, None);
        for (from, percent) in [("2023-01-01", "8"), ("2024-01-01", "9")] {
            set_tax_rate(&mut slot, "STANDARD", from, percent).unwrap();
        }

        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Jurong Co", &debtors, "", None, Some(SUPPLIER_GST_NO), None).unwrap().ledger.id;
        let output = ledger(&mut slot, "Output GST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "GST", TaxSide::Output).unwrap();
        let ask = |date: &str| InvoiceTaxInput {
            kind: VoucherKind::Sales,
            date: date.into(),
            party_id: buyer.clone(),
            lines: vec![InvoiceTaxLine {
                item_id: String::new(),
                ledger_id: String::new(),
                tax_class: "STANDARD".into(),
                taxable_value: "1999.95".into(),
            }],
        };
        // No region is asked for: the country code stands in on both sides.
        let t = invoice_tax(&slot, &ask("2026-05-01")).unwrap();
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.place_of_supply.as_deref(), Some("SG"));
        let rows: Vec<_> =
            t.rows.iter().map(|r| (r.component.as_str(), r.percent.as_str(), r.amount.as_str(), r.ledger_name.as_deref())).collect();
        assert_eq!(rows, [("GST", "9", "180", Some("Output GST"))], "179.9955 rounds to 180.00");
        let old = invoice_tax(&slot, &ask("2023-06-30")).unwrap();
        assert_eq!((old.rows[0].percent.as_str(), old.rows[0].amount.as_str()), ("8", "160"), "the 2023 rate");

        // Invoice numbers are unique across years.
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let sale = |date: &str, number: &str| crate::api::VoucherInput {
            number: Some(number.into()),
            ..voucher("Sales", date, vec![entry(&cash, DrCr::Dr, "100"), entry(&sales, DrCr::Cr, "100")])
        };
        post_voucher(&mut slot, sale("2026-12-31", "TI-1")).unwrap();
        assert!(post_voucher(&mut slot, sale("2027-01-01", "ti-1")).unwrap_err().contains("already used"));
    }

    #[test]
    fn gst_f5_from_a_small_singapore_company() {
        let (_f, mut slot) = sg_company("gst-f5");
        set_company_tax(&mut slot, GST_NO, "").unwrap();
        for (class, from, percent) in
            [("STANDARD", "2024-01-01", "9"), ("ZERO", "1994-04-01", "0"), ("EXEMPT", "1994-04-01", "0"), ("OUT-OF-SCOPE", "1994-04-01", "0")]
        {
            set_tax_rate(&mut slot, class, from, percent).unwrap();
        }
        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let buyer = create_party_ledger(&mut slot, "Tampines Co", &debtors, "", None, Some("T08LL1234A"), None).unwrap().ledger.id;
        let supplier =
            create_party_ledger(&mut slot, "Woodlands Supplier", &creditors, "", None, Some(SUPPLIER_GST_NO), None).unwrap().ledger.id;
        let unregistered = ledger(&mut slot, "Small Trader", "Sundry Creditors", "");
        let overseas = ledger(&mut slot, "Jakarta Importer", "Sundry Debtors", "");
        let abroad: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": "Jakarta Importer",
            "group_id": debtors,
            "mailing": { "country": "ID" },
        }))
        .unwrap();
        crate::masters::alter_ledger(&mut slot, &overseas, &abroad).unwrap();
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let output = ledger(&mut slot, "Output GST", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Input GST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "GST", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "GST", TaxSide::Input).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &nos, "STANDARD");
        let export = item(&mut slot, "Export kit", &nos, "ZERO");
        let flat = item(&mut slot, "Residential lease", &nos, "EXEMPT");
        let ftz = item(&mut slot, "Goods sold in FTZ", &nos, "OUT-OF-SCOPE");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, "1", amount);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();

        // Standard-rated, zero-rated, exempt and out-of-scope lines on one invoice.
        let mut mixed = vec![
            entry(&buyer, DrCr::Dr, "14400"),
            line(&sales, DrCr::Cr, "10000", &laptop),
            entry(&output, DrCr::Cr, "900"),
        ];
        mixed[1].items.extend(items(&export, "1", "2000"));
        mixed[1].items.extend(items(&flat, "1", "1000"));
        mixed[1].items.extend(items(&ftz, "1", "500"));
        mixed[1].amount = "13500".into();
        post(&mut slot, "Sales", "2026-04-10", mixed);
        let cancelled = post(&mut slot, "Sales", "2026-04-11", vec![
            entry(&buyer, DrCr::Dr, "1090"),
            line(&sales, DrCr::Cr, "1000", &laptop),
            entry(&output, DrCr::Cr, "90"),
        ]);
        cancel_voucher(&mut slot, &cancelled.id).unwrap();
        // An untaxed sale to a party in Indonesia: an export.
        post(&mut slot, "Sales", "2026-04-12", vec![entry(&overseas, DrCr::Dr, "3000"), entry(&sales, DrCr::Cr, "3000")]);
        // A sales return.
        post(&mut slot, "CreditNote", "2026-05-02", vec![
            entry(&buyer, DrCr::Cr, "2180"),
            line(&sales, DrCr::Dr, "2000", &laptop),
            entry(&output, DrCr::Dr, "180"),
        ]);
        // Purchases, a purchase return, and GST from a supplier without a GST number.
        post(&mut slot, "Purchase", "2026-05-05", vec![
            entry(&supplier, DrCr::Cr, "6540"),
            line(&purchase, DrCr::Dr, "6000", &laptop),
            entry(&input, DrCr::Dr, "540"),
        ]);
        post(&mut slot, "DebitNote", "2026-05-12", vec![
            entry(&supplier, DrCr::Dr, "1090"),
            line(&purchase, DrCr::Cr, "1000", &laptop),
            entry(&input, DrCr::Cr, "90"),
        ]);
        post(&mut slot, "Purchase", "2026-05-20", vec![
            entry(&unregistered, DrCr::Cr, "109"),
            entry(&purchase, DrCr::Dr, "100"),
            entry(&input, DrCr::Dr, "9"),
        ]);
        // Outside the quarter.
        post(&mut slot, "Sales", "2026-07-01", vec![
            entry(&buyer, DrCr::Dr, "1090"),
            line(&sales, DrCr::Cr, "1000", &laptop),
            entry(&output, DrCr::Cr, "90"),
        ]);

        let r = gst_f5(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!(r.gst_no.as_deref(), Some(GST_NO));
        let f5 = &r.ret;
        assert_eq!(f5.standard_rated, dec!(8000), "Box 1: 10000 − 2000 returned");
        assert_eq!((f5.zero_rated, f5.exempt, f5.total_supplies), (dec!(5000), dec!(1000), dec!(14000)), "Boxes 2–4");
        assert_eq!(f5.out_of_scope, dec!(500));
        assert_eq!((f5.taxable_purchases, f5.output_tax, f5.input_tax), (dec!(5000), dec!(720), dec!(450)), "Boxes 5–7");
        assert_eq!(f5.net_gst, dec!(270), "Box 8");
        assert_eq!(f5.revenue, dec!(14500), "Box 13 estimate");
        assert!(f5.warnings.iter().any(|w| w.contains("no supplier GST registration number")), "{:?}", f5.warnings);
        assert!(!f5.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", f5.warnings);

        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["gst_no"].as_str(), j["net_gst"].as_str(), j["reverse_charge"]["yes"].as_bool()), (Some(GST_NO), Some("270"), Some(false)));
    }

    #[test]
    fn gst_f5_is_only_for_singapore_companies() {
        let (_f, slot) = indian_company("gst-f5-india");
        assert!(gst_f5(&slot, "2026-04-01", "2026-06-30").unwrap_err().contains("Singapore"));
        let (_g, slot) = sg_company("gst-f5-period");
        assert!(gst_f5(&slot, "2026-06-30", "2026-04-01").unwrap_err().contains("ends before"));
        let r = gst_f5(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert!(r.ret.warnings.iter().any(|w| w.contains("no GST registration number")), "{:?}", r.ret.warnings);
    }

    fn alter(slot: &mut Slot, id: &str, fields: serde_json::Value) {
        let input: crate::masters::LedgerInput = serde_json::from_value(fields).unwrap();
        crate::masters::alter_ledger(slot, id, &input).unwrap();
    }

    #[test]
    fn reverse_charge_and_customs_imports() {
        let (_f, mut slot) = sg_company("gst-f5-rc");
        set_company_tax(&mut slot, GST_NO, "").unwrap();
        set_tax_rate(&mut slot, "STANDARD", "2024-01-01", "9").unwrap();
        let creditors = group_id(&slot, "Sundry Creditors");
        let saas = ledger(&mut slot, "Cloud Inc", "Sundry Creditors", "");
        alter(&mut slot, &saas, serde_json::json!({ "name": "Cloud Inc", "group_id": creditors, "mailing": { "country": "US" } }));
        let maker = ledger(&mut slot, "Shenzhen Maker", "Sundry Creditors", "");
        alter(&mut slot, &maker, serde_json::json!({ "name": "Shenzhen Maker", "group_id": creditors, "mailing": { "country": "CN" } }));
        let expenses = group_id(&slot, "Indirect Expenses");
        let software = ledger(&mut slot, "Software", "Indirect Expenses", "");
        alter(&mut slot, &software, serde_json::json!({ "name": "Software", "group_id": expenses, "tax_class": "RC:STANDARD" }));
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let output = ledger(&mut slot, "Output GST", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Input GST", "Duties & Taxes", "");
        let import_gst = ledger(&mut slot, "Import GST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "GST", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "GST", TaxSide::Input).unwrap();
        set_ledger_tax(&mut slot, &import_gst, "GST", TaxSide::Input).unwrap();
        let duties = group_id(&slot, "Duties & Taxes");
        alter(&mut slot, &import_gst, serde_json::json!({ "name": "Import GST", "group_id": duties, "tax_class": "IMPORT" }));
        let bank = ledger(&mut slot, "DBS", "Bank Accounts", "");
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let machine = item(&mut slot, "Machine", &nos, "IMPORT:STANDARD");
        post_voucher(&mut slot, voucher("Purchase", "2026-04-05", vec![
            entry(&saas, DrCr::Cr, "1000"),
            entry(&software, DrCr::Dr, "1000"),
            entry(&input, DrCr::Dr, "90"),
            entry(&output, DrCr::Cr, "90"),
        ]))
        .unwrap();
        let mut goods = entry(&purchase, DrCr::Dr, "15600");
        goods.items = items(&machine, "1", "15600");
        post_voucher(&mut slot, voucher("Purchase", "2026-04-20", vec![entry(&maker, DrCr::Cr, "15600"), goods])).unwrap();
        // The import permit's GST, paid to Singapore Customs through the bank.
        post_voucher(&mut slot, voucher("Payment", "2026-04-21", vec![entry(&import_gst, DrCr::Dr, "1404"), entry(&bank, DrCr::Cr, "1404")])).unwrap();

        let r = gst_f5(&slot, "2026-04-01", "2026-06-30").unwrap().ret;
        assert_eq!(r.standard_rated, dec!(1000), "Box 1: the reverse charge value");
        assert_eq!(r.taxable_purchases, dec!(16600), "Box 5: reverse charge + import");
        assert_eq!((r.output_tax, r.input_tax), (dec!(90), dec!(1494)));
        assert_eq!((r.reverse_charge.yes, r.reverse_charge.amount), (true, dec!(1000)));
        assert!(!r.warnings.iter().any(|w| w.contains("journal or other")), "{:?}", r.warnings);
        assert!(!r.warnings.iter().any(|w| w.contains("posts 0 to output")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("RC business")), "{:?}", r.warnings);
    }
}

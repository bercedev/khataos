//! GSTR-2B reconciliation from the books: the purchase register (Purchase
//! vouchers and purchase returns, as `gst.rs` reads them) against the
//! GSTR-2B JSON downloaded from the GST portal. The rules are in
//! `khata_in::gstr2b`.
//!
//! Each purchase is matched on the supplier's GSTIN and its number: the
//! voucher number, or a bill reference (New Ref) on the supplier's line,
//! which is where the supplier's invoice number usually goes. Purchases up
//! to a year before the period are searched (suppliers report late); only
//! those in the period are reported missing from GSTR-2B.

use chrono::Duration;
use khata_in::gstr2b::{self, BookDoc, Reconciliation, Tolerance};
use khata_in::returns::{Amounts, Period};
use serde::Serialize;

use crate::api::{ApiResult, Slot, open_ref, parse_date};
use crate::gst::collect_period;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Gstr2bDto {
    /// From the file.
    pub gstin: Option<String>,
    pub return_period: Option<String>,
    pub generated: Option<String>,
    /// The company's GSTIN differs from the file's.
    pub other_gstin: bool,
    #[serde(flatten)]
    pub recon: Reconciliation,
}

/// Reconcile a GSTR-2B JSON file with the purchases of `[from, to]`.
/// `days` and `amount` are the tolerances (default 3 days and ₹1).
pub fn gstr2b_reconcile(slot: &Slot, json_text: &str, from: &str, to: &str, days: Option<i64>, amount: Option<&str>) -> ApiResult<Gstr2bDto> {
    let book = open_ref(slot)?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let portal = gstr2b::parse(json_text)?;
    let tol = Tolerance {
        days: days.unwrap_or(Tolerance::default().days).clamp(0, 366),
        amount: match amount.map(str::trim).filter(|a| !a.is_empty()) {
            Some(a) => crate::api::parse_amount(a)?.abs(),
            None => Tolerance::default().amount,
        },
    };
    let docs = collect_period(book, Period { from: period.from - Duration::days(366), to: period.to })?;
    let books: Vec<BookDoc> = docs
        .inward
        .iter()
        .zip(&docs.inward_info)
        .filter(|(d, _)| !d.cancelled)
        .map(|(d, info)| {
            let mut a = Amounts::default();
            for l in &d.lines {
                let o = Amounts::of_line(l);
                a.taxable += o.taxable;
                a.igst += o.igst;
                a.cgst += o.cgst;
                a.sgst += o.sgst;
                a.cess += o.cess;
            }
            BookDoc {
                id: d.id.clone(),
                kind: d.kind,
                number: d.number.clone(),
                references: info.refs.clone(),
                date: d.date,
                supplier_gstin: d.party_gstin.as_ref().map(|g| g.trim().to_ascii_uppercase()),
                supplier_name: d.party_name.clone(),
                value: info.value,
                amounts: a,
            }
        })
        .collect();
    let mut recon = gstr2b::reconcile(&portal, &books, period, tol);
    let company = book.profile().and_then(|p| p.tax_id.clone());
    let other_gstin = matches!((&company, &portal.gstin), (Some(c), Some(f)) if !c.eq_ignore_ascii_case(f));
    if other_gstin {
        recon.warnings.insert(0, format!(
            "this GSTR-2B is for {}, not the company's GSTIN {}",
            portal.gstin.clone().unwrap_or_default(),
            company.unwrap_or_default()
        ));
    }
    Ok(Gstr2bDto { gstin: portal.gstin, return_period: portal.period, generated: portal.generated, other_gstin, recon })
}

/// Read a GSTR-2B JSON file the user picked (desktop; at most 20 MB).
/// `read_text_file` only takes .csv and .txt.
pub fn read_json_file(path: &str) -> ApiResult<String> {
    let p = std::path::Path::new(path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        return Err("pick the GSTR-2B .json file".into());
    }
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > 20 * 1024 * 1024 {
        return Err("the file is larger than 20 MB".into());
    }
    std::fs::read_to_string(p).map_err(|e| format!("cannot read {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher};
    use crate::api::{BillInput, create_party_ledger, post_voucher};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::{BillKind, DrCr, TaxSide};
    use khata_in::gstr2b::Status;
    use rust_decimal_macros::dec;

    #[test]
    fn reconciles_purchases_with_a_gstr2b_file() {
        let (_f, mut slot) = indian_company("gstr2b");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_tax_rate(&mut slot, "8471", "2017-07-01", "18").unwrap();
        let creditors = group_id(&slot, "Sundry Creditors");
        let ka = "29AAGCB7383J1Z4";
        let supplier = create_party_ledger(&mut slot, "KA Supplier", &creditors, "", None, Some(ka), None).unwrap().ledger.id;
        let goods = ledger(&mut slot, "Purchases", "Purchase Accounts", "");
        let igst = ledger(&mut slot, "Input IGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &igst, "IGST", TaxSide::Input).unwrap();
        let purchase = |slot: &mut Slot, date: &str, reference: &str, taxable: &str, tax: &str, total: &str| {
            let mut party = entry(&supplier, DrCr::Cr, total);
            party.bills = vec![BillInput { kind: BillKind::New, reference: reference.into(), amount: total.into(), due: None }];
            post_voucher(slot, voucher("Purchase", date, vec![party, entry(&goods, DrCr::Dr, taxable), entry(&igst, DrCr::Dr, tax)])).unwrap().id
        };
        let matched = purchase(&mut slot, "2026-05-05", "INV/001", "30000", "5400", "35400");
        let short = purchase(&mut slot, "2026-05-06", "INV/002", "10000", "1700", "11700");
        let missing = purchase(&mut slot, "2026-05-20", "INV/009", "100", "18", "118");
        let json = serde_json::json!({
            "data": { "gstin": "27AAPFU0939F1ZV", "rtnprd": "052026", "docdata": { "b2b": [{ "ctin": ka, "trdnm": "KA Supplier", "inv": [
                { "inum": "INV-1", "dt": "05-05-2026", "val": 35400, "rev": "N", "itcavl": "Y", "txval": 30000, "igst": 5400, "cgst": 0, "sgst": 0, "cess": 0 },
                { "inum": "INV-2", "dt": "06-05-2026", "val": 11800, "rev": "N", "itcavl": "Y", "txval": 10000, "igst": 1800, "cgst": 0, "sgst": 0, "cess": 0 },
                { "inum": "INV-5", "dt": "07-05-2026", "val": 1180, "rev": "N", "itcavl": "N", "rsn": "P", "txval": 1000, "igst": 180, "cgst": 0, "sgst": 0, "cess": 0 }
            ] }] } }
        })
        .to_string();
        let r = gstr2b_reconcile(&slot, &json, "2026-05-01", "2026-05-31", None, None).unwrap();
        assert!(!r.other_gstin);
        let status = |id: &str| r.recon.rows.iter().find(|x| x.books.as_ref().is_some_and(|b| b.id == id)).map(|x| x.status);
        assert_eq!(status(&matched), Some(Status::Matched));
        assert_eq!(status(&short), Some(Status::Mismatched));
        assert_eq!(status(&missing), Some(Status::OnlyInBooks));
        let only: Vec<_> = r.recon.rows.iter().filter(|x| x.status == Status::OnlyIn2b).map(|x| x.portal.as_ref().unwrap().number.as_str()).collect();
        assert_eq!(only, ["INV-5"]);
        assert_eq!(r.recon.itc_unavailable.len(), 1);
        let mismatch = r.recon.rows.iter().find(|x| x.status == Status::Mismatched).unwrap();
        assert_eq!(mismatch.differences, ["IGST 1700 in books, 1800 in GSTR-2B"]);
        // A ₹200 tolerance accepts it.
        let r = gstr2b_reconcile(&slot, &json, "2026-05-01", "2026-05-31", None, Some("200")).unwrap();
        assert_eq!(r.recon.summary[0].count, 2);
        assert_eq!(r.recon.summary[0].amounts.igst, dec!(7200));

        let other = json.replace("27AAPFU0939F1ZV", "29AAGCB7383J1Z4");
        assert!(gstr2b_reconcile(&slot, &other, "2026-05-01", "2026-05-31", None, None).unwrap().other_gstin);
        assert!(read_json_file("statement.csv").is_err());
    }
}

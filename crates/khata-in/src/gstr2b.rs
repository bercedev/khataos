//! GSTR-2B: read the statement downloaded from the GST portal (JSON) and
//! reconcile it with the purchase register.
//!
//! **Fields relied on** (`docs/research/india-gst-advanced.md` §3; the
//! layout is the GSTN GSTR-2B API's, as GSPs document it; the portal's JSON
//! download carries the same `docdata` object):
//! - envelope: `gstin`, `rtnprd` (MMYYYY), `gendt`, and `docdata`, found at
//!   the top level or under one or more `data` keys;
//! - `docdata.b2b[]` and `b2ba[]` (amended): `ctin` (supplier GSTIN),
//!   `trdnm` (name), `supprd` (supplier's filing period) and `inv[]`:
//!   `inum`, `dt` (DD-MM-YYYY), `val`, `pos`, `rev` (Y/N reverse charge),
//!   `itcavl` (Y/N), `rsn` (reason ITC is not available), `typ`,
//!   `txval`, `igst`, `cgst`, `sgst`, `cess`, `imsStatus`; b2ba adds
//!   `oinum`, `oidt`;
//! - `docdata.cdnr[]` and `cdnra[]`: as b2b with `nt[]`: `ntnum`, `typ`
//!   (C credit note, D debit note), `dt`, the same amounts and flags;
//! - `docdata.impg[]`: imports of goods: `boenum`, `boedt`, `portcode`,
//!   `refdt`, `isamd`, `txval`, `igst`, `cess` (listed, not matched).
//!
//! Older files put the amounts in an `items[]` list (`num`, `rt`, `txval`,
//! `igst`…) instead of on the invoice; GSTR-2A style names (`idt`, `iamt`,
//! `camt`, `samt`, `csamt`, `itms[].itm_det`) are read too. ISD, e-commerce
//! (`ecom`) and SEZ imports sections are counted but not read.
//!
//! Matching, per supplier GSTIN and document type: invoice numbers are
//! compared normalised (case, spaces and punctuation ignored, leading zeros
//! of each number dropped: "INV/001" = "inv-1"); then the date must be
//! within the tolerance and the taxable value and each tax within the
//! amount tolerance (₹1 by default). A document whose number differs but
//! whose amounts and date agree is paired as a mismatch ("number
//! differs"). None of this is legal advice.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

use crate::returns::{Amounts, DocKind, Period};

/// Which GSTR-2B table a document came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    B2b,
    B2ba,
    Cdnr,
    Cdnra,
}

/// One supplier document in GSTR-2B.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortalDoc {
    pub section: Section,
    pub supplier_gstin: String,
    pub supplier_name: String,
    /// The supplier's GSTR-1 period (MMYYYY), when given.
    pub supplier_period: Option<String>,
    pub kind: DocKind,
    pub number: String,
    pub date: Option<NaiveDate>,
    pub value: Decimal,
    pub amounts: Amounts,
    pub reverse_charge: bool,
    /// `itcavl`: false when the portal says ITC is not available.
    pub itc_available: bool,
    /// `rsn`: why ITC is not available (a portal code, e.g. "P").
    pub reason: Option<String>,
    /// Amended documents: the original number.
    pub original_number: Option<String>,
    pub ims_status: Option<String>,
}

/// One bill of entry (imports of goods).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportDoc {
    pub boe_number: String,
    pub boe_date: Option<NaiveDate>,
    pub port: String,
    pub amended: bool,
    pub taxable: Decimal,
    pub igst: Decimal,
    pub cess: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gstr2b {
    pub gstin: Option<String>,
    /// Return period, MMYYYY.
    pub period: Option<String>,
    pub generated: Option<String>,
    pub docs: Vec<PortalDoc>,
    pub imports: Vec<ImportDoc>,
    pub warnings: Vec<String>,
}

fn find_docdata(v: &Value, depth: usize) -> Option<&Value> {
    let o = v.as_object()?;
    if let Some(d) = o.get("docdata") {
        return Some(d);
    }
    if depth == 0 {
        return None;
    }
    o.get("data").and_then(|d| find_docdata(d, depth - 1))
}

/// The object holding `docdata` (for `gstin`, `rtnprd`, `gendt`).
fn find_envelope(v: &Value, depth: usize) -> Option<&Value> {
    let o = v.as_object()?;
    if o.contains_key("docdata") {
        return Some(v);
    }
    if depth == 0 {
        return None;
    }
    o.get("data").and_then(|d| find_envelope(d, depth - 1))
}

fn text(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match v.get(*k)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn number(v: &Value, keys: &[&str]) -> Option<Decimal> {
    keys.iter().find_map(|k| match v.get(*k)? {
        Value::Number(n) => n.to_string().parse::<Decimal>().ok(),
        Value::String(s) => s.trim().replace(',', "").parse::<Decimal>().ok(),
        _ => None,
    })
}

/// "05-05-2026" (the portal), or "2026-05-05".
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%d-%m-%Y")
        .or_else(|_| NaiveDate::parse_from_str(s, "%d/%m/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .ok()
}

const TAXABLE: &[&str] = &["txval"];
const IGST: &[&str] = &["igst", "iamt"];
const CGST: &[&str] = &["cgst", "camt"];
const SGST: &[&str] = &["sgst", "samt"];
const CESS: &[&str] = &["cess", "csamt"];

fn amounts_of(v: &Value) -> Amounts {
    let get = |k: &[&str]| number(v, k).unwrap_or_default();
    Amounts { taxable: get(TAXABLE), igst: get(IGST), cgst: get(CGST), sgst: get(SGST), cess: get(CESS) }
}

/// The document's amounts: on the document itself, else the sum of its
/// `items[]` (or GSTR-2A style `itms[].itm_det`).
fn doc_amounts(v: &Value) -> Amounts {
    let on_doc = TAXABLE.iter().chain(IGST).chain(CGST).chain(SGST).chain(CESS).any(|k| v.get(*k).is_some());
    if on_doc {
        return amounts_of(v);
    }
    let mut sum = Amounts::default();
    for list in ["items", "itms"] {
        for it in v.get(list).and_then(Value::as_array).into_iter().flatten() {
            sum.add(&amounts_of(it.get("itm_det").unwrap_or(it)));
        }
    }
    sum
}

fn flag(v: &Value, key: &str, default: bool) -> bool {
    match text(v, &[key]).as_deref().map(str::to_ascii_uppercase).as_deref() {
        Some("Y") | Some("YES") => true,
        Some("N") | Some("NO") => false,
        _ => default,
    }
}

/// Read a GSTR-2B JSON file.
pub fn parse(json: &str) -> Result<Gstr2b, String> {
    let root: Value = serde_json::from_str(json.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("this is not a JSON file: {e}"))?;
    let docdata = find_docdata(&root, 4).ok_or(
        "no GSTR-2B documents (docdata) in this file; download the GSTR-2B JSON from the GST portal (Returns → GSTR-2B → Download JSON)",
    )?;
    let env = find_envelope(&root, 4).unwrap_or(&root);
    let mut out = Gstr2b {
        gstin: text(env, &["gstin"]),
        period: text(env, &["rtnprd", "ret_period", "fp"]),
        generated: text(env, &["gendt"]),
        docs: Vec::new(),
        imports: Vec::new(),
        warnings: Vec::new(),
    };
    let mut undated = 0;
    for (key, section) in [("b2b", Section::B2b), ("b2ba", Section::B2ba), ("cdnr", Section::Cdnr), ("cdnra", Section::Cdnra)] {
        let notes = matches!(section, Section::Cdnr | Section::Cdnra);
        for supplier in docdata.get(key).and_then(Value::as_array).into_iter().flatten() {
            let gstin = text(supplier, &["ctin"]).unwrap_or_default().to_ascii_uppercase();
            let name = text(supplier, &["trdnm"]).unwrap_or_default();
            let period = text(supplier, &["supprd"]);
            let list = if notes { "nt" } else { "inv" };
            for d in supplier.get(list).and_then(Value::as_array).into_iter().flatten() {
                let kind = if !notes {
                    DocKind::Invoice
                } else if text(d, &["typ", "ntty"]).is_some_and(|t| t.eq_ignore_ascii_case("D")) {
                    DocKind::DebitNote
                } else {
                    DocKind::CreditNote
                };
                let date = text(d, &["dt", "idt", "nt_dt"]).and_then(|s| parse_date(&s));
                undated += usize::from(date.is_none());
                out.docs.push(PortalDoc {
                    section,
                    supplier_gstin: gstin.clone(),
                    supplier_name: name.clone(),
                    supplier_period: period.clone(),
                    kind,
                    number: text(d, &["inum", "ntnum", "nt_num"]).unwrap_or_default(),
                    date,
                    value: number(d, &["val"]).unwrap_or_default(),
                    amounts: doc_amounts(d),
                    reverse_charge: flag(d, "rev", false),
                    itc_available: flag(d, "itcavl", true),
                    reason: text(d, &["rsn"]),
                    original_number: text(d, &["oinum", "ontnum"]),
                    ims_status: text(d, &["imsStatus"]),
                });
            }
        }
    }
    for b in docdata.get("impg").and_then(Value::as_array).into_iter().flatten() {
        let a = amounts_of(b);
        out.imports.push(ImportDoc {
            boe_number: text(b, &["boenum"]).unwrap_or_default(),
            boe_date: text(b, &["boedt"]).and_then(|s| parse_date(&s)),
            port: text(b, &["portcode", "portcd"]).unwrap_or_default(),
            amended: flag(b, "isamd", false),
            taxable: a.taxable,
            igst: a.igst,
            cess: a.cess,
        });
    }
    if undated > 0 {
        out.warnings.push(format!("{undated} document(s) in the file have no readable date"));
    }
    let skipped: Vec<&str> = ["isd", "isda", "impgsez", "ecom", "ecoma"]
        .into_iter()
        .filter(|k| docdata.get(*k).and_then(Value::as_array).is_some_and(|a| !a.is_empty()))
        .collect();
    if !skipped.is_empty() {
        out.warnings.push(format!("the file's {} section(s) are not reconciled", skipped.join(", ")));
    }
    Ok(out)
}

/// An invoice number for matching: letters and digits only, upper case,
/// with leading zeros dropped from each run of digits.
pub fn normalise_number(n: &str) -> String {
    let mut out = String::new();
    let mut in_digits = false;
    for c in n.chars() {
        if !c.is_ascii_alphanumeric() {
            in_digits = false;
            continue;
        }
        if c.is_ascii_digit() {
            if c == '0' && !in_digits {
                continue;
            }
            in_digits = true;
        } else {
            in_digits = false;
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// A purchase document in the books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BookDoc {
    /// The app's reference (a voucher id).
    pub id: String,
    /// Supplier's point of view: a purchase is an invoice, a purchase
    /// return the supplier's credit note.
    pub kind: DocKind,
    /// The voucher number.
    pub number: String,
    /// Other numbers the supplier's invoice may be under (the bill
    /// references on the supplier's line).
    pub references: Vec<String>,
    pub date: NaiveDate,
    pub supplier_gstin: Option<String>,
    pub supplier_name: String,
    pub value: Option<Decimal>,
    pub amounts: Amounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tolerance {
    /// Days the dates may differ.
    pub days: i64,
    /// Rupees each amount may differ.
    pub amount: Decimal,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self { days: 3, amount: Decimal::ONE }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Status {
    Matched,
    Mismatched,
    /// In GSTR-2B, not in the books.
    OnlyIn2b,
    /// In the books (in the period), not in GSTR-2B.
    OnlyInBooks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchRow {
    pub status: Status,
    pub books: Option<BookDoc>,
    pub portal: Option<PortalDoc>,
    /// What differs ("taxable value 1000 in books, 900 in GSTR-2B").
    pub differences: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusTotal {
    pub status: Status,
    pub count: usize,
    /// GSTR-2B amounts for matched, mismatched and 2B-only rows; book
    /// amounts for book-only rows.
    pub amounts: Amounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reconciliation {
    pub rows: Vec<MatchRow>,
    pub summary: Vec<StatusTotal>,
    /// GSTR-2B documents on which the portal says ITC is not available.
    pub itc_unavailable: Vec<PortalDoc>,
    pub imports: Vec<ImportDoc>,
    pub warnings: Vec<String>,
}

fn doc_class(k: DocKind) -> u8 {
    match k {
        DocKind::Invoice => 0,
        DocKind::CreditNote => 1,
        DocKind::DebitNote => 2,
    }
}

fn differences(b: &BookDoc, p: &PortalDoc, tol: Tolerance) -> Vec<String> {
    let mut out = Vec::new();
    match p.date {
        Some(d) if (d - b.date).num_days().abs() > tol.days => {
            out.push(format!("date {} in books, {} in GSTR-2B", b.date.format("%d-%m-%Y"), d.format("%d-%m-%Y")));
        }
        _ => {}
    }
    for (label, mine, theirs) in [
        ("taxable value", b.amounts.taxable, p.amounts.taxable),
        ("IGST", b.amounts.igst, p.amounts.igst),
        ("CGST", b.amounts.cgst, p.amounts.cgst),
        ("SGST/UTGST", b.amounts.sgst, p.amounts.sgst),
        ("cess", b.amounts.cess, p.amounts.cess),
    ] {
        if (mine - theirs).abs() > tol.amount {
            out.push(format!("{label} {} in books, {} in GSTR-2B", mine.normalize(), theirs.normalize()));
        }
    }
    out
}

fn close(b: &BookDoc, p: &PortalDoc, tol: Tolerance) -> bool {
    let near = |x: Decimal, y: Decimal| (x - y).abs() <= tol.amount;
    p.date.is_none_or(|d| (d - b.date).num_days().abs() <= tol.days)
        && near(b.amounts.taxable, p.amounts.taxable)
        && near(b.amounts.tax(), p.amounts.tax())
}

/// Reconcile GSTR-2B with the books. `books` may reach before the period
/// (suppliers report late); only book documents dated in `period` are
/// reported as missing from GSTR-2B. Book documents without a supplier
/// GSTIN are left out.
pub fn reconcile(portal: &Gstr2b, books: &[BookDoc], period: Period, tol: Tolerance) -> Reconciliation {
    let mut warnings = portal.warnings.clone();
    let books: Vec<&BookDoc> = books.iter().filter(|b| b.supplier_gstin.as_deref().is_some_and(|g| !g.trim().is_empty())).collect();
    // (GSTIN, class, normalised number) → book docs.
    let mut index: BTreeMap<(String, u8, String), Vec<usize>> = BTreeMap::new();
    for (i, b) in books.iter().enumerate() {
        let gstin = b.supplier_gstin.clone().unwrap_or_default().trim().to_ascii_uppercase();
        let mut keys: BTreeSet<String> = b.references.iter().map(|r| normalise_number(r)).collect();
        keys.insert(normalise_number(&b.number));
        for k in keys.into_iter().filter(|k| !k.is_empty()) {
            index.entry((gstin.clone(), doc_class(b.kind), k)).or_default().push(i);
        }
    }
    let mut used = vec![false; books.len()];
    let mut rows = Vec::new();
    let mut unmatched: Vec<&PortalDoc> = Vec::new();
    for p in &portal.docs {
        let key = (p.supplier_gstin.clone(), doc_class(p.kind), normalise_number(&p.number));
        let hit = index.get(&key).and_then(|c| {
            // The closest unused candidate by date.
            c.iter()
                .copied()
                .filter(|i| !used[*i])
                .min_by_key(|i| p.date.map_or(0, |d| (d - books[*i].date).num_days().abs()))
        });
        match hit {
            Some(i) => {
                used[i] = true;
                let diff = differences(books[i], p, tol);
                rows.push(MatchRow {
                    status: if diff.is_empty() { Status::Matched } else { Status::Mismatched },
                    books: Some(books[i].clone()),
                    portal: Some(p.clone()),
                    differences: diff,
                });
            }
            None => unmatched.push(p),
        }
    }
    // Same supplier, same amounts and date, another number.
    for p in unmatched {
        let near = (0..books.len()).find(|i| {
            !used[*i]
                && books[*i].supplier_gstin.as_deref().is_some_and(|g| g.trim().eq_ignore_ascii_case(&p.supplier_gstin))
                && doc_class(books[*i].kind) == doc_class(p.kind)
                && close(books[*i], p, tol)
        });
        match near {
            Some(i) => {
                used[i] = true;
                rows.push(MatchRow {
                    status: Status::Mismatched,
                    books: Some(books[i].clone()),
                    portal: Some(p.clone()),
                    differences: vec![format!("number {} in books, {} in GSTR-2B", books[i].number, p.number)],
                });
            }
            None => rows.push(MatchRow { status: Status::OnlyIn2b, books: None, portal: Some(p.clone()), differences: Vec::new() }),
        }
    }
    for (i, b) in books.iter().enumerate() {
        if !used[i] && period.contains(b.date) {
            rows.push(MatchRow { status: Status::OnlyInBooks, books: Some((*b).clone()), portal: None, differences: Vec::new() });
        }
    }
    rows.sort_by(|a, b| {
        let key = |r: &MatchRow| {
            let date = r.books.as_ref().map(|b| b.date).or_else(|| r.portal.as_ref().and_then(|p| p.date));
            (r.status, date)
        };
        key(a).cmp(&key(b))
    });
    let mut summary: Vec<StatusTotal> = [Status::Matched, Status::Mismatched, Status::OnlyIn2b, Status::OnlyInBooks]
        .into_iter()
        .map(|status| StatusTotal { status, count: 0, amounts: Amounts::default() })
        .collect();
    for r in &rows {
        let s = &mut summary[r.status as usize];
        s.count += 1;
        let (a, kind) = match (&r.portal, &r.books) {
            (Some(p), _) => (p.amounts, p.kind),
            (None, Some(b)) => (b.amounts, b.kind),
            (None, None) => continue,
        };
        let sign = if kind == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
        s.amounts.add(&a.scaled(sign));
    }
    let itc_unavailable: Vec<PortalDoc> = portal.docs.iter().filter(|p| !p.itc_available).cloned().collect();
    if !portal.imports.is_empty() {
        warnings.push(format!(
            "{} bill(s) of entry for imports are listed but not matched with the books",
            portal.imports.len()
        ));
    }
    Reconciliation { rows, summary, itc_unavailable, imports: portal.imports.clone(), warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const KA: &str = "29AAGCB7383J1Z4";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// A GSTR-2B file in the API layout (nested `data`), current format.
    fn sample() -> String {
        serde_json::json!({
            "chksum": "x",
            "data": {
                "gstin": "27AAPFU0939F1ZV",
                "rtnprd": "052026",
                "version": "1.0",
                "gendt": "14-06-2026",
                "docdata": {
                    "b2b": [{
                        "ctin": KA, "trdnm": "KA Supplier", "supprd": "052026", "supfildt": "11-06-2026",
                        "inv": [
                            { "inum": "INV/001", "typ": "R", "dt": "05-05-2026", "val": 35400, "pos": "27", "rev": "N", "itcavl": "Y", "rsn": "", "txval": 30000, "igst": 5400, "cgst": 0, "sgst": 0, "cess": 0 },
                            { "inum": "INV/002", "typ": "R", "dt": "06-05-2026", "val": 11800, "pos": "27", "rev": "N", "itcavl": "Y", "txval": 10000, "igst": 1800, "cgst": 0, "sgst": 0, "cess": 0 },
                            { "inum": "INV/003", "typ": "R", "dt": "07-05-2026", "val": 1180, "pos": "29", "rev": "N", "itcavl": "N", "rsn": "P", "txval": 1000, "igst": 180, "cgst": 0, "sgst": 0, "cess": 0 },
                            { "inum": "X-9", "typ": "R", "dt": "08-05-2026", "val": 5900, "pos": "27", "rev": "N", "itcavl": "Y",
                              "items": [{ "num": 1, "rt": 18, "txval": 5000, "igst": 900, "cgst": 0, "sgst": 0, "cess": 0 }] }
                        ]
                    }],
                    "cdnr": [{
                        "ctin": KA, "trdnm": "KA Supplier",
                        "nt": [{ "ntnum": "CN-1", "typ": "C", "suptyp": "R", "dt": "12-05-2026", "val": 11800, "pos": "27", "rev": "N", "itcavl": "Y", "txval": 10000, "igst": 1800, "cgst": 0, "sgst": 0, "cess": 0 }]
                    }],
                    "impg": [{ "refdt": "10-05-2026", "portcode": "INNSA1", "boenum": "1234567", "boedt": "09-05-2026", "isamd": "N", "txval": 100000, "igst": 18000, "cess": 0 }],
                    "isd": [{ "ctin": "27AAPFU0939F1ZV", "doclist": [] }]
                }
            }
        })
        .to_string()
    }

    fn book(id: &str, kind: DocKind, number: &str, date: NaiveDate, taxable: Decimal, igst: Decimal) -> BookDoc {
        BookDoc {
            id: id.into(),
            kind,
            number: number.into(),
            references: Vec::new(),
            date,
            supplier_gstin: Some(KA.into()),
            supplier_name: "KA Supplier".into(),
            value: None,
            amounts: Amounts { taxable, igst, ..Amounts::default() },
        }
    }

    #[test]
    fn parses_the_portal_layout() {
        let g = parse(&sample()).unwrap();
        assert_eq!((g.gstin.as_deref(), g.period.as_deref()), (Some("27AAPFU0939F1ZV"), Some("052026")));
        assert_eq!(g.docs.len(), 5);
        let first = &g.docs[0];
        assert_eq!((first.number.as_str(), first.date, first.amounts.igst, first.itc_available), ("INV/001", Some(d(2026, 5, 5)), dec!(5400), true));
        assert_eq!((g.docs[2].itc_available, g.docs[2].reason.as_deref()), (false, Some("P")));
        assert_eq!(g.docs[3].amounts.taxable, dec!(5000), "summed from items");
        assert_eq!((g.docs[4].kind, g.docs[4].section), (DocKind::CreditNote, Section::Cdnr));
        assert_eq!((g.imports[0].boe_number.as_str(), g.imports[0].igst), ("1234567", dec!(18000)));
        assert!(g.warnings.iter().any(|w| w.contains("isd")), "{:?}", g.warnings);
        // Top-level docdata and GSTR-2A style names.
        let old = serde_json::json!({ "docdata": { "b2b": [{ "ctin": KA, "inv": [{ "inum": "7", "idt": "01-05-2026", "val": "118",
            "itms": [{ "num": 1, "itm_det": { "txval": 100, "iamt": 18 } }] }] }] } });
        let g = parse(&old.to_string()).unwrap();
        assert_eq!((g.docs[0].amounts.igst, g.docs[0].value, g.docs[0].date), (dec!(18), dec!(118), Some(d(2026, 5, 1))));
        assert!(parse("{}").unwrap_err().contains("docdata"));
        assert!(parse("not json").is_err());
    }

    #[test]
    fn numbers_normalise() {
        assert_eq!(normalise_number("INV/001"), "INV1");
        assert_eq!(normalise_number("inv-1"), "INV1");
        assert_eq!(normalise_number("2026/0010"), "202610");
        assert_eq!(normalise_number("A 0 B"), "AB", "a lone zero run is dropped");
        assert_eq!(normalise_number("10"), "10", "inner zeros stay");
    }

    #[test]
    fn reconciles_against_the_books() {
        let g = parse(&sample()).unwrap();
        let may = Period { from: d(2026, 5, 1), to: d(2026, 5, 31) };
        let mut by_ref = book("p2", DocKind::Invoice, "17", d(2026, 5, 6), dec!(10000), dec!(1800));
        by_ref.references = vec!["inv-2".into()];
        let books = [
            book("p1", DocKind::Invoice, "INV-1", d(2026, 5, 5), dec!(30000), dec!(5400)),
            by_ref,
            // Same invoice, different tax.
            book("p3", DocKind::Invoice, "inv/3", d(2026, 5, 7), dec!(1000), dec!(90)),
            // Different number, same amounts: paired.
            book("p4", DocKind::Invoice, "X9-A", d(2026, 5, 9), dec!(5000), dec!(900)),
            // Only in the books.
            book("p5", DocKind::Invoice, "Z-1", d(2026, 5, 20), dec!(100), dec!(18)),
            // Last year's, not reported missing.
            book("p6", DocKind::Invoice, "OLD", d(2026, 3, 20), dec!(100), dec!(18)),
            // No GSTIN: left out.
            BookDoc { supplier_gstin: None, ..book("p7", DocKind::Invoice, "U", d(2026, 5, 3), dec!(1), dec!(0)) },
        ];
        let r = reconcile(&g, &books, may, Tolerance::default());
        let status = |id: &str| r.rows.iter().find(|x| x.books.as_ref().is_some_and(|b| b.id == id)).map(|x| x.status);
        assert_eq!(status("p1"), Some(Status::Matched));
        assert_eq!(status("p2"), Some(Status::Matched), "matched on the bill reference");
        assert_eq!(status("p3"), Some(Status::Mismatched));
        let p3 = r.rows.iter().find(|x| x.books.as_ref().is_some_and(|b| b.id == "p3")).unwrap();
        assert_eq!(p3.differences, ["IGST 90 in books, 180 in GSTR-2B"]);
        assert_eq!(status("p4"), Some(Status::Mismatched));
        assert_eq!(status("p5"), Some(Status::OnlyInBooks));
        assert_eq!(status("p6"), None);
        assert_eq!(status("p7"), None);
        let only_2b: Vec<_> = r.rows.iter().filter(|x| x.status == Status::OnlyIn2b).map(|x| x.portal.as_ref().unwrap().number.as_str()).collect();
        assert_eq!(only_2b, ["CN-1"]);
        let counts: Vec<_> = r.summary.iter().map(|s| (s.status, s.count)).collect();
        assert_eq!(counts, [(Status::Matched, 2), (Status::Mismatched, 2), (Status::OnlyIn2b, 1), (Status::OnlyInBooks, 1)]);
        assert_eq!(r.summary[2].amounts.igst, dec!(-1800), "credit notes reduce");
        assert_eq!(r.itc_unavailable.len(), 1);
        assert!(r.warnings.iter().any(|w| w.contains("bill(s) of entry")));

        // A date outside the tolerance is a mismatch.
        let late = [book("p1", DocKind::Invoice, "INV-1", d(2026, 5, 15), dec!(30000), dec!(5400))];
        let r = reconcile(&g, &late, may, Tolerance::default());
        assert!(r.rows[0].differences[0].starts_with("date 15-05-2026 in books"), "{:?}", r.rows[0]);
    }
}

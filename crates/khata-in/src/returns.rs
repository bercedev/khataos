//! GST returns: GSTR-1 sections and a GSTR-3B summary worked out from
//! outward and inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its vouchers
//! into documents. What it computes follows `docs/research/india-compliance.md`
//! §5 (CGST s.37, s.39; Rule 59(4) as amended by Notif. 12/2024-CT):
//! - B2B: invoices to registered recipients, invoice-wise and rate-wise;
//! - B2CL: inter-State invoices to unregistered recipients above the
//!   threshold (₹1 lakh from 1 Aug 2024, ₹2.5 lakh before);
//! - B2CS: other supplies to unregistered recipients, consolidated by
//!   place of supply and rate (credit notes net into it);
//! - CDNR / CDNUR: credit and debit notes to registered recipients, and to
//!   unregistered ones where the note is inter-State above the threshold
//!   (assumption: the note's own value is tested, as the original invoice
//!   is not linked) or relates to an export;
//! - exports (place of supply 96), nil-rated supplies (Table 8);
//! - the HSN summary (Table 12) split into B2B and B2C, and documents issued
//!   (Table 13).
//!
//! None of this is legal advice: the figures are a preparation aid and must
//! be checked before filing. [`gstr1_json`] in particular is not validated
//! against the portal schema.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::gstin;

/// The place-of-supply code the portal uses for exports ("other countries").
pub const EXPORT_POS: &str = "96";

/// GST rates a derived rate snaps to: the GST 2.0 slabs (0, 5, 18, 40), the
/// special rates (0.1, 0.25, 1.5, 3), and 12 and 28 for documents dated
/// before 22 Sep 2025.
pub fn standard_rates() -> [Decimal; 10] {
    [0, 10, 25, 150, 300, 500, 1200, 1800, 2800, 4000].map(|r| Decimal::new(r, 2).normalize())
}

/// The B2CL threshold for an invoice date: ₹1 lakh from 1 Aug 2024 (Notif.
/// 12/2024-CT amending Rule 59(4)), ₹2.5 lakh before.
pub fn b2cl_threshold(date: NaiveDate) -> Decimal {
    let from = NaiveDate::from_ymd_opt(2024, 8, 1).expect("valid date");
    if date >= from { Decimal::new(100_000, 0) } else { Decimal::new(250_000, 0) }
}

/// Snap a rate worked out from amounts to the nearest standard rate. The
/// flag is false when no standard rate is within 0.01, and the rate is then
/// the derived one rounded to two places.
pub fn snap_rate(derived: Decimal) -> (Decimal, bool) {
    let tolerance = Decimal::new(1, 2);
    let nearest = standard_rates()
        .into_iter()
        .min_by_key(|r| (derived - r).abs())
        .expect("the list is not empty");
    if (derived - nearest).abs() <= tolerance { (nearest, true) } else { (derived.round_dp(2).normalize(), false) }
}

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply (a sales return or a price reduction).
    CreditNote,
    /// Adds to the supply.
    DebitNote,
}

impl DocKind {
    /// +1 for invoices and debit notes, −1 for credit notes.
    fn sign(self) -> Decimal {
        if self == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE }
    }
}

/// One line of a document. Amounts are positive; the document kind says
/// whether they add or reduce.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DocLine {
    /// HSN or SAC code.
    pub hsn: Option<String>,
    /// Unit quantity code as the portal lists it ("NOS", "KGS"…).
    pub uqc: Option<String>,
    pub qty: Option<Decimal>,
    pub taxable_value: Decimal,
    /// GST rate in percent (the IGST rate, i.e. CGST + SGST together). When
    /// `None` it is worked out from the amounts and snapped to
    /// [`standard_rates`].
    pub rate: Option<Decimal>,
    pub igst: Decimal,
    pub cgst: Decimal,
    /// SGST or UTGST.
    pub sgst: Decimal,
    pub cess: Decimal,
    /// Inward documents only: tax on this line is paid by the recipient on
    /// reverse charge (CGST s.9(3), 9(4)); the tax fields are that tax.
    /// GSTR-3B 3.1(d) and 4(A)(3). Ignored on outward documents.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reverse_charge: bool,
    /// Inward documents only: credit is blocked under s.17(5). Its tax stays
    /// in 4(A) and is reversed in 4(B)(1) (Circular 170/02/2022-GST).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
}

/// A document the business issued: a tax invoice, credit note or debit note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    /// The app's own reference (a voucher id), carried into the sections so
    /// a report can open the document.
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The recipient's GSTIN; `None` for an unregistered recipient.
    pub party_gstin: Option<String>,
    /// Two-digit state code, or [`EXPORT_POS`] for an export.
    pub place_of_supply: String,
    /// The document total as booked (with round-off and other charges);
    /// `None`: the lines' taxable value plus tax.
    pub value: Option<Decimal>,
    /// Cancelled documents only count in Table 13.
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// A document received from a supplier, for input tax credit. `kind` is
/// from the supplier's point of view: invoices and debit notes add credit,
/// credit notes reduce it. A purchase return the business records as its
/// own debit note is the counterpart of the supplier's credit note, so the
/// app maps it to [`DocKind::CreditNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    pub party_gstin: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The return period, both ends inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Period {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Period {
    pub fn contains(&self, d: NaiveDate) -> bool {
        self.from <= d && d <= self.to
    }

    /// The portal's return period, MMYYYY, from the period's last month.
    pub fn fp(&self) -> String {
        format!("{:02}{}", self.to.month(), self.to.year())
    }
}

/// Taxable value and tax.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Amounts {
    pub taxable: Decimal,
    pub igst: Decimal,
    pub cgst: Decimal,
    /// SGST or UTGST.
    pub sgst: Decimal,
    pub cess: Decimal,
}

impl Amounts {
    pub(crate) fn add(&mut self, o: &Amounts) {
        self.taxable += o.taxable;
        self.igst += o.igst;
        self.cgst += o.cgst;
        self.sgst += o.sgst;
        self.cess += o.cess;
    }

    pub(crate) fn scaled(&self, by: Decimal) -> Amounts {
        Amounts {
            taxable: self.taxable * by,
            igst: self.igst * by,
            cgst: self.cgst * by,
            sgst: self.sgst * by,
            cess: self.cess * by,
        }
    }

    /// IGST + CGST + SGST + cess.
    pub fn tax(&self) -> Decimal {
        self.igst + self.cgst + self.sgst + self.cess
    }

    pub fn of_line(l: &DocLine) -> Amounts {
        Amounts { taxable: l.taxable_value, igst: l.igst, cgst: l.cgst, sgst: l.sgst, cess: l.cess }
    }
}

/// A document's lines at one rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RateItem {
    pub rate: Decimal,
    pub amounts: Amounts,
}

/// A document as reported invoice-wise (B2B, B2CL, CDNR, CDNUR, exports).
/// Amounts are as on the document (positive for credit notes too).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReturnDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    pub party_gstin: Option<String>,
    pub place_of_supply: String,
    pub value: Decimal,
    /// Rate-wise lines; nil-rated lines are left out (they go to Table 8),
    /// except on exports.
    pub items: Vec<RateItem>,
    /// CDNUR and exports: "B2CL", "EXPWP" (with payment of IGST) or "EXPWOP".
    pub typ: Option<String>,
}

impl ReturnDoc {
    pub fn totals(&self) -> Amounts {
        let mut t = Amounts::default();
        for i in &self.items {
            t.add(&i.amounts);
        }
        t
    }
}

/// B2CS: supplies to unregistered recipients by place of supply and rate.
/// Credit notes are netted in, so a row can be negative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct B2csRow {
    pub inter_state: bool,
    pub place_of_supply: String,
    pub rate: Decimal,
    pub amounts: Amounts,
}

/// Table 8: nil-rated supplies. The books do not tell nil-rated, exempt and
/// non-GST supplies apart, so every 0% line is reported as nil-rated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NilRow {
    pub inter_state: bool,
    pub registered: bool,
    pub value: Decimal,
}

/// Table 12: one HSN summary row. Credit notes reduce it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HsnRow {
    /// Empty when the lines had no HSN/SAC code (a warning says so).
    pub hsn: String,
    pub uqc: String,
    pub qty: Decimal,
    pub rate: Decimal,
    pub amounts: Amounts,
}

/// Table 13: one series of one document type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocIssueRow {
    /// The portal's document-type number (1 invoices for outward supply,
    /// 4 debit notes, 5 credit notes).
    pub doc_num: u8,
    pub doc_type: String,
    pub from: String,
    pub to: String,
    pub total: u32,
    pub cancelled: u32,
    pub net_issued: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    B2b,
    B2cl,
    B2cs,
    Cdnr,
    Cdnur,
    Exp,
    Nil,
    HsnB2b,
    HsnB2c,
    Docs,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::B2b => "B2B invoices (4A, 4B)",
            Section::B2cl => "B2C large invoices (5)",
            Section::B2cs => "B2C small (7)",
            Section::Cdnr => "Credit/debit notes, registered (9B)",
            Section::Cdnur => "Credit/debit notes, unregistered (9B)",
            Section::Exp => "Exports (6A)",
            Section::Nil => "Nil-rated supplies (8)",
            Section::HsnB2b => "HSN summary, B2B (12)",
            Section::HsnB2c => "HSN summary, B2C (12)",
            Section::Docs => "Documents issued (13)",
        }
    }
}

/// One line of the section summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionTotal {
    pub section: Section,
    pub label: String,
    /// Documents (rows for B2CS, Table 8, Table 12; documents for Table 13).
    pub count: usize,
    pub amounts: Amounts,
}

/// GSTR-1 for a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gstr1 {
    pub period: Period,
    pub fp: String,
    pub supplier_state: String,
    pub b2b: Vec<ReturnDoc>,
    pub b2cl: Vec<ReturnDoc>,
    pub b2cs: Vec<B2csRow>,
    pub cdnr: Vec<ReturnDoc>,
    pub cdnur: Vec<ReturnDoc>,
    pub exp: Vec<ReturnDoc>,
    pub nil: Vec<NilRow>,
    pub hsn_b2b: Vec<HsnRow>,
    pub hsn_b2c: Vec<HsnRow>,
    pub docs_issued: Vec<DocIssueRow>,
    pub summary: Vec<SectionTotal>,
    /// All outward supplies of the period, net of credit notes.
    pub net: Amounts,
    pub warnings: Vec<String>,
}

/// A line with its rate settled.
struct Resolved {
    hsn: Option<String>,
    uqc: Option<String>,
    qty: Decimal,
    rate: Decimal,
    amounts: Amounts,
}

fn label(kind: DocKind, number: &str) -> String {
    let k = match kind {
        DocKind::Invoice => "Invoice",
        DocKind::CreditNote => "Credit note",
        DocKind::DebitNote => "Debit note",
    };
    format!("{k} {number}")
}

/// Settle each line's rate: the one given, else tax ÷ taxable value
/// snapped to a standard rate.
fn resolve(doc: &str, lines: &[DocLine], warnings: &mut Vec<String>) -> Vec<Resolved> {
    let untaxed = lines.iter().all(|l| l.rate.is_none() && Amounts::of_line(l).tax().is_zero());
    if untaxed && !lines.is_empty() {
        warnings.push(format!(
            "{doc} has no tax lines; it is reported as nil-rated (check whether it is nil-rated, exempt or non-GST)"
        ));
    }
    lines
        .iter()
        .map(|l| {
            let a = Amounts::of_line(l);
            let rate = l.rate.unwrap_or_else(|| {
                let gst = (a.igst + a.cgst + a.sgst).abs();
                if a.taxable.is_zero() {
                    return Decimal::ZERO;
                }
                let (rate, snapped) = snap_rate(gst / a.taxable.abs() * Decimal::ONE_HUNDRED);
                if !snapped {
                    warnings.push(format!("{doc}: tax works out to {rate}%, which is not a standard GST rate"));
                }
                rate
            });
            let hsn = l.hsn.as_deref().map(str::trim).filter(|h| !h.is_empty()).map(str::to_owned);
            Resolved { hsn, uqc: l.uqc.clone(), qty: l.qty.unwrap_or_default(), rate, amounts: a }
        })
        .collect()
}

fn rate_items<'a>(lines: impl Iterator<Item = &'a Resolved>) -> Vec<RateItem> {
    let mut by_rate: BTreeMap<Decimal, Amounts> = BTreeMap::new();
    for l in lines {
        by_rate.entry(l.rate).or_default().add(&l.amounts);
    }
    by_rate.into_iter().map(|(rate, amounts)| RateItem { rate, amounts }).collect()
}

/// Work out GSTR-1 from the outward documents. Documents outside the period
/// are ignored; cancelled ones only count in Table 13.
pub fn gstr1(docs: &[OutwardDoc], period: Period, supplier_state: &str) -> Gstr1 {
    let mut warnings = Vec::new();
    if period.from.year() != period.to.year() || period.from.month() != period.to.month() {
        warnings.push(format!("The period spans more than one month; the return period is taken as {}", period.fp()));
    }
    let mut r = Gstr1 {
        period,
        fp: period.fp(),
        supplier_state: supplier_state.to_owned(),
        b2b: Vec::new(),
        b2cl: Vec::new(),
        b2cs: Vec::new(),
        cdnr: Vec::new(),
        cdnur: Vec::new(),
        exp: Vec::new(),
        nil: Vec::new(),
        hsn_b2b: Vec::new(),
        hsn_b2c: Vec::new(),
        docs_issued: Vec::new(),
        summary: Vec::new(),
        net: Amounts::default(),
        warnings: Vec::new(),
    };
    let mut b2cs: BTreeMap<(bool, String, Decimal), Amounts> = BTreeMap::new();
    let mut nil: BTreeMap<(bool, bool), Decimal> = BTreeMap::new();
    let mut hsn: BTreeMap<(bool, String, String, Decimal), (Decimal, Amounts)> = BTreeMap::new();
    let mut in_period: Vec<&OutwardDoc> = docs.iter().filter(|d| period.contains(d.date)).collect();
    in_period.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));

    for d in in_period.iter().filter(|d| !d.cancelled) {
        let name = label(d.kind, &d.number);
        let gstin = d.party_gstin.as_deref().map(str::trim).filter(|g| !g.is_empty()).map(gstin::normalise);
        if let Some(g) = &gstin
            && let Err(e) = gstin::validate(g)
        {
            warnings.push(format!("{name}: the recipient's GSTIN {g} is invalid ({e})"));
        }
        let registered = gstin.is_some();
        let export = d.place_of_supply == EXPORT_POS;
        let inter = export || d.place_of_supply != supplier_state;
        let lines = resolve(&name, &d.lines, &mut warnings);
        if d.lines.is_empty() {
            warnings.push(format!("{name} has no lines"));
        }
        if lines.iter().any(|l| l.hsn.is_none()) {
            warnings.push(format!("{name} has a line without an HSN/SAC code"));
        }
        let mut sum = Amounts::default();
        for l in &lines {
            sum.add(&l.amounts);
        }
        if inter && !(sum.cgst.is_zero() && sum.sgst.is_zero()) {
            warnings.push(format!("{name} is inter-State by place of supply but charges CGST/SGST"));
        } else if !inter && !sum.igst.is_zero() {
            warnings.push(format!("{name} is within the State by place of supply but charges IGST"));
        }
        let value = d.value.unwrap_or(sum.taxable + sum.tax());
        let sign = d.kind.sign();
        r.net.add(&sum.scaled(sign));

        // Table 12 takes every line; Table 8 the nil-rated ones (not exports).
        for l in &lines {
            let key = (
                registered,
                l.hsn.clone().unwrap_or_default(),
                l.uqc.clone().unwrap_or_else(|| "NA".into()),
                l.rate,
            );
            let e = hsn.entry(key).or_default();
            e.0 += l.qty * sign;
            e.1.add(&l.amounts.scaled(sign));
            if !export && l.rate.is_zero() {
                *nil.entry((inter, registered)).or_default() += l.amounts.taxable * sign;
            }
        }
        let taxed: Vec<&Resolved> = lines.iter().filter(|l| export || !l.rate.is_zero()).collect();
        if taxed.is_empty() {
            continue;
        }
        let large = inter && !export && value > b2cl_threshold(d.date);
        let export_typ = if sum.igst.is_zero() { "EXPWOP" } else { "EXPWP" };
        let doc = |typ: Option<&str>| ReturnDoc {
            id: d.id.clone(),
            kind: d.kind,
            number: d.number.clone(),
            date: d.date,
            party_name: d.party_name.clone(),
            party_gstin: gstin.clone(),
            place_of_supply: d.place_of_supply.clone(),
            value,
            items: rate_items(taxed.iter().copied()),
            typ: typ.map(str::to_owned),
        };
        match (d.kind == DocKind::Invoice, registered && !export, export, large) {
            (true, _, true, _) => r.exp.push(doc(Some(export_typ))),
            (true, true, _, _) => r.b2b.push(doc(None)),
            (true, false, _, true) => r.b2cl.push(doc(None)),
            (false, _, true, _) => r.cdnur.push(doc(Some(export_typ))),
            (false, true, _, _) => r.cdnr.push(doc(None)),
            (false, false, _, true) => r.cdnur.push(doc(Some("B2CL"))),
            (_, false, false, false) => {
                for l in &taxed {
                    b2cs.entry((inter, d.place_of_supply.clone(), l.rate))
                        .or_default()
                        .add(&l.amounts.scaled(sign));
                }
            }
        }
    }

    r.b2cs = b2cs
        .into_iter()
        .map(|((inter_state, place_of_supply, rate), amounts)| B2csRow { inter_state, place_of_supply, rate, amounts })
        .collect();
    for row in r.b2cs.iter().filter(|b| b.amounts.taxable.is_sign_negative()) {
        warnings.push(format!(
            "B2C small for state {} at {}% is negative after credit notes; the portal may not accept it",
            row.place_of_supply, row.rate
        ));
    }
    r.nil = nil
        .into_iter()
        .filter(|(_, v)| !v.is_zero())
        .map(|((inter_state, registered), value)| NilRow { inter_state, registered, value })
        .collect();
    for ((registered, hsn, uqc, rate), (qty, amounts)) in hsn {
        let row = HsnRow { hsn, uqc, qty, rate, amounts };
        if registered { r.hsn_b2b.push(row) } else { r.hsn_b2c.push(row) }
    }
    r.docs_issued = docs_issued(&in_period, &mut warnings);
    r.summary = summary(&r);
    r.warnings = warnings;
    r
}

fn doc_type(kind: DocKind) -> (u8, &'static str) {
    match kind {
        DocKind::Invoice => (1, "Invoices for outward supply"),
        DocKind::DebitNote => (4, "Debit Note"),
        DocKind::CreditNote => (5, "Credit Note"),
    }
}

/// "26-27/12" → ("26-27/", Some(12)); a number without trailing digits is
/// its own series.
fn split_number(n: &str) -> (&str, Option<u64>) {
    let prefix = n.trim_end_matches(|c: char| c.is_ascii_digit());
    (prefix, n[prefix.len()..].parse().ok())
}

/// Table 13: each document type's series, from the lowest to the highest
/// running number, with how many were cancelled.
fn docs_issued(docs: &[&OutwardDoc], warnings: &mut Vec<String>) -> Vec<DocIssueRow> {
    struct Series<'a> {
        low: (u64, &'a str),
        high: (u64, &'a str),
        total: u32,
        cancelled: u32,
    }
    let mut series: BTreeMap<(u8, &str), Series> = BTreeMap::new();
    for d in docs {
        let (num, _) = doc_type(d.kind);
        let (prefix, running) = split_number(&d.number);
        let running = running.unwrap_or(0);
        let s = series.entry((num, prefix)).or_insert(Series {
            low: (running, &d.number),
            high: (running, &d.number),
            total: 0,
            cancelled: 0,
        });
        if running < s.low.0 {
            s.low = (running, &d.number);
        }
        if running > s.high.0 {
            s.high = (running, &d.number);
        }
        s.total += 1;
        s.cancelled += u32::from(d.cancelled);
    }
    let names: BTreeMap<u8, &str> = [DocKind::Invoice, DocKind::DebitNote, DocKind::CreditNote]
        .into_iter()
        .map(doc_type)
        .collect();
    series
        .into_iter()
        .map(|((doc_num, _), s)| {
            let span = s.high.0 - s.low.0 + 1;
            if span > u64::from(s.total) {
                warnings.push(format!(
                    "{} {} to {}: {} number(s) in between are missing; Table 13 expects them reported as cancelled",
                    names[&doc_num],
                    s.low.1,
                    s.high.1,
                    span - u64::from(s.total)
                ));
            }
            DocIssueRow {
                doc_num,
                doc_type: names[&doc_num].to_owned(),
                from: s.low.1.to_owned(),
                to: s.high.1.to_owned(),
                total: s.total,
                cancelled: s.cancelled,
                net_issued: s.total - s.cancelled,
            }
        })
        .collect()
}

fn summary(r: &Gstr1) -> Vec<SectionTotal> {
    let docs = |section: Section, docs: &[ReturnDoc]| {
        let mut amounts = Amounts::default();
        for d in docs {
            amounts.add(&d.totals());
        }
        SectionTotal { section, label: section.label().into(), count: docs.len(), amounts }
    };
    let rows = |section: Section, count: usize, list: &mut dyn Iterator<Item = Amounts>| {
        let mut amounts = Amounts::default();
        for a in list {
            amounts.add(&a);
        }
        SectionTotal { section, label: section.label().into(), count, amounts }
    };
    vec![
        docs(Section::B2b, &r.b2b),
        docs(Section::B2cl, &r.b2cl),
        rows(Section::B2cs, r.b2cs.len(), &mut r.b2cs.iter().map(|b| b.amounts)),
        docs(Section::Cdnr, &r.cdnr),
        docs(Section::Cdnur, &r.cdnur),
        docs(Section::Exp, &r.exp),
        rows(Section::Nil, r.nil.len(), &mut r.nil.iter().map(|n| Amounts { taxable: n.value, ..Amounts::default() })),
        rows(Section::HsnB2b, r.hsn_b2b.len(), &mut r.hsn_b2b.iter().map(|h| h.amounts)),
        rows(Section::HsnB2c, r.hsn_b2c.len(), &mut r.hsn_b2c.iter().map(|h| h.amounts)),
        SectionTotal {
            section: Section::Docs,
            label: Section::Docs.label().into(),
            count: r.docs_issued.iter().map(|d| d.total as usize).sum(),
            amounts: Amounts::default(),
        },
    ]
}

// ------------------------------------------------------------------ GSTR-3B

/// Table 3.2: inter-State supplies to unregistered persons, by place of supply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaceRow {
    pub place_of_supply: String,
    pub taxable: Decimal,
    pub igst: Decimal,
}

/// Output tax, input tax credit and their difference for one component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetRow {
    pub component: String,
    /// Output tax: 3.1(a) + 3.1(b), plus tax on reverse charge (3.1(d)).
    pub output: Decimal,
    /// Net ITC available, 4(C).
    pub itc: Decimal,
    /// Output − ITC. Negative: more credit than tax. No set-off order
    /// between IGST, CGST and SGST credit (CGST s.49, 49A, 49B; Rule 88A)
    /// is applied, and tax on reverse charge, which is paid in cash, is
    /// netted like the rest; see `reverse_charge`.
    pub net: Decimal,
    /// The part of `output` that is tax on reverse charge (3.1(d)). It is
    /// paid in cash: credit cannot pay it (s.49(4) with s.2(82)).
    pub reverse_charge: Decimal,
}

/// GSTR-3B for a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gstr3b {
    pub period: Period,
    /// 3.1(a): outward taxable supplies other than zero-rated, nil-rated and exempt.
    pub outward_taxable: Amounts,
    /// 3.1(b): zero-rated supplies (exports).
    pub zero_rated: Amounts,
    /// 3.1(c): nil-rated and exempt supplies (taxable value only).
    pub nil_exempt: Amounts,
    /// 3.1(d): inward supplies liable to reverse charge, from registered and
    /// unregistered suppliers alike.
    pub reverse_charge_inward: Amounts,
    /// 3.2: inter-State supplies to unregistered persons.
    pub inter_state_unregistered: Vec<PlaceRow>,
    /// 4(A)(3): ITC on inward supplies liable to reverse charge, taken as
    /// the tax paid on them in the same period [UNCERTAIN: credit follows
    /// payment of that tax].
    pub itc_reverse_charge: Amounts,
    /// 4(A)(5): all other ITC, from forward-charge documents of registered
    /// suppliers, net of supplier credit notes. Blocked credit is included
    /// here and reversed in 4(B)(1), as GSTR-2B fills 4(A) with it
    /// (Circular 170/02/2022-GST para 4.3(A), (B)).
    pub itc: Amounts,
    /// 4(B)(1): reversals that are not reclaimable; here the s.17(5)
    /// blocked credit in 4(A). Rule 38, 42 and 43 reversals are added by
    /// the user (see `rcm::rule42`).
    pub itc_reversed_permanent: Amounts,
    /// 4(B)(2): other reversals, reclaimable later; here Rule 37 (supplier
    /// unpaid after 180 days), filled by [`with_rule37`].
    pub itc_reversed_other: Amounts,
    /// 4(C): net ITC available = 4(A) − 4(B)(1) − 4(B)(2).
    pub itc_net: Amounts,
    pub net: Vec<NetRow>,
    pub warnings: Vec<String>,
}

fn minus(a: &Amounts, b: &Amounts) -> Amounts {
    let mut o = *a;
    o.add(&b.scaled(Decimal::NEGATIVE_ONE));
    o
}

fn net_rows(output: &Amounts, reverse_charge: &Amounts, itc: &Amounts) -> Vec<NetRow> {
    [
        ("IGST", output.igst, reverse_charge.igst, itc.igst),
        ("CGST", output.cgst, reverse_charge.cgst, itc.cgst),
        ("SGST/UTGST", output.sgst, reverse_charge.sgst, itc.sgst),
        ("Cess", output.cess, reverse_charge.cess, itc.cess),
    ]
    .into_iter()
    .map(|(c, output, rc, itc)| NetRow { component: c.into(), output, itc, net: output - itc, reverse_charge: rc })
    .collect()
}

impl Gstr3b {
    fn settle(&mut self) {
        let mut a = self.itc;
        a.add(&self.itc_reverse_charge);
        let mut reversed = self.itc_reversed_permanent;
        reversed.add(&self.itc_reversed_other);
        self.itc_net = minus(&a, &reversed);
        let mut output = self.outward_taxable;
        output.add(&self.zero_rated);
        output.add(&self.reverse_charge_inward);
        self.net = net_rows(&output, &self.reverse_charge_inward, &self.itc_net);
    }
}

/// Add Rule 37 reversals (4(B)(2)) and work out 4(C) and the net again.
pub fn with_rule37(mut g: Gstr3b, reversals: &Amounts) -> Gstr3b {
    g.itc_reversed_other.add(reversals);
    g.settle();
    g
}

/// Work out GSTR-3B. Tables 3.1 and 3.2 come from the same classification
/// as [`gstr1`], so they agree with it (from the July 2025 period the
/// portal fills them from GSTR-1 and locks them). Inward lines flagged
/// [`DocLine::reverse_charge`] go to 3.1(d) and 4(A)(3) (supplier
/// registered or not); other inward lines from registered suppliers to
/// 4(A)(5); lines flagged [`DocLine::blocked`] are reversed in 4(B)(1).
/// Imports (4(A)(1), (2)), ISD credit (4(A)(4)), Rule 38/42/43 reversals
/// and 4(D) are not worked out; Rule 37 comes in through [`with_rule37`].
pub fn gstr3b(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period, supplier_state: &str) -> Gstr3b {
    let g1 = gstr1(outward, period, supplier_state);
    let mut warnings = g1.warnings.clone();
    let signed = |d: &ReturnDoc| d.totals().scaled(d.kind.sign());

    let mut outward_taxable = Amounts::default();
    let mut zero_rated = Amounts::default();
    let mut places: BTreeMap<String, (Decimal, Decimal)> = BTreeMap::new();
    let mut place = |pos: &str, a: &Amounts| {
        let e = places.entry(pos.to_owned()).or_default();
        e.0 += a.taxable;
        e.1 += a.igst;
    };
    for d in g1.b2b.iter().chain(&g1.cdnr) {
        outward_taxable.add(&signed(d));
    }
    for d in g1.b2cl.iter().chain(&g1.cdnur) {
        if d.typ.as_deref().is_some_and(|t| t.starts_with("EXP")) {
            zero_rated.add(&signed(d));
        } else {
            outward_taxable.add(&signed(d));
            place(&d.place_of_supply, &signed(d));
        }
    }
    for b in &g1.b2cs {
        outward_taxable.add(&b.amounts);
        if b.inter_state {
            place(&b.place_of_supply, &b.amounts);
        }
    }
    for d in &g1.exp {
        zero_rated.add(&signed(d));
    }
    let nil_exempt = Amounts { taxable: g1.nil.iter().map(|n| n.value).sum(), ..Amounts::default() };

    let mut itc = Amounts::default();
    let mut reverse_charge_inward = Amounts::default();
    let mut itc_reverse_charge = Amounts::default();
    let mut blocked = Amounts::default();
    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let registered = d.party_gstin.as_deref().is_some_and(|g| !g.trim().is_empty());
        let mut dropped = Amounts::default();
        for l in &d.lines {
            let a = Amounts::of_line(l).scaled(d.kind.sign());
            if l.reverse_charge {
                reverse_charge_inward.add(&a);
                itc_reverse_charge.add(&a);
            } else if registered {
                itc.add(&a);
            } else {
                dropped.add(&a);
                continue;
            }
            if l.blocked {
                blocked.add(&a);
            }
        }
        if !dropped.tax().is_zero() {
            warnings.push(format!(
                "Purchase {} from {} has tax but no supplier GSTIN; its tax is left out of ITC",
                d.number, d.party_name
            ));
        }
    }
    // A reversal has no taxable value of its own.
    let itc_reversed_permanent = Amounts { taxable: Decimal::ZERO, ..blocked };
    let mut g = Gstr3b {
        period,
        outward_taxable,
        zero_rated,
        nil_exempt,
        reverse_charge_inward,
        inter_state_unregistered: places
            .into_iter()
            .map(|(place_of_supply, (taxable, igst))| PlaceRow { place_of_supply, taxable, igst })
            .collect(),
        itc_reverse_charge,
        itc,
        itc_reversed_permanent,
        itc_reversed_other: Amounts::default(),
        itc_net: Amounts::default(),
        net: Vec::new(),
        warnings,
    };
    g.settle();
    g
}

// ------------------------------------------------------------- JSON export

fn num(d: Decimal) -> Value {
    serde_json::Number::from_f64(d.round_dp(2).to_f64().unwrap_or_default()).map_or(Value::Null, Value::Number)
}

fn portal_date(d: NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

fn items(doc: &ReturnDoc, intra_fields: bool) -> Value {
    doc.items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let a = &it.amounts;
            let mut det = Map::new();
            det.insert("txval".into(), num(a.taxable));
            det.insert("rt".into(), num(it.rate));
            det.insert("iamt".into(), num(a.igst));
            if intra_fields {
                det.insert("camt".into(), num(a.cgst));
                det.insert("samt".into(), num(a.sgst));
            }
            det.insert("csamt".into(), num(a.cess));
            json!({ "num": i + 1, "itm_det": det })
        })
        .collect()
}

/// Group invoice-wise documents by a key, keeping first-seen order.
fn grouped(docs: &[ReturnDoc], key: impl Fn(&ReturnDoc) -> String) -> Vec<(String, Vec<&ReturnDoc>)> {
    let mut out: Vec<(String, Vec<&ReturnDoc>)> = Vec::new();
    for d in docs {
        let k = key(d);
        match out.iter_mut().find(|(g, _)| *g == k) {
            Some((_, v)) => v.push(d),
            None => out.push((k, vec![d])),
        }
    }
    out
}

fn note_type(kind: DocKind) -> &'static str {
    if kind == DocKind::CreditNote { "C" } else { "D" }
}

fn hsn_rows(rows: &[HsnRow]) -> Value {
    rows.iter()
        .enumerate()
        .map(|(i, h)| {
            json!({
                "num": i + 1,
                "hsn_sc": h.hsn,
                "desc": "",
                "uqc": h.uqc,
                "qty": num(h.qty),
                "rt": num(h.rate),
                "txval": num(h.amounts.taxable),
                "iamt": num(h.amounts.igst),
                "camt": num(h.amounts.cgst),
                "samt": num(h.amounts.sgst),
                "csamt": num(h.amounts.cess),
            })
        })
        .collect()
}

/// GSTR-1 in the shape of the GST portal's offline-tool JSON, as far as it
/// is publicly documented. The research marks the exact schema UNCERTAIN
/// (`docs/research/india-compliance.md` §5): this output is NOT validated
/// against the official schema and must be checked with the GST Returns
/// Offline Tool before it is uploaded or filed. Known gaps: no `version` or
/// `hash` keys, items are numbered 1, 2, … per invoice, Table 12 is written
/// as `hsn.hsn_b2b` / `hsn.hsn_b2c`, and exports carry no shipping-bill
/// details. Everything that touches the schema is in this one function.
pub fn gstr1_json(r: &Gstr1, gstin: &str) -> Value {
    let mut out = Map::new();
    out.insert("gstin".into(), json!(gstin));
    out.insert("fp".into(), json!(r.fp));
    if !r.b2b.is_empty() {
        let b2b: Vec<Value> = grouped(&r.b2b, |d| d.party_gstin.clone().unwrap_or_default())
            .into_iter()
            .map(|(ctin, docs)| {
                let inv: Vec<Value> = docs
                    .iter()
                    .map(|d| {
                        json!({
                            "inum": d.number,
                            "idt": portal_date(d.date),
                            "val": num(d.value),
                            "pos": d.place_of_supply,
                            "rchrg": "N",
                            "inv_typ": "R",
                            "itms": items(d, true),
                        })
                    })
                    .collect();
                json!({ "ctin": ctin, "inv": inv })
            })
            .collect();
        out.insert("b2b".into(), b2b.into());
    }
    if !r.b2cl.is_empty() {
        let b2cl: Vec<Value> = grouped(&r.b2cl, |d| d.place_of_supply.clone())
            .into_iter()
            .map(|(pos, docs)| {
                let inv: Vec<Value> = docs
                    .iter()
                    .map(|d| json!({ "inum": d.number, "idt": portal_date(d.date), "val": num(d.value), "itms": items(d, false) }))
                    .collect();
                json!({ "pos": pos, "inv": inv })
            })
            .collect();
        out.insert("b2cl".into(), b2cl.into());
    }
    if !r.b2cs.is_empty() {
        let b2cs: Vec<Value> = r
            .b2cs
            .iter()
            .map(|b| {
                let a = &b.amounts;
                let mut row = Map::new();
                row.insert("sply_ty".into(), json!(if b.inter_state { "INTER" } else { "INTRA" }));
                row.insert("pos".into(), json!(b.place_of_supply));
                row.insert("typ".into(), json!("OE"));
                row.insert("rt".into(), num(b.rate));
                row.insert("txval".into(), num(a.taxable));
                if b.inter_state {
                    row.insert("iamt".into(), num(a.igst));
                } else {
                    row.insert("camt".into(), num(a.cgst));
                    row.insert("samt".into(), num(a.sgst));
                }
                row.insert("csamt".into(), num(a.cess));
                Value::Object(row)
            })
            .collect();
        out.insert("b2cs".into(), b2cs.into());
    }
    if !r.cdnr.is_empty() {
        let cdnr: Vec<Value> = grouped(&r.cdnr, |d| d.party_gstin.clone().unwrap_or_default())
            .into_iter()
            .map(|(ctin, docs)| {
                let nt: Vec<Value> = docs
                    .iter()
                    .map(|d| {
                        json!({
                            "ntty": note_type(d.kind),
                            "nt_num": d.number,
                            "nt_dt": portal_date(d.date),
                            "val": num(d.value),
                            "pos": d.place_of_supply,
                            "rchrg": "N",
                            "inv_typ": "R",
                            "itms": items(d, true),
                        })
                    })
                    .collect();
                json!({ "ctin": ctin, "nt": nt })
            })
            .collect();
        out.insert("cdnr".into(), cdnr.into());
    }
    if !r.cdnur.is_empty() {
        let cdnur: Vec<Value> = r
            .cdnur
            .iter()
            .map(|d| {
                json!({
                    "typ": d.typ.clone().unwrap_or_default(),
                    "ntty": note_type(d.kind),
                    "nt_num": d.number,
                    "nt_dt": portal_date(d.date),
                    "val": num(d.value),
                    "pos": d.place_of_supply,
                    "itms": items(d, false),
                })
            })
            .collect();
        out.insert("cdnur".into(), cdnur.into());
    }
    if !r.exp.is_empty() {
        let exp: Vec<Value> = grouped(&r.exp, |d| d.typ.clone().unwrap_or_default())
            .into_iter()
            .map(|(typ, docs)| {
                let inv: Vec<Value> = docs
                    .iter()
                    .map(|d| json!({ "inum": d.number, "idt": portal_date(d.date), "val": num(d.value), "itms": items(d, false) }))
                    .collect();
                json!({ "exp_typ": if typ == "EXPWP" { "WPAY" } else { "WOPAY" }, "inv": inv })
            })
            .collect();
        out.insert("exp".into(), exp.into());
    }
    if !r.nil.is_empty() {
        let inv: Vec<Value> = r
            .nil
            .iter()
            .map(|n| {
                let ty = match (n.inter_state, n.registered) {
                    (true, true) => "INTRB2B",
                    (false, true) => "INTRAB2B",
                    (true, false) => "INTRB2C",
                    (false, false) => "INTRAB2C",
                };
                json!({ "sply_ty": ty, "nil_amt": num(n.value), "expt_amt": 0, "ngsup_amt": 0 })
            })
            .collect();
        out.insert("nil".into(), json!({ "inv": inv }));
    }
    if !(r.hsn_b2b.is_empty() && r.hsn_b2c.is_empty()) {
        out.insert("hsn".into(), json!({ "hsn_b2b": hsn_rows(&r.hsn_b2b), "hsn_b2c": hsn_rows(&r.hsn_b2c) }));
    }
    if !r.docs_issued.is_empty() {
        let mut det: Vec<(u8, String, Vec<Value>)> = Vec::new();
        for row in &r.docs_issued {
            let doc = json!({
                "from": row.from,
                "to": row.to,
                "totnum": row.total,
                "cancel": row.cancelled,
                "net_issue": row.net_issued,
            });
            match det.iter_mut().find(|(n, _, _)| *n == row.doc_num) {
                Some((_, _, docs)) => docs.push(doc),
                None => det.push((row.doc_num, row.doc_type.clone(), vec![doc])),
            }
        }
        let doc_det: Vec<Value> = det
            .into_iter()
            .map(|(n, typ, docs)| {
                let docs: Vec<Value> = docs
                    .into_iter()
                    .enumerate()
                    .map(|(i, mut d)| {
                        d["num"] = json!(i + 1);
                        d
                    })
                    .collect();
                json!({ "doc_num": n, "doc_typ": typ, "docs": docs })
            })
            .collect();
        out.insert("doc_issue".into(), json!({ "doc_det": doc_det }));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const MH: &str = "27";
    const KA_GSTIN: &str = "29AAGCB7383J1Z4";
    const MH_GSTIN: &str = "27AAPFU0939F1ZV";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn may() -> Period {
        Period { from: d(2026, 5, 1), to: d(2026, 5, 31) }
    }

    /// One HSN line at 18%: IGST when inter-State, else CGST + SGST.
    fn line(taxable: Decimal, inter: bool) -> DocLine {
        let tax = taxable * dec!(0.18);
        DocLine {
            hsn: Some("8471".into()),
            uqc: Some("NOS".into()),
            qty: Some(dec!(1)),
            taxable_value: taxable,
            rate: None,
            igst: if inter { tax } else { dec!(0) },
            cgst: if inter { dec!(0) } else { tax / dec!(2) },
            sgst: if inter { dec!(0) } else { tax / dec!(2) },
            cess: dec!(0),
            reverse_charge: false,
            blocked: false,
        }
    }

    fn doc(number: &str, kind: DocKind, gstin: Option<&str>, pos: &str, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 10),
            party_name: format!("Party {number}"),
            party_gstin: gstin.map(str::to_owned),
            place_of_supply: pos.into(),
            value: None,
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn rates_snap_to_standard_slabs() {
        assert_eq!(snap_rate(dec!(18.004)), (dec!(18), true));
        assert_eq!(snap_rate(dec!(0.2499)), (dec!(0.25), true));
        assert_eq!(snap_rate(dec!(17.5)), (dec!(17.5), false));
        assert_eq!(snap_rate(dec!(0)), (dec!(0), true));
        // A line whose tax doesn't fit a slab is flagged.
        let odd = DocLine { igst: dec!(175), ..line(dec!(1000), true) };
        let r = gstr1(&[doc("1", DocKind::Invoice, Some(KA_GSTIN), "29", vec![odd])], may(), MH);
        assert_eq!(r.b2b[0].items[0].rate, dec!(17.5));
        assert!(r.warnings.iter().any(|w| w.contains("17.5%")), "{:?}", r.warnings);
        // A rate given on the line is kept as is.
        let given = DocLine { rate: Some(dec!(5)), ..line(dec!(1000), true) };
        let r = gstr1(&[doc("2", DocKind::Invoice, Some(KA_GSTIN), "29", vec![given])], may(), MH);
        assert_eq!(r.b2b[0].items[0].rate, dec!(5));
    }

    #[test]
    fn invoices_split_into_b2b_b2cl_and_b2cs_at_the_threshold() {
        let docs = [
            doc("1", DocKind::Invoice, Some(KA_GSTIN), "29", vec![line(dec!(500000), true)]),
            // Inter-State, unregistered: 84,746 + 18% = 1,00,000.28 > ₹1 lakh.
            doc("2", DocKind::Invoice, None, "29", vec![line(dec!(84746), true)]),
            // Exactly ₹1 lakh is not "more than": B2CS.
            OutwardDoc { value: Some(dec!(100000)), ..doc("3", DocKind::Invoice, None, "29", vec![line(dec!(84745.76), true)]) },
            // Intra-State unregistered, however large: B2CS.
            doc("4", DocKind::Invoice, None, MH, vec![line(dec!(900000), false)]),
        ];
        let r = gstr1(&docs, may(), MH);
        assert_eq!(r.b2b.iter().map(|d| d.number.as_str()).collect::<Vec<_>>(), ["1"]);
        assert_eq!(r.b2b[0].party_gstin.as_deref(), Some(KA_GSTIN));
        assert_eq!(r.b2cl.iter().map(|d| d.number.as_str()).collect::<Vec<_>>(), ["2"]);
        assert_eq!(r.b2cs.len(), 2);
        let inter = r.b2cs.iter().find(|b| b.inter_state).unwrap();
        assert_eq!((inter.place_of_supply.as_str(), inter.rate, inter.amounts.taxable), ("29", dec!(18), dec!(84745.76)));
        let intra = r.b2cs.iter().find(|b| !b.inter_state).unwrap();
        assert_eq!((intra.amounts.cgst, intra.amounts.sgst), (dec!(81000), dec!(81000)));

        // Before 1 Aug 2024 the threshold was ₹2.5 lakh.
        let old = Period { from: d(2024, 7, 1), to: d(2024, 7, 31) };
        let big = OutwardDoc { date: d(2024, 7, 31), ..doc("5", DocKind::Invoice, None, "29", vec![line(dec!(200000), true)]) };
        let r = gstr1(std::slice::from_ref(&big), old, MH);
        assert!(r.b2cl.is_empty() && r.b2cs.len() == 1);
        let aug = Period { from: d(2024, 8, 1), to: d(2024, 8, 31) };
        let r = gstr1(&[OutwardDoc { date: d(2024, 8, 1), ..big }], aug, MH);
        assert_eq!(r.b2cl.len(), 1);
    }

    #[test]
    fn notes_go_to_cdnr_cdnur_or_net_into_b2cs() {
        let docs = [
            doc("S1", DocKind::Invoice, None, MH, vec![line(dec!(10000), false)]),
            doc("C1", DocKind::CreditNote, Some(KA_GSTIN), "29", vec![line(dec!(1000), true)]),
            doc("C2", DocKind::CreditNote, None, "29", vec![line(dec!(150000), true)]),
            doc("C3", DocKind::CreditNote, None, MH, vec![line(dec!(2000), false)]),
            doc("D1", DocKind::DebitNote, Some(MH_GSTIN), MH, vec![line(dec!(500), false)]),
        ];
        let r = gstr1(&docs, may(), MH);
        assert_eq!(r.cdnr.iter().map(|d| d.number.as_str()).collect::<Vec<_>>(), ["C1", "D1"]);
        assert_eq!(r.cdnur.len(), 1);
        assert_eq!((r.cdnur[0].number.as_str(), r.cdnur[0].typ.as_deref()), ("C2", Some("B2CL")));
        // The small local credit note reduces B2CS.
        assert_eq!(r.b2cs.len(), 1);
        assert_eq!(r.b2cs[0].amounts.taxable, dec!(8000));
        assert_eq!(r.net.taxable, dec!(10000) - dec!(1000) - dec!(150000) - dec!(2000) + dec!(500));

        let g3 = gstr3b(&docs, &[], may(), MH);
        assert_eq!(g3.outward_taxable.taxable, r.net.taxable);
        // 3.2: the inter-State unregistered credit note reduces Karnataka.
        assert_eq!(g3.inter_state_unregistered, [PlaceRow { place_of_supply: "29".into(), taxable: dec!(-150000), igst: dec!(-27000) }]);
    }

    #[test]
    fn hsn_summary_splits_b2b_and_b2c_and_nets_credit_notes() {
        let nil = DocLine { hsn: Some("0401".into()), uqc: Some("LTR".into()), qty: Some(dec!(10)), taxable_value: dec!(600), ..DocLine::default() };
        let docs = [
            doc("1", DocKind::Invoice, Some(KA_GSTIN), "29", vec![line(dec!(1000), true), nil.clone()]),
            doc("2", DocKind::Invoice, None, MH, vec![line(dec!(400), false), DocLine { hsn: None, ..line(dec!(100), false) }]),
            doc("3", DocKind::CreditNote, Some(KA_GSTIN), "29", vec![line(dec!(250), true)]),
        ];
        let r = gstr1(&docs, may(), MH);
        let b2b: Vec<_> = r.hsn_b2b.iter().map(|h| (h.hsn.as_str(), h.uqc.as_str(), h.qty, h.rate, h.amounts.taxable)).collect();
        assert_eq!(b2b, [("0401", "LTR", dec!(10), dec!(0), dec!(600)), ("8471", "NOS", dec!(0), dec!(18), dec!(750))]);
        let b2c: Vec<_> = r.hsn_b2c.iter().map(|h| (h.hsn.as_str(), h.amounts.taxable)).collect();
        assert_eq!(b2c, [("", dec!(100)), ("8471", dec!(400))]);
        assert!(r.warnings.iter().any(|w| w.contains("Invoice 2 has a line without an HSN/SAC code")), "{:?}", r.warnings);
        // The nil-rated line is in Table 8, not in the B2B items.
        assert_eq!(r.nil, [NilRow { inter_state: true, registered: true, value: dec!(600) }]);
        assert_eq!(r.b2b[0].items.len(), 1);
        assert_eq!(r.b2b[0].value, dec!(1000) + dec!(180) + dec!(600));
        let g3 = gstr3b(&docs, &[], may(), MH);
        assert_eq!(g3.nil_exempt.taxable, dec!(600));
    }

    #[test]
    fn documents_issued_count_series_and_cancellations() {
        let mut docs: Vec<OutwardDoc> =
            (1..=5).map(|n| doc(&format!("26-27/{n}"), DocKind::Invoice, None, MH, vec![line(dec!(100), false)])).collect();
        docs[2].cancelled = true;
        docs.remove(3); // "26-27/4" was deleted: a gap
        docs.push(doc("CN/1", DocKind::CreditNote, None, MH, vec![line(dec!(10), false)]));
        let r = gstr1(&docs, may(), MH);
        let rows: Vec<_> = r
            .docs_issued
            .iter()
            .map(|d| (d.doc_num, d.from.as_str(), d.to.as_str(), d.total, d.cancelled, d.net_issued))
            .collect();
        assert_eq!(rows, [(1, "26-27/1", "26-27/5", 4, 1, 3), (5, "CN/1", "CN/1", 1, 0, 1)]);
        assert!(r.warnings.iter().any(|w| w.contains("1 number(s) in between are missing")), "{:?}", r.warnings);
        // The cancelled invoice counts nowhere else.
        assert_eq!(r.b2cs[0].amounts.taxable, dec!(290));
        let docs_row = r.summary.iter().find(|s| s.section == Section::Docs).unwrap();
        assert_eq!(docs_row.count, 5);
    }

    #[test]
    fn untaxed_documents_and_wrong_components_are_flagged() {
        let bare = DocLine { taxable_value: dec!(100), ..DocLine::default() };
        let r = gstr1(&[doc("1", DocKind::Invoice, None, MH, vec![bare]), doc("2", DocKind::Invoice, Some(KA_GSTIN), "29", vec![line(dec!(100), false)])], may(), MH);
        assert!(r.warnings.iter().any(|w| w.contains("Invoice 1 has no tax lines")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("Invoice 2 is inter-State by place of supply but charges CGST/SGST")));
    }

    #[test]
    fn gstr3b_nets_output_against_itc_and_drops_unregistered_input() {
        let outward = [
            doc("1", DocKind::Invoice, Some(KA_GSTIN), "29", vec![line(dec!(10000), true)]),
            doc("2", DocKind::Invoice, None, MH, vec![line(dec!(5000), false)]),
            OutwardDoc {
                lines: vec![DocLine { rate: Some(dec!(18)), ..line(dec!(3000), true) }].into_iter().map(|l| DocLine { igst: dec!(0), ..l }).collect(),
                ..doc("E1", DocKind::Invoice, None, EXPORT_POS, vec![])
            },
        ];
        let inward = [
            InwardDoc {
                id: "p1".into(),
                kind: DocKind::Invoice,
                number: "P-1".into(),
                date: d(2026, 5, 3),
                party_name: "Supplier".into(),
                party_gstin: Some(KA_GSTIN.into()),
                cancelled: false,
                lines: vec![line(dec!(4000), true)],
            },
            InwardDoc {
                id: "p2".into(),
                kind: DocKind::CreditNote,
                number: "DN-1".into(),
                date: d(2026, 5, 20),
                party_name: "Supplier".into(),
                party_gstin: Some(KA_GSTIN.into()),
                cancelled: false,
                lines: vec![line(dec!(1000), true)],
            },
            InwardDoc {
                id: "p3".into(),
                kind: DocKind::Invoice,
                number: "X".into(),
                date: d(2026, 5, 3),
                party_name: "Unregistered".into(),
                party_gstin: None,
                cancelled: false,
                lines: vec![line(dec!(1000), false)],
            },
        ];
        let g = gstr3b(&outward, &inward, may(), MH);
        assert_eq!(g.outward_taxable.taxable, dec!(15000));
        assert_eq!((g.outward_taxable.igst, g.outward_taxable.cgst), (dec!(1800), dec!(450)));
        assert_eq!(g.zero_rated.taxable, dec!(3000));
        assert_eq!(g.itc.igst, dec!(540));
        assert!(g.warnings.iter().any(|w| w.contains("Purchase X")), "{:?}", g.warnings);
        let net: Vec<_> = g.net.iter().map(|n| (n.component.as_str(), n.net)).collect();
        assert_eq!(net, [("IGST", dec!(1260)), ("CGST", dec!(450)), ("SGST/UTGST", dec!(450)), ("Cess", dec!(0))]);
    }

    #[test]
    fn gstr3b_reverse_charge_and_blocked_credit() {
        let purchase = |id: &str, gstin: Option<&str>, kind: DocKind, lines: Vec<DocLine>| InwardDoc {
            id: id.into(),
            kind,
            number: id.into(),
            date: d(2026, 5, 4),
            party_name: "Supplier".into(),
            party_gstin: gstin.map(str::to_owned),
            cancelled: false,
            lines,
        };
        let rcm = |taxable: Decimal, inter: bool| DocLine { reverse_charge: true, ..line(taxable, inter) };
        let inward = [
            // Legal services from an advocate (s.9(3)), registered or not.
            purchase("r1", None, DocKind::Invoice, vec![rcm(dec!(10000), false)]),
            purchase("r2", Some(KA_GSTIN), DocKind::Invoice, vec![rcm(dec!(5000), true)]),
            // A blocked credit (s.17(5)) from a registered supplier.
            purchase("b1", Some(MH_GSTIN), DocKind::Invoice, vec![DocLine { blocked: true, ..line(dec!(2000), false) }, line(dec!(1000), false)]),
            // A blocked reverse-charge supply: liability, credit reversed.
            purchase("rb", None, DocKind::Invoice, vec![DocLine { blocked: true, ..rcm(dec!(1000), false) }]),
            // A return of part of r2.
            purchase("r2-ret", Some(KA_GSTIN), DocKind::CreditNote, vec![rcm(dec!(1000), true)]),
        ];
        let g = gstr3b(&[], &inward, may(), MH);
        assert_eq!(g.reverse_charge_inward.taxable, dec!(15000));
        assert_eq!((g.reverse_charge_inward.igst, g.reverse_charge_inward.cgst), (dec!(720), dec!(990)));
        assert_eq!(g.itc_reverse_charge, g.reverse_charge_inward);
        assert_eq!((g.itc.cgst, g.itc.sgst), (dec!(270), dec!(270)), "blocked credit stays in 4(A)(5)");
        assert_eq!((g.itc_reversed_permanent.cgst, g.itc_reversed_permanent.taxable), (dec!(270), dec!(0)), "180 + 90 reversed");
        assert_eq!(g.itc_net.cgst, dec!(990), "990 + 270 − 270");
        assert!(g.warnings.is_empty(), "an unregistered supplier on reverse charge is fine: {:?}", g.warnings);
        let net: Vec<_> = g.net.iter().map(|n| (n.component.as_str(), n.output, n.itc, n.reverse_charge)).collect();
        assert_eq!(net[0], ("IGST", dec!(720), dec!(720), dec!(720)));
        assert_eq!(net[1], ("CGST", dec!(990), dec!(990), dec!(990)));

        let g = with_rule37(g, &Amounts { cgst: dec!(50), sgst: dec!(50), ..Amounts::default() });
        assert_eq!((g.itc_reversed_other.cgst, g.itc_net.cgst, g.net[1].net), (dec!(50), dec!(940), dec!(50)));
    }

    #[test]
    fn json_has_the_portal_keys() {
        let docs = [
            doc("26-27/1", DocKind::Invoice, Some(KA_GSTIN), "29", vec![line(dec!(1000), true)]),
            doc("26-27/2", DocKind::Invoice, None, MH, vec![line(dec!(500), false)]),
            doc("26-27/3", DocKind::Invoice, None, "29", vec![line(dec!(200000), true)]),
            doc("CN/1", DocKind::CreditNote, Some(KA_GSTIN), "29", vec![line(dec!(100), true)]),
        ];
        let j = gstr1_json(&gstr1(&docs, may(), MH), MH_GSTIN);
        assert_eq!(j["gstin"], MH_GSTIN);
        assert_eq!(j["fp"], "052026");
        assert_eq!(
            j["b2b"][0],
            json!({
                "ctin": KA_GSTIN,
                "inv": [{
                    "inum": "26-27/1", "idt": "10-05-2026", "val": 1180.0, "pos": "29", "rchrg": "N", "inv_typ": "R",
                    "itms": [{ "num": 1, "itm_det": { "txval": 1000.0, "rt": 18.0, "iamt": 180.0, "camt": 0.0, "samt": 0.0, "csamt": 0.0 } }]
                }]
            })
        );
        assert_eq!(j["b2cl"][0]["pos"], "29");
        assert_eq!(j["b2cl"][0]["inv"][0]["itms"][0]["itm_det"], json!({ "txval": 200000.0, "rt": 18.0, "iamt": 36000.0, "csamt": 0.0 }));
        assert_eq!(
            j["b2cs"][0],
            json!({ "sply_ty": "INTRA", "pos": "27", "typ": "OE", "rt": 18.0, "txval": 500.0, "camt": 45.0, "samt": 45.0, "csamt": 0.0 })
        );
        assert_eq!(j["cdnr"][0]["nt"][0]["ntty"], "C");
        assert_eq!(j["cdnr"][0]["nt"][0]["nt_dt"], "10-05-2026");
        assert_eq!(j["hsn"]["hsn_b2b"][0]["hsn_sc"], "8471");
        assert_eq!(j["hsn"]["hsn_b2c"][0]["txval"], 200500.0);
        assert_eq!(
            j["doc_issue"]["doc_det"][0],
            json!({ "doc_num": 1, "doc_typ": "Invoices for outward supply", "docs": [{ "num": 1, "from": "26-27/1", "to": "26-27/3", "totnum": 3, "cancel": 0, "net_issue": 3 }] })
        );
        assert!(j.get("exp").is_none() && j.get("cdnur").is_none(), "empty sections are left out");
    }
}

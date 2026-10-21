//! A sales tax liability report by jurisdiction, from neutral documents.
//!
//! Every state has its own return form, filing frequency and lines
//! (`docs/research/us-compliance.md` §3), so KhataOS does not fill any one
//! state's return. It gives the figures those returns are built from, per
//! jurisdiction the company defined (see [`crate::sales_tax`]) and rolled
//! up per state:
//! - **gross sales**: every line's value, before tax;
//! - **exempt sales**: lines exempt by class, by a 0% state override, by
//!   a resale/exemption class, or because the customer's resale or
//!   exemption certificate for the state was in force on the sale's date
//!   ([`crate::exemptions`]); the last are also shown on their own as
//!   **certificate-exempt sales** (many state returns ask for sales for
//!   resale separately);
//! - **not collected**: lines in a state where the company has no rate on
//!   file (no permit or nexus there). These are sales the company must
//!   check against that state's economic nexus threshold ([`crate::nexus`]);
//! - **taxable sales**: gross − exempt − not collected;
//! - **tax collected**: the sales tax charged on the documents.
//!
//! Credit notes reduce the period they are dated in; debit notes add to
//! it. Cancelled documents are left out. Sales abroad (exports) are listed
//! as one total and are not in any jurisdiction.
//!
//! Use tax on purchases is a separate report ([`crate::use_tax`]). Not
//! modelled: bad-debt deductions, vendor discounts for
//! timely filing, cash-basis reporting (some states allow it), and
//! marketplace sales a marketplace facilitator collected on (enter them as
//! a `RESALE…`/`EXEMPT…` class or leave them out).
//!
//! None of this is legal advice: the figures are a preparation aid.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::sales_tax::{Taxability, parse_jurisdiction};
use crate::{nexus, states};

/// What a document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the sale (a credit note / sales return).
    CreditNote,
    /// Adds to the sale.
    DebitNote,
}

impl DocKind {
    /// +1 for invoices and debit notes, −1 for credit notes.
    pub fn sign(self) -> Decimal {
        if self == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE }
    }
}

/// One line. Amounts are positive; the document kind gives the sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleLine {
    pub taxability: Taxability,
    /// Value before tax, in USD.
    pub value: Decimal,
    /// Sales tax charged on the line (all components).
    pub tax: Decimal,
}

/// A sales document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleDoc {
    /// The app's own reference (a voucher id).
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The jurisdiction code ("CA" or "CA-LA"); `None` for a sale abroad.
    pub jurisdiction: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<SaleLine>,
    /// The number of the customer's certificate that makes this sale
    /// exempt in its state (see [`crate::exemptions::covering`]).
    pub certificate: Option<String>,
}

/// The report period, both ends inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Period {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Period {
    pub fn contains(&self, d: NaiveDate) -> bool {
        self.from <= d && d <= self.to
    }
}

/// Totals for one jurisdiction or state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub gross_sales: Decimal,
    pub exempt_sales: Decimal,
    /// The part of `exempt_sales` exempt by a customer's certificate.
    pub certificate_exempt_sales: Decimal,
    pub not_collected_sales: Decimal,
    pub taxable_sales: Decimal,
    pub tax_collected: Decimal,
    /// Documents counted (credit notes included).
    pub documents: usize,
}

impl Totals {
    fn add(&mut self, line: &SaleLine, sign: Decimal, certified: bool) {
        let v = line.value * sign;
        self.gross_sales += v;
        match line.taxability {
            Taxability::Taxable if certified => {
                self.exempt_sales += v;
                self.certificate_exempt_sales += v;
            }
            Taxability::Taxable => self.taxable_sales += v,
            Taxability::Exempt => self.exempt_sales += v,
            Taxability::NotCollected => self.not_collected_sales += v,
        }
        self.tax_collected += line.tax * sign;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JurisdictionRow {
    /// "CA" or "CA-LA".
    pub jurisdiction: String,
    pub state: String,
    pub state_name: String,
    #[serde(flatten)]
    pub totals: Totals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateRow {
    pub state: String,
    pub state_name: String,
    #[serde(flatten)]
    pub totals: Totals,
    /// The state's economic nexus test over the calendar year to the end
    /// of the period, when KhataOS has a sourced threshold for it.
    pub nexus: Option<nexus::NexusCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalesTaxReport {
    pub period: Period,
    pub jurisdictions: Vec<JurisdictionRow>,
    pub states: Vec<StateRow>,
    /// Sales abroad, net of credit notes.
    pub export_sales: Decimal,
    /// All jurisdictions together (exports not included).
    pub total: Totals,
    pub warnings: Vec<String>,
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Invoice",
        DocKind::CreditNote => "Credit note",
        DocKind::DebitNote => "Debit note",
    }
}

/// Work out the report for a period. `docs` may hold documents outside
/// the period: those from 1 January of the period's last year count for
/// the nexus test only.
pub fn sales_tax_report(docs: &[SaleDoc], period: Period) -> SalesTaxReport {
    let mut warnings = Vec::new();
    let mut by_juris: BTreeMap<String, Totals> = BTreeMap::new();
    let mut by_state: BTreeMap<String, Totals> = BTreeMap::new();
    let mut year_sales: BTreeMap<String, (Decimal, u64)> = BTreeMap::new();
    let mut export_sales = Decimal::ZERO;
    let mut total = Totals::default();
    let year_start = NaiveDate::from_ymd_opt(chrono::Datelike::year(&period.to), 1, 1).expect("1 January exists");

    for d in docs.iter().filter(|d| !d.cancelled) {
        let in_period = period.contains(d.date);
        let in_year = year_start <= d.date && d.date <= period.to;
        if !in_period && !in_year {
            continue;
        }
        let name = format!("{} {}", label(d.kind), d.number);
        let sign = d.kind.sign();
        let Some(code) = d.jurisdiction.as_deref() else {
            if in_period {
                export_sales += d.lines.iter().map(|l| l.value).sum::<Decimal>() * sign;
                if d.lines.iter().any(|l| !l.tax.is_zero()) {
                    warnings.push(format!("{name}: a sale abroad carries sales tax; check it"));
                }
            }
            continue;
        };
        let juris = match parse_jurisdiction(code) {
            Ok(j) => j,
            Err(e) => {
                if in_period {
                    warnings.push(format!("{name}: {e}; it is left out"));
                }
                continue;
            }
        };
        if in_year {
            let y = year_sales.entry(juris.state.clone()).or_default();
            y.0 += d.lines.iter().map(|l| l.value).sum::<Decimal>() * sign;
            if d.kind == DocKind::Invoice {
                y.1 += 1;
            }
        }
        if !in_period {
            continue;
        }
        let j = by_juris.entry(juris.code()).or_default();
        let s = by_state.entry(juris.state.clone()).or_default();
        j.documents += 1;
        s.documents += 1;
        total.documents += 1;
        let certified = d.certificate.is_some();
        for l in &d.lines {
            j.add(l, sign, certified);
            s.add(l, sign, certified);
            total.add(l, sign, certified);
            if l.taxability != Taxability::Taxable && !l.tax.is_zero() {
                warnings.push(format!("{name}: an exempt or uncollected line carries tax {}; check it", l.tax.normalize()));
            }
        }
        if let Some(number) = &d.certificate
            && d.lines.iter().any(|l| l.taxability == Taxability::Taxable && !l.tax.is_zero())
        {
            warnings.push(format!(
                "{name}: tax was charged although the customer's certificate {number} covers the sale; refund or credit it, or the certificate does not apply"
            ));
        }
    }
    let name_of = |code: &str| states::state(code).map(|s| s.name.to_owned()).unwrap_or_default();
    let jurisdictions = by_juris
        .into_iter()
        .map(|(code, totals)| {
            let state = code.split('-').next().unwrap_or_default().to_owned();
            JurisdictionRow { jurisdiction: code, state_name: name_of(&state), state, totals }
        })
        .collect();
    let mut states_rows: Vec<StateRow> = by_state
        .into_iter()
        .map(|(state, totals)| StateRow { state_name: name_of(&state), nexus: None, state, totals })
        .collect();
    for row in &mut states_rows {
        let (sales, count) = year_sales.get(&row.state).copied().unwrap_or_default();
        row.nexus = nexus::check(&row.state, sales, count);
        if row.totals.not_collected_sales > Decimal::ZERO {
            let met = row.nexus.as_ref().is_some_and(|n| n.met);
            warnings.push(format!(
                "{}: sales of {} with no sales tax rate on file{}",
                row.state_name,
                row.totals.not_collected_sales.normalize(),
                if met { " — the state's economic nexus threshold is met this year; register and collect" } else { "; check the state's nexus rules" }
            ));
        }
    }
    SalesTaxReport { period, jurisdictions, states: states_rows, export_sales, total, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn line(t: Taxability, value: Decimal, tax: Decimal) -> SaleLine {
        SaleLine { taxability: t, value, tax }
    }

    fn doc(kind: DocKind, date: NaiveDate, juris: Option<&str>, lines: Vec<SaleLine>) -> SaleDoc {
        SaleDoc {
            id: String::new(),
            kind,
            number: "1".into(),
            date,
            party_name: "Customer".into(),
            jurisdiction: juris.map(str::to_owned),
            cancelled: false,
            lines,
            certificate: None,
        }
    }

    #[test]
    fn totals_by_jurisdiction_and_state() {
        let q = Period { from: d(2026, 4, 1), to: d(2026, 6, 30) };
        let mut cancelled = doc(DocKind::Invoice, d(2026, 4, 3), Some("CA"), vec![line(Taxability::Taxable, dec!(999), dec!(72))]);
        cancelled.cancelled = true;
        let docs = vec![
            doc(DocKind::Invoice, d(2026, 4, 2), Some("CA-LA"), vec![
                line(Taxability::Taxable, dec!(1000), dec!(95)),
                line(Taxability::Exempt, dec!(200), dec!(0)),
            ]),
            doc(DocKind::Invoice, d(2026, 5, 2), Some("ca"), vec![line(Taxability::Taxable, dec!(100), dec!(7.25))]),
            doc(DocKind::CreditNote, d(2026, 5, 9), Some("CA-LA"), vec![line(Taxability::Taxable, dec!(100), dec!(9.5))]),
            doc(DocKind::Invoice, d(2026, 6, 1), Some("NY"), vec![line(Taxability::NotCollected, dec!(500), dec!(0))]),
            doc(DocKind::Invoice, d(2026, 6, 2), None, vec![line(Taxability::Exempt, dec!(300), dec!(0))]),
            doc(DocKind::Invoice, d(2026, 7, 1), Some("CA"), vec![line(Taxability::Taxable, dec!(5), dec!(1))]),
            cancelled,
        ];
        let r = sales_tax_report(&docs, q);
        let j: Vec<_> = r.jurisdictions.iter().map(|j| (j.jurisdiction.as_str(), j.totals.gross_sales, j.totals.taxable_sales, j.totals.tax_collected)).collect();
        assert_eq!(j, [("CA", dec!(100), dec!(100), dec!(7.25)), ("CA-LA", dec!(1100), dec!(900), dec!(85.5)), ("NY", dec!(500), dec!(0), dec!(0))]);
        let ca = &r.states[0];
        assert_eq!((ca.state_name.as_str(), ca.totals.exempt_sales, ca.totals.tax_collected, ca.totals.documents), ("California", dec!(200), dec!(92.75), 3));
        assert_eq!((r.export_sales, r.total.gross_sales, r.total.not_collected_sales), (dec!(300), dec!(1700), dec!(500)));
        assert!(r.warnings.iter().any(|w| w.starts_with("New York: sales of 500")), "{:?}", r.warnings);
    }

    #[test]
    fn certificates_make_taxable_sales_exempt() {
        let q = Period { from: d(2026, 4, 1), to: d(2026, 6, 30) };
        let mut resale = doc(DocKind::Invoice, d(2026, 4, 2), Some("CA"), vec![line(Taxability::Taxable, dec!(1000), dec!(0)), line(Taxability::Exempt, dec!(50), dec!(0))]);
        resale.certificate = Some("SR-1".into());
        let mut charged = doc(DocKind::Invoice, d(2026, 4, 3), Some("CA"), vec![line(Taxability::Taxable, dec!(100), dec!(7.25))]);
        charged.certificate = Some("SR-1".into());
        let plain = doc(DocKind::Invoice, d(2026, 4, 4), Some("CA"), vec![line(Taxability::Taxable, dec!(200), dec!(14.5))]);
        let r = sales_tax_report(&[resale, charged, plain], q);
        let ca = &r.jurisdictions[0].totals;
        assert_eq!((ca.gross_sales, ca.exempt_sales, ca.certificate_exempt_sales, ca.taxable_sales), (dec!(1350), dec!(1150), dec!(1100), dec!(200)));
        assert_eq!(ca.tax_collected, dec!(21.75), "tax charged is still reported as collected");
        assert!(r.warnings.iter().any(|w| w.contains("certificate SR-1 covers the sale")), "{:?}", r.warnings);
    }

    #[test]
    fn suspicious_lines_and_unknown_jurisdictions_warn() {
        let p = Period { from: d(2026, 1, 1), to: d(2026, 1, 31) };
        let docs = vec![
            doc(DocKind::Invoice, d(2026, 1, 2), Some("CA"), vec![line(Taxability::Exempt, dec!(10), dec!(1))]),
            doc(DocKind::Invoice, d(2026, 1, 3), Some("ZZ"), vec![line(Taxability::Taxable, dec!(10), dec!(1))]),
            doc(DocKind::Invoice, d(2026, 1, 4), None, vec![line(Taxability::Taxable, dec!(10), dec!(1))]),
        ];
        let r = sales_tax_report(&docs, p);
        assert_eq!(r.warnings.len(), 3, "{:?}", r.warnings);
        assert_eq!(r.jurisdictions.len(), 1);
    }
}

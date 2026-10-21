//! The monthly IVA summary (pago definitivo mensual) from outward and
//! inward documents.
//!
//! The input is a neutral document model, as in the other packs. The
//! figures follow the structure of the monthly IVA declaration
//! (`docs/research/mexico-compliance.md` §1):
//! - **actos o actividades** by treatment: taxed at 16%, taxed at 8%
//!   (border stimulus), 0% and exempt, net of credit notes;
//! - **IVA trasladado**: IVA charged on sales;
//! - **IVA acreditable**: IVA on purchases backed by a supplier with an
//!   RFC (a CFDI is needed to credit IVA; purchases without an RFC keep
//!   their value but not their IVA, with a warning);
//! - **IVA retenido por clientes**: IVA customers withheld on the
//!   business's invoices (LIVA art. 1-A; see [`crate::retenciones`]). The
//!   customer pays it to the SAT, so it comes off what the business owes;
//! - **IVA a cargo** = trasladado − retenido por clientes − acreditable
//!   when positive; otherwise **saldo a favor**;
//! - **retenciones a enterar**: IVA and ISR the business withheld from
//!   its own suppliers (personas físicas, freight, RESICO). These are paid
//!   over separately by the 17th of the following month (LIVA art. 1-A
//!   fr. IV; LISR arts. 106, 113-J); the IVA withheld is creditable once
//!   paid over (LIVA art. 5 fr. IV), which this summary takes as given in
//!   the same month [MEDIUM: research §13];
//! - **ISR retenido por clientes**: informative, for the ISR provisional
//!   payment (not part of IVA).
//!
//! Simplifications, each a reason to check before declaring:
//! - **Cash basis.** IVA is due, and creditable, when the consideration is
//!   actually collected or paid (LIVA arts. 1-B and 5), not on the invoice
//!   date. This summary works on document dates: it matches the law only
//!   for documents paid in the same month (PUE). PPD invoices settled in a
//!   later month belong to that month's declaration [research §1].
//! - saldos a favor of earlier months, proportional crediting and IVA on
//!   imports are not worked out; withholdings come from what the documents
//!   say (booked withholding ledgers), on document dates.
//!
//! None of this is legal advice: the figures are a preparation aid.

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::iva::Treatment;

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    /// Ingreso.
    Invoice,
    /// Egreso: a nota de crédito, reduces the supply.
    CreditNote,
    /// A further charge (cargo adicional), adds to the supply.
    DebitNote,
}

impl DocKind {
    pub fn sign(self) -> Decimal {
        if self == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE }
    }
}

/// One line; amounts positive, the document kind gives the sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLine {
    pub treatment: Treatment,
    /// The rate, e.g. 16 or 8 (0 when not taxed).
    pub percent: Decimal,
    /// Value before IVA, in MXN.
    pub value: Decimal,
    pub iva: Decimal,
}

/// Taxes withheld on a document, positive; the document kind gives the
/// sign.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Retenido {
    pub iva: Decimal,
    pub isr: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
    /// What the customer withheld.
    pub retenido: Retenido,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The supplier's RFC; `None` when unknown.
    pub party_rfc: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
    /// What the business withheld from the supplier.
    pub retenido: Retenido,
}

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

/// Values by treatment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Activities {
    /// Taxed at 16%.
    pub taxed_16: Decimal,
    /// Taxed at 8% (border stimulus).
    pub taxed_8: Decimal,
    /// Taxed at another positive rate (a rate history the user entered).
    pub taxed_other: Decimal,
    pub zero_rated: Decimal,
    pub exempt: Decimal,
}

impl Activities {
    fn add(&mut self, l: &DocLine, sign: Decimal) {
        let v = l.value * sign;
        match l.treatment {
            Treatment::Exempt => self.exempt += v,
            Treatment::ZeroRated => self.zero_rated += v,
            Treatment::Taxed if l.percent == Decimal::from(16) => self.taxed_16 += v,
            Treatment::Taxed if l.percent == Decimal::from(8) => self.taxed_8 += v,
            Treatment::Taxed => self.taxed_other += v,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IvaSummary {
    pub period: Period,
    /// Sales by treatment.
    pub sales: Activities,
    /// Purchases by treatment.
    pub purchases: Activities,
    /// IVA charged on sales.
    pub iva_trasladado: Decimal,
    /// IVA on purchases that can be credited.
    pub iva_acreditable: Decimal,
    /// IVA customers withheld (they pay it to the SAT).
    pub iva_retenido_por_clientes: Decimal,
    /// ISR customers withheld (for the ISR provisional payment).
    pub isr_retenido_por_clientes: Decimal,
    /// IVA the business withheld from suppliers: to pay over.
    pub iva_retenido_a_enterar: Decimal,
    /// ISR the business withheld from suppliers: to pay over.
    pub isr_retenido_a_enterar: Decimal,
    /// Payable: trasladado − retenido por clientes − acreditable, when
    /// positive.
    pub iva_a_cargo: Decimal,
    /// Balance in favour, when the credits exceed trasladado.
    pub saldo_a_favor: Decimal,
    pub warnings: Vec<String>,
}

fn centavos(a: Decimal) -> Decimal {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Factura",
        DocKind::CreditNote => "Nota de crédito",
        DocKind::DebitNote => "Nota de cargo",
    }
}

/// Work out the summary for a period (normally a calendar month).
pub fn iva_summary(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> IvaSummary {
    let mut warnings = Vec::new();
    let mut sales = Activities::default();
    let mut purchases = Activities::default();
    let (mut trasladado, mut acreditable) = (Decimal::ZERO, Decimal::ZERO);
    let (mut by_customers, mut to_pay) = (Retenido::default(), Retenido::default());
    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        by_customers.iva += d.retenido.iva * sign;
        by_customers.isr += d.retenido.isr * sign;
        for l in &d.lines {
            sales.add(l, sign);
            if l.treatment == Treatment::Taxed {
                trasladado += l.iva * sign;
            } else if !l.iva.is_zero() {
                warnings.push(format!("{} {}: a 0% or exempt line carries IVA {}; it is left out", label(d.kind), d.number, l.iva.normalize()));
            }
        }
    }
    let mut no_rfc = 0usize;
    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        to_pay.iva += d.retenido.iva * sign;
        to_pay.isr += d.retenido.isr * sign;
        let has_rfc = d.party_rfc.as_deref().is_some_and(|r| !r.trim().is_empty());
        for l in &d.lines {
            purchases.add(l, sign);
            if l.treatment != Treatment::Taxed || l.iva.is_zero() {
                continue;
            }
            if has_rfc {
                acreditable += l.iva * sign;
            } else {
                no_rfc += 1;
            }
        }
    }
    if no_rfc > 0 {
        warnings.push(format!(
            "{no_rfc} purchase line(s) with IVA are from suppliers without an RFC on file; their IVA is not credited (a CFDI is needed)"
        ));
    }
    let net = centavos(trasladado - by_customers.iva - acreditable);
    if period.from.format("%Y-%m").to_string() != period.to.format("%Y-%m").to_string() {
        warnings.push("the IVA declaration is monthly; this period spans more than one month".into());
    }
    IvaSummary {
        period,
        sales,
        purchases,
        iva_trasladado: centavos(trasladado),
        iva_acreditable: centavos(acreditable),
        iva_retenido_por_clientes: centavos(by_customers.iva),
        isr_retenido_por_clientes: centavos(by_customers.isr),
        iva_retenido_a_enterar: centavos(to_pay.iva),
        isr_retenido_a_enterar: centavos(to_pay.isr),
        iva_a_cargo: net.max(Decimal::ZERO),
        saldo_a_favor: (-net).max(Decimal::ZERO),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn l(t: Treatment, percent: Decimal, value: Decimal, iva: Decimal) -> DocLine {
        DocLine { treatment: t, percent, value, iva }
    }

    fn out(kind: DocKind, date: NaiveDate, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc { id: String::new(), kind, number: "A-1".into(), date, party_name: "Cliente".into(), cancelled: false, lines, retenido: Retenido::default() }
    }

    fn inw(date: NaiveDate, rfc: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: String::new(),
            kind: DocKind::Invoice,
            number: "F-9".into(),
            date,
            party_name: "Proveedor".into(),
            party_rfc: rfc.map(str::to_owned),
            cancelled: false,
            lines,
            retenido: Retenido::default(),
        }
    }

    #[test]
    fn a_month_with_sales_purchases_and_a_credit_note() {
        let may = Period { from: d(2026, 5, 1), to: d(2026, 5, 31) };
        let outward = vec![
            out(DocKind::Invoice, d(2026, 5, 2), vec![
                l(Treatment::Taxed, dec!(16), dec!(1000), dec!(160)),
                l(Treatment::Taxed, dec!(8), dec!(500), dec!(40)),
                l(Treatment::ZeroRated, dec!(0), dec!(300), dec!(0)),
                l(Treatment::Exempt, dec!(0), dec!(200), dec!(0)),
            ]),
            out(DocKind::CreditNote, d(2026, 5, 9), vec![l(Treatment::Taxed, dec!(16), dec!(100), dec!(16))]),
            out(DocKind::Invoice, d(2026, 6, 1), vec![l(Treatment::Taxed, dec!(16), dec!(1), dec!(0.16))]),
        ];
        let inward = vec![
            inw(d(2026, 5, 3), Some("EKU9003173C9"), vec![l(Treatment::Taxed, dec!(16), dec!(600), dec!(96))]),
            inw(d(2026, 5, 4), None, vec![l(Treatment::Taxed, dec!(16), dec!(100), dec!(16))]),
        ];
        let s = iva_summary(&outward, &inward, may);
        assert_eq!((s.sales.taxed_16, s.sales.taxed_8, s.sales.zero_rated, s.sales.exempt), (dec!(900), dec!(500), dec!(300), dec!(200)));
        assert_eq!((s.iva_trasladado, s.iva_acreditable, s.iva_a_cargo, s.saldo_a_favor), (dec!(184), dec!(96), dec!(88), dec!(0)));
        assert_eq!(s.purchases.taxed_16, dec!(700));
        assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
    }

    #[test]
    fn a_balance_in_favour_and_period_warnings() {
        let p = Period { from: d(2026, 5, 1), to: d(2026, 6, 30) };
        let inward = vec![inw(d(2026, 5, 3), Some("EKU9003173C9"), vec![l(Treatment::Taxed, dec!(16), dec!(600), dec!(96))])];
        let outward = vec![out(DocKind::Invoice, d(2026, 5, 2), vec![l(Treatment::Exempt, dec!(0), dec!(10), dec!(1.6))])];
        let s = iva_summary(&outward, &inward, p);
        assert_eq!((s.iva_a_cargo, s.saldo_a_favor), (dec!(0), dec!(96)));
        assert_eq!(s.warnings.len(), 2, "{:?}", s.warnings);
    }

    #[test]
    fn withholdings_by_customers_and_to_pay_over() {
        let may = Period { from: d(2026, 5, 1), to: d(2026, 5, 31) };
        // Honorarios to a persona moral: 1000 + 160 IVA, ISR 100 and IVA
        // 106.67 withheld by the customer.
        let mut sale = out(DocKind::Invoice, d(2026, 5, 2), vec![l(Treatment::Taxed, dec!(16), dec!(1000), dec!(160))]);
        sale.retenido = Retenido { iva: dec!(106.67), isr: dec!(100) };
        // A freelancer's fee the business paid, withholding from them.
        let mut fee = inw(d(2026, 5, 3), Some("EKU9003173C9"), vec![l(Treatment::Taxed, dec!(16), dec!(200), dec!(32))]);
        fee.retenido = Retenido { iva: dec!(21.33), isr: dec!(20) };
        let s = iva_summary(&[sale], &[fee], may);
        assert_eq!((s.iva_trasladado, s.iva_retenido_por_clientes, s.isr_retenido_por_clientes), (dec!(160), dec!(106.67), dec!(100)));
        assert_eq!((s.iva_acreditable, s.iva_retenido_a_enterar, s.isr_retenido_a_enterar), (dec!(32), dec!(21.33), dec!(20)));
        assert_eq!((s.iva_a_cargo, s.saldo_a_favor), (dec!(21.33), dec!(0)), "160 − 106.67 − 32");
    }
}

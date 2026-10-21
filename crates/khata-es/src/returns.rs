//! The modelo 303 (autoliquidación del IVA) worked out from outward and
//! inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its
//! vouchers into documents. The casillas follow the AEAT's modelo 303 and
//! its instructions (`docs/research/spain-compliance.md` §4).
//!
//! What is worked out (outward: the company's sales; inward: purchases):
//! - IVA devengado, régimen general: 01–03 (4%), 04–06 (10%), 07–09 (21%)
//!   as base, tipo and cuota; 150–152 (0%) and 153–155 (5%) for the
//!   temporary food and energy rates; 10/11 adquisiciones
//!   intracomunitarias de bienes y servicios; 12/13 otras operaciones con
//!   inversión del sujeto pasivo; 14/15 modificación de bases y cuotas
//!   (credit and debit notes the company issued); 27 total cuota
//!   devengada.
//! - IVA deducible: 28/29 operaciones interiores corrientes (including the
//!   self-assessed IVA of 12/13 and of services acquired in the EU); 36/37
//!   adquisiciones intracomunitarias de bienes corrientes; 40/41
//!   rectificación de deducciones (credit notes received); 45 total a
//!   deducir; 46 resultado régimen general (27 − 45).
//! - Información adicional: 59 entregas intracomunitarias de bienes y
//!   servicios; 60 exportaciones y operaciones asimiladas; 120 operaciones
//!   no sujetas por reglas de localización; 122 operaciones sujetas con
//!   inversión del sujeto pasivo (the company's sales).
//! - Resultado: 64 (= 46), 65 (100% atribuible a la Administración del
//!   Estado), 66, 69 and 71 (the result to pay, or to refund or carry
//!   forward when negative).
//!
//! Not modelled (left at zero): the recargo de equivalencia (16–26), bienes
//! de inversión (30/31, 38/39), importaciones (32–35, 77), compensaciones
//! agrarias (42), the regularizaciones (43, 44, 68), cuotas a compensar de
//! periodos anteriores (110, 78, 87), the foral share (65 below 100%), the
//! régimen simplificado and the prorrata. All input IVA is taken as
//! deductible and as gastos corrientes.
//!
//! Amounts are euros and cents. Cuotas at 4%, 10% and 21% are base × tipo,
//! as the form computes them, with a warning when the IVA booked differs.
//! None of this is tax advice: the figures are a preparation aid to be
//! checked before the modelo is filed at the AEAT's sede electrónica.

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::comunidades;
use crate::iva::{REDUCED_PERCENT, STANDARD_PERCENT, SUPER_REDUCED_PERCENT, Treatment};

/// The 5% of the temporary food (2023–2024) and energy measures.
pub const TEMPORARY_5_PERCENT: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// A factura rectificativa that reduces the supply.
    CreditNote,
    /// A factura rectificativa that adds to it.
    DebitNote,
}

impl DocKind {
    pub fn sign(self) -> Decimal {
        if self == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE }
    }

    /// A rectificativa: its amounts go in the modificación casillas.
    pub fn rectificativa(self) -> bool {
        self != DocKind::Invoice
    }
}

/// One line of a document. Amounts are positive; the document kind says
/// whether they add or reduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLine {
    pub treatment: Treatment,
    /// Goods (a stock item) rather than services.
    pub goods: bool,
    /// Value excluding IVA, in EUR.
    pub taxable_value: Decimal,
    /// The rate that applies: the rate charged on a taxed line, and on an
    /// inversión del sujeto pasivo or intra-EU purchase the rate the
    /// company owes.
    pub percent: Decimal,
    /// IVA booked on the line, in EUR; zero on untaxed lines.
    pub iva: Decimal,
}

/// A document the company issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The buyer's country (ISO 3166-1 alpha-2); `None`: Spain.
    pub party_country: Option<String>,
    /// The buyer's VAT identifier.
    pub party_vat_id: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// A document received from a supplier (`kind` from the supplier's point
/// of view: a purchase return is the supplier's credit note).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The supplier's country; `None`: Spain.
    pub party_country: Option<String>,
    pub party_vat_id: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The period, both ends inclusive (a quarter or a month).
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

/// The modelo 303 for a period. Field names are the casillas (`c01` …).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Modelo303 {
    pub period: Option<Period>,
    /// Régimen general 4%: base imponible, cuota.
    pub c01: Decimal,
    pub c03: Decimal,
    /// 10%.
    pub c04: Decimal,
    pub c06: Decimal,
    /// 21%.
    pub c07: Decimal,
    pub c09: Decimal,
    /// 0% (temporary).
    pub c150: Decimal,
    pub c152: Decimal,
    /// 5% (temporary).
    pub c153: Decimal,
    pub c155: Decimal,
    /// Adquisiciones intracomunitarias de bienes y servicios: base, cuota.
    pub c10: Decimal,
    pub c11: Decimal,
    /// Otras operaciones con inversión del sujeto pasivo (excepto adq. intracom.).
    pub c12: Decimal,
    pub c13: Decimal,
    /// Modificación de bases y cuotas.
    pub c14: Decimal,
    pub c15: Decimal,
    /// Total cuota devengada.
    pub c27: Decimal,
    /// Por cuotas soportadas en operaciones interiores corrientes: base, cuota.
    pub c28: Decimal,
    pub c29: Decimal,
    /// En adquisiciones intracomunitarias de bienes corrientes: base, cuota.
    pub c36: Decimal,
    pub c37: Decimal,
    /// Rectificación de deducciones: base, cuota.
    pub c40: Decimal,
    pub c41: Decimal,
    /// Total a deducir.
    pub c45: Decimal,
    /// Resultado régimen general (27 − 45).
    pub c46: Decimal,
    /// Entregas intracomunitarias de bienes y servicios.
    pub c59: Decimal,
    /// Exportaciones y operaciones asimiladas.
    pub c60: Decimal,
    /// Operaciones no sujetas por reglas de localización (excepto las de 123).
    pub c120: Decimal,
    /// Operaciones sujetas con inversión del sujeto pasivo.
    pub c122: Decimal,
    /// Suma de resultados.
    pub c64: Decimal,
    /// % atribuible a la Administración del Estado.
    pub c65: Decimal,
    /// Atribuible a la Administración del Estado.
    pub c66: Decimal,
    /// Resultado.
    pub c69: Decimal,
    /// Resultado de la liquidación: positive to pay; negative to refund or
    /// carry forward.
    pub c71: Decimal,
    pub warnings: Vec<String>,
}

/// Euros and cents, half away from zero.
pub fn cents(a: Decimal) -> Decimal {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Factura",
        DocKind::CreditNote | DocKind::DebitNote => "Factura rectificativa",
    }
}

fn abroad_eu(country: Option<&str>) -> bool {
    country.is_some_and(|c| !comunidades::is_spain(c) && comunidades::in_eu(c))
}

fn abroad(country: Option<&str>) -> bool {
    country.is_some_and(|c| !c.trim().is_empty() && !comunidades::is_spain(c))
}

fn has(id: &Option<String>) -> bool {
    id.as_deref().is_some_and(|s| !s.trim().is_empty())
}

#[derive(Default)]
struct Sums {
    b4: Decimal,
    t4: Decimal,
    b10: Decimal,
    t10: Decimal,
    b21: Decimal,
    t21: Decimal,
    b0: Decimal,
    b5: Decimal,
    t5: Decimal,
    b_aic: Decimal,
    t_aic: Decimal,
    b_isp: Decimal,
    t_isp: Decimal,
    b_mod: Decimal,
    t_mod: Decimal,
    b_int: Decimal,
    t_int: Decimal,
    b_aicb: Decimal,
    t_aicb: Decimal,
    b_rect: Decimal,
    t_rect: Decimal,
    b59: Decimal,
    b60: Decimal,
    b120: Decimal,
    b122: Decimal,
}

/// Work out the modelo 303 for a period from the documents dated in it.
pub fn modelo_303(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> Modelo303 {
    let mut w = Vec::new();
    let mut s = Sums::default();

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let sign = d.kind.sign();
        let eu = abroad_eu(d.party_country.as_deref());
        for l in &d.lines {
            let v = l.taxable_value * sign;
            let t = l.iva * sign;
            match l.treatment {
                Treatment::Taxed | Treatment::Zero if d.kind.rectificativa() => {
                    s.b_mod += v;
                    s.t_mod += t;
                }
                Treatment::Taxed => {
                    if l.percent == SUPER_REDUCED_PERCENT {
                        s.b4 += v;
                        s.t4 += t;
                    } else if l.percent == REDUCED_PERCENT {
                        s.b10 += v;
                        s.t10 += t;
                    } else if l.percent == STANDARD_PERCENT {
                        s.b21 += v;
                        s.t21 += t;
                    } else if l.percent == TEMPORARY_5_PERCENT {
                        s.b5 += v;
                        s.t5 += t;
                    } else {
                        w.push(format!("{name}: IVA at {}% has no casilla of its own; the line is left out (correct it with a rectificativa)", l.percent.normalize()));
                    }
                }
                Treatment::Zero => s.b0 += v,
                Treatment::Exempt => {}
                Treatment::ReverseCharge => s.b122 += v,
                Treatment::IntraEu => {
                    if !has(&d.party_vat_id) {
                        w.push(format!("{name}: an intra-EU supply to {} without the buyer's VAT identifier is not exempt (art. 25 LIVA); it is still put in casilla 59", d.party_name));
                    }
                    s.b59 += v;
                }
                Treatment::Export => s.b60 += v,
                Treatment::OutOfScope => {
                    if eu && !l.goods && has(&d.party_vat_id) {
                        s.b59 += v;
                    } else {
                        s.b120 += v;
                    }
                }
            }
            if !l.treatment.taxed() && !l.iva.is_zero() {
                w.push(format!("{name}: a line without IVA treatment carries IVA {}; it is left out", l.iva.normalize()));
            }
        }
    }

    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {} ({})", label(d.kind), d.number, d.party_name);
        let sign = d.kind.sign();
        let country = d.party_country.as_deref();
        for l in &d.lines {
            let v = l.taxable_value * sign;
            let owed = l.taxable_value * l.percent / Decimal::ONE_HUNDRED * sign;
            match l.treatment {
                Treatment::Taxed | Treatment::Zero => {
                    if d.kind.rectificativa() {
                        s.b_rect += v;
                        s.t_rect += l.iva * sign;
                    } else {
                        s.b_int += v;
                        s.t_int += l.iva * sign;
                    }
                }
                Treatment::IntraEu if l.goods => {
                    if !abroad_eu(country) {
                        w.push(format!("{name}: an intra-EU acquisition from a supplier outside the EU or without a country is put in casillas 10/11"));
                    }
                    s.b_aic += v;
                    s.t_aic += owed;
                    s.b_aicb += v;
                    s.t_aicb += owed;
                }
                Treatment::IntraEu | Treatment::ReverseCharge | Treatment::Export => {
                    if l.percent.is_zero() {
                        w.push(format!("{name}: an inversión del sujeto pasivo at 0% owes no IVA; give it the rate the company owes"));
                    }
                    if abroad_eu(country) {
                        // Services from an EU business: casillas 10/11.
                        s.b_aic += v;
                        s.t_aic += owed;
                    } else {
                        if !abroad(country) && l.treatment != Treatment::ReverseCharge {
                            w.push(format!("{name}: a purchase from a Spanish supplier is put in casillas 12/13"));
                        }
                        s.b_isp += v;
                        s.t_isp += owed;
                    }
                    s.b_int += v;
                    s.t_int += owed;
                }
                _ => {}
            }
            if !matches!(l.treatment, Treatment::Taxed) && !l.iva.is_zero() {
                w.push(format!("{name}: a line without IVA treatment carries IVA {}; it is left out", l.iva.normalize()));
            }
        }
    }

    let tax = |b: Decimal, rate: Decimal| cents(cents(b) * rate / Decimal::ONE_HUNDRED);
    let mut r = Modelo303 { period: Some(period), ..Modelo303::default() };
    r.c01 = cents(s.b4);
    r.c03 = tax(s.b4, SUPER_REDUCED_PERCENT);
    r.c04 = cents(s.b10);
    r.c06 = tax(s.b10, REDUCED_PERCENT);
    r.c07 = cents(s.b21);
    r.c09 = tax(s.b21, STANDARD_PERCENT);
    for (casilla, computed, booked) in [("03", r.c03, s.t4), ("06", r.c06, s.t10), ("09", r.c09, s.t21)] {
        if (computed - booked).abs() > Decimal::ONE {
            w.push(format!("casilla {casilla}: the IVA booked is {}, but base × tipo gives {computed}; check the rates on file", cents(booked)));
        }
    }
    r.c150 = cents(s.b0);
    r.c153 = cents(s.b5);
    r.c155 = cents(s.t5);
    r.c10 = cents(s.b_aic);
    r.c11 = cents(s.t_aic);
    r.c12 = cents(s.b_isp);
    r.c13 = cents(s.t_isp);
    r.c14 = cents(s.b_mod);
    r.c15 = cents(s.t_mod);
    r.c27 = r.c03 + r.c06 + r.c09 + r.c152 + r.c155 + r.c11 + r.c13 + r.c15;
    // Goods acquired in the EU are deducted in 36/37; everything else
    // (services acquired in the EU included) in 28/29.
    r.c36 = cents(s.b_aicb);
    r.c37 = cents(s.t_aicb);
    r.c28 = cents(s.b_int);
    r.c29 = cents(s.t_int);
    r.c40 = cents(s.b_rect);
    r.c41 = cents(s.t_rect);
    r.c45 = r.c29 + r.c37 + r.c41;
    r.c46 = r.c27 - r.c45;
    r.c59 = cents(s.b59);
    r.c60 = cents(s.b60);
    r.c120 = cents(s.b120);
    r.c122 = cents(s.b122);
    r.c64 = r.c46;
    r.c65 = Decimal::ONE_HUNDRED;
    r.c66 = cents(r.c64 * r.c65 / Decimal::ONE_HUNDRED);
    r.c69 = r.c66;
    r.c71 = r.c69;
    r.warnings = w;
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn q1() -> Period {
        Period { from: d(2026, 1, 1), to: d(2026, 3, 31) }
    }

    fn taxed(value: Decimal, percent: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Taxed, goods: true, taxable_value: value, percent, iva: (value * percent / dec!(100)).round_dp(2) }
    }

    fn untaxed(treatment: Treatment, goods: bool, value: Decimal, percent: Decimal) -> DocLine {
        DocLine { treatment, goods, taxable_value: value, percent, iva: dec!(0) }
    }

    fn sale(number: &str, kind: DocKind, country: Option<&str>, vat: Option<&str>, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 2, 10),
            party_name: format!("Cliente {number}"),
            party_country: country.map(str::to_owned),
            party_vat_id: vat.map(str::to_owned),
            cancelled: false,
            lines,
        }
    }

    fn purchase(number: &str, kind: DocKind, country: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 2, 5),
            party_name: "Proveedor".into(),
            party_country: country.map(str::to_owned),
            party_vat_id: None,
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn domestic_sales_purchases_and_rectificativas() {
        let outward = [
            sale("A-1", DocKind::Invoice, None, None, vec![taxed(dec!(1000), dec!(21)), taxed(dec!(200), dec!(10)), taxed(dec!(50), dec!(4))]),
            sale("R-1", DocKind::CreditNote, None, None, vec![taxed(dec!(100), dec!(21))]),
            sale("A-2", DocKind::Invoice, None, None, vec![untaxed(Treatment::Exempt, false, dec!(800), dec!(0))]),
            sale("A-3", DocKind::Invoice, None, Some("B58378431"), vec![untaxed(Treatment::ReverseCharge, false, dec!(2500), dec!(0))]),
        ];
        let inward = [
            purchase("P-1", DocKind::Invoice, None, vec![taxed(dec!(500), dec!(21)), taxed(dec!(100), dec!(10))]),
            purchase("P-R", DocKind::CreditNote, None, vec![taxed(dec!(50), dec!(21))]),
            purchase("OBRA-1", DocKind::Invoice, None, vec![untaxed(Treatment::ReverseCharge, false, dec!(1000), dec!(21))]),
        ];
        let r = modelo_303(&outward, &inward, q1());
        assert_eq!((r.c01, r.c03, r.c04, r.c06, r.c07, r.c09), (dec!(50), dec!(2.00), dec!(200), dec!(20.00), dec!(1000), dec!(210.00)));
        assert_eq!((r.c14, r.c15), (dec!(-100), dec!(-21)), "the rectificativa is a modificación");
        assert_eq!((r.c12, r.c13, r.c122), (dec!(1000), dec!(210), dec!(2500)), "ISP received in 12/13, issued in 122");
        // 27: 2 + 20 + 210 − 21 + 210 = 421.
        assert_eq!(r.c27, dec!(421.00));
        // 28/29: 600 + 1000 (ISP) → 105 + 10 + 210 = 325; 40/41: −50 / −10.50.
        assert_eq!((r.c28, r.c29, r.c40, r.c41), (dec!(1600), dec!(325), dec!(-50), dec!(-10.50)));
        assert_eq!((r.c45, r.c46, r.c64, r.c65, r.c66, r.c71), (dec!(314.50), dec!(106.50), dec!(106.50), dec!(100), dec!(106.50), dec!(106.50)));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn eu_trade_exports_and_services() {
        let outward = [
            sale("UE-1", DocKind::Invoice, Some("FR"), Some("FR40303265045"), vec![untaxed(Treatment::IntraEu, true, dec!(3000), dec!(0))]),
            sale("EX-1", DocKind::Invoice, Some("US"), None, vec![untaxed(Treatment::Export, true, dec!(2000), dec!(0))]),
            sale("S-1", DocKind::Invoice, Some("DE"), Some("DE136695976"), vec![untaxed(Treatment::OutOfScope, false, dec!(1500), dec!(0))]),
            sale("S-2", DocKind::Invoice, Some("US"), None, vec![untaxed(Treatment::OutOfScope, false, dec!(700), dec!(0))]),
        ];
        let inward = [
            purchase("FR-1", DocKind::Invoice, Some("FR"), vec![untaxed(Treatment::IntraEu, true, dec!(1000), dec!(21))]),
            purchase("DE-1", DocKind::Invoice, Some("DE"), vec![untaxed(Treatment::ReverseCharge, false, dec!(200), dec!(21))]),
            purchase("US-1", DocKind::Invoice, Some("US"), vec![untaxed(Treatment::ReverseCharge, false, dec!(300), dec!(21))]),
        ];
        let r = modelo_303(&outward, &inward, q1());
        assert_eq!((r.c59, r.c60, r.c120), (dec!(4500), dec!(2000), dec!(700)));
        assert_eq!((r.c10, r.c11, r.c12, r.c13), (dec!(1200), dec!(252), dec!(300), dec!(63)));
        assert_eq!((r.c36, r.c37, r.c28, r.c29), (dec!(1000), dec!(210), dec!(500), dec!(105)));
        assert_eq!((r.c27, r.c45, r.c71), (dec!(315), dec!(315), dec!(0)), "self-assessed IVA is deducted again");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn temporary_rates_period_and_warnings() {
        let mut outside = sale("0", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(21))]);
        outside.date = d(2026, 4, 1);
        let mut cancelled = sale("X", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(21))]);
        cancelled.cancelled = true;
        let food = sale("F-1", DocKind::Invoice, None, None, vec![taxed(dec!(100), dec!(5)), untaxed(Treatment::Zero, true, dec!(40), dec!(0))]);
        let old = sale("V-1", DocKind::Invoice, None, None, vec![taxed(dec!(100), dec!(18))]);
        let no_id = sale("UE-2", DocKind::Invoice, Some("IT"), None, vec![untaxed(Treatment::IntraEu, true, dec!(10), dec!(0))]);
        let r = modelo_303(&[outside, cancelled, food, old, no_id], &[], q1());
        assert_eq!((r.c07, r.c153, r.c155, r.c150, r.c152, r.c59), (dec!(0), dec!(100), dec!(5), dec!(40), dec!(0), dec!(10)));
        assert_eq!(r.c71, dec!(5));
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("18%"));
        assert!(r.warnings[1].contains("without the buyer's VAT identifier"));
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["c155"].as_str(), j["c65"].as_str()), (Some("5"), Some("100")));
    }
}

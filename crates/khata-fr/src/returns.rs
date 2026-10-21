//! The CA3 (déclaration de TVA n° 3310-CA3, régime réel normal) worked
//! out from outward and inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its
//! vouchers into documents. The lines follow the 3310-CA3 form and its
//! notice (`docs/research/france-compliance.md` §6).
//!
//! What is worked out (outward: the company's sales; inward: purchases):
//! - cadre A, opérations imposables (bases HT): A1 ventes, prestations de
//!   services; A2 autres opérations imposables (the buyer's side of a
//!   domestic autoliquidation); A3 achats de prestations de services auprès
//!   d'un assujetti non établi en France (services from abroad,
//!   autoliquidées); B2 acquisitions intracommunautaires; B5
//!   régularisations (credit notes on taxable sales, a positive figure);
//! - opérations non imposables: E1 exportations hors UE (and supplies to
//!   or from the overseas régions); E2 autres opérations non imposables
//!   (exempt supplies, services to customers abroad, the seller's side of a
//!   domestic autoliquidation); F2 livraisons intracommunautaires à
//!   destination d'une personne assujettie; F8 régularisations (credit
//!   notes on non-taxable operations);
//! - cadre B, TVA brute by rate (base and tax): 08 (20%), 09 (5.5%), 9B
//!   (10%), T6 (2.1% in continental France); in the DOM 10 (8.5%), 11
//!   (2.1%), T1 (1.75%), T2 (1.05%); in Corse TC (13%), T3 (10%), T4
//!   (2.1%), T5 (0.9%); 13 anciens taux (19.6%, 7%). The self-assessed TVA
//!   of A2, A3 and B2 is in these lines too. 15: TVA antérieurement
//!   déduite à reverser (suppliers' credit notes); 16 total TVA brute; 17
//!   dont TVA sur acquisitions intracommunautaires;
//! - TVA déductible: 20 autres biens et services (input TVA on purchase
//!   invoices plus the self-assessed TVA, all taken as deductible); 21
//!   autre TVA à déduire (the TVA of the company's own credit notes); 23
//!   total; 25 crédit de TVA (23 − 16) and 27 crédit à reporter, or TD TVA
//!   due (16 − 23) and 28 TVA nette due; 32 total à payer.
//!
//! The CA3 never takes a negative amount: credit notes go to the
//! régularisation lines (B5, F8, 15, 21). A box that still comes out
//! negative (a credit note on an autoliquidated purchase) gets a warning.
//!
//! Not modelled (left at zero): A4/A5 and I1–I6 imports, B1/B3, B4 (a
//! non-established supplier selling in France), E3–E6, F1, F3–F7, F9, T7,
//! 5B, 18, 19 (immobilisations: all input TVA is in 20), 22 (the credit
//! carried from the last return), 2C, 26/AA, 29 taxes assimilées, the
//! energy-excise lines, the annexe 3310 A, the CA12 simplified scheme and
//! partial deduction.
//!
//! Rounding: "la base imposable et le montant de l'impôt sont arrondis à
//! l'euro le plus proche" (0.50 counts as 1), box by box; the tax of a rate
//! line is the rounded base × rate, with a warning when the TVA booked
//! differs (research §6). None of this is tax advice: the figures are a
//! preparation aid to be keyed into impots.gouv.fr (EFI) after checking.

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::regions;
use crate::tva::{
    DOM_REDUCED_PERCENT, DOM_STANDARD_PERCENT, INTERMEDIATE_PERCENT, REDUCED_PERCENT, STANDARD_PERCENT, Treatment,
};

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply (a facture d'avoir).
    CreditNote,
    /// Adds to the supply.
    DebitNote,
}

impl DocKind {
    pub fn sign(self) -> Decimal {
        if self == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE }
    }
}

/// One line of a document. Amounts are positive; the document kind says
/// whether they add or reduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLine {
    pub treatment: Treatment,
    /// Goods (a stock item) rather than services.
    pub goods: bool,
    /// Value excluding TVA (HT), in EUR.
    pub taxable_value: Decimal,
    /// The rate that applies: the rate charged on a taxed line, and on an
    /// autoliquidated or intra-EU purchase the rate the company owes.
    pub percent: Decimal,
    /// TVA booked on the line, in EUR; zero on untaxed lines.
    pub tva: Decimal,
}

/// A document the company issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The buyer's country (ISO 3166-1 alpha-2); `None`: France.
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
    /// The supplier's country; `None`: France.
    pub party_country: Option<String>,
    pub party_vat_id: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The period, both ends inclusive (a month or a quarter).
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

/// One rate line of cadre B: base HT and tax, whole euros.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RateLine {
    pub base: Decimal,
    pub tax: Decimal,
}

/// A rate without a line of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OtherRate {
    pub percent: Decimal,
    pub base: Decimal,
    pub tax: Decimal,
}

/// The CA3 for a period. Field names are the form's line codes; whole euros.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Ca3 {
    pub period: Option<Period>,
    /// Ventes, prestations de services.
    pub a1: Decimal,
    /// Autres opérations imposables.
    pub a2: Decimal,
    /// Achats de prestations de services réalisés auprès d'un assujetti non
    /// établi en France (art. 283-2).
    pub a3: Decimal,
    /// Acquisitions intracommunautaires.
    pub b2: Decimal,
    /// Régularisations (opérations imposables).
    pub b5: Decimal,
    /// Exportations hors UE.
    pub e1: Decimal,
    /// Autres opérations non imposables.
    pub e2: Decimal,
    /// Livraisons intracommunautaires à destination d'une personne assujettie.
    pub f2: Decimal,
    /// Régularisations (opérations non imposables).
    pub f8: Decimal,
    /// Taux normal 20%.
    pub l08: RateLine,
    /// Taux réduit 5.5%.
    pub l09: RateLine,
    /// Taux réduit 10%.
    pub l9b: RateLine,
    /// DOM: taux normal 8.5%.
    pub l10: RateLine,
    /// DOM: taux réduit 2.1%.
    pub l11: RateLine,
    /// DOM: 1.75%.
    pub t1: RateLine,
    /// DOM: 1.05%.
    pub t2: RateLine,
    /// Corse: 13%.
    pub tc: RateLine,
    /// Corse: 10%.
    pub t3: RateLine,
    /// Corse: 2.1%.
    pub t4: RateLine,
    /// Corse: 0.9%.
    pub t5: RateLine,
    /// France continentale: 2.1%.
    pub t6: RateLine,
    /// Anciens taux (19.6%, 7%).
    pub l13: RateLine,
    /// Rates without a line (left out of line 16, with a warning).
    pub other_rates: Vec<OtherRate>,
    /// TVA antérieurement déduite à reverser.
    pub l15: Decimal,
    /// Total de la TVA brute due.
    pub l16: Decimal,
    /// Dont TVA sur acquisitions intracommunautaires.
    pub l17: Decimal,
    /// TVA déductible: autres biens et services.
    pub l20: Decimal,
    /// Autre TVA à déduire (régularisation de TVA collectée).
    pub l21: Decimal,
    /// Total TVA déductible.
    pub l23: Decimal,
    /// Crédit de TVA (23 − 16), when positive.
    pub l25: Decimal,
    /// TVA due (16 − 23), when positive.
    pub td: Decimal,
    /// Crédit de TVA à reporter (no refund is asked for).
    pub l27: Decimal,
    /// TVA nette due.
    pub l28: Decimal,
    /// Total à payer.
    pub l32: Decimal,
    pub warnings: Vec<String>,
}

/// Whole euros, to the nearest (0.50 up, away from zero).
pub fn euros(a: Decimal) -> Decimal {
    a.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Facture",
        DocKind::CreditNote => "Avoir",
        DocKind::DebitNote => "Facture rectificative",
    }
}

fn abroad_eu(country: Option<&str>) -> bool {
    country.is_some_and(|c| !regions::is_france(c) && regions::in_eu(c))
}

fn abroad(country: Option<&str>) -> bool {
    country.is_some_and(|c| !c.trim().is_empty() && !regions::is_france(c))
}

fn has(id: &Option<String>) -> bool {
    id.as_deref().is_some_and(|s| !s.trim().is_empty())
}

/// Bases and booked tax by rate, before rounding.
#[derive(Default)]
struct ByRate(Vec<(Decimal, Decimal, Decimal)>);

impl ByRate {
    fn add(&mut self, percent: Decimal, base: Decimal, tax: Decimal) {
        match self.0.iter_mut().find(|(p, _, _)| *p == percent) {
            Some(x) => {
                x.1 += base;
                x.2 += tax;
            }
            None => self.0.push((percent, base, tax)),
        }
    }
}

#[derive(Default)]
struct Sums {
    rates: ByRate,
    a1: Decimal,
    a2: Decimal,
    a3: Decimal,
    b2: Decimal,
    b5: Decimal,
    e1: Decimal,
    e2: Decimal,
    f2: Decimal,
    f8: Decimal,
    t15: Decimal,
    t17: Decimal,
    t20: Decimal,
    t21: Decimal,
}

fn pct(units: i64, scale: u32) -> Decimal {
    Decimal::new(units, scale)
}

/// The rate line of a rate in the company's territory.
fn rate_line(r: &mut Ca3, percent: Decimal, territory: Territory) -> Option<&mut RateLine> {
    let p = percent.normalize();
    let line = match territory {
        _ if p == STANDARD_PERCENT => &mut r.l08,
        _ if p == REDUCED_PERCENT => &mut r.l09,
        Territory::Dom if p == DOM_STANDARD_PERCENT => &mut r.l10,
        Territory::Dom if p == DOM_REDUCED_PERCENT => &mut r.l11,
        Territory::Dom if p == pct(175, 2) => &mut r.t1,
        Territory::Dom if p == pct(105, 2) => &mut r.t2,
        Territory::Corse if p == pct(13, 0) => &mut r.tc,
        Territory::Corse if p == INTERMEDIATE_PERCENT => &mut r.t3,
        Territory::Corse if p == pct(21, 1) => &mut r.t4,
        Territory::Corse if p == pct(9, 1) => &mut r.t5,
        Territory::Continent if p == INTERMEDIATE_PERCENT => &mut r.l9b,
        Territory::Continent if p == pct(21, 1) => &mut r.t6,
        _ => return None,
    };
    Some(line)
}

/// Where the company makes its supplies, which picks its rate lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Territory {
    /// France continentale.
    Continent,
    /// Corse (art. 297 CGI).
    Corse,
    /// Guadeloupe, Martinique, La Réunion (art. 296 CGI).
    Dom,
}

/// Work out the CA3 for a period from the documents dated in it.
pub fn ca3(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period, territory: Territory) -> Ca3 {
    let mut w = Vec::new();
    let mut s = Sums::default();

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let credit = d.kind == DocKind::CreditNote;
        for l in &d.lines {
            let v = l.taxable_value;
            match l.treatment {
                Treatment::Taxed if credit => {
                    s.b5 += v;
                    s.t21 += l.tva;
                }
                Treatment::Taxed => {
                    s.a1 += v;
                    s.rates.add(l.percent, v, l.tva);
                }
                _ if credit => s.f8 += v,
                Treatment::Zero | Treatment::Exempt | Treatment::ReverseCharge | Treatment::OutOfScope => s.e2 += v,
                Treatment::IntraEu => {
                    if l.goods {
                        if !has(&d.party_vat_id) {
                            w.push(format!("{name}: an intra-EU supply to {} without the buyer's VAT identifier is not exempt (art. 262 ter CGI); it is still put in F2", d.party_name));
                        }
                        s.f2 += v;
                    } else {
                        s.e2 += v;
                    }
                }
                Treatment::Export => s.e1 += v,
            }
            if !l.treatment.taxed() && !l.tva.is_zero() {
                w.push(format!("{name}: a line without TVA treatment carries TVA {}; it is left out", l.tva.normalize()));
            }
        }
    }

    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {} ({})", label(d.kind), d.number, d.party_name);
        let credit = d.kind == DocKind::CreditNote;
        let sign = d.kind.sign();
        let country = d.party_country.as_deref();
        for l in &d.lines {
            let v = l.taxable_value * sign;
            let owed = l.taxable_value * l.percent / Decimal::ONE_HUNDRED * sign;
            match l.treatment {
                Treatment::Taxed if credit => s.t15 += l.tva,
                Treatment::Taxed => s.t20 += l.tva,
                Treatment::ReverseCharge | Treatment::IntraEu | Treatment::Export => {
                    if l.percent.is_zero() {
                        w.push(format!("{name}: an autoliquidated purchase at 0% owes no TVA; give it the rate the company owes"));
                    }
                    if l.treatment == Treatment::IntraEu && l.goods {
                        if !abroad_eu(country) {
                            w.push(format!("{name}: an intra-EU acquisition from a supplier outside the EU or without a country is put in B2"));
                        }
                        s.b2 += v;
                        s.t17 += owed;
                    } else if abroad(country) {
                        s.a3 += v;
                    } else {
                        if l.treatment != Treatment::ReverseCharge {
                            w.push(format!("{name}: a purchase from a French supplier without TVA is put in A2 as an autoliquidation"));
                        }
                        s.a2 += v;
                    }
                    s.rates.add(l.percent, v, owed);
                    s.t20 += owed;
                }
                _ => {}
            }
            if !l.treatment.taxed() && !l.tva.is_zero() {
                w.push(format!("{name}: a line without TVA treatment carries TVA {}; it is left out", l.tva.normalize()));
            }
        }
    }

    let mut r = Ca3 { period: Some(period), ..Ca3::default() };
    r.a1 = euros(s.a1);
    r.a2 = euros(s.a2);
    r.a3 = euros(s.a3);
    r.b2 = euros(s.b2);
    r.b5 = euros(s.b5);
    r.e1 = euros(s.e1);
    r.e2 = euros(s.e2);
    r.f2 = euros(s.f2);
    r.f8 = euros(s.f8);
    s.rates.0.sort_by_key(|r| std::cmp::Reverse(r.0));
    let old = [pct(196, 1), pct(7, 0)];
    for (percent, base, booked) in s.rates.0 {
        let exact = base * percent / Decimal::ONE_HUNDRED;
        let line = RateLine { base: euros(base), tax: euros(euros(base) * percent / Decimal::ONE_HUNDRED) };
        if (exact - booked).abs() > Decimal::ONE {
            w.push(format!(
                "TVA at {}%: the TVA booked is {}, but the base × rate gives {}; check the rates on file",
                percent.normalize(),
                booked.round_dp(2),
                exact.round_dp(2)
            ));
        }
        if let Some(x) = rate_line(&mut r, percent, territory) {
            x.base += line.base;
            x.tax += line.tax;
        } else if old.contains(&percent.normalize()) {
            r.l13.base += line.base;
            r.l13.tax += line.tax;
        } else {
            w.push(format!("TVA at {}% has no line on this company's CA3; it is left out of line 16", percent.normalize()));
            r.other_rates.push(OtherRate { percent, base: line.base, tax: line.tax });
        }
    }
    r.l15 = euros(s.t15);
    let lines = [&r.l08, &r.l09, &r.l9b, &r.l10, &r.l11, &r.t1, &r.t2, &r.tc, &r.t3, &r.t4, &r.t5, &r.t6, &r.l13];
    r.l16 = lines.iter().map(|l| l.tax).sum::<Decimal>() + r.l15;
    r.l17 = euros(s.t17);
    r.l20 = euros(s.t20);
    r.l21 = euros(s.t21);
    r.l23 = r.l20 + r.l21;
    if r.l16 >= r.l23 {
        r.td = r.l16 - r.l23;
    } else {
        r.l25 = r.l23 - r.l16;
    }
    r.l27 = r.l25;
    r.l28 = r.td;
    r.l32 = r.l28;
    let boxes = [("A2", r.a2), ("A3", r.a3), ("B2", r.b2), ("08", r.l08.base), ("17", r.l17), ("20", r.l20)];
    for (code, v) in boxes {
        if v < Decimal::ZERO {
            w.push(format!("line {code} is negative ({v}); the CA3 takes no negative amounts: enter the correction as a régularisation"));
        }
    }
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

    fn march() -> Period {
        Period { from: d(2026, 3, 1), to: d(2026, 3, 31) }
    }

    fn taxed(value: Decimal, percent: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Taxed, goods: true, taxable_value: value, percent, tva: (value * percent / dec!(100)).round_dp(2) }
    }

    fn untaxed(treatment: Treatment, goods: bool, value: Decimal, percent: Decimal) -> DocLine {
        DocLine { treatment, goods, taxable_value: value, percent, tva: dec!(0) }
    }

    fn sale(number: &str, kind: DocKind, country: Option<&str>, vat: Option<&str>, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 3, 10),
            party_name: format!("Client {number}"),
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
            date: d(2026, 3, 5),
            party_name: "Fournisseur".into(),
            party_country: country.map(str::to_owned),
            party_vat_id: None,
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn domestic_sales_purchases_and_regularisations() {
        let outward = [
            sale("1", DocKind::Invoice, None, None, vec![
                taxed(dec!(10000.50), dec!(20)),
                taxed(dec!(1000), dec!(10)),
                taxed(dec!(200), dec!(5.5)),
                taxed(dec!(100), dec!(2.1)),
            ]),
            sale("AV-1", DocKind::CreditNote, None, None, vec![taxed(dec!(1000), dec!(20))]),
            sale("2", DocKind::Invoice, None, None, vec![untaxed(Treatment::Exempt, false, dec!(800), dec!(0))]),
            sale("AV-2", DocKind::CreditNote, None, None, vec![untaxed(Treatment::Exempt, false, dec!(80), dec!(0))]),
        ];
        let inward = [
            purchase("F-1", DocKind::Invoice, None, vec![taxed(dec!(5000), dec!(20)), taxed(dec!(100), dec!(5.5))]),
            purchase("F-AV", DocKind::CreditNote, None, vec![taxed(dec!(500), dec!(20))]),
        ];
        let r = ca3(&outward, &inward, march(), Territory::Continent);
        // A1: 10000.50 + 1000 + 200 + 100 = 11300.50 → 11301; the avoirs in B5 and F8.
        assert_eq!((r.a1, r.b5, r.e2, r.f8), (dec!(11301), dec!(1000), dec!(800), dec!(80)));
        // 08: 10000.50 → 10001 → 2000.20 → 2000.
        assert_eq!((r.l08.base, r.l08.tax, r.l9b.base, r.l9b.tax, r.l09.base, r.l09.tax), (dec!(10001), dec!(2000), dec!(1000), dec!(100), dec!(200), dec!(11)));
        assert_eq!((r.t6.base, r.t6.tax), (dec!(100), dec!(2)), "2.1% in continental France is T6");
        // 15: the supplier's avoir (100); 16: 2000 + 100 + 11 + 2 + 100 = 2213.
        assert_eq!((r.l15, r.l16), (dec!(100), dec!(2213)));
        // 20: 1000 + 5.50 → 1006 (1005.50 rounds up); 21: the company's avoir, 200.
        assert_eq!((r.l20, r.l21, r.l23, r.td, r.l28, r.l25, r.l32), (dec!(1006), dec!(200), dec!(1206), dec!(1007), dec!(1007), dec!(0), dec!(1007)));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn eu_trade_exports_and_autoliquidation() {
        let outward = [
            sale("UE-1", DocKind::Invoice, Some("DE"), Some("DE136695976"), vec![untaxed(Treatment::IntraEu, true, dec!(3000), dec!(0))]),
            sale("EX-1", DocKind::Invoice, Some("US"), None, vec![untaxed(Treatment::Export, true, dec!(2000), dec!(0))]),
            sale("S-1", DocKind::Invoice, Some("BE"), Some("BE0123456749"), vec![untaxed(Treatment::OutOfScope, false, dec!(1500), dec!(0))]),
            sale("BTP-1", DocKind::Invoice, None, Some("FR40303265045"), vec![untaxed(Treatment::ReverseCharge, false, dec!(4000), dec!(0))]),
        ];
        let inward = [
            purchase("DE-1", DocKind::Invoice, Some("DE"), vec![untaxed(Treatment::IntraEu, true, dec!(1000), dec!(20))]),
            purchase("BE-1", DocKind::Invoice, Some("BE"), vec![untaxed(Treatment::ReverseCharge, false, dec!(200), dec!(20))]),
            purchase("US-1", DocKind::Invoice, Some("US"), vec![untaxed(Treatment::ReverseCharge, false, dec!(300), dec!(20))]),
            purchase("BTP-2", DocKind::Invoice, None, vec![untaxed(Treatment::ReverseCharge, false, dec!(500), dec!(20))]),
        ];
        let r = ca3(&outward, &inward, march(), Territory::Continent);
        assert_eq!((r.f2, r.e1, r.e2), (dec!(3000), dec!(2000), dec!(5500)));
        assert_eq!((r.b2, r.a3, r.a2), (dec!(1000), dec!(500), dec!(500)));
        assert_eq!((r.l08.base, r.l08.tax, r.l17), (dec!(2000), dec!(400), dec!(200)));
        assert_eq!((r.l16, r.l20, r.td, r.l25), (dec!(400), dec!(400), dec!(0), dec!(0)), "self-assessed TVA is deducted again");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn territories_old_rates_credit_and_warnings() {
        let lines = vec![taxed(dec!(1000), dec!(8.5)), taxed(dec!(100), dec!(2.1)), taxed(dec!(100), dec!(1.75))];
        let r = ca3(&[sale("R-1", DocKind::Invoice, None, None, lines.clone())], &[], march(), Territory::Dom);
        assert_eq!((r.l10.base, r.l10.tax, r.l11.base, r.l11.tax, r.t1.tax), (dec!(1000), dec!(85), dec!(100), dec!(2), dec!(2)));
        assert_eq!(r.l16, dec!(89));
        let r = ca3(&[sale("R-1", DocKind::Invoice, None, None, lines)], &[], march(), Territory::Continent);
        assert_eq!((r.other_rates.len(), r.t6.base, r.l16), (2, dec!(100), dec!(2)), "DOM rates have no line in continental France");
        let corse = vec![taxed(dec!(1000), dec!(10)), taxed(dec!(100), dec!(13)), taxed(dec!(100), dec!(0.9)), taxed(dec!(100), dec!(20))];
        let r = ca3(&[sale("C-1", DocKind::Invoice, None, None, corse)], &[], march(), Territory::Corse);
        assert_eq!((r.t3.tax, r.tc.tax, r.t5.tax, r.l08.tax, r.l9b.base), (dec!(100), dec!(13), dec!(1), dec!(20), dec!(0)));
        let r = ca3(&[sale("V-1", DocKind::Invoice, None, None, vec![taxed(dec!(1000), dec!(19.6))])], &[], march(), Territory::Continent);
        assert_eq!((r.l13.base, r.l13.tax), (dec!(1000), dec!(196)), "anciens taux");

        let mut cancelled = sale("X", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(20))]);
        cancelled.cancelled = true;
        let no_id = sale("UE-2", DocKind::Invoice, Some("IT"), None, vec![untaxed(Treatment::IntraEu, true, dec!(10), dec!(0))]);
        let big = purchase("F-2", DocKind::Invoice, None, vec![taxed(dec!(1000), dec!(20))]);
        let r = ca3(&[cancelled, no_id], &[big], march(), Territory::Continent);
        assert_eq!((r.f2, r.l16, r.l23, r.l25, r.l27, r.td, r.l32), (dec!(10), dec!(0), dec!(200), dec!(200), dec!(200), dec!(0), dec!(0)), "a crédit de TVA");
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("without the buyer's VAT identifier"));
        let back = purchase("DE-AV", DocKind::CreditNote, Some("DE"), vec![untaxed(Treatment::IntraEu, true, dec!(100), dec!(20))]);
        let r = ca3(&[], &[back], march(), Territory::Continent);
        assert!(r.warnings.iter().any(|w| w.contains("no negative amounts")), "{:?}", r.warnings);
        assert_eq!((euros(dec!(10.50)), euros(dec!(10.49)), euros(dec!(-10.50))), (dec!(11), dec!(10), dec!(-11)));
        let j = serde_json::to_value(ca3(&[], &[], march(), Territory::Continent)).unwrap();
        assert_eq!((j["l25"].as_str(), j["l08"]["base"].as_str()), (Some("0"), Some("0")));
    }
}

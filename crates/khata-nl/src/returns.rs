//! The aangifte omzetbelasting worked out from outward and inward
//! documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its
//! vouchers into documents. The rubrieken follow the Belastingdienst's
//! aangifte and its toelichting (`docs/research/netherlands-compliance.md`
//! §6).
//!
//! What is worked out (outward: the company's sales; inward: purchases):
//! - 1a / 1b: supplies and services taxed at 21% / 9% (omzet, btw);
//!   1c: other rates except 0% (corrections at an old rate);
//! - 1e: supplies taxed at 0% or not taxed by the company (domestic "btw
//!   verlegd" sales);
//! - 2a: supplies and services where the btw is shifted to the company
//!   (domestic reverse charge received), btw self-assessed;
//! - 3a: goods to countries outside the EU (uitvoer); 3b: goods to, and
//!   services in, other EU countries (the ICP-opgaaf lines);
//! - 4a: services (and goods under an art. 23 licence) from outside the
//!   EU; 4b: goods and services from EU countries; btw self-assessed;
//! - 5a: btw due (sum of the btw columns); 5b: voorbelasting (input tax on
//!   purchase invoices plus the self-assessed btw of 2a, 4a and 4b, all
//!   taken as deductible); 5c: 5a − 5b; 5g: the total to pay (positive) or
//!   to reclaim (negative).
//!
//! Not modelled (left at zero, with the reason in the docs): 1d
//! privégebruik, 3c installation and distance sales in the EU (the OSS),
//! 5d (KOR reduction), 5e / 5f (estimates), import VAT paid at customs, the
//! art. 23 licence, and partial deduction.
//!
//! Rounding: the aangifte takes whole euros. Bases are rounded down
//! (cents dropped, toward zero); the btw due is rounded down and the
//! voorbelasting up, which the Belastingdienst allows "in uw voordeel"
//! (research §6.2).
//!
//! Credit notes reduce, and debit notes add to, the period they are dated
//! in. None of this is tax advice: the figures are a preparation aid to be
//! checked before they are filed in Mijn Belastingdienst Zakelijk.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::btw::{REDUCED_PERCENT, STANDARD_PERCENT, Treatment};
use crate::provincies;

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply (a creditnota).
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
    /// Value excluding btw, in EUR.
    pub taxable_value: Decimal,
    /// The rate that applies: the rate charged on a taxed line, and on a
    /// reverse-charge or intra-EU purchase the rate the company owes.
    pub percent: Decimal,
    /// Btw booked on the line, in EUR; zero on untaxed lines.
    pub btw: Decimal,
}

/// A document the company issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The buyer's country (ISO 3166-1 alpha-2); `None`: the Netherlands.
    pub party_country: Option<String>,
    /// The buyer's btw-id or other VAT identifier.
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
    /// The supplier's country; `None`: the Netherlands.
    pub party_country: Option<String>,
    pub party_vat_id: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The period, both ends inclusive (a quarter, a month or a year).
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

/// The aangifte for a period. Field names are the rubrieken; `*_tax`
/// fields are the btw column beside the omzet column. Whole euros.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Aangifte {
    pub period: Option<Period>,
    /// Leveringen/diensten belast met hoog tarief.
    pub r1a: Decimal,
    pub r1a_tax: Decimal,
    /// Leveringen/diensten belast met laag tarief.
    pub r1b: Decimal,
    pub r1b_tax: Decimal,
    /// Leveringen/diensten belast met overige tarieven, behalve 0%.
    pub r1c: Decimal,
    pub r1c_tax: Decimal,
    /// Privégebruik (not worked out).
    pub r1d: Decimal,
    pub r1d_tax: Decimal,
    /// Leveringen/diensten belast met 0% of niet bij u belast.
    pub r1e: Decimal,
    /// Leveringen/diensten waarbij de heffing van omzetbelasting naar u is verlegd.
    pub r2a: Decimal,
    pub r2a_tax: Decimal,
    /// Leveringen naar landen buiten de EU (uitvoer).
    pub r3a: Decimal,
    /// Leveringen naar of diensten in landen binnen de EU.
    pub r3b: Decimal,
    /// Installatie/afstandsverkopen binnen de EU (not worked out).
    pub r3c: Decimal,
    /// Leveringen/diensten uit landen buiten de EU.
    pub r4a: Decimal,
    pub r4a_tax: Decimal,
    /// Leveringen/diensten uit landen binnen de EU.
    pub r4b: Decimal,
    pub r4b_tax: Decimal,
    /// Verschuldigde omzetbelasting (the btw columns of 1a to 4b).
    pub r5a: Decimal,
    /// Voorbelasting.
    pub r5b: Decimal,
    /// Subtotaal: 5a − 5b.
    pub r5c: Decimal,
    /// Vermindering volgens de kleineondernemersregeling (not worked out).
    pub r5d: Decimal,
    /// Schatting vorige aangifte(n) (not worked out).
    pub r5e: Decimal,
    /// Schatting deze aangifte (not worked out).
    pub r5f: Decimal,
    /// Totaal te betalen (positive) of terug te vragen (negative).
    pub r5g: Decimal,
    pub warnings: Vec<String>,
}

/// Whole euros, cents dropped (toward zero): the omzet columns.
pub fn whole_euros(a: Decimal) -> Decimal {
    a.trunc()
}

/// Btw due, rounded down to whole euros (in the taxpayer's favour).
pub fn tax_down(a: Decimal) -> Decimal {
    a.floor()
}

/// Voorbelasting, rounded up to whole euros (in the taxpayer's favour).
pub fn tax_up(a: Decimal) -> Decimal {
    a.ceil()
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Factuur",
        DocKind::CreditNote => "Creditnota",
        DocKind::DebitNote => "Debetnota",
    }
}

fn abroad_eu(country: Option<&str>) -> bool {
    country.is_some_and(|c| !provincies::is_netherlands(c) && provincies::in_eu(c))
}

fn abroad(country: Option<&str>) -> bool {
    country.is_some_and(|c| !c.trim().is_empty() && !provincies::is_netherlands(c))
}

fn has(id: &Option<String>) -> bool {
    id.as_deref().is_some_and(|s| !s.trim().is_empty())
}

#[derive(Default)]
struct Sums {
    b1a: Decimal,
    t1a: Decimal,
    b1b: Decimal,
    t1b: Decimal,
    b1c: Decimal,
    t1c: Decimal,
    b1e: Decimal,
    b2a: Decimal,
    t2a: Decimal,
    b3a: Decimal,
    b3b: Decimal,
    b4a: Decimal,
    t4a: Decimal,
    b4b: Decimal,
    t4b: Decimal,
    input: Decimal,
}

/// Work out the aangifte for a period from the documents dated in it.
pub fn aangifte(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> Aangifte {
    let mut w = Vec::new();
    let mut s = Sums::default();

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let sign = d.kind.sign();
        let eu = abroad_eu(d.party_country.as_deref());
        for l in &d.lines {
            let v = l.taxable_value * sign;
            match l.treatment {
                Treatment::Taxed => {
                    let t = l.btw * sign;
                    if l.percent == STANDARD_PERCENT {
                        s.b1a += v;
                        s.t1a += t;
                    } else if l.percent == REDUCED_PERCENT {
                        s.b1b += v;
                        s.t1b += t;
                    } else {
                        s.b1c += v;
                        s.t1c += t;
                    }
                }
                Treatment::Zero | Treatment::ReverseCharge => s.b1e += v,
                Treatment::Exempt => {}
                Treatment::IntraEu => {
                    if l.goods && !has(&d.party_vat_id) {
                        w.push(format!("{name}: an intra-EU supply of goods to {} without the buyer's VAT identifier is not 0%-rated; it is still put in rubriek 3b", d.party_name));
                    }
                    s.b3b += v;
                }
                Treatment::Export => {
                    if !l.goods {
                        w.push(format!("{name}: rubriek 3a is for goods; services to customers outside the EU are not on the aangifte, and this line is still put in 3a"));
                    }
                    s.b3a += v;
                }
                Treatment::OutOfScope => {
                    if eu && !l.goods && has(&d.party_vat_id) {
                        s.b3b += v;
                    }
                }
            }
            if !l.treatment.taxed() && !l.btw.is_zero() {
                w.push(format!("{name}: a line without btw treatment carries btw {}; it is left out", l.btw.normalize()));
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
                Treatment::Taxed => s.input += l.btw * sign,
                Treatment::ReverseCharge | Treatment::IntraEu | Treatment::Export => {
                    if l.percent.is_zero() {
                        w.push(format!("{name}: a reverse-charged purchase at 0% owes no btw; give it the rate the company owes"));
                    }
                    if abroad_eu(country) || (l.treatment == Treatment::IntraEu && !abroad(country)) {
                        if l.treatment == Treatment::IntraEu && !abroad(country) {
                            w.push(format!("{name}: an intra-EU purchase from a supplier without a foreign country is put in rubriek 4b"));
                        }
                        s.b4b += v;
                        s.t4b += owed;
                    } else if abroad(country) {
                        s.b4a += v;
                        s.t4a += owed;
                    } else {
                        s.b2a += v;
                        s.t2a += owed;
                    }
                }
                _ => {}
            }
            if !l.treatment.taxed() && !l.btw.is_zero() {
                w.push(format!("{name}: a line without btw treatment carries btw {}; it is left out", l.btw.normalize()));
            }
        }
    }

    let mut r = Aangifte { period: Some(period), ..Aangifte::default() };
    r.r1a = whole_euros(s.b1a);
    r.r1a_tax = tax_down(s.t1a);
    r.r1b = whole_euros(s.b1b);
    r.r1b_tax = tax_down(s.t1b);
    r.r1c = whole_euros(s.b1c);
    r.r1c_tax = tax_down(s.t1c);
    for (rubriek, base, booked, rate) in [("1a", s.b1a, s.t1a, STANDARD_PERCENT), ("1b", s.b1b, s.t1b, REDUCED_PERCENT)] {
        let computed = base * rate / Decimal::ONE_HUNDRED;
        if (computed - booked).abs() > Decimal::ONE {
            w.push(format!("rubriek {rubriek}: the btw booked is {}, but omzet × rate gives {}; check the rates on file", booked.round_dp(2), computed.round_dp(2)));
        }
    }
    r.r1e = whole_euros(s.b1e);
    r.r2a = whole_euros(s.b2a);
    r.r2a_tax = tax_down(s.t2a);
    r.r3a = whole_euros(s.b3a);
    r.r3b = whole_euros(s.b3b);
    r.r4a = whole_euros(s.b4a);
    r.r4a_tax = tax_down(s.t4a);
    r.r4b = whole_euros(s.b4b);
    r.r4b_tax = tax_down(s.t4b);
    r.r5a = r.r1a_tax + r.r1b_tax + r.r1c_tax + r.r1d_tax + r.r2a_tax + r.r4a_tax + r.r4b_tax;
    // The self-assessed btw is deducted again at its unrounded amount.
    r.r5b = tax_up(s.input + s.t2a + s.t4a + s.t4b);
    r.r5c = r.r5a - r.r5b;
    r.r5g = r.r5c - r.r5d - r.r5e + r.r5f;
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
        DocLine { treatment: Treatment::Taxed, goods: true, taxable_value: value, percent, btw: (value * percent / dec!(100)).round_dp(2) }
    }

    fn untaxed(treatment: Treatment, goods: bool, value: Decimal, percent: Decimal) -> DocLine {
        DocLine { treatment, goods, taxable_value: value, percent, btw: dec!(0) }
    }

    fn sale(number: &str, kind: DocKind, country: Option<&str>, vat: Option<&str>, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 2, 10),
            party_name: format!("Klant {number}"),
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
            party_name: "Leverancier".into(),
            party_country: country.map(str::to_owned),
            party_vat_id: None,
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn domestic_sales_and_purchases() {
        let outward = [
            sale("1", DocKind::Invoice, None, None, vec![taxed(dec!(10000.80), dec!(21)), taxed(dec!(1000.50), dec!(9))]),
            sale("C-1", DocKind::CreditNote, None, None, vec![taxed(dec!(1000), dec!(21))]),
            sale("2", DocKind::Invoice, None, None, vec![untaxed(Treatment::Exempt, false, dec!(800), dec!(0))]),
            sale("3", DocKind::Invoice, None, Some("NL004495445B01"), vec![untaxed(Treatment::ReverseCharge, false, dec!(2500), dec!(0))]),
        ];
        let inward = [
            purchase("I-1", DocKind::Invoice, None, vec![taxed(dec!(5000), dec!(21)), taxed(dec!(100.10), dec!(9))]),
            purchase("I-C", DocKind::CreditNote, None, vec![taxed(dec!(500), dec!(21))]),
        ];
        let r = aangifte(&outward, &inward, q1());
        // 1a: 10000.80 − 1000 = 9000.80 → 9000; btw 2100.17 − 210 = 1890.17 → 1890.
        assert_eq!((r.r1a, r.r1a_tax, r.r1b, r.r1b_tax), (dec!(9000), dec!(1890), dec!(1000), dec!(90)));
        assert_eq!(r.r1e, dec!(2500), "btw verlegd to a Dutch buyer");
        // 5b: 1050 + 9.01 − 105 = 954.01 → 955 (rounded up).
        assert_eq!((r.r5a, r.r5b, r.r5c, r.r5g), (dec!(1980), dec!(955), dec!(1025), dec!(1025)));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn eu_trade_exports_and_reverse_charge() {
        let outward = [
            sale("EU-1", DocKind::Invoice, Some("DE"), Some("DE136695976"), vec![untaxed(Treatment::IntraEu, true, dec!(3000.99), dec!(0))]),
            sale("EX-1", DocKind::Invoice, Some("US"), None, vec![untaxed(Treatment::Export, true, dec!(2000), dec!(0))]),
            sale("S-1", DocKind::Invoice, Some("BE"), Some("BE0123456749"), vec![untaxed(Treatment::OutOfScope, false, dec!(1500), dec!(0))]),
            sale("S-2", DocKind::Invoice, Some("US"), None, vec![untaxed(Treatment::OutOfScope, false, dec!(700), dec!(0))]),
        ];
        let inward = [
            purchase("DE-1", DocKind::Invoice, Some("DE"), vec![untaxed(Treatment::IntraEu, true, dec!(1000), dec!(21))]),
            purchase("BE-1", DocKind::Invoice, Some("BE"), vec![untaxed(Treatment::ReverseCharge, false, dec!(200), dec!(21))]),
            purchase("US-1", DocKind::Invoice, Some("US"), vec![untaxed(Treatment::ReverseCharge, false, dec!(300), dec!(21))]),
            purchase("BOUW-1", DocKind::Invoice, None, vec![untaxed(Treatment::ReverseCharge, false, dec!(4000), dec!(21))]),
        ];
        let r = aangifte(&outward, &inward, q1());
        assert_eq!((r.r3a, r.r3b), (dec!(2000), dec!(4500)), "goods to DE and the service to BE are in 3b; S-2 is not reported");
        assert_eq!((r.r4b, r.r4b_tax, r.r4a, r.r4a_tax, r.r2a, r.r2a_tax), (dec!(1200), dec!(252), dec!(300), dec!(63), dec!(4000), dec!(840)));
        assert_eq!((r.r5a, r.r5b, r.r5g), (dec!(1155), dec!(1155), dec!(0)), "self-assessed btw is deducted again");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn rounding_period_and_warnings() {
        let mut outside = sale("0", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(21))]);
        outside.date = d(2026, 4, 1);
        let mut cancelled = sale("X", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(21))]);
        cancelled.cancelled = true;
        let old_rate = sale("C-2018", DocKind::CreditNote, None, None, vec![taxed(dec!(100), dec!(6))]);
        let no_id = sale("EU-2", DocKind::Invoice, Some("IT"), None, vec![untaxed(Treatment::IntraEu, true, dec!(10), dec!(0))]);
        let r = aangifte(&[outside, cancelled, old_rate, no_id], &[], q1());
        assert_eq!((r.r1a, r.r1c, r.r1c_tax, r.r3b), (dec!(0), dec!(-100), dec!(-6), dec!(10)));
        assert_eq!(r.r5g, dec!(-6), "a refund is negative");
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("without the buyer's VAT identifier"));
        assert_eq!((whole_euros(dec!(10.99)), tax_down(dec!(10.99)), tax_up(dec!(10.01))), (dec!(10), dec!(10), dec!(11)));
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["r1c"].as_str(), j["r5g"].as_str()), (Some("-100"), Some("-6")));
    }
}

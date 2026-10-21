//! The Umsatzsteuer-Voranmeldung (UStVA) worked out from outward and
//! inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its
//! vouchers into documents. The Kennzahlen (Kz) follow the official form
//! "USt 1 A" (`docs/research/germany-compliance.md` §6).
//!
//! What is worked out (outward: the company's sales; inward: purchases):
//! - Kz 81 / 86: taxable supplies at 19% / 7% (base); their tax is base ×
//!   rate, as the form computes it. Kz 35 / 36: other rates (16% and 5% of
//!   2H 2020, corrections), base and the tax booked. Kz 87: taxable at 0%
//!   (§12 Abs. 3, photovoltaics).
//! - Kz 41: tax-free intra-EU supplies to buyers with a USt-IdNr; Kz 43:
//!   other tax-free supplies with input-tax deduction (exports); Kz 48:
//!   tax-free supplies without input-tax deduction (§4 Nr. 8 ff.).
//! - Kz 60: supplies where the recipient owes the tax (§13b, seller side).
//! - Kz 21: services taxable in another EU state reported on the
//!   Zusammenfassende Meldung (§18b, buyer an EU business); Kz 45: other
//!   supplies not taxable in Germany.
//! - Kz 89 / 93: intra-EU acquisitions at 19% / 7% (base), tax base × rate,
//!   deducted again in Kz 61.
//! - Kz 46 / 47: services from an EU supplier under §13b Abs. 1 (base,
//!   tax); Kz 84 / 85: other §13b supplies received (base, tax). Both
//!   deducted again in Kz 67.
//! - Kz 66: input tax on purchase invoices (all taken as deductible).
//! - Kz 83: the remaining prepayment (positive) or refund (negative):
//!   output tax − input tax.
//!
//! Bases are whole euros (cents dropped, toward zero); taxes are euros and
//! cents. Credit notes reduce, and debit notes add to, the period they are
//! dated in (§17 UStG corrects in the period of the change).
//!
//! Not modelled: Kz 42 (triangular trade), 44/49 (new vehicles), 73/74
//! (§13b goods), 59/64/65/63 and the Sondervorauszahlung (Kz 39),
//! Istversteuerung, partial input-tax deduction and §15a corrections.
//!
//! None of this is legal advice: the figures are a preparation aid and
//! must be checked before they are filed through ELSTER.

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::laender;
use crate::ust::{REDUCED_PERCENT, STANDARD_PERCENT, Treatment};

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply (a credit note, Gutschrift im kaufmännischen Sinn).
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
    /// Value excluding USt, in EUR.
    pub taxable_value: Decimal,
    /// The rate that applies: the rate charged on a taxed line, and on a
    /// reverse-charge or intra-EU purchase the rate the company owes.
    pub percent: Decimal,
    /// USt booked on the line, in EUR; zero on untaxed lines.
    pub ust: Decimal,
}

/// A document the company issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The buyer's country (ISO 3166-1 alpha-2); `None`: Germany.
    pub party_country: Option<String>,
    /// The buyer's USt-IdNr or other VAT identifier.
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
    /// The supplier's country; `None`: Germany.
    pub party_country: Option<String>,
    pub party_vat_id: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The UStVA period, both ends inclusive (a month or a quarter).
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

/// The UStVA for a period. Field names are the Kennzahlen; `*_tax` fields
/// are the tax column beside a base.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Ustva {
    pub period: Option<Period>,
    /// Steuerpflichtige Umsätze zum Steuersatz von 19 Prozent (base, whole euros).
    pub kz81: Decimal,
    pub kz81_tax: Decimal,
    /// Steuerpflichtige Umsätze zum Steuersatz von 7 Prozent (base).
    pub kz86: Decimal,
    pub kz86_tax: Decimal,
    /// Steuerpflichtige Umsätze zu anderen Steuersätzen (base).
    pub kz35: Decimal,
    /// Steuer auf Kz 35.
    pub kz36: Decimal,
    /// Steuerpflichtige Umsätze zum Steuersatz von 0 Prozent (§12 Abs. 3).
    pub kz87: Decimal,
    /// Steuerfreie innergemeinschaftliche Lieferungen an Abnehmer mit USt-IdNr.
    pub kz41: Decimal,
    /// Weitere steuerfreie Umsätze mit Vorsteuerabzug (Ausfuhrlieferungen…).
    pub kz43: Decimal,
    /// Steuerfreie Umsätze ohne Vorsteuerabzug.
    pub kz48: Decimal,
    /// Übrige steuerpflichtige Umsätze, für die der Leistungsempfänger die Steuer nach §13b schuldet.
    pub kz60: Decimal,
    /// Nicht steuerbare sonstige Leistungen gem. §18b Satz 1 Nr. 2 (EU business customers).
    pub kz21: Decimal,
    /// Übrige nicht steuerbare Umsätze (Leistungsort nicht im Inland).
    pub kz45: Decimal,
    /// Steuerpflichtige innergemeinschaftliche Erwerbe zum Steuersatz von 19 Prozent (base).
    pub kz89: Decimal,
    pub kz89_tax: Decimal,
    /// … zum Steuersatz von 7 Prozent (base).
    pub kz93: Decimal,
    pub kz93_tax: Decimal,
    /// Sonstige Leistungen eines im übrigen Gemeinschaftsgebiet ansässigen Unternehmers (§13b Abs. 1), base.
    pub kz46: Decimal,
    /// Steuer auf Kz 46.
    pub kz47: Decimal,
    /// Andere Leistungen (§13b Abs. 2 Nr. 1, 2, 4 bis 12), base.
    pub kz84: Decimal,
    /// Steuer auf Kz 84.
    pub kz85: Decimal,
    /// Vorsteuerbeträge aus Rechnungen von anderen Unternehmern.
    pub kz66: Decimal,
    /// Vorsteuerbeträge aus dem innergemeinschaftlichen Erwerb von Gegenständen.
    pub kz61: Decimal,
    /// Vorsteuerbeträge aus Leistungen im Sinne des §13b.
    pub kz67: Decimal,
    /// Umsatzsteuer: every tax column above (output side).
    pub output_tax: Decimal,
    /// Abziehbare Vorsteuerbeträge: Kz 66 + 61 + 67.
    pub input_tax: Decimal,
    /// Verbleibende Umsatzsteuer-Vorauszahlung (positive) bzw. verbleibender Überschuss (negative).
    pub kz83: Decimal,
    pub warnings: Vec<String>,
}

/// Whole euros, cents dropped (toward zero): the base columns.
pub fn whole_euros(a: Decimal) -> Decimal {
    a.trunc()
}

/// Euros and cents, half away from zero: the tax columns.
fn cents(a: Decimal) -> Decimal {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Rechnung",
        DocKind::CreditNote => "Gutschrift/Storno",
        DocKind::DebitNote => "Belastung",
    }
}

fn abroad_eu(country: Option<&str>) -> bool {
    country.is_some_and(|c| !laender::is_germany(c) && laender::in_eu(c))
}

fn has(id: &Option<String>) -> bool {
    id.as_deref().is_some_and(|s| !s.trim().is_empty())
}

/// Sums before rounding.
#[derive(Default)]
struct Sums {
    b81: Decimal,
    t81: Decimal,
    b86: Decimal,
    t86: Decimal,
    b35: Decimal,
    t36: Decimal,
    b87: Decimal,
    b41: Decimal,
    b43: Decimal,
    b48: Decimal,
    b60: Decimal,
    b21: Decimal,
    b45: Decimal,
    b89: Decimal,
    b93: Decimal,
    b46: Decimal,
    t47: Decimal,
    b84: Decimal,
    t85: Decimal,
    t66: Decimal,
}

/// Work out the UStVA for a period from the documents dated in it.
pub fn ustva(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> Ustva {
    let mut w = Vec::new();
    let mut s = Sums::default();

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let sign = d.kind.sign();
        let eu = abroad_eu(d.party_country.as_deref());
        for l in &d.lines {
            let v = l.taxable_value * sign;
            match l.treatment {
                Treatment::Standard | Treatment::Reduced => {
                    if l.percent == STANDARD_PERCENT {
                        s.b81 += v;
                        s.t81 += l.ust * sign;
                    } else if l.percent == REDUCED_PERCENT {
                        s.b86 += v;
                        s.t86 += l.ust * sign;
                    } else {
                        s.b35 += v;
                        s.t36 += l.ust * sign;
                    }
                }
                Treatment::Zero => s.b87 += v,
                Treatment::Exempt => s.b48 += v,
                Treatment::ReverseCharge => s.b60 += v,
                Treatment::IntraEu => {
                    if !has(&d.party_vat_id) {
                        w.push(format!("{name}: an intra-EU supply to {} without the buyer's VAT identifier is not tax-free (§6a Abs. 1 Nr. 4 UStG); it is still put in Kz 41", d.party_name));
                    }
                    if !l.goods {
                        w.push(format!("{name}: an intra-EU supply in Kz 41 is a supply of goods; services to EU businesses go in Kz 21"));
                    }
                    s.b41 += v;
                }
                Treatment::Export => s.b43 += v,
                Treatment::OutOfScope => {
                    if eu && !l.goods && has(&d.party_vat_id) {
                        s.b21 += v;
                    } else {
                        s.b45 += v;
                    }
                }
            }
            if !l.treatment.taxed() && !l.ust.is_zero() {
                w.push(format!("{name}: a line without USt treatment carries USt {}; it is left out", l.ust.normalize()));
            }
        }
    }

    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {} ({})", label(d.kind), d.number, d.party_name);
        let sign = d.kind.sign();
        let eu = abroad_eu(d.party_country.as_deref());
        for l in &d.lines {
            let v = l.taxable_value * sign;
            match l.treatment {
                Treatment::Standard | Treatment::Reduced => s.t66 += l.ust * sign,
                Treatment::ReverseCharge => {
                    let tax = l.taxable_value * l.percent / Decimal::ONE_HUNDRED * sign;
                    if eu && !l.goods {
                        s.b46 += v;
                        s.t47 += tax;
                    } else {
                        s.b84 += v;
                        s.t85 += tax;
                    }
                }
                Treatment::IntraEu => {
                    if l.percent == REDUCED_PERCENT {
                        s.b93 += v;
                    } else {
                        if l.percent != STANDARD_PERCENT {
                            w.push(format!("{name}: an intra-EU acquisition at {}% is reported at 19% (Kz 89); other rates go in Kz 95/98, which are not worked out", l.percent.normalize()));
                        }
                        s.b89 += v;
                    }
                }
                _ => {}
            }
            if !l.treatment.taxed() && !l.ust.is_zero() {
                w.push(format!("{name}: a line without USt treatment carries USt {}; it is left out", l.ust.normalize()));
            }
        }
    }

    let base_tax = |b: Decimal, rate: Decimal| cents(whole_euros(b) * rate / Decimal::ONE_HUNDRED);
    let mut r = Ustva { period: Some(period), ..Ustva::default() };
    r.kz81 = whole_euros(s.b81);
    r.kz81_tax = base_tax(s.b81, STANDARD_PERCENT);
    r.kz86 = whole_euros(s.b86);
    r.kz86_tax = base_tax(s.b86, REDUCED_PERCENT);
    for (kz, computed, booked) in [("81", r.kz81_tax, s.t81), ("86", r.kz86_tax, s.t86)] {
        if (computed - booked).abs() > Decimal::ONE {
            w.push(format!("Kz {kz}: the USt booked is {}, but the base × rate gives {computed}; check the rates on file", cents(booked)));
        }
    }
    r.kz35 = whole_euros(s.b35);
    r.kz36 = cents(s.t36);
    r.kz87 = whole_euros(s.b87);
    r.kz41 = whole_euros(s.b41);
    r.kz43 = whole_euros(s.b43);
    r.kz48 = whole_euros(s.b48);
    r.kz60 = whole_euros(s.b60);
    r.kz21 = whole_euros(s.b21);
    r.kz45 = whole_euros(s.b45);
    r.kz89 = whole_euros(s.b89);
    r.kz89_tax = base_tax(s.b89, STANDARD_PERCENT);
    r.kz93 = whole_euros(s.b93);
    r.kz93_tax = base_tax(s.b93, REDUCED_PERCENT);
    r.kz46 = whole_euros(s.b46);
    r.kz47 = cents(s.t47);
    r.kz84 = whole_euros(s.b84);
    r.kz85 = cents(s.t85);
    r.kz66 = cents(s.t66);
    r.kz61 = r.kz89_tax + r.kz93_tax;
    r.kz67 = r.kz47 + r.kz85;
    r.output_tax = r.kz81_tax + r.kz86_tax + r.kz36 + r.kz89_tax + r.kz93_tax + r.kz47 + r.kz85;
    r.input_tax = r.kz66 + r.kz61 + r.kz67;
    r.kz83 = r.output_tax - r.input_tax;
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
        let treatment = if percent == dec!(7) { Treatment::Reduced } else { Treatment::Standard };
        DocLine { treatment, goods: true, taxable_value: value, percent, ust: (value * percent / dec!(100)).round_dp(2) }
    }

    fn untaxed(treatment: Treatment, goods: bool, value: Decimal, percent: Decimal) -> DocLine {
        DocLine { treatment, goods, taxable_value: value, percent, ust: dec!(0) }
    }

    fn sale(number: &str, kind: DocKind, country: Option<&str>, vat: Option<&str>, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 3, 10),
            party_name: format!("Kunde {number}"),
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
            party_name: "Lieferant".into(),
            party_country: country.map(str::to_owned),
            party_vat_id: Some("ATU13585627".into()),
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn domestic_sales_and_purchases() {
        let outward = [
            sale("1", DocKind::Invoice, None, None, vec![taxed(dec!(10000.80), dec!(19)), taxed(dec!(1000), dec!(7))]),
            sale("G-1", DocKind::CreditNote, None, None, vec![taxed(dec!(1000), dec!(19))]),
            sale("2", DocKind::Invoice, None, None, vec![untaxed(Treatment::Exempt, false, dec!(800), dec!(0))]),
        ];
        let inward = [
            purchase("E-1", DocKind::Invoice, None, vec![taxed(dec!(5000), dec!(19)), taxed(dec!(100), dec!(7))]),
            purchase("E-G", DocKind::CreditNote, None, vec![taxed(dec!(500), dec!(19))]),
        ];
        let r = ustva(&outward, &inward, march());
        // 81: 10000.80 − 1000 = 9000.80 → 9000; tax 1710.00. 86: 1000 → 70.00.
        assert_eq!((r.kz81, r.kz81_tax, r.kz86, r.kz86_tax, r.kz48), (dec!(9000), dec!(1710.00), dec!(1000), dec!(70.00), dec!(800)));
        // 66: 950 + 7 − 95 = 862.
        assert_eq!((r.kz66, r.input_tax), (dec!(862.00), dec!(862.00)));
        assert_eq!((r.output_tax, r.kz83), (dec!(1780.00), dec!(918.00)));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn eu_trade_exports_and_reverse_charge() {
        let outward = [
            sale("EU-1", DocKind::Invoice, Some("FR"), Some("FR40303265045"), vec![untaxed(Treatment::IntraEu, true, dec!(3000.99), dec!(0))]),
            sale("EX-1", DocKind::Invoice, Some("CH"), None, vec![untaxed(Treatment::Export, true, dec!(2000), dec!(0))]),
            sale("S-1", DocKind::Invoice, Some("AT"), Some("ATU13585627"), vec![untaxed(Treatment::OutOfScope, false, dec!(1500), dec!(0))]),
            sale("S-2", DocKind::Invoice, Some("US"), None, vec![untaxed(Treatment::OutOfScope, false, dec!(700), dec!(0))]),
            sale("B-1", DocKind::Invoice, None, Some("DE811569869"), vec![untaxed(Treatment::ReverseCharge, false, dec!(4000), dec!(0))]),
        ];
        let inward = [
            purchase("AT-1", DocKind::Invoice, Some("AT"), vec![untaxed(Treatment::ReverseCharge, false, dec!(1000), dec!(19))]),
            purchase("BAU-1", DocKind::Invoice, None, vec![untaxed(Treatment::ReverseCharge, false, dec!(2000), dec!(19))]),
            purchase("NL-1", DocKind::Invoice, Some("NL"), vec![
                untaxed(Treatment::IntraEu, true, dec!(1000), dec!(19)),
                untaxed(Treatment::IntraEu, true, dec!(200), dec!(7)),
            ]),
        ];
        let r = ustva(&outward, &inward, march());
        assert_eq!((r.kz41, r.kz43, r.kz21, r.kz45, r.kz60), (dec!(3000), dec!(2000), dec!(1500), dec!(700), dec!(4000)));
        assert_eq!((r.kz46, r.kz47, r.kz84, r.kz85, r.kz67), (dec!(1000), dec!(190.00), dec!(2000), dec!(380.00), dec!(570.00)));
        assert_eq!((r.kz89, r.kz89_tax, r.kz93, r.kz93_tax, r.kz61), (dec!(1000), dec!(190.00), dec!(200), dec!(14.00), dec!(204.00)));
        assert_eq!(r.output_tax, r.input_tax, "self-assessed tax is deducted again");
        assert_eq!(r.kz83, dec!(0));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn other_rates_period_cancellation_and_warnings() {
        let mut outside = sale("0", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(19))]);
        outside.date = d(2026, 4, 1);
        let mut cancelled = sale("C", DocKind::Invoice, None, None, vec![taxed(dec!(999), dec!(19))]);
        cancelled.cancelled = true;
        let old_rate = sale("K-2020", DocKind::CreditNote, None, None, vec![taxed(dec!(100), dec!(16))]);
        let no_id = sale("EU-2", DocKind::Invoice, Some("IT"), None, vec![untaxed(Treatment::IntraEu, true, dec!(10), dec!(0))]);
        let wrong = sale("Z", DocKind::Invoice, None, None, vec![DocLine { ust: dec!(5), ..untaxed(Treatment::Exempt, false, dec!(100), dec!(0)) }]);
        let r = ustva(&[outside, cancelled, old_rate, no_id, wrong], &[], march());
        assert_eq!((r.kz81, r.kz35, r.kz36, r.kz41, r.kz48), (dec!(0), dec!(-100), dec!(-16.00), dec!(10), dec!(100)));
        assert_eq!(r.kz83, dec!(-16.00), "a refund is negative");
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("without the buyer's VAT identifier"));
        assert!(r.warnings[1].contains("carries USt 5"));
    }

    #[test]
    fn whole_euros_drop_the_cents_and_serialise_as_strings() {
        assert_eq!(whole_euros(dec!(1234.99)), dec!(1234));
        assert_eq!(whole_euros(dec!(-1234.99)), dec!(-1234));
        let r = ustva(&[sale("1", DocKind::Invoice, None, None, vec![taxed(dec!(99.99), dec!(19))])], &[], march());
        assert_eq!((r.kz81, r.kz81_tax), (dec!(99), dec!(18.81)), "the tax column follows the whole-euro base");
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["kz81"].as_str(), j["kz81_tax"].as_str()), (Some("99"), Some("18.81")));
        assert_eq!(j["period"]["from"], "2026-03-01");
    }
}

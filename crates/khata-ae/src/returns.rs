//! The VAT 201 return worked out from outward and inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its vouchers
//! into documents. The boxes follow `docs/research/mena-compliance.md` §1.4
//! (FTA VAT returns guide and EmaraTax practice [SECONDARY]; the box labels
//! were not checked on the live portal [UNCERTAIN]).
//!
//! What is worked out, and every simplification:
//! - Boxes 1a to 1g: standard-rated supplies (amount and VAT) by the emirate
//!   of the establishment that made the supply. Credit notes reduce, and
//!   debit notes add to, the period they are dated in. The "adjustment"
//!   column (bad-debt relief and the like) is not modelled.
//! - Box 2 (tourist refunds, pre-filled by the FTA): not supported, always 0.
//! - Box 3 (supplies subject to the reverse charge): purchase lines marked
//!   "RC:" ([`Mechanism::ReverseCharge`]): standard- and zero-rated services
//!   and goods from abroad not imported through UAE Customs, and domestic
//!   reverse-charge goods (FTA VAT Returns User Guide v4.0 pp.15-16
//!   [PRIMARY]; exempt lines are not reverse-charged).
//!   The VAT is what the app works out at the rate class; no supplier TRN
//!   is needed. A sale line marked "RC:" (the business is the supplier of
//!   domestic reverse-charge goods) goes in its Box 1 row with its value and
//!   no VAT [UNCERTAIN: see the research doc], and is totalled in
//!   `reverse_charge_sales` for reference.
//! - Boxes 4 and 5: zero-rated and exempt supplies (amount only).
//! - Box 6: goods imported through UAE Customs, purchase invoices with lines
//!   marked "IMPORT:" ([`Mechanism::Import`]). The FTA pre-fills Box 6 from
//!   the customs declarations; a warning asks to compare. Box 7: supplier
//!   credit and debit notes on such lines.
//! - Box 8: totals of Boxes 1 to 7.
//! - Box 9: standard-rated expenses and their VAT, all taken as recoverable.
//!   Blocked input tax (the blocked categories are not in the research;
//!   CD 149/2026 changes the employee-benefit limits), the cash-payment
//!   block of CD 149/2026 and input-tax apportionment for businesses with exempt
//!   supplies are NOT applied: the user must adjust for them. Purchases from
//!   a supplier without a TRN are left out (no valid tax invoice to recover
//!   from), with a warning when they carry VAT.
//! - Box 10: the full value and the VAT of Boxes 3, 6 and 7 (guide
//!   pp.22-23), all VAT taken as recoverable (the same blocking and
//!   apportionment caveats as Box 9; a warning says so). Zero-rated import
//!   lines are left out: the FTA pre-fills Box 6 at 5% and the correction
//!   belongs in Box 7, by hand.
//! - Box 11: totals of Boxes 9 and 10.
//! - Boxes 12 to 14: total VAT due (Box 8 VAT), total recoverable (Box 11
//!   VAT), and the net: positive is payable, negative is recoverable.
//!
//! None of this is legal advice: the figures are a preparation aid and must
//! be checked before filing on EmaraTax.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::emirates::{self, EMIRATES};
use crate::vat::{Mechanism, Treatment};

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply (a tax credit note).
    CreditNote,
    /// Adds to the supply.
    DebitNote,
}

impl DocKind {
    /// +1 for invoices and debit notes, −1 for credit notes.
    pub fn sign(self) -> Decimal {
        if self == DocKind::CreditNote { Decimal::NEGATIVE_ONE } else { Decimal::ONE }
    }
}

/// One line of a document. Amounts are positive; the document kind says
/// whether they add or reduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLine {
    pub treatment: Treatment,
    /// Value excluding VAT, in AED.
    pub taxable_value: Decimal,
    /// VAT in AED; zero on zero-rated and exempt lines. On a reverse-charge
    /// or import line, the VAT the business accounts for itself (the app
    /// works it out from the rate class).
    pub vat: Decimal,
    /// Reverse charge or import ([`Mechanism`]); [`Mechanism::Normal`] for
    /// everything else.
    pub mechanism: Mechanism,
}

/// A document the business issued: a tax invoice, credit note or debit note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    /// The app's own reference (a voucher id).
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The emirate code ([`crate::emirates`]) of the establishment that made
    /// the supply: it picks the Box 1 row.
    pub emirate: String,
    /// Cancelled documents are left out.
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// A document received from a supplier. `kind` is from the supplier's
/// point of view: invoices and debit notes add, credit notes reduce. A
/// purchase return the business records as its own debit note is the
/// counterpart of the supplier's credit note, so the app maps it to
/// [`DocKind::CreditNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The supplier's TRN; `None` for an unregistered supplier.
    pub party_trn: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The tax period, both ends inclusive.
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

/// An amount and its VAT, in AED.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BoxAmounts {
    pub amount: Decimal,
    pub vat: Decimal,
}

impl BoxAmounts {
    fn add(&mut self, o: BoxAmounts) {
        self.amount += o.amount;
        self.vat += o.vat;
    }
}

/// One of Boxes 1a to 1g.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EmirateRow {
    /// "1a" to "1g".
    #[serde(rename = "box")]
    pub box_id: &'static str,
    pub emirate: &'static str,
    pub name: &'static str,
    pub amounts: BoxAmounts,
}

/// VAT 201 for a period. Field comments give the box numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Vat201 {
    pub period: Period,
    /// 1a to 1g, always all seven emirates in box order.
    pub standard_rated: Vec<EmirateRow>,
    /// 2: not supported, always 0.
    pub tourist_refunds: BoxAmounts,
    /// 3: purchases under the reverse charge (lines marked "RC:"), with
    /// the VAT the business accounts for.
    pub reverse_charge_supplies: BoxAmounts,
    /// 4 (amount only).
    pub zero_rated: Decimal,
    /// 5 (amount only).
    pub exempt: Decimal,
    /// 6: goods imported through UAE Customs (purchase invoices with lines
    /// marked "IMPORT:"). The FTA pre-fills this box from customs data.
    pub imports: BoxAmounts,
    /// 7: adjustments to imports: supplier credit and debit notes on
    /// "IMPORT:" lines.
    pub import_adjustments: BoxAmounts,
    /// 8: Boxes 1 to 7.
    pub total_supplies: BoxAmounts,
    /// 9: standard-rated expenses and recoverable VAT (no blocking or
    /// apportionment applied).
    pub standard_expenses: BoxAmounts,
    /// 10: recoverable VAT on Boxes 3, 6 and 7, all taken as recoverable.
    pub reverse_charge_recovery: BoxAmounts,
    /// 11: Boxes 9 and 10.
    pub total_expenses: BoxAmounts,
    /// 12: total VAT due (Box 8 VAT).
    pub total_due: Decimal,
    /// 13: total recoverable VAT (Box 11 VAT).
    pub total_recoverable: Decimal,
    /// 14: Box 12 − Box 13. Positive: payable; negative: recoverable.
    pub net: Decimal,
    /// For reference: the value of domestic reverse-charge supplies the
    /// business made (already in its Box 1 row, with no VAT).
    pub reverse_charge_sales: Decimal,
    pub warnings: Vec<String>,
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Invoice",
        DocKind::CreditNote => "Credit note",
        DocKind::DebitNote => "Debit note",
    }
}

/// Work out VAT 201 for a period from the documents dated in it.
pub fn vat201(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> Vat201 {
    let mut warnings = Vec::new();
    let mut by_emirate: BTreeMap<&'static str, BoxAmounts> = BTreeMap::new();
    let mut zero_rated = Decimal::ZERO;
    let mut exempt = Decimal::ZERO;
    let mut reverse_charge_sales = Decimal::ZERO;

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let sign = d.kind.sign();
        let emirate = emirates::emirate(&d.emirate);
        if emirate.is_none() && d.lines.iter().any(|l| l.treatment == Treatment::Standard) {
            warnings.push(format!(
                "{name}: {:?} is not an emirate code, so its standard-rated lines are left out of Box 1",
                d.emirate
            ));
        }
        for l in &d.lines {
            let signed = BoxAmounts { amount: l.taxable_value * sign, vat: l.vat * sign };
            if l.mechanism != Mechanism::Normal {
                // A domestic reverse-charge supply: the recipient accounts
                // for the VAT; the supplier reports the value only.
                if l.mechanism == Mechanism::Import {
                    warnings.push(format!("{name}: a sale line is marked IMPORT; it is treated as a reverse-charge supply"));
                }
                if !l.vat.is_zero() {
                    warnings.push(format!(
                        "{name}: a reverse-charge sale line carries VAT {}; the recipient accounts for it, so it is left out",
                        l.vat.normalize()
                    ));
                }
                match emirate {
                    Some(e) => {
                        by_emirate.entry(e.code).or_default().add(BoxAmounts { amount: signed.amount, vat: Decimal::ZERO })
                    }
                    None => warnings.push(format!(
                        "{name}: {:?} is not an emirate code, so its reverse-charge lines are left out of Box 1",
                        d.emirate
                    )),
                }
                reverse_charge_sales += signed.amount;
                continue;
            }
            match (l.treatment, emirate) {
                (Treatment::Standard, Some(e)) => by_emirate.entry(e.code).or_default().add(signed),
                (Treatment::Standard, None) => {}
                (Treatment::ZeroRated, _) => zero_rated += signed.amount,
                (Treatment::Exempt, _) => exempt += signed.amount,
            }
            if l.treatment != Treatment::Standard && !l.vat.is_zero() {
                warnings.push(format!(
                    "{name}: a {} line carries VAT {}; that VAT is left out of the return",
                    if l.treatment == Treatment::Exempt { "exempt" } else { "zero-rated" },
                    l.vat.normalize()
                ));
            }
        }
    }

    let mut standard_expenses = BoxAmounts::default();
    let (mut reverse_charge_supplies, mut imports, mut import_adjustments) =
        (BoxAmounts::default(), BoxAmounts::default(), BoxAmounts::default());
    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        let mut sum = BoxAmounts::default();
        for l in &d.lines {
            let signed = BoxAmounts { amount: l.taxable_value * sign, vat: l.vat * sign };
            match l.mechanism {
                Mechanism::Normal if l.treatment == Treatment::Standard => sum.add(BoxAmounts { amount: l.taxable_value, vat: l.vat }),
                Mechanism::Normal => {}
                // Self-accounted: no supplier TRN or tax invoice needed.
                // Box 3 takes standard- and zero-rated services from abroad
                // (FTA VAT Returns User Guide v4.0 pp.15-16); exempt ones are
                // not reverse-charged.
                Mechanism::ReverseCharge if l.treatment != Treatment::Exempt => reverse_charge_supplies.add(signed),
                Mechanism::Import if l.treatment == Treatment::Standard => {
                    if d.kind == DocKind::Invoice { imports.add(signed) } else { import_adjustments.add(signed) }
                }
                Mechanism::ReverseCharge | Mechanism::Import => {}
            }
        }
        if d.party_trn.as_deref().is_none_or(|t| t.trim().is_empty()) {
            if !sum.vat.is_zero() {
                warnings.push(format!(
                    "Purchase {} from {} has VAT but no supplier TRN; it is left out of Box 9",
                    d.number, d.party_name
                ));
            }
            continue;
        }
        standard_expenses.add(BoxAmounts { amount: sum.amount * sign, vat: sum.vat * sign });
    }
    if !imports.amount.is_zero() || !import_adjustments.amount.is_zero() {
        warnings.push(
            "Box 6 is pre-filled by the FTA from the customs declarations: compare it with the imports worked out here, and put any difference in Box 7"
                .to_owned(),
        );
    }
    if !reverse_charge_supplies.vat.is_zero() || !imports.vat.is_zero() || !import_adjustments.vat.is_zero() {
        warnings.push(
            "All reverse-charge and import VAT is shown as recoverable in Box 10: reduce it for blocked input tax and apportionment"
                .to_owned(),
        );
    }

    let standard_rated: Vec<EmirateRow> = EMIRATES
        .iter()
        .map(|e| EmirateRow {
            box_id: e.vat201_box,
            emirate: e.code,
            name: e.name,
            amounts: by_emirate.get(e.code).copied().unwrap_or_default(),
        })
        .collect();
    let tourist_refunds = BoxAmounts::default();
    let mut total_supplies = BoxAmounts::default();
    for r in &standard_rated {
        total_supplies.add(r.amounts);
    }
    for b in [tourist_refunds, reverse_charge_supplies, imports, import_adjustments] {
        total_supplies.add(b);
    }
    total_supplies.amount += zero_rated + exempt;
    let mut reverse_charge_recovery = reverse_charge_supplies;
    reverse_charge_recovery.add(imports);
    reverse_charge_recovery.add(import_adjustments);
    let mut total_expenses = standard_expenses;
    total_expenses.add(reverse_charge_recovery);
    Vat201 {
        period,
        standard_rated,
        tourist_refunds,
        reverse_charge_supplies,
        zero_rated,
        exempt,
        imports,
        import_adjustments,
        total_supplies,
        standard_expenses,
        reverse_charge_recovery,
        total_expenses,
        total_due: total_supplies.vat,
        total_recoverable: total_expenses.vat,
        net: total_supplies.vat - total_expenses.vat,
        reverse_charge_sales,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const TRN: &str = "100123456700003";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn q2() -> Period {
        Period { from: d(2026, 4, 1), to: d(2026, 6, 30) }
    }

    fn std(value: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, taxable_value: value, vat: value * dec!(0.05), mechanism: Mechanism::Normal }
    }

    fn untaxed(treatment: Treatment, value: Decimal) -> DocLine {
        DocLine { treatment, taxable_value: value, vat: dec!(0), mechanism: Mechanism::Normal }
    }

    fn sale(number: &str, kind: DocKind, emirate: &str, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 10),
            party_name: format!("Party {number}"),
            emirate: emirate.into(),
            cancelled: false,
            lines,
        }
    }

    fn purchase(number: &str, kind: DocKind, trn: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 3),
            party_name: "Supplier".into(),
            party_trn: trn.map(str::to_owned),
            cancelled: false,
            lines,
        }
    }

    fn box1(r: &Vat201) -> Vec<(&str, &str, Decimal, Decimal)> {
        r.standard_rated.iter().map(|e| (e.box_id, e.emirate, e.amounts.amount, e.amounts.vat)).collect()
    }

    #[test]
    fn standard_supplies_split_by_emirate_with_notes_netted() {
        let outward = [
            sale("1", DocKind::Invoice, "DXB", vec![std(dec!(10000)), std(dec!(2000))]),
            sale("2", DocKind::Invoice, "auh", vec![std(dec!(4000))]),
            sale("CN-1", DocKind::CreditNote, "DXB", vec![std(dec!(1000))]),
            sale("DN-1", DocKind::DebitNote, "FUJ", vec![std(dec!(100))]),
        ];
        let r = vat201(&outward, &[], q2());
        assert_eq!(box1(&r), [
            ("1a", "AUH", dec!(4000), dec!(200)),
            ("1b", "DXB", dec!(11000), dec!(550)),
            ("1c", "SHJ", dec!(0), dec!(0)),
            ("1d", "AJM", dec!(0), dec!(0)),
            ("1e", "UAQ", dec!(0), dec!(0)),
            ("1f", "RAK", dec!(0), dec!(0)),
            ("1g", "FUJ", dec!(100), dec!(5)),
        ]);
        assert_eq!(r.total_supplies, BoxAmounts { amount: dec!(15100), vat: dec!(755) });
        assert_eq!((r.total_due, r.total_recoverable, r.net), (dec!(755), dec!(0), dec!(755)));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn zero_rated_and_exempt_supplies_have_their_own_boxes() {
        let outward = [
            sale("1", DocKind::Invoice, "SHJ", vec![
                std(dec!(1000)),
                untaxed(Treatment::ZeroRated, dec!(3000)),
                untaxed(Treatment::Exempt, dec!(500)),
            ]),
            sale("CN", DocKind::CreditNote, "SHJ", vec![untaxed(Treatment::ZeroRated, dec!(300))]),
        ];
        let r = vat201(&outward, &[], q2());
        assert_eq!((r.zero_rated, r.exempt), (dec!(2700), dec!(500)));
        assert_eq!(r.total_supplies, BoxAmounts { amount: dec!(4200), vat: dec!(50) }, "Box 8 adds Boxes 1 to 7");
    }

    #[test]
    fn tourist_refunds_stay_zero() {
        let r = vat201(&[sale("1", DocKind::Invoice, "DXB", vec![std(dec!(100))])], &[], q2());
        for b in [r.tourist_refunds, r.reverse_charge_supplies, r.imports, r.import_adjustments, r.reverse_charge_recovery] {
            assert_eq!(b, BoxAmounts::default());
        }
        assert!(r.warnings.is_empty());
    }

    fn marked(mechanism: Mechanism, value: Decimal) -> DocLine {
        DocLine { mechanism, ..std(value) }
    }

    #[test]
    fn reverse_charge_purchases_go_in_box_3_and_box_10_without_a_trn() {
        let inward = [
            purchase("SaaS-1", DocKind::Invoice, None, vec![marked(Mechanism::ReverseCharge, dec!(2000))]),
            purchase("SaaS-CN", DocKind::CreditNote, None, vec![marked(Mechanism::ReverseCharge, dec!(200))]),
            purchase("P-1", DocKind::Invoice, Some(TRN), vec![std(dec!(1000)), marked(Mechanism::ReverseCharge, dec!(400))]),
            // A zero-rated service from abroad: its value in Box 3, no VAT;
            // an exempt one is not reverse-charged.
            purchase("Z", DocKind::Invoice, None, vec![DocLine { mechanism: Mechanism::ReverseCharge, ..untaxed(Treatment::ZeroRated, dec!(50)) }]),
            purchase("E", DocKind::Invoice, None, vec![DocLine { mechanism: Mechanism::ReverseCharge, ..untaxed(Treatment::Exempt, dec!(70)) }]),
        ];
        let r = vat201(&[], &inward, q2());
        assert_eq!(r.reverse_charge_supplies, BoxAmounts { amount: dec!(2250), vat: dec!(110) }, "2000 − 200 + 400 + 50");
        assert_eq!(r.reverse_charge_recovery, r.reverse_charge_supplies, "Box 10 recovers Box 3");
        assert_eq!(r.standard_expenses, BoxAmounts { amount: dec!(1000), vat: dec!(50) });
        assert_eq!(r.total_supplies, BoxAmounts { amount: dec!(2250), vat: dec!(110) }, "Box 8 includes Box 3");
        assert_eq!(r.total_expenses, BoxAmounts { amount: dec!(3250), vat: dec!(160) });
        assert_eq!((r.total_due, r.total_recoverable, r.net), (dec!(110), dec!(160), dec!(-50)), "the reverse charge nets to zero");
        assert!(!r.warnings.iter().any(|w| w.contains("no supplier TRN")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("Box 10")), "{:?}", r.warnings);
    }

    #[test]
    fn customs_imports_go_in_box_6_their_notes_in_box_7_and_both_in_box_10() {
        let inward = [
            purchase("BOE-1", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(10000))]),
            purchase("BOE-CN", DocKind::CreditNote, None, vec![marked(Mechanism::Import, dec!(1000))]),
        ];
        let r = vat201(&[], &inward, q2());
        assert_eq!(r.imports, BoxAmounts { amount: dec!(10000), vat: dec!(500) });
        assert_eq!(r.import_adjustments, BoxAmounts { amount: dec!(-1000), vat: dec!(-50) });
        assert_eq!(r.reverse_charge_recovery, BoxAmounts { amount: dec!(9000), vat: dec!(450) });
        assert_eq!(r.net, dec!(0));
        assert!(r.warnings.iter().any(|w| w.contains("pre-filled")), "{:?}", r.warnings);
    }

    #[test]
    fn domestic_reverse_charge_sales_show_their_value_without_vat() {
        let outward = [
            sale("G-1", DocKind::Invoice, "DXB", vec![DocLine { vat: dec!(0), ..marked(Mechanism::ReverseCharge, dec!(5000)) }]),
            sale("G-2", DocKind::Invoice, "DXB", vec![marked(Mechanism::ReverseCharge, dec!(100))]),
        ];
        let r = vat201(&outward, &[], q2());
        assert_eq!(r.standard_rated[1].amounts, BoxAmounts { amount: dec!(5100), vat: dec!(0) });
        assert_eq!(r.reverse_charge_sales, dec!(5100));
        assert_eq!(r.total_due, dec!(0));
        assert!(r.warnings.iter().any(|w| w.contains("Invoice G-2") && w.contains("recipient accounts")), "{:?}", r.warnings);
    }

    #[test]
    fn expenses_net_supplier_credit_notes_and_need_a_trn() {
        let inward = [
            purchase("P-1", DocKind::Invoice, Some(TRN), vec![std(dec!(6000)), untaxed(Treatment::Exempt, dec!(900))]),
            purchase("P-CN", DocKind::CreditNote, Some(TRN), vec![std(dec!(1000))]),
            purchase("X", DocKind::Invoice, None, vec![std(dec!(2000))]),
            purchase("Y", DocKind::Invoice, Some(" "), vec![untaxed(Treatment::ZeroRated, dec!(50))]),
        ];
        let outward = [sale("1", DocKind::Invoice, "DXB", vec![std(dec!(4000))])];
        let r = vat201(&outward, &inward, q2());
        assert_eq!(r.standard_expenses, BoxAmounts { amount: dec!(5000), vat: dec!(250) }, "exempt purchases are not Box 9");
        assert_eq!(r.total_expenses, r.standard_expenses);
        assert_eq!((r.total_due, r.total_recoverable, r.net), (dec!(200), dec!(250), dec!(-50)), "a refund position");
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("Purchase X"), "{:?}", r.warnings);
    }

    #[test]
    fn period_cancellation_and_bad_data() {
        let mut outside = sale("0", DocKind::Invoice, "DXB", vec![std(dec!(999))]);
        outside.date = d(2026, 7, 1);
        let mut cancelled = sale("C", DocKind::Invoice, "DXB", vec![std(dec!(999))]);
        cancelled.cancelled = true;
        let mut old_purchase = purchase("P0", DocKind::Invoice, Some(TRN), vec![std(dec!(999))]);
        old_purchase.date = d(2026, 3, 31);
        let outward = [
            outside,
            cancelled,
            sale("Q", DocKind::Invoice, "XYZ", vec![std(dec!(100)), untaxed(Treatment::Exempt, dec!(7))]),
            sale("Z", DocKind::Invoice, "DXB", vec![DocLine { vat: dec!(5), ..untaxed(Treatment::ZeroRated, dec!(100)) }]),
        ];
        let r = vat201(&outward, &[old_purchase], q2());
        assert_eq!(r.total_supplies, BoxAmounts { amount: dec!(107), vat: dec!(0) }, "Z's zero-rated and Q's exempt value");
        assert_eq!(r.standard_expenses, BoxAmounts::default());
        assert!(r.warnings.iter().any(|w| w.contains("Invoice Q") && w.contains("Box 1")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("Invoice Z") && w.contains("zero-rated line carries VAT 5")));
    }

    #[test]
    fn serialises_with_box_ids_and_decimal_strings() {
        let r = vat201(&[sale("1", DocKind::Invoice, "DXB", vec![std(dec!(100))])], &[], q2());
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["standard_rated"][1]["box"], "1b");
        assert_eq!(j["standard_rated"][1]["amounts"]["vat"], "5.00");
        assert_eq!(j["net"], "5.00", "decimals are strings, never floats");
    }
}

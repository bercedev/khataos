//! The GST F5 return worked out from outward and inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its vouchers
//! into documents. Box labels and contents follow IRAS *Completing GST
//! returns* (`docs/research/singapore-compliance.md` §5 [PRIMARY]).
//!
//! What is worked out, and every simplification:
//! - Box 1: standard-rated supplies (value excluding GST). Credit notes
//!   reduce, and debit notes add to, the period they are dated in. Plus the
//!   value of purchases under the reverse charge (lines marked "RC:",
//!   [`Mechanism::ReverseCharge`]): imported services and low-value goods
//!   of an RC business (e-Tax Guide "GST: Reverse Charge", 10th ed., §11.1.1
//!   [PRIMARY]).
//! - Boxes 2 and 3: zero-rated and exempt supplies. Net realised exchange
//!   gains and losses (part of Box 3) are not added.
//! - Box 4: Boxes 1 + 2 + 3. Out-of-scope supplies are in no box.
//! - Box 5: taxable purchases, the standard-rated and zero-rated lines of
//!   purchases from suppliers with a GST registration number. Disallowed
//!   expenses (certain motor car and staff medical costs), private use and
//!   partial-exemption apportionment are NOT applied: the user must adjust.
//!   Plus reverse-charge values, and imports with GST paid to Singapore
//!   Customs (lines marked "IMPORT:", no supplier number needed; the value
//!   should be the one on the import permit).
//! - Box 6: the GST on Box 1, including the reverse-charge GST the business
//!   accounts for (worked out by the app at the rate class). Bad debts
//!   recovered are not modelled.
//! - Box 7: the GST on Box 5 purchases, all taken as claimable. Tourist
//!   refunds, bad debt relief, reverse charge refunds and pre-registration
//!   claims are not modelled. Plus the reverse-charge GST (claimed in full:
//!   an RC business is by definition not fully taxable, so a warning asks
//!   for the normal recovery rules) and import GST paid at customs (lines
//!   marked "IMPORT:" carrying GST; the app takes it from GST ledgers
//!   classed "IMPORT").
//! - Box 8: Box 6 − Box 7; positive is payable, negative is claimable. IRAS
//!   neither collects nor refunds a net amount under S$5 (a warning says so).
//! - Box 14: "Yes" with the value of reverse-charge purchases (also in
//!   Boxes 1 and 5).
//! - Boxes 9 (MES/A3PL imports: value in Boxes 5 and 9, no GST), 10–12
//!   (claims in Box 7), 15–17 (marketplaces and low-value goods) and the
//!   IGDS section (18–21: deferred import GST in Box 19, value in Box 21):
//!   not modelled, always 0 / "No" (pointers in the research doc).
//! - Box 13 (revenue): an estimate, the net value of every outward document
//!   in the period, out-of-scope lines included (reverse-charge values are
//!   not revenue). IRAS accepts a best
//!   estimate; the user should replace it with revenue from the accounts.
//!
//! None of this is tax advice: the figures are a preparation aid and must
//! be checked before filing on myTax Portal.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::gst::{Mechanism, Treatment};

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply (a credit note).
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
    /// Value excluding GST, in SGD.
    pub taxable_value: Decimal,
    /// GST in SGD; zero on zero-rated, exempt and out-of-scope lines. On a
    /// reverse-charge line, the GST the business accounts for itself; on an
    /// import line, the GST paid at customs (the line may then have no value:
    /// see [`Mechanism::Import`]).
    pub gst: Decimal,
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
    /// The supplier's GST registration number; `None` when not registered.
    pub party_gst_no: Option<String>,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The accounting period, both ends inclusive.
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

/// A yes/no question with an amount (Boxes 10–12 and 14–17).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct YesNoBox {
    pub yes: bool,
    pub amount: Decimal,
}

/// GST F5 for a period, in SGD. Field comments give the box numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GstF5 {
    pub period: Period,
    /// 1: total value of standard-rated supplies.
    pub standard_rated: Decimal,
    /// 2: total value of zero-rated supplies.
    pub zero_rated: Decimal,
    /// 3: total value of exempt supplies.
    pub exempt: Decimal,
    /// 4: Boxes 1 + 2 + 3.
    pub total_supplies: Decimal,
    /// 5: total value of taxable purchases (no disallowance applied).
    pub taxable_purchases: Decimal,
    /// 6: output tax due.
    pub output_tax: Decimal,
    /// 7: input tax and refunds claimed (no disallowance applied).
    pub input_tax: Decimal,
    /// 8: Box 6 − Box 7. Positive: payable; negative: claimable.
    pub net_gst: Decimal,
    /// 9: goods imported under MES / A3PL / other approved schemes; not modelled.
    pub scheme_imports: Decimal,
    /// 10: GST refunded to tourists, claimed in Box 7; not modelled.
    pub tourist_refunds: YesNoBox,
    /// 11: bad debt relief and reverse charge refund claims; not modelled.
    pub bad_debt_relief: YesNoBox,
    /// 12: pre-registration claims; not modelled.
    pub pre_registration: YesNoBox,
    /// 13: revenue, estimated from the outward documents.
    pub revenue: Decimal,
    /// 14: imported services and low-value goods under reverse charge (value).
    pub reverse_charge: YesNoBox,
    /// 15: remote services supplied as an electronic marketplace operator; not modelled.
    pub marketplace_remote_services: YesNoBox,
    /// 16: low-value goods supplied as a redeliverer or marketplace operator; not modelled.
    pub marketplace_low_value_goods: YesNoBox,
    /// 17: own supplies of imported low-value goods; not modelled.
    pub own_low_value_goods: YesNoBox,
    /// Out-of-scope supplies: in no box, shown for reference.
    pub out_of_scope: Decimal,
    pub warnings: Vec<String>,
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Invoice",
        DocKind::CreditNote => "Credit note",
        DocKind::DebitNote => "Debit note",
    }
}

fn name(t: Treatment) -> &'static str {
    match t {
        Treatment::Standard => "standard-rated",
        Treatment::ZeroRated => "zero-rated",
        Treatment::Exempt => "exempt",
        Treatment::OutOfScope => "out-of-scope",
    }
}

/// Net GST below this is neither paid nor refunded (IRAS, Box 8).
pub const MIN_NET_GST: Decimal = Decimal::from_parts(5, 0, 0, false, 0);

/// Work out GST F5 for a period from the documents dated in it.
pub fn gst_f5(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> GstF5 {
    let mut warnings = Vec::new();
    let (mut standard_rated, mut output_tax, mut zero_rated, mut exempt, mut out_of_scope) =
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        for l in &d.lines {
            let value = l.taxable_value * sign;
            if l.mechanism != Mechanism::Normal {
                warnings.push(format!(
                    "{} {}: a sale line is marked {}; reverse charge and imports apply to purchases, so it is reported as an ordinary line",
                    label(d.kind),
                    d.number,
                    if l.mechanism == Mechanism::Import { "IMPORT" } else { "RC" }
                ));
            }
            match l.treatment {
                Treatment::Standard => {
                    standard_rated += value;
                    output_tax += l.gst * sign;
                }
                Treatment::ZeroRated => zero_rated += value,
                Treatment::Exempt => exempt += value,
                Treatment::OutOfScope => out_of_scope += value,
            }
            if l.treatment != Treatment::Standard && !l.gst.is_zero() {
                warnings.push(format!(
                    "{} {}: a {} line carries GST {}; that GST is left out of the return",
                    label(d.kind),
                    d.number,
                    name(l.treatment),
                    l.gst.normalize()
                ));
            }
        }
    }

    let (mut taxable_purchases, mut input_tax) = (Decimal::ZERO, Decimal::ZERO);
    let (mut rc_value, mut rc_gst, mut import_value, mut import_gst) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        let claimable = |l: &&DocLine| l.mechanism == Mechanism::Normal && matches!(l.treatment, Treatment::Standard | Treatment::ZeroRated);
        for l in d.lines.iter().filter(|l| l.mechanism != Mechanism::Normal) {
            match (l.mechanism, l.treatment) {
                (Mechanism::ReverseCharge, Treatment::Standard) => {
                    rc_value += l.taxable_value * sign;
                    rc_gst += l.gst * sign;
                }
                (Mechanism::Import, Treatment::Standard | Treatment::ZeroRated) => {
                    import_value += l.taxable_value * sign;
                    import_gst += l.gst * sign;
                }
                _ => {}
            }
        }
        let value: Decimal = d.lines.iter().filter(claimable).map(|l| l.taxable_value).sum();
        let gst: Decimal = d.lines.iter().filter(claimable).map(|l| l.gst).sum();
        if d.party_gst_no.as_deref().is_none_or(|t| t.trim().is_empty()) {
            if !gst.is_zero() {
                warnings.push(format!(
                    "Purchase {} from {} has GST but no supplier GST registration number; it is left out of Boxes 5 and 7",
                    d.number, d.party_name
                ));
            }
            continue;
        }
        taxable_purchases += value * sign;
        input_tax += gst * sign;
    }
    // Reverse charge (e-Tax Guide "GST: Reverse Charge" §11.1.1): the value
    // in Boxes 1, 5 and 14, the GST in Boxes 6 and 7.
    standard_rated += rc_value;
    output_tax += rc_gst;
    taxable_purchases += rc_value + import_value;
    input_tax += rc_gst + import_gst;
    let reverse_charge = YesNoBox { yes: !rc_value.is_zero(), amount: rc_value };
    if !rc_gst.is_zero() {
        warnings.push(
            "Reverse charge GST is claimed in full in Box 7: an RC business is not entitled to full input tax credit, so apply the normal recovery rules (apportionment) before filing"
                .to_owned(),
        );
    }
    if !import_value.is_zero() && import_gst.is_zero() {
        warnings.push("Imports are in Box 5 but no import GST was posted to a GST ledger classed IMPORT: claim the GST on the import permits in Box 7".to_owned());
    }

    let revenue = standard_rated - rc_value + zero_rated + exempt + out_of_scope;
    let net_gst = output_tax - input_tax;
    if !net_gst.is_zero() && net_gst.abs() < MIN_NET_GST {
        warnings.push(format!(
            "Net GST is {}: IRAS neither collects nor refunds a net amount under S$5",
            net_gst.normalize()
        ));
    }
    GstF5 {
        period,
        standard_rated,
        zero_rated,
        exempt,
        total_supplies: standard_rated + zero_rated + exempt,
        taxable_purchases,
        output_tax,
        input_tax,
        net_gst,
        scheme_imports: Decimal::ZERO,
        tourist_refunds: YesNoBox::default(),
        bad_debt_relief: YesNoBox::default(),
        pre_registration: YesNoBox::default(),
        revenue,
        reverse_charge,
        marketplace_remote_services: YesNoBox::default(),
        marketplace_low_value_goods: YesNoBox::default(),
        own_low_value_goods: YesNoBox::default(),
        out_of_scope,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const GST_NO: &str = "M21234567K";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn q2() -> Period {
        Period { from: d(2026, 4, 1), to: d(2026, 6, 30) }
    }

    fn std(value: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, taxable_value: value, gst: value * dec!(0.09), mechanism: Mechanism::Normal }
    }

    fn untaxed(treatment: Treatment, value: Decimal) -> DocLine {
        DocLine { treatment, taxable_value: value, gst: dec!(0), mechanism: Mechanism::Normal }
    }

    fn sale(number: &str, kind: DocKind, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 10),
            party_name: format!("Party {number}"),
            cancelled: false,
            lines,
        }
    }

    fn purchase(number: &str, kind: DocKind, gst_no: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 3),
            party_name: "Supplier".into(),
            party_gst_no: gst_no.map(str::to_owned),
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn supplies_split_by_treatment_with_notes_netted() {
        let outward = [
            sale("1", DocKind::Invoice, vec![
                std(dec!(10000)),
                untaxed(Treatment::ZeroRated, dec!(3000)),
                untaxed(Treatment::Exempt, dec!(500)),
                untaxed(Treatment::OutOfScope, dec!(700)),
            ]),
            sale("CN-1", DocKind::CreditNote, vec![std(dec!(1000)), untaxed(Treatment::ZeroRated, dec!(300))]),
            sale("DN-1", DocKind::DebitNote, vec![std(dec!(100))]),
        ];
        let r = gst_f5(&outward, &[], q2());
        assert_eq!((r.standard_rated, r.zero_rated, r.exempt), (dec!(9100), dec!(2700), dec!(500)));
        assert_eq!(r.total_supplies, dec!(12300), "Box 4 = 1 + 2 + 3, out of scope left out");
        assert_eq!(r.out_of_scope, dec!(700));
        assert_eq!(r.output_tax, dec!(819));
        assert_eq!((r.input_tax, r.net_gst), (dec!(0), dec!(819)));
        assert_eq!(r.revenue, dec!(13000), "every outward value, out of scope included");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn purchases_need_a_supplier_gst_number_and_net_credit_notes() {
        let inward = [
            purchase("P-1", DocKind::Invoice, Some(GST_NO), vec![
                std(dec!(6000)),
                untaxed(Treatment::ZeroRated, dec!(400)),
                untaxed(Treatment::Exempt, dec!(900)),
                untaxed(Treatment::OutOfScope, dec!(50)),
            ]),
            purchase("P-CN", DocKind::CreditNote, Some(GST_NO), vec![std(dec!(1000))]),
            purchase("X", DocKind::Invoice, None, vec![std(dec!(2000))]),
            purchase("Y", DocKind::Invoice, Some(" "), vec![untaxed(Treatment::ZeroRated, dec!(50))]),
        ];
        let outward = [sale("1", DocKind::Invoice, vec![std(dec!(4000))])];
        let r = gst_f5(&outward, &inward, q2());
        assert_eq!(r.taxable_purchases, dec!(5400), "standard and zero-rated purchases; not exempt or out of scope");
        assert_eq!(r.input_tax, dec!(450));
        assert_eq!((r.output_tax, r.net_gst), (dec!(360), dec!(-90)), "a claim position");
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("Purchase X"), "{:?}", r.warnings);
    }

    #[test]
    fn boxes_not_worked_out_stay_no_and_zero() {
        let r = gst_f5(&[sale("1", DocKind::Invoice, vec![std(dec!(100))])], &[], q2());
        assert_eq!(r.scheme_imports, dec!(0));
        for b in [
            r.tourist_refunds,
            r.bad_debt_relief,
            r.pre_registration,
            r.reverse_charge,
            r.marketplace_remote_services,
            r.marketplace_low_value_goods,
            r.own_low_value_goods,
        ] {
            assert_eq!(b, YesNoBox::default());
        }
    }

    fn marked(mechanism: Mechanism, value: Decimal, gst: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, taxable_value: value, gst, mechanism }
    }

    #[test]
    fn reverse_charge_fills_boxes_1_5_6_7_and_14() {
        let inward = [
            purchase("SaaS-1", DocKind::Invoice, None, vec![marked(Mechanism::ReverseCharge, dec!(1000), dec!(90))]),
            purchase("LVG-1", DocKind::Invoice, None, vec![marked(Mechanism::ReverseCharge, dec!(300), dec!(27))]),
            purchase("SaaS-CN", DocKind::CreditNote, None, vec![marked(Mechanism::ReverseCharge, dec!(100), dec!(9))]),
        ];
        let outward = [sale("1", DocKind::Invoice, vec![std(dec!(2000))])];
        let r = gst_f5(&outward, &inward, q2());
        assert_eq!(r.standard_rated, dec!(3200), "Box 1: 2000 + 1200 reverse charge");
        assert_eq!(r.taxable_purchases, dec!(1200), "Box 5");
        assert_eq!((r.output_tax, r.input_tax, r.net_gst), (dec!(288), dec!(108), dec!(180)));
        assert_eq!(r.reverse_charge, YesNoBox { yes: true, amount: dec!(1200) }, "Box 14");
        assert_eq!(r.total_supplies, dec!(3200));
        assert_eq!(r.revenue, dec!(2000), "Box 13 is revenue: the reverse charge is not");
        assert!(r.warnings.iter().any(|w| w.contains("RC business")), "{:?}", r.warnings);
        assert!(!r.warnings.iter().any(|w| w.contains("no supplier GST")), "{:?}", r.warnings);
    }

    #[test]
    fn customs_imports_fill_boxes_5_and_7_without_a_supplier_number() {
        let inward = [
            // The overseas supplier's invoice: the goods' value.
            purchase("PO-9", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(15600), dec!(0))]),
            // The import permit's GST, posted to a GST ledger classed IMPORT.
            purchase("PERMIT", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(0), dec!(1404))]),
        ];
        let r = gst_f5(&[], &inward, q2());
        assert_eq!((r.taxable_purchases, r.input_tax), (dec!(15600), dec!(1404)));
        assert_eq!((r.standard_rated, r.output_tax), (dec!(0), dec!(0)), "import GST is not output tax");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let r = gst_f5(&[], &inward[..1], q2());
        assert!(r.warnings.iter().any(|w| w.contains("classed IMPORT")), "{:?}", r.warnings);
    }

    #[test]
    fn period_cancellation_bad_data_and_small_nets() {
        let mut outside = sale("0", DocKind::Invoice, vec![std(dec!(999))]);
        outside.date = d(2026, 7, 1);
        let mut cancelled = sale("C", DocKind::Invoice, vec![std(dec!(999))]);
        cancelled.cancelled = true;
        let mut old_purchase = purchase("P0", DocKind::Invoice, Some(GST_NO), vec![std(dec!(999))]);
        old_purchase.date = d(2026, 3, 31);
        let outward = [
            outside,
            cancelled,
            sale("Z", DocKind::Invoice, vec![DocLine { gst: dec!(9), ..untaxed(Treatment::ZeroRated, dec!(100)) }]),
            sale("S", DocKind::Invoice, vec![std(dec!(40))]),
        ];
        let r = gst_f5(&outward, &[old_purchase], q2());
        assert_eq!((r.standard_rated, r.zero_rated, r.output_tax), (dec!(40), dec!(100), dec!(3.60)));
        assert_eq!(r.taxable_purchases, dec!(0));
        assert!(r.warnings.iter().any(|w| w.contains("Invoice Z") && w.contains("zero-rated line carries GST 9")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("under S$5")), "{:?}", r.warnings);
    }

    #[test]
    fn serialises_with_decimal_strings() {
        let r = gst_f5(&[sale("1", DocKind::Invoice, vec![std(dec!(100))])], &[], q2());
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["output_tax"], "9.00", "decimals are strings, never floats");
        assert_eq!(j["reverse_charge"]["yes"], false);
        assert_eq!(j["period"]["from"], "2026-04-01");
    }
}

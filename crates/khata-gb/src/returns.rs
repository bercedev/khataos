//! The 9-box VAT return worked out from outward and inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its vouchers
//! into documents. The boxes follow VAT Notice 700/12 §3 and the VAT (MTD)
//! API (`docs/research/uk-compliance.md` §1.5 [PRIMARY]).
//!
//! What is worked out, and every simplification:
//! - Box 1: VAT on sales (standard and reduced-rated lines). Credit notes
//!   reduce, and debit notes add to, the period they are dated in.
//! - Box 2: VAT due on goods acquired into Northern Ireland from the EU.
//!   **Not modelled, always 0**: acquisition VAT is self-accounted (Box 2
//!   and Box 4 together) and the books hold no such posting. A warning says
//!   so whenever Box 9 is not zero.
//! - Box 3: Box 1 + Box 2.
//! - Box 4: VAT on purchases, all taken as deductible. Not applied: blocked
//!   input tax (business entertainment, cars), partial exemption, the
//!   capital goods scheme. Purchases from a supplier without a VAT number
//!   keep their value in Box 7 but their VAT is left out of Box 4 (no VAT
//!   invoice to reclaim from), with a warning.
//! - Box 5: Box 3 − Box 4. Positive: payable; negative: repayable. (The MTD
//!   API takes the absolute value; see [`crate::mtd`].)
//! - Box 6: the value of all sales excluding VAT: standard, reduced,
//!   zero-rated and exempt supplies and exports (§3.7).
//! - Box 7: the value of all purchases excluding VAT (§3.8). Only purchase
//!   documents are counted, so wages, drawings, loans and other items the
//!   notice leaves out never reach it.
//! - Box 8: goods supplied from Northern Ireland to the EU (the documents
//!   the app marks [`OutwardDoc::ni_eu_goods`]); also in Box 6.
//! - Box 9: goods acquired into Northern Ireland from the EU
//!   ([`InwardDoc::ni_eu_goods`]); also in Box 7.
//!
//! Boxes 1 to 5 are in pounds and pence. Boxes 6 to 9 are whole pounds:
//! the MTD API defines them "to 2 zeroed decimal places" [PRIMARY]. The
//! pence are dropped (truncated toward zero), the long-standing paper-form
//! instruction [UNCERTAIN: not on the current Notice 700/12 page].
//!
//! Not modelled: the domestic reverse charge (construction, Notice 735),
//! postponed import VAT accounting, the Flat Rate, Cash and Annual
//! Accounting schemes (the return is on invoice dates), bad-debt relief and
//! error corrections from earlier periods (Notice 700/45).
//!
//! None of this is legal advice: the figures are a preparation aid and must
//! be checked before they are submitted to HMRC.

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::vat::{Mechanism, Treatment};

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
    /// Goods (a stock item) rather than services: only goods go in Boxes 8
    /// and 9.
    pub goods: bool,
    /// Value excluding VAT, in GBP.
    pub taxable_value: Decimal,
    /// VAT in GBP; zero on zero-rated and exempt lines. On a reverse-charge
    /// purchase line, the VAT the business accounts for itself; on an import
    /// or postponed-import line, the import VAT (such a line may have no
    /// value: the app makes one from the VAT ledgers classed "IMPORT" or
    /// "IMPORT:PVA").
    pub vat: Decimal,
    /// Reverse charge, import or postponed import ([`Mechanism`]).
    pub mechanism: Mechanism,
    /// Capital expenditure (a purchase posted to a fixed-asset ledger): only
    /// the Flat Rate Scheme cares ([`crate::schemes`]).
    pub capital: bool,
}

/// A document the business issued: an invoice, credit note or debit note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    /// The app's own reference (a voucher id).
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// Goods supplied from Northern Ireland to a customer in the EU: its
    /// goods lines also go in Box 8.
    pub ni_eu_goods: bool,
    /// Cancelled documents are left out.
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// A document received from a supplier. `kind` is from the supplier's
/// point of view: a purchase return the business records as its own debit
/// note is the counterpart of the supplier's credit note, so the app maps
/// it to [`DocKind::CreditNote`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The supplier's VAT registration number; `None` when unregistered.
    pub party_vrn: Option<String>,
    /// Goods acquired into Northern Ireland from a supplier in the EU: its
    /// goods lines also go in Box 9.
    pub ni_eu_goods: bool,
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// The VAT period, both ends inclusive.
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

/// The 9-box VAT return for a period. Field comments give the box numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatReturn {
    pub period: Period,
    /// 1: VAT due on sales and other outputs.
    pub vat_due_sales: Decimal,
    /// 2: VAT due on acquisitions of goods into Northern Ireland from the
    /// EU. Not modelled, always 0.
    pub vat_due_acquisitions: Decimal,
    /// 3: Boxes 1 + 2.
    pub total_vat_due: Decimal,
    /// 4: VAT reclaimed on purchases and other inputs.
    pub vat_reclaimed: Decimal,
    /// 5: Box 3 − Box 4. Positive: payable to HMRC; negative: repayable.
    pub net_vat_due: Decimal,
    /// 6: total sales excluding VAT, whole pounds.
    pub total_sales_ex_vat: Decimal,
    /// 7: total purchases excluding VAT, whole pounds.
    pub total_purchases_ex_vat: Decimal,
    /// 8: goods supplied from Northern Ireland to the EU, whole pounds.
    pub goods_supplied_to_eu: Decimal,
    /// 9: goods acquired into Northern Ireland from the EU, whole pounds.
    pub goods_acquired_from_eu: Decimal,
    pub warnings: Vec<String>,
}

pub(crate) fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Invoice",
        DocKind::CreditNote => "Credit note",
        DocKind::DebitNote => "Debit note",
    }
}

/// Pounds and pence (Boxes 1 to 5).
pub(crate) fn pence(a: Decimal) -> Decimal {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Whole pounds, pence dropped (Boxes 6 to 9).
pub fn whole_pounds(a: Decimal) -> Decimal {
    a.trunc()
}

/// The documents of a period, sorted into what the boxes are made of. The
/// normal return and the Flat Rate Scheme ([`crate::schemes`]) put them
/// together differently.
#[derive(Debug, Clone, Default)]
pub(crate) struct Parts {
    /// Normal sales: VAT (Box 1), values (Box 6) and VAT-inclusive values by
    /// date (the flat rate turnover).
    pub sales_vat: Decimal,
    pub sales_value: Decimal,
    pub sales_gross: Vec<(NaiveDate, Decimal)>,
    /// Reverse-charge sales: value only (Box 6), outside the flat rate.
    pub outside_sales_value: Decimal,
    pub box8: Decimal,
    /// Normal purchases: value (Box 7) and VAT reclaimed (Box 4, only from
    /// a supplier with a VAT number).
    pub purchase_value: Decimal,
    pub purchase_vat: Decimal,
    /// Capital purchases: (value, VAT reclaimable) of each normal purchase
    /// line posted to a fixed-asset ledger.
    pub capital: Vec<(Decimal, Decimal)>,
    /// Goods (stock items) bought, VAT included, capital excluded: the limited
    /// cost business test.
    pub goods_gross: Decimal,
    pub rc_value: Decimal,
    pub rc_vat: Decimal,
    pub drc_value: Decimal,
    pub drc_vat: Decimal,
    pub import_value: Decimal,
    pub import_vat: Decimal,
    pub pva_value: Decimal,
    pub pva_vat: Decimal,
    pub box9: Decimal,
    pub warnings: Vec<String>,
}

pub(crate) fn parts(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> Parts {
    let mut p = Parts::default();
    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let sign = d.kind.sign();
        for l in &d.lines {
            if d.ni_eu_goods && l.goods {
                p.box8 += l.taxable_value * sign;
            }
            match l.mechanism {
                Mechanism::ReverseCharge | Mechanism::DomesticReverseCharge => {
                    // The customer accounts for the VAT (Notice 735 §7.7).
                    p.outside_sales_value += l.taxable_value * sign;
                    if !l.vat.is_zero() {
                        p.warnings.push(format!(
                            "{name}: a reverse-charge sale line carries VAT {}; the customer accounts for it, so it is left out",
                            l.vat.normalize()
                        ));
                    }
                    continue;
                }
                Mechanism::Import | Mechanism::PostponedImport => p.warnings.push(format!(
                    "{name}: a sale line is marked as an import; it is reported as an ordinary sale"
                )),
                Mechanism::Normal => {}
            }
            p.sales_value += l.taxable_value * sign;
            p.sales_gross.push((d.date, (l.taxable_value + if l.treatment.taxed() { l.vat } else { Decimal::ZERO }) * sign));
            if l.treatment.taxed() {
                p.sales_vat += l.vat * sign;
            } else if !l.vat.is_zero() {
                p.warnings.push(format!(
                    "{name}: a {} line carries VAT {}; that VAT is left out of the return",
                    if l.treatment == Treatment::Exempt { "exempt" } else { "zero-rated" },
                    l.vat.normalize()
                ));
            }
        }
    }

    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        let registered = d.party_vrn.as_deref().is_some_and(|t| !t.trim().is_empty());
        let mut vat = Decimal::ZERO;
        for l in &d.lines {
            let (value, line_vat) = (l.taxable_value * sign, if l.treatment.taxed() { l.vat * sign } else { Decimal::ZERO });
            if d.ni_eu_goods && l.goods {
                p.box9 += value;
            }
            match l.mechanism {
                Mechanism::Normal => {
                    p.purchase_value += value;
                    vat += line_vat;
                    if l.capital {
                        p.capital.push((value, if registered { line_vat } else { Decimal::ZERO }));
                    } else if l.goods {
                        p.goods_gross += value + line_vat;
                    }
                }
                Mechanism::ReverseCharge => {
                    p.rc_value += value;
                    p.rc_vat += line_vat;
                }
                Mechanism::DomesticReverseCharge => {
                    p.drc_value += value;
                    p.drc_vat += line_vat;
                }
                // Import VAT needs the C79 or the statement, not a supplier VRN.
                Mechanism::Import => {
                    p.import_value += value;
                    p.import_vat += l.vat * sign;
                }
                Mechanism::PostponedImport => {
                    p.pva_value += value;
                    p.pva_vat += l.vat * sign;
                }
            }
        }
        if registered {
            p.purchase_vat += vat;
        } else if !vat.is_zero() {
            p.warnings.push(format!(
                "Purchase {} from {} has VAT but no supplier VAT number; its VAT is left out of Box 4",
                d.number, d.party_name
            ));
        }
    }
    p
}

/// Boxes 1 to 9 from their parts, with the rounding and the Box 2 warning.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble(
    period: Period,
    box1: Decimal,
    box4: Decimal,
    box6: Decimal,
    box7: Decimal,
    box8: Decimal,
    box9: Decimal,
    mut warnings: Vec<String>,
) -> VatReturn {
    let box9 = whole_pounds(box9);
    let box2 = Decimal::ZERO;
    if !box9.is_zero() {
        warnings.push(
            "Box 9 has acquisitions of goods from the EU into Northern Ireland: the acquisition VAT (Box 2, and its reclaim in Box 4) is not worked out; add it before submitting"
                .to_owned(),
        );
    }
    let box1: Decimal = pence(box1);
    let box3 = box1 + box2;
    let box4: Decimal = pence(box4);
    VatReturn {
        period,
        vat_due_sales: box1,
        vat_due_acquisitions: box2,
        total_vat_due: box3,
        vat_reclaimed: box4,
        net_vat_due: box3 - box4,
        total_sales_ex_vat: whole_pounds(box6),
        total_purchases_ex_vat: whole_pounds(box7),
        goods_supplied_to_eu: whole_pounds(box8),
        goods_acquired_from_eu: box9,
        warnings,
    }
}

/// Work out the 9-box return for a period from the documents dated in it.
///
/// Reverse charges and imports (Notice 700/12 §4.6; Notice 741A §5.2;
/// Notice 735 §§7.7, 8.5; "Complete your VAT Return to account for import
/// VAT" [PRIMARY]):
/// - imported services ("RC:"): VAT in Boxes 1 and 4, value in Boxes 6
///   and 7;
/// - domestic reverse charge ("DRC:"), as the customer: VAT in Boxes 1
///   and 4, value in Box 7; as the supplier: value in Box 6, no VAT;
/// - imports with VAT paid at the border ("IMPORT:"): value in Box 7, the
///   import VAT in Box 4 (no supplier VRN needed);
/// - postponed import VAT ("IMPORT:PVA"): VAT in Boxes 1 and 4.
///
/// All reclaims are in full: the normal input-tax rules (blocking, partial
/// exemption) are the user's to apply.
pub fn vat_return(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> VatReturn {
    let p = parts(outward, inward, period);
    let box1 = p.sales_vat + p.rc_vat + p.drc_vat + p.pva_vat;
    let box4 = p.purchase_vat + p.rc_vat + p.drc_vat + p.import_vat + p.pva_vat;
    let box6 = p.sales_value + p.outside_sales_value + p.rc_value;
    let box7 = p.purchase_value + p.rc_value + p.drc_value + p.import_value + p.pva_value;
    assemble(period, box1, box4, box6, box7, p.box8, p.box9, p.warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const VRN: &str = "GB562235945";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn q2() -> Period {
        Period { from: d(2026, 4, 1), to: d(2026, 6, 30) }
    }

    fn std(value: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, goods: true, taxable_value: value, vat: value * dec!(0.20), mechanism: Mechanism::Normal, capital: false }
    }

    fn reduced(value: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Reduced, goods: true, taxable_value: value, vat: value * dec!(0.05), mechanism: Mechanism::Normal, capital: false }
    }

    fn untaxed(treatment: Treatment, value: Decimal) -> DocLine {
        DocLine { treatment, goods: false, taxable_value: value, vat: dec!(0), mechanism: Mechanism::Normal, capital: false }
    }

    fn sale(number: &str, kind: DocKind, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 10),
            party_name: format!("Party {number}"),
            ni_eu_goods: false,
            cancelled: false,
            lines,
        }
    }

    fn purchase(number: &str, kind: DocKind, vrn: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 3),
            party_name: "Supplier".into(),
            party_vrn: vrn.map(str::to_owned),
            ni_eu_goods: false,
            cancelled: false,
            lines,
        }
    }

    fn boxes(r: &VatReturn) -> [Decimal; 9] {
        [
            r.vat_due_sales,
            r.vat_due_acquisitions,
            r.total_vat_due,
            r.vat_reclaimed,
            r.net_vat_due,
            r.total_sales_ex_vat,
            r.total_purchases_ex_vat,
            r.goods_supplied_to_eu,
            r.goods_acquired_from_eu,
        ]
    }

    #[test]
    fn nine_boxes_from_sales_and_purchases() {
        let outward = [
            sale("1", DocKind::Invoice, vec![
                std(dec!(10000)),
                reduced(dec!(1000)),
                untaxed(Treatment::ZeroRated, dec!(2000)),
                untaxed(Treatment::Exempt, dec!(500.75)),
            ]),
            sale("CN-1", DocKind::CreditNote, vec![std(dec!(1000))]),
            sale("DN-1", DocKind::DebitNote, vec![std(dec!(100))]),
        ];
        let inward = [
            purchase("P-1", DocKind::Invoice, Some(VRN), vec![std(dec!(6000)), untaxed(Treatment::Exempt, dec!(900))]),
            purchase("P-CN", DocKind::CreditNote, Some(VRN), vec![std(dec!(1000))]),
        ];
        let r = vat_return(&outward, &inward, q2());
        // Box 1: 2000 + 50 − 200 + 20; Box 6: 13500.75 − 1000 + 100 = 12600.75 → 12600.
        assert_eq!(boxes(&r), [
            dec!(1870),
            dec!(0),
            dec!(1870),
            dec!(1000),
            dec!(870),
            dec!(12600),
            dec!(5900),
            dec!(0),
            dec!(0)
        ]);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn a_repayment_return_has_a_negative_box_5() {
        let r = vat_return(&[sale("1", DocKind::Invoice, vec![std(dec!(100))])], &[purchase(
            "P",
            DocKind::Invoice,
            Some(VRN),
            vec![std(dec!(1000))],
        )], q2());
        assert_eq!((r.total_vat_due, r.vat_reclaimed, r.net_vat_due), (dec!(20), dec!(200), dec!(-180)));
    }

    #[test]
    fn whole_pounds_drop_the_pence() {
        assert_eq!(whole_pounds(dec!(1234.99)), dec!(1234));
        assert_eq!(whole_pounds(dec!(-1234.99)), dec!(-1234), "toward zero");
        let r = vat_return(&[sale("1", DocKind::Invoice, vec![std(dec!(99.99))])], &[], q2());
        assert_eq!((r.vat_due_sales, r.total_sales_ex_vat), (dec!(20.00), dec!(99)), "Box 1 keeps pence (19.998 → 20.00)");
    }

    #[test]
    fn northern_ireland_goods_trade_with_the_eu_goes_in_boxes_8_and_9() {
        let mut dispatch = sale("NI-1", DocKind::Invoice, vec![untaxed(Treatment::ZeroRated, dec!(3000.50)), DocLine {
            goods: true,
            ..untaxed(Treatment::ZeroRated, dec!(700))
        }]);
        dispatch.ni_eu_goods = true;
        let mut acquisition = purchase("EU-1", DocKind::Invoice, Some("IE6388047V"), vec![DocLine {
            goods: true,
            ..untaxed(Treatment::ZeroRated, dec!(1200))
        }]);
        acquisition.ni_eu_goods = true;
        let r = vat_return(&[dispatch], &[acquisition], q2());
        assert_eq!((r.goods_supplied_to_eu, r.total_sales_ex_vat), (dec!(700), dec!(3700)), "services stay out of Box 8");
        assert_eq!((r.goods_acquired_from_eu, r.total_purchases_ex_vat), (dec!(1200), dec!(1200)));
        assert_eq!(r.vat_due_acquisitions, dec!(0));
        assert!(r.warnings.iter().any(|w| w.contains("Box 2")), "{:?}", r.warnings);
    }

    #[test]
    fn unregistered_suppliers_count_in_box_7_but_not_box_4() {
        let inward = [
            purchase("X", DocKind::Invoice, None, vec![std(dec!(2000))]),
            purchase("Y", DocKind::Invoice, Some(" "), vec![untaxed(Treatment::ZeroRated, dec!(50))]),
        ];
        let r = vat_return(&[], &inward, q2());
        assert_eq!((r.total_purchases_ex_vat, r.vat_reclaimed), (dec!(2050), dec!(0)));
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("Purchase X"));
    }

    #[test]
    fn period_cancellation_and_vat_on_untaxed_lines() {
        let mut outside = sale("0", DocKind::Invoice, vec![std(dec!(999))]);
        outside.date = d(2026, 7, 1);
        let mut cancelled = sale("C", DocKind::Invoice, vec![std(dec!(999))]);
        cancelled.cancelled = true;
        let wrong = sale("Z", DocKind::Invoice, vec![DocLine { vat: dec!(5), ..untaxed(Treatment::ZeroRated, dec!(100)) }]);
        let r = vat_return(&[outside, cancelled, wrong], &[], q2());
        assert_eq!((r.vat_due_sales, r.total_sales_ex_vat), (dec!(0), dec!(100)));
        assert!(r.warnings.iter().any(|w| w.contains("Invoice Z") && w.contains("zero-rated line carries VAT 5")));
    }

    fn marked(mechanism: Mechanism, value: Decimal, vat: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, goods: false, taxable_value: value, vat, mechanism, capital: false }
    }

    #[test]
    fn imported_services_fill_boxes_1_4_6_and_7() {
        let inward = [purchase("SaaS", DocKind::Invoice, None, vec![marked(Mechanism::ReverseCharge, dec!(1000), dec!(200))])];
        let r = vat_return(&[], &inward, q2());
        assert_eq!(boxes(&r), [dec!(200), dec!(0), dec!(200), dec!(200), dec!(0), dec!(1000), dec!(1000), dec!(0), dec!(0)]);
        assert!(r.warnings.is_empty(), "no supplier VRN needed: {:?}", r.warnings);
    }

    #[test]
    fn domestic_reverse_charge_customer_and_supplier() {
        // Customer (Notice 735 §8.5): Boxes 1, 4 and 7, never Box 6.
        let inward = [purchase("CIS-1", DocKind::Invoice, Some(VRN), vec![marked(Mechanism::DomesticReverseCharge, dec!(5000), dec!(1000))])];
        // Supplier (§7.7): Box 6 only.
        let outward = [
            sale("PH-1", DocKind::Invoice, vec![marked(Mechanism::DomesticReverseCharge, dec!(6000), dec!(0))]),
            sale("PH-2", DocKind::Invoice, vec![marked(Mechanism::DomesticReverseCharge, dec!(100), dec!(20))]),
        ];
        let r = vat_return(&outward, &inward, q2());
        assert_eq!(boxes(&r), [dec!(1000), dec!(0), dec!(1000), dec!(1000), dec!(0), dec!(6100), dec!(5000), dec!(0), dec!(0)]);
        assert!(r.warnings.iter().any(|w| w.contains("Invoice PH-2") && w.contains("customer accounts")), "{:?}", r.warnings);
    }

    #[test]
    fn import_vat_paid_or_postponed() {
        let inward = [
            // The goods from an overseas supplier: Box 7.
            purchase("PO-1", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(8000), dec!(0))]),
            // C79 import VAT from a VAT ledger classed IMPORT: Box 4 only.
            purchase("C79", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(0), dec!(1600))]),
            // Postponed import VAT from the monthly statement: Boxes 1 and 4.
            purchase("MPIVS", DocKind::Invoice, None, vec![marked(Mechanism::PostponedImport, dec!(0), dec!(400))]),
        ];
        let r = vat_return(&[], &inward, q2());
        assert_eq!(boxes(&r), [dec!(400), dec!(0), dec!(400), dec!(2000), dec!(-1600), dec!(0), dec!(8000), dec!(0), dec!(0)]);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn serialises_decimals_as_strings() {
        let r = vat_return(&[sale("1", DocKind::Invoice, vec![std(dec!(100))])], &[], q2());
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["vat_due_sales"], "20.00");
        assert_eq!(j["total_sales_ex_vat"], "100");
        assert_eq!(j["period"]["from"], "2026-04-01");
    }
}

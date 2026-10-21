//! The KSA VAT return worked out from outward and inward documents.
//!
//! The input is a neutral document model ([`OutwardDoc`], [`InwardDoc`]),
//! so this module knows nothing about the books; the app turns its vouchers
//! into documents. The lines follow `docs/research/mena-compliance.md` §5
//! (IR Art 59 and the ZATCA return form [SECONDARY: Avalara 2017, Tally,
//! Wafeq]). The portal's line numbering was not checked [UNCERTAIN, research
//! §9 item 10], so the fields carry names, not box numbers.
//!
//! What is worked out, and every simplification:
//! - Standard-rated sales (amount and VAT). Credit notes reduce, and debit
//!   notes add to, the period they are dated in. The "adjustment" column is
//!   not modelled.
//! - Private healthcare and private education to citizens (VAT borne by the
//!   state): not modelled, always 0. A sale the user zero-rates with
//!   VATEX-SA-HEA or -EDU lands in zero-rated domestic sales instead.
//! - Sales to VAT-registered customers in other GCC states: the line of the
//!   original 2017 form [SECONDARY: Avalara 2017]. The GCC intra-state
//!   mechanism is not modelled (and is not described in the research as in
//!   force), so it is always 0 [UNCERTAIN whether the live form still has
//!   it].
//! - Zero-rated domestic sales, exports (zero-rated lines of documents
//!   marked as exports), exempt sales (amount only).
//! - Standard-rated domestic purchases and their VAT, all taken as
//!   recoverable: restricted input tax (the April 2025 IR amendments) and
//!   apportionment are NOT applied. Purchases from a supplier without a VAT
//!   number are left out (no valid tax invoice), with a warning when they
//!   carry VAT.
//! - Imports paid at customs (field 8): lines marked "IMPORT:" (value) and
//!   the VAT posted to VAT ledgers classed "IMPORT" (the customs
//!   declaration's VAT). Imports under the reverse charge (field 9): lines
//!   marked "RC:", services bought from abroad, with the VAT the app works
//!   out at the rate class. Field 9 is entered once and the portal treats
//!   its VAT as due and deductible (ZATCA Imports and Exports Guideline,
//!   2nd ed. May 2026, §§5.3, 6.3, 10.4 [PRIMARY]): so it is in total
//!   purchases and added back to the VAT due. Neither needs a supplier VAT
//!   number. Field numbers 1 and 7 to 9 are confirmed by ZATCA guidelines;
//!   the rest are [UNCERTAIN] and so not shown.
//! - Zero-rated and exempt purchases (amount only).
//! - Total VAT due (sales VAT − purchase VAT), corrections from previous
//!   periods (0, not modelled), VAT credit carried forward (0, not tracked)
//!   and the net: positive is payable, negative is a credit.
//! - Out-of-scope lines are on neither side.
//!
//! None of this is legal advice: the figures are a preparation aid and must
//! be checked before filing on the ZATCA portal.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::vat::{Mechanism, Treatment};

/// What a document is, from the supplier's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum DocKind {
    Invoice,
    /// Reduces the supply.
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

/// One line of a document. Amounts are positive, in SAR; the document kind
/// says whether they add or reduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLine {
    pub treatment: Treatment,
    pub taxable_value: Decimal,
    /// Zero on zero-rated, exempt and out-of-scope lines. On a reverse-charge
    /// line, the VAT the business accounts for itself; on an import line,
    /// the VAT paid at customs (such a line may have no value: the app makes
    /// one from the VAT ledgers classed "IMPORT").
    pub vat: Decimal,
    pub mechanism: Mechanism,
}

/// A document the business issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutwardDoc {
    /// The app's own reference (a voucher id).
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// A supply to a customer abroad: its zero-rated lines are exports.
    pub export: bool,
    /// Cancelled documents are left out.
    pub cancelled: bool,
    pub lines: Vec<DocLine>,
}

/// A document received from a supplier; `kind` is from the supplier's point
/// of view (a purchase return is the supplier's credit note).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InwardDoc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party_name: String,
    /// The supplier's VAT number; `None` for an unregistered supplier.
    pub party_vat: Option<String>,
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

/// An amount and its VAT, in SAR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Amounts {
    pub amount: Decimal,
    pub vat: Decimal,
}

impl Amounts {
    fn add(&mut self, o: Amounts) {
        self.amount += o.amount;
        self.vat += o.vat;
    }
}

/// The VAT return for a period, in the form's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatReturn {
    pub period: Period,
    pub standard_rated_sales: Amounts,
    /// Not modelled, always 0.
    pub citizen_healthcare_education_sales: Amounts,
    /// Not modelled, always 0.
    pub gcc_registered_customer_sales: Amounts,
    pub zero_rated_domestic_sales: Decimal,
    pub exports: Decimal,
    pub exempt_sales: Decimal,
    pub total_sales: Amounts,
    pub standard_rated_purchases: Amounts,
    /// Field 8: imports with VAT paid at customs (lines marked "IMPORT:";
    /// the VAT from VAT ledgers classed "IMPORT").
    pub imports_paid_at_customs: Amounts,
    /// Field 9: imports (services, mostly) under the reverse charge (lines
    /// marked "RC:"), with the self-accounted VAT.
    pub imports_reverse_charge: Amounts,
    pub zero_rated_purchases: Decimal,
    pub exempt_purchases: Decimal,
    pub total_purchases: Amounts,
    /// Sales VAT + reverse-charge VAT − purchase VAT (reverse-charge VAT
    /// included): the reverse charge nets to zero.
    pub total_vat_due: Decimal,
    /// Not modelled, always 0.
    pub corrections: Decimal,
    /// Not tracked, always 0.
    pub credit_carried_forward: Decimal,
    /// Positive: payable; negative: a credit.
    pub net: Decimal,
    pub warnings: Vec<String>,
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "Invoice",
        DocKind::CreditNote => "Credit note",
        DocKind::DebitNote => "Debit note",
    }
}

/// Work out the VAT return for a period from the documents dated in it.
pub fn vat_return(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period) -> VatReturn {
    let mut warnings = Vec::new();
    let mut standard_rated_sales = Amounts::default();
    let (mut zero_rated_domestic_sales, mut exports, mut exempt_sales) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);

    for d in outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        for l in &d.lines {
            let amount = l.taxable_value * sign;
            if l.mechanism != Mechanism::Normal {
                warnings.push(format!(
                    "{} {}: a sale line is marked as a reverse charge or import; it is reported as an ordinary sale",
                    label(d.kind),
                    d.number
                ));
            }
            match l.treatment {
                Treatment::Standard => standard_rated_sales.add(Amounts { amount, vat: l.vat * sign }),
                Treatment::ZeroRated if d.export => exports += amount,
                Treatment::ZeroRated => zero_rated_domestic_sales += amount,
                Treatment::Exempt => exempt_sales += amount,
                Treatment::OutOfScope => {}
            }
            if l.treatment != Treatment::Standard && !l.vat.is_zero() {
                warnings.push(format!(
                    "{} {}: a line that is not standard-rated carries VAT {}; that VAT is left out of the return",
                    label(d.kind),
                    d.number,
                    l.vat.normalize()
                ));
            }
        }
    }

    let mut standard_rated_purchases = Amounts::default();
    let (mut imports_paid_at_customs, mut imports_reverse_charge) = (Amounts::default(), Amounts::default());
    let (mut zero_rated_purchases, mut exempt_purchases) = (Decimal::ZERO, Decimal::ZERO);
    for d in inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let sign = d.kind.sign();
        let registered = d.party_vat.as_deref().is_some_and(|v| !v.trim().is_empty());
        let mut standard = Amounts::default();
        for l in &d.lines {
            // Self-accounted or paid at customs: no supplier VAT number needed.
            match (l.mechanism, l.treatment) {
                (Mechanism::ReverseCharge, Treatment::Standard) => {
                    imports_reverse_charge.add(Amounts { amount: l.taxable_value * sign, vat: l.vat * sign });
                    continue;
                }
                (Mechanism::Import, Treatment::Standard | Treatment::ZeroRated) => {
                    imports_paid_at_customs.add(Amounts { amount: l.taxable_value * sign, vat: l.vat * sign });
                    continue;
                }
                _ => {}
            }
            match l.treatment {
                Treatment::Standard => standard.add(Amounts { amount: l.taxable_value, vat: l.vat }),
                Treatment::ZeroRated => zero_rated_purchases += l.taxable_value * sign,
                Treatment::Exempt => exempt_purchases += l.taxable_value * sign,
                Treatment::OutOfScope => {}
            }
        }
        if !registered {
            if !standard.vat.is_zero() {
                warnings.push(format!(
                    "Purchase {} from {} has VAT but no supplier VAT number; it is left out of standard-rated purchases",
                    d.number, d.party_name
                ));
            }
            continue;
        }
        standard_rated_purchases.add(Amounts { amount: standard.amount * sign, vat: standard.vat * sign });
    }

    let (citizen_healthcare_education_sales, gcc_registered_customer_sales) = Default::default();
    if !imports_reverse_charge.vat.is_zero() {
        warnings.push(
            "Reverse charge VAT is in field 9 once: the return treats it as both due and deductible; if it is not fully deductible, put the difference in field 9's adjustment column"
                .to_owned(),
        );
    }
    let mut total_sales = standard_rated_sales;
    total_sales.add(citizen_healthcare_education_sales);
    total_sales.add(gcc_registered_customer_sales);
    total_sales.amount += zero_rated_domestic_sales + exports + exempt_sales;
    let mut total_purchases = standard_rated_purchases;
    total_purchases.add(imports_paid_at_customs);
    total_purchases.add(imports_reverse_charge);
    total_purchases.amount += zero_rated_purchases + exempt_purchases;
    // Field 9's VAT is due and deductible at once (Imports and Exports
    // Guideline §§6.3, 10.4): it adds to the VAT due as much as it deducts.
    let total_vat_due = total_sales.vat + imports_reverse_charge.vat - total_purchases.vat;
    let (corrections, credit_carried_forward) = (Decimal::ZERO, Decimal::ZERO);
    VatReturn {
        period,
        standard_rated_sales,
        citizen_healthcare_education_sales,
        gcc_registered_customer_sales,
        zero_rated_domestic_sales,
        exports,
        exempt_sales,
        total_sales,
        standard_rated_purchases,
        imports_paid_at_customs,
        imports_reverse_charge,
        zero_rated_purchases,
        exempt_purchases,
        total_purchases,
        total_vat_due,
        corrections,
        credit_carried_forward,
        net: total_vat_due + corrections - credit_carried_forward,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const VAT_NO: &str = "300075588700003";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn q2() -> Period {
        Period { from: d(2026, 4, 1), to: d(2026, 6, 30) }
    }

    fn std(value: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, taxable_value: value, vat: value * dec!(0.15), mechanism: Mechanism::Normal }
    }

    fn untaxed(treatment: Treatment, value: Decimal) -> DocLine {
        DocLine { treatment, taxable_value: value, vat: dec!(0), mechanism: Mechanism::Normal }
    }

    fn sale(number: &str, kind: DocKind, export: bool, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc { id: number.into(), kind, number: number.into(), date: d(2026, 5, 10), party_name: "Buyer".into(), export, cancelled: false, lines }
    }

    fn purchase(number: &str, kind: DocKind, vat: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: number.into(),
            kind,
            number: number.into(),
            date: d(2026, 5, 3),
            party_name: "Supplier".into(),
            party_vat: vat.map(str::to_owned),
            cancelled: false,
            lines,
        }
    }

    #[test]
    fn sales_split_by_treatment_with_notes_netted() {
        let outward = [
            sale("1", DocKind::Invoice, false, vec![std(dec!(10000)), untaxed(Treatment::ZeroRated, dec!(500)), untaxed(Treatment::Exempt, dec!(300))]),
            sale("2", DocKind::Invoice, true, vec![untaxed(Treatment::ZeroRated, dec!(4000))]),
            sale("CN-1", DocKind::CreditNote, false, vec![std(dec!(1000))]),
            sale("DN-1", DocKind::DebitNote, false, vec![std(dec!(100))]),
            sale("3", DocKind::Invoice, false, vec![untaxed(Treatment::OutOfScope, dec!(999))]),
        ];
        let r = vat_return(&outward, &[], q2());
        assert_eq!(r.standard_rated_sales, Amounts { amount: dec!(9100), vat: dec!(1365) });
        assert_eq!((r.zero_rated_domestic_sales, r.exports, r.exempt_sales), (dec!(500), dec!(4000), dec!(300)));
        assert_eq!(r.total_sales, Amounts { amount: dec!(13900), vat: dec!(1365) }, "out of scope is left out");
        assert_eq!((r.total_vat_due, r.net), (dec!(1365), dec!(1365)));
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn purchases_need_a_vat_number_for_recovery() {
        let inward = [
            purchase("P-1", DocKind::Invoice, Some(VAT_NO), vec![std(dec!(6000)), untaxed(Treatment::Exempt, dec!(900)), untaxed(Treatment::ZeroRated, dec!(50))]),
            purchase("P-CN", DocKind::CreditNote, Some(VAT_NO), vec![std(dec!(1000))]),
            purchase("X", DocKind::Invoice, None, vec![std(dec!(2000))]),
        ];
        let outward = [sale("1", DocKind::Invoice, false, vec![std(dec!(4000))])];
        let r = vat_return(&outward, &inward, q2());
        assert_eq!(r.standard_rated_purchases, Amounts { amount: dec!(5000), vat: dec!(750) });
        assert_eq!((r.zero_rated_purchases, r.exempt_purchases), (dec!(50), dec!(900)));
        assert_eq!(r.total_purchases, Amounts { amount: dec!(5950), vat: dec!(750) });
        assert_eq!((r.total_vat_due, r.net), (dec!(-150), dec!(-150)), "a credit position");
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("Purchase X"));
    }

    #[test]
    fn unsupported_lines_stay_zero_and_bad_data_is_flagged() {
        let mut outside = sale("0", DocKind::Invoice, false, vec![std(dec!(999))]);
        outside.date = d(2026, 7, 1);
        let mut cancelled = sale("C", DocKind::Invoice, false, vec![std(dec!(999))]);
        cancelled.cancelled = true;
        let wrong = sale("Z", DocKind::Invoice, false, vec![DocLine { vat: dec!(15), ..untaxed(Treatment::ZeroRated, dec!(100)) }]);
        let r = vat_return(&[outside, cancelled, wrong], &[], q2());
        assert_eq!(r.total_sales, Amounts { amount: dec!(100), vat: dec!(0) });
        for a in [r.citizen_healthcare_education_sales, r.gcc_registered_customer_sales] {
            assert_eq!(a, Amounts::default());
        }
        assert_eq!((r.corrections, r.credit_carried_forward), (dec!(0), dec!(0)));
        assert!(r.warnings[0].contains("Invoice Z") && r.warnings[0].contains("VAT 15"), "{:?}", r.warnings);
    }

    fn marked(mechanism: Mechanism, value: Decimal, vat: Decimal) -> DocLine {
        DocLine { treatment: Treatment::Standard, taxable_value: value, vat, mechanism }
    }

    #[test]
    fn reverse_charge_is_field_9_and_nets_to_zero() {
        let inward = [
            purchase("SaaS", DocKind::Invoice, None, vec![marked(Mechanism::ReverseCharge, dec!(100000), dec!(15000))]),
            purchase("SaaS-CN", DocKind::CreditNote, None, vec![marked(Mechanism::ReverseCharge, dec!(1000), dec!(150))]),
        ];
        let outward = [sale("1", DocKind::Invoice, false, vec![std(dec!(4000))])];
        let r = vat_return(&outward, &inward, q2());
        assert_eq!(r.imports_reverse_charge, Amounts { amount: dec!(99000), vat: dec!(14850) });
        assert_eq!(r.total_purchases, Amounts { amount: dec!(99000), vat: dec!(14850) });
        assert_eq!((r.total_vat_due, r.net), (dec!(600), dec!(600)), "only the sales VAT is payable");
        assert!(!r.warnings.iter().any(|w| w.contains("no supplier VAT number")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("field 9")), "{:?}", r.warnings);
    }

    #[test]
    fn customs_imports_are_field_8_and_recoverable() {
        let inward = [
            purchase("PO", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(20000), dec!(0))]),
            purchase("CUSTOMS", DocKind::Invoice, None, vec![marked(Mechanism::Import, dec!(0), dec!(3000))]),
        ];
        let r = vat_return(&[], &inward, q2());
        assert_eq!(r.imports_paid_at_customs, Amounts { amount: dec!(20000), vat: dec!(3000) });
        assert_eq!((r.total_vat_due, r.net), (dec!(-3000), dec!(-3000)));
    }

    #[test]
    fn serialises_decimals_as_strings() {
        let r = vat_return(&[sale("1", DocKind::Invoice, false, vec![std(dec!(100))])], &[], q2());
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["standard_rated_sales"]["vat"], "15.00");
        assert_eq!(j["net"], "15.00");
    }
}

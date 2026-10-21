//! TCS (tax collected at source) on sales and receipts: natures of
//! collection, rates, thresholds and the amount to collect.
//!
//! Source: `docs/research/india-tds-tcs.md` §1. Every rate and threshold
//! here is the Table in s.394(1) of the Income-tax Act 2025 (as gazetted,
//! Act No. 30 of 2025) as amended by s.85 of the Finance Act 2026 (Act
//! No. 4 of 2026, in force 1 Apr 2026) [PRIMARY]. The 4-digit collection
//! codes are Annexure 2 of Protean's file format for Form 143 v1.2
//! [PRIMARY].
//!
//! - **From 1 Apr 2026 only.** Collections before then fell under s.206C of
//!   the 1961 Act, whose rates were not re-verified, so [`compute`] refuses
//!   them.
//! - **No PAN** (s.397(2)(b)(ii)): twice the rate or 5%, whichever is higher,
//!   but not more than 20%. Not for a non-resident without a permanent
//!   establishment in India (s.397(2)(d)).
//! - **Declaration** (s.394(2)): no collection for Sl. Nos. 1 to 5 when a
//!   resident buyer declares the goods are for manufacturing, processing,
//!   producing or power, not trading.
//! - **Who collects** (s.402(33)): a government, local authority, company,
//!   firm or co-operative; an individual or HUF only when its turnover in
//!   the previous tax year exceeded ₹1 crore (business) or ₹50 lakh
//!   (profession). Shown as a condition: the books cannot check it.
//! - **Rounding** to the nearest rupee, half up, as for TDS (an assumption,
//!   **UNCERTAIN**).
//! - **The amount**: what the buyer pays (s.394(1)(c) "amount payable by
//!   the buyer"); whether GST is part of it is not settled in the sources
//!   (**UNCERTAIN**). Callers pass the amount they read as payable.
//!
//! Left out: LRS remittances (Sl. No. 7, collected only by authorised
//! dealers, i.e. banks) and the "other goods" the Central Government
//! notifies under Sl. No. 6(b) (the notification under the 2025 Act was
//! not retrieved).

use std::fmt;

use chrono::{Datelike, NaiveDate};
use khata_core::{Amount, DeducteeKind};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::tds::{LowerCertificate, Quarter, rupees, round_rupee};

const fn d(units: u32, scale: u32) -> Decimal {
    Decimal::from_parts(units, 0, 0, false, scale)
}

/// The rates below apply to collections from this date (the 2025 Act and
/// the Finance Act 2026 both commenced 1 Apr 2026).
pub const RULES_FROM: NaiveDate = match NaiveDate::from_ymd_opt(2026, 4, 1) {
    Some(x) => x,
    None => panic!("valid date"),
};

/// `payable_for` on a TCS payable ledger that holds every nature.
pub const ALL_NATURES: &str = "all";

/// The no-PAN floor (s.397(2)(b)(ii)(B)) and ceiling.
pub const NO_PAN_FLOOR: Decimal = d(5, 0);
pub const NO_PAN_CAP: Decimal = d(20, 0);

/// When a nature attracts TCS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "basis")]
pub enum TcsThreshold {
    /// Every amount.
    Every,
    /// Only when one sale's consideration exceeds the limit; then on the
    /// whole of it (Sl. No. 6, "sale consideration exceeding ten lakh").
    SaleOver { limit: Amount },
}

/// One nature of collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NatureOfCollection {
    /// Stable code stored on ledgers.
    pub code: &'static str,
    pub name: &'static str,
    /// The 1961 Act's provision ("206C(1)").
    pub old_section: &'static str,
    /// The row of the s.394(1) Table ("Sl. No. 4").
    pub table: &'static str,
    /// The 4-digit code statements use (Form 143 Annexure 2).
    pub tax_code: &'static str,
    /// Percent.
    pub rate: Decimal,
    pub threshold: TcsThreshold,
    /// Who collects ("Seller", "Licensor or lessor").
    pub collector: &'static str,
    /// The buyer's s.394(2) declaration can stop the collection.
    pub declaration_allowed: bool,
    /// A condition the books cannot check.
    pub condition: Option<&'static str>,
    pub caution: Option<&'static str>,
}

impl NatureOfCollection {
    /// "s.394(1) [Table: Sl. No. 4] (ex-206C(1))".
    pub fn section(&self) -> String {
        format!("s.394(1) [Table: {}] (ex-{})", self.table, self.old_section)
    }

    pub fn threshold_text(&self) -> String {
        match self.threshold {
            TcsThreshold::Every => "every amount".into(),
            TcsThreshold::SaleOver { limit } => format!("a sale over ₹{}", rupees(limit)),
        }
    }
}

const SELLER: &str = "Collected only by a government, local authority, company, firm or co-operative, or by an \
                      individual or HUF whose turnover in the previous tax year exceeded ₹1 crore (business) or \
                      ₹50 lakh (profession) (s.402(33)).";
const GOODS_BUYERS: &str = "Not from a public sector company, a government, an embassy or consulate, a club, or a \
                            retail buyer for personal consumption (s.402(6), Sl. No. 2): mark such a buyer as \
                            excluded.";
const SITE_CAUTION: &str = "Applies to the grant of use for business, excluding mining and quarrying of mineral oil; \
                            check the licensee or lessee exclusions in s.402 before collecting.";

/// Natures of collection for ordinary businesses with a rate and limit in
/// the Act as in force from 1 Apr 2026.
pub const NATURES: &[NatureOfCollection] = &[
    NatureOfCollection {
        code: "liquor",
        name: "Sale of alcoholic liquor for human consumption",
        old_section: "206C(1)",
        table: "Sl. No. 1",
        tax_code: "1068",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(GOODS_BUYERS),
    },
    NatureOfCollection {
        code: "tendu",
        name: "Sale of tendu leaves",
        old_section: "206C(1)",
        table: "Sl. No. 2",
        tax_code: "1069",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(GOODS_BUYERS),
    },
    NatureOfCollection {
        code: "timber_lease",
        name: "Sale of timber obtained under a forest lease",
        old_section: "206C(1)",
        table: "Sl. No. 3",
        tax_code: "1070",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(GOODS_BUYERS),
    },
    NatureOfCollection {
        code: "timber_other",
        name: "Sale of timber obtained otherwise than under a forest lease",
        old_section: "206C(1)",
        table: "Sl. No. 3",
        tax_code: "1071",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(GOODS_BUYERS),
    },
    NatureOfCollection {
        code: "forest_produce",
        name: "Sale of other forest produce (not timber or tendu leaves) obtained under a forest lease",
        old_section: "206C(1)",
        table: "Sl. No. 3",
        tax_code: "1072",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(GOODS_BUYERS),
    },
    NatureOfCollection {
        code: "scrap",
        name: "Sale of scrap",
        old_section: "206C(1)",
        table: "Sl. No. 4",
        tax_code: "1073",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(
            "Scrap is waste and scrap from manufacture or mechanical working of materials that is definitely not \
             usable as such (s.402(31)). Not from a public sector company, a government, an embassy, a club or a \
             retail buyer for personal consumption: mark such a buyer as excluded.",
        ),
    },
    NatureOfCollection {
        code: "minerals",
        name: "Sale of minerals, being coal, lignite or iron ore",
        old_section: "206C(1)",
        table: "Sl. No. 5",
        tax_code: "1074",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller",
        declaration_allowed: true,
        condition: Some(SELLER),
        caution: Some(GOODS_BUYERS),
    },
    NatureOfCollection {
        code: "motor_vehicle",
        name: "Sale of a motor vehicle for over ₹10 lakh",
        old_section: "206C(1F)",
        table: "Sl. No. 6",
        tax_code: "1075",
        rate: d(1, 0),
        threshold: TcsThreshold::SaleOver { limit: d(1_000_000, 0) },
        collector: "Seller",
        declaration_allowed: false,
        condition: Some(SELLER),
        caution: Some(
            "The limit is read per sale (\"sale consideration exceeding ten lakh rupees\"). Not from a government, \
             an embassy, a local authority, or a public sector company carrying passengers (s.402(6), Sl. No. 3).",
        ),
    },
    NatureOfCollection {
        code: "tour_package",
        name: "Sale of an overseas tour programme package",
        old_section: "206C(1G)",
        table: "Sl. No. 8",
        tax_code: "1088",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Seller of the package",
        declaration_allowed: false,
        condition: None,
        caution: Some(
            "2% on any amount from 1 Apr 2026 (Finance Act 2026 s.85). Nothing is collected when the buyer is liable \
             to deduct TDS on it and has deducted (s.394(5)). The file format still lists two codes (1088 up to the \
             old limit, 1089 above it); KhataOS reports 1088 [UNCERTAIN: confirm in the RPU].",
        ),
    },
    NatureOfCollection {
        code: "parking",
        name: "Use of a parking lot for business",
        old_section: "206C(1C)",
        table: "Sl. No. 9",
        tax_code: "1090",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Licensor or lessor",
        declaration_allowed: false,
        condition: None,
        caution: Some(SITE_CAUTION),
    },
    NatureOfCollection {
        code: "toll_plaza",
        name: "Use of a toll plaza for business",
        old_section: "206C(1C)",
        table: "Sl. No. 9",
        tax_code: "1091",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Licensor or lessor",
        declaration_allowed: false,
        condition: None,
        caution: Some(SITE_CAUTION),
    },
    NatureOfCollection {
        code: "mine_quarry",
        name: "Use of a mine or quarry for business (not mineral oil)",
        old_section: "206C(1C)",
        table: "Sl. No. 9",
        tax_code: "1092",
        rate: d(2, 0),
        threshold: TcsThreshold::Every,
        collector: "Licensor or lessor",
        declaration_allowed: false,
        condition: None,
        caution: Some(SITE_CAUTION),
    },
];

pub fn nature(code: &str) -> Option<&'static NatureOfCollection> {
    NATURES.iter().find(|n| n.code.eq_ignore_ascii_case(code.trim()))
}

/// One sale or receipt to work TCS out for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcsInput<'a> {
    pub nature: &'a str,
    pub collectee: Option<DeducteeKind>,
    /// A valid PAN of the collectee is on record.
    pub pan: bool,
    pub date: NaiveDate,
    /// The amount received or debited to the buyer.
    pub amount: Amount,
    /// The buyer gave the s.394(2) declaration.
    pub declaration: bool,
    /// The party is not a "buyer" (or licensee) under the Act.
    pub excluded: bool,
    pub non_resident: bool,
    pub permanent_establishment: bool,
    /// A lower-collection certificate (s.395(3)).
    pub certificate: Option<LowerCertificate>,
}

/// Why the rate is what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TcsBasis {
    Normal,
    NoPan,
    LowerCertificate,
    /// Nothing: the buyer's declaration.
    Declaration,
    /// Nothing: not a buyer under the Act.
    Excluded,
    /// Nothing: under the limit.
    BelowLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TcsResult {
    pub nature: &'static str,
    pub section: String,
    pub tax_code: &'static str,
    /// Percent applied (0 when nothing is collected).
    pub rate: Decimal,
    pub basis: TcsBasis,
    /// The amount the rate is applied to.
    pub base: Amount,
    /// TCS to collect, rounded to the rupee.
    pub tcs: Amount,
    /// The statement's "reason" flag (Form 143 field 32), if any:
    /// A certificate, B declaration, C no PAN.
    pub remark: Option<char>,
    pub explanation: String,
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcsError {
    UnknownNature(String),
    BeforeRules(NaiveDate),
    NegativeAmount,
}

impl fmt::Display for TcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNature(c) => write!(f, "{c:?} is not a TCS nature of collection KhataOS knows"),
            Self::BeforeRules(d) => write!(f, "TCS rates before 1 Apr 2026 are not in KhataOS ({d} is earlier)"),
            Self::NegativeAmount => write!(f, "TCS is worked out on amounts that are not negative"),
        }
    }
}

impl std::error::Error for TcsError {}

fn pct(r: Decimal) -> String {
    format!("{}%", r.normalize())
}

/// The rate without a PAN: twice the rate or 5%, whichever is higher, at
/// most 20% (s.397(2)(b)(ii)).
pub fn no_pan_rate(rate: Decimal) -> Decimal {
    (rate * Decimal::TWO).max(NO_PAN_FLOOR).min(NO_PAN_CAP)
}

/// TCS on one sale or receipt.
pub fn compute(input: &TcsInput<'_>) -> Result<TcsResult, TcsError> {
    let n = nature(input.nature).ok_or_else(|| TcsError::UnknownNature(input.nature.to_owned()))?;
    if input.date < RULES_FROM {
        return Err(TcsError::BeforeRules(input.date));
    }
    if input.amount.is_sign_negative() {
        return Err(TcsError::NegativeAmount);
    }
    let mut cautions: Vec<String> = Vec::new();
    let section = n.section();
    let nothing = |basis: TcsBasis, remark: Option<char>, why: String, cautions: Vec<String>| TcsResult {
        nature: n.code,
        section: section.clone(),
        tax_code: n.tax_code,
        rate: Decimal::ZERO,
        basis,
        base: Amount::ZERO,
        tcs: Amount::ZERO,
        remark,
        explanation: format!("{section} {}: no TCS, {why}.", n.name),
        cautions,
    };
    cautions.extend(n.condition.map(str::to_owned));
    cautions.extend(n.caution.map(str::to_owned));
    if input.excluded {
        return Ok(nothing(TcsBasis::Excluded, None, "the party is marked as not a buyer under the Act".into(), cautions));
    }
    if input.declaration && !input.pan {
        cautions.push("The buyer's declaration is not valid without a PAN (s.397(2)(f)(i) and (g)).".into());
    } else if input.declaration {
        if n.declaration_allowed && !input.non_resident {
            return Ok(nothing(TcsBasis::Declaration, Some('B'), "the buyer declared the goods are not for trading (s.394(2))".into(), cautions));
        }
        cautions.push(format!(
            "The buyer's declaration is ignored: s.394(2) covers only resident buyers of Sl. Nos. 1 to 5, not {}.",
            n.name.to_lowercase()
        ));
    }
    if let TcsThreshold::SaleOver { limit } = n.threshold {
        if input.amount <= limit {
            return Ok(nothing(TcsBasis::BelowLimit, None, format!("the sale is not over ₹{}", rupees(limit)), cautions));
        }
    }
    let pan_rule_applies = !input.non_resident || input.permanent_establishment;
    if input.certificate.is_some() && !input.pan {
        cautions.push("The lower-collection certificate is ignored without a PAN (s.397(2)(f)).".into());
    }
    let certificate = input.certificate.as_ref().filter(|_| input.pan);
    let (rate, basis, remark) = match (certificate, input.pan) {
        (_, false) if pan_rule_applies => (no_pan_rate(n.rate), TcsBasis::NoPan, Some('C')),
        (Some(c), _) if c.valid_on(input.date) => (c.rate, TcsBasis::LowerCertificate, Some('A')),
        (Some(c), _) => {
            let number = c.number.as_deref().unwrap_or("on record");
            cautions.push(format!("Lower-collection certificate {number} is not valid on {}; the normal rate is used.", input.date));
            (n.rate, TcsBasis::Normal, None)
        }
        (None, _) => {
            if !input.pan {
                cautions.push("No PAN, but a non-resident without a permanent establishment in India is not charged the no-PAN rate (s.397(2)(d)).".into());
            }
            (n.rate, TcsBasis::Normal, None)
        }
    };
    let base = input.amount;
    let tcs = round_rupee(base * rate / Decimal::ONE_HUNDRED);
    let note = match basis {
        TcsBasis::NoPan => format!(" (no PAN: twice {} or 5%, at most 20%)", pct(n.rate)),
        TcsBasis::LowerCertificate => format!(
            " (lower-collection certificate{})",
            input.certificate.as_ref().and_then(|c| c.number.as_deref()).map(|x| format!(" {x}")).unwrap_or_default()
        ),
        _ => String::new(),
    };
    let explanation = format!("{section} {}: ₹{} at {}{note} on ₹{}.", n.name, rupees(tcs), pct(rate), rupees(base));
    Ok(TcsResult { nature: n.code, section, tax_code: n.tax_code, rate, basis, base, tcs, remark, explanation, cautions })
}

/// The quarterly TCS statement's form: Form 143 from tax year 2026-27,
/// Form 27EQ before (research doc §5).
pub fn statement_form(q: Quarter) -> &'static str {
    if q.year >= RULES_FROM.year() { "Form 143" } else { "Form 27EQ" }
}

/// When TCS collected on `date` must be paid: within seven days from the
/// end of the month, March included. This is the old Rule 37CA; the
/// Income-tax Rules 2026 equivalent is reported the same by secondary
/// sources only (**UNCERTAIN**).
pub fn deposit_due(date: NaiveDate) -> NaiveDate {
    let (y, m) = if date.month() == 12 { (date.year() + 1, 1) } else { (date.year(), date.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 7).expect("the 7th exists")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn day(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    fn input(nature: &str, amount: Amount) -> TcsInput<'_> {
        TcsInput {
            nature,
            collectee: Some(DeducteeKind::Company),
            pan: true,
            date: day(2026, 6, 15),
            amount,
            declaration: false,
            excluded: false,
            non_resident: false,
            permanent_establishment: false,
            certificate: None,
        }
    }

    #[test]
    fn natures_are_unique_and_fit_a_ledger() {
        for (i, n) in NATURES.iter().enumerate() {
            assert!(n.code.len() <= khata_core::chart::MAX_TDS_NATURE, "{}", n.code);
            assert!(!NATURES[..i].iter().any(|m| m.code == n.code || m.tax_code == n.tax_code), "{}", n.code);
            assert_ne!(n.code, ALL_NATURES);
            assert!(n.tax_code.len() == 4 && n.tax_code.bytes().all(|b| b.is_ascii_digit()));
            // Every nature this crate also has as TDS stays distinct.
            assert!(crate::tds::nature(n.code).is_none(), "{}", n.code);
        }
        assert_eq!(nature(" Scrap ").unwrap().section(), "s.394(1) [Table: Sl. No. 4] (ex-206C(1))");
    }

    #[test]
    fn rates_follow_the_finance_act_2026() {
        // Sl. Nos. 1, 2, 4, 5 moved to 2% (FA 2026 s.85); Sl. No. 3 was 2% already.
        for code in ["liquor", "tendu", "timber_lease", "timber_other", "forest_produce", "scrap", "minerals", "parking", "toll_plaza", "mine_quarry"] {
            let r = compute(&input(code, dec!(100_000))).unwrap();
            assert_eq!((r.rate, r.tcs, r.basis), (dec!(2), dec!(2_000), TcsBasis::Normal), "{code}");
        }
        // Overseas tour package: 2% on any amount.
        assert_eq!(compute(&input("tour_package", dec!(1_500_000))).unwrap().tcs, dec!(30_000));
        assert_eq!(compute(&input("tour_package", dec!(100))).unwrap().tcs, dec!(2));
        assert_eq!(compute(&input("scrap", dec!(25_025))).unwrap().tcs, dec!(501), "500.50 rounds up");
    }

    #[test]
    fn motor_vehicle_only_over_ten_lakh() {
        let r = compute(&input("motor_vehicle", dec!(1_000_000))).unwrap();
        assert_eq!((r.tcs, r.basis), (dec!(0), TcsBasis::BelowLimit), "exactly ₹10 lakh is not over it");
        let r = compute(&input("motor_vehicle", dec!(1_000_001))).unwrap();
        assert_eq!((r.base, r.tcs, r.rate), (dec!(1_000_001), dec!(10_000), dec!(1)), "1% on the whole consideration");
        assert_eq!(r.tax_code, "1075");
    }

    #[test]
    fn no_pan_doubles_with_a_floor_and_a_cap() {
        assert_eq!((no_pan_rate(dec!(1)), no_pan_rate(dec!(2)), no_pan_rate(dec!(5)), no_pan_rate(dec!(20))), (dec!(5), dec!(5), dec!(10), dec!(20)));
        let mut i = input("scrap", dec!(100_000));
        i.pan = false;
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.tcs, r.basis, r.remark), (dec!(5), dec!(5_000), TcsBasis::NoPan, Some('C')));
        // A non-resident without a permanent establishment: normal rate.
        i.non_resident = true;
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.basis), (dec!(2), TcsBasis::Normal));
        assert!(r.cautions.iter().any(|c| c.contains("s.397(2)(d)")));
        i.permanent_establishment = true;
        assert_eq!(compute(&i).unwrap().rate, dec!(5));
    }

    #[test]
    fn declaration_exclusion_and_certificate() {
        let mut i = input("scrap", dec!(100_000));
        i.declaration = true;
        let r = compute(&i).unwrap();
        assert_eq!((r.tcs, r.basis, r.remark), (dec!(0), TcsBasis::Declaration, Some('B')));
        // Not for a motor vehicle, nor for a non-resident buyer.
        let mut m = input("motor_vehicle", dec!(2_000_000));
        m.declaration = true;
        let r = compute(&m).unwrap();
        assert_eq!(r.tcs, dec!(20_000));
        assert!(r.cautions.iter().any(|c| c.contains("declaration is ignored")));
        i.non_resident = true;
        assert_eq!(compute(&i).unwrap().tcs, dec!(2_000));
        // Without a PAN the declaration is void: the no-PAN rate applies.
        let mut np = input("scrap", dec!(100_000));
        np.declaration = true;
        np.pan = false;
        let r = compute(&np).unwrap();
        assert_eq!((r.tcs, r.basis), (dec!(5_000), TcsBasis::NoPan));
        assert!(r.cautions.iter().any(|c| c.contains("not valid without a PAN")));
        // Excluded buyer: nothing, no flag.
        let mut x = input("liquor", dec!(50_000));
        x.excluded = true;
        let r = compute(&x).unwrap();
        assert_eq!((r.tcs, r.basis, r.remark), (dec!(0), TcsBasis::Excluded, None));
        // Lower-collection certificate while valid.
        let mut c = input("scrap", dec!(100_000));
        c.certificate = Some(LowerCertificate { rate: dec!(0.5), number: Some("LCC-1".into()), valid_from: Some(day(2026, 4, 1)), valid_to: Some(day(2026, 9, 30)) });
        let r = compute(&c).unwrap();
        assert_eq!((r.rate, r.tcs, r.remark), (dec!(0.5), dec!(500), Some('A')));
        assert!(r.explanation.contains("LCC-1"));
        c.date = day(2026, 10, 1);
        let r = compute(&c).unwrap();
        assert_eq!((r.rate, r.remark), (dec!(2), None));
        assert!(r.cautions.iter().any(|x| x.contains("not valid")));
        c.pan = false;
        let r = compute(&c).unwrap();
        assert_eq!(r.rate, dec!(5));
        assert!(r.cautions.iter().any(|x| x.contains("ignored without a PAN")));
    }

    #[test]
    fn dates_and_errors() {
        let mut i = input("scrap", dec!(1));
        i.date = day(2026, 3, 31);
        assert_eq!(compute(&i), Err(TcsError::BeforeRules(day(2026, 3, 31))));
        assert_eq!(compute(&input("goods", dec!(1))), Err(TcsError::UnknownNature("goods".into())), "206C(1H) is gone");
        assert_eq!(compute(&input("scrap", dec!(-1))), Err(TcsError::NegativeAmount));
        assert_eq!(statement_form(Quarter { year: 2026, q: 1 }), "Form 143");
        assert_eq!(statement_form(Quarter { year: 2025, q: 4 }), "Form 27EQ");
        assert_eq!(deposit_due(day(2026, 5, 31)), day(2026, 6, 7));
        assert_eq!(deposit_due(day(2027, 3, 31)), day(2027, 4, 7), "March too: no 30 April for TCS");
        assert_eq!(deposit_due(day(2026, 12, 1)), day(2027, 1, 7));
    }
}

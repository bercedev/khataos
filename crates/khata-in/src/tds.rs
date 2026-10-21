//! TDS (tax deducted at source) on payments to residents: natures of
//! payment, rates, thresholds and the amount to deduct.
//!
//! Sources: `docs/research/india-compliance.md` §5 (the first seed, all
//! SECONDARY) and `docs/research/india-tds-tcs.md`, which checked every rate
//! and threshold below against the gazetted Income-tax Act 2025 (s.393
//! tables) and the Finance Act 2026 [PRIMARY]. Read `docs/india-tds.md`
//! before relying on them.
//!
//! - **Two Acts by date.** The Income-tax Act 2025 commenced 1 Apr 2026;
//!   resident TDS sits in the tables of s.393(1) and s.393(3), non-residents
//!   in s.393(2). Payments up to 31 Mar 2026 fall under the 1961 Act's
//!   sections (194C, 194J…). Table serials and the 4-digit tax codes come
//!   from the Act and from Protean's file format for Form 140 v1.2
//!   (Annexure 2) [PRIMARY].
//! - **Non-residents** (old s.195, new s.393(2) Sl. No. 17): the rate
//!   depends on the treaty and the kind of income, so it is always the
//!   **user's** ([`TdsInput::user_rate`]); KhataOS supplies none.
//! - **From 1 Apr 2025 only.** The research gives the FA 2025 thresholds
//!   ("from 1 Apr 2025"); earlier dates had other thresholds that it does
//!   not list, so [`compute`] refuses them.
//! - **No-PAN rate** (old s.206AA equivalent): "20% (or higher)", applied
//!   as the higher of 20% and the normal rate.
//! - **Rounding** to the nearest rupee, half up: an assumption, the
//!   research doc is silent (**UNCERTAIN**).
//! - **Crossing an annual limit**: TDS on the year's total so far, less TDS
//!   already deducted. The research only gives the limits; deducting on
//!   the whole aggregate once it is crossed is the common reading
//!   (**UNCERTAIN**). Purchase of goods (194Q) is taxed only "above
//!   ₹50 lakh", as the research says.

use std::fmt;

use chrono::{Datelike, NaiveDate};
use khata_core::{Amount, DeducteeKind};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

/// `units / 10^scale`, usable in constants.
const fn d(units: u32, scale: u32) -> Decimal {
    Decimal::from_parts(units, 0, 0, false, scale)
}

/// The Income-tax Act 2025 applies to payments from this date.
pub const NEW_ACT_FROM: NaiveDate = match NaiveDate::from_ymd_opt(2026, 4, 1) {
    Some(x) => x,
    None => panic!("valid date"),
};

/// The rates and thresholds below are those in force from this date
/// (Finance Act 2025 thresholds).
pub const RULES_FROM: NaiveDate = match NaiveDate::from_ymd_opt(2025, 4, 1) {
    Some(x) => x,
    None => panic!("valid date"),
};

/// The no-PAN rate, used when it is higher than the normal rate
/// (s.397(2)(b)(i)(C) of the 2025 Act; old s.206AA).
pub const NO_PAN_RATE: Decimal = d(20, 0);
/// The no-PAN rate for purchase of goods (s.397(2)(b)(i)(C): 5% for
/// s.393(1) Sl. No. 8(ii) and 8(v)).
pub const NO_PAN_RATE_GOODS: Decimal = d(5, 0);

/// `payable_for` on a TDS payable ledger that holds every nature.
pub const ALL_NATURES: &str = "all";

/// Which Act a payment falls under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Act {
    /// Income-tax Act 1961: payments up to 31 Mar 2026.
    Ita1961,
    /// Income-tax Act 2025: payments from 1 Apr 2026.
    Ita2025,
}

impl Act {
    pub fn for_date(date: NaiveDate) -> Self {
        if date < NEW_ACT_FROM { Self::Ita1961 } else { Self::Ita2025 }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ita1961 => "Income-tax Act 1961",
            Self::Ita2025 => "Income-tax Act 2025",
        }
    }
}

/// When a nature attracts TDS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "basis")]
pub enum Threshold {
    /// Every payment, whatever its size (non-residents).
    Every,
    /// No TDS unless one payment exceeds `single`, or the financial year's
    /// total to the party for this nature exceeds `aggregate`.
    Annual { single: Option<Amount>, aggregate: Option<Amount> },
    /// TDS only on the part of the year's total above the limit (194Q).
    AnnualExcess { limit: Amount },
    /// The amount for a month exceeds the limit (194I rent).
    Monthly { limit: Amount },
    /// The consideration for one transaction is the limit or more (194-IA).
    PerTransaction { limit: Amount },
}

/// Where the deduction is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Statement {
    /// The quarterly statement for resident non-salary TDS (Form 140,
    /// ex-26Q).
    Quarterly,
    /// A challan-cum-statement per transaction (Form 141, ex-26QB).
    ChallanCumStatement,
    /// The quarterly statement for payments to non-residents (Form 144,
    /// ex-27Q).
    NonResident,
}

/// One nature of payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NatureOfPayment {
    /// Stable code stored on ledgers.
    pub code: &'static str,
    pub name: &'static str,
    /// Section under the 1961 Act ("194C").
    pub old_section: &'static str,
    /// Provision under the 2025 Act ("s.393(1)").
    pub new_section: &'static str,
    /// The row of that section's table ("Sl. No. 6(i)").
    pub table: &'static str,
    /// The 4-digit tax code statements use from tax year 2026-27, for an
    /// individual or HUF deductee and for others (Form 140 / 144 file
    /// format, Annexure 2). Empty where the nature is not reported on a
    /// quarterly statement.
    pub tax_code_individual_huf: &'static str,
    pub tax_code_others: &'static str,
    /// The rate is the user's (non-residents); the two rates above are 0.
    pub user_rate: bool,
    /// Percent, for an individual or HUF deductee.
    pub rate_individual_huf: Decimal,
    /// Percent, for every other deductee.
    pub rate_others: Decimal,
    pub threshold: Threshold,
    pub statement: Statement,
    /// A condition the software cannot check from the books.
    pub condition: Option<&'static str>,
    /// What is uncertain about this nature.
    pub caution: Option<&'static str>,
}

impl NatureOfPayment {
    pub fn rate(&self, deductee: Option<DeducteeKind>) -> Decimal {
        match deductee {
            Some(DeducteeKind::Individual | DeducteeKind::Huf) => self.rate_individual_huf,
            _ => self.rate_others,
        }
    }

    /// The 4-digit tax code for this deductee (statements from 2026-27).
    pub fn tax_code(&self, deductee: Option<DeducteeKind>) -> &'static str {
        match deductee {
            Some(DeducteeKind::Individual | DeducteeKind::Huf) => self.tax_code_individual_huf,
            _ => self.tax_code_others,
        }
    }

    /// "s.194C" before 1 Apr 2026, "s.393(1) (ex-194C)" from then.
    pub fn section(&self, date: NaiveDate) -> String {
        match Act::for_date(date) {
            Act::Ita1961 => format!("s.{}", self.old_section),
            Act::Ita2025 => format!("{} (ex-{})", self.new_section, self.old_section),
        }
    }

    /// The threshold in words, for pickers and reports.
    pub fn threshold_text(&self) -> String {
        match self.threshold {
            Threshold::Annual { single: Some(s), aggregate: Some(a) } => {
                format!("one payment over ₹{} or over ₹{} in the year", rupees(s), rupees(a))
            }
            Threshold::Annual { single: Some(s), aggregate: None } => format!("one payment over ₹{}", rupees(s)),
            Threshold::Annual { single: None, aggregate: Some(a) } => format!("over ₹{} in the year", rupees(a)),
            Threshold::Annual { single: None, aggregate: None } | Threshold::Every => "every payment".into(),
            Threshold::AnnualExcess { limit } => format!("on the part above ₹{} in the year", rupees(limit)),
            Threshold::Monthly { limit } => format!("over ₹{} a month", rupees(limit)),
            Threshold::PerTransaction { limit } => format!("₹{} or more in one transaction", rupees(limit)),
        }
    }
}

const RENT_CAUTION: &str = "The limit is ₹50,000 for a month or part of a month; the amount is taken as one \
                            month's rent (split a quarterly or yearly bill by month yourself).";
const AGGREGATE_CAUTION: &str = "From 1 Apr 2026 the Act applies the limit to the amount or aggregate of amounts in \
                                 the tax year (s.393(1)(a)); for earlier payments that reading is from secondary \
                                 sources [UNCERTAIN].";

/// What to tell the user about every payment to a non-resident.
pub const NON_RESIDENT_REMINDER: &str = "The rate is yours: take it from the tax treaty (with the payee's tax \
    residency certificate) or the Act, including any surcharge and cess that apply; KhataOS does not know \
    non-resident rates. Before remitting, file Form 145 (ex-Form 15CA) and, where required, get the \
    accountant's Form 146 (ex-Form 15CB) [form numbers SECONDARY: verify on the e-filing portal].";

/// Natures of payment KhataOS knows. Others (salary is in `salary.rs`;
/// dividends, lottery, insurance commission, rent by individuals, joint
/// development, e-commerce, virtual digital assets…) are left out; see
/// `docs/india-tds.md`.
pub const NATURES: &[NatureOfPayment] = &[
    NatureOfPayment {
        code: "contract",
        name: "Payments to contractors and sub-contractors",
        old_section: "194C",
        new_section: "s.393(1)",
        table: "Sl. No. 6(i)",
        tax_code_individual_huf: "1023",
        tax_code_others: "1024",
        user_rate: false,
        rate_individual_huf: d(1, 0),
        rate_others: d(2, 0),
        threshold: Threshold::Annual { single: Some(d(30_000, 0)), aggregate: Some(d(100_000, 0)) },
        statement: Statement::Quarterly,
        condition: None,
        caution: None,
    },
    NatureOfPayment {
        code: "professional",
        name: "Fees for professional services",
        old_section: "194J",
        new_section: "s.393(1)",
        table: "Sl. No. 6(iii)",
        tax_code_individual_huf: "1027",
        tax_code_others: "1027",
        user_rate: false,
        rate_individual_huf: d(10, 0),
        rate_others: d(10, 0),
        threshold: Threshold::Annual { single: None, aggregate: Some(d(50_000, 0)) },
        statement: Statement::Quarterly,
        condition: None,
        caution: Some(AGGREGATE_CAUTION),
    },
    NatureOfPayment {
        code: "technical",
        name: "Fees for technical services, call centres, film royalty",
        old_section: "194J",
        new_section: "s.393(1)",
        table: "Sl. No. 6(iii)",
        tax_code_individual_huf: "1026",
        tax_code_others: "1026",
        user_rate: false,
        rate_individual_huf: d(2, 0),
        rate_others: d(2, 0),
        threshold: Threshold::Annual { single: None, aggregate: Some(d(50_000, 0)) },
        statement: Statement::Quarterly,
        condition: None,
        caution: Some(AGGREGATE_CAUTION),
    },
    NatureOfPayment {
        code: "rent_building",
        name: "Rent of land, building or furniture",
        old_section: "194I",
        new_section: "s.393(1)",
        table: "Sl. No. 2(ii)",
        tax_code_individual_huf: "1009",
        tax_code_others: "1009",
        user_rate: false,
        rate_individual_huf: d(10, 0),
        rate_others: d(10, 0),
        threshold: Threshold::Monthly { limit: d(50_000, 0) },
        statement: Statement::Quarterly,
        condition: None,
        caution: Some(RENT_CAUTION),
    },
    NatureOfPayment {
        code: "rent_machinery",
        name: "Rent of plant and machinery",
        old_section: "194I",
        new_section: "s.393(1)",
        table: "Sl. No. 2(ii)",
        tax_code_individual_huf: "1008",
        tax_code_others: "1008",
        user_rate: false,
        rate_individual_huf: d(2, 0),
        rate_others: d(2, 0),
        threshold: Threshold::Monthly { limit: d(50_000, 0) },
        statement: Statement::Quarterly,
        condition: None,
        caution: Some(RENT_CAUTION),
    },
    NatureOfPayment {
        code: "commission",
        name: "Commission or brokerage",
        old_section: "194H",
        new_section: "s.393(1)",
        table: "Sl. No. 1(ii)",
        tax_code_individual_huf: "1006",
        tax_code_others: "1006",
        user_rate: false,
        rate_individual_huf: d(2, 0),
        rate_others: d(2, 0),
        threshold: Threshold::Annual { single: None, aggregate: Some(d(20_000, 0)) },
        statement: Statement::Quarterly,
        condition: None,
        caution: Some(AGGREGATE_CAUTION),
    },
    NatureOfPayment {
        code: "interest",
        name: "Interest other than on securities (payer not a bank)",
        old_section: "194A",
        new_section: "s.393(1)",
        table: "Sl. No. 5(iii)",
        tax_code_individual_huf: "1022",
        tax_code_others: "1022",
        user_rate: false,
        rate_individual_huf: d(10, 0),
        rate_others: d(10, 0),
        threshold: Threshold::Annual { single: None, aggregate: Some(d(10_000, 0)) },
        statement: Statement::Quarterly,
        condition: Some(
            "Only for a payer that is not a bank: the research gives ₹50,000 (₹1 lakh for senior citizens) \
             for banks, which this nature does not cover.",
        ),
        caution: Some(AGGREGATE_CAUTION),
    },
    NatureOfPayment {
        code: "property",
        name: "Purchase of immovable property (not agricultural land)",
        old_section: "194-IA",
        new_section: "s.393(1)",
        table: "Sl. No. 3(i)",
        tax_code_individual_huf: "",
        tax_code_others: "",
        user_rate: false,
        rate_individual_huf: d(1, 0),
        rate_others: d(1, 0),
        threshold: Threshold::PerTransaction { limit: d(5_000_000, 0) },
        statement: Statement::ChallanCumStatement,
        condition: Some("Reported per transaction on the challan-cum-statement, not in the quarterly statement."),
        caution: Some(
            "From 1 Apr 2026 the 1% is on the consideration or the stamp duty value, whichever is higher \
             (s.393(1) Sl. No. 3(i)): enter the higher amount.",
        ),
    },
    NatureOfPayment {
        code: "goods",
        name: "Purchase of goods",
        old_section: "194Q",
        new_section: "s.393(1)",
        table: "Sl. No. 8(ii)",
        tax_code_individual_huf: "1031",
        tax_code_others: "1031",
        user_rate: false,
        rate_individual_huf: d(1, 1),
        rate_others: d(1, 1),
        threshold: Threshold::AnnualExcess { limit: d(5_000_000, 0) },
        statement: Statement::Quarterly,
        condition: Some(
            "Only when your turnover in the previous tax year exceeded ₹10 crore, and not on a purchase where the \
             seller collects TCS (s.393(1) Sl. No. 8(ii), Note 1).",
        ),
        caution: Some("Whether GST is part of the amount is not settled in the sources [UNCERTAIN]."),
    },
    NatureOfPayment {
        code: "partner",
        name: "Salary, remuneration, commission or interest to a partner",
        old_section: "194T",
        new_section: "s.393(3)",
        table: "Sl. No. 7",
        tax_code_individual_huf: "1067",
        tax_code_others: "1067",
        user_rate: false,
        rate_individual_huf: d(10, 0),
        rate_others: d(10, 0),
        threshold: Threshold::Annual { single: None, aggregate: Some(d(20_000, 0)) },
        statement: Statement::Quarterly,
        condition: Some("Paid by a firm to its partner (in force from 1 Apr 2025)."),
        caution: Some(AGGREGATE_CAUTION),
    },
    NatureOfPayment {
        code: "non_resident",
        name: "Interest or any other sum to a non-resident (not salary)",
        old_section: "195",
        new_section: "s.393(2)",
        table: "Sl. No. 17",
        tax_code_individual_huf: "1057",
        tax_code_others: "1057",
        user_rate: true,
        rate_individual_huf: Decimal::ZERO,
        rate_others: Decimal::ZERO,
        threshold: Threshold::Every,
        statement: Statement::NonResident,
        condition: Some(NON_RESIDENT_REMINDER),
        caution: None,
    },
];

pub fn nature(code: &str) -> Option<&'static NatureOfPayment> {
    NATURES.iter().find(|n| n.code.eq_ignore_ascii_case(code.trim()))
}

/// A lower- or nil-deduction certificate held by the deductee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerCertificate {
    /// Percent.
    pub rate: Decimal,
    pub number: Option<String>,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
}

impl LowerCertificate {
    pub fn valid_on(&self, date: NaiveDate) -> bool {
        self.valid_from.is_none_or(|f| f <= date) && self.valid_to.is_none_or(|t| date <= t)
    }
}

/// One payment or credit to work TDS out for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TdsInput<'a> {
    pub nature: &'a str,
    /// `None`: not known; the rate for "others" is used (the higher one).
    pub deductee: Option<DeducteeKind>,
    /// A valid PAN of the deductee is on record.
    pub pan: bool,
    pub date: NaiveDate,
    /// This payment or credit.
    pub amount: Amount,
    /// Paid or credited to the same party for the same nature earlier in the
    /// financial year.
    pub ytd_amount: Amount,
    /// TDS already deducted on `ytd_amount`.
    pub ytd_tds: Amount,
    pub certificate: Option<LowerCertificate>,
    /// The user's rate (percent) for a nature whose rate KhataOS does not
    /// know (non-residents: the treaty or Act rate). Ignored elsewhere.
    pub user_rate: Option<Decimal>,
}

/// Which rate applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RateBasis {
    Normal,
    /// The deductee has not given a PAN.
    NoPan,
    LowerCertificate,
}

/// Which threshold decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ThresholdHit {
    /// Under every limit: no TDS.
    Below,
    /// This one payment is over the single-payment limit.
    Single,
    /// The year's total crosses the annual limit with this payment.
    AggregateCrossed,
    /// The year's total was already over the annual limit.
    AggregateAlready,
    /// Part (or all) of this payment is above the annual limit (194Q).
    Excess,
    /// The month's amount is over the monthly limit.
    Monthly,
    /// The transaction meets the limit.
    Transaction,
    /// The nature has no limit.
    Every,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TdsResult {
    pub nature: &'static str,
    pub act: Act,
    pub section: String,
    /// Percent applied.
    pub rate: Decimal,
    pub rate_basis: RateBasis,
    /// The amount the rate was applied to.
    pub base: Amount,
    /// TDS to deduct now, rounded to the rupee.
    pub tds: Amount,
    pub threshold: ThresholdHit,
    pub explanation: String,
    /// Conditions and uncertainties the user should know about.
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TdsError {
    UnknownNature(String),
    /// The research has no thresholds before 1 Apr 2025.
    BeforeRules(NaiveDate),
    NegativeAmount,
    /// A nature whose rate the user must give (non-residents) came without one.
    RateRequired(&'static str),
}

impl fmt::Display for TdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNature(c) => write!(f, "{c:?} is not a TDS nature of payment KhataOS knows"),
            Self::BeforeRules(d) => write!(f, "TDS rates before 1 Apr 2025 are not in KhataOS ({d} is earlier)"),
            Self::NegativeAmount => write!(f, "TDS is worked out on amounts that are not negative"),
            Self::RateRequired(c) => write!(
                f,
                "enter the rate for {c} on the party ledger yourself (from the tax treaty or the Act): KhataOS does not know non-resident rates"
            ),
        }
    }
}

impl std::error::Error for TdsError {}

/// Round to the nearest rupee, half up (an assumption; see the module docs).
pub fn round_rupee(a: Amount) -> Amount {
    a.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
}

fn percent_of(base: Amount, rate: Decimal) -> Amount {
    round_rupee(base * rate / Decimal::ONE_HUNDRED)
}

/// "1,00,000" (Indian grouping, no decimals when whole).
pub fn rupees(a: Amount) -> String {
    let a = a.normalize();
    let s = a.abs().to_string();
    let (int, frac) = s.split_once('.').map_or((s.as_str(), None), |(i, f)| (i, Some(f)));
    let mut out = String::new();
    let n = int.len();
    for (i, c) in int.chars().enumerate() {
        let left = n - i;
        if i > 0 && (left == 3 || (left > 3 && (left - 3) % 2 == 0)) {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    if a.is_sign_negative() && !a.is_zero() { format!("-{out}") } else { out }
}

fn pct(r: Decimal) -> String {
    format!("{}%", r.normalize())
}

/// TDS on one payment or credit.
pub fn compute(input: &TdsInput<'_>) -> Result<TdsResult, TdsError> {
    let n = nature(input.nature).ok_or_else(|| TdsError::UnknownNature(input.nature.to_owned()))?;
    if input.date < RULES_FROM {
        return Err(TdsError::BeforeRules(input.date));
    }
    if input.amount.is_sign_negative() || input.ytd_amount.is_sign_negative() || input.ytd_tds.is_sign_negative() {
        return Err(TdsError::NegativeAmount);
    }
    let mut cautions = Vec::new();
    let normal = if n.user_rate {
        let r = input.user_rate.ok_or(TdsError::RateRequired(n.code))?;
        if r.is_sign_negative() || r > Decimal::ONE_HUNDRED {
            return Err(TdsError::NegativeAmount);
        }
        r
    } else {
        n.rate(input.deductee)
    };
    let no_pan_rate = if n.code == "goods" { NO_PAN_RATE_GOODS } else { NO_PAN_RATE };
    if input.deductee.is_none() && n.rate_individual_huf != n.rate_others {
        cautions.push(format!("Deductee type not known: the rate for others ({}) is used.", pct(n.rate_others)));
    }
    let (rate, rate_basis) = match (&input.certificate, input.pan) {
        (_, false) => {
            if input.certificate.is_some() {
                cautions.push("The lower-deduction certificate is ignored without a PAN.".into());
            }
            (normal.max(no_pan_rate), RateBasis::NoPan)
        }
        (Some(c), true) if c.valid_on(input.date) => (c.rate, RateBasis::LowerCertificate),
        (Some(c), true) => {
            let number = c.number.as_deref().unwrap_or("on record");
            cautions.push(format!("Lower-deduction certificate {number} is not valid on {}; the normal rate is used.", input.date));
            (normal, RateBasis::Normal)
        }
        (None, true) => (normal, RateBasis::Normal),
    };
    let amount = input.amount;
    let total = input.ytd_amount + amount;
    let (threshold, base, tds, why) = match n.threshold {
        Threshold::Every => (ThresholdHit::Every, amount, percent_of(amount, rate), "on every payment".to_owned()),
        Threshold::Annual { single, aggregate } => {
            let over_single = single.is_some_and(|s| amount > s);
            match aggregate {
                Some(a) if input.ytd_amount > a => (
                    ThresholdHit::AggregateAlready,
                    amount,
                    percent_of(amount, rate),
                    format!("the year's total was already over ₹{}", rupees(a)),
                ),
                Some(a) if total > a => {
                    let due = (percent_of(total, rate) - input.ytd_tds).max(Amount::ZERO);
                    let why = if input.ytd_amount.is_zero() {
                        format!("over ₹{} in the year", rupees(a))
                    } else {
                        format!(
                            "the year's total ₹{} crosses ₹{}, so TDS is on the whole total less ₹{} already deducted",
                            rupees(total),
                            rupees(a),
                            rupees(input.ytd_tds)
                        )
                    };
                    (ThresholdHit::AggregateCrossed, total, due, why)
                }
                _ if over_single => (
                    ThresholdHit::Single,
                    amount,
                    percent_of(amount, rate),
                    format!("this payment is over ₹{}", rupees(single.unwrap_or_default())),
                ),
                _ => (ThresholdHit::Below, Amount::ZERO, Amount::ZERO, format!("under the limit ({})", n.threshold_text())),
            }
        }
        Threshold::AnnualExcess { limit } => {
            let before = (input.ytd_amount - limit).max(Amount::ZERO);
            let after = (total - limit).max(Amount::ZERO);
            let base = after - before;
            if base.is_zero() {
                (ThresholdHit::Below, base, base, format!("the year's total ₹{} is not over ₹{}", rupees(total), rupees(limit)))
            } else {
                (ThresholdHit::Excess, base, percent_of(base, rate), format!("₹{} of it is above ₹{} in the year", rupees(base), rupees(limit)))
            }
        }
        Threshold::Monthly { limit } => {
            if amount > limit {
                (ThresholdHit::Monthly, amount, percent_of(amount, rate), format!("over ₹{} for the month", rupees(limit)))
            } else {
                (ThresholdHit::Below, Amount::ZERO, Amount::ZERO, format!("not over ₹{} for the month", rupees(limit)))
            }
        }
        Threshold::PerTransaction { limit } => {
            if amount >= limit {
                (ThresholdHit::Transaction, amount, percent_of(amount, rate), format!("₹{} or more", rupees(limit)))
            } else {
                (ThresholdHit::Below, Amount::ZERO, Amount::ZERO, format!("under ₹{}", rupees(limit)))
            }
        }
    };
    let section = n.section(input.date);
    let rate_note = match rate_basis {
        RateBasis::Normal => String::new(),
        RateBasis::NoPan => format!(" (no PAN: {} or the normal rate if higher)", pct(no_pan_rate)),
        RateBasis::LowerCertificate => format!(
            " (lower-deduction certificate{})",
            input.certificate.as_ref().and_then(|c| c.number.as_deref()).map(|x| format!(" {x}")).unwrap_or_default()
        ),
    };
    let explanation = if threshold == ThresholdHit::Below {
        format!("{} {}: no TDS, {why}.", section, n.name)
    } else {
        format!("{} {}: ₹{} at {}{rate_note} on ₹{}; {why}.", section, n.name, rupees(tds), pct(rate), rupees(base))
    };
    cautions.extend(n.condition.map(str::to_owned));
    cautions.extend(n.caution.map(str::to_owned));
    if rate_basis == RateBasis::NoPan && n.code == "goods" && Act::for_date(input.date) == Act::Ita1961 {
        cautions.push("The 5% no-PAN rate for purchase of goods is the 2025 Act's; for earlier dates it is not re-verified [UNCERTAIN].".into());
    }
    if n.user_rate && rate_basis == RateBasis::NoPan {
        cautions.push(
            "No PAN: 20% (or your rate if higher). The Act lets a non-resident without a PAN avoid this on conditions \
             the rules prescribe (s.397(2)(c)); check them before deducting less."
                .into(),
        );
    }
    Ok(TdsResult { nature: n.code, act: Act::for_date(input.date), section, rate, rate_basis, base, tds, threshold, explanation, cautions })
}

// ------------------------------------------------------------ calendar

/// A quarter of a financial (tax) year; `year` is the year it starts in
/// (2026 for 2026-27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Quarter {
    pub year: i32,
    /// 1 (Apr-Jun) to 4 (Jan-Mar).
    pub q: u32,
}

impl Quarter {
    pub fn of(date: NaiveDate) -> Self {
        let year = if date.month() >= 4 { date.year() } else { date.year() - 1 };
        let q = match date.month() {
            4..=6 => 1,
            7..=9 => 2,
            10..=12 => 3,
            _ => 4,
        };
        Self { year, q }
    }

    /// First and last day; `None` for a quarter number outside 1 to 4.
    pub fn range(self) -> Option<(NaiveDate, NaiveDate)> {
        let (y, m1, m2, last) = match self.q {
            1 => (self.year, 4, 6, 30),
            2 => (self.year, 7, 9, 30),
            3 => (self.year, 10, 12, 31),
            4 => (self.year + 1, 1, 3, 31),
            _ => return None,
        };
        Some((NaiveDate::from_ymd_opt(y, m1, 1)?, NaiveDate::from_ymd_opt(y, m2, last)?))
    }

    /// "2026-27".
    pub fn year_label(self) -> String {
        format!("{}-{:02}", self.year, (self.year + 1).rem_euclid(100))
    }

    /// The quarterly statement's form: Form 140 from tax year 2026-27,
    /// Form 26Q before (research doc §5, "Quarterly statements").
    pub fn form(self) -> &'static str {
        if self.year >= NEW_ACT_FROM.year() { "Form 140" } else { "Form 26Q" }
    }

    /// Statement due date: 31 Jul, 31 Oct, 31 Jan, 31 May under Rule 219(4)
    /// of the Income-tax Rules 2026 (research doc, SECONDARY). `None` for
    /// earlier years, whose due dates the research does not give.
    pub fn statement_due(self) -> Option<NaiveDate> {
        if self.year < NEW_ACT_FROM.year() {
            return None;
        }
        match self.q {
            1 => NaiveDate::from_ymd_opt(self.year, 7, 31),
            2 => NaiveDate::from_ymd_opt(self.year, 10, 31),
            3 => NaiveDate::from_ymd_opt(self.year + 1, 1, 31),
            4 => NaiveDate::from_ymd_opt(self.year + 1, 5, 31),
            _ => None,
        }
    }
}

/// When TDS deducted on `date` must be paid to the government: the 7th of
/// the next month, and 30 April for March. This is the old Rule 30; the
/// research could not verify the new rules' equivalent (**UNCERTAIN**).
pub fn deposit_due(date: NaiveDate) -> NaiveDate {
    let next = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 7)
    } else if date.month() == 3 {
        NaiveDate::from_ymd_opt(date.year(), 4, 30)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 7)
    };
    next.expect("the 7th and 30 April exist")
}

/// The challan-cum-statement's form for a transaction on `date` (Form 141
/// from 1 Apr 2026, Form 26QB before for 194-IA).
pub fn challan_statement_form(date: NaiveDate) -> &'static str {
    if date < NEW_ACT_FROM { "Form 26QB" } else { "Form 141" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn day(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    fn input(nature: &str, amount: Amount) -> TdsInput<'_> {
        TdsInput {
            nature,
            deductee: Some(DeducteeKind::Company),
            pan: true,
            date: day(2026, 6, 15),
            amount,
            ytd_amount: Amount::ZERO,
            ytd_tds: Amount::ZERO,
            certificate: None,
            user_rate: None,
        }
    }

    fn tds(i: &TdsInput<'_>) -> (Amount, ThresholdHit) {
        let r = compute(i).unwrap();
        (r.tds, r.threshold)
    }

    #[test]
    fn natures_are_unique_and_fit_a_ledger() {
        for (i, n) in NATURES.iter().enumerate() {
            assert!(n.code.len() <= khata_core::chart::MAX_TDS_NATURE, "{}", n.code);
            assert!(!NATURES[..i].iter().any(|m| m.code == n.code), "{}", n.code);
            assert_ne!(n.code, ALL_NATURES);
        }
        assert_eq!(nature(" Contract ").unwrap().old_section, "194C");
        assert!(nature("salary").is_none());
    }

    #[test]
    fn contractors_single_and_aggregate_limits() {
        // Individual 1%, others 2%; one payment over 30,000.
        let mut i = input("contract", dec!(30_001));
        i.deductee = Some(DeducteeKind::Individual);
        assert_eq!(tds(&i), (dec!(300), ThresholdHit::Single));
        i.deductee = Some(DeducteeKind::Firm);
        assert_eq!(tds(&i), (dec!(600), ThresholdHit::Single));
        i.amount = dec!(30_000);
        assert_eq!(tds(&i), (dec!(0), ThresholdHit::Below), "exactly the limit is not over it");

        // Crossing 1 lakh mid-year with small bills: TDS on the whole total.
        let mut i = input("contract", dec!(25_000));
        i.ytd_amount = dec!(80_000);
        let r = compute(&i).unwrap();
        assert_eq!((r.tds, r.base, r.threshold), (dec!(2_100), dec!(105_000), ThresholdHit::AggregateCrossed));
        assert!(r.explanation.contains("1,05,000") && r.explanation.contains("crosses"), "{}", r.explanation);
        // Less what an earlier single payment already had deducted.
        i.ytd_tds = dec!(800);
        assert_eq!(tds(&i), (dec!(1_300), ThresholdHit::AggregateCrossed));
        // After the limit, each payment on its own.
        i.ytd_amount = dec!(105_000);
        i.amount = dec!(1_000);
        assert_eq!(tds(&i), (dec!(20), ThresholdHit::AggregateAlready));
        // Unknown deductee: the higher "others" rate, with a caution.
        let mut i = input("contract", dec!(40_000));
        i.deductee = None;
        let r = compute(&i).unwrap();
        assert_eq!(r.tds, dec!(800));
        assert!(r.cautions.iter().any(|c| c.contains("Deductee type not known")));
    }

    #[test]
    fn professional_and_technical_fees() {
        assert_eq!(tds(&input("professional", dec!(50_000))), (dec!(0), ThresholdHit::Below));
        assert_eq!(tds(&input("professional", dec!(60_000))), (dec!(6_000), ThresholdHit::AggregateCrossed));
        let mut i = input("technical", dec!(20_000));
        i.ytd_amount = dec!(40_000);
        assert_eq!(tds(&i), (dec!(1_200), ThresholdHit::AggregateCrossed), "2% of 60,000");
    }

    #[test]
    fn rent_is_per_month() {
        assert_eq!(tds(&input("rent_building", dec!(50_000))), (dec!(0), ThresholdHit::Below));
        let mut i = input("rent_building", dec!(60_000));
        i.ytd_amount = dec!(600_000);
        assert_eq!(tds(&i), (dec!(6_000), ThresholdHit::Monthly), "the year's total does not matter");
        assert_eq!(tds(&input("rent_machinery", dec!(75_000))), (dec!(1_500), ThresholdHit::Monthly));
    }

    #[test]
    fn commission_interest_and_partners() {
        assert_eq!(tds(&input("commission", dec!(20_000))), (dec!(0), ThresholdHit::Below));
        assert_eq!(tds(&input("commission", dec!(20_001))), (dec!(400), ThresholdHit::AggregateCrossed));
        let mut i = input("interest", dec!(4_000));
        i.ytd_amount = dec!(7_000);
        assert_eq!(tds(&i), (dec!(1_100), ThresholdHit::AggregateCrossed));
        assert!(compute(&i).unwrap().cautions.iter().any(|c| c.contains("not a bank")));
        let mut i = input("partner", dec!(25_000));
        i.deductee = Some(DeducteeKind::Individual);
        assert_eq!(tds(&i), (dec!(2_500), ThresholdHit::AggregateCrossed));
    }

    #[test]
    fn property_is_per_transaction() {
        assert_eq!(tds(&input("property", dec!(4_999_999))), (dec!(0), ThresholdHit::Below));
        let r = compute(&input("property", dec!(5_000_000))).unwrap();
        assert_eq!((r.tds, r.threshold), (dec!(50_000), ThresholdHit::Transaction));
        assert!(r.cautions.iter().any(|c| c.contains("challan-cum-statement")));
        assert_eq!(nature("property").unwrap().statement, Statement::ChallanCumStatement);
        assert_eq!((challan_statement_form(day(2026, 3, 1)), challan_statement_form(day(2026, 4, 1))), ("Form 26QB", "Form 141"));
    }

    #[test]
    fn goods_only_above_fifty_lakh() {
        let mut i = input("goods", dec!(1_000_000));
        i.ytd_amount = dec!(4_500_000);
        let r = compute(&i).unwrap();
        assert_eq!((r.base, r.tds, r.threshold), (dec!(500_000), dec!(500), ThresholdHit::Excess));
        assert!(r.cautions.iter().any(|c| c.contains("₹10 crore")));
        i.ytd_amount = dec!(6_000_000);
        assert_eq!(tds(&i), (dec!(1_000), ThresholdHit::Excess), "0.1% of the whole payment");
        i.ytd_amount = dec!(1_000_000);
        assert_eq!(tds(&i), (dec!(0), ThresholdHit::Below));
    }

    #[test]
    fn no_pan_means_twenty_percent_or_more() {
        let mut i = input("contract", dec!(40_000));
        i.pan = false;
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.tds, r.rate_basis), (dec!(20), dec!(8_000), RateBasis::NoPan));
        assert!(r.explanation.contains("no PAN"), "{}", r.explanation);
        // A certificate needs a PAN.
        i.certificate = Some(LowerCertificate { rate: dec!(0.5), number: None, valid_from: None, valid_to: None });
        assert!(compute(&i).unwrap().cautions.iter().any(|c| c.contains("ignored without a PAN")));
        let mut g = input("goods", dec!(100_000));
        g.ytd_amount = dec!(5_000_000);
        g.pan = false;
        let r = compute(&g).unwrap();
        assert_eq!((r.rate, r.tds), (dec!(5), dec!(5_000)), "5% for purchase of goods, s.397(2)(b)(i)(C)");
        assert!(!r.cautions.iter().any(|c| c.contains("not re-verified")));
        g.date = day(2026, 3, 1);
        assert!(compute(&g).unwrap().cautions.iter().any(|c| c.contains("not re-verified")));
    }

    #[test]
    fn lower_certificate_applies_while_valid() {
        let mut i = input("professional", dec!(100_000));
        i.certificate = Some(LowerCertificate {
            rate: dec!(2),
            number: Some("LDC-26-001".into()),
            valid_from: Some(day(2026, 4, 1)),
            valid_to: Some(day(2026, 9, 30)),
        });
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.tds, r.rate_basis), (dec!(2), dec!(2_000), RateBasis::LowerCertificate));
        assert!(r.explanation.contains("LDC-26-001"));
        i.date = day(2026, 10, 1);
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.tds, r.rate_basis), (dec!(10), dec!(10_000), RateBasis::Normal));
        assert!(r.cautions.iter().any(|c| c.contains("not valid")));
        // Nil certificate.
        i.certificate = Some(LowerCertificate { rate: dec!(0), number: None, valid_from: None, valid_to: None });
        assert_eq!(compute(&i).unwrap().tds, dec!(0));
    }

    #[test]
    fn tax_codes_and_tables_follow_the_act() {
        let c = nature("contract").unwrap();
        assert_eq!((c.tax_code(Some(DeducteeKind::Individual)), c.tax_code(Some(DeducteeKind::Huf)), c.tax_code(None)), ("1023", "1023", "1024"));
        let codes: Vec<_> = NATURES.iter().map(|n| (n.code, n.new_section, n.table, n.tax_code_others)).collect();
        assert!(codes.contains(&("partner", "s.393(3)", "Sl. No. 7", "1067")));
        assert!(codes.contains(&("rent_building", "s.393(1)", "Sl. No. 2(ii)", "1009")));
        assert!(codes.contains(&("non_resident", "s.393(2)", "Sl. No. 17", "1057")));
        for n in NATURES {
            let quarterly = n.statement != Statement::ChallanCumStatement;
            assert_eq!(n.tax_code_others.len() == 4, quarterly, "{}", n.code);
            assert!(n.user_rate == (n.code == "non_resident"));
        }
    }

    #[test]
    fn non_resident_rate_is_the_users() {
        let mut i = input("non_resident", dec!(100_000));
        assert_eq!(compute(&i), Err(TdsError::RateRequired("non_resident")));
        i.user_rate = Some(dec!(10.4));
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.tds, r.threshold, r.section.as_str()), (dec!(10.4), dec!(10_400), ThresholdHit::Every, "s.393(2) (ex-195)"));
        assert!(r.cautions.iter().any(|c| c.contains("Form 145") && c.contains("15CA")));
        // Any amount: no limit.
        i.amount = dec!(1);
        assert_eq!(compute(&i).unwrap().threshold, ThresholdHit::Every);
        // No PAN: 20% unless the user's rate is higher.
        i.amount = dec!(100_000);
        i.pan = false;
        let r = compute(&i).unwrap();
        assert_eq!((r.rate, r.tds), (dec!(20), dec!(20_000)));
        assert!(r.cautions.iter().any(|c| c.contains("s.397(2)(c)")));
        i.user_rate = Some(dec!(31.2));
        assert_eq!(compute(&i).unwrap().rate, dec!(31.2));
        i.user_rate = Some(dec!(101));
        assert!(compute(&i).is_err());
    }

    #[test]
    fn act_and_section_follow_the_date() {
        let mut i = input("contract", dec!(40_000));
        i.date = day(2026, 3, 31);
        let r = compute(&i).unwrap();
        assert_eq!((r.act, r.section.as_str()), (Act::Ita1961, "s.194C"));
        i.date = day(2026, 4, 1);
        let r = compute(&i).unwrap();
        assert_eq!((r.act, r.section.as_str()), (Act::Ita2025, "s.393(1) (ex-194C)"));
        i.date = day(2025, 3, 31);
        assert_eq!(compute(&i), Err(TdsError::BeforeRules(day(2025, 3, 31))));
        assert_eq!(compute(&input("salary", dec!(1))), Err(TdsError::UnknownNature("salary".into())));
        assert_eq!(compute(&input("contract", dec!(-1))), Err(TdsError::NegativeAmount));
    }

    #[test]
    fn rounds_to_the_rupee() {
        assert_eq!(round_rupee(dec!(300.50)), dec!(301));
        assert_eq!(round_rupee(dec!(300.49)), dec!(300));
        assert_eq!(tds(&input("contract", dec!(30_025))).0, dec!(601), "600.50 rounds up");
        assert_eq!(rupees(dec!(10_000_000)), "1,00,00,000");
        assert_eq!(rupees(dec!(500)), "500");
        assert_eq!(rupees(dec!(1234.50)), "1,234.5");
    }

    #[test]
    fn quarters_due_dates_and_deposits() {
        let q = Quarter::of(day(2027, 2, 10));
        assert_eq!((q, q.year_label(), q.form()), (Quarter { year: 2026, q: 4 }, "2026-27".to_owned(), "Form 140"));
        assert_eq!(q.range(), Some((day(2027, 1, 1), day(2027, 3, 31))));
        let dues: Vec<_> = (1..=4).map(|q| Quarter { year: 2026, q }.statement_due().unwrap()).collect();
        assert_eq!(dues, [day(2026, 7, 31), day(2026, 10, 31), day(2027, 1, 31), day(2027, 5, 31)]);
        let old = Quarter { year: 2025, q: 4 };
        assert_eq!((old.form(), old.statement_due()), ("Form 26Q", None));
        assert_eq!(Quarter { year: 2026, q: 5 }.range(), None);
        assert_eq!(Quarter::of(day(2026, 9, 30)).q, 2);
        assert_eq!(deposit_due(day(2026, 5, 31)), day(2026, 6, 7));
        assert_eq!(deposit_due(day(2026, 12, 1)), day(2027, 1, 7));
        assert_eq!(deposit_due(day(2027, 3, 31)), day(2027, 4, 30));
    }
}

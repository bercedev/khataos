//! Salary TDS basics: an **estimate** of an employee's tax for the year
//! under the new tax regime, and the monthly deduction that spreads what is
//! left over the remaining months. A calculator, not payroll.
//!
//! Source: `docs/research/india-tds-tcs.md` §3, all from the gazetted
//! Income-tax Act 2025 and Finance Act 2026 [PRIMARY]:
//!
//! - s.392(1): the employer deducts at the time of payment at the average
//!   rate of income-tax on the estimated salary income for the tax year.
//! - s.202(1): the new tax regime is the default for an individual; its
//!   slabs are Nil up to ₹4,00,000, then 5%, 10%, 15%, 20%, 25% in steps of
//!   ₹4,00,000, and 30% above ₹24,00,000. The Finance Act 2026 did not
//!   change them (its only change to s.202 is in sub-section (2)).
//! - s.19(1) Table Sl. No. 2: standard deduction ₹75,000 (or the salary, if
//!   less) when tax is computed under s.202(1).
//! - s.156(2): a resident individual under s.202(1) gets a rebate of the
//!   tax up to ₹60,000 when total income does not exceed ₹12,00,000, and
//!   above that the tax is limited to the income over ₹12,00,000 (marginal
//!   relief).
//! - Finance Act 2026 s.3(10) and (16): salary TDS is at the Part III rates
//!   plus the 4% Health and Education Cess.
//! - s.516: total income and tax are rounded to the nearest ₹10.
//!
//! Not modelled: surcharge (total income over ₹50 lakh: the estimate says
//! so and is too low), the old regime (an employee who opts out under
//! s.202), perquisites, exempt allowances, relief for arrears, and other
//! heads beyond one "other income" figure the employee declares
//! (s.392(4)). Spreading the remaining tax evenly over the remaining months
//! is the usual practice, not a rule in the Act.

use std::fmt;

use khata_core::Amount;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::tds::round_rupee;

const fn d(units: u32, scale: u32) -> Decimal {
    Decimal::from_parts(units, 0, 0, false, scale)
}

/// The only tax year whose rates are in the research.
pub const TAX_YEAR: i32 = 2026;

pub const STANDARD_DEDUCTION: Decimal = d(75_000, 0);
pub const REBATE_LIMIT: Decimal = d(1_200_000, 0);
pub const REBATE_MAX: Decimal = d(60_000, 0);
pub const CESS_RATE: Decimal = d(4, 0);
/// Above this total income a surcharge applies, which is not modelled.
pub const SURCHARGE_FROM: Decimal = d(5_000_000, 0);

/// s.202(1) Table: (upper limit of the slab, rate %); `None` is the top.
pub const NEW_REGIME_SLABS: &[(Option<Decimal>, Decimal)] = &[
    (Some(d(400_000, 0)), d(0, 0)),
    (Some(d(800_000, 0)), d(5, 0)),
    (Some(d(1_200_000, 0)), d(10, 0)),
    (Some(d(1_600_000, 0)), d(15, 0)),
    (Some(d(2_000_000, 0)), d(20, 0)),
    (Some(d(2_400_000, 0)), d(25, 0)),
    (None, d(30, 0)),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalaryInput {
    /// The tax year by its first calendar year (2026 for 2026-27).
    pub tax_year: i32,
    /// Salary from this employer for the whole year, estimated.
    pub salary: Amount,
    /// Salary from earlier employers in the year, as the employee declared.
    pub other_salary: Amount,
    /// Income under other heads the employee declared (s.392(4)(a)(iv)).
    pub other_income: Amount,
    /// Tax already deducted this year by this and earlier employers.
    pub tds_so_far: Amount,
    /// Months of salary left in the year, this month included (1 to 12).
    pub months_remaining: u32,
    /// Resident in India (the rebate is for residents only).
    pub resident: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlabLine {
    pub from: Amount,
    pub to: Option<Amount>,
    /// Percent.
    pub rate: Decimal,
    /// The part of the income in this slab.
    pub income: Amount,
    pub tax: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalaryEstimate {
    pub regime: &'static str,
    pub gross: Amount,
    pub standard_deduction: Amount,
    /// Rounded to ₹10.
    pub total_income: Amount,
    pub slabs: Vec<SlabLine>,
    pub tax_on_income: Amount,
    pub rebate: Amount,
    pub cess: Amount,
    /// Rounded to ₹10.
    pub annual_tax: Amount,
    pub tds_so_far: Amount,
    /// Still to deduct this year (not below zero).
    pub remaining: Amount,
    pub months_remaining: u32,
    /// To deduct each remaining month, rounded to the rupee.
    pub monthly: Amount,
    /// Total income is over ₹50 lakh: surcharge is left out, so the tax is too low.
    pub surcharge_left_out: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalaryError {
    TaxYear(i32),
    Months(u32),
    NegativeAmount,
}

impl fmt::Display for SalaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaxYear(y) => write!(f, "salary rates for tax year {y}-{:02} are not in KhataOS (only 2026-27)", (y + 1) % 100),
            Self::Months(m) => write!(f, "months remaining must be 1 to 12, not {m}"),
            Self::NegativeAmount => write!(f, "salary figures cannot be negative"),
        }
    }
}

impl std::error::Error for SalaryError {}

/// Round to the nearest ₹10, five and above up (s.516), paise ignored.
pub fn round_ten(a: Amount) -> Amount {
    let rupees = a.trunc();
    let ten = Decimal::TEN;
    let rem = rupees % ten;
    if rem >= d(5, 0) { rupees - rem + ten } else { rupees - rem }
}

/// Tax on `income` by the s.202(1) slabs.
pub fn slab_tax(income: Amount) -> (Amount, Vec<SlabLine>) {
    let mut lines = Vec::new();
    let mut from = Amount::ZERO;
    let mut total = Amount::ZERO;
    for &(upper, rate) in NEW_REGIME_SLABS {
        let top = upper.map_or(income, |u| income.min(u));
        let part = (top - from).max(Amount::ZERO);
        let tax = part * rate / Decimal::ONE_HUNDRED;
        lines.push(SlabLine { from, to: upper, rate, income: part, tax });
        total += tax;
        match upper {
            Some(u) if income > u => from = u,
            _ => break,
        }
    }
    (total, lines)
}

/// The estimate for one employee.
pub fn estimate(input: &SalaryInput) -> Result<SalaryEstimate, SalaryError> {
    if input.tax_year != TAX_YEAR {
        return Err(SalaryError::TaxYear(input.tax_year));
    }
    if !(1..=12).contains(&input.months_remaining) {
        return Err(SalaryError::Months(input.months_remaining));
    }
    if [input.salary, input.other_salary, input.other_income, input.tds_so_far].iter().any(|a| a.is_sign_negative()) {
        return Err(SalaryError::NegativeAmount);
    }
    let salaries = input.salary + input.other_salary;
    let gross = salaries + input.other_income;
    let standard_deduction = STANDARD_DEDUCTION.min(salaries);
    let total_income = round_ten(gross - standard_deduction);
    let (tax_on_income, slabs) = slab_tax(total_income);
    let rebate = if !input.resident {
        Amount::ZERO
    } else if total_income <= REBATE_LIMIT {
        tax_on_income.min(REBATE_MAX)
    } else {
        (tax_on_income - (total_income - REBATE_LIMIT)).max(Amount::ZERO)
    };
    let after_rebate = tax_on_income - rebate;
    let cess = after_rebate * CESS_RATE / Decimal::ONE_HUNDRED;
    let annual_tax = round_ten(after_rebate + cess);
    let remaining = (annual_tax - input.tds_so_far).max(Amount::ZERO);
    let monthly = round_rupee(remaining / Decimal::from(input.months_remaining));
    let surcharge_left_out = total_income > SURCHARGE_FROM;
    let mut notes = vec![
        "An estimate: the employer deducts at the average rate on the estimated salary for the year (s.392(1)); \
         revise it when pay, declarations or other income change."
            .to_owned(),
        "New tax regime (s.202(1)), the default. An employee who opts out of it needs the old regime's \
         computation, which KhataOS does not do."
            .to_owned(),
    ];
    if surcharge_left_out {
        notes.push("Total income is over ₹50 lakh: surcharge (and its marginal relief) is not included, so this tax is too low.".into());
    }
    if !input.resident {
        notes.push("Not resident: no s.156 rebate.".into());
    } else if rebate > Amount::ZERO && total_income > REBATE_LIMIT {
        notes.push("Marginal relief: the tax before cess is limited to the income above ₹12 lakh (s.156(2)(b)).".into());
    }
    if input.tds_so_far > annual_tax {
        notes.push("More has been deducted than the year's estimate: deduct nothing more and review (s.392(5)(c) lets the employer adjust).".into());
    }
    Ok(SalaryEstimate {
        regime: "New tax regime, s.202(1) of the Income-tax Act 2025",
        gross,
        standard_deduction,
        total_income,
        slabs,
        tax_on_income,
        rebate,
        cess,
        annual_tax,
        tds_so_far: input.tds_so_far,
        remaining,
        months_remaining: input.months_remaining,
        monthly,
        surcharge_left_out,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn input(salary: Amount) -> SalaryInput {
        SalaryInput {
            tax_year: 2026,
            salary,
            other_salary: Amount::ZERO,
            other_income: Amount::ZERO,
            tds_so_far: Amount::ZERO,
            months_remaining: 12,
            resident: true,
        }
    }

    #[test]
    fn slabs_match_section_202() {
        assert_eq!(slab_tax(dec!(400_000)).0, dec!(0));
        assert_eq!(slab_tax(dec!(800_000)).0, dec!(20_000));
        assert_eq!(slab_tax(dec!(1_200_000)).0, dec!(60_000));
        assert_eq!(slab_tax(dec!(1_600_000)).0, dec!(120_000));
        assert_eq!(slab_tax(dec!(2_000_000)).0, dec!(200_000));
        assert_eq!(slab_tax(dec!(2_400_000)).0, dec!(300_000));
        assert_eq!(slab_tax(dec!(3_000_000)).0, dec!(480_000));
        let (_, lines) = slab_tax(dec!(900_000));
        assert_eq!(lines.len(), 3);
        assert_eq!((lines[2].from, lines[2].income, lines[2].tax), (dec!(800_000), dec!(100_000), dec!(10_000)));
    }

    #[test]
    fn rebate_makes_twelve_lakh_tax_free() {
        // ₹12,75,000 salary - ₹75,000 = ₹12 lakh: tax 60,000, rebate 60,000.
        let e = estimate(&input(dec!(1_275_000))).unwrap();
        assert_eq!((e.total_income, e.tax_on_income, e.rebate, e.annual_tax, e.monthly), (dec!(1_200_000), dec!(60_000), dec!(60_000), dec!(0), dec!(0)));
        // A non-resident gets no rebate: 60,000 + 4% cess.
        let mut nr = input(dec!(1_275_000));
        nr.resident = false;
        assert_eq!(estimate(&nr).unwrap().annual_tax, dec!(62_400));
    }

    #[test]
    fn marginal_relief_just_above_twelve_lakh() {
        // Income ₹12,10,000: slab tax 61,500, but tax limited to the 10,000 above 12 lakh.
        let e = estimate(&input(dec!(1_285_000))).unwrap();
        assert_eq!((e.total_income, e.tax_on_income, e.rebate), (dec!(1_210_000), dec!(61_500), dec!(51_500)));
        assert_eq!((e.cess, e.annual_tax), (dec!(400), dec!(10_400)));
        assert!(e.notes.iter().any(|n| n.contains("Marginal relief")));
        // Far above: no relief.
        let e = estimate(&input(dec!(1_675_000))).unwrap();
        assert_eq!((e.total_income, e.tax_on_income, e.rebate, e.annual_tax), (dec!(1_600_000), dec!(120_000), dec!(0), dec!(124_800)));
    }

    #[test]
    fn monthly_spreads_what_is_left() {
        // ₹20,75,000: income 20 lakh, tax 2,00,000 + 8,000 cess = 2,08,000.
        let mut i = input(dec!(2_075_000));
        i.months_remaining = 7;
        i.tds_so_far = dec!(80_000);
        let e = estimate(&i).unwrap();
        assert_eq!((e.annual_tax, e.remaining, e.monthly), (dec!(208_000), dec!(128_000), dec!(18_286)), "1,28,000 / 7 = 18,285.71");
        // Other salary and other income count; standard deduction is capped at salary.
        let mut i = input(dec!(50_000));
        i.other_income = dec!(1_000_000);
        let e = estimate(&i).unwrap();
        assert_eq!((e.standard_deduction, e.total_income), (dec!(50_000), dec!(1_000_000)));
        // Over-deducted: nothing more.
        let mut i = input(dec!(1_675_000));
        i.tds_so_far = dec!(200_000);
        let e = estimate(&i).unwrap();
        assert_eq!((e.remaining, e.monthly), (dec!(0), dec!(0)));
        assert!(e.notes.iter().any(|n| n.contains("More has been deducted")));
    }

    #[test]
    fn rounding_surcharge_and_errors() {
        assert_eq!((round_ten(dec!(1234.99)), round_ten(dec!(1235)), round_ten(dec!(1239.50))), (dec!(1230), dec!(1240), dec!(1240)));
        let e = estimate(&input(dec!(6_000_000))).unwrap();
        assert!(e.surcharge_left_out && e.notes.iter().any(|n| n.contains("surcharge")));
        assert!(!estimate(&input(dec!(5_075_000))).unwrap().surcharge_left_out, "₹50 lakh exactly is not over it");
        let mut i = input(dec!(1));
        i.tax_year = 2027;
        assert_eq!(estimate(&i), Err(SalaryError::TaxYear(2027)));
        i.tax_year = 2026;
        i.months_remaining = 0;
        assert_eq!(estimate(&i), Err(SalaryError::Months(0)));
        i.months_remaining = 1;
        i.salary = dec!(-1);
        assert_eq!(estimate(&i), Err(SalaryError::NegativeAmount));
    }
}

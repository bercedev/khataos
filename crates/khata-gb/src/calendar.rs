//! UK statutory due dates for the compliance calendar:
//! - the VAT return and payment under Making Tax Digital, "one calendar
//!   month and 7 days after the end of an accounting period", by HMRC
//!   stagger (1: quarters to March, 2: to April, 3: to May) or monthly;
//! - companies and LLPs: Companies House accounts 9 months after the year
//!   end (the first accounts of a period over 12 months: 21 months after
//!   incorporation, or 3 months after the period if later) and the
//!   confirmation statement 14 days after each anniversary of
//!   incorporation;
//! - companies: corporation tax payment 9 months and 1 day after the
//!   accounting period, the CT600 return 12 months after it;
//! - sole traders and partnerships: the Self Assessment return and payment
//!   by 31 January after the tax year (to 5 April), and the second payment
//!   on account by 31 July.
//!
//! HMRC's own dates come from the MTD obligations (Making Tax Digital → VAT
//! periods) and win over these estimates. HMRC and Companies House do not
//! move a deadline that falls on a weekend ([`Shift::Fixed`]). The Annual
//! Accounting Scheme is not covered. Research and sources:
//! `docs/research/accountant-desk.md`.

use chrono::{Datelike, Months, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift};

const GOV_UK_VAT: &str = "https://www.gov.uk/submit-vat-return";
const GOV_UK_ACCOUNTS: &str = "https://www.gov.uk/file-your-company-accounts-and-tax-return";
const GOV_UK_CS: &str = "https://www.gov.uk/file-your-confirmation-statement-with-companies-house";
const GOV_UK_CT_PAY: &str = "https://www.gov.uk/pay-corporation-tax";
const GOV_UK_CT600: &str = "https://www.gov.uk/company-tax-returns";
const GOV_UK_SA: &str = "https://www.gov.uk/self-assessment-tax-returns/deadlines";

const FIXED: Shift = Shift::Fixed;

pub const VAT_RETURN: Rule = Rule::new("GB.VAT", "VAT Return (MTD) and payment", "One calendar month and 7 days after the end of the VAT period", GOV_UK_VAT, Category::Return).shift(FIXED);
pub const ACCOUNTS: Rule = Rule::new("GB.ACCOUNTS", "Annual accounts to Companies House", "9 months after the accounting reference date (Companies Act 2006 s.442)", GOV_UK_ACCOUNTS, Category::Filing).shift(FIXED);
pub const FIRST_ACCOUNTS: Rule = Rule::new("GB.ACCOUNTS", "Annual accounts to Companies House", "First accounts of a period over 12 months: 21 months after incorporation, or 3 months after the period if later (s.442(3))", GOV_UK_ACCOUNTS, Category::Filing).shift(FIXED);
pub const CONFIRMATION: Rule = Rule::new("GB.CS01", "Confirmation statement", "Within 14 days after the end of each 12-month review period from incorporation", GOV_UK_CS, Category::Filing).shift(FIXED);
pub const CT_PAYMENT: Rule = Rule::new("GB.CTPAY", "Corporation tax payment", "9 months and 1 day after the end of the accounting period (companies below the quarterly instalment limit)", GOV_UK_CT_PAY, Category::Payment).shift(FIXED);
pub const CT600: Rule = Rule::new("GB.CT600", "Company Tax Return (CT600)", "12 months after the end of the accounting period", GOV_UK_CT600, Category::Return).shift(FIXED);
pub const SELF_ASSESSMENT: Rule = Rule::new("GB.SA", "Self Assessment return and payment", "31 January after the tax year ending 5 April (online)", GOV_UK_SA, Category::Return).shift(FIXED);
pub const PAYMENT_ON_ACCOUNT: Rule = Rule::new("GB.SAPOA", "Second payment on account (Self Assessment)", "31 July after the tax year ending 5 April", GOV_UK_SA, Category::Payment).shift(FIXED).optional();

/// Quarters ending March, June, September and December (stagger 1); the
/// stagger HMRC gives each business is set in the compliance profile.
pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Quarterly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::Entity, ProfileField::FyEnd, ProfileField::VatFrom, ProfileField::TaxPeriod],
        entities: &[Entity::Company, Entity::Llp, Entity::Partnership, Entity::SoleProprietor],
        period: PeriodChoice::UkStagger,
    }
}

/// The due date for a VAT period ending on `end` (a month end): the end of
/// the next month plus 7 days, as HMRC's obligations show it (30 June →
/// 7 August, not 6 August).
pub fn vat_due(end: NaiveDate) -> NaiveDate {
    calendar::days_after(calendar::end_of_next_month(end), 7)
}

/// `n` months after `end`, keeping month ends at month ends (30 June + 9 → 31 March).
fn months_after(end: NaiveDate, n: u32) -> NaiveDate {
    if end.succ_opt().is_some_and(|x| x.day() == 1) { calendar::end_of_month_after(end, n) } else { calendar::add_months(end, n) }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    let mut out: Vec<DueDate> = calendar::vat_periods(c, &defaults(), start, to).into_iter().map(|p| DueDate::new(VAT_RETURN, p, vat_due(p.1))).collect();
    let entity = c.entity_or(Entity::Company);
    if matches!(entity, Entity::Company | Entity::Llp) {
        // The first accounting reference date is the end of the month of the first anniversary.
        for p in calendar::company_years(c, 6, start, to) {
            let first_long = calendar::is_first_year(c, p) && calendar::add_months(p.0, 12) <= p.1;
            if first_long {
                let due = calendar::add_months(p.0, 21).max(months_after(p.1, 3));
                out.push(DueDate::new(FIRST_ACCOUNTS, p, due));
            } else {
                out.push(DueDate::new(ACCOUNTS, p, months_after(p.1, 9)));
            }
            if entity == Entity::Company {
                out.push(DueDate::new(CT_PAYMENT, p, calendar::days_after(months_after(p.1, 9), 1)));
                out.push(DueDate::new(CT600, p, months_after(p.1, 12)));
            }
        }
        if let Some(inc) = c.incorporated {
            for a in calendar::anniversaries(inc, start, to) {
                let review = (a.checked_sub_months(Months::new(12)).expect("dates in range"), a);
                out.push(DueDate::new(CONFIRMATION, review, calendar::days_after(a, 14)));
            }
        }
    } else {
        // Tax years 6 April to 5 April.
        for y in start.year() - 1..=to.year() {
            let ty = (NaiveDate::from_ymd_opt(y - 1, 4, 6).expect("6 April"), NaiveDate::from_ymd_opt(y, 4, 5).expect("5 April"));
            if c.incorporated.is_some_and(|i| ty.1 < i) {
                continue;
            }
            out.push(DueDate::new(SELF_ASSESSMENT, ty, NaiveDate::from_ymd_opt(y + 1, 1, 31).expect("31 January")));
            out.push(DueDate::new(PAYMENT_ON_ACCOUNT, ty, NaiveDate::from_ymd_opt(y, 7, 31).expect("31 July")));
        }
    }
    calendar::within(out, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn on(v: &[DueDate], code: &str) -> Vec<NaiveDate> {
        v.iter().filter(|x| x.rule.code == code).map(|x| x.due).collect()
    }

    #[test]
    fn one_month_and_seven_days_as_hmrc_dates_it() {
        // HMRC sandbox obligations: 2017-03-31 → 2017-05-07, 2017-06-30 → 2017-08-07, 2017-09-30 → 2017-11-07.
        assert_eq!(vat_due(d(2017, 3, 31)), d(2017, 5, 7));
        assert_eq!(vat_due(d(2017, 6, 30)), d(2017, 8, 7));
        assert_eq!(vat_due(d(2017, 9, 30)), d(2017, 11, 7));
        assert_eq!(vat_due(d(2026, 12, 31)), d(2027, 2, 7));
        assert_eq!(vat_due(d(2027, 1, 31)), d(2027, 3, 7));
        // 7 November 2026 is a Saturday: HMRC keeps it.
        let v = due_dates(&CalendarCompany { fy_start_month: 4, ..Default::default() }, d(2026, 11, 1), d(2026, 11, 30));
        assert_eq!(on(&v, "GB.VAT"), [d(2026, 11, 7)]);
    }

    #[test]
    fn staggers_and_monthly_returns() {
        let c = CalendarCompany { fy_start_month: 4, ..Default::default() };
        let v = due_dates(&c, d(2026, 5, 1), d(2026, 5, 31));
        let q = v.iter().find(|x| x.rule.code == "GB.VAT").unwrap();
        assert_eq!((q.period_from, q.period_to, q.due), (d(2026, 1, 1), d(2026, 3, 31), d(2026, 5, 7)));
        // Stagger 2: quarters end April, July, October, January.
        let s = CalendarCompany { quarter_end: Some(4), ..c.clone() };
        assert_eq!(due_dates(&s, d(2026, 6, 1), d(2026, 6, 30)).iter().find(|x| x.rule.code == "GB.VAT").unwrap().period_to, d(2026, 4, 30));
        let m = CalendarCompany { frequency: Some(Frequency::Monthly), ..c };
        assert_eq!(on(&due_dates(&m, d(2026, 6, 1), d(2026, 6, 30)), "GB.VAT").len(), 1);
        assert!(VAT_RETURN.source.starts_with("https://www.gov.uk/"));
    }

    #[test]
    fn company_dates_in_a_normal_year() {
        // A March year end, incorporated 15 June 2015.
        let c = CalendarCompany { fy_start_month: 4, incorporated: Some(d(2015, 6, 15)), ..Default::default() };
        let v = due_dates(&c, d(2026, 4, 1), d(2027, 3, 31));
        assert_eq!(on(&v, "GB.ACCOUNTS"), [d(2026, 12, 31)]);
        assert_eq!(on(&v, "GB.CTPAY"), [d(2027, 1, 1)]);
        assert_eq!(on(&v, "GB.CS01"), [d(2026, 6, 29)]);
        let ct600 = due_dates(&c, d(2027, 3, 1), d(2027, 3, 31));
        assert_eq!(on(&ct600, "GB.CT600"), [d(2027, 3, 31)]);
        // A June year end: 9 months is 31 March, so tax is due on 1 April.
        let june = CalendarCompany { fy_start_month: 7, ..c };
        assert_eq!(on(&due_dates(&june, d(2027, 4, 1), d(2027, 4, 1)), "GB.CTPAY"), [d(2027, 4, 1)]);
    }

    #[test]
    fn first_accounts_21_months_after_incorporation() {
        // Incorporated 10 October 2025; the first year end is 31 October 2026 (the anniversary month).
        let c = CalendarCompany { fy_start_month: 11, incorporated: Some(d(2025, 10, 10)), ..Default::default() };
        let v = due_dates(&c, d(2025, 10, 1), d(2027, 12, 31));
        assert_eq!(on(&v, "GB.ACCOUNTS"), [d(2027, 7, 10)], "21 months after incorporation");
        assert_eq!(on(&v, "GB.CS01"), [d(2026, 10, 24), d(2027, 10, 24)]);
        // An LLP files accounts and statements but no corporation tax.
        let llp = CalendarCompany { entity: Some(Entity::Llp), ..c };
        let v = due_dates(&llp, d(2025, 10, 1), d(2027, 12, 31));
        assert!(on(&v, "GB.CTPAY").is_empty() && !on(&v, "GB.ACCOUNTS").is_empty());
    }

    #[test]
    fn sole_traders_self_assessment() {
        let c = CalendarCompany { fy_start_month: 4, entity: Some(Entity::SoleProprietor), vat_unregistered: true, ..Default::default() };
        let v = due_dates(&c, d(2027, 1, 1), d(2027, 7, 31));
        let sa = v.iter().find(|x| x.rule.code == "GB.SA").unwrap();
        assert_eq!((sa.period_from, sa.period_to, sa.due), (d(2025, 4, 6), d(2026, 4, 5), d(2027, 1, 31)));
        assert_eq!(on(&v, "GB.SAPOA"), [d(2027, 7, 31)]);
        assert!(on(&v, "GB.ACCOUNTS").is_empty() && on(&v, "GB.VAT").is_empty());
    }
}

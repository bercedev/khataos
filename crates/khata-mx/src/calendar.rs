//! Mexican statutory due dates for the compliance calendar:
//! - the monthly provisional ISR and definitive IVA payments, by the 17th
//!   of the next month (LISR art. 14, LIVA art. 5-D);
//! - the DIOT (informative return of operations with third parties), by
//!   the last day of the next month (LIVA art. 32 VIII);
//! - the annual return: personas morales by 31 March (LISR art. 76 V),
//!   personas físicas by 30 April (LISR art. 150).
//!
//! A deadline on a Saturday, Sunday or non-working day moves to the next
//! business day (CFF art. 12); weekends are applied here. Research:
//! `docs/research/accountant-desk.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift, Weekend};

const LISR: &str = "https://www.diputados.gob.mx/LeyesBiblio/pdf/LISR.pdf";
const LIVA: &str = "https://www.diputados.gob.mx/LeyesBiblio/pdf/LIVA.pdf";
const NEXT: Shift = Shift::NextWorkingDay(Weekend::SatSun);

pub const MONTHLY: Rule = Rule::new("MX.MENSUAL", "Monthly ISR and IVA payments", "By the 17th of the next month (LISR art. 14, LIVA art. 5-D)", LIVA, Category::Return).shift(NEXT);
pub const DIOT: Rule = Rule::new("MX.DIOT", "DIOT (operations with third parties)", "By the last day of the next month (LIVA art. 32 VIII)", LIVA, Category::Return).shift(NEXT);
pub const ANNUAL_MORAL: Rule = Rule::new("MX.ANUAL", "Annual return (persona moral)", "By 31 March of the following year (LISR art. 76 V)", LISR, Category::Return).shift(NEXT);
pub const ANNUAL_FISICA: Rule = Rule::new("MX.ANUAL", "Annual return (persona física)", "By 30 April of the following year (LISR art. 150)", LISR, Category::Return).shift(NEXT);

pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Monthly,
        quarter_end: 3,
        quarterly_allowed: false,
        fields: &[ProfileField::Incorporated, ProfileField::Entity],
        entities: &[Entity::Company, Entity::SoleProprietor],
        period: PeriodChoice::MonthlyQuarterly,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    // The tax year is the calendar year (CFF art. 11); payments are monthly.
    let m = CalendarCompany { frequency: Some(Frequency::Monthly), fy_start_month: 1, ..c.clone() };
    let mut out = Vec::new();
    for p in calendar::vat_periods(&m, &defaults(), start, to) {
        out.push(DueDate::new(MONTHLY, p, calendar::day_of_next_month(p.1, 17)));
        out.push(DueDate::new(DIOT, p, calendar::end_of_next_month(p.1)));
    }
    let fisica = c.entity_or(Entity::Company) == Entity::SoleProprietor;
    for y in calendar::company_years(&m, 0, start, to) {
        let (rule, month, day) = if fisica { (ANNUAL_FISICA, 4, 30) } else { (ANNUAL_MORAL, 3, 31) };
        out.push(DueDate::new(rule, y, NaiveDate::from_ymd_opt(y.1.year() + 1, month, day).expect("valid date")));
    }
    calendar::within(out, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn monthly_payments_diot_and_annual_returns() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 10, 1), d(2026, 10, 31));
        // 17 October 2026 is a Saturday: Monday 19 October. 31 October is a Saturday too: Monday 2 November.
        let pay = v.iter().find(|x| x.rule.code == "MX.MENSUAL").unwrap();
        assert_eq!((pay.period_to, pay.unadjusted, pay.due), (d(2026, 9, 30), d(2026, 10, 17), d(2026, 10, 19)));
        assert!(due_dates(&c, d(2026, 11, 2), d(2026, 11, 2)).iter().any(|x| x.rule.code == "MX.DIOT" && x.unadjusted == d(2026, 10, 31)));
        let a = due_dates(&c, d(2027, 3, 1), d(2027, 3, 31)).into_iter().find(|x| x.rule.code == "MX.ANUAL").unwrap();
        assert_eq!((a.period_to, a.due), (d(2026, 12, 31), d(2027, 3, 31)));
        let f = CalendarCompany { entity: Some(Entity::SoleProprietor), ..c };
        let a = due_dates(&f, d(2027, 4, 1), d(2027, 4, 30)).into_iter().find(|x| x.rule.code == "MX.ANUAL").unwrap();
        assert_eq!(a.due, d(2027, 4, 30));
    }

    #[test]
    fn a_first_year_from_incorporation() {
        let c = CalendarCompany { fy_start_month: 1, incorporated: Some(d(2026, 6, 10)), ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2027, 3, 31));
        let first = v.iter().find(|x| x.rule.code == "MX.MENSUAL").unwrap();
        assert_eq!((first.period_from, first.due), (d(2026, 6, 10), d(2026, 7, 17)));
        let a = v.iter().find(|x| x.rule.code == "MX.ANUAL").unwrap();
        assert_eq!(a.period_from, d(2026, 6, 10));
    }
}

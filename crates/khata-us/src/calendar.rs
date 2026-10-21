//! US statutory due dates for the compliance calendar (estimates):
//! - state sales tax returns, per state at the frequency the state
//!   assigned (most are due on the 20th of the month after the period;
//!   each state sets its own day);
//! - Form 1099-NEC to recipients and the IRS by 31 January;
//! - the federal income tax return by entity: Form 1120 (C corporation)
//!   on the 15th day of the 4th month after the year end (the 3rd month
//!   for a June year end, for years beginning before 2026); Forms 1120-S
//!   and 1065 on the 15th day of the 3rd month; a sole proprietor's
//!   Form 1040 (Schedule C) by 15 April.
//!
//! A federal deadline on a Saturday, Sunday or legal holiday moves to the
//! next business day (IRC §7503); weekends are applied here, holidays are
//! not. State rules vary ([`Shift::MayMove`]). Research:
//! `docs/research/accountant-desk.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift, Weekend};

const IRS_P509: &str = "https://www.irs.gov/publications/p509";
const IRS_1099NEC: &str = "https://www.irs.gov/forms-pubs/about-form-1099-nec";
const NEXT: Shift = Shift::NextWorkingDay(Weekend::SatSun);

pub const SALES_TAX: Rule = Rule::new("US.SALESTAX", "State sales tax return", "Usually the 20th of the month after the period; each state sets its own day", IRS_P509, Category::Return);
pub const F1099NEC: Rule = Rule::new("US.1099NEC", "Form 1099-NEC", "31 January after the calendar year, to recipients and the IRS", IRS_1099NEC, Category::Return).shift(NEXT).optional();
pub const F1120: Rule = Rule::new("US.1120", "Form 1120 (corporation income tax)", "15th day of the 4th month after the year end (3rd month for a June year end before 2026)", IRS_P509, Category::Return).shift(NEXT);
pub const F1120S: Rule = Rule::new("US.1120S", "Form 1120-S (S corporation)", "15th day of the 3rd month after the year end", IRS_P509, Category::Return).shift(NEXT);
pub const F1065: Rule = Rule::new("US.1065", "Form 1065 (partnership)", "15th day of the 3rd month after the year end", IRS_P509, Category::Return).shift(NEXT);
pub const F1040: Rule = Rule::new("US.1040", "Form 1040 with Schedule C", "15 April after the calendar year", IRS_P509, Category::Return).shift(NEXT);

pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Quarterly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::Entity, ProfileField::FyEnd, ProfileField::States],
        entities: &[Entity::Company, Entity::SCorporation, Entity::Llp, Entity::Partnership, Entity::SoleProprietor],
        period: PeriodChoice::MonthlyQuarterlyAnnual,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    let mut out = Vec::new();
    for s in &c.states {
        // State periods are calendar months, quarters or years.
        let st = CalendarCompany { frequency: Some(s.frequency), quarter_end: Some(if s.frequency == Frequency::Annual { 12 } else { 3 }), fy_start_month: 1, vat_from: None, ..c.clone() };
        for p in calendar::vat_periods(&st, &defaults(), start, to) {
            out.push(DueDate::new(SALES_TAX, p, calendar::day_of_next_month(p.1, 20)).for_subject(&s.state));
        }
    }
    let alive = |p: &(NaiveDate, NaiveDate)| c.incorporated.is_none_or(|i| p.1 >= i);
    for y in calendar::periods(Frequency::Annual, 12, start, to).into_iter().filter(alive) {
        out.push(DueDate::new(F1099NEC, y, NaiveDate::from_ymd_opt(y.1.year() + 1, 1, 31).expect("31 January")));
    }
    let entity = c.entity_or(Entity::Company);
    if entity == Entity::SoleProprietor {
        for y in calendar::periods(Frequency::Annual, 12, start, to).into_iter().filter(alive) {
            out.push(DueDate::new(F1040, y, NaiveDate::from_ymd_opt(y.1.year() + 1, 4, 15).expect("15 April")));
        }
    } else {
        // A short first year is normal: it runs from incorporation to the first year end.
        for y in calendar::company_years(c, 0, start, to) {
            let june_exception = y.1.month() == 6 && y.0 < NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid");
            let (rule, months) = match entity {
                Entity::SCorporation => (F1120S, 3),
                Entity::Partnership | Entity::Llp => (F1065, 3),
                _ if june_exception => (F1120, 3),
                _ => (F1120, 4),
            };
            out.push(DueDate::new(rule, y, calendar::day_of_month_after(y.1, months, 15)));
        }
    }
    calendar::within(out, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use khata_compliance::StateFiling;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn on(v: &[DueDate], code: &str) -> Vec<NaiveDate> {
        v.iter().filter(|x| x.rule.code == code).map(|x| x.due).collect()
    }

    #[test]
    fn federal_returns_by_entity() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2027, 1, 1), d(2027, 12, 31));
        assert_eq!(on(&v, "US.1120"), [d(2027, 4, 15)]);
        // 31 January 2027 is a Sunday: the 1099-NEC moves to Monday 1 February.
        let nec = v.iter().find(|x| x.rule.code == "US.1099NEC").unwrap();
        assert_eq!((nec.unadjusted, nec.due), (d(2027, 1, 31), d(2027, 2, 1)));
        let s = CalendarCompany { entity: Some(Entity::SCorporation), ..c.clone() };
        // 15 March 2027 is a Monday.
        assert_eq!(on(&due_dates(&s, d(2027, 1, 1), d(2027, 12, 31)), "US.1120S"), [d(2027, 3, 15)]);
        let p = CalendarCompany { entity: Some(Entity::Partnership), ..c.clone() };
        assert_eq!(on(&due_dates(&p, d(2027, 1, 1), d(2027, 12, 31)), "US.1065"), [d(2027, 3, 15)]);
        let sole = CalendarCompany { entity: Some(Entity::SoleProprietor), ..c };
        assert_eq!(on(&due_dates(&sole, d(2027, 1, 1), d(2027, 12, 31)), "US.1040"), [d(2027, 4, 15)]);
    }

    #[test]
    fn a_short_first_year_and_a_june_year_end() {
        // Incorporated 1 July 2026 with a calendar year: the first 1120 covers July to December.
        let c = CalendarCompany { fy_start_month: 1, incorporated: Some(d(2026, 7, 1)), ..Default::default() };
        let f = due_dates(&c, d(2026, 1, 1), d(2027, 12, 31)).into_iter().find(|x| x.rule.code == "US.1120").unwrap();
        assert_eq!((f.period_from, f.period_to, f.due), (d(2026, 7, 1), d(2026, 12, 31), d(2027, 4, 15)));
        // A June year end that began before 2026: 15 September (3rd month).
        let june = CalendarCompany { fy_start_month: 7, incorporated: None, ..c };
        assert_eq!(on(&due_dates(&june, d(2026, 9, 1), d(2026, 9, 30)), "US.1120"), [d(2026, 9, 15)]);
    }

    #[test]
    fn state_sales_tax_per_state() {
        let c = CalendarCompany {
            fy_start_month: 1,
            states: vec![StateFiling { state: "CA".into(), frequency: Frequency::Quarterly }, StateFiling { state: "TX".into(), frequency: Frequency::Monthly }],
            ..Default::default()
        };
        let v: Vec<_> = due_dates(&c, d(2026, 10, 1), d(2026, 10, 31)).into_iter().filter(|x| x.rule.code == "US.SALESTAX").collect();
        let mut who: Vec<_> = v.iter().map(|x| (x.subject.as_deref().unwrap(), x.period_from)).collect();
        who.sort();
        assert_eq!(who, [("CA", d(2026, 7, 1)), ("TX", d(2026, 9, 1))]);
        assert!(v.iter().all(|x| x.due == d(2026, 10, 20)));
    }
}

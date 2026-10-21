//! Spanish statutory due dates for the compliance calendar:
//! - Modelo 303 (VAT self-assessment): quarterly by the 20th of April, July
//!   and October and by 30 January for the fourth quarter; monthly filers
//!   (SII, large companies) by the 30th of the next month (30 January for
//!   December);
//! - Modelo 390 (annual VAT summary): by 30 January of the following year,
//!   for those not on SII.
//!
//! A deadline on a Saturday, Sunday or public holiday moves to the next
//! working day (Ley 39/2015 art. 30); weekends are applied here.
//! Research: `docs/research/accountant-desk.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift, Weekend};

const AEAT_CALENDAR: &str = "https://sede.agenciatributaria.gob.es/Sede/calendario-contribuyente.html";
const NEXT: Shift = Shift::NextWorkingDay(Weekend::SatSun);

pub const M303: Rule = Rule::new("ES.303", "Modelo 303 (VAT return) and payment", "Quarterly: by the 20th of the next month, 30 January for the fourth quarter; monthly: by the 30th of the next month", AEAT_CALENDAR, Category::Return).shift(NEXT);
pub const M390: Rule = Rule::new("ES.390", "Modelo 390 (annual VAT summary)", "By 30 January of the following year; not for SII filers", AEAT_CALENDAR, Category::Return).shift(NEXT).optional();

pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Quarterly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::VatFrom, ProfileField::TaxPeriod],
        entities: &[Entity::Company],
        period: PeriodChoice::MonthlyQuarterly,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    let vat = CalendarCompany { quarter_end: Some(3), fy_start_month: 1, ..c.clone() };
    let monthly = vat.frequency == Some(Frequency::Monthly);
    let mut out = Vec::new();
    for p in calendar::vat_periods(&vat, &defaults(), start, to) {
        let day = if monthly || p.1.month() == 12 { 30 } else { 20 };
        out.push(DueDate::new(M303, p, calendar::day_of_next_month(p.1, day)));
    }
    for y in calendar::vat_periods(&CalendarCompany { frequency: Some(Frequency::Annual), ..vat }, &defaults(), start, to) {
        out.push(DueDate::new(M390, y, NaiveDate::from_ymd_opt(y.1.year() + 1, 1, 30).expect("30 January")));
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
    fn modelo_303_quarters_and_390() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31));
        let m303: Vec<_> = v.iter().filter(|x| x.rule.code == "ES.303").map(|x| x.due).collect();
        // 30 January 2026 is a Friday; 20 April a Monday; 20 July a Monday; 20 October a Tuesday.
        assert_eq!(m303, [d(2026, 1, 30), d(2026, 4, 20), d(2026, 7, 20), d(2026, 10, 20)]);
        assert_eq!(v.iter().filter(|x| x.rule.code == "ES.390").map(|x| x.due).collect::<Vec<_>>(), [d(2026, 1, 30)]);
        // 30 January 2027 is a Saturday: Monday 1 February.
        assert!(due_dates(&c, d(2027, 2, 1), d(2027, 2, 1)).iter().any(|x| x.rule.code == "ES.303" && x.unadjusted == d(2027, 1, 30)));
    }

    #[test]
    fn monthly_filers_and_a_first_year() {
        let c = CalendarCompany { fy_start_month: 1, frequency: Some(Frequency::Monthly), incorporated: Some(d(2026, 2, 9)), ..Default::default() };
        let v: Vec<_> = due_dates(&c, d(2026, 1, 1), d(2026, 4, 30)).into_iter().filter(|x| x.rule.code == "ES.303").collect();
        assert_eq!((v[0].period_from, v[0].period_to, v[0].due), (d(2026, 2, 9), d(2026, 2, 28), d(2026, 3, 30)));
        assert!(M303.source.starts_with("https://sede.agenciatributaria.gob.es/"));
    }
}

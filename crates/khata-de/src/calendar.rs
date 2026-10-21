//! German statutory due dates for the compliance calendar:
//! - the advance VAT return (UStVA) and payment, the 10th day after each
//!   month or quarter (UStG §18(1)); a permanent extension
//!   (Dauerfristverlängerung) adds a month and is not modelled;
//! - the annual VAT return (USt-Erklärung), 31 July of the following year
//!   (AO §149(2)); later when a tax adviser prepares it.
//!
//! A deadline on a Saturday, Sunday or public holiday ends on the next
//! working day (AO §108(3)); weekends are applied here, public holidays
//! (which differ by Land) are not. Research: `docs/research/accountant-desk.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift, Weekend};

const USTG_18: &str = "https://www.gesetze-im-internet.de/ustg_1980/__18.html";
const AO_149: &str = "https://www.gesetze-im-internet.de/ao_1977/__149.html";
const NEXT: Shift = Shift::NextWorkingDay(Weekend::SatSun);

pub const USTVA: Rule = Rule::new("DE.USTVA", "UStVA (advance VAT return) and payment", "10th day after the end of the month or quarter (UStG §18(1)); one month later with a permanent extension", USTG_18, Category::Return).shift(NEXT);
pub const UST_ANNUAL: Rule = Rule::new("DE.UST", "Annual VAT return (USt-Erklärung)", "31 July of the following year (AO §149(2)); later when a tax adviser files it", AO_149, Category::Return).shift(NEXT);

/// Monthly: new businesses file monthly, and so does anyone whose VAT for
/// the previous year was over €9,000; quarterly otherwise.
pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Monthly,
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
    let mut out: Vec<DueDate> = calendar::vat_periods(&vat, &defaults(), start, to).into_iter().map(|p| DueDate::new(USTVA, p, calendar::day_of_next_month(p.1, 10))).collect();
    // The annual return covers the calendar year.
    for y in calendar::vat_periods(&CalendarCompany { frequency: Some(Frequency::Annual), ..vat }, &defaults(), start, to) {
        out.push(DueDate::new(UST_ANNUAL, y, NaiveDate::from_ymd_opt(y.1.year() + 1, 7, 31).expect("31 July")));
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
    fn ustva_on_the_10th_moved_past_the_weekend() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 10, 1), d(2026, 10, 31));
        // 10 October 2026 is a Saturday: September's UStVA is due Monday 12 October.
        assert_eq!((v[0].period_to, v[0].unadjusted, v[0].due), (d(2026, 9, 30), d(2026, 10, 10), d(2026, 10, 12)));
        let q = CalendarCompany { frequency: Some(Frequency::Quarterly), ..c.clone() };
        assert_eq!(due_dates(&q, d(2026, 11, 1), d(2026, 11, 30)).len(), 0, "no quarter ends in October");
        let annual = due_dates(&c, d(2026, 7, 1), d(2026, 7, 31));
        let a = annual.iter().find(|x| x.rule.code == "DE.UST").unwrap();
        assert_eq!((a.period_from, a.period_to, a.due), (d(2025, 1, 1), d(2025, 12, 31), d(2026, 7, 31)));
    }

    #[test]
    fn a_first_year_starts_with_the_registration() {
        let c = CalendarCompany { fy_start_month: 1, incorporated: Some(d(2026, 3, 16)), ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2027, 12, 31));
        assert_eq!(v[0].period_from, d(2026, 3, 16));
        let a = v.iter().find(|x| x.rule.code == "DE.UST").unwrap();
        assert_eq!((a.period_from, a.unadjusted, a.due), (d(2026, 3, 16), d(2027, 7, 31), d(2027, 8, 2)), "31 July 2027 is a Saturday");
        assert!(USTVA.source.starts_with("https://www.gesetze-im-internet.de/"));
    }
}

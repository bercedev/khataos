//! French statutory due dates for the compliance calendar:
//! - the CA3 VAT return and payment, monthly (régime réel normal) or
//!   quarterly (VAT under €4,000 a year): between the 15th and the 24th of
//!   the next month, on a day set for each company (shown as the 15th);
//! - under the simplified regime (annual): the CA12 return on the second
//!   working day after 1 May for a calendar year (3 months after other
//!   year ends), with instalments in July and December.
//!
//! The exact CA3 day is in the company's professional space on
//! impots.gouv.fr, so the dates may move ([`Shift::MayMove`]). Research:
//! `docs/research/accountant-desk.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Weekend};

const IMPOTS_TVA: &str = "https://www.impots.gouv.fr/professionnel/la-tva";

pub const CA3: Rule = Rule::new("FR.CA3", "CA3 VAT return and payment", "Between the 15th and the 24th of the next month, on the day set for the company (shown as the 15th)", IMPOTS_TVA, Category::Return);
pub const CA12: Rule = Rule::new("FR.CA12", "CA12 annual VAT return (simplified regime)", "Second working day after 1 May for a calendar year; 3 months after other year ends", IMPOTS_TVA, Category::Return);
pub const CA12_INSTALMENT: Rule = Rule::new("FR.CA12A", "VAT instalment (simplified regime)", "July (55%) and December (40%) of the year, around the 15th to 24th", IMPOTS_TVA, Category::Payment);

pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Monthly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::FyEnd, ProfileField::VatFrom, ProfileField::TaxPeriod],
        entities: &[Entity::Company],
        period: PeriodChoice::MonthlyQuarterlyAnnual,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    let mut out = Vec::new();
    match c.frequency.unwrap_or(Frequency::Monthly) {
        Frequency::Annual => {
            for p in calendar::vat_periods(c, &defaults(), start, to) {
                let due = if p.1.month() == 12 {
                    calendar::working_days_after(NaiveDate::from_ymd_opt(p.1.year() + 1, 5, 1).expect("1 May"), 2, Weekend::SatSun)
                } else {
                    calendar::end_of_month_after(p.1, 3)
                };
                out.push(DueDate::new(CA12, p, due));
            }
            for y in calendar::vat_periods(c, &defaults(), start, calendar::days_after(to, 366)) {
                for m in [7, 12] {
                    let date = calendar::day_of_month_after(y.0, m - 1, 15);
                    if date <= y.1 {
                        out.push(DueDate::new(CA12_INSTALMENT, y, date));
                    }
                }
            }
        }
        _ => {
            let vat = CalendarCompany { quarter_end: Some(3), ..c.clone() };
            out.extend(calendar::vat_periods(&vat, &defaults(), start, to).into_iter().map(|p| DueDate::new(CA3, p, calendar::day_of_next_month(p.1, 15))));
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

    #[test]
    fn ca3_monthly_and_quarterly() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 10, 1), d(2026, 10, 31));
        assert_eq!((v[0].period_to, v[0].due), (d(2026, 9, 30), d(2026, 10, 15)));
        let q = CalendarCompany { frequency: Some(Frequency::Quarterly), ..c };
        assert_eq!(due_dates(&q, d(2026, 1, 1), d(2026, 12, 31)).iter().map(|x| x.due).collect::<Vec<_>>(), [d(2026, 1, 15), d(2026, 4, 15), d(2026, 7, 15), d(2026, 10, 15)]);
    }

    #[test]
    fn ca12_for_the_simplified_regime() {
        let c = CalendarCompany { fy_start_month: 1, frequency: Some(Frequency::Annual), ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31));
        // 1 May 2026 is a Friday: the second working day after it is Tuesday 5 May.
        assert_eq!(v.iter().filter(|x| x.rule.code == "FR.CA12").map(|x| x.due).collect::<Vec<_>>(), [d(2026, 5, 5)]);
        assert_eq!(v.iter().filter(|x| x.rule.code == "FR.CA12A").map(|x| x.due).collect::<Vec<_>>(), [d(2026, 7, 15), d(2026, 12, 15)]);
        // A company formed in September: its first CA12 covers September to December.
        let new = CalendarCompany { incorporated: Some(d(2026, 9, 1)), ..c };
        let first = due_dates(&new, d(2026, 1, 1), d(2027, 12, 31));
        let ca12 = first.iter().find(|x| x.rule.code == "FR.CA12").unwrap();
        assert_eq!((ca12.period_from, ca12.due), (d(2026, 9, 1), d(2027, 5, 4)));
        assert!(CA3.source.starts_with("https://www.impots.gouv.fr/"));
    }
}

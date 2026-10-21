//! Dutch statutory due dates for the compliance calendar: the VAT return
//! (aangifte omzetbelasting) and payment, by the last day of the month
//! after each period, quarterly by default (monthly or annual when the
//! Belastingdienst says so). A deadline on a Saturday, Sunday or public
//! holiday moves to the next working day (Algemene termijnenwet); weekends
//! are applied here. Research: `docs/research/accountant-desk.md`.

use chrono::NaiveDate;
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift, Weekend};

const BELASTINGDIENST: &str = "https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/wanneer-btw-aangifte-doen";

pub const BTW: Rule = Rule::new("NL.BTW", "VAT return (aangifte omzetbelasting) and payment", "By the last day of the month after the period", BELASTINGDIENST, Category::Return).shift(Shift::NextWorkingDay(Weekend::SatSun));

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
    let vat = CalendarCompany { quarter_end: Some(3), fy_start_month: 1, ..c.clone() };
    let out = calendar::vat_periods(&vat, &defaults(), calendar::lookback(from), to).into_iter().map(|p| DueDate::new(BTW, p, calendar::end_of_next_month(p.1))).collect();
    calendar::within(out, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn quarterly_by_the_end_of_the_next_month() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31));
        // 31 January 2026 is a Saturday, 31 October 2026 too: both move to Monday.
        assert_eq!(v.iter().map(|x| x.due).collect::<Vec<_>>(), [d(2026, 2, 2), d(2026, 4, 30), d(2026, 7, 31), d(2026, 11, 2)]);
        let first = CalendarCompany { incorporated: Some(d(2026, 5, 4)), ..c };
        assert_eq!(due_dates(&first, d(2026, 1, 1), d(2026, 12, 31))[0].period_from, d(2026, 5, 4));
        assert!(BTW.source.starts_with("https://www.belastingdienst.nl/"));
    }
}

//! Singapore statutory due dates for the compliance calendar:
//! - the GST F5 return and payment, one month after each accounting period
//!   IRAS assigned;
//! - for companies: the estimated chargeable income (ECI) 3 months after
//!   the financial year end, Form C-S/C on 30 November of the year of
//!   assessment, the AGM within 6 months of the year end and the ACRA
//!   annual return within 7 months.
//!
//! IRAS generally grants no extensions; whether a weekend moves a date is
//! not stated, so they may move ([`Shift::MayMove`]). Research and sources:
//! `docs/research/accountant-desk.md`, `docs/research/singapore-compliance.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule};

const IRAS_DUE: &str = "https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/gst-filing-due-dates-and-requests-for-extension";
const IRAS_ECI: &str = "https://www.iras.gov.sg/taxes/corporate-income-tax/basics-of-corporate-income-tax/estimated-chargeable-income-(eci)-filing";
const IRAS_FORM_C: &str = "https://www.iras.gov.sg/taxes/corporate-income-tax/filing-and-paying-corporate-income-tax";
const ACRA: &str = "https://www.acra.gov.sg/compliance/annual-filing-requirements";

pub const GST_F5: Rule = Rule::new("SG.F5", "GST F5 return and payment", "One month after the end of the accounting period", IRAS_DUE, Category::Return);
pub const ECI: Rule = Rule::new("SG.ECI", "Estimated chargeable income (ECI)", "Within 3 months of the financial year end; waived when revenue is S$5 million or less and the ECI is nil", IRAS_ECI, Category::Return).optional();
pub const FORM_C: Rule = Rule::new("SG.FORMC", "Corporate income tax return (Form C-S/C)", "30 November of the year of assessment (the year after the financial year end)", IRAS_FORM_C, Category::Return);
pub const AGM: Rule = Rule::new("SG.AGM", "Annual general meeting", "Within 6 months of the financial year end (Companies Act s.175), unless the company has dispensed with it", ACRA, Category::Filing).optional();
pub const ANNUAL_RETURN: Rule = Rule::new("SG.AR", "ACRA annual return", "Within 7 months of the financial year end (Companies Act s.197)", ACRA, Category::Filing);

/// Quarterly periods: calendar quarters (Jan to Mar…) unless IRAS gave the
/// company others.
pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Quarterly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::FyEnd, ProfileField::VatFrom, ProfileField::TaxPeriod],
        entities: &[Entity::Company],
        period: PeriodChoice::QuarterEnd,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    let mut out: Vec<DueDate> = calendar::vat_periods(c, &defaults(), start, to).into_iter().map(|p| DueDate::new(GST_F5, p, calendar::end_of_next_month(p.1))).collect();
    if c.entity_or(Entity::Company) == Entity::Company {
        // A first financial year may run up to 18 months.
        for p in calendar::company_years(c, 6, start, to) {
            out.push(DueDate::new(ECI, p, calendar::end_of_month_after(p.1, 3)));
            out.push(DueDate::new(FORM_C, p, NaiveDate::from_ymd_opt(p.1.year() + 1, 11, 30).expect("30 November")));
            out.push(DueDate::new(AGM, p, calendar::end_of_month_after(p.1, 6)));
            out.push(DueDate::new(ANNUAL_RETURN, p, calendar::end_of_month_after(p.1, 7)));
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
    fn f5_one_month_after_each_quarter() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31));
        assert_eq!(on(&v, "SG.F5"), [d(2026, 1, 31), d(2026, 4, 30), d(2026, 7, 31), d(2026, 10, 31)]);
        let monthly = CalendarCompany { frequency: Some(Frequency::Monthly), ..c };
        assert_eq!(due_dates(&monthly, d(2026, 3, 1), d(2026, 3, 31)).iter().find(|x| x.rule.code == "SG.F5").unwrap().period_to, d(2026, 2, 28));
        assert!(GST_F5.source.starts_with("https://www.iras.gov.sg/"));
    }

    #[test]
    fn company_filings_for_a_december_year_end() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31));
        assert_eq!(on(&v, "SG.ECI"), [d(2026, 3, 31)]);
        assert_eq!(on(&v, "SG.AGM"), [d(2026, 6, 30)]);
        assert_eq!(on(&v, "SG.AR"), [d(2026, 7, 31)]);
        assert_eq!(on(&v, "SG.FORMC"), [d(2026, 11, 30)], "YA 2026 for the year to 31 December 2025");
    }

    #[test]
    fn a_first_year_from_incorporation() {
        // Formed on 1 August 2026 with a December year end: the first year runs to 31 December 2027.
        let c = CalendarCompany { fy_start_month: 1, incorporated: Some(d(2026, 8, 1)), ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2028, 12, 31));
        assert_eq!(on(&v, "SG.AR"), [d(2028, 7, 31)]);
        assert_eq!(on(&v, "SG.F5").first(), Some(&d(2026, 10, 31)), "the first GST period starts on incorporation");
    }
}

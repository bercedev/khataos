//! Saudi statutory due dates for the compliance calendar:
//! - the VAT return and payment, by the last day of the month after each
//!   period (monthly above SAR 40 million of supplies, else quarterly);
//! - the zakat (or income tax) return, 120 days after the financial year;
//! - the commercial registration's annual confirmation (optional; see the
//!   research document).
//!
//! The 24-hour rule for reporting simplified e-invoices is a reminder from
//! the ZATCA queue, not a calendar date ([`REPORTING_24H`] holds its
//! citation). A deadline on a non-working day (Friday or Saturday) moves to
//! the next working day; public holidays are not known to KhataOS.
//! Research and sources: `docs/research/accountant-desk.md`.

use chrono::NaiveDate;
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule, Shift, Weekend};

const ZATCA_VAT: &str = "https://zatca.gov.sa/en/HelpCenter/guidelines/Documents/Agents%20Guideline.pdf";
const ZATCA_ZAKAT: &str = "https://zatca.gov.sa/en/HelpCenter/FAQs/Pages/FAQ_035.aspx";
const ZATCA_EINVOICE: &str = "https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/Simplified%20guide_En.pdf";
const MC_CR: &str = "https://mc.gov.sa/en/Regulations/Pages/default.aspx";

const NEXT: Shift = Shift::NextWorkingDay(Weekend::FriSat);

pub const VAT_RETURN: Rule = Rule::new("SA.VAT", "VAT return and payment", "By the last day of the month after the tax period (VAT Implementing Regulations Art. 59 and 62)", ZATCA_VAT, Category::Return).shift(NEXT);
pub const ZAKAT: Rule = Rule::new("SA.ZAKAT", "Zakat or income tax return", "Within 120 days of the end of the financial year", ZATCA_ZAKAT, Category::Return).shift(NEXT);
pub const CR_CONFIRM: Rule = Rule::new("SA.CR", "Commercial registration annual confirmation", "Confirm the registration's data each year, around its anniversary (Commercial Register Law 2025)", MC_CR, Category::Filing).optional();
/// Simplified (B2C) tax invoices and their notes are reported to ZATCA
/// within 24 hours of being issued (Phase 2 integration).
pub const REPORTING_24H: Rule = Rule::new("SA.REPORT24H", "Report simplified invoices to ZATCA", "Within 24 hours of issuing a simplified tax invoice or note", ZATCA_EINVOICE, Category::Return).shift(Shift::Fixed);

/// Quarterly (calendar quarters) unless taxable supplies exceed SAR 40
/// million a year, which makes the period monthly.
pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Quarterly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::FyEnd, ProfileField::VatFrom, ProfileField::TaxPeriod],
        entities: &[Entity::Company],
        period: PeriodChoice::MonthlyQuarterly,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    // ZATCA's quarters are calendar quarters.
    let vat = CalendarCompany { quarter_end: Some(3), ..c.clone() };
    let mut out: Vec<DueDate> = calendar::vat_periods(&vat, &defaults(), start, to).into_iter().map(|p| DueDate::new(VAT_RETURN, p, calendar::end_of_next_month(p.1))).collect();
    for p in calendar::company_years(c, 6, start, to) {
        out.push(DueDate::new(ZAKAT, p, calendar::days_after(p.1, 120)));
    }
    if let Some(inc) = c.incorporated {
        for a in calendar::anniversaries(inc, start, to) {
            out.push(DueDate::new(CR_CONFIRM, (a.checked_sub_months(chrono::Months::new(12)).expect("dates in range"), a), a));
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
    fn vat_due_by_the_end_of_the_next_month_not_28_days() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v: Vec<_> = due_dates(&c, d(2026, 4, 1), d(2026, 4, 30)).into_iter().filter(|x| x.rule.code == "SA.VAT").collect();
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].period_from, v[0].period_to, v[0].due), (d(2026, 1, 1), d(2026, 3, 31), d(2026, 4, 30)));
        let m = CalendarCompany { frequency: Some(Frequency::Monthly), ..c };
        let feb = due_dates(&m, d(2028, 2, 1), d(2028, 3, 1));
        assert_eq!((feb[0].period_to, feb[0].unadjusted, feb[0].due), (d(2028, 1, 31), d(2028, 2, 29), d(2028, 2, 29)), "a Tuesday stays");
        // 31 July 2026 is a Friday: June's return moves to Sunday 2 August.
        let jul = due_dates(&m, d(2026, 7, 1), d(2026, 8, 5));
        let june = jul.iter().find(|x| x.period_to == d(2026, 6, 30)).unwrap();
        assert_eq!((june.unadjusted, june.due), (d(2026, 7, 31), d(2026, 8, 2)));
    }

    #[test]
    fn zakat_120_days_after_the_year_and_the_first_year() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let z: Vec<_> = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31)).into_iter().filter(|x| x.rule.code == "SA.ZAKAT").collect();
        assert_eq!((z[0].period_to, z[0].due), (d(2025, 12, 31), d(2026, 4, 30)));
        // Formed on 1 September 2026: the first year runs to 31 December 2027.
        let new = CalendarCompany { incorporated: Some(d(2026, 9, 1)), ..c };
        let v = due_dates(&new, d(2026, 1, 1), d(2028, 12, 31));
        let first = v.iter().find(|x| x.rule.code == "SA.ZAKAT").unwrap();
        assert_eq!((first.period_from, first.period_to), (d(2026, 9, 1), d(2027, 12, 31)));
        assert_eq!(v.iter().filter(|x| x.rule.code == "SA.CR").map(|x| x.due).collect::<Vec<_>>(), [d(2027, 9, 1), d(2028, 9, 1)]);
        for r in [VAT_RETURN, ZAKAT, REPORTING_24H] {
            assert!(r.source.starts_with("https://zatca.gov.sa/"), "{}", r.code);
        }
    }
}

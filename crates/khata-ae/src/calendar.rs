//! UAE statutory due dates for the compliance calendar:
//! - the VAT return and payment, 28 days after each tax period the FTA
//!   assigned (quarters may start in any month);
//! - the corporate tax return and payment, 9 months after each tax period
//!   (the financial year; the first from incorporation, for periods starting
//!   on or after 1 June 2023).
//!
//! Economic Substance Regulations reports are not due for financial years
//! starting on or after 1 January 2023 (Cabinet Resolution 98 of 2024),
//! and the UBO register is kept, not filed periodically, so neither is a
//! date. Estimates: whether a deadline on a weekend moves is reported only
//! by secondary sources ([`Shift::MayMove`]). Research and sources:
//! `docs/research/accountant-desk.md`.

use chrono::NaiveDate;
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule};

const FTA_VAT: &str = "https://tax.gov.ae/en/taxes/vat/vat.topics/filing.vat.returns.and.making.payments.aspx";
const FTA_CT: &str = "https://tax.gov.ae/en/media.centre/news/federal.tax.authority.urges.submission.of.corporate.tax.returns.and.settlement.of.corporate.tax.liabilities.within.nine.months.from.the.end.of.the.tax.period.aspx";

pub const VAT201: Rule = Rule::new("AE.VAT201", "VAT return (VAT 201) and payment", "Within 28 days of the end of the tax period", FTA_VAT, Category::Return);
pub const CORPORATE_TAX: Rule = Rule::new("AE.CT", "Corporate tax return and payment", "Within 9 months of the end of the tax period (the financial year; the first from incorporation)", FTA_CT, Category::Return);

/// Corporate tax applies to tax periods starting on or after this date.
pub const CT_FROM: NaiveDate = match NaiveDate::from_ymd_opt(2023, 6, 1) {
    Some(d) => d,
    None => panic!("valid date"),
};

/// Quarterly tax periods by default; the FTA assigns each registrant its
/// stagger, set in the compliance profile as the month a quarter starts.
pub fn defaults() -> CalendarDefaults {
    CalendarDefaults {
        frequency: Frequency::Quarterly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::Entity, ProfileField::FyEnd, ProfileField::VatFrom, ProfileField::TaxPeriod, ProfileField::CorporateTax],
        entities: &[Entity::Company, Entity::Partnership, Entity::SoleProprietor],
        period: PeriodChoice::QuarterStart,
    }
}

pub fn due_dates(c: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    let start = calendar::lookback(from);
    let mut out: Vec<DueDate> = calendar::vat_periods(c, &defaults(), start, to).into_iter().map(|p| DueDate::new(VAT201, p, calendar::days_after(p.1, 28))).collect();
    // Juridical persons are registered for corporate tax; a natural person only above AED 1 million turnover.
    let ct = c.corporate_tax.unwrap_or(c.entity_or(Entity::Company) == Entity::Company);
    if ct {
        for p in calendar::company_years(c, 6, start, to).into_iter().filter(|p| p.0 >= CT_FROM) {
            out.push(DueDate::new(CORPORATE_TAX, p, calendar::end_of_month_after(p.1, 9)));
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
    fn vat_due_28_days_after_the_period() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let v: Vec<_> = due_dates(&c, d(2026, 10, 1), d(2026, 10, 31)).into_iter().filter(|x| x.rule.code == "AE.VAT201").collect();
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].period_from, v[0].period_to, v[0].due), (d(2026, 7, 1), d(2026, 9, 30), d(2026, 10, 28)));
        // Quarters starting in February (Feb-Apr): due 28 May.
        let s = CalendarCompany { quarter_end: Some(4), ..c.clone() };
        assert_eq!(due_dates(&s, d(2026, 5, 1), d(2026, 5, 31))[0].due, d(2026, 5, 28));
        // Monthly filers: January's return by 28 February (28 days after 31 Jan).
        let m = CalendarCompany { frequency: Some(Frequency::Monthly), ..c };
        assert_eq!(due_dates(&m, d(2026, 2, 1), d(2026, 2, 28))[0].due, d(2026, 2, 28));
    }

    #[test]
    fn registration_starts_the_vat_periods() {
        let c = CalendarCompany { fy_start_month: 1, vat_from: Some(d(2026, 8, 10)), corporate_tax: Some(false), ..Default::default() };
        let v = due_dates(&c, d(2026, 1, 1), d(2027, 1, 31));
        assert_eq!(v.iter().map(|x| (x.period_from, x.due)).collect::<Vec<_>>(), [(d(2026, 8, 10), d(2026, 10, 28)), (d(2026, 10, 1), d(2027, 1, 28))]);
    }

    #[test]
    fn corporate_tax_nine_months_after_the_year_and_the_first_period() {
        let c = CalendarCompany { fy_start_month: 1, ..Default::default() };
        let ct: Vec<_> = due_dates(&c, d(2026, 1, 1), d(2026, 12, 31)).into_iter().filter(|x| x.rule.code == "AE.CT").collect();
        assert_eq!((ct[0].period_to, ct[0].due), (d(2025, 12, 31), d(2026, 9, 30)));
        // Formed on 1 March 2026: the first tax period runs to 31 December 2026 (more than 6 months).
        let new = CalendarCompany { incorporated: Some(d(2026, 3, 1)), ..c.clone() };
        let first: Vec<_> = due_dates(&new, d(2026, 1, 1), d(2028, 12, 31)).into_iter().filter(|x| x.rule.code == "AE.CT").collect();
        assert_eq!((first[0].period_from, first[0].period_to, first[0].due), (d(2026, 3, 1), d(2026, 12, 31), d(2027, 9, 30)));
        // Not registered for corporate tax: no CT dates.
        let off = CalendarCompany { corporate_tax: Some(false), ..c };
        assert!(due_dates(&off, d(2026, 1, 1), d(2026, 12, 31)).iter().all(|x| x.rule.code != "AE.CT"));
        assert!(VAT201.source.starts_with("https://tax.gov.ae/") && CORPORATE_TAX.source.starts_with("https://tax.gov.ae/"));
    }
}

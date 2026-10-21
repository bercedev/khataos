//! India's statutory due dates for the compliance calendar:
//! - GST returns (monthly, or quarterly under QRMP) and the annual GSTR-9/9C;
//! - TDS and TCS payments and statements, advance tax;
//! - the income-tax return and the tax audit report;
//! - company-law filings (AGM, AOC-4, MGT-7, DIR-3 KYC, DPT-3, MSME-1) and
//!   LLP Forms 8 and 11.
//!
//! The financial ("previous") year is always April to March. A company formed
//! in January to March closes its first year the next March (Companies Act
//! s.2(41)). Every date is an estimate: the GST Council, CBDT and MCA extend
//! dates only by notification, so the date may move ([`Shift::MayMove`]).
//! Sources and confidence: `docs/research/accountant-desk.md`.

use chrono::{Datelike, NaiveDate};
use khata_compliance::calendar::{self, CalendarCompany, CalendarDefaults, Category, DueDate, Entity, Frequency, PeriodChoice, ProfileField, Rule};

use crate::{tcs, tds};

const GST_PORTAL_GSTR1: &str = "https://tutorial.gst.gov.in/userguide/returns/GSTR_1.htm";
const GST_PORTAL_GSTR3B: &str = "https://tutorial.gst.gov.in/userguide/returns/GSTR3B.htm";
const QRMP_ADVISORY: &str = "https://tutorial.gst.gov.in/offlineutilities/returns/QRMP_Advisory.pdf";
const IFF_FAQ: &str = "https://tutorial.gst.gov.in/userguide/returns/FAQs_IFF.htm";
const CGST_ACT: &str = "https://cbic-gst.gov.in/gst-acts.html";
const TDS_FAQ: &str = "https://www.incometaxindia.gov.in/documents/81799/11848482/FAQs-on-Interplay-and-Transition.pdf";
const ADVANCE_TAX: &str = "https://www.incometaxindia.gov.in/w/section-406-5";
const ITR: &str = "https://www.incometax.gov.in/iec/foportal/help/individual/return-applicable-1";
const MCA_ACTS: &str = "https://www.mca.gov.in/content/mca/global/en/acts-rules/ebooks/acts.html";
const MCA_LLP: &str = "https://www.mca.gov.in/content/mca/global/en/acts-rules/ebooks/rules.html";

use Category::{Audit, Filing, Payment, Return};

pub const GSTR1: Rule = Rule::new("IN.GSTR1", "GSTR-1", "11th of the next month (CGST s.37, Rule 59)", GST_PORTAL_GSTR1, Return);
pub const GSTR3B: Rule = Rule::new("IN.GSTR3B", "GSTR-3B and GST payment", "20th of the next month (CGST s.39, Rule 61)", GST_PORTAL_GSTR3B, Return);
pub const GSTR1_QRMP: Rule = Rule::new("IN.GSTR1Q", "GSTR-1 (quarterly, QRMP)", "13th of the month after the quarter", GST_PORTAL_GSTR1, Return);
pub const IFF: Rule = Rule::new("IN.IFF", "Invoice Furnishing Facility (IFF)", "13th of the next month, for months 1 and 2 of a QRMP quarter; optional, and closed after the due date", IFF_FAQ, Return).optional();
pub const PMT06: Rule = Rule::new("IN.PMT06", "GST payment (PMT-06, QRMP)", "25th of the next month, for months 1 and 2 of a QRMP quarter", QRMP_ADVISORY, Payment);
pub const GSTR3B_QRMP_22: Rule = Rule::new("IN.GSTR3BQ", "GSTR-3B (quarterly, QRMP)", "22nd of the month after the quarter for this state (Rule 61, Category X states)", QRMP_ADVISORY, Return);
pub const GSTR3B_QRMP_24: Rule = Rule::new("IN.GSTR3BQ", "GSTR-3B (quarterly, QRMP)", "24th of the month after the quarter for this state (Rule 61, Category Y states)", QRMP_ADVISORY, Return);
pub const GSTR9: Rule = Rule::new("IN.GSTR9", "GSTR-9 (annual return)", "31 December after the financial year (CGST s.44, Rule 80); not needed up to ₹2 crore turnover", CGST_ACT, Return).optional();
pub const GSTR9C: Rule = Rule::new("IN.GSTR9C", "GSTR-9C (reconciliation statement)", "31 December after the financial year, with GSTR-9, above ₹5 crore turnover (Rule 80(3))", CGST_ACT, Audit).optional();
pub const TDS_PAY: Rule = Rule::new("IN.TDSPAY", "TDS payment", "7th of the next month; 30 April for March (old Rule 30, Rule 218 of the 2026 Rules)", TDS_FAQ, Payment);
pub const TDS_RETURN: Rule = Rule::new("IN.TDSQ", "TDS quarterly statements", "31 July, 31 October, 31 January and 31 May (Forms 138 and 140; 24Q and 26Q before 2026-27)", TDS_FAQ, Return);
pub const TCS_PAY: Rule = Rule::new("IN.TCSPAY", "TCS payment", "7th of the next month, March included (old Rule 37CA)", TDS_FAQ, Payment);
pub const TCS_RETURN: Rule = Rule::new("IN.TCSQ", "TCS quarterly statement", "31 July, 31 October, 31 January and 31 May from 2026-27 (Form 143); the 15th of those months before (27EQ)", TDS_FAQ, Return);
pub const ADVANCE: Rule = Rule::new("IN.ADVTAX", "Advance tax instalment", "15 June (15%), 15 September (45%), 15 December (75%) and 15 March (100%) when the year's tax is ₹10,000 or more", ADVANCE_TAX, Payment).optional();
pub const ITR_AUDIT: Rule = Rule::new("IN.ITR", "Income-tax return", "31 October after the year for companies and for accounts under tax audit (old s.139(1))", ITR, Return);
pub const ITR_OTHER: Rule = Rule::new("IN.ITR", "Income-tax return", "31 July after the year when no tax audit applies (old s.139(1))", ITR, Return);
pub const TAX_AUDIT: Rule = Rule::new("IN.TAXAUDIT", "Tax audit report (Form 3CA/3CB with 3CD)", "30 September after the year, a month before the return (old s.44AB; Form 26 under the 2025 Act)", ITR, Audit);
pub const AGM: Rule = Rule::new("IN.AGM", "Annual general meeting", "Within 6 months of the year end; the first AGM within 9 months of the first year end (Companies Act s.96)", MCA_ACTS, Filing);
pub const AOC4: Rule = Rule::new("IN.AOC4", "AOC-4 (financial statements to the ROC)", "30 days after the AGM, counted from its last day (Companies Act s.137)", MCA_ACTS, Filing);
pub const MGT7: Rule = Rule::new("IN.MGT7", "MGT-7 (annual return to the ROC)", "60 days after the AGM, counted from its last day (Companies Act s.92)", MCA_ACTS, Filing);
pub const DIR3_KYC: Rule = Rule::new("IN.DIR3KYC", "DIR-3 KYC of directors and designated partners", "30 September each year (Rule 12A, Appointment and Qualification of Directors Rules)", MCA_ACTS, Filing);
pub const DPT3: Rule = Rule::new("IN.DPT3", "DPT-3 (return of deposits and loans)", "30 June each year, as at 31 March (Acceptance of Deposits Rules, Rule 16)", MCA_ACTS, Filing).optional();
pub const MSME1: Rule = Rule::new("IN.MSME1", "MSME-1 (dues to micro and small suppliers)", "31 October for April to September; 30 April for October to March, when dues are over 45 days", MCA_ACTS, Filing).optional();
pub const LLP11: Rule = Rule::new("IN.LLP11", "LLP Form 11 (annual return)", "30 May, within 60 days of the year end (LLP Rules, Rule 25)", MCA_LLP, Filing);
pub const LLP8: Rule = Rule::new("IN.LLP8", "LLP Form 8 (statement of account and solvency)", "30 October, within 30 days of six months from the year end (LLP Rules, Rule 24)", MCA_LLP, Filing);

/// States and union territories whose quarterly GSTR-3B is due on the
/// 22nd (Rule 61 proviso, "Category X"); every other one is due on the
/// 24th. Codes are GSTIN state codes; 25 (Daman and Diu) and 28 (Andhra
/// Pradesh) are old codes still found in GSTINs.
pub const QRMP_22ND: &[&str] = &["22", "23", "24", "25", "26", "27", "28", "29", "30", "31", "32", "33", "34", "35", "36", "37"];

/// States and union territories due on the 24th ("Category Y").
pub const QRMP_24TH: &[&str] = &["01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20", "21", "38"];

/// The quarterly GSTR-3B rule for a state and its day of the month. An
/// unknown state gets the earlier date (22nd), so the reminder is never late.
pub fn gstr3b_quarterly(region: Option<&str>) -> (Rule, u32) {
    match region {
        Some(r) if QRMP_24TH.contains(&r) => (GSTR3B_QRMP_24, 24),
        _ => (GSTR3B_QRMP_22, 22),
    }
}

pub fn defaults() -> CalendarDefaults {
    // QRMP quarters are fixed: April to June and so on.
    CalendarDefaults {
        frequency: Frequency::Monthly,
        quarter_end: 3,
        quarterly_allowed: true,
        fields: &[ProfileField::Incorporated, ProfileField::Entity, ProfileField::VatFrom, ProfileField::TaxPeriod, ProfileField::Audit, ProfileField::Withholding],
        entities: &[Entity::Company, Entity::Llp, Entity::Partnership, Entity::SoleProprietor],
        period: PeriodChoice::MonthlyQuarterly,
    }
}

fn due(rule: Rule, p: (NaiveDate, NaiveDate), due: NaiveDate) -> DueDate {
    DueDate::new(rule, p, due)
}

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("a valid date")
}

/// The 15th-of-month statement date used for TCS before 2026-27.
fn old_tcs_statement_due(q: tds::Quarter) -> Option<NaiveDate> {
    let (_, end) = q.range()?;
    Some(calendar::day_of_month_after(end, if q.q == 4 { 2 } else { 1 }, 15))
}

/// Quarterly statement date for a quarter; the same dates applied before
/// 2026-27 (old Rule 31A), which `tds::Quarter::statement_due` leaves open.
fn statement_due(q: tds::Quarter) -> Option<NaiveDate> {
    q.statement_due().or_else(|| {
        let (_, end) = q.range()?;
        Some(if q.q == 4 { calendar::day_of_month_after(end, 2, 31) } else { calendar::end_of_next_month(end) })
    })
}

pub fn due_dates(company: &CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    // The previous year is April to March whatever the books say; QRMP quarters are fixed.
    let c = &CalendarCompany { fy_start_month: 4, quarter_end: Some(3), ..company.clone() };
    let start = calendar::lookback(from);
    let d = defaults();
    let mut out = Vec::new();
    let gst = calendar::vat_periods(&CalendarCompany { frequency: Some(Frequency::Monthly), ..c.clone() }, &d, start, to);
    match c.frequency.unwrap_or(Frequency::Monthly) {
        Frequency::Quarterly => {
            let (r3b, r3b_day) = gstr3b_quarterly(c.region.as_deref());
            for p in calendar::vat_periods(c, &d, start, to) {
                out.push(due(GSTR1_QRMP, p, calendar::day_of_next_month(p.1, 13)));
                out.push(due(r3b, p, calendar::day_of_next_month(p.1, r3b_day)));
            }
            // Months 1 and 2 of each quarter: IFF and the PMT-06 payment.
            for &p in gst.iter().filter(|p| p.1.month() % 3 != 0) {
                out.push(due(IFF, p, calendar::day_of_next_month(p.1, 13)));
                out.push(due(PMT06, p, calendar::day_of_next_month(p.1, 25)));
            }
        }
        _ => {
            for &p in &gst {
                out.push(due(GSTR1, p, calendar::day_of_next_month(p.1, 11)));
                out.push(due(GSTR3B, p, calendar::day_of_next_month(p.1, 20)));
            }
        }
    }
    let entity = c.entity_or(Entity::Company);
    // Income tax and GST count a first "previous year" from incorporation to
    // the next 31 March; company law may run the first year on to the
    // following March (s.2(41)), which moves the AGM and ROC filings.
    for y in calendar::company_years(c, 3, start, to) {
        let fy_end = y.1;
        match entity {
            Entity::Company => {
                let agm = calendar::end_of_month_after(fy_end, if calendar::is_first_year(c, y) { 9 } else { 6 });
                out.push(due(AGM, y, agm));
                out.push(due(AOC4, y, calendar::days_after(agm, 30)));
                out.push(due(MGT7, y, calendar::days_after(agm, 60)));
                out.push(due(DPT3, y, ymd(fy_end.year(), 6, 30)));
                out.push(due(DIR3_KYC, y, ymd(fy_end.year(), 9, 30)));
            }
            Entity::Llp => {
                out.push(due(LLP11, y, ymd(fy_end.year(), 5, 30)));
                out.push(due(LLP8, y, ymd(fy_end.year(), 10, 30)));
                out.push(due(DIR3_KYC, y, ymd(fy_end.year(), 9, 30)));
            }
            _ => {}
        }
    }
    for y in calendar::company_years(c, 0, start, calendar::days_after(to, 366)) {
        let (fy_start, fy_end) = y;
        if y.1 <= to {
            if !c.vat_unregistered && c.vat_from.is_none_or(|v| v <= y.1) {
                out.push(due(GSTR9, y, ymd(fy_end.year(), 12, 31)));
                out.push(due(GSTR9C, y, ymd(fy_end.year(), 12, 31)));
            }
            let audited = c.audit || entity == Entity::Company;
            out.push(due(if audited { ITR_AUDIT } else { ITR_OTHER }, y, ymd(fy_end.year(), if audited { 10 } else { 7 }, 31)));
            if c.audit {
                out.push(due(TAX_AUDIT, y, ymd(fy_end.year(), 9, 30)));
            }
        }
        // Advance tax: 15 Jun, Sep, Dec of the year's first calendar year, 15 Mar of the next.
        for (yy, m) in [(fy_end.year() - 1, 6), (fy_end.year() - 1, 9), (fy_end.year() - 1, 12), (fy_end.year(), 3)] {
            let date = ymd(yy, m, 15);
            if date >= fy_start {
                out.push(due(ADVANCE, y, date));
            }
        }
    }
    if entity == Entity::Company {
        // MSME-1: half-years April to September and October to March.
        for p in calendar::periods(Frequency::Monthly, 3, start, to).into_iter().filter(|p| p.1.month() == 9 || p.1.month() == 3) {
            let half = (p.0.checked_sub_months(chrono::Months::new(5)).expect("dates in range"), p.1);
            if c.incorporated.is_none_or(|i| half.1 >= i) {
                out.push(due(MSME1, half, if p.1.month() == 9 { ymd(p.1.year(), 10, 31) } else { ymd(p.1.year(), 4, 30) }));
            }
        }
    }
    if c.withholding || c.collection {
        let alive = |p: &(NaiveDate, NaiveDate)| c.incorporated.is_none_or(|i| p.1 >= i);
        let months: Vec<_> = calendar::periods(Frequency::Monthly, 3, start, to).into_iter().filter(alive).collect();
        let quarters: Vec<_> = calendar::periods(Frequency::Quarterly, 3, start, to).into_iter().filter(alive).collect();
        for &p in &months {
            if c.withholding {
                out.push(due(TDS_PAY, p, tds::deposit_due(p.1)));
            }
            if c.collection {
                out.push(due(TCS_PAY, p, tcs::deposit_due(p.1)));
            }
        }
        for &p in &quarters {
            let q = tds::Quarter::of(p.1);
            if c.withholding {
                out.extend(statement_due(q).map(|d| due(TDS_RETURN, p, d)));
            }
            if c.collection {
                let d = if q.year >= tcs::RULES_FROM.year() { statement_due(q) } else { old_tcs_statement_due(q) };
                if let Some(d) = d {
                    out.push(due(TCS_RETURN, p, d));
                }
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|d| seen.insert((d.rule.code, d.due, d.period_to)));
    calendar::within(out, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn company(frequency: Frequency, region: &str) -> CalendarCompany {
        CalendarCompany { frequency: Some(frequency), region: Some(region.into()), fy_start_month: 4, ..Default::default() }
    }

    fn codes_on(dates: &[DueDate], on: NaiveDate) -> Vec<&'static str> {
        dates.iter().filter(|x| x.due == on).map(|x| x.rule.code).collect()
    }

    #[test]
    fn monthly_gst_returns() {
        let v = due_dates(&company(Frequency::Monthly, "27"), d(2026, 11, 1), d(2026, 11, 30));
        let r1 = v.iter().find(|x| x.rule.code == "IN.GSTR1").unwrap();
        assert_eq!((r1.period_from, r1.period_to, r1.due), (d(2026, 10, 1), d(2026, 10, 31), d(2026, 11, 11)));
        assert_eq!(codes_on(&v, d(2026, 11, 20)), ["IN.GSTR3B"]);
        assert!(v.iter().all(|x| x.rule.code != "IN.TDSPAY"), "no TDS without deductions");
        assert_eq!(codes_on(&v, d(2026, 12, 31)), Vec::<&str>::new(), "outside the range");
    }

    #[test]
    fn qrmp_quarters_by_state() {
        // Maharashtra (27) is due on the 22nd, Delhi (07) on the 24th.
        let mh = due_dates(&company(Frequency::Quarterly, "27"), d(2027, 1, 1), d(2027, 1, 31));
        assert_eq!(codes_on(&mh, d(2027, 1, 13)), ["IN.GSTR1Q"]);
        assert_eq!(codes_on(&mh, d(2027, 1, 22)), ["IN.GSTR3BQ"]);
        let dl = due_dates(&company(Frequency::Quarterly, "07"), d(2027, 1, 1), d(2027, 1, 31));
        assert_eq!(codes_on(&dl, d(2027, 1, 24)), ["IN.GSTR3BQ"]);
        assert!(codes_on(&dl, d(2027, 1, 22)).is_empty());
        assert!(dl.iter().all(|x| x.rule.code != "IN.GSTR1" && x.rule.code != "IN.GSTR3B"), "no monthly returns under QRMP");
        // An unknown state: the earlier date.
        assert_eq!(gstr3b_quarterly(None), (GSTR3B_QRMP_22, 22));
        // Months 1 and 2 of the quarter: IFF on the 13th, PMT-06 on the 25th.
        let nov = due_dates(&company(Frequency::Quarterly, "07"), d(2026, 11, 1), d(2026, 11, 30));
        assert_eq!(codes_on(&nov, d(2026, 11, 13)), ["IN.IFF"]);
        assert_eq!(codes_on(&nov, d(2026, 11, 25)), ["IN.PMT06"]);
        let jan = due_dates(&company(Frequency::Quarterly, "07"), d(2027, 1, 25), d(2027, 1, 25));
        assert!(jan.is_empty(), "no PMT-06 for the quarter's last month (December)");
    }

    #[test]
    fn tds_and_tcs_dates() {
        let c = CalendarCompany { withholding: true, collection: true, entity: Some(Entity::Partnership), ..company(Frequency::Monthly, "27") };
        let apr = due_dates(&c, d(2027, 4, 1), d(2027, 4, 30));
        assert_eq!(codes_on(&apr, d(2027, 4, 30)), ["IN.TDSPAY"], "March TDS by 30 April");
        assert_eq!(codes_on(&apr, d(2027, 4, 7)), ["IN.TCSPAY"], "March TCS by 7 April");
        let jul = due_dates(&c, d(2026, 7, 1), d(2026, 7, 31));
        assert!(codes_on(&jul, d(2026, 7, 31)).contains(&"IN.TDSQ"));
        assert!(codes_on(&jul, d(2026, 7, 31)).contains(&"IN.TCSQ"), "the 2026 rules move TCS to the 31st");
        let may = due_dates(&c, d(2026, 5, 1), d(2026, 5, 31));
        assert!(codes_on(&may, d(2026, 5, 31)).contains(&"IN.TDSQ"), "Q4 of 2025-26 under the old rules");
        assert_eq!(codes_on(&may, d(2026, 5, 15)), ["IN.TCSQ"], "27EQ for Q4 2025-26 on the 15th");
    }

    #[test]
    fn annual_dates() {
        let v = due_dates(&company(Frequency::Monthly, "27"), d(2026, 12, 1), d(2026, 12, 31));
        let r9 = v.iter().find(|x| x.rule.code == "IN.GSTR9").unwrap();
        assert_eq!((r9.period_from, r9.period_to, r9.due), (d(2025, 4, 1), d(2026, 3, 31), d(2026, 12, 31)));
        assert!(r9.rule.optional);
        let adv: Vec<_> = due_dates(&company(Frequency::Monthly, "27"), d(2026, 4, 1), d(2027, 3, 31))
            .into_iter()
            .filter(|x| x.rule.code == "IN.ADVTAX")
            .map(|x| x.due)
            .collect();
        assert_eq!(adv, [d(2026, 6, 15), d(2026, 9, 15), d(2026, 12, 15), d(2027, 3, 15)]);
    }

    fn on(dates: &[DueDate], code: &str) -> Vec<NaiveDate> {
        dates.iter().filter(|x| x.rule.code == code).map(|x| x.due).collect()
    }

    #[test]
    fn company_filings_in_a_normal_year() {
        let c = company(Frequency::Monthly, "27");
        let v = due_dates(&c, d(2026, 4, 1), d(2027, 3, 31));
        assert_eq!(on(&v, "IN.AGM"), [d(2026, 9, 30)]);
        assert_eq!(on(&v, "IN.AOC4"), [d(2026, 10, 30)]);
        assert_eq!(on(&v, "IN.MGT7"), [d(2026, 11, 29)]);
        assert_eq!(on(&v, "IN.ITR"), [d(2026, 10, 31)], "companies: 31 October");
        assert_eq!(on(&v, "IN.DIR3KYC"), [d(2026, 9, 30)]);
        assert_eq!(on(&v, "IN.DPT3"), [d(2026, 6, 30)]);
        assert_eq!(on(&v, "IN.MSME1"), [d(2026, 4, 30), d(2026, 10, 31)]);
        assert!(on(&v, "IN.TAXAUDIT").is_empty() && on(&v, "IN.LLP11").is_empty());
    }

    #[test]
    fn a_first_year_runs_from_incorporation() {
        // Formed on 10 February 2026: the first year runs to 31 March 2027, its AGM within 9 months.
        let c = CalendarCompany { incorporated: Some(d(2026, 2, 10)), ..company(Frequency::Monthly, "27") };
        let v = due_dates(&c, d(2026, 1, 1), d(2028, 3, 31));
        assert_eq!(on(&v, "IN.AGM"), [d(2027, 12, 31)]);
        assert_eq!(on(&v, "IN.AOC4"), [d(2028, 1, 30)]);
        let r1 = v.iter().find(|x| x.rule.code == "IN.GSTR1").unwrap();
        assert_eq!((r1.period_from, r1.period_to, r1.due), (d(2026, 2, 10), d(2026, 2, 28), d(2026, 3, 11)), "nothing for January");
        let adv = on(&v, "IN.ADVTAX");
        assert_eq!(adv.first(), Some(&d(2026, 3, 15)), "the first year's instalments from its start");
        // Not registered for GST: no GST returns at all.
        let none = CalendarCompany { vat_unregistered: true, ..c };
        assert!(due_dates(&none, d(2026, 1, 1), d(2028, 3, 31)).iter().all(|x| !x.rule.code.starts_with("IN.GSTR")));
    }

    #[test]
    fn llps_and_audited_firms() {
        let llp = CalendarCompany { entity: Some(Entity::Llp), ..company(Frequency::Monthly, "27") };
        let v = due_dates(&llp, d(2026, 4, 1), d(2027, 3, 31));
        assert_eq!((on(&v, "IN.LLP11"), on(&v, "IN.LLP8")), (vec![d(2026, 5, 30)], vec![d(2026, 10, 30)]));
        assert_eq!(on(&v, "IN.ITR"), [d(2026, 7, 31)], "no audit: 31 July");
        assert!(on(&v, "IN.AGM").is_empty());
        let firm = CalendarCompany { entity: Some(Entity::Partnership), audit: true, ..llp };
        let v = due_dates(&firm, d(2026, 4, 1), d(2027, 3, 31));
        assert_eq!((on(&v, "IN.TAXAUDIT"), on(&v, "IN.ITR")), (vec![d(2026, 9, 30)], vec![d(2026, 10, 31)]));
        assert!(on(&v, "IN.DIR3KYC").is_empty());
    }

    #[test]
    fn every_rule_cites_an_official_source() {
        for r in [
            GSTR1, GSTR3B, GSTR1_QRMP, IFF, PMT06, GSTR3B_QRMP_22, GSTR3B_QRMP_24, GSTR9, GSTR9C, TDS_PAY, TDS_RETURN, TCS_PAY, TCS_RETURN, ADVANCE, ITR_AUDIT,
            ITR_OTHER, TAX_AUDIT, AGM, AOC4, MGT7, DIR3_KYC, DPT3, MSME1, LLP11, LLP8,
        ] {
            assert_eq!(r.shift, khata_compliance::Shift::MayMove, "{}: India moves dates only by notification", r.code);
            assert!(r.source.starts_with("https://") && (r.source.contains(".gov.in")), "{}", r.code);
        }
        for s in QRMP_22ND {
            assert!(!QRMP_24TH.contains(s), "{s} in both groups");
        }
    }
}

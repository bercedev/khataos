//! Statutory due dates for the compliance calendar.
//!
//! Each pack turns its filing rules into dated [`DueDate`]s for a company
//! ([`crate::CompliancePack::due_dates`]). Every [`Rule`] carries the text of
//! the rule and an official source, so the screen can show where a date
//! comes from. The dates are **estimates**: governments extend deadlines by
//! notification, and weekends and holidays move some of them, so the app
//! always says to check the official portal. Nothing here is posted to the
//! books.

use chrono::{Datelike, Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// How often the company files a periodic return.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    #[default]
    Monthly,
    Quarterly,
    /// Once a year (France's CA12, a US state's annual sales tax return).
    Annual,
}

/// The legal form of the business, which decides its annual filings.
/// Each pack lists the forms it knows ([`CalendarDefaults::entities`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    /// A company limited by shares (private limited, LLC in the Gulf, GmbH, SARL, BV, SL, C corporation, S.A. de C.V.).
    Company,
    /// A limited liability partnership.
    Llp,
    /// A partnership or firm.
    Partnership,
    /// A sole proprietor or individual in business.
    SoleProprietor,
    /// A US S corporation (Form 1120-S).
    SCorporation,
}

/// A US state the company files sales tax in, and how often.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateFiling {
    /// Two-letter state code ("CA").
    pub state: String,
    pub frequency: Frequency,
}

/// What a pack needs to know about a company to date its filings. The app
/// fills it from the company's compliance profile and books; `None` takes
/// the pack's default ([`crate::CompliancePack::calendar_defaults`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarCompany {
    pub frequency: Option<Frequency>,
    /// A month (1 to 12) a quarter ends in: 3 for Mar, Jun, Sep and Dec;
    /// 1 for Jan, Apr, Jul and Oct (a UK "stagger" or a UAE tax period).
    pub quarter_end: Option<u32>,
    /// The company's tax region (an Indian state code from the GSTIN).
    pub region: Option<String>,
    /// First month of the financial year.
    pub fy_start_month: u32,
    /// The company deducts tax at source (India: TDS).
    pub withholding: bool,
    /// The company collects tax at source (India: TCS).
    pub collection: bool,
    /// Date of incorporation (or of starting the business): nothing is due
    /// for a period before it, and first-year rules count from it.
    pub incorporated: Option<NaiveDate>,
    /// `None`: the pack's usual form (a company).
    pub entity: Option<Entity>,
    /// Registered for VAT/GST from this date; `None`: for as long as the books show.
    pub vat_from: Option<NaiveDate>,
    /// Not registered for VAT/GST at all: no periodic VAT/GST returns.
    pub vat_unregistered: bool,
    /// Registered for corporate tax (UAE); `None`: the pack's default.
    pub corporate_tax: Option<bool>,
    /// The accounts are audited, or a tax audit applies (India).
    pub audit: bool,
    /// US: the states it files sales tax in.
    pub states: Vec<StateFiling>,
}

impl CalendarCompany {
    pub fn entity_or(&self, default: Entity) -> Entity {
        self.entity.unwrap_or(default)
    }

    /// A company or LLP: the entity files with a companies registrar.
    pub fn incorporated_body(&self) -> bool {
        matches!(self.entity.unwrap_or(Entity::Company), Entity::Company | Entity::Llp | Entity::SCorporation)
    }
}

/// A field of the compliance profile, in the order the dialog asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileField {
    Incorporated,
    Entity,
    FyEnd,
    VatFrom,
    TaxPeriod,
    CorporateTax,
    Audit,
    Withholding,
    States,
}

/// How the dialog asks for the VAT/GST period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodChoice {
    /// Monthly or quarterly (calendar quarters, or India's QRMP).
    MonthlyQuarterly,
    /// Monthly, quarterly or annual (France).
    MonthlyQuarterlyAnnual,
    /// Monthly, or quarterly by HMRC stagger 1, 2 or 3.
    UkStagger,
    /// Monthly, or quarters starting in a given month (UAE FTA periods).
    QuarterStart,
    /// Monthly, or quarters ending in a given month (Singapore, KSA).
    QuarterEnd,
}

/// A pack's default filing pattern and the profile fields it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CalendarDefaults {
    pub frequency: Frequency,
    pub quarter_end: u32,
    /// Whether the company may choose quarterly filing at all.
    pub quarterly_allowed: bool,
    /// The profile fields this country uses, in the dialog's order.
    pub fields: &'static [ProfileField],
    /// The legal forms the country's rules know; the first is the default.
    pub entities: &'static [Entity],
    pub period: PeriodChoice,
}

/// What a filing is, for grouping and filtering on the desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// A tax return (it may include the payment).
    Return,
    /// A tax payment without a return.
    Payment,
    /// A company-law filing (registrar, AGM, annual return).
    Filing,
    /// An audit report.
    Audit,
}

/// The weekend of a country, for moving deadlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekend {
    SatSun,
    FriSat,
}

/// What happens to a deadline on a weekend or holiday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "weekend")]
pub enum Shift {
    /// The law keeps the date (HMRC, Companies House).
    Fixed,
    /// The law moves it to the next working day; weekends are applied here,
    /// public holidays are not known to KhataOS.
    NextWorkingDay(Weekend),
    /// Unclear, or moved only when the authority announces it.
    MayMove,
}

/// One filing rule: what is due, by when, and where the rule is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rule {
    /// Stable id, "IN.GSTR1"; the app keeps "filed" marks under it.
    pub code: &'static str,
    /// Short English name ("GSTR-1"); the screen translates it.
    pub title: &'static str,
    /// The rule in words ("11th of the next month").
    pub rule: &'static str,
    /// An official page for the rule.
    pub source: &'static str,
    /// Only for some companies (turnover limits, optional returns).
    pub optional: bool,
    pub category: Category,
    pub shift: Shift,
}

impl Rule {
    /// A rule that applies to every company of its kind, with no weekend rule yet.
    pub const fn new(code: &'static str, title: &'static str, rule: &'static str, source: &'static str, category: Category) -> Self {
        Self { code, title, rule, source, optional: false, category, shift: Shift::MayMove }
    }

    /// Only for some companies.
    pub const fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub const fn shift(mut self, shift: Shift) -> Self {
        self.shift = shift;
        self
    }
}

/// A dated obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueDate {
    #[serde(flatten)]
    pub rule: Rule,
    /// The period the filing or payment covers, both ends inclusive.
    pub period_from: NaiveDate,
    pub period_to: NaiveDate,
    /// After the weekend rule ([`Shift::NextWorkingDay`]).
    pub due: NaiveDate,
    /// The date the rule gives, before any weekend move.
    pub unadjusted: NaiveDate,
    /// What one rule is dated for when it applies several times over the
    /// same period (a US state code for its sales tax return).
    pub subject: Option<String>,
}

impl DueDate {
    /// Date `due` for `rule` over `period`, moved for weekends when the rule says so.
    pub fn new(rule: Rule, (period_from, period_to): (NaiveDate, NaiveDate), due: NaiveDate) -> Self {
        let moved = match rule.shift {
            Shift::NextWorkingDay(w) => next_working_day(due, w),
            Shift::Fixed | Shift::MayMove => due,
        };
        Self { rule, period_from, period_to, due: moved, unadjusted: due, subject: None }
    }

    /// The same obligation for one subject (a US state).
    pub fn for_subject(mut self, subject: &str) -> Self {
        self.subject = Some(subject.to_owned());
        self
    }
}

/// `d`, or the next day that isn't a weekend day.
pub fn next_working_day(mut d: NaiveDate, weekend: Weekend) -> NaiveDate {
    use chrono::Weekday::*;
    loop {
        let off = match weekend {
            Weekend::SatSun => matches!(d.weekday(), Sat | Sun),
            Weekend::FriSat => matches!(d.weekday(), Fri | Sat),
        };
        if !off {
            return d;
        }
        d = d.succ_opt().expect("dates in range");
    }
}

/// The `n`th working day after `d` (`d` itself not counted).
pub fn working_days_after(d: NaiveDate, n: u32, weekend: Weekend) -> NaiveDate {
    let mut x = d;
    for _ in 0..n {
        x = next_working_day(x.succ_opt().expect("dates in range"), weekend);
    }
    x
}

/// `d` plus `n` calendar months, clamped to the month's end (31 Jan + 1 → 28 Feb).
pub fn add_months(d: NaiveDate, n: u32) -> NaiveDate {
    d.checked_add_months(Months::new(n)).expect("dates in range")
}

/// The last day of the month `n` months after the month of `d`.
pub fn end_of_month_after(d: NaiveDate, n: u32) -> NaiveDate {
    let first = month_start_after(d, n);
    month_end(first.year(), first.month())
}

/// Financial years (start, end) ending in `from..=to`, with the first one
/// starting on the incorporation date. A year end less than `min_months`
/// after incorporation is skipped: that first period runs on to the next
/// year end (India: a company formed in January to March closes its first
/// year the next March).
pub fn company_years(c: &CalendarCompany, min_months: u32, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    let mut years = year_ends(c.fy_start_month, from, to);
    if let Some(inc) = c.incorporated {
        let earliest = add_months(inc, min_months);
        years.retain(|y| y.1 >= earliest);
        let first_end = year_ends(c.fy_start_month, earliest, add_months(earliest, 13)).first().map(|y| y.1);
        for y in &mut years {
            if Some(y.1) == first_end {
                y.0 = inc;
            }
        }
    }
    years
}

/// Whether `period` is the company's first financial year.
pub fn is_first_year(c: &CalendarCompany, period: (NaiveDate, NaiveDate)) -> bool {
    c.incorporated == Some(period.0)
}

/// VAT/GST periods ending in `from..=to` for the company's registration:
/// none when unregistered, none ending before the registration (or
/// incorporation) date, and the first one starting on it.
pub fn vat_periods(c: &CalendarCompany, d: &CalendarDefaults, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    if c.vat_unregistered {
        return Vec::new();
    }
    let frequency = c.frequency.unwrap_or(d.frequency);
    let quarter_end = c.quarter_end.unwrap_or(d.quarter_end);
    let mut out = match frequency {
        Frequency::Annual => year_ends(c.fy_start_month, from, to),
        f => periods(f, quarter_end, from, to),
    };
    if let Some(start) = c.vat_from.or(c.incorporated) {
        out.retain(|p| p.1 >= start);
        for p in &mut out {
            if p.0 < start {
                p.0 = start;
            }
        }
    }
    out
}

/// Anniversaries of `start` falling in `from..=to` (the first one a year after `start`).
pub fn anniversaries(start: NaiveDate, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    (1..200u32).map(|k| add_months(start, 12 * k)).skip_while(|d| *d < from).take_while(|d| *d <= to).collect()
}

/// The last day of a month.
pub fn month_end(year: i32, month: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("month in range");
    first.checked_add_months(Months::new(1)).and_then(|d| d.pred_opt()).expect("dates in range")
}

/// The first day of the month `n` months after the month of `d`.
fn month_start_after(d: NaiveDate, n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1)
        .and_then(|f| f.checked_add_months(Months::new(n)))
        .expect("dates in range")
}

/// Day `day` of the month after the month of `d`, or that month's last day
/// when it is shorter.
pub fn day_of_next_month(d: NaiveDate, day: u32) -> NaiveDate {
    day_of_month_after(d, 1, day)
}

/// Day `day` of the month `n` months after the month of `d` (clamped to
/// the month's end).
pub fn day_of_month_after(d: NaiveDate, n: u32, day: u32) -> NaiveDate {
    let first = month_start_after(d, n);
    let end = month_end(first.year(), first.month());
    NaiveDate::from_ymd_opt(first.year(), first.month(), day.min(end.day())).expect("clamped")
}

/// The last day of the month after the month of `d`.
pub fn end_of_next_month(d: NaiveDate) -> NaiveDate {
    let first = month_start_after(d, 1);
    month_end(first.year(), first.month())
}

/// `n` days after `d`.
pub fn days_after(d: NaiveDate, n: u64) -> NaiveDate {
    d.checked_add_days(Days::new(n)).expect("dates in range")
}

/// Tax periods (start, end) of `frequency` whose end falls in `from..=to`.
/// Quarters end in `quarter_end` and every third month from it.
pub fn periods(frequency: Frequency, quarter_end: u32, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    let step = match frequency {
        Frequency::Monthly => 1,
        Frequency::Quarterly => 3,
        Frequency::Annual => 12,
    };
    let end_month = quarter_end.clamp(1, 12);
    let phase = (end_month - 1) % 3;
    let mut out = Vec::new();
    let mut m = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).expect("valid");
    while m <= to {
        let end = month_end(m.year(), m.month());
        let ends = match step {
            1 => true,
            3 => (m.month() - 1) % 3 == phase,
            _ => m.month() == end_month,
        };
        if end >= from && end <= to && ends {
            let start = month_start_after(end, 1).checked_sub_months(Months::new(step)).expect("dates in range");
            out.push((start, end));
        }
        m = m.checked_add_months(Months::new(1)).expect("dates in range");
    }
    out
}

/// Financial years (start, end) whose end falls in `from..=to`.
pub fn year_ends(fy_start_month: u32, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    let start_month = fy_start_month.clamp(1, 12);
    let end_month = if start_month == 1 { 12 } else { start_month - 1 };
    let mut out = Vec::new();
    for year in from.year() - 1..=to.year() {
        let end = month_end(year, end_month);
        if end >= from && end <= to {
            let start = month_start_after(end, 1).checked_sub_months(Months::new(12)).expect("dates in range");
            out.push((start, end));
        }
    }
    out
}

/// Keep the due dates in `from..=to`, sorted by date and code.
pub fn within(mut dates: Vec<DueDate>, from: NaiveDate, to: NaiveDate) -> Vec<DueDate> {
    dates.retain(|d| d.due >= from && d.due <= to);
    dates.sort_by(|a, b| (a.due, a.rule.code, a.period_to).cmp(&(b.due, b.rule.code, b.period_to)));
    dates
}

/// How far back to look for periods whose due date may fall in a range:
/// the longest rule (UK first accounts, 21 months after incorporation;
/// returns due 12 months after the year) plus a margin.
pub const LOOKBACK_MONTHS: u32 = 24;

/// The earliest period end worth generating for due dates from `from`.
pub fn lookback(from: NaiveDate) -> NaiveDate {
    from.checked_sub_months(Months::new(LOOKBACK_MONTHS)).expect("dates in range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn month_arithmetic_clamps_to_short_months() {
        assert_eq!(month_end(2028, 2), d(2028, 2, 29));
        assert_eq!(day_of_next_month(d(2027, 1, 31), 30), d(2027, 2, 28));
        assert_eq!(day_of_next_month(d(2026, 12, 31), 11), d(2027, 1, 11));
        assert_eq!(end_of_next_month(d(2026, 1, 31)), d(2026, 2, 28));
        assert_eq!(day_of_month_after(d(2026, 6, 30), 2, 7), d(2026, 8, 7));
        assert_eq!(days_after(d(2026, 9, 30), 28), d(2026, 10, 28));
    }

    #[test]
    fn quarters_follow_the_stagger() {
        let q = periods(Frequency::Quarterly, 3, d(2026, 1, 1), d(2026, 12, 31));
        assert_eq!(q.iter().map(|p| p.1).collect::<Vec<_>>(), [d(2026, 3, 31), d(2026, 6, 30), d(2026, 9, 30), d(2026, 12, 31)]);
        assert_eq!(q[0].0, d(2026, 1, 1));
        let s = periods(Frequency::Quarterly, 1, d(2026, 1, 1), d(2026, 6, 30));
        assert_eq!(s, [(d(2025, 11, 1), d(2026, 1, 31)), (d(2026, 2, 1), d(2026, 4, 30))]);
        // 12 is the same stagger as 3 (Mar, Jun, Sep, Dec are every third month from Dec).
        assert_eq!(periods(Frequency::Quarterly, 12, d(2026, 1, 1), d(2026, 12, 31)), q);
        assert_eq!(periods(Frequency::Monthly, 3, d(2026, 1, 15), d(2026, 3, 30)).len(), 2, "Jan and Feb end in range; Mar doesn't");
    }

    #[test]
    fn weekends_move_only_when_the_rule_says_so() {
        // 10 Oct 2026 is a Saturday.
        assert_eq!(next_working_day(d(2026, 10, 10), Weekend::SatSun), d(2026, 10, 12));
        assert_eq!(next_working_day(d(2026, 10, 10), Weekend::FriSat), d(2026, 10, 11));
        assert_eq!(next_working_day(d(2026, 10, 9), Weekend::FriSat), d(2026, 10, 11), "Friday in the Gulf");
        let r = Rule::new("X", "x", "x", "https://example.gov", Category::Return);
        let p = (d(2026, 9, 1), d(2026, 9, 30));
        assert_eq!(DueDate::new(r, p, d(2026, 10, 10)).due, d(2026, 10, 10), "may move: shown as the rule gives it");
        let moved = DueDate::new(r.shift(Shift::NextWorkingDay(Weekend::SatSun)), p, d(2026, 10, 10));
        assert_eq!((moved.due, moved.unadjusted), (d(2026, 10, 12), d(2026, 10, 10)));
        assert_eq!(working_days_after(d(2026, 5, 1), 2, Weekend::SatSun), d(2026, 5, 5), "1 May 2026 is a Friday");
    }

    #[test]
    fn first_years_start_on_incorporation() {
        let c = CalendarCompany { fy_start_month: 4, incorporated: Some(d(2026, 2, 10)), ..Default::default() };
        // Less than 3 months to 31 March: the first year runs to the next March.
        assert_eq!(company_years(&c, 3, d(2026, 1, 1), d(2027, 12, 31)), [(d(2026, 2, 10), d(2027, 3, 31))]);
        let c = CalendarCompany { incorporated: Some(d(2025, 11, 1)), ..c };
        let y = company_years(&c, 3, d(2026, 1, 1), d(2027, 12, 31));
        assert_eq!(y, [(d(2025, 11, 1), d(2026, 3, 31)), (d(2026, 4, 1), d(2027, 3, 31))]);
        assert!(is_first_year(&c, y[0]) && !is_first_year(&c, y[1]));
        assert_eq!(company_years(&CalendarCompany { fy_start_month: 1, ..Default::default() }, 3, d(2026, 1, 1), d(2026, 12, 31)).len(), 1, "no incorporation date: every year");
    }

    #[test]
    fn vat_periods_follow_the_registration() {
        let dflt = CalendarDefaults { frequency: Frequency::Quarterly, quarter_end: 3, quarterly_allowed: true, fields: &[], entities: &[Entity::Company], period: PeriodChoice::QuarterEnd };
        let c = CalendarCompany { fy_start_month: 1, vat_from: Some(d(2026, 5, 15)), ..Default::default() };
        assert_eq!(vat_periods(&c, &dflt, d(2026, 1, 1), d(2026, 12, 31)), [(d(2026, 5, 15), d(2026, 6, 30)), (d(2026, 7, 1), d(2026, 9, 30)), (d(2026, 10, 1), d(2026, 12, 31))]);
        assert!(vat_periods(&CalendarCompany { vat_unregistered: true, ..c.clone() }, &dflt, d(2026, 1, 1), d(2026, 12, 31)).is_empty());
        let annual = CalendarCompany { frequency: Some(Frequency::Annual), vat_from: None, ..c };
        assert_eq!(vat_periods(&annual, &dflt, d(2026, 1, 1), d(2026, 12, 31)), [(d(2026, 1, 1), d(2026, 12, 31))]);
        assert_eq!(anniversaries(d(2024, 2, 29), d(2025, 1, 1), d(2026, 12, 31)), [d(2025, 2, 28), d(2026, 2, 28)]);
    }

    #[test]
    fn year_ends_for_april_and_january_years() {
        assert_eq!(year_ends(4, d(2026, 1, 1), d(2026, 12, 31)), [(d(2025, 4, 1), d(2026, 3, 31))]);
        assert_eq!(year_ends(1, d(2026, 1, 1), d(2026, 12, 31)), [(d(2026, 1, 1), d(2026, 12, 31))]);
    }
}

//! Voucher numbering series.
//!
//! Each voucher type has one series. A series can restart every financial
//! year or every month, and can wrap the running number in a prefix and
//! suffix with date tokens. A number only has to be unique within its
//! series period, which is what invoice rules ask for (India: unique for a
//! financial year; EU: one or more series that identify the invoice), and
//! what Tally's "restart numbering" does.
//!
//! Tokens in a prefix or suffix: `{FY}` the financial year ("26-27", or
//! "2026" when the year is a calendar year), `{YYYY}` / `{YY}` the calendar
//! year and `{MM}` the month of the voucher date.

use chrono::{Datelike, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// When a series starts again from 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Restart {
    /// One series for the life of the company.
    #[default]
    Never,
    /// A new series each financial year.
    Yearly,
    /// A new series each calendar month.
    Monthly,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberSeries {
    #[serde(default)]
    pub restart: Restart,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
    /// Zero-pad the running number to this many digits (0: no padding).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: u8,
}

fn is_zero(w: &u8) -> bool {
    *w == 0
}

/// Longest prefix or suffix template accepted.
pub const MAX_AFFIX: usize = 16;
/// Widest zero padding accepted.
pub const MAX_WIDTH: u8 = 9;

impl NumberSeries {
    /// A series suited to tax invoices in most countries: a fresh series
    /// each financial year, with the year in the number so that bill
    /// references stay unique across years ("26-27/1").
    pub fn yearly_with_year() -> Self {
        Self { restart: Restart::Yearly, prefix: "{FY}/".into(), suffix: String::new(), width: 0 }
    }

    /// Whether the templates are acceptable: short, printable, and only
    /// known tokens.
    pub fn is_valid(&self) -> bool {
        let ok = |t: &str| {
            t.chars().count() <= MAX_AFFIX
                && !t.chars().any(char::is_control)
                && !strip_tokens(t).contains(['{', '}'])
        };
        ok(&self.prefix) && ok(&self.suffix) && self.width <= MAX_WIDTH
    }

    /// The series period containing `date`: `None` for a never-restarting
    /// series, else its first and last day.
    pub fn period(&self, date: NaiveDate, fy_start_month: u32) -> Option<(NaiveDate, NaiveDate)> {
        let start = match self.restart {
            Restart::Never => return None,
            Restart::Monthly => NaiveDate::from_ymd_opt(date.year(), date.month(), 1)?,
            Restart::Yearly => fy_start(date, fy_start_month),
        };
        let months = if self.restart == Restart::Monthly { 1 } else { 12 };
        Some((start, start.checked_add_months(Months::new(months))?.pred_opt()?))
    }

    /// Prefix and suffix with the tokens filled in for `date`.
    pub fn affixes(&self, date: NaiveDate, fy_start_month: u32) -> (String, String) {
        (render(&self.prefix, date, fy_start_month), render(&self.suffix, date, fy_start_month))
    }

    /// The running number inside `number`, if it belongs to this series on `date`.
    pub fn running(&self, number: &str, date: NaiveDate, fy_start_month: u32) -> Option<u64> {
        let (pre, suf) = self.affixes(date, fy_start_month);
        let lower = number.to_lowercase();
        let middle = lower.strip_prefix(&pre.to_lowercase())?.strip_suffix(&suf.to_lowercase())?;
        if middle.is_empty() || !middle.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        middle.parse().ok()
    }

    /// The number with running value `n` on `date`.
    pub fn format(&self, n: u64, date: NaiveDate, fy_start_month: u32) -> String {
        let (pre, suf) = self.affixes(date, fy_start_month);
        format!("{pre}{n:0width$}{suf}", width = usize::from(self.width))
    }
}

/// First day of the financial year containing `date`.
pub fn fy_start(date: NaiveDate, start_month: u32) -> NaiveDate {
    let m = start_month.clamp(1, 12);
    let year = if date.month() >= m { date.year() } else { date.year() - 1 };
    NaiveDate::from_ymd_opt(year, m, 1).expect("day 1 of a valid month")
}

/// "26-27" for an April–March year, "2026" for a calendar year.
pub fn fy_label(date: NaiveDate, start_month: u32) -> String {
    let start = fy_start(date, start_month);
    if start.month() == 1 {
        start.year().to_string()
    } else {
        format!("{:02}-{:02}", start.year() % 100, (start.year() + 1) % 100)
    }
}

const TOKENS: [&str; 4] = ["{FY}", "{YYYY}", "{YY}", "{MM}"];

fn strip_tokens(t: &str) -> String {
    TOKENS.iter().fold(t.to_owned(), |s, tok| s.replace(tok, ""))
}

fn render(t: &str, date: NaiveDate, fy_start_month: u32) -> String {
    t.replace("{FY}", &fy_label(date, fy_start_month))
        .replace("{YYYY}", &date.year().to_string())
        .replace("{YY}", &format!("{:02}", date.year() % 100))
        .replace("{MM}", &format!("{:02}", date.month()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn yearly_series_follow_the_financial_year() {
        let s = NumberSeries::yearly_with_year();
        assert_eq!(s.period(d(2026, 3, 31), 4), Some((d(2025, 4, 1), d(2026, 3, 31))));
        assert_eq!(s.period(d(2026, 4, 1), 4), Some((d(2026, 4, 1), d(2027, 3, 31))));
        assert_eq!(s.format(7, d(2026, 4, 1), 4), "26-27/7");
        assert_eq!(s.format(7, d(2026, 4, 1), 1), "2026/7");
        assert_eq!(s.running("26-27/12", d(2026, 5, 1), 4), Some(12));
        assert_eq!(s.running("25-26/12", d(2026, 5, 1), 4), None, "last year's series");
    }

    #[test]
    fn padding_monthly_and_validation() {
        let s = NumberSeries { restart: Restart::Monthly, prefix: "INV/{YY}{MM}/".into(), suffix: "-B".into(), width: 4 };
        assert!(s.is_valid());
        assert_eq!(s.format(3, d(2026, 7, 9), 4), "INV/2607/0003-B");
        assert_eq!(s.running("inv/2607/0003-b", d(2026, 7, 1), 4), Some(3));
        assert_eq!(s.period(d(2026, 2, 10), 4), Some((d(2026, 2, 1), d(2026, 2, 28))));
        assert!(!NumberSeries { prefix: "{QQ}".into(), ..Default::default() }.is_valid());
        assert!(!NumberSeries { prefix: "A".repeat(17), ..Default::default() }.is_valid());
        assert_eq!(NumberSeries::default().period(d(2026, 1, 1), 4), None);
    }
}

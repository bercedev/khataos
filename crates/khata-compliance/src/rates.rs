//! Effective-dated tax rates.
//!
//! Rates change by notification (India's GST 2.0 moved most goods to 5% and
//! 18% from 22 Sep 2025), and invoices, credit notes and amendments dated
//! before a change must keep the old rate. So rates are data with history,
//! never constants in a pack.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A rate that takes effect on `from` and holds until the next change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateChange {
    pub from: NaiveDate,
    /// Percentage, e.g. 18 for 18%.
    pub percent: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RateError {
    #[error("a tax class is required")]
    EmptyClass,
    #[error("rate {0}% is not between 0 and 100")]
    OutOfRange(Decimal),
}

/// Rate history by tax class. A class is an HSN/SAC code or a named
/// category; a lookup uses the longest class that prefixes the code and has
/// a rate in force, so a heading ("8471") covers its sub-headings
/// ("84713010") unless they have their own entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateSchedule {
    classes: BTreeMap<String, Vec<RateChange>>,
}

/// Classes compare without spaces and case: "8471 30" is "847130".
fn normalise(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_uppercase).collect()
}

impl RateSchedule {
    /// Record that `class` is taxed at `percent` from `from` onwards. A
    /// second change on the same date replaces the first.
    pub fn set(&mut self, class: &str, from: NaiveDate, percent: Decimal) -> Result<(), RateError> {
        let class = normalise(class);
        if class.is_empty() {
            return Err(RateError::EmptyClass);
        }
        if percent.is_sign_negative() || percent > Decimal::ONE_HUNDRED {
            return Err(RateError::OutOfRange(percent));
        }
        let changes = self.classes.entry(class).or_default();
        changes.retain(|c| c.from != from);
        changes.push(RateChange { from, percent });
        changes.sort_by_key(|c| c.from);
        Ok(())
    }

    /// The rate in force for `class` on `on`, or `None` if nothing covers it.
    pub fn rate(&self, class: &str, on: NaiveDate) -> Option<Decimal> {
        let class = normalise(class);
        self.classes
            .iter()
            .filter(|(key, _)| class.starts_with(key.as_str()))
            .filter_map(|(key, changes)| {
                let current = changes.iter().filter(|c| c.from <= on).max_by_key(|c| c.from)?;
                Some((key.len(), current.percent))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, percent)| percent)
    }

    /// The full history of one class, oldest first.
    pub fn history(&self, class: &str) -> &[RateChange] {
        self.classes.get(&normalise(class)).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn a_rate_holds_until_the_next_change() {
        let mut r = RateSchedule::default();
        r.set("8471", d(2017, 7, 1), dec!(18)).unwrap();
        r.set("8471", d(2025, 9, 22), dec!(5)).unwrap();
        assert_eq!(r.rate("8471", d(2017, 6, 30)), None, "nothing before the first change");
        assert_eq!(r.rate("8471", d(2017, 7, 1)), Some(dec!(18)));
        assert_eq!(r.rate("8471", d(2025, 9, 21)), Some(dec!(18)), "old invoices keep the old rate");
        assert_eq!(r.rate("8471", d(2025, 9, 22)), Some(dec!(5)));
        assert_eq!(r.rate("8471", d(2030, 1, 1)), Some(dec!(5)));
    }

    #[test]
    fn longest_matching_class_wins_while_it_is_in_force() {
        let mut r = RateSchedule::default();
        r.set("84", d(2017, 7, 1), dec!(18)).unwrap();
        r.set("847130", d(2025, 9, 22), dec!(5)).unwrap();
        assert_eq!(r.rate("84713010", d(2026, 1, 1)), Some(dec!(5)), "sub-heading entry");
        assert_eq!(r.rate("84713010", d(2025, 1, 1)), Some(dec!(18)), "chapter rate before the sub-heading had one");
        assert_eq!(r.rate("8472", d(2026, 1, 1)), Some(dec!(18)));
        assert_eq!(r.rate("9983", d(2026, 1, 1)), None);
        assert_eq!(r.rate("8", d(2026, 1, 1)), None, "a shorter code is not covered by a longer class");
    }

    #[test]
    fn classes_ignore_spaces_and_case_and_same_day_changes_replace() {
        let mut r = RateSchedule::default();
        r.set("sac 9983", d(2017, 7, 1), dec!(18)).unwrap();
        r.set("SAC9983", d(2017, 7, 1), dec!(12)).unwrap();
        assert_eq!(r.history("Sac 9983"), [RateChange { from: d(2017, 7, 1), percent: dec!(12) }]);
        assert_eq!(r.rate("SAC 998311", d(2020, 1, 1)), Some(dec!(12)));
    }

    #[test]
    fn rejects_bad_rates_and_empty_classes() {
        let mut r = RateSchedule::default();
        assert_eq!(r.set(" ", d(2017, 7, 1), dec!(5)), Err(RateError::EmptyClass));
        assert_eq!(r.set("1", d(2017, 7, 1), dec!(-1)), Err(RateError::OutOfRange(dec!(-1))));
        assert_eq!(r.set("1", d(2017, 7, 1), dec!(100.01)), Err(RateError::OutOfRange(dec!(100.01))));
        r.set("1", d(2017, 7, 1), dec!(0)).unwrap();
        assert_eq!(r.rate("1", d(2018, 1, 1)), Some(dec!(0)), "nil-rated is a rate, not a missing one");
    }

    #[test]
    fn schedules_round_trip_as_data() {
        let mut r = RateSchedule::default();
        r.set("8471", d(2017, 7, 1), dec!(18)).unwrap();
        r.set("8471", d(2025, 9, 22), dec!(5)).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"18\""), "percentages are decimal strings, never floats: {json}");
        assert_eq!(serde_json::from_str::<RateSchedule>(&json).unwrap(), r);
    }
}

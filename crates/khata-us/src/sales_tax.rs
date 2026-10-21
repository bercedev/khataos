//! Sales tax on an invoice line from the company's own jurisdictions and
//! rates.
//!
//! There is no federal sales tax or VAT: sales tax is levied by 45 states
//! and DC and by thousands of local jurisdictions (counties, cities and
//! special districts), each with its own rates, taxability rules and rate
//! changes (`docs/research/us-compliance.md` §1, §4). KhataOS cannot ship
//! and maintain rates for every jurisdiction, so **the company defines the
//! jurisdictions it collects in and their rates**, and this module adds the
//! components up per jurisdiction. See `docs/us-sales-tax.md`.
//!
//! # Jurisdiction codes
//!
//! A jurisdiction is the state's USPS code, optionally followed by `-` and
//! a local code the company chooses (1 to [`MAX_LOCAL_LEN`] letters or
//! digits): `CA` is "California, state tax only"; `CA-LA` might be "Los
//! Angeles city, Los Angeles County". The line's jurisdiction is its place
//! of supply ([`PlaceOfSupply::Region`]): for a sale, where the goods are
//! delivered (destination sourcing). The app takes it from the party's
//! address region, so a customer's region can be `CA` or `CA-LA`.
//!
//! Origin sourcing (some states tax certain in-state sales at the seller's
//! location) is not applied automatically: the company sets the customer's
//! jurisdiction to its own for such sales [UNCERTAIN: the rules differ by
//! state; research §3].
//!
//! # Rate keys
//!
//! Rates live in the ordinary effective-dated [`RateSchedule`], keyed by
//! these names (all within the 24-character class limit of the books):
//!
//! | key               | component  | example                     |
//! |-------------------|------------|-----------------------------|
//! | `CA`              | `STATE`    | 7.25 (from the state's date)|
//! | `CA-LA/COUNTY`    | `COUNTY`   | 0.25                        |
//! | `CA-LA/CITY`      | `CITY`     | 0.75                        |
//! | `CA-LA/DISTRICT`  | `DISTRICT` | 1.5 (all special districts) |
//! | `CA:FOOD`         | state rate for one tax class (see below) | 0 |
//!
//! Keys are looked up **exactly** (never by prefix), so `CA` never stands
//! in for a missing `CA-LA/COUNTY`. A line in `CA-LA` carries a `STATE`
//! component from `CA` and one component for each local key on file.
//!
//! **No state key means the company does not collect in that state** (no
//! permit, no nexus): the line carries no tax and is reported as
//! "not collected" ([`Taxability::NotCollected`]), not as an error. The
//! report lists those sales by state, so sales into a state where the
//! company may owe tax are visible.
//!
//! # Taxability by tax class
//!
//! The line's tax class is the product's taxability category, as in other
//! packs (an item or service ledger's class):
//! - a class whose name starts with `EXEMPT`, `NONTAXABLE` or `RESALE` is
//!   exempt everywhere (no components);
//! - a key `<STATE>:<CLASS>` replaces the state rate for that class in that
//!   state; a 0% override makes the line exempt in the state, local taxes
//!   included (a simplification: a few states let local taxes apply where
//!   the state exempts [UNCERTAIN]);
//! - any other class is taxable at the full combined rate.
//!
//! Exemption and resale certificates are per customer, not per product:
//! they are kept per customer ledger ([`crate::exemptions`]) and make the
//! customer's sales in the certificate's state exempt in the sales tax
//! report. On a single invoice, a class `EXEMPT…`/`RESALE…` (or no tax)
//! still does the same.
//!
//! # Rounding
//!
//! The combined tax is rounded to the cent, half away from zero, and split
//! over the components (each rounded, the last taking what is left), so the
//! components always add up to the rounded combined tax. States set their
//! own rounding rules (per invoice or per line, some with bracket tables)
//! [UNCERTAIN: not uniform; research §3].
//!
//! Exports ([`PlaceOfSupply::Abroad`]) carry no sales tax.

use chrono::NaiveDate;
use khata_compliance::{LineTax, PlaceOfSupply, RateSchedule, TaxComponent, TaxError, TaxLine};
use khata_core::Amount;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::states;

/// The state's own rate.
pub const STATE: &str = "STATE";
/// A county's rate.
pub const COUNTY: &str = "COUNTY";
/// A city's (or town's) rate.
pub const CITY: &str = "CITY";
/// Special-purpose districts (transit, stadium…), added together.
pub const DISTRICT: &str = "DISTRICT";

/// The local components, in the order a line lists them after `STATE`.
pub const LOCAL_LEVELS: [&str; 3] = [COUNTY, CITY, DISTRICT];

/// Longest local code: `XX-` + 12 + `/DISTRICT` is 24 characters, the
/// books' tax-class limit.
pub const MAX_LOCAL_LEN: usize = 12;

/// Class prefixes that are never taxed.
pub const EXEMPT_PREFIXES: [&str; 3] = ["EXEMPT", "NONTAXABLE", "RESALE"];

/// A parsed jurisdiction code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Jurisdiction {
    /// USPS state code ("CA").
    pub state: String,
    /// The company's local code ("LA"), if any.
    pub local: Option<String>,
}

impl Jurisdiction {
    /// "CA" or "CA-LA".
    pub fn code(&self) -> String {
        match &self.local {
            Some(l) => format!("{}-{l}", self.state),
            None => self.state.clone(),
        }
    }

    /// The rate key of a component of this jurisdiction (`STATE` is the
    /// state code; the others need a local code).
    pub fn key(&self, level: &str) -> Option<String> {
        if level == STATE {
            return Some(self.state.clone());
        }
        self.local.as_ref().map(|l| format!("{}-{l}/{level}", self.state))
    }
}

fn unknown(code: &str) -> TaxError {
    TaxError::UnknownRegion { kind: "US jurisdiction (state code, or state-local)", code: code.to_owned() }
}

/// Parse "CA" or "CA-LA" (case and surrounding space ignored). The state
/// must be one of the 50 states or DC.
pub fn parse_jurisdiction(code: &str) -> Result<Jurisdiction, TaxError> {
    let upper = code.trim().to_ascii_uppercase();
    let (state, local) = match upper.split_once('-') {
        Some((s, l)) => (s, Some(l)),
        None => (upper.as_str(), None),
    };
    let state = states::state(state).ok_or_else(|| unknown(code))?.code.to_owned();
    if local.is_some_and(|l| l.is_empty() || l.len() > MAX_LOCAL_LEN || !l.chars().all(|c| c.is_ascii_alphanumeric())) {
        return Err(unknown(code));
    }
    Ok(Jurisdiction { state, local: local.map(str::to_owned) })
}

/// The rate on file for exactly `key` on `on` (no prefix fallback).
pub fn exact_rate(rates: &RateSchedule, key: &str, on: NaiveDate) -> Option<Decimal> {
    rates.history(key).iter().filter(|c| c.from <= on).max_by_key(|c| c.from).map(|c| c.percent)
}

fn clean_class(class: &str) -> String {
    class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase()
}

/// Whether a class is exempt everywhere by its name.
pub fn exempt_class(class: &str) -> bool {
    let class = clean_class(class);
    EXEMPT_PREFIXES.iter().any(|p| class.starts_with(p))
}

/// How a line is treated in its jurisdiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Taxability {
    /// Taxed at the combined rate.
    Taxable,
    /// Exempt by its class, by a 0% class override, or an export.
    Exempt,
    /// The company has no rate on file for the state: it does not collect
    /// there.
    NotCollected,
}

/// The components of a jurisdiction's rate for a class on a date, with
/// the line's taxability. Components are (name, percent) in the order
/// `STATE`, `COUNTY`, `CITY`, `DISTRICT`, only those on file.
pub fn rate_components(
    juris: &Jurisdiction,
    class: &str,
    on: NaiveDate,
    rates: &RateSchedule,
) -> (Taxability, Vec<(&'static str, Decimal)>) {
    if exempt_class(class) {
        return (Taxability::Exempt, Vec::new());
    }
    let Some(state_rate) = exact_rate(rates, &juris.state, on) else {
        return (Taxability::NotCollected, Vec::new());
    };
    let class = clean_class(class);
    let override_key = format!("{}:{class}", juris.state);
    let state_rate = match exact_rate(rates, &override_key, on) {
        Some(r) if r.is_zero() => return (Taxability::Exempt, Vec::new()),
        Some(r) => r,
        None => state_rate,
    };
    let mut out = vec![(STATE, state_rate)];
    for level in LOCAL_LEVELS {
        if let Some(r) = juris.key(level).and_then(|k| exact_rate(rates, &k, on)) {
            out.push((level, r));
        }
    }
    (Taxability::Taxable, out)
}

/// Round to the cent, half away from zero.
pub fn round_cents(a: Amount) -> Amount {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn pct(value: Amount, percent: Decimal) -> Result<Amount, TaxError> {
    value.checked_mul(percent).and_then(|a| a.checked_div(Decimal::ONE_HUNDRED)).ok_or(TaxError::Overflow)
}

/// Split the rounded combined tax over the components.
fn split(value: Amount, parts: &[(&'static str, Decimal)]) -> Result<LineTax, TaxError> {
    let combined: Decimal = parts.iter().map(|(_, p)| *p).sum();
    let total = round_cents(pct(value, combined)?);
    let last = parts.iter().rposition(|(_, p)| !p.is_zero());
    let mut left = total;
    let mut components = Vec::with_capacity(parts.len());
    for (i, (tax, percent)) in parts.iter().enumerate() {
        let amount = if Some(i) == last { left } else { round_cents(pct(value, *percent)?) };
        left -= amount;
        components.push(TaxComponent { tax, percent: *percent, amount });
    }
    Ok(LineTax { components })
}

/// Sales tax on one line (see the module notes). The supplier's region
/// must be a known state; `without_payment` is refused, as in every pack;
/// `special_zone` (foreign-trade zones) changes nothing.
pub fn line_tax(line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
    let supply = &line.supply;
    parse_jurisdiction(&supply.supplier_region)?;
    let juris = match &supply.place_of_supply {
        PlaceOfSupply::Abroad => return Ok(LineTax::default()),
        PlaceOfSupply::Region(code) => parse_jurisdiction(code)?,
    };
    if supply.without_payment {
        return Err(TaxError::NotZeroRated);
    }
    let (_, parts) = rate_components(&juris, &line.tax_class, line.date, rates);
    if parts.is_empty() {
        return Ok(LineTax::default());
    }
    split(line.taxable_value, &parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use khata_compliance::Supply;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn rates() -> RateSchedule {
        let mut r = RateSchedule::default();
        r.set("CA", d(2017, 1, 1), dec!(7.25)).unwrap();
        r.set("CA-LA/COUNTY", d(2017, 1, 1), dec!(0.25)).unwrap();
        r.set("CA-LA/CITY", d(2017, 1, 1), dec!(0.75)).unwrap();
        r.set("CA-LA/DISTRICT", d(2017, 1, 1), dec!(1.25)).unwrap();
        r.set("CA:FOOD", d(2017, 1, 1), dec!(0)).unwrap();
        r.set("TX", d(2017, 1, 1), dec!(6.25)).unwrap();
        r.set("TX:REDUCED", d(2017, 1, 1), dec!(3)).unwrap();
        r
    }

    fn line(class: &str, value: Amount, from: &str, to: Option<&str>) -> TaxLine {
        TaxLine {
            date: d(2026, 6, 1),
            tax_class: class.into(),
            taxable_value: value,
            supply: Supply {
                supplier_region: from.into(),
                place_of_supply: to.map_or(PlaceOfSupply::Abroad, |c| PlaceOfSupply::Region(c.into())),
                special_zone: false,
                without_payment: false,
            },
        }
    }

    fn parts(t: &LineTax) -> Vec<(&'static str, Decimal, Amount)> {
        t.components.iter().map(|c| (c.tax, c.percent, c.amount)).collect()
    }

    #[test]
    fn jurisdiction_codes() {
        assert_eq!(parse_jurisdiction(" ca ").unwrap(), Jurisdiction { state: "CA".into(), local: None });
        let la = parse_jurisdiction("ca-la").unwrap();
        assert_eq!((la.code(), la.key(STATE), la.key(CITY)), ("CA-LA".into(), Some("CA".into()), Some("CA-LA/CITY".into())));
        assert_eq!(parse_jurisdiction("CA").unwrap().key(COUNTY), None);
        for bad in ["", "XX", "PR", "CA-", "CA-LA-X", "CA-LOSANGELESCTY", "CA-L A"] {
            assert!(parse_jurisdiction(bad).is_err(), "{bad}");
        }
        assert_eq!(format!("CA-{}/{DISTRICT}", "X".repeat(MAX_LOCAL_LEN)).len(), 24, "keys fit the class limit");
    }

    #[test]
    fn components_per_jurisdiction_add_up() {
        let t = line_tax(&line("TAXABLE", dec!(100), "CA", Some("CA-LA")), &rates()).unwrap();
        assert_eq!(
            parts(&t),
            [(STATE, dec!(7.25), dec!(7.25)), (COUNTY, dec!(0.25), dec!(0.25)), (CITY, dec!(0.75), dec!(0.75)), (DISTRICT, dec!(1.25), dec!(1.25))]
        );
        assert_eq!(t.total(), dec!(9.50));
        let state_only = line_tax(&line("TAXABLE", dec!(100), "CA", Some("CA")), &rates()).unwrap();
        assert_eq!(parts(&state_only), [(STATE, dec!(7.25), dec!(7.25))]);
        // An unknown local code has only the state's component.
        let other = line_tax(&line("TAXABLE", dec!(100), "CA", Some("CA-SF")), &rates()).unwrap();
        assert_eq!(other.total(), dec!(7.25));
    }

    #[test]
    fn rounding_keeps_components_equal_to_the_combined_tax() {
        // 9.5% of 10.05 = 0.95475 → 0.95; parts 0.7286, 0.0251, 0.0754, 0.1256.
        let t = line_tax(&line("TAXABLE", dec!(10.05), "CA", Some("CA-LA")), &rates()).unwrap();
        assert_eq!(t.total(), dec!(0.95));
        assert_eq!(t.components.iter().map(|c| c.amount).collect::<Vec<_>>(), [dec!(0.73), dec!(0.03), dec!(0.08), dec!(0.11)]);
        let credit = line_tax(&line("TAXABLE", dec!(-10.05), "CA", Some("CA-LA")), &rates()).unwrap();
        assert_eq!(credit.total(), dec!(-0.95), "credit notes mirror");
    }

    #[test]
    fn exempt_classes_overrides_and_states_without_rates() {
        let r = rates();
        for class in ["EXEMPT", "nontaxable services", "Resale"] {
            let t = line_tax(&line(class, dec!(100), "CA", Some("CA-LA")), &r).unwrap();
            assert!(t.components.is_empty(), "{class}");
        }
        assert!(line_tax(&line("FOOD", dec!(100), "CA", Some("CA-LA")), &r).unwrap().components.is_empty(), "0% override");
        assert_eq!(line_tax(&line("FOOD", dec!(100), "TX", Some("TX")), &r).unwrap().total(), dec!(6.25), "overrides are per state");
        assert_eq!(parts(&line_tax(&line("reduced", dec!(100), "TX", Some("TX")), &r).unwrap()), [(STATE, dec!(3), dec!(3))]);
        let juris = parse_jurisdiction("NY").unwrap();
        assert_eq!(rate_components(&juris, "TAXABLE", d(2026, 1, 1), &r), (Taxability::NotCollected, vec![]));
        assert!(line_tax(&line("TAXABLE", dec!(100), "CA", Some("NY")), &r).unwrap().components.is_empty());
        let la = parse_jurisdiction("CA-LA").unwrap();
        assert_eq!(rate_components(&la, "FOOD", d(2026, 1, 1), &r).0, Taxability::Exempt);
        assert_eq!(rate_components(&la, "TAXABLE", d(2016, 12, 31), &r).0, Taxability::NotCollected, "before the first rate");
    }

    #[test]
    fn exports_errors_and_dates() {
        let r = rates();
        assert!(line_tax(&line("TAXABLE", dec!(100), "CA", None), &r).unwrap().components.is_empty());
        assert!(matches!(line_tax(&line("TAXABLE", dec!(1), "ZZ", Some("CA")), &r), Err(TaxError::UnknownRegion { .. })));
        let mut l = line("TAXABLE", dec!(1), "CA", Some("CA"));
        l.supply.without_payment = true;
        assert_eq!(line_tax(&l, &r), Err(TaxError::NotZeroRated));
        let mut r2 = rates();
        r2.set("CA", d(2026, 7, 1), dec!(7.5)).unwrap();
        assert_eq!(line_tax(&line("TAXABLE", dec!(100), "CA", Some("CA")), &r2).unwrap().total(), dec!(7.25), "rate in force on the date");
        assert_eq!(line_tax(&line("TAXABLE", Amount::MAX, "CA", Some("CA")), &r), Err(TaxError::Overflow));
    }
}

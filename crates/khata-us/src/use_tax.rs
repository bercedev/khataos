//! Use tax the business owes on its own purchases.
//!
//! Use tax is the complement of sales tax: when a business buys taxable
//! goods for its own use, storage or consumption in its state and the
//! seller charged no sales tax (typically a remote, out-of-state seller
//! not registered in the buyer's state), the buyer owes use tax at the
//! same rate as the sales tax where the goods are used, and reports it on
//! its sales and use tax return (or a consumer use tax return)
//! (`docs/research/us-compliance.md` §11: e.g. CDTFA Publication 123,
//! "the use tax rate is the same as the sales tax rate") [MEDIUM: every
//! state with a sales tax has a use tax; details vary].
//!
//! What this report does, from neutral purchase documents:
//! - a purchase **with any sales tax on it** (a STATE/COUNTY/CITY/DISTRICT
//!   ledger) was taxed by the vendor, or the use tax is already accrued on
//!   it: it is left out;
//! - a purchase from a vendor **in the company's own state** is left out
//!   (that vendor should have charged sales tax; such purchases are
//!   counted and warned about, since the buyer still owes use tax if the
//!   vendor did not charge it);
//! - a purchase from a vendor **outside the state** (another state, or
//!   abroad) with no tax charged is subject to use tax, line by line:
//!   - lines **for resale** (stock items bought for inventory) are exempt:
//!     they will be taxed when sold;
//!   - other lines (supplies, equipment, services ledgers) are taxed at the
//!     company's **own jurisdiction** rate for the line's class, from the
//!     same rate keys as sales ([`crate::sales_tax::rate_components`]):
//!     `EXEMPT…`/`NONTAXABLE…`/`RESALE…` classes and 0% state overrides
//!     are exempt;
//! - a vendor with no state on file is left out with a warning.
//!
//! The result is the liability to self-assess (by component), not posted:
//! the user books it (Dr the expense or asset, Cr a use tax payable
//! ledger) and pays it with the return. Services are taxable for use tax
//! only where the state taxes them, which the class must say.
//!
//! None of this is legal advice: the figures are a preparation aid.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use khata_compliance::RateSchedule;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::returns::{DocKind, Period};
use crate::sales_tax::{self, Jurisdiction, Taxability, round_cents};

/// One purchase line, positive; the document kind gives the sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseLine {
    pub description: String,
    /// The line's tax class (item or ledger); `None`: taxable.
    pub class: Option<String>,
    pub value: Decimal,
    /// Bought for resale (inventory): not subject to use tax.
    pub for_resale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseDoc {
    pub id: String,
    /// `Invoice` for a purchase, `CreditNote` for a purchase return.
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub vendor_name: String,
    /// The vendor's state code (from its address region), if in the US.
    pub vendor_state: Option<String>,
    /// The vendor's country when not the US.
    pub vendor_country: Option<String>,
    pub cancelled: bool,
    /// Sales tax on the purchase (all components booked on it).
    pub tax_charged: Decimal,
    pub lines: Vec<PurchaseLine>,
}

/// One purchase subject to use tax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UseTaxRow {
    pub id: String,
    pub number: String,
    pub date: NaiveDate,
    pub vendor_name: String,
    /// "NY", or the country code for a vendor abroad.
    pub vendor_location: String,
    /// Lines the use tax is worked out on.
    pub taxable_value: Decimal,
    /// Lines for resale, or exempt by class.
    pub exempt_value: Decimal,
    pub use_tax: Decimal,
    /// (component, percent, amount): STATE, COUNTY, CITY, DISTRICT.
    pub components: Vec<(&'static str, Decimal, Decimal)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UseTaxReport {
    pub period: Period,
    /// The company's jurisdiction ("CA-LA").
    pub jurisdiction: String,
    pub rows: Vec<UseTaxRow>,
    pub taxable_value: Decimal,
    pub use_tax: Decimal,
    /// Use tax by component.
    pub by_component: Vec<(&'static str, Decimal)>,
    /// Purchases from out-of-state vendors that charged sales tax.
    pub taxed_by_vendor: usize,
    /// Untaxed purchases from vendors in the company's state.
    pub in_state_untaxed: usize,
    pub warnings: Vec<String>,
}

fn label(kind: DocKind) -> &'static str {
    match kind {
        DocKind::CreditNote => "Purchase return",
        _ => "Purchase",
    }
}

/// Work out the use tax for a period, at the rates of `home` (the
/// company's jurisdiction).
pub fn use_tax_report(docs: &[PurchaseDoc], home: &Jurisdiction, rates: &RateSchedule, period: Period) -> UseTaxReport {
    let mut warnings = Vec::new();
    let mut rows = Vec::new();
    let mut by_component: BTreeMap<usize, (&'static str, Decimal)> = BTreeMap::new();
    let (mut taxed_by_vendor, mut in_state_untaxed) = (0, 0);
    let mut no_rate = false;
    for d in docs.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = format!("{} {}", label(d.kind), d.number);
        let location = match (&d.vendor_country, &d.vendor_state) {
            (Some(c), _) => c.clone(),
            (None, Some(s)) => s.clone(),
            (None, None) => {
                warnings.push(format!("{name}: {} has no state on file; it is left out", d.vendor_name));
                continue;
            }
        };
        let in_state = d.vendor_country.is_none()
            && d.vendor_state.as_deref().and_then(|s| s.split('-').next()).is_some_and(|s| s.trim().eq_ignore_ascii_case(&home.state));
        if !d.tax_charged.is_zero() {
            if !in_state {
                taxed_by_vendor += 1;
            }
            continue;
        }
        if in_state {
            in_state_untaxed += 1;
            continue;
        }
        let sign = d.kind.sign();
        let mut row = UseTaxRow {
            id: d.id.clone(),
            number: d.number.clone(),
            date: d.date,
            vendor_name: d.vendor_name.clone(),
            vendor_location: location,
            taxable_value: Decimal::ZERO,
            exempt_value: Decimal::ZERO,
            use_tax: Decimal::ZERO,
            components: Vec::new(),
        };
        for l in &d.lines {
            let value = l.value * sign;
            if l.for_resale {
                row.exempt_value += value;
                continue;
            }
            let (taxability, parts) = sales_tax::rate_components(home, l.class.as_deref().unwrap_or_default(), d.date, rates);
            match taxability {
                Taxability::Exempt => row.exempt_value += value,
                Taxability::NotCollected => {
                    no_rate = true;
                    row.exempt_value += value;
                }
                Taxability::Taxable => {
                    row.taxable_value += value;
                    let combined: Decimal = parts.iter().map(|p| p.1).sum();
                    let total = round_cents(value * combined / Decimal::ONE_HUNDRED);
                    let last = parts.iter().rposition(|p| !p.1.is_zero());
                    let mut left = total;
                    for (i, (component, pct)) in parts.iter().enumerate() {
                        let amount = if Some(i) == last { left } else { round_cents(value * pct / Decimal::ONE_HUNDRED) };
                        left -= amount;
                        match row.components.iter_mut().find(|c| c.0 == *component) {
                            Some(c) => c.2 += amount,
                            None => row.components.push((component, *pct, amount)),
                        }
                        let level = sales_tax::LOCAL_LEVELS.iter().position(|l| l == component).map_or(0, |p| p + 1);
                        by_component.entry(level).or_insert((component, Decimal::ZERO)).1 += amount;
                    }
                    row.use_tax += total;
                }
            }
        }
        if row.taxable_value.is_zero() && row.use_tax.is_zero() && row.exempt_value.is_zero() {
            continue;
        }
        rows.push(row);
    }
    if no_rate {
        warnings.push(format!("the company has no {} rate on file for the use date; those lines are left out", home.state));
    }
    if in_state_untaxed > 0 {
        warnings.push(format!(
            "{in_state_untaxed} purchase(s) from vendors in {} carry no sales tax; if the vendor should have charged it and did not, use tax is owed on them too",
            home.state
        ));
    }
    let taxable_value = rows.iter().map(|r| r.taxable_value).sum();
    let use_tax = rows.iter().map(|r| r.use_tax).sum();
    UseTaxReport {
        period,
        jurisdiction: home.code(),
        rows,
        taxable_value,
        use_tax,
        by_component: by_component.into_values().collect(),
        taxed_by_vendor,
        in_state_untaxed,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sales_tax::parse_jurisdiction;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn rates() -> RateSchedule {
        let mut r = RateSchedule::default();
        r.set("CA", d(2017, 1, 1), dec!(7.25)).unwrap();
        r.set("CA-LA/COUNTY", d(2017, 1, 1), dec!(0.25)).unwrap();
        r.set("CA-LA/DISTRICT", d(2017, 1, 1), dec!(2)).unwrap();
        r.set("CA:FOOD", d(2017, 1, 1), dec!(0)).unwrap();
        r
    }

    fn line(class: Option<&str>, value: Decimal, for_resale: bool) -> PurchaseLine {
        PurchaseLine { description: "x".into(), class: class.map(str::to_owned), value, for_resale }
    }

    fn doc(number: &str, state: Option<&str>, country: Option<&str>, tax: Decimal, lines: Vec<PurchaseLine>) -> PurchaseDoc {
        PurchaseDoc {
            id: number.into(),
            kind: DocKind::Invoice,
            number: number.into(),
            date: d(2026, 5, 4),
            vendor_name: format!("Vendor {number}"),
            vendor_state: state.map(str::to_owned),
            vendor_country: country.map(str::to_owned),
            cancelled: false,
            tax_charged: tax,
            lines,
        }
    }

    #[test]
    fn untaxed_out_of_state_purchases_owe_use_tax_at_home_rates() {
        let home = parse_jurisdiction("CA-LA").unwrap();
        let q = Period { from: d(2026, 4, 1), to: d(2026, 6, 30) };
        let mut ret = doc("R1", Some("OR"), None, dec!(0), vec![line(None, dec!(100), false)]);
        ret.kind = DocKind::CreditNote;
        let docs = vec![
            // Office equipment from Oregon (no sales tax there): 9.5% use tax.
            doc("P1", Some("OR"), None, dec!(0), vec![line(None, dec!(1000), false), line(None, dec!(500), true), line(Some("FOOD"), dec!(40), false)]),
            // A New York vendor that charged tax: nothing owed.
            doc("P2", Some("NY"), None, dec!(8), vec![line(None, dec!(100), false)]),
            // An in-state vendor without tax: warned about, not counted.
            doc("P3", Some("CA-SF"), None, dec!(0), vec![line(None, dec!(100), false)]),
            // An import from Canada.
            doc("P4", None, Some("CA"), dec!(0), vec![line(Some("TAXABLE"), dec!(200), false)]),
            doc("P5", None, None, dec!(0), vec![line(None, dec!(1), false)]),
            ret,
        ];
        let r = use_tax_report(&docs, &home, &rates(), q);
        let rows: Vec<_> = r.rows.iter().map(|x| (x.number.as_str(), x.vendor_location.as_str(), x.taxable_value, x.exempt_value, x.use_tax)).collect();
        assert_eq!(rows, [
            ("P1", "OR", dec!(1000), dec!(540), dec!(95)),
            ("P4", "CA", dec!(200), dec!(0), dec!(19)),
            ("R1", "OR", dec!(-100), dec!(0), dec!(-9.5)),
        ]);
        assert_eq!(r.rows[0].components, [("STATE", dec!(7.25), dec!(72.5)), ("COUNTY", dec!(0.25), dec!(2.5)), ("DISTRICT", dec!(2), dec!(20))]);
        assert_eq!((r.taxable_value, r.use_tax, r.jurisdiction.as_str()), (dec!(1100), dec!(104.5), "CA-LA"));
        assert_eq!(r.by_component, [("STATE", dec!(79.75)), ("COUNTY", dec!(2.75)), ("DISTRICT", dec!(22))]);
        assert_eq!((r.taxed_by_vendor, r.in_state_untaxed), (1, 1));
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
    }

    #[test]
    fn a_company_without_a_home_rate_is_warned() {
        let home = parse_jurisdiction("NY").unwrap();
        let q = Period { from: d(2026, 4, 1), to: d(2026, 6, 30) };
        let r = use_tax_report(&[doc("P1", Some("OR"), None, dec!(0), vec![line(None, dec!(10), false)])], &home, &rates(), q);
        assert_eq!(r.use_tax, dec!(0));
        assert!(r.warnings[0].contains("no NY rate"));
    }
}

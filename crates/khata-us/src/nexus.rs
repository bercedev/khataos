//! Economic nexus thresholds, for the states KhataOS has a sourced figure
//! for.
//!
//! Since *South Dakota v. Wayfair, Inc.* (2018) a state can require a
//! seller with no physical presence to collect its sales tax once the
//! seller's sales into the state pass a threshold the state sets
//! (`docs/research/us-compliance.md` §2). Every state measures it its own
//! way (what sales count, over which period). KhataOS lists only the
//! states whose rule it has from the state's own revenue department, and
//! the check here is an **early warning**, not a determination: it
//! compares the calendar year to date with the threshold, whatever the
//! state's real measuring period ([`Threshold::measured`] says what it is).

use rust_decimal::Decimal;
use serde::Serialize;

/// When this table was last checked.
pub const NEXUS_VERSION: &str = "2026-10-20";

/// A state's economic nexus rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Threshold {
    pub state: &'static str,
    /// Sales (as the state counts them) at which nexus starts, in USD.
    pub sales: u64,
    /// A number of transactions that must also be passed, when the state
    /// has one (New York: both tests).
    pub transactions_and: Option<u64>,
    /// "exceeds" (strictly more than) or "reaches".
    pub strictly_more: bool,
    /// What counts and over which period, in the state's words.
    pub measured: &'static str,
    pub source: &'static str,
}

/// The sourced thresholds (see the research document for each source).
pub const THRESHOLDS: &[Threshold] = &[
    Threshold {
        state: "CA",
        sales: 500_000,
        transactions_and: None,
        strictly_more: true,
        measured: "total combined sales of tangible personal property delivered in California, preceding or current calendar year",
        source: "https://www.cdtfa.ca.gov/industry/wayfair.htm",
    },
    Threshold {
        state: "FL",
        sales: 100_000,
        transactions_and: None,
        strictly_more: true,
        measured: "taxable remote sales into Florida in the previous calendar year",
        source: "https://floridarevenue.com/taxes/tips/Documents/TIP_21A01-03.pdf",
    },
    Threshold {
        state: "NY",
        sales: 500_000,
        transactions_and: Some(100),
        strictly_more: true,
        measured: "gross receipts from sales of tangible personal property delivered into New York AND more than 100 such sales, in the immediately preceding four sales tax quarters",
        source: "https://www.tax.ny.gov/pubs_and_bulls/publications/sales/nexus.htm",
    },
    Threshold {
        state: "TX",
        sales: 500_000,
        transactions_and: None,
        strictly_more: false,
        measured: "total Texas revenue in the preceding twelve calendar months ($500,000 or more)",
        source: "https://comptroller.texas.gov/taxes/sales/remote-sellers.php",
    },
    Threshold {
        state: "WA",
        sales: 100_000,
        transactions_and: None,
        strictly_more: true,
        measured: "cumulative gross receipts from Washington sales in the current or prior calendar year",
        source: "https://dor.wa.gov/taxes-rates/retail-sales-tax/marketplace-fairness-leveling-playing-field/remote-sellers",
    },
];

/// The threshold for a state, if KhataOS has one.
pub fn threshold(state: &str) -> Option<&'static Threshold> {
    THRESHOLDS.iter().find(|t| t.state.eq_ignore_ascii_case(state.trim()))
}

/// The result of comparing sales with a state's threshold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NexusCheck {
    pub threshold: Threshold,
    /// Sales counted (calendar year to date, net of credit notes).
    pub sales: Decimal,
    /// Invoices counted.
    pub transactions: u64,
    /// Whether the counted figures pass the threshold.
    pub met: bool,
}

/// Compare year-to-date sales and invoice count with a state's threshold;
/// `None` when KhataOS has no sourced threshold for the state.
pub fn check(state: &str, sales: Decimal, transactions: u64) -> Option<NexusCheck> {
    let t = *threshold(state)?;
    let limit = Decimal::from(t.sales);
    let sales_ok = if t.strictly_more { sales > limit } else { sales >= limit };
    let count_ok = t.transactions_and.is_none_or(|n| transactions > n);
    Some(NexusCheck { threshold: t, sales, transactions, met: sales_ok && count_ok })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn thresholds_and_checks() {
        assert_eq!(THRESHOLDS.len(), 5);
        assert!(check("OH", dec!(1_000_000), 1000).is_none(), "no sourced figure");
        assert!(!check("CA", dec!(500_000), 1).unwrap().met, "California: more than $500,000");
        assert!(check("ca", dec!(500_000.01), 1).unwrap().met);
        assert!(check("TX", dec!(500_000), 1).unwrap().met, "Texas: $500,000 or more");
        assert!(!check("NY", dec!(600_000), 100).unwrap().met, "New York needs both tests");
        assert!(check("NY", dec!(600_000), 101).unwrap().met);
        assert!(check("FL", dec!(100_001), 1).unwrap().met && !check("WA", dec!(100_000), 1).unwrap().met);
    }
}

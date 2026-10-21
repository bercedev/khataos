//! The 50 states and the District of Columbia, by USPS code.
//!
//! The codes are the two-letter USPS state abbreviations (USPS Publication
//! 28, Appendix B), which are also the ISO 3166-2:US subdivision codes
//! without the "US-" prefix. Territories (PR, GU, VI, AS, MP) are not
//! regions here: they have their own tax systems, not state sales tax.
//!
//! `statewide_sales_tax` is false for the five states with no statewide
//! sales tax (Alaska, Delaware, Montana, New Hampshire and Oregon;
//! `docs/research/us-compliance.md` §1). Alaska allows local sales taxes,
//! so a company can still define local jurisdictions there.

/// When this table was last checked.
pub const STATES_VERSION: &str = "2026-10-20";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// USPS code ("CA").
    pub code: &'static str,
    pub name: &'static str,
    /// The state levies a statewide sales tax.
    pub statewide_sales_tax: bool,
}

const fn s(code: &'static str, name: &'static str) -> State {
    State { code, name, statewide_sales_tax: true }
}

const fn none(code: &'static str, name: &'static str) -> State {
    State { code, name, statewide_sales_tax: false }
}

/// The 50 states and DC in alphabetical order of name.
pub const STATES: &[State] = &[
    s("AL", "Alabama"),
    none("AK", "Alaska"),
    s("AZ", "Arizona"),
    s("AR", "Arkansas"),
    s("CA", "California"),
    s("CO", "Colorado"),
    s("CT", "Connecticut"),
    none("DE", "Delaware"),
    s("DC", "District of Columbia"),
    s("FL", "Florida"),
    s("GA", "Georgia"),
    s("HI", "Hawaii"),
    s("ID", "Idaho"),
    s("IL", "Illinois"),
    s("IN", "Indiana"),
    s("IA", "Iowa"),
    s("KS", "Kansas"),
    s("KY", "Kentucky"),
    s("LA", "Louisiana"),
    s("ME", "Maine"),
    s("MD", "Maryland"),
    s("MA", "Massachusetts"),
    s("MI", "Michigan"),
    s("MN", "Minnesota"),
    s("MS", "Mississippi"),
    s("MO", "Missouri"),
    none("MT", "Montana"),
    s("NE", "Nebraska"),
    s("NV", "Nevada"),
    none("NH", "New Hampshire"),
    s("NJ", "New Jersey"),
    s("NM", "New Mexico"),
    s("NY", "New York"),
    s("NC", "North Carolina"),
    s("ND", "North Dakota"),
    s("OH", "Ohio"),
    s("OK", "Oklahoma"),
    none("OR", "Oregon"),
    s("PA", "Pennsylvania"),
    s("RI", "Rhode Island"),
    s("SC", "South Carolina"),
    s("SD", "South Dakota"),
    s("TN", "Tennessee"),
    s("TX", "Texas"),
    s("UT", "Utah"),
    s("VT", "Vermont"),
    s("VA", "Virginia"),
    s("WA", "Washington"),
    s("WV", "West Virginia"),
    s("WI", "Wisconsin"),
    s("WY", "Wyoming"),
];

/// Look up a state by its USPS code, ignoring case and surrounding space.
pub fn state(code: &str) -> Option<&'static State> {
    let code = code.trim();
    STATES.iter().find(|s| s.code.eq_ignore_ascii_case(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn fifty_states_and_dc_with_unique_codes() {
        assert_eq!(STATES.len(), 51);
        let codes: BTreeSet<_> = STATES.iter().map(|s| s.code).collect();
        assert_eq!(codes.len(), 51);
        assert!(STATES.iter().all(|s| s.code.len() == 2 && s.code.chars().all(|c| c.is_ascii_uppercase())));
        assert_eq!(state(" ca ").map(|s| s.name), Some("California"));
        assert_eq!(state("DC").map(|s| s.name), Some("District of Columbia"));
        assert_eq!(state("PR"), None, "territories are not states");
    }

    #[test]
    fn five_states_have_no_statewide_sales_tax() {
        let without: Vec<_> = STATES.iter().filter(|s| !s.statewide_sales_tax).map(|s| s.code).collect();
        assert_eq!(without, ["AK", "DE", "MT", "NH", "OR"]);
    }
}

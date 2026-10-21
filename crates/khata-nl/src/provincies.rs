//! The 12 provinces as address regions, and the EU member states.
//!
//! Btw rates are the same in every province; the province is only part of
//! the address. The codes are the ISO 3166-2:NL subdivision codes without
//! the "NL-" prefix (`docs/research/netherlands-compliance.md` §1.6). The
//! Caribbean Netherlands (Bonaire, Sint Eustatius, Saba) are outside the EU
//! VAT area (they have the ABB, algemene bestedingsbelasting) and are not
//! regions here: a party there is abroad.

/// When these tables were last checked.
pub const PROVINCIES_VERSION: &str = "2026-10-21";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provincie {
    /// KhataOS's code ("NH").
    pub code: &'static str,
    pub name: &'static str,
    /// ISO 3166-2 code ("NL-NH").
    pub iso: &'static str,
}

const fn p(code: &'static str, name: &'static str, iso: &'static str) -> Provincie {
    Provincie { code, name, iso }
}

/// In ISO 3166-2:NL code order.
pub const PROVINCIES: &[Provincie] = &[
    p("DR", "Drenthe", "NL-DR"),
    p("FL", "Flevoland", "NL-FL"),
    p("FR", "Fryslân", "NL-FR"),
    p("GE", "Gelderland", "NL-GE"),
    p("GR", "Groningen", "NL-GR"),
    p("LI", "Limburg", "NL-LI"),
    p("NB", "Noord-Brabant", "NL-NB"),
    p("NH", "Noord-Holland", "NL-NH"),
    p("OV", "Overijssel", "NL-OV"),
    p("UT", "Utrecht", "NL-UT"),
    p("ZE", "Zeeland", "NL-ZE"),
    p("ZH", "Zuid-Holland", "NL-ZH"),
];

/// Look up a province by its code, ignoring case, surrounding space and an
/// "NL-" prefix.
pub fn provincie(code: &str) -> Option<&'static Provincie> {
    let code = code.trim();
    let code = code.strip_prefix("NL-").or_else(|| code.strip_prefix("nl-")).unwrap_or(code);
    PROVINCIES.iter().find(|p| p.code.eq_ignore_ascii_case(code))
}

/// The 27 EU member states by ISO 3166-1 alpha-2 code (Greece is "GR"
/// here, "EL" in VAT identifiers).
pub const EU_MEMBER_STATES: &[&str] = &[
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV", "LT", "LU",
    "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
];

/// Whether an ISO country code (or the VAT prefix "EL") is an EU member state.
pub fn in_eu(country: &str) -> bool {
    let c = country.trim().to_ascii_uppercase();
    c == "EL" || EU_MEMBER_STATES.contains(&c.as_str())
}

/// Whether a country code is the Netherlands'.
pub fn is_netherlands(country: &str) -> bool {
    country.trim().eq_ignore_ascii_case("NL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_provinces_with_iso_codes() {
        assert_eq!(PROVINCIES.len(), 12);
        for x in PROVINCIES {
            assert_eq!(x.iso, format!("NL-{}", x.code));
        }
        assert_eq!(provincie(" nh ").map(|p| p.name), Some("Noord-Holland"));
        assert_eq!(provincie("NL-ZH").map(|p| p.name), Some("Zuid-Holland"));
        assert_eq!(provincie("BQ"), None, "the Caribbean Netherlands are not a province");
    }

    #[test]
    fn eu_membership() {
        assert_eq!(EU_MEMBER_STATES.len(), 27);
        assert!(in_eu("de") && in_eu("EL") && in_eu("NL"));
        assert!(!in_eu("BQ") && !in_eu("GB") && !in_eu("CH") && !in_eu(""));
        assert!(is_netherlands(" nl ") && !is_netherlands("BE"));
    }
}

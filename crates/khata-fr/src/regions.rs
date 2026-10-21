//! The 18 régions as address regions, where they change the TVA, and the
//! EU member states.
//!
//! The régions are the 13 of metropolitan France (Corse included) and the
//! five overseas régions (régions d'outre-mer), with their ISO 3166-2:FR
//! codes (`docs/research/france-compliance.md` §8.2). They matter for TVA
//! because the CGI treats them differently:
//! - **metropolitan France**, Corse included: the ordinary rates (Corse also
//!   has its own reduced rates, art. 297 CGI, which a company sets up as
//!   classes of its own);
//! - **Guadeloupe, Martinique and La Réunion**: TVA applies at the DOM rates
//!   of art. 296 CGI (8.5% and 2.1%);
//! - **Guyane and Mayotte**: TVA is "provisoirement" not applicable
//!   (art. 294-1 CGI).
//!
//! Supplies between metropolitan France and the overseas régions are
//! treated as exports and imports (art. 294-2 CGI).

/// When these tables were last checked.
pub const REGIONS_VERSION: &str = "2026-10-21";

/// How TVA applies in a région.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Territory {
    /// Metropolitan France (Corse included): the ordinary rates.
    Metropole,
    /// Guadeloupe, Martinique, La Réunion: the DOM rates (art. 296 CGI).
    Dom,
    /// Guyane, Mayotte: no TVA (art. 294-1 CGI).
    NoTva,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// KhataOS's code: the ISO 3166-2 code without "FR-" ("IDF", "971").
    pub code: &'static str,
    pub name: &'static str,
    /// ISO 3166-2 code ("FR-IDF").
    pub iso: &'static str,
    pub territory: Territory,
}

const fn r(code: &'static str, name: &'static str, iso: &'static str, territory: Territory) -> Region {
    Region { code, name, iso, territory }
}

use Territory::{Dom, Metropole, NoTva};

/// Metropolitan régions in ISO code order, then the overseas régions by
/// their department number.
pub const REGIONS: &[Region] = &[
    r("ARA", "Auvergne-Rhône-Alpes", "FR-ARA", Metropole),
    r("BFC", "Bourgogne-Franche-Comté", "FR-BFC", Metropole),
    r("BRE", "Bretagne", "FR-BRE", Metropole),
    r("CVL", "Centre-Val de Loire", "FR-CVL", Metropole),
    r("20R", "Corse", "FR-20R", Metropole),
    r("GES", "Grand Est", "FR-GES", Metropole),
    r("HDF", "Hauts-de-France", "FR-HDF", Metropole),
    r("IDF", "Île-de-France", "FR-IDF", Metropole),
    r("NOR", "Normandie", "FR-NOR", Metropole),
    r("NAQ", "Nouvelle-Aquitaine", "FR-NAQ", Metropole),
    r("OCC", "Occitanie", "FR-OCC", Metropole),
    r("PDL", "Pays de la Loire", "FR-PDL", Metropole),
    r("PAC", "Provence-Alpes-Côte d'Azur", "FR-PAC", Metropole),
    r("971", "Guadeloupe", "FR-971", Dom),
    r("972", "Martinique", "FR-972", Dom),
    r("973", "Guyane", "FR-973", NoTva),
    r("974", "La Réunion", "FR-974", Dom),
    r("976", "Mayotte", "FR-976", NoTva),
];

/// Look up a région by its code, ignoring case, surrounding space and an
/// "FR-" prefix.
pub fn region(code: &str) -> Option<&'static Region> {
    let code = code.trim();
    let code = code.strip_prefix("FR-").or_else(|| code.strip_prefix("fr-")).unwrap_or(code);
    REGIONS.iter().find(|r| r.code.eq_ignore_ascii_case(code))
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

/// Whether a country code is France's. Monaco ("MC") is treated as France
/// for TVA (convention of 18 May 1963 [SECONDARY]) and counts too.
pub fn is_france(country: &str) -> bool {
    let c = country.trim();
    c.eq_ignore_ascii_case("FR") || c.eq_ignore_ascii_case("MC")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eighteen_regions_with_iso_codes() {
        assert_eq!(REGIONS.len(), 18);
        for x in REGIONS {
            assert_eq!(x.iso, format!("FR-{}", x.code));
        }
        assert_eq!(REGIONS.iter().filter(|r| r.territory == Metropole).count(), 13);
        assert_eq!(region(" idf ").map(|r| r.name), Some("Île-de-France"));
        assert_eq!(region("FR-974").map(|r| r.territory), Some(Dom));
        assert_eq!(region("976").map(|r| r.territory), Some(NoTva));
        assert_eq!(region("75"), None, "départements are not régions");
    }

    #[test]
    fn eu_membership() {
        assert_eq!(EU_MEMBER_STATES.len(), 27);
        assert!(in_eu("de") && in_eu("EL") && in_eu("FR"));
        assert!(!in_eu("CH") && !in_eu("GB") && !in_eu("MC") && !in_eu(""));
        assert!(is_france(" fr ") && is_france("MC") && !is_france("BE"));
    }
}

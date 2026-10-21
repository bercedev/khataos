//! The 16 Bundesländer (federal states) as address regions, and the EU
//! member states.
//!
//! USt rates are the same in every state; the state matters for the
//! Steuernummer (its format and its prefix in the 13-digit ELSTER form,
//! see [`crate::steuernummer`]) and for which Finanzamt the company files
//! with. The codes are the ISO 3166-2:DE subdivision codes without the
//! "DE-" prefix (`docs/research/germany-compliance.md` §1.4).

/// When these tables were last checked.
pub const LAENDER_VERSION: &str = "2026-10-20";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Land {
    /// KhataOS's code ("BY").
    pub code: &'static str,
    pub name: &'static str,
    /// ISO 3166-2 code ("DE-BY").
    pub iso: &'static str,
}

const fn l(code: &'static str, name: &'static str, iso: &'static str) -> Land {
    Land { code, name, iso }
}

/// In ISO 3166-2:DE code order.
pub const LAENDER: &[Land] = &[
    l("BB", "Brandenburg", "DE-BB"),
    l("BE", "Berlin", "DE-BE"),
    l("BW", "Baden-Württemberg", "DE-BW"),
    l("BY", "Bayern", "DE-BY"),
    l("HB", "Bremen", "DE-HB"),
    l("HE", "Hessen", "DE-HE"),
    l("HH", "Hamburg", "DE-HH"),
    l("MV", "Mecklenburg-Vorpommern", "DE-MV"),
    l("NI", "Niedersachsen", "DE-NI"),
    l("NW", "Nordrhein-Westfalen", "DE-NW"),
    l("RP", "Rheinland-Pfalz", "DE-RP"),
    l("SH", "Schleswig-Holstein", "DE-SH"),
    l("SL", "Saarland", "DE-SL"),
    l("SN", "Sachsen", "DE-SN"),
    l("ST", "Sachsen-Anhalt", "DE-ST"),
    l("TH", "Thüringen", "DE-TH"),
];

/// Look up a state by its code, ignoring case, surrounding space and a
/// "DE-" prefix.
pub fn land(code: &str) -> Option<&'static Land> {
    let code = code.trim();
    let code = code.strip_prefix("DE-").or_else(|| code.strip_prefix("de-")).unwrap_or(code);
    LAENDER.iter().find(|l| l.code.eq_ignore_ascii_case(code))
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

/// Whether a country code is Germany's.
pub fn is_germany(country: &str) -> bool {
    country.trim().eq_ignore_ascii_case("DE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_states_with_iso_codes() {
        assert_eq!(LAENDER.len(), 16);
        for x in LAENDER {
            assert_eq!(x.iso, format!("DE-{}", x.code));
        }
        let mut codes: Vec<_> = LAENDER.iter().map(|l| l.code).collect();
        codes.dedup();
        assert_eq!(codes.len(), 16);
        assert_eq!(land(" by ").map(|l| l.name), Some("Bayern"));
        assert_eq!(land("DE-NW").map(|l| l.name), Some("Nordrhein-Westfalen"));
        assert_eq!(land("XX"), None);
    }

    #[test]
    fn eu_membership() {
        assert_eq!(EU_MEMBER_STATES.len(), 27);
        assert!(in_eu("fr") && in_eu("EL") && in_eu("DE"));
        assert!(!in_eu("CH") && !in_eu("GB") && !in_eu("NO") && !in_eu(""));
        assert!(is_germany(" de ") && !is_germany("AT"));
    }
}

//! The four nations of the United Kingdom, and the EU member states.
//!
//! VAT rates are the same in every nation; the nations are address regions.
//! Northern Ireland matters for goods: under the Windsor Framework it
//! follows EU VAT rules for goods, so goods moved between Northern Ireland
//! and the EU go in VAT return Boxes 2, 8 and 9 (VAT Notice 700/12 §3.3,
//! §3.9, §3.10; `docs/research/uk-compliance.md` §3 [PRIMARY]).
//!
//! The codes are the ISO 3166-2:GB country codes without the "GB-" prefix
//! [SECONDARY].

/// When these tables were last checked.
pub const NATIONS_VERSION: &str = "2026-10-20";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nation {
    /// KhataOS's code ("NIR").
    pub code: &'static str,
    pub name: &'static str,
    /// ISO 3166-2 code ("GB-NIR").
    pub iso: &'static str,
}

const fn n(code: &'static str, name: &'static str, iso: &'static str) -> Nation {
    Nation { code, name, iso }
}

pub const NATIONS: &[Nation] = &[
    n("ENG", "England", "GB-ENG"),
    n("SCT", "Scotland", "GB-SCT"),
    n("WLS", "Wales", "GB-WLS"),
    n("NIR", "Northern Ireland", "GB-NIR"),
];

/// Northern Ireland's code.
pub const NORTHERN_IRELAND: &str = "NIR";

/// Look up a nation by its code, ignoring case and surrounding space.
pub fn nation(code: &str) -> Option<&'static Nation> {
    let code = code.trim();
    NATIONS.iter().find(|n| n.code.eq_ignore_ascii_case(code))
}

/// Whether a code is Northern Ireland's.
pub fn is_northern_ireland(code: &str) -> bool {
    code.trim().eq_ignore_ascii_case(NORTHERN_IRELAND)
}

/// The 27 EU member states by ISO 3166-1 alpha-2 code (Greece is "GR"
/// here, "EL" in VAT numbers), for telling goods moved between Northern
/// Ireland and the EU (Boxes 8 and 9) from other trade abroad.
pub const EU_MEMBER_STATES: &[&str] = &[
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV", "LT", "LU",
    "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
];

/// Whether an ISO country code (or the VAT code "EL") is an EU member state.
pub fn in_eu(country: &str) -> bool {
    let c = country.trim().to_ascii_uppercase();
    c == "EL" || EU_MEMBER_STATES.contains(&c.as_str())
}

/// Countries inside the UK VAT area: the UK itself and the Isle of Man
/// (VAT Notice 700 §3.2 [PRIMARY]). A party there is not abroad.
pub fn in_uk_vat_area(country: &str) -> bool {
    matches!(country.trim().to_ascii_uppercase().as_str(), "GB" | "UK" | "IM")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_nations_with_iso_codes() {
        let codes: Vec<_> = NATIONS.iter().map(|n| n.code).collect();
        assert_eq!(codes, ["ENG", "SCT", "WLS", "NIR"]);
        for x in NATIONS {
            assert_eq!(x.iso, format!("GB-{}", x.code));
        }
        assert_eq!(nation(" nir ").map(|n| n.name), Some("Northern Ireland"));
        assert_eq!(nation("GB-NIR"), None, "ISO codes are not the stored codes");
        assert!(is_northern_ireland("nir") && !is_northern_ireland("ENG"));
    }

    #[test]
    fn eu_has_27_members_and_the_uk_is_not_one() {
        assert_eq!(EU_MEMBER_STATES.len(), 27);
        assert!(in_eu("ie") && in_eu("DE") && in_eu("EL") && in_eu("GR"));
        assert!(!in_eu("GB") && !in_eu("XI") && !in_eu("CH") && !in_eu("NO") && !in_eu(""));
        assert!(in_uk_vat_area("gb") && in_uk_vat_area("IM") && !in_uk_vat_area("IE") && !in_uk_vat_area("JE"));
    }
}

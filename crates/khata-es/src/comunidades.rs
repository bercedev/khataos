//! The 17 autonomous communities and the autonomous cities of Ceuta and
//! Melilla as address regions, where IVA applies, and the EU member
//! states.
//!
//! Codes are the ISO 3166-2:ES autonomous community codes without the
//! "ES-" prefix (`docs/research/spain-compliance.md` §1.5). IVA rates are the
//! same everywhere it applies, but the IVA territory (Ley 37/1992 art. 3)
//! leaves out:
//! - **Canarias**: the IGIC (Impuesto General Indirecto Canario) applies
//!   instead; out of scope;
//! - **Ceuta and Melilla**: the IPSI applies instead; out of scope.
//!
//! The País Vasco and Navarra apply the common IVA rates but their own
//! haciendas forales collect it (Concierto / Convenio Económico), and the
//! Basque provinces require TicketBAI (Batuz in Bizkaia) instead of
//! VeriFactu: see the research.

/// When these tables were last checked.
pub const COMUNIDADES_VERSION: &str = "2026-10-21";

/// Which indirect tax applies in a community.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Territory {
    /// The IVA territory, collected by the AEAT.
    Iva,
    /// The IVA territory, collected by a hacienda foral (País Vasco, Navarra).
    IvaForal,
    /// Canarias: IGIC, not IVA.
    Igic,
    /// Ceuta and Melilla: IPSI, not IVA.
    Ipsi,
}

impl Territory {
    /// IVA applies to supplies made there.
    pub fn iva(self) -> bool {
        matches!(self, Territory::Iva | Territory::IvaForal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comunidad {
    /// KhataOS's code ("MD").
    pub code: &'static str,
    pub name: &'static str,
    /// ISO 3166-2 code ("ES-MD").
    pub iso: &'static str,
    pub territory: Territory,
}

const fn c(code: &'static str, name: &'static str, iso: &'static str, territory: Territory) -> Comunidad {
    Comunidad { code, name, iso, territory }
}

use Territory::{Igic, Ipsi, Iva, IvaForal};

/// In ISO 3166-2:ES code order, then Ceuta and Melilla.
pub const COMUNIDADES: &[Comunidad] = &[
    c("AN", "Andalucía", "ES-AN", Iva),
    c("AR", "Aragón", "ES-AR", Iva),
    c("AS", "Asturias, Principado de", "ES-AS", Iva),
    c("CB", "Cantabria", "ES-CB", Iva),
    c("CL", "Castilla y León", "ES-CL", Iva),
    c("CM", "Castilla-La Mancha", "ES-CM", Iva),
    c("CN", "Canarias", "ES-CN", Igic),
    c("CT", "Catalunya", "ES-CT", Iva),
    c("EX", "Extremadura", "ES-EX", Iva),
    c("GA", "Galicia", "ES-GA", Iva),
    c("IB", "Illes Balears", "ES-IB", Iva),
    c("MC", "Murcia, Región de", "ES-MC", Iva),
    c("MD", "Madrid, Comunidad de", "ES-MD", Iva),
    c("NC", "Navarra, Comunidad Foral de", "ES-NC", IvaForal),
    c("PV", "País Vasco", "ES-PV", IvaForal),
    c("RI", "La Rioja", "ES-RI", Iva),
    c("VC", "Valenciana, Comunitat", "ES-VC", Iva),
    c("CE", "Ceuta", "ES-CE", Ipsi),
    c("ML", "Melilla", "ES-ML", Ipsi),
];

/// Look up a community by its code, ignoring case, surrounding space and
/// an "ES-" prefix.
pub fn comunidad(code: &str) -> Option<&'static Comunidad> {
    let code = code.trim();
    let code = code.strip_prefix("ES-").or_else(|| code.strip_prefix("es-")).unwrap_or(code);
    COMUNIDADES.iter().find(|c| c.code.eq_ignore_ascii_case(code))
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

/// Whether a country code is Spain's.
pub fn is_spain(country: &str) -> bool {
    country.trim().eq_ignore_ascii_case("ES")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nineteen_regions_with_iso_codes() {
        assert_eq!(COMUNIDADES.len(), 19);
        for x in COMUNIDADES {
            assert_eq!(x.iso, format!("ES-{}", x.code));
        }
        assert_eq!(COMUNIDADES.iter().filter(|c| c.territory.iva()).count(), 16);
        assert_eq!(comunidad(" md ").map(|c| c.name), Some("Madrid, Comunidad de"));
        assert_eq!(comunidad("ES-CN").map(|c| c.territory), Some(Igic));
        assert_eq!(comunidad("M"), None, "provinces are not communities");
    }

    #[test]
    fn eu_membership() {
        assert_eq!(EU_MEMBER_STATES.len(), 27);
        assert!(in_eu("pt") && in_eu("EL") && in_eu("ES"));
        assert!(!in_eu("AD") && !in_eu("GB") && !in_eu("GI") && !in_eu(""));
        assert!(is_spain(" es ") && !is_spain("PT"));
    }
}

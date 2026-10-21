//! The 32 federal entities of Mexico (31 states and Mexico City), by
//! ISO 3166-2:MX code without the "MX-" prefix.
//!
//! Mexico City's code is `CMX` (it replaced `DIF`, "Distrito Federal",
//! after the 2016 reform; ISO 3166-2 newsletter) [MEDIUM:
//! `docs/research/mexico-compliance.md` §9]. IVA is federal: the entity
//! never changes the rate. The 8% border-region rate depends on the
//! municipality and the taxpayer's registration under the decree, not on
//! the state, so it is a tax class ([`crate::iva::BORDER_CLASS`]), not a
//! property of a state.

/// When this table was last checked.
pub const STATES_VERSION: &str = "2026-10-20";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entity {
    /// ISO 3166-2:MX code without "MX-" ("JAL").
    pub code: &'static str,
    pub name: &'static str,
}

const fn e(code: &'static str, name: &'static str) -> Entity {
    Entity { code, name }
}

/// The 32 entities in alphabetical order of code.
pub const ENTITIES: &[Entity] = &[
    e("AGU", "Aguascalientes"),
    e("BCN", "Baja California"),
    e("BCS", "Baja California Sur"),
    e("CAM", "Campeche"),
    e("CHH", "Chihuahua"),
    e("CHP", "Chiapas"),
    e("CMX", "Ciudad de México"),
    e("COA", "Coahuila de Zaragoza"),
    e("COL", "Colima"),
    e("DUR", "Durango"),
    e("GRO", "Guerrero"),
    e("GUA", "Guanajuato"),
    e("HID", "Hidalgo"),
    e("JAL", "Jalisco"),
    e("MEX", "México"),
    e("MIC", "Michoacán de Ocampo"),
    e("MOR", "Morelos"),
    e("NAY", "Nayarit"),
    e("NLE", "Nuevo León"),
    e("OAX", "Oaxaca"),
    e("PUE", "Puebla"),
    e("QUE", "Querétaro"),
    e("ROO", "Quintana Roo"),
    e("SIN", "Sinaloa"),
    e("SLP", "San Luis Potosí"),
    e("SON", "Sonora"),
    e("TAB", "Tabasco"),
    e("TAM", "Tamaulipas"),
    e("TLA", "Tlaxcala"),
    e("VER", "Veracruz de Ignacio de la Llave"),
    e("YUC", "Yucatán"),
    e("ZAC", "Zacatecas"),
];

/// Look up an entity by code, ignoring case, surrounding space and an
/// "MX-" prefix.
pub fn entity(code: &str) -> Option<&'static Entity> {
    let code = code.trim();
    let code = code.get(..3).filter(|p| p.eq_ignore_ascii_case("MX-")).map_or(code, |_| &code[3..]);
    ENTITIES.iter().find(|e| e.code.eq_ignore_ascii_case(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn thirty_two_entities_with_unique_codes() {
        assert_eq!(ENTITIES.len(), 32);
        assert_eq!(ENTITIES.iter().map(|e| e.code).collect::<BTreeSet<_>>().len(), 32);
        assert_eq!(entity("mx-cmx").map(|e| e.name), Some("Ciudad de México"));
        assert_eq!(entity(" jal ").map(|e| e.code), Some("JAL"));
        assert_eq!(entity("DIF"), None, "the old Distrito Federal code is gone");
        assert_eq!(entity("MX"), None);
    }
}

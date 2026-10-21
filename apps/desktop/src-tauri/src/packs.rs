//! The country compliance packs this build ships, one line per country.
//! Adding a country: add its crate to Cargo.toml and one line here.

use khata_compliance::CompliancePack;

/// Every pack, in the order the country picker lists them.
pub const PACKS: &[&'static dyn CompliancePack] = &[
    &khata_in::India,
    &khata_ae::Uae,
    &khata_sa::Saudi,
    &khata_sg::Singapore,
    &khata_gb::UnitedKingdom,
    &khata_us::UnitedStates,
    &khata_mx::Mexico,
    &khata_de::Germany,
    &khata_fr::France,
    &khata_nl::Netherlands,
    &khata_es::Spain,
];

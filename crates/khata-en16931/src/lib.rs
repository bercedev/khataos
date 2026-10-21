//! EN 16931: the European standard semantic model of an electronic invoice,
//! its core business rules, and writers for its two syntaxes.
//!
//! This crate is country-neutral and pure (no I/O). A country pack maps its
//! books' invoice to [`Invoice`], calls [`Invoice::finalise`] to work out
//! the VAT breakdown and totals, checks it with [`rules::validate`], and
//! writes it with [`ubl::write`] or [`cii::write`]. See `docs/en16931.md`.
//!
//! - [`model`]: the invoice, by EN 16931 business term (BT-n) and business
//!   group (BG-n) numbers. Only the terms KhataOS fills are modelled.
//! - [`rules`]: the core rules (BR-01…, BR-CO-*, BR-DEC-*, and the VAT
//!   category rules BR-S/Z/E/AE/IC/G/O-*), with the wording and tolerances
//!   of the CEN/TC 434 validation artefacts
//!   (`github.com/ConnectingEurope/eInvoicing-EN16931`, EUPL) [PRIMARY]; and
//!   a few XRechnung (BR-DE-*) checks [UNCERTAIN where noted].
//! - [`ubl`]: OASIS UBL 2.1 `Invoice` / `CreditNote`.
//! - [`cii`]: UN/CEFACT Cross Industry Invoice D16B (`CrossIndustryInvoice`).
//! - [`units`]: UN/ECE Recommendation 20 unit codes from free-text units.
//! - [`pint`]: Peppol PINT billing UBL with the Singapore (PINT SG,
//!   InvoiceNow) and UAE (PINT AE) specialisations: their tax categories,
//!   identifiers and rules.
//! - [`facturx`]: a Factur-X / ZUGFeRD PDF/A-3 that embeds the CII XML.
//!
//! The writers do not validate: write only an invoice whose
//! [`rules::validate_for`] list is empty, or show the list to the user.
//! Schema (XSD) and Schematron validation against the official artefacts
//! is not run here; KoSIT's validator is the reference for XRechnung.

pub mod cii;
pub mod facturx;
pub mod model;
pub mod pint;
pub mod rules;
pub mod ubl;
pub mod units;
mod xml;

pub use model::{
    Address, AllowanceCharge, Contact, CreditTransfer, ElectronicAddress, Invoice, Line, Party, PaymentInstructions,
    PrecedingInvoice, Totals, TypeCode, VatBreakdown, VatCategory,
};
pub use rules::{Violation, fatal, validate, validate_for};

use serde::Serialize;

/// The specification an invoice claims to follow (BT-24, "Specification
/// identifier"), and the business process (BT-23) that goes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Specification {
    /// Plain EN 16931 (`urn:cen.eu:en16931:2017`) [PRIMARY: the CEN
    /// artefacts' example invoices].
    En16931,
    /// XRechnung 3.0, the German CIUS by KoSIT.
    XRechnung3,
    /// Peppol BIS Billing 3.0 (OpenPeppol).
    PeppolBis3,
}

/// BT-24 for EN 16931 itself.
pub const EN16931_ID: &str = "urn:cen.eu:en16931:2017";
/// BT-24 for XRechnung 3.0.x (KoSIT, "Specification XRechnung 3.0")
/// [PRIMARY: KoSIT Schematron v2.6.0, docs/research/germany-compliance.md §9].
pub const XRECHNUNG_3_ID: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
/// BT-24 for Peppol BIS Billing 3.0 (docs.peppol.eu/poacc/billing/3.0).
pub const PEPPOL_BIS3_ID: &str = "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0";
/// BT-23 for Peppol BIS Billing 3.0.
pub const PEPPOL_BIS3_PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

impl Specification {
    /// The Specification identifier (BT-24).
    pub fn customization_id(self) -> &'static str {
        match self {
            Specification::En16931 => EN16931_ID,
            Specification::XRechnung3 => XRECHNUNG_3_ID,
            Specification::PeppolBis3 => PEPPOL_BIS3_ID,
        }
    }

    /// The Business process type (BT-23), when the specification sets one.
    /// Peppol requires it. XRechnung leaves it optional; KoSIT's examples
    /// use the Peppol value, which is written here too [UNCERTAIN: optional
    /// either way].
    pub fn profile_id(self) -> Option<&'static str> {
        match self {
            Specification::En16931 => None,
            Specification::XRechnung3 | Specification::PeppolBis3 => Some(PEPPOL_BIS3_PROCESS),
        }
    }

    /// Parse "en16931", "xrechnung" or "peppol" (any case).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en16931" | "en 16931" | "en" => Some(Specification::En16931),
            "xrechnung" | "xrechnung3" | "xrechnung_3.0" => Some(Specification::XRechnung3),
            "peppol" | "peppol-bis3" | "bis3" => Some(Specification::PeppolBis3),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specification_identifiers() {
        assert_eq!(Specification::En16931.customization_id(), "urn:cen.eu:en16931:2017");
        assert_eq!(
            Specification::XRechnung3.customization_id(),
            "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0"
        );
        assert_eq!(
            Specification::PeppolBis3.customization_id(),
            "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0"
        );
        assert_eq!(Specification::PeppolBis3.profile_id(), Some("urn:fdc:peppol.eu:2017:poacc:billing:01:1.0"));
        assert_eq!(Specification::En16931.profile_id(), None);
        assert_eq!(Specification::parse(" XRechnung "), Some(Specification::XRechnung3));
        assert_eq!(Specification::parse("zugferd"), None);
    }
}

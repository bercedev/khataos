//! TRN (Tax Registration Number) validation.
//!
//! A TRN is a 15-digit number issued by the FTA. Its first 10 digits are
//! the TIN, which e-invoicing uses as the Peppol participant ID (`0235:` +
//! TIN). Each VAT-group member has its own TIN: the first 10 digits of its
//! own TRN (MoF *UAE Electronic Invoicing Guidelines* v1.1 §2, §3;
//! `docs/research/mena-compliance.md` §1.1 [PRIMARY]).
//!
//! **No check digit is validated.** The FTA has not published a checksum
//! for the TRN (research §1.1 and §9 item 14 [UNCERTAIN]), so this module
//! checks the format only, and any 15 digits pass. Whether a TRN is really
//! registered can only be confirmed online (EmaraTax TRN verification).
//!
//! Most TRNs start with "100", but that is not a published rule
//! [SECONDARY]: [`usual_prefix`] lets the app warn without blocking.
//!
//! A TRN encodes no emirate, so [`TaxIdDetails::region`] is always `None`.

use khata_compliance::{TaxIdDetails, TaxIdError};

pub const LEN: usize = 15;

/// Length of the TIN at the start of a TRN.
pub const TIN_LEN: usize = 10;

/// The prefix most TRNs carry; not a rule.
pub const USUAL_PREFIX: &str = "100";

/// Trim the ends and drop the spaces and hyphens people type to group the
/// digits ("100-1234-5678-9003").
pub fn normalise(trn: &str) -> String {
    trn.chars().filter(|c| !(c.is_whitespace() || *c == '-')).collect()
}

/// Validate a TRN offline: 15 digits after [`normalise`]. Returns the
/// normalised TRN with its TIN, or the first problem found.
pub fn validate(trn: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(trn);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    let found = value.chars().count();
    if found != LEN {
        return Err(TaxIdError::Length { expected: LEN, found });
    }
    if let Some(i) = value.chars().position(|c| !c.is_ascii_digit()) {
        return Err(TaxIdError::Format { position: i + 1, expected: "a digit" });
    }
    Ok(TaxIdDetails { entity_id: Some(value[..TIN_LEN].to_owned()), region: None, value })
}

/// The TIN inside a valid TRN (its first 10 digits).
pub fn tin(trn: &str) -> Option<String> {
    validate(trn).ok().and_then(|d| d.entity_id)
}

/// Whether a TRN starts with "100", as most do. A `false` is worth a
/// warning, never a block.
pub fn usual_prefix(trn: &str) -> bool {
    normalise(trn).starts_with(USUAL_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made-up numbers in the published format. Without a checksum, no real
    /// TRN is needed to test the format.
    const SAMPLE: &str = "100123456700003";

    fn err(t: &str) -> TaxIdError {
        validate(t).expect_err(t)
    }

    #[test]
    fn accepts_fifteen_digits_and_exposes_the_tin() {
        let d = validate(SAMPLE).unwrap();
        assert_eq!(d.value, SAMPLE);
        assert_eq!(d.entity_id.as_deref(), Some("1001234567"));
        assert_eq!(d.region, None, "a TRN encodes no emirate");
        assert_eq!(tin(SAMPLE).as_deref(), Some("1001234567"));
        assert_eq!(tin("123"), None);
    }

    #[test]
    fn spaces_and_hyphens_are_dropped() {
        for typed in ["100 1234 5670 0003", " 100-123-456-700-003 ", "100-1234567-00003\n", "100\t123456700003"] {
            assert_eq!(validate(typed).unwrap().value, SAMPLE, "{typed:?}");
        }
    }

    #[test]
    fn there_is_no_check_digit() {
        // Every last digit passes: no checksum is published, so none is invented.
        for last in '0'..='9' {
            assert!(validate(&format!("{}{last}", &SAMPLE[..14])).is_ok());
        }
    }

    #[test]
    fn rejects_empty_wrong_length_and_non_digits() {
        assert_eq!(err(""), TaxIdError::Empty);
        assert_eq!(err(" - "), TaxIdError::Empty);
        assert_eq!(err("10012345670000"), TaxIdError::Length { expected: 15, found: 14 });
        assert_eq!(err("1001234567000031"), TaxIdError::Length { expected: 15, found: 16 });
        assert_eq!(err("10012345670000A"), TaxIdError::Format { position: 15, expected: "a digit" });
        assert_eq!(err("1O0123456700003"), TaxIdError::Format { position: 2, expected: "a digit" });
        assert_eq!(err("100.123.456.700"), TaxIdError::Format { position: 4, expected: "a digit" });
        assert_eq!(err("١٠٠١٢٣٤٥٦٧٠٠٠٠٣"), TaxIdError::Format { position: 1, expected: "a digit" }, "Arabic-Indic digits");
    }

    #[test]
    fn an_unusual_prefix_is_only_a_hint() {
        assert!(usual_prefix(SAMPLE));
        assert!(usual_prefix("100-1"));
        let other = "200123456700003";
        assert!(!usual_prefix(other));
        assert!(validate(other).is_ok(), "an unusual prefix still validates");
    }
}

//! KSA VAT registration number validation.
//!
//! A VAT number is **15 digits whose first and last digits are "3"**. For a
//! VAT group the **11th digit is "1"**, and each member is then identified
//! by its own 10-digit TIN (ZATCA XML Implementation Standard v1.2, BR-KSA-40
//! and BR-KSA-44; Security Features Standard v1.2, CSR table;
//! `docs/research/mena-compliance.md` §5 [PRIMARY]).
//!
//! **No check digit is validated.** ZATCA has not published a checksum
//! (research §5 and §9 item 14 [UNCERTAIN]), so this module checks the
//! format only. Whether a number is really registered can only be confirmed
//! online with ZATCA.
//!
//! A VAT number encodes no region, so [`TaxIdDetails::region`] is always
//! `None`. The research does not say that the TIN sits inside the VAT
//! number, so [`TaxIdDetails::entity_id`] is `None` too.

use khata_compliance::{TaxIdDetails, TaxIdError};

pub const LEN: usize = 15;

/// Position (1-based) of the digit that marks a VAT group.
pub const GROUP_DIGIT_POSITION: usize = 11;

/// Trim the ends and drop the spaces and hyphens people type to group the
/// digits ("300-1234-5678-9003").
pub fn normalise(vat: &str) -> String {
    vat.chars().filter(|c| !(c.is_whitespace() || *c == '-')).collect()
}

/// Validate a VAT number offline: 15 digits after [`normalise`], starting
/// and ending with "3". Returns the normalised number, or the first problem
/// found.
pub fn validate(vat: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(vat);
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
    if !value.starts_with('3') {
        return Err(TaxIdError::Format { position: 1, expected: "3" });
    }
    if !value.ends_with('3') {
        return Err(TaxIdError::Format { position: LEN, expected: "3" });
    }
    Ok(TaxIdDetails { value, region: None, entity_id: None })
}

/// Whether a valid VAT number belongs to a VAT group (11th digit "1").
pub fn is_vat_group(vat: &str) -> bool {
    validate(vat).is_ok_and(|d| d.value.as_bytes()[GROUP_DIGIT_POSITION - 1] == b'1')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up number in the published format.
    const SAMPLE: &str = "300075588700003";

    fn err(t: &str) -> TaxIdError {
        validate(t).expect_err(t)
    }

    #[test]
    fn accepts_fifteen_digits_from_three_to_three() {
        let d = validate(SAMPLE).unwrap();
        assert_eq!((d.value.as_str(), d.region, d.entity_id), (SAMPLE, None, None));
        for typed in ["300 0755 8870 0003", " 300-075588700003 ", "3000755887\t00003"] {
            assert_eq!(validate(typed).unwrap().value, SAMPLE, "{typed:?}");
        }
    }

    #[test]
    fn there_is_no_check_digit() {
        // Any middle digits pass: no checksum is published, so none is invented.
        for mid in '0'..='9' {
            assert!(validate(&format!("3{}3", mid.to_string().repeat(13))).is_ok());
        }
    }

    #[test]
    fn rejects_bad_formats() {
        assert_eq!(err(""), TaxIdError::Empty);
        assert_eq!(err(" - "), TaxIdError::Empty);
        assert_eq!(err("30007558870003"), TaxIdError::Length { expected: 15, found: 14 });
        assert_eq!(err("3000755887000033"), TaxIdError::Length { expected: 15, found: 16 });
        assert_eq!(err("30007558870000A"), TaxIdError::Format { position: 15, expected: "a digit" });
        assert_eq!(err("100075588700003"), TaxIdError::Format { position: 1, expected: "3" });
        assert_eq!(err("300075588700001"), TaxIdError::Format { position: 15, expected: "3" });
        assert_eq!(err("٣٠٠٠٧٥٥٨٨٧٠٠٠٠٣"), TaxIdError::Format { position: 1, expected: "a digit" }, "Arabic-Indic digits");
    }

    #[test]
    fn the_eleventh_digit_marks_a_vat_group() {
        assert!(!is_vat_group(SAMPLE));
        assert!(is_vat_group("300075588710003"));
        assert!(!is_vat_group("30007558871000"), "invalid numbers are not groups");
    }
}

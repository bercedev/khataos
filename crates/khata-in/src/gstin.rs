//! GSTIN validation: format, state code, embedded PAN and check character.
//!
//! A GSTIN has 15 characters:
//!
//! | Position | Meaning |
//! |---|---|
//! | 1-2 | State code ([`crate::states`]) |
//! | 3-12 | PAN: five letters, four digits, one letter |
//! | 13 | Registration number for this PAN in the state: 1-9, then A-Z |
//! | 14 | "Z" by default |
//! | 15 | Check character |
//!
//! The check character is a Luhn mod-36 code over the first 14 characters.
//! GSTN has not published the algorithm in the Act or Rules; it is the one
//! every third-party validator uses, and it agrees with GSTINs printed on
//! real invoices. Special registrations (UIN, NRTP, TDS/TCS, OIDAR) can use
//! other layouts: those go through the override in rule M2, with a reason.

use khata_compliance::{Region, TaxIdDetails, TaxIdError};

use crate::states::{self, Kind};

pub const LEN: usize = 15;

const ALPHABET: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn value_of(c: u8) -> Option<u32> {
    ALPHABET.iter().position(|&a| a == c).map(|v| v as u32)
}

/// The check character for the first 14 characters of a GSTIN, or `None`
/// if they contain anything outside 0-9 and A-Z.
pub fn check_character(first14: &str) -> Option<char> {
    let bytes = first14.as_bytes();
    if bytes.len() != LEN - 1 {
        return None;
    }
    let mut sum = 0;
    for (i, &b) in bytes.iter().enumerate() {
        // Factors alternate 1, 2, 1, 2… from the left; each product is
        // folded back into base 36 (quotient + remainder).
        let product = value_of(b)? * if i % 2 == 0 { 1 } else { 2 };
        sum += product / 36 + product % 36;
    }
    Some(ALPHABET[((36 - sum % 36) % 36) as usize] as char)
}

/// Upper-case and trim what the user typed. Inner spaces are kept so they
/// are reported, not silently dropped.
pub fn normalise(gstin: &str) -> String {
    gstin.trim().to_ascii_uppercase()
}

/// Validate a GSTIN offline. Returns the normalised GSTIN with its state and
/// PAN, or the first problem found, reading left to right.
pub fn validate(gstin: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(gstin);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    let found = value.chars().count();
    if found != LEN {
        return Err(TaxIdError::Length { expected: LEN, found });
    }
    let b = value.as_bytes();
    let format = |position, expected| Err(TaxIdError::Format { position, expected });
    for (i, &c) in b.iter().enumerate() {
        let position = i + 1;
        let ok = match position {
            1 | 2 | 8..=11 => c.is_ascii_digit(),
            3..=7 | 12 => c.is_ascii_uppercase(),
            13 => matches!(c, b'1'..=b'9' | b'A'..=b'Z'),
            14 => c == b'Z',
            _ => c.is_ascii_alphanumeric(),
        };
        if !ok {
            return format(
                position,
                match position {
                    1 | 2 => "a digit (state code)",
                    3..=7 | 12 => "a letter (PAN)",
                    8..=11 => "a digit (PAN)",
                    13 => "1-9 or A-Z (registration number)",
                    14 => "Z",
                    _ => "a digit or letter (check character)",
                },
            );
        }
    }
    let code = &value[..2];
    let state = states::state(code)
        .ok_or_else(|| TaxIdError::UnknownRegion { kind: "GST state code", code: code.to_owned() })?;
    let expected = check_character(&value[..14]).expect("characters checked above");
    let found = b[14] as char;
    if expected != found {
        return Err(TaxIdError::CheckCharacter { expected, found });
    }
    let region = (state.kind != Kind::CentreJurisdiction).then_some(Region { code: state.code, name: state.name });
    Ok(TaxIdDetails { entity_id: Some(value[2..12].to_owned()), region, value })
}

/// Whether a GSTIN embeds `pan` (both compared upper-case), for checking a
/// GSTIN against a PAN captured separately.
pub fn embeds_pan(gstin: &str, pan: &str) -> bool {
    let g = normalise(gstin);
    g.len() == LEN && g.is_ascii() && g[2..12] == pan.trim().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GSTINs printed on real, published documents: the sample in GSTN's
    /// API documentation (27AAPFU0939F1ZV) and registrations of large listed
    /// companies, all public on the GST portal's search.
    const KNOWN_GOOD: &[&str] = &[
        "27AAPFU0939F1ZV",
        "29AAGCB7383J1Z4",
        "27AAACR5055K1Z7",
        "24AAACC1206D1ZM",
        "09AAACH7409R1ZZ",
        "27AAACT2727Q1ZW",
    ];

    fn err(g: &str) -> TaxIdError {
        validate(g).expect_err(g)
    }

    #[test]
    fn accepts_known_good_gstins() {
        for g in KNOWN_GOOD {
            let d = validate(g).unwrap_or_else(|e| panic!("{g}: {e}"));
            assert_eq!(d.value, *g);
        }
    }

    #[test]
    fn exposes_state_and_pan() {
        let d = validate("27AAPFU0939F1ZV").unwrap();
        assert_eq!(d.region, Some(Region { code: "27", name: "Maharashtra" }));
        assert_eq!(d.entity_id.as_deref(), Some("AAPFU0939F"));
        assert!(embeds_pan("27aapfu0939f1zv", " aapfu0939f "));
        assert!(!embeds_pan("27AAPFU0939F1ZV", "AAPFU0939G"));
        assert!(!embeds_pan("27AAPFU", "AAPFU0939F"));
    }

    #[test]
    fn normalises_case_and_surrounding_space() {
        assert_eq!(validate("  27aapfu0939f1zv\n").unwrap().value, "27AAPFU0939F1ZV");
    }

    #[test]
    fn every_wrong_check_character_is_caught() {
        for g in KNOWN_GOOD {
            let right = g.as_bytes()[14];
            for &c in ALPHABET.iter().filter(|&&c| c != right) {
                let bad = format!("{}{}", &g[..14], c as char);
                assert_eq!(
                    err(&bad),
                    TaxIdError::CheckCharacter { expected: right as char, found: c as char },
                    "{bad}"
                );
            }
        }
    }

    #[test]
    fn single_character_typos_in_the_body_are_caught() {
        // Changing any one of the 14 body characters to another valid
        // character must fail somewhere: format, state or check character.
        for g in KNOWN_GOOD {
            for i in 0..14 {
                for &c in ALPHABET {
                    let mut bad = g.as_bytes().to_vec();
                    if bad[i] == c {
                        continue;
                    }
                    bad[i] = c;
                    let bad = String::from_utf8(bad).unwrap();
                    assert!(validate(&bad).is_err(), "{bad} passed");
                }
            }
        }
    }

    #[test]
    fn adjacent_swaps_in_the_pan_are_caught() {
        let g = "27AAPFU0939F1ZV";
        for i in 2..11 {
            let mut b = g.as_bytes().to_vec();
            if b[i] == b[i + 1] {
                continue;
            }
            b.swap(i, i + 1);
            let swapped = String::from_utf8(b).unwrap();
            assert!(validate(&swapped).is_err(), "{swapped} passed");
        }
    }

    #[test]
    fn rejects_wrong_lengths_and_empty_input() {
        assert_eq!(err(""), TaxIdError::Empty);
        assert_eq!(err("   "), TaxIdError::Empty);
        assert_eq!(err("27AAPFU0939F1Z"), TaxIdError::Length { expected: 15, found: 14 });
        assert_eq!(err("27AAPFU0939F1ZVX"), TaxIdError::Length { expected: 15, found: 16 });
        assert_eq!(err("27AAPFU0939F1ZΩ"), TaxIdError::Format { position: 15, expected: "a digit or letter (check character)" });
    }

    #[test]
    fn rejects_bad_layout_position_by_position() {
        let pos = |g: &str| match err(g) {
            TaxIdError::Format { position, .. } => position,
            other => panic!("{g}: {other:?}"),
        };
        assert_eq!(pos("A7AAPFU0939F1ZV"), 1, "state code must be digits");
        assert_eq!(pos("271APFU0939F1ZV"), 3, "PAN starts with letters");
        assert_eq!(pos("27AAPF00939F1ZV"), 7);
        assert_eq!(pos("27AAPFUA939F1ZV"), 8, "PAN digits");
        assert_eq!(pos("27AAPFU093911ZV"), 12, "PAN ends with a letter");
        assert_eq!(pos("27AAPFU0939F0ZV"), 13, "entity number is never 0");
        assert_eq!(pos("27AAPFU0939F1YV"), 14, "14th character is Z");
        assert_eq!(pos("27AAPFU0939F1Z-"), 15);
        assert_eq!(pos("27AAPFU 939F1ZV"), 8, "inner spaces are reported, not dropped");
    }

    #[test]
    fn rejects_unknown_state_codes() {
        for code in ["00", "39", "40", "96", "98"] {
            let body = format!("{code}AAPFU0939F1Z");
            let g = format!("{body}{}", check_character(&body).unwrap());
            assert_eq!(err(&g), TaxIdError::UnknownRegion { kind: "GST state code", code: code.into() }, "{g}");
        }
    }

    #[test]
    fn centre_jurisdiction_has_no_region_and_legacy_codes_still_validate() {
        let body = "99AAPFU0939F1Z";
        let d = validate(&format!("{body}{}", check_character(body).unwrap())).unwrap();
        assert_eq!(d.region, None);
        let body = "25AAPFU0939F1Z";
        let d = validate(&format!("{body}{}", check_character(body).unwrap())).unwrap();
        assert_eq!(d.region.unwrap().name, "Daman and Diu");
    }

    #[test]
    fn sandbox_dummies_fail_the_check_character() {
        // The e-invoice sandbox accepts made-up GSTINs; these don't pass.
        assert!(matches!(err("29AWGPV7107B1Z1"), TaxIdError::CheckCharacter { expected: 'F', found: '1' }));
        assert!(matches!(err("37ARZPT4384Q1MT"), TaxIdError::Format { position: 14, .. }));
    }

    #[test]
    fn check_character_needs_14_valid_characters() {
        assert_eq!(check_character("27AAPFU0939F1Z"), Some('V'));
        assert_eq!(check_character("27AAPFU0939F1"), None);
        assert_eq!(check_character("27aapfu0939f1z"), None, "lower case is normalised before, not here");
    }
}

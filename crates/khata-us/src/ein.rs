//! US taxpayer identification numbers: EIN, and the SSN / ITIN a payee
//! may give on Form W-9.
//!
//! Formats (`docs/research/us-compliance.md` §5):
//! - **EIN**: nine digits written `NN-NNNNNNN`. The IRS publishes no check
//!   digit, so only the format is checked; a pass says nothing about
//!   whether the number was issued (the IRS TIN Matching program can).
//!   The IRS lists prefixes it never assigns any more (IRM
//!   21.7.13.2.4.1: 00, 07, 08, 09, 17, 18, 19, 28, 29, 49, 69, 70, 78,
//!   79, 89, 96, 97 [HIGH]), but entities holding them still exist, so
//!   [`unassigned_prefix`] is a warning for the app to show, never a
//!   rejection.
//! - **SSN**: nine digits written `NNN-NN-NNNN`; area 000 or 666, group 00
//!   and serial 0000 are never issued [MEDIUM: SSA, not re-read for this
//!   pack].
//! - **ITIN**: nine digits `9NN-NN-NNNN`; an ITIN always begins with 9
//!   [MEDIUM: IRS ITIN pages]. Its middle-digit ranges have changed over
//!   time, so they are not checked.
//!
//! Nine digits without hyphens are read as an EIN (the pack's label), and
//! the stored form is then `NN-NNNNNNN`. A TIN encodes no state, so
//! [`TaxIdDetails::region`] is always `None`.
//!
//! A TIN, an SSN especially, is sensitive: the books store what the user
//! types, and KhataOS never logs or sends it.

use khata_compliance::{TaxIdDetails, TaxIdError};
use serde::Serialize;

/// What kind of TIN a valid number is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TinKind {
    Ein,
    Ssn,
    Itin,
}

fn digits(s: &str) -> Option<Vec<u32>> {
    s.chars().map(|c| c.to_digit(10)).collect()
}

fn n(d: &[u32]) -> u32 {
    d.iter().fold(0, |a, x| a * 10 + x)
}

/// Check a TIN's format and say which kind it is, with its normalised form.
pub fn classify(id: &str) -> Result<(TinKind, String), TaxIdError> {
    let s: String = id.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return Err(TaxIdError::Empty);
    }
    let dashes: Vec<usize> = s.char_indices().filter(|(_, c)| *c == '-').map(|(i, _)| i).collect();
    let bare: String = s.chars().filter(|c| *c != '-').collect();
    if bare.chars().count() != 9 {
        return Err(TaxIdError::Length { expected: 9, found: bare.chars().count() });
    }
    let Some(d) = digits(&bare) else {
        let position = s.chars().position(|c| !(c.is_ascii_digit() || c == '-')).unwrap_or(0) + 1;
        return Err(TaxIdError::Format { position, expected: "a digit" });
    };
    match dashes.as_slice() {
        [] | [2] => Ok((TinKind::Ein, format!("{}-{}", &bare[..2], &bare[2..]))),
        [3, 6] => {
            let (area, group, serial) = (n(&d[..3]), n(&d[3..5]), n(&d[5..]));
            if area >= 900 {
                return Ok((TinKind::Itin, s));
            }
            if area == 0 || area == 666 {
                return Err(TaxIdError::Format { position: 1, expected: "an SSN area other than 000 or 666" });
            }
            if group == 0 {
                return Err(TaxIdError::Format { position: 5, expected: "an SSN group other than 00" });
            }
            if serial == 0 {
                return Err(TaxIdError::Format { position: 8, expected: "an SSN serial other than 0000" });
            }
            Ok((TinKind::Ssn, s))
        }
        [first, ..] => Err(TaxIdError::Format {
            position: s[..*first].chars().count() + 1,
            expected: "NN-NNNNNNN (EIN) or NNN-NN-NNNN (SSN/ITIN)",
        }),
    }
}

/// EIN prefixes the IRS no longer assigns (IRM 21.7.13.2.4.1).
pub const UNASSIGNED_PREFIXES: [&str; 17] =
    ["00", "07", "08", "09", "17", "18", "19", "28", "29", "49", "69", "70", "78", "79", "89", "96", "97"];

/// Whether a valid EIN starts with a prefix the IRS no longer assigns: a
/// reason to double-check the number, not to refuse it.
pub fn unassigned_prefix(ein: &str) -> bool {
    matches!(classify(ein), Ok((TinKind::Ein, v)) if UNASSIGNED_PREFIXES.contains(&&v[..2]))
}

/// Validate a TIN for the pack.
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let (_, value) = classify(id)?;
    Ok(TaxIdDetails { value, region: None, entity_id: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eins_are_nine_digits_two_and_seven() {
        assert_eq!(classify("12-3456789"), Ok((TinKind::Ein, "12-3456789".into())));
        assert_eq!(classify(" 123456789 "), Ok((TinKind::Ein, "12-3456789".into())));
        assert_eq!(validate("12-3456789").unwrap().region, None);
        assert_eq!(classify(""), Err(TaxIdError::Empty));
        assert_eq!(classify("12-345678"), Err(TaxIdError::Length { expected: 9, found: 8 }));
        assert_eq!(classify("12-34567A9"), Err(TaxIdError::Format { position: 9, expected: "a digit" }));
        assert!(matches!(classify("1234-56789"), Err(TaxIdError::Format { position: 5, .. })));
        assert!(unassigned_prefix("07-1234567") && !unassigned_prefix("12-3456789") && !unassigned_prefix("070-12-3456"));
    }

    #[test]
    fn ssns_and_itins() {
        assert_eq!(classify("123-45-6789"), Ok((TinKind::Ssn, "123-45-6789".into())));
        assert_eq!(classify("912-70-1234"), Ok((TinKind::Itin, "912-70-1234".into())));
        assert_eq!(classify("912-12-1234").unwrap().0, TinKind::Itin, "ITIN ranges are not checked");
        for bad in ["000-12-3456", "666-12-3456", "123-00-4567", "123-45-0000"] {
            assert!(matches!(classify(bad), Err(TaxIdError::Format { .. })), "{bad}");
        }
    }
}

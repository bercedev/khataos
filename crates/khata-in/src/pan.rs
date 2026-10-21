//! PAN (Permanent Account Number) validation: structure and holder type.
//!
//! | Position | Meaning |
//! |---|---|
//! | 1-3 | Letters (an alphabetic series) |
//! | 4 | Holder type: P individual, H HUF, C company, F firm, A AOP, B BOI, T trust, L local authority, J artificial juridical person, G government |
//! | 5 | Letter (a surname or name initial) |
//! | 6-9 | Digits |
//! | 10 | Letter (a check character, algorithm not published) |
//!
//! The research doc (`docs/research/india-compliance.md`, GSTIN row) gives
//! the five letters / four digits / one letter shape, as embedded in a
//! GSTIN. The holder-type letters are the widely published Income Tax
//! Department list; they are not in the research doc [SECONDARY]. The
//! check letter is not verified (its algorithm is not public).

use std::fmt;

use khata_core::DeducteeKind;

pub const LEN: usize = 10;

/// A PAN that passed the structural checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pan {
    /// Upper-cased and trimmed.
    pub value: String,
    pub holder: DeducteeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanError {
    /// Not 10 characters after trimming.
    Length(usize),
    /// Character at 1-based `position` is not what that position needs.
    Character { position: usize, expected: &'static str },
    /// The fourth letter is not a known holder type.
    UnknownHolder(char),
}

impl fmt::Display for PanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(n) => write!(f, "a PAN has 10 characters, not {n}"),
            Self::Character { position, expected } => write!(f, "character {position} of the PAN should be {expected}"),
            Self::UnknownHolder(c) => write!(f, "the 4th character of the PAN ({c}) is not a known holder type"),
        }
    }
}

impl std::error::Error for PanError {}

/// The holder type a PAN's fourth letter stands for.
pub fn holder(letter: char) -> Option<DeducteeKind> {
    Some(match letter.to_ascii_uppercase() {
        'P' => DeducteeKind::Individual,
        'H' => DeducteeKind::Huf,
        'C' => DeducteeKind::Company,
        'F' => DeducteeKind::Firm,
        'A' => DeducteeKind::Aop,
        'B' => DeducteeKind::Boi,
        'T' => DeducteeKind::Trust,
        'L' => DeducteeKind::LocalAuthority,
        'J' => DeducteeKind::Juridical,
        'G' => DeducteeKind::Government,
        _ => return None,
    })
}

/// Check a PAN's shape (AAAAA9999A) and its holder letter; the first
/// problem found, reading left to right.
pub fn validate(pan: &str) -> Result<Pan, PanError> {
    let value = pan.trim().to_ascii_uppercase();
    let chars: Vec<char> = value.chars().collect();
    if chars.len() != LEN {
        return Err(PanError::Length(chars.len()));
    }
    for (i, c) in chars.iter().enumerate() {
        let ok = match i {
            5..=8 => c.is_ascii_digit(),
            _ => c.is_ascii_uppercase(),
        };
        if !ok {
            let expected = if (5..=8).contains(&i) { "a digit" } else { "a letter" };
            return Err(PanError::Character { position: i + 1, expected });
        }
    }
    let holder = holder(chars[3]).ok_or(PanError::UnknownHolder(chars[3]))?;
    Ok(Pan { value, holder })
}

/// The PAN inside a GSTIN (characters 3 to 12), when the GSTIN is long
/// enough and that part is a valid PAN. The research doc's GSTIN row:
/// "3-12 PAN".
pub fn from_gstin(gstin: &str) -> Option<Pan> {
    let g = gstin.trim();
    if !g.is_ascii() || g.len() < 12 {
        return None;
    }
    validate(&g[2..12]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_pans_give_their_holder() {
        assert_eq!(validate(" aapfu0939f ").unwrap(), Pan { value: "AAPFU0939F".into(), holder: DeducteeKind::Firm });
        let kinds: Vec<_> = ["ABCPE1234F", "ABCHE1234F", "ABCCE1234F", "ABCAE1234F", "ABCTE1234F", "ABCGE1234F"]
            .iter()
            .map(|p| validate(p).unwrap().holder)
            .collect();
        assert_eq!(kinds, [
            DeducteeKind::Individual,
            DeducteeKind::Huf,
            DeducteeKind::Company,
            DeducteeKind::Aop,
            DeducteeKind::Trust,
            DeducteeKind::Government
        ]);
        assert_eq!(holder('b'), Some(DeducteeKind::Boi));
        assert_eq!(holder('L'), Some(DeducteeKind::LocalAuthority));
        assert_eq!(holder('J'), Some(DeducteeKind::Juridical));
    }

    #[test]
    fn structure_errors_name_the_position() {
        assert_eq!(validate("AAPFU0939"), Err(PanError::Length(9)));
        assert_eq!(validate(""), Err(PanError::Length(0)));
        assert_eq!(validate("1APFU0939F"), Err(PanError::Character { position: 1, expected: "a letter" }));
        assert_eq!(validate("AAPFUA939F"), Err(PanError::Character { position: 6, expected: "a digit" }));
        assert_eq!(validate("AAPFU09391"), Err(PanError::Character { position: 10, expected: "a letter" }));
        assert_eq!(validate("AAPXU0939F"), Err(PanError::UnknownHolder('X')));
        assert_eq!(validate("AAPÉU0939F"), Err(PanError::Character { position: 4, expected: "a letter" }));
        assert!(validate("AAPXU0939F").unwrap_err().to_string().contains("4th character"));
    }

    #[test]
    fn pan_comes_out_of_a_gstin() {
        assert_eq!(from_gstin("27AAPFU0939F1ZV").unwrap().value, "AAPFU0939F");
        assert_eq!(from_gstin("27AAPFU"), None);
        assert_eq!(from_gstin("27AAPXU0939F1ZV"), None);
    }
}

//! Leitweg-ID: the routing identifier of a German public buyer, written in
//! an XRechnung's Buyer reference (BT-10).
//!
//! Source: *Leitweg-ID Format-Spezifikation* 2.0.2 (28 Jul 2021) §2
//! [PRIMARY, research §9]: Grobadressierung (2 to 12 digits) –
//! optional Feinadressierung (up to 30 letters A–Z, any case, and digits) –
//! a two-digit check number "gemäß ISO/IEC 7064:2003 per Modulo 97-10";
//! hyphens as separators; 5 to 46 characters in all.

/// Why a Leitweg-ID is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeitwegError {
    Length(usize),
    Parts,
    Grob,
    Fein,
    Check { expected: String, found: String },
}

impl std::fmt::Display for LeitwegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeitwegError::Length(n) => write!(f, "a Leitweg-ID has 5 to 46 characters, not {n}"),
            LeitwegError::Parts => write!(f, "a Leitweg-ID is Grobadressierung-[Feinadressierung-]Prüfziffer"),
            LeitwegError::Grob => write!(f, "the Grobadressierung has 2 to 12 digits"),
            LeitwegError::Fein => write!(f, "the Feinadressierung has up to 30 letters and digits"),
            LeitwegError::Check { expected, found } => write!(f, "the check number should be {expected}, not {found}"),
        }
    }
}

/// The value MOD 97 of digits and letters (A = 10 … Z = 35).
fn mod97(s: &str) -> u32 {
    let mut r: u32 = 0;
    for c in s.chars() {
        let v = c.to_digit(36).expect("alphanumeric");
        let digits = if v >= 10 { 100 } else { 10 };
        r = (r * digits + v) % 97;
    }
    r
}

/// The two check digits for a Grob- and Feinadressierung (without hyphens):
/// 98 − (value·100 mod 97).
pub fn check_number(address: &str) -> String {
    format!("{:02}", 98 - mod97(&format!("{address}00")))
}

/// Check a Leitweg-ID's shape and check number.
pub fn validate(id: &str) -> Result<(), LeitwegError> {
    let id = id.trim();
    let n = id.chars().count();
    if !(5..=46).contains(&n) {
        return Err(LeitwegError::Length(n));
    }
    let parts: Vec<&str> = id.split('-').collect();
    let (grob, fein, check) = match parts.as_slice() {
        [g, c] => (*g, "", *c),
        [g, f, c] => (*g, *f, *c),
        _ => return Err(LeitwegError::Parts),
    };
    if !(2..=12).contains(&grob.len()) || !grob.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LeitwegError::Grob);
    }
    if parts.len() == 3 && (fein.is_empty() || fein.len() > 30 || !fein.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return Err(LeitwegError::Fein);
    }
    if check.len() != 2 || !check.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LeitwegError::Parts);
    }
    let expected = check_number(&format!("{grob}{fein}"));
    if expected != check {
        return Err(LeitwegError::Check { expected, found: check.to_owned() });
    }
    Ok(())
}

/// Whether a buyer reference is shaped like a Leitweg-ID attempt: digits,
/// optional letters and digits, and two digits, joined by hyphens.
pub fn looks_like(reference: &str) -> bool {
    let parts: Vec<&str> = reference.trim().split('-').collect();
    matches!(parts.len(), 2 | 3)
        && parts[0].len() >= 2
        && parts[0].bytes().all(|b| b.is_ascii_digit())
        && parts.last().is_some_and(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_specifications_example() {
        assert_eq!(check_number("040110001234512345"), "06");
        assert_eq!(validate("04011000-1234512345-06"), Ok(()));
        assert_eq!(validate("04011000-1234512345-07"), Err(LeitwegError::Check { expected: "06".into(), found: "07".into() }));
        assert_eq!(validate(&format!("991-{}", check_number("991"))), Ok(()), "no Feinadressierung");
        assert_eq!(validate(&format!("991-ab12-{}", check_number("991AB12"))), Ok(()), "letters in any case");
    }

    #[test]
    fn shape_errors() {
        assert_eq!(validate("1-2"), Err(LeitwegError::Length(3)));
        assert_eq!(validate("0401-x-y-06"), Err(LeitwegError::Parts));
        assert_eq!(validate("0A011000-06"), Err(LeitwegError::Grob));
        assert_eq!(validate("04011000-12_45-06"), Err(LeitwegError::Fein));
        assert!(looks_like("04011000-1234512345-06") && !looks_like("PO 4711") && !looks_like("12"));
    }
}

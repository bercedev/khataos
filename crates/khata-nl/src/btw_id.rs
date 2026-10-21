//! Btw-identificatienummer (btw-id) and omzetbelastingnummer validation.
//!
//! Format: "NL", nine digits, "B" and two digits (`NL123456789B01`), 14
//! characters (`docs/research/netherlands-compliance.md` §1). Two check
//! rules exist, and a number is well formed when either holds:
//! - the **btw-id** given to sole traders since 1 January 2020 is checked
//!   with ISO 7064 MOD 97-10 over the whole number, letters read as two
//!   digits (A = 10 … N = 23, L = 21, B = 11): the remainder must be 1;
//! - the older **omzetbelastingnummer** (still the btw-id of legal
//!   entities, built on the RSIN) passes the elfproef (11-test) on its nine
//!   digits: 9·d1 + 8·d2 + … + 2·d8 − d9 ≡ 0 (mod 11).
//!
//! The two digits after the "B" are a sequence number, never 00. A pass
//! shows the number is well formed; only VIES (or the Belastingdienst) can
//! say it is registered.

use khata_compliance::{TaxIdDetails, TaxIdError};

/// The country prefix.
pub const PREFIX: &str = "NL";

/// Characters after the prefix: nine digits, "B", two digits.
pub const BODY_LEN: usize = 12;

/// Trim, drop spaces, dots and hyphens, upper-case letters.
pub fn normalise(id: &str) -> String {
    id.chars().filter(|c| !(c.is_whitespace() || matches!(c, '-' | '.'))).flat_map(char::to_uppercase).collect()
}

/// The elfproef on nine digits (shorter runs are padded with leading
/// zeros, as an RSIN or BSN of eight digits is): weights 9 down to 2 on the
/// first eight digits, −1 on the last; the sum must be a multiple of 11.
/// `None` unless every character is an ASCII digit.
pub fn elfproef(digits: &str) -> Option<bool> {
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let padded = format!("{digits:0>9}");
    let d: Vec<i64> = padded.bytes().map(|b| i64::from(b - b'0')).collect();
    let sum: i64 = (0..8).map(|i| (9 - i as i64) * d[i]).sum::<i64>() - d[8];
    Some(sum.rem_euclid(11) == 0)
}

/// ISO 7064 MOD 97-10 remainder of a string of digits and capital letters,
/// letters read as two digits (A = 10 … Z = 35), as for an IBAN. `None` on
/// any other character.
pub fn mod_97_10(s: &str) -> Option<u32> {
    let mut r: u32 = 0;
    for c in s.chars() {
        let v = match c {
            '0'..='9' => c as u32 - '0' as u32,
            'A'..='Z' => c as u32 - 'A' as u32 + 10,
            _ => return None,
        };
        r = if v >= 10 { (r * 100 + v) % 97 } else { (r * 10 + v) % 97 };
    }
    Some(r)
}

/// Which check a well-formed number passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// MOD 97-10 over "NL…B..": a btw-id issued since 2020.
    BtwId,
    /// The elfproef on the nine digits: an omzetbelastingnummer (RSIN based).
    Omzetbelastingnummer,
}

/// The check a normalised, well-shaped number passes, if any.
pub fn kind(value: &str) -> Option<Kind> {
    let digits = value.get(2..11)?;
    if elfproef(digits) == Some(true) {
        return Some(Kind::Omzetbelastingnummer);
    }
    (mod_97_10(value) == Some(1)).then_some(Kind::BtwId)
}

/// Whether a normalised string looks like a Dutch btw number: the prefix
/// "NL", or nine digits followed by "B".
pub fn looks_like(value: &str) -> bool {
    value.starts_with(PREFIX) || (value.len() >= 10 && value[..9].bytes().all(|b| b.is_ascii_digit()) && &value[9..10] == "B")
}

/// Validate a btw-id or omzetbelastingnummer offline. The "NL" prefix may
/// be left out; the value returned always has it. `entity_id` is the nine
/// digits (the RSIN for an omzetbelastingnummer).
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    let skip = if value.starts_with(PREFIX) { PREFIX.len() } else { 0 };
    let body = &value[skip..];
    for (i, c) in body.chars().enumerate() {
        let ok = if i == 9 { c == 'B' } else { c.is_ascii_digit() };
        if !ok {
            let expected = match i {
                9 => "the letter B",
                0 if skip == 0 => "a digit, or the prefix NL",
                _ => "a digit",
            };
            return Err(TaxIdError::Format { position: skip + i + 1, expected });
        }
    }
    if body.len() != BODY_LEN {
        return Err(TaxIdError::Length { expected: PREFIX.len() + BODY_LEN, found: PREFIX.len() + body.len() });
    }
    if &body[..9] == "000000000" {
        return Err(TaxIdError::Format { position: PREFIX.len() + 1, expected: "nine digits that are not all zero" });
    }
    if &body[10..] == "00" {
        return Err(TaxIdError::Format { position: PREFIX.len() + 11, expected: "a sequence number from 01 to 99" });
    }
    let full = format!("{PREFIX}{body}");
    if kind(&full).is_none() {
        // Name the last digit the elfproef would accept, when there is one.
        let expected = (0..10u8)
            .map(|d| format!("{}{}", &body[..8], d))
            .find(|n| elfproef(n) == Some(true))
            .map_or('?', |n| n.as_bytes()[8] as char);
        return Err(TaxIdError::CheckCharacter { expected, found: body.as_bytes()[8] as char });
    }
    Ok(TaxIdDetails { value: full, region: None, entity_id: Some(body[..9].to_owned()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_examples_pass() {
        // python-stdnum's stdnum.nl.btw examples (research §1): the first
        // passes the elfproef, the others MOD 97-10.
        assert_eq!(kind("NL004495445B01"), Some(Kind::Omzetbelastingnummer));
        for n in ["NL002455799B11", "NL000099998B57", "NL123456789B13"] {
            assert_eq!(kind(n), Some(Kind::BtwId), "{n}");
        }
        let d = validate("nl 0044.95.445.B01").unwrap();
        assert_eq!((d.value.as_str(), d.entity_id.as_deref()), ("NL004495445B01", Some("004495445")));
        assert_eq!(validate("002455799B11").unwrap().value, "NL002455799B11", "the prefix may be left out");
    }

    #[test]
    fn the_checks() {
        assert_eq!(elfproef("004495445"), Some(true));
        assert_eq!(elfproef("4495445"), Some(true), "padded with zeros");
        assert_eq!(elfproef("004495446"), Some(false));
        assert_eq!(elfproef("12345678x"), None);
        assert_eq!(mod_97_10("NL002455799B11"), Some(1));
        assert_eq!(mod_97_10("NL004495445B01"), Some(40));
        assert_eq!(mod_97_10("nl"), None);
        assert!(matches!(validate("NL004495446B01"), Err(TaxIdError::CheckCharacter { expected: '5', found: '6' })));
        assert!(validate("NL002455799B12").is_err());
    }

    #[test]
    fn format_errors() {
        assert_eq!(validate(""), Err(TaxIdError::Empty));
        assert_eq!(validate("NL004495445B0"), Err(TaxIdError::Length { expected: 14, found: 13 }));
        assert_eq!(validate("NL004495445C01"), Err(TaxIdError::Format { position: 12, expected: "the letter B" }));
        assert_eq!(validate("BE0123456789"), Err(TaxIdError::Format { position: 1, expected: "a digit, or the prefix NL" }));
        assert!(matches!(validate("NL000000000B01"), Err(TaxIdError::Format { position: 3, .. })));
        assert!(matches!(validate("NL004495445B00"), Err(TaxIdError::Format { position: 13, .. })));
        assert!(looks_like("NL1") && looks_like("004495445B01") && !looks_like("12345678"));
    }
}

//! Umsatzsteuer-Identifikationsnummer (USt-IdNr) validation.
//!
//! Format: "DE" and nine digits, the last a check digit computed with the
//! ISO/IEC 7064 "MOD 11,10" hybrid system over the first eight
//! (`docs/research/germany-compliance.md` §1.2 [SECONDARY: Wikipedia de,
//! python-stdnum]; ELSTER publishes the same algorithm for the Steuer-IdNr
//! [PRIMARY]). The first digit is never 0: python-stdnum rejects it, no
//! official text was found [UNCERTAIN]. The BZSt issues the
//! number (§27a UStG); a pass shows it is well formed, not that it is
//! valid: only the BZSt's confirmation service (eVatR) or the EU's VIES
//! can say that.

use khata_compliance::{TaxIdDetails, TaxIdError};

/// The country prefix.
pub const PREFIX: &str = "DE";

/// Digits after the prefix.
pub const DIGITS: usize = 9;

/// Trim, drop spaces, dots, hyphens and slashes, upper-case letters.
pub fn normalise(id: &str) -> String {
    id.chars().filter(|c| !(c.is_whitespace() || matches!(c, '-' | '.' | '/'))).flat_map(char::to_uppercase).collect()
}

/// The ISO 7064 MOD 11,10 check digit of a run of decimal digits:
/// start with P = 10; for each digit, S = (digit + P) mod 10, read 0 as
/// 10, then P = 2·S mod 11; the check digit is 11 − P, read 10 as 0.
/// `None` unless every character is an ASCII digit.
pub fn mod_11_10(digits: &str) -> Option<u32> {
    let mut p = 10;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        let mut s = (u32::from(b - b'0') + p) % 10;
        if s == 0 {
            s = 10;
        }
        p = (2 * s) % 11;
    }
    Some(match 11 - p {
        10 => 0,
        c => c,
    })
}

/// Whether nine digits end in their MOD 11,10 check digit.
pub fn check_digit_ok(nine: &str) -> bool {
    nine.len() == DIGITS
        && mod_11_10(&nine[..DIGITS - 1]).is_some_and(|c| nine.as_bytes()[DIGITS - 1] == b'0' + c as u8)
}

/// Whether a normalised string looks like a USt-IdNr attempt: "DE" and
/// digits, or exactly nine digits.
pub fn looks_like(value: &str) -> bool {
    value.starts_with(PREFIX) || (value.len() == DIGITS && value.bytes().all(|b| b.is_ascii_digit()))
}

/// Validate a USt-IdNr offline. The prefix may be left out; the value
/// returned always has it. `entity_id` is the nine digits.
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    let skip = if value.starts_with(PREFIX) { PREFIX.len() } else { 0 };
    let body = &value[skip..];
    if let Some(i) = body.chars().position(|c| !c.is_ascii_digit()) {
        let expected = if i == 0 && skip == 0 { "a digit, or the prefix DE" } else { "a digit" };
        return Err(TaxIdError::Format { position: skip + i + 1, expected });
    }
    if body.len() != DIGITS {
        return Err(TaxIdError::Length { expected: PREFIX.len() + DIGITS, found: PREFIX.len() + body.len() });
    }
    if body.starts_with('0') {
        return Err(TaxIdError::Format { position: PREFIX.len() + 1, expected: "a digit from 1 to 9 (a USt-IdNr does not start with 0)" });
    }
    let expected = mod_11_10(&body[..DIGITS - 1]).expect("digits");
    let found = body.as_bytes()[DIGITS - 1] as char;
    let expected = char::from_digit(expected, 10).expect("one digit");
    if found != expected {
        return Err(TaxIdError::CheckCharacter { expected, found });
    }
    Ok(TaxIdDetails { value: format!("{PREFIX}{body}"), region: None, entity_id: Some(body.to_owned()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_examples_pass() {
        // python-stdnum's stdnum.de.vat docstring example and its EU VAT
        // test numbers, and the W-IdNr example's base (research §1.2).
        for n in ["DE136695976", "DE 118 619 592", "de125014955", "DE129304291", "811363057", "DE232637828"] {
            let d = validate(n).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert!(d.value.starts_with("DE") && d.value.len() == 11, "{n}");
        }
        assert_eq!(validate("136695976").unwrap().entity_id.as_deref(), Some("136695976"));
    }

    #[test]
    fn the_check_digit() {
        assert_eq!(mod_11_10("13669597"), Some(6));
        assert_eq!(mod_11_10("1366959x"), None);
        assert!(check_digit_ok("136695976") && !check_digit_ok("136695977") && !check_digit_ok("13669597"));
        assert_eq!(validate("DE136695977"), Err(TaxIdError::CheckCharacter { expected: '6', found: '7' }));
        // Every eight-digit body has exactly one valid check digit.
        let ok = (0..10).filter(|c| check_digit_ok(&format!("12345678{c}"))).count();
        assert_eq!(ok, 1);
    }

    #[test]
    fn format_errors() {
        assert_eq!(validate(""), Err(TaxIdError::Empty));
        assert_eq!(validate("DE12345678"), Err(TaxIdError::Length { expected: 11, found: 10 }));
        assert_eq!(validate("DE1366959760"), Err(TaxIdError::Length { expected: 11, found: 12 }));
        assert_eq!(validate("ATU13585627"), Err(TaxIdError::Format { position: 1, expected: "a digit, or the prefix DE" }));
        assert_eq!(validate("DE13669597A"), Err(TaxIdError::Format { position: 11, expected: "a digit" }));
        assert!(matches!(validate("DE036695976"), Err(TaxIdError::Format { position: 3, .. })));
        assert!(looks_like("DE123") && looks_like("136695976") && !looks_like("2181508150"));
    }
}

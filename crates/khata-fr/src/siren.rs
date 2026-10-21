//! SIREN, SIRET and the numéro de TVA intracommunautaire.
//!
//! - **SIREN**: nine digits given by INSEE to every business; the last is a
//!   Luhn check digit (`docs/research/france-compliance.md` §1).
//! - **SIRET**: the SIREN and a five-digit NIC (one per establishment),
//!   14 digits with a Luhn check over all of them. La Poste's
//!   establishments (SIREN 356 000 000) are numbered beyond what the Luhn
//!   check allows; theirs is checked by the sum of the digits being a
//!   multiple of 5.
//! - **Numéro de TVA intracommunautaire**: "FR", a two-character key and
//!   the SIREN (13 characters). The numeric key is
//!   `(12 + 3 × (SIREN mod 97)) mod 97`, two digits. Some numbers given to
//!   businesses without a SIREN have a key with letters; those are checked
//!   with python-stdnum's rules [SECONDARY] (research §1.3).
//!
//! A pass shows the number is well formed; only VIES (or the annuaire
//! des entreprises) can say it is registered.

use khata_compliance::{TaxIdDetails, TaxIdError};

/// The country prefix.
pub const PREFIX: &str = "FR";

/// Digits in a SIREN.
pub const SIREN_LEN: usize = 9;

/// Digits in a SIRET.
pub const SIRET_LEN: usize = 14;

/// La Poste's SIREN, whose SIRETs use the sum-of-digits rule.
pub const LA_POSTE_SIREN: &str = "356000000";

/// The characters of an alphanumeric key, in python-stdnum's order (no I
/// and no O).
const KEY_ALPHABET: &str = "0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Trim, drop spaces, dots, hyphens and slashes, upper-case letters.
pub fn normalise(id: &str) -> String {
    id.chars().filter(|c| !(c.is_whitespace() || matches!(c, '-' | '.' | '/'))).flat_map(char::to_uppercase).collect()
}

/// The Luhn check over a run of decimal digits (from the right, every
/// second digit doubled, 9 subtracted above 9; the sum must end in 0).
/// `None` unless every character is an ASCII digit.
pub fn luhn(digits: &str) -> Option<bool> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| {
            let d = u32::from(b - b'0');
            if i % 2 == 1 { if d * 2 > 9 { d * 2 - 9 } else { d * 2 } } else { d }
        })
        .sum();
    Some(sum % 10 == 0)
}

/// The numeric TVA key of a SIREN: `(12 + 3 × (SIREN mod 97)) mod 97`.
/// `None` unless the SIREN is nine digits.
pub fn tva_key(siren: &str) -> Option<u32> {
    if siren.len() != SIREN_LEN || !siren.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = siren.parse().ok()?;
    Some(((12 + 3 * (n % 97)) % 97) as u32)
}

/// The numéro de TVA intracommunautaire of a SIREN with a numeric key.
pub fn tva_number(siren: &str) -> Option<String> {
    tva_key(siren).map(|k| format!("{PREFIX}{k:02}{siren}"))
}

fn digit_error(value: &str, offset: usize) -> Option<TaxIdError> {
    value.chars().position(|c| !c.is_ascii_digit()).map(|i| TaxIdError::Format { position: offset + i + 1, expected: "a digit" })
}

/// Validate a SIREN (nine digits, Luhn).
pub fn validate_siren(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if let Some(e) = digit_error(&value, 0) {
        return Err(e);
    }
    if value.len() != SIREN_LEN {
        return Err(TaxIdError::Length { expected: SIREN_LEN, found: value.len() });
    }
    check_luhn(&value)?;
    Ok(TaxIdDetails { entity_id: Some(value.clone()), value, region: None })
}

/// The last digit that would make the Luhn check pass.
fn check_luhn(value: &str) -> Result<(), TaxIdError> {
    if luhn(value) == Some(true) {
        return Ok(());
    }
    let head = &value[..value.len() - 1];
    let expected = (0..10u8).map(|d| (b'0' + d) as char).find(|c| luhn(&format!("{head}{c}")) == Some(true)).unwrap_or('?');
    Err(TaxIdError::CheckCharacter { expected, found: value.as_bytes()[value.len() - 1] as char })
}

/// Validate a SIRET (14 digits: the SIREN, which must pass, and the NIC).
/// `entity_id` is the SIREN.
pub fn validate_siret(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if let Some(e) = digit_error(&value, 0) {
        return Err(e);
    }
    if value.len() != SIRET_LEN {
        return Err(TaxIdError::Length { expected: SIRET_LEN, found: value.len() });
    }
    let siren = &value[..SIREN_LEN];
    check_luhn(siren)?;
    let la_poste = siren == LA_POSTE_SIREN && value.bytes().map(|b| u32::from(b - b'0')).sum::<u32>() % 5 == 0;
    if !la_poste {
        check_luhn(&value)?;
    }
    Ok(TaxIdDetails { entity_id: Some(siren.to_owned()), value, region: None })
}

/// Whether a normalised string looks like a French TVA number attempt.
pub fn looks_like(value: &str) -> bool {
    value.starts_with(PREFIX)
}

fn key_index(c: char) -> Option<u64> {
    KEY_ALPHABET.find(c).map(|i| i as u64)
}

/// Validate a numéro de TVA intracommunautaire offline. `entity_id` is the
/// SIREN.
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if !value.starts_with(PREFIX) {
        return Err(TaxIdError::Format { position: 1, expected: "the prefix FR" });
    }
    let body = &value[PREFIX.len()..];
    for (i, c) in body.chars().enumerate() {
        let ok = if i < 2 { key_index(c).is_some() } else { c.is_ascii_digit() };
        if !ok {
            let expected = if i < 2 { "a digit or a capital letter other than I and O (the key)" } else { "a digit" };
            return Err(TaxIdError::Format { position: PREFIX.len() + i + 1, expected });
        }
    }
    if body.len() != 2 + SIREN_LEN {
        return Err(TaxIdError::Length { expected: PREFIX.len() + 2 + SIREN_LEN, found: value.len() });
    }
    let (key, siren) = body.split_at(2);
    // Monaco's numbers (FRxx000…) carry no SIREN (python-stdnum).
    if !siren.starts_with("000") {
        check_luhn(siren)?;
    }
    let n: u64 = siren.parse().expect("digits");
    let chars: Vec<char> = key.chars().collect();
    if key.bytes().all(|b| b.is_ascii_digit()) {
        let expected = tva_key(siren).expect("nine digits");
        let expected = format!("{expected:02}");
        if key != expected {
            let (e, f) = expected.chars().zip(key.chars()).find(|(e, f)| e != f).expect("they differ");
            return Err(TaxIdError::CheckCharacter { expected: e, found: f });
        }
    } else {
        // python-stdnum stdnum.fr.tva [SECONDARY].
        let (a, b) = (key_index(chars[0]).expect("checked"), key_index(chars[1]).expect("checked"));
        let ok = if chars[0].is_ascii_digit() {
            let check = a * 24 + b - 10;
            n % 11 == check % 11
        } else {
            let check = a * 34 + b - 100;
            (n + 1 + check / 11) % 11 == check % 11
        };
        if !ok {
            return Err(TaxIdError::CheckCharacter { expected: '?', found: chars[1] });
        }
    }
    let entity_id = Some(siren.to_owned());
    Ok(TaxIdDetails { value, region: None, entity_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siren_and_siret() {
        // INSEE's own SIREN and SIRET, python-stdnum's examples (research §1).
        for s in ["303 265 045", "732829320", "552100554", "356000000"] {
            assert!(validate_siren(s).is_ok(), "{s}");
        }
        assert_eq!(validate_siren("732829321"), Err(TaxIdError::CheckCharacter { expected: '0', found: '1' }));
        assert_eq!(validate_siren("73282932"), Err(TaxIdError::Length { expected: 9, found: 8 }));
        assert_eq!(validate_siren("73282932A"), Err(TaxIdError::Format { position: 9, expected: "a digit" }));
        let d = validate_siret("732 829 320 00074").unwrap();
        assert_eq!((d.value.as_str(), d.entity_id.as_deref()), ("73282932000074", Some("732829320")));
        assert!(validate_siret("73282932000075").is_err());
        assert!(validate_siret("35600000049837").is_ok(), "La Poste: sum of digits a multiple of 5");
        assert!(validate_siret("35600000049838").is_err());
        assert_eq!(luhn("12a"), None);
    }

    #[test]
    fn tva_numbers_with_numeric_keys() {
        assert_eq!(tva_key("303265045"), Some(40));
        assert_eq!(tva_number("732829320").as_deref(), Some("FR44732829320"));
        assert_eq!(tva_key("12345678"), None);
        // python-stdnum and research §1 vectors (computed there and here).
        for n in ["FR40303265045", "fr 40 303 265 045", "FR44732829320", "FR96552100554", "FR84323140392", "FR83404833048", "FR46443121975", "FR19552008443", "FR23334175221"] {
            let d = validate(n).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert!(d.value.starts_with("FR") && d.value.len() == 13, "{n}");
        }
        assert_eq!(validate("FR44732829320").unwrap().entity_id.as_deref(), Some("732829320"));
        assert_eq!(validate("FR41303265045"), Err(TaxIdError::CheckCharacter { expected: '0', found: '1' }));
        assert_eq!(validate("FR40303265046"), Err(TaxIdError::CheckCharacter { expected: '5', found: '6' }), "the SIREN's own check");
    }

    #[test]
    fn tva_format_errors_and_alphanumeric_keys() {
        assert_eq!(validate(""), Err(TaxIdError::Empty));
        assert_eq!(validate("40303265045"), Err(TaxIdError::Format { position: 1, expected: "the prefix FR" }));
        assert_eq!(validate("FR4030326504"), Err(TaxIdError::Length { expected: 13, found: 12 }));
        assert!(matches!(validate("FRI0303265045"), Err(TaxIdError::Format { position: 3, .. })), "no I in keys");
        assert!(matches!(validate("FR4030326504X"), Err(TaxIdError::Format { position: 13, .. })));
        // python-stdnum's alphanumeric examples.
        assert!(validate("FRK7399859412").is_ok());
        assert!(validate("FR4Z123456782").is_ok());
        assert!(validate("FRK7399859413").is_err());
        assert!(validate("FRIO334175221").is_err(), "no I or O in keys");
        assert!(validate("FR84323140391").is_err(), "the SIREN fails Luhn");
        for monaco in ["FR32000000330", "FR87000018746", "FR96000110899"] {
            assert!(validate(monaco).is_ok(), "{monaco}");
        }
    }
}

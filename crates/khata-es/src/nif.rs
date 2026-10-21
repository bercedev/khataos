//! NIF (número de identificación fiscal) validation: DNI, NIE, the
//! K/L/M personal numbers and the NIF of legal entities (the former CIF).
//!
//! Sources: `docs/research/spain-compliance.md` §1 (Real Decreto
//! 1065/2007 art. 18 to 22, Orden EHA/451/2008 for the entity letters; the
//! check characters as published by the Ministerio del Interior for the DNI
//! and as implemented by python-stdnum for the rest).
//!
//! - **DNI** (Spanish nationals): eight digits and a letter,
//!   `TRWAGMYFPDXBNJZSQVHLCKE`[number mod 23].
//! - **NIE** (foreigners): X, Y or Z, seven digits and a letter; X, Y and Z
//!   are read as 0, 1 and 2 and the DNI letter rule applies.
//! - **K, L, M** (minors, residents abroad, foreigners without a NIE): the
//!   letter, seven digits and the DNI letter of the seven digits.
//! - **Entities**: a letter for the kind of entity (A sociedad anónima, B
//!   sociedad de responsabilidad limitada, …), seven digits and a control
//!   character. Over the seven digits, the digits in even positions are
//!   added to the digit sums of twice the digits in odd positions; the
//!   control digit is (10 − sum mod 10) mod 10, or the letter
//!   `JABCDEFGHI`[control digit]. Either form is accepted whatever the
//!   entity letter (as python-stdnum does; research §1.3 on which entities
//!   use which form).
//!
//! The intra-EU VAT number is "ES" followed by the NIF. A pass shows the
//! number is well formed; only the AEAT (or VIES for an ES number) can say
//! it is registered.

use khata_compliance::{TaxIdDetails, TaxIdError};

/// The country prefix of the intra-EU VAT number.
pub const PREFIX: &str = "ES";

/// Characters in a NIF.
pub const LEN: usize = 9;

/// The DNI letters, by number mod 23.
pub const DNI_LETTERS: &str = "TRWAGMYFPDXBNJZSQVHLCKE";

/// The letter form of an entity's control digit.
pub const ENTITY_LETTERS: &str = "JABCDEFGHI";

/// Letters that start an entity's NIF (Orden EHA/451/2008).
pub const ENTITY_KINDS: &str = "ABCDEFGHJNPQRSUVW";

/// What kind of holder a NIF belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dni,
    Nie,
    /// K, L or M.
    Personal,
    /// A legal entity or other organisation.
    Entity,
}

/// Trim, drop spaces, dots and hyphens, upper-case letters.
pub fn normalise(id: &str) -> String {
    id.chars().filter(|c| !(c.is_whitespace() || matches!(c, '-' | '.' | '/'))).flat_map(char::to_uppercase).collect()
}

/// The DNI letter of a run of digits (at most 8). `None` on other input.
pub fn dni_letter(digits: &str) -> Option<char> {
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    DNI_LETTERS.chars().nth((n % 23) as usize)
}

/// The control digit of an entity's seven digits. `None` on other input.
pub fn entity_control(seven: &str) -> Option<u32> {
    if seven.len() != 7 || !seven.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = seven
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let d = u32::from(b - b'0');
            // Positions 1, 3, 5, 7 (counting from 1) are doubled.
            if i % 2 == 0 { (d * 2) / 10 + (d * 2) % 10 } else { d }
        })
        .sum();
    Some((10 - sum % 10) % 10)
}

/// The kind of a normalised nine-character NIF from its first character.
pub fn kind(value: &str) -> Option<Kind> {
    match value.chars().next()? {
        '0'..='9' => Some(Kind::Dni),
        'X' | 'Y' | 'Z' => Some(Kind::Nie),
        'K' | 'L' | 'M' => Some(Kind::Personal),
        c if ENTITY_KINDS.contains(c) => Some(Kind::Entity),
        _ => None,
    }
}

/// Validate a NIF offline (an "ES" prefix is allowed and dropped). The
/// value returned is the bare NIF; `entity_id` is the same.
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let mut value = normalise(id);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if value.len() == LEN + PREFIX.len() && value.starts_with(PREFIX) {
        value.drain(..PREFIX.len());
    }
    let kind = kind(&value).ok_or(TaxIdError::Format {
        position: 1,
        expected: "a digit, X, Y, Z, K, L, M or an entity letter (A, B, C, D, E, F, G, H, J, N, P, Q, R, S, U, V, W)",
    })?;
    let first_digit = if kind == Kind::Dni { 0 } else { 1 };
    if let Some(i) = value.chars().take(8).skip(first_digit).position(|c| !c.is_ascii_digit()) {
        return Err(TaxIdError::Format { position: first_digit + i + 1, expected: "a digit" });
    }
    if value.len() != LEN {
        return Err(TaxIdError::Length { expected: LEN, found: value.len() });
    }
    let found = value.chars().nth(8).expect("nine characters");
    let ok = match kind {
        Kind::Dni => dni_letter(&value[..8]).map(|c| (c == found, c)),
        Kind::Nie => {
            let lead = (b'0' + (value.as_bytes()[0] - b'X')) as char;
            dni_letter(&format!("{lead}{}", &value[1..8])).map(|c| (c == found, c))
        }
        Kind::Personal => dni_letter(&value[1..8]).map(|c| (c == found, c)),
        Kind::Entity => entity_control(&value[1..8]).map(|d| {
            let digit = char::from_digit(d, 10).expect("one digit");
            let letter = ENTITY_LETTERS.chars().nth(d as usize).expect("ten letters");
            (found == digit || found == letter, if found.is_ascii_digit() { digit } else { letter })
        }),
    };
    match ok {
        Some((true, _)) => Ok(TaxIdDetails { entity_id: Some(value.clone()), value, region: None }),
        Some((false, expected)) => Err(TaxIdError::CheckCharacter { expected, found }),
        None => Err(TaxIdError::Format { position: LEN, expected: "a check letter or digit" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_examples_pass() {
        // python-stdnum's es.dni / es.nie / es.cif / es.nif examples, the
        // AEAT's own NIF (Q2826000H) and the AEAT's VeriFactu test NIF
        // 89890001K (research §1).
        for (n, k) in [
            ("54362315-K", Kind::Dni),
            ("x-2482300w", Kind::Nie),
            ("Y1234567X", Kind::Nie),
            ("Z1234567R", Kind::Nie),
            ("K1234567L", Kind::Personal),
            ("B-58378431", Kind::Entity),
            ("ES B64717838", Kind::Entity),
            ("J99216582", Kind::Entity),
            ("Q2826000H", Kind::Entity),
            ("89890001K", Kind::Dni),
        ] {
            let d = validate(n).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert_eq!(kind(&d.value), Some(k), "{n}");
            assert_eq!(d.value.len(), 9, "{n}");
        }
        assert_eq!(validate("ESB58378431").unwrap().value, "B58378431", "the ES prefix is dropped");
    }

    #[test]
    fn check_characters() {
        assert_eq!(dni_letter("54362315"), Some('K'));
        assert_eq!(dni_letter("0"), Some('T'));
        assert_eq!(entity_control("5837843"), Some(1));
        assert_eq!(entity_control("6471783"), Some(8));
        assert_eq!(validate("54362315T"), Err(TaxIdError::CheckCharacter { expected: 'K', found: 'T' }));
        assert_eq!(validate("X2482300A"), Err(TaxIdError::CheckCharacter { expected: 'W', found: 'A' }));
        assert_eq!(validate("B58378432"), Err(TaxIdError::CheckCharacter { expected: '1', found: '2' }));
        assert_eq!(validate("Q2826000J"), Err(TaxIdError::CheckCharacter { expected: 'H', found: 'J' }));
        assert!(validate("B5837843A").is_ok(), "either form of the control is accepted");
    }

    #[test]
    fn format_errors() {
        assert_eq!(validate(""), Err(TaxIdError::Empty));
        assert!(matches!(validate("I1234567A"), Err(TaxIdError::Format { position: 1, .. })));
        assert_eq!(validate("5436231K"), Err(TaxIdError::Format { position: 8, expected: "a digit" }));
        assert_eq!(validate("543623150K"), Err(TaxIdError::Length { expected: 9, found: 10 }));
        assert_eq!(validate("B583784"), Err(TaxIdError::Length { expected: 9, found: 7 }));
    }
}

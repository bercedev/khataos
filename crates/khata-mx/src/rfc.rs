//! RFC (Registro Federal de Contribuyentes) validation.
//!
//! Format (`docs/research/mexico-compliance.md` §3):
//! - **persona moral**: 12 characters: 3 letters (`A`–`Z`, `Ñ`, `&`), the
//!   date of incorporation `YYMMDD`, and a 3-character homoclave;
//! - **persona física**: 13 characters: 4 letters, the date of birth
//!   `YYMMDD`, and the homoclave.
//!
//! The homoclave's last character is a check digit, computed as the SAT
//! published it (reply to IFAI request 0610100135506 of 18 Oct 2006,
//! Anexo III [HIGH]) over the first 12 characters, a persona moral's RFC
//! being padded with one leading space ("un espacio, tres letras y seis
//! dígitos"):
//! - each character has a value: `0`–`9` → 0–9, `A`–`N` → 10–23, `&` → 24,
//!   `O`–`Z` → 25–36, space → 37, `Ñ` → 38;
//! - the values are weighted 13, 12, … 2 and summed;
//! - with `r` = sum mod 11, the digit is `0` when `r` is 0, else `11 − r`,
//!   written `A` when it is 10 [MEDIUM: the SAT's wording for the `A` case
//!   is loose; this reading is the only consistent one, and no real RFC
//!   ending in `A` was found to test it].
//!
//! About 1.5% of RFCs in use are reported to carry a wrong check digit
//! (python-stdnum notes, research §3), so the app should let a user keep
//! an RFC that fails, with a reason (the desktop's tax-ID override).
//!
//! The generic RFCs `XAXX010101000` (público en general, whose last digit
//! does **not** pass the check) and `XEXX010101000` (residents abroad) are
//! accepted as they are (Anexo 20 guide); a pass shows a number is well formed, not that it is
//! registered (the SAT's online validator says that).
//!
//! The RFC encodes no state, so [`TaxIdDetails::region`] is `None`.

use chrono::NaiveDate;
use khata_compliance::{TaxIdDetails, TaxIdError};
use serde::Serialize;

/// The RFC for sales to the general public (factura global).
pub const PUBLICO_EN_GENERAL: &str = "XAXX010101000";
/// The RFC for customers resident abroad.
pub const EXTRANJERO: &str = "XEXX010101000";

/// Whose RFC it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Persona {
    /// A legal entity (12 characters).
    Moral,
    /// An individual (13 characters).
    Fisica,
}

/// Trim, drop spaces and hyphens people type, and upper-case (Ñ too).
pub fn normalise(rfc: &str) -> String {
    rfc.chars().filter(|c| !(c.is_whitespace() || *c == '-')).flat_map(char::to_uppercase).collect()
}

fn value(c: char) -> Option<u32> {
    Some(match c {
        '0'..='9' => c as u32 - '0' as u32,
        'A'..='N' => c as u32 - 'A' as u32 + 10,
        '&' => 24,
        'O'..='Z' => c as u32 - 'O' as u32 + 25,
        ' ' => 37,
        'Ñ' => 38,
        _ => return None,
    })
}

/// The check digit of an RFC's first 11 (moral) or 12 (física)
/// characters.
pub fn check_digit(body: &str) -> Option<char> {
    let chars: Vec<char> = body.chars().collect();
    let padded: Vec<char> = match chars.len() {
        11 => std::iter::once(' ').chain(chars).collect(),
        12 => chars,
        _ => return None,
    };
    let mut sum = 0;
    for (i, c) in padded.iter().enumerate() {
        sum += value(*c)? * (13 - i as u32);
    }
    Some(match sum % 11 {
        0 => '0',
        r => match 11 - r {
            10 => 'A',
            d => char::from_digit(d, 10).expect("a digit"),
        },
    })
}

fn is_letter(c: char) -> bool {
    c.is_ascii_uppercase() || c == 'Ñ' || c == '&'
}

/// Check an RFC and say whose it is.
pub fn classify(rfc: &str) -> Result<(Persona, String), TaxIdError> {
    let s = normalise(rfc);
    if s.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if s == PUBLICO_EN_GENERAL || s == EXTRANJERO {
        return Ok((Persona::Fisica, s));
    }
    let chars: Vec<char> = s.chars().collect();
    let (persona, letters) = match chars.len() {
        12 => (Persona::Moral, 3),
        13 => (Persona::Fisica, 4),
        found => return Err(TaxIdError::Length { expected: 13, found }),
    };
    if let Some(i) = chars[..letters].iter().position(|c| !is_letter(*c)) {
        return Err(TaxIdError::Format { position: i + 1, expected: "a letter" });
    }
    if let Some(i) = chars[letters..letters + 6].iter().position(|c| !c.is_ascii_digit()) {
        return Err(TaxIdError::Format { position: letters + i + 1, expected: "a digit of the date (YYMMDD)" });
    }
    let date: String = chars[letters..letters + 6].iter().collect();
    let (yy, mm, dd) = (date[..2].parse::<i32>().unwrap_or(0), date[2..4].parse().unwrap_or(0), date[4..].parse().unwrap_or(0));
    // Any century works for the check: 2000 is a leap year like 1900 is not,
    // so 29 Feb of "00" is accepted.
    if NaiveDate::from_ymd_opt(2000 + yy, mm, dd).is_none() {
        return Err(TaxIdError::Format { position: letters + 3, expected: "a valid date (YYMMDD)" });
    }
    let homo = &chars[letters + 6..];
    if let Some(i) = homo[..2].iter().position(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit())) {
        return Err(TaxIdError::Format { position: letters + 7 + i, expected: "a letter or digit of the homoclave" });
    }
    let found = homo[2];
    let body: String = chars[..chars.len() - 1].iter().collect();
    let expected = check_digit(&body).ok_or(TaxIdError::Format { position: 1, expected: "an RFC character" })?;
    if found != expected {
        return Err(TaxIdError::CheckCharacter { expected, found });
    }
    Ok((persona, s))
}

/// Validate an RFC for the pack.
pub fn validate(rfc: &str) -> Result<TaxIdDetails, TaxIdError> {
    let (_, value) = classify(rfc)?;
    Ok(TaxIdDetails { value, region: None, entity_id: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sat_test_rfcs_pass() {
        // SAT's CFDI 4.0 test issuer (persona moral).
        assert_eq!(classify("EKU9003173C9"), Ok((Persona::Moral, "EKU9003173C9".into())));
        assert_eq!(check_digit("EKU9003173C"), Some('9'));
        assert_eq!(classify(" xaxx010101000 ").unwrap().1, PUBLICO_EN_GENERAL);
        assert_eq!(classify("XEXX010101000").unwrap().1, EXTRANJERO);
    }

    /// Worked examples from the research document (§3): the SAT's own
    /// GODE561231GR8 and RFCs exercising `&` and `Ñ`.
    #[test]
    fn published_and_real_examples_pass() {
        assert_eq!(check_digit("GODE561231GR"), Some('8'));
        for rfc in ["GODE561231GR8", "URE180429TM6", "H&E951128469", "OÑO120726RX3", "ZUÑ920208KL4", "EWE1709045U0"] {
            assert!(classify(rfc).is_ok(), "{rfc}");
        }
        assert_eq!(classify("oño120726rx3").unwrap(), (Persona::Moral, "OÑO120726RX3".into()));
        assert_eq!(check_digit("XAXX010101000"[..12].as_ref()), Some('4'), "the generic RFC is whitelisted, not computed");
    }

    #[test]
    fn bad_rfcs_fail() {
        assert_eq!(classify(""), Err(TaxIdError::Empty));
        assert_eq!(classify("EKU9003173C8"), Err(TaxIdError::CheckCharacter { expected: '9', found: '8' }));
        assert!(matches!(classify("EKU900317"), Err(TaxIdError::Length { .. })));
        assert!(matches!(classify("E1U9003173C9"), Err(TaxIdError::Format { position: 2, .. })));
        assert!(matches!(classify("EKU9013173C9"), Err(TaxIdError::Format { position: 6, .. })), "month 13");
        assert!(matches!(classify("EKU90031_3C9"), Err(TaxIdError::Format { .. })));
    }

    #[test]
    fn check_digit_values_and_a_and_zero() {
        assert_eq!(value('Ñ'), Some(38));
        assert_eq!(value('&'), Some(24));
        assert_eq!(value('O'), Some(25));
        assert_eq!(check_digit("TOO SHORT"), None);
        // Every body has a digit in 0-9 or A, and it round-trips.
        for body in ["GODE561231GR", "AAA010101AA", "ÑAÑ0002290A", "A&A9912319Z"] {
            let c = check_digit(body).unwrap();
            assert!(c.is_ascii_digit() || c == 'A', "{body}");
            assert_eq!(classify(&format!("{body}{c}")).map(|r| r.1), Ok(format!("{body}{c}")));
        }
    }
}

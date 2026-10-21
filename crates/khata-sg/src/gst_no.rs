//! GST registration number validation.
//!
//! IRAS uses one of two kinds of number (`docs/research/singapore-compliance.md`
//! §2; IRAS *Invoicing customers* [PRIMARY]):
//! - the business's **UEN**, when IRAS registers it under its UEN; the UEN
//!   is then exposed as the entity ID;
//! - a number IRAS issues itself, shown as `M2-1234567-K` (or `M21234567K`).
//!
//! UEN shapes (IDA/MOF *Annex E – UEN format*, 2008 [PRIMARY]; the R/S/T
//! century letter [SECONDARY]):
//! - businesses registered with ACRA: `nnnnnnnnX`, 8 digits and a check letter;
//! - local companies: `yyyynnnnnX`, the year, 5 digits and a check letter;
//! - all other entities: `TyyPQnnnnX`, where `T` (or `S`, `R` for earlier
//!   centuries) and `yy` give the year of issue, `PQ` the entity type (`LL`,
//!   `FC`, `SS`…), then 4 digits and a check letter. The list of entity
//!   types is ACRA's and is not checked: `P` must be a letter, `Q` a letter
//!   or digit.
//!
//! **No check letter is validated.** The format names a "check alphabet",
//! but neither ACRA nor IRAS publishes the algorithm (research §2
//! [UNCERTAIN]), so none is invented here. The same goes for `M` numbers:
//! IRAS publishes only an example, so this module accepts `M`, a letter or
//! digit, 7 digits, then a letter or digit. That is deliberately looser than
//! the example, so a real but older number is not refused. Only an IRAS
//! online search confirms a registration.
//!
//! Singapore has no tax regions, so [`TaxIdDetails::region`] is always `None`.

use khata_compliance::{TaxIdDetails, TaxIdError};
use serde::Serialize;

/// Which shape a GST registration number has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GstNoKind {
    /// A business registered with ACRA: `nnnnnnnnX`.
    UenBusiness,
    /// A local company: `yyyynnnnnX`.
    UenLocalCompany,
    /// Any other entity (LLP, foreign company, society…): `TyyPQnnnnX`.
    UenOtherEntity,
    /// A number issued by IRAS: `M2-1234567-K`.
    IrasIssued,
}

impl GstNoKind {
    pub fn is_uen(self) -> bool {
        self != GstNoKind::IrasIssued
    }
}

/// Trim, drop the spaces and hyphens people type ("M2-1234567-K") and upper-case.
pub fn normalise(id: &str) -> String {
    id.chars().filter(|c| !(c.is_whitespace() || *c == '-')).map(|c| c.to_ascii_uppercase()).collect()
}

#[derive(Clone, Copy)]
enum Want {
    Digit,
    Letter,
    LetterOrDigit,
    Century,
}

impl Want {
    fn ok(self, c: char) -> bool {
        match self {
            Want::Digit => c.is_ascii_digit(),
            Want::Letter => c.is_ascii_uppercase(),
            Want::LetterOrDigit => c.is_ascii_digit() || c.is_ascii_uppercase(),
            Want::Century => matches!(c, 'R' | 'S' | 'T'),
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Want::Digit => "a digit",
            Want::Letter => "a letter",
            Want::LetterOrDigit => "a letter or digit",
            Want::Century => "R, S or T",
        }
    }
}

use Want::{Century, Digit, Letter, LetterOrDigit};

const BUSINESS: [Want; 9] = [Digit, Digit, Digit, Digit, Digit, Digit, Digit, Digit, Letter];
const LOCAL_COMPANY: [Want; 10] = [Digit, Digit, Digit, Digit, Digit, Digit, Digit, Digit, Digit, Letter];
const OTHER_ENTITY: [Want; 10] = [Century, Digit, Digit, Letter, LetterOrDigit, Digit, Digit, Digit, Digit, Letter];
/// `M` is checked before the shape is picked.
const IRAS_ISSUED: [Want; 10] =
    [Letter, LetterOrDigit, Digit, Digit, Digit, Digit, Digit, Digit, Digit, LetterOrDigit];

fn check(value: &[char], shape: &[Want]) -> Result<(), TaxIdError> {
    for (i, (c, want)) in value.iter().zip(shape).enumerate() {
        if !want.ok(*c) {
            return Err(TaxIdError::Format { position: i + 1, expected: want.describe() });
        }
    }
    Ok(())
}

/// The shape of a valid number, or the first problem found.
pub fn kind(id: &str) -> Result<GstNoKind, TaxIdError> {
    let value: Vec<char> = normalise(id).chars().collect();
    match value.len() {
        0 => Err(TaxIdError::Empty),
        n @ 1..=8 => Err(TaxIdError::Length { expected: 9, found: n }),
        9 => check(&value, &BUSINESS).map(|()| GstNoKind::UenBusiness),
        10 => match value[0] {
            c if c.is_ascii_digit() => check(&value, &LOCAL_COMPANY).map(|()| GstNoKind::UenLocalCompany),
            'M' => check(&value, &IRAS_ISSUED).map(|()| GstNoKind::IrasIssued),
            'R' | 'S' | 'T' => check(&value, &OTHER_ENTITY).map(|()| GstNoKind::UenOtherEntity),
            _ => Err(TaxIdError::Format { position: 1, expected: "a digit, M, R, S or T" }),
        },
        n => Err(TaxIdError::Length { expected: 10, found: n }),
    }
}

/// Validate a GST registration number offline (9 or 10 characters after
/// [`normalise`]). A UEN is also returned as the entity ID.
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let kind = kind(id)?;
    let value = normalise(id);
    Ok(TaxIdDetails { entity_id: kind.is_uen().then(|| value.clone()), region: None, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(t: &str) -> TaxIdError {
        validate(t).expect_err(t)
    }

    #[test]
    fn the_iras_examples_pass() {
        // IRAS *Invoicing customers*: "123456789A" (UEN) and "M2-1234567-K".
        let uen = validate("123456789A").unwrap();
        assert_eq!((uen.value.as_str(), uen.entity_id.as_deref(), uen.region), ("123456789A", Some("123456789A"), None));
        let m = validate("M2-1234567-K").unwrap();
        assert_eq!((m.value.as_str(), m.entity_id), ("M21234567K", None), "an IRAS number is not a UEN");
        assert_eq!(validate("m21234567k").unwrap().value, "M21234567K");
        assert_eq!(kind(" M2 1234567 K ").unwrap(), GstNoKind::IrasIssued);
    }

    #[test]
    fn the_three_uen_shapes() {
        // Annex E examples.
        assert_eq!(kind("12345678A"), Ok(GstNoKind::UenBusiness));
        assert_eq!(kind("199012345X"), Ok(GstNoKind::UenLocalCompany));
        assert_eq!(kind("T08LL1234A"), Ok(GstNoKind::UenOtherEntity));
        assert_eq!(kind("S99SS0001B"), Ok(GstNoKind::UenOtherEntity), "19xx");
        assert_eq!(kind("R88FC0001C"), Ok(GstNoKind::UenOtherEntity), "18xx");
        assert_eq!(kind("T10F10001C"), Ok(GstNoKind::UenOtherEntity), "Q may be a digit");
        assert!(kind("T08LL1234A").unwrap().is_uen());
    }

    #[test]
    fn there_is_no_check_letter() {
        for last in 'A'..='Z' {
            assert!(validate(&format!("12345678{last}")).is_ok());
            assert!(validate(&format!("T08LL1234{last}")).is_ok());
        }
    }

    #[test]
    fn rejects_bad_shapes_with_the_position() {
        assert_eq!(err(""), TaxIdError::Empty);
        assert_eq!(err(" - "), TaxIdError::Empty);
        assert_eq!(err("1234567A"), TaxIdError::Length { expected: 9, found: 8 });
        assert_eq!(err("12345678901A"), TaxIdError::Length { expected: 10, found: 12 });
        assert_eq!(err("123456789"), TaxIdError::Format { position: 9, expected: "a letter" });
        assert_eq!(err("1234A678B"), TaxIdError::Format { position: 5, expected: "a digit" });
        assert_eq!(err("1990123456"), TaxIdError::Format { position: 10, expected: "a letter" });
        assert_eq!(err("U08LL1234A"), TaxIdError::Format { position: 1, expected: "a digit, M, R, S or T" });
        assert_eq!(err("T0XLL1234A"), TaxIdError::Format { position: 3, expected: "a digit" });
        assert_eq!(err("T081L1234A"), TaxIdError::Format { position: 4, expected: "a letter" });
        assert_eq!(err("T08LL12345"), TaxIdError::Format { position: 10, expected: "a letter" });
        assert_eq!(err("M2123456XK"), TaxIdError::Format { position: 9, expected: "a digit" });
        assert_eq!(err("M2.1234567K"), TaxIdError::Length { expected: 10, found: 11 });
        assert_eq!(err("１２３４５６７８A"), TaxIdError::Format { position: 1, expected: "a digit" }, "full-width digits");
    }
}

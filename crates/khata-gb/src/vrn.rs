//! UK VAT registration number (VRN) validation.
//!
//! Formats (`docs/research/uk-compliance.md` §1.1):
//! - **9 digits**, written "123 4567 89": seven digits and two check digits
//!   [PRIMARY: HMRC VAT Software Developer Forum minutes, 10 Mar 2009];
//! - **12 digits**: a 9-digit VRN and a 3-digit branch identifier
//!   [SECONDARY]; the check digits are on the first nine;
//! - **`GD` + 000–499** (government departments) and **`HA` + 500–999**
//!   (health authorities) [SECONDARY]: no check digits.
//!
//! Each may carry the country prefix **`GB`**, or **`XI`** for a business
//! trading goods under the Windsor Framework (Northern Ireland). The XI
//! number is the same VRN with another prefix [PRIMARY: GOV.UK "Register
//! for VAT: selling or moving goods between Northern Ireland and the EU";
//! VAT Regulations 1995 reg 14(2)(b)].
//!
//! **Check digits** ([`check_digits_ok`]): HMRC applies "first Mod 97 and,
//! if that failed, Mod 9755. A 'fail' of both checks would indicate a
//! spurious VRN" (forum minutes, item 6 [PRIMARY]). The algorithm is on
//! HMRC's slides from the same forum: weight the first seven digits by
//! 8, 7, 6, 5, 4, 3, 2, subtract 97 from the sum until it is 97 or less,
//! and subtract the result from 97; the Mod 97-55 series adds 55 to the
//! sum first. HMRC's worked example is 562 2359 45 (Mod 97) and
//! 562 2359 87 (Mod 97-55). Some secondary sources say 55 is *subtracted*;
//! HMRC's example only works when it is added, so that is what is checked.
//!
//! A pass shows that a number is well formed, not that it is registered:
//! only HMRC's online "Check a UK VAT number" service can say that.
//!
//! A VRN encodes no nation, so [`TaxIdDetails::region`] is always `None`;
//! an `XI` prefix is exposed by [`is_xi`] instead (it says that the
//! business trades goods in Northern Ireland, not where it is based).

use khata_compliance::{TaxIdDetails, TaxIdError};

/// Length of a standard VRN.
pub const LEN: usize = 9;

/// Length of a branch-trader VRN (a VRN and a 3-digit branch).
pub const BRANCH_LEN: usize = 12;

/// The usual country prefix.
pub const GB: &str = "GB";

/// The Northern Ireland (Windsor Framework) prefix.
pub const XI: &str = "XI";

/// Weights of the first seven digits.
const WEIGHTS: [u32; 7] = [8, 7, 6, 5, 4, 3, 2];

/// What the Mod 97-55 series adds to the weighted sum.
const MOD_9755_OFFSET: u32 = 55;

/// Trim, drop the spaces, hyphens and dots people type to group the digits
/// ("GB 123 4567 89", "123-4567-89") and upper-case the letters.
pub fn normalise(vrn: &str) -> String {
    vrn.chars().filter(|c| !(c.is_whitespace() || *c == '-' || *c == '.')).flat_map(char::to_uppercase).collect()
}

/// The prefix of a normalised number, if it has one.
fn prefix(value: &str) -> Option<&'static str> {
    [GB, XI].into_iter().find(|p| value.starts_with(p))
}

/// The two check digits HMRC's Mod 97 (`offset` 0) or Mod 97-55 (`offset`
/// 55) gives for the first seven digits.
fn expected_check(first7: &[u32], offset: u32) -> u32 {
    let sum: u32 = first7.iter().zip(WEIGHTS).map(|(d, w)| d * w).sum::<u32>() + offset;
    (97 - sum % 97) % 97
}

/// The two check digits of a nine-digit body under each scheme:
/// `(mod 97, mod 97-55)`. `None` unless `digits` is nine ASCII digits.
pub fn check_digits(digits: &str) -> Option<(u32, u32)> {
    if digits.len() != LEN || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let d: Vec<u32> = digits.bytes().map(|b| u32::from(b - b'0')).collect();
    Some((expected_check(&d[..7], 0), expected_check(&d[..7], MOD_9755_OFFSET)))
}

/// Whether nine digits carry valid check digits under Mod 97 or Mod 97-55.
pub fn check_digits_ok(digits: &str) -> bool {
    check_digits(digits).is_some_and(|(m97, m9755)| {
        let found: u32 = digits[7..].parse().unwrap_or(u32::MAX);
        found == m97 || found == m9755
    })
}

/// Validate a VRN offline. Returns the normalised number (prefix kept, as
/// typed) with the VRN proper as `entity_id` (nine digits, without a
/// branch; or the `GD`/`HA` code), or the first problem found.
pub fn validate(vrn: &str) -> Result<TaxIdDetails, TaxIdError> {
    let value = normalise(vrn);
    if value.is_empty() {
        return Err(TaxIdError::Empty);
    }
    let skip = prefix(&value).map_or(0, str::len);
    let body = &value[skip..];
    let entity = if body.starts_with("GD") || body.starts_with("HA") {
        government(body, skip)?
    } else {
        let found = body.chars().count();
        if let Some(i) = body.chars().position(|c| !c.is_ascii_digit()) {
            let expected = if i == 0 && skip == 0 { "a digit, or the prefix GB or XI" } else { "a digit" };
            return Err(TaxIdError::Format { position: skip + i + 1, expected });
        }
        if found != LEN && found != BRANCH_LEN {
            return Err(TaxIdError::Length { expected: skip + LEN, found: skip + found });
        }
        if !check_digits_ok(&body[..LEN]) {
            return Err(TaxIdError::Format {
                position: skip + 8,
                expected: "check digits matching the first seven (HMRC modulus 97 or 97-55)",
            });
        }
        body[..LEN].to_owned()
    };
    Ok(TaxIdDetails { entity_id: Some(entity), region: None, value })
}

/// `GD` + 000–499 or `HA` + 500–999; `skip` is the prefix length.
fn government(body: &str, skip: usize) -> Result<String, TaxIdError> {
    let found = body.chars().count();
    if found != 5 {
        return Err(TaxIdError::Length { expected: skip + 5, found: skip + found });
    }
    if let Some(i) = body[2..].chars().position(|c| !c.is_ascii_digit()) {
        return Err(TaxIdError::Format { position: skip + 3 + i, expected: "a digit" });
    }
    let n: u32 = body[2..].parse().expect("three digits");
    match (&body[..2], n) {
        ("GD", 0..=499) | ("HA", 500..=999) => Ok(body.to_owned()),
        ("GD", _) => Err(TaxIdError::Format { position: skip + 3, expected: "a number from 000 to 499 after GD" }),
        _ => Err(TaxIdError::Format { position: skip + 3, expected: "a number from 500 to 999 after HA" }),
    }
}

/// The VRN proper (nine digits, or the GD/HA code) of a valid number: what
/// the MTD API takes as `{vrn}`.
pub fn vrn(id: &str) -> Option<String> {
    validate(id).ok().and_then(|d| d.entity_id)
}

/// The 3-digit branch identifier of a valid 12-digit number.
pub fn branch(id: &str) -> Option<String> {
    let value = validate(id).ok()?.value;
    let body = &value[prefix(&value).map_or(0, str::len)..];
    (body.len() == BRANCH_LEN).then(|| body[LEN..].to_owned())
}

/// Whether a number carries the Northern Ireland `XI` prefix.
pub fn is_xi(id: &str) -> bool {
    normalise(id).starts_with(XI)
}

/// "GB 123 4567 89" / "XI 123 4567 89 001" / "GBGD001": the grouping HMRC
/// prints, with the prefix (GB when none was typed). `None` when invalid.
pub fn display(id: &str) -> Option<String> {
    let value = validate(id).ok()?.value;
    let p = prefix(&value).unwrap_or(GB);
    let body = &value[prefix(&value).map_or(0, str::len)..];
    Some(match body.len() {
        LEN => format!("{p} {} {} {}", &body[..3], &body[3..7], &body[7..]),
        BRANCH_LEN => format!("{p} {} {} {} {}", &body[..3], &body[3..7], &body[7..9], &body[9..]),
        _ => format!("{p}{body}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(t: &str) -> TaxIdError {
        validate(t).expect_err(t)
    }

    /// HMRC's own worked example (VAT Software Developer Forum slides,
    /// 10 Mar 2009): the same seven digits under each scheme.
    #[test]
    fn hmrcs_worked_example_passes_both_schemes() {
        assert_eq!(check_digits("562235945"), Some((45, 87)));
        assert!(check_digits_ok("562235945"), "Mod 97");
        assert!(check_digits_ok("562235987"), "Mod 97-55");
        assert!(!check_digits_ok("562235946"));
        // The "55 subtracted" reading would give 03; HMRC's example rules it out.
        assert!(!check_digits_ok("562235903"));
    }

    #[test]
    fn other_published_mod_97_numbers_pass() {
        // taxationweb.co.uk's worked example and python-stdnum's docstring.
        for n in ["339072747", "980780684"] {
            assert!(check_digits_ok(n), "{n}");
        }
    }

    #[test]
    fn the_sample_on_notice_700s_invoice_is_illustrative_only() {
        // "VAT No: 987 6543 21" (Notice 700 §16.7): 9·8+8·7+7·6+6·5+5·4+4·3+3·2 = 238.
        assert_eq!(check_digits("987654321"), Some((53, 95)));
        assert!(validate("987 6543 21").is_err());
    }

    #[test]
    fn check_digits_97_to_99_never_pass() {
        // A sum divisible by 97 gives 00 under Mod 97, never 97.
        for first7 in ["0000000", "1234567", "9999999", "5622359"] {
            for check in 97..=99 {
                assert!(!check_digits_ok(&format!("{first7}{check}")), "{first7}{check}");
            }
        }
        assert!(check_digits_ok("000000000"), "sum 0: Mod 97 gives 00 (format only; not a real VRN)");
    }

    #[test]
    fn accepts_prefixes_spaces_and_case() {
        for typed in ["562235945", "GB562235945", "gb 562 2359 45", " GB-562-2359-45 ", "562.2359.45"] {
            let d = validate(typed).unwrap();
            assert_eq!(d.entity_id.as_deref(), Some("562235945"), "{typed:?}");
            assert_eq!(d.region, None);
        }
        assert_eq!(validate("gb 562 2359 45").unwrap().value, "GB562235945");
        assert_eq!(validate("562 2359 45").unwrap().value, "562235945");
        let xi = validate("xi 562 2359 87").unwrap();
        assert_eq!((xi.value.as_str(), xi.entity_id.as_deref()), ("XI562235987", Some("562235987")));
        assert!(is_xi("xi 562 2359 87") && !is_xi("GB562235987"));
        assert_eq!(vrn("XI 562 2359 87").as_deref(), Some("562235987"));
    }

    #[test]
    fn branch_numbers_check_the_first_nine_digits() {
        let d = validate("GB 562 2359 45 001").unwrap();
        assert_eq!((d.value.as_str(), d.entity_id.as_deref()), ("GB562235945001", Some("562235945")));
        assert_eq!(branch("562235945001").as_deref(), Some("001"));
        assert_eq!(branch("562235945"), None);
        assert!(validate("562235946001").is_err());
    }

    #[test]
    fn government_and_health_authority_numbers() {
        assert_eq!(validate("GD001").unwrap().entity_id.as_deref(), Some("GD001"));
        assert_eq!(validate("gbgd499").unwrap().value, "GBGD499");
        assert_eq!(validate("HA500").unwrap().entity_id.as_deref(), Some("HA500"));
        assert_eq!(validate("HA999").unwrap().value, "HA999");
        assert_eq!(err("GD500"), TaxIdError::Format { position: 3, expected: "a number from 000 to 499 after GD" });
        assert_eq!(err("GBHA499"), TaxIdError::Format { position: 5, expected: "a number from 500 to 999 after HA" });
        assert_eq!(err("GD1"), TaxIdError::Length { expected: 5, found: 3 });
        assert_eq!(err("GD12X"), TaxIdError::Format { position: 5, expected: "a digit" });
    }

    #[test]
    fn rejects_empty_wrong_length_letters_and_bad_check_digits() {
        assert_eq!(err(""), TaxIdError::Empty);
        assert_eq!(err(" - "), TaxIdError::Empty);
        assert_eq!(err("56223594"), TaxIdError::Length { expected: 9, found: 8 });
        assert_eq!(err("GB5622359450"), TaxIdError::Length { expected: 11, found: 12 });
        assert_eq!(err("FR562235945"), TaxIdError::Format { position: 1, expected: "a digit, or the prefix GB or XI" });
        assert_eq!(err("GB56223594A"), TaxIdError::Format { position: 11, expected: "a digit" });
        assert_eq!(
            err("GB 562 2359 46"),
            TaxIdError::Format { position: 10, expected: "check digits matching the first seven (HMRC modulus 97 or 97-55)" }
        );
        assert_eq!(err("٥٦٢٢٣٥٩٤٥"), TaxIdError::Format { position: 1, expected: "a digit, or the prefix GB or XI" });
    }

    #[test]
    fn displays_in_hmrcs_grouping() {
        assert_eq!(display("562235945").as_deref(), Some("GB 562 2359 45"));
        assert_eq!(display("xi562235987").as_deref(), Some("XI 562 2359 87"));
        assert_eq!(display("562235945001").as_deref(), Some("GB 562 2359 45 001"));
        assert_eq!(display("HA500").as_deref(), Some("GBHA500"));
        assert_eq!(display("123"), None);
    }
}

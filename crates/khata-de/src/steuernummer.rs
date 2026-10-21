//! Steuernummer (the tax number a Finanzamt gives a business), accepted on
//! invoices instead of the USt-IdNr (§14 Abs. 4 Nr. 2 UStG).
//!
//! Source: ELSTER, *Prüfung der Steuer- und Steueridentifikationsnummer*
//! (Stand 02.09.2026), §3 to §7 [PRIMARY]; `docs/research/germany-compliance.md`
//! §1.3. Two written forms exist:
//! - the printed form of the state's tax notices, 10 or 11 digits with the
//!   state's grouping ("FF/BBB/UUUUP" in Berlin, "FFF/BBB/UUUUP" in
//!   Bayern…), and
//! - the 13-digit ELSTER form `FFFF0BBBUUUUP` (NRW `FFFF0BBBBUUUP`): the
//!   4-digit Bundesfinanzamtsnummer starting with the state's prefix, a 0,
//!   district (Bezirk), distinguishing number and check digit.
//!
//! Checked: 13 digits, a known state prefix, the 0 in position 5, the
//! Bezirk rules (not 000/998/999, NRW 0000/0998/0999; at least 100 in BY,
//! BB, MV, SL, SN, ST, TH; NRW `UUUP` above 0009) and the state's check
//! digit (11er, NRW 11er, modified 11er for RP, 2er for BW/HE/SH). Not
//! checked: that the Finanzamt exists (ELSTER's Finanzamt list), and
//! Berlin's per-Finanzamt choice of method A or B (ELSTER §7.2): a Berlin
//! number passes when either method gives its check digit.
//!
//! A printed number fits several states, so it is accepted when it fits
//! some state's printed form and passes that state's check; its Bundesland
//! is reported only when exactly one state fits.

use khata_compliance::{Region, TaxIdDetails, TaxIdError};

use crate::laender;

/// The check-digit method of a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Weighted sum mod 11 with these weights over positions 1 to 12.
    Elfer([u32; 12]),
    /// Berlin: method A or B depending on Finanzamt and Bezirk.
    Berlin,
    /// Nordrhein-Westfalen's 11er.
    Nrw,
    /// Rheinland-Pfalz's modified 11er.
    Rp,
    /// The 2er method (BW, HE, SH).
    Zweier,
}

const W_BY: [u32; 12] = [0, 5, 4, 3, 0, 2, 7, 6, 5, 4, 3, 2];
const W_HB: [u32; 12] = [0, 0, 4, 3, 0, 2, 7, 6, 5, 4, 3, 2];
const W_NI: [u32; 12] = [0, 0, 2, 9, 0, 8, 7, 6, 5, 4, 3, 2];
const W_BERLIN_A: [u32; 12] = [0, 0, 0, 0, 0, 7, 6, 5, 8, 4, 3, 2];
const W_NRW: [u32; 12] = [0, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1];
const W_RP: [u32; 12] = [0, 0, 1, 2, 0, 1, 2, 1, 2, 1, 2, 1];
const SUMMAND_2ER: [u32; 12] = [0, 0, 9, 8, 0, 7, 6, 5, 4, 3, 2, 1];
const FACTOR_2ER: [u32; 12] = [0, 0, 512, 256, 0, 128, 64, 32, 16, 8, 4, 2];

/// One state's Steuernummer forms and method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    /// The Bundesland code ("BY").
    pub land: &'static str,
    /// The printed form; F Finanzamt, B Bezirk, U distinguishing number,
    /// P check digit; other digits are fixed.
    pub printed: &'static str,
    /// The 13-digit form's prefix: the first one or two digits.
    pub elster_prefix: &'static str,
    pub method: Method,
    /// The Bezirk must be at least 100.
    pub bezirk_from_100: bool,
}

const fn f(land: &'static str, printed: &'static str, elster_prefix: &'static str, method: Method, bezirk_from_100: bool) -> Format {
    Format { land, printed, elster_prefix, method, bezirk_from_100 }
}

/// Every state's forms (ELSTER Tabelle 4-1).
pub const FORMATS: &[Format] = &[
    f("BW", "FFBBB/UUUUP", "28", Method::Zweier, false),
    f("BY", "FFF/BBB/UUUUP", "9", Method::Elfer(W_BY), true),
    f("BE", "FF/BBB/UUUUP", "11", Method::Berlin, false),
    f("BB", "0FF/BBB/UUUUP", "30", Method::Elfer(W_BY), true),
    f("HB", "FF BBB UUUUP", "24", Method::Elfer(W_HB), false),
    f("HH", "FF/BBB/UUUUP", "22", Method::Elfer(W_HB), false),
    f("HE", "0FF BBB UUUUP", "26", Method::Zweier, false),
    f("MV", "0FF/BBB/UUUUP", "40", Method::Elfer(W_BY), true),
    f("NI", "FF/BBB/UUUUP", "23", Method::Elfer(W_NI), false),
    f("NW", "FFF/BBBB/UUUP", "5", Method::Nrw, false),
    f("RP", "FF/BBB/UUUUP", "27", Method::Rp, false),
    f("SL", "0FF/BBB/UUUUP", "10", Method::Elfer(W_BY), true),
    f("SN", "2FF/BBB/UUUUP", "32", Method::Elfer(W_BY), true),
    f("ST", "1FF/BBB/UUUUP", "31", Method::Elfer(W_BY), true),
    f("SH", "FF/BBB/UUUUP", "21", Method::Zweier, false),
    f("TH", "1FF/BBB/UUUUP", "41", Method::Elfer(W_BY), true),
];

/// The format of a Bundesland.
pub fn format(land: &str) -> Option<&'static Format> {
    let l = laender::land(land)?;
    FORMATS.iter().find(|f| f.land == l.code)
}

/// The digits of a Steuernummer as typed (slashes, spaces dropped).
pub fn digits(id: &str) -> String {
    id.chars().filter(|c| !(c.is_whitespace() || matches!(c, '/' | '-' | '.'))).collect()
}

/// The state whose 13-digit prefix a number starts with. No two-digit
/// prefix starts with 5 or 9, the one-digit prefixes of NRW and Bayern.
pub fn land_of_elster(thirteen: &str) -> Option<&'static Format> {
    FORMATS.iter().find(|f| thirteen.starts_with(f.elster_prefix))
}

fn elfer(d: &[u32], w: &[u32; 12]) -> Option<u32> {
    let s: u32 = d.iter().zip(w).map(|(a, b)| a * b).sum();
    let p = if s % 11 == 0 { 0 } else { 11 - s % 11 };
    (p < 10).then_some(p)
}

fn digit_root(n: u32) -> u32 {
    if n == 0 { 0 } else { 1 + (n - 1) % 9 }
}

/// The check digit(s) a state's method gives for the first twelve digits
/// of an ELSTER number; Berlin gives two candidates (methods A and B).
fn expected(fmt: &Format, d: &[u32]) -> Vec<u32> {
    match fmt.method {
        Method::Elfer(w) => elfer(d, &w).into_iter().collect(),
        Method::Berlin => [elfer(d, &W_BERLIN_A), elfer(d, &W_NI)].into_iter().flatten().collect(),
        Method::Nrw => {
            let p = d.iter().zip(W_NRW).map(|(a, b)| a * b).sum::<u32>() % 11;
            if p < 10 { vec![p] } else { vec![] }
        }
        Method::Rp => {
            let s: u32 = d.iter().zip(W_RP).map(|(a, b)| a * b).map(|p| if p >= 10 { p % 10 + 1 } else { p }).sum();
            vec![(10 - s % 10) % 10]
        }
        Method::Zweier => {
            let s: u32 = (0..12)
                .filter(|&i| FACTOR_2ER[i] > 0)
                .map(|i| digit_root(((d[i] + SUMMAND_2ER[i]) % 10) * FACTOR_2ER[i]))
                .sum();
            vec![if s % 10 == 0 { 0 } else { 10 - s % 10 }]
        }
    }
}

/// Check a 13-digit ELSTER number; returns its state.
pub fn check_elster(thirteen: &str) -> Result<&'static Format, TaxIdError> {
    if thirteen.len() != 13 || !thirteen.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TaxIdError::Length { expected: 13, found: thirteen.chars().count() });
    }
    let fmt = land_of_elster(thirteen).ok_or(TaxIdError::Format {
        position: 1,
        expected: "a Bundesland prefix of the 13-digit Steuernummer (28, 9, 11, 30, 24, 22, 26, 40, 23, 5, 27, 10, 32, 31, 21 or 41)",
    })?;
    if thirteen.as_bytes()[4] != b'0' {
        return Err(TaxIdError::Format { position: 5, expected: "0 in the 13-digit Steuernummer" });
    }
    let nrw = fmt.land == "NW";
    let bezirk: u32 = thirteen[5..if nrw { 9 } else { 8 }].parse().expect("digits");
    let bad_bezirk = matches!(bezirk, 0 | 998 | 999) || (fmt.bezirk_from_100 && bezirk < 100);
    if bad_bezirk {
        return Err(TaxIdError::Format {
            position: 6,
            expected: "a Bezirk other than 000, 998 and 999 (at least 100 in BY, BB, MV, SL, SN, ST and TH)",
        });
    }
    if nrw && thirteen[9..].parse::<u32>().expect("digits") <= 9 {
        return Err(TaxIdError::Format { position: 10, expected: "a distinguishing number above 0009 (Nordrhein-Westfalen)" });
    }
    let d: Vec<u32> = thirteen.bytes().map(|b| u32::from(b - b'0')).collect();
    let candidates = expected(fmt, &d[..12]);
    let found = thirteen.as_bytes()[12] as char;
    if !candidates.iter().any(|&p| p == d[12]) {
        let expected = candidates.first().and_then(|&p| char::from_digit(p, 10)).unwrap_or('?');
        return Err(TaxIdError::CheckCharacter { expected, found });
    }
    Ok(fmt)
}

/// The 13-digit ELSTER form of a printed number in a given state: the
/// state's prefix, the Finanzamt digits, a 0, and the last eight digits.
/// `None` when the digits do not fit the state's printed form.
pub fn to_elster(id: &str, land: &str) -> Option<String> {
    let fmt = format(land)?;
    let d = digits(id);
    let pattern: String = fmt.printed.chars().filter(|c| !matches!(c, '/' | ' ')).collect();
    if d.len() != pattern.len() || !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Fixed digits in the pattern (a leading 0, 1 or 2) must match.
    if pattern.bytes().zip(d.bytes()).any(|(p, c)| p.is_ascii_digit() && p != c) {
        return None;
    }
    let fa: String = pattern.chars().zip(d.chars()).filter(|(p, _)| *p == 'F').map(|(_, c)| c).collect();
    let out = format!("{}{fa}0{}", fmt.elster_prefix, &d[d.len() - 8..]);
    (out.len() == 13).then_some(out)
}

/// The states whose printed form and check digit a printed number fits.
pub fn printed_lands(id: &str) -> Vec<&'static Format> {
    FORMATS.iter().filter(|f| to_elster(id, f.land).is_some_and(|e| check_elster(&e).is_ok())).collect()
}

/// Whether a normalised value is a Steuernummer attempt: digits only, 10
/// to 13 of them.
pub fn looks_like(value: &str) -> bool {
    (10..=13).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit())
}

fn region(fmt: &Format) -> Region {
    let land = laender::land(fmt.land).expect("known Bundesland");
    Region { code: land.code, name: land.name }
}

/// Validate a Steuernummer offline. `value` keeps the digits without
/// separators; `entity_id` is the 13-digit ELSTER form when it is known
/// (a 13-digit number, or a printed one that fits exactly one state).
pub fn validate(id: &str) -> Result<TaxIdDetails, TaxIdError> {
    let d = digits(id);
    if d.is_empty() {
        return Err(TaxIdError::Empty);
    }
    if let Some(i) = d.chars().position(|c| !c.is_ascii_digit()) {
        return Err(TaxIdError::Format { position: i + 1, expected: "a digit" });
    }
    match d.len() {
        10 | 11 => {
            let lands = printed_lands(&d);
            match lands.as_slice() {
                [] => Err(TaxIdError::Format {
                    position: 1,
                    expected: "a Steuernummer in a Bundesland's printed form with its check digit (or the 13-digit ELSTER form)",
                }),
                [one] => Ok(TaxIdDetails { value: d.clone(), region: Some(region(one)), entity_id: to_elster(&d, one.land) }),
                _ => Ok(TaxIdDetails { value: d, region: None, entity_id: None }),
            }
        }
        13 => {
            let fmt = check_elster(&d)?;
            Ok(TaxIdDetails { value: d.clone(), region: Some(region(fmt)), entity_id: Some(d) })
        }
        n => Err(TaxIdError::Length { expected: if n < 10 { 10 } else { 13 }, found: n }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ELSTER's test numbers (Tabelle 4-1), 13-digit and printed.
    const ELSTER: &[(&str, &str, &str)] = &[
        ("BW", "2866081508156", "66815/08156"),
        ("BY", "9198081508152", "198/815/08152"),
        ("BE", "1197081508154", "97/815/08154"),
        ("BB", "3098081508157", "098/815/08157"),
        ("HB", "2497012301233", "97 123 01233"),
        ("HH", "2293081508150", "93/815/08150"),
        ("HE", "2653081508158", "053 815 08158"),
        ("MV", "4098081508157", "098/815/08157"),
        ("NI", "2388081508158", "88/815/08158"),
        ("NW", "5400081508159", "400/8150/8159"),
        ("RP", "2799081508152", "99/815/08152"),
        ("SL", "1096081508187", "096/815/08187"),
        ("SN", "3248081508156", "248/815/08156"),
        ("ST", "3198081508152", "198/815/08152"),
        ("SH", "2138081508154", "38/815/08154"),
        ("TH", "4198081508152", "198/815/08152"),
    ];

    #[test]
    fn a_format_for_every_state() {
        assert_eq!(FORMATS.len(), 16);
        for l in laender::LAENDER {
            let f = format(l.code).unwrap_or_else(|| panic!("{}", l.code));
            let pattern: String = f.printed.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            assert!(matches!(pattern.len(), 10 | 11), "{}", l.code);
            assert_eq!(f.elster_prefix.len() + pattern.chars().filter(|c| *c == 'F').count(), 4, "{}: four Finanzamt digits", l.code);
        }
    }

    #[test]
    fn elsters_test_numbers_pass_in_both_forms() {
        for (land, thirteen, printed) in ELSTER {
            assert_eq!(check_elster(thirteen).map(|f| f.land), Ok(*land), "{thirteen}");
            assert_eq!(to_elster(printed, land).as_deref(), Some(*thirteen), "{printed}");
            let d = validate(thirteen).unwrap();
            assert_eq!(d.region.map(|r| r.code), Some(*land));
            assert!(validate(printed).is_ok(), "{printed}");
            assert!(printed_lands(printed).iter().any(|f| f.land == *land), "{printed} fits {land}");
        }
    }

    #[test]
    fn wrong_check_digits_and_rules() {
        assert_eq!(validate("9198081508153"), Err(TaxIdError::CheckCharacter { expected: '2', found: '3' }));
        assert_eq!(validate("9198181508152"), Err(TaxIdError::Format { position: 5, expected: "0 in the 13-digit Steuernummer" }));
        assert!(matches!(validate("6198081508152"), Err(TaxIdError::Format { position: 1, .. })));
        assert!(matches!(validate("9198009908152"), Err(TaxIdError::Format { position: 6, .. })), "Bayern: Bezirk at least 100");
        assert!(matches!(validate("5400081500009"), Err(TaxIdError::Format { position: 10, .. })), "NRW: UUUP above 0009");
        for (_, thirteen, _) in ELSTER {
            let mut b = thirteen.as_bytes().to_vec();
            b[11] = if b[11] == b'9' { b'0' } else { b[11] + 1 };
            let changed = String::from_utf8(b).unwrap();
            assert!(validate(&changed).is_err() || thirteen.starts_with("11"), "{changed}: one changed digit is caught");
        }
        assert_eq!(validate("12/345"), Err(TaxIdError::Length { expected: 10, found: 5 }));
        assert_eq!(validate("123456789012"), Err(TaxIdError::Length { expected: 13, found: 12 }));
        assert_eq!(validate("21/815/0815X"), Err(TaxIdError::Format { position: 10, expected: "a digit" }));
        assert_eq!(validate(" / "), Err(TaxIdError::Empty));
    }

    #[test]
    fn printed_numbers_fit_states() {
        assert_eq!(to_elster("113 815 08153", "HE"), None, "the fixed leading 0 must be there");
        assert_eq!(to_elster("21/815/0815", "BE"), None);
        assert_eq!(to_elster("21/815/08150", "XX"), None);
        let nrw = validate("400/8150/8159").unwrap();
        assert_eq!((nrw.region.map(|r| r.code), nrw.entity_id.as_deref()), (Some("NW"), Some("5400081508159")), "only NRW groups 3/4/4");
        assert!(validate("198/815/08153").is_err(), "no state's check digit fits");
    }
}

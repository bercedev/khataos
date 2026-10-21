//! Text clean-up: encodings, stray control characters, Tally amounts and dates.

use std::str::FromStr;

use chrono::NaiveDate;
use khata_core::{Amount, Quantity};

/// Decode UTF-8 or UTF-16 (LE/BE, with or without BOM). Tally writes
/// UTF-16 LE for Unicode exports and over HTTP when asked in UTF-16.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let utf16 = |le: bool, body: &[u8]| {
        let units = body
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) });
        char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(true, rest),
        [0xFE, 0xFF, rest @ ..] => utf16(false, rest),
        [a, 0, ..] if *a != 0 => utf16(true, bytes),
        [0, b, ..] if *b != 0 => utf16(false, bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Characters XML 1.0 allows in a document.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// Drop control characters, raw or as character references such as the
/// `&#4;` Tally puts before reserved names ("&#4; Primary"), which XML 1.0
/// forbids and strict parsers reject. Also drops a leading BOM.
pub(crate) fn sanitize(s: &str) -> String {
    let s = s.strip_prefix('\u{FEFF}').unwrap_or(s);
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("&#") {
        out.extend(rest[..i].chars().filter(|&c| is_xml_char(c)));
        rest = &rest[i..];
        let end = rest.find(';').filter(|&e| e <= 12);
        let code = end.and_then(|e| {
            let num = &rest[2..e];
            match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => num.parse().ok(),
            }
        });
        match (end, code) {
            (Some(e), Some(code)) if !char::from_u32(code).is_some_and(is_xml_char) => rest = &rest[e + 1..],
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.extend(rest.chars().filter(|&c| is_xml_char(c)));
    out
}

/// Parse a Tally amount, keeping Tally's sign (negative = debit).
///
/// Accepts plain numbers ("-1180.00"), Indian or Western digit grouping,
/// currency symbols ("-₹ 1,180.00"), "Dr"/"Cr" suffixes ("1,180.00 Dr" is a
/// debit), accounting parentheses, and forex forms
/// ("-$10.00 @ ₹ 83.00/$ = -₹ 830.00", where the base-currency value after
/// the last `=` is used). Blank means zero. `None` when it is not a number.
pub(crate) fn tally_amount(raw: &str) -> Option<Amount> {
    let mut s = raw.trim();
    if let Some((_, base)) = s.rsplit_once('=') {
        s = base.trim();
    }
    if s.is_empty() {
        return Some(Amount::ZERO);
    }
    // ASCII lowercasing keeps byte offsets, so `lower` indexes `s`.
    let lower = s.to_ascii_lowercase();
    let mut side = None;
    for tag in ["dr", "cr"] {
        let tail = lower.trim_end_matches('.').trim_end();
        if tail.ends_with(tag) {
            side = Some(tag);
            s = &s[..tail.len() - 2];
            break;
        }
        if lower.starts_with(tag) {
            side = Some(tag);
            s = &s[2..];
            break;
        }
    }
    let s = s.trim();
    let paren = s.starts_with('(') && s.ends_with(')');
    let cleaned: String = s.chars().filter(|c| matches!(c, '0'..='9' | '.' | '-' | '+')).collect();
    let misplaced_sign = cleaned.get(1..).is_some_and(|t| t.contains(['-', '+']));
    if misplaced_sign || !cleaned.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = Amount::from_str(&cleaned).ok()?;
    Some(match side {
        Some("dr") => -value.abs(),
        Some(_) => value.abs(),
        None if paren => -value.abs(),
        None => value,
    })
}

/// Parse a Tally quantity such as " 10 Nos", "2.500 kg", "1,200 Nos" or
/// "10 Nos = 1 Box" (alternate units: the base-unit figure before `=` is
/// used). Returns the number, normalized (no trailing zeros), and the unit
/// symbol written after it. Blank means zero. `None` when it is not a
/// number, or for a compound-unit quantity like "2 Box 5 Nos".
pub(crate) fn tally_qty(raw: &str) -> Option<(Quantity, String)> {
    let s = raw.split('=').next().unwrap_or("").trim();
    if s.is_empty() {
        return Some((Quantity::ZERO, String::new()));
    }
    let end = s.find(|c: char| !matches!(c, '0'..='9' | '.' | ',' | '-' | '+')).unwrap_or(s.len());
    let (num, unit) = s.split_at(end);
    let cleaned: String = num.chars().filter(|&c| c != ',').collect();
    if !cleaned.bytes().any(|b| b.is_ascii_digit()) || cleaned.get(1..).is_some_and(|t| t.contains(['-', '+'])) {
        return None;
    }
    let qty = Quantity::from_str(&cleaned).ok()?.normalize();
    let unit = unit.trim();
    let numeric = |t: &str| {
        t.chars().next().is_some_and(|c| c.is_ascii_digit()) && Quantity::from_str(&t.replace(',', "")).is_ok()
    };
    if unit.split_whitespace().any(numeric) {
        return None;
    }
    Some((qty, unit.to_owned()))
}

/// Parse a Tally rate such as "1,000.00/Nos" or "₹ 50/kg": the amount
/// before the `/unit`, as a positive number.
pub(crate) fn tally_rate(raw: &str) -> Option<Amount> {
    let price = raw.rsplit_once('/').map_or(raw, |(p, _)| p);
    tally_amount(price).map(|a| a.abs())
}

/// Tally dates: "20240401" in XML; also the "1-Apr-2024" and "01/04/2024"
/// forms some hand-made files use.
pub(crate) fn tally_date(raw: &str) -> Option<NaiveDate> {
    let s = raw.trim();
    ["%Y%m%d", "%d-%b-%Y", "%d-%b-%y", "%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(s, f).ok())
}

/// A due date from Tally's `BILLCREDITPERIOD`: a credit period in days
/// after `from` ("30 Days", "30"), or a due date written out
/// ("15-May-2024", "20240515"). `None` when blank or unreadable.
pub(crate) fn tally_due(raw: &str, from: NaiveDate) -> Option<NaiveDate> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(d) = tally_date(s) {
        return Some(d);
    }
    let lower = s.to_ascii_lowercase();
    let days = lower.trim_end_matches('s').trim_end_matches("day").trim();
    days.parse::<u64>().ok().and_then(|n| from.checked_add_days(chrono::Days::new(n)))
}

/// Tally logical values are "Yes"/"No".
pub(crate) fn tally_bool(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "yes" | "true" | "1")
}

/// Compare names ignoring case, spaces and punctuation, so "Cash in Hand"
/// finds "Cash-in-Hand" and "Suspense A/c" normalizes predictably.
pub(crate) fn normalize(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn amounts_keep_tally_sign() {
        assert_eq!(tally_amount("-1180.00"), Some(dec!(-1180)));
        assert_eq!(tally_amount(" 1180.50 "), Some(dec!(1180.50)));
        assert_eq!(tally_amount("-₹ 1,18,000.00"), Some(dec!(-118000)));
        assert_eq!(tally_amount("₹ 1,180.00 Dr"), Some(dec!(-1180)));
        assert_eq!(tally_amount("1,180.00 Cr"), Some(dec!(1180)));
        assert_eq!(tally_amount("1180 Dr."), Some(dec!(-1180)));
        assert_eq!(tally_amount("Dr 25"), Some(dec!(-25)));
        assert_eq!(tally_amount("(500.00)"), Some(dec!(-500)));
        assert_eq!(tally_amount("-$10.00 @ ₹ 83.00/$ = -₹ 830.00"), Some(dec!(-830)));
        assert_eq!(tally_amount(""), Some(Amount::ZERO));
        assert_eq!(tally_amount("0.1"), Some(dec!(0.1)), "exact decimal, no float rounding");
        assert_eq!(tally_amount("abc"), None);
        assert_eq!(tally_amount("12-34"), None);
        assert_eq!(tally_amount("-10.00 @ 83.00"), None);
    }

    #[test]
    fn quantities_and_rates() {
        assert_eq!(tally_qty(" 10 Nos"), Some((dec!(10), "Nos".into())));
        assert_eq!(tally_qty("2.500 kg"), Some((dec!(2.5), "kg".into())));
        assert_eq!(tally_qty("1,200.00 Nos"), Some((dec!(1200), "Nos".into())));
        assert_eq!(tally_qty("-3 Nos"), Some((dec!(-3), "Nos".into())));
        assert_eq!(tally_qty(" 10 Nos =  1 Box"), Some((dec!(10), "Nos".into())));
        assert_eq!(tally_qty("5"), Some((dec!(5), String::new())));
        assert_eq!(tally_qty(""), Some((Quantity::ZERO, String::new())));
        assert_eq!(tally_qty("2 Box 5 Nos"), None, "compound-unit quantity");
        assert_eq!(tally_qty("many"), None);
        assert_eq!(tally_qty("1.50 Nos").unwrap().0.scale(), 1, "normalized");
        assert_eq!(tally_rate("1,000.00/Nos"), Some(dec!(1000)));
        assert_eq!(tally_rate("-₹ 50.50/kg"), Some(dec!(50.50)));
        assert_eq!(tally_rate("12"), Some(dec!(12)));
        assert_eq!(tally_rate("x/Nos"), None);
    }

    #[test]
    fn dates() {
        assert_eq!(tally_date("20240401"), NaiveDate::from_ymd_opt(2024, 4, 1));
        assert_eq!(tally_date("1-Apr-2024"), NaiveDate::from_ymd_opt(2024, 4, 1));
        assert_eq!(tally_date("20241301"), None);
    }

    #[test]
    fn decodes_utf16_with_and_without_bom() {
        let le: Vec<u8> = "<A>₹</A>".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let be: Vec<u8> = "<A>₹</A>".encode_utf16().flat_map(u16::to_be_bytes).collect();
        assert_eq!(decode(&[[0xFF, 0xFE].as_slice(), &le].concat()), "<A>₹</A>");
        assert_eq!(decode(&le), "<A>₹</A>");
        assert_eq!(decode(&[[0xFE, 0xFF].as_slice(), &be].concat()), "<A>₹</A>");
        assert_eq!(decode("<A>₹</A>".as_bytes()), "<A>₹</A>");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'<', b'A', b'/', b'>']), "<A/>");
    }

    #[test]
    fn strips_control_characters() {
        assert_eq!(sanitize("<P>&#4; Primary</P>"), "<P> Primary</P>");
        assert_eq!(sanitize("<P>&#x4;\u{4}x&#13;&#10;&amp;&#8377;</P>"), "<P>x&#13;&#10;&amp;&#8377;</P>");
        assert_eq!(sanitize("\u{FEFF}<P>a & b; &#</P>"), "<P>a & b; &#</P>");
    }
}

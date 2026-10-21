//! UN/ECE Recommendation 20 unit codes (BT-130) from the free-text unit
//! symbols people type into their books ("Nos", "kg", "Std.").
//!
//! Every code here is in the code list of the CEN/TC 434 artefacts
//! (`EN16931-UBL-codes.sch`, rule BR-CL-23) [PRIMARY]. A symbol not listed
//! has no mapping; the caller then uses [`ONE`] ("C62") and says so.

/// "one": a unit count without a named unit (Rec 20 C62).
pub const ONE: &str = "C62";

/// (code, symbols that map to it, lower-cased, without dots or spaces).
const TABLE: &[(&str, &[&str])] = &[
    ("H87", &["nos", "no", "pcs", "pc", "piece", "pieces", "stk", "stück", "stueck", "st"]),
    (ONE, &["unit", "units", "u", "ea", "each", "c62"]),
    ("KGM", &["kg", "kgs", "kilogram", "kilograms", "kilogramm"]),
    ("GRM", &["g", "gm", "gms", "gram", "grams", "gramm"]),
    ("TNE", &["t", "tonne", "tonnes", "ton", "mt"]),
    ("MTR", &["m", "mtr", "mtrs", "metre", "meter", "metres", "meters"]),
    ("CMT", &["cm", "centimetre", "zentimeter"]),
    ("MMT", &["mm", "millimetre", "millimeter"]),
    ("KMT", &["km", "kilometre", "kilometer"]),
    ("MTK", &["m2", "m²", "sqm", "qm"]),
    ("MTQ", &["m3", "m³", "cbm", "cum"]),
    ("LTR", &["l", "ltr", "ltrs", "litre", "liter", "litres", "liters"]),
    ("MLT", &["ml", "millilitre", "milliliter"]),
    ("HUR", &["h", "hr", "hrs", "hour", "hours", "std", "stunde", "stunden"]),
    ("MIN", &["min", "mins", "minute", "minutes", "minuten"]),
    ("DAY", &["day", "days", "tag", "tage"]),
    ("WEE", &["week", "weeks", "woche", "wochen"]),
    ("MON", &["month", "months", "monat", "monate"]),
    ("ANN", &["year", "years", "yr", "jahr", "jahre"]),
    ("SET", &["set", "sets", "satz"]),
    ("PR", &["pair", "pairs", "paar"]),
    ("DZN", &["dozen", "doz", "dzn", "dutzend"]),
    ("KWH", &["kwh"]),
    ("XBX", &["box", "boxes", "karton"]),
    ("XPP", &["packet", "pkt", "pack", "paket"]),
];

/// The Rec 20/21 code for a unit symbol, if known. A symbol that already
/// is one of the codes above maps to itself.
pub fn rec20(symbol: &str) -> Option<&'static str> {
    let s: String = symbol.chars().filter(|c| !c.is_whitespace() && *c != '.').collect::<String>().to_lowercase();
    if s.is_empty() {
        return None;
    }
    TABLE
        .iter()
        .find(|(code, names)| code.eq_ignore_ascii_case(&s) || names.contains(&s.as_str()))
        .map(|(code, _)| *code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_symbols() {
        assert_eq!(rec20("Nos"), Some("H87"));
        assert_eq!(rec20(" Stk. "), Some("H87"));
        assert_eq!(rec20("kg"), Some("KGM"));
        assert_eq!(rec20("Std."), Some("HUR"));
        assert_eq!(rec20("m²"), Some("MTK"));
        assert_eq!(rec20("KGM"), Some("KGM"));
        assert_eq!(rec20("bag"), None);
        assert_eq!(rec20(""), None);
    }
}

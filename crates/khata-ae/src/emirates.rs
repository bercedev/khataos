//! The seven emirates.
//!
//! VAT 201 splits standard-rated supplies by emirate, in Boxes 1a to 1g, in
//! this order: Abu Dhabi, Dubai, Sharjah, Ajman, Umm Al Quwain, Ras Al
//! Khaimah, Fujairah (`docs/research/mena-compliance.md` §1.4 [SECONDARY]).
//!
//! The three-letter codes are KhataOS's own stable codes, stored in the
//! company profile; the FTA does not publish codes for the VAT 201 split.
//! Each emirate also carries its ISO 3166-2:AE subdivision code, for
//! mapping to other systems.

/// When this table was last checked.
pub const EMIRATES_VERSION: &str = "2026-10-20";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Emirate {
    /// KhataOS's code ("DXB").
    pub code: &'static str,
    pub name: &'static str,
    /// ISO 3166-2 subdivision code ("AE-DU").
    pub iso: &'static str,
    /// The VAT 201 box for its standard-rated supplies ("1b").
    pub vat201_box: &'static str,
}

const fn e(code: &'static str, name: &'static str, iso: &'static str, vat201_box: &'static str) -> Emirate {
    Emirate { code, name, iso, vat201_box }
}

/// In VAT 201 order (Box 1a to 1g).
pub const EMIRATES: &[Emirate] = &[
    e("AUH", "Abu Dhabi", "AE-AZ", "1a"),
    e("DXB", "Dubai", "AE-DU", "1b"),
    e("SHJ", "Sharjah", "AE-SH", "1c"),
    e("AJM", "Ajman", "AE-AJ", "1d"),
    e("UAQ", "Umm Al Quwain", "AE-UQ", "1e"),
    e("RAK", "Ras Al Khaimah", "AE-RK", "1f"),
    e("FUJ", "Fujairah", "AE-FU", "1g"),
];

/// Look up an emirate by its code, ignoring case and surrounding space.
pub fn emirate(code: &str) -> Option<&'static Emirate> {
    let code = code.trim();
    EMIRATES.iter().find(|e| e.code.eq_ignore_ascii_case(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_emirates_in_vat201_order() {
        assert_eq!(EMIRATES.len(), 7);
        let boxes: Vec<_> = EMIRATES.iter().map(|e| e.vat201_box).collect();
        assert_eq!(boxes, ["1a", "1b", "1c", "1d", "1e", "1f", "1g"]);
        assert_eq!(EMIRATES[0].name, "Abu Dhabi");
        assert_eq!(EMIRATES[6].name, "Fujairah");
    }

    #[test]
    fn codes_are_unique_three_letter_upper_case() {
        for (i, a) in EMIRATES.iter().enumerate() {
            assert!(a.code.len() == 3 && a.code.bytes().all(|b| b.is_ascii_uppercase()), "{}", a.code);
            assert!(a.iso.starts_with("AE-") && a.iso.len() == 5, "{}", a.iso);
            for b in &EMIRATES[i + 1..] {
                assert!(a.code != b.code && a.iso != b.iso && a.name != b.name);
            }
        }
    }

    #[test]
    fn lookup_ignores_case_and_space() {
        assert_eq!(emirate("DXB").map(|e| e.name), Some("Dubai"));
        assert_eq!(emirate(" rak ").map(|e| e.iso), Some("AE-RK"));
        assert_eq!(emirate("AE-DU"), None, "ISO codes are not the stored codes");
        assert_eq!(emirate(""), None);
        assert_eq!(emirate("27"), None);
    }
}

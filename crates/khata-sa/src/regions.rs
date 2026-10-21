//! The thirteen administrative regions of Saudi Arabia.
//!
//! `docs/research/mena-compliance.md` gives no region codes for KSA, and no
//! KSA tax rule depends on the region: VAT is 15% everywhere, the VAT return
//! has no regional split, and a VAT number encodes no region (research §5).
//! The region only fills the seller's and buyer's `CountrySubentity` on a
//! ZATCA e-invoice and the company's address.
//!
//! So the stored codes are the **ISO 3166-2:SA** subdivision codes, as
//! KhataOS knows them; they were not checked against the published standard
//! in this research [UNCERTAIN]. ISO numbers the regions 01 to 12 and 14:
//! there is no SA-13. The Arabic names are the usual official names, for
//! bilingual prints; have them checked by an Arabic reader [UNCERTAIN].

/// When this table was last checked.
pub const REGIONS_VERSION: &str = "2026-10-20";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaRegion {
    /// ISO 3166-2:SA code ("SA-01"), stored in the company profile.
    pub code: &'static str,
    pub name: &'static str,
    pub name_ar: &'static str,
}

const fn r(code: &'static str, name: &'static str, name_ar: &'static str) -> SaRegion {
    SaRegion { code, name, name_ar }
}

/// In ISO code order.
pub const REGIONS: &[SaRegion] = &[
    r("SA-01", "Riyadh", "الرياض"),
    r("SA-02", "Makkah", "مكة المكرمة"),
    r("SA-03", "Madinah", "المدينة المنورة"),
    r("SA-04", "Eastern Province", "المنطقة الشرقية"),
    r("SA-05", "Al-Qassim", "القصيم"),
    r("SA-06", "Ha'il", "حائل"),
    r("SA-07", "Tabuk", "تبوك"),
    r("SA-08", "Northern Borders", "الحدود الشمالية"),
    r("SA-09", "Jazan", "جازان"),
    r("SA-10", "Najran", "نجران"),
    r("SA-11", "Al-Bahah", "الباحة"),
    r("SA-12", "Al-Jawf", "الجوف"),
    r("SA-14", "Asir", "عسير"),
];

/// Look up a region by its code, ignoring case and surrounding space.
pub fn region(code: &str) -> Option<&'static SaRegion> {
    let code = code.trim();
    REGIONS.iter().find(|r| r.code.eq_ignore_ascii_case(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirteen_regions_with_iso_codes() {
        assert_eq!(REGIONS.len(), 13);
        assert_eq!((REGIONS[0].code, REGIONS[0].name), ("SA-01", "Riyadh"));
        assert_eq!(REGIONS[12].code, "SA-14", "ISO skips SA-13");
        assert!(region("SA-13").is_none());
    }

    #[test]
    fn codes_and_names_are_unique() {
        for (i, a) in REGIONS.iter().enumerate() {
            assert!(a.code.starts_with("SA-") && a.code.len() == 5, "{}", a.code);
            assert!(!a.name_ar.is_empty() && !a.name_ar.is_ascii());
            for b in &REGIONS[i + 1..] {
                assert!(a.code != b.code && a.name != b.name && a.name_ar != b.name_ar);
            }
        }
    }

    #[test]
    fn lookup_ignores_case_and_space() {
        assert_eq!(region(" sa-04 ").map(|r| r.name), Some("Eastern Province"));
        assert_eq!(region("01"), None, "the stored code includes the country prefix");
        assert_eq!(region(""), None);
    }
}

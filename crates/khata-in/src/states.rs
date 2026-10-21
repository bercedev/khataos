//! GST state and union-territory codes.
//!
//! The first two characters of a GSTIN, the place-of-supply code on an
//! invoice, and the state code in e-invoice and e-way bill JSON all use this
//! table. It is compiled from the GST portal state master and is versioned
//! so that a change shows up in review (research: verify against the
//! portal's master codes before GA).

/// When this table was last checked.
pub const STATES_VERSION: &str = "2026-10-19";

/// How intra-State tax is split in a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// States, and union territories with a legislature (Delhi, Puducherry,
    /// Jammu and Kashmir): CGST + SGST.
    State,
    /// Union territories without a legislature, and "Other Territory":
    /// CGST + UTGST (UTGST Act 2017, s.2).
    UnionTerritory,
    /// Not a place where supplies are consumed: 99 marks registrations under
    /// Centre jurisdiction (for example OIDAR suppliers).
    CentreJurisdiction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub code: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    /// Codes that GSTINs issued earlier can still carry, but that new
    /// supplies don't use (25 merged into 26 in 2020; 28 is pre-2014 Andhra
    /// Pradesh).
    pub legacy: bool,
}

const fn s(code: &'static str, name: &'static str) -> State {
    State { code, name, kind: Kind::State, legacy: false }
}

const fn ut(code: &'static str, name: &'static str) -> State {
    State { code, name, kind: Kind::UnionTerritory, legacy: false }
}

pub const STATES: &[State] = &[
    s("01", "Jammu and Kashmir"),
    s("02", "Himachal Pradesh"),
    s("03", "Punjab"),
    ut("04", "Chandigarh"),
    s("05", "Uttarakhand"),
    s("06", "Haryana"),
    s("07", "Delhi"),
    s("08", "Rajasthan"),
    s("09", "Uttar Pradesh"),
    s("10", "Bihar"),
    s("11", "Sikkim"),
    s("12", "Arunachal Pradesh"),
    s("13", "Nagaland"),
    s("14", "Manipur"),
    s("15", "Mizoram"),
    s("16", "Tripura"),
    s("17", "Meghalaya"),
    s("18", "Assam"),
    s("19", "West Bengal"),
    s("20", "Jharkhand"),
    s("21", "Odisha"),
    s("22", "Chhattisgarh"),
    s("23", "Madhya Pradesh"),
    s("24", "Gujarat"),
    State { legacy: true, ..ut("25", "Daman and Diu") },
    ut("26", "Dadra and Nagar Haveli and Daman and Diu"),
    s("27", "Maharashtra"),
    State { legacy: true, ..s("28", "Andhra Pradesh (before 2014)") },
    s("29", "Karnataka"),
    s("30", "Goa"),
    ut("31", "Lakshadweep"),
    s("32", "Kerala"),
    s("33", "Tamil Nadu"),
    s("34", "Puducherry"),
    ut("35", "Andaman and Nicobar Islands"),
    s("36", "Telangana"),
    s("37", "Andhra Pradesh"),
    ut("38", "Ladakh"),
    ut("97", "Other Territory"),
    State { code: "99", name: "Centre Jurisdiction", kind: Kind::CentreJurisdiction, legacy: false },
];

/// Look up a two-digit state code.
pub fn state(code: &str) -> Option<&'static State> {
    STATES.iter().find(|s| s.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_two_digit_and_sorted() {
        for pair in STATES.windows(2) {
            assert!(pair[0].code < pair[1].code, "{} then {}", pair[0].code, pair[1].code);
        }
        assert!(STATES.iter().all(|s| s.code.len() == 2 && s.code.bytes().all(|b| b.is_ascii_digit())));
        assert_eq!(STATES.len(), 40);
    }

    #[test]
    fn union_territories_split_utgst_only_without_a_legislature() {
        let kind = |c| state(c).unwrap().kind;
        for c in ["04", "25", "26", "31", "35", "38", "97"] {
            assert_eq!(kind(c), Kind::UnionTerritory, "{c}");
        }
        for c in ["01", "07", "34", "27"] {
            assert_eq!(kind(c), Kind::State, "{c} has a legislature (or is a State)");
        }
        assert_eq!(state("96"), None, "96 (other countries) is not a GSTIN state");
        assert_eq!(state("00"), None);
    }
}

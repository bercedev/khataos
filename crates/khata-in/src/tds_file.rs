//! The quarterly TDS / TCS statement as the official text file: the
//! `^`-delimited input that Protean's File Validation Utility (FVU) checks,
//! for the 2025-Act forms from tax year 2026-27:
//!
//! | Form | Ex | File type | What |
//! |---|---|---|---|
//! | 140 | 26Q | `NS1` | TDS on payments to residents other than salary |
//! | 143 | 27EQ | `TC1` | TCS |
//! | 144 | 27Q | `NS1` | TDS on payments to non-residents |
//!
//! Source: Protean (TIN) "File Format for Form Number 140 / 143 / 144, Q1
//! to Q4, Version 1.2" (`Form Number 140 - 26Q - Q1 to Q4_22072026.xlsx`
//! and siblings, downloaded 20 Oct 2026) and the sample files `140RQ1.txt`,
//! `143.txt`, `144RQ1.txt` from the same page [PRIMARY]. Record lengths
//! here match both the spec tables and the samples: file header (FH) 18
//! fields, batch header (BH) 72, challan (CD) 30, deductee / collectee (DD)
//! 45; lines end in CR LF; amounts have two decimals, rates four; dates are
//! `ddmmyyyy`. Codes (states, deductor types, collectee status, countries,
//! natures of remittance) are the spec's annexures.
//!
//! What this module does **not** do, so the FVU still has the last word:
//! - it does not import the challan status (`.csi`) file the RPU and FVU
//!   check challans against (the spec's note 12);
//! - it writes one batch, regular statements only (no corrections);
//! - deductee record numbers restart at 1 in each challan, as the older
//!   formats did; the v1.2 text for Form 144 says "in a batch"
//!   [UNCERTAIN: check with the FVU];
//! - it leaves optional fields empty (Form 121 numbers, the Form 145
//!   acknowledgement for non-residents, government-only fields).
//!
//! [`render`] checks what the spec marks mandatory and what this module can
//! check from the data; a file it returns is still to be validated with
//! the FVU before filing.

use chrono::{Datelike, NaiveDate};
use khata_core::{Amount, DeducteeKind};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::tds::Quarter;

/// The version of Protean's format this module writes.
pub const FORMAT_VERSION: &str = "1.2";

/// What goes in the file header's "Name of Return Preparation Utility".
pub const UTILITY_NAME: &str = "KhataOS";

/// Which statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Form {
    /// TDS, residents, non-salary (ex-26Q).
    F140,
    /// TCS (ex-27EQ).
    F143,
    /// TDS, non-residents (ex-27Q).
    F144,
}

impl Form {
    pub fn number(self) -> &'static str {
        match self {
            Self::F140 => "140",
            Self::F143 => "143",
            Self::F144 => "144",
        }
    }

    /// File header field 3.
    pub fn file_type(self) -> &'static str {
        match self {
            Self::F143 => "TC1",
            Self::F140 | Self::F144 => "NS1",
        }
    }

    pub fn label(self) -> String {
        let ex = match self {
            Self::F140 => "26Q",
            Self::F143 => "27EQ",
            Self::F144 => "27Q",
        };
        format!("Form {} (ex-{ex})", self.number())
    }
}

/// The deductor or collector (batch header fields 13 to 32).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Deductor {
    pub tan: String,
    /// PAN, or "PANNOTREQD" where the deductor need not have one.
    pub pan: String,
    pub name: String,
    /// Up to five address lines of at most 25 characters (line 1 required).
    pub address: Vec<String>,
    /// Statement state code (Annexure 1; not the GST code).
    pub state: String,
    pub pin: String,
    pub email: String,
    /// Country calling code ("91").
    pub phone_country: String,
    pub phone: String,
    /// Annexure 4 category: K company, F firm, Q individual / HUF…
    pub deductor_type: String,
    pub gstin: Option<String>,
}

/// The person responsible for deducting or collecting (batch header 33 to
/// 45 and 59).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Responsible {
    pub name: String,
    pub designation: String,
    pub pan: String,
    pub address: Vec<String>,
    pub state: String,
    pub pin: String,
    pub email: String,
    /// Country of the person ("INDIA").
    pub country: String,
    pub phone_country: String,
    pub phone: String,
}

/// One challan (CD record) and the rows paid through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challan {
    /// Tax, interest, fee and other parts of the deposit, whole rupees.
    pub tax: Amount,
    pub interest: Amount,
    pub fee: Amount,
    pub others: Amount,
    /// 7-digit BSR code of the bank branch.
    pub bsr: String,
    /// Challan serial number from the bank (up to 5 digits).
    pub serial: String,
    /// Date of deposit as the bank records it.
    pub date: NaiveDate,
    /// Minor head: 200 (payable by taxpayer) or 400 (regular assessment).
    pub minor_head: String,
    pub rows: Vec<Row>,
}

/// One deductee or collectee row (DD record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// A valid PAN, or `None` (written as PANNOTAVBL).
    pub pan: Option<String>,
    pub name: String,
    /// The 4-digit tax or collection code.
    pub code: String,
    /// Date paid or credited (TDS), received or debited (TCS).
    pub date: NaiveDate,
    pub amount: Amount,
    /// Tax deducted or collected (and deposited).
    pub tax: Amount,
    /// Date of deduction or collection; required when `tax` is not zero.
    pub deducted_on: Option<NaiveDate>,
    /// Percent.
    pub rate: Decimal,
    /// The reason flag (Annexure 6), e.g. A certificate, B declaration, C no PAN.
    pub remark: Option<char>,
    pub certificate: Option<String>,
    pub kind: Option<DeducteeKind>,
    /// Form 143 and 144: opting out of the s.202(1) regime (Y/N, mandatory).
    pub opting_out: bool,
    /// Form 143.
    pub non_resident: bool,
    pub permanent_establishment: bool,
    /// Form 144.
    pub nr: Option<NonResidentRow>,
}

/// Form 144 deductee details.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NonResidentRow {
    /// Annexure 9 code (e.g. "21" fees for technical services).
    pub remittance: Option<String>,
    /// Annexure 10 code.
    pub country: String,
    /// The rate is the treaty's ("B") rather than the Act's ("A").
    pub treaty: bool,
    pub grossed_up: bool,
    pub address: String,
    pub email: String,
    pub phone: String,
    pub tin: String,
}

/// A whole regular statement for one quarter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub form: Form,
    pub quarter: Quarter,
    /// File creation date (not in the future).
    pub created: NaiveDate,
    /// File sequence number (unique across files).
    pub sequence: u32,
    pub deductor: Deductor,
    pub responsible: Responsible,
    /// Token number of the previous regular statement of this form, when
    /// one was filed (batch header 9 and 52).
    pub previous_token: Option<String>,
    pub challans: Vec<Challan>,
}

// ------------------------------------------------------------ annexures

/// Annexure 1: statement state codes, with the GST state code of the same
/// state or territory.
pub const STATES: &[(&str, &str, &str)] = &[
    ("01", "35", "ANDAMAN AND NICOBAR ISLANDS"),
    ("02", "37", "ANDHRA PRADESH"),
    ("03", "12", "ARUNACHAL PRADESH"),
    ("04", "18", "ASSAM"),
    ("05", "10", "BIHAR"),
    ("06", "04", "CHANDIGARH"),
    ("07", "26", "DADRA & NAGAR HAVELI AND DAMAN & DIU"),
    ("09", "07", "DELHI"),
    ("10", "30", "GOA"),
    ("11", "24", "GUJARAT"),
    ("12", "06", "HARYANA"),
    ("13", "02", "HIMACHAL PRADESH"),
    ("14", "01", "JAMMU & KASHMIR"),
    ("15", "29", "KARNATAKA"),
    ("16", "32", "KERALA"),
    ("17", "31", "LAKSHWADEEP"),
    ("18", "23", "MADHYA PRADESH"),
    ("19", "27", "MAHARASHTRA"),
    ("20", "14", "MANIPUR"),
    ("21", "17", "MEGHALAYA"),
    ("22", "15", "MIZORAM"),
    ("23", "13", "NAGALAND"),
    ("24", "21", "ODISHA"),
    ("25", "34", "PONDICHERRY"),
    ("26", "03", "PUNJAB"),
    ("27", "08", "RAJASTHAN"),
    ("28", "11", "SIKKIM"),
    ("29", "33", "TAMIL NADU"),
    ("30", "16", "TRIPURA"),
    ("31", "09", "UTTAR PRADESH"),
    ("32", "19", "WEST BENGAL"),
    ("33", "22", "CHHATTISGARH"),
    ("34", "05", "UTTARAKHAND"),
    ("35", "20", "JHARKHAND"),
    ("36", "36", "TELANGANA"),
    ("37", "38", "LADAKH"),
];

/// The statement state code for a GST state code ("27" → "19").
pub fn state_from_gst(gst: &str) -> Option<&'static str> {
    STATES.iter().find(|s| s.1 == gst.trim()).map(|s| s.0)
}

/// Annexure 4: deductor / collector categories for non-government bodies
/// (government categories need ministry and AIN fields this module does
/// not write).
pub const DEDUCTOR_TYPES: &[(&str, &str)] = &[
    ("K", "Company"),
    ("M", "Branch / Division of Company"),
    ("P", "Association of Person (AOP)"),
    ("T", "Association of Person (Trust)"),
    ("J", "Artificial Juridical Person"),
    ("B", "Body of Individuals"),
    ("Q", "Individual/HUF"),
    ("F", "Firm"),
];

/// Annexure 8 of Forms 143 and 144: collectee / deductee status code.
/// A trust, local authority or government is "Others" (10), and a
/// co-operative society (6) has no deductee type in the books yet
/// [UNCERTAIN: check the trust mapping in the RPU].
pub fn status_code(kind: Option<DeducteeKind>) -> &'static str {
    match kind {
        Some(DeducteeKind::Company) => "1",
        Some(DeducteeKind::Individual) => "2",
        Some(DeducteeKind::Huf) => "3",
        Some(DeducteeKind::Aop) => "4",
        Some(DeducteeKind::Firm) => "7",
        Some(DeducteeKind::Boi) => "8",
        Some(DeducteeKind::Juridical) => "9",
        Some(DeducteeKind::Trust | DeducteeKind::LocalAuthority | DeducteeKind::Government) | None => "10",
    }
}

/// Annexure 9 natures of remittance for tax code 1057 (s.393(2) Sl. No. 17,
/// any other sum to a non-resident).
pub const REMITTANCES: &[(&str, &str)] = &[
    ("27", "Interest income"),
    ("16", "Dividend income referred to in section 207(1) [Table: Sl. No. 2]"),
    ("73", "Dividend income referred to in section 207(1) [Table: Sl. No. 1]"),
    ("28", "Investment income"),
    ("66", "Long term capital gains referred to in section 214 [Table: Sl. No. 2]"),
    ("67", "Long term capital gains referred to in section 197(1)"),
    ("68", "LTCG other than those in Schedule II, Sl. Nos. 14 and 17"),
    ("69", "Long term capital gains referred to in section 198 above ₹1,25,000"),
    ("70", "Short term capital gains referred to in section 196"),
    ("21", "Fees for technical services / included services (agreement after 31 Mar 1976)"),
    ("74", "Fees for technical services / included services (agreement 1 Mar 1964 to 31 Mar 1976)"),
    ("49", "Royalty (agreement after 31 Mar 1976)"),
    ("75", "Royalty (agreement 1 Apr 1961 to 31 Mar 1976)"),
    ("99", "Other income"),
];

/// Remittance codes for which Form 144 needs the payee's foreign address,
/// email, phone and tax number (DD fields 37 to 41, condition d).
const REMITTANCE_NEEDS_CONTACT: &[&str] = &["16", "27", "49", "21", "52", "31"];

/// Annexure 10: country codes for Form 144 (as published, spelling kept).
pub const COUNTRIES: &[(&str, &str)] = &[
    ("01", "AFGHANISTAN"), ("02", "AKROTIRI"), ("03", "ALBANIA"), ("04", "ALGERIA"), ("05", "AMERICAN SAMOA"),
    ("06", "ANDORRA"), ("07", "ANGOLA"), ("08", "ANGUILLA"), ("09", "ANTARCTICA"), ("10", "ANTIGUA AND BARBUDA"),
    ("11", "ARGENTINA"), ("12", "ARMENIA"), ("13", "ARUBA"), ("14", "ASHMORE AND CARTIER ISLANDS"),
    ("15", "AUSTRALIA"), ("16", "AUSTRIA"), ("17", "AZERBAIJAN"), ("18", "BAHRAIN"), ("19", "BAILIWICK OF GUERNSEY"),
    ("20", "BAILIWICK OF JERSEY"), ("21", "BAKER ISLAND"), ("22", "BANGLADESH"), ("23", "BARBADOS"),
    ("24", "BELARUS"), ("25", "BELGIUM"), ("26", "BELIZE"), ("27", "BENIN PORTO"), ("28", "BERMUDA"),
    ("29", "BHUTAN"), ("30", "BOLIVIA"), ("31", "BOSNIAAND HERZEGOVINA"), ("32", "BOTSWANA"),
    ("33", "BOUVET ISLAND"), ("34", "BRAZIL"), ("35", "BRITISH INDIAN OCEAN TERRITORY"), ("36", "BRUNEI"),
    ("37", "BULGARIA"), ("38", "BURKINA FASO"), ("39", "BURUNDI"), ("40", "CAMBODIA"), ("41", "CAMEROON"),
    ("42", "CANADA"), ("43", "CAPE VERDE"), ("44", "CAYMAN ISLANDS"), ("45", "CENTRAL AFRICAN REPUBLIC"),
    ("46", "CHAD"), ("47", "CHILE"), ("48", "CHINA"), ("49", "CHRISTMAS ISLAND"), ("50", "CLIPPERTON ISLAND"),
    ("51", "COCOS (KEELING) ISLANDS"), ("52", "COLOMBIA"), ("53", "COMMONWEALTH OF PUERTO RICO"),
    ("54", "COMMONWEALTH OF THE NORTHERN MARIANA ISLANDS"), ("55", "COMOROS"),
    ("56", "CONGO,DEMOCRATIC REPUBLIC OF THE"), ("57", "CONGO,REPUBLIC OF THE"), ("58", "COOK ISLANDS"),
    ("59", "CORAL SEA ISLANDS"), ("60", "CORAL SEA ISLANDS TERRITORY"), ("61", "COSTA RICA"),
    ("62", "COTE D'IVOIRE"), ("63", "CROATIA"), ("64", "CUBA"), ("65", "CYPRUS"), ("66", "CZECH REPUBLIC"),
    ("67", "DENMARK"), ("68", "DEPARTMENTAL COLLECTIVITY OF MAYOTTE"), ("69", "DHEKELIA"), ("70", "DJIBOUTI"),
    ("71", "DOMINICA"), ("72", "DOMINICAN REPUBLIC"), ("73", "EAST TIMOR (TIMORLASTE)"), ("74", "ECUADOR"),
    ("75", "EGYPT"), ("76", "ELSALVADOR"), ("77", "EQUATORIAL GUINEA"), ("78", "ERITREA"), ("79", "ESTONIA"),
    ("80", "ETHIOPIA"), ("81", "FALKLAND ISLANDS (ISLAS MALVINAS)"), ("82", "FAROE ISLANDS"), ("83", "FIJI"),
    ("84", "FINLAND"), ("85", "FRANCE"), ("86", "FRENCH GUIANA"), ("87", "FRENCH POLYNESIA"),
    ("88", "FRENCH SOUTHERN ISLANDS"), ("89", "GABON"), ("90", "GEORGIA"), ("91", "GERMANY"), ("92", "GEURNSEY"),
    ("93", "GHANA"), ("94", "GIBRALTAR"), ("95", "GREECE"), ("96", "GREENLAND"), ("97", "GRENADA"),
    ("98", "GUADELOUPE"), ("100", "GUAM"), ("101", "GUATEMALA"), ("102", "GUERNSEY"), ("103", "GUINEA"),
    ("104", "GUINEABISSAU"), ("105", "GUYANA"), ("106", "HAITI"), ("107", "HEARD ISLAND AND MCDONALD ISLANDS"),
    ("108", "HONDURAS"), ("109", "HONG KONG"), ("110", "HOWLAND ISLAND"), ("111", "HUNGARY"), ("112", "ICELAND"),
    ("113", "INDIA"), ("114", "INDONESIA"), ("115", "IRAN"), ("116", "IRAQ"), ("117", "IRELAND"),
    ("118", "ISLE OF MAN"), ("119", "ISRAEL"), ("120", "ITALY"), ("121", "JAMAICA"), ("122", "JAN MAYEN"),
    ("123", "JAPAN"), ("124", "JARVIS ISLAND"), ("125", "JERSEY"), ("126", "JOHNSTON ATOLL"), ("127", "JORDAN"),
    ("128", "KAZAKHSTAN"), ("129", "KENYA"), ("130", "KINGMAN REEF"), ("131", "KIRIBATI"), ("132", "KOREA, NORTH"),
    ("133", "KOREA, SOUTH"), ("134", "KOSOVO"), ("135", "KUWAIT"), ("136", "KYRGYZSTAN"), ("137", "LAOS"),
    ("138", "LATVIA"), ("139", "LEBANON"), ("140", "LESOTHO"), ("141", "LIBERIA"), ("142", "LIBYA"),
    ("143", "LIECHTENSTEIN"), ("144", "LITHUANIA"), ("145", "LUXEMBOURG"), ("146", "MACAU"), ("147", "MACEDONIA"),
    ("148", "MADAGASCAR"), ("149", "MALAWI"), ("150", "MALAYSIA"), ("151", "MALAYSIA (LABUAN)"), ("152", "MALDIVES"),
    ("153", "MALI"), ("154", "MALTA"), ("155", "MARSHALLISLANDS"), ("156", "MARTINIQUE"), ("157", "MAURITANIA"),
    ("158", "MAURITIUS"), ("159", "MAYOTTE"), ("160", "UNITED MEXICAN STATES"),
    ("161", "MICRONESIA, FEDERATED STATES OF"), ("162", "MIDWAY ISLANDS"), ("163", "MOLDOVA"), ("164", "MONACO"),
    ("165", "MONGOLIA"), ("166", "MONTENEGRO"), ("167", "MONTSERRAT"), ("168", "MOROCCO"), ("169", "MOZAMBIQUE"),
    ("170", "MYANMAR (BURMA)"), ("171", "NAMIBIA"), ("172", "NAURU"), ("173", "NAVASSA ISLAND"), ("174", "NEPAL"),
    ("175", "NETHERLANDS"), ("176", "NETHERLANDS ANTILLES"), ("177", "NEW CALEDONIA"), ("178", "NEWZEALAND"),
    ("179", "NICARAGUA"), ("180", "NIGER"), ("181", "NIGERIA"), ("182", "NIUE"), ("183", "NORFOLK ISLAND"),
    ("184", "NORTHERN MARIANA ISLANDS"), ("185", "NORWAY"), ("186", "OMAN"), ("187", "PAKISTAN"), ("188", "PALAU"),
    ("189", "PALMYRA ATOLL"), ("190", "PANAMA"), ("191", "PAPUA NEW GUINEA"), ("192", "PARACEL ISLANDS"),
    ("193", "PARAGUAY"), ("194", "PERU"), ("195", "PHILIPPINES"), ("196", "PITCAIRN ISLANDS"),
    ("197", "PITCAIRN, HENDERSON, DUCIE, AND OENO ISLANDS"), ("198", "POLAND"), ("199", "PORTUGAL"),
    ("200", "PUERTO RICO"), ("201", "QATAR"), ("202", "REUNION"), ("203", "ROMANIA"), ("204", "RUSSIA"),
    ("205", "RWANDA"), ("206", "SAINT BARTHELEMY"), ("207", "SAINT HELENA"), ("208", "SAINT KITTS AND NEVIS"),
    ("209", "SAINT LUCIA"), ("210", "SAINT MARTIN"), ("211", "SAINT PIERRE AND MIQUELON"),
    ("212", "SAINT VINCENT AND THE GRENADINES"), ("213", "SAMOA"), ("214", "SANMARINO"),
    ("215", "SAO TOME AND PRINCIPE"), ("216", "SAUDI ARABIA"), ("217", "SENEGAL"), ("218", "SERBIA"),
    ("219", "SEYCHELLES"), ("220", "SIERRA LEONE"), ("221", "SINGAPORE"), ("222", "SLOVAKIA"), ("223", "SLOVENIA"),
    ("224", "SOLOMON ISLANDS"), ("225", "SOMALIA"), ("226", "SOUTH AFRICA"),
    ("227", "SOUTH GEORGIA AND SOUTH SANDWICH ISLANDS"), ("228", "SPRATLY ISLANDS"), ("229", "SPAIN"),
    ("230", "SRI LANKA"), ("231", "ST. VINCENT & GRENADINES"), ("232", "SUDAN"), ("233", "SURINAME"),
    ("234", "SVALBARD"), ("235", "SWAZILAND"), ("236", "SWEDEN"), ("237", "SWITZERLAND"), ("238", "SYRIA"),
    ("239", "TAIWAN"), ("240", "TAJIKISTAN"), ("241", "TANZANIA"),
    ("242", "TERRITORIAL COLLECTIVITY OF ST. PIERRE & MIQUELON"), ("243", "TERRITORY OF AMERICAN SAMOA"),
    ("244", "TERRITORY OF ASHMORE AND CARTIER ISLANDS"), ("245", "TERRITORY OF CHRISTMAS ISLAND"),
    ("246", "TERRITORY OF COCOS (KEELING) ISLANDS"), ("247", "TERRITORY OF GUAM"),
    ("248", "TERRITORY OF HEARD ISLAND & MCDONALD ISLANDS"), ("249", "TERRITORY OF NORFOLK ISLAND"),
    ("250", "THAILAND"), ("251", "THE BAHAMAS"), ("252", "THE GAMBIA"), ("253", "TOGO"), ("254", "TOKELAU"),
    ("255", "TONGA"), ("256", "TRINIDAD AND TOBAGO"), ("257", "TUNISIA"), ("258", "TURKEY"), ("259", "TURKMENISTAN"),
    ("260", "TURKS AND CAICOS ISLANDS"), ("261", "TUVALU"), ("262", "UGANDA"), ("263", "UKRAINE"),
    ("264", "UNITED ARAB EMIRATES"), ("265", "UNITED KINGDOM"), ("266", "UNITED STATES VIRGIN ISLANDS"),
    ("267", "UNITED STATES OF AMERICA"), ("268", "URUGUAY"), ("269", "UZBEKISTAN"), ("270", "VANUATU"),
    ("271", "VATICAN CITY (HOLYSEE)"), ("272", "VENEZUELA"), ("273", "VIETNAM"), ("274", "VIRGIN ISLANDS, BRITISH"),
    ("275", "VIRGIN ISLANDS, U.S."), ("276", "WAKE ISLAND"), ("277", "WALLIS AND FUTUNA"), ("278", "WESTERN SAHARA"),
    ("279", "YEMEN"), ("280", "ZAMBIA"), ("281", "ZIMBABWE"), ("282", "COMBODIA"), ("283", "CONGO"),
    ("284", "IVORY COAST"), ("285", "WEST INDIES"), ("286", "BRITISH VIRGIN ISLANDS"),
];

pub fn country(code: &str) -> Option<&'static str> {
    COUNTRIES.iter().find(|c| c.0 == code.trim()).map(|c| c.1)
}

// ------------------------------------------------------------ rendering

fn date(d: NaiveDate) -> String {
    d.format("%d%m%Y").to_string()
}

fn money(a: Amount) -> String {
    format!("{:.2}", a.round_dp(2))
}

fn rate(r: Decimal) -> String {
    format!("{:.4}", r.round_dp(4))
}

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

/// A record of `n` empty fields; `set` takes 1-based positions as the spec
/// numbers them.
struct Rec(Vec<String>);

impl Rec {
    fn new(n: usize) -> Self {
        Self(vec![String::new(); n])
    }

    fn set(&mut self, field: usize, v: impl Into<String>) -> &mut Self {
        self.0[field - 1] = v.into();
        self
    }

    fn line(&self) -> String {
        let mut s = self.0.join("^");
        s.push_str("\r\n");
        s
    }
}

/// Split free-form address text into lines of at most 25 characters,
/// breaking at commas, newlines and spaces; `None` if it needs more than
/// five lines or a word longer than 25.
pub fn address_lines(text: &str) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    for part in text.split([',', '\n']).map(str::trim).filter(|p| !p.is_empty()) {
        let mut cur = String::new();
        for word in part.split_whitespace() {
            if word.chars().count() > 25 {
                return None;
            }
            let len = cur.chars().count();
            if len > 0 && len + 1 + word.chars().count() > 25 {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
        if !cur.is_empty() {
            // Join short parts onto the previous line when they fit.
            match lines.last_mut() {
                Some(last) if last.chars().count() + 2 + cur.chars().count() <= 25 && !last.is_empty() => {
                    last.push_str(", ");
                    last.push_str(&cur);
                }
                _ => lines.push(cur),
            }
        }
    }
    (lines.len() <= 5).then_some(lines)
}

/// Checks; each failure is one message naming the field.
struct Check(Vec<String>);

impl Check {
    fn need(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.0.push(msg());
        }
    }

    fn text(&mut self, what: &str, v: &str, max: usize, required: bool) {
        let v = v.trim();
        if v.is_empty() {
            self.need(!required, || format!("{what} is required"));
            return;
        }
        self.need(v.chars().count() <= max, || format!("{what} is longer than {max} characters"));
        self.need(!v.contains('^'), || format!("{what} must not contain ^"));
        self.need(v.chars().any(char::is_alphanumeric), || format!("{what} cannot be only special characters"));
        self.need(v.is_ascii(), || format!("{what} must be plain ASCII (the file is an ASCII file)"));
    }

    fn digits(&mut self, what: &str, v: &str, min: usize, max: usize) {
        let ok = (min..=max).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit());
        self.need(ok, || if min == max { format!("{what} must be {min} digits") } else { format!("{what} must be {min} to {max} digits") });
    }

    fn email(&mut self, what: &str, v: &str) {
        let ok = match v.split_once('@') {
            Some((a, b)) => !a.is_empty() && b.contains('.') && !b.starts_with('.') && !b.ends_with('.'),
            None => false,
        } && !v.contains(['^', ' ']);
        self.need(ok, || format!("{what} is not a valid email"));
    }

    fn address(&mut self, what: &str, lines: &[String]) {
        self.need(!lines.is_empty() && !lines[0].trim().is_empty(), || format!("{what}: address line 1 is required"));
        self.need(lines.len() <= 5, || format!("{what}: at most 5 address lines"));
        for (i, l) in lines.iter().enumerate().take(5) {
            self.text(&format!("{what} address line {}", i + 1), l, 25, i == 0);
        }
    }
}

fn is_tan(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 10 && b[..4].iter().all(u8::is_ascii_uppercase) && b[4..9].iter().all(u8::is_ascii_digit) && b[9].is_ascii_uppercase()
}

fn is_pan(p: &str) -> bool {
    crate::pan::validate(p).is_ok_and(|v| v.value == p)
}

/// Whole rupees: the challan fields allow no paise.
fn whole(a: Amount) -> bool {
    a.fract().is_zero() && !a.is_sign_negative()
}

fn validate(s: &Statement) -> Vec<String> {
    let mut c = Check(Vec::new());
    let d = &s.deductor;
    let r = &s.responsible;
    let range = s.quarter.range();
    c.need(range.is_some(), || "the quarter must be 1 to 4".into());
    c.need(s.quarter.year >= crate::tds::NEW_ACT_FROM.year(), || {
        "this file format is for tax year 2026-27 onwards; earlier quarters use the old forms and FVU".into()
    });
    c.need(is_tan(&d.tan), || format!("the TAN {:?} is not 4 letters, 5 digits and a letter", d.tan));
    c.need(is_pan(&d.pan) || d.pan == "PANNOTREQD", || format!("the deductor PAN {:?} is not valid", d.pan));
    c.text("Deductor name", &d.name, 75, true);
    c.address("Deductor", &d.address);
    c.need(STATES.iter().any(|x| x.0 == d.state), || "the deductor's state code is not in the statement's list".into());
    c.digits("Deductor PIN", &d.pin, 6, 6);
    c.email("Deductor email", &d.email);
    c.digits("Deductor phone country code", &d.phone_country, 1, 5);
    c.digits("Deductor phone", &d.phone, 6, 10);
    c.need(DEDUCTOR_TYPES.iter().any(|x| x.0 == d.deductor_type), || {
        format!("deductor type {:?} is not one KhataOS writes ({})", d.deductor_type, DEDUCTOR_TYPES.iter().map(|x| x.0).collect::<Vec<_>>().join(", "))
    });
    if let Some(g) = &d.gstin {
        c.need(crate::gstin::validate(g).is_ok(), || format!("the GSTIN {g} is not valid"));
    }
    c.text("Responsible person's name", &r.name, 75, true);
    c.text("Responsible person's designation", &r.designation, 20, true);
    c.need(is_pan(&r.pan), || format!("the responsible person's PAN {:?} is not valid", r.pan));
    c.address("Responsible person", &r.address);
    c.need(STATES.iter().any(|x| x.0 == r.state), || "the responsible person's state code is not in the statement's list".into());
    c.digits("Responsible person's PIN", &r.pin, 6, 6);
    c.email("Responsible person's email", &r.email);
    c.text("Responsible person's country", &r.country, 25, true);
    c.digits("Responsible person's phone country code", &r.phone_country, 1, 5);
    c.digits("Responsible person's phone", &r.phone, 6, 10);
    if let Some(t) = &s.previous_token {
        c.digits("Previous statement's token number", t, 15, 15);
    }
    c.need(!s.challans.is_empty(), || "there is no challan".into());
    c.need(s.challans.iter().any(|ch| !ch.rows.is_empty()), || {
        "a regular statement needs at least one deductee or collectee row".into()
    });
    for (i, ch) in s.challans.iter().enumerate() {
        let what = format!("Challan {} ({})", i + 1, ch.date.format("%d-%m-%Y"));
        for (name, a) in [("tax", ch.tax), ("interest", ch.interest), ("fee", ch.fee), ("others", ch.others)] {
            c.need(whole(a), || format!("{what}: {name} must be whole rupees"));
        }
        c.digits(&format!("{what}: BSR code"), &ch.bsr, 7, 7);
        c.digits(&format!("{what}: challan serial number"), &ch.serial, 1, 5);
        c.need(["200", "400"].contains(&ch.minor_head.as_str()), || format!("{what}: minor head must be 200 or 400"));
        if let Some((from, to)) = range {
            c.need(ch.date <= to.max(s.created), || format!("{what}: deposited after the statement date"));
            let earliest = NaiveDate::from_ymd_opt(s.quarter.year - 1, 4, 1);
            c.need(earliest.is_none_or(|f| ch.date >= f) && from.year() > 0, || format!("{what}: deposited before the previous year"));
        }
        let deposited: Amount = ch.rows.iter().map(|r| r.tax).sum();
        let total = ch.tax + ch.interest + ch.fee + ch.others;
        c.need(total >= deposited + ch.interest + ch.others + ch.fee, || {
            format!("{what}: the rows' tax {} is more than the challan's tax {}", money(deposited), money(ch.tax))
        });
        for (j, row) in ch.rows.iter().enumerate() {
            let rw = format!("{what}, row {} ({})", j + 1, row.name);
            c.text(&format!("{rw}: name"), &row.name, 75, true);
            if let Some(p) = &row.pan {
                c.need(is_pan(p), || format!("{rw}: the PAN {p:?} is not valid"));
            }
            c.need(row.code.len() == 4 && row.code.bytes().all(|b| b.is_ascii_digit()), || format!("{rw}: the tax code must be 4 digits"));
            c.need(row.amount > Amount::ZERO, || format!("{rw}: the amount must be more than zero"));
            c.need(!row.tax.is_sign_negative(), || format!("{rw}: the tax cannot be negative"));
            if let Some((from, to)) = range {
                c.need(from <= row.date && row.date <= to, || format!("{rw}: the date {} is outside the quarter", row.date));
                if let Some(dd) = row.deducted_on {
                    c.need(dd >= from, || format!("{rw}: deducted before the quarter"));
                }
            }
            c.need(row.tax.is_zero() || row.deducted_on.is_some(), || format!("{rw}: the date of deduction is required"));
            c.need(row.remark != Some('A') || row.certificate.as_deref().is_some_and(|x| [10, 15].contains(&x.len())), || {
                format!("{rw}: flag A needs the 10 or 15 character certificate number")
            });
            if s.form == Form::F144 {
                match &row.nr {
                    None => c.0.push(format!("{rw}: non-resident details (country, rate basis) are missing")),
                    Some(n) => {
                        c.need(country(&n.country).is_some(), || format!("{rw}: the country code {:?} is not in the statement's list", n.country));
                        if let Some(code) = &n.remittance {
                            c.need(REMITTANCES.iter().any(|x| x.0 == code), || format!("{rw}: nature of remittance {code:?} is not in the list"));
                        }
                        c.need(row.code != "1057" || n.remittance.is_some(), || format!("{rw}: the nature of remittance is required for code 1057"));
                        let needs_contact = row.rate < Decimal::from(20)
                            || row.remark == Some('C')
                            || row.pan.is_none()
                            || n.remittance.as_deref().is_some_and(|x| REMITTANCE_NEEDS_CONTACT.contains(&x));
                        if needs_contact {
                            c.text(&format!("{rw}: address in the country of residence"), &n.address, 150, true);
                            c.email(&format!("{rw}: email"), &n.email);
                            c.digits(&format!("{rw}: phone"), &n.phone, 6, 15);
                            c.text(&format!("{rw}: tax number in the country of residence"), &n.tin, 25, true);
                        }
                    }
                }
            }
        }
    }
    c.0
}

/// The statement file, or every problem found. A returned file is ready
/// for the FVU, which remains the authority.
pub fn render(s: &Statement) -> Result<String, Vec<String>> {
    let problems = validate(s);
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut out = String::new();
    let mut line = 0usize;
    let mut next = || {
        line += 1;
        line.to_string()
    };
    let d = &s.deductor;
    let r = &s.responsible;
    let mut fh = Rec::new(18);
    fh.set(1, next()).set(2, "FH").set(3, s.form.file_type()).set(4, "R").set(5, date(s.created));
    fh.set(6, s.sequence.to_string()).set(7, "D").set(8, &d.tan).set(9, "1").set(10, UTILITY_NAME);
    out += &fh.line();

    let y = s.quarter.year;
    let total_deposit: Amount = s.challans.iter().map(|c| c.tax + c.interest + c.fee + c.others).sum();
    let mut bh = Rec::new(72);
    bh.set(1, next()).set(2, "BH").set(3, "1").set(4, s.challans.len().to_string()).set(5, s.form.number());
    if let Some(t) = &s.previous_token {
        bh.set(9, t);
    }
    bh.set(13, &d.tan).set(15, &d.pan).set(16, format!("{}{:02}", y + 1, (y + 2) % 100)).set(17, format!("{y}{:02}", (y + 1) % 100));
    bh.set(18, format!("Q{}", s.quarter.q)).set(19, d.name.trim()).set(20, "INDIA");
    for (i, l) in d.address.iter().take(5).enumerate() {
        bh.set(21 + i, l.trim());
    }
    bh.set(26, &d.state).set(27, &d.pin).set(28, &d.email).set(29, &d.phone_country).set(30, &d.phone).set(32, &d.deductor_type);
    bh.set(33, r.name.trim()).set(34, r.designation.trim());
    for (i, l) in r.address.iter().take(5).enumerate() {
        bh.set(35 + i, l.trim());
    }
    bh.set(40, &r.state).set(41, &r.pin).set(42, &r.email).set(43, r.country.trim()).set(44, &r.phone_country).set(45, &r.phone);
    bh.set(47, money(total_deposit)).set(52, yn(s.previous_token.is_some())).set(59, &r.pan);
    if let Some(g) = &d.gstin {
        bh.set(69, g);
    }
    out += &bh.line();

    for (i, ch) in s.challans.iter().enumerate() {
        let serial = (i + 1).to_string();
        let deposited: Amount = ch.rows.iter().map(|r| r.tax).sum();
        let mut cd = Rec::new(30);
        cd.set(1, next()).set(2, "CD").set(3, "1").set(4, &serial).set(5, ch.rows.len().to_string()).set(6, "N");
        cd.set(8, money(ch.tax)).set(9, money(ch.interest)).set(10, money(ch.fee)).set(11, money(ch.others));
        cd.set(12, money(ch.tax + ch.interest + ch.fee + ch.others)).set(13, "C").set(15, &ch.bsr).set(17, &ch.serial);
        cd.set(19, date(ch.date)).set(21, money(deposited)).set(22, money(deposited)).set(23, &ch.minor_head);
        cd.set(24, money(ch.interest)).set(25, money(ch.others));
        out += &cd.line();
        for (j, row) in ch.rows.iter().enumerate() {
            let mut dd = Rec::new(45);
            dd.set(1, next()).set(2, "DD").set(3, "1").set(4, (j + 1).to_string()).set(5, &serial).set(6, "O");
            dd.set(8, row.pan.clone().unwrap_or_else(|| "PANNOTAVBL".into())).set(9, row.name.trim()).set(15, &row.code);
            dd.set(18, "Y").set(19, date(row.date)).set(20, money(row.amount)).set(24, money(row.tax)).set(25, money(row.tax));
            if !row.tax.is_zero() {
                dd.set(27, row.deducted_on.map(date).unwrap_or_default());
            }
            dd.set(28, rate(if row.tax.is_zero() { Decimal::ZERO } else { row.rate }));
            if let Some(m) = row.remark {
                dd.set(32, m.to_string());
            }
            if let Some(cert) = &row.certificate {
                dd.set(33, cert);
            }
            match s.form {
                Form::F140 => {}
                Form::F143 => {
                    dd.set(11, yn(row.non_resident)).set(12, status_code(row.kind)).set(14, yn(row.opting_out));
                    if row.non_resident {
                        dd.set(13, yn(row.permanent_establishment));
                    }
                }
                Form::F144 => {
                    let n = row.nr.clone().unwrap_or_default();
                    dd.set(12, status_code(row.kind)).set(14, yn(row.opting_out)).set(16, n.remittance.clone().unwrap_or_default());
                    dd.set(17, &n.country).set(21, yn(n.grossed_up)).set(30, if n.treaty { "B" } else { "A" });
                    dd.set(37, n.address.trim()).set(38, n.email.trim()).set(39, &n.phone).set(41, n.tin.trim());
                }
            }
            out += &dd.line();
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn day(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    fn deductor() -> Deductor {
        Deductor {
            tan: "MUMA12345B".into(),
            pan: "AAPFU0939F".into(),
            name: "Mehta Traders".into(),
            address: vec!["12 Market Road".into(), "Andheri East".into()],
            state: "19".into(),
            pin: "400069".into(),
            email: "accounts@mehta.example".into(),
            phone_country: "91".into(),
            phone: "9820012345".into(),
            deductor_type: "F".into(),
            gstin: None,
        }
    }

    fn responsible() -> Responsible {
        Responsible {
            name: "Ravi Mehta".into(),
            designation: "Partner".into(),
            pan: "ABCPM1234K".into(),
            address: vec!["12 Market Road".into()],
            state: "19".into(),
            pin: "400069".into(),
            email: "ravi@mehta.example".into(),
            country: "INDIA".into(),
            phone_country: "91".into(),
            phone: "9820012346".into(),
        }
    }

    fn row(name: &str, pan: Option<&str>, code: &str, amount: Amount, tax: Amount, rate: Decimal) -> Row {
        Row {
            pan: pan.map(str::to_owned),
            name: name.into(),
            code: code.into(),
            date: day(2026, 5, 10),
            amount,
            tax,
            deducted_on: Some(day(2026, 5, 10)),
            rate,
            remark: None,
            certificate: None,
            kind: Some(DeducteeKind::Company),
            opting_out: false,
            non_resident: false,
            permanent_establishment: false,
            nr: None,
        }
    }

    fn statement(form: Form, rows: Vec<Row>) -> Statement {
        let tax: Amount = rows.iter().map(|r| r.tax).sum();
        Statement {
            form,
            quarter: Quarter { year: 2026, q: 1 },
            created: day(2026, 7, 20),
            sequence: 1,
            deductor: deductor(),
            responsible: responsible(),
            previous_token: None,
            challans: vec![Challan {
                tax,
                interest: Amount::ZERO,
                fee: Amount::ZERO,
                others: Amount::ZERO,
                bsr: "0510308".into(),
                serial: "12345".into(),
                date: day(2026, 6, 5),
                minor_head: "200".into(),
                rows,
            }],
        }
    }

    fn fields(file: &str) -> Vec<(String, usize)> {
        file.split("\r\n").filter(|l| !l.is_empty()).map(|l| (l.split('^').nth(1).unwrap().to_owned(), l.split('^').count())).collect()
    }

    /// The record lengths of Protean's own sample files.
    #[test]
    fn record_lengths_match_the_spec_and_samples() {
        let s = statement(Form::F140, vec![row("Kumar Builders", Some("AAGCB7383J"), "1024", dec!(40000), dec!(800), dec!(2))]);
        let f = render(&s).unwrap();
        assert!(f.ends_with("\r\n") && !f.contains("\n\n"));
        let lens = fields(&f);
        assert_eq!(lens, [("FH".into(), 18), ("BH".into(), 72), ("CD".into(), 30), ("DD".into(), 45)]);
        // Protean's sample 140RQ1.txt: its header and deductee record have the same lengths.
        let sample_fh = "1^FH^NS1^R^06052026^1^D^BLRF02329F^1^Protean RPU 1.0^^^^^^^^";
        let sample_dd = "4^DD^1^1^1^O^^BRNPA6778G^ABCD^^^^^^1008^^^^02052026^1000.00^^^^900.00^900.00^^02052026^03.0000^^^^E^^^^^^^^^^^^^";
        assert_eq!((sample_fh.split('^').count(), sample_dd.split('^').count()), (18, 45));
        // Same positions: PAN 8, name 9, code 15, date 19, amount 20, tax 24 and 25, date 27, rate 28, flag 32.
        let at = |l: &str, i: usize| l.split('^').nth(i - 1).unwrap().to_owned();
        let ours = f.lines().nth(3).unwrap();
        for i in [8, 9, 15, 19, 20, 24, 25, 27, 28] {
            assert!(!at(sample_dd, i).is_empty() && !at(ours, i).is_empty(), "field {i}");
        }
    }

    #[test]
    fn golden_form_140() {
        let mut nopan = row("Ramesh Plumber", None, "1027", dec!(60000), dec!(12000), dec!(20));
        nopan.remark = Some('C');
        nopan.kind = Some(DeducteeKind::Individual);
        let s = statement(Form::F140, vec![row("Kumar Builders", Some("AAGCB7383J"), "1024", dec!(40000), dec!(800), dec!(2)), nopan]);
        let f = render(&s).unwrap();
        let golden = concat!(
            "1^FH^NS1^R^20072026^1^D^MUMA12345B^1^KhataOS^^^^^^^^\r\n",
            "2^BH^1^1^140^^^^^^^^MUMA12345B^^AAPFU0939F^202728^202627^Q1^Mehta Traders^INDIA^12 Market Road^Andheri East^^^^19^400069^accounts@mehta.example^91^9820012345^^F^Ravi Mehta^Partner^12 Market Road^^^^^19^400069^ravi@mehta.example^INDIA^91^9820012346^^12800.00^^^^^N^^^^^^^ABCPM1234K^^^^^^^^^^^^^\r\n",
            "3^CD^1^1^2^N^^12800.00^0.00^0.00^0.00^12800.00^C^^0510308^^12345^^05062026^^12800.00^12800.00^200^0.00^0.00^^^^^\r\n",
            "4^DD^1^1^1^O^^AAGCB7383J^Kumar Builders^^^^^^1024^^^Y^10052026^40000.00^^^^800.00^800.00^^10052026^2.0000^^^^^^^^^^^^^^^^^\r\n",
            "5^DD^1^2^1^O^^PANNOTAVBL^Ramesh Plumber^^^^^^1027^^^Y^10052026^60000.00^^^^12000.00^12000.00^^10052026^20.0000^^^^C^^^^^^^^^^^^^\r\n",
        );
        assert_eq!(f, golden);
    }

    #[test]
    fn golden_form_143() {
        let mut r = row("Kabadi Traders", Some("AAPFU0939F"), "1073", dec!(100000), dec!(2000), dec!(2));
        r.kind = Some(DeducteeKind::Firm);
        let mut nr = row("Gulf Metals LLC", None, "1073", dec!(50000), dec!(1000), dec!(2));
        nr.non_resident = true;
        let s = statement(Form::F143, vec![r, nr]);
        let f = render(&s).unwrap();
        let lines: Vec<&str> = f.split("\r\n").collect();
        assert!(lines[0].starts_with("1^FH^TC1^R^"));
        assert!(lines[1].starts_with("2^BH^1^1^143^"));
        assert_eq!(lines[3], "4^DD^1^1^1^O^^AAPFU0939F^Kabadi Traders^^N^7^^N^1073^^^Y^10052026^100000.00^^^^2000.00^2000.00^^10052026^2.0000^^^^^^^^^^^^^^^^^");
        assert_eq!(lines[4], "5^DD^1^2^1^O^^PANNOTAVBL^Gulf Metals LLC^^Y^1^N^N^1073^^^Y^10052026^50000.00^^^^1000.00^1000.00^^10052026^2.0000^^^^^^^^^^^^^^^^^");
        assert_eq!(fields(&f).iter().map(|x| x.1).collect::<Vec<_>>(), [18, 72, 30, 45, 45]);
    }

    #[test]
    fn golden_form_144_and_its_contact_rule() {
        let mut r = row("Acme Inc", None, "1057", dec!(100000), dec!(10000), dec!(10));
        r.nr = Some(NonResidentRow { remittance: Some("21".into()), country: "267".into(), treaty: true, ..NonResidentRow::default() });
        let s = statement(Form::F144, vec![r.clone()]);
        let errs = render(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("address in the country of residence")), "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("tax number")), "{errs:?}");
        r.nr = Some(NonResidentRow {
            remittance: Some("21".into()),
            country: "267".into(),
            treaty: true,
            grossed_up: false,
            address: "1 Main St, Austin TX".into(),
            email: "ap@acme.example".into(),
            phone: "15125550100".into(),
            tin: "12-3456789".into(),
        });
        let f = render(&statement(Form::F144, vec![r])).unwrap();
        let dd = f.split("\r\n").nth(3).unwrap();
        assert_eq!(dd, "4^DD^1^1^1^O^^PANNOTAVBL^Acme Inc^^^1^^N^1057^21^267^Y^10052026^100000.00^N^^^10000.00^10000.00^^10052026^10.0000^^B^^^^^^^1 Main St, Austin TX^ap@acme.example^15125550100^^12-3456789^^^^");
        assert_eq!(dd.split('^').count(), 45);
    }

    #[test]
    fn mandatory_fields_are_checked() {
        let mut s = statement(Form::F140, vec![row("Kumar Builders", Some("AAGCB7383J"), "1024", dec!(40000), dec!(800), dec!(2))]);
        s.deductor.tan = "MUM12345B".into();
        s.deductor.email = "no-at-sign".into();
        s.responsible.designation = "Chief Financial Officer and Partner".into();
        s.challans[0].bsr = "51030".into();
        s.challans[0].tax = dec!(500);
        s.challans[0].rows[0].date = day(2026, 7, 1);
        let errs = render(&s).unwrap_err();
        for want in ["TAN", "Deductor email", "designation is longer than 20", "BSR code must be 7 digits", "more than the challan's tax", "outside the quarter"] {
            assert!(errs.iter().any(|e| e.contains(want)), "{want}: {errs:?}");
        }
        let mut s = statement(Form::F140, vec![]);
        s.quarter = Quarter { year: 2025, q: 4 };
        let errs = render(&s).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("2026-27 onwards")));
        assert!(errs.iter().any(|e| e.contains("at least one")));
        let mut cert = statement(Form::F140, vec![row("X Ltd", Some("AAGCB7383J"), "1024", dec!(1), dec!(0), dec!(0))]);
        cert.challans[0].rows[0].remark = Some('A');
        assert!(render(&cert).unwrap_err().iter().any(|e| e.contains("certificate number")));
        cert.challans[0].rows[0].certificate = Some("LDC1234567".into());
        let f = render(&cert).unwrap();
        assert!(f.contains("^0.00^0.00^^^0.0000^^^^A^LDC1234567^"), "no deduction date and a zero rate when nothing was deducted: {f}");
    }

    #[test]
    fn annexures_and_addresses() {
        assert_eq!((state_from_gst("27"), state_from_gst("07"), state_from_gst("38"), state_from_gst("97")), (Some("19"), Some("09"), Some("37"), None));
        // Every current GST state or territory has a statement code, and the names agree.
        let norm = |s: &str| s.to_uppercase().replace(" & ", " AND ").replace("PUDUCHERRY", "PONDICHERRY").replace("LAKSHADWEEP", "LAKSHWADEEP");
        for st in crate::states::STATES.iter().filter(|s| !s.legacy && s.kind != crate::states::Kind::CentreJurisdiction && s.code != "97") {
            let code = state_from_gst(st.code).unwrap_or_else(|| panic!("{}", st.name));
            let name = STATES.iter().find(|x| x.0 == code).unwrap().2;
            assert_eq!(norm(st.name), norm(name), "{}", st.code);
        }
        assert_eq!(COUNTRIES.len(), 285);
        assert_eq!((country("267"), country("113"), country("999")), (Some("UNITED STATES OF AMERICA"), Some("INDIA"), None));
        assert_eq!((status_code(Some(DeducteeKind::Company)), status_code(Some(DeducteeKind::Huf)), status_code(None)), ("1", "3", "10"));
        assert_eq!(address_lines("Flat 4, Sunrise Apartments, 12 MG Road, Andheri East, Mumbai").unwrap(), ["Flat 4", "Sunrise Apartments", "12 MG Road, Andheri East", "Mumbai"]);
        assert_eq!(address_lines("").unwrap(), Vec::<String>::new());
        assert!(address_lines("a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t, u, v, w, x, y, z, aa, bb, cc, dd, ee").is_none_or(|l| l.len() <= 5));
        assert!(address_lines("Supercalifragilisticexpialidocious Road").is_none());
        assert_eq!(Form::F143.label(), "Form 143 (ex-27EQ)");
    }
}

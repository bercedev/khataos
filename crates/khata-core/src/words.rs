//! Amounts in words for invoices and cheques, by country and language.
//!
//! [`amount_words`] writes an amount the way that country's documents and
//! cheques usually do ([`WordsStyle`]): Indian English with lakh and crore
//! ("Rupees One Lakh Only"), British, American and Singapore cheque wording,
//! Gulf English, Arabic tafqit ("فقط … لا غير"), German, French, Spanish
//! (Spain and Mexico's CFDI "… pesos 25/100 M.N.") and Dutch.
//! [`crate::format::amount_in_words`] stays as it was for existing callers.
//! The conventions and their sources are in `docs/i18n.md`, "Amounts in words".

use rust_decimal::RoundingStrategy;

use crate::Amount;
use crate::format::{Grouping, number_in_words};

/// How an amount is written in words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordsStyle {
    /// "Rupees One Lakh Eighteen Thousand and Fifty Paise Only" (lakh, crore).
    India,
    /// "Four thousand five hundred pounds and twenty-five pence only".
    Uk,
    /// "Four Thousand Five Hundred and 25/100 Dollars" (the check line).
    Us,
    /// "Singapore Dollars Four Thousand Five Hundred and Cents Twenty-Five Only".
    Singapore,
    /// International English, currency first: "Saudi Riyals Four Thousand
    /// Five Hundred and Twenty-Five Halalas Only" (Gulf and other countries).
    English,
    /// "فقط أربعة آلاف وخمسمائة ريال سعودي لا غير".
    Arabic,
    /// "Viertausendfünfhundert Euro und fünfundzwanzig Cent".
    German,
    /// "Quatre mille cinq cents euros et vingt-cinq centimes".
    French,
    /// "Cuatro mil quinientos euros con veinticinco céntimos".
    Spanish,
    /// "Cuatro mil quinientos pesos 25/100 M.N." (CFDI printouts).
    Mexico,
    /// "Vierduizend vijfhonderd euro en vijfentwintig cent".
    Dutch,
}

impl WordsStyle {
    /// The style of a country's own documents (ISO 3166 alpha-2).
    pub fn for_country(country: Option<&str>) -> Self {
        match country.unwrap_or_default().trim().to_ascii_uppercase().as_str() {
            "IN" => Self::India,
            "GB" => Self::Uk,
            "US" => Self::Us,
            "SG" => Self::Singapore,
            "DE" | "AT" => Self::German,
            "FR" => Self::French,
            "ES" => Self::Spanish,
            "MX" => Self::Mexico,
            "NL" => Self::Dutch,
            _ => Self::English,
        }
    }

    /// The style for a print in `language` ("ar", "de", "fr-FR"…) by a
    /// company in `country`. Arabic, German, French, Spanish and Dutch pick
    /// that language; English or no language keeps the country's own style
    /// (a German company's invoice says "viertausendfünfhundert Euro").
    pub fn for_print(country: Option<&str>, language: &str) -> Self {
        let lang = language.trim().to_ascii_lowercase();
        let lang = lang.split(['-', '_']).next().unwrap_or_default();
        let mexico = country.is_some_and(|c| c.trim().eq_ignore_ascii_case("MX"));
        match lang {
            "ar" => Self::Arabic,
            "de" => Self::German,
            "fr" => Self::French,
            "nl" => Self::Dutch,
            "es" if mexico => Self::Mexico,
            "es" => Self::Spanish,
            _ => Self::for_country(country),
        }
    }
}

/// `amount` of `currency` (ISO 4217) in words, in `style`. Amounts are
/// rounded to the currency's decimals (three for KWD, BHD and OMR).
pub fn amount_words(amount: Amount, currency: &str, style: WordsStyle) -> String {
    let code = currency.trim().to_ascii_uppercase();
    let decimals = decimals(&code);
    let (whole, frac) = split(amount, decimals);
    let negative = amount.is_sign_negative() && (whole > 0 || frac > 0);
    match style {
        WordsStyle::India | WordsStyle::English | WordsStyle::Singapore | WordsStyle::Us | WordsStyle::Uk => {
            english(style, &code, whole, frac, decimals, negative)
        }
        WordsStyle::Arabic => arabic(&code, whole, frac, negative),
        WordsStyle::German => german(&code, whole, frac, negative),
        WordsStyle::French => french(&code, whole, frac, negative),
        WordsStyle::Spanish | WordsStyle::Mexico => spanish(style == WordsStyle::Mexico, &code, whole, frac, decimals, negative),
        WordsStyle::Dutch => dutch(&code, whole, frac, negative),
    }
}

/// Minor-unit decimals of a currency.
fn decimals(code: &str) -> u32 {
    match code {
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" | "LYD" | "IQD" => 3,
        "JPY" | "KRW" | "VND" => 0,
        _ => 2,
    }
}

/// The whole units and the minor units (hundredths, or thousandths).
fn split(amount: Amount, decimals: u32) -> (u64, u64) {
    let a = amount.abs().round_dp_with_strategy(decimals, RoundingStrategy::MidpointAwayFromZero);
    let whole = a.trunc();
    let scale = Amount::from(10u64.pow(decimals));
    let frac = ((a - whole) * scale).trunc();
    (whole.try_into().unwrap_or(u64::MAX), frac.try_into().unwrap_or(0))
}

fn capitalize(s: &str) -> String {
    // Dutch: "één" starts a sentence as "Eén".
    if let Some(rest) = s.strip_prefix("één") {
        return format!("Eén{rest}");
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// "pounds sterling" → "Pounds Sterling"; "US dollars" stays "US Dollars".
fn title_case(s: &str) -> String {
    s.split(' ').map(capitalize).collect::<Vec<_>>().join(" ")
}

// ------------------------------------------------------------------ English

/// English currency names, in running-text case.
struct EnCurrency {
    /// The name at home ("pound"/"pounds" in the UK, "dollars" in the US).
    local: (&'static str, &'static str),
    /// The name abroad ("pound sterling", "US dollars").
    intl: (&'static str, &'static str),
    minor: (&'static str, &'static str),
}

fn en_currency(code: &str) -> EnCurrency {
    let c = |local, intl, minor| EnCurrency { local, intl, minor };
    match code {
        "INR" => c(("rupee", "rupees"), ("Indian rupee", "Indian rupees"), ("paisa", "paise")),
        "USD" => c(("dollar", "dollars"), ("US dollar", "US dollars"), ("cent", "cents")),
        "EUR" => c(("euro", "euros"), ("euro", "euros"), ("cent", "cents")),
        "GBP" => c(("pound", "pounds"), ("pound sterling", "pounds sterling"), ("penny", "pence")),
        "AED" => c(("dirham", "dirhams"), ("UAE dirham", "UAE dirhams"), ("fils", "fils")),
        "SAR" => c(("riyal", "riyals"), ("Saudi riyal", "Saudi riyals"), ("halala", "halalas")),
        "QAR" => c(("riyal", "riyals"), ("Qatari riyal", "Qatari riyals"), ("dirham", "dirhams")),
        "OMR" => c(("rial", "rials"), ("Omani rial", "Omani rials"), ("baisa", "baisa")),
        "KWD" => c(("dinar", "dinars"), ("Kuwaiti dinar", "Kuwaiti dinars"), ("fils", "fils")),
        "BHD" => c(("dinar", "dinars"), ("Bahraini dinar", "Bahraini dinars"), ("fils", "fils")),
        "EGP" => c(("pound", "pounds"), ("Egyptian pound", "Egyptian pounds"), ("piastre", "piastres")),
        "SGD" => c(("dollar", "dollars"), ("Singapore dollar", "Singapore dollars"), ("cent", "cents")),
        "MXN" => c(("peso", "pesos"), ("Mexican peso", "Mexican pesos"), ("centavo", "centavos")),
        "AUD" => c(("dollar", "dollars"), ("Australian dollar", "Australian dollars"), ("cent", "cents")),
        "CAD" => c(("dollar", "dollars"), ("Canadian dollar", "Canadian dollars"), ("cent", "cents")),
        "CHF" => c(("franc", "francs"), ("Swiss franc", "Swiss francs"), ("rappen", "rappen")),
        "JPY" => c(("yen", "yen"), ("yen", "yen"), ("", "")),
        _ => EnCurrency { local: ("", ""), intl: ("", ""), minor: ("cent", "cents") },
    }
}

/// A whole number in British English: "one hundred and five thousand and twenty-one".
fn british(n: u64) -> String {
    fn hundreds(n: u64) -> String {
        let (h, rest) = (n / 100, n % 100);
        let tail = if rest == 0 { String::new() } else { number_in_words(rest, Grouping::Western) };
        match (h, rest) {
            (0, _) => tail,
            (_, 0) => format!("{} Hundred", number_in_words(h, Grouping::Western)),
            _ => format!("{} Hundred and {tail}", number_in_words(h, Grouping::Western)),
        }
    }
    if n == 0 {
        return "zero".into();
    }
    let scales: [(u64, &str); 4] = [(1_000_000_000_000, "Trillion"), (1_000_000_000, "Billion"), (1_000_000, "Million"), (1_000, "Thousand")];
    let mut parts = Vec::new();
    let mut rest = n;
    for (div, name) in scales {
        let q = rest / div;
        if q > 0 {
            let words = if q >= 1000 { british(q) } else { hundreds(q) };
            parts.push(format!("{words} {name}"));
            rest %= div;
        }
    }
    if rest > 0 {
        // "One thousand and five": "and" before a last part under a hundred.
        let and = if n >= 1000 && rest < 100 { "and " } else { "" };
        parts.push(format!("{and}{}", hundreds(rest)));
    }
    parts.join(" ").to_lowercase()
}

fn english(style: WordsStyle, code: &str, whole: u64, frac: u64, decimals: u32, negative: bool) -> String {
    let cur = en_currency(code);
    // An unknown currency: its code as the name.
    let intl = if cur.intl.1.is_empty() { (code, code) } else { cur.intl };
    let words = |n: u64| {
        if style == WordsStyle::India && code == "INR" { number_in_words(n, Grouping::Indian) } else { number_in_words(n, Grouping::Western) }
    };
    let out = match style {
        WordsStyle::Us => {
            let major = if code == "USD" { cur.local.1 } else { intl.1 };
            let fraction = if decimals == 0 { String::new() } else { format!(" and {frac:0w$}/{}", 10u64.pow(decimals), w = decimals as usize) };
            format!("{}{fraction} {}", words(whole), title_case(major))
        }
        WordsStyle::Uk => {
            let (sing, plural) = if code == "GBP" { cur.local } else { intl };
            let unit = |n: u64, (s, p): (&str, &str)| if n == 1 { s.to_owned() } else { p.to_owned() };
            let mut s = if whole == 0 && frac > 0 { String::new() } else { format!("{} {}", british(whole), unit(whole, (sing, plural))) };
            if frac > 0 {
                let minor = format!("{} {}", british(frac), unit(frac, cur.minor));
                s = if s.is_empty() { minor } else { format!("{s} and {minor}") };
            }
            format!("{} only", capitalize(&s))
        }
        WordsStyle::Singapore => {
            let mut s = format!("{} {}", title_case(intl.1), words(whole));
            if frac > 0 {
                s.push_str(&format!(" and {} {}", title_case(cur.minor.1), words(frac)));
            }
            format!("{s} Only")
        }
        // India and international: "{Currency} {words} and {minor words} {Minor} Only".
        _ => {
            let (major, minor) = if style == WordsStyle::India && code == "INR" { ("Rupees", "Paise") } else { (intl.1, cur.minor.1) };
            let mut s = format!("{} {}", title_case(major), words(whole));
            if frac > 0 {
                s.push_str(&format!(" and {} {}", words(frac), title_case(minor)));
            }
            format!("{s} Only")
        }
    };
    let out = out.trim().to_owned();
    if negative { format!("Minus {out}") } else { out }
}

// ------------------------------------------------------------------ Arabic

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gender {
    M,
    F,
}

/// A counted noun's forms: singular (also the genitive after 100, 1000…),
/// dual, plural (after 3–10) and accusative singular (after 11–99).
#[derive(Clone, Copy)]
struct ArNoun<'a> {
    sing: &'a str,
    dual: &'a str,
    plural: &'a str,
    acc: &'a str,
    gender: Gender,
}

const fn noun<'a>(sing: &'a str, dual: &'a str, plural: &'a str, acc: &'a str, gender: Gender) -> ArNoun<'a> {
    ArNoun { sing, dual, plural, acc, gender }
}

/// The currency and its unit, and the nationality adjective in the
/// masculine singular ("سعودي"), which agrees with the counted noun.
struct ArCurrency<'a> {
    major: ArNoun<'a>,
    minor: ArNoun<'a>,
    adjective: &'a str,
}

fn ar_currency(code: &str) -> ArCurrency<'static> {
    use Gender::{F, M};
    const RIYAL: ArNoun<'static> = noun("ريال", "ريالان", "ريالات", "ريالاً", M);
    const DIRHAM: ArNoun<'static> = noun("درهم", "درهمان", "دراهم", "درهماً", M);
    const DINAR: ArNoun<'static> = noun("دينار", "ديناران", "دنانير", "ديناراً", M);
    const POUND: ArNoun<'static> = noun("جنيه", "جنيهان", "جنيهات", "جنيهاً", M);
    const FILS: ArNoun<'static> = noun("فلس", "فلسان", "فلوس", "فلساً", M);
    const HALALA: ArNoun<'static> = noun("هللة", "هللتان", "هللات", "هللةً", F);
    const BAISA: ArNoun<'static> = noun("بيسة", "بيستان", "بيسات", "بيسةً", F);
    const QIRSH: ArNoun<'static> = noun("قرش", "قرشان", "قروش", "قرشاً", M);
    const CENT: ArNoun<'static> = noun("سنت", "سنتان", "سنتات", "سنتاً", M);
    let c = |major, minor, adjective| ArCurrency { major, minor, adjective };
    match code {
        "SAR" => c(RIYAL, HALALA, "سعودي"),
        "AED" => c(DIRHAM, FILS, "إماراتي"),
        "QAR" => c(RIYAL, DIRHAM, "قطري"),
        "OMR" => c(RIYAL, BAISA, "عماني"),
        "KWD" => c(DINAR, FILS, "كويتي"),
        "BHD" => c(DINAR, FILS, "بحريني"),
        "JOD" => c(DINAR, FILS, "أردني"),
        "EGP" => c(POUND, QIRSH, "مصري"),
        "USD" => c(noun("دولار", "دولاران", "دولارات", "دولاراً", M), CENT, "أمريكي"),
        "EUR" => c(noun("يورو", "يوروان", "يوروهات", "يورو", M), CENT, ""),
        "GBP" => c(POUND, noun("بنس", "بنسان", "بنسات", "بنساً", M), "إسترليني"),
        "INR" => c(noun("روبية", "روبيتان", "روبيات", "روبيةً", F), BAISA, "هندي"),
        "SGD" => c(noun("دولار", "دولاران", "دولارات", "دولاراً", M), CENT, "سنغافوري"),
        _ => ArCurrency { major: noun("", "", "", "", M), minor: CENT, adjective: "" },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Sing,
    Dual,
    Plural,
    Acc,
}

/// The nationality adjective agreeing with a noun's form and gender. Plurals
/// of things take the feminine singular ("ريالات سعودية").
fn ar_adjective(adj: &str, form: Form, gender: Gender) -> String {
    if adj.is_empty() {
        return String::new();
    }
    let ending = match (form, gender) {
        (Form::Sing, Gender::M) => "",
        (Form::Sing, Gender::F) | (Form::Plural, _) => "ة",
        (Form::Dual, Gender::M) => "ان",
        (Form::Dual, Gender::F) => "تان",
        (Form::Acc, Gender::M) => "اً",
        (Form::Acc, Gender::F) => "ةً",
    };
    format!(" {adj}{ending}")
}

/// 1–10 agreeing with the counted noun: 3–10 take the opposite gender's
/// form ("ثلاثة ريالات", "ثلاث هللات").
fn ar_unit(n: u64, g: Gender) -> &'static str {
    const M: [&str; 11] = ["", "واحد", "اثنان", "ثلاثة", "أربعة", "خمسة", "ستة", "سبعة", "ثمانية", "تسعة", "عشرة"];
    const F: [&str; 11] = ["", "واحدة", "اثنتان", "ثلاث", "أربع", "خمس", "ست", "سبع", "ثماني", "تسع", "عشر"];
    if g == Gender::M { M[n as usize] } else { F[n as usize] }
}

/// 1–99 for a noun of gender `g`.
fn ar_below_100(n: u64, g: Gender) -> String {
    const TENS: [&str; 10] = ["", "", "عشرون", "ثلاثون", "أربعون", "خمسون", "ستون", "سبعون", "ثمانون", "تسعون"];
    let f = g == Gender::F;
    match n {
        1..=10 => ar_unit(n, g).to_owned(),
        11 => (if f { "إحدى عشرة" } else { "أحد عشر" }).to_owned(),
        12 => (if f { "اثنتا عشرة" } else { "اثنا عشر" }).to_owned(),
        13..=19 => format!("{} {}", ar_unit(n - 10, g), if f { "عشرة" } else { "عشر" }),
        _ if n % 10 == 0 => TENS[(n / 10) as usize].to_owned(),
        _ => {
            let u = n % 10;
            let unit = if u == 1 && f { "إحدى" } else { ar_unit(u, g) };
            format!("{unit} و{}", TENS[(n / 10) as usize])
        }
    }
}

/// 1–999. `construct`: a noun follows straight after, so 200 is "مائتا".
fn ar_below_1000(n: u64, g: Gender, construct: bool) -> String {
    const HUNDREDS: [&str; 10] = ["", "مائة", "مائتان", "ثلاثمائة", "أربعمائة", "خمسمائة", "ستمائة", "سبعمائة", "ثمانمائة", "تسعمائة"];
    let (h, r) = (n / 100, n % 100);
    let mut parts = Vec::new();
    if h == 2 && r == 0 && construct {
        parts.push("مائتا".to_owned());
    } else if h > 0 {
        parts.push(HUNDREDS[h as usize].to_owned());
    }
    if r > 0 {
        parts.push(ar_below_100(r, g));
    }
    parts.join(" و")
}

/// The form a counted noun takes after `n` (n ≥ 3): plural after 3–10,
/// accusative singular after 11–99, genitive singular after hundreds and
/// thousands, and after a compound ending in 1 or 2 ("مائة وواحد ريال").
fn ar_form_after(n: u64) -> Form {
    match n % 100 {
        3..=10 => Form::Plural,
        11..=99 => Form::Acc,
        _ => Form::Sing,
    }
}

fn ar_noun_form<'a>(n: &ArNoun<'a>, form: Form) -> &'a str {
    match form {
        Form::Sing => n.sing,
        Form::Dual => n.dual,
        Form::Plural => n.plural,
        Form::Acc => n.acc,
    }
}

/// A whole number ≥ 1 counting a noun of gender `g`; `noun_follows` puts
/// a final 200 or 2000 in the construct state ("مائتا", "ألفا").
fn ar_number(n: u64, g: Gender, noun_follows: bool) -> String {
    // (divisor, singular, dual, dual in construct, plural, accusative)
    const SCALES: [(u64, &str, &str, &str, &str, &str); 3] = [
        (1_000_000_000, "مليار", "ملياران", "مليارا", "مليارات", "ملياراً"),
        (1_000_000, "مليون", "مليونان", "مليونا", "ملايين", "مليوناً"),
        (1_000, "ألف", "ألفان", "ألفا", "آلاف", "ألفاً"),
    ];
    let mut parts = Vec::new();
    let mut rest = n;
    for (div, sing, dual, dual_c, plural, acc) in SCALES {
        let c = rest / div;
        rest %= div;
        if c == 0 {
            continue;
        }
        let last = rest == 0 && noun_follows;
        parts.push(match c {
            1 => sing.to_owned(),
            2 => (if last { dual_c } else { dual }).to_owned(),
            // Beyond 999 billion: "ألف مليار".
            _ if c >= 1000 => format!("{} {sing}", ar_number(c, Gender::M, true)),
            _ => {
                // A last "ألف" before the noun is in construct, without
                // tanween: "أحد عشر ألف ريال", but "أحد عشر ألفاً وخمسة".
                let form = match ar_form_after(c) {
                    Form::Plural => plural,
                    Form::Acc if !last => acc,
                    _ => sing,
                };
                format!("{} {form}", ar_below_1000(c, Gender::M, true))
            }
        });
    }
    if rest > 0 {
        parts.push(ar_below_1000(rest, g, noun_follows));
    }
    parts.join(" و")
}

/// `n` of a noun: "ريال سعودي واحد", "ريالان سعوديان", "ثلاثة ريالات سعودية",
/// "أحد عشر ريالاً سعودياً", "مائة ريال سعودي".
fn ar_count(n: u64, unit: &ArNoun, adjective: &str) -> String {
    let adj = |form| ar_adjective(adjective, form, unit.gender);
    match n {
        0 => format!("صفر {}{}", unit.sing, adj(Form::Sing)),
        1 => format!("{}{} {}", unit.sing, adj(Form::Sing), ar_unit(1, unit.gender)),
        2 => format!("{}{}", unit.dual, adj(Form::Dual)),
        _ => {
            let form = ar_form_after(n);
            format!("{} {}{}", ar_number(n, unit.gender, true), ar_noun_form(unit, form), adj(form))
        }
    }
}

fn arabic(code: &str, whole: u64, frac: u64, negative: bool) -> String {
    let mut cur: ArCurrency = ar_currency(code);
    // An unknown currency: its code, as an invariable noun.
    if cur.major.sing.is_empty() {
        cur.major = noun(code, code, code, code, Gender::M);
    }
    let body = if whole == 0 && frac > 0 {
        ar_count(frac, &cur.minor, cur.adjective)
    } else {
        let mut s = ar_count(whole, &cur.major, cur.adjective);
        if frac > 0 {
            s.push_str(&format!(" و{}", ar_count(frac, &cur.minor, "")));
        }
        s
    };
    let sign = if negative { "سالب " } else { "" };
    format!("فقط {sign}{body} لا غير")
}

// ------------------------------------------------------------------ German

/// 1–999 in one word; `final_one`: a last 1 is "eins" ("einhunderteins"),
/// else "ein" before "tausend".
fn de_below_1000(n: u64, final_one: bool) -> String {
    const UNITS: [&str; 20] = [
        "", "ein", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun", "zehn", "elf", "zwölf", "dreizehn", "vierzehn",
        "fünfzehn", "sechzehn", "siebzehn", "achtzehn", "neunzehn",
    ];
    const TENS: [&str; 10] = ["", "", "zwanzig", "dreißig", "vierzig", "fünfzig", "sechzig", "siebzig", "achtzig", "neunzig"];
    let (h, r) = (n / 100, n % 100);
    let mut s = String::new();
    if h > 0 {
        // "einhundert", as amounts are written on cheques and transfers.
        s.push_str(UNITS[h as usize]);
        s.push_str("hundert");
    }
    match r {
        0 => {}
        1 if final_one => s.push_str("eins"),
        1..=19 => s.push_str(UNITS[r as usize]),
        _ if r % 10 == 0 => s.push_str(TENS[(r / 10) as usize]),
        _ => {
            s.push_str(UNITS[(r % 10) as usize]);
            s.push_str("und");
            s.push_str(TENS[(r / 10) as usize]);
        }
    }
    s
}

fn de_number(n: u64) -> String {
    if n == 0 {
        return "null".into();
    }
    let mut parts = Vec::new();
    let mut rest = n;
    for (div, one, many) in [(1_000_000_000, "eine Milliarde", "Milliarden"), (1_000_000, "eine Million", "Millionen")] {
        let c = rest / div;
        rest %= div;
        match c {
            0 => {}
            1 => parts.push(one.to_owned()),
            _ => parts.push(format!("{} {many}", de_number_below_million(c, false))),
        }
    }
    if rest > 0 {
        parts.push(de_number_below_million(rest, true));
    }
    parts.join(" ")
}

/// Under a million, written as one word: "viertausendfünfhundert".
fn de_number_below_million(n: u64, final_one: bool) -> String {
    let (t, r) = (n / 1000, n % 1000);
    let mut s = String::new();
    if t > 0 {
        s.push_str(&de_below_1000(t, false));
        s.push_str("tausend");
    }
    if r > 0 {
        s.push_str(&de_below_1000(r, final_one));
    }
    s
}

fn de_currency(code: &str) -> (&'static str, &'static str) {
    match code {
        "EUR" => ("Euro", "Cent"),
        "USD" => ("US-Dollar", "Cent"),
        "GBP" => ("Pfund Sterling", "Pence"),
        "CHF" => ("Franken", "Rappen"),
        _ => ("", "Cent"),
    }
}

fn german(code: &str, whole: u64, frac: u64, negative: bool) -> String {
    let (major, minor) = de_currency(code);
    let major = if major.is_empty() { code } else { major };
    // "ein Euro", "ein Cent": one before a noun.
    let count = |n: u64| if n == 1 { "ein".to_owned() } else { de_number(n) };
    let mut s = if whole == 0 && frac > 0 { String::new() } else { format!("{} {major}", count(whole)) };
    if frac > 0 {
        let m = format!("{} {minor}", count(frac));
        s = if s.is_empty() { m } else { format!("{s} und {m}") };
    }
    let s = if negative { format!("minus {s}") } else { s };
    capitalize(&s)
}

// ------------------------------------------------------------------ French

/// 1–99, traditional spelling: hyphens below 100, "et" in 21–71.
/// `ends`: nothing but a noun follows, so 80 is "quatre-vingts".
fn fr_below_100(n: u64, ends: bool) -> String {
    const UNITS: [&str; 17] = [
        "", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze", "douze", "treize", "quatorze", "quinze", "seize",
    ];
    const TENS: [&str; 7] = ["", "", "vingt", "trente", "quarante", "cinquante", "soixante"];
    let (t, u) = (n / 10, n % 10);
    match n {
        0 => String::new(),
        1..=16 => UNITS[n as usize].to_owned(),
        17..=19 => format!("dix-{}", UNITS[u as usize]),
        _ if t <= 6 => match u {
            0 => TENS[t as usize].to_owned(),
            1 => format!("{} et un", TENS[t as usize]),
            _ => format!("{}-{}", TENS[t as usize], UNITS[u as usize]),
        },
        70 => "soixante-dix".into(),
        71 => "soixante et onze".into(),
        72..=79 => format!("soixante-{}", fr_below_100(n - 60, false)),
        80 => (if ends { "quatre-vingts" } else { "quatre-vingt" }).into(),
        _ => format!("quatre-vingt-{}", fr_below_100(n - 80, false)),
    }
}

/// 1–999; `ends` as in [`fr_below_100`]: "deux cents" but "deux cent mille".
fn fr_below_1000(n: u64, ends: bool) -> String {
    let (h, r) = (n / 100, n % 100);
    let hundreds = match h {
        0 => String::new(),
        1 => "cent".to_owned(),
        _ => format!("{} cent{}", fr_below_100(h, false), if r == 0 && ends { "s" } else { "" }),
    };
    match (hundreds.is_empty(), r) {
        (_, 0) => hundreds,
        (true, _) => fr_below_100(r, ends),
        (false, _) => format!("{hundreds} {}", fr_below_100(r, ends)),
    }
}

fn fr_number(n: u64) -> String {
    if n == 0 {
        return "zéro".into();
    }
    let mut parts = Vec::new();
    let mut rest = n;
    for (div, name) in [(1_000_000_000, "milliard"), (1_000_000, "million")] {
        let c = rest / div;
        rest %= div;
        if c > 0 {
            // A noun: "deux cents millions", "quatre-vingts millions".
            let count = if c >= 1000 { fr_number(c) } else { fr_below_1000(c, true) };
            parts.push(format!("{count} {name}{}", if c > 1 { "s" } else { "" }));
        }
    }
    let (t, r) = (rest / 1000, rest % 1000);
    match t {
        0 => {}
        1 => parts.push("mille".into()),
        _ => parts.push(format!("{} mille", fr_below_1000(t, false))),
    }
    if r > 0 {
        parts.push(fr_below_1000(r, true));
    }
    parts.join(" ")
}

/// (singular, plural, feminine) of the currency and of its unit.
fn fr_currency(code: &str) -> ((&'static str, &'static str, bool), (&'static str, &'static str, bool)) {
    match code {
        "EUR" => (("euro", "euros", false), ("centime", "centimes", false)),
        "USD" => (("dollar", "dollars", false), ("cent", "cents", false)),
        "CHF" => (("franc", "francs", false), ("centime", "centimes", false)),
        "GBP" => (("livre sterling", "livres sterling", true), ("penny", "pence", false)),
        _ => (("", "", false), ("centime", "centimes", false)),
    }
}

/// `n` of a noun, with "de" after a round million ("un million d'euros").
fn fr_count(n: u64, (sing, plural, feminine): (&str, &str, bool)) -> String {
    let mut words = fr_number(n);
    if feminine && (words == "un" || words.ends_with(" un") || words.ends_with("-un")) {
        words.push('e');
    }
    let noun = if n >= 2 { plural } else { sing };
    if n >= 1_000_000 && n % 1_000_000 == 0 {
        let de = if noun.starts_with(['a', 'e', 'é', 'i', 'o', 'u', 'h']) { "d'" } else { "de " };
        format!("{words} {de}{noun}")
    } else {
        format!("{words} {noun}")
    }
}

fn french(code: &str, whole: u64, frac: u64, negative: bool) -> String {
    let (major, minor) = fr_currency(code);
    let major: (&str, &str, bool) = if major.0.is_empty() { (code, code, false) } else { major };
    let mut s = if whole == 0 && frac > 0 { String::new() } else { fr_count(whole, major) };
    if frac > 0 {
        let m = fr_count(frac, minor);
        s = if s.is_empty() { m } else { format!("{s} et {m}") };
    }
    let s = if negative { format!("moins {s}") } else { s };
    capitalize(&s)
}

// ------------------------------------------------------------------ Spanish

/// 1–99; `apocope`: a masculine noun or "mil" follows, so 1 is "un"
/// ("veintiún euros", "treinta y un mil").
fn es_below_100(n: u64, apocope: bool) -> String {
    const UNITS: [&str; 30] = [
        "", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "once", "doce", "trece", "catorce", "quince",
        "dieciséis", "diecisiete", "dieciocho", "diecinueve", "veinte", "veintiuno", "veintidós", "veintitrés", "veinticuatro", "veinticinco",
        "veintiséis", "veintisiete", "veintiocho", "veintinueve",
    ];
    const TENS: [&str; 10] = ["", "", "", "treinta", "cuarenta", "cincuenta", "sesenta", "setenta", "ochenta", "noventa"];
    let unit = |u: u64| match (u, apocope) {
        (1, true) => "un".to_owned(),
        (21, true) => "veintiún".to_owned(),
        _ => UNITS[u as usize].to_owned(),
    };
    match n {
        0 => String::new(),
        1..=29 => unit(n),
        _ if n % 10 == 0 => TENS[(n / 10) as usize].to_owned(),
        _ => format!("{} y {}", TENS[(n / 10) as usize], unit(n % 10)),
    }
}

fn es_below_1000(n: u64, apocope: bool) -> String {
    const HUNDREDS: [&str; 10] =
        ["", "ciento", "doscientos", "trescientos", "cuatrocientos", "quinientos", "seiscientos", "setecientos", "ochocientos", "novecientos"];
    let (h, r) = (n / 100, n % 100);
    match (h, r) {
        (1, 0) => "cien".into(),
        (0, _) => es_below_100(r, apocope),
        (_, 0) => HUNDREDS[h as usize].into(),
        _ => format!("{} {}", HUNDREDS[h as usize], es_below_100(r, apocope)),
    }
}

/// Under a million: "cuatro mil quinientos", "veintiún mil".
fn es_below_million(n: u64, apocope: bool) -> String {
    let (t, r) = (n / 1000, n % 1000);
    let mut parts = Vec::new();
    match t {
        0 => {}
        1 => parts.push("mil".to_owned()),
        _ => parts.push(format!("{} mil", es_below_1000(t, true))),
    }
    if r > 0 {
        parts.push(es_below_1000(r, apocope));
    }
    parts.join(" ")
}

/// A whole number before a masculine noun ("un", "veintiún"). Long scale:
/// 10⁹ is "mil millones", 10¹² "un billón".
fn es_number(n: u64) -> String {
    if n == 0 {
        return "cero".into();
    }
    let mut parts = Vec::new();
    let (b, rest) = (n / 1_000_000_000_000, n % 1_000_000_000_000);
    if b > 0 {
        parts.push(if b == 1 { "un billón".to_owned() } else { format!("{} billones", es_below_million(b, true)) });
    }
    let (m, rest) = (rest / 1_000_000, rest % 1_000_000);
    if m > 0 {
        parts.push(if m == 1 { "un millón".to_owned() } else { format!("{} millones", es_below_million(m, true)) });
    }
    if rest > 0 {
        parts.push(es_below_million(rest, true));
    }
    parts.join(" ")
}

fn es_currency(code: &str) -> ((&'static str, &'static str), (&'static str, &'static str)) {
    match code {
        "EUR" => (("euro", "euros"), ("céntimo", "céntimos")),
        "MXN" => (("peso", "pesos"), ("centavo", "centavos")),
        "USD" => (("dólar", "dólares"), ("centavo", "centavos")),
        "GBP" => (("libra esterlina", "libras esterlinas"), ("penique", "peniques")),
        _ => (("", ""), ("céntimo", "céntimos")),
    }
}

/// `n` of a masculine noun, with "de" after a round million ("un millón de pesos").
fn es_count(n: u64, (sing, plural): (&str, &str)) -> String {
    let noun = if n == 1 { sing } else { plural };
    let de = if n >= 1_000_000 && n % 1_000_000 == 0 { "de " } else { "" };
    format!("{} {de}{noun}", es_number(n))
}

fn spanish(mexico: bool, code: &str, whole: u64, frac: u64, decimals: u32, negative: bool) -> String {
    let (major, minor) = es_currency(code);
    let major: (&str, &str) = if major.0.is_empty() { (code, code) } else { major };
    let s = if mexico {
        // CFDI: "Cuatro mil quinientos pesos 25/100 M.N."; foreign currency: its code.
        let suffix = if code == "MXN" { "M.N." } else { code };
        let fraction = if decimals == 0 { String::new() } else { format!(" {frac:0w$}/{}", 10u64.pow(decimals), w = decimals as usize) };
        format!("{}{fraction} {suffix}", es_count(whole, major))
    } else {
        let mut s = if whole == 0 && frac > 0 { String::new() } else { es_count(whole, major) };
        if frac > 0 {
            let m = es_count(frac, minor);
            s = if s.is_empty() { m } else { format!("{s} con {m}") };
        }
        s
    };
    let s = if negative { format!("menos {s}") } else { s };
    capitalize(&s)
}

// ------------------------------------------------------------------ Dutch

/// 1–999 in one word: "honderdeen", "tweeëntwintig".
fn nl_below_1000(n: u64) -> String {
    const UNITS: [&str; 20] = [
        "", "een", "twee", "drie", "vier", "vijf", "zes", "zeven", "acht", "negen", "tien", "elf", "twaalf", "dertien", "veertien", "vijftien",
        "zestien", "zeventien", "achttien", "negentien",
    ];
    const TENS: [&str; 10] = ["", "", "twintig", "dertig", "veertig", "vijftig", "zestig", "zeventig", "tachtig", "negentig"];
    let (h, r) = (n / 100, n % 100);
    let mut s = String::new();
    match h {
        0 => {}
        1 => s.push_str("honderd"),
        _ => {
            s.push_str(UNITS[h as usize]);
            s.push_str("honderd");
        }
    }
    match r {
        0 => {}
        1..=19 => s.push_str(UNITS[r as usize]),
        _ if r % 10 == 0 => s.push_str(TENS[(r / 10) as usize]),
        _ => {
            let u = UNITS[(r % 10) as usize];
            s.push_str(u);
            // "tweeëntwintig", "drieëntwintig": a diaeresis after a final e.
            s.push_str(if u.ends_with('e') { "ën" } else { "en" });
            s.push_str(TENS[(r / 10) as usize]);
        }
    }
    s
}

/// Written together up to "duizend", with a space after it and around
/// "miljoen" and "miljard": "vierduizend vijfhonderd", "twee miljoen".
fn nl_number(n: u64) -> String {
    if n == 0 {
        return "nul".into();
    }
    let mut parts = Vec::new();
    let mut rest = n;
    for (div, name) in [(1_000_000_000, "miljard"), (1_000_000, "miljoen")] {
        let c = rest / div;
        rest %= div;
        if c > 0 {
            parts.push(format!("{} {name}", nl_number(c)));
        }
    }
    let (t, r) = (rest / 1000, rest % 1000);
    match t {
        0 => {}
        1 => parts.push("duizend".to_owned()),
        _ => parts.push(format!("{}duizend", nl_below_1000(t))),
    }
    if r > 0 {
        parts.push(nl_below_1000(r));
    }
    parts.join(" ")
}

fn nl_currency(code: &str) -> (&'static str, &'static str) {
    match code {
        "EUR" => ("euro", "cent"),
        "USD" => ("dollar", "cent"),
        "GBP" => ("pond", "pence"),
        _ => ("", "cent"),
    }
}

fn dutch(code: &str, whole: u64, frac: u64, negative: bool) -> String {
    let (major, minor) = nl_currency(code);
    let major = if major.is_empty() { code } else { major };
    // "één euro": the accent tells the number from the article.
    let count = |n: u64| if n == 1 { "één".to_owned() } else { nl_number(n) };
    let mut s = if whole == 0 && frac > 0 { String::new() } else { format!("{} {major}", count(whole)) };
    if frac > 0 {
        let m = format!("{} {minor}", count(frac));
        s = if s.is_empty() { m } else { format!("{s} en {m}") };
    }
    let s = if negative { format!("min {s}") } else { s };
    capitalize(&s)
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;
    use crate::format::amount_in_words;

    /// (amount, expected) for one style and currency.
    fn check(style: WordsStyle, currency: &str, cases: &[(Amount, &str)]) {
        for (a, want) in cases {
            assert_eq!(amount_words(*a, currency, style), *want, "{style:?} {currency} {a}");
        }
    }

    #[test]
    fn styles_by_country_and_language() {
        use WordsStyle as S;
        let c = |c: &str| S::for_country(Some(c));
        assert_eq!([c("IN"), c("GB"), c("US"), c("SG"), c("AE"), c("SA")], [S::India, S::Uk, S::Us, S::Singapore, S::English, S::English]);
        assert_eq!([c("DE"), c("FR"), c("ES"), c("MX"), c("NL"), c("AU")], [S::German, S::French, S::Spanish, S::Mexico, S::Dutch, S::English]);
        assert_eq!(S::for_country(None), S::English);
        assert_eq!(S::for_print(Some("SA"), "ar"), S::Arabic);
        assert_eq!(S::for_print(Some("IN"), "ar-SA"), S::Arabic);
        assert_eq!(S::for_print(Some("DE"), "en"), S::German, "English keeps the country's own");
        assert_eq!(S::for_print(Some("GB"), ""), S::Uk);
        assert_eq!(S::for_print(Some("MX"), "es"), S::Mexico);
        assert_eq!(S::for_print(Some("US"), "es-US"), S::Spanish);
        assert_eq!(S::for_print(Some("BE"), "fr"), S::French);
    }

    #[test]
    #[allow(clippy::inconsistent_digit_grouping)] // Indian grouping, on purpose
    fn india_is_unchanged() {
        // The old function and the India style agree everywhere.
        for a in [dec!(0), dec!(1), dec!(12345.50), dec!(118000.50), dec!(1_25_07_015), dec!(-250), dec!(0.05), dec!(99.999)] {
            assert_eq!(amount_words(a, "INR", WordsStyle::India), amount_in_words(a, Grouping::Indian, "Rupees", "Paise"), "{a}");
        }
        check(WordsStyle::India, "INR", &[
            (dec!(0), "Rupees Zero Only"),
            (dec!(1), "Rupees One Only"),
            (dec!(2), "Rupees Two Only"),
            (dec!(11), "Rupees Eleven Only"),
            (dec!(21), "Rupees Twenty-One Only"),
            (dec!(100), "Rupees One Hundred Only"),
            (dec!(101), "Rupees One Hundred One Only"),
            (dec!(1000), "Rupees One Thousand Only"),
            (dec!(1001), "Rupees One Thousand One Only"),
            (dec!(100000), "Rupees One Lakh Only"),
            (dec!(1000000), "Rupees Ten Lakh Only"),
            (dec!(118000.50), "Rupees One Lakh Eighteen Thousand and Fifty Paise Only"),
            (dec!(1_00_00_000), "Rupees One Crore Only"),
        ]);
        // A foreign-currency invoice from India: international scales and names.
        check(WordsStyle::India, "USD", &[(dec!(1500000.10), "US Dollars One Million Five Hundred Thousand and Ten Cents Only")]);
    }

    #[test]
    fn united_kingdom() {
        check(WordsStyle::Uk, "GBP", &[
            (dec!(0), "Zero pounds only"),
            (dec!(1), "One pound only"),
            (dec!(2), "Two pounds only"),
            (dec!(11), "Eleven pounds only"),
            (dec!(21), "Twenty-one pounds only"),
            (dec!(100), "One hundred pounds only"),
            (dec!(101), "One hundred and one pounds only"),
            (dec!(1000), "One thousand pounds only"),
            (dec!(1001), "One thousand and one pounds only"),
            (dec!(100000), "One hundred thousand pounds only"),
            (dec!(1000000), "One million pounds only"),
            (dec!(4500.25), "Four thousand five hundred pounds and twenty-five pence only"),
            (dec!(1247.50), "One thousand two hundred and forty-seven pounds and fifty pence only"),
            (dec!(56.23), "Fifty-six pounds and twenty-three pence only"),
            (dec!(1.01), "One pound and one penny only"),
            (dec!(0.50), "Fifty pence only"),
            (dec!(1000021), "One million and twenty-one pounds only"),
            (dec!(105000), "One hundred and five thousand pounds only"),
        ]);
        check(WordsStyle::Uk, "EUR", &[(dec!(2), "Two euros only")]);
        check(WordsStyle::Uk, "USD", &[(dec!(1), "One US dollar only")]);
    }

    #[test]
    fn united_states_check_line() {
        check(WordsStyle::Us, "USD", &[
            (dec!(0), "Zero and 00/100 Dollars"),
            (dec!(1), "One and 00/100 Dollars"),
            (dec!(2), "Two and 00/100 Dollars"),
            (dec!(11), "Eleven and 00/100 Dollars"),
            (dec!(21), "Twenty-One and 00/100 Dollars"),
            (dec!(100), "One Hundred and 00/100 Dollars"),
            (dec!(101), "One Hundred One and 00/100 Dollars"),
            (dec!(1000), "One Thousand and 00/100 Dollars"),
            (dec!(1001), "One Thousand One and 00/100 Dollars"),
            (dec!(100000), "One Hundred Thousand and 00/100 Dollars"),
            (dec!(1000000), "One Million and 00/100 Dollars"),
            (dec!(4500.25), "Four Thousand Five Hundred and 25/100 Dollars"),
            (dec!(125.75), "One Hundred Twenty-Five and 75/100 Dollars"),
            (dec!(0.07), "Zero and 07/100 Dollars"),
        ]);
        check(WordsStyle::Us, "CAD", &[(dec!(10.5), "Ten and 50/100 Canadian Dollars")]);
    }

    #[test]
    fn singapore() {
        check(WordsStyle::Singapore, "SGD", &[
            (dec!(0), "Singapore Dollars Zero Only"),
            (dec!(1), "Singapore Dollars One Only"),
            (dec!(2), "Singapore Dollars Two Only"),
            (dec!(11), "Singapore Dollars Eleven Only"),
            (dec!(21), "Singapore Dollars Twenty-One Only"),
            (dec!(100), "Singapore Dollars One Hundred Only"),
            (dec!(101), "Singapore Dollars One Hundred One Only"),
            (dec!(1000), "Singapore Dollars One Thousand Only"),
            (dec!(1001), "Singapore Dollars One Thousand One Only"),
            (dec!(100000), "Singapore Dollars One Hundred Thousand Only"),
            (dec!(1000000), "Singapore Dollars One Million Only"),
            (dec!(4500.25), "Singapore Dollars Four Thousand Five Hundred and Cents Twenty-Five Only"),
        ]);
    }

    #[test]
    fn gulf_english() {
        check(WordsStyle::English, "SAR", &[
            (dec!(0), "Saudi Riyals Zero Only"),
            (dec!(1), "Saudi Riyals One Only"),
            (dec!(21), "Saudi Riyals Twenty-One Only"),
            (dec!(1001), "Saudi Riyals One Thousand One Only"),
            (dec!(1000000), "Saudi Riyals One Million Only"),
            (dec!(4500.25), "Saudi Riyals Four Thousand Five Hundred and Twenty-Five Halalas Only"),
        ]);
        check(WordsStyle::English, "AED", &[(dec!(100), "UAE Dirhams One Hundred Only"), (dec!(0.5), "UAE Dirhams Zero and Fifty Fils Only")]);
        // Three-decimal currencies count thousandths.
        check(WordsStyle::English, "KWD", &[(dec!(12.250), "Kuwaiti Dinars Twelve and Two Hundred Fifty Fils Only")]);
        check(WordsStyle::English, "OMR", &[(dec!(1.005), "Omani Rials One and Five Baisa Only")]);
        // An unknown currency: its code.
        check(WordsStyle::English, "XYZ", &[(dec!(5), "XYZ Five Only")]);
        check(WordsStyle::English, "USD", &[(dec!(-5), "Minus US Dollars Five Only")]);
    }

    #[test]
    fn arabic_saudi_riyals() {
        check(WordsStyle::Arabic, "SAR", &[
            (dec!(0), "فقط صفر ريال سعودي لا غير"),
            // 1 and 2: the noun alone, singular or dual, the number after it.
            (dec!(1), "فقط ريال سعودي واحد لا غير"),
            (dec!(2), "فقط ريالان سعوديان لا غير"),
            // 3–10: the plural, the number in the other gender's form.
            (dec!(3), "فقط ثلاثة ريالات سعودية لا غير"),
            (dec!(10), "فقط عشرة ريالات سعودية لا غير"),
            // 11–99: the accusative singular.
            (dec!(11), "فقط أحد عشر ريالاً سعودياً لا غير"),
            (dec!(12), "فقط اثنا عشر ريالاً سعودياً لا غير"),
            (dec!(15), "فقط خمسة عشر ريالاً سعودياً لا غير"),
            (dec!(21), "فقط واحد وعشرون ريالاً سعودياً لا غير"),
            (dec!(99), "فقط تسعة وتسعون ريالاً سعودياً لا غير"),
            // Hundreds and thousands: the genitive singular.
            (dec!(100), "فقط مائة ريال سعودي لا غير"),
            (dec!(101), "فقط مائة وواحد ريال سعودي لا غير"),
            (dec!(105), "فقط مائة وخمسة ريالات سعودية لا غير"),
            (dec!(200), "فقط مائتا ريال سعودي لا غير"),
            (dec!(250), "فقط مائتان وخمسون ريالاً سعودياً لا غير"),
            (dec!(1000), "فقط ألف ريال سعودي لا غير"),
            (dec!(1001), "فقط ألف وواحد ريال سعودي لا غير"),
            (dec!(2000), "فقط ألفا ريال سعودي لا غير"),
            (dec!(2500), "فقط ألفان وخمسمائة ريال سعودي لا غير"),
            (dec!(3000), "فقط ثلاثة آلاف ريال سعودي لا غير"),
            (dec!(10000), "فقط عشرة آلاف ريال سعودي لا غير"),
            (dec!(11000), "فقط أحد عشر ألف ريال سعودي لا غير"),
            (dec!(11005), "فقط أحد عشر ألفاً وخمسة ريالات سعودية لا غير"),
            (dec!(100000), "فقط مائة ألف ريال سعودي لا غير"),
            (dec!(200000), "فقط مائتا ألف ريال سعودي لا غير"),
            (dec!(1000000), "فقط مليون ريال سعودي لا غير"),
            (dec!(2000000), "فقط مليونا ريال سعودي لا غير"),
            (dec!(5000000), "فقط خمسة ملايين ريال سعودي لا غير"),
            (dec!(25000000), "فقط خمسة وعشرون مليون ريال سعودي لا غير"),
            (dec!(1000000000), "فقط مليار ريال سعودي لا غير"),
            // Halalas are feminine: "خمس وعشرون هللةً", "هللتان", "ثلاث هللات".
            (dec!(4500.25), "فقط أربعة آلاف وخمسمائة ريال سعودي وخمس وعشرون هللةً لا غير"),
            (dec!(1.01), "فقط ريال سعودي واحد وهللة واحدة لا غير"),
            (dec!(1.02), "فقط ريال سعودي واحد وهللتان لا غير"),
            (dec!(5.03), "فقط خمسة ريالات سعودية وثلاث هللات لا غير"),
            (dec!(5.11), "فقط خمسة ريالات سعودية وإحدى عشرة هللةً لا غير"),
            (dec!(5.12), "فقط خمسة ريالات سعودية واثنتا عشرة هللةً لا غير"),
            (dec!(5.21), "فقط خمسة ريالات سعودية وإحدى وعشرون هللةً لا غير"),
            (dec!(5.18), "فقط خمسة ريالات سعودية وثماني عشرة هللةً لا غير"),
            (dec!(118000.50), "فقط مائة وثمانية عشر ألف ريال سعودي وخمسون هللةً لا غير"),
        ]);
    }

    /// Outputs published with the `tafqit` Dart package (pub.dev/packages/tafqit),
    /// spelt here with "مائة" and "لا غير" as two words.
    #[test]
    fn arabic_published_examples() {
        check(WordsStyle::Arabic, "SAR", &[
            (dec!(2.62), "فقط ريالان سعوديان واثنتان وستون هللةً لا غير"),
            (dec!(0.62), "فقط اثنتان وستون هللةً سعوديةً لا غير"),
            (dec!(-357.29), "فقط سالب ثلاثمائة وسبعة وخمسون ريالاً سعودياً وتسع وعشرون هللةً لا غير"),
        ]);
        // "مئتان وأربعة وثلاثون ألفاً ومئتان وأربع وثلاثون" with a feminine noun.
        check(WordsStyle::Arabic, "INR", &[(dec!(234234), "فقط مائتان وأربعة وثلاثون ألفاً ومائتان وأربع وثلاثون روبيةً هنديةً لا غير")]);
        // Alyafei's nArabicWords: "ألفا جنيه" (2000 followed by its noun), "ثلاثمائة وخمسون".
        check(WordsStyle::Arabic, "EGP", &[(dec!(2000), "فقط ألفا جنيه مصري لا غير"), (dec!(350), "فقط ثلاثمائة وخمسون جنيهاً مصرياً لا غير")]);
    }

    #[test]
    fn arabic_other_currencies() {
        check(WordsStyle::Arabic, "AED", &[
            (dec!(1), "فقط درهم إماراتي واحد لا غير"),
            (dec!(2), "فقط درهمان إماراتيان لا غير"),
            (dec!(7), "فقط سبعة دراهم إماراتية لا غير"),
            (dec!(21), "فقط واحد وعشرون درهماً إماراتياً لا غير"),
            (dec!(100.75), "فقط مائة درهم إماراتي وخمسة وسبعون فلساً لا غير"),
            (dec!(3.04), "فقط ثلاثة دراهم إماراتية وأربعة فلوس لا غير"),
        ]);
        // Three decimals: fils and baisa count thousandths.
        check(WordsStyle::Arabic, "KWD", &[
            (dec!(3), "فقط ثلاثة دنانير كويتية لا غير"),
            (dec!(1.250), "فقط دينار كويتي واحد ومائتان وخمسون فلساً لا غير"),
            (dec!(0.5), "فقط خمسمائة فلس كويتي لا غير"),
        ]);
        check(WordsStyle::Arabic, "BHD", &[(dec!(12), "فقط اثنا عشر ديناراً بحرينياً لا غير")]);
        check(WordsStyle::Arabic, "OMR", &[(dec!(2.002), "فقط ريالان عمانيان وبيستان لا غير"), (dec!(4.005), "فقط أربعة ريالات عمانية وخمس بيسات لا غير")]);
        check(WordsStyle::Arabic, "QAR", &[(dec!(10.10), "فقط عشرة ريالات قطرية وعشرة دراهم لا غير")]);
        check(WordsStyle::Arabic, "EGP", &[(dec!(3.15), "فقط ثلاثة جنيهات مصرية وخمسة عشر قرشاً لا غير")]);
        check(WordsStyle::Arabic, "USD", &[(dec!(1500), "فقط ألف وخمسمائة دولار أمريكي لا غير")]);
        check(WordsStyle::Arabic, "INR", &[(dec!(3), "فقط ثلاث روبيات هندية لا غير"), (dec!(11), "فقط إحدى عشرة روبيةً هنديةً لا غير")]);
    }

    #[test]
    fn german() {
        check(WordsStyle::German, "EUR", &[
            (dec!(0), "Null Euro"),
            (dec!(1), "Ein Euro"),
            (dec!(2), "Zwei Euro"),
            (dec!(11), "Elf Euro"),
            (dec!(21), "Einundzwanzig Euro"),
            (dec!(100), "Einhundert Euro"),
            (dec!(101), "Einhunderteins Euro"),
            (dec!(1000), "Eintausend Euro"),
            (dec!(1001), "Eintausendeins Euro"),
            (dec!(100000), "Einhunderttausend Euro"),
            (dec!(1000000), "Eine Million Euro"),
            (dec!(2300000), "Zwei Millionen dreihunderttausend Euro"),
            (dec!(4500), "Viertausendfünfhundert Euro"),
            (dec!(4500.25), "Viertausendfünfhundert Euro und fünfundzwanzig Cent"),
            (dec!(1.01), "Ein Euro und ein Cent"),
            (dec!(0.30), "Dreißig Cent"),
            (dec!(171000), "Einhunderteinundsiebzigtausend Euro"),
        ]);
    }

    #[test]
    fn french() {
        check(WordsStyle::French, "EUR", &[
            (dec!(0), "Zéro euro"),
            (dec!(1), "Un euro"),
            (dec!(2), "Deux euros"),
            (dec!(11), "Onze euros"),
            (dec!(21), "Vingt et un euros"),
            (dec!(71), "Soixante et onze euros"),
            (dec!(80), "Quatre-vingts euros"),
            (dec!(81), "Quatre-vingt-un euros"),
            (dec!(91), "Quatre-vingt-onze euros"),
            (dec!(100), "Cent euros"),
            (dec!(101), "Cent un euros"),
            (dec!(200), "Deux cents euros"),
            (dec!(201), "Deux cent un euros"),
            (dec!(1000), "Mille euros"),
            (dec!(1001), "Mille un euros"),
            (dec!(4500), "Quatre mille cinq cents euros"),
            (dec!(80000), "Quatre-vingt mille euros"),
            (dec!(100000), "Cent mille euros"),
            (dec!(200000), "Deux cent mille euros"),
            (dec!(1000000), "Un million d'euros"),
            (dec!(2000000), "Deux millions d'euros"),
            (dec!(200000000), "Deux cents millions d'euros"),
            (dec!(80000000), "Quatre-vingts millions d'euros"),
            (dec!(1000100), "Un million cent euros"),
            (dec!(4500.25), "Quatre mille cinq cents euros et vingt-cinq centimes"),
            (dec!(1.01), "Un euro et un centime"),
        ]);
        check(WordsStyle::French, "GBP", &[(dec!(21), "Vingt et une livres sterling")]);
    }

    #[test]
    fn spanish() {
        check(WordsStyle::Spanish, "EUR", &[
            (dec!(0), "Cero euros"),
            (dec!(1), "Un euro"),
            (dec!(2), "Dos euros"),
            (dec!(11), "Once euros"),
            (dec!(21), "Veintiún euros"),
            (dec!(31), "Treinta y un euros"),
            (dec!(100), "Cien euros"),
            (dec!(101), "Ciento un euros"),
            (dec!(500), "Quinientos euros"),
            (dec!(1000), "Mil euros"),
            (dec!(1001), "Mil un euros"),
            (dec!(21000), "Veintiún mil euros"),
            (dec!(100000), "Cien mil euros"),
            (dec!(1000000), "Un millón de euros"),
            (dec!(2500000), "Dos millones quinientos mil euros"),
            (dec!(1000000000), "Mil millones de euros"),
            (dec!(4500.25), "Cuatro mil quinientos euros con veinticinco céntimos"),
            (dec!(1.01), "Un euro con un céntimo"),
        ]);
    }

    #[test]
    fn mexico_cfdi() {
        check(WordsStyle::Mexico, "MXN", &[
            (dec!(0), "Cero pesos 00/100 M.N."),
            (dec!(1), "Un peso 00/100 M.N."),
            (dec!(2), "Dos pesos 00/100 M.N."),
            (dec!(11), "Once pesos 00/100 M.N."),
            (dec!(21), "Veintiún pesos 00/100 M.N."),
            (dec!(100), "Cien pesos 00/100 M.N."),
            (dec!(101), "Ciento un pesos 00/100 M.N."),
            (dec!(1000), "Mil pesos 00/100 M.N."),
            (dec!(1001), "Mil un pesos 00/100 M.N."),
            (dec!(100000), "Cien mil pesos 00/100 M.N."),
            (dec!(1000000), "Un millón de pesos 00/100 M.N."),
            (dec!(4500.25), "Cuatro mil quinientos pesos 25/100 M.N."),
            (dec!(317), "Trescientos diecisiete pesos 00/100 M.N."),
        ]);
        check(WordsStyle::Mexico, "USD", &[(dec!(10.5), "Diez dólares 50/100 USD")]);
    }

    #[test]
    fn dutch() {
        check(WordsStyle::Dutch, "EUR", &[
            (dec!(0), "Nul euro"),
            (dec!(1), "Eén euro"),
            (dec!(2), "Twee euro"),
            (dec!(11), "Elf euro"),
            (dec!(21), "Eenentwintig euro"),
            (dec!(22), "Tweeëntwintig euro"),
            (dec!(23), "Drieëntwintig euro"),
            (dec!(100), "Honderd euro"),
            (dec!(101), "Honderdeen euro"),
            (dec!(1000), "Duizend euro"),
            (dec!(1001), "Duizend een euro"),
            (dec!(2022), "Tweeduizend tweeëntwintig euro"),
            (dec!(100000), "Honderdduizend euro"),
            (dec!(1000000), "Een miljoen euro"),
            (dec!(4000000), "Vier miljoen euro"),
            (dec!(4500), "Vierduizend vijfhonderd euro"),
            (dec!(4500.25), "Vierduizend vijfhonderd euro en vijfentwintig cent"),
            (dec!(1.01), "Eén euro en één cent"),
        ]);
    }
}

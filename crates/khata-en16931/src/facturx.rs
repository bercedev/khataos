//! Factur-X / ZUGFeRD: a PDF/A-3 invoice a person can read, with the CII
//! XML embedded as `factur-x.xml`.
//!
//! The PDF is written by hand (no PDF crate): a text-only invoice in one
//! embedded monospace font (Courier Prime, SIL OFL 1.1,
//! `assets/CourierPrime-OFL.txt`), an sRGB output intent (the CC0
//! `sRGB-v2-micro.icc` of `github.com/saucecontrol/Compact-ICC-Profiles`),
//! XMP metadata with the PDF/A identification and the Factur-X extension
//! schema, and the XML as an associated file.
//!
//! What the Factur-X / ZUGFeRD specification asks of the PDF, as found in
//! the reference implementation (`github.com/akretion/factur-x`,
//! `facturx.py` and `xmp/Factur-X_extension_schema.xmp`) [SECONDARY: the
//! FNFE-MPE / FeRD specification PDF itself was not read]:
//! - PDF/A-3 (`pdfaid:part` 3, conformance B here);
//! - the XML embedded as `factur-x.xml`, MIME type `text/xml`, with
//!   `/AFRelationship` `/Data`, `/Source` or `/Alternative` (`/Data` only
//!   for MINIMUM and BASIC WL), listed in the catalog's `/AF` array and in
//!   the `/EmbeddedFiles` name tree;
//! - XMP properties in the namespace
//!   `urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#` (prefix `fx`):
//!   `DocumentType` INVOICE, `DocumentFileName`, `Version` 1.0 and
//!   `ConformanceLevel` (MINIMUM, BASIC WL, BASIC, EN 16931, EXTENDED,
//!   XRECHNUNG), with their PDF/A extension schema description. ZUGFeRD
//!   2.1 and later use the same namespace and file name.
//!
//! PDF/A-3 points this writer takes care of (ISO 19005-3, from secondary
//! descriptions; the ISO text was not read): file header with a binary
//! comment, cross-reference table with 20-byte entries, trailer `/ID`, no
//! encryption, an `/OutputIntents` entry with an ICC profile, every font
//! embedded with widths taken from the font program, an uncompressed XMP
//! metadata stream, no document information dictionary (so nothing can
//! disagree with the XMP), embedded files with `/Subtype`, `/Params
//! /ModDate`, `/F`, `/UF` and `/AFRelationship`. See [`CONFORMANCE_GAPS`]
//! for what was not verified.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::OnceLock;

use chrono::NaiveDateTime;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use serde::Serialize;

use crate::model::{Invoice, Party, TypeCode};
use crate::xml::{amount, number, some};

/// The name of the embedded XML file (Factur-X 1.0 and ZUGFeRD 2.1+).
pub const FILE_NAME: &str = "factur-x.xml";
/// The XMP namespace of the Factur-X properties (note the trailing `#`).
pub const FX_NAMESPACE: &str = "urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#";

static FONT_DATA: &[u8] = include_bytes!("../assets/CourierPrime-Regular.ttf");
static ICC_PROFILE: &[u8] = include_bytes!("../assets/sRGB-v2-micro.icc");

/// The Factur-X profile of the embedded XML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Profile {
    /// EN 16931 ("COMFORT" in ZUGFeRD 1): the full core invoice.
    En16931,
}

impl Profile {
    /// `fx:ConformanceLevel` in the XMP.
    pub fn xmp_level(self) -> &'static str {
        match self {
            Profile::En16931 => "EN 16931",
        }
    }

    /// The CII `GuidelineSpecifiedDocumentContextParameter/ID` (BT-24) of
    /// the profile: plain EN 16931 (`urn:cen.eu:en16931:2017`).
    pub fn guideline_id(self) -> &'static str {
        match self {
            Profile::En16931 => crate::EN16931_ID,
        }
    }

    /// `/AFRelationship` of the embedded XML: `Alternative` (the XML is an
    /// equivalent form of the visible invoice); only MINIMUM and BASIC WL
    /// must use `Data`.
    pub fn af_relationship(self) -> &'static str {
        match self {
            Profile::En16931 => "Alternative",
        }
    }
}

/// How the PDF is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfOptions {
    /// When the file is created (UTC): the XMP dates and the embedded
    /// file's modification date.
    pub created: NaiveDateTime,
    pub profile: Profile,
}

/// What this writer does not do, or what could not be checked, for full
/// PDF/A-3 and Factur-X conformance.
pub const CONFORMANCE_GAPS: &[&str] = &[
    "Not validated with veraPDF (PDF/A-3b) or a Factur-X/ZUGFeRD validator (Mustang, FNFE-MPE/FeRD Schematron): the structure follows the published requirements, but no conformance checker was run.",
    "PDF/A-3 level B only: no tagged PDF (level A) and no ToUnicode map (level U). Text extraction relies on the WinAnsi encoding of the embedded TrueType font.",
    "Only WinAnsi (Windows-1252) characters can be shown; others (Arabic, Devanagari, ₹, …) print as '?'. The embedded XML keeps them.",
    "The visible invoice is plain monospaced text, not the KhataOS invoice layout, and has no logo.",
    "The embedded XML is UN/CEFACT CII D16B (EN 16931 profile). Factur-X 1.08+/ZUGFeRD 2.4+ publish D22B schemas that are said to be backward compatible with D16B [UNCERTAIN: not checked against the D22B XSD].",
    "The whole font is embedded (not subset), so each PDF carries about 40 KB of font data.",
    "French e-invoicing (2026 reform) and German B2B rules on which profile and AFRelationship to use were not checked beyond the Factur-X reference implementation.",
];

// ---------------------------------------------------------------- font

/// What the PDF needs from the TrueType font.
struct FontInfo {
    units_per_em: i32,
    bbox: [i32; 4],
    ascent: i32,
    descent: i32,
    cap_height: i32,
    italic_angle: f64,
    /// Width (1/1000 em) of codes 32..=255; 0 for codes with no glyph.
    widths: [i32; 224],
    /// Whether code 32 + i has a glyph.
    has_glyph: [bool; 224],
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], o: usize) -> i16 {
    u16_at(d, o) as i16
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn table(d: &[u8], tag: &[u8; 4]) -> Option<usize> {
    let n = usize::from(u16_at(d, 4));
    (0..n).map(|i| 12 + 16 * i).find(|&r| d.get(r..r + 4) == Some(&tag[..])).map(|r| u32_at(d, r + 8) as usize)
}

/// The glyph of a character in a (3,1) cmap subtable of format 4 at `s`.
fn glyph_fmt4(d: &[u8], s: usize, c: u32) -> u16 {
    if c > 0xFFFF {
        return 0;
    }
    let c = c as u16;
    let seg = usize::from(u16_at(d, s + 6)) / 2;
    let ends = s + 14;
    let starts = ends + 2 * seg + 2;
    let deltas = starts + 2 * seg;
    let offsets = deltas + 2 * seg;
    for i in 0..seg {
        let end = u16_at(d, ends + 2 * i);
        if c > end {
            continue;
        }
        let start = u16_at(d, starts + 2 * i);
        if c < start {
            return 0;
        }
        let delta = u16_at(d, deltas + 2 * i);
        let ro = usize::from(u16_at(d, offsets + 2 * i));
        if ro == 0 {
            return c.wrapping_add(delta);
        }
        let g = u16_at(d, offsets + 2 * i + ro + 2 * usize::from(c - start));
        return if g == 0 { 0 } else { g.wrapping_add(delta) };
    }
    0
}

fn scale(v: i32, upem: i32) -> i32 {
    let x = v * 1000;
    if x >= 0 { (x + upem / 2) / upem } else { (x - upem / 2) / upem }
}

/// Windows-1252 (WinAnsiEncoding) code → Unicode, `None` where undefined.
fn cp1252(code: u8) -> Option<char> {
    const HIGH: [u32; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017D, 0, 0,
        0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    match code {
        0x20..=0x7E | 0xA0..=0xFF => Some(char::from(code)),
        0x80..=0x9F => char::from_u32(HIGH[usize::from(code - 0x80)]).filter(|&c| c != '\0'),
        _ => None,
    }
}

fn font() -> &'static FontInfo {
    static INFO: OnceLock<FontInfo> = OnceLock::new();
    INFO.get_or_init(|| {
        let d = FONT_DATA;
        let head = table(d, b"head").expect("head table");
        let hhea = table(d, b"hhea").expect("hhea table");
        let hmtx = table(d, b"hmtx").expect("hmtx table");
        let cmap = table(d, b"cmap").expect("cmap table");
        let upem = i32::from(u16_at(d, head + 18));
        let bbox = [36, 38, 40, 42].map(|o| scale(i32::from(i16_at(d, head + o)), upem));
        let ascent = scale(i32::from(i16_at(d, hhea + 4)), upem);
        let descent = scale(i32::from(i16_at(d, hhea + 6)), upem);
        let metrics = usize::from(u16_at(d, hhea + 34));
        let cap_height = table(d, b"OS/2")
            .filter(|&o| u16_at(d, o) >= 2)
            .map(|o| scale(i32::from(i16_at(d, o + 88)), upem))
            .unwrap_or(ascent);
        let italic_angle = table(d, b"post").map_or(0.0, |o| f64::from(u32_at(d, o + 4) as i32) / 65536.0);
        let n = usize::from(u16_at(d, cmap + 2));
        let sub = (0..n)
            .map(|i| cmap + 4 + 8 * i)
            .find(|&r| u16_at(d, r) == 3 && u16_at(d, r + 2) == 1)
            .map(|r| cmap + u32_at(d, r + 4) as usize)
            .filter(|&s| u16_at(d, s) == 4)
            .expect("a (3,1) cmap of format 4");
        let mut widths = [0; 224];
        let mut has_glyph = [false; 224];
        for code in 32u8..=255 {
            let i = usize::from(code - 32);
            let Some(c) = cp1252(code) else { continue };
            let g = glyph_fmt4(d, sub, c as u32);
            if g == 0 {
                continue;
            }
            let g = usize::from(g).min(metrics.saturating_sub(1));
            widths[i] = scale(i32::from(u16_at(d, hmtx + 4 * g)), upem);
            has_glyph[i] = true;
        }
        FontInfo { units_per_em: upem, bbox, ascent, descent, cap_height, italic_angle, widths, has_glyph }
    })
}

/// Text as WinAnsi bytes; characters without a code or a glyph become '?',
/// tabs become spaces and other control characters are dropped.
pub(crate) fn encode(text: &str) -> Vec<u8> {
    let f = font();
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let c = if c == '\t' { ' ' } else { c };
        if c.is_control() {
            continue;
        }
        let code = (32u8..=255).find(|&b| cp1252(b) == Some(c) && f.has_glyph[usize::from(b - 32)]);
        out.push(code.unwrap_or(b'?'));
    }
    out
}

// ---------------------------------------------------------------- text

/// Characters per line of the page (9 pt Courier Prime on A4 with 42 pt margins).
const WIDTH: usize = 95;
/// Text lines per page, leaving room for the footer.
const LINES_PER_PAGE: usize = 62;
const PAGE_W: i32 = 595;
const PAGE_H: i32 = 842;
const MARGIN: i32 = 42;
const FONT_SIZE: f64 = 9.0;
const LEADING: f64 = 11.5;

fn len(s: &str) -> usize {
    s.chars().count()
}

fn fit(s: &str, w: usize) -> String {
    s.chars().take(w).collect()
}

fn left(s: &str, w: usize) -> String {
    let s = fit(s, w);
    let n = len(&s);
    s + &" ".repeat(w - n)
}

fn right(s: &str, w: usize) -> String {
    let s = fit(s, w);
    " ".repeat(w - len(&s)) + &s
}

/// Word-wrap to `w` characters; words longer than a line are cut.
fn wrap(text: &str, w: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let mut word = word.to_owned();
            while len(&word) > w {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                out.push(fit(&word, w));
                word = word.chars().skip(w).collect();
            }
            if line.is_empty() {
                line = word;
            } else if len(&line) + 1 + len(&word) <= w {
                line.push(' ');
                line.push_str(&word);
            } else {
                out.push(std::mem::replace(&mut line, word));
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    out
}

/// Fields side by side, three spaces apart, on as many lines as needed.
fn fields(parts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in parts {
        match out.last_mut() {
            Some(l) if len(l) + 3 + len(p) <= WIDTH => {
                l.push_str("   ");
                l.push_str(p);
            }
            _ => out.extend(wrap(p, WIDTH)),
        }
    }
    out
}

fn title(t: TypeCode) -> &'static str {
    match t {
        TypeCode::Commercial => "INVOICE",
        TypeCode::CreditNote => "CREDIT NOTE",
        TypeCode::Corrected => "CORRECTED INVOICE",
        TypeCode::SelfBilled => "SELF-BILLED INVOICE",
        TypeCode::Partial => "PARTIAL INVOICE",
    }
}

const COL: usize = 46;

fn party_lines(heading: &str, p: &Party, seller: bool) -> Vec<String> {
    let mut v = vec![heading.to_owned()];
    let mut push = |s: String| v.extend(wrap(&s, COL));
    push(p.name.trim().to_owned());
    if let Some(t) = p.trading_name.as_deref().and_then(some) {
        push(format!("Trading as {t}"));
    }
    let a = &p.address;
    for l in [&a.line1, &a.line2, &a.line3] {
        if let Some(l) = some(l) {
            push(l.to_owned());
        }
    }
    let city = [a.postal_code.trim(), a.city.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
    if !city.is_empty() {
        push(city);
    }
    if let Some(s) = some(&a.subdivision) {
        push(s.to_owned());
    }
    if let Some(c) = some(&a.country) {
        push(c.to_owned());
    }
    if let Some(id) = p.vat_id.as_deref().and_then(some) {
        push(format!("VAT ID: {id}"));
    }
    if seller && let Some(id) = p.tax_registration.as_deref().and_then(some) {
        push(format!("Tax number: {id}"));
    }
    if let Some(id) = p.legal_id.as_deref().and_then(some) {
        push(format!("Registration: {id}"));
    }
    if let Some(e) = p.electronic_address.as_ref().filter(|e| some(&e.value).is_some()) {
        push(format!("E-address: {} ({})", e.value.trim(), e.scheme.trim()));
    }
    let c = &p.contact;
    if let Some(n) = some(&c.name) {
        push(format!("Contact: {n}"));
    }
    if let Some(n) = some(&c.phone) {
        push(format!("Tel: {n}"));
    }
    if let Some(n) = some(&c.email) {
        push(format!("Email: {n}"));
    }
    v
}

fn rate_label(code: &str, rate: Option<rust_decimal::Decimal>) -> String {
    match rate {
        Some(r) => format!("{code} {}%", number(r)),
        None => code.to_owned(),
    }
}

fn date(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// The visible invoice as monospace text lines of at most 95 characters.
pub fn render_text(inv: &Invoice) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let rule = "-".repeat(WIDTH);
    let cur = inv.currency.trim();
    out.push(title(inv.type_code).to_owned());
    out.push(String::new());
    let mut head = vec![format!("Number: {}", inv.number.trim()), format!("Issue date: {}", date(inv.issue_date))];
    if let Some(d) = inv.due_date {
        head.push(format!("Due date: {}", date(d)));
    }
    out.extend(fields(&head));
    let mut more = Vec::new();
    if let Some(d) = inv.delivery_date {
        more.push(format!("Delivery date: {}", date(d)));
    }
    if let Some(d) = inv.tax_point_date {
        more.push(format!("Tax point date: {}", date(d)));
    }
    if let Some((s, e)) = inv.period {
        let f = |d: Option<chrono::NaiveDate>| d.map(date).unwrap_or_else(|| "…".into());
        more.push(format!("Period: {} to {}", f(s), f(e)));
    }
    more.push(format!("Currency: {cur}"));
    out.extend(fields(&more));
    out.push(String::new());

    let seller = party_lines("SELLER", &inv.seller, true);
    let buyer = party_lines("BUYER", &inv.buyer, false);
    for i in 0..seller.len().max(buyer.len()) {
        let l = seller.get(i).map_or("", String::as_str);
        let r = buyer.get(i).map_or("", String::as_str);
        out.push(format!("{}   {}", left(l, COL), r).trim_end().to_owned());
    }
    out.push(String::new());
    let mut refs = Vec::new();
    if let Some(r) = inv.buyer_reference.as_deref().and_then(some) {
        refs.push(format!("Buyer reference: {r}"));
    }
    if let Some(r) = inv.order_reference.as_deref().and_then(some) {
        refs.push(format!("Order: {r}"));
    }
    if let Some(p) = &inv.preceding {
        refs.push(match p.date {
            Some(d) => format!("Corrects invoice {} of {}", p.number.trim(), date(d)),
            None => format!("Corrects invoice {}", p.number.trim()),
        });
    }
    for r in refs {
        out.extend(wrap(&r, WIDTH));
    }
    if out.last().is_some_and(|l| !l.is_empty()) {
        out.push(String::new());
    }

    // Lines: 4 + 30 + 10 + 5 + 13 + 14 + 9 and six separating spaces = 91.
    let row = |a: &str, b: &str, c: &str, d: &str, e: &str, f: &str, g: &str| {
        format!("{} {} {} {} {} {} {}", left(a, 4), left(b, 30), right(c, 10), left(d, 5), right(e, 13), right(f, 14), left(g, 9))
            .trim_end()
            .to_owned()
    };
    out.push(row("#", "Item", "Quantity", "Unit", "Price", "Net amount", "VAT"));
    out.push(rule.clone());
    for l in &inv.lines {
        let name = wrap(l.item_name.trim(), 30);
        let mut price = number(l.net_price);
        if let Some(b) = l.price_base_quantity {
            price = format!("{price}/{}", number(b));
        }
        out.push(row(
            l.id.trim(),
            name.first().map_or("", String::as_str),
            &number(l.quantity),
            l.unit_code.trim(),
            &price,
            &amount(l.net_amount),
            &rate_label(l.category.code(), l.rate),
        ));
        for n in name.iter().skip(1) {
            out.push(format!("     {n}"));
        }
        for extra in [l.item_description.as_deref(), l.note.as_deref()].into_iter().flatten().filter_map(some) {
            out.extend(wrap(extra, WIDTH - 5).into_iter().map(|s| format!("     {s}")));
        }
        if let Some(id) = l.seller_item_id.as_deref().and_then(some) {
            out.push(format!("     Item no. {id}"));
        }
    }
    out.push(rule.clone());
    for a in &inv.allowances_charges {
        let what = if a.charge { "Charge" } else { "Allowance" };
        let reason = a.reason.as_deref().and_then(some).or(a.reason_code.as_deref().and_then(some)).unwrap_or("");
        let signed = if a.charge { amount(a.amount) } else { format!("-{}", amount(a.amount)) };
        out.push(row("", &format!("{what}: {reason}"), "", "", "", &signed, &rate_label(a.category.code(), a.rate)));
    }
    if !inv.allowances_charges.is_empty() {
        out.push(String::new());
    }

    out.push("VAT BREAKDOWN".to_owned());
    out.push(format!("{} {} {} {}", left("Category", 30), right("Rate", 10), right("Taxable amount", 20), right("VAT", 20)));
    for b in &inv.vat_breakdown {
        let rate = b.rate.map(|r| format!("{}%", number(r))).unwrap_or_default();
        out.push(format!(
            "{} {} {} {}",
            left(&format!("{} {}", b.category.code(), b.category.name()), 30),
            right(&rate, 10),
            right(&amount(b.taxable), 20),
            right(&amount(b.tax), 20)
        ));
        let reason = [b.exemption_code.as_deref(), b.exemption_reason.as_deref()].into_iter().flatten().filter_map(some).collect::<Vec<_>>();
        if !reason.is_empty() {
            out.extend(wrap(&reason.join(": "), WIDTH - 5).into_iter().map(|s| format!("     {s}")));
        }
    }
    out.push(String::new());

    let t = &inv.totals;
    let total = |label: &str, a: rust_decimal::Decimal| format!("{} {}", right(label, 70), right(&format!("{} {cur}", amount(a)), 20));
    out.push(total("Sum of lines", t.line_net));
    if !t.allowances.is_zero() {
        out.push(total("Allowances", -t.allowances));
    }
    if !t.charges.is_zero() {
        out.push(total("Charges", t.charges));
    }
    out.push(total("Total without VAT", t.without_vat));
    out.push(total("VAT", t.vat));
    out.push(total("Total with VAT", t.with_vat));
    if !t.paid.is_zero() {
        out.push(total("Paid", -t.paid));
    }
    if !t.rounding.is_zero() {
        out.push(total("Rounding", t.rounding));
    }
    out.push(total("Amount due", t.due));
    out.push(String::new());

    if let Some(p) = &inv.payment {
        let means = match p.means_text.as_deref().and_then(some) {
            Some(text) => format!("Payment: {text} (code {})", p.means_code.trim()),
            None => format!("Payment means code: {}", p.means_code.trim()),
        };
        out.extend(wrap(&means, WIDTH));
        if let Some(ct) = &p.credit_transfer {
            out.push(format!("Account: {}", ct.account.trim()));
            if let Some(b) = ct.bic.as_deref().and_then(some) {
                out.push(format!("BIC: {b}"));
            }
            if let Some(n) = ct.name.as_deref().and_then(some) {
                out.extend(wrap(&format!("Account name: {n}"), WIDTH));
            }
        }
        if let Some(r) = p.remittance.as_deref().and_then(some) {
            out.extend(wrap(&format!("Reference: {r}"), WIDTH));
        }
    }
    if let Some(terms) = inv.payment_terms.as_deref().and_then(some) {
        out.extend(wrap(&format!("Terms: {terms}"), WIDTH));
    }
    if !inv.notes.is_empty() {
        out.push(String::new());
        for n in &inv.notes {
            out.extend(wrap(n, WIDTH));
        }
    }
    out.push(String::new());
    out.extend(wrap(
        &format!("Structured invoice data embedded as {FILE_NAME} (Factur-X / ZUGFeRD, profile EN 16931)."),
        WIDTH,
    ));
    out
}

// ---------------------------------------------------------------- PDF

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::best());
    e.write_all(data).expect("in-memory write");
    e.finish().expect("in-memory write")
}

/// A literal string: `(`, `)` and `\` escaped; other bytes as they are.
fn literal(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 2);
    out.push(b'(');
    for &b in bytes {
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

fn xml_escape(s: &str) -> String {
    crate::xml::escape(s)
}

struct Writer {
    buf: Vec<u8>,
    offsets: Vec<usize>,
}

impl Writer {
    fn object(&mut self, id: usize, body: &[u8]) {
        debug_assert_eq!(id, self.offsets.len() + 1, "objects are written in order");
        self.offsets.push(self.buf.len());
        self.buf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.buf.extend_from_slice(body);
        self.buf.extend_from_slice(b"\nendobj\n");
    }

    /// A stream object; `dict` holds the entries other than `/Length`.
    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let mut body = format!("<< /Length {} {dict} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(id, &body);
    }
}

/// Two FNV-1a hashes with different offsets: 16 bytes for the trailer `/ID`.
fn file_id(data: &[u8]) -> String {
    let mut out = String::new();
    for seed in [0xcbf2_9ce4_8422_2325u64, 0x6c62_272e_07bb_0142] {
        let mut h = seed;
        for &b in data {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        let _ = write!(out, "{h:016X}");
    }
    out
}

fn xmp(inv: &Invoice, opts: &PdfOptions) -> String {
    let when = opts.created.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
    let title = xml_escape(&format!("{} {}", title(inv.type_code), inv.number.trim()));
    let creator = xml_escape(inv.seller.name.trim());
    let level = opts.profile.xmp_level();
    format!(
        r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
      <pdfaid:part>3</pdfaid:part>
      <pdfaid:conformance>B</pdfaid:conformance>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
      <dc:format>application/pdf</dc:format>
      <dc:title>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">{title}</rdf:li>
        </rdf:Alt>
      </dc:title>
      <dc:creator>
        <rdf:Seq>
          <rdf:li>{creator}</rdf:li>
        </rdf:Seq>
      </dc:creator>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
      <xmp:CreatorTool>KhataOS</xmp:CreatorTool>
      <xmp:CreateDate>{when}</xmp:CreateDate>
      <xmp:ModifyDate>{when}</xmp:ModifyDate>
      <xmp:MetadataDate>{when}</xmp:MetadataDate>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
      <pdf:Producer>KhataOS khata-en16931</pdf:Producer>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:fx="{FX_NAMESPACE}">
      <fx:DocumentType>INVOICE</fx:DocumentType>
      <fx:DocumentFileName>{FILE_NAME}</fx:DocumentFileName>
      <fx:Version>1.0</fx:Version>
      <fx:ConformanceLevel>{level}</fx:ConformanceLevel>
    </rdf:Description>
    <rdf:Description rdf:about="" xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#" xmlns:pdfaProperty="http://www.aiim.org/pdfa/ns/property#">
      <pdfaExtension:schemas>
        <rdf:Bag>
          <rdf:li rdf:parseType="Resource">
            <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>
            <pdfaSchema:namespaceURI>{FX_NAMESPACE}</pdfaSchema:namespaceURI>
            <pdfaSchema:prefix>fx</pdfaSchema:prefix>
            <pdfaSchema:property>
              <rdf:Seq>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>DocumentFileName</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>name of the embedded XML invoice file</pdfaProperty:description>
                </rdf:li>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>DocumentType</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>INVOICE</pdfaProperty:description>
                </rdf:li>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>Version</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>The actual version of the Factur-X XML schema</pdfaProperty:description>
                </rdf:li>
                <rdf:li rdf:parseType="Resource">
                  <pdfaProperty:name>ConformanceLevel</pdfaProperty:name>
                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>
                  <pdfaProperty:category>external</pdfaProperty:category>
                  <pdfaProperty:description>The conformance level of the embedded Factur-X data</pdfaProperty:description>
                </rdf:li>
              </rdf:Seq>
            </pdfaSchema:property>
          </rdf:li>
        </rdf:Bag>
      </pdfaExtension:schemas>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#,
        bom = '\u{FEFF}',
    )
}

fn page_content(lines: &[String], page: usize, pages: usize) -> Vec<u8> {
    let top = f64::from(PAGE_H - MARGIN) - FONT_SIZE;
    let mut c = format!("BT\n/F1 {FONT_SIZE} Tf\n{LEADING} TL\n{MARGIN} {top} Td\n").into_bytes();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            c.extend_from_slice(b"T* ");
        }
        c.extend_from_slice(&literal(&encode(l)));
        c.extend_from_slice(b" Tj\n");
    }
    c.extend_from_slice(b"ET\n");
    let footer = right(&format!("Page {page} of {pages}"), WIDTH);
    c.extend_from_slice(format!("BT\n/F1 {FONT_SIZE} Tf\n{MARGIN} {} Td\n", MARGIN - 14).as_bytes());
    c.extend_from_slice(&literal(&encode(&footer)));
    c.extend_from_slice(b" Tj\nET\n");
    c
}

/// A PDF/A-3b file showing `inv` as text, with `cii_xml` embedded as
/// [`FILE_NAME`]. Write the XML with `crate::cii::write(inv,
/// Specification::En16931)` and check `crate::rules::validate` first.
pub fn pdf(inv: &Invoice, cii_xml: &str, opts: &PdfOptions) -> Vec<u8> {
    const CATALOG: usize = 1;
    const PAGES: usize = 2;
    const FONT: usize = 3;
    const DESCRIPTOR: usize = 4;
    const FONT_FILE: usize = 5;
    const METADATA: usize = 6;
    const ICC: usize = 7;
    const EMBEDDED: usize = 8;
    const FILESPEC: usize = 9;
    const FIRST_PAGE: usize = 10;

    let text = render_text(inv);
    let chunks: Vec<&[String]> = text.chunks(LINES_PER_PAGE).collect();
    let pages = chunks.len().max(1);
    let page_id = |k: usize| FIRST_PAGE + 2 * k;

    let mut w = Writer { buf: b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec(), offsets: Vec::new() };
    let kids: Vec<String> = (0..pages).map(|k| format!("{} 0 R", page_id(k))).collect();
    let fname = literal(FILE_NAME.as_bytes());
    let mut catalog = format!(
        "<< /Type /Catalog /Pages {PAGES} 0 R /Metadata {METADATA} 0 R /OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB IEC61966-2.1) /Info (sRGB IEC61966-2.1) /DestOutputProfile {ICC} 0 R >>] /AF [{FILESPEC} 0 R] /Names << /EmbeddedFiles << /Names ["
    )
    .into_bytes();
    catalog.extend_from_slice(&fname);
    catalog.extend_from_slice(format!(" {FILESPEC} 0 R] >> >> /PageMode /UseAttachments /Lang (en) >>").as_bytes());
    w.object(CATALOG, &catalog);
    w.object(PAGES, format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.join(" ")).as_bytes());

    let f = font();
    let widths: Vec<String> = f.widths.iter().map(i32::to_string).collect();
    w.object(
        FONT,
        format!(
            "<< /Type /Font /Subtype /TrueType /BaseFont /CourierPrime /FirstChar 32 /LastChar 255 /Widths [{}] /Encoding /WinAnsiEncoding /FontDescriptor {DESCRIPTOR} 0 R >>",
            widths.join(" ")
        )
        .as_bytes(),
    );
    let [x0, y0, x1, y1] = f.bbox;
    w.object(
        DESCRIPTOR,
        format!(
            "<< /Type /FontDescriptor /FontName /CourierPrime /Flags 33 /FontBBox [{x0} {y0} {x1} {y1}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {FONT_FILE} 0 R >>",
            f.italic_angle, f.ascent, f.descent, f.cap_height
        )
        .as_bytes(),
    );
    debug_assert!(f.units_per_em > 0);
    w.stream(FONT_FILE, &format!("/Length1 {} /Filter /FlateDecode", FONT_DATA.len()), &deflate(FONT_DATA));
    w.stream(METADATA, "/Type /Metadata /Subtype /XML", xmp(inv, opts).as_bytes());
    w.stream(ICC, "/N 3 /Filter /FlateDecode", &deflate(ICC_PROFILE));
    let xml = cii_xml.as_bytes();
    let mod_date = opts.created.format("D:%Y%m%d%H%M%S+00'00'").to_string();
    w.stream(
        EMBEDDED,
        &format!("/Type /EmbeddedFile /Subtype /text#2Fxml /Params << /ModDate ({mod_date}) /Size {} >> /Filter /FlateDecode", xml.len()),
        &deflate(xml),
    );
    let mut spec = b"<< /Type /Filespec /F ".to_vec();
    spec.extend_from_slice(&fname);
    spec.extend_from_slice(b" /UF ");
    spec.extend_from_slice(&fname);
    spec.extend_from_slice(
        format!(
            " /EF << /F {EMBEDDED} 0 R /UF {EMBEDDED} 0 R >> /AFRelationship /{} /Desc (Factur-X invoice) >>",
            opts.profile.af_relationship()
        )
        .as_bytes(),
    );
    w.object(FILESPEC, &spec);
    let empty: &[String] = &[];
    for k in 0..pages {
        let id = page_id(k);
        w.object(
            id,
            format!(
                "<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 {FONT} 0 R >> >> /Contents {} 0 R >>",
                id + 1
            )
            .as_bytes(),
        );
        let lines = chunks.get(k).copied().unwrap_or(empty);
        w.stream(id + 1, "/Filter /FlateDecode", &deflate(&page_content(lines, k + 1, pages)));
    }

    let xref = w.buf.len();
    let n = w.offsets.len() + 1;
    let mut tail = format!("xref\n0 {n}\n0000000000 65535 f \n");
    for o in &w.offsets {
        let _ = writeln!(tail, "{o:010} 00000 n ");
    }
    let id = file_id(&w.buf);
    let _ = write!(tail, "trailer\n<< /Size {n} /Root {CATALOG} 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n");
    w.buf.extend_from_slice(tail.as_bytes());
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::tests::{line, sample};
    use crate::{Specification, VatCategory, cii};
    use rust_decimal_macros::dec;
    use std::io::Read;

    fn opts() -> PdfOptions {
        PdfOptions { created: chrono::NaiveDate::from_ymd_opt(2026, 3, 2).unwrap().and_hms_opt(9, 30, 0).unwrap(), profile: Profile::En16931 }
    }

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    /// The (decoded) data of object `id`'s stream.
    fn stream_of(pdf: &[u8], id: usize) -> Vec<u8> {
        let start = find(pdf, format!("\n{id} 0 obj\n").as_bytes()).unwrap();
        let obj = &pdf[start..];
        let dict_end = find(obj, b"stream\n").unwrap();
        let dict = std::str::from_utf8(&obj[..dict_end]).unwrap();
        let length: usize = dict.split("/Length ").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        let data = &obj[dict_end + 7..dict_end + 7 + length];
        assert_eq!(&obj[dict_end + 7 + length..dict_end + 7 + length + 10], b"\nendstream");
        if dict.contains("/FlateDecode") {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(data).read_to_end(&mut out).unwrap();
            out
        } else {
            data.to_vec()
        }
    }

    fn check_structure(pdf: &[u8]) -> usize {
        assert!(pdf.starts_with(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let sx = pdf.windows(10).rposition(|w| w == b"startxref\n").unwrap();
        let tail = std::str::from_utf8(&pdf[sx..]).unwrap();
        let xref: usize = tail[10..].lines().next().unwrap().parse().unwrap();
        let table = std::str::from_utf8(&pdf[xref..sx]).unwrap();
        assert!(table.starts_with("xref\n0 "));
        let n: usize = table.lines().nth(1).unwrap().split(' ').nth(1).unwrap().parse().unwrap();
        let entries: Vec<&str> = table.split('\n').skip(2).take(n).collect();
        assert_eq!(entries[0], "0000000000 65535 f ");
        for (i, e) in entries.iter().enumerate().skip(1) {
            assert_eq!(e.len() + 1, 20, "20-byte entries");
            let off: usize = e[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{i} 0 obj\n").as_bytes()), "object {i}");
        }
        assert!(table.contains(&format!("trailer\n<< /Size {n} /Root 1 0 R /ID [<")));
        n
    }

    #[test]
    fn text_of_the_sample_invoice() {
        let t = render_text(&sample());
        assert!(t.iter().all(|l| l.chars().count() <= WIDTH), "{t:#?}");
        assert_eq!(t[0], "INVOICE");
        for want in [
            "Number: RE-2026-001   Issue date: 2026-03-02   Due date: 2026-04-01",
            "Delivery date: 2026-02-27   Currency: EUR",
            "SELLER                                           BUYER",
            "Muster GmbH                                      Kunde AG",
            "Hauptstraße 1                                    Marktplatz 5",
            "10115 Berlin                                     80331 München",
            "VAT ID: DE136695976                              VAT ID: DE811569869",
            "Tax number: 2893081508152                        E-address: ap@kunde.de (EM)",
            "Buyer reference: 04011000-12345-34",
            "#    Item                             Quantity Unit          Price     Net amount VAT",
            "1    Artikel 1                               2 H87              50         100.00 S 19%",
            "2    Artikel 2                               3 H87            10.1          30.30 S 7%",
            "S Standard rated                      19%               100.00                19.00",
            "                                                        Total with VAT           151.42 EUR",
            "                                                            Amount due           151.42 EUR",
            "Payment means code: 58",
            "Account: DE02120300000000202051",
            "Terms: Zahlbar innerhalb von 30 Tagen ohne Abzug.",
            "Vielen Dank für Ihren Auftrag & Ihr Vertrauen.",
        ] {
            assert!(t.iter().any(|l| l == want), "missing {want:?} in\n{}", t.join("\n"));
        }
        assert!(t.last().unwrap().contains("factur-x.xml"));
    }

    #[test]
    fn credit_note_title_and_exemption_reason() {
        let mut inv = sample();
        inv.type_code = TypeCode::CreditNote;
        inv.lines.truncate(1);
        inv.lines[0].category = VatCategory::AE;
        inv.lines[0].rate = Some(dec!(0));
        inv.finalise();
        let t = render_text(&inv);
        assert_eq!(t[0], "CREDIT NOTE");
        assert!(t.iter().any(|l| l == "     VATEX-EU-AE: Reverse charge"), "{t:#?}");
    }

    #[test]
    fn pdf_structure_xmp_and_embedded_xml() {
        let inv = sample();
        let xml = cii::write(&inv, Specification::En16931);
        let pdf = pdf(&inv, &xml, &opts());
        let n = check_structure(&pdf);
        assert_eq!(n, 12, "9 fixed objects and one page with its content, plus the free entry");
        let s = String::from_utf8_lossy(&pdf);
        assert!(!s[s.rfind("trailer").unwrap()..].contains("/Info"), "no document information dictionary");
        assert!(!s.contains("/Encrypt"));
        // The XML, byte for byte.
        assert_eq!(stream_of(&pdf, 8), xml.as_bytes());
        assert!(s.contains(&format!("/Type /EmbeddedFile /Subtype /text#2Fxml /Params << /ModDate (D:20260302093000+00'00') /Size {} >>", xml.len())));
        assert!(s.contains("/Type /Filespec /F (factur-x.xml) /UF (factur-x.xml) /EF << /F 8 0 R /UF 8 0 R >> /AFRelationship /Alternative"));
        assert!(s.contains("/AF [9 0 R] /Names << /EmbeddedFiles << /Names [(factur-x.xml) 9 0 R] >> >>"));
        assert!(s.contains("/OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 "));
        assert_eq!(stream_of(&pdf, 7), ICC_PROFILE);
        assert_eq!(stream_of(&pdf, 5), FONT_DATA);
        // XMP: uncompressed, PDF/A-3B, the Factur-X properties and their schema.
        let x = String::from_utf8(stream_of(&pdf, 6)).unwrap();
        assert!(s.contains("6 0 obj\n<< /Length ") && !s[s.find("6 0 obj").unwrap()..].split("stream").next().unwrap().contains("Filter"));
        assert!(x.starts_with("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>"));
        assert!(x.ends_with("<?xpacket end=\"w\"?>"));
        for want in [
            "<pdfaid:part>3</pdfaid:part>",
            "<pdfaid:conformance>B</pdfaid:conformance>",
            "<fx:DocumentType>INVOICE</fx:DocumentType>",
            "<fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>",
            "<fx:Version>1.0</fx:Version>",
            "<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>",
            "xmlns:fx=\"urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#\"",
            "<pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>",
            "<pdfaSchema:prefix>fx</pdfaSchema:prefix>",
            "<pdfaProperty:name>ConformanceLevel</pdfaProperty:name>",
            "<xmp:CreateDate>2026-03-02T09:30:00+00:00</xmp:CreateDate>",
            "<rdf:li xml:lang=\"x-default\">INVOICE RE-2026-001</rdf:li>",
            "<rdf:li>Muster GmbH</rdf:li>",
        ] {
            assert!(x.contains(want), "missing {want} in the XMP");
        }
        let mut r = quick_xml::Reader::from_str(&x);
        loop {
            if r.read_event().expect("well-formed XMP") == quick_xml::events::Event::Eof {
                break;
            }
        }
        // Font: 224 widths from the font program, 600 for 'A'.
        let widths = s.split("/Widths [").nth(1).unwrap().split(']').next().unwrap();
        let widths: Vec<i32> = widths.split(' ').map(|w| w.parse().unwrap()).collect();
        assert_eq!(widths.len(), 224);
        assert_eq!(widths[usize::from(b'A' - 32)], 600);
        assert_eq!(widths[0x80 - 32], 600, "€");
        assert_eq!(widths[0x81 - 32], 0, "undefined in WinAnsi");
        assert!(s.contains("/Subtype /TrueType /BaseFont /CourierPrime /FirstChar 32 /LastChar 255"));
        assert!(s.contains(&format!("/Length1 {} ", FONT_DATA.len())));
        assert!(s.contains("/Flags 33 "));
        // The page's text.
        let page = stream_of(&pdf, 11);
        assert!(find(&page, b"(INVOICE) Tj").is_some());
        assert!(find(&page, b"(Hauptstra\xDFe 1").is_some(), "WinAnsi bytes");
        assert!(find(&page, b"Page 1 of 1) Tj").is_some());
        // Deterministic.
        assert_eq!(pdf, super::pdf(&inv, &xml, &opts()));
    }

    #[test]
    fn long_invoices_paginate() {
        let mut inv = sample();
        inv.lines = (1..=150).map(|i| line(&i.to_string(), dec!(1), dec!(2.5), VatCategory::S, Some(dec!(19)))).collect();
        inv.finalise();
        let text = render_text(&inv);
        let pages = text.len().div_ceil(LINES_PER_PAGE);
        assert!(pages >= 3, "{pages}");
        let pdf = pdf(&inv, &cii::write(&inv, Specification::En16931), &opts());
        let n = check_structure(&pdf);
        assert_eq!(n, 10 + 2 * pages);
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains(&format!("/Count {pages} >>")));
        assert_eq!(s.matches("/Type /Page /Parent").count(), pages);
        let last = stream_of(&pdf, 10 + 2 * (pages - 1) + 1);
        assert!(find(&last, format!("Page {pages} of {pages}) Tj").as_bytes()).is_some());
    }

    #[test]
    fn characters_outside_winansi_become_question_marks() {
        assert_eq!(encode("A₹ب€é\u{7f}\tz"), b"A??\x80\xE9 z");
        assert_eq!(encode("(x) \\"), b"(x) \\");
        assert_eq!(literal(b"(x) \\"), b"(\\(x\\) \\\\)");
        let f = font();
        assert_eq!(f.units_per_em, 2048);
        assert!(f.has_glyph.iter().enumerate().all(|(i, &g)| g == cp1252(i as u8 + 32).is_some()), "every WinAnsi character has a glyph");
        assert!(f.ascent > 0 && f.descent < 0 && f.cap_height > 0);
    }

    /// `FACTURX_SAMPLE=/path/sample.pdf cargo test -p khata-en16931 facturx`
    /// writes the sample invoice, to open it or run veraPDF on it.
    #[test]
    fn sample_file_on_request() {
        if let Some(path) = std::env::var_os("FACTURX_SAMPLE") {
            let mut inv = sample();
            inv.lines[0].item_name = "Laptop ₹ عربي – Größe".into();
            inv.finalise();
            std::fs::write(path, pdf(&inv, &cii::write(&inv, Specification::En16931), &opts())).unwrap();
        }
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("aa bb cc", 5), ["aa bb", "cc"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(right("12", 4), "  12");
        assert_eq!(left("abcdef", 3), "abc");
    }
}

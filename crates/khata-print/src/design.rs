//! Invoice design: what a company chooses about how its invoices look
//! (template, colour, font, paper, logo, bank details, terms…), and what
//! the law of its country does not let it choose ([`Required`]).
//!
//! The design only ever *adds* to an invoice or changes its look. Every
//! renderer (HTML in `lib.rs`, PDF in `pdf/`) prints the country-mandatory
//! content whatever the design: a toggle that would hide a mandatory field
//! is overridden by [`Required`], and statutory notes
//! ([`crate::InvoiceDoc::legal_notes`]) cannot be switched off.
//! See `docs/invoice-design.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The built-in designs. Each is a style (layout of the header, colours,
/// spacing), never a different set of fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Template {
    /// Centred header, boxed party and number details: the print KhataOS
    /// always had.
    #[default]
    Classic,
    /// Logo and company at the start, the title large in the accent colour,
    /// a filled table header.
    Modern,
    /// Smaller type and tighter rows, for A5 and long invoices.
    Compact,
}

impl Template {
    pub const ALL: [Template; 3] = [Template::Classic, Template::Modern, Template::Compact];

    pub fn name(self) -> &'static str {
        match self {
            Template::Classic => "Classic",
            Template::Modern => "Modern",
            Template::Compact => "Compact",
        }
    }
}

/// Paper the invoice is laid out for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    A4,
    /// US Letter, 8.5 × 11 in.
    Letter,
    A5,
    /// An 80 mm receipt roll (thermal printers): only for simplified / B2C
    /// invoices, see [`InvoiceDesign::receipt_for_b2c`].
    Thermal80,
}

impl Paper {
    pub const ALL: [Paper; 4] = [Paper::A4, Paper::Letter, Paper::A5, Paper::Thermal80];

    /// Width and height in millimetres. A receipt's height is a default
    /// page; the PDF writer makes one page as long as the receipt.
    pub fn size_mm(self) -> (f32, f32) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::Letter => (215.9, 279.4),
            Paper::A5 => (148.0, 210.0),
            Paper::Thermal80 => (80.0, 297.0),
        }
    }

    /// The CSS `@page` size.
    pub fn css(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::Letter => "letter",
            Paper::A5 => "A5",
            Paper::Thermal80 => "80mm 297mm",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::Letter => "Letter",
            Paper::A5 => "A5",
            Paper::Thermal80 => "80 mm receipt",
        }
    }
}

/// Type on printed invoices: safe system fonts only (nothing is
/// downloaded). Arabic always falls back to the Arabic fonts the prints
/// already use. The PDF export embeds one font (DejaVu Sans) whatever the
/// choice, so a PDF reads the same on every device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FontChoice {
    /// The system UI font (the existing prints' stack).
    #[default]
    System,
    /// Arial / Helvetica / Liberation Sans.
    Arial,
    /// Georgia / Times New Roman / Noto Serif.
    Serif,
    /// Courier New / Courier Prime.
    Mono,
}

impl FontChoice {
    pub const ALL: [FontChoice; 4] = [FontChoice::System, FontChoice::Arial, FontChoice::Serif, FontChoice::Mono];

    /// The CSS `font-family` list, Arabic fallbacks last.
    pub fn css(self) -> &'static str {
        match self {
            FontChoice::System => "system-ui, -apple-system, 'Segoe UI', 'Noto Sans', 'Noto Naskh Arabic', 'Geeza Pro', sans-serif",
            FontChoice::Arial => "Arial, Helvetica, 'Liberation Sans', 'Noto Sans', 'Noto Naskh Arabic', 'Geeza Pro', sans-serif",
            FontChoice::Serif => "Georgia, 'Times New Roman', 'Noto Serif', 'Liberation Serif', 'Noto Naskh Arabic', 'Geeza Pro', serif",
            FontChoice::Mono => "'Courier New', 'Courier Prime', 'Liberation Mono', 'Noto Naskh Arabic', 'Geeza Pro', monospace",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FontChoice::System => "System",
            FontChoice::Arial => "Arial / Helvetica",
            FontChoice::Serif => "Serif (Georgia / Times)",
            FontChoice::Mono => "Monospace (Courier)",
        }
    }
}

/// The company's bank account, printed so customers can pay. Every field is
/// optional; empty ones are not printed. The labels follow the country
/// ([`BankLabels::for_country`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BankDetails {
    pub account_name: String,
    pub bank_name: String,
    pub branch: String,
    pub account_number: String,
    pub iban: String,
    /// The country's bank or branch code: IFSC (India), sort code (UK),
    /// routing number (US), bank/branch code (Singapore).
    pub local_code: String,
    /// SWIFT / BIC.
    pub swift: String,
}

impl BankDetails {
    pub fn is_empty(&self) -> bool {
        [&self.account_name, &self.bank_name, &self.branch, &self.account_number, &self.iban, &self.local_code, &self.swift]
            .iter()
            .all(|s| s.trim().is_empty())
    }
}

/// How the bank fields are called in a country.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BankLabels {
    pub account_number: &'static str,
    /// `None`: the country has no separate bank/branch code (IBAN countries).
    pub local_code: Option<&'static str>,
    /// Whether an IBAN is usual there.
    pub iban: bool,
}

impl BankLabels {
    pub fn for_country(code: &str) -> Self {
        let l = |account_number, local_code, iban| BankLabels { account_number, local_code, iban };
        match code {
            "IN" => l("Account number", Some("IFSC"), false),
            "GB" => l("Account number", Some("Sort code"), true),
            "US" => l("Account number", Some("Routing number (ABA)"), false),
            "SG" => l("Account number", Some("Bank / branch code"), false),
            "MX" => l("CLABE", None, false),
            "AE" | "SA" | "DE" | "FR" | "NL" | "ES" => l("Account number", None, true),
            _ => l("Account number", Some("Bank code"), true),
        }
    }
}

/// A company's invoice design. Stored as JSON in the company file's
/// settings table (key `invoice.design`, see `docs/invoice-design.md`);
/// images are kept apart ([`Image`]). Unknown or missing keys take their
/// defaults, so older files read fine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InvoiceDesign {
    pub template: Template,
    /// `#rrggbb`, see [`InvoiceDesign::accent`].
    pub accent: String,
    pub font: FontChoice,
    /// Paper for invoices and vouchers (A4, Letter or A5).
    pub paper: Paper,
    /// Print simplified / B2C invoices (a sale to a buyer with no tax ID) on
    /// an 80 mm receipt instead of `paper`.
    pub receipt_for_b2c: bool,
    pub show_hsn: bool,
    pub show_qty_rate: bool,
    pub show_amount_in_words: bool,
    pub show_signatory: bool,
    pub bank: BankDetails,
    /// Printed on invoices and notes (not on payments or journals).
    pub terms: String,
    /// A line at the foot of every invoice ("Thank you for your business").
    pub footer: String,
    /// A declaration printed above the signature ("We declare that this
    /// invoice shows the actual price…").
    pub declaration: String,
    /// India: the ARN of the Letter of Undertaking for exports and SEZ
    /// supplies without payment of IGST (Rule 96A), printed with the export
    /// endorsement. A voucher can carry its own.
    pub lut_arn: String,
    /// Title per voucher type ("Sales" → "Invoice"), where the law allows a
    /// choice (see the desktop's `invoice_design::title_rule`).
    pub titles: BTreeMap<String, String>,
}

impl Default for InvoiceDesign {
    fn default() -> Self {
        Self {
            template: Template::Classic,
            accent: DEFAULT_ACCENT.to_owned(),
            font: FontChoice::System,
            paper: Paper::A4,
            receipt_for_b2c: false,
            show_hsn: true,
            show_qty_rate: true,
            show_amount_in_words: true,
            show_signatory: true,
            bank: BankDetails::default(),
            terms: String::new(),
            footer: String::new(),
            declaration: String::new(),
            lut_arn: String::new(),
            titles: BTreeMap::new(),
        }
    }
}

/// The accent colour when none (or a bad one) is set: near-black, so the
/// Classic design prints as it always did.
pub const DEFAULT_ACCENT: &str = "#111111";

/// Longest terms, footer or declaration text kept (characters).
pub const MAX_TEXT: usize = 2000;

impl InvoiceDesign {
    /// The accent as `#rrggbb`: the stored value when it is one, else the default.
    pub fn accent(&self) -> String {
        parse_hex_colour(&self.accent).map_or_else(|| DEFAULT_ACCENT.to_owned(), |(r, g, b)| format!("#{r:02x}{g:02x}{b:02x}"))
    }

    /// The accent as RGB fractions (PDF).
    pub fn accent_rgb(&self) -> (f32, f32, f32) {
        let (r, g, b) = parse_hex_colour(&self.accent).or_else(|| parse_hex_colour(DEFAULT_ACCENT)).unwrap_or((17, 17, 17));
        (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// Check and tidy a design typed by the user: a valid colour, texts
    /// trimmed and limited, no control characters, no empty titles.
    pub fn validated(mut self) -> Result<Self, String> {
        if parse_hex_colour(&self.accent).is_none() {
            return Err(format!("{:?} is not a colour: use #rrggbb, e.g. #1f4e79", self.accent));
        }
        self.accent = self.accent();
        if self.paper == Paper::Thermal80 {
            return Err("80 mm receipts are for B2C invoices: pick A4, Letter or A5 and switch on receipts for B2C".into());
        }
        for (what, s) in [("terms", &mut self.terms), ("footer", &mut self.footer), ("declaration", &mut self.declaration)] {
            *s = clean_text(s, true);
            if s.chars().count() > MAX_TEXT {
                return Err(format!("the {what} are too long: keep them under {MAX_TEXT} characters"));
            }
        }
        let b = &mut self.bank;
        for s in [&mut b.account_name, &mut b.bank_name, &mut b.branch, &mut b.account_number, &mut b.iban, &mut b.local_code, &mut b.swift] {
            *s = clean_text(s, false);
            if s.chars().count() > 120 {
                return Err("bank details are too long".into());
            }
        }
        self.lut_arn = clean_text(&self.lut_arn, false);
        let mut titles = BTreeMap::new();
        for (k, v) in std::mem::take(&mut self.titles) {
            let v = clean_text(&v, false);
            if v.chars().count() > 60 {
                return Err(format!("the title for {k} is too long"));
            }
            if !v.is_empty() {
                titles.insert(k, v);
            }
        }
        self.titles = titles;
        Ok(self)
    }
}

/// `#rgb` or `#rrggbb` (the `#` optional).
pub fn parse_hex_colour(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().trim_start_matches('#');
    let v = |s: &str| u8::from_str_radix(s, 16).ok();
    match h.len() {
        6 if h.is_ascii() => Some((v(&h[0..2])?, v(&h[2..4])?, v(&h[4..6])?)),
        3 if h.is_ascii() => {
            let d = |i: usize| v(&h[i..=i]).map(|x| x * 17);
            Some((d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

/// Trim, drop control characters (keeping new lines when `multiline`).
fn clean_text(s: &str, multiline: bool) -> String {
    let kept: String = s.chars().filter(|&c| !c.is_control() || (multiline && c == '\n')).collect();
    if multiline {
        kept.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_owned()
    } else {
        kept.trim().to_owned()
    }
}

/// What the country's invoice rules make mandatory whatever the design:
/// the toggles it overrides. Statutory text lives on the document itself
/// ([`crate::InvoiceDoc::legal_notes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Required {
    /// The HSN/SAC or tax-class column (India, Rule 46(g)).
    pub tax_class: bool,
    /// Quantity and unit price columns (India Rule 46(i); UAE ER Art 59(1)(h);
    /// KSA IR Art 53).
    pub qty_rate: bool,
    /// "For <company> / Authorised signatory" (India Rule 46(q) on paper).
    pub signatory: bool,
}

/// An image for the invoice: the logo or a signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub kind: ImageKind,
    /// The file as stored (SVGs already sanitised).
    pub data: Vec<u8>,
    /// For an SVG: a PNG rendition (made by the app's webview when the SVG
    /// was added), used by the PDF writer, which draws no SVG.
    pub raster: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageKind {
    Png,
    Jpeg,
    Svg,
}

impl ImageKind {
    pub fn mime(self) -> &'static str {
        match self {
            ImageKind::Png => "image/png",
            ImageKind::Jpeg => "image/jpeg",
            ImageKind::Svg => "image/svg+xml",
        }
    }
}

impl Image {
    /// A `data:` URL for an `<img>` (images in `<img>` never run scripts).
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.kind.mime(), crate::base64(&self.data))
    }

    /// The PNG or JPEG bytes the PDF writer can draw, if any.
    pub fn bitmap(&self) -> Option<(ImageKind, &[u8])> {
        match self.kind {
            ImageKind::Png | ImageKind::Jpeg => Some((self.kind, &self.data)),
            ImageKind::Svg => self.raster.as_deref().map(|r| (ImageKind::Png, r)),
        }
    }
}

/// Everything about the look of one print: the design, the images, what the
/// country makes mandatory, the paper this document goes on and the bank
/// field names.
#[derive(Debug, Clone)]
pub struct Look<'a> {
    pub design: &'a InvoiceDesign,
    pub logo: Option<&'a Image>,
    pub signature: Option<&'a Image>,
    pub required: Required,
    /// The paper for this document (the caller resolves `receipt_for_b2c`).
    pub paper: Paper,
    pub bank_labels: BankLabels,
    /// Print bank details, terms and declaration: invoices and notes yes,
    /// payments and journals no.
    pub trade_document: bool,
}

impl Look<'_> {
    /// The look of the prints before invoice designs existed.
    pub fn plain() -> Look<'static> {
        static DEFAULT: std::sync::OnceLock<InvoiceDesign> = std::sync::OnceLock::new();
        Look {
            design: DEFAULT.get_or_init(InvoiceDesign::default),
            logo: None,
            signature: None,
            required: Required::default(),
            paper: Paper::A4,
            bank_labels: BankLabels::for_country(""),
            trade_document: true,
        }
    }

    pub fn show_tax_class(&self) -> bool {
        self.design.show_hsn || self.required.tax_class
    }

    pub fn show_qty_rate(&self) -> bool {
        self.design.show_qty_rate || self.required.qty_rate
    }

    pub fn show_signatory(&self) -> bool {
        self.design.show_signatory || self.required.signatory
    }

    pub fn thermal(&self) -> bool {
        self.paper == Paper::Thermal80
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_and_bad_ones_are_refused() {
        assert_eq!(parse_hex_colour("#1F4E79"), Some((0x1f, 0x4e, 0x79)));
        assert_eq!(parse_hex_colour("abc"), Some((0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_hex_colour("red"), None);
        assert_eq!(parse_hex_colour("#12345"), None);
        assert_eq!(parse_hex_colour("#ééé"), None);
        let d = InvoiceDesign { accent: "javascript:alert(1)".into(), ..Default::default() };
        assert!(d.clone().validated().is_err());
        assert_eq!(d.accent(), DEFAULT_ACCENT, "a bad stored colour prints as the default");
    }

    #[test]
    fn validation_tidies_texts_and_drops_empty_titles() {
        let mut d = InvoiceDesign { accent: "#ABC".into(), terms: "  1. Pay in 30 days.  \r\n\u{7}2. Interest at 18%.\n\n".into(), ..Default::default() };
        d.titles.insert("Sales".into(), "  ".into());
        d.titles.insert("Payment".into(), " Payment Advice ".into());
        let v = d.validated().unwrap();
        assert_eq!(v.accent, "#aabbcc");
        assert_eq!(v.terms, "1. Pay in 30 days.\n2. Interest at 18%.");
        assert_eq!(v.titles.len(), 1);
        assert_eq!(v.titles["Payment"], "Payment Advice");
        let long = InvoiceDesign { footer: "x".repeat(MAX_TEXT + 1), ..Default::default() };
        assert!(long.validated().is_err());
        let receipt = InvoiceDesign { paper: Paper::Thermal80, ..Default::default() };
        assert!(receipt.validated().is_err(), "receipts are only for B2C invoices");
    }

    #[test]
    fn older_or_partial_json_takes_defaults() {
        let d: InvoiceDesign = serde_json::from_str(r#"{"template":"Modern","future_key":1}"#).unwrap();
        assert_eq!(d.template, Template::Modern);
        assert!(d.show_hsn && d.show_signatory);
        assert_eq!(d.paper, Paper::A4);
    }

    #[test]
    fn required_fields_override_the_toggles() {
        let design = InvoiceDesign { show_hsn: false, show_qty_rate: false, show_signatory: false, ..Default::default() };
        let mut look = Look { design: &design, ..Look::plain() };
        assert!(!look.show_tax_class() && !look.show_qty_rate() && !look.show_signatory());
        look.required = Required { tax_class: true, qty_rate: true, signatory: true };
        assert!(look.show_tax_class() && look.show_qty_rate() && look.show_signatory());
    }
}

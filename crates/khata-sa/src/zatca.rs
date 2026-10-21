//! ZATCA e-invoices (Fatoora): UBL 2.1 XML, the invoice hash, the ICV/PIH
//! chain and the QR payload. Pure: no clock, no randomness, no storage, no
//! network. The caller supplies the UUID, the issue time and the chain
//! state, and persists what comes back.
//!
//! Sources: `docs/research/mena-compliance.md` §6 (ZATCA XML Implementation
//! Standard v1.2 and Data Dictionary of 19 May 2023, Security Features
//! Standard v1.2, Detailed Technical Guideline v2) [PRIMARY unless marked].
//!
//! What [`generate`] builds:
//! - an `Invoice` document (`urn:oasis:names:specification:ubl:schema:xsd:Invoice-2`)
//!   in UBL element order, with `ProfileID` "reporting:1.0", the invoice
//!   number, UUID (KSA-1), issue date and time (KSA-25), the type code
//!   (388 invoice, 381 credit note, 383 debit note) with the NNPNESB
//!   subtype in `@name` (KSA-2: "0100000" standard, "0200000" simplified,
//!   E flag for exports), SAR as document and tax currency (BT-5, BT-6),
//!   the billing reference of a note (the original invoice number), the
//!   ICV (KSA-16) and PIH (KSA-13) references, the QR reference, the seller
//!   (CRN, Saudi National Address, VAT number) and buyer, the supply date
//!   (KSA-5), the payment means, document-level allowances, two `TaxTotal`s
//!   (one with the VAT breakdown by category, one with the SAR total only),
//!   the monetary totals and the lines with their tax category (S, Z, E, O)
//!   and line VAT;
//! - the invoice hash: SHA-256 over the XML with `ext:UBLExtensions`, the
//!   QR reference and `cac:Signature` removed, canonicalised, Base64
//!   ([`invoice_hash`]);
//! - the QR payload ([`crate::qr`]): tags 1 to 5 always; tags 6 to 9 when
//!   the document is stamped (tag 9 only on simplified documents);
//! - with a [`Stamp`] (the device key and its CSID certificate), the XAdES
//!   enveloped signature: `ext:UBLExtensions` with `ds:Signature`
//!   ([`crate::xades`]) straight after the `Invoice` start tag, and the
//!   `cac:Signature` block straight after the QR reference, both written
//!   without surrounding whitespace as in ZATCA's SDK samples. Cutting them
//!   out for the hash leaves exactly the unstamped text, so **a document
//!   has the same hash stamped or not**: a document generated before the
//!   device was onboarded can be stamped later without breaking the chain.
//!
//! Onboarding (CSR [`crate::csr`], OTP, CSIDs) and the clearance and
//! reporting APIs are network work, done by the desktop app. Prepayments
//! (386 and KSA-26 lines) are not modelled.
//!
//! [UNCERTAIN] Exact canonicalisation: ZATCA asks for C14N 1.1. KhataOS
//! writes its XML already in canonical form (no XML declaration in the hash
//! input, attributes in canonical order, start/end tag pairs, C14N
//! escaping, LF line ends), so the canonical form of the transformed
//! document is the text left after cutting the three elements out, with the
//! surrounding whitespace kept (as an XSLT identity transform would). This
//! is not a general C14N implementation: [`hash_input`] refuses input it
//! cannot vouch for. Golden-file tests against the ZATCA SDK's hash are
//! needed before production.
//!
//! [UNCERTAIN] Credit notes use the `Invoice` root element with type code
//! 381 (research §6.2: "The UBL message type is 'Invoice' for invoices,
//! debit notes and prepayments; credit notes use the CreditNote type code");
//! confirm against the SDK samples.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{NaiveDate, NaiveTime};
use rust_decimal::Decimal;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::qr::{self, QrError, QrFields};
pub use crate::xades::Stamp;
use crate::vat::{Treatment, round_halalas};
use crate::vat_number;

/// `cbc:ProfileID` (research §6.2 data dictionary).
pub const PROFILE_ID: &str = "reporting:1.0";

/// The PIH of a device's first document: Base64 of the SHA-256 of "0" as
/// ZATCA publishes it (BR-KSA-26, Security Standard §3; research §6.2). It
/// is the Base64 of the digest's *hex text*, unlike later hashes, which are
/// the Base64 of the digest bytes.
pub const FIRST_PIH: &str = "NWZlY2ViNjZmZmM4NmYzOGQ5NTI3ODZjNmQ2OTZjNzljMmRiYzIzOWRkNGU5MWI0NjcyOWQ3M2EyN2ZiNTdlOQ==";

const NS_INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
const NS_CAC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2";
const NS_CBC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2";
const NS_EXT: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2";

/// Who the document is for (the NN of the subtype).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InvoiceKind {
    /// Tax invoice (B2B), "01": cleared by ZATCA before it reaches the buyer.
    Standard,
    /// Simplified tax invoice (B2C), "02": reported within 24 hours.
    Simplified,
}

/// What the document is (BT-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DocumentType {
    Invoice,
    CreditNote,
    DebitNote,
}

impl DocumentType {
    /// UNCL1001 code.
    pub fn code(self) -> &'static str {
        match self {
            DocumentType::Invoice => "388",
            DocumentType::CreditNote => "381",
            DocumentType::DebitNote => "383",
        }
    }
}

/// A Saudi National Address (or a foreign buyer's address). Empty fields
/// are left out of the XML.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Address {
    pub street: String,
    pub additional_street: String,
    /// 4 digits in the National Address (BR-KSA-37).
    pub building_number: String,
    /// The National Address "additional number".
    pub additional_number: String,
    /// District (`CitySubdivisionName`).
    pub district: String,
    pub city: String,
    pub postal_code: String,
    /// Region (`CountrySubentity`).
    pub region: String,
    /// ISO 3166-1 alpha-2 ("SA").
    pub country: String,
}

/// The seller or the buyer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Party {
    /// Registration name.
    pub name: String,
    pub vat_number: Option<String>,
    /// Another identifier with its scheme: the seller's CRN (or MOM, MLS,
    /// 700, SAG, OTH); a buyer's TIN, CRN, NAT, IQA, GCC… (BR-KSA-08, -14).
    pub other_id: Option<(String, String)>,
    pub address: Address,
}

/// The VAT category of a line or allowance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Category {
    pub treatment: Treatment,
    pub percent: Decimal,
    /// VATEX code and reason text, for Z, E and O (BR-KSA-69, -83).
    pub exemption: Option<(String, String)>,
}

impl Category {
    pub fn standard(percent: Decimal) -> Self {
        Self { treatment: Treatment::Standard, percent, exemption: None }
    }

    fn key(&self) -> (Treatment, Decimal, Option<String>) {
        (self.treatment, self.percent.normalize(), self.exemption.as_ref().map(|e| e.0.clone()))
    }
}

/// One invoice line. Amounts are positive, in SAR, VAT excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    pub name: String,
    pub quantity: Decimal,
    /// UN/ECE Rec 20 unit code; "PCE" (piece) when unknown.
    pub unit_code: String,
    pub unit_price: Decimal,
    /// The line's net amount (BT-131).
    pub net_amount: Decimal,
    pub category: Category,
}

/// A document-level allowance (discount), VAT excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Allowance {
    pub reason: String,
    pub amount: Decimal,
    pub category: Category,
}

/// Everything an e-invoice says, apart from its place in the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EInvoice {
    pub kind: InvoiceKind,
    pub doc_type: DocumentType,
    /// The invoice number (BT-1).
    pub number: String,
    /// KSA-1: letters, digits and dashes; KhataOS uses a v4 UUID.
    pub uuid: String,
    pub issue_date: NaiveDate,
    /// KSA-25, in Riyadh time (AST, UTC+3), written hh:mm:ss.
    pub issue_time: NaiveTime,
    /// KSA-5, mandatory on standard invoices (BR-KSA-15).
    pub supply_date: Option<NaiveDate>,
    /// An export (E flag of the subtype; standard invoices only).
    pub export: bool,
    /// The original invoice number, for credit and debit notes.
    pub billing_reference: Option<String>,
    /// Why a credit or debit note was issued (`InstructionNote`).
    pub reason: Option<String>,
    /// UNCL4461 payment means code ("10" cash, "30" credit transfer…).
    pub payment_means: String,
    pub seller: Party,
    /// Mandatory on standard invoices.
    pub buyer: Option<Party>,
    pub lines: Vec<Line>,
    pub allowances: Vec<Allowance>,
}

impl EInvoice {
    /// The 7-character subtype NNPNESB (KSA-2, research §6.2). Third-party,
    /// nominal, summary and self-billed flags are not modelled (0).
    pub fn subtype(&self) -> String {
        let nn = if self.kind == InvoiceKind::Standard { "01" } else { "02" };
        let export = u8::from(self.export && self.kind == InvoiceKind::Standard);
        format!("{nn}00{export}00")
    }
}

/// VAT for one category of the breakdown (BG-23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Subtotal {
    pub category: Category,
    pub taxable: Decimal,
    pub vat: Decimal,
}

/// The document totals (BG-22), in SAR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub line_extension: Decimal,
    pub allowance_total: Decimal,
    pub tax_exclusive: Decimal,
    pub vat: Decimal,
    pub tax_inclusive: Decimal,
    pub payable: Decimal,
    pub subtotals: Vec<Subtotal>,
}

fn vat_on(value: Decimal, percent: Decimal) -> Decimal {
    round_halalas(value * percent / Decimal::ONE_HUNDRED)
}

/// Line VAT, rounded half up.
pub fn line_vat(l: &Line) -> Decimal {
    vat_on(l.net_amount, l.category.percent)
}

/// The totals: the VAT of each category is worked out on the category's
/// taxable amount and rounded once (XML standard §10: round final results,
/// not intermediates).
pub fn totals(inv: &EInvoice) -> Totals {
    let mut subtotals: Vec<Subtotal> = Vec::new();
    let mut add = |c: &Category, amount: Decimal| match subtotals.iter_mut().find(|s| s.category.key() == c.key()) {
        Some(s) => s.taxable += amount,
        None => subtotals.push(Subtotal { category: c.clone(), taxable: amount, vat: Decimal::ZERO }),
    };
    for l in &inv.lines {
        add(&l.category, l.net_amount);
    }
    for a in &inv.allowances {
        add(&a.category, -a.amount);
    }
    for s in &mut subtotals {
        s.taxable = round_halalas(s.taxable);
        s.vat = vat_on(s.taxable, s.category.percent);
    }
    let line_extension = round_halalas(inv.lines.iter().map(|l| l.net_amount).sum());
    let allowance_total = round_halalas(inv.allowances.iter().map(|a| a.amount).sum());
    let tax_exclusive = line_extension - allowance_total;
    let vat: Decimal = subtotals.iter().map(|s| s.vat).sum();
    let tax_inclusive = tax_exclusive + vat;
    Totals { line_extension, allowance_total, tax_exclusive, vat, tax_inclusive, payable: tax_inclusive, subtotals }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZatcaError {
    /// The document breaks a rule the XML cannot be built without.
    Invalid(String),
    /// [`hash_input`] got XML it cannot canonicalise.
    NotCanonical(&'static str),
    Qr(QrError),
}

impl std::fmt::Display for ZatcaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZatcaError::Invalid(why) => write!(f, "{why}"),
            ZatcaError::NotCanonical(why) => write!(f, "cannot hash this XML: {why}"),
            ZatcaError::Qr(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ZatcaError {}

impl From<QrError> for ZatcaError {
    fn from(e: QrError) -> Self {
        ZatcaError::Qr(e)
    }
}

fn invalid(why: impl Into<String>) -> ZatcaError {
    ZatcaError::Invalid(why.into())
}

/// The rules [`generate`] enforces (errors) and the ones it only reports
/// (warnings), so that a document can still be previewed while the company
/// fills in its address.
pub fn check(inv: &EInvoice) -> Result<Vec<String>, ZatcaError> {
    if inv.number.trim().is_empty() {
        return Err(invalid("the invoice number is missing"));
    }
    if inv.uuid.is_empty() || !inv.uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(invalid("the UUID may only have letters, digits and dashes (BR-KSA-03)"));
    }
    if inv.lines.is_empty() {
        return Err(invalid("an e-invoice needs at least one line"));
    }
    let seller_vat = inv.seller.vat_number.as_deref().unwrap_or_default();
    vat_number::validate(seller_vat).map_err(|e| invalid(format!("the seller's VAT number is invalid: {e}")))?;
    if inv.seller.name.trim().is_empty() {
        return Err(invalid("the seller's name is missing"));
    }
    if inv.doc_type != DocumentType::Invoice && inv.billing_reference.as_deref().is_none_or(|r| r.trim().is_empty()) {
        return Err(invalid("a credit or debit note must name the invoice it corrects"));
    }
    if inv.kind == InvoiceKind::Standard && inv.buyer.as_ref().is_none_or(|b| b.name.trim().is_empty()) {
        return Err(invalid("a standard tax invoice needs the buyer's name (BR-KSA-42)"));
    }
    let negative = inv.lines.iter().map(|l| (l.net_amount, l.quantity, l.unit_price)).any(|(a, q, p)| a < Decimal::ZERO || q < Decimal::ZERO || p < Decimal::ZERO)
        || inv.allowances.iter().any(|a| a.amount < Decimal::ZERO);
    if negative {
        return Err(invalid("amounts are positive on every document; the type code says whether it reduces"));
    }

    let mut w = Vec::new();
    let a = &inv.seller.address;
    for (field, value) in [("street", &a.street), ("district", &a.district), ("city", &a.city), ("postal code", &a.postal_code)] {
        if value.trim().is_empty() {
            w.push(format!("the seller's {field} is missing from its National Address (BR-KSA-09)"));
        }
    }
    if !(a.building_number.len() == 4 && a.building_number.bytes().all(|b| b.is_ascii_digit())) {
        w.push("the seller's building number must be 4 digits (BR-KSA-37)".to_owned());
    }
    if inv.seller.other_id.is_none() {
        w.push("the seller's CRN (or other ID) is missing (BR-KSA-08)".to_owned());
    }
    if let Some(b) = &inv.buyer {
        if inv.kind == InvoiceKind::Standard
            && [&b.address.street, &b.address.city, &b.address.country].iter().any(|v| v.trim().is_empty())
        {
            w.push("a standard invoice needs the buyer's street, city and country (BR-KSA-10)".to_owned());
        }
        if inv.export && b.vat_number.is_some() {
            w.push("an export invoice must not carry a buyer VAT number (BR-KSA-46)".to_owned());
        }
    }
    if inv.kind == InvoiceKind::Standard && inv.supply_date.is_none() {
        w.push("a standard invoice needs the supply date (BR-KSA-15)".to_owned());
    }
    if inv.doc_type != DocumentType::Invoice && inv.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
        w.push("give the reason for the credit or debit note".to_owned());
    }
    let mut cats = inv.lines.iter().map(|l| &l.category).chain(inv.allowances.iter().map(|a| &a.category));
    if cats.any(|c| c.treatment != Treatment::Standard && c.exemption.is_none()) {
        w.push("zero-rated, exempt and out-of-scope lines need a VATEX reason code (BR-KSA-69); name it in the tax class".to_owned());
    }
    for (i, l) in inv.lines.iter().enumerate() {
        if round_halalas(l.quantity * l.unit_price) != round_halalas(l.net_amount) {
            w.push(format!("line {}: quantity × price is not the line amount", i + 1));
        }
    }
    Ok(w)
}

// ------------------------------------------------------------ XML writer

/// A tiny writer whose output is already in canonical form: start/end tag
/// pairs, attributes sorted by name, C14N escaping, LF line ends.
struct Xml {
    out: String,
    depth: usize,
}

fn text_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '\r' => o.push_str("&#xD;"),
            c => o.push(c),
        }
    }
    o
}

fn attr_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '"' => o.push_str("&quot;"),
            '\t' => o.push_str("&#x9;"),
            '\n' => o.push_str("&#xA;"),
            '\r' => o.push_str("&#xD;"),
            c => o.push(c),
        }
    }
    o
}

impl Xml {
    fn tag(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.out.push('\n');
        self.out.push_str(&"    ".repeat(self.depth));
        self.out.push('<');
        self.out.push_str(name);
        let mut attrs = attrs.to_vec();
        attrs.sort_by_key(|(k, _)| *k);
        for (k, v) in attrs {
            self.out.push_str(&format!(" {k}=\"{}\"", attr_escape(v)));
        }
        self.out.push('>');
    }

    fn open(&mut self, name: &str) {
        self.tag(name, &[]);
        self.depth += 1;
    }

    fn close(&mut self, name: &str) {
        self.depth -= 1;
        self.out.push('\n');
        self.out.push_str(&"    ".repeat(self.depth));
        self.out.push_str(&format!("</{name}>"));
    }

    fn leaf_with(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) {
        self.tag(name, attrs);
        self.out.push_str(&text_escape(text));
        self.out.push_str(&format!("</{name}>"));
    }

    fn leaf(&mut self, name: &str, text: &str) {
        self.leaf_with(name, &[], text);
    }

    /// A leaf only when there is something to say.
    fn opt(&mut self, name: &str, text: &str) {
        if !text.trim().is_empty() {
            self.leaf(name, text.trim());
        }
    }

    fn sar(&mut self, name: &str, a: Decimal) {
        self.leaf_with(name, &[("currencyID", "SAR")], &money(a));
    }
}

/// An amount with exactly two decimals ("1150.00").
pub fn money(a: Decimal) -> String {
    format!("{:.2}", round_halalas(a))
}

fn number(d: Decimal) -> String {
    d.normalize().to_string()
}

fn tax_scheme(x: &mut Xml, with_scheme_ids: bool) {
    x.open("cac:TaxScheme");
    if with_scheme_ids {
        x.leaf_with("cbc:ID", &[("schemeAgencyID", "6"), ("schemeID", "UN/ECE 5153")], "VAT");
    } else {
        x.leaf("cbc:ID", "VAT");
    }
    x.close("cac:TaxScheme");
}

fn tax_category(x: &mut Xml, element: &str, c: &Category, with_scheme_ids: bool) {
    x.open(element);
    if with_scheme_ids {
        x.leaf_with("cbc:ID", &[("schemeAgencyID", "6"), ("schemeID", "UN/ECE 5305")], c.treatment.category());
    } else {
        x.leaf("cbc:ID", c.treatment.category());
    }
    x.leaf("cbc:Percent", &format!("{:.2}", c.percent));
    if let Some((code, reason)) = &c.exemption {
        x.leaf("cbc:TaxExemptionReasonCode", code);
        x.leaf("cbc:TaxExemptionReason", reason);
    }
    tax_scheme(x, with_scheme_ids);
    x.close(element);
}

fn party(x: &mut Xml, p: &Party, seller: bool) {
    x.open("cac:Party");
    if let Some((scheme, id)) = &p.other_id {
        x.open("cac:PartyIdentification");
        x.leaf_with("cbc:ID", &[("schemeID", scheme)], id);
        x.close("cac:PartyIdentification");
    }
    let a = &p.address;
    x.open("cac:PostalAddress");
    x.opt("cbc:StreetName", &a.street);
    x.opt("cbc:AdditionalStreetName", &a.additional_street);
    x.opt("cbc:BuildingNumber", &a.building_number);
    x.opt("cbc:PlotIdentification", &a.additional_number);
    x.opt("cbc:CitySubdivisionName", &a.district);
    x.opt("cbc:CityName", &a.city);
    x.opt("cbc:PostalZone", &a.postal_code);
    x.opt("cbc:CountrySubentity", &a.region);
    if !a.country.trim().is_empty() {
        x.open("cac:Country");
        x.leaf("cbc:IdentificationCode", a.country.trim());
        x.close("cac:Country");
    }
    x.close("cac:PostalAddress");
    if seller || p.vat_number.is_some() {
        x.open("cac:PartyTaxScheme");
        if let Some(v) = &p.vat_number {
            x.leaf("cbc:CompanyID", v);
        }
        tax_scheme(x, false);
        x.close("cac:PartyTaxScheme");
    }
    x.open("cac:PartyLegalEntity");
    x.leaf("cbc:RegistrationName", p.name.trim());
    x.close("cac:PartyLegalEntity");
    x.close("cac:Party");
}

/// `cac:Signature` (BR-KSA-60), in the SDK sample's layout.
const SIGNATURE_BLOCK: &str = "<cac:Signature>\n      <cbc:ID>urn:oasis:names:specification:ubl:signature:Invoice</cbc:ID>\n      <cbc:SignatureMethod>urn:oasis:names:specification:ubl:dsig:enveloped:xades</cbc:SignatureMethod>\n</cac:Signature>";

fn embedded(x: &mut Xml, id: &str, value: &str) {
    x.open("cac:AdditionalDocumentReference");
    x.leaf("cbc:ID", id);
    x.open("cac:Attachment");
    x.leaf_with("cbc:EmbeddedDocumentBinaryObject", &[("mimeCode", "text/plain")], value);
    x.close("cac:Attachment");
    x.close("cac:AdditionalDocumentReference");
}

/// The XML without its declaration. `ext` is the `ext:UBLExtensions`
/// block of a stamped document; with it the `cac:Signature` block is
/// written too. Both are written without whitespace around them, so the
/// hashed text is the same with or without them.
fn render(inv: &EInvoice, icv: u64, pih: &str, qr: &str, ext: Option<&str>) -> String {
    let t = totals(inv);
    let mut x = Xml {
        out: format!("<Invoice xmlns=\"{NS_INVOICE}\" xmlns:cac=\"{NS_CAC}\" xmlns:cbc=\"{NS_CBC}\" xmlns:ext=\"{NS_EXT}\">{}", ext.unwrap_or_default()),
        depth: 1,
    };
    x.leaf("cbc:ProfileID", PROFILE_ID);
    x.leaf("cbc:ID", inv.number.trim());
    x.leaf("cbc:UUID", &inv.uuid);
    x.leaf("cbc:IssueDate", &inv.issue_date.format("%Y-%m-%d").to_string());
    x.leaf("cbc:IssueTime", &inv.issue_time.format("%H:%M:%S").to_string());
    x.leaf_with("cbc:InvoiceTypeCode", &[("name", &inv.subtype())], inv.doc_type.code());
    x.leaf("cbc:DocumentCurrencyCode", "SAR");
    x.leaf("cbc:TaxCurrencyCode", "SAR");
    if let Some(r) = inv.billing_reference.as_deref().filter(|_| inv.doc_type != DocumentType::Invoice) {
        x.open("cac:BillingReference");
        x.open("cac:InvoiceDocumentReference");
        x.leaf("cbc:ID", r.trim());
        x.close("cac:InvoiceDocumentReference");
        x.close("cac:BillingReference");
    }
    x.open("cac:AdditionalDocumentReference");
    x.leaf("cbc:ID", "ICV");
    x.leaf("cbc:UUID", &icv.to_string());
    x.close("cac:AdditionalDocumentReference");
    embedded(&mut x, "PIH", pih);
    embedded(&mut x, "QR", qr);
    if ext.is_some() {
        x.out.push_str(SIGNATURE_BLOCK);
    }
    x.open("cac:AccountingSupplierParty");
    party(&mut x, &inv.seller, true);
    x.close("cac:AccountingSupplierParty");
    // UBL requires the element; a simplified invoice may leave it empty
    // [UNCERTAIN: compare with the SDK's simplified samples].
    x.open("cac:AccountingCustomerParty");
    if let Some(b) = &inv.buyer {
        party(&mut x, b, false);
    }
    x.close("cac:AccountingCustomerParty");
    if let Some(d) = inv.supply_date {
        x.open("cac:Delivery");
        x.leaf("cbc:ActualDeliveryDate", &d.format("%Y-%m-%d").to_string());
        x.close("cac:Delivery");
    }
    x.open("cac:PaymentMeans");
    x.leaf("cbc:PaymentMeansCode", if inv.payment_means.trim().is_empty() { "10" } else { inv.payment_means.trim() });
    if inv.doc_type != DocumentType::Invoice {
        x.opt("cbc:InstructionNote", inv.reason.as_deref().unwrap_or_default());
    }
    x.close("cac:PaymentMeans");
    for a in &inv.allowances {
        x.open("cac:AllowanceCharge");
        x.leaf("cbc:ChargeIndicator", "false");
        x.leaf("cbc:AllowanceChargeReason", a.reason.trim());
        x.sar("cbc:Amount", a.amount);
        tax_category(&mut x, "cac:TaxCategory", &a.category, true);
        x.close("cac:AllowanceCharge");
    }
    x.open("cac:TaxTotal");
    x.sar("cbc:TaxAmount", t.vat);
    for s in &t.subtotals {
        x.open("cac:TaxSubtotal");
        x.sar("cbc:TaxableAmount", s.taxable);
        x.sar("cbc:TaxAmount", s.vat);
        tax_category(&mut x, "cac:TaxCategory", &s.category, true);
        x.close("cac:TaxSubtotal");
    }
    x.close("cac:TaxTotal");
    // The second TaxTotal: the VAT in the tax currency (SAR), no breakdown.
    x.open("cac:TaxTotal");
    x.sar("cbc:TaxAmount", t.vat);
    x.close("cac:TaxTotal");
    x.open("cac:LegalMonetaryTotal");
    x.sar("cbc:LineExtensionAmount", t.line_extension);
    x.sar("cbc:TaxExclusiveAmount", t.tax_exclusive);
    x.sar("cbc:TaxInclusiveAmount", t.tax_inclusive);
    x.sar("cbc:AllowanceTotalAmount", t.allowance_total);
    x.sar("cbc:PrepaidAmount", Decimal::ZERO);
    x.sar("cbc:PayableAmount", t.payable);
    x.close("cac:LegalMonetaryTotal");
    for (i, l) in inv.lines.iter().enumerate() {
        let vat = line_vat(l);
        x.open("cac:InvoiceLine");
        x.leaf("cbc:ID", &(i + 1).to_string());
        let unit = if l.unit_code.trim().is_empty() { "PCE" } else { l.unit_code.trim() };
        x.leaf_with("cbc:InvoicedQuantity", &[("unitCode", unit)], &number(l.quantity));
        x.sar("cbc:LineExtensionAmount", l.net_amount);
        x.open("cac:TaxTotal");
        x.sar("cbc:TaxAmount", vat);
        x.sar("cbc:RoundingAmount", round_halalas(l.net_amount) + vat);
        x.close("cac:TaxTotal");
        x.open("cac:Item");
        x.leaf("cbc:Name", l.name.trim());
        tax_category(&mut x, "cac:ClassifiedTaxCategory", &l.category, false);
        x.close("cac:Item");
        x.open("cac:Price");
        x.leaf_with("cbc:PriceAmount", &[("currencyID", "SAR")], &number(l.unit_price));
        x.close("cac:Price");
        x.close("cac:InvoiceLine");
    }
    x.out.push_str("\n</Invoice>");
    x.out
}

/// Cut one element (from its start tag to its end tag) out of `xml`,
/// keeping the whitespace around it. `pick` chooses among the elements of
/// that name by their content.
fn cut(xml: &mut String, name: &str, pick: impl Fn(&str) -> bool) -> Result<(), ZatcaError> {
    let (open_plain, open_attr, close) = (format!("<{name}>"), format!("<{name} "), format!("</{name}>"));
    let mut from = 0;
    while let Some(rel) = xml[from..].find(&format!("<{name}")) {
        let start = from + rel;
        let rest = &xml[start..];
        if !(rest.starts_with(&open_plain) || rest.starts_with(&open_attr)) {
            from = start + 1;
            continue;
        }
        let end = start + rest.find(&close).ok_or(ZatcaError::NotCanonical("an element is not closed"))? + close.len();
        if xml[start + 1..end].contains(&format!("<{name}>")) {
            return Err(ZatcaError::NotCanonical("nested elements of the same name"));
        }
        if pick(&xml[start..end]) {
            xml.replace_range(start..end, "");
        } else {
            from = end;
        }
    }
    Ok(())
}

/// The text that is hashed: the document element only (no XML
/// declaration), with `ext:UBLExtensions`, `cac:Signature` and the
/// `cac:AdditionalDocumentReference` whose ID is "QR" removed. Accepts
/// KhataOS's own serialisation (see the module notes); refuses XML whose
/// hashed part has self-closing tags, comments, CDATA, processing
/// instructions or carriage returns, which it would have to canonicalise
/// (the removed signature block may have self-closing tags: it is not
/// hashed).
pub fn hash_input(xml: &str) -> Result<String, ZatcaError> {
    let start = xml.find("<Invoice").ok_or(ZatcaError::NotCanonical("no Invoice element"))?;
    let end = xml.rfind("</Invoice>").ok_or(ZatcaError::NotCanonical("no Invoice element"))? + "</Invoice>".len();
    let mut doc = xml[start..end].to_owned();
    cut(&mut doc, "ext:UBLExtensions", |_| true)?;
    cut(&mut doc, "cac:Signature", |_| true)?;
    cut(&mut doc, "cac:AdditionalDocumentReference", |e| e.contains("<cbc:ID>QR</cbc:ID>"))?;
    if doc.contains("/>") {
        return Err(ZatcaError::NotCanonical("self-closing tags"));
    }
    if doc.contains("<!--") || doc.contains("<![CDATA[") || doc.contains("<?") {
        return Err(ZatcaError::NotCanonical("comments, CDATA or processing instructions"));
    }
    if doc.contains('\r') {
        return Err(ZatcaError::NotCanonical("carriage returns"));
    }
    Ok(doc)
}

/// The invoice hash: SHA-256 of [`hash_input`], as Base64 and as bytes.
pub fn invoice_hash(xml: &str) -> Result<(String, [u8; 32]), ZatcaError> {
    let digest: [u8; 32] = Sha256::digest(hash_input(xml)?.as_bytes()).into();
    Ok((STANDARD.encode(digest), digest))
}

/// A device's (EGS's) place in the chain: one sequence per device for every
/// document type (Detailed Guideline v2 §4.3, research §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ChainState {
    /// The last ICV used; 0 before the first document. Never reset.
    pub last_icv: u64,
    /// The hash of the last document generated (even one later rejected).
    pub last_hash: String,
}

impl Default for ChainState {
    fn default() -> Self {
        Self { last_icv: 0, last_hash: FIRST_PIH.to_owned() }
    }
}

/// A generated document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Generated {
    /// The XML with its declaration and the QR reference.
    pub xml: String,
    pub uuid: String,
    pub icv: u64,
    /// The PIH it carries.
    pub pih: String,
    /// Its own hash (Base64), the next document's PIH.
    pub hash: String,
    /// The QR payload (Base64 TLV).
    pub qr: String,
    /// The stamp's signing time (UTC, "…Z"), when stamped.
    pub signing_time: Option<String>,
    pub totals: Totals,
    pub warnings: Vec<String>,
}

/// Build a document at a given place in the chain, stamped when a
/// [`Stamp`] is given. Deterministic: the same input (and signing time)
/// gives the same bytes.
pub fn generate(inv: &EInvoice, icv: u64, pih: &str, stamp: Option<&Stamp>) -> Result<Generated, ZatcaError> {
    let warnings = check(inv)?;
    if icv == 0 {
        return Err(invalid("the invoice counter (ICV) starts at 1"));
    }
    if pih.trim().is_empty() {
        return Err(invalid("the previous invoice hash (PIH) is missing"));
    }
    // The QR reference is cut out before hashing, whitespace around it kept,
    // so the hash is taken on the final layout with an empty QR.
    let (hash, digest) = invoice_hash(&render(inv, icv, pih, "", None))?;
    let signature = stamp.map(|s| crate::xades::signature_value(s.signer, &digest));
    let t = totals(inv);
    let fields = QrFields {
        seller_name: inv.seller.name.trim().to_owned(),
        vat_number: vat_number::normalise(inv.seller.vat_number.as_deref().unwrap_or_default()),
        timestamp: format!("{}T{}", inv.issue_date.format("%Y-%m-%d"), inv.issue_time.format("%H:%M:%S")),
        total_with_vat: money(t.tax_inclusive),
        vat_total: money(t.vat),
        stamp: stamp.zip(signature.as_deref()).map(|(s, sig)| crate::xades::qr_stamp(&hash, sig, s.certificate, inv.kind == InvoiceKind::Simplified)),
    };
    let qr = qr::encode(&fields)?;
    let ext = stamp
        .zip(signature.as_deref())
        .map(|(s, sig)| crate::xades::ubl_extensions(&hash, sig, s.certificate, s.signing_time));
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", render(inv, icv, pih, &qr, ext.as_deref()));
    debug_assert_eq!(invoice_hash(&xml).map(|h| h.0).as_ref(), Ok(&hash), "the QR and the stamp are outside the hash");
    let signing_time = stamp.map(|s| crate::xades::signing_time_text(s.signing_time));
    Ok(Generated { xml, uuid: inv.uuid.clone(), icv, pih: pih.to_owned(), hash, qr, signing_time, totals: t, warnings })
}

/// Build the device's next document and move the chain on.
pub fn generate_next(inv: &EInvoice, chain: &mut ChainState, stamp: Option<&Stamp>) -> Result<Generated, ZatcaError> {
    let icv = chain.last_icv.checked_add(1).ok_or_else(|| invalid("the invoice counter is exhausted"))?;
    let g = generate(inv, icv, &chain.last_hash, stamp)?;
    chain.last_icv = g.icv;
    chain.last_hash = g.hash.clone();
    Ok(g)
}

/// The archive file name: `<seller VAT>_<yyyymmdd>T<hhmmss>_<number>.xml`
/// with the number's non-alphanumerics replaced by "-" (XML standard §14).
pub fn file_name(inv: &EInvoice) -> String {
    format!(
        "{}_{}T{}_{}.xml",
        vat_number::normalise(inv.seller.vat_number.as_deref().unwrap_or_default()),
        inv.issue_date.format("%Y%m%d"),
        inv.issue_time.format("%H%M%S"),
        crate::vat::file_name_form(inv.number.trim())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const SELLER_VAT: &str = "300075588700003";
    const BUYER_VAT: &str = "311111111100003";

    fn seller() -> Party {
        Party {
            name: "Riyadh Trading Co".into(),
            vat_number: Some(SELLER_VAT.into()),
            other_id: Some(("CRN".into(), "1010010000".into())),
            address: Address {
                street: "King Fahd Road".into(),
                building_number: "1234".into(),
                additional_number: "5678".into(),
                district: "Al Olaya".into(),
                city: "Riyadh".into(),
                postal_code: "12211".into(),
                region: "Riyadh".into(),
                country: "SA".into(),
                ..Address::default()
            },
        }
    }

    fn buyer() -> Party {
        Party {
            name: "Jeddah Supplies & Sons".into(),
            vat_number: Some(BUYER_VAT.into()),
            other_id: None,
            address: Address {
                street: "Tahlia Street".into(),
                city: "Jeddah".into(),
                country: "SA".into(),
                ..Address::default()
            },
        }
    }

    fn line(name: &str, qty: Decimal, price: Decimal) -> Line {
        Line {
            name: name.into(),
            quantity: qty,
            unit_code: String::new(),
            unit_price: price,
            net_amount: qty * price,
            category: Category::standard(dec!(15)),
        }
    }

    fn invoice(kind: InvoiceKind) -> EInvoice {
        EInvoice {
            kind,
            doc_type: DocumentType::Invoice,
            number: "SINV/2026/1".into(),
            uuid: "3cf5ee18-ee25-44ea-a444-2c37ba7f28be".into(),
            issue_date: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            issue_time: NaiveTime::from_hms_opt(10, 15, 30).unwrap(),
            supply_date: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            export: false,
            billing_reference: None,
            reason: None,
            payment_means: "10".into(),
            seller: seller(),
            buyer: (kind == InvoiceKind::Standard).then(buyer),
            lines: vec![line("Laptop", dec!(2), dec!(400)), line("Mouse <USB>", dec!(1), dec!(200))],
            allowances: vec![],
        }
    }

    fn first(inv: &EInvoice) -> Generated {
        generate(inv, 1, FIRST_PIH, None).unwrap()
    }

    #[test]
    fn the_first_pih_is_the_published_value() {
        let hex: String = Sha256::digest(b"0").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(STANDARD.encode(hex), FIRST_PIH);
        assert_eq!(ChainState::default(), ChainState { last_icv: 0, last_hash: FIRST_PIH.into() });
    }

    #[test]
    fn a_standard_invoice_has_every_required_element() {
        let g = first(&invoice(InvoiceKind::Standard));
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        assert!(g.xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Invoice xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\""));
        for needle in [
            "<cbc:ProfileID>reporting:1.0</cbc:ProfileID>",
            "<cbc:ID>SINV/2026/1</cbc:ID>",
            "<cbc:UUID>3cf5ee18-ee25-44ea-a444-2c37ba7f28be</cbc:UUID>",
            "<cbc:IssueDate>2026-05-01</cbc:IssueDate>",
            "<cbc:IssueTime>10:15:30</cbc:IssueTime>",
            "<cbc:InvoiceTypeCode name=\"0100000\">388</cbc:InvoiceTypeCode>",
            "<cbc:DocumentCurrencyCode>SAR</cbc:DocumentCurrencyCode>",
            "<cbc:TaxCurrencyCode>SAR</cbc:TaxCurrencyCode>",
            "<cbc:ID>ICV</cbc:ID>\n        <cbc:UUID>1</cbc:UUID>",
            &format!("<cbc:ID>PIH</cbc:ID>\n        <cac:Attachment>\n            <cbc:EmbeddedDocumentBinaryObject mimeCode=\"text/plain\">{FIRST_PIH}</cbc:EmbeddedDocumentBinaryObject>"),
            "<cbc:ID>QR</cbc:ID>",
            "<cbc:ID schemeID=\"CRN\">1010010000</cbc:ID>",
            "<cbc:StreetName>King Fahd Road</cbc:StreetName>",
            "<cbc:BuildingNumber>1234</cbc:BuildingNumber>",
            "<cbc:PlotIdentification>5678</cbc:PlotIdentification>",
            "<cbc:CitySubdivisionName>Al Olaya</cbc:CitySubdivisionName>",
            "<cbc:CityName>Riyadh</cbc:CityName>",
            "<cbc:PostalZone>12211</cbc:PostalZone>",
            "<cbc:CountrySubentity>Riyadh</cbc:CountrySubentity>",
            "<cbc:IdentificationCode>SA</cbc:IdentificationCode>",
            &format!("<cbc:CompanyID>{SELLER_VAT}</cbc:CompanyID>"),
            &format!("<cbc:CompanyID>{BUYER_VAT}</cbc:CompanyID>"),
            "<cbc:RegistrationName>Jeddah Supplies &amp; Sons</cbc:RegistrationName>",
            "<cbc:ActualDeliveryDate>2026-05-01</cbc:ActualDeliveryDate>",
            "<cbc:PaymentMeansCode>10</cbc:PaymentMeansCode>",
            "<cbc:TaxAmount currencyID=\"SAR\">150.00</cbc:TaxAmount>",
            "<cbc:TaxableAmount currencyID=\"SAR\">1000.00</cbc:TaxableAmount>",
            "<cbc:ID schemeAgencyID=\"6\" schemeID=\"UN/ECE 5305\">S</cbc:ID>",
            "<cbc:Percent>15.00</cbc:Percent>",
            "<cbc:LineExtensionAmount currencyID=\"SAR\">1000.00</cbc:LineExtensionAmount>",
            "<cbc:TaxExclusiveAmount currencyID=\"SAR\">1000.00</cbc:TaxExclusiveAmount>",
            "<cbc:TaxInclusiveAmount currencyID=\"SAR\">1150.00</cbc:TaxInclusiveAmount>",
            "<cbc:PayableAmount currencyID=\"SAR\">1150.00</cbc:PayableAmount>",
            "<cbc:InvoicedQuantity unitCode=\"PCE\">2</cbc:InvoicedQuantity>",
            "<cbc:RoundingAmount currencyID=\"SAR\">920.00</cbc:RoundingAmount>",
            "<cbc:Name>Mouse &lt;USB&gt;</cbc:Name>",
            "<cac:ClassifiedTaxCategory>\n                <cbc:ID>S</cbc:ID>",
            "<cbc:PriceAmount currencyID=\"SAR\">400</cbc:PriceAmount>",
        ] {
            assert!(g.xml.contains(needle), "missing {needle:?} in\n{}", g.xml);
        }
        assert_eq!(g.xml.matches("<cac:TaxTotal>").count(), 2 + 2, "two document TaxTotals and one per line");
        assert_eq!(g.xml.matches("<cac:TaxSubtotal>").count(), 1);
        assert!(!g.xml.contains("/>"), "no self-closing tags");
        assert!(!g.xml.contains("BillingReference"));
        // UBL order: references, then parties, delivery, payment, totals, lines.
        let at = |s: &str| g.xml.find(s).unwrap();
        assert!(at("<cbc:ID>ICV") < at("<cbc:ID>PIH") && at("<cbc:ID>PIH") < at("<cbc:ID>QR"));
        assert!(at("<cbc:ID>QR") < at("AccountingSupplierParty") && at("AccountingSupplierParty") < at("AccountingCustomerParty"));
        assert!(at("AccountingCustomerParty") < at("<cac:Delivery>") && at("<cac:Delivery>") < at("<cac:PaymentMeans>"));
        assert!(at("<cac:PaymentMeans>") < at("<cac:TaxTotal>") && at("<cac:LegalMonetaryTotal>") < at("<cac:InvoiceLine>"));
        assert_eq!(file_name(&invoice(InvoiceKind::Standard)), "300075588700003_20260501T101530_SINV-2026-1.xml");
    }

    #[test]
    fn a_simplified_invoice_needs_no_buyer() {
        let g = first(&invoice(InvoiceKind::Simplified));
        assert!(g.xml.contains("<cbc:InvoiceTypeCode name=\"0200000\">388</cbc:InvoiceTypeCode>"));
        assert!(g.xml.contains("<cac:AccountingCustomerParty>\n    </cac:AccountingCustomerParty>"));
        let mut std = invoice(InvoiceKind::Standard);
        std.buyer = None;
        assert!(generate(&std, 1, FIRST_PIH, None).unwrap_err().to_string().contains("buyer's name"));
    }

    #[test]
    fn credit_and_debit_notes_reference_the_invoice() {
        let mut cn = invoice(InvoiceKind::Standard);
        cn.doc_type = DocumentType::CreditNote;
        assert!(generate(&cn, 1, FIRST_PIH, None).unwrap_err().to_string().contains("invoice it corrects"));
        cn.billing_reference = Some("SINV/2026/1".into());
        let g = first(&cn);
        assert!(g.warnings.iter().any(|w| w.contains("reason")), "{:?}", g.warnings);
        cn.reason = Some("Returned goods".into());
        let g = first(&cn);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        for needle in [
            "<cbc:InvoiceTypeCode name=\"0100000\">381</cbc:InvoiceTypeCode>",
            "<cac:BillingReference>\n        <cac:InvoiceDocumentReference>\n            <cbc:ID>SINV/2026/1</cbc:ID>",
            "<cbc:InstructionNote>Returned goods</cbc:InstructionNote>",
            "<cbc:PayableAmount currencyID=\"SAR\">1150.00</cbc:PayableAmount>",
        ] {
            assert!(g.xml.contains(needle), "missing {needle:?}");
        }
        let mut dn = cn.clone();
        dn.doc_type = DocumentType::DebitNote;
        dn.kind = InvoiceKind::Simplified;
        assert!(first(&dn).xml.contains("<cbc:InvoiceTypeCode name=\"0200000\">383</cbc:InvoiceTypeCode>"));
    }

    #[test]
    fn exports_zero_rated_lines_and_allowances() {
        let mut inv = invoice(InvoiceKind::Standard);
        inv.export = true;
        inv.buyer = Some(Party { vat_number: None, address: Address { country: "IN".into(), ..buyer().address }, ..buyer() });
        let zero = Category { treatment: Treatment::ZeroRated, percent: dec!(0), exemption: Some(("VATEX-SA-32".into(), "Export of goods".into())) };
        for l in &mut inv.lines {
            l.category = zero.clone();
        }
        inv.allowances.push(Allowance { reason: "Discount".into(), amount: dec!(100), category: zero });
        let g = first(&inv);
        assert!(g.warnings.is_empty(), "{:?}", g.warnings);
        assert_eq!(inv.subtype(), "0100100");
        for needle in [
            "name=\"0100100\"",
            "<cbc:TaxExemptionReasonCode>VATEX-SA-32</cbc:TaxExemptionReasonCode>",
            "<cbc:TaxExemptionReason>Export of goods</cbc:TaxExemptionReason>",
            "<cbc:ChargeIndicator>false</cbc:ChargeIndicator>",
            "<cbc:AllowanceTotalAmount currencyID=\"SAR\">100.00</cbc:AllowanceTotalAmount>",
            "<cbc:TaxableAmount currencyID=\"SAR\">900.00</cbc:TaxableAmount>",
            "<cbc:PayableAmount currencyID=\"SAR\">900.00</cbc:PayableAmount>",
        ] {
            assert!(g.xml.contains(needle), "missing {needle:?}");
        }
        let mut simplified = inv.clone();
        simplified.kind = InvoiceKind::Simplified;
        assert_eq!(simplified.subtype(), "0200000", "no export flag on simplified documents");
    }

    #[test]
    fn categories_round_once_and_warn_without_reasons() {
        let mut inv = invoice(InvoiceKind::Standard);
        // Ten lines of 0.10 at 15%: 0.15 at document level, 0.20 if each line were rounded.
        inv.lines = (0..10).map(|i| line(&format!("Pin {i}"), dec!(1), dec!(0.10))).collect();
        inv.lines.push(Line { category: Category { treatment: Treatment::Exempt, percent: dec!(0), exemption: None }, ..line("Insurance", dec!(1), dec!(50)) });
        let t = totals(&inv);
        assert_eq!((t.vat, t.tax_inclusive, t.subtotals.len()), (dec!(0.15), dec!(51.15), 2));
        assert_eq!(line_vat(&inv.lines[0]), dec!(0.02));
        let g = first(&inv);
        assert!(g.warnings.iter().any(|w| w.contains("BR-KSA-69")), "{:?}", g.warnings);
    }

    #[test]
    fn hard_errors_refuse_to_build() {
        let base = invoice(InvoiceKind::Simplified);
        let fails = |f: &dyn Fn(&mut EInvoice)| {
            let mut inv = base.clone();
            f(&mut inv);
            generate(&inv, 1, FIRST_PIH, None).is_err()
        };
        assert!(fails(&|i| i.lines.clear()));
        assert!(fails(&|i| i.seller.vat_number = Some("123".into())));
        assert!(fails(&|i| i.uuid = "not a uuid!".into()));
        assert!(fails(&|i| i.number = " ".into()));
        assert!(fails(&|i| i.lines[0].net_amount = dec!(-1)));
        assert!(generate(&base, 0, FIRST_PIH, None).is_err(), "ICV starts at 1");
        assert!(generate(&base, 1, "", None).is_err());
        let mut sparse = base.clone();
        sparse.seller.address = Address::default();
        sparse.seller.other_id = None;
        let w = first(&sparse).warnings;
        assert!(w.iter().any(|w| w.contains("BR-KSA-37")) && w.iter().any(|w| w.contains("BR-KSA-08")), "{w:?}");
    }

    #[test]
    fn the_hash_is_deterministic_and_follows_the_content() {
        let inv = invoice(InvoiceKind::Simplified);
        let a = first(&inv);
        assert_eq!(a, first(&inv), "same input, same XML, hash and QR");
        assert_eq!(a.hash.len(), 44, "Base64 of 32 bytes");
        assert_eq!(invoice_hash(&a.xml).unwrap().0, a.hash, "the hash is recomputable from the XML");
        let mut changed = inv.clone();
        changed.lines[0].unit_price = dec!(401);
        changed.lines[0].net_amount = dec!(802);
        assert_ne!(first(&changed).hash, a.hash);
        assert_ne!(generate(&inv, 2, FIRST_PIH, None).unwrap().hash, a.hash, "the ICV is hashed");
        assert_ne!(generate(&inv, 1, &a.hash, None).unwrap().hash, a.hash, "the PIH is hashed");
    }

    #[test]
    fn the_hash_ignores_extensions_signature_and_qr() {
        let a = first(&invoice(InvoiceKind::Simplified));
        let input = hash_input(&a.xml).unwrap();
        assert!(input.starts_with("<Invoice ") && input.ends_with("</Invoice>"));
        assert!(!input.contains("<cbc:ID>QR</cbc:ID>") && input.contains("<cbc:ID>PIH</cbc:ID>"));
        // A signed document's extra blocks don't change the hash.
        let signed = a
            .xml
            .replacen("<cbc:ProfileID>", "<ext:UBLExtensions><ext:UBLExtension><ext:ExtensionContent>stamp</ext:ExtensionContent></ext:UBLExtension></ext:UBLExtensions>\n    <cbc:ProfileID>", 1)
            .replacen("<cac:AccountingSupplierParty>", "<cac:Signature><cbc:ID>urn:oasis:names:specification:ubl:signature:Invoice</cbc:ID></cac:Signature>\n    <cac:AccountingSupplierParty>", 1);
        let expected_input = input
            .replacen("<cbc:ProfileID>", "\n    <cbc:ProfileID>", 1)
            .replacen("<cac:AccountingSupplierParty>", "\n    <cac:AccountingSupplierParty>", 1);
        assert_eq!(hash_input(&signed).unwrap(), expected_input, "elements go; the whitespace around them stays");
        assert!(hash_input("<Invoice><a/></Invoice>").is_err());
        assert!(hash_input("<Invoice>\r\n</Invoice>").is_err());
        assert!(hash_input("<Invoice><!-- x --></Invoice>").is_err());
        assert!(hash_input("nothing").is_err());
    }

    #[test]
    fn the_qr_carries_tags_one_to_five() {
        let g = first(&invoice(InvoiceKind::Simplified));
        let fields = qr::decode(&g.qr).unwrap();
        let text: Vec<(u8, String)> = fields.into_iter().map(|(t, v)| (t, String::from_utf8(v).unwrap())).collect();
        assert_eq!(text, [
            (1, "Riyadh Trading Co".to_owned()),
            (2, SELLER_VAT.to_owned()),
            (3, "2026-05-01T10:15:30".to_owned()),
            (4, "1150.00".to_owned()),
            (5, "150.00".to_owned()),
        ]);
        assert!(g.xml.contains(&format!("mimeCode=\"text/plain\">{}</cbc:EmbeddedDocumentBinaryObject>", g.qr)));
    }

    #[test]
    fn a_stamp_adds_the_signature_and_tags_six_to_nine() {
        use crate::xades::tests::{test_cert, test_signer, time};
        let (signer, cert) = (test_signer(), test_cert());
        let stamp = Stamp::new(&signer, &cert, time()).unwrap();
        let simplified = invoice(InvoiceKind::Simplified);
        let g = generate(&simplified, 1, FIRST_PIH, Some(&stamp)).unwrap();
        let fields = qr::decode(&g.qr).unwrap();
        assert_eq!(fields.iter().map(|f| f.0).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(fields[5].1, g.hash.as_bytes(), "tag 6 is the invoice hash (Base64 text, as in the SDK sample)");
        let digest: [u8; 32] = STANDARD.decode(&g.hash).unwrap().try_into().unwrap();
        let sig = STANDARD.decode(&fields[6].1).unwrap();
        assert!(crate::sign::verify(&fields[7].1, &digest, &sig), "tag 7 verifies with tag 8");
        assert_eq!(fields[8].1, cert.signature(), "tag 9 is the certificate's signature");
        assert_eq!(g.hash, first(&simplified).hash, "stamping doesn't change the hash");
        assert_eq!(g.signing_time.as_deref(), Some("2026-05-01T07:15:30Z"));
        assert_eq!(g, generate(&simplified, 1, FIRST_PIH, Some(&stamp)).unwrap(), "deterministic signatures");
        let v = crate::xades::verify(&g.xml).unwrap();
        assert_eq!((v.invoice_hash.as_str(), v.signing_time.as_str()), (g.hash.as_str(), "2026-05-01T07:15:30Z"));
        let std = generate(&invoice(InvoiceKind::Standard), 1, FIRST_PIH, Some(&stamp)).unwrap();
        assert_eq!(qr::decode(&std.qr).unwrap().len(), 8, "tag 9 is for simplified documents only");
        crate::xades::verify(&std.xml).unwrap();
    }

    #[test]
    fn the_stamp_has_zatcas_structure() {
        use crate::xades::tests::{test_cert, test_signer, time};
        let (signer, cert) = (test_signer(), test_cert());
        let stamp = Stamp::new(&signer, &cert, time()).unwrap();
        let g = generate(&invoice(InvoiceKind::Simplified), 1, FIRST_PIH, Some(&stamp)).unwrap();
        let x = &g.xml;
        assert!(x.contains("xmlns:ext=\"urn:oasis:names:specification:ubl:schema:xsd:CommonExtensionComponents-2\"><ext:UBLExtensions>\n    <ext:UBLExtension>"));
        let at = |s: &str| x.find(s).unwrap_or_else(|| panic!("missing {s:?}"));
        for path in [
            "<ext:UBLExtensions>",
            "<ext:UBLExtension>",
            "<ext:ExtensionURI>urn:oasis:names:specification:ubl:dsig:enveloped:xades</ext:ExtensionURI>",
            "<ext:ExtensionContent>",
            "<sig:UBLDocumentSignatures ",
            "<sac:SignatureInformation>",
            "<cbc:ID>urn:oasis:names:specification:ubl:signature:1</cbc:ID>",
            "<sbc:ReferencedSignatureID>urn:oasis:names:specification:ubl:signature:Invoice</sbc:ReferencedSignatureID>",
            "<ds:Signature xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\" Id=\"signature\">",
            "<ds:SignedInfo>",
            "<ds:CanonicalizationMethod Algorithm=\"http://www.w3.org/2006/12/xml-c14n11\"/>",
            "<ds:SignatureMethod Algorithm=\"http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256\"/>",
            "<ds:Reference Id=\"invoiceSignedData\" URI=\"\">",
            "<ds:XPath>not(//ancestor-or-self::ext:UBLExtensions)</ds:XPath>",
            "<ds:XPath>not(//ancestor-or-self::cac:Signature)</ds:XPath>",
            "<ds:XPath>not(//ancestor-or-self::cac:AdditionalDocumentReference[cbc:ID='QR'])</ds:XPath>",
            &format!("<ds:DigestValue>{}</ds:DigestValue>", g.hash),
            "<ds:Reference Type=\"http://www.w3.org/2000/09/xmldsig#SignatureProperties\" URI=\"#xadesSignedProperties\">",
            "<ds:SignatureValue>",
            &format!("<ds:X509Certificate>{}</ds:X509Certificate>", cert.base64()),
            "<xades:QualifyingProperties xmlns:xades=\"http://uri.etsi.org/01903/v1.3.2#\" Target=\"signature\">",
            "<xades:SignedProperties Id=\"xadesSignedProperties\">",
            "<xades:SigningTime>2026-05-01T07:15:30Z</xades:SigningTime>",
            &format!("<ds:DigestValue>{}</ds:DigestValue>", cert.digest()),
            "<ds:X509IssuerName>CN=KhataOS Test CA, O=KhataOS</ds:X509IssuerName>",
            "<ds:X509SerialNumber>81985529216486895</ds:X509SerialNumber>",
            "</ext:UBLExtensions>\n    <cbc:ProfileID>",
        ] {
            at(path);
        }
        assert_eq!(x.matches("<ds:Reference ").count(), 2, "the invoice and the SignedProperties");
        assert!(at("<ds:SignedInfo>") < at("<ds:SignatureValue>") && at("<ds:SignatureValue>") < at("<ds:KeyInfo>") && at("<ds:KeyInfo>") < at("<ds:Object>"));
        // cac:Signature right after the QR reference, before the seller.
        assert!(x.contains("</cac:AdditionalDocumentReference><cac:Signature>\n      <cbc:ID>urn:oasis:names:specification:ubl:signature:Invoice</cbc:ID>"));
        assert!(at("<cac:Signature>") > at("<cbc:ID>QR</cbc:ID>") && at("<cac:Signature>") < at("<cac:AccountingSupplierParty>"));
        // The hashed text is exactly the unstamped document's.
        assert_eq!(hash_input(x).unwrap(), hash_input(&first(&invoice(InvoiceKind::Simplified)).xml).unwrap());
        // Tampering is caught.
        assert!(crate::xades::verify(&x.replace("Laptop", "Laptops")).is_err(), "content changed after signing");
        assert!(crate::xades::verify(&x.replace("2026-05-01T07:15:30Z</xades:SigningTime>", "2026-05-01T07:15:31Z</xades:SigningTime>")).is_err());
        // Another signing time, another stamp; same hash.
        let later = Stamp::new(&signer, &cert, time() + chrono::Duration::seconds(1)).unwrap();
        let g2 = generate(&invoice(InvoiceKind::Simplified), 1, FIRST_PIH, Some(&later)).unwrap();
        assert_eq!(g2.hash, g.hash);
        assert_ne!(g2.xml, g.xml);
    }

    #[test]
    fn the_chain_counts_up_and_links_hashes() {
        let mut chain = ChainState::default();
        let first = generate_next(&invoice(InvoiceKind::Simplified), &mut chain, None).unwrap();
        let mut second_inv = invoice(InvoiceKind::Standard);
        second_inv.number = "SINV/2026/2".into();
        second_inv.uuid = "6a1c3a9e-0d38-4b8f-9d0f-0e6c1d0c2b11".into();
        let second = generate_next(&second_inv, &mut chain, None).unwrap();
        assert_eq!((first.icv, first.pih.as_str()), (1, FIRST_PIH));
        assert_eq!((second.icv, second.pih.as_str()), (2, first.hash.as_str()), "one chain for B2C and B2B");
        assert_eq!(chain, ChainState { last_icv: 2, last_hash: second.hash.clone() });
        assert!(second.xml.contains("<cbc:UUID>2</cbc:UUID>"));
        assert!(second.xml.contains(&first.hash));
        // A rejected build doesn't move the chain.
        let mut bad = second_inv.clone();
        bad.lines.clear();
        assert!(generate_next(&bad, &mut chain, None).is_err());
        assert_eq!(chain.last_icv, 2);
    }
}

//! Invoice design (Company → Invoice design) and direct PDF export.
//!
//! A company picks how its invoices look ([`InvoiceDesign`]: template,
//! accent colour, font, paper, column toggles, bank details, terms, footer,
//! declaration, titles) and adds a logo and a signature image. Every print
//! and PDF of a voucher goes through [`prepare`], which also adds what the
//! country makes mandatory, whatever the design ([`title_rule`],
//! [`required`], the statutory notes of [`legal_notes`]). See
//! `docs/invoice-design.md`.
//!
//! **Storage.** The design, the images and the per-voucher print details
//! (ship-to, export shipping data) are the company's **shared records**
//! (`khata_store::records`), like the UK VAT schemes and the IRP records:
//! not audit entries (they aren't accounting data, and the trail's format
//! is unchanged), but synced with co-workers and the accountant on every
//! path the books take, `.khatapkg` files included, last writer wins
//! (`docs/sync-settings.md`).

use std::collections::HashMap;

use base64::Engine;
use chrono::NaiveDate;
use khata_core::format::Grouping;
use khata_core::{Book, UserRole, Voucher, VoucherId, VoucherKind};
use khata_print::pdf::{PdfDoc, PdfLabels};
use khata_print::{
    BankLabels, ExportDetails, FontChoice, Header, Image, ImageKind, InvoiceDesign, InvoiceDoc, InvoiceLayout, InvoiceLine, LegalNote,
    Look, Paper, PrintBlock, PrintLanguage, Required, ShipTo, Template,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::api::{ApiResult, Slot, err, open_mut, open_ref, parse_date, parse_uuid};

const DESIGN_KEY: &str = "invoice.design";
const LOGO_KEY: &str = "invoice.logo";
const SIGNATURE_KEY: &str = "invoice.signature";
const DETAILS_PREFIX: &str = "invoice.voucher.";
/// Most vouchers in one batch PDF.
pub const MAX_BATCH: usize = 500;

fn store_of(slot: &Slot) -> ApiResult<&khata_store::Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

// ------------------------------------------------------------ storage

/// An image as its shared record keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredImage {
    kind: ImageKind,
    /// Base64 of the file.
    data: String,
    /// Base64 of the PNG rendition of an SVG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    raster: Option<String>,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn read_design(store: &khata_store::Store) -> ApiResult<InvoiceDesign> {
    match store.record(DESIGN_KEY).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("the invoice design is damaged: {e}")),
        None => Ok(InvoiceDesign::default()),
    }
}

fn image_key(which: &str) -> ApiResult<&'static str> {
    match which {
        "logo" => Ok(LOGO_KEY),
        "signature" => Ok(SIGNATURE_KEY),
        _ => Err(format!("unknown invoice image {which:?}")),
    }
}

fn read_image(store: &khata_store::Store, key: &str) -> Option<Image> {
    let s: StoredImage = serde_json::from_str(&store.record(key).ok()??).ok()?;
    Some(Image { kind: s.kind, data: b64().decode(s.data).ok()?, raster: s.raster.and_then(|r| b64().decode(r).ok()) })
}

fn write_image(store: &khata_store::Store, key: &str, img: &Image) -> ApiResult<()> {
    let s = StoredImage { kind: img.kind, data: b64().encode(&img.data), raster: img.raster.as_ref().map(|r| b64().encode(r)) };
    store.set_record(key, Some(&serde_json::to_string(&s).map_err(err)?), &crate::api::os_user()).map(|_| ()).map_err(err)
}

/// The company's design and images; the plain design when none is set or
/// it can't be read (a print never fails over its design).
pub(crate) struct Branding {
    pub design: InvoiceDesign,
    pub logo: Option<Image>,
    pub signature: Option<Image>,
}

pub(crate) fn branding(slot: &Slot) -> Branding {
    let Ok(store) = store_of(slot) else { return Branding { design: InvoiceDesign::default(), logo: None, signature: None } };
    Branding { design: read_design(store).unwrap_or_default(), logo: read_image(store, LOGO_KEY), signature: read_image(store, SIGNATURE_KEY) }
}

/// What a voucher's print adds to the books: a ship-to address and the
/// shipping data of an export. Kept per voucher as a shared record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoucherPrintDetails {
    pub ship_to_name: String,
    pub ship_to_address: String,
    /// "" (decided by the party's country), "yes" or "no".
    pub export: String,
    /// India, exports and SEZ supplies: "" (from the voucher's IGST),
    /// "lut" (under LUT or bond, without IGST) or "igst" (with IGST).
    pub india_tax: String,
    /// The LUT ARN when it isn't the company's (Invoice design).
    pub lut_arn: String,
    pub port_of_loading: String,
    pub port_of_discharge: String,
    pub incoterms: String,
    pub destination_country: String,
    pub shipping_bill_no: String,
    /// YYYY-MM-DD.
    pub shipping_bill_date: String,
}

impl VoucherPrintDetails {
    fn validated(mut self) -> ApiResult<Self> {
        let one_line = |s: &mut String, what: &str, max: usize| -> ApiResult<()> {
            *s = s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_owned();
            if s.chars().count() > max { Err(format!("the {what} is too long")) } else { Ok(()) }
        };
        one_line(&mut self.ship_to_name, "ship-to name", 120)?;
        self.ship_to_address = self
            .ship_to_address
            .lines()
            .map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if self.ship_to_address.chars().count() > 400 {
            return Err("the ship-to address is too long".into());
        }
        for (s, what) in [
            (&mut self.lut_arn, "LUT ARN"),
            (&mut self.port_of_loading, "port of loading"),
            (&mut self.port_of_discharge, "port of discharge"),
            (&mut self.incoterms, "Incoterms"),
            (&mut self.destination_country, "country of destination"),
            (&mut self.shipping_bill_no, "shipping bill number"),
        ] {
            one_line(s, what, 80)?;
        }
        if !matches!(self.export.as_str(), "" | "yes" | "no") {
            return Err(format!("{:?} is not yes, no or blank", self.export));
        }
        if !matches!(self.india_tax.as_str(), "" | "lut" | "igst") {
            return Err(format!("{:?} is not lut, igst or blank", self.india_tax));
        }
        self.shipping_bill_date = self.shipping_bill_date.trim().to_owned();
        if !self.shipping_bill_date.is_empty() {
            parse_date(&self.shipping_bill_date)?;
        }
        Ok(self)
    }
}

fn read_details(store: &khata_store::Store, id: VoucherId) -> VoucherPrintDetails {
    store
        .record(&format!("{DETAILS_PREFIX}{}", id.0))
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

// ------------------------------------------------------------ the law

/// Whether a voucher type's printed title may be changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum TitleRule {
    /// Set by law: the design can't change it.
    Fixed { title: &'static str, reason: &'static str },
    /// One of a few titles the law names.
    Choice { choices: Vec<&'static str>, reason: &'static str },
    /// Any title.
    Free,
}

fn trade(kind: VoucherKind) -> bool {
    matches!(kind, VoucherKind::Sales | VoucherKind::CreditNote | VoucherKind::DebitNote)
}

/// The rule for a voucher type's title in a country.
pub fn title_rule(country: Option<&str>, kind: VoucherKind) -> TitleRule {
    use VoucherKind as K;
    let fixed = |title, reason| TitleRule::Fixed { title, reason };
    match (country.unwrap_or_default(), kind) {
        // ZATCA titles, in Arabic and English (VAT IR Art 53); a simplified
        // invoice's title comes from its e-invoice.
        ("SA", K::Sales) => fixed("Tax Invoice", "KSA: \"Tax Invoice\" or \"Simplified Tax Invoice\" with the Arabic (VAT IR Art 53)"),
        ("SA", K::CreditNote) => fixed("Credit Note", "KSA: the ZATCA document title, with the Arabic"),
        ("SA", K::DebitNote) => fixed("Debit Note", "KSA: the ZATCA document title, with the Arabic"),
        ("AE", K::Sales) => fixed("Tax Invoice", "UAE: the words \"Tax Invoice\" (VAT ER Art 59(1)(a))"),
        ("AE", K::CreditNote) => fixed("Tax Credit Note", "UAE: the words \"Tax Credit Note\" (VAT ER Art 60)"),
        ("SG", K::Sales) => fixed("Tax Invoice", "Singapore: the words \"Tax Invoice\" (GST Regulations reg. 11)"),
        ("IN", K::Sales) => TitleRule::Choice {
            choices: vec!["Tax Invoice", "Bill of Supply", "Invoice-cum-bill of supply"],
            reason: "India: a tax invoice (Rule 46), a bill of supply for exempt supplies or composition dealers (Rule 49), or an invoice-cum-bill of supply (Rule 46A)",
        },
        ("IN", K::CreditNote) => fixed("Credit Note", "India: Rule 53"),
        ("IN", K::DebitNote) => fixed("Debit Note", "India: Rule 53"),
        _ => TitleRule::Free,
    }
}

/// A voucher type as the settings and the frontend name it ("CreditNote").
fn kind_key(kind: VoucherKind) -> String {
    format!("{kind:?}")
}

/// The title a voucher prints with under the design and the country's rule.
pub fn effective_title(country: Option<&str>, design: &InvoiceDesign, kind: VoucherKind) -> String {
    let chosen = design.titles.get(&kind_key(kind)).map(|s| s.trim()).filter(|s| !s.is_empty());
    match title_rule(country, kind) {
        TitleRule::Fixed { title, .. } => title.to_owned(),
        TitleRule::Choice { choices, .. } => chosen
            .and_then(|c| choices.iter().find(|x| x.eq_ignore_ascii_case(c)))
            .map_or_else(|| crate::invoice::title(kind).to_owned(), |c| (*c).to_owned()),
        TitleRule::Free => chosen.map_or_else(|| crate::invoice::title(kind).to_owned(), str::to_owned),
    }
}

/// What the design can't hide on a country's invoices and notes.
pub fn required(country: Option<&str>, kind: VoucherKind) -> Required {
    if !trade(kind) {
        return Required::default();
    }
    let c = country.unwrap_or_default();
    Required {
        // HSN/SAC (India, Rule 46(g)); the VAT rate or category of each line
        // (UAE ER Art 59(1)(h), KSA IR Art 53, UK Notice 700/21 §4.3, EU VAT
        // Directive Art 226(9)).
        tax_class: matches!(c, "IN" | "AE" | "SA" | "GB" | "DE" | "FR" | "NL" | "ES"),
        // Quantity and unit price: India Rule 46(i); UAE ER 59(1)(h); KSA IR
        // Art 53; UK Notice 700/21; EU Art 226(6), (8); Mexico's CFDI.
        qty_rate: matches!(c, "IN" | "AE" | "SA" | "GB" | "DE" | "FR" | "NL" | "ES" | "MX"),
        // India Rule 46(q): signature of the supplier on a paper invoice.
        signatory: c == "IN",
    }
}

/// Rule 46 (first proviso) endorsement, with payment of IGST.
pub const IN_EXPORT_WITH_IGST: &str =
    "SUPPLY MEANT FOR EXPORT/SUPPLY TO SEZ UNIT OR SEZ DEVELOPER FOR AUTHORISED OPERATIONS ON PAYMENT OF INTEGRATED TAX";
/// Rule 46 (first proviso) endorsement, under LUT or bond.
pub const IN_EXPORT_UNDER_LUT: &str = "SUPPLY MEANT FOR EXPORT/SUPPLY TO SEZ UNIT OR SEZ DEVELOPER FOR AUTHORISED OPERATIONS UNDER BOND OR LETTER OF UNDERTAKING WITHOUT PAYMENT OF INTEGRATED TAX";

/// The zero-rating statement of an export in the GCC, the UK and Singapore.
fn zero_rated_note(country: &str) -> Option<LegalNote> {
    Some(match country {
        "AE" | "SA" => LegalNote { text: "Zero-rated supply: export".into(), arabic: Some("توريد خاضع للضريبة بنسبة الصفر: تصدير".into()) },
        "GB" => LegalNote::en("Zero-rated export (0% VAT)"),
        "SG" => LegalNote::en("Zero-rated supply: export (0% GST)"),
        _ => return None,
    })
}

/// Whether the voucher's party is abroad (its mailing country isn't the company's).
fn party_abroad(book: &Book, party: Option<&khata_core::Ledger>) -> bool {
    let ours = book.country().unwrap_or_default();
    party
        .and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .map(str::trim)
        .is_some_and(|c| !c.is_empty() && !c.eq_ignore_ascii_case(ours))
}

/// Whether an export or SEZ voucher of an Indian company charges IGST.
fn charges_igst(book: &Book, v: &Voucher) -> bool {
    v.entries.iter().any(|e| {
        !e.amount.is_zero() && book.ledger(e.ledger).and_then(|l| l.tax.as_ref()).is_some_and(|t| t.component.eq_ignore_ascii_case("IGST"))
    })
}

/// The statutory statements of a sale: India's export / SEZ endorsement
/// (with the LUT ARN), or the zero-rating of an export elsewhere.
pub fn legal_notes(book: &Book, v: &Voucher, d: &VoucherPrintDetails, design: &InvoiceDesign) -> Vec<LegalNote> {
    if v.kind != VoucherKind::Sales {
        return Vec::new();
    }
    let party = crate::invoice::party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger));
    let export = match d.export.as_str() {
        "yes" => true,
        "no" => false,
        _ => party_abroad(book, party),
    };
    let country = book.country().unwrap_or_default();
    if country == "IN" {
        let sez = party.and_then(|l| l.mailing.as_ref()).is_some_and(|m| m.special_zone);
        if !(export || sez) {
            return Vec::new();
        }
        let with_igst = match d.india_tax.as_str() {
            "igst" => true,
            "lut" => false,
            _ => charges_igst(book, v),
        };
        if with_igst {
            return vec![LegalNote::en(IN_EXPORT_WITH_IGST)];
        }
        let mut notes = vec![LegalNote::en(IN_EXPORT_UNDER_LUT)];
        let arn = if d.lut_arn.trim().is_empty() { design.lut_arn.trim() } else { d.lut_arn.trim() };
        if !arn.is_empty() {
            notes.push(LegalNote::en(format!("LUT ARN: {arn}")));
        }
        return notes;
    }
    if export { zero_rated_note(country).into_iter().collect() } else { Vec::new() }
}

// ------------------------------------------------------------ preparing a print

/// A voucher ready to print or export: the document and how it looks.
pub(crate) struct Prepared {
    pub doc: InvoiceDoc,
    pub layout: InvoiceLayout,
    pub paper: Paper,
    pub required: Required,
    pub trade: bool,
    /// The IRN or CFDI block of the HTML print (spliced above the party boxes).
    pub html_block: Option<String>,
}

/// Whether a document is a B2C one for the receipt paper: a sale to a buyer
/// with no tax ID (or a KSA simplified invoice).
fn b2c(doc: &InvoiceDoc, kind: VoucherKind) -> bool {
    kind == VoucherKind::Sales && (doc.buyer_tax.is_none() || doc.title.eq_ignore_ascii_case("Simplified Tax Invoice"))
}

fn paper_for(design: &InvoiceDesign, doc: &InvoiceDoc, kind: VoucherKind) -> Paper {
    if design.receipt_for_b2c && b2c(doc, kind) { Paper::Thermal80 } else { design.paper }
}

fn grouping(book: &Book) -> Grouping {
    if book.country() == Some("IN") { Grouping::Indian } else { Grouping::Western }
}

/// Build the print of a saved voucher: the document with its title, ship-to,
/// export data and statutory notes, the tax-authority blocks and QR codes,
/// and the layout the country needs.
pub(crate) fn prepare(slot: &Slot, book: &Book, v: &Voucher, lang: PrintLanguage, hijri: Option<&str>, design: &InvoiceDesign) -> Prepared {
    let country = book.country();
    let mut doc = crate::invoice::invoice_doc(book, v);
    doc.title = effective_title(country, design, v.kind);
    doc.hijri = hijri.filter(|s| !s.trim().is_empty()).map(str::to_owned);
    let details = store_of(slot).map(|s| read_details(s, v.id)).unwrap_or_default();
    // Bill-to: the party's mailing address.
    if let Some(l) = crate::invoice::party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger))
        && let Some(m) = &l.mailing
    {
        let mut lines: Vec<String> = m.address.lines().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
        if !m.postal_code.trim().is_empty() {
            lines.push(m.postal_code.trim().to_owned());
        }
        if let Some(c) = m.country.as_deref().filter(|c| !c.trim().is_empty() && Some(c.trim()) != country) {
            lines.push(c.trim().to_owned());
        }
        doc.buyer_address = lines.join("\n");
    }
    if !details.ship_to_name.is_empty() || !details.ship_to_address.is_empty() {
        doc.ship_to = Some(ShipTo { name: details.ship_to_name.clone(), address: details.ship_to_address.clone() });
    }
    let export = ExportDetails {
        port_of_loading: details.port_of_loading.clone(),
        port_of_discharge: details.port_of_discharge.clone(),
        incoterms: details.incoterms.clone(),
        destination_country: details.destination_country.clone(),
        shipping_bill_no: details.shipping_bill_no.clone(),
        shipping_bill_date: NaiveDate::parse_from_str(&details.shipping_bill_date, "%Y-%m-%d").ok(),
    };
    if !export.is_empty() {
        doc.export = Some(export);
    }
    doc.legal_notes = legal_notes(book, v, &details, design);

    let gulf = matches!(country, Some("SA") | Some("AE"));
    let layout = if gulf && trade(v.kind) {
        InvoiceLayout::Bilingual
    } else if lang == PrintLanguage::Arabic {
        InvoiceLayout::Arabic
    } else {
        InvoiceLayout::English
    };
    words_for_layout(book, &mut doc, layout);
    let mut html_block = None;
    match country {
        Some("SA") if v.kind == VoucherKind::Sales => {
            // A KSA invoice with a generated e-invoice prints its title and QR.
            if let Some((title, qr)) = crate::zatca::print_extras(slot, v.id) {
                doc.title = title.to_owned();
                doc.seller_qr = Some(qr);
            }
        }
        Some("MX") if matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote | VoucherKind::Receipt) => {
            // A CFDI stamped by a PAC prints its folio fiscal, TFD data and QR.
            html_block = crate::pac_client::print_block_html(slot, v.id);
            if let Some(p) = crate::pac_client::print_info(slot, v.id) {
                let mut flags = Vec::new();
                if p.test {
                    flags.push("SIN VALIDEZ FISCAL (PAC simulado)".to_owned());
                }
                if p.cancelled {
                    flags.push("CFDI CANCELADO".to_owned());
                }
                let row = |l: &str, v: &str| (l.to_owned(), v.to_owned());
                doc.blocks.push(PrintBlock {
                    flags,
                    rows: vec![
                        row("Folio fiscal (UUID)", &p.uuid),
                        row("Fecha de certificación", &p.fecha_timbrado),
                        row("No. de serie del CSD", &p.no_certificado),
                        row("No. de serie del certificado del SAT", &p.no_certificado_sat),
                        row("Sello digital del CFDI", &p.sello_cfd),
                        row("Sello del SAT", &p.sello_sat),
                        row("Cadena original del complemento de certificación digital del SAT", &p.tfd_cadena),
                    ],
                    note: "Este documento es una representación impresa de un CFDI.".into(),
                    qr: p.qr,
                });
            }
        }
        Some("IN") if matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote) => {
            // An e-invoice registered on the IRP prints its IRN and signed QR.
            html_block = crate::irp_client::print_block_html(slot, v.id);
            if let Some(p) = crate::irp_client::print_info(slot, v.id) {
                doc.blocks.push(PrintBlock {
                    flags: if p.cancelled { vec!["IRN CANCELLED".to_owned()] } else { Vec::new() },
                    rows: vec![("IRN".into(), p.irn), ("Ack No".into(), p.ack_no), ("Ack Date".into(), p.ack_dt)],
                    note: String::new(),
                    qr: p.qr,
                });
            }
        }
        _ => {}
    }
    let paper = paper_for(design, &doc, v.kind);
    Prepared { required: required(country, v.kind), trade: trade(v.kind), doc, layout, paper, html_block }
}

/// The amount in words for the print's layout: Arabic tafqit on an Arabic
/// print, and under the English on a bilingual KSA/UAE invoice. In the
/// invoice's own currency (a foreign-currency invoice's, else the base).
fn words_for_layout(book: &Book, doc: &mut InvoiceDoc, layout: InvoiceLayout) {
    if !matches!(layout, InvoiceLayout::Arabic | InvoiceLayout::Bilingual) {
        return;
    }
    let currency = doc.currency.as_ref().map(|c| c.code.as_str());
    let arabic = crate::invoice::words(book, doc.total, currency, true);
    if layout == InvoiceLayout::Arabic {
        doc.total_in_words = arabic;
    } else {
        doc.total_in_words_ar = Some(arabic);
    }
}

fn look<'a>(p: &Prepared, b: &'a Branding, country: Option<&str>) -> Look<'a> {
    Look {
        design: &b.design,
        logo: b.logo.as_ref(),
        signature: b.signature.as_ref(),
        required: p.required,
        paper: p.paper,
        bank_labels: BankLabels::for_country(country.unwrap_or_default()),
        trade_document: p.trade,
    }
}

fn header_parts(book: &Book) -> (String, Grouping) {
    (book.profile().map(|p| p.address.clone()).unwrap_or_default(), grouping(book))
}

/// The HTML of a prepared print, with its IRN / CFDI block.
fn html_of(book: &Book, p: &Prepared, b: &Branding) -> String {
    let (address, grouping) = header_parts(book);
    let h = Header { company: &book.name, address: &address, grouping };
    let html = khata_print::invoice_html_styled(&h, &p.doc, p.layout, &look(p, b, book.country()));
    match &p.html_block {
        Some(block) => crate::irp_client::splice_print_block(html, block),
        None => html,
    }
}

/// Print-ready HTML of a saved voucher in the company's design.
pub fn voucher_html(slot: &Slot, id: &str, lang: PrintLanguage, hijri: Option<&str>) -> ApiResult<String> {
    let book = open_ref(slot)?;
    let v = book.voucher(VoucherId(parse_uuid(id, "voucher")?)).ok_or("unknown voucher")?;
    let b = branding(slot);
    let p = prepare(slot, book, v, lang, hijri, &b.design);
    Ok(html_of(book, &p, &b))
}

fn pdf_labels(layout: InvoiceLayout) -> PdfLabels {
    match layout {
        InvoiceLayout::English => PdfLabels::English,
        InvoiceLayout::Bilingual => PdfLabels::Bilingual,
        // A right-to-left page; digits stay Latin, as in the app (docs/i18n.md).
        InvoiceLayout::Arabic => PdfLabels::Arabic { arabic_indic_digits: false },
    }
}

/// A PDF to save with `write_binary_file`.
#[derive(Debug, Clone, Serialize)]
pub struct PdfDto {
    pub file_name: String,
    pub pdf_base64: String,
    /// Vouchers in the file.
    pub count: usize,
}

/// A voucher number or title as part of a file name.
fn file_safe(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '.' | '_') { c } else { '-' }).collect();
    out.trim_matches(['-', ' ', '.']).to_owned()
}

fn pdf_of(book: &Book, prepared: &[Prepared], b: &Branding) -> Vec<u8> {
    let (address, grouping) = header_parts(book);
    let h = Header { company: &book.name, address: &address, grouping };
    let docs: Vec<PdfDoc> = prepared.iter().map(|p| PdfDoc { doc: &p.doc, look: look(p, b, book.country()), labels: pdf_labels(p.layout) }).collect();
    khata_print::pdf::invoices_pdf(&h, &docs)
}

/// A saved voucher as a PDF in the company's design.
pub fn voucher_pdf(slot: &Slot, id: &str, lang: PrintLanguage, hijri: Option<&str>) -> ApiResult<PdfDto> {
    let book = open_ref(slot)?;
    let v = book.voucher(VoucherId(parse_uuid(id, "voucher")?)).ok_or("unknown voucher")?;
    let b = branding(slot);
    let p = prepare(slot, book, v, lang, hijri, &b.design);
    let name = format!("{} {}.pdf", file_safe(&p.doc.title), file_safe(&v.number));
    let pdf = pdf_of(book, std::slice::from_ref(&p), &b);
    Ok(PdfDto { file_name: name, pdf_base64: b64().encode(pdf), count: 1 })
}

/// Several vouchers in one PDF, each from a new page: the ones listed, or
/// (when `ids` is empty) every uncancelled Sales, Credit Note and Debit
/// Note from `from` to `to`, in date order. `hijri` maps ISO dates
/// ("2026-05-02") to the Hijri date the caller formatted, printed under
/// each voucher's date (Rust has no Hijri calendar).
pub fn vouchers_pdf(slot: &Slot, ids: &[String], from: &str, to: &str, lang: PrintLanguage, hijri: Option<&HashMap<String, String>>) -> ApiResult<PdfDto> {
    let book = open_ref(slot)?;
    let mut vouchers: Vec<&Voucher> = if ids.is_empty() {
        let (from, to) = (parse_date(from)?, parse_date(to)?);
        if to < from {
            return Err("the period ends before it starts".into());
        }
        book.vouchers().filter(|v| trade(v.kind) && !v.cancelled && v.date >= from && v.date <= to).collect()
    } else {
        ids.iter()
            .map(|id| book.voucher(VoucherId(parse_uuid(id, "voucher")?)).ok_or_else(|| "unknown voucher".to_owned()))
            .collect::<ApiResult<_>>()?
    };
    if vouchers.is_empty() {
        return Err("there are no invoices or notes in that period".into());
    }
    if vouchers.len() > MAX_BATCH {
        return Err(format!("that is {} vouchers; export at most {MAX_BATCH} at a time (pick a shorter period)", vouchers.len()));
    }
    vouchers.sort_by(|a, b| (a.date, a.kind as u8, &a.number).cmp(&(b.date, b.kind as u8, &b.number)));
    let b = branding(slot);
    let hijri_of = |v: &Voucher| hijri.and_then(|m| m.get(&v.date.format("%Y-%m-%d").to_string())).map(String::as_str);
    let prepared: Vec<Prepared> = vouchers.iter().map(|v| prepare(slot, book, v, lang, hijri_of(v), &b.design)).collect();
    let name = if ids.is_empty() { format!("Invoices {from} to {to}.pdf") } else { format!("Vouchers ({}).pdf", vouchers.len()) };
    let pdf = pdf_of(book, &prepared, &b);
    Ok(PdfDto { file_name: name, pdf_base64: b64().encode(pdf), count: vouchers.len() })
}

// ------------------------------------------------------------ the design screen

/// A voucher type's title on the design screen.
#[derive(Debug, Clone, Serialize)]
pub struct TitleDto {
    /// "Sales", "CreditNote"…
    pub kind: String,
    /// The title printed today.
    pub title: String,
    /// The title without a choice.
    pub default: &'static str,
    #[serde(flatten)]
    pub rule: TitleRule,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageDto {
    pub kind: ImageKind,
    /// For the screen's own `<img>`.
    pub data_url: String,
    /// An SVG without its PNG rendition yet (the screen makes one).
    pub needs_raster: bool,
    pub bytes: usize,
}

fn image_dto(img: Option<Image>) -> Option<ImageDto> {
    img.map(|i| ImageDto { kind: i.kind, data_url: i.data_url(), needs_raster: i.kind == ImageKind::Svg && i.raster.is_none(), bytes: i.data.len() })
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub value: String,
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvoiceDesignDto {
    pub design: InvoiceDesign,
    pub logo: Option<ImageDto>,
    pub signature: Option<ImageDto>,
    pub country: Option<String>,
    pub bank_labels: BankLabels,
    pub titles: Vec<TitleDto>,
    pub templates: Vec<Choice>,
    pub papers: Vec<Choice>,
    pub fonts: Vec<Choice>,
    /// The design's toggles that the country keeps on anyway.
    pub required: RequiredDto,
    /// Whether this user may change the design (Owner).
    pub can_edit: bool,
    pub max_image_kb: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequiredDto {
    pub tax_class: bool,
    pub qty_rate: bool,
    pub signatory: bool,
}

fn variant_name<T: Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

pub fn design_dto(slot: &Slot) -> ApiResult<InvoiceDesignDto> {
    let book = open_ref(slot)?;
    let store = store_of(slot)?;
    let design = read_design(store)?;
    let country = book.country();
    let titles = [VoucherKind::Sales, VoucherKind::CreditNote, VoucherKind::DebitNote, VoucherKind::Purchase, VoucherKind::Receipt, VoucherKind::Payment]
        .into_iter()
        .map(|k| TitleDto { kind: kind_key(k), title: effective_title(country, &design, k), default: crate::invoice::title(k), rule: title_rule(country, k) })
        .collect();
    let r = required(country, VoucherKind::Sales);
    Ok(InvoiceDesignDto {
        logo: image_dto(read_image(store, LOGO_KEY)),
        signature: image_dto(read_image(store, SIGNATURE_KEY)),
        country: country.map(str::to_owned),
        bank_labels: BankLabels::for_country(country.unwrap_or_default()),
        titles,
        templates: Template::ALL.iter().map(|t| Choice { value: variant_name(t), label: t.name() }).collect(),
        papers: Paper::ALL.iter().filter(|p| **p != Paper::Thermal80).map(|p| Choice { value: variant_name(p), label: p.name() }).collect(),
        fonts: FontChoice::ALL.iter().map(|f| Choice { value: variant_name(f), label: f.name() }).collect(),
        required: RequiredDto { tax_class: r.tax_class, qty_rate: r.qty_rate, signatory: r.signatory },
        can_edit: crate::api::require(book, UserRole::Owner).is_ok(),
        max_image_kb: khata_print::images::MAX_IMAGE_BYTES / 1024,
        design,
    })
}

/// Check a design against the country: titles the law fixes can't be set,
/// a choice must be one of the law's.
fn check_design(country: Option<&str>, design: InvoiceDesign) -> ApiResult<InvoiceDesign> {
    let mut d = design.validated()?;
    for (key, value) in d.titles.clone() {
        let kind: VoucherKind = serde_json::from_value(serde_json::Value::String(key.clone())).map_err(|_| format!("unknown voucher type {key:?}"))?;
        match title_rule(country, kind) {
            TitleRule::Fixed { title, reason } => {
                if !value.eq_ignore_ascii_case(title) {
                    return Err(format!("the title of a {} can't be changed: {reason}", crate::invoice::title(kind)));
                }
                d.titles.remove(&key);
            }
            TitleRule::Choice { choices, reason } => match choices.iter().find(|c| c.eq_ignore_ascii_case(&value)) {
                Some(c) => {
                    d.titles.insert(key, (*c).to_owned());
                }
                None => return Err(format!("{value:?} isn't allowed: {reason}")),
            },
            TitleRule::Free => {}
        }
    }
    Ok(d)
}

/// Save the design (owner only).
pub fn set_design(slot: &mut Slot, design: InvoiceDesign) -> ApiResult<InvoiceDesignDto> {
    {
        let (store, book) = open_mut(slot)?;
        crate::api::require(book, UserRole::Owner)?;
        let d = check_design(book.country(), design)?;
        store.set_record(DESIGN_KEY, Some(&serde_json::to_string(&d).map_err(err)?), &crate::api::os_user()).map_err(err)?;
    }
    design_dto(slot)
}

/// Add or replace the logo or signature from an image file's bytes.
pub fn set_image(slot: &mut Slot, which: &str, bytes: &[u8]) -> ApiResult<InvoiceDesignDto> {
    let key = image_key(which)?;
    {
        let (store, book) = open_mut(slot)?;
        crate::api::require(book, UserRole::Owner)?;
        let img = khata_print::images::check_image(bytes)?;
        write_image(store, key, &img)?;
    }
    design_dto(slot)
}

/// The PNG rendition of an SVG logo or signature (made by the webview).
pub fn set_image_raster(slot: &mut Slot, which: &str, png_base64: &str) -> ApiResult<InvoiceDesignDto> {
    let key = image_key(which)?;
    {
        let (store, book) = open_mut(slot)?;
        crate::api::require(book, UserRole::Owner)?;
        let png = b64().decode(png_base64.trim().trim_start_matches("data:image/png;base64,")).map_err(|e| format!("the rendition isn't base64: {e}"))?;
        khata_print::images::check_raster(&png)?;
        let mut img = read_image(store, key).ok_or("add the image first")?;
        if img.kind != ImageKind::Svg {
            return Err("only SVG images need a rendition".into());
        }
        img.raster = Some(png);
        write_image(store, key, &img)?;
    }
    design_dto(slot)
}

pub fn remove_image(slot: &mut Slot, which: &str) -> ApiResult<InvoiceDesignDto> {
    let key = image_key(which)?;
    {
        let (store, book) = open_mut(slot)?;
        crate::api::require(book, UserRole::Owner)?;
        store.set_record(key, None, &crate::api::os_user()).map_err(err)?;
    }
    design_dto(slot)
}

/// A sample invoice for the preview when the company has no sale yet.
fn sample(book: &Book, design: &InvoiceDesign) -> InvoiceDoc {
    use rust_decimal::Decimal;
    let country = book.country();
    let label = crate::api::pack(book).map(|p| p.tax_id_label().to_owned());
    let (class, tax) = match country {
        Some("IN") => ("847130", "Output IGST"),
        Some("US") => ("", "Sales tax"),
        _ => ("STANDARD", "Output VAT"),
    };
    let line = |d: &str, q: i64, r: i64| InvoiceLine {
        description: d.into(),
        tax_class: (!class.is_empty()).then(|| class.to_owned()),
        qty: Some(format!("{q} Nos")),
        rate: Some(Decimal::from(r)),
        amount: Decimal::from(q * r),
    };
    let lines = vec![line("Sample item", 2, 1500), line("Another item", 1, 750)];
    let taxable: Decimal = lines.iter().map(|l| l.amount).sum();
    // The sample charges the country's usual standard rate.
    let percent = match country {
        Some("IN") => 18,
        Some("AE") => 5,
        Some("SA") => 15,
        Some("MX") => 16,
        Some("SG") => 9,
        Some("DE") => 19,
        Some("NL") | Some("ES") => 21,
        Some("US") => 8,
        _ => 20,
    };
    let tax_amount = (taxable * Decimal::new(percent, 2)).round_dp(2);
    let total = taxable + tax_amount;
    InvoiceDoc {
        title: effective_title(country, design, VoucherKind::Sales),
        number: "SAMPLE/1".into(),
        date: crate::api::today(),
        seller_tax: label.clone().zip(book.profile().and_then(|p| p.tax_id.clone())),
        buyer_name: "Sample Customer".into(),
        buyer_address: "1 Example Street\nSample City".into(),
        lines,
        taxes: vec![(tax.into(), tax_amount)],
        total,
        total_in_words: crate::invoice::words(book, total, None, false),
        ..Default::default()
    }
}

/// The preview of an unsaved design: the voucher given, else the latest
/// sale, else a sample. `receipt` previews a B2C sale on the receipt paper
/// (when the design prints those on receipts).
pub fn preview(slot: &Slot, design: InvoiceDesign, voucher_id: Option<&str>, receipt: bool, lang: PrintLanguage) -> ApiResult<String> {
    let book = open_ref(slot)?;
    let design = check_design(book.country(), design).unwrap_or_else(|_| InvoiceDesign::default());
    let stored = branding(slot);
    let b = Branding { design, logo: stored.logo, signature: stored.signature };
    let v = match voucher_id.filter(|s| !s.trim().is_empty()) {
        Some(id) => Some(book.voucher(VoucherId(parse_uuid(id, "voucher")?)).ok_or("unknown voucher")?),
        None => book.vouchers().filter(|v| v.kind == VoucherKind::Sales && !v.cancelled).max_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number))),
    };
    let mut p = match v {
        Some(v) => prepare(slot, book, v, lang, None, &b.design),
        None => {
            let mut doc = sample(book, &b.design);
            let gulf = matches!(book.country(), Some("SA") | Some("AE"));
            let layout = if gulf { InvoiceLayout::Bilingual } else if lang == PrintLanguage::Arabic { InvoiceLayout::Arabic } else { InvoiceLayout::English };
            words_for_layout(book, &mut doc, layout);
            Prepared {
                paper: b.design.paper,
                layout,
                required: required(book.country(), VoucherKind::Sales),
                trade: true,
                doc,
                html_block: None,
            }
        }
    };
    p.paper = if receipt && b.design.receipt_for_b2c { Paper::Thermal80 } else if p.paper == Paper::Thermal80 && !receipt { b.design.paper } else { p.paper };
    Ok(html_of(book, &p, &b))
}

// ------------------------------------------------------------ voucher print details

#[derive(Debug, Clone, Serialize)]
pub struct VoucherPrintDetailsDto {
    pub details: VoucherPrintDetails,
    /// What a blank "export" means for this voucher: the party is abroad.
    pub party_abroad: bool,
    /// India: the voucher charges IGST (blank "india_tax" means "igst").
    pub charges_igst: bool,
    pub country: Option<String>,
}

pub fn voucher_details(slot: &Slot, voucher_id: &str) -> ApiResult<VoucherPrintDetailsDto> {
    let book = open_ref(slot)?;
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    let party = crate::invoice::party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger));
    Ok(VoucherPrintDetailsDto {
        details: read_details(store_of(slot)?, v.id),
        party_abroad: party_abroad(book, party),
        charges_igst: charges_igst(book, v),
        country: book.country().map(str::to_owned),
    })
}

/// Save a voucher's ship-to and export details (Data Entry and up). Nothing
/// in the books changes.
pub fn set_voucher_details(slot: &mut Slot, voucher_id: &str, details: VoucherPrintDetails) -> ApiResult<VoucherPrintDetailsDto> {
    {
        let (store, book) = open_mut(slot)?;
        let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
        let d = details.validated()?;
        let key = format!("{DETAILS_PREFIX}{}", v.id.0);
        let value = (d != VoucherPrintDetails::default()).then(|| serde_json::to_string(&d)).transpose().map_err(err)?;
        store.set_record(&key, value.as_deref(), &crate::api::os_user()).map_err(err)?;
    }
    voucher_details(slot, voucher_id)
}

// ------------------------------------------------------------ commands

type AppState<'a> = State<'a, crate::AppState>;

fn with<T>(state: &AppState, f: impl FnOnce(&mut Slot) -> ApiResult<T>) -> ApiResult<T> {
    let mut slot = state.company.lock().map_err(|_| "company state is poisoned".to_owned())?;
    f(&mut slot)
}

#[tauri::command(rename_all = "snake_case")]
pub fn invoice_design(state: AppState) -> ApiResult<InvoiceDesignDto> {
    with(&state, |s| design_dto(s))
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_invoice_design(state: AppState, design: InvoiceDesign) -> ApiResult<InvoiceDesignDto> {
    with(&state, |s| set_design(s, design))
}

/// Add the logo or signature (`which`) from a file the user picked: PNG,
/// JPEG or SVG of 300 KB or less.
#[tauri::command(rename_all = "snake_case")]
pub fn import_invoice_image(app: tauri::AppHandle, state: AppState, which: String, path: String) -> ApiResult<InvoiceDesignDto> {
    let limit = khata_print::images::MAX_IMAGE_BYTES as u64;
    let local = crate::phone_files::local_input(&app, &path, &["png", "jpg", "jpeg", "svg"], limit + 1)?;
    let ext_ok = local.extension().and_then(|e| e.to_str()).is_some_and(|e| ["png", "jpg", "jpeg", "svg"].contains(&e.to_ascii_lowercase().as_str()));
    if !ext_ok {
        return Err("pick a .png, .jpg or .svg image".into());
    }
    let meta = std::fs::metadata(&local).map_err(|e| format!("cannot read {}: {e}", local.display()))?;
    if meta.len() > limit {
        return Err(format!("the image is {} KB; use one of {} KB or less", meta.len().div_ceil(1024), limit / 1024));
    }
    let bytes = std::fs::read(&local).map_err(|e| format!("cannot read {}: {e}", local.display()))?;
    with(&state, |s| set_image(s, &which, &bytes))
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_invoice_image_raster(state: AppState, which: String, png_base64: String) -> ApiResult<InvoiceDesignDto> {
    with(&state, |s| set_image_raster(s, &which, &png_base64))
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_invoice_image(state: AppState, which: String) -> ApiResult<InvoiceDesignDto> {
    with(&state, |s| remove_image(s, &which))
}

#[tauri::command(rename_all = "snake_case")]
pub fn invoice_design_preview(
    state: AppState,
    design: InvoiceDesign,
    voucher_id: Option<String>,
    receipt: bool,
    language: Option<String>,
) -> ApiResult<String> {
    let lang = PrintLanguage::from_code(language.as_deref());
    with(&state, |s| preview(s, design, voucher_id.as_deref(), receipt, lang))
}

#[tauri::command(rename_all = "snake_case")]
pub fn voucher_print_details(state: AppState, voucher_id: String) -> ApiResult<VoucherPrintDetailsDto> {
    with(&state, |s| voucher_details(s, &voucher_id))
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_voucher_print_details(state: AppState, voucher_id: String, details: VoucherPrintDetails) -> ApiResult<VoucherPrintDetailsDto> {
    with(&state, |s| set_voucher_details(s, &voucher_id, details))
}

/// A saved voucher as a PDF (save `pdf_base64` with `write_binary_file`).
#[tauri::command(rename_all = "snake_case")]
pub fn export_voucher_pdf(state: AppState, id: String, language: Option<String>, hijri: Option<String>) -> ApiResult<PdfDto> {
    let lang = PrintLanguage::from_code(language.as_deref());
    with(&state, |s| voucher_pdf(s, &id, lang, hijri.as_deref()))
}

/// Several vouchers (or a period's invoices and notes) in one PDF; `hijri`:
/// ISO date → Hijri date, when the user shows Hijri dates.
#[tauri::command(rename_all = "snake_case")]
pub fn export_vouchers_pdf(
    state: AppState,
    ids: Vec<String>,
    from: String,
    to: String,
    language: Option<String>,
    hijri: Option<HashMap<String, String>>,
) -> ApiResult<PdfDto> {
    let lang = PrintLanguage::from_code(language.as_deref());
    with(&state, |s| vouchers_pdf(s, &ids, &from, &to, lang, hijri.as_ref()))
}

// ------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use khata_core::{DrCr, TaxSide};

    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, ledger, voucher};
    use crate::api::{StockLineInput, create_company, create_party_ledger, post_voucher, set_country};
    use crate::masters::{LedgerInput, StockItemInput, alter_ledger, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax, set_tax_rate};

    fn company(code: &str, name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Co").unwrap();
        set_country(&mut slot, Some(&f.0), code).unwrap();
        (f, slot)
    }

    fn item(slot: &mut Slot, name: &str, class: &str) -> String {
        let nos = match save_unit(slot, "", "Nos", "", 0) {
            Ok(u) => u.id,
            Err(_) => crate::masters::list_units(slot).unwrap().into_iter().find(|u| u.symbol == "Nos").unwrap().id,
        };
        let input: StockItemInput = serde_json::from_value(serde_json::json!({ "name": name, "unit_id": nos, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    fn party(slot: &mut Slot, name: &str, tax_id: Option<&str>, mailing: serde_json::Value) -> String {
        let debtors = group_id(slot, "Sundry Debtors");
        let id = create_party_ledger(slot, name, &debtors, "", None, tax_id, None).unwrap().ledger.id;
        let mut input = serde_json::json!({ "name": name, "group_id": debtors, "mailing": mailing });
        if let Some(t) = tax_id {
            input["tax_id"] = t.into();
        }
        let input: LedgerInput = serde_json::from_value(input).unwrap();
        alter_ledger(slot, &id, &input).unwrap();
        id
    }

    /// A sale of `qty` × `rate` of an item, with an optional tax line.
    fn sale(slot: &mut Slot, kind: &str, buyer: &str, item_id: &str, qty: &str, rate: &str, tax: Option<(&str, &str)>) -> String {
        let sales = ledger(slot, &format!("Sales {}", uuid::Uuid::new_v4()), "Sales Accounts", "");
        let value: rust_decimal::Decimal = qty.parse::<rust_decimal::Decimal>().unwrap() * rate.parse::<rust_decimal::Decimal>().unwrap();
        let tax_amount: rust_decimal::Decimal = tax.map_or(rust_decimal::Decimal::ZERO, |(_, a)| a.parse().unwrap());
        let (party_side, line_side) = if kind == "CreditNote" { (DrCr::Cr, DrCr::Dr) } else { (DrCr::Dr, DrCr::Cr) };
        let mut line = entry(&sales, line_side, &value.to_string());
        line.items = vec![StockLineInput { item_id: item_id.into(), qty: qty.into(), rate: rate.into(), amount: String::new(), godown_id: String::new(), batch: String::new() }];
        let mut entries = vec![entry(buyer, party_side, &(value + tax_amount).to_string()), line];
        if let Some((l, a)) = tax {
            entries.push(entry(l, line_side, a));
        }
        post_voucher(slot, voucher(kind, "2026-05-02", entries)).unwrap().id
    }

    /// One print of a voucher under a design: its HTML, its PDF's text and QR codes.
    struct Printed {
        what: String,
        html: String,
        pdf: String,
        pdf_qr: usize,
    }

    fn printed(slot: &Slot, id: &str, what: String) -> Printed {
        let html = voucher_html(slot, id, PrintLanguage::English, None).unwrap();
        let book = open_ref(slot).unwrap();
        let v = book.voucher(VoucherId(parse_uuid(id, "voucher").unwrap())).unwrap();
        let b = branding(slot);
        let p = prepare(slot, book, v, PrintLanguage::English, None, &b.design);
        let (address, grouping) = header_parts(book);
        let h = Header { company: &book.name, address: &address, grouping };
        let docs = [PdfDoc { doc: &p.doc, look: look(&p, &b, book.country()), labels: pdf_labels(p.layout) }];
        let pdf = khata_print::pdf::invoices_pdf_text(&h, &docs).join("\n");
        let pdf_qr = khata_print::pdf::invoices_pdf_qr_codes(&h, &docs);
        let bytes = khata_print::pdf::invoices_pdf(&h, &docs);
        assert!(bytes.starts_with(b"%PDF-") && bytes.ends_with(b"%%EOF\n"), "{what}: a whole PDF");
        Printed { what, html, pdf, pdf_qr }
    }

    /// The voucher printed in every template and paper (and on the B2C
    /// receipt), with every optional column and line switched off, so only
    /// what the law keeps is left to find.
    fn every_design(slot: &mut Slot, id: &str) -> Vec<Printed> {
        let mut out = Vec::new();
        let base = branding(slot).design;
        for template in Template::ALL {
            for paper in [Paper::A4, Paper::Letter, Paper::A5] {
                for receipt in [false, true] {
                    let d = InvoiceDesign {
                        template,
                        paper,
                        receipt_for_b2c: receipt,
                        accent: "#1f4e79".into(),
                        show_hsn: false,
                        show_qty_rate: false,
                        show_amount_in_words: false,
                        show_signatory: false,
                        ..base.clone()
                    };
                    set_design(slot, d).unwrap();
                    out.push(printed(slot, id, format!("{template:?} {paper:?} receipt={receipt}")));
                }
            }
        }
        out
    }

    /// The text without white space: PDF lines wrap long text (at spaces,
    /// or inside an unbroken IRN).
    fn squash(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    fn has_all(p: &Printed, needles: &[&str]) {
        let pdf = squash(&p.pdf);
        for n in needles {
            assert!(p.html.contains(n), "{}: HTML is missing {n:?}", p.what);
            assert!(pdf.contains(&squash(n)), "{}: PDF is missing {n:?}\n{}", p.what, p.pdf);
        }
    }

    fn write_setting(slot: &Slot, key: &str, value: &impl Serialize) {
        store_of(slot).unwrap().set_record(key, Some(&serde_json::to_string(value).unwrap()), "test").unwrap();
    }

    #[test]
    fn india_keeps_gstin_place_of_supply_hsn_and_the_irn_in_every_design() {
        let (_f, mut slot) = company("IN", "design-in");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_company_address(&mut slot, "12 MG Road\nPune 411001").unwrap();
        let buyer = party(&mut slot, "Bengaluru Co", Some("29AAGCB7383J1Z4"), serde_json::json!({ "address": "1 Brigade Rd\nBengaluru" }));
        let igst = ledger(&mut slot, "Output IGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &igst, "IGST", TaxSide::Output).unwrap();
        let laptop = item(&mut slot, "Laptop", "847130");
        let id = sale(&mut slot, "Sales", &buyer, &laptop, "2", "50000", Some((&igst, "18000")));
        let irn = "a".repeat(64);
        let record = crate::irp_client::IrnRecord { voucher_id: id.clone(), irn: irn.clone(), ack_no: "112010036563310".into(), ack_dt: "2026-05-02 11:05:00".into(), status: "ACT".into(), signed_qr_code: "eyJhbGciOiJSUzI1NiJ9.eyJkYXRhIjoiMSJ9.c2ln".into(), ..Default::default() };
        write_setting(&slot, &format!("irp.irn.{id}"), &record);
        for p in every_design(&mut slot, &id) {
            has_all(&p, &["Tax Invoice", "27AAPFU0939F1ZV", "29AAGCB7383J1Z4", "Karnataka (29)", "847130", "2 Nos", "50,000.00", "1,18,000.00", "Output IGST", &irn, "112010036563310", "Authorised signatory", "Brigade Rd"]);
            assert!(p.html.contains("<div class=\"irn-block\"") && p.html.contains("<svg"), "{}: the signed QR", p.what);
            assert!(p.pdf_qr >= 1, "{}: the signed QR in the PDF", p.what);
        }
    }

    #[test]
    fn india_exports_print_the_rule_46_endorsement_lut_arn_and_shipping_data() {
        let (_f, mut slot) = company("IN", "design-in-export");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        let buyer = party(&mut slot, "Dubai Imports LLC", None, serde_json::json!({ "address": "Jebel Ali", "country": "AE" }));
        let laptop = item(&mut slot, "Laptop", "847130");
        let lut = sale(&mut slot, "Sales", &buyer, &laptop, "1", "1000", None);
        let mut design = InvoiceDesign { lut_arn: "AD2704260012345".into(), ..Default::default() };
        design.titles.insert("Sales".into(), "tax invoice".into());
        set_design(&mut slot, design.clone()).unwrap();
        let details = VoucherPrintDetails {
            ship_to_name: "Acme Warehouse".into(),
            ship_to_address: "Plot 9\nJebel Ali Free Zone".into(),
            port_of_loading: "Nhava Sheva".into(),
            port_of_discharge: "Jebel Ali".into(),
            incoterms: "FOB Nhava Sheva".into(),
            destination_country: "United Arab Emirates".into(),
            shipping_bill_no: "1234567".into(),
            shipping_bill_date: "2026-05-04".into(),
            ..Default::default()
        };
        set_voucher_details(&mut slot, &lut, details.clone()).unwrap();
        for p in every_design(&mut slot, &lut) {
            has_all(&p, &[IN_EXPORT_UNDER_LUT, "LUT ARN: AD2704260012345", "Acme Warehouse", "Jebel Ali Free Zone", "Nhava Sheva", "FOB Nhava Sheva", "United Arab Emirates", "1234567", "4-May-2026", "Dubai Imports LLC"]);
        }
        // With IGST paid the endorsement says so, and no ARN is printed.
        let igst = ledger(&mut slot, "Output IGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &igst, "IGST", TaxSide::Output).unwrap();
        let paid = sale(&mut slot, "Sales", &buyer, &laptop, "1", "1000", Some((&igst, "180")));
        let p = printed(&slot, &paid, "with IGST".into());
        has_all(&p, &[IN_EXPORT_WITH_IGST]);
        assert!(!p.html.contains("LUT ARN"));
        // A domestic sale has no endorsement.
        let local = party(&mut slot, "Pune Retail", None, serde_json::json!({ "address": "Camp" }));
        let home = sale(&mut slot, "Sales", &local, &laptop, "1", "1000", None);
        assert!(!printed(&slot, &home, "domestic".into()).html.contains("SUPPLY MEANT FOR EXPORT"));
    }

    #[test]
    fn india_titles_are_the_rules_choices_only() {
        let (_f, mut slot) = company("IN", "design-in-titles");
        let mut d = InvoiceDesign::default();
        d.titles.insert("Sales".into(), "bill of supply".into());
        d.titles.insert("Payment".into(), "Payment Advice".into());
        let saved = set_design(&mut slot, d).unwrap();
        assert_eq!(saved.design.titles["Sales"], "Bill of Supply");
        assert_eq!(saved.titles.iter().find(|t| t.kind == "Payment").unwrap().title, "Payment Advice");
        let mut d = InvoiceDesign::default();
        d.titles.insert("Sales".into(), "Invoice".into());
        assert!(set_design(&mut slot, d).unwrap_err().contains("Rule 46"));
        let mut d = InvoiceDesign::default();
        d.titles.insert("CreditNote".into(), "Refund".into());
        assert!(set_design(&mut slot, d).unwrap_err().contains("can't be changed"));
    }

    #[test]
    fn amounts_in_words_follow_the_country_and_the_print_language() {
        // KSA: bilingual, English then the Arabic tafqit, on the HTML and the PDF.
        let (_f, mut slot) = company("SA", "design-sa-words");
        let g = gulf(&mut slot, "300075588700003", "SA-01", Some("311111111100003"));
        let id = sale(&mut slot, "Sales", &g.buyer, &g.laptop, "2", "500", Some((&g.output, "150")));
        has_all(&printed(&slot, &id, "KSA".into()), &["Saudi Riyals One Thousand One Hundred Fifty Only", "فقط ألف ومائة وخمسون ريالاً سعودياً لا غير"]);
        // UAE.
        let (_f, mut slot) = company("AE", "design-ae-words");
        let g = gulf(&mut slot, "100123456700003", "DXB", Some("100234567800003"));
        let id = sale(&mut slot, "Sales", &g.buyer, &g.laptop, "2", "1000", Some((&g.output, "100")));
        has_all(&printed(&slot, &id, "UAE".into()), &["UAE Dirhams Two Thousand One Hundred Only", "فقط ألفان ومائة درهم إماراتي لا غير"]);
        // UK: British wording, no Arabic.
        let (_f, mut slot) = company("GB", "design-gb-words");
        set_tax_rate(&mut slot, "STANDARD", "2011-01-04", "20").unwrap();
        let buyer = party(&mut slot, "Leeds Ltd", None, serde_json::json!({ "address": "1 Park Row\nLeeds" }));
        let widget = item(&mut slot, "Widget", "STANDARD");
        let id = sale(&mut slot, "Sales", &buyer, &widget, "3", "1500.25", None);
        let p = printed(&slot, &id, "UK".into());
        has_all(&p, &["Four thousand five hundred pounds and seventy-five pence only"]);
        assert!(!p.html.contains("فقط"));
        // Germany: German words.
        let (_f, mut slot) = company("DE", "design-de-words");
        let buyer = party(&mut slot, "Berlin GmbH", None, serde_json::json!({ "address": "Unter den Linden 1\nBerlin" }));
        let widget = item(&mut slot, "Widget", "STANDARD");
        let id = sale(&mut slot, "Sales", &buyer, &widget, "3", "1500", None);
        has_all(&printed(&slot, &id, "DE".into()), &["Viertausendfünfhundert Euro"]);
        // India printed in Arabic: Arabic words, in rupees.
        let (_f, mut slot) = company("IN", "design-in-words-ar");
        let buyer = party(&mut slot, "Pune Retail", None, serde_json::json!({ "address": "Camp" }));
        let laptop = item(&mut slot, "Laptop", "847130");
        let id = sale(&mut slot, "Sales", &buyer, &laptop, "1", "1000", None);
        assert!(voucher_html(&slot, &id, PrintLanguage::Arabic, None).unwrap().contains("فقط ألف روبية هندية لا غير"));
        assert!(printed(&slot, &id, "IN".into()).html.contains("Rupees One Thousand Only"));
    }

    #[test]
    fn a_cheque_follows_the_banks_country() {
        let (_f, mut slot) = company("GB", "cheque-gb");
        let bank = ledger(&mut slot, "Barclays", "Bank Accounts", "100000");
        let rent = ledger(&mut slot, "Landlord", "Sundry Creditors", "");
        let paid = post_voucher(&mut slot, voucher("Payment", "2026-05-07", vec![entry(&rent, DrCr::Dr, "1247.50"), entry(&bank, DrCr::Cr, "1247.50")])).unwrap();
        let html = crate::invoice::cheque_html(&slot, &paid.id, false).unwrap();
        assert!(html.contains("One thousand two hundred and forty-seven pounds and fifty pence only"), "{html}");
        // An account with a bank abroad: that country's cheque wording.
        let banks = group_id(&slot, "Bank Accounts");
        let input: LedgerInput = serde_json::from_value(serde_json::json!({ "name": "Barclays", "group_id": banks, "mailing": { "country": "US" } })).unwrap();
        alter_ledger(&mut slot, &bank, &input).unwrap();
        let html = crate::invoice::cheque_html(&slot, &paid.id, false).unwrap();
        assert!(html.contains("One Thousand Two Hundred Forty-Seven and 50/100 Pounds Sterling"), "{html}");
    }

    struct Gulf {
        buyer: String,
        output: String,
        laptop: String,
    }

    fn gulf(slot: &mut Slot, vat_no: &str, region: &str, buyer_vat: Option<&str>) -> Gulf {
        set_company_tax(slot, vat_no, region).unwrap();
        set_company_address(slot, "King Fahd Road\nRiyadh 12211").unwrap();
        set_tax_rate(slot, "STANDARD", "2018-01-01", "15").unwrap();
        let buyer = party(slot, "Jeddah Supplies", buyer_vat, serde_json::json!({ "address": "Tahlia Street\nJeddah", "postal_code": "23434" }));
        let output = ledger(slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(slot, &output, "VAT", TaxSide::Output).unwrap();
        let laptop = item(slot, "Laptop", "STANDARD");
        Gulf { buyer, output, laptop }
    }

    #[test]
    fn ksa_keeps_the_bilingual_layout_vat_numbers_and_the_zatca_qr_in_every_design() {
        let (_f, mut slot) = company("SA", "design-sa");
        let g = gulf(&mut slot, "300075588700003", "SA-01", Some("311111111100003"));
        let id = sale(&mut slot, "Sales", &g.buyer, &g.laptop, "2", "500", Some((&g.output, "150")));
        crate::zatca::zatca_invoice_xml(&mut slot, &id, "standard").unwrap();
        for p in every_design(&mut slot, &id) {
            has_all(&p, &["فاتورة ضريبية", "300075588700003", "311111111100003", "الرقم الضريبي", "2 Nos", "500.00", "1,150.00", "ضريبة القيمة المضافة", "الإجمالي"]);
            assert!(p.html.contains("Tax Invoice</span> / <span lang=\"ar\" dir=\"rtl\">فاتورة ضريبية</span>"), "{}", p.what);
            assert!(squash(&p.pdf).contains(&squash("Tax Invoice / فاتورة ضريبية")), "{}", p.what);
            assert!(p.html.contains("<svg") && p.pdf_qr >= 1, "{}: the ZATCA QR", p.what);
        }
        let mut d = InvoiceDesign::default();
        d.titles.insert("Sales".into(), "Invoice".into());
        assert!(set_design(&mut slot, d).unwrap_err().contains("KSA"), "the KSA titles are fixed");
    }

    #[test]
    fn ksa_simplified_invoices_go_on_the_receipt_with_their_arabic_and_qr() {
        let (_f, mut slot) = company("SA", "design-sa-b2c");
        let g = gulf(&mut slot, "300075588700003", "SA-01", None);
        let id = sale(&mut slot, "Sales", &g.buyer, &g.laptop, "1", "100", Some((&g.output, "15")));
        crate::zatca::zatca_invoice_xml(&mut slot, &id, "simplified").unwrap();
        set_design(&mut slot, InvoiceDesign { receipt_for_b2c: true, template: Template::Modern, ..Default::default() }).unwrap();
        let p = printed(&slot, &id, "receipt".into());
        assert!(p.html.contains("size: 80mm 297mm"), "on the receipt paper");
        has_all(&p, &["فاتورة ضريبية مبسطة", "300075588700003", "115.00"]);
        assert!(p.html.contains("<svg") && p.pdf_qr == 1);
    }

    #[test]
    fn gulf_exports_print_the_zero_rating_in_english_and_arabic() {
        let (_f, mut slot) = company("SA", "design-sa-export");
        let g = gulf(&mut slot, "300075588700003", "SA-01", None);
        let abroad = party(&mut slot, "London Buyer Ltd", None, serde_json::json!({ "address": "1 Strand\nLondon", "country": "GB" }));
        let id = sale(&mut slot, "Sales", &abroad, &g.laptop, "1", "100", None);
        let p = printed(&slot, &id, "export".into());
        has_all(&p, &["Zero-rated supply: export", "توريد خاضع للضريبة بنسبة الصفر: تصدير", "GB"]);
    }

    #[test]
    fn uae_keeps_the_trn_and_the_tax_invoice_title_in_every_design() {
        let (_f, mut slot) = company("AE", "design-ae");
        let g = gulf(&mut slot, "100123456700003", "DXB", Some("100234567800003"));
        let id = sale(&mut slot, "Sales", &g.buyer, &g.laptop, "3", "100", Some((&g.output, "15")));
        for p in every_design(&mut slot, &id) {
            has_all(&p, &["Tax Invoice", "فاتورة ضريبية", "100123456700003", "100234567800003", "3 Nos", "315.00"]);
        }
        let note = sale(&mut slot, "CreditNote", &g.buyer, &g.laptop, "1", "100", Some((&g.output, "5")));
        has_all(&printed(&slot, &note, "credit note".into()), &["Tax Credit Note"]);
    }

    #[test]
    fn uk_keeps_the_vat_number_and_the_reverse_charge_wording_in_every_design() {
        let (_f, mut slot) = company("GB", "design-gb");
        set_company_tax(&mut slot, "GB 562 2359 45", "ENG").unwrap();
        set_tax_rate(&mut slot, "STANDARD", "2011-01-04", "20").unwrap();
        let buyer = party(&mut slot, "Phone Shop Ltd", Some("562235987"), serde_json::json!({ "address": "2 High St\nLeeds" }));
        let phones = item(&mut slot, "Mobile phones", "DRC:STANDARD");
        let id = sale(&mut slot, "Sales", &buyer, &phones, "2", "3000", None);
        for p in every_design(&mut slot, &id) {
            has_all(&p, &["562235945", "Reverse charge: Customer to pay the VAT to HMRC.", "2 Nos", "3,000.00"]);
        }
    }

    #[test]
    fn mexico_keeps_the_pac_block_in_every_design() {
        let (_f, mut slot) = company("MX", "design-mx");
        set_company_tax(&mut slot, "EKU9003173C9", "JAL").unwrap();
        let buyer = party(&mut slot, "Cliente SA", Some("XAXX010101000"), serde_json::json!({ "address": "Av. Juárez 1\nGuadalajara" }));
        let widget = item(&mut slot, "Widget", "01010101");
        let id = sale(&mut slot, "Sales", &buyer, &widget, "1", "100", None);
        let record = crate::pac_client::PacRecord {
            voucher_id: id.clone(),
            uuid: "5FB2822E-396D-4725-8521-CDC4BDD20CCF".into(),
            fecha_timbrado: "2026-05-02T10:00:00".into(),
            sello_cfd: "SELLOCFD".into(),
            sello_sat: "SELLOSAT".into(),
            no_certificado: "30001000000500003416".into(),
            no_certificado_sat: "30001000000500003456".into(),
            rfc_emisor: "EKU9003173C9".into(),
            rfc_receptor: "XAXX010101000".into(),
            total: "100".parse().unwrap(),
            status: crate::pac_client::VIGENTE.into(),
            ..Default::default()
        };
        write_setting(&slot, &format!("mx.cfdi.{id}"), &record);
        for p in every_design(&mut slot, &id) {
            has_all(&p, &["Folio fiscal (UUID)", "5FB2822E-396D-4725-8521-CDC4BDD20CCF", "SELLOSAT", "30001000000500003456", "Este documento es una representación impresa de un CFDI."]);
            assert!(p.html.contains("<div class=\"cfdi-block\"") && p.pdf_qr >= 1, "{}: the CFDI QR", p.what);
        }
    }

    #[test]
    fn images_are_checked_and_kept_in_the_company_file() {
        let (_f, mut slot) = company("IN", "design-images");
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 2"><rect width="4" height="2" fill="#123"/></svg>"##;
        let dto = set_image(&mut slot, "logo", svg).unwrap();
        let logo = dto.logo.unwrap();
        assert!(logo.needs_raster && logo.data_url.starts_with("data:image/svg+xml;base64,"));
        assert!(set_image(&mut slot, "logo", b"<svg><script>alert(1)</script></svg>").is_err());
        assert!(set_image(&mut slot, "banner", svg).is_err());
        // A 1 × 1 RGBA PNG, as the webview's canvas makes them.
        let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let dto = set_image_raster(&mut slot, "logo", png).unwrap();
        assert!(!dto.logo.unwrap().needs_raster);
        assert!(branding(&slot).logo.unwrap().bitmap().is_some(), "PDFs draw the rendition");
        // Settings, not audit entries.
        let entries = store_of(&slot).unwrap().len().unwrap();
        set_design(&mut slot, InvoiceDesign { footer: "Thank you".into(), ..Default::default() }).unwrap();
        assert_eq!(store_of(&slot).unwrap().len().unwrap(), entries, "the design is not in the audit trail");
        remove_image(&mut slot, "logo").unwrap();
        assert!(branding(&slot).logo.is_none());
    }

    #[test]
    fn batch_pdfs_take_a_period_or_a_list() {
        let (_f, mut slot) = company("IN", "design-batch");
        let buyer = party(&mut slot, "Pune Retail", None, serde_json::json!({ "address": "Camp" }));
        let laptop = item(&mut slot, "Laptop", "847130");
        let a = sale(&mut slot, "Sales", &buyer, &laptop, "1", "100", None);
        let _b = sale(&mut slot, "Sales", &buyer, &laptop, "1", "200", None);
        let all = vouchers_pdf(&slot, &[], "2026-05-01", "2026-05-31", PrintLanguage::English, None).unwrap();
        assert_eq!(all.count, 2);
        assert!(all.file_name.ends_with(".pdf"));
        let one = vouchers_pdf(&slot, std::slice::from_ref(&a), "", "", PrintLanguage::English, None).unwrap();
        assert_eq!(one.count, 1);
        assert!(vouchers_pdf(&slot, &[], "2026-06-01", "2026-06-30", PrintLanguage::English, None).is_err());
        // Hijri dates the caller passes print under each voucher's date.
        let hijri = HashMap::from([("2026-05-02".to_owned(), "15 ذو القعدة 1447 هـ".to_owned())]);
        let book = open_ref(&slot).unwrap();
        let v = book.voucher(VoucherId(parse_uuid(&a, "voucher").unwrap())).unwrap();
        let b = branding(&slot);
        let p = prepare(&slot, book, v, PrintLanguage::Arabic, hijri.get("2026-05-02").map(String::as_str), &b.design);
        assert_eq!(p.doc.hijri.as_deref(), Some("15 ذو القعدة 1447 هـ"));
        let dated = vouchers_pdf(&slot, &[], "2026-05-01", "2026-05-31", PrintLanguage::Arabic, Some(&hijri)).unwrap();
        assert_eq!(dated.count, 2);
        let (address, grouping) = header_parts(book);
        let h = Header { company: &book.name, address: &address, grouping };
        let docs = [PdfDoc { doc: &p.doc, look: look(&p, &b, book.country()), labels: pdf_labels(p.layout) }];
        let text = khata_print::pdf::invoices_pdf_text(&h, &docs).join("\n");
        assert!(text.contains("15 ذو القعدة 1447 هـ") && text.contains("التاريخ"), "{text}");
        let single = voucher_pdf(&slot, &a, PrintLanguage::Arabic, None).unwrap();
        assert!(b64().decode(single.pdf_base64).unwrap().starts_with(b"%PDF-"));
        assert_eq!(single.file_name, "Tax Invoice 26-27-1.pdf");
    }

    #[test]
    fn the_preview_uses_the_unsaved_design_and_falls_back_to_a_sample() {
        let (_f, slot) = company("GB", "design-preview");
        let d = InvoiceDesign { template: Template::Modern, footer: "Unsaved footer".into(), ..Default::default() };
        let html = preview(&slot, d, None, false, PrintLanguage::English).unwrap();
        assert!(html.contains("Sample Customer") && html.contains("Unsaved footer") && html.contains("text-transform: uppercase"));
        assert!(branding(&slot).design.footer.is_empty(), "nothing saved");
    }

    /// A foreign-currency sale: the party kept in USD, the lines and tax in USD at `rate`.
    fn dollar_sale(slot: &mut Slot, tax_component: &str, rate: &str) -> String {
        let r: rust_decimal::Decimal = rate.parse().unwrap();
        let debtors = group_id(slot, "Sundry Debtors");
        let acme = create_party_ledger(slot, "Acme Inc", &debtors, "", None, None, None).unwrap().ledger.id;
        crate::forex::set_ledger_currency(slot, &acme, "USD").unwrap();
        let sales = ledger(slot, "Export Sales", "Sales Accounts", "");
        let tax = ledger(slot, &format!("Output {tax_component}"), "Duties & Taxes", "");
        set_ledger_tax(slot, &tax, tax_component, TaxSide::Output).unwrap();
        let fx = |amount: &str| Some(crate::forex::ForexInput { currency: "USD".into(), amount: amount.into(), rate: rate.into(), bills: Vec::new() });
        let base = |usd: i64| (r * rust_decimal::Decimal::from(usd)).to_string();
        let mut party = entry(&acme, DrCr::Dr, &base(1090));
        party.forex = fx("1090");
        let mut income = entry(&sales, DrCr::Cr, &base(1000));
        income.forex = fx("1000");
        let mut t = entry(&tax, DrCr::Cr, &base(90));
        t.forex = fx("90");
        post_voucher(slot, voucher("Sales", "2026-05-02", vec![party, income, t])).unwrap().id
    }

    #[test]
    fn foreign_currency_invoices_keep_the_base_currency_figures_each_country_asks_for() {
        for (country, tax, rate, needles) in [
            ("SG", "GST", "1.35", vec!["Amounts in USD at 1 USD = 1.35 SGD", "Total excluding GST in SGD", "1,350.00", "GST in SGD", "121.50", "Total including GST in SGD", "1,471.50"]),
            ("IN", "IGST", "83", vec!["Amounts in USD at 1 USD = 83 INR", "GST in INR", "7,470.00", "Total in INR", "90,470.00"]),
            ("AE", "VAT", "3.6725", vec!["Amounts in USD at 1 USD = 3.6725 AED", "VAT in AED", "330.53", "Total in AED", "4,003.03"]),
            ("GB", "VAT", "0.79", vec!["Amounts in USD at 1 USD = 0.79 GBP", "VAT in GBP", "71.10", "Total in GBP", "861.10"]),
        ] {
            let (_f, mut slot) = company(country, &format!("design-fx-{country}"));
            let id = dollar_sale(&mut slot, tax, rate);
            for p in every_design(&mut slot, &id) {
                let p = Printed { what: format!("{country} {}", p.what), ..p };
                has_all(&p, &needles);
                has_all(&p, &["1,090.00"]);
            }
        }
    }
}

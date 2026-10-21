//! E-invoices on the EN 16931 / Peppol model from the books: Peppol PINT
//! SG (Singapore InvoiceNow) and PINT AE (UAE) UBL, the company's Peppol
//! participant id, Factur-X / ZUGFeRD PDFs for companies in the EU, and a
//! generic "send through an Access Point" HTTP client.
//!
//! The documents live in `khata_en16931` (`pint`, `cii`, `facturx`); this
//! module turns a Sales or Credit Note voucher into them, the way
//! `vat_de.rs` does for XRechnung:
//! - the party line carries the total; item lines (goods) and other ledgers
//!   on the opposite side (services) are the invoice lines; ledgers with a
//!   type of tax give the tax; ledgers on the party's side are document
//!   allowances (discounts); a ledger named "Round off" is left out;
//! - each line's tax category comes from its tax class and the rate on file
//!   for the voucher date (see [`pint_sg_xml`], [`pint_ae_xml`],
//!   [`facturx_pdf`]);
//! - the seller is the company: its name, tax registration and address.
//!   Until the company profile holds them, other details are read from
//!   "Key: value" lines of the company address: `UEN:`, `Peppol ID:`,
//!   `Trade licence:` and `Licensing authority:` (or `Emirates ID:`),
//!   `Tel:`, `Email:`, `Contact:`, `IBAN:`, `BIC:`, `Bank account:`,
//!   `PayNow:`;
//! - the buyer is the party ledger: its name, tax id and mailing address,
//!   with the same "Key: value" lines in the mailing address (`Peppol ID:`,
//!   `UEN:`, `Trade licence:`, `Licensing authority:`);
//! - "Key: value" lines of the voucher narration give the buyer reference
//!   (`Buyer reference:`, `Your reference:`), the purchase order (`PO:`,
//!   `Order:`), the payment terms (`Payment terms:`) and, for UAE credit
//!   notes, the reason code (`Reason: DL8.61.1.D`); the other lines are
//!   invoice notes;
//! - the UUID (SG BT-SG-003, AE BTAE-07) is the voucher's own id, so the
//!   same voucher always gives the same UUID.
//!
//! **Transmission** goes through an Access Point (Singapore: an
//! IMDA-accredited InvoiceNow Access Point; UAE: an Accredited Service
//! Provider). Every provider has its own API, sign-in and status calls;
//! there is no standard one. [`ap_send`] is a generic HTTP POST of the UBL
//! to a configured URL with an API key header, which some providers accept
//! as is and others need an adapter for. KhataOS claims compatibility with
//! no particular provider. Its settings, with the key, are kept in
//! `<app data>/peppol/access-point.json` (owner-only, `0600`), never in
//! the books.

use std::path::Path;
use std::sync::Mutex;

use base64::Engine as _;
use chrono::Days;
use khata_compliance::RateSchedule;
use khata_core::{Amount, BillKind, Book, DrCr, Ledger, Voucher, VoucherId, VoucherKind};
use khata_en16931::pint::{
    self, AeVat, Jurisdiction, LegalIdKind, LegalRegistration, LineTax, PintInvoice, SgGst, TaxCategory,
};
use khata_en16931::{
    Address, AllowanceCharge, Contact, CreditTransfer, ElectronicAddress, Invoice, Line, Party, PaymentInstructions,
    PrecedingInvoice, Specification, TypeCode, VatBreakdown, VatCategory, cii, facturx, units, validate_for,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, group_is_party, open_ref, parse_uuid};

// ------------------------------------------------------------ the voucher

fn label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Sales",
        VoucherKind::CreditNote => "Credit Note",
        _ => "Voucher",
    }
}

/// The party line: a debtor/creditor, else the cash or bank line on the
/// party's side, else the first line on that side.
fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = if v.kind == VoucherKind::Sales { DrCr::Dr } else { DrCr::Cr };
    let is_party = |e: &khata_core::Entry| book.ledger(e.ledger).is_some_and(|l| group_is_party(book, l.group));
    v.entries
        .iter()
        .position(is_party)
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

/// One invoice line before its tax category.
struct RawLine {
    name: String,
    qty: Decimal,
    price: Decimal,
    unit: String,
    amount: Amount,
    class: Option<String>,
    goods: bool,
}

struct Parsed {
    party: Option<usize>,
    lines: Vec<RawLine>,
    discounts: Vec<(String, Amount)>,
    tax: Amount,
}

fn parse(book: &Book, v: &Voucher, warnings: &mut Vec<String>) -> Parsed {
    let name = format!("{} {}", label(v.kind), v.number);
    let party = party_line(book, v);
    let party_side = party.map(|i| v.entries[i].side);
    let mut p = Parsed { party, lines: Vec::new(), discounts: Vec::new(), tax: Amount::ZERO };
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        if ledger.is_some_and(|l| l.tax.is_some()) {
            p.tax += if on_party_side { -e.amount } else { e.amount };
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                p.lines.push(RawLine {
                    name: item.map(|i| i.name.clone()).unwrap_or_default(),
                    qty: l.qty,
                    price: l.rate,
                    unit: item.and_then(|i| book.unit(i.unit)).map(|u| u.symbol.clone()).unwrap_or_default(),
                    amount: if on_party_side { -l.amount } else { l.amount },
                    class: item.and_then(|i| i.tax_class.clone()),
                    goods: true,
                });
            }
        } else if ledger_name.to_lowercase().contains("round") {
            warnings.push(format!("{name}: {ledger_name} (rounding) is left out"));
        } else if on_party_side {
            p.discounts.push((ledger_name, e.amount));
        } else {
            p.lines.push(RawLine {
                name: ledger_name,
                qty: Decimal::ONE,
                price: e.amount,
                unit: String::new(),
                amount: e.amount,
                class: ledger.and_then(|l| l.tax_class.clone()),
                goods: false,
            });
        }
    }
    p
}

fn rates(book: &Book, defaults: RateSchedule) -> ApiResult<RateSchedule> {
    let mut rates = defaults;
    if let Some(p) = book.profile() {
        for (class, changes) in &p.tax_rates {
            for (from, r) in changes {
                rates.set(class, *from, *r).map_err(err)?;
            }
        }
    }
    Ok(rates)
}

/// A class compared without spaces, hyphens and case.
fn class_key(class: &str) -> String {
    class.chars().filter(|c| !(c.is_whitespace() || *c == '-' || *c == '_')).collect::<String>().to_ascii_uppercase()
}

/// "Key: value" lines of an address, and the rest.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Card {
    lines: Vec<String>,
    phone: String,
    email: String,
    contact: String,
    iban: Option<String>,
    bic: Option<String>,
    account: Option<String>,
    paynow: Option<String>,
    uen: Option<String>,
    peppol: Option<String>,
    trade_licence: Option<String>,
    emirates_id: Option<String>,
    authority: Option<String>,
}

fn card(text: &str) -> Card {
    let mut c = Card::default();
    for raw in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some((key, value)) = raw.split_once(':') {
            let k: String = key.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
            let v = value.trim().to_owned();
            let slot = match k.as_str() {
                "tel" | "telephone" | "phone" | "telefon" | "fon" | "mobile" => Some(&mut c.phone),
                "email" | "mail" => Some(&mut c.email),
                "contact" | "kontakt" | "ansprechpartner" => Some(&mut c.contact),
                _ => None,
            };
            if let Some(s) = slot {
                *s = v;
                continue;
            }
            let opt = match k.as_str() {
                "iban" => Some(&mut c.iban),
                "bic" | "swift" => Some(&mut c.bic),
                "bankaccount" | "account" | "accountno" | "accountnumber" => Some(&mut c.account),
                "paynow" => Some(&mut c.paynow),
                "uen" => Some(&mut c.uen),
                "peppol" | "peppolid" | "participantid" | "peppolparticipantid" => Some(&mut c.peppol),
                "tradelicence" | "tradelicense" | "tradelicenceno" | "tradelicenseno" | "licence" | "license" => {
                    Some(&mut c.trade_licence)
                }
                "emiratesid" => Some(&mut c.emirates_id),
                "licensingauthority" | "issuingauthority" | "authority" => Some(&mut c.authority),
                _ => None,
            };
            if let Some(s) = opt {
                *s = Some(v).filter(|v| !v.is_empty());
                continue;
            }
        }
        c.lines.push(raw.to_owned());
    }
    c.iban = c.iban.map(|i| i.chars().filter(|c| !c.is_whitespace()).collect());
    c
}

const COUNTRY_NAMES: &[(&str, &str)] = &[
    ("singapore", "SG"),
    ("united arab emirates", "AE"),
    ("uae", "AE"),
    ("germany", "DE"),
    ("deutschland", "DE"),
    ("france", "FR"),
    ("netherlands", "NL"),
    ("nederland", "NL"),
    ("spain", "ES"),
    ("españa", "ES"),
];

/// The postal address from an address's plain lines, for a party in
/// `country`. Singapore: a 6-digit postal code ("Singapore 048616"), city
/// Singapore. Elsewhere: a "PLZ City" line ("10115 Berlin", "75001
/// Paris"), else the last line is the city. `postal` (a separate post code
/// field) wins over one found in the lines.
fn address(lines: &[String], country: &str, postal: &str) -> Address {
    let mut lines: Vec<String> = lines.to_vec();
    // A last line naming the country.
    if lines.len() > 1
        && let Some(last) = lines.last()
        && COUNTRY_NAMES.iter().any(|(n, c)| last.trim().eq_ignore_ascii_case(n) && *c == country)
        && country != "SG"
    {
        lines.pop();
    }
    let mut a = Address { country: country.to_owned(), ..Address::default() };
    if country == "SG" {
        a.city = "Singapore".into();
        // The line holding the 6-digit postal code; drop it when it is only
        // "Singapore 048616".
        if let Some(i) = lines.iter().rposition(|l| l.split_whitespace().any(is_sg_postal)) {
            let code = lines[i].split_whitespace().find(|w| is_sg_postal(w)).unwrap_or_default().to_owned();
            let rest: String = lines[i].split_whitespace().filter(|w| !is_sg_postal(w)).collect::<Vec<_>>().join(" ");
            a.postal_code = code;
            let rest_trim = rest.trim_matches(|c: char| c == ',' || c.is_whitespace());
            if rest_trim.is_empty() || rest_trim.eq_ignore_ascii_case("singapore") {
                lines.remove(i);
            } else {
                lines[i] = rest_trim.to_owned();
            }
        }
        lines.retain(|l| !l.trim().eq_ignore_ascii_case("singapore"));
    } else if let Some(i) = lines.iter().rposition(|l| {
        let head = l.split_whitespace().next().unwrap_or("");
        let head = head.strip_prefix("D-").or_else(|| head.strip_prefix("F-")).unwrap_or(head);
        (4..=5).contains(&head.len()) && head.bytes().all(|b| b.is_ascii_digit()) && l.split_whitespace().count() > 1
    }) {
        let l = lines.remove(i);
        let (head, rest) = l.split_once(char::is_whitespace).unwrap_or((&l, ""));
        a.postal_code = head.trim_start_matches("D-").trim_start_matches("F-").to_owned();
        a.city = rest.trim().to_owned();
    } else if lines.len() > 1 {
        a.city = lines.pop().unwrap_or_default();
    }
    if !postal.trim().is_empty() {
        a.postal_code = postal.trim().to_owned();
    }
    let mut it = lines.into_iter();
    a.line1 = it.next().unwrap_or_default();
    a.line2 = it.next().unwrap_or_default();
    a.line3 = it.collect::<Vec<_>>().join(", ");
    a
}

fn is_sg_postal(w: &str) -> bool {
    let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    w.len() == 6 && w.bytes().all(|b| b.is_ascii_digit())
}

/// "Key: value" lines of the narration: the buyer reference, the order,
/// the payment terms and a reason code; the rest are notes.
#[derive(Debug, Default)]
struct Narration {
    buyer_reference: Option<String>,
    order: Option<String>,
    terms: Option<String>,
    reason: Option<String>,
    notes: Vec<String>,
}

fn narration(text: &str) -> Narration {
    let mut n = Narration::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some((k, v)) = line.split_once(':') {
            let k: String = k.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
            let v = Some(v.trim().to_owned()).filter(|v| !v.is_empty());
            let slot = match k.as_str() {
                "buyerreference" | "yourreference" | "yourref" | "customerreference" | "leitwegid" | "käuferreferenz"
                | "ihrereferenz" => Some(&mut n.buyer_reference),
                "po" | "pono" | "ponumber" | "purchaseorder" | "order" | "orderno" | "ordernumber" | "bestellung" => {
                    Some(&mut n.order)
                }
                "paymentterms" | "terms" | "zahlungsbedingungen" => Some(&mut n.terms),
                "reason" | "creditreason" | "reasoncode" => Some(&mut n.reason),
                _ => None,
            };
            if let (Some(s), Some(v)) = (slot, v) {
                *s = Some(v);
                continue;
            }
        }
        n.notes.push(line.to_owned());
    }
    n
}

fn party_ledger<'a>(book: &'a Book, v: &Voucher, p: &Parsed) -> Option<&'a Ledger> {
    p.party.and_then(|i| book.ledger(v.entries[i].ledger)).filter(|l| group_is_party(book, l.group))
}

fn mailing_country(l: Option<&Ledger>, home: &str) -> String {
    l.and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| c.len() == 2)
        .unwrap_or_else(|| home.to_owned())
}

/// The EN 16931 skeleton of a Sales or Credit Note voucher: number, dates,
/// type, references, notes, lines and discounts. Categories are filled by
/// the caller; the parties too.
fn skeleton(book: &Book, v: &Voucher, p: &Parsed, currency: &str, n: &Narration) -> ApiResult<Invoice> {
    let credit = match v.kind {
        VoucherKind::Sales => false,
        VoucherKind::CreditNote => true,
        _ => return Err("e-invoices are made from Sales and Credit Note vouchers".into()),
    };
    if v.cancelled {
        return Err("a cancelled voucher has no e-invoice".into());
    }
    let mut inv = Invoice::new(&v.number, v.date, if credit { TypeCode::CreditNote } else { TypeCode::Commercial }, currency);
    inv.buyer_reference = n.buyer_reference.clone();
    inv.order_reference = n.order.clone();
    inv.payment_terms = n.terms.clone();
    inv.notes = n.notes.clone();
    let party = party_ledger(book, v, p);
    let party_entry = p.party.map(|i| &v.entries[i]);
    inv.due_date = party_entry
        .and_then(|e| e.bills.iter().find_map(|b| b.due))
        .or_else(|| party.and_then(|l| l.credit_days).and_then(|d| v.date.checked_add_days(Days::new(u64::from(d)))));
    if credit {
        inv.preceding = party_entry
            .and_then(|e| e.bills.iter().find(|b| b.kind == BillKind::Against && !b.reference.trim().is_empty()))
            .map(|b| PrecedingInvoice {
                number: b.reference.clone(),
                date: book.vouchers().find(|x| x.kind == VoucherKind::Sales && x.number == b.reference).map(|x| x.date),
            });
    }
    for (i, l) in p.lines.iter().enumerate() {
        if l.amount < Decimal::ZERO {
            return Err(format!("line {} is negative; put discounts on a ledger on the party's side", i + 1));
        }
        let unit = units::rec20(&l.unit).unwrap_or(if l.goods { "H87" } else { "C62" });
        inv.lines.push(Line {
            id: (i + 1).to_string(),
            note: None,
            quantity: l.qty,
            unit_code: unit.to_owned(),
            net_amount: l.amount,
            net_price: if l.qty.is_zero() { l.price } else { (l.amount / l.qty).round_dp(4).normalize() },
            price_base_quantity: None,
            item_name: l.name.clone(),
            item_description: None,
            seller_item_id: None,
            category: VatCategory::S,
            rate: None,
        });
    }
    for (reason, amount) in &p.discounts {
        inv.allowances_charges.push(AllowanceCharge {
            charge: false,
            amount: *amount,
            base: None,
            percent: None,
            category: VatCategory::S,
            rate: None,
            reason: Some(reason.clone()),
            reason_code: Some("95".into()),
        });
    }
    Ok(inv)
}

/// The category of the document allowances: that of the lines when they
/// all share one; otherwise a discount cannot be split by itself.
fn allowance_tax(lines: &[LineTax], discounts: usize) -> ApiResult<Vec<LineTax>> {
    if discounts == 0 {
        return Ok(Vec::new());
    }
    let first = lines.first().ok_or("the voucher has a discount but no lines")?;
    if lines.iter().any(|t| (t.category, t.rate) != (first.category, first.rate)) {
        return Err("a discount on an invoice with more than one tax category can't be split; give each line its own discounted rate".into());
    }
    let rate = first.rate.unwrap_or_default();
    Ok((0..discounts).map(|_| LineTax::for_allowance(first.category, rate)).collect())
}

fn payment(c: &Card, remittance: &str) -> Option<PaymentInstructions> {
    let transfer = |means: &str, account: &str| PaymentInstructions {
        means_code: means.into(),
        means_text: None,
        remittance: Some(remittance.to_owned()),
        credit_transfer: Some(CreditTransfer { account: account.to_owned(), name: None, bic: c.bic.clone() }),
    };
    if let Some(iban) = &c.iban {
        Some(transfer("30", iban))
    } else {
        c.account.as_deref().map(|a| transfer("30", a))
    }
}

fn contact(c: &Card) -> Contact {
    Contact { name: c.contact.clone(), phone: c.phone.clone(), email: c.email.clone() }
}

fn safe(number: &str) -> String {
    number.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

/// An e-invoice XML with the rules it breaks.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PintDto {
    pub xml: String,
    /// `PINT-SG-<number>.xml`, unsafe characters replaced.
    pub file_name: String,
    /// BT-24 of the document.
    pub customization_id: String,
    /// The buyer's participant id ("0195:SGUEN…"), where to send it.
    pub receiver: Option<String>,
    /// The seller's participant id.
    pub sender: Option<String>,
    /// Broken PINT rules ("[BR-105-GST-SG] …"): not valid until fixed.
    pub errors: Vec<String>,
    /// Warnings, and what the books could not say.
    pub warnings: Vec<String>,
}

fn participant(e: &Option<ElectronicAddress>) -> Option<String> {
    e.as_ref().map(|e| format!("{}:{}", e.scheme, e.value))
}

fn finish(p: &PintInvoice, prefix: &str, number: &str, mut warnings: Vec<String>) -> PintDto {
    let mut errors = Vec::new();
    for x in p.validate() {
        if x.warning { warnings.push(x.to_string()) } else { errors.push(x.to_string()) }
    }
    PintDto {
        xml: p.write(),
        file_name: format!("{prefix}-{}.xml", safe(number)),
        customization_id: p.jurisdiction.customization_id().to_owned(),
        receiver: participant(&p.invoice.buyer.electronic_address),
        sender: participant(&p.invoice.seller.electronic_address),
        errors,
        warnings,
    }
}

fn voucher<'a>(book: &'a Book, voucher_id: &str) -> ApiResult<&'a Voucher> {
    book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or_else(|| "unknown voucher".to_owned())
}

/// Booked tax against the document's.
fn compare_tax(warnings: &mut Vec<String>, worked_out: Decimal, booked: Decimal, what: &str) {
    if (worked_out - booked).abs() > Decimal::new(5, 2) {
        warnings.push(format!("the invoice's {what} is {worked_out} by its tax breakdown, but {} is booked", booked.round_dp(2)));
    }
}

// ------------------------------------------------------------ Singapore

/// The PINT SG category and rate of a line, from its tax class and the
/// rate on file (`khata_sg::gst::treatment`): standard → SR, zero-rated →
/// ZR, exempt → ESN33, out of scope → OS. A class named after a GST
/// category code ("ES33", "DS", "SRRC"…) is that category. A company with
/// no GST registration number issues NG lines (no GST).
fn sg_line(class: Option<&str>, date: chrono::NaiveDate, rates: &RateSchedule, registered: bool) -> ApiResult<LineTax> {
    if !registered {
        return Ok(LineTax::new(TaxCategory::Sg(SgGst::Ng), Decimal::ZERO));
    }
    let class = class.unwrap_or(khata_sg::gst::STANDARD_CLASS);
    let standard = rates.rate(khata_sg::gst::STANDARD_CLASS, date);
    if let Some(code) = SgGst::parse(class) {
        let rate = rates.rate(class, date).filter(|r| !r.is_zero()).or(standard).unwrap_or_default();
        return Ok(LineTax::new(TaxCategory::Sg(code), rate));
    }
    let rate = rates.rate(class, date).ok_or_else(|| format!("no GST rate for {class:?} on {date}: add it to the company's rate list"))?;
    let code = match khata_sg::gst::treatment(class, rate) {
        khata_sg::gst::Treatment::Standard => SgGst::Sr,
        khata_sg::gst::Treatment::ZeroRated => SgGst::Zr,
        khata_sg::gst::Treatment::Exempt => SgGst::Esn33,
        khata_sg::gst::Treatment::OutOfScope => SgGst::Os,
    };
    Ok(LineTax::new(TaxCategory::Sg(code), rate))
}

fn sg_invoice(book: &Book, v: &Voucher, warnings: &mut Vec<String>) -> ApiResult<(PintInvoice, Decimal)> {
    crate::forex::refuse_foreign_einvoice(v, "A PINT SG invoice")?;
    if book.country() != Some(khata_sg::COUNTRY.code) {
        return Err("PINT SG (InvoiceNow) invoices are for companies in Singapore".into());
    }
    let profile = book.profile().ok_or("pick the company's country first")?;
    let rates = rates(book, khata_sg::gst::default_rates())?;
    let p = parse(book, v, warnings);
    let n = narration(&v.narration);
    let mut inv = skeleton(book, v, &p, khata_sg::CURRENCY.code, &n)?;
    let party = party_ledger(book, v, &p);

    // Seller.
    let gst_no = profile.tax_id.clone().filter(|t| !t.trim().is_empty());
    let registered = gst_no.is_some();
    if !registered {
        warnings.push("the company has no GST registration number: the lines are NG (not GST-registered)".into());
    }
    let c = card(&profile.address);
    let uen = c.uen.clone().or_else(|| {
        gst_no.as_deref().filter(|g| khata_sg::gst_no::kind(g).is_ok_and(|k| k.is_uen())).map(khata_sg::gst_no::normalise)
    });
    if uen.is_none() {
        warnings.push("the company's UEN is not known: add a \"UEN: …\" line to the company address".into());
    }
    let endpoint = c.peppol.as_deref().and_then(pint::parse_participant_id).or_else(|| {
        uen.as_deref().map(|u| ElectronicAddress { value: pint::sg_endpoint(u), scheme: pint::SG_UEN_SCHEME.into() })
    });
    inv.seller = Party {
        name: book.name.clone(),
        legal_id: uen,
        vat_id: gst_no,
        electronic_address: endpoint,
        address: address(&c.lines, "SG", ""),
        contact: contact(&c),
        ..Party::default()
    };
    inv.payment = match &c.paynow {
        Some(id) => Some(PaymentInstructions {
            means_code: "Z02".into(),
            means_text: Some("PayNow".into()),
            remittance: Some(v.number.clone()),
            credit_transfer: Some(CreditTransfer { account: id.clone(), name: None, bic: None }),
        }),
        None => payment(&c, &v.number),
    };

    // Buyer.
    let abroad;
    match party {
        Some(l) => {
            let m = l.mailing.clone().unwrap_or_default();
            let bc = card(&m.address);
            let country = mailing_country(Some(l), "SG");
            abroad = country != "SG";
            let tax_id = l.tax_id.clone().filter(|t| !t.trim().is_empty());
            let buyer_gst = tax_id.clone().filter(|t| !abroad && khata_sg::gst_no::validate(t).is_ok());
            let buyer_uen = bc.uen.clone().or_else(|| {
                buyer_gst.as_deref().filter(|g| khata_sg::gst_no::kind(g).is_ok_and(|k| k.is_uen())).map(khata_sg::gst_no::normalise)
            });
            let endpoint = bc.peppol.as_deref().and_then(pint::parse_participant_id).or_else(|| {
                buyer_uen.as_deref().map(|u| ElectronicAddress { value: pint::sg_endpoint(u), scheme: pint::SG_UEN_SCHEME.into() })
            });
            if endpoint.is_none() {
                warnings.push(format!("{}'s Peppol id is not known: add a \"Peppol ID: 0195:SGUEN…\" or \"UEN: …\" line to its mailing address", l.name));
            }
            let mut a = address(&bc.lines, &country, &m.postal_code);
            if abroad && a.city.is_empty() {
                a.city = std::mem::take(&mut a.line2);
            }
            inv.buyer = Party {
                name: l.name.clone(),
                legal_id: buyer_uen,
                vat_id: if abroad { tax_id } else { buyer_gst },
                electronic_address: endpoint,
                address: a,
                contact: Contact { email: m.email.trim().to_owned(), phone: m.phone.trim().to_owned(), ..Contact::default() },
                ..Party::default()
            };
        }
        None => {
            abroad = false;
            warnings.push("the voucher has no party ledger: the buyer has no address or Peppol id".into());
            inv.buyer = Party { name: "Cash customer".into(), address: Address { country: "SG".into(), ..Address::default() }, ..Party::default() };
        }
    }
    if inv.due_date.is_none() && inv.payment_terms.is_none() {
        inv.due_date = Some(v.date);
        warnings.push("no credit period or due date: the invoice is taken as due on its date".into());
    }

    // Lines.
    let mut line_tax = Vec::new();
    for l in &p.lines {
        let mut t = sg_line(l.class.as_deref(), v.date, &rates, registered)?;
        if p.tax.is_zero() && abroad && t.category == TaxCategory::Sg(SgGst::Sr) {
            t = LineTax::new(TaxCategory::Sg(SgGst::Zr), Decimal::ZERO);
        }
        line_tax.push(t);
    }
    if registered && !p.tax.is_zero() && !line_tax.iter().any(|t| t.category.charges_tax()) {
        warnings.push("the voucher has GST but only zero-rated, exempt or out-of-scope lines; they are written as SR".into());
        let rate = rates.rate(khata_sg::gst::STANDARD_CLASS, v.date).unwrap_or(khata_sg::gst::STANDARD_PERCENT);
        line_tax = line_tax.iter().map(|_| LineTax::new(TaxCategory::Sg(SgGst::Sr), rate)).collect();
    }
    let allowance_tax = allowance_tax(&line_tax, p.discounts.len())?;
    let mut pint = PintInvoice::new(Jurisdiction::Sg, inv, &v.id.0.to_string());
    pint.line_tax = line_tax;
    pint.allowance_tax = allowance_tax;
    pint.finalise();
    Ok((pint, p.tax))
}

/// The PINT SG (InvoiceNow) UBL for a Sales or Credit Note voucher of a
/// Singapore company, with the PINT rules it breaks. Save the XML with
/// `write_gst_json`; send it with [`ap_send`] or the company's
/// InvoiceNow-Ready Solution.
pub fn pint_sg_xml(slot: &Slot, voucher_id: &str) -> ApiResult<PintDto> {
    let book = open_ref(slot)?;
    let v = voucher(book, voucher_id)?;
    let mut warnings = Vec::new();
    let (p, booked) = sg_invoice(book, v, &mut warnings)?;
    compare_tax(&mut warnings, p.invoice.totals.vat, booked, "GST");
    Ok(finish(&p, "PINT-SG", &v.number, warnings))
}

// ------------------------------------------------------------ UAE

/// "DL8.46.2" in an exempt class name ("EXEMPT DL8.46.2").
fn ae_exemption_code(class: &str) -> Option<&'static str> {
    let key = class_key(class);
    pint::AE_EXEMPTION_REASONS.iter().map(|(c, _)| *c).find(|c| key.contains(&class_key(c)))
}

/// The PINT AE category and rate of a line, from its class
/// (`khata_ae::vat::mechanism` and `treatment`): standard → S, zero-rated
/// → Z, "EXEMPT…" → E (with the exemption code named in the class, such as
/// "EXEMPT DL8.46.2"), "OUT-OF-SCOPE…" → O, "RC:…" → AE.
fn ae_line(class: Option<&str>, date: chrono::NaiveDate, rates: &RateSchedule) -> ApiResult<LineTax> {
    let class = class.unwrap_or(khata_ae::vat::STANDARD_CLASS);
    let (mechanism, rate_class) = khata_ae::vat::mechanism(class);
    if class_key(&rate_class).starts_with("OUTOFSCOPE") {
        return Ok(LineTax::new(TaxCategory::Ae(AeVat::O), Decimal::ZERO));
    }
    let rate = rates
        .rate(&rate_class, date)
        .or_else(|| ae_exemption_code(&rate_class).map(|_| Decimal::ZERO))
        .ok_or_else(|| format!("no VAT rate for {rate_class:?} on {date}: add it to the company's rate list"))?;
    if mechanism == khata_ae::vat::Mechanism::ReverseCharge {
        return Ok(LineTax::new(TaxCategory::Ae(AeVat::Ae), rate));
    }
    Ok(match khata_ae::vat::treatment(&rate_class, rate) {
        khata_ae::vat::Treatment::Standard => LineTax::new(TaxCategory::Ae(AeVat::S), rate),
        khata_ae::vat::Treatment::ZeroRated => LineTax::new(TaxCategory::Ae(AeVat::Z), Decimal::ZERO),
        khata_ae::vat::Treatment::Exempt => {
            let code = ae_exemption_code(&rate_class);
            let text = code.and_then(|c| pint::AE_EXEMPTION_REASONS.iter().find(|(k, _)| *k == c)).map(|(_, t)| *t);
            LineTax::new(TaxCategory::Ae(AeVat::E), Decimal::ZERO).with_exemption(code, text)
        }
    })
}

/// The emirate code of a city or emirate name ("Dubai" → DXB).
fn emirate_of(text: &str) -> Option<&'static str> {
    let t = text.trim();
    khata_ae::emirates::emirate(t)
        .or_else(|| khata_ae::emirates::EMIRATES.iter().find(|e| e.name.eq_ignore_ascii_case(t) || e.iso.eq_ignore_ascii_case(t)))
        .map(|e| e.code)
}

fn legal(c: &Card) -> (Option<String>, Option<LegalRegistration>) {
    if let Some(tl) = &c.trade_licence {
        return (Some(tl.clone()), Some(LegalRegistration { kind: LegalIdKind::TradeLicence, authority: c.authority.clone() }));
    }
    if let Some(eid) = &c.emirates_id {
        return (Some(eid.clone()), Some(LegalRegistration { kind: LegalIdKind::EmiratesId, authority: c.authority.clone() }));
    }
    (None, None)
}

fn ae_invoice(book: &Book, v: &Voucher, warnings: &mut Vec<String>) -> ApiResult<(PintInvoice, Decimal)> {
    crate::forex::refuse_foreign_einvoice(v, "A PINT AE invoice")?;
    if book.country() != Some(khata_ae::COUNTRY.code) {
        return Err("PINT AE e-invoices are for companies in the UAE".into());
    }
    let profile = book.profile().ok_or("pick the company's country first")?;
    let rates = rates(book, khata_ae::vat::default_rates())?;
    let p = parse(book, v, warnings);
    let n = narration(&v.narration);
    let mut inv = skeleton(book, v, &p, khata_ae::CURRENCY.code, &n)?;
    let party = party_ledger(book, v, &p);

    // Seller.
    let trn = profile.tax_id.as_deref().map(khata_ae::trn::normalise).filter(|t| !t.is_empty());
    let tin = trn.as_deref().and_then(khata_ae::trn::tin);
    if trn.is_some() && tin.is_none() {
        warnings.push("the company's TRN is not a valid 15-digit TRN".into());
    }
    let c = card(&profile.address);
    let endpoint = c.peppol.as_deref().and_then(pint::parse_participant_id).or_else(|| {
        tin.as_deref().map(|t| ElectronicAddress { value: t.to_owned(), scheme: pint::AE_TIN_SCHEME.into() })
    });
    let (seller_legal_id, seller_legal) = legal(&c);
    if seller_legal_id.is_none() {
        warnings.push("the company's trade licence is not known: add \"Trade licence: …\" and \"Licensing authority: …\" lines to the company address".into());
    }
    let mut a = address(&c.lines, "AE", "");
    a.subdivision = profile.tax_region.clone().unwrap_or_default();
    inv.seller = Party {
        name: book.name.clone(),
        legal_id: seller_legal_id,
        vat_id: trn,
        electronic_address: endpoint,
        address: a,
        contact: contact(&c),
        ..Party::default()
    };
    inv.payment = Some(payment(&c, &v.number).unwrap_or_else(|| {
        warnings.push("no IBAN on the company address: the payment means is \"not defined\" (1)".into());
        PaymentInstructions { means_code: "1".into(), means_text: None, remittance: Some(v.number.clone()), credit_transfer: None }
    }));

    // Buyer.
    let mut abroad = false;
    let mut buyer_legal = None;
    match party {
        Some(l) => {
            let m = l.mailing.clone().unwrap_or_default();
            let bc = card(&m.address);
            let country = mailing_country(Some(l), "AE");
            abroad = country != "AE";
            let trn = l.tax_id.as_deref().map(khata_ae::trn::normalise).filter(|t| !abroad && khata_ae::trn::validate(t).is_ok());
            let endpoint = bc.peppol.as_deref().and_then(pint::parse_participant_id).unwrap_or_else(|| {
                let value = match trn.as_deref().and_then(khata_ae::trn::tin) {
                    Some(tin) => tin,
                    None if abroad => pint::AE_EXPORT_ENDPOINT.to_owned(),
                    None => {
                        warnings.push(format!("{} has no TRN or Peppol id: sent to the \"not yet on e-invoicing\" endpoint {}; issue a paper or PDF tax invoice too", l.name, pint::AE_NOT_ON_EINVOICING_ENDPOINT));
                        pint::AE_NOT_ON_EINVOICING_ENDPOINT.to_owned()
                    }
                };
                ElectronicAddress { value, scheme: pint::AE_TIN_SCHEME.into() }
            });
            let (legal_id, kind) = legal(&bc);
            buyer_legal = kind;
            let mut a = address(&bc.lines, &country, &m.postal_code);
            if !abroad {
                a.subdivision = m
                    .region
                    .as_deref()
                    .and_then(emirate_of)
                    .or_else(|| emirate_of(&a.city))
                    .map(str::to_owned)
                    .unwrap_or_default();
            } else if let Some(r) = m.region.as_deref().filter(|r| !r.trim().is_empty()) {
                a.subdivision = r.trim().to_owned();
            }
            inv.buyer = Party {
                name: l.name.clone(),
                legal_id,
                vat_id: if abroad { l.tax_id.clone().filter(|t| !t.trim().is_empty()) } else { trn },
                electronic_address: Some(endpoint),
                address: a,
                contact: Contact { email: m.email.trim().to_owned(), phone: m.phone.trim().to_owned(), ..Contact::default() },
                ..Party::default()
            };
        }
        None => {
            warnings.push("the voucher has no party ledger: B2C sales are outside UAE e-invoicing for now".into());
            inv.buyer = Party { name: "Cash customer".into(), address: Address { country: "AE".into(), ..Address::default() }, ..Party::default() };
        }
    }
    if !v.kind.eq(&VoucherKind::CreditNote) && inv.due_date.is_none() {
        inv.due_date = Some(v.date);
        warnings.push("no credit period or due date: the invoice is taken as due on its date".into());
    }

    // Lines.
    let mut export = false;
    let mut line_tax = Vec::new();
    for l in &p.lines {
        let mut t = ae_line(l.class.as_deref(), v.date, &rates)?;
        if p.tax.is_zero() && abroad && t.category == TaxCategory::Ae(AeVat::S) {
            t = LineTax::new(TaxCategory::Ae(AeVat::Z), Decimal::ZERO);
        }
        export |= abroad && t.category == TaxCategory::Ae(AeVat::Z);
        line_tax.push(t);
    }
    if !p.tax.is_zero() && !line_tax.iter().any(|t| t.category.charges_tax()) {
        warnings.push("the voucher has VAT but no standard-rated lines; they are written as S".into());
        let rate = rates.rate(khata_ae::vat::STANDARD_CLASS, v.date).unwrap_or(khata_ae::vat::STANDARD_PERCENT);
        line_tax = line_tax.iter().map(|_| LineTax::new(TaxCategory::Ae(AeVat::S), rate)).collect();
    }
    let allowance_tax = allowance_tax(&line_tax, p.discounts.len())?;
    let mut pint = PintInvoice::new(Jurisdiction::Ae, inv, &v.id.0.to_string());
    pint.line_tax = line_tax;
    pint.allowance_tax = allowance_tax;
    pint.ae.transaction.export = export;
    pint.ae.seller_legal = seller_legal;
    pint.ae.buyer_legal = buyer_legal;
    if v.kind == VoucherKind::CreditNote {
        pint.ae.credit_reason = Some(match n.reason.as_deref().map(str::trim) {
            Some(r) => r.to_ascii_uppercase(),
            None => {
                warnings.push("credit note reason taken as goods returned (DL8.61.1.D); put \"Reason: DL8.61.1.x\" in the narration to change it".into());
                "DL8.61.1.D".into()
            }
        });
    }
    pint.finalise();
    Ok((pint, p.tax))
}

/// The PINT AE UBL for a Sales or Credit Note voucher of a UAE company,
/// with the PINT AE rules it breaks. Hand it to the company's Accredited
/// Service Provider (or [`ap_send`]); save it with `write_gst_json`.
pub fn pint_ae_xml(slot: &Slot, voucher_id: &str) -> ApiResult<PintDto> {
    let book = open_ref(slot)?;
    let v = voucher(book, voucher_id)?;
    let mut warnings = Vec::new();
    let (p, booked) = ae_invoice(book, v, &mut warnings)?;
    compare_tax(&mut warnings, p.invoice.totals.vat, booked, "VAT");
    Ok(finish(&p, "PINT-AE", &v.number, warnings))
}

// ------------------------------------------------------------ participant id

/// A Peppol participant identifier.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ParticipantDto {
    /// The EAS scheme ("0195", "0235", "9930").
    pub scheme: String,
    pub value: String,
    /// "scheme:value", as an SMP lists it.
    pub id: String,
    /// Where it came from ("Peppol ID line", "UEN", "TRN", "VAT id").
    pub source: String,
}

/// The EAS scheme of an EU VAT number, by its country prefix (CEF EAS code
/// list, as shipped with PINT SG 1.4.1).
fn eu_vat_scheme(prefix: &str) -> Option<&'static str> {
    Some(match prefix {
        "AT" => "9914",
        "BE" => "9925",
        "BG" => "9926",
        "CY" => "9928",
        "CZ" => "9929",
        "DE" => "9930",
        "EE" => "9931",
        "EL" | "GR" => "9933",
        "ES" => "9920",
        "FR" => "9957",
        "HR" => "9934",
        "HU" => "9910",
        "IE" => "9935",
        "IT" => "0211",
        "LT" => "9937",
        "LU" => "9938",
        "LV" => "9939",
        "MT" => "9943",
        "NL" => "9944",
        "PL" => "9945",
        "PT" => "9946",
        "RO" => "9947",
        "SI" => "9949",
        "SK" => "9950",
        _ => return None,
    })
}

/// The company's Peppol participant id: a "Peppol ID: scheme:value" line
/// of the company address; else Singapore `0195:SGUEN<UEN>`, UAE
/// `0235:<TIN>`, an EU company's VAT number under its country's EAS scheme.
/// Registration in the Peppol directory (SMP) is done by the Access Point.
pub fn peppol_participant_id(slot: &Slot) -> ApiResult<ParticipantDto> {
    let book = open_ref(slot)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let c = card(&profile.address);
    let dto = |e: ElectronicAddress, source: &str| ParticipantDto {
        id: format!("{}:{}", e.scheme, e.value),
        scheme: e.scheme,
        value: e.value,
        source: source.to_owned(),
    };
    if let Some(e) = c.peppol.as_deref().and_then(pint::parse_participant_id) {
        return Ok(dto(e, "Peppol ID line"));
    }
    let tax_id = profile.tax_id.clone().filter(|t| !t.trim().is_empty());
    match book.country() {
        Some("SG") => {
            let uen = c
                .uen
                .clone()
                .or_else(|| tax_id.filter(|g| khata_sg::gst_no::kind(g).is_ok_and(|k| k.is_uen())))
                .ok_or("the company's UEN is not known: add a \"UEN: …\" line to the company address")?;
            Ok(dto(ElectronicAddress { value: pint::sg_endpoint(&uen), scheme: pint::SG_UEN_SCHEME.into() }, "UEN"))
        }
        Some("AE") => {
            let tin = tax_id.as_deref().and_then(khata_ae::trn::tin).ok_or("set the company's TRN first (Company → Tax registration)")?;
            Ok(dto(ElectronicAddress { value: tin, scheme: pint::AE_TIN_SCHEME.into() }, "TRN"))
        }
        Some(country) if khata_de::laender::in_eu(country) => {
            let vat = tax_id.map(|t| t.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase());
            let vat = vat
                .filter(|v| v.len() > 2 && eu_vat_scheme(&v[..2]).is_some())
                .ok_or("the company's VAT number is not known: set it (Company → Tax registration) or add a \"Peppol ID: …\" line")?;
            let scheme = eu_vat_scheme(&vat[..2]).unwrap_or_default();
            Ok(dto(ElectronicAddress { value: vat, scheme: scheme.into() }, "VAT id"))
        }
        _ => Err("no Peppol participant id scheme is known for this company's country; add a \"Peppol ID: scheme:value\" line to the company address".into()),
    }
}

// ------------------------------------------------------------ Factur-X

/// A Factur-X / ZUGFeRD PDF.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacturxDto {
    /// The PDF, base64. Save it with `write_binary_file`.
    pub pdf_base64: String,
    /// The embedded CII XML (EN 16931 profile).
    pub xml: String,
    /// `Factur-X-<number>.pdf`.
    pub file_name: String,
    /// Broken EN 16931 rules: the embedded invoice is not valid until fixed.
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// What this PDF writer does not check or do for full PDF/A-3 and
    /// Factur-X conformance.
    pub conformance_gaps: Vec<String>,
}

/// A VAT identifier with its country prefix, from a tax id as typed.
fn eu_vat_id(id: Option<&str>) -> Option<String> {
    let n: String = id?.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    (n.len() > 4 && n.as_bytes()[..2].iter().all(u8::is_ascii_uppercase) && khata_de::laender::in_eu(&n[..2])).then_some(n)
}

/// The EN 16931 category of a line from its class and rate: "EXEMPT…" → E,
/// "REVERSE…" or "RC…" → AE, 0% → Z, else S.
fn eu_category(class: &str, rate: Decimal) -> VatCategory {
    let key = class_key(class);
    if key.starts_with("EXEMPT") {
        VatCategory::E
    } else if key.starts_with("REVERSE") || key.starts_with("RC") {
        VatCategory::AE
    } else if rate.is_zero() {
        VatCategory::Z
    } else {
        VatCategory::S
    }
}

fn eu_invoice(book: &Book, v: &Voucher, warnings: &mut Vec<String>) -> ApiResult<(Invoice, Decimal)> {
    crate::forex::refuse_foreign_einvoice(v, "An EN 16931 e-invoice")?;
    let home = book.country().unwrap_or_default().to_owned();
    if !khata_de::laender::in_eu(&home) {
        return Err("Factur-X / ZUGFeRD PDFs are for companies in the EU".into());
    }
    let profile = book.profile().ok_or("pick the company's country first")?;
    let rates = rates(book, RateSchedule::default())?;
    let p = parse(book, v, warnings);
    let n = narration(&v.narration);
    let mut inv = skeleton(book, v, &p, "EUR", &n)?;
    let party = party_ledger(book, v, &p);

    let c = card(&profile.address);
    let tax_id = profile.tax_id.clone().filter(|t| !t.trim().is_empty());
    let vat = eu_vat_id(tax_id.as_deref());
    inv.seller = Party {
        name: book.name.clone(),
        vat_id: vat.clone(),
        tax_registration: if vat.is_none() { tax_id } else { None },
        electronic_address: c
            .peppol
            .as_deref()
            .and_then(pint::parse_participant_id)
            .or_else(|| Some(c.email.clone()).filter(|e| !e.is_empty()).map(|e| ElectronicAddress { value: e, scheme: "EM".into() })),
        address: address(&c.lines, &home, ""),
        contact: contact(&c),
        ..Party::default()
    };
    inv.payment = c.iban.as_ref().map(|iban| PaymentInstructions {
        means_code: "58".into(),
        means_text: None,
        remittance: Some(v.number.clone()),
        credit_transfer: Some(CreditTransfer { account: iban.clone(), name: Some(book.name.clone()), bic: c.bic.clone() }),
    });
    let mut buyer_country = home.clone();
    let mut buyer_vat = None;
    match party {
        Some(l) => {
            let m = l.mailing.clone().unwrap_or_default();
            let bc = card(&m.address);
            buyer_country = mailing_country(Some(l), &home);
            buyer_vat = eu_vat_id(l.tax_id.as_deref());
            let mut a = address(&bc.lines, &buyer_country, &m.postal_code);
            if a.city.is_empty() {
                a.city = std::mem::take(&mut a.line2);
            }
            let email = m.email.trim().to_owned();
            inv.buyer = Party {
                name: l.name.clone(),
                vat_id: buyer_vat.clone(),
                electronic_address: bc
                    .peppol
                    .as_deref()
                    .and_then(pint::parse_participant_id)
                    .or_else(|| Some(email.clone()).filter(|e| !e.is_empty()).map(|e| ElectronicAddress { value: e, scheme: "EM".into() })),
                address: a,
                contact: Contact { email, phone: m.phone.trim().to_owned(), ..Contact::default() },
                ..Party::default()
            };
        }
        None => {
            warnings.push("the voucher has no party ledger: the buyer has no address".into());
            inv.buyer = Party { name: "Cash customer".into(), address: Address { country: home.clone(), ..Address::default() }, ..Party::default() };
        }
    }
    let abroad = buyer_country != home;
    let eu_business = abroad && khata_de::laender::in_eu(&buyer_country) && buyer_vat.is_some();
    let mut exempt = false;
    let standard = rates.rate("STANDARD", v.date);
    for (l, raw) in inv.lines.iter_mut().zip(&p.lines) {
        let class = raw.class.clone().unwrap_or_else(|| "STANDARD".into());
        let rate = rates.rate(&class, v.date).or(if class_key(&class).starts_with("EXEMPT") { Some(Decimal::ZERO) } else { None });
        let rate = rate.ok_or_else(|| format!("no VAT rate for {class:?} on {}: add it to the company's rate list", v.date))?;
        let mut cat = eu_category(&class, rate);
        if p.tax.is_zero() && cat == VatCategory::S && abroad {
            cat = match (eu_business, raw.goods) {
                (true, true) => VatCategory::K,
                (true, false) => VatCategory::AE,
                (false, true) => VatCategory::G,
                (false, false) => VatCategory::O,
            };
        }
        exempt |= cat == VatCategory::E;
        l.category = cat;
        l.rate = match cat {
            VatCategory::O => None,
            VatCategory::S => Some(rate.normalize()),
            _ => Some(Decimal::ZERO),
        };
    }
    if !p.tax.is_zero() && !inv.lines.iter().any(|l| l.category == VatCategory::S) {
        let rate = standard.ok_or("the voucher has VAT but no line has a VAT rate; add a STANDARD rate to the company's rate list")?;
        warnings.push("the voucher has VAT but no standard-rated lines; they are written as standard-rated".into());
        for l in &mut inv.lines {
            l.category = VatCategory::S;
            l.rate = Some(rate);
        }
    }
    // Discounts take the lines' category when they all share one.
    if !inv.allowances_charges.is_empty() {
        let first = inv.lines.first().map(|l| (l.category, l.rate)).ok_or("the voucher has a discount but no lines")?;
        if inv.lines.iter().any(|l| (l.category, l.rate) != first) {
            return Err("a discount on an invoice with more than one VAT rate can't be split; give each line its own discounted price".into());
        }
        for a in &mut inv.allowances_charges {
            (a.category, a.rate) = first;
        }
    }
    if exempt {
        inv.vat_breakdown.push(VatBreakdown::exemption(VatCategory::E, Some(Decimal::ZERO), None, Some("Exempt from VAT")));
    }
    if inv.delivery_date.is_none() && inv.lines.iter().any(|l| l.category == VatCategory::K) {
        inv.delivery_date = Some(v.date);
        inv.deliver_to_country = Some(buyer_country.clone());
        warnings.push("the delivery date of the intra-community supply is taken to be the invoice date".into());
    }
    inv.finalise();
    Ok((inv, p.tax))
}

/// A Factur-X / ZUGFeRD PDF (profile EN 16931) for a Sales or Credit Note
/// voucher of a company in the EU: a text invoice as PDF/A-3b with the CII
/// XML embedded as `factur-x.xml`. The XML follows EN 16931, not a national
/// CIUS: a German public buyer needs an XRechnung instead.
pub fn facturx_pdf(slot: &Slot, voucher_id: &str) -> ApiResult<FacturxDto> {
    let book = open_ref(slot)?;
    let v = voucher(book, voucher_id)?;
    let mut warnings = Vec::new();
    let (inv, booked) = eu_invoice(book, v, &mut warnings)?;
    compare_tax(&mut warnings, inv.totals.vat, booked, "VAT");
    let mut errors = Vec::new();
    for x in validate_for(&inv, Specification::En16931) {
        if x.warning { warnings.push(x.to_string()) } else { errors.push(x.to_string()) }
    }
    let xml = cii::write(&inv, Specification::En16931);
    let opts = facturx::PdfOptions { created: chrono::Utc::now().naive_utc(), profile: facturx::Profile::En16931 };
    let pdf = facturx::pdf(&inv, &xml, &opts);
    Ok(FacturxDto {
        pdf_base64: base64::engine::general_purpose::STANDARD.encode(pdf),
        xml,
        file_name: format!("Factur-X-{}.pdf", safe(&v.number)),
        errors,
        warnings,
        conformance_gaps: facturx::CONFORMANCE_GAPS.iter().map(|s| (*s).to_owned()).collect(),
    })
}

/// Write a PDF the user picked a path for (`base64` content). Only `.pdf`
/// names are accepted.
pub fn write_binary_file(path: &str, contents_base64: &str) -> ApiResult<()> {
    let bytes = decode_pdf(contents_base64)?;
    let p = Path::new(path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return Err("save this file with a .pdf name".into());
    }
    std::fs::write(p, bytes).map_err(|e| format!("cannot write {path}: {e}"))
}

pub(crate) fn decode_pdf(contents_base64: &str) -> ApiResult<Vec<u8>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(contents_base64.trim())
        .map_err(|e| format!("the file contents are not base64: {e}"))?;
    if !bytes.starts_with(b"%PDF-") {
        return Err("only PDF files are written here".into());
    }
    Ok(bytes)
}

/// Phones: write the PDF to a picked content:// or file:// URI.
#[cfg(mobile)]
pub(crate) fn write_binary_uri(app: &tauri::AppHandle, path: &str, contents_base64: &str) -> Option<ApiResult<()>> {
    use std::io::Write;
    use tauri_plugin_fs::{FsExt, OpenOptions};
    let uri = crate::phone_files::uri(path)?;
    Some((|| {
        if !crate::phone_files::name_allowed(&uri, &["pdf"]) {
            return Err("save this file with a .pdf name".to_owned());
        }
        let bytes = decode_pdf(contents_base64)?;
        let shown = uri.to_string();
        let mut opts = OpenOptions::new();
        opts.write(true).truncate(true).create(true);
        let written = (|| {
            let mut f = app.fs().open(uri.clone(), opts)?;
            f.write_all(&bytes)?;
            f.flush()
        })();
        #[cfg(target_os = "ios")]
        let _ = app.fs().stop_accessing_security_scoped_resource(uri);
        written.map_err(|e| format!("cannot write {shown}: {e}"))
    })())
}

// ------------------------------------------------------------ Access Point

/// The settings file in `<app data>/peppol`.
pub const AP_SETTINGS_FILE: &str = "access-point.json";

/// What the settings screen may see: the key only as "set or not".
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApSettingsDto {
    /// The URL the UBL is POSTed to.
    pub endpoint: String,
    /// The request header carrying the key ("Authorization", "X-API-Key").
    pub auth_header: String,
    /// What goes before the key in that header ("Bearer ", or nothing).
    pub auth_prefix: String,
    pub api_key_set: bool,
    /// A reminder that every provider's API differs.
    pub note: String,
}

/// New settings; `api_key` `None` keeps the stored key, `Some("")` clears it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ApSettingsInput {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub auth_header: Option<String>,
    #[serde(default)]
    pub auth_prefix: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct ApVault {
    endpoint: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    auth_header: String,
    #[serde(default)]
    auth_prefix: String,
}

const AP_NOTE: &str = "Access Point and ASP APIs differ: this sends the UBL as an HTTP POST with your key in one header. Check that your provider accepts that; KhataOS is not certified with any provider.";

fn load_vault(dir: &Path) -> ApiResult<ApVault> {
    match std::fs::read_to_string(dir.join(AP_SETTINGS_FILE)) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("the Access Point settings are damaged: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ApVault::default()),
        Err(e) => Err(format!("cannot read the Access Point settings: {e}")),
    }
}

/// Write the settings readable by this user only (folder `0700`, file
/// `0600`), through a temporary file.
fn save_vault(dir: &Path, vault: &ApVault) -> ApiResult<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create the app data folder: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let json = serde_json::to_vec_pretty(vault).map_err(err)?;
    let tmp = dir.join(format!("{AP_SETTINGS_FILE}.tmp"));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(&tmp)
        .and_then(|mut f| f.write_all(&json).and_then(|_| f.sync_all()))
        .map_err(|e| format!("cannot save the Access Point settings: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, dir.join(AP_SETTINGS_FILE)).map_err(|e| format!("cannot save the Access Point settings: {e}"))
}

fn dto(v: &ApVault) -> ApSettingsDto {
    ApSettingsDto {
        endpoint: v.endpoint.clone(),
        auth_header: if v.auth_header.is_empty() { "Authorization".into() } else { v.auth_header.clone() },
        auth_prefix: v.auth_prefix.clone(),
        api_key_set: !v.api_key.is_empty(),
        note: AP_NOTE.into(),
    }
}

/// HTTPS only; plain HTTP just for a mock on this computer.
fn check_endpoint(url: &str) -> ApiResult<String> {
    let url = url.trim();
    let local = ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"].iter().any(|p| url.starts_with(p));
    if url.is_empty() {
        return Err("enter the Access Point's URL for sending documents".into());
    }
    if !(url.starts_with("https://") || local) || url.contains(char::is_whitespace) {
        return Err("the Access Point URL must start with https://".into());
    }
    Ok(url.to_owned())
}

fn check_header(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok("Authorization".into());
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(format!("{name:?} is not a valid HTTP header name"));
    }
    Ok(name.to_owned())
}

pub fn ap_settings(dir: &Path) -> ApiResult<ApSettingsDto> {
    Ok(dto(&load_vault(dir)?))
}

pub fn set_ap_settings(dir: &Path, input: &ApSettingsInput) -> ApiResult<ApSettingsDto> {
    let mut v = load_vault(dir)?;
    v.endpoint = check_endpoint(&input.endpoint)?;
    if let Some(k) = &input.api_key {
        if k.contains(['\r', '\n']) {
            return Err("the API key can't contain line breaks".into());
        }
        v.api_key = k.trim().to_owned();
    }
    if let Some(h) = &input.auth_header {
        v.auth_header = check_header(h)?;
    }
    if let Some(p) = &input.auth_prefix {
        if p.contains(['\r', '\n']) {
            return Err("the key prefix can't contain line breaks".into());
        }
        v.auth_prefix = p.clone();
    }
    save_vault(dir, &v)?;
    Ok(dto(&v))
}

/// What the Access Point answered.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApSendDto {
    pub status: u16,
    /// A 2xx status: the Access Point took the document. Delivery to the
    /// buyer (and, in the UAE, reporting to the FTA) is confirmed later,
    /// through the provider.
    pub accepted: bool,
    /// The start of the answer's body (at most 4,000 characters).
    pub response: String,
    pub file_name: String,
    pub sender: Option<String>,
    pub receiver: Option<String>,
    pub document_type_id: String,
}

/// Build the PINT document (`document` "pint-sg" or "pint-ae") for a
/// voucher and POST it to the configured Access Point:
/// `Content-Type: application/xml`, the key in the configured header, and
/// informative `X-Peppol-Sender-ID`, `X-Peppol-Receiver-ID`,
/// `X-Peppol-Document-Type-ID` and `X-Peppol-Process-ID` headers. A
/// document that breaks PINT rules is not sent. The company lock is held
/// only while the XML is built.
pub fn ap_send(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, document: &str) -> ApiResult<ApSendDto> {
    let vault = load_vault(dir)?;
    let endpoint = check_endpoint(&vault.endpoint).map_err(|_| "set up the Access Point first (URL and API key)".to_owned())?;
    let (doc, jurisdiction, credit) = {
        let slot = company.lock().map_err(|_| "the company is busy; try again".to_owned())?;
        let doc = match document.trim().to_ascii_lowercase().as_str() {
            "pint-sg" | "sg" => pint_sg_xml(&slot, voucher_id)?,
            "pint-ae" | "ae" => pint_ae_xml(&slot, voucher_id)?,
            other => return Err(format!("unknown document {other:?} (pint-sg or pint-ae)")),
        };
        let book = open_ref(&slot)?;
        let credit = voucher(book, voucher_id)?.kind == VoucherKind::CreditNote;
        let j = if doc.customization_id == pint::PINT_SG_ID { Jurisdiction::Sg } else { Jurisdiction::Ae };
        (doc, j, credit)
    };
    if !doc.errors.is_empty() {
        return Err(format!("the document breaks PINT rules, so it was not sent: {}", doc.errors.join(" ")));
    }
    let document_type_id = jurisdiction.document_type_id(credit);
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_global(Some(std::time::Duration::from_secs(90)))
        .build()
        .new_agent();
    let mut req = agent
        .post(&endpoint)
        .header("Content-Type", "application/xml")
        .header("Accept", "application/json, application/xml, text/plain")
        .header("X-Peppol-Document-Type-ID", &document_type_id)
        .header("X-Peppol-Process-ID", jurisdiction.profile_id());
    if let Some(s) = &doc.sender {
        req = req.header("X-Peppol-Sender-ID", s);
    }
    if let Some(r) = &doc.receiver {
        req = req.header("X-Peppol-Receiver-ID", r);
    }
    if !vault.api_key.is_empty() {
        let header = if vault.auth_header.is_empty() { "Authorization" } else { vault.auth_header.as_str() };
        let prefix = if vault.auth_header.is_empty() && vault.auth_prefix.is_empty() { "Bearer" } else { vault.auth_prefix.trim() };
        let value = if prefix.is_empty() { vault.api_key.clone() } else { format!("{prefix} {}", vault.api_key) };
        req = req.header(header, &value);
    }
    let mut resp = req.send(doc.xml.as_str()).map_err(|e| match e {
        ureq::Error::Timeout(_) => "the Access Point did not answer in time; try again".to_owned(),
        ureq::Error::HostNotFound => "couldn't find the Access Point; check the internet connection and the URL".to_owned(),
        e => format!("couldn't reach the Access Point: {e}"),
    })?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().unwrap_or_default();
    Ok(ApSendDto {
        status,
        accepted: (200..300).contains(&status),
        response: body.chars().take(4000).collect(),
        file_name: doc.file_name,
        sender: doc.sender,
        receiver: doc.receiver,
        document_type_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, ledger, voucher as voucher_input};
    use crate::api::{StockLineInput, cancel_voucher, create_company, post_voucher, set_country};
    use crate::masters::{LedgerInput, StockItemInput, alter_ledger, get_ledger, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::{Mailing, TaxSide};

    /// `PEPPOL_SAMPLE_DIR=/some/dir cargo test -p khataos-desktop peppol`
    /// keeps the documents the tests make, for the official validators.
    fn dump(name: &str, bytes: &[u8]) {
        if let Some(dir) = std::env::var_os("PEPPOL_SAMPLE_DIR") {
            std::fs::write(std::path::Path::new(&dir).join(name.replace('/', "_")), bytes).unwrap();
        }
    }

    fn items(item_id: &str, qty: &str, rate: &str) -> Vec<StockLineInput> {
        vec![StockLineInput {
            item_id: item_id.into(),
            qty: qty.into(),
            rate: rate.into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }]
    }

    fn item(slot: &mut Slot, name: &str, unit: &str, class: &str) -> String {
        let input: StockItemInput =
            serde_json::from_value(serde_json::json!({ "name": name, "unit_id": unit, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    fn party(slot: &mut Slot, name: &str, tax_id: Option<&str>, m: Mailing) -> String {
        let id = ledger(slot, name, "Sundry Debtors", "");
        let l = get_ledger(slot, &id).unwrap();
        alter_ledger(slot, &id, &LedgerInput {
            name: l.name,
            group_id: crate::api::tests::group_id(slot, "Sundry Debtors"),
            opening: String::new(),
            credit_days: Some(30),
            tax_id: tax_id.map(str::to_owned),
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: Some(m),
            tax_class: None,
            pan: None,
            tds: None,
        })
        .unwrap();
        id
    }

    fn post(slot: &mut Slot, kind: &str, date: &str, narration: &str, entries: Vec<crate::api::EntryInput>) -> String {
        let mut v = voucher_input(kind, date, entries);
        v.narration = narration.into();
        post_voucher(slot, v).unwrap().id
    }

    fn stock_line(ledger: &str, side: DrCr, amount: &str, item_id: &str, qty: &str, rate: &str) -> crate::api::EntryInput {
        let mut e = entry(ledger, side, amount);
        e.items = items(item_id, qty, rate);
        e
    }

    struct Sg {
        slot: Slot,
        _f: TempFile,
        sale: String,
        export: String,
        credit: String,
    }

    fn sg_books(name: &str) -> Sg {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Lion Trading Pte Ltd").unwrap();
        set_country(&mut slot, Some(&f.0), "SG").unwrap();
        set_company_tax(&mut slot, "M2-1234567-K", "").unwrap();
        set_company_address(
            &mut slot,
            "1 Raffles Place\n#20-01 One Raffles Place\nSingapore 048616\nUEN: 201912345K\nTel: +65 6123 4567\nEmail: billing@lion.sg\nBank account: 123-456789-0\nBIC: DBSSSGSG",
        )
        .unwrap();
        for (class, from, percent) in [("STANDARD", "2024-01-01", "9"), ("ZERO", "2018-01-01", "0"), ("EXEMPT", "2018-01-01", "0")] {
            set_tax_rate(&mut slot, class, from, percent).unwrap();
        }
        let m = |a: &str, postal: &str, country: &str| Mailing {
            address: a.into(),
            postal_code: postal.into(),
            country: Some(country.into()),
            email: "ap@buyer.sg".into(),
            ..Mailing::default()
        };
        let merlion = party(&mut slot, "Merlion Buyer Pte Ltd", Some("200254321Z"), m("10 Anson Road\n#05-01", "079903", "SG"));
        let sydney = party(&mut slot, "Sydney Imports Pty Ltd", None, m("1 George St\nSydney NSW\nPeppol ID: 0151:12345678901", "2000", "AU"));
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let gst = ledger(&mut slot, "Output GST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &gst, "GST", TaxSide::Output).unwrap();
        let pcs = save_unit(&mut slot, "", "pcs", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &pcs, "STANDARD");
        let book = item(&mut slot, "Training course", &pcs, "ES33");
        set_tax_rate(&mut slot, "ES33", "2018-01-01", "0").unwrap();
        let mut lines = stock_line(&sales, DrCr::Cr, "2100", &laptop, "2", "1000");
        lines.items.extend(items(&book, "1", "100"));
        let sale = post(&mut slot, "Sales", "2026-05-04", "PO: PO-778\nThank you for your business", vec![
            entry(&merlion, DrCr::Dr, "2280"),
            lines,
            entry(&gst, DrCr::Cr, "180"),
        ]);
        let export = post(&mut slot, "Sales", "2026-05-05", "Your reference: SYD-1", vec![
            entry(&sydney, DrCr::Dr, "1000"),
            stock_line(&sales, DrCr::Cr, "1000", &laptop, "1", "1000"),
        ]);
        let credit = post(&mut slot, "CreditNote", "2026-05-10", "PO: PO-778\nOne laptop returned", vec![
            entry(&merlion, DrCr::Cr, "1090"),
            stock_line(&sales, DrCr::Dr, "1000", &laptop, "1", "1000"),
            entry(&gst, DrCr::Dr, "90"),
        ]);
        let cancelled = post(&mut slot, "Sales", "2026-05-11", "", vec![
            entry(&merlion, DrCr::Dr, "1090"),
            stock_line(&sales, DrCr::Cr, "1000", &laptop, "1", "1000"),
            entry(&gst, DrCr::Cr, "90"),
        ]);
        cancel_voucher(&mut slot, &cancelled).unwrap();
        Sg { slot, _f: f, sale, export, credit }
    }

    #[test]
    fn address_and_card_parsing() {
        let c = card("1 Raffles Place\nSingapore 048616\nUEN: 201912345K\nPeppol ID: 0195:SGUEN201912345K\nIBAN: DE02 1203 0000 0000 2020 51\nTrade licence: 12345\nLicensing authority: Dubai DET");
        assert_eq!(c.lines, ["1 Raffles Place", "Singapore 048616"]);
        assert_eq!((c.uen.as_deref(), c.peppol.as_deref(), c.iban.as_deref()), (Some("201912345K"), Some("0195:SGUEN201912345K"), Some("DE02120300000000202051")));
        assert_eq!((c.trade_licence.as_deref(), c.authority.as_deref()), (Some("12345"), Some("Dubai DET")));
        let a = address(&c.lines, "SG", "");
        assert_eq!((a.line1.as_str(), a.city.as_str(), a.postal_code.as_str(), a.line2.as_str()), ("1 Raffles Place", "Singapore", "048616", ""));
        let a = address(&["Hauptstraße 1".into(), "10115 Berlin".into(), "Deutschland".into()], "DE", "");
        assert_eq!((a.line1.as_str(), a.city.as_str(), a.postal_code.as_str()), ("Hauptstraße 1", "Berlin", "10115"));
        let a = address(&["Office 12, Al Fahidi St".into(), "Bur Dubai".into(), "Dubai".into()], "AE", "");
        assert_eq!((a.line1.as_str(), a.line2.as_str(), a.city.as_str()), ("Office 12, Al Fahidi St", "Bur Dubai", "Dubai"));
        let n = narration("PO: 778\nBuyer reference: R-1\nReason: dl8.61.1.a\nThanks");
        assert_eq!((n.order.as_deref(), n.buyer_reference.as_deref(), n.reason.as_deref(), n.notes.len()), (Some("778"), Some("R-1"), Some("dl8.61.1.a"), 1));
        assert_eq!(emirate_of("Dubai"), Some("DXB"));
        assert_eq!(emirate_of("AE-AZ"), Some("AUH"));
        assert_eq!(ae_exemption_code("EXEMPT DL8.46.2"), Some("DL8.46.2"));
        assert_eq!(eu_vat_scheme("DE"), Some("9930"));
        assert_eq!(eu_vat_id(Some("de 136 695 976")).as_deref(), Some("DE136695976"));
        assert_eq!(eu_vat_id(Some("2893081508152")), None);
    }

    #[test]
    fn pint_sg_from_a_sale() {
        let b = sg_books("pint-sg");
        let x = pint_sg_xml(&b.slot, &b.sale).unwrap();
        dump(&x.file_name, x.xml.as_bytes());
        assert!(x.errors.is_empty(), "{:?}", x.errors);
        assert_eq!(x.customization_id, "urn:peppol:pint:billing-1@sg-1");
        assert_eq!((x.sender.as_deref(), x.receiver.as_deref()), (Some("0195:SGUEN201912345K"), Some("0195:SGUEN200254321Z")));
        let number = crate::api::get_voucher(&b.slot, &b.sale).unwrap().number;
        assert_eq!(x.file_name, format!("PINT-SG-{}.xml", safe(&number)));
        for s in [
            &format!("<cbc:UUID>{}</cbc:UUID>", b.sale),
            "<cbc:EndpointID schemeID=\"0195\">SGUEN201912345K</cbc:EndpointID>",
            "<cbc:StreetName>1 Raffles Place</cbc:StreetName>\n        <cbc:AdditionalStreetName>#20-01 One Raffles Place</cbc:AdditionalStreetName>\n        <cbc:CityName>Singapore</cbc:CityName>\n        <cbc:PostalZone>048616</cbc:PostalZone>",
            "<cbc:CompanyID>M21234567K</cbc:CompanyID>",
            "<cbc:RegistrationName>Lion Trading Pte Ltd</cbc:RegistrationName>\n        <cbc:CompanyID>201912345K</cbc:CompanyID>",
            "<cbc:CompanyID>200254321Z</cbc:CompanyID>\n        <cac:TaxScheme>\n          <cbc:ID>GST</cbc:ID>",
            "<cac:OrderReference>\n    <cbc:ID>PO-778</cbc:ID>",
            "<cbc:Note>Thank you for your business</cbc:Note>",
            "<cbc:DueDate>2026-06-03</cbc:DueDate>",
            "<cbc:PaymentMeansCode>30</cbc:PaymentMeansCode>",
            "<cbc:ID>123-456789-0</cbc:ID>",
            "<cbc:ID>SR</cbc:ID>\n        <cbc:Percent>9</cbc:Percent>",
            "<cbc:ID>ES33</cbc:ID>\n        <cbc:Percent>0</cbc:Percent>",
            "<cbc:TaxAmount currencyID=\"SGD\">180.00</cbc:TaxAmount>",
            "<cbc:PayableAmount currencyID=\"SGD\">2280.00</cbc:PayableAmount>",
        ] {
            assert!(x.xml.contains(s), "missing {s} in\n{}", x.xml);
        }
        assert!(!x.warnings.iter().any(|w| w.contains("booked")), "{:?}", x.warnings);
    }

    #[test]
    fn pint_sg_export_credit_note_and_refusals() {
        let b = sg_books("pint-sg-2");
        let x = pint_sg_xml(&b.slot, &b.export).unwrap();
        assert!(x.errors.is_empty(), "{:?}", x.errors);
        assert_eq!(x.receiver.as_deref(), Some("0151:12345678901"));
        assert!(x.xml.contains("<cbc:ID>ZR</cbc:ID>") && x.xml.contains("<cbc:BuyerReference>SYD-1</cbc:BuyerReference>"));
        assert!(x.xml.contains("<cbc:IdentificationCode>AU</cbc:IdentificationCode>"));
        let c = pint_sg_xml(&b.slot, &b.credit).unwrap();
        assert!(c.errors.is_empty(), "{:?}", c.errors);
        assert!(c.xml.contains("<CreditNote ") && c.xml.contains("<cbc:Note>One laptop returned</cbc:Note>"));
        let book = open_ref(&b.slot).unwrap();
        let cancelled = book.vouchers().find(|v| v.cancelled).unwrap().id.0.to_string();
        assert!(pint_sg_xml(&b.slot, &cancelled).unwrap_err().contains("cancelled"));
        assert!(pint_ae_xml(&b.slot, &b.sale).unwrap_err().contains("UAE"));
        assert!(facturx_pdf(&b.slot, &b.sale).unwrap_err().contains("EU"));
        let p = peppol_participant_id(&b.slot).unwrap();
        assert_eq!((p.id.as_str(), p.source.as_str()), ("0195:SGUEN201912345K", "UEN"));
    }

    const TRN: &str = "100123456700003";
    const BUYER_TRN: &str = "100765432100003";

    struct Ae {
        slot: Slot,
        _f: TempFile,
        sale: String,
        export: String,
        credit: String,
    }

    fn ae_books(name: &str) -> Ae {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Falcon Supplies LLC").unwrap();
        set_country(&mut slot, Some(&f.0), "AE").unwrap();
        set_company_tax(&mut slot, TRN, "DXB").unwrap();
        set_company_address(
            &mut slot,
            "Office 12, Al Fahidi Street\nBur Dubai\nDubai\nTrade licence: 112345678900003\nLicensing authority: Dubai DET\nIBAN: AE070331234567890123456",
        )
        .unwrap();
        for (class, percent) in [("STANDARD", "5"), ("ZERO", "0"), ("EXEMPT DL8.46.2", "0")] {
            set_tax_rate(&mut slot, class, "2018-01-01", percent).unwrap();
        }
        let oasis = party(&mut slot, "Oasis Retail LLC", Some(BUYER_TRN), Mailing {
            address: "Hamdan Street\nAbu Dhabi\nTrade licence: CN-1234567\nLicensing authority: Abu Dhabi DED".into(),
            region: Some("AUH".into()),
            country: Some("AE".into()),
            ..Mailing::default()
        });
        let mumbai = party(&mut slot, "Mumbai Imports Pvt Ltd", None, Mailing {
            address: "1 Marine Drive\nMumbai".into(),
            region: Some("MH".into()),
            postal_code: "400001".into(),
            country: Some("IN".into()),
            ..Mailing::default()
        });
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let rent = ledger(&mut slot, "Residential rent", "Indirect Incomes", "");
        let vat = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &vat, "VAT", TaxSide::Output).unwrap();
        let rent_ledger = get_ledger(&slot, &rent).unwrap();
        let incomes = crate::api::tests::group_id(&slot, "Indirect Incomes");
        alter_ledger(&mut slot, &rent, &LedgerInput {
            name: rent_ledger.name,
            group_id: incomes,
            opening: String::new(),
            credit_days: None,
            tax_id: None,
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: None,
            tax_class: Some("EXEMPT DL8.46.2".into()),
            pan: None,
            tds: None,
        })
        .unwrap();
        let pcs = save_unit(&mut slot, "", "pcs", "", 0).unwrap().id;
        let phone = item(&mut slot, "Phone case", &pcs, "STANDARD");
        let sale = post(&mut slot, "Sales", "2027-01-12", "", vec![
            entry(&oasis, DrCr::Dr, "1250"),
            stock_line(&sales, DrCr::Cr, "1000", &phone, "100", "10"),
            entry(&rent, DrCr::Cr, "200"),
            entry(&vat, DrCr::Cr, "50"),
        ]);
        let export = post(&mut slot, "Sales", "2027-01-13", "", vec![
            entry(&mumbai, DrCr::Dr, "500"),
            stock_line(&sales, DrCr::Cr, "500", &phone, "50", "10"),
        ]);
        let credit = post(&mut slot, "CreditNote", "2027-01-20", "Reason: DL8.61.1.D\nTen cases returned", vec![
            entry(&oasis, DrCr::Cr, "105"),
            stock_line(&sales, DrCr::Dr, "100", &phone, "10", "10"),
            entry(&vat, DrCr::Dr, "5"),
        ]);
        Ae { slot, _f: f, sale, export, credit }
    }

    #[test]
    fn pint_ae_from_a_sale_an_export_and_a_credit_note() {
        let b = ae_books("pint-ae");
        let x = pint_ae_xml(&b.slot, &b.sale).unwrap();
        dump(&x.file_name, x.xml.as_bytes());
        assert!(x.errors.is_empty(), "{:?}", x.errors);
        assert_eq!((x.sender.as_deref(), x.receiver.as_deref()), (Some("0235:1001234567"), Some("0235:1007654321")));
        for s in [
            "<cbc:CustomizationID>urn:peppol:pint:billing-1@ae-1</cbc:CustomizationID>",
            "<cbc:ProfileExecutionID>00000000</cbc:ProfileExecutionID>",
            "<cbc:CountrySubentity>DXB</cbc:CountrySubentity>",
            "<cbc:CountrySubentity>AUH</cbc:CountrySubentity>",
            "<cbc:CompanyID schemeAgencyID=\"TL\" schemeAgencyName=\"Dubai DET\">112345678900003</cbc:CompanyID>",
            "<cbc:CompanyID schemeAgencyID=\"TL\" schemeAgencyName=\"Abu Dhabi DED\">CN-1234567</cbc:CompanyID>",
            "<cbc:ID>E</cbc:ID>\n        <cbc:TaxExemptionReasonCode>DL8.46.2</cbc:TaxExemptionReasonCode>",
            "<cbc:TaxAmount currencyID=\"AED\">50.00</cbc:TaxAmount>",
            "<cbc:PayableAmount currencyID=\"AED\">1250.00</cbc:PayableAmount>",
            "<cbc:DueDate>2027-02-11</cbc:DueDate>",
            "<cbc:ID>AE070331234567890123456</cbc:ID>",
        ] {
            assert!(x.xml.contains(s), "missing {s} in\n{}", x.xml);
        }
        let e = pint_ae_xml(&b.slot, &b.export).unwrap();
        assert!(e.errors.is_empty(), "{:?}", e.errors);
        assert_eq!(e.receiver.as_deref(), Some("0235:9900000099"));
        assert!(e.xml.contains("<cbc:ProfileExecutionID>00000001</cbc:ProfileExecutionID>"));
        dump("PINT-AE-export.xml", e.xml.as_bytes());
        let c = pint_ae_xml(&b.slot, &b.credit).unwrap();
        dump("PINT-AE-credit.xml", c.xml.as_bytes());
        assert!(c.xml.contains("<cbc:ResponseCode>DL8.61.1.D</cbc:ResponseCode>"), "{}", c.xml);
        // The credit note names no invoice it corrects: ibr-055-ae.
        assert!(c.errors.iter().any(|e| e.contains("ibr-055-ae")), "{:?}", c.errors);
        let p = peppol_participant_id(&b.slot).unwrap();
        assert_eq!(p.id, "0235:1001234567");
    }

    #[test]
    fn facturx_from_a_german_sale() {
        let f = TempFile::new("facturx-de");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Muster GmbH").unwrap();
        set_country(&mut slot, Some(&f.0), "DE").unwrap();
        set_company_tax(&mut slot, "DE136695976", "BE").unwrap();
        set_company_address(&mut slot, "Hauptstraße 1\n10115 Berlin\nTel.: +49 30 123456\nE-Mail: rechnung@muster.de\nIBAN: DE02120300000000202051").unwrap();
        for (class, percent) in [("STANDARD", "19"), ("REDUCED", "7")] {
            set_tax_rate(&mut slot, class, "2007-01-01", percent).unwrap();
        }
        let kunde = party(&mut slot, "Kunde AG", Some("DE129304291"), Mailing {
            address: "Marktplatz 5\nMünchen".into(),
            postal_code: "80331".into(),
            country: Some("DE".into()),
            email: "ap@kunde.de".into(),
            ..Mailing::default()
        });
        let sales = ledger(&mut slot, "Umsatzerlöse", "Sales Accounts", "");
        let ust = ledger(&mut slot, "Umsatzsteuer", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &ust, "USt", TaxSide::Output).unwrap();
        let stk = save_unit(&mut slot, "", "Stk", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &stk, "STANDARD");
        let id = post(&mut slot, "Sales", "2026-03-02", "Danke für Ihren Auftrag.", vec![
            entry(&kunde, DrCr::Dr, "2380"),
            stock_line(&sales, DrCr::Cr, "2000", &laptop, "2", "1000"),
            entry(&ust, DrCr::Cr, "380"),
        ]);
        let x = facturx_pdf(&slot, &id).unwrap();
        assert!(x.errors.is_empty(), "{:?}", x.errors);
        assert!(x.file_name.starts_with("Factur-X-") && x.file_name.ends_with(".pdf"));
        assert!(x.xml.contains("<ram:ID>urn:cen.eu:en16931:2017</ram:ID>"), "{}", x.xml);
        assert!(x.xml.contains("<ram:GrandTotalAmount>2380.00</ram:GrandTotalAmount>"));
        assert!(x.xml.contains("<ram:CityName>München</ram:CityName>"));
        let pdf = base64::engine::general_purpose::STANDARD.decode(&x.pdf_base64).unwrap();
        dump("facturx-de.pdf", &pdf);
        assert!(pdf.starts_with(b"%PDF-1.7"));
        assert!(!x.conformance_gaps.is_empty());
        // Saving: only .pdf, only PDF bytes.
        let out = std::env::temp_dir().join(format!("khata-facturx-{}.pdf", std::process::id()));
        write_binary_file(out.to_str().unwrap(), &x.pdf_base64).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), pdf);
        let _ = std::fs::remove_file(&out);
        assert!(write_binary_file("/tmp/x.exe", &x.pdf_base64).unwrap_err().contains(".pdf"));
        let not_pdf = base64::engine::general_purpose::STANDARD.encode(b"MZ\x90\x00");
        assert!(write_binary_file("/tmp/x.pdf", &not_pdf).unwrap_err().contains("PDF"));
        let p = peppol_participant_id(&slot).unwrap();
        assert_eq!(p.id, "9930:DE136695976");
    }

    /// A one-shot HTTP server on 127.0.0.1: it records the request and
    /// answers `status` with `body`.
    fn mock_ap(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/v1/documents", listener.local_addr().unwrap().port());
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                head.push_str(&line);
            }
            let mut body_in = vec![0u8; length];
            reader.read_exact(&mut body_in).unwrap();
            let mut s = stream;
            write!(s, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            s.flush().unwrap();
            format!("{head}\r\n{}", String::from_utf8(body_in).unwrap())
        });
        (url, handle)
    }

    #[test]
    fn access_point_settings_are_private_and_sending_posts_the_ubl() {
        let b = sg_books("pint-ap");
        let dir = std::env::temp_dir().join(format!("khata-peppol-ap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let none = ap_settings(&dir).unwrap();
        assert!(!none.api_key_set && none.endpoint.is_empty() && none.note.contains("differ"));
        let company = Mutex::new(b.slot);
        assert!(ap_send(&dir, &company, &b.sale, "pint-sg").unwrap_err().contains("set up the Access Point"));
        assert!(set_ap_settings(&dir, &ApSettingsInput { endpoint: "http://ap.example.com/send".into(), api_key: None, auth_header: None, auth_prefix: None }).is_err());
        assert!(set_ap_settings(&dir, &ApSettingsInput { endpoint: "https://ap.example.com".into(), api_key: None, auth_header: Some("Bad Header".into()), auth_prefix: None }).is_err());
        let (url, server) = mock_ap(202, "{\"id\":\"msg-1\",\"status\":\"queued\"}");
        let s = set_ap_settings(&dir, &ApSettingsInput {
            endpoint: url,
            api_key: Some("secret-key-123".into()),
            auth_header: Some("X-API-Key".into()),
            auth_prefix: Some(String::new()),
        })
        .unwrap();
        assert!(s.api_key_set);
        assert_eq!(s.auth_header, "X-API-Key");
        let json = std::fs::read_to_string(dir.join(AP_SETTINGS_FILE)).unwrap();
        assert!(json.contains("secret-key-123"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(AP_SETTINGS_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(!serde_json::to_string(&ap_settings(&dir).unwrap()).unwrap().contains("secret"), "the key never leaves the file");
        let r = ap_send(&dir, &company, &b.sale, "pint-sg").unwrap();
        assert_eq!((r.status, r.accepted), (202, true));
        assert!(r.response.contains("queued"));
        assert_eq!(r.receiver.as_deref(), Some("0195:SGUEN200254321Z"));
        let request = server.join().unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/documents HTTP/1.1"), "{request}");
        assert!(lower.contains("content-type: application/xml"));
        assert!(lower.contains("x-api-key: secret-key-123"));
        assert!(lower.contains("x-peppol-receiver-id: 0195:sguen200254321z"));
        assert!(lower.contains("x-peppol-document-type-id: urn:oasis:names:specification:ubl:schema:xsd:invoice-2::invoice##urn:peppol:pint:billing-1@sg-1::2.1"));
        assert!(request.contains("<cbc:CustomizationID>urn:peppol:pint:billing-1@sg-1</cbc:CustomizationID>"));
        // A rejection is reported, not hidden.
        let (url, server) = mock_ap(400, "{\"error\":\"invalid document\"}");
        set_ap_settings(&dir, &ApSettingsInput { endpoint: url, api_key: None, auth_header: None, auth_prefix: None }).unwrap();
        let r = ap_send(&dir, &company, &b.sale, "pint-sg").unwrap();
        assert_eq!((r.status, r.accepted), (400, false));
        assert!(server.join().unwrap().to_ascii_lowercase().contains("x-api-key: secret-key-123"), "the stored key is kept");
        assert!(ap_send(&dir, &company, &b.sale, "xrechnung").unwrap_err().contains("unknown document"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

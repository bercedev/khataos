//! Saudi Arabia: ZATCA e-invoices and the VAT return from the books. The
//! rules live in `khata_sa`; this module turns vouchers into its documents
//! and keeps the device's chain.
//!
//! **The chain is per device.** The invoice counter (ICV) and the last
//! invoice hash (the next PIH) live in this company file's settings table
//! (`zatca.chain`), which is outside the audit trail and never synced:
//! every computer that issues invoices is its own EGS (research §6.2). Each
//! generated document is recorded under `zatca.doc.<voucher id>` (kind,
//! ICV, UUID, PIH, hash, issue time, QR and the XML itself), so generating
//! the same voucher again returns the same XML. The chain is written before
//! the record: a crash in between skips a counter value but never reuses
//! one (ZATCA allows gaps from failed documents; reuse breaks the chain).
//!
//! **Seller details.** ZATCA needs the seller's CRN and Saudi National
//! Address (building number, street, district, city, postal code), which
//! the core company profile doesn't hold yet. They are a shared record of
//! the company (`zatca.seller`, synced; see [`zatca_seller`]); anything left blank is
//! taken from the company address where possible (first line: street; last
//! line: city; a 5-digit number: postal code). These should move into the
//! core profile later.
//!
//! **Onboarding and the stamp** ([`zatca_onboard`], [`zatca_status`],
//! [`zatca_submit`]; HTTP in `zatca_api.rs`). Each company file on each
//! computer is one device (EGS) with its own id (`zatca.device` in the
//! settings: environment, CSR, check results; nothing secret). Its private
//! key and CSID tokens and secrets are kept **outside the books**, in
//! `<app data>/zatca/<device id>.json`, created readable and writable by
//! this user only (0600 on Unix; on Windows the file inherits the per-user
//! app data folder's ACL, which grants only the user, Administrators and
//! SYSTEM). That file is never synced, never in backups of the company
//! file and never in the audit trail; a restored or copied company is
//! onboarded again. Once the device has its production CSID, generated
//! documents are stamped (XAdES, QR tags 6 to 9). A document generated
//! before onboarding is stamped when it is submitted: stamping does not
//! change its hash, so the chain holds. ZATCA's answers (warnings, errors,
//! the cleared XML of a standard document) are kept per voucher under
//! `zatca.submission.<voucher id>`.
//!
//! Vouchers: Sales → invoice (388); Credit Note → credit note (381); a Debit
//! Note to a customer (a debtor) → debit note (383). A Debit Note to a
//! supplier is a purchase return, not an e-invoice. Lines split as on the
//! printed invoice: items and service ledgers are lines, VAT ledgers give
//! the booked VAT, ledgers on the party's side are discounts, "round off"
//! ledgers are left out (with a warning).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{NaiveTime, Timelike, Utc};
use khata_compliance::RateSchedule;
use khata_core::{Amount, BillKind, Book, DrCr, Voucher, VoucherId, VoucherKind};
use khata_sa::returns::{self, DocKind, DocLine, InwardDoc, OutwardDoc, Period, VatReturn};
use khata_sa::vat::{self, Mechanism, Treatment};
use khata_sa::csr::{self, Environment};
use khata_sa::xades::SigningCertificate;
use khata_sa::zatca::{self, Address, Allowance, Category, DocumentType, EInvoice, InvoiceKind, Line, Party, Stamp};
use khata_store::Store;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, group_is_party, open_mut, open_ref, parse_date, parse_uuid};
use crate::zatca_api::{Client, Csid, Document, Outcome};

const SELLER_KEY: &str = "zatca.seller";
const CHAIN_KEY: &str = "zatca.chain";
const DOC_PREFIX: &str = "zatca.doc.";

fn ksa_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_sa::COUNTRY.code) {
        return Err("ZATCA e-invoices and the KSA VAT return are for companies in Saudi Arabia".into());
    }
    Ok(())
}

fn store_ref(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

/// The seller details are a shared record of the company; the device's
/// EGS unit (CSID record, chain, documents, queue) stays in this computer's
/// settings (`khata_store::records::DEVICE_LOCAL_KEYS`).
fn read<T: for<'de> Deserialize<'de>>(store: &Store, key: &str) -> ApiResult<Option<T>> {
    let json = if khata_store::records::is_shared_key(key) { store.record(key) } else { store.setting(key) };
    match json.map_err(err)? {
        Some(json) => serde_json::from_str(&json).map(Some).map_err(|e| format!("setting {key} is damaged: {e}")),
        None => Ok(None),
    }
}

fn write<T: Serialize>(store: &Store, key: &str, value: &T) -> ApiResult<()> {
    let json = serde_json::to_string(value).map_err(err)?;
    if khata_store::records::is_shared_key(key) {
        return store.set_record(key, Some(&json), &crate::api::os_user()).map(|_| ()).map_err(err);
    }
    store.set_setting(key, Some(&json)).map_err(err)
}

// ------------------------------------------------------------ seller

/// The seller details ZATCA needs beyond the company profile. Blank fields
/// fall back to the company address (see the module notes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZatcaSeller {
    /// Commercial registration number (scheme CRN).
    pub crn: String,
    /// 4 digits.
    pub building_number: String,
    pub street: String,
    pub additional_street: String,
    /// The National Address "additional number".
    pub additional_number: String,
    pub district: String,
    pub city: String,
    pub postal_code: String,
}

/// (street, city, postal code) guessed from an address written as lines.
fn address_from_text(text: &str) -> (String, String, String) {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let postal = text
        .split(|c: char| !c.is_ascii_digit())
        .find(|t| t.len() == 5)
        .unwrap_or_default()
        .to_owned();
    let street = lines.first().map(|l| (*l).to_owned()).unwrap_or_default();
    let city = if lines.len() > 1 {
        let last = lines[lines.len() - 1];
        last.split_whitespace().filter(|w| !(w.len() == 5 && w.bytes().all(|b| b.is_ascii_digit()))).collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    };
    (street, city, postal)
}

fn or(primary: &str, fallback: String) -> String {
    if primary.trim().is_empty() { fallback } else { primary.trim().to_owned() }
}

/// What is stored, and what will be used once the address fills the gaps.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZatcaSellerDto {
    pub stored: ZatcaSeller,
    pub effective: ZatcaSeller,
}

fn effective_seller(book: &Book, stored: &ZatcaSeller) -> ZatcaSeller {
    let address = book.profile().map(|p| p.address.clone()).unwrap_or_default();
    let (street, city, postal) = address_from_text(&address);
    ZatcaSeller {
        street: or(&stored.street, street),
        city: or(&stored.city, city),
        postal_code: or(&stored.postal_code, postal),
        ..stored.clone()
    }
}

/// The ZATCA seller details.
pub fn zatca_seller(slot: &Slot) -> ApiResult<ZatcaSellerDto> {
    let book = open_ref(slot)?;
    ksa_only(book)?;
    let stored: ZatcaSeller = read(store_ref(slot)?, SELLER_KEY)?.unwrap_or_default();
    Ok(ZatcaSellerDto { effective: effective_seller(book, &stored), stored })
}

/// Save the ZATCA seller details (owner only).
pub fn set_zatca_seller(slot: &mut Slot, seller: &ZatcaSeller) -> ApiResult<ZatcaSellerDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, khata_core::UserRole::Owner)?;
    ksa_only(book)?;
    let b = seller.building_number.trim();
    if !b.is_empty() && !(b.len() == 4 && b.bytes().all(|c| c.is_ascii_digit())) {
        return Err("the building number has 4 digits (BR-KSA-37)".into());
    }
    let clean = ZatcaSeller {
        crn: seller.crn.trim().to_owned(),
        building_number: b.to_owned(),
        street: seller.street.trim().to_owned(),
        additional_street: seller.additional_street.trim().to_owned(),
        additional_number: seller.additional_number.trim().to_owned(),
        district: seller.district.trim().to_owned(),
        city: seller.city.trim().to_owned(),
        postal_code: seller.postal_code.trim().to_owned(),
    };
    write(store, SELLER_KEY, &clean)?;
    zatca_seller(slot)
}

// ------------------------------------------------- vouchers into lines

fn rates(book: &Book) -> ApiResult<RateSchedule> {
    let mut rates = RateSchedule::default();
    if let Some(p) = book.profile() {
        for (class, changes) in &p.tax_rates {
            for (from, r) in changes {
                rates.set(class, *from, *r).map_err(err)?;
            }
        }
    }
    Ok(rates)
}

fn label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Sales",
        VoucherKind::Purchase => "Purchase",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        _ => "Voucher",
    }
}

/// The party line: a debtor/creditor, else the cash or bank line on the
/// party's side, else the first line on that side.
fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = match v.kind {
        VoucherKind::Sales | VoucherKind::DebitNote => DrCr::Dr,
        _ => DrCr::Cr,
    };
    let is_party = |e: &khata_core::Entry| book.ledger(e.ledger).is_some_and(|l| group_is_party(book, l.group));
    v.entries
        .iter()
        .position(is_party)
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

fn is_debtor(book: &Book, ledger: khata_core::LedgerId) -> bool {
    let debtors = khata_core::GroupId::default_named("Sundry Debtors");
    crate::api::ancestors(book, book.ledger(ledger).map(|l| l.group)).any(|g| g.id == debtors)
}

/// One line of a voucher, before tax treatment.
struct RawLine {
    name: String,
    qty: Decimal,
    rate: Decimal,
    /// Signed: negative when on the party's side.
    amount: Amount,
    class: Option<String>,
    /// A stock item (goods) rather than a service ledger.
    goods: bool,
}

struct Parsed {
    party: Option<usize>,
    lines: Vec<RawLine>,
    /// Discounts: ledgers on the party's side (positive amounts).
    discounts: Vec<(String, Amount)>,
    vat: Amount,
    warnings: Vec<String>,
}

fn parse(book: &Book, v: &Voucher) -> Parsed {
    let name = format!("{} {}", label(v.kind), v.number);
    let party = party_line(book, v);
    let party_side = party.map(|i| v.entries[i].side);
    let mut p = Parsed { party, lines: Vec::new(), discounts: Vec::new(), vat: Amount::ZERO, warnings: Vec::new() };
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if t.component.eq_ignore_ascii_case(vat::VAT) {
                p.vat += signed;
            } else {
                p.warnings.push(format!("{name}: {ledger_name} collects {}, which is not VAT; it is left out", t.component));
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                p.lines.push(RawLine {
                    name: item.map(|i| i.name.clone()).unwrap_or_default(),
                    qty: l.qty,
                    rate: l.rate,
                    amount: if on_party_side { -l.amount } else { l.amount },
                    class: item.and_then(|i| i.tax_class.clone()),
                    goods: true,
                });
            }
        } else if ledger_name.to_lowercase().contains("round") {
            p.warnings.push(format!("{name}: {ledger_name} (rounding) is left out of the e-invoice and the return"));
        } else if on_party_side {
            p.discounts.push((ledger_name, e.amount));
        } else {
            p.lines.push(RawLine {
                name: ledger_name,
                qty: Decimal::ONE,
                rate: e.amount,
                amount: e.amount,
                class: ledger.and_then(|l| l.tax_class.clone()),
                goods: false,
            });
        }
    }
    p
}

fn abroad(book: &Book, v: &Voucher, party: Option<usize>) -> bool {
    party
        .and_then(|i| book.ledger(v.entries[i].ledger))
        .and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .is_some_and(|c| !c.trim().is_empty() && !c.eq_ignore_ascii_case("SA"))
}

// ------------------------------------------------------------ e-invoice

/// One generated document as the device recorded it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Record {
    kind: String,
    icv: u64,
    uuid: String,
    pih: String,
    hash: String,
    issue_time: String,
    qr: String,
    xml: String,
    /// When it was stamped (UTC, "…Z"); `None` until the device is onboarded.
    #[serde(default)]
    signing_time: Option<String>,
    /// When it was generated (UTC, "…Z"): a simplified document must be
    /// reported within 24 hours of it. `None` on records kept before this
    /// was added (the voucher date and issue time are used instead).
    #[serde(default)]
    generated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZatcaInvoiceDto {
    pub xml: String,
    pub uuid: String,
    pub icv: u64,
    /// This document's hash (Base64): the next document's PIH.
    pub hash: String,
    /// The PIH it carries.
    pub pih: String,
    /// QR payload (Base64 TLV): tags 1 to 5, and 6 to 9 once stamped.
    pub qr: String,
    /// `<VAT>_<date>T<time>_<number>.xml`.
    pub file_name: String,
    /// The document was already generated; this is the recorded one.
    pub regenerated: bool,
    /// It carries the cryptographic stamp (XAdES signature, QR tags 6–9).
    #[serde(default)]
    pub signed: bool,
    pub warnings: Vec<String>,
}

fn parse_kind(kind: &str) -> ApiResult<InvoiceKind> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "standard" => Ok(InvoiceKind::Standard),
        "simplified" => Ok(InvoiceKind::Simplified),
        other => Err(format!("unknown e-invoice kind {other:?} (standard or simplified)")),
    }
}

fn kind_name(kind: InvoiceKind) -> &'static str {
    if kind == InvoiceKind::Standard { "standard" } else { "simplified" }
}

fn region_name(code: Option<&str>) -> String {
    code.and_then(khata_sa::regions::region).map(|r| r.name.to_owned()).unwrap_or_default()
}

/// The e-invoice for a voucher, at a given UUID and issue time.
fn einvoice(book: &Book, store: &Store, v: &Voucher, kind: InvoiceKind, uuid: &str, time: NaiveTime) -> ApiResult<(EInvoice, Vec<String>)> {
    let doc_type = match v.kind {
        VoucherKind::Sales => DocumentType::Invoice,
        VoucherKind::CreditNote => DocumentType::CreditNote,
        VoucherKind::DebitNote => DocumentType::DebitNote,
        _ => return Err("e-invoices are made from Sales, Credit Note and Debit Note vouchers".into()),
    };
    if v.cancelled {
        return Err("a cancelled voucher has no e-invoice".into());
    }
    book.profile().ok_or("pick the company's country first")?;
    let p = parse(book, v);
    let mut warnings = p.warnings;
    let party_ledger = p.party.and_then(|i| book.ledger(v.entries[i].ledger));
    if v.kind == VoucherKind::DebitNote && !party_ledger.is_some_and(|l| is_debtor(book, l.id)) {
        return Err("this Debit Note is a purchase return (its party is not a customer); it has no e-invoice".into());
    }
    let rates = rates(book)?;
    let export = abroad(book, v, p.party);
    let mut lines = Vec::new();
    for (i, l) in p.lines.iter().enumerate() {
        if l.amount < Decimal::ZERO {
            return Err(format!("line {} is negative; put discounts on a ledger on the party's side", i + 1));
        }
        let class = l.class.clone().unwrap_or_else(|| vat::STANDARD_CLASS.to_owned());
        let rate = rates.rate(&class, v.date).ok_or_else(|| {
            format!("no VAT rate for {class:?} on {}: add it to the company's rate list", v.date)
        })?;
        let category = if export {
            let code = if l.goods { "VATEX-SA-32" } else { "VATEX-SA-33" };
            let x = vat::vatex(code).expect("known code");
            Category { treatment: Treatment::ZeroRated, percent: Decimal::ZERO, exemption: Some((x.code.into(), x.meaning.into())) }
        } else {
            let treatment = vat::treatment(&class, rate);
            let exemption = (treatment != Treatment::Standard)
                .then(|| vat::vatex_in_class(&class))
                .flatten()
                .map(|x| (x.code.to_owned(), x.meaning.to_owned()));
            Category { treatment, percent: if treatment == Treatment::Standard { rate } else { Decimal::ZERO }, exemption }
        };
        lines.push(Line {
            name: l.name.clone(),
            quantity: l.qty,
            // KhataOS units are free text, not UN/ECE Rec 20 codes: "PCE" for all.
            unit_code: String::new(),
            unit_price: l.rate,
            net_amount: l.amount,
            category,
        });
    }
    let mut allowances = Vec::new();
    if let Some(first) = lines.first().map(|l| l.category.clone()) {
        let category = lines.iter().map(|l| &l.category).find(|c| c.treatment == Treatment::Standard).cloned().unwrap_or(first);
        for (reason, amount) in &p.discounts {
            allowances.push(Allowance { reason: reason.clone(), amount: *amount, category: category.clone() });
        }
        if !p.discounts.is_empty() {
            warnings.push(format!("discounts are put on the {} lines at {}%", category.treatment.category(), category.percent.normalize()));
        }
    }

    let seller = seller_party(book, store)?;
    let buyer = party_ledger.filter(|l| group_is_party(book, l.group)).map(|l| {
        let m = l.mailing.clone().unwrap_or_default();
        let (street, city, postal) = address_from_text(&m.address);
        let country = m.country.clone().filter(|c| !c.trim().is_empty()).unwrap_or_else(|| "SA".into());
        let vat_number = l.tax_id.clone().filter(|t| !export && khata_sa::vat_number::validate(t).is_ok());
        if l.tax_id.is_some() && vat_number.is_none() && !export {
            warnings.push(format!("{}'s tax ID is not a valid KSA VAT number; it is left out", l.name));
        }
        Party {
            name: l.name.clone(),
            vat_number,
            other_id: None,
            address: Address {
                street,
                city,
                postal_code: or(&m.postal_code, postal),
                region: if country == "SA" { region_name(m.region.as_deref()) } else { String::new() },
                country,
                ..Address::default()
            },
        }
    });
    let billing_reference = match doc_type {
        DocumentType::Invoice => None,
        _ => p.party.and_then(|i| {
            v.entries[i].bills.iter().find(|b| b.kind == BillKind::Against && !b.reference.trim().is_empty()).map(|b| b.reference.clone())
        }),
    };
    let payment_means = if p.party.is_some_and(|i| book.is_cash_or_bank(v.entries[i].ledger)) { "10" } else { "30" };
    let inv = EInvoice {
        kind,
        doc_type,
        number: v.number.clone(),
        uuid: uuid.to_owned(),
        issue_date: v.date,
        issue_time: time,
        supply_date: Some(v.date),
        export: export && kind == InvoiceKind::Standard,
        billing_reference,
        reason: Some(v.narration.trim().to_owned()).filter(|n| !n.is_empty()),
        payment_means: payment_means.into(),
        seller,
        buyer,
        lines,
        allowances,
    };
    // The books are the truth: say so when the XML's VAT differs from them.
    let computed = zatca::totals(&inv).vat;
    if computed != p.vat {
        warnings.push(format!("VAT booked is {}, but the e-invoice works out {}", p.vat.normalize(), computed.normalize()));
    }
    Ok((inv, warnings))
}

/// The seller as ZATCA sees it: the company name and VAT number, the CRN
/// and National Address from the ZATCA seller details.
fn seller_party(book: &Book, store: &Store) -> ApiResult<Party> {
    let profile = book.profile().ok_or("pick the company's country first")?;
    let stored: ZatcaSeller = read(store, SELLER_KEY)?.unwrap_or_default();
    let s = effective_seller(book, &stored);
    Ok(Party {
        name: book.name.clone(),
        vat_number: profile.tax_id.clone(),
        other_id: (!s.crn.is_empty()).then(|| ("CRN".to_owned(), s.crn.clone())),
        address: Address {
            street: s.street,
            additional_street: s.additional_street,
            building_number: s.building_number,
            additional_number: s.additional_number,
            district: s.district,
            city: s.city,
            postal_code: s.postal_code,
            region: region_name(profile.tax_region.as_deref()),
            country: "SA".into(),
        },
    })
}

/// Riyadh time now (AST, UTC+3; no daylight saving), to the second.
fn riyadh_now() -> NaiveTime {
    let t = (Utc::now() + chrono::Duration::hours(3)).time();
    NaiveTime::from_hms_opt(t.hour(), t.minute(), t.second()).expect("valid time")
}

fn dto(inv: &EInvoice, r: &Record, regenerated: bool, warnings: Vec<String>) -> ZatcaInvoiceDto {
    ZatcaInvoiceDto {
        xml: r.xml.clone(),
        uuid: r.uuid.clone(),
        icv: r.icv,
        hash: r.hash.clone(),
        pih: r.pih.clone(),
        qr: r.qr.clone(),
        file_name: zatca::file_name(inv),
        regenerated,
        signed: r.signing_time.is_some(),
        warnings,
    }
}

/// The ZATCA XML for a voucher. The first call takes the device's next
/// counter value and hash; later calls return the recorded document. Not
/// stamped: see [`zatca_invoice_xml_stamped`].
pub fn zatca_invoice_xml(slot: &mut Slot, voucher_id: &str, kind: &str) -> ApiResult<ZatcaInvoiceDto> {
    let kind = parse_kind(kind)?;
    let (store, book) = open_mut(slot)?;
    ksa_only(book)?;
    let id = VoucherId(parse_uuid(voucher_id, "voucher")?);
    let (inv, r, regenerated, warnings) = document(store, book, id, kind, None)?;
    Ok(dto(&inv, &r, regenerated, warnings))
}

/// [`zatca_invoice_xml`], stamped with the device's key and CSID once the
/// device is onboarded (app data folder `data_dir`). A document recorded
/// unstamped is stamped now when the voucher is unchanged; its hash, and so
/// the chain, stays the same.
pub fn zatca_invoice_xml_stamped(slot: &mut Slot, data_dir: &Path, voucher_id: &str, kind: &str) -> ApiResult<ZatcaInvoiceDto> {
    let kind = parse_kind(kind)?;
    let (store, book) = open_mut(slot)?;
    ksa_only(book)?;
    let id = VoucherId(parse_uuid(voucher_id, "voucher")?);
    let keys = signing_keys(store, data_dir)?;
    let mut extra = Vec::new();
    let stamp = match &keys {
        Some(k) => {
            extra.extend(k.warnings.iter().cloned());
            Some((&k.signer, &k.certificate))
        }
        None => None,
    };
    let (inv, r, regenerated, mut warnings) = document(store, book, id, kind, stamp)?;
    warnings.extend(extra);
    Ok(dto(&inv, &r, regenerated, warnings))
}

/// The recorded document for a voucher, generated (and the chain moved on)
/// the first time; stamped when `stamp` is given and it isn't yet.
fn document(
    store: &Store,
    book: &Book,
    id: VoucherId,
    kind: InvoiceKind,
    stamp: Option<(&khata_sa::sign::Signer, &SigningCertificate)>,
) -> ApiResult<(EInvoice, Record, bool, Vec<String>)> {
    let v = book.voucher(id).ok_or("unknown voucher")?;
    crate::forex::refuse_foreign_einvoice(v, "A ZATCA e-invoice")?;
    let key = format!("{DOC_PREFIX}{}", id.0);
    let now = Utc::now().naive_utc();
    if let Some(mut r) = read::<Record>(store, &key)? {
        if r.kind != kind_name(kind) {
            return Err(format!("this voucher's e-invoice was already generated as a {} invoice", r.kind));
        }
        let time = NaiveTime::parse_from_str(&r.issue_time, "%H:%M:%S").map_err(err)?;
        let (inv, mut warnings) = einvoice(book, store, v, kind, &r.uuid, time)?;
        let again = zatca::generate(&inv, r.icv, &r.pih, None).map_err(err)?;
        let changed = again.hash != r.hash;
        if changed {
            warnings.push(
                "the voucher changed after its e-invoice was generated; this is the document as issued (correct it with a credit or debit note)"
                    .into(),
            );
        }
        if let (Some((signer, cert)), None, false) = (stamp, &r.signing_time, changed) {
            let s = Stamp::new(signer, cert, now).map_err(err)?;
            let g = zatca::generate(&inv, r.icv, &r.pih, Some(&s)).map_err(err)?;
            debug_assert_eq!(g.hash, r.hash, "stamping keeps the hash");
            r.xml = g.xml;
            r.qr = g.qr;
            r.signing_time = g.signing_time;
            write(store, &key, &r)?;
        }
        return Ok((inv, r, true, warnings));
    }
    let uuid = uuid::Uuid::new_v4().to_string();
    let time = riyadh_now();
    let (inv, mut warnings) = einvoice(book, store, v, kind, &uuid, time)?;
    let s = match stamp {
        Some((signer, cert)) => Some(Stamp::new(signer, cert, now).map_err(err)?),
        None => None,
    };
    let mut chain: zatca::ChainState = read(store, CHAIN_KEY)?.unwrap_or_default();
    let g = zatca::generate_next(&inv, &mut chain, s.as_ref()).map_err(err)?;
    // Chain first: a crash before the record skips a counter, never reuses one.
    write(store, CHAIN_KEY, &chain)?;
    let r = Record {
        kind: kind_name(kind).into(),
        icv: g.icv,
        uuid: g.uuid.clone(),
        pih: g.pih.clone(),
        hash: g.hash.clone(),
        issue_time: time.format("%H:%M:%S").to_string(),
        qr: g.qr.clone(),
        xml: g.xml.clone(),
        signing_time: g.signing_time.clone(),
        generated_at: Some(utc_text(Utc::now())),
    };
    write(store, &key, &r)?;
    // Every new document waits in the queue until ZATCA accepts it.
    enqueue(store, &id.0.to_string())?;
    warnings.extend(g.warnings);
    Ok((inv, r, false, warnings))
}

/// For printing: the recorded e-invoice's title and QR symbol, when the
/// voucher has one. A cleared standard invoice prints ZATCA's QR from the
/// cleared XML.
pub fn print_extras(slot: &Slot, voucher_id: VoucherId) -> Option<(&'static str, khata_print::QrSymbol)> {
    let store = store_ref(slot).ok()?;
    let r: Record = read(store, &format!("{DOC_PREFIX}{}", voucher_id.0)).ok().flatten()?;
    let cleared = read::<Submission>(store, &format!("{SUBMISSION_PREFIX}{}", voucher_id.0))
        .ok()
        .flatten()
        .and_then(|s| s.outcome.cleared_xml)
        .and_then(|x| qr_of(&x));
    let m = khata_sa::qr::matrix(cleared.as_deref().unwrap_or(&r.qr)).ok()?;
    let title = if r.kind == "simplified" { "Simplified Tax Invoice" } else { "Tax Invoice" };
    Some((title, khata_print::QrSymbol { width: m.width, dark: m.dark }))
}

// ------------------------------------------------- device (EGS) and ZATCA

const DEVICE_KEY: &str = "zatca.device";
const SUBMISSION_PREFIX: &str = "zatca.submission.";
/// Under the app data folder.
const SECRETS_DIR: &str = "zatca";

/// The device as the company file records it. Nothing secret.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Device {
    device_id: String,
    environment: String,
    common_name: String,
    egs_serial: String,
    invoice_types: String,
    /// The CSR sent (public).
    csr_pem: String,
    created_at: String,
    compliance_csid: bool,
    checks: Vec<ComplianceCheck>,
    production_csid: bool,
    /// The certificate that stamps documents: its expiry (UTC).
    certificate_expiry: Option<String>,
    last_error: Option<String>,
    /// ZATCA refused the production CSID (401) at this time, with this
    /// message: it expired or was revoked. Cleared by a renewal.
    csid_refused: Option<String>,
    /// When the production CSID was last renewed (UTC).
    renewed_at: Option<String>,
}

/// The secrets file. Never logged: `Debug` hides everything but the id.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Secrets {
    device_id: String,
    environment: String,
    private_key_pem: String,
    compliance: Option<Csid>,
    production: Option<Csid>,
    /// The key of a renewal in progress, saved before its CSR is sent so a
    /// crash after ZATCA answers loses nothing.
    pending_key_pem: Option<String>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets").field("device_id", &self.device_id).finish_non_exhaustive()
    }
}

/// Where a device's secrets live.
pub fn secrets_path(data_dir: &Path, device_id: &str) -> PathBuf {
    data_dir.join(SECRETS_DIR).join(format!("{device_id}.json"))
}

fn read_secrets(path: &Path) -> ApiResult<Option<Secrets>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|_| format!("the ZATCA credentials file {} is damaged", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read the ZATCA credentials file {}: {e}", path.display())),
    }
}

/// Write the secrets readable by this user only (0600 on Unix), through a
/// temporary file created with the same mode and renamed into place.
fn write_secrets(path: &Path, secrets: &Secrets) -> ApiResult<()> {
    let dir = path.parent().ok_or("bad credentials path")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let json = serde_json::to_vec_pretty(secrets).map_err(err)?;
    {
        use std::io::Write;
        let mut f = opts.open(&tmp).map_err(|e| format!("cannot save the ZATCA credentials: {e}"))?;
        f.write_all(&json).and_then(|_| f.sync_all()).map_err(|e| format!("cannot save the ZATCA credentials: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot save the ZATCA credentials: {e}"))
}

/// One compliance check's result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ComplianceCheck {
    /// "standard invoice", "simplified credit note"…
    pub document: String,
    /// PASS, WARNING or ERROR.
    pub status: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

fn texts(messages: &[crate::zatca_api::Message]) -> Vec<String> {
    messages
        .iter()
        .map(|m| {
            let code = m.code.clone().unwrap_or_default();
            let msg = m.message.clone().unwrap_or_default();
            if code.is_empty() { msg } else { format!("{code}: {msg}") }
        })
        .collect()
}

/// The device's onboarding state, for the screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ZatcaStatusDto {
    /// Ready to stamp and submit documents.
    pub onboarded: bool,
    pub environment: Option<String>,
    pub device_id: Option<String>,
    pub common_name: Option<String>,
    pub egs_serial: Option<String>,
    pub invoice_types: Option<String>,
    /// The CSR has been built (the key exists).
    pub csr_created: bool,
    pub csr_pem: Option<String>,
    pub compliance_csid: bool,
    pub checks: Vec<ComplianceCheck>,
    pub production_csid: bool,
    pub certificate_expiry: Option<String>,
    /// Whole days until the stamping certificate expires (negative: expired).
    pub days_to_expiry: Option<i64>,
    /// The certificate expires within [`RENEW_BEFORE_DAYS`] days: renew it.
    pub renewal_due: bool,
    pub expired: bool,
    /// ZATCA refused the CSID (401): when, and its message.
    pub csid_refused: Option<String>,
    pub renewed_at: Option<String>,
    /// Documents waiting to be reported or cleared ([`zatca_queue`]).
    pub queue_pending: usize,
    /// Where the key and CSID secrets are kept on this computer.
    pub credentials_path: Option<String>,
    /// The credentials file is on this computer.
    pub credentials_present: bool,
    /// The device's counter and the next PIH.
    pub icv: u64,
    pub pih: String,
    pub next_step: String,
    pub last_error: Option<String>,
    pub warnings: Vec<String>,
}

/// Warn this many days before the CSID's certificate expires.
pub const RENEW_BEFORE_DAYS: i64 = 30;

fn utc_text(t: chrono::DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn parse_utc(text: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(text).ok().map(|t| t.with_timezone(&Utc))
}

/// The certificate that stamps documents: the production one when it is for
/// the device's key, else the compliance one (the sandbox's sample
/// production certificate is for another key).
fn stamping_certificate(sec: &Secrets) -> Option<SigningCertificate> {
    let key = khata_sa::sign::Signer::from_pem(&sec.private_key_pem).ok()?;
    let mine = |c: &Csid| SigningCertificate::from_binary_security_token(&c.token).ok().filter(|x| x.public_key_der() == key.public_key_der().as_slice());
    sec.production.as_ref().and_then(mine).or_else(|| sec.compliance.as_ref().and_then(mine))
}

fn status_of(store: &Store, data_dir: &Path) -> ApiResult<ZatcaStatusDto> {
    status_at(store, data_dir, Utc::now())
}

fn status_at(store: &Store, data_dir: &Path, now: chrono::DateTime<Utc>) -> ApiResult<ZatcaStatusDto> {
    let chain: zatca::ChainState = read(store, CHAIN_KEY)?.unwrap_or_default();
    let device: Option<Device> = read(store, DEVICE_KEY)?;
    let mut s = ZatcaStatusDto { icv: chain.last_icv, pih: chain.last_hash, ..ZatcaStatusDto::default() };
    let Some(d) = device else {
        s.next_step = "Onboard this computer: pick the environment, get an OTP from the Fatoora portal and enter it here.".into();
        return Ok(s);
    };
    let path = secrets_path(data_dir, &d.device_id);
    let secrets = read_secrets(&path)?.filter(|x| x.device_id == d.device_id);
    s.credentials_present = secrets.is_some();
    s.credentials_path = Some(path.display().to_string());
    s.environment = Some(d.environment.clone());
    s.device_id = Some(d.device_id.clone());
    s.common_name = Some(d.common_name.clone());
    s.egs_serial = Some(d.egs_serial.clone());
    s.invoice_types = Some(d.invoice_types.clone());
    s.csr_created = !d.csr_pem.is_empty() && secrets.as_ref().is_some_and(|x| !x.private_key_pem.is_empty());
    s.csr_pem = Some(d.csr_pem.clone()).filter(|p| !p.is_empty());
    s.compliance_csid = d.compliance_csid && secrets.as_ref().is_some_and(|x| x.compliance.is_some());
    s.checks = d.checks.clone();
    s.production_csid = d.production_csid && secrets.as_ref().is_some_and(|x| x.production.is_some());
    s.certificate_expiry = d.certificate_expiry.clone();
    s.last_error = d.last_error.clone();
    s.csid_refused = d.csid_refused.clone();
    s.renewed_at = d.renewed_at.clone();
    s.queue_pending = read::<Vec<QueueEntry>>(store, QUEUE_KEY)?.map_or(0, |q| q.len());
    s.onboarded = s.production_csid && s.csr_created;
    if let Some(cert) = secrets.as_ref().filter(|_| s.production_csid).and_then(stamping_certificate) {
        let expiry = chrono::DateTime::from_timestamp(cert.not_after_unix() as i64, 0);
        if let Some(t) = expiry {
            s.certificate_expiry = Some(t.format("%Y-%m-%d").to_string());
            let days = (t - now).num_days();
            s.days_to_expiry = Some(days);
            s.expired = t <= now;
            s.renewal_due = days <= RENEW_BEFORE_DAYS;
            if s.expired {
                s.warnings.push(format!(
                    "the CSID's certificate expired on {}: ZATCA refuses documents stamped with it; renew it with a new OTP",
                    t.format("%Y-%m-%d")
                ));
            } else if s.renewal_due {
                s.warnings.push(format!(
                    "the CSID's certificate expires on {} (in {days} days): renew it now with a new OTP from the Fatoora portal",
                    t.format("%Y-%m-%d")
                ));
            }
        }
    }
    if let Some(r) = &d.csid_refused {
        s.warnings.push(format!(
            "ZATCA refused this device's CSID ({r}): it expired or was revoked on the Fatoora portal. Renew it with a new OTP; if it was revoked, onboard again (Start again). Documents wait in the queue meanwhile"
        ));
    }
    if secrets.is_none() {
        s.warnings.push(format!(
            "the key and CSID for this device are not on this computer ({}); onboard again (a copied or restored company is a new device)",
            path.display()
        ));
    }
    if d.environment != Environment::Production.name() {
        s.warnings.push(format!("this device is onboarded with ZATCA's {} environment: its documents have no legal effect", d.environment));
    }
    s.next_step = if s.onboarded && (s.expired || s.csid_refused.is_some()) {
        "Renew the CSID: get a new OTP from the Fatoora portal, enter it and choose Renew.".into()
    } else if s.onboarded && s.renewal_due {
        "Ready, but the CSID expires soon: renew it with a new OTP (Renew).".into()
    } else if s.onboarded {
        "Ready: e-invoices are stamped, and Submit sends them to ZATCA (clearance for standard, reporting for simplified).".into()
    } else if !s.compliance_csid {
        "Enter an OTP from the Fatoora portal to get the compliance CSID.".into()
    } else {
        "Run onboarding again (no OTP needed) to finish the compliance checks and get the production CSID.".into()
    };
    Ok(s)
}

/// The device's onboarding state.
pub fn zatca_status(slot: &Slot, data_dir: &Path) -> ApiResult<ZatcaStatusDto> {
    let book = open_ref(slot)?;
    ksa_only(book)?;
    status_of(store_ref(slot)?, data_dir)
}

/// The key and certificate that stamp documents, once onboarded.
struct SigningKeys {
    signer: khata_sa::sign::Signer,
    certificate: SigningCertificate,
    production: Csid,
    environment: Environment,
    warnings: Vec<String>,
}

fn signing_keys(store: &Store, data_dir: &Path) -> ApiResult<Option<SigningKeys>> {
    let Some(d) = read::<Device>(store, DEVICE_KEY)? else { return Ok(None) };
    if !d.production_csid {
        return Ok(None);
    }
    let path = secrets_path(data_dir, &d.device_id);
    let Some(sec) = read_secrets(&path)?.filter(|x| x.device_id == d.device_id) else { return Ok(None) };
    let Some(production) = sec.production.clone() else { return Ok(None) };
    let signer = khata_sa::sign::Signer::from_pem(&sec.private_key_pem).map_err(err)?;
    let environment = Environment::parse(&sec.environment).ok_or("unknown ZATCA environment in the credentials file")?;
    let mut warnings = Vec::new();
    let prod_cert = SigningCertificate::from_binary_security_token(&production.token).map_err(err)?;
    let certificate = if prod_cert.public_key_der() == signer.public_key_der().as_slice() {
        prod_cert
    } else {
        // The developer portal's production CSID is a fixed sample
        // certificate for another key (checked live): stamp with the
        // compliance certificate, which is for this key.
        let compliance = sec.compliance.as_ref().ok_or("the production certificate is for another key")?;
        let c = SigningCertificate::from_binary_security_token(&compliance.token).map_err(err)?;
        if c.public_key_der() != signer.public_key_der().as_slice() {
            return Err("neither CSID certificate is for this device's key; onboard again".into());
        }
        warnings.push("the production certificate is not for this device's key (the sandbox issues a sample one); documents are stamped with the compliance certificate".into());
        c
    };
    Ok(Some(SigningKeys { signer, certificate, production, environment, warnings }))
}

/// Optional CSR details for onboarding; blanks are filled from the company.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OnboardOptions {
    /// OU: branch name (a VAT group member's 10-digit TIN). Default "Main Branch".
    pub branch: String,
    /// registeredAddress: the National Address short address or the
    /// branch address. Default: from the seller details.
    pub location: String,
    /// businessCategory. Default "Trading".
    pub industry: String,
    /// "1100" (standard and simplified, the default), "1000" or "0100".
    pub invoice_types: String,
    /// Start again with a new device id and key (a lost key, or another
    /// environment). The old device's documents keep their chain.
    pub restart: bool,
}

/// What onboarding needs, read under the lock.
struct Prepared {
    device: Device,
    secrets: Secrets,
    path: PathBuf,
    seller: Party,
}

fn new_key(entropy: Option<[u8; 32]>) -> ApiResult<khata_sa::sign::Signer> {
    if let Some(e) = entropy {
        return khata_sa::sign::key_from_entropy(&e).ok_or_else(|| "bad test key".to_owned());
    }
    for _ in 0..8 {
        let mut buf = [0u8; 32];
        getrandom::fill(&mut buf).map_err(|e| format!("no secure random numbers: {e}"))?;
        if let Some(k) = khata_sa::sign::key_from_entropy(&buf) {
            return Ok(k);
        }
    }
    Err("could not make a key".into())
}

/// The CSR for this device: the EGS unit's name and serial, the seller's
/// VAT number and address, and the options (blanks filled as described on
/// [`OnboardOptions`]).
#[allow(clippy::too_many_arguments)]
fn build_csr(
    book: &Book,
    seller: &Party,
    vat: &str,
    env: Environment,
    options: &OnboardOptions,
    types: &str,
    common_name: String,
    serial: String,
    signer: &khata_sa::sign::Signer,
) -> ApiResult<(csr::CsrInput, csr::Csr)> {
    let a = &seller.address;
    let location = if !options.location.trim().is_empty() {
        options.location.trim().to_owned()
    } else {
        [format!("{} {}", a.building_number, a.street).trim().to_owned(), a.district.clone(), a.city.clone()]
            .into_iter()
            .filter(|p| !p.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let input = csr::CsrInput {
        common_name,
        serial_number: serial,
        vat_number: vat.to_owned(),
        organization_unit: if options.branch.trim().is_empty() { "Main Branch".into() } else { options.branch.trim().into() },
        organization: book.name.trim().to_owned(),
        invoice_types: types.to_owned(),
        location: if location.is_empty() { "Saudi Arabia".into() } else { location },
        industry: if options.industry.trim().is_empty() { "Trading".into() } else { options.industry.trim().into() },
    };
    let built = csr::build(&input, env, signer).map_err(err)?;
    Ok((input, built))
}

fn prepare_onboarding(slot: &mut Slot, data_dir: &Path, env: Environment, options: &OnboardOptions, entropy: Option<[u8; 32]>) -> ApiResult<Prepared> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, khata_core::UserRole::Owner)?;
    ksa_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let vat = profile.tax_id.clone().unwrap_or_default();
    khata_sa::vat_number::validate(&vat).map_err(|e| format!("enter the company's VAT number first (Company → Tax registration): {e}"))?;
    let seller = seller_party(book, store)?;
    let mut device: Device = read(store, DEVICE_KEY)?.unwrap_or_default();
    let fresh = device.device_id.is_empty() || options.restart || device.environment != env.name();
    if !fresh && device.production_csid {
        let path = secrets_path(data_dir, &device.device_id);
        if read_secrets(&path)?.is_some_and(|s| s.production.is_some()) {
            return Err("this device is already onboarded; choose Start again to onboard a new device".into());
        }
    }
    if fresh {
        device = Device {
            device_id: uuid::Uuid::new_v4().to_string(),
            environment: env.name().into(),
            created_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            ..Device::default()
        };
    }
    let path = secrets_path(data_dir, &device.device_id);
    let mut secrets = read_secrets(&path)?.filter(|s| s.device_id == device.device_id && !s.private_key_pem.is_empty()).unwrap_or_default();
    if secrets.private_key_pem.is_empty() || device.csr_pem.is_empty() {
        // A new key and CSR. The key is saved before anything is sent.
        let signer = new_key(entropy)?;
        let types = if options.invoice_types.trim().is_empty() { "1100".to_owned() } else { options.invoice_types.trim().to_owned() };
        let common_name = format!("KhataOS-{}", &device.device_id[..8]);
        let serial = csr::egs_serial(env!("CARGO_PKG_VERSION"), &device.device_id);
        let (input, built) = build_csr(book, &seller, &vat, env, options, &types, common_name, serial, &signer)?;
        secrets = Secrets {
            device_id: device.device_id.clone(),
            environment: env.name().into(),
            private_key_pem: signer.to_sec1_pem().to_string(),
            ..Secrets::default()
        };
        write_secrets(&path, &secrets)?;
        device.common_name = input.common_name;
        device.egs_serial = input.serial_number;
        device.invoice_types = types;
        device.csr_pem = built.pem;
        device.compliance_csid = false;
        device.production_csid = false;
        device.checks.clear();
    }
    device.last_error = None;
    write(store, DEVICE_KEY, &device)?;
    Ok(Prepared { device, secrets, path, seller })
}

/// The sample documents ZATCA's compliance step asks for, one per document
/// type the device issues, chained among themselves (ICV 1…, from the
/// first PIH; the device's own chain is not touched).
fn compliance_documents(seller: &Party, invoice_types: &str, keys: (&khata_sa::sign::Signer, &SigningCertificate)) -> ApiResult<Vec<(String, zatca::Generated)>> {
    let t = invoice_types.as_bytes();
    let mut kinds = Vec::new();
    if t.first() == Some(&b'1') {
        kinds.push(InvoiceKind::Standard);
    }
    if t.get(1) == Some(&b'1') {
        kinds.push(InvoiceKind::Simplified);
    }
    let buyer = Party {
        name: "Compliance Check Buyer".into(),
        vat_number: Some("399999999800003".into()),
        other_id: None,
        address: Address {
            street: "King Abdulaziz Road".into(),
            building_number: "1111".into(),
            district: "Al Murabba".into(),
            city: "Riyadh".into(),
            postal_code: "12345".into(),
            region: "Riyadh".into(),
            country: "SA".into(),
            ..Address::default()
        },
    };
    let now = Utc::now().naive_utc();
    let stamp = Stamp::new(keys.0, keys.1, now).map_err(err)?;
    let riyadh = now + chrono::Duration::hours(3);
    let mut chain = zatca::ChainState::default();
    let mut out = Vec::new();
    for kind in kinds {
        for doc_type in [DocumentType::Invoice, DocumentType::CreditNote, DocumentType::DebitNote] {
            let n = chain.last_icv + 1;
            let inv = EInvoice {
                kind,
                doc_type,
                number: format!("CMPL-{n}"),
                uuid: uuid::Uuid::new_v4().to_string(),
                issue_date: riyadh.date(),
                issue_time: riyadh_now(),
                supply_date: Some(riyadh.date()),
                export: false,
                billing_reference: (doc_type != DocumentType::Invoice).then(|| "CMPL-1".to_owned()),
                reason: (doc_type != DocumentType::Invoice).then(|| "Compliance check".to_owned()),
                payment_means: "10".into(),
                seller: seller.clone(),
                buyer: Some(buyer.clone()),
                lines: vec![Line {
                    name: "Compliance check item".into(),
                    quantity: Decimal::ONE,
                    unit_code: String::new(),
                    unit_price: Decimal::ONE_HUNDRED,
                    net_amount: Decimal::ONE_HUNDRED,
                    category: Category::standard(Decimal::from(15)),
                }],
                allowances: vec![],
            };
            let g = zatca::generate_next(&inv, &mut chain, Some(&stamp)).map_err(err)?;
            let what = match doc_type {
                DocumentType::Invoice => "invoice",
                DocumentType::CreditNote => "credit note",
                DocumentType::DebitNote => "debit note",
            };
            out.push((format!("{} {what}", kind_name(kind)), g));
        }
    }
    Ok(out)
}

/// The network part of onboarding, outside the company lock: compliance
/// CSID (with the OTP), the compliance checks, the production CSID. Saves
/// the secrets file after each CSID; returns the device with its results.
fn run_onboarding(p: &mut Prepared, otp: &str, client: &Client) -> ApiResult<()> {
    if p.secrets.compliance.is_none() {
        let csr_value = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, p.device.csr_pem.as_bytes());
        if otp.trim().is_empty() {
            return Err("enter the OTP from the Fatoora portal (valid for one hour)".into());
        }
        let c = client.compliance_csid(otp, &csr_value)?;
        p.secrets.compliance = Some(c);
        write_secrets(&p.path, &p.secrets)?;
        p.device.compliance_csid = true;
    }
    let compliance = p.secrets.compliance.clone().ok_or("no compliance CSID")?;
    let signer = khata_sa::sign::Signer::from_pem(&p.secrets.private_key_pem).map_err(err)?;
    let cert = SigningCertificate::from_binary_security_token(&compliance.token).map_err(err)?;
    if cert.public_key_der() != signer.public_key_der().as_slice() {
        return Err("ZATCA's compliance certificate is not for this device's key; choose Start again".into());
    }
    p.device.certificate_expiry = chrono::DateTime::from_timestamp(cert.not_after_unix() as i64, 0).map(|t| t.format("%Y-%m-%d").to_string());
    if !p.device.production_csid {
        let docs = compliance_documents(&p.seller, &p.device.invoice_types, (&signer, &cert))?;
        p.device.checks.clear();
        for (what, g) in &docs {
            let o = client.compliance_check(&compliance, Document { xml: &g.xml, hash: &g.hash, uuid: &g.uuid })?;
            p.device.checks.push(ComplianceCheck { document: what.clone(), status: o.status.clone(), warnings: texts(&o.warnings), errors: texts(&o.errors) });
        }
        if let Some(bad) = p.device.checks.iter().find(|c| c.status == "ERROR") {
            return Err(format!("ZATCA's compliance check of the {} failed: {}", bad.document, bad.errors.join("; ")));
        }
    }
    if p.secrets.production.is_none() {
        let prod = client.production_csid(&compliance)?;
        p.secrets.production = Some(prod);
        write_secrets(&p.path, &p.secrets)?;
    }
    p.device.production_csid = true;
    Ok(())
}

/// Onboard this computer as a ZATCA device: build the key and CSR, get the
/// compliance CSID with the OTP, pass the compliance checks and get the
/// production CSID. Re-run without an OTP to finish after a failure.
pub fn zatca_onboard(slot: &Mutex<Slot>, data_dir: &Path, environment: &str, otp: &str, options: &OnboardOptions) -> ApiResult<ZatcaStatusDto> {
    let env = Environment::parse(environment).ok_or("the environment is sandbox, simulation or production")?;
    onboard_with(slot, data_dir, env, otp, options, &Client::new(env), None)
}

pub(crate) fn onboard_with(
    slot: &Mutex<Slot>,
    data_dir: &Path,
    env: Environment,
    otp: &str,
    options: &OnboardOptions,
    client: &Client,
    entropy: Option<[u8; 32]>,
) -> ApiResult<ZatcaStatusDto> {
    let lock = || slot.lock().map_err(|_| "company state is poisoned".to_owned());
    let mut p = prepare_onboarding(&mut *lock()?, data_dir, env, options, entropy)?;
    let result = run_onboarding(&mut p, otp, client);
    p.device.last_error = result.as_ref().err().cloned();
    {
        let mut g = lock()?;
        let (store, _) = open_mut(&mut g)?;
        // Only if the company still has this device (it wasn't restarted meanwhile).
        if read::<Device>(store, DEVICE_KEY)?.is_some_and(|d| d.device_id == p.device.device_id) {
            write(store, DEVICE_KEY, &p.device)?;
        }
    }
    result?;
    let g = lock()?;
    status_of(store_ref(&g)?, data_dir)
}

/// What is kept of a submission (`zatca.submission.<voucher id>`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Submission {
    kind: String,
    /// "clearance" or "reporting".
    endpoint: String,
    environment: String,
    submitted_at: String,
    hash: String,
    accepted: bool,
    outcome: Outcome,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZatcaSubmissionDto {
    pub voucher_id: String,
    pub kind: String,
    pub endpoint: String,
    pub environment: String,
    pub submitted_at: String,
    /// ZATCA took it (cleared or reported, perhaps with warnings).
    pub accepted: bool,
    /// Sent before; this is the stored answer.
    pub already_submitted: bool,
    pub http_status: u16,
    /// PASS, WARNING or ERROR.
    pub status: String,
    pub reporting_status: Option<String>,
    pub clearance_status: Option<String>,
    pub info: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    /// The XML ZATCA cleared (standard documents): give this to the buyer.
    pub cleared_xml: Option<String>,
    pub file_name: String,
    pub notes: Vec<String>,
}

fn submission_dto(voucher_id: &str, s: &Submission, already: bool, file_name: String, notes: Vec<String>) -> ZatcaSubmissionDto {
    let o = &s.outcome;
    ZatcaSubmissionDto {
        voucher_id: voucher_id.to_owned(),
        kind: s.kind.clone(),
        endpoint: s.endpoint.clone(),
        environment: s.environment.clone(),
        submitted_at: s.submitted_at.clone(),
        accepted: s.accepted,
        already_submitted: already,
        http_status: o.http_status,
        status: o.status.clone(),
        reporting_status: o.reporting_status.clone(),
        clearance_status: o.clearance_status.clone(),
        info: texts(&o.info),
        warnings: texts(&o.warnings),
        errors: texts(&o.errors),
        cleared_xml: o.cleared_xml.clone(),
        file_name,
        notes: notes.into_iter().chain(o.note.clone()).collect(),
    }
}

/// The QR payload in an e-invoice XML.
fn qr_of(xml: &str) -> Option<String> {
    let at = xml.find("<cbc:ID>QR</cbc:ID>")?;
    let rest = &xml[at..];
    let open = rest.find("<cbc:EmbeddedDocumentBinaryObject")?;
    let start = open + rest[open..].find('>')? + 1;
    let end = start + rest[start..].find("</cbc:EmbeddedDocumentBinaryObject>")?;
    Some(rest[start..end].trim().to_owned())
}

/// The kind to submit: the recorded one, the one asked for, else standard
/// for a buyer with a valid VAT number and simplified otherwise.
fn submit_kind(store: &Store, book: &Book, id: VoucherId, asked: Option<&str>) -> ApiResult<InvoiceKind> {
    if let Some(r) = read::<Record>(store, &format!("{DOC_PREFIX}{}", id.0))? {
        return parse_kind(&r.kind);
    }
    if let Some(k) = asked.filter(|k| !k.trim().is_empty()) {
        return parse_kind(k);
    }
    let v = book.voucher(id).ok_or("unknown voucher")?;
    let p = parse(book, v);
    let registered = p
        .party
        .and_then(|i| book.ledger(v.entries[i].ledger))
        .and_then(|l| l.tax_id.as_deref())
        .is_some_and(|t| khata_sa::vat_number::validate(t).is_ok());
    Ok(if registered { InvoiceKind::Standard } else { InvoiceKind::Simplified })
}

/// Send a voucher's e-invoice to ZATCA: generate and stamp it if needed,
/// then clear it (standard) or report it (simplified), and keep ZATCA's
/// answer. A document ZATCA already took is not sent again.
pub fn zatca_submit(slot: &Mutex<Slot>, data_dir: &Path, voucher_id: &str, kind: Option<&str>) -> ApiResult<ZatcaSubmissionDto> {
    submit_with(slot, data_dir, voucher_id, kind, None)
}

pub(crate) fn submit_with(slot: &Mutex<Slot>, data_dir: &Path, voucher_id: &str, kind: Option<&str>, client: Option<&Client>) -> ApiResult<ZatcaSubmissionDto> {
    submit_inner(slot, data_dir, voucher_id, kind, client, Utc::now()).map_err(|e| e.message)
}

/// Why a submission did not reach an answer from ZATCA.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SubmitError {
    kind: crate::zatca_api::CallFailure,
    message: String,
}

impl SubmitError {
    fn other(message: impl Into<String>) -> Self {
        Self { kind: crate::zatca_api::CallFailure::Other, message: message.into() }
    }
}

impl From<String> for SubmitError {
    fn from(message: String) -> Self {
        Self::other(message)
    }
}

impl From<&str> for SubmitError {
    fn from(message: &str) -> Self {
        Self::other(message)
    }
}

impl From<crate::zatca_api::CallError> for SubmitError {
    fn from(e: crate::zatca_api::CallError) -> Self {
        Self { kind: e.kind, message: e.message }
    }
}

fn submit_inner(
    slot: &Mutex<Slot>,
    data_dir: &Path,
    voucher_id: &str,
    kind: Option<&str>,
    client: Option<&Client>,
    now: chrono::DateTime<Utc>,
) -> Result<ZatcaSubmissionDto, SubmitError> {
    let lock = || slot.lock().map_err(|_| SubmitError::other("company state is poisoned"));
    // Under the lock: the stamped document.
    let (keys, inv, r, kind, mut notes) = {
        let mut g = lock()?;
        let (store, book) = open_mut(&mut g)?;
        ksa_only(book)?;
        let id = VoucherId(parse_uuid(voucher_id, "voucher")?);
        if let Some(s) = read::<Submission>(store, &format!("{SUBMISSION_PREFIX}{}", id.0))?.filter(|s| s.accepted) {
            let file = read::<Record>(store, &format!("{DOC_PREFIX}{}", id.0))?
                .and_then(|r| {
                    let t = NaiveTime::parse_from_str(&r.issue_time, "%H:%M:%S").ok()?;
                    einvoice(book, store, book.voucher(id)?, parse_kind(&r.kind).ok()?, &r.uuid, t).ok().map(|(inv, _)| zatca::file_name(&inv))
                })
                .unwrap_or_default();
            dequeue(store, voucher_id)?;
            return Ok(submission_dto(voucher_id, &s, true, file, vec![]));
        }
        let keys = signing_keys(store, data_dir)?.ok_or("onboard this computer with ZATCA first (ZATCA → Onboarding)")?;
        let not_after = chrono::DateTime::from_timestamp(keys.certificate.not_after_unix() as i64, 0);
        if let Some(t) = not_after.filter(|t| *t <= now) {
            return Err(SubmitError {
                kind: crate::zatca_api::CallFailure::Credentials,
                message: format!("the CSID's certificate expired on {}: renew it with a new OTP (ZATCA → Onboarding); the document stays queued", t.format("%Y-%m-%d")),
            });
        }
        let kind = submit_kind(store, book, id, kind)?;
        let (inv, r, _, warnings) = document(store, book, id, kind, Some((&keys.signer, &keys.certificate)))?;
        if r.signing_time.is_none() {
            return Err("this e-invoice was generated before onboarding and its voucher has changed since, so it cannot be stamped; correct it with a credit or debit note".into());
        }
        let mut notes = warnings;
        notes.extend(keys.warnings.iter().cloned());
        (keys, inv, r, kind, notes)
    };
    // Outside the lock: the network call.
    let default_client;
    let client = match client {
        Some(c) => c,
        None => {
            default_client = Client::new(keys.environment);
            &default_client
        }
    };
    let doc = Document { xml: &r.xml, hash: &r.hash, uuid: &r.uuid };
    let sent = (|| -> Result<(String, Outcome), crate::zatca_api::CallError> {
        let (endpoint, mut outcome) = if kind == InvoiceKind::Standard {
            ("clearance", client.clear(&keys.production, doc)?)
        } else {
            ("reporting", client.report(&keys.production, doc)?)
        };
        let mut endpoint = endpoint.to_owned();
        if kind == InvoiceKind::Standard && outcome.http_status == 303 {
            notes.push("ZATCA has clearance switched off for this taxpayer (303): the invoice was reported instead".into());
            outcome = client.report(&keys.production, doc)?;
            endpoint = "reporting".into();
        }
        Ok((endpoint, outcome))
    })();
    let (endpoint, outcome) = match sent {
        Ok(x) => x,
        Err(e) => {
            if e.kind == crate::zatca_api::CallFailure::Credentials {
                // Expired or revoked: say so on the device until renewed.
                let mut g = lock()?;
                let (store, _) = open_mut(&mut g)?;
                if let Some(mut d) = read::<Device>(store, DEVICE_KEY)? {
                    d.csid_refused = Some(format!("{}: {}", utc_text(now), e.message));
                    write(store, DEVICE_KEY, &d)?;
                }
            }
            return Err(e.into());
        }
    };
    let accepted = outcome.accepted() || outcome.http_status == 409 || outcome.http_status == 208;
    if outcome.http_status == 409 || outcome.http_status == 208 {
        notes.push("ZATCA already had this document".into());
    }
    let sub = Submission {
        kind: kind_name(kind).into(),
        endpoint,
        environment: keys.environment.name().into(),
        submitted_at: utc_text(now),
        hash: r.hash.clone(),
        accepted,
        outcome,
    };
    {
        let mut g = lock()?;
        let (store, _) = open_mut(&mut g)?;
        write(store, &format!("{SUBMISSION_PREFIX}{voucher_id}"), &sub)?;
        if accepted {
            dequeue(store, voucher_id)?;
        } else {
            let errors = texts(&sub.outcome.errors).join("; ");
            update_entry(store, voucher_id, |q| {
                q.failed = true;
                q.attempts += 1;
                q.last_attempt = Some(utc_text(now));
                q.next_attempt = None;
                q.last_error = Some(if errors.is_empty() { format!("ZATCA refused it ({})", sub.outcome.http_status) } else { errors });
            })?;
        }
    }
    Ok(submission_dto(voucher_id, &sub, false, zatca::file_name(&inv), notes))
}

/// ZATCA's stored answer for a voucher, if it was submitted.
pub fn zatca_submission(slot: &Slot, voucher_id: &str) -> ApiResult<Option<ZatcaSubmissionDto>> {
    let book = open_ref(slot)?;
    ksa_only(book)?;
    let id = VoucherId(parse_uuid(voucher_id, "voucher")?);
    let store = store_ref(slot)?;
    Ok(read::<Submission>(store, &format!("{SUBMISSION_PREFIX}{}", id.0))?.map(|s| submission_dto(voucher_id, &s, true, String::new(), vec![])))
}

// ------------------------------------------------------- reporting queue

/// The queue of documents waiting for ZATCA (`zatca.queue` in the company
/// file's settings, per device like the chain). A simplified document must
/// be reported within 24 hours of its generation (E-Invoicing
/// Implementation Resolution, Second(3)(B) [PRIMARY]); a standard one must
/// be cleared before it is shared with the buyer. Documents join the queue
/// when they are generated and leave it once ZATCA accepts them. A
/// transient failure (no connection, 429, 5xx) is retried with a growing
/// delay ([`retry_delay`]); a refusal waits until the user corrects the
/// voucher and submits again (the entry is "failed"); a refused CSID (401)
/// stops the run.
const QUEUE_KEY: &str = "zatca.queue";

/// Report simplified documents within this many hours of generation.
pub const REPORTING_HOURS: i64 = 24;

/// One queued document's attempts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct QueueEntry {
    voucher_id: String,
    attempts: u32,
    last_attempt: Option<String>,
    /// Not before this time (UTC) on an automatic run.
    next_attempt: Option<String>,
    last_error: Option<String>,
    /// ZATCA refused the document: not retried until the user submits again.
    failed: bool,
}

fn queue(store: &Store) -> ApiResult<Vec<QueueEntry>> {
    Ok(read(store, QUEUE_KEY)?.unwrap_or_default())
}

fn enqueue(store: &Store, voucher_id: &str) -> ApiResult<()> {
    let mut q = queue(store)?;
    if !q.iter().any(|e| e.voucher_id == voucher_id) {
        q.push(QueueEntry { voucher_id: voucher_id.to_owned(), ..QueueEntry::default() });
        write(store, QUEUE_KEY, &q)?;
    }
    Ok(())
}

fn dequeue(store: &Store, voucher_id: &str) -> ApiResult<()> {
    let mut q = queue(store)?;
    let before = q.len();
    q.retain(|e| e.voucher_id != voucher_id);
    if q.len() != before {
        write(store, QUEUE_KEY, &q)?;
    }
    Ok(())
}

fn update_entry(store: &Store, voucher_id: &str, f: impl FnOnce(&mut QueueEntry)) -> ApiResult<()> {
    let mut q = queue(store)?;
    let at = match q.iter().position(|e| e.voucher_id == voucher_id) {
        Some(i) => i,
        None => {
            q.push(QueueEntry { voucher_id: voucher_id.to_owned(), ..QueueEntry::default() });
            q.len() - 1
        }
    };
    f(&mut q[at]);
    write(store, QUEUE_KEY, &q)
}

/// The wait before automatic attempt `attempts + 1`: 1 minute, doubling,
/// at most an hour. The guidelines ask for retries "in regular intervals
/// until successful" within the 24 hours [PRIMARY]; the intervals are a
/// KhataOS choice.
pub fn retry_delay(attempts: u32) -> chrono::Duration {
    let minutes = 1i64 << attempts.saturating_sub(1).min(6);
    chrono::Duration::minutes(minutes.min(60))
}

/// One document in the queue, for the screen.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZatcaQueueItem {
    pub voucher_id: String,
    pub number: String,
    pub date: String,
    pub party: String,
    /// "standard" or "simplified"; empty when not generated yet.
    pub kind: String,
    /// "pending", "retrying", "failed" or "not_generated".
    pub state: String,
    /// When the document was generated (UTC).
    pub generated_at: Option<String>,
    /// Report by (simplified documents: generation + 24 hours, UTC).
    pub deadline: Option<String>,
    /// Hours left before the deadline (negative: late).
    pub hours_left: Option<i64>,
    pub overdue: bool,
    pub attempts: u32,
    pub next_attempt: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZatcaQueueDto {
    pub items: Vec<ZatcaQueueItem>,
    pub pending: usize,
    pub failed: usize,
    pub overdue: usize,
    pub not_generated: usize,
    /// The device can submit (onboarded, CSID not expired or refused).
    pub ready: bool,
    pub notes: Vec<String>,
}

/// When a document was generated: as recorded, else its voucher date and
/// issue time in Riyadh (UTC+3).
fn generated_at(r: &Record, date: chrono::NaiveDate) -> Option<chrono::DateTime<Utc>> {
    r.generated_at.as_deref().and_then(parse_utc).or_else(|| {
        let t = NaiveTime::parse_from_str(&r.issue_time, "%H:%M:%S").ok()?;
        Some(date.and_time(t).and_utc() - chrono::Duration::hours(3))
    })
}

/// The vouchers that are e-invoices: sales, credit notes and debit notes to
/// a customer.
fn is_einvoice(book: &Book, v: &Voucher) -> bool {
    match v.kind {
        VoucherKind::Sales | VoucherKind::CreditNote => true,
        VoucherKind::DebitNote => party_line(book, v).is_some_and(|i| is_debtor(book, v.entries[i].ledger)),
        _ => false,
    }
}

fn queue_items(store: &Store, book: &Book, now: chrono::DateTime<Utc>) -> ApiResult<Vec<ZatcaQueueItem>> {
    let mut q = queue(store)?;
    // Documents generated before the queue existed, never accepted.
    let mut changed = false;
    for (k, _) in store.settings().map_err(err)? {
        let Some(id) = k.strip_prefix(DOC_PREFIX) else { continue };
        if q.iter().any(|e| e.voucher_id == id) {
            continue;
        }
        let accepted = read::<Submission>(store, &format!("{SUBMISSION_PREFIX}{id}"))?.is_some_and(|s| s.accepted);
        if !accepted {
            q.push(QueueEntry { voucher_id: id.to_owned(), ..QueueEntry::default() });
            changed = true;
        }
    }
    if changed {
        write(store, QUEUE_KEY, &q)?;
    }
    let mut items = Vec::new();
    for e in &q {
        let Ok(uuid) = parse_uuid(&e.voucher_id, "voucher") else { continue };
        let Some(v) = book.voucher(VoucherId(uuid)) else { continue };
        let r = read::<Record>(store, &format!("{DOC_PREFIX}{}", e.voucher_id))?;
        let party = party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger)).map(|l| l.name.clone()).unwrap_or_default();
        let made = r.as_ref().and_then(|r| generated_at(r, v.date));
        let simplified = r.as_ref().is_some_and(|r| r.kind == "simplified");
        let deadline = made.filter(|_| simplified).map(|g| g + chrono::Duration::hours(REPORTING_HOURS));
        let state = if r.is_none() {
            "not_generated"
        } else if e.failed {
            "failed"
        } else if e.attempts > 0 {
            "retrying"
        } else {
            "pending"
        };
        items.push(ZatcaQueueItem {
            voucher_id: e.voucher_id.clone(),
            number: v.number.clone(),
            date: v.date.to_string(),
            party,
            kind: r.as_ref().map(|r| r.kind.clone()).unwrap_or_default(),
            state: state.into(),
            generated_at: made.map(utc_text),
            deadline: deadline.map(utc_text),
            hours_left: deadline.map(|d| (d - now).num_hours()),
            overdue: deadline.is_some_and(|d| d < now),
            attempts: e.attempts,
            next_attempt: e.next_attempt.clone(),
            last_error: e.last_error.clone(),
        });
    }
    // E-invoices never generated, dated from the device's onboarding on.
    let device: Option<Device> = read(store, DEVICE_KEY)?;
    if let Some(since) = device.filter(|d| d.production_csid).and_then(|d| parse_utc(&d.created_at)).map(|t| t.date_naive()) {
        for v in book.vouchers().filter(|v| !v.cancelled && v.date >= since && is_einvoice(book, v)) {
            let id = v.id.0.to_string();
            if q.iter().any(|e| e.voucher_id == id) || read::<Record>(store, &format!("{DOC_PREFIX}{id}"))?.is_some() {
                continue;
            }
            let party = party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger)).map(|l| l.name.clone()).unwrap_or_default();
            items.push(ZatcaQueueItem {
                voucher_id: id,
                number: v.number.clone(),
                date: v.date.to_string(),
                party,
                kind: String::new(),
                state: "not_generated".into(),
                generated_at: None,
                deadline: None,
                hours_left: None,
                overdue: false,
                attempts: 0,
                next_attempt: None,
                last_error: None,
            });
        }
    }
    items.sort_by(|a, b| (a.deadline.is_none(), &a.deadline, &a.date, &a.number).cmp(&(b.deadline.is_none(), &b.deadline, &b.date, &b.number)));
    Ok(items)
}

fn queue_dto(store: &Store, book: &Book, data_dir: &Path, now: chrono::DateTime<Utc>) -> ApiResult<ZatcaQueueDto> {
    let items = queue_items(store, book, now)?;
    let status = status_at(store, data_dir, now)?;
    let ready = status.onboarded && !status.expired && status.csid_refused.is_none();
    let mut notes = Vec::new();
    if !status.onboarded {
        notes.push("This computer is not onboarded with ZATCA: documents wait here until it is.".to_owned());
    } else if !ready {
        notes.push("The CSID has expired or was refused: renew it (ZATCA → Onboarding) and the queue is sent again.".to_owned());
    }
    let overdue = items.iter().filter(|i| i.overdue).count();
    if overdue > 0 {
        notes.push(format!(
            "{overdue} simplified document(s) are past the 24-hour reporting window: send them now, and if ZATCA cannot be reached tell ZATCA through its form for reporting delays, keeping evidence of the attempts"
        ));
    }
    Ok(ZatcaQueueDto {
        pending: items.iter().filter(|i| matches!(i.state.as_str(), "pending" | "retrying")).count(),
        failed: items.iter().filter(|i| i.state == "failed").count(),
        not_generated: items.iter().filter(|i| i.state == "not_generated").count(),
        overdue,
        ready,
        notes,
        items,
    })
}

/// The documents waiting for ZATCA: pending, retrying, refused, and
/// e-invoices not generated yet.
pub fn zatca_queue(slot: &Slot, data_dir: &Path) -> ApiResult<ZatcaQueueDto> {
    let book = open_ref(slot)?;
    ksa_only(book)?;
    queue_dto(store_ref(slot)?, book, data_dir, Utc::now())
}

/// What a run of the queue did.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ZatcaQueueRunDto {
    /// Documents sent to ZATCA in this run.
    pub sent: usize,
    pub accepted: usize,
    /// Refused by ZATCA (they need correcting).
    pub refused: usize,
    /// Not reached ZATCA (offline, busy): tried again later.
    pub retry_later: usize,
    /// The run stopped early: why (a refused or expired CSID).
    pub stopped: Option<String>,
    pub queue: ZatcaQueueDto,
}

/// Send the queue. `all` (Submit all): every pending, refused and
/// not-yet-generated document, at once; otherwise (the automatic run) only
/// generated documents whose retry time has come, so it is cheap to call
/// every minute or two while the app is open and online.
pub fn zatca_queue_run(slot: &Mutex<Slot>, data_dir: &Path, all: bool) -> ApiResult<ZatcaQueueRunDto> {
    queue_run_with(slot, data_dir, all, None, Utc::now())
}

pub(crate) fn queue_run_with(
    slot: &Mutex<Slot>,
    data_dir: &Path,
    all: bool,
    client: Option<&Client>,
    now: chrono::DateTime<Utc>,
) -> ApiResult<ZatcaQueueRunDto> {
    let lock = || slot.lock().map_err(|_| "company state is poisoned".to_owned());
    let (todo, ready) = {
        let g = lock()?;
        let book = open_ref(&g)?;
        ksa_only(book)?;
        let dto = queue_dto(store_ref(&g)?, book, data_dir, now)?;
        let entries = queue(store_ref(&g)?)?;
        let due = |id: &str| {
            entries
                .iter()
                .find(|e| e.voucher_id == id)
                .and_then(|e| e.next_attempt.as_deref())
                .and_then(parse_utc)
                .is_none_or(|t| t <= now)
        };
        let todo: Vec<String> = dto
            .items
            .iter()
            .filter(|i| match i.state.as_str() {
                "pending" | "retrying" => all || due(&i.voucher_id),
                "failed" | "not_generated" => all,
                _ => false,
            })
            .map(|i| i.voucher_id.clone())
            .collect();
        (todo, dto.ready)
    };
    let mut run = ZatcaQueueRunDto { sent: 0, accepted: 0, refused: 0, retry_later: 0, stopped: None, queue: empty_queue() };
    if ready {
        for id in todo {
            match submit_inner(slot, data_dir, &id, None, client, now) {
                Ok(dto) => {
                    run.sent += usize::from(!dto.already_submitted);
                    if dto.accepted {
                        run.accepted += 1;
                    } else {
                        run.refused += 1;
                    }
                }
                Err(e) => {
                    let mut g = lock()?;
                    let (store, _) = open_mut(&mut g)?;
                    match e.kind {
                        crate::zatca_api::CallFailure::Transient => {
                            run.retry_later += 1;
                            update_entry(store, &id, |q| {
                                q.attempts += 1;
                                q.last_attempt = Some(utc_text(now));
                                q.next_attempt = Some(utc_text(now + retry_delay(q.attempts)));
                                q.last_error = Some(e.message.clone());
                            })?;
                        }
                        crate::zatca_api::CallFailure::Credentials => {
                            update_entry(store, &id, |q| q.last_error = Some(e.message.clone()))?;
                            run.stopped = Some(e.message);
                            break;
                        }
                        crate::zatca_api::CallFailure::Other => {
                            run.refused += 1;
                            update_entry(store, &id, |q| {
                                q.failed = true;
                                q.attempts += 1;
                                q.last_attempt = Some(utc_text(now));
                                q.next_attempt = None;
                                q.last_error = Some(e.message.clone());
                            })?;
                        }
                    }
                }
            }
        }
    }
    let g = lock()?;
    run.queue = queue_dto(store_ref(&g)?, open_ref(&g)?, data_dir, now)?;
    Ok(run)
}

fn empty_queue() -> ZatcaQueueDto {
    ZatcaQueueDto { items: vec![], pending: 0, failed: 0, overdue: 0, not_generated: 0, ready: false, notes: vec![] }
}

// ------------------------------------------------------- CSID renewal

/// Renew the production CSID before it expires (or after ZATCA refused
/// it): a new key and CSR for the same device (EGS unit name and serial),
/// sent with a new OTP from the Fatoora portal and the current production
/// CSID. The device's chain (ICV, PIH) carries on. A revoked CSID cannot be
/// renewed: onboard again (Start again). `options` fill the CSR as in
/// onboarding.
pub fn zatca_renew(slot: &Mutex<Slot>, data_dir: &Path, otp: &str, options: &OnboardOptions) -> ApiResult<ZatcaStatusDto> {
    renew_with(slot, data_dir, otp, options, None, None)
}

pub(crate) fn renew_with(
    slot: &Mutex<Slot>,
    data_dir: &Path,
    otp: &str,
    options: &OnboardOptions,
    client: Option<&Client>,
    entropy: Option<[u8; 32]>,
) -> ApiResult<ZatcaStatusDto> {
    let lock = || slot.lock().map_err(|_| "company state is poisoned".to_owned());
    if otp.trim().is_empty() {
        return Err("enter a new OTP from the Fatoora portal (valid for one hour) to renew the CSID".into());
    }
    // Under the lock: the new key (saved first) and CSR.
    let (mut device, mut secrets, path, env, csr_pem, new_key_pem) = {
        let mut g = lock()?;
        let (store, book) = open_mut(&mut g)?;
        crate::api::require(book, khata_core::UserRole::Owner)?;
        ksa_only(book)?;
        let device = read::<Device>(store, DEVICE_KEY)?.filter(|d| d.production_csid).ok_or("this computer has no production CSID to renew: onboard it first")?;
        let path = secrets_path(data_dir, &device.device_id);
        let mut secrets = read_secrets(&path)?
            .filter(|x| x.device_id == device.device_id && x.production.is_some())
            .ok_or("this device's credentials are not on this computer: onboard again")?;
        let env = Environment::parse(&secrets.environment).ok_or("unknown ZATCA environment in the credentials file")?;
        let vat = book.profile().and_then(|p| p.tax_id.clone()).unwrap_or_default();
        let seller = seller_party(book, store)?;
        let signer = new_key(entropy)?;
        let (_, built) = build_csr(book, &seller, &vat, env, options, &device.invoice_types, device.common_name.clone(), device.egs_serial.clone(), &signer)?;
        let key_pem = signer.to_sec1_pem().to_string();
        secrets.pending_key_pem = Some(key_pem.clone());
        write_secrets(&path, &secrets)?;
        (device, secrets, path, env, built.pem, key_pem)
    };
    let default_client;
    let client = match client {
        Some(c) => c,
        None => {
            default_client = Client::new(env);
            &default_client
        }
    };
    let production = secrets.production.clone().ok_or("no production CSID")?;
    let csr_value = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, csr_pem.as_bytes());
    let result = client.renew_production_csid(&production, otp, &csr_value).and_then(|renewed| {
        let cert = SigningCertificate::from_binary_security_token(&renewed.token).map_err(err)?;
        let signer = khata_sa::sign::Signer::from_pem(&new_key_pem).map_err(err)?;
        if cert.public_key_der() == signer.public_key_der().as_slice() {
            secrets.private_key_pem = new_key_pem.clone();
            device.csr_pem = csr_pem.clone();
        } else if env != Environment::Sandbox {
            return Err("ZATCA's renewed certificate is not for the new key; try again or onboard again".to_owned());
        }
        // The sandbox issues a sample certificate for another key: keep the
        // old key, which the compliance certificate stamps for.
        secrets.production = Some(renewed);
        Ok(())
    });
    secrets.pending_key_pem = None;
    let mut g = lock()?;
    let (store, _) = open_mut(&mut g)?;
    match &result {
        Ok(()) => {
            write_secrets(&path, &secrets)?;
            device.csid_refused = None;
            device.last_error = None;
            device.renewed_at = Some(utc_text(Utc::now()));
            if let Some(cert) = stamping_certificate(&secrets) {
                device.certificate_expiry = chrono::DateTime::from_timestamp(cert.not_after_unix() as i64, 0).map(|t| t.format("%Y-%m-%d").to_string());
            }
        }
        Err(e) => {
            write_secrets(&path, &secrets)?;
            device.last_error = Some(format!("renewal: {e}"));
        }
    }
    if read::<Device>(store, DEVICE_KEY)?.is_some_and(|d| d.device_id == device.device_id) {
        write(store, DEVICE_KEY, &device)?;
    }
    result?;
    status_of(store, data_dir)
}

// ------------------------------------------------------------ VAT return

/// VAT posted to ledgers classed "IMPORT" (import VAT paid at customs), and
/// output VAT posted on the voucher (credits less debits: a purchase's
/// self-accounted reverse charge).
fn import_and_output_vat(book: &Book, v: &Voucher) -> (Amount, Amount) {
    let (mut import, mut output) = (Amount::ZERO, Amount::ZERO);
    for e in &v.entries {
        let Some(l) = book.ledger(e.ledger) else { continue };
        let Some(t) = l.tax.as_ref().filter(|t| t.component.eq_ignore_ascii_case(vat::VAT)) else { continue };
        if l.tax_class.as_deref().is_some_and(|c| vat::mechanism(c).0 == Mechanism::Import) {
            import += if e.side == DrCr::Dr { e.amount } else { -e.amount };
        } else if t.side == khata_core::TaxSide::Output {
            output += if e.side == DrCr::Cr { e.amount } else { -e.amount };
        }
    }
    (import, output)
}

/// One voucher's lines with treatments and VAT for the return: the booked
/// VAT is spread over the standard-rated lines by value, as in `vat_ae.rs`.
/// A line classed "RC:…" (a purchase under the reverse charge) carries the
/// VAT worked out at its rate class instead; `self_output` is the output VAT
/// the voucher posts, checked against it. `vat` is the voucher's own VAT
/// (import VAT posted to ledgers classed "IMPORT" taken out).
fn return_lines(v: &Voucher, p: &Parsed, vat_total: Amount, rates: &RateSchedule, inward: bool, self_output: Amount, warnings: &mut Vec<String>) -> Vec<DocLine> {
    let name = format!("{} {}", label(v.kind), v.number);
    let mut rc_vat = Amount::ZERO;
    let mut out: Vec<DocLine> = p
        .lines
        .iter()
        .map(|l| {
            let (mechanism, base) = l.class.as_deref().map_or((Mechanism::Normal, String::new()), vat::mechanism);
            let key = if mechanism == Mechanism::Normal { l.class.clone() } else { Some(base) };
            let known = key.as_deref().and_then(|c| rates.rate(c, v.date).map(|r| (vat::treatment(c, r), r)));
            let treatment = known.map_or(Treatment::Standard, |(t, _)| t);
            let mut line = DocLine { treatment, taxable_value: l.amount, vat: Decimal::ZERO, mechanism };
            if mechanism == Mechanism::ReverseCharge && inward {
                match known {
                    Some((Treatment::Standard, r)) => {
                        line.vat = vat::round_halalas(l.amount * r / Decimal::ONE_HUNDRED);
                        rc_vat += line.vat;
                    }
                    Some(_) => {}
                    None => warnings.push(format!("{name}: a reverse-charge line has no rate on file for its class; its VAT is taken as 0")),
                }
            }
            line
        })
        .collect();
    if inward && (rc_vat - self_output).abs() > Decimal::ONE {
        warnings.push(format!(
            "{name}: the reverse charge worked out is VAT {}, but the voucher posts {} to output VAT (book Dr input VAT, Cr output VAT for it)",
            rc_vat.normalize(),
            self_output.normalize()
        ));
    }
    let carries = |l: &DocLine| l.mechanism != Mechanism::ReverseCharge;
    // Discounts reduce the standard-rated value first.
    let discount: Amount = p.discounts.iter().map(|d| d.1).sum();
    if !discount.is_zero() {
        let at = out.iter().position(|l| carries(l) && l.treatment == Treatment::Standard).or((!out.is_empty()).then_some(0));
        match at {
            Some(i) => out[i].taxable_value -= discount,
            None => warnings.push(format!("{name}: a discount with no lines is left out")),
        }
    }
    if out.is_empty() {
        if !vat_total.is_zero() {
            warnings.push(format!("{name} has VAT but no lines to put it on; its VAT is left out"));
        }
        return out;
    }
    if !vat_total.is_zero() && !out.iter().any(|l| carries(l) && l.treatment == Treatment::Standard) {
        if out.iter().any(carries) {
            warnings.push(format!("{name} has VAT but only zero-rated or exempt lines; it is reported as standard-rated"));
            for l in out.iter_mut().filter(|l| carries(l)) {
                l.treatment = Treatment::Standard;
            }
        } else {
            warnings.push(format!("{name} posts VAT {} beyond its reverse charge; it is left out", vat_total.normalize()));
            return out;
        }
    }
    let standard: Vec<usize> = (0..out.len()).filter(|&i| carries(&out[i]) && out[i].treatment == Treatment::Standard).collect();
    let weight: Decimal = standard.iter().map(|&i| out[i].taxable_value).sum();
    let last = standard.iter().rev().copied().find(|&i| !out[i].taxable_value.is_zero());
    let mut left = vat_total;
    for &i in &standard {
        let share = match last {
            Some(k) if i == k => left,
            Some(_) if !weight.is_zero() => (vat_total * out[i].taxable_value / weight).round_dp(2),
            _ => Decimal::ZERO,
        };
        left -= share;
        out[i].vat = share;
    }
    if last.is_none() && !vat_total.is_zero() {
        warnings.push(format!("{name}: its lines add up to zero, so its VAT is left out"));
    }
    out
}

/// Import VAT paid at customs on a voucher, as a one-line document for the
/// return's field 8.
fn import_vat_doc(v: &Voucher, vat: Amount) -> Option<InwardDoc> {
    (!vat.is_zero()).then(|| InwardDoc {
        id: format!("{}-import-vat", v.id.0),
        kind: DocKind::Invoice,
        number: v.number.clone(),
        date: v.date,
        party_name: "Customs".into(),
        party_vat: None,
        cancelled: v.cancelled,
        lines: vec![DocLine { treatment: Treatment::Standard, taxable_value: Decimal::ZERO, vat, mechanism: Mechanism::Import }],
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KsaVatReturnDto {
    pub vat_number: Option<String>,
    #[serde(flatten)]
    pub ret: VatReturn,
}

/// The KSA VAT return for a period: lines and warnings.
pub fn ksa_vat_return(slot: &Slot, from: &str, to: &str) -> ApiResult<KsaVatReturnDto> {
    let book = open_ref(slot)?;
    ksa_only(book)?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let rates = rates(book)?;
    let (mut outward, mut inward, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    let mut other_kinds = 0;
    let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| period.contains(v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let p = parse(book, v);
        let party_ledger = p.party.and_then(|i| book.ledger(v.entries[i].ledger));
        let (is_outward, kind) = match v.kind {
            VoucherKind::Sales => (true, DocKind::Invoice),
            VoucherKind::CreditNote => (true, DocKind::CreditNote),
            VoucherKind::DebitNote if party_ledger.is_some_and(|l| is_debtor(book, l.id)) => (true, DocKind::DebitNote),
            VoucherKind::Purchase => (false, DocKind::Invoice),
            // A purchase return: the supplier's credit note.
            VoucherKind::DebitNote => (false, DocKind::CreditNote),
            _ => {
                let (import, _) = import_and_output_vat(book, v);
                inward.extend(import_vat_doc(v, import));
                let taxed = v.entries.iter().any(|e| {
                    book.ledger(e.ledger).is_some_and(|l| {
                        l.tax.is_some() && !l.tax_class.as_deref().is_some_and(|c| vat::mechanism(c).0 == Mechanism::Import)
                    })
                });
                other_kinds += usize::from(taxed && !v.cancelled);
                continue;
            }
        };
        warnings.extend(p.warnings.iter().cloned());
        let (import, self_output) = import_and_output_vat(book, v);
        inward.extend(import_vat_doc(v, import));
        let mut lines = return_lines(v, &p, p.vat - import, &rates, !is_outward, self_output, &mut warnings);
        let export = is_outward && abroad(book, v, p.party) && lines.iter().all(|l| l.vat.is_zero());
        if !is_outward
            && !v.cancelled
            && abroad(book, v, p.party)
            && lines.iter().all(|l| l.mechanism == Mechanism::Normal && l.vat.is_zero())
            && lines.iter().any(|l| l.treatment == Treatment::Standard && !l.taxable_value.is_zero())
        {
            warnings.push(format!(
                "{} {} from a supplier abroad has no VAT: if it is an imported service, give its lines an RC: class; goods imported through customs an IMPORT: class",
                label(v.kind),
                v.number
            ));
        }
        if export {
            for l in lines.iter_mut().filter(|l| l.treatment == Treatment::Standard && l.mechanism == Mechanism::Normal) {
                l.treatment = Treatment::ZeroRated;
            }
        }
        let party_name = party_ledger.map(|l| l.name.clone()).unwrap_or_default();
        let id = v.id.0.to_string();
        if is_outward {
            outward.push(OutwardDoc { id, kind, number: v.number.clone(), date: v.date, party_name, export, cancelled: v.cancelled, lines });
        } else {
            inward.push(InwardDoc {
                id,
                kind,
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_vat: party_ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty()),
                cancelled: v.cancelled,
                lines,
            });
        }
    }
    if other_kinds > 0 {
        warnings.push(format!("{other_kinds} journal or other voucher(s) post to tax ledgers; they are not in this return"));
    }
    let mut ret = returns::vat_return(&outward, &inward, period);
    ret.warnings.splice(0..0, warnings);
    Ok(KsaVatReturnDto { vat_number: book.profile().and_then(|p| p.tax_id.clone()), ret })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, ledger, voucher};
    use crate::zatca_api::Client;
    use crate::api::{BillInput, StockLineInput, cancel_voucher, create_company, create_party_ledger, post_voucher, set_country};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

    const VAT_NO: &str = "300075588700003";
    const BUYER_VAT: &str = "311111111100003";

    pub(crate) fn saudi_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Riyadh Trading Co").unwrap();
        set_country(&mut slot, Some(&f.0), "SA").unwrap();
        (f, slot)
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

    struct Books {
        buyer: String,
        sales: String,
        output: String,
        laptop: String,
    }

    /// A Riyadh company with a VAT number, address, rates and a customer.
    fn setup(slot: &mut Slot) -> Books {
        set_company_tax(slot, VAT_NO, "SA-01").unwrap();
        set_company_address(slot, "King Fahd Road\nRiyadh 12211").unwrap();
        for (class, from, percent) in [("STANDARD", "2018-01-01", "5"), ("STANDARD", "2020-07-01", "15"), ("ZERO", "2018-01-01", "0"), ("EXEMPT", "2018-01-01", "0")] {
            set_tax_rate(slot, class, from, percent).unwrap();
        }
        let debtors = group_id(slot, "Sundry Debtors");
        let buyer = create_party_ledger(slot, "Jeddah Supplies", &debtors, "", None, Some(BUYER_VAT), None).unwrap().ledger.id;
        let with_address: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": "Jeddah Supplies",
            "group_id": debtors,
            "tax_id": BUYER_VAT,
            "mailing": { "address": "Tahlia Street\nJeddah", "region": "SA-02", "postal_code": "23434" },
        }))
        .unwrap();
        crate::masters::alter_ledger(slot, &buyer, &with_address).unwrap();
        let sales = ledger(slot, "Sales", "Sales Accounts", "");
        let output = ledger(slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(slot, &output, "VAT", TaxSide::Output).unwrap();
        let nos = save_unit(slot, "", "Nos", "", 0).unwrap().id;
        let laptop = item(slot, "Laptop", &nos, "STANDARD");
        Books { buyer, sales, output, laptop }
    }

    fn sale(slot: &mut Slot, b: &Books, date: &str, value: &str, vat: &str, total: &str) -> String {
        let mut line = entry(&b.sales, DrCr::Cr, value);
        line.items = items(&b.laptop, "1", value);
        post_voucher(slot, voucher("Sales", date, vec![entry(&b.buyer, DrCr::Dr, total), line, entry(&b.output, DrCr::Cr, vat)])).unwrap().id
    }

    #[test]
    fn seller_details_fall_back_to_the_address() {
        let (_f, mut slot) = saudi_company("zatca-seller");
        setup(&mut slot);
        let s = zatca_seller(&slot).unwrap();
        assert_eq!(s.stored, ZatcaSeller::default());
        assert_eq!((s.effective.street.as_str(), s.effective.city.as_str(), s.effective.postal_code.as_str()), ("King Fahd Road", "Riyadh", "12211"));
        assert!(set_zatca_seller(&mut slot, &ZatcaSeller { building_number: "12".into(), ..ZatcaSeller::default() }).is_err());
        let saved = set_zatca_seller(&mut slot, &ZatcaSeller {
            crn: " 1010010000 ".into(),
            building_number: "1234".into(),
            district: "Al Olaya".into(),
            city: "Ar Riyad".into(),
            ..ZatcaSeller::default()
        })
        .unwrap();
        assert_eq!((saved.stored.crn.as_str(), saved.effective.city.as_str(), saved.effective.street.as_str()), ("1010010000", "Ar Riyad", "King Fahd Road"));
    }

    #[test]
    fn invoices_chain_and_regenerate_identically() {
        let (_f, mut slot) = saudi_company("zatca-chain");
        let b = setup(&mut slot);
        set_zatca_seller(&mut slot, &ZatcaSeller { crn: "1010010000".into(), building_number: "1234".into(), district: "Al Olaya".into(), ..ZatcaSeller::default() }).unwrap();
        let first = sale(&mut slot, &b, "2026-05-01", "1000", "150", "1150");
        let second = sale(&mut slot, &b, "2026-05-02", "1999.90", "299.99", "2299.89");

        let a = zatca_invoice_xml(&mut slot, &first, "standard").unwrap();
        assert!(!a.regenerated);
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert_eq!((a.icv, a.pih.as_str()), (1, zatca::FIRST_PIH));
        for needle in [
            "<cbc:InvoiceTypeCode name=\"0100000\">388</cbc:InvoiceTypeCode>",
            "<cbc:ID schemeID=\"CRN\">1010010000</cbc:ID>",
            "<cbc:CountrySubentity>Riyadh</cbc:CountrySubentity>",
            &format!("<cbc:CompanyID>{BUYER_VAT}</cbc:CompanyID>"),
            "<cbc:RegistrationName>Jeddah Supplies</cbc:RegistrationName>",
            "<cbc:TaxInclusiveAmount currencyID=\"SAR\">1150.00</cbc:TaxInclusiveAmount>",
            "<cbc:PaymentMeansCode>30</cbc:PaymentMeansCode>",
            "<cbc:Name>Laptop</cbc:Name>",
            "<cbc:CityName>Jeddah</cbc:CityName>",
            "<cbc:CountrySubentity>Makkah</cbc:CountrySubentity>",
        ] {
            assert!(a.xml.contains(needle), "missing {needle:?}");
        }
        assert!(a.file_name.starts_with("300075588700003_20260501T") && a.file_name.ends_with(".xml"));

        let b2 = zatca_invoice_xml(&mut slot, &second, "simplified").unwrap();
        assert_eq!((b2.icv, b2.pih.as_str()), (2, a.hash.as_str()), "ICV counts up; PIH is the previous hash");
        assert!(b2.xml.contains("name=\"0200000\"") && b2.xml.contains("299.99"));

        let again = zatca_invoice_xml(&mut slot, &first, "standard").unwrap();
        assert!(again.regenerated);
        assert_eq!((again.xml.as_str(), again.icv, again.hash.as_str(), again.uuid.as_str()), (a.xml.as_str(), 1, a.hash.as_str(), a.uuid.as_str()));
        assert!(again.warnings.is_empty(), "{:?}", again.warnings);
        assert!(zatca_invoice_xml(&mut slot, &first, "simplified").unwrap_err().contains("already generated"));
        let third = sale(&mut slot, &b, "2026-05-03", "100", "15", "115");
        assert_eq!(zatca_invoice_xml(&mut slot, &third, "standard").unwrap().icv, 3, "never reused after a regeneration");

        // The QR decodes to tags 1 to 5, and the print shows it.
        let tags: Vec<u8> = khata_sa::qr::decode(&a.qr).unwrap().into_iter().map(|t| t.0).collect();
        assert_eq!(tags, [1, 2, 3, 4, 5]);
        let html = crate::invoice::voucher_html(&slot, &second).unwrap();
        assert!(html.contains("<svg") && html.contains("فاتورة ضريبية مبسطة") && html.contains("Simplified Tax Invoice"));
    }

    #[test]
    fn credit_notes_need_the_original_invoice() {
        let (_f, mut slot) = saudi_company("zatca-cn");
        let b = setup(&mut slot);
        sale(&mut slot, &b, "2026-05-01", "1000", "150", "1150");
        let mut party = entry(&b.buyer, DrCr::Cr, "230");
        let mut line = entry(&b.sales, DrCr::Dr, "200");
        line.items = items(&b.laptop, "1", "200");
        let cn = post_voucher(&mut slot, voucher("CreditNote", "2026-05-05", vec![party.clone(), line.clone(), entry(&b.output, DrCr::Dr, "30")])).unwrap().id;
        assert!(zatca_invoice_xml(&mut slot, &cn, "standard").unwrap_err().contains("invoice it corrects"));
        party.bills = vec![BillInput { kind: BillKind::Against, reference: "1".into(), amount: "230".into(), due: None }];
        let mut v = voucher("CreditNote", "2026-05-06", vec![party, line, entry(&b.output, DrCr::Dr, "30")]);
        v.narration = "Returned goods".into();
        let cn = post_voucher(&mut slot, v).unwrap().id;
        let g = zatca_invoice_xml(&mut slot, &cn, "standard").unwrap();
        assert!(g.xml.contains(">381</cbc:InvoiceTypeCode>"));
        assert!(g.xml.contains("<cac:InvoiceDocumentReference>\n            <cbc:ID>1</cbc:ID>"));
        assert!(g.xml.contains("<cbc:InstructionNote>Returned goods</cbc:InstructionNote>"));
        assert_eq!(g.icv, 1, "a refused build doesn't use a counter value");
    }

    #[test]
    fn only_for_saudi_companies() {
        let (_f, mut slot) = crate::api::tests::uae_company("zatca-uae");
        assert!(zatca_invoice_xml(&mut slot, &uuid::Uuid::new_v4().to_string(), "standard").unwrap_err().contains("Saudi Arabia"));
        assert!(ksa_vat_return(&slot, "2026-01-01", "2026-03-31").unwrap_err().contains("Saudi Arabia"));
        let (_g, mut slot) = saudi_company("zatca-kind");
        assert!(zatca_invoice_xml(&mut slot, "x", "b2b").unwrap_err().contains("standard or simplified"));
    }

    #[test]
    fn vat_return_from_a_small_riyadh_company() {
        let (_f, mut slot) = saudi_company("ksa-return");
        let b = setup(&mut slot);
        let creditors = group_id(&slot, "Sundry Creditors");
        let supplier = create_party_ledger(&mut slot, "Dammam Wholesale", &creditors, "", None, Some("322222222200003"), None).unwrap().ledger.id;
        let unregistered = ledger(&mut slot, "Small Trader", "Sundry Creditors", "");
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let input = ledger(&mut slot, "Input VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &input, "VAT", TaxSide::Input).unwrap();
        let nos = save_unit(&mut slot, "", "Pcs", "", 0).unwrap().id;
        let medicine = item(&mut slot, "Medicine", &nos, "ZERO VATEX-SA-35");
        let lease = item(&mut slot, "Flat lease", &nos, "EXEMPT");
        set_tax_rate(&mut slot, "ZERO VATEX-SA-35", "2018-01-01", "0").unwrap();
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap().id;

        // Standard, zero-rated and exempt lines on one invoice.
        let mut mixed = entry(&b.sales, DrCr::Cr, "13000");
        mixed.items = items(&b.laptop, "1", "10000");
        mixed.items.extend(items(&medicine, "1", "2000"));
        mixed.items.extend(items(&lease, "1", "1000"));
        post(&mut slot, "Sales", "2026-04-10", vec![entry(&b.buyer, DrCr::Dr, "14500"), mixed, entry(&b.output, DrCr::Cr, "1500")]);
        let cancelled = sale(&mut slot, &b, "2026-04-11", "1000", "150", "1150");
        cancel_voucher(&mut slot, &cancelled).unwrap();
        // A sales return.
        let mut back = entry(&b.sales, DrCr::Dr, "2000");
        back.items = items(&b.laptop, "1", "2000");
        post(&mut slot, "CreditNote", "2026-05-02", vec![entry(&b.buyer, DrCr::Cr, "2300"), back, entry(&b.output, DrCr::Dr, "300")]);
        // Purchases, a purchase return, and VAT from a supplier without a VAT number.
        let mut bought = entry(&purchase, DrCr::Dr, "6000");
        bought.items = items(&b.laptop, "1", "6000");
        post(&mut slot, "Purchase", "2026-05-05", vec![entry(&supplier, DrCr::Cr, "6900"), bought, entry(&input, DrCr::Dr, "900")]);
        let mut returned = entry(&purchase, DrCr::Cr, "1000");
        returned.items = items(&b.laptop, "1", "1000");
        post(&mut slot, "DebitNote", "2026-05-12", vec![entry(&supplier, DrCr::Dr, "1150"), returned, entry(&input, DrCr::Cr, "150")]);
        post(&mut slot, "Purchase", "2026-05-20", vec![entry(&unregistered, DrCr::Cr, "115"), entry(&purchase, DrCr::Dr, "100"), entry(&input, DrCr::Dr, "15")]);
        // Outside the quarter.
        sale(&mut slot, &b, "2026-07-01", "1000", "150", "1150");

        let r = ksa_vat_return(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!(r.vat_number.as_deref(), Some(VAT_NO));
        let ret = &r.ret;
        assert_eq!((ret.standard_rated_sales.amount, ret.standard_rated_sales.vat), (dec!(8000), dec!(1200)), "10000 − 2000 returned");
        assert_eq!((ret.zero_rated_domestic_sales, ret.exports, ret.exempt_sales), (dec!(2000), dec!(0), dec!(1000)));
        assert_eq!(ret.total_sales.amount, dec!(11000));
        assert_eq!((ret.standard_rated_purchases.amount, ret.standard_rated_purchases.vat), (dec!(5000), dec!(750)));
        assert_eq!((ret.total_vat_due, ret.net), (dec!(450), dec!(450)));
        assert!(ret.warnings.iter().any(|w| w.contains("no supplier VAT number")), "{:?}", ret.warnings);
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["vat_number"].as_str(), j["net"].as_str()), (Some(VAT_NO), Some("450")));
    }

    #[test]
    fn vat_return_reverse_charge_and_customs_imports() {
        let (_f, mut slot) = saudi_company("ksa-return-rc");
        let b = setup(&mut slot);
        let creditors = group_id(&slot, "Sundry Creditors");
        let alter = |slot: &mut Slot, id: &str, fields: serde_json::Value| {
            let input: crate::masters::LedgerInput = serde_json::from_value(fields).unwrap();
            crate::masters::alter_ledger(slot, id, &input).unwrap();
        };
        let saas = ledger(&mut slot, "Cloud Inc", "Sundry Creditors", "");
        alter(&mut slot, &saas, serde_json::json!({ "name": "Cloud Inc", "group_id": creditors, "mailing": { "country": "US" } }));
        let maker = ledger(&mut slot, "Shenzhen Maker", "Sundry Creditors", "");
        alter(&mut slot, &maker, serde_json::json!({ "name": "Shenzhen Maker", "group_id": creditors, "mailing": { "country": "CN" } }));
        let expenses = group_id(&slot, "Indirect Expenses");
        let software = ledger(&mut slot, "Software", "Indirect Expenses", "");
        alter(&mut slot, &software, serde_json::json!({ "name": "Software", "group_id": expenses, "tax_class": "RC:STANDARD" }));
        let purchase = ledger(&mut slot, "Purchase", "Purchase Accounts", "");
        let input = ledger(&mut slot, "Input VAT", "Duties & Taxes", "");
        let customs = ledger(&mut slot, "Import VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &input, "VAT", TaxSide::Input).unwrap();
        set_ledger_tax(&mut slot, &customs, "VAT", TaxSide::Input).unwrap();
        let duties = group_id(&slot, "Duties & Taxes");
        alter(&mut slot, &customs, serde_json::json!({ "name": "Import VAT", "group_id": duties, "tax_class": "IMPORT" }));
        let bank = ledger(&mut slot, "Al Rajhi", "Bank Accounts", "");
        let nos = save_unit(&mut slot, "", "Pcs", "", 0).unwrap().id;
        let machine = item(&mut slot, "Machine", &nos, "IMPORT:STANDARD");
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap().id;
        sale(&mut slot, &b, "2026-04-10", "4000", "600", "4600");
        post(&mut slot, "Purchase", "2026-04-11", vec![entry(&saas, DrCr::Cr, "10000"), entry(&software, DrCr::Dr, "10000"), entry(&input, DrCr::Dr, "1500"), entry(&b.output, DrCr::Cr, "1500")]);
        let mut goods = entry(&purchase, DrCr::Dr, "20000");
        goods.items = items(&machine, "1", "20000");
        post(&mut slot, "Purchase", "2026-04-12", vec![entry(&maker, DrCr::Cr, "20000"), goods]);
        post(&mut slot, "Payment", "2026-04-13", vec![entry(&customs, DrCr::Dr, "3000"), entry(&bank, DrCr::Cr, "3000")]);
        let r = ksa_vat_return(&slot, "2026-04-01", "2026-06-30").unwrap().ret;
        assert_eq!((r.imports_reverse_charge.amount, r.imports_reverse_charge.vat), (dec!(10000), dec!(1500)));
        assert_eq!((r.imports_paid_at_customs.amount, r.imports_paid_at_customs.vat), (dec!(20000), dec!(3000)));
        assert_eq!(r.standard_rated_sales.vat, dec!(600));
        assert_eq!((r.total_vat_due, r.net), (dec!(-2400), dec!(-2400)), "600 due − 3000 import VAT; the reverse charge nets out");
        assert!(!r.warnings.iter().any(|w| w.contains("journal or other") || w.contains("output VAT (book")), "{:?}", r.warnings);
    }

    #[test]
    fn exports_are_zero_rated_with_vatex_codes() {
        let (_f, mut slot) = saudi_company("zatca-export");
        let b = setup(&mut slot);
        let buyer = ledger(&mut slot, "Mumbai Importers", "Sundry Debtors", "");
        let abroad: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": "Mumbai Importers",
            "group_id": group_id(&slot, "Sundry Debtors"),
            "mailing": { "country": "IN", "address": "Nariman Point\nMumbai" },
        }))
        .unwrap();
        crate::masters::alter_ledger(&mut slot, &buyer, &abroad).unwrap();
        let mut line = entry(&b.sales, DrCr::Cr, "4000");
        line.items = items(&b.laptop, "2", "2000");
        let id = post_voucher(&mut slot, voucher("Sales", "2026-02-02", vec![entry(&buyer, DrCr::Dr, "4000"), line])).unwrap().id;
        let g = zatca_invoice_xml(&mut slot, &id, "standard").unwrap();
        for needle in ["name=\"0100100\"", "<cbc:TaxExemptionReasonCode>VATEX-SA-32</cbc:TaxExemptionReasonCode>", "<cbc:IdentificationCode>IN</cbc:IdentificationCode>", "<cbc:CityName>Mumbai</cbc:CityName>"] {
            assert!(g.xml.contains(needle), "missing {needle:?}");
        }
        let r = ksa_vat_return(&slot, "2026-01-01", "2026-03-31").unwrap();
        assert_eq!((r.ret.exports, r.ret.standard_rated_sales.amount), (dec!(4000), dec!(0)));
    }

    // ------------------------------------------- onboarding and submission

    /// A self-signed certificate for the test key `[0x11; 32]` (never a real key).
    const TEST_CERT: &str = include_str!("../../../../crates/khata-sa/tests/data/test-cert.b64");
    const TEST_KEY: [u8; 32] = [0x11; 32];

    /// A local stand-in for ZATCA's APIs, answering as the sandbox does
    /// (shapes from ZATCA's API specification), and checking what KhataOS
    /// sends: headers, Basic auth, the hash and the stamp.
    mod mock {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::Json;
        use axum::extract::State;
        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::post;
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD;
        use serde_json::{Value, json};

        #[derive(Clone)]
        pub struct Cfg {
            pub token: String,
            pub clearance_off: bool,
            pub calls: Arc<AtomicUsize>,
            /// The production CSID's current secret (a renewal changes it).
            pub secret: Arc<std::sync::Mutex<String>>,
            /// Answer this many document calls with 503 (ZATCA down).
            pub fail_next: Arc<AtomicUsize>,
            /// The CSID is revoked: 401 on documents and renewal.
            pub revoked: Arc<std::sync::atomic::AtomicBool>,
        }

        fn production_auth(c: &Cfg) -> String {
            basic(&c.token, &c.secret.lock().unwrap())
        }

        /// 503 while `fail_next` lasts, 401 when revoked.
        fn outage(c: &Cfg) -> Option<Reply> {
            if c.revoked.load(Ordering::SeqCst) {
                return Some((StatusCode::UNAUTHORIZED, Json(json!({ "status": 401 }))));
            }
            let left = c.fail_next.load(Ordering::SeqCst);
            if left > 0 {
                c.fail_next.store(left - 1, Ordering::SeqCst);
                return Some((StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "message": "Service Unavailable" }))));
            }
            None
        }

        fn basic(token: &str, secret: &str) -> String {
            format!("Basic {}", STANDARD.encode(format!("{token}:{secret}")))
        }

        fn header<'h>(h: &'h HeaderMap, k: &str) -> &'h str {
            h.get(k).and_then(|v| v.to_str().ok()).unwrap_or_default()
        }

        type Reply = (StatusCode, Json<Value>);

        fn csid(token: &str, secret: &str, id: Value) -> Reply {
            (StatusCode::OK, Json(json!({ "requestID": id, "dispositionMessage": "ISSUED", "binarySecurityToken": token, "secret": secret, "errors": null })))
        }

        /// Validate a document as ZATCA's validator would on the points it
        /// checked live: the body hash, and the QR against the stamp.
        fn validate(body: &Value) -> Result<String, String> {
            let xml = String::from_utf8(STANDARD.decode(body["invoice"].as_str().unwrap_or_default()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let v = khata_sa::xades::verify(&xml).map_err(|e| e.to_string())?;
            if Some(v.invoice_hash.as_str()) != body["invoiceHash"].as_str() {
                return Err("invalid-invoice-hash".into());
            }
            if body["uuid"].as_str().is_none_or(|u| !xml.contains(&format!("<cbc:UUID>{u}</cbc:UUID>"))) {
                return Err("uuid".into());
            }
            Ok(xml)
        }

        fn result(ok: &Result<String, String>, key: &str, yes: &str, no: &str, extra: Value) -> Reply {
            let mut r = match ok {
                Ok(_) => json!({ "validationResults": { "infoMessages": [{ "type": "INFO", "code": "XSD_ZATCA_VALID", "category": "XSD validation", "message": "Complied with UBL 2.1 standards", "status": "PASS" }], "warningMessages": [], "errorMessages": [], "status": "PASS" }, key: yes }),
                Err(e) => json!({ "validationResults": { "infoMessages": [], "warningMessages": [], "errorMessages": [{ "type": "ERROR", "code": e, "category": "MOCK", "message": e, "status": "ERROR" }], "status": "ERROR" }, key: no }),
            };
            if let (Some(o), Some(e)) = (r.as_object_mut(), extra.as_object()) {
                o.extend(e.clone());
            }
            (if ok.is_ok() { StatusCode::OK } else { StatusCode::BAD_REQUEST }, Json(r))
        }

        async fn compliance(State(c): State<Cfg>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
            c.calls.fetch_add(1, Ordering::SeqCst);
            if header(&h, "Accept-Version") != "V2" {
                return (StatusCode::NOT_ACCEPTABLE, Json(json!("This Version is not supported")));
            }
            if header(&h, "OTP") != "123345" {
                return (StatusCode::BAD_REQUEST, Json(json!({ "errors": [{ "code": "Invalid-OTP", "message": "The provided OTP is invalid" }] })));
            }
            let pem = STANDARD.decode(b["csr"].as_str().unwrap_or_default()).ok().and_then(|p| String::from_utf8(p).ok()).unwrap_or_default();
            if !pem.starts_with("-----BEGIN CERTIFICATE REQUEST-----") {
                return (StatusCode::BAD_REQUEST, Json(json!({ "errors": [{ "code": "Invalid-CSR", "message": "The provided CSR is invalid" }] })));
            }
            csid(&c.token, "mock-compliance-secret", json!(1234567890123u64))
        }

        async fn check(State(c): State<Cfg>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
            c.calls.fetch_add(1, Ordering::SeqCst);
            if header(&h, "Authorization") != basic(&c.token, "mock-compliance-secret") {
                return (StatusCode::UNAUTHORIZED, Json(json!({ "status": 401, "error": "Unauthorized" })));
            }
            let ok = validate(&b);
            let simplified = ok.as_ref().is_ok_and(|x| x.contains("name=\"02"));
            if simplified { result(&ok, "reportingStatus", "REPORTED", "NOT_REPORTED", json!({})) } else { result(&ok, "clearanceStatus", "CLEARED", "NOT_CLEARED", json!({})) }
        }

        async fn production(State(c): State<Cfg>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
            c.calls.fetch_add(1, Ordering::SeqCst);
            if header(&h, "Authorization") != basic(&c.token, "mock-compliance-secret") || b["compliance_request_id"] != json!("1234567890123") {
                return (StatusCode::UNAUTHORIZED, Json(json!({ "status": 401 })));
            }
            csid(&c.token, "mock-production-secret", json!(30368))
        }

        async fn report(State(c): State<Cfg>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
            c.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(r) = outage(&c) {
                return r;
            }
            if header(&h, "Authorization") != production_auth(&c) || header(&h, "Clearance-Status") != "0" {
                return (StatusCode::UNAUTHORIZED, Json(json!({ "status": 401 })));
            }
            let (code, mut j) = result(&validate(&b), "reportingStatus", "REPORTED", "NOT_REPORTED", json!({}));
            if code == StatusCode::OK {
                j.0["validationResults"]["warningMessages"] = json!([{ "type": "WARNING", "code": "BR-KSA-98", "category": "KSA", "message": "The simplified invoice should be submitted within 24 hours of issuing the invoice.", "status": "WARNING" }]);
                j.0["validationResults"]["status"] = json!("WARNING");
                return (StatusCode::ACCEPTED, j);
            }
            (code, j)
        }

        async fn clear(State(c): State<Cfg>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
            c.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(r) = outage(&c) {
                return r;
            }
            if header(&h, "Authorization") != production_auth(&c) || header(&h, "Clearance-Status") != "1" {
                return (StatusCode::UNAUTHORIZED, Json(json!({ "status": 401 })));
            }
            if c.clearance_off {
                return (StatusCode::SEE_OTHER, Json(json!({ "message": "Clearance is deactivated. Please use the /invoices/reporting/single endpoint instead." })));
            }
            let ok = validate(&b);
            let cleared = ok.as_ref().map(|x| STANDARD.encode(x)).unwrap_or_default();
            result(&ok, "clearanceStatus", "CLEARED", "NOT_CLEARED", json!({ "clearedInvoice": cleared }))
        }

        /// `PATCH /production/csids`: a renewal with a new OTP and CSR,
        /// authenticated with the current production CSID.
        async fn renew(State(c): State<Cfg>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
            c.calls.fetch_add(1, Ordering::SeqCst);
            if c.revoked.load(Ordering::SeqCst) || header(&h, "Authorization") != production_auth(&c) {
                return (StatusCode::UNAUTHORIZED, Json(json!({ "status": 401 })));
            }
            if header(&h, "OTP") != "123345" {
                return (StatusCode::BAD_REQUEST, Json(json!({ "errors": [{ "code": "Invalid-OTP", "message": "The provided OTP is invalid" }] })));
            }
            let pem = STANDARD.decode(b["csr"].as_str().unwrap_or_default()).ok().and_then(|p| String::from_utf8(p).ok()).unwrap_or_default();
            if !pem.starts_with("-----BEGIN CERTIFICATE REQUEST-----") {
                return (StatusCode::BAD_REQUEST, Json(json!({ "errors": [{ "code": "Invalid-CSR", "message": "The provided CSR is invalid" }] })));
            }
            *c.secret.lock().unwrap() = "mock-renewed-secret".into();
            csid(&c.token, "mock-renewed-secret", json!(30369))
        }

        pub struct Server {
            pub url: String,
            pub calls: Arc<AtomicUsize>,
            pub fail_next: Arc<AtomicUsize>,
            pub revoked: Arc<std::sync::atomic::AtomicBool>,
            stop: Option<tokio::sync::oneshot::Sender<()>>,
            thread: Option<std::thread::JoinHandle<()>>,
        }

        impl Drop for Server {
            fn drop(&mut self) {
                if let Some(s) = self.stop.take() {
                    let _ = s.send(());
                }
                if let Some(t) = self.thread.take() {
                    let _ = t.join();
                }
            }
        }

        pub fn start(token: &str, clearance_off: bool) -> Server {
            let calls = Arc::new(AtomicUsize::new(0));
            let fail_next = Arc::new(AtomicUsize::new(0));
            let revoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let cfg = Cfg {
                token: token.to_owned(),
                clearance_off,
                calls: calls.clone(),
                secret: Arc::new(std::sync::Mutex::new("mock-production-secret".into())),
                fail_next: fail_next.clone(),
                revoked: revoked.clone(),
            };
            let app = axum::Router::new()
                .route("/compliance", post(compliance))
                .route("/compliance/invoices", post(check))
                .route("/production/csids", post(production).patch(renew))
                .route("/invoices/reporting/single", post(report))
                .route("/invoices/clearance/single", post(clear))
                .with_state(cfg);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (tx, rx) = tokio::sync::oneshot::channel::<()>();
            let thread = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async move {
                    let l = tokio::net::TcpListener::from_std(listener).unwrap();
                    axum::serve(l, app).with_graceful_shutdown(async { let _ = rx.await; }).await.unwrap();
                });
            });
            Server { url, calls, fail_next, revoked, stop: Some(tx), thread: Some(thread) }
        }
    }

    fn data_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("khataos-zatca-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn onboarded(name: &str, clearance_off: bool) -> (TempFile, Mutex<Slot>, Books, PathBuf, mock::Server) {
        let (f, mut slot) = saudi_company(name);
        let b = setup(&mut slot);
        set_zatca_seller(&mut slot, &ZatcaSeller { crn: "1010010000".into(), building_number: "1234".into(), district: "Al Olaya".into(), ..ZatcaSeller::default() }).unwrap();
        let dir = data_dir(name);
        let server = mock::start(&base64::Engine::encode(&base64::engine::general_purpose::STANDARD, TEST_CERT.trim()), clearance_off);
        let slot = Mutex::new(slot);
        let client = Client::with_base(&server.url);
        onboard_with(&slot, &dir, Environment::Sandbox, "123345", &OnboardOptions::default(), &client, Some(TEST_KEY)).unwrap();
        (f, slot, b, dir, server)
    }

    #[test]
    fn onboarding_keeps_secrets_outside_the_books() {
        let (_f, mut slot) = saudi_company("zatca-onboard");
        setup(&mut slot);
        let dir = data_dir("onboard");
        let server = mock::start(&base64::Engine::encode(&base64::engine::general_purpose::STANDARD, TEST_CERT.trim()), false);
        let client = Client::with_base(&server.url);
        let slot = Mutex::new(slot);
        assert!(!zatca_status(&slot.lock().unwrap(), &dir).unwrap().csr_created);

        // A wrong OTP: the key and CSR exist, no CSID yet.
        let e = onboard_with(&slot, &dir, Environment::Sandbox, "999999", &OnboardOptions::default(), &client, Some(TEST_KEY)).unwrap_err();
        assert!(e.contains("Invalid-OTP"), "{e}");
        let s = zatca_status(&slot.lock().unwrap(), &dir).unwrap();
        assert!(s.csr_created && !s.compliance_csid && !s.onboarded, "{s:?}");
        assert!(s.last_error.as_deref().is_some_and(|e| e.contains("Invalid-OTP")));
        let csr = s.csr_pem.clone().unwrap();
        assert!(csr.starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
        assert!(s.common_name.as_deref().unwrap().starts_with("KhataOS-"));
        assert!(s.egs_serial.as_deref().unwrap().starts_with("1-KhataOS|2-"));

        // The right OTP: compliance CSID, six checks, production CSID.
        let s = onboard_with(&slot, &dir, Environment::Sandbox, "123345", &OnboardOptions::default(), &client, None).unwrap();
        assert!(s.onboarded && s.compliance_csid && s.production_csid, "{s:?}");
        assert_eq!(s.checks.len(), 6, "standard and simplified invoice, credit and debit notes");
        assert!(s.checks.iter().all(|c| c.status == "PASS"), "{:?}", s.checks);
        assert_eq!(s.environment.as_deref(), Some("sandbox"));
        assert!(s.warnings.iter().any(|w| w.contains("no legal effect")));
        assert_eq!((s.icv, s.pih.as_str()), (0, zatca::FIRST_PIH), "the device's chain is untouched by the checks");
        assert!(onboard_with(&slot, &dir, Environment::Sandbox, "123345", &OnboardOptions::default(), &client, None).unwrap_err().contains("already onboarded"));

        // The secrets file: user-only, holds the key and both CSIDs.
        let path = PathBuf::from(s.credentials_path.clone().unwrap());
        assert!(path.starts_with(&dir) && path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("BEGIN EC PRIVATE KEY") && text.contains("mock-production-secret"));
        // Nothing secret in the company file's settings.
        let g = slot.lock().unwrap();
        let store = store_ref(&g).unwrap();
        for (k, v) in store.settings().unwrap() {
            assert!(!v.contains("PRIVATE KEY") && !v.contains("mock-compliance-secret") && !v.contains("mock-production-secret"), "{k} leaks a secret");
        }
        // …nor in its shared records, which sync.
        for r in store.all_records().unwrap() {
            let v = r.value.unwrap_or_default();
            assert!(!v.contains("PRIVATE KEY") && !v.contains("mock-compliance-secret") && !v.contains("mock-production-secret"), "{} leaks a secret", r.key);
            assert!(!khata_store::records::is_device_local(&r.key));
        }
        let status_json = serde_json::to_string(&zatca_status(&g, &dir).unwrap()).unwrap();
        assert!(!status_json.contains("secret\":\"mock") && !status_json.contains("PRIVATE KEY"));
        // Another computer (no credentials file) must onboard again.
        let elsewhere = data_dir("elsewhere");
        let s = zatca_status(&g, &elsewhere).unwrap();
        assert!(!s.onboarded && !s.credentials_present);
        assert!(s.warnings.iter().any(|w| w.contains("not on this computer")));
        drop(g);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn submission_stamps_clears_and_reports() {
        let (_f, slot, b, dir, server) = onboarded("zatca-submit", false);
        let client = Client::with_base(&server.url);
        // Generated before submitting, unstamped (the plain export).
        let first = sale(&mut slot.lock().unwrap(), &b, "2026-05-01", "1000", "150", "1150");
        let plain = zatca_invoice_xml(&mut slot.lock().unwrap(), &first, "standard").unwrap();
        assert!(!plain.signed && !plain.xml.contains("ds:Signature"));

        let r = submit_with(&slot, &dir, &first, None, Some(&client)).unwrap();
        assert!(r.accepted && !r.already_submitted, "{r:?}");
        assert_eq!((r.kind.as_str(), r.endpoint.as_str(), r.clearance_status.as_deref()), ("standard", "clearance", Some("CLEARED")));
        assert!(r.cleared_xml.as_deref().is_some_and(|x| x.contains("<ds:SignatureValue>")));
        // Stamped in place: same counter, UUID and hash, so the chain holds.
        let stamped = zatca_invoice_xml(&mut slot.lock().unwrap(), &first, "standard").unwrap();
        assert!(stamped.signed && stamped.regenerated);
        assert_eq!((stamped.icv, stamped.uuid.as_str(), stamped.hash.as_str()), (plain.icv, plain.uuid.as_str(), plain.hash.as_str()));
        khata_sa::xades::verify(&stamped.xml).unwrap();
        let tags: Vec<u8> = khata_sa::qr::decode(&stamped.qr).unwrap().into_iter().map(|t| t.0).collect();
        assert_eq!(tags, [1, 2, 3, 4, 5, 6, 7, 8], "no tag 9 on a standard invoice");

        // Not sent twice.
        let calls = server.calls.load(std::sync::atomic::Ordering::SeqCst);
        let again = submit_with(&slot, &dir, &first, None, Some(&client)).unwrap();
        assert!(again.already_submitted && again.accepted);
        assert_eq!(server.calls.load(std::sync::atomic::Ordering::SeqCst), calls);
        assert_eq!(zatca_submission(&slot.lock().unwrap(), &first).unwrap().unwrap().clearance_status.as_deref(), Some("CLEARED"));

        // A cash sale with no VAT-registered buyer: simplified, reported, stamped at generation.
        let cash = ledger(&mut slot.lock().unwrap(), "Cash Customer", "Sundry Debtors", "");
        let second = {
            let mut g = slot.lock().unwrap();
            let mut line = entry(&b.sales, DrCr::Cr, "100");
            line.items = items(&b.laptop, "1", "100");
            post_voucher(&mut g, voucher("Sales", "2026-05-02", vec![entry(&cash, DrCr::Dr, "115"), line, entry(&b.output, DrCr::Cr, "15")])).unwrap().id
        };
        let x = zatca_invoice_xml_stamped(&mut slot.lock().unwrap(), &dir, &second, "simplified").unwrap();
        assert!(x.signed && x.icv == 2 && x.pih == stamped.hash);
        assert_eq!(khata_sa::qr::decode(&x.qr).unwrap().len(), 9, "tag 9 on a simplified invoice");
        let r = submit_with(&slot, &dir, &second, None, Some(&client)).unwrap();
        assert_eq!((r.endpoint.as_str(), r.reporting_status.as_deref(), r.http_status, r.accepted), ("reporting", Some("REPORTED"), 202, true));
        assert!(r.warnings.iter().any(|w| w.contains("BR-KSA-98")), "{r:?}");

        // A voucher never generated: the buyer's VAT number picks standard.
        let third = sale(&mut slot.lock().unwrap(), &b, "2026-05-03", "100", "15", "115");
        let r = submit_with(&slot, &dir, &third, None, Some(&client)).unwrap();
        assert_eq!((r.kind.as_str(), r.accepted), ("standard", true));

        // The print of a cleared invoice uses the cleared XML's QR.
        let html = crate::invoice::voucher_html(&slot.lock().unwrap(), &first).unwrap();
        assert!(html.contains("<svg") && html.contains("Tax Invoice"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearance_switched_off_reports_instead_and_refusals_are_kept() {
        let (_f, slot, b, dir, server) = onboarded("zatca-303", true);
        let client = Client::with_base(&server.url);
        let id = sale(&mut slot.lock().unwrap(), &b, "2026-05-01", "1000", "150", "1150");
        let r = submit_with(&slot, &dir, &id, Some("standard"), Some(&client)).unwrap();
        assert_eq!((r.endpoint.as_str(), r.reporting_status.as_deref(), r.accepted), ("reporting", Some("REPORTED"), true));
        assert!(r.notes.iter().any(|n| n.contains("303")), "{r:?}");

        // Without onboarding there is nothing to sign with.
        let (_g, mut other) = saudi_company("zatca-not-onboarded");
        let b2 = setup(&mut other);
        let v = sale(&mut other, &b2, "2026-05-01", "100", "15", "115");
        let other = Mutex::new(other);
        assert!(submit_with(&other, &data_dir("none"), &v, None, Some(&client)).unwrap_err().contains("onboard"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A simplified sale dated today (the queue lists e-invoices from the
    /// device's onboarding date on).
    fn cash_sale_today(slot: &Mutex<Slot>, b: &Books) -> String {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let mut g = slot.lock().unwrap();
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let cash = ledger(&mut g, &format!("Walk-in Customer {n}"), "Sundry Debtors", "");
        let mut line = entry(&b.sales, DrCr::Cr, "100");
        line.items = items(&b.laptop, "1", "100");
        let today = Utc::now().date_naive().to_string();
        post_voucher(&mut g, voucher("Sales", &today, vec![entry(&cash, DrCr::Dr, "115"), line, entry(&b.output, DrCr::Cr, "15")])).unwrap().id
    }

    #[test]
    fn the_queue_retries_with_backoff_until_zatca_accepts() {
        let (_f, slot, b, dir, server) = onboarded("zatca-queue", false);
        let client = Client::with_base(&server.url);
        let now = Utc::now();
        let first = cash_sale_today(&slot, &b);
        let x = zatca_invoice_xml_stamped(&mut slot.lock().unwrap(), &dir, &first, "simplified").unwrap();
        assert!(x.signed);
        let second = cash_sale_today(&slot, &b);
        let q = zatca_queue(&slot.lock().unwrap(), &dir).unwrap();
        assert_eq!((q.pending, q.not_generated, q.failed, q.overdue, q.ready), (1, 1, 0, 0, true), "{q:?}");
        let item = q.items.iter().find(|i| i.voucher_id == first).unwrap();
        assert_eq!((item.kind.as_str(), item.state.as_str()), ("simplified", "pending"));
        assert!(item.hours_left.is_some_and(|h| (23..=24).contains(&h)), "{item:?}");
        assert_eq!(zatca_status(&slot.lock().unwrap(), &dir).unwrap().queue_pending, 1);

        // ZATCA is down twice: retried after 1, then 2 minutes.
        server.fail_next.store(2, std::sync::atomic::Ordering::SeqCst);
        let r = queue_run_with(&slot, &dir, false, Some(&client), now).unwrap();
        assert_eq!((r.sent, r.retry_later, r.accepted), (0, 1, 0), "the automatic run leaves the ungenerated sale alone");
        let item = r.queue.items.iter().find(|i| i.voucher_id == first).unwrap().clone();
        assert_eq!((item.state.as_str(), item.attempts), ("retrying", 1));
        assert_eq!(item.next_attempt.as_deref(), Some(utc_text(now + chrono::Duration::minutes(1)).as_str()));
        assert!(item.last_error.as_deref().is_some_and(|e| e.contains("503")), "{item:?}");
        let calls = server.calls.load(std::sync::atomic::Ordering::SeqCst);
        let r = queue_run_with(&slot, &dir, false, Some(&client), now + chrono::Duration::seconds(30)).unwrap();
        assert_eq!((r.retry_later, r.accepted), (0, 0), "not due yet");
        assert_eq!(server.calls.load(std::sync::atomic::Ordering::SeqCst), calls, "nothing was sent");
        let r = queue_run_with(&slot, &dir, false, Some(&client), now + chrono::Duration::minutes(1)).unwrap();
        let item = r.queue.items.iter().find(|i| i.voucher_id == first).unwrap().clone();
        assert_eq!((r.retry_later, item.attempts), (1, 2));
        assert_eq!(item.next_attempt.as_deref(), Some(utc_text(now + chrono::Duration::minutes(3)).as_str()));
        let r = queue_run_with(&slot, &dir, false, Some(&client), now + chrono::Duration::minutes(3)).unwrap();
        assert_eq!((r.sent, r.accepted), (1, 1));
        assert!(r.queue.items.iter().all(|i| i.voucher_id != first), "accepted documents leave the queue");

        // Submit all generates, stamps and sends the rest.
        let r = queue_run_with(&slot, &dir, true, Some(&client), now + chrono::Duration::minutes(4)).unwrap();
        assert_eq!((r.sent, r.accepted, r.queue.items.len()), (1, 1, 0), "{r:?}");
        assert!(zatca_submission(&slot.lock().unwrap(), &second).unwrap().is_some_and(|s| s.accepted));
        assert_eq!((retry_delay(1), retry_delay(3), retry_delay(20)), (chrono::Duration::minutes(1), chrono::Duration::minutes(4), chrono::Duration::minutes(60)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_csid_stops_the_queue_until_it_is_renewed() {
        let (_f, slot, b, dir, server) = onboarded("zatca-renew", false);
        let client = Client::with_base(&server.url);
        let id = cash_sale_today(&slot, &b);
        zatca_invoice_xml_stamped(&mut slot.lock().unwrap(), &dir, &id, "simplified").unwrap();
        server.revoked.store(true, std::sync::atomic::Ordering::SeqCst);
        let r = queue_run_with(&slot, &dir, true, Some(&client), Utc::now()).unwrap();
        assert!(r.stopped.as_deref().is_some_and(|e| e.contains("401")), "{r:?}");
        assert!(!r.queue.ready && r.queue.pending == 1, "the document waits: {:?}", r.queue);
        let s = zatca_status(&slot.lock().unwrap(), &dir).unwrap();
        assert!(s.csid_refused.is_some() && s.next_step.contains("Renew"), "{s:?}");
        assert!(s.warnings.iter().any(|w| w.contains("revoked")), "{:?}", s.warnings);

        // Revoked: renewal is refused too; once ZATCA takes the CSID again it renews.
        let opts = OnboardOptions::default();
        assert!(renew_with(&slot, &dir, "123345", &opts, Some(&client), Some(TEST_KEY)).unwrap_err().contains("401"));
        server.revoked.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(renew_with(&slot, &dir, "", &opts, Some(&client), Some(TEST_KEY)).unwrap_err().contains("OTP"));
        assert!(renew_with(&slot, &dir, "999999", &opts, Some(&client), Some(TEST_KEY)).unwrap_err().contains("Invalid-OTP"));
        let s = renew_with(&slot, &dir, "123345", &opts, Some(&client), Some(TEST_KEY)).unwrap();
        assert!(s.onboarded && s.csid_refused.is_none() && s.renewed_at.is_some(), "{s:?}");
        assert_eq!(s.icv, 1, "the chain carries on");
        let text = std::fs::read_to_string(s.credentials_path.as_deref().unwrap()).unwrap();
        assert!(text.contains("mock-renewed-secret") && !text.contains("pending_key_pem\":\"-----"), "the new CSID, no pending key");
        // The mock now takes only the renewed secret.
        let r = queue_run_with(&slot, &dir, false, Some(&client), Utc::now()).unwrap();
        assert_eq!((r.accepted, r.queue.pending), (1, 0), "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expiry_is_read_from_the_certificate_and_warned_30_days_ahead() {
        let (_f, slot, b, dir, server) = onboarded("zatca-expiry", false);
        let client = Client::with_base(&server.url);
        let g = slot.lock().unwrap();
        let s = zatca_status(&g, &dir).unwrap();
        let expiry = chrono::NaiveDate::parse_from_str(s.certificate_expiry.as_deref().unwrap(), "%Y-%m-%d").unwrap();
        let at = |days: i64| (expiry - chrono::Duration::days(days)).and_hms_opt(0, 0, 0).unwrap().and_utc();
        let early = status_at(store_ref(&g).unwrap(), &dir, at(60)).unwrap();
        assert!(!early.renewal_due && !early.expired && early.days_to_expiry.is_some_and(|d| d >= 59));
        let soon = status_at(store_ref(&g).unwrap(), &dir, at(10)).unwrap();
        assert!(soon.renewal_due && !soon.expired && soon.next_step.contains("expires soon"), "{soon:?}");
        assert!(soon.warnings.iter().any(|w| w.contains("renew it now")), "{:?}", soon.warnings);
        let late = status_at(store_ref(&g).unwrap(), &dir, at(-2)).unwrap();
        assert!(late.expired && late.days_to_expiry.is_some_and(|d| d < 0), "{late:?}");
        drop(g);
        // An expired certificate is not sent: the document stays queued.
        let id = cash_sale_today(&slot, &b);
        let e = submit_inner(&slot, &dir, &id, Some("simplified"), Some(&client), at(-2)).unwrap_err();
        assert_eq!(e.kind, crate::zatca_api::CallFailure::Credentials);
        assert!(e.message.contains("expired"), "{e:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against ZATCA's real developer portal (network): onboarding with the
    /// test OTP, the six compliance checks, the production CSID, then a
    /// standard invoice cleared and a simplified one reported. Run with
    /// `cargo test -p khataos-desktop live_sandbox -- --ignored`.
    #[test]
    #[ignore = "calls ZATCA's sandbox over the internet"]
    fn live_sandbox_onboarding_and_submission() {
        let (_f, mut slot) = saudi_company("zatca-live");
        let b = setup(&mut slot);
        crate::tax::set_company_tax(&mut slot, "399999999900003", "SA-01").unwrap();
        set_zatca_seller(&mut slot, &ZatcaSeller { crn: "1010010000".into(), building_number: "1234".into(), district: "Al Olaya".into(), ..ZatcaSeller::default() }).unwrap();
        let dir = data_dir("live");
        let slot = Mutex::new(slot);
        let s = zatca_onboard(&slot, &dir, "sandbox", "123345", &OnboardOptions::default()).unwrap();
        assert!(s.onboarded, "{s:?}");
        assert!(s.checks.iter().all(|c| c.status == "PASS"), "{:?}", s.checks);
        let std = sale(&mut slot.lock().unwrap(), &b, "2026-10-20", "1000", "150", "1150");
        let r = zatca_submit(&slot, &dir, &std, Some("standard")).unwrap();
        assert!(r.accepted && r.clearance_status.as_deref() == Some("CLEARED"), "{r:?}");
        let cash = ledger(&mut slot.lock().unwrap(), "Walk-in Customer", "Sundry Debtors", "");
        let simple = {
            let mut g = slot.lock().unwrap();
            let mut line = entry(&b.sales, DrCr::Cr, "100");
            line.items = items(&b.laptop, "1", "100");
            post_voucher(&mut g, voucher("Sales", "2026-10-20", vec![entry(&cash, DrCr::Dr, "115"), line, entry(&b.output, DrCr::Cr, "15")])).unwrap().id
        };
        let r = zatca_submit(&slot, &dir, &simple, Some("simplified")).unwrap();
        assert!(r.accepted && r.reporting_status.as_deref() == Some("REPORTED"), "{r:?}");
        eprintln!("live sandbox: {} checks passed; clearance and reporting accepted ({:?})", s.checks.len(), r.warnings);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

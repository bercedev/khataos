//! Mexico: the monthly IVA summary and CFDI 4.0 XML from the books. The
//! rules live in `khata_mx`; this module turns vouchers into its documents
//! and keeps the issuer's settings and CSD.
//!
//! **Vouchers.** Sales → CFDI de ingreso (`I`); Credit Note → CFDI de
//! egreso (`E`, related to the invoice's UUID when given). For the IVA
//! summary, Purchase and Debit Note (purchase return) are the inward side.
//! Lines split as in `zatca.rs`: items and service ledgers are conceptos;
//! ledgers whose type of tax is `IVA` give the booked IVA; ledgers on the
//! party's side are left out with a warning; "round off" ledgers are left
//! out. A line's IVA comes from its tax class and the rate on file
//! (`STANDARD` 16%, `BORDER` 8%, `ZERO`, `EXEMPT`); a line without a class
//! is taken at 16% with a warning.
//!
//! **Issuer settings** (régimen fiscal, lugar de expedición, serie) are not
//! secret and are a shared record of the company (`mx.emisor`, synced).
//!
//! **The CSD stays out of the books.** The `.cer` and the encrypted `.key`
//! are copied into the app data directory (`mx-csd/<RFC>.cer|.key`, files
//! readable only by this user, directory 0700) when the user picks them;
//! they are never written to the company file, synced, or sent anywhere.
//! The key's password is **never stored**: it is asked for each time a
//! CFDI is sealed, used in memory and dropped. Without a CSD (or a
//! password) the XML is an unsealed draft.
//!
//! **Withholdings.** What a customer withholds comes from the customer
//! ledger's tax class (`RET:ISR10+IVA2/3`, `RET:RESICO`, `RET:IVA4`… see
//! `khata_mx::retenciones`), or from the per-CFDI `retenciones` option.
//! In the books, withheld amounts sit on duty ledgers whose type of tax is
//! `RET-IVA` or `RET-ISR` (on the party's side of the voucher: a debit on
//! a sale, "ISR/IVA retenido por clientes"; a credit on a purchase, "ISR/IVA
//! retenido por enterar"). The IVA summary uses those booked amounts.
//!
//! **Payment receipts.** A Receipt voucher whose party line settles PPD
//! invoices bill by bill (Against references) becomes a CFDI de tipo `P`
//! with the Complemento de Pagos 2.0 (`pago_xml`): one DoctoRelacionado per
//! Against reference, whose IdDocumento is the UUID of the invoice as
//! stamped through the PAC connector (`pac_client.rs`), or one the user
//! gives. ImpSaldoAnt and NumParcialidad come from the earlier Against
//! allocations to the same bill.
//!
//! **Timbrado** goes through `pac_client.rs`, which calls
//! [`prepare_for_pac`] for the sealed XML (see `docs/mexico-cfdi.md`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime, NaiveTime, Timelike};
use khata_compliance::RateSchedule;
use khata_core::{Amount, BillKind, Book, DrCr, Ledger, LedgerId, Voucher, VoucherId, VoucherKind};
use khata_mx::cfdi::{self, Comprobante, Concepto, Csd, Emisor, Impuesto, Iva, MetodoPago, Receptor, TipoDeComprobante};
use khata_mx::iva::{self, Treatment};
use khata_mx::pagos::{self, DocTaxes, DoctoRelacionado, Pago, ReciboPago};
use khata_mx::retenciones::{self, Rule};
use khata_mx::returns::{self, DocKind, DocLine, InwardDoc, IvaSummary, OutwardDoc, Period, Retenido};
use khata_mx::rfc;
use khata_store::Store;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, group_is_party, open_mut, open_ref, parse_date, parse_uuid};

const EMISOR_KEY: &str = "mx.emisor";
const CSD_DIR: &str = "mx-csd";

pub(crate) fn mx_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_mx::COUNTRY.code) {
        return Err("CFDI and the IVA summary are for companies in Mexico".into());
    }
    Ok(())
}

pub(crate) fn store_ref(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

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

fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = match v.kind {
        VoucherKind::Sales | VoucherKind::DebitNote | VoucherKind::Payment => DrCr::Dr,
        _ => DrCr::Cr,
    };
    let is_party = |e: &khata_core::Entry| book.ledger(e.ledger).is_some_and(|l| group_is_party(book, l.group));
    v.entries
        .iter()
        .position(is_party)
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

/// One voucher line before tax.
struct RawLine {
    name: String,
    qty: Decimal,
    rate: Decimal,
    /// Signed: negative when on the party's side.
    amount: Amount,
    /// (treatment, percent) from the class and the rate on file.
    tax: Option<(Treatment, Decimal)>,
    goods: bool,
}

struct Parsed {
    party: Option<usize>,
    lines: Vec<RawLine>,
    iva: Amount,
    /// Withholdings booked on `RET-IVA` / `RET-ISR` ledgers, positive when
    /// on the party's side.
    retenido: Retenido,
    warnings: Vec<String>,
}

/// The type of tax of a withholding ledger: `RET-IVA` / `RET-ISR` (also
/// `RETIVA`, `IVA-RET`, `IVA RETENIDO`…).
pub fn withholding_component(component: &str) -> Option<Impuesto> {
    let c: String = component.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    match c.as_str() {
        "RETIVA" | "IVARET" | "IVARETENIDO" => Some(Impuesto::Iva),
        "RETISR" | "ISRRET" | "ISRRETENIDO" => Some(Impuesto::Isr),
        _ => None,
    }
}

fn parse(book: &Book, v: &Voucher, rates: &RateSchedule) -> Parsed {
    let name = format!("{} {}", label(v.kind), v.number);
    let party = party_line(book, v);
    let party_side = party.map(|i| v.entries[i].side);
    let mut p = Parsed { party, lines: Vec::new(), iva: Amount::ZERO, retenido: Retenido::default(), warnings: Vec::new() };
    let known = |class: Option<&str>| class.and_then(|c| rates.rate(c, v.date).map(|r| (iva::treatment(c, r), r)));
    let mut unclassed = false;
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if t.component.eq_ignore_ascii_case(iva::IVA) {
                p.iva += signed;
            } else if let Some(imp) = withholding_component(&t.component) {
                let withheld = if on_party_side { e.amount } else { -e.amount };
                match imp {
                    Impuesto::Iva => p.retenido.iva += withheld,
                    Impuesto::Isr => p.retenido.isr += withheld,
                }
            } else {
                p.warnings.push(format!("{name}: {ledger_name} collects {}, which is not IVA; it is left out", t.component));
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                let tax = known(item.and_then(|i| i.tax_class.as_deref()));
                unclassed |= tax.is_none();
                p.lines.push(RawLine {
                    name: item.map(|i| i.name.clone()).unwrap_or_default(),
                    qty: l.qty,
                    rate: l.rate,
                    amount: if on_party_side { -l.amount } else { l.amount },
                    tax,
                    goods: true,
                });
            }
        } else if ledger_name.to_lowercase().contains("round") {
            p.warnings.push(format!("{name}: {ledger_name} (rounding) is left out"));
        } else if on_party_side {
            p.warnings.push(format!("{name}: {ledger_name} is on the party's side; discounts are not put on the CFDI or the summary"));
        } else {
            let tax = known(ledger.and_then(|l| l.tax_class.as_deref()));
            unclassed |= tax.is_none();
            p.lines.push(RawLine { name: ledger_name, qty: Decimal::ONE, rate: e.amount, amount: e.amount, tax, goods: false });
        }
    }
    if unclassed {
        p.warnings.push(format!("{name}: lines without a tax class or rate on file are taken at 16% IVA"));
    }
    p
}

fn line_tax(l: &RawLine) -> (Treatment, Decimal) {
    l.tax.unwrap_or((Treatment::Taxed, iva::STANDARD_PERCENT))
}

/// Doc lines with the booked IVA spread over the taxed lines by value.
fn doc_lines(p: &Parsed, name: &str, warnings: &mut Vec<String>) -> Vec<DocLine> {
    let mut out: Vec<DocLine> = p
        .lines
        .iter()
        .map(|l| {
            let (treatment, percent) = line_tax(l);
            DocLine { treatment, percent: if treatment == Treatment::Taxed { percent } else { Decimal::ZERO }, value: l.amount, iva: Decimal::ZERO }
        })
        .collect();
    let taxed: Vec<usize> = (0..out.len()).filter(|&i| out[i].treatment == Treatment::Taxed).collect();
    let weight: Decimal = taxed.iter().map(|&i| out[i].value * out[i].percent).sum();
    if p.iva.is_zero() {
        return out;
    }
    if taxed.is_empty() || weight.is_zero() {
        warnings.push(format!("{name} has IVA {} but no taxed lines; it is left out", p.iva.normalize()));
        return out;
    }
    let mut left = p.iva;
    for (n, &i) in taxed.iter().enumerate() {
        let share = if n + 1 == taxed.len() { left } else { (p.iva * out[i].value * out[i].percent / weight).round_dp(2) };
        left -= share;
        out[i].iva = share;
    }
    out
}

/// The withholding rules for a party: the override when given (`RET:…`,
/// the prefix optional), else the party ledger's `RET:` class.
fn party_rules(party: &Ledger, override_class: &str) -> ApiResult<Vec<Rule>> {
    let o = override_class.trim();
    if !o.is_empty() {
        let class = if o.to_ascii_uppercase().starts_with(retenciones::PREFIX) { o.to_owned() } else { format!("{}{o}", retenciones::PREFIX) };
        return Ok(retenciones::parse_class(&class)?.unwrap_or_default());
    }
    match party.tax_class.as_deref().map(retenciones::parse_class) {
        Some(Ok(Some(rules))) => Ok(rules),
        Some(Err(e)) => Err(format!("{}: tax class: {e}", party.name)),
        _ => Ok(Vec::new()),
    }
}

/// Withholdings worked out from a class over a document's lines.
fn class_retenido(rules: &[Rule], lines: &[DocLine]) -> Retenido {
    let mut r = Retenido::default();
    for l in lines {
        let iva = match l.treatment {
            Treatment::Exempt => Some(Iva::Exento),
            _ => Some(Iva::Tasa(l.percent)),
        };
        for ret in retenciones::for_concepto(rules, iva) {
            let amount = cfdi::money(cfdi::money(l.value) * ret.tasa);
            match ret.impuesto {
                Impuesto::Iva => r.iva += amount,
                Impuesto::Isr => r.isr += amount,
            }
        }
    }
    r
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IvaSummaryMxDto {
    pub rfc: Option<String>,
    #[serde(flatten)]
    pub summary: IvaSummary,
}

/// The IVA summary for a period (normally a month).
pub fn iva_summary_mx(slot: &Slot, from: &str, to: &str) -> ApiResult<IvaSummaryMxDto> {
    let book = open_ref(slot)?;
    mx_only(book)?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let rates = rates(book)?;
    let (mut outward, mut inward, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| period.contains(v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let out = match v.kind {
            VoucherKind::Sales | VoucherKind::CreditNote => true,
            VoucherKind::Purchase | VoucherKind::DebitNote => false,
            _ => continue,
        };
        let name = format!("{} {}", label(v.kind), v.number);
        let mut p = parse(book, v, &rates);
        warnings.append(&mut p.warnings);
        let lines = doc_lines(&p, &name, &mut warnings);
        let ledger = p.party.and_then(|i| book.ledger(v.entries[i].ledger));
        let party_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        let kind = if matches!(v.kind, VoucherKind::Sales | VoucherKind::Purchase) { DocKind::Invoice } else { DocKind::CreditNote };
        let id = v.id.0.to_string();
        let mut retenido = Retenido { iva: p.retenido.iva.abs(), isr: p.retenido.isr.abs() };
        if retenido == Retenido::default() && !v.cancelled {
            // Nothing booked: the party's RET: class, if any, says what was withheld.
            if let Some(rules) = ledger.and_then(|l| party_rules(l, "").ok()).filter(|r| !r.is_empty()) {
                retenido = class_retenido(&rules, &lines);
                if retenido != Retenido::default() {
                    warnings.push(format!(
                        "{name}: no withholding is booked; {} (from the party's class {}) is assumed",
                        retenciones::describe(&rules),
                        ledger.and_then(|l| l.tax_class.as_deref()).unwrap_or_default()
                    ));
                }
            }
        }
        if out {
            outward.push(OutwardDoc { id, kind, number: v.number.clone(), date: v.date, party_name, cancelled: v.cancelled, lines, retenido });
        } else {
            inward.push(InwardDoc {
                id,
                kind,
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_rfc: ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty()),
                cancelled: v.cancelled,
                lines,
                retenido,
            });
        }
    }
    let mut summary = returns::iva_summary(&outward, &inward, period);
    summary.warnings.splice(0..0, warnings);
    Ok(IvaSummaryMxDto { rfc: book.profile().and_then(|p| p.tax_id.clone()), summary })
}

// ------------------------------------------------------------ issuer

/// The issuer details a CFDI needs beyond the company profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MxIssuer {
    /// c_RegimenFiscal ("601" General de Ley Personas Morales).
    pub regimen_fiscal: String,
    /// The issuing place's postal code (LugarExpedicion).
    pub lugar_expedicion: String,
    /// Serie, optional.
    pub serie: String,
    /// The name exactly as on the Constancia de Situación Fiscal; blank:
    /// the company name.
    pub nombre: String,
}

fn read_issuer(store: &Store) -> ApiResult<MxIssuer> {
    match store.record(EMISOR_KEY).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("setting {EMISOR_KEY} is damaged: {e}")),
        None => Ok(MxIssuer::default()),
    }
}

pub fn mx_issuer(slot: &Slot) -> ApiResult<MxIssuer> {
    mx_only(open_ref(slot)?)?;
    read_issuer(store_ref(slot)?)
}

fn three_digits(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit())
}

fn postal(s: &str) -> bool {
    s.len() == 5 && s.bytes().all(|b| b.is_ascii_digit())
}

pub fn set_mx_issuer(slot: &mut Slot, issuer: &MxIssuer) -> ApiResult<MxIssuer> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, khata_core::UserRole::Owner)?;
    mx_only(book)?;
    let clean = MxIssuer {
        regimen_fiscal: issuer.regimen_fiscal.trim().to_owned(),
        lugar_expedicion: issuer.lugar_expedicion.trim().to_owned(),
        serie: issuer.serie.trim().to_owned(),
        nombre: issuer.nombre.trim().to_owned(),
    };
    if !clean.regimen_fiscal.is_empty() && !three_digits(&clean.regimen_fiscal) {
        return Err("the régimen fiscal is a 3-digit c_RegimenFiscal code (601, 612, 626…)".into());
    }
    if !clean.lugar_expedicion.is_empty() && !postal(&clean.lugar_expedicion) {
        return Err("the lugar de expedición is a 5-digit postal code".into());
    }
    if clean.serie.len() > 25 || !clean.serie.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("the serie has at most 25 letters or digits".into());
    }
    let json = serde_json::to_string(&clean).map_err(err)?;
    store.set_record(EMISOR_KEY, Some(&json), &crate::api::os_user()).map_err(err)?;
    Ok(clean)
}

// ------------------------------------------------------------ CSD

/// Which CSD is installed for the company's RFC (never the key itself).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CsdStatus {
    pub rfc: String,
    pub installed: bool,
    pub no_certificado: Option<String>,
}

pub(crate) fn company_rfc(book: &Book) -> ApiResult<String> {
    book.profile()
        .and_then(|p| p.tax_id.clone())
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "set the company's RFC first (Company → Tax registration)".to_owned())
}

/// `<dir>/mx-csd/<RFC>.cer|.key`; the RFC's letters and digits only.
pub(crate) fn csd_paths(dir: &Path, rfc: &str) -> (PathBuf, PathBuf) {
    let safe: String = rfc.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let base = dir.join(CSD_DIR);
    (base.join(format!("{safe}.cer")), base.join(format!("{safe}.key")))
}

fn write_private(path: &Path, bytes: &[u8]) -> ApiResult<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(path).and_then(|mut f| f.write_all(bytes)).map_err(|e| format!("cannot save {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(err)?;
    }
    Ok(())
}

fn load_csd(dir: &Path, rfc: &str, password: &str) -> ApiResult<Option<Csd>> {
    let (cer, key) = csd_paths(dir, rfc);
    if !cer.exists() || !key.exists() {
        return Ok(None);
    }
    let cer = std::fs::read(&cer).map_err(err)?;
    let key = std::fs::read(&key).map_err(err)?;
    Csd::from_der(&cer, &key, password).map(Some)
}

pub fn mx_csd_status(dir: &Path, slot: &Slot) -> ApiResult<CsdStatus> {
    let book = open_ref(slot)?;
    mx_only(book)?;
    let rfc = company_rfc(book)?;
    let (cer, key) = csd_paths(dir, &rfc);
    let installed = cer.exists() && key.exists();
    let no_certificado = if installed {
        // The certificate number needs only the certificate.
        std::fs::read(&cer).ok().and_then(|der| cfdi::certificate_number(&der).ok())
    } else {
        None
    };
    Ok(CsdStatus { rfc, installed, no_certificado })
}

/// Install the company's CSD from the SAT's `.cer` and `.key` files. The
/// password only proves the key opens and matches the certificate; it is
/// not kept. Owner only.
pub fn set_mx_csd(dir: &Path, slot: &Slot, cer_path: &str, key_path: &str, password: &str) -> ApiResult<CsdStatus> {
    let book = open_ref(slot)?;
    crate::api::require(book, khata_core::UserRole::Owner)?;
    mx_only(book)?;
    let rfc = company_rfc(book)?;
    let cer = std::fs::read(cer_path).map_err(|e| format!("cannot read {cer_path}: {e}"))?;
    let key = std::fs::read(key_path).map_err(|e| format!("cannot read {key_path}: {e}"))?;
    let csd = Csd::from_der(&cer, &key, password)?;
    let (cer_to, key_to) = csd_paths(dir, &rfc);
    let base = cer_to.parent().expect("a directory");
    std::fs::create_dir_all(base).map_err(err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(base, std::fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    write_private(&cer_to, &cer)?;
    write_private(&key_to, &key)?;
    Ok(CsdStatus { rfc, installed: true, no_certificado: Some(csd.no_certificado.clone()) })
}

/// Remove the installed CSD files. Owner only.
pub fn clear_mx_csd(dir: &Path, slot: &Slot) -> ApiResult<CsdStatus> {
    let book = open_ref(slot)?;
    crate::api::require(book, khata_core::UserRole::Owner)?;
    mx_only(book)?;
    let rfc = company_rfc(book)?;
    let (cer, key) = csd_paths(dir, &rfc);
    for p in [cer, key] {
        if p.exists() {
            std::fs::remove_file(&p).map_err(err)?;
        }
    }
    mx_csd_status(dir, slot)
}

// ------------------------------------------------------------ CFDI

/// What the user picks for one CFDI (an invoice, a credit note or a
/// payment receipt).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CfdiOptions {
    /// c_FormaPago ("03"); "99" with PPD. For a payment receipt: how the
    /// money came in (FormaDePagoP, never 99).
    pub forma_pago: String,
    /// "PUE" or "PPD".
    pub metodo_pago: String,
    /// c_UsoCFDI ("G03"); a payment receipt is always CP01.
    pub uso_cfdi: String,
    /// The receptor's c_RegimenFiscal ("601").
    pub regimen_fiscal_receptor: String,
    /// For a credit note: the UUID of the invoice it corrects. For a
    /// payment receipt: the UUID of an earlier receipt it replaces
    /// (TipoRelacion 04). For an invoice: the UUID of the CFDI it replaces
    /// after a cancellation with motivo 01 (TipoRelacion 04).
    pub related_uuid: String,
    /// The CSD key's password; blank: an unsealed draft.
    pub password: String,
    /// Withholdings for this CFDI instead of the customer's class
    /// (`RET:ISR10+IVA2/3`, `RET:NONE`); blank: the class.
    pub retenciones: String,
    /// Payment receipts: the payment's reference (cheque, SPEI key).
    pub num_operacion: String,
    /// Payment receipts: UUIDs of invoices stamped outside KhataOS, by
    /// bill reference.
    pub uuids: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CfdiDto {
    pub xml: String,
    pub cadena_original: String,
    /// Sealed with the CSD (otherwise a draft without Sello).
    pub sealed: bool,
    pub no_certificado: String,
    pub total: String,
    pub warnings: Vec<String>,
}

fn generic_rfc(r: &str) -> bool {
    r == rfc::PUBLICO_EN_GENERAL || r == rfc::EXTRANJERO
}

/// The receptor from the party ledger and the options.
fn receptor_for(party: Option<&Ledger>, issuer: &MxIssuer, opts: &CfdiOptions, warnings: &mut Vec<String>) -> ApiResult<(Receptor, bool)> {
    let abroad = party
        .and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .is_some_and(|c| !c.trim().is_empty() && !c.trim().eq_ignore_ascii_case("MX"));
    let party_rfc = party.and_then(|l| l.tax_id.as_deref()).map(rfc::normalise).filter(|r| !r.is_empty());
    let receptor_rfc = match (party_rfc, abroad) {
        (Some(r), _) => r,
        (None, true) => rfc::EXTRANJERO.to_owned(),
        (None, false) => {
            warnings.push("the customer has no RFC: the CFDI is to público en general (XAXX010101000)".into());
            rfc::PUBLICO_EN_GENERAL.to_owned()
        }
    };
    let generic = generic_rfc(&receptor_rfc);
    let receptor = Receptor {
        rfc: receptor_rfc.clone(),
        nombre: if generic && !abroad { "PUBLICO EN GENERAL".into() } else { party.map(|l| l.name.clone()).unwrap_or_default() },
        domicilio_fiscal: if generic {
            issuer.lugar_expedicion.clone()
        } else {
            party.and_then(|l| l.mailing.as_ref()).map(|m| m.postal_code.trim().to_owned()).unwrap_or_default()
        },
        regimen_fiscal: if generic { "616".into() } else { opts.regimen_fiscal_receptor.trim().to_owned() },
        uso_cfdi: if generic { "S01".into() } else { opts.uso_cfdi.trim().to_owned() },
    };
    Ok((receptor, abroad))
}

fn issuer_ready(slot: &Slot) -> ApiResult<(String, MxIssuer)> {
    let rfc_emisor = company_rfc(open_ref(slot)?)?;
    let issuer = read_issuer(store_ref(slot)?)?;
    if issuer.regimen_fiscal.is_empty() || issuer.lugar_expedicion.is_empty() {
        return Err("set the issuer's régimen fiscal and lugar de expedición first".into());
    }
    Ok((rfc_emisor, issuer))
}

fn emisor(book: &Book, rfc_emisor: &str, issuer: &MxIssuer) -> Emisor {
    Emisor {
        rfc: rfc_emisor.to_owned(),
        nombre: if issuer.nombre.is_empty() { book.name.clone() } else { issuer.nombre.clone() },
        regimen_fiscal: issuer.regimen_fiscal.clone(),
    }
}

/// An invoice or credit note's comprobante from the books, with the
/// warnings and the IVA and withholdings booked.
struct Built {
    comprobante: Comprobante,
    warnings: Vec<String>,
    booked_iva: Amount,
    booked_retenido: Retenido,
}

fn build_comprobante(book: &Book, rfc_emisor: &str, issuer: &MxIssuer, v: &Voucher, opts: &CfdiOptions) -> ApiResult<Built> {
    let tipo = match v.kind {
        VoucherKind::Sales => TipoDeComprobante::Ingreso,
        VoucherKind::CreditNote => TipoDeComprobante::Egreso,
        _ => return Err("a CFDI is made from a Sales or Credit Note voucher".into()),
    };
    if v.cancelled {
        return Err("this voucher is cancelled".into());
    }
    // Moneda and TipoCambio aren't written yet (docs/mexico-cfdi.md).
    crate::forex::refuse_foreign_einvoice(v, "A CFDI")?;
    let rates = rates(book)?;
    let mut p = parse(book, v, &rates);
    let mut warnings = std::mem::take(&mut p.warnings);
    let party = p.party.and_then(|i| book.ledger(v.entries[i].ledger));
    let (receptor, abroad) = receptor_for(party, issuer, opts, &mut warnings)?;
    if abroad {
        warnings.push("a customer abroad: Exportacion is 01 (no aplica); exports of goods need Exportacion 02 and the Comercio Exterior complement, which KhataOS does not make".into());
    }
    if !generic_rfc(&receptor.rfc) && (receptor.regimen_fiscal.is_empty() || receptor.uso_cfdi.is_empty()) {
        return Err("give the customer's régimen fiscal and the uso del CFDI".into());
    }
    let metodo = match opts.metodo_pago.trim().to_ascii_uppercase().as_str() {
        "" | "PUE" => MetodoPago::Pue,
        "PPD" => MetodoPago::Ppd,
        other => return Err(format!("{other:?} is not a método de pago (PUE or PPD)")),
    };
    let forma = match (opts.forma_pago.trim(), metodo) {
        ("", MetodoPago::Ppd) => "99".to_owned(),
        ("", MetodoPago::Pue) => return Err("give the forma de pago (01 efectivo, 03 transferencia…)".into()),
        (f, _) => f.to_owned(),
    };
    if p.lines.is_empty() {
        return Err("this voucher has no lines to put on a CFDI".into());
    }
    let rules = match party {
        Some(l) => party_rules(l, &opts.retenciones)?,
        None => Vec::new(),
    };
    if !rules.is_empty() && generic_rfc(&receptor.rfc) {
        return Err("withholdings need a customer with an RFC (not público en general)".into());
    }
    let conceptos: Vec<Concepto> = p
        .lines
        .iter()
        .map(|l| {
            let (treatment, percent) = line_tax(l);
            let amount = l.amount.abs();
            let iva = match treatment {
                Treatment::Exempt => Iva::Exento,
                _ => Iva::Tasa(percent),
            };
            Concepto {
                clave_prod_serv: "01010101".into(),
                no_identificacion: None,
                cantidad: if l.qty.is_zero() { Decimal::ONE } else { l.qty.abs() },
                clave_unidad: if l.goods { "H87".into() } else { "E48".into() },
                unidad: None,
                descripcion: l.name.clone(),
                valor_unitario: if l.qty.is_zero() { amount } else { l.rate.abs() },
                importe: amount,
                descuento: None,
                iva: Some(iva),
                retenciones: retenciones::for_concepto(&rules, Some(iva)),
            }
        })
        .collect();
    warnings.push("ClaveProdServ is 01010101 (no existe en el catálogo) on every concepto: KhataOS items carry no SAT product key yet".into());
    let fecha = NaiveDateTime::new(v.date, Local::now().time().with_nanosecond(0).unwrap_or_default());
    let related = opts.related_uuid.trim().to_uppercase();
    let relacionados = match (tipo, related.is_empty()) {
        (_, true) => None,
        (TipoDeComprobante::Egreso, false) => Some(("01".to_owned(), vec![related])),
        (_, false) => Some(("04".to_owned(), vec![related])),
    };
    let comprobante = Comprobante {
        serie: Some(issuer.serie.clone()).filter(|s| !s.is_empty()),
        folio: v.number.chars().filter(|c| c.is_ascii_alphanumeric()).collect(),
        fecha,
        forma_pago: forma,
        metodo_pago: metodo,
        moneda: "MXN".into(),
        tipo_cambio: None,
        tipo,
        exportacion: "01".into(),
        lugar_expedicion: issuer.lugar_expedicion.clone(),
        relacionados,
        emisor: emisor(book, rfc_emisor, issuer),
        receptor,
        conceptos,
    };
    if tipo == TipoDeComprobante::Egreso && comprobante.relacionados.is_none() {
        warnings.push("a credit note should relate to the invoice's UUID (TipoRelacion 01)".into());
    }
    Ok(Built { comprobante, warnings, booked_iva: p.iva, booked_retenido: p.retenido })
}

fn csd_for(dir: &Path, rfc_emisor: &str, password: &str, warnings: &mut Vec<String>) -> ApiResult<Option<Csd>> {
    if password.is_empty() {
        warnings.push("no CSD password given: this is an unsealed draft".into());
        return Ok(None);
    }
    match load_csd(dir, rfc_emisor, password)? {
        Some(c) => Ok(Some(c)),
        None => Err("no CSD is installed for this RFC".into()),
    }
}

fn check_booked(built: &Built, g: &cfdi::Generated, warnings: &mut Vec<String>) {
    let tolerance = Decimal::new(1, 2) * Decimal::from(built.comprobante.conceptos.len().max(1));
    let booked = built.booked_iva.abs();
    let computed = g.totales.total_traslados.unwrap_or_default();
    if (booked - computed).abs() > tolerance {
        warnings.push(format!("IVA booked is {}, but the CFDI's traslados total {}", booked.normalize(), computed.normalize()));
    }
    for (imp, label) in [(Impuesto::Isr, "ISR"), (Impuesto::Iva, "IVA")] {
        let booked = match imp {
            Impuesto::Isr => built.booked_retenido.isr.abs(),
            Impuesto::Iva => built.booked_retenido.iva.abs(),
        };
        let on_cfdi = g.totales.retenciones.iter().find(|r| r.0 == imp).map(|r| r.1).unwrap_or_default();
        if (booked - on_cfdi).abs() > tolerance {
            warnings.push(format!("{label} withheld in the books is {}, but the CFDI withholds {}", booked.normalize(), on_cfdi.normalize()));
        }
    }
}

/// A sealed (or draft) document ready for a PAC.
pub struct Prepared {
    pub generated: cfdi::Generated,
    pub tipo: TipoDeComprobante,
    pub voucher_number: String,
    pub rfc_emisor: String,
    pub rfc_receptor: String,
    /// Total of the comprobante (0 for a payment receipt).
    pub total: Decimal,
    pub metodo_pago: Option<MetodoPago>,
    pub serie: Option<String>,
    pub folio: String,
    /// For an invoice: its taxes, for later payment receipts.
    pub taxes: Option<DocTaxes>,
    pub warnings: Vec<String>,
}

/// Build the CFDI for a Sales, Credit Note or Receipt voucher, sealed with
/// the installed CSD when a password is given.
pub fn prepare_for_pac(dir: &Path, slot: &Slot, voucher_id: &str, opts: &CfdiOptions) -> ApiResult<Prepared> {
    let book = open_ref(slot)?;
    mx_only(book)?;
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    let (rfc_emisor, issuer) = issuer_ready(slot)?;
    if v.kind == VoucherKind::Receipt {
        let (recibo, mut warnings) = build_pago(book, store_ref(slot)?, &rfc_emisor, &issuer, v, opts)?;
        let csd = csd_for(dir, &rfc_emisor, &opts.password, &mut warnings)?;
        let generated = pagos::generate(&recibo, csd.as_ref())?;
        return Ok(Prepared {
            generated,
            tipo: TipoDeComprobante::Pago,
            voucher_number: v.number.clone(),
            rfc_emisor,
            rfc_receptor: recibo.receptor.rfc.clone(),
            total: Decimal::ZERO,
            metodo_pago: None,
            serie: recibo.serie.clone(),
            folio: recibo.folio.clone(),
            taxes: None,
            warnings,
        });
    }
    let mut built = build_comprobante(book, &rfc_emisor, &issuer, v, opts)?;
    let mut warnings = std::mem::take(&mut built.warnings);
    let csd = csd_for(dir, &rfc_emisor, &opts.password, &mut warnings)?;
    let generated = cfdi::generate(&built.comprobante, csd.as_ref())?;
    check_booked(&built, &generated, &mut warnings);
    let c = &built.comprobante;
    Ok(Prepared {
        total: generated.totales.total,
        generated,
        tipo: c.tipo,
        voucher_number: v.number.clone(),
        rfc_emisor,
        rfc_receptor: c.receptor.rfc.clone(),
        metodo_pago: Some(c.metodo_pago),
        serie: c.serie.clone(),
        folio: c.folio.clone(),
        taxes: Some(DocTaxes::from_comprobante(c)),
        warnings,
    })
}

fn dto(p: Prepared) -> CfdiDto {
    CfdiDto {
        sealed: !p.generated.sello.is_empty(),
        no_certificado: p.generated.no_certificado.clone(),
        total: crate::api::amt(p.total),
        xml: p.generated.xml,
        cadena_original: p.generated.cadena_original,
        warnings: p.warnings,
    }
}

/// The CFDI 4.0 XML for a Sales or Credit Note voucher, sealed with the
/// installed CSD when a password is given. Save it with `write_gst_json`.
pub fn cfdi_xml(dir: &Path, slot: &Slot, voucher_id: &str, opts: &CfdiOptions) -> ApiResult<CfdiDto> {
    let book = open_ref(slot)?;
    mx_only(book)?;
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    if !matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote) {
        return Err("a CFDI is made from a Sales or Credit Note voucher".into());
    }
    prepare_for_pac(dir, slot, voucher_id, opts).map(dto)
}

/// The payment receipt (CFDI de tipo P with the Complemento de Pagos 2.0)
/// for a Receipt voucher that settles PPD invoices bill by bill.
pub fn pago_xml(dir: &Path, slot: &Slot, voucher_id: &str, opts: &CfdiOptions) -> ApiResult<CfdiDto> {
    let book = open_ref(slot)?;
    mx_only(book)?;
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    if v.kind != VoucherKind::Receipt {
        return Err("a payment receipt is made from a Receipt voucher".into());
    }
    prepare_for_pac(dir, slot, voucher_id, opts).map(dto)
}

// ------------------------------------------------------------ payment receipts

/// The Sales voucher that raised bill `reference` for `ledger` (a New
/// allocation with that reference, else a Sales voucher with that number).
fn invoice_for<'a>(book: &'a Book, ledger: LedgerId, reference: &str) -> Option<&'a Voucher> {
    let sales = || book.vouchers().filter(|v| v.kind == VoucherKind::Sales && !v.cancelled);
    let raises = |v: &&Voucher| {
        v.entries.iter().any(|e| e.ledger == ledger && e.bills.iter().any(|b| b.kind == BillKind::New && b.reference.trim().eq_ignore_ascii_case(reference)))
    };
    sales()
        .find(raises)
        .or_else(|| sales().find(|v| v.number.trim().eq_ignore_ascii_case(reference) && v.entries.iter().any(|e| e.ledger == ledger)))
}

/// The bill's New amount on the invoice.
fn bill_amount(v: &Voucher, ledger: LedgerId, reference: &str) -> Option<Amount> {
    let news: Vec<Amount> = v
        .entries
        .iter()
        .filter(|e| e.ledger == ledger)
        .flat_map(|e| e.bills.iter().filter(|b| b.kind == BillKind::New && b.reference.trim().eq_ignore_ascii_case(reference)).map(|b| b.amount))
        .collect();
    if news.is_empty() {
        v.entries.iter().find(|e| e.ledger == ledger && e.side == DrCr::Dr).map(|e| e.amount)
    } else {
        Some(news.iter().copied().sum())
    }
}

/// What was settled on the bill before `receipt` (earlier date, or the
/// same date and earlier in the books), and how many receipts did it.
fn settled_before(book: &Book, ledger: LedgerId, reference: &str, receipt: &Voucher, invoice: &Voucher) -> (Amount, u32) {
    let order: BTreeMap<VoucherId, usize> = book.vouchers().enumerate().map(|(i, v)| (v.id, i)).collect();
    let pos = |v: &Voucher| (v.date, order.get(&v.id).copied().unwrap_or(usize::MAX));
    let mut settled = Amount::ZERO;
    let mut receipts = 0u32;
    for v in book.vouchers().filter(|v| !v.cancelled && v.id != receipt.id && v.id != invoice.id && pos(v) < pos(receipt)) {
        let mut counted = false;
        for e in v.entries.iter().filter(|e| e.ledger == ledger) {
            for b in e.bills.iter().filter(|b| b.kind == BillKind::Against && b.reference.trim().eq_ignore_ascii_case(reference)) {
                settled += if e.side == DrCr::Cr { b.amount } else { -b.amount };
                if e.side == DrCr::Cr && v.kind == VoucherKind::Receipt && !counted {
                    receipts += 1;
                    counted = true;
                }
            }
        }
    }
    (settled, receipts)
}

fn build_pago(book: &Book, store: &Store, rfc_emisor: &str, issuer: &MxIssuer, v: &Voucher, opts: &CfdiOptions) -> ApiResult<(ReciboPago, Vec<String>)> {
    if v.cancelled {
        return Err("this voucher is cancelled".into());
    }
    crate::forex::refuse_foreign_einvoice(v, "A payment complement (CFDI de pagos)")?;
    let mut warnings = Vec::new();
    let party_i = party_line(book, v).ok_or("this receipt has no customer line")?;
    let entry = &v.entries[party_i];
    let party = book.ledger(entry.ledger).ok_or("unknown customer ledger")?;
    let mut against: Vec<(String, Amount)> = Vec::new();
    for e in v.entries.iter().filter(|e| e.ledger == entry.ledger && e.side == DrCr::Cr) {
        for b in &e.bills {
            if b.kind == BillKind::Against {
                match against.iter_mut().find(|(r, _)| r.eq_ignore_ascii_case(b.reference.trim())) {
                    Some(x) => x.1 += b.amount,
                    None => against.push((b.reference.trim().to_owned(), b.amount)),
                }
            } else {
                warnings.push(format!("{} {} of this receipt is not against an invoice; it is not in the payment complement", b.amount.normalize(), match b.kind {
                    BillKind::Advance => "(advance)",
                    BillKind::OnAccount => "(on account)",
                    _ => "(new reference)",
                }));
            }
        }
    }
    if against.is_empty() {
        return Err("allocate the receipt against the invoices it pays (bill-wise, Against references) first".into());
    }
    let mut documentos = Vec::new();
    for (reference, paid) in &against {
        let inv = invoice_for(book, entry.ledger, reference).ok_or_else(|| format!("no Sales voucher raises bill {reference:?} for {}", party.name))?;
        let record = crate::pac_client::read_record(store, &inv.id.0.to_string())?.filter(|r| r.is_active());
        let given = opts.uuids.iter().find(|(k, _)| k.trim().eq_ignore_ascii_case(reference)).map(|(_, u)| u.trim().to_uppercase()).filter(|u| !u.is_empty());
        let (uuid, total, taxes, serie, folio) = match (&record, given) {
            (Some(r), _) => {
                if r.metodo_pago.as_deref() == Some("PUE") {
                    return Err(format!("invoice {} was stamped PUE (paid at issue): it takes no payment complement", inv.number));
                }
                let taxes = r.taxes.clone().ok_or_else(|| format!("the stamp record of {} has no tax detail; stamp it again or give its UUID", inv.number))?;
                (r.uuid.clone(), r.total, taxes, r.serie.clone(), Some(r.folio.clone()).filter(|f| !f.is_empty()))
            }
            (None, Some(u)) => {
                let o = CfdiOptions { metodo_pago: "PPD".into(), forma_pago: "99".into(), uso_cfdi: "G03".into(), regimen_fiscal_receptor: "601".into(), ..opts.clone() };
                let o = CfdiOptions { retenciones: String::new(), ..o };
                let built = build_comprobante(book, rfc_emisor, issuer, inv, &o)?;
                warnings.push(format!("invoice {}: not stamped through KhataOS; its taxes are worked out from the books as they are now", inv.number));
                let taxes = DocTaxes::from_comprobante(&built.comprobante);
                (u, taxes.total, taxes, built.comprobante.serie.clone(), Some(built.comprobante.folio.clone()))
            }
            (None, None) => return Err(format!("invoice {} has no folio fiscal: stamp it through the PAC first, or give its UUID", inv.number)),
        };
        if !pagos::is_uuid(&uuid) {
            return Err(format!("{uuid:?} (invoice {}) is not a UUID", inv.number));
        }
        if let Some(bill) = bill_amount(inv, entry.ledger, reference)
            && (bill - total).abs() > Decimal::new(1, 2)
        {
            warnings.push(format!("invoice {}: the bill in the books is {}, the CFDI's Total {}", inv.number, bill.normalize(), total.normalize()));
        }
        let (before, receipts) = settled_before(book, entry.ledger, reference, v, inv);
        let saldo = cfdi::money(total - before);
        if saldo <= Decimal::ZERO {
            return Err(format!("invoice {} is already settled in full", inv.number));
        }
        let pagado = cfdi::money(*paid);
        if pagado > saldo {
            return Err(format!("{} is allocated to invoice {}, whose balance is {}: put the rest on account", pagado.normalize(), inv.number, saldo.normalize()));
        }
        documentos.push(DoctoRelacionado { id_documento: uuid, serie, folio, num_parcialidad: receipts + 1, saldo_anterior: saldo, pagado, taxes });
    }
    let monto: Decimal = documentos.iter().map(|d| d.pagado).sum();
    let forma = opts.forma_pago.trim();
    if forma.is_empty() || forma == "99" {
        return Err("give how the payment was received (FormaDePagoP: 01 efectivo, 02 cheque, 03 transferencia…; never 99)".into());
    }
    let (receptor, _) = receptor_for(Some(party), issuer, &CfdiOptions { uso_cfdi: pagos::USO_CFDI.into(), ..opts.clone() }, &mut warnings)?;
    if !generic_rfc(&receptor.rfc) && receptor.regimen_fiscal.is_empty() {
        return Err("give the customer's régimen fiscal".into());
    }
    let now = Local::now().naive_local().with_nanosecond(0).unwrap_or_default();
    let fecha_pago = NaiveDateTime::new(v.date, NaiveTime::from_hms_opt(12, 0, 0).expect("noon"));
    let related = opts.related_uuid.trim();
    let recibo = ReciboPago {
        serie: Some(issuer.serie.clone()).filter(|s| !s.is_empty()),
        folio: v.number.chars().filter(|c| c.is_ascii_alphanumeric()).collect(),
        fecha: now.max(fecha_pago),
        lugar_expedicion: issuer.lugar_expedicion.clone(),
        sustituye: if related.is_empty() { Vec::new() } else { vec![related.to_uppercase()] },
        emisor: emisor(book, rfc_emisor, issuer),
        receptor,
        pagos: vec![Pago {
            fecha_pago,
            forma_de_pago: forma.to_owned(),
            monto,
            num_operacion: Some(opts.num_operacion.trim().to_owned()).filter(|n| !n.is_empty()),
            documentos,
        }],
    };
    Ok((recibo, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, ledger, voucher};
    use crate::api::{StockLineInput, create_company, create_party_ledger, post_voucher, set_country};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

    const CER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../crates/khata-mx/testdata/test-csd.cer");
    const KEY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../crates/khata-mx/testdata/test-csd.key");

    fn mx_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Escuela Kemper Urgate").unwrap();
        set_country(&mut slot, Some(&f.0), "MX").unwrap();
        set_company_tax(&mut slot, "EKU9003173C9", "JAL").unwrap();
        for (class, pct) in [("STANDARD", "16"), ("BORDER", "8"), ("ZERO", "0"), ("EXEMPT", "0")] {
            set_tax_rate(&mut slot, class, "2010-01-01", pct).unwrap();
        }
        (f, slot)
    }

    /// A sale (2 × 1000 at 16%, 300 at 0%) and a purchase; the sale's id.
    fn books(slot: &mut Slot) -> String {
        let debtors = group_id(slot, "Sundry Debtors");
        let creditors = group_id(slot, "Sundry Creditors");
        let buyer = create_party_ledger(slot, "Cliente SA", &debtors, "", None, Some("EKU9003173C9"), None).unwrap().ledger.id;
        let input: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": "Cliente SA", "group_id": debtors, "tax_id": "EKU9003173C9", "mailing": { "postal_code": "26015" },
        }))
        .unwrap();
        crate::masters::alter_ledger(slot, &buyer, &input).unwrap();
        let supplier = create_party_ledger(slot, "Proveedor", &creditors, "", None, Some("EKU9003173C9"), None).unwrap().ledger.id;
        let sales = ledger(slot, "Ventas", "Sales Accounts", "");
        let purchase = ledger(slot, "Compras", "Purchase Accounts", "");
        let output = ledger(slot, "IVA trasladado", "Duties & Taxes", "");
        let input_iva = ledger(slot, "IVA acreditable", "Duties & Taxes", "");
        set_ledger_tax(slot, &output, "IVA", TaxSide::Output).unwrap();
        set_ledger_tax(slot, &input_iva, "IVA", TaxSide::Input).unwrap();
        let nos = save_unit(slot, "", "Pza", "", 0).unwrap().id;
        let item = |slot: &mut Slot, name: &str, class: &str| {
            let input: StockItemInput = serde_json::from_value(serde_json::json!({ "name": name, "unit_id": nos, "tax_class": class })).unwrap();
            save_stock_item(slot, &input).unwrap().id
        };
        let laptop = item(slot, "Laptop", "STANDARD");
        let book = item(slot, "Libro", "ZERO");
        let line = |ledger: &str, side: DrCr, items: &[(&str, &str, &str)]| {
            let total: Decimal = items.iter().map(|(_, q, r)| q.parse::<Decimal>().unwrap() * r.parse::<Decimal>().unwrap()).sum();
            let mut e = entry(ledger, side, &total.to_string());
            e.items = items
                .iter()
                .map(|(id, q, r)| StockLineInput {
                    item_id: (*id).into(),
                    qty: (*q).into(),
                    rate: (*r).into(),
                    amount: String::new(),
                    godown_id: String::new(),
                    batch: String::new(),
                })
                .collect();
            e
        };
        let sale = post_voucher(slot, voucher("Sales", "2026-05-02", vec![
            entry(&buyer, DrCr::Dr, "2620"),
            line(&sales, DrCr::Cr, &[(&laptop, "2", "1000"), (&book, "1", "300")]),
            entry(&output, DrCr::Cr, "320"),
        ]))
        .unwrap()
        .id;
        post_voucher(slot, voucher("Purchase", "2026-05-03", vec![
            entry(&supplier, DrCr::Cr, "1160"),
            line(&purchase, DrCr::Dr, &[(&laptop, "1", "1000")]),
            entry(&input_iva, DrCr::Dr, "160"),
        ]))
        .unwrap();
        sale
    }

    #[test]
    fn monthly_iva_summary_from_the_books() {
        let (_f, mut slot) = mx_company("mx-iva");
        books(&mut slot);
        let s = iva_summary_mx(&slot, "2026-05-01", "2026-05-31").unwrap();
        assert_eq!(s.rfc.as_deref(), Some("EKU9003173C9"));
        let m = &s.summary;
        assert_eq!((m.sales.taxed_16, m.sales.zero_rated, m.purchases.taxed_16), (dec!(2000), dec!(300), dec!(1000)));
        assert_eq!((m.iva_trasladado, m.iva_acreditable, m.iva_a_cargo, m.saldo_a_favor), (dec!(320), dec!(160), dec!(160), dec!(0)));
        assert!(m.warnings.is_empty(), "{:?}", m.warnings);
    }

    #[test]
    fn cfdi_draft_and_sealed_with_a_csd_outside_the_books() {
        let (f, mut slot) = mx_company("mx-cfdi");
        let sale = books(&mut slot);
        let opts = CfdiOptions {
            forma_pago: "03".into(),
            metodo_pago: "PUE".into(),
            uso_cfdi: "G03".into(),
            regimen_fiscal_receptor: "601".into(),
            ..Default::default()
        };
        let dir = f.0.with_extension("appdata");
        assert!(cfdi_xml(&dir, &slot, &sale, &opts).unwrap_err().contains("régimen fiscal"));
        assert!(set_mx_issuer(&mut slot, &MxIssuer { lugar_expedicion: "4507".into(), ..Default::default() }).is_err());
        set_mx_issuer(&mut slot, &MxIssuer { regimen_fiscal: "601".into(), lugar_expedicion: "45079".into(), serie: "A".into(), nombre: String::new() }).unwrap();
        assert_eq!(mx_issuer(&slot).unwrap().serie, "A");

        let draft = cfdi_xml(&dir, &slot, &sale, &opts).unwrap();
        assert!(!draft.sealed && !draft.xml.contains("Sello="));
        assert_eq!(draft.total, "2620");
        assert!(draft.xml.contains("Rfc=\"EKU9003173C9\" Nombre=\"Cliente SA\" DomicilioFiscalReceptor=\"26015\""));
        assert!(draft.xml.contains("TasaOCuota=\"0.160000\" Importe=\"320.00\""));
        assert!(draft.xml.contains("TasaOCuota=\"0.000000\" Importe=\"0.00\""));
        assert!(!draft.warnings.iter().any(|w| w.contains("IVA booked")), "{:?}", draft.warnings);

        // Installing the CSD copies it to the app data dir, user-only.
        assert!(!mx_csd_status(&dir, &slot).unwrap().installed);
        assert!(set_mx_csd(&dir, &slot, CER, KEY, "wrong").is_err());
        let st = set_mx_csd(&dir, &slot, CER, KEY, "12345678a").unwrap();
        assert_eq!((st.installed, st.no_certificado.as_deref()), (true, Some("30001000000500003416")));
        assert_eq!(mx_csd_status(&dir, &slot).unwrap().no_certificado.as_deref(), Some("30001000000500003416"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let (cer, key) = csd_paths(&dir, "EKU9003173C9");
            for p in [cer, key] {
                assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o077, 0, "{p:?} is user-only");
            }
            assert_eq!(std::fs::metadata(dir.join(CSD_DIR)).unwrap().permissions().mode() & 0o077, 0);
        }
        let sealed = cfdi_xml(&dir, &slot, &sale, &CfdiOptions { password: "12345678a".into(), ..opts.clone() }).unwrap();
        assert!(sealed.sealed && sealed.xml.contains("NoCertificado=\"30001000000500003416\"") && sealed.xml.contains("Sello=\""));
        assert!(sealed.cadena_original.starts_with("||4.0|A|"));
        assert!(cfdi_xml(&dir, &slot, &sale, &CfdiOptions { password: "nope".into(), ..opts.clone() }).is_err());
        // The company file holds no key material.
        let raw = std::fs::read(&f.0).unwrap();
        let key = std::fs::read(KEY).unwrap();
        assert!(!raw.windows(32).any(|w| w == &key[..32]), "the key is not in the books");
        assert!(!clear_mx_csd(&dir, &slot).unwrap().installed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_foreign_currency_invoice_gets_no_cfdi_until_moneda_is_written() {
        let (f, mut slot) = mx_company("mx-forex");
        books(&mut slot);
        set_mx_issuer(&mut slot, &MxIssuer { regimen_fiscal: "601".into(), lugar_expedicion: "45079".into(), serie: "A".into(), nombre: String::new() }).unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let acme = create_party_ledger(&mut slot, "Acme Inc", &debtors, "", None, None, None).unwrap().ledger.id;
        crate::forex::set_ledger_currency(&mut slot, &acme, "USD").unwrap();
        let sales = ledger(&mut slot, "Ventas exportación", "Sales Accounts", "");
        let fx = || Some(crate::forex::ForexInput { currency: "USD".into(), amount: "100".into(), rate: "18".into(), bills: Vec::new() });
        let (mut party, mut income) = (entry(&acme, DrCr::Dr, "1800"), entry(&sales, DrCr::Cr, "1800"));
        (party.forex, income.forex) = (fx(), fx());
        let sale = post_voucher(&mut slot, voucher("Sales", "2026-05-04", vec![party, income])).unwrap().id;
        let opts = CfdiOptions { forma_pago: "03".into(), metodo_pago: "PUE".into(), uso_cfdi: "G03".into(), regimen_fiscal_receptor: "601".into(), ..Default::default() };
        let refused = cfdi_xml(&f.0.with_extension("appdata"), &slot, &sale, &opts).unwrap_err();
        assert!(refused.starts_with("A CFDI for an invoice in USD is not supported yet"), "{refused}");
    }

    #[test]
    fn withholdings_in_the_iva_summary() {
        let (_f, mut slot) = mx_company("mx-ret");
        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let classed = |slot: &mut Slot, name: &str, group: &str, class: &str| {
            let id = create_party_ledger(slot, name, group, "", None, Some("URE180429TM6"), None).unwrap().ledger.id;
            let input: crate::masters::LedgerInput =
                serde_json::from_value(serde_json::json!({ "name": name, "group_id": group, "tax_id": "URE180429TM6", "tax_class": class })).unwrap();
            crate::masters::alter_ledger(slot, &id, &input).unwrap();
            id
        };
        let customer = classed(&mut slot, "Universidad", &debtors, "RET:ISR10+IVA2/3");
        let other = classed(&mut slot, "Instituto", &debtors, "RET:ISR10+IVA2/3");
        let freelancer = classed(&mut slot, "Diseñadora", &creditors, "RET:ISR10+IVA2/3");
        let fees = ledger(&mut slot, "Honorarios", "Sales Accounts", "");
        let design = ledger(&mut slot, "Diseño", "Indirect Expenses", "");
        let output = ledger(&mut slot, "IVA trasladado", "Duties & Taxes", "");
        let input_iva = ledger(&mut slot, "IVA acreditable", "Duties & Taxes", "");
        let ret_iva = ledger(&mut slot, "IVA retenido por clientes", "Duties & Taxes", "");
        let ret_isr = ledger(&mut slot, "ISR retenido por clientes", "Duties & Taxes", "");
        let owe_iva = ledger(&mut slot, "IVA retenido por enterar", "Duties & Taxes", "");
        let owe_isr = ledger(&mut slot, "ISR retenido por enterar", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "IVA", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input_iva, "IVA", TaxSide::Input).unwrap();
        for (l, c, side) in [(&ret_iva, "RET-IVA", TaxSide::Input), (&ret_isr, "RET-ISR", TaxSide::Input), (&owe_iva, "RET-IVA", TaxSide::Output), (&owe_isr, "RET-ISR", TaxSide::Output)] {
            set_ledger_tax(&mut slot, l, c, side).unwrap();
        }
        assert_eq!(withholding_component("IVA RETENIDO"), Some(Impuesto::Iva));
        assert_eq!(withholding_component("IVA"), None);
        // Booked: 1000 + 160 − 100 − 106.67.
        post_voucher(&mut slot, voucher("Sales", "2026-05-02", vec![
            entry(&customer, DrCr::Dr, "953.33"),
            entry(&ret_isr, DrCr::Dr, "100"),
            entry(&ret_iva, DrCr::Dr, "106.67"),
            entry(&fees, DrCr::Cr, "1000"),
            entry(&output, DrCr::Cr, "160"),
        ]))
        .unwrap();
        // Not booked: worked out from the class, with a warning.
        post_voucher(&mut slot, voucher("Sales", "2026-05-03", vec![entry(&other, DrCr::Dr, "580"), entry(&fees, DrCr::Cr, "500"), entry(&output, DrCr::Cr, "80")])).unwrap();
        // The business withholds from a freelancer: 200 + 32 − 20 − 21.33.
        post_voucher(&mut slot, voucher("Purchase", "2026-05-04", vec![
            entry(&freelancer, DrCr::Cr, "190.67"),
            entry(&owe_isr, DrCr::Cr, "20"),
            entry(&owe_iva, DrCr::Cr, "21.33"),
            entry(&design, DrCr::Dr, "200"),
            entry(&input_iva, DrCr::Dr, "32"),
        ]))
        .unwrap();
        let s = iva_summary_mx(&slot, "2026-05-01", "2026-05-31").unwrap().summary;
        assert_eq!((s.iva_trasladado, s.iva_retenido_por_clientes, s.isr_retenido_por_clientes), (dec!(240), dec!(160), dec!(150)), "106.67 + 53.33; 100 + 50");
        assert_eq!((s.iva_acreditable, s.iva_retenido_a_enterar, s.isr_retenido_a_enterar), (dec!(32), dec!(21.33), dec!(20)));
        assert_eq!(s.iva_a_cargo, dec!(48), "240 − 160 − 32");
        assert_eq!(s.warnings.iter().filter(|w| w.contains("no withholding is booked")).count(), 1, "{:?}", s.warnings);
    }

    #[test]
    fn mexico_only() {
        let f = TempFile::new("mx-only");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Nada").unwrap();
        assert!(iva_summary_mx(&slot, "2026-05-01", "2026-05-31").is_err());
        assert!(mx_issuer(&slot).is_err());
    }
}

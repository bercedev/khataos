//! Spain: the modelo 303 and VeriFactu billing records from the books. The
//! rules live in `khata_es` (`returns`, `verifactu`); the vouchers are read
//! as in `vat_de.rs`, with the reader shared by France and the Netherlands
//! (`vat_fr::eu`): Sales and Credit Notes are outward, Purchases and Debit
//! Notes inward; ledgers whose type of tax is "IVA" (or "VAT") give the
//! IVA. Credit notes are facturas rectificativas: casillas 14/15 for the
//! company's own, 40/41 for its suppliers'.
//!
//! **VeriFactu records** ([`verifactu_record`]). The chain is per
//! installation, like the ZATCA device chain in `zatca.rs`: the record
//! count and the last record's id and huella live in this company file's
//! settings (`verifactu.chain`), outside the audit trail and never synced.
//! Each voucher's record is kept under `verifactu.doc.<voucher id>` (and
//! its anulación under `verifactu.cancel.<voucher id>`), so asking again
//! returns the same record. The chain is written before the record: a
//! crash in between leaves a gap in the count but never reuses a huella.
//! The billing software's details (SistemaInformatico: producer name and
//! NIF, installation number) and the QR environment are in
//! `verifactu.system` ([`verifactu_system`]). Nothing is sent to the AEAT.
//!
//! Vouchers: a Sales voucher → F1 (F2, a factura simplificada, when the
//! buyer has no NIF); a Credit Note → R1 por diferencias, quoting the
//! invoice it is against. A cancelled voucher with a record gets a
//! registro de anulación.

use chrono::{DateTime, FixedOffset, Local};
use khata_core::{BillKind, Book, Voucher, VoucherId, VoucherKind};
use khata_es::iva::{self, Treatment};
use khata_es::returns::{self, DocKind, DocLine, InwardDoc, Modelo303, OutwardDoc, Period};
use khata_es::verifactu::{self, Alta, ChainState, Detalle, Entorno, IdFactura, Registro, SistemaInformatico, TipoFactura};
use khata_es::{comunidades, nif};
use khata_store::Store;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, open_mut, open_ref, parse_date, parse_uuid};
use crate::vat_fr::eu::{self, Kind};

const CHAIN_KEY: &str = "verifactu.chain";
const SYSTEM_KEY: &str = "verifactu.system";
const DOC_PREFIX: &str = "verifactu.doc.";
const CANCEL_PREFIX: &str = "verifactu.cancel.";

fn classify(class: &str, rate: Decimal) -> Kind {
    match iva::treatment(class, rate) {
        Treatment::Taxed => Kind::Taxed,
        Treatment::Zero => Kind::Zero,
        Treatment::Exempt => Kind::Exempt,
        Treatment::ReverseCharge => Kind::Reverse,
        Treatment::IntraEu => Kind::IntraEu,
        Treatment::Export => Kind::Export,
        Treatment::OutOfScope => Kind::OutOfScope,
    }
}

pub(crate) const SPAIN: eu::Country = eu::Country {
    code: "ES",
    components: &[iva::IVA, "VAT"],
    in_eu: comunidades::in_eu,
    also_home: &[],
    standard_class: iva::STANDARD_CLASS,
    classify,
};

fn treatment(k: Kind) -> Treatment {
    match k {
        Kind::Taxed => Treatment::Taxed,
        Kind::Zero => Treatment::Zero,
        Kind::Exempt => Treatment::Exempt,
        Kind::Reverse => Treatment::ReverseCharge,
        Kind::IntraEu => Treatment::IntraEu,
        Kind::Export => Treatment::Export,
        Kind::OutOfScope => Treatment::OutOfScope,
    }
}

fn spain_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_es::COUNTRY.code) {
        return Err("the modelo 303 and VeriFactu records are for companies in Spain".into());
    }
    Ok(())
}

// ------------------------------------------------------------ modelo 303

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Modelo303Dto {
    /// The company's NIF as stored.
    pub tax_id: Option<String>,
    #[serde(flatten)]
    pub ret: Modelo303,
}

/// The modelo 303 casillas for a period (a quarter or a month).
pub fn modelo_303_es(slot: &Slot, from: &str, to: &str) -> ApiResult<Modelo303Dto> {
    let book = open_ref(slot)?;
    spain_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let mut warnings = Vec::new();
    let docs = eu::documents(book, period.from, period.to, &SPAIN, &mut warnings)?;
    let (mut outward, mut inward) = (Vec::new(), Vec::new());
    for d in docs {
        let kind = if d.credit { DocKind::CreditNote } else { DocKind::Invoice };
        let lines = d
            .lines
            .iter()
            .map(|l| DocLine { treatment: treatment(l.kind), goods: l.goods, taxable_value: l.value, percent: l.percent, iva: l.tax })
            .collect();
        if d.outward {
            outward.push(OutwardDoc {
                id: d.id,
                kind,
                number: d.number,
                date: d.date,
                party_name: d.party_name,
                party_country: d.party_country,
                party_vat_id: d.party_vat_id,
                cancelled: d.cancelled,
                lines,
            });
        } else {
            inward.push(InwardDoc {
                id: d.id,
                kind,
                number: d.number,
                date: d.date,
                party_name: d.party_name,
                party_country: d.party_country,
                party_vat_id: d.party_vat_id,
                cancelled: d.cancelled,
                lines,
            });
        }
    }
    let mut ret = returns::modelo_303(&outward, &inward, period);
    ret.warnings.splice(0..0, warnings);
    Ok(Modelo303Dto { tax_id: profile.tax_id.clone(), ret })
}

// ------------------------------------------------------------ VeriFactu

fn read<T: for<'de> Deserialize<'de>>(store: &Store, key: &str) -> ApiResult<Option<T>> {
    match store.setting(key).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map(Some).map_err(|e| format!("setting {key} is damaged: {e}")),
        None => Ok(None),
    }
}

fn write<T: Serialize>(store: &Store, key: &str, value: &T) -> ApiResult<()> {
    let json = serde_json::to_string(value).map_err(err)?;
    store.set_setting(key, Some(&json)).map_err(err)
}

/// The billing software's details and where the QR code points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VerifactuSystem {
    pub sistema: SistemaInformatico,
    /// The QR code points at the AEAT's test environment.
    pub pruebas: bool,
}

impl Default for VerifactuSystem {
    fn default() -> Self {
        VerifactuSystem {
            sistema: SistemaInformatico {
                nombre_razon: String::new(),
                nif: String::new(),
                nombre_sistema: "KhataOS".into(),
                id_sistema: "KO".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                numero_instalacion: String::new(),
                solo_verifactu: false,
                multi_ot: true,
                multiples_ot: false,
            },
            pruebas: false,
        }
    }
}

fn system(store: &Store) -> ApiResult<VerifactuSystem> {
    let mut s: VerifactuSystem = read(store, SYSTEM_KEY)?.unwrap_or_default();
    if s.sistema.numero_instalacion.trim().is_empty() {
        // One installation per company file on each computer.
        s.sistema.numero_instalacion = uuid::Uuid::new_v4().simple().to_string()[..16].to_uppercase();
        write(store, SYSTEM_KEY, &s)?;
    }
    Ok(s)
}

/// The VeriFactu software details (with this installation's number).
pub fn verifactu_system(slot: &mut Slot) -> ApiResult<VerifactuSystem> {
    let (store, book) = open_mut(slot)?;
    spain_only(book)?;
    system(store)
}

/// Replace the software details; a blank installation number keeps the
/// current one.
pub fn set_verifactu_system(slot: &mut Slot, settings: &VerifactuSystem) -> ApiResult<VerifactuSystem> {
    let (store, book) = open_mut(slot)?;
    spain_only(book)?;
    let mut s = settings.clone();
    if s.sistema.numero_instalacion.trim().is_empty() {
        s.sistema.numero_instalacion = system(store)?.sistema.numero_instalacion;
    }
    if !s.sistema.nif.trim().is_empty() {
        s.sistema.nif = nif::validate(&s.sistema.nif).map_err(|e| format!("the producer's NIF is invalid: {e}"))?.value;
    }
    write(store, SYSTEM_KEY, &s)?;
    Ok(s)
}

/// One voucher's record as kept in the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    alta: Alta,
    registro: Registro,
    qr_url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VerifactuRecordDto {
    /// "alta" or "anulacion".
    pub kind: String,
    /// TipoFactura of the invoice ("F1", "F2", "R1").
    pub tipo: String,
    pub num_serie: String,
    pub xml: String,
    pub huella: String,
    /// The huella this record chains to; `None` for the first record.
    pub huella_anterior: Option<String>,
    /// The record's place in the installation's chain, from 1.
    pub numero: u64,
    /// FechaHoraHusoGenRegistro.
    pub generado: String,
    /// The QR code's URL (the AEAT cotejo service).
    pub qr_url: String,
    /// `VERIFACTU-<number>-alta.xml`.
    pub file_name: String,
    /// The record was already generated; this is the one recorded.
    pub regenerated: bool,
    pub warnings: Vec<String>,
}

fn dto(s: &Stored, regenerated: bool, warnings: Vec<String>) -> VerifactuRecordDto {
    let r = &s.registro;
    let safe: String = r.id.num_serie.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    VerifactuRecordDto {
        kind: r.kind.clone(),
        tipo: s.alta.tipo.code().into(),
        num_serie: r.id.num_serie.clone(),
        xml: r.xml.clone(),
        huella: r.huella.clone(),
        huella_anterior: r.anterior.as_ref().map(|a| a.huella.clone()),
        numero: r.numero,
        generado: r.generado.clone(),
        qr_url: s.qr_url.clone(),
        file_name: format!("VERIFACTU-{safe}-{}.xml", r.kind),
        regenerated,
        warnings,
    }
}

/// The record's invoice from a voucher: issuer, type, breakdown and totals.
fn alta_of(book: &Book, v: &Voucher, warnings: &mut Vec<String>) -> ApiResult<Alta> {
    let credit = match v.kind {
        VoucherKind::Sales => false,
        VoucherKind::CreditNote => true,
        _ => return Err("VeriFactu records are made for Sales and Credit Note vouchers".into()),
    };
    // The record's amounts are in the base currency (docs/spain-vat.md).
    crate::forex::refuse_foreign_einvoice(v, "A VeriFactu record")?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let emisor = profile
        .tax_id
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .ok_or("set the company's NIF first (Company → Tax registration)")?;
    let emisor = nif::validate(emisor).map_err(|e| format!("the company's NIF is invalid: {e}"))?.value;
    let rates = eu::rates(book)?;
    let doc = eu::document(book, v, &rates, &SPAIN, warnings)?.ok_or("not a sales document")?;
    let party = eu::party_line(book, v).and_then(|i| book.ledger(v.entries[i].ledger));
    let buyer_nif = party.and_then(|l| l.tax_id.as_deref()).and_then(|t| nif::validate(t).ok()).map(|d| d.value);
    let destinatario = buyer_nif.map(|n| (party.map(|l| l.name.clone()).unwrap_or_default(), n));
    if destinatario.is_none() && doc.party_vat_id.is_some() {
        warnings.push("the buyer has a foreign VAT id; foreign buyers (IDOtro) are not written yet, so the record has no Destinatario".into());
    }
    let tipo = match (credit, destinatario.is_some()) {
        (false, true) => TipoFactura::F1,
        (false, false) => TipoFactura::F2,
        (true, true) => TipoFactura::R1,
        (true, false) => TipoFactura::R5,
    };
    // Credit notes are rectificativas por diferencias: negative amounts.
    let sign = if credit { Decimal::NEGATIVE_ONE } else { Decimal::ONE };
    let mut desglose: Vec<Detalle> = Vec::new();
    for l in &doc.lines {
        let (clave, calificacion, tipo, cuota) = match l.kind {
            Kind::Taxed | Kind::Zero => ("01", "S1", Some(l.percent), Some(l.tax)),
            Kind::Reverse => ("01", "S2", Some(Decimal::ZERO), Some(Decimal::ZERO)),
            Kind::Exempt => ("01", "E1", None, None),
            Kind::IntraEu => ("01", "E5", None, None),
            Kind::Export => ("02", "E2", None, None),
            Kind::OutOfScope => ("01", "N2", None, None),
        };
        let found = desglose.iter_mut().find(|d| d.clave_regimen == clave && d.calificacion == calificacion && d.tipo == tipo);
        match found {
            Some(d) => {
                d.base += l.value * sign;
                d.cuota = d.cuota.map(|c| c + cuota.unwrap_or_default() * sign);
            }
            None => desglose.push(Detalle {
                clave_regimen: clave.into(),
                calificacion: calificacion.into(),
                tipo,
                base: l.value * sign,
                cuota: cuota.map(|c| c * sign),
            }),
        }
    }
    for d in &mut desglose {
        d.base = d.base.round_dp(2);
        d.cuota = d.cuota.map(|c| c.round_dp(2));
    }
    let cuota_total: Decimal = desglose.iter().filter_map(|d| d.cuota).sum();
    let importe_total = desglose.iter().map(|d| d.base).sum::<Decimal>() + cuota_total;
    let rectificada = credit.then(|| {
        let against: Vec<IdFactura> = eu::party_line(book, v)
            .map(|i| &v.entries[i])
            .into_iter()
            .flat_map(|e| e.bills.iter())
            .filter(|b| b.kind == BillKind::Against && !b.reference.trim().is_empty())
            .filter_map(|b| {
                book.vouchers()
                    .find(|x| x.kind == VoucherKind::Sales && x.number == b.reference)
                    .map(|x| IdFactura { emisor: emisor.clone(), num_serie: x.number.clone(), fecha: x.date })
            })
            .collect();
        if against.is_empty() {
            warnings.push("the credit note is not against a sales invoice (bill-wise): FacturasRectificadas is empty".into());
        }
        ("I".to_owned(), against)
    });
    let descripcion = v.narration.lines().map(str::trim).find(|l| !l.is_empty()).map_or_else(
        || if credit { "Factura rectificativa".to_owned() } else { "Venta".to_owned() },
        |l| l.chars().take(500).collect(),
    );
    Ok(Alta {
        id: IdFactura { emisor, num_serie: v.number.clone(), fecha: v.date },
        nombre_emisor: book.name.clone(),
        tipo,
        descripcion,
        destinatario,
        rectificada,
        desglose,
        cuota_total,
        importe_total,
    })
}

fn now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

/// The VeriFactu record for a Sales or Credit Note voucher: generated (and
/// the chain moved on) the first time, the recorded one after that. A
/// cancelled voucher that has a record gets its registro de anulación.
pub fn verifactu_record(slot: &mut Slot, voucher_id: &str) -> ApiResult<VerifactuRecordDto> {
    verifactu_record_at(slot, voucher_id, now())
}

/// [`verifactu_record`] generated at `at` (tests pin the time).
pub fn verifactu_record_at(slot: &mut Slot, voucher_id: &str, at: DateTime<FixedOffset>) -> ApiResult<VerifactuRecordDto> {
    let (store, book) = open_mut(slot)?;
    spain_only(book)?;
    let id = VoucherId(parse_uuid(voucher_id, "voucher")?);
    let v = book.voucher(id).ok_or("unknown voucher")?;
    let sys = system(store)?;
    let mut warnings = Vec::new();
    if sys.sistema.nombre_razon.trim().is_empty() || sys.sistema.nif.trim().is_empty() {
        warnings.push("the billing software's producer (name and NIF) is blank in the VeriFactu settings; the AEAT requires it".into());
    }
    let doc_key = format!("{DOC_PREFIX}{}", id.0);
    let stored: Option<Stored> = read(store, &doc_key)?;
    let entorno = if sys.pruebas { Entorno::Pruebas } else { Entorno::Produccion };

    if v.cancelled {
        let alta = stored.ok_or("a cancelled voucher that was never recorded has no VeriFactu record")?;
        let key = format!("{CANCEL_PREFIX}{}", id.0);
        if let Some(c) = read::<Stored>(store, &key)? {
            return Ok(dto(&c, true, warnings));
        }
        let mut chain: ChainState = read(store, CHAIN_KEY)?.unwrap_or_default();
        let r = verifactu::anulacion(&alta.alta.id, &mut chain, &sys.sistema, at).map_err(err)?;
        write(store, CHAIN_KEY, &chain)?;
        let c = Stored { alta: alta.alta, registro: r, qr_url: alta.qr_url };
        write(store, &key, &c)?;
        return Ok(dto(&c, false, warnings));
    }

    let alta = alta_of(book, v, &mut warnings)?;
    if let Some(s) = stored {
        if s.alta != alta {
            warnings.push(
                "the voucher changed after its VeriFactu record was generated; this is the record as issued (correct it with a factura rectificativa)"
                    .into(),
            );
        }
        return Ok(dto(&s, true, warnings));
    }
    if let Some(gap) = {
        let issued: Vec<khata_compliance::IssuedDocument<'_>> = book
            .vouchers()
            .filter(|x| x.kind == v.kind && x.id != v.id && x.date <= v.date && !x.cancelled)
            .map(|x| khata_compliance::IssuedDocument { number: &x.number, date: x.date })
            .collect();
        iva::correlative_gap(&v.number, v.date, &issued)
    } {
        warnings.push(format!("invoice numbers must be correlative within a series (RD 1619/2012 art. 6.1.a): {gap}"));
    }
    if alta.id.fecha < chrono::NaiveDate::from_ymd_opt(2024, 10, 28).expect("valid date") {
        warnings.push("the AEAT refuses records of invoices issued before 28-10-2024".into());
    }
    let mut chain: ChainState = read(store, CHAIN_KEY)?.unwrap_or_default();
    warnings.extend(verifactu::chain_time_warning(&chain, at));
    let r = verifactu::alta(&alta, &mut chain, &sys.sistema, at).map_err(err)?;
    // Chain first: a crash before the record leaves a gap, never a reused huella.
    write(store, CHAIN_KEY, &chain)?;
    let qr_url = verifactu::qr_url(entorno, true, &alta.id, alta.importe_total);
    let s = Stored { alta, registro: r, qr_url };
    write(store, &doc_key, &s)?;
    Ok(dto(&s, false, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, indian_company, ledger, voucher};
    use crate::api::{cancel_voucher, check_tax_id, create_company, post_voucher, set_country};
    use crate::masters::save_unit;
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use crate::vat_nl::tests::{item, items, party};
    use chrono::Datelike;
    use khata_core::{DrCr, TaxSide};
    use rust_decimal_macros::dec;

    fn es_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Empresa de Pruebas SL").unwrap();
        set_country(&mut slot, Some(&f.0), "ES").unwrap();
        set_company_tax(&mut slot, "B58378431", "MD").unwrap();
        for (class, percent) in [("STANDARD", "21"), ("REDUCED", "10"), ("SUPERREDUCED", "4"), ("EXEMPT", "0"), ("REVERSE", "0")] {
            set_tax_rate(&mut slot, class, "2012-09-01", percent).unwrap();
        }
        (f, slot)
    }

    #[test]
    fn a_spanish_company_checks_nifs() {
        let f = TempFile::new("es-ids");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Empresa SL").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "es").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("ES", "EUR", "NIF"));
        assert_eq!((info.fy_start.month(), info.fy_start.day()), (1, 1), "calendar years");
        assert_eq!(check_tax_id(&slot, "x-2482300w").unwrap().value, "X2482300W");
        assert!(check_tax_id(&slot, "54362315T").unwrap().error.is_some());
        let tax = set_company_tax(&mut slot, "B58378431", "CT").unwrap();
        assert_eq!((tax.label.as_str(), tax.region_name.as_deref(), tax.regions.len()), ("NIF", Some("Catalunya"), 19));
    }

    struct Books {
        slot: Slot,
        _f: TempFile,
        sale: String,
        credit: String,
    }

    fn books(name: &str) -> Books {
        let (f, mut slot) = es_company(name);
        let cliente = party(&mut slot, "Cliente SA", "Sundry Debtors", Some("A58818501"), "ES");
        let paris = party(&mut slot, "Paris SARL", "Sundry Debtors", Some("FR40303265045"), "FR");
        let proveedor = party(&mut slot, "Proveedor SL", "Sundry Creditors", Some("B64717838"), "ES");
        let ventas = ledger(&mut slot, "Ventas", "Sales Accounts", "");
        let compras = ledger(&mut slot, "Compras", "Purchase Accounts", "");
        let repercutido = ledger(&mut slot, "IVA repercutido", "Duties & Taxes", "");
        let soportado = ledger(&mut slot, "IVA soportado", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &repercutido, "IVA", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &soportado, "IVA", TaxSide::Input).unwrap();
        let ud = save_unit(&mut slot, "", "Ud", "", 0).unwrap().id;
        let portatil = item(&mut slot, "Portátil", &ud, "STANDARD");
        let pan = item(&mut slot, "Pan", &ud, "SUPERREDUCED");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str, qty: &str, rate: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, qty, rate);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap().id;
        let mut l = line(&ventas, DrCr::Cr, "1050", &portatil, "1", "1000");
        l.items.extend(items(&pan, "10", "5"));
        let sale = post(&mut slot, "Sales", "2026-01-15", vec![entry(&cliente, DrCr::Dr, "1262"), l, entry(&repercutido, DrCr::Cr, "212")]);
        let mut cn = voucher("CreditNote", "2026-02-01", vec![
            entry(&cliente, DrCr::Cr, "121"),
            line(&ventas, DrCr::Dr, "100", &portatil, "1", "100"),
            entry(&repercutido, DrCr::Dr, "21"),
        ]);
        let sale_number = crate::api::get_voucher(&slot, &sale).unwrap().number;
        cn.entries[0].bills = vec![crate::api::BillInput { kind: BillKind::Against, reference: sale_number, due: None, amount: "121".into() }];
        let credit = post_voucher(&mut slot, cn).unwrap().id;
        post(&mut slot, "Sales", "2026-02-10", vec![entry(&paris, DrCr::Dr, "3000"), line(&ventas, DrCr::Cr, "3000", &portatil, "3", "1000")]);
        post(&mut slot, "Purchase", "2026-02-05", vec![
            entry(&proveedor, DrCr::Cr, "605"),
            line(&compras, DrCr::Dr, "500", &portatil, "1", "500"),
            entry(&soportado, DrCr::Dr, "105"),
        ]);
        Books { slot, _f: f, sale, credit }
    }

    #[test]
    fn modelo_303_from_the_books() {
        let b = books("es-303");
        let r = modelo_303_es(&b.slot, "2026-01-01", "2026-03-31").unwrap();
        assert_eq!(r.tax_id.as_deref(), Some("B58378431"));
        let m = &r.ret;
        assert_eq!((m.c01, m.c03, m.c07, m.c09), (dec!(50), dec!(2.00), dec!(1000), dec!(210.00)));
        assert_eq!((m.c14, m.c15, m.c59), (dec!(-100), dec!(-21), dec!(3000)));
        assert_eq!((m.c27, m.c28, m.c29, m.c46), (dec!(191.00), dec!(500), dec!(105), dec!(86.00)));
        assert!(!m.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", m.warnings);
        assert!(modelo_303_es(&b.slot, "2026-03-31", "2026-01-01").unwrap_err().contains("before it starts"));
    }

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn verifactu_records_chain_per_installation() {
        let mut b = books("es-verifactu");
        let a = verifactu_record_at(&mut b.slot, &b.sale, at("2026-01-15T10:00:00+01:00")).unwrap();
        assert_eq!((a.kind.as_str(), a.tipo.as_str(), a.numero, a.huella_anterior.as_deref(), a.regenerated), ("alta", "F1", 1, None, false));
        assert!(a.warnings.iter().any(|w| w.contains("producer")), "{:?}", a.warnings);
        let number = crate::api::get_voucher(&b.slot, &b.sale).unwrap().number;
        let expected = verifactu::sha256_upper(&format!(
            "IDEmisorFactura=B58378431&NumSerieFactura={number}&FechaExpedicionFactura=15-01-2026&TipoFactura=F1&CuotaTotal=212.00&ImporteTotal=1262.00&Huella=&FechaHoraHusoGenRegistro=2026-01-15T10:00:00+01:00"
        ));
        assert_eq!(a.huella, expected);
        for s in ["<sum1:CalificacionOperacion>S1</sum1:CalificacionOperacion>", "<sum1:TipoImpositivo>4.00</sum1:TipoImpositivo>", "<sum1:CuotaRepercutida>210.00</sum1:CuotaRepercutida>", "<sum1:NIF>A58818501</sum1:NIF>"] {
            assert!(a.xml.contains(s), "missing {s} in\n{}", a.xml);
        }
        assert!(a.qr_url.starts_with("https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR?nif=B58378431&numserie="), "{}", a.qr_url);
        assert!(a.qr_url.ends_with("&fecha=15-01-2026&importe=1262.00"), "{}", a.qr_url);

        let again = verifactu_record_at(&mut b.slot, &b.sale, at("2026-01-16T10:00:00+01:00")).unwrap();
        assert_eq!((again.huella.as_str(), again.numero, again.regenerated), (a.huella.as_str(), 1, true), "the same record");

        let c = verifactu_record_at(&mut b.slot, &b.credit, at("2026-02-01T09:30:00+01:00")).unwrap();
        assert_eq!((c.tipo.as_str(), c.numero, c.huella_anterior.as_deref()), ("R1", 2, Some(a.huella.as_str())));
        assert!(c.xml.contains("<sum1:TipoRectificativa>I</sum1:TipoRectificativa>"), "{}", c.xml);
        assert!(c.xml.contains(&format!("<sum1:NumSerieFactura>{number}</sum1:NumSerieFactura>")), "quotes the corrected invoice");
        assert!(c.xml.contains("<sum1:ImporteTotal>-121.00</sum1:ImporteTotal>"), "{}", c.xml);

        cancel_voucher(&mut b.slot, &b.credit).unwrap();
        let x = verifactu_record_at(&mut b.slot, &b.credit, at("2026-02-02T09:30:00+01:00")).unwrap();
        assert_eq!((x.kind.as_str(), x.numero, x.huella_anterior.as_deref()), ("anulacion", 3, Some(c.huella.as_str())));
        assert!(x.xml.contains("<sum1:RegistroAnulacion"));

        let sys = verifactu_system(&mut b.slot).unwrap();
        assert_eq!((sys.sistema.nombre_sistema.as_str(), sys.sistema.numero_instalacion.len()), ("KhataOS", 16));
        let mut changed = sys.clone();
        changed.sistema.nif = "q 2826000 h".into();
        changed.sistema.numero_instalacion = String::new();
        let saved = set_verifactu_system(&mut b.slot, &changed).unwrap();
        assert_eq!((saved.sistema.nif.as_str(), saved.sistema.numero_instalacion.as_str()), ("Q2826000H", sys.sistema.numero_instalacion.as_str()));
        changed.sistema.nif = "Q2826000J".into();
        assert!(set_verifactu_system(&mut b.slot, &changed).is_err());
    }

    #[test]
    fn a_foreign_currency_invoice_gets_no_verifactu_record_yet() {
        let (_f, mut slot) = es_company("es-forex");
        let acme = party(&mut slot, "Acme Inc", "Sundry Debtors", None, "US");
        crate::forex::set_ledger_currency(&mut slot, &acme, "USD").unwrap();
        let ventas = ledger(&mut slot, "Ventas", "Sales Accounts", "");
        let fx = || Some(crate::forex::ForexInput { currency: "USD".into(), amount: "100".into(), rate: "0.9".into(), bills: Vec::new() });
        let (mut cliente, mut income) = (entry(&acme, DrCr::Dr, "90"), entry(&ventas, DrCr::Cr, "90"));
        (cliente.forex, income.forex) = (fx(), fx());
        let sale = post_voucher(&mut slot, voucher("Sales", "2026-03-02", vec![cliente, income])).unwrap().id;
        let refused = verifactu_record_at(&mut slot, &sale, at("2026-03-02T10:00:00+01:00")).unwrap_err();
        assert!(refused.starts_with("A VeriFactu record for an invoice in USD is not supported yet"), "{refused}");
    }

    #[test]
    fn spain_only() {
        let (_f, slot) = indian_company("es-india");
        assert!(modelo_303_es(&slot, "2026-01-01", "2026-03-31").unwrap_err().contains("Spain"));
    }
}

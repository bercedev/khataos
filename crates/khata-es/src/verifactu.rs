//! VeriFactu billing records (registros de facturación) under Real Decreto
//! 1007/2023 and Orden HAC/1177/2024: the registro de alta and the registro
//! de anulación, their SHA-256 fingerprint (huella) chained to the
//! previous record, the record's XML, and the QR code URL of the AEAT's
//! cotejo (verification) service printed on the invoice.
//!
//! Generation only: nothing is sent to the AEAT. Submitting the records
//! (the VERI*FACTU web service, SOAP with a qualified certificate) and the
//! "no VERI*FACTU" mode (XAdES-signed records and the event log) are future
//! work (`docs/spain-vat.md`).
//!
//! **The huella** (AEAT, *Detalle de las especificaciones técnicas para
//! generación de la huella o hash de los registros de facturación*,
//! research §6.7): SHA-256 over the UTF-8 bytes of
//!
//! ```text
//! IDEmisorFactura=<NIF>&NumSerieFactura=<number>&FechaExpedicionFactura=<dd-mm-yyyy>
//! &TipoFactura=<F1…>&CuotaTotal=<tax>&ImporteTotal=<total>&Huella=<previous huella>
//! &FechaHoraHusoGenRegistro=<yyyy-mm-ddThh:mm:ss+hh:mm>
//! ```
//!
//! (one line, no spaces), written as 64 upper-case hexadecimal characters.
//! Values are trimmed; the first record of a chain has an empty `Huella=`.
//! An anulación hashes `IDEmisorFacturaAnulada`, `NumSerieFacturaAnulada`,
//! `FechaExpedicionFacturaAnulada`, `Huella` and `FechaHoraHusoGenRegistro`.
//! The values must be written exactly as in the record's XML; KhataOS
//! writes amounts with two decimals in both.
//!
//! **The chain** is per issuer (NIF) and per installation: every record,
//! alta or anulación, carries the previous record's id and huella.

use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The kind of invoice (TipoFactura, Orden HAC/1177/2024 list L2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoFactura {
    /// Factura (art. 6, 7.2 y 7.3 del RD 1619/2012).
    F1,
    /// Factura simplificada y facturas sin identificación del destinatario (art. 6.1.d).
    F2,
    /// Factura emitida en sustitución de facturas simplificadas facturadas y declaradas.
    F3,
    /// Factura rectificativa (error fundado en derecho y art. 80 Uno, Dos y Seis LIVA).
    R1,
    /// Factura rectificativa (art. 80.3).
    R2,
    /// Factura rectificativa (art. 80.4).
    R3,
    /// Factura rectificativa (resto).
    R4,
    /// Factura rectificativa en facturas simplificadas.
    R5,
}

impl TipoFactura {
    pub fn code(self) -> &'static str {
        match self {
            TipoFactura::F1 => "F1",
            TipoFactura::F2 => "F2",
            TipoFactura::F3 => "F3",
            TipoFactura::R1 => "R1",
            TipoFactura::R2 => "R2",
            TipoFactura::R3 => "R3",
            TipoFactura::R4 => "R4",
            TipoFactura::R5 => "R5",
        }
    }

    pub fn rectificativa(self) -> bool {
        self.code().starts_with('R')
    }
}

/// The id of an invoice in the records: issuer, number (series included)
/// and issue date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdFactura {
    /// The issuer's NIF (IDEmisorFactura).
    pub emisor: String,
    /// NumSerieFactura: series and number, at most 60 characters.
    pub num_serie: String,
    /// FechaExpedicionFactura.
    pub fecha: NaiveDate,
}

/// The previous record of the chain, as the next one quotes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistroAnterior {
    pub id: IdFactura,
    pub huella: String,
}

/// The chain state an installation keeps between records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainState {
    /// Records generated so far (altas and anulaciones).
    pub count: u64,
    /// The last record, `None` before the first.
    pub last: Option<RegistroAnterior>,
    /// When the last record was generated (FechaHoraHusoGenRegistro).
    #[serde(default)]
    pub last_generado: Option<String>,
}

/// One line of the tax breakdown (DetalleDesglose).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detalle {
    /// ClaveRegimen (list L8A): "01" régimen general, "02" exportación…
    pub clave_regimen: String,
    /// CalificacionOperacion (L9: S1 sujeta no exenta sin ISP, S2 con
    /// inversión del sujeto pasivo, N1/N2 no sujeta) or OperacionExenta
    /// (L10: E1 art. 20, E2 art. 21 exportación, E3 art. 22, E4 art. 23–24,
    /// E5 art. 25 entregas intracomunitarias, E6 otros).
    pub calificacion: String,
    /// TipoImpositivo, percent; `None` on exempt and non-subject lines.
    pub tipo: Option<Decimal>,
    /// BaseImponibleOimporteNoSujeto.
    pub base: Decimal,
    /// CuotaRepercutida; `None` on exempt and non-subject lines.
    pub cuota: Option<Decimal>,
}

impl Detalle {
    /// Whether the calificación is an exemption code (E1–E6).
    pub fn exenta(&self) -> bool {
        self.calificacion.starts_with('E')
    }
}

/// The billing software (SistemaInformatico).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SistemaInformatico {
    /// The producer's name and NIF.
    pub nombre_razon: String,
    pub nif: String,
    pub nombre_sistema: String,
    /// IdSistemaInformatico: two characters.
    pub id_sistema: String,
    pub version: String,
    /// NumeroInstalacion.
    pub numero_instalacion: String,
    /// The software can only be used in VERI*FACTU mode.
    pub solo_verifactu: bool,
    /// It can serve several taxpayers (obligados tributarios).
    pub multi_ot: bool,
    /// It is serving several now.
    pub multiples_ot: bool,
}

/// What a registro de alta says about an invoice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alta {
    pub id: IdFactura,
    /// NombreRazonEmisor.
    pub nombre_emisor: String,
    pub tipo: TipoFactura,
    /// DescripcionOperacion (at most 500 characters).
    pub descripcion: String,
    /// The buyer (name, NIF); `None` on a simplified invoice.
    pub destinatario: Option<(String, String)>,
    /// For a rectificativa: "I" (por diferencias) or "S" (por sustitución),
    /// and the invoices it corrects.
    pub rectificada: Option<(String, Vec<IdFactura>)>,
    pub desglose: Vec<Detalle>,
    pub cuota_total: Decimal,
    pub importe_total: Decimal,
}

/// A generated record: its XML, huella and place in the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registro {
    /// "alta" or "anulacion".
    pub kind: String,
    pub id: IdFactura,
    /// The record quoted as RegistroAnterior; `None` for the first record.
    pub anterior: Option<RegistroAnterior>,
    /// FechaHoraHusoGenRegistro as written.
    pub generado: String,
    /// The string hashed.
    pub cadena: String,
    pub huella: String,
    pub xml: String,
    /// Position in the chain, from 1.
    pub numero: u64,
}

/// "dd-mm-yyyy", the record's date format.
pub fn fecha(d: NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

/// "yyyy-mm-ddThh:mm:ss+hh:mm" (ISO 8601 with the offset), to the second.
pub fn fecha_hora(t: DateTime<FixedOffset>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// An amount with two decimals, "." as the separator ("123.45", "-5.00").
pub fn importe(a: Decimal) -> String {
    let r = a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    format!("{r:.2}")
}

/// A percent as the record writes it ("21.00", "5.50").
pub fn tipo(p: Decimal) -> String {
    importe(p)
}

/// SHA-256 of a string, as 64 upper-case hexadecimal characters.
pub fn sha256_upper(s: &str) -> String {
    hex::encode_upper(Sha256::digest(s.as_bytes()))
}

/// The string a registro de alta's huella is computed over.
pub fn cadena_alta(id: &IdFactura, tipo: TipoFactura, cuota_total: &str, importe_total: &str, huella_anterior: &str, generado: &str) -> String {
    format!(
        "IDEmisorFactura={}&NumSerieFactura={}&FechaExpedicionFactura={}&TipoFactura={}&CuotaTotal={}&ImporteTotal={}&Huella={}&FechaHoraHusoGenRegistro={}",
        id.emisor.trim(),
        id.num_serie.trim(),
        fecha(id.fecha),
        tipo.code(),
        cuota_total.trim(),
        importe_total.trim(),
        huella_anterior.trim(),
        generado.trim()
    )
}

/// The string a registro de anulación's huella is computed over.
pub fn cadena_anulacion(id: &IdFactura, huella_anterior: &str, generado: &str) -> String {
    format!(
        "IDEmisorFacturaAnulada={}&NumSerieFacturaAnulada={}&FechaExpedicionFacturaAnulada={}&Huella={}&FechaHoraHusoGenRegistro={}",
        id.emisor.trim(),
        id.num_serie.trim(),
        fecha(id.fecha),
        huella_anterior.trim(),
        generado.trim()
    )
}

/// Why a record cannot be generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    Missing(&'static str),
    TooLong { field: &'static str, max: usize },
    /// CuotaTotal is not the sum of the breakdown's cuotas, or ImporteTotal
    /// not bases + cuotas.
    Totals(String),
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordError::Missing(field) => write!(f, "the record needs {field}"),
            RecordError::TooLong { field, max } => write!(f, "{field} may have at most {max} characters"),
            RecordError::Totals(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for RecordError {}

fn check(a: &Alta) -> Result<(), RecordError> {
    for (field, value, max) in [
        ("IDEmisorFactura (the issuer's NIF)", a.id.emisor.as_str(), 9),
        ("NumSerieFactura", a.id.num_serie.as_str(), 60),
        ("NombreRazonEmisor", a.nombre_emisor.as_str(), 120),
        ("DescripcionOperacion", a.descripcion.as_str(), 500),
    ] {
        if value.trim().is_empty() {
            return Err(RecordError::Missing(field));
        }
        if value.trim().chars().count() > max {
            return Err(RecordError::TooLong { field, max });
        }
    }
    if a.desglose.is_empty() {
        return Err(RecordError::Missing("a tax breakdown (Desglose)"));
    }
    if a.desglose.len() > 12 {
        return Err(RecordError::TooLong { field: "Desglose (DetalleDesglose lines)", max: 12 });
    }
    let cuotas: Decimal = a.desglose.iter().filter_map(|d| d.cuota).sum();
    let bases: Decimal = a.desglose.iter().map(|d| d.base).sum();
    if (cuotas - a.cuota_total).abs() > Decimal::new(1, 2) {
        return Err(RecordError::Totals(format!("CuotaTotal {} is not the sum of the breakdown's cuotas {}", importe(a.cuota_total), importe(cuotas))));
    }
    if (bases + cuotas - a.importe_total).abs() > Decimal::new(1, 2) {
        return Err(RecordError::Totals(format!("ImporteTotal {} is not bases + cuotas {}", importe(a.importe_total), importe(bases + cuotas))));
    }
    Ok(())
}

/// The AEAT's SuministroInformacion namespace (prefix `sum1`).
pub const NS_SUM1: &str =
    "https://www2.agenciatributaria.gob.es/static_files/common/internet/dep/aplicaciones/es/aeat/tike/cont/ws/SuministroInformacion.xsd";

/// The record layout version (IDVersion).
pub const ID_VERSION: &str = "1.0";

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.trim().chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn el(x: &mut String, indent: usize, name: &str, value: &str) {
    x.push_str(&format!("{}<sum1:{name}>{}</sum1:{name}>\n", "  ".repeat(indent), esc(value)));
}

fn open(x: &mut String, indent: usize, name: &str) {
    x.push_str(&format!("{}<sum1:{name}>\n", "  ".repeat(indent)));
}

fn close(x: &mut String, indent: usize, name: &str) {
    x.push_str(&format!("{}</sum1:{name}>\n", "  ".repeat(indent)));
}

fn id_xml(x: &mut String, indent: usize, id: &IdFactura, suffix: &str) {
    el(x, indent, &format!("IDEmisorFactura{suffix}"), &id.emisor);
    el(x, indent, &format!("NumSerieFactura{suffix}"), &id.num_serie);
    el(x, indent, &format!("FechaExpedicionFactura{suffix}"), &fecha(id.fecha));
}

fn chain_xml(x: &mut String, anterior: Option<&RegistroAnterior>) {
    open(x, 1, "Encadenamiento");
    match anterior {
        None => el(x, 2, "PrimerRegistro", "S"),
        Some(a) => {
            open(x, 2, "RegistroAnterior");
            id_xml(x, 3, &a.id, "");
            el(x, 3, "Huella", &a.huella);
            close(x, 2, "RegistroAnterior");
        }
    }
    close(x, 1, "Encadenamiento");
}

fn sistema_xml(x: &mut String, s: &SistemaInformatico) {
    let sn = |b: bool| if b { "S" } else { "N" };
    open(x, 1, "SistemaInformatico");
    el(x, 2, "NombreRazon", &s.nombre_razon);
    el(x, 2, "NIF", &s.nif);
    el(x, 2, "NombreSistemaInformatico", &s.nombre_sistema);
    el(x, 2, "IdSistemaInformatico", &s.id_sistema);
    el(x, 2, "Version", &s.version);
    el(x, 2, "NumeroInstalacion", &s.numero_instalacion);
    el(x, 2, "TipoUsoPosibleSoloVerifactu", sn(s.solo_verifactu));
    el(x, 2, "TipoUsoPosibleMultiOT", sn(s.multi_ot));
    el(x, 2, "IndicadorMultiplesOT", sn(s.multiples_ot));
    close(x, 1, "SistemaInformatico");
}

fn alta_xml(a: &Alta, anterior: Option<&RegistroAnterior>, s: &SistemaInformatico, generado: &str, huella: &str) -> String {
    let mut x = format!("<sum1:RegistroAlta xmlns:sum1=\"{NS_SUM1}\">\n");
    el(&mut x, 1, "IDVersion", ID_VERSION);
    open(&mut x, 1, "IDFactura");
    id_xml(&mut x, 2, &a.id, "");
    close(&mut x, 1, "IDFactura");
    el(&mut x, 1, "NombreRazonEmisor", &a.nombre_emisor);
    el(&mut x, 1, "TipoFactura", a.tipo.code());
    if let Some((kind, ids)) = &a.rectificada {
        el(&mut x, 1, "TipoRectificativa", kind);
        open(&mut x, 1, "FacturasRectificadas");
        for id in ids {
            open(&mut x, 2, "IDFacturaRectificada");
            id_xml(&mut x, 3, id, "");
            close(&mut x, 2, "IDFacturaRectificada");
        }
        close(&mut x, 1, "FacturasRectificadas");
    }
    el(&mut x, 1, "DescripcionOperacion", &a.descripcion);
    if let Some((name, nif)) = &a.destinatario {
        open(&mut x, 1, "Destinatarios");
        open(&mut x, 2, "IDDestinatario");
        el(&mut x, 3, "NombreRazon", name);
        el(&mut x, 3, "NIF", nif);
        close(&mut x, 2, "IDDestinatario");
        close(&mut x, 1, "Destinatarios");
    }
    open(&mut x, 1, "Desglose");
    for d in &a.desglose {
        open(&mut x, 2, "DetalleDesglose");
        el(&mut x, 3, "Impuesto", "01");
        el(&mut x, 3, "ClaveRegimen", &d.clave_regimen);
        el(&mut x, 3, if d.exenta() { "OperacionExenta" } else { "CalificacionOperacion" }, &d.calificacion);
        if let Some(t) = d.tipo {
            el(&mut x, 3, "TipoImpositivo", &tipo(t));
        }
        el(&mut x, 3, "BaseImponibleOimporteNoSujeto", &importe(d.base));
        if let Some(c) = d.cuota {
            el(&mut x, 3, "CuotaRepercutida", &importe(c));
        }
        close(&mut x, 2, "DetalleDesglose");
    }
    close(&mut x, 1, "Desglose");
    el(&mut x, 1, "CuotaTotal", &importe(a.cuota_total));
    el(&mut x, 1, "ImporteTotal", &importe(a.importe_total));
    chain_xml(&mut x, anterior);
    sistema_xml(&mut x, s);
    el(&mut x, 1, "FechaHoraHusoGenRegistro", generado);
    el(&mut x, 1, "TipoHuella", "01");
    el(&mut x, 1, "Huella", huella);
    x.push_str("</sum1:RegistroAlta>\n");
    x
}

/// The AEAT checks before each record that the previous one is not dated
/// more than a minute after it (Orden HAC/1177/2024 art. 7.i, research §6.8):
/// a warning when the installation's clock has gone back. Invoicing does
/// not stop for it.
pub fn chain_time_warning(chain: &ChainState, now: DateTime<FixedOffset>) -> Option<String> {
    let last = DateTime::parse_from_rfc3339(chain.last_generado.as_deref()?).ok()?;
    (last > now + chrono::Duration::seconds(60)).then(|| {
        format!("the previous VeriFactu record was generated at {last}, more than a minute after this one ({}); check the computer's clock", fecha_hora(now))
    })
}

/// Generate the registro de alta for an invoice as the chain's next record,
/// generated at `now` (the installation's local time with its offset), and
/// move the chain on.
pub fn alta(a: &Alta, chain: &mut ChainState, sistema: &SistemaInformatico, now: DateTime<FixedOffset>) -> Result<Registro, RecordError> {
    check(a)?;
    let generado = fecha_hora(now);
    let anterior = chain.last.clone();
    let prev = anterior.as_ref().map_or("", |p| p.huella.as_str());
    let cadena = cadena_alta(&a.id, a.tipo, &importe(a.cuota_total), &importe(a.importe_total), prev, &generado);
    let huella = sha256_upper(&cadena);
    let xml = alta_xml(a, anterior.as_ref(), sistema, &generado, &huella);
    chain.count += 1;
    chain.last = Some(RegistroAnterior { id: a.id.clone(), huella: huella.clone() });
    chain.last_generado = Some(generado.clone());
    Ok(Registro { kind: "alta".into(), id: a.id.clone(), anterior, generado, cadena, huella, xml, numero: chain.count })
}

/// Generate the registro de anulación of an invoice already recorded, as
/// the chain's next record, and move the chain on.
pub fn anulacion(id: &IdFactura, chain: &mut ChainState, sistema: &SistemaInformatico, now: DateTime<FixedOffset>) -> Result<Registro, RecordError> {
    if id.emisor.trim().is_empty() {
        return Err(RecordError::Missing("IDEmisorFacturaAnulada (the issuer's NIF)"));
    }
    let generado = fecha_hora(now);
    let anterior = chain.last.clone();
    let prev = anterior.as_ref().map_or("", |p| p.huella.as_str());
    let cadena = cadena_anulacion(id, prev, &generado);
    let huella = sha256_upper(&cadena);
    let mut x = format!("<sum1:RegistroAnulacion xmlns:sum1=\"{NS_SUM1}\">\n");
    el(&mut x, 1, "IDVersion", ID_VERSION);
    open(&mut x, 1, "IDFactura");
    id_xml(&mut x, 2, id, "Anulada");
    close(&mut x, 1, "IDFactura");
    chain_xml(&mut x, anterior.as_ref());
    sistema_xml(&mut x, sistema);
    el(&mut x, 1, "FechaHoraHusoGenRegistro", &generado);
    el(&mut x, 1, "TipoHuella", "01");
    el(&mut x, 1, "Huella", &huella);
    x.push_str("</sum1:RegistroAnulacion>\n");
    chain.count += 1;
    chain.last = Some(RegistroAnterior { id: id.clone(), huella: huella.clone() });
    chain.last_generado = Some(generado.clone());
    Ok(Registro { kind: "anulacion".into(), id: id.clone(), anterior, generado, cadena, huella, xml: x, numero: chain.count })
}

/// Which AEAT host the QR code points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entorno {
    Produccion,
    /// The AEAT's test environment (preproducción).
    Pruebas,
}

/// The cotejo service's base URL: VERI*FACTU or not, production or test
/// (AEAT, *Características del QR y especificaciones del servicio de
/// cotejo*, research §6.9).
pub fn qr_base(entorno: Entorno, verifactu: bool) -> &'static str {
    match (entorno, verifactu) {
        (Entorno::Produccion, true) => "https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR",
        (Entorno::Produccion, false) => "https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQRNoVerifactu",
        (Entorno::Pruebas, true) => "https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQR",
        (Entorno::Pruebas, false) => "https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQRNoVerifactu",
    }
}

/// Encode a query value as Java's `URLEncoder.encode(value, "UTF-8")`
/// does, which the AEAT's QR specification names: letters, digits and
/// `.-*_` are kept, a space becomes `+`, every other byte `%XX`.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'*' | b'_') => out.push(b as char),
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The QR code's URL: `nif`, `numserie`, `fecha` (dd-mm-yyyy) and
/// `importe` (ImporteTotal), each URL-encoded.
pub fn qr_url(entorno: Entorno, verifactu: bool, id: &IdFactura, importe_total: Decimal) -> String {
    format!(
        "{}?nif={}&numserie={}&fecha={}&importe={}",
        qr_base(entorno, verifactu),
        url_encode(id.emisor.trim()),
        url_encode(id.num_serie.trim()),
        url_encode(&fecha(id.fecha)),
        url_encode(&importe(importe_total))
    )
}

/// The legend printed with the QR code on a VERI*FACTU invoice.
pub const QR_LEGEND: &str = "VERI*FACTU";

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn id(num: &str) -> IdFactura {
        IdFactura { emisor: "89890001K".into(), num_serie: num.into(), fecha: d(2024, 1, 1) }
    }

    /// The AEAT's worked examples (huella document, research §6.7): a first
    /// alta, a second alta chained to it, and an anulación of the second.
    #[test]
    fn the_aeat_golden_examples() {
        let first = cadena_alta(&id("12345678/G33"), TipoFactura::F1, "12.35", "123.45", "", "2024-01-01T19:20:30+01:00");
        assert_eq!(
            first,
            "IDEmisorFactura=89890001K&NumSerieFactura=12345678/G33&FechaExpedicionFactura=01-01-2024&TipoFactura=F1&CuotaTotal=12.35&ImporteTotal=123.45&Huella=&FechaHoraHusoGenRegistro=2024-01-01T19:20:30+01:00"
        );
        let h1 = sha256_upper(&first);
        assert_eq!(h1, "3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60");
        let second = cadena_alta(&id("12345679/G34"), TipoFactura::F1, "12.35", "123.45", &h1, "2024-01-01T19:20:35+01:00");
        let h2 = sha256_upper(&second);
        assert_eq!(h2, "F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97");
        let cancel = cadena_anulacion(&id("12345679/G34"), &h2, "2024-01-01T19:20:40+01:00");
        assert_eq!(
            cancel,
            "IDEmisorFacturaAnulada=89890001K&NumSerieFacturaAnulada=12345679/G34&FechaExpedicionFacturaAnulada=01-01-2024&Huella=F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97&FechaHoraHusoGenRegistro=2024-01-01T19:20:40+01:00"
        );
        assert_eq!(sha256_upper(&cancel), "177547C0D57AC74748561D054A9CEC14B4C4EA23D1BEFD6F2E69E3A388F90C68");
    }

    fn invoice(num: &str) -> Alta {
        Alta {
            id: id(num),
            nombre_emisor: "Empresa de Pruebas SL".into(),
            tipo: TipoFactura::F1,
            descripcion: "Venta de mercaderías".into(),
            destinatario: Some(("Cliente & Hijos SA".into(), "B58378431".into())),
            rectificada: None,
            desglose: vec![Detalle {
                clave_regimen: "01".into(),
                calificacion: "S1".into(),
                tipo: Some(dec!(10.00)),
                base: dec!(111.10),
                cuota: Some(dec!(12.35)),
            }],
            cuota_total: dec!(12.35),
            importe_total: dec!(123.45),
        }
    }

    #[test]
    fn records_chain_and_reproduce_the_golden_hashes() {
        let sistema = SistemaInformatico { nombre_razon: "KhataOS".into(), id_sistema: "KO".into(), ..SistemaInformatico::default() };
        let mut chain = ChainState::default();
        let a = alta(&invoice("12345678/G33"), &mut chain, &sistema, at("2024-01-01T19:20:30+01:00")).unwrap();
        assert_eq!((a.numero, a.anterior.as_ref()), (1, None));
        assert_eq!(a.huella, "3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60");
        assert!(a.xml.contains("<sum1:PrimerRegistro>S</sum1:PrimerRegistro>"), "{}", a.xml);
        assert!(a.xml.contains("<sum1:NombreRazon>Cliente &amp; Hijos SA</sum1:NombreRazon>"));
        assert!(a.xml.contains("<sum1:FechaExpedicionFactura>01-01-2024</sum1:FechaExpedicionFactura>"));
        assert!(a.xml.contains("<sum1:TipoImpositivo>10.00</sum1:TipoImpositivo>"));
        assert!(a.xml.contains(&format!("<sum1:Huella>{}</sum1:Huella>", a.huella)));
        let b = alta(&invoice("12345679/G34"), &mut chain, &sistema, at("2024-01-01T19:20:35+01:00")).unwrap();
        assert_eq!(b.huella, "F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97");
        assert_eq!(b.anterior.as_ref().map(|p| p.huella.as_str()), Some(a.huella.as_str()));
        assert!(b.xml.contains("<sum1:NumSerieFactura>12345678/G33</sum1:NumSerieFactura>"), "the previous record is quoted");
        let c = anulacion(&id("12345679/G34"), &mut chain, &sistema, at("2024-01-01T19:20:40+01:00")).unwrap();
        assert_eq!(c.huella, "177547C0D57AC74748561D054A9CEC14B4C4EA23D1BEFD6F2E69E3A388F90C68");
        assert!(c.xml.contains("<sum1:NumSerieFacturaAnulada>12345679/G34</sum1:NumSerieFacturaAnulada>"));
        assert_eq!((chain.count, chain.last.as_ref().map(|l| l.huella.clone())), (3, Some(c.huella.clone())));
        assert_eq!(chain_time_warning(&chain, at("2024-01-01T19:20:00+01:00")), None, "40 s earlier is tolerated");
        assert!(chain_time_warning(&chain, at("2024-01-01T19:19:00+01:00")).unwrap().contains("clock"));
        assert_eq!(chain_time_warning(&ChainState::default(), at("2024-01-01T19:19:00+01:00")), None);
        let j = serde_json::to_string(&chain).unwrap();
        assert_eq!(serde_json::from_str::<ChainState>(&j).unwrap(), chain, "the chain state round-trips");
    }

    #[test]
    fn records_are_checked() {
        let s = SistemaInformatico::default();
        let mut chain = ChainState::default();
        let now = at("2026-07-01T10:00:00+02:00");
        let mut bad = invoice("A-1");
        bad.cuota_total = dec!(12.00);
        assert!(matches!(alta(&bad, &mut chain, &s, now), Err(RecordError::Totals(_))));
        let mut bad = invoice("A-1");
        bad.id.num_serie = "9".repeat(61);
        assert_eq!(alta(&bad, &mut chain, &s, now), Err(RecordError::TooLong { field: "NumSerieFactura", max: 60 }));
        let mut bad = invoice("A-1");
        bad.descripcion = " ".into();
        assert_eq!(alta(&bad, &mut chain, &s, now), Err(RecordError::Missing("DescripcionOperacion")));
        assert_eq!(chain, ChainState::default(), "a refused record does not move the chain");
    }

    #[test]
    fn formats_and_the_qr_url() {
        assert_eq!(fecha(d(2026, 7, 9)), "09-07-2026");
        assert_eq!(fecha_hora(at("2026-07-09T08:05:03+02:00")), "2026-07-09T08:05:03+02:00");
        assert_eq!((importe(dec!(241.4)), importe(dec!(-5)), importe(dec!(0.005))), ("241.40".into(), "-5.00".into(), "0.01".into()));
        let q = qr_url(Entorno::Produccion, true, &IdFactura { emisor: "89890001K".into(), num_serie: "12345678&G33".into(), fecha: d(2024, 9, 1) }, dec!(241.4));
        assert_eq!(q, "https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR?nif=89890001K&numserie=12345678%26G33&fecha=01-09-2024&importe=241.40");
        assert!(qr_url(Entorno::Pruebas, false, &id("A/1"), dec!(1)).starts_with("https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQRNoVerifactu?nif=89890001K&numserie=A%2F1&"));
        assert_eq!(url_encode("a b+ñ*~"), "a+b%2B%C3%B1*%7E");
    }
}

//! CFDI 4.0 (Anexo 20) XML, its cadena original and the CSD sello.
//!
//! Sources: `docs/research/mexico-compliance.md` §4 to §7 (Anexo 20
//! versión 4.0, its XSD `cfdv40.xsd`, the XSLT `cadenaoriginal_4_0.xslt`
//! and the Anexo 20 filling guide).
//!
//! What is generated, from a neutral [`Comprobante`]:
//! - `cfdi:Comprobante` with the `cfdi` and `xsi` namespaces and the
//!   schema location, `Version="4.0"`, Serie, Folio, Fecha (local time, no
//!   zone), FormaPago, NoCertificado, Certificado, SubTotal, Descuento,
//!   Moneda, TipoCambio, Total, TipoDeComprobante (`I` ingreso / `E`
//!   egreso), Exportacion, MetodoPago (`PUE` / `PPD`), LugarExpedicion and
//!   Sello;
//! - `cfdi:CfdiRelacionados` (for an egreso: the related UUIDs), Emisor,
//!   Receptor, Conceptos (each with its IVA Traslado and its ISR/IVA
//!   Retenciones, see [`crate::retenciones`]) and the comprobante's
//!   Impuestos totals (Retenciones, TotalImpuestosRetenidos, Traslados,
//!   TotalImpuestosTrasladados), in the XSD's element order.
//!
//! The Recibo Electrónico de Pago (TipoDeComprobante `P` with the
//! Complemento de Pagos 2.0) is built in [`crate::pagos`] with the same
//! node writer.
//!
//! The **cadena original** is built here directly, not by running the
//! SAT's XSLT: it follows the XSLT's rules for exactly the nodes and
//! attributes this module writes (`||` at both ends, `|` between values,
//! optional attributes skipped when absent, whitespace normalised as
//! XPath `normalize-space`). It was checked during development against
//! the SAT's own `cadenaoriginal_4_0.xslt` (xsltproc) and `cfdv40.xsd`
//! (xmllint) on the test documents here, byte for byte, and the
//! research document's xsltproc golden cadena is a test
//! ([HIGH] for these nodes; not yet validated by a PAC). Anything outside
//! these nodes (InformacionGlobal, complementos other than Pagos 2.0,
//! terceros, información aduanera…) is not produced, so is not in the
//! cadena.
//!
//! The **sello** is the RSA PKCS#1 v1.5 signature with SHA-256 of the
//! cadena's UTF-8 bytes, in base64, made with the issuer's CSD private
//! key ([`Csd`]); NoCertificado is the CSD certificate's serial number
//! read as ASCII digits, and Certificado the certificate in base64 DER.
//!
//! **Not a valid CFDI yet.** A CFDI only exists for the SAT once a PAC
//! (proveedor autorizado de certificación) validates it and adds the
//! TimbreFiscalDigital complement with the folio fiscal (UUID). The XML
//! here is what a PAC receives; [`crate::timbre`] reads the PAC's answer
//! and the desktop app's PAC connector sends it.

use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::NaiveDateTime;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x509_cert::Certificate;
use x509_cert::der::{Decode, Encode};

/// The CFDI 4.0 namespace.
pub const NS_CFDI: &str = "http://www.sat.gob.mx/cfd/4";
/// XML Schema instance.
pub const NS_XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
/// `xsi:schemaLocation`.
pub const SCHEMA_LOCATION: &str = "http://www.sat.gob.mx/cfd/4 http://www.sat.gob.mx/sitio_internet/cfd/4/cfdv40.xsd";

/// IVA's code in c_Impuesto.
pub const IMPUESTO_IVA: &str = "002";
/// ISR's code in c_Impuesto.
pub const IMPUESTO_ISR: &str = "001";

/// A tax that can be withheld (c_Impuesto; IEPS is not modelled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Impuesto {
    /// `001`.
    Isr,
    /// `002`.
    Iva,
}

impl Impuesto {
    pub fn code(self) -> &'static str {
        match self {
            Impuesto::Isr => IMPUESTO_ISR,
            Impuesto::Iva => IMPUESTO_IVA,
        }
    }

    /// The highest TasaOCuota c_TasaOCuota allows for a retención: ISR
    /// 0.350000, IVA 0.160000 (ranges) [HIGH, catalogue].
    pub fn max_retention(self) -> Decimal {
        match self {
            Impuesto::Isr => Decimal::new(35, 2),
            Impuesto::Iva => Decimal::new(16, 2),
        }
    }
}

/// A withholding on a concepto: TipoFactor `Tasa` at `tasa` (a fraction
/// with six decimals, e.g. 0.106667), on the concepto's base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retencion {
    pub impuesto: Impuesto,
    pub tasa: Decimal,
}

/// TipoDeComprobante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoDeComprobante {
    /// `I`: an invoice.
    Ingreso,
    /// `E`: a credit note (nota de crédito).
    Egreso,
    /// `P`: a payment receipt with the Complemento de Pagos 2.0, made by
    /// [`crate::pagos::generate`], never by [`generate`].
    Pago,
}

impl TipoDeComprobante {
    pub fn code(self) -> &'static str {
        match self {
            TipoDeComprobante::Ingreso => "I",
            TipoDeComprobante::Egreso => "E",
            TipoDeComprobante::Pago => "P",
        }
    }
}

/// MetodoPago (c_MetodoPago).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetodoPago {
    /// Pago en una sola exhibición: paid in full at or before issue.
    Pue,
    /// Pago en parcialidades o diferido: FormaPago must be `99` and each
    /// payment needs a Complemento de Pagos (not generated).
    Ppd,
}

impl MetodoPago {
    pub fn code(self) -> &'static str {
        match self {
            MetodoPago::Pue => "PUE",
            MetodoPago::Ppd => "PPD",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emisor {
    pub rfc: String,
    pub nombre: String,
    /// c_RegimenFiscal ("601").
    pub regimen_fiscal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receptor {
    pub rfc: String,
    pub nombre: String,
    /// The receptor's postal code (DomicilioFiscalReceptor).
    pub domicilio_fiscal: String,
    /// c_RegimenFiscal of the receptor.
    pub regimen_fiscal: String,
    /// c_UsoCFDI ("G03", "S01"…).
    pub uso_cfdi: String,
}

/// A concept's IVA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Iva {
    /// TipoFactor Tasa at a rate (16, 8 or 0 percent).
    Tasa(Decimal),
    /// TipoFactor Exento: no rate, no amount.
    Exento,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Concepto {
    /// c_ClaveProdServ ("01010101" when none fits).
    pub clave_prod_serv: String,
    pub no_identificacion: Option<String>,
    pub cantidad: Decimal,
    /// c_ClaveUnidad ("H87" pieza, "E48" unidad de servicio).
    pub clave_unidad: String,
    pub unidad: Option<String>,
    pub descripcion: String,
    pub valor_unitario: Decimal,
    /// Cantidad × ValorUnitario, before discount.
    pub importe: Decimal,
    pub descuento: Option<Decimal>,
    /// `None` and no retenciones: ObjetoImp `01` (not subject to tax);
    /// otherwise `02`.
    pub iva: Option<Iva>,
    /// Taxes the customer withholds on this concepto (ISR, IVA).
    #[serde(default)]
    pub retenciones: Vec<Retencion>,
}

impl Concepto {
    /// The IVA base: importe − descuento.
    pub fn base(&self) -> Decimal {
        self.importe - self.descuento.unwrap_or_default()
    }

    /// The IVA amount, rounded to the centavo (`None` for exento or not
    /// subject).
    pub fn iva_importe(&self) -> Option<Decimal> {
        match self.iva {
            Some(Iva::Tasa(p)) => Some(money(self.base() * p / Decimal::ONE_HUNDRED)),
            _ => None,
        }
    }

    /// Each retención with its Importe: the base × TasaOCuota, rounded to
    /// the centavo (the value a PAC checks against its tolerance).
    pub fn retencion_importes(&self) -> Vec<(Retencion, Decimal)> {
        self.retenciones.iter().map(|r| (*r, money(money(self.base()) * r.tasa))).collect()
    }

    /// ObjetoImp: `02` when the concepto carries any tax, else `01`.
    pub fn objeto_imp(&self) -> &'static str {
        if self.iva.is_some() || !self.retenciones.is_empty() { "02" } else { "01" }
    }
}

/// A comprobante before sealing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comprobante {
    pub serie: Option<String>,
    pub folio: String,
    pub fecha: NaiveDateTime,
    /// c_FormaPago ("01" efectivo, "03" transferencia, "99" por definir).
    pub forma_pago: String,
    pub metodo_pago: MetodoPago,
    /// "MXN".
    pub moneda: String,
    pub tipo_cambio: Option<Decimal>,
    pub tipo: TipoDeComprobante,
    /// c_Exportacion ("01" no aplica).
    pub exportacion: String,
    /// The issuer's postal code.
    pub lugar_expedicion: String,
    /// c_TipoRelacion and the related UUIDs ("01" nota de crédito).
    pub relacionados: Option<(String, Vec<String>)>,
    pub emisor: Emisor,
    pub receptor: Receptor,
    pub conceptos: Vec<Concepto>,
}

/// Round to the centavo, half away from zero.
pub fn money(a: Decimal) -> Decimal {
    a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Fixed decimals ("1000.00", "0.160000").
pub(crate) fn fixed(a: Decimal, dp: u32) -> String {
    let mut a = a.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero);
    a.rescale(dp);
    a.to_string()
}

/// Quantities: at most 6 decimals, trailing zeros dropped ("2", "1.5").
fn cantidad(a: Decimal) -> String {
    a.round_dp_with_strategy(6, RoundingStrategy::MidpointAwayFromZero).normalize().to_string()
}

/// Unit values and exchange rates: at most 6 decimals, at least 2.
pub(crate) fn qty(a: Decimal) -> String {
    let a = a.round_dp_with_strategy(6, RoundingStrategy::MidpointAwayFromZero).normalize();
    if a.scale() < 2 { fixed(a, 2) } else { a.to_string() }
}

/// The rate as TasaOCuota: a fraction with six decimals (16 → "0.160000").
pub fn tasa(percent: Decimal) -> String {
    fixed(percent / Decimal::ONE_HUNDRED, 6)
}

/// A fraction as TasaOCuota (0.1066666 → "0.106667").
pub fn tasa_fraction(fraction: Decimal) -> String {
    fixed(fraction, 6)
}

/// Comprobante-level totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Totales {
    pub sub_total: Decimal,
    pub descuento: Option<Decimal>,
    /// (TipoFactor Tasa rate in percent, base, importe), by rate.
    pub traslados_tasa: Vec<(Decimal, Decimal, Decimal)>,
    /// Base of exento lines, when there are any.
    pub base_exenta: Option<Decimal>,
    /// Sum of the Tasa traslados; `None` when there are only exento ones.
    pub total_traslados: Option<Decimal>,
    /// Withheld taxes by impuesto (ISR first, then IVA).
    pub retenciones: Vec<(Impuesto, Decimal)>,
    /// Sum of the retenciones; `None` when there are none.
    pub total_retenciones: Option<Decimal>,
    /// SubTotal − Descuento + traslados − retenciones.
    pub total: Decimal,
}

impl Comprobante {
    pub fn totales(&self) -> Totales {
        let sub_total: Decimal = self.conceptos.iter().map(|c| money(c.importe)).sum();
        let desc: Decimal = self.conceptos.iter().map(|c| money(c.descuento.unwrap_or_default())).sum();
        let mut tasas: Vec<(Decimal, Decimal, Decimal)> = Vec::new();
        let mut exenta: Option<Decimal> = None;
        for c in &self.conceptos {
            match c.iva {
                Some(Iva::Tasa(p)) => {
                    let imp = c.iva_importe().unwrap_or_default();
                    match tasas.iter_mut().find(|t| t.0 == p) {
                        Some(t) => {
                            t.1 += money(c.base());
                            t.2 += imp;
                        }
                        None => tasas.push((p, money(c.base()), imp)),
                    }
                }
                Some(Iva::Exento) => *exenta.get_or_insert(Decimal::ZERO) += money(c.base()),
                None => {}
            }
        }
        let total_traslados = (!tasas.is_empty()).then(|| tasas.iter().map(|t| t.2).sum::<Decimal>());
        let mut retenciones: Vec<(Impuesto, Decimal)> = Vec::new();
        for (r, importe) in self.conceptos.iter().flat_map(Concepto::retencion_importes) {
            match retenciones.iter_mut().find(|x| x.0 == r.impuesto) {
                Some(x) => x.1 += importe,
                None => retenciones.push((r.impuesto, importe)),
            }
        }
        retenciones.sort_by_key(|r| r.0);
        let total_retenciones = (!retenciones.is_empty()).then(|| retenciones.iter().map(|r| r.1).sum::<Decimal>());
        let total = sub_total - desc + total_traslados.unwrap_or_default() - total_retenciones.unwrap_or_default();
        Totales {
            sub_total,
            descuento: self.conceptos.iter().any(|c| c.descuento.is_some()).then_some(desc),
            traslados_tasa: tasas,
            base_exenta: exenta,
            total_traslados,
            retenciones,
            total_retenciones,
            total,
        }
    }

    /// Problems the SAT would reject (checked before generating).
    pub fn check(&self) -> Result<(), String> {
        if self.conceptos.is_empty() {
            return Err("a CFDI needs at least one concepto".into());
        }
        if self.metodo_pago == MetodoPago::Ppd && self.forma_pago != "99" {
            return Err("with MetodoPago PPD, FormaPago must be 99 (por definir)".into());
        }
        if self.moneda != "MXN" && self.tipo_cambio.is_none() {
            return Err(format!("Moneda {} needs a TipoCambio", self.moneda));
        }
        for (what, cp) in [("LugarExpedicion", &self.lugar_expedicion), ("DomicilioFiscalReceptor", &self.receptor.domicilio_fiscal)] {
            if cp.len() != 5 || !cp.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("{what} must be a 5-digit postal code, not {cp:?}"));
            }
        }
        if self.tipo == TipoDeComprobante::Pago {
            return Err("a payment receipt (P) is made with khata_mx::pagos".into());
        }
        for c in &self.conceptos {
            if c.descripcion.trim().is_empty() || c.clave_prod_serv.trim().is_empty() || c.clave_unidad.trim().is_empty() {
                return Err("every concepto needs a Descripcion, ClaveProdServ and ClaveUnidad".into());
            }
            for (r, importe) in c.retencion_importes() {
                if r.tasa < Decimal::ZERO || r.tasa > r.impuesto.max_retention() {
                    return Err(format!("a retención of {} must be between 0 and {}", r.impuesto.code(), tasa_fraction(r.impuesto.max_retention())));
                }
                if r.impuesto == Impuesto::Iva && importe > c.iva_importe().unwrap_or_default() {
                    return Err(format!("{}: the IVA withheld is more than the IVA charged", c.descripcion.trim()));
                }
            }
        }
        Ok(())
    }
}

/// The issuer's CSD: certificate and private key. The key never leaves
/// this value and is never printed (`Debug` shows the certificate number
/// only).
pub struct Csd {
    key: RsaPrivateKey,
    /// The certificate in DER.
    pub certificate: Vec<u8>,
    /// The certificate number (20 digits).
    pub no_certificado: String,
}

impl std::fmt::Debug for Csd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Csd").field("no_certificado", &self.no_certificado).finish_non_exhaustive()
    }
}

/// The certificate number (NoCertificado) of a DER certificate: its
/// serial number's bytes read as ASCII digits (SAT serials are 20 digits
/// stored that way), else the serial in hexadecimal.
pub fn certificate_number(cer: &[u8]) -> Result<String, String> {
    let cert = Certificate::from_der(cer).map_err(|e| format!("the .cer file is not a DER certificate: {e}"))?;
    let serial = cert.tbs_certificate.serial_number.as_bytes();
    Ok(if serial.iter().all(u8::is_ascii_digit) {
        String::from_utf8(serial.to_vec()).expect("ASCII digits")
    } else {
        serial.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02X}");
            s
        })
    })
}

impl Csd {
    /// Load a CSD from the SAT's `.cer` (DER X.509) and `.key` (encrypted
    /// PKCS#8, DER or PEM) files and the key's password. The key must
    /// match the certificate's public key. PBES2 keys (PBKDF2 with
    /// HMAC-SHA1 or SHA-256, and 3DES or AES) open; the legacy PKCS#12
    /// PBE schemes do not [UNCERTAIN which scheme current SAT keys use;
    /// research §6]. The decrypted key is zeroised when dropped.
    pub fn from_der(cer: &[u8], key: &[u8], password: &str) -> Result<Self, String> {
        let cert = Certificate::from_der(cer).map_err(|e| format!("the .cer file is not a DER certificate: {e}"))?;
        let no_certificado = certificate_number(cer)?;
        let opened = match std::str::from_utf8(key) {
            Ok(pem) if pem.trim_start().starts_with("-----BEGIN") => RsaPrivateKey::from_pkcs8_encrypted_pem(pem, password.as_bytes()),
            _ => RsaPrivateKey::from_pkcs8_encrypted_der(key, password.as_bytes()),
        };
        let key = opened
            .map_err(|_| "the .key file could not be opened: wrong password, or not a CSD private key".to_owned())?;
        let spki = cert.tbs_certificate.subject_public_key_info.to_der().map_err(|e| e.to_string())?;
        let public = RsaPublicKey::from_public_key_der(&spki).map_err(|_| "the certificate's key is not RSA".to_owned())?;
        if public.n() != key.n() || public.e() != key.e() {
            return Err("the .key file does not belong to this certificate".into());
        }
        Ok(Self { key, certificate: cer.to_vec(), no_certificado })
    }

    /// Sign a cadena original: RSA PKCS#1 v1.5 with SHA-256, base64.
    pub fn sello(&self, cadena: &str) -> String {
        let signer = SigningKey::<Sha256>::new(self.key.clone());
        B64.encode(signer.sign(cadena.as_bytes()).to_bytes())
    }

    /// The certificate in base64 (the Certificado attribute).
    pub fn certificado_b64(&self) -> String {
        B64.encode(&self.certificate)
    }
}

/// XPath `normalize-space`: trim and collapse runs of whitespace.
pub fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// An element under construction: attributes in the order given, which is
/// also the cadena's order for every node this crate writes.
pub(crate) struct Node {
    prefix: &'static str,
    name: &'static str,
    pub(crate) attrs: Vec<(&'static str, String)>,
}

impl Node {
    pub(crate) fn new(name: &'static str) -> Self {
        Self { prefix: "cfdi", name, attrs: Vec::new() }
    }

    /// An element in another namespace (`pago20`).
    pub(crate) fn ns(prefix: &'static str, name: &'static str) -> Self {
        Self { prefix, name, attrs: Vec::new() }
    }

    pub(crate) fn a(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.attrs.push((name, value.into()));
        self
    }

    pub(crate) fn opt(self, name: &'static str, value: Option<String>) -> Self {
        match value {
            Some(v) => self.a(name, v),
            None => self,
        }
    }

    pub(crate) fn open(&self, xml: &mut String, close: bool) {
        let _ = write!(xml, "<{}:{}", self.prefix, self.name);
        for (k, v) in &self.attrs {
            let _ = write!(xml, " {k}=\"{}\"", esc(v));
        }
        xml.push_str(if close { "/>" } else { ">" });
    }

    pub(crate) fn close(&self, xml: &mut String) {
        let _ = write!(xml, "</{}:{}>", self.prefix, self.name);
    }

    pub(crate) fn cadena(&self, out: &mut Vec<String>, skip: &[&str]) {
        out.extend(self.attrs.iter().filter(|(k, _)| !skip.contains(k)).map(|(_, v)| normalize_space(v)));
    }
}

/// The generated document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Generated {
    pub xml: String,
    pub cadena_original: String,
    /// Empty when generated without a CSD.
    pub sello: String,
    pub no_certificado: String,
    pub totales: Totales,
}

pub(crate) fn opt_s(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// CfdiRelacionados, Emisor and Receptor, in the XSD order, into the
/// cadena and the body.
pub(crate) fn write_parties(
    relacionados: Option<&(String, Vec<String>)>,
    emisor: &Emisor,
    r: &Receptor,
    cadena: &mut Vec<String>,
    body: &mut String,
) {
    if let Some((tipo, uuids)) = relacionados {
        let rel = Node::new("CfdiRelacionados").a("TipoRelacion", tipo.trim());
        rel.cadena(cadena, &[]);
        rel.open(body, false);
        for u in uuids {
            let n = Node::new("CfdiRelacionado").a("UUID", u.trim());
            n.cadena(cadena, &[]);
            n.open(body, true);
        }
        rel.close(body);
    }
    let emisor = Node::new("Emisor").a("Rfc", emisor.rfc.trim()).a("Nombre", emisor.nombre.trim()).a("RegimenFiscal", emisor.regimen_fiscal.trim());
    let receptor = Node::new("Receptor")
        .a("Rfc", r.rfc.trim())
        .a("Nombre", r.nombre.trim())
        .a("DomicilioFiscalReceptor", r.domicilio_fiscal.trim())
        .a("RegimenFiscalReceptor", r.regimen_fiscal.trim())
        .a("UsoCFDI", r.uso_cfdi.trim());
    emisor.cadena(cadena, &[]);
    emisor.open(body, true);
    receptor.cadena(cadena, &[]);
    receptor.open(body, true);
}

/// Seal the cadena and write the document: the root with its namespaces
/// (`extra_ns` adds `xmlns:…` declarations and schema locations), Sello
/// and Certificado, then the body.
pub(crate) fn finish(
    root: Node,
    cadena: Vec<String>,
    body: &str,
    csd: Option<&Csd>,
    extra_ns: &[(&str, &str, &str)],
    totales: Totales,
) -> Generated {
    let cadena_original = format!("||{}||", cadena.join("|"));
    let sello = csd.map(|k| k.sello(&cadena_original)).unwrap_or_default();
    let no_certificado = csd.map(|k| k.no_certificado.clone()).unwrap_or_default();
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut root = root.a("Sello", sello.clone());
    if let Some(k) = csd {
        root = root.a("Certificado", k.certificado_b64());
    }
    let mut schema = SCHEMA_LOCATION.to_owned();
    let _ = write!(xml, "<cfdi:Comprobante xmlns:cfdi=\"{NS_CFDI}\" xmlns:xsi=\"{NS_XSI}\"");
    for (prefix, ns, location) in extra_ns {
        let _ = write!(xml, " xmlns:{prefix}=\"{ns}\"");
        let _ = write!(schema, " {ns} {location}");
    }
    let _ = write!(xml, " xsi:schemaLocation=\"{schema}\"");
    for (k, v) in &root.attrs {
        if *k == "Sello" && v.is_empty() {
            continue;
        }
        let _ = write!(xml, " {k}=\"{}\"", esc(v));
    }
    xml.push('>');
    xml.push_str(body);
    xml.push_str("</cfdi:Comprobante>\n");
    Generated { xml, cadena_original, sello, no_certificado, totales }
}

/// Build the XML, cadena original and sello. Without a CSD, NoCertificado,
/// Certificado and Sello are left out (a draft for checking, which no PAC
/// will accept).
pub fn generate(c: &Comprobante, csd: Option<&Csd>) -> Result<Generated, String> {
    c.check()?;
    let t = c.totales();
    let root = Node::new("Comprobante")
        .a("Version", "4.0")
        .opt("Serie", opt_s(&c.serie))
        .a("Folio", c.folio.trim())
        .a("Fecha", c.fecha.format("%Y-%m-%dT%H:%M:%S").to_string())
        .a("FormaPago", c.forma_pago.trim())
        .opt("NoCertificado", csd.map(|k| k.no_certificado.clone()))
        .a("SubTotal", fixed(t.sub_total, 2))
        .opt("Descuento", t.descuento.map(|d| fixed(d, 2)))
        .a("Moneda", c.moneda.trim())
        .opt("TipoCambio", c.tipo_cambio.map(qty))
        .a("Total", fixed(t.total, 2))
        .a("TipoDeComprobante", c.tipo.code())
        .a("Exportacion", c.exportacion.trim())
        .a("MetodoPago", c.metodo_pago.code())
        .a("LugarExpedicion", c.lugar_expedicion.trim());

    let mut cadena: Vec<String> = Vec::new();
    root.cadena(&mut cadena, &[]);
    let mut body = String::new();
    write_parties(c.relacionados.as_ref(), &c.emisor, &c.receptor, &mut cadena, &mut body);
    body.push_str("<cfdi:Conceptos>");
    for k in &c.conceptos {
        let n = Node::new("Concepto")
            .a("ClaveProdServ", k.clave_prod_serv.trim())
            .opt("NoIdentificacion", opt_s(&k.no_identificacion))
            .a("Cantidad", cantidad(k.cantidad))
            .a("ClaveUnidad", k.clave_unidad.trim())
            .opt("Unidad", opt_s(&k.unidad))
            .a("Descripcion", k.descripcion.trim())
            .a("ValorUnitario", qty(k.valor_unitario))
            .a("Importe", fixed(k.importe, 2))
            .opt("Descuento", k.descuento.map(|d| fixed(d, 2)))
            .a("ObjetoImp", k.objeto_imp());
        n.cadena(&mut cadena, &[]);
        if k.objeto_imp() == "01" {
            n.open(&mut body, true);
            continue;
        }
        n.open(&mut body, false);
        body.push_str("<cfdi:Impuestos>");
        // Concepto level: Traslados first, then Retenciones (XSD and XSLT).
        if let Some(iva) = k.iva {
            let tr = Node::new("Traslado").a("Base", fixed(k.base(), 2)).a("Impuesto", IMPUESTO_IVA);
            let tr = match iva {
                Iva::Tasa(p) => tr.a("TipoFactor", "Tasa").a("TasaOCuota", tasa(p)).a("Importe", fixed(k.iva_importe().unwrap_or_default(), 2)),
                Iva::Exento => tr.a("TipoFactor", "Exento"),
            };
            tr.cadena(&mut cadena, &[]);
            body.push_str("<cfdi:Traslados>");
            tr.open(&mut body, true);
            body.push_str("</cfdi:Traslados>");
        }
        let rets = k.retencion_importes();
        if !rets.is_empty() {
            body.push_str("<cfdi:Retenciones>");
            for (r, importe) in rets {
                let n = Node::new("Retencion")
                    .a("Base", fixed(k.base(), 2))
                    .a("Impuesto", r.impuesto.code())
                    .a("TipoFactor", "Tasa")
                    .a("TasaOCuota", tasa_fraction(r.tasa))
                    .a("Importe", fixed(importe, 2));
                n.cadena(&mut cadena, &[]);
                n.open(&mut body, true);
            }
            body.push_str("</cfdi:Retenciones>");
        }
        body.push_str("</cfdi:Impuestos></cfdi:Concepto>");
    }
    body.push_str("</cfdi:Conceptos>");
    let has_traslados = !t.traslados_tasa.is_empty() || t.base_exenta.is_some();
    if has_traslados || !t.retenciones.is_empty() {
        let imp = Node::new("Impuestos")
            .opt("TotalImpuestosRetenidos", t.total_retenciones.map(|v| fixed(v, 2)))
            .opt("TotalImpuestosTrasladados", t.total_traslados.map(|v| fixed(v, 2)));
        imp.open(&mut body, false);
        // Comprobante level: each Retencion, TotalImpuestosRetenidos, each
        // Traslado, TotalImpuestosTrasladados (the XSLT's order).
        if !t.retenciones.is_empty() {
            body.push_str("<cfdi:Retenciones>");
            for (impuesto, importe) in &t.retenciones {
                let n = Node::new("Retencion").a("Impuesto", impuesto.code()).a("Importe", fixed(*importe, 2));
                n.cadena(&mut cadena, &[]);
                n.open(&mut body, true);
            }
            body.push_str("</cfdi:Retenciones>");
        }
        if let Some(total) = t.total_retenciones {
            cadena.push(fixed(total, 2));
        }
        if has_traslados {
            body.push_str("<cfdi:Traslados>");
            let mut nodes = Vec::new();
            if let Some(base) = t.base_exenta {
                nodes.push(Node::new("Traslado").a("Base", fixed(base, 2)).a("Impuesto", IMPUESTO_IVA).a("TipoFactor", "Exento"));
            }
            for (p, base, importe) in &t.traslados_tasa {
                nodes.push(
                    Node::new("Traslado")
                        .a("Base", fixed(*base, 2))
                        .a("Impuesto", IMPUESTO_IVA)
                        .a("TipoFactor", "Tasa")
                        .a("TasaOCuota", tasa(*p))
                        .a("Importe", fixed(*importe, 2)),
                );
            }
            for n in &nodes {
                n.cadena(&mut cadena, &[]);
                n.open(&mut body, true);
            }
            body.push_str("</cfdi:Traslados>");
        }
        if let Some(total) = t.total_traslados {
            cadena.push(fixed(total, 2));
        }
        body.push_str("</cfdi:Impuestos>");
    }
    Ok(finish(root, cadena, &body, csd, &[], t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rsa::pkcs1v15::VerifyingKey;
    use rsa::signature::Verifier;
    use rust_decimal_macros::dec;

    const CER: &[u8] = include_bytes!("../testdata/test-csd.cer");
    const KEY: &[u8] = include_bytes!("../testdata/test-csd.key");
    const OTHER_KEY: &[u8] = include_bytes!("../testdata/other.key");
    const PASSWORD: &str = "12345678a";

    fn sample() -> Comprobante {
        Comprobante {
            serie: Some("A".into()),
            folio: "123".into(),
            fecha: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap().and_hms_opt(10, 30, 0).unwrap(),
            forma_pago: "03".into(),
            metodo_pago: MetodoPago::Pue,
            moneda: "MXN".into(),
            tipo_cambio: None,
            tipo: TipoDeComprobante::Ingreso,
            exportacion: "01".into(),
            lugar_expedicion: "45079".into(),
            relacionados: None,
            emisor: Emisor { rfc: "EKU9003173C9".into(), nombre: "ESCUELA KEMPER URGATE".into(), regimen_fiscal: "601".into() },
            receptor: Receptor {
                rfc: "XAXX010101000".into(),
                nombre: "PUBLICO EN GENERAL".into(),
                domicilio_fiscal: "45079".into(),
                regimen_fiscal: "616".into(),
                uso_cfdi: "S01".into(),
            },
            conceptos: vec![
                Concepto {
                    clave_prod_serv: "43211503".into(),
                    no_identificacion: None,
                    cantidad: dec!(2),
                    clave_unidad: "H87".into(),
                    unidad: Some("Pieza".into()),
                    descripcion: "Laptop  &  cargador".into(),
                    valor_unitario: dec!(500),
                    importe: dec!(1000),
                    descuento: Some(dec!(100)),
                    iva: Some(Iva::Tasa(dec!(16))),
                    retenciones: vec![],
                },
                Concepto {
                    clave_prod_serv: "86121500".into(),
                    no_identificacion: None,
                    cantidad: dec!(1),
                    clave_unidad: "E48".into(),
                    unidad: None,
                    descripcion: "Colegiatura".into(),
                    valor_unitario: dec!(250.5),
                    importe: dec!(250.5),
                    descuento: None,
                    iva: Some(Iva::Exento),
                    retenciones: vec![],
                },
            ],
        }
    }

    #[test]
    fn totals_and_formats() {
        let t = sample().totales();
        assert_eq!((t.sub_total, t.descuento, t.total_traslados, t.base_exenta), (dec!(1250.5), Some(dec!(100)), Some(dec!(144)), Some(dec!(250.5))));
        assert_eq!(t.total, dec!(1294.5));
        assert_eq!((tasa(dec!(16)), tasa(dec!(8)), tasa(dec!(0))), ("0.160000".into(), "0.080000".into(), "0.000000".into()));
        assert_eq!((qty(dec!(2)), qty(dec!(1.5)), qty(dec!(0.1234567))), ("2.00".into(), "1.50".into(), "0.123457".into()));
        assert_eq!((cantidad(dec!(2.000)), cantidad(dec!(1.50)), cantidad(dec!(0.1234567))), ("2".into(), "1.5".into(), "0.123457".into()));
        assert_eq!(normalize_space("  a \n b\t\tc "), "a b c");
    }

    #[test]
    fn draft_xml_and_cadena_without_a_csd() {
        let g = generate(&sample(), None).unwrap();
        assert_eq!(
            g.cadena_original,
            "||4.0|A|123|2026-05-01T10:30:00|03|1250.50|100.00|MXN|1294.50|I|01|PUE|45079\
             |EKU9003173C9|ESCUELA KEMPER URGATE|601|XAXX010101000|PUBLICO EN GENERAL|45079|616|S01\
             |43211503|2|H87|Pieza|Laptop & cargador|500.00|1000.00|100.00|02|900.00|002|Tasa|0.160000|144.00\
             |86121500|1|E48|Colegiatura|250.50|250.50|02|250.50|002|Exento\
             |250.50|002|Exento|900.00|002|Tasa|0.160000|144.00|144.00||"
        );
        assert!(g.xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<cfdi:Comprobante xmlns:cfdi=\"http://www.sat.gob.mx/cfd/4\""));
        assert!(g.xml.contains("xsi:schemaLocation=\"http://www.sat.gob.mx/cfd/4 http://www.sat.gob.mx/sitio_internet/cfd/4/cfdv40.xsd\""));
        assert!(g.xml.contains("Descripcion=\"Laptop  &amp;  cargador\""), "XML keeps the text; the cadena normalises it");
        assert!(!g.xml.contains("Sello=") && !g.xml.contains("NoCertificado="));
        assert!(g.xml.contains("<cfdi:Impuestos TotalImpuestosTrasladados=\"144.00\"><cfdi:Traslados><cfdi:Traslado Base=\"250.50\" Impuesto=\"002\" TipoFactor=\"Exento\"/>"));
        let order: Vec<usize> = ["<cfdi:Emisor", "<cfdi:Receptor", "<cfdi:Conceptos", "<cfdi:Impuestos TotalImpuestos"].iter().map(|n| g.xml.find(n).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "XSD element order");
    }

    #[test]
    fn sealed_with_a_csd() {
        let csd = Csd::from_der(CER, KEY, PASSWORD).unwrap();
        assert_eq!(csd.no_certificado, "30001000000500003416");
        assert!(!format!("{csd:?}").contains("key"), "the key is never printed");
        let g = generate(&sample(), Some(&csd)).unwrap();
        assert!(g.cadena_original.starts_with("||4.0|A|123|2026-05-01T10:30:00|03|30001000000500003416|1250.50|"));
        assert!(g.xml.contains(&format!("Sello=\"{}\"", g.sello)));
        assert!(g.xml.contains(&format!("Certificado=\"{}\"", B64.encode(CER))));
        assert!(g.xml.contains("NoCertificado=\"30001000000500003416\""));
        // The sello verifies against the certificate's public key.
        let cert = Certificate::from_der(CER).unwrap();
        let spki = cert.tbs_certificate.subject_public_key_info.to_der().unwrap();
        let public = RsaPublicKey::from_public_key_der(&spki).unwrap();
        let sig = rsa::pkcs1v15::Signature::try_from(B64.decode(&g.sello).unwrap().as_slice()).unwrap();
        VerifyingKey::<Sha256>::new(public).verify(g.cadena_original.as_bytes(), &sig).unwrap();
        // PKCS#1 v1.5 is deterministic.
        assert_eq!(csd.sello(&g.cadena_original), g.sello);
        // The same bytes as `openssl dgst -sha256 -sign` over UTF-8 text.
        let openssl = include_str!("../testdata/openssl-sello.b64").trim();
        assert_eq!(csd.sello("||4.0|Ñandú|áé||"), openssl);
    }

    #[test]
    fn bad_csds_and_comprobantes_are_refused() {
        assert!(Csd::from_der(CER, KEY, "wrong").unwrap_err().contains("password"));
        assert!(Csd::from_der(CER, OTHER_KEY, PASSWORD).unwrap_err().contains("does not belong"));
        assert!(Csd::from_der(b"nope", KEY, PASSWORD).is_err());
        let pem = include_bytes!("../testdata/test-csd.key.pem");
        assert_eq!(Csd::from_der(CER, pem, PASSWORD).unwrap().no_certificado, "30001000000500003416", "PEM keys open too");
        let mut c = sample();
        c.metodo_pago = MetodoPago::Ppd;
        assert!(generate(&c, None).unwrap_err().contains("FormaPago must be 99"));
        c.forma_pago = "99".into();
        assert!(generate(&c, None).is_ok());
        c.lugar_expedicion = "4507".into();
        assert!(generate(&c, None).is_err());
        let mut c = sample();
        c.conceptos.clear();
        assert!(generate(&c, None).is_err());
    }

    /// The research document's golden cadena (§5), produced by the SAT's
    /// own cadenaoriginal_4_0.xslt with xsltproc on an XSD-valid invoice.
    #[test]
    fn matches_the_sat_xslt_golden_cadena() {
        let csd = Csd::from_der(CER, KEY, PASSWORD).unwrap();
        let c = Comprobante {
            serie: Some("A".into()),
            folio: "1001".into(),
            fecha: NaiveDate::from_ymd_opt(2026, 10, 20).unwrap().and_hms_opt(10, 15, 0).unwrap(),
            forma_pago: "03".into(),
            metodo_pago: MetodoPago::Pue,
            moneda: "MXN".into(),
            tipo_cambio: None,
            tipo: TipoDeComprobante::Ingreso,
            exportacion: "01".into(),
            lugar_expedicion: "20000".into(),
            relacionados: None,
            emisor: Emisor { rfc: "EKU9003173C9".into(), nombre: "ESCUELA KEMPER URGATE".into(), regimen_fiscal: "601".into() },
            receptor: Receptor {
                rfc: "URE180429TM6".into(),
                nombre: "UNIVERSIDAD ROBOTICA ESPAÑOLA".into(),
                domicilio_fiscal: "86991".into(),
                regimen_fiscal: "601".into(),
                uso_cfdi: "G03".into(),
            },
            conceptos: vec![
                Concepto {
                    clave_prod_serv: "84111506".into(),
                    no_identificacion: Some("SRV-01".into()),
                    cantidad: dec!(1),
                    clave_unidad: "E48".into(),
                    unidad: Some("Servicio".into()),
                    descripcion: "Servicio   de\n      contabilidad".into(),
                    valor_unitario: dec!(1000),
                    importe: dec!(1000),
                    descuento: None,
                    iva: Some(Iva::Tasa(dec!(16))),
                    retenciones: vec![],
                },
                Concepto {
                    clave_prod_serv: "50221200".into(),
                    no_identificacion: None,
                    cantidad: dec!(2),
                    clave_unidad: "H87".into(),
                    unidad: Some("Pieza".into()),
                    descripcion: "Tortilla de maiz".into(),
                    valor_unitario: dec!(250),
                    importe: dec!(500),
                    descuento: None,
                    iva: Some(Iva::Tasa(dec!(0))),
                    retenciones: vec![],
                },
            ],
        };
        let g = generate(&c, Some(&csd)).unwrap();
        assert_eq!(
            g.cadena_original,
            "||4.0|A|1001|2026-10-20T10:15:00|03|30001000000500003416|1500.00|MXN|1660.00|I|01|PUE|20000|EKU9003173C9|ESCUELA KEMPER URGATE|601|URE180429TM6|UNIVERSIDAD ROBOTICA ESPAÑOLA|86991|601|G03|84111506|SRV-01|1|E48|Servicio|Servicio de contabilidad|1000.00|1000.00|02|1000.00|002|Tasa|0.160000|160.00|50221200|2|H87|Pieza|Tortilla de maiz|250.00|500.00|02|500.00|002|Tasa|0.000000|0.00|1000.00|002|Tasa|0.160000|160.00|500.00|002|Tasa|0.000000|0.00|160.00||"
        );
    }

    #[test]
    fn credit_notes_relate_to_their_invoice() {
        let mut c = sample();
        c.tipo = TipoDeComprobante::Egreso;
        c.relacionados = Some(("01".into(), vec!["5FB2822E-396D-4725-8521-CDC4BDD20CCF".into()]));
        let g = generate(&c, None).unwrap();
        assert!(g.cadena_original.contains("|45079|01|5FB2822E-396D-4725-8521-CDC4BDD20CCF|EKU9003173C9|"));
        assert!(g.xml.contains("TipoDeComprobante=\"E\""));
        assert!(g.xml.contains("<cfdi:CfdiRelacionados TipoRelacion=\"01\"><cfdi:CfdiRelacionado UUID="));
    }
}

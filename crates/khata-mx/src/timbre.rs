//! The PAC's answer and what follows from it: the TimbreFiscalDigital 1.1
//! complement, its cadena original, the printed representation's QR text,
//! and the rules of a cancellation request.
//!
//! Sources: `docs/research/mexico-compliance.md` §7 and §13: the SAT's
//! `TimbreFiscalDigitalv11.xsd` and `cadenaoriginal_TFD_1_1.xslt`
//! [HIGH]; Anexo 20 (DOF 13 Jan 2022) §I.D (QR of the printed CFDI) and
//! the cancellation rules (motivos 01–04, FolioSustitucion with 01)
//! [HIGH]; the receiver's acceptance and its exceptions (CFF art. 29-A;
//! RMF rules 2.7.1.34–35) [UNCERTAIN: sources disagree, so they are not
//! encoded: the PAC reports what the SAT answered].

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::pagos::is_uuid;

/// The TFD namespace.
pub const NS_TFD: &str = "http://www.sat.gob.mx/TimbreFiscalDigital";
/// Its schema.
pub const SCHEMA_TFD: &str = "http://www.sat.gob.mx/sitio_internet/cfd/TimbreFiscalDigital/TimbreFiscalDigitalv11.xsd";
/// The SAT's verification page the printed QR points to.
pub const VERIFY_URL: &str = "https://verificacfdi.facturaelectronica.sat.gob.mx/default.aspx";

/// The TimbreFiscalDigital 1.1 a PAC adds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timbre {
    pub version: String,
    /// The folio fiscal.
    pub uuid: String,
    /// `AAAA-MM-DDThh:mm:ss`, Mexico City time.
    pub fecha_timbrado: String,
    /// The PAC's RFC.
    pub rfc_prov_certif: String,
    pub leyenda: Option<String>,
    /// The issuer's Sello as stamped.
    pub sello_cfd: String,
    pub no_certificado_sat: String,
    pub sello_sat: String,
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&#124;", "|").replace("&amp;", "&")
}

/// The value of attribute `name` in an element's start tag (`tag` is the
/// text from `<` to `>`).
pub fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let i = rest.find(name)?;
        let before = rest[..i].chars().last();
        let after = &rest[i + name.len()..];
        let after_trim = after.trim_start();
        if before.is_some_and(char::is_whitespace) && after_trim.starts_with('=') {
            let v = after_trim[1..].trim_start();
            let quote = v.chars().next().filter(|q| *q == '"' || *q == '\'')?;
            let end = v[1..].find(quote)?;
            return Some(unescape(&v[1..1 + end]));
        }
        rest = &rest[i + name.len()..];
    }
}

/// The start tag of the first element named `local` (any prefix).
pub fn start_tag<'a>(xml: &'a str, local: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = xml[from..].find('<').map(|i| i + from) {
        let rest = &xml[i + 1..];
        let name_end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name == local || (name.contains(':') && name.rsplit(':').next() == Some(local)) {
            let end = rest.find('>')?;
            return Some(&xml[i..i + 1 + end + 1]);
        }
        from = i + 1;
    }
    None
}

impl Timbre {
    /// Read the TFD from a stamped CFDI.
    pub fn from_xml(xml: &str) -> Option<Timbre> {
        let tag = start_tag(xml, "TimbreFiscalDigital")?;
        let a = |n: &str| attribute(tag, n);
        Some(Timbre {
            version: a("Version")?,
            uuid: a("UUID")?.to_uppercase(),
            fecha_timbrado: a("FechaTimbrado")?,
            rfc_prov_certif: a("RfcProvCertif").unwrap_or_default(),
            leyenda: a("Leyenda").filter(|l| !l.is_empty()),
            sello_cfd: a("SelloCFD").unwrap_or_default(),
            no_certificado_sat: a("NoCertificadoSAT").unwrap_or_default(),
            sello_sat: a("SelloSAT").unwrap_or_default(),
        })
    }

    /// The TFD's cadena original (`cadenaoriginal_TFD_1_1.xslt`):
    /// `||Version|UUID|FechaTimbrado|RfcProvCertif[|Leyenda]|SelloCFD|NoCertificadoSAT||`.
    pub fn cadena_original(&self) -> String {
        let mut parts = vec![self.version.as_str(), &self.uuid, &self.fecha_timbrado, &self.rfc_prov_certif];
        if let Some(l) = &self.leyenda {
            parts.push(l);
        }
        parts.push(&self.sello_cfd);
        parts.push(&self.no_certificado_sat);
        let parts: Vec<String> = parts.iter().map(|p| crate::cfdi::normalize_space(p)).collect();
        format!("||{}||", parts.join("|"))
    }

    /// The element, with its namespace and schema location.
    pub fn to_xml(&self) -> String {
        let esc = crate::cfdi::esc;
        let leyenda = self.leyenda.as_ref().map(|l| format!(" Leyenda=\"{}\"", esc(l))).unwrap_or_default();
        format!(
            "<tfd:TimbreFiscalDigital xmlns:tfd=\"{NS_TFD}\" xsi:schemaLocation=\"{NS_TFD} {SCHEMA_TFD}\" Version=\"{}\" UUID=\"{}\" FechaTimbrado=\"{}\" RfcProvCertif=\"{}\"{leyenda} SelloCFD=\"{}\" NoCertificadoSAT=\"{}\" SelloSAT=\"{}\"/>",
            esc(&self.version),
            esc(&self.uuid),
            esc(&self.fecha_timbrado),
            esc(&self.rfc_prov_certif),
            esc(&self.sello_cfd),
            esc(&self.no_certificado_sat),
            esc(&self.sello_sat)
        )
    }

    /// Put the TFD in the CFDI's Complemento (created when missing).
    pub fn attach(&self, xml: &str) -> Result<String, String> {
        if start_tag(xml, "TimbreFiscalDigital").is_some() {
            return Err("the CFDI is already stamped".into());
        }
        let tfd = self.to_xml();
        if let Some(i) = xml.rfind("</cfdi:Complemento>") {
            return Ok(format!("{}{tfd}{}", &xml[..i], &xml[i..]));
        }
        let i = xml.rfind("</cfdi:Comprobante>").ok_or("not a CFDI: no cfdi:Comprobante")?;
        Ok(format!("{}<cfdi:Complemento>{tfd}</cfdi:Complemento>{}", &xml[..i], &xml[i..]))
    }
}

/// The text of the printed CFDI's QR code (Anexo 20 §I.D): the SAT's
/// verification URL with the UUID, both RFCs (XML-escaped, so `&` is
/// `&amp;`), the Total and the last 8 characters of the issuer's Sello.
/// The Total is written with six decimals and trailing zeros dropped
/// (`1453.33`, `0.0`), as phpcfdi does for CFDI 4.0 [MEDIUM: Anexo 20's
/// own example pads to `0000014300.000000`; the SAT accepts the short
/// form in practice].
pub fn qr_text(uuid: &str, rfc_emisor: &str, rfc_receptor: &str, total: Decimal, sello_cfd: &str) -> String {
    let mut t = total.round_dp(6);
    t.rescale(6);
    let mut tt = t.to_string().trim_end_matches('0').to_owned();
    if tt.ends_with('.') {
        tt.push('0');
    }
    let fe: String = sello_cfd.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
    let esc = crate::cfdi::esc;
    format!("{VERIFY_URL}?id={}&re={}&rr={}&tt={tt}&fe={fe}", uuid.to_uppercase(), esc(rfc_emisor), esc(rfc_receptor))
}

/// Why a CFDI is cancelled (Anexo 20, cancellation; RMF 2022 onwards).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motivo {
    /// `01` Comprobante emitido con errores con relación: the replacement
    /// CFDI is issued first, and its UUID goes in FolioSustitucion.
    ErroresConRelacion,
    /// `02` Comprobante emitido con errores sin relación.
    ErroresSinRelacion,
    /// `03` No se llevó a cabo la operación.
    NoSeLlevoACabo,
    /// `04` Operación nominativa relacionada en una factura global.
    FacturaGlobal,
}

impl Motivo {
    pub fn code(self) -> &'static str {
        match self {
            Motivo::ErroresConRelacion => "01",
            Motivo::ErroresSinRelacion => "02",
            Motivo::NoSeLlevoACabo => "03",
            Motivo::FacturaGlobal => "04",
        }
    }

    pub fn parse(code: &str) -> Result<Motivo, String> {
        Ok(match code.trim() {
            "01" | "1" => Motivo::ErroresConRelacion,
            "02" | "2" => Motivo::ErroresSinRelacion,
            "03" | "3" => Motivo::NoSeLlevoACabo,
            "04" | "4" => Motivo::FacturaGlobal,
            other => return Err(format!("{other:?} is not a cancellation motive (01, 02, 03 or 04)")),
        })
    }
}

/// Check a cancellation request: FolioSustitucion is required with motivo
/// 01 (and must be another UUID), and not allowed with the others.
pub fn check_cancellation(uuid: &str, motivo: Motivo, folio_sustitucion: Option<&str>) -> Result<Option<String>, String> {
    let folio = folio_sustitucion.map(str::trim).filter(|f| !f.is_empty()).map(str::to_uppercase);
    match (motivo, &folio) {
        (Motivo::ErroresConRelacion, None) => Err("motivo 01 needs the UUID of the CFDI that replaces this one (issue the replacement first)".into()),
        (Motivo::ErroresConRelacion, Some(f)) if !is_uuid(f) => Err(format!("{f:?} is not a UUID")),
        (Motivo::ErroresConRelacion, Some(f)) if f.eq_ignore_ascii_case(uuid) => Err("a CFDI cannot replace itself".into()),
        (Motivo::ErroresConRelacion, Some(_)) => Ok(folio),
        (_, Some(_)) => Err("a replacement UUID goes only with motivo 01".into()),
        (_, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    const STAMPED: &str = "<?xml version=\"1.0\"?><cfdi:Comprobante xmlns:cfdi=\"http://www.sat.gob.mx/cfd/4\" Total=\"10.00\" Sello=\"abc\"><cfdi:Complemento>\
        <tfd:TimbreFiscalDigital xmlns:tfd=\"http://www.sat.gob.mx/TimbreFiscalDigital\" Version=\"1.1\" UUID=\"1147a19d-8fd5-44f6-9c83-674974518572\" \
        FechaTimbrado=\"2017-05-12T16:32:27\" RfcProvCertif=\"AAA010101AAA\" SelloCFD=\"hFHb/Ew==\" NoCertificadoSAT=\"20001000000300022323\" SelloSAT=\"Pp0n+4sw==\"/>\
        </cfdi:Complemento></cfdi:Comprobante>";

    #[test]
    fn reads_the_tfd_and_its_cadena() {
        let t = Timbre::from_xml(STAMPED).unwrap();
        assert_eq!(t.uuid, "1147A19D-8FD5-44F6-9C83-674974518572");
        assert_eq!((t.no_certificado_sat.as_str(), t.sello_sat.as_str()), ("20001000000300022323", "Pp0n+4sw=="));
        // The SW Sapien README's cadenaOriginalSAT has the same shape.
        assert_eq!(t.cadena_original(), "||1.1|1147A19D-8FD5-44F6-9C83-674974518572|2017-05-12T16:32:27|AAA010101AAA|hFHb/Ew==|20001000000300022323||");
        assert!(Timbre::from_xml("<cfdi:Comprobante/>").is_none());
        assert_eq!(attribute("<x NoCertificadoSAT=\"1\" CertificadoSAT='2'>", "CertificadoSAT").as_deref(), Some("2"), "whole names only");
        assert_eq!(attribute("<x A=\"a &amp; b\">", "A").as_deref(), Some("a & b"));
    }

    #[test]
    fn attaches_a_tfd_once() {
        let t = Timbre::from_xml(STAMPED).unwrap();
        let plain = "<cfdi:Comprobante Total=\"1\"><cfdi:Conceptos/></cfdi:Comprobante>\n";
        let stamped = t.attach(plain).unwrap();
        assert!(stamped.contains("<cfdi:Conceptos/><cfdi:Complemento><tfd:TimbreFiscalDigital xmlns:tfd="));
        assert_eq!(Timbre::from_xml(&stamped).unwrap(), t);
        assert!(t.attach(&stamped).is_err());
        let with_pagos = "<cfdi:Comprobante><cfdi:Complemento><pago20:Pagos/></cfdi:Complemento></cfdi:Comprobante>";
        assert!(t.attach(with_pagos).unwrap().contains("<pago20:Pagos/><tfd:TimbreFiscalDigital"));
    }

    #[test]
    fn qr_text_and_cancellations() {
        assert_eq!(
            qr_text("5803eb8d-81cd-4557-8719-26632d2fa434", "VISM990101474", "XAXX010101000", dec!(14300), "xxxxrH8/bw=="),
            "https://verificacfdi.facturaelectronica.sat.gob.mx/default.aspx?id=5803EB8D-81CD-4557-8719-26632D2FA434&re=VISM990101474&rr=XAXX010101000&tt=14300.0&fe=rH8/bw=="
        );
        assert!(qr_text("u", "A&B010101AAA", "X", dec!(1453.33), "s").contains("re=A&amp;B010101AAA&rr=X&tt=1453.33&fe=s"));
        let u = "5FB2822E-396D-4725-8521-CDC4BDD20CCF";
        assert!(check_cancellation(u, Motivo::ErroresConRelacion, None).is_err());
        assert!(check_cancellation(u, Motivo::ErroresConRelacion, Some(u)).is_err());
        assert_eq!(
            check_cancellation(u, Motivo::ErroresConRelacion, Some("1147a19d-8fd5-44f6-9c83-674974518572")),
            Ok(Some("1147A19D-8FD5-44F6-9C83-674974518572".into()))
        );
        assert!(check_cancellation(u, Motivo::NoSeLlevoACabo, Some("1147a19d-8fd5-44f6-9c83-674974518572")).is_err());
        assert_eq!(check_cancellation(u, Motivo::ErroresSinRelacion, Some(" ")), Ok(None));
        assert_eq!(Motivo::parse("3").unwrap().code(), "03");
        assert!(Motivo::parse("05").is_err());
    }
}

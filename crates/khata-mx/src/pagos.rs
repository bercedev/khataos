//! The Recibo Electrónico de Pago: a CFDI 4.0 de tipo `P` carrying the
//! Complemento para recepción de Pagos 2.0, for payments of invoices
//! issued with MetodoPago `PPD`.
//!
//! Sources: `docs/research/mexico-compliance.md` §8.1 and §13: the SAT's
//! `Pagos20.xsd` and `Pagos20.xslt` (fetched 2026-10-21) and the *Guía de
//! llenado del comprobante al que se le incorpore el complemento para
//! recepción de pagos* for CFDI 4.0 [HIGH for the structure and the
//! fixed values below].
//!
//! The comprobante (Guía, section I): `SubTotal="0"`, `Moneda="XXX"`,
//! `Total="0"`, `TipoDeComprobante="P"`, `Exportacion="01"`; no
//! FormaPago, MetodoPago, Descuento, TipoCambio or Impuestos; the receptor's
//! UsoCFDI `CP01`; one Concepto `84111506`, Cantidad `1`, ClaveUnidad
//! `ACT`, Descripcion `Pago`, ValorUnitario `0`, Importe `0`, ObjetoImp
//! `01`. A receipt that replaces an earlier one relates to it with
//! TipoRelacion `04`.
//!
//! The complement (Guía, section II): `pago20:Pagos Version="2.0"` →
//! `Totales` (MXN) → one `Pago` per payment (FechaPago, FormaDePagoP ≠ 99,
//! MonedaP, TipoCambioP `1` for MXN, Monto) → one `DoctoRelacionado` per
//! invoice paid (IdDocumento = the invoice's UUID, MonedaDR, EquivalenciaDR
//! `1` when the currencies match, NumParcialidad, ImpSaldoAnt, ImpPagado,
//! ImpSaldoInsoluto = ImpSaldoAnt − ImpPagado, ObjetoImpDR) with its
//! `ImpuestosDR`, then the payment's `ImpuestosP` summary.
//!
//! **Taxes paid.** Each invoice's bases are taken in proportion to the
//! amount paid: BaseDR = base × ImpPagado / invoice Total, ImporteDR =
//! BaseDR × TasaOCuotaDR, each rounded to the centavo (the Guía leaves the
//! method to "las disposiciones fiscales vigentes"; proportional is common
//! practice [MEDIUM]). TrasladoP / RetencionP add up the documents' rows
//! by (impuesto, factor, rate) and by impuesto; the Totales are the
//! payment summaries × TipoCambioP, rounded.
//!
//! **Cadena original.** As for the comprobante ([`crate::cfdi`]); the
//! complement follows `Pagos20.xslt`: Version; the Totales attributes;
//! each Pago's attributes, then each DoctoRelacionado's attributes, its
//! RetencionDR rows and then its TrasladoDR rows (note: retenciones
//! first), then the payment's RetencionP and TrasladoP rows.
//!
//! Only MXN payments of MXN invoices are made (foreign currency needs
//! TipoCambioP and EquivalenciaDR from the user, not modelled).

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::cfdi::{self, Comprobante, Csd, Emisor, Generated, Impuesto, Iva, Node, Receptor, Totales, fixed, money, tasa, tasa_fraction};

/// The Pagos 2.0 namespace.
pub const NS_PAGOS: &str = "http://www.sat.gob.mx/Pagos20";
/// Its schema.
pub const SCHEMA_PAGOS: &str = "http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xsd";

/// The Concepto every receipt carries (Guía).
pub const CLAVE_PROD_SERV: &str = "84111506";
pub const CLAVE_UNIDAD: &str = "ACT";
/// UsoCFDI of a payment receipt.
pub const USO_CFDI: &str = "CP01";

/// One IVA traslado of a paid invoice: the factor and the invoice's base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrasladoBase {
    pub iva: Iva,
    pub base: Decimal,
}

/// One withholding of a paid invoice: impuesto, TasaOCuota (fraction) and
/// the invoice's base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetencionBase {
    pub impuesto: Impuesto,
    pub tasa: Decimal,
    pub base: Decimal,
}

/// The taxes of an invoice as stamped (MXN): what the payment complement
/// takes its proportions from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocTaxes {
    /// The invoice's Total.
    pub total: Decimal,
    pub traslados: Vec<TrasladoBase>,
    pub retenciones: Vec<RetencionBase>,
}

impl DocTaxes {
    /// From an invoice: traslados by rate (and exento), retenciones by
    /// (impuesto, rate).
    pub fn from_comprobante(c: &Comprobante) -> DocTaxes {
        let t = c.totales();
        let mut traslados: Vec<TrasladoBase> = t.traslados_tasa.iter().map(|(p, base, _)| TrasladoBase { iva: Iva::Tasa(*p), base: *base }).collect();
        if let Some(base) = t.base_exenta {
            traslados.push(TrasladoBase { iva: Iva::Exento, base });
        }
        let mut retenciones: Vec<RetencionBase> = Vec::new();
        for k in &c.conceptos {
            for r in &k.retenciones {
                let base = money(k.base());
                match retenciones.iter_mut().find(|x| x.impuesto == r.impuesto && x.tasa == r.tasa) {
                    Some(x) => x.base += base,
                    None => retenciones.push(RetencionBase { impuesto: r.impuesto, tasa: r.tasa, base }),
                }
            }
        }
        DocTaxes { total: t.total, traslados, retenciones }
    }
}

/// An invoice paid (in part) by this payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctoRelacionado {
    /// The invoice's folio fiscal (UUID).
    pub id_documento: String,
    pub serie: Option<String>,
    pub folio: Option<String>,
    /// 1 for the first payment of the invoice.
    pub num_parcialidad: u32,
    /// What was owed before this payment (the invoice Total for the first).
    pub saldo_anterior: Decimal,
    /// What this payment pays of it.
    pub pagado: Decimal,
    pub taxes: DocTaxes,
}

impl DoctoRelacionado {
    pub fn saldo_insoluto(&self) -> Decimal {
        self.saldo_anterior - self.pagado
    }
}

/// One payment received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pago {
    /// When the money was received (12:00:00 when the time is unknown).
    pub fecha_pago: NaiveDateTime,
    /// c_FormaPago, never 99.
    pub forma_de_pago: String,
    /// The amount received, at least the sum paid on the documents.
    pub monto: Decimal,
    /// Cheque number, SPEI tracking key…
    pub num_operacion: Option<String>,
    pub documentos: Vec<DoctoRelacionado>,
}

/// The receipt before sealing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReciboPago {
    pub serie: Option<String>,
    pub folio: String,
    /// When the receipt is issued.
    pub fecha: NaiveDateTime,
    pub lugar_expedicion: String,
    /// UUIDs of earlier receipts this one replaces (TipoRelacion 04).
    pub sustituye: Vec<String>,
    pub emisor: Emisor,
    /// UsoCFDI is always written as CP01.
    pub receptor: Receptor,
    pub pagos: Vec<Pago>,
}

/// A UUID as the XSD's IdDocumento pattern wants it (8-4-4-4-12 hex).
pub fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5 && parts.iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// One DoctoRelacionado's tax rows for this payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DrTaxes {
    /// (impuesto, TasaOCuota fraction, BaseDR, ImporteDR).
    pub retenciones: Vec<(Impuesto, Decimal, Decimal, Decimal)>,
    /// (iva, BaseDR, ImporteDR: `None` for exento).
    pub traslados: Vec<(Iva, Decimal, Option<Decimal>)>,
}

/// The proportional taxes of a payment on one document.
pub fn dr_taxes(d: &DoctoRelacionado) -> DrTaxes {
    let factor = if d.taxes.total.is_zero() { Decimal::ONE } else { d.pagado / d.taxes.total };
    let retenciones = d
        .taxes
        .retenciones
        .iter()
        .map(|r| {
            let base = money(r.base * factor);
            (r.impuesto, r.tasa, base, money(base * r.tasa))
        })
        .filter(|r| r.2 > Decimal::ZERO)
        .collect();
    let traslados = d
        .taxes
        .traslados
        .iter()
        .map(|t| {
            let base = money(t.base * factor);
            let importe = match t.iva {
                Iva::Tasa(p) => Some(money(base * p / Decimal::ONE_HUNDRED)),
                Iva::Exento => None,
            };
            (t.iva, base, importe)
        })
        .filter(|t| t.1 > Decimal::ZERO)
        .collect();
    DrTaxes { retenciones, traslados }
}

/// The payment's ImpuestosP: RetencionP by impuesto, TrasladoP by factor
/// and rate (16, 8, 0, then exento).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImpuestosP {
    pub retenciones: Vec<(Impuesto, Decimal)>,
    /// (iva, BaseP, ImporteP).
    pub traslados: Vec<(Iva, Decimal, Option<Decimal>)>,
}

fn iva_order(i: &Iva) -> (u8, Decimal) {
    match i {
        Iva::Tasa(p) => (0, -*p),
        Iva::Exento => (1, Decimal::ZERO),
    }
}

pub fn impuestos_p(p: &Pago) -> ImpuestosP {
    let mut out = ImpuestosP::default();
    for d in &p.documentos {
        let t = dr_taxes(d);
        for (imp, _, _, importe) in t.retenciones {
            match out.retenciones.iter_mut().find(|x| x.0 == imp) {
                Some(x) => x.1 += importe,
                None => out.retenciones.push((imp, importe)),
            }
        }
        for (iva, base, importe) in t.traslados {
            match out.traslados.iter_mut().find(|x| x.0 == iva) {
                Some(x) => {
                    x.1 += base;
                    x.2 = x.2.map(|a| a + importe.unwrap_or_default());
                }
                None => out.traslados.push((iva, base, importe)),
            }
        }
    }
    out.retenciones.sort_by_key(|r| r.0);
    out.traslados.sort_by_key(|t| iva_order(&t.0));
    out
}

/// What a receipt adds up to (the Totales node).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PagoTotales {
    pub retenciones_iva: Option<Decimal>,
    pub retenciones_isr: Option<Decimal>,
    pub base_iva16: Option<Decimal>,
    pub impuesto_iva16: Option<Decimal>,
    pub base_iva8: Option<Decimal>,
    pub impuesto_iva8: Option<Decimal>,
    pub base_iva0: Option<Decimal>,
    pub impuesto_iva0: Option<Decimal>,
    pub base_iva_exento: Option<Decimal>,
    pub monto_total_pagos: Decimal,
}

fn add(slot: &mut Option<Decimal>, v: Decimal) {
    *slot = Some(slot.unwrap_or_default() + v);
}

/// The Totales (TipoCambioP is 1: MXN only).
pub fn totales(r: &ReciboPago) -> Result<PagoTotales, String> {
    let mut t = PagoTotales::default();
    for p in &r.pagos {
        t.monto_total_pagos += p.monto;
        let imp = impuestos_p(p);
        for (i, importe) in imp.retenciones {
            match i {
                Impuesto::Iva => add(&mut t.retenciones_iva, importe),
                Impuesto::Isr => add(&mut t.retenciones_isr, importe),
            }
        }
        for (iva, base, importe) in imp.traslados {
            let importe = importe.unwrap_or_default();
            match iva {
                Iva::Tasa(p) if p == Decimal::from(16) => {
                    add(&mut t.base_iva16, base);
                    add(&mut t.impuesto_iva16, importe);
                }
                Iva::Tasa(p) if p == Decimal::from(8) => {
                    add(&mut t.base_iva8, base);
                    add(&mut t.impuesto_iva8, importe);
                }
                Iva::Tasa(p) if p.is_zero() => {
                    add(&mut t.base_iva0, base);
                    add(&mut t.impuesto_iva0, importe);
                }
                Iva::Tasa(p) => return Err(format!("IVA at {}% has no place in the Pagos 2.0 Totales (16, 8, 0 or exento)", p.normalize())),
                Iva::Exento => add(&mut t.base_iva_exento, base),
            }
        }
    }
    Ok(t)
}

/// Problems the SAT would reject (checked before generating).
pub fn check(r: &ReciboPago) -> Result<(), String> {
    if r.pagos.is_empty() {
        return Err("a payment receipt needs at least one payment".into());
    }
    for (what, cp) in [("LugarExpedicion", &r.lugar_expedicion), ("DomicilioFiscalReceptor", &r.receptor.domicilio_fiscal)] {
        if cp.len() != 5 || !cp.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("{what} must be a 5-digit postal code, not {cp:?}"));
        }
    }
    for u in &r.sustituye {
        if !is_uuid(u.trim()) {
            return Err(format!("{u:?} is not a UUID"));
        }
    }
    for p in &r.pagos {
        let forma = p.forma_de_pago.trim();
        if forma.is_empty() || forma == "99" {
            return Err("a payment's forma de pago (FormaDePagoP) is how the money came in, never 99".into());
        }
        if p.monto <= Decimal::ZERO {
            return Err("a payment's Monto must be more than zero".into());
        }
        if p.documentos.is_empty() {
            return Err("a payment must settle at least one invoice (DoctoRelacionado)".into());
        }
        let paid: Decimal = p.documentos.iter().map(|d| money(d.pagado)).sum();
        if paid > money(p.monto) {
            return Err(format!("the documents are paid {} in all, more than the payment's Monto {}", fixed(paid, 2), fixed(p.monto, 2)));
        }
        for d in &p.documentos {
            if !is_uuid(d.id_documento.trim()) {
                return Err(format!("{:?} is not the invoice's folio fiscal (UUID)", d.id_documento));
            }
            if !(1..=999).contains(&d.num_parcialidad) {
                return Err("NumParcialidad is 1 to 999".into());
            }
            if d.saldo_anterior <= Decimal::ZERO || d.pagado <= Decimal::ZERO {
                return Err(format!("{}: the balance before the payment and the amount paid must be more than zero", d.id_documento));
            }
            if money(d.saldo_insoluto()) < Decimal::ZERO {
                return Err(format!("{}: {} is paid on a balance of {}", d.id_documento, fixed(d.pagado, 2), fixed(d.saldo_anterior, 2)));
            }
        }
    }
    totales(r).map(|_| ())
}

fn opt_fixed(v: Option<Decimal>) -> Option<String> {
    v.map(|v| fixed(v, 2))
}

/// Build the receipt's XML, cadena original and sello. Without a CSD it is
/// an unsealed draft, as for [`cfdi::generate`].
pub fn generate(r: &ReciboPago, csd: Option<&Csd>) -> Result<Generated, String> {
    check(r)?;
    let tot = totales(r)?;
    let root = Node::new("Comprobante")
        .a("Version", "4.0")
        .opt("Serie", cfdi::opt_s(&r.serie))
        .a("Folio", r.folio.trim())
        .a("Fecha", r.fecha.format("%Y-%m-%dT%H:%M:%S").to_string())
        .opt("NoCertificado", csd.map(|k| k.no_certificado.clone()))
        .a("SubTotal", "0")
        .a("Moneda", "XXX")
        .a("Total", "0")
        .a("TipoDeComprobante", "P")
        .a("Exportacion", "01")
        .a("LugarExpedicion", r.lugar_expedicion.trim());
    let mut cadena: Vec<String> = Vec::new();
    root.cadena(&mut cadena, &[]);
    let mut body = String::new();
    let relacionados = (!r.sustituye.is_empty()).then(|| ("04".to_owned(), r.sustituye.iter().map(|u| u.trim().to_uppercase()).collect::<Vec<_>>()));
    let receptor = Receptor { uso_cfdi: USO_CFDI.into(), ..r.receptor.clone() };
    cfdi::write_parties(relacionados.as_ref(), &r.emisor, &receptor, &mut cadena, &mut body);
    let concepto = Node::new("Concepto")
        .a("ClaveProdServ", CLAVE_PROD_SERV)
        .a("Cantidad", "1")
        .a("ClaveUnidad", CLAVE_UNIDAD)
        .a("Descripcion", "Pago")
        .a("ValorUnitario", "0")
        .a("Importe", "0")
        .a("ObjetoImp", "01");
    concepto.cadena(&mut cadena, &[]);
    body.push_str("<cfdi:Conceptos>");
    concepto.open(&mut body, true);
    body.push_str("</cfdi:Conceptos><cfdi:Complemento>");

    let pagos = Node::ns("pago20", "Pagos").a("Version", "2.0");
    pagos.cadena(&mut cadena, &[]);
    pagos.open(&mut body, false);
    let totales_node = Node::ns("pago20", "Totales")
        .opt("TotalRetencionesIVA", opt_fixed(tot.retenciones_iva))
        .opt("TotalRetencionesISR", opt_fixed(tot.retenciones_isr))
        .opt("TotalTrasladosBaseIVA16", opt_fixed(tot.base_iva16))
        .opt("TotalTrasladosImpuestoIVA16", opt_fixed(tot.impuesto_iva16))
        .opt("TotalTrasladosBaseIVA8", opt_fixed(tot.base_iva8))
        .opt("TotalTrasladosImpuestoIVA8", opt_fixed(tot.impuesto_iva8))
        .opt("TotalTrasladosBaseIVA0", opt_fixed(tot.base_iva0))
        .opt("TotalTrasladosImpuestoIVA0", opt_fixed(tot.impuesto_iva0))
        .opt("TotalTrasladosBaseIVAExento", opt_fixed(tot.base_iva_exento))
        .a("MontoTotalPagos", fixed(tot.monto_total_pagos, 2));
    totales_node.cadena(&mut cadena, &[]);
    totales_node.open(&mut body, true);
    for p in &r.pagos {
        let pago = Node::ns("pago20", "Pago")
            .a("FechaPago", p.fecha_pago.format("%Y-%m-%dT%H:%M:%S").to_string())
            .a("FormaDePagoP", p.forma_de_pago.trim())
            .a("MonedaP", "MXN")
            .a("TipoCambioP", "1")
            .a("Monto", fixed(p.monto, 2))
            .opt("NumOperacion", cfdi::opt_s(&p.num_operacion));
        pago.cadena(&mut cadena, &[]);
        pago.open(&mut body, false);
        for d in &p.documentos {
            let t = dr_taxes(d);
            let objeto = if t.retenciones.is_empty() && t.traslados.is_empty() { "01" } else { "02" };
            let dr = Node::ns("pago20", "DoctoRelacionado")
                .a("IdDocumento", d.id_documento.trim().to_uppercase())
                .opt("Serie", cfdi::opt_s(&d.serie))
                .opt("Folio", cfdi::opt_s(&d.folio))
                .a("MonedaDR", "MXN")
                .a("EquivalenciaDR", "1")
                .a("NumParcialidad", d.num_parcialidad.to_string())
                .a("ImpSaldoAnt", fixed(d.saldo_anterior, 2))
                .a("ImpPagado", fixed(d.pagado, 2))
                .a("ImpSaldoInsoluto", fixed(d.saldo_insoluto(), 2))
                .a("ObjetoImpDR", objeto);
            dr.cadena(&mut cadena, &[]);
            if objeto == "01" {
                dr.open(&mut body, true);
                continue;
            }
            dr.open(&mut body, false);
            body.push_str("<pago20:ImpuestosDR>");
            if !t.retenciones.is_empty() {
                body.push_str("<pago20:RetencionesDR>");
                for (imp, rate, base, importe) in &t.retenciones {
                    let n = Node::ns("pago20", "RetencionDR")
                        .a("BaseDR", fixed(*base, 2))
                        .a("ImpuestoDR", imp.code())
                        .a("TipoFactorDR", "Tasa")
                        .a("TasaOCuotaDR", tasa_fraction(*rate))
                        .a("ImporteDR", fixed(*importe, 2));
                    n.cadena(&mut cadena, &[]);
                    n.open(&mut body, true);
                }
                body.push_str("</pago20:RetencionesDR>");
            }
            if !t.traslados.is_empty() {
                body.push_str("<pago20:TrasladosDR>");
                for (iva, base, importe) in &t.traslados {
                    let n = Node::ns("pago20", "TrasladoDR").a("BaseDR", fixed(*base, 2)).a("ImpuestoDR", cfdi::IMPUESTO_IVA);
                    let n = match iva {
                        Iva::Tasa(p) => n.a("TipoFactorDR", "Tasa").a("TasaOCuotaDR", tasa(*p)).a("ImporteDR", fixed(importe.unwrap_or_default(), 2)),
                        Iva::Exento => n.a("TipoFactorDR", "Exento"),
                    };
                    n.cadena(&mut cadena, &[]);
                    n.open(&mut body, true);
                }
                body.push_str("</pago20:TrasladosDR>");
            }
            body.push_str("</pago20:ImpuestosDR>");
            dr.close(&mut body);
        }
        let imp = impuestos_p(p);
        if !imp.retenciones.is_empty() || !imp.traslados.is_empty() {
            body.push_str("<pago20:ImpuestosP>");
            if !imp.retenciones.is_empty() {
                body.push_str("<pago20:RetencionesP>");
                for (i, importe) in &imp.retenciones {
                    let n = Node::ns("pago20", "RetencionP").a("ImpuestoP", i.code()).a("ImporteP", fixed(*importe, 2));
                    n.cadena(&mut cadena, &[]);
                    n.open(&mut body, true);
                }
                body.push_str("</pago20:RetencionesP>");
            }
            if !imp.traslados.is_empty() {
                body.push_str("<pago20:TrasladosP>");
                for (iva, base, importe) in &imp.traslados {
                    let n = Node::ns("pago20", "TrasladoP").a("BaseP", fixed(*base, 2)).a("ImpuestoP", cfdi::IMPUESTO_IVA);
                    let n = match iva {
                        Iva::Tasa(p) => n.a("TipoFactorP", "Tasa").a("TasaOCuotaP", tasa(*p)).a("ImporteP", fixed(importe.unwrap_or_default(), 2)),
                        Iva::Exento => n.a("TipoFactorP", "Exento"),
                    };
                    n.cadena(&mut cadena, &[]);
                    n.open(&mut body, true);
                }
                body.push_str("</pago20:TrasladosP>");
            }
            body.push_str("</pago20:ImpuestosP>");
        }
        pago.close(&mut body);
    }
    pagos.close(&mut body);
    body.push_str("</cfdi:Complemento>");
    let zero = Totales {
        sub_total: Decimal::ZERO,
        descuento: None,
        traslados_tasa: Vec::new(),
        base_exenta: None,
        total_traslados: None,
        retenciones: Vec::new(),
        total_retenciones: None,
        total: Decimal::ZERO,
    };
    Ok(cfdi::finish(root, cadena, &body, csd, &[("pago20", NS_PAGOS, SCHEMA_PAGOS)], zero))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfdi::{Concepto, MetodoPago, Retencion, TipoDeComprobante};
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    const CER: &[u8] = include_bytes!("../testdata/test-csd.cer");
    const KEY: &[u8] = include_bytes!("../testdata/test-csd.key");
    pub(crate) const UUID: &str = "5FB2822E-396D-4725-8521-CDC4BDD20CCF";

    fn at(d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 6, d).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    fn emisor() -> Emisor {
        Emisor { rfc: "EKU9003173C9".into(), nombre: "ESCUELA KEMPER URGATE".into(), regimen_fiscal: "601".into() }
    }

    fn receptor() -> Receptor {
        Receptor {
            rfc: "URE180429TM6".into(),
            nombre: "UNIVERSIDAD ROBOTICA ESPAÑOLA".into(),
            domicilio_fiscal: "86991".into(),
            regimen_fiscal: "601".into(),
            uso_cfdi: "G03".into(),
        }
    }

    /// A PPD invoice: 1000 at 16% with ISR 10% and IVA 2/3 withheld, plus
    /// 500 exento. Total = 1500 + 160 − 100 − 106.67 = 1453.33.
    pub(crate) fn invoice() -> Comprobante {
        let c = |desc: &str, importe: Decimal, iva: Iva, ret: Vec<Retencion>| Concepto {
            clave_prod_serv: "84111506".into(),
            no_identificacion: None,
            cantidad: dec!(1),
            clave_unidad: "E48".into(),
            unidad: None,
            descripcion: desc.into(),
            valor_unitario: importe,
            importe,
            descuento: None,
            iva: Some(iva),
            retenciones: ret,
        };
        Comprobante {
            serie: Some("A".into()),
            folio: "77".into(),
            fecha: at(1, 9),
            forma_pago: "99".into(),
            metodo_pago: MetodoPago::Ppd,
            moneda: "MXN".into(),
            tipo_cambio: None,
            tipo: TipoDeComprobante::Ingreso,
            exportacion: "01".into(),
            lugar_expedicion: "20000".into(),
            relacionados: None,
            emisor: emisor(),
            receptor: receptor(),
            conceptos: vec![
                c(
                    "Honorarios",
                    dec!(1000),
                    Iva::Tasa(dec!(16)),
                    vec![Retencion { impuesto: Impuesto::Isr, tasa: dec!(0.1) }, Retencion { impuesto: Impuesto::Iva, tasa: dec!(0.106667) }],
                ),
                c("Curso", dec!(500), Iva::Exento, vec![]),
            ],
        }
    }

    fn recibo(pagado: Decimal, saldo: Decimal, parcialidad: u32) -> ReciboPago {
        let taxes = DocTaxes::from_comprobante(&invoice());
        ReciboPago {
            serie: Some("P".into()),
            folio: "9".into(),
            fecha: at(20, 10),
            lugar_expedicion: "20000".into(),
            sustituye: vec![],
            emisor: emisor(),
            receptor: receptor(),
            pagos: vec![Pago {
                fecha_pago: at(19, 12),
                forma_de_pago: "03".into(),
                monto: pagado,
                num_operacion: Some("SPEI-123".into()),
                documentos: vec![DoctoRelacionado {
                    id_documento: UUID.into(),
                    serie: Some("A".into()),
                    folio: Some("77".into()),
                    num_parcialidad: parcialidad,
                    saldo_anterior: saldo,
                    pagado,
                    taxes,
                }],
            }],
        }
    }

    #[test]
    fn invoice_taxes_for_the_complement() {
        let t = DocTaxes::from_comprobante(&invoice());
        assert_eq!(t.total, dec!(1453.33));
        assert_eq!(t.traslados, [TrasladoBase { iva: Iva::Tasa(dec!(16)), base: dec!(1000) }, TrasladoBase { iva: Iva::Exento, base: dec!(500) }]);
        assert_eq!(t.retenciones.len(), 2);
    }

    #[test]
    fn a_full_payment_carries_the_invoice_taxes() {
        let g = generate(&recibo(dec!(1453.33), dec!(1453.33), 1), None).unwrap();
        assert!(g.xml.contains("xmlns:pago20=\"http://www.sat.gob.mx/Pagos20\""));
        assert!(g.xml.contains("http://www.sat.gob.mx/Pagos20 http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xsd\""));
        assert!(g.xml.contains("SubTotal=\"0\" Moneda=\"XXX\" Total=\"0\" TipoDeComprobante=\"P\" Exportacion=\"01\""));
        assert!(!g.xml.contains("FormaPago=") && !g.xml.contains("MetodoPago="));
        assert!(g.xml.contains("UsoCFDI=\"CP01\""));
        assert_eq!(
            g.cadena_original,
            "||4.0|P|9|2026-06-20T10:00:00|0|XXX|0|P|01|20000\
             |EKU9003173C9|ESCUELA KEMPER URGATE|601|URE180429TM6|UNIVERSIDAD ROBOTICA ESPAÑOLA|86991|601|CP01\
             |84111506|1|ACT|Pago|0|0|01\
             |2.0|106.67|100.00|1000.00|160.00|500.00|1453.33\
             |2026-06-19T12:00:00|03|MXN|1|1453.33|SPEI-123\
             |5FB2822E-396D-4725-8521-CDC4BDD20CCF|A|77|MXN|1|1|1453.33|1453.33|0.00|02\
             |1000.00|001|Tasa|0.100000|100.00|1000.00|002|Tasa|0.106667|106.67\
             |1000.00|002|Tasa|0.160000|160.00|500.00|002|Exento\
             |001|100.00|002|106.67|1000.00|002|Tasa|0.160000|160.00|500.00|002|Exento||"
        );
    }

    #[test]
    fn a_partial_payment_is_proportional() {
        // Half of the invoice.
        let r = recibo(dec!(726.67), dec!(1453.33), 1);
        let t = dr_taxes(&r.pagos[0].documentos[0]);
        // factor 0.5000034…: bases 500.00 and 250.00.
        assert_eq!(t.traslados, [(Iva::Tasa(dec!(16)), dec!(500.00), Some(dec!(80.00))), (Iva::Exento, dec!(250.00), None)]);
        assert_eq!(t.retenciones, [(Impuesto::Isr, dec!(0.1), dec!(500.00), dec!(50.00)), (Impuesto::Iva, dec!(0.106667), dec!(500.00), dec!(53.33))]);
        let g = generate(&r, None).unwrap();
        assert!(g.xml.contains("NumParcialidad=\"1\" ImpSaldoAnt=\"1453.33\" ImpPagado=\"726.67\" ImpSaldoInsoluto=\"726.66\""));
        let second = generate(&recibo(dec!(726.66), dec!(726.66), 2), None).unwrap();
        assert!(second.xml.contains("NumParcialidad=\"2\" ImpSaldoAnt=\"726.66\" ImpPagado=\"726.66\" ImpSaldoInsoluto=\"0.00\""));
    }

    /// With `KHATA_MX_DUMP=<dir>`, write the test documents (an invoice
    /// with retenciones, full and partial receipts, sealed) for checking
    /// against the SAT's XSD and XSLT with xmllint and xsltproc
    /// (`docs/mexico-cfdi.md`, "Conformance").
    #[test]
    fn dump_for_conformance() {
        let Some(dir) = std::env::var_os("KHATA_MX_DUMP") else { return };
        let dir = std::path::PathBuf::from(dir);
        let csd = Csd::from_der(CER, KEY, "12345678a").unwrap();
        let write = |name: &str, g: &Generated| {
            std::fs::write(dir.join(format!("{name}.xml")), &g.xml).unwrap();
            std::fs::write(dir.join(format!("{name}.cadena")), &g.cadena_original).unwrap();
        };
        write("ingreso-retenciones", &cfdi::generate(&invoice(), Some(&csd)).unwrap());
        write("pago-total", &generate(&recibo(dec!(1453.33), dec!(1453.33), 1), Some(&csd)).unwrap());
        write("pago-parcial", &generate(&recibo(dec!(726.67), dec!(1453.33), 1), Some(&csd)).unwrap());
    }

    #[test]
    fn sealed_and_checked() {
        let csd = Csd::from_der(CER, KEY, "12345678a").unwrap();
        let g = generate(&recibo(dec!(1453.33), dec!(1453.33), 1), Some(&csd)).unwrap();
        assert!(g.cadena_original.starts_with("||4.0|P|9|2026-06-20T10:00:00|30001000000500003416|0|XXX|"));
        assert!(g.xml.contains("Sello=\"") && g.xml.contains("Certificado=\""));
        let mut bad = recibo(dec!(10), dec!(5), 1);
        assert!(generate(&bad, None).unwrap_err().contains("balance of"));
        bad = recibo(dec!(10), dec!(100), 1);
        bad.pagos[0].forma_de_pago = "99".into();
        assert!(generate(&bad, None).unwrap_err().contains("never 99"));
        bad = recibo(dec!(10), dec!(100), 1);
        bad.pagos[0].documentos[0].id_documento = "A-77".into();
        assert!(generate(&bad, None).is_err());
        bad = recibo(dec!(10), dec!(100), 1);
        bad.pagos[0].monto = dec!(9);
        assert!(generate(&bad, None).unwrap_err().contains("more than the payment"));
        let mut sub = recibo(dec!(10), dec!(100), 1);
        sub.sustituye = vec![UUID.to_lowercase()];
        assert!(generate(&sub, None).unwrap().xml.contains("<cfdi:CfdiRelacionados TipoRelacion=\"04\"><cfdi:CfdiRelacionado UUID=\"5FB2822E"));
        assert!(is_uuid(UUID) && !is_uuid("5FB2822E396D47258521CDC4BDD20CCF"));
    }
}

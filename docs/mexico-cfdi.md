# Mexico pack (`khata-mx`): IVA, CFDI 4.0, payments and PAC timbrado

KhataOS can keep books for a Mexican taxpayer: it checks RFCs with the SAT
check digit, charges IVA at 16%, 8% (border region), 0% or exempt, puts the
ISR and IVA a customer withholds on the CFDI, works out a monthly IVA
summary with withholdings, and writes **CFDI 4.0 XML** (Anexo 20) with its
cadena original and the **sello** made with the issuer's CSD, including the
**Recibo Electrónico de Pago** (CFDI de tipo P with the Complemento de
Pagos 2.0) for PPD invoices. A **PAC connector** sends the sealed XML to a
PAC for timbrado (adapter for SW Sapien's REST API, plus a mock PAC for
trying the flow), keeps the folio fiscal (UUID) and asks the PAC to cancel.
Nothing here is tax advice.

Code: `crates/khata-mx` (`rfc.rs`, `states.rs`, `iva.rs`, `cfdi.rs`,
`retenciones.rs`, `pagos.rs`, `timbre.rs`, `returns.rs`),
`apps/desktop/src-tauri/src/tax_mx.rs` (vouchers into the crate's
documents, issuer settings, CSD storage; the `iva_summary_mx`,
`mx_issuer`, `set_mx_issuer`, `mx_csd_status`, `set_mx_csd`,
`clear_mx_csd`, `cfdi_xml` and `pago_xml` commands),
`apps/desktop/src-tauri/src/pac_client.rs` (the PAC connector: the
`pac_settings`, `set_pac_settings`, `pac_record`, `pac_stamp`,
`pac_cancel` and `pac_confirm_cancellation` commands) and
`apps/desktop/src/screens/tax_mx.ts` (`MxIvaSummaryReport`, `MxCfdiList`,
`exportCfdi`, `exportPago`, `stampWithPac`, `cancelWithPac`,
`confirmCancellation`, `editPacSettings`, `editIssuer`, `installCsd`).
Sources, confidence tags and open questions:
`docs/research/mexico-compliance.md` (§13 for this round).

## Setting up a Mexican company

1. Pick **Mexico**. The books use MXN (2 decimals) and **calendar years**:
   the ejercicio fiscal is the calendar year (CFF art. 11).
2. **Company → Company details** (Alt+K): the **RFC** (12 characters for a
   persona moral, 13 for a persona física; the last character is the SAT
   check digit, from the SAT's published algorithm; a real RFC that fails
   the check can be kept with the tax-ID override and a reason) and the
   **state** (ISO 3166-2:MX code: `JAL`, `CMX`,
   `NLE`…). The state never changes the IVA rate.
3. **Rates** (also `khata_mx::iva::default_rates`): `STANDARD` 16 from
   2010-01-01, `BORDER` 8 from 2019-01-01 and back to 16 from 2027-01-01
   (the decrees were last extended to 31 Dec 2026, DOF 31 Dec 2025; if
   they are extended again, delete that change), `ZERO` 0, `EXEMPT` 0.
   Choosing
   `BORDER` is your statement that the border-region decree applies to the
   supply (registration in the padrón, delivery in the region, goods not
   excluded); KhataOS does not check those conditions.
4. **Duty ledgers**: "IVA trasladado" (output) and "IVA acreditable"
   (input), type of tax `IVA`.
5. **Issuer details** (CFDI list → Ctrl+Alt+E): régimen fiscal
   (c_RegimenFiscal, e.g. `601`), lugar de expedición (the issuing
   place's postal code), serie (optional) and the name exactly as on the
   Constancia de Situación Fiscal (blank: the company name). These live in
   the company file's settings; they are not secret.
6. **CSD** (CFDI list → Ctrl+Alt+K): pick the SAT `.cer` and `.key` and type
   the key's password (DER or PEM encrypted PKCS#8; PBES2 keys open, the
   legacy PKCS#12 PBE scheme does not [UNCERTAIN which one current SAT keys
   use]). KhataOS checks that the key opens and belongs to
   the certificate, then copies both files to this computer's app data
   directory (`mx-csd/<RFC>.cer` and `.key`, readable only by your user,
   folder `0700`). The key stays encrypted as the SAT issued it, and **the
   password is never stored**: it is asked for each time you seal a CFDI.
   The CSD is never written to the company file, synced or sent anywhere,
   and the decrypted key lives in memory only for the signing call
   (Anexo 20 §I.B asks for exactly that).
7. **Customers**: the party's RFC as its tax ID and its **postal code** in
   the address (DomicilioFiscalReceptor). Without an RFC the CFDI goes to
   `XAXX010101000` (público en general: régimen 616, uso S01, your own
   postal code); a party abroad without one gets `XEXX010101000`.
8. **Customers that withhold** (a persona moral paying your honorarios or
   rent as a persona física, a RESICO, freight): put a `RET:` class on the
   customer ledger's tax class, see [Withholdings](#withholdings-retenciones).
   Book what they withhold on duty ledgers whose type of tax is `RET-ISR`
   and `RET-IVA` ("ISR / IVA retenido por clientes", debits on the sale);
   what you withhold from your own suppliers on `RET-ISR` / `RET-IVA`
   ledgers credited on the purchase ("… retenido por enterar").
9. **PAC** (CFDI list → Ctrl+Alt+P): provider (`sw` or `mock`),
   environment (`test`, `production`, or `custom` with a base URL), user
   and password or account token, see [PAC timbrado](#pac-timbrado).

## CFDI 4.0 export

The CFDI list (`MxCfdiList`) shows the period's Sales, Credit Notes and
Receipts with their folio fiscal and status. Enter on a Sales or Credit
Note asks for the forma de pago, método de pago (`PUE`/`PPD`; PPD forces
forma de pago `99`), uso del CFDI, the customer's régimen fiscal, for a
credit note the UUID of the invoice it corrects (TipoRelacion `01`; for an
invoice, the UUID of a cancelled CFDI it replaces, TipoRelacion `04`), the
withholdings (blank: the customer's class) and the CSD password, then saves
the XML (`write_gst_json` accepts `.xml`). Enter on a Receipt makes the
payment receipt (below).

What is written:

- `cfdi:Comprobante` with `xmlns:cfdi="http://www.sat.gob.mx/cfd/4"`,
  `xmlns:xsi` and `xsi:schemaLocation="http://www.sat.gob.mx/cfd/4
  http://www.sat.gob.mx/sitio_internet/cfd/4/cfdv40.xsd"`; `Version="4.0"`,
  Serie, Folio (the voucher number's letters and digits), Fecha
  (`AAAA-MM-DDThh:mm:ss`: the voucher date and the time of export),
  FormaPago, NoCertificado, Certificado, SubTotal, Descuento, Moneda `MXN`,
  Total, TipoDeComprobante (`I` Sales / `E` Credit Note), Exportacion `01`,
  MetodoPago, LugarExpedicion, Sello;
- CfdiRelacionados (credit notes), Emisor (Rfc, Nombre, RegimenFiscal),
  Receptor (Rfc, Nombre, DomicilioFiscalReceptor, RegimenFiscalReceptor,
  UsoCFDI);
- one Concepto per item or service line: ClaveProdServ, Cantidad,
  ClaveUnidad (`H87` goods / `E48` services), Descripcion, ValorUnitario,
  Importe, ObjetoImp `02`, and its IVA Traslado (Base, Impuesto `002`,
  TipoFactor `Tasa` with TasaOCuota `0.160000`/`0.080000`/`0.000000` and
  Importe, or `Exento` with neither);
- each concepto's Retenciones (Base, Impuesto `001` ISR / `002` IVA,
  TipoFactor `Tasa`, TasaOCuota, Importe), after its Traslados;
- the comprobante's Impuestos: TotalImpuestosRetenidos and
  TotalImpuestosTrasladados; one Retencion per impuesto, then one Traslado
  per rate (and one Exento with its base). Total = SubTotal − Descuento +
  traslados − retenciones.

**Cadena original and sello.** The cadena is built in Rust following the
SAT XSLT (`cadenaoriginal_4_0.xslt`) for exactly the nodes above: `||` at
both ends, `|` between values, attributes in the XSLT's order, absent
optional attributes skipped, whitespace normalised (`normalize-space`);
NoCertificado is in it, Sello and Certificado are not. The sello is RSA
PKCS#1 v1.5 with SHA-256 over the cadena's UTF-8 bytes, in base64 (checked
against `openssl dgst -sha256 -sign` in the tests). NoCertificado is the
certificate's serial number read as ASCII digits.

The port was checked during development against the SAT's own
`cadenaoriginal_4_0.xslt` (xsltproc) and `cfdv40.xsd` (xmllint): the
generated documents (an ingreso with a discount, 16% and exento lines; an
egreso with CfdiRelacionados) validate and their cadenas match the XSLT's
byte for byte, and the research document's xsltproc golden cadena is a
unit test. It has not been through a PAC yet.

**Without a password** the XML is an unsealed draft (no Sello,
NoCertificado or Certificado) for checking.

**Conformance.** The documents of this round (an ingreso with ISR and IVA
retenciones and an exento line; full and partial payment receipts, all
sealed) were checked on 2026-10-21 against the SAT's own
`cadenaoriginal_4_0.xslt` + `Pagos20.xslt` (xsltproc: cadenas identical
byte for byte) and `cfdv40.xsd` + `Pagos20.xsd` with the catalogues
(xmllint: valid; a deliberately broken copy fails). To repeat it, run
`KHATA_MX_DUMP=<dir> cargo test -p khata-mx dump_for_conformance` and
apply the SAT files (a local copy of the 4.0 XSLT that includes only
`utilerias.xslt` and `Pagos20.xslt`) to the files written.

**Known gaps on the CFDI**: ClaveProdServ is `01010101` ("no existe en el
catálogo") because items carry no SAT product key yet; discounts on the
party's side are not put on the CFDI; no IEPS, InformacionGlobal (factura
global), Comercio Exterior for exports, or foreign-currency payments.

**Foreign currency**: a voucher with lines in a foreign currency gets no
CFDI (nor a payment complement) yet: KhataOS refuses it ("A CFDI for an
invoice in USD is not supported yet…", `forex::refuse_foreign_einvoice`).
CFDI 4.0 does allow it, with `Moneda` and `TipoCambio` (and `MonedaP` /
`TipoCambioP` / `EquivalenciaDR` in Pagos 2.0), but the CFDI is written in
MXN, which would not match the invoice printed in the foreign currency.
It stays refused until those fields are written.

## Withholdings (retenciones)

What a customer withholds is the **customer ledger's tax class**,
`RET:` followed by tokens joined with `+` (`khata_mx::retenciones`; the
class limit is 24 characters):

| Class | Case | On the CFDI |
|-------|------|-------------|
| `RET:ISR10+IVA2/3` | honorarios or rent of a persona física paid by a persona moral (LISR arts. 106, 116; LIVA art. 1-A fr. II a; RLIVA art. 3 fr. I) | ISR `0.100000`; IVA `0.106667` (two thirds of 16%; `0.053333` on 8%) |
| `RET:RESICO+IVA2/3` | the same from a RESICO persona física (LISR art. 113-J) | ISR `0.012500`; IVA `0.106667` |
| `RET:RESICO` | goods from a RESICO persona física | ISR `0.012500` |
| `RET:IVA4` | freight (autotransporte terrestre de bienes; RLIVA art. 3 fr. II) | IVA `0.040000` |
| `RET:IVAALL` | all the IVA (e.g. LIVA art. 1-A fr. III) | IVA at the line's rate |
| `RET:NONE` | nothing (overrides) | — |

Any `ISR<n>` (n ≤ 35) or `IVA<n>` (n ≤ 16) also works. ISR applies to every
concepto; IVA withholdings only to conceptos taxed at a positive rate. The
CFDI dialog's "Withholdings" field overrides the class for one CFDI. The
Importe is Base × TasaOCuota rounded to the centavo (the value a PAC
checks); with `0.106667` it can differ from exactly two thirds by a
centavo on large bases [UNCERTAIN which one the SAT prefers]. The export
warns when the withholding booked (`RET-ISR`/`RET-IVA` ledgers) differs
from the CFDI's. Whether a withholding applies is your call.

## Payment receipts (Complemento de Pagos 2.0)

An invoice issued **PPD** (pago en parcialidades o diferido, FormaPago 99)
needs a payment receipt for every payment. In KhataOS that is a **Receipt
voucher** whose customer line is allocated **bill by bill**: one Against
reference per invoice paid (the reference is the bill the Sales voucher
raised, or its number). `pago_xml(voucher, options)` (Enter on a Receipt in
the CFDI list) writes a CFDI de tipo **P**:

- the comprobante as the Guía de llenado fixes it: `SubTotal="0"`,
  `Moneda="XXX"`, `Total="0"`, `TipoDeComprobante="P"`, `Exportacion="01"`,
  no FormaPago/MetodoPago; UsoCFDI `CP01`; one Concepto `84111506`, `1`,
  `ACT`, `Pago`, `0`, `0`, ObjetoImp `01`; TipoRelacion `04` with the UUID
  of an earlier receipt it replaces, if given;
- `pago20:Pagos Version="2.0"` → Totales (TotalRetencionesIVA/ISR,
  TotalTrasladosBase/ImpuestoIVA16, IVA8, IVA0, BaseIVAExento,
  MontoTotalPagos) → one Pago (FechaPago = the voucher date at 12:00:00,
  FormaDePagoP = how the money came in, never 99, MonedaP `MXN`,
  TipoCambioP `1`, Monto, NumOperacion) → one **DoctoRelacionado** per
  invoice: **IdDocumento = the UUID of the invoice as stamped** through
  the PAC connector (or one you give for an invoice stamped elsewhere,
  "bill=UUID"), Serie, Folio, MonedaDR `MXN`, EquivalenciaDR `1`,
  NumParcialidad (1 + earlier Receipts against the bill), ImpSaldoAnt (the
  invoice Total less earlier Against allocations), ImpPagado,
  ImpSaldoInsoluto, ObjetoImpDR, and ImpuestosDR; then the Pago's
  ImpuestosP;
- the taxes paid are the invoice's in proportion to the amount paid
  (BaseDR = base × ImpPagado / Total, ImporteDR = BaseDR × TasaOCuotaDR,
  each to the centavo), from the invoice's stamp record, so a receipt
  matches the invoice as the SAT has it.

Refused: a receipt without Against allocations, an invoice stamped PUE,
an invoice not stamped (and no UUID given), an invoice being cancelled or
cancelled, an allocation above the balance. Other allocations (advance, on
account) are left out with a warning. Only MXN.

## PAC timbrado

A CFDI is valid only once a PAC certifies it and adds the
**TimbreFiscalDigital 1.1** (UUID, FechaTimbrado, RfcProvCertif, SelloCFD,
NoCertificadoSAT, SelloSAT). In the CFDI list, **Ctrl+Alt+T** asks for the
CFDI's details and the CSD password, seals the XML and sends it
(`pac_stamp`); the stamped XML can be saved. A voucher already stamped is
not sent again. An invoice's Fecha is the voucher date with the current
time, so stamp it within a few days: PACs are reported to refuse CFDIs
dated more than 72 hours earlier [UNCERTAIN, §4.2 of the research]; a
payment receipt's Fecha is the moment it is made.

Providers (`pac_client.rs`; the interface is `PacConnector`, `stamp` and
`cancel`):

- **`sw`: SW Sapien REST**, from SW's public SDK (`lunasoft/sw-sdk-php`):
  sign-in `POST /v2/security/authenticate` (`user`, `password` → token;
  or an account token in the settings), stamp `POST /cfdi33/stamp/v4`
  (multipart field `xml`, `Authorization: Bearer`; the 3.3-era path name is
  used for 4.0), cancel `POST /cfdi33/cancel/csd` or
  `/cfdi33/cancel/{rfc}/{uuid}/{motivo}/{folio}`. Test
  `https://services.test.sw.com.mx`, production `https://services.sw.com.mx`.
  **Both need an SW account; no PAC offers a public sandbox without
  credentials, so no live call has been made**: the adapter is tested
  against a local mock server built from the SDK's request and response
  shapes [MEDIUM].
- **`mock`**: stamps on this computer with a made-up UUID and the Leyenda
  "PAC simulado: sin validez fiscal"; its records are marked as tests and
  print "SIN VALIDEZ FISCAL". For trying the flow, never for customers.

Other PACs (Finkok's SOAP `stamp`/`cancel` services, for one) can be added
as `PacConnector` adapters.

**Where things are kept.** The PAC credentials in `mx-pac.json` in the app
data directory, keyed by RFC, readable only by your user (0600), never in
the company file, sync, exports or the webview; the session token in
memory. The stamp (UUID, FechaTimbrado, SelloSAT, NoCertificadoSAT,
RfcProvCertif, the stamped XML, the invoice's total, método de pago and
taxes for its payment receipts, cancellation status) in the company
as a shared record under `mx.cfdi.<voucher id>`, like the IRP records: not
in the audit trail, synced with the books (`docs/sync-settings.md`).

**Cancellation** (Ctrl+Alt+X, `pac_cancel`): motivo `01` (errors, with a
replacement: issue the replacement first and give its UUID as folio de
sustitución; relate the replacement with TipoRelacion `04`), `02` (errors,
no replacement), `03` (operation not carried out), `04` (nominative
operation included in a factura global). With the CSD password the CSD
goes with the request (SW's cancellation by CSD, over HTTPS to the PAC);
without it the PAC must hold your CSD. The SAT answers `201` (request
received) or `202` (already cancelled). When the receiver's acceptance is
needed (CFF art. 29-A; RMF 2.7.1.34–35: exceptions by amount and type,
which sources describe differently [UNCERTAIN], so KhataOS does not
encode them) the CFDI stays "cancellation requested" until you record the
SAT's answer (Ctrl+Alt+V: cancelled, or still in force when rejected). A
CFDI being cancelled takes no payment receipts.

**On prints.** `pac_client::print_block_html(slot, voucher)` gives the
printed representation's block: folio fiscal, fecha de certificación, the
CSD's and the SAT's certificate numbers, the sellos, the TFD's cadena
original and the QR code
(`https://verificacfdi.facturaelectronica.sat.gob.mx/default.aspx?id=…&re=…&rr=…&tt=…&fe=…`,
Total with trailing zeros dropped, last 8 characters of the sello, as
phpcfdi does [MEDIUM]), with "CFDI CANCELADO" or "SIN VALIDEZ FISCAL" when
so.

## Monthly IVA summary

`iva_summary_mx(from, to)` gives actos o actividades by treatment (16%,
8%, other rate, 0%, exempt) for sales and purchases, IVA trasladado, **IVA
retenido por clientes** (what customers withheld: it comes off what you
owe), IVA acreditable (purchases from suppliers with an RFC; a CFDI is
needed to credit IVA), and IVA a cargo (trasladado − retenido por
clientes − acreditable) or saldo a favor. Separately: **IVA and ISR
retenidos a enterar** (what you withheld from suppliers: paid over by the
17th of the next month, LIVA art. 1-A fr. IV, LISR arts. 106/113-J; the
IVA withheld is creditable once paid over, LIVA art. 5 fr. IV, which the
summary takes as the same month [MEDIUM]) and **ISR retenido por
clientes** (for your ISR provisional payment). Withholdings come from the
`RET-IVA`/`RET-ISR` ledgers booked on the vouchers; when none is booked
and the party has a `RET:` class, the class is applied with a warning. It
works on **document dates**, while IVA is due when payments are actually
collected or made (LIVA arts. 1-B and 5): PPD invoices settled in a later
month belong to that month. Earlier balances, proportional crediting and
IVA on imports are not worked out. The declaration is due by the 17th of the
following month (LIVA art. 5-D); keep the records for 5 years (CFF
art. 30).

## Future work

- The SAT's public status service (`ConsultaCFDIService`, by the QR
  expression) to learn a cancellation's outcome instead of recording it
  by hand; more PAC adapters (Finkok SOAP…).
- PPD cash-basis IVA in the summary from the payment receipts; DIOT;
  Contabilidad electrónica (Anexo 24: catálogo de cuentas with código
  agrupador, balanza de comprobación XML).

## Where it is in the app

- Go To (Alt+G), Mexican companies only: **IVA summary (Mexico)**
  (`MxIvaSummaryReport`) and **CFDI invoices (Mexico)** (`MxCfdiList`).
- On the CFDI list, Enter exports the CFDI under the cursor (for a Receipt,
  the CFDI de Pago), Ctrl+Alt+T stamps it through the PAC, Ctrl+Alt+X asks
  the PAC to cancel it, Ctrl+Alt+V records the SAT's answer to a
  cancellation, Ctrl+Alt+P sets up the PAC, Ctrl+Alt+E edits the issuer
  details and Ctrl+Alt+K installs the CSD.
- On a saved Sales invoice or Credit Note, Ctrl+E (Export) → **CFDI 4.0**
  saves the sealed XML (`exportCfdi`). Stamping is on the CFDI list.
- The invoice print (`invoice_design.rs`) splices in
  `pac_client::print_block_html` for Mexican Sales, Credit Note and Receipt
  vouchers, so a stamped CFDI prints the folio fiscal (UUID), the SAT's
  certificate and sello, the TFD cadena and the verification QR, with
  "SIN VALIDEZ FISCAL (PAC simulado)" for mock stamps and "CFDI CANCELADO"
  once cancelled.
- The commands `pago_xml`, `pac_settings`, `set_pac_settings`,
  `pac_record`, `pac_stamp`, `pac_cancel` and `pac_confirm_cancellation`
  are registered in `lib.rs`.

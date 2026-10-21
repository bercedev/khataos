# Spain IVA pack (`khata-es`)

KhataOS can keep books for a Spanish business. It:
- checks the NIF in all its forms;
- charges IVA at 21%, 10% or 4%;
- handles inversión del sujeto pasivo;
- checks invoice numbers and their series;
- works out the modelo 303 from the books;
- generates VeriFactu billing records: the SHA-256 huella chain, the
  record XML and the QR code URL.

Nothing is sent to the AEAT. Nothing here is tax advice: check the
figures with your asesor before you file.

Code:
- `crates/khata-es` holds the rules: `nif.rs`, `comunidades.rs`, `iva.rs`,
  `returns.rs`, `verifactu.rs`.
- `apps/desktop/src-tauri/src/vat_es.rs` turns vouchers into 303 documents
  and VeriFactu records. It provides the commands `modelo_303_es(from, to)`,
  `verifactu_record(voucher_id)`, `verifactu_system()` and
  `set_verifactu_system(settings)`. It reads vouchers with the shared
  reader in `vat_fr.rs`.
- `apps/desktop/src/screens/vat_es.ts` provides `Modelo303Report` (Go To
  → **Modelo 303 (Spain VAT return)**) and `exportVerifactu` (Ctrl+E on a
  saved Sales invoice or Credit Note → **VeriFactu record**).

Sources and open questions are in
[`docs/research/spain-compliance.md`](research/spain-compliance.md).
Tags:
- **[PRIMARY]**: read in the BOE or an AEAT document.
- **[SECONDARY]**: from python-stdnum, Wikipedia or other non-official
  sources.
- **[UNCERTAIN]**: not confirmed.

## Setting up a Spanish company

1. Pick **Spain** as the country. The books use EUR and **calendar
   years**. The Impuesto sobre Sociedades period is the company's own
   year, at most 12 months (LIS art. 27); IVA periods are calendar
   quarters or months either way.
2. In **Company → Company details** (Alt+K), enter the **NIF** and pick the
   **comunidad**. The codes are ISO 3166-2:ES without "ES-": `AN`, `AR`,
   `AS`, `CB`, `CL`, `CM`, `CN`, `CT`, `EX`, `GA`, `IB`, `MC`, `MD`, `NC`,
   `PV`, `RI`, `VC`, `CE`, `ML`.
   - **Canarias** (IGIC), **Ceuta** and **Melilla** (IPSI) are outside the
     IVA territory (LIVA art. 3). A company there gets an error. A sale to
     a party there is 0% (an export).
   - **País Vasco** and **Navarra** apply the common rates, but their
     haciendas forales collect the IVA. Bizkaia, Gipuzkoa and Álava use
     TicketBAI / Batuz, not VeriFactu; this is not modelled.
3. Add the rate list (also in `khata_es::iva::default_rates`):

   | Class | Rate | From | Notes |
   |---|---|---|---|
   | `STANDARD` | 21% | 1 Sep 2012 | 18% from 1 Jul 2010 in the list [PRIMARY] |
   | `REDUCED` | 10% | 1 Sep 2012 | 8% before |
   | `SUPERREDUCED` | 4% | | bread, milk, books, medicines; olive oil from 2025 |
   | `ZERO` | 0% | | the temporary food measure (until 30 Sep 2024) |
   | `EXEMPT` | 0% | | art. 20 exemptions |
   | `REVERSE` | 0% | | inversión del sujeto pasivo (art. 84.Uno.2º) |

   The temporary 5% (food and energy, to 30 Sep 2024), and the 2% and 7.5%
   of Oct–Dec 2024, are classes you add if old periods are booked.

   The furnished short lets of RDL 29/2026 (10% from 1 Dec 2026) await
   convalidation [PRIMARY, research §2].

## Tax identifiers

All NIFs are nine characters [PRIMARY for the DNI rule; SECONDARY for the
rest, python-stdnum]:

| Kind | Form | Check character |
|---|---|---|
| DNI | eight digits and a letter | `TRWAGMYFPDXBNJZSQVHLCKE`[n mod 23] |
| NIE | X, Y or Z, seven digits, a letter | X/Y/Z read as 0/1/2, then the DNI rule |
| K, L, M | the letter, seven digits, a letter | the DNI letter of the seven digits |
| Entity (former CIF) | a letter for the legal form (Orden EHA/451/2008), seven digits, a control | over the seven digits: even positions added to the digit sums of twice the odd positions; control = (10 − sum mod 10) mod 10, or `JABCDEFGHI`[control] |

- **Entity controls**: which legal forms use a letter or a digit is
  [UNCERTAIN] (sources conflict), so both are accepted, as python-stdnum
  does.
- **ES prefix**: an `ES` prefix (the NIF-IVA) is accepted and dropped.
- **Test vectors**: 54362315K, X2482300W, B58378431, A58818501, Q2826000H,
  89890001K (the AEAT's VeriFactu test NIF).
- **Other EU VAT identifiers** are accepted on their shape only.

## Invoices

- **Numbers and series** (RD 1619/2012 art. 6.1.a): "Número y, en su caso,
  serie. La numeración de las facturas dentro de cada serie será
  correlativa" [PRIMARY].
  - **Separate series** are mandatory for rectificativas, for invoices
    issued by the recipient or a third party, and for simplified and full
    invoices in the same year.
  - **Validation**: the pack requires a number to be unique in its series
    across all years, and at most 60 characters (VeriFactu's
    NumSerieFactura) of `[A-Za-z0-9/-]`, inside VeriFactu's printable
    ASCII.
  - **Correlative check**: `iva::correlative_gap` warns when a hand-typed
    number does not follow its series, or an earlier number is dated
    after it. `verifactu_record` reports it.
  - **Recommended series**: KhataOS numbers each voucher type in its own
    gap-free series. Give Credit Notes their own prefix (`R-…`) so
    rectificativas are a separate series.
- **Printed invoices** must carry:
  - "inversión del sujeto pasivo" on a REVERSE line;
  - from the VeriFactu dates, the QR code with "QR tributario:" above it,
    and "VERI*FACTU" or "Factura verificable en la sede electrónica de la
    AEAT" below it (art. 6.5, added by RD 1007/2023).

## Modelo 303

The desktop's `modelo_303_es(from, to)` reads Sales and Credit Notes as
outward and Purchases and Debit Notes as inward. Tax ledgers of type "IVA"
(or "VAT") give the IVA. Credit notes are facturas rectificativas.
`khata_es::returns::modelo_303` turns them into casillas
[PRIMARY: AEAT Instrucciones 2026, diseño DR303e26v101]:

| Casillas | What goes in them |
|---|---|
| 01–03 / 04–06 / 07–09 | 4% / 10% / 21%: base and cuota (base × tipo, with a warning when the IVA booked differs) |
| 150–152, 153–155 | 0% and 5% (temporary measures; the 2026 instructions give 153–155 no rate [UNCERTAIN]) |
| 10/11 | Adquisiciones intracomunitarias de bienes y servicios (self-assessed) |
| 12/13 | Otras operaciones con inversión del sujeto pasivo: REVERSE purchases from Spain, services from outside the EU |
| 14/15 | Modificación de bases y cuotas: the company's rectificativas |
| 27 | Total cuota devengada |
| 28/29 | Operaciones interiores corrientes, including the self-assessed IVA of 12/13 and of EU services |
| 36/37 | Adquisiciones intracomunitarias de bienes corrientes |
| 40/41 | Rectificación de deducciones: suppliers' rectificativas |
| 45 / 46 | Total a deducir; resultado régimen general (27 − 45) |
| 59 / 60 | Entregas intracomunitarias de bienes y servicios; exportaciones |
| 120 / 122 | No sujetas por reglas de localización; sujetas con inversión del sujeto pasivo (sales) |
| 64, 65, 66, 69, 71 | Results: 65 is 100% (no foral share); no amounts carried from earlier periods |

Not worked out (left at zero):
- the recargo de equivalencia (16–26, 156–158, 168–170);
- bienes de inversión (30/31, 38/39) and importaciones (32–35, 77);
- the regularizaciones (43, 44, 68), cuotas a compensar (110, 78, 87),
  casilla 112;
- the régimen simplificado, the criterio de caja and the prorrata.

Filing is quarterly (by the 20th of April, July and October, and the 30th
of January), or monthly for REDEME, large companies and the SII.

**SII.** Companies in the SII (turnover over €6 M, REDEME, VAT groups)
report invoices within 4 days and are outside VeriFactu. The SII is not
modelled.

## VeriFactu

Real Decreto 1007/2023 sets the requirements for billing software:
integridad, conservación, accesibilidad, legibilidad, trazabilidad and
inalterabilidad. Orden HAC/1177/2024 fixes the record layout.

Current dates [PRIMARY: RDL 15/2025, BOE 3 Dec 2025]:
- before **1 Jan 2027** for Impuesto sobre Sociedades taxpayers;
- before **1 Jul 2027** for everyone else.

Earlier dates were 1 Jul 2025 (RD 1007/2023) and 1 Jan / 1 Jul 2026 (RD
254/2025).

`khata_es::verifactu` generates the records:

- **Registro de alta** for an invoice. It follows the element order of the
  AEAT's `SuministroInformacion.xsd` v1.0, in namespace `sum1`:
  - `IDFactura`, `NombreRazonEmisor`, `TipoFactura`;
  - for a rectificativa, `TipoRectificativa` "I" and
    `FacturasRectificadas`;
  - `DescripcionOperacion`, `Destinatarios`;
  - `Desglose`, at most 12 lines: S1, S2, E1–E6, N2;
  - `CuotaTotal`, `ImporteTotal`, `Encadenamiento`, `SistemaInformatico`;
  - `FechaHoraHusoGenRegistro`, `TipoHuella` 01, `Huella`.
- **Registro de anulación** for a cancelled invoice.
- **The huella** is SHA-256 over the UTF-8 string
  `IDEmisorFactura=…&NumSerieFactura=…&FechaExpedicionFactura=dd-mm-yyyy&TipoFactura=…&CuotaTotal=…&ImporteTotal=…&Huella=<previous>&FechaHoraHusoGenRegistro=yyyy-mm-ddThh:mm:ss+hh:mm`.
  - The anulación string is `IDEmisorFacturaAnulada=…&NumSerieFacturaAnulada=…&FechaExpedicionFacturaAnulada=…&Huella=…&FechaHoraHusoGenRegistro=…`.
  - The result is written as 64 upper-case hex characters.
  - Values are trimmed. The first record has `Huella=` empty.
  - Amounts are written with two decimals, in the hash and in the XML
    alike.
- **The golden examples**: the AEAT's three worked examples (a first alta,
  a chained second alta, an anulación) are tests, and all three
  reproduce. The first is `3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60`
  [PRIMARY: *Detalle de las especificaciones técnicas para generación de
  la huella o hash*].
- **The QR URL** is
  `https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR?nif=…&numserie=…&fecha=dd-mm-yyyy&importe=…`.
  - The test host is `prewww2.aeat.es`. Without VeriFactu mode the path
    is `ValidarQRNoVerifactu`.
  - Values are encoded as Java's `URLEncoder` does, which the AEAT's QR
    specification names.
  - The QR image itself (ISO/IEC 18004, level M, 30–40 mm) is left to the
    print code.

### In the desktop

`verifactu_record(voucher_id)` keeps the chain per installation, as the
ZATCA device chain does. The settings live in the company file, outside
the audit trail, and are never synced:

| Setting | What it holds |
|---|---|
| `verifactu.chain` | Record count, the last record's id and huella, and its time |
| `verifactu.doc.<voucher id>` | The voucher's alta, its XML and QR URL. Asking again returns the same record; if the voucher changed, a warning says to issue a rectificativa |
| `verifactu.cancel.<voucher id>` | The anulación of a cancelled voucher that has an alta |
| `verifactu.system` | The SistemaInformatico (producer name and NIF, system name "KhataOS", id "KO", version, a generated installation number) and the test-QR flag |

How a record is made:
- The chain is written before the record, so a crash never reuses a
  huella.
- **TipoFactura**: Sales → F1, or F2 when the buyer has no Spanish NIF;
  Credit Note → R1, or R5 for a simplified one.
- **Warnings**:
  - the producer fields are blank;
  - the invoice is dated before 28-10-2024;
  - the previous record is more than a minute later (Orden art. 7.i);
  - the number breaks its series;
  - foreign buyers (`IDOtro`) are not written yet.
- **Foreign currency**: a voucher with lines in a foreign currency gets no
  record yet ("A VeriFactu record for an invoice in USD is not supported
  yet…", `forex::refuse_foreign_einvoice`). The record holds the books'
  EUR figures, which would not match an invoice printed in the foreign
  currency, so it stays refused until the two are made to agree.

Future work:
- **Submission**: the VERI*FACTU SOAP service at
  `www1.agenciatributaria.gob.es/wlpl/TIKE-CONT/ws/SistemaFacturacion/VerifactuSOAP`
  needs a qualified certificate. Records are sent in batches of up to
  1,000, with the AEAT's wait time, and retried.
- **The "no VERI*FACTU" mode**: XAdES-signed records and the event log
  with its own chain.
- **The rest**: `Subsanacion`, `IDOtro` for foreign buyers, the QR image
  on the printed invoice, the producer's declaración responsable, and
  validation against the XSD.

## Where it is in the app

- **Modelo 303:** Go To (Alt+G) → **Modelo 303 (Spain VAT return)**, shown
  only for Spanish companies.
- **VeriFactu:** on a saved Sales invoice or Credit Note, Ctrl+E (Export) →
  **VeriFactu record** saves the record XML and shows its huella and QR
  address. The same menu offers **Factur-X / ZUGFeRD PDF**.
- Still open: no record is made when a voucher is saved (calling
  `esApi.verifactuRecord` at save time is the next step), the QR is not on
  the printed invoice, and there is no settings screen for
  `esApi.verifactuSystem` / `setVerifactuSystem`.

## Next steps

- Record at save time, the QR on the printed invoice, AEAT submission.
- Modelo 349 and 390; the recargo de equivalencia; the SII.
- B2B e-invoicing: RD 238/2026 makes UBL the reference syntax, and Orden
  HAC/1028/2026 starts the clock (about Oct 2027 for companies over €8 M
  and about Oct 2028 for others). These belong to the shared
  `khata-en16931` crate.

# Mexico Compliance Requirements (IVA, RFC, CFDI 4.0): KhataOS Spec

> **Status as of:** 2026-10-20. **Scope:** Mexican *personas morales* and *personas físicas con actividad empresarial* keeping books in KhataOS, a local-first desktop accounting app. Covers IVA, RFC validation, CFDI 4.0 generation (XML, cadena original, sello) and the main SAT reporting obligations.
> **Not legal advice.** This file turns Mexican statutes, the SAT's Anexo 20 and the SAT's published XSD/XSLT files into software requirements. Get a Mexican *contador público* to confirm each item before release. Re-check the SAT catalogues (`catCFDI`) and the RMF every quarter: SAT revises them several times a year.
>
> **Confidence tags:**
> - **[HIGH]** means confirmed against primary text retrieved in this research: the SAT's XSD/XSLT files under `sat.gob.mx/sitio_internet/cfd/…`, Anexo 20 (DOF 13 Jan 2022), the *Guía de llenado* on `omawww.sat.gob.mx`, the SAT catalogue workbook, the LIVA/RLIVA text in the Cámara de Diputados compilation, or a computation run and checked here.
> - **[MEDIUM]** means reported consistently by reputable secondary sources (Big 4, law firms, established vendors, Wikipedia, python-stdnum), or primary text read only through an unofficial mirror.
> - **[LOW]** means a single secondary source.
> - **UNCERTAIN** means sources conflict, the text was not found, or the point is interpretive. Flagged for review.
>
> **Access note:** `diputados.gob.mx` and `dof.gob.mx` could not be reached from the research environment. Statute text was read from Cámara de Diputados compilations hosted elsewhere (SEP *normateca*, gob.mx) and from mley.mx. Each such citation says so.

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **RFC validation**: 12 characters (moral) / 13 (física), pattern from `tdCFDI:t_RFC`, mod-11 check digit (Anexo III table). A failed check is a **warning**, not a block. Generic RFCs `XAXX010101000` / `XEXX010101000` are whitelisted | SAT/IFAI algorithm document; `tdCFDI.xsd` (§3) |
| 2 | **IVA rates** 16% / 8% (border stimulus, effective-dated to 2026-12-31) / 0% / exempt, kept as separate tax classes | LIVA arts 1, 2-A, 9, 15, 20, 29; border decrees (§1) |
| 3 | **Withholdings** (retenciones) when the company is a persona moral paying personas físicas: IVA 2/3 (10.6667%), ISR 10% on professional fees, ISR 1.25% for RESICO | LIVA 1-A; RLIVA art 3; LISR arts 106, 113-J (§1.3) |
| 4 | **Monthly IVA**: trasladado − acreditable − retenido, on a cash ("efectivamente cobrado/pagado") basis, due on the **17th** of the following month; saldo a favor carried forward or refunded | LIVA arts 1-B, 4, 5, 5-D, 6 (§1.4) |
| 5 | **CFDI 4.0 XML** valid against `cfdv40.xsd`, with catalogue keys, rounding per Anexo 20 | Anexo 20 v4.0; Guía de llenado (§4) |
| 6 | **Cadena original** produced by SAT's `cadenaoriginal_4_0.xslt` (or a port that matches it byte for byte) | §5 |
| 7 | **Sello**: SHA-256 + RSA (PKCS#1 v1.5) over the UTF-8 cadena, with the user's CSD `.key`/`.cer`; `NoCertificado` from the certificate serial | Anexo 20 §I.B (§6) |
| 8 | **PAC stamping** (TimbreFiscalDigital 1.1) is needed before the CFDI is valid. It is a commercial connector (future work); KhataOS ships a "sealed, not stamped" state | Anexo 20 §III (§7) |
| 9 | Fiscal year = calendar year; records kept **5 years** | CFF arts 11, 30 (§2, §9) |
| 10 | Masters: 32 federal entities (SAT `c_Estado` = ISO 3166-2:MX suffixes), currency MXN with 2 decimals | `catCFDI` workbook (§9) |

---

## 1. IVA (Ley del Impuesto al Valor Agregado)

LIVA text below is from the Cámara de Diputados compilation "Última reforma publicada DOF 12-11-2021", read via the SEP mirror (`normatecainterna.sep.gob.mx/storage/recursos/2025/07/JvTL2s146m-ley_del_impuesto_al_valor_agregado.pdf`). The 2026 economic package was reported to amend the CFF, LIEPS and Ley Federal de Derechos, not the LIVA (PwC, Tabasco CEAC summaries) **UNCERTAIN: no later LIVA reform was found, but the current diputados.gob.mx text could not be checked.**

### 1.1 Rates

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **General rate 16%**: "El impuesto se calculará aplicando a los valores que señala esta Ley, la tasa del 16%. El impuesto al valor agregado en ningún caso se considerará que forma parte de dichos valores." Tax is transferred "en forma expresa y por separado" | LIVA art 1, 2nd and 3rd paragraphs [HIGH] | Current | `IVA16` = 0.160000. IVA is always computed on the price before tax and shown separately |
| **0% rate** (art 2-A): (I) sale of non-industrialised animals and plants; "medicinas de patente y productos destinados a la alimentación humana y animal" except listed items (non-milk beverages including juices, soft-drink syrups, caviar/smoked salmon/eels, flavourings and additives, chewing gum, pet food); ice and still water in containers of 10 litres or more; agricultural machinery; fertilisers; gold of 80% or more not sold retail; books/newspapers/magazines published by the taxpayer; "Toallas sanitarias, tampones y copas, para la gestión menstrual". Food "preparados para su consumo en el lugar", or to take away or for delivery, is at 16%. (II) listed services to farmers, milling of corn or wheat, milk pasteurisation, reinsurance, domestic water supply, etc. (IV) **exports** under art 29 | LIVA art 2-A [HIGH] | Current (menstrual products from 2022) | `IVA0` class. Item master flag "tasa 0%" with a reason code. 0%-rated sales keep input-IVA credit ("producirán los mismos efectos legales" as taxed acts) |
| **Exports at 0%**: "Las empresas residentes en el país calcularán el impuesto aplicando la tasa del 0% … cuando unos u otros se exporten"; includes definitive exports under the Ley Aduanera | LIVA art 29 [HIGH] | Current | Export sales use `IVA0`. CFDI `Exportacion` = 02/03/04 when goods are exported (§4.2) |
| **Exempt** (no IVA, and the related input IVA is not creditable): **art 9** sales (land; housing; books by their author; used goods sold by non-companies; lottery tickets; currency; shares and credit instruments, …); **art 15** services (mortgage commissions; AFORE fees; free services; education by authorised schools; …); **art 20** rentals (to authorised donees; housing, unless furnished or hotel; farmland; books) | LIVA arts 9, 15, 20 (headings and first fracciones) [HIGH] | Current | `IVA_EXENTO` class, kept apart from `IVA0` (same split as the UK and UAE packs). In CFDI: `TipoFactor="Exento"` with no `TasaOCuota`/`Importe` (§4.5) |
| **Not subject** ("no objeto"): acts outside art 1 | LIVA art 4-A [HIGH, heading only] | — | CFDI `ObjetoImp="01"`. Not a rate class |

### 1.2 Border-region 8% (estímulo fiscal región fronteriza norte / sur)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Qualifying taxpayers in the northern and southern border regions get a **credit equal to 50% of the IVA**, which leaves an effective rate of **8%**. A separate credit of one third of ISR leaves a 20% ISR rate | Deloitte *Flash Fiscal 03/2025* (6 Jan 2025): "crédito del 50% del Impuesto al Valor Agregado para que quede en una tasa del 8 por ciento" [MEDIUM]. SAT catalogue `c_TasaOCuota` row: Fijo **0.080000**, Impuesto "IVA Crédito aplicado del 50%", Tasa, Traslado Sí, Retención No [HIGH] | 1 Jan 2019 (north); 1 Jan 2021 (south) | `IVA8` class, flagged "estímulo región fronteriza"; only enabled when the company profile says the taxpayer is enrolled. CFDI uses `Impuesto="002"`, `TasaOCuota="0.080000"` |
| **Decrees**: north decree DOF 31 Dec 2018, amended DOF 30 Dec 2020 and 21 Oct 2022; south decree DOF 30 Dec 2020. DOF **24 Dec 2024** extended both to **31 Dec 2025** | Deloitte *Flash Fiscal 03/2025* [MEDIUM] | — | Effective-dated rate row |
| DOF **31 Dec 2025** (evening edition) extended both decrees to **31 Dec 2026**, for 2026 only (not multi-year) | DFK *Flash Fiscal 2/2026*; Trade Law College ("Se extiende el plazo de los beneficios fiscales hasta el 31 de diciembre de 2026"); El Siglo de Torreón; SAT-hosted decree PDF `sat.gob.mx/minisitio/EstimulosFiscalesFronteraNorteSur/documentos/Decreto_modifica_diversos_estimulos_fiscales_region_fronteriza_norte_sur.pdf` (title seen in search; the PDF timed out) [MEDIUM] | to 2026-12-31 | Seed `IVA8` with `valid_to = 2026-12-31`. **No 2027 extension found as of 2026-10-20 (UNCERTAIN).** After 31 Dec 2026, block `IVA8` until a new decree is entered. Show an alert from 1 Dec 2026 |
| Eligibility needs a SAT *aviso* and listing in the beneficiaries' register (padrón); existing beneficiaries did not have to renew for 2026 | Manuel Nevárez blog [LOW] | — | Out of scope: the user confirms eligibility. The municipality list is in the decrees and is not encoded [UNCERTAIN] |

### 1.3 Withholding (retenciones)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Personas morales must withhold the IVA charged to them** when they (a) receive independent personal services from, or rent goods from, personas físicas; (b) buy scrap; (c) receive land freight services; (d) receive commission services from personas físicas | LIVA art 1-A fr II [HIGH] | Current | Purchase-side withholding rules, triggered by (buyer = moral) × (supplier = física) × service type |
| The amount is lower: "La retención se hará por las **dos terceras partes** del impuesto que se les traslade y que haya sido efectivamente pagado" for (a) independent personal services, (b) commission, (c) rental by personas físicas. Freight: **4%** of the consideration | RLIVA art 3 fr I and II (text "Última reforma DOF 25-09-2014", gob.mx copy) [HIGH] | Current | 2/3 × 16% = **10.6667%**. In CFDI, `Retencion Impuesto="002" TipoFactor="Tasa" TasaOCuota="0.106667"`. The catalogue allows IVA retention as a **range 0.000000–0.160000** [HIGH, `c_TasaOCuota`]. Using 0.106667 instead of exactly 2/3 can differ by 1 centavo on large bases [UNCERTAIN which one PACs expect; 0.106667 is common practice] |
| Withholding happens when the price is paid; the withholder takes over the duty to pay that IVA | LIVA art 1-A (later paragraphs) [HIGH] | — | Record retention on payment, not on invoice |
| **ISR 10% on professional fees**: "Cuando los contribuyentes presten servicios profesionales a las personas morales, éstas deberán retener, como pago provisional, el monto que resulte de aplicar la tasa del 10% sobre el monto de los pagos…, sin deducción alguna" | LISR art 106 (via mley.mx; source check marked "pendiente" there) [MEDIUM] | Current | `Retencion Impuesto="001" TasaOCuota="0.100000"`. ISR retention is a range 0–0.35 in the catalogue [HIGH] |
| **ISR 1.25% RESICO**: personas morales paying RESICO personas físicas (art 113-E) "deberán retener, como pago mensual, el monto que resulte de aplicar la tasa del 1.25% sobre el monto de los pagos…, sin considerar el impuesto al valor agregado", paid to SAT by the 17th of the following month | LISR art 113-J (via mley.mx) [MEDIUM]; contadormx, El Contribuyente [MEDIUM] | 2022 | `Retencion Impuesto="001" TasaOCuota="0.012500"` when the supplier's régimen is 626 and the supplier is a persona física. An exception exists for exclusively agricultural activities (RMF rule 3.13.x; the number differs between sources) [UNCERTAIN] |

### 1.4 Monthly IVA return

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| IVA is computed **per calendar month**; payment by return "a más tardar el **día 17** del mes siguiente al que corresponda el pago" | LIVA art 5-D [HIGH] | Current | Period = calendar month; due date = 17th of month+1 (weekend/holiday roll-forward under CFF art 12 is **not verified here** [UNCERTAIN]) |
| Monthly amount = IVA on the month's activities − creditable IVA − IVA withheld from the taxpayer that month | LIVA arts 1 (4th para), 5-D (3rd para) [HIGH] | — | `iva_a_cargo = trasladado_cobrado − acreditable_pagado − retenido_al_contribuyente`. A negative result is a saldo a favor |
| **Cash basis**: consideration is taxed when "efectivamente cobradas" (cash, goods, services, any form that extinguishes the obligation, including advances) | LIVA art 1-B [HIGH] | — | IVA trasladado is reported in the month the **payment is received**, not when invoiced. PPD invoices therefore need payment receipts (§8.1) |
| **Creditable IVA** needs: the expense is strictly indispensable (deductible for ISR, pro rata if partly deductible); IVA "trasladado expresamente … y que conste por separado" on a CFDI; actually paid; retained IVA paid over | LIVA art 5 fr I–IV [HIGH for I, II; others not re-read] | — | A purchase line only counts as acreditable when it has a linked CFDI UUID, separate IVA and a payment date in the period |
| **Saldo a favor**: may only be credited "contra el impuesto a su cargo que le corresponda en los meses siguientes hasta agotarlo o solicitar su devolución"; a refund request covers the whole balance, and refunded balances cannot be credited later | LIVA art 6 [HIGH] | — | Carry forward by default; a "solicitar devolución" flag removes the balance from the carry-forward |
| **DIOT** (supplier IVA report) monthly, "a más tardar el día 17 del mes inmediato posterior" | LIVA art 32 fr VIII [HIGH] | Current | §8.3 |

---

## 2. Fiscal year (CFF art 11)

| Requirement | Source | What the software must do |
|---|---|---|
| "Cuando las leyes fiscales establezcan que las contribuciones se calcularán por ejercicios fiscales, éstos coincidirán con el año de calendario." A persona moral starting after 1 January has an irregular first year from its start date to 31 December. Liquidation, merger or split ends the year early. Monthly taxes use the calendar month | CFF art 11 (via mley.mx; that site marks its source check "pendiente") [MEDIUM]; matches sdv.com.mx [MEDIUM] | Mexico pack: fiscal year fixed at 1 Jan–31 Dec, not editable (unlike India). First year may start mid-year |

---

## 3. RFC (Registro Federal de Contribuyentes)

### 3.1 Structure

| Requirement | Source | What the software must do |
|---|---|---|
| "consta de un espacio, tres letras y seis dígitos para Personas Morales y de cuatro letras y seis dígitos en el caso de Personas Físicas", then 2 homonym characters and 1 check digit; "La clave completa … consta de 13 posiciones" (the moral's leading space is not written) | SAT, *Algoritmo para generar el RFC con homoclave para personas físicas y morales*, IFAI request 0610100135506, 18 Oct 2006, §1.2 (mirror: `mariovaldez.net/files/IFAI 0610100135506 065 Algoritmo….pdf`) [HIGH content / MEDIUM hosting] | Moral: 3 letters + YYMMDD + 3. Física: 4 letters + YYMMDD + 3 |
| XSD pattern `t_RFC`: `[A-Z&Ñ]{3,4}[0-9]{2}(0[1-9]\|1[012])(0[1-9]\|[12][0-9]\|3[01])[A-Z0-9]{2}[0-9A]`, length 12–13, whitespace collapse. `t_RFC_PM` (3 letters, length 12) and `t_RFC_PF` (4 letters, length 13) also exist | `http://www.sat.gob.mx/sitio_internet/cfd/tipoDatos/tdCFDI/tdCFDI.xsd` [HIGH] | Use this regex first. Upper-case input. `Ñ` is one character (2 bytes in UTF-8): count characters, not bytes |
| A check-digit test is only well-formedness. "a lot of numbers (estimated at around 1.5% of all numbers) are in use with invalid check digits", so python-stdnum disables the check by default | python-stdnum `stdnum/mx/rfc.py` [MEDIUM] | Check digit failure = **warning**. Registered status is checked online in the SAT RFC lookup (future) |
| Generic RFCs: `XAXX010101000` (público en general, nationals) and `XEXX010101000` (foreign residents) | Guía de llenado CFDI global, Receptor/Rfc; Guía de llenado Anexo 20, RegimenFiscalReceptor/UsoCFDI [HIGH] | Whitelist both. **`XAXX010101000` fails the check digit** (computed digit is 4), so it must bypass the check |

### 3.2 Check digit (dígito verificador): exact algorithm

From the SAT document, "Procedimiento para calcular el dígito verificador" and Anexo III [HIGH]:

**Anexo III table** (character → value). "TODOS LOS CARACTERES QUE NO SE ENCUENTREN EN ESTA TABLA SE LES ASIGNARA UN VALOR DE '00'".

| Chars | Values | Chars | Values |
|---|---|---|---|
| `0`–`9` | 00–09 | `&` | 24 |
| `A`–`N` | 10–23 (A=10, B=11, …, N=23) | `O`–`Z` | 25–36 (O=25, …, Z=36) |
| space ("BLANCO") | 37 | `Ñ` | 38 |

The same order as one string: `"0123456789ABCDEFGHIJKLMN&OPQRSTUVWXYZ Ñ"` (index = value), the `_alphabet` used by python-stdnum [MEDIUM, matches the table].

**Steps:**
1. Take the 12 characters before the check digit. For a persona moral (11 characters) **prepend one space** so there are 12 (§1.2 "un espacio, tres letras…"; python-stdnum pads the same way).
2. `S = Σ value(c_i) × (13 − i)` for i = 0..11 (weights 13, 12, …, 2, left to right). The document writes this as `Vi * (Pi + 1)`, with Pi the position counted from the right, 1 to 12.
3. `r = S mod 11`.
4. `r = 0` → digit `'0'`. Otherwise `d = 11 − r`; `d = 10` → `'A'`; else the digit `d`.

The document's wording "Si el residuo es igual a 10 el dígito verificador será 'A'" is loose. Read as written, it would map r = 10 to 'A' and leave r = 1 undefined. The consistent reading, used by python-stdnum (`_alphabet[(11 - check) % 11]`) and fitting the XSD's `[0-9A]` last character, is **(11 − r) = 10, i.e. r = 1 → 'A'**. Test RFC XIA190128J61 has r = 10 and digit `1`, which fits 11 − r [MEDIUM on the 'A' case: no real RFC ending in 'A' was found to test against].

**Worked example (SAT's own):** `GODE561231GR` → G16·13 + O25·12 + D13·11 + E14·10 + 5·9 + 6·8 + 1·7 + 2·6 + 3·5 + 1·4 + G16·3 + R28·2 = **1026**; 1026 mod 11 = 3; 11 − 3 = **8** → `GODE561231GR8` [HIGH, matches the document].

**Persona moral example:** `EKU9003173C9` (SAT sandbox emitter, "Escuela Kemper Urgate"; widely used in SAT test CSDs, but no SAT page naming it was found [UNCERTAIN provenance]): ␠37·13 + E14·12 + K20·11 + U31·10 + 9·9 + 0·8 + 0·7 + 3·6 + 1·5 + 7·4 + 3·3 + C12·2 = **1344**; 1344 mod 11 = 2; 11 − 2 = **9** ✓.

**Test vectors** (computed with a script in this research; all match except where noted):

| RFC | Sum | mod 11 | Computed | Result |
|---|---|---|---|---|
| GODE561231GR8 | 1026 | 3 | 8 | pass (SAT example) |
| EKU9003173C9 | 1344 | 2 | 9 | pass |
| URE180429TM6 | 1578 | 5 | 6 | pass |
| H&E951128469 | 1289 | 2 | 9 | pass (`&` = 24) |
| OÑO120726RX3, ZUÑ920208KL4 | 1702, 1877 | 8, 7 | 3, 4 | pass (`Ñ` = 38) |
| EWE1709045U0, MAS0810247C0 | 1364, 1287 | 0, 0 | 0, 0 | pass (r = 0 case) |
| XEXX010101000 | 1342 | 0 | 0 | pass |
| **XAXX010101000** | 1294 | 7 | **4** | **fails**: whitelist |

The other vectors (URE…, H&E…, OÑO…, ZUÑ…, EWE…, MAS…, CACX7605101P8, XIQB891116QE4) are names commonly shipped with SAT test certificates; their provenance was not confirmed [LOW]. All pass, which supports the table, the space padding and the Ñ/& values.

**Not in scope:** generating the homoclave (2 homonym characters) from a name. That uses a different table (Anexo I, e.g. space = 00, & = 10, Ñ = 40) and only SAT assigns RFCs officially (same document) [HIGH]. Do not mix the Anexo I and Anexo III tables.

---

## 4. CFDI 4.0 (Anexo 20, version 4.0)

### 4.1 Status and files

| Item | Source | Note |
|---|---|---|
| Anexo 20 of the RMF 2022 (CFDI 4.0 standard) published in the DOF on **13 Jan 2022** | Anexo20_2022.pdf header "DIARIO OFICIAL Jueves 13 de enero de 2022" (`omawww.sat.gob.mx/tramitesyservicios/Paginas/documentos/Anexo20_2022.pdf`) [HIGH] | — |
| CFDI 3.3 coexisted until **31 Mar 2023**; 4.0 is the only version from **1 Apr 2023** (payroll had an extension to 30 Jun 2023) | La Razón; BDO *BF 1ª quincena abril 2023*; KPMG [MEDIUM] | Generate 4.0 only |
| Official files (SAT Anexo 20 page `omawww.sat.gob.mx/tramitesyservicios/Paginas/anexo_20.htm`) [HIGH]: XSD `http://www.sat.gob.mx/sitio_internet/cfd/4/cfdv40.xsd` (Last-Modified 4 Jun 2024); XSLT `…/cfd/4/cadenaoriginal_4_0/cadenaoriginal_4_0.xslt` (Last-Modified 17 Apr 2026); catalogues `…/cfd/catalogos/catCFDI.xsd` (Last-Modified 13 Dec 2024) and workbook `catCFDI_V_4_20261015.xls`; data types `…/cfd/tipoDatos/tdCFDI/tdCFDI.xsd`; Guía de llenado; error matrix `MatrizDeErrores_CFDI_v40_20260325.xls` | Downloaded 2026-10-20 [HIGH] | **Vendor and pin** these files with their hashes in the pack. Re-check the Last-Modified dates in CI |
| XML must be UTF-8; escape `&` `"` `<` `>` `'` as `&amp;` `&quot;` `&lt;` `&gt;` `&apos;`. No attribute may contain `\|` (XSD patterns `[^\|]{…}`) | Anexo 20 §I.A intro; cadena rule 1 [HIGH] | Reject `\|` at data entry for every text field that reaches a CFDI |

### 4.2 Root and Comprobante attributes

Root: `<cfdi:Comprobante xmlns:cfdi="http://www.sat.gob.mx/cfd/4" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.sat.gob.mx/cfd/4 http://www.sat.gob.mx/sitio_internet/cfd/4/cfdv40.xsd" …>`. Target namespace `http://www.sat.gob.mx/cfd/4`, `elementFormDefault="qualified"`, attributes unqualified [HIGH, cfdv40.xsd]. The xsi/schemaLocation form is the one validated here with xmllint; the XSD itself does not require it [MEDIUM].

All from cfdv40.xsd and catCFDI [HIGH] unless tagged:

| Attribute | Use | Rule |
|---|---|---|
| `Version` | req | fixed `4.0` |
| `Serie` | opt | 1–25 chars, `[^\|]` |
| `Folio` | opt | 1–40 chars, `[^\|]` |
| `Fecha` | req | `t_FechaH`: `(20[1-9][0-9])-(0[1-9]\|1[0-2])-(0[1-9]\|[12][0-9]\|3[01])T(([01][0-9]\|2[0-3]):[0-5][0-9]:[0-5][0-9])`, **no timezone, no fractions**; "debe corresponder con la hora local donde se expide" (Anexo 20). A PAC 72-hour stamping window is often quoted but was **not found in Anexo 20** [UNCERTAIN] |
| `Sello` | req | Base64 (§6) |
| `FormaPago` | opt (conditional) | `c_FormaPago`: 01 Efectivo, 02 Cheque nominativo, 03 Transferencia electrónica, 04 Tarjeta de crédito, 05 Monedero electrónico, 06 Dinero electrónico, 08 Vales de despensa, 12–27 (dación, subrogación, consignación, condonación, compensación, novación, confusión, remisión, prescripción, a satisfacción), 28 Tarjeta de débito, 29 Tarjeta de servicios, 30 Aplicación de anticipos, 31 Intermediario pagos, **99 Por definir**. Not paid at issue → "99" and `MetodoPago` "PPD" (Guía). Must not exist for T, P or N |
| `NoCertificado` | req | exactly 20 digits `[0-9]{20}` |
| `Certificado` | req | CSD certificate as Base64 text (Anexo 20); the base64 of the DER `.cer` [MEDIUM, matches the CryptoSys example] |
| `CondicionesDePago` | opt | 1–1000 chars; not for T, P, N |
| `SubTotal` | req | `t_Importe`; Guía: up to the currency's decimals; for I/E/N = rounded sum of concept `Importe`; T/P = 0 |
| `Descuento` | opt | rounded sum of concept `Descuento`; omit otherwise |
| `Moneda` | req | `c_Moneda` (ISO 4217). **MXN: 2 decimals**; XXX: 0 (catalogue workbook) |
| `TipoCambio` | opt | 1–6 decimals; **required when `Moneda` ≠ MXN and ≠ XXX**; a FIX rate outside the catalogue tolerance needs a PAC `Confirmacion` (Guía) |
| `Total` | req | SubTotal − Descuento + traslados − retenciones; non-negative; T/P = 0 (Guía) |
| `TipoDeComprobante` | req | `I` ingreso, `E` egreso, `T` traslado, `N` nómina, `P` pago |
| `Exportacion` | req | `01` No aplica, `02` Definitiva clave A1, `03` Temporal, `04` Definitiva distinta a A1 / no enajenación |
| `MetodoPago` | opt | `PUE` (pagado al emitir) / `PPD` (parcialidades o diferido; then `FormaPago=99`). Not for T/P |
| `LugarExpedicion` | req | `c_CodigoPostal` (5 digits) |
| `Confirmacion` | opt | 5 chars `[0-9a-zA-Z]{5}`, from the PAC |

### 4.3 Element order (xs:sequence) [HIGH, cfdv40.xsd]

`Comprobante` → `InformacionGlobal`? → `CfdiRelacionados`* → `Emisor` → `Receptor` → `Conceptos` → `Impuestos`? → `Complemento`? → `Addenda`?.
`Concepto` → `Impuestos`? (→ `Traslados`? then `Retenciones`?) → `ACuentaTerceros`? → `InformacionAduanera`* → `CuentaPredial`* → `ComplementoConcepto`? → `Parte`*.
Comprobante-level `Impuestos` → `Retenciones`? then `Traslados`?.
**Note the opposite order:** at concept level Traslados come first; at Comprobante level Retenciones come first.

### 4.4 Emisor, Receptor, Concepto

| Node / attribute | Rule | Source |
|---|---|---|
| `Emisor`: `Rfc` (t_RFC), `Nombre` (1–300), `RegimenFiscal` (c_RegimenFiscal), `FacAtrAdquirente` opt (10 digits) | 601 General de Ley PM, 603 PM no lucrativos, 605 Sueldos, 606 Arrendamiento, 612 PF actividades empresariales y profesionales, 616 Sin obligaciones, 621 Incorporación Fiscal, 625 Plataformas, **626 RESICO** (PF and PM), … The workbook marks each regime as física / moral | XSD + workbook [HIGH] |
| `Receptor`: `Rfc`, `Nombre`, `DomicilioFiscalReceptor` (5 digits; for non-generic RFCs it must be the CP registered for that RFC), `ResidenciaFiscal` opt (c_Pais), `NumRegIdTrib` opt (1–40), `RegimenFiscalReceptor`, `UsoCFDI` | `UsoCFDI` must be allowed for the receiver's régimen in the `c_UsoCFDI` column "Régimen Fiscal Receptor": G01 Adquisición de mercancías, G02 Devoluciones/descuentos, G03 Gastos en general, I01–I08 inversiones, D01–D10 deducciones personales (PF only), **S01 Sin efectos fiscales**, **CP01 Pagos**, CN01 Nómina. (`P01` is still in catCFDI.xsd but not in the 2026 workbook; do not offer it) | XSD, workbook, Guía [HIGH] |
| `XEXX010101000` receiver → `RegimenFiscalReceptor="616"`, `UsoCFDI="S01"` | — | Guía [HIGH] |
| `Concepto`: `ClaveProdServ` (c_ClaveProdServ, e.g. 84111506 "servicios de facturación", 01010101 "no existe en el catálogo"), `NoIdentificacion` opt (1–100), `Cantidad` (>0, ≤6 decimals), `ClaveUnidad` (c_ClaveUnidad e.g. **H87** pieza, **E48** unidad de servicio, ACT actividad), `Unidad` opt (1–20), `Descripcion` (1–1000), `ValorUnitario` (t_Importe; >0 for I/E/N, 0 for P), `Importe` (= Cantidad × ValorUnitario, ≤6 decimals), `Descuento` opt (≤ Importe), `ObjetoImp` | Catalogue keys verified present in catCFDI.xsd | XSD + Guía [HIGH] |
| `ObjetoImp` (c_ObjetoImp): `01` No objeto, `02` Sí objeto (break down taxes per concept), `03` Sí objeto, no obligado al desglose, `04` Sí objeto y no causa impuesto, `05` IVA crédito PODEBI, `06` Sí objeto del IVA, no traslado IVA, `07` No traslado del IVA, sí desglose IEPS, `08` No traslado del IVA, no desglose IEPS (05–08 added 2023–2024) | — | Workbook [HIGH] |

### 4.5 Taxes (Impuestos)

| Rule | Source |
|---|---|
| Concept `Traslado`: `Base` (req, >0, ≤6 decimals), `Impuesto` (req; `001` ISR, `002` IVA, `003` IEPS), `TipoFactor` (req; `Tasa`/`Cuota`/`Exento`), `TasaOCuota` (opt, **6 decimals**, e.g. `0.160000`), `Importe` (opt, ≤6 decimals) | XSD, Guía [HIGH] |
| "Si el valor registrado es **Exento** no se deben registrar los atributos TasaOCuota ni Importe. Si el valor registrado es Tasa o Cuota, se deben registrar…" | Anexo 20 validations [HIGH] |
| Concept `Retencion`: `Base`, `Impuesto`, `TipoFactor` (must not be Exento), `TasaOCuota` (req), `Importe` (req) | XSD; Anexo 20 [HIGH] |
| `TasaOCuota` must use 6 decimals from `c_TasaOCuota`. IVA traslado: fixed 0.000000, 0.160000, 0.080000 (border). IVA retención: range 0–0.160000. ISR retención: range 0–0.350000 | Guía Q40; workbook [HIGH] |
| Comprobante `Impuestos`: `TotalImpuestosRetenidos` opt, `TotalImpuestosTrasladados` opt (required when any concept has a factor other than Exento). `Retencion`: `Impuesto`, `Importe`. `Traslado`: `Base` (req, t_Importe), `Impuesto`, `TipoFactor`, `TasaOCuota` opt, `Importe` opt, with **one row per (Impuesto, TipoFactor, TasaOCuota)**; Base and Importe = rounded sums of the matching concept rows, to the currency's decimals. All concepts Exento → only Base, Impuesto, TipoFactor | Guía; Anexo 20 validations [HIGH] |
| No `Impuestos` node when TipoDeComprobante is T, P or N | Guía [HIGH] |

### 4.6 Rounding and decimals

| Rule | Source |
|---|---|
| Concepts may use 0–6 decimals; totals are rounded **at the end** to the currency's decimals (MXN 2). Concept `Importe` is rounded in `SubTotal`, concept discounts in `Descuento`, concept taxes in the Comprobante tax summary. Example: four lines with 4-decimal tax sum to 0.7104 → 0.71 | Guía Apéndice 7, Q37 and Q39 [HIGH] |
| Trailing zeros may be padded to 6 or omitted; the count of decimals must be ≤ the standard's maximum | Guía Q38 [HIGH] |
| PAC tolerance for concept `Importe`: lower = (Cantidad − 10^−dC/2)·(ValorUnitario − 10^−dV/2), **truncated**; upper = (Cantidad + 10^−dC/2 − 10^−12)·(ValorUnitario + 10^−dV/2 − 10^−12), **rounded up**, both to the `Importe`'s own decimals. Tax `Importe`: lower = (Base − 10^−dB/2)·TasaOCuota truncated; upper = (Base + 10^−dB/2 − 10^−12)·TasaOCuota rounded up | Anexo 20 validations [HIGH] |
| KhataOS rule: compute line amounts with `rust_decimal`, emit line amounts at 2 decimals for MXN (inside the tolerance band), sum, round half-up to 2 for totals, and **self-check every line against the band above** before sealing | Design choice. Half-up versus banker's rounding is not specified by SAT [UNCERTAIN; half-up chosen] |

---

## 5. Cadena original (cadenaoriginal_4_0.xslt)

Rules from Anexo 20 §I.B "Cadena Original" [HIGH]: (1) no `|` in data; (2) starts with `||`; (3) values only, no attribute names; (4) `|` between values; (5) whitespace: tabs/CR/LF → space, trim around each `|`, collapse runs of spaces to one; (6) "Los datos opcionales no expresados, no aparecen en la cadena original y no tienen delimitador alguno"; (7) ends with `||`; (8) UTF-8; (9–10) ComplementoConcepto/Complemento in document order; (11) the TimbreFiscalDigital is **not** part of the CFDI cadena.

**XSLT mechanics** (fetched 2026-10-20 from `http://www.sat.gob.mx/sitio_internet/cfd/4/cadenaoriginal_4_0/cadenaoriginal_4_0.xslt`, which includes `…/cfd/2/cadenaoriginal_2_0/utilerias.xslt`) [HIGH]:
- Root template: `|` + Comprobante + `||`. `Requerido` always emits `|` + `normalize-space(string(@x))`. `Opcional` emits the same only `if test="$valor"`, i.e. **when the attribute exists**: an attribute present with an empty value still emits a bare `|`. So KhataOS must **omit** absent optionals, never write `Attr=""`. The first `Requerido` gives the leading `||`.
- Whitespace: values are those the XML parser returns (entities decoded, attribute-value normalisation done), then XPath `normalize-space` (strip and collapse #x20/#x9/#xD/#xA).
- **Order is fixed by the XSLT, not by attribute order in the XML.**

**Exact field order** (R = required, O = optional) [HIGH]:

1. **Comprobante**: Version R, Serie O, Folio O, Fecha R, FormaPago O, **NoCertificado R**, CondicionesDePago O, SubTotal R, Descuento O, Moneda R, TipoCambio O, Total R, TipoDeComprobante R, Exportacion R, MetodoPago O, LugarExpedicion R, Confirmacion O. **`Sello` and `Certificado` are not included.**
2. **InformacionGlobal**: Periodicidad R, Meses R, Año R.
3. **CfdiRelacionados** (each node): TipoRelacion R, then each CfdiRelacionado UUID R.
4. **Emisor**: Rfc R, Nombre R, RegimenFiscal R, FacAtrAdquirente O.
5. **Receptor**: Rfc R, Nombre R, DomicilioFiscalReceptor R, ResidenciaFiscal O, NumRegIdTrib O, RegimenFiscalReceptor R, UsoCFDI R.
6. **Concepto** (each): ClaveProdServ R, NoIdentificacion O, Cantidad R, ClaveUnidad R, Unidad O, Descripcion R, ValorUnitario R, Importe R, Descuento O, ObjetoImp R; then each **Concepto Traslado**: Base R, Impuesto R, TipoFactor R, TasaOCuota O, Importe O; then each **Concepto Retencion**: Base R, Impuesto R, TipoFactor R, TasaOCuota R, Importe R; then ACuentaTerceros (RfcACuentaTerceros, NombreACuentaTerceros, RegimenFiscalACuentaTerceros, DomicilioFiscalACuentaTerceros, all R); InformacionAduanera NumeroPedimento R; CuentaPredial Numero R; ComplementoConcepto children; Parte (ClaveProdServ R, NoIdentificacion O, Cantidad R, Unidad O, Descripcion R, ValorUnitario O, Importe O, then its InformacionAduanera).
7. **Impuestos** (Comprobante level): each Retencion (Impuesto R, Importe R); **TotalImpuestosRetenidos O**; each Traslado (Base R, Impuesto R, TipoFactor R, TasaOCuota O, Importe O); **TotalImpuestosTrasladados O**, after the Traslado rows.
8. **Complemento**: each child through its own included XSLT (Pagos20, CartaPorte, etc.). The TFD has no template here.

**Verification run here** [HIGH]: (a) the SAT XSLT, applied with xsltproc to CryptoSys's CFDI 4.0 example (`cryptosys.net/firmasat/cfdv40-examples.html`), gave a cadena identical to CryptoSys's `cfdv40-ejemplo-cadena.txt`, whose SHA-256 is `c1492662dbd98ddbb7892d027c10808236e296eded13deec87179c2a8fdc742e` as published. (b) A KhataOS-style invoice (one 16% service line, one 0% food line, MXN, PUE) validated against `cfdv40.xsd` and gave:

```
||4.0|A|1001|2026-10-20T10:15:00|03|30001000000500003416|1500.00|MXN|1660.00|I|01|PUE|20000|EKU9003173C9|ESCUELA KEMPER URGATE|601|URE180429TM6|UNIVERSIDAD ROBOTICA ESPAÑOLA|86991|601|G03|84111506|SRV-01|1|E48|Servicio|Servicio de contabilidad|1000.00|1000.00|02|1000.00|002|Tasa|0.160000|160.00|50221200|2|H87|Pieza|Tortilla de maiz|250.00|500.00|02|500.00|002|Tasa|0.000000|0.00|1000.00|002|Tasa|0.160000|160.00|500.00|002|Tasa|0.000000|0.00|160.00||
```

The source `Descripcion` was `"Servicio   de⏎      contabilidad"`, which shows the collapse. NoCertificado there is a placeholder. Use both cases as golden tests: a Rust port of the XSLT must match xsltproc output byte for byte. Keep an xsltproc-based conformance test in CI.

---

## 6. Sello (digital seal)

| Requirement | Source | What the software must do |
|---|---|---|
| Algorithms: "SHA-2 256 … digestión", then "RSAPrivateEncrypt, que utiliza la clave privada del emisor para encriptar la digestión"; result to Base 64. The digest covers the cadena "incluyendo los nodos Complementarios" | Anexo 20 §I.B "Generación del Sello Digital" steps I–III [HIGH] | `sello = base64(RSASSA-PKCS1-v1_5-SIGN(SHA-256, utf8(cadena)))` |
| That is standard **PKCS#1 v1.5 with SHA-256** (`openssl dgst -sha256 -sign key.pem`) | Verified here: signing the CryptoSys example cadena with its test key (`emisor.p8e`, password `12345678a`) reproduced the example's `Sello` exactly [HIGH by reproduction; the Anexo text does not name "PKCS#1 v1.5"] | Use the `rsa` crate `Pkcs1v15Sign::new::<Sha256>()`; no PSS |
| The private key "sólo debe mantenerse en memoria durante la llamada a la función de encripción; inmediatamente después … debe ser eliminada" | Anexo 20 §I.B note [HIGH] | Decrypt the `.key` per signing call; `zeroize` buffers; never store the decrypted key or the password |
| CSD `.key` = PKCS#8 **EncryptedPrivateKeyInfo** (DER), password-protected; `.cer` = X.509 DER | Example key: ASN.1 PBES2 + PBKDF2 (openssl asn1parse) [HIGH for the example]; the real SAT `.key` being the DER form of the same structure is [MEDIUM] (CryptoSys FirmaSAT docs) | Accept DER and PEM. Support PBES2 (PBKDF2 + 3DES/AES) and legacy PBE-SHA1-3DES [UNCERTAIN which encryption SAT currently uses] |
| `NoCertificado` = the certificate serial read as ASCII: serial `0x3330303031303030303030333030303233373038` → `"30001000000300023708"` | openssl on the example certificate, matching the example's `NoCertificado` [HIGH by reproduction] | Hex-decode the serial bytes into ASCII digits; assert 20 digits |
| `Certificado` = base64 of the `.cer` DER, no line breaks | Anexo 20 "como texto en formato base 64" [HIGH]; example starts `MIIF…` (DER SEQUENCE) [MEDIUM] | — |
| Check the CSD is a CSD (not e.firma), not expired, and that its RFC matches `Emisor/@Rfc` | PAC error "certificado … no coincide con el emisor" (satfacil.com.mx 403) [LOW]; cancellation error 310 "Uso de certificado de e.firma inválido" (Anexo 20 §I.C) [HIGH] | Read the RFC from the subject `x500UniqueIdentifier`; warn on mismatch or expiry |

---

## 7. Timbrado (PAC) and cancellation

| Requirement | Source | What the software must do |
|---|---|---|
| `TimbreFiscalDigital` 1.1 (namespace `http://www.sat.gob.mx/TimbreFiscalDigital`, XSD `…/cfd/TimbreFiscalDigital/TimbreFiscalDigitalv11.xsd`) is the "Complemento requerido para el Timbrado Fiscal Digital que da validez al Comprobante" | Anexo 20 §III.B [HIGH] | **An unstamped CFDI has no fiscal validity.** Label KhataOS output "Sellado, sin timbrar". Never print it as a factura |
| Attributes: `Version` 1.1, `UUID` (36 chars, RFC 4122), `FechaTimbrado` (t_FechaH, "hora de la Zona Centro"), `RfcProvCertif` (t_RFC_PM), `Leyenda` opt (12–150), `SelloCFD` (the CFDI's Sello), `NoCertificadoSAT` (20 digits), `SelloSAT` | Anexo 20 §III.B [HIGH] | Store the returned XML unchanged; index by UUID |
| TFD cadena (`cadenaoriginal_TFD_1_1.xslt`): `\|\|Version\|UUID\|FechaTimbrado\|RfcProvCertif[\|Leyenda]\|SelloCFD\|NoCertificadoSAT\|\|` (SelloSAT excluded) | XSLT fetched [HIGH] | Used to verify a stamped CFDI and print the QR/representation |
| Cancellation: SAT web service, signed with the CSD (XML-DSig), by UUID; `Motivo` **01 02 03 04**; `FolioSustitucion` required when Motivo = 01 | Anexo 20 §I.C [HIGH] | Future work through the PAC connector |
| Motives: 01 issued with errors, with a replacement CFDI; 02 errors, no replacement; 03 operation did not take place; 04 nominative operation already in a global invoice | Siigo, contadormx [MEDIUM] | Store the motive with the cancellation status |
| Receiver acceptance is needed in some cases (3 working days to answer, after which the request is treated as accepted); exceptions by amount (e.g. ≤ $1,000) and type; deadline tied to the annual ISR return month (CFF 29-A; RMF 2.7.1.x) | Secondary sources disagree [UNCERTAIN] | Do not encode; the PAC reports the result |

---

## 8. Other CFDI and reporting obligations (pointers)

### 8.1 Complemento para recepción de pagos 2.0 (REP)
- Namespace `http://www.sat.gob.mx/Pagos20`, prefix `pago20`; XSD `http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xsd`; included in the 4.0 cadena XSLT (`…/cfd/Pagos/Pagos20.xslt`) [HIGH].
- Structure: `Pagos` (Version) → `Totales` (`MontoTotalPagos` …) → `Pago` (`FechaPago`, `FormaDePagoP`, `MonedaP`, `Monto`, …) → `DoctoRelacionado` (`IdDocumento` = UUID of the PPD invoice, `MonedaDR`, `NumParcialidad`, `ImpSaldoAnt`, `ImpPagado`, `ImpSaldoInsoluto`, `ObjetoImpDR`) with `ImpuestosDR` and `ImpuestosP` [HIGH, XSD].
- Carried by a CFDI of type `P` (SubTotal = Total = 0, UsoCFDI `CP01`, no FormaPago/MetodoPago) [HIGH, Guía]. One REP per payment, or grouped by period; due by the 5th calendar day of the month after the payment (RMF 2.7.1.35) [UNCERTAIN on the exact current day].
- Needed for the cash-basis IVA in §1.4: KhataOS should link REP UUIDs to receipts.

### 8.2 Factura global (público en general)
- `InformacionGlobal` (Periodicidad 01 diario / 02 semanal / 03 quincenal / 04 mensual / 05 bimestral (régimen 621 only); `Meses` 01–12, or 13–18 for bimestral; `Año` ≥ 2019). Receptor `Rfc="XAXX010101000"`, `Nombre="PUBLICO EN GENERAL"`, `DomicilioFiscalReceptor` = `LugarExpedicion`, `RegimenFiscalReceptor="616"`, `UsoCFDI="S01"`, no ResidenciaFiscal/NumRegIdTrib. Concepts: `ClaveProdServ="01010101"`, `NoIdentificacion` = ticket folio, `Cantidad="1"`, `ClaveUnidad="ACT"`, no `Unidad`, `Descripcion="Venta"` [HIGH, *Guía de llenado CFDI global*, `omawww.sat.gob.mx/…/documentos/Guia_llenado_CFDI_global.pdf`].

### 8.3 DIOT
- Monthly supplier report on IVA paid, withheld, credited and transferred, broken down by rate, due "a más tardar el día 17 del mes inmediato posterior" (LIVA art 32 fr VIII) [HIGH]. Filed in SAT's own format and application. The 2025 redesign and any RMF deadline extension were not checked [UNCERTAIN]. KhataOS: future export from the purchase register (supplier RFC, type of operation, base by rate, IVA, retained IVA).

### 8.4 Contabilidad electrónica (Anexo 24)
- Monthly upload through the SAT portal of the **catálogo de cuentas** (Anexo 24 structure, each account mapped to SAT's **código agrupador**; sent with the first balanza and again when it changes) and the **balanza de comprobación** (opening balances, period movements, closing balances; the year-end balanza includes fiscal adjustments) [HIGH for content: RMF rule 2.8.1.6 on `wwwmat.sat.gob.mx/articulo/57336/regla-2.8.1.6`, RMF 2024 text]. Anexo 24 for 2026 was published in the DOF on 13 Jan 2026, with a changed financial-sector grouping code and two-digit payment-method keys [MEDIUM, idconline]. Due dates (rule 2.8.1.7) [UNCERTAIN, not retrieved]. Legal basis CFF art 28 fr IV [MEDIUM].
- KhataOS: a `codigo_agrupador` field on each ledger group/account; XML exporters for catálogo and balanza (future work).

---

## 9. Masters: currency, states, retention

| Item | Source | What the software must do |
|---|---|---|
| Currency **MXN**, 2 decimals (`c_Moneda`: MXN "Peso Mexicano", Decimales 2) | Workbook [HIGH] | Base currency MXN; amounts in centavos |
| **32 federal entities** (31 states + Ciudad de México). SAT `c_Estado` (country MEX) uses the same 3-letter codes as ISO 3166-2:MX | Workbook `c_Estado` [HIGH]; Wikipedia *ISO 3166-2:MX* [MEDIUM] | Store `MX-XXX`; the SAT code is the suffix |
| **CMX** Ciudad de México replaced **DIF** (Distrito Federal); the name change took effect 2016-01-30 | Name change: statoids.com [MEDIUM]. Current code MX-CMX: Wikipedia and SAT `c_Estado` [MEDIUM/HIGH]. Date of the ISO code change (commonly given as Nov 2016) **not verified** [UNCERTAIN] | Map legacy `DIF`/`MX-DIF` on import to `CMX` |
| **Retention: 5 years**: "La documentación … y la contabilidad, deberán conservarse durante un plazo de cinco años … contado a partir de la fecha en la que se presentaron o debieron haberse presentado las declaraciones con ellas relacionadas" | CFF art 30, 3rd paragraph (via mley.mx) [MEDIUM] | Retention lock = 5 years from the filing due date of the related return. Longer periods for items with lasting effects (assets, losses, capital) [UNCERTAIN, not retrieved]: let the user extend per document |

**ISO 3166-2:MX / SAT c_Estado** [HIGH SAT, MEDIUM ISO]:

| Code | Entity | Code | Entity | Code | Entity | Code | Entity |
|---|---|---|---|---|---|---|---|
| AGU | Aguascalientes | COL | Colima | MIC | Michoacán | SIN | Sinaloa |
| BCN | Baja California | DUR | Durango | MOR | Morelos | SON | Sonora |
| BCS | Baja California Sur | GUA | Guanajuato | NAY | Nayarit | TAB | Tabasco |
| CAM | Campeche | GRO | Guerrero | NLE | Nuevo León | TAM | Tamaulipas |
| CHP | Chiapas | HID | Hidalgo | OAX | Oaxaca | TLA | Tlaxcala |
| CHH | Chihuahua | JAL | Jalisco | PUE | Puebla | VER | Veracruz |
| CMX | Ciudad de México | MEX | Estado de México | QUE | Querétaro | YUC | Yucatán |
| COA | Coahuila | ROO | Quintana Roo | SLP | San Luis Potosí | ZAC | Zacatecas |

---

## 10. Design implications for KhataOS (`khata-mx` pack)

1. **Scope of the open-source pack:** generate the CFDI 4.0 XML, the cadena original and the sello locally from a user-supplied CSD (`.cer` + `.key` + password). The output is a *sealed, unstamped* CFDI ("pre-CFDI"). It is useful for review, for handing to any PAC, and for tests. It is **not** a valid factura until a PAC adds the TFD (§7). The UI must say so.
2. **PAC stamping is a commercial connector** (future work): a `PacConnector` trait (`stamp(xml) -> tfd_xml`, `cancel(uuid, motivo, folio_sustitucion)`, `status(uuid)`). There is no free SAT stamping endpoint for third parties. Ship a mock PAC for tests that emits a fake TFD clearly marked as a test.
3. **Determinism:** build XML with a fixed attribute order (the XSLT order), no empty optional attributes, UTF-8 without BOM, `\|` rejected at entry. Cadena generator = a Rust port of the XSLT, tested byte for byte against xsltproc + the vendored SAT XSLT on golden files (§5). Hash: `sha2`. Signature: `rsa` PKCS#1 v1.5. Key handling: `pkcs8` with `encryption` features, `zeroize`.
4. **Catalogue snapshot:** vendor `catCFDI.xsd`/workbook subsets (FormaPago, MetodoPago, RegimenFiscal, UsoCFDI × régimen matrix, ObjetoImp, Impuesto, TipoFactor, TasaOCuota, Moneda, Estado, Exportacion, TipoRelacion, Periodicidad, Meses; ClaveProdServ/ClaveUnidad/CodigoPostal searchable) with the SAT file date. Validate against `cfdv40.xsd` before sealing.
5. **Tax engine:** classes `IVA16`, `IVA8` (effective-dated, flag-gated, ends 2026-12-31), `IVA0`, `IVA_EXENTO`, `NO_OBJETO`; withholding rules `RET_IVA_2_3` (0.106667), `RET_IVA_FLETE_4`, `RET_ISR_HON_10`, `RET_ISR_RESICO_1_25`, triggered by party type (moral/física) and régimen.
6. **IVA return on a cash basis:** the monthly worksheet takes IVA trasladado from receipts (PUE invoices and REP payments) and acreditable IVA from paid purchases with a CFDI UUID; it subtracts IVA withheld, and carries saldo a favor (§1.4).
7. **RFC validator:** regex + check digit (warning only) + generic whitelist; persona type inferred from the length (12 = moral, 13 = física).
8. **Fiscal year** locked to the calendar year; voucher-numbering series per `Serie` (≤ 25 chars) with `Folio` ≤ 40 chars (see `voucher-numbering.md`).
9. **Audit trail:** a sealed or stamped CFDI is immutable. Corrections are made by cancellation and substitution (Motivo 01 + `CfdiRelacionados TipoRelacion="04"`) or by an egreso (E) note, never by editing.

---

## 11. Open items to verify before GA

1. Whether a decree extends the border 8% IVA beyond **31 Dec 2026** (check DOF in late Dec 2026).
2. Any LIVA reform after DOF 12-11-2021 (diputados.gob.mx not reachable here).
3. RFC check digit 'A' case: find a real RFC ending in 'A' to confirm r = 1 → 'A'.
4. Retention `TasaOCuota` for 2/3 IVA: 0.106667 versus exact 2/3, and the rounding PACs expect.
5. Encryption scheme of current SAT `.key` files (PBES2 versus legacy PBE).
6. The PAC stamping window after `Fecha` (72 hours?) and the RMF rule number.
7. REP deadline (5th day?) and current RMF rule numbers (2.7.1.35, 2.8.1.7, 3.13.x).
8. Cancellation acceptance thresholds and deadlines (CFF 29-A, RMF 2026).
9. CFF art 30 longer retention cases; CFF art 12 due-date roll-forward.
10. ISO OBP date of the MX-DIF → MX-CMX change.

---

## 12. Source list

**SAT (primary)**
- Anexo 20 page: <http://omawww.sat.gob.mx/tramitesyservicios/Paginas/anexo_20.htm>
- Anexo 20 RMF 2022 (DOF 13-01-2022): <http://omawww.sat.gob.mx/tramitesyservicios/Paginas/documentos/Anexo20_2022.pdf>
- Guía de llenado CFDI: <http://omawww.sat.gob.mx/tramitesyservicios/Paginas/documentos/Anexo_20_Guia_de_llenado_CFDI.pdf>
- Guía de llenado CFDI global: <http://omawww.sat.gob.mx/tramitesyservicios/Paginas/documentos/Guia_llenado_CFDI_global.pdf>
- Catálogos workbook: <http://omawww.sat.gob.mx/tramitesyservicios/Paginas/documentos/catCFDI_V_4_20261015.xls>
- cfdv40.xsd: <http://www.sat.gob.mx/sitio_internet/cfd/4/cfdv40.xsd>
- cadenaoriginal_4_0.xslt: <http://www.sat.gob.mx/sitio_internet/cfd/4/cadenaoriginal_4_0/cadenaoriginal_4_0.xslt>
- utilerias.xslt: <http://www.sat.gob.mx/sitio_internet/cfd/2/cadenaoriginal_2_0/utilerias.xslt>
- catCFDI.xsd: <http://www.sat.gob.mx/sitio_internet/cfd/catalogos/catCFDI.xsd>
- tdCFDI.xsd: <http://www.sat.gob.mx/sitio_internet/cfd/tipoDatos/tdCFDI/tdCFDI.xsd>
- TFD 1.1 XSD / XSLT: <https://www.sat.gob.mx/sitio_internet/cfd/TimbreFiscalDigital/TimbreFiscalDigitalv11.xsd>, <https://www.sat.gob.mx/sitio_internet/cfd/TimbreFiscalDigital/cadenaoriginal_TFD_1_1.xslt>
- Pagos 2.0 XSD: <http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xsd>
- RMF rule 2.8.1.6: <https://wwwmat.sat.gob.mx/articulo/57336/regla-2.8.1.6>
- Border decree PDF (not retrieved, timed out): <https://www.sat.gob.mx/minisitio/EstimulosFiscalesFronteraNorteSur/documentos/Decreto_modifica_diversos_estimulos_fiscales_region_fronteriza_norte_sur.pdf>
- RFC algorithm (SAT reply to IFAI 0610100135506, mirror): <https://www.mariovaldez.net/files/IFAI%200610100135506%20065%20Algoritmo%20para%20generar%20el%20RFC%20con%20homoclave%20para%20personas%20fisicas%20y%20morales.pdf>

**Legislation (Cámara de Diputados compilations, via mirrors)**
- LIVA (DOF 12-11-2021 text): <https://normatecainterna.sep.gob.mx/storage/recursos/2025/07/JvTL2s146m-ley_del_impuesto_al_valor_agregado.pdf>
- RLIVA (DOF 25-09-2014 text): <https://www.gob.mx/cms/uploads/attachment/file/431963/28_riva.pdf>
- CFF arts 11, 30: <https://mley.mx/CFF/articulo/11/>, <https://mley.mx/CFF/articulo/30/>
- LISR arts 106, 113-J: <https://mley.mx/LISR/articulo/106/>, <https://mley.mx/LISR/articulo/113-J/>
- Official index (unreachable here): <https://www.diputados.gob.mx/LeyesBiblio/ref/cff.htm>

**Secondary**
- Border decree: Deloitte Flash Fiscal 03/2025 <https://www.deloitte.com/content/dam/assets-zone4/latam/es/docs/services/tax/2025/flash-fiscal-03-2025-es.pdf>; DFK Flash Fiscal 2/2026 <https://www.dfk.com.mx/flash-fiscal-2-prorroga-a-los-estimulos-fiscales-en-la-region-fronteriza-norte-y-sur/>; Trade Law College <https://www.tradelawcollege.edu.mx/single-post/actualizaci%C3%B3n-de-est%C3%ADmulos-en-regiones-fronterizas-norte-y-sur-para-el-ejercicio-2026>; El Siglo de Torreón <https://www.elsiglodetorreon.com.mx/noticia/2026/continua-el-iva-del-8-por-ciento-en-la-frontera-norte-de-mexico-durante-2026.html>; Manuel Nevárez <https://www.manuelnevarez.com.mx/es/journal/avisos-de-inscripcion-y-renovacion-para-aplicar-los-estimulos-region-fronteriza-norte-y-sur-4/>
- 2026 package scope: PwC <https://www.pwc.com/mx/es/archivo/2025/propuesta-iniciativa-reforma-fiscal2026.pdf>; Tabasco CEAC <https://ceac.tabasco.gob.mx/sites/all/files/vol/ceac.tabasco.gob.mx/fi/REFORMAS%20FISCALES.pdf>
- CFDI 4.0 go-live: La Razón <https://www.razon.com.mx/negocios/partir-abril-obligatorio-emitir-facturas-version-4-0-523161>; BDO <https://www.bdomexico.com/getmedia/738db9a9-0355-43af-bf8e-fd993ebf8f73/BF_1%C2%AA-Quincena-de-abril-2023.pdf.aspx?ext=.pdf>; KPMG <https://kpmg.com/mx/es/home/tendencias/2023/04/flash-sat-prorroga-para-la-entrada-en-vigor-del-CFDI-4-de-nomina.html>
- CryptoSys CFDI 4.0 examples (test vectors): <https://cryptosys.net/firmasat/cfdv40-examples.html>
- python-stdnum RFC: <https://github.com/arthurdejong/python-stdnum/blob/master/stdnum/mx/rfc.py>
- Cancellation motives: <https://www.siigo.com/mx/blog/obligaciones-fiscales/motivos-de-cancelacion-cfdi-4-0/>; <https://contadormx.com/motivos-de-cancelacion-de-cfdi/>
- RESICO retention: <https://contadormx.com/retenciones-del-isr-al-resico-en-el-cfdi/>; <https://www.elcontribuyente.mx/2022/02/como-se-declaran-al-sat-las-retenciones-de-isr-al-resico/>
- Anexo 24 2026: <https://idconline.mx/fiscal-contable/2026/01/16/anexo-24-rmisc-2026-cambios-en-contabilidad>
- ISO 3166-2:MX: <https://en.wikipedia.org/wiki/ISO_3166-2:MX>; statoids <https://statoids.com/umx.html>
- CSD mismatch error: <https://www.satfacil.com.mx/errores-sat/403>

---

## 13. Addendum (2026-10-21): retenciones on the CFDI, Complemento de Pagos 2.0, PAC web services, cancellation

Research for the withholding, payment-receipt and PAC work (`khata_mx::retenciones`, `pagos`, `timbre`; `pac_client.rs`). Same confidence tags as above.

### 13.1 Retenciones on a CFDI 4.0

| Finding | Source | Used as |
|---|---|---|
| Concepto level: `Impuestos` → `Traslados` then `Retenciones`; each `Retencion` has Base, Impuesto, TipoFactor (not Exento), TasaOCuota and Importe, all required. Comprobante level: `Impuestos` with `TotalImpuestosRetenidos` and `TotalImpuestosTrasladados`, children `Retenciones` (one `Retencion` per Impuesto: Impuesto, Importe) then `Traslados` | `cfdv40.xsd`; §4.3, §4.5 above [HIGH] | `cfdi.rs` writes exactly this |
| Cadena order: concepto Traslado rows, then concepto Retencion rows; comprobante: each Retencion, TotalImpuestosRetenidos, each Traslado, TotalImpuestosTrasladados | `cadenaoriginal_4_0.xslt` (fetched 2026-10-21, lines 230–266 and 372–406) [HIGH] | Port; checked with xsltproc on an ingreso with ISR 10% + IVA 0.106667 and an exento line: identical cadena; `xmllint` against `cfdv40.xsd` with the catalogues: valid |
| ISR 10% on honorarios paid by personas morales (LISR art. 106) and on rent (art. 116); IVA two thirds (RLIVA art. 3 fr. I); freight 4% (fr. II); RESICO ISR 1.25% (LISR 113-J) | §1.3 above [HIGH/MEDIUM there] | The `RET:` class tokens (`ISR10`, `IVA2/3`, `IVA4`, `RESICO`) |
| IVA retained by the customer is subtracted from the supplier's IVA payable; IVA the business withholds is paid over by the 17th of the following month and is creditable once paid (LIVA arts. 1-A fr. IV, 5 fr. IV) | LIVA text (Cámara de Diputados compilation, §1 sources) [MEDIUM: read for this addendum only through the existing citations] | IVA summary: `iva_retenido_por_clientes` off IVA a cargo; `retenido a enterar` shown apart; same-month crediting is a simplification |
| Importe of a retención = Base × TasaOCuota rounded to the currency's decimals; the PAC tolerance is computed from Base and TasaOCuota (§4.6) | Anexo 20 validations [HIGH] | `money(base × tasa)`; with 0.106667 a centavo away from exactly 2/3 on large bases [UNCERTAIN which the SAT prefers; unchanged from §1.3] |

### 13.2 Complemento para recepción de Pagos 2.0

Files fetched 2026-10-21: `http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xsd`, `…/Pagos/Pagos20.xslt`, `…/catalogos/Pagos/catPagos.xsd`; *Guía de llenado del comprobante al que se le incorpore el complemento para recepción de pagos, aplica para la versión 4.0 del CFDI* (copy at bhrmx.com, 99 pages; the omawww.sat.gob.mx URLs tried returned no PDF) [HIGH for XSD/XSLT; HIGH for the Guía text read].

| Finding | Source | Used as |
|---|---|---|
| The comprobante P: Version 4.0, no FormaPago, CondicionesDePago, Descuento, TipoCambio or MetodoPago; `SubTotal="0"`, `Moneda="XXX"`, `Total="0"`, `TipoDeComprobante="P"`, `Exportacion="01"`; CfdiRelacionados only TipoRelacion `04` (sustitución of an earlier P); Receptor UsoCFDI `CP01`; one Concepto ClaveProdServ `84111506`, Cantidad `1`, ClaveUnidad `ACT`, Descripcion `Pago`, ValorUnitario `0`, Importe `0`, ObjetoImp `01`, no NoIdentificacion/Unidad/Descuento/Impuestos; no comprobante Impuestos | Guía §I [HIGH] | `pagos::generate` |
| `pago20:Pagos Version="2.0"` → `Totales` (TotalRetencionesIVA/ISR/IEPS, TotalTrasladosBaseIVA16, TotalTrasladosImpuestoIVA16, …IVA8, …IVA0, TotalTrasladosBaseIVAExento, MontoTotalPagos req.) → `Pago`+ (FechaPago, FormaDePagoP, MonedaP, TipoCambioP, Monto, NumOperacion, RfcEmisorCtaOrd, NomBancoOrdExt, CtaOrdenante, RfcEmisorCtaBen, CtaBeneficiario, TipoCadPago, CertPago, CadPago, SelloPago) → `DoctoRelacionado`+ (IdDocumento, Serie, Folio, MonedaDR, EquivalenciaDR, NumParcialidad, ImpSaldoAnt, ImpPagado, ImpSaldoInsoluto, ObjetoImpDR) → `ImpuestosDR` (`RetencionesDR`/`RetencionDR`: BaseDR, ImpuestoDR, TipoFactorDR, TasaOCuotaDR, ImporteDR all req.; `TrasladosDR`/`TrasladoDR`: BaseDR, ImpuestoDR, TipoFactorDR req., TasaOCuotaDR, ImporteDR opt.) and then `ImpuestosP` (`RetencionesP`/`RetencionP`: ImpuestoP, ImporteP; `TrasladosP`/`TrasladoP`: BaseP, ImpuestoP, TipoFactorP, TasaOCuotaP, ImporteP). XSD element order inside ImpuestosDR/ImpuestosP: Retenciones then Traslados. IdDocumento pattern: UUID or `[0-9]{3}-[0-9]{2}-[0-9]{9}`; NumParcialidad `[1-9][0-9]{0,2}`; BaseDR ≥ 0.000001, 6 decimals; Totales are `t_ImporteMXN` | `Pagos20.xsd` [HIGH] | Node and attribute order |
| Cadena: Version; Totales attributes in the order above; per Pago its attributes, then per DoctoRelacionado its attributes, its **RetencionDR rows, then TrasladoDR rows**, then the Pago's RetencionP rows and TrasladoP rows | `Pagos20.xslt` [HIGH] | Port; checked with xsltproc (4.0 XSLT including Pagos20.xslt) on full and partial receipts: identical; xmllint with `Pagos20.xsd`: valid |
| FormaDePagoP never `99`; MonedaP not `XXX`; with MonedaP MXN, TipoCambioP must be `"1"`; FechaPago is when the payee received it, `12:00:00` when the time is unknown; Monto > 0 and ≥ Σ ImpPagado (with a rounding margin); MonedaDR = MonedaP → EquivalenciaDR `"1"`; ImpSaldoAnt = previous saldo insoluto (the invoice total for the first payment); ImpSaldoInsoluto = ImpSaldoAnt − ImpPagado ≥ 0; ObjetoImpDR `02` requires ImpuestosDR, `01`/`03` forbid it; one RetencionP per impuesto; one TrasladoP per (impuesto, factor, tasa); Totales = rounded Σ(BaseP/ImporteP × TipoCambioP) | Guía §II [HIGH] | `pagos::check`, `totales` |
| How BaseDR is determined is left to "las disposiciones fiscales vigentes"; ImporteDR "será calculado por el sistema" | Guía §II [HIGH] (the method itself is not stated) | Proportional: BaseDR = base × ImpPagado / invoice Total; ImporteDR = BaseDR × TasaOCuotaDR, to the centavo [MEDIUM: common practice] |
| Revision B (Dec 2023): with EquivalenciaDR = 1 and several documents, 10 decimals are used in the margin calculation | elcontribuyente.mx (2023-12) [LOW] | Not relevant to MXN-only receipts |
| A credit note (egreso) applies to the original invoice directly; no payment receipt is issued to apply it | Guía, Apéndice 1 nota 4 [HIGH] | Earlier Against allocations from credit notes reduce ImpSaldoAnt; not counted as parcialidades |

### 13.3 PAC web services

| Finding | Source | Used as |
|---|---|---|
| **SW Sapien (SW smarter web)**: test base `https://services.test.sw.com.mx`, production `https://services.sw.com.mx`; production credentials from SW (account executive / soporte) | SW SDK README; developers.sw.com.mx integration guide (search summary) [MEDIUM] | `sw` provider environments |
| Sign-in: `POST {base}/v2/security/authenticate`, JSON `{"user","password"}` → `{"data":{"token"},"status":"success"}` | `SWServices/Authentication/AuthRequest.php` (github.com/lunasoft/sw-sdk-php, master, fetched 2026-10-21) [MEDIUM: SDK source, not live] | `SwSapien::token` (cached in memory 1 h [UNCERTAIN lifetime]) |
| Stamp: `POST {base}/cfdi33/stamp/{v1..v4}[/b64]`, `multipart/form-data` with one part `name="xml"; filename="xml"`, `Content-Type: text/xml`, header `Authorization: Bearer <token>`; the same path serves CFDI 4.0 (the SDK's README shows 4.0 documents). v4 answers `data.{cadenaOriginalSAT, noCertificadoSAT, noCertificadoCFDI, uuid, selloSAT, selloCFDI, fechaTimbrado, qrCode, cfdi}`; errors `{"message","messageDetail","status":"error"}` | `SWServices/Stamp/StampRequest.php`, `StampService.php`, README [MEDIUM] | `SwSapien::stamp` (v4; TFD read from `data.cfdi`, so RfcProvCertif and Leyenda come from the XML) |
| Cancel: `POST {base}/cfdi33/cancel/csd` JSON `{rfc, uuid, motivo, b64Cer, b64Key, password, foliosustitucion}`; `/cfdi33/cancel/pfx`; `/cfdi33/cancel/xml` (signed cancellation XML, multipart); `/cfdi33/cancel/{rfc}/{uuid}/{motivo}/{foliosustitucion}` (CSD stored in the SW account). Success `data.{acuse, uuid:{<UUID>: "201"|"202"}}`; 201 = request received (not yet a guarantee), 202 = previously cancelled | `SWServices/Cancelation/CancelationRequest.php`, `CancelationService.php`, README [MEDIUM] | `SwSapien::cancel` (CSD and UUID methods) |
| **No PAC offers a public sandbox without credentials**: SW's test environment needs a (free) test account; Finkok's demo needs a demo user | SW README ("cuentaUsuario"/"contraseña" placeholders); Finkok samples [MEDIUM] | No live call was made; tests use a local axum mock of the SW API |
| **Finkok**: SOAP services, demo `https://demo-facturacion.finkok.com/servicios/soap/stamp` (namespace `http://facturacion.finkok.com/stamp`, operation `stamp` with xml, username, password), cancel namespace `http://facturacion.finkok.com/cancel` | Chilkat Finkok samples; satcfdi docs (search summaries) [LOW/MEDIUM] | Not implemented; a future `PacConnector` adapter |
| TFD 1.1: attributes Version, UUID, FechaTimbrado, RfcProvCertif, Leyenda (opt.), SelloCFD, NoCertificadoSAT, SelloSAT; cadena `||Version|UUID|FechaTimbrado|RfcProvCertif[|Leyenda]|SelloCFD|NoCertificadoSAT||` | `TimbreFiscalDigitalv11.xsd`, `cadenaoriginal_TFD_1_1.xslt` (fetched 2026-10-21) [HIGH]; matches SW's `cadenaOriginalSAT` example | `timbre::Timbre` |
| Printed QR: `https://verificacfdi.facturaelectronica.sat.gob.mx/default.aspx?id=<UUID>&re=<RFC emisor>&rr=<RFC receptor>&tt=<total>&fe=<last 8 chars of the Sello>`; Anexo 20's text says the total omits non-significant zeros (max 18 integers, 6 decimals) while its own example shows `tt=0000014300.000000`; phpcfdi (`cfdi-expresiones`, `FormatTotal18x6`) writes 6 decimals with trailing zeros trimmed (`14300.0`) and XML-escapes the RFCs | Anexo 20 (DOF 13 Jan 2022) §I.D [HIGH for the fields; UNCERTAIN for the total's format]; phpcfdi source [MEDIUM] | `timbre::qr_text` follows phpcfdi |

### 13.4 Cancellation

| Finding | Source | Used as |
|---|---|---|
| Motivos 01 (errors with relation: issue the replacement first, its UUID in FolioSustitucion), 02 (errors without relation), 03 (operation not carried out), 04 (nominative operation in a factura global); FolioSustitucion only with 01 | §7 above; idconline.mx (2021-12-23); Reachcore cancellation doc [HIGH/MEDIUM] | `timbre::check_cancellation` |
| Receiver acceptance: needed except in listed cases (RMF rule 2.7.1.35: e.g. total ≤ $1,000, nómina, egreso, traslado…); the receiver answers within 3 working days or the request counts as accepted (one source says 5 days); whether a CFDI with Complemento de Pagos can be cancelled without acceptance is described differently (Alegra and contadormx: no; EY's RMF 2026 note lists it among the exceptions); deadline tied to the annual return month (CFF 29-A) | sovos.com RMF 2026 blog; elcontribuyente.mx (2025-09); alegra.com; contadormx.com "cancelación de CFDI en 2026"; EY Tax Flash RMF 2026 (search summaries) [UNCERTAIN] | Not encoded: the record stays "cancelación solicitada" until the user records the SAT's answer |

Sources for this addendum:
- Pagos 2.0 XSD/XSLT: <http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xsd>, <http://www.sat.gob.mx/sitio_internet/cfd/Pagos/Pagos20.xslt>, <http://www.sat.gob.mx/sitio_internet/cfd/catalogos/Pagos/catPagos.xsd>
- Guía de llenado, complemento de pagos, CFDI 4.0: <https://bhrmx.com/wp-content/uploads/2022/01/GuiallenadopagosCFDI4.0.pdf>; Sovos on its 2024 revision <https://sovos.com/mx/cambios-regulatorios/iva/mexico-se-publica-la-guia-de-llenado-del-complemento-de-recepcion-de-pagos/>; Revision B <https://www.elcontribuyente.mx/2023/12/el-sat-libero-el-complemento-para-recepcion-de-pagos-2-0-revision-b/>; matriz de errores (copy) <https://cdn.taecel.com/src/fiscal/descargas/matriz-errores-complemento-recepcion-de-pagos.pdf>
- TFD 1.1: <http://www.sat.gob.mx/sitio_internet/cfd/TimbreFiscalDigital/TimbreFiscalDigitalv11.xsd>, <http://www.sat.gob.mx/sitio_internet/cfd/TimbreFiscalDigital/cadenaoriginal_TFD_1_1.xslt>
- SW Sapien SDK: <https://github.com/lunasoft/sw-sdk-php> (`SWServices/Authentication/AuthRequest.php`, `SWServices/Stamp/StampRequest.php`, `SWServices/Stamp/StampService.php`, `SWServices/Cancelation/CancelationRequest.php`, `SWServices/Cancelation/CancelationService.php`, `README.md`); <https://packagist.org/packages/lunasoft/sw-sdk-php>; developers portal <https://developers.sw.com.mx/>; integration guide <https://developers.sw.com.mx/knowledge-base/integrate-your-service-with-sw-sapien-even-if-you-are-outside-of-mexico/>
- Finkok: <https://www.example-code.com/csharp/finkok_stamp.asp>; <https://satcfdi.readthedocs.io/en/4.9.25/generated/satcfdi.pacs.finkok.html>
- phpcfdi QR expressions: <https://github.com/phpcfdi/cfdi-expresiones> (`src/Extractors/Standards/Comprobante20170701.php`, `FormatTotal18x6.php`)
- Cancellation: <https://idconline.mx/fiscal-contable/2021/12/23/posibles-motivos-para-la-cancelacion-del-cfdi>; <https://go.reachcore.com/docs/Articulos/CancelaCFDI2022_EN>; <https://sovos.com/mx/blog/iva/rmf-2026-cambios-cfdi/>; <https://www.elcontribuyente.mx/2025/09/cancelacion-de-cfdi-plazos-excepciones-y-como-responder-para-evitar-errores/>; <https://blog.alegra.com/mexico/cancelacion-de-cfdi-paso-a-paso/>; <https://contadormx.com/cancelacion-de-cfdi-en-2026/>; <https://www.ey.com/content/dam/ey-unified-site/ey-com/es-mx/technical/tax/documents/ey-tax-flash-rmf2026-declaraciones.pdf>; SAT scheme <https://www.sat.gob.mx/minisitio/Factura/documentos/EsquemaCancelacionCFDI.pdf>

# Spain Compliance Requirements (IVA, facturación, VERI*FACTU): KhataOS Spec

> **Status as of:** 2026-10-21. **Scope:** businesses taxed in the IVA territory of Spain (*territorio común*: the Peninsula and the Balearic Islands) keeping books in KhataOS, a local-first desktop accounting app: *autónomos* (IRPF), companies (Impuesto sobre Sociedades, IS), non-residents with a permanent establishment and *entidades en atribución de rentas*. Canarias (IGIC), Ceuta and Melilla (IPSI) and the foral territories (País Vasco: TicketBAI/Batuz; Navarra) are out of scope (§7, §2). A future `khata-es` pack would hold the identity, VAT and modelo 303 items, and a `verifactu` module the invoice-record chain, hash and QR. Structured B2B e-invoices (UBL / EN 16931) belong to the shared EU crate `khata-en16931` (see §8).
> **Not legal advice.** This file turns the Ley 37/1992 (LIVA), the RIVA (RD 1624/1992), the Reglamento de facturación (RD 1619/2012), the RRSIF (RD 1007/2023), Orden HAC/1177/2024 and the AEAT technical documents into software requirements. Get a Spanish *asesor fiscal* to confirm each item before release.
>
> **Confidence tags:**
> - **[PRIMARY]** means read in the primary text: boe.es (consolidated text, incl. the BOE's own amendment notes), sede.agenciatributaria.gob.es / agenciatributaria.es (AEAT instructions, *diseños de registro*, VERI*FACTU developer documents, XSD/WSDL), or a ministry page.
> - **[SECONDARY]** means reported consistently by reputable non-official sources (Wikipedia, python-stdnum, tax-advisor and software-vendor sites), but the official text was not retrieved in this research.
> - **[UNCERTAIN]** means sources conflict, the item is interpretive, or the point was not found. Flagged for review.
>
> All AEAT VERI*FACTU PDFs were downloaded and converted with `pdftotext`; the Java code samples (images in the PDFs) were read from page renders. The interior.gob.es page on the DNI letter returned HTTP 403; its content was taken from a search-engine extract of that official page (marked as such).

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **NIF validation**: DNI (8 digits + letter `TRWAGMYFPDXBNJZSQVHLCKE`[n mod 23]); NIE (X/Y/Z → 0/1/2, same letter); K/L/M personal NIFs (letter on the 7 digits); entity NIF (letter + 7 digits + control digit or letter `JABCDEFGHI`) | Interior ministry page; RD 1065/2007 arts. 19–22; Orden EHA/451/2008 (letters); control algorithm from Wikipedia/python-stdnum (§1) |
| 2 | **NIF-IVA** = `ES` + NIF for intra-EU operations | python-stdnum, VIES practice (§1.4) |
| 3 | **Rates** 21% / 10% / 4% / 0% (donations to non-profits) from an effective-dated master; history rows for the 2022–2024 temporary 5%, 2%, 7.5% and 0% food/energy rates; recargo de equivalencia 5.2 / 1.4 / 0.5 / 1.75 | LIVA arts. 90, 91, 161; AEAT VERI*FACTU validation rules (§2) |
| 4 | **Invoice content** RD 1619/2012 art. 6; number + series "correlativa"; mandatory separate series (rectificativas, self-billing, …); separate series for simplified vs full invoices in the same year; *factura simplificada* ≤ €400 (≤ €3,000 for listed retail/hospitality activities); rectificativas art. 15; issue at the time of the operation or before the 16th of the next month for B2B | RD 1619/2012 arts. 4, 6, 7, 11, 15 (§3) |
| 5 | **Modelo 303** 2026: casillas 150–152 (0%), 01–03 (4%), 04–06 (10%), 07–09 (21%), 10–15, recargo 156–158/168–170/16–24, 27, deducible 28–45, 46, info 59/60/120/122–124, result 64–71 (new **112** from period 02/2T 2026); quarterly by the 20th (4T by 30 Jan); monthly by the 30th for REDEME/large companies/SII | AEAT *Instrucciones 2026* and *diseño de registro* DR303e26v101 (§4) |
| 6 | **VERI*FACTU** (RRSIF): every invoice issued creates a chained *registro de facturación de alta* (or *anulación*) with SHA-256 *huella*; mandatory **before 1 Jan 2027** (IS taxpayers) / **before 1 Jul 2027** (everyone else); producers had to offer only adapted software from **29 Jul 2025** | RD 1007/2023 as amended by RD 254/2025 and RDL 15/2025; AEAT FAQ v1.3 (§6) |
| 7 | **Hash**: `IDEmisorFactura=…&NumSerieFactura=…&FechaExpedicionFactura=…&TipoFactura=…&CuotaTotal=…&ImporteTotal=…&Huella=…&FechaHoraHusoGenRegistro=…` (alta) / 5-field string (anulación), trimmed values, UTF-8, SHA-256, 64 uppercase hex; the three AEAT golden examples reproduced here bit-for-bit | AEAT *Detalle de las especificaciones técnicas para generación de la huella o hash* v0.1.2 (§6.7) |
| 8 | **QR + legend** on every invoice: ISO/IEC 18004, level M, 30–40 mm, text "QR tributario:" above; "VERI*FACTU" or "Factura verificable en la sede electrónica de la AEAT" below (VERI*FACTU mode only); URL `…/wlpl/TIKE-CONT/ValidarQR?nif=&numserie=&fecha=&importe=` (URL-encoded, UTF-8) | Orden HAC/1177/2024 arts. 20–21; AEAT QR spec v0.5.0 (§6.9) |
| 9 | **Mode choice**: VERI*FACTU (send every record to the AEAT; no signature, no event log) recommended for KhataOS; "no VERI*FACTU" needs XAdES-signed records + event log + 6-hourly event summaries | RD 1007/2023 arts. 8, 12, 16; Orden HAC/1177/2024 arts. 3, 6–9, 14 (§6.4) |
| 10 | **Fiscal year**: IS period = the entity's *ejercicio económico* (≤ 12 months; calendar year by default); IRPF and VAT periods are calendar-based; currency EUR, VAT and record amounts always in euros | LIS art. 27; RIVA art. 71; RD 1619/2012 art. 12; RRSIF art. 10.2 (§9) |

---

## 1. Identity numbers

### 1.1 NIF types

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Spanish nationals: the NIF is "el número de su documento nacional de identidad seguido del correspondiente código o carácter de verificación, constituido por una letra mayúscula" | RD 1065/2007 art. 19.1 [PRIMARY] | 1 Jan 2008 | `NIF = 8 digits + letter` (§1.2) |
| Spaniards without DNI: 9 characters, initial **L** (resident abroad) or **K** (resident in Spain, under 14), "siete caracteres alfanuméricos y un carácter de verificación alfabético" | RD 1065/2007 art. 19.2 [PRIMARY] | — | Accept `K`/`L` + 7 + letter |
| Foreigners: the NIF is the NIE; without NIE the AEAT assigns **M** + 7 alphanumerics + alphabetic check character | RD 1065/2007 art. 20 [PRIMARY] | — | Accept `X/Y/Z` NIE and `M` NIF |
| Entities: "nueve caracteres": (a) a letter for the legal form (Spanish entity) or foreign entity / permanent establishment, (b) "Un número aleatorio de siete dígitos", (c) "Un carácter de control" | Orden EHA/451/2008 art. 2 [PRIMARY]; RD 1065/2007 art. 22 [PRIMARY] | 2008 | Entity NIF validator (§1.3) |
| NIF of an entity is invariable unless its legal form or nationality changes | Orden EHA/451/2008 art. 1 [PRIMARY] | — | Store NIF on the party master; history on change |
| The VERI*FACTU schema type `NIFType` is exactly 9 characters | AEAT `SuministroInformacion.xsd` [PRIMARY] | — | Normalise to 9 chars, uppercase, no separators |

### 1.2 DNI / NIE / K-L-M check letter

Algorithm (Ministerio del Interior, *Cálculo del dígito de control del NIF/NIE* [PRIMARY, via search-engine extract of the official page; direct fetch returned 403]):

1. DNI: take the 8-digit number `n`; letter = `"TRWAGMYFPDXBNJZSQVHLCKE"[n mod 23]`. Official example: 12345678 mod 23 = 14 → **Z** → `12345678Z`.
2. NIE: replace the first letter `X → 0`, `Y → 1`, `Z → 2`, then apply the DNI rule to the resulting 8-digit number.
3. K / L / M: the check letter is the DNI letter of the **7 digits** after the initial letter [SECONDARY, python-stdnum `es/nif.py`: "these use the old checkdigit algorithm (the DNI one)"; not found in an official text: UNCERTAIN].

**Test vectors** (computed here, `python3 -I`):

| Number | Source | Result |
|---|---|---|
| 12345678Z | Interior ministry example | valid (12345678 mod 23 = 14) |
| 89890001K | AEAT VERI*FACTU test NIF (hash and QR documents) | valid (89890001 mod 23 = 21 → K) |
| 54362315K | python-stdnum `es/dni.py` | valid |
| 54362315Z | python-stdnum (negative) | invalid (expected K) |
| X2482300W, X5253868R, X5277343Q, Y5277343F, Z5277343K | python-stdnum `es/nie.py`, `test_eu_vat.doctest` | valid |
| X2482300A | python-stdnum (negative) | invalid (expected W) |
| M1234567L | python-stdnum `es/nif.py` | valid (K/L/M rule) |
| K2814015F | python-stdnum `test_eu_vat.doctest` (invalid list) | invalid (expected B) |
| 00000000T, 99999999R | computed | valid (edge values) |

### 1.3 Entity NIF (former CIF)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Legal-form letters: **A** Sociedades anónimas; **B** Sociedades de responsabilidad limitada; **C** Sociedades colectivas; **D** Sociedades comanditarias; **E** Comunidades de bienes y herencias yacentes; **F** Sociedades cooperativas; **G** Asociaciones; **H** Comunidades de propietarios en régimen de propiedad horizontal; **J** Sociedades civiles, con o sin personalidad jurídica; **P** Corporaciones Locales; **Q** Organismos públicos; **R** Congregaciones e instituciones religiosas; **S** Órganos de la Administración del Estado y de las Comunidades Autónomas; **U** Uniones Temporales de Empresas; **V** Otros tipos no definidos en el resto de claves; **N** entidad extranjera; **W** establecimiento permanente de una entidad no residente | Orden EHA/451/2008 arts. 3–5 [PRIMARY] | 2008 | First character ∈ `ABCDEFGHJNPQRSUVW`; show the legal form |
| Control character: over the 7 digits d1…d7: `A` = d2 + d4 + d6; `B` = Σ over d1, d3, d5, d7 of the digit sum of `2·d`; `D = (10 − (A + B) mod 10) mod 10`; digit form = `D`, letter form = `"JABCDEFGHI"[D]` (J=0, A=1 … I=9). Equivalent to the Luhn check digit of the 7 digits | Wikipedia es *Código de identificación fiscal* [SECONDARY]; python-stdnum `es/cif.py` (`luhn.calc_check_digit` + `'JABCDEFGHI'`) [SECONDARY]. Orden EHA/451/2008 defines only "un carácter de control"; no official text with the algorithm was found [UNCERTAIN] | — | Implement; accept both forms by default |
| Letter vs digit: "Será una LETRA si la clave de entidad es P, Q, R, S, W o N" (also when the first two digits are 00, non-residents); "Será un NÚMERO si la entidad es A, B, E o H"; other letters either | Wikipedia es [SECONDARY]. python-stdnum accepts either form for every letter because "there seems to be conflicting information" [SECONDARY] — **conflict** [UNCERTAIN] | — | Accept either form; warn (not reject) when A/B/E/H end in a letter or P/Q/R/S/W/N end in a digit |
| Worked example A58818501: 5881850 → A = 14, B = 15, C = 29, D = 1 → digit 1 (letter A) | Wikipedia es [SECONDARY]; recomputed here | — | Unit test |

**Test vectors** (computed here; all match python-stdnum's expectations):

| Number | Source | Result |
|---|---|---|
| A58818501 | Wikipedia es example | valid, digit control (letter would be A) |
| J99216582 | python-stdnum `es/cif.py` | valid, digit (2/B) |
| J99216583 | python-stdnum (negative) | invalid (expected 2 or B) |
| A13585625 | python-stdnum `split()` example | valid, digit (5/E) |
| B58378431, B64717838 | python-stdnum `es/nif.py` | valid, digit |
| B64717839 | python-stdnum (negative) | invalid (expected 8 or H) |
| Q0818001J, Q2818015F, R9600075G, W4003922D | python-stdnum `test_eu_vat.doctest` | valid, **letter** control |
| U99216426, U99216632, V99218067, F13005293 | python-stdnum tests | valid, digit control |
| A28184661, A60631835, A60997582, A08370314, A17028366, B64240914, B79110482, B80975063, A84409408, A08074908, B60432291, B18889204, B55015432, B64995111, B96902531, A78109592, A58951310 | python-stdnum `test_eu_vat.doctest` (valid `ES…` list) | valid, digit control |

### 1.4 NIF-IVA (intra-EU VAT number)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| The Spanish VAT number is `ES` + the 9-character NIF (python-stdnum `compact('ES B-58378431') == 'B58378431'`) | python-stdnum `es/nif.py` [SECONDARY]; VERI*FACTU list L7 code `02` = "NIF-IVA" [PRIMARY, Orden HAC/1177/2024 annex] | — | Store NIF; print `ES` + NIF on intra-EU invoices |
| Recipient's NIF is mandatory on the invoice for exempt intra-EU supplies of goods (LIVA art. 25), reverse-charge operations and operations located in Spain by an established supplier | RD 1619/2012 art. 6.1.d) [PRIMARY] | — | Block issue without the customer NIF in these cases |
| Being in VIES requires registration in the *Registro de Operadores Intracomunitarios* (ROI) | Common practice [SECONDARY] | — | Optional VIES check (future work) |

### 1.5 Autonomous communities (ISO 3166-2:ES)

ES-AN Andalucía, ES-AR Aragón, ES-AS Asturias (Principado de), ES-CN Canarias, ES-CB Cantabria, ES-CL Castilla y León, ES-CM Castilla-La Mancha, ES-CT Catalunya, ES-EX Extremadura, ES-GA Galicia, ES-IB Illes Balears, ES-RI La Rioja, ES-MD Madrid (Comunidad de), ES-MC Murcia (Región de), ES-NC Navarra (Comunidad Foral de), ES-PV País Vasco, ES-VC Valenciana (Comunidad); autonomous cities ES-CE Ceuta, ES-ML Melilla. 17 + 2 = 19 codes, all as listed in the task [SECONDARY, Wikipedia *ISO 3166-2:ES*, last change 2020-11-24; the ISO OBP was not queried].

Tax-territory flags per region: ES-CN → IGIC; ES-CE, ES-ML → IPSI; ES-PV → foral (Álava, Bizkaia, Gipuzkoa; TicketBAI); ES-NC → foral (Navarra). The RRSIF applies in the foral territories only to taxpayers "cuando tengan su domicilio fiscal en territorio común" [PRIMARY, RD 1007/2023 art. 1.3].

---

## 2. Territory and rates (LIVA arts. 3, 90, 91, 161)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Territory**: Canarias is excluded from harmonised turnover taxes and Ceuta and Melilla from the customs union; they are not "interior del país" for IVA | LIVA art. 3.Dos.1.º a), b) [PRIMARY] | — | Out of scope; treat supplies to them as exports (casilla 60 includes "envíos con carácter definitivo a Canarias, Ceuta y Melilla" [PRIMARY, Instrucciones 303 2026]) |
| **General rate 21%**: "El Impuesto se exigirá al tipo del 21 por ciento, salvo lo dispuesto en el artículo siguiente"; "El tipo impositivo aplicable a cada operación será el vigente en el momento del devengo" | LIVA art. 90.Uno, Dos [PRIMARY]; 21% "con efectos desde el 1 de septiembre de 2012" by RDL 20/2012 [PRIMARY, BOE note] | 1 Sep 2012 | `GENERAL` 21%; rate chosen by *devengo* date, not invoice date |
| **Reduced 10%** (art. 91.Uno: food, water, medical devices, housing, transport of passengers, hospitality/restaurants, etc.) | LIVA art. 91.Uno [PRIMARY]; art. 91 amended effective 1 Sep 2012 (RDL 20/2012) [PRIMARY, BOE note] | 1 Sep 2012 | `REDUCIDO` 10% |
| **Superreduced 4%** (art. 91.Dos: basic food incl. olive oil from 1 Jan 2025, books/press, medicines, VPO housing, sanitary products, …) | LIVA art. 91.Dos [PRIMARY]; olive oil into Dos.1.1.º "con efectos desde el 1 de enero de 2025" (RDL 4/2024) [PRIMARY, BOE note] | — | `SUPERREDUCIDO` 4% |
| **0%** for goods donated to non-profit entities (Ley 49/2002) used for general-interest purposes | LIVA art. 91.Cuatro, added by Ley 7/2022 [PRIMARY] | 2022 [SECONDARY on exact date] | `CERO` 0% class (taxable at 0%, casillas 150–152) |
| **Coming change (pending):** RDL 29/2026 (6 Oct 2026, BOE 7 Oct) moves furnished short-term lettings (≤ 30 nights, not the landlord's home, or with hotel-type services) out of the exemption and into the 10% rate, and rewrites art. 91.Uno.2.10.º and Dos.1.6.º, **with effect from 1 Dec 2026**. The earlier RDL 26/2026 with the same content was *derogado* by Congress (Resolución 2 Oct 2026). RDL 29/2026's convalidation was not yet listed on BOE | BOE consolidated LIVA notes; RDL 29/2026 art. 7 [PRIMARY]; convalidation status [UNCERTAIN] | 1 Dec 2026 (if convalidated) | No rate change; item-level classification only. Re-check before December 2026 |
| **Temporary rates history** — values the AEAT still accepts in VERI*FACTU records by operation date: 5% only for 1 Jul 2022–30 Sep 2024; 2% and 7.5% only for 1 Oct–31 Dec 2024; otherwise 0, 4, 10, 21 | AEAT *Validaciones y errores* v1.2.2 §15.1 [PRIMARY] | historic | Effective-dated history rows (below) |
| Food: 0% (basic food, previously 4%) and 5% (oils and pasta, previously 10%) from 1 Jan 2023 (RDL 20/2022), extended through 30 Sep 2024; then 2% and 7.5% for 1 Oct–31 Dec 2024 (RDL 4/2024, olive oil in the 0%/2% group from mid-2024); back to 4%/10% from 1 Jan 2025 | Tax-advisor summaries of RDL 20/2022, RDL 8/2023, RDL 4/2024 [SECONDARY]; date windows match AEAT validations [PRIMARY] | 2023–2024 | History rows `FOOD_TEMP_0`, `FOOD_TEMP_5`, `FOOD_TEMP_2`, `FOOD_TEMP_7_5` with `valid_from/valid_to`; no default use after 2024 |
| Electricity (and gas/firewood) at reduced rates during the energy crisis (10% from mid-2021, 5% from 1 Jul 2022 through 2023, 10% in 2024 conditional on wholesale prices, 21% from 2025) | Secondary summaries [SECONDARY]; only the 5% window is confirmed by AEAT validations [PRIMARY]; product scope and exact conditions [UNCERTAIN] | historic | Do not seed; allow a manual historic rate |
| **Recargo de equivalencia** (retail traders): 5.2% general; 1.4% for 10% goods; 0.50% for 4% goods; 1.75% for tobacco | LIVA art. 161 [PRIMARY] | 1 Sep 2012 | Pointer: `recargo` column per rate; temporary RE rates 0, 0.26, 0.62, 1 (2023–2024) per AEAT validations §15.3 [PRIMARY] |
| VERI*FACTU accepted rate/RE pairs: 21 → 5.2 or 1.75; 10 → 1.4; 7.5 → 1; 5 → 0.5 (≤ 2022) or 0.62 (2023–30 Sep 2024); 4 → 0.5; 2 → 0.26; 0 → 0 (2023–30 Sep 2024) | AEAT *Validaciones* §15.3 [PRIMARY] | — | Validate RE pairs at entry |

**Effective-dated default list to seed** (territorio común):

| Code | Rate | Valid from | Valid to | 303 casillas (2026) |
|---|---|---|---|---|
| GENERAL | 21% | 2012-09-01 | — | 07/08/09 |
| REDUCIDO | 10% | 2012-09-01 | — | 04/05/06 |
| SUPERREDUCIDO | 4% | 2012-09-01 [SECONDARY on the 4% start] | — | 01/02/03 |
| CERO (donativos art. 91.Cuatro) | 0% | 2022 [SECONDARY] | — | 150/151/152 |
| FOOD_TEMP_0 | 0% | 2023-01-01 | 2024-09-30 | historic (150–152) |
| TEMP_5 | 5% | 2022-07-01 | 2024-09-30 | historic (153–155 in 2023–2024 forms) |
| FOOD_TEMP_2 | 2% | 2024-10-01 | 2024-12-31 | historic [UNCERTAIN casilla] |
| FOOD_TEMP_7_5 | 7.5% | 2024-10-01 | 2024-12-31 | historic [UNCERTAIN casilla] |
| EXENTA (arts. 20–25) | — | — | — | not in 27; 59/60 or none |

---

## 3. Invoices (RD 1619/2012, Reglamento por el que se regulan las obligaciones de facturación)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Number and series**: "Número y, en su caso, serie. La numeración de las facturas dentro de cada serie será correlativa." | RD 1619/2012 art. 6.1.a) [PRIMARY] | 1 Jan 2013 | Gap-free, increasing number per series; number assigned only at issue (§6.12) |
| Separate series allowed "cuando existan razones que lo justifiquen", e.g. several establishments or operations of a different nature | art. 6.1.a) [PRIMARY] | — | User-defined series (per establishment / activity) |
| **Mandatory separate series** for: (1.º) invoices issued by the recipient or a third party (art. 5), one series per issuer; (2.º) **rectificativas**; (3.º) invoices under RIVA DA 5.ª; (4.º) invoices under LIVA art. 84.Uno.2.º g); (5.º) invoices under RIVA art. 61 quinquies.2 | art. 6.1.a) [PRIMARY] | — | Series flags `RECTIFICATIVA`, `AUTOFACTURA` (per third party); refuse mixing |
| Full and simplified invoices in the same calendar year: "será obligatoria la expedición mediante series separadas de unas y otras" | art. 7.1.a) [PRIMARY] | — | Series bound to one invoice kind |
| **Content of a full invoice** (art. 6.1): a) number/series; b) issue date; c) full name / business name of issuer and recipient; d) issuer NIF (recipient NIF mandatory for art. 25 intra-EU supplies, reverse charge, and operations in Spain by an established issuer); e) addresses; f) description with all data for the taxable base, unit price without tax, discounts; g) rate(s); h) tax amount, shown separately; i) operation date (or advance-payment date) if different; j) exemption reference (Directive 2006/112/CE or LIVA article, or "exenta"); k) new means of transport data; l) «facturación por el destinatario»; m) «inversión del sujeto pasivo»; n) «régimen especial de las agencias de viajes»; o) «régimen especial de los bienes usados» / «… de los objetos de arte» / «… de las antigüedades y objetos de colección»; p) «régimen especial del criterio de caja» | RD 1619/2012 art. 6.1 [PRIMARY; legends quoted exactly] | — | Invoice model with every field; legend constants exactly as quoted |
| Bases must be split per exempt/non-exempt, reverse-charge/non, and per rate | art. 6.2 [PRIMARY] | — | Tax lines per (rate, regime) |
| **VERI*FACTU additions**: QR code (for an electronic invoice the QR content may replace the image) and, only when all records are sent to the AEAT, the phrase "Factura verificable en la sede electrónica de la AEAT" or "VERI*FACTU" | art. 6.5 and 7.5, added by RD 1007/2023 [PRIMARY] | 7 Dec 2023 (applies with RRSIF dates, §6.1) | §6.9 |
| **Factura simplificada** allowed when the amount ≤ **€400 IVA incluido**, or for a rectificativa; ≤ **€3,000 IVA incluido** for: retail sales, ambulant sales, home sales, passenger transport, hospitality and restaurants, dance halls, public phone booths, hairdressing/beauty, sports facilities, photo studios, parking, film rental, dry cleaning, toll roads | art. 4.1–4.2 [PRIMARY] | — | Allow kind `SIMPLIFICADA` under the thresholds; VERI*FACTU `TipoFactura` F2 |
| No simplified invoice for exempt intra-EU supplies (art. 25 LIVA), certain distance sales, reverse-charge self-billing by the recipient, art. 2.3.b) b') | art. 4.4 [PRIMARY] | — | Block |
| **Simplified content** (art. 7.1): number/series; issue date; operation date if different; issuer NIF and name; type of goods/services; rate and optionally «IVA incluido» (base split per rate if several); total; for rectificativas the reference to the corrected invoice; legends j)–p). If a business recipient asks: recipient NIF and address, and the tax amount separately (art. 7.2) | art. 7 [PRIMARY] | — | Simplified layout; "convert to full" path → F3 (§6.5) |
| **Issue deadline**: "en el momento de realizarse la operación"; for a business recipient "antes del día 16 del mes siguiente" to the *devengo*; same for intra-EU supplies (from start of transport); *recapitulativas* per calendar month | arts. 11, 13 [PRIMARY] | — | Warn on uninvoiced deliveries past the deadline |
| **Currency/language**: amounts in any currency, but the tax "se exprese en euros"; any language | art. 12 [PRIMARY] | — | Invoice currency + EUR tax totals |
| **One original only**; duplicates only for several recipients or loss, marked «duplicado» | art. 14 [PRIMARY] | — | Reprint = copy, never a new number |
| **Rectificativas**: mandatory when the original lacks art. 6/7 requirements, tax was wrongly determined, or an art. 80 LIVA base modification occurs; within four years of the *devengo*; the new invoice identifies the corrected invoice(s) (or only the period for volume discounts); amounts either as the difference or as the corrected totals with the difference shown; a full invoice replacing simplified invoices is **not** a rectificativa | art. 15 [PRIMARY] | — | Rectificativa = new document in its own series, links originals; VERI*FACTU R1–R5 with `TipoRectificativa` S (sustitución) / I (diferencias) (§6.5) |
| **Retention**: issued copies, received invoices and justificantes for the LGT period | art. 19 [PRIMARY]; LGT four-year limitation and Código de Comercio six years [SECONDARY] | — | Retention metadata; VERI*FACTU records kept as long as the invoices (§6.3) |
| Electronic invoice definition changed by RD 238/2026 (B2B mandatory e-invoicing) | RD 1619/2012 arts. 8–10 as amended, in force 20 Apr 2026 [PRIMARY, BOE notes] | 20 Apr 2026 | See §8 |

---

## 4. Modelo 303 (IVA, autoliquidación)

### 4.1 Filing rules

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Period = **calendar quarter**; **monthly** for: (1.º) volume of operations in the prior year > **€6,010,121.04**; (2.º) acquirers of a business pushing the combined volume over that amount; (3.º) taxpayers in the monthly-refund register (REDEME, RIVA art. 30); (4.º) VAT groups; (5.º) fuel tax-warehouse keepers | RIVA art. 71.3 [PRIMARY] | — | Period selector; flag monthly when any condition holds |
| **Due dates**: quarterly "durante los veinte primeros días naturales del mes siguiente" (April, July, October); last period "durante los treinta primeros días naturales del mes de enero"; monthly (SII filers, art. 62.6) within the first 30 days of the next month, January's return by the last day of February | RIVA art. 71.4 [PRIMARY]; AEAT Instrucciones 2026 [PRIMARY] | — | Due-date calendar (Q3 2026 was due 20 Oct 2026) |
| **Electronic only** (certificate, Cl@ve or eIDAS); individuals may use Cl@ve; SA/SL, large companies and monthly filers must use a certificate | AEAT Instrucciones 2026 [PRIMARY] | — | Export the official file; submission via Pre303 / AEAT services is future work |
| Result types (header "Tipo Declaración"): C compensación, D devolución, G cuenta corriente tributaria-ingreso, I ingreso, N sin actividad/resultado cero, V cuenta corriente tributaria-devolución, U domiciliación, X devolución por transferencia al extranjero | DR303e26v101, page 01, Nota 1 [PRIMARY] | 2026 | Map casilla 71 sign + user choice |
| Annual summary **modelo 390** due 1–30 January (some quarterly filers are exempt and fill page 4 of the 4T/12 return instead) | AEAT Instrucciones 2026 [PRIMARY] | — | Pointer (390 not implemented) |
| File format: positional text records `<T3030…>` per page; numeric amounts 17 positions = "15 enteros y 2 decimales" (implied decimals, right-aligned, zero-padded); signed fields carry "N" in the first position when negative; design "deber ser utilizado para las presentaciones a partir del ejercicio 2026" | DR303e26v101 (actualizado 28/01/26), sheets DP30300/DP30301, Notas 2 and 4 [PRIMARY] | 2026 | Writer for the AEAT import file (future); model data in cents |

### 4.2 Casillas (2026; labels from the AEAT *diseño de registro* DR303e26v101 and *Instrucciones 2026*)

Régimen general — IVA devengado (page 01):

| Casillas (base / tipo / cuota) | Label (DR303e26v101) | Tipo constant in the design | Meaning per Instrucciones 2026 |
|---|---|---|---|
| **150 / 151 / 152** | IVA Devengado - Régimen general - Base imponible / Tipo % / Cuota | "00000" | "bases imponibles gravadas al tipo del 0% y las cuotas resultantes" |
| 165 / 166 / 167 | IVA Devengado - Régimen general | "00000" | listed among devengado casillas ("01 a 09 y 150 a 155 y 165 a 167") with no rate assigned in 2026 [UNCERTAIN] |
| **01 / 02 / 03** | IVA Devengado - Régimen general | "00400" | 4% |
| 153 / 154 / 155 | IVA Devengado - Régimen general | "00000" | was 5% in the 2023–2024 instructions; no 2026 rate assigned [UNCERTAIN] |
| **04 / 05 / 06** | IVA Devengado - Régimen general | "01000" | 10% |
| **07 / 08 / 09** | IVA Devengado - Régimen general | "02100" | 21% |
| **10 / 11** | Adquisiciones intracomunitarias de bienes y servicios - Base imponible / Cuota | — | total intra-EU acquisitions of goods and services ("Todos, incluido cero") |
| **12 / 13** | Otras operaciones con inversión del sujeto pasivo (excepto. adq. intracom) - Base imponible / Cuota | — | reverse charge under art. 84.Uno.2.º and 4.º other than 10/11 |
| **14 / 15** | Modificación bases y cuotas - Base imponible / Cuota | — | art. 80 LIVA modifications, signed, not split by rate |
| 156 / 157 / 158 | Recargo equivalencia | "00175" | RE 1.75% |
| 168 / 169 / 170 | Recargo equivalencia | "00050" | RE 0.5% |
| 16 / 17 / 18 | Recargo equivalencia | "00000" | RE 0% (historic) [UNCERTAIN use in 2026] |
| 19 / 20 / 21 | Recargo equivalencia | "00140" | RE 1.4% |
| 22 / 23 / 24 | Recargo equivalencia | "00520" | RE 5.2% |
| 25 / 26 | Modificaciones bases y cuotas del recargo de equivalencia | — | signed |
| **27** | Total cuota devengada ( [152] + [167] + [03] + [155] + [06] + [09] + [11] + [13] + [15] + [158] + [170] + [18] + [21] + [24] + [26] ) | — | — |

IVA deducible:

| Casillas | Label (DR303e26v101) |
|---|---|
| **28 / 29** | Por cuotas soportadas en operaciones interiores corrientes - Base / Cuota |
| **30 / 31** | Por cuotas soportadas en operaciones interiores con bienes de inversión - Base / Cuota |
| 32 / 33 | Por cuotas soportadas en las importaciones de bienes corrientes - Base / Cuota |
| 34 / 35 | Por cuotas soportadas en las importaciones de bienes de inversión - Base / Cuota |
| **36 / 37** | En adquisiciones intracomunitarias de bienes y servicios corrientes - Base / Cuota |
| 38 / 39 | En adquisiciones intracomunitarias de bienes de inversión - Base / Cuota |
| 40 / 41 | Rectificación de deducciones - Base / Cuota (signed) |
| 42 | Compensaciones Régimen Especial A.G. y P. - Cuota |
| 43 | Regularización inversiones - Cuota |
| 44 | Regularización por aplicación del porcentaje definitivo de prorrata - Cuota (only 4T/12 or cessation) |
| **45** | Total a deducir ( [29] + [31] + [33] + [35] + [37] + [39] + [41] + [42] + [43] + [44] ) |
| **46** | Resultado régimen general ( [27] - [45] ) |

Instructions: deductible bases are shown unprorated, quotas after prorrata; "En este apartado no deben incluirse las adquisiciones a tipo cero o exentas" [PRIMARY]. Régimen simplificado (page 02, casillas 47–58) is out of scope.

Información adicional and resultado (page 03):

| Casilla | Label (DR303e26v101) |
|---|---|
| **59** | Entregas intracomunitarias de bienes y servicios |
| **60** | Exportaciones y operaciones asimiladas |
| **120** | Operaciones no sujetas por reglas de localización (excepto las incluidas en la casilla 123) |
| **122** | Operaciones sujetas con inversión del sujeto pasivo |
| 123 | Operaciones no sujetas por reglas de localización acogidas a los regímenes especiales de ventanilla única |
| 124 | Operaciones sujetas y acogidas a los regímenes especiales de ventanilla única |
| 62 / 63 | Criterio de caja: entregas … devengadas conforme a la regla general de devengo contenida en el art. 75 LIVA - Base Imponible / Cuota |
| 74 / 75 | Criterio de caja: adquisiciones … - Base Imponible / Cuota |
| 76 | Regularización cuotas art. 80.cinco.5ª LIVA |
| **64** | Suma de resultados ( [46] + [58] + [76] ) |
| **65** | % Atribuible a la Administración del Estado (100% unless joint State/foral taxation) |
| **66** | Atribuible a la Administración del Estado (= 64 × 65) |
| **77** | IVA a la importación liquidado por la Aduana pendiente de ingreso |
| **110** | Cuotas a compensar pendientes de periodos anteriores |
| **78** | Cuotas a compensar de periodos anteriores aplicadas en este periodo |
| **87** | Cuotas a compensar de periodos previos pendientes para periodos posteriores ([110] - [78]) |
| **68** | Exclusivamente para sujetos pasivos que tributan conjuntamente a la Administración del Estado y a las Haciendas Forales Resultado de la regularización anual |
| 108 | Rectificativa — discrepancia de criterio administrativo, otros ajustes |
| **69** | Resultado de la autoliquidación ( [66] + [77] - [78] + [68] + [108] ) |
| **70** | Resultados a ingresar de anteriores autoliquidaciones o liquidaciones administrativas … (rectificativa only) |
| 109 | Devoluciones acordadas por la Agencia Tributaria … (rectificativa only) |
| **112** (new 2026) | Pago a cuenta de entregas de gasolinas, gasóleos y biocarburantes posteriores a la ultimación del régimen de depósito distinto del aduanero atribuible a la Administración del Estado (Suma de la casilla 36 de todos los modelos 319 …) |
| **71** | Resultado ( [69] - [70] + [109] - [112] ) |
| 111 | Rectificativa - Rectificación - Importe |

Page 04 (only last period, taxpayers exonerated from the 390): territory % 89 Álava, 90 Guipuzcoa, 91 Vizcaya, 92 Navarra, 107 Territorio común; volume of operations 80, 81, 93, 94, 83, 84, 125, 126, 127, 128, 86, 95, 96, 97, 98, 79, 99, total **88**. Page 05: prorrata (500–524, CNAE-2025) and *sectores diferenciados* (700–735). [PRIMARY, DR303e26v101]

**Casilla 61 does not exist** in the 2026 design (the full list of casillas printed in DR303e26v101 was extracted and compared) [PRIMARY]; taxdown says it was removed in July 2021 by Orden HAC/646/2021 [SECONDARY].

### 4.3 2026 changes

| Change | Source | Effective date | What the software must do |
|---|---|---|---|
| New casilla **112** and header flag "Sujeto pasivo con derecho a deducir pago a cuenta de entregas de gasolinas, gasóleos y biocarburantes…" (fuel tax-warehouse regime, modelo 319); 71 = 69 − 70 + 109 − 112; rectificativa rules for 70/109 use 69 − 112 | Instrucciones 02 a 12 y 2T a 4T de 2026 vs 01-1T (diffed here) [PRIMARY]; header flag Nota 8 "0 Para los periodos 1T, 2T, 3T, 4T y 01" [PRIMARY]; the approving order (a blog names "Orden HAC/27/2026") [UNCERTAIN] | from period 02 / 2T 2026 | Always 0 for non-fuel businesses; keep the formula |
| Rate rows: 2026 instructions list only 0%, 4%, 10%, 21% and RE 1.75 / 0.5 / 1.4 / 5.2; the 2025 instructions were the same; the 2024 instructions still had 5% (153–155) and RE "Cero, 0,5% y 0,62%" in 16–18 | AEAT Instrucciones 2024, 2025, 2026 [PRIMARY] | 2025 → | Map 2023–2024 history rates to the year-specific layout |
| CNAE-2025 codes in the prorrata block | DR303e26v101 page 05 [PRIMARY] | 2026 | — |

Implementation notes: 27 − 45 = 46 must equal the books' output VAT minus deductible input VAT for the period, to the cent; the form has no rounding rule beyond two decimals [UNCERTAIN whether per-invoice or per-line rounding is expected; common practice is per-invoice per-rate].

---

## 5. SII (Suministro Inmediato de Información) — pointer only

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Who: the *libros registro* are kept "a través de la Sede electrónica de la AEAT, mediante el suministro electrónico de los registros de facturación" by taxpayers whose liquidation period is the calendar month under art. 71.3 (large companies > €6,010,121.04, REDEME, VAT groups, fuel warehouses), plus voluntary adherents | RIVA art. 62.6 [PRIMARY]; header flag "Sujeto pasivo acogido voluntariamente al SII" in 303 [PRIMARY] | 1 Jul 2017 | Out of scope; flag the company as SII |
| Deadline: issued invoices within **four calendar days** of issue (eight if issued by the recipient/third party), always before the 16th of the next month; received invoices within four days of booking; Saturdays, Sundays and national holidays excluded from the count | RIVA art. 69 bis [PRIMARY] | — | — |
| **SII taxpayers are outside the RRSIF**: "El presente Reglamento no se aplicará a los contribuyentes que lleven los libros registros en los términos establecidos en el apartado 6 del artículo 62" RIVA; the AEAT confirms all SII adherents "están excluidos del cumplimiento de la normativa del RD 1007/2023" | RD 1007/2023 art. 3.3 [PRIMARY]; AEAT FAQ v1.3 §12.2 [PRIMARY] | — | If the company is SII, disable VERI*FACTU; a switch RRSIF ↔ SII is a change of SIF (new installation number) per FAQ §3 [PRIMARY] |

---

## 6. VERI*FACTU (Reglamento de requisitos de los sistemas informáticos de facturación, RRSIF)

### 6.1 Legal basis and dates

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Legal basis: LGT art. 29.2.j) — systems must guarantee "la integridad, conservación, accesibilidad, legibilidad, trazabilidad e inalterabilidad de los registros" | RD 1007/2023 art. 1.1 [PRIMARY]; FAQ §11: art. 29.2.j) LGT in force since October 2021 for any system [PRIMARY] | Oct 2021 / RRSIF dates | Applies to the whole invoicing flow |
| RD 1007/2023 (BOE 6 Dec 2023) in force 7 Dec 2023; **original deadline 1 Jul 2025** for users | BOE consolidated text [PRIMARY]; FAQ v1.3 §1 "La norma original … establecía como fecha límite … el 1 de julio de 2025" [PRIMARY] | — | History only |
| **Orden HAC/1177/2024** of 17 Oct 2024, published 28 Oct 2024, in force the next day (29 Oct 2024); corrección de errores BOE 8 Nov 2024 | FAQ §1 [PRIMARY]; Orden DF única [PRIMARY]; BOE-A-2024-23180 note [PRIMARY] | 29 Oct 2024 | Technical source of truth |
| **RD 254/2025** (1 Apr 2025, BOE 2 Apr, in force 3 Apr 2025) moved the dates to **before 1 Jan 2026** (IS taxpayers, art. 3.1.a) and **before 1 Jul 2026** (everyone else) | BOE consolidated DF 4.ª history [PRIMARY]; FAQ §1 quotes the text [PRIMARY] | superseded | History only |
| **RDL 15/2025** (2 Dec 2025, BOE 3 Dec, in force 4 Dec 2025), DF 1.ª: IS taxpayers "deberán tener adaptados los sistemas informáticos … antes del 1 de enero de 2027. El resto de obligados tributarios mencionados en el artículo 3.1 deberán tener operativos los citados sistemas informáticos antes del 1 de julio de 2027." **Convalidated** by Congress (Resolución 11 Dec 2025, BOE-A-2025-25695) | BOE-A-2025-24446 and its *Referencias posteriores* [PRIMARY] | **current as of 2026-10-21**: 1 Jan 2027 / 1 Jul 2027 | Hard dates in config; the consolidated RD 1007/2023 shows no later change (last update 03/12/2025) [PRIMARY] |
| **Producers** (art. 3.2) "deberán ofrecer sus productos plenamente adaptados al reglamento en el plazo máximo de nueve meses desde la entrada en vigor de la orden ministerial" — i.e. **29 Jul 2025**; after that date producers may offer only adapted SIF, except multi-year maintenance contracts | RD 1007/2023 DF 4.ª as amended [PRIMARY]; FAQ §1 phase b) "hasta el 29/07/2025 … fecha clave para los productores" [PRIMARY] | 29 Jul 2025 | KhataOS offered to Spanish users must be adapted now [UNCERTAIN whether free open-source distribution is "comercialización"] |
| AEAT VERI*FACTU services (sending, QR *cotejo*, query) in production since **23 Apr 2025**; adapted systems may be used now "con plena validez fiscal" | FAQ v1.3 §1–2 [PRIMARY] | 23 Apr 2025 | Early adoption allowed |

### 6.2 Scope

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Who: (a) IS taxpayers (except fully exempt entities; partially exempt only for taxable income); (b) IRPF taxpayers with economic activities; (c) IRNR with a permanent establishment; (d) *entidades en atribución de rentas* — "aunque solo los usen para una parte de su actividad" | RD 1007/2023 art. 3.1 [PRIMARY] | — | Company flag `rrsif_class` = IS (2027-01-01) / other (2027-07-01) |
| Not applicable to: SII taxpayers (art. 3.3); operations of RD 1619/2012 DA 3.ª and 6.ª; invoices of foreign permanent establishments; self-billed/third-party invoices where the issuer keeps SII-style books | RD 1007/2023 arts. 3.3, 4 (as amended by RD 254/2025) [PRIMARY] | — | Exclusion switches |
| A SIF is hardware + software that (a) accepts invoicing data, (b) keeps it, (c) processes it; "factura" includes full and simplified invoices | RD 1007/2023 art. 1.2 [PRIMARY] | — | KhataOS invoicing = a SIF |
| Multi-company: the same system may serve several taxpayers if records are separated; it must behave as independent systems per taxpayer: separate record and event chains, VERI*FACTU per taxpayer, and always show which taxpayer is active | RD 1007/2023 art. 7.a); Orden art. 2 [PRIMARY] | — | One chain per (company, installation); `IndicadorMultiplesOT` = S when the user has more than one invoicing set (FAQ §4) [PRIMARY] |
| A SIF is identified by NIF of the issuer + `IdSistemaInformatico` (2 chars, chosen by the producer) + `NumeroInstalacion`, which "no puede repetirse nunca" (reinstall ⇒ new number); recommended: install timestamp or a never-reused sequence. Each separate invoicing set (company or independent shop) needs its own installation number | FAQ v1.3 §4 [PRIMARY] | — | Generate `NumeroInstalacion` at setup; never reuse |
| The user must not switch dynamically between adapted/non-adapted, or between RRSIF/TicketBAI/SII, per session or invoice; a permanent change = a new SIF | FAQ §3 [PRIMARY] | — | Mode chosen at company setup; changing it creates a new installation |

### 6.3 System requirements (RD 1007/2023 art. 8; Orden HAC/1177/2024 arts. 6–9)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Integrity/inalterability**: once generated, records "no puedan ser alterados sin que el sistema informático lo detecte y avise de ello"; any correction or cancellation "mediante al menos un registro de facturación adicional posterior" | RD art. 8.2.a) [PRIMARY] | — | Append-only record store; no UPDATE/DELETE |
| **Traceability**: records chained "desde el primero al último", every record dated with the moment of registration | RD art. 8.2.b) [PRIMARY] | — | Chain (§6.8) |
| **Conservation, accessibility, legibility** for the LGT period, with a secure download/export to external storage in a readable electronic format | RD art. 8.2.c) [PRIMARY]; Orden art. 8 [PRIMARY] | — | Export records per period as XML (structure of annex §3); delete exported records only oldest-first, never leaving gaps (Orden art. 8.2) |
| The user must keep the records as long as the invoice copies (RD 1619/2012 art. 19.1), even after changing system | Orden art. 8.3 [PRIMARY] | — | Retention metadata |
| Capacity to send all records to the AEAT "de forma continuada, segura, correcta, íntegra, automática, consecutiva, instantánea y fehaciente": Internet, certificate handling, secure protocols, processing responses | RD art. 8.1; Orden arts. 4–5 [PRIMARY] | — | Even "no VERI*FACTU" systems must be able to send (on request, *Requerimiento* service) |
| Hash per record (Orden art. 13), check one record's hash on demand, check all/part of the chain "de forma rápida, fácil e intuitiva"; alarm + event when integrity cannot be guaranteed | Orden art. 6 [PRIMARY] | — | "Verify chain" command and UI |
| Navigate the chain backwards/forwards from any record, showing whether each hash link and the timestamp order are correct | Orden art. 7.h) [PRIMARY] | — | Chain viewer |
| Clock: record timestamp = exact local time where the invoice is issued, with time zone; the user must ensure the clock is within **one minute** | Orden art. 7.e)–g) [PRIMARY] | — | NTP check / warning (needed for "no VERI*FACTU"; good practice for both) |
| Separate access to tax-relevant data from confidential non-financial data so the AEAT can consult records and events directly | RD art. 8.4 [PRIMARY] | — | Read-only inspector role |
| Records are **XML, UTF-8**, structure per the annex (§3 alta/anulación; §5 events) | Orden arts. 9.4, 10, 11 [PRIMARY] | — | Serialise with the AEAT schema (§6.10) |
| **Declaración responsable** by the producer, starting with the title «DECLARACIÓN RESPONSABLE DEL SISTEMA INFORMÁTICO DE FACTURACIÓN», with (a) system name, (b) system code, (c) version, (d) components and functions, (e) whether it can only work as VERI*FACTU, (f) multi-taxpayer capability, (g) signature types (non-VERI*FACTU), (h) producer name, (i) producer NIF (or other ID + country), (j) postal address, (k) compliance statement (LGT 29.2.j, RD 1007/2023, the Orden, AEAT sede specs), (l) date (day, month, year) and place (town, country); visible inside the system for every version | RD art. 13 [PRIMARY]; Orden art. 15 [PRIMARY]; AEAT *Ejemplos de declaraciones responsables* (not read) | — | Ship a per-version declaration screen and file; producer identity for an open-source project [UNCERTAIN] |

### 6.4 The two modes

| Aspect | VERI*FACTU | "No VERI*FACTU" | Source |
|---|---|---|---|
| Definition | System that effectively sends **all** records to the AEAT continuously and instantly | Adapted system that keeps records locally | RD art. 16.1 [PRIMARY] |
| Presumed compliant with art. 8 "por diseño"; AEAT copy request (art. 14.2) does not apply | Yes | No | RD art. 16.2 [PRIMARY] |
| Electronic signature of records | Not required ("siendo suficiente con que calculen la huella") | **XAdES Enveloped** (ETSI EN 319 132) with a qualified certificate on the EU Trusted List | RD arts. 12, 16.3; Orden art. 14 [PRIMARY] |
| Orden articles that do not apply | 6.b)–f), 7.f), 7.h), 7.i), 7.j), 8 and 9 | all apply | Orden art. 3 [PRIMARY] |
| Event log (events L2E 01–10, 90; summary event at least every 6 hours of operation and before shutdown; own hash chain) | Not required | Required | Orden art. 9 [PRIMARY]; FAQ §15.5 [PRIMARY] |
| Invoice legend | "VERI*FACTU" or "Factura verificable en la sede electrónica de la AEAT" | none | Orden art. 20.1.b) [PRIMARY] |
| QR URL path | `ValidarQR` | `ValidarQRNoVerifactu` | QR spec v0.5.0 §5 [PRIMARY] |
| Opt-in/out | Starts by systematically sending; lasts at least to 31 Dec of the year of the first effective send; renounce via `FechaFinVeriFactu` in the message header before year-end | — | RD art. 16.5; Orden art. 17 [PRIMARY] |
| Certificate | Qualified certificate needed for TLS authentication when sending (also for "no VERI*FACTU", for signing); FAQ: an adapted SIF can only be used with "un certificado electrónico cualificado válido" installed | both | Orden art. 5; FAQ §3, §11 [PRIMARY] |
| Flow control | Wait `t` seconds (initially **60**) between sends, or until 1,000 records accumulate; `t` updated by each response; on outage retry at least every hour, flag `Incidencia` and show how many records are pending | — | Orden art. 16.2–16.4 [PRIMARY]; annex §2.2 (1–1,000 records per send) [PRIMARY] |
| AEAT validation of a wrong hash | Record "Aceptado con errores" | — | Hash spec §7 [PRIMARY] |

**KhataOS recommendation**: implement VERI*FACTU mode first (no XAdES, no event log), with an offline queue that preserves generation order and retries hourly. "No VERI*FACTU" (XAdES + event log) is a later option for fully offline users [design decision, not a legal statement].

### 6.5 Registro de facturación de alta (`RegistroAlta`)

Content required by RD 1007/2023 art. 10 [PRIMARY]: issuer NIF and name; recipient NIF and name when mandatory; issued by issuer/recipient/third party; number and series; issue date and operation date; invoice type (full/simplified, …); rectificativa type and corrected invoices; replaced simplified invoices; general description; total amount; VAT regime(s); whether the recipient is the taxpayer (art. 84.Uno.2.º/3.º); base, rate, quota, RE rate and quota; exempt base and cause; non-subject amount and cause; previous record's number/series/date and part of its hash (except the first); system ID and producer data; generation date-time to the second. All amounts in euros (art. 10.2).

Element order in `RegistroFacturacionAltaType` (AEAT `SuministroInformacion.xsd`, v1.0) [PRIMARY]; `?` = optional, `[n]` = max occurrences:

`IDVersion` ("1.0", list L15) · `IDFactura` { `IDEmisorFactura` (NIF, 9) · `NumSerieFactura` (1–60) · `FechaExpedicionFactura` (`dd-mm-yyyy`) } · `RefExterna`? (≤ 60) · `NombreRazonEmisor` (≤ 120) · `Subsanacion`? · `RechazoPrevio`? · `TipoFactura` · `TipoRectificativa`? · `FacturasRectificadas`? { `IDFacturaRectificada`[1000] } · `FacturasSustituidas`? { `IDFacturaSustituida`[1000] } · `ImporteRectificacion`? { `BaseRectificada`, `CuotaRectificada`, `CuotaRecargoRectificado`? } · `FechaOperacion`? · `DescripcionOperacion` (≤ 500) · `FacturaSimplificadaArt7273`? · `FacturaSinIdentifDestinatarioArt61d`? · `Macrodato`? · `EmitidaPorTerceroODestinatario`? · `Tercero`? · `Destinatarios`? { `IDDestinatario`[1000] (NombreRazon + NIF or IDOtro{CodigoPais?, IDType, ID}) } · `Cupon`? · `Desglose` { `DetalleDesglose`[12] } · `CuotaTotal` · `ImporteTotal` · `Encadenamiento` { `PrimerRegistro` ("S") | `RegistroAnterior` { `IDEmisorFactura`, `NumSerieFactura`, `FechaExpedicionFactura`, `Huella` } } · `SistemaInformatico` · `FechaHoraHusoGenRegistro` (xs:dateTime with offset, e.g. `2024-09-13T19:20:30+01:00`) · `NumRegistroAcuerdoFacturacion`? · `IdAcuerdoSistemaInformatico`? · `TipoHuella` ("01" = SHA-256, list L12) · `Huella` (64) · `ds:Signature`? (only "no VERI*FACTU").

`DetalleDesglose`: `Impuesto`? (L1: 01 IVA, 02 IPSI, 03 IGIC, 05 Otros; default 01) · `ClaveRegimen`? (L8A, e.g. 01 régimen general, 02 exportación, 07 criterio de caja, 18 recargo de equivalencia, 20 régimen simplificado) · `CalificacionOperacion` (L9: S1 sujeta no exenta sin ISP, S2 con ISP, N1 no sujeta arts. 7, 14, otros, N2 no sujeta por reglas de localización) **or** `OperacionExenta` (L10: E1 art. 20, E2 art. 21, E3 art. 22, E4 arts. 23 y 24, E5 art. 25, E6 otros) · `TipoImpositivo`? · `BaseImponibleOimporteNoSujeto` · `BaseImponibleACoste`? · `CuotaRepercutida`? · `TipoRecargoEquivalencia`? · `CuotaRecargoEquivalencia`? [PRIMARY, XSD + Orden annex lists].

`SistemaInformatico`: `NombreRazon` (producer) · `NIF` | `IDOtro` · `NombreSistemaInformatico` (≤ 30) · `IdSistemaInformatico` (≤ 2) · `Version` (≤ 50) · `NumeroInstalacion` (≤ 100) · `TipoUsoPosibleSoloVerifactu` (S/N) · `TipoUsoPosibleMultiOT` (S/N) · `IndicadorMultiplesOT` (S/N) [PRIMARY, XSD].

`TipoFactura` (list L2) [PRIMARY, Orden annex]: **F1** Factura (art. 6, 7.2 y 7.3 del RD 1619/2012); **F2** Factura Simplificada y Facturas sin identificación del destinatario art. 6.1.d) RD 1619/2012; **F3** Factura emitida en sustitución de facturas simplificadas facturadas y declaradas; **R1** Factura Rectificativa (Error fundado en derecho y art. 80 Uno Dos y Seis LIVA); **R2** (art. 80.3); **R3** (art. 80.4); **R4** (Resto); **R5** Factura Rectificativa en facturas simplificadas. `TipoRectificativa` (L3): **S** Por sustitución, **I** Por diferencias.

Formatting and validation rules:

| Rule | Source |
|---|---|
| Amounts `ImporteSgn12.2Type`: pattern `(\+\|-)?\d{1,12}(\.\d{0,2})?` (the `\|` is the regex alternation, escaped for this table) — "." decimal separator, optional sign, max 12 integer digits | XSD [PRIMARY] |
| No leading zeros in numbers ("01 ó 001 ó 01230 serían incorrectos"); trailing decimal zeros only indicate precision (12345.7 ≡ 12345.70); dates keep leading zeros (`02-07-2014`) | *Descripción de servicios web* v1.0.3 §6.8 [PRIMARY] |
| `NumSerieFactura`: printable ASCII 32–126 only, and not `"` `'` `<` `>` `=`; surrounding spaces are stripped and stored trimmed ("12345678 / G33") | *Validaciones* v1.2.2 [PRIMARY]; SWeb doc [PRIMARY] |
| `FechaExpedicionFactura` ≤ today and ≥ 28-10-2024; may be earlier than `FechaOperacion` only for ClaveRegimen 14/15 | *Validaciones* [PRIMARY] |
| `IDEmisorFactura` must equal the header's `ObligadoEmision/NIF` | *Validaciones* [PRIMARY] |
| `CuotaTotal` = Σ(CuotaRepercutida + CuotaRecargoEquivalencia); `ImporteTotal` = Σ(BaseImponibleOimporteNoSujeto + CuotaRepercutida + CuotaRecargoEquivalencia); ±€10.00 tolerance, warning only; not for ClaveRegimen 03, 05, 06, 08, 09 | *Validaciones* §16–17 [PRIMARY] |
| `FechaHoraHusoGenRegistro` ≤ AEAT system time (with a margin), warning only | *Validaciones* §20 [PRIMARY] |
| An invoice ID (issuer + series/number + issue date) can have only one alta record at the AEAT ("Registro de facturación duplicado") | FAQ §6 [PRIMARY] |

### 6.6 Registro de facturación de anulación (`RegistroAnulacion`)

Used "cuando se haya emitido erróneamente una factura y sea por lo tanto necesario anular su correspondiente registro de facturación de alta" (RD art. 11.1) [PRIMARY]. Content (art. 11.2): generator NIF and name; generated by issuer/recipient/third party; number/series and issue date of the cancelled invoice; previous record's identification and hash; system data; generation timestamp.

XSD order (`RegistroFacturacionAnulacionType`) [PRIMARY]: `IDVersion` · `IDFactura` { `IDEmisorFacturaAnulada` · `NumSerieFacturaAnulada` · `FechaExpedicionFacturaAnulada` } · `RefExterna`? · `SinRegistroPrevio`? · `RechazoPrevio`? · `GeneradoPor`? (L16: E expedidor, D destinatario, T tercero) · `Generador`? · `Encadenamiento` { `PrimerRegistro` | `RegistroAnterior` } · `SistemaInformatico` · `FechaHoraHusoGenRegistro` · `TipoHuella` · `Huella` · `ds:Signature`?.

A cancelled invoice answers "Factura no encontrada" in the QR *cotejo* (QR spec v0.4.6) [PRIMARY]. Corrections that do not require a rectificativa are made with a new alta record with `Subsanacion`=S and the same invoice key; the original record stays unaltered (SWeb doc §9.1.2) [PRIMARY].

### 6.7 Huella (hash) — exact specification

Source: AEAT, *Detalle de las especificaciones técnicas para generación de la huella o hash de los registros de facturación*, **v0.1.2, 27/08/2024** (current on the developer page on 2026-10-21) [PRIMARY]; Orden HAC/1177/2024 art. 13 and list L12 [PRIMARY].

| Rule | Text / detail |
|---|---|
| Algorithm | "En la fecha de publicación de este documento el único algoritmo permitido es SHA-256" (L12 code `01`) |
| Fields, **alta** (in this order) | 1 `IDEmisorFactura` (RegistroAlta/IDFactura/IDEmisorFactura), 2 `NumSerieFactura`, 3 `FechaExpedicionFactura`, 4 `TipoFactura`, 5 `CuotaTotal`, 6 `ImporteTotal`, 7 `Huella` (RegistroAlta/Encadenamiento/RegistroAnterior/Huella), 8 `FechaHoraHusoGenRegistro` |
| Fields, **anulación** | 1 `IDEmisorFacturaAnulada`, 2 `NumSerieFacturaAnulada`, 3 `FechaExpedicionFacturaAnulada`, 4 `Huella` (RegistroAnulacion/Encadenamiento/RegistroAnterior/Huella), 5 `FechaHoraHusoGenRegistro` |
| Fields, **evento** ("no VERI*FACTU") | 1 `NIF` (SistemaInformatico/NIF), 2 `ID` (SistemaInformatico/IDOtro/ID), 3 `IdSistemaInformatico`, 4 `Version`, 5 `NumeroInstalacion`, 6 `NIF` (ObligadoEmision/NIF), 7 `TipoEvento`, 8 `HuellaEvento` (EventoAnterior/HuellaEvento), 9 `FechaHoraHusoGenEvento` |
| Structure | `nombreCampo1=valorCampo1&nombreCampo2=valorCampo2&…&nombreCampoN=valorCampoN`; the name is "un valor constante tal y como se describe en el XML del diseño de registro" |
| Values | the same information as the XML field, "eliminando los espacios al inicio y al final de cada valor"; values are **not** URL-encoded (the Java sample uses plain `valor.trim()`) |
| Numbers | "se tratarán indistintamente los valores con una o dos posiciones en los decimales, sin tener relevancia los ceros a la derecha" (123.1 ≡ 123.10). **KhataOS rule**: hash exactly the string written to the XML and always write two decimals [design decision; the AEAT's normalisation is described only as tolerance: UNCERTAIN for other digit counts] |
| Missing/empty field | name and `=` only, e.g. first record `…ImporteTotal=123.45&Huella=&FechaHoraHusoGenRegistro=…`; event with NIF but no ID: `NIF=89890001K&ID=&IdSistemaInformatico=…` |
| Encoding | the string "será codificada en un array de bytes en formato UTF-8" |
| Output | hexadecimal, **uppercase**, 64 characters, stored in `RegistroAlta/Huella`, `RegistroAnulacion/Huella` or `RegistroEvento/Evento/HuellaEvento` |
| First record | `PrimerRegistro` = "S", no `RegistroAnterior` block, but the record's own `Huella` must still be computed (with `Huella=` empty in the input) |
| Chain content | the previous record's NIF, number/series, issue date and "Los primeros 64 caracteres de la huella" (= the full SHA-256 hex) | Orden art. 7.a) [PRIMARY] |

Java reference (page 8 of the PDF, read from the page image) [PRIMARY]: `getValorCampo(nombre, valor, separador)` returns `nombre + "=" + (valor == null ? "" : valor.trim())` plus `"&"` except after the last field; the digest is `MessageDigest.getInstance("SHA-256").digest(msg.getBytes(StandardCharsets.UTF_8))` encoded with `new Base16(false)` (uppercase).

**Golden examples** (copied verbatim from §6 of the AEAT document; line breaks in the PDF removed) and **verification** (computed here with `hashlib.sha256(s.encode('utf-8')).hexdigest().upper()`, `python3 -I`):

| Case | Input string | Expected (AEAT) | Computed here |
|---|---|---|---|
| 1. First record, alta | `IDEmisorFactura=89890001K&NumSerieFactura=12345678/G33&FechaExpedicionFactura=01-01-2024&TipoFactura=F1&CuotaTotal=12.35&ImporteTotal=123.45&Huella=&FechaHoraHusoGenRegistro=2024-01-01T19:20:30+01:00` | `3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60` | **match** |
| 2. Second record, alta | `IDEmisorFactura=89890001K&NumSerieFactura=12345679/G34&FechaExpedicionFactura=01-01-2024&TipoFactura=F1&CuotaTotal=12.35&ImporteTotal=123.45&Huella=3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60&FechaHoraHusoGenRegistro=2024-01-01T19:20:35+01:00` | `F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97` | **match** |
| 3. Third record, anulación | `IDEmisorFacturaAnulada=89890001K&NumSerieFacturaAnulada=12345679/G34&FechaExpedicionFacturaAnulada=01-01-2024&Huella=F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97&FechaHoraHusoGenRegistro=2024-01-01T19:20:40+01:00` | `177547C0D57AC74748561D054A9CEC14B4C4EA23D1BEFD6F2E69E3A388F90C68` | **match** |

All three match, and the chain 1 → 2 → 3 also reproduces when each step uses the hash computed in the previous step. (A third-party library note that a document "last character should be 1 but shows 4" refers to an older revision; v0.1.2 had its example "recodificado" on 27/08/2024 and is internally consistent.) [SECONDARY for the remark; PRIMARY for the match]

**Own vector** (not official; for regression tests): rectificativa after case 3 — `IDEmisorFactura=89890001K&NumSerieFactura=R-2026/0001&FechaExpedicionFactura=21-10-2026&TipoFactura=R1&CuotaTotal=-21.00&ImporteTotal=-121.00&Huella=177547C0D57AC74748561D054A9CEC14B4C4EA23D1BEFD6F2E69E3A388F90C68&FechaHoraHusoGenRegistro=2026-10-21T10:15:00+02:00` → `9FFE973502036B60B4FE84129656752E9334AF9E28642A343DC036C9F46D7E04`.

No official golden example exists for the **event** hash [UNCERTAIN; needed only for "no VERI*FACTU"].

### 6.8 Chain checks before each new record

| Requirement | Source | What the software must do |
|---|---|---|
| Before generating a new record (except the first): (1.º) the last record is correctly chained; (2.º) the last record's timestamp is not more than one minute **later** than the time used for the new record | Orden art. 7.i) [PRIMARY] | Check record n−1's `RegistroAnterior/Huella` equals record n−2's `Huella`, then link n to n−1 (FAQ §15.1) [PRIMARY]; generate records strictly in issue order |
| If the check fails ("no VERI*FACTU"): store an event and show the error, but "la facturación por este motivo NUNCA debe interrumpirse" | FAQ §15.2 [PRIMARY] | Warn + continue |
| One chain per taxpayer per system; it contains both altas and anulaciones (events have their own chain) | Orden art. 7.c), 7.d), 9.3 [PRIMARY] | `chain_id = (company, NumeroInstalacion)` |
| Drafts, proformas, *albaranes* are allowed and are not invoices until validated, but their records must also be kept unaltered (LGT 29.2.j) | FAQ §11 [PRIMARY] | Drafts immutable-with-history; invoice number assigned only at validation |
| Test/training invoices issued by a production SIF are real invoices: they must be cancelled (alta → anulación sent to the AEAT); suggested special series such as "PRU 25 XXXX"; numbers can never be reused | FAQ §6, §11 [PRIMARY] | "Training mode" = special series + automatic anulación; no number reuse ever |
| Imported invoices from another system | FAQ §10 [PRIMARY, not read in detail] [UNCERTAIN] | Treat migrated history as not re-registered |

### 6.9 QR code and legend

Source: Orden HAC/1177/2024 arts. 20–21 [PRIMARY]; AEAT *Detalle de las especificaciones técnicas del código «QR» de la factura y de la «URL» del servicio de cotejo o remisión de información por parte del receptor de la factura*, **v0.5.0, 10/12/2025** [PRIMARY].

| Requirement | Detail |
|---|---|
| Size and standard | between **30×30 and 40×40 mm**, ISO/IEC 18004 (:2015 in the AEAT doc), error correction **level M** |
| Placement | at the start of the invoice, before the content; first QR on the invoice; once, on page 1; portrait: top, preferably centred (else top-left); landscape: left, near the top-left; ≥ 2 mm quiet zone (6 mm recommended); high contrast |
| Text above | «QR tributario:» always, preferably centred |
| Text below (VERI*FACTU only) | «Factura verificable en la sede electrónica de la AEAT» or «VERI*FACTU», multiple lines allowed; font size ≥ the rest of the invoice data |
| Electronic (structured) invoice | include the QR **URL** as a separate field; the image is not needed (Orden art. 20.2) |
| URL, VERI*FACTU | test `https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQR?nif=…&numserie=…&fecha=DD-MM-AAAA&importe=…`; production `https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR?…` |
| URL, no VERI*FACTU | same hosts, path `ValidarQRNoVerifactu` |
| Parameters (exactly these 4 in the QR) | `nif` (9, issuer NIF); `numserie` (series + number, ≤ 60, ASCII 32–126); `fecha` (issue date `DD-MM-AAAA`); `importe` (ImporteTotal, "." decimal separator, ≤ 12 integer + 2 decimals) |
| Encoding | parameter values URL-encoded ("URL encoding") with UTF-8; the Java sample uses `java.net.URLEncoder.encode(param, "UTF-8")` (so space → `+`, `/` → `%2F`, `&` → `%26`; `A–Z a–z 0–9 . - * _` unchanged) |
| Optional service params (never in the printed QR for `formato`) | `idioma` ∈ gl, ca, eu, es, va, en (default es); `formato=json` for machine responses |

Examples (verbatim from the AEAT document):
- Encoding example (test): parameters nif 89890001K, numserie `12345678&G33`, fecha 01-01-2024, importe 241.4 → `https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQR?nif=89890001K&numserie=12345678%26G33&fecha=01-01-2024&importe=241.4` (reproduced here with a Java-URLEncoder emulation: **match**). The unencoded `…numserie=12345678&G33…` is given as the wrong form.
- §8.3 production, VERI*FACTU: `https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR?nif=89890001K&numserie=12345678-G33&fecha=01-09-2024&importe=241.4`
- §8.4 production, no VERI*FACTU: `https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQRNoVerifactu?nif=89890001K&numserie=12345678-G33&fecha=01-09-2024&importe=241.4`
- Computed here for the hash case 1 invoice (test host): `https://prewww2.aeat.es/wlpl/TIKE-CONT/ValidarQR?nif=89890001K&numserie=12345678%2FG33&fecha=01-01-2024&importe=123.45`

JSON *cotejo* response example (§9.1.1.2): `{"status":"OK","mensaje":"Encontrada", …, "respuesta":{"resultado":"00","nif":"89890001K","numserie":"12345678-G33","fecha":"01-09-2024","importe":"241.40"}}` [PRIMARY]. Error codes include 3001 (technical error) and 3002 (daily attempts exceeded) [PRIMARY].

### 6.10 Web service (future work)

| Item | Detail | Source |
|---|---|---|
| Message | `RegFactuSistemaFacturacion` { `Cabecera` (ObligadoEmision {NombreRazon, NIF}, Representante?, RemisionVoluntaria? {FechaFinVeriFactu?, Incidencia?}, RemisionRequerimiento? {RefRequerimiento, FinRequerimiento?}) · `RegistroFactura`[1..1000] (RegistroAlta \| RegistroAnulacion) } | `SuministroLR.xsd`, `SuministroInformacion.xsd` [PRIMARY] |
| Namespaces | `https://www2.agenciatributaria.gob.es/static_files/common/internet/dep/aplicaciones/es/aeat/tike/cont/ws/SuministroLR.xsd` and `…/SuministroInformacion.xsd` | XSD [PRIMARY] |
| Schemas/WSDL | `https://www2.agenciatributaria.gob.es/static_files/common/internet/dep/aplicaciones/es/aeat/tikeV1.0/cont/ws/SistemaFacturacion.wsdl` (prewww2 for tests); also ConsultaLR, RespuestaSuministro, RespuestaConsultaLR, EventosSIF, RespuestaValRegistNoVeriFactu | AEAT developer pages; SWeb doc v1.0.3 [PRIMARY] |
| SOAP endpoints (VERI*FACTU) | production `https://www1.agenciatributaria.gob.es/wlpl/TIKE-CONT/ws/SistemaFacturacion/VerifactuSOAP`; production with *certificado de sello* `https://www10.agenciatributaria.gob.es/…/VerifactuSOAP`; tests `https://prewww1.aeat.es/…/VerifactuSOAP` and `https://prewww10.aeat.es/…/VerifactuSOAP` | WSDL ports `SistemaVerifactu`, `…Sello`, `…Pruebas`, `…SelloPruebas` [PRIMARY] |
| SOAP endpoints (requerimiento, no VERI*FACTU) | same hosts, path `…/SistemaFacturacion/RequerimientoSOAP` | WSDL [PRIMARY] |
| Authentication | "Las aplicaciones que envían información a los servicios web deberán autenticarse con certificado electrónico cualificado reconocido" (client TLS) | SWeb doc v1.0.3 [PRIMARY] |
| Response | CSV (*código seguro de verificación*) when at least one record is accepted; per-record errors; states Aceptado / AceptadoConErrores / rejected | Orden art. 16.5 [PRIMARY]; SWeb doc [PRIMARY, not detailed] |
| Test environment | *Portal de Pruebas Externas* (preportal.aeat.es) | developer page [PRIMARY] |
| Record design spreadsheet `DsRegistroVeriFactu.xlsx` (v1.0), validations v1.2.2 (08/04/2026), FAQ v1.3 (04/12/2025), signature spec (XAdES) page, non-VERI*FACTU validation service v0.2 | AEAT developer page [PRIMARY, listing] | Re-check versions before release |

### 6.11 Implementation checklist (VERI*FACTU mode)

1. Company setup: NIF (validated), name, `rrsif_class`, SII flag (disables RRSIF), foral flag (disables RRSIF), mode = VERI*FACTU, `NumeroInstalacion` generated once.
2. On "Issue invoice": assign the next number in the series → build `RegistroAlta` (all amounts in EUR, 2 decimals, `FechaHoraHusoGenRegistro` = now with offset) → chain check (§6.8) → hash (§6.7) → persist append-only → render PDF with QR + legend → queue for sending.
3. Sender: batches ≤ 1,000, waits `t` (60 s initially), honours `t` from responses, retries ≥ hourly with `Incidencia`, shows the pending count.
4. Cancel: `RegistroAnulacion` (never delete). Correct: rectificativa (R1–R5, own series) or `Subsanacion`.
5. Chain viewer, single-record and whole-chain verification, export of records by period, declaración responsable screen.

---

## 7. Foral territories — pointers only

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| RRSIF applies in País Vasco and Navarra only to taxpayers with *domicilio fiscal* in territorio común | RD 1007/2023 art. 1.3 [PRIMARY] | — | Foral companies: disable VERI*FACTU |
| **TicketBAI** (Álava, Bizkaia, Gipuzkoa): each invoice carries a TBAI code and QR; Álava and Gipuzkoa send each invoice file in real time; in Bizkaia it is part of **Batuz** (records go in the LROE books) with a phased calendar 2024–2026 | Software-vendor sources (Cegid, fiskaly, guiafiscal) [SECONDARY]; dates per territory [UNCERTAIN] | 2022–2026 | Out of scope; separate future module |
| **Navarra**: own system ("NaTicket") announced 3 Sep 2025 in the 2025–2027 anti-fraud plan; regulation not published as of March 2026 per a vendor | fiskaly, Cegid [SECONDARY]; status Oct 2026 [UNCERTAIN] | — | Watch item |
| Mapping of the VERI*FACTU L13 *causa de no sujeción* to TicketBAI | FAQ §21 [PRIMARY, not read in detail] | — | — |

---

## 8. B2B e-invoicing (Ley 18/2022 "Crea y Crece") — pointer for `khata-en16931`

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| RD 238/2026 (25 Mar 2026, BOE 31 Mar 2026, in force after 20 days) develops mandatory e-invoicing between businesses and amends RD 1619/2012 | BOE-A-2026-7295 [PRIMARY] | 20 Apr 2026 | — |
| Accepted syntaxes: **CII**, **UBL** (with B2B adaptations), **EDIFACT** invoice message, **Facturae**; UBL is the reference syntax of the public solution (EN 16931). Private platforms must send a faithful **UBL copy** of each invoice to the public solution simultaneously with issue | RD 238/2026 arts. 6, 11 and preamble [PRIMARY] | — | `khata-en16931` UBL output; Facturae optional |
| Application deferred: **12 months** after the ministerial order for businesses with volume of operations > **€8 million** in the prior year; **24 months** for the rest | RD 238/2026 DF 4.ª [PRIMARY] | — | — |
| **Orden HAC/1028/2026** (2 Oct 2026, BOE 5 Oct 2026) regulates the public solution and "entrará en vigor el día siguiente al de su publicación … dándose inicio al cómputo de los plazos" | BOE-A-2026-20587 [PRIMARY] | in force 6 Oct 2026 → obligation ≈ 6 Oct 2027 (> €8M) / ≈ 6 Oct 2028 (others) [exact day count UNCERTAIN] | Plan UBL B2B output before Oct 2027 |
| Transitional: > €8M issuers add a PDF copy for 12 months; invoice-status reporting phased for small individuals | RD 238/2026 DT 2.ª, 3.ª [PRIMARY, summarised] | — | — |
| A structured e-invoice issued by a SIF carries the QR **URL** as a field instead of the image | Orden HAC/1177/2024 art. 20.2 [PRIMARY] | — | Map to an EN 16931 field [UNCERTAIN which BT] |

---

## 9. Financial year and currency

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| IS: "El período impositivo coincidirá con el ejercicio económico de la entidad" and "no excederá de 12 meses"; it also ends on extinction, change of residence abroad, or certain transformations | Ley 27/2014 (LIS) art. 27 [PRIMARY] | — | Default calendar year; allow another 12-month year for companies; short first/last year |
| IRPF: calendar year for individuals [SECONDARY; LIRPF art. 12 not read] | — | — | Autónomos: calendar year only |
| VAT periods are calendar quarters/months regardless of the financial year | RIVA art. 71.3 [PRIMARY] | — | VAT periods independent of the financial year |
| Currency **EUR**; invoices may be in any currency but the VAT amount in euros (exchange rate per LIVA art. 79.Once); RRSIF amounts always in euros | RD 1619/2012 art. 12 [PRIMARY]; RD 1007/2023 art. 10.2 [PRIMARY] | — | Base currency EUR, 2 decimals; store the FX rate used |

---

## 10. Sources

- BOE consolidated texts: Ley 37/1992 del IVA <https://www.boe.es/buscar/act.php?id=BOE-A-1992-28740> (arts. 3, 90, 91, 161; last update 07/10/2026); RIVA, RD 1624/1992 <https://www.boe.es/buscar/act.php?id=BOE-A-1992-28925> (arts. 30, 62.6, 69 bis, 71); RD 1619/2012 Reglamento de facturación <https://www.boe.es/buscar/act.php?id=BOE-A-2012-14696> (arts. 4–7, 11–15, 19; last update 31/03/2026); RD 1007/2023 RRSIF <https://www.boe.es/buscar/act.php?id=BOE-A-2023-24840>; Orden HAC/1177/2024 <https://www.boe.es/buscar/act.php?id=BOE-A-2024-22138>; RDL 15/2025 <https://www.boe.es/buscar/act.php?id=BOE-A-2025-24446> (ELI <https://www.boe.es/eli/es/rdl/2025/12/02/15>); RD 254/2025 (BOE-A-2025-6600, via the RD 1007/2023 notes); RD 1065/2007 <https://www.boe.es/buscar/act.php?id=BOE-A-2007-15984> (arts. 18–22); Orden EHA/451/2008 <https://www.boe.es/eli/es/o/2008/02/20/eha451>; Ley 27/2014 LIS <https://www.boe.es/buscar/act.php?id=BOE-A-2014-12328> (art. 27); RDL 29/2026 <https://www.boe.es/buscar/act.php?id=BOE-A-2026-20823>; RD 238/2026 <https://www.boe.es/buscar/act.php?id=BOE-A-2026-7295>; Orden HAC/1028/2026 <https://www.boe.es/eli/es/o/2026/10/02/hac1028>
- AEAT VERI*FACTU developer page: <https://www.agenciatributaria.es/AEAT.desarrolladores/Desarrolladores/_menu_/Documentacion/Sistemas_Informaticos_de_Facturacion_y_Sistemas_VERI_FACTU/Sistemas_Informaticos_de_Facturacion_y_Sistemas_VERI_FACTU.html>
  - Hash spec v0.1.2: <https://www.agenciatributaria.es/static_files/AEAT_Desarrolladores/EEDD/IVA/VERI-FACTU/Veri-Factu_especificaciones_huella_hash_registros.pdf>
  - QR spec v0.5.0: <https://www.agenciatributaria.es/static_files/AEAT_Desarrolladores/EEDD/IVA/VERI-FACTU/DetalleEspecificacTecnCodigoQRfactura.pdf>
  - Web service description v1.0.3: <https://www.agenciatributaria.es/static_files/AEAT_Desarrolladores/EEDD/IVA/VERI-FACTU/Veri-Factu_Descripcion_SWeb.pdf>
  - FAQ for developers v1.3 (04/12/2025): <https://www.agenciatributaria.es/static_files/AEAT_Desarrolladores/EEDD/IVA/VERI-FACTU/FAQs-Desarrolladores.pdf>
  - Validations and errors v1.2.2 (08/04/2026): <https://www.agenciatributaria.es/static_files/AEAT_Desarrolladores/EEDD/IVA/VERI-FACTU/Validaciones_Errores_Veri-Factu.pdf>
  - Record design: <https://www.agenciatributaria.es/static_files/AEAT_Desarrolladores/EEDD/IVA/VERI-FACTU/DsRegistroVeriFactu.xlsx>
  - XSD/WSDL: <https://prewww2.aeat.es/static_files/common/internet/dep/aplicaciones/es/aeat/tikeV1.0/cont/ws/SuministroInformacion.xsd>, `…/SuministroLR.xsd`, `…/SistemaFacturacion.wsdl`
- AEAT modelo 303: procedure page <https://sede.agenciatributaria.gob.es/Sede/procedimientoini/G414.shtml>; Instrucciones 2026 <https://sede.agenciatributaria.gob.es/Sede/todas-gestiones/impuestos-tasas/iva/modelo-303-iva-autoliquidacion_/instrucciones-2026.html> and <https://sede.agenciatributaria.gob.es/Sede/todas-gestiones/impuestos-tasas/iva/modelo-303-iva-autoliquidacion_/instrucciones-2026/instrucciones-02-12-2t-4t-2026.html>; Instrucciones 2025 `…/instrucciones-2025.html`; Instrucciones 2024 `…/instrucciones.html`; *diseño de registro* DR303e26v101 <https://sede.agenciatributaria.gob.es/static_files/Sede/Disenyo_registro/DR_300_399/archivos_26/DR303e26v101.xlsx> (index <https://sede.agenciatributaria.gob.es/Sede/ayuda/disenos-registro/modelos-300-399.html>)
- Ministerio del Interior, *Cálculo del dígito de control del NIF/NIE*: <https://www.interior.gob.es/opencms/es/servicios-al-ciudadano/tramites-y-gestiones/dni/calculo-del-digito-de-control-del-nif-nie/> (HTTP 403 to direct fetch; content via search extract)
- Secondary: python-stdnum `stdnum/es/dni.py`, `nie.py`, `cif.py`, `nif.py`, `tests/test_eu_vat.doctest` <https://github.com/arthurdejong/python-stdnum>; Wikipedia es *Código de identificación fiscal* <https://es.wikipedia.org/wiki/C%C3%B3digo_de_identificaci%C3%B3n_fiscal>; Wikipedia *ISO 3166-2:ES* <https://en.wikipedia.org/wiki/ISO_3166-2:ES>; RDL 4/2024 summaries (fiscal-impuestos.com) <https://www.fiscal-impuestos.com/real-decreto-ley-4-2024-medidas-fiscales-iva-alimentos-basicos-retenciones-libertad-amortizacion-vehiculos-electricos-palma-reserva-capitalizacion>; taxdown on casilla 61 <https://taxdown.es/modelos-hacienda/casillas-59-60-61-modelo-303>; Holded on 2026 casilla 112 <https://www.holded.com/es/blog/modelo-303-instrucciones-rellenarlo>; RDL 15/2025 commentary (Legal Today) <https://www.legaltoday.com/actualidad-juridica/noticias-de-derecho/nueva-prorroga-verifactu-no-sera-obligatorio-hasta-2027-para-sociedades-y-otros-contribuyentes-2025-12-04/>; TicketBAI/NaTicket (fiskaly, Cegid) <https://www.fiskaly.com/blog/myths-and-facts-ticketbai>, <https://www.cegid.com/ib/es/blog/naticket-navarra-que-es-como-funciona/>; Orden HAC/1028/2026 commentary (PwC Periscopio) <https://periscopiofiscalylegal.pwc.es/wp-content/uploads/2026/10/Periscopio-pdf.pdf>

---

## 11. Open questions

1. Whether a free, open-source desktop app counts as "producción y comercialización" under RRSIF art. 3.2 (declaración responsable, 29 Jul 2025 rule), and who signs the declaration (§6.1, §6.3).
2. No official text for the entity-NIF control algorithm or the letter-vs-digit rule; Wikipedia (P, Q, R, S, W, N → letter; A, B, E, H → digit) and python-stdnum (either) disagree. Same for the K/L/M check rule (§1.2, §1.3).
3. Meaning of 2026 modelo 303 casillas 153–155 and 165–167 (tipo constant "00000", no rate in the 2026 instructions) and of 16–18; which casillas held the Q4-2024 2% and 7.5% rates (§4.2).
4. The order that introduced casilla 112 (a blog names Orden HAC/27/2026; not verified on BOE) (§4.3).
5. Number normalisation in the hash beyond "one or two decimals" (e.g. `123` vs `123.00`, `+` sign): KhataOS will always write and hash two decimals (§6.7).
6. No official golden vector for the event-record hash; XAdES signature profile not read (only needed for "no VERI*FACTU") (§6.4, §6.7).
7. RDL 29/2026 (short-term lettings to 10% from 1 Dec 2026) awaits convalidation — re-check in November 2026 (§2).
8. Exact start dates of B2B mandatory e-invoicing (12/24 months from 6 Oct 2026) and which EN 16931 field carries the VERI*FACTU QR URL (§8).
9. Exact effective dates of the 4% (pre-2012) and 0% donation rate, and the energy-crisis rate scope (only date windows were confirmed) (§2).
10. TicketBAI/Batuz calendars per territory and the status of Navarra's NaTicket as of Oct 2026 (§7).
11. Rounding convention for VAT (per line vs per invoice) and its fit with the ±€10 VERI*FACTU tolerance and modelo 303 totals (§4.3, §6.5).

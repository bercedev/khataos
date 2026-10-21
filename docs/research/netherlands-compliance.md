# Netherlands Compliance Requirements (Omzetbelasting / btw, KOR, e-factuur): KhataOS Spec

> **Status as of:** 2026-10-21. **Scope:** Dutch businesses (eenmanszaak, vof/maatschap, bv/nv, stichting/vereniging) in the European Netherlands keeping books in KhataOS, a local-first desktop accounting app. The `khata-nl` pack implements the btw and identity items below (see `docs/netherlands-vat.md`). Structured invoices (Peppol BIS Billing 3.0 / NLCIUS) belong to the shared EU crate `khata-en16931` (see `docs/en16931.md`). Caribisch Nederland (Bonaire, Sint Eustatius, Saba) is out of scope (§9).
> **Not legal advice.** This file turns the Wet op de omzetbelasting 1968 (Wet OB), the Algemene wet inzake rijksbelastingen (AWR), Boek 2 BW, Belastingdienst guidance and the Peppol / Logius specifications into software requirements. Get a Dutch belastingadviseur to confirm each item before release.
>
> **Confidence tags:**
> - **[PRIMARY]** means read in the primary text: wetten.overheid.nl (Wet OB 1968 and AWR as in force on 1 Jan 2026, BW Boek 2), zoek.officielebekendmakingen.nl (Staatscourant), belastingdienst.nl (pages and the *Toelichting bij de btw-aangifte* 2024/2025/2026 PDFs), rijksoverheid.nl / tweedekamer.nl (Kamerbrieven), logius.nl, ondernemersplein.overheid.nl, business.gov.nl, the OpenPEPPOL Schematron.
> - **[SECONDARY]** means reported consistently by reputable non-official sources (python-stdnum, Wikipedia, accountancy and software-vendor sites), but the official text was not found.
> - **[UNCERTAIN]** means sources conflict, the item is interpretive, or the point was not found. Flagged for review.
>
> web.archive.org was offline during this research; kleineondernemer.nl (the source python-stdnum cites for the 2020 btw-id algorithm) returned HTTP 403. Dynamic belastingdienst.nl pages were read with WebFetch where curl only got the JavaScript shell.

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **btw-id validation**: `NL` + 9 digits + `B` + 2 digits. Valid if **either** ISO 7064 MOD 97-10 over the whole string (letters A=10 … Z=35, so N=23, L=21, B=11) gives remainder 1 (btw-id issued since 1 Jan 2020, mostly eenmanszaken) **or** the 9 digits pass the elfproef with weights 9…2 and −1 (number built on an RSIN/BSN) | Belastingdienst (format) [PRIMARY]; python-stdnum `nl/btw.py` (algorithms) [SECONDARY]; 29 real numbers checked here (§1) |
| 2 | **omzetbelastingnummer (ob-nummer)**: 9 digits (RSIN or BSN) + `B` + 2-digit subnummer `01`–`99`; used only with the Belastingdienst, never printed as the btw-id | Belastingdienst btw-id page; business.gov.nl [PRIMARY] (§1.2) |
| 3 | **Rates** 21% / 9% / 0% from an effective-dated master: 19% → 21% on 1 Oct 2012; 6% → 9% on 1 Jan 2019; **logies 9% → 21% from 1 Jan 2026**; culture/media/sport **stays 9%** (planned increase reversed) | Art. 9 Wet OB; Tabel I as in force 1 Jan 2026; Staatscourant 2012, 26808 and 2018, 68655; Belastingdienst logies page [PRIMARY] (§2) |
| 4 | **Invoice particulars** art. 35a lid 1 Wet OB (a–q), incl. "een opeenvolgend nummer, met één of meer reeksen", «btw verlegd», exemption note; issue by the **15th of the following month**; vereenvoudigde factuur ≤ €100 | Art. 34c–35d Wet OB [PRIMARY] (§3) |
| 5 | **Aangifte omzetbelasting** rubrieken 1a–1e, 2a, 3a–3c, 4a–4b, 5a, 5b and the total; **all amounts in whole euros, rounding in the taxpayer's favour allowed**; negative amounts with a minus sign | *Toelichting bij de btw-aangifte 2026* [PRIMARY] (§6) |
| 6 | **Periods**: kwartaal by default; maand on request (or imposed); jaar only if btw < €1,883/year and other conditions. **Due = last day of the month after the period** (aangifte and payment); year return by 31 March; no weekend shift | Belastingdienst; AWR art. 19 [PRIMARY] (§6.1) |
| 7 | **KOR**: optional exemption if jaaromzet in Nederland ≤ **€20,000** in the current **and** the previous calendar year; no btw on invoices, no aangifte, no input deduction; 2025 changes (no 3-year minimum, shorter exclusion, EU-KOR €100,000) | Art. 25–25c Wet OB; Staatscourant 2024, 38565 [PRIMARY] (§5) |
| 8 | **Retention 7 years** (AWR art. 52 lid 4; BW 2:10 lid 3); immovable-property records **9 years after the year of first use** (art. 34a Wet OB) | [PRIMARY] (§7) |
| 9 | **E-invoicing**: no B2B mandate today; central-government suppliers must e-invoice (contracts since 1 Jan 2017); cabinet plans **domestic B2B e-invoicing from 1 Jul 2030** and reporting from 1 Jul 2031 (not yet law) | Logius; Kamerbrief "Contourenbrief elektronisch factureren en rapporteren" (2026) [PRIMARY] (§4) |
| 10 | **Fiscal year** = calendar year unless the statutes say otherwise (rechtspersonen); VAT periods are calendar-based regardless; currency EUR, btw amount on the invoice in euro | BW 2:10a; art. 35a lid 4 Wet OB [PRIMARY] (§8) |
| 11 | **Provinces** as address regions, ISO 3166-2:NL (12 codes) | Wikipedia *ISO 3166-2:NL* [SECONDARY] (§1.6) |
| 12 | **Filing** only via Mijn Belastingdienst Zakelijk (manual) or software over SBR/Digipoort — future work | Toelichting 2026; odb.belastingdienst.nl [PRIMARY] (§6.1) |

---

## 1. Identity numbers

### 1.1 btw-identificatienummer (btw-id)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Format: "landcode NL", "9 cijfers", "letter B", "2 cijfers"; example `NL123456789B01` | Belastingdienst *Alles over uw btw-identificatienummer (btw-id) en omzetbelastingnummer (ob-nummer)* [PRIMARY] | — | Accept with/without `NL`, case-insensitive, strip spaces and `.-`; store `NL` + 9 digits + `B` + 2 digits (14 chars) |
| The btw-id is for "contact met klanten en leveranciers, ook in het buitenland. Zet uw btw-id daarom op uw facturen en website" | Toelichting bij de btw-aangifte 2026, p. 3 [PRIMARY] | — | Print the btw-id (not the ob-nummer) on invoices |
| Since **1 Jan 2020** eenmanszaken get a btw-id that no longer contains the BSN: "In uw ob-nummer is uw burgerservicenummer (bsn) verwerkt, in uw btw-id niet". business.gov.nl: "The 9 digits are not related to your citizen service number and the 2 check digits are random"; example `NL000099998B57` | Belastingdienst btw-id page [PRIMARY]; business.gov.nl *VAT number* [PRIMARY]; date from nl.wikipedia *Btw-nummer (Nederland)* and the python-stdnum commit of 9 Jan 2020 [SECONDARY] | 1 Jan 2020 | Two check algorithms (§1.3) |
| Legal entities kept btw-ids built on the RSIN (e.g. `NL806753742B01`); these pass the elfproef, not MOD 97-10 | 29 numbers from python-stdnum `tests/test_eu_vat.doctest`, checked here (§1.4) [SECONDARY] | — | Accept either check |
| The new number "zal dan niet meer voldoen aan de 11-proef, wel aan de 97-modulus proef" | nl.wikipedia *Btw-nummer (Nederland)* [SECONDARY] | 2020 | — |
| Future-format warning quoted by the python-stdnum contributor: "Op termijn kan de structuur met uitzondering van de landaanduiding op positie 1-2 en de 2 cijfers op positie 13-14 wijzigen. Dat wil zeggen dat op de posities 3-12 cijfers, hoofdletters of de tekens '+' en '*' kunnen komen." | python-stdnum PR #185 quoting kleineondernemer.nl, which reportedly quotes a Belastingdienst text [SECONDARY; original not retrieved: UNCERTAIN] | "op termijn" | Keep the MOD 97-10 routine generic over `[0-9A-Z]`; treat `+`/`*` as unsupported until a mapping is published |
| Online validity: format/check pass ≠ registered. Check via EU **VIES** ("Het systeem is hoofdlettergevoelig") | Belastingdienst *Btw-id controleren* [PRIMARY, via search extract] | — | Optional online check (future work) |

### 1.2 Omzetbelastingnummer (ob-nummer), RSIN, BSN, KvK

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| ob-nummer: "9 cijfers, de letter B, en 2 cijfers", example `111234567B01`; for zzp'ers the 9 digits are the BSN. Used "alleen bij contact met ons" (aangifte, ICP-opgaaf, letters, phone) | Belastingdienst btw-id page; Toelichting 2026 p. 3 [PRIMARY] | — | Company setting "ob-nummer" (separate from btw-id); never print it on sales documents |
| "The VAT tax number … consists of either the … (RSIN), or your Citizen Service Number (BSN), followed by a code of 3 characters between B01 and B99" | business.gov.nl *VAT number* [PRIMARY] | — | Suffix `B01`–`B99` (the subnummer; a second business of the same person gets B02 etc. [nl.wikipedia, SECONDARY]) |
| The KOR applies to all subnummers of one ondernemer together | Belastingdienst *Wat betekent meedoen met de KOR* [PRIMARY] | — | KOR flag per company, turnover summed over subnummers |
| **RSIN**: 9 digits, issued automatically by KVK to legal entities and partnerships; "Sole proprietorships do not have an RSIN" (the owner has a BSN) | business.gov.nl *An RSIN number for your business* [PRIMARY] | — | Optional field; validate with the elfproef (§1.3) |
| **KVK-nummer**: 8 digits; **vestigingsnummer**: 12 digits | KVK Developer Portal and kvk.nl *Vestigingsnummer* [PRIMARY, via search extract] | — | 8-digit field; no public check digit known (a NuGet library README states KVK publishes none) [SECONDARY] — format check only |
| Belastingdienst lists "als u bij de KVK bent ingeschreven: uw KVK-nummer" among invoice items | Belastingdienst *Factuureisen* [PRIMARY]; not in art. 35a Wet OB (legal basis presumably the Handelsregister rules, not read) [UNCERTAIN] | — | Print the KVK-nummer when the company has one |
| Peppol: Dutch suppliers' legal entity ID "MUST be either a KVK or OIN number (schemeID 0106 or 0190)" (NL-R-003) | OpenPEPPOL `PEPPOL-EN16931-UBL.sch` [PRIMARY] | — | Store KVK-nummer for e-invoices (§4) |

### 1.3 Check algorithms, as implemented

Python-stdnum (`stdnum/nl/btw.py`, release 2.2) accepts a number when `not bsn.is_valid(number[:9])` is false **or** `mod_97_10.is_valid('NL' + number)` is true, after: strip `' -.'`, upper-case, drop a leading `NL`, left-pad the part before the last 3 chars to 9 digits; require 9 digits with value > 0, char 10 = `B`, last 2 digits with value > 0 (so `00` is rejected), length 12 (without `NL`) [SECONDARY].

**A. MOD 97-10 (btw-id issued since 2020)** over the full 14-char string `NL dddddddd d B cc`:

1. Replace each letter by its base-36 value (A=10 … Z=35): N → `23`, L → `21`, B → `11`; digits stay. `NL000099998B57` → `23210000999981157`.
2. Valid when that integer mod 97 = **1** (ISO 7064 MOD 97-10, the IBAN check).
3. Generating the 2 digits: `cc = 98 − (int(conv("NL" + 9 digits + "B") + "00") mod 97)`, printed with 2 digits. Consequence: a valid MOD 97-10 suffix is always in **02–98**.

Source: python-stdnum `iso7064/mod_97_10.py` (`_to_base10` uses `int(x, 36)`, `checksum() == 1`, `calc_check_digits = '%02d' % (98 - checksum(number + '00'))`) and `nl/btw.py` [SECONDARY]. No Belastingdienst technical note to software developers stating the algorithm was found [UNCERTAIN whether one is public]. The algorithm matches every 2020-style example (Belastingdienst/business.gov.nl `NL000099998B57`, and four real numbers from python-stdnum, §1.4).

**B. Elfproef (omzetbelastingnummer / RSIN / BSN based btw-id)** over the 9 digits `d1 … d9` (left-pad shorter numbers with zeros):

1. `S = 9·d1 + 8·d2 + 7·d3 + 6·d4 + 5·d5 + 4·d6 + 3·d7 + 2·d8 − 1·d9`
2. Valid when `S mod 11 = 0` (and the number is not all zeros).
3. The suffix after `B` is not checked by this rule (`B01`…`B99`).

Source: python-stdnum `nl/bsn.py` (`(sum((9 - i) * int(n) for i, n in enumerate(number[:-1])) - int(number[-1])) % 11`) [SECONDARY]; nl.wikipedia *Elfproef*: "Bij burgerservicenummers wordt het laatste getal met −1 vermenigvuldigd in plaats van met 1" [SECONDARY]. **For the RSIN** no official statement of the weights was found [UNCERTAIN], but the empirical evidence is strong: all 29 RSIN/BSN-based btw-ids in the python-stdnum test list pass the **−1** variant, and 27 of them **fail** the plain 9…1 variant (the 2 that pass both end in 0, where −0 = +0). Use weights 9…2, −1 for RSIN, BSN and ob-nummer alike.

Worked example `004495445`: 0·9 + 0·8 + 4·7 + 4·6 + 9·5 + 5·4 + 4·3 + 4·2 = 137; 137 − 5 = 132 = 12 × 11 → valid.

### 1.4 Test vectors

Computed here with python-stdnum 2.2 (source tarball, run with `python3 -I`) and with an independent re-implementation of A and B; both agree.

| Number | Origin | Elfproef (9 digits) | MOD 97-10 remainder | Expected |
|---|---|---|---|---|
| `NL004495445B01` | python-stdnum docstring | pass | 40 | **valid** (elfproef) |
| `NL4495445B01` | python-stdnum docstring (short form, pads to `004495445`) | pass | 40 | **valid** |
| `NL 0044.95.445 b01` | separators, lower case | pass | 40 | **valid** after normalising |
| `NL002455799B11` | python-stdnum docstring ("valid since 2020-01-01") | fail | 1 | **valid** (MOD 97-10) |
| `NL000099998B57` | business.gov.nl example; python-stdnum issue #182 | fail | 1 | **valid** (MOD 97-10) |
| `NL001162938B28`, `NL001452330B47`, `NL001617419B92` | python-stdnum PR #185 (real 2020 numbers) | fail | 1 | **valid** (MOD 97-10) |
| `NL806753742B01`, `NL818643778B01` | python-stdnum test list (RSIN based) | pass | 24 / 45 | **valid** (elfproef) |
| `NL004495445B02` | suffix changed | pass | 41 | **valid** (elfproef ignores the suffix) |
| `NL123456789B13` | `calc_check_digits('NL123456789B')` = 13 | fail | 1 | **valid** (MOD 97-10) |
| `NL123456789B01` | Belastingdienst illustrative format example | fail (9…2,−1) | 86 | **invalid** — do not use as a positive test |
| `111234567B01` | Belastingdienst illustrative ob-nummer | fail | 28 | **invalid** — illustration only |
| `NL123456789B90` | python-stdnum docstring (negative) | fail | 78 | **invalid** (InvalidChecksum) |
| `NL002455799B12` | last digit changed | fail | 2 | **invalid** |
| `NL004495446B01` | 9th digit changed | fail | 49 | **invalid** |
| `NL000099998B01` | wrong suffix for a 2020 number | fail | 42 | **invalid** |
| `NL000000000B00` | all zeros | (zero) | 25 | **invalid** (format) |

BSN/RSIN elfproef alone: `111222333` valid, `123456782` valid, `111234567` invalid, `806753742` valid.

### 1.5 Which number goes where

| Number | On sales invoice | In aangifte / ICP | Peppol |
|---|---|---|---|
| btw-id (`NL…B..`) | yes (art. 35a lid 1 c) | — | seller VAT ID (BT-31) |
| ob-nummer | no | yes (identifies the filer) | — |
| KVK-nummer | yes if registered [PRIMARY Belastingdienst; basis UNCERTAIN] | — | legal entity ID, scheme 0106 |
| RSIN / BSN | no (BSN is privacy-sensitive) | — | — |

### 1.6 Provinces (ISO 3166-2:NL)

NL-DR Drenthe, NL-FL Flevoland, NL-FR Fryslân, NL-GE Gelderland, NL-GR Groningen, NL-LI Limburg, NL-NB Noord-Brabant, NL-NH Noord-Holland, NL-OV Overijssel, NL-UT Utrecht, NL-ZE Zeeland, NL-ZH Zuid-Holland. ISO 3166-2:NL also lists the countries NL-AW Aruba, NL-CW Curaçao, NL-SX Sint Maarten and the special municipalities NL-BQ1 Bonaire, NL-BQ2 Saba, NL-BQ3 Sint Eustatius (ISO 3166-1 `BQ`; change of 2011-12-13). [SECONDARY, Wikipedia *ISO 3166-2:NL*; the ISO OBP was not queried.] Provinces play no role in btw; they are address data only. The BES codes are outside the Wet OB (§9).

---

## 2. Rates (art. 9 Wet OB, Tabel I and II)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Algemeen tarief**: "De belasting bedraagt 21 percent." | Art. 9 lid 1 Wet OB (version 1 Jan 2026) [PRIMARY] | 21% since **1 Oct 2012**: "de verhoging van het algemene btw-tarief van 19% naar 21% (artikel 9, lid 1 …) per 1 oktober 2012" (Staatscourant 2012, 26808) [PRIMARY] | `STANDARD` 21%; 19% for supplies before 1 Oct 2012 |
| **Verlaagd tarief**: "9 percent voor leveringen van goederen en diensten, genoemd in de bij deze wet behorende tabel I" | Art. 9 lid 2 a Wet OB [PRIMARY] | 9% since **1 Jan 2019**: "de verhoging van het verlaagde btw-tarief van 6% naar 9% met ingang van 1 januari 2019" (Staatscourant 2018, 68655) [PRIMARY] | `REDUCED` 9%; 6% before 1 Jan 2019 |
| **Nultarief**: "nihil voor leveringen van goederen en diensten, genoemd in de bij deze wet behorende tabel II, mits is voldaan aan bij algemene maatregel van bestuur vast te stellen voorwaarden" (exports, intra-EU supplies, solar panels on/near dwellings: Tabel II b post 7, etc.) | Art. 9 lid 2 b Wet OB; Tabel II [PRIMARY] | — | `ZERO` 0% (taxable, keeps input deduction) — distinct from `EXEMPT` (art. 11) |
| **Logies 9% → 21%**: "Voor het verstrekken van logies in hotels, pensions en vakantiebestedingsbedrijven in een korte periode geldt vanaf 1 januari 2026 het tarief van 21%." Also applies to prepayments made in 2025 for stays in 2026+ and to single-purpose vouchers; ancillary gas/electricity/water, sanitary facilities, laundry, parking with logies also 21%; separately offered facilities such as breakfast stay 9% (all-in prices split by market value). Tabel I b post 11 is "[Red: vervallen;]" in the 2026 text; **camping** (post b 10, "het geven van gelegenheid tot kamperen … voor een korte periode") stays in Tabel I | Belastingdienst *Btw-tarief logies* and news item; Tabel I as in force 1 Jan 2026 [PRIMARY]; Rijksoverheid 17 Dec 2025 (Belastingplan 2026 package; also short-stay logies ≤ 6 months to workers, students, asylum seekers) [PRIMARY] | 1 Jan 2026 | Item class "logies" at 21% from 2026-01-01 (9% before); camping stays 9%; tax point for prepayments follows the stay date per the Belastingdienst text |
| **Culture, media, sport stay 9%**: the Belastingplan 2025 increase to 21% per 1 Jan 2026 was reversed by the *Wet behoud verlaagd btw-tarief cultuur, media en sport* (bill 16 Sep 2025); Rijksoverheid: "Om het terugdraaien van de btw-verhoging op cultuur, media en sport te betalen, worden de schijven van de inkomstenbelasting … niet volledig aangepast"; Tabel I b posts 3 (sport), 14 (circus, zoo, museums, concerts, theatre, cinema, sport events, attraction parks), 17 (performing artists), a 30 / b 21 (books, e-publications) are present in the 2026 text | Rijksoverheid news 17 Dec 2025; bill PDF on rijksoverheid.nl; Tabel I 2026 [PRIMARY] | 1 Jan 2026 (no change) | No rate change for these classes |
| Sportkantine forfait: "forfaitair btw-tarief van 13% … over uw totale kantineontvangsten inclusief btw" goes in rubriek 1c | Toelichting 2026, 1c [PRIMARY] | — | Not modelled; allow manual 1c entry |
| **Aanvullende margeregeling** abolished from 1 Jan 2025 | Toelichting 2025, 1a/1b note [PRIMARY] | 1 Jan 2025 | Margin schemes not modelled |
| Exempt supplies (art. 11: medical, education, finance, letting of immovable property, etc.) are **not reported** in the aangifte: "Leveringen van goederen en diensten die onder een vrijstelling vallen, hoeft u niet aan te geven" | Toelichting 2026, 1e note [PRIMARY] | — | `EXEMPT` lines are excluded from every rubriek |

---

## 3. Invoices (art. 34c–35d Wet OB)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Obligation**: an invoice for supplies to "een andere ondernemer of een rechtspersoon, andere dan ondernemer", for distance sales (art. 5a), new means of transport (Tabel II a 6) and for advance payments | Art. 34c lid 1 [PRIMARY] | — | B2C invoices optional (retail); B2B mandatory |
| **Timing**: "uiterlijk op de vijftiende dag van de maand volgende op die waarin de goederenlevering of de dienst is verricht"; for prepayments before they fall due | Art. 34g [PRIMARY] | — | Warn on uninvoiced deliveries past the 15th of the next month (future work) |
| **Mandatory particulars** (art. 35a lid 1): a) issue date; b) "een opeenvolgend nummer, met één of meer reeksen, waardoor de factuur eenduidig wordt geïdentificeerd"; c) the supplier's btw-identificatienummer; d) the customer's btw-id when the customer owes the tax (reverse charge) or for a Tabel II a 6 supply; e) "de volledige naam en het volledige adres van de ondernemer en zijn afnemer"; f) quantity and nature of goods / extent and nature of services; g) the supply date (or prepayment date) if it differs from the issue date; h) "de vergoeding voor elk tarief of elke vrijstelling, de eenheidsprijs, belasting niet inbegrepen", plus discounts not in the unit price; i) the rate applied; j) the btw amount payable; k) «factuur uitgereikt door afnemer» for self-billing; l) "in geval van een vrijstelling … enige aanduiding daarvan"; m) "wanneer de afnemer tot voldoening van de belasting is gehouden, de vermelding «btw verlegd»"; n) new-vehicle data; o) «bijzondere regeling reisbureaus»; p) «bijzondere regeling – gebruikte goederen» / «– kunstvoorwerpen» / «– voorwerpen voor verzamelingen of antiquiteiten»; q) fiscal representative's btw-id, name and address | Art. 35a lid 1 a–q Wet OB [PRIMARY; b, e, h, l, m quoted, rest paraphrased] | — | Invoice model carries every field; numbering per series unique and gap-free by construction |
| **Numbering**: Belastingdienst: "Gebruik opeenvolgende nummers voor uw facturen, met 1 of meer reeksen. Elk factuurnummer mag maar 1 keer voorkomen." Year-prefixed series "2023001, 2023002, etc. Dat is een prima opvolgende reeks." | Belastingdienst *Factuureisen*; Stichting en Vereniging Loket *Factuureisen* [PRIMARY] | — | Allow several series; never reuse a number; gap-free not stated in the law ("opeenvolgend") — KhataOS enforces no gaps anyway [whether a gap is a breach: UNCERTAIN] |
| Reverse charge (verleggingsregeling): "rekent u geen btw, maar vermeldt u op de factuur: 'btw verlegd' en het btw-identificatienummer van die afnemer. De vergoeding vermeldt u per btw-tarief." | Belastingdienst *U maakt gebruik van de verleggingsregeling* [PRIMARY] | — | Print exactly `btw verlegd`, customer btw-id required, 0 btw |
| **Vereenvoudigde factuur** allowed when the amount is ≤ **€100**, for a correcting document (art. 34f), or when the KOR applies (art. 25a lid 1). Contents: issue date, supplier identity, nature of goods/services, btw amount or data to compute it, and for a correction a reference to the original invoice. Not allowed for intra-EU supplies (art. 34c lid 1 b/c) or where a non-established supplier's customer owes the tax | Art. 34d, 35a lid 2 [PRIMARY]; Belastingdienst: "het factuurbedrag is maximaal € 100 (inclusief btw)" [PRIMARY] | — | Simplified layout when gross ≤ €100 and not intra-EU / reverse charge |
| **Credit notes**: "Ieder document of bericht dat wijzigingen aanbrengt in, en specifiek en ondubbelzinnig verwijst naar de oorspronkelijke factuur, geldt als factuur" | Art. 34f [PRIMARY] | — | Credit note references the original invoice number (Peppol NL-R-001 makes this fatal in e-invoices) |
| Periodic invoice for several supplies, period ≤ one calendar month; self-billing needs prior agreement and an acceptance procedure | Art. 35 [PRIMARY] | — | Not modelled |
| Currency: amounts in any currency "mits het te betalen of te herziene bedrag van de belasting is uitgedrukt in euro's" | Art. 35a lid 4 [PRIMARY] | — | Foreign-currency invoices still show the btw in EUR |
| **KOR invoice**: "U bent niet verplicht om een factuur uit te reiken. Als u dit toch doet, vermeldt u op de factuur dat u gebruik maakt van de kleineondernemersregeling. U zet geen btw op de factuur." The law: the KOR user "mag op de factuur op geen enkele wijze melding maken van omzetbelasting" | Belastingdienst *U maakt gebruik van de kleineondernemersregeling*; art. 25a lid 4 Wet OB [PRIMARY] | 2020 | Print a note such as "Vrijgesteld van btw op grond van de kleineondernemersregeling (KOR)" [exact wording not prescribed: UNCERTAIN]; no btw lines, no rate, no "0% btw" |
| Electronic invoices: "Elektronische facturering wordt toegepast behoudens aanvaarding door de afnemer"; authenticity, integrity and legibility guaranteed until the end of the retention period, by any business control giving "een betrouwbaar controlespoor" | Art. 35b lid 1–2 [PRIMARY] | — | PDF/e-invoice by e-mail only with customer consent (setting per party); immutable issued invoices |
| Retain copies of issued and all received invoices; for electronic storage, also the data guaranteeing authenticity/integrity | Art. 35c [PRIMARY] | — | Store issued PDFs/XML and received files unaltered (hash) |

---

## 4. E-invoicing (e-factuur)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **No B2B mandate today**: e-invoicing needs the customer's acceptance (art. 35b lid 1). "In 2026 is een e-factuur in Nederland alleen verplicht als u levert aan de rijksoverheid" | Art. 35b Wet OB [PRIMARY]; status sentence from vendor/accountancy summaries [SECONDARY] | — | Offer PDF and (optional) Peppol UBL |
| **Central government (B2G)**: "Bij inkoopovereenkomsten na 1 januari 2017 zijn Rijksoverheden en hun leveranciers verplicht om e-facturatie toe te passen"; all public contracting authorities must be able to receive and process EN 16931 invoices from **18 April 2019**; "Een pdf is geen e-factuur"; send via Peppol, Digipoort or the Leveranciersportaal; buyer addressed by its **OIN** | Logius *E-facturatie in Europees formaat verplicht vanaf 18 april*; Logius *Hoe werkt het*; Ondernemersplein *Een e-factuur versturen naar de overheid* [PRIMARY] | 1 Jan 2017 (Rijk contracts); 18 Apr 2019 (receipt, all authorities) | `khata-en16931` emits Peppol BIS Billing 3.0 UBL; Peppol transport via an access point is future work |
| **NLCIUS** (formerly SI-UBL 2.0): the Dutch CIUS of EN 16931, maintained under NEN; Peppol document type ID `urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##urn:cen.eu:en16931:2017#compliant#urn:fdc:nen.nl:nlcius:v1.0::2.1` (so CustomizationID `urn:cen.eu:en16931:2017#compliant#urn:fdc:nen.nl:nlcius:v1.0`); SI-UBL 1.2 no longer accepted by the Rijk since 1 Jan 2023 | eConnect NLCIUS docs [SECONDARY]; Logius search extract for SI-UBL 1.2 [SECONDARY]; NEN text not read [UNCERTAIN] | — | Default to Peppol BIS 3.0 (accepted by the Rijk); NLCIUS as a later option |
| **Peppol BIS 3.0 Dutch rules** (fatal, "For suppliers in the Netherlands"): NL-R-001 credit note must reference the invoice; NL-R-002 supplier street, city, post code; NL-R-003 supplier legal entity ID is KVK (0106) or OIN (0190); NL-R-004/-005 same for a Dutch customer; NL-R-006 fiscal representative address; NL-R-007 PaymentMeans required when the customer pays; NL-R-008 for Dutch customers PaymentMeansCode ∈ {30, 48, 49, 57, 58, 59}; NL-R-009 order line reference needs a document order reference | OpenPEPPOL `peppol-bis-invoice-3/rules/sch/PEPPOL-EN16931-UBL.sch` [PRIMARY] | — | Validate before export |
| **ViDA** (Directive (EU) 2025/516): from **1 Jul 2030** cross-border intra-EU B2B transactions need EN-standard e-invoices, issued within 10 days, reported per invoice near-real-time | Kamerbrief "Contourenbrief elektronisch factureren en rapporteren" (TK 2026D42856) [PRIMARY]; directive number from vendor summaries [SECONDARY] | 1 Jul 2030 | Plan EN 16931 output for intra-EU B2B by 2030 |
| **Dutch domestic plan**: the cabinet chooses "een e-facturatie- (vanaf 1 juli 2030) en digitale rapportageverplichting (vanaf 1 juli 2031) … voor binnenlandse transacties met andere ondernemers (B2B)"; no separate micro-business threshold beyond the existing small-business threshold; existing invoicing exceptions kept where possible; no non-EU standards for domestic invoices; infrastructure (Peppol advised by EY) still under study until Oct 2026; internet consultation planned autumn 2026, bill before the 2027 summer recess, parliamentary handling before 1 Jul 2028. **Not yet law** | Contourenbrief (TK 2026D42856, letter date 11 Sep 2026 per Accountancy Vanmorgen [date SECONDARY]) [PRIMARY]; earlier Kamerbrief 26 Jun 2025 on ViDA implementation [PRIMARY, title only] | 1 Jul 2030 / 1 Jul 2031 (planned) | Watch item; design the invoice store so per-invoice reporting can be added |

---

## 5. Kleineondernemersregeling (KOR, art. 25–25c Wet OB)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Threshold**: an ondernemer established in the Netherlands "van wie de jaaromzet in Nederland niet meer bedraagt dan € 20.000 kan kiezen voor toepassing van vrijstelling". Belastingdienst: "De maximale omzet van € 20.000 geldt voor het kalenderjaar waarin u zich aanmeldt, én voor het kalenderjaar daarvóór." | Art. 25a lid 1 [PRIMARY]; Belastingdienst *Voorwaarden voor deelname aan de KOR* [PRIMARY] | Scheme since 1 Jan 2020 [SECONDARY; Kamerstuk 35033 not read]; two-year test as worded from 2025 | Turnover monitor (current and prior calendar year) |
| **Jaaromzet in Nederland** counts supplies that would be taxed in NL (21%, 9%, 0% incl. exports and intra-EU supplies, and domestic reverse-charge supplies made), plus exempt supplies of immovable property (art. 11 lid 1 a, b), financial (i, j) and insurance services; not counted: sales of business assets (immovable property, depreciable movables), private use, supplies taxed abroad, intra-EU acquisitions | Art. 25 lid 1–2 [PRIMARY]; Belastingdienst *Voorwaarden* [PRIMARY] | 2025 text | Classify lines for the KOR turnover total |
| **Effects**: exempt; "geen recht op aftrek van belasting en mag op de factuur op geen enkele wijze melding maken van omzetbelasting"; "U doet geen btw-aangifte meer" (except incidental returns, e.g. reverse charge received or intra-EU acquisitions) | Art. 25a lid 4 [PRIMARY]; Belastingdienst *Wat betekent meedoen met de KOR* [PRIMARY] | — | KOR company: no btw on sales, purchases booked gross, no aangifte generated (flag incidental cases) |
| Not exempt under the KOR: new means of transport sent to another member state; immovable property used in the business | Art. 25a lid 3 [PRIMARY] | — | Warn |
| **Start**: after a melding, "vanaf het eerstvolgende belastingtijdvak dat minimaal vier weken na ontvangst van de melding aanvangt"; the melding states last year's and this year's turnover | Art. 25a lid 5 [PRIMARY] | — | Store KOR start date |
| **End by choice**: "de eerste dag van het eerstvolgende kalenderkwartaal dat minimaal vier weken na ontvangst van de wederopzegging aanvangt"; re-entry only after the end of that calendar year **and** the following one | Art. 25a lid 7 [PRIMARY] | 1 Jan 2025 | Store KOR end date; block re-entry window |
| **Crossing €20,000**: no longer eligible; the exemption "niet langer van toepassing [is] voor de levering van het goed of de dienst waardoor die overschrijding tot stand komt, gedurende het resterende kalenderjaar en het daaropvolgende kalenderjaar". Belastingdienst: "Uw afmelding gaat dan direct in." | Art. 25a lid 9 [PRIMARY]; Belastingdienst [PRIMARY] | 2025 | The supply that crosses €20,000 and all later ones are taxed normally |
| **2025 changes**: "De minimale toepassingstermijn van de KOR wordt afgeschaft" (previously three years); exclusion after termination shortened to the end of the following calendar year; **registratiedrempel** raised from €1,800 to **€2,200** (art. 25b Wet OB, art. 25 Uitvoeringsbeschikking OB 1968) | Staatscourant 2024, 38565 (in force 1 Jan 2025) [PRIMARY] | 1 Jan 2025 | Historic rule (3-year minimum) only for KOR periods before 2025 |
| **Registratiedrempel** (art. 25b): turnover ≤ €2,200, not registered with KVK or the Belastingdienst and not obliged to: exemption without a melding, relieved of invoicing/record obligations | Art. 25b; Belastingdienst *Registratiedrempel* [PRIMARY] | 1 Jan 2025 | Out of scope (such users are not KVK-registered businesses) |
| **EU-KOR**: an NL-established ondernemer may apply the exemption in other member states after a "voorafgaande kennisgeving" and identification with a number with suffix «EX»; EU-wide turnover (jaaromzet in de Unie) ≤ **€100,000** (the same cap applies under art. 25a lid 2 to businesses established elsewhere in the EU that use the Dutch KOR); quarterly "opgaaf kwartaalomzet" | Art. 25a lid 2, 25c Wet OB; Toelichting 2026 p. 8; Belastingdienst KOR page ("Sinds 1 januari 2025") [PRIMARY] | 1 Jan 2025 | Not modelled |
| Old KOR (before 2020) was a tax **reduction** for btw ≤ €1,883 shown in rubriek 5d "Vermindering volgens de kleineondernemersregeling" | AFAS help (old form) [SECONDARY] | to 31 Dec 2019 | Historic only; 5d not used (§6.2) |

---

## 6. Aangifte omzetbelasting (btw-aangifte)

### 6.1 Filing rules

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Period**: "Btw-aangifte doet u meestal per kwartaal. Maar er zijn uitzonderingen mogelijk." Monthly on request (letter) or imposed after late filing/payment; **yearly** only if all hold: "U betaalt per jaar minder dan € 1.883 btw", no vergunning artikel 23, a natural person or a partnership of natural persons only, and < €10,000/year of intra-EU acquisitions and (separately) < €10,000 of intra-EU services received | Belastingdienst *Aangiftetijdvak voor de btw* [PRIMARY] | current | Period selector (kwartaal default) |
| **Due date**: aangifte and payment received by the last day of the month after the period. 2026 table: Jan → 28 Feb, Feb → 31 Mar, …, Dec → 31 Jan 2027; Q1 → 30 Apr, Q2 → 31 Jul, Q3 → 31 Oct, Q4 → 31 Jan 2027; year 2026 → **31 Mar 2027** | Belastingdienst *Wanneer moeten mijn btw-aangifte en mijn betaling binnen zijn?* [PRIMARY]; AWR art. 19 lid 1: pay "binnen één maand na het einde van dat tijdvak" [PRIMARY] | — | Due-date calendar = last day of following month; year return 31 Mar |
| **No weekend/holiday shift**: "De Algemene termijnenwet is niet van toepassing op de in het eerste en in het derde lid gestelde termijn van één maand" | AWR art. 19 lid 5 [PRIMARY] | — | Never move a due date to the next working day |
| Software submissions accepted "vanaf de 24e van de maand voor het einde van het betreffende tijdvak"; earlier gives an error | Belastingdienst due-date page [PRIMARY] | — | Future SBR client: block submission before that date |
| Non-established businesses: 2 months (quarter/month) / 3 months (year) | Belastingdienst search extract [PRIMARY, not read in full] | — | Not in scope |
| **Channels**: "Doe aangifte in Mijn Belastingdienst Zakelijk" (DigiD, eHerkenning or a European login); "U kunt uw aangifte ook doen met aangifte- of administratiesoftware"; first-time starters may file on paper | Toelichting 2026 pp. 3–4 [PRIMARY] | — | MVP: produce the figures to type into Mijn Belastingdienst Zakelijk; SBR/Digipoort submission is future work (Digipoort migration deadline for software was 1 May 2026, odb.belastingdienst.nl [PRIMARY, title]) |
| Always file when a return is issued, also a **nihilaangifte**; answer "Hebt u in dit tijdvak iets aan te geven?" | Toelichting 2026 p. 4 [PRIMARY] | — | Generate a zero return |
| **Corrections**: error ≤ **€1,000** may be processed in the next aangifte; > €1,000 needs a "Suppletie btw" | Toelichting 2026 p. 7 [PRIMARY] | — | Correction workflow (future) |
| **Oninbare vorderingen**: reclaim at the latest 1 year after the agreed due date (or 30 days after receipt if none) by deducting the btw and the omzet in 1a/1b | Toelichting 2026 p. 7 [PRIMARY] | — | Bad-debt adjustment as negative 1a/1b (future) |
| Payment to NL04 RABO 0200 1122 44 (Belastingdienst, Apeldoorn) with the **betalingskenmerk** | Toelichting 2026 p. 6 [PRIMARY] | — | Show payment details; kenmerk entered by the user |
| ICP-opgaaf (intra-EU supplies in 3b) is a separate filing | Toelichting 2026 3b [PRIMARY] | — | Future work |

### 6.2 Rubrieken (2025 and 2026)

**Rounding** [PRIMARY, Toelichting 2024, 2025 and 2026, identical text]: "Rond alle bedragen af op hele euro's. Dit mag u in uw voordeel doen. Zet bij negatieve bedragen een minteken (-) voor het bedrag." So every box, base and btw alike, is in whole euros; rounding toward the taxpayer is allowed (bases and btw due down, voorbelasting up). The Belastingdienst solar example repeats "Rond het bedrag af naar hele euro's in uw eigen voordeel" for 5b [PRIMARY].

**Columns**: in Mijn Belastingdienst Zakelijk "Vul in de kolom 'Omzet' het bedrag waarover btw wordt berekend in. Vul in de kolom 'Btw' het btw-bedrag in" [PRIMARY, Toelichting 2026 1a/1b]; the paper form calls them "Bedrag waarover omzetbelasting wordt berekend" and "Omzetbelasting" [PRIMARY, Stichting en Vereniging Loket]; "De opbouw van de rubrieken is zowel bij de digitale btw-aangifte … als bij de papieren btw-aangifte hetzelfde" [PRIMARY].

Labels are the headings of the *Toelichting bij de btw-aangifte 2026* (Dutch part for NL-established businesses) [PRIMARY]; 2025 is identical except where noted.

| Rubriek | Exact label (2026) | Base column (Omzet) | Tax column (Btw) | What goes in it |
|---|---|---|---|---|
| 1 | Prestaties binnenland | | | heading |
| 1a | Leveringen/diensten belast met hoog tarief | yes | yes | Domestic supplies at 21%; bad-debt reclaims as negatives; logies from 2026 |
| 1b | Leveringen/diensten belast met laag tarief | yes | yes | Domestic supplies at 9% |
| 1c | Leveringen/diensten belast met overige tarieven, behalve 0% | yes | yes | Sportkantine 13% forfait (only use named) |
| 1d | Privégebruik | [UNCERTAIN whether the base column exists] | yes | "Vul deze vraag alleen in de laatste aangifte van het jaar in": btw on private use (cars, energy, phone), deemed supplies; on business cessation in that period |
| 1e | Leveringen/diensten belast met 0% of niet bij u belast | yes | no | Domestic Tabel II 0% supplies (not exports 3a or intra-EU 3b) and supplies where the btw is verlegd to the customer ("btw verlegd" sales). Exempt supplies are **not** reported anywhere |
| 2 | Verleggingsregelingen binnenland | | | heading |
| 2a | Leveringen/diensten waarbij de btw naar u is verlegd (2024: "…waarbij de heffing van omzetbelasting naar u is verlegd") | yes | yes | Purchases where a Dutch supplier reverse-charged to you (construction subcontracting, scrap, chips/phones, etc.); also EU services on immovable property; deduct again in 5b |
| 3 | Prestaties naar of in het buitenland | | | heading |
| 3a | Leveringen naar landen buiten de EU (uitvoer) | yes | no | Exports outside the EU (incl. goods into a customs warehouse) |
| 3b | Leveringen naar of diensten in landen binnen de EU | yes | no | Intra-EU supplies of goods (by invoice date) and B2B services to EU customers (by period of supply); must match the ICP-opgaaf |
| 3c | Installatie/afstandsverkopen binnen de EU | yes | no | Installation/assembly in another EU country; distance sales above the €10,000 EU threshold when **not** using OSS |
| 4 | Prestaties vanuit het buitenland aan u geleverd | | | heading |
| 4a | Leveringen/diensten uit landen buiten de EU | yes | yes | Imports under vergunning artikel 23; services from non-EU suppliers reverse-charged to you; "U hoeft deze bedragen niet te splitsen naar de verschillende btw-tarieven"; deduct in 5b |
| 4b | Leveringen/diensten uit landen binnen de EU | yes | yes | Intra-EU acquisitions of goods; services from EU suppliers reverse-charged to you (not services on immovable property → 2a); deduct in 5b |
| 5 | Voorbelasting | | | heading |
| 5a | "Verschuldige btw (rubrieken 1 t/m 4)" (sic, Belastingdienst step-by-step page); "Verschuldigde btw" (solar example page) | — | computed | Sum of the btw column of 1a–4b; MBZ fills it automatically ("In de digitale aangifte wordt Rubriek 5a automatisch voor u ingevuld") |
| 5b | Voorbelasting | — | yes | Deductible Dutch btw invoiced to you in the period (even if unpaid), plus the self-assessed btw of 2a/4a/4b where deductible; non-deductible: private, exempt-use, horeca food and drink, staff benefits > €227/employee/year, wrongly charged btw, foreign EU btw |
| — | Totaal te betalen of terug te vragen | — | computed | "Het aangifteprogramma berekent automatisch het bedrag dat u moet betalen of terugvraagt" = 5a − 5b; positive = pay, negative = refund |

**5c–5g** [UNCERTAIN]: older forms (AFAS help, pre-2019) list 5c Subtotaal, 5d Vermindering volgens de kleineondernemersregeling, 5e Schatting vorige aangifte(n), 5f Schatting deze aangifte, 5g Totaal te betalen/terug te vragen. The 2024–2026 Toelichtingen explain only 5b and the total; the old KOR reduction (5d) ended with the 2020 KOR reform [SECONDARY]; a software vendor (MKG release note, 24 Feb 2025) removed 5e and 5f "from the VAT form" [SECONDARY]; vendor sites disagree on the current numbering of the total line (5c, 5e or 5g). Implementation: compute 5a, 5b and the total (= 5a − 5b); if the screen still shows 5c–5g, 5c = 5a − 5b and 5d/5e/5f = 0, 5g = 5c. Confirm against the live Mijn Belastingdienst Zakelijk screen or the SBR taxonomy before filing support.

Not filled in the Dutch aangifte: OSS turnover ("Deze omzet geeft u dus niet aan in de btw-aangifte"), foreign EU btw (reclaim via the EU refund portal), exempt supplies.

**Test vector** (Belastingdienst step-by-step example [PRIMARY]): quarter, 21% sales € 5,000 → 1a Omzet 5000, Btw 1050; purchases € 1,000 + € 210 → 5b 210; 5a 1050; total to pay € 840. Solar example: forfait btw € 60 in 1a, € 800 input → 5b 800, total −740 (refund).

---

## 7. Bookkeeping and retention

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Administratieplichtigen (lichamen; natural persons running a business or profession) must keep books "zodanig … dat te allen tijde hun rechten en verplichtingen … duidelijk blijken" and keep them "gedurende zeven jaar" | AWR art. 52 lid 1, 2, 4 [PRIMARY] | — | Never auto-delete within 7 years after year-end |
| Rechtspersonen: the board keeps books and documents "gedurende zeven jaren"; balance sheet and P&L within six months after the boekjaar | BW 2:10 lid 2–3 [PRIMARY] | — | Same |
| Data may be moved to another medium if correct and complete and readable within reasonable time (except the paper balance sheet and P&L) | AWR art. 52 lid 5; BW 2:10 lid 4 [PRIMARY] | — | Export/archive formats must be lossless |
| Records on immovable property "gedurende negen jaren, volgende op het jaar waarin hij het goed is gaan gebruiken" (≈ 10-year correction period) | Art. 34a Wet OB [PRIMARY] | — | Longer retention class for real-estate documents |
| Platforms facilitating supplies keep records "gedurende tien jaren" | Art. 34 lid 7 Wet OB [PRIMARY] | — | Not relevant to MVP |
| Inspector must be able to audit within a reasonable time; cooperation incl. insight into the set-up of the administration | AWR art. 52 lid 6 [PRIMARY] | — | Read-only auditor export (journal, ledgers, documents) |
| btw records (art. 34): notes of supplies made and received, intra-EU acquisitions, imports/exports, call-off stock registers | Art. 34 lid 1–2 Wet OB [PRIMARY] | — | Journal + document store cover this |

---

## 8. Financial year and currency

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| "Het boekjaar van een rechtspersoon is het kalenderjaar, indien in de statuten geen ander boekjaar is aangewezen." | BW 2:10a [PRIMARY] | — | Default calendar year; allow another start month for rechtspersonen |
| Eenmanszaak / vof: income-tax profit is determined per calendar year | Wet IB 2001 not read [UNCERTAIN] | — | Default calendar year; warn if changed |
| btw periods are calendar months/quarters/years, independent of the boekjaar | Belastingdienst *Aangiftetijdvak* [PRIMARY] | — | VAT periods independent of FY |
| Currency **EUR**, 2 decimals; btw amount on invoices in euro (art. 35a lid 4); aangifte in whole euros (§6.2) | [PRIMARY] | — | Base currency EUR, minor unit 2 |

---

## 9. Out of scope: Caribisch Nederland (BES)

Bonaire, Sint Eustatius and Saba (NL-BQ1..3) are public bodies of the Netherlands but outside the Wet OB and the EU VAT area; they levy the **algemene bestedingsbelasting (ABB)**, collected by the Belastingdienst Caribisch Nederland (rates reported for 2024: Bonaire 8% general / 25% higher; Saba and Sint Eustatius 6%) [SECONDARY, Rijksoverheid *Belangrijkste wijzigingen belastingen 2024* via search extract]. Aruba, Curaçao and Sint Maarten are separate countries with their own taxes. Not modelled.

---

## 10. Sources

- Wet op de omzetbelasting 1968 (BWBR0002629), version in force 1 Jan 2026: <https://wetten.overheid.nl/BWBR0002629/2026-01-01> — art. 9, 11, 25–25c, 34–35d, Tabel I and II
- Algemene wet inzake rijksbelastingen (BWBR0002320), version 1 Jan 2026: <https://wetten.overheid.nl/BWBR0002320/2026-01-01> — art. 19, 52
- Burgerlijk Wetboek Boek 2 (BWBR0003045) art. 10, 10a: <https://wetten.overheid.nl/jci1.3:c:BWBR0003045&boek=2&titeldeel=1&artikel=10&z=2026-01-01&g=2026-01-01>
- Belastingdienst *Toelichting bij de btw-aangifte* 2026 / 2025 / 2024: <https://download.belastingdienst.nl/belastingdienst/docs/toelichting_bij_btw_aangifte_ob0731t63fd.pdf>, <https://download.belastingdienst.nl/belastingdienst/docs/toelichting_bij_btw_aangifte_ob0731t53fd.pdf>, <https://download.belastingdienst.nl/belastingdienst/docs/toelichting_bij_dig_aangifte_omzetbelasting_ob0731t42fd.pdf> (index: <https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/themaoverstijgend/brochures_en_publicaties/toelichting_bij_de_digitale_aangifte_omzetbelasting>)
- Belastingdienst btw-id / ob-nummer: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/wat-betekent-het-nieuwe-btw-id-voor-mij>; btw-id controleren: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/btw-id-controleren>
- Belastingdienst step-by-step aangifte: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/ik-moet-btw-aangifte-doen-hoe-vul-ik-die-in>; solar examples: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/zonnepanelen-en-btw-voorbeeldaangiften>
- Belastingdienst periods: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/wijziging-aangiftetijdvak-btw>; due dates: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/uiterste-aangifte-en-betaaldatums>
- Belastingdienst logies: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/btw-logies>, <https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/berichten/nieuws/btw-logies>
- Belastingdienst invoices: <https://www.belastingdienst.nl/wps/wcm/connect/bldcontentnl/belastingdienst/zakelijk/btw/administratie_bijhouden/facturen_maken/factuureisen/factuureisen>; KOR, verlegging, vereenvoudigde factuur: `…/factuureisen/aangepaste_regels_facturen/u_maakt_gebruik_van_de_kleineondernemersregeling`, `…/u_maakt_gebruik_van_de_verleggingsregeling`, `…/vereenvoudigde_factuur`; Stichting en Vereniging Loket: <https://stichtingenvereniging.belastingdienst.nl/bijzondere-onderwerpen/administratie/factuureisen/>, <https://stichtingenvereniging.belastingdienst.nl/aangifte/aangifte-omzetbelasting/invullen-rubrieken/>
- Belastingdienst KOR: <https://www.belastingdienst.nl/kor> (→ `…/zakelijk/btw/hoe_werkt_de_btw/kleineondernemersregeling/kleineondernemersregeling`), voorwaarden: <https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/kor-voorwaarden>, gevolgen: `…/kleineondernemersregeling/wat-betekent-meedoen-met-de-kleineondernemersregeling/wat-betekent-meedoen-met-de-kleineondernemersregeling`, registratiedrempel: `…/kleineondernemersregeling/registratiedrempel-voor-kleine-ondernemers`
- Staatscourant 2024, 38565 (KOR changes 2025): <https://zoek.officielebekendmakingen.nl/stcrt-2024-38565.html>; Staatscourant 2012, 26808 (21% per 1 Oct 2012): <https://zoek.officielebekendmakingen.nl/stcrt-2012-26808.html>; Staatscourant 2018, 68655 (9% per 1 Jan 2019): <https://zoek.officielebekendmakingen.nl/stcrt-2018-68655.html>
- Rijksoverheid 17 Dec 2025 *Kabinet zet met belastingwijzigingen 2026 stappen…*: <https://www.rijksoverheid.nl/actueel/nieuws/2025/12/17/kabinet-zet-met-belastingwijzigingen-2026-stappen-naar-een-beter-belastingstelsel>; Wetsvoorstel *Wet behoud verlaagd btw-tarief cultuur, media en sport* (16 Sep 2025): <https://www.rijksoverheid.nl/binaries/rijksoverheid/documenten/kamerstukken/2025/09/16/wetsvoorstel-wet-behoud-verlaagd-btw-tarief-cultuur-media-en-sport/Wetsvoorstel-Wet-behoud-verlaagde-btw-tarief-cultuur-media-en-sport.pdf>
- Contourenbrief elektronisch factureren en rapporteren (TK 2026D42856): <https://www.tweedekamer.nl/kamerstukken/brieven_regering/detail?id=2026Z18679&did=2026D42856> (document: <https://www.tweedekamer.nl/downloads/document?id=2026D42856>); Kamerbrief 26 Jun 2025 ViDA: <https://www.rijksoverheid.nl/documenten/kamerstukken/2025/06/26/kamerbrief-over-de-implementatie-van-de-richtlijn-btw-in-het-digitale-tijdperk>; Accountancy Vanmorgen 11 Sep 2026: <https://www.accountancyvanmorgen.nl/2026/09/11/e-facturatie-verplicht-vanaf-2030-digitale-btw-rapportage-aan-fiscus-een-jaar-later/>
- Logius e-factureren: <https://www.logius.nl/actueel/e-facturatie-europees-formaat-verplicht-vanaf-18-april>, <https://logius.nl/domeinen/gegevensuitwisseling/e-factureren/hoe-werkt-het>; Ondernemersplein: <https://ondernemersplein.overheid.nl/wetten-en-regels/een-e-factuur-versturen-naar-de-overheid/>; odb.belastingdienst.nl Digipoort migration: <https://odb.belastingdienst.nl/en/digipoort-migration-by-may-1-2026-at-the-latest/>
- OpenPEPPOL BIS Billing 3.0 Schematron (NL-R rules): <https://github.com/OpenPEPPOL/peppol-bis-invoice-3/blob/master/rules/sch/PEPPOL-EN16931-UBL.sch>
- business.gov.nl VAT number: <https://business.gov.nl/regulation/vat-number/>; RSIN: <https://business.gov.nl/running-your-business/administration/an-rsin-number-for-your-business/>; KVK: <https://www.kvk.nl/over-het-handelsregister/vestigingsnummer/>, <https://developers.kvk.nl/nl/documentation/basisprofiel-api>
- Secondary: python-stdnum 2.2 `stdnum/nl/btw.py`, `stdnum/nl/bsn.py`, `stdnum/iso7064/mod_97_10.py`, `tests/test_eu_vat.doctest`, issue #182 and PR #185 <https://github.com/arthurdejong/python-stdnum>; nl.wikipedia *Btw-nummer (Nederland)* <https://nl.wikipedia.org/wiki/Btw-nummer_(Nederland)>, *Elfproef* <https://nl.wikipedia.org/wiki/Elfproef>; Wikipedia *ISO 3166-2:NL* <https://en.wikipedia.org/wiki/ISO_3166-2:NL>; AFAS help (old rubrieken) <https://help.afas.nl/help/en/se/Fin_Config_VatIct_Sction_Ship.htm>; MKG release note (5e/5f) <https://www.mkg.eu/mijn-mkg/support/kenniscentrum/id/11728/de-btw-rubrieken-5e-en-5f-zijn-verwijderd-van-het-omzetbelastingformulier-en-van-het-rapport-btw-aangifte-8644>; eConnect NLCIUS <https://econnect.eu/en/docs/knowledge/document-formats/formats/nlcius>; accountant.nl on rounding (Ahold) <https://www.accountant.nl/nieuws/2009/5/bij-btw-aangifte-halve-centen-naar-boven-afronden/>

---

## 11. Open questions

1. Is the btw-id MOD 97-10 algorithm published by the Belastingdienst (e.g. a note to software developers)? Only python-stdnum and its contributors' sources were found; the kleineondernemer.nl article they cite returned 403 and the archive was offline (§1.3).
2. The RSIN elfproef weights (9…2, −1 vs 9…1) are not stated in any official text found; the −1 variant is supported by all 29 real numbers tested (§1.3).
3. The current rubriek 5 layout in Mijn Belastingdienst Zakelijk and SBR (5c–5g: removed, renumbered or still shown?) and whether 1d has a base column (§6.2). Check the live screen or the Nederlandse Taxonomie OB entrypoint before building SBR filing.
4. Per-line vs per-invoice rounding of the btw amount on invoices: not in the Wet OB; the Hoge Raad (Ahold, 2009) required the usual arithmetic rounding (half a cent up) per receipt [SECONDARY]. Decide and document the invoice-level rule.
5. Whether a gap in invoice numbering breaches art. 35a lid 1 b ("opeenvolgend") and the legal basis of the KVK-nummer on invoices (§3).
6. The prescribed KOR invoice wording, if any (Belastingdienst only says to mention that the KOR applies) (§3, §5).
7. SBR / Digipoort filing for btw: PKIoverheid certificate needs, the current OB taxonomy version, and whether an open-source desktop app can register as software developer (§6.1).
8. The final shape of the 2030 domestic B2B e-invoicing law (infrastructure choice due after Oct 2026; consultation autumn 2026) (§4).
9. The exact legal start date of the 2020 btw-id and of the 2020 KOR (only secondary sources and Kamerstuk titles read) (§1.1, §5).
10. Income-tax year for eenmanszaken (assumed calendar year; Wet IB 2001 not read) (§8).

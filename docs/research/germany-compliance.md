# Germany Compliance Requirements (Umsatzsteuer, E-Rechnung, GoBD): KhataOS Spec

> **Status as of:** 2026-10-20. **Scope:** German businesses (Einzelunternehmer, Personengesellschaften, Kapitalgesellschaften) keeping books in KhataOS, a local-first desktop accounting app. The `khata-de` pack implements the VAT and identity items below (see `docs/germany-vat.md`); the structured-invoice items (XRechnung, ZUGFeRD, Peppol) belong to the shared EU crate `khata-en16931` (see `docs/en16931.md`).
> **Not legal advice.** This file turns the UStG, UStDV, AO, EStG, BMF letters, ELSTER and KoSIT specifications into software requirements. Get a German Steuerberater to confirm each item before release.
>
> **Confidence tags:**
> - **[PRIMARY]** means read in the primary text: gesetze-im-internet.de (UStG, UStDV, AO, EStG, EStDV, ERechV), a BMF letter or BMF form, an ELSTER or KoSIT specification, the KoSIT Schematron source, xeinkauf.de, ferd-net.de or docs.peppol.eu.
> - **[SECONDARY]** means reported consistently by reputable non-official sources (Wikipedia, python-stdnum, the Mustang ZUGFeRD reference library, tax-advisor and chamber sites), but the official text was not retrieved in this research.
> - **[UNCERTAIN]** means sources conflict, the item is interpretive, or the point was not found. Flagged for review.
>
> Several bundesfinanzministerium.de URLs returned HTTP 503 / a bot check during this research; BMF letters marked [PRIMARY] were read from the identical PDF via web.archive.org or a chamber mirror (URLs in §12).

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **USt-IdNr validation**: `DE` + 9 digits, first digit not 0, ISO 7064 MOD 11,10 check digit | Wikipedia (ISO/IEC 7064 MOD 11,10); python-stdnum; ELSTER publishes the same algorithm for the IdNr (§1.2) |
| 2 | **Steuernummer**: accept the printed per-state format, convert to the 13-digit ELSTER format with the state's BUFA prefix, validate the state check digit (11er, 2er, modified 11er; Berlin A/B) | ELSTER *Prüfung der Steuer- und Steueridentifikationsnummer* (Stand 02.09.2026) (§1.3) |
| 3 | **Rates** 19% / 7% / 0% (PV, §12 Abs. 3) from an effective-dated master; 16% / 5% for 1 Jul–31 Dec 2020; restaurant food 7% permanently from 1 Jan 2026 | §12, §28 UStG (§2) |
| 4 | **Invoice particulars** §14 Abs. 4 (10 items), §14a notes ("Steuerschuldnerschaft des Leistungsempfängers", "Gutschrift"), §33 UStDV Kleinbetragsrechnung ≤ €250, §34a UStDV Kleinunternehmer invoice | §14, §14a UStG; §§33, 34a UStDV (§3) |
| 5 | **E-Rechnung**: receive from 1 Jan 2025; issue EN 16931 e-invoices (XRechnung or ZUGFeRD ≥ 2.0.1 except MINIMUM/BASIC WL) for domestic B2B; transition to 31 Dec 2026 / 31 Dec 2027 (≤ €800,000 or EDI) | §14 Abs. 1–2, §27 Abs. 38 UStG; BMF 15 Oct 2024 and 15 Oct 2025 (§4) |
| 6 | **§19 Kleinunternehmer**: €25,000 prior year / €100,000 current year (from 1 Jan 2025); exempt supplies go to Kz 48 | §19 UStG; BMF 9 Dec 2024 (§5) |
| 7 | **UStVA** for 2025 and 2026 from the books, with the official Kennzahlen; bases in whole euros, tax in euros and cents; due on the 10th; ELSTER/ERiC only | BMF Vordruckmuster USt 1 A 2025 (9 Dec 2024) and 2026 (29 Dec 2025); §18 UStG (§6) |
| 8 | **Retention**: books 10 years, Buchungsbelege and invoices 8 years (since BEG IV), other business letters 6 years; keep e-invoice XML unaltered | §147 AO; §14b UStG; GoBD as amended 14 Jul 2025 (§7) |
| 9 | **Wirtschaftsjahr** = calendar year by default; registered traders may use another 12-month period (change needs Finanzamt consent); currency EUR, 2 decimals | §4a EStG; §8b EStDV (§8) |
| 10 | **XRechnung 3.0.x** (UBL 2.1 or CII) with BR-DE rules; Leitweg-ID MOD 97-10 for B2G | KoSIT xeinkauf.de; XRechnung Schematron v2.6.0; Leitweg-ID spec 2.0.2 (§9) |
| 11 | **ZUGFeRD / Factur-X** PDF/A-3 hybrid with embedded CII XML; profile URNs | FeRD; Mustang reference library (§10) |

---

## 1. Identity numbers

### 1.1 USt-IdNr (Umsatzsteuer-Identifikationsnummer)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Format: always begins with `DE`, followed by 9 digits, the last a check digit | Wikipedia de *Umsatzsteuer-Identifikationsnummer* [SECONDARY]; python-stdnum `stdnum/de/vat.py` [SECONDARY]; BMF Vordruck USt 1 A 2026 shows "D E" + digits in the W-IdNr field [PRIMARY, layout only] | — | Accept with/without `DE`, strip spaces, `-./,`; store `DE` + 9 digits |
| Check digit: "In Deutschland wird die Prüfziffer nach ISO/IEC 7064, MOD 11,10 berechnet" | Wikipedia de [SECONDARY]; python-stdnum uses `iso7064.mod_11_10` [SECONDARY]. No BZSt page with the algorithm was found [UNCERTAIN whether BZSt publishes it] | — | Implement §1.2 |
| First digit after `DE` may **not** be `0` | python-stdnum `validate()`: `number[0] == '0'` → `InvalidFormat` [SECONDARY]; lookuptax ("El primer dígito nunca es cero") [SECONDARY]; not found in an official text [UNCERTAIN] | — | Reject a leading 0 (a warning would be a safer UX if a counter-example ever appears) |
| Online validity: check-digit pass shows only that the number is well-formed. Registration is confirmed via the BZSt **eVatR** service (simple or "qualifizierte" confirmation with name/address) or the EU **VIES** | Wikipedia de [SECONDARY] | — | Optional online check (future work) |
| eVatR moved to a **REST API** (published July 2025); the old XML-RPC interface was switched off on **30 Nov 2025** | BZSt newsletter USTKV 01/2025 as cited by the `rechtlogisch/evatr` package and a Shopware forum report [SECONDARY]; official BZSt page not retrieved [UNCERTAIN] | 30 Nov 2025 | Any eVatR client must target the REST API |
| **Wirtschafts-Identifikationsnummer (W-IdNr)**: `DE` + 9 digits (same body as the USt-IdNr) + `-` + 5-digit Unterscheidungsmerkmal (e.g. `DE232637828-12345`); assigned by the BZSt gradually from Nov 2024 (§139c AO). The 2025 and 2026 UStVA forms have a W-IdNr field in line 1, filled "nur …, wenn diese Ihnen bereits bekannt ist" | Form layout and instruction: BMF USt 1 A / USt 1 E 2026 [PRIMARY]; format and rollout: Buhl, Papierkram, Ebner Stolz [SECONDARY] | Nov 2024 onward | Optional party field; validate the 9-digit body with §1.2 and the suffix as 5 digits |

### 1.2 USt-IdNr check digit (ISO 7064 MOD 11,10), as implemented

For `DE d1 … d9`:

1. `product = 10`
2. For each of `d1 … d8`: `sum = (d + product) mod 10`; if `sum == 0` then `sum = 10`; `product = (2 × sum) mod 11`.
3. `check = 11 − product`; if `check == 10` then `check = 0`.
4. Valid when `check == d9`.

ELSTER publishes exactly this procedure (`produkt = m = 10`, `summe = (ziffer + produkt) % 10`, `if summe == 0 then summe = 10`, `produkt = (2*summe) % 11`, `pruefZiffer = 11 − produkt`, `10 → 0`) as the check-digit algorithm for the 11-digit Steuer-IdNr [PRIMARY, ELSTER *Prüfung der Steuer- und Steueridentifikationsnummer* §2.2]. Applying it to the USt-IdNr rests on the MOD 11,10 statements above [SECONDARY]. python-stdnum's formulation (`check = 5`; `check = (((check or 10) * 2) % 11 + digit) % 10`; valid when the final value is 1) gives the same result; this was confirmed here on 200,000 random 9-digit numbers.

**Test vectors** (computed here with the algorithm above, Python `-I`):

| Number | Source | Result |
|---|---|---|
| DE136695976 | python-stdnum `de/vat.py` docstring | valid |
| DE136695978 | python-stdnum docstring (negative case) | invalid |
| DE118619592, DE125014955, DE129304291, DE811363057 | python-stdnum `tests/test_eu_vat.doctest` | valid |
| DE232637828 | W-IdNr example `DE232637828-12345` (Buhl / lohnsteuer-kompakt) | valid (body) |

### 1.3 Steuernummer (tax number from the Finanzamt)

The ELSTER format is 13 digits, `FFFF0BBBUUUUP` (all states except NRW) or `FFFF0BBBBUUUP` (NRW): 4-digit Bundesfinanzamtsnummer (BUFA), a fixed `0`, Bezirk, Unterscheidungsnummer, 1-digit Prüfziffer [PRIMARY, ELSTER doc §3]. Formal rules [PRIMARY, §5]: exactly 13 digits; the 5th digit is always `0` (NRW too); the BUFA must be in ELSTER's list of supported Finanzämter (`Finanzamtsdaten.xlsx/.xml` download); Bezirk ≠ 000, 998, 999 (NRW: 0000, 0998, 0999); in BY, BB, MV, SL, SN, ST, TH the Bezirk must be ≥ 100; NRW `UUUP` > 0009. "Gültige Steuernummern haben immer eine einstellige Prüfziffer" (numbers whose computation gives two digits are not issued) [PRIMARY, §6.1].

| State | ISO 3166-2 | Printed (Bescheid) format | ELSTER 13-digit | BUFA prefix | Check method | ELSTER test number (13-digit / printed) |
|---|---|---|---|---|---|---|
| Baden-Württemberg | DE-BW | `FFBBB/UUUUP` | `28FF0BBBUUUUP` | 28 | 2er | 2866081508156 / 66815/08156 |
| Bayern | DE-BY | `FFF/BBB/UUUUP` | `9FFF0BBBUUUUP` | 9 | 11er (Bayern) | 9198081508152 / 198/815/08152 |
| Berlin | DE-BE | `FF/BBB/UUUUP` | `11FF0BBBUUUUP` | 11 | 11er Berlin-A or Berlin-B (by Finanzamt and Bezirk) | 1197081508154 / 97/815/08154 |
| Brandenburg | DE-BB | `FFF/BBB/UUUUP` (FFF starts 0) | `30FF0BBBUUUUP` | 30 | 11er | 3098081508157 / 098/815/08157 |
| Bremen | DE-HB | `FF BBB UUUUP` | `24FF0BBBUUUUP` | 24 | 11er | 2497012301233 / 97 123 01233 |
| Hamburg | DE-HH | `FF/BBB/UUUUP` | `22FF0BBBUUUUP` | 22 | 11er | 2293081508150 / 93/815/08150 |
| Hessen | DE-HE | `0FF BBB UUUUP` | `26FF0BBBUUUUP` | 26 | 2er | 2653081508158 / 053 815 08158 |
| Mecklenburg-Vorpommern | DE-MV | `FFF/BBB/UUUUP` (FFF starts 0) | `40FF0BBBUUUUP` | 40 | 11er | 4098081508157 / 098/815/08157 |
| Niedersachsen | DE-NI | `FF/BBB/UUUUP` | `23FF0BBBUUUUP` | 23 | 11er | 2388081508158 / 88/815/08158 |
| Nordrhein-Westfalen | DE-NW | `FFF/BBBB/UUUP` | `5FFF0BBBBUUUP` | 5 | 11er (NRW variant) | 5400081508159 / 400/8150/8159 |
| Rheinland-Pfalz | DE-RP | `FF/BBB/UUUUP` (old: `FF/BBB/UUUU/P`) | `27FF0BBBUUUUP` | 27 | modified 11er | 2799081508152 / 99/815/08152 |
| Saarland | DE-SL | `FFF/BBB/UUUUP` (FFF starts 0; Bavarian format) | `10FF0BBBUUUUP` | 10 | 11er (Bayern) | 1096081508187 / 096/815/08187 |
| Sachsen | DE-SN | `FFF/BBB/UUUUP` (FFF starts 2) | `32FF0BBBUUUUP` | 32 | 11er | 3248081508156 / 248/815/08156 |
| Sachsen-Anhalt | DE-ST | `FFF/BBB/UUUUP` (FFF starts 1) | `31FF0BBBUUUUP` | 31 | 11er | 3198081508152 / 198/815/08152 |
| Schleswig-Holstein | DE-SH | `FF/BBB/UUUUP` (old: spaces) | `21FF0BBBUUUUP` | 21 | 2er | 2138081508154 / 38/815/08154 |
| Thüringen | DE-TH | `FFF/BBB/UUUUP` (FFF starts 1) | `41FF0BBBUUUUP` | 41 | 11er | 4198081508152 / 198/815/08152 |

Sources: printed formats, test numbers and methods from ELSTER Tabelle 4-1 and §6.3 [PRIMARY]; prefixes read off the ELSTER test numbers and matching python-stdnum `de/stnr.py` [PRIMARY + SECONDARY]. ISO codes: §1.4. All 19 ELSTER test numbers (incl. NRW 5500…, 5600… and Bayern 9296…) pass the algorithms below, implemented and run here.

Notes [PRIMARY, ELSTER §4]: in Hessen advisers often drop the leading 0; Berlin now always prints the last two BUFA digits and leading zeros; in Bayern some Finanzämter omit the FFF on letters; Rheinland-Pfalz dropped the slash before P; Schleswig-Holstein moved from spaces to slashes. Wikipedia de *Steuernummer* gives Saarland as `FF/BBB/UUUUP` and python-stdnum as `0FFBBBUUUUP`; ELSTER's own example is `096/815/08187` → use ELSTER [Wikipedia SECONDARY, outdated]. Printed 10- and 11-digit numbers are ambiguous across states, so **the user must pick the Bundesland** before conversion.

**Check-digit algorithms** (positions are the 12 digits before P in ELSTER format; P is position 13) [PRIMARY, ELSTER §6.4–§7.2]:

- **11er** (all except NRW): `S = Σ digit × weight`; `P = 0` if `S mod 11 = 0`, else `11 − (S mod 11)` (a result of 10 is never issued). Weights:
  - BY, BB, MV, SL, SN, ST, TH: `0 5 4 3 0 2 7 6 5 4 3 2`
  - HB, HH: `0 0 4 3 0 2 7 6 5 4 3 2`
  - NI and Berlin-B: `0 0 2 9 0 8 7 6 5 4 3 2`
  - Berlin-A: `0 0 0 0 0 7 6 5 8 4 3 2`
- **NRW 11er**: weights `0 3 2 1 0 7 6 5 4 3 2 1`; `P = S mod 11`.
- **Berlin A/B selection**: per BUFA, e.g. 1113 Charlottenburg = A, but B for Bezirke 201–693; 1115, 1118, 1131–1138, 1194–1198 = B; 1127, 1129, 1130 = A; Neukölln (1116) B for Bezirke 1–29, 201–693, 875–899; Spandau (1119) B for 201–639, 680, 684. Full table in ELSTER §7.2.
- **Modified 11er** (RP): weights `0 0 1 2 0 1 2 1 2 1 2 1`; each product ≥ 10 becomes `(product mod 10) + 1`; `P = (10 − Σ mod 10) mod 10`.
- **2er** (BW, HE, SH): per position add summand `0 0 9 8 0 7 6 5 4 3 2 1`, keep the last digit; multiply by factor `0 0 512 256 0 128 64 32 16 8 4 2`; reduce each product to a single digit by repeated digit sums; add; `P = 0` if the sum ends in 0, else `10 − (sum mod 10)`.
- Bayern no longer accepts 2er-method numbers [PRIMARY, §7.1].

### 1.4 Bundesländer (ISO 3166-2:DE)

DE-BW Baden-Württemberg, DE-BY Bayern, DE-BE Berlin, DE-BB Brandenburg, DE-HB Bremen, DE-HH Hamburg, DE-HE Hessen, DE-MV Mecklenburg-Vorpommern, DE-NI Niedersachsen, DE-NW Nordrhein-Westfalen, DE-RP Rheinland-Pfalz, DE-SL Saarland, DE-SN Sachsen, DE-ST Sachsen-Anhalt, DE-SH Schleswig-Holstein, DE-TH Thüringen. [SECONDARY, Wikipedia *ISO 3166-2:DE*; the ISO OBP was not queried]

The Leitweg-ID uses a different numbering (AGS state codes): 01 SH, 02 HH, 03 NI, 04 HB, 05 NW, 06 HE, 07 RP, 08 BW, 09 BY, 10 SL, 11 BE, 12 BB, 13 MV, 14 SN, 15 ST, 16 TH, 99 Bund [PRIMARY, Leitweg-ID spec 2.0.2 §2.2.1]. These are **not** the Steuernummer BUFA prefixes.

---

## 2. Rates (§12 UStG) and scope

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Standard rate 19%**: "für jeden steuerpflichtigen Umsatz 19 Prozent der Bemessungsgrundlage" | §12 Abs. 1 UStG [PRIMARY] | 1 Jan 2007 [SECONDARY; earlier history not needed] | `STANDARD` 19% |
| **Reduced rate 7%**: "Die Steuer ermäßigt sich auf sieben Prozent für die folgenden Umsätze" (Anlage 2 goods, books, local transport, etc.) | §12 Abs. 2 UStG [PRIMARY] | — | `REDUCED` 7% |
| **Temporary 16% / 5%**: "vom 1. Juli 2020 bis 31. Dezember 2020" | §28 Abs. 1 (16%) and Abs. 2 (5%) UStG [PRIMARY] (Zweites Corona-Steuerhilfegesetz [SECONDARY]) | 2020-07-01 to 2020-12-31 inclusive | Effective-dated rows: 16% and 5% for that window, 19%/7% before and after. The 2026 UStVA guide still tells filers to report payments received now for supplies made "nach dem 30. Juni 2020 und vor dem 1. Januar 2021" in line 16 (Kz 35/36) [PRIMARY] |
| **0% for photovoltaic**: supply and installation of solar modules and essential components to the operator of a PV system on or near private dwellings / public-interest buildings; deemed met if gross output per Marktstammdatenregister ≤ 30 kW (peak). Reported in UStVA line 15, **Kz 87** | §12 Abs. 3 UStG [PRIMARY]; USt 1 E 2026 line 15 [PRIMARY]; from 1 Jan 2023 [SECONDARY] | 1 Jan 2023 | `ZERO_PV` 0% class — a taxable supply at 0%, not an exemption |
| **Restaurant and catering services (excluding drinks)** at 7%, now permanent in §12 Abs. 2 Nr. 15 | §12 Abs. 2 UStG text lists "Restaurant- und Verpflegungsdienstleistungen (ohne Getränkeabgabe)" [PRIMARY]; Steueränderungsgesetz 2025, BGBl. 2025 I Nr. 363, 23 Dec 2025, in force 1 Jan 2026 (IWW, Haufe, nwb) [SECONDARY] | 1 Jan 2026 (an earlier temporary 7% ran 1 Jul 2020–31 Dec 2023 [SECONDARY]) | Item-level rate choice; drinks stay 19% |
| Gas via the gas grid and heat via a heat network at 7%: 1 Oct 2022–31 Mar 2024 | §28 Abs. 5, 6 UStG [PRIMARY] | historic | Not seeded (historic) |
| **Exempt** supplies §4 Nr. 8–29 (finance, insurance, letting, medical, etc.) — no input-tax deduction; §4 Nr. 1–7 (exports, intra-EU supplies) keep deduction | UStVA lines 22/23 wording [PRIMARY] | — | Two exemption classes: `EXEMPT_WITH_CREDIT` (Kz 41/43/44/49) and `EXEMPT_NO_CREDIT` (Kz 48) |

---

## 3. Invoices (§14, §14a UStG; §§33, 34, 34a UStDV)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Definitions**: "Rechnung ist jedes Dokument, mit dem über eine Lieferung oder sonstige Leistung abgerechnet wird"; an *elektronische Rechnung* is "in einem strukturierten elektronischen Format ausgestellt, übermittelt und empfangen … und ermöglicht eine elektronische Verarbeitung"; a *sonstige Rechnung* is paper or another electronic format | §14 Abs. 1 UStG [PRIMARY] | 1 Jan 2025 | Model `InvoiceForm = EInvoice / Other` |
| **Obligation** to invoice within **six months** for supplies to another business, to a non-business legal person, and certain construction-related supplies, unless exempt under §4 Nr. 8–29 | §14 Abs. 2 Satz 2 UStG [PRIMARY] | — | Warn on uninvoiced deliveries older than 6 months (future work) |
| **§14 Abs. 4 particulars**: (1) full name and address of supplier and recipient; (2) "die dem leistenden Unternehmer vom Finanzamt erteilte Steuernummer" or the BZSt-issued USt-IdNr; (3) issue date; (4) "eine fortlaufende Nummer mit einer oder mehreren Zahlenreihen, die zur Identifizierung der Rechnung vom Rechnungsaussteller einmalig vergeben wird"; (5) quantity and trade name of goods / scope and type of service; (6) "den Zeitpunkt der Lieferung oder sonstigen Leistung" (or of the advance payment); (7) consideration split by tax rate and by exemption, plus every agreed reduction (e.g. Skonto) not already in the price; (8) rate and tax amount per rate, or a note that an exemption applies; (9) the note on the recipient's 2-year retention duty in §14b Abs. 1 Satz 5 cases (construction-related supplies to private persons); (10) the word "Gutschrift" for self-billing | §14 Abs. 4 Satz 1 Nr. 1–10 UStG [PRIMARY; Nr. 2, 4, 6 quoted, the rest paraphrased from the text] | — | Invoice model carries every field; numbering per series is unique and gap-free by construction [gap-free not required by the text: UNCERTAIN] |
| Kleinbetragsrechnungen, Fahrausweise and Kleinunternehmer invoices "müssen keine fortlaufende Nummer enthalten" | UStAE 14.5 Abs. 14 as amended by BMF 15 Oct 2025 [PRIMARY] | 2025 | Number them anyway (harmless) |
| **Intra-EU B2B services (§3a Abs. 2)** where the customer owes the tax in another member state: invoice with the note **"Steuerschuldnerschaft des Leistungsempfängers"**, issued by the 15th of the following month, with **both** USt-IdNrn | §14a Abs. 1 Sätze 1–3 UStG [PRIMARY] | — | Auto-add the note and require both VAT IDs |
| **Intra-EU supply of goods**: invoice by the 15th of the following month with both USt-IdNrn; the exemption note comes from §14 Abs. 4 Nr. 8 | §14a Abs. 3 UStG [PRIMARY] | — | Require customer VAT ID; print exemption note |
| **Domestic reverse charge (§13b Abs. 2)** where the recipient owes the tax under §13b Abs. 5: invoice "mit der Angabe „Steuerschuldnerschaft des Leistungsempfängers"", and **no separate VAT amount** (§14 Abs. 4 Nr. 8 does not apply) | §14a Abs. 5 UStG [PRIMARY] | — | Reverse-charge invoices print the exact note and 0 VAT |
| Special notes: "Sonderregelung für Reisebüros" (§25); margin scheme (§25a) "Gebrauchtgegenstände/Sonderregelung", "Kunstgegenstände/Sonderregelung", "Sammlungsstücke und Antiquitäten/Sonderregelung" | §14a Abs. 6 UStG [PRIMARY] | — | Text constants (schemes not modelled) |
| **Kleinbetragsrechnung**: "Eine Rechnung, deren Gesamtbetrag 250 Euro nicht übersteigt": supplier name and address, issue date, quantity/type, gross amount and tax as one sum plus the rate (or exemption note). Not for §3c, §6a, §13b supplies. May always be sent as a *sonstige Rechnung* even where §14 Abs. 2 requires an e-invoice | §33 UStDV [PRIMARY] | €250 since 1 Jan 2017 [SECONDARY] | Simplified layout when gross ≤ €250 and not 3c/6a/13b |
| **Fahrausweise** (§34 UStDV) are likewise outside the e-invoice obligation | BMF 15 Oct 2025 (UStAE 14.1, 14.5) [PRIMARY] | — | Not modelled |
| **Kleinunternehmer invoice (§34a UStDV)**: names and addresses of both parties; Steuernummer, USt-IdNr or Kleinunternehmer-IdNr; issue date; goods/services; one total amount and a statement that the §19 exemption applies; "Gutschrift" for self-billing. May always be issued as a *sonstige Rechnung* | §34a UStDV [PRIMARY, paraphrased]; UStAE 14.7a Abs. 3 per BMF 15 Oct 2025 [PRIMARY] | 1 Jan 2025 | Print a note such as "Steuerfrei aufgrund der Kleinunternehmerregelung gemäß § 19 UStG" [wording not prescribed: UNCERTAIN]; no VAT lines |

### 3.1 §13b reverse charge — supplies covered (summary)

§13b Abs. 1: services under §3a Abs. 2 from an entrepreneur established elsewhere in the EU. §13b Abs. 2: Nr. 1 Werklieferungen and services by foreign-established entrepreneurs; Nr. 2 security goods outside insolvency; Nr. 3 transactions under the GrEStG; Nr. 4 Bauleistungen; Nr. 5 a) §3g goods from foreign entrepreneurs, b) gas/electricity; Nr. 6 emission allowances and certificates; Nr. 7 Anlage 3 goods (scrap etc.); Nr. 8 Gebäudereinigung; Nr. 9 gold ≥ 325/1000; Nr. 10 mobile phones, tablets, game consoles, integrated circuits when the invoiced total per transaction ≥ €5,000; Nr. 11 Anlage 4 metals when ≥ €5,000; Nr. 12 telecommunications services. For Abs. 1 and Abs. 2 Nr. 1–3 the recipient owes the tax if it is a business or a legal person; for Nr. 4, 8, 12 and some energy supplies further conditions apply (e.g. Finanzamt certificate). [PRIMARY, §13b UStG, paraphrased]

UStVA mapping: buyer side Kz 46/47 (Abs. 1), 73/74 (Abs. 2 Nr. 3), 84/85 (other); input tax Kz 67; seller side Kz 60 [PRIMARY, USt 1 A 2026].

---

## 4. E-Rechnung (mandatory B2B e-invoicing)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Format rule**: the structured format "muss der europäischen Norm für die elektronische Rechnungsstellung" (EN 16931, Directive 2014/55/EU) entsprechen, or "kann zwischen Rechnungsaussteller und Rechnungsempfänger vereinbart werden" if it allows "die richtige und vollständige Extraktion" of the required data into an EN 16931-conformant or interoperable format | §14 Abs. 1 Sätze 6–7 UStG [PRIMARY] | 1 Jan 2025 | Emit EN 16931 (XRechnung or ZUGFeRD EN16931/XRECHNUNG profile) |
| **Who must issue**: for B2B supplies between businesses resident in Germany (Nr. 1), "als elektronische Rechnung nach Absatz 1 Satz 3 und 6 auszustellen". Issuing an e-invoice to a domestic business needs **no consent**; any other electronic invoice needs the recipient's consent | §14 Abs. 2 UStG [PRIMARY]; BMF 15 Oct 2025, UStAE 14.1 Abs. 4 [PRIMARY] | 1 Jan 2025 | Default domestic B2B to e-invoice; mark B2C and foreign customers as "other" |
| **Receiving**: "Inländische Unternehmer müssen die technischen Voraussetzungen zum Empfang einer E-Rechnung schaffen", including §19 Kleinunternehmer; an e-mail inbox is sufficient | BMF 15 Oct 2025, UStAE 14.1 Abs. 5 [PRIMARY] | 1 Jan 2025 (no transition) | Import XRechnung/ZUGFeRD XML for purchase bookings (future work); store the XML as received |
| The obligation also covers §13b Abs. 2 supplies, §§23a–25c schemes, §4 Nr. 1–7 exempt supplies (e.g. intra-EU supply to a German business's foreign branch), mixed invoices and §9 options | BMF 15 Oct 2025, UStAE 14.1 Abs. 4 Sätze 8–10 [PRIMARY] | 2025 | Do not exempt these from the e-invoice path |
| **Exceptions**: Kleinbetragsrechnungen ≤ €250 (§33 UStDV), Fahrausweise (§34 UStDV), Kleinunternehmer invoices (§34a UStDV); supplies exempt under §4 Nr. 8–29; B2C | §14 Abs. 2 UStG; §§33, 34a UStDV; BMF 15 Oct 2025 [PRIMARY] | — | Allow PDF/paper for these |
| **Transition** (§27 Abs. 38 UStG): (1) supplies 1 Jan 2025–31 Dec 2026: paper, or with the recipient's consent another electronic format; (2) supplies in 2027: same, "wenn der Gesamtumsatz (§ 19 Absatz 2) des die Rechnung ausstellenden Unternehmers im vorangegangenen Kalenderjahr nicht mehr als 800 000 Euro betragen hat"; (3) supplies in 2027: EDI (Recommendation 94/820/EC) in a non-EN format with consent | §27 Abs. 38 Nr. 1–3 UStG [PRIMARY] | to 31 Dec 2026; to 31 Dec 2027 | Setting "issue e-invoice from" (default: now). From **1 Jan 2028** every domestic B2B invoice must be an e-invoice |
| **What counts**: XRechnung "entspricht der Normenreihe EN 16931 und den Anforderungen des § 14 Abs. 1 Satz 6 Nr. 1 UStG"; "das hybride Format ZUGFeRD ab der Version 2.0.1 – ausgenommen die Profile MINIMUM und BASIC-WL – beruht auf der Normenreihe EN 16931". Other EN 16931 formats are also allowed | BMF 15 Oct 2025, UStAE 14.1 Abs. 13–14 [PRIMARY] (same in BMF 15 Oct 2024 [SECONDARY via chamber summaries]) | 2025 | Never offer MINIMUM / BASIC WL as an e-invoice |
| **Hybrid precedence**: "Bei einem hybriden Format bilden die im strukturierten Teil vorliegenden Rechnungsdaten den führenden Teil … gehen die Daten des strukturierten Teils denen der Bilddatei vor" | BMF 15 Oct 2025, UStAE 14.4 [PRIMARY] | 2025 | Render the PDF from the same data as the XML; never let them differ |
| **New in 2025 letter — error classes**: a *Formatfehler* (file does not meet the syntax) means the file is **not** an e-invoice but a *sonstige Rechnung*; a *Geschäftsregelfehler* (breach of the format's business rules, e.g. tax amount vs rate, or missing "BT-10 Buyer reference" in an XRechnung) is distinct; violations of rules such as BT-10 are "umsatzsteuerlich unbeachtlich"; *Inhaltsfehler* concern §14 Abs. 4 content | BMF 15 Oct 2025, Rn. 6a, 6b; UStAE 14.1 Abs. 11, 14.5 Abs. 1 [PRIMARY] | Supplies after 31 Dec 2024 | Validate every outgoing XML against XSD + Schematron (KoSIT) before issuing; block on Formatfehler |
| The 2025 letter applies to all supplies after 31 Dec 2024 | BMF 15 Oct 2025, Anwendungsregelung [PRIMARY] | 2025 | — |
| A visual (human-readable) copy is not required: "Die zusätzliche Erstellung eines menschenlesbaren Dokuments ist nicht erforderlich" | BMF 15 Oct 2025, UStAE 14.4 [PRIMARY] | 2025 | PDF optional for pure XRechnung |
| **Public sector (B2G)**: federal suppliers must issue and transmit invoices electronically (§3 Abs. 1 ERechV in force **27 Nov 2020**); exceptions include direct orders up to **€1,000**; format "Datenaustauschstandard XRechnung" or another EN-conformant standard; additional mandatory data: **Leitweg-ID**, bank details, payment terms, De-Mail/e-mail of the issuer (plus supplier number and order number if given) | ERechV §3, §4, §5, §11 [PRIMARY] | 27 Nov 2020 (federal) | Leitweg-ID in BT-10; Länder have their own rules [UNCERTAIN, not researched] |

---

## 5. Kleinunternehmer (§19 UStG)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| From 2025 the supplies are **tax-exempt** if the Gesamtumsatz "im vorangegangenen Kalenderjahr 25 000 Euro nicht überschritten hat" and "im laufenden Kalenderjahr 100 000 Euro nicht überschreitet" | §19 Abs. 1 UStG [PRIMARY]; BMF 9 Dec 2024 (Jahressteuergesetz 2024, BGBl. 2024 I Nr. 387) [PRIMARY] | 1 Jan 2025 | Turnover monitor on both limits |
| If €100,000 is exceeded during the year, "unterliegt bereits der die Grenze überschreitende Umsatz der Regelbesteuerung"; enter the date in UStVA line 12 (**Kz 70**) | BMF 9 Dec 2024 [PRIMARY]; USt 1 A 2025/2026 line 12 [PRIMARY] | 2025 | Switch the transaction that crosses €100,000 (and later ones) to normal VAT; report the date |
| Before 2025: €22,000 prior year / €50,000 expected current year (not a tax exemption but non-collection) | Wikipedia and tax-advisor sources [SECONDARY] | to 31 Dec 2024 | Historic only |
| Kleinunternehmer report exempt supplies in **Kz 48** besides the normal Kennzahlen | BMF 9 Dec 2024 [PRIMARY] | 2025 | Map §19 sales to Kz 48 |
| Waiver: declare by the last day of February of the second following year; binding for at least five years | §19 Abs. 3 UStG; BMF 9 Dec 2024 [PRIMARY] | — | Flag on the company |
| EU Kleinunternehmer scheme (§19a, special reporting procedure, EU-wide €100,000) | BMF 9 Dec 2024 [PRIMARY] | 1 Jan 2025 | Not modelled |

---

## 6. UStVA (Umsatzsteuer-Voranmeldung)

### 6.1 Filing rules

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Period is the **calendar quarter**; **monthly** if prior-year tax > **€9,000**; Finanzamt may exempt if prior-year tax ≤ **€2,000** | §18 Abs. 2 UStG [PRIMARY] | current | Period selector with a hint from last year's tax |
| Optional monthly filing if the prior-year **refund surplus** > €9,000 (elect by 10 Feb, binding for the year) | §18 Abs. 2a UStG [PRIMARY] | current | Setting |
| New businesses: monthly in the year of start and the following year — but for **2021 to 2026** the actual/expected tax decides instead | §18 Abs. 2 Sätze 4, 6 UStG [PRIMARY] | Suspension ends after 2026 | From 2027 start-ups file monthly again unless extended [UNCERTAIN: extension not checked] |
| Due "bis zum zehnten Tag nach Ablauf jedes Voranmeldungszeitraums"; payment due the same day | §18 Abs. 1 UStG; USt 1 E 2026 lines 53–54 [PRIMARY] | — | Due-date calendar |
| **Dauerfristverlängerung**: +1 month for filing and payment (USt 1 H); monthly filers pay a **Sondervorauszahlung = 1/11** of last year's total (USt 1 H Kz 38), deducted in the December return (**Kz 39**); quarterly filers need no Sondervorauszahlung and the extension need not be renewed yearly | USt 1 H 2026 and USt 5 E 2026 [PRIMARY]; §§46–48 UStDV [PRIMARY, statutory reference] | — | Due date +1 month when set; Kz 39 in the last period |
| **Electronic only**: "nach amtlich vorgeschriebenem Datensatz durch Datenfernübertragung authentifiziert" (needs an ELSTER certificate; paper only on hardship application) | §18 Abs. 1 UStG; BMF 29 Dec 2025 §III; USt 1 E 2026 [PRIMARY] | — | Export figures + (future) ERiC submission |
| **ERiC**: a free C library with interface specification from the tax administration; developers register (checked by the Bayerisches Landesamt für Steuern) and apply for a **Hersteller-ID**; ERiC checks plausibility, encrypts and transmits, and can render a PDF | elster.de developer page [PRIMARY] | — | Desktop app must bundle ERiC (C FFI) per platform; licence/redistribution terms not found [UNCERTAIN]; minimum ERiC version is raised yearly (April) [SECONDARY] |

### 6.2 Kennzahlen (USt 1 A 2026; 2025 identical except where noted)

Amount rule: "Tragen Sie bei den Bemessungsgrundlagen bitte nur Beträge in vollen Euro ein; bei den Umsatzsteuer- und Vorsteuerbeträgen ist dagegen stets auch die Eintragung von Centbeträgen erforderlich." Negative amounts carry a minus sign; foreign currency is converted to euro [PRIMARY, USt 1 E 2026]. How to reach whole euros (truncate or round) is not stated [UNCERTAIN; common practice is to drop cents]. Lines are from the 2026 form; line numbers can change yearly, Kennzahlen are stable.

| Line | Kz (base, whole €) | Kz (tax, € + ct) | Meaning |
|---|---|---|---|
| 10 | 10 | — | Berichtigte Anmeldung (1 = Ja) |
| 11 | 22 | — | Belege beigefügt / gesondert eingereicht (1 = Ja) |
| 12 | 70 (date) | — | Datum des Wechsels von der Kleinunternehmer-Regelung zur Regelbesteuerung |
| 13 | **81** | (computed, no Kz) | Steuerpflichtige Umsätze 19% |
| 14 | **86** | (computed) | Steuerpflichtige Umsätze 7% |
| 15 | **87** | — | Steuerpflichtige Umsätze 0% (PV, §12 Abs. 3) |
| 16 | **35** | **36** | Umsätze zu anderen Steuersätzen (e.g. 16%/5% 2020 payments) |
| 17 | 77 | — | §24 Land- und Forstwirte: Lieferungen an Abnehmer mit USt-IdNr |
| 18 | 76 | 80 | §24: Umsätze mit Steuer nach §24 (Sägewerkserzeugnisse, Getränke, Wein) |
| 19 | **41** | — | Innergemeinschaftliche Lieferungen (§4 Nr. 1 b) an Abnehmer mit USt-IdNr |
| 20 | 44 | — | Innergem. Lieferungen neuer Fahrzeuge an Abnehmer ohne USt-IdNr |
| 21 | 49 | — | Innergem. Lieferungen neuer Fahrzeuge außerhalb eines Unternehmens (§2a) |
| 22 | **43** | — | Weitere steuerfreie Umsätze mit Vorsteuerabzug (Ausfuhrlieferungen, §4 Nr. 2–7, EU-Verordnungen; 2026: also SAFE Reg. (EU) 2025/1106 supplies) |
| 23 | **48** | — | Steuerfreie Umsätze ohne Vorsteuerabzug (§4 Nr. 8–29; §19 Abs. 1) |
| 24 | 91 | — | Steuerfreie innergemeinschaftliche Erwerbe (§§4b, 25c) |
| 25 | **89** | (computed) | Steuerpflichtige innergem. Erwerbe 19% |
| 26 | **93** | (computed) | Steuerpflichtige innergem. Erwerbe 7% |
| 27 | 90 | — | Innergem. Erwerbe 0% |
| 28 | 95 | 98 | Innergem. Erwerbe zu anderen Steuersätzen |
| 29 | 94 | 96 | Innergem. Erwerbe neuer Fahrzeuge von Lieferern ohne USt-IdNr (allgemeiner Steuersatz) |
| 30 | **46** | **47** | §13b Abs. 1: sonstige Leistungen (§3a Abs. 2) eines im übrigen Gemeinschaftsgebiet ansässigen Unternehmers |
| 31 | 73 | 74 | §13b Abs. 2 Nr. 3: Umsätze, die unter das GrEStG fallen |
| 32 | **84** | **85** | §13b Abs. 2 Nr. 1, 2, 4–12: andere Leistungen |
| 33 | 42 | — | Lieferungen des ersten Abnehmers bei innergem. Dreiecksgeschäften (§25b) |
| 34 | **60** | — | Steuerpflichtige Umsätze des leistenden Unternehmers, für die der Empfänger nach §13b Abs. 5 schuldet |
| 35 | **21** | — | Nicht steuerbare sonstige Leistungen gemäß §18b Satz 1 Nr. 2 (EU B2B services; also go in the ZM) |
| 36 | **45** | — | Übrige nicht steuerbare Umsätze (Leistungsort nicht im Inland) |
| 37 | — | (sum) | Umsatzsteuer: Summe der Zeilen 13–18 und 25–32 |
| 38 | — | **66** | Vorsteuer aus Rechnungen anderer Unternehmer (§15 Abs. 1 Nr. 1), aus §27 Abs. 40a-Leistungen [2026 wording] und aus Dreiecksgeschäften (§25b Abs. 5) |
| 39 | — | **61** | Vorsteuer aus innergem. Erwerb (§15 Abs. 1 Nr. 3) |
| 40 | — | **62** | Entstandene Einfuhrumsatzsteuer (§15 Abs. 1 Nr. 2) |
| 41 | — | **67** | Vorsteuer aus Leistungen i. S. d. §13b (§15 Abs. 1 Nr. 4) |
| 42 | — | 63 | Vorsteuer nach Durchschnittssätzen (§23a) |
| 43 | — | 59 | Vorsteuerabzug für innergem. Lieferungen neuer Fahrzeuge (§2a) und von Kleinunternehmern (§15 Abs. 4a) |
| 44 | — | 64 | Berichtigung des Vorsteuerabzugs (§15a) |
| 45 | — | (sum) | Verbleibender Betrag (Zeile 37 − Zeilen 38–44) |
| 46 | — | 65 | Steuer infolge Wechsels der Besteuerungsform; Nachsteuer auf Anzahlungen wegen Steuersatzänderung |
| 47 | — | 69 | §14c-Beträge (unrichtig/unberechtigt ausgewiesen) and tax owed under §6a Abs. 4 S. 2, §17 Abs. 1 S. 7, §25b Abs. 2, §27 Abs. 40a [2026 wording] |
| 48 | — | (sum) | Umsatzsteuer-Vorauszahlung/Überschuss (Zeilen 45–47) |
| 49 | — | **39** | Abzug der festgesetzten Sondervorauszahlung (usually only in the last return of the year) |
| 50 | — | **83** | Verbleibende Vorauszahlung / verbleibender Überschuss (minus sign) — "Bitte in jedem Fall ausfüllen" |
| 51 | 50 | — | §17 Minderung der Bemessungsgrundlage (included in lines 13–18) |
| 52 | — | 37 | §17 Minderung der abziehbaren Vorsteuer (included in lines 38, 42, 43) |
| 53 | 29 | — | Verrechnung des Erstattungsbetrags erwünscht / abgetreten (1 = Ja) |
| 54 | 26 | — | SEPA-Lastschriftmandat für diesen Zeitraum nicht verwenden (1 = Ja) |
| 55 | **500** (2026) / 23 (2025) | — | Ergänzende Angaben zur Steueranmeldung. 2025: Kz 23, "1 = Ja". 2026: Kz 500 with values 1 = steuererhebliche Sachverhalte nicht erklärt, 2 = bewusst abweichende Rechtsauffassung, 3 = personell vertiefte Prüfung, 4 = mehrere Gründe |

Sources: USt 1 A 2025 (BMF 9 Dec 2024, GZ III C 3 – S 7344/19/10001 :006) and USt 1 A 2026 (BMF 29 Dec 2025, GZ III C 3 – S 7344/00039/007/036) [PRIMARY]. The full set of Kennzahlen printed on both forms is identical except Kz 23 → Kz 500 (compared here by extracting all numbers from both PDFs). The other 2026 changes are wording (§27 Abs. 40a VAT-warehouse transition in Kz 66/69; SAFE supplies into Kz 43; §24 average rate 19% for 2026). The 2025 form introduced the Kleinunternehmer handling (Kz 70 date, Kz 48) from the JStG 2024 [PRIMARY]. The ELSTER XML element names (e.g. `Kz81`) and the 2026 data-type version were not retrieved [UNCERTAIN].

Implementation notes: the tax on Kz 81/86/89/93 has no own Kennzahl (ELSTER computes it). Kz 83 = sum − Kz 39 must equal the books' VAT payable/receivable for the period, to the cent.

---

## 7. Bookkeeping, GoBD and retention

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Retention periods**: **10 years** for books, records, inventories, annual financial statements, management reports, opening balance sheets and working instructions (§147 Abs. 1 Nr. 1 and Nr. 4a); **8 years** for Buchungsbelege (Nr. 4); **6 years** for received and sent business letters and other tax-relevant documents (Nr. 2, 3, 5). The period does not expire while the documents matter for an open assessment | §147 Abs. 1, 3 AO [PRIMARY] | 8-year rule since BEG IV (BGBl. 29 Oct 2024), in force 1 Jan 2025, for all Belege whose period had not expired on 1 Jan 2025 [SECONDARY] | Retention metadata per document class; never auto-delete inside the period |
| **Invoices**: "acht Jahre aufzubewahren", starting "mit dem Schluss des Kalenderjahres, in dem die Rechnung ausgestellt worden ist" | §14b Abs. 1 UStG [PRIMARY] | 1 Jan 2025 (was 10) [SECONDARY] | Same |
| Retention start: end of the calendar year of the last entry / creation | §147 Abs. 4 AO [SECONDARY; Abs. 4 not quoted] | — | Compute from year-end |
| **Data access in audits**: (Z1) direct read access to stored data and the DP system; (Z2) taxpayer evaluates data per the authority's instructions (2025 wording adds "oder anschließend einen Nur-Lesezugriff ermöglicht"); (Z3) handover in a machine-readable, machine-evaluable format | §147 Abs. 6 AO [PRIMARY]; GoBD Rz. 166 as amended 14 Jul 2025 [PRIMARY] | — | Read-only auditor role; full export of journal, ledgers, master data and documents (CSV/XML) |
| **GoBD**: BMF 28 Nov 2019 (BStBl I S. 1269), 1st amendment **11 Mar 2024** (BStBl I S. 374), 2nd amendment **14 Jul 2025** (BStBl I S. 1502, applies from 14 Jul 2025) | BMF 14 Jul 2025 [PRIMARY] | 14 Jul 2025 | Follow the GoBD principles below |
| 2025 GoBD changes for e-invoices: keep the **structured part**; the PDF of a hybrid invoice only if it holds additional tax-relevant information (e.g. booking notes); incoming e-invoices are kept "in dem Format …, in dem sie empfangen wurden" (e.g. XML); with an invoicing program no image copy of the outgoing invoice is needed if an identical copy can be produced on demand; the XML must not be lost by format conversion | GoBD Rz. 76, 118, 119, 125, 131 as amended 14 Jul 2025 [PRIMARY] | 14 Jul 2025 | Store received XML byte-for-byte (hash it); outgoing invoices must be reproducible exactly from immutable data |
| The e-invoice's structured part must be kept "unversehrt in seiner ursprünglichen Form"; storing outside a GoBD-compliant system is regularly not, by itself, a §14b breach | UStAE 14b.1 per BMF 15 Oct 2025 [PRIMARY] | 2025 | — |
| GoBD core principles: completeness, correctness, timely recording, order, **immutability** (§146 Abs. 4 AO: an entry may not be changed so its original content can no longer be determined; changes must stay traceable), procedural documentation (**Verfahrensdokumentation**) | §146 Abs. 1, 4 AO and GoBD (2019) [SECONDARY: GoBD Rz. on immutability and Verfahrensdokumentation not re-read here] | — | Append-only journal with reversal entries, audit trail (who/when/what), period lock; ship a Verfahrensdokumentation template (future work) |

---

## 8. Financial year and currency

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Wirtschaftsjahr: Land- und Forstwirte 1 Jul–30 Jun; traders registered in the Handelsregister: the period for which they regularly close their books — moving to a non-calendar year "nur …, wenn sie im Einvernehmen mit dem Finanzamt vorgenommen wird"; other traders: "das Kalenderjahr" | §4a Abs. 1 Satz 2 Nr. 1–3 EStG [PRIMARY] | — | Default calendar year; allow another start month only for a registered company, with a "Finanzamt consent" note |
| The Wirtschaftsjahr covers twelve months; a Rumpfwirtschaftsjahr (short year) only on opening, acquisition, closure, sale, or a change of closing date; changes between calendar and non-calendar years need Finanzamt consent | §8b EStDV [PRIMARY] | — | Support a short first/last year |
| The UStVA/annual VAT period is always the calendar year regardless of the Wirtschaftsjahr | §18 UStG (Kalenderjahr/-monat/-vierteljahr) [PRIMARY] | — | VAT periods independent of the financial year |
| Currency **EUR**, 2 decimals; foreign-currency values are converted to euro for the UStVA | USt 1 E 2026 [PRIMARY]; EN 16931 amounts max 2 decimals [SECONDARY] | — | Base currency EUR, minor unit 2 |

---

## 9. XRechnung (KoSIT) and Leitweg-ID

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| XRechnung is the German **CIUS of EN 16931**, maintained by KoSIT for the IT-Planungsrat; syntaxes **UBL 2.1** (Invoice/CreditNote) and **UN/CEFACT CII D16B** | xeinkauf.de; BMF 15 Oct 2025 ("Standard XRechnung als rein strukturiertes Format") [PRIMARY]; D16B version [SECONDARY] | — | `khata-en16931` writes both syntaxes; UBL first |
| **Current version: 3.0** ("Am 01. Februar 2024 ist die Version 3.0 in Kraft getreten"), latest spec **3.0.2** (20 Jun 2024), latest bundle "3.0.2 Summer 2026 Bugfix" (31 Aug 2026) plus a visualization bugfix (13 Oct 2026). 3.0 stays in force **until at least 31 Jul 2027** (no normative release in summer 2026) | xeinkauf.de *Versionen und Bundles* [PRIMARY] | 1 Feb 2024 → ≥ 31 Jul 2027 | Target 3.0.2 |
| **XRechnung 4.0.0 Vorversion** published 15 Sep 2026; final 4.0 "voraussichtlich im Frühjahr 2027" | xeinkauf.de [PRIMARY] | 2027 (expected) | Watch item; keep the version in one constant |
| **CustomizationID (BT-24)**: `urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0`; Extension: `…xrechnung_3.0#conformant#urn:xeinkauf.de:kosit:extension:xrechnung_3.0`; CVD: `…xrechnung_3.0#compliant#urn:xeinkauf.de:kosit:xrechnung:cvd_0.9` | KoSIT Schematron `ubl/XRechnung-UBL-validation.sch` and `common.sch`, release v2.6.0 (2 Sep 2026), variables `XR-CIUS-ID`, `XR-EXTENSION-ID`, `XR-CVD-ID` [PRIMARY] | 3.0 | Constant per version |
| Validation: KoSIT validator + "validator-configuration-xrechnung" (EN 16931 + XRechnung Schematron); release 3.0.2 dated 2024-06-20 | github.com/itplr-kosit/validator-configuration-xrechnung [SECONDARY] | — | Port or embed the Schematron checks; at minimum run them in CI on generated samples |

**Main BR-DE rules** (texts from the XRechnung UBL Schematron v2.6.0 [PRIMARY]; flag `fatal` unless stated). In XRechnung they apply to every XRechnung; only the Peppol national ruleset words them as "If both supplier and customer are located in Germany" (`xr-rules-list.xml`) [PRIMARY].

| Rule | Requirement |
|---|---|
| BR-DE-1 | Invoice must contain PAYMENT INSTRUCTIONS (BG-16) |
| BR-DE-2 | Group SELLER CONTACT (BG-6) must be present |
| BR-DE-3 / -4 | Seller city (BT-37) / Seller post code (BT-38) |
| BR-DE-5 / -6 / -7 | Seller contact point (BT-41) / telephone (BT-42) / e-mail (BT-43) |
| BR-DE-8 / -9 | Buyer city (BT-52) / Buyer post code (BT-53) |
| BR-DE-10 / -11 | Deliver-to city (BT-77) / post code (BT-78) if BG-15 is present |
| BR-DE-14 | VAT category rate (BT-119) must be present |
| BR-DE-15 | **Buyer reference (BT-10)** must be present (Leitweg-ID for public buyers; any reference otherwise) |
| BR-DE-16 | If VAT codes S, Z, E, AE, K, G, L or M are used: Seller VAT identifier (BT-31) or Seller tax registration identifier (BT-32, the Steuernummer) or SELLER TAX REPRESENTATIVE PARTY (BG-11) |
| BR-DE-17 (warning) | Invoice type code (BT-3) should be 326, 380, 384, 389, 381, 875, 876, 877 |
| BR-DE-18 | Skonto lines in Payment terms (BT-20): `#SKONTO#TAGE=n#PROZENT=n.nn#` with optional `BASISBETRAG=n.nn#`, each line ending with a line break (regex `XR-SKONTO-REGEX`) |
| BR-DE-19 / -20 (warning) | BT-84 / BT-91 should be a valid IBAN for payment means 58 / 59 |
| BR-DE-21 (warning) | Specification identifier (BT-24) should be the XRechnung ID |
| BR-DE-22 | Attachment filenames unique |
| BR-DE-23-a/b, -24-a/b, -25-a/b | Payment means 30/58 → CREDIT TRANSFER (BG-17) only; 48/54/55 → PAYMENT CARD (BG-18) only; 59 → DIRECT DEBIT (BG-19) only |
| BR-DE-26 (warning) | Type code 384 should have a PRECEDING INVOICE REFERENCE (BG-3) |
| BR-DE-27 / -28 (warning) | BT-42 at least three digits; BT-43 one `@`, proper e-mail shape |
| BR-DE-30 / -31 | With DIRECT DEBIT: creditor identifier (BT-90) and debited account (BT-91) |
| BR-DE-TMP-32 (information) | Supply date via BT-72, BG-14 or every BG-26 |

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Leitweg-ID** format: Grobadressierung (2–12 digits: state/Bund 2, Regierungsbezirk 1, Landkreis 2, Gemeinde 3/4/7) – optional Feinadressierung (≤ 30 alphanumerics A–Z case-insensitive, 0–9) – 2-digit Prüfziffer; separator `-` (U+002D); length 5–46 | Leitweg-ID Format-Spezifikation 2.0.2 (28 Jul 2021) §2 [PRIMARY] | — | Field validator |
| Check digit: "gemäß ISO/IEC 7064:2003 per Modulo 97-10": concatenate Grob + Fein without hyphens, letters → A=10 … Z=35, append `00`, `98 − (n mod 97)`; valid if the full number mod 97 = 1. Example `04011000-1234512345-06` (verified here: …500 mod 97 = 92 → 06; …506 mod 97 = 1) | Leitweg-ID spec §2.4 [PRIMARY] | — | Implement and test with the example |
| Leitweg-ID is registered as ISO 6523 ICD **0204** (usable as a Peppol participant ID scheme) | Leitweg-ID spec §1.5 [PRIMARY] | 30 Sep 2019 | Endpoint scheme 0204 for public buyers |

---

## 10. ZUGFeRD / Factur-X (hybrid PDF)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| ZUGFeRD and Factur-X are "technically identical" formats by FeRD (DE) and FNFE-MPE (FR) | FNFE-MPE/FeRD press releases [SECONDARY] | — | One implementation |
| **Versions**: 2.3.3 (7 May 2025); **2.4 / Factur-X 1.08** (4 Dec 2025, to be used from 15 Jan 2026, based on CII D22B, backward compatible with D16B, adds sub-lines); **2.5 / 1.09** (10 Jun 2026; code lists and validation artefacts updated to the latest EN 16931 code lists, new EXTENDED elements; use from 1 Jul 2026 per Projektron); **2.5.2 / 1.09.2** (4 Aug 2026, corrigendum) — the current version. ZUGFeRD 2.4 "is no longer available for download" | ferd-net.de publications list and 2.4 page [PRIMARY for dates]; contents from press-release summaries [SECONDARY]; "use from 1 Jul 2026" [UNCERTAIN] | 2025–2026 | Target the EN 16931 and XRECHNUNG profiles of 2.5.2; avoid EXTENDED-only elements |
| Container: **PDF/A-3** (A-3b minimum; Mustang defaults to A-3u) with the CII XML embedded as an associated file (`/AF`) with **AFRelationship `Alternative`** (`Data` for MINIMUM / BASIC WL) | Mustang `ZUGFeRDExporterFromA3` citing "ZUGFeRD 2.1.1 Technical Supplement Part A §2.2.2" [SECONDARY] | — | Writer must support PDF/A-3 + EmbeddedFiles + AF |
| **Embedded file name**: `factur-x.xml` (ZUGFeRD ≥ 2.1 / Factur-X); `xrechnung.xml` for the XRECHNUNG profile; `zugferd-invoice.xml` for ZUGFeRD 2.0 (ZUGFeRD 1: `ZUGFeRD-invoice.xml`) | Mustang `getFilenameForVersion` [SECONDARY] | — | Use `factur-x.xml` / `xrechnung.xml` |
| **XMP**: namespace `urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#`, prefix `fx`; properties `DocumentType` = `INVOICE`, `DocumentFileName`, `Version` = `1.0`, `ConformanceLevel` ∈ {`MINIMUM`, `BASIC WL`, `BASIC`, `EN 16931`, `EXTENDED`, `XRECHNUNG`}; plus the PDF/A extension-schema description of these properties | Mustang `XMPSchemaZugferd`, `Profile.getXMPName` [SECONDARY]; FeRD spec not read [UNCERTAIN on exact extension-schema block] | — | Write the fx block and the `pdfaExtension:schemas` description |
| **Guideline IDs (BT-24 / `GuidelineSpecifiedDocumentContextParameter`)**: MINIMUM `urn:factur-x.eu:1p0:minimum`; BASIC WL `urn:factur-x.eu:1p0:basicwl`; BASIC `urn:cen.eu:en16931:2017#compliant#urn:factur-x.eu:1p0:basic`; EN 16931 (formerly COMFORT) `urn:cen.eu:en16931:2017`; EXTENDED `urn:cen.eu:en16931:2017#conformant#urn:factur-x.eu:1p0:extended`; XRECHNUNG `urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0`; (France: EXTENDED-CTC-FR `urn:cen.eu:en16931:2017#conformant#urn.cpro.gouv.fr:1p0:extended-ctc-fr`) | Mustang `Profiles.java` [SECONDARY]; the XRECHNUNG value matches the KoSIT Schematron [PRIMARY] | — | Constants; only BASIC, EN 16931, EXTENDED, XRECHNUNG count as German e-invoices (§4) |

**Pure-Rust PDF/A-3 options (next step).** `krilla` (0.8.2 on docs.rs) has PDF/A-3a/3b/3u validators (`Validator::A3_A/A3_B/A3_U`), allows embedded files under PDF/A-3 (forbidden under A-2), and has `EmbeddedFile { path, mime_type, description, association_kind, data, … }` with `AssociationKind::{Source, Data, Alternative, Supplement, Unspecified}` (→ AFRelationship). Its `Metadata` writes standard XMP via `xmp-writer` but has no API for custom XMP namespaces ("temporary until custom fields can be written to XMP"), so the Factur-X `fx:` block and extension schema would need a patch or a post-processing step [read in krilla source, `interchange/embed.rs`, `interchange/metadata.rs`, `configure/validate.rs`; SECONDARY/UNCERTAIN]. Alternatives: `lopdf` to post-process a krilla/printpdf PDF (add EmbeddedFiles, `/AF`, replace the XMP stream) [UNCERTAIN, not tested]. Validate output with veraPDF and the Mustang/FeRD validator.

---

## 11. Peppol BIS Billing 3.0

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| CustomizationID `urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0`; ProfileID `urn:fdc:peppol.eu:2017:poacc:billing:01:1.0`; current doc release 3.0.21 | docs.peppol.eu/poacc/billing/3.0/bis/ [PRIMARY] | — | Constants in `khata-en16931` |
| Germany: XRechnung can be sent over Peppol; the Leitweg-ID scheme (ICD 0204) works as a participant ID; the Peppol BIS German national ruleset mirrors BR-DE-1…31 as DE-R-0xx, conditioned on seller and buyer being in Germany | Leitweg-ID spec §1.3–1.5 [PRIMARY]; KoSIT `xr-rules-list.xml` [PRIMARY]; KoSIT as German Peppol Authority, federal portals accepting Peppol [SECONDARY] | — | Peppol transport (via an access point) is future work; a desktop app cannot be an access point itself [UNCERTAIN] |

---

## 12. Sources

- UStG on gesetze-im-internet.de: §12 <https://www.gesetze-im-internet.de/ustg_1980/__12.html>, §13b <https://www.gesetze-im-internet.de/ustg_1980/__13b.html>, §14 <https://www.gesetze-im-internet.de/ustg_1980/__14.html>, §14a <https://www.gesetze-im-internet.de/ustg_1980/__14a.html>, §14b <https://www.gesetze-im-internet.de/ustg_1980/__14b.html>, §18 <https://www.gesetze-im-internet.de/ustg_1980/__18.html>, §19 <https://www.gesetze-im-internet.de/ustg_1980/__19.html>, §27 <https://www.gesetze-im-internet.de/ustg_1980/__27.html>, §28 <https://www.gesetze-im-internet.de/ustg_1980/__28.html>
- UStDV §33 <https://www.gesetze-im-internet.de/ustdv_1980/__33.html>, §34a <https://www.gesetze-im-internet.de/ustdv_1980/__34a.html>
- AO §147 <https://www.gesetze-im-internet.de/ao_1977/__147.html>; EStG §4a <https://www.gesetze-im-internet.de/estg/__4a.html>; EStDV §8b <https://www.gesetze-im-internet.de/estdv_1955/__8b.html>
- ERechV §3 <https://www.gesetze-im-internet.de/erechv/__3.html>, §4 <https://www.gesetze-im-internet.de/erechv/__4.html>, §5 <https://www.gesetze-im-internet.de/erechv/__5.html>, §11 <https://www.gesetze-im-internet.de/erechv/__11.html>
- BMF 15 Oct 2025 *Einführung der obligatorischen E-Rechnung* (amends BMF 15 Oct 2024, BStBl I S. 1320): <https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Umsatzsteuer/Umsatzsteuer-Anwendungserlass/2025-10-15-einfuehrung-obligatorische-e-rechnung.pdf> (read via web.archive.org); BMF FAQ: <https://www.bundesfinanzministerium.de/Content/DE/FAQ/e-rechnung.html>
- BMF 29 Dec 2025 *Vordruckmuster USt-Voranmeldung 2026*: <https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Umsatzsteuer/2025-12-29-vordruckmuster-USt-voranmeldung-2026.pdf> (read via web.archive.org)
- BMF 9 Dec 2024 *Vordruckmuster USt-Voranmeldung 2025* (copy, archived): <https://web.archive.org/web/20250601000000/https://www.collmex.de/ust_voranmeldung.pdf>
- BMF 14 Jul 2025 *GoBD, 2. Änderung*: <https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Weitere_Steuerthemen/Abgabenordnung/2025-07-14-GoBD-2-aenderung.pdf> (read via web.archive.org)
- ELSTER *Prüfung der Steuer- und Steueridentifikationsnummer* (Stand 02.09.2026): <https://download.elster.de/download/schnittstellen/Pruefung_der_Steuer_und_Steueridentifikatsnummer.pdf>; ELSTER developer page: <https://www.elster.de/eportal/infoseite/entwickler>
- KoSIT / xeinkauf.de versions: <https://xeinkauf.de/xrechnung/versionen-und-bundles/>; XRechnung Schematron v2.6.0: <https://github.com/itplr-kosit/xrechnung-schematron> (`src/validation/schematron/ubl/XRechnung-UBL-validation.sch`, `common.sch`, `src/tools/xr2nrs/xr-rules-list.xml`); validator configuration: <https://github.com/itplr-kosit/validator-configuration-xrechnung>
- Leitweg-ID Format-Spezifikation 2.0.2: <https://e-rechnung.service.rlp.de/fileadmin/zre-upload.rlp.de/allgemeine_Dokumente/Leitweg-ID-Formatspezifikation-v2-0-2.pdf>
- FeRD publications: <https://www.ferd-net.de/en/downloads/publications>; ZUGFeRD 2.4 page: <https://www.ferd-net.de/en/downloads/publications/details/zugferd-24-english>; FNFE-MPE press releases (4 Dec 2025, 10 Jun 2026): <https://fnfe-mpe.org/factur-x/factur-x_en/>
- Peppol BIS Billing 3.0: <https://docs.peppol.eu/poacc/billing/3.0/bis/>
- Secondary: python-stdnum `de/vat.py`, `de/stnr.py`, `iso7064/mod_11_10.py`, `tests/test_eu_vat.doctest` <https://github.com/arthurdejong/python-stdnum>; Wikipedia de *Umsatzsteuer-Identifikationsnummer* <https://de.wikipedia.org/wiki/Umsatzsteuer-Identifikationsnummer>, *Steuernummer* <https://de.wikipedia.org/wiki/Steuernummer>; Wikipedia *ISO 3166-2:DE* <https://en.wikipedia.org/wiki/ISO_3166-2:DE>; Mustang project <https://github.com/ZUGFeRD/mustangproject> (`Profiles.java`, `ZUGFeRDExporterFromA3.java`, `XMPSchemaZugferd.java`); krilla <https://docs.rs/krilla/latest/krilla/>, <https://github.com/LaurenzV/krilla>; Ebner Stolz on UStVA 2026 Kz 500 <https://www.ebnerstolz.de/de/unser-angebot/leistungen/steuerberatung/umsatzsteuer/vordruckmuster-umsatzsteuervoranmeldung-2026-100714.html>; BEG IV retention (Ecovis, Lexware) <https://ecovis-kso.com/blog/aufbewahrungsfristen-2025-archivierungspflicht-fuer-geschaeftsunterlagen/>; Steueränderungsgesetz 2025 gastronomy (IWW) <https://www.iww.de/gstb/steuerrecht-aktuell/restaurant-und-verpflegungsdienstleistungen-weihnachtsgeschenk-fuer-die-gastronomie-gastrosteuer-von-7-ust-jetzt-amtlich-f171879>; eVatR REST (rechtlogisch/evatr) <https://root.packagist.org/packages/rechtlogisch/evatr>; W-IdNr (Buhl) <https://www.buhl.de/steuer/?p=68999>

---

## 13. Open questions

1. Whether the BZSt (or any official text) states the USt-IdNr check-digit algorithm and the "no leading 0" rule; only secondary sources and ELSTER's identical IdNr algorithm were found (§1.1–1.2).
2. Whole-euro conversion of UStVA bases: truncate or round? The form only says "volle Euro" (§6.2).
3. ELSTER UStVA XML data-type version for 2026 and the ERiC licence/redistribution terms for bundling in an open-source desktop app (§6.1).
4. Whether the 2021–2026 suspension of monthly filing for start-ups (§18 Abs. 2 Satz 6) is extended beyond 2026.
5. Exact Factur-X XMP extension-schema block and AFRelationship rules in the 2.5.2 spec (only the Mustang reference implementation was read); whether "use from 1 Jul 2026" for 2.5 is official (§10).
6. Whether krilla will accept custom XMP namespaces, or whether a lopdf post-processing step is needed (§10).
7. The pre-2025 Kleinunternehmer limits (€22,000 / €50,000) and the BEG IV effective date / transition were only confirmed from secondary sources (§5, §7).
8. Länder rules for B2G e-invoicing (Leitweg-ID use, thresholds) beyond the federal ERechV (§4).
9. The wording of the Kleinunternehmer invoice note (not prescribed by §34a UStDV) and whether gap-free numbering is required (only "einmalig vergeben") (§3).
10. Whether XRechnung 4.0 (expected spring 2027) changes the CustomizationID and BR-DE rules; plan a version switch before mid-2027 (§9).

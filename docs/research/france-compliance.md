# France Compliance Requirements (TVA, facturation électronique, CA3): KhataOS Spec

> **Status as of:** 2026-10-21. **Scope:** French businesses (entrepreneurs individuels, micro-entrepreneurs, sociétés) keeping books in KhataOS, a local-first desktop accounting app. The planned `khata-fr` pack should implement the VAT and identity items below. The structured-invoice items (UBL, CII, Factur-X) belong to the shared EU crate `khata-en16931` (see `docs/en16931.md`). Transmission through a *plateforme agréée* (PA) is future work, because a desktop app cannot be a PA itself (§4).
> **Not legal advice.** This file turns the CGI, BOFiP, the DGFiP forms and notices, the DGFiP e-invoicing external specifications and service-public pages into software requirements. Get a French expert-comptable to confirm each item before release.
>
> **Confidence tags:**
> - **[PRIMARY]** means read in a primary text: bofip.impots.gouv.fr, an impots.gouv.fr form, notice, FAQ, fact sheet or the *spécifications externes* v3.2, or entreprendre.service-public.gouv.fr (DILA, the official administrative information site).
> - **[SECONDARY]** means reported consistently by reputable non-official sources (python-stdnum, Wikipedia, professional and press sites), but the official text was not retrieved in this research.
> - **[UNCERTAIN]** means sources conflict, the item is interpretive, or the point was not found. Flagged for review.
>
> legifrance.gouv.fr and economie.gouv.fr returned HTTP 403 to every request in this research, and web.archive.org could not be reached. CGI articles are therefore cited through BOFiP, the forms and service-public. No article text was read on Légifrance.

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **N° TVA intracommunautaire**: `FR` + 2-character key + 9-digit SIREN. Numeric key = `(12 + 3 × (SIREN mod 97)) mod 97`. Accept the newer alphanumeric keys (alphabet without I and O) and Monaco numbers (`FR` key `000` …) | service-public F23570 for the structure (§1.1); python-stdnum for the algorithm (§1.2) |
| 2 | **SIREN** (9 digits, Luhn) and **SIRET** = SIREN + 5-digit NIC (14 digits, Luhn over all 14). La Poste exception: digit sum divisible by 5 | INSEE (9th digit is a check digit); Wikipedia and python-stdnum for Luhn and La Poste (§1.3) |
| 3 | **Rates** from an effective-dated master: 20% / 10% / 5.5% / 2.1% in mainland France (since 1 Jan 2014); Corse 13% / 10% / 2.1% / 0.9%; Guadeloupe, Martinique, Réunion 8.5% / 2.1% / 1.75% / 1.05%; Guyane and Mayotte outside VAT | BOFiP BOI-TVA-LIQ, BOI-TVA-GEO-10-10, BOI-TVA-GEO-20-10; CA3 2026 lines (§2) |
| 4 | **Invoice**: one number per invoice, from a chronological and continuous sequence. Separate series are allowed when justified. Mandatory mentions, including the four new e-invoicing mentions (customer SIREN, operation category, option "débits", delivery address). Fixed notes: "Autoliquidation", "Autofacturation", "TVA non applicable, art. 293 B du CGI" | BOI-TVA-DECLA-30-20-20-10; DGFiP FAQ (version 01/09/2026); service-public F31808 (§3) |
| 5 | **Facturation électronique**: every business must be able to receive e-invoices from 1 Sep 2026. Large companies (GE) and ETI issue from 1 Sep 2026; PME and micro-enterprises from 1 Sep 2027. Invoices pass through a PA; the PPF runs the *annuaire* and collects the data. Formats are UBL 2.1, CII D22B and Factur-X | DGFiP FAQ and fiche 3 (Oct 2026); *spécifications externes* v3.2 (30 Apr 2026) (§4) |
| 6 | **E-reporting** of B2C and international transactions and of payment data (VAT on receipts). Same schedule as e-invoicing; frequency set by the VAT regime | DGFiP FAQ; fiche 6 (§4.3) |
| 7 | **Franchise en base (art. 293 B)**: thresholds in 2026 are unchanged at €85,000 / €93,500 (goods, lodging) and €37,500 / €41,250 (services). The single €25,000 threshold from the loi de finances 2025 was abandoned | service-public F21746, verified 1 Jan 2026 (§5) |
| 8 | **CA3 (3310-CA3-SD, cerfa 10963\*31, millésime 2026)** from the books, using the official line codes. Amounts rounded to the nearest euro; never negative; monthly filing, quarterly if annual VAT < €4,000. Online only (EFI/EDI) | Form and notice 3310-NOT-CA3-SD (50449#29) (§6) |
| 9 | **Exercice comptable**: usually 12 months with a free closing date (31 Dec for micro-entrepreneurs); currency EUR, 2 decimals. Regions: 13 metropolitan (incl. Corse `FR-20R`) + 5 overseas (`FR-971`…`FR-976`) | service-public / justice.fr fiche; Wikipedia ISO 3166-2:FR (§8) |
| 10 | **FEC** export (18 fields, art. A47 A-1 LPF) and **cash-register certification** (art. 286 I 3° bis CGI, NF525): pointers only | BOFiP BOI-CF-IOR-60-40-20; BOFiP actualité 25 Mar 2026 (§7) |

---

## 1. Identity numbers

### 1.1 Numéro de TVA intracommunautaire

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| "En France, le numéro est constitué du code FR, d'une clé informatique (2 chiffres) et du numéro Siren de l'entreprise (9 chiffres)" | entreprendre.service-public.gouv.fr F23570 (vérifié le 3 Mar 2025) [PRIMARY] | — | Store `FR` + key + SIREN (13 characters). Accept input with or without `FR`, lower case, and spaces, `-` or `.` separators |
| A registered business receives the number automatically from its SIE. Businesses not liable for VAT (e.g. under franchise) request it through the impots.gouv.fr professional messaging | F23570 [PRIMARY] | — | Optional field on the company; not derivable for non-registered businesses |
| Older numbers have a numeric key. "With new style numbers at least one is a alphabetic"; the letters I and O are never used | python-stdnum `fr/tva.py` docstring [SECONDARY]. No official page describing alphanumeric keys was found [UNCERTAIN on when and to whom they are issued] | — | Accept both forms (§1.2) |
| Numbers whose body starts `000` (positions 3–5) are Monaco numbers. They are valid TVA numbers but not SIRENs | python-stdnum `validate()` comment "numbers from Monaco are valid TVA but not SIREN" [SECONDARY] | — | Skip the SIREN check for `FRkk000xxxxxx`; do not derive a SIREN |
| The CA3 header has the field "N° de TVA intracommunautaire (ne concerne pas les DOM)" | 3310-CA3-SD 2026 [PRIMARY, layout] | — | Do not require an intra-EU VAT number for DOM-only businesses |
| A passing key shows only that the number is well formed. Whether it is registered is checked in **VIES** | CA3 notice §II ("en consultant le site internet : http://ec.europa.eu/taxation_customs/vies/") [PRIMARY] | — | Optional online check (future work) |

### 1.2 Key algorithm (as implemented)

Normalise first: remove spaces, `-` and `.`, upper-case the string, then strip a leading `FR`. What remains must be 11 characters `k1 k2 d1…d9`. Both key characters must come from the alphabet `0123456789ABCDEFGHJKLMNPQRSTUVWXYZ` (34 characters, no I and no O), and `d1…d9` must be digits. Then:

1. **SIREN check:** if `d1 d2 d3 ≠ 000`, then `d1…d9` must pass the SIREN Luhn check (§1.3).
2. **Numeric key** (both key characters are digits): valid when `int(k1k2) == (12 + 3 × (SIREN mod 97)) mod 97`.
   - The same value is `int(SIREN ‖ "12") mod 97`, because 100 ≡ 3 (mod 97). This is how python-stdnum writes it: `int(number[2:] + '12') % 97`. The two forms were checked here to agree on 200,000 random 9-digit numbers.
   - The key is always 0–96, printed with 2 digits (`%02d`).
3. **Alphanumeric key** (at least one key character is a letter), per python-stdnum `fr/tva.py` [SECONDARY; no official text found, so UNCERTAIN]:
   - If `k1` is a digit: `check = idx(k1) × 24 + idx(k2) − 10`.
   - Otherwise: `check = idx(k1) × 34 + idx(k2) − 100`.
   - Here `idx` is the position in the 34-character alphabet above.
   - Valid when `(int(d1…d9) + 1 + check div 11) mod 11 == check mod 11`.
   - There is no "compute the key" direction for this form. Only validate it.

Sources: numeric formula from python-stdnum `fr/tva.py` and `fr/siren.py` (`to_tva`: `'%02d' % (int(siren + '12') % 97)`) [SECONDARY]. The `(12 + 3 × (SIREN mod 97)) mod 97` form appears consistently on French accounting sites (e.g. lafabriquedunet.fr, copeps.fr) [SECONDARY]. No DGFiP or INSEE page that states the formula was found [UNCERTAIN whether it is published officially]. Some secondary pages are wrong: one gives SIREN 123 456 789 → key 23. In fact 123456789 mod 97 = 39, so the key is 32, and 123456789 fails the SIREN Luhn check anyway (computed here).

**Test vectors** (computed here with the algorithm above, Python `-I`):

| Number | Source | Result |
|---|---|---|
| FR40303265045 (`Fr 40 303 265 045`) | python-stdnum `fr/tva.py` docstring | valid (SIREN 303265045 → key 40) |
| FR41303265045 | key changed by +1 | invalid |
| 23334175221 | python-stdnum docstring | valid |
| `84 323 140 391` | python-stdnum docstring (negative) | invalid (the SIREN fails Luhn) |
| FR84323140392 | python-stdnum `tests/test_eu_vat.doctest` | valid |
| K7399859412 | python-stdnum docstring ("new-style number") | valid |
| 4Z123456782 | python-stdnum docstring ("new-style number starting with digit") | valid |
| IO334175221 | python-stdnum docstring (letters I/O not allowed) | invalid (format) |
| FR83404833048 | SIREN 404833048 (python-stdnum `siren.py`) | valid (key 83) |
| FR46443121975 | python-stdnum `siren.to_tva('443 121 975')` → `46 443 121 975` | valid |
| FR44732829320 | SIREN of SIRET 73282932000074 (`siret.to_tva` → `44 732 829 320`) | valid |
| FR19552008443 | SIREN 552008443 (python-stdnum); also in the doctest | valid |
| FR32000000330, FR87000018746, FR96000110899 | doctest (Monaco, body starts `000`) | valid (no SIREN check) |
| All 72 FR numbers in `test_eu_vat.doctest` (lines 276–349) | python-stdnum | all valid |

### 1.3 SIREN and SIRET

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| SIREN: 9 digits. "Les huit premiers chiffres n'ont aucune signification. Le neuvième chiffre est un chiffre de contrôle de validité du numéro." The number is never reassigned | INSEE definition c2047 [PRIMARY] | — | 9 digits; check digit per the next row |
| The check is a "clé « 1-2 », suivant l'algorithme de Luhn": counting from the right, digits in odd positions are multiplied by 1 and digits in even positions by 2. The sum of the digits of all products must be a multiple of 10 | Wikipedia fr *Système d'identification du répertoire des entreprises*, citing INSEE *Identifiants économiques : siren, nic et siret* [SECONDARY]; python-stdnum `fr/siren.py` uses `luhn.validate` [SECONDARY] | — | Standard Luhn over 9 digits |
| SIRET = SIREN (9) + NIC (5) = 14 digits. Luhn over all 14 digits ("le cinquième [chiffre du NIC] est une clé de contrôle calculée modulo 10 sur le numéro complet") | Search-result summary of an INSEE document; python-stdnum `fr/siret.py` [SECONDARY] | — | Validate the 14-digit Luhn **and** the 9-digit SIREN Luhn |
| **La Poste exception**: SIRETs starting `356000000` other than the head office `35600000000048` "do not use the Luhn checksum but the sum of digits must be a multiple of 5" | python-stdnum `fr/siret.py` [SECONDARY; no INSEE text found, so UNCERTAIN] | — | Special case exactly as stated |
| A check-digit pass does not prove the business exists or is still active. The official lookup is the INSEE Sirene API / *Annuaire des Entreprises* | INSEE-derived text quoted on third-party sites [SECONDARY] | — | Optional online check (future work) |
| Presentation: SIREN `NNN NNN NNN`; SIRET `NNN NNN NNN NNNNN` | python-stdnum `format()` [SECONDARY] | — | Display format |
| The CA3 header asks for the "N° d'identification de l'établissement (SIRET)" | 3310-CA3-SD 2026 [PRIMARY] | — | Company record needs SIREN and head-office NIC |

**Test vectors** (computed here, Python `-I`):

| Number | Source | Result |
|---|---|---|
| 404833048 / 404833047 | python-stdnum `siren.py` docstring | valid / invalid |
| 552008443, 443121975, 303265045 | python-stdnum | valid |
| 732829320 | Wikipedia fr worked example (digit sum 40) | valid |
| 552100554 | Wikipedia fr (Peugeot SA) | valid |
| 123456789 | used as an "example" on some sites | **invalid** |
| 73282932000074 / 73282932000079 | python-stdnum `siret.py` docstring | valid / invalid |
| 40483304800022 | INSEE-derived example "404 833 048 00022" | valid |
| 35600000000048 | La Poste head office (Luhn) | valid |
| 35600000049837 / 35600000049838 | constructed here (digit sums 45 / 46) | valid / invalid under the La Poste rule |

### 1.4 E-invoicing addressing (SIREN-based)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| The PPF runs a central **annuaire** of every VAT-registered entity in scope. "Chaque entité est répertoriée avec un identifiant unique (SIREN ou SIRET)". The issuer's PA uses it to find the customer's receiving PA. It is updated daily by the administration and the PAs | DGFiP fiche 1 / fiche 6 glossary [PRIMARY] | 1 Sep 2026 | Customer master needs SIREN (and optionally SIRET) for French B2B customers |
| Addressing levels (*mailles*): unité légale (SIREN), établissement (SIRET), code routage, suffixe. At initialisation the PPF creates a default line per private company at SIREN level, with a placeholder ("fictive") platform. Public bodies get SIRET and code-routage lines with Chorus Pro as the receiving platform. Receiving PAs (PAR) update the lines | *Spécifications externes* v3.2, Dossier général §3.x (30 Apr 2026) [PRIMARY] | 2026 | Store an optional routing code / suffix per customer address |
| In the structured invoice, the SIREN scheme identifier is `0002` ("Valeur = 0002 pour un SIREN"); `0231` is used for the SIREN of an *assujetti unique*. Electronic addresses must use the CEF EAS code list; the French FRCTC electronic address (suffix) is `0225`, with characters limited to digits, unaccented Latin letters and `- _ .` (rule G1.115) | Annexe 1 (Flux 1, v1.2) and Annexe 7 (Règles de gestion V1.9), spec v3.2 [PRIMARY] | 2026 | Emit `0002` for the buyer's legal ID, and the EAS scheme from the annuaire for the endpoint. The SIRET scheme code (`0009` in ISO 6523) was not confirmed in the annex [UNCERTAIN] |

---

## 2. Rates and territories

### 2.1 Rates

| Class (proposed code) | Rate | Scope (summary) | Source | Effective date |
|---|---|---|---|---|
| `FR_STANDARD` | **20%** | "Il est fixé à 20 % pour la France métropolitaine et la Corse" (CGI art. 278); "applicable à toutes les opérations pour lesquelles aucun autre taux n'est prévu" | BOI-TVA-LIQ-20 [PRIMARY]; CA3 line 08 [PRIMARY] | 1 Jan 2014 [SECONDARY: Assemblée nationale written question 50405 and press]. **19.6%** before that (from 1 Apr 2000 [SECONDARY/UNCERTAIN on the start date]) |
| `FR_INTERMEDIATE` | **10%** | "La généralité des autres biens et services soumis au taux réduit est taxée au taux de 10 %" (e.g. passenger transport, lodging, restaurants, renovation work) | BOI-TVA-LIQ-30 (23 Aug 2023) [PRIMARY]; CA3 line 9B [PRIMARY]. Legal basis art. 278 bis to 279 bis CGI [SECONDARY, Légifrance not read] | 1 Jan 2014; it replaced **7%**, which applied 1 Jan 2012–31 Dec 2013 [SECONDARY: AN questions 50405 and 35193] |
| `FR_REDUCED` | **5.5%** | "restreint aux seuls biens et services listés à l'article 278-0 bis" (most foodstuffs, disability equipment, some energy-saving work, etc.) | BOI-TVA-LIQ-30 [PRIMARY]; CA3 line 09 [PRIMARY] | Kept at 5.5% on 1 Jan 2014 (the planned cut to 5% was dropped) [SECONDARY] |
| `FR_PARTICULAR` | **2.1%** | "le seul taux particulier en vigueur" in mainland France: reimbursed medicines and blood products (art. 281 octies), first performances of certain shows (art. 281 quater); press (art. 298 septies) | BOI-TVA-LIQ-40 (28 Jun 2023) [PRIMARY]; CA3 line **T6** "Opérations réalisées en France continentale au taux de 2,1 %" [PRIMARY] | — |
| `CORSE_13` | **13%** | Petroleum products delivered in Corse | BOI-TVA-GEO-10-10 (20 Nov 2024), art. 297 CGI [PRIMARY]; CA3 line TC [PRIMARY] | — |
| `CORSE_10` | **10%** | Building works, some agricultural equipment, building materials, low-voltage electricity, alcoholic drinks consumed on the premises | same [PRIMARY]; CA3 T3 | — |
| `CORSE_2_1` | **2.1%** | Most food, books, hotel accommodation, school meals, water, some shows, etc. | same [PRIMARY]; CA3 T4 | — |
| `CORSE_0_9` | **0.9%** | First 140 performances of certain new shows; live animals for butchery sold to non-registered buyers | same [PRIMARY]; CA3 T5 | — |
| Corse, other supplies | 20% / 10% / 5.5% | Supplies without a special Corse rate use the mainland rates (BOI-TVA-LIQ-20 covers "la Corse" for 20%) | [PRIMARY for 20%; UNCERTAIN how the 5.5% and 10% items not listed in art. 297 are treated] | — |
| `DOM_NORMAL` | **8.5%** | Guadeloupe, Martinique, La Réunion: "taux normal : 8,5 %" (art. 296, 1° b) | BOI-TVA-GEO-20-10 (5 Jun 2019) [PRIMARY]; CA3 line 10 | — |
| `DOM_REDUCED` | **2.1%** | "taux réduit : 2,1 %" (art. 296, 1° a), same scope as mainland reduced rates | same [PRIMARY]; CA3 line 11 | — |
| `DOM_1_75` | **1.75%** | Live animals for butchery sold to non-registered buyers (art. 296 bis b) | same [PRIMARY]; CA3 T1 | — |
| `DOM_1_05` | **1.05%** | First 140 performances of certain shows; press (art. 296 bis a) | same [PRIMARY]; CA3 T2 | — |
| Guyane, Mayotte | none | "la TVA n'est provisoirement pas applicable dans les collectivités de Guyane et de Mayotte" (art. 294-1). There is nothing to declare or pay for operations located there | BOI-TVA-GEO-20 [PRIMARY] | — |
| `EXEMPT_*` | 0 | Exports (E1), intra-EU supplies (F2), other non-taxable operations (E2), etc. These are not a rate; see §6 | CA3 [PRIMARY] | — |

Notes:
- The PPF accepts only these VAT rates in e-invoice data (rule G1.24): `0, 10, 13, 20, 8.5, 19.6, 2.1, 5.5, 7, 20.6, 1.05, 0.9, 1.75, 9.2, 9.6`. They are written as percentages (e.g. `20`, never `0.20`, no `%` sign) [PRIMARY, Annexe 7 V1.9]. The meaning of 9.2 and 9.6 was not researched [UNCERTAIN]. A rate master must not produce any other value on an e-invoice.
- The CA3 has a line 13 "Anciens taux" for operations still taxable at former rates, e.g. receipts in 2014 for supplies made before 2014 [PRIMARY for the line; its use is interpretive].
- Corse conversion coefficients for tax-inclusive takings: 0.90% → 0.991; 2.10% → 0.979; 10% → 0.909; 13% → 0.884 [PRIMARY, BOI-TVA-GEO-10]. They are not needed if VAT is computed per line.

### 2.2 Territorial points that affect the software

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Guadeloupe, Martinique and Réunion use the same CA3 as the mainland, on lines 10, 11, T1, T2. A business with operations in both mainland France and the DOM may file one combined return in one place; this choice is irrevocable | CA3 notice IV.2 [PRIMARY] | — | Pick the rate set from the *place of supply*, not the company's head office |
| Guyane, Mayotte and Réunion are export territories relative to each other and to Guadeloupe/Martinique. Guadeloupe ↔ Martinique trade is not an export: it is taxed at 2.1% or 8.5%. For the DOM, sales to mainland France count as exports (line E1); goods entering mainland France from a DOM count as imports (line A4) | CA3 notice (lines A4, E1, IV.2) [PRIMARY] | — | Treat a DOM ↔ mainland movement like a third-country import/export |
| A sale shipped from Corse ↔ mainland France is a domestic sale; Corse is inside the EU VAT territory | inferred from BOI-TVA-GEO-10 (Corse has special rates only) [UNCERTAIN, not stated explicitly in what was read] | — | No export treatment for Corse |

### 2.3 Rate changes 2025–2026

No change to the rates above was found for 2025 or 2026. The CA3 2026 prints the same rate lines as the 2025 form; the full set of box codes is identical (compared here by extracting every code from both PDFs) [PRIMARY]. The changes in 2025–2026 concern the franchise thresholds (§5), e-invoicing (§4) and energy excise lines on the CA3 (§6).

---

## 3. Invoices (art. 289 and 242 nonies A annexe II CGI; Code de commerce L441-9)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Number**: "la facture doit comporter un numéro unique basé sur une séquence chronologique et continue" | BOI-TVA-DECLA-30-20-20-10 §70 (in force since 18 Oct 2013) [PRIMARY] | — | Gap-free counter per series, assigned at issue time, never reused |
| **Series**: numbering may run "par séries distinctes lorsque les conditions d'exercice de l'activité de l'assujetti le justifient". Each series must be chronological and continuous, assigned as invoices are issued, and "deux factures émises la même année ne puissent pas porter le même numéro". A distinct prefix per series is recommended. Valid reasons include several invoicing sites, several customer categories or several issuing channels; customers in several countries are not, in principle, a reason for a series per customer. A mandatary keeps a separate sequence per principal | BOI-TVA-DECLA-30-20-20-10 §80–130 [PRIMARY] | — | Series = prefix + counter. Uniqueness is required within a calendar year; a yearly reset is neither required nor forbidden (yearly reset allowed only if the year is part of the number [interpretive, UNCERTAIN]) |
| **E-invoice uniqueness rule**: the invoice identifier = invoice number (**max 35 characters**, TT-19) + year of the issue date (TT-20) + supplier SIREN (TT-33). It "doit être unique"; a duplicate "fera l'objet d'un rejet par les plateformes. Le contrôle d'unicité est systématiquement bloquant" | Annexe 7 rule G1.42, spec v3.2 [PRIMARY] | 1 Sep 2026 | Cap invoice numbers at 35 characters; never reuse a number within a year for the same SIREN |
| **Date**: issue date; for e-invoices "celle de sa délivrance ou de son émission". Date of the supply only if it "diffère de la date de délivrance ou d'émission" | BOI §140, §160 [PRIMARY] | — | `issue_date`, optional `supply_date` |
| **Parties**: name and address of supplier and customer; supplier VAT number (and the fiscal representative's, if any) | BOI §10–50 [PRIMARY] | — | Required fields |
| Customer VAT number "of the business client if it is liable for VAT". The VAT number is not required on invoices of €150 HT or less | service-public F31808 (vérifié le 11 Aug 2026) [PRIMARY]; whether the €150 relief covers the supplier's number, the customer's, or both is not clear from the page [UNCERTAIN] | — | Require the customer VAT number on intra-EU B2B and reverse-charge invoices; warn otherwise |
| **Seller identity (Code de commerce)**: entrepreneur individuel: name + "Entrepreneur individuel" or "EI", address, SIREN. Company: name, SIREN, registered office, legal form, share capital. A company still being registered prints "Siret en cours d'attribution" | F31808 [PRIMARY] | — | Company profile fields; print them on every invoice |
| **Lines**: quantity, precise description, unit price excl. VAT, discounts per line (or in the footer if only known at the end), rate per line or once per rate group, total excl. VAT per rate and the tax for that rate, total tax payable | BOI §180–300, §480 [PRIMARY] | — | Invoice model and VAT summary per rate |
| **Exemption**: a reference to the exempting provision, or failing that an unambiguous statement | BOI §490–500 [PRIMARY] | — | Per exemption class, a configurable note. Usual wordings such as "Exonération de TVA, article 262 ter I du CGI" (intra-EU supply) and "article 262 I du CGI" (export) [UNCERTAIN, not prescribed in what was read] |
| **Special notes**: "Autoliquidation" when the customer owes the tax (§520); "Autofacturation" for self-billing (§550); travel agents and second-hand goods schemes (§570–580) | BOI §520–580 [PRIMARY] | — | Exact text constants. service-public spells it "Auto-liquidation" and "Auto-facturation" [PRIMARY]; use the BOFiP spelling |
| **Franchise en base**: "TVA non applicable - article 293 B du CGI" (art. 293 E). Omitting it can lead to a reassessment | F21746 and F31808 [PRIMARY] | — | Auto-print it for companies under franchise; no VAT lines |
| **Payment terms (Code de commerce)**: payment due date; early-payment discount terms (or "Escompte pour paiement anticipé : néant"); late-payment penalty rate; fixed recovery indemnity of **€40** (B2B only); purchase-order number if the buyer issued one; legal guarantee of conformity for consumers (listed goods) | F31808 [PRIMARY] | — | Footer fields with defaults |
| **Four new mentions**: (1) "le SIREN du client"; (2) "la catégorie de l'opération : livraison de biens, prestation de service ou double (ie à la fois une livraison et une prestation indépendantes l'une de l'autre)"; (3) "l'option de paiement de la TVA sur les débits, le cas échéant"; (4) "l'adresse de livraison des biens, si elle est différente de l'adresse de facturation" | DGFiP FAQ *Je découvre la facturation électronique* §3.4 (version 01/09/2026): décret n° 2022-1299 du 7 octobre 2022 amends art. 242 nonies A annexe II "pour rendre obligatoires ces nouvelles mentions à compter du 1er septembre 2026" [PRIMARY] | 1 Sep 2026 (see the next row on who) | Add the four fields to every invoice from 1 Sep 2026 (the safe reading) |
| **Who must print them, and when**: service-public says the new mentions apply "from 1 September 2026 for large companies and ETIs; from 1 September 2027 for SMEs and micro-enterprises" | F31808 [PRIMARY]; the DGFiP FAQ gives a single date [PRIMARY] — the two conflict [UNCERTAIN] | 2026 / 2027 | Print them for everyone from 1 Sep 2026; it is harmless for a PME |
| **Operation category in e-invoices (BT-23 "cadre de facturation")**: `B1` goods, `S1` services, `M1` double (goods and services not ancillary to each other); `B2/S2/M2` already paid; `B4/S4/M4` final invoice after a deposit; `S5` subcontractor; `S6` co-contractor; `B7/S7` already e-reported (VAT already collected) | Annexe 7 rule G1.02 [PRIMARY] | 1 Sep 2026 | Map the invoice category (and deposit/paid state) to BT-23 |
| **Option "débits" in e-invoices**: indicated in BT-8 with code **5 (CII)** or **3 (UBL)**; mandatory for services and double invoices only if the supplier opted for VAT on debits | Annexe 7 [PRIMARY] | 1 Sep 2026 | Company flag `vat_on_debits`, written to BT-8 |
| Unpaid invoices: to recover VAT already paid on a definitively unpaid invoice, send the debtor a duplicate marked "facture demeurée impayée pour la somme de … € (prix hors taxe) et pour la somme de … € (TVA correspondante) qui ne peut faire l'objet d'une déduction (art. 272 du CGI)", or a recap statement | CA3 notice, line 21 [PRIMARY] | — | Bad-debt workflow (future work) |

---

## 4. Facturation électronique and e-reporting

### 4.1 Scope, calendar and actors

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Scope**: e-invoicing (art. 289 bis CGI) covers operations "(i) réalisées entre assujettis à la TVA établis en France, (ii) qui entrent dans le champ de la TVA en France et (iii) pour lesquelles les règles de facturation françaises sont applicables". B2C customers and customers established abroad → **e-reporting** (art. 290 I CGI). Out-of-scope operations, and operations exempt under art. 261 to 261 E, are excluded from both | DGFiP FAQ §1.1, §1.2, §4.1 (version 01/09/2026) [PRIMARY] | 1 Sep 2026 | Classify each sales document as `E_INVOICE` (domestic B2B), `E_REPORT_TRANSACTION` (B2C, international) or `NONE` |
| **Receiving**: "Depuis le 1er septembre 2026, toutes les entreprises, quelles que soient leur taille et leur forme juridique, ont l'obligation de recevoir des factures sous format électronique, par l'intermédiaire d'une plateforme agréée". This includes businesses under franchise and those making only exempt supplies (e.g. a GP must have chosen a PA) | FAQ §1.2, §2.2, §5.1; fiche 3 (mise à jour octobre 2026) [PRIMARY] | 1 Sep 2026 | Import received e-invoices (UBL/CII/Factur-X) from the user's PA for purchase booking (future work) |
| **Issuing**: GE and ETI "doivent également émettre toutes leurs factures sous format électronique" from 1 Sep 2026. PME and micro-enterprises "au plus tard le 1er septembre 2027"; early voluntary entry is allowed. Size is assessed per legal unit (SIREN) | FAQ §5.1–5.3; fiche 3 [PRIMARY] | 1 Sep 2026 / 1 Sep 2027 | Company setting "e-invoicing issuer from" (default 2027-09-01 for PME/micro) |
| Size definitions (fiche 3 footnotes): GE: headcount > 5,000, or turnover > €1.5 bn and balance sheet > €2 bn. ETI: 250–5,000 staff and turnover < €1.5 bn or balance sheet < €2 bn. PME: 10–250 staff and turnover < €50 m or balance sheet < €43 m. Micro: < 10 staff and turnover < €2 m or balance sheet < €2 m | fiche 3 [PRIMARY] | — | Help text only |
| "Un PDF simple envoyé par voie électronique n'est pas une facture électronique au sens du dispositif". Invoices to business customers go "par l'intermédiaire obligatoire d'une plateforme agréée" | FAQ §3.1 [PRIMARY] | 1 Sep 2026 | Once the company is an issuer, a domestic B2B invoice is only "issued" when it has been handed to a PA |
| A received invoice can be **refused** with a reason (final). Disputes use provisional statuses; invoices carry life-cycle **statuses** | FAQ §3.2 [PRIMARY]; statuses in AFNOR XP Z12-012 [not read] | 1 Sep 2026 | Model invoice status (deposited, received, refused, disputed, paid "Encaissée") (future work) |
| **Plateformes agréées (PA)**: registered ("immatriculées") by the DGFiP for three years, renewable. Only they can issue, transmit and receive e-invoices and send invoice, transaction and payment data to the administration. A *solution compatible* (invoicing or accounting software) must be connected to at least one PA. Choice is free; a business may use several PAs and change at any time. The list is published at impots.gouv.fr/je-consulte-la-liste-des-plateformes-agreees | impots.gouv.fr *Facturation électronique et plateformes agréées*; FAQ §2.1–2.3 [PRIMARY] | 2026 | KhataOS aims to be a *solution compatible*: generate EN 16931 XML and hand it to the user's PA via API (XP Z12-013) (future work) |
| The older term **PDP** (plateforme de dématérialisation partenaire) is replaced by **plateforme agréée**; the 2026 texts use "plateforme agréée" only | DGFiP pages and FAQ [PRIMARY for current usage]; which law or decree made the change (loi de finances 2025, or décret n° 2026-677 du 27 juillet 2026 as one secondary source says) [UNCERTAIN] | — | Use "PA" in the UI |
| **PPF (portail public de facturation)**: hosts the annuaire and collects and processes invoice, transaction and payment data sent by the PAs. It is not an invoicing platform for businesses | impots.gouv.fr *factures-norme-afnor* page (modified 2 Jul 2026); spec v3.2 [PRIMARY] | — | Never send to the PPF directly |
| The plan for the PPF to also be a free invoicing platform was dropped in **October 2024** (announced 15 Oct 2024) | Next.ink, Hayot Expertise, Keobiz and others [SECONDARY] | Oct 2024 | — |
| **Public sector (B2G)**: since 1 Jan 2017 public bodies receive invoices through **Chorus Pro**. From 1 Sep 2026 the B2G rules match B2B (formats, structured data). Suppliers either go through a PA connected to Chorus Pro (the target) or keep using Chorus Pro directly (portal, API, EDI) for now | FAQ §11 [PRIMARY] | 1 Sep 2026 | Chorus Pro export is future work |
| **Go-live**: on 1 Sep 2026 the DGFiP counted "149 plateformes dont l'immatriculation est validée", with 14 more in interoperability testing. No tolerance period was reported. impots.gouv.fr news of 28 Sep 2026 restates the obligations. Bercy asked the PAs to strengthen security after two intrusions at the DGFiP (late June and late July 2026) | presse-citron.net (1 Sep 2026), siecledigital.fr (27 Aug 2026) [SECONDARY]; impots.gouv.fr actualité 28/09/2026 [PRIMARY] | 1 Sep 2026 | — |
| **Legal basis**: art. 289 bis CGI (e-invoicing), art. 290 (transaction e-reporting), art. 242 nonies A annexe II (mentions), art. 41 septies D annexe IV (invoice data sent to the administration), art. 242 nonies M annexe II (transaction data). Décret n° 2022-1299 du 7 octobre 2022 | DGFiP FAQ §3.4, §6.1, §9.1 [PRIMARY] | — | — |
| The present 2026/2027 calendar was set by **art. 91 of loi n° 2023-1322 du 29 décembre 2023** (loi de finances pour 2024), implemented by décret n° 2024-266 du 25 mars 2024. Adjusted by **loi n° 2026-103 du 19 février 2026** (loi de finances pour 2026, art. 123), which also raised the penalties | eurojuris.fr and other legal-press summaries [SECONDARY]; Légifrance not read [UNCERTAIN on article numbers] | — | — |
| **Penalties** (art. 1737 CGI as amended by LF 2026): €50 per invoice not issued electronically (was €15), €500 per missing e-reporting transmission (was €250), each capped at €15,000 per calendar year. Not choosing a receiving PA: formal notice, then €500 and later €1,000 per quarter (art. 1737 IV bis). A first offence may be excused if corrected within 30 days | journaldeleconomie.fr, fiducial.fr, ma-facture-electronique.org [SECONDARY]; Légifrance not read [UNCERTAIN] | 2026 | Help text only |

### 4.2 Formats and specifications

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| The **socle** of three formats: "le format Cross Industry Invoice (CII) et le format Universal Business Language (UBL), sont structurés pour un échange informatisé efficace. Le troisième, Factur-X, est un format mixte composé d'un fichier de données structurées au format XML et d'un fichier de données lisibles au format PDF". PAs are obliged to transmit invoices in one of these three | DGFiP fiche 1 [PRIMARY] | 1 Sep 2026 | `khata-en16931` writes UBL 2.1 and CII D22B, plus Factur-X PDF/A-3 |
| Versions: UBL per OASIS UBL 2.1; CII per UN/CEFACT CCTS 3.0, "la version de langage retenue dans le cadre de la réforme est le CII D22B". All three are EN 16931-based. The French extensions are labelled `EXT-FR-FE-xxx` | Spec v3.2 Dossier général, footnotes 95–100 [PRIMARY] | — | Target CII D22B (Factur-X 1.08+ is D22B-based, see the Germany doc §10) |
| **Peppol** is integrated "comme infrastructure d'interopérabilité complémentaire" between PAs; if two PAs cannot interoperate and one uses Peppol, they must use it | Spec v3.2 §2 [PRIMARY] | 2026 | Peppol BIS output can serve as a transport to PAs (future work) |
| **External specifications**: version **3.2 of 30 Apr 2026** (BtoB, GtoB, BtoG) is current; earlier versions are v3.1 (31 Oct 2025), v3.0 (18 Dec 2024), v2.4 (19 Jun 2024) and back to v1.0 (30 Sep 2021). Annexes: 1 Format sémantique Flux 1 v1.2; 2 CDV (life cycle) Flux 6 V2.3; 3 Annuaire V1.8; 6 E-reporting V1.10; 7 Règles de gestion V1.9; XSDs; annuaire OpenAPI 1.11.0 | impots.gouv.fr *Spécifications externes et normes* (modified 2 Jul 2026); zip contents [PRIMARY] | 30 Apr 2026 | Pin to v3.2; implement the G1.xx business rules that apply to the issuer |
| **AFNOR standards**: XP Z12-012 (formats and profiles of the Facture and Statut de cycle de vie messages), XP Z12-013 (APIs between business systems and PAs), XP Z12-014 (B2B use cases); free from AFNOR | impots.gouv.fr [PRIMARY]; the PDF copy of XP Z12-012 (May 2025) linked from impots.gouv.fr was not downloadable [UNCERTAIN on the current edition] | — | Read XP Z12-012 before building the CII/UBL profiles |
| Mandatory structured data grows in steps: "26 données obligatoires en 2026 puis 34 données dès 2027" | FAQ §6.3 [PRIMARY] | 2026 → 2027 | Plan for the 2027 data set from the start |
| Factur-X profile for France: EXTENDED-CTC-FR `urn:cen.eu:en16931:2017#conformant#urn.cpro.gouv.fr:1p0:extended-ctc-fr` | Mustang `Profiles.java` (Germany doc §10) [SECONDARY] | — | Check against XP Z12-012 [UNCERTAIN] |

### 4.3 E-reporting

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Transaction e-reporting** applies to every VAT-registered business established in France for operations with non-taxable persons (consumers, some associations) or foreign operators (e.g. intra-EU acquisitions and supplies, exports). Some non-established businesses are covered too (e.g. intra-EU distance sales taxable in France). Imports are excluded | FAQ §7.1; fiche 6 [PRIMARY] | Same schedule as issuing: GE/ETI 1 Sep 2026; PME/micro 1 Sep 2027 (entering e-invoicing early does not bring e-reporting forward) | Per period, aggregate B2C sales by day and rate; send international B2B invoice data invoice by invoice |
| B2C: "le montant total des opérations réalisées par jour et sur une période définie". International B2B: "facture par facture, certaines données de l'opération" | FAQ §9.1 [PRIMARY] | — | Daily totals by rate (B2C) |
| **Frequency** is set by the VAT regime. Franchise en base: every two months (calendar bimonth). Réel normal mensuel: transactions **three times a month** (1–10, 11–20, 21–end), each due 10 days after the period ends (the 20th, the 30th, the 10th of the next month); payment data monthly. Other regimes: see the DGFiP table *Fréquences et délais de transmission* (not read) | FAQ §9.3, §10.4; fiche 6 [PRIMARY]; réel simplifié and quarterly timings [UNCERTAIN] | 2026/2027 | Scheduler per regime |
| **Payment e-reporting** applies only where VAT is due on receipt (*encaissements*), typically services without the "débits" option and deposits. Not needed where the supplier opted for débits or the customer self-assesses. The supplier reports. For an e-invoice: lifecycle status "Encaissée" with the receipt date and the amount received split by VAT rate. Otherwise: a dedicated XML flow | FAQ §10.1–10.3 [PRIMARY] | 2026/2027 | Record receipts with a per-rate split for VAT-on-receipt invoices |
| Data sent to the administration is meant to **pre-fill the CA3** | FAQ §6.1, §6.3 [PRIMARY] | — | Keep CA3 figures reconcilable with the e-invoicing and e-reporting data |

---

## 5. Franchise en base (art. 293 B CGI)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Thresholds (businesses established in France), prior-year turnover (base) / current-year turnover (majoré): commerce and lodging **€85,000 / €93,500**; services **€37,500 / €41,250**; liberal professions (excluding lawyers) €37,500 / €41,250; lawyers, regulated activities €50,000 / €55,000; lawyers, other activities €35,000 / €38,500; authors and performers, works and rights €50,000 / €55,000; authors and performers, other activities €35,000 / €38,500 | service-public F21746 (vérifié le 1 Jan 2026) [PRIMARY] | 2026 | Turnover monitor by activity type |
| Exceeding the *majoré* threshold during the year ends the franchise "from the first day of the overrun" | F21746 [PRIMARY] | — | Switch the crossing transaction (and later ones) to normal VAT |
| The loi de finances 2025 set a single €25,000 threshold. It was suspended throughout 2025, and "a été abandonnée"; for 2026 the thresholds "restent inchangés". The PLF 2026 (art. 25) proposed €37,500 general / €25,000 for building works; that article was not adopted | F21746 [PRIMARY for abandonment and 2026 thresholds]; legifiscal.fr and CCI Lyon (Mar 2026) for the suspension and the PLF 2026 article [SECONDARY] | 2025–2026 | Seed the thresholds above; do not seed €25,000 |
| EU businesses: same thresholds, plus an EU-wide €100,000 cap above which the French franchise does not apply (art. 293 B ter, prior notification) | F21746 [PRIMARY] | 1 Jan 2025 [SECONDARY] | Not modelled |
| Invoice note "TVA non applicable - article 293 B du CGI" (art. 293 E); option to pay VAT (art. 293 F) | F21746 [PRIMARY] | — | §3 |
| Businesses under franchise must still **receive** e-invoices from 1 Sep 2026, and must e-report and issue e-invoices on the PME schedule (franchise e-reporting is every two months) | DGFiP fiche 1; FAQ §9.3 [PRIMARY] | 2026/2027 | — |

---

## 6. CA3 (3310-CA3-SD) and filing regimes

### 6.1 Filing rules

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Réel normal**: "La TVA et les taxes assimilées doivent être déclarées et acquittées selon une périodicité mensuelle. Seuls les redevables dont la taxe exigible annuellement est inférieure à 4 000 € peuvent déposer des déclarations n° 3310-CA3 trimestrielles." Never combine several months (or quarters) in one return; file a "néant" return (box 0010) when there were no operations | CA3 notice 3310-NOT-CA3-SD (50449#29), §I [PRIMARY] | 2026 | Period selector monthly / quarterly with a hint from last year's tax |
| Due date: between the **15th and 24th** of the following month (depends on the business) | service-public F23566 (vérifié le 21 Feb 2026) [PRIMARY] | — | Due-date setting per company (the exact day is set by the tax office) [UNCERTAIN on how it is assigned] |
| **Online only**: "Vous devez télédéclarer par transfert de fichiers (mode EDI) ou en ligne depuis le site impots.gouv.fr (mode EFI) et télépayer la TVA". EFI = the business types the figures on impots.gouv.fr and signs; EDI = a provider transmits a file produced by accounting software | CA3 notice, page 1 and §I [PRIMARY] | — | MVP: produce the CA3 figures for keying into EFI. EDI-TVA transmission is future work (it needs a DGFiP-registered EDI partner) [UNCERTAIN on the partner procedure] |
| **Réel simplifié (RSI)**: prior-year turnover €85,000–€945,000 (commerce, restaurants, lodging) or €37,500–€286,000 (other services), with current-year *majoré* limits of €1,040,000 / €323,000; prior-year VAT < **€15,000**. Annual return **CA12** (3517-S-SD), due by the 2nd working day after 1 May; installments of 55% of last year's VAT in July and 40% in December (none if last year's VAT < €1,000) | F23566 [PRIMARY] | 2026 | Not in the MVP. The **mini-réel** option lets an RSI business file CA3s (CA3 notice) |
| Above €945,000 / €286,000 → réel normal | F23566 [PRIMARY] | — | Regime hint |
| **Rounding**: "La base imposable et le montant de l'impôt sont arrondis à l'euro le plus proche. Les bases et cotisations inférieures à 0,50 euro sont négligées et celles supérieures ou égales à 0,50 euro sont comptées pour 1"; deductible VAT is rounded per category the same way | CA3 notice §I and cadre B [PRIMARY] | — | Round each box half-up to whole euros (round per box, not per invoice) |
| "Ne jamais indiquer de sommes négatives"; credit notes go to the *régularisation* lines (B5, F8, 15, 21), not as negatives | CA3 notice, cadre B, B5, F8 [PRIMARY] | — | Route negative movements to the regularisation lines |
| Crediting: a refund (line 26, form 3519) needs a credit of at least **€760** for a monthly or quarterly claim (January–November or the first three quarters) or **€150** for the annual claim (December or Q4); otherwise the credit is carried forward (line 27 → line 22 of the next return) | CA3 notice line 26–27 [PRIMARY] | — | Carry the credit forward by default |
| Intra-EU operations: also file the **DEB** (goods) / **DES** (services) with the customs administration (DGDDI) | CA3 form footer and notice §II [PRIMARY] | — | Future work |

### 6.2 Line codes (3310-CA3-SD, cerfa n° 10963\*31, millésime 2026)

The form prints a line code (e.g. `A1`) and a 4-digit box code (e.g. `0979`). The 2025 form (10963\*30) has exactly the same box codes; the 2024 form (10963\*29) differs only in the energy-excise box (no XA/YA/ZB/M1–M9) [PRIMARY, compared here]. There is **no line 14** on the 2025/2026 forms. **[UNCERTAIN]** In cadre B one box code is printed per line, covering both "Base hors taxe" and "Taxe due"; how EDI-TVA/EFI keys the base and the tax separately was not researched.

**Cadre A — Montant des opérations réalisées (HT, whole euros)**

| Line | Box | Label (form) | Filled by KhataOS from |
|---|---|---|---|
| A1 | 0979 | Ventes, prestations de services | All taxable sales (net of VAT), any rate, mainland, Corse and DOM; includes distance sales taxable in France |
| A2 | 0981 | Autres opérations imposables | Asset disposals, self-supplies; **buyer side of domestic reverse charge** (art. 283-2 sexies to nonies: scrap, emission allowances, electronic communications, construction subcontracting) and gold |
| A3 | 0044 | Achats de prestations de services réalisés auprès d'un assujetti non établi en France (article 283-2 du CGI) | Services bought from EU or non-EU suppliers, VAT self-assessed (art. 259-1° + 283-2) |
| A4 | 0056 | Importations (autres que les produits pétroliers) | Imports whose VAT is self-assessed on the CA3 (tax on I1–I6). The notice says A4 and I1–I6 are wrongly pre-filled for some customs exits; correct them by hand [PRIMARY] |
| A5 | 0051 | Sorties de régime fiscal suspensif (autres que les produits pétroliers) … | not modelled |
| B1 | 0048 | Mises à la consommation de produits pétroliers | not modelled |
| B2 | 0031 | Acquisitions intracommunautaires | Taxable intra-EU acquisitions of goods |
| B3 | 0030 | Achats d'électricité, de gaz naturel, de chaleur ou de froid imposables en France | not modelled |
| B4 | 0040 | Achats de biens ou de prestations de services réalisés auprès d'un assujetti non établi en France (article 283-1 du CGI) | Domestic supplies of goods/services by a non-established supplier where the buyer owes the tax |
| B5 | 0036 | Régularisations (important : cf. notice) | Sales credit notes and rebates on taxable sales (positive figure). The matching VAT goes to line 21 "dont régularisation de TVA collectée" |
| E1 | 0032 | Exportations hors UE | Exports outside the EU (art. 262-I); for DOM businesses also sales to mainland France / other DOM |
| E2 | 0033 | Autres opérations non imposables | Exempt supplies; **services to customers established abroad** (B2B general rule); **seller side of domestic reverse charge** (art. 283-2 sexies–nonies); transfers of a going concern (257 bis); OSS-declared e-services |
| E3 | 0047 | Ventes à distance taxables dans un autre État membre au profit des personnes non assujetties – Ventes B to C | Intra-EU B2C distance sales above €10,000 or under the OSS |
| E4 | 0052 | Importations (autres que les produits pétroliers) [with zero VAT] | not modelled |
| E5 | 0053 | Sorties de régime fiscal suspensif (autres que les produits pétroliers) | not modelled |
| E6 | 0054 | Importations placées sous régime fiscal suspensif (autres que les produits pétroliers) | not modelled |
| F1 | 0055 | Acquisitions intracommunautaires [non taxées] | Exempt intra-EU acquisitions |
| F2 | 0034 | Livraisons intracommunautaires à destination d'une personne assujettie – Ventes B to B | Exempt intra-EU supplies of goods |
| F3 | 0029 | Livraisons d'électricité, de gaz naturel, de chaleur ou de froid non imposables en France | not modelled |
| F4 | 0049 | Mises à la consommation de produits pétroliers [non taxables] | not modelled |
| F5 | 0050 | Importations de produits pétroliers placées sous régime fiscal suspensif | not modelled |
| F6 | 0037 | Achats en franchise | Duty-free purchases by exporters (art. 275) |
| F7 | 0043 | Ventes de biens ou prestations de services réalisées par un assujetti non établi en France (article 283-1 du CGI) | only for non-established filers |
| F8 | 0039 | Régularisations (important : cf. notice) | Credit notes on non-taxable operations (positive figure) |
| F9 | 0061 | Opérations internes réalisées entre membres d'un assujetti unique | not modelled |

**Cadre B — Décompte de la TVA à payer (base and tax, whole euros)**

| Line | Box | Label (form) | Filled by KhataOS from |
|---|---|---|---|
| 08 | 0207 | Taux normal 20 % (France métropolitaine) | Output VAT at 20%, including self-assessed VAT on A2/A3/B2/B4 at 20% |
| 09 | 0105 | Taux réduit 5,5 % | 5.5% |
| 9B | 0151 | Taux réduit 10 % | 10% |
| 10 | 0201 | Taux normal 8,5 % (DOM) | DOM 8.5% |
| 11 | 0100 | Taux réduit 2,1 % (DOM) | DOM 2.1% |
| T1 | 1120 | Opérations réalisées dans les DOM et imposables au taux de 1,75 % | DOM 1.75% |
| T2 | 1110 | Opérations réalisées dans les DOM et imposables au taux de 1,05 % | DOM 1.05% |
| TC | 1090 | Opérations réalisées en Corse et imposables au taux de 13 % | Corse 13% (sales of petroleum products after release for consumption) |
| T3 | 1081 | Opérations réalisées en Corse et imposables au taux de 10 % | Corse 10% |
| T4 | 1050 | Opérations réalisées en Corse et imposables au taux de 2,1 % | Corse 2.1% |
| T5 | 1040 | Opérations réalisées en Corse et imposables au taux de 0,9 % | Corse 0.9% |
| T6 | 1010 | Opérations réalisées en France continentale au taux de 2,1 % | Mainland 2.1% |
| T7 | 0990 | Retenue de TVA sur droits d'auteur | not modelled |
| 13 | 0900 | Anciens taux | Supplies taxable at former rates (e.g. 19.6% / 7%) |
| P1 / P2 | 0208 / 0152 | Produits pétroliers: taux normal 20 % / taux réduit 13 % | not modelled |
| I1–I6 | 0210–0215 | Importations: 20 %, 10 %, 8,5 %, 5,5 %, 2,1 %, 1,05 % | Self-assessed import VAT by rate (with A4) |
| 15 | 0600 | TVA antérieurement déduite à reverser | Input VAT to repay: supplier credit notes received, deductions made in error, changes to the deduction percentage |
| 5B | 0602 | Sommes à ajouter, y compris acompte congés | Corrections of under-declared amounts (explain in the correspondence box) |
| 16 | — | Total de la TVA brute due (lignes 08 à 5B) | sum |
| 17 | 0035 | Dont TVA sur acquisitions intracommunautaires | Tax on B2 ("Doit être impérativement mentionnée") |
| 18 | 0038 | Dont TVA sur opérations à destination de Monaco | not modelled (warn if Monaco customers exist) |
| 19 | 0703 | Biens constituant des immobilisations | Input VAT on fixed assets |
| 20 | 0702 | Autres biens et services | Input VAT on goods and services, including self-assessed VAT on A2/A3/B2/B4 and import VAT |
| 21 | 0059 | Autre TVA à déduire (dont … régularisation de TVA collectée …) | Omitted deductions from earlier periods (back to 1 Jan of year N−2); output VAT recovered on customer credit notes, cancellations or definitively unpaid invoices (shown separately as "dont régularisation de TVA collectée") |
| 22 | 8001 | Report du crédit apparaissant ligne 27 de la précédente déclaration | Credit brought forward |
| 2C | 0603 | Sommes à imputer, y compris acompte congés | Over-declared amounts from earlier returns |
| 22A | — | Coefficient de taxation unique applicable pour la période s'il est différent de 100 % | Partial exemption (not modelled) |
| 23 | — | Total TVA déductible (ligne 19 à 2C) | sum |
| 24 | 0710 | Dont TVA déductible sur importations hors produits pétroliers | Import VAT part of 19–20 |
| 2E | 0711 | Dont TVA déductible sur les produits pétroliers | not modelled |
| 25 | 0705 | Crédit de TVA (ligne 23 – ligne 16) | If 23 > 16 |
| TD | 8900 | TVA due (ligne 16 – ligne 23) | If 16 ≥ 23 |
| X1–X5, Y1–Y6, Z1–Z5, XA, YA, ZB, M1–M9 | 81xx, 82xx | Consommateurs d'énergie : régularisation d'accise sur les énergies (incl. the excise surcharge from 1 Aug 2025) | not modelled (X5 = 0) |
| 26 | 8002 | Remboursement de crédit de TVA demandé sur formulaire n°3519 | Refund claimed (≥ €760 / €150) |
| AA | 8005 | Crédit de TVA transféré à la société tête de groupe … 3310-CA3G | not modelled |
| 27 | 8003 | Crédit de TVA à reporter (ligne 25 – ligne 26) | Carry forward → next return's line 22 |
| 28 | 8901 | TVA nette due (ligne TD – ligne X5) | Net VAT payable (never negative) |
| 29 | 9979 | Taxes assimilées calculées sur l'annexe n° 3310 A | 0 unless annex taxes are modelled |
| AB | 9991 | Total à payer acquitté par la société tête de groupe … | not modelled |
| 32 | 9992 | Total à payer (lignes 28 + 29 + Z5 – AB) | Amount to pay |

Header boxes: 0001 (representative of an *assujetti unique*), 0005 (group payment consolidation, DGE only), 0010 (déclaration « néant ») [PRIMARY].

Implementation notes:
- **Self-assessed VAT** (A2/A3/B2/B4/A4) appears twice: as a base in cadre A and the tax in the rate line (08/9B/09/I1…), then again as deductible on line 20 (or 19), net zero if fully deductible. The notice warns that not declaring self-assessed VAT is fined 5% of the deductible amount even though it is neutral (art. 1788 A-4) [PRIMARY].
- The **domestic reverse-charge seller** reports the base in E2 and prints "Autoliquidation"; the buyer reports it in A2 [PRIMARY, notice A2/E2; BOFiP §520].
- Line 28 (+ 29) must equal the ledger's VAT payable for the period after whole-euro rounding; keep the rounding difference in a dedicated account [interpretive].

---

## 7. Bookkeeping pointers: FEC, cash registers, retention

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **FEC (fichier des écritures comptables)**: businesses with computerised books must hand over a file of all entries in the standard format during a tax audit (art. L47 A-I LPF). Format in art. **A47 A-1 LPF**; the 18 fields in order: JournalCode, JournalLib, EcritureNum, EcritureDate, CompteNum, CompteLib, CompAuxNum, CompAuxLib, PieceRef, PieceDate, EcritureLib, Debit, Credit, EcritureLet, DateLet, ValidDate, Montantdevise, Idevise. First line = field names; example file name `123456789FEC20131231_01` | BOFiP BOI-CF-IOR-60-40-20 (7 Jun 2017) [PRIMARY for the legal basis, the example name and the field list as used there]. BOFiP writes `CompteAuxNum`/`CompteAuxLib` in its examples, while most implementations use `CompAuxNum`/`CompAuxLib` [UNCERTAIN, check A47 A-1]. Naming rule SIREN + "FEC" + closing date AAAAMMJJ, separators tab or `|`, encodings, fine of €5,000 or 10% [SECONDARY] | since 1 Jan 2014 | Future work: FEC export; the DGFiP *Test Compta Demat* tool validates files |
| **Cash-register software** (art. 286 I 3° bis CGI): VAT-registered businesses that record sales to individuals in cash-register, management or accounting software must use software meeting integrity, security, retention and archiving conditions, proven by a certificate from an accredited body (e.g. NF525) or by the publisher's individual attestation. The attestation route was reinstated by the loi de finances pour 2026 (loi n° 2026-103, art. 125); BOFiP actualité of 25 Mar 2026 | BOFiP (search-result summary of the actualité and BOI-TVA-DECLA-30-10-30) [SECONDARY, page not read]; deadlines extended to 31 Aug 2026 for publishers (BOI of 1 Oct 2025) [SECONDARY] | 2026 | **Not in scope**: KhataOS must not market itself as a cash register (POS) in France without certification. Recording B2C takings in it may fall under art. 286 [UNCERTAIN, needs a professional opinion] |
| **Retention**: invoices and supporting documents 6 years for tax purposes (art. L102 B LPF); books and supporting documents 10 years under commercial law (art. L123-22 Code de commerce) | Eurojuris (L102 B, six years) [SECONDARY]; L123-22 not researched [UNCERTAIN] | — | Retention metadata; never auto-delete within 10 years |

---

## 8. Financial year, currency and regions

### 8.1 Exercice comptable and currency

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| The closing date is generally free, chosen at registration; the year "generally lasts 12 months". **Micro-entrepreneurs must close on 31 December**. The first year may be shorter or longer, up to 24 months | Entreprendre Service Public fiche *Choisir la date de clôture de l'exercice comptable*, read via its justice.fr copy [PRIMARY-equivalent]; the 24-month limit and its conditions by legal form [SECONDARY, sources disagree] | — | Default 1 Jan–31 Dec; allow any 12-month period; allow a long or short first year (warn above 24 months) |
| Changing the closing date: for companies by an extraordinary general meeting, then a filing on the *guichet unique* within a month | legal guides [SECONDARY] | — | Support a short transition year |
| VAT periods (CA3 month or quarter, CA12 year) are calendar-based and independent of the exercice | CA3 notice ("au titre d'un mois ou d'un trimestre") [PRIMARY]; CA12 with a non-calendar year-end follows the exercice [UNCERTAIN, not researched] | — | VAT periods separate from the financial year |
| Currency **EUR**, 2 decimals in the books; CA3 in whole euros (§6.1). E-invoices in a foreign currency are allowed, with VAT also in EUR (rule S1.16 "gérer les factures en devise étrangère") | CA3 notice [PRIMARY]; Annexe 7 change log [PRIMARY, detail not read] | — | Base currency EUR; for a foreign-currency invoice, compute and print the VAT total in EUR (BT-111) [interpretive] |

### 8.2 Regions (recommended address-region list)

Recommendation: use the **18 regions** (13 metropolitan + 5 overseas), keyed by their ISO 3166-2:FR codes. Do **not** use `FR-GP`, `FR-MQ`, `FR-GF`, `FR-RE`, `FR-YT`: ISO deleted these subdivision codes on 2021-11-25. The five overseas regions are now coded `FR-971`…`FR-976`, and they also have their own ISO 3166-1 country codes (GP, MQ, GF, RE, YT), which some partners will send instead. Accept those as aliases on import. [SECONDARY, Wikipedia *ISO 3166-2:FR* change history citing ISO Online Browsing Platform updates 2016-11-15, 2020-11-24 and 2021-11-25; the ISO OBP itself was not queried]

| ISO 3166-2 | Region | ISO category | VAT rates |
|---|---|---|---|
| FR-ARA | Auvergne-Rhône-Alpes | metropolitan region | mainland |
| FR-BFC | Bourgogne-Franche-Comté | metropolitan region | mainland |
| FR-BRE | Bretagne | metropolitan region | mainland |
| FR-CVL | Centre-Val de Loire | metropolitan region | mainland |
| FR-GES | Grand Est | metropolitan region | mainland |
| FR-HDF | Hauts-de-France | metropolitan region | mainland |
| FR-IDF | Île-de-France | metropolitan region | mainland |
| FR-NOR | Normandie | metropolitan region | mainland |
| FR-NAQ | Nouvelle-Aquitaine | metropolitan region | mainland |
| FR-OCC | Occitanie | metropolitan region | mainland |
| FR-PDL | Pays-de-la-Loire | metropolitan region | mainland |
| FR-PAC | Provence-Alpes-Côte-d'Azur | metropolitan region | mainland |
| FR-20R | Corse | metropolitan collectivity with special status (was FR-COR until 2020-11-24) | **Corse rates** (art. 297): 13 / 10 / 2.1 / 0.9 + mainland 20% |
| FR-971 | Guadeloupe | overseas departmental collectivity | **DOM rates** (art. 296): 8.5 / 2.1 / 1.75 / 1.05; outside the EU VAT territory |
| FR-972 | Martinique | overseas unique territorial collectivity | **DOM rates** |
| FR-973 | Guyane (française) | overseas unique territorial collectivity | **no VAT** (art. 294) |
| FR-974 | La Réunion | overseas departmental collectivity | **DOM rates** |
| FR-976 | Mayotte | overseas departmental collectivity | **no VAT** (art. 294) |

Notes: ISO 3166-2:FR also lists FR-69M (Métropole de Lyon), FR-75C (Paris) and FR-6AE (Alsace) as collectivities, plus the 96 metropolitan departments. These are not regions: Paris is in FR-IDF, Lyon in FR-ARA, Alsace in FR-GES. An optional *département* field (2-digit code, `2A`/`2B` for Corse) could drive the region automatically from the postcode [not researched]. Overseas collectivities outside the DOM (Saint-Barthélemy, Saint-Martin, Saint-Pierre-et-Miquelon, etc.) have their own tax systems and are out of scope.

---

## 9. Sources

- service-public (entreprendre.service-public.gouv.fr): F23570 *Numéro de TVA intracommunautaire* <https://entreprendre.service-public.gouv.fr/vosdroits/F23570>; F21746 *Franchise en base de TVA* <https://entreprendre.service-public.gouv.fr/vosdroits/F21746>; F31808 *Mentions obligatoires sur une facture* <https://entreprendre.service-public.gouv.fr/vosdroits/F31808>; F23566 *Déclarer et payer la TVA* <https://entreprendre.service-public.gouv.fr/vosdroits/F23566>; exercise closing date (copy) <https://www.justice.fr/fiche/choisir-date-cloture-exercice-comptable>
- INSEE, SIREN definition: <https://www.insee.fr/fr/metadonnees/definition/c2047>
- BOFiP: BOI-TVA-LIQ <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-LIQ>; BOI-TVA-LIQ-20 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-LIQ-20>; BOI-TVA-LIQ-30 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-LIQ-30>; BOI-TVA-LIQ-40 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-LIQ-40>; BOI-TVA-LIQ-50 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-LIQ-50>; BOI-TVA-GEO-10 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-GEO-10>; BOI-TVA-GEO-10-10 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-GEO-10-10>; BOI-TVA-GEO-20 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-GEO-20>; BOI-TVA-GEO-20-10 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-GEO-20-10>; BOI-TVA-DECLA-30-20-20 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-DECLA-30-20-20>; BOI-TVA-DECLA-30-20-20-10 <https://bofip.impots.gouv.fr/doctrine/BOI-TVA-DECLA-30-20-20-10>; BOI-CF-IOR-60-40-20 <https://bofip.impots.gouv.fr/doctrine/BOI-CF-IOR-60-40-20>
- CA3 forms page (millésimes 2024–2026) <https://www.impots.gouv.fr/formulaire/3310-ca3-sd/tva-et-taxes-assimilees-regime-du-reel-normal-mini-reel>; form 2026 (10963\*31) <https://www.impots.gouv.fr/sites/default/files/formulaires/3310-ca3-sd/2026/3310-ca3-sd_5377.pdf>; notice 2026 (50449#29) <https://www.impots.gouv.fr/sites/default/files/formulaires/3310-ca3-sd/2026/3310-ca3-sd_5426.pdf>; form 2025 (10963\*30) <https://www.impots.gouv.fr/sites/default/files/formulaires/3310-ca3-sd/2025/3310-ca3-sd_4931.pdf>; form 2024 (10963\*29) <https://www.impots.gouv.fr/sites/default/files/formulaires/3310-ca3-sd/2024/3310-ca3-sd_4652.pdf>
- DGFiP e-invoicing: FAQ *Je découvre la facturation électronique* (version 01/09/2026) <https://www.impots.gouv.fr/sites/default/files/media/1_metier/2_professionnel/EV/2_gestion/290_facturation_electronique/faq---fe_je-decouvre-la-facturation-electronique.pdf>; fiche 3 (mise à jour octobre 2026) <https://www.impots.gouv.fr/sites/default/files/media/1_metier/2_professionnel/EV/2_gestion/290_facturation_electronique/fiche-3_tpe_a-partir-de-quand-mon-entreprise-doit-etre-prete.pdf>; fiche 1 <https://www.impots.gouv.fr/sites/default/files/media/1_metier/2_professionnel/EV/2_gestion/290_facturation_electronique/fiches_reforme/fiche-1_f.pdf>; fiche 6 (RN mensuel) <https://www.impots.gouv.fr/sites/default/files/media/1_metier/2_professionnel/EV/2_gestion/290_facturation_electronique/fiches_reforme/fiche-6_rnm_v.pdf>; *Facturation électronique et plateformes agréées* <https://www.impots.gouv.fr/facturation-electronique-et-plateformes-agreees>; *Je découvre la facturation électronique* <https://www.impots.gouv.fr/professionnel/je-decouvre-la-facturation-electronique>; actualité <https://www.impots.gouv.fr/actualite/facturation-electronique>; *Spécifications externes et normes* <https://www.impots.gouv.fr/factures-norme-afnor>; spec v3.2 zip (Dossier général v3.2, Annexes 1, 2, 3, 6, 7, XSD, annuaire OpenAPI 1.11.0) <https://www.impots.gouv.fr/sites/default/files/media/1_metier/2_professionnel/EV/2_gestion/290_facturation_electronique/specification_externes_b2b/specifications-externes-v3.2.zip>; list of PAs <https://www.impots.gouv.fr/je-consulte-la-liste-des-plateformes-agreees>
- Secondary: python-stdnum `fr/tva.py`, `fr/siren.py`, `fr/siret.py`, `tests/test_eu_vat.doctest` <https://github.com/arthurdejong/python-stdnum>; Wikipedia fr *Système d'identification du répertoire des entreprises* <https://fr.wikipedia.org/wiki/Syst%C3%A8me_d%27identification_du_r%C3%A9pertoire_des_entreprises>; Wikipedia *ISO 3166-2:FR* <https://en.wikipedia.org/wiki/ISO_3166-2:FR>; key-formula pages <https://www.lafabriquedunet.fr/logiciels/tendances/tva-intracommunautaire-numero>, <https://copeps.fr/actualites/modulo-97-tva-intracommunautaire/>; 2014 rate change (Assemblée nationale QE 50405, 35193) <https://www.assemblee-nationale.fr/dyn/14/questions/QANR5L14QE50405.pdf>, <https://questions.assemblee-nationale.fr/dyn/14/questions/QANR5L14QE35193.pdf>; PLF 2026 franchise <https://www.legifiscal.fr/actualites-fiscales/4273-plf-2026-franchise-base-tva-37500-btp.html>, <https://bpifrance-creation.fr/entrepreneur/actualites/plf-2026-franchise-base-tva-annoncee-a-37-500-eu>; e-invoicing legal basis summary <https://www.eurojuris.fr/articles/facturation-electronique-calendrier-obligations-43703.htm>; PPF refocusing <https://next.ink/154911/facture-electronique-bercy-revoit-les-ambitions-du-portail-public-de-facturation/>, <https://hayot-expertise.fr/blog/portail-public-de-facturation>; penalties <https://www.journaldeleconomie.fr/facturation-electronique-la-loi-de-finances-a-triple-les-amendes-sans-prevenir-voici-ce-que-vous-risquez-concretement-des-septembre-2026/>, <https://www.fiducial.fr/facturation-electronique/faq/sanctions-non-conformite-obligation-facturation-electronique>; go-live <https://www.presse-citron.net/facturation-electronique-entre-vigueur-aujourdhui-voici-149-plateformes-agreees-france/>, <https://siecledigital.fr/2026/08/27/facturation-electronique-face-aux-craintes-de-piratage-le-gouvernement-veut-rassurer/>; FEC naming <https://www.lafabriquedunet.fr/logiciels/tendances/fichier-des-ecritures-comptables-fec>

---

## 10. Open questions

1. No official (DGFiP/INSEE) statement of the numeric TVA key formula or of the alphanumeric-key algorithm was found. Both rest on python-stdnum and consistent secondary sources (§1.2). Also unknown: when alphanumeric keys are issued, and whether the "12" constant has an official definition.
2. The SIREN/SIRET Luhn rule and the La Poste exception were confirmed only through Wikipedia (citing INSEE) and python-stdnum (§1.3).
3. Who must print the four new invoice mentions, and from when: everyone from 1 Sep 2026 (DGFiP FAQ) or by issuance wave (service-public)? (§3)
4. Exact legal references: art. 91 LF 2024 (the calendar), the loi de finances 2025 and loi n° 2026-103 (LF 2026) articles on e-invoicing, penalties (art. 1737) and cash registers (art. 125), the décret that adopted the "plateforme agréée" name, and the LF 2025 / LF 2026 franchise texts. Légifrance was unreachable (§4, §5, §7).
5. E-reporting frequencies and deadlines for réel simplifié, quarterly réel normal and mini-réel filers; the DGFiP table *Fréquences et délais* was not read (§4.3).
6. How EDI-TVA / EFI distinguishes the base and the tax for a cadre B line that carries a single 4-digit box code, and what it takes to become an EDI partner for CA3 transmission (§6).
7. The CA3 due day (15th–24th): how the tax office assigns it to a business (§6.1).
8. The 1 Jan 2014 rate change law references (LFR 2012 art. 68 / LF 2014) and the start date of 19.6% (§2.1).
9. Corse: treatment of goods and services that have no special rate in art. 297 (mainland 5.5% and 10% items) (§2.1).
10. The current edition of AFNOR XP Z12-012 (May 2025?) and the exact Factur-X EXTENDED-CTC-FR profile requirements; the SIRET scheme ID used in e-invoices (§1.4, §4.2).
11. Whether recording B2C cash takings in KhataOS brings it within art. 286 I 3° bis (certified cash-register software), and the current penalty (§7).
12. The FEC field names `CompAuxNum` vs `CompteAuxNum`, and the A47 A-1 naming and encoding rules, to be checked against the arrêté text (§7).

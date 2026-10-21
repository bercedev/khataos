# United Kingdom Compliance Requirements (VAT): KhataOS Spec

> **Status as of:** 2026-10-20. **Scope:** VAT-registered UK businesses (companies, partnerships, sole traders) in Great Britain and Northern Ireland keeping books in KhataOS, a local-first desktop accounting app. The `khata-gb` pack implements the VAT items below (see `docs/uk-vat.md`).
> **Not legal advice.** This file turns HMRC guidance, legislation and HMRC's API specification into software requirements. Get a UK chartered accountant or chartered tax adviser to confirm each item before release.
>
> **Confidence tags:**
> - **[PRIMARY]** means confirmed against the legislation (legislation.gov.uk), HMRC guidance or notices on GOV.UK, an HMRC internal manual, HMRC's own (archived) developer-forum material, or the HMRC Developer Hub API specification.
> - **[SECONDARY]** means reported consistently by reputable non-HMRC sources (Wikipedia citing HMRC documents, open-source validators), but the HMRC text was not retrieved in this research.
> - **[UNCERTAIN]** means sources conflict, the item is interpretive, or the point was not found. Flagged for review.

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **VAT registration number (VRN) validation**: 9 digits with two HMRC check digits (modulus 97, or the newer "97-55" series), 12-digit branch numbers, GD/HA government and health-authority numbers, `GB` / `XI` prefixes | HMRC VAT Software Developer Forum, 10 Mar 2009 (§1.2) |
| 2 | **VAT rates** 20% / 5% / 0% from an effective-dated master; exempt and outside-the-scope kept apart from zero-rated | VATA 1994 s2, s29A, s30, s31; VAT Notice 700 §3 |
| 3 | **VAT invoices** with every reg 14 particular, a sequential number from one or more series, issued within 30 days; simplified invoices up to £250 | VAT Regulations 1995 regs 13, 14, 16; Notice 700 §16; Notice 700/21 §4 |
| 4 | **The 9-box VAT return** from the books (Boxes 1 to 5 in pounds and pence, 6 to 9 in whole pounds) | Notice 700/12 §3; VAT (MTD) API spec |
| 5 | **Making Tax Digital**: digital records, digital links, submission through the VAT (MTD) API with OAuth 2.0 and fraud-prevention headers | Notice 700/22; HMRC Developer Hub |
| 6 | Northern Ireland: `XI` prefix for goods trade with the EU under the Windsor Framework; Boxes 2, 8, 9 are NI-only | GOV.UK register-for-vat NI page; Notice 700/12 §3.3, §3.9, §3.10 |
| 7 | Records kept **6 years** | Notice 700/21 §2.4 |

---

## 1. VAT (Value Added Tax Act 1994; VAT Regulations 1995)

### 1.1 Registration and identity

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Registration threshold**: register when taxable turnover goes above **£90,000**; may deregister at or below £88,000 | GOV.UK *How VAT works: VAT thresholds* [PRIMARY] (the 1 Apr 2024 start date was not on the page retrieved [UNCERTAIN]) | Current | Rolling 12-month taxable-turnover monitor (future work) |
| **VRN format**: 9 digits, written "123 4567 89", usually shown with the country prefix **GB** | Notice 700 §16.7 example "VAT No: 987 6543 21" [PRIMARY]; HMRC Developer Hub VAT (MTD) API: `vrn` "A nine-digit number" [PRIMARY] | 1 Apr 1973 | Accept with or without the prefix and with spaces; store the digits (and the prefix if typed) |
| **Check digits**: the VRN is "seven digit numbers with two check digits attached (derived using Modulus 97)"; a new series used "modulus 9755"; "the moduli would both be applied, first Mod 97 and, if that failed, Mod 9755. A 'fail' of both checks would indicate a spurious VRN." | HMRC, *Minutes of VAT Software Developer Forum*, 10 Mar 2009, item 6 (archived: `web.archive.org/web/20121012000313/http://www.hmrc.gov.uk/ebu/dev-forum-mins100309.pdf`) [PRIMARY] | New series from Nov 2009 [SECONDARY: Wikipedia citing the same HMRC forum] | Implement both checks (see §1.2) |
| **Algorithm** (HMRC slides "VAT Registration Numbers — New series", Stan Quayle, same forum): multiply the first seven digits by **8, 7, 6, 5, 4, 3, 2**; take the sum of products; "if sum > 97 subtract 97, repeat"; "subtract result from 97 = check digits". The revised **Modulus 97-55** adds 55 to the sum first. HMRC's worked example: **562 2359 45** (Mod 97) and **562 2359 87** (Mod 9755) | HMRC slide deck *VAT Forum 10 March 2009.ppt* in `dev-forum-slides100309.zip` (archived at `web.archive.org/web/20121012000326/http://www.hmrc.gov.uk/ebu/dev-forum-slides100309.zip`) [PRIMARY]; the "add 55" reading is confirmed by HMRC's own example (149 + 55 = 204; 204 − 2×97 = 10; 97 − 10 = 87), not by explicit wording [PRIMARY by example] | 2009 | See §1.2 |
| Some sources word the 97-55 rule as "55 is subtracted" | Wikipedia *VAT identification number* citing HMRC slides [SECONDARY]; python-stdnum accepts remainders 0, 42 and 55 [SECONDARY] | — | **Not followed**: HMRC's own example 562 2359 87 only passes the "add 55" form. KhataOS accepts remainder 0 (Mod 97) or 42 (Mod 97-55) only [UNCERTAIN whether any real VRN uses the "subtract" form] |
| **Branch / group traders**: 12 digits (the 9-digit VRN followed by a 3-digit branch identifier) | Wikipedia (no inline citation) [SECONDARY]; python-stdnum [SECONDARY] | — | Accept 12 digits; check digits on the first 9; expose the 9-digit VRN |
| **Government departments** `GD` + 3 digits 000–499; **health authorities** `HA` + 3 digits 500–999 | Wikipedia citing HMRC *V1-28 Registration Vol 1* (archived) [SECONDARY] | — | Accept, without a check digit (none exists for these) |
| The HMRC example VRN on Notice 700's sample invoice ("987 6543 21") does **not** pass either check | Calculated here: 9·8+8·7+7·6+6·5+5·4+4·3+3·2 = 238; Mod 97 gives 53, Mod 97-55 gives 95 | — | Example numbers in guidance are illustrative; do not use them as test data |
| A check-digit pass only shows the number is well-formed. Whether it is registered is confirmed online (HMRC *Check a UK VAT number* service / API) | HMRC forum minutes ("spurious VRN") [PRIMARY]; validator sites [SECONDARY] | — | Optional online check is future work |
| **Northern Ireland `XI` prefix**: "You need a VAT registration number starting with XI to trade under the Windsor Framework"; after HMRC's confirmation email, "start using the XI prefix before your usual VAT registration number" ("XI 123456789 instead of GB 123456789"); use it "on all documentation" with EU customers and suppliers. Eligible when goods are located in NI at the time of sale, received in NI from VAT-registered EU businesses, or sold/moved from NI to the EU | GOV.UK *Register for VAT: selling or moving goods between Northern Ireland and the EU* [PRIMARY]; VAT Regulations 1995 reg 14(2)(b): "the letters "XI" as a prefix to his registration number" on invoices to a person in an EU member state [PRIMARY] | 1 Jan 2021 | Accept `XI` + the same 9 digits; the XI number is the same VRN. Treat a party with an XI number as NI for goods (Boxes 8/9) |
| Notice 700/21 §4.2 still says NI businesses invoicing EU persons put "the letters 'GB' in front of your registration number" | GOV.UK Notice 700/21 [PRIMARY, apparently stale] | — | Follows reg 14(2)(b) (`XI`); flag Notice 700/21 §4.2 as inconsistent [UNCERTAIN] |

### 1.2 VRN check digits — the algorithm as implemented

For a 9-digit VRN `d1 … d9` (and the first 9 digits of a 12-digit branch number):

1. `S = 8·d1 + 7·d2 + 6·d3 + 5·d4 + 4·d5 + 3·d6 + 2·d7`.
2. **Mod 97**: expected check `C97 = (97 − S mod 97) mod 97` (HMRC: "subtract 97 until ≤ 97, subtract result from 97"; a remainder of 0 gives 97 − 97 = 00).
3. **Mod 97-55**: expected check `C9755 = (97 − (S + 55) mod 97) mod 97`.
4. The number is valid when `10·d8 + d9` equals `C97` or `C9755`. Both expected values are 00–96, so check digits 97–99 never pass.

Equivalently, with weights 8,7,6,5,4,3,2,10,1 over all nine digits, the weighted sum mod 97 is 0 (Mod 97) or 42 (Mod 97-55).

Test vectors: HMRC's own 562 2359 45 (Mod 97) and 562 2359 87 (Mod 97-55) [PRIMARY]; 339 0727 47 (taxationweb.co.uk worked example) and 980 7806 84 (python-stdnum docstring), both Mod 97 [SECONDARY, verified by the algorithm].

### 1.3 Rates, exemption, scope

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Standard rate 20%** | VATA 1994 s2(1) "VAT shall be charged at the rate of 20 per cent" (words substituted by F(No.2)A 2010 s3(1), 27 Jul 2010) [PRIMARY]; Notice 700 §3.3.1 table: 20% from **4 January 2011** [PRIMARY]; GOV.UK *VAT rates* [PRIMARY] | **4 Jan 2011** (17.5% before) | `STANDARD` 20% from 2011-01-04 in the default rate list; earlier rates are not seeded |
| **Reduced rate 5%** ("for example, children's car seats and home energy") | GOV.UK *VAT rates* [PRIMARY]; Notice 700 §3.3.1: 5% from 1 Sep 1997 [PRIMARY]; VATA 1994 s29A and Sch 7A [PRIMARY, statutory reference] | 1 Sep 1997 | `REDUCED` 5% (seeded from 2011-01-04 alongside the others; earlier history is not needed for current books) |
| **Zero rate 0%** ("most food and children's clothes"); a zero-rated supply is still a taxable supply; "The zero rate has existed throughout that time" | GOV.UK *VAT rates*; Notice 700 §3.3.1, §3.5 [PRIMARY]; VATA 1994 s30, Sch 8 | 1 Apr 1973 | `ZERO` 0%. Zero-rated supplies keep input-tax recovery |
| **Exports of goods** outside the UK are zero-rated (with evidence conditions) | VATA 1994 s30(6), (8) [PRIMARY, statutory reference]; Notice 703 [not retrieved] | — | A sale to a party abroad is zero-rated; evidence conditions are not checked |
| **Exempt** ("such as postage stamps, financial and property transactions"): no VAT, and related input tax is generally not recoverable | GOV.UK *VAT rates*; Notice 700 §3.6 [PRIMARY]; VATA 1994 s31, Sch 9 | — | `EXEMPT` class (0%), told apart from zero-rated by name, as in `khata-ae` |
| **Outside the scope**: supplies by a non-taxable person, made outside the UK and Isle of Man, or not in the course or furtherance of business (also e.g. optional service charges, §8.14) | Notice 700 §3.2 [PRIMARY] | — | Not a rate class: such items should not be on a VAT invoice. Not modelled (no "outside scope" treatment yet) |
| **Isle of Man** is in the same VAT area; IoM registrations share the 9 and 12-digit formats | Notice 700 §3.2, §4.7 [PRIMARY]; Wikipedia citing IoM Government [SECONDARY] | — | IoM VRNs validate the same way; IoM is not a region in the pack |

### 1.4 VAT invoices

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| A VAT-registered person supplying standard or reduced-rated goods or services to another VAT-registered person "must give that person a VAT invoice", normally "within 30 days of the tax point" | Notice 700 §16.2.1, §16.2.3 [PRIMARY]; VAT Regulations 1995 reg 13 [PRIMARY, statutory reference] | — | Warn when a B2B invoice is issued more than 30 days after the tax point (future work) |
| **Full VAT invoice particulars** (reg 14(1)): (a) "a sequential number based on one or more series which uniquely identifies the document"; (b) the time of the supply; (c) the date of issue; (d) supplier's name, address and registration number; (e) recipient's name and address; (g) a description identifying the goods or services; (h) for each description, the quantity or extent, the rate of VAT and the amount payable excluding VAT; (i) the gross total excluding VAT; (j) the rate of any cash discount; (l) "the total amount of VAT chargeable, expressed in sterling"; (m) the unit price; (n) a reference to any margin scheme | VAT Regulations 1995 reg 14(1) on legislation.gov.uk (revised to 8 Nov 2021, no outstanding effects) [PRIMARY]; Notice 700/21 §4.1 [PRIMARY] | — | Invoice print template carries every field (print work is outside this pack) |
| **Invoice numbering**: sequential, from **one or more series**, uniquely identifying the document. No length or character set is prescribed | reg 14(1)(a) [PRIMARY] | — | KhataOS rule: unique across all years in the series (no reset is implied by the text), ≤ 35 characters of letters, digits, `-` and `/` (a KhataOS choice, as for the UAE) [UNCERTAIN: whether HMRC requires gap-free numbering is interpretive; KhataOS series are gap-free by construction] |
| **Simplified (less detailed) invoice** when the supply, VAT included, is **£250 or less**: supplier's name, address and VRN, time of supply, description, VAT-inclusive total and the rate for each rate; **exempt supplies must not be included** | Notice 700/21 §4.4, §4.5 [PRIMARY]; Notice 700 §16.6.1, §16.6.2 [PRIMARY]; reg 16 [PRIMARY, statutory reference]; HMRC manual VATREC16042 [PRIMARY] | — | Future work: a simplified-invoice layout gated on the £250 limit |
| **Modified invoice**: above £250, a full or modified VAT invoice "showing VAT inclusive rather than VAT exclusive values" (with the customer's agreement) | Notice 700/21 §4.4, §4.5 [PRIMARY]; VATF42421 [PRIMARY] | — | Future work |
| **Northern Ireland to EU** invoices: supplier's VRN with the `XI` prefix and the customer's EU VAT number with its country code | reg 14(2) [PRIMARY]; GOV.UK NI register page [PRIMARY] | 1 Jan 2021 | Print the stored prefix |
| **Rounding**: "You may round down the total VAT payable on all goods and services shown on a VAT invoice to a whole penny" (§17.5). At line level, round **down to the nearest 0.1p**, or **to the nearest 1p or 0.5p** — "Whatever you decide, you must be consistent" (§17.5.1). Per-unit amounts: 4 places rounded to 3, or nearest 1p / 0.5p; VAT on a standard or reduced-rated item must not be rounded down to nil (§17.5.2). Retailers "must not round the VAT figure down" (§17.6) | VAT Notice 700 §17.5–§17.6 [PRIMARY] | — | KhataOS rounds each line to the **nearest 1p** (allowed by §17.5.1), midpoint away from zero [UNCERTAIN: the notice does not say which way a half-penny goes]. `vat::document_vat` gives the invoice-level figure rounded **down** to the penny (§17.5) |

### 1.5 VAT return (the 9 boxes)

| Box | Definition (Notice 700/12, last updated 19 Dec 2025) | Source |
|---|---|---|
| 1 | "Include the VAT due on all goods and services you supplied in the period covered by the return." Also: VAT under the domestic reverse charge (customer), postponed import VAT | §3.2 [PRIMARY]; reverse charge / PVA: GOV.UK guidance pages [PRIMARY] |
| 2 | "the VAT due on all goods and related costs bought from VAT-registered suppliers in EU member states" — "Since 1 January 2021, you're only allowed to make acquisitions on goods you bring into Northern Ireland from the EU." | §3.3 [PRIMARY] |
| 3 | "the total VAT due, that is, boxes 1 and 2 added together" | §3.4 [PRIMARY] |
| 4 | "the total amount of deductible VAT charged on your business purchases" | §3.5 [PRIMARY] |
| 5 | "deduct the number in box 4 from the number in box 3"; if box 4 is larger, HMRC repays. No minus sign on paper | §3.6, §3.1 [PRIMARY] |
| 6 | "the total value of all your business sales and other specific outputs but leave out any VAT" — **includes zero-rated, reduced-rate and exempt supplies, exports and reverse-charge transactions**; excludes capital introduced, loans, dividends, gifts of money, insurance claims | §3.7 [PRIMARY] |
| 7 | "the total value of your purchases and expenses but leave out any VAT" — includes imports, NI acquisitions, reverse charge; excludes wages, PAYE/NI, drawings, loans, dividends, insurance claims, MOT, vehicle licence duty, rates, and anything outside the scope of VAT | §3.8 [PRIMARY] |
| 8 | "the total value of all supplies of goods to EU member states and directly related costs" — goods dispatched **from Northern Ireland** to the EU from 1 Jan 2021; also included in Box 6; excludes separately invoiced services | §3.9 [PRIMARY] |
| 9 | "the total value of all acquisitions of goods from VAT-registered suppliers in EU member states" — **into Northern Ireland** from 1 Jan 2021; also included in Box 7 | §3.10 [PRIMARY] |

**Pounds and pence.** The VAT (MTD) API defines Boxes 1–5 as "a monetary value (to 2 decimal places)" and Boxes 6–9 as "a monetary value (to 2 zeroed decimal places)" (whole pounds), `netVatDue` (Box 5) as "The absolute difference between the totalVatDue and vatReclaimedCurrPeriod … a positive number … HMRC will automatically determine whether the number represents net VAT that is due or net VAT that can be reclaimed", with minimum 0 [PRIMARY: HMRC Developer Hub OAS `vat-api` 1.0, schema `VATReturnDeclaration`]. How to get to whole pounds is not stated: KhataOS **truncates the pence** (drops them toward zero), the long-standing paper-return instruction [UNCERTAIN: the instruction was not on the current Notice 700/12 page; HMRC does not penalise either way for pence].

**Due date and periods.** "You usually need to send a VAT Return to HMRC every 3 months"; deadline "one calendar month and 7 days after the end of an accounting period", payment the same [PRIMARY: GOV.UK *VAT Returns*; Notice 700/12 §5]. "The standard tax period is 3 months" (Notice 700 §20.3.1); monthly and non-standard periods on application (§20.3.3) [PRIMARY]. Annual Accounting: one return a year, due **2 months** after the period (Notice 700/12 §5) [PRIMARY]. In MTD the period comes from the obligations API as a 4-character `periodKey` (e.g. `18A1`, occasionally with `#`) [PRIMARY: OAS].

**Error correction.** Net errors up to **£10,000**, or up to **1% of Box 6** (max **£50,000**), may be corrected on the next return (Method 1); otherwise report separately (Method 2); 4-year time limit [PRIMARY: Notice 700/45 §4.2–§4.7]. Not modelled: the return is worked out from the period's documents only.

### 1.6 Making Tax Digital for VAT (MTD)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Businesses above the threshold: digital records and software submission for VAT periods starting on or after **1 Apr 2019**; **all** VAT-registered businesses from their first period starting on or after **1 Apr 2022**; HMRC now signs new registrations up automatically | GOV.UK MTD for VAT collection and news [PRIMARY as reported by search; pages not fetched in full → SECONDARY]; Notice 700/22 §1.2 "all VAT-registered businesses to keep records digitally" [PRIMARY] | 1 Apr 2019 / 1 Apr 2022 | KhataOS is the digital record |
| **Digital records**: business name, principal place of business, VRN, VAT schemes used; for each supply the time of supply, value excluding VAT and rate | Notice 700/22 §3.3.1–§3.3.2 [PRIMARY] | — | Already in the books (vouchers, company profile) |
| **Digital links**: data must move between programs electronically (linked cells, XML/CSV import/export, API); "cut and paste" or "copy and paste" and manual re-keying are not digital links | Notice 700/22 §3.2.1 [PRIMARY] | — | The return comes straight from the books; the MTD JSON export is a file transfer (a digital link) |
| **Functional compatible software** must record and preserve digital records, provide returns to HMRC from them through the API platform, and receive information from HMRC | Notice 700/22 §3.2 [PRIMARY] | — | **Submission is future work** |
| **Submit endpoint**: `POST /organisations/vat/{vrn}/returns`, body `VATReturnDeclaration` with `periodKey` (4 chars), `vatDueSales`, `vatDueAcquisitions`, `totalVatDue` (= sales + acquisitions), `vatReclaimedCurrPeriod`, `netVatDue` (≥ 0), `totalValueSalesExVAT`, `totalValuePurchasesExVAT`, `totalValueGoodsSuppliedExVAT`, `totalAcquisitionsExVAT`, `finalised` (must be `true`, else 403 `NOT_FINALISED`). Errors `VAT_TOTAL_VALUE`, `VAT_NET_VALUE`, `INVALID_MONETARY_AMOUNT`. Range ±9,999,999,999,999.99 (boxes 1–4), 0–99,999,999,999.99 (box 5), ±9,999,999,999,999 (6–9) | HMRC Developer Hub, VAT (MTD) API 1.0 OAS (`/api-documentation/docs/api/service/vat-api/1.0/oas/file`) [PRIMARY] | — | `khata_gb::mtd` renders this body (no network) |
| **Accept header** `application/vnd.hmrc.1.0+json`; money "with two decimal places and GBP currency" unless documented | HMRC Developer Hub reference guide [PRIMARY] | — | Future work (client) |
| **OAuth 2.0** authorisation-code flow, scopes `write:vat` / `read:vat`; sandbox `test-api.service.hmrc.gov.uk`, production `api.service.hmrc.gov.uk` | OAS `securitySchemes` [PRIMARY] | — | Future work |
| **Fraud-prevention headers** are required by law; for a desktop app connecting directly (`Gov-Client-Connection-Method: DESKTOP_APP_DIRECT`) all of: `Gov-Client-Device-ID`, `-Local-IPs`, `-Local-IPs-Timestamp`, `-MAC-Addresses`, `-Multi-Factor`, `-Screens`, `-Timezone`, `-User-Agent`, `-User-IDs`, `-Window-Size`, `Gov-Vendor-License-IDs`, `Gov-Vendor-Product-Name`, `Gov-Vendor-Version`. Repeated bad data can lead to fines and API blocking | HMRC Developer Hub fraud-prevention guide and *Desktop application direct* page [PRIMARY]; legal basis linked as SI 2019/360 [PRIMARY link, text not read] | — | Future work; KhataOS is a desktop app (DESKTOP_APP_DIRECT, or VIA_SERVER if a relay is used) |

### 1.7 Schemes and special regimes (pointers — not modelled)

| Item | Source | What KhataOS does |
|---|---|---|
| **Flat Rate Scheme**: join at expected taxable turnover ≤ £150,000; leave above £230,000 | GOV.UK *VAT Flat Rate Scheme: eligibility* [PRIMARY]; Notice 733 [not retrieved] | Not modelled: the return uses standard accounting |
| **Cash Accounting Scheme**: ≤ £1.35m to join, leave above £1.6m | GOV.UK *VAT Cash Accounting Scheme: eligibility* [PRIMARY]; Notice 731 | Not modelled: VAT is by invoice date (tax point), not payment |
| **Annual Accounting Scheme**: ≤ £1.35m; one return a year, due 2 months after; advance payments | GOV.UK *VAT Annual Accounting Scheme* [PRIMARY]; Notice 732; Notice 700/12 §5 | Not modelled (any period can still be computed) |
| **Postponed import VAT accounting (PVA)**: import VAT in Box 1, reclaim in Box 4, in the period of import, from the monthly postponed import VAT statement | GOV.UK *Completing your VAT Return to account for import VAT* [PRIMARY] | Not modelled: imports carry no VAT in the books unless posted |
| **Domestic reverse charge for building and construction services (CIS)**, from **1 Mar 2021**: the customer accounts for the VAT (Box 1 and Box 4); the supplier puts the net value in Box 6 | GOV.UK *Check when you must use the VAT domestic reverse charge for building and construction services*; Revenue and Customs Brief 7 (2020); Notice 735 [PRIMARY] | Not modelled |
| Other domestic reverse charges (mobile phones and computer chips, wholesale gas/electricity, emissions allowances…) | Notice 735 [not retrieved in detail] | Not modelled |
| Partial exemption, capital goods scheme, margin schemes, bad-debt relief, blocked input tax (business entertainment, cars) | Notice 700 [not retrieved in detail] [UNCERTAIN] | Not modelled: Box 4 takes all VAT on purchases as deductible |
| Northern Ireland goods trade with the EU (acquisitions, dispatches, EC Sales List) | Notice 700/12 §3.3, §3.9, §3.10; GOV.UK NI pages [PRIMARY] | Box 8 / Box 9 values from NI parties are worked out; acquisition VAT (Box 2) is not |

---

## 2. Accounting periods, currency, records

| Requirement | Source | What the software must do |
|---|---|---|
| **Company year**: a company's accounting reference date (ARD) is "the last day of the month in which the anniversary of their incorporation falls", and can be changed (form AA01). Any year end is possible | GOV.UK *Life of a company — Part 1: accounts* [PRIMARY]; Companies Act 2006 s391–s392 [statutory reference] | The books' first month is per company |
| **Corporation Tax financial year**: "A financial year starts on 1 April, and ends on the following 31 March", named after the calendar year it starts in | HMRC Company Taxation Manual CTM01405 (Interpretation Act 1978 Sch 1) [PRIMARY] | **Default books year: 1 April – 31 March** (the CT financial year, and a common company year end); companies change it to their ARD |
| **Tax year for individuals**: 6 April – 5 April | Income Tax Act 2007 s4 [statutory reference, not retrieved → SECONDARY] | Not the default: a sole trader can still pick April; 6 April starts are not supported by the month-based year model [UNCERTAIN whether needed] |
| **Currency**: sterling, 100 pence; the total VAT on an invoice must be "expressed in sterling" | reg 14(1)(l) [PRIMARY] | GBP, symbol "£", 2 decimals |
| **Record keeping**: "Generally, you must keep all your business records for VAT purposes for at least 6 years" | Notice 700/21 §2.4 [PRIMARY]; VATA 1994 Sch 11 para 6(3) [statutory reference] | Retention of 6 years (KhataOS never deletes posted vouchers) |
| **Companies House accounts** (pointers only): private companies file within 9 months of the ARD (public 6); first accounts within 21 months of incorporation; micro-entity thresholds for periods from 6 Apr 2025 (2 of: turnover ≤ £1m, balance sheet ≤ £500,000, ≤ 10 employees); from **1 Apr 2028** filing through software in **iXBRL** and micro-entities must deliver a profit and loss account | GOV.UK *Life of a company — Part 1* [PRIMARY] | Not modelled. Accounts frameworks FRS 102 / FRS 105 (FRC) are pointers only [not retrieved] |
| **Corporation Tax returns** (CT600 with iXBRL accounts and computations) | [not researched] | Not modelled |

---

## 3. Regions

The UK pack lists the four nations as address regions, with ISO 3166-2:GB codes (`GB-ENG`, `GB-SCT`, `GB-WLS`, `GB-NIR`) shortened to `ENG`, `SCT`, `WLS`, `NIR` [SECONDARY: ISO 3166-2:GB]. VAT rates do not differ by nation. **Northern Ireland matters for goods**: under the Windsor Framework NI follows EU VAT rules for goods, so NI-to-EU dispatches (Box 8) and EU-to-NI acquisitions (Box 2, Box 9) exist only for a business with goods in NI [PRIMARY: Notice 700/12 §3.3, §3.9, §3.10]. Scotland and Wales have devolved income-tax rates but no VAT difference.

---

## 4. Open items to verify before relying on the pack

1. Whether HMRC still publishes, or will provide on request, the full Mod 97 / 97-55 specification; the implementation follows the 2009 forum minutes and slides and HMRC's worked example (§1.1–1.2).
2. The GD/HA and 12-digit branch formats against a current HMRC source (only SECONDARY found).
3. Whole-pound conversion for Boxes 6–9: truncation (KhataOS) vs rounding.
4. Notice 700/21 §4.2 (`GB` prefix) vs reg 14(2)(b) (`XI` prefix) for NI → EU invoices.
5. The midpoint direction for rounding VAT to the nearest penny (§17.5.1 does not say).
6. Whether gap-free numbering is required, or only unique and sequential.
7. Current MTD fraud-prevention header specification version before building a client.

---

## 5. Source list

- VAT Notice 700 *The VAT guide*: <https://www.gov.uk/guidance/vat-guide-notice-700> (§3.2, §3.3.1, §3.5, §3.6, §16.2, §16.6, §16.7, §17.5, §17.6, §20.3)
- VAT Notice 700/12 *How to fill in and submit your VAT Return* (updated 19 Dec 2025): <https://www.gov.uk/guidance/how-to-fill-in-and-submit-your-vat-return-vat-notice-70012>
- VAT Notice 700/21 *Record keeping for VAT*: <https://www.gov.uk/guidance/record-keeping-for-vat-notice-70021>
- VAT Notice 700/22 *Making Tax Digital for VAT* (updated 1 Apr 2022): <https://www.gov.uk/government/publications/vat-notice-70022-making-tax-digital-for-vat/vat-notice-70022-making-tax-digital-for-vat>
- VAT Notice 700/45 *How to correct VAT errors*: <https://www.gov.uk/guidance/how-to-correct-vat-errors-and-make-adjustments-or-claims-vat-notice-70045>
- GOV.UK *VAT rates*: <https://www.gov.uk/vat-rates>; *VAT Returns*: <https://www.gov.uk/vat-returns>; *VAT thresholds*: <https://www.gov.uk/how-vat-works/vat-thresholds>
- GOV.UK schemes: <https://www.gov.uk/vat-flat-rate-scheme/eligibility>, <https://www.gov.uk/vat-cash-accounting-scheme/eligibility>, <https://www.gov.uk/vat-annual-accounting-scheme>
- GOV.UK *Register for VAT: selling or moving goods between Northern Ireland and the EU*: <https://www.gov.uk/register-for-vat/selling-or-moving-goods-between-northern-ireland-and-the-eu>
- GOV.UK *Completing your VAT Return to account for import VAT*: <https://www.gov.uk/guidance/complete-your-vat-return-to-account-for-import-vat>
- GOV.UK *VAT domestic reverse charge for building and construction services*: <https://www.gov.uk/guidance/vat-domestic-reverse-charge-for-building-and-construction-services>
- MTD for VAT collection: <https://www.gov.uk/government/collections/making-tax-digital-for-vat>
- VAT Act 1994 s2: <https://www.legislation.gov.uk/ukpga/1994/23/section/2>
- VAT Regulations 1995 reg 14: <https://www.legislation.gov.uk/uksi/1995/2518/regulation/14>
- HMRC manuals: VATREC16042 <https://www.gov.uk/hmrc-internal-manuals/vat-trader-records/vatrec16042>; VATF42421 <https://www.gov.uk/hmrc-internal-manuals/vat-fraud/vatf42421>; CTM01405 <https://www.gov.uk/hmrc-internal-manuals/company-taxation-manual/ctm01405>
- GOV.UK *Life of a company — Part 1: accounts*: <https://www.gov.uk/government/publications/life-of-a-company-annual-requirements/life-of-a-company-part-1-accounts>
- HMRC Developer Hub VAT (MTD) API: <https://developer.service.hmrc.gov.uk/api-documentation/docs/api/service/vat-api/1.0> (OAS: `/oas/file`); reference guide: <https://developer.service.hmrc.gov.uk/api-documentation/docs/reference-guide>; fraud prevention: <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/>, <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/connection-method/desktop-app-direct/>
- HMRC *Minutes of VAT Software Developer Forum*, 10 Mar 2009 (archived): <http://web.archive.org/web/20121012000313/http://www.hmrc.gov.uk/ebu/dev-forum-mins100309.pdf>; slides: <http://web.archive.org/web/20121012000326/http://www.hmrc.gov.uk/ebu/dev-forum-slides100309.zip>
- Secondary: Wikipedia *VAT identification number* <https://en.wikipedia.org/wiki/VAT_identification_number>; python-stdnum `stdnum/gb/vat.py` <https://github.com/arthurdejong/python-stdnum/blob/master/stdnum/gb/vat.py>; taxationweb *VAT points of practice* <https://www.taxationweb.co.uk/tax-articles/vat-a-excise-duties/vat-points-of-practice.html>

## 6. Addendum (21 Oct 2026): reverse charges, import VAT and the schemes

Checked on gov.uk for the VAT-pack work (`docs/uk-vat.md`); all [PRIMARY]
unless tagged. Box rules summarised in VAT Notice 700/12 §§3, 4
<https://www.gov.uk/guidance/how-to-fill-in-and-submit-your-vat-return-vat-notice-70012>.

### 6.1 Domestic reverse charge (Notice 735; construction)

- Notice 735 <https://www.gov.uk/guidance/the-vat-domestic-reverse-charge-procedure-notice-735>:
  scope §3.1 (mobile phones, computer chips, wholesale gas and electricity,
  emissions allowances, wholesale telecoms, renewable energy certificates,
  construction), only between UK-registered businesses. Supplier §7.7:
  value in Box 6, "no output tax to include in box 1". Customer §8.5:
  output tax in Box 1, net value "must not be entered in box 6", input tax
  in Box 4 under the normal rules, value in Box 7. De minimis §6.1: on an
  invoice basis, phones/chips of £5,000 or more (VAT-exclusive) per invoice,
  "no exclusion for the first £5,000"; §6.2 mixed invoices split; §6.3
  grouping only for one order over £5,000 by agreement. Invoice wording
  §7.6.1 ("reverse charge: VAT Act 1994 Section 55A applies" or "Customer
  to pay the VAT to HMRC").
- Construction: <https://www.gov.uk/guidance/vat-domestic-reverse-charge-for-building-and-construction-services>,
  technical guide <https://www.gov.uk/guidance/vat-reverse-charge-technical-guide>,
  VATREVCON31000. Customer VAT in Box 1 and Box 4; "no impact on … boxes 6
  and 7" (read with 700/12 §4.6: supplier Box 6, customer Box 7). Applies
  when the customer is VAT- and CIS-registered, the services are in CIS
  scope, standard or reduced rated, and no written end-user or intermediary
  notice was given; employment businesses supplying workers and items within
  a zero-rated supply excluded; 5% disregard by agreement, not per invoice.
  Reverse-charge supplies are "not to be accounted for under" the Flat Rate
  Scheme, and cash accounting cannot be used for them.

### 6.2 Imported services and import VAT

- Imported services (Notice 741A **§5**, not §15)
  <https://www.gov.uk/guidance/vat-place-of-supply-of-services-notice-741a>:
  applies when the place of supply is the UK and the supplier belongs
  outside it (§5.1; exempt services excluded §5.5). §5.2: Box 1 output tax
  on the full value, Box 4 input tax under the normal rules, the value in
  **Box 6 and Box 7**. Value includes taxes levied abroad (§5.12); exchange
  rate and time of supply refer to Notice 700 [UNCERTAIN: not extracted].
- Postponed VAT accounting
  <https://www.gov.uk/guidance/complete-your-vat-return-to-account-for-import-vat>:
  Box 1 the postponed import VAT, Box 4 the VAT reclaimed, Box 7 "total
  value of all imports of goods"; Box 6 not used. Estimates only when the
  statement is missing, adjusted next return. Cash accounting cannot be
  used for imports. Monthly statement (MPIVS) on CDS by the 10th working
  day, viewable 6 months, "download and keep a copy"
  <https://www.gov.uk/guidance/get-your-postponed-import-vat-statement>.
  H8 declarations produce no statement [UNCERTAIN: search summary only].

### 6.3 Flat Rate Scheme (Notice 733)

<https://www.gov.uk/guidance/flat-rate-scheme-for-small-businesses-vat-notice-733--2>
- Join: expected taxable turnover excluding VAT £150,000 or less (§3.1).
  Leave: income for the year over £230,000 including VAT (stay if the next
  12 months will not exceed £191,500) (§3.5); also if the next 30 days
  alone exceed £230,000 (§12.2(b)); check on each anniversary (§12.3).
- Flat rate turnover: "all the supplies your business makes, including
  VAT" (§6.2), including zero-rated and exempt income; excluded: bank
  interest, share income, non-business income, supplies outside the scope,
  goods not used in the business, capital goods sold where input tax was
  claimed (§6.3). Methods: basic (§8.1), cash-based (§9.1: "the VAT
  inclusive supplies for which you have been paid"), retailer's (§10); kept
  at least 12 months (§6.1).
- Boxes (700/12 §4.1): Box 1 flat rate × turnover plus reverse-charge
  output VAT; Box 4 only capital goods; Box 6 the turnover the rate applied
  to, **including VAT**, plus the VAT-exclusive value of supplies outside
  the scheme (including reverse-charge transactions) and Box 8; Box 7
  "usually no figure … unless you've bought a capital good costing more
  than £2,000", plus Box 9 and reverse-charge transactions; imports in Box 7
  for scheme users [UNCERTAIN; included].
- Capital goods: a single purchase of capital expenditure goods costing
  **£2,000 or more** including VAT (§15.2); 700/12 says "more than £2,000"
  [conflict: 733 followed]. Not several purchases, items under £2,000 or
  services.
- Imported services reverse charge: out of the flat rate turnover, Box 1
  and Box 4 as under normal accounting (§6.4). PVA imports for periods from
  1 June 2022: out of the turnover, import VAT added to Box 1 after the
  flat rate (§6.5). NI acquisitions: Box 2 at the standard rate, no
  recovery (§§6.4, 2.4).
- First year: 1% off in the first 12 months after VAT registration, not if
  registered more than 12 months late (§4.7).
- Limited cost business: 16.5% when relevant goods including VAT are under
  2% of flat rate turnover, or over 2% but under £1,000 a year (£250 for a
  quarter) (§4.4); relevant goods exclude capital goods, food and drink,
  and services (§4.6).
- Rates: <https://www.gov.uk/vat-flat-rate-scheme/how-much-you-pay> (54
  sectors, catering 12.5, hotels 10.5 and pubs 6.5 from 1 Apr 2022; lower
  2020-22 hospitality rates historical); trade-to-sector lookup FRS7200,
  FRS7300 <https://www.gov.uk/hmrc-internal-manuals/vat-flat-rate-scheme/frs7200>.
  The full table is in `crates/khata-gb/src/schemes.rs` (`SECTORS`).

### 6.4 Cash Accounting Scheme (Notice 731)

<https://www.gov.uk/guidance/vat-cash-accounting-scheme-notice-731>
- Join at expected taxable supplies £1.35m or less with no outstanding
  returns, offences or debt (§2.1); leave once 12-month supplies exceed
  £1.6m (§§2.5, 6.2; one-off exception §2.6).
- VAT on the date of payment: cash on receipt; giro, standing order, direct
  debit when the bank is credited; card on the voucher date; cheques on the
  later of receipt (or sending) and the cheque's date; dishonoured payments
  reversed (§§4.4, 4.5). Boxes 1, 4, 6 and 7 on payments (§4.7; 700/12
  §4.2: Box 6 "based on the payments you've had", Box 7 on payments made;
  Box 8 on supplies made).
- Allocation: "You must allocate the payment to the invoices in the order
  in which you issue or receive them" (§5.7); a lump sum clears the
  earliest invoice first (§8.2). Payments on account are not covered
  further [UNCERTAIN; oldest-first used]. A part payment is VAT-inclusive
  and carries the invoice's VAT in proportion: £780 of £1,200 with £200
  VAT is 65% → £130 (§8.1); mixed rates apportioned (§5.7; example §8.2,
  45% of £3,400 → £180 VAT); foreign currency the same (§5.11).
- Excluded: hire purchase, lease purchase, conditional and credit sale;
  imports and acquisitions (normal rules, import VAT in Box 4, 700/12
  §4.2); the domestic reverse charge; invoices not payable in full within 6
  months; invoices issued in advance of supply (§§2.7, 2.8, 5.5). Not with
  the Flat Rate Scheme (§2.8). Factoring §5.3, debt sale §5.4; bad debts
  §6.6; leaving §§6.4, 6.5.

### 6.5 Annual Accounting Scheme (Notice 732)

<https://www.gov.uk/guidance/vat-annual-accounting-notice-732>
- Join up to £1,350,000 taxable supplies (§2.1); leave over £1,600,000
  (§2.4). Interim payments: 9 of 10% of last year's liability in months 4
  to 12, or 3 of 25% in months 4, 7 and 10 (§§4.1, 4.2, 4.5); new
  registrants on an estimate; none for periods under 4 months (§3.4).
  Annual return "normally 2 months after the end of the annual accounting
  year" (§3.6); balancing payment = Box 5 − interim payments (700/12 §4.3).
  Combines with cash accounting and FRS (§§2.8, 9). No £2,000-style
  minimum found [UNCERTAIN]; electronic payment only [UNCERTAIN: search
  excerpt].

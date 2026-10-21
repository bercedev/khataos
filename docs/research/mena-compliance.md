# MENA Compliance Requirements (UAE and KSA): KhataOS Spec

> **Status as of:** 2026-10-19. **Scope:** SMEs in the **United Arab Emirates** (mainland and free-zone companies, sole establishments) and the **Kingdom of Saudi Arabia** keeping books in KhataOS, a local-first desktop accounting app. MENA is the second market after India (see `india-compliance.md`).
> **Not legal advice.** This file turns statutes, executive regulations and official technical specifications into software requirements. Get a UAE-registered tax agent and a Saudi chartered accountant to confirm each item before release. Re-check the UAE MoF e-invoicing page, FTA public clarifications and ZATCA's e-invoicing pages every month until UAE go-live (1 Jan 2027). Both regimes are changing quickly.
>
> **Confidence tags:**
> - **[PRIMARY]** means confirmed against the decision, regulation or official technical specification text (UAE MoF / FTA, ZATCA, OpenPeppol PINT AE).
> - **[SECONDARY]** means reported consistently by reputable professional sources (Big 4, law firms, established vendors), but the official text was not retrieved in this research.
> - **[UNCERTAIN]** means sources conflict, the item is announced but its text was not found, or it is interpretive. Flagged for review.

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **Tax-ID validation**: UAE TRN (15 digits; first 10 = TIN; no published checksum), KSA VAT number (15 digits, first and last digit "3"; 11th digit "1" for VAT groups). Format check offline; online verification is optional | MoF e-invoicing guidelines; ZATCA BR-KSA-40/44 |
| 2 | **UAE VAT 5% / KSA VAT 15%** with effective-dated tax-category masters (standard, zero, exempt, out of scope, reverse charge, margin scheme), designated-zone flags (UAE) and VATEX exemption codes (KSA) | UAE VAT Law Arts 43, 45, 46, 48; ZATCA XML standard §11.2.4 |
| 3 | **Tax invoice and simplified tax invoice templates** with every mandatory field, AED/SAR tax amounts whatever the document currency, the words "Tax Invoice" / "Tax Credit Note", and **Arabic text on KSA invoices** | UAE VAT ER Arts 59, 60 (amended by CD 100/2025, CD 149/2026); KSA VAT IR Arts 53, 54 |
| 4 | Credit/debit notes linked to original invoices, within the deadlines (UAE: e-credit note within 14 days; KSA: within 15 days after month end) | MD 243/2025 Art 6; KSA IR (amended Apr 2025) |
| 5 | **UAE VAT201** and **KSA VAT return** computation, with the emirate split (UAE Box 1a to 1g) and the citizen healthcare/education line (KSA) | FTA EmaraTax; ZATCA return form |
| 6 | **UAE FTA Audit File (FAF)** export (company info, supply listing, purchase listing, general ledger) | FTA FAF guidance |
| 7 | **UAE e-invoicing (PINT AE)**: UBL 2.1 Invoice/CreditNote XML generation, UUID, 8-flag transaction type, `0235:` participant IDs, hand-off to the customer's **Accredited Service Provider (ASP)**, status tracking, 14-day issue deadline. Mandatory for **revenue ≥ AED 50m from 1 Jan 2027**, everyone else from **1 Jul 2027** | MD 243/2025, MD 244/2025 (as amended 2026), PINT AE 1.0.4 |
| 8 | **KSA Fatoora Phase 2**: per-device (EGS) key pair + CSR + CSID onboarding, XAdES signing, invoice counter (ICV), previous-invoice-hash (PIH) chain, TLV QR (tags 1 to 9), **clearance** for B2B before delivery, **reporting** of B2C within 24 hours, offline queue. **Wave 25 brings SMEs above SAR 187,500 in by 1 Feb 2027** | ZATCA XML Implementation Standard v1.2; Security Features Standard v1.2; Detailed Technical Guideline v2 |
| 9 | **Immutable records**: no deletion or edit of issued invoices; the India audit-trail engine is reused for both countries | KSA e-invoicing controls; UAE TPL record-keeping; MD 243 Art 11 |
| 10 | Retention engine: UAE 5 years (7 or 15 for real estate, 7 for Corporate Tax); KSA 6 years (11 for capital assets, 15 for real estate) | UAE TP ER Art 3, CT Law Art 56; KSA IR Art 66 |
| 11 | **UAE Corporate Tax** support: 9% above AED 375,000, Small Business Relief flag (revenue ≤ AED 3m, now to 2029), IFRS / IFRS for SMEs / cash-basis selector, related-party and connected-person tagging for TP disclosure, 9-month return calendar | FDL 47/2022; MD 73/2023, MD 131/2026, MD 114/2023, MD 97/2023 |
| 12 | Arabic RTL rendering, bilingual (Arabic + English) invoice layouts, Arabic party names in masters | KSA IR Art 53; FAF Arabic name field; PINT AE bilingual support |

---

## 1. UAE: VAT (Federal Decree-Law No. 8 of 2017, as amended)

### 1.1 Registration and identity

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Mandatory registration** when taxable supplies + imports exceed **AED 375,000** in the previous 12 months, or are expected to in the next 30 days. **Voluntary** registration from **AED 187,500** (supplies or expenses) | VAT Law Arts 13, 17; ER Art 7 [SECONDARY] | 1 Jan 2018 | Rolling 12-month and 30-day forward taxable-turnover monitor per entity, with an alert at 80% and 100% of each threshold. Exclude exempt and out-of-scope supplies from the measure |
| **TRN** is a 15-digit number issued by the FTA. The **TIN is the first 10 digits of the TRN**. Each tax-group member has its own TIN (the first 10 digits of its *own* TRN, not the representative's) | MoF *UAE Electronic Invoicing Guidelines* v1.1 (1 Jun 2026), §2 and §3 [PRIMARY] | Now | Store TRN and derived TIN separately on every party. Validation: exactly 15 digits, numeric. Warn (do not block) when it does not start with "100", which is common but not a published rule [SECONDARY] |
| **TRN checksum** | No FTA publication found [UNCERTAIN] | none | **Do not implement a checksum.** Offer an optional "verify on FTA" action (EmaraTax TRN verification) when online, and store the verification date and result |
| A person in scope of e-invoicing who is not registered for any tax must register with the FTA to obtain a TIN | MoF guidelines §2 [PRIMARY] | Before own e-invoicing phase | Allow a party profile with TIN but no TRN (non-VAT-registered business) |

### 1.2 Rates, zones, reverse charge

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Standard rate **5%** | VAT Law Art 3 [SECONDARY] | 1 Jan 2018 | Rate from an effective-dated master, never hard-coded |
| **Zero-rated** (Art 45): exports of goods and services outside the implementing GCC states, international transport, qualifying means of transport, investment precious metals, first supply of residential buildings (within 3 years), qualifying education and healthcare, crude oil and natural gas. **Exempt** (Art 46): certain financial services, residential leases/sales (other than zero-rated), bare land, local passenger transport | VAT Law Arts 45, 46 [SECONDARY]; MoF guidelines §10.5 lists the same categories [PRIMARY] | 1 Jan 2018 | Separate tax categories: zero-rated (input tax recoverable) vs exempt (input tax not recoverable, feeds apportionment). Map them to PINT AE codes `Z` and `E` |
| **CD 149/2026** amends the ER: "medical products" replaces pharmaceuticals and medical equipment for zero-rating; composite-supply rule; employee-benefit input-tax limits; **input tax blocked on cash-paid supplies above a threshold the Minister will set**; **input-tax apportionment changes to an output (supply-value) method** | KPMG and PwC alerts; MoF news, 8 Sep 2026 [SECONDARY] | Most from **1 Oct 2026**; apportionment from the **first tax year starting after 1 Oct 2027** | Record payment mode on every purchase (cash vs other) so the cash-payment block can be applied once the threshold is published. Make the apportionment method pluggable per period (input-based to 2027, then supply-value based) |
| **Designated Zones**: supplies of goods within or between Designated Zones can be outside the scope if conditions are met; services are generally treated as supplied in the UAE; goods released to the mainland are treated as imports | VAT Law Art 51; ER Arts 50, 51; Cabinet Decision 59/2017 (list, amended) [SECONDARY] | 1 Jan 2018 | "Designated Zone" flag on own branches and party addresses. Tax determination uses (supplier zone, recipient zone, goods vs services). Note: a Free Zone is not automatically a Designated Zone, so keep the official list as a versioned master |
| **Import reverse charge** (Art 48): the recipient accounts for VAT on imported goods/services. **FDL 16/2025** removed the requirement to issue a **self-invoice** for reverse-charge imports used for business | FDL 16/2025 amending Art 48(1) [SECONDARY: KPMG, PwC, Baker McKenzie] | **1 Jan 2026** | RCM purchase vouchers post output VAT (Box 3) and input VAT (Box 10) together. Self-invoice is optional (keep the feature for evidential support when supplier documents are missing) |
| **Domestic reverse charge** on: electronic devices (CD 91/2023), precious metals and stones (CD 127/2024), crude/refined oil, natural gas, hydrocarbons (Art 48), **metal scrap (CD 153/2025)** | MoF guidelines §10.5.1 [PRIMARY]; scrap date [SECONDARY] | Scrap from **14 Jan 2026** | Item-level "RCM goods type" master. The sales invoice shows no VAT and must state the reverse-charge reason and goods type (PINT AE category `AE` and AE-GoodsType code). Capture the buyer's declaration where the decision requires one |
| **Profit margin scheme** (Art 43) | VAT Law Art 43; CD 149/2026 changes the purchase-price rule [SECONDARY] | 1 Jan 2018 / 1 Oct 2026 | Margin-scheme item flag; VAT shown as 0 on the e-invoice (MoF guidelines §10.4 scenario 3) |
| Excess recoverable input tax **lapses after 5 years** from the end of the period in which it arose. Refund claims must be filed within 5 years (transitional window to 31 Dec 2026) | FDL 16/2025 Art 74(3); FDL 17/2025 Art 38 [SECONDARY: KPMG] | 1 Jan 2026 | Track the age of the credit balance per period. Warn before lapse |
| Input tax denied where the supply is in a chain linked to tax evasion and the taxpayer "knew or should have known" | FDL 16/2025 Art 54 [SECONDARY] | 1 Jan 2026 | Keep supplier TRN verification evidence with purchases |

### 1.3 Tax invoices and credit notes

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Full tax invoice (ER Art 59(1))**: (a) the words "Tax Invoice" clearly displayed; (b) supplier name, address, TRN; (c) recipient name, address and TRN where registered; (d) sequential number or unique number that identifies the invoice and its order; (e) date of issue; (f) date of supply if different; (g) description of goods/services; (h) for each item: unit price, quantity, rate of tax and amount payable in AED; (i) any discount; (j) gross amount payable in AED; (k) tax payable in AED **and the exchange rate used** if another currency is used; (l) for reverse charge: a statement that the recipient must account for the tax, with the legal reference | ER Art 59(1) [SECONDARY: FTA public clarification VATP006 and multiple summaries; the consolidated ER text was not retrieved] | 1 Jan 2018; amended by CD 100/2024 (15 Nov 2024) and CD 100/2025 (late Sep 2025) | Validate every field before a voucher can be saved as "Tax Invoice". Per-line VAT and net values. AED totals even for foreign-currency invoices |
| **Simplified tax invoice (ER Art 59(2), (5))**: allowed where the recipient is **not registered**, or is registered and the consideration is **AED 10,000 or less**. Fields: "Tax Invoice", supplier name/address/TRN, date, description, total consideration and tax charged. Line items may be shown gross | ER Art 59(2), (5) [SECONDARY] | 1 Jan 2018 | Template variant. Block a simplified invoice to a registered recipient above AED 10,000 |
| **CD 100/2025** adds a clause to Art 59: certain invoice clauses stop applying when the invoice is issued as an electronic invoice, and the **simplified tax invoice provisions do not apply to electronic invoices** (mandatory or voluntary) | Crowe and Orbitax summaries; Bloomberg Tax (MoF posted 26 Sep 2025) [SECONDARY]; article scope varies across sources [UNCERTAIN] | ~29 Sep 2025 | For a customer in e-invoicing scope, always produce a full electronic tax invoice. Simplified invoices remain for B2C (B2C is outside e-invoicing, see §3) |
| **Tax invoice timing**: issue within **14 days** of the date of supply | VAT Law Art 67 [SECONDARY] | 1 Jan 2018 | Warn when the invoice date is more than 14 days after the delivery or service date |
| A person subject to e-invoicing must issue its tax invoice or tax credit note **in the form of** an electronic invoice / credit note | VAT Law Art 65(5) as amended, quoted in MoF guidelines §6.2 [PRIMARY for the guideline statement] | With each person's e-invoicing phase | Status model: once a company's phase starts, an invoice is "issued" only when it is handed to the ASP and confirmed (see §3) |
| **Tax credit note (ER Art 60)**: "Tax Credit Note" displayed (CD 149/2026 corrected this wording); supplier and recipient name/address/TRN; date; value of supply on the original invoice; corrected amount; difference; VAT on the difference; reason; original invoice reference. Issued within 14 days of the adjustment event | ER Art 60; CD 149/2026 Art 60(1)(a) [SECONDARY]; VAT Law Arts 62, 70 cited in MoF guidelines footnote 4 [PRIMARY for the citation] | 1 Jan 2018; wording change 1 Oct 2026 | CN voucher linked to one or many original invoices (PINT AE allows several preceding-invoice references and partial credit notes) |
| **Currency**: amounts may be in any currency, but VAT and the gross total must also be shown in **AED** at the **Central Bank of the UAE** rate | VAT Law Art 69, applied in MoF guidelines §12.2 items 16, 17 [PRIMARY] | 1 Jan 2018 | Exchange-rate master with source "CBUAE" and date. Persist the rate used on each invoice |
| **Language**: no rule found that requires Arabic invoices. English invoices are accepted in practice; the FTA can ask for an Arabic translation of records | Tax Procedures Law and FTA practice [SECONDARY]; PINT AE supports Arabic and English (MoF guidelines §5.1) [PRIMARY] | 1 Jan 2018 | English default, optional bilingual (Arabic + English) layout. Arabic legal name on the company and party masters |

### 1.4 Returns (VAT201)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Periods**: quarterly by default; **monthly** for annual turnover **≥ AED 150m**; the FTA may assign other periods. Return and payment are due **28 days** after the period end | FTA practice; FTA VAT Returns User Guide [SECONDARY] | 1 Jan 2018 | Period configured per entity from the EmaraTax profile (stagger). Calendar reminder |
| **Boxes**: **1a to 1g** standard-rated supplies by emirate (Abu Dhabi, Dubai, Sharjah, Ajman, Umm Al Quwain, Ras Al Khaimah, Fujairah; split by the fixed establishment that makes the supply); **2** tourist refunds (pre-filled); **3** reverse-charge supplies (imported services, goods not declared through customs); **4** zero-rated; **5** exempt; **6** imports through UAE Customs (pre-filled); **7** adjustments to Box 6; **8** total; **9** standard-rated expenses (recoverable input tax); **10** reverse-charge recovery; **11** total; **12** to **14** totals and net payable; **15** refund request (VAT311) | FTA guide and EmaraTax practice [SECONDARY]; box labels were not checked on the live portal [UNCERTAIN] | 2018; EmaraTax 2023 | Each tax category carries a box mapping. Emirate tag on each branch. Box 6 reconciliation against customs data import. Variance report (books vs return) |

### 1.5 Records, FAF, retention

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Retention**: **5 years** after the end of the tax period for taxable persons (5 years from the end of the calendar year for others); **7 years** for real-estate records; **+4 years** if there is a dispute, audit or notice of audit; **+1 year** after a voluntary disclosure filed in the 5th year | Cabinet Decision 74/2023 (Tax Procedures ER) Art 3, as restated in MoF guidelines §5.4 [PRIMARY for the guideline restatement] | 1 Aug 2023 | Retention engine with per-record holds (audit, dispute). **[UNCERTAIN]** the original VAT ER set **15 years** for real estate. Default to 15 years for real-estate entities until counsel confirms that the 7-year rule overrides it |
| **FTA Audit File (FAF)**: an invoice-level export the FTA can request. Sections reported by vendors: **Company information** (taxable person name in English and Arabic, TRN, tax agency name and TAN, tax agent name and TAAN, period start/end, file creation date, product/version, FAF version); **Supply listing** (customer name, TRN, country/emirate, invoice date/number, permit number, transaction ID, line number, description, value and VAT in AED, tax code, foreign-currency code, value and VAT in FCY); **Purchase listing** (same fields for suppliers); **General ledger** (date, account ID and name, description, counterparty, transaction ID, source document ID and type, debit, credit, balance); **totals** per section | Tally, Aviaan, Enerpize descriptions [SECONDARY]; the official FTA FAF specification was not retrieved, and exact field names [UNCERTAIN] | 2018 | Build the FAF export from the voucher and GL tables. Rounding notes where values are rounded. **Action:** get the FTA "FAF" guide from tax.gov.ae and pin the field list and format (CSV) with round-trip tests |
| **E-invoice storage**: e-invoices, e-credit notes and "associated data" must be stored "within the State" for the Tax Procedures Law period. The MoF reads this as: kept in a system that preserves integrity, can be produced promptly, and can be retrieved by the FTA in readable form, **whatever the server location** | MD 243/2025 Art 11; MoF guidelines §5.4 and Appendix 4 [PRIMARY] | With e-invoicing | Store the exact XML sent and received plus ASP and FTA confirmations, immutably, in the local database and in backups. Storage may be delegated to the ASP by contract, but the obligation stays with the business |

---

## 2. UAE: Corporate Tax (Federal Decree-Law No. 47 of 2022)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Applies to tax periods starting on or after **1 Jun 2023**. **Rates**: **0%** on taxable income up to **AED 375,000**, **9%** above | FDL 47/2022 Art 3; Cabinet Decision 116/2023 [SECONDARY] | 1 Jun 2023 | CT computation sheet from the P&L: accounting profit, adjustments (non-deductible items, 50% entertainment, interest cap), exempt income, then the rate bands |
| **Qualifying Free Zone Person (QFZP)**: 0% on qualifying income, 9% on the rest; de minimis non-qualifying revenue is the lower of 5% of revenue or AED 5m; audited accounts required | Cabinet Decision 100/2023, MD 265/2023 [SECONDARY] | 1 Jun 2023 | Revenue tagging per income stream (qualifying / non-qualifying / excluded). De minimis test report |
| **Domestic Minimum Top-up Tax 15%** for multinational groups with consolidated revenue ≥ €750m | FDL 60/2023; Cabinet Decision 142/2024 [SECONDARY] | FYs from 1 Jan 2025 | Out of SME scope. Flag "member of MNE group" on the company |
| **Small Business Relief (SBR)**: elect to be treated as having no taxable income when revenue is **≤ AED 3m** in the period and every previous period. Not for QFZPs or MNE-group members. **Extended to tax periods ending on or before 31 Dec 2029** | MD 73/2023 [SECONDARY]; extension by **MD 131/2026**, announced 7 Aug 2026 [SECONDARY: Khaleej Times, Entrepreneur ME, Farahat] | 1 Jun 2023; extension Aug 2026 | Revenue test per period with history. Election reminder for the CT return. Keep the cut-off date in data |
| **Tax period**: the financial year (normally 12 Gregorian months); change needs FTA approval | FDL 47/2022 Art 57 [SECONDARY] | 1 Jun 2023 | Company FY can be any 12-month period (not fixed to Jan to Dec, unlike India's Apr to Mar) |
| **Registration**: all resident juridical persons (including free-zone entities); natural persons whose business turnover exceeds **AED 1m** per Gregorian year; deadlines by licence month (FTA Decision 3/2024); late-registration penalty AED 10,000 | FTA Decision 3/2024; Cabinet Decision 10/2024 [SECONDARY] | 1 Mar 2024 | Store CT TRN on the company. Natural-person turnover monitor |
| **Return and payment**: within **9 months** of the end of the tax period | FDL 47/2022 Art 53 [SECONDARY] | First returns 2024-25 | Calendar. Exportable CT working papers |
| **Transfer pricing**: arm's-length principle (Arts 34 to 36). **TP Disclosure Form** with the return if related-party transactions exceed **AED 40m** in total (or **AED 4m** in one category); connected-person payments above **AED 500,000** disclosed. **Master File and Local File** if the entity's revenue is **≥ AED 200m** or it belongs to an MNE group with consolidated revenue **≥ AED 3.15bn** | MD 97/2023; MoF news release [SECONDARY]; disclosure-form thresholds from FTA practice [SECONDARY] | 1 Jun 2023 | Party flags "related party" and "connected person" (owner, director, relative). Related-party transaction report by category with running totals against AED 4m / 40m / 500k |
| **Accounting standard**: **IFRS**; **IFRS for SMEs** allowed when revenue is **≤ AED 50m**; **cash basis** allowed when revenue is **≤ AED 3m** (or with FTA approval) | MD 114/2023 [SECONDARY: PwC, DLA Piper] | 1 Jun 2023 | Company setting for the reporting framework and basis (accrual/cash). Cash basis only offered when the revenue test passes. Statement templates for IFRS and IFRS for SMEs |
| **Audited financial statements** required when revenue **exceeds AED 50m**, or for every QFZP | MD 82/2023 [SECONDARY; decision text not retrieved] | 1 Jun 2023 | Audit-pack export (TB, GL, schedules). "Audit required" indicator |
| **Record keeping: 7 years** after the end of the tax period | FDL 47/2022 Art 56 [SECONDARY] | 1 Jun 2023 | Retention engine: CT records 7 years (longer than the 5-year VAT rule) |

## 3. UAE: e-Invoicing (Electronic Invoicing System)

> **Legal basis:** Ministerial Decision (MD) **243/2025** (scope and obligations), **MD 244/2025** (phased implementation), MD 64/2025 (ASP accreditation), **Cabinet Decision 106/2025** (penalties), the amended VAT Law (Art 65), the Tax Procedures Law and the VAT ER. Technical standard: **Peppol PINT AE** (OpenPeppol). Official guidance: MoF *UAE Electronic Invoicing Guidelines* **v1.1, 1 Jun 2026**.

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Scope**: every person doing **Business** in the UAE, for every **Business Transaction** (B2B, B2G, G2B, G2G), **whether or not VAT-registered**. Commercial (non-tax) invoices are in scope too. Intra-VAT-group transactions are in scope | MD 243 Art 3; MoF guidelines §6 [PRIMARY] | Per phase | Invoice-type matrix: electronic Tax Invoice, electronic Tax Credit Note, Commercial Invoice, Electronic Credit Note, plus self-billed tax invoice and credit note (6 categories). No "provisional invoice" category: provisional bills are e-invoices too |
| **B2C excluded** until a further Ministerial decision. A person doing **only** B2C transactions is not in scope | MD 244 Art 5(2) [PRIMARY] | until further notice | Customer type (business / government / consumer) on the party master decides whether an e-invoice is required |
| **Exclusions**: sovereign government activities; international passenger air transport with e-ticket, and ancillary EMD services; **air cargo with airway bill for 24 months**; **VAT-exempt financial services** and exempt services exported at zero rate. FTA administrative exceptions for tax invoices **do not** carry over to e-invoices | MD 243 Art 4; MoF guidelines §7 [PRIMARY] | Per phase | "Excluded transaction" flag with reason code |
| **Intra-VAT-group grace period**: e-invoicing not required for transactions between members of the same VAT group for **24 months from 1 Jan 2027** | MoF guidelines §6.3.2.1 [PRIMARY] | 1 Jan 2027 to 31 Dec 2028 | Suppress e-invoicing for intra-group parties until 1 Jan 2029 (data-driven date) |
| **Timeline (MD 244 Art 5)**: pilot from **1 Jul 2026** (written opt-in); voluntary from **1 Jul 2026** (no penalties). **Revenue ≥ AED 50m**: appoint ASP by **31 Jul 2026** and go live **1 Jan 2027**. **Revenue < AED 50m**: appoint ASP by **31 Mar 2027**, go live **1 Jul 2027**. **Government entities**: ASP by 31 Mar 2027, live **1 Oct 2027**. "Revenue" = gross income of the latest accounting period per financial statements | MD 244 Arts 1, 3 to 5 [PRIMARY] | as stated | Company setting: e-invoicing phase and go-live date, derived from latest-FY revenue with an override. Readiness checklist screen |
| **Amendment (May 2026)**: ASP-appointment deadline for the ≥ AED 50m group **extended to 30 Oct 2026**; go-live unchanged at 1 Jan 2027. The amending decision number is reported as MD 56/2026 or MD 66/2026. One law firm gives **1 Oct 2027** as go-live for < AED 50m (MD 244 says 1 Jul 2027) | WAM news 10 May 2026; Gulf News; BSA Law 13 May 2026 [SECONDARY]; number and small-business date [UNCERTAIN]. Note: the MoF guidelines v1.1 (1 Jun 2026) still print 31 Jul 2026 | 10 May 2026 | Keep all phase dates in a versioned data file, not in code. **Action:** get the amending decision from mof.gov.ae before GA |
| **5-corner model**: C1 supplier sends invoice data to its ASP (C2); C2 validates, converts to PINT AE XML, sends to the buyer's ASP (C3) over Peppol and reports **Tax Data** to the FTA (C5); C3 validates, delivers to the buyer (C4) and also reports to C5; C5 confirms to C2 and C3; confirmations flow back to C1 and C4. **One ASP per person** for both sending and receiving | MoF guidelines §5.1, §6.1 [PRIMARY] | Per phase | KhataOS is **corner 1 / corner 4 software, not an ASP**. Build an ASP connector interface (outbound invoice data, inbound invoices, status and confirmation callbacks). MVP: export a valid PINT AE XML file and import ASP status/confirmation files. Direct ASP APIs come later, one adapter per ASP |
| **Format**: UBL 2.1 Invoice / CreditNote XML per **PINT AE** (current release **1.0.4**, with *PINT AE Self-Billing 1.0.4* and the *UAE Tax Data Document 1.0.4* for C5 reporting). **No QR code or barcode** on UAE e-invoices. Sample identifiers: ProfileID `urn:peppol:bis:billing`, CustomizationID `urn:peppol:pint:billing-1@ae-1` | MoF guidelines §5.3, §12.1 [PRIMARY]; docs.peppol.eu/poac/ae [PRIMARY] | Per phase | Generate UBL 2.1 XML and validate it locally with the official PINT AE Schematron before hand-off. Pin the PINT AE version per document. Do not print a QR on UAE invoices |
| **Mandatory header elements (1..1)**: CustomizationID (IBT-024), ProfileID (IBT-023), **invoice transaction type code** (BTAE-02, `cbc:ProfileExecutionID`), invoice number (IBT-001), **UUID** (BTAE-07), issue date, invoice type code, currency code, seller, buyer, tax total, document totals. The MoF publishes a separate mandatory-fields document; vendors count **51 mandatory fields** for a standard tax invoice | PINT AE syntax page [PRIMARY]; 51-field count [SECONDARY] | Per phase | Data model must hold every PINT AE BT/IBT/BTAE term. Map internal fields to the semantic IDs once and reuse the mapping for KSA (see §8) |
| **Invoice type codes**: `380` commercial/tax invoice, `480` invoice out of scope of tax (credit notes use the CreditNote document; codes such as `381` [SECONDARY]) | PINT AE code list UNCL1001-inv [PRIMARY] | Per phase | Type code chosen from document type + tax category |
| **Transaction type code**: an 8-character string of 0/1 flags: (1) Free trade zone, (2) Deemed supply, (3) Profit margin scheme, (4) Summary invoice, (5) Continuous supply, (6) Agent billing, (7) Supply through e-commerce, (8) Exports. Several can be 1 at once | PINT AE code list AE-TransType [PRIMARY] | Per phase | Derive the flags from voucher attributes (zone, deemed supply, margin item, summary period, recurring contract, agent, e-commerce channel, export) |
| **VAT category codes**: `S` standard, `Z` zero-rated, `E` exempt, `O` out of scope, `AE` reverse charge, `N` standard rate on an additional base not in document totals. Exemption reasons from AE-Exempt; RCM goods types from AE-GoodsType | PINT AE code lists [PRIMARY] | Per phase | Tax-category master carries the PINT AE code |
| **Participant ID** = `0235:` + 10-digit TIN. Reserved endpoints: **`0235:9900000097`** deemed supplies (report only), **`0235:9900000098`** buyer not yet on e-invoicing (a PDF/paper tax invoice must also be issued), **`0235:9900000099`** export buyer without Peppol ID | MoF guidelines §3, §10.2.2, §10.4 [PRIMARY] | Per phase | Party field "Peppol ID". Auto-fill the reserved endpoint when missing. When `…98` is used, also print the human-readable tax invoice |
| **Field rules**: TRN mandatory on electronic tax invoices and tax credit notes (not on commercial, exempt or out-of-scope); no line-count limit; **rounding only at invoice-total level to 2 decimals** (not per line or category); item type G/S/B; line VAT and amount payable in AED; "Tax accounting currency" (AED) and AED total mandatory when the document currency is not AED; advance payments use Paid Amount + preceding-invoice reference (or a final invoice for the balance only, Appendix 5); trade-licence **Authority Name** typed free-text; beneficiary details for Free Zone customers; **HSN codes optional for now**; no custom fields | MoF guidelines §10.4, §12.2, Appendix 5 [PRIMARY] | Per phase | Rounding engine per country pack. Advance-receipt vouchers produce their own tax invoice. Item master has an optional HS code |
| **Timing**: issue and transmit through the system **within 14 days** of the Date of Business Transaction (earlier of the transaction or the payment). Registrants must also meet the VAT Law invoice timeline. Reporting to the FTA "within the timeline prescribed by the Minister" | MD 243 Art 6(4) to (6), Art 1 [PRIMARY]; FTA reporting deadline not yet published [UNCERTAIN] | Per phase | "Pending e-invoice" queue with a 14-day countdown from min(supply date, payment date). Dashboard of unconfirmed documents |
| **E-credit note required** when the transaction is cancelled, the price is reduced, consideration is returned, or there is an administrative/numerical error | MD 243 Art 6(2) [PRIMARY] | Per phase | Cancelling an issued invoice always creates an e-credit note (no deletion) |
| **System failure**: issuer and recipient must notify the FTA **within 2 business days**. Also tell the ASP about changes to FTA-registered data within **5 business days** | MD 243 Arts 5(3), 12 [PRIMARY] | Per phase | Log ASP/connectivity outages with start time and show a "notify FTA" prompt with a draft. Re-send queued documents when service resumes |
| **Self-billing** only if both parties are VAT registrants and the buyer is on the system. **Agents** may issue on behalf of a principal | MD 243 Arts 8, 9; MoF guidelines §10.3 [PRIMARY] | Per phase | Self-billing agreement flag per supplier. Purchase-side self-billed invoice generation |
| **Penalties (CD 106/2025)**: failure to implement / appoint ASP on time AED 5,000 per month; each e-invoice or e-credit note not issued/transmitted on time AED 100 (max AED 5,000 per month); late system-failure notice AED 1,000 per day; late data-change notice to ASP AED 1,000 per day. Not applied to voluntary adopters | CD 106/2025 annex [PRIMARY] | From publication (Dec 2025) | Compliance dashboard showing exposure |

## 4. UAE: ESR and UBO (relevance check)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Economic Substance Regulations** apply only to financial years **1 Jan 2019 to 31 Dec 2022**; no notifications or reports for later years | Cabinet Decision 98/2024 (Gazette 16 Sep 2024) [SECONDARY] | 2024 | Nothing for the MVP |
| **Ultimate Beneficial Owner** register (Cabinet Decision 109/2023) is a company-secretarial filing with the licensing authority | [SECONDARY] | 2023 | Out of scope. Optional shareholder/UBO fields on the company profile help with related-party tagging for CT |

---

## 5. KSA: VAT (VAT Law + VAT Implementing Regulations, GCC VAT Agreement)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Standard rate **15%** (raised from 5%) | Royal Decree A/638 [SECONDARY] | 1 Jul 2020 | Effective-dated rate master (5% history from 1 Jan 2018) |
| **Registration**: mandatory above **SAR 375,000** annual taxable supplies; voluntary from **SAR 187,500** | VAT Law / IR Arts 3, 5, 6 [SECONDARY] | 1 Jan 2018 | Rolling 12-month turnover monitor (same engine as UAE) |
| **VAT registration number**: **15 digits, first and last digit "3"**. For VAT groups the **11th digit is "1"** and each member is identified by its 10-digit TIN | ZATCA XML Implementation Standard v1.2, BR-KSA-40, BR-KSA-44 [PRIMARY]; Security Features Standard v1.2, CSR table [PRIMARY] | Now | Hard validation on format (15 numeric, 3…3). VAT-group flag derived from digit 11. No checksum is published [UNCERTAIN], so do not invent one |
| **Tax invoice (IR Art 53)** for supplies to taxable persons, legal persons, establishments, intra-GCC supplies and exports. Fields: issue date (and supply date if different), unique sequential number, supplier name/address/VAT number, customer name/address/VAT number, description and quantity, unit price excluding VAT, discounts, VAT rate and VAT amount in **SAR**, VAT treatment wording (zero-rate, exemption, margin scheme). Issued **by the 15th of the month after the supply** | IR Art 53 (10th edition, Apr 2025) [SECONDARY: noqta.tn summary of the regulation; Arabic official text not retrieved] | 4 Dec 2021 (amended); Apr 2025 edition | Template + field validation. Under Phase 2 the B2B invoice must also be **cleared** before it reaches the buyer, which in practice means at issue |
| **Simplified tax invoice (IR Art 53(7))** for supplies to consumers and other persons; for business sales **under SAR 1,000** a simplified invoice *may* be issued (not for intra-GCC or exports). Fields: date, sequential number, supplier name/address/VAT number, description, total with VAT or a VAT-inclusive statement. Issued at the earlier of supply or payment | IR Art 53(1)(c), (7) [SECONDARY] | 4 Dec 2021 | B2C invoice type. POS flow |
| **Arabic**: invoice content visible to the customer must be in **Arabic**; other languages may be added alongside. ZATCA has cited Art 53(5) against all-English descriptions | Forum answers, vendor docs, ZATCA notice quoted by ClearTax [SECONDARY]; ZATCA XML standard: numbers in XML must use 0-9 digits, visual invoices may also show Eastern Arabic numerals (٠١٢٣٤٥٦٧٨٩) [PRIMARY] | 4 Dec 2021 | Arabic field next to every printable text (item names, party names, address, terms). Bilingual print templates with RTL layout and embedded Arabic fonts. Digits in XML always 0-9 |
| **Credit and debit notes** must be issued **within 15 days after the end of the month** of the event that requires them | Amended IR (ZATCA Board Resolution 01-06-24, Gazette 18 Apr 2025) [SECONDARY: EY]; sources differ on whether "month" is the event month or the next month [UNCERTAIN] | Apr 2025 | Deadline tracker on returns, discounts and price changes. Default to the stricter reading (end of the event month + 15 days) |
| Other April 2025 IR amendments: VAT-group eligibility (grace to 15 Oct 2025), TOGC notification by end of the next month, restricted input tax widened (food, hospitality, employee medical insurance unless mandatory), nominal-supply rules, refund minimum SAR 5,000 | EY alert 22 Apr 2025 [SECONDARY] | Apr 2025 | Input-tax "blocked" flags per expense category |
| **VAT return**: **monthly** if annual taxable supplies exceed **SAR 40m**, otherwise **quarterly**; due by the **last day of the following month**. Lines: standard-rated sales, **private healthcare / private education to citizens** (VAT borne by the state), zero-rated domestic sales, exports, exempt sales; standard-rated domestic purchases, imports paid at customs, imports under reverse charge, zero-rated and exempt purchases; corrections from previous periods; credit carried forward; net VAT | IR Art 59; ZATCA return form [SECONDARY: Avalara 2017, Tally, Wafeq]; box numbering not checked on the live portal [UNCERTAIN] | 1 Jan 2018 | Box mapping per tax category; citizen flag on the customer (national ID scheme `NAT`) for healthcare/education; first-home sales to citizens tracked separately |
| **Records**: at least **6 years**; **11 years** for movable capital assets; **15 years** for real estate. Records may be electronic | IR Art 66 [SECONDARY: ClearTax, Tally, Zoho; sources disagree on real estate] [UNCERTAIN on whether records must physically stay in KSA] | 1 Jan 2018; amended 2021 and 2025 | Retention engine per asset class. Keep signed XML and ZATCA responses for the same periods |
| **Currency**: an invoice may be in another currency, but the **VAT amount must be in SAR**; in the e-invoice, the tax currency code (BT-6) must exist and the tax totals must be in SAR | IR Art 53 [SECONDARY]; XML standard BR-KSA-68 and §7 [PRIMARY] | 1 Jan 2018 | Same dual-currency model as UAE (document currency + tax accounting currency). Exchange rate source configurable (SAMA rate expected) [UNCERTAIN] |

## 6. KSA: ZATCA e-Invoicing (Fatoora)

### 6.1 Phase 1 (Generation)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| All resident VAT-registered taxpayers (and anyone issuing invoices on their behalf) must **generate and store** tax invoices and notes electronically with a compliant solution (EGS) | ZATCA roll-out page [PRIMARY] | **4 Dec 2021** | KhataOS is an EGS. Every invoice and note is created only through the app |
| Simplified invoices carry a **QR code with tags 1 to 5** (seller name, VAT number, timestamp, total with VAT, VAT total) | Security Features Standard v1.2 §4 [PRIMARY] | 4 Dec 2021 | QR generator (TLV + Base64), see 6.3 |
| **Prohibited functions** reported in the E-invoicing Controls: no anonymous access, no deletion or alteration of issued invoices, no tampering with the sequence counter or the system time, no handwritten or word-processor invoices | E-invoicing Resolution / Controls [SECONDARY: official annex not retrieved] | 4 Dec 2021 | Issued documents are append-only (reuse the India audit trail). Corrections only through credit/debit notes. User login required. Clock-skew detection |

### 6.2 Phase 2 (Integration): document rules

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Format**: **UBL 2.1** XML (`urn:oasis:names:specification:ubl:schema:xsd:Invoice-2`), or **PDF/A-3 with embedded XML**. UBL element order and all UBL mandatory elements must be respected. The UBL message type is "Invoice" for invoices, debit notes and prepayments; credit notes use the CreditNote type code | XML Implementation Standard v1.2 (19 May 2023) [PRIMARY] | 1 Jan 2023 (waves) | XML generator + local XSD and Schematron validation (the ZATCA SDK ships both) |
| **Invoice type code (BT-3)** with subtype in `@name` (KSA-2): `388` tax invoice, `386` prepayment, `383` debit note, `381` credit note. `@name` = **NNPNESB**: NN `01` standard (B2B) or `02` simplified (B2C); P third-party; N nominal supply; E export; S summary; B self-billed (each 0/1). Self-billing not allowed for exports. Simplified documents only accept the third-party, nominal and summary flags | XML standard §11.2.1, BR-KSA-06, -07, -31 [PRIMARY] | 1 Jan 2023 | Derive the 7-character code from voucher attributes (structurally the same idea as UAE's 8-flag string) |
| **Identifiers**: **UUID** (KSA-1, letters/digits/dashes) per document; **invoice number** (BT-1); **Invoice Counter Value (ICV, KSA-16)**, digits only; **Previous Invoice Hash (PIH, KSA-13)**, Base64 SHA-256 | BR-KSA-03, -33, -34, -26, -61 [PRIMARY] | 1 Jan 2023 | Generate UUID v4 locally. Keep a per-EGS monotonic counter that is never reset or reused |
| **Hash chain**: one sequence **per EGS** covering both B2B and B2C documents. Each document's PIH = hash of the **last document generated**, even if that one was rejected. Documents may be submitted out of order as long as PIH points to the previous *generated* document. A rejected document keeps its hash and ICV; the corrected one is a new document with a new ICV and PIH | Detailed Technical Guideline v2 §4.3 and FAQ [PRIMARY] | 1 Jan 2023 | Chain state (last hash, last ICV) stored transactionally per EGS. Never "edit and resend"; always issue a new document |
| **Hash method**: remove `ext:UBLExtensions`, the `cac:AdditionalDocumentReference` with ID `QR`, and `cac:Signature`; canonicalise with **C14N 1.1**; SHA-256; Base64. **First document's PIH** = Base64 of SHA-256 of "0": `NWZlY2ViNjZmZmM4NmYzOGQ5NTI3ODZjNmQ2OTZjNzljMmRiYzIzOWRkNGU5MWI0NjcyOWQ3M2EyN2ZiNTdlOQ==` | BR-KSA-26; Security Standard §3 [PRIMARY] | 1 Jan 2023 | Use a proven C14N 1.1 library. Golden-file tests against SDK output |
| **Dates and time**: issue date (BT-2) **not later than today**; **issue time** (KSA-25) mandatory as hh:mm:ss (AST) or hh:mm:ssZ (UTC); supply date (KSA-5) mandatory on standard tax invoices; summary simplified invoices need supply start and end dates. All dates are Gregorian (ISO 8601) | BR-KSA-04, -15, -70, -72 [PRIMARY] | 1 Jan 2023 | Store timestamps with time zone. **Hijri dates are for display only** (optional second date on prints); XML stays Gregorian [SECONDARY: no Hijri rule found in the specs] |
| **Parties**: seller VAT number mandatory; seller other ID with scheme CRN, MOM, MLS, 700, SAG or OTH; **seller address**: street, **4-digit building number**, postal code, city, **district**, country (Saudi National Address); buyer name mandatory on standard invoices; buyer address (street, city, country) on standard invoices; non-VAT buyers identified by TIN, CRN, MOM, MLS, 700, SAG, **NAT** (national ID), GCC, **IQA** (iqama) or OTH; export invoices must **not** carry a buyer VAT number | BR-KSA-08, -09, -10, -14, -37, -39, -42, -46 [PRIMARY] | 1 Jan 2023 | Party and address masters built around the Saudi National Address (building no., street, district, city, postal code, additional no.). ID-type picklist |
| **VAT categories**: `S` standard; `Z` zero-rated with reason (VATEX-SA-32 export of goods, -33 export of services, -34-1 to -34-5 international transport, -35 medicines and medical equipment, -36 qualifying metals, -EDU and -HEA private education/healthcare to citizens, -MLTRY qualified military goods); `E` exempt (VATEX-SA-29 financial services, -29-7 life insurance, -30 real estate); `O` out of scope (VATEX-SA-OOS + free text). Code lists carry both **English and Arabic** texts | XML standard §11.2.4; BR-KSA-69, -83 [PRIMARY] | 1 Jan 2023 | Tax-category master with VATEX code and bilingual text, printed on the invoice |
| **Rounding**: **half-up**; document totals rounded to 2 decimals; round final results, not intermediates; amounts up to 2 decimals, unit prices/quantities/percentages unrestricted; line VAT and line amount with VAT checked by BR-KSA-51/52/53 | XML standard §10, §6 [PRIMARY] | 1 Jan 2023 | Per-country rounding policy in the pack (UAE: invoice-total level only; KSA: half-up with line rules; India: its own rules) |
| **Prepayments**: the final invoice lists each prepayment as an extra line with Prepayment ID (KSA-26), issue date (KSA-28) and time (KSA-29), and VAT split by category | XML standard §8; BR-KSA-73 to -80 [PRIMARY] | 1 Jan 2023 | Advance-receipt voucher issues a `386` document; settlement adds the prepayment lines automatically |
| **File name**: `<seller VAT no.>_<date without dashes>T<time without colons>_<invoice no. with non-alphanumerics replaced by "-">.xml` | XML standard §14 [PRIMARY] | 1 Jan 2023 | Use for exports and archives |
| **Data dictionary and XML standard** last published **19 May 2023** (v1.2); ZATCA's page was updated 10 Aug 2026 without new versions | ZATCA Systems Developers pages [PRIMARY] | 2023 | Pin to v1.2. **Action:** watch for a new release before GA |

### 6.3 Phase 2: cryptographic stamp and QR

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Algorithms**: **ECDSA**, 256-bit key, **SHA-256**; key pair per FIPS 186; public-key validation per NIST SP 800-56A. Curve **secp256k1** in the official SDK and developer guides | Security Features Standard v1.2 §2.2.1 req. 6, 16 [PRIMARY]; curve [SECONDARY: SDK/developer guides; the standard does not name it] | 1 Jan 2023 | Key generation in the app on secp256k1. **Signing key non-exportable**, held in a hardware or software security module (OS keychain / TPM / Secure Enclave where it supports the curve; otherwise an encrypted software keystore) |
| **Stamp format**: **XAdES** enveloped signature for XML (baseline **B-B**), **PAdES** for PDF/A-3; full certificate chain to ZATCA's trust anchor included; signature method `urn:oasis:names:specification:ubl:dsig:enveloped:xades`; IDs `urn:oasis:names:specification:ubl:signature:1` and `…:signature:Invoice` | Security Standard §2.2.1, §2.3.3; BR-KSA-28 to -30 [PRIMARY] | 1 Jan 2023 | Mandatory on simplified documents (BR-KSA-60); optional on standard documents (ZATCA adds its own stamp at clearance) |
| **Certificate revocation**: CRLs valid **7 days**, which allows an EGS to **work fully offline for up to 7 days**; validity checked before stamping | Security Standard §2.2.1 [PRIMARY] | 1 Jan 2023 | Cache CRL with expiry; block B2C stamping when the cached CRL is more than 7 days old |
| **QR code**: **TLV** (1-byte tag, 1-byte length, UTF-8 value), Base64, **up to 700 characters**. Tags: **1** seller name, **2** seller VAT number, **3** timestamp (ISO 8601), **4** total with VAT, **5** VAT total (BT-110) [from 4 Dec 2021]; **6** XML hash (32-byte SHA-256), **7** ECDSA signature of the hash, **8** ECDSA public key, **9** for simplified documents only, the signature of the CSID certificate by ZATCA's CA [from 1 Jan 2023]. For **standard** documents the QR is returned by ZATCA in the clearance response | Security Standard v1.2 §4; Detailed Guideline §4.3 [PRIMARY] | 4 Dec 2021 / 1 Jan 2023 | QR built from **bytes** (length = UTF-8 byte count, not characters: Arabic seller names are a known bug source). Simplified: the app builds the QR. Standard: print only the QR from the cleared XML |

### 6.4 Phase 2: onboarding (CSID)

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| Taxpayer logs into the **Fatoora portal** and generates an **OTP** (about 1 hour validity [SECONDARY]) per EGS unit (or a batch) | Detailed Guideline v2 §3.3.2 [PRIMARY] | 1 Jan 2023 | Onboarding wizard per installation/device: enter OTP |
| EGS creates a **PKCS#10 CSR** signed with its private key. Subject: **CN** (unit name/asset no.), **serial number** `1-<maker>|2-<model/version>|3-<serial>`, **organizationIdentifier** = VAT number, **OU** = branch (or the 10-digit TIN for VAT-group members), **O** = taxpayer name, **C** = SA, **title** (functionality map **TSCZ**, e.g. `1100` = standard + simplified, `1000` B2B only, `0100` B2C only), **registeredAddress** (preferably the National Address short address), **businessCategory** (industry) | Security Standard Table 1 [PRIMARY] | 1 Jan 2023 | CSR builder with these fields; KhataOS supplies its own maker/model/serial |
| CSR + OTP go to the compliance API: returns a **Compliance CSID** (certificate + secret + request ID). The EGS must then pass **compliance checks** by submitting sample documents for each type it supports (standard invoice, debit note, credit note; simplified invoice, debit note, credit note). Then it requests the **Production CSID** from the ZATCA CA | Detailed Guideline §3.3.3 to §3.3.5 [PRIMARY] | 1 Jan 2023 | Automated compliance run that generates and submits the required samples, then stores the production CSID. Failure restarts from a new OTP |
| API auth: **OAuth 2 Basic** with the CSID certificate as client ID and the issued secret | Security Standard §5 [PRIMARY] | 1 Jan 2023 | Store certificate and secret in the OS keychain, never in plain config or backups |
| **Renewal** before certificate expiry (new OTP + CSR); **revocation** when a device is lost, sold or compromised. Joining a VAT group **revokes all CSIDs** of the member; newly registered taxpayers must wait **2 business days** before submitting | Detailed Guideline §3.2.2, §3.2.3, FAQ [PRIMARY] | 1 Jan 2023 | Expiry monitor (alert 30 days before). "Retire this device" flow that requests revocation. Backups must not restore a private key to a second machine without re-onboarding |
| Developer tools: **Compliance and Enablement Toolbox (SDK)** (offline validator for XML, QR and signatures) and the **Integration Sandbox** (test backend for onboarding, reporting and clearance); the developer portal needs an account | Detailed Guideline §2.1.4, §2.1.5 [PRIMARY]; portal URLs [SECONDARY] | 2022 | CI runs the SDK validator on golden invoices. Use the sandbox for integration tests |

### 6.5 Phase 2: clearance vs reporting, and waves

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Clearance (standard, B2B)**: submit to the Clearance API **before** giving the document to the buyer; it is valid only once it carries ZATCA's stamp. Responses: valid (stamped XML + QR returned), accepted with warnings (stamped), rejected (errors). Under self-billing the buyer submits | Detailed Guideline §4.3.2 [PRIMARY] | Per wave | B2B invoice status: Draft, Signed, **Pending clearance**, Cleared, Cleared with warnings, Rejected. Printing a "Tax Invoice" is blocked until Cleared. **Offline**: B2B cannot be finalised offline; queue it and tell the user |
| **Reporting (simplified, B2C)**: the seller signs, adds the QR and hands the invoice to the customer immediately, then reports it **within 24 hours**. ZATCA does not stamp simplified documents | Detailed Guideline §4.3.1 [PRIMARY] | Per wave | Durable offline outbox with a 24-hour SLA timer and retry/backoff; alert at about 20 hours. A rejected B2C report is fixed going forward (no reissue to the customer needed in most cases); log it for the VAT return |
| If ZATCA **disables clearance**, the Clearance API returns **303** and standard documents go through the Reporting API (not stamped) | Detailed Guideline FAQ [PRIMARY] | Per wave | Handle the 303 fallback automatically |
| **Waves** (Phase 2 integration): announced at least 6 months ahead, targeting by revenue in a look-back window. **Wave 23**: > SAR 750,000, by **31 Mar 2026**; **Wave 24**: > SAR 375,000 (2022 to 2024), by **30 Jun 2026**; **Wave 25**: VAT-subject revenue **> SAR 187,500 in any of 2022, 2023, 2024 or 2025**, integrate **by 1 Feb 2027** (announced 24 Jul 2026). No Wave 26 found | ZATCA news "Wave25 E-invoicing" [PRIMARY for Wave 25]; Waves 23 and 24 [SECONDARY: Origami, Fiscal Solutions; Wave 24 look-back years differ] | as stated | Company setting "integration date" (from the ZATCA notification letter). Wave 25 covers almost every VAT-registered SME, so **Phase 2 is MVP-critical for KSA** |

## 7. KSA: Zakat and income tax basics for bookkeeping

| Requirement | Source | Effective date | What the software must do |
|---|---|---|---|
| **Zakat** on Saudi/GCC-owned entities at **2.5%** of the **zakat base** (not of profit). When the financial year is Gregorian, the rate is scaled by days (365/354 × 2.5% ≈ **2.5776%**) | Zakat Implementing Regulation, MR 1007 (19/08/1445H, ~29 Feb 2024), amended by MR 1248 (11/10/1446H) [SECONDARY: KPMG] | FYs starting on or after **1 Jan 2024** | Zakat-base working paper from the balance sheet (equity, long-term liabilities and provisions as additions; fixed assets, long-term investments as deductions, per the regulation's caps). Rate scaled to the actual year length |
| Mixed ownership: zakat on the Saudi/GCC share, **income tax 20%** on the foreign share | Income Tax Law [SECONDARY] | 2004 | Ownership-percentage table on the company |
| **Zakat/income tax return** and payment within **120 days** of the year end | KPMG [SECONDARY] | ongoing | Calendar |
| Entities without proper books fall under a deemed-basis regime | ZATCA rules for deemed basis (2021) [SECONDARY] | 2021 | Out of scope (KhataOS keeps proper books) |
| Books language and retention for zakat/income tax (commonly cited: books in Arabic, kept 10 years) | [UNCERTAIN: not verified for the 2024 regulation or the Commercial Books Law] | none | Arabic ledger names available; retention ≥ 10 years for zakat entities until verified |

---

## 8. Design implications for KhataOS (country compliance pack)

The India research already calls for effective-dated tax masters, a per-FY numbering series, an append-only hash-chained audit trail, an e-invoice status gate and stored portal responses. The UAE and KSA fit the same pack interface if it is designed around the points below. **No code here; this is the contract each country pack must satisfy.**

1. **Tax-ID validators (pluggable per ID type).** Each pack declares its ID types, format rules, optional checksum, derived parts and online-verification hook.
   - India GSTIN: 15 chars, state + PAN + mod-36 checksum (warning only).
   - UAE TRN: 15 digits, derived TIN = first 10, no checksum, "100" prefix is a warning.
   - KSA VAT: 15 digits, 3…3, VAT-group flag at digit 11.
   - Also secondary IDs: UAE trade licence + issuing authority; KSA CRN/MOM/MLS/700/SAG/NAT/IQA/GCC scheme picklist.
2. **Canonical invoice model on EN 16931 semantics.** PINT AE (Peppol, EN 16931-based) and ZATCA (EN 16931-based UBL 2.1) share most business terms (BT-1 number, BT-2 date, BT-3 type, BT-5 currency, BT-6 tax currency, BG-23 VAT breakdown, BT-110 VAT total, BT-115 payable). Store invoices in one internal model keyed to BT/IBT IDs, plus country extension fields (BTAE-xx, KSA-xx). India's INV-01 JSON is mapped from the same model (it is not UBL). Each pack supplies serialisers (UBL XML for UAE/KSA, JSON for India), validators (XSD + Schematron, JSON Schema) and pinned spec versions.
3. **Transaction flags are data.** UAE's 8-flag string, KSA's NNPNESB and India's supply types (B2B, SEZWP, EXPWOP, RCMDOM…) are all derived from the same voucher attributes (export, zone, deemed/nominal, summary, self-billed, agent, e-commerce, margin). Model the attributes once; each pack maps them.
4. **Numbering, identity and chaining.** The pack declares:
   - the legal number rule (India: ≤16 chars `[A-Za-z0-9/-]`, unique per FY; UAE: sequential or unique; KSA: unique sequential);
   - whether a **UUID** is needed (UAE: per e-invoice, generated by the ASP per guidelines but harmless to also generate locally; KSA: generated by the EGS);
   - whether a **device-level counter + hash chain** is needed (KSA only: ICV + PIH per EGS, including rejected documents; India and UAE: no).
   Reuse the India audit-trail hash chain implementation, but keep the **ZATCA PIH chain separate**: its hashing rules are fixed by ZATCA (C14N 1.1 over the XML), and its scope is per device, not per database.
5. **Submission model per document type.** One abstraction with modes:
   - *clearance-before-issue* (KSA B2B; India IRN is similar: an invoice without IRN "shall not be treated as an invoice");
   - *report-within(duration)* (KSA B2C 24 hours; UAE C2 to FTA reporting, timeline TBD);
   - *exchange-via-provider within(duration)* (UAE: through the ASP, 14 days; India: optional GSP later);
   - *none* (B2C in UAE, exempt classes).
   Each mode defines statuses, deadlines (India 30 days for AATO ≥ ₹10 cr; UAE 14 days; KSA 24 h), allowed cancellation (India: 24-hour IRN cancel; KSA and UAE: **no cancel**, credit note only), and what blocks printing.
6. **Offline queue (local-first).** A durable outbox per company and channel with retry, backoff and deadline alerts. KSA: B2C can be signed and handed over offline, reported within 24 h, with the CRL cache allowing up to 7 days offline; B2B cannot be finalised offline. UAE: hand-off to the ASP within 14 days; log outages and prompt the **2-business-day FTA system-failure notice**. India: offline JSON export/import already fits this outbox (manual channel).
7. **Signing keys and credentials.** Only KSA needs document-signing keys in the MVP. Per EGS (one per installation or POS device): secp256k1 private key, non-exportable, in the OS keystore or an encrypted keystore; CSID certificate + secret; expiry and revocation status. Keys must **not** travel in ordinary backups; restoring on a new machine means re-onboarding (new OTP/CSR). ASP and GSP API credentials (UAE/India, later) go in the same secret store. This also lines up with India's DPDP encryption-at-rest requirement.
8. **Response storage.** Keep verbatim, immutable and in backups: India IRP response (IRN, Ack, SignedQRCode, SignedInvoice); KSA cleared XML with ZATCA stamp and QR, validation warnings, reporting results; UAE ASP and FTA confirmations, UUID and the exact XML sent and received. These feed the audit trail and the FAF / return reconciliations.
9. **Arabic, RTL and bilingual output.** Arabic name fields on company, party, item, ledger and tax-category masters; bidirectional text layout; embedded Arabic fonts in PDF; bilingual templates (Arabic required for KSA customer-facing invoices, optional for UAE); digits 0-9 in XML, Eastern Arabic numerals optional on KSA prints; QR TLV lengths computed on UTF-8 bytes. Optional PDF/A-3 with embedded XML (KSA allows it).
10. **Currency and rounding policy per pack.** Document currency vs **tax accounting currency** (AED/SAR mandatory on foreign-currency invoices; INR for India); exchange-rate source per country (CBUAE; SAMA [UNCERTAIN]; RBI/CBIC for India) with the rate stored on the voucher; rounding rules (UAE: only at invoice total, 2 dp; KSA: half-up, 2 dp document totals, line checks; India: ₹ round-off conventions). 2-decimal currencies for AED/SAR/INR; the model should still allow 3-decimal currencies (KWD, BHD, OMR) for later GCC packs.
11. **Calendars and periods.** India FY Apr to Mar; UAE CT period = any 12-month FY; KSA Gregorian or Hijri zakat year (rate scaling); VAT periods monthly/quarterly per entity (UAE stagger from EmaraTax). Effective-dated rates, thresholds and phase dates in versioned data files for all three.
12. **Retention and immutability engine.** One engine with per-pack rules: India max(8 FY, 72 months after annual return, IT period); UAE VAT 5 years (+4 audit, real estate 7 or 15), CT 7 years; KSA 6/11/15 years (zakat ≥ 10 [UNCERTAIN]). Issued documents are never deleted (KSA prohibition; UAE requires e-credit notes; India audit-trail rule).
13. **Returns and audit files.** Each tax category maps to return boxes per pack (GSTR-1/3B tables; VAT201 boxes with emirate tag; KSA return lines with citizen flag). Audit exports: India (CA packs), UAE **FAF**, KSA (no FAF equivalent found; ZATCA already holds the e-invoices).
14. **Where the pack interfaces are shared:** tax-ID validation, tax category + rate masters, document numbering, canonical invoice model + serialisers, submission modes + outbox, response store, print templates (LTR/RTL), rounding/currency policy, return box mapping, retention rules. **Where they diverge:** KSA device keys and PIH chain (KSA-only module), UAE ASP connector (provider adapters), India GSP/IRP and e-way bill.

---

## 9. Open items to verify before GA (consolidated)

1. **UAE MD 244 amendment (May 2026)**: get the decision text (reported as MD 56/2026 or MD 66/2026); confirm the 30 Oct 2026 ASP deadline and whether the < AED 50m go-live is 1 Jul 2027 (MD 244) or 1 Oct 2027 (one law firm).
2. **UAE FTA reporting timeline** for Tax Data (MD 243 Art 6(6) leaves it to the Minister) and the official MoF **mandatory-fields document** (to confirm the 51-field count).
3. **UAE VAT ER consolidated text** after CD 100/2024, CD 100/2025 and CD 149/2026: exact Art 59 and Art 60 wording, and the new e-invoice clause in Art 59; the cash-payment input-tax threshold (Ministerial decision pending).
4. **UAE real-estate retention**: 7 years (TP ER Art 3 per MoF) vs 15 years (original VAT ER).
5. **UAE FAF** official field list and file format from tax.gov.ae.
6. **UAE VAT201** current box labels on EmaraTax; the monthly-filer threshold (AED 150m).
7. **UAE CT**: confirm MD 131/2026 (SBR to 2029), MD 82/2023 audit thresholds, and the TP disclosure thresholds against the decision texts.
8. **KSA VAT IR** official Arabic text (10th edition, Apr 2025): Art 53 fields, the Arabic-language clause, and the **credit/debit note 15-day trigger** (event month vs following month).
9. **KSA record retention** (Art 66) for real estate (15 vs 11 years), and whether records must be stored inside the Kingdom.
10. **KSA VAT return** current line numbering and the SAR 40m monthly threshold.
11. **ZATCA spec versions**: check for releases after XML Standard / Data Dictionary / Security Standard v1.2 (19 May 2023) and the current SDK version; confirm the secp256k1 curve against the SDK.
12. **ZATCA E-invoicing Controls annex** (Phase 1 required/prohibited functions) was not retrieved.
13. **KSA Zakat**: books-language rule and retention period under the 2024 regulation; exchange-rate source for SAR conversion.
14. **TRN and KSA VAT checksums**: none published; do not implement unless an official algorithm appears.

## 10. Source list

**UAE: e-invoicing (official)**
- MoF e-invoicing hub: <https://mof.gov.ae/einvoicing/>
- *UAE Electronic Invoicing Guidelines* v1.1 (1 Jun 2026; file name V1.0 23Feb2026): <https://mof.gov.ae/wp-content/uploads/2026/02/UAE-Electronic-Invoicing-Guidelines_V-1.0-23Feb2026.pdf>
- Ministerial Decision 243/2025: <https://mof.gov.ae/wp-content/uploads/2025/09/Ministerial-Decision-no.-243-of-2025-on-the-Electronic-Invoicing-System.pdf>
- Ministerial Decision 244/2025: <https://mof.gov.ae/wp-content/uploads/2025/09/Ministerial-Decision-No.-244-of-2025-on-the-Implementation-of-the-Electronic-Invoicing-System.pdf>
- Cabinet Decision 106/2025 (penalties): <https://mof.gov.ae/wp-content/uploads/2025/12/Cabinet-Decision-Violations-and-Penalties-eInvoicing-final-version-en-8.12.25.pdf>
- MD 64/2025 (ASP accreditation): <https://mof.gov.ae/wp-content/uploads/2025/03/Ministerial_Decision_Eligibility_and_Accreditation_procedure_for_SPs_EN.pdf>
- Pre-approved ASPs: <https://mof.gov.ae/en/pre-approved-einvoicing-service-providers/>
- PINT AE 1.0.4 (OpenPeppol): <https://docs.peppol.eu/poac/ae/>; transaction type <https://docs.peppol.eu/poac/ae/pint-ae/trn-invoice/codelist/transactiontype/>; tax categories <https://docs.peppol.eu/poac/ae/pint-ae/trn-invoice/codelist/Aligned-TaxCategoryCodes/>; type codes <https://docs.peppol.eu/poac/ae/pint-ae/trn-invoice/codelist/UNCL1001-inv/>; syntax <https://docs.peppol.eu/poac/ae/pint-ae/trn-invoice/syntax/>

**UAE: e-invoicing amendment (secondary)**
- WAM, 10 May 2026: <https://www.wam.ae/en/article/c055277-mof-announces-targeted-amendments-einvoicing>; Gulf News: <https://gulfnews.com/amp/story/business%2Fuae-announces-extension-of-einvoicing-provider-deadline-to-october-2026-1.500535895>; BSA Law: <https://bsalaw.com/insight/uae-electronic-invoicing-system-key-amendments/>; EY: <https://taxnews.ey.com/news/2025-1964-uae-ministry-of-finance-publishes-ministerial-decisions-on-e-invoicing>

**UAE: VAT and tax procedures**
- FDL 16/2025 and 17/2025 (KPMG): <https://kpmg.com/ae/en/insights/tax-insights/federal-decree-law-no-16-and-17-of-2025.html>; PwC: <https://www.pwc.com/m1/en/services/tax/me-tax-legal-news/2025/uae-significant-legislative-updates-tax-procedures-vat-excise-tax-laws.html>
- Cabinet Decision 149/2026 (KPMG): <https://kpmg.com/ae/en/insights/tax-insights/Cabinet-Decision-149-of-2026-aending-certain-provisions-of-the-vat-executive-regulation.html>; MoF news: <https://mof.gov.ae/en/news/ministry-of-finance-announces-amendments-to-the-vat-executive-regulation/>; Deloitte: <https://www.deloitte.com/middle-east/en/services/tax/perspectives/amendments-to-the-uae-vat-executive-regulation.html>
- Cabinet Decision 100/2025 (Arts 59, 60): Crowe <https://www.crowe.com/ae/news/what-is-the-recent-amendment-to-uae-vat>; Grant Thornton <https://www.grantthornton.ae/insights/articles2/tax-alert-executive-regulation-amendments/>; Bloomberg Tax <https://news.bloombergtax.com/daily-tax-report-international/united-arab-emirates-mof-posts-cabinet-decision-amending-executive-regulations-of-federal-decree-law-on-vat>
- Cabinet Decision 100/2024: <https://www.pgplaw.com/analytics-and-brochures/alerts/the-uae-tax-case-study-overview-of-the-cabinet-decision-no-100-of-2024-on-the-vat-executive-regulati/>
- Metal-scrap RCM (CD 153/2025): <https://kpmg.com/ae/en/insights/tax-insights/cabinet-decision-no-153-of-2025.html>
- Cabinet Decision 74/2023 (TP ER): <https://mof.gov.ae/wp-content/uploads/2023/07/Cabinet-Decision-No-74-of-2023-On-Executive-Regulations-of-Federal-Decree-Law-No.-28-of-2023-Tax-Procedures-Law.pdf>; Crowe summary <https://www.crowe.com/ae/news/new-executive-regulations-to-the-tax-procedures>
- Tax invoice particulars: <https://mondaq.com/sales-taxes-vat-gst/670546/tax-invoice-under-uae-vat-law>
- VAT201: <https://www.businessdubai.ae/blogs/uae-vat-return-filing>
- FAF: <https://tallysolutions.com/mena/uae-vat/fta-audit-file-faf-uae/>; <https://www.aviaanaccounting.com/what-is-fta-audit-file-of-vat-in-dubai/>
- TRN: <https://lookuptax.com/docs/validate/united-arab-emirates/trn>; <https://bcl.ae/blogs/trn-verification-uae/>

**UAE: Corporate Tax, ESR**
- SBR extension (MD 131/2026): <https://khaleejtimes.com/business/uae-extends-corporate-tax-relief-for-small-businesses-until-2029>; <https://farahatco.com/blog/uae-extends-small-business-relief-until-31-december-2029-under-ministerial-decision-no-131-of-2026/>
- TP documentation (MD 97/2023): <https://mof.gov.ae/ministry-of-finance-issues-decision-on-transfer-pricing-documentation-requirements-for-corporate-tax-purposes>; <https://bcl.ae/resources/guides-and-insights/transfer-pricing/what-are-the-transfer-pricing-documentation-requirements-in-the-uae/>
- Accounting standards (MD 114/2023): <https://pwc.com/m1/en/services/tax/me-tax-legal-news/2023/uae-ct-ministerial-decisions-no-114-115-releases.html>; <https://www.dlapiper.com/insights/publications/gulf-tax-insights/gulf-tax-insights---may-2023/ministerial-decision-no-114-of-2023-provides-clarity-on-applicable-accounting-standards>
- ESR (CD 98/2024): <https://cms.law/en/omn/legal-updates/amendments-to-the-uae-economic-substance-regulations>

**KSA: ZATCA (official)**
- Roll-out phases: <https://zatca.gov.sa/en/E-Invoicing/Introduction/Pages/Roll-out-phases.aspx>
- Wave 25 announcement (24 Jul 2026): <https://zatca.gov.sa/en/MediaCenter/News/Pages/Wave25-E-invoicing.aspx>
- XML Implementation Standard v1.2 (19 May 2023): <https://zatca.gov.sa/ar/E-Invoicing/SystemsDevelopers/Documents/20230519_ZATCA_Electronic_Invoice_XML_Implementation_Standard_%20vF.pdf>
- Data Dictionary (19 May 2023): <https://zatca.gov.sa/ar/E-Invoicing/SystemsDevelopers/Documents/20230519_EInvoice_Data_Dictionary%20vF.xlsx>
- Security Features Implementation Standards v1.2: <https://zatca.gov.sa/ar/E-Invoicing/SystemsDevelopers/Documents/20230519_ZATCA_Electronic_Invoice_Security_Features_Implementation_Standards_vF.pdf>
- E-invoicing Detailed Technical Guideline v2 (Nov 2022): <https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/E-invoicing-Detailed-Technical-Guideline.pdf>
- Spec pages: <https://zatca.gov.sa/en/E-Invoicing/SystemsDevelopers/Pages/E-Invoice-specifications.aspx>; <https://zatca.gov.sa/en/E-Invoicing/SystemsDevelopers/Pages/Security-Requirements.aspx>

**KSA: secondary**
- Wave 25: KPMG <https://kpmg.com/us/en/taxnewsflash/news/2026/07/saudi-arabia-e-invoicing-mandatory-for-taxpayers-with-revenue-above-sar-187500.html>; EY <https://taxnews.ey.com/news/2026-1705-saudi-arabia-announces-25th-wave-of-phase-2-e-invoicing-integration>; Wave 24: <https://www.fiscal-requirements.com/news/5753-saudi-arabia-zatca-announces-24th-wave-for-e-invoicing-integration-phase-deadline-30-june-2026>; <https://origami.sa/en/blog/zatca-phase-2-wave-24-integration-guide/>
- VAT IR amendments (Apr 2025): EY <https://taxnews.ey.com/news/2025-0932>; credit-note timing: <https://www.wafeq.com/en-sa/tax-and-reporting/how-to-handle-credit-notes-in-ksa-e-invoicing>
- Art 53 summary: <https://noqta.tn/en/blog/saudi-simplified-tax-invoice-article-53-2026>
- Arabic invoices: <https://zatca1.discourse.group/t/what-should-be-the-language-of-the-invoice-shared-with-zatca/206>; <https://cleartax.com/sa/zatca-notices-non-compliance-e-invoicing-regulations>
- Records: <https://www.cleartax.com/sa/vat-records-books-of-accounts-saudi-arabia>; <https://tallysolutions.com/mena/saudi-vat/maintaining-vat-records-saudi-arabia/>
- VAT return: <https://tallysolutions.com/mena/saudi-vat/saudi-vat-return-format>; <https://www.avalara.com/vatlive/en/vat-news/saudi-arabia-releases-vat-return.html>
- Zakat regulation: <https://kpmg.com/sa/en/insights/tax-insights/tax-alert-saudi-minister-of-finance-approves-new-zakat-implementing-regulation.html>; <https://kpmg.com/sa/en/services/tax/direct-tax-and-zakat/zakat.html>
- QR guide (2021): <https://www.pwc.com/m1/en/services/tax/me-tax-legal-news/2021/saudi-arabia-guide-to-develop-compliant-qr-code-for-simplified-einvoices.html>

## 11. Addendum (21 Oct 2026): reverse charge, imports, ZATCA renewal and the reporting window

Checked against primary sources for the VAT-pack work (`docs/uae-vat.md`,
`docs/ksa-zatca.md`). Tags as above.

### 11.1 UAE VAT 201: Boxes 3, 6, 7 and 10

- **Box 3 "Supplies subject to the reverse charge provisions"** (output side):
  net value and output VAT of goods and services received under the
  reverse charge: services from foreign suppliers, **standard-rated and
  zero-rated**; goods under the reverse charge **not declared to UAE
  Customs**; and "local supplies subject to the reverse charge provisions
  (e.g. specific supplies within the oil and gas industry)". Reverse-charge
  imports declared through Customs go in Box 6 instead. FTA *VAT Returns
  User Guide* v4.0 (Aug 2021) pp.15-16 [PRIMARY]
  <https://tax.gov.ae/DataFolder/Files/Pdf/VAT%20Returns%20User%20GuideEnglishV40%2015%2008%202021%20SEP2021.pdf>;
  the EmaraTax manual *VAT 201 – Returns for e-commerce reporting* v1.0
  (July 2023) p.25 keeps the layout [PRIMARY].
- **Box 10** (input side) recovers the reverse-charge VAT of **Boxes 3, 6
  and 7**: the full net value and only the recoverable VAT (example: AED 1m
  with AED 50k VAT at 50% recovery → Box 10 AED 1m / AED 25k); reverse-charge
  purchases are in Box 10, not Box 9. Guide pp.22-23 [PRIMARY].
- **Box 6 "Goods imported into the UAE"**: imports declared through UAE
  Customs whose VAT is paid on the return; **pre-filled** from the customs
  declarations under the customs number linked to the TRN, value including
  customs duty and excise, VAT at 5%. Guide p.17 [PRIMARY]. Basis: Executive
  Regulation Art 48(1) (registered at import, customs number given, etc.);
  otherwise VAT is paid at import (ER Art 50(1)) [PRIMARY: MoF consolidated
  ER, CD 52/2017 to CD 100/2024]. The box that recovers VAT physically paid
  at customs is not in the guide [UNCERTAIN].
- **Box 7 "Adjustments to goods imported into the UAE"**: only when Box 6
  is incomplete or wrong, positive or negative, including the 5% assumed on
  zero-rated imports. Guide pp.17-18 [PRIMARY].
- **Designated zones**: Decree-Law Art 50 (a zone meeting the ER's
  conditions is outside the State), Art 51 (goods move between zones
  untaxed), Art 52; ER Art 51 (goods for consumption in a zone, services,
  water and energy are inside the State; goods consumed by the owner or
  found short are deemed imported). Return effect: zone goods consumed in
  the zone in Box 1; others left out of Boxes 1 and 9 (guide pp.14-15, 22)
  [PRIMARY: FDL 8/2017 as amended by FDL 18/2022 and FDL 16/2024,
  <https://tax.gov.ae/Datafolder/Files/Legislation/2025/Federal-Decree-Law-No-8-of-2017-and-amendments.pdf>].
- **Domestic reverse charge**:
  - crude or refined oil, natural gas, pure hydrocarbons to a registrant for
    resale or energy: Decree-Law Art 48(3)-(6), with written declarations;
    not when zero-rated (Art 45(1), 45(12)); Box 3/10 per the guide's "oil
    and gas" example [PRIMARY];
  - electronic devices (phones, computers, tablets and parts) for resale or
    manufacture: **Cabinet Decision 91/2023**, from 30 Oct 2023; parts in
    Ministerial Decision 262/2023; public clarification **VATP034**: "The
    Recipient shall declare the Due Tax in box 3" (p.5) [PRIMARY]
    <https://tax.gov.ae/Datafolder/Files/Guides/VAT/PublicClarifications/VATP034%20-%20Reverse%20Charge%20Mechanism%20on%20Electronic%20Devices%20-%2006%2010%202023.pdf>;
  - precious metals and stones and qualifying jewellery: **Cabinet Decision
    127/2024** (repeals CD 25/2018), FTA copy "effective from 15 February
    2025", two written declarations by the recipient [PRIMARY]
    <https://tax.gov.ae/Datafolder/Files/Legislation/Cabinet-Decision-No-127-of-2024-on-Reverse-Charge-Mechanism-for-Precious-Metals.pdf>;
    no box stated (Box 3/10 by analogy) [UNCERTAIN];
  - metal scrap: Cabinet Decision 153/2025 from 14 Jan 2026 [SECONDARY: MoF
    news; text not reviewed].
  - Which box the **supplier** of a domestic reverse-charge supply uses is
    not stated in these sources [UNCERTAIN]; KhataOS puts the value in its
    Box 1 row with no VAT.

### 11.2 KSA VAT return: imports and the reverse charge

- Layout: fields 1-6 sales, 7-12 purchases, 13-16 net VAT (ZATCA *VAT
  Circular – RCM*, Jan 2023, now a 404; wording from the search index
  [SECONDARY]). Confirmed numbers: **1** standard-rated sales; **7**
  standard-rated domestic purchases (Input Tax Deduction Guideline, July
  2020); **8** imports subject to VAT paid at customs; **9** imports subject
  to VAT accounted for through the reverse charge mechanism (Imports and
  Exports Guideline, 2nd ed., May 2026, §5.3) [PRIMARY]
  <https://zatca.gov.sa/en/HelpCenter/guidelines/Documents/Guideline-on-Imports-and-Exports-under-VAT-Provision.pdf>.
  Fields 2-5, 10, 11, 13-16 by number [UNCERTAIN]. No regional split.
  Columns: amount, adjustment, VAT.
- **Reverse charge on imported services**: entered **once, in field 9**;
  "the Tax Return form automatically treats the Input Tax as deductible"
  (§6.3); "output tax and input tax … reported on the same VAT return
  (Box 9)" (§10.4). A business that cannot deduct in full puts the
  non-deductible part in field 9's adjustment column. Exempt imported
  services are not reported. No self-invoice is needed. Import VAT deferred
  under IR Art 44 also goes in field 9 [PRIMARY]. The IR article ("persons
  liable to pay tax", para 1) is numbered 47 by sequence [UNCERTAIN];
  IR Art 59(2)(d), (f) require disclosing reverse-charge supplies and
  import tax [PRIMARY].
- Correction threshold: current ZATCA guidelines say **SAR 15,000**
  (Imports and Exports Guideline §11.5; Tax Groups Guideline 3rd ed., May
  2026) after Board resolution 01-06-24 (19 Nov 2024); the older IR text of
  Art 63(3) still says SAR 5,000 [PRIMARY].

### 11.3 ZATCA: the 24-hour window, clearance, CSID expiry and renewal

- **Simplified** invoices and their notes "must be reported … within a
  period which must not exceed (24) hours from its generation"
  (*Controls, Requirements, Technical Specifications and Procedural Rules*,
  E-Invoicing Implementation Resolution, 19 May 2023, Clause Second(3)(B))
  [PRIMARY]
  <https://zatca.gov.sa/en/E-Invoicing/Introduction/LawsAndRegulations/Documents/20230519_E-Invoicing%20Implementation%20Resolution%20English.pdf>;
  Detailed Guidelines v2 (May 2023) §6.4 [PRIMARY]
  <https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/E-Invoicing_Detailed__Guideline.pdf>.
- **Standard** invoices: cleared "prior to sharing them with the customers"
  (Second(3)(A)); clearance is "a prerequisite for sharing" (§6.4); 303
  means report instead [PRIMARY].
- **Offline**: invoices "are queued, and solution remains operational …
  reported after connection re-established" (Resolution Annex 2). B2C:
  retry "in regular intervals until successful" within 24 hours; past that,
  notify ZATCA through its form and keep evidence. B2B with ZATCA down:
  retry about 5 minutes ("timing TBC"), share uncleared, keep retrying
  about every 15 minutes, then share the cleared one. 429, 500, 503, 504
  mean "please resend". Detailed Guidelines v2 pp.45-49 [PRIMARY].
- **PCSID validity**: NotAfter = generation "+ Up to 60 months (5 years)";
  the portal reminds before expiry (§2.1.2); CRLs valid 7 days (*Security
  Features Implementation Standards* v1.2, 19 May 2023) [PRIMARY]
  <https://zatca.gov.sa/ar/E-Invoicing/SystemsDevelopers/Documents/20230519_ZATCA_Electronic_Invoice_Security_Features_Implementation_Standards_vF.pdf>.
- **Renewal**: the same flow as onboarding (new OTP, new CSR, compliance
  checks); the CA revokes the old CSID and issues a new one; renew "before
  the expiry date" (*E-invoicing Detailed Technical Guidelines* v2, Nov
  2022, §3.2.2) [PRIMARY]
  <https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/E-invoicing-Detailed-Technical-Guideline.pdf>.
  OTP valid one hour (Fatoora Portal User Manual v3) [PRIMARY].
  `/production/csids` "Request or renew a production CSID" (same manual
  §3) [PRIMARY]. *Developer Portal Manual* v3 §2.3.10.4 "Production CSID
  (Renewal) API": a valid OTP, the CSR in the body, `accept-version: V2`,
  Basic auth [PRIMARY]
  <https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/DEVELOPER-PORTAL-MANUAL.pdf>.
  The verb **PATCH** and authenticating with the **current PCSID** come
  from ZATCA's developer forum and SDKs [UNCERTAIN].
- **Expiry or revocation**: a device may submit only while its CSID is not
  expired or revoked (Technical Guidelines §4.1). Revocation: by the
  taxpayer on the Fatoora portal (compromised key, device sold or lost,
  major upgrade, §3.2.3), or automatically on deregistration, suspension or
  tax-group changes (§3.3.8) [PRIMARY]. The API error code for an expired
  CSID [UNCERTAIN].

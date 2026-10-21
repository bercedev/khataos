# Singapore Compliance Requirements (GST first): KhataOS Spec

> **Status as of:** 2026-10-20. **Scope:** Singapore SMEs (companies, sole proprietors, partnerships, LLPs) that are, or may become, GST-registered and keep books in KhataOS.
> **Not legal advice.** This file turns IRAS and ACRA guidance into software requirements. Have a Singapore tax agent or chartered accountant confirm each item before release. Re-check the IRAS GST pages, and especially the InvoiceNow page, every quarter. IRAS changed the InvoiceNow timeline during 2026.
>
> **Confidence tags** (same meaning as `mena-compliance.md`):
> - **[PRIMARY]**: read on an official page in this research (iras.gov.sg, an ACRA/government document).
> - **[SECONDARY]**: reported consistently by professional or vendor sources (law firms, accounting firms, software vendors). The official text was not retrieved here.
> - **[UNCERTAIN]**: sources conflict, the official text was not found, or the item is interpretive. Flagged for review.

Sources used (all retrieved 2026-10-20):

| Key | Source |
|---|---|
| S-RATES | IRAS, *GST rates* (quick links: tax rates), https://www.iras.gov.sg/quick-links/tax-rates/goods-and-services-tax-(gst)-rates |
| S-F5 | IRAS, *Before filing your GST F5 return / Completing GST returns*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/completing-gst-returns |
| S-INV | IRAS, *Invoicing customers*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/basics-of-gst/invoicing-price-display-and-record-keeping/invoicing-customers |
| S-DUE | IRAS, *Due dates and requests for extension*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/due-dates-and-requests-for-extension |
| S-LATE | IRAS, *Late filing or non-filing of GST returns (F5/F8)*, https://iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/late-filing-or-non-filing-of-GST-returns-f5-f8 |
| S-F7 | IRAS, *Correcting errors made in GST return (filing GST F7)*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/correcting-errors-made-in-gst-return-(filing-gst-f7) |
| S-REC | IRAS, *Keeping records* (GST), https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/basics-of-gst/invoicing-price-display-and-record-keeping/keeping-records |
| S-INOW | IRAS, *GST InvoiceNow requirement*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/gst-invoicenow-requirement |
| S-IMDA | IMDA press release *InvoiceNow for GST-registered businesses* (2024), https://imda.gov.sg/resources/press-releases-factsheets-and-speeches/press-releases/2024/invoicenow-gst-registered-businesses |
| S-RC | IRAS, *GST on imported services*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/gst-and-digital-economy/gst-on-imported-services |
| S-EXEMPT | IRAS, *Supplies exempt from GST*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/charging-gst-(output-tax)/when-is-gst-not-charged/supplies-exempt-from-gst |
| S-OOS | IRAS, *Out-of-scope supplies*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/charging-gst-(output-tax)/when-is-gst-not-charged/out-of-scope-supplies |
| S-INTL | IRAS, *Providing international services* (zero-rating under s21(3)), https://www.iras.gov.sg/irashome/GST/GST-registered-businesses/Working-out-your-taxes/When-to-charge-0-GST/Providing-International-Services/ |
| S-REG | IRAS, *Do I need to register for GST*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/gst-registration-deregistration/do-i-need-to-register-for-gst |
| S-UEN | IDA/MOF, *Annex E – UEN format* (PDF, 6 Aug 2008), https://isomer-user-content.by.gov.sg/153/17ee6a09-7414-4e9b-ada9-3e6d9a01840e/annexe060808.pdf |
| S-SLA | SLA *Practice Circular 1 of 2009* (UEN year prefixes R/S/T), https://www.sla.gov.sg/qql/slot/u143/Newsroom/Circulars/2009/PRS%20-%20pc/Practice%20Circular%201%20of%202009.pdf (seen in search results only) |
| S-CS | IRAS, *Overview of Form C-S / Form C-S (Lite) / Form C*, https://www.iras.gov.sg/taxes/corporate-income-tax/form-c-s-form-c-s-(lite)-form-c-filing/overview-of-form-c-s-form-c-s-(lite)-form-c |
| S-XBRL | ACRA, *Who needs to file financial statements* (XBRL), https://www.acra.gov.sg/xbrl-filing-and-resources/who-needs-to-file-financial-statements |
| S-IGDS | IRAS, *Import GST Deferment Scheme (IGDS)*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/general-gst-schemes/import-gst-deferment-scheme-(igds) |
| SEC-* | Secondary: Rajah & Tann (InvoiceNow, Nov 2025), EY SG GST updates 02/2024 (F7 $3,000 concession), Osome / Sleek / Razorpay / 3E (UEN formats), DollarsAndSense / Kaizen CPA / Baker McKenzie (FYE rules), PwC / Wolters Kluwer (OVR thresholds) |

---

## 0. MVP must-do checklist

| # | Capability | Driven by |
|---|---|---|
| 1 | GST at the rate in force on the **time of supply**, from an effective-dated schedule: 3% → 4% → 5% → 7% → 8% (2023) → 9% (2024) | S-RATES |
| 2 | GST registration number check: UEN formats or the IRAS "M2-1234567-K" style. **Format only: no checksum is published** | S-INV, S-UEN |
| 3 | Tax invoice, simplified tax invoice (≤ S$1,000 incl. GST) and receipt templates with every mandatory field; SGD totals on foreign-currency invoices | S-INV |
| 4 | Standard-rated, zero-rated, exempt and out-of-scope treatments kept apart: they go to different F5 boxes, and out-of-scope supplies are not reported | S-F5, S-OOS |
| 5 | GST F5 Boxes 1–8 and 13 from the books; Boxes 9–12 and 14–21 shown as "not modelled" until reverse charge, schemes and claims exist | S-F5 |
| 6 | Return calendar: quarterly by default, due one month after the period ends | S-DUE |
| 7 | 5-year record retention; no hard deletion of issued documents | S-REC |
| 8 | **InvoiceNow** (Peppol PINT SG) transmission of invoice data to IRAS, phased by registration type and turnover, from 1 Nov 2025 to 1 Apr 2031 | S-INOW |

---

## 1. GST rate history

| Period | Rate | Source | Confidence |
|---|---|---|---|
| 1 Apr 1994 – 31 Dec 2002 | 3% | S-RATES | PRIMARY |
| 1 Jan 2003 – 31 Dec 2003 | 4% | S-RATES | PRIMARY |
| 1 Jan 2004 – 30 Jun 2007 | 5% | S-RATES | PRIMARY |
| 1 Jul 2007 – 31 Dec 2022 | 7% | S-RATES | PRIMARY |
| 1 Jan 2023 – 31 Dec 2023 | 8% | S-RATES; announced in Budget 2022 | PRIMARY |
| From 1 Jan 2024 | 9% (current) | S-RATES | PRIMARY |

**Transitional rules.** The rate is the one in force at the time of supply. For supplies that span a change, the earlier of invoice and payment decides; for example, IRAS says that where "an invoice is issued or payment is received before 1 January 2023, the GST rate is at 7%" (IRAS rate-change guidance, quoted via S-RATES search results [PRIMARY]). There are further rules for supplies spanning the change date (BDO Indirect Tax News 4/2022 [SECONDARY]).
**Software:** rates come from effective-dated data, never constants. KhataOS takes the rate in force on the **document date**. That is right whenever the invoice date is the time of supply. Where payment comes first (deposits) or a supply spans a rate change, the user must check the transitional rules [UNCERTAIN: not modelled].

---

## 2. Registration and identifiers

| Requirement | Source | Confidence | What the software must do |
|---|---|---|---|
| **Compulsory registration** when taxable turnover is more than **S$1 million** at the end of a calendar year (retrospective view), or is expected to exceed S$1 million in the next 12 months (prospective view). Taxable turnover means standard-rated and zero-rated supplies; exempt and out-of-scope supplies and sales of capital assets are excluded. Retrospective view: apply by 30 January, effective 1 March. Prospective view: forecasts made on or after 1 Jul 2025 are effective 2 months after the forecast date | S-REG | PRIMARY | Turnover monitor (later): a rolling 12-month sum of Box 1 + Box 2 values with an alert near S$1m |
| Voluntary registration is possible below the threshold, and the business must then stay registered for at least 2 years | S-REG (voluntary registration section, not re-read) | UNCERTAIN | Informational only |
| **GST registration number = UEN** when the business has a UEN and IRAS uses it ("If your UEN and GST registration number is the same": example `123456789A`). **Otherwise IRAS issues a separate number**, for example `M21234567K`, also written `M2-1234567-K` | S-INV | PRIMARY | Accept both. Store normalised (no spaces or hyphens, upper case). When the number is a UEN, expose it as the entity ID |
| **UEN formats.** Businesses registered with ACRA: `nnnnnnnnX` (8 digits + check letter, "9 digits" in the source, e.g. `12345678A`). Local companies: `yyyynnnnnX` (year + 5 digits + check letter, e.g. `199012345X`). All other entities (LLPs, foreign companies, public accounting firms, societies, representative offices, mutual benefit organisations…): `TyyPQnnnnX`, where "Tyy = year of issuance, PQ = type of entities, n = running number, X = check alphabet" (e.g. `T08LL1234A`). In use since 1 Jan 2009 | S-UEN | PRIMARY | Validate the three shapes |
| The year prefix letter is `R` for 18xx, `S` for 19xx and `T` for 20xx (e.g. `S99…`, `T08…`) | S-SLA (search snippet), SEC (Razorpay) | SECONDARY | Accept R, S or T as the first character of the third format |
| Entity-type codes (PQ) include `LL` (LLP), `LP` (limited partnership), `FC` (foreign company), `PF` (public accounting firm), `RF` (representative office / foreign registered entity), `SS` (society). The full list is maintained by ACRA and was not found | SEC (Osome, Razorpay, Sleek) | SECONDARY / UNCERTAIN | Do not check PQ against a list. Require a letter first and a letter or digit second |
| **UEN check letter algorithm.** The format names a "check alphabet", but no official algorithm was found. Community implementations exist, but none is published by ACRA | S-UEN (no algorithm given); no ACRA publication found | UNCERTAIN | **Do not validate a check letter.** Offer the IRAS "GST registered business search" / uen.gov.sg lookup when online |
| **"M" GST numbers.** IRAS shows only the example `M2-1234567-K` (M, a digit, 7 digits, a check letter). No specification or checksum is published, and older numbers in other shapes (e.g. a letter in the 2nd position or a digit at the end) are reported informally | S-INV [PRIMARY for the example]; others UNCERTAIN | PRIMARY / UNCERTAIN | Accept `M` + 8 characters where characters 3–9 are digits and characters 2 and 10 are letters or digits (a lenient superset of the published example). No checksum |
| Tax invoices must show the supplier's GST registration number | S-INV | PRIMARY | Print it on every tax invoice, simplified tax invoice and receipt |

---

## 3. Charging GST: treatments

| Treatment | What it covers | F5 box | Source | Confidence |
|---|---|---|---|---|
| **Standard-rated** (9%) | Local supplies of goods and services, including deposits taken as part payment, commercial property, hotel rooms | 1 (value), 6 (GST) | S-F5 | PRIMARY |
| **Zero-rated** (0%) | **Export of goods** (goods must leave Singapore; export evidence required) and **international services** listed in **s21(3) of the GST Act**. "Not all services provided to overseas customers can be zero-rated"; some categories depend on the customer "belonging" overseas | 2 | S-F5, S-INTL; goods-export evidence [SECONDARY] | PRIMARY (services), SECONDARY (export evidence rules) |
| **Exempt** | Financial services (Fourth Schedule to the GST Act), **sale and lease of residential property**, **import and local supply of investment precious metals (IPM)**, **digital payment tokens** (exchange and lending, from 1 Jan 2020). Intermediary and advisory services on these remain taxable. Box 3 also takes the absolute value of net realised exchange gains/losses | 3 | S-EXEMPT, S-F5 | PRIMARY |
| **Out of scope** | Sale of goods not brought into Singapore (overseas to overseas), sales of overseas goods within a Free Trade Zone or Zero GST Warehouse, private transactions. "They are not subject to GST and need not be reported in the GST return." Keep bills of lading or air waybills as evidence | none | S-OOS | PRIMARY |
| **Reverse charge (B2B)** | GST-registered businesses belonging in Singapore "which are not entitled to full input tax claims" account for GST on **imported services from 1 Jan 2020** and on **low-value goods from 1 Jan 2023**, as if they were the supplier | 1, 5, 6, 7, 14 | S-RC, S-F5 | PRIMARY |
| **Overseas Vendor Registration (OVR, B2C)** | Overseas vendors and electronic marketplaces account for GST on B2C digital services (from 1 Jan 2020), non-digital remote services and **low-value goods (≤ S$400, imported by air or post)** (from 1 Jan 2023) | 1, 15–17 | S-RC (LVG threshold), SEC (PwC, Wolters Kluwer) | PRIMARY (S$400, dates), SECONDARY (OVR thresholds: S$1m global and S$100,000 B2C) |
| **Customer accounting** | For prescribed goods the customer accounts for the GST, so the supplier leaves it out of Box 6 | 6 (exclusion) | S-F5 | PRIMARY (mention only; goods list not retrieved) |
| Imports of goods | GST is paid to Singapore Customs at import (or deferred under IGDS, or suspended under MES/A3PL) | 5, 7, 9, 18–21 | S-F5, S-IGDS | PRIMARY |

**Software:** a tax class per item or ledger (`STANDARD`, `ZERO`, `EXEMPT`, `OUT-OF-SCOPE`). The class name tells exempt and out-of-scope apart from zero-rated, because all three are 0%. Treat a sale to a party whose address is outside Singapore, charged no GST, as an export (zero-rated). The export conditions and the s21(3) categories are **not** checked.

---

## 4. Invoices

| Requirement | Source | Confidence |
|---|---|---|
| **Tax invoice** must show: the words "Tax Invoice"; the supplier's name and address; GST registration number; invoice date; an invoice identifying number; the customer's name and address; a description of the goods/services; the GST rate; the total amount payable excluding GST; the total GST; and the total amount payable including GST. Exempt, zero-rated and other supplies must be shown separately where applicable | S-INV | PRIMARY |
| **Simplified tax invoice** where the total payable (incl. GST) **does not exceed S$1,000**: the supplier's name, address and GST number; the date of issue; an identifying number; a description; the total payable including GST; and a statement like "price payable includes GST" | S-INV | PRIMARY |
| **Receipts** (instead of a tax invoice, to non-GST-registered customers): serially printed, with the date, business name, GST number, total including GST and the words "Price payable includes GST". Keep a duplicate | S-INV | PRIMARY |
| A tax invoice is generally issued **within 30 days** of the time of supply | S-INV | PRIMARY |
| **Invoice number.** IRAS asks only for "an invoice identifying number". No format, length or sequence rule is published | S-INV | PRIMARY (absence of rule) |
| **Rounding.** "You can round the total GST to be paid on all goods and services to the nearest cent (i.e. two decimal places)." The total bill may also be rounded to the nearest 5 cents if done consistently. The direction for a half cent is not stated on the page | S-INV | PRIMARY (cent, invoice total); UNCERTAIN (half-cent direction) |
| **Foreign currency.** Invoices may be in a foreign currency, but the total excluding GST, the total GST and the total including GST must also be shown in **SGD**, at exchange rates approved for GST purposes | S-INV | PRIMARY |
| Tax credit notes / debit notes adjust the original supply. They are documents for InvoiceNow too | S-INOW | PRIMARY |

**Software choices** (KhataOS, not legal rules): invoice numbers are unique across all years, at most 35 characters of letters, digits, "-" and "/" (same rule as the UAE pack, for e-invoice friendliness: PINT SG and UN/EDIFACT carry them). GST is rounded to the cent per line, half away from zero; an invoice-level figure (`document_gst`) is also provided for e-invoicing.

---

## 5. GST F5 return

**Frequency and due dates.** Accounting periods are normally quarterly (Jan–Mar, Apr–Jun, Jul–Sep, Oct–Dec). Monthly periods and special accounting periods exist. "Your GST return and payment are due 1 month after the end of your accounting period": 30 Apr, 31 Jul, 31 Oct and 31 Jan for the standard quarters. GIRO deductions are made on the 15th of the following month (S-DUE [PRIMARY]). Nil returns are required (S-LATE [PRIMARY]). Businesses under IGDS file monthly (S-IGDS [PRIMARY]).

The return has Boxes 1–17 plus an IGDS section (Boxes 18–21). Labels are quoted from S-F5 [PRIMARY]:

| Box | Label | Content | KhataOS |
|---|---|---|---|
| 1 | Total value of standard-rated supplies | Excluding GST. Includes deposits, and the value of imported services/LVG under reverse charge | Modelled (no reverse charge) |
| 2 | Total value of zero-rated supplies | Exports, qualifying international services, net of credit notes | Modelled |
| 3 | Total value of exempt supplies | Residential property, financial services, IPM, DPT; absolute value of net realised FX gains/losses | Modelled (FX gains not added) |
| 4 | Total value of (1) + (2) + (3) | Computed | Modelled |
| 5 | Total value of taxable purchases | Excluding GST: standard- and zero-rated purchases and imports for which input tax can be claimed, plus reverse-charge imports. Excludes disallowed expenses (certain motor car costs, staff medical costs), private, exempt, purchases from non-GST-registered suppliers, wages | Modelled: purchases from suppliers with a GST number, standard- and zero-rated lines; no disallowance applied |
| 6 | Output tax due | GST on Box 1, reverse-charge GST, bad debts recovered. Excludes customer-accounting supplies | Modelled (Box 1 GST only) |
| 7 | Input tax and refunds claimed | GST on Box 5 purchases, plus tourist refunds, bad debt relief, reverse-charge refunds, pre-registration claims | Modelled: GST on Box 5 purchases, all taken as claimable |
| 8 | Net GST to be paid to or claimed from IRAS | Box 6 − Box 7. "Net amounts under $5 are not paid or refunded" | Modelled; the S$5 rule is a note |
| 9 | Total value of goods imported under the MES / A3PL / other approved schemes | Only for scheme members | 0, not modelled |
| 10 | Did you claim GST you refunded to tourists? | Yes/No + amount included in Box 7 | No / 0 |
| 11 | Did you make any bad debt relief claims and/or refund claims for reverse charge transactions? | Yes/No + amount | No / 0 |
| 12 | Did you make any pre-registration claims? | Yes/No + amount; first return only | No / 0 |
| 13 | Revenue | Income from main sources (goods, services, gross sales). Excludes asset disposals, grants, amounts collected for others. "A best estimate is acceptable" | Estimate: net value of Sales and Credit Notes, all treatments, in the period |
| 14 | Did you import services and/or Low-Value Goods subject to GST under reverse charge? | Yes/No + value (also in Box 1). For businesses not entitled to full input tax claims | No / 0, not modelled |
| 15 | Did you operate an electronic marketplace to supply remote services subject to GST on behalf of third-party suppliers? | Yes/No + value (also in Box 1) | No / 0 |
| 16 | Are you a redeliverer or electronic marketplace operator supplying imported Low-Value Goods on behalf of third-party suppliers? | Yes/No + value; from 1 Jan 2023 | No / 0 |
| 17 | Did you supply imported Low-Value Goods that is subject to GST? | Yes/No + value; from 1 Jan 2023 | No / 0 |
| 18–21 | Import GST Deferment Scheme (IGDS) section | Only for IGDS-approved businesses. **Individual box labels were not retrieved** (see the IGDS e-Tax Guide, 10th ed., 1 Nov 2024) | Not modelled [UNCERTAIN labels] |

IRAS's own introduction calls the F5 "fifteen boxes" while its guidance covers Boxes 1–21; the box list above follows the box-by-box guidance [UNCERTAIN: wording mismatch on the IRAS page].

**F8** is the final return on deregistration (S-LATE [PRIMARY], mention only).

**F7 (corrections).** Errors in a filed F5, F7 or F8 are corrected by filing a GST F7 that restates all boxes. Exception: errors only in Box 13 need no correction. From 1 Jan 2024, errors in Boxes 1–7 whose net GST across all affected periods is **at most S$3,000** may instead be adjusted in the next F5 (administrative concession). A requested F7 must be e-filed within 14 days. Corrections must be made within 5 years from the end of the period. Under the Voluntary Disclosure Programme, there is no late-payment penalty if the F7 is filed within 1 year of the original due date and the conditions are met (S-F7 [PRIMARY for the process]; S$3,000 and dates via EY SG GST updates 02/2024 [SECONDARY]).

**Software:** the F5 screen is a preparation aid. A later "F7 helper" can diff a filed F5 snapshot against the recomputed figures.

---

## 6. Books: financial year, currency, records

| Item | Source | Confidence | Software |
|---|---|---|---|
| **Financial year.** A company chooses its own financial year end (FYE). For companies incorporated on or after 31 Aug 2018, the first financial year starts on incorporation and may not exceed 18 months without Registrar approval; later years run 12 months (or 52 weeks). The FYE can be changed via BizFile+, with restrictions (approval if over 18 months, or if changed within the last 5 years) | SEC (DollarsAndSense, Kaizen CPA, Baker McKenzie 2018 Companies Act update) | SECONDARY | Default to calendar years (KhataOS convention); the company sets its own first month. GST periods are independent of the FYE |
| **Currency**: Singapore dollar (SGD), 100 cents; GST figures in SGD | S-INV | PRIMARY | SGD, 2 decimals |
| **Record keeping**: business and accounting records for **at least 5 years**, including invoices, receipts, import/export documents, bank statements, stock lists and ledgers. If you switch accounting software, records from the old system must remain retrievable for 5 years | S-REC | PRIMARY | Retention warning; never purge; export to open formats |
| Missing records can lead to disallowed input tax, estimated assessments and penalties (up to S$5,000) | S-REC (search summary) | SECONDARY | — |

---

## 7. InvoiceNow (Peppol) for GST

From S-INOW [PRIMARY], with the earlier announcement in S-IMDA:

| Date | Who must transmit invoice data to IRAS via an InvoiceNow-Ready Solution |
|---|---|
| 1 May 2025 | Soft launch (voluntary adoption) [PRIMARY via search summary] |
| **1 Nov 2025** | Companies that register for GST **voluntarily within 6 months of incorporation** |
| **1 Apr 2026** | All businesses applying for **voluntary** GST registration on or after this date, regardless of incorporation date or business structure |
| **1 Apr 2028** | Businesses applying for **compulsory** registration on or after this date, **and existing** GST-registered businesses with total annual supplies (Box 4, for periods ending in calendar 2025) of **S$200,000 or less** |
| **1 Apr 2029** | Existing GST-registered businesses with total annual supplies of **S$1,000,000 or less** |
| **1 Apr 2030** | Existing GST-registered businesses with total annual supplies of **S$4,000,000 or less** |
| **1 Apr 2031** | Existing GST-registered businesses with total annual supplies **above S$4,000,000** |

- **What is sent:** invoice data for the standard-rated, zero-rated and exempt supplies and purchases reported in the GST return: sales invoices, tax invoices, simplified tax invoices, serially numbered receipts, debit notes and credit notes. **Not needed:** deemed supplies, goods exported without sales, reverse charge transactions, exempt financial services, exchange or loan of digital payment tokens, import permits. Data is due by the earlier of the GST return filing date and its due date.
- **Exempt from the requirement:** overseas entities (OVR pay-only and full regime) and businesses registered only because of reverse charge.
- Businesses registered before 2026 are notified of their date "from mid-2026". IRAS-funded free solution packages run to 31 Mar 2031.
- [UNCERTAIN] The earlier announcement (2024) and secondary sources (Rajah & Tann, Nov 2025) only described the first two phases. The 2028–2031 phases come from the current IRAS page, read 2026-10-20. Re-check before building.
- **Technical:** InvoiceNow is Singapore's Peppol network (IMDA is the Peppol Authority). Documents use the PINT SG specification, and transmission is through an accredited Access Point [SECONDARY: IMDA material, not re-read here]. **KhataOS future work:** a PINT SG UBL serialiser (from the neutral document model used by the F5) and Access Point integration. Invoice numbers already keep to a conservative character set for this.

---

## 8. Pointers (not modelled)

| Topic | Notes | Source | Confidence |
|---|---|---|---|
| **Corporate income tax** | Companies file Form C-S (Singapore-incorporated, revenue ≤ S$5m, income taxed only at 17%, no group relief, carry-back, investment allowance or foreign tax credit), Form C-S (Lite) (revenue ≤ S$200,000) or Form C (all others). Financial statements and tax computations must be kept ready for IRAS | S-CS | PRIMARY |
| **SFRS** | Companies prepare financial statements under Singapore Financial Reporting Standards (SFRS, or SFRS(I) for listed entities) or SFRS for Small Entities | SEC | SECONDARY |
| **XBRL** | Singapore-incorporated companies file financial statements with ACRA as part of the Annual Return in BizFile+. Some file a full XBRL set, others XBRL key data plus a PDF. Solvent exempt private companies are exempt from filing (but encouraged to). Prepared with the BizFinx tool | S-XBRL | PRIMARY (filing obligation), SECONDARY (exemption detail) |
| **Bad debt relief, pre-registration claims, tourist refunds (eTRS), Gross Margin Scheme, input tax disallowance (motor cars, medical), partial exemption apportionment** | All affect Boxes 5–7 and 10–12 | S-F5 | PRIMARY (existence) |

---

## 9. Open items

1. Half-cent rounding direction for GST (IRAS says "nearest cent"). KhataOS rounds half away from zero. [UNCERTAIN]
2. The official list of UEN entity-type (PQ) codes, and whether any ACRA check-letter algorithm is officially published. [UNCERTAIN]
3. Shapes of "M" GST numbers other than IRAS's `M2-1234567-K` example. [UNCERTAIN]
4. IGDS Boxes 18–21 labels (see the IGDS e-Tax Guide appendix). [UNCERTAIN]
5. Transitional rate rules across the 2023/2024 changes (deposits, spanning supplies) are not modelled. [UNCERTAIN]
6. The InvoiceNow technical spec (PINT SG version, Access Point accreditation) must be checked on IMDA's site before work begins.

## 10. Addendum (21 Oct 2026): reverse charge, imports, MES and IGDS

Checked against IRAS for the VAT-pack work (`docs/singapore-gst.md`).
Sources [PRIMARY]:
- e-Tax Guide **"GST: Reverse Charge"**, 10th ed., 30 Jan 2026 (file still
  named "…(2nd Edition).pdf"): <https://www.iras.gov.sg/irashome/uploadedFiles/IRASHome/e-Tax_Guides/GST%20Taxing%20imported%20services%20by%20way%20of%20reverse%20charge%20(2nd%20Edition).pdf>
- "Completing GST returns": <https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/completing-gst-returns>
- e-Tax Guide "GST: Major Exporter Scheme", 18th ed., 3 Aug 2026: <https://www.iras.gov.sg/docs/default-source/e-tax/etaxguide_gst_gst-major-exporter-scheme.pdf?sfvrsn=a4316528_116>
- e-Tax Guide "GST: Import GST Deferment Scheme", 11th ed., 30 Jan 2026: <https://www.iras.gov.sg/docs/default-source/e-tax/etaxguide_gst_import-gst-deferment-scheme.pdf?sfvrsn=ea5681d9_102>
- "Importing of goods": <https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/claiming-gst-(input-tax)/importing-of-goods>
- The OVR guide for low-value goods (5th ed., 30 Jan 2026) covers suppliers,
  not reverse-charge recipients.

**Box labels** (Completing GST returns): 1-8 as in §5; 9 "Total value of
goods imported under the MES/ A3PL/ other approved schemes"; 10 tourist
refunds; 11 "bad debt relief claims and/or refund claims for reverse
charge transactions"; 12 pre-registration claims; 13 revenue; **14 "Did you
import services and/or Low-Value Goods subject to GST under reverse
charge?"** (Y/N + value); 15-17 marketplaces, redeliverers and own
low-value goods; 18-21 the IGDS section, only for IGDS-approved
businesses. Box 19 "Deferred import GST payable", Box 21 "Total value of
goods imported under this scheme" (IGDS guide App. D(c)); Boxes 18 and 20
probably the Box 8 net GST and the total payable (Box 8 + Box 19)
[UNCERTAIN: inferred from the §3.4 diagram]; IGDS boxes renumbered in the
8th edition (1 Jan 2023). Box 8: under S$5 neither paid nor refunded, not
carried forward.

**Reverse charge** (RC guide):
- RC business (§§4.1.2-4.1.5): GST-registered and not entitled to full
  input tax credit (non-business supplies, or failing the reg 28 de minimis
  rule: exempt supplies ≤ S$40,000 a month on average and ≤ 5% of total
  supplies, RC import values excluded), or in a GST group with such a
  member (then every member). It accounts "as if you were the supplier"
  and may claim the GST as input tax (§4.1.3).
- Boxes (§11.1.1): the value in **Boxes 1, 5 and 14**, the output tax in
  **Box 6**, the claimable input tax in **Box 7** (normal recovery rules).
  Confirmed by "Completing GST returns" for each box.
- Unpaid overseas supplier after 12 months: net GST in Box 7 and Box 11,
  within 5 years; on later payment, output tax in Box 6 and input tax in
  Box 7 (§§11.2.1-11.2.2, Example 32). Exempt-only registrants by
  approval: Boxes 1, 14 and 6 only (§11.1.2). Pay-Only GST RC Groups:
  Boxes 1, 6, 11, 14, half-yearly.
- Low-value goods (§4.3.1): not dutiable (or duty waived), not exempt,
  outside Singapore at sale, by air or post, **S$400 or less per item**,
  tested on the s18 import value (CIF + duty + charges, §4.3.4; Examples 7,
  8). From 1 Jan 2023 RC businesses reverse-charge all LVG, from any
  supplier (§4.3.3), except LVG directly attributable to taxable supplies
  when no fixed or special recovery rate applies (§4.3.2). A fully taxable
  business is not an RC business and need not complete Box 14.

**Imports with GST paid to Singapore Customs**: GST at 9% on CIF + duties +
commission and incidentals (CIF $12,000 + duty $3,600 = $15,600; GST
$1,404, "Importing of goods"). Box 5 "based on the value of import
reflected in the import permits", reconciled with the supplier's invoice
(over-declared permits: the correct invoice value; under-declared: a
short-payment permit); Box 7 "based on the GST amount reflected in the
import permit", only for the importer named on the permit. No import line
in Box 6. Whether Box 5 is CIF only or CIF + duty [UNCERTAIN; the permit
value used].

**MES / A3PL** (MES guide App. B): the import value in **Box 5 and Box 9**,
no GST in Box 7; ZG warehouse removals likewise; later local sales
standard-rated, exports zero-rated; a payment permit used by mistake →
Box 5 + Box 7.

**IGDS** (IGDS guide App. D, §§3.2, 3.4, 4(d)): deferred import GST in
**Box 19** (not Box 6), value in **Box 21**, in the period of the permit's
approval date; if claimable, value in Box 5 and GST in Box 7 (example:
output $15,000, input $10,000, deferred $10,000 → pays $15,000); monthly
filing.

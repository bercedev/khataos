# E-invoicing on the shared EN 16931 / Peppol model: PINT SG, PINT AE, Factur-X

> **Status as of:** 2026-10-21. **Scope:** what KhataOS needs to write Peppol PINT invoices for Singapore (InvoiceNow) and the UAE (Electronic Invoicing System), and Factur-X / ZUGFeRD hybrid PDFs for EU companies, all from the `khata-en16931` invoice model.
> **Not legal advice.** This file turns official specifications into software requirements. Have a tax adviser in each country confirm the legal points, and re-validate generated files with the official validators before relying on them. All three standards changed during 2026. Re-check the docs.peppol.eu release notes, the IRAS InvoiceNow page, the UAE MoF e-invoicing page and the FNFE-MPE / FeRD pages every quarter.
>
> **Confidence tags** (same meaning as `mena-compliance.md` and `singapore-compliance.md`):
> - **[PRIMARY]**: read in an official artefact in this research: a docs.peppol.eu page, a Schematron or code list from the official `resources.zip`, an IRAS or MoF page.
> - **[SECONDARY]**: reported by news, law-firm or vendor sources, or taken from a reference implementation (akretion's `facturx.py`). The official text was not read.
> - **[UNCERTAIN]**: sources conflict, the official text was not found, or the item is interpretive.

Sources (all retrieved 2026-10-20 or 2026-10-21):

| Key | Source |
|---|---|
| P-SG | OpenPeppol, *PINT SG Billing v1.4.1* (release 25 May 2026), https://docs.peppol.eu/poac/sg/pint-sg/ ; BIS text https://docs.peppol.eu/poac/sg/pint-sg/bis/ ; syntax https://docs.peppol.eu/poac/sg/pint-sg/trn-invoice/syntax/ ; code lists https://docs.peppol.eu/poac/sg/pint-sg/trn-invoice/codelist/ |
| P-SG-R | *Singapore specific PINT rules 1.4.0*, https://docs.peppol.eu/poac/sg/pint-sg/trn-invoice/rule/PINT-jurisdiction-aligned-rules/ , and the Schematron in https://docs.peppol.eu/poac/sg/pint-sg/resources.zip (`trn-invoice/schematron/*.sch`, examples, code lists, UBL 2.1 XSD) |
| P-AE | OpenPeppol, *PINT AE Billing 1.0.4*, *PINT AE Self-Billing 1.0.4*, *UAE Tax Data Document 1.0.4* (PDK 1.4.4, 29 Jul 2026), https://docs.peppol.eu/poac/ae/ ; syntax https://docs.peppol.eu/poac/ae/pint-ae/trn-invoice/syntax/ |
| P-AE-R | PINT AE Schematron, examples and code lists in https://docs.peppol.eu/poac/ae/pint-ae/resources.zip (`PINT-jurisdiction-aligned-rules.sch`, `UNCL1001-inv.gc`, `CreditReason.gc`, `Aligned-TaxCategoryCodes.gc`, `Aligned-TaxExemptionCodes.gc`, `transactiontype.gc`, `GoodsType.gc`, `ItemType.gc`, `UNCL4461.gc`, `eas.gc`) |
| P-PINT | Shared PINT rules (`PINT-UBL-validation-preprocessed.sch`, in both zips above); PINT core pages https://docs.peppol.eu/poac/pint/pint/ |
| S-INOW | IRAS, *GST InvoiceNow Requirement*, https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/gst-invoicenow-requirement |
| S-ID | Singapore Peppol ID form: Terra Advisory, https://terraadvisoryservices.com/how-to-register-for-a-peppol-id-in-singapore/ ; Raffles Corporate Services, https://rafflescorporateservices.com/?p=9462 ; SQL Account, https://docs.sql.com.my/sqlacc/singapore/invoice-now |
| U-MOF | UAE MoF, *UAE Electronic Invoicing Guidelines* (v1.1, 1 Jun 2026; the PDF URL carries v1.0 23 Feb 2026), https://mof.gov.ae/wp-content/uploads/2026/02/UAE-Electronic-Invoicing-Guidelines_V-1.0-23Feb2026.pdf ; earlier research in `mena-compliance.md` §3 |
| U-NEWS | WAM, *MoF announces targeted amendments to eInvoicing system decisions* (10 May 2026), https://www.wam.ae/en/article/c055277-mof-announces-targeted-amendments-einvoicing ; Gulf News, https://gulfnews.com/amp/story/business%2Fuae-announces-extension-of-einvoicing-provider-deadline-to-october-2026-1.500535895 ; Sovos, https://sovos.com/regulatory-updates/global-vat/uae-e-invoicing-service-provider-appointment-deadline-for-phase-1-businesses-extended/ ; Banqup, https://www.banqup.com/en-be/resources/blog/uae-confirms-phased-e-invoicing-mandate-rollout |
| F-FNFE | FNFE-MPE, *Factur-X* page, https://fnfe-mpe.org/factur-x/factur-x_en/ ; press releases: 1.08 / ZUGFeRD 2.4 (4 Dec 2025) https://fnfe-mpe.org/wp-content/uploads/2025/12/2025-12-04_Factur-X_1.08_ZUGFeRD_2.4_Press_Release_EN.pdf ; 1.09 / 2.5 (10 Jun 2026) https://fnfe-mpe.org/wp-content/uploads/2026/06/2026-06-10_Factur-X_1.09_ZUGFeRD_2.5_Press_Release_EN.pdf |
| F-FERD | FeRD, *ZUGFeRD 2.4 (English)*, https://www.ferd-net.de/en/downloads/publications/details/zugferd-24-english ; Factur-X 1.07.2 (DE), https://easyfirma.net/wp-content/uploads/2024/12/1.-FACTUR-X-1.07.2-DE.pdf (seen in search results only) |
| F-AKR | akretion, *factur-x* (Python reference library), https://github.com/akretion/factur-x : `src/facturx/facturx.py`, `src/facturx/xmp/Factur-X_extension_schema.xmp` |
| F-SEC | Secondary: Gotenberg Factur-X docs https://gotenberg.dev/docs/manipulate-pdfs/factur-x ; VATCalc https://vatcalc.com/france/france-german-e-invoicing-standard-update ; Shared Services Link https://sharedserviceslink.com/news/factur-x-and-zugferd-updated-ahead-of-france-and-germany-e-invoicing-reforms ; docs.rs `en16931-formats` https://docs.rs/crate/en16931-formats/latest/source/src/zugferd/mod.rs |

---

## 1. Peppol PINT: the international core

- **What it is.** Peppol International (PINT) is OpenPeppol's invoice model for any country. It reuses the EN 16931 semantic model with "IBT-n" terms (the EN 16931 BT-n plus extra terms, such as IBT-168 issue time and IBT-185/186 line exemption reason). It drops the EU-specific rules: VAT category codes and VAT-number prefixes are left to each jurisdiction. The syntax is UBL 2.1 `Invoice` / `CreditNote` [PRIMARY: P-SG, P-AE syntax pages].
- **Identifiers.** A jurisdiction specialises PINT with its own CustomizationID (IBT-024): `urn:peppol:pint:billing-1@<jurisdiction>-1`. Singapore uses `urn:peppol:pint:billing-1@sg-1` and the UAE `urn:peppol:pint:billing-1@ae-1`. The ProfileID (IBT-023, business process) is `urn:peppol:bis:billing`. The UAE also has `urn:peppol:pint:selfbilling-1@ae-1` with `urn:peppol:bis:selfbilling` [PRIMARY: P-SG BIS, PINT-SG examples, P-AE-R aligned-ibrp-001-ae / 002-ae].
- **Shared rules** (`ibr-*`, all `fatal`) that matter for a writer [PRIMARY: P-PINT]:
  - both electronic addresses are mandatory, each with a scheme: Seller EndpointID (ibr-081, ibr-062) and Buyer EndpointID (ibr-080, ibr-063). Schemes come from the CEF EAS list (ibr-cl-25);
  - Note (IBT-022) at most once (ibr-sr-51). Several notes must be joined into one;
  - no empty elements (ibr-079); dates `YYYY-MM-DD` (ibr-073);
  - totals: ibr-co-10 to 16, at most 2 decimals on totals (ibr-091, 121 to 125), the line net price not negative (ibr-027), base quantity above zero (ibr-087);
  - a supplier identifier (IBT-029, IBT-030 or IBT-031) is present (ibr-co-26). At most two PartyTaxScheme on the seller (ibr-sr-42);
  - payment means code from UNCL 4461 (ibr-cl-16); unit codes UN/ECE Rec 20/21 (ibr-cl-23);
  - all `currencyID` equal to the invoice currency, except tax-accounting-currency amounts (ibr-126). The tax accounting currency (IBT-006) differs from IBT-005 (ibr-077);
  - the CustomizationID contains no wildcard `*` (ibr-sr-63). That rule exists because Peppol SMP lookups can match PINT customizations by wildcard [UNCERTAIN: the SMP wildcard scheme was not read here].
- **Aligned rules** (`aligned-ibrp-*`) restate the EN 16931 category rules (S/Z/E/O/AE -01, -05 to -10) for jurisdictions that keep the UNCL 5305 codes. The UAE uses them, with UAE edits (`-ae` suffixes). Singapore replaces them with its own GST rules (§2.3).
- **How a jurisdiction specialises:** its own tax category code list, the tax scheme ID (`GST` in Singapore, `VAT` in the UAE), its party-identifier scheme, extra terms (BT-SG-*, BTAE-*), and its own Schematron run on top of the shared one.
- **Peppol document type identifier** for SMP registration and AS4 routing. The pattern is `<root namespace>::<root element>##<CustomizationID>::2.1`, scheme `busdox-docid-qns`. For PINT SG that gives `urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##urn:peppol:pint:billing-1@sg-1::2.1` [UNCERTAIN: the BIS 3.0 form was confirmed in vendor docs; the exact PINT string and whether PINT uses the wildcard scheme `peppol-doctype-wildcard` was not confirmed in an official page]. Access Points normally set it. KhataOS only passes it as information.

## 2. Singapore: InvoiceNow and PINT SG

### 2.1 Specification and identifiers

| Item | Value | Confidence |
|---|---|---|
| Current version | PINT SG Billing v1.4.1, released 25 May 2026. Singapore-specific rules 1.4.0. It replaces the older "SG Peppol BIS Billing 3.0" | [PRIMARY: P-SG] |
| CustomizationID | `urn:peppol:pint:billing-1@sg-1` (PEPPOL-EN16931-R004-SG checks that it *starts with* this) | [PRIMARY: P-SG-R] |
| ProfileID | `urn:peppol:bis:billing` (R007-SG) | [PRIMARY] |
| Tax scheme ID | `GST` on PartyTaxScheme and on every TaxCategory / ClassifiedTaxCategory | [PRIMARY: P-SG BIS examples] |
| Seller GST registration number | IBT-031 in `PartyTaxScheme/CompanyID` with scheme `GST` (the example uses an IRAS number `M2-1234567-K`). No format rule in PINT SG | [PRIMARY] |
| Legal registration (UEN) | IBT-030 in `PartyLegalEntity/CompanyID` (example `200212345Z`, no schemeID) | [PRIMARY: example 02] |
| Electronic address scheme | EAS `0195` = "Singapore UEN identifier" | [PRIMARY: eas.gc] |
| Peppol participant ID | `0195:SGUEN<UEN>`, e.g. EndpointID `schemeID="0195"` with value `SGUEN200212345Z` | [PRIMARY: PINT-SG example 02]; form confirmed by [SECONDARY: S-ID]. Government buyers have their own IDs (e.g. `0195:SGUENT08GA0028A`) [SECONDARY] |
| Unique invoice identifier | BT-SG-003, `cbc:UUID` right after `cbc:ID`, 0..1 in the syntax. It becomes mandatory through BR-108 and is checked as a canonical UUID (BR-109) | [PRIMARY] |
| Invoice type codes | UNCL 1001 invoice subset; CreditNote document for credit notes | [PRIMARY: code lists] |
| Payment means | UNCL 4461 or `Z01` (GIRO) / `Z02` (PayNow, where the UEN is the payee account in `PayeeFinancialAccount/cbc:ID`) | [PRIMARY: BR-CL-16-SG, P-SG BIS] |

### 2.2 GST category codes (code list `GST-cat`)

| Code | Meaning (code list) | Rate in the BIS table |
|---|---|---|
| SR | Standard rated: local supply of goods and services | 9% |
| SRCA-S | Standard rated, customer accounting: supply made by the supplier | N/A |
| SRCA-C | Standard rated, customer accounting: supply made by the customer on the supplier's behalf | 9% |
| SROVR-RS | Remote services accountable by the electronic marketplace under OVR | 9% |
| SROVR-LVG | Low-value goods accountable by the redeliverer or marketplace for third-party suppliers | 9% |
| SRRC | Reverse charge for B2B supplies of imported services | 9% |
| SRLVG | Own supply of low-value goods | 9% |
| ZR | Zero rated: goods for export, international services | 0% |
| ES33 | Exempt supplies listed under regulation 33 of the GST (General) Regulations | N/A |
| ESN33 | Exempt supplies other than regulation 33 | N/A |
| DS | Deemed supplies that must be reported | 9% |
| OS | Out-of-scope supplies (goods not brought into Singapore, FTZ / zero-GST warehouse sales, private transactions) | N/A |
| NA | Taxable supplies where GST need not be charged (Gross Margin, AMFT, 3PL schemes) | N/A |
| NG | Supplies from a company not registered for GST | N/A |

[PRIMARY: P-SG code list and BIS Table 3.] The BIS says the GST rate is given for every category except NG (BR-48-GST-SG). The table's "N/A" is read as "no GST charged": write `0`. That reading is [UNCERTAIN] for SRCA-S, NA and the exempt codes, because the table and the rule wording differ.

### 2.3 Singapore rules (all flagged `fatal` in the Schematron)

[PRIMARY: P-SG-R.] "SR-type" below means any of SR, SRCA-S, SRCA-C, ZR, SRRC, SROVR-RS, SROVR-LVG, SRLVG or NA in the breakdown.

- **R003-SG**: Buyer reference (IBT-010) **or** purchase order reference (IBT-013) is mandatory.
- **BR-105**: SR-type → Seller GST identifier (or the tax representative's).
- **BR-106 / 107**: SR-type → Seller and Buyer `StreetName` (address line 1) and `PostalZone`.
- **BR-108 / 109**: SR-type → `cbc:UUID`, in UUID format.
- **BR-111**: an SR-type CreditNote has a Note with the reason for credit.
- **BR-112**: SR-type → Seller legal registration identifier (IBT-030, the UEN).
- **BR-113**: SR-type → if the invoice currency is not SGD, `TaxCurrencyCode` = SGD. If it is SGD, no TaxCurrencyCode. With a tax currency: a second TaxTotal in SGD (BR-53, R054), and the SGD totals in `AdditionalDocumentReference` with `DocumentTypeCode` `sgdtotal-excl-gst` / `sgdtotal-incl-gst`, `ID` = `SGD`, and the amount in `DocumentDescription` (BR-100 to 104, BR-110-GST).
- **BR-CO-25-SG**: amount due > 0 → Payment due date or Payment terms.
- **BR-11-SG**: Buyer country code.
- **BR-CO-10 to 18-GST-SG, BR-DEC-*-SG**: the EN 16931 sums and decimals with "GST" wording. BR-CO-17-GST-SG: tax = taxable × rate ÷ 100.
- **BR-NG-01 to 14**: an NG invoice has exactly one NG breakdown and nothing else, with tax 0, and no Seller / tax representative / Buyer GST identifiers.
- **BR-CL-17 / 18**: tax category codes only from the SG list (on TaxCategory and ClassifiedTaxCategory).
- **BR-CL-16-SG**: payment means UNCL 4461 or Z01 / Z02.
- **BR-110-SG**: order references on the document or on lines, not both. **BR-CO-03-GST-SG**: tax point date and tax point date code are exclusive.
- **UBL-SR-43**: `AdditionalDocumentReference/DocumentTypeCode` only for 130 (invoiced object), 50 (project, CreditNote) or the two SGD totals.

The Schematron scope note says these rules apply to every Singapore sender, but only receivers with Singapore receiving capability apply them. Other receivers apply only the shared PINT rules.

### 2.4 InvoiceNow: the IRAS requirement

From S-INOW [PRIMARY, read 2026-10-21]:

| Date | Who must transmit invoice data to IRAS |
|---|---|
| 1 Nov 2025 | Companies that register for GST voluntarily within 6 months of incorporation |
| 1 Apr 2026 | All businesses applying for voluntary GST registration on or after this date |
| 1 Apr 2028 | New compulsory registrants from this date, and existing registrants with total annual supplies of S$200,000 or less |
| 1 Apr 2029 | Existing registrants with ≤ S$1,000,000 |
| 1 Apr 2030 | Existing registrants with ≤ S$4,000,000 |
| 1 Apr 2031 | Existing registrants above S$4,000,000 |

Total annual supplies are based on prescribed accounting periods ending in calendar 2025. IRAS notifies businesses registered before 2026 of their date from mid-2026.

- **What IRAS receives** (the "corner 5" copy): data for the standard-rated, zero-rated and exempt supplies **and purchases** reported in the GST return. That means sales and tax invoices, simplified tax invoices, serially numbered receipts, debit and credit notes. POS, simplified-invoice and petty-cash items may be aggregated.
- **Not needed:** deemed supplies and goods exported without sales, reverse charge transactions, exempt financial services, digital payment tokens, import permits.
- **Exempt businesses:** overseas entities (OVR pay-only and full regime) and businesses registered only because of reverse charge.
- **Due:** by the earlier of the date the relevant GST return is filed and its due date.
- **How:** IRAS receives a copy through **IMDA-accredited Access Point Providers** once the business activates the submission feature. Off-the-shelf software users go through an **InvoiceNow-Ready Solution** provider and get a Peppol ID in the SG Peppol Directory using their UEN. Businesses with in-house systems connect through an AP. KhataOS is neither: it can only produce the PINT SG file and hand it to an AP or solution.
- [UNCERTAIN] The data that reaches IRAS for purchases (received invoices) and for B2C or aggregated sales may not be a full PINT SG invoice. That was not read here.

## 3. UAE: PINT AE and the Electronic Invoicing System

### 3.1 Specification and identifiers

| Item | Value | Confidence |
|---|---|---|
| Versions | PINT AE Billing 1.0.4, PINT AE Self-Billing 1.0.4, UAE Tax Data Document (TDD) 1.0.4, PDK 1.4.4, released 29 Jul 2026 | [PRIMARY: P-AE] |
| CustomizationID / ProfileID | `urn:peppol:pint:billing-1@ae-1` / `urn:peppol:bis:billing`. Self-billing: `urn:peppol:pint:selfbilling-1@ae-1` / `urn:peppol:bis:selfbilling` | [PRIMARY: aligned-ibrp-001-ae / 002-ae] |
| Transaction type (BTAE-02) | `cbc:ProfileExecutionID`, 1..1: exactly 8 characters of 0/1 (ibr-154). Positions: 1 free trade zone, 2 deemed supply, 3 profit margin scheme, 4 summary invoice, 5 continuous supply, 6 agent billing, 7 e-commerce, 8 exports | [PRIMARY: transactiontype.gc] |
| UUID (BTAE-07) | `cbc:UUID`, 1..1 (ibr-193) | [PRIMARY] |
| Invoice type codes | `380` commercial invoice, `480` invoice out of scope of tax. Credit notes: `381` credit note, `81` credit note related to goods or services | [PRIMARY: UNCL1001-inv/-cn.gc] |
| Tax scheme | `VAT`; a seller with only a TIN (IBT-032) uses another scheme code (ibr-133, ibr-178) | [PRIMARY] |
| TRN | IBT-031 / IBT-048 for AE parties: 15 digits, starting with `1` and ending with `03` (ibr-132) | [PRIMARY] |
| TIN | IBT-032: 10 digits `1XXXXXXXXX` (ibr-148) | [PRIMARY] |
| Participant ID | EAS `0235` = "UAE Tax Identification Number (TIN)": `0235:<10-digit TIN>`. Reserved endpoints: `9900000097` deemed supply (report only), `9900000098` buyer not yet on e-invoicing (also issue a paper/PDF tax invoice), `9900000099` export buyer without a Peppol ID | eas.gc [PRIMARY]; reserved values U-MOF §3, §10 [PRIMARY via mena-compliance.md] |
| QR code | None on UAE e-invoices | [PRIMARY: U-MOF via mena-compliance.md] |

### 3.2 VAT category codes and code lists

- Categories (`Aligned-TaxCategoryCodes`): `S` standard, `Z` zero rated, `E` exempt, `O` outside scope / not subject, `AE` reverse charge, and `N` "standard rate additional VAT" (VAT on an additional taxable base not in the document totals) [PRIMARY]. **Artefact oddity:** the `N` entry is the Greek capital letter Nu (U+039D), not Latin `N`. Since ibr-139 checks codes against this list, a Latin `N` may fail the official validator. This looks like a bug [UNCERTAIN].
- Exemption reasons (`Aligned-TaxExemptionCodes`, IBT-121/186): `DL8.46.1` certain financial services, `DL8.46.2` residential units (lease or sale), `DL8.46.3` bare land, `DL8.46.4` local passenger transport [PRIMARY].
- Credit note reasons (`CreditReason`, BTAE-03 in `cac:DiscrepancyResponse/cbc:ResponseCode`): `DL8.61.1.A` supply cancelled, `.B` tax treatment changed, `.C` consideration altered (e.g. bad debt relief), `.D` goods or services returned, `.E` tax charged or treatment applied in error, `VD` volume discount [PRIMARY].
- Reverse-charge goods types (`GoodsType`, BTAE-09 `cbc:NatureCode`): `DL8.48.8.2` electronic devices, `DL8.48.8.1` gold and diamonds, `DL8.48.3.1/2/3` crude or refined oil, natural gas, pure hydrocarbons [PRIMARY]. Item type (BTAE-13 `cbc:CommodityCode`): `G` goods, `S` services, `B` both [PRIMARY].
- Payment means: UNCL 4461 subset including `1` (not defined), `10` cash, `30` credit transfer, `49` direct debit, `54`/`55` credit/debit card, `68` online payment service [PRIMARY: UNCL4461.gc].

### 3.3 Mandatory content beyond PINT (selected UAE rules, all `fatal`)

[PRIMARY: P-AE-R; element names from the P-AE syntax page.]

- **Addresses:** seller and buyer `StreetName`, `CityName` and `CountrySubentity` (ibr-143/144). For country AE, `CountrySubentity` is one of `AUH, DXB, SHJ, UAQ, FUJ, AJM, RAK` (ibr-128).
- **Legal registration:** with a `0235` seller endpoint, IBT-030 is mandatory (ibr-150). Its type (BTAE-15) is in `PartyLegalEntity/CompanyID/@schemeAgencyID`: `TL` trade licence, `EID` Emirates ID, `PAS` passport, `CD` cabinet decision (ibr-173, 181). The issuing authority (BTAE-12) goes in `@schemeAgencyName` and is required for `TL` (ibr-172). A passport needs the issuing country (ibr-012). Buyer side: ibr-180, 183 (`CL` is also accepted there), 101, 010.
- **Buyer identification:** with endpoint `0235` not starting with 1 or 9, a buyer TRN is required (ibr-149). Outside exports, a non-TIN `0235` endpoint needs a buyer identifier or TRN (ibr-135).
- **Seller TRN** (IBT-031) is mandatory, except on 480 / 81 documents (ibr-134).
- **Lines:** Item description (ibr-125). Price with `BaseQuantity` and a price `AllowanceCharge` carrying the gross price `BaseAmount` (ibr-126), with net = gross − discount (aligned-ibrp-004). `cac:ItemPriceExtension/cbc:Amount` (BTAE-10, line amount with VAT, ibr-194) and `ItemPriceExtension/cac:TaxTotal/cbc:TaxAmount` (BTAE-08, line VAT, ibr-104), which is left out for `E` lines (ibr-163) and is 0 for `Z` and `AE` (ibr-165, 162). One ClassifiedTaxCategory per line (ibr-123).
- **Category rates:** `S` > 0 (aligned-ibrp-s-05, ibr-190), `Z` = 0 (ibr-120), `E` and `O` without a rate on lines and breakdowns (aligned-ibrp-e-05, ibr-121, aligned-ibrp-o-05, -o-11-ae), `AE` with a rate on the line (aligned-ibrp-ae-05-ae) and tax 0 in the breakdown. `E` needs an exemption reason code (ibr-167). A 380 or 381 must have at least one line that is not `E` or `O` (ibr-151).
- **Reverse charge** (`AE`) needs the buyer TRN (ibr-103), the goods type BTAE-09 from the GoodsType list (ibr-166, ibr-006), and an item standard identifier with scheme `0160` (GTIN) (ibr-174).
- **Invoices:** a payment due date when the amount due is > 0 (ibr-127) and a payment means code (ibr-191; credit notes and deemed supplies excepted). Credit transfer needs an account (ibr-192).
- **Credit notes:** a reason code (ibr-158). A preceding invoice reference unless the reason is `VD` (ibr-055). No tax point date (ibr-124).
- **Currency:** any `TaxCurrencyCode` is `AED` (ibr-140). For a non-AED invoice, an exchange rate (BTAE-04, at most 6 decimals, ibr-159/002) and the AED totals (ibr-175).
- **Transaction-type driven:** a free zone needs the beneficiary ID BTAE-01 (ibr-007), agent billing a principal ID (ibr-137, 176), a summary invoice an invoicing period (ibr-138), e-commerce and exports a deliver-to address (ibr-142, 152), the margin scheme category `N` (ibr-116).
- **Goods classification:** item type `G` needs an HS code (`ItemClassificationCode listID="HS"`, ibr-184, 188), and `S` a service accounting code (`AdditionalItemIdentification` with scheme `SAC`, ibr-185, 189). Item type itself is optional (0..1).

### 3.4 Model, ASPs and timeline

- **Five-corner model:** the supplier (C1) gives invoice data to its Accredited Service Provider (C2). C2 validates, converts to PINT AE, sends to the buyer's ASP (C3) over Peppol, and reports a **Tax Data Document** to the FTA (C5). C3 delivers to the buyer (C4) and reports too. One ASP per person for sending and receiving [PRIMARY: U-MOF §5.1, §6.1 via mena-compliance.md].
- **ASPs:** 32 service providers approved by 10 May 2026, with more in final accreditation. A white-label mechanism (amendment to MD 64/2025) lets UAE companies partner with international technology providers. One source adds a two-year operating-history requirement for proposed solutions [SECONDARY: U-NEWS; the two-year item is from Banqup only, UNCERTAIN].
- **Timeline** (MD 244/2025 as amended):
  - pilot and voluntary adoption from 1 Jul 2026;
  - revenue ≥ AED 50m: appoint an ASP by **30 Oct 2026** (extended from 31 Jul 2026 on 10 May 2026), live **1 Jan 2027** (unchanged);
  - revenue < AED 50m: ASP by 31 Mar 2027, live 1 Jul 2027;
  - government entities: live 1 Oct 2027.
  
  [PRIMARY for MD 244 dates via mena-compliance.md; the extension is SECONDARY: WAM, Gulf News, Sovos. The amending decision is reported as MD 56/2026 (Banqup) and elsewhere as MD 66/2026: UNCERTAIN. The < AED 50m dates in 2026 reporting come from one source: UNCERTAIN.]
- **Scope reminders** (from `mena-compliance.md` §3): B2B and B2G whether or not VAT-registered, B2C excluded for now, 14 days to issue, and an e-invoice must be stored "within the State" with the ASP and FTA confirmations.

## 4. Factur-X / ZUGFeRD

### 4.1 Versions

| Version | Date | Notes | Confidence |
|---|---|---|---|
| Factur-X 1.07.2 / ZUGFeRD 2.3.2 | Nov 2024 | CII D16B | [SECONDARY: F-FERD search result] |
| Factur-X 1.08 / ZUGFeRD 2.4 | announced 4 Dec 2025, applicable 15 Jan 2026 | based on UN/CEFACT CII **D22B**, "fully backward compatible" with D16B | [PRIMARY: F-FNFE press release and page] |
| Factur-X 1.09 / ZUGFeRD 2.5 | 10 Jun 2026 | updated code lists and validation artefacts, new EXTENDED elements | [PRIMARY: F-FNFE press release] |
| Factur-X 1.09.2 / ZUGFeRD 2.5.2 | 4 Aug 2026 | corrections and additions; FR and DE versions technically identical | [SECONDARY: F-SEC] |

The `fx:Version` XMP value stays `1.0`: it is the version of the XMP schema, not of the release [SECONDARY: F-AKR].

### 4.2 Profiles and guideline identifiers

Guideline IDs go in `rsm:ExchangedDocumentContext/ram:GuidelineSpecifiedDocumentContextParameter/ram:ID`, with the XMP `fx:ConformanceLevel` value for each [SECONDARY: F-AKR comments and level map; Gotenberg docs list the same XMP values]:

| Profile | Guideline ID | `fx:ConformanceLevel` |
|---|---|---|
| MINIMUM | `urn:factur-x.eu:1p0:minimum` | `MINIMUM` |
| BASIC WL | `urn:factur-x.eu:1p0:basicwl` | `BASIC WL` |
| BASIC | `urn:cen.eu:en16931:2017#compliant#urn:factur-x.eu:1p0:basic` | `BASIC` |
| EN 16931 (COMFORT) | `urn:cen.eu:en16931:2017` | `EN 16931` |
| EXTENDED | `urn:cen.eu:en16931:2017#conformant#urn:factur-x.eu:1p0:extended` | `EXTENDED` |
| XRECHNUNG (ZUGFeRD) | the XRechnung CIUS ID | `XRECHNUNG` |

The French CTC extended profile `urn:cen.eu:en16931:2017#conformant#urn.cpro.gouv.fr:1p0:extended-ctc-fr` is also listed in akretion's comments [SECONDARY]. MINIMUM and BASIC WL do not meet EN 16931 and are not invoices for German VAT purposes [SECONDARY].

### 4.3 PDF packaging

[SECONDARY: F-AKR, the reference implementation. The Factur-X specification text itself was not read.]

- **File name:** `factur-x.xml` (ZUGFeRD 2.x uses the same name for its EN 16931 profiles).
- **File specification:** `/Type /Filespec /F (factur-x.xml) /UF (factur-x.xml) /EF << /F … /UF … >> /Desc (…) /AFRelationship /…`. Allowed values for the XML are `Data`, `Source` and `Alternative`. MINIMUM and BASIC WL must use `Data`. Alternative is the usual value for EN 16931 and the akretion default when lines are carried (`en16931-formats` also defaults to Alternative) [SECONDARY].
- **Embedded file stream:** `/Type /EmbeddedFile /Subtype /text#2Fxml` (MIME type), `/Params << /ModDate … /Size … /CheckSum <md5> >>`.
- **Catalog:** `/AF [filespec]` and `/Names << /EmbeddedFiles << /Names [(factur-x.xml) filespec] >> >>`. akretion also sets `/PageMode /UseAttachments`.
- **XMP:** `pdfaid:part` `3` and `pdfaid:conformance` `B` (or U/A). The Factur-X properties in namespace `urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#`, prefix `fx`: `fx:DocumentType` `INVOICE`, `fx:DocumentFileName` `factur-x.xml`, `fx:Version` `1.0`, `fx:ConformanceLevel` (table above). Plus the **PDF/A extension schema description** (`pdfaExtension:schemas` → `pdfaSchema:schema` "Factur-X PDFA Extension Schema", `namespaceURI`, `prefix` `fx`, and one `pdfaProperty` per property: name, `valueType` Text, `category` external, description). Without it the fx properties are not valid PDF/A metadata. Note the mixed case `CrossIndustryDocument` and the trailing `#` (PDFlib's note in `Factur-X_extension_schema.xmp`).
- **PDF/A-3** (ISO 19005-3) requirements that affect a hand-written PDF [SECONDARY: general PDF/A knowledge, ISO text not read]:
  - all fonts embedded, with widths matching the font program;
  - a PDF/A OutputIntent (`/S /GTS_PDFA1` with an ICC DestOutputProfile) when device colour is used;
  - a trailer `/ID`, no encryption, a correct xref;
  - XMP metadata consistent with any Info dictionary;
  - embedded files allowed only with a MIME `/Subtype`, `/AFRelationship`, and the file specification in an `/AF` array.
  
  PDF/A-2 forbids non-PDF/A attachments, so a Factur-X file must be PDF/A-3.
- **What veraPDF checks:** the PDF/A-3 profile (fonts, colour, metadata, embedded-file keys). It does not check the invoice XML. The Factur-X / ZUGFeRD XSD and Schematron (FNFE-MPE / FeRD packages, or the Mustang validator) check the XML and its match with the XMP [SECONDARY].

## 5. What KhataOS builds from this

- `khata-en16931::pint` writes PINT SG and PINT AE UBL from the `Invoice` model, with jurisdiction category codes. It checks the subset of shared and jurisdiction rules that the books can break. Golden-snippet tests compare against the official examples' element order.
- The desktop maps vouchers to these documents. The seller comes from the company, the buyer from the party ledger. The participant ID is `0195:SGUEN<UEN>` or `0235:<TIN>`. The UUID is the voucher's UUID, so it stays stable.
- Transmission is left to a commercial Access Point / ASP. KhataOS can post the file to a configured endpoint with an API key kept outside the books. **Each AP's API is proprietary and different.** The generic client is a starting point, not a claim of compatibility with any provider.
- Factur-X: a text-layout PDF/A-3b with the EN 16931 CII embedded as `factur-x.xml`, written without a PDF library. Its conformance gaps are listed in `docs/peppol.md`.

## 6. Open items

1. **Run the official Schematron.** The PINT SG / AE Schematron and the Factur-X / ZUGFeRD Schematron need an XSLT 2.0 processor (Saxon). None is on this machine, and xsltproc is XSLT 1.0. Validate real output with the Peppol testbed, the AP's validator, or Saxon + the `resources.zip` XSLT before going live. XSD checks with xmllint against the UBL 2.1 schema in the PINT SG zip are possible locally.
2. **Run veraPDF and a Factur-X validator** (Mustang, FNFE-MPE / FeRD packages) on generated PDFs. Neither was available here.
3. **PINT SG foreign currency:** TaxCurrencyCode SGD, the second TaxTotal and the `sgdtotal-*` references need an SGD exchange rate per invoice. Not modelled.
4. **PINT SG category reading:** whether SRCA-S, NA, ES33, ESN33 and OS carry `Percent` 0 or nothing (the BIS table says "N/A" while BR-48 asks for a rate except NG).
5. **PINT AE reverse charge** needs the goods type (BTAE-09) and a GTIN (scheme 0160) per line. Free zone, agent billing, margin scheme (`N`, Greek Nu oddity), summary invoices, e-commerce and exports with deliver-to addresses need data the books do not hold.
6. **PINT AE legal registration** (trade licence number, type and issuing authority) and buyer emirates must be captured on the company and party masters.
7. **UAE amending decision:** get the text (MD 56/2026 or MD 66/2026) from mof.gov.ae and confirm the < AED 50m dates.
8. **Document type identifier** for PINT in the SMP (`busdox-docid-qns` vs the wildcard scheme): confirm with the Peppol Policy for use of identifiers. APs usually set it.
9. **Factur-X 1.08+ XSD (D22B):** confirm that a D16B EN 16931 CII passes the current Factur-X validator, as the backward-compatibility statement suggests.
10. **AP/ASP APIs:** each provider (IMDA-accredited APs, UAE ASPs) has its own REST API, authentication and status callbacks. One adapter per provider is needed for real use.

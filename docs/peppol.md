# Peppol PINT (Singapore, UAE), Factur-X and Access Points

KhataOS writes three e-invoice formats from a saved Sales or Credit Note
voucher, all on the EN 16931 model of `crates/khata-en16931`:

| Format | Who | Module | Command |
|---|---|---|---|
| PINT SG Billing (UBL 2.1) for Singapore's InvoiceNow | companies in SG | `khata_en16931::pint` | `pint_sg_xml(voucher_id)` |
| PINT AE Billing (UBL 2.1) for the UAE Electronic Invoicing System | companies in AE | `khata_en16931::pint` | `pint_ae_xml(voucher_id)` |
| Factur-X / ZUGFeRD PDF (EN 16931 profile) | companies in Germany, France, the Netherlands and Spain | `khata_en16931::facturx` | `facturx_pdf(voucher_id)` |

Plus `peppol_participant_id()` (the company's Peppol id; no screen uses it
yet) and a generic
Access Point client (`peppol_ap_settings`, `set_peppol_ap_settings`,
`peppol_ap_send`). The desktop side is `apps/desktop/src-tauri/src/peppol.rs`;
the export actions are in `apps/desktop/src/screens/peppol.ts`.

The research behind it, with sources and confidence, is
[`docs/research/einvoicing-peppol.md`](research/einvoicing-peppol.md).
**Nothing here has been run through the official Schematron or a PDF/A
validator** (see "What to verify").

## Identifiers

| | Singapore | UAE |
|---|---|---|
| CustomizationID (BT-24) | `urn:peppol:pint:billing-1@sg-1` | `urn:peppol:pint:billing-1@ae-1` |
| ProfileID (BT-23) | `urn:peppol:bis:billing` | `urn:peppol:bis:billing` |
| Tax scheme | `GST` | `VAT` |
| Participant id | `0195:SGUEN<UEN>` | `0235:<TIN>` (first 10 digits of the TRN) |
| UUID | `cbc:UUID` (BT-SG-003) | `cbc:UUID` (BTAE-07) |
| Extra | — | `cbc:ProfileExecutionID` (BTAE-02, 8 flags) |

The UUID is the voucher's own id, so re-exporting a voucher gives the same
UUID. Peppol document type id (for Access Points):
`urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##urn:peppol:pint:billing-1@sg-1::2.1`
(CreditNote likewise) [UNCERTAIN: check the receiver's SMP entry].

## Tax categories from the books

**Singapore** (`sg_line`): the line's tax class and the rate on file
(`khata_sg::gst::treatment`): standard → `SR`, zero-rated → `ZR`, exempt →
`ESN33`, out of scope → `OS`. A class named after a GST category code
(`ES33`, `DS`, `SRRC`, `NA`, …) is that code. A company with no GST
registration number issues `NG` lines (no GST, no rate). A sale with no
GST to a party abroad is `ZR`. Rates are written for every category except
NG (BR-48-GST-SG): 9% (or the rate on file) for SR, 0 for the others.

**UAE** (`ae_line`): `khata_ae::vat::mechanism` and `treatment`: standard →
`S` (must be 5%, ibr-190-ae), zero-rated → `Z`, `EXEMPT…` → `E` with the
exemption code named in the class (`EXEMPT DL8.46.2`; ibr-167-ae requires
one of DL8.46.1–4), `OUT-OF-SCOPE…` → `O`, `RC:…` → `AE`. A sale with no
VAT to a party abroad is `Z` and an export (`ProfileExecutionID`
`00000001`), delivered to the buyer's address (ibr-152-ae). Exempt and
out-of-scope lines carry no rate (ibr-119/121-ae, aligned-ibrp-e-05).

**EU / Factur-X** (`eu_category`): `EXEMPT…` → E (reason "Exempt from
VAT"), `REVERSE…`/`RC…` → AE, 0% → Z, else S. A sale with no VAT to a party
abroad: EU business with a VAT id → K (goods) / AE (services); outside the
EU → G (goods) / O (services; note BR-O-02 forbids the seller's VAT id with
O, so such an invoice reports an error to fix by hand).

A discount (a ledger on the party's side) becomes a document allowance in
the category of the lines; a voucher whose lines have several categories
and a discount is refused.

## Where the data comes from

Until the company profile and ledger masters hold these fields,
`peppol.rs` reads "Key: value" lines:

| Where | Lines |
|---|---|
| Company address | `UEN:`, `Peppol ID: scheme:value`, `Trade licence:`, `Licensing authority:`, `Emirates ID:`, `Tel:`, `Email:`, `Contact:`, `IBAN:`, `BIC:`, `Bank account:`, `PayNow:` |
| Party mailing address | `Peppol ID:`, `UEN:`, `Trade licence:`, `Licensing authority:` |
| Voucher narration | `Buyer reference:` / `Your reference:`, `PO:` / `Order:`, `Payment terms:`, `Reason:` (UAE credit note code); other lines are the invoice note |

Other sources: seller name = company name; GST no / TRN / VAT id = the
company tax registration; UAE seller emirate = the company's tax region;
buyer = party ledger (name, tax id, mailing address, post code, region,
country, e-mail); due date = the bill's due date or the party's credit days
(else the invoice date, with a warning); preceding invoice of a credit note
= its "Against" bill reference.

Singapore addresses: the 6-digit postal code is found in a line such as
"Singapore 048616" and the city is Singapore. UAE: the last plain line is
the city; the buyer's emirate is the mailing region (or guessed from the
city). EU: a "PLZ City" line ("10115 Berlin").

UAE buyers: TRN → `0235:<TIN>`; abroad without a Peppol id →
`0235:9900000099`; in the UAE without TRN or Peppol id →
`0235:9900000098` with a warning to issue a paper/PDF tax invoice too.
Credit notes default to reason `DL8.61.1.D` (goods returned) with a warning.

## Rules checked (`PintInvoice::validate`)

Shared PINT: ibr-002, 006, 007, 009, 011, 016, 021, 023, 025, 027, 029,
033, 038, 049, 062, 063, 080, 081, ibr-co-10 to 16, 19, 26, ibr-091,
121–125, ibr-cl-04, ibr-sr-58; breakdown sums and tax (KhataOS ids
`KHATA-PINT-*`, BR-CO-17-GST-SG / aligned-ibrp-s/z-09). By construction:
one Note (ibr-sr-51), CustomizationID/ProfileID (R004-SG, R007-SG,
aligned-ibrp-001/002-ae), the 8-flag code (ibr-154-ae), item description,
gross price and base quantity, line amounts (ibr-104/125/126/194-ae).

Singapore: PEPPOL-EN16931-R003-SG, BR-CO-25-SG, BR-48, BR-105 to 109,
BR-111 to 113, BR-NG-01/02/11/12/13 (`-GST-SG`).

UAE: ibr-001, 010, 012, 055, 101, 103, 119, 124, 127, 128, 132, 134, 135,
141, 143, 144, 149, 150, 151, 152, 158, 159, 166, 167, 172, 180, 181,
190, 191, 192, 193 (`-ae`), aligned-ibrp-s/z/e/o/ae-05/06 and s-10;
unsupported transaction types (free zone, margin scheme, summary without
period, disclosed agent, e-commerce) are refused.

Factur-X: EN 16931 core rules (`rules::validate_for(…, En16931)`).

## Not supported

- Foreign-currency invoices (SG: GST in SGD, BT-SG-001/002; UAE: exchange
  rate BTAE-04 and AED totals).
- UAE reverse-charge supplies (need the goods type BTAE-09 and a GTIN),
  margin scheme (category N), free zone, deemed supplies, summary,
  continuous supply, agent and e-commerce flags, item classification (HS /
  service accounting codes), out-of-scope invoice types 480/81,
  self-billing, the Tax Data Document (the ASP files it).
- SG: customer accounting (SRCA-*), GIRO (Z01), the B2G variant.
- Line allowances and charges; a discount across several tax categories.
- Factur-X profiles other than EN 16931 (MINIMUM, BASIC, EXTENDED,
  XRECHNUNG); national CIUS rules (XRechnung for German public buyers is a
  separate export, see `docs/germany-vat.md`).

## Factur-X PDF

`facturx::pdf` writes a PDF/A-3b file by hand (no PDF crate): a monospaced
text rendering of the invoice (Courier Prime, SIL OFL 1.1, embedded in
full; the licence is `crates/khata-en16931/assets/CourierPrime-OFL.txt`),
an sRGB output intent (the CC0 `sRGB-v2-micro.icc` of
saucecontrol/Compact-ICC-Profiles), XMP with `pdfaid:part 3` /
`conformance B`, the Factur-X `fx:` properties (`DocumentType INVOICE`,
`DocumentFileName factur-x.xml`, `Version 1.0`, `ConformanceLevel EN 16931`)
and their PDF/A extension schema, and the CII XML (guideline
`urn:cen.eu:en16931:2017`) embedded as `factur-x.xml` (`/Subtype
/text#2Fxml`, `/Params /ModDate`, `/AFRelationship /Alternative`, listed in
the catalog `/AF` and `/EmbeddedFiles`). No Info dictionary, so it cannot
disagree with the XMP.

**Conformance gaps** (also returned to the UI as `conformance_gaps`):
- not validated with veraPDF or a Factur-X/ZUGFeRD validator (Mustang,
  FNFE-MPE/FeRD Schematron) — neither is installed here; `qpdf --check`,
  `pdfinfo`, `pdffonts`, `pdfdetach` and `pdftotext` read the sample file
  without complaint;
- level B only (no tagged PDF, no ToUnicode map);
- only Windows-1252 characters can be shown, others print as `?` (the XML
  keeps them);
- the page is plain text, not the KhataOS invoice layout;
- the XML is CII D16B; Factur-X 1.08+ / ZUGFeRD 2.4+ publish D22B schemas
  said to be backward compatible [UNCERTAIN];
- the whole font is embedded (about 40 KB per PDF);
- profile and AFRelationship choices for the French 2026 reform and German
  B2B rules were checked only against the Factur-X reference
  implementation.

`write_binary_file(path, base64)` saves the PDF; it accepts only `.pdf`
names and bytes starting with `%PDF-`.

## Access Points (transmission)

KhataOS is corner 1 software. Sending goes through the company's Access
Point (Singapore: an IMDA-accredited InvoiceNow Access Point / InvoiceNow-
Ready Solution; UAE: an Accredited Service Provider, which also reports to
the FTA). **Every provider's API is different**: sign-in, URL, payload
wrapping, status callbacks. KhataOS ships one generic adapter and claims
compatibility with no provider:

- `set_peppol_ap_settings({ endpoint, api_key, auth_header, auth_prefix })`
  stores the URL (https only; http only to localhost for tests) and the key
  in `<app data>/peppol/access-point.json`, owner-only (`0600`, folder
  `0700`), never in the company file or its backups. `peppol_ap_settings`
  returns the key only as "set or not".
- `peppol_ap_send(voucher_id, "pint-sg" | "pint-ae")` builds the document,
  refuses it if it breaks PINT rules, and POSTs the UBL with
  `Content-Type: application/xml`, the key in the configured header
  (default `Authorization: Bearer …`) and informative
  `X-Peppol-Sender-ID`, `X-Peppol-Receiver-ID`, `X-Peppol-Document-Type-ID`,
  `X-Peppol-Process-ID` headers. It reports the HTTP status and the start of
  the answer; a 2xx means only that the Access Point took the document.
- Tested against a local mock server (`peppol::tests`), not a real provider.

A provider needing another request shape needs its own adapter (one module
per provider, like `irp_client.rs` for India). Status tracking, inbound
invoices and the UAE 14-day deadline queue are not built.

## Where it is in the app

`apps/desktop/src/screens/peppol.ts` exports `exportPintSg(shell, v)`,
`exportPintAe(shell, v)`, `exportFacturX(shell, v)` (save dialog, then
`api.writeGstJson` or `peppolApi.writeBinaryFile`), and
`sendViaAccessPoint(shell, v, "pint-sg" | "pint-ae")` /
`setUpAccessPoint(shell)`. The voucher export key (Ctrl+E or Alt+E on a
saved Sales invoice or Credit Note, `screens/einvoice.ts` `exportVoucher`)
offers them:

- Singapore: **InvoiceNow e-invoice (PINT SG)**, **Send via Access Point**,
  **Access Point settings**;
- UAE: **UAE e-invoice (PINT AE)**, **Send via Access Point**,
  **Access Point settings**;
- Germany: **XRechnung (UBL or CII)** and **Factur-X / ZUGFeRD PDF**;
  France and the Netherlands: **Factur-X / ZUGFeRD PDF**; Spain:
  **VeriFactu record** and **Factur-X / ZUGFeRD PDF**.

The Access Point settings have no other menu row: they open from the
Export menu, or on the first send.

## What to verify

- Run the official PINT SG 1.4.x / PINT AE 1.0.4 Schematron (XSLT 2.0, e.g.
  Saxon-HE) on real output; the samples in `pint::tests`
  (`PINT_SAMPLE_DIR=… cargo test -p khata-en16931 pint`) validate against the
  UBL 2.1 XSDs shipped with PINT SG.
- Run veraPDF (PDF/A-3B) and a Factur-X validator on `facturx` output
  (`FACTURX_SAMPLE=/path.pdf cargo test -p khata-en16931 facturx`).
- Confirm with IMDA / your Access Point the SG categories for customer
  accounting and NA, and whether 0% must be written for ES33/ESN33/OS.
- Confirm the UAE ASP's expectations (some take their own JSON, not UBL).

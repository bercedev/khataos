# Saudi Arabia pack and ZATCA e-invoicing (`khata-sa`)

KhataOS can keep books for a company in the Kingdom of Saudi Arabia: it
checks VAT numbers, charges VAT on invoices, checks invoice numbers, works
out VAT return figures, builds ZATCA e-invoice XML with its hash, counter,
hash chain and QR code, and prints invoices in Arabic and English.

The VAT return figures are a preparation aid, not a filing. Phase 2
(integration) is implemented: onboarding a computer as a ZATCA device (CSR,
compliance CSID, compliance checks, production CSID), the XAdES
cryptographic stamp, clearance of standard invoices and reporting of
simplified ones (see [Phase 2](#phase-2-onboarding-the-stamp-clearance-and-reporting)).
It has passed ZATCA's **developer-portal sandbox** end to end; it has **not**
been run against the simulation or production environments, which need a
real taxpayer's OTP (see [What was tested](#what-was-tested-against-what)).
None of this is legal advice: check it with a Saudi chartered accountant
before use.

Code: `crates/khata-sa` (`vat_number.rs`, `regions.rs`, `vat.rs`,
`returns.rs`, `zatca.rs`, `qr.rs`, `sign.rs`, `csr.rs`, `xades.rs`),
`apps/desktop/src-tauri/src/zatca.rs` (vouchers to e-invoices and returns,
the device chain, the seller details, onboarding and submission),
`apps/desktop/src-tauri/src/zatca_api.rs` (ZATCA's HTTP APIs),
`apps/desktop/src/screens/zatca.ts` (onboarding and submission screens) and
`crates/khata-print` (`invoice_html_bilingual`). Sources and open questions are in
`docs/research/mena-compliance.md` §5 (VAT), §6 (ZATCA), §8 (pack design)
and §9 (open items). Tags come from that file: **[PRIMARY]** confirmed
against the official text, **[SECONDARY]** reported by professional
sources, **[UNCERTAIN]** not confirmed.

## Setting up a Saudi company

1. Pick **Saudi Arabia** as the country. The books use SAR (2 decimals,
   printed "SAR") and Gregorian calendar years. Hijri dates are not used in
   the books; ZATCA e-invoices are Gregorian too (research §6.2). A Zakat
   year may be Hijri, which changes only the Zakat rate's day count
   (research §7); Zakat is not modelled.
2. In **Company → Company details** (Alt+K), enter the company's **VAT number**
   and pick its **region**. The region does not change VAT; it is printed
   on e-invoices (`CountrySubentity`).
3. Add the rate list. Suggested classes (also in `khata_sa::vat::default_rates`):

   | Class | Rate | From | Meaning |
   |---|---|---|---|
   | `STANDARD` | 5 | 2018-01-01 | Standard rate when VAT started |
   | `STANDARD` | 15 | 2020-07-01 | Standard rate from 1 Jul 2020 (Royal Decree A/638) [SECONDARY] |
   | `ZERO` | 0 | 2018-01-01 | Zero-rated supplies |
   | `EXEMPT` | 0 | 2018-01-01 | Exempt supplies |
   | `OUT OF SCOPE` | 0 | 2018-01-01 | Not subject to VAT (ZATCA category O) |

   A class whose name starts with `EXEMPT` is exempt, `OUT OF SCOPE`
   out of scope, any other 0% class zero-rated. Add the **VATEX code** to a
   zero-rated, exempt or out-of-scope class name (`ZERO VATEX-SA-35`,
   `EXEMPT VATEX-SA-29`): e-invoices then carry the exemption reason
   (BR-KSA-69) [PRIMARY]. Known codes: VATEX-SA-29, -29-7, -30 (exempt);
   -32, -33, -34-1 to -34-5, -35, -36, -EDU, -HEA, -MLTRY (zero-rated);
   -OOS (out of scope). The reason texts are KhataOS's short English
   wording; ZATCA's code list has the official English and Arabic texts
   [UNCERTAIN: replace before production].
4. Create **Output VAT** and **Input VAT** ledgers under Duties & Taxes
   with type of tax `VAT`.
5. Enter the **ZATCA seller details** (Alt+K → **Seller details (ZATCA)**;
   commands `zatca_seller` / `set_zatca_seller`):
   CRN, and the Saudi National Address (4-digit building number, street,
   district, city, postal code, additional number). These are a shared
   record of the company (synced; `docs/sync-settings.md`), not in the
   books, until the core company profile holds them. The device's EGS unit
   (CSID, invoice chain, queue) stays on each computer. Blank street, city and postal code are taken from
   the company address (first line, last line, a 5-digit number).

## What the pack checks

| Rule | What KhataOS does | Source |
|---|---|---|
| VAT number | 15 digits, first and last digit `3`, spaces and hyphens dropped. Digit 11 = `1` marks a VAT group (`is_vat_group`). **No checksum**: none is published | BR-KSA-40, -44 [PRIMARY]; checksum [UNCERTAIN] |
| Regions | The 13 regions with **ISO 3166-2:SA codes** (`SA-01` Riyadh … `SA-12` Al-Jawf, `SA-14` Asir; ISO has no SA-13). The research gives no codes, so these are KhataOS's choice from ISO, not checked against the standard text here | [UNCERTAIN] |
| VAT rate | One `VAT` component at the class's rate on the invoice date; exports (party abroad) 0%. Domestic supplies can't skip VAT | §5 [SECONDARY] |
| Rounding | Half up to the halala; e-invoice VAT is rounded once per category | XML standard §10 [PRIMARY] |
| Invoice numbers | Unique across all years, at most 35 characters of letters, digits, `-` and `/`. Numbers that give the same ZATCA file name (`A/1`, `A-1`) clash. The research only asks for a unique sequential number; 35 and the character set are KhataOS choices (as in the UAE pack) | IR Art 53 [SECONDARY]; XML standard §14 [PRIMARY] |

## ZATCA e-invoices

`zatca_invoice_xml(voucher_id, kind)` (`kind` = `standard` for a B2B tax
invoice, `simplified` for a B2C simplified tax invoice) returns
`{ xml, uuid, icv, hash, pih, qr, file_name, regenerated, warnings }`.
Sales vouchers become invoices (`388`), Credit Notes credit notes (`381`)
and Debit Notes to customers debit notes (`383`); a Debit Note to a
supplier is a purchase return and has no e-invoice. A note must be linked
to its invoice (bill-wise "Agst Ref"), and its narration is the reason.

### What is generated

- **UBL 2.1 `Invoice`** in schema element order [PRIMARY: XML standard
  v1.2, data dictionary 19 May 2023]: `ProfileID` `reporting:1.0`, number,
  **UUID** (v4, KSA-1), issue date and **time** (KSA-25, Riyadh time,
  fixed at first generation), `InvoiceTypeCode` with the **NNPNESB**
  subtype (`0100000` standard, `0200000` simplified, export flag on
  standard invoices to customers abroad), SAR document and tax currency,
  billing reference on notes, **ICV**, **PIH** and **QR** references,
  seller (CRN, National Address, VAT number), buyer (required on standard
  invoices; empty on simplified ones without a customer), supply date
  (KSA-5), payment means (`10` cash or bank party, else `30`), discounts as
  document allowances, **two `TaxTotal`s** (with the VAT breakdown, and the
  SAR total alone), monetary totals, and lines with categories `S`, `Z`,
  `E`, `O`, line VAT and `RoundingAmount`.
- **Invoice hash**: SHA-256 over the XML with `ext:UBLExtensions`, the QR
  reference and `cac:Signature` removed, Base64 (BR-KSA-26) [PRIMARY].
  ZATCA asks for **C14N 1.1**. KhataOS writes its XML already in canonical
  form (attributes sorted, start/end tag pairs, C14N escaping, LF line
  ends) and cuts the three elements out keeping the whitespace around
  them, as an XSLT identity transform would; it is not a general C14N
  implementation and refuses XML it did not write. **ZATCA's sandbox
  validator recomputed the same hash** for all six document types (it
  answers `invalid-invoice-hash` when the hash differs, which we provoked
  by editing a signed document) [tested live, 20 Oct 2026]. The stamp's
  two blocks are written with no whitespace around them (as in ZATCA's SDK
  samples), so a document has **the same hash stamped or unstamped**.
- **Chain** (per device, research §6.2 [PRIMARY]): the ICV counts up by one
  for every generated document of any kind and is never reset; each PIH is
  the previous document's hash; the first is ZATCA's published value
  (Base64 of SHA-256 of "0", as hex text). The desktop keeps the chain in
  the company file's settings (`zatca.chain`, outside the audit trail, not
  synced), writes it **before** recording the document, and records every
  document (`zatca.doc.<voucher id>`, with its XML). Asking again returns
  the recorded XML unchanged (with a warning if the voucher changed since:
  correct it with a credit or debit note). A refused build uses no counter.
- **QR** (Base64 TLV, at most 700 characters) [PRIMARY: Security Standard
  §4]: tags 1 to 5 (seller name, VAT number, timestamp, total with VAT, VAT
  total), lengths in UTF-8 bytes. A stamped document adds tag 6 (the
  invoice hash as its **Base64 text**, 44 bytes), 7 (the signature value's
  **Base64 text**), 8 (the public key, DER SubjectPublicKeyInfo, 88 bytes)
  and, on simplified documents only, 9 (the CSID certificate's signature
  by ZATCA's CA, DER). The standard's text says tag 6 is "32 bytes"; ZATCA's
  SDK sample invoices carry the Base64 text, and **ZATCA's sandbox refuses
  the raw-byte form** (`invoiceHash_QRCODE_INVALID`,
  `INVOICE_SIGNATURE_VALUE_QRCODE_INVALID`) while accepting the text form
  [tested live, 20 Oct 2026].
- **Stamp**: ECDSA on **secp256k1** (`k256`), SHA-256, deterministic (RFC
  6979), over the 32 hash bytes (standard §4: "ECDSA signature of the XML
  Hash") inside a XAdES B-B `ds:Signature` (see
  [The stamp](#the-xades-stamp)). Documents are stamped once the computer
  is onboarded; before that they carry tags 1 to 5 only, and are stamped
  when submitted.
- **Print**: vouchers of KSA (and UAE) companies print bilingually.
  Once its e-invoice exists, a KSA invoice prints "Tax Invoice" or
  "Simplified Tax Invoice" with its QR code as an inline SVG.
- **File name**: `<VAT>_<yyyymmdd>T<hhmmss>_<number>.xml` [PRIMARY: §14].

[UNCERTAIN] Credit notes use the `Invoice` root with type code 381, as the
research reads; simplified invoices without a customer carry an empty
`AccountingCustomerParty` (UBL requires the element); unit codes are all
`PCE`; round-off ledgers are left out of the e-invoice (with a warning).
The XML's VAT is worked out from the rate list; a warning says when it
differs from the VAT booked.

### Phases and waves

| What | Applies | Source |
|---|---|---|
| **Phase 1 (generation)**: every resident VAT-registered taxpayer generates and stores invoices with a compliant solution; QR tags 1–5 on simplified invoices; no deleting or editing issued invoices | from **4 Dec 2021** | ZATCA roll-out page [PRIMARY]; controls annex [SECONDARY] |
| **Phase 2 (integration)**: UBL XML, UUID, ICV, PIH, cryptographic stamp, QR tags 1–9; **clearance** of standard invoices before they reach the buyer; **reporting** of simplified invoices within 24 hours | from 1 Jan 2023, **by wave** | XML standard v1.2, Security Standard v1.2, Detailed Guideline v2 [PRIMARY] |
| Wave 23: revenue > SAR 750,000, by 31 Mar 2026; Wave 24: > SAR 375,000, by 30 Jun 2026 | as stated | [SECONDARY] |
| **Wave 25**: VAT-subject revenue > **SAR 187,500** in any of 2022–2025, integrate by **1 Feb 2027** (announced 24 Jul 2026). Almost every VAT-registered SME | as stated | ZATCA news [PRIMARY] |

The company's own integration date comes from its ZATCA notification letter.

## Phase 2: onboarding, the stamp, clearance and reporting

### Environments

| Environment | Base URL | OTP | Certificate template (CSR) | Source |
|---|---|---|---|---|
| Developer portal (Integration Sandbox) | `https://gw-fatoora.zatca.gov.sa/e-invoicing/developer-portal` | the test OTP **123345**, open to anyone | `TSTZATCA-Code-Signing` (the sandbox also accepts `ZATCA-Code-Signing`) | ZATCA's API specifications (the `servers` entry) [PRIMARY]; OTP and templates tested live |
| Simulation | `https://gw-fatoora.zatca.gov.sa/e-invoicing/simulation` | from the Fatoora **simulation** portal | `PREZATCA-Code-Signing` [SECONDARY: the SDK's `-sim` CSR; UNCERTAIN until onboarded with a real OTP] | ZATCA developer forum and integrators [SECONDARY]; the server answers our CSRs with an OTP check (tested live) |
| Production ("core") | `https://gw-fatoora.zatca.gov.sa/e-invoicing/core` | from the Fatoora portal | `ZATCA-Code-Signing` | Fatoora Portal User Manual [PRIMARY]; sandbox API sample CSR [PRIMARY]; **not called** |

The developer portal is a test service with mock data: its production CSID
is a fixed sample certificate for another key (KhataOS then stamps with the
compliance certificate, which is for the device's key, and says so), and
reporting there answers a `certificate-issuer-name` warning. Documents sent
to the sandbox or simulation have no legal effect.

### Onboarding steps (Alt+K → ZATCA onboarding)

The Company menu (Alt+K) of a Saudi company has **Seller details (ZATCA)**,
**ZATCA onboarding** and **ZATCA queue**. A saved Sales invoice, Credit
Note or Debit Note has **Submit to ZATCA** and **ZATCA e-invoice XML** on
its Export menu (Ctrl+E).


1. Fill in the company's VAT number and the ZATCA seller details (CRN,
   National Address) first; onboarding refuses a company without a valid
   VAT number.
2. Log in to the Fatoora portal (production) or the Fatoora simulation
   portal, choose *Onboard new solution unit/device* and generate an
   **OTP** (valid for one hour). For the developer portal use 123345.
3. In KhataOS, pick the environment, type the OTP and, if needed, the
   branch (for a VAT group: the member's 10-digit TIN), location (the
   National Address short address), industry and invoice types (`1100`
   standard and simplified, `1000`, `0100`). Accept.
4. KhataOS then, in one go (`zatca_onboard`):
   1. gives this computer a **device id** (UUID) and makes a secp256k1 key;
   2. builds the **CSR** (`khata_sa::csr`): subject C=SA, OU=branch,
      O=company name, CN=`KhataOS-<first 8 of the device id>`; certificate
      template extension (OID 1.3.6.1.4.1.311.20.2) by environment; SAN
      directory name with SN=`1-KhataOS|2-<app version>|3-<device id>`,
      UID=VAT number, title=invoice types, registeredAddress, businessCategory;
   3. saves the key **before** sending anything, then asks for the
      **compliance CSID** (`POST /compliance`, header `OTP`, body
      `{"csr": Base64 of the CSR PEM}`);
   4. runs the **compliance checks**: one stamped sample of each document
      type the device issues (standard and simplified invoice, credit note
      and debit note: six for `1100`), chained among themselves from the
      first PIH, sent to `POST /compliance/invoices` with the compliance CSID.
      The device's own invoice counter and chain are not touched;
   5. asks for the **production CSID** (`POST /production/csids` with
      `compliance_request_id`).
5. If a step fails (wrong OTP, a failed check, no network), the screen
   shows ZATCA's message; Accept again with an empty OTP resumes from the
   failed step. *Start again* (Ctrl+Alt+R) makes a new device and key.

CSID **renewal** (`PATCH /production/csids` with a new OTP) is Ctrl+Alt+N
on the onboarding screen; see the renewal section below. Revocation is not
implemented. The screen shows the certificate's expiry.

### Where the secrets are

- The private key (SEC1 PEM), and the compliance and production CSIDs
  (`binarySecurityToken`, `secret`, request id) are in
  `<app data folder>/zatca/<device id>.json` (macOS
  `~/Library/Application Support/<app id>/zatca/`, Windows
  `%APPDATA%\<app id>\zatca\`, Linux `~/.local/share/<app id>/zatca/`).
  The screen prints the exact path. On Unix the folder is 0700 and the file
  0600 (created with that mode, written through a temporary file and
  renamed). **Windows**: no explicit ACL is set; the file inherits the
  per-user `%APPDATA%` folder's ACL (the user, Administrators and SYSTEM).
- They are **never** in the company file, its backups, the audit trail or
  sync, never logged, and never sent to the screen (the status says only
  whether they exist). A copied or restored company is a new device and
  must be onboarded again on that computer.
- The company file keeps only non-secret device state in its local
  settings: `zatca.device` (device id, environment, CSR, check results,
  certificate expiry), `zatca.chain` (ICV and PIH, as before), and
  `zatca.submission.<voucher id>` (ZATCA's answer).

### The XAdES stamp

`khata_sa::xades` writes `ext:UBLExtensions/ext:UBLExtension/
ext:ExtensionContent/sig:UBLDocumentSignatures/sac:SignatureInformation/
ds:Signature` straight after the `Invoice` start tag, and `cac:Signature`
(`urn:oasis:names:specification:ubl:signature:Invoice`, method
`urn:oasis:names:specification:ubl:dsig:enveloped:xades`, BR-KSA-60)
straight after the QR reference, in the layout of ZATCA's SDK sample
invoices (SDK 3.2.x, `Data/Samples/Simplified/Invoice/Simplified_Invoice.xml`):

- `ds:SignedInfo`: C14N 1.1, `ecdsa-sha256`, two references: the invoice
  (`URI=""`, the three XPath transforms excluding `ext:UBLExtensions`,
  `cac:Signature` and the QR reference, then C14N 1.1; SHA-256; digest =
  the invoice hash) and the SignedProperties (`Type=…#SignatureProperties`,
  `URI="#xadesSignedProperties"`) [PRIMARY: Security Standard §2.3.3 and the
  SDK sample].
- `ds:SignatureValue`: Base64 DER ECDSA over the invoice hash bytes (see
  above). `ds:KeyInfo/ds:X509Data/ds:X509Certificate`: the CSID certificate.
- `xades:QualifyingProperties/xades:SignedProperties`: SigningTime (UTC,
  `…Z`), SigningCertificate with CertDigest, X509IssuerName (RFC 4514
  order, ", " separated) and X509SerialNumber (decimal).
- **Digest rules, reproduced exactly against the SDK sample's values**
  (golden test `the_sdk_sample_digests_are_reproduced`): CertDigest =
  Base64 of the hex SHA-256 of the certificate's **Base64 text**;
  SignedProperties digest = Base64 of the hex SHA-256 of the element's text
  with `xmlns:xades` added to it and `xmlns:ds` to each `ds:` element.
- [UNCERTAIN] The standard's table also lists SigningCertificateV2 with
  the whole chain, SignaturePolicyIdentifier and SignedDataObjectProperties;
  the SDK sample has none of them, nor does KhataOS, and the sandbox
  accepts it.
- Stamping happens when a document is generated once the device holds its
  production CSID (`zatca_invoice_xml_stamped`), or at submission for a
  document generated earlier. Because the hash doesn't change, the chain
  holds; a document whose voucher changed after it was generated can't be
  stamped (correct it with a credit or debit note).

### Clearance and reporting (`zatca_submit`)

- Standard documents go to `POST /invoices/clearance/single`
  (`Clearance-Status: 1`); simplified ones to `POST /invoices/reporting/single`
  (`Clearance-Status: 0`). Body `{"invoiceHash", "uuid", "invoice": Base64
  XML}`, HTTP Basic auth with the production CSID (token as user, secret as
  password), `Accept-Version: V2`, `Accept-Language: en` [PRIMARY: ZATCA's
  API specifications].
- Answers kept per voucher: HTTP status, PASS/WARNING/ERROR, the reporting
  or clearance status, info, warning and error messages, and the **cleared
  XML** of a standard document (ZATCA's own stamp and QR: the one to give
  the buyer; the print then uses its QR). 303 (clearance switched off)
  reports the invoice instead; 409/208 (already reported/cleared) count as
  accepted. A document ZATCA accepted is not sent again.
- The kind (standard or simplified) is the one recorded; for a voucher
  never generated, the one asked for, else standard when the buyer has a
  valid KSA VAT number.
- A 401 marks the device's CSID as refused (expired or revoked): the
  onboarding screen says so, and the queue stops until it is renewed. A
  certificate past its expiry date is not sent at all.

### The reporting and clearance queue (`zatca_queue`, `zatca_queue_run`)

Simplified invoices and their notes must be reported **within 24 hours of
generation** (E-Invoicing Implementation Resolution, 19 May 2023, Clause
Second(3)(B); Detailed Guidelines v2 §6.4 [PRIMARY]); standard ones must be
**cleared before they are shared** with the buyer (Second(3)(A)). When
ZATCA or the connection is down, the solution queues the documents, stays
operational and reports them once connected, retrying "in regular
intervals until successful"; past 24 hours, the taxpayer tells ZATCA
through its form and keeps evidence of the attempts (Resolution Annex 2;
Guidelines pp.45-49, which list 429, 500, 503 and 504 as "please resend"
[PRIMARY]).

- **The queue** is `zatca.queue` in the company file's settings (per
  device, like the chain): every generated document joins it and leaves it
  once ZATCA accepts it. Records generated before the queue existed are
  picked up when it is listed. Each record now keeps its generation time
  (UTC) for the deadline (older ones: the voucher date and issue time,
  Riyadh UTC+3).
- **The list** (`ZatcaQueueReport`, "ZATCA Queue"): number, date, party,
  kind, state (waiting, retrying, refused, not generated), the reporting
  deadline (generation + 24 h, hours left, late) for simplified documents
  ("clear before sharing" for standard ones), attempts and the last
  error. E-invoice vouchers dated from the device's onboarding on that were
  never generated are listed as "not generated".
- **Automatic retry**: `zatca_queue_run(all = false)` sends only generated
  documents whose retry time has come. A transient failure (no connection,
  429, 5xx) waits 1, 2, 4… minutes, at most an hour (`retry_delay`, a
  KhataOS choice); a refusal (400) marks the entry "refused" until the user
  corrects the voucher and sends again; a refused CSID (401) stops the run.
  The screen module's `startZatcaQueue(shell)` calls it every 90 seconds
  while the app is online and a Saudi company is open, and when the
  connection comes back.
- **Submit all** (Ctrl+Alt+T, `all = true`): everything at once, refused
  ones included, generating and stamping the e-invoices not generated yet;
  standard invoices are cleared in bulk this way (one call each: ZATCA's
  API takes one document per call).

### CSID expiry, renewal and revocation (`zatca_renew`)

- **Expiry** comes from the stamping certificate itself (its NotAfter;
  the PCSID is valid up to 5 years, Security Features Implementation
  Standards v1.2 [PRIMARY]). The status shows the date and the days left;
  **30 days ahead** (`RENEW_BEFORE_DAYS`) it warns and the next step says
  to renew; once expired, documents are not sent (they wait in the queue).
- **Renewal** (Ctrl+Alt+N on the onboarding screen, with a new OTP in the
  OTP field): a new key and CSR for the same EGS unit (same name and
  serial), sent to `PATCH /production/csids` with the OTP and the current
  production CSID (Developer Portal Manual v3 §2.3.10.4 [PRIMARY]; the verb
  and the credentials are [UNCERTAIN], from ZATCA's developer community and
  SDKs). The new key is saved in the credentials file before the CSR is
  sent; the new CSID and key replace the old ones on success. The invoice
  chain (ICV, PIH) carries on. On the developer portal, whose renewed
  certificate is a sample for another key, the old key is kept and
  documents are stamped with the compliance certificate, as after
  onboarding. The Detailed Technical Guidelines §3.2.2 describe renewal as
  the onboarding flow again (OTP, CSR, compliance checks) and say the old
  CSID is revoked: if ZATCA refuses the renewal call, **Start again**
  (Ctrl+Alt+R) onboards a new device.
- **Revocation** (on the Fatoora portal for a lost device or compromised
  key, or automatic on deregistration or tax-group changes, Guidelines
  §§3.2.3, 3.3.8): ZATCA answers 401; the device is marked refused, the
  queue stops, and the onboarding screen asks to renew or onboard again. A
  revoked CSID cannot be renewed (the mock server refuses it too).
- Tested against the local mock server: renewal with a wrong OTP, a revoked
  CSID, then success (the mock then takes only the renewed secret); expiry
  warnings 60, 10 and −2 days around the certificate's date; the backoff
  schedule; Submit all. Not tested against the live portal.

### What was tested against what

| What | Against | Result |
|---|---|---|
| CSR built by `khata_sa::csr` | OpenSSL (`openssl req -verify -text`) | parses, self-signature verifies, fields as above |
| CSR, template `TSTZATCA-Code-Signing` and `ZATCA-Code-Signing` | **live developer portal**, OTP 123345 | compliance CSID issued, for our key |
| CSRs with each template | **live simulation** | `Invalid-OTP` (a real OTP is needed), so the CSR is not judged; an OpenSSL CSR with extra keyUsage and basicConstraints extensions was refused as `Invalid-CSR` there and as "Invalid Request" by the sandbox, which is why KhataOS sends only the two extensions of ZATCA's sample |
| Stamped standard and simplified invoice, credit note and debit note | **live developer portal** `/compliance/invoices` | all six PASS, no warnings |
| Edited content after stamping; wrong signature value; raw-byte QR tags | live developer portal | refused (`invalid-invoice-hash`; `INVOICE_SIGNATURE_VALUE_QRCODE_INVALID`; QR hash/signature mismatch) |
| A signature by the right key over another document, consistent with the QR; an edited SigningTime | live developer portal | **accepted**: the sandbox does not verify the ECDSA signature or the SignedProperties digest, so those are checked only by KhataOS's own tests (signature verifies with the certificate's key; digests reproduce the SDK sample) |
| Production CSID, clearance (CLEARED, cleared XML returned), reporting (REPORTED) | live developer portal | accepted; reporting warns `certificate-issuer-name` (sandbox sample certificate) |
| The desktop's whole flow (`zatca_onboard`, `zatca_submit`) | live developer portal (`live_sandbox_onboarding_and_submission`, `#[ignore]`, run by hand) | passed 20 Oct 2026 |
| Onboarding errors, secrets file mode and content, nothing secret in the settings, stamping at submission, no double submission, 303 fallback | local mock server (axum) answering in ZATCA's documented shapes | `cargo test -p khataos-desktop zatca` |
| Simulation and production clearance/reporting | — | **not tested** (needs a taxpayer's OTP) |
| ZATCA SDK's local validator (`fatoora -validate`) | — | **not run** (the official SDK download link was not reachable) |

### Still missing or open

1. [UNCERTAIN] The standard's CSR table names organizationIdentifier
   (2.5.4.97) for the VAT number and businessCategory for the invoice
   types; KhataOS follows the SDK and sandbox sample (UID and title), which
   the sandbox accepts. Confirm in simulation.
2. [UNCERTAIN] The QR timestamp (tag 3) has no "Z" (`2026-05-01T10:15:30`,
   Riyadh time); the SDK sample writes `…Z` on local time. The sandbox
   accepts ours.
3. [UNCERTAIN] The SignedProperties digest form (see the stamp) is not
   checked by the sandbox; it reproduces the SDK sample's value exactly.
4. The renewal API's verb and credentials against the live portal; the
   PDF/A-3 output for buyers; the exact error code ZATCA returns for an
   expired CSID [UNCERTAIN].
5. Run every document type through the simulation environment with a real
   taxpayer, and through the SDK validator; confirm the credit-note root
   element, the empty customer party on simplified invoices, `PCE` units,
   and the payment-means codes (all accepted by the sandbox).
6. Also not modelled: prepayments (`386` and their settlement lines),
   third-party, nominal, summary and self-billed flags, VAT groups' member
   TINs, buyer IDs other than the VAT number (NAT, IQA…), document
   currencies other than SAR (foreign-currency vouchers are refused before
   any XML is written), and the credit/debit note 15-day deadline.

### Sources for Phase 2

- ZATCA, *e-Invoicing Sandbox Release (2.1.0)* API specifications
  (Compliance CSID, Production CSID, Compliance Invoice, Reporting,
  Clearance, Renewal), served by the developer portal
  (`gw-fatoora.zatca.gov.sa/e-invoicing/developer-portal/users/api/v1/swagger/render?fileId=COMPLIANCE_CSID`
  and `ONBOARDING`, `COMPLIANCE_INVOICE`, `REPORTING`, `CLEARANCE`,
  `RENEWAL`), fetched 20 Oct 2026 [PRIMARY]. Linked from
  <https://sandbox.zatca.gov.sa/IntegrationSandbox>.
- ZATCA, *Electronic Invoice Security Features Implementation Standards*
  v1.1 (2022-06-24), §2.2.2 (certificate profile, CSR Table 1), §2.3.3
  (XAdES structure), §3 (PIH), §4 (QR):
  <https://zatca.gov.sa/ar/E-Invoicing/SystemsDevelopers/Documents/20220624_ZATCA_Electronic_Invoice_Security_Features_Implementation_Standards.pdf>
  [PRIMARY].
- ZATCA, *Fatoora Portal User Manual* (production base URL, onboarding with
  an OTP): <https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/Fatoora_Portal_User_Manual_English.pdf>
  [PRIMARY].
- ZATCA e-invoicing SDK 3.2.7 sample data (signed sample invoices, CSR
  configuration template, `fatoora` usage text with `-nonprod`/`-sim`), as
  mirrored at <https://github.com/mrsool/zatca/tree/main/einvoicing-sdk>
  (the official download from zatca.gov.sa returned 403 to a script)
  [PRIMARY content, SECONDARY copy]. Only data files were read; no SDK code
  was run.
- ZATCA developer community forum (zatca1.discourse.group): simulation base
  URL, OTP validity, signed-properties hashing threads [SECONDARY].
- wes4m/zatca-xml-js (open-source TypeScript implementation): CSR template,
  certificate and SignedProperties digest method, Basic auth form
  <https://github.com/wes4m/zatca-xml-js> [SECONDARY, confirmed against the
  SDK sample and the sandbox above].

## VAT return

`ksa_vat_return(from, to)` returns the return's figures by name (the
portal's line numbers were not checked [UNCERTAIN]) from Sales, Credit
Note, Purchase and Debit Note vouchers [SECONDARY: ZATCA return form as
reported by Avalara, Tally, Wafeq]:

| Line | How |
|---|---|
| Standard-rated sales | Amount and VAT; credit notes reduce, customer debit notes add. Booked VAT is spread over the standard-rated lines by value |
| Private healthcare / education to citizens | **0, not modelled** (a VATEX-SA-HEA/-EDU class lands in zero-rated sales) |
| Sales to registered customers in other GCC states | **0, not modelled** (line of the 2017 form; whether the live form keeps it is [UNCERTAIN]) |
| Zero-rated domestic sales, exports, exempt sales | Amount only; an untaxed sale to a party whose address is abroad is an export |
| Standard-rated purchases | Amount and VAT, all taken as recoverable: **restricted input tax and apportionment are not applied**. Purchases without a supplier VAT number are left out, with a warning |
| Imports subject to VAT paid at customs (**field 8**) | Lines classed `IMPORT:…` (value) and VAT posted to VAT ledgers classed `IMPORT` on any voucher (the customs declaration's VAT); no supplier VAT number needed |
| Imports under the reverse charge (**field 9**) | Lines classed `RC:…` (services bought from abroad), VAT worked out at the rate class; the return treats it as both due and deductible, so it is in total purchases **and** added back to the VAT due |
| Zero-rated and exempt purchases | Amount only |
| Total VAT due, corrections (**0**), credit carried forward (**0**), net | Net positive is payable |

Field numbers 1, 7, 8 and 9 are confirmed by ZATCA guidelines (Input Tax
Deduction Guideline, July 2020; Imports and Exports Guideline, 2nd ed.,
May 2026, §§5.3, 6.3, 10.4 [PRIMARY]); the others are [UNCERTAIN], so the
screen shows names. Field 9 is entered once: ZATCA's form treats its VAT as
both output and input tax, and a business that cannot deduct all of it
puts the difference in field 9's adjustment column (a warning says so).
Exempt imported services are not reported. Import VAT deferred under IR
Art 44 also goes in field 9; not modelled. Mark services from abroad
`RC:STANDARD` (stock item or service ledger) and self-account them in the
books (Dr expense, Dr input VAT, Cr supplier, Cr output VAT; a warning
compares); mark imported goods `IMPORT:STANDARD`. This is the same class
convention as the UAE, UK and Singapore packs. A purchase from a supplier
abroad with no VAT and nothing marked gets a reminder. Note: ZATCA's
guidelines now give **SAR 15,000** as the threshold for correcting an
understatement in the next return rather than a voluntary disclosure
(Imports and Exports Guideline §11.5), not the older IR Art 63(3) SAR
5,000; corrections are still not modelled.

Out-of-scope lines are on neither side. Journals posting to tax ledgers are
left out with a warning, except postings to VAT ledgers classed `IMPORT`. Monthly filing above SAR 40m of supplies, else
quarterly, due by the end of the following month [SECONDARY].

## Bilingual invoices

`khata_print::invoice_html_bilingual(header, invoice, &ArabicLabels::default(), qr)`
prints every label in English and Arabic, user text with `dir="auto"`,
Arabic with `lang="ar"`, and an Arabic-capable font stack ('Noto Naskh
Arabic', 'Segoe UI', 'Geeza Pro', 'Arial'). The desktop uses it for KSA and
UAE companies; India is unchanged. The Arabic wording follows common ZATCA
and UAE FTA usage; have an Arabic-reading adviser check it [UNCERTAIN]:

| English | Arabic |
|---|---|
| Tax Invoice | فاتورة ضريبية |
| Simplified Tax Invoice | فاتورة ضريبية مبسطة |
| Credit Note / Debit Note | إشعار دائن / إشعار مدين |
| VAT number | الرقم الضريبي |
| VAT | ضريبة القيمة المضافة |
| Total | الإجمالي |
| Invoice number / Date | رقم الفاتورة / التاريخ |
| Buyer / Seller | المشتري / البائع |
| Description / Quantity / Unit price / Amount | الوصف / الكمية / سعر الوحدة / المبلغ |
| Total excluding VAT | المبلغ الخاضع للضريبة |
| Tax class | الفئة الضريبية |
| Place of supply | مكان التوريد |
| Amount in words | المبلغ كتابة |
| Authorised signatory | المفوض بالتوقيع |

KSA requires Arabic on customer-facing invoices (research §5 [SECONDARY]):
party names, item names and addresses print as entered, so enter Arabic
names (or both) in the masters. Eastern Arabic numerals are not used on
prints; XML always uses 0–9.

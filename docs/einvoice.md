# e-Invoice and e-way bill JSON (India)

KhataOS can turn a saved voucher into the JSON that the government portals'
offline and bulk tools take: an **e-invoice** in the FORM GST INV-01 shape
for an Invoice Registration Portal (IRP), and an **e-way bill** in the bulk
generation shape. With API access set up, it can also **send** a Sales or
Credit Note voucher to an IRP and keep the IRN, acknowledgement and signed
QR code it returns (see [IRN generation through an IRP](#irn-generation-through-an-irp-connector)).
It does not claim the portal will accept a document: the portal's
validations are the only authority. Nothing here is legal advice.

Code: `crates/khata-in/src/einvoice.rs` and `crates/khata-in/src/ewaybill.rs`
(the JSON shapes and checks, on a neutral document model),
`apps/desktop/src-tauri/src/einvoice.rs` (vouchers to documents; Tauri
commands `einvoice_json`, `einvoice_bulk_json`, `ewaybill_json`; save the
result with `write_gst_json`). Sources: `docs/research/india-compliance.md`
§4.3 (HSN), §4.5 (e-invoicing), §4.6 (e-way bill), §9 (open items).

## What is generated

**e-Invoice (INV-01, `Version` "1.1")**, one document or a bulk array:

| Block | Fields | From the books |
|---|---|---|
| `TranDtls` | `TaxSch` "GST", `SupTyp` (B2B, SEZWP, SEZWOP, EXPWP, EXPWOP, DEXP), `RegRev`, `IgstOnIntra` | B2B by default; an export (with or without IGST, as booked) when the party's mailing country is not India; SEZ, deemed export, reverse charge and IGST-on-intra are chosen per call |
| `DocDtls` | `Typ` (INV, CRN, DBN), `No`, `Dt` (dd/mm/yyyy) | Sales → INV, Credit Note → CRN. KhataOS's Debit Note is a purchase return, so it is not e-invoiced; DBN is supported by the builder only |
| `SellerDtls` | `Gstin`, `LglNm`, `Addr1`, `Addr2`, `Loc`, `Pin`, `Stcd` | Company GSTIN, name, profile address (first line → `Addr1`, middle lines → `Addr2`, last line → `Loc`; PIN = the last six-digit number), registered state |
| `BuyerDtls` | `Gstin`, `LglNm`, `Pos`, `Addr1`, `Addr2`, `Loc`, `Pin`, `Stcd` | Party GSTIN and name; address, PIN and state from the ledger's mailing details; place of supply from the GSTIN's state, else the mailing state. Exports: `URP`, `Pos`/`Stcd` 96, `Pin` 999999 when none is given [UNCERTAIN] |
| `ItemList` | `SlNo`, `PrdDesc`, `IsServc` (Y when the HSN starts with 99), `HsnCd`, `Qty`, `Unit`, `UnitPrice`, `TotAmt`, `Discount`, `AssAmt`, `GstRt`, `IgstAmt`, `CgstAmt`, `SgstAmt`, `CesRt`, `CesAmt`, `TotItemVal` | One item per stock line (name, HSN, quantity, unit → UQC, rate, amount). Discounts on the party's side are spread over the items by amount. Booked tax is spread over the items by amount × the rate on file (or by amount when a rate is missing, and the rate is then worked out from the amounts and snapped to a GST rate) |
| `ValDtls` | `AssVal`, `CgstVal`, `SgstVal`, `IgstVal`, `CesVal`, `RndOffAmt`, `TotInvVal` | Sums of the items; a ledger named "Round off" is `RndOffAmt`; `TotInvVal` must equal the party line |

The bulk command takes every uncancelled Sales and Credit Note in a period
to a registered buyer, or classed as export/SEZ, and skips B2C. It is all
or nothing: if any document fails, the error lists every failing document.

**e-Way bill (bulk JSON: `version`, `billLists`)**, Part A and an optional
Part B: `userGstin`, `supplyType` (O for a Sales voucher, I for a Purchase),
`subSupplyType`, `docType` (INV, BIL, CHL), `docNo`, `docDate`, from and to
GSTIN (`URP` when unregistered), trade name, address, place, PIN, state
and actual state; `transactionType`; `itemList` (goods lines only: name,
HSN, quantity, UQC, taxable amount, CGST/SGST or IGST rate, cess rate);
`otherValue` (round-off, and non-stock lines such as freight or services);
total, tax and invoice values; `transMode`, `transDistance`,
`transporterId`, `transporterName`, transport document number and date,
`vehicleNo`, `vehicleType`. On a purchase, the supplier's invoice number and
date can be given (the voucher number is used otherwise).

Helpers for the UI: `einvoice::applicability_threshold()` (₹5 crore),
`einvoice::last_reporting_day(date, aato)` (30 days, for AATO of ₹10 crore
or more), `ewaybill::needed(value)` (over ₹50,000), and
`ewaybill::validity_days(km, over_dimensional)`.

## Local checks

These run before any JSON is produced, and the messages name the document
("Sales 26-27/1: the buyer's PIN code is missing; …"):

- document number: at most 16 characters of letters, digits, `-` and `/`
  (Rule 46(b));
- company GSTIN present and valid, and its state is the company's state;
  buyer GSTIN present and valid for B2B, SEZ and deemed exports;
- legal name, first address line, location, six-digit PIN and state code
  for the company and the buyer (PIN and state may be left out for an
  export buyer);
- every item has an HSN/SAC of digits only, with at least the minimum
  digits passed in (4 or 6), and 4, 6 or 8 digits in all; goods have a
  quantity and a unit that maps to a UQC (an unmapped unit is an error, not
  silently "OTH");
- quantity × price, and each item's tax against assessable value × rate,
  within ₹1 [UNCERTAIN: the IRP's own tolerances are not confirmed];
- the tax split matches the place of supply (CGST + SGST within the
  company's state, else IGST; IGST only for exports and SEZ; no tax under
  LUT/bond);
- items + tax + round-off equal the document total within a paisa;
- e-way bill: the company is the consignor (outward) or consignee
  (inward), both PINs and states, an approximate distance, goods lines with
  HSN and UQC, a vehicle number only with road transport, a transport
  document for rail, air and ship, a 15-character transporter ID.

## What the books don't have yet

- **Party addresses**: ledgers now carry mailing details (address, state,
  PIN, country). When they are missing, the commands take the buyer's
  address, location, PIN and state per call (`BuyerDetails`, `EwayDetails`)
  and say what is missing. Nothing given per call is saved.
- **HSN/SAC on ledgers**: sales and service ledgers now carry their own
  HSN/SAC (`Ledger.tax_class`), used for lines without stock items.
- **Aggregate turnover**: the books don't hold the AATO. The company sets
  the HSN digits its invoices must show (Company details: 4 up to ₹5 crore,
  6 above), and the 30-day reporting limit is a helper the UI calls with the
  turnover.
- **SEZ buyers** are marked in the party's mailing details (`special_zone`);
  the supply type then defaults to SEZWP/SEZWOP. Deemed exports are still
  chosen per call.
- **Supplier invoice number** on purchases is not stored as its own field;
  it is usually the bill-wise reference on the supplier's line, and is
  given per call for inward e-way bills.
- **Delivery challans** (stock transfers, job work) are not voucher types
  yet, so e-way bills come from Sales and Purchase only.

## What must be verified before release

1. **The INV-01 field list, types and limits** against the official schema
   on the e-invoice portal (developer documentation). The schema was not
   retrieved (research §9 item 2; version 1.1 vs a claimed 1.11 is
   [UNCERTAIN]). In particular: whether `Pin` is a number and `Stcd` a
   string, whether `SlNo` is a string, optional fields (`Addr2`, `Unit`
   and `Qty` for services), and maximum lengths. All field names live in
   one function, `khata_in::einvoice::to_json`. Partly answered by NIC's
   Generate IRN page (archived 2024; its JSON schema is on that page):
   `Version` "1.1", `Pin` a number, `Stcd` and `SlNo` strings, `Qty` and
   `Unit` optional for services, at most 1000 items, payload ≤ 2 MB, the
   document number not starting with 0, `/` or `-` [PRIMARY]. A
   field-by-field comparison of `to_json` with that schema is still to do.
2. **The bulk e-invoice format** (assumed to be a plain JSON array of
   INV-01 objects) against the IRP's bulk upload tool.
3. **Export buyer conventions**: `Gstin` "URP", `Pos`/`Stcd` 96 and `Pin`
   999999 — confirmed by the Generate IRN validations ("In case of
   transaction of direct export, recipient GSTIN has to be URP and state
   code has to be 96, PIN code should be 999999, POS should be 96")
   [PRIMARY].
4. **The e-way bill bulk JSON**: the `version` value ("1.0.0621" is
   [UNCERTAIN]), field names (`actFromStateCode`, `otherValue`, …), which
   values are numbers or strings, and the code lists for `subSupplyType`,
   `transactionType` and `transMode` (all from the format as commonly
   reproduced, not from a retrieved specification). All of it lives in
   `khata_in::ewaybill::bill_json`.
5. Amount tolerances on the e-way bill system. The IRP's (Generate IRN
   validations): item and total tax values between the calculated value's
   rupee part minus ₹1 and its next rupee plus ₹1; round-off between
   −99.99 and +99.99 [PRIMARY]; the ₹1 local check is close to that.
6. State-specific intra-State e-way bill thresholds (research §9 item 9);
   only the general ₹50,000 limit is modelled.
7. RCM self-invoice supply types RCMDOM and RCMIMP (sandbox only as of
   Oct 2026, research §4.2); supply types are an enum, easy to extend once
   notified.

## Applicability (from the research doc)

| Rule | Status |
|---|---|
| e-Invoicing for B2B invoices, CN/DN, exports and SEZ supplies by notified persons (CGST Rule 48(4)) | [PRIMARY] |
| Threshold: aggregate turnover above ₹5 crore in any FY from 2017-18, from 1 Aug 2023 (Notif. 10/2023-CT) | [SECONDARY] |
| Lowering to ₹2 crore | [UNCERTAIN: proposed, not notified] |
| Exempt classes (banks, insurers, NBFCs, GTA, passenger transport, multiplex admission, SEZ units, government) | [SECONDARY] |
| 30-day reporting limit for AATO of ₹10 crore or more, from 1 Apr 2025 (GSTN advisory, 5 Nov 2024) | [PRIMARY] |
| Cancellation within 24 hours, whole document, number not reusable | [SECONDARY] |
| HSN: 4 digits up to ₹5 crore AATO, 6 above (Notif. 78/2020-CT) | [SECONDARY] |
| e-Way bill above ₹50,000 consignment value (Rule 138(1)); Part A/B; validity 1 day per 200 km (Rule 138(10)) | [PRIMARY] |
| No e-way bill for documents older than 180 days, from 1 Jan 2025 | [PRIMARY] |

## IRN generation through an IRP (connector)

Code: `crates/khata-in/src/irp.rs` (the NIC API's crypto and message
shapes, no I/O), `apps/desktop/src-tauri/src/irp_client.rs` (credentials,
HTTP, storage, printing helpers; Tauri commands `irp_settings`,
`set_irp_settings`, `irp_record`, `irp_generate`, `irp_cancel`,
`irp_details`, `irp_ewaybill`, `irp_gstin`),
`apps/desktop/src/screens/irp.ts` (IRP settings and the IRN screen;
`irpApi` in `api.ts`).

### What it does

| Step | API (NIC v1.03/v1.04) | What KhataOS does |
|---|---|---|
| Sign in | `POST /eivital/v1.04/auth`, headers `client_id`, `client_secret`, `Gstin` | Makes a random 32-byte AppKey; Base64-encodes the JSON `UserName`, `Password`, `AppKey`, `ForceRefreshAccessToken`, encrypts it with the IRP public key (RSA **PKCS#1 v1.5**) and sends `{"Data": ..}`. Decrypts the returned `Sek` with the AppKey (AES-256-ECB, PKCS#7). Keeps the AuthToken and SEK **in memory only** until 10 minutes before `TokenExpiry` (6 hours; 1 hour on the sandbox). Signs in again once if a call returns 1005 (token expired) |
| Generate IRN | `POST /eicore/v1.03/Invoice`, plus headers `user_name`, `AuthToken` | Sends the INV-01 JSON of the voucher (the same builder and checks as the offline export) encrypted with the SEK. Stores `Irn`, `AckNo`, `AckDt`, `SignedInvoice`, `SignedQRCode`, `Status`, `EwbNo`/`EwbDt`/`EwbValidTill`, `Remarks`, and the decrypted answer verbatim. A voucher with an active IRN is **not sent again** (NIC asks integrators to flag sent payloads) |
| Duplicate (2150) | `InfoDtls` `DUPIRN` | Fetches the existing IRN (`GET /eicore/v1.03/Invoice/irn/{irn}`, else by document details) and stores it, so a lost first answer is recovered. 2295 (registered on another IRP) is reported with that IRN; it is not registered again |
| Get IRN details | `GET .../Invoice/irn/{irn}`; `GET .../Invoice/irnbydocdetails?doctype=&docnum=&docdate=` | Refreshes status (ACT, CNL, REJ), QR and e-way bill. NIC: only within 3 days of generation (2283 after that) |
| Cancel IRN | `POST /eicore/v1.03/Invoice/Cancel` with `Irn`, `CnlRsn` (1 duplicate, 2 data entry mistake, 3 order cancelled, 4 other), `CnlRem` (≤ 100) | Checked first on this computer: within **24 hours of `AckDt`** (read as IST), no e-way bill generated, not already cancelled. The voucher in the books is **not** changed; its number can never get an IRN again, and the screen says so |
| e-Way bill by IRN | `POST /eiewb/v1.03/ewaybill` | Part B (mode, vehicle or transport document, distance; 0 = the portal's PIN-to-PIN distance) or a transporter id alone (Part A). Checked against the schema on the NIC page before sending. Invoices only (NIC: not for credit/debit notes or services) |
| Get GSTIN details | `GET /eivital/v1.04/Master/gstin/{gstin}` | Returns legal/trade name, address, state, taxpayer type, status, e-way bill block status |

Error codes from NIC's lists (`VITAL` and `INV`) are turned into plain
messages with the IRP's own text in brackets (`khata_in::irp::explain`):
for example 2150 duplicate, 2270 cancellation period over, 2230 e-way bill
exists, 2278 cancelled number reused, 1008/1017/1019 wrong API user or
password, 1013/1016 wrong public key, 1010 wrong client id/secret.

### Where things are kept

- **Credentials**: `irp-credentials.json` in the app data folder, keyed by
  GSTIN, written with mode 0600 (Unix; on Windows the per-user AppData
  folder). Never in the company file, the audit trail, sync, accountant
  packages or exports; never returned to the webview (only "set / not
  set"); `Debug` output is redacted; error messages never include them.
- **The IRP's answer**: a shared record of the company, key
  `irp.irn.<voucher id>`. It is not in the audit trail, but it syncs with
  the books (office server, own server, relay, `.khatapkg`;
  `docs/sync-settings.md`), so every computer sharing the company prints
  the IRN and nobody registers the invoice twice. Moving it into the books
  themselves needs a core change (a voucher field and an audit entry) and
  is open.
- If the voucher is altered after its IRN, the IRN screen warns (the
  registered `DocDtls`/`ValDtls` are kept for comparison): cancel within 24
  hours and register again, or issue a credit/debit note.

### Signed QR code and printing

`SignedQRCode` and `SignedInvoice` are JWTs (RS256). Their content is
decoded without verification (`khata_in::irp::signed_qr`); when NIC's
verification certificate or key is pasted into IRP settings, both
signatures are checked (RSA-SHA256, `alg` `RS256` or the XML-DSig
rsa-sha256 URI that NIC's Get IRN sample shows) and the screen says
"verified" or "does NOT match". The printed QR code encodes the
`SignedQRCode` string exactly as returned (`irp_client::qr_text`), rendered
with the same QR encoder and `khata_print::qr_svg` as the Saudi invoices.

For the invoice print, `irp_client::print_info(slot, voucher_id)` returns
the IRN, Ack No, Ack date, whether it is cancelled, and the QR symbol;
`irp_client::print_block_html(slot, voucher_id)` returns a ready block, and
`irp_client::splice_print_block(html, &block)` places it above the buyer
and number boxes of `khata_print::invoice_html`. The invoice print
(`invoice_design.rs`) does this for Indian Sales invoices and Credit Notes,
so a printed or PDF invoice with an IRN carries the IRN, Ack No and date and
the signed QR, with "IRN CANCELLED" once it is cancelled.

### Setting it up

1. Get API access. Direct access from NIC needs registration on the
   e-invoice portal, a sandbox test report and up to four whitelisted Indian
   static IPs; other taxpayers go through a GSP or ERP, which gives the
   client id and secret (NIC developer portal, *On-boarding*). Create the
   **API user** (user name and password) on the IRP portal for each GSTIN;
   it is not the portal login. Production and sandbox credentials and keys
   are separate.
2. Download the IRP public key (sandbox: `einv_sandbox.pem` from the
   developer portal after login; production: from the production portal).
   Optionally download NIC's public key/certificate for verifying signed
   content.
3. In KhataOS: Go To (Alt+G) → **e-Invoice portal (IRP) settings** (also
   **S** on the IRN screen) → choose *NIC sandbox*, *NIC IRP 1*, *NIC IRP
   2* or *Other IRP or GSP* (enter its API base URL); enter the client id,
   client secret, API user name and password; paste or load (Ctrl+O) the
   public key and, optionally, the verification certificate.
4. Open a saved Sales invoice or Credit Note, press **Ctrl+E** (Export) and
   choose **Generate IRN (e-invoice portal)**. On the IRN screen **G**
   generates, **D** fetches details, **C** cancels (24 hours), **E** makes an
   e-way bill, **S** opens IRP settings.

The offline files are on the same Export menu: **e-Invoice JSON** (Sales
and Credit Notes) and **e-Way Bill JSON** (Sales and Purchase). On the
GSTR-1 screen, **Ctrl+Alt+E** saves the period's e-invoice JSON in one file.

### Which IRP

| IRP | Portal | API base used | Status |
|---|---|---|---|
| NIC sandbox | einv-apisandbox.nic.in | `https://einv-apisandbox.nic.in` | [PRIMARY: developer portal; paths as listed by Microsoft's Dynamics 365 India setup] |
| NIC IRP 1 | einvoice1.gst.gov.in | `https://api.einvoice1.gst.gov.in` | [SECONDARY: NIC advisory as reproduced by TaxTMI] |
| NIC IRP 2 | einvoice2.gst.gov.in | `https://api.einvoice2.gst.gov.in` | [SECONDARY: same]. NIC 1 and 2 interoperate: an `irp` header (`NIC1`/`NIC2`) lets one portal act on the other's IRNs for get, cancel and e-way bill; KhataOS does not send it yet |
| Cygnet (IRP 3), ClearTax (IRP 4), EY (IRP 5), IRIS (IRP 6) | einvoice3 to einvoice6.gst.gov.in | enter the base URL | [UNCERTAIN] Each publishes its own API documentation and sandbox (GSTN advisory on direct API integration with IRPs, Nov 2023, as reported). Whether they accept NIC's request shapes, paths and encryption unchanged was not verified |
| GSPs | — | enter the base URL | [UNCERTAIN] Many GSPs wrap the NIC API with their own authentication (for example Vayana's `X-Asp-Auth-Token`/`X-Asp-Auth-Signature` headers and `/gus/irp/{provider}/...` paths), so a GSP that is not NIC-pass-through will not work with this connector as is |

### What was tested

- **Crypto round trips** (`cargo test -p khata-in irp`): RSA PKCS#1 v1.5
  auth payload decrypted by the "IRP side" with a test private key
  (credentials, 44-character AppKey); SEK encrypted/decrypted with the
  AppKey; payloads and responses under the SEK (plain JSON and Base64-JSON
  inside); AES-256 against the FIPS-197 vector; public keys from PEM
  `PUBLIC KEY`, X.509 `CERTIFICATE` and bare Base64; JWT signatures that
  verify with the right key only; NIC's own sample `SignedQRCode` decodes
  to the sample's GSTIN, document and IRN (its signature, by NIC's key,
  correctly does not verify with ours); envelopes in every shape the NIC
  samples show (`Status`/`status`/`" Status "`, `ErrorDetails` as JSON, as a
  JSON string, as Base64; `InfoDtls` `DUPIRN`, `DUPIRNGLP`, `EWBERR`); the
  24-hour window in IST; Cancel and e-way bill payload checks.
- **A mock IRP** (axum, in `irp_client.rs` tests, using the same crypto from
  the IRP side): sign-in with header and credential checks; generate, store,
  print data and no resend; duplicate 2150 with a lost local record →
  existing IRN fetched; 1005 → one more sign-in; cancel inside and outside
  24 hours, then no new IRN for that number; get details after cancelling;
  e-way bill by IRN and the cancel block it causes; GSTIN details; wrong
  password → plain message without the password; unreachable IRP; settings
  file mode 0600 and no secrets in the company file, trail or webview.
- **Not tested live.** The NIC sandbox needs registration (a GSTIN enabled
  for e-invoicing, client id/secret issued after login on
  einv-apisandbox.nic.in) and was not reachable from where this was built
  (connections refused or timed out; the documentation was read through the
  Internet Archive). No private IRP or GSP was tried.

### Open points [UNCERTAIN]

1. Header names: NIC's tables say `client_id`, `client_secret`,
   `user_name`; the Java and C# samples send `client-id`, `client-secret`.
   Both spellings of the client headers are sent. Confirm on the sandbox.
2. Whether the decrypted `Data` is the JSON itself or Base64 of it (both are
   accepted).
3. The path prefix of Get IRN by document details (`/eicore/v1.03/Invoice/
   irnbydocdetails` assumed) and the production paths on the `api.` hosts
   (assumed the same as the sandbox).
4. IRP times (`AckDt`, `TokenExpiry`, `CancelDate`) are read as IST.
5. The sign-in payload must fit one RSA block (245 bytes for a 2048-bit
   key after Base64): very long user names or passwords are refused with a
   message.
6. The e-way bill API's own cancellation (needed before cancelling an IRN
   with an e-way bill) is not implemented: cancel the e-way bill on the
   e-way bill portal.
7. MFA (mandatory for portal users from 1 Apr 2025) applies to portal
   logins; API users authenticate with the credentials above. Whether the
   API user creation now needs MFA steps on the portal was not checked.
8. An "IRN pending" list for the 30-day reporting limit (AATO ≥ ₹10 crore)
   is still to be built on top of `irp_record`.

Sources: NIC e-Invoice API developer portal pages *Authentication*
(v1.03, v1.04), *Generate IRN*, *Cancel IRN*, *Get e-Invoice details*,
*Get IRN details by document details*, *Generate e-way bill by IRN*, *Get
GSTIN details*, *Sample code in Java* and *C#*, *Release notes*, *Note on
top errors*, *On-boarding* (einv-apisandbox.nic.in, read via
web.archive.org, 2024 captures); error code lists at
einvoice1.gst.gov.in/others/geterrorcodes/VITAL and /INV (archived);
Microsoft Learn, *Electronic invoices in India* (sandbox URLs); TaxTMI
news 31397 (NIC1/NIC2 hosts and the `irp` header); ClearTax, *IRP providers
list* and ClearIRP FAQs (IRP 3 to 6); Vayana GSP docs (GSP differences,
token reuse margin).

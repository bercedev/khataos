# United Kingdom VAT pack (`khata-gb`)

KhataOS can keep books for a VAT-registered business in the United Kingdom:
it checks VAT registration numbers with HMRC's check digits, charges 20%,
5% or 0% VAT on invoices, checks invoice numbers, works out the 9-box VAT
return from the books, saves the Making Tax Digital (MTD) API body for it
and can submit it to HMRC through the VAT (MTD) API with your own HMRC
developer application (see [Submitting to HMRC](#submitting-to-hmrc-making-tax-digital)).
Nothing here is tax advice: check the figures with your accountant before
you file with HMRC.

Code: `crates/khata-gb` (the rules: `vrn.rs`, `nations.rs`, `vat.rs`,
`returns.rs`, `schemes.rs`, `mtd.rs`, `fraud.rs`), `apps/desktop/src-tauri/src/vat_gb.rs`
(vouchers to return documents; the `vat_return_gb(from, to)` and
`mtd_vat_json(from, to, period_key)` commands),
`apps/desktop/src-tauri/src/mtd_client.rs` (HMRC: OAuth, fraud-prevention
headers, obligations, submit, view, liabilities, payments),
`apps/desktop/src/screens/vat_gb.ts` (`UkVatReturnReport`) and
`apps/desktop/src/screens/mtd.ts` (the MTD screens); neither screen is on a
menu yet. Sources and open questions are in `docs/research/uk-compliance.md`.
Tags: **[PRIMARY]** confirmed against legislation, HMRC guidance, HMRC
material or the HMRC API specification; **[SECONDARY]** reported by other
sources; **[UNCERTAIN]** not confirmed.

## Setting up a UK company

1. Pick **United Kingdom** as the country. The books use GBP (2 decimals,
   "£") and **April to March** years by default. A company's own year runs
   to its accounting reference date (any date, set at Companies House), so
   change the books' first month to match it. April is the default because
   the Corporation Tax financial year runs 1 April to 31 March (HMRC
   CTM01405) and many companies use it. The personal tax year (6 April to
   5 April) does not start on the 1st and is not supported as a year.
2. In **Company → Tax registration**, enter the **VAT Reg No** and pick the
   **nation**: England, Scotland, Wales or Northern Ireland (`ENG`, `SCT`,
   `WLS`, `NIR`). The nation never changes the rate. Northern Ireland, or a
   VAT number with the `XI` prefix, turns on Boxes 8 and 9 for goods traded
   with the EU.
3. Add the rate list (also in `khata_gb::vat::default_rates`):

   | Class | Rate | From | Meaning |
   |---|---|---|---|
   | `STANDARD` | 20 | 2011-01-04 | Standard rate (VATA 1994 s2; 20% from 4 Jan 2011, Notice 700 §3.3.1) [PRIMARY] |
   | `REDUCED` | 5 | 2011-01-04 | Reduced rate (s29A, Sch 7A): home energy, children's car seats… (5% since 1 Sep 1997) [PRIMARY] |
   | `ZERO` | 0 | 2011-01-04 | Zero-rated (s30, Sch 8): most food, children's clothes, books, exports [PRIMARY] |
   | `EXEMPT` | 0 | 2011-01-04 | Exempt (s31, Sch 9): financial and property transactions, postage stamps, insurance [PRIMARY] |

   Standard rates before 4 Jan 2011 are not seeded; add them (Notice 700
   §3.3.1 lists them) if you enter older invoices. Exempt and zero-rated are told apart **by name
   only**: a class starting `EXEMPT` is exempt, any other 0% class
   zero-rated; a class starting `REDUCED` is reduced.
4. Create **Output VAT** and **Input VAT** ledgers under Duties & Taxes with
   type of tax `VAT`.
5. For customers and suppliers abroad, set the **country** in the party's
   mailing details: a sale with no VAT to a party outside the UK (and the
   Isle of Man) is reported as an export (zero-rated).

## What is checked and computed

**VAT registration numbers.** Spaces, hyphens and dots are dropped and
letters upper-cased; a `GB` or `XI` prefix is kept as typed.
- **9 digits** with two check digits. HMRC applies "first Mod 97 and, if
  that failed, Mod 9755" (VAT Software Developer Forum minutes, 10 Mar 2009
  [PRIMARY]). Weight the first seven digits by 8, 7, 6, 5, 4, 3, 2 and sum;
  Mod 97 check = 97 − (sum mod 97) (00 when the remainder is 0); Mod 97-55
  adds 55 to the sum first. HMRC's own example: 562 2359 **45** (Mod 97)
  and 562 2359 **87** (Mod 97-55) — both are in the tests. Some secondary
  sources say 55 is *subtracted*; HMRC's example rules that out.
- **12 digits** (branch traders): the check digits are on the first nine
  [SECONDARY].
- **`GD` + 000–499** (government departments) and **`HA` + 500–999**
  (health authorities), no check digits [SECONDARY].
- The VRN proper (nine digits, no prefix or branch) is the entity ID; it is
  what the MTD API takes. A number encodes no nation.
- Passing the check means the number is well formed, not that it is
  registered: HMRC's online "Check a UK VAT number" service confirms that.
  (The "987 6543 21" on Notice 700's sample invoice fails the check: example
  numbers in guidance are illustrative.)

**VAT on an invoice line.** One `VAT` component at the class's rate on the
invoice date from the company's rate list, rounded to the **nearest penny**
(Notice 700 §17.5.1 allows rounding line VAT "to the nearest 1p"
[PRIMARY]; a half-penny goes away from zero [UNCERTAIN]). Exports (place of
supply abroad) are zero-rated whatever the class; the export conditions are
not checked. "Without payment" is refused on a UK supply.
`vat::document_vat` gives the invoice-level total rounded **down** to a
whole penny, which Notice 700 §17.5 allows for invoice traders (not
retailers, §17.6); the books use the line figures.

**Invoice numbers.** VAT Regulations 1995 reg 14(1)(a) asks for "a
sequential number based on one or more series which uniquely identifies the
document" [PRIMARY]. KhataOS checks:
- not empty, **unique across all years** in the series, ignoring case (the
  rule implies no yearly reset);
- at most **35 characters** of **letters, digits, `-` and `/`**. These are
  KhataOS choices (as for the UAE), not HMRC rules: 35 is the UN/EDIFACT
  document-number length, and no spaces means "INV 1" and "INV1" can't both
  exist. Each voucher type is its own gap-free series, so "sequential"
  holds.

**The 9-box return** (`returns::vat_return`), for any period, from Sales and
Credit Notes (outward) and Purchases and Debit Notes (inward):

| Box | What goes in | Units |
|---|---|---|
| 1 | VAT on sales (standard and reduced lines); credit notes reduce it | £ and p |
| 2 | VAT on acquisitions of goods from the EU into Northern Ireland: **not worked out, always 0** (warning when Box 9 is not zero) | £ and p |
| 3 | Box 1 + Box 2 | £ and p |
| 4 | VAT on purchases, all treated as reclaimable, from suppliers with a VAT number; purchase returns reduce it | £ and p |
| 5 | Box 3 − Box 4 (negative: a repayment) | £ and p |
| 6 | All sales excluding VAT: standard, reduced, zero-rated, exempt, exports | whole £ |
| 7 | All purchases excluding VAT (only purchase vouchers are counted) | whole £ |
| 8 | Goods (stock-item lines) sold from Northern Ireland to EU customers | whole £ |
| 9 | Goods bought into Northern Ireland from EU suppliers | whole £ |

Boxes 6–9 are whole pounds: the MTD API defines them "to 2 zeroed decimal
places" [PRIMARY]. The pence are dropped (truncated) [UNCERTAIN: truncation
vs rounding is not stated on the current pages]. Returns are normally
quarterly and due one calendar month and 7 days after the period ends
[PRIMARY: GOV.UK "VAT Returns"].

**MTD JSON** (`mtd::vat_return_json`, the screen's **Export MTD JSON**,
Ctrl+Alt+J). Asks for the 4-character **period key** HMRC gives for the
period (from the VAT obligations; "26A1", sometimes with `#`) and saves the
`VATReturnDeclaration` body of `POST /organisations/vat/{vrn}/returns`
(HMRC Developer Hub, VAT (MTD) API 1.0 [PRIMARY]):
`periodKey`, `vatDueSales`, `vatDueAcquisitions`, `totalVatDue` (always
Box 1 + Box 2), `vatReclaimedCurrPeriod`, `netVatDue` (the **absolute**
difference: the API takes 0 or more and works out the direction itself),
`totalValueSalesExVAT`, `totalValuePurchasesExVAT`,
`totalValueGoodsSuppliedExVAT`, `totalAcquisitionsExVAT` and `finalised`.
Amounts are exact JSON numbers (never floating point), with the API's range
limits checked. **`finalised` is `false` in the saved file**: it is the
user's legal declaration. When KhataOS submits a return itself, it sends
`finalised: true` only after the user ticks the declaration (below).

## Reverse charges and import VAT

**How a line is marked (no core change).** The core model has no field for
these, so they ride on the **tax class** (`khata_gb::vat::Mechanism`; the
same convention as the UAE, Singapore and Saudi packs). The rest of the
class is the rate class (`DRC:REDUCED` is the domestic reverse charge at
the reduced rate; a bare prefix means STANDARD). The invoice carries no VAT
for such a line (`vat::line_tax` gives no component).

| Class | Where | Boxes (customer) | Source [PRIMARY] |
|---|---|---|---|
| `RC:STANDARD` | stock item or service ledger: services from a supplier belonging outside the UK | VAT in **1 and 4**, value in **6 and 7** | Notice 741A §5.2; 700/12 §4.6 |
| `DRC:STANDARD` | the domestic reverse charge: mobile phones and computer chips (£5,000 or more of them per invoice, no exclusion for the first £5,000), wholesale gas and electricity, wholesale telecoms, emissions allowances, renewable energy certificates, and CIS construction services | VAT in **1 and 4**, value in **7 only** (never 6) | Notice 735 §§3.1, 6.1, 8.5; construction guide; 700/12 §4.6 |
| `DRC:` on a **sale** | the business supplies under the domestic reverse charge | value in **6**, no VAT in Box 1 | Notice 735 §7.7 |
| `IMPORT:STANDARD` | stock item: goods imported (from the overseas supplier's invoice) | value in **7** | "Complete your VAT Return to account for import VAT" |
| VAT ledger classed `IMPORT` | import VAT paid at the border (C79), posted on any voucher (a Payment, say) | debits in **4** (no supplier VRN needed) | 700/12 §4.2 |
| VAT ledgers classed `IMPORT:PVA` (an output and an input ledger) | postponed import VAT from the monthly statement (MPIVS, on CDS by the 10th working day; keep a copy), as a Journal: Dr the input ledger, Cr the output ledger | the output side's credits in **1**, and the same in **4** (a warning if the input side differs) | same page: Box 1, Box 4, Box 7 for the import value |

A sale, credit note or debit note with a `DRC:` item or service line prints "Reverse charge: Customer to pay the VAT to HMRC." under the lines, the wording Notice 735 §7.6.1 accepts (`invoice.rs`).

For the reverse charges, self-account in the books (Dr expense, Dr input
VAT, Cr supplier, Cr output VAT): the return works the VAT out at the rate
class and a warning compares it with what the voucher posts to output VAT.
A purchase from a party outside the UK VAT area with no VAT and nothing
marked gets a reminder. Who must apply the domestic reverse charge (the
£5,000 test, end-user or intermediary notices, the 5% disregard) is the
user's call.

## VAT schemes (Flat Rate, Cash Accounting, Annual Accounting)

Ctrl+Alt+F on the VAT Return screen (`vat_gb_schemes`,
`set_vat_gb_schemes`; owner only). **Where the settings live:** they
change tax figures, so they belong to the company, but the core profile has
no place for them yet. They are a **shared record of the company**
(`vat_gb.schemes`): not in the audit trail, but synced with the books to
the company's other computers and its accountant (office server, own
server, relay, `.khatapkg`; `docs/sync-settings.md`). The last change
wins.

**Flat Rate Scheme** (`schemes::flat_rate_return`; Notice 733 and 700/12
§4.1 [PRIMARY]):
- the company stores its sector and rate (HMRC's 54 sector rates are listed
  on the screen, from "VAT Flat Rate Scheme: how much you pay", checked 21
  Oct 2026), its VAT registration date (1% off the rate for supplies in the
  first 12 months, §4.7; it also applies to the 16.5% rate here), the
  limited cost business answer (auto, yes, no) and the turnover basis
  (invoices, or cash-based §9);
- **Box 1** = rate × flat rate turnover (all supplies **including VAT**,
  zero-rated and exempt included, §6.2), plus reverse-charge VAT and
  postponed import VAT, which stay outside the flat rate (§§6.4, 6.5);
- **Box 4** = VAT on a single capital purchase of **£2,000 or more
  including VAT** (§15.2; a purchase line posted to a ledger under Fixed
  Assets), plus the reverse-charge VAT; nothing else;
- **Box 6** = flat rate turnover (VAT included) plus the value of supplies
  outside the scheme (reverse-charge sales, imported services); **Box 7** =
  those capital purchases, reverse-charge purchases and imports, VAT
  excluded, and Box 9;
- **limited cost business** (16.5%, §4.4): goods bought, VAT included,
  below the higher of 2% of flat rate turnover and £1,000 a year (pro rata:
  £250 a quarter). "Auto" counts stock-item purchase lines (capital lines
  out) and says so in a warning: the books cannot leave out food and drink
  for staff, vehicles and fuel (§4.6), so set yes or no when you know.
- warnings: import VAT (border or postponed) is not reclaimed except on
  capital goods (add those by hand) [UNCERTAIN for postponed VAT]; the
  domestic reverse charge under the scheme is reclaimed in Box 4 as for
  imported services [UNCERTAIN: check HMRC's construction guide]; turnover
  over £230,000 for a year means leaving the scheme (§3.5). Join limit:
  £150,000 expected taxable turnover excluding VAT (§3.1).

**Cash Accounting Scheme** (`schemes::cash_basis`; Notice 731 and 700/12
§4.2 [PRIMARY]): VAT on the dates payments are received and made, and
Boxes 6 and 7 on payments too (§4.7). Each payment carries its invoice's
VAT in proportion (§8.1: £780 of a £1,200 invoice with £200 VAT is £130).
Payments go to the invoices they are allocated to (Agst Ref, or Advance
with the invoice's reference); **unallocated payments, on-account amounts
and credit notes go to the party's oldest open invoices first** (§5.7, §8.2
[PRIMARY]; for payments on account [UNCERTAIN]), with a warning naming the
party, and a payment with nothing to apply to is listed in a warning and
left out. A sale or purchase for cash (the party line is a cash or bank
ledger) is paid on its date. A credit note set against an invoice reduces
what is left to pay; one not set against anything counts on its own date.
Reverse charges and imports stay on their document date (§§2.7, 5.5). Not
modelled: the excluded supplies (hire purchase, lease purchase,
conditional and credit sales, invoices payable more than 6 months ahead or
issued in advance, §2.8): keep those off the scheme by hand; Boxes 8 and 9
(on supplies made, not payments: a warning). Join at £1.35m, leave above
£1.6m (§§2.1, 2.5). It cannot be combined with the Flat Rate Scheme (the
settings refuse it; use cash-based turnover instead).

**Annual Accounting Scheme** (Notice 732): a note only. Pick the whole
accounting year as the period; the return is due 2 months after it ends
(§3.6), and Box 5 less the interim payments (9 of 10% or 3 of 25% of last
year's liability, §§4.1, 4.2) is the balance to pay. Join up to £1.35m,
leave above £1.6m. It combines with cash accounting and the flat rate
scheme.

## Submitting to HMRC (Making Tax Digital)

KhataOS talks to HMRC's **VAT (MTD) API 1.0** directly from the desktop
(fraud-prevention connection method `DESKTOP_APP_DIRECT`). It reads the
obligations (HMRC's VAT periods and their period keys), submits the 9-box
return from the books for an open period, shows returns HMRC holds, and
lists liabilities and payments. Obligations and submit are the two
endpoints HMRC requires as the "minimum level of compliance"; the others
are optional [PRIMARY: VAT (MTD) OAS; VAT (MTD) end-to-end service guide].

### Why you register your own HMRC application

HMRC's token endpoint needs the application's **client secret** ("Client
Secret is missing: 400 invalid_request"), even though HMRC also supports
PKCE ("We also support PKCE, but you do not have to use it")
[PRIMARY: Developer Hub, *User-restricted endpoints*]. A secret shipped in
open-source code is not secret, so each user (or their accountant)
registers their own application and types its client ID and secret into
KhataOS. KhataOS sends PKCE (S256) as well.

**Production credentials.** HMRC issues production credentials only after
its approval process: the product must send the fraud-prevention headers
and offer obligations and submit, the sandbox testing must be seen in
HMRC's logs, you email SDSTeam@hmrc.gov.uk within 2 weeks of testing, fill
in 2 questionnaires and accept the terms of use; HMRC answers in about 10
working days [PRIMARY: end-to-end service guide, *Production approvals
process*]. [UNCERTAIN] whether HMRC approves an application registered by a
single business for use with open-source software it did not write, and
whether HMRC will accept the empty `Gov-Vendor-License-IDs` and
`Gov-Client-Multi-Factor` headers (below) without a discussion first. Until
then, use the sandbox.

### Setting it up

1. Create an account on the HMRC Developer Hub
   (<https://developer.service.hmrc.gov.uk>) and **add an application** in
   the sandbox.
2. Subscribe it to **VAT (MTD)** (1.0) and **Test Fraud Prevention
   Headers** (1.0); for testing also **Create Test User**.
3. Add the redirect URI **`http://localhost:48765/callback`** (or the port
   you choose in KhataOS). HMRC accepts `http://localhost:[PORT]` for
   installed applications, allows up to five redirect URIs, and needs the
   one used to match exactly [PRIMARY: *Reference guide*, Redirect URIs;
   *User-restricted endpoints*, OAuth 2.0 for installed applications].
4. Copy the client ID and create a client secret.
5. In KhataOS open **MTD Settings** (`MtdSettingsScreen`; `Ctrl+Alt+M` on
   the obligations screen): environment `sandbox` or `production`, client
   ID, client secret, redirect port. The company needs its **VAT Reg No**
   (Company → Tax registration).
6. On **MTD VAT Obligations** press **`Ctrl+Alt+C` (Connect)**. Your browser
   opens HMRC's sign-in; sign in with the business's **Government Gateway
   (organisation) account**, grant access, and the browser comes back to
   KhataOS ("Connected to HMRC"). KhataOS waits up to 5 minutes.

What KhataOS stores, outside the books and never synced: `<app data>/mtd/settings.json`
(environment, client ID, client secret, port), `tokens.json` (access and
refresh tokens per environment and VRN) and `device-id`. On macOS and
Linux the folder is `0700` and the files `0600`; on Windows they inherit
the per-user permissions of `%APPDATA%`. Tokens and the secret are never
logged, never sent to the window and never shown in error messages.
Changing the environment or client ID forgets the tokens.

### Using it

- **Obligations** (`MtdObligationsReport`): open periods (all of them) and
  periods fulfilled in the chosen range (F2; at most 366 days, HMRC's limit
  for obligations). HMRC's "no data" (`NOT_FOUND`) shows as an empty list.
- **Enter on an open period** opens the 9-box return worked out from the
  books for HMRC's dates (`MtdReturnReport`, the VAT Return screen). Check
  it; the period can't be changed there.
- **`Ctrl+Alt+T` Submit to HMRC** shows the period, the environment and
  HMRC's declaration with a tick box:

  > When you submit this VAT information you are making a legal declaration
  > that the information is true and complete. A false declaration can
  > result in prosecution.

  (HMRC's text for a business [PRIMARY: end-to-end service guide,
  *Obligations*, "Submit a VAT Return with a declaration through
  software"].) Space ticks it, Enter submits, Esc cancels. Without the tick
  nothing is sent (the backend refuses too). KhataOS first checks the period
  key is among HMRC's **open** obligations and takes the dates from HMRC.
  The body is `khata_gb::mtd` with `finalised: true`.
- **The receipt**: HMRC's `processingDate`, `formBundleNumber`,
  `paymentIndicator`, `chargeRefNumber` and the `Receipt-ID`,
  `Receipt-Timestamp` and `X-CorrelationId` response headers, with the body
  sent, the time and who sent it, are saved as a shared record of the
  company (`mtd.receipt.<environment>.<period key>`; synced, not the
  audit trail, which is for book entries) and shown at once
  (`MtdReceiptReport`); the obligations list shows "Sent by KhataOS" for the
  period. If HMRC accepts the return but the receipt can't be saved, the
  error message gives the form bundle number and receipt ID to write down.
- **Enter on a fulfilled period** shows the return HMRC holds (View VAT
  Return; returns from the last four years; a `#` in a period key is sent
  as `%23`).
- **`Ctrl+Alt+L`**: liabilities and payments (at most 365 days).
- **`Ctrl+Alt+H`** (sandbox): HMRC's Test Fraud Prevention Headers API
  verdict for this computer's headers, HMRC's feedback on the latest sandbox
  call to each VAT endpoint, and the headers sent.

Errors are put in plain words, with HMRC's correlation ID for support, for
example `DUPLICATE_SUBMISSION` ("HMRC already has a VAT return for this
period…"), `TAX_PERIOD_NOT_ENDED`, `VRN_INVALID`, `NOT_FINALISED`,
`VAT_TOTAL_VALUE`, `VAT_NET_VALUE`, `CLIENT_OR_AGENT_NOT_AUTHORISED`,
`INVALID_CREDENTIALS`, `RESOURCE_FORBIDDEN`, `MESSAGE_THROTTLED_OUT`
[PRIMARY: VAT (MTD) OAS; *Reference guide*, Errors].

### OAuth, as built

[PRIMARY: *User-restricted endpoints*, *Application-restricted endpoints*]

| Step | KhataOS |
|---|---|
| Authorise | `https://test-www.tax.service.gov.uk/oauth/authorize` (sandbox) or `https://www.tax.service.gov.uk/oauth/authorize`, with `response_type=code`, `client_id`, `scope=read:vat write:vat`, a random `state`, `redirect_uri`, `code_challenge` (S256 of a 43-character verifier) and `code_challenge_method=S256`; no secret |
| Callback | a one-shot listener on `127.0.0.1` and `::1` at the port, opened before the browser; other paths get 404 and a wrong `state` is ignored; `error` / `error_description` are reported |
| Browser | `open` (macOS), `xdg-open` (Linux), `rundll32 url.dll,FileProtocolHandler` (Windows); the app has no opener plugin |
| Exchange | `POST {api}/oauth/token`, form body `client_secret`, `client_id`, `grant_type=authorization_code`, `redirect_uri`, `code`, `code_verifier`; the code lasts 10 minutes |
| Tokens | access token 4 hours (`expires_in` 14400); refreshed a minute before it runs out, or once when HMRC says `INVALID_CREDENTIALS`; refresh tokens are single-use, so refreshes are serialised and the new one saved at once; `invalid_grant` forgets the tokens ("Connect to HMRC again"); a new sign-in is needed 18 months after granting (shown as `reauthorise_by`) |
| Header check | the Test Fraud Prevention Headers API is application-restricted: a `client_credentials` token, kept in memory for the one call |

Base URLs: `https://test-api.service.hmrc.gov.uk` (sandbox) and
`https://api.service.hmrc.gov.uk`; every call sends
`Accept: application/vnd.hmrc.1.0+json` [PRIMARY: OAS `servers`;
*Reference guide*]. [UNCERTAIN] Fraud-prevention headers are sent on every
VAT API call and the header check, not on `/oauth/token` (HMRC's token
examples show none).

### Fraud-prevention headers

Required by law for the VAT (MTD) API; specification **3.3, 27 January
2025** [PRIMARY: *Send fraud prevention data*; *Desktop application direct*;
*Getting it right*; change log]. Formatting is `khata_gb::fraud` (pure,
tested against HMRC's examples); gathering is `mtd_client.rs`. Values are
US-ASCII: keys and values percent-encoded (RFC 3986 unreserved characters
kept), separators (`=`, `&`, `,`) not.

| Header | KhataOS sends |
|---|---|
| `Gov-Client-Connection-Method` | `DESKTOP_APP_DIRECT` |
| `Gov-Client-Device-ID` | a UUID (v4) made on first use and kept in `<app data>/mtd/device-id`; remade if deleted, as HMRC asks |
| `Gov-Client-User-IDs` | `os=<OS user>` (`DOMAIN\name` on Windows, the login name elsewhere) and, when someone is signed in to the company, `khataos=<KhataOS user name>` |
| `Gov-Client-Timezone` | `UTC±hh:mm` from the computer's current offset |
| `Gov-Client-Local-IPs` | every interface address (via `local-ip-address`), read immediately before each request; IPv6 percent-encoded. Loopback addresses are left out (KhataOS's choice [UNCERTAIN]) |
| `Gov-Client-Local-IPs-Timestamp` | when they were read, `yyyy-MM-ddThh:mm:ss.sssZ` |
| `Gov-Client-MAC-Addresses` | every interface's MAC (via `mac_address`), lower-case `aa:bb:…`, each encoded; all-zero ones left out |
| `Gov-Client-Screens` | every monitor Tauri reports: width and height in logical pixels (physical ÷ scale, as a browser's `window.screen` gives them), `scaling-factor` (to 4 decimals, no trailing zeros), `colour-depth` from `screen.colorDepth`; falls back to `window.screen`. [UNCERTAIN] physical or logical pixels: HMRC says "reported width of the screen" |
| `Gov-Client-Window-Size` | the window's inner size (`innerWidth` × `innerHeight`) |
| `Gov-Client-User-Agent` | `os-family` (`MacOS`, `Windows`, `Linux`), `os-version` (`sw_vers`, `ver`, `/etc/os-release`), `device-manufacturer` and `device-model` (`Apple` + `sysctl hw.model`; the BIOS registry keys; `/sys/class/dmi/id`); a field the OS doesn't give is left out, which the key-value rules allow |
| `Gov-Client-Multi-Factor` | **empty**: KhataOS company users sign in with a password only. HMRC: "If you authenticate with username and password only, you will not be able to collect a value" |
| `Gov-Vendor-Version` | `KhataOS=<app version>` |
| `Gov-Vendor-License-IDs` | **empty**: KhataOS is free software with no licence keys. HMRC: "If there are no licenses on the originating device, you will not be able to collect a value" |
| `Gov-Vendor-Product-Name` | `KhataOS` |

For a value that can't be collected HMRC says: "you must contact us to
explain why… After discussing a missing header with us, you can omit the
header or submit it with an empty value. You must not include a
placeholder value" (*Getting it right*). KhataOS sends the two headers with
an empty value, never a placeholder; whoever applies for production
credentials must explain this to HMRC (softwaredevelopersupport@service.hmrc.gov.uk).
The validator's `EMPTY_HEADER` code ("header required for the connection
method is empty") will flag them [PRIMARY: Test Fraud Prevention Headers
OAS]. If the screens or window size are missing or zero, nothing is sent.

### Testing in the sandbox

1. Set up a sandbox application as above (with Create Test User).
2. Create a test **organisation** with the Create Test User API (or the
   Developer Hub's test-user page): HMRC gives a user ID, password and a
   test **VRN** [PRIMARY: end-to-end service guide, Testing requirements].
   Enter that VRN as the company's VAT Reg No. [UNCERTAIN] whether test
   VRNs pass the Mod 97 / 97-55 check KhataOS applies; if one doesn't,
   KhataOS refuses it and you need another test user.
3. Connect, signing in as the test user (the sandbox has no 2-step
   verification).
4. `Ctrl+Alt+H` to check the headers; fix errors, review advisories.
5. Load the obligations (the sandbox gives canned periods, e.g. 2017–2018,
   whatever the books hold), open an open period and submit. In the
   sandbox "there is no requirement to use the periodKeys in the obligation
   response" and the data submitted is stored, so View VAT Return shows it.
6. Developers can pass a sandbox `Gov-Test-Scenario` through the
   `test_scenario` argument of `mtd_obligations`, `mtd_submit`,
   `mtd_liabilities` and `mtd_payments` (for example
   `DUPLICATE_SUBMISSION`, `TAX_PERIOD_NOT_ENDED`, `INVALID_VRN`,
   `QUARTERLY_NONE_MET`, `MULTIPLE_LIABILITIES`); it is never sent in
   production. The screens don't offer it.
7. `Ctrl+Alt+H` again shows HMRC's feedback on the latest call to each VAT
   endpoint.

### What was tested

**Not against HMRC.** No sandbox application could be registered for this
work, so nothing has been sent to HMRC's sandbox or production. Tested:

- `khata_gb::fraud`: every header format against the examples on HMRC's
  *Desktop application direct* page (local IPs, timestamp, MAC addresses,
  multi-factor, screens, window size, timezone, user agent, user IDs,
  vendor version, product name), empty-value and refusal rules.
- `mtd_client.rs` against a **local mock HMRC** (axum): the full OAuth
  flow through the localhost listener (authorise URL parameters, PKCE
  verifier checked against the challenge, wrong `state` and other paths
  ignored, refused sign-in, time-out), private file permissions, redacted
  `Debug`, token refresh before expiry and after `INVALID_CREDENTIALS`,
  single-use refresh tokens, `invalid_grant` forgetting the tokens; every
  fraud-prevention header present and formatted on each call;
  `Gov-Test-Scenario` only in the sandbox; obligations (open, fulfilled,
  `NOT_FOUND`); submit from a UK company's books (declaration required, HMRC's
  open period and dates, the exact body with `finalised: true`, receipt
  headers and body recorded in the company file and shown on the
  obligation, `DUPLICATE_SUBMISSION`, `TAX_PERIOD_NOT_ENDED`, a fulfilled
  period refused); View VAT Return with a `#` period key; liabilities and
  payments; the header validator with a `client_credentials` token; error
  messages (including HMRC's `errors` array and the correlation ID).

The mock follows HMRC's documented requests and responses; HMRC's real
behaviour may differ in ways the documents don't show.

## Assumptions and simplifications

- **Standard accounting** by invoice date unless a scheme is set (see
  [VAT schemes](#vat-schemes-flat-rate-cash-accounting-annual-accounting)).
- **Box 4 takes all VAT on purchases as reclaimable**. Not applied: blocked
  input tax (business entertainment, cars), partial exemption, the capital
  goods scheme. A purchase from a supplier without a VAT number keeps its
  value in Box 7, but its VAT is left out of Box 4 with a warning.
- **Line treatment** comes from each stock item's (or ledger's) class and
  the rate in force; lines without a class or rate are taken as
  standard-rated, with a warning when the voucher has no VAT. A voucher's
  VAT is spread over its taxed lines by value; a warning shows when it
  differs from the rates on file by more than £1.
- **Northern Ireland**: the company is in NI when its nation is `NIR` or its
  VAT number starts `XI`; a party is in the EU by its mailing country. Only
  stock-item lines count as goods for Boxes 8 and 9; ledger lines are
  services. Acquisition VAT (Box 2 and its reclaim) is not posted or worked
  out.
- Journals and other voucher types are left out even when they post to VAT
  ledgers (a warning says so), except postings to VAT ledgers classed
  `IMPORT` or `IMPORT:PVA`. Cancelled vouchers are left out.
- Corrections of earlier periods (Notice 700/45: up to £10,000, or 1% of
  Box 6 up to £50,000, on the next return) are not modelled: the return is
  only the period's documents.

## Not supported yet

- MTD: the VAT customer information, penalties and financial-details
  endpoints; HMRC Assist for VAT (from April 2027); agents (the agent
  declaration and client lists); `DESKTOP_APP_VIA_SERVER`.
- Margin schemes, bad-debt relief, the retailer's flat rate turnover
  method (Notice 733 §10), and the Annual Accounting Scheme's interim payment
  schedule.
- Simplified (≤ £250 VAT-inclusive) and modified VAT invoices, the 30-day
  issue rule, the £90,000 registration-threshold monitor.
- Companies House accounts (FRS 102 / FRS 105, iXBRL filing from 1 April
  2028), Corporation Tax (CT600) and payroll.
- Menu entries for the VAT Return screen and the MTD screens (`MtdObligationsReport`, `MtdSettingsScreen` in `screens/mtd.ts`; the screens exist).

## What to verify before relying on it

From `docs/research/uk-compliance.md` §4:
- the Mod 97 / 97-55 specification against current HMRC material (this
  follows HMRC's 2009 forum minutes, slides and worked example);
- the 12-digit branch and GD/HA formats (secondary sources only);
- whole pounds for Boxes 6–9: truncation (here) or rounding;
- Notice 700/21 §4.2 still says `GB` for NI → EU invoices, while VAT
  Regulations reg 14(2)(b) says `XI`;
- the half-penny direction when rounding line VAT;
- the MTD items marked [UNCERTAIN] in
  [Submitting to HMRC](#submitting-to-hmrc-making-tax-digital), and a live
  sandbox run (not done: it needs a registered HMRC application).

## Sources

See `docs/research/uk-compliance.md` §5. The main ones:
- VAT Notice 700 (VAT guide): <https://www.gov.uk/guidance/vat-guide-notice-700>
- VAT Notice 700/12 (filling in the return): <https://www.gov.uk/guidance/how-to-fill-in-and-submit-your-vat-return-vat-notice-70012>
- VAT Notice 700/21 (records): <https://www.gov.uk/guidance/record-keeping-for-vat-notice-70021>
- VAT Notice 700/22 (MTD): <https://www.gov.uk/government/publications/vat-notice-70022-making-tax-digital-for-vat/vat-notice-70022-making-tax-digital-for-vat>
- VAT Regulations 1995 reg 14: <https://www.legislation.gov.uk/uksi/1995/2518/regulation/14>
- HMRC VAT (MTD) API: <https://developer.service.hmrc.gov.uk/api-documentation/docs/api/service/vat-api/1.0> (OpenAPI: `…/1.0/oas/file`)
- HMRC VAT (MTD) end-to-end service guide: <https://developer.service.hmrc.gov.uk/guides/vat-mtd-end-to-end-service-guide/> (declaration text: `…/documentation/obligations.html`)
- HMRC OAuth: <https://developer.service.hmrc.gov.uk/api-documentation/docs/authorisation/user-restricted-endpoints>, <https://developer.service.hmrc.gov.uk/api-documentation/docs/authorisation/application-restricted-endpoints>; reference guide (redirect URIs, errors): <https://developer.service.hmrc.gov.uk/api-documentation/docs/reference-guide>
- HMRC fraud prevention (v3.3, 27 Jan 2025): <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/>, <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/connection-method/desktop-app-direct/>, <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/getting-it-right/>, change log <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/change-log/>
- HMRC Test Fraud Prevention Headers API 1.0: <https://developer.service.hmrc.gov.uk/api-documentation/docs/api/service/txm-fph-validator-api/1.0>
- HMRC VAT Software Developer Forum, 10 Mar 2009 (archived minutes and slides): <http://web.archive.org/web/20121012000313/http://www.hmrc.gov.uk/ebu/dev-forum-mins100309.pdf>

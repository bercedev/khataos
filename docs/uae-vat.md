# UAE VAT pack (`khata-ae`)

KhataOS can keep books for a company in the United Arab Emirates: it checks
TRNs, charges 5% VAT on invoices, checks tax invoice numbers and works out
VAT 201 figures from the books. The VAT 201 figures are a preparation aid.
They are not a filing, and nothing here is legal advice: check them with a
UAE-registered tax agent before you file on EmaraTax.

Code: `crates/khata-ae` (the rules: `trn.rs`, `emirates.rs`, `vat.rs`,
`returns.rs`), `apps/desktop/src-tauri/src/vat_ae.rs` (vouchers to VAT 201
documents, the `vat201(from, to)` command) and `apps/desktop/src/screens/vat.ts`
(the VAT 201 screen).
Sources and open questions are in `docs/research/mena-compliance.md` §1
(VAT), §8 (pack design) and §9 (open items). Tags below come from that file:
**[PRIMARY]** confirmed against the official text, **[SECONDARY]** reported
by professional sources, **[UNCERTAIN]** not confirmed.

## Setting up a UAE company

1. Pick **United Arab Emirates** as the country. The books use AED (2
   decimals, printed "AED") and calendar years (January to December).
2. In **Company → Company details** (Alt+K), enter the company's TRN and pick its
   **emirate**. A TRN does not encode an emirate, so the emirate is always
   chosen by hand. It decides the VAT 201 Box 1 row.
3. Add the rate list. Suggested classes (also in `khata_ae::vat::default_rates`):

   | Class | Rate | From | Meaning |
   |---|---|---|---|
   | `STANDARD` | 5 | 2018-01-01 | Standard rate (VAT Law Art 3) [SECONDARY] |
   | `ZERO` | 0 | 2018-01-01 | Zero-rated supplies (Art 45): exports, international transport, qualifying education and healthcare… |
   | `EXEMPT` | 0 | 2018-01-01 | Exempt supplies (Art 46): certain financial services, residential leases, bare land, local passenger transport |

   Give each stock item one of these classes (or an HS code with its own
   rate). **Exempt and zero-rated are told apart by name only**: a class
   whose name starts with `EXEMPT` is exempt, any other 0% class is
   zero-rated.
4. Create **Output VAT** and **Input VAT** ledgers under Duties & Taxes and
   set their type of tax to `VAT`.

The company's financial year can still be changed per company. UAE law sets
no single year: VAT periods are quarterly or monthly as the FTA assigns them
(§1.4 [SECONDARY]), and a Corporate Tax period is the company's own
financial year (FDL 47/2022 Art 57, §2 [SECONDARY]).

## What is checked and computed

**TRN.** 15 digits once spaces and hyphens are removed. The first 10 digits
are the TIN (MoF *UAE Electronic Invoicing Guidelines* v1.1 §2, §3
[PRIMARY]), shown as the entity ID. **No checksum is checked**: the FTA has
not published one (§1.1, §9 item 14 [UNCERTAIN]), so any 15 digits pass and
only an online EmaraTax lookup confirms a registration. `trn::usual_prefix`
tells whether a TRN starts with "100", as most do; that is a hint for a
warning, never a rule [SECONDARY].

**Emirates.** KhataOS stores its own stable codes, in VAT 201 order:

| Code | Emirate | ISO 3166-2 | VAT 201 box |
|---|---|---|---|
| AUH | Abu Dhabi | AE-AZ | 1a |
| DXB | Dubai | AE-DU | 1b |
| SHJ | Sharjah | AE-SH | 1c |
| AJM | Ajman | AE-AJ | 1d |
| UAQ | Umm Al Quwain | AE-UQ | 1e |
| RAK | Ras Al Khaimah | AE-RK | 1f |
| FUJ | Fujairah | AE-FU | 1g |

**VAT on an invoice line.** One `VAT` component at the class's rate on the
invoice date, from the company's rate list (rates are never hard-coded).
Each line's VAT is rounded to the fil, half away from zero.
- The emirates of supplier and customer must be known codes; they never
  change the rate.
- An **export** (place of supply abroad) is zero-rated (Art 45) whatever the
  class. The export conditions (evidence, time limits) are not checked.
- A customer in a **designated zone** is taxed like any other UAE customer.
  Goods supplied within or between designated zones can be outside the
  scope of VAT when the conditions of Art 51 and ER Arts 50 and 51 are met
  (§1.2 [SECONDARY]), but that depends on facts KhataOS doesn't hold (goods
  or services, the supplier's zone, the official list of zones). A supply
  marked both "special zone" and "without payment" is the user's statement
  that it is out of scope; it then carries no VAT. Any other supply marked
  "without payment" is refused.

**Tax invoice numbers** (ER Art 59(1)(d) asks for a sequential or unique
number that identifies the invoice and its order, §1.3 [SECONDARY]):
- not empty;
- at most **35 characters**. No UAE limit is published; 35 is a KhataOS
  choice, the length of a document number in UN/EDIFACT (data element 1004),
  which keeps the number usable by trading partners' systems and on one
  printed line;
- only **letters, digits, `-` and `/`**. Also a KhataOS choice: no spaces
  ("INV 1" and "INV1" would look like two numbers), and the same set as
  India, which every system in an e-invoice chain can carry;
- **unique across all years** in the series, ignoring case. The UAE rule has
  no yearly reset (unlike India's Rule 46). KhataOS's default numbering puts
  the year in the number ("2026/1"), so a new year's series stays unique.

**VAT 201** (`returns::vat201`), for any period, from Sales and Credit Notes
(outward) and Purchases and Debit Notes (inward):

| Box | What goes in |
|---|---|
| 1a–1g | Standard-rated supplies and their VAT, in the row of the company's emirate; credit notes reduce it |
| 2 | Tourist refunds: **not supported, always 0** |
| 3 | Supplies subject to the reverse charge: purchase lines with an `RC:` class (see [Reverse charge and imports](#reverse-charge-and-imports-boxes-3-6-7-and-10)) |
| 4 | Zero-rated supplies (amount) |
| 5 | Exempt supplies (amount) |
| 6, 7 | Goods imported through UAE Customs (purchase lines with an `IMPORT:` class), and supplier credit and debit notes on them |
| 8 | Totals of Boxes 1 to 7 |
| 9 | Standard-rated expenses and their VAT, from suppliers with a TRN; purchase returns reduce it |
| 10 | Reverse charge recovery: the value and VAT of Boxes 3, 6 and 7, all taken as recoverable |
| 11 | Totals of Boxes 9 and 10 |
| 12, 13, 14 | VAT due (Box 8), recoverable (Box 11), and the difference: positive is payable, negative recoverable |

## Assumptions and simplifications

- **One establishment per company.** Every standard-rated supply goes in
  the Box 1 row of the company's emirate. The FTA asks for the split by the
  emirate of the establishment that makes the supply (§1.4 [SECONDARY]); a
  business with branches in several emirates needs one company per branch
  for now, or must split Box 1 by hand.
- **Line treatment** comes from each stock item's class and the rate in
  force: `EXEMPT…` exempt, other 0% classes zero-rated, else standard. Lines
  without a class or a rate on file (plain ledger lines) are taken as
  **standard-rated**, with a warning when the voucher has no VAT. A sale
  charged no VAT to a party whose mailing country is not the UAE is an
  export, zero-rated in Box 4.
- A voucher's VAT (from ledgers whose type of tax is VAT) is spread over its
  standard-rated lines by value. A warning shows when it differs from the
  rates on file by more than AED 1, or when VAT sits on a voucher with only
  zero-rated or exempt lines (it is then reported as standard-rated).
- **Box 9 takes all VAT on standard-rated purchases as recoverable.** Not
  applied: input tax the law blocks (the blocked categories are not in the
  research, so verify them; CD 149/2026 changes the employee-benefit
  limits), the input-tax block on cash-paid supplies above a
  threshold still to be set (CD 149/2026, §1.2 [SECONDARY]), input-tax
  apportionment for businesses with exempt supplies, and the 5-year lapse of
  excess recoverable tax (FDL 16/2025). Purchases from a supplier without a
  TRN are left out, with a warning when they carry VAT.
- The "adjustment" column of Boxes 1 and 9 (bad-debt relief and the like)
  is not modelled.
- Journals and other voucher types are left out even when they post to VAT
  ledgers (a warning says so). Cancelled vouchers are left out.
- **Rounding**: each line's VAT is rounded to the fil, half away from zero,
  because the tax invoice shows VAT per line (ER Art 59(1)(h)) and the books
  post it. The MoF e-invoicing guidelines round only at the invoice total
  (§12.2 [PRIMARY]); `vat::document_vat` gives that figure for a future
  e-invoice. The two can differ by a few fils on a long invoice. The
  rounding direction is not in the research [UNCERTAIN].

## Reverse charge and imports (Boxes 3, 6, 7 and 10)

**How a line is marked (no core change).** The core model has no field for
the reverse charge, so it rides on the **tax class**, the same convention
in the UAE, UK, Singapore and Saudi packs (`khata_ae::vat::Mechanism`):

| Class | Meaning | VAT 201 |
|---|---|---|
| `RC:STANDARD` (or `RC:` + any rate class; bare `RC` = STANDARD) | Reverse charge: services and goods from abroad not imported through UAE Customs, and domestic reverse-charge goods (electronic devices, precious metals and stones, hydrocarbons, metal scrap) | Purchase: Box 3 (value and VAT) and Box 10. Sale: its value in the Box 1 row, no VAT |
| `IMPORT:STANDARD` | Goods imported through UAE Customs by a registered importer (VAT accounted for on the return, ER Art 48(1)) | Box 6 (invoice), Box 7 (supplier credit and debit notes), and Box 10 |

Put the class on the stock item, or on the service ledger for an
accounting invoice (`RC:STANDARD` on a "Cloud services" ledger, say). The
rest of the class is the rate class: the VAT is worked out at its rate on
the voucher date (the supplier charged none), so `STANDARD` must be in the
company's rate list. The invoice itself carries no VAT for such a line
(`vat::line_tax` gives no component).

**In the books**, self-account the VAT on the purchase: Dr the expense,
Dr input VAT, Cr the supplier, Cr output VAT. The return works the VAT out
from the rate anyway; a warning compares it with what the voucher posts to
output VAT ledgers, so a missing posting shows. No supplier TRN is needed
for these lines. A purchase from a party whose mailing country is not AE,
with no VAT and nothing marked, gets a reminder to mark it.

What goes where follows the FTA *VAT Returns User Guide* (v4.0, Aug 2021)
[PRIMARY]:
- Box 3 takes standard-rated **and zero-rated** services from abroad,
  goods under the reverse charge not declared to customs and domestic
  reverse-charge supplies (pp.15-16). Exempt lines are not reverse-charged.
- Box 6 is **pre-filled by the FTA** from the customs declarations linked
  to the TRN (with duty and excise in the value, VAT at 5%, p.17). The
  figure worked out here is for comparison; differences belong in Box 7
  (p.17-18), which KhataOS fills only from supplier credit and debit notes
  on import lines. Zero-rated import lines are left out (the FTA's 5% on
  them is corrected in Box 7 by hand).
- Box 10 holds the full value and the **recoverable** VAT of Boxes 3, 6
  and 7 (pp.22-23). KhataOS takes all of it as recoverable (a warning
  says so): reduce it for blocked input tax and apportionment.
- The domestic reverse charge: VATP034 says the recipient of electronic
  devices declares the tax in Box 3 [PRIMARY]; precious metals (Cabinet
  Decision 127/2024, which repealed CD 25/2018, from 15 Feb 2025) and metal
  scrap (CD 153/2025) are put there by analogy [UNCERTAIN]. Who must apply
  it (the written declarations, resale purpose) is the user's call. The
  **supplier** side (value in its Box 1 row, no VAT) is [UNCERTAIN]: no
  primary source in the research says which box the supplier uses.

**Designated zones** (notes only; nothing is automatic). A designated zone
meeting the Executive Regulation's conditions is treated as outside the
State (Decree-Law Art 50); goods move between zones untaxed (Art 51);
goods supplied for consumption in a zone, services, water and energy are
inside the State (ER Art 51). Mark an out-of-scope zone supply with
`special_zone` and `without_payment` (as before); goods consumed in a zone
go in Box 1 as usual.

## Not supported yet

The profit margin scheme
(Art 43), out-of-scope supplies, VAT paid at customs by importers who do
not meet ER Art 48 (its box is not in the guide [UNCERTAIN]), tourist refunds,
simplified tax invoices and the AED 10,000 limit, the 14-day invoice
deadline, the CBUAE rate feed for foreign-currency invoices (they can be
entered with a typed rate, but PINT AE refuses them), the registration
threshold monitor (AED 375,000 / 187,500), e-invoicing beyond the PINT AE
export below (ASP status tracking, the 14-day deadline), the FTA Audit File, Corporate Tax and
a split of Box 10 into recoverable and blocked parts.

## E-invoicing (PINT AE)

The UAE Electronic Invoicing System uses **PINT AE** (UBL 2.1, v1.0.4,
`urn:peppol:pint:billing-1@ae-1`) through Accredited Service Providers
(ASPs) in a 5-corner model; KhataOS is corner 1 software. Timeline
(research §3, `docs/research/einvoicing-peppol.md`): pilot and voluntary
use from 1 Jul 2026; revenue ≥ AED 50m: ASP by 30 Oct 2026 (extended in
May 2026), live 1 Jan 2027; < AED 50m: ASP by 31 Mar 2027, live 1 Jul 2027
[SECONDARY for the smaller-business dates]; B2C is out of scope for now.

`pint_ae_xml` (export action `exportPintAe`) writes the PINT AE UBL for a
Sales or Credit Note voucher with the PINT AE rules it breaks:
- categories from the class: standard → S (5%), zero-rated → Z, "EXEMPT
  DL8.46.n" → E with that exemption code, "OUT-OF-SCOPE…" → O, "RC:…" → AE
  (refused for now: it needs the goods type and a GTIN); a sale with no VAT
  to a party abroad is an export (Z, transaction type `00000001`);
- seller: TRN, `0235:<TIN>` endpoint, the company's emirate, and the trade
  licence from "Trade licence: …" and "Licensing authority: …" lines in the
  company address; IBAN from an "IBAN: …" line;
- buyer: TRN → `0235:<TIN>`, abroad → `0235:9900000099`, no TRN →
  `0235:9900000098` (issue a paper/PDF invoice too); emirate from the
  mailing region or city;
- credit notes: reason code from "Reason: DL8.61.1.x" in the narration
  (default DL8.61.1.D, goods returned) and the original invoice from the
  "Against" bill;
- the VAT is worked out at the invoice total, per the MoF guidelines (the
  books round per line; a difference is shown as a warning).

`peppol_ap_send` can POST the document to an ASP endpoint with an API key
(generic; every ASP's API differs and many expect their own format). Not
supported: foreign currency (BTAE-04), reverse charge, margin scheme, free
zone, deemed supplies, summary/continuous supply, agents, e-commerce, the
out-of-scope invoice type 480, self-billing, ASP status and FTA
confirmations. See [`docs/peppol.md`](peppol.md).

## What to verify before relying on it

From `docs/research/mena-compliance.md` §9:
- the current VAT 201 box labels on EmaraTax (item 6), and whether Box 1 is
  split by the supplier's establishment as assumed here;
- the consolidated Executive Regulation text after CD 100/2024, CD 100/2025
  and CD 149/2026, for the exact Art 59 and 60 wording (item 3);
- whether the FTA or PINT AE sets a length or character limit on invoice
  numbers (KhataOS's 35 characters of `[A-Za-z0-9/-]` is its own choice);
- that no TRN checksum has been published (item 14);
- the rounding rule for VAT on tax invoices, as opposed to e-invoices.

## Sources

From `docs/research/mena-compliance.md` §10:
- MoF *UAE Electronic Invoicing Guidelines* v1.1:
  <https://mof.gov.ae/wp-content/uploads/2026/02/UAE-Electronic-Invoicing-Guidelines_V-1.0-23Feb2026.pdf>
- Tax invoice particulars (ER Art 59): <https://mondaq.com/sales-taxes-vat-gst/670546/tax-invoice-under-uae-vat-law>
- VAT 201: <https://www.businessdubai.ae/blogs/uae-vat-return-filing>
- TRN: <https://lookuptax.com/docs/validate/united-arab-emirates/trn>; <https://bcl.ae/blogs/trn-verification-uae/>
- FDL 16/2025 and 17/2025 (KPMG): <https://kpmg.com/ae/en/insights/tax-insights/federal-decree-law-no-16-and-17-of-2025.html>
- Cabinet Decision 149/2026 (KPMG): <https://kpmg.com/ae/en/insights/tax-insights/Cabinet-Decision-149-of-2026-aending-certain-provisions-of-the-vat-executive-regulation.html>
- Cabinet Decision 100/2025 (Crowe): <https://www.crowe.com/ae/news/what-is-the-recent-amendment-to-uae-vat>

# Singapore GST pack (`khata-sg`)

KhataOS can keep books for a business in Singapore. It checks GST
registration numbers, charges GST at the rate in force on the invoice date
(9% from 1 Jan 2024), checks tax invoice numbers and works out GST F5
figures from the books. The GST F5 figures are a preparation aid, not a
filing, and nothing here is tax advice. Check them, ideally with a
Singapore tax agent, before you file on myTax Portal.

Code: `crates/khata-sg` (the rules: `gst_no.rs`, `gst.rs`, `returns.rs`),
`apps/desktop/src-tauri/src/gst_sg.rs` (vouchers to GST F5 documents, the
`gst_f5(from, to)` command), and `apps/desktop/src/screens/gst_sg.ts` (the
`GstF5Report` screen, in Go To (Alt+G) as **GST F5 (Singapore GST return)**
for Singapore companies). Sources and
open questions are in `docs/research/singapore-compliance.md`. Tags:
**[PRIMARY]** read on an official IRAS/ACRA/government page,
**[SECONDARY]** professional sources only, **[UNCERTAIN]** not confirmed.

## Setting up a Singapore company

1. Pick **Singapore** as the country. The books use SGD (2 decimals,
   printed "S$") and calendar years (January to December). A company
   chooses its own financial year end, and the first year may run up to 18
   months (§6 [SECONDARY]), so change the books' first month if yours
   differs. GST periods are usually calendar quarters, whatever the FYE.
2. In **Company → Company details** (Alt+K), enter the **GST Reg No**. Singapore
   has **no tax regions**, so leave the State/region field empty. Invoices
   never ask for a region: the country code `SG` stands in on both sides of
   every supply.
3. Add the rate list. The suggested classes are below, and also in
   `khata_sg::gst::default_rates`:

   | Class | Rate | From | Meaning |
   |---|---|---|---|
   | `STANDARD` | 3 / 4 / 5 / 7 / 8 / **9** | 1994-04-01 / 2003-01-01 / 2004-01-01 / 2007-07-01 / 2023-01-01 / **2024-01-01** | Standard rate history (IRAS rate table [PRIMARY]) |
   | `ZERO` | 0 | 1994-04-01 | Zero-rated: exports of goods, international services under s21(3) |
   | `EXEMPT` | 0 | 1994-04-01 | Exempt: financial services, sale/lease of residential property, investment precious metals, digital payment tokens |
   | `OUT-OF-SCOPE` | 0 | 1994-04-01 | Out of scope: overseas-to-overseas goods, overseas goods sold in an FTZ / Zero GST Warehouse, private transactions |

   You only need the rates for the years you keep books for (for example
   `STANDARD` 8 from 2023-01-01 and 9 from 2024-01-01). **Zero-rated, exempt
   and out-of-scope are told apart by name only.** A class starting with
   `EXEMPT` is exempt, one starting with `OUT-OF-SCOPE` (or `OUT OF SCOPE`)
   is out of scope, and any other 0% class is zero-rated.
4. Create **Output GST** and **Input GST** ledgers under Duties & Taxes,
   and set their type of tax to `GST`.

## What is checked and computed

**GST registration number** (`gst_no.rs`). After spaces and hyphens are
removed and letters are upper-cased, one of:

| Shape | Example | Who | Source |
|---|---|---|---|
| `nnnnnnnnX` (9) | `12345678A` | UEN of a business registered with ACRA | Annex E UEN format [PRIMARY] |
| `yyyynnnnnX` (10) | `199012345X` | UEN of a local company | Annex E [PRIMARY] |
| `TyyPQnnnnX` (10) | `T08LL1234A` | UEN of any other entity (LLP, foreign company, society…); `R`/`S` for 18xx/19xx years | Annex E [PRIMARY]; R/S [SECONDARY] |
| `M` + 9 (10) | `M2-1234567-K` | A number issued by IRAS when it differs from the UEN | IRAS *Invoicing customers* [PRIMARY example] |

**No check letter is validated.** The UEN format names a "check alphabet",
but no algorithm is published. IRAS gives only an example for `M` numbers,
so KhataOS accepts `M`, a letter or digit, 7 digits and a letter or digit:
a lenient superset that won't refuse an older real number [UNCERTAIN].
Entity-type codes (`PQ`) are not checked against ACRA's list. A UEN is
shown as the entity ID; an `M` number has none. Only the IRAS "GST
registered business search" confirms a registration.

**GST on an invoice line** (`gst.rs`). One `GST` component at the class's
rate on the invoice date, from the company's rate list (rates are never
hard-coded). Each line is rounded to the cent, half away from zero.
- IRAS lets you round "the total GST … to the nearest cent" [PRIMARY]. The
  half-cent direction is not stated [UNCERTAIN]. `document_gst` rounds once
  per invoice for e-invoicing; the books round per line, so a long invoice
  can differ by a few cents.
- An **export** (place of supply abroad) is zero-rated whatever the class.
  The export evidence and the s21(3) conditions are not checked. Note that
  the invoice screen does not yet mark supplies abroad, so give export lines
  the `ZERO` class. The GST F5 also treats an untaxed sale to a party whose
  mailing country is not SG as zero-rated.
- An `OUT-OF-SCOPE` line carries no GST component at all. A supply to a
  special zone marked "without payment" is the user's statement that it is
  out of scope (FTZ). Any other local supply marked "without payment" is
  refused.

**Invoice numbers.** IRAS asks only for "an invoice identifying number"
[PRIMARY]. KhataOS keeps the UAE pack's rule: not empty, at most 35
characters of letters, digits, `-` and `/`, and **unique across all
years** (no yearly reset). This is a KhataOS choice that keeps numbers safe
for InvoiceNow.

**GST F5** (`returns.rs`, desktop `gst_f5`). From Sales and Credit Notes
(outward), and Purchases and Debit Notes (inward), in the period:

| Box | Worked out as |
|---|---|
| 1 | Standard-rated value, net of credit notes, plus reverse-charge purchases |
| 2 | Zero-rated value (0% classes, and untaxed sales to parties abroad) |
| 3 | Exempt value |
| 4 | 1 + 2 + 3 (out-of-scope supplies are in no box; shown for reference) |
| 5 | Standard- and zero-rated purchases from suppliers **with a GST Reg No**, net of purchase returns, plus reverse-charge purchases and imports (no supplier number needed) |
| 6 | GST on Box 1, including the reverse-charge GST |
| 7 | GST on Box 5, all taken as claimable, including reverse-charge GST and import GST |
| 8 | 6 − 7. A warning appears when it is under S$5, which IRAS neither collects nor refunds |
| 9–12, 15–17 | Not modelled: shown as 0 / "No" |
| 14 | "Yes" with the value of reverse-charge purchases (also in Boxes 1 and 5) |
| 13 | **Estimate:** the net value of all Sales and Credit Notes. IRAS accepts a best estimate, but replace it with revenue from your accounts |
| 18–21 (IGDS) | Not shown |

Warnings list purchases with GST but no supplier GST number, GST on zero-rated
or exempt lines, GST that does not match the rates on file, lines without a
class, and journals that post to tax ledgers.

## Reverse charge and imports

**How a line is marked (no core change):** on the **tax class**, as in the
UAE, UK and Saudi packs (`khata_sg::gst::Mechanism`). The rest of the class
is the rate class; the invoice carries no GST for such a line.

- `RC:STANDARD` on the stock item or service ledger: **imported services
  (from 1 Jan 2020) and low-value goods (from 1 Jan 2023) under the reverse
  charge**. Only an **RC business** applies it: a GST-registered person not
  entitled to full input tax credit (it makes exempt or non-business
  supplies beyond the de minimis rule), or in a GST group with such a
  member (e-Tax Guide "GST: Reverse Charge", 10th ed., 30 Jan 2026,
  §§4.1.2-4.1.5 [PRIMARY]). Low-value goods: not dutiable, outside
  Singapore when sold, delivered by air or post, **S$400 or less per item**
  (§4.3.1). The value goes in **Boxes 1, 5 and 14**, the GST worked out at
  the rate class in **Boxes 6 and 7** (§11.1.1 [PRIMARY]). Box 7 is
  claimed in full; a warning reminds that the normal recovery rules
  (apportionment) apply, since an RC business is by definition not fully
  taxable. Self-account in the books (Dr expense, Dr input GST, Cr
  supplier, Cr output GST); a warning compares what the voucher posts to
  output GST. A fully taxable business does not use the reverse charge
  and need not complete Box 14.
- `IMPORT:STANDARD` on the stock item: **goods imported with GST paid to
  Singapore Customs**. The line's value goes in **Box 5** (it should match
  the import permit's value: CIF plus duty and other charges, IRAS
  "Importing of goods" [PRIMARY]; whether Box 5 takes the permit value or
  the CIF value is [UNCERTAIN]). The permit's GST is posted to a GST ledger
  classed `IMPORT` on any voucher (the Payment to the forwarder, say): its
  debits go in **Box 7**. Neither needs a supplier GST number. A warning
  shows when imports are in Box 5 with no import GST posted.
- A purchase from a party abroad with no GST and nothing marked gets a
  reminder.

Pointers only (not modelled; IRAS e-Tax Guides [PRIMARY]): **MES / A3PL**
imports (GST suspended): the permit value in **Box 5 and Box 9**, no GST in
Box 7 (MES guide, 18th ed., App. B). **IGDS**: the deferred import GST in
**Box 19** and the value in **Box 21** (not Box 6), and, when claimable,
the value in Box 5 and the GST in Box 7; monthly filing (IGDS guide, 11th
ed., App. D). The labels of Boxes 18 and 20 are [UNCERTAIN]. Reverse-charge
refund claims for unpaid overseas suppliers (Box 11) are not modelled.

## What is not modelled (check by hand)

- OVR and customer accounting (Boxes 15–17); MES/A3PL (Box 9), IGDS
  (18–21), as above.
- Input tax disallowance (certain motor car and staff medical costs),
  private use, partial-exemption apportionment, the Gross Margin Scheme.
- Bad debt relief, tourist refunds, pre-registration claims (Boxes 10–12).
- Net realised exchange gains/losses in Box 3.
- Rate-change transitional rules: the rate is the one on the document date.
- F7 corrections (adjust errors with net GST up to S$3,000 in the next F5
  instead [SECONDARY]) and the F8 final return.
- Due dates: one month after the end of each accounting period (30 Apr,
  31 Jul, 31 Oct, 31 Jan for calendar quarters) [PRIMARY]. Not tracked yet.

## What to verify

- The IGDS box labels, any official UEN check-letter algorithm, and the
  shapes of older `M` numbers (research §9).
- The half-cent rounding direction.
- Records must be kept for at least 5 years [PRIMARY], and KhataOS never
  purges them. Keep exports when you change software.

## InvoiceNow (PINT SG e-invoices)

IRAS requires GST-registered businesses to transmit invoice data through
InvoiceNow (Singapore's Peppol network) [PRIMARY, IRAS page as of
2026-10-20]:
- new voluntary registrants: within 6 months of incorporation from 1 Nov
  2025, and all from 1 Apr 2026;
- new compulsory registrants, and existing businesses with ≤ S$200k
  supplies: from 1 Apr 2028;
- ≤ S$1m from 1 Apr 2029, ≤ S$4m from 1 Apr 2030, and the rest from 1 Apr
  2031.

KhataOS writes the **PINT SG Billing** UBL (v1.4.1,
`urn:peppol:pint:billing-1@sg-1`) for a Sales or Credit Note voucher
(`pint_sg_xml`, export action `exportPintSg`), with the PINT SG rules it
breaks. GST categories come from the tax class: standard → SR, zero-rated →
ZR, exempt → ESN33 (name the class `ES33` for regulation 33 supplies), out
of scope → OS; a company without a GST number issues NG. The company's
UEN (a "UEN: …" line in the company address, or the GST number when it is a
UEN) gives its participant id `0195:SGUEN<UEN>` (`peppol_participant_id`).
Buyers need a Peppol id or UEN ("Peppol ID: …" / "UEN: …" in their mailing
address, or a UEN-shaped GST number), a street and a postal code; the
voucher needs a buyer reference or PO ("PO: …" in the narration).

Sending is through an IMDA-accredited Access Point: `peppol_ap_send` POSTs
the UBL to a configured URL with an API key, a generic adapter that does
not claim compatibility with any provider's API. Not supported: foreign
currency (GST in SGD), customer accounting (SRCA-*), GIRO. Details and
gaps: [`docs/peppol.md`](peppol.md). Until checked with your Access Point,
treat the output as a draft.

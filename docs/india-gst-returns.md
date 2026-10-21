# GST returns (India): GSTR-1 and GSTR-3B

KhataOS works out GSTR-1 and GSTR-3B figures from the books and can save a
draft GSTR-1 JSON. These are a preparation aid. They are not a filing, and
nothing here is legal advice: check the figures, and validate the JSON with
the GST portal's Returns Offline Tool, before filing.

Code: `crates/khata-in/src/returns.rs` (the rules, on a neutral document
model), `apps/desktop/src-tauri/src/gst.rs` (vouchers to documents),
`apps/desktop/src/screens/gst.ts` (screens). Sources and open questions are
in `docs/research/india-compliance.md` §5.

## What is computed

**GSTR-1**, for any period (the return period `fp` is the month of the end date):

| Section | What goes in |
|---|---|
| B2B (4A) | Invoices to parties with a GSTIN, invoice-wise and rate-wise |
| B2CL (5) | Inter-State invoices to unregistered parties with a value **above ₹1 lakh** (₹2.5 lakh for invoices dated before 1 Aug 2024, Notif. 12/2024-CT) |
| B2CS (7) | Other supplies to unregistered parties, by place of supply and rate; credit notes to them are netted in |
| CDNR / CDNUR (9B) | Credit notes to registered parties; to unregistered parties only when inter-State above the B2CL threshold, or for exports |
| Exports (6A) | Documents with place of supply 96 (with or without IGST) |
| Nil-rated (8) | Lines at 0% |
| HSN summary (12) | All lines by HSN, UQC and rate, split into B2B and B2C; credit notes reduce it |
| Documents issued (13) | Per document type and number series: lowest and highest number, total, cancelled, net issued |

**GSTR-3B**: 3.1(a) taxable outward supplies, 3.1(b) zero-rated, 3.1(c)
nil-rated, **3.1(d) inward supplies liable to reverse charge**, 3.2 inter-State
supplies to unregistered persons by place of supply, **4(A)(3)** ITC on
reverse charge, 4(A)(5) all other ITC, **4(B)(1)** blocked credit (s.17(5)),
**4(B)(2)** Rule 37 reversals, **4(C)** net ITC, and for each tax (IGST, CGST,
SGST/UTGST, cess) output tax (including tax on reverse charge, shown
separately as it is paid in cash) − net ITC. Tables 3.1 and 3.2 use the same
classification as GSTR-1, so they agree with it.

The advanced parts (reverse charge, blocked and reversed credit, GSTR-2B
reconciliation, GSTR-1A candidates) are described below; sources and open
questions are in `docs/research/india-gst-advanced.md`.

## How vouchers are read (assumptions)

- **Outward**: Sales are invoices; Credit Notes (sales returns) are credit
  notes. **Inward**: Purchases add ITC; Debit Notes (purchase returns) reduce
  it. Journals and other voucher types are left out even when they post to
  tax ledgers (a warning says so).
- The party line is found as on the printed invoice. Item lines and other
  ledgers on the opposite side make up the taxable value; a ledger whose
  name contains "round" is treated as round-off and left out; a non-tax line
  on the party's side (a discount) is **not** deducted (a warning says so).
- Tax comes from ledgers marked with a tax type (IGST, CGST, SGST, UTGST,
  cess). It is spread over the lines by taxable value × the rate on file for
  each line's HSN; when a rate is missing, by taxable value, and the rate is
  worked out as tax ÷ taxable value and snapped to 0, 0.1, 0.25, 1.5, 3, 5,
  12, 18, 28 or 40% (a warning if it is more than 0.01 off).
- Place of supply is the state in the party's GSTIN, or, for an
  unregistered party, the State in the ledger's mailing details. A party with
  neither is taken as within the company's state (a warning counts them).
- Every 0% line is reported as nil-rated: the books do not tell nil-rated,
  exempt and non-GST supplies apart.
- For CDNUR the note's own value is tested against the threshold, because
  notes are not linked to the original invoice.
- Table 13 counts the documents in the books; numbers missing from a series
  (deleted vouchers) are flagged, since they should be reported as cancelled.
- GSTR-3B net tax is output − ITC per tax, with **no** set-off order between
  IGST, CGST and SGST credit (s.49, Rule 88A). Imports (4(A)(1), (2)), ISD
  credit, Rule 38/42/43 reversals (Rule 42 has a calculator), 4(D), interest
  and late fee are not worked out. Purchases from a supplier with no GSTIN
  give no ITC, unless the line is on reverse charge.

## Reverse charge (RCM)

Code: `crates/khata-in/src/rcm.rs`, `apps/desktop/src-tauri/src/{tax.rs,
gst.rs, gst_rcm.rs}`, screen `apps/desktop/src/screens/gst_rcm.ts`.

- **Flag**: the ledger's or stock item's HSN/SAC carries a prefix: `RCM:9982`
  (reverse charge), `BLOCKED:8703` (blocked credit), `RCM:BLOCKED:…` (both).
  No change to the core model. The GST Flags screen toggles them (Alt+R,
  Alt+B); the code after the prefix is used for the rate and the HSN summary,
  and printed without the prefix on the self-invoice. Other prints and the
  e-invoice still read the raw class (see "Known gaps").
- **Posting**: on a Purchase or Debit Note, `invoice_tax` gives per tax the
  credit on the input-tax ledger (positive) and the tax owed on the
  output-tax ledger (negative, `reverse_charge: true`); the voucher screen
  posts negatives on the supplier's side, so the supplier is owed the taxable
  value only. On a Sale, an `RCM:` line charges no tax (the buyer pays) and
  warns; GSTR-1 does not mark outward reverse-charge supplies.
- **Returns**: on a purchase, output-tax ledgers are the tax owed (3.1(d)),
  spread over the reverse-charge lines; input-tax ledgers less that tax are the
  credit on the other lines. 4(A)(3) = the tax owed (credit assumed in the
  same period). Suppliers without a GSTIN are fine on reverse charge.
- **Self-invoice** (s.31(3)(f), Rule 47A: within 30 days of receipt) for
  reverse-charge purchases from unregistered suppliers: the Reverse Charge
  Register lists them with the number and due date; Ctrl+Alt+P prints it.
  Numbers `SI/26-27/n` run per financial year in the order the purchases were
  entered (from the audit trail, not stored): cancel such a purchase rather
  than delete it. No payment voucher (s.31(3)(g)) is produced.

## Blocked and reversed ITC

- **Blocked credit (s.17(5))**: lines flagged `BLOCKED:` post as usual (with a
  warning); GSTR-3B keeps their tax in 4(A) and reverses it in 4(B)(1), as
  Circular 170/02/2022-GST says (not 4(D)).
- **Rule 37** (`rule37_report(as_at)`): purchases (not reverse charge, credit
  not blocked, supplier registered) whose New Ref bill on the supplier's line
  is still open more than 180 days after the voucher date; ITC to reverse =
  ITC × unpaid ÷ invoice value. GSTR-3B for the month containing day 181 puts
  it in 4(B)(2). Re-availment after payment (4(A)(5) + 4(D)(1)) and interest
  are not worked out; purchases without bill-wise details are counted in a
  warning. Rule 37A (supplier did not file GSTR-3B) is not worked out.
- **Rule 42** (`rule42_calc`): the common-credit split for one tax: C1, C2,
  D1 = E/F × C2, D2 = 5% of C2, C3; D1 + D2 is to be reversed in 4(B)(1).
  Rule 43 (capital goods) is not covered.

## GSTR-2B reconciliation

`gstr2b_reconcile(json_text, from, to)` (screen `gstr2b.ts`, Alt+S imports the
file through `read_gstr2b_file`, which accepts `.json`): reads `docdata` b2b,
b2ba, cdnr, cdnra and impg (fields in the research note §3) and matches each
document with the purchases on supplier GSTIN, normalised number (voucher
number or New Ref bill reference), date (±3 days) and amounts (₹1); both
tolerances can be changed. Result: matched, mismatched (with what differs),
in 2B not in books, in books not in 2B (period only; a year before is
searched), documents with ITC not available (`itcavl = N`, `rsn`), and the
bills of entry (listed, not matched). Enter opens the purchase.

## GSTR-1A candidates

`gstr1_mark_filed(period)` records "GSTR-1 for YYYY-MM filed now" as a
shared record of the company (synced with the books; `docs/sync-settings.md`). `gstr1_amendments` then lists the
period's Sales and Credit Notes added, altered, cancelled or deleted after that
time, from the audit trail, with number, date, party, value, taxable value and
tax as filed and now. Report them in GSTR-1A before filing GSTR-3B, or as
amendments in a later GSTR-1. No GSTR-1A JSON is produced.

## Known gaps

- Invoice print, e-invoice and e-way bill read the raw tax class, so a
  flagged item would show `RCM:`/`BLOCKED:` there (flags belong on purchase
  ledgers and items; outward use is not supported).
- Imports (4(A)(1), (2)), ISD, 4(D), interest, Rule 37 re-availment, Rule 37A,
  Rule 43, outward reverse charge in GSTR-1.

## Draft GSTR-1 JSON

`gstr1_json` writes the keys of the portal's offline-tool JSON as publicly
documented (`gstin`, `fp`, `b2b[{ctin, inv[{inum, idt, val, pos, rchrg,
inv_typ, itms[{num, itm_det{txval, rt, iamt, camt, samt, csamt}}]}]}]`,
`b2cl`, `b2cs`, `cdnr`, `cdnur`, `exp`, `nil`, `hsn`, `doc_issue`). The
official schema was not retrieved (research: UNCERTAIN), so the file is
**not portal-validated**. Known differences to check:

- no `version` or `hash` keys; items are numbered 1, 2, … per invoice;
- Table 12 is written as `hsn.hsn_b2b` and `hsn.hsn_b2c`;
- exports have no shipping-bill number, date or port code;
- Table 13 document-type numbers (1 invoices, 4 debit notes, 5 credit notes)
  are from memory of the offline tool.

## Before filing, verify

1. The schema against the current Returns Offline Tool (store it under
   `/schemas/gst/` with its version, and add a round-trip test).
2. Rates on file for every HSN, and HSN/SAC on every item and service line.
3. Unregistered parties outside the company's state (place of supply).
4. Nil-rated vs exempt vs non-GST lines, discounts, freight and other charges.
5. GSTR-3B against GSTR-1 and GSTR-2B/IMS (ITC is from the books; reconcile
   with the GSTR-2B screen).
6. Reverse-charge, blocked-credit and Rule 37 figures (see the research note's
   open questions).

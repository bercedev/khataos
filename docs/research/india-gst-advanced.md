# India GST, the advanced parts: reverse charge, blocked and reversed ITC, GSTR-2B, GSTR-1A

> **Status as of:** 2026-10-21. Extends `india-compliance.md` §4.2 (RCM) and §5
> (returns). **Not legal advice.** Tags: **[PRIMARY]** read in the CBIC text or
> a CBIC circular; **[SECONDARY]** a GSP, practitioner or news source;
> **[UNCERTAIN]** not confirmed, or an interpretation KhataOS had to make.
> No rate or rule here was made up; where a source was not found, it says so.

Code: `crates/khata-in/src/{rcm.rs, gstr2b.rs, returns.rs}`,
`apps/desktop/src-tauri/src/{tax.rs, gst.rs, gst_rcm.rs, gstr2b.rs}`,
`apps/desktop/src/screens/{gst_rcm.ts, gstr2b.ts}`. User-facing summary:
`docs/india-gst-returns.md`.

## 1. Reverse charge (RCM)

| Rule | Source | What KhataOS does |
|---|---|---|
| Tax on notified goods and services is paid by the recipient (CGST s.9(3)); on specified supplies from unregistered suppliers, by the registered recipient (s.9(4)); IGST s.5(3), 5(4) likewise | CGST Act s.9 [SECONDARY: well established; the notified lists are in Notif. 13/2017-CT(R) and 4/2017-CT(R) as amended, not reproduced here] | A ledger or stock item is flagged by a tax-class prefix `RCM:` (e.g. `RCM:9982`). KhataOS does **not** ship the notified lists: the user flags the ledgers (legal fees, GTA freight, sponsorship…) |
| The recipient liable under s.9(3)/(4) issues an invoice for supplies **from an unregistered supplier** (s.31(3)(f)) and a payment voucher when paying (s.31(3)(g)) | CGST Act s.31(3)(f), (g) [SECONDARY via `india-compliance.md` §4.2] | Self-invoice printed from the Purchase (`print_self_invoice`). The payment voucher (s.31(3)(g), Rule 52) is **not** generated |
| **Rule 47A**: "he shall issue the said invoice within a period of thirty days from the date of receipt of the said supply" — inserted w.e.f. **1 Nov 2024** by Notif. 20/2024-CT dated 08.10.2024 | CGST Rule 47A [PRIMARY: <https://taxinformation.cbic.gov.in/content/html/tax_repository/gst/rules/cgst_rules/active/chapter6/rule47a_v1.00.html>] | The register shows the due date = voucher date + 30 days. The voucher date is taken as the date of receipt [UNCERTAIN: the books have no separate receipt date] |
| Self-invoice contents follow Rule 46, incl. (p) "whether the tax is payable on reverse charge basis" | CGST Rule 46 [PRIMARY, see `india-compliance.md` §4.2] | Title "Self Invoice (Reverse Charge)", "Tax is payable on reverse charge: Yes", HSN/SAC, place of supply, rate-wise tax |
| Self-invoice numbering: a Rule 46(b) serial number, ≤ 16 characters, unique in the financial year | CGST Rule 46(b) [PRIMARY] | Own series `SI/26-27/n`, worked out from the books (order of entry in the audit trail, per FY) — no new storage. Gaps when a purchase is deleted; cancelling keeps numbers [UNCERTAIN: whether a derived series is acceptable; a stored series would need a core change] |
| RCM liability is paid in cash: the electronic credit ledger pays "output tax", which excludes tax payable on reverse charge | CGST s.49(4) with s.2(82) [SECONDARY: well established] | GSTR-3B net rows show the reverse-charge part separately (`reverse_charge`), but `net` still nets it like the rest (documented) |
| ITC of RCM tax: reported in **4(A)(3)** "Inward supplies liable to reverse charge (other than 1 & 2 above)"; the liability in **3.1(d)** | GSTR-3B form; Circular 170/02/2022-GST Annexure Table 2 row 4(A)(3) [PRIMARY] | 3.1(d) = RCM lines (registered or unregistered supplier); 4(A)(3) = the same tax, in the same period [UNCERTAIN: credit only after the tax is paid; KhataOS assumes payment in the same period] |
| Imports of services (IGST s.5(3)) go to 4(A)(2); imports of goods to 4(A)(1) | Circular 170 Annexure [PRIMARY] | Not separated: a foreign supplier's RCM line is treated like any other RCM line (the books do not mark a party as foreign) [UNCERTAIN] |
| e-Invoicing of RCM self-invoices (supply types RCMDOM/RCMIMP) | `india-compliance.md` §4.2 [UNCERTAIN: sandbox only] | Not done |
| Outward supplies on reverse charge (GSTR-1 `rchrg: "Y"`) | — | **Not supported**: a sale line flagged `RCM:` charges no tax and warns; GSTR-1 does not mark it (a warning says so) |

**Posting.** `invoice_tax` for a Purchase/Debit Note with an `RCM:` line returns,
per tax, a positive row on the **input**-tax ledger (the credit) and a negative
row (`reverse_charge: true`) on the **output**-tax ledger (the tax owed). The
voucher screen already posts negative rows on the party's side, so the supplier
is credited with the taxable value only. `gst.rs` reads output-tax ledgers on a
purchase as the RCM liability and input-tax ledgers as credit.

## 2. Blocked and reversed ITC

| Rule | Source | What KhataOS does |
|---|---|---|
| Blocked credits (motor vehicles, food, club membership, works contracts for immovable property, personal use…) | CGST s.17(5) [SECONDARY: the list itself is not encoded] | Flag `BLOCKED:` on the ledger/item; the user decides what is blocked |
| "Total ITC (eligible as well as ineligible) is being auto-populated from statement in FORM GSTR-2B in different fields of Table 4A"; reversals "absolute in nature … rule 38, rule 42, rule 43 … and for reporting ineligible ITC under section 17(5) … in Table 4 (B) (1)" | Circular 170/02/2022-GST, 6 Jul 2022, para 4.3(A), (B) [PRIMARY: <https://www.gstcouncil.gov.in/sites/default/files/2024-06/cir-170-02-2022-cgst.pdf>] | Blocked lines stay in 4(A)(5) (or 4(A)(3) when also RCM) and their tax goes to **4(B)(1)** |
| "reversal of ITC of ineligible credit under section 17(5) … is required to be made under Table 4(B) and not under Table 4(D)"; 4(D)(1) no longer for s.17(5) | Circular 170 para 4.3(E), 4.4 [PRIMARY] | 4(D) is not used. (The task brief mentioned 4(D)(1); the circular supersedes that layout.) |
| Reclaimable reversals (Rule 37, s.16(2)(b), (c)) in **4(B)(2)**; reclaimed later in 4(A)(5) and shown in 4(D)(1) | Circular 170 para 4.3(C) [PRIMARY] | Rule 37 reversals due in the period go to 4(B)(2). Re-availment on later payment is **not** worked out |
| Net ITC 4(C) = 4A − [4B(1) + 4B(2)] | Circular 170 para 4.3(D) [PRIMARY] | `itc_net`; GSTR-3B `net[].itc` is 4(C) |
| 4(D)(2): ITC not available under s.16(4) or where the recipient of an intra-State supply is in another State | Circular 170 para 4.3(F) [PRIMARY] | Not worked out; GSTR-2B `itcavl = N` documents are listed in the 2B reconciliation |
| **Rule 37(1)**: ITC on a supply "other than the supplies on which tax is payable on reverse charge basis" not paid "whether wholly or partly" within the time in the second proviso to s.16(2) (180 days) is paid or reversed, "proportionate to the amount not paid to the supplier", with interest under s.50, "while furnishing the return in FORM GSTR-3B for the tax period immediately following the period of one hundred and eighty days from the date of the issue of the invoice"; Schedule I supplies without consideration and s.15(2)(b) amounts are deemed paid; (2) re-availed on payment; (4) the s.16(4) time limit does not apply to re-availing. Substituted w.e.f. 01.10.2022 (Notif. 19/2022-CT), words inserted by Notif. 26/2022-CT | CGST Rule 37 [PRIMARY: <https://taxinformation.cbic.gov.in/content/html/tax_repository/gst/rules/cgst_rules/active/chapter5/rule37_v1.00.html>] | `rule37_report(as_at)`: forward-charge purchases from registered suppliers, credit not blocked, older than 180 days, with an open New Ref bill on the supplier line; reversal = ITC × unpaid ÷ invoice value. Interest is **not** computed |
| Which GSTR-3B period gets the reversal | Rule 37(1) wording above | KhataOS takes the month containing day 181 after the invoice date (`reverse_from`) [UNCERTAIN: "the tax period immediately following the period of 180 days" could also be read as the month after the month in which day 180 falls] |
| The 180 days run from the supplier's invoice date | Rule 37(1) | The voucher date is used [UNCERTAIN: the books keep no separate supplier-invoice date] |
| **Rule 37A**: ITC on invoices whose supplier has not filed the corresponding GSTR-3B by 30 September after the FY is reversed by 30 November; re-availed when the supplier files | CGST Rule 37A [PRIMARY: <https://taxinformation.cbic.gov.in/content/html/tax_repository/gst/rules/cgst_rules/active/chapter5/rule37a_v1.00.html>] | **Not computed**: needs the supplier's GSTR-3B status, which neither the books nor GSTR-2B carry. Documented in the Rule 37 screen |
| **Rule 42(1)**: T, T1 (non-business), T2 (exempt), T3 (s.17(5)), C1 = T − (T1+T2+T3), T4 (taxable only), C2 = C1 − T4, D1 = (E/F) × C2 (E exempt supplies, F total turnover in the State), D2 = 5% of C2 for partly non-business use, C3 = C2 − (D1+D2); computed per tax (clause (l)); D1 + D2 reversed in GSTR-3B (clause (m)); no turnover in the period → last period's E/F | CGST Rule 42 [PRIMARY: <https://taxinformation.cbic.gov.in/content/html/tax_repository/gst/rules/cgst_rules/active/chapter5/rule42_v1.00.html>] | `rule42_calc` for one tax. Annual true-up (42(2)), the real-estate variant and Rule 43 (capital goods) are **not** implemented |

## 3. GSTR-2B JSON and reconciliation

The official GSTN API documentation is only on the GST developer portal behind
registration; the field list below is from a GSP's published mirror of the GSTN
GSTR-2B "get details" response (schema keys `docdata.b2b.items.[i].inv.items.[j].inum`
etc.) [SECONDARY: Sandbox.co.in API reference,
<https://developer.sandbox.co.in/api-reference/gst/compliance/endpoints/taxpayer/gstr-2b/document>;
cross-checked with an open-source importer that reads `data.docdata.b2b[].inv[]`,
<https://github.com/arjukjin-cyber/itc-rescue/pull/27>]. The portal's own JSON
download is assumed to carry the same `docdata` object [UNCERTAIN: no portal
download was inspected; the parser looks for `docdata` at the top level and
under up to four `data` keys].

| Section | Fields read |
|---|---|
| envelope | `gstin`, `rtnprd` (MMYYYY), `gendt` |
| `b2b[]`, `b2ba[]` | `ctin`, `trdnm`, `supprd`, `inv[]`: `inum`, `typ` (R, DE, SEWP, SEWOP, CBW), `dt` (DD-MM-YYYY), `val`, `pos`, `rev` (Y/N), `itcavl` (Y/N), `rsn`, `txval`, `igst`, `cgst`, `sgst`, `cess`, `imsStatus`; b2ba adds `oinum`, `oidt` |
| `cdnr[]`, `cdnra[]` | as above with `nt[]`: `ntnum`, `typ` (C/D), `suptyp`, `dt`, …; cdnra adds `ontnum`, `ontdt`, `onttyp` |
| `impg[]` | `refdt`, `recdt`, `portcode`, `boenum`, `boedt`, `isamd`, `txval`, `igst`, `cess` (listed, not matched) |
| not read | `isd`, `isda`, `impgsez`, `ecom`, `ecoma`, `itcrev`, `docRejdata`, `cpsumm`, `itcsumm` (a warning names non-empty ones) |

Older files carry per-rate `items[]` (`num`, `rt`, `txval`, `igst`…) instead of
amounts on the document (pre Oct 2024 per the same GSP) — summed. GSTR-2A style
names (`idt`, `iamt`, `camt`, `samt`, `csamt`, `itms[].itm_det`), as in the task
brief, are accepted too.

`rsn` codes: `P` appears in GSTN samples; its meaning (recipient in a different
State from the place of supply of an intra-State supply) is from Circular 170
para 4.3(F) and practitioner guides [UNCERTAIN]. `C` is shown as "time limit
under s.16(4)" [UNCERTAIN: not confirmed from GSTN]. Other codes are shown raw.

**IMS** (from 14 Oct 2024): accepted/pending/rejected actions decide what goes
into 2B (`india-compliance.md` §5) [SECONDARY]. KhataOS reads `imsStatus` but
does not act on it.

**Matching** (KhataOS's own design, not a GSTN rule): supplier GSTIN +
document class (invoice / credit note / debit note) + number normalised
(letters and digits only, upper case, leading zeros of each digit run
dropped); the number in the books is the voucher number **or** a New Ref bill
reference on the supplier's line. Then date within ±3 days and taxable value
and each tax within ₹1 (both adjustable). Unpaired 2B documents are paired with
a book document of the same supplier and class with the same taxable value and
total tax and a date within the tolerance ("number differs"). Book documents up
to 366 days before the period are searched; only those dated in the period are
reported as "in books, not in 2B". Book documents without a supplier GSTIN are
left out.

**File reading**: the existing `read_text_file` command accepts only `.csv` and
`.txt`, so a new command `read_gstr2b_file` accepts `.json` (20 MB limit, same
mobile path).

## 4. GSTR-1A and amendments

| Rule | Source | What KhataOS does |
|---|---|---|
| GSTR-1A: amend or add outward supplies of a tax period after GSTR-1 is furnished and before GSTR-3B is filed; optional, once per period; effect in the same period's GSTR-3B; recipient sees it in the next 2B | Rule 59(1) proviso, Notif. 12/2024-CT dated 10.07.2024 [PRIMARY via `india-compliance.md` §5]; GSTN advisory [SECONDARY: <https://taxguru.in/goods-and-service-tax/advisory-form-gstr-1a.html>, <https://taxscan.in/gstn-issues-advisory-on-detailed-manual-and-faqs-on-filing-of-gstr-1a/424987>] | `gstr1_mark_filed(period)` stores the time as a shared record of the company (`gst.gstr1_filed.YYYY-MM`, synced); `gstr1_amendments` lists Sales and Credit Notes of that month added, altered, cancelled or deleted after that time, from the audit trail, with values as filed and now. Bank-date-only changes are ignored. No GSTR-1A JSON is produced |
| Start date conflict: July 2024 period vs August 2024 | GSTN advisory vs news [SECONDARY, conflicting] | Not relevant to the computation |
| After GSTR-3B is filed, corrections go into a later GSTR-1 (amendment tables 9A/9C) | `india-compliance.md` §5 | The screen says so; the amendment tables are not produced |

## 5. Open questions (UNCERTAIN, for a CA)

1. Rule 37 reversal month: day 181's month, or the month after day 180's month?
2. Is a self-invoice series derived from the audit trail acceptable, or must
   the number be fixed when issued (store it — needs a core field)?
3. Date of receipt vs voucher date (Rule 47A) and supplier-invoice date vs
   voucher date (Rule 37, 2B matching).
4. ITC on RCM in the same period as the liability (paid in cash in that return).
5. RCM imports of services (4(A)(2)) and foreign suppliers' place of supply.
6. The portal JSON download layout vs the API layout (inspect a real file and
   add it as a fixture).
7. Meaning of `rsn` codes other than `P`.

# India: GST, TDS and TCS

**What you'll do:** set up an Indian company for GST, enter GST invoices, prepare GSTR-1 and GSTR-3B, reconcile GSTR-2B, generate e-invoices, and handle TDS and TCS.

> KhataOS prepares returns and files from your books. It doesn't file anything with the GST portal or the income-tax department. **Check every figure and file with your accountant, and with the official offline tools, before you file.**

## Set up

1. Pick **India** when the company first opens. The financial year runs April to March, in rupees.
2. `Alt+K`, **Company details**: type the company's **GSTIN**. The state fills in from it. Add the address, and choose the **HSN/SAC digits** for invoices (4 up to ₹5 crore turnover, 6 above).
3. **Duty ledgers** under Duties & Taxes, one for each tax you charge or pay: for example "Output CGST 9%" with Type of tax **CGST**, "Output SGST 9%" with **SGST**, "Output IGST 18%" with **IGST**, and the input ledgers for purchases. See [Masters](masters.md#ledgers).
4. **Tax rates:** `C`, **Tax Rates**: add each HSN/SAC code with its rate and the date it applies from.
5. Give each **stock item** (or sales and purchase ledger) its HSN/SAC code.
6. On each **party ledger**, add the **GSTIN** (checked as you type: format, state code, PAN and check character) or, for an unregistered party, the **State**. Mark SEZ parties. A party in another **Country** is an export.

## GST on invoices

Enter a Sales or Purchase invoice as usual ([Vouchers](vouchers.md#sales-purchase-credit-note-and-debit-note-item-invoice)). After the items, KhataOS fills in the tax:

- **CGST + SGST** (or UTGST) when the place of supply is in your state, **IGST** when it isn't. The place of supply comes from the party's GSTIN, or its State.
- SEZ supplies and exports are always inter-State. Exports under LUT are zero-rated; put the LUT ARN in Invoice design.
- Invoice numbers must be 16 characters or fewer, using letters, digits, `-` and `/`, and unique in the financial year (Rule 46).

## Returns and GST reports

All of these are in **Go To** (`Alt+G`), under GST. Type part of the name.

| Go To | What it does | Keys |
|---|---|---|
| **GSTR-1 (outward supplies)** | B2B, B2CL, B2CS, credit and debit notes, exports, nil-rated, HSN summary and documents issued | `Ctrl+Alt+J` saves the draft GSTR-1 JSON; `Ctrl+Alt+E` saves e-invoice JSON for the period |
| **GSTR-3B (summary return)** | Tables 3.1, 3.2 and 4, with tax payable | Print and CSV |
| **GSTR-2B reconciliation** | Matches your purchases with the GSTR-2B JSON downloaded from the portal | `Alt+S` imports the JSON; `Ctrl+Alt+T` sets the tolerance |
| **Input tax credit (GSTR-3B Table 4)** | ITC available, reversed and ineligible | |
| **Reverse charge register and self-invoices** | Reverse-charge purchases and their self-invoices | `Ctrl+Alt+P` prints a self-invoice |
| **Reverse charge and blocked credit flags** | Mark purchases as reverse charge or blocked credit | `Alt+R`, `Alt+B`; `Alt+4` the Rule 42 calculator |
| **ITC to reverse for unpaid bills (Rule 37)** | Supplier bills unpaid after 180 days | |
| **GSTR-1A: changes after filing** | Outward vouchers changed after you filed | `Alt+F` marks GSTR-1 as filed |
| **MSME overdue payables (over 45 days)** | Supplier bills past 45 days | |

To file GSTR-1: save the JSON with `Ctrl+Alt+J`, open it in the GST offline tool, check it there, and upload it from the tool. The JSON hasn't been checked against the offline tool yet. Detail: [GST returns](../india-gst-returns.md).

The [Desk](desk.md) shows the GSTR-1, GSTR-3B and other due dates once you set monthly or quarterly (QRMP) returns in the compliance profile.

## e-Invoice and e-way bill

On a saved Sales invoice or Credit Note, press `Ctrl+E` (Export):

- **Generate IRN (e-invoice portal)** sends the invoice to an IRP and brings back the IRN and the signed QR code, which then print on the invoice. The IRN screen also cancels an IRN within 24 hours (`C`) and generates an e-way bill from the IRN (`E`).
- **e-Invoice JSON** saves the invoice as JSON for the portal's offline upload.
- **e-Way Bill JSON** (Sales and Purchase) saves the JSON for the e-way bill portal's bulk upload.

**Set up the IRP:** `Alt+G`, **e-Invoice portal (IRP) settings**: choose the IRP (the NIC sandbox by default), and enter your API credentials and the IRP's public key.

> The IRP connection has been tested **only against a mock** of the published API, not a real IRP. The e-invoice and e-way bill JSON haven't been checked with the official tools. There is no standalone e-way bill API (Part B, cancel, extend) yet. Detail: [e-Invoice](../einvoice.md).

## TDS and TCS

1. On a supplier's ledger, fill the **TDS** section: PAN, deductee type and any lower-deduction certificate. On a customer's, the **TCS** section.
2. On expense and purchase ledgers, set the **Nature of payment (TDS)**; on sales and income ledgers, the **Nature of collection (TCS)**.
3. `Alt+G`, **Deductor details (TAN)**: your TAN and the person responsible.
4. In vouchers, KhataOS suggests a TDS line on Journal, Purchase and Payment, and TCS on Sales and Receipt, once the year's threshold is crossed. `Ctrl+D` removes a suggested line.

Reports in Go To, under TDS: **TDS Payable** and **TCS Payable** (with due dates), **TDS Quarterly Statement**, **TDS on payments to non-residents**, **TCS Quarterly Statement**, and **Salary TDS estimate**. In a quarterly statement, `Ctrl+Alt+X` saves the statement text file for the FVU (from tax year 2026-27), `Ctrl+Alt+J` the CSV, and `Enter` on a challan edits its details.

> Validate every statement file with the official File Validation Utility before filing. The salary TDS return and Form 16 aren't produced. Detail: [TDS and TCS](../india-tds.md).

## Schedule III

Go To, under Statutory: **Balance Sheet (Schedule III)**, **Statement of Profit and Loss (Schedule III)**, **Notes to Accounts (Schedule III)** and **Schedule III mapping** (which group goes to which line). Previous-period figures are included. Detail: [Schedule III](../india-schedule-3.md).

## Not there yet

Direct filing with the GST portal (through a GSP), GSTR-9 and 9C, the composition scheme (CMP-08, GSTR-4), more than one GSTIN in a company, ITC-04, TDS certificates and challan matching, payroll (PF, ESI, PT), and the Rule 53 number check on credit and debit notes.

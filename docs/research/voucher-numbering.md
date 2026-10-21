# Voucher and invoice numbering

Research done 19 October 2026 to decide whether voucher numbers should restart each financial year. Secondary sources are marked as such. Check the primary texts before relying on this for compliance advice.

## What the rules say

| Jurisdiction | Rule | What it requires |
|---|---|---|
| India (GST) | CGST Rules, Rule 46(b) for tax invoices; Rule 49 for bills of supply; Rule 53 for credit and debit notes | A consecutive serial number of at most 16 characters, made of letters, digits, hyphen and slash, **unique for a financial year**. GSTN's 2019 advisory asks taxpayers to start a fresh series each financial year. The e-invoice portal (IRP) treats numbers as case-insensitive, and a number can't start with "/" or "-". |
| UAE (VAT) | Executive Regulation (Cabinet Decision 52 of 2017), Art. 59 | A tax invoice carries a sequential number that uniquely identifies it. The secondary sources found don't mention a yearly reset rule. |
| Saudi Arabia (ZATCA e-invoicing) | E-invoicing Resolution and its technical specs | The **Invoice Counter Value (ICV)** of the e-invoicing device must be tamper-resistant and must never be reset (it starts again only on a new CSID). The ICV is separate from the human-readable invoice number. |
| EU | VAT Directive 2006/112/EC, Art. 226(2) | A sequential number "based on one or more series, which uniquely identifies the invoice". Yearly series with the year in the number are common and allowed. Member states may add rules. |

## What Tally does

- Each voucher type has a numbering method: automatic, automatic with manual override, manual, or multi-user automatic. There is an option to prevent duplicate numbers.
- **Restart numbering** sets a starting number, an applicable-from date and a periodicity (yearly, monthly and so on). A prefix and suffix can each have their own applicable-from date.
- Tally's help recommends restarting numbering for every relevant voucher type when moving to a new financial year. For GST, it recommends that invoice numbers stay unique and continuous, using prefixes and suffixes where needed.

## What KhataOS does

- **Per-type series.** Each voucher type has its own series in the company profile (`khata_core::numbering`). The series restarts never, every financial year, or every month. Its prefix and suffix can use the tokens `{FY}` ("26-27", or "2026" for a calendar year), `{YYYY}`, `{YY}` and `{MM}`, and the running number can be zero-padded.
- **Uniqueness follows the series.** A number must be unique within its series period, compared case-insensitively. That matches Rule 46's "unique for a financial year", and still keeps numbers unique for life when a series never restarts.
- **Defaults for new companies.** Every voucher type restarts each financial year. Sales, Credit Note and Debit Note also carry the year, as in `26-27/1`. Bill-wise references default to the invoice number and are tracked by party and reference, so the year marker stops last year's "1" merging with this year's "1".
- **Existing companies.** They keep one series for life until someone changes it under Alter → Voucher Type.
- **Unsafe changes are refused.** For example, going back to one series for life after two years have both used "1" would make saved vouchers share a number, so the core refuses it.
- **Compliance checks stay in the pack.** The India pack still enforces the 16-character limit and the allowed characters on invoices and notes. The ZATCA ICV is a separate device counter and belongs in a future Saudi pack, not in voucher numbering.

## Sources

- India:
  - [Rule 46 text, CAclub](https://caclub.in/rule-46-of-cgst-rules-2017-tax-invoice/)
  - [GSTN advisory on a new series each year, IRIS GST](https://irisgst.com/effective-1st-april-2026-reset-the-invoice-number-series/)
  - [Masters India on the GSTN advisory](https://www.mastersindia.co/blog/gstn-issues-advisory-to-start-new-invoice-series/)
  - [ICMAI note on GST invoice rules](https://icmai.in/upload/Taxation/Rule_Invoice_GST.pdf.filepart)
  - [Busy on invoice numbering mistakes](https://busy.in/gst/invoice-numbering-mistakes-under-gst-causes-corrections-and-prevention/)
- Tally:
  - [Moving to the next financial year](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/moving-to-the-next-financial-year/)
  - [Voucher numbering methods](https://help.tallysolutions.com/use-voucher-numbering-methods/)
  - [Restart voucher numbering](https://help.tallysolutions.com/article/Tally.ERP9/Creating_Masters/Accounts_Info/Restart_Voucher_Numbering.htm)
- UAE:
  - [Mondaq on UAE tax invoices](https://mondaq.com/sales-taxes-vat-gst/670546/tax-invoice-under-uae-vat-law)
  - [Wafeq on UAE VAT invoice requirements](https://www.wafeq.com/en-ae/tax-and-reporting/vat-invoice-requirements-in-uae)
- Saudi Arabia:
  - [ZATCA forum on ICV resets](https://zatca1.discourse.group/t/invoice-counter-value-reset/659)
  - [ZATCA forum on UUID, ICV and IRN](https://zatca1.discourse.group/t/question-regarding-the-uuid-icv-and-irn/1307)
- EU:
  - [VAT Directive Art. 226](https://www.legislation.gov.uk/eudr/2006/112/article/226)

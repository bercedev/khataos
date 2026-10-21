# Reports

**What you'll do:** open the main reports, move through them with the keyboard, change their date or period, and print or export them.

## Open a report

| Report | From the home menu | Or |
|---|---|---|
| Day Book | `K` | `Alt+G` (Go To), type "day book" |
| Balance Sheet | `B` | Go To |
| Profit & Loss A/c | `P` | Go To |
| Chart of Accounts | `H` | Go To |
| Stock Summary | `S` | Go To |
| Ratio Analysis | `R` | Go To |
| Dashboard | `O` | Go To |
| Trial Balance | `D` (Report Directory), then type "trial" | Go To |
| Outstanding Receivables or Payables | `D`, then "outstanding" | Go To |
| Ageing: Receivables or Payables | `D`, then "ageing" | Go To |
| Godown Summary, Batch Summary | `D` | Go To |
| A ledger's statement | `Alt+F` (Find), type part of the ledger name | Go To, under "Ledger statements" |
| Bank Reconciliation, Cheque Register | `N` (Banking) | |

**Go To** (`Alt+G`) finds any report, voucher type, master form or ledger. Type words in any order: "vat saudi" finds "VAT Return (Saudi Arabia)". Tax reports show only for your company's country.

**Report Directory** (`D` on the home menu) lists every report, including the tax reports for your country and all ledger statements.

## Move around a report

1. `↑` `↓` move one row. `PgUp` `PgDn` move ten rows. `Home` and `End` (or `Ctrl+Home`, `Ctrl+End`) go to the first and last row.
2. In reports with two sides (Profit & Loss, Balance Sheet, Ratio Analysis), `←` `→` move to the other side.
3. `Enter` opens the row: a group opens its ledgers, a ledger opens its statement, and a voucher opens for alteration.
4. `Esc` goes back. The report you came from keeps your place.

You can also click a row to select it and double click to open it.

## Change the date or period of a report

1. Press `F2` (or `Alt+F2`) in the report.
2. A report over a period asks for **From** and **To**. A report "as at" a date (Trial Balance, Balance Sheet, Outstanding, Ageing) asks for **As at**.
3. Type the dates and press `Enter`. `Esc` clears a field, and pressed again it cancels.

This changes only the report on screen.

### Typing dates

Dates are always **day first**. Anything you leave out comes from the date already in the field.

| You type | You get |
|---|---|
| `19` | the 19th of the same month and year |
| `19-10` or `19/10` | 19 October, same year |
| `1-4-25` | 1 April 2025 (years with two digits are 20xx) |
| `19 oct 2026`, `19-Oct` | 19 October 2026, 19 October |
| `19.10.2026` | 19 October 2026 |
| `2026-10-19` | 19 October 2026 |

Arabic month names and Arabic-Indic digits work too.

## Set the current date and period for the whole company

These are the dates every new voucher and report starts from. They last until you close the company.

**Current date (`F2` on the home menu):**

1. On the home menu, press `F2`.
2. Type the date and press `Enter`.

New vouchers take this date, and the Day Book opens on it.

**Current period (`Alt+F2` on the home menu):**

1. On the home menu, press `Alt+F2`.
2. Type **From**. Leave **To** blank for the whole financial year that From falls in: `1-4-25` alone gives 1 April 2025 to 31 March 2026.
3. Press `Enter`.

Reports you open after this use the new period. Reports already open keep theirs. The home screen shows both the current period and the current date.

With nothing set, the period runs from the start of the financial year to the current date, and the current date is your computer's date.

**Moving to a new financial year** needs no closing step. The books are one continuous record, so balances carry forward by themselves. Set the new year with `Alt+F2`.

## The main reports

**Day Book.** Every voucher in the period, one day by default. `Enter` opens a voucher. `Alt+X` cancels the voucher on the row after asking: it keeps its number but no longer counts. Cancelled vouchers stay in the list, marked "(Cancelled)", with no amounts. `Alt+E` saves a PDF of the period's invoices and notes, or of the one voucher on the row. To delete a voucher, open it with `Enter` and press `Alt+D` there.

**Trial Balance.** Every ledger's balance as at a date, with Opening Stock, and a "Difference in opening balances" row if the opening balances don't agree. It is a flat list of ledgers: there is no view by group or detailed view yet.

**Profit & Loss A/c.** Expenses and income for the period, with gross profit, opening and closing stock, and net profit or loss. `Enter` on a group lists its ledgers (Group Summary).

**Balance Sheet.** Liabilities and assets as at a date, with the Profit & Loss A/c balance and closing stock. `Enter` on a group lists its ledgers, and on Profit & Loss A/c opens the P&L. The P&L balance is one figure since the books began; it isn't split into brought forward and current year yet.

**Group Summary.** The ledgers under a group (subgroups are flattened into one list). You reach it by opening a group from another report.

**Ledger statement.** One ledger's vouchers with the opening balance, running balance and closing balance. A ledger in a foreign currency also shows the foreign amount. A ✎ mark shows vouchers that have a note.

**Outstanding Receivables and Payables.** Every open bill, with its due date, pending amount and days overdue. Overdue days count from the due date, or from the bill date when there is no due date. These need bill-wise details on your vouchers (see [Vouchers](vouchers.md)).

**Ageing.** Open bills per party in fixed bands: 0 to 30, 31 to 60, 61 to 90 and over 90 days overdue. The bands can't be changed yet.

**MSME overdue payables (India).** Supplier bills unpaid more than 45 days after the bill date.

**Ratio Analysis.** Working capital, cash, bank, debtors and creditors, with the main ratios (current ratio, quick ratio, gross and net profit %, and others). `Enter` on most rows opens the report behind it.

**Dashboard.** Nine figure tiles and five charts. `Tab` moves between them, and `Enter` opens the report behind one. `Ctrl+H` switches the chart under the cursor between a chart and a table.

**Stock Summary.** Items by stock group, with closing quantity, rate and value. `Alt+F5` (or `Alt+F1`) switches to the detailed view with opening, inward, outward and closing figures. `Enter` on a group folds or unfolds it, and on an item opens its vouchers.

**Godown Summary and Batch Summary.** Stock on hand as at a date, by godown or by batch.

**Bank Reconciliation and Cheque Register.** See [Banking](masters.md#banking).

## Notes on a report row

Press `Alt+W` to add a note or to-do on the ledger, voucher or stock item under the cursor. It works on the Day Book, Trial Balance, Group Summary, ledger statements, Outstanding, Ageing, MSME and Stock Summary. See [Desk](desk.md).

## Print, PDF and export

**Print or save as PDF:**

1. Press `Ctrl+P` on the report.
2. A print window opens with the formatted page, then your system's print dialog.
3. Pick a printer, or choose "Save as PDF" (or "Microsoft Print to PDF" on Windows).

Day Book, Trial Balance, Profit & Loss, Balance Sheet and ledger statements print as formatted documents. Other reports print the table on screen. Pages are A4. Printing isn't available on phones yet.

**Export to CSV:**

1. Press `Ctrl+E`.
2. Choose where to save the file. The name is suggested from the report, for example `Trial-Balance.csv`.

The file opens in Excel or any spreadsheet. Amounts are saved without digit grouping. There is no Excel (.xlsx) export yet.

## Not built yet

These report keys that Tally users know are reserved but not built. See the [key reference](keyboard.md#key-reference).

- Filters (`Ctrl+F`), extra columns and period comparison (`Alt+C`, `Alt+N`), saved views (`Ctrl+L`).
- Detailed view (`Alt+F5`) anywhere except Stock Summary, and expand one line (`Shift+Enter`).
- Change the report's subject (`F4`), basis of values (`Ctrl+B`), exception reports (`Ctrl+J`).
- Duplicate or insert a voucher from the Day Book (`Alt+2`, `Alt+I`), hide and show rows (`Ctrl+R`, `Ctrl+U`).
- Sales, purchase and journal registers, ledger monthly summaries, cash and bank books, cash flow and funds flow.
- Email and WhatsApp (`Ctrl+M`, `Ctrl+Alt+W`).

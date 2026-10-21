---
description: "Moving from TallyPrime to KhataOS, free and open source accounting software: the keys you know, what works differently, and how to import your data from Tally's XML export."
---

# Coming from Tally

**What you'll do:** see how KhataOS, a free and open source alternative to TallyPrime, compares with what you know: which of your habits work as they are, which work a little differently, and what isn't there yet, so the first week holds no surprises.

KhataOS is an independent project, not a copy of Tally, and it is not affiliated with, endorsed by or sponsored by Tally Solutions. It reproduces the *behaviour* Tally users rely on, such as the keys and the workflow, and it can [import your Tally data](import-from-tally.md). Nothing here comes from Tally's code, text, graphics or branding; the screens are KhataOS's own design, arranged around the keyboard conventions Tally users know.

## Habits that work as they are

| Your habit | In KhataOS |
|---|---|
| Single letters on the home menu | The same letters: `C` Create, `A` Alter, `H` Chart of Accounts, `V` Vouchers, `K` Day Book, `N` Banking, `B` Balance Sheet, `P` Profit & Loss, `S` Stock Summary, `R` Ratio Analysis, `D` more reports, `O` Dashboard, `Q` Quit |
| Voucher keys | `F4` Contra, `F5` Payment, `F6` Receipt, `F7` Journal, `F8` Sales, `F9` Purchase, `Alt+F5` Debit Note, `Alt+F6` Credit Note, `Alt+F7` Stock Journal |
| Accept and go back | `Ctrl+A` saves, `Esc` goes back, `Enter` moves to the next field |
| Go To | `Alt+G`, then type part of a report, voucher type or ledger name |
| Change mode in a voucher | `Ctrl+H`: double entry, single entry, item invoice |
| Repeat the last narration | `Ctrl+R` for this voucher type, `Alt+R` for a ledger |
| Create a master from a field | `Alt+C` in a ledger or item field; `Ctrl+Enter` alters the one under the cursor |
| Calculator | `Ctrl+N`, or `Alt+C` in an amount field |
| Bill-wise details | A party line opens bill-wise details: new reference, against reference, advance, on account |
| Drill down | `Enter` on a report row goes one level down, to the voucher in the end. `Esc` comes back to the same row |
| Date and period | `F2` and `Alt+F2`, on the home menu (for the company) or in a report (for that report) |
| Cancel and delete a voucher | `Alt+X` cancels and keeps the number; `Alt+D` deletes, after asking |
| Detailed Stock Summary | `Alt+F5` (or `Alt+F1`) |
| Company, import, exchange and share menus | `Alt+K`, `Alt+O`, `Alt+Z`, `Alt+M` |
| Display language | `Ctrl+K` (English and Arabic) |
| Voucher numbering | Automatic per voucher type, restarting each year or month, with prefix and suffix |
| Edit log you can't switch off | Every change is kept, always. See below |

The full list is in the [key reference](keyboard.md#key-reference).

## What's different

- **Your company is one file.** Each company is a single `.khata` file on your computer. Back it up with `Alt+Y` → **Backup**, as in Tally (see [Backup](backup.md)). There is no data folder with many files.
- **One company open at a time.** `F3` closes the company and shows the company list, rather than switching between several loaded companies.
- **The country is picked once.** When a company is opened for the first time, you pick its country. That sets the currency, the financial year and the tax rules. See [Your first company](first-company.md).
- **No closing or splitting at the year end.** The books are one continuous record, so balances carry forward by themselves. Move to the new year with `Alt+F2`.
- **The edit log is the database.** Every change is a linked entry that shows any tampering, and it can't be switched off or edited. There is no screen to browse it yet (`Alt+Q` is reserved).
- **`V` on the home menu opens a Payment voucher.** Press `F4` to `F9` to switch type. The keys for voucher types work inside voucher entry, not on the home menu.
- **Voucher numbers always come from the series.** You can't type a number by hand on the voucher screen. Set prefixes, suffixes and restarts per voucher type instead (`A`, **Voucher Type**).
- **Payment, Receipt and Contra open in single entry**, and Sales and Purchase open as an item invoice when the company has stock items. `Ctrl+H` switches. There is no separate "accounting invoice" mode: use the voucher mode with a sales ledger line.
- **`Alt+R` looks at the field you're in.** On a ledger line it repeats the last narration for that ledger; anywhere else it uses the voucher's first ledger. If the narration already has different text, it asks before replacing it.
- **Changing saved vouchers needs the Accountant role.** A Data Entry user can only enter new ones.
- **Find (`Alt+F`) searches ledgers** by any part of the name. Go To (`Alt+G`) searches everything else.
- **Every party line asks for bill-wise details.** There is no switch on a ledger to turn bill-wise off.
- **The Trial Balance is a flat list of ledgers**, with no view by group or detailed view yet. Profit & Loss and the Balance Sheet drill into groups.
- **Printing goes through your system's print dialog**, on A4. Choose "Save as PDF" there for a PDF. Export is CSV, not Excel.
- **On a Mac**, `Ctrl` is the Control key and `Alt` is Option, function keys need `fn`, and Command shortcuts stay with macOS. See [Keyboard](keyboard.md#on-a-mac).
- **Sharing works without remote desktop tools.** Colleagues connect to the office PC over the network, your accountant connects through an encrypted relay, or you send a sealed file. See [Sharing](sharing.md).
- **A few keys are KhataOS's own:** `T` on the home menu opens the [Desk](desk.md) (to-dos and due dates), `Alt+W` adds a note to the record under the cursor, and `Ctrl+Alt+S` syncs now.
- **Tax returns are prepared, not filed**, in most countries. KhataOS builds the return figures and files from your books. Direct filing exists only where the country page says so.

## Not there yet

Pressing a reserved key for one of these says "isn't available in this version yet", or does nothing. The lists come from our gap studies ([accounting](../research/tally-gap-accounting.md), [inventory, payroll and tax](../research/tally-gap-inventory-payroll-tax.md), [user experience](../research/tally-gap-ux-platform.md)).

**Vouchers**

- Sales and Purchase Orders, Delivery and Receipt Notes, Rejections In and Out, Physical Stock, Payroll (`Ctrl+F8`, `Ctrl+F9`, `Alt+F8`, `Alt+F9`, `Ctrl+F6`, `Ctrl+F5`, `Ctrl+F7`, `Ctrl+F4`).
- Optional, postdated, memorandum and reversing vouchers (`Ctrl+L`, `Ctrl+T`).
- Duplicate or insert a voucher from the Day Book (`Alt+2`, `Alt+I`).
- Unused voucher numbers (`Ctrl+Alt+U`), stock query (`Alt+S`).
- Custom voucher types with their own series (imported custom types become their base type), and manual voucher numbers.
- Interest calculation.

**Masters**

- Cost categories and cost centres, budgets and scenarios.
- New primary groups (subgroups work), and group options such as making a group behave like a subledger.
- Opening balances bill by bill on party ledgers.
- Compound and alternate units, price lists, standard rates, reorder levels, batch expiry dates, bill of materials.
- Bank details and cheque books on bank ledgers.
- A currencies master, a table of exchange rates and revaluation at the year end (ledgers and vouchers in a foreign currency do work; see [Invoices](invoices.md#foreign-currency)).

**Reports**

- Filters (`Ctrl+F`), extra columns and comparisons (`Alt+C`, `Alt+N`), saved views (`Ctrl+L`), `F4` change report, `Ctrl+B`, `Ctrl+J`, hide lines.
- Trial Balance, Balance Sheet and P&L by group and in detail (`Alt+F5` works on Stock Summary only).
- Sales, purchase and journal registers, ledger monthly summary, cash and bank books, cash flow and funds flow.
- Exception reports, the edit log viewer (`Alt+Q`), statistics.
- Order, stock ageing, movement and reorder reports.

**Everything else**

- Repair, split and Tally-style data verification in the Data menu (`Alt+Y` has Backup, Restore and Automatic backups).
- Help (`F1`), company features (`F11`), configuration (`F12`), Switch To (`Ctrl+G`), notifications.
- Email and WhatsApp (`Ctrl+M`, `Ctrl+Alt+W`), the print and export menus (`Alt+P`, `Alt+E` outside the Day Book), print preview and paper sizes other than A4.
- Import from Excel, export to Excel or XML, live import from a running Tally.
- Several companies open at once, and group companies.
- Custom security levels, rights for each voucher type, locking the books up to a date, and encrypting the company file.
- Payroll, PF, ESI and PT.
- Live connection to the GST portal for filing.

What's planned next is in the [roadmap](../roadmap.md).

## Your first week

1. [Install KhataOS](install.md) and [create a test company](first-company.md).
2. [Import your Tally data](import-from-tally.md) into it, and read the skipped list.
3. Check the tax ID and state on your main party ledgers. The import brings them across when Tally has them; the import summary lists any GSTIN it couldn't verify.
4. Enter a few days of vouchers in both, and compare the Day Book, Trial Balance and outstandings.
5. [Tell us](https://github.com/khataos/khataos/issues/new/choose) what's missing or behaves differently from what you expect.

---

"Tally", "TallyPrime" and "Tally.ERP 9" are trademarks of Tally Solutions Pvt. Ltd. KhataOS is an independent project, not affiliated with, endorsed by or sponsored by Tally Solutions. The names are used only to describe compatibility.

# Import from Tally

**What you'll do:** export your masters and Day Book from Tally as XML, then bring both into a KhataOS company in one go. You'll then check the list of anything that was skipped.

> KhataOS is in alpha. Import into a **test company** first, and keep using Tally for your real books until you have checked the result.

## Before you start

- Create the KhataOS company and pick its country first. The import menu works only in an open company with a country set. See [Create your first company](first-company.md).
- If the company has users, sign in as Owner, Accountant or Data Entry. An Auditor can't import, and neither can anyone on a company shared with them read-only (as a Viewer).
- There is no undo. The import is saved as soon as it finishes, and it is recorded in the audit trail. A fresh test company is the easy way to try again.

## 1. Export from Tally

You need two XML files from the same Tally company:

1. **All masters**: the List of Accounts export with all masters (groups, ledgers, voucher types and inventory masters), in XML (Data Interchange) format.
2. **Day Book**: the Day Book for the whole period you want, in XML (Data Interchange) format, with the voucher details included.

These work from TallyPrime and from Tally.ERP 9. UTF-8 and Unicode (UTF-16) files both work.

Export both, because they depend on each other. Custom voucher types are traced back to their base type through the masters file. Without it, those vouchers are skipped as "unknown voucher type".

## 2. Import into KhataOS

1. Open the company.
2. Press `Alt+O` (Import). You can also press `Alt+G` (Go To) and type "import".
3. Choose **Import from Tally XML**.
4. In the file dialog, select **both** XML files together (hold `Ctrl`, or `Cmd` on a Mac, and click each one). The order doesn't matter.
5. Wait for "Importing 2 file(s)…" to finish. The **Tally Import** result screen opens.

If any file is empty, isn't Tally XML, or is cut short, nothing is imported and the company is left as it was.

## 3. Read the result

The result screen shows:

- how many groups, ledgers, opening balances, vouchers, units, stock groups, stock items, godowns and stock lines came in;
- **Vouchers numbered by KhataOS**: vouchers that had no number in Tally and got the next KhataOS number;
- **Skipped**: every item that was left out, with its kind, its name and the reason.

Fix skipped items in Tally and import again, or create them in KhataOS by hand. Press `Esc` to leave the screen.

## What comes across

| Tally data | In KhataOS |
|---|---|
| Predefined groups | Matched to the KhataOS default groups (not created twice) |
| Your own groups | Created under their parent group |
| Ledgers | Name, aliases, group and opening balance |
| Units | Simple units, with their decimal places |
| Stock groups | Created under their parent |
| Stock items | Group, unit, HSN code, costing method (FIFO and last purchase cost are kept; others become average cost), opening quantity and value |
| Godowns | Created under their parent. Tally's main location is KhataOS's main location |
| Vouchers | Contra, Payment, Receipt, Journal, Sales, Purchase, Credit Note and Debit Note, with date, number, narration and ledger lines. Custom voucher types become their base type |
| Item invoices | Stock lines on Sales, Purchase, Credit Note and Debit Note, with godown and batch names |
| Voucher numbers | Kept as they are |

## What doesn't come across (yet)

- **Ledger details:** GSTIN or other tax ID, address, state, credit period, bank details and the bill-wise setting. Add the tax ID and state on each party ledger after the import (see [Masters](masters.md)).
- **Bill-wise allocations:** outstanding bills don't come across, so **Outstanding and Ageing reports start empty**. Opening bills on ledgers can't be entered yet either.
- **Cost centres and categories**, bank allocations (cheque details) and the GST or other tax detail stored on vouchers.
- **Foreign currency:** only the base-currency amount is kept.
- **Custom groups placed directly under Primary.** Recreate them under one of the default groups.
- **Compound units**, and the stock items that use them.
- **Opening stock by godown or batch:** it comes in as one total at the main location, with a note.
- **Stock vouchers:** Stock Journal, Manufacturing Journal, Delivery Note, Receipt Note, Rejections In and Out, Physical Stock, Material In and Out.
- **Other vouchers:** Sales and Purchase Orders, Memorandum, Reversing Journals, Job Work orders, Payroll and Attendance.
- **Cancelled and optional vouchers.**
- **Stock lines on other voucher types:** if a Journal or Payment carries stock lines, the voucher is posted without them, with a note.
- **Live import from a running Tally** (over its network port). The importer library can talk to it, but the app doesn't offer it yet. Use the XML files.

## Importing again

- Masters that already exist are reused, not created twice. Existing ledgers keep their opening balance.
- A voucher whose number is already used for the same type in the same numbering period is skipped as a duplicate.
- **A voucher with no number is posted again** with a new KhataOS number, so it appears twice. Import into a fresh company if you need to start over.

## Common skip reasons

| Reason in the list | What it means | What to do |
|---|---|---|
| Unknown voucher type | The masters file wasn't imported with the Day Book | Import both files together |
| Unknown ledger | A voucher uses a ledger that isn't in the files | Include all masters in the export |
| Custom primary group | Your group sits directly under Primary | Create it under a default group |
| Duplicate ledger / stock item | That name already exists here | Nothing, or rename one of them |
| Duplicate number | The number is already used in that series | Check the series, or renumber in Tally |
| Unbalanced | The voucher's debits and credits differ | Fix it in Tally |
| Contra uses a non-cash ledger | A Contra must use only cash and bank ledgers | Change it to a Journal in Tally |

Developers: the importer is the [`khata-import-tally`](../../crates/khata-import-tally) crate.

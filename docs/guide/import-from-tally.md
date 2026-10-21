# Import from Tally

**What you'll do:** export your masters and Day Book from Tally as XML, then bring them into a KhataOS company in one go. You'll then read the import summary, which says what came in, what didn't and why, and whether the balances match Tally's.

> KhataOS is in alpha. Import into a **new test company** first, and keep using Tally for your real books until you have checked the result.

## Before you import real data

Work through this list the first time you move a real company. It takes about an hour and catches the problems that matter.

**In TallyPrime**

1. Note the company's **books beginning date** (`Alt+K`, Company, Alter).
2. Write down, for the last date you'll export: the **Trial Balance** totals and the closing balances of cash, every bank, cash credit and overdraft account, and your largest parties; the totals of **Bills Receivable** and **Bills Payable** (Outstandings); and the number of vouchers of each type from the **Statistics** report (`Alt+G`, then type Statistics).
3. Export **all masters** as XML (step 1 below), including voucher types.
4. Export the **Day Book from the books beginning date** to the last date, not just recent months. A Day Book that starts later gives wrong closing balances even though the Trial Balance still agrees, because Tally's opening balances are as at the books beginning date.
5. Export the **Trial Balance** for the same last date as XML, shown ledger by ledger. KhataOS compares its closing balances with it.

**In KhataOS**

6. Create a **new company** for the import, with the country set to India. Keep the suggested financial year (April to March) and the suggested numbering, which restarts every financial year as Tally's usually does. See [Create your first company](first-company.md).
7. **Import all three files together** on a computer (step 2). Phones refuse files over 200 MB.
8. **Read the summary from the top** (step 3). Anything under **Warnings** means balances may not match Tally. Then check the vouchers by type against Tally's Statistics, the skipped list and the notes.
9. **Compare with Tally:** the Trial Balance and the "Difference in opening balances" row; Outstandings and Ageing for receivables and payables; the cash, bank, cash credit and overdraft balances; and, if you're GST registered, one month's GSTR-1 against Tally's.
10. **Save the shareable summary** (step 4) and send it to the KhataOS maintainers if anything looks wrong. It has counts only, with no names, amounts or dates.
11. To try again, delete the test company and start from step 6. Don't import the same files twice into one company.

## Before you start

- The import works only in an open company with a country set.
- If the company has users, sign in as Owner, Accountant or Data Entry. An Auditor can't import, and neither can a Viewer.
- Import into a new, empty company. Into a company that already has books, KhataOS asks first and takes an automatic [backup](backup.md) before it starts; if the import goes wrong, `Alt+Y` → **Restore** brings that backup back. The import itself has no undo: it is saved as soon as it finishes, and it is recorded in the audit trail.
- An import never half applies. If any file is empty, isn't Tally XML or is cut short, nothing is imported. If saving fails at the end, the company is left as it was.

## 1. Export from Tally

You need these XML files from the same Tally company. In TallyPrime, the **Export** menu is `Alt+E`; menu names vary a little between releases.

1. **All masters:** Export, Masters, with **All Masters** and the file format **XML (Data Interchange)**. This includes groups, ledgers (with their bills, GSTIN, state and tax details), voucher types and the inventory masters.
2. **Day Book:** open the Day Book, set the period (`F2`) from the books beginning date to the last date, then Export, Current, **XML (Data Interchange)**.
3. **Trial Balance (recommended):** open the Trial Balance for the same last date, show it ledger by ledger (`Alt+F5`, Detailed), then Export, Current, **XML (Data Interchange)**. A Trial Balance by group also works; KhataOS then compares group totals.

These work from TallyPrime and from Tally.ERP 9. UTF-8 and Unicode (UTF-16) files both work. Avoid the ASCII option if any name uses an Indian script.

Export the masters and the Day Book together, because they depend on each other. Custom voucher types are traced back to their base type through the masters file. Without it, those vouchers are skipped as "unknown voucher type".

## 2. Import into KhataOS

1. Open the company.
2. Press `Alt+O` (Import). You can also press `Alt+G` (Go To) and type "import".
3. Choose **Import from Tally XML**.
4. In the file dialog, select **all the files** together (hold `Ctrl`, or `Cmd` on a Mac, and click each one). The order doesn't matter.
5. Wait for the import to finish. A large Day Book takes a little while; the window keeps working meanwhile. The **Tally Import** summary opens.

## 3. Read the summary

The summary shows, from the top:

- **Warnings**, when anything means balances may not match Tally: a Day Book that starts well after the start of its financial year, no opening balances, opening balances that don't add up, or closing balances that differ from Tally's Trial Balance.
- **Imported**: groups, ledgers, opening balances, vouchers posted, vouchers posted as cancelled, vouchers numbered by KhataOS (they had no number in Tally), units, stock groups, stock items, godowns and stock lines.
- **Vouchers by Tally voucher type**: for each of your voucher types (for example "Sales GST"), the KhataOS type it was posted as, and how many were posted, cancelled and skipped. Compare these with Tally's Statistics.
- **Bills**: new bills, settlements, advances, on account amounts and opening bills.
- **GST and party details**: GSTINs, states, PANs, addresses, tax ledgers tagged with their tax, HSN or SAC codes and their rates.
- **Checks**: the Day Book's first and last dates, the difference in opening balances, and how many closing balances were compared with Tally's and how many differ, with the differing ledgers listed.
- **Notes**: things imported with a caveat, for example a voucher given a new number, a GSTIN that fails the check, or a tax ledger whose input or output side was guessed.
- **Skipped**: every item that was left out, with its kind, name and reason.

## 4. Save the report

- **Save report** saves the whole summary as a CSV or text file, with names and amounts. Keep it with your migration notes.
- **Save shareable summary** saves counts only: no ledger, party or voucher type names, no amounts, no dates and no file names. Send this one when you ask the KhataOS maintainers for help.

Press `Esc` to leave the summary.

## What comes across

| Tally data | In KhataOS |
|---|---|
| Predefined groups | Matched to the KhataOS default groups (not created twice) |
| Your own groups | Created under their parent group |
| Ledgers | Name, aliases, group and opening balance |
| Opening bills on party ledgers | Each bill with its name, date, due date and amount, so Outstandings and Ageing start from your real bills. Any part of the opening balance the bills don't cover shows as "Opening Balance" |
| Party details | GSTIN, state, PAN, address, PIN code, phone, email and credit period, from older and newer TallyPrime layouts. A GSTIN that fails the check is kept with a note |
| GST tax ledgers | Tagged as CGST, SGST, UTGST, IGST or cess, input or output, so GST reports find the tax lines |
| HSN and SAC codes | On stock items and on sales, purchase and service ledgers, with their GST rate from the date it applies |
| Bank, cash credit and overdraft accounts | Ledgers under Bank Accounts, Bank OCC A/c and Bank OD A/c all work in Contra, Payment and Receipt, and in bank reconciliation |
| Units | Simple units, with their decimal places |
| Stock groups and godowns | Created under their parent. Tally's main location is KhataOS's main location |
| Stock items | Group, unit, HSN code, costing method (FIFO and last purchase cost are kept; others become average cost), opening quantity and value |
| Vouchers | Contra, Payment, Receipt, Journal, Sales, Purchase, Credit Note and Debit Note, with date, number, narration and ledger lines |
| Custom voucher types | Posted as their base type, keeping their own name as the voucher's number series. "Sales GST 1" and "Cash Sales 1" can both exist, as in Tally |
| Bill allocations | New Ref, Agst Ref, Advance and On Account on voucher lines, with due dates from the credit period |
| Bank dates | The date the bank cleared a cheque, for bank reconciliation |
| Cancelled vouchers | Kept as cancelled, keeping their numbers, with no effect on balances |
| Post-dated vouchers | Posted on their date, with a note. Reports up to an earlier date leave them out |
| Item invoices | Stock lines on Sales, Purchase, Credit Note and Debit Note, with godown and batch names |

**Voucher numbers** are kept as they are. If Tally restarts numbering every financial year and the company's numbering doesn't, KhataOS makes it restart yearly and says so. If Tally used the same number twice in one voucher type and year (its duplicate check was off), the second one is posted as, for example, "12 (2)", with a note.

## What doesn't come across (yet)

- **Cost centres and categories:** the ledger amounts come in; the split by cost centre doesn't. The summary counts the lines affected.
- **Bank allocation details** other than the bank date: instrument number and type.
- **GST and other tax detail stored on vouchers** (the tax analysis), supplier invoice numbers and e-way bill details.
- **Registration type** (regular, composition, consumer): KhataOS treats a party with a GSTIN as registered and one without as unregistered.
- **GST classifications:** a master that takes its rate from a classification is listed in the notes; set its rate by hand.
- **Foreign currency:** only the amount in the base currency is kept.
- **Custom groups placed directly under Primary.** Recreate them under one of the default groups.
- **Compound units**, and the stock items that use them.
- **Opening stock by godown or batch:** it comes in as one total at the main location, with a note.
- **Stock vouchers:** Stock Journal, Manufacturing Journal, Delivery Note, Receipt Note, Rejections In and Out, Physical Stock, Material In and Out.
- **Other vouchers:** Sales and Purchase Orders, Memorandum, Reversing Journals, Job Work orders, Payroll, Attendance, and optional vouchers.
- **Live import from a running Tally** (over its network port). The importer library can talk to it, but the app doesn't offer it yet. Use the XML files.

## Importing again

- Masters that already exist are reused, not created twice. Existing ledgers keep their opening balance and details.
- A numbered voucher that is already in the company from before is skipped as "already in the company".
- **A voucher with no number is posted again** with a new KhataOS number, so it appears twice. Import into a fresh company if you need to start over.

## Common skip reasons and notes

| In the summary | What it means | What to do |
|---|---|---|
| The Day Book starts well after the start of its financial year | The Day Book doesn't begin at the books beginning date | Export the Day Book from the books beginning date and import again into a fresh company |
| Closing balances that differ from Tally's | A ledger's balance here isn't the one in Tally's Trial Balance | Look for skipped vouchers on that ledger, and check the Day Book's period |
| Unknown voucher type | The masters file wasn't imported with the Day Book | Import the files together |
| Unknown ledger | A voucher uses a ledger that isn't in the files | Include all masters in the export |
| Custom primary group | Your group sits directly under Primary | Create it under a default group |
| Duplicate ledger or stock item | That name already exists here | Nothing, or rename one of them |
| Already in the company | The same voucher was imported before | Nothing; to start over, use a fresh company |
| Unbalanced | The voucher's debits and credits differ | Fix it in Tally |
| Contra with a ledger that is not cash or bank | A Contra must use only cash, bank, cash credit and overdraft ledgers | Change it to a Journal in Tally |
| Tax ledger side guessed | One tax ledger is used on both sales and purchases | Check it under Alter Ledger; GST reports still count it by voucher type |

Developers: the importer is the [`khata-import-tally`](../../crates/khata-import-tally) crate. What a real export may still trip on is in the [readiness review](../research/tally-import-readiness.md).

---

"Tally", "TallyPrime" and "Tally.ERP 9" are trademarks of Tally Solutions Pvt. Ltd. KhataOS is an independent project, not affiliated with, endorsed by or sponsored by Tally Solutions. The import reads the XML that Tally's own Export menu writes; the names are used only to describe that compatibility.

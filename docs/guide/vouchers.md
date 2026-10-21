# Vouchers

**What you'll do:** enter vouchers in each mode, add bill-wise details, then open a saved voucher to change, cancel, delete or print it. You'll also set how vouchers are numbered.

## Open voucher entry

1. On the home menu, press `V` (Vouchers). A **Payment** voucher opens. Or press `Alt+G` (Go To) and type the voucher type, for example "sales voucher".
2. Switch type with a function key. These work inside voucher entry, not on the home menu:

| Key | Voucher |
|---|---|
| `F4` | Contra |
| `F5` | Payment |
| `F6` | Receipt |
| `F7` | Journal |
| `F8` | Sales |
| `F9` | Purchase |
| `Alt+F5` | Debit Note |
| `Alt+F6` | Credit Note |
| `Alt+F7` | Stock Journal |

If you've started typing, KhataOS asks before discarding the voucher. Sales and Purchase Orders, Delivery and Receipt Notes, Rejections, Physical Stock, Payroll and the `F10` list of other voucher types aren't available yet.

## Enter a voucher

The top shows the voucher type, its number and its date.

- **Date:** press `F2` to change it. New vouchers start on the current date (see [Reports](reports.md#set-the-current-date-and-period-for-the-whole-company)).
- **Number:** comes from the voucher type's series. You can't type your own number on screen; see [Numbering](#numbering).

### Payment, Receipt and Contra (single entry)

These open in single entry: one cash or bank account at the top, and the other ledgers below.

1. **Account:** pick the cash or bank ledger.
2. **Particulars:** pick a ledger, then type the **Amount**. Repeat for more lines.
3. Pick **End of List** when you're done.
4. Type the **Narration** and press `Enter`. Answer `Y` to "Accept?", or press `Ctrl+A` at any time to save straight away.

Press `Ctrl+H` to switch to double entry.

### Journal, and double entry for any type

1. For each line, set **Dr** or **Cr** (press `D` or `C`, or `Space` to switch), pick the ledger, and type the amount.
2. KhataOS picks the side of the next line and fills in the difference for you.
3. When debits equal credits, `Enter` goes to **End of List**, then to the narration.
4. Press `Ctrl+A` to save.

Journals use double entry only.

### Sales, Purchase, Credit Note and Debit Note (item invoice)

If the company has stock items, these open as an **item invoice**. Otherwise they open as a double entry voucher.

1. **Party:** pick the customer or supplier. For a cash sale, pick a cash or bank ledger.
2. **Sales ledger** (or Purchase ledger).
3. For each item: **Name of item**, **Godown** (if you have godowns), **Quantity** and **Rate**. The amount is worked out; typing the amount works out the rate instead.
   - The rate starts at the last rate for that item, preferring the same party.
   - The line shows the closing stock. If stock would go below zero, you see a warning, but you can still save.
   - `Ctrl+I` shows or hides a **Batch** column.
4. Pick **End of List** after the last item.
5. **Other ledgers:** tax lines are filled in for you from your country's rules (for India, CGST and SGST within the state or IGST between states). Add discounts (as a minus amount), freight and so on. Typing over a tax line makes it your own.
6. **Bill-wise details** open for the party (see below).
7. Type the narration and accept.

Press `Ctrl+H` to switch between the item invoice and a plain voucher.

If a tax line can't be filled in, the screen says why: for example, there's no duty ledger for that tax yet (create one under Duties & Taxes), or the company's tax ID isn't set (`Alt+K`, Company details).

**India:** TDS is suggested on Journal, Purchase and Payment, and TCS on Sales and Receipt, when the ledgers are set up for it. See [India](gst-india.md).

### Keys while entering

| Key | What it does |
|---|---|
| `Enter` | Next field |
| `↑` or `Shift+Tab` | Previous field |
| `Backspace` | In an empty narration, goes back a field |
| `Esc` | Clears the field; pressed again, asks "Quit without saving?" |
| `Ctrl+A` | Save |
| `Ctrl+D` | Remove the line under the cursor |
| `Ctrl+R` | Repeat the narration of the last voucher of this type |
| `Alt+R` | Repeat the narration of the last voucher, of any type, for a ledger: the one in the field under the cursor, else the voucher's first ledger (the party on an invoice) |
| `Alt+C` | In a ledger, item or godown field: create a new one. In an amount field: the calculator |
| `Ctrl+Enter` | Alter the ledger, item or godown highlighted in the list |
| `Ctrl+N` | Calculator |
| `F2` | Voucher date |
| `Ctrl+H` | Change mode |

After a save, a fresh voucher of the same type opens, with the same account or sales ledger.

## Bill-wise details

Lines for party ledgers (under Sundry Debtors or Sundry Creditors) always ask for bill-wise details. They feed Outstanding and Ageing.

1. The **Bill-wise details** window opens after the party's amount (or after the items on an invoice).
2. Pick the **Type** for each row:
   - **New Ref**: a new bill. Give it a name, usually the invoice number. Its due date starts at the voucher date plus the party's credit period.
   - **Agst Ref**: settles an open bill. Pick it from the party's open bills.
   - **Advance**: money paid or received ahead of a bill, with a due date.
   - **On Account**: not tied to any bill.
3. Type each amount. The bottom shows how much is allocated and what's left.
4. When the rows add up to the line, press `Enter` on the last amount, or `Ctrl+A`.

`Backspace` on the first row's Type goes back to the voucher. `Esc` asks before leaving without changes.

## Foreign currency

A line on a ledger kept in a foreign currency asks for a **Rate of exchange** (for example 1 USD = … INR), filled in with the last rate you used. Type the amount in the foreign currency; the base amount is worked out as you type. One voucher can use one foreign currency. When a foreign bill is settled at a different rate, the gain or loss goes to Forex Gain/Loss when you save. See [Invoices](invoices.md#foreign-currency).

## Change a saved voucher

1. Open it: `Enter` on its row in the Day Book (`K`), a ledger statement, or another list.
2. The title says **(Alteration)**. Change what you need.
3. Press `Ctrl+A`. KhataOS returns to where you came from.

A saved voucher keeps its type: to change the type, delete it and enter a new one. Changing a saved voucher needs an Accountant (a Data Entry user can only enter new ones). Every change is kept in the audit trail.

## Cancel or delete a voucher

**Cancel** keeps the voucher and its number in the series, but it no longer counts in any balance. Use it for a spoilt invoice, so the series has no gaps.

1. Open the voucher, or put the cursor on its row in the Day Book.
2. Press `Alt+X` and answer `Y`.

A cancelled voucher shows in the Day Book as "(Cancelled)" and opens for reading only.

**Delete** removes the voucher. The audit trail keeps a full copy.

1. Open the voucher.
2. Press `Alt+D` and answer `Y`.

Both need an Accountant.

## Print a voucher or a cheque

- After saving a Sales invoice, Credit Note or Debit Note, KhataOS asks "Print?".
- After saving a Payment from a bank account, it asks "Print cheque?", then whether to cross it "A/c Payee". Cheques print on the standard CTS-2010 layout.
- On any saved voucher, press `Ctrl+P` to print it (for a bank payment, choose Cheque or Voucher).
- Press `Ctrl+E` (or `Alt+E`) on a saved voucher for the **Export** menu: a PDF, your country's e-invoice files, and ship-to and export details. See [Invoices](invoices.md).

## Add a note

On a saved voucher, press `Alt+W` to add a note or to-do. It shows under the narration and on the [Desk](desk.md).

## Numbering

Each voucher type has its own series.

1. On the home menu, press `A` (Alter), then **Voucher Type**.
2. Pick the type. The list shows each type's next number.
3. Set:
   - **Restart numbering:** never, every financial year, or every month;
   - **Prefix** and **Suffix**, up to 16 characters each. They can use `{FY}` (such as 26-27), `{YYYY}`, `{YY}` and `{MM}`;
   - **Pad to digits:** for example 4 gives 0001.
4. Check the **Next number** preview, and press `Ctrl+A`.

New companies restart every type each financial year, and Sales, Credit Note and Debit Note carry the year, such as `26-27/1`. Numbers are unique within the series period. Only an Owner can change numbering. Stock Journal numbering can't be changed yet.

**India:** a sales invoice number must be 16 characters or fewer, use only letters, digits, `-` and `/`, and be unique within the financial year (GST Rule 46).

## Stock Journal

Use it to move stock between godowns, or to record consumption and production.

1. In voucher entry, press `Alt+F7`.
2. Under **Source**, enter what goes out: item, godown, quantity. It is valued at average cost.
3. Pick End of List. Under **Destination**, enter what comes in. A single destination line takes the whole value.
4. Type the narration and press `Ctrl+A`.

## Not built yet

These keys that Tally users know are reserved, and pressing them says so: postdated (`Ctrl+T`) and optional (`Ctrl+L`) vouchers, stock query (`Alt+S`), tax analysis (`Alt+A`), related reports (`Ctrl+O`), unused numbers (`Ctrl+Alt+U`), the edit log (`Alt+Q`) and statutory adjustment (`Alt+J`). `PgUp` and `PgDn` don't move between saved vouchers yet, and `Ctrl+F` (autofill) does nothing. Email and WhatsApp aren't available yet. Memorandum and reversing journals, voucher classes, custom voucher types and cost centres aren't there either.

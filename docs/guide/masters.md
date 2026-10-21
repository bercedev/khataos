# Masters

**What you'll do:** create and change the records your vouchers use: ledgers, groups, stock items, stock groups, units, godowns and tax rates. This page also covers the Banking menu.

## Create or alter a master

**Create:**

1. On the home menu, press `C` (Create).
2. Pick the master: **Ledger** or **Group** (accounting), **Stock Item**, **Stock Group**, **Unit** or **Godown** (inventory), or **Tax Rates**.
3. Fill in the fields. `Enter` moves to the next field, and `↑` or `Shift+Tab` goes back.
4. Press `Ctrl+A` to save, or `Enter` past the last field and answer `Y` to "Accept?".

After saving, the form clears for the next record and keeps the last group or unit you used.

**Alter:**

1. On the home menu, press `A` (Alter).
2. Pick the kind of master. A list opens; type to filter it.
3. Press `Enter` on the record, change it, and press `Ctrl+A`.

`Alt+G` (Go To) also has "Create Ledger" and "Create Group".

**Keys in master forms:**

- `Esc` clears the field you're in; pressed again, it asks "Quit without saving?" if you changed anything.
- `Alt+C` in a list field (such as **Under**) creates a new record for it without leaving the form.
- `Alt+D` deletes the record you're altering, after asking. A master that is in use can't be deleted (see below).

Who can do what: a Data Entry user can create masters; altering and deleting need an Accountant; tax rates need an Owner.

## Ledgers

1. Press `C`, then **Ledger**.
2. **Name.**
3. **Under**: the group. Type to filter, or press `Alt+C` to create a group.
4. The rest of the form depends on the group:

**Party ledgers** (under Sundry Debtors or Sundry Creditors):

- **Currency**: leave blank for the company's own currency, or type an ISO code such as `USD` for a party you bill in another currency. See [Invoices](invoices.md#foreign-currency).
- **Default credit period**: days until a new bill is due.
- **Tax ID** (GSTIN, TRN, VAT number… named for your country). It is checked as you type. If it is invalid, you can't save until you fix it or type a **Reason to keep it**, which goes into the audit trail. Keeping a reason needs an Accountant.
- **India:** PAN, TDS and TCS details, and for a party abroad the terms for non-residents.
- **Mailing details**: address, country, state, postal code, phone and email. A **Country** other than your company's makes the party "abroad", so sales to it count as exports. India asks whether the party is an SEZ unit; the UAE asks about designated zones.
- **Opening balance** and `Dr` or `Cr`. Press `D` or `C`, or `Space`, to switch sides.

**Cash and bank ledgers:** name, group, currency and opening balance.

**Duty ledgers** (under Duties & Taxes): pick the **Type of tax** (for example CGST, SGST, IGST, VAT or GST) and whether it is used on sales or purchases. Invoices fill tax into these ledgers. India also has TDS payable and TCS payable.

**Sales, purchase, income and expense ledgers:** an optional **HSN/SAC or tax category**, so tax is worked out on lines that use this ledger. India also asks for the TDS or TCS nature.

The side panel shows whether the company's opening balances agree.

**Not there yet:** ledger aliases, bank account details on the ledger, and opening balances bill by bill. (For bank details on invoices, see [Invoices](invoices.md).) A ledger used by any voucher can't be deleted.

## Groups

A new company comes with the standard groups (Capital Account, Current Assets, Sundry Debtors, Duties & Taxes, Sales Accounts and so on).

1. Press `C`, then **Group**.
2. Type the **Name**, and pick the parent group in **Under**.
3. Press `Ctrl+A`.

The new group takes its nature (asset, liability, income or expense) from its parent; the side panel shows it.

- **You can't create a new primary group** (one directly at the top). Create it under one of the standard groups.
- **You can rename any group**, including the standard ones. KhataOS finds them by an internal id, not by name. A primary group can be renamed but not moved.
- When altering, you can add an **Alias** (other names, separated by commas).
- A subgroup can move only under a group of the same nature.
- A primary group, or one with subgroups or ledgers, can't be deleted.

## Units

1. Press `C`, then **Unit**.
2. **Symbol** (for example `Nos` or `kg`), an optional **Formal name**, and the **Number of decimal places** (0 to 4; 0 for whole numbers).

Quantities are checked against the decimal places. Compound units (a box of 12 pieces) and alternate units aren't available yet.

## Stock groups

1. Press `C`, then **Stock Group**.
2. **Name**, and **Under** (another stock group, or Primary).

## Stock items

1. Press `C`, then **Stock Item**.
2. **Name**.
3. **Under**: the stock group (`Alt+C` creates one).
4. **Units** (`Alt+C` creates one).
5. **HSN/SAC or tax category**: the code that picks the tax rate from the Tax Rates list. The form shows the rate in force.
6. **Costing method**: Average cost, FIFO or Last purchase cost.
7. **Opening balance**: quantity, rate and value.

Opening stock is held at the main location; you can't put it in a godown yet. Price lists, reorder levels and batch expiry dates aren't available yet. An item with any stock movement can't be deleted.

## Godowns

1. Press `C`, then **Godown**.
2. **Name** (for example Warehouse or Shop), and **Under** (another godown, or Primary).

Once you have a godown, item lines in vouchers get a **Godown** column. Stock that isn't in a godown is at the **Main Location**.

## Tax rates

The rate list says which tax rate applies to each HSN/SAC code or tax category, and from which date.

1. Press `C`, then **Tax Rates** (or `Alt+K`, **Tax Rates**).
2. The list shows each code, the date it applies from (in force, upcoming or replaced) and the rate.
3. Press `Alt+C` to add a rate: the code, **Applicable from**, and the **Rate** in percent (0 for exempt or nil-rated). `Enter` on a row alters it, and `Alt+D` deletes it.

Changing a rate's code or date when altering adds a new rate change and keeps the old one, so older invoices keep their rate. An item uses the longest code in the list that its own code starts with. Only an Owner can change rates. US companies keep their sales tax jurisdictions here (see [US](us.md)).

## Chart of Accounts

Press `H` on the home menu. It shows every group and ledger as a tree, with each ledger's balance. `Enter` on a ledger opens its statement.

## Banking

Press `N` on the home menu.

### Reconcile a bank account

1. Choose **Bank Reconciliation**, then the bank ledger.
2. For each entry the bank has cleared, type the **Bank Date** and press `Enter`. A blank date clears it. The date can't be before the voucher date.
3. The bottom shows the balance as per your books, the amounts not yet in the bank, and the balance as per the bank.

`Ctrl+Enter` opens the voucher on the row. `F2` changes the period. Setting bank dates needs an Accountant.

### Import a bank statement

1. In Bank Reconciliation, press `Alt+S` (Import Statement) and pick the bank's CSV file.
2. KhataOS finds the date, description, reference, and withdrawal and deposit (or amount) columns by their headings, and matches each line to a voucher by amount, date and cheque or reference number.
3. Check the matches. `Space` ticks or unticks a line, `Ctrl+Space` all of them.
4. On a line with no match, `Enter` opens a new Receipt or Payment for it.
5. Press `Ctrl+A` to set the bank dates for the ticked lines.

Only CSV statements can be read; Excel and MT940 files can't yet.

### Cheque register

**Cheque Register** lists withdrawals from bank accounts that haven't cleared yet. Cheque printing is on a saved bank Payment (see [Vouchers](vouchers.md#print-a-voucher-or-a-cheque)).

# UK: VAT and Making Tax Digital

**What you'll do:** set up a UK company for VAT, pick your VAT scheme, prepare the 9-box return, and submit it to HMRC through Making Tax Digital (MTD).

> The return is a preparation aid. Check it before you submit. The MTD connection has been tested **only against a mock** of HMRC's API so far: it needs a tester with a registered HMRC developer application.

## Set up

1. Pick **United Kingdom** when the company first opens. The books are in pounds, with the financial year starting in April (the VAT periods come from HMRC or your stagger).
2. `Alt+K`, **Company details**: the company's **VAT registration number** (checked with HMRC's check digits; `GB` and Northern Ireland `XI` numbers work) and its nation.
3. **Tax rates** (`C`, **Tax Rates**): the standard (20%), reduced (5%) and zero (0%) classes you use, given to each stock item or sales ledger.
4. Create **Output VAT** and **Input VAT** ledgers under Duties & Taxes, with Type of tax **VAT**.
5. On party ledgers, add customers' and suppliers' VAT numbers. A party in another **Country** counts as abroad.

Reverse charge (imported services, the domestic reverse charge and construction), import VAT and postponed import VAT are handled through tax classes. A reverse-charge sale prints "Reverse charge: Customer to pay the VAT to HMRC." Detail: [UK VAT](../uk-vat.md).

## Choose your VAT scheme

1. `Alt+G`, **VAT Return (UK, 9 boxes)**.
2. Press `Ctrl+Alt+F` (VAT Schemes).
3. Choose standard accounting, **cash accounting** or the **flat rate scheme** (with your sector, percentage and the first-year 1% discount), and set limited cost business, turnover basis and annual accounting as they apply.
4. Accept.

The scheme settings stay in this company file on this computer; they don't sync.

## Prepare the return

1. `Alt+G`, **VAT Return (UK, 9 boxes)**.
2. `F2` sets the period.
3. Check the nine boxes. `Ctrl+P` prints, `Ctrl+E` saves a CSV, and `Ctrl+Alt+J` saves the MTD JSON body.

## Submit through MTD

**Connect once:**

1. `Alt+G`, **Making Tax Digital settings (UK)**: enter your own HMRC developer application's client ID and secret, choose sandbox or production, and the redirect port. These are kept on this computer.
2. `Alt+G`, **Making Tax Digital: VAT periods (UK)**, then `Ctrl+Alt+C` (Connect). Sign in to HMRC in your browser and grant access.

**Each period:**

1. In **Making Tax Digital: VAT periods (UK)**, the open periods (obligations) come from HMRC.
2. Press `Enter` on the open period to see its return.
3. Tick the declaration, and press `Ctrl+Alt+T` to submit.
4. Keep the receipt HMRC sends back.

`Ctrl+Alt+L` shows your VAT liabilities. In the sandbox, `Ctrl+Alt+H` tests the fraud-prevention headers.

The [Desk](desk.md) shows VAT, Corporation Tax, accounts and confirmation statement due dates once you set your stagger in the compliance profile.

## Not there yet

E-invoices (the UK has no mandate yet; invoices are PDF), payroll and RTI, and filing Corporation Tax or accounts.

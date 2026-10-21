# Create your first company

**What you'll do:** create a company file, pick the country it keeps its books in, add its tax registration and address, and set up the compliance profile that drives your due dates.

> Use a **test company** with made-up data while KhataOS is in alpha.

## 1. Create the company

1. Start KhataOS. The **Select Company** list opens. (With no companies yet, it opens straight on the create form.)
2. Press `Alt+C` (Create Company). You can also type a new name in the list and press `Enter`.
3. Type the **Company name** and press `Enter`.
4. Check the **Company file** path. KhataOS suggests a file in its own data folder, named after the company, such as `sharma-sons.khata`. Change it if you want the file somewhere else.
5. Press `Ctrl+A`, or `Enter` on the last field and answer `Y` to "Create this company?".

Each company is one file. To open a company file stored anywhere else, choose **Open Company File…** in the list and type its path.

## 2. Pick the country

The first time a company opens, KhataOS asks where it keeps its books.

1. Type to filter the list, or move with `↑` `↓`.
2. Press `Enter` on the country.

| Country | Currency | Financial year starts | Party tax ID |
|---|---|---|---|
| India | INR | April | GSTIN |
| United Arab Emirates | AED | January | TRN |
| Saudi Arabia | SAR | January | VAT number |
| Singapore | SGD | January | GST Reg No |
| United Kingdom | GBP | April | VAT Reg No |
| United States | USD | January | EIN |
| Mexico | MXN | January | RFC |
| Germany | EUR | January | USt-IdNr |
| France | EUR | January | N° TVA |
| Netherlands | EUR | January | Btw-id |
| Spain | EUR | January | NIF |

The country sets the currency, the financial year, the tax rules and the voucher numbering (each voucher type restarts every financial year; sales invoices and notes also carry the year).

**The country can't be changed later**, and there is no screen to change the financial year start. Pick carefully. Only an Owner can pick it (when the company has users).

## 3. Look around the home menu

The home screen shows the company, its country and currency, the current period (`Alt+F2`), the current date (`F2`), the number of ledgers and vouchers, any to-dos due soon, whether the opening balances agree, who is signed in, and where the company file is.

A new company has the standard groups (Capital Account, Current Assets, Sundry Debtors and so on) but **no ledgers**, not even Cash. Create the ledgers you need next (see [Masters](masters.md)).

## 4. Add the tax registration and address

1. Press `Alt+K` (Company), and choose **Company details**. Or press `Alt+G` and type "company details".
2. Type the company's tax ID (GSTIN, TRN, VAT number…). Leave it blank if the company isn't registered. It is checked as you type, and an invalid ID can't be saved here.
3. Pick the **State** (or emirate, region…). For India it fills in from the GSTIN.
4. Type the **Address** that prints on invoices. Press `Enter` on a blank line to move on.
5. India only: choose how many **HSN/SAC digits** go on invoices (4 up to ₹5 crore turnover, 6 above it).
6. Press `Ctrl+A`.

Only an Owner can change these. For your logo, bank details and invoice terms, see [Invoices](invoices.md). For country-specific set-up (e-invoicing, MTD, ZATCA…), see your [country page](README.md#your-country).

## 5. Set up the compliance profile on the Desk

The Desk lists your filing due dates. It needs a few facts about the company to know which dates apply.

1. On the home menu, press `T` (To-dos and Due Dates).
2. Press `Enter` on **Set up your compliance profile**, or press `F12` (Profile).
3. Answer the questions. KhataOS asks only those your country uses, for example:
   - the date of incorporation and the business form (company, LLP, partnership, sole proprietor);
   - the month the financial year ends;
   - the date you registered for VAT or GST, or `N` if you aren't registered;
   - your return periods: monthly or quarterly GST for India (QRMP), the VAT stagger for the UK, the quarter months for the UAE and Singapore, the sales tax states and frequencies for the US;
   - India: whether tax audit applies, and whether you deduct TDS or collect TCS.
4. Accept. The Desk now lists the due dates that apply to you.

The profile is kept in this company file only. It doesn't sync to other computers yet. See [Desk](desk.md).

## 6. Add users (optional)

With no users, anyone who opens the file can do everything. To require a sign-in:

1. Press `Alt+K` and choose **Users and passwords**.
2. Press `Alt+C` (Add User). The first user must be an **Owner**.
3. Type the name, pick the role, and type the password twice (at least 8 characters). Press `Ctrl+A`.

| Role | Can |
|---|---|
| Owner | Everything, including users, company details, tax rates and numbering |
| Accountant | Also alter, cancel and delete saved entries, and set bank dates |
| Data Entry | Enter new vouchers and masters, but not change saved ones |
| Auditor | See reports and vouchers, and change nothing |

Each role can do everything the roles below it can. From now on everyone signs in when the company opens. Sign out from the Company menu (`Alt+K`).

## Next

- [Create ledgers and items](masters.md)
- [Enter vouchers](vouchers.md)
- Coming from Tally? [Import your data](import-from-tally.md) instead of typing it.

# United States: sales tax

**What you'll do:** set up a US company, define the sales tax jurisdictions you collect in, charge sales tax on invoices, and use the sales tax, use tax, exemption certificate and Form 1099 reports.

> The US has thousands of local sales tax jurisdictions, and KhataOS can't ship their rates. **You enter the rates you collect.** The reports are a preparation aid; nothing is filed with any state or the IRS.

## Set up

1. Pick **United States** when the company first opens. The books are in USD, with calendar financial years.
2. `Alt+K`, **Company details**: the **EIN** (format `NN-NNNNNNN`) and the company's **state** (required).
3. **Tax rates** (`C`, **Tax Rates**): add one row per jurisdiction part you collect, with its rate and start date:

   | Code | Means |
   |---|---|
   | `CA` | California's state rate |
   | `CA-LA/COUNTY`, `CA-LA/CITY`, `CA-LA/DISTRICT` | the county, city and district parts of a local jurisdiction you name `CA-LA` |
   | `CA:FOOD` | the state rate for the tax class `FOOD` (0 for exempt) |

   A state with no row of its own is a state you don't collect in.
4. **Duty ledgers** under Duties & Taxes with Type of tax **STATE**, **COUNTY**, **CITY** or **DISTRICT** (one per part, or one ledger for all of them).
5. **Customers:** the customer's **State** in the mailing details is its jurisdiction, such as `CA` or `CA-LA`. A customer with no state is taxed at your company's state. A customer in another country is an export.
6. **Items and service ledgers:** the tax class is the taxability category. Classes starting with `EXEMPT`, `NONTAXABLE` or `RESALE` are never taxed.

Detail: [US sales tax](../us-sales-tax.md).

## Sales tax on invoices

Enter a Sales invoice as usual. KhataOS adds up the jurisdiction's parts for each line and fills in the tax lines. If the customer has a valid exemption certificate, no tax is charged, and the invoice shows a note.

## Reports

All in Go To (`Alt+G`), under Sales tax:

| Go To | What it shows | Keys |
|---|---|---|
| **Sales tax by jurisdiction (US)** | Tax collected per jurisdiction and part, with a note on states where sales may create nexus | |
| **Use tax on untaxed purchases (US)** | Purchases where no sales tax was charged, so use tax may be due | `Ctrl+Alt+J` sets where the goods are used |
| **Exemption certificates (US)** | Customers' resale and exemption certificates | `Ctrl+Alt+A` adds one, `Ctrl+Alt+D` removes one |
| **Form 1099 vendors (US)** | Vendors you may need to send a Form 1099 | |

`Ctrl+P` prints and `Ctrl+E` saves a CSV. There is no nexus setup: the nexus note is a reminder to check.

The [Desk](desk.md) shows federal and sales tax due dates. List your states and their filing frequency in the compliance profile, for example `CA:Q, TX:M`.

## Not there yet

Rates looked up from an address, filing with states, Forms 1099 and W-2 files, and payroll.

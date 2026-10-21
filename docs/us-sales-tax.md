# United States pack (`khata-us`): sales tax, use tax, certificates and Form 1099

KhataOS can keep books for a US business that collects state and local
sales tax. It checks EIN, SSN and ITIN formats, works out sales tax per
jurisdiction from **the jurisdictions and rates the company sets up**,
reports the tax collected by jurisdiction and state with an economic nexus
early warning, keeps customers' resale and exemption certificates (and
treats their sales as exempt), works out the use tax to self-assess on
untaxed out-of-state purchases, and lists the vendors that need a Form
1099. Nothing here is
tax advice, and nothing is filed with a state or the IRS: check the figures
with your accountant before you file.

Code: `crates/khata-us` (`states.rs`, `ein.rs`, `sales_tax.rs`, `nexus.rs`,
`returns.rs`, `use_tax.rs`, `exemptions.rs`, `form1099.rs`),
`apps/desktop/src-tauri/src/tax_us.rs` (vouchers into the crate's
documents; the `sales_tax_report_us(from, to)`, `use_tax_report_us(from,
to, jurisdiction)`, `us_certificates(ledger_id)`,
`set_us_certificates(ledger_id, certificates)`,
`certificate_report_us(as_of, within_days)` and `form1099_us(year)`
commands) and `apps/desktop/src/screens/tax_us.ts`
(`UsSalesTaxReportScreen`, `UsUseTaxReportScreen`, `UsCertificatesScreen`,
`addCertificate`, `Us1099ReportScreen`). All four screens are in Go To
(Alt+G) under "Sales tax" for US companies (see
[Where it is in the app](#where-it-is-in-the-app)). Sources, confidence tags and
open questions: `docs/research/us-compliance.md`.

## Why the company defines its own rates

There is no federal sales tax or VAT. Sales tax is levied by 45 states and
DC and by roughly 11,000 to 13,000 local jurisdictions (counties, cities,
special districts) whose rates change every quarter, with taxability rules
that differ by product and state (research §1, §3, §4). KhataOS cannot ship
or maintain that data. The pack instead takes the company's own list of the
jurisdictions it collects in, with each component's rate, effective-dated
like every other rate in KhataOS, and adds the components up per line.

## Setting up a US company

1. Pick **United States**. The books use USD (2 decimals, "$") and
   **calendar years**. A business may use any fiscal year (IRS Pub 538),
   but the app has no setting to change the first month yet.
2. **Company → Company details** (Alt+K): the **EIN** (`NN-NNNNNNN`, 9 digits; the
   IRS publishes no check digit, so only the format is checked) and the
   company's **state** (USPS code, one of the 50 states and DC). The state
   is required: `invoice_tax` needs the company's region because the pack
   has regions.
3. **Rates** (Company → Tax rates). Keys, all looked up exactly:

   | key              | component  | meaning                                   |
   |------------------|------------|-------------------------------------------|
   | `CA`             | `STATE`    | the state rate (7.25% in California)      |
   | `CA-LA/COUNTY`   | `COUNTY`   | a local jurisdiction's county part        |
   | `CA-LA/CITY`     | `CITY`     | its city part                             |
   | `CA-LA/DISTRICT` | `DISTRICT` | all its special districts together        |
   | `CA:FOOD`        | (state)    | the state rate for tax class `FOOD` (0 = exempt in that state) |

   `CA-LA` is a code you choose: the state, `-`, and up to 12 letters or
   digits. **A state with no `XX` key is a state you do not collect in**:
   sales there carry no tax and show as "not collected" in the report.
   States with no statewide tax (AK, DE, MT, NH, OR) can still have
   `AK` = 0 plus local keys.
4. **Duty ledgers**: create a Duties & Taxes ledger per component you
   collect and set its type of tax to `STATE`, `COUNTY`, `CITY` or
   `DISTRICT` (one ledger can collect everything if you prefer: set it to
   `STATE` and the others too).
5. **Customers**: the customer's address **region** is its jurisdiction:
   `CA` (state tax only) or `CA-LA`. A customer with no region is taxed at
   the company's state; a customer whose address country is not US is an
   export (no tax).
6. **Items and service ledgers**: the tax class is the taxability category.
   Classes starting with `EXEMPT`, `NONTAXABLE` or `RESALE` are never taxed
   (use one for resale/exemption-certificate sales and marketplace sales
   the marketplace collected on). Any other class (or none) is taxable at
   the combined rate, unless a `STATE:CLASS` override says otherwise.

### Example

California company, Los Angeles customer, rates `CA` 7.25, `CA-LA/COUNTY`
0.25, `CA-LA/CITY` 0.75, `CA-LA/DISTRICT` 1.25: a $100 taxable line gives
STATE 7.25 + COUNTY 0.25 + CITY 0.75 + DISTRICT 1.25 = $9.50. The combined
tax is rounded to the cent (half away from zero) and split over the
components so they always add up to it. States set their own rounding
rules; this is a common convention, not a rule of any one state
[UNCERTAIN].

## The sales tax report

`sales_tax_report_us(from, to)` lists, per jurisdiction and per state:
gross sales, exempt sales, not collected (no rate on file), taxable sales
and tax collected (the amounts on `STATE`/`COUNTY`/`CITY`/`DISTRICT`
ledgers). Credit notes reduce the period they are dated in; cancelled
vouchers are left out; exports are one total. Each state's return has its
own form and lines; use these figures to fill it.

**Economic nexus early warning.** Since *South Dakota v. Wayfair* (2018) a
state can require collection once a remote seller's sales pass its
threshold. For the five states with a threshold sourced from the state
(research §2) the report compares **this calendar year to date** with it:

| State | Threshold                                    | State's own measure                        |
|-------|----------------------------------------------|--------------------------------------------|
| CA    | more than $500,000                           | preceding or current calendar year         |
| TX    | $500,000 or more                             | preceding twelve calendar months           |
| NY    | more than $500,000 **and** more than 100 sales | preceding four sales tax quarters        |
| FL    | more than $100,000 (taxable remote sales)    | previous calendar year                     |
| WA    | more than $100,000                           | current or prior calendar year             |

The measuring periods differ, so this is a warning, not a determination.
Sales into a state with no rate on file are listed with a warning.

**Exempt by certificate.** A sale to a customer that has a resale or
exemption certificate **for the sale's state, in force on the sale's
date**, counts as exempt (its taxable lines move to exempt sales and are
also shown as "of which by certificate", since many returns ask for sales
for resale separately). If tax was charged on such a sale anyway, the
report still counts it as collected and warns.

## Exemption and resale certificates

Kept per customer ledger (`us_certificates`, `set_us_certificates`;
`khata_us::exemptions`): one record per certificate with

| Field   | Meaning                                                    |
|---------|------------------------------------------------------------|
| state   | the USPS code the certificate is for (one state per record; add one per state for an SST or MTC multistate form) |
| number  | the buyer's permit, registration or exemption number       |
| kind    | `Resale`, `ExemptOrganization`, `Government`, `Other`      |
| issued  | the date you received it (optional; it covers sales from then) |
| expires | the last day it is good for; blank when the state sets no date |
| note    | free text (where the scan is kept…)                        |

KhataOS does not know each state's renewal rule (SST blanket
certificates last while sales recur within 12 months; Florida's annual
resale certificate expires on 31 December; others vary): enter the expiry
the state gives. **Exemption Certificates** (`certificate_report_us(as_of,
within_days)`, screen `UsCertificatesScreen`) lists every certificate,
expired first, then those expiring within the window (60 days by
default), then valid ones and those without a date; Ctrl+Alt+A adds one,
Ctrl+Alt+D removes the one under the cursor.

**Synced as shared records.** Certificates are a shared record of the
company (`us.exempt.<ledger id>`, JSON): not in the books, but synced to
the company's other devices, the office server and accountant files, last
change wins (`docs/sync-settings.md`). They are not in exports yet. A
company shared as Viewer can't change them.

## Use tax

`use_tax_report_us(from, to, jurisdiction)` (`khata_us::use_tax`, screen
`UsUseTaxReportScreen`) lists the use tax to **self-assess** on purchases
(Purchase vouchers; Debit Notes as returns):

- only purchases **with no sales tax on them** (no `STATE`/`COUNTY`/
  `CITY`/`DISTRICT` ledger) from vendors **outside the company's state**:
  the vendor's address region (state part) is another state, or its
  country is not the US. Untaxed purchases from vendors in the company's
  own state are counted and warned about (the vendor should have charged
  tax; if it did not, use tax is owed on them too);
- **stock-item lines are taken as bought for resale** (no use tax); other
  lines (expense or asset ledgers) are taxed by their ledger's class at
  the rates of the **jurisdiction where the goods are used**: by default
  the company's state; `jurisdiction` (e.g. `CA-LA`, within the
  company's state, remembered as a shared record of the company; Ctrl+Alt+J on
  the screen) adds its local components. `EXEMPT…`/`NONTAXABLE…`/
  `RESALE…` classes and 0% state overrides are exempt;
- vendors without a state on file are left out with a warning.

The report is the liability by purchase and by component; it is not
posted. Book it (Dr the expense or asset, Cr a "Use tax payable" ledger)
and pay it with the sales and use tax return (or the state's consumer use
tax return). The use tax rate is the sales tax rate where the goods are
used (e.g. CDTFA Publication 123) [MEDIUM]; services are subject only
where the state taxes them, which the ledger's class must say.

## Form 1099 list

`form1099_us(year)` totals **Payment vouchers** to parties under Sundry
Creditors in the calendar year and lists those at or over the threshold:

- **$600** for payments made through 31 Dec 2025; **$2,000** for payments
  made after 31 Dec 2025 (One Big Beautiful Bill Act, P.L. 119-21,
  s70433, amending IRC s6041(a) and s6041A(a)(2)), indexed for inflation
  for years after 2026 — the list warns to check the IRS figure from 2027.
- Payments in 2026 on are labelled **1099-NEC box 1a** (the 2026
  instructions renamed box 1). Royalties ($10) and rents are supported by
  the crate but the desktop treats every payment as nonemployee
  compensation for now.
- The payee's TIN is its ledger's tax ID (EIN, or SSN/ITIN in
  `NNN-NN-NNNN` form); a payee without one gets a "request a Form W-9"
  warning (backup withholding is 24% without a TIN). EINs with a prefix
  the IRS no longer assigns are flagged, not refused.
- KhataOS cannot tell corporations (generally not reported, except for
  legal services) or card / payment-app payments (reported on 1099-K by
  the processor) from others: the user decides who gets a form. No IRIS or
  FIRE file is produced.

## Not modelled

Certificate scans, per-state certificate forms,
use tax credit for tax paid to another state, marketplace facilitator
flags, origin-based sourcing (enter the company's own
jurisdiction for such sales), Texas's 2% local cap and partial taxability
(e.g. 80% of Texas data processing), address-to-jurisdiction lookup, state
return forms and e-filing, 52-53-week years, payroll and income tax.

## Where it is in the app

- Go To (Alt+G), section "Sales tax", US companies only:
  **Sales tax by jurisdiction (US)**, **Form 1099 vendors (US)**,
  **Use tax on untaxed purchases (US)** (Ctrl+Alt+J sets where the goods
  are used) and **Exemption certificates (US)** (Ctrl+Alt+A adds a
  certificate with `addCertificate`, Ctrl+Alt+D removes one).
- The duty-ledger "Type of tax" list offers `STATE`, `COUNTY`, `CITY` and
  `DISTRICT` for US companies.
- `tax.rs` `invoice_tax` calls `tax_us::exempting_certificate` for a US
  sale: when the customer holds a valid certificate for the state, no tax
  is charged and the invoice shows "no sales tax: the customer's exemption
  certificate … covers …", so the invoice and the report agree.

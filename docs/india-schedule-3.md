# Schedule III financial statements (India)

KhataOS draws up the Balance Sheet, the Statement of Profit and Loss and
the notes to accounts in the Schedule III format (Companies Act 2013). It
supports two layouts:

- **Division I**, for companies that follow Accounting Standards. This is
  the default.
- **Division II**, for Ind AS companies.

The statements start from the Tally-style default groups and apply the
company's own mapping on top. Both statements use the vertical format, with
a Note No. column and the previous period's figures alongside.

## Where things live

- **Mapping and statements:** `crates/khata-in/src/schedule3.rs`.
  - `balance_sheet` and `profit_and_loss` give Division I.
  - `ind_as_balance_sheet` and `ind_as_profit_and_loss` give Division II.
  - Also in this file: `Mapping`, `parse_code`, `validate`, `line_options`, `Rounding`, `DEFAULT_MAPPING` and `DIVISION2_DEFAULTS`.
- **Notes and ratios:** `crates/khata-in/src/schedule3_notes.rs`.
  - `statements::<LineItem | IndAsLine>` builds them for a given division.
  - `company_statements` reads the division, mapping and rounding from the company profile.
- **Stored settings:** the company profile in `khata-core`, with four
  additive fields:
  - `schedule3_map` (group → line code);
  - `msme_parties`;
  - `schedule3_division`;
  - `schedule3_rounding`.

  The setters are `Book::set_schedule3_mapping`, `set_msme_party` and
  `set_schedule3_format`. The core stores the codes as opaque strings and
  checks only their shape: lower-case letters, digits and `_ : ,`, at most
  64 characters. The India pack checks what the codes mean, before saving
  and again when reading. Every change is an audit-trail entry, so it syncs
  and replays.
- **Print (A4 HTML):** in `khata_print`.
  - `schedule3_statement_html` prints either statement in the rounding unit.
  - `schedule3_notes_html` prints the notes.
  - The older `schedule3_balance_sheet_html` and `schedule3_profit_and_loss_html` still print in rupees.
- **Desktop commands** (`apps/desktop/src-tauri/src/schedule3.rs`):
  - Statements: `schedule3_balance_sheet` and `schedule3_profit_and_loss`.
    Both now follow the company's division and rounding.
  - Notes: `schedule3_statements` and `print_schedule3_notes`.
  - Mapping: `schedule3_mapping`, `schedule3_line_options`,
    `set_schedule3_mapping`, `set_schedule3_msme` and
    `set_schedule3_format`. Changing the mapping needs the Accountant role.
- **Screens** (`apps/desktop/src/screens/schedule3.ts`):
  - `Schedule3BalanceSheet` and `Schedule3ProfitLoss` show the statements.
  - `Schedule3Notes` lists the notes. Enter opens a note; Enter on a ledger row in a note opens that ledger.
  - `Schedule3Mapping` shows the group tree with each group's Division I and Division II line.
    - Enter on a group chooses its line, or restores the default.
    - Enter on a supplier toggles MSME.
    - Enter on the first two rows changes the division or the rounding unit.

## Requirements this follows

All of these are cited, with confidence tags, in
`docs/research/india-schedule-3.md` (MCA notification of 24 Mar 2021, ICAI
Guidance Notes), which adds to §3 of `docs/research/india-compliance.md`.

- Division I and Division II line items, with a previous-year column.
- **The 2021 amendments:**
  - MSME and other trade payables shown separately;
  - ageing schedules for receivables, payables and CWIP;
  - current maturities of long-term borrowings shown under short-term borrowings;
  - "Total Income";
  - mandatory rounding by total income;
  - promoter shareholding;
  - the eleven ratios, with reasons for any change over 25%;
  - the additional regulatory information.
- Whether anything was amended after 2021 is **[UNCERTAIN]**. We found no
  amendment up to October 2026.

The headings follow the MCA text as read for that page.

## Default mapping

Default groups are matched by their stable id (`GroupId::default_named`),
never by display name, so renaming a group (for example "Sundry Debtors" to
"Customers") does not change where it goes. Sub-groups that users create go
wherever their parent goes.

### Balance Sheet

| Schedule III line | Default groups | Notes |
|---|---|---|
| **Shareholders' funds**: Share capital | Capital Account (except Reserves & Surplus) | |
| Reserves and surplus | Reserves & Surplus, plus the surplus in the Statement of Profit and Loss | The surplus is shown in two parts: *brought forward*, which is accumulated profit up to the start of the financial year (1 April), and *profit for the year*. |
| Money received against share warrants | none | Can only be filled by a mapping |
| Share application money pending allotment | none | Can only be filled by a mapping |
| **Non-current liabilities**: Long-term borrowings | Loans (Liability), Secured Loans, Unsecured Loans | **Assumption**: all loans are long-term. See "What you must classify" |
| Deferred tax liabilities (net), Other long-term liabilities, Long-term provisions | none | Can only be filled by a mapping |
| **Current liabilities**: Short-term borrowings | Bank OD A/c, Bank OCC A/c | |
| Trade payables | Sundry Creditors | Split on the face into (A) MSME and (B) others, using the MSME marks |
| Other current liabilities | Current Liabilities (its own ledgers), Duties & Taxes, Branch / Divisions, Suspense Account | **Assumptions**: branch balances settle within the year, and Suspense should be nil (a warning appears when it is not) |
| Short-term provisions | Provisions | |
| *Difference in opening balances* | none | Not a Schedule III line. It appears only when opening balances do not tally (same figure as the core Balance Sheet) and comes with a warning |
| **Non-current assets**: Property, Plant and Equipment | Fixed Assets | Intangible assets, capital work-in-progress and intangible assets under development can only be filled by a mapping |
| Non-current investments | Investments | **Assumption**: all investments are non-current |
| Deferred tax assets (net), Long-term loans and advances, Other non-current assets | none | Can only be filled by a mapping |
| **Current assets**: Current investments | none | Can only be filled by a mapping |
| Inventories | Closing stock (inventory valuation), plus any Stock-in-Hand ledgers | |
| Trade receivables | Sundry Debtors | |
| Cash and cash equivalents | Cash-in-Hand, Bank Accounts | |
| Short-term loans and advances | Loans & Advances (Asset), Deposits (Asset) | **Assumption**: these are current |
| Other current assets | Current Assets (its own ledgers), Misc. Expenses (Asset) | **Assumption**: Misc. Expenses (Asset) is placed here. It may belong under other non-current assets |

### Statement of Profit and Loss

| Schedule III line | Default groups | Notes |
|---|---|---|
| Revenue from operations | Sales Accounts, Direct Incomes | Direct incomes are treated as other operating revenue |
| Other income | Indirect Incomes | |
| **Total income** | | |
| Cost of materials consumed | none | Manufacturers should map their raw-material purchase groups here |
| Purchases of stock-in-trade | Purchase Accounts | **Assumption**: a trading business |
| Changes in inventories of finished goods, work-in-progress and stock-in-trade | Computed as opening stock − closing stock | Groups cannot be mapped to this line |
| Employee benefits expense, Finance costs, Depreciation and amortisation expense | none | The default groups do not separate these. See below |
| Other expenses | Direct Expenses, Indirect Expenses | |
| **Total expenses**; **Profit before tax** | | **Assumption**: there are no exceptional or extraordinary items, so profit before tax = total income − total expenses |
| Tax expense: Current tax, Deferred tax | none | Can only be filled by a mapping. Without one, profit for the period is the profit before tax, and a warning says so |
| **Profit (Loss) for the period** | | Equal to the core Profit & Loss net profit |

### When no mapping applies

A ledger whose group chain reaches no mapped group falls back on its
nature: Other current liabilities, Other current assets, Other income or
Other expenses. A warning names the group. This cannot happen with today's
chart, because every group sits under a default primary group and all of
those are mapped. The fallback exists only as a safeguard.

## The company's mapping

`Mapping::of_book` reads the stored line codes. For each ledger, KhataOS
walks up from the ledger's group. At each group it checks the company's
mapping first, then that group's default. The first group with either one
wins. This means:

- A mapping moves that group's own ledgers, plus those of its sub-groups
  that have no mapping of their own. A default sub-group (for example
  Secured Loans under Loans (Liability)) keeps its own default unless it
  is mapped too.
- A mapping must match the side of the statement. A liability group can
  only map to an equity or liability line, an asset group to an asset
  line, an income group to an income line, and an expense group to an
  expense or tax line.
  - The exception is OCI: either an income or an expense group can map there.
  - `validate` refuses a wrong-side code before it is saved.
  - A stored code that does not fit (for example after a group was moved),
    cannot be read, or names a deleted group is ignored, with a warning.
- *Changes in inventories* cannot be a target.

### Line codes

A code is a line, optionally followed by classification tags:
`<line>` or `<line>:<tag>,<tag>`.

**The line part** is one of:

- a Division I code, such as `trade_payables` or `long_term_borrowings`.
  These are `LineItem`s in snake_case.
- a Division II-only code. These are listed below.

**The tags** each apply only to certain lines:

| Tag | Allowed on | Effect |
|---|---|---|
| `current_maturities` | Short-term borrowings | Current maturities of long-term borrowings, shown apart in the note. Move the current portion into a group with this mapping, using a journal. |
| `secured` / `unsecured` | Borrowings, trade receivables | Secured or unsecured classification |
| `msme` | Trade payables | Dues to micro and small enterprises, for a whole group |
| `disputed` | Trade payables and receivables | The disputed rows of the ageing schedules |
| `doubtful` | Trade receivables | Considered doubtful. Division II shows it as credit impaired. |

Instead of grouping MSME suppliers, you can mark them one by one. The marks
are stored as `msme_parties`, a list of ledgers in the profile. This adds
nothing to the core `Ledger`. Each party is marked in the mapping screen.

**Division II-only codes:**

- `investment_property`, `goodwill`
- `trade_receivables_non_current`, `other_financial_assets_non_current`,
  `other_financial_assets_current`, `other_bank_balances`,
  `current_tax_assets`
- `lease_liabilities_non_current`, `lease_liabilities_current`,
  `trade_payables_non_current`, `other_financial_liabilities_non_current`,
  `other_financial_liabilities_current`, `current_tax_liabilities`
- `oci_not_reclassified`, `oci_reclassified`

Each one has a Division I fallback (`IndAsLine::division1`), so a Division I
company can use them too:

- long-term trade receivables and payables go to other non-current assets
  and other long-term liabilities;
- lease liabilities go to long-term borrowings, and the current part to
  other current liabilities [UNCERTAIN];
- current tax assets go to short-term loans and advances [UNCERTAIN];
- OCI goes to other income or other expenses.

### Division II layout

Every placement carries both a Division I line and a Division II line. The
Division II line is the translation of the Division I line, unless the
group has a Division II code. Translations:

| Division I | Division II |
|---|---|
| Share capital | Equity share capital |
| Reserves and surplus, share warrants, share application money | Other equity |
| Borrowings | Financial liabilities: borrowings |
| Trade payables | Financial liabilities: trade payables |
| Investments, trade receivables, loans and advances | The financial-asset lines |

One default group differs from its translation: Deposits (Asset) become
other financial assets.

So Division II totals equal Division I totals, and the tests check it:

- The Balance Sheet starts with ASSETS.
- The Statement of Profit and Loss adds "Profit/(loss) before exceptional
  items and tax".
- An Other Comprehensive Income section shows parts A and B. They are nil
  unless mapped, and are shown net of their tax.
- Total comprehensive income is the Division I profit for the period.

A warning reminds Ind AS companies to move payables that are financial
liabilities into `other_financial_liabilities_*`.

## Rounding

The unit is chosen in the mapping screen and stored as `schedule3_rounding`.
The choices are rupees, hundreds, thousands, lakhs, millions or crores, and
figures show to two decimals in the unit.

- Schedule III makes rounding compulsory.
  - Total income under ₹100 crore: hundreds, thousands, lakhs or millions.
  - Total income of ₹100 crore or more: lakhs, millions or crores.
- Exact rupees are not a permitted unit. They stay the default for working
  papers, with a warning until you choose a permitted unit.
- Each figure is rounded on its own, so a rounded total can differ by a
  rounding difference from the sum of its rounded lines.

## Notes to accounts

The notes are numbered as on the statements. The Balance Sheet's lines come
first, and the Statement of Profit and Loss carries on from the Balance
Sheet's last number. Each note is built from its line's own ledgers and
computed figures, so its total equals the line. The tests check this for
every note.

| Note | Content | From |
|---|---|---|
| Share capital | Ledgers; an empty promoter shareholding table; the particulars to supply (classes, par value, reconciliation, holders over 5%) | Capital Account ledgers |
| Reserves and surplus (Other equity) | Each ledger's opening, additions, deductions and closing; the surplus brought forward, profit for the year and closing | Vouchers in each financial year |
| Long-term / short-term borrowings | Current maturities; secured; unsecured; not classified (with a warning); remarks for terms, security and defaults | Tags; Secured Loans / Unsecured Loans; Bank OCC treated as secured [UNCERTAIN] |
| Trade payables | (A) MSME, (B) others; ageing for both dates (not due, < 1, 1–2, 2–3, > 3 years) with rows MSME / Others / Disputed MSME / Disputed others; remarks for the MSMED Act s.22 interest | Bill-wise outstanding |
| Trade receivables | Secured good / unsecured good / doubtful; ageing for both dates (not due, < 6 months, 6–12 months, 1–2, 2–3, > 3 years) with the four Division I or six Division II rows | Bill-wise outstanding |
| PPE and intangibles | For each year: opening, additions, disposals, depreciation and closing per ledger; remarks for title deeds and revaluation | Vouchers on the asset ledgers |
| CWIP and intangibles under development | Ledgers, plus the ageing (projects in progress) for both dates | Debits, oldest used up first |
| Inventories | Closing stock by top-level stock group, plus Stock-in-Hand ledgers | Stock valuation |
| Cash and cash equivalents | Cash on hand; balances with banks | Cash-in-Hand vs other ledgers |
| Changes in inventories | Opening, less closing, (increase)/decrease | Stock valuation |
| Every other line | Its ledgers | |
| Ratios | The eleven ratios, current and previous, with % variance and "explain" over 25% | The Division I statements (see the research page for the formulas) |
| Additional regulatory information | Checklist of the disclosures the books cannot give: title deeds, revaluation, loans to promoters, benami property, quarterly returns, wilful defaulter, struck-off companies, charges, layers, schemes, use of borrowed funds, undisclosed income, CSR, crypto | |

### How the notes compute

**Bills and ageing:**

- The bill-wise outstanding is built as the core's outstanding-bills report
  builds it, but for any ledger on the line.
- Each unallocated entry is kept on its own date. Whatever the bills do not
  explain is added as of the balance-sheet date, so the ageing always ties.
- Items age from the due date, or from the bill date when there is none.
  Opening balances age from the start of the books (the financial year of
  the first voucher).
- Credit items, such as advances, net within their age band.

**Depreciation:**

- A credit to an asset in a voucher that debits a depreciation ledger
  counts as depreciation. Other credits are disposals.
- A depreciation ledger is one under a group mapped to
  `depreciation_and_amortisation`, or an expense ledger whose name contains
  "depreciation" or "amortis".
- A warning flags a depreciation-looking ledger that is not mapped there.

**Not built:**

- Accumulated depreciation kept outside the books.
- The CWIP completion schedule.
- The title-deeds, struck-off-company and loans-to-promoters tables. They
  are listed as to be supplied.

## What you must classify

These items cannot be worked out from the default groups. The statements
show a warning for each one that has a balance.

1. **Current vs non-current.** By default, loans are long-term borrowings,
   investments are non-current, and loans, advances and deposits given are
   current. Current maturities of long-term borrowings belong under
   short-term borrowings (2021 amendment). Put them in their own group and
   map it to `short_term_borrowings:current_maturities`. The warning goes
   away once any group carries the tag.
2. **MSME trade payables.** Mark MSME suppliers in the mapping screen, or
   group them under a group mapped to `trade_payables:msme`. Until one is
   marked, a warning says that all dues are shown as dues to others.
3. **Employee benefits expense**, such as salaries, wages, PF/ESI
   contributions and staff welfare.
4. **Finance costs**, such as interest and other borrowing costs.
5. **Depreciation and amortisation expense.**
6. **Tax expense**: current tax and deferred tax, plus deferred tax assets
   and liabilities on the Balance Sheet.
7. **Intangible assets and capital work-in-progress**, if you hold them
   under Fixed Assets.
8. **Cost of materials consumed vs purchases of stock-in-trade**, for
   manufacturers. Note that closing stock is not split between raw
   materials and finished goods. All of it goes to changes in inventories.

For items 3 to 6, create sub-groups (for example "Salaries & Wages" under
Indirect Expenses) and map each one to its line.

## Tying out

The statements are built from the same balances, closing-stock valuation,
accumulated profit and opening difference as the core reports. The tests
in `schedule3.rs` check that:

- Total equity and liabilities equals the core Balance Sheet's total
  liabilities, and total assets equals its total assets, for both periods.
- Equity and liabilities equals assets.
- Profit (Loss) for the period equals the core Profit & Loss net profit,
  for both periods, including with mappings in place and with stock.
- Division II totals equal Division I totals. Division II total
  comprehensive income equals Division I profit, and its OCI is taken out
  of profit for the period (`schedule3_notes.rs` tests).
- Every note's total equals its statement line, for both years. The ageing
  totals equal the line, for both dates.
- The mapping, MSME parties, division and rounding survive replay of the
  audit trail. They give identical statements (core and pack tests).

## Drill-down and numbering

Each line carries its figure, the previous figure, its sources (groups,
closing or opening stock, surplus brought forward, profit for the year) and
the ledgers behind it.

When you press Enter on a line:

- If the line is exactly one group's whole subtree, the Group Summary
  opens.
- If the line is made only of stock figures, the Stock Summary opens.
- Otherwise, a list of the line's ledgers opens. Enter on a ledger opens
  its ledger statement.

Note numbers are the order of the lines shown. The Balance Sheet runs 1, 2,
3…, and the Statement of Profit and Loss carries on from the Balance
Sheet's last note as at its end date. Lines that are nil in both periods
are left out. In Division II, the OCI lines are always shown, but get a
note number only when they are not nil. The Balance
Sheet always shows Share capital and Reserves and surplus, and the
Statement of Profit and Loss always shows Revenue from operations.

## Gaps

- **Unbilled dues** (receivables and payables) are not tracked. A remark
  asks for them.
- **Information outside the books** is not captured: share particulars,
  promoter shareholding, security and repayment terms, MSMED Act interest,
  and the additional regulatory information. Each is listed in its note as
  to be supplied.
- **Exceptional and extraordinary items, discontinued operations and EPS**
  are not built. Division II also lacks biological assets and the
  Statement of Changes in Equity.
- **Division III (NBFC)** is not supported, and neither is the simplified
  format for proprietorships and partnerships.
- **Return on investment** is not computed.
  - It needs market values and the timing of cash flows.
  - Credit sales and purchases are not separated from cash ones, so the
    turnover ratios use all sales and purchases.
- **Excel export** for the CA is the generic table CSV of each screen.
- **Menus:** all four screens are in Go To (Alt+G) and the Report Directory
  under "Statutory": Balance Sheet (Schedule III), Statement of Profit and
  Loss (Schedule III), Notes to Accounts (Schedule III) and Schedule III
  mapping.

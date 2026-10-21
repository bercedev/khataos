# Research: Schedule III (Companies Act 2013), 2021 amendment and Division II

This page supports `docs/india-schedule-3.md` and the code in
`crates/khata-in/src/schedule3.rs` and `schedule3_notes.rs`. It adds to
§3 of `docs/research/india-compliance.md`.

Each finding has a confidence tag:

- **[PRIMARY]**: read in the MCA notification or an ICAI Guidance Note.
- **[SECONDARY]**: from commentary.
- **[UNCERTAIN]**: not confirmed. Check it before general availability.

Research date: October 2026.

## Sources

| Source | What it is | Tag |
|---|---|---|
| MCA notification of 24 Mar 2021 amending Schedule III, "with effect from 1st day of April, 2021" | The original link, <http://www.mca.gov.in/Ministry/pdf/ScheduleIIIAmendmentNotification_24032021.pdf>, returned 403 to our fetcher. We read an identical mirror: <https://ca2013.com/wp-content/uploads/2021/03/ScheduleIIIAmendmentNotification_24032021.pdf>. The G.S.R. number is blank in that copy. | [PRIMARY] |
| G.S.R. number 207(E) | Given in MCA's later clarification on rounding, as quoted at <https://studycafe.in/mca-issued-clarification-on-mandating-companies-to-round-off-figures-appearing-in-financial-statements-163147.html> | [SECONDARY] |
| ICAI Guidance Note on Division I, revised January 2022 | <https://resource.cdn.icai.org/68981clcgc55147-gnd1.pdf> | [PRIMARY] |
| ICAI Guidance Note on Division II, July 2019 (before the 2021 amendment; used only for the base formats) | <https://live.icai.org/bos/vcc-2nd-batch-recorded-lectures/pdf/6.%20ICAI%20Guidance%20Note%20on%20Division%20II-%20Ind%20AS%20Schedule%20III_Revised.pdf> | [PRIMARY] |
| ICAI Guidance Note on Division II, January 2022 edition | Listed at <https://publication.icai.org/publication/77>. Not read. | [UNCERTAIN] |

## 1. Rounding off

- The 2021 amendment changed General Instruction 4(i) of Division I from "may" to "shall", and from "Turnover" to "Total Income". [PRIMARY: notification]
- The amended text: "Depending upon the Total Income of the company, the figures appearing in the Financial Statements shall be rounded off". The units are: [PRIMARY: GN Div I]
  - total income less than ₹100 crore: "To the nearest hundreds, thousands, lakhs or millions, or decimals thereof";
  - total income of ₹100 crore or more: "To the nearest lakhs, millions or crores, or decimals thereof".
- "Once a unit of measurement is used, it should be used uniformly in the Financial Statements." The notes count: GN para 6.14. [PRIMARY]
- Division II already said "shall" before 2021. In 2021 only "Turnover" became "Total Income". [PRIMARY]
- GN para 6.15: "It is now compulsory to apply rounding off and a company cannot continue to disclose full figures." Exact rupees are therefore not a permitted unit. [PRIMARY]
- MCA has said that absolute figures in the AOC-4 e-form are not incorrect certification (Sep 2022). [SECONDARY: studycafe]

**In KhataOS:** `Rounding` offers rupees, hundreds, thousands, lakhs, millions and crores. Figures are shown to two decimals in the chosen unit. Rupees remain the default for working papers, but the statements show a warning until a permitted unit is chosen. The warning also appears when the unit does not suit the company's total income (`Rounding::allowed`). Each figure is rounded on its own, so a rounded total can differ from the sum of the rounded lines. [UNCERTAIN] How rounding differences should be presented is not set out in the text.

## 2. Trade receivables ageing

- **Division I** [PRIMARY: notification]
  - Location: under heading P "Trade Receivables", item (i). The same table appears for long-term receivables under M "Other non-current assets".
  - The table is "Outstanding for following periods from due date of payment".
  - Columns: Less than 6 months | 6 months - 1 year | 1-2 years | 2-3 years | More than 3 years | Total.
  - Rows:
    - (i) Undisputed Trade receivables – considered good
    - (ii) Undisputed Trade Receivables – considered doubtful
    - (iii) Disputed Trade Receivables considered good
    - (iv) Disputed Trade Receivables considered doubtful
  - Footnote: where no due date is specified, the disclosure is "from the date of the transaction". "Unbilled dues shall be disclosed separately."
- **ICAI Guidance Note** [PRIMARY: GN Div I]
  - Add "Unbilled" and "Not due" columns before the ageing columns, so that the total ties to the balance sheet.
  - The due date comes from the terms agreed between buyer and supplier.
  - With no due date, use the date the right to consideration becomes unconditional.
- **Division II** uses the same columns, with six rows [PRIMARY: notification]:
  - (i) Undisputed – considered good
  - (ii) Undisputed – which have significant increase in credit risk
  - (iii) Undisputed – credit impaired
  - (iv) to (vi): the same three for disputed receivables
- **Classification** [PRIMARY]
  - Division I: "(a) Secured, considered good; (b) Unsecured considered good; (c) Doubtful". The allowance for bad and doubtful debts is shown separately.
  - Division II: "Considered good – Secured; Considered good – Unsecured; significant increase in credit risk; credit impaired".

**In KhataOS:**

- Ages are taken from bill-wise outstanding. Each item ages from its due date, or from its bill date when there is none.
- Items whose due date has not arrived go in a "Not due" column.
- Opening balances age from the start of the books (the financial year of the first voucher). [UNCERTAIN] The books hold no earlier date.
- Unbilled dues are not tracked; a remark asks for them.
- Disputed and doubtful are group-level classifications (`trade_receivables:disputed`, `:doubtful`).
- In Division II, "doubtful" is shown as credit impaired. The "significant increase in credit risk" rows are always nil.

## 3. Trade payables ageing

- Location: the new heading "FB. Trade payables due for payment". [PRIMARY: notification]
- Columns: Less than 1 year | 1-2 years | 2-3 years | More than 3 years | Total. They run from the due date of payment, with the same footnote as receivables. [PRIMARY]
- Rows:
  - (i) MSME
  - (ii) Others
  - (iii) Disputed dues – MSME
  - (iv) Disputed dues – Others
- The GN says: [PRIMARY: GN Div I]
  - rows (i) and (ii) are undisputed dues only;
  - unbilled payables include accruals but not provisions;
  - a "Not due" column is added, and ageing does not apply to dues that are not yet due.
- The face of both divisions splits trade payables into "(A) total outstanding dues of micro enterprises and small enterprises; and (B) total outstanding dues of creditors other than micro enterprises and small enterprises". [PRIMARY: GN Div II 2019] [SECONDARY] We believe Division I got the same split in the October 2018 amendment, G.S.R. 1022(E), but did not confirm the number.

**In KhataOS:** a supplier counts as MSME in either of two ways. It can be marked as an MSME party in the profile (`msme_parties`), or it can sit under a group mapped to `trade_payables:msme`. The face shows the (A)/(B) split as detail rows. The note gives the split and the ageing for both dates. A remark asks for the MSMED Act s.22 interest disclosures.

## 4. Capital work-in-progress and intangible assets under development

- **CWIP ageing schedule** [PRIMARY: notification]
  - Columns: "Amount in CWIP for a period of" Less than 1 year | 1-2 years | 2-3 years | More than 3 years | Total.
  - Rows: Projects in progress; Projects temporarily suspended.
  - The total "shall tally with CWIP amount in the balance sheet".
- **Completion schedule** [PRIMARY]: needed for projects that are overdue or over their original cost, with columns "To be completed in" < 1, 1-2, 2-3, > 3 years.
- Intangible assets under development have the same two tables. [PRIMARY]

**In KhataOS:** the ageing treats the oldest debits as used up first: what remains is the newest debits. Every project is shown as "in progress". Suspended projects and the completion schedule are remarks for the company to supply.

## 5. Share capital and promoter shareholding

- Table "Shares held by promoters at the end of the year": [PRIMARY]
  - Columns: Promoter name | No. of Shares | % of total shares | % Change during the year.
  - The table is given for each class of shares.
  - The change is measured against the number at the beginning of the year.
- The other share-capital particulars (classes, par value, reconciliation, holders of more than 5%) are long-standing Division I requirements. [PRIMARY: GN Div I]

**In KhataOS:** the books hold amounts only. The note lists the Capital Account ledgers, gives an empty promoter table, and lists the particulars the company must supply.

## 6. Borrowings

- "Current maturities of Long term borrowings shall be disclosed separately" under short-term borrowings. Old item G(a) under other current liabilities was omitted. Division II made the same change. [PRIMARY: notification]
- Borrowings are "sub-classified as secured and unsecured", and the nature of security is stated. The following are also disclosed: [PRIMARY: GN Div I]
  - loans guaranteed by directors or others;
  - terms of repayment;
  - the period and amount of any continuing default.

**In KhataOS:**

- A group mapped to `short_term_borrowings:current_maturities` is shown on its own. Users move the current portion into such a group with a journal.
- Secured and unsecured come from the default groups (Secured Loans, Unsecured Loans) or a `:secured` / `:unsecured` tag.
- **Assumption [UNCERTAIN]:** Bank OCC (cash credit) is treated as secured.
- Bank OD and other loans are "not classified" until tagged, and a warning names them.

## 7. Ratios (eleven, with reasons for changes over 25%)

- **Requirement [PRIMARY: notification]:** the eleven ratios (a)–(k). "The company shall explain the items included in numerator and denominator… Further explanation shall be provided for any change in the ratio by more than 25% as compared to the preceding year."
- **Formulas: ICAI GN Div I 2022, Annexure B [PRIMARY]**

| Ratio | Formula in the GN | KhataOS |
|---|---|---|
| Current ratio | Current assets / current liabilities | Schedule III sections |
| Debt-equity | Total debt / shareholders' equity | Long-term + short-term borrowings / shareholders' funds |
| Debt service coverage | (PBT + non-cash expenses such as depreciation + interest + other adjustments) / (interest and lease payments + principal repayments) | Principal repaid = debits to long-term borrowing ledgers in vouchers that move no other borrowing; lease payments are not identified [UNCERTAIN] |
| Return on equity | (PAT − preference dividend) / average equity | Preference dividend is not identified |
| Inventory turnover | COGS or sales / average inventory | COGS = materials + purchases + changes in inventories |
| Trade receivables turnover | Net credit sales / average receivables | All revenue from operations (credit sales are not separated) |
| Trade payables turnover | Net credit purchases / average payables | All purchases of stock-in-trade and materials |
| Net capital turnover | Net sales / average working capital | Revenue from operations |
| Net profit ratio | PAT / net sales | Revenue from operations |
| Return on capital employed | EBIT / (tangible net worth + total debt + deferred tax liability) | Intangibles are taken off net worth |
| Return on investment | Time-weighted rate of return | Not computed; needs market values and cash-flow timing |

- **Averages:** (opening + closing) / 2. The GN says to use the closing balance when no opening is available. [PRIMARY] KhataOS does the same for a first year.
- **Reuse of `khata-core` analysis.** The ratios are built from the Schedule III statements, so they tie to the face. `khata_core::analysis::ratio_analysis` follows Tally's groups, and its definitions differ: its current liabilities exclude bank OD, and its sales exclude direct incomes. So it was not reused.

## 8. Additional regulatory information and P&L disclosures

- **Division I para 6, heading "Y. Additional Regulatory Information"** [PRIMARY: notification]:
  - (i) title deeds;
  - (ii) PPE revaluation by a registered valuer;
  - (iii) loans to promoters, directors, KMPs and related parties that are repayable on demand or have no terms;
  - (iv) CWIP;
  - (v) intangibles under development;
  - (vi) benami property;
  - (vii) quarterly returns to banks for borrowings secured on current assets;
  - (viii) wilful defaulter;
  - (ix) struck-off companies;
  - (x) charges not registered;
  - (xi) layers;
  - (xii) ratios;
  - (xiii) schemes of arrangement;
  - (xiv) utilisation of borrowed funds and share premium.
- **Division II, heading "L"** runs (i)–(xvi). It adds investment-property fair value and the revaluation of right-of-use assets and intangibles. [PRIMARY]
- **Statement of Profit and Loss, Division I para 5** [PRIMARY]:
  - (ix) undisclosed income;
  - (x) CSR, items (a)–(h);
  - (xi) crypto or virtual currency.
- **Title-deeds table columns** [PRIMARY]: line item | description | gross carrying value | held in the name of | whether the holder is a promoter or director (or a relative or employee of one) | held since | reason (and whether in dispute).

**In KhataOS:** none of these facts are in the books. The "Additional regulatory information" note lists each one as to be supplied.

## 9. Division II (Ind AS) formats

- **Balance sheet** [PRIMARY: GN Div II 2019 plus the 2021 change]
  - **Order:** ASSETS first, then EQUITY AND LIABILITIES.
  - **Non-current assets:** PPE; CWIP; Investment Property; Goodwill; Other Intangible assets; Intangible assets under development; Biological Assets other than bearer plants; Financial Assets (Investments, Trade receivables, Loans, Others); Deferred tax assets (net); Other non-current assets.
  - **Current assets:** Inventories; Financial Assets (Investments, Trade receivables, Cash and cash equivalents, Bank balances other than (iii) above, Loans, Others); Current Tax Assets (Net); Other current assets.
  - **Equity:** Equity Share capital; Other Equity.
  - **Liabilities:** Financial Liabilities (Borrowings, **(ia) Lease liabilities**, added in 2021, Trade Payables (MSME / others), Other financial liabilities); Provisions; Deferred tax liabilities (Net); Other non-current liabilities. The current side is the same, plus Current Tax Liabilities (Net).
- **Statement of Profit and Loss** [PRIMARY]
  - I to IV: revenue, other income, total income, expenses.
  - V: profit before exceptional items and tax.
  - VI: exceptional items.
  - VII: profit before tax.
  - VIII: tax expense.
  - IX to XIII: continuing and discontinued operations, then profit for the period.
  - XIV: Other Comprehensive Income. A (i) items that will not be reclassified, (ii) their income tax. B (i) items that will be reclassified, (ii) their income tax.
  - XV: Total Comprehensive Income.
  - XVI to XVIII: EPS.
- **Statement of Changes in Equity** [PRIMARY]: Part A, equity share capital, and Part B, other equity by component. The format was replaced in 2021.

**In KhataOS:**

- The same placement drives both divisions. Each Division I line has a fixed Division II equivalent, so totals are equal by construction. Division II-only codes, such as `other_financial_liabilities_current`, `lease_liabilities_non_current` and `oci_not_reclassified`, map to a Division I fallback.
- "Deposits (Asset)" default to other financial assets in Division II.
- OCI is shown net of tax: map the tax on OCI items to the same OCI line. The section is nil unless a group is mapped to it.
- **Not built:** biological assets, exceptional items, discontinued operations, EPS and the Statement of Changes in Equity.
- [UNCERTAIN] Where Division I shows the current part of lease liabilities. We used other current liabilities.
- [UNCERTAIN] Advance tax (current tax assets) is placed with short-term loans and advances in Division I.

## 10. Amendments after 2021

- [UNCERTAIN] We found no Schedule III amendment notified between 2022 and October 2026. The changes we found amend other rules:
  - Companies (Accounts) Amendment Rules 2025, G.S.R. 317(E), 19 May 2025.
  - Companies (Accounts) Second Amendment Rules 2025, G.S.R. 357(E), 30 May 2025. <https://ca2013.com/wp-content/uploads/2025/06/MCA-Notification-regarding-Companies-Accounts-Second-Amendment-Rules-2025-dated-30.05.2025.pdf>
  - Small-company definition, G.S.R. 880(E), 1 Dec 2025.
  - Companies (Accounting Standards) and Ind AS amendment rules, 2026 (Pillar Two disclosures). <https://taxguru.in/company-law/mca-notifies-companies-ind-amendment-rules-2026.html>
- An Ind AS based on IFRS 18 is reported to be in draft. It could change the Division II format.
- Re-check the notifications page on mca.gov.in before general availability.

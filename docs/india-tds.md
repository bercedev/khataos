# TDS and TCS (India)

KhataOS suggests TDS and TCS while you enter a voucher, reports what was deducted or collected and paid, and prepares the quarterly statements. It writes the official statement text file for Forms 140, 143 and 144, and estimates salary TDS. It is a preparation aid. **Validate every statement file with the official File Validation Utility (FVU) before filing.**

Sources:

- `docs/research/india-tds-tcs.md` (October 2026). The rates, thresholds and file layout below come from the gazetted Income-tax Act 2025, the Finance Act 2026 and Protean's file formats v1.2 [PRIMARY], unless marked otherwise.
- `docs/research/india-compliance.md` §5, the first pass, from secondary sources.

## Code

| Where | What |
|---|---|
| `crates/khata-core/src/chart.rs` | `Ledger.pan`; `Ledger.tds` (`LedgerTds`, `TdsTerms`, `DeducteeKind`, and `NonResidentTerms` inside `TdsTerms`); `Ledger.tcs` (`LedgerTcs`, `TcsTerms`). The core checks only the shape of these fields. |
| `crates/khata-in/src/pan.rs` | PAN structure and holder type. |
| `crates/khata-in/src/tds.rs` | TDS natures, the 2025 Act table rows and 4-digit tax codes, the calculator, quarters, due dates. |
| `crates/khata-in/src/tcs.rs` | TCS natures (s.394), the calculator, the TCS deposit date. |
| `crates/khata-in/src/salary.rs` | The new-regime salary estimate (s.392, s.202(1)). |
| `crates/khata-in/src/tds_file.rs` | The FVU text file for Forms 140, 143 and 144, and its code lists. |
| `apps/desktop/src-tauri/src/tds.rs` | Reads vouchers. Holds the deductor and challan settings. Serves the commands listed below. |
| `apps/desktop/src/screens/tds.ts` | `TdsPayableReport`, `TcsPayableReport`, `TdsQuarterlyReport`, `TcsQuarterlyReport`, `TdsNonResidentReport`, `SalaryTdsEstimateReport`, `editTdsDeductor`, `editChallanDetails`, `editLedgerTds`, `editLedgerNonResident`, `editLedgerTcs`. |

Commands:

| Area | Commands |
|---|---|
| TDS | `tds_natures`, `tds_for`, `tds_payable`, `tds_quarterly`, `tds_quarterly_csv`, `set_ledger_tds` |
| TCS | `tcs_natures`, `tcs_for`, `tcs_payable`, `tcs_quarterly`, `ledger_tcs`, `set_ledger_tcs` |
| Non-residents | `tds_quarterly_nr`, `tds_nr_codes` |
| Statements | `tds_deductor`, `set_tds_deductor`, `tds_challan`, `set_tds_challan`, `tds_statement_file`, `write_tds_file` |
| Salary | `salary_estimate` |

## Which Act applies

| Payment, credit or collection date | Law | Section shown | Quarterly statements |
|---|---|---|---|
| Up to 31 Mar 2026 | Income-tax Act 1961 | `s.194C` and so on | 26Q, 27Q, 27EQ (data and CSV only; no text file) |
| From 1 Apr 2026 | Income-tax Act 2025 | `s.393(1) (ex-194C)`; TCS `s.394(1) [Table: Sl. No. 4] (ex-206C(1))` | Form 140 (ex-26Q), Form 144 (ex-27Q), Form 143 (ex-27EQ), Form 138 (ex-24Q, not generated) |

- **TDS** rates and limits apply from 1 Apr 2025, as before, and the calculator refuses earlier dates. From 1 Apr 2026 they match the s.393 tables of the 2025 Act [PRIMARY].
- **TCS** is supported only from 1 Apr 2026. The 1961 Act's s.206C rates were not re-verified, so earlier dates are refused.

## TDS natures

| Code | Nature | Old section | 2025 Act | Tax code | Rate | Limit |
|---|---|---|---|---|---|---|
| `contract` | Contractors | 194C | s.393(1) Sl. No. 6(i) | 1023 individual/HUF, 1024 others | 1% / 2% | one sum over ₹30,000, or over ₹1,00,000 in the year |
| `professional` | Professional fees | 194J | 6(iii) | 1027 | 10% | over ₹50,000 |
| `technical` | Technical services, call centres, film royalty | 194J | 6(iii) | 1026 | 2% | over ₹50,000 |
| `rent_building` | Rent of land, building, furniture | 194I | 2(ii) | 1009 | 10% | over ₹50,000 a month |
| `rent_machinery` | Rent of plant and machinery | 194I | 2(ii) | 1008 | 2% | over ₹50,000 a month |
| `commission` | Commission or brokerage | 194H | 1(ii) | 1006 | 2% | over ₹20,000 |
| `interest` | Interest, payer not a bank | 194A | 5(iii) | 1022 | 10% | over ₹10,000 |
| `property` | Immovable property | 194-IA | 3(i) | none (Form 141) | 1% of the higher of the consideration and the stamp-duty value | ₹50 lakh or more |
| `goods` | Purchase of goods | 194Q | 8(ii) | 1031 | 0.1% on the part over ₹50 lakh | buyer's turnover over ₹10 crore |
| `partner` | Partner's remuneration or interest | 194T | **s.393(3)** Sl. No. 7 | 1067 | 10% | over ₹20,000 |
| `non_resident` | Any sum to a non-resident (not salary) | 195 | s.393(2) Sl. No. 17 | 1057 | **the user's** | every payment |

Every row is [PRIMARY] against the Act from 1 Apr 2026. For earlier dates, the yearly reading of the limits stays [UNCERTAIN] and is shown as a caution.

Rules:

- **No PAN** (s.397(2)(b)(i)): the higher of 20% and the normal rate. For purchase of goods it is **5%**, not 20% [PRIMARY]. For dates before 1 Apr 2026 the 5% carries a "not re-verified" caution.
- **Lower or nil certificate** (s.395): the rate applies within its validity, and only when a PAN is on record. The certificate's amount limit is not modelled.
- **Crossing a yearly limit**: TDS on the whole total, less TDS already deducted. s.393(1)(a) says "on the entire amount … where the amount or aggregate of amounts exceeds the threshold".
- **Rounding** to the nearest rupee, halves up [UNCERTAIN: the Act's s.516 ₹10 rounding is for total income and tax payable, not each deduction].
- **Not modelled**:
  - advances (TDS is due at the earlier of credit or payment; tag the advance line or deduct on the bill);
  - bank interest limits;
  - director's fees (nil limit);
  - dividends, lottery, insurance commission, e-commerce, virtual digital assets, cash withdrawals, rent paid by individuals.

## Non-residents (old s.195; new s.393(2))

Rates depend on the tax treaty, the kind of income and the payee's documents, so **KhataOS ships no non-resident rates**.

- Tag the expense ledger with `non_resident`.
- On the party, record `NonResidentTerms` with `editLedgerNonResident`:
  - **the rate you determined**, including any surcharge and cess that apply;
  - whether it is the **treaty** rate (Form 144 "B") or the Act's ("A");
  - the **country** (Form 144 code list, e.g. 267 USA);
  - the **nature of remittance** (e.g. 21 fees for technical services, 49 royalty, 27 interest, 99 other);
  - the payee's **foreign tax number**.
- Without a rate, the suggestion asks for one and calculates nothing.
- Without a PAN: 20%, or your rate if higher. s.397(2)(c) can relax this on prescribed conditions (a caution says so).
- Every suggestion and the Form 144 report remind you to file **Form 145 (ex-15CA)**, and where required **Form 146 (ex-15CB)**, before remitting. The new form numbers are [SECONDARY]: verify on the e-filing portal.
- Payments to non-residents appear on Form 144 (`tds_quarterly_nr`), not Form 140. Keep a separate TDS payable ledger tagged `non_resident`, so that its challans match. Challans from an all-natures ledger go to Form 140.

## TCS (s.394 of the 2025 Act, from 1 Apr 2026)

| Code | Nature | Old | Table | Collection code | Rate | Limit |
|---|---|---|---|---|---|---|
| `liquor` | Alcoholic liquor for human consumption | 206C(1) | Sl. No. 1 | 1068 | 2% | none |
| `tendu` | Tendu leaves | 206C(1) | Sl. No. 2 | 1069 | 2% | none |
| `timber_lease`, `timber_other`, `forest_produce` | Timber; other forest produce under a lease | 206C(1) | Sl. No. 3 | 1070, 1071, 1072 | 2% | none |
| `scrap` | Scrap | 206C(1) | Sl. No. 4 | 1073 | 2% | none |
| `minerals` | Coal, lignite, iron ore | 206C(1) | Sl. No. 5 | 1074 | 2% | none |
| `motor_vehicle` | Motor vehicle | 206C(1F) | Sl. No. 6 | 1075 | 1% on the whole consideration | a sale over ₹10 lakh |
| `tour_package` | Overseas tour programme package | 206C(1G) | Sl. No. 8 | 1088 | 2% | none |
| `parking`, `toll_plaza`, `mine_quarry` | Use of a parking lot, toll plaza, mine or quarry for business | 206C(1C) | Sl. No. 9 | 1090, 1091, 1092 | 2% | none |

The 2% rates for Sl. Nos. 1, 2, 4 and 5, and for the tour package, come from the Finance Act 2026 s.85, in force 1 Apr 2026 [PRIMARY].

Left out:

- LRS remittances (authorised dealers only).
- The "other goods" the Central Government notifies under Sl. No. 6(b): the notification was not retrieved.
- s.206C(1H) TCS on sale of goods, which was omitted from 1 Apr 2025.

Rules:

- **No PAN** (s.397(2)(b)(ii)): twice the rate or 5%, whichever is higher, at most 20%. Not for a non-resident without a permanent establishment in India.
- **Declaration** (s.394(2)): for Sl. Nos. 1-5, a resident buyer declares the goods are not for trading. Nothing is collected and the statement flag is B. A declaration without a PAN is void, and the no-PAN rate applies.
- **Excluded buyer**: a government, embassy, public sector company, club, or retail buyer for personal consumption (s.402(6)). Mark the party "excluded" and nothing is collected.
- **Lower-collection certificate** (s.395(3)): flag A, and only with a PAN.
- **Who collects**: individuals and HUFs only above ₹1 crore (business) or ₹50 lakh (profession) turnover in the previous year (s.402(33)). Shown as a condition.
- **The amount**: what the buyer pays, **GST included** [UNCERTAIN]. The suggestion takes the amount you pass. The reports add the voucher's GST output lines to its largest nature.
- **Deposit**: within seven days from the end of the month, March included (old Rule 37CA) [SECONDARY for the 2026 rules].

### How the books are read for TCS

- A **sales ledger** gets a nature of collection, for example Scrap Sales → `scrap`. The **party** gets the buyer's terms. A **TCS payable ledger** under Duties & Taxes holds one nature or `all`.
- TCS settings live in `Ledger.tcs`, separate from `Ledger.tds`, so one party can be both a deductee and a collectee.
- Example sales voucher:

  | Ledger | Debit | Credit |
  |---|---|---|
  | Kabadi Traders | 1,20,360 | |
  | Scrap Sales | | 1,00,000 |
  | Output CGST | | 9,000 |
  | Output SGST | | 9,000 |
  | TCS Payable | | 2,360 |

  Base ₹1,18,000, TCS 2%.
- **A challan** is a voucher that debits a TCS payable ledger and has no nature line.
- **Advances**: TCS on a receipt before the invoice is not detected (the party line is not counted). Collect it on the invoice, or tag the receipt's income line.

## Salary TDS estimate (s.392; new regime, s.202(1))

`salary_estimate` and `SalaryTdsEstimateReport` give **an estimate for one employee, not payroll**, for tax year 2026-27 only. Other years are refused.

| Step | Rule | Source |
|---|---|---|
| Standard deduction | ₹75,000, or the salary if less | s.19(1) Sl. No. 2 |
| Slabs | Nil to ₹4L · 5% to ₹8L · 10% to ₹12L · 15% to ₹16L · 20% to ₹20L · 25% to ₹24L · 30% above | s.202(1), unchanged by the Finance Act 2026 |
| Rebate (resident) | the tax up to ₹60,000 when total income ≤ ₹12 lakh; marginal relief above | s.156(2) |
| Cess | 4% Health and Education Cess | Finance Act 2026 s.3(10), (16) |
| Rounding | total income and tax to ₹10 | s.516 |
| Monthly | (the year's tax − TDS so far) ÷ months left, to the rupee | common practice; the Act says "average rate" (s.392(1)) |

All [PRIMARY].

Not modelled:

- surcharge: total income over ₹50 lakh is flagged as "too low";
- the old regime for those who opt out;
- perquisites, allowances, arrears relief;
- Form 138 and Form 130.

## The quarterly statement text file

From tax year 2026-27, the statement files follow Protean's "File Format for Form Number 140 / 143 / 144, Q1 to Q4, Version 1.2" (22 Jul 2026) [PRIMARY]:

- an ASCII `.txt` file, `^`-delimited, with CR LF line ends;
- records FH (18 fields), BH (72), one CD (30) per challan, and one DD (45) per row;
- amounts with two decimals, rates with four;
- dates as `ddmmyyyy`;
- 4-digit tax codes;
- the statement's own state codes (Maharashtra 19, not the GST code 27).

The tests compare the output field by field with Protean's sample files. The `CSV` export stays as a working list.

Before a file can be made:

1. **Deductor details** (`Ctrl+Alt+D` on a quarterly report, or `editTdsDeductor`):
   - TAN, kept in this company file's settings under `tds.tan`;
   - deductor type: K company, F firm, Q individual/HUF, and so on;
   - PIN, email, phone;
   - the responsible person's name, designation (up to 20 characters) and PAN.

   The PAN, address and state default to the company GSTIN and address. These settings are shared records of the company: synced with the books (`docs/sync-settings.md`), not in the audit trail. The statement file's sequence number stays a device-local counter.
2. **Challan bank details** (Enter on a challan in a quarterly report): the 7-digit BSR code, the challan serial number and the date of deposit, plus any interest, fee or other parts. Stored per challan voucher.
3. **Rows to challans**: rows are matched to challans of the same nature (or an all-natures ledger) in date order, while the challan's tax lasts. Challans paid up to the quarter's last deposit date (for Q1: 7 July) are included.

`Ctrl+Alt+X` saves the file. If anything is missing, it lists the problems instead.

### What to verify in the FVU

1. The file validates with **FVU 1.2** (statements for 2026-27 onward), with the `.csi` challan file from TIN imported in the RPU or FVU. KhataOS does not check challans against the `.csi` file.
2. **Deductee record numbers** restart at 1 in each challan (DD field 4). The Form 144 spec says "in a batch" [UNCERTAIN].
3. **CD fields 24-25** (interest and other allocation) are filled from the challan's interest and others, as the spec says. The samples leave them empty.
4. **The overseas tour package** is reported with code 1088. After the 2026 amendment there is one rate, but the format still lists 1088 and 1089.
5. **Opting out of s.202(1)** is reported as "N", and amounts as not **grossed up**. Change them in the RPU where wrong.
6. **Status code** 10 (Others) is used for trusts, local authorities and governments. Co-operatives (6) cannot be recorded yet.
7. **Challans across quarters**: a challan paid in the first days of a quarter can be offered to both the previous quarter's rows and this quarter's. Check its unconsumed balance in the RPU or on TRACES.
8. **Not written**:
   - corrections;
   - government deductor fields (AIN, ministry, state name);
   - Form 121 numbers;
   - the Form 145 acknowledgement number (Form 144 DD 31).

## Due dates

| What | Date | Confidence |
|---|---|---|
| Paying TDS | 7th of the next month; 30 April for March | Old Rule 30; the same under the 2026 rules (Rule 218) per secondary sources |
| Paying TCS | 7th of the next month, March included | Old Rule 37CA; secondary for the 2026 rules |
| Quarterly statements, TY 2026-27 onward | 31 Jul, 31 Oct, 31 Jan, 31 May (TDS and TCS) | Rule 219(4) [SECONDARY] |

Interest and fees for late deduction, payment or filing are not computed.

## In the app

- **Vouchers** (`voucher.ts`): on Sales and Receipt, `tcsApi.suggest(date, party_id, sales_ledger_id, amount)` is asked shortly after a change, with `amount` the credit of the sales or income lines of the nature (or the buyer's nature) plus the voucher's GST lines. When it `applies`, an auto line credits `payable_ledger_id` and the party's debit (else a cash or bank debit) grows by the same amount; an item invoice's total and a single-entry Receipt's account follow by themselves. The line is tagged "auto TCS <collection code> <rate>%". The explanation and warnings show once. Typing over the amount makes it the user's own line; Ctrl+D removes it (the party's debit drops back) until the base, party or date change. An item invoice gets its TCS once its items are done, after the GST lines.
- A non-resident payee (`non_resident` nature): the rate request and the Form 145/146 reminder from `tdsApi.suggest` show as a warning.
- **Ledgers** (`ledger.ts`): India party ledgers have a TCS section (buyer type, the s.394(2) declaration, "not a buyer under the Act", a lower-collection certificate) next to the TDS one; sales and income ledgers a nature of collection; a duty ledger's Type of tax offers "TCS payable" with the nature it holds. A party abroad (or one already saved as a non-resident) gets the non-resident terms: the rate, treaty or Act, Form 144 country (filled from the mailing country), nature of remittance and foreign tax number, and for TCS whether it has a permanent establishment in India. `LedgerDto` carries `tcs`, and the TCS settings are saved with `set_ledger_tcs` after the ledger itself. The alteration form shows the saved TDS and TCS settings in one line.
- **Launcher** (`launcher.ts`): TCS Payable, TCS Quarterly Statement, TDS on payments to non-residents, Salary TDS estimate and Deductor details (TAN).

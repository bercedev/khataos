# India: TCS, non-resident TDS, salary TDS and the quarterly statement file

Research for `crates/khata-in/src/{tcs.rs, tds.rs, salary.rs, tds_file.rs}`, done 20 Oct 2026. It adds to `india-compliance.md` §5, which was based on secondary sources. Most figures below come from primary sources: the gazetted Acts and Protean's published file formats.

Confidence tags:

- **[PRIMARY]**: read in the official text. The page or line is cited.
- **[SECONDARY]**: from practitioner or news sources only.
- **[UNCERTAIN]**: no source settles it, or the sources conflict. The code shows a caution or leaves it out.

## Sources read

| # | Source | What it is | Where |
|---|---|---|---|
| S1 | Income-tax Act, 2025 (No. 30 of 2025), Gazette of India Extraordinary Part II s.1, 21 Aug 2025 | The Act as passed. Commences 1 Apr 2026 | `https://egazette.gov.in/WriteReadData/2025/265620.pdf` |
| S2 | Finance Act, 2026 (No. 4 of 2026), assented 30 Mar 2026, gazette CG-DL-E-31032026-271439 | Rates for tax year 2026-27, and amendments to the 2025 Act | LiveLaw copy of the gazette PDF: `https://www.livelawbiz.com/pdf_upload/2026/03/31/finance-act-2026-664737.pdf`. The corrigendum (22 Apr 2026, `https://egazette.gov.in/WriteReadData/2026/271980.pdf`) touches no TDS or TCS section |
| S3 | Finance Bill, 2026, as introduced (Bill No. 3 of 2026), with its Notes on Clauses | The intent behind the TCS changes. Enacted unchanged as S2 s.85 | `https://www.indiabudget.gov.in/doc/Finance_Bill.pdf` |
| S4 | Protean (TIN) "File Format for Form Number 140 / 143 / 144 Q1 to Q4 Version 1.2", files dated 22 Jul 2026 | The official `^`-delimited text file layout for 2025-Act statements | `https://tinpan.proteantech.in/downloads/e-tds/eTDS-download-regular.html`, files `Form Number 140 - 26Q - Q1 to Q4_22072026.xlsx`, `Form Number 143-27EQ - Q1 to Q4_22072026.xlsx`, `Form Number 144-27Q - Q1 to Q4_22072026.xlsx`. Each sheet's own title says "Version 1.1"; the download page says 1.2 |
| S5 | Protean sample files `140RQ1.txt`, `143.txt`, `144RQ1.txt` | Example statements in the S4 format | Same page as S4 |
| S6 | Protean "Key Features: RPU and FVU version 1.2" | What changed from 26Q, 27Q and 27EQ | Same page as S4. A copy is also at `https://blog.tdsman.com/downloads/key_features_fvu_1_2.pdf` |
| S7 | TDSMAN blog, BusinessToday, TaxGuru, eximpe, calcguru, Outlook Money | Commentary | Cited where used |

`https://www.incometaxindia.gov.in` returned HTTP 403 to automated fetches, so its section pages (`/w/section-394-6` and others) were not read directly. S1 and S2 are the same text, taken from the Gazette.

## 1. TCS: s.394 of the 2025 Act

### Rates and thresholds

S1 p.417-418, s.394(1) Table, as amended by S2 s.85 [PRIMARY]. S2 s.1(2)(a) brings s.85 into force on **1 Apr 2026**, the same day the 2025 Act commences. The pre-amendment rates in the Act therefore never applied.

| Sl. No. | Nature of receipt | Who collects | Rate as enacted (S1) | Rate from 1 Apr 2026 (S2 s.85) | Threshold | Code (S4 143, Annex. 2) | In KhataOS |
|---|---|---|---|---|---|---|---|
| 1 | Sale of alcoholic liquor for human consumption | Seller | 1% | **2%** | none | 1068 | `liquor` |
| 2 | Sale of tendu leaves | Seller | 5% | **2%** | none | 1069 | `tendu` |
| 3 | Sale of timber (forest lease or otherwise), or other forest produce under a forest lease | Seller | 2% | 2% (unchanged) | none | 1070 / 1071 / 1072 | `timber_lease`, `timber_other`, `forest_produce` |
| 4 | Sale of scrap | Seller | 1% | **2%** | none | 1073 | `scrap` |
| 5 | Sale of minerals: coal, lignite, iron ore | Seller | 1% | **2%** | none | 1074 | `minerals` |
| 6 | Sale consideration exceeding ₹10 lakh: (a) motor vehicle; (b) other goods the Central Government notifies | Seller | 1% | 1% | over ₹10 lakh | 1075 (vehicle); 1076-1085 (notified goods) | `motor_vehicle` only |
| 7 | LRS remittance over ₹10 lakh | Authorised dealer | (a) 5% education or medical, (b) 20% other | (a) **2%**, (b) 20% | over ₹10 lakh | 1086 / 1087 | left out: banks only |
| 8 | Sale of an overseas tour programme package | Seller | 5% up to ₹10 lakh, 20% above | **2% on any amount** (S3 notes: "irrespective of the amount") | none | 1088 / 1089 | `tour_package`, reported as 1088 [UNCERTAIN] |
| 9 | Use of a parking lot, toll plaza, mine or quarry for business (not mineral oil) | Licensor or lessor | 2% | 2% | none | 1090 / 1091 / 1092 | `parking`, `toll_plaza`, `mine_quarry` |

Other rules:

- **When to collect**: s.394(1)(c) says on debit to the buyer's account or on receipt, whichever is earlier [PRIMARY].
- **Declaration**: s.394(2) says nothing is collected for Sl. Nos. 1 to 5 when a resident buyer gives a written declaration that the goods are for manufacturing, processing, producing or power, and not for trading [PRIMARY]. The form is reported as Form 127 [SECONDARY: TDSMAN]. S4 uses flag "B" for this.
- **Tour packages and LRS**: s.394(4)-(5) exclude amounts already collected on a tour package and education loans. Nothing is collected if the buyer is liable to deduct TDS and has deducted it [PRIMARY]. KhataOS shows these as a caution only.
- **Who is a seller**: s.402(33). For Sl. Nos. 1-6 this is a government, a local authority, a statutory corporation, a company, a firm or a co-operative. An individual or HUF counts only if its turnover in the preceding tax year exceeded ₹1 crore (business) or ₹50 lakh (profession) [PRIMARY]. KhataOS shows this as a condition.
- **Who is a buyer**: s.402(6) Table [PRIMARY].
  - For Sl. Nos. 1-5 the buyer is not a public sector company, a government, an embassy, high commission, consulate or trade representation, a club, or a retail buyer for personal consumption.
  - For Sl. No. 6 the buyer is not a government or embassy, a local authority, or a public sector company carrying passengers.
  - KhataOS has an "excluded" flag on the party.
- **Scrap**: s.402(31) defines it as "waste and scrap from the manufacture or mechanical working of materials which is definitely not usable as such…" [PRIMARY].
- **No PAN**: s.397(2)(b)(ii) sets the higher of twice the rate and 5%, but not more than 20% [PRIMARY]. Under s.397(2)(d) this does not apply to a non-resident without a permanent establishment in India [PRIMARY]. Under s.397(2)(f)(i) and (g), a declaration without a PAN is invalid and tax is collected at the no-PAN rate [PRIMARY].
- **Lower-collection certificate**: s.395(3) [PRIMARY]. s.397(2)(f)(ii) says no certificate is granted without a PAN.
- **206C(1H)** (TCS on sale of goods) was omitted from 1 Apr 2025. Only the buyer's TDS on purchase of goods remains [PRIMARY: earlier research]. s.393(1) Sl. No. 8(ii) Note 1(a) says TDS on purchase of goods does not apply where TCS is collectible [PRIMARY].

Open points:

- **The amount TCS is charged on.** The Act says "amount payable by the buyer". Whether GST is part of it is not settled by S1-S3. KhataOS takes the GST-inclusive amount the buyer pays [UNCERTAIN].
- **Motor vehicle limit.** "Sale consideration exceeding ten lakh rupees" is read per sale [UNCERTAIN: the Act does not say per vehicle or per buyer].
- **Notified goods.** The Sl. No. 6(b) notification under the 2025 Act was not retrieved. S4's code list names wrist watches, art, collectibles, yachts, sunglasses, bags, shoes, sportswear, home theatres and horses. These are left out [UNCERTAIN].
- **Tour package code.** After the 2026 amendment there is only one rate, but S4 still has two codes (1088 for "up to threshold" and 1089 for "above"). KhataOS writes 1088 [UNCERTAIN].
- **Deposit date for TCS.** Within seven days from the end of the month, March included. This is old Rule 37CA. The Income-tax Rules 2026 equivalent comes from secondary sources only: Rule 218 per TaxGuru, `https://taxguru.in/income-tax/7-october-2026-tds-tcs-deposit-deadline-income-tax-act-2025-compliance-guide.html` [SECONDARY].
- **Statement due dates.** 31 Jul, 31 Oct, 31 Jan and 31 May, for TCS as well, under Rule 219(4) [SECONDARY, from the earlier research].
- **Collections before 1 Apr 2026.** These fall under s.206C of the 1961 Act, whose rates were not re-verified. `tcs::compute` refuses them.

## 2. Non-residents: s.393(2) (ex-s.195)

- s.393(2) Table, Sl. No. 17: "Any interest … or any other sum chargeable under the provisions of this Act, not being income chargeable under the head 'Salaries'" paid to a non-resident (not a company) or a foreign company, by any person, at **"Rates in force"** [PRIMARY: S1 p.407]. S2 First Schedule Part II gives those rates by kind of income. Some examples: 20% on investment income of a non-resident Indian, 12.5% on some long-term gains, 20% on some royalties [PRIMARY: S2 pp.89-90]. Surcharge and the 4% cess apply to non-residents (S2 s.3(16)-(17)).
- **Treaty rates.** The Act gives treaty relief through s.159 agreements. S1 Note 2 to s.393(2) applies the treaty rate for some serial numbers when the payee gives the certificate under s.159(8). The right rate depends on the treaty, the kind of income and the payee's documents.
  - **KhataOS ships no non-resident rates.** The nature `non_resident` (tax code 1057, from S4 144 Annexure 2) takes the user's rate from the party (`NonResidentTerms.rate`).
  - The user marks whether the rate is the treaty's ("B") or the Act's ("A"). This is the S4 144 DD field 30.
- **No PAN.** s.397(2)(b)(i)(C) sets 20%, or the user's rate if higher [PRIMARY]. s.397(2)(c) relaxes this for a non-resident on "such conditions as may be prescribed" (the old Rule 37BC). KhataOS shows it as a caution.
- **Lower or nil certificate.** s.395(1). The s.395(2) "proportion" determination is for non-residents [PRIMARY]. S4 flag "A".
- **Form 15CA / 15CB.** s.397(3)(d) requires information on payments to non-residents "in such form … as may be prescribed" [PRIMARY]. Under the Income-tax Rules 2026 these are reported as **Form 145 (ex-15CA)** and **Form 146 (ex-15CB)**, under Rule 220 [SECONDARY: eximpe `https://eximpe.com/blog/payments/form-15ca-15cb-form-145-146`, TaxGuru `https://taxguru.in/?p=1067000`, calcguru]. KhataOS shows a reminder with these numbers, marked to verify.
- **Statement.** Form 144 (ex-27Q) [PRIMARY: S4, S6].
  - The country is coded from S4 144 Annexure 10, which has 285 countries with numeric codes. The United States of America is 267; India is 113.
  - The nature of remittance is coded from Annexure 9. For 1057 the codes are 27 interest, 16 and 73 dividends, 28 investment income, 66-70 capital gains, 21 and 74 fees for technical services, 49 and 75 royalty, 99 other.
  - Business income has no code under 1057 in Annexure 9 [noted: use 99 or check the RPU].

## 3. Salary TDS: s.392 and the new regime

All of the following is [PRIMARY], from S1 and S2:

- **s.392(1)**: deduct "at the time of such payment at the average rate of income-tax computed on the basis of the rates in force for the tax year … on the estimated income of the assessee under this head". s.392(5)(c) lets the employer increase or reduce later deductions to adjust earlier excess or shortfall.
- **s.392(4)(a)**: the employee may give salary from other employers, house property loss, other income and other TDS or TCS. These change the deduction.
- **s.202(1), the new tax regime.** It is the default for an individual, HUF, AOP, BOI and artificial juridical person unless the person opts out under s.202(4). Slabs:

  | Total income | Rate |
  |---|---|
  | Up to ₹4,00,000 | Nil |
  | ₹4,00,001 to ₹8,00,000 | 5% |
  | ₹8,00,001 to ₹12,00,000 | 10% |
  | ₹12,00,001 to ₹16,00,000 | 15% |
  | ₹16,00,001 to ₹20,00,000 | 20% |
  | ₹20,00,001 to ₹24,00,000 | 25% |
  | Above ₹24,00,000 | 30% |

  S2 s.56 changes only s.202(2)(a), the list of disallowed deductions. The slabs stand for tax year 2026-27.
- **The regime number.** The 2025 Act numbers the new regime **s.202** (formerly s.115BAC). KhataOS uses that number.
- **Standard deduction**: s.19(1) Table Sl. No. 2 gives ₹75,000 or the salary, whichever is less, under s.202(1). Otherwise it is ₹50,000.
- **Rebate**: under s.156(2), a resident individual chargeable under s.202(1) gets:
  - if total income is not more than ₹12 lakh: the tax or ₹60,000, whichever is less;
  - above ₹12 lakh: marginal relief, so the tax does not exceed the income above ₹12 lakh.

  The rebate is capped at the tax (s.156(3)).
- **Cess**: S2 s.3(10)(ii) and (16) say salary TDS is at the Part III rates, plus surcharge, plus the 4% Health and Education Cess. Part III Paragraph A of S2 sets out the old regime's slabs, for those who opt out.
- **Surcharge** (S2 First Schedule Part III Paragraph F): 10% over ₹50 lakh and 15% over ₹1 crore [PRIMARY]. S2's TDS surcharge tables show a 25% top rate for income chargeable under s.202, but the Part III cap for s.202 was not traced line by line [UNCERTAIN]. **Not modelled.** The estimate flags any total income over ₹50 lakh.
- **Rounding**: s.516 rounds total income and the tax payable to the nearest ₹10.
- **Statements**: Form 138 (ex-24Q) and Form 130 (ex-Form 16) [PRIMARY: S6; earlier research]. Not generated.

The convention that monthly TDS equals the remaining annual tax divided by the remaining months is common practice. The Act only requires the "average rate" [UNCERTAIN as a rule, but standard].

## 4. The statement text file (S4, S5, S6)

- **Format still in force?** Yes, with new values. The 2025-Act statements keep Protean's `^`-delimited text format, validated by the new FVU 1.x series [PRIMARY: S4, S6]. The download page lists "File Format for Form Number 140/143/144 Q1 to Q4 Version 1.2", RPU 1.2 and "e-TDS/TCS FVU.exe (Version 1.2)". It says FVU 9.5 and 2.191 cover statements up to FY 2025-26.
  - FVU 1.0 was reported released on 1 Jul 2026 and applies from the quarter ending 30 Jun 2026 [SECONDARY: TDSMAN `https://blog.tdsman.com/2026/07/new-file-validation-utility-fvu-version-1-0-released-on-1st-july-2026/`].
- **General rules** (S4 "General Notes") [PRIMARY]:
  - ASCII, `.txt`;
  - each record on its own line ending CR LF;
  - constants in capitals;
  - amounts with two decimals, and amounts that must be whole are written `2345.00`;
  - the DD rate with four decimals (`2.0000`);
  - dates as `ddmmyyyy`;
  - `^`-delimited, variable width, with one delimiter fewer than the field count;
  - at least one deductee or collectee row in a regular statement;
  - the RPU and FVU check challans against the `.csi` file from TIN.
- **Records and field counts** (S4 tables; the same counts in S5) [PRIMARY]:

  | Record | Fields | Notes |
  |---|---|---|
  | FH | 18 | File type `NS1` for 140 and 144, `TC1` for 143 (`SL1` for 138); upload type `R`; uploader type `D`; TAN; batch count; utility name |
  | BH | 72 | Batch header. Form number at 5, TAN at 13, PAN at 15. Assessment year at 16 (`202728` for tax year 2026-27) and tax year at 17 (`202627`). Quarter at 18 (`Q1` and so on). Deductor name and address at 19-27, including address 6 = country at 20, the Annex. 1 state code at 26 and the PIN at 27. Email at 28, calling code at 29, phone at 30, deductor type at 32 (Annex. 4). Responsible person at 33-45 and 59. Total deposit at 47. Previous regular statement Y/N at 52, its token at 9. GSTIN at 69 |
  | CD | 30 | One per challan. Tax, interest, fee and others at 8-11 (whole rupees), total at 12, mode `C`/`B` at 13, BSR at 15, challan serial at 17, deposit date at 19. Sum of deductee rows at 21-22, minor head at 23 (200 or 400), interest and other allocation at 24-25 |
  | DD | 45 | One per deductee or collectee row. PAN at 8 (`PANNOTAVBL` when there is none), name at 9, 4-digit code at 15, deposited Y/N at 18, date paid at 19, amount at 20, tax deducted and deposited at 24-25, date of deduction at 27, rate at 28, flag at 32, certificate number at 33. Form 143 adds non-resident at 11, status code at 12, PE at 13 and opting out of s.202(1) at 14. Form 144 adds status at 12, opting out at 14, nature of remittance at 16, country at 17, grossed up at 21, Act or DTAA at 30, the Form 15CA (Form 145) acknowledgement at 31, and the foreign address, email, phone and TIN at 37-41 |

- **Code lists** (S4 annexures) [PRIMARY]:
  - Annex. 1 has statement state codes, which are not GST codes (Maharashtra 19, Delhi 09, Ladakh 37).
  - Annex. 2 has the 4-digit tax and collection codes. Codes for the TDS natures KhataOS has:

    | Code | Nature |
    |---|---|
    | 1023 / 1024 | contract (individual or HUF / other) |
    | 1026 | technical |
    | 1027 | professional |
    | 1008 | rent of machinery |
    | 1009 | rent of other assets |
    | 1006 | commission |
    | 1022 | interest, s.393(1) Sl. No. 5(iii) |
    | 1031 | goods |
    | 1067 | partner, s.393(3) Sl. No. 7 |
    | 1057 | non-resident, s.393(2) Sl. No. 17 |

  - Annex. 4 has deductor types: K company, F firm, Q individual or HUF, and others.
  - Annex. 6 has flags: A certificate, B declaration, C no PAN, Y below threshold, and others.
  - Annex. 7 has minor heads.
  - Annex. 8 (143 and 144) has status codes: 1 company, 2 individual, 3 HUF, 4 AOP, 5 AOP of companies, 6 co-operative, 7 firm, 8 BOI, 9 artificial juridical person, 10 others.
  - Annex. 9 and 10 are listed above.
- **The s.393 tables, checked** [PRIMARY: S1 pp.398-410, S2 Part II]. The seed TDS natures in `tds.rs` match the Act:

  | Nature | Section and Sl. No. | Rate | Threshold |
  |---|---|---|---|
  | Contract | 393(1) 6(i) | 1% / 2% | ₹30,000 per sum or ₹1,00,000 aggregate |
  | Professional | 393(1) 6(iii) | 10% | ₹50,000 |
  | Technical | 393(1) 6(iii) | 2% | ₹50,000 |
  | Rent | 393(1) 2(ii) | 2% / 10% | ₹50,000 for a month or part of a month |
  | Commission | 393(1) 1(ii) | 2% | ₹20,000 |
  | Interest | 393(1) 5(iii) | "rates in force", 10% in S2 Part II | ₹10,000; banks under 5(ii): ₹50,000, or ₹1,00,000 for senior citizens |
  | Property | 393(1) 3(i) | 1% of the higher of consideration and stamp duty value | ₹50 lakh, "equal to or greater" |
  | Goods | 393(1) 8(ii) | 0.1% on the sum over ₹50 lakh | buyer's previous-year turnover over ₹10 crore (s.402(6)) |
  | Partner | **393(3)** 7 | 10% | ₹20,000 |

  - s.393(1)(a) deducts "on the entire amount … where the amount or aggregate of amounts exceeds the threshold". This supports KhataOS's catch-up on the year's total.
  - The no-PAN rate is 20%, but **5% for purchase of goods** under s.397(2)(b)(i)(C). The earlier seed used 20% for goods. That is corrected for 2025-Act dates.

### What KhataOS writes, and what the FVU must confirm

`khata_in::tds_file::render` writes Forms 140, 143 and 144 with the field counts and positions above. The tests compare its output field by field with S5. Before relying on it, confirm these in the FVU [UNCERTAIN]:

1. **Deductee record numbering.** DD field 4 restarts at 1 in each challan, as in the older formats. The Form 144 text says "running serial number … in a batch".
2. **CD fields 24-25.** The interest and other allocations are filled from CD 9 and 11, as the spec says, though the samples leave them empty.
3. **DD field 18** is always `Y` (deposited). The rows are allocated to challans with enough tax left.
4. **The tour package code** is 1088 (see §1).
5. **Opting out of s.202(1)** is reported as `N` (Form 143 DD 14 and Form 144 DD 14), and **grossed up** as `N` (Form 144 DD 21).
6. **Status code** for a trust, local authority or government is 10 (Others).
7. **Not written:** corrections, government deductor fields (AIN, ministry, state name), Form 121 numbers, and the Form 145 acknowledgement number. The `.csi` challan check is not run.

# United States Compliance Requirements (Sales Tax, 1099): KhataOS Spec

> **Status as of:** 2026-10-20. **Scope:** US businesses keeping books in KhataOS, a local-first desktop accounting app. This covers state and local sales and use tax (the US has no federal VAT or sales tax), the IRS Employer Identification Number (EIN), and the payer side of Forms 1099-NEC, 1099-MISC and 1099-K. A planned `khata-us` pack would implement the items in §8.
> **Not legal advice.** This file turns IRS publications, the Internal Revenue Code as amended by P.L. 119-21, and state revenue-department guidance into software requirements. Get a US CPA or enrolled agent, and a sales-tax specialist for each state where the company files, to confirm each item before release.
>
> **Confidence tags:**
> - **[HIGH]** means the text was read on a primary source: irs.gov (publications, instructions, Internal Revenue Manual), the enacted statute on govinfo.gov or congress.gov, supremecourt.gov, or a state revenue department's own site.
> - **[MEDIUM]** means it comes from a primary source seen only as a search summary, or is reported consistently by reputable secondary sources (Tax Foundation, Avalara, the Multistate Tax Commission or the Streamlined Sales Tax Governing Board describing themselves).
> - **[LOW]** means a single secondary source, or a figure that has changed between editions.
> - **UNCERTAIN** means sources conflict, the point is interpretive, or it was not found. Flagged for review.

---

## 0. MVP must-do checklist (summary)

| # | Capability | Driven by |
|---|---|---|
| 1 | **No built-in rate table.** The company defines its own jurisdictions (state, county, city, district) and rates, all effective-dated; tax is computed and reported per jurisdiction | 45 states + DC levy sales tax, 38 states allow local taxes, 12,000+ jurisdictions (§1) |
| 2 | **Nexus monitor**: running totals of sales (and, where a state still counts them, transactions) by ship-to state against each state's threshold and lookback period | *South Dakota v. Wayfair* (2018); state rules (§2) |
| 3 | **Exempt and resale customers**: certificate on file (type, state, number, expiry), with a tax-exempt flag on the line | SST Certificate of Exemption (F0003); MTC Uniform Resale Certificate (§3.2) |
| 4 | **Marketplace sales** kept apart: the facilitator collects, the seller reports them as non-taxable | CA, NY, FL marketplace rules (§3.3) |
| 5 | **EIN validation**: 9 digits, `NN-NNNNNNN`, a prefix check against the IRS list (warn, do not reject); no check digit | IRS *Valid EINs*; IRM 21.7.13.2.4 (§5) |
| 6 | **1099 report**: vendor payments for services by TIN, per calendar year, against **$2,000** (payments after 31 Dec 2025; $600 before), excluding card and third-party-network payments and most payments to corporations | IRC §6041 as amended by P.L. 119-21 §70433; i1099mec (Rev. 12/2026) (§6) |
| 7 | **W-9 / TIN on file**; flag backup withholding at **24%** when there is no TIN or a wrong one | IRS *Backup withholding* (§6.3) |
| 8 | Fiscal year chosen by the company; USD; records kept **≥ 3 years** (IRS) and **≥ 4 years** (CA, TX sales tax) | Pub 538; IRS *How long should I keep records*; CDTFA Pub 116 (§7) |

---

## 1. Sales tax is state and local

| Fact | Source | What the software must do |
|---|---|---|
| The US has **no federal VAT or general sales tax**; sales and use tax is levied by states and localities. "Forty-five states levy a state-level sales tax" | Tax Foundation, *State and Local Sales Tax Rates, 2026* (state rates as of 1 Jul 2026) [MEDIUM: think tank, not a government source; the federal point is general knowledge and no statute grants a federal sales tax] | No national rate. Tax config is per state |
| **District of Columbia** levies 6.00% and appears in the same table | Tax Foundation 2026 table [MEDIUM]. Whether the "45" counts DC: the page does not say. Commonly written "45 states + DC" [MEDIUM] | Treat DC as a jurisdiction like a state |
| **No statewide sales tax: Alaska, Delaware, Montana, New Hampshire, Oregon**. "Of these, only Alaska allows localities to impose local sales taxes." | Tax Foundation 2026 [MEDIUM] | An Alaska company may still need city or borough rates with no state component |
| **38 states allow local sales taxes** (including Alaska) | Tax Foundation 2026 [MEDIUM] | Local components are the normal case, not the exception |
| **Number of jurisdictions**: "over 12,000 sales and use tax jurisdictions" (Avalara, 1 Aug 2025); earlier Avalara editions (2023) said "over 13,000"; Tax Foundation, citing Vertex, counted "over 11,000 standard" jurisdictions in 2020; Texas alone has over 1,600 | Avalara blog *A handy guide to US sales tax jurisdictions* [LOW: vendor figure, changed between editions]; Tax Foundation *State Sales Tax Jurisdictions in the US, 2020* [MEDIUM, from a search summary] | Read as "roughly 11,000 to 13,000". KhataOS cannot ship or maintain these rates (§8) |

---

## 2. Economic nexus after *Wayfair*

**The case.** *South Dakota v. Wayfair, Inc.*, No. 17-494, argued 17 Apr 2018, **decided 21 Jun 2018**, 585 U.S. 162. Syllabus: "Because the physical presence rule of Quill is unsound and incorrect, Quill Corp. v. North Dakota, 504 U. S. 298, and National Bellas Hess, Inc. v. Department of Revenue of Ill., 386 U. S. 753, are overruled." The South Dakota Act upheld there "covers only sellers that, on an annual basis, deliver more than $100,000 of goods or services into the State or engage in 200 or more separate transactions for the delivery of goods or services into the State." [HIGH: preliminary print, supremecourt.gov]

Since then a seller with no physical presence must register and collect in a state once its sales into that state pass the state's **economic nexus** threshold. Each state sets its own test and lookback period.

### 2.1 Thresholds for the five states checked (state sources only)

| State | Threshold | Measurement period | What counts / notes | Source |
|---|---|---|---|---|
| **California** | Total combined sales of tangible personal property for delivery in California (including related persons) **exceed $500,000** | "the preceding or current calendar year" | Applies to sales on and after **1 Apr 2019**, "not otherwise retroactive". A retailer over $500,000 is "engaged in business" in every CA district from 25 Apr 2019 (Pub 105 says 1 Apr 2019: UNCERTAIN which date governs district tax) | CDTFA *Use Tax Collection Requirements Based on Sales into California Due to the Wayfair Decision* [HIGH]; CDTFA district tax guide [MEDIUM, search summary] |
| **Texas** | Total Texas revenue of **$500,000** or more ("less than $500,000" is the safe harbor) | "preceding twelve calendar months" | Collection starts "no later than the first day of the fourth month after the month" the threshold is exceeded. A seller selling only through a marketplace provider that collects "is not required to hold a Texas tax permit" | Texas Comptroller *Remote Sellers* [HIGH] |
| **New York** | Gross receipts from sales of tangible personal property delivered into NY **exceed $500,000 AND more than 100 sales** (both) | "the immediately preceding four sales tax quarters"; NY quarters run Mar–May, Jun–Aug, Sep–Nov, Dec–Feb | Effective 21 Jun 2018; the dollar figure was raised from $300,000 to $500,000 by a 24 Jun 2019 amendment, retroactive to 21 Jun 2018. Sales through an online marketplace: "Yes, those sales should be included in the calculation." Review after each quarter | NY DTF *Registration requirement for businesses with no physical presence in New York State* (updated 29 Jul 2026) [HIGH]; TSB-M-19(4)S [MEDIUM, search summary]. Sovos says sales through a registered facilitator are excluded [LOW]: **UNCERTAIN**, the DTF page says include |
| **Florida** | Taxable remote sales into Florida **exceeding $100,000** (any number of transactions) | "the previous calendar year" | Effective **1 Jul 2021**. Counts *taxable* remote sales only. Remote sellers also collect county discretionary sales surtax. A marketplace seller registers only for sales made outside the marketplace | FL DOR TIP 21A01-03 (14 May 2021) [HIGH]; FL DOR *Account Management and Registration* [HIGH] |
| **Washington** | More than **$100,000** in combined gross receipts sourced or attributed to WA | "the current or prior year" | Includes "sales made through a marketplace facilitator, through your own website or otherwise" and exempt sales. Collection begins on "the first day of the month that starts at least 30 days from the date you met the threshold". The 200-transaction test was removed by **SSB 5581**, effective on signing in **March 2019**; the B&O threshold matched $100,000 from 1 Jan 2020 | WA DOR *Remote sellers* [HIGH]; WA DOR news release (2019) [HIGH] |

### 2.2 Recent changes: the transaction-count test is going away

| State | Change | Source |
|---|---|---|
| **Illinois** | "Effective January 1, 2026 … The 200-transaction threshold no longer applies." Only $100,000 or more in cumulative gross receipts during the lookback period remains (Public Act 104-0006). Retailers registered only because of the transaction test were moved to voluntary use tax | IDOR Bulletin FY 2026-12 [HIGH]; companion FY 2026-13 for service occupation tax [MEDIUM] |
| **Kentucky** | HB 757 (2026) "removed the economic nexus standard of 200 or more sales transactions as a registration requirement for remote retailers (KRS 139.340) and marketplace providers (KRS 139.450)", effective **1 Aug 2026** | KY DOR SSUTA recertification letter, 29 Jul 2026 [HIGH]. Whether the $100,000 test is unchanged: not stated there (UNCERTAIN, presumed unchanged) |
| Others | Avalara counts 16 states that had eliminated the 200-transaction threshold by 1 Jan 2026; New Jersey bills (S711/A3419) pending | Avalara blog [LOW] |

**Software consequence.** Thresholds, the "AND"/"OR" logic, the transaction count and the lookback window are all per-state data that changes. They must be editable, effective-dated rows, not code.

---

## 3. Taxability, certificates, marketplaces, filing, SST, sourcing

### 3.1 Taxability differs by product and by state

| Example | Source |
|---|---|
| **Texas** taxes **data processing services**, but "twenty percent of the charge" is exempt (tax on 80%); Tax Code §151.351 | Texas Comptroller pub. 94-127 *Data Processing Services are Taxable*; STAR 202010013L [MEDIUM, search summary] |
| **New York** taxes **prewritten software** as tangible personal property "regardless of how the software is conveyed"; custom software is not taxed; separately stated, reasonable training or maintenance services are generally exempt | NY DTF TB-ST-128 *Computer Software* [MEDIUM, search summary] |
| **Florida**: general state rate 6% "with the following exceptions" (the page lists special rates) | FL DOR *Florida Sales and Use Tax* [HIGH] |
| **New York** clothing and footwear have their own local rate table (Pub 718-C) | NY DTF *Sales Tax Rate Publications* [MEDIUM] |

So taxability is a property of (item category × state), sometimes a partial percentage. KhataOS cannot ship it. The company marks each item taxable or exempt per jurisdiction it files in, with an optional taxable fraction (for cases like Texas's 80%) [design choice].

### 3.2 Exemption and resale certificates

| Item | Source | What the software must do |
|---|---|---|
| **Streamlined Sales Tax Certificate of Exemption** (SSTGB Form **F0003**): accepted by the SST member states. The buyer gives it to the seller ("Do not send this form to the Streamlined Sales Tax Governing Board"); the buyer is responsible for being eligible in that state; a state sales tax ID is entered where that state requires registration | SSTGB F0003 form and instructions (rev. 12/2021 found) [MEDIUM, search summary] | Customer record: certificate type, state(s), purchaser ID, reason, date received, expiry; store the scan |
| **MTC Uniform Sales & Use Tax Resale Certificate – Multijurisdiction**: 36 states "have indicated can be used as a resale certificate"; the MTC "cannot guarantee that any state will accept this certificate"; footnoted state limits (e.g. Florida requires a resale authorization number; Maryland requires a Maryland registration number) | MTC certificate page and FAQ [MEDIUM] | Same record; a per-state "accepted" note is the user's responsibility |
| Keep resale certificates at least **4 years** (CA motor-vehicle guide) | CDTFA [MEDIUM] | Retention follows §7 |

### 3.3 Marketplace facilitator laws

| State | Rule | Source |
|---|---|---|
| California | Marketplace Facilitator Act, from **1 Oct 2019**: a registered facilitator is the retailer for sales it facilitates; a seller selling only through registered facilitators may not need its own permit; the facilitator's $500,000 test includes facilitated sales | CDTFA *Tax Guide for Marketplace Facilitator Act*; R&TC ch. 1.7; Reg. 1684.5 [MEDIUM, search summary] |
| New York | Marketplace providers collect state and local tax on facilitated sales delivered in NY. A seller holding a **Certificate of Collection (ST-150)** does not collect, but reports the sales as non-taxable (included in gross sales) and keeps the certificate | NY DTF *Sales tax requirements for marketplace providers* (updated 15 Jan 2026) [HIGH] |
| Florida | From 1 Jul 2021, a provider that certifies it will collect: the marketplace seller "may not collect the tax and must exclude sales made through the marketplace from the marketplace seller's tax return" | FL DOR TIP 21A01-03 [HIGH] |
| Texas | A remote seller selling only through a collecting marketplace provider needs no permit, but "must keep" marketplace sales records for at least four years | Texas Comptroller *Remote Sellers* [HIGH] |

Whether every sales-tax state has such a law was not checked here (UNCERTAIN; widely reported as yes). The handling differs (exclude vs report as non-taxable), so a sales channel flag "marketplace-collected" is needed, and the return layout per state decides what to do with it.

### 3.4 Filing frequency is assigned by the state

| State | Rule | Source |
|---|---|---|
| California | CDTFA assigns "quarterly prepay, quarterly, monthly, fiscal yearly, yearly", "based on your reported sales tax or your anticipated taxable sales at the time of registration". Quarterly prepay applies when the measure of tax averages $17,000 or more per month and CDTFA notifies you (R&TC §6471) | CDTFA Pub 51 [HIGH]; CDTFA *Filing Dates* / CDTFA-367 [MEDIUM, search summary] |
| Texas | The Comptroller tells new permit holders by letter "whether they will file monthly or quarterly"; monthly due the 20th of the next month; quarterly due 20 Apr, 20 Jul, 20 Oct, 20 Jan; yearly due 20 Jan | Texas Comptroller *Sales and Use Tax* [HIGH]. Dollar cut-offs for each frequency were not on that page (UNCERTAIN) |
| Florida | By annual tax collected: over $1,000 monthly; $501–$1,000 quarterly; $101–$500 semiannual; $100 or less annual; most new businesses start quarterly | FL DOR *Florida Sales and Use Tax* [HIGH] |
| New York | Quarterly (ST-100) if taxable receipts etc. are under $300,000 in the previous quarter; part-quarterly (monthly) at $300,000 or more in a quarter; annual (ST-101) if the vendor owes $3,000 or less in the annual period | NY DTF *Filing Requirements for Sales and Use Tax Returns* [HIGH] |

**Software consequence:** the filing period per state is a company setting (monthly, quarterly, semiannual, annual, and NY's March–February quarters), not derived.

### 3.5 Streamlined Sales and Use Tax Agreement (SSUTA)

"Today twenty-four states have adopted the simplification measures in the Agreement (representing over 31 percent of the population)" [HIGH: SSTGB *About*]. The site lists **23 full members** (AR, GA, IN, IA, KS, KY, MI, MN, NE, NV, NJ, NC, ND, OH, OK, RI, SD, UT, VT, WA, WV, WI, WY) and **1 associate member (TN)** [MEDIUM: SSTGB pages, search summary]. None of CA, TX, NY, FL is a member; WA is. The Agreement text is amended regularly (latest found: as amended through 19 Dec 2025) [MEDIUM].

### 3.6 Sourcing: origin vs destination

| State | Rule | Source |
|---|---|---|
| Texas | "Local sales tax is imposed on each sale of a taxable item consummated within the local taxing jurisdiction's boundaries", usually the seller's place of business (origin). Local tax is capped: "The maximum amount of local sales and use tax due on a sale in Texas is 2 percent." A remote seller may use local use tax "based on the ship-to location" or the single local use tax rate (currently **1.75%**, published before each calendar year) | Texas Comptroller pub. 94-105 *Local Sales and Use Tax Collection – A Guide for Sellers* [HIGH] |
| California | Hybrid: the statewide 7.25% (with its Bradley-Burns local part) follows the place of sale; **district** use tax follows the delivery address and is owed only where the retailer is "engaged in business" (always so above $500,000). District amounts are reported on Schedule A by county and city | CDTFA *Local and District Tax Guide for Retailers*; Pub 44 [MEDIUM, search summary] |
| Illinois | Bulletin FY 2026-12 is titled "Destination-Based Retailers' Occupation Tax Changes" (from 1 Jan 2026) | IDOR [HIGH for the title; details not read: UNCERTAIN] |

**Software consequence:** each sales line needs both a ship-from and a ship-to jurisdiction; the company picks, per state, whether that state's components follow origin or destination [design choice].

---

## 4. Statewide base rates (local add-ons on top)

| State | State rate | Local add-ons | Source |
|---|---|---|---|
| California | **7.25%** ("The base statewide sales and use tax rate is 7.25 percent") | District taxes "range from 0.10% to 2.00%", sometimes more than one per area; rate page showed rates "Effective October 1, 2026" | CDTFA Pub 51; CDTFA *Sales and Use Tax Rates*; *Tax Rate FAQ* [HIGH] |
| Texas | **6.25%** | Up to 2%, "a maximum combined rate of 8.25 percent" | Texas Comptroller *Sales and Use Tax* [HIGH] |
| New York | **4%** ("the state rate (currently 4%) plus any local tax rate imposed by a city, county, or school district") | Plus **0.375%** in the Metropolitan Commuter Transportation District (MCTD); combined rates by jurisdiction with reporting codes in Pub 718 | NY DTF *Sales Tax Rates, Additional Sales Taxes, and Fees* [HIGH]; Pub 718 [MEDIUM] |
| Florida | **6%** ("Florida's general state sales tax rate is 6% with the following exceptions") | County discretionary sales surtax ("Many Florida counties have a discretionary sales surtax"); rates on DR-15DSS | FL DOR *Florida Sales and Use Tax* [HIGH]; TIP 21A01-03 [HIGH] |
| Washington | **6.5%** | Local rates by city/county, "updated quarterly"; DOR advises checking 30 days before the quarter | Tax Foundation 2026 [MEDIUM]; WA DOR Q1 2026 local rate flyer (state component .0650) [MEDIUM]; WA DOR *Sales & use tax rates* [HIGH for "updated quarterly"] |

**Rates change.** CDTFA: "Rates change often as voters approve new tax districts and old districts expire" [HIGH]. WA publishes quarterly change lists; a 2022 WA flyer says local changes are limited to 1 Jan, 1 Apr and 1 Jul, yet a 2026 notice lists an Oct 1, 2026 change (UNCERTAIN). Tax Foundation reports no state raised its base rate between Jan and Jul 2026 [MEDIUM]. CDTFA warns "it is not always possible to determine the correct tax rate based solely on a mailing address or zip code" [HIGH], so ZIP-only lookup is not enough.

---

## 5. Identifiers

### 5.1 EIN

| Requirement | Source | What the software must do |
|---|---|---|
| "The system assigns a **nine-digit EIN in the format NN-NNNNNNN**. The first two digits (the EIN Prefix) are determined by the Campus of Record assigning the EIN, unless the taxpayer is applying using the online EIN application … or through the Small Business Association (SBA)." | IRM 21.7.13.2.4 (04-30-2026) [HIGH]; IRS *About Form SS-4* example 12-3456789 [MEDIUM, search summary] | Accept with or without the hyphen; store 9 digits; display `NN-NNNNNNN` |
| **Valid prefixes** (IRS *Valid EINs* page, reviewed 09 Apr 2026): Andover 10, 12; Atlanta 60, 67; Austin 50, 53; Brookhaven 01–06, 11, 13, 14, 16, 21, 22, 23, 25, 34, 51, 52, 54–59, 65; Cincinnati 30, 32, 35–38, 61; Fresno 15, 24; Kansas City 40, 44; Memphis 94, 95; Ogden 80, 90; Philadelphia 33, 39, 41, 42, 43, 46, 48, 62, 63, 64, 66, 68, 71–77, 85–88, 91, 92, 93, 98, 99; Internet 20, 26, 27, 33, 39, 41, 42, 45, 46, 47, 81–88, 92, 93, 99; SBA 31 | IRS *How EINs are assigned and valid EIN prefixes* [HIGH] | Prefix table as data |
| The IRM's Philadelphia list differs slightly (adds 82, 83, 84; omits 46). Either way, the union of all lists equals every prefix **except** the ones the IRM calls invalid | IRM 21.7.13.2.4 [HIGH]; cross-check done here | Validate against the complement list below |
| "EIN Prefixes **00, 07, 08, 09, 17, 18, 19, 28, 29, 49, 69, 70, 78, 79, 89, 96 and 97** are considered invalid for input and are no longer being assigned. However, some entities with these prefixes may still be in business; paying taxes and issuing Forms W-2." 69, 70 and 79 were SSA prefixes (1955–1986); 69 was briefly issued by the IRS in early 2002 | IRM 21.7.13.2.4.1 (05-22-2025) [HIGH] | **Warn, do not reject**, on these prefixes (old EINs still exist) |
| "6/7 Million series" EINs (a 6 or 7 right after the hyphen) are reserved for trusts, estates and exempt organizations | IRM 21.7.13.2.4 [HIGH] | Optional hint only |
| **Check digit**: none found in any IRS source read. Validity can only be confirmed by the IRS (TIN Matching for 1099 payers) | Absence in IRM 21.7.13 and the IRS prefix page (UNCERTAIN as a negative finding) | Format + prefix check only |

### 5.2 State sales tax registration numbers

Each state issues its own registration under its own name: a **seller's permit** (CA, CDTFA), a **Texas sales and use tax permit** (Comptroller), a **Certificate of Authority** (NY, applied for through New York Business Express), and a **dealer** registration (FL) [HIGH: the CA, TX, NY and FL pages cited above]. Number formats were not researched (UNCERTAIN). **Software:** free-text registration number per state, no format validation.

### 5.3 Payee TINs on 1099s

A recipient TIN can be an SSN, ITIN, ATIN or EIN; the payee copy may show a truncated TIN (Regulations §301.6109-4); "Enter the recipient's TIN using hyphens in the proper format" [HIGH: i1099mec]. Store the TIN type with the number.

---

## 6. Forms 1099-NEC, 1099-MISC and 1099-K (payer side)

### 6.1 The threshold change (P.L. 119-21, the "One Big Beautiful Bill Act")

| Point | Source |
|---|---|
| **Public Law 119-21**, H.R. 1, 119th Congress, **approved 4 Jul 2025** | Enrolled text, govinfo.gov PLAW-119publ21 [HIGH] |
| **Sec. 70433** "Increase in threshold for requiring information reporting with respect to certain payees": (a) "Section 6041(a) is amended by striking '$600' and inserting '$2,000'." (c) §6041A(a)(2) (remuneration for services, the 1099-NEC rule) now uses "the dollar amount in effect for such calendar year under section 6041(a)". (d) §3406(b)(6) backup withholding uses the same amount. (e) §6041(a) now runs on a "calendar year" | P.L. 119-21 §70433 [HIGH] |
| **Effective date**: "The amendments made by this section shall apply with respect to payments made after December 31, 2025." So payments made **through 31 Dec 2025 use $600**; payments in **2026 use $2,000** | §70433(f) [HIGH]; IRS Form 1099-K FAQ Q21: "For payments made after December 31, 2025, the reporting threshold for Form 1099-MISC or Form 1099-NEC is met if the amount of payments to a recipient total $2,000 or more" [HIGH] |
| **Inflation indexing**: new §6041(h): "In the case of any calendar year after 2026", the $2,000 is increased by the cost-of-living adjustment (base calendar year 2025), "rounded to the nearest multiple of $100". First possible change: **2027** | §70433(b) [HIGH]; i1099mec "may be adjusted for inflation beginning in calendar year 2027" [HIGH] |
| **The 2027 figure** has not been found in any IRS release (as of 2026-10-20) | Search only (UNCERTAIN) |
| The IRS instructions word the start as "For tax years beginning after 2025", while the statute says "payments made after December 31, 2025". Same result for calendar-year payers | i1099mec (Rev. 12/2026) [HIGH]; statute [HIGH]. **Follow the statute** (payment date) |
| Proposed regulations to match (REG-113229-25) exist; not final as far as found | Federal Register public inspection 2026-07519 [LOW, search summary] (UNCERTAIN) |
| Other changes in the same instructions: 1099-NEC box 1 is now **box 1a**; new boxes 1b and 13a for cash tips (P.L. 119-21 §70201) | i1099mec [HIGH] |

### 6.2 1099-K: Sec. 70432 restored $20,000 and 200 transactions

| Point | Source |
|---|---|
| **Sec. 70432** "Repeal of revision to de minimis rules for third party network transactions": §6050W(e) again requires a third party settlement organization to report only if payments "exceed $20,000" **and** "the aggregate number of such transactions exceeds 200". Effective "as if included in section 9674 of the American Rescue Plan Act" (retroactive; the $600 ARPA rule never applied) | P.L. 119-21 §70432(a) [HIGH]; IRS Fact Sheet 2025-08 / *Form 1099-K FAQs: Third party filers* (23 Oct 2025) [HIGH] |
| Backup withholding on third-party network payments follows the same de minimis rule, for calendar years after 31 Dec 2024 (new §3406(b)(8)) | §70432(b) [HIGH] |
| **Payment cards**: "There is not a de minimis exception for reporting payment card transactions. All payment card transactions must be reported on Form 1099-K." | IRS 1099-K FAQ Q7 [HIGH] |
| A state may have a lower 1099-K threshold | IRS 1099-K FAQs [MEDIUM, search summary] |

### 6.3 What the payer must do

| Requirement | Source | What the software must do |
|---|---|---|
| **1099-NEC** if all four hold: paid someone "who is not your employee"; "for services in the course of your trade or business"; to "an individual, partnership, estate, or, in some cases, a corporation"; "payments to the payee of at least $2,000 during the year" | i1099mec (Rev. 12/2026), box 1a [HIGH] | Vendor flag "1099 payee"; expense accounts / items flagged as services |
| **Rents** on 1099-MISC box 1: "$2,000 or more for all types of rents" (not if paid to a real estate agent or property manager) | i1099mec [HIGH] | Box mapping per account |
| **Royalties** on 1099-MISC box 2: "$10 or more" (unchanged) | i1099mec [HIGH] | Separate $10 threshold |
| **Attorneys**: fees of "$2,000 or more" in 1099-NEC box 1a (§6041A(a)(1)); **gross proceeds** paid to an attorney "$600 or more" in 1099-MISC box 10 (§6045(f)); "The exemption from reporting payments made to corporations does not apply to payments for legal services" | i1099mec [HIGH] | Attorney flag overrides the corporation exemption |
| **Corporations generally exempt**: no 1099-MISC for "Generally, payments to a corporation (including a limited liability company (LLC) that is treated as a C or S corporation)", with listed exceptions (legal services, medical payments, fish purchases, etc.). Also exempt: "Payments for merchandise, telegrams, telephone, freight, storage, and similar items" | i1099mec *Exceptions* [HIGH] | Vendor entity type (individual / partnership / LLC-disregarded / C corp / S corp); exclude corporations except attorneys and other listed cases |
| **Card and third-party-network payments are excluded**: they "must be reported on Form 1099-K by the payment settlement entity under section 6050W and are not subject to reporting on Form 1099-MISC or Form 1099-NEC" | i1099mec [HIGH] | Payment mode on each vendor payment; card/PayPal-type payments drop out of the 1099-NEC total |
| **Backup withholding**: payers withhold "at the current rate of 24 percent" when the payee "failed to provide a correct taxpayer identification number (TIN)"; the TIN is given in writing and certified "under penalties of perjury" (Form W-9) | IRS *Backup withholding* [HIGH] | W-9 received (date, TIN, TIN type, entity type); warn and compute 24% when missing |
| A 1099 must be filed for anyone from whom federal income tax was withheld under backup withholding "regardless of the amount of the payment" | i1099mec [HIGH] | Withheld payees are always on the report |
| **Due dates**: 1099-NEC filed with the IRS "on or before January 31" (paper or electronic, §6071(c)); 1099-MISC to the IRS by **Feb 28** on paper or **Mar 31** electronically; payee statements by **Jan 31**; a weekend or holiday moves to the next business day | i1099mec *Filing dates* [HIGH] | Reminder calendar |
| **E-file** is required at **10** or more information returns in aggregate (T.D. 9972, returns filed on or after 1 Jan 2024) | i1099mec [HIGH] | Export file for IRIS / FIRE is future work |

---

## 7. Tax year, currency, records

| Requirement | Source | What the software must do |
|---|---|---|
| "A tax year is an annual accounting period for keeping records and reporting income and expenses." A **calendar year** runs 1 Jan–31 Dec; a **fiscal year** is "12 consecutive months ending on the last day of any month except December 31st"; a **52-53-week** year can be elected. "Unless you have a required tax year, you adopt a tax year by filing your first income tax return using that tax year." Books must follow the chosen year. A calendar year is mandatory if, among other cases, "You keep no books or records". Changing it generally needs **Form 1128** | IRS Pub 538 (Rev. January 2022) [HIGH] | **Default books year: 1 Jan – 31 Dec**; any month-end start allowed. 52-53-week years not supported (open item) |
| Required tax years for partnerships, S corporations and personal service corporations | Pub 538 mentions "required tax year" [HIGH]; the detailed rules were not read (UNCERTAIN) | Not enforced |
| **Currency**: USD, 2 decimals | General knowledge [MEDIUM]; IRS forms are in dollars and cents | `USD`, symbol `$` |
| **IRS record retention**: "Keep records for 3 years" in the general case; 6 years if unreported income is "more than 25% of the gross income shown on your return"; 7 years for a worthless-securities or bad-debt claim; "indefinitely" if no return or a fraudulent return; employment tax records "at least 4 years" | IRS *How long should I keep records?* [HIGH] (the same rules appear in Pub 583, not separately read) | KhataOS never deletes posted vouchers; a 7-year minimum covers the usual cases |
| **California sales tax**: "You should keep required records for at least four years unless we give you specific, written authorization to destroy them sooner"; longer during an audit, appeal or refund claim | CDTFA Pub 116 *Retaining Records* [HIGH] | Same |
| **Texas**: marketplace sales records at least four years | Texas Comptroller *Remote Sellers* [HIGH] | Same |

---

## 8. Design implications for KhataOS (`khata-us`)

1. **No shipped rate database.** With 11,000–13,000 jurisdictions whose rates change every quarter (§1, §4), KhataOS cannot ship or keep rates current. The pack ships only the **state list** (50 + DC, with the five no-state-tax states marked) and the statewide base rates of §4 as editable, effective-dated **examples**. The company enters the jurisdictions it actually collects in.
2. **Jurisdiction components.** A tax jurisdiction is one of `state`, `county`, `city`, `district` (special / transit), each with its own code, name, rate and effective-from/to dates. The FL surtax, NY MCTD, CA districts and TX's 2% cap all fit this model.
3. **Tax classes are combined jurisdiction codes**, e.g. `CA-LA` = {CA state 7.25%, plus the Los Angeles County district components} or `NY-NYC` = {NY 4%, NYC local, MCTD 0.375%}. The class rate is the sum of its components on the document date; tax is **computed and stored per component** (one ledger row per jurisdiction), because returns (CA Schedule A, NY Pub 718 reporting codes) report per jurisdiction. Rounding: per component per line, half away from zero, to the cent (KhataOS choice: no source fixes a national rule, UNCERTAIN per state).
4. **Optional TX cap**: allow a cap on the sum of local components (TX 2%) as a class setting.
5. **Taxability**: item × state taxable / exempt / partial (taxable fraction, for TX data processing 80%). Customer exemptions come from a certificate record (SST F0003, MTC, state forms) with expiry, and the invoice line shows the exempt reason.
6. **Origin vs destination** per state (§3.6): invoices carry ship-from and ship-to; the class is chosen from the ship-to address for destination states and from the selling location for origin states, user-overridable.
7. **Nexus monitor** (report, not enforcement): per ship-to state, rolling sales (gross / taxable / including or excluding marketplace, per the state's rule) and transaction counts over the state's lookback window, against the thresholds in §2 as editable data. A warning at 80% of a threshold.
8. **Marketplace channel**: sales flagged "marketplace-collected" carry no tax and appear as non-taxable on the per-state sales tax summary.
9. **Filing periods** are a per-state company setting (§3.4); the sales tax report runs by jurisdiction for any period, including NY's Mar–Feb quarters.
10. **EIN**: §5.1 validation; the company profile holds the EIN and a list of state registrations (free text).
11. **1099 report from vendor payments by TIN** (calendar year, by payment date): sum payments to each payee TIN by box (NEC 1a, MISC 1 rents, MISC 2 royalties, MISC 10 attorney gross proceeds), excluding card / third-party-network payments and corporate payees (except attorneys), then apply the threshold for that **payment year** ($600 up to 2025, $2,000 for 2026, the indexed figure from 2027 entered as data). Withheld payees always included. Export CSV now; IRS IRIS/FIRE e-file later.
12. **W-9 tracking and backup withholding**: vendor W-9 date, TIN, TIN type, entity type; a 24% backup withholding warning when a payable 1099 vendor has no TIN.

---

## 9. Open items to verify before relying on the pack

1. The 2027 inflation-adjusted §6041 threshold (expected in a late-2026 IRS release).
2. Status of the proposed regulations implementing §70433 (REG-113229-25).
3. NY: whether sales through a *registered* marketplace provider count toward the $500,000/100-sale test (the DTF FAQ says include online-marketplace sales; Sovos says registered-provider sales are excluded).
4. CA: the start date for "engaged in business in every district" (1 Apr vs 25 Apr 2019), and whether facilitated sales count toward a marketplace *seller's* own $500,000 test.
5. Kentucky: confirm the $100,000 test is otherwise unchanged after HB 757.
6. Texas filing-frequency dollar cut-offs; state permit number formats.
7. Whether DC is inside Tax Foundation's "45 states" (treated here as 45 states + DC).
8. That the EIN has no check digit (negative finding).
9. Per-state rounding rules for multi-component tax.
10. Required tax years for partnerships, S corps and PSCs (Pub 538 detail not read).

---

## 10. Source list

- *South Dakota v. Wayfair, Inc.*, 585 U.S. 162 (2018), preliminary print: <https://www.supremecourt.gov/opinions/17pdf/585us1r58_pok0.pdf>; slip opinion: <https://www.supremecourt.gov/opinions/17pdf/17-494_j4el.pdf>
- Public Law 119-21 (H.R. 1), §§70432, 70433: <https://www.govinfo.gov/content/pkg/PLAW-119publ21/html/PLAW-119publ21.htm>; <https://www.congress.gov/119/plaws/publ21/PLAW-119publ21.htm>
- IRS Instructions for Forms 1099-MISC and 1099-NEC (Rev. 12/2026, page reviewed 01 Jul 2026): <https://www.irs.gov/instructions/i1099mec>
- IRS *Form 1099-K FAQs: Third party filers of Form 1099-K* (Fact Sheet 2025-08): <https://www.irs.gov/newsroom/form-1099-k-faqs-third-party-filers-of-form-1099-k>; <https://www.irs.gov/pub/taxpros/fs-2025-08.pdf>; news release: <https://www.irs.gov/newsroom/irs-issues-faqs-on-form-1099-k-threshold-under-the-one-big-beautiful-bill-dollar-limit-reverts-to-20000>
- IRS *Backup withholding*: <https://www.irs.gov/businesses/small-businesses-self-employed/backup-withholding>
- IRS *How EINs are assigned and valid EIN prefixes*: <https://www.irs.gov/businesses/small-businesses-self-employed/how-eins-are-assigned-and-valid-ein-prefixes>
- IRM 21.7.13 *Assigning Employer Identification Numbers*: <https://www.irs.gov/irm/part21/irm_21-007-013r>; *About Form SS-4*: <https://www.irs.gov/forms-pubs/about-form-ss-4>
- IRS Pub 538 *Accounting Periods and Methods*: <https://www.irs.gov/publications/p538>
- IRS *How long should I keep records?*: <https://www.irs.gov/businesses/small-businesses-self-employed/how-long-should-i-keep-records>
- CDTFA: Wayfair guide <https://www.cdtfa.ca.gov/industry/wayfair.htm>; Pub 51 rates and frequencies <https://cdtfa.ca.gov/formspubs/pub51/tax-and-fee-rates-and-filing-frequencies.htm>; rates <https://cdtfa.ca.gov/taxes-and-fees/sales-use-tax-rates.htm>; rate FAQ <https://cdtfa.ca.gov/taxes-and-fees/tax-rate-faq.htm>; tax practitioners <https://cdtfa.ca.gov/industry/tax-practitioners/filing-and-payments.htm>; filing dates <https://cdtfa.ca.gov/taxes-and-fees/sales-use-tax-returns-filing-dates.htm>; Pub 116 <https://cdtfa.ca.gov/formspubs/pub116/retaining-records.htm>; district tax guide <https://cdtfa.ca.gov/industry/local-and-district-retailer-taxes/district-tax.htm>; Marketplace Facilitator Act <https://cdtfa.ca.gov/industry/MPFAct.htm>
- Texas Comptroller: remote sellers <https://comptroller.texas.gov/taxes/sales/remote-sellers.php>; sales and use tax <https://comptroller.texas.gov/taxes/sales/>; pub. 94-105 <https://comptroller.texas.gov/taxes/publications/94-105.php>; pub. 94-127 <https://comptroller.texas.gov/taxes/publications/94-127.php>
- NY DTF: nexus <https://www.tax.ny.gov/pubs_and_bulls/publications/sales/nexus.htm>; marketplace providers <https://www.tax.ny.gov/pubs_and_bulls/publications/sales/marketplace.htm>; rates <https://www.tax.ny.gov/pubs_and_bulls/tg_bulletins/st/sales_tax_rates_additional_sales_taxes_and_fees.htm>; filing requirements <https://www.tax.ny.gov/pubs_and_bulls/tg_bulletins/st/filing_requirements_for_sales_and_use_tax_returns.htm>; computer software <https://www.tax.ny.gov/pubs_and_bulls/tg_bulletins/st/computer_software.htm>; rate publications <https://www.tax.ny.gov/pubs_and_bulls/tg_bulletins/st/sales_tax_rate_publications.htm>
- Florida DOR: TIP 21A01-03 <https://floridarevenue.com/taxes/tips/Documents/TIP_21A01-03.pdf>; registration <https://floridarevenue.com/taxes/eservices/Pages/registration.aspx>; sales and use tax <https://floridarevenue.com/taxes/taxesfees/Pages/sales_tax.aspx>
- Washington DOR: remote sellers <https://dor.wa.gov/taxes-rates/retail-sales-tax/marketplace-fairness-leveling-playing-field/remote-sellers>; 2019 release <https://dor.wa.gov/about/news-releases/2019/new-law-updates-washington-state-tax-requirements-out-state-businesses>; rates <https://dor.wa.gov/taxes-rates/sales-use-tax-rates>; Q1 2026 flyer <https://dor.wa.gov/sites/default/files/2025-11/Q126_Alpha_LSU_flyer.pdf>
- Illinois DOR Bulletin FY 2026-12: <https://tax.illinois.gov/research/publications/bulletins/fy-2026-12.html>
- Kentucky DOR SSUTA recertification (29 Jul 2026): <https://revenue.ky.gov/Business/Sales-Use-Tax/Documents/KY%20SSUTA%20Recertification%20-%207-29-26.pdf>
- Streamlined Sales Tax: about <https://www.streamlinedsalestax.org/about-us/about-sstgb>; state detail <https://www.streamlinedsalestax.org/Shared-Pages/State-Detail>; exemptions and F0003 <https://www.streamlinedsalestax.org/Shared-Pages/exemptions->
- Multistate Tax Commission resale certificate: <https://www.mtc.gov/resources/uniform-sales-use-tax-exemption-certificate/>; FAQ <https://www.mtc.gov/resources/faq-uniform-sales-and-use-tax-certificate/>
- Secondary: Tax Foundation 2026 rates <https://taxfoundation.org/data/all/state/2026-sales-tax-rates/>; Tax Foundation 2020 jurisdictions <https://taxfoundation.org/data/all/state/state-sales-tax-jurisdictions-in-the-us-2020/>; Avalara jurisdictions <https://www.avalara.com/us/en/blog/a-handy-guide-to-us-sales-tax-jurisdictions.html>; Avalara transaction thresholds <https://www.avalara.com/us/en/blog/states-eliminating-economic-nexus-transaction-thresholds.html>; Sovos NY nexus <https://sovos.com/blog/sut/how-has-the-new-york-economic-nexus-changed/>

---

## 11. Addendum (2026-10-21): use tax and exemption certificate records

Research for `khata_us::use_tax` and `khata_us::exemptions`. Same confidence tags as above.

### 11.1 Use tax on purchases

| Finding | Source | Used as |
|---|---|---|
| A business that buys items for use, storage or consumption in California from an out-of-state seller that does not collect California tax may owe use tax, paid directly to the CDTFA; **"the use tax rate is the same as your local sales tax rate"**; the guide covers physical merchandise (supplies, furniture, fixtures, equipment) | CDTFA Publication 123 (July 2024) (search summary) [MEDIUM] | Use tax at the company's own jurisdiction rates (state + local keys), on non-inventory lines |
| Use tax applies even where title passed in the state; state agencies pursue buyers when out-of-state vendors do not collect | CDTFA annotation 325.0167; practitioner blog (Klasing) [MEDIUM/LOW] | Warning text only |
| Every state with a sales tax has a complementary use tax; purchases for resale are excluded (taxed when sold) | §1 and §3.2 above; general rule [MEDIUM] | Stock-item lines = for resale; `RESALE…`/`EXEMPT…` classes exempt |
| Where self-assessed use tax goes on each state's return (line), credits for tax paid to another state, and taxability of services for use tax | Not found per state [UNCERTAIN] | Not modelled: the report is the liability by component; the user books and files it |

### 11.2 Exemption and resale certificate records

| Finding | Source | Used as |
|---|---|---|
| SST blanket certificates: a seller with a blanket certificate is relieved when it has a **recurring business relationship** with the buyer (no more than 12 months between sales); states may not ask the seller to renew blanket certificates while the relationship recurs | Iowa Code §423.51 (2015, 2020); W. Va. Code §11-15B-24; R.I. Gen. Laws §44-18.1-18 (SSUTA implementations) [MEDIUM] | No built-in expiry rule; "no expiry date" allowed |
| A seller that obtains a fully completed certificate at the time of sale, **within 90 days after the sale**, or before an audit starts (SST states) is relieved of liability | Ryan (2011) summary of SST guidelines; NCDOR notice on Form E-595E (2018) [MEDIUM/LOW] | Documented; `issued` date recorded; certificates given after the sale do not reach back automatically (enter an earlier "given on" date if the state allows) |
| Michigan's form: blanket certificate for recurring relationships, otherwise an expiry date of up to 4 years | Michigan Form 3372 instructions (copy) [MEDIUM] | Expiry date per record |
| Florida's Annual Resale Certificate runs to 31 December of the year; other secondary sources disagree (one year from issue; five years) | Secondary sources only (vendor blogs) [UNCERTAIN] | Not encoded: the user enters the state's expiry date |
| What to record: certificate type, state(s), purchaser ID, reason, date received, expiry; keep at least 4 years (CA) | §3.2 above [MEDIUM] | Record fields: state, number, kind (resale / exempt organization / government / other), issued, expires, note |

Sources for this addendum:
- CDTFA Publication 123: <https://www.cdtfa.ca.gov/formspubs/pub123.pdf>; annotation 325.0167 <https://cdtfa.ca.gov/lawguides/annotations/325-0167.pdf>; Klasing (secondary) <https://klasing-associates.com/use-sales-tax-enforcement/>
- SSUTA implementations: Iowa <https://www.legis.iowa.gov/docs/code/2020/423.51.pdf>; West Virginia <https://code.wvlegislature.gov/11-15B-24/>; Rhode Island <https://webserver.rilegislature.gov/statutes/TITLE44/44-18.1/44-18.1-18.htm>
- 90-day rule: Ryan <https://ryan.com/about-ryan/news-and-insights/2011/streamlined-sales-tax-provides-guidelines-for-acceptance-of-exemption-certificates/>; NCDOR <https://ncdor.gov/documents/files/important-notice-updated-form-e-595e-certificate-exemption/open>
- Michigan Form 3372 (copy): <https://www.smithcorona.com/assets/tax-forms/michigan-sales-and-use-tax-certificate-sc.pdf>; South Dakota exemption certificate <https://dor.sd.gov/media/q0xpvden/exemption-certificate.pdf>
- Florida annual resale certificate (secondary, conflicting): <https://trykintsugi.com/blogs/resale-certificate-florida-quick-step-process>

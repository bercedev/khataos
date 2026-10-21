# Accountant's desk: notes, to-dos and a compliance calendar

Research (October 2026) and the design of the desk add-on. The desk is
reached with **T** on the home menu (Gateway) or **Accountant's Desk** in
Go To (Alt+G). Code:

- `crates/khata-compliance/src/calendar.rs`: the shared types and date
  arithmetic.
- `crates/khata-{in,ae,sa,sg,gb,de,fr,nl,es,us,mx}/src/calendar.rs`: each
  country's rules, with a citation per rule.
- `apps/desktop/src-tauri/src/desk.rs`: storage, rights and reminders.
- `apps/desktop/src/desk-api.ts` and `apps/desktop/src/screens/desk.ts`:
  the screen.

## 1. What accountants rely on today

Sources are vendor help pages and reviews. Items marked *(summary)* were
read through search results only.

| Product | What it offers | Source |
|---|---|---|
| Zoho Books | Tasks add-on: assign to users, priority, status, email or in-app reminders, linked to a transaction and shown on it. Off by default; only on some plans. | https://zoho.com/books/help/tasks |
| Zoho Books | Comments and History tab on every transaction. Internal by default; the API has a `comment_type: internal` field. | https://www.zoho.com/books/api/v3/bills/comments/ |
| Zoho Practice | For CA firms: client due dates, filing alerts, task assignment, client portal *(summary)*. | https://www.zoho.com/practice/ |
| QuickBooks Desktop | **Reminders** list: invoices due, bills to pay, memorised transactions, cheques to print. A separate To Do list. | https://quickbooks.intuit.com/learn-support/en-us/other-questions/new-to-qbo-where-are-the-reminders-i-had-in-desktop/00/1435000 |
| QuickBooks Online Accountant | "Books review": a month-end checklist (Setup, Transaction review, Account reconciliation, Final review, Wrap-up), with custom tasks per tab. | https://quickbooks.intuit.com/learn-support/en-us/help-article/manage-client/month-end-reviews-finish-clients-open-tasks/L4GubaqKy_US_en_US |
| Busy (India) | Post-dated cheques: Type = PDC on payment and receipt vouchers. They leave the books alone until the "regularize" date. | https://busy.in/faqs/features/accounts/23/ |
| TallyPrime | Post-dated vouchers and the Post-Dated Summary (Banking). Narration is the only note on a voucher. Receivable reminders by email or WhatsApp. **No alert when a PDC falls due, and no statutory calendar.** | https://help.tallysolutions.com/tally-prime/banking-utilities/post-dated-cheque-managment-tally |
| Tally add-ons (TDL) | Mostly overdue-payment reminders by WhatsApp, SMS or email. None found with a statutory calendar. | https://precisiontech.in/apps/tally/tally-tdl-addons/ |
| Indian CA practice tools (Suvit, Vyapar TaxOne, AI Accountant) | Compliance calendars per client by business type and frequency (GSTR-1, 3B, 9, TDS, ITR). Reminders 7, 3 and 1 day before. Tasks by client and period. Main complaint: "most due date trackers are rigid". | https://aiaccountant.com/blog/task-management-software-chartered-accountants , https://www.aiaccountant.com/blog/gst-return-calendar-with-reminders |
| Karbon (international) | Month-end close templates, tasks by role. Users want a calendar view and exportable to-do lists. | https://karbonhq.com/templates/month-end-close-auto-send-core/ , https://capterra.com/p/156181/Karbon/reviews/ |
| Marg ERP, CompuOffice, Saral | Nothing usable found online. Unverified. | none |

**Table stakes**

1. Internal notes on a voucher or party.
2. To-dos with a due date, linked to a record.
3. One reminders view: bills due, overdue receivables and payables, PDCs.
4. A statutory calendar driven by the company's filing frequency.
5. A month-end checklist.

**What users complain about**

- Reminders that newer versions hide (QuickBooks Online).
- Trackers they can't change.
- Lists they can't export.
- Heavy setup.

The desk answers with:

- a calendar view;
- free-form to-dos for anything the rules miss, including month-end
  checklist items;
- "filed" marks;
- reminders read straight from the books, with no setup.

## 2. Statutory due dates

All dates are **estimates**. The screen says: "Due dates are estimates
worked out from the published rules. Governments extend deadlines: check
the official portal before you file." No date is moved for weekends or
holidays.

### India (`khata-in/src/calendar.rs`)

| Return | When | Due | Source | Confidence |
|---|---|---|---|---|
| GSTR-1 | monthly | 11th of the next month | https://tutorial.gst.gov.in/userguide/returns/GSTR_1.htm | High |
| GSTR-3B and payment | monthly | 20th of the next month | https://tutorial.gst.gov.in/userguide/returns/GSTR3B.htm | High |
| GSTR-1 (QRMP) | quarterly | 13th of the month after the quarter | GSTR_1 page above; QRMP advisory https://tutorial.gst.gov.in/offlineutilities/returns/QRMP_Advisory.pdf | High |
| IFF (QRMP, optional) | months 1 and 2 of the quarter | 13th of the next month | https://tutorial.gst.gov.in/userguide/returns/FAQs_IFF.htm | High |
| PMT-06 (QRMP) | months 1 and 2 of the quarter | 25th of the next month | QRMP advisory Q13, Q17 | High |
| GSTR-3B (QRMP) | quarterly | **22nd** (Category X) or **24th** (Category Y), by the GSTIN's state | QRMP advisory Q28; CGST Rule 61 | High (grouping from 2020–21; no later change found) |
| GSTR-9 (optional) | yearly | 31 December after the financial year; not needed up to ₹2 crore turnover | CGST s.44, Rule 80 (https://cbic-gst.gov.in/gst-acts.html) | Medium-high; often extended |
| TDS payment | monthly | 7th of the next month; **30 April** for March | Rule 218 of the Income-tax Rules 2026, with the same timelines as the old Rule 30, per the CBDT transition FAQ: https://www.incometaxindia.gov.in/documents/81799/11848482/FAQs-on-Interplay-and-Transition.pdf | High |
| TDS statements | quarterly | 31 Jul, 31 Oct, 31 Jan, 31 May (Forms 138/140 from 2026-27; 24Q/26Q before) | CBDT FAQ above; Rule 219(4) | Medium (secondary sources for the new rules) |
| TCS payment | monthly | 7th of the next month, March included | old Rule 37CA | Medium |
| TCS statement | quarterly | 31st of Jul, Oct, Jan, May from 2026-27 (Form 143); the 15th of those months before (27EQ) | Rule 219(4), secondary | **Uncertain** |
| Advance tax (optional) | 4 instalments | 15 Jun, 15 Sep, 15 Dec, 15 Mar | Income-tax Act 2025 s.406: https://www.incometaxindia.gov.in/w/section-406-5 | Medium-high |

**India: annual and company-law dates** (added with the compliance
profile). The previous year is always April to March. Income tax and GST
count a first year from incorporation to the next 31 March. Company law
lets a company formed in January to March run its first year to the
following March (Companies Act s.2(41)); that moves the first AGM and the
ROC filings.

| Filing | Who | Due | Source | Confidence |
|---|---|---|---|---|
| Income-tax return | companies, and anyone under tax audit | 31 October after the year | old s.139(1) Expl. 2; https://www.incometax.gov.in/iec/foportal/help/individual/return-applicable-1 | Medium; the 2025 Act's section number not verified |
| Income-tax return | others (no audit) | 31 July after the year | same | Medium |
| Tax audit report (3CA/3CB with 3CD) | "audit applies" in the profile | 30 September after the year | old s.44AB; reported as Form 26 under the 2025 Act | Medium |
| Annual general meeting | companies | within 6 months of the year end (30 Sep); the first within 9 months of the first year end | Companies Act s.96, https://www.mca.gov.in/content/mca/global/en/acts-rules/ebooks/acts.html | High |
| AOC-4 | companies | 30 days after the AGM; dated from the AGM's last allowed day (30 Oct) | s.137 | High (the date assumes the AGM is held on its last day) |
| MGT-7 | companies | 60 days after the AGM (29 Nov) | s.92 | High (same assumption) |
| DIR-3 KYC | directors of companies, designated partners of LLPs | 30 September each year | Rule 12A, Appointment and Qualification of Directors Rules | Medium-high |
| DPT-3 (optional) | companies with deposits or loans | 30 June, as at 31 March | Acceptance of Deposits Rules r.16 | Medium-high |
| MSME-1 (optional) | companies with MSE dues over 45 days | 31 Oct (Apr–Sep), 30 Apr (Oct–Mar) | MSME-1 order under s.405 | Medium-high |
| LLP Form 11 | LLPs | 30 May (60 days after the year end) | LLP Rules r.25, https://www.mca.gov.in/content/mca/global/en/acts-rules/ebooks/rules.html | High |
| LLP Form 8 | LLPs | 30 October | LLP Rules r.24 | High |
| GSTR-9C (optional) | turnover above ₹5 crore | 31 December, with GSTR-9 | CGST Rule 80(3) | Medium-high |

Partnerships and sole proprietors get only the income-tax return, the tax
audit (when set) and advance tax. All Indian dates are marked "may move":
the GST Council, CBDT and MCA extend them only by notification, and a
weekend doesn't move them.

**QRMP states**

- **22nd (Category X):** GSTIN state codes 22–37: Chhattisgarh, Madhya
  Pradesh, Gujarat, Daman and Diu, Dadra and Nagar Haveli and Daman and
  Diu, Maharashtra, old Andhra Pradesh (28), Karnataka, Goa, Lakshadweep,
  Kerala, Tamil Nadu, Puducherry, Andaman and Nicobar, Telangana and Andhra
  Pradesh.
- **24th (Category Y):** codes 01–21 and Ladakh (38).
- **Unknown state:** the earlier date, the 22nd.

**When TDS and TCS dates show**

The TDS and TCS dates show when a ledger has TDS (or TCS) details. The
calendar settings can switch them on or off.

**MSME rule (MSMED Act s.15)**

- Pay micro and small suppliers by the agreed date, at most **45 days**
  from acceptance (15 days with no written agreement).
- An amount unpaid beyond that is deductible only when paid. This was
  s.43B(h) of the 1961 Act and is s.37(2)(g) of the 2025 Act (secondary
  source).
- The desk is a reminder, not a calendar rule: supplier bills past, or
  within a week of, bill date + 45 days.
- Parties marked MSME (Schedule III mapping) are flagged. The others still
  show, as in the MSME report, because KhataOS can't tell a supplier's
  category.

### UAE (`khata-ae/src/calendar.rs`)

| Return | Due | Source | Confidence |
|---|---|---|---|
| VAT 201 and payment | **28 days** after the end of the tax period. Quarterly by default; the stagger the FTA assigned is set in the settings. | https://tax.gov.ae/en/taxes/vat/vat.topics/filing.vat.returns.and.making.payments.aspx | High. The FTA shows "28-10-2026" for the September quarter. |
| Corporate tax return and payment | 9 months after the financial year ends | https://tax.gov.ae/en/media.centre/news/federal.tax.authority.urges.submission.of.corporate.tax.returns.and.settlement.of.corporate.tax.liabilities.within.nine.months.from.the.end.of.the.tax.period.aspx | High |

Whether a deadline that falls on a weekend moves to the next working day is
reported only by secondary sources, so UAE dates are marked "may move".
**Uncertain.**

**Profile rules**

- Quarters may start in any month. The profile asks for "the month a
  quarter starts", and the VAT periods follow it from the registration
  date.
- Corporate tax shows when the business is registered for it. It defaults
  to yes for companies.
- The first tax period runs from incorporation (6 to 18 months), for
  periods starting on or after 1 June 2023.

**ESR and UBO**

- Economic Substance Regulations reports are not required for financial
  years starting on or after 1 January 2023 (Cabinet Resolution 98 of
  2024, secondary source).
- The UBO register is kept and updated within 15 days of a change, with no
  periodic filing.
- So neither is a calendar date.

### Saudi Arabia (`khata-sa/src/calendar.rs`)

| Return | Due | Source | Confidence |
|---|---|---|---|
| VAT return and payment | **Last day of the month after the period**, not 28 days. Quarterly (calendar quarters) unless supplies exceed SAR 40m, which makes it monthly. | VAT Implementing Regulations Art. 59 and 62; https://zatca.gov.sa/en/HelpCenter/guidelines/Documents/Agents%20Guideline.pdf | High |
| Zakat or income tax return | 120 days after the year end | https://zatca.gov.sa/en/HelpCenter/FAQs/Pages/FAQ_035.aspx | High |
| Simplified (B2C) e-invoices | Reported **within 24 hours** of issue; standard invoices are cleared in real time | https://zatca.gov.sa/en/E-Invoicing/Introduction/Guidelines/Documents/Simplified%20guide_En.pdf | High |

The brief said "28 days" for KSA; that is the UAE rule only.

The 24-hour rule is a **reminder**, not a calendar date. The desk reads the
ZATCA queue (`zatcaQueueApi.queue()`) and shows:

- documents waiting (today);
- simplified invoices past the 24-hour limit (overdue).

A non-working day moves the VAT and zakat deadlines to the next working day
(Art. 74, secondary source). The desk applies this for the Friday–Saturday
weekend. Public holidays are not known to KhataOS.

**Zakat first year**

The first year runs from incorporation (company years shorter than 6
months run on to the next year end).

**CR annual confirmation (optional, uncertain)**

- The Commercial Register Law in force from April 2025 requires the
  registration's data to be confirmed every year.
- The desk dates it on each anniversary of incorporation and marks it
  "may move".
- Source: Ministry of Commerce, https://mc.gov.sa/en/Regulations/Pages/default.aspx.
- I found no official day count. **Uncertain.**

### United Kingdom (`khata-gb/src/calendar.rs`)

| Return | Due | Source | Confidence |
|---|---|---|---|
| VAT Return (MTD) and payment | "One calendar month and 7 days after the end of the VAT period". Quarters end March, June, September and December by default; the stagger is set in the settings. | https://www.gov.uk/submit-vat-return | High |

**How "one month and 7 days" is counted**

The date is the **end of the next month plus 7 days**: 30 June → 7 August,
not 6 August. This matches HMRC's own obligations (the sandbox's
`2017-06-30` → `2017-08-07`).

**What is not covered**

- HMRC does not move a deadline that falls on a weekend.
- The Annual Accounting Scheme (one return, due 2 months after the year)
  is not covered.

The MTD obligations from HMRC (Making Tax Digital → VAT periods) are the
authority. The desk doesn't call HMRC: it works offline and needs no
sign-in to HMRC.

**VAT stagger in the profile:** 1 (quarters to March), 2 (to April), 3 (to
May), or M for monthly.

**Company and LLP filings** (all `Shift::Fixed`: neither HMRC nor Companies
House moves a weekend deadline):

| Filing | Who | Due | Source |
|---|---|---|---|
| Accounts to Companies House | companies, LLPs | 9 months after the accounting reference date (s.442) | https://www.gov.uk/file-your-company-accounts-and-tax-return |
| First accounts (period over 12 months) | same | 21 months after incorporation, or 3 months after the period if later (s.442(3)) | same |
| Confirmation statement | same | 14 days after each anniversary of incorporation | https://www.gov.uk/file-your-confirmation-statement-with-companies-house |
| Corporation tax payment | companies | 9 months and 1 day after the accounting period | https://www.gov.uk/pay-corporation-tax |
| CT600 | companies | 12 months after the accounting period | https://www.gov.uk/company-tax-returns |
| Self Assessment return and payment | sole traders, partnerships | 31 January after the tax year to 5 April; second payment on account 31 July | https://www.gov.uk/self-assessment-tax-returns/deadlines |

**How the first year and month ends are counted**

- **First year:** the accounting reference date is the end of the month of
  the first anniversary. A first year end less than 6 months after
  incorporation is skipped.
- **Month ends:** they stay month ends. A 30 June year end gives 31 March
  (accounts) and 1 April (tax).
- **Confidence:** HMRC's "9 months and 1 day" for a 30 June year end is
  read as 1 April. **Check.**

### Singapore (`khata-sg/src/calendar.rs`)

| Return | Due | Source | Confidence |
|---|---|---|---|
| GST F5 and payment | One month after the end of the accounting period (30 Apr, 31 Jul, 31 Oct, 31 Jan for calendar quarters). Quarterly by default; monthly is a setting. | https://www.iras.gov.sg/taxes/goods-services-tax-(gst)/filing-gst/gst-filing-due-dates-and-requests-for-extension | High |

IRAS generally grants no extensions. GIRO deducts the tax on the 15th of the
month after the due date. IRAS doesn't say whether a weekend moves a date,
so the dates are marked "may move".

| Filing | Due | Source | Confidence |
|---|---|---|---|
| ECI (optional) | 3 months after the year end; waived at revenue up to S$5m with nil ECI | https://www.iras.gov.sg/taxes/corporate-income-tax/basics-of-corporate-income-tax/estimated-chargeable-income-(eci)-filing | High |
| Form C-S/C | 30 November of the year of assessment (the year after the year end) | https://www.iras.gov.sg/taxes/corporate-income-tax/filing-and-paying-corporate-income-tax | High |
| AGM (optional) | within 6 months of the year end (s.175), unless dispensed with | https://www.acra.gov.sg/compliance/annual-filing-requirements | High |
| ACRA annual return | within 7 months of the year end (s.197) | same | High |

The first financial year runs from incorporation. A year end less than 6
months after incorporation is skipped; ACRA allows up to 18 months.

### Germany (`khata-de/src/calendar.rs`)

| Return | Due | Source | Weekend |
|---|---|---|---|
| UStVA and payment | 10th day after each month or quarter (UStG §18(1)). Monthly by default; quarterly is a setting. The permanent extension (+1 month) is not modelled. | https://www.gesetze-im-internet.de/ustg_1980/__18.html | next working day (AO §108(3)) |
| Annual VAT return | 31 July of the following year (AO §149(2)); later with a tax adviser | https://www.gesetze-im-internet.de/ao_1977/__149.html | next working day |

### France (`khata-fr/src/calendar.rs`)

| Return | Due | Source | Weekend |
|---|---|---|---|
| CA3 (monthly or quarterly) | between the 15th and 24th of the next month, on a day set for each company; **shown as the 15th** | https://www.impots.gouv.fr/professionnel/la-tva | may move |
| CA12 (annual, simplified regime) | second working day after 1 May for a calendar year; 3 months after other year ends | same | the working-day count is applied |
| CA12 instalments | July (55%) and December (40%); shown on the 15th | same | may move |

### Netherlands (`khata-nl/src/calendar.rs`)

| Return | Due | Source | Weekend |
|---|---|---|---|
| Aangifte omzetbelasting and payment | last day of the month after the period (quarterly by default) | https://www.belastingdienst.nl/wps/wcm/connect/nl/btw/content/wanneer-btw-aangifte-doen | next working day (Algemene termijnenwet) |

### Spain (`khata-es/src/calendar.rs`)

| Return | Due | Source | Weekend |
|---|---|---|---|
| Modelo 303 | quarterly: 20 April, 20 July, 20 October, 30 January; monthly: 30th of the next month | https://sede.agenciatributaria.gob.es/Sede/calendario-contribuyente.html | next working day (Ley 39/2015 art. 30) |
| Modelo 390 (optional) | 30 January of the following year; not for SII filers | same | next working day |

### United States (`khata-us/src/calendar.rs`)

| Return | Who | Due | Source | Weekend |
|---|---|---|---|---|
| State sales tax return | each state in the profile, at its frequency (M, Q, A). The home state (`tax_region`) quarterly if none is set. | **usually the 20th** of the month after the period; each state sets its own day | IRS Pub 509 (general calendar) and each state's revenue department | may move |
| Form 1099-NEC (optional) | anyone who paid contractors | 31 January | https://www.irs.gov/forms-pubs/about-form-1099-nec | next business day (IRC §7503) |
| Form 1120 | C corporations | 15th day of the 4th month after the year end; the 3rd month for a June year end in years beginning before 2026 | https://www.irs.gov/publications/p509 | next business day |
| Form 1120-S / 1065 | S corporations / partnerships and LLPs | 15th day of the 3rd month after the year end | same | next business day |
| Form 1040 with Schedule C | sole proprietors | 15 April | same | next business day |

**Notes**

- A short first year from incorporation is normal and is dated that way.
- Estimated tax payments are not modelled.
- Federal holidays are not counted.

### Mexico (`khata-mx/src/calendar.rs`)

| Return | Due | Source | Weekend |
|---|---|---|---|
| Monthly ISR and IVA payments | 17th of the next month (LISR art. 14, LIVA art. 5-D) | https://www.diputados.gob.mx/LeyesBiblio/pdf/LIVA.pdf | next business day (CFF art. 12) |
| DIOT | last day of the next month (LIVA art. 32 VIII) | same | next business day |
| Annual return | personas morales 31 March (LISR art. 76 V); personas físicas 30 April (art. 150) | https://www.diputados.gob.mx/LeyesBiblio/pdf/LISR.pdf | next business day |

## 2a. Weekends and holidays

Every rule carries a `Shift`. The screen keeps "Estimate" on every date and
adds what a weekend does.

| Country | Rule | Shift |
|---|---|---|
| India | GST, income tax and MCA dates move only by notification | may move |
| UAE | unverified | may move |
| KSA | VAT and zakat move to the next working day (Fri–Sat weekend) | next working day |
| UK | HMRC and Companies House keep the date | fixed |
| Singapore | not stated by IRAS | may move |
| Germany, Netherlands, Spain, Mexico | moved to the next working day by law | next working day (Sat–Sun) |
| France | CA3 day set per company | may move |
| US | federal dates move to the next business day (IRC §7503); states vary | next business day / may move |

Public holidays are never counted. A moved date shows "moved from <date>,
a weekend".

## 2b. The compliance profile

F12 on the desk opens the profile. It is stored with the calendar settings
in `desk.calendar`, in the company file and not in the books.
`khata-core` is unchanged.

The pack declares which fields it uses (`CalendarDefaults::fields`), and the
dialog asks only those.

| Field | Countries |
|---|---|
| Incorporation date | all |
| Business form | IN (company, LLP, partnership, sole proprietor); AE; GB; US (C corp, S corp, LLP, partnership, sole proprietor); MX (moral, física) |
| Financial year end month (default: the books' year) | AE, SA, SG, GB, FR, US |
| VAT/GST registration date, or N for not registered | all but US and MX |
| Tax period | IN (M or Q for QRMP); GB (stagger 1, 2, 3 or M); AE (M, or the month a quarter starts); SG (M, or the month a quarter ends); SA (M above SAR 40m, or Q); DE, NL, ES (M or Q); FR (M, Q or A) |
| Corporate tax registered | AE |
| Tax audit applies | IN |
| TDS / TCS | IN |
| Sales tax states with frequency | US (checked against `khata_us::states`) |

**Profile prompt**

Until the profile is saved, the desk's first row is "Set up your compliance
profile", and Enter opens it.

**Filed marks**

A filed mark is kept per rule code, plus the US state when there is one:
`US.SALESTAX.CA`.

## 2c. Sorting and filtering

**Sections**

- Overdue.
- Today.
- Next 7 days.
- Next 90 days: returns, filings and to-dos only, so annual filings show
  in time.
- To-dos and notes.

**Grouping**

Inside each section, rows are grouped by category, in this order:

1. Tax returns.
2. Payments.
3. Company filings.
4. Audit.
5. Reminders from the books.
6. To-dos.

Each group is sorted by due date.

**Filter**

**Ctrl+F** (TallyPrime's report Filter key) moves through one category at a
time, then back to everything. It also filters the calendar's day list and
day dots.

## 3. Design

### Storage: the company file's settings table, not the audit trail

A note is not an accounting entry. The audit trail can't hold one without a
format change:

- `Book::apply_entry` accepts only the entity kinds it knows (group,
  ledger, voucher, unit, stockgroup, stockitem, godown, profile).
- Any other kind is `UnreplayableEntry`.
- A `note:` kind would therefore break replay on every older copy of the
  app and on the sync server.

Putting notes in the profile was also rejected. Every edit would rewrite
the whole profile into the hash-chained trail, and the trail is meant for
the books.

So notes are kept like `vat_gb.schemes`, as the company's **shared
records** (the company file's `records` table, beside the trail;
`docs/sync-settings.md`):

| Row | What it holds |
|---|---|
| `desk.note.<uuid>` | One note per row. JSON: title, text, optional due date, done, optional link to a ledger, voucher or stock item, who and when. |
| `desk.filed.<code>.<period end>` | A return marked filed, by whom and when. |
| `desk.calendar` | The compliance profile and filing pattern: incorporation date, business form, year-end month, VAT registration date (or not registered), tax period and stagger, corporate tax, audit, TDS and TCS, US states. |

**Synced like the books**

- The rows reach co-workers and the company's accountant on every path the
  books take: the office server, a self-hosted server, the relay (sealed
  with the company key) and `.khatapkg` files.
- They sync on a second channel beside the trail, chosen over an
  "annotation" entity in the trail so that older apps and servers keep
  syncing the books (`docs/sync-settings.md`).
- Each row merges last writer wins. If two people edit one note offline,
  the later edit wins.
- Deleting a note is final: an edit made offline elsewhere never brings it
  back.
- They survive a rebase, which takes the records along to the new file.

### Rights

These checks are in `desk.rs`. Users and roles come from `khata_core::UserRole`.

| Who | Can |
|---|---|
| Auditor (view only), and anyone when the company has no users | Read notes, the calendar and the reminders. |
| Viewer shares (LAN or relay "viewer") | Read only. `lan::check_writable` refuses writes, and the server refuses a viewer token's record pushes. |
| Data Entry | Add notes, change any note, tick done, mark returns filed, delete **own** notes. |
| Accountant, Owner | Also delete anyone's note and change the calendar settings. |

### Reminders from the books (read-only, never stored)

| Reminder | What it shows |
|---|---|
| Bills | Open bill-wise bills with a due date: overdue, or due in range. Receivable or payable. |
| Post-dated vouchers | Active vouchers dated after the current date (F2), such as post-dated cheques. |
| MSME (India) | Creditor bills past, or within 7 days of, bill date + 45 days. |
| ZATCA (KSA) | The queue's waiting and overdue documents. |

The home screen shows **Due soon**. It counts:

- open to-dos and unfiled returns due in the next 7 days;
- bills falling due in the next 7 days;
- post-dated vouchers in the next 7 days;
- the overdue to-dos and returns, as a separate number.

Overdue bills are not counted there; the desk lists them.

### Keys

| Key | Where | Does |
|---|---|---|
| **T** | Home (Gateway) | Opens the desk ("To-dos and Due Dates"). TallyPrime's Gateway doesn't use T (letters C A H V K N B P S R D O Q). |
| Go To (Alt+G) | anywhere | "Accountant's Desk" (also found by desk, to-do, notes, reminders, calendar, due dates). |
| **Alt+W** | report rows, saved vouchers | Adds a note or to-do on the record under the cursor. Alt+W has no TallyPrime meaning (`docs/research/tally-shortcuts.md`). Bound in the `Report` and `Voucher` contexts only. |
| Alt+C / Ctrl+Enter / Alt+D | desk | New to-do (dated to the selected day in the calendar) / alter / delete. These are the same keys as the Users and Sharing lists. |
| Space | desk | Ticks a to-do done, or marks a return filed. |
| Enter | desk | Opens the row: the party's ledger statement, the voucher, the matching return report for its period, or the ZATCA queue. |
| Tab | desk | Moves between the lists and the calendar. |
| Arrows / PgUp, PgDn / Home | calendar | Day and week (mirrored in Arabic) / month / today. Enter goes into the day's items, and Esc comes back. |
| F12 (Configure) | desk | The compliance profile. |
| Ctrl+F (Filter) | desk | Cycles the category filter: tax returns, payments, company filings, audit, reminders, to-dos, everything. |

**What Alt+W notes, by screen**

| Screen | Alt+W notes |
|---|---|
| Ledger statement | The ledger |
| Day Book | The voucher |
| Trial Balance, group summary, outstanding, ageing and MSME rows | The party or ledger |
| Stock Summary | The stock item |
| A saved voucher | The voucher |

Notes show on the ledger statement (its own and its vouchers', with ✎ on
voucher rows) and under a saved voucher's narration.

**Hijri dates**

With Ctrl+K → "Show Hijri dates" on:

- the calendar shows each day's Umm al-Qura day number;
- the month heading shows the Hijri months it spans;
- every date in the lists carries the Hijri date.

Weeks start on Sunday for SA and AE companies and on Monday elsewhere.

**Phones**

`desk.css` stacks the two columns and wraps each row into two lines. The
calendar is a CSS grid, not a table, so `mobile.css`'s sideways-scrolling
tables don't apply to it.

## 4. Open points

- **India, TCS statements:** the 15th (old Rule 31AA) for quarters to
  Q4 2025-26, and the 31st under Rule 219(4) from 2026-27. The new date
  comes from secondary sources only.
- **India, TDS and TCS forms:** the replacements for 27Q and 27EQ under the
  2026 Rules are not confirmed, so the rule text names Forms 138, 140 and
  143 only.
- **India, GSTR-9:** applies only above ₹2 crore, and the date is often
  extended. It is shown as optional.
- **Advance tax:** shown to every Indian company, as optional ("when the
  year's tax is ₹10,000 or more"). Presumptive taxpayers pay once, by
  15 March, and are not modelled.
- **UAE weekends:** whether the FTA moves a deadline that falls on a
  weekend is unverified.
- **UAE, KSA, UK and Singapore staggers:** the desk can't know the
  company's assigned periods. It defaults to calendar quarters, and F12
  sets the month a quarter ends in.
- **UK Annual Accounting Scheme:** not modelled. HMRC's MTD obligations
  remain the authority.
- **Weekends and holidays:** no date is moved in any country.
- **India, ROC dates:** AOC-4 and MGT-7 are dated from the last allowed AGM
  date. An earlier AGM moves them earlier. Use a to-do for the real date.
- **UK, corporation tax for a June year end:** 9 months and 1 day is read
  as 1 April.
- **UK, first year:** the default accounting reference date is the end of
  the month of the first anniversary. A first year end within 6 months of
  incorporation is skipped.
- **KSA, CR annual confirmation:** the day count is not found. Optional and
  dated on the anniversary.
- **UAE, corporate tax registration:** natural persons are registered only
  above AED 1m turnover. The profile's Y/N decides.
- **France, CA3:** the day (15th to 24th) depends on the company. The
  desk shows the 15th.
- **US, state sales tax:** the 20th is only typical. Each state's day,
  pre-payments and weekend rule differ. Estimated tax payments are not
  modelled.
- **Mexico, DIOT:** monthly by the last day of the next month (LIVA art. 32
  VIII). The SAT's 2025 changes to the DIOT format were not checked.
- **Germany, adviser deadlines:** the later dates for returns prepared by a
  tax adviser (AO §149(3)) and the permanent extension are not modelled.
- **Holidays:** no country's public holidays are counted.
- **Not built yet:**
  - assigning a to-do to a user;
  - repeating to-dos (a monthly close checklist);
  - email or WhatsApp reminders;
  - a notes list on the stock item vouchers report.

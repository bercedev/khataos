# Roadmap

KhataOS is in **alpha**. This page says honestly what works today, what works only in part, and what comes next. It was last checked against the code on 2026-10-21.

The "next" list is built from the ranked top-15 lists in the three Tally gap studies: [accounting](research/tally-gap-accounting.md), [inventory, payroll and tax](research/tally-gap-inventory-payroll-tax.md) and [user experience and platform](research/tally-gap-ux-platform.md). Row ids such as M5 or D1 refer to those studies.

## Where things stand

**Works** means built and wired to its key or menu. **Partial** means part of the job is done. **Not yet** means nothing usable exists, even if a Tally-compatible key is reserved for it.

### Core, storage and the app

| Area | Works | Partial | Not yet |
|---|---|---|---|
| Books and storage | Double-entry engine with Tally-style default groups; one SQLite file per company; hash-chained, append-only audit trail that is the storage format; the trail is verified on every open | | Snapshot checkpoints for very large trails; encryption of the company file |
| Keyboard and shell | Tally-compatible key map resolved by screen context; home menu letters; Go To (`Alt+G`), Find (`Alt+F`, ledgers), Report Directory; current date `F2` and period `Alt+F2`; type-ahead while screens load; a "not available yet" message on most reserved keys | Some reserved keys still do nothing silently (for example `Ctrl+F` in vouchers and reports) | Help (`F1`), Switch To (`Ctrl+G`), `F11` features, `F12` configure (except on the Desk), Data / Print menus, notifications, a screen to remap keys |
| Companies | Create, open by path, company list; country picked once on first open; company details (tax ID, region, address) | One company open at a time | Several companies open at once, group companies, changing the financial year start |
| Users and security | Users per company with Argon2id passwords; roles Owner, Accountant, Data Entry, Auditor, enforced in the backend | Four fixed roles | Custom security levels, rights per voucher type, locking the books up to a date |
| Languages | English and Arabic with a full right-to-left layout (`Ctrl+K`); Hijri dates for display; Arabic keyboard support; bilingual KSA and UAE invoices | Some country screens and messages are still in English | Data-entry language (`Ctrl+W`), more display languages |
| Mobile | Android and iOS builds of the same app: touch layout, F-key drawer, phone calculator, phone file handling | | Printing on phones, office-server mode on phones, store releases, a tablet layout |
| Releases | Builds from source; CI runs tests and lints on Linux, macOS and Windows | | Signed installers and app-store builds |

### Accounting

| Area | Works | Partial | Not yet |
|---|---|---|---|
| Masters | Ledgers (party, cash and bank, duty, income and expense) with tax ID checks, credit period, mailing details, currency; groups (renamable defaults); units; stock groups; stock items with HSN/SAC and costing method; godowns; tax rates by date; alter and delete-when-unused | No new primary groups; group alias only when altering | Ledger aliases, opening bills on party ledgers (M5), bank details and cheque books on ledgers (M7), cost centres and categories (M8, M9), budgets, scenarios, voucher classes, a multi-master grid |
| Vouchers | Contra, Payment, Receipt, Journal, Sales, Purchase, Credit Note, Debit Note and Stock Journal; double entry, single entry and item invoice (`Ctrl+H`); bill-wise details; tax lines filled from the country pack; alter, cancel keeping the number (`Alt+X`), delete (`Alt+D`); numbering series per type with yearly or monthly restart and `{FY}`-style tokens; foreign-currency lines with forex gain or loss | Every party line asks for bill-wise details (no per-ledger switch); no manual voucher numbers on screen; one foreign currency per voucher; no currencies master, rates table or revaluation screen | Orders, delivery and receipt notes, rejections, physical stock, payroll vouchers; optional, post-dated, memorandum and reversing vouchers; custom voucher types (M13); duplicate and insert (`Alt+2`, `Alt+I`); repeat narration (`Ctrl+R`, `Alt+R`); interest calculation |
| Inventory | Units, stock groups and items with opening stock; item invoices; godowns and batches on lines; Stock Journal for transfers, consumption and production; average, FIFO and last purchase costing; rate memory per item and party; negative-stock warning | Batches are names only | Batch expiry, reorder levels, price lists, compound and alternate units, landed cost, bill of materials, job work, POS |
| Banking | Bank reconciliation; CSV statement import with automatic matching and vouchers from unmatched lines; cheque printing (CTS-2010); cheque register of uncleared withdrawals | One cheque layout; CSV only | Post-dated cheque management (BNK-07), Excel and MT940 statements, deposit slips, payment advice, e-payment files |
| Reports | Day Book, ledger statements, Trial Balance, Profit & Loss, Balance Sheet, Group Summary, Chart of Accounts, outstanding receivables and payables, ageing, MSME overdue, ratio analysis, dashboard, stock summary (detailed with `Alt+F5`), stock item vouchers, godown and batch summaries; drill-down to the voucher; print (save as PDF) and CSV export | Trial Balance is a flat ledger list; fixed ageing bands; group summaries are flattened | Registers (B6, B7), ledger monthly summary and cash/bank books (B3, B5), filters (`Ctrl+F`), columns and period comparison (`Alt+C`, `Alt+N`), saved views, exception reports, edit-log viewer (`Alt+Q`), cash flow and funds flow |
| Year end | Balances carry forward on their own (one continuous trail); yearly numbering restarts | Profit & Loss balance isn't split into brought forward and current year | Locking a closed year, splitting a company |

### Documents, sharing and data

| Area | Works | Partial | Not yet |
|---|---|---|---|
| Invoices and printing | Invoice design (templates, logo, colours, bank details, terms, titles); print through the system dialog; a native PDF writer for invoices and notes; Day Book PDF of a period's invoices; ship-to and export details | A4/Letter/A5 and 80 mm receipts only through the design; reports print on A4 | E-mail (`Ctrl+M`) and WhatsApp (`Ctrl+Alt+W`), print preview settings, batch printing other than the Day Book PDF, Excel export |
| Sharing and sync | Office server built into the app (LAN); invites with Accountant or Viewer access, expiry and revoke; end-to-end encrypted relay with moving relays; `.khatapkg` accountant files; browser access for viewers on the LAN; self-hosted `khata-server` with sync, rebase and a review screen | Desk notes, the compliance profile, invoice design, UK VAT schemes and sync settings stay on one computer | A hosted relay run by the project |
| Desk and calculator | Accountant's Desk: to-dos, notes on records (`Alt+W`), compliance calendar from each country's rules, reminders from bills; calculator panel (`Ctrl+N`) with tax add and take-out | Due dates don't count public holidays | System notifications |
| Import and export | Tally XML import (masters and Day Book) with a skip report; CSV export of every report | The importer drops bill-wise, cost centre and bank allocations, voucher tax detail, stock vouchers, orders and compound units | Live import from a running Tally (the crate has a client; the app doesn't use it), Excel import, XML/JSON export of masters and vouchers |
| Backup | Backup and restore in the Data menu (`Alt+Y`): a checked snapshot of the open company, optional password encryption, restore as a new company or replace with a kept copy; automatic backups on close and daily that keep N; a backup before every risky action; a reminder after 7 days; phones save and pick through the system dialogs | Automatic backups on phones stay inside the app until saved elsewhere | Cloud backup target, backups of several companies at once, a notifications panel for failed automatic backups (H4) |

### Countries

The status labels are those in the [README](../README.md#countries): **tested live** against the authority's test service, **tested against a test server only** (built, and tested against our own server that imitates the published API), and **needs official validation** (the file is generated but not yet checked with the official validator).

| Country | Works in the app | Connection | Still open |
|---|---|---|---|
| India | GSTIN checks, place of supply, CGST/SGST/IGST; Rule 46 numbering; GSTR-1 (with draft JSON) and GSTR-3B; GSTR-2B reconciliation; reverse charge, ITC, blocked credit, Rule 37 and GSTR-1A; e-invoice and e-way bill JSON; IRN through an IRP; TDS and TCS with payable reports, quarterly statements and the FVU text file; salary TDS estimate; Schedule III statements, notes and mapping; MSME overdue | IRP: tested against a test server only. JSON and FVU files: need official validation | GSP filing, GSTR-9/9C, composition, several GSTINs per company, Rule 53 checks on notes, e-way bill API (Part B, cancel, extend), TDS certificates, payroll |
| UAE | TRN checks, 5% VAT, reverse charge and imports, VAT 201, PINT AE XML | Peppol access point: tested against a test server only. PINT AE: needs validation | Input-tax blocking, FTA audit file, Corporate Tax, WPS |
| Saudi Arabia | VAT number checks, 15% VAT, VAT return, bilingual invoices, ZATCA Phase 2 (hash chain, QR, XAdES, CSID onboarding, clearance and reporting, queue) | Tested live on ZATCA's developer sandbox | Simulation and production runs, ZATCA SDK checks, Zakat |
| Singapore | GST number checks, GST rate history, reverse charge and import GST, GST F5, PINT SG (InvoiceNow) | Access point: tested against a test server only. PINT SG: needs validation | F5 e-filing, IRAS audit file, MES, A3PL and IGDS schemes |
| UK | VAT number check digits, 20/5/0%, reverse charge, import VAT, VAT schemes (cash, flat rate, annual), 9 box VAT return, MTD submission with fraud-prevention headers | MTD: tested against a test server only | A live HMRC test with a registered developer application |
| Germany, France, Netherlands, Spain | Tax ID checks, rates, reverse charge, UStVA, CA3, aangifte, modelo 303; XRechnung, Factur-X, VeriFactu records | None (nothing is sent). Files: need validation | Filing, Peppol BIS export, VeriFactu settings and QR on prints, French legal mentions, ViDA |
| US | EIN formats, company-defined jurisdictions, nexus note, use tax, exemption certificates, Form 1099 list | None | Address-based rates, state filing |
| Mexico | RFC checks, IVA and withholdings, CFDI 4.0 with the CSD seal, payment receipts, IVA summary, PAC stamping and cancellation | PAC: tested against a test server only. CFDI: needs validation | DIOT, electronic accounting, payroll CFDI |
| Others | | | Brazil (NF-e) and other LatAm packs |

## Next

Ranked by the pain a Tally user feels in the first week, then by value for the effort. Items from the three studies that cover the same need are merged.

### First: protect the data, and make migration complete

1. ~~**Backup and restore**, then **scheduled backups** that keep several versions (D1, D2, D3).~~ Done: see [Backup](guide/backup.md). Next: make the Tally import ready for real company data ([readiness review](research/tally-import-readiness.md)).
2. **Opening bills on party ledgers, and importing bill allocations from Tally** (M5). Without them, outstandings and ageing are empty after a migration.
3. **Edit-log viewer** (`Alt+Q`) with a version diff, then edit-log summaries (Y7, Y8, S5). The data is already in the trail; auditors need to see it.

### Daily entry and reports

4. **Speed keys in data entry:** duplicate (`Alt+2`) and insert (`Alt+I`) a voucher from the Day Book, repeat narration (`Ctrl+R`, `Alt+R`), `PgUp` and `PgDn` between saved vouchers (V3, V5, E2, E3, E8, E9).
5. **Group-wise Trial Balance and group summaries** with detailed and condensed (`Alt+F5`) and `Shift+Enter` (R1, B4, F4, N7).
6. **Registers:** sales, purchase, journal and the other voucher types, month by month, with `F4` to switch type (B6, B7).
7. **Ledger monthly summary, and the cash and bank books** (B3, B5).
8. **Report filter** (`Ctrl+F`), starting with the Day Book and ledger statements (F11, N11).
9. **Custom voucher types** with their own series, kept by the importer (M13).
10. **Post-dated cheques** (`Ctrl+T`) **and optional vouchers** (`Ctrl+L`), with their registers (V7, V8, BNK-07).
11. **Exception reports** (negative ledgers, overdue only, cancelled) and `Ctrl+J` (X7, X8, X10).
12. **Multi-column reports and period comparison** (`Alt+C`, `Alt+N`) (F5, F6, N12).

### Trade and inventory

13. **Sales and Purchase Orders**, then **Delivery and Receipt Notes** with order-to-invoice tracking and pending bills (ORD-01 to ORD-09).
14. **Batch manufacturing and expiry dates**, **reorder levels** and the reorder report, and the **physical stock** voucher (INV-08, INV-12, INV-15, RPT-05, RPT-08).
15. **Landed cost**, **alternate and compound units**, and **price lists** (MFG-08, INV-04, INV-05, INV-09).
16. **Cost centres and categories**, with allocation on lines, reports and import (M8, M9, X5).

### Sending and printing

17. **E-mail the document on screen** (`Ctrl+M`) as a PDF, then WhatsApp (X7, X8).
18. **Export to Excel (.xlsx) and to a PDF file for any report**, with the `Alt+E` menu (X3, X4).
19. **Print preview and print settings**, including A5 and thermal paper per voucher type, and the `Alt+P` menu (X10, X11).
20. **Import from Excel or CSV** with a column-mapping template (X1).

### Help and comfort

21. **Help** (`F1`): the keys for this screen, a link to the guide, what's new, report a problem (H1, H2).
22. **Switch To** (`Ctrl+G`) and the **last entry date** on the home screen and company list (N3, U4).
23. **Syncing company settings** (desk notes, compliance profile, invoice design, VAT schemes) with the books.

### Compliance and payroll

24. **Official validation** of every generated file (GSTR-1 JSON, e-invoice and e-way bill JSON, FVU file, PINT, XRechnung, Factur-X, VeriFactu, CFDI), and **live tests** with real credentials for the IRP, HMRC MTD, the Mexican PAC, Peppol access points and ZATCA simulation.
25. **India:** e-way bill online (GST-09), several GSTINs per company, Rule 53 checks on notes, GSTR-9/9C; the server running the pack's checks on push.
26. **Payroll core** (employees, pay heads, salary structures, attendance, payroll voucher, payslips), then **India PF, ESI and PT**, then **UAE WPS and gratuity** (PAY-01 to PAY-13, PAY-UAE).
27. **Lock the books up to a date**, and split the Balance Sheet's profit into brought forward and current year (Y4, Y5).
28. **Interest on balances and overdue bills**, with interest reports (V11, X4).
29. **Bill of materials** driving the manufacturing journal (MFG-01, MFG-02).
30. **Banking completeness:** Excel and MT940 statements, payment advice, deposit slips, cheque-book ranges (BNK-02, BNK-06, BNK-08, BNK-09).

Also planned: a hosted relay once the project's domain is chosen (a build sets it with `KHATA_DEFAULT_RELAY`), live import from a running Tally with a migration wizard, signed release builds, and several companies open at once.

Want to help with one of these? See [Contributing](../CONTRIBUTING.md).

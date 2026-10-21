# TallyPrime vs KhataOS: gaps in the user experience and the app platform

> **Researched:** 2026-10-21, against TallyPrime 7.x (7.0 is the current major release, and 7.1/7.2 notes are on the help site).
> **Scope:** navigation, help, data entry aids, import/export/sharing, security, data management and usability. The accounting and tax rules are out of scope; other research files cover them.
> **Clean room:** every feature below is described in our own words from Tally's public help pages, release notes and product pages. No Tally text, screen layout, image, binary or login was used. Read the sources for the facts, not for wording or design.
> **Companion files:** `tally-shortcuts.md` (the key map) and `tally-pain-points.md` (user complaints).

## How to read this file

- **Status:** `have` (built and wired to its key), `partial` (some of it is built, or it is built but not reachable the Tally way), `missing`, or `in progress` (another helper is building it now).
- **File:** where the KhataOS code lives, or where it would go. Paths are relative to the repository root. `NOT_YET` and `NOT_BUILT` are sets in `apps/desktop/src/shell.ts`: those keys are bound in `crates/khata-keymap/src/tally.rs`, but when no screen handles one, pressing it only shows "isn't available in this version yet".
- **Effort:** S is up to about 2 days, M is about 1 to 2 weeks, and L is more than 2 weeks.
- **Priority:** P1 means a Tally user misses it in the first week. P2 means it is expected by power users or offices. P3 is nice to have.
- **Source:** a public page. Pages with `te9rel` or `docs/te9` in the path are Tally.ERP 9-era help. They are cited only where TallyPrime has no newer page, and their option names may differ.

Common sources, cited by short name below:

| Short name | URL |
|---|---|
| KEYS | https://help.tallysolutions.com/tally-prime/keyboard-shortcuts/keyboard-shortcuts-tally-prime/ |
| FAMILIAR | https://help.tallysolutions.com/?p=108972 (Get familiar with TallyPrime: home screen, Help menu, buttons) |
| REPORTS | https://help.tallysolutions.com/tally-prime/reports-in-tallyprime/working-with-reports/ |
| FAQ | https://help.tallysolutions.com/tallyprime-faq/ |
| R70 | https://help.tallysolutions.com/release-notes-tallyprime-7-0 |

---

## 1. Navigation and the key map

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| N1 | Gateway menu | The home menu: one highlighted letter opens each area (create, alter, vouchers, Day Book, reports). | **have** | `apps/desktop/src/screens/home.ts` (SECTIONS); Gateway letters in `crates/khata-keymap/src/tally.rs` | – | – | FAMILIAR |
| N2 | Go To (Alt+G) | Search box that opens any report, voucher type or master. Esc then returns you to where you were. | **have**. Gaps: no stock item, stock group or group rows, and no "alter X" rows | `apps/desktop/src/screens/launcher.ts` (GoToScreen) | S to extend | P2 | FAQ |
| N3 | Switch To (Ctrl+G) | Like Go To, but it replaces the current screen, so Esc does not come back. | **missing** (`NOT_YET`: SwitchTo) | `apps/desktop/src/shell.ts`; reuse GoToScreen and reset the stack | S | P2 | FAQ |
| N4 | Find / SmartFind | Search across masters and transactions, and still match when a name is misspelled or partial (SmartFind is new in 7.0). | **partial**: Alt+F finds ledgers by substring only | `apps/desktop/src/screens/launcher.ts` (FindScreen) | M | P2 | R70 |
| N5 | Calculator panel (Ctrl+N) | A calculator that opens next to any screen and can paste its result into a field. | **have** (updated 2026-10-21: built since this study; Ctrl+N anywhere, Alt+C in an amount field, result into the field with Ctrl+Enter) | `apps/desktop/src/calcpanel.ts`, `apps/desktop/src/calc.ts` | – | – | KEYS |
| N6 | Current date (F2) and period (Alt+F2) at the top level | Sets the working date and period for the open company, and the home panel shows both. | **have** (just added) | `apps/desktop/src/shell.ts` (askCurrentDate, askCurrentPeriod); `apps/desktop/src/screens/home.ts` | – | – | FAMILIAR |
| N7 | Detailed/condensed toggle | Expands a report to show sub-groups and ledgers, and collapses it again. TallyPrime uses Alt+F5; Alt+F1 is the Tally.ERP 9 key. Both are bound. | **partial**: only the stock screens handle ToggleDetailed. Trial Balance, Balance Sheet and P&L do not | `crates/khata-keymap/src/tally.rs`; `apps/desktop/src/screens/inventory.ts`; `apps/desktop/src/screens/reports.ts` | M | P1 | REPORTS |
| N8 | Change View (Ctrl+H in reports) | Lists the other views of the current report, such as monthly, columnar or by group, and switches to one. | **partial**: only the dashboard handles it | `apps/desktop/src/screens/analysis.ts` | M | P2 | KEYS |
| N9 | Change report context (F4) and other views (F5/F6) | Changes the subject of the report, for example another ledger or voucher type, or another view, without leaving it. | **missing** (bound; no report handles ChangeContext, AltView1 or AltView2) | `apps/desktop/src/screens/reports.ts` | M | P2 | REPORTS |
| N10 | Basis of values (Ctrl+B) | Chooses what a report counts, for example whether post-dated or optional vouchers are included. | **missing** | `apps/desktop/src/screens/reports.ts` | M | P3 | https://help.tallysolutions.com/use-save-view-feature-in-tallyprime/ |
| N11 | Report filter (Ctrl+F) and filter details | Filters a report by a field and a condition (basic, multi or advanced). A row can then show which field matched. | **missing** (Filter and FilterDetails are bound, but no screen handles them) | `apps/desktop/src/screens/reports.ts` | M | P1 | https://help.tallysolutions.com/apply-filter-in-reports/ |
| N12 | Report columns (Alt+C, Alt+N) | Adds comparison columns: another period, another company or automatic monthly or quarterly columns. | **missing** | `apps/desktop/src/screens/reports.ts`; `crates/khata-core/src/reports.rs` | L | P2 | REPORTS |
| N13 | Save View (Ctrl+L in reports) | Saves a report's settings under a name and reopens them later with fresh data. | **missing** | `apps/desktop/src/screens/reports.ts` | M | P3 | https://help.tallysolutions.com/use-save-view-feature-in-tallyprime/ |
| N14 | Exception reports (Ctrl+J) | Lists the problems in the current report, such as negative stock or unbalanced entries. | **missing** | `apps/desktop/src/screens/reports.ts` | M | P3 | KEYS |
| N15 | Hide and restore lines (Ctrl+R, Ctrl+U in reports) | Temporarily hides report rows while you read or print. | **missing** | `apps/desktop/src/screens/reports.ts` | S | P3 | KEYS |
| N16 | F12 Configure | Per-screen options, for example extra fields in voucher entry or in masters, and which columns a report shows. | **missing** (`NOT_YET`: Configure) | `apps/desktop/src/shell.ts`; `apps/desktop/src/screens/settings.ts` | L | P2 | https://help.tallysolutions.com/docs/te9rel53/Maintaining_Company_Data/F12_Config/Voucher_Entry.htm |
| N17 | F11 Company features | One screen to switch feature areas on and off: accounting (bills, cost centres), inventory, tax and online access. | **partial**: there is no single screen (`NOT_YET`: CompanyFeatures). The country pack fixes tax (`country.ts`), inventory is always on, and online access sits under Alt+K (`sharing.ts` onlineAccessItems) | `apps/desktop/src/screens/country.ts`; `apps/desktop/src/screens/sharing.ts` | M | P2 | https://help.tallysolutions.com/company-features-f11-tally/ |
| N18 | Several companies open at once (F3) | Keeps several companies loaded, switches between them with F3, and the home panel lists each one. | **partial**: one company at a time. F3 closes the company and returns to the company list | `apps/desktop/src/shell.ts` (closeCompany) | L | P2 | FAMILIAR |
| N19 | Create a master from a list field (Alt+C) | Creates a missing ledger, item or godown in the middle of a voucher. | **have** | `apps/desktop/src/screens/voucher.ts` (bottom bar, CreateMasterInline) | – | – | KEYS |
| N20 | Button bar with key labels | The side or bottom bar shows each available action with its key, and greys out the ones that do not apply here. | **have** | `apps/desktop/src/shell.ts` (btn, FSLOTS) | – | – | FAMILIAR |
| N21 | Top menu keys (Alt+K, Alt+Y, Alt+Z, Alt+O, Alt+E, Alt+M, Alt+P) | The menus for company, data, exchange, import, export, share and print. | **partial**: Alt+K, Alt+Z, Alt+O and Alt+M work. Alt+Y (data), Alt+E (export) and Alt+P (print) are `NOT_YET` | `apps/desktop/src/shell.ts`; `apps/desktop/src/screens/launcher.ts`; `apps/desktop/src/screens/sharing.ts` | S per menu, once its contents exist | P2 | FAMILIAR |

## 2. Help and learning

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| H1 | Help menu (F1) | Opens the help site, what's new, troubleshooting (repair, migrate), settings (startup, display, language), add-ons and an about page. A dot marks it when an upgrade is out. | **missing** (`NOT_YET`: Help) | `apps/desktop/src/shell.ts` | M | P1 | FAMILIAR |
| H2 | Context help (Ctrl+F1) | Help for the field or screen you are on. | **missing** (`NOT_YET`: ContextHelp) | `apps/desktop/src/shell.ts`; `docs/ux/interaction-spec.md` | M | P2 | KEYS |
| H3 | Learn / "Try Now" tips | Feature tips on the home screen, and a page that keeps the tips you dismissed, so you can learn the product inside it. | **missing** | new screen; `apps/desktop/src/screens/home.ts` | M | P3 | https://help.tallysolutions.com/?p=192621 |
| H4 | Notifications panel (Ctrl+Alt+N) | A bell with a list of time-bound items: subscription, upgrades, return status, failed backups and scheduler status. An item clears once you act on it. | **missing** (`NOT_YET`: Notifications). This would also be the natural home for the compliance calendar below | `apps/desktop/src/shell.ts` | M | P2 | https://help.tallysolutions.com/notifications-report/ |
| H5 | Accountant notes, to-do list and compliance calendar | Reminders for due dates and open items. Tally has no exact equivalent; its notifications cover the due-date part. | **have** (updated 2026-10-21: built since this study as the Accountant's Desk, T on the home menu, with Alt+W notes) | `apps/desktop/src/screens/desk.ts`, `apps/desktop/src-tauri/src/desk.rs` | – | – | https://help.tallysolutions.com/notifications-report/ |
| H6 | Display language (Ctrl+K) | Shows labels in another language while the data stays as entered. | **partial**: English and Arabic. Tally offers Arabic and Bangla display, and Indian languages for data entry | `apps/desktop/src/i18n.ts` (LANGUAGES); `apps/desktop/src/screens/launcher.ts` (languageMenu) | M per language | P2 (Hindi first) | https://help.tallysolutions.com/use-tallyprime-in-your-preferred-language/ |
| H7 | Data entry language (Ctrl+W) | Types names and narration in a regional script, with phonetic transliteration. | **missing** (`NOT_YET`: EntryLanguage) | `apps/desktop/src/shell.ts` | L | P3 | https://help.tallysolutions.com/use-tallyprime-in-your-preferred-language/ |

## 3. Data entry aids

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| E1 | Type-to-filter on every list | Typing narrows any pick list of ledgers, items, groups or companies. | **have** (Go To matches words in any order) | `apps/desktop/src/picker.ts`; `apps/desktop/src/screens/launcher.ts` (matches) | – | – | KEYS |
| E2 | Duplicate a voucher (Alt+2) | From the Day Book or a ledger, copies a saved voucher into a new one so you only change the date and amounts. | **missing** (DuplicateVoucher is bound in Report context; nothing handles it) | `apps/desktop/src/screens/reports.ts`; `apps/desktop/src/screens/voucher.ts` | S | P1 | https://help.tallysolutions.com/accounting-faq/ |
| E3 | Insert a voucher before or after a row (Alt+I, Alt+A in the Day Book) | Opens a new voucher on the selected row's date, at that position. | **missing** (InsertVoucherBefore and AddVoucherAfter are not handled) | `apps/desktop/src/screens/reports.ts` (DayBook) | S | P2 | https://help.tallysolutions.com/day-book-tally/ |
| E4 | Copy and paste voucher lines | Tally only copies and pastes text inside fields. Copying whole voucher lines is not documented. | **missing** (the OS text copy works in fields) | `apps/desktop/src/screens/voucher.ts` | M | P3 | KEYS |
| E5 | Recurring and scheduled vouchers | Tally has no true scheduler. Its guidance is to keep a memorandum voucher as a template and enter future-dated ones as post-dated. | **missing** | new: `crates/khata-core/src/voucher.rs` and a screen | L | P2 (a chance to improve on the usual workaround) | https://tallysolutions.com/business-guides/how-to-automate-recurring-invoices-and-expenses-in-tallyprime/ |
| E6 | Optional or memorandum vouchers (Ctrl+L) and post-dated vouchers (Ctrl+T) | Saves an entry that does not touch the books until it is made regular or until its date arrives. | **missing** (MarkOptional and MarkPostDated are bound; nothing handles them) | `apps/desktop/src/screens/voucher.ts`; `crates/khata-core/src/voucher.rs` | M | P2 | KEYS |
| E7 | Voucher entry modes (Ctrl+H) | Switches between item invoice, accounting invoice and plain voucher. Plain voucher can be single or double entry. | **have** (double, single and item, with Ctrl+H) | `apps/desktop/src/screens/voucher.ts` (Mode, modeLabel) | – | – | https://help.tallysolutions.com/sales-of-goods-services/ |
| E8 | Retrieve narration (Ctrl+R, Alt+R) and narration templates | Fills the narration from the last voucher of the same type (Ctrl+R) or the same ledger (Alt+R). | **missing** (both keys bound; nothing handles them) | `apps/desktop/src/screens/voucher.ts` | S | P1 | KEYS |
| E9 | Previous and next voucher (PgUp, PgDn) in voucher alteration | Walks through saved vouchers without going back to the Day Book. | **missing** in the voucher screen (reports handle it) | `apps/desktop/src/screens/voucher.ts` | S | P2 | KEYS |
| E10 | Unused voucher numbers (Ctrl+Alt+U) | Lists the numbers freed by deletion and lets you reuse one. | **missing** (bound; nothing handles it) | `apps/desktop/src/screens/voucher.ts`; `crates/khata-core/src/numbering.rs` | S | P3 | https://help.tallysolutions.com/accounting-faq/ |
| E11 | Stock query (Alt+S) and related reports (Ctrl+O) from a voucher | Checks an item's stock, or opens the party's ledger, without leaving the entry. | **missing** (both keys bound; nothing handles them) | `apps/desktop/src/screens/voucher.ts` | S | P3 | KEYS |

## 4. Import, export, sending and printing

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| X1 | Import from Excel | Imports masters and transactions from a spreadsheet, using sample files or a column-mapping template. Wrong rows go to an exceptions list. | **missing** (Alt+O offers only Tally XML and .khatapkg) | `apps/desktop/src/screens/launcher.ts` (importMenu); `apps/desktop/src-tauri/src/export.rs` | L | P1 | https://help.tallysolutions.com/tally-prime/data-management/import-data-in-tally/ |
| X2 | Import from XML or JSON | Imports Tally-format XML, or JSON from 7.0, for masters and vouchers. | **partial**: Tally XML file and live Tally import (masters and Day Book), with a list of skipped items. No JSON | `crates/khata-import-tally/src/`; `apps/desktop/src/screens/company.ts` (importFromTally); `apps/desktop/src/screens/reports.ts` | M | P2 | https://help.tallysolutions.com/import-data-from-xml-json/ |
| X3 | Export a report to Excel | Saves the current report as a spreadsheet. | **partial**: CSV only (it opens in Excel, but has no formatting or sheets) | `apps/desktop/src/screens/reports.ts` (ExportCurrent); `apps/desktop/src-tauri/src/commands.rs` (report_csv, write_csv) | M | P2 | https://help.tallysolutions.com/tally-prime/data-management/export-data-in-tally/ |
| X4 | Export to PDF | Saves an invoice or report straight to a PDF file. | **partial**: through the print window and the OS "Save as PDF". Direct PDF only for EU Factur-X | `apps/desktop/src/main.ts` (printMode); `apps/desktop/src/api.ts` (facturxPdf) | M | P2 | https://help.tallysolutions.com/tally-prime/data-management/export-data-in-tally/ |
| X5 | Export data as XML or JSON | Exports masters and vouchers, with their dependent masters, for other software or another company. | **missing** (only tax-return JSON such as GSTR-1 and e-invoice. .khatapkg is KhataOS-only) | `apps/desktop/src-tauri/src/export.rs` | M | P2 | https://help.tallysolutions.com/tally-prime/data-management/export-data-in-tally/ |
| X6 | Export and import masters between companies | Copies the chart of accounts and items from one company into a new one. | **missing** | `apps/desktop/src-tauri/src/masters.rs`; `apps/desktop/src-tauri/src/export.rs` | M | P2 | https://help.tallysolutions.com/how-to-import-data-from-xml-into-tallyprime-tallyhelp/ |
| X7 | E-mail the current document (Ctrl+M) | Sends an invoice or report as a PDF attachment. The sending profile supports SMTP, app passwords and browser (OAuth) login. | **missing** (`NOT_YET`: EmailCurrent). Party e-mail is already stored on the ledger | `apps/desktop/src/shell.ts`; `apps/desktop/src/screens/ledger.ts` (email field) | M (L with OAuth) | P1 | https://help.tallysolutions.com/e-mailing-in-tallyprime/ |
| X8 | WhatsApp the current document (Ctrl+Alt+W) | Sends an invoice or report to the party on WhatsApp. Tally does this through a paid business provider. | **missing** (`NOT_YET`: WhatsAppCurrent) | `apps/desktop/src/shell.ts` | M (an OS share sheet or a wa.me link with the PDF is S) | P2 | https://help.tallysolutions.com/share-documents-using-whatsapp-for-business/ |
| X9 | Print the current document (Ctrl+P) | Prints the voucher or report on screen, or a cheque. | **have** | `apps/desktop/src/screens/reports.ts` (print); `apps/desktop/src-tauri/src/commands.rs` (print_report, print_voucher, print_cheque) | – | – | https://help.tallysolutions.com/print-documents/ |
| X10 | Print preview | Shows the page as it will print, and can be set to always preview first. | **partial**: the print window draws the page but goes straight to the OS dialog | `apps/desktop/src/main.ts` (printMode) | S | P2 | https://help.tallysolutions.com/configure-printing-for-invoices-reports/ |
| X11 | Print configuration (Alt+P → Configuration) | Sets the printer, paper size, copies per voucher type and default preview. | **missing**: the page is fixed at A4 and Alt+P is `NOT_YET` | `crates/khata-print/src/lib.rs` (`@page { size: A4 }`); `apps/desktop/src/shell.ts` | M | P2 | https://help.tallysolutions.com/configure-printing-for-invoices-reports/ |
| X12 | Print, e-mail or export many vouchers at once | Batch-prints a day's or a party's invoices, one per page, and the main books (cash and bank books, ledgers, registers). | **missing** | `apps/desktop/src-tauri/src/commands.rs`; `crates/khata-print/src/lib.rs` | M | P2 | https://help.tallysolutions.com/tally-prime/accounting-financial-reports/printing-multi-account-reports |
| X13 | Send to the accountant as a file | Sends the books, or only the entries since the last file, to an auditor, and merges them on the other side. | **have** (encrypted .khatapkg that only adds new entries; Tally does this with export and import) | `apps/desktop/src/screens/sharing.ts` (sendToAccountant, openAccountantFile); `apps/desktop/src-tauri/src/package.rs` | – | – | https://help.tallysolutions.com/how-to-import-data-from-xml-into-tallyprime-tallyhelp/ |

## 5. Security and users

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| S1 | Users and passwords, and changing user without closing the company | Gives each person a login. Signing out and back in changes the rights in force. | **have** (Argon2id hashes; sign-out sits in the Company menu) | `apps/desktop/src-tauri/src/security.rs`; `apps/desktop/src/screens/signin.ts` | – | – | https://help.tallysolutions.com/manage-users-in-tallyprime/ |
| S2 | Security levels | Lets the owner define named levels, each a set of allowed and disallowed rights. | **partial**: 4 fixed roles (Owner, Accountant, Data Entry, Auditor). No custom levels | `apps/desktop/src/screens/signin.ts` (ROLES); `crates/khata-core/src/profile.rs` (UserRole) | M | P2 | https://help.tallysolutions.com/manage-users-in-tallyprime/ |
| S3 | Rights per voucher type and report, and back-dated limits | Allows display, create or alter per voucher type or report. Can also block back-dated entries past a number of days or a cut-off date. | **missing** | `apps/desktop/src-tauri/src/security.rs` | L | P2 | https://help.tallysolutions.com/manage-users-in-tallyprime/ |
| S4 | TallyVault (encryption at rest) | Encrypts the company data with a password that is never stored, so a lost password cannot be recovered. | **missing**: the SQLite company file is plain. Only the .khatapkg transfer file is encrypted | `crates/khata-store/src/lib.rs`; `apps/desktop/src-tauri/src/package.rs` | L | P2 | https://help.tallysolutions.com/data-security-faq/ |
| S5 | Edit log / audit trail (Alt+Q) | Records every create, alter and delete with the user and time, and cannot be switched off. Alt+Q on a voucher or master lists its versions and highlights the differences. | **partial**: the backend keeps a hash-chained trail, and database triggers make it append-only. There is no viewer: EditLog (Alt+Q) is bound but nothing handles it | `crates/khata-core/src/audit.rs`; `crates/khata-store/src/lib.rs`; `apps/desktop/src/screens/voucher.ts` | M | P1 | https://help.tallysolutions.com/edit-log-in-tallyprime-faq/ |
| S6 | Remote access | Uses the office company from elsewhere with the full app: entries and reports. | **have** (invites, revoke, relay or LAN server, connectivity status) | `apps/desktop/src-tauri/src/lan.rs`; `apps/desktop/src/screens/sharing.ts` (RemoteAccessScreen, ConnectivityStatusScreen) | – | – | https://help.tallysolutions.com/tally/tally-remote-access |
| S7 | Browser access | Read-only reports in any browser, with drill-down to vouchers. Tally also lets you download them as PDF or Excel. | **have** (Viewer portal). Not checked: PDF or Excel download from the portal | `crates/khata-server/src/portal.rs`; `apps/desktop/src/screens/sharing.ts` (BrowserAccessScreen) | S (downloads) | P3 | https://help.tallysolutions.com/browser-reports-faq-tally/ |

## 6. Data management

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| D1 | Backup (manual) | Copies one or more companies to a folder or drive. | **have** (`Alt+Y` → Backup: a `VACUUM INTO` snapshot of the open company, verified after writing, optional password encryption; one company at a time) | `crates/khata-backup/src/lib.rs`; `apps/desktop/src-tauri/src/backup.rs`; `apps/desktop/src/screens/backup.ts` | – | – | https://help.tallysolutions.com/backup-local/ |
| D2 | Restore | Brings a backup back as a company, or restores one dated version. | **have** (`Alt+Y` → Restore: verified first, then a new company file, or Replace with the name typed, the old file kept as `.before-restore-…khata` and the company lock respected) | same as D1 | – | – | https://help.tallysolutions.com/backup-local/ |
| D3 | Scheduled auto backup | Backs up on a schedule (incremental by default), keeps N versions, and can also back up to Tally's cloud with an encryption password. Failed runs show in notifications. | **partial**: on close and daily, keeps N (default 10), a backup before risky actions, a desk reminder after 7 days. Full copies, not incremental; no cloud target; failures only in the log until H4 exists | same as D1 (`start_scheduler`, `due_on_close`, `rotate`) | S (H4, cloud) | P2 | https://help.tallysolutions.com/tallydrive-backup/ |
| D4 | Repair | Finds and fixes damaged company data, drops what it cannot recover, and reports counts before and after. | **missing**. The design makes it less needed: the trail is append-only and the books are rebuilt from it on every open | `crates/khata-store/src/lib.rs` (load replays the trail) | M | P3 | https://help.tallysolutions.com/repair-company-data-tally/ |
| D5 | Data verification | Checks the company data for errors, for example before a split. | **partial**: the hash chain is verified each time a company opens. There is no verify command or report the user can run | `crates/khata-store/src/lib.rs` (load); `crates/khata-core/src/audit.rs` | S | P2 | https://help.tallysolutions.com/split-company-data-in-tallyprime-faq/ |
| D6 | Migration | Moves data from older releases into the current format. It can be paused, and a summary of counts follows. | **have** for Tally to KhataOS (XML file and live Tally, with a skipped-items report). Schema versions in the store | `crates/khata-import-tally/src/`; `apps/desktop/src/screens/reports.ts` (import result); `crates/khata-store/src/lib.rs` (schema_version) | – | – | https://help.tallysolutions.com/tally-prime/data-management/migrate-company-data-tally/ |
| D7 | Split company data | Cuts a company at a date into two: the period before it and the period from it on. | **missing**. Less needed, because one file holds many years. A year-end "new company from closing balances" would cover the same need | `apps/desktop/src-tauri/src/commands.rs` | M | P3 | https://help.tallysolutions.com/split-company-data-in-tallyprime-faq/ |
| D8 | Group company (consolidation) | A virtual company that adds several member companies' Trial Balance, P&L and Balance Sheet together, without changing their entries. | **missing** | new: `crates/khata-core/src/reports.rs`; `apps/desktop/src/screens/reports.ts` | L | P2 | https://help.tallysolutions.com/group-company-tally/ |
| D9 | Company data path | Lets you pick the folder for each company when you create it, or open a company from any folder. | **have** (default app-data folder, path field on create, open by path) | `apps/desktop/src/screens/startup.ts`; `apps/desktop/src-tauri/src/commands.rs` (default_company_dir, list_company_files) | – | – | https://help.tallysolutions.com/set-up-company-tally/ |
| D10 | Several data locations | Lists companies from several folders or drives together, and can hide a location. | **partial**: the list shows the default folder, plus the folder of the last company opened | `apps/desktop/src/screens/startup.ts` (lines 80–87) | S | P3 | https://help.tallysolutions.com/article/te9rel65/Maintaining_Company_Data/F12_Config/data_configuration.htm |
| D11 | Open companies at startup | Opens a chosen set of companies on launch. | **have** for one company (the last one opened is remembered) | `apps/desktop/src/screens/startup.ts` (lastPath) | S for several | P3 | https://help.tallysolutions.com/set-up-tallyprime-auto-login-language-multiple-addresses/ |

## 7. Usability

| # | Feature | What it does | KhataOS status | File | Effort | Pri | Source |
|---|---|---|---|---|---|---|---|
| U1 | Dashboard | Tiles for sales and purchase trends, cash flow, receivables and payables, top ledgers and ratios. You can add, remove and repeat tiles, and keep several dashboards. | **partial**: a fixed set of tiles and charts, each with a table view. No choice of tiles, and one dashboard only | `apps/desktop/src/screens/analysis.ts` (DashboardScreen); `apps/desktop/src-tauri/src/insights.rs` | M | P3 | https://help.tallysolutions.com/tally-prime/dashboard/ |
| U2 | New company onboarding | Create the company (name, path, mailing name, address, financial year), then go straight to the features screen. | **partial**: create (name, path), then a country pick (FY, currency, tax rules). The address and tax ID are a separate form, there is no guided next step, and there is no opening-balance step | `apps/desktop/src/screens/startup.ts`; `apps/desktop/src/screens/country.ts`; `apps/desktop/src/screens/settings.ts` (CompanyDetailsForm) | M | P2 | https://help.tallysolutions.com/set-up-company-tally/ |
| U3 | Day Book shortcuts | Change the date or range (F2), drill into a voucher, cancel (Alt+X), duplicate (Alt+2), insert (Alt+I), filter (Ctrl+F), and select many rows to delete or cancel them together. | **partial**: range, drill-down and Alt+X work. Alt+2, Alt+I, filtering and selecting many rows do not | `apps/desktop/src/screens/reports.ts` (DayBook) | M | P1 | https://help.tallysolutions.com/day-book-tally/ |
| U4 | Last entry date shown | The home panel and the open-company list show the date of each company's last voucher. | **missing**: home shows the ledger and voucher counts, not the last entry date. The company list shows only the name and file | `apps/desktop/src/screens/home.ts` (paintInfo); `apps/desktop/src/types.ts` (CompanyInfo, CompanyFile) | S | P2 | https://help.tallysolutions.com/article/te9rel54/Maintaining_Company_Data/Company_Info/Gateway_of_Tally.htm |
| U5 | List of companies | A company list you can filter, that opens the chosen company, or creates or opens one elsewhere. | **have** | `apps/desktop/src/screens/startup.ts` (StartupScreen) | – | – | https://help.tallysolutions.com/set-up-company-tally/ |
| U6 | Home information panel | Shows the current period, the current date, the company and its online status. | **have** (also shows the country, counts, opening balance difference, user and file) | `apps/desktop/src/screens/home.ts` | – | – | FAMILIAR |
| U7 | Select rows in reports (Space, Ctrl+Space) | Marks rows for a bulk action: delete, cancel, print. | **partial**: only bank reconciliation | `apps/desktop/src/screens/banking.ts` | S | P3 | KEYS |

---

## Counts

Counted over the 77 rows above:

| Status | Rows |
|---|---|
| have | 19 |
| partial | 19 |
| missing | 39 |
| in progress | 0 |

N6 (F2 and Alt+F2 at the top level) was just added and is counted under **have**. N5 (calculator) and H5 (accountant's desk) were in progress when this study was written and are built now (updated 2026-10-21).

## Housekeeping found while checking

- `NOT_YET` in `apps/desktop/src/shell.ts` still lists `ExchangeMenu` and `ShareMenu`, but both are built and wired in `apps/desktop/src/main.ts`. The `switch` in `runShellAction` handles them before the `NOT_YET` check, so users see no problem. The set is just stale.
- *Updated 2026-10-21:* the shell now has a `NOT_BUILT` set next to `NOT_YET`, so most of the 25 actions that used to do nothing silently now show "isn't available in this version yet". A few still do nothing: Filter (handled only on the Desk), AltView1, AltView2 and Autofill. NewColumn and DeleteColumn are handled only by the Tax Rates list (`apps/desktop/src/screens/settings.ts`), where they add and delete a rate; elsewhere in reports they do nothing.
- Tally's official key table treats Alt+F5 as the detailed/condensed key in TallyPrime. Alt+F1 is the Tally.ERP 9 habit. Binding both, as `tally.rs` does, is right.

## Build next: ranked top 15

The ranking weighs what a Tally user misses in the first week against effort. In-progress items are left out.

| Rank | Item | Rows | Effort | Why now |
|---|---|---|---|---|
| 1 | ~~Backup and restore (manual)~~ (done) | D1, D2 | S | Local-first means the user owns the risk. With no backup, one lost disk loses the books. |
| 2 | ~~Scheduled auto backup with N kept versions~~ (done, except failures in a notifications panel) | D3 | M | Tally 7.0 made this standard. It builds on 1 and feeds the notifications panel. |
| 3 | Edit log viewer (Alt+Q) for vouchers and masters | S5 | M | The trail already exists, hash chained and append only. Auditors (Rule 3(1)) need to see it, and today they cannot. |
| 4 | Duplicate a voucher (Alt+2) from the Day Book and ledgers | E2 | S | The daily shortcut for repeat entries, and the key is already bound. |
| 5 | Retrieve narration (Ctrl+R, Alt+R) | E8 | S | Small, and heavily used in data entry. Both keys are already bound. |
| 6 | Detailed/condensed (Alt+F5, Alt+F1) on Trial Balance, Balance Sheet, P&L and group summaries | N7 | M | The most-pressed report key after Enter and Esc. |
| 7 | Report filter (Ctrl+F) with basic conditions, starting with the Day Book and ledger statements | N11, U3 | M | Finding one entry in a long Day Book is a core task. |
| 8 | E-mail the current document (Ctrl+M) as a PDF | X7, X4 | M | Sending invoices is expected. The party e-mail is already stored. Build direct PDF output with it. |
| 9 | F1 help menu: keys for this screen, link to the docs, what's new, about, report a problem | H1, H2 | M | Every Tally user's first key on an unfamiliar screen. It also gathers the settings that are now scattered. |
| 10 | Import from Excel/CSV with a column-mapping template (ledgers, items, vouchers) | X1 | L | The main route in for anyone not coming from Tally, and for bulk opening balances. |
| 11 | Day Book completeness: insert (Alt+I), select many to cancel or delete, and PgUp/PgDn in voucher alteration | U3, E3, E9, U7 | M | Closes the gap on the screen accountants live in. |
| 12 | Switch To (Ctrl+G) | N3 | S | Already designed. It reuses Go To with a stack reset. |
| 13 | Last entry date on home and in the company list | U4 | S | A small detail that accountants check every time they open a company. |
| 14 | Print preview, print configuration (paper size including A5 and thermal, copies per voucher type) and the Alt+P menu | X10, X11, N21 | M | A fixed A4 page blocks shops that print on small or thermal paper. |
| 15 | Export to Excel (.xlsx) and to a PDF file, and the Alt+E menu | X3, X4, N21 | M | CSV loses formatting. Accountants send .xlsx to clients and auditors. |

Next in line: notifications panel (H4, pairs with the compliance calendar), WhatsApp share (X8), several companies open at once (N18), F11 features screen (N17), custom security levels and per-voucher-type rights (S2, S3), encryption at rest (S4), recurring vouchers (E5), group company consolidation (D8).

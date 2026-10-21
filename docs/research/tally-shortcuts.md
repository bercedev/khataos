# TallyPrime Keyboard Map (functional reference)

> **Purpose.** This file records how TallyPrime responds to keys, so KhataOS can offer keyboard behaviour that Tally users already know.
> It covers **function only**. Do not copy Tally's UI, wording, screens, icons or branding.
>
> **Researched:** 2026-10-19. **Current Tally release at time of research:** TallyPrime 7.x. The help site banner advertises 7.2, and the release-wise feature page lists 7.0 as the latest major release.
>
> **Primary source:** Tally's official "Keyboard Shortcuts in TallyPrime" page, which also has a "Shortcut in Tally.ERP 9" column:
> <https://help.tallysolutions.com/tally-prime/keyboard-shortcuts/keyboard-shortcuts-tally-prime/>
> (Mirrors: <https://help.tallysolutions.com/keyboard-shortcuts-tally-prime/> and <https://help.tallysolutions.com/tally-prime/welcome-to-tally-prime/keyboard-shortcuts-tally-prime>)
>
> **Other sources:**
> - S2 Working with Reports in TallyPrime: <https://help.tallysolutions.com/tally-prime/reports-in-tallyprime/working-with-reports/>
> - S3 TallyPrime Features, Release-wise: <https://help.tallysolutions.com/tally-prime/welcome-to-tally-prime/get-familiar-with-tally-prime/>
> - S4 Tally.ERP 9 Function Key Combination: <https://help.tallysolutions.com/docs/te9rel54/Common_Files/Function_Key_Combination.htm>
> - S5 Tally.ERP 9 Hidden Shortcut Keys: <https://help.tallysolutions.com/docs/te9rel54/Common_Files/Key_Combination_Used_for_Navigation.htm>
> - S6 Tally.ERP 9 Keyboard Shortcut for Faster Navigation: <https://help.tallysolutions.com/article/te9rel54/Common_Files/Shortcut_Keys.htm>
> - S7 Day Book help: <https://help.tallysolutions.com/day-book-tally/>
> - S8 Sales voucher help: <https://help.tallysolutions.com/sales-of-goods-services/>
> - S9 TallyPrime FAQ: <https://help.tallysolutions.com/tallyprime-faq/>
>
> **Confidence tags used in `notes`:**
> - `[P]` means stated on the primary shortcut page.
> - `[Sx]` means stated in source Sx.
> - `[UNVERIFIED]` means taken from product knowledge or third-party material and not confirmed on an official page. Check these in a live TallyPrime install before treating them as a spec.

---

## 0. Corrections to the brief (read first)

Several common assumptions describe **Tally.ERP 9**, not TallyPrime. The table below shows what the official page says.

| assumed | TallyPrime actual | Tally.ERP 9 | source |
|---|---|---|---|
| Ctrl+F8 opens Credit Note | **Alt+F6** opens Credit Note. Ctrl+F8 opens **Sales Order** | Ctrl+F8 opened Credit Note | [P] |
| Ctrl+F9 opens Debit Note | **Alt+F5** opens Debit Note. Ctrl+F9 opens **Purchase Order** | Ctrl+F9 opened Debit Note | [P] |
| Alt+V opens Stock Journal | **Alt+F7** opens Stock Journal. Alt+V opens a **Manufacturing Journal from the Quantity field** of a journal voucher | same | [P] |
| Ctrl+K is a company key | **Ctrl+K** selects the display language. **Alt+K** opens the Company menu | Ctrl+K logged in to Control Centre. Alt+G set display language. Alt+K set input language | [P], [S4] |
| Alt+F12 is the range filter | **Ctrl+F** in TallyPrime 3.0 and later. Alt+F12 was used up to TallyPrime 2.1 | Alt+F12 | [P] |
| Ctrl+F12 is the value filter | **Ctrl+F then F7** in TallyPrime 3.0 and later. Ctrl+F12 was used up to 2.1 | Ctrl+F12 | [P] |
| Alt+F5 toggles detailed/condensed | Correct. **Alt+F1 also works** | Alt+F1 | [P], [S2] |
| D on the Gateway opens Day Book | **K** opens Day Book. D opens *Display More Reports* | D (Display) then D (Day Book) | [S8] for Display More Reports; K is `[UNVERIFIED]` |
| Alt+F1 closes the company | **Ctrl+F3** shuts loaded companies. In TallyPrime, Alt+F1 means detailed view | Alt+F1 closed the company | [P] |

---

## 1. Global keys (work across most screens)

| key | context | action | notes |
|---|---|---|---|
| Ctrl+A | global | Accept or save the current screen (voucher, master or config sub-screen) | [P]. Same in ERP 9 |
| Esc | global | Close the current screen and go back. In a field, clears the typed or selected input | [P]. ERP 9: "indicate you do not want to accept a voucher or master" [S6]. With unsaved data, Tally asks for confirmation to quit `[UNVERIFIED]` exact prompt |
| Ctrl+Q | global (bottom bar) | Exit a screen, or the application at the Gateway | [P]. ERP 9: exit a screen without saving [S4] |
| Alt+F4 | global | Quit the application | [P]. ERP 9: none (Alt+F4 there disconnected Tally.NET at the Gateway [S4]) |
| F1 | top menu | Open the Help menu | [P]. ERP 9 F1 at the Gateway selected and loaded a company [S4] |
| Ctrl+F1 | global | Context-sensitive help for the current screen | [P]. ERP 9: Alt+H |
| F2 | global (right button) | Change the voucher date, or the report date or period | [P] |
| Alt+F2 | global (right button) | Change the report period. Also changes the company period | [P] |
| F3 | global (right button) | Switch to another open company | [P] |
| Alt+F3 | global (right button) | Select and open another company, including from other data paths | [P] |
| Ctrl+F3 | global (right button) | Shut the currently loaded companies | [P]. ERP 9: Alt+F1 |
| F11 | global (top menu) | Company Features (enable modules such as GST, TDS, inventory) | [P] |
| F12 | global (right button) | Configuration list for the current report or screen | [P] |
| Alt+G | global (top menu) | **Go To**: type-to-search launcher for reports, masters and vouchers. Opens over the current screen. Esc returns to the previous screen | [P], [S2]. New in TallyPrime 1.0. ERP 9: none |
| Ctrl+G | global (top menu) | **Switch To**: move to a different report or task, replacing the current flow | [P], [S3] |
| Alt+K | global (top menu) | Company menu (create, alter, select, shut company, and so on) | [P]. ERP 9: none |
| Alt+Y | global (top menu) | Data menu (backup, restore, repair, migrate, split) | [P] |
| Alt+Z | global (top menu) | Exchange menu (send or receive data, synchronise) | [P] |
| Alt+O | global (top menu) | Import menu (masters, transactions, bank statements) | [P]. ERP 9 Alt+O uploaded to a website or synced [S4] |
| Alt+E | global (top menu) | Export menu | [P] |
| Ctrl+E | global | Export the current voucher or report | [P]. ERP 9: Alt+E |
| Alt+M | global (top menu) | Share menu (e-mail or WhatsApp) | [P] |
| Ctrl+M | global | E-mail the current voucher or report | [P]. ERP 9: Alt+M |
| Ctrl+Alt+W | global | Share the current voucher or report on WhatsApp | [P]. Arrived with WhatsApp in TallyPrime 4.0 (inferred from [S3]) |
| Alt+P | global (top menu) | Print menu | [P] |
| Ctrl+P | global | Print the current voucher or report | [P]. ERP 9: Alt+P |
| Ctrl+K | global (top menu) | Display language | [P]. ERP 9: Alt+G |
| Ctrl+W | global (top menu) | Data-entry language | [P]. ERP 9: Alt+K |
| Ctrl+Alt+N | top menu | Notifications report | [P]. Bell icon added in 5.0 (inferred from [S3]) |
| Alt+F | global | Find bar: search user-entered text in masters and transactions | [P]. Related to SmartFind in 7.0 [S3] |
| Ctrl+N | global | Open or hide the calculator panel | [P]. ERP 9: Ctrl+N opens it, Ctrl+M hides it |
| Ctrl+Alt+R | global | Rewrite (re-index) company data | [P] |
| Ctrl+Alt+B | global | Build information | [P] |
| Ctrl+Alt+T | global | TDL and add-on details | [P] |
| Alt+Enter | table | Expand or collapse a group in a table | [P] |
| Alt+T | table | Hide or show details in a table | [P] |

## 2. Navigation and field behaviour

| key | context | action | notes |
|---|---|---|---|
| Enter | field | Accept the field value and move to the **next field** | [S6] ERP 9: "accept anything you type into a field". TallyPrime keeps the same keyboard-first model [S3] |
| Enter | menu or list | Select the highlighted menu item or list entry | [S6] |
| Enter | report line | Drill down to the next level (opens a new screen). At the last level, opens the voucher or master | [P], [S2] |
| Enter | last field of voucher or master | Accept the screen (same effect as Ctrl+A). Tally may ask for confirmation, depending on settings | `[UNVERIFIED]` confirmation behaviour |
| Tab / Shift+Tab | field | Next or previous input field | [P] |
| Down arrow | field or list | Next field, or one line down in a list | [P] |
| Up arrow | field or list | Previous field, or one line up in a list | [P] |
| Left / Right arrow | field, columns or menus | Move one character in text, move between columns, or move between top menus | [P] |
| Backspace | field | Remove typed characters | [P] ("To remove the value typed") |
| Backspace | empty field | Move back to the **previous field** so it can be edited | `[UNVERIFIED]`. This is long-standing Tally behaviour, but neither the official ERP 9 nor the TallyPrime shortcut table states it. Up arrow / Shift+Tab is the documented way back |
| Home / End | text field | Go to the start or end of the text | [P] |
| Home or PgUp / End or PgDn | list | Go to the first or last line of the list | [P] |
| Ctrl+Home / Ctrl+End | screen | Go to the first or last field or line | [P] |
| Ctrl+Up / Ctrl+Down | menu | First or last menu item in a section. In the Dashboard, moves up or down | [P] |
| Ctrl+Left / Ctrl+Right | top menu | Left-most or right-most drop-down top menu | [P]. ERP 9: none |
| Page Up / Page Down | voucher or master entry | Open the previous or next saved voucher or master | [P]. ERP 9 same [S6] |
| Page Up / Page Down | report | Scroll up or down | [P] |
| + / - | report or voucher | Next or previous item in context. In reports, moves the date or report in the sequence forward or back | [P] |
| (typing) | list picker | **Type-to-filter.** Lists are "selection tables" that filter as you type, can be expanded and collapsed, and allow on-the-spot creation | [S3] (Release 1.0, "List Experience"). Go To works the same way: "type a report name and search" [S2]. Matching rules (prefix, substring or fuzzy) are `[UNVERIFIED]`. SmartFind in 7.0 adds misspelling tolerance [S3] |
| Alt+C | list field in voucher or master | **Create a master on the fly** without leaving the screen (for example a new ledger or stock item while entering a voucher) | [P], [S5], [S8] |
| Alt+C | amount field | Open the calculator panel | [P]. Alt+C means *New Column* in reports, so behaviour depends on context [S5] |
| Ctrl+Enter | list field in voucher or master, or a report line | **Alter the selected master** in place (for example edit the ledger picked in the field), then return | [P], [S5], [S6] |
| Ctrl+C / Ctrl+V | text field | Copy and paste field text | [P]. ERP 9 (and still in TallyPrime): Ctrl+Alt+C / Ctrl+Alt+V |
| Ctrl+4 / Alt+4 | text field | Insert the base-currency or ₹ symbol | [P]. ERP 9: Ctrl+4 |
| Ctrl+Shift+6 / Ctrl+Shift+7 | text field | Insert the AED or SAR symbol | [P]. 7.0 [S3] |
| Spacebar | report line | Select or deselect the line | [P] |
| Spacebar | text field (ERP 9) | Clear text in the field | [S6]. ERP 9 only. Not listed for TallyPrime |
| Spacebar | Yes/No field | Toggle the value | `[UNVERIFIED]`. Typing Y or N is the usual way. Confirm in a live install |

## 3. Voucher type hot-keys (open a voucher-entry screen, or switch voucher type inside one)

In TallyPrime these work from the Gateway, from inside any voucher screen (they switch type), and through F10 for the full list [P]. The location column on the primary page is "F10 > Accounting/Inventory/Order/Payroll Vouchers".

| key | context | action | notes |
|---|---|---|---|
| F4 | voucher | Contra | [P]. Same in ERP 9 |
| F5 | voucher | Payment | [P]. Same in ERP 9 |
| F6 | voucher | Receipt | [P]. Same in ERP 9 |
| F7 | voucher | Journal | [P]. Same in ERP 9 |
| F8 | voucher | Sales | [P]. Same in ERP 9 |
| F9 | voucher | Purchase | [P]. Same in ERP 9 |
| Alt+F5 | voucher | **Debit Note** | [P]. ERP 9: Ctrl+F9 |
| Alt+F6 | voucher | **Credit Note** | [P]. ERP 9: Ctrl+F8. ERP 9 Alt+F6 opened Rejection Out |
| Ctrl+F8 | voucher | **Sales Order** | [P]. ERP 9: none (order vouchers were reached with Ctrl+F2) |
| Ctrl+F9 | voucher | **Purchase Order** | [P]. ERP 9: none |
| Alt+F7 | voucher | Stock Journal | [P]. Same in ERP 9 |
| Ctrl+F7 | voucher | Physical Stock | [P]. ERP 9: Alt+F10 (ERP 9 Ctrl+F7 opened Indent [S4]) |
| Alt+F8 | voucher | Delivery Note | [P]. Same in ERP 9 |
| Alt+F9 | voucher | Receipt Note | [P]. Same in ERP 9 |
| Ctrl+F6 | voucher | Rejection In | [P]. Same in ERP 9 |
| Ctrl+F5 | voucher | Rejection Out | [P]. ERP 9: Alt+F6 |
| Ctrl+F4 | voucher | Payroll | [P]. ERP 9: none |
| F10 | voucher or master | List of all voucher types (Other Vouchers), or all masters on a master screen | [P]. ERP 9 F10 opened Reversing Journal and Ctrl+F10 opened Memo [S4] |

## 4. Voucher and master entry keys

| key | context | action | notes |
|---|---|---|---|
| Ctrl+A | voucher or master | Save | [P] |
| Alt+D | voucher or master | **Delete** the voucher or master, after confirmation | [P] |
| Alt+X | voucher | **Cancel** the voucher. The number is kept and the voucher is marked cancelled with zero values | [P]. Cancellation keeps the number sequence intact, which matters for GST serial numbering (see india-compliance.md) |
| Ctrl+D | voucher line | Remove an item or ledger line | [P] |
| Ctrl+H | voucher | **Change Mode**: Item Invoice, Accounting Invoice or As Voucher (double-entry) | [P], [S8]. ERP 9: Ctrl+V (voucher/invoice) and Alt+I (item/accounting invoice) |
| Ctrl+I | voucher or master | **More Details**: open optional fields for this instance only | [P]. New in TallyPrime 1.0 [S3]. ERP 9: none |
| Ctrl+T | voucher | Mark as post-dated | [P] |
| Ctrl+L | voucher | Mark as optional (memo-like, does not affect books) | [P]. In **reports**, Ctrl+L means *Save View* (2.0 and later) [S2] |
| Alt+J | voucher | Stat adjustment details (GST/TDS) | [P] |
| Ctrl+F | stat voucher | Autofill details in statutory vouchers (for example GST or TDS payment) | [P]. ERP 9: Ctrl+A. In the Accounting Voucher screen, Ctrl+F also autofills using "Docs by Ira" |
| Alt+S | voucher (item line) | Stock Query for the selected item | [P]. ERP 9 Alt+S also opened Stat Payment (Payment voucher) and Stat Adjustment (Journal) [S4] |
| Alt+A | sales or purchase voucher | GST Tax Analysis | [P] |
| Alt+R | voucher narration | Retrieve the narration from the previous voucher for the **same party or ledger** | [P] |
| Ctrl+R | voucher narration | Retrieve the narration from the previous voucher of the **same voucher type** | [P] |
| Alt+V | journal voucher, Quantity field | Open a Manufacturing Journal sub-screen | [P] |
| Ctrl+O | voucher | Related Reports while entering a voucher | [P] |
| Ctrl+Alt+U | voucher number field | Voucher No. Details: pick an unused voucher number | [P] |
| Alt+Q | voucher or master alteration | **Edit Log** report (version history) for this voucher or master | [P]. Supports the audit-trail requirement (see india-compliance.md). Edit Log arrived in Release 2.1 [S3] |
| Alt+L | ledger creation or alteration | Fetch party details using GSTIN/UIN (Connected GST) | [P]. 5.0 and later (Connected GST [S3]) |
| Alt+R | Company Creation | Open Group Company creation | [P] |

### ERP 9-only master hot-keys (not in TallyPrime)

On ERP 9 master creation and alteration screens, these jumped between master types [S4]. TallyPrime uses Go To or the Chart of Accounts instead.

| key | context | action | notes |
|---|---|---|---|
| Ctrl+G | ERP 9 master screen | Group | [S4] |
| Ctrl+L | ERP 9 master screen | Ledger | [S4] |
| Ctrl+C | ERP 9 master screen | Cost Centre (in inventory masters: Stock Category) | [S4] |
| Ctrl+I | ERP 9 inventory master screen | Stock Item | [S4] |
| Ctrl+O | ERP 9 inventory master screen | Godown | [S4] |
| Ctrl+U | ERP 9 master screen | Unit of Measure | [S4] |
| Ctrl+B | ERP 9 master screen | Budget | [S4] |
| Ctrl+V | ERP 9 master screen | Voucher Type | [S4] |
| Alt+F1 / Ctrl+F2 / Ctrl+F1 | ERP 9 voucher screen | Switch to the Inventory, Order or Payroll voucher group | [S4] |
| Ctrl+J / Ctrl+W | ERP 9 inventory voucher | Material Out / Material In | [S4] |

## 5. Report keys

### 5a. Right-button bar (consistent across reports)

TallyPrime 1.0 made F2 to F10 work "consistently across reports" [S3]. **F2 sets the date, F3 the company and F4 the context.** F5 to F10 change their meaning from report to report [S2].

| key | context | action | notes |
|---|---|---|---|
| F2 | report | Change the date or period | [S2] |
| Alt+F2 | report | Change the period (from and to dates) | [P] |
| F3 | report | Change the company | [S2] |
| F4 | report | **Change context.** The label depends on the report. Day Book: *Voucher Type*. Stock Summary: *Stock Group*. Ledger Vouchers: *Ledger*. Banking Activities: *Bank ledger* | [S2], [S7], [P] |
| F5 | report | First alternate view. Examples: Stock Summary *Stock Item-wise*; GSTR-1/3B *Nature View / Return View*; Bank Reconciliation *Reconciled*; Track GST *Return-wise / Period-wise* | [S2], [P] |
| F6 | report | Second alternate view. Examples: Stock Summary *Monthly*; Bills Payable *Bills Receivable*; GSTR-1 *Validate GSTIN*; imported bank data *Monthly* (Alt+F6 *Quarterly*) | [S2], [P] |
| F7 to F10 | report | Report-specific actions. Examples: GSTR-1 F7 *Validate HSN*; F8 *Validate HSN summary*; F10 *Mark as filed* (Alt+F10 undoes it); IMS F6 to F9 view switches | [P] |
| Alt+F1 / Alt+F5 | report | Toggle **detailed or condensed** (expand all lines one level) | [P], [S2]. ERP 9: Alt+F1 |
| Shift+Enter | report line | Expand or collapse **this line** in place | [P], [S2] |
| Ctrl+B | report | **Basis of Values**: change how values are computed or shown (for example accounting entries only, forex only, scale factor) | [P], [S2]. ERP 9: none (spread across F12) |
| Ctrl+H | report | **Change View**: alternate views or related reports without losing context (for example Stock Summary to Monthly or Ageing) | [P], [S2]. ERP 9: F7/F8/F9 or Alt+T depending on the report |
| Ctrl+J | report | **Exception Reports** (for example optional, post-dated or negative ledgers) | [P], [S2]. Source conflict: the Day Book help says "Ctrl+E (Exception Reports)" [S7], but Ctrl+E is Export everywhere else. Treat **Ctrl+J** as canonical |
| Ctrl+F | report | **Filter** by a range of conditions. 3.0 and later | [P]. 2.1 and earlier, and ERP 9: Alt+F12 |
| Ctrl+F then F7 | report | Recalculate balances using only the vouchers that match the filter | [P]. 2.1 and earlier, and ERP 9: Ctrl+F12 |
| Ctrl+Alt+F | report | Filter Details: show which field matched the filter. 7.0 and later | [P]. 6.2 and earlier: Alt+F |
| Alt+C | report | Add a **New Column** (comparison by period, company and so on). Not available in Day Book or GST returns | [P], [S2] |
| Alt+A | report | Alter a column | [P] |
| Alt+D | report | Delete a column | [P] |
| Alt+N | report | **Auto Repeat Columns** (for example monthly columns) | [P]. Source conflict: S2's summary list says "Alt+N – new column, Alt+O – auto columns", but the main table and S2's own example both use Alt+C for New Column. Treat [P] as canonical |
| Ctrl+L | report | **Save View**: save the report configuration under a name, which then appears in Go To. 2.0 and later | [S2] |
| F12 | report | Report configuration (for example vertical Balance Sheet) | [S2] |
| Ctrl+Q | report | Quit the report | [S2] |

### 5b. Bottom-bar or line-level actions in reports

The bottom bar exists from Release 2.0.1 [S2].

| key | context | action | notes |
|---|---|---|---|
| Enter | report line | Drill down | [P] |
| Ctrl+Enter | report line | Open the voucher for display, or alter the master | [P]. ERP 9: Enter |
| Spacebar | report line | Select or deselect a line | [P] |
| Shift+Spacebar | report line | Select or deselect a line | [P] |
| Shift+Spacebar+PgUp/PgDn | report | Select or deselect continuously | [P]. ERP 9: Ctrl+Spacebar |
| Ctrl+Spacebar | report | Select or deselect all lines | [S2]. ERP 9: Ctrl+Alt+A selected all lines [S5] |
| Shift+Up/Down | report | Linear multi-select | [P]. ERP 9: none |
| Ctrl+Shift+End / Ctrl+Shift+Home | report | Select to the end or to the top | [P] |
| Ctrl+Alt+I | report | Invert the selection | [P]. ERP 9 Ctrl+Alt+I imported statutory masters [S5] |
| Alt+A | report (Day Book, registers) | **Add** a voucher (same type, after the current line) | [P] |
| Alt+I | report | **Insert** a voucher before the current line | [P] |
| Alt+2 | report | **Duplicate** the selected voucher as a new entry | [P] |
| Alt+D | report | Delete the selected entries | [P] |
| Alt+X | report | Cancel the selected voucher | [P] |
| Ctrl+R | report | Remove (hide) the line | [P]. ERP 9: Alt+R |
| Ctrl+U | report | Restore the last hidden line (pressing again restores in reverse order) | [P]. ERP 9: Alt+U |
| Alt+U | report | Restore all hidden lines | [P]. ERP 9: Ctrl+U. **The ERP 9 and TallyPrime keys are swapped** |

### 5c. Statutory, banking and other screens (subset relevant to the India MVP)

| key | context | action | notes |
|---|---|---|---|
| F5 | GSTR-1 / GSTR-3B | Toggle Nature View and Return View | [P] |
| F6 | GSTR-1 | Validate party GSTIN/UIN | [P] |
| F7 | GSTR-1 | Validate HSN/SAC | [P] |
| F8 | GSTR-1 > Validate HSN summary | Validate HSN voucher-wise or master-wise | [P] |
| F10 / Alt+F10 | GSTR-1 / GSTR-3B | Mark as filed / undo filing | [P] |
| Alt+J | GSTR-1 / GSTR-3B | Record a stat adjustment | [P] |
| Alt+V | GST reports | Open the GST portal | [P]. ERP 9: Ctrl+O |
| Alt+S | GSTR-2A / 2B | Set the GST status of a matched line | [P] |
| F6 to F9, F10, Alt+Q | IMS reports | Switch views (Default, Reconciliation, Action, Books), set the IMS action status, show records only in books | [P]. 6.1 and later (IMS [S3]) |
| S / X / I | e-Invoice or e-Way Bill screens | **S**: send online. **X**: export for offline generation. **I**: cancel the e-invoice or e-way bill together with the voucher | [P]. Single-letter inline buttons |
| F8 / F9 / Alt+R / Alt+F7 | Bank Reconciliation | Reconcile, set status, reconcile manually, create a consolidated voucher | [P]. 6.0 and later (Connected Banking [S3]) |
| Alt+A / Alt+D / Alt+V / Alt+C | Dashboard | Add, remove, expand or configure a tile | [P]. 4.0 and later [S3] |

## 6. TallyPrime main menu letters

Each Gateway item has a highlighted letter, and pressing that letter opens the item. The Gateway is grouped as Masters, Transactions/Vouchers, Utilities and Reports [S3]. ERP 9 documents the convention ("press V in the Gateway of Tally menu" [S6]). **The TallyPrime shortcut page does not list the individual letters**, so the TallyPrime letters below are `[UNVERIFIED]` unless marked otherwise. Confirm them against a live install.

| key | context | action | notes |
|---|---|---|---|
| C | gateway | Create (master): type-to-select from a list of master types, such as Ledger, Group or Stock Item | `[UNVERIFIED]`. The menu item exists [S8] |
| A | gateway | Alter (master) | `[UNVERIFIED]` |
| H | gateway | Chart of Accounts (lists all masters; Ctrl+H switches to multi-master mode) | `[UNVERIFIED]` letter. The item exists [S9] |
| V | gateway | Vouchers (opens the last-used voucher type, or Sales by default; then use F4 to F10) | `[UNVERIFIED]` for TallyPrime. ERP 9 V = Accounting Vouchers [S6]. "Gateway of Tally > Vouchers > press F8" [S8] |
| K | gateway | Day Book | `[UNVERIFIED]`. A search snippet of Tally help says "K is to open Day Book, and B for Balance Sheet", but the page could not be fetched |
| N | gateway | Banking (utilities) | `[UNVERIFIED]` |
| B | gateway | Balance Sheet | `[UNVERIFIED]` (same search snippet as K) |
| P | gateway | Profit & Loss A/c | `[UNVERIFIED]` |
| S | gateway | Stock Summary | `[UNVERIFIED]` |
| R | gateway | Ratio Analysis | `[UNVERIFIED]` |
| D | gateway | Display More Reports (drill-down tree of all reports) | `[UNVERIFIED]` letter. The item exists [S2], [S8] |
| Q | gateway | Quit | `[UNVERIFIED]` |
| (Dashboard) | gateway | Dashboard (4.0 and later) | Letter unknown |
| (ERP 9) D then D | ERP 9 gateway | Display, then Day Book | `[UNVERIFIED]`. This is where "D = Day Book" comes from |
| (ERP 9) A / I | ERP 9 gateway | Accounts Info / Inventory Info (masters) | `[UNVERIFIED]` |

**Recommendation for KhataOS:** use mnemonic single letters on the home screen, and resolve clashes by underlining the chosen letter, as Tally does. Treat Alt+G-style type-to-search as the main way to navigate. Treat menu letters as a secondary accelerator.

---

## 7. Version differences summary (Tally.ERP 9 vs TallyPrime 1.x to 7.x)

| release | keyboard-relevant change | source |
|---|---|---|
| Tally.ERP 9 (6.x, final) | Menu-tree navigation. Order vouchers via Ctrl+F2, inventory vouchers via Alt+F1. Credit Note Ctrl+F8, Debit Note Ctrl+F9. Alt+G/Alt+K language. Alt+F1 closes the company. Alt+F12/Ctrl+F12 filters. Alt+R/Alt+U/Ctrl+U for remove and restore lines. Ctrl+Alt+C/V copy and paste | [S4], [S5], [S6], [P] |
| TallyPrime 1.0 | **Re-mapped**: Go To (Alt+G) and Switch To (Ctrl+G); top menu (Alt+K/Y/Z/O/E/M/P); Ctrl+K/Ctrl+W language; Ctrl+F3 shuts the company; Alt+F5 (Alt+F1) detailed; Ctrl+B Basis of Values; Ctrl+H Change View/Mode; Ctrl+J Exceptions; Ctrl+I More Details; F10 list of all vouchers; Credit Note Alt+F6, Debit Note Alt+F5, Sales Order Ctrl+F8, Purchase Order Ctrl+F9, Physical Stock Ctrl+F7, Rejection Out Ctrl+F5, Payroll Ctrl+F4; Ctrl+R/Ctrl+U/Alt+U for remove and restore; Ctrl+C/Ctrl+V copy and paste; selection-table lists with type-to-search and create-on-the-fly | [S3], [P] |
| 2.0 | Save View (Ctrl+L in reports) | [S2] |
| 2.0.1 | Bottom bar showing context shortcuts (can be hidden: F1 > Settings > Display) | [S2], [S3] |
| 2.1 | Edit Log (Alt+Q from alteration screens). Ships as an "Edit Log" edition (always on) and a standard edition (optional) | [S3], [P] |
| 3.0 | Report filter moves from Alt+F12 to **Ctrl+F**. Filtered recompute moves from Ctrl+F12 to **Ctrl+F > F7** | [P] |
| 4.0 | Dashboard keys. WhatsApp sharing (Ctrl+Alt+W). Excel import | [S3], [P] (version of key introduction inferred) |
| 5.0 | Connected GST keys (Alt+L fetch GSTIN, upload/file S/X). Notifications (Ctrl+Alt+N). Stripe View | [S3], [P] (inferred) |
| 6.0 / 6.1 / 6.2 | Connected Banking and bank-reconciliation keys (6.0). IMS views (6.1). Edit Log Summary (6.1) | [S3], [P] |
| 7.0 (current major; 7.2 advertised) | **Filter Details** Alt+F becomes **Ctrl+Alt+F**. Import GSTIN from Excel Alt+F becomes **Alt+L**. Alt+F is now the Find bar (SmartFind). AED/SAR currency symbol keys | [P], [S3] |

> The user asked about "TallyPrime 4.x/5.x/6.x". The official page marks version splits only at 2.1/3.0 (filters) and 6.2/7.0 (Alt+F family). No other key was remapped between 4.x and 6.x. Those releases only **added** keys for new features.

---

## 8. Design notes for KhataOS (derived from the above)

1. **Context-scoped keymap.** The same chord does different things by context. For example, Alt+C means *create master* in a list field, *calculator* in an amount field and *new column* in a report. Ctrl+L means *optional voucher* in a voucher and *save view* in a report. Model the keymap as `(context, key) -> command`, not as one global map.
2. **Keyboard-only completion.** Every flow must finish without a mouse. Enter moves forward. Esc goes back or clears. Ctrl+A saves from anywhere.
3. **List picker = selection table** with incremental filter, a "create new" path (Alt+C) and an "alter selected" path (Ctrl+Enter).
4. **Voucher-type hot-keys work inside the voucher screen** and switch the type while keeping the date and context.
5. **Delete vs cancel** (Alt+D vs Alt+X) are different operations. Cancel keeps the number, which GST numbering continuity needs.
6. **Edit log (Alt+Q)** must be reachable from every alteration screen. This is a compliance requirement, not only a convenience.
7. **Offer an "ERP 9 compatibility" keymap preset** for the keys that differ (Ctrl+F8/F9 notes, Alt+F12 filter, Alt+R/U line hiding). Many long-time users still have ERP 9 muscle memory.

## Hands-on verification — TallyPrime 7.2 (EDU), 2026-10-19

Observed directly in a running install. This resolves the `[UNVERIFIED]` Gateway letters above.

| Key | TallyPrime main menu item | Section |
|---|---|---|
| C | Create | Masters |
| A | Alter | Masters |
| H | Chart of Accounts | Masters |
| V | Vouchers | Transactions |
| K | Day Book | Transactions |
| N | Banking | Utilities |
| B | Balance Sheet | Reports |
| P | Profit & Loss A/c | Reports |
| S | Stock Summary | Reports |
| R | Ratio Analysis | Reports |
| D | Display More Reports | Reports |
| O | Dashboard | Reports |
| Q | Quit | — |

Also observed:

- **Ctrl+A** accepts a form from any field and saves immediately, without asking for confirmation.
- **Esc** goes back one screen and restores the list cursor to the row chosen before.
- **Shift+Tab** moves to the previous field.
- In a popup list, **Up/Down** move through the list and wrap from the last row to the first; they do not change fields.
- **Single letters** trigger dialog buttons ("A: Accept", "T: Try It For Free").
- **List filtering** matches the typed text anywhere in the item name, not just at the start.

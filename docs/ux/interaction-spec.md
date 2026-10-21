# KhataOS interaction spec (v0.1)

How KhataOS behaves from the keyboard, so people moving from TallyPrime keep their muscle memory. These rules describe **behaviour**. Visual design, wording and branding are KhataOS's own.

> This is the target behaviour, not a list of what is built. Some rules describe features that don't exist yet (for example optional and post-dated vouchers, report filters, saved views and IFSC look-up). What works today is in the [user guide](../guide/README.md) and the [roadmap](../roadmap.md).

The rules come from:
- a hands-on behavioural study of TallyPrime 7.2 (Educational mode, 2026-10-19). Notes are kept privately, outside this repo;
- [`../research/tally-shortcuts.md`](../research/tally-shortcuts.md), which has the full key table;
- [`../research/tally-pain-points.md`](../research/tally-pain-points.md), which records the problems Tally users report.

Each rule is tagged **KEEP** (match the behaviour), **FIX** (match the behaviour but remove a known pain point) or **NEW** (not seen in the TallyPrime study). Statements below about how TallyPrime behaved come from that hands-on study of release 7.2, unless a source is linked.

The key bindings themselves are implemented in `crates/khata-keymap`.

---

## 1. Screen layout

| # | Rule | Tag |
|---|---|---|
| L1 | Four fixed zones: top bar (global search on Alt+F, then the action menu), title strip (screen name on the left, company in the centre), work area, and two button bars (right side and bottom). | KEEP |
| L2 | Every button and menu item is labelled `<Key>: <Label>`. On menu items, the hotkey letter is highlighted inside the word (Ba**N**king). | KEEP |
| L3 | Each F-key has a **fixed slot** in the right bar on every screen. Unused slots stay visible but greyed, so positions never move. | KEEP |
| L4 | On the voucher screen, the current voucher type's button is greyed. The bar doubles as a "you are here" marker. | KEEP |
| L5 | Each voucher type has its own background tint (Payment, Sales, Receipt…), so users can see at a glance what they are entering. | KEEP |
| L6 | When a window is inactive the whole UI dims. | KEEP |
| L7 | A parent screen stays visible, faded, behind a child screen (sub-forms, inline creation). | KEEP |

## 2. Moving between fields

| # | Rule | Tag |
|---|---|---|
| F1 | **Enter** accepts the field and moves to the next one. Forms flow down the left column, then down the right column. | KEEP |
| F2 | Fields whose value is already decided are skipped (choosing a State fills and skips Country). Empty optional fields pass straight through. | KEEP |
| F3 | **Shift+Tab** goes to exactly the previous field, including the alias field. **Up** on the first field does nothing. | KEEP |
| F4 | **Esc** in a field with text clears the text first. A second Esc leaves the screen. | KEEP |
| F5 | **Ctrl+A** saves the whole form from any field without asking. Pressing Enter past the last field asks "Accept? Yes / No", answered with Y or N. Answering N returns to the first field with every value kept. | KEEP |
| F6 | Esc on a form with unsaved changes asks "Quit? Yes / No". On the home screen, Esc asks to quit the app; this prompt is centred, while form prompts sit at the bottom right. | KEEP |
| F7 | After a master or voucher is saved, a new blank one of the same type opens straight away, with the last-used group or ledger carried over. | KEEP |
| F8 | Keys typed while a screen is opening are buffered, not lost. | FIX: Tally appeared to drop them (not yet confirmed) |

## 3. List picker (one component, used everywhere)

| # | Rule | Tag |
|---|---|---|
| P1 | A list field opens a panel with a header, optional column titles, **action rows pinned at the top** ("Create", "New State"…) and grouped items. Counts of hidden rows show at each end ("5 ▲", "15 ▼"). | KEEP |
| P2 | The panel sits on the right of the screen, or drops down from the field when that fits better. | KEEP |
| P3 | Typing filters on every keystroke and matches **anywhere in the text**, not only at the start. Matched characters are highlighted, and the cursor jumps to the first match. | KEEP |
| P4 | Inside a list, Up and Down move through the rows and wrap from the last row to the first. Enter selects. | KEEP |
| P5 | Yes/No fields are two-row lists. Typing Y or N selects. | KEEP |
| P6 | **Text that matches nothing never creates a record silently.** Enter on unmatched text shows a `Create "<text>"…` row that needs confirming. In Tally, a typo like "YMaha" became a permanent State with no prompt. | FIX |
| P7 | Alt+C creates a master from inside the field, with the voucher still visible behind it. When it is saved, the new value fills the field and the cursor moves on. The group list is filtered to what fits the context: a Payment's particulars line offers expense, party and liability groups, not bank groups. | FIX: Tally defaulted to the last-used group |
| P8 | Ctrl+Enter alters the selected master in place, then returns to the field. | KEEP |

## 4. Messages

| # | Rule | Tag |
|---|---|---|
| M1 | Errors appear in red-bordered panels and warnings in orange-bordered ones, at the bottom right. They don't block: any key dismisses them. | KEEP |
| M2 | **An invalid tax ID (GSTIN, TRN, VAT number) is a blocking error, not a warning.** Overriding it needs a role permission and a reason, and the override is written to the audit trail. Tally warned but still saved a GSTIN with a wrong check digit. | FIX |
| M3 | No warnings that don't apply. For example, never ask for a GSTIN on the company's own bank account. Noisy warnings train people to dismiss everything. | FIX |

## 5. Masters

| # | Rule | Tag |
|---|---|---|
| Ma1 | The ledger form's sections depend on the nature of the chosen group: bank fields (account number, IFSC, SWIFT, branch); party fields (bill-by-bill tracking, credit period, PAN, tax registration); or expense fields (tax applicability, HSN/SAC, rate). Sections change instantly when the group changes. | KEEP |
| Ma2 | The parent group's nature appears in italics under the group name ("(Current Assets)"). | KEEP |
| Ma3 | Tax attributes are inherited from **company → group → ledger**, and the form shows where each value comes from. | KEEP |
| Ma4 | Typing an IFSC code fills in the bank and branch from the public RBI IFSC dataset, offline. | NEW |
| Ma5 | Tally users expect these default groups in KhataOS's default chart: Bank Accounts, Bank OCC A/c, Bank OD A/c, Branch / Divisions, Capital Account, Cash-in-Hand, Current Assets, Current Liabilities, Deposits (Asset), Direct/Indirect Expenses and Incomes, Duties & Taxes, Fixed Assets, Investments, Loans & Advances (Asset), Loans (Liability), Misc. Expenses (Asset), Provisions, Purchase Accounts, Reserves & Surplus, Sales Accounts, Secured Loans, Stock-in-Hand, Sundry Creditors, Sundry Debtors, Suspense A/c, Unsecured Loans. Group aliases are searchable ("Expenses (Direct)" finds Direct Expenses). | KEEP |
| Ma6 | Amounts are right-aligned and shown in the locale's grouping: **2,50,000.00** for India, 250,000.00 elsewhere. Dr/Cr is a separate sub-field; its default follows the group's nature, and typing D or C sets it. | KEEP |
| Ma7 | While opening balances are being entered, a panel shows the company-wide opening total and how far it is from balancing. | KEEP |

## 6. Vouchers

| # | Rule | Tag |
|---|---|---|
| V1 | The voucher screen shows the type badge, an automatic number (one series per type), and the date and weekday on the right. | KEEP |
| V2 | The F4–F10 keys (and the Alt and Ctrl variants listed in the keymap) switch voucher type in place. | KEEP |
| V3 | **Balances are projected live before saving.** The header account shows its balance as it will be after this voucher, each line shows its ledger's balance, and the running total sits at the bottom right. | KEEP |
| V4 | A bank ledger on a voucher always opens a bank-details sub-screen, which can't be skipped. It records the instrument type, cheque range, instrument number and instrument date, for bank reconciliation. KhataOS adds UPI, NEFT, RTGS and IMPS reference numbers. | KEEP + NEW |
| V5 | In a sub-screen, Backspace in the first field closes it and returns to the parent. | KEEP |
| V6 | Optional (memo) vouchers, post-dated vouchers, "fill an unused number" and changing the entry mode (single entry, double entry or invoice) are all first-class. | KEEP |
| V7 | Cancelling a voucher keeps its number and zeroes its values (GST invoice serials must have no gaps). Deleting a voucher is logged with a full snapshot. | KEEP |
| V8 | The party picker on Sales or Purchase only offers eligible ledgers: cash, bank and party groups. | KEEP |

## 7. Reports

| # | Rule | Tag |
|---|---|---|
| R1 | Every report row drills down with Enter, and the last level opens the voucher. | KEEP |
| R2 | Users can act on report rows directly: alter, add after, insert before, duplicate, delete, cancel, hide or restore a line, and select one or many rows with Space or Shift+arrows. | KEEP |
| R3 | The right bar holds the report keys: F2 date, F4 context (such as voucher type), F5/F6 alternate views, Basis of Values, Change View, Exception Reports, Filter and Save View. | KEEP |
| R4 | Saved views are stored per user and appear in Go To (Alt+G). | KEEP |
| R5 | Day Book columns: date, particulars (the other ledger, in bold), voucher type, voucher number, then debit over inwards quantity and credit over outwards quantity. | KEEP |

## 8. Compliance built into the UX

| # | Rule | Tag |
|---|---|---|
| C1 | **One build. The audit trail is always on and can't be switched off.** Tally ships separate builds with and without its edit log ([Edit Log FAQ](https://help.tallysolutions.com/edit-log-in-tallyprime-faq/)). KhataOS doesn't: India requires the edit log for companies (Companies Act Rule 3(1)) and for all GST registrants (GST Rule 56(8)). | FIX |
| C2 | On first run, the user picks the **country pack**, which sets the financial-year start, currency, phone prefix, state list and the tax regimes that must be on (GST for India). Advanced switches sit behind "show more" / "show all". | KEEP |
| C3 | A company can hold several tax registrations, one per state. | KEEP |

---

## Still to study

Keep this open list in step with the private study log:

- The Balance Sheet, P&L, Trial Balance and ledger-vouchers reports, including drill-down depth and the effect of each F-key.
- GSTR-1, GSTR-3B and the GSTR-2B reconciliation screens, with e-invoice and e-way bill generation.
- Sales item invoices: item lines, batches, GST tax analysis (Alt+A) and invoice printing.
- Banking: bank reconciliation, cheque printing and bank statement import.
- The Go To launcher (Alt+G) ranking, and global search (Alt+F).
- Backup, restore and repair under Alt+Y, together with the data-corruption pain points.
- Whether keys typed during screen transitions are buffered (rule F8).
- How button labels distinguish Ctrl from Alt chords visually (for example "A: Accept" is Ctrl+A, while "D: Delete" is Alt+D).

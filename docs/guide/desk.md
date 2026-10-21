# Desk

**What you'll do:** use the accountant's desk to keep to-dos and notes, see your filing due dates on a calendar, and mark returns as filed.

The Desk is a KhataOS addition. It has no Tally equivalent.

## Open the Desk

On the home menu, press `T` (To-dos and Due Dates). Or press `Alt+G` and type "desk".

The home screen's **Due soon** line counts what's due in the next 7 days and what's overdue. It is highlighted when anything is overdue.

## What's on it

The left side has lists:

- **Overdue**, **Today**, **Next 7 days** and **Next 90 days**;
- **To-dos and notes**.

Inside each list, items are grouped as tax returns, payments, company filings, audit, reminders and to-dos.

The right side has a month calendar, with dots for the days that have items, and the selected day's items.

Items come from three places:

1. **Due dates** for returns, tax payments and company filings, worked out from your country's rules and your compliance profile.
2. **Reminders from the books**: bills falling due, vouchers dated after today, and (India) supplier bills near the 45-day MSME limit. Saudi companies also see documents waiting in the ZATCA queue.
3. **Your own to-dos and notes.**

> Due dates are estimates from the published rules. Governments extend deadlines, and holidays aren't counted. Check the official portal before you file.

## Set up the compliance profile

The Desk needs a few facts to know which due dates apply. See [Your first company](first-company.md#5-set-up-the-compliance-profile-on-the-desk).

1. On the Desk, press `F12` (Profile), or `Enter` on **Set up your compliance profile**.
2. Answer the questions and accept.

Saving the profile needs an Accountant.

## Add a to-do

1. Press `Alt+C` (New To-do). In the calendar, the to-do is dated on the selected day.
2. Type the **Title**, any **Details**, and **Due on** (leave blank for no date).
3. Accept.

## Add a note on a voucher or ledger

1. Open a saved voucher, or put the cursor on a row in the Day Book, Trial Balance, a ledger statement, Outstanding, Ageing, MSME or Stock Summary.
2. Press `Alt+W` (Note).
3. Type the title, details and due date, and accept.

The note shows on that record and on the Desk. `Enter` on the note in the Desk opens the voucher or ledger.

## Work through the list

| Key | What it does |
|---|---|
| `↑` `↓` | Move (by week in the calendar) |
| `←` `→` | Move by day in the calendar |
| `PgUp` `PgDn` | Ten rows in a list; a month in the calendar |
| `Home` | Top of the list; today in the calendar |
| `Tab` | Switch between the lists and the calendar |
| `Enter` | Open: a return opens its report for that period, a note opens its voucher or ledger, a bill opens the party's statement |
| `Space` | Mark a to-do done, or a return filed (press again to undo) |
| `Ctrl+Enter` | Alter a note |
| `Alt+D` | Delete a note |
| `Ctrl+F` | Show one kind of item at a time; press again for the next kind |
| `F12` | The compliance profile |
| `Esc` | Back |

`Enter` on a return opens its report for GSTR-1, GSTR-3B, TDS and TCS (India), VAT 201 (UAE), the KSA VAT return, the UK VAT return and GST F5 (Singapore). For other returns it shows the rule and its source.

When you mark a return filed, keep the acknowledgement from the portal: KhataOS doesn't file it for you.

## Good to know

- **Notes, to-dos, filed marks and the profile stay in this company file on this computer.** They don't sync to your co-workers or your accountant yet, and they aren't in accountant files (`.khatapkg`). They do travel with a copy of the company file.
- **There are no pop-up reminders** or system notifications. Check the Desk and the home screen's Due soon line. `Ctrl+Alt+N` (notifications) isn't built yet.
- Reading the Desk needs any sign-in. Adding notes and marking items needs Data Entry or above. Deleting someone else's note needs an Accountant.

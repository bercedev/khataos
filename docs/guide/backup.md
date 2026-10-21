# Backup

**What you'll do:** back up a company, bring a backup back, and set up automatic backups.

Everything is in the **Data** menu: press `Alt+Y`. Type the first letter of a row and press `Enter`: `B` for Backup, `R` for Restore, `A` for Automatic backups.

## Back up now

1. Open the company and press `Alt+Y`, then `B` and `Enter`.
2. Choose:
   - **Back up now** (`B`): a backup in the company's backup folder.
   - **Encrypted with a password** (`E`): the same, sealed with a password you type twice. Nobody can open the backup without it, and KhataOS doesn't keep it, so write it down somewhere safe.
   - **Folder: choose another** (`F`): pick a different folder, such as a USB drive or a synced cloud folder. KhataOS remembers it for this company on this computer.
3. KhataOS makes the backup, then checks it before saying it worked. The message shows the file, its size and the folder.

The backup is taken while the company stays open. You don't need to shut it first.

**The folder.** By default backups go to `Documents/KhataOS Backups/<company name>/`. Each file is named `<company>_<YYYY-MM-DD_HHMM>.khatabak`, for example `sharma-sons_2026-10-21_1430.khatabak`. Automatic backups end in `_auto.khatabak`.

Keep copies in more than one place: the backup folder on this computer is no help if the disk fails. Point the folder at a USB drive or a cloud folder, or copy the files there now and then.

## What "checked" means

A backup is reported as saved only after KhataOS has:

1. taken a consistent snapshot of the open company (SQLite's `VACUUM INTO`, never a plain copy of a file in use);
2. written it to the folder;
3. opened the written file read-only (decrypting it if you gave a password), replayed the whole audit trail and verified the hash of every entry;
4. compared the number of entries and the last hash with the books it came from.

If any step fails, nothing is left in the folder and you see why.

## Restore a backup

1. Press `Alt+Y`, then `R` and `Enter`. This works with no company open too, for example on a new computer or after reinstalling.
2. Choose the `.khatabak` file. For an encrypted backup, type its password.
3. KhataOS checks the backup first (the same checks as above) and shows what it holds: the company, when it was backed up, the number of vouchers and their date range, the last voucher date, the audit trail entries, and the notes and settings.
4. Choose how to restore:
   - **Restore as a new company file** (the default, and the safest). The backup becomes a new file, `<company>-restored.khata`, in the companies folder, and KhataOS opens it. Nothing is overwritten. You can compare it with the current books and keep whichever you need.
   - **Replace the open company.** Only the Owner can do this, and only with a backup of the same company. Type the company name to confirm. KhataOS then:
     1. makes an automatic backup of the current books;
     2. renames the current file to `<name>.before-restore-<date_time>.khata` beside it (it is never deleted or overwritten);
     3. puts the backup in its place and opens it.

     If the company is open in another KhataOS window or user account, Replace is refused: close it there first.

If the backup is older than the open books, KhataOS says how many later entries are not in it. Those entries are still in the file kept as `.before-restore-…khata` and in the automatic backup taken first.

A corrupted backup (a damaged disk, a file cut short in a download, a wrong password) is refused with a message saying what is wrong. Nothing is changed.

## Automatic backups

Press `Alt+Y`, then `A` and `Enter`. Move with `↑` `↓` and press `Enter` to change a row. Changes are saved at once.

| Setting | Default | What it does |
|---|---|---|
| Automatic backups | On | Switches the two below on or off. |
| When the company closes | On | A backup each time the company closes: `Ctrl+F3`, switching company, quitting, or closing the window. |
| Once a day while it is open | On | A backup when the last one is a day old. |
| Automatic backups to keep | 10 | Older automatic backups in the folder are deleted. |
| Folder | `Documents/KhataOS Backups/<company>/` | Where backups go. Shared with Backup now. |

Automatic backups run in the background: you can keep working. They are skipped when nothing changed since the last backup, so an unchanged company doesn't fill the folder with copies.

**What is deleted.** Only files in that company's backup folder whose names are exactly `<company>_<YYYY-MM-DD_HHMM>_auto.khatabak` (or with `-2`, `-3` for a second one in the same minute). Your own backups, other companies' backups and any other file are never touched.

**Before risky actions** KhataOS always takes an automatic backup first, even with automatic backups off:

- importing from Tally into a company that already has books;
- restoring with Replace;
- a sync rebase (after a conflict);
- moving the company to another relay.

If that backup can't be made (the folder is full or missing), the action doesn't start.

**Reminders.** The home screen shows **Last backup**. If there has been none for 7 days and the books changed since, it shows in orange, and the [Desk](desk.md) shows a reminder under Today. Press `Enter` on it to back up.

## What is in a backup

In:

- the company's books: every master, voucher, user, setting of the books and the full hash-chained audit trail;
- the shared records: desk notes and to-dos, filed marks, the compliance calendar, the invoice design, logo and signature, voucher print details, tax settings such as UK VAT schemes, and what tax authorities answered (IRNs, CFDI stamps, MTD receipts).

Not in:

- **sync and relay credentials:** the sync token, the relay's end-to-end key and the server addresses (`sync.*`, `share.*`). A restored company is not connected: set up sharing again, or join with an invite.
- **anything else that belongs to one computer** (see `docs/sync-settings.md`): sync cursors, accountant-file cursors, a device's ZATCA e-invoicing unit and VeriFactu chain, the TDS file counter. A restored company counts as a new device for ZATCA (Saudi Arabia): onboard it again before you submit invoices.
- **online credentials** for e-invoicing and tax portals (India IRP, ZATCA keys, HMRC MTD, the Mexican PAC, Peppol access points). These were never in the company file; they stay in KhataOS's data folder on that computer.
- the backup settings themselves, which belong to each computer.

## Phones (Android and iPhone)

On a phone, company files live inside the app. **Uninstalling KhataOS deletes them**, and the automatic backups too, because they are also kept inside the app.

- **Backup** on a phone opens the system's save dialog: save the backup to Downloads, a cloud drive or a USB stick, or share it. Do this before you uninstall or change phones.
- **Restore** opens the system's file picker: choose the `.khatabak` file wherever you saved it.

## The file format

A plain `.khatabak` file is an SQLite company file with one extra table that records when it was made and a checksum. If ever needed, a copy renamed to `.khata` opens in KhataOS like any company file. An encrypted backup is the same file sealed with your password (Argon2id and XChaCha20-Poly1305, the same as accountant files), in blocks that can't be removed, reordered or cut off without it being noticed. Not even the company name is readable without the password.

## Other ways your books are kept safe

- **Sharing keeps other copies.** Every computer that shares the company (office server, relay, accountant) has a full copy of the books. That helps, but it isn't a backup plan.
- **Accountant files** (`.khatapkg`, `Alt+M`, **Send to accountant**) are an encrypted copy of the entries and shared records, meant for sending, not for keeping.
- **Files named `.conflict-<time>.khata`** are copies KhataOS kept, unchanged, when it rebased your offline changes after a sync conflict. Files named `.before-restore-<time>.khata` are the books as they were before a Replace restore. Both show in the company list with the same company name. Keep them until you've checked.
- **One KhataOS at a time per company file.** While a company is open, KhataOS keeps a small `<company>.khata.lock` file next to it so another KhataOS window or user account can't open the same file. It doesn't need backing up, and a leftover one (after a crash) doesn't stop the company opening.
- **The audit trail is checked every time the company opens.** A damaged file shows up straight away, instead of quietly giving wrong figures.

# Backup

**What you'll do:** find your company file and copy it somewhere safe, and bring a copy back if you need it.

> **There is no backup or restore command in KhataOS yet.** `Alt+Y` (the Data menu) isn't built. Until it is, back up by copying the company file yourself, as below. Scheduled backups are on the [roadmap](../roadmap.md).

## What to back up

Each company is **one file** ending in `.khata`. It holds everything: masters, vouchers, the full audit trail, users, and this company's settings (invoice design, desk notes, sync settings).

The home screen shows the file's location under **Company file**. By default company files are in KhataOS's data folder:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\org.khataos.desktop\companies\` |
| macOS | `~/Library/Application Support/org.khataos.desktop/companies/` |
| Linux | `~/.local/share/org.khataos.desktop/companies/` |

A company you created elsewhere is wherever you put it.

Not in the company file, so not in its copy: the online credentials for e-invoicing and tax portals (India IRP, ZATCA, HMRC MTD, the Mexican PAC, Peppol access points), which stay in KhataOS's data folder on that computer.

## Make a backup

1. In KhataOS, press `Ctrl+F3` to shut the company (or quit KhataOS).
2. Copy the `.khata` file to a USB drive, another disk or a cloud folder.
3. Put the date in the copy's name, for example `sharma-sons-2026-10-21.khata`.

Shut the company first. The file is safe to copy whenever KhataOS isn't saving, but a copy taken in the middle of a save could be incomplete.

Keep several dated copies, in more than one place.

## Bring a backup back

1. Quit KhataOS, or shut the company.
2. Copy the backup file back into place. To keep the current file too, give the backup a different name.
3. Start KhataOS, choose **Open Company File…** in the company list, and type the backup's path (or put it in the companies folder so it shows in the list).

If the company syncs with a server or relay, an old copy can be behind what was already synced, and KhataOS refuses to sync it ("was a backup restored?"). Open the latest file instead, or ask for help.

A copied company counts as a new device for ZATCA (Saudi Arabia), so onboard it again before submitting invoices.

## Other ways your books are kept safe

- **Sharing keeps other copies.** Every computer that shares the company (office server, relay, accountant) has a full copy of the books. That isn't a backup plan, but it helps.
- **Accountant files** (`.khatapkg`, `Alt+M`, **Send to accountant**, **Everything**) are an encrypted copy of all the entries. Opening one on another computer rebuilds the books. They don't carry the desk notes or invoice design.
- **Files named `.conflict-<time>.khata`** are copies KhataOS kept, unchanged, when it rebased your offline changes after a sync conflict. They show in the company list with the same company name. Keep them until you've checked the rebase.
- **One KhataOS at a time per company file.** While a company is open, KhataOS keeps a small `<company>.khata.lock` file next to it so another KhataOS window or user account can't open the same file. It doesn't need backing up, and a leftover one (after a crash) doesn't stop the company opening.
- **The audit trail is checked every time the company opens.** A damaged file shows up straight away, instead of quietly giving wrong figures.

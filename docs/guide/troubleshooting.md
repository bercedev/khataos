# Troubleshooting

**What you'll do:** send a crash report, and fix the messages you're most likely to see.

## After a crash

KhataOS saves a short crash note on your computer. Nothing is sent by itself.

1. Next time you start KhataOS, it asks "KhataOS closed unexpectedly last time. Send a report?".
2. Choose **Review & report** to see the text first. It contains the version, your system, the country pack, the language, the error and the last screens you used. Names, paths, tax IDs, email addresses and amounts are removed.
3. Choose **Open on GitHub**. The issue opens in your browser, filled in. Check it and submit it.

Choose **Don't ask again** to stop the question and stop saving crash notes on this computer.

Crash notes are in the `crashes` folder in KhataOS's data folder (the last 20 are kept):

| System | Folder |
|---|---|
| Windows | `%APPDATA%\org.khataos.desktop\crashes\` |
| macOS | `~/Library/Application Support/org.khataos.desktop/crashes/` |
| Linux | `~/.local/share/org.khataos.desktop/crashes/` |

**Report a problem** without a crash: press `Alt+G` and type "report", or find it in the Company menu (`Alt+K`) or the Language menu (`Ctrl+K`, which works with no company open). See also [Crash reporting](../crash-reporting.md).

## Common messages

| Message | What it means | What to do |
|---|---|---|
| "… isn't available in this version yet." | The key is reserved for a feature Tally users know that isn't built yet | See the [key reference](keyboard.md#key-reference) |
| "… vouchers aren't available yet." | Orders, notes, rejections, payroll and physical stock aren't built | Use the voucher types that exist ([Vouchers](vouchers.md)) |
| "Sign in first, or press Esc to go back to the company list." | The company has users and nobody has signed in | Sign in |
| "… is signed in as …, which can't do this; it needs …" | Your role isn't allowed to do this | Ask an Owner or Accountant, or sign in as one |
| "Open a company first …" | The menu needs an open company | Open one from the company list |
| "This company was shared with you as a Viewer …" | Your copy can only be read | Ask the owner for an Accountant invite |
| "GSTIN … is invalid: …" (or TRN, VAT number…) | The tax ID failed its check | Correct it, or (as an Accountant) give a reason to keep it |
| "voucher number … is already used for this voucher type" | The number is taken in this series | Check the [numbering](vouchers.md#numbering) settings |
| "invoice number … is already used in financial year …" | An invoice number must be unique in the year | As above |
| "a ledger named … already exists" | Names must be unique | Use another name, or alter the existing one |
| "ledger is used by vouchers and cannot be deleted" | A master in use can't be deleted | Delete or change those vouchers first |
| "This company is open in another KhataOS window or on another user account on this computer. Close it there first." | Another KhataOS has this company file open | Shut the company there (`Ctrl+F3`) or quit that KhataOS, then open it here |
| "a file already exists at …" | Creating a company over an existing file | Choose another file name |
| "No ledger for output CGST 9% — create one under Duties & Taxes" | A tax line has no duty ledger | Create the duty ledger ([Masters](masters.md#ledgers)) |
| "Set the company's … under Company (Alt+K) …" | The company has no tax ID, so tax isn't filled in | Add it in `Alt+K`, **Company details** |
| "Printing isn't available on phones yet …" | Phones can't print | Export instead, or print on a computer |
| "sync isn't set up for this company yet …" | `Ctrl+Alt+S` with no server set | Set up [sharing](sharing.md) or sync settings |
| "Conflict: … change(s) here, … on the server" | Both sides changed the books | Accept the rebase, then check the review screen |
| "the server refused this token; check Sync settings" | The access token is wrong or revoked | Ask for a new invite or token |
| "couldn't reach the sync server …" | No network, or the server is off | Check the connection; your entries wait on this computer |
| "this file is older than what was already synced (was a backup restored?) …" | You opened an old copy | Open the latest file ([Backup](backup.md)) |
| "wrong passphrase, or the file was changed on the way …" | Accountant file passphrase is wrong | Check the passphrase with the sender |
| "an earlier file is missing …" | An accountant file was skipped | Open the earlier file first |
| "both sides changed the books since they were last in step …" | Both you and your accountant made entries | Send "Everything", or agree who enters what |

## Other problems

**A key does nothing.** Some reserved keys have nothing behind them and give no message yet, such as `Ctrl+F` in a voucher or in most reports. Check the [key reference](keyboard.md#key-reference). On a Mac, function keys need `fn`, and macOS may take some keys first (see [Keyboard](keyboard.md#on-a-mac)).

**Outstanding and Ageing are empty after importing from Tally.** Bill-wise details don't come across in the import. See [Import from Tally](import-from-tally.md).

**The same company shows twice in the company list.** One of them is probably a `.conflict-<time>.khata` file kept after a sync rebase. See [Backup](backup.md#other-ways-your-books-are-kept-safe).

**Running KhataOS twice.** KhataOS doesn't stop you from starting it twice and opening the same company file in both. Don't: keep each company file open in one place only.

**Reports don't show new entries from colleagues.** Reports already open don't refresh by themselves. Press `Esc` and open the report again.

**A screen is still in English after switching to Arabic.** Screens already open change when you open them again; some screens aren't translated yet ([Arabic](arabic.md)).

Still stuck? [Open an issue](https://github.com/khataos/khataos/issues/new/choose).

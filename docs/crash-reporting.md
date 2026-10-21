# Crash reports

KhataOS has no crash server and uses no third-party crash service. Nothing leaves your
device unless you send it yourself.

When the app crashes, it writes a short note about the crash to a file on this device.
On the next start it asks:

> KhataOS closed unexpectedly last time. Send a report?
>
> **R**eview & report · **N**ot now · **D**on't ask again

**Review & report** shows the exact text of the report first. If you go ahead, KhataOS
opens a new GitHub issue in your browser with that text filled in. You can edit it
there, and nothing is sent until you press **Submit** on GitHub. **Not now** asks again
next time. **Don't ask again** turns crash notes off (see [Turning it off](#turning-it-off)).

You can also report a problem when nothing has crashed. **Report a problem** is in the
Company menu (Alt+K), the Language menu (Ctrl+K, which works with no company open) and
Go To (Alt+G). It opens a blank report with only the version and system details filled in.

The code is in `apps/desktop/src-tauri/src/crash.rs` (saving, scrubbing, the issue
link) and `apps/desktop/src/crash.ts` (the page's error handler and the dialogs). The
GitHub form is `.github/ISSUE_TEMPLATE/crash_report.yml`.

## What a report holds

| Field | Example | Where it comes from |
|---|---|---|
| App version | `0.1.0` | The build |
| OS and architecture | `macos aarch64` | The build target |
| Platform | `desktop`, `android` or `ios` | The build target |
| Country pack | `IN` | The open company's country code; empty if none was open |
| Display language | `en`, `ar` | Your language setting |
| The error | `called Option::unwrap() on a None value` | A Rust panic message and its `file:line:column`, or a JavaScript error message and stack |
| Screens visited | `StartupScreen > HomeScreen > VoucherScreen` | The class names of the last 12 screens opened |
| Time | `2026-10-21T10:15:00Z` | The clock, in UTC |

The screen list holds class names only, never screen titles (a title can hold a ledger
name) and never anything typed into a field.

## What a report never holds

- Company files, or any data read from them: ledgers, vouchers, parties, items, amounts,
  balances, reports.
- Company names, company file names or folders.
- Your user name, home folder, email address, phone number, tax IDs or passwords.
- Settings, keys or tokens (relay keys, HMRC, IRP, ZATCA, PAC or Access Point credentials).
- Screenshots or files of any kind.

Error text can still mention a value, for example a ledger name in an error message. So
before anything is saved, and again before the issue link is built, KhataOS scrubs the
text:

| What | Becomes |
|---|---|
| Your home folder (`/Users/<you>`, `/home/<you>`, `C:\Users\<you>`) | `~` |
| The open company's path, file name and name, and any `*.khata` / `*.khatapkg` path | `<company>` |
| Your OS user name and the signed-in company user | `<user>` |
| Anything that looks like an email address | `<email>` |
| GSTIN, PAN, and other upper-case tax IDs (TRN, VAT numbers, RFC) | `<tax-id>` |
| More than six digits in a row, or digit groups that add up to nine or more (phone, account, SSN) | `<number>` or `<id>` |
| Decimal and grouped amounts (`12,500.00`, `1,23,456`, `99.5`) | `<amount>` |
| Quoted text that reads like a name or value (`"Rahul Traders"`) | `"…"` |

Version numbers (`2.12.1`), source locations (`api.rs:812:31`) and dates stay. The report
is cut to fit a GitHub link (about 7,000 characters): the stack is shortened first, then
the message.

The scrubber is a safety net, not a guarantee. **Read the report before you submit it**,
both in KhataOS's review dialog and on GitHub, and delete anything you don't want public.
GitHub issues are public. Never attach a company file.

## Where the files are

Crash notes are JSON files in a `crashes` folder inside the app's data folder. Only your
OS user can read them (folder `0700`, files `0600` on macOS, Linux, Android and iOS;
Windows keeps them in your own `%APPDATA%`). KhataOS keeps the last 20 and deletes older
ones. A crash that repeats while the first is still waiting is saved once, and no more
than five are saved in one run.

| OS | Folder |
|---|---|
| Windows | `%APPDATA%\org.khataos.desktop\crashes\` |
| macOS | `~/Library/Application Support/org.khataos.desktop/crashes/` |
| Linux | `~/.local/share/org.khataos.desktop/crashes/` (or `$XDG_DATA_HOME/org.khataos.desktop/crashes/`) |
| Android | The app's private data folder (`org.khataos.mobile`), which other apps can't read. Clearing the app's storage removes it. |
| iOS | The app's private container (`org.khataos.mobile`). Deleting the app removes it. |

On a desktop, the review dialog for a crash has **Show crash files**, which opens this
folder. You can read or delete the files at any time. Deleting them has no other effect.

## Turning it off

Choose **Don't ask again** in the start-up prompt. KhataOS then stops asking and stops
writing crash notes. **Report a problem** still works, because it only opens the issue
page you review.

The choice is a device setting, not part of any company. It is stored in
`crashes/settings.json` in the folder above, as `{"ask_on_start": false}`. To turn crash
notes back on, delete that file or set the value to `true`.

## Builds from a fork

The issue link goes to `https://github.com/khataos/khataos/issues`. A build can send
reports to another tracker by setting `KHATA_ISSUES_URL` when it is built:

```sh
KHATA_ISSUES_URL=https://github.com/<you>/<repo>/issues pnpm tauri build
```

The tracker needs `.github/ISSUE_TEMPLATE/crash_report.yml`. The app fills the fields
`title`, `version`, `os`, `platform`, `country`, `language` and `details`.

# FAQ

## About KhataOS

**What is KhataOS?**
Free, open-source accounting that keeps your books in a file on your own device. It is keyboard-first and uses Tally-compatible shortcuts. It runs on Windows, macOS and Linux, and builds for Android and iOS.

**Is it ready for my real books?**
Not yet. KhataOS is in alpha. Try it on a test company, and keep your real books where they are until it's out of alpha.

**What does it cost?**
Nothing. It is free software under the GNU Affero General Public License (AGPL-3.0-or-later): you can use, study, change and share it.

**Is it made by Tally Solutions, or a copy of Tally?**
No. KhataOS is an independent project. It reproduces the behaviour Tally users rely on, such as the keys and the workflow, without copying Tally's code, screens or text. It imports data through Tally's published XML export.

**Which countries does it support?**
India, the UAE, Saudi Arabia, Singapore and the UK are the launch markets. Germany, France, the Netherlands, Spain, the US and Mexico are also in the app, and are less mature. See [the country pages](README.md#your-country).

**Which languages?**
English and Arabic, with a full right-to-left layout (`Ctrl+K`). See [Arabic](arabic.md).

## Your data

**Where are my books?**
In one `.khata` file per company, on your device. The home screen shows its path. See [Backup](backup.md).

**Does anything go to the internet?**
Only what you choose: syncing through a relay or server, e-invoice and tax-portal connections you set up, and crash reports you review and send yourself.

**Can someone read my books on the relay?**
No. Entries are encrypted on your computer before they're sent, and the relay only stores the sealed entries. See [Sharing](sharing.md).

**Is the company file encrypted?**
No. The company file on your disk isn't encrypted (accountant files are). Protect it like any other business file, and use your device's disk encryption.

**How do I back up?**
Copy the company file. There's no backup command yet. See [Backup](backup.md).

**Can I edit or delete history?**
You can alter, cancel and delete vouchers, but every change is kept in the audit trail, which can't be switched off or edited. This is what Indian rules require (Companies (Accounts) Rule 3(1)).

## Coming from Tally

**Will my Tally keys work?**
Most of the everyday ones do: the home menu letters, `F4` to `F9`, `Ctrl+A`, `Alt+G`, `F2`, `Alt+F2`, `Ctrl+H`, `Alt+X` and more. Some are reserved but not built yet. See [Coming from Tally](coming-from-tally.md) and the [key reference](keyboard.md#key-reference).

**Can I import my Tally data?**
Yes: masters and vouchers from Tally's XML export. Bill-wise details, cost centres and some voucher types don't come across yet. See [Import from Tally](import-from-tally.md).

**Can I keep using Tally alongside?**
Yes. KhataOS doesn't change your Tally data. Import into a test company and compare.

**Is there payroll? Orders? Cost centres?**
Not yet. See [what isn't there yet](coming-from-tally.md#not-there-yet) and the [roadmap](../roadmap.md).

## Tax and compliance

**Does KhataOS file my returns?**
In most countries, no: it prepares the return figures and the files, and you file them. Saudi Arabia's ZATCA e-invoicing works against ZATCA's sandbox, and the UK MTD submission is built but tested only against a mock. Check the status on each country page.

**Can I trust the tax figures?**
They are a preparation aid. Check them with your accountant before you file. If you find a wrong tax rule, [report it](https://github.com/khataos/khataos/issues/new/choose) with the official source.

**Why can't I change the company's country?**
The country sets the currency, financial year and tax rules for all the books, so it's set once. Create a new company for another country.

## Using it

**Do I need a mouse?**
No. Every action has a key. The mouse and touch work too.

**Does it work on a Mac?**
Yes. `Ctrl` is the Control key and `Alt` is Option, and function keys need `fn`. See [Keyboard](keyboard.md#on-a-mac).

**Can several people work on the same books?**
Yes: over the office network, through the relay, or with accountant files. See [Sharing](sharing.md).

**Can I open two companies at once?**
Not yet. One company is open at a time; `F3` goes back to the company list.

**Can I e-mail or WhatsApp an invoice?**
Not yet. Save the PDF (`Ctrl+E`, **PDF**) and send it yourself.

**Where do I report a bug?**
On [GitHub](https://github.com/khataos/khataos/issues/new/choose), or in the app: `Alt+G`, "report a problem". See [Troubleshooting](troubleshooting.md).

**How can I help?**
Try it and tell us what's wrong, check tax rules for your country, translate, or write code. See [Contributing](https://github.com/khataos/khataos/blob/main/CONTRIBUTING.md).

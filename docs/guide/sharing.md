# Sharing

**What you'll do:** choose how to share a company's books, then set it up: with colleagues on the office network, with your accountant over the internet through the encrypted relay, or by sending an encrypted accountant file. You'll also see how roles work.

Everyone works on their own copy of the books, and KhataOS keeps the copies in step. Every entry is checked by the same rules on every computer. The full guide, with every option, is [Sharing your books](../sharing.md).

| Where are the people? | Use |
|---|---|
| Same office network | **Office server**: one PC holds the books |
| Anywhere, every day | **Relay**: over the internet, encrypted so only you and the people you share with can read it |
| Anywhere, now and then (month end, audit) | **Accountant file** (`.khatapkg`) |

## Colleagues on the office network

**On the PC that holds the books:**

1. Open the company.
2. Press `Alt+Z` (Exchange), then **Configure: Client/Server**.
3. Set **KhataOS acts as** to **Server**. Leave the port at 9090.
4. Press `Ctrl+A`, and answer `Y` to share the company on the office network.
5. If your system asks, allow KhataOS on private networks.

Keep KhataOS open on this PC while others work.

**Invite a colleague:**

1. On the server PC, press `Alt+K`, then **Remote Access**.
2. Press `Alt+C` (Invite). Pick **Accountant** (can enter and change vouchers) or **Viewer** (reports only), who it's for, and an expiry in days if you want one.
3. Press `Enter` to copy the invite code (it starts with `khata1:`). It is shown only once, and anyone who has it can open the books, so send it only to that person.

**On your colleague's PC:**

1. Press `Alt+Z`, then **Connect to company…**.
2. Paste the code and press `Enter`. The books are copied and opened.

The books then sync on opening, after each save, and every 2 minutes. `Ctrl+Alt+S` syncs at once. To remove someone, open **Remote Access** on the server PC, move to them, and press `Alt+D` (Revoke).

## Your accountant, over the internet (relay)

The relay passes changes between computers but **can't read your books**: each entry is encrypted on your computer first.

1. Get a relay address from whoever runs one (your CA firm or IT person), or [host your own](../deploy-relay.md). KhataOS has no relay switched on by default.
2. Press `Alt+K`, then **Configure Online Access**. Set **Sync via** to **Relay (internet)** and type the relay address. A private relay asks once for its signup key.
3. Invite your accountant from **Remote Access** as above. They connect with **Connect to company…**.

**Move to another relay…** (in `Alt+Z`) moves a company to a new relay with a new key; invite people again afterwards.

## Send an accountant file

No server or relay needed. Send the file any way you like: it's encrypted.

**Send:**

1. Press `Alt+M` (Share), then **Send to accountant (file)**.
2. If you've sent one before, choose **New entries only** or **Everything**.
3. Type a passphrase (at least 8 characters) twice.
4. Save the `.khatapkg` file, and send it. Tell the passphrase to your accountant another way, such as a phone call.

**Open (your accountant):**

1. Press `Alt+O` (Import), then **Open accountant file (.khatapkg)**.
2. Pick the file and type the passphrase.

The first file creates the company; later files add only the new entries. If both sides made entries since the last file, KhataOS refuses and says so.

## Roles

There are two kinds of role.

**Sharing roles**, chosen on an invite:

- **Accountant**: can enter and change vouchers.
- **Viewer**: can only read. Every report works, but nothing can be entered or changed. A Viewer on the office network can also read reports in a web browser: see **Browser Access** in the Company menu (`Alt+K`).

**Users and passwords inside the company** (`Alt+K`, **Users and passwords**): Owner, Accountant, Data Entry and Auditor. See [Your first company](first-company.md#6-add-users-optional). Users sync with the books.

## Sync with your own server

If you run a KhataOS sync server, put its address, the company id and the access token in `Alt+K`, **Sync settings**. Then `Ctrl+Alt+S` syncs. If two people changed the books offline, KhataOS offers to rebase your changes onto the server's. A review screen lists any renumbered vouchers and entries that need attention, and the file as it was before is kept as `<name>.conflict-<time>.khata`.

## What isn't shared yet

Desk notes and to-dos, the compliance profile, the invoice design, UK VAT scheme settings and sync settings stay on the computer where they were made. Sending by email and WhatsApp isn't built yet.

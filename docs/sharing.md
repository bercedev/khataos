# Sharing your books

KhataOS can share a company's books with your staff, your CA or an auditor. You don't need to install a server or Docker, and you don't need any IT knowledge. If you have used TallyPrime's Client/Server setup, this works the same way.

There are three ways to share. Pick the one that fits where people sit:

| Where are the people? | Use | What you need |
|---|---|---|
| Same office, same Wi-Fi or network cable | **Office server**: one PC acts as the server | KhataOS open on that PC |
| Anywhere, working together day to day | **Relay**: end-to-end encrypted, over the internet | An internet connection and a relay address |
| Anywhere, sending books now and then (month-end, audit) | **Accountant file** (`.khatapkg`) | Any way to send a file: WhatsApp, e-mail, USB stick |

Everyone works on their own copy of the books. KhataOS keeps the copies in step. Every entry is checked by the same rules on every computer, and the audit trail records who made it.

Notes and to-dos, the invoice design and logo, and tax settings such as UK VAT schemes travel the same three ways. If two people change the same note, the later change wins, and a deleted note stays deleted ([how this works](sync-settings.md)). Sync passwords and keys never leave your computer.

---

## 1. Office server (LAN)

One office PC keeps the books for everyone, as in TallyPrime. The others connect to it.

### On the PC that will hold the books (the "server")

1. Open the company.
2. Press **Alt+Z** (Exchange), then choose **Configure: Client/Server**.
3. Press Enter on **KhataOS acts as** and choose **Server**. Choose **Both** if this PC also connects to companies on another server.
4. Leave **Port** at **9090**. TallyPrime uses 9000, so both programs can run side by side.
5. Press **Ctrl+A** to save. The screen shows **Server: Running on port 9090**, plus this PC's **office network address**, for example `192.168.1.20:9090`.
6. When asked *"Share <company> on the office network now?"*, press **Y**.
7. If Windows or macOS asks whether KhataOS may accept connections, allow it on **private networks**.

Keep KhataOS open on this PC while others work. Entries made on the server PC are shared within a few seconds.

### Invite a colleague

1. On the server PC, press **Alt+K** (Company) and choose **Remote Access**.
2. Press **Alt+C** (Invite) and choose a role:
   - **Accountant**: can enter and change vouchers.
   - **Viewer**: can see reports only. Use this for auditors and advisors.
3. Type who the invite is for (for example *Ravi (accounts)*). You can also set the number of days before it expires; leave this blank for no expiry. Press Enter through the fields.
4. KhataOS shows one long **invite code** that starts with `khata1:`. Press **Enter** to copy it, then send it on WhatsApp or by e-mail.

The invite code is shown only once. It works like a key: anyone who has it can open the books, so send it only to the person it is for.

### On the colleague's PC

1. Open KhataOS. Press **Alt+Z** (Exchange) and choose **Connect to company…**. With a company already open, you can also use **Alt+K → Connect to company…**.
2. Paste the invite code. KhataOS shows the company name, the server and the access level.
3. Press **Enter**. KhataOS creates the company on this PC, copies all the books from the server and opens them.

From then on the books sync by themselves:
- when the company is opened;
- right after each entry is saved;
- every 2 minutes, to pick up other people's entries.

You can change the interval in **Alt+K → Configure Online Access**. **Ctrl+Alt+S** syncs straight away.

### Removing someone's access

On the server PC, open **Alt+K → Remote Access**, move to the person and press **Alt+D** (Revoke). Their copy stops syncing straight away. An invite with an expiry date stops working on that date by itself.

### Viewers

A company opened with a **Viewer** invite is read-only: every report is available, but nothing can be entered or changed. A Viewer who doesn't have KhataOS can read the reports in a web browser on the office network. Open **Alt+K → Browser Access** for the address. The Viewer's invite screen shows the company id and token they need to enter on that page.

---

## 2. Over the internet: a relay (end-to-end encrypted)

Use a relay when people work from different places, for example a CA's office and the business. The relay passes changes between computers, but it **cannot read your books**: KhataOS encrypts every entry on your computer before it is sent, and only computers with the company's key can open it.

### Where the relay comes from

KhataOS doesn't switch on any relay by itself. You need a relay address, from one of these:

| Option | What to do |
|---|---|
| **Someone runs a relay for you** (your CA firm, your IT person, or KhataOS while it offers a hosted relay) | Ask them for the address, for example `https://relay.your-firm.in`. A private relay also has a **sign-up key**, needed once when you first share a company. |
| **Host your own** | Any always-on Windows, Mac or Linux PC can run one. No Docker, no router changes. See [Hosting a KhataOS relay](deploy-relay.md). |
| **No relay at all** | Send **accountant files** instead (section 3). |

If you choose the relay but no address is set, KhataOS says so and offers these same options.

### Sharing a company through a relay

1. Press **Alt+Z** (Exchange) and choose **Configure: Client/Server**. Set **Relay address** and press **Ctrl+A** to save. This is the relay for every company on this computer. A copy of KhataOS can come with an address built in; it is shown as *(built in)*.
2. Open the company. Press **Alt+K** and choose **Configure Online Access**.
3. Set **Sync via** to **Relay (internet)**. **Relay address** shows this computer's relay; change it here only if this company should use a different one. Press **Ctrl+A** to save.
4. Choose **Alt+K → Connect**. If the relay is private, KhataOS asks for its **sign-up key** once. The key isn't saved.
5. Invite people from **Alt+K → Remote Access** in the same way as on the office network. A relay invite code also carries the company's **encryption key**, so treat it with extra care. People who join with an invite code don't need the relay address or the sign-up key.

**The company key is a secret.** It is kept inside the company file on each connected computer and in the invite codes. It is never sent to the relay. If you lose every computer that has the key, the relay copy can't be read by anyone, including KhataOS.

Browser Access isn't available for relay companies, because the relay can't read the books.

### Moving to another relay

A company can move from one relay to another. For example, KhataOS or your CA hosts a relay for a while, and later your firm runs its own.

1. Ask everyone to sync (**Ctrl+Alt+S**) and to stop making entries for a few minutes. Sync on this computer too.
2. On the owner's computer, open the company and choose **Alt+K → Move to another relay…** (also in **Alt+Z** Exchange).
3. Enter the new relay's address and, if it is private, its sign-up key. Confirm.

KhataOS then:
- copies every entry to the new relay, encrypted with a **new key**;
- switches this computer to the new relay;
- cancels the old invite codes on the old relay.

**Colleagues need new invite codes**: invite them again from **Alt+K → Remote Access**. They paste the new code with **Connect to company…**, which makes a fresh copy of the company on their computer. Their old copy stops syncing; they can keep it for reference or delete it.

KhataOS refuses to move a company while this computer or the old relay has changes the other doesn't, so nothing is left behind. The old relay keeps its encrypted copy. With the old key retired, entries made after the move can't be read with it, even by someone who still has an old invite code.

---

## 3. Accountant files (`.khatapkg`): no server, no shared network

This is the easiest option for sending books to your CA at month-end, or receiving their adjustments back.

### Sending

1. Open the company and press **Alt+M** (Share). Choose **Send to accountant (file)**.
2. Choose **New entries only** (everything since the last file you sent) or **Everything**. The first time, KhataOS always sends everything.
3. Type a passphrase twice, at least 8 characters.
4. Save the file, for example `sharma-traders-2026-10-19.khatapkg`, and send it any way you like.
5. Tell the accountant the passphrase **by phone or in person**, never in the same message as the file.

The file is encrypted. WhatsApp, the e-mail provider or anyone who intercepts it can't read it without the passphrase.

### Receiving

1. Press **Alt+O** (Import) and choose **Open accountant file (.khatapkg)**. With no company open, use **Alt+Z → Open accountant file**.
2. Pick the file and type the passphrase.
3. KhataOS checks every entry before changing anything:
   - The first file you receive creates the company on your computer.
   - Later files add only the new entries to that company.
   - Opening the same file twice changes nothing.

### When KhataOS can't apply a file

| Message | What to do |
|---|---|
| *An earlier file is missing* | A file was skipped. Ask the sender for the earlier file, or for a full file (**Everything**). |
| *Both sides changed the books* | You made entries since the last file, and so did the sender. Send your changes to them first, or use the office server or relay, which handle this. |
| *Wrong passphrase* | Check the passphrase with the sender. |
| *This file is for another company* | Shut the open company and open the file again. |

---

## Settings at a glance

| Setting | Where | Applies to |
|---|---|---|
| KhataOS acts as: None / Client / Server / Both | Alt+Z → Configure | This computer |
| Port (default 9090) | Alt+Z → Configure | This computer |
| Connect Company on loading: Yes / No | Alt+K → Configure Online Access | This company, on this computer |
| Sync every (minutes, default 2) | Alt+K → Configure Online Access | This company, on this computer |
| Relay address (blank = none, or the built-in one) | Alt+Z → Configure | This computer |
| Sync via: Office server / Relay | Alt+K → Configure Online Access | This company, on this computer |
| Relay address for this company (blank = this computer's) | Alt+K → Configure Online Access | This company, on this computer |

**Alt+K → Connectivity Status** shows:
- the server address;
- the last sync;
- how many entries on this computer haven't been sent yet;
- how many entries the server holds.

**Alt+K → Disconnect** pauses background sync until you choose **Connect**.

## Security in short

- Plain `http://` is used only on this computer and on private office-network addresses (192.168.x.x, 10.x.x.x, 172.16–31.x.x, `.local` names, and Tailscale-style 100.64–127.x.x VPN addresses, whose link is already encrypted). Anything across the internet must use `https://`. KhataOS refuses invite codes that would break this rule.
- Only hashes of access tokens are stored on the server. Access can be revoked and can expire.
- Every change from another computer goes through the same checks as a local entry before it is saved: balanced vouchers, valid references, and an unbroken audit-trail chain.
- Relay companies are end-to-end encrypted. Accountant files are encrypted with your passphrase.
- The office server's admin key is random and created once. It is stored in the app's data folder, readable only by your user account.
- A relay sees only encrypted entries. A public relay limits how many companies one internet address can create each day, and how much each company can store. See [Hosting a KhataOS relay](deploy-relay.md).

## Relays: who runs them

KhataOS comes with no relay switched on. Use a relay that someone runs for you, host your own on any always-on PC ([Hosting a KhataOS relay](deploy-relay.md)), or send accountant files. KhataOS may host a relay for a limited period. If it does, its address will be announced, and companies on it can later move to your own relay (see *Moving to another relay*).

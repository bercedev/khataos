# Hosting a KhataOS relay

A **relay** lets KhataOS share books over the internet between people who don't sit on the same office network, for example a business and its CA. This guide is for the person who sets one up: an accountant's IT person, a CA firm, or the KhataOS team.

You don't need Docker, a database server or any programming. The relay is one small program, `khata-server`, plus something that gives it an `https://` address.

## What the relay can and can't see

KhataOS encrypts every entry on the user's computer **before** it is sent. The key travels only inside invite codes and company files, never to the relay. So the relay:

- **stores** sealed (encrypted) entries, their order, and hashes of access tokens;
- **can't read** names, amounts, ledgers or anything else in the books;
- **can't forge** entries: every computer checks each entry it receives with the company's key and the same accounting rules as a local entry.

At worst, a relay that is switched off or misbehaves stops syncing. The books on each computer stay complete and correct.

## Choose a setup

| Setup | Good for | You need | HTTPS from |
|---|---|---|---|
| **A. Any always-on PC + Cloudflare Tunnel** (easiest) | an office, a CA firm, a small group of clients | a Windows, Mac or Linux PC that stays on; a free Cloudflare account; ideally a domain name | Cloudflare (no router changes, no open ports) |
| **B. A small VPS + Caddy** | a CA firm serving many clients; the KhataOS hosted relay | a VPS from any provider (1 CPU, 1 GB RAM is plenty) and a domain name | Caddy (automatic Let's Encrypt certificates) |
| **C. Docker** (optional) | people who already run Docker | Docker and a reverse proxy | Caddy or Cloudflare, as in A or B |

Ready-to-copy files for all of these are in [`deploy/relay/`](../deploy/relay).

## Before you start

**1. Get `khata-server`.** Use the `khata-server` program for your system (`khata-server.exe` on Windows) from the KhataOS release, or build it from the source code:

```sh
cargo build --release -p khata-server
# the program is then target/release/khata-server (khata-server.exe on Windows)
```

**2. Decide who may create companies on your relay.**

- **Private (the default, recommended for your own relay).** Only people who know the relay's **sign-up key** can share a new company through it. The sign-up key is the relay's admin key. KhataOS asks for it once, when the owner first connects a company, and doesn't save it. Colleagues who join with an invite code never need it.
- **Open** (`KHATA_OPEN_RELAY=1`). Anyone who knows the address can share a company through it, within the limits in [Settings](#settings-for-every-setup). This is how the KhataOS hosted relay runs.

**3. Pick an address**, such as `relay.your-firm.in`. With Cloudflare Tunnel the domain must use Cloudflare for its DNS (free). Without a domain you can still try things out with a temporary `trycloudflare.com` address (part A2).

---

## A. Any always-on PC + Cloudflare Tunnel

The relay runs on a PC in your office. Cloudflare Tunnel makes it reachable at an `https://` address. The PC connects out to Cloudflare, so you don't change anything on your router or open any ports.

The PC must stay on and must not go to sleep:
- **Windows:** Settings → System → Power → Screen and sleep → *When plugged in, put my device to sleep after*: **Never**.
- **macOS:** System Settings → Energy (or Battery → Options) → turn on **Prevent automatic sleeping when the display is off**.

### A1. Run the relay as a background service

The relay listens only on the PC itself (`127.0.0.1:8787`). Nobody on the network can reach it directly; Cloudflare Tunnel forwards to it.

#### Windows

1. Put `khata-server.exe` and `install-windows.ps1` (from `deploy/relay/`) in one folder, for example `Downloads`.
2. Open **PowerShell as Administrator**: Start → type *PowerShell* → right-click → *Run as administrator*. Running as Administrator lets the relay start when the PC starts, even before anyone signs in. Without it, the relay starts when you sign in.
3. Run:

   ```powershell
   cd $HOME\Downloads
   powershell -ExecutionPolicy Bypass -File .\install-windows.ps1 -ServerExe .\khata-server.exe
   ```

   Add `-OpenSignup` for an open relay. Add `-Port 8788` if port 8787 is already taken.

The script downloads nothing. It:
- copies `khata-server.exe` to `%LOCALAPPDATA%\KhataOS-Relay`;
- creates the `data` folder there;
- creates a random admin key in `admin.key`, readable only by your Windows account;
- registers the scheduled task **KhataOS Relay**, which restarts the relay if it ever stops;
- checks the relay answers and prints the next steps.

Run it again with a newer `khata-server.exe` to update. Your key and data are kept.

- **Stop:** `Stop-ScheduledTask 'KhataOS Relay'`
- **Start:** `Start-ScheduledTask 'KhataOS Relay'`
- **Messages:** `%LOCALAPPDATA%\KhataOS-Relay\relay.log`

> Prefer a real Windows service? Use [NSSM](https://nssm.cc): `nssm install "KhataOS Relay" C:\KhataOS-Relay\khata-server.exe`, then set the environment variables (`KHATA_ADMIN_KEY`, `KHATA_DATA_DIR`, `KHATA_BIND=127.0.0.1:8787`, `KHATA_TRUST_PROXY=1`) on its *Environment* tab. NSSM keeps them in the registry, readable by administrators. `sc.exe create` alone won't work, because `khata-server` is a plain program, not a Windows service.

#### macOS

The file `deploy/relay/com.khataos.relay.plist` starts the relay at boot. Open it in TextEdit and replace `YOUR-MAC-USER` with your account name (run `whoami` in Terminal to see it). Then, in Terminal, in the folder with `khata-server` and the plist:

```sh
sudo mkdir -p /usr/local/bin /usr/local/var/khata-relay
sudo cp khata-server /usr/local/bin/ && sudo chmod 755 /usr/local/bin/khata-server
sudo chown "$(whoami)" /usr/local/var/khata-relay && chmod 700 /usr/local/var/khata-relay
(umask 077; openssl rand -hex 32 > /usr/local/var/khata-relay/admin.key)
sudo cp com.khataos.relay.plist /Library/LaunchDaemons/
sudo chown root:wheel /Library/LaunchDaemons/com.khataos.relay.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/com.khataos.relay.plist
curl http://127.0.0.1:8787/v1/health      # prints: ok
```

If macOS says the program is from an unidentified developer, allow it in System Settings → Privacy & Security.

- **Stop:** `sudo launchctl bootout system/com.khataos.relay`
- **Messages:** `/usr/local/var/khata-relay/relay.log`

#### Linux (a PC or small server in the office)

Use `deploy/relay/khata-relay.service`. The commands at the top of that file install it:

```sh
sudo useradd --system --home-dir /var/lib/khata-relay --shell /usr/sbin/nologin khata
sudo install -m 0755 khata-server /usr/local/bin/khata-server
sudo install -d -m 0700 /etc/khata-relay
sudo sh -c 'umask 077; printf "KHATA_ADMIN_KEY=%s\n" "$(openssl rand -hex 32)" > /etc/khata-relay/env'
sudo cp khata-relay.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now khata-relay
curl http://127.0.0.1:8787/v1/health      # prints: ok
```

- **Messages:** `journalctl -u khata-relay`
- **Data:** `/var/lib/khata-relay`

### A2. Try it with a temporary address

Install `cloudflared`, Cloudflare's tunnel program:
- **Windows:** `winget install --id Cloudflare.cloudflared`, then open a new PowerShell window.
- **macOS:** `brew install cloudflared`
- **Linux:** follow Cloudflare's instructions for your distribution's package repository, or install the `.deb`/`.rpm` from [github.com/cloudflare/cloudflared/releases](https://github.com/cloudflare/cloudflared/releases).

Then, on Windows, macOS or Linux alike, run:

```sh
cloudflared tunnel --url http://localhost:8787
```

After a few seconds it prints an address such as `https://plain-words-here.trycloudflare.com`. Open `<that address>/v1/health` in a browser; it should say `ok`.

This address changes every time `cloudflared` restarts, and invite codes contain the address. **Use it only for trying things out**, not for real books.

### A3. A permanent address on your own domain

You need a domain whose DNS is managed by Cloudflare (a free plan is enough).

1. Sign in at [dash.cloudflare.com](https://dash.cloudflare.com) and open **Zero Trust**. Cloudflare moves these menus now and then; look for **Networks → Tunnels**. Choose **Create a tunnel**, type **Cloudflared**, and name it `khata-relay`.
2. Choose your PC's system (Windows, macOS or Linux). Cloudflare shows one command that installs `cloudflared` as a service, such as `cloudflared.exe service install eyJh…`. Run it:
   - **Windows:** in PowerShell as Administrator;
   - **macOS / Linux:** with `sudo`.

   The long token in that command is a secret: don't share it.
3. Add a **public hostname**: subdomain `relay`, your domain, service type **HTTP**, URL **`localhost:8787`**. Save.
4. Open `https://relay.your-firm.in/v1/health`. It should say `ok`.

> Prefer the command line? `cloudflared tunnel login`, `cloudflared tunnel create khata-relay`, `cloudflared tunnel route dns khata-relay relay.your-firm.in`, then a `config.yml` with `ingress: [{hostname: relay.your-firm.in, service: http://localhost:8787}, {service: http_status:404}]`, and `cloudflared service install`.

The relay is set up with `KHATA_TRUST_PROXY=1`, so it reads each user's real address from Cloudflare for the sign-up limit. That is safe because the relay listens only on `127.0.0.1`, so nothing but `cloudflared` can reach it.

### A4. Use it in KhataOS

On each computer that will share a company through this relay:

1. Press **Alt+Z** (Exchange) and choose **Configure**. Set **Relay address** to `https://relay.your-firm.in`. Press **Ctrl+A** to save.
2. Open the company, then **Alt+K → Configure Online Access → Sync via: Relay**, and **Alt+K → Connect**.
3. If the relay is private, KhataOS asks for the **sign-up key**: the contents of `admin.key` (Windows and macOS) or the value after `KHATA_ADMIN_KEY=` in `/etc/khata-relay/env` (Linux).
4. Invite colleagues from **Alt+K → Remote Access**. They only paste the invite code; they don't need the relay address or the sign-up key.

---

## B. A small VPS + Caddy

For a relay that many clients use, or the KhataOS hosted relay. Any provider works. Pick a region close to your users (for India, Mumbai or another Indian region).

1. **Create the server:** Ubuntu 24.04 LTS or Debian 12, 1 vCPU, 1 GB RAM, 25 GB disk to start. Use SSH keys, not passwords.
2. **Point your domain at it:** add an `A` record (and an `AAAA` record for IPv6) for `relay.your-firm.in` with the server's address.
3. **Copy the program and the service file.** Use a Linux x86-64 build of `khata-server`:

   ```sh
   scp khata-server deploy/relay/khata-relay.service you@relay.your-firm.in:/tmp/
   ```

4. **Install the relay.** On the server, `cd /tmp` and run the Linux commands from part A1. To make it an open relay, edit `/etc/systemd/system/khata-relay.service` first: set `KHATA_OPEN_RELAY=1` and, if you like, `KHATA_RELAY_MAX_TOTAL_BYTES`.
5. **Install Caddy** (`sudo apt install caddy`, or Caddy's own package repository for the newest version). Copy `deploy/relay/Caddyfile` to `/etc/caddy/Caddyfile` and replace `relay.example.in` with your address. Then:

   ```sh
   sudo systemctl reload caddy
   ```

   Caddy gets and renews the HTTPS certificate by itself.
6. **Firewall:** allow only SSH, HTTP and HTTPS:

   ```sh
   sudo ufw allow OpenSSH && sudo ufw allow 80,443/tcp && sudo ufw enable
   ```

7. **Check:** `curl https://relay.your-firm.in/v1/health` prints `ok`. Then continue with part A4.

---

## C. Docker (optional)

If you already run Docker, `deploy/relay/Dockerfile` and `deploy/relay/docker-compose.yml` build and run the relay. From `deploy/relay/`:

```sh
(umask 077; printf "KHATA_ADMIN_KEY=%s\n" "$(openssl rand -hex 32)" > .env)
docker compose up -d --build
curl http://127.0.0.1:8787/v1/health      # prints: ok
```

Data is in the `khata-data` volume. Put Caddy (part B) or Cloudflare Tunnel (part A) in front of it, forwarding to `127.0.0.1:8787`.

---

## Settings for every setup

`khata-server` reads these environment variables:

| Variable | Default | Meaning |
|---|---|---|
| `KHATA_ADMIN_KEY` | *(required)* | Secret of 24+ characters. Also the **sign-up key** of a private relay. Make one with `openssl rand -hex 32`. |
| `KHATA_DATA_DIR` | `./khata-data` | Folder for `server.db`. |
| `KHATA_BIND` | `127.0.0.1:8787` | Address and port. Keep `127.0.0.1` behind Caddy or Cloudflare Tunnel. |
| `KHATA_OPEN_RELAY` | off | `1`: anyone may create encrypted companies without the sign-up key. |
| `KHATA_RELAY_MAX_NEW_PER_IP_DAY` | `20` | New companies per user address per day without the sign-up key (`0` = no limit). Over the limit, KhataOS shows *try again tomorrow*. |
| `KHATA_RELAY_MAX_BYTES` | `1073741824` (1 GiB) | Storage per company (`0` = no limit). Over it, the relay refuses new entries with a clear message, and the owner can move the company to another relay. |
| `KHATA_RELAY_MAX_TOTAL_BYTES` | none | Storage for all companies together. When it is reached, no new entries and no new companies are accepted. |
| `KHATA_TRUST_PROXY` | off | `1`: take the user's address from `X-Forwarded-For`. Only behind Caddy or Cloudflare Tunnel, with `KHATA_BIND` on `127.0.0.1`. Otherwise anyone could fake their address. |
| `KHATA_SOURCE_URL` | the KhataOS repository | Where `/source` points. Set it if you run a modified server (AGPL). |

A few notes on these settings:
- Limits only refuse **new** data. Nothing stored is ever changed or removed: the relay is append-only.
- The per-address count is kept in memory, so a restart starts it again from zero.
- Behind a Caddy or Cloudflare proxy, set `KHATA_TRUST_PROXY=1`. Without it, all users appear to come from one address and share one daily allowance.

**How much space?** A sealed entry takes roughly two to three times the size of the change it carries, typically a few kilobytes per voucher. 1 GiB holds several hundred thousand changes, which is years of work for most small businesses.

## Data, backups and updates

All of the relay's data is one SQLite file, `server.db`, in the data folder:

| Setup | Data folder |
|---|---|
| Windows script | `%LOCALAPPDATA%\KhataOS-Relay\data` |
| macOS plist | `/usr/local/var/khata-relay/data` |
| Linux / VPS | `/var/lib/khata-relay` |
| Docker | volume `khata-data` |

It holds the sealed entries, the companies' ids, and hashes of access tokens. It contains no readable books. Back it up anyway: if it is lost, every company on the relay has to be shared again.

**Backup while running** (Linux, macOS; needs the `sqlite3` tool):

```sh
sqlite3 /var/lib/khata-relay/server.db ".backup '/var/backups/khata-relay/server-$(date +%F).db'"
```

Run it daily (cron or a systemd timer), copy the result to another place, and keep about two weeks.

**Backup by copying** (any system, including Windows):
1. Stop the relay.
2. Copy the whole data folder.
3. Start the relay again.

**Restore** (only when the data is lost or damaged):
1. Stop the relay.
2. Put the backed-up `server.db` in the data folder.
3. Start the relay.

A restored relay knows only what was in the backup. A company that synced after the backup was made will report that the server's history differs. Its books are still complete on its users' computers: the owner moves it with **Move to another relay…**, which copies everything from the owner's computer, and invites colleagues again. That's why daily backups matter.

**Update:**
1. Replace the `khata-server` program.
2. Restart it: `sudo systemctl restart khata-relay`, `sudo launchctl kickstart -k system/com.khataos.relay`, or on Windows run `install-windows.ps1` again.

## D. How KhataOS runs its temporary hosted relay

KhataOS will run a public relay **for a limited period**, for people who can't host one yet. The long-term plan is that each firm or client runs its own (parts A to C), and moves its companies there with **Move to another relay…**.

**Setup:** part B (VPS + Caddy) with these settings in the systemd unit and `/etc/khata-relay/env`:

```ini
KHATA_ADMIN_KEY=<openssl rand -hex 32; kept in the team password manager only>
KHATA_DATA_DIR=/var/lib/khata-relay
KHATA_BIND=127.0.0.1:8787
KHATA_OPEN_RELAY=1
KHATA_TRUST_PROXY=1
KHATA_RELAY_MAX_NEW_PER_IP_DAY=20
KHATA_RELAY_MAX_BYTES=1073741824
KHATA_RELAY_MAX_TOTAL_BYTES=<about 70% of the disk>
```

**Monitoring:**
- An external uptime check on `https://<relay>/v1/health` every few minutes. It must answer `ok`.
- An alert when the disk is 80% full.
- `journalctl -u khata-relay` for errors.
- Storage in use, without reading anything private:

  ```sh
  sqlite3 -readonly /var/lib/khata-relay/server.db "SELECT COUNT(*), SUM(bytes) FROM relay_usage"
  ```

**Backups:** the nightly `sqlite3 .backup` above, copied off the server, plus a restore test every few months.

**The app's default relay:** the desktop app has no relay address built in. When the domain is ready, release builds are made with `KHATA_DEFAULT_RELAY=https://relay.<domain>` set at build time. Users can still change or clear it in Exchange → Configure. Builds without it tell users plainly that no relay is set and offer the other options.

**Ending the service:**
1. Announce the end date well in advance, in the app's release notes and on the website.
2. Tell owners to use **Alt+K → Move to another relay…** before that date. Moving copies the whole company, sealed with a **new key**, to the new relay. It also cancels colleagues' old invite codes, so the owner re-invites colleagues on the new relay.
3. After the date, stop the service.
4. After a grace period, delete the server and its backups. The relay is append-only, so single companies can't be removed while it runs. Ending the service removes everything at once.

## Moving companies between relays

Owners move a company themselves: **Alt+K (Company) → Move to another relay…**, or **Alt+Z (Exchange) → Move to another relay…**. See *Moving to another relay* in [sharing.md](sharing.md). Before moving, everyone should sync. KhataOS refuses to move a company while this computer or the old relay has changes the other doesn't.

## Security notes

- **The relay never sees the books.** Entries are sealed with XChaCha20-Poly1305 on the user's computer, and each sealed entry is bound to its company and position. What a relay operator, the hosting provider or Cloudflare *can* see:
  - how many entries each company has, their sizes and when they arrive;
  - the users' internet addresses;
  - the access tokens in requests.

  Tokens let someone fetch sealed data, which is useless without the key, or disrupt syncing. They can't be used to read or forge entries.
- **Protect the admin key.** It lets anyone create companies on your relay; on a private relay it is the sign-up key. It can't read any books. If it leaks, set a new one and restart. Existing companies and invites keep working.
  - Keep it in a password manager.
  - Never put it in chat, e-mail or a Git repository.
  - Never put it on a command line others can see.
- **HTTPS is mandatory.** The KhataOS app refuses plain `http://` except on the same computer or a private office network. Never expose port 8787 to the internet directly. Keep `KHATA_BIND=127.0.0.1:8787` behind Caddy or Cloudflare Tunnel.
- **Keep it small and up to date:**
  - Install system updates.
  - Update `khata-server` when KhataOS releases a new version.
  - Allow only SSH (with keys), HTTP and HTTPS on a VPS.
- **Logs:** the provided Caddyfile keeps no access log. Don't add logging of request bodies or `Authorization` headers in any proxy.
- **Backups** hold only sealed data and token hashes. Store them as carefully as the server anyway.

## When something goes wrong

| What KhataOS says | What it means | What to do |
|---|---|---|
| *No relay address is set* | This computer has no relay address. | Enter one in Exchange → Configure → Relay address, or use accountant files. |
| *This relay doesn't accept new companies without a sign-up key* | The relay is private. | Enter the sign-up key (the relay's admin key) when asked. |
| *At most N new companies a day from one address* | The open relay's daily limit. | Try again tomorrow, or raise `KHATA_RELAY_MAX_NEW_PER_IP_DAY`. Behind a proxy, check `KHATA_TRUST_PROXY=1`. |
| *This company has reached the relay's storage limit* | `KHATA_RELAY_MAX_BYTES` is reached. | Move the company to another relay, or ask the operator to raise the limit. |
| *This relay is full* | `KHATA_RELAY_MAX_TOTAL_BYTES` is reached. | The operator frees disk space and raises the limit, or owners move to another relay. |
| *The server URL must use https://* | The address starts with `http://`. | Use the `https://` address from Cloudflare or Caddy. |
| *Couldn't reach the relay* | The relay PC is off or asleep, or the tunnel is down. | Check `<address>/v1/health` in a browser, and the relay's log. |

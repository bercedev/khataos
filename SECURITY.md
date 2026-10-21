# Security policy

KhataOS holds people's books, their customers' details and their credentials for tax authorities. We take reports seriously and thank everyone who reports responsibly.

## Reporting a vulnerability

**Please don't open a public issue, discussion or pull request for a vulnerability.**

Report it privately through GitHub's private vulnerability reporting:
**[Report a vulnerability](https://github.com/khataos/khataos/security/advisories/new)** (repository → Security → Advisories → Report a vulnerability).

Please include:
- what is affected (crate, command, endpoint or screen) and the commit or version;
- the steps to reproduce it, or a proof of concept;
- what an attacker could do with it, and what they need first (network access, a token, a file the victim opens).

Never include real company files, real customer data or real credentials. Use a test company and made-up data.

What to expect:
- we acknowledge the report within 7 days;
- we keep you updated while we investigate and fix it;
- we publish an advisory once a fix is available, and credit you unless you prefer not to be named.

KhataOS is a volunteer project in alpha, so these are aims, not guarantees.

## Supported versions

| Version | Supported |
|---|---|
| `main` branch | Yes |
| The latest alpha release (`0.x`), once releases exist | Yes |
| Older alphas | No: please update |

## Scope

In scope, and the areas we most want reviewed:

- **The sync server** (`crates/khata-server`): authentication and the owner, accountant and viewer tokens; access across companies; push validation; the accountant portal (`/portal`) and its read-only endpoints; request limits and denial of service.
- **The relay** (`khata-server` `/v1/relay`, `crates/khata-seal`, `crates/khata-sync`): anything that lets the relay or another party read sealed entries, forge or replay them, or keep access after a relay move or a token revocation. Invite codes and the company key. Open-relay sign-up limits.
- **Credential stores**: the owner-only files where the desktop app keeps credentials for ZATCA, HMRC MTD, the India IRP, the Mexico PAC and Peppol Access Points. Anything that leaks them into a company file, a backup, a log, the audit trail or the UI, or that lets another local user read them. Leaks of sync tokens, relay keys and invite codes are in scope too.
- **Audit-trail integrity** (`crates/khata-core`, `crates/khata-store`): any way to change or delete history without the hash chain breaking, to get an entry past `Book::apply_entry`, or to get around the append-only triggers.
- **Accountant files** (`crates/khata-package`, `.khatapkg`): decrypting without the passphrase, or applying a tampered or partial package.
- **Untrusted input**: Tally XML imports, bank statement CSV files, GSTR-2B JSON and server responses. Parsing bugs that crash the app, corrupt the books or run code.
- **The desktop and mobile app**: the Tauri command surface, the content security policy, and the office-server (LAN) mode.

Out of scope:

- A wrong tax rule or figure: report it with the "Tax rule" issue form. It is a correctness bug, not a vulnerability, unless it can be triggered by someone else.
- Attacks that need full control of the victim's unlocked device or user account.
- Problems in a tax authority's or provider's own service.
- Missing hardening on a self-hosted server that the operator controls, such as TLS or rate limiting at the reverse proxy (see [`docs/deploy-relay.md`](docs/deploy-relay.md)).

The security model is described in [`docs/architecture.md`](docs/architecture.md#security-model).

# Architecture

```
            ┌──────────── apps ────────────┐
            │ desktop + mobile (Tauri 2)   │   one app: Windows, macOS, Linux, Android, iOS
            └──────┬───────────────────────┘
                   │ commands (JSON, money as decimal strings)
   ┌───────────────┼───────────────────────────────────────┐
   │ khata-keymap  │  khata-core  ◄── compliance packs     │   khata-compliance + one crate per country (khata-in, -ae, -sa, …)
   │ (keys→actions)│  (ledger engine, reports, audit trail)│
   └───────────────┼───────────────────────────────────────┘
                   │ Book ⇄ AuditLog (replay)
            ┌──────┴──────┐            push / pull audit entries
            │ khata-store │  ◄─────────────────────────────►  khata-server
            │ SQLite file │            (bearer tokens)        (sequencer, roles,
            └─────────────┘                                   validation)
```

## The audit trail is the database

Every state change (creating or altering a ledger, setting an opening balance, posting, altering or deleting a voucher) is an `AuditEntry`. Each entry records who did what, when, and snapshots of the record before and after. Its SHA-256 hash covers the previous entry's hash.

- **Storage:** `khata-store` writes the entries to SQLite and nothing else. Loading a company calls `Book::replay`, which verifies the chain and rebuilds the books. Database triggers reject `UPDATE` and `DELETE` on the trail.
- **Compliance:** this is the edit log India requires: Companies (Accounts) Rule 3(1) for companies, and GST Rule 56(8) for every GST registrant. It can't be disabled, because it *is* the data.
- **Sync:** devices exchange entries. The server is the sequencer. A push names the trail length it built on, and a stale push gets `409 Conflict`. The server replays the trail and validates each voucher before accepting a push.
- **Recovery:** any replica (laptop, server, the CA's copy) can rebuild the books, and corruption shows up as a broken hash.

Default account groups have deterministic ids (UUID v5 of the name), so references agree across companies, devices and replays.

### Design notes and open items

- **Concurrent offline edits.** This is handled by `khata_sync::rebase`. The device rebuilds the books from the server's trail. It then re-applies its unsynced changes as new, validated operations (`Book::reapply`), keeping record ids, author and time. Voucher numbers taken in the meantime are renumbered. Changes that can't apply, such as altering a voucher someone else deleted, are returned as `needs_attention`. The pre-rebase file is never modified: it is archived as `<name>.conflict-<time>-<n>.khata`. The desktop app's **Sync: Rebase Review** screen lists the renumbered vouchers, the `needs_attention` items and the archived file.
- **Snapshots.** Replaying is fast for an SME's trail. Periodic snapshot checkpoints will bound load time for very large trails.

## Security model

Every change, whether a local edit, a replay, a rebase or a server push, goes through one gate: `Book::apply_entry`. It checks:
- the hash link;
- that the entry names the record it carries;
- that a Create targets a new record, and that an Alter or Delete targets an existing one whose current state matches `before`;
- that references exist, and that groups inherit from an existing parent (so there are no cycles);
- unique names and voucher numbers, and the double-entry and Contra rules;
- the in-use rules on delete;
- decimal overflow.

A trail that this app could not have produced is rejected, so it can't reach the append-only file.

The server keeps each company's verified books in memory behind a per-company lock. A push is validated on a copy and stored before the cached books change. A panic in one request is contained, because poisoned locks are recovered. There are limits on body size, entries per push and pull page size, and internal error details are logged, not returned.

Clients validate pulled entries the same way before writing them. They require `https://`, except on this computer or the office network (private and link-local IPv4 ranges, the 100.64.0.0/10 range Tailscale uses, IPv6 unique-local/link-local, `.local` names), where the built-in office server runs. They also require a UUID company id. Requests time out after 10 s to connect and 120 s overall. `rebase` refuses a file that is linked to another company, checks that the server's history extends what was synced, and reserves unique file names (`create_new`). If the swap fails, it puts the original back.

**Authorship:** an entry's `user` and `at` fields are what the device recorded, and offline devices can legitimately post days later. The server adds its own provenance: `push_log` records which token pushed which entry range, and when.

**Operational notes:** run behind TLS. Use a long random `KHATA_ADMIN_KEY`. Open companies stay cached in memory, and eviction is not implemented yet. Rate limiting belongs in the reverse proxy for now.

## Sharing modes

See `docs/sharing.md` for the user-facing guide.
- **Office server:** one PC runs `khata-server` inside the desktop app ("KhataOS acts as: Server"). Others connect over the LAN.
- **Relay:** a `khata-server` stores only entries sealed with `khata-seal` (XChaCha20-Poly1305). The company key travels only in invite codes. Devices validate every pulled entry.
  - **Who runs it:** a firm or its IT person (any always-on PC behind Cloudflare Tunnel, or a VPS behind Caddy), or KhataOS for a transition period. See `docs/deploy-relay.md`.
  - **Relay address:** no address is hard-coded. A build may set one (`KHATA_DEFAULT_RELAY` at build time), users set their own in `sharing.json`, and a company may override it.
  - **Sign-up:** with `KHATA_OPEN_RELAY=1`, anyone may create relay companies, within limits:
    - new companies per client address per day (`X-Forwarded-For` is honoured only with `KHATA_TRUST_PROXY=1`);
    - bytes stored per company (refused with 413);
    - optionally, bytes stored in all (507).

    Limits refuse new data only; the trail stays append-only. Otherwise a sign-up key (the admin key) is required.
  - **Moving relays:** `khata_sync::move_relay` moves a company to another relay. It requires the file to be fully synced, then re-seals the whole trail for the new company id under a **new key**. Invites must be reissued anyway, and the new key keeps holders of the old one out of everything written after the move. It then revokes colleagues' tokens on the old relay, so stale devices fail loudly instead of diverging.
- **Accountant files:** `khata-package` `.khatapkg` files, sealed with a passphrase (Argon2id), are applied all-or-nothing.

## Country neutrality

`khata-core` never matches on a country. Country rules (financial year, currency, tax-ID validation, tax computation, return formats, e-invoicing) belong in **compliance packs**, one crate per jurisdiction, behind the `CompliancePack` trait in `khata-compliance`. Eleven packs ship today: India (`khata-in`), the UAE (`khata-ae`), Saudi Arabia (`khata-sa`), Singapore (`khata-sg`), the UK (`khata-gb`), Germany (`khata-de`), France (`khata-fr`), the Netherlands (`khata-nl`), Spain (`khata-es`), the US (`khata-us`) and Mexico (`khata-mx`), with the shared EN 16931 e-invoice model in `khata-en16931`. The app registers them in `apps/desktop/src-tauri/src/packs.rs`.

The core stores only country-neutral facts: the company's country code (a `profile:company` record in the audit trail) and a party's tax ID with any override reason. The app picks the pack for the country and runs its checks before it changes the books. The server doesn't run packs yet, so a trail pushed by a modified client isn't checked against them.

## Access model

| Role | Read | Write | Manage access |
|---|---|---|---|
| owner | ✓ | ✓ | ✓ |
| accountant (staff, the company's CA) | ✓ | ✓ | |
| viewer (external auditor or advisor) | ✓ | | |

Tokens are scoped to one company, can expire, can be revoked, and are stored only as SHA-256 hashes. A viewer can use the server's read-only report endpoints without installing the app.

## Money

All amounts are `rust_decimal::Decimal`. Floats are never used. Amounts cross API and FFI boundaries as decimal strings.

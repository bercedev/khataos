# Syncing the company's other data (shared records)

The books sync as a hash-chained audit trail (`docs/architecture.md`). A
company also has data that is not accounting entries but still belongs to
the company, not to one computer:

- the accountant's desk: notes and to-dos, "filed" marks, the compliance calendar profile;
- the invoice design, the logo and the signature, and each voucher's ship-to and export details;
- tax settings that change figures, such as UK VAT schemes;
- what a tax authority answered about the company's documents.

These are **shared records**. They sync to co-workers and the company's
accountant on every path the books take:

- the office server (LAN);
- a self-hosted server;
- the end-to-end encrypted relay;
- accountant files (`.khatapkg`).

What belongs to one computer stays in the company file's `settings` table and
never leaves it: sync and relay credentials, cursors, a device's e-invoicing
chain. Credentials are not in the company file at all; they stay in app-data
files.

Code: `khata_store::records` (model, key classes, merge rule),
`khata_sync::sync_records` (client), `khata-server` `records.rs` and
`relay.rs` (server and relay), `khata_package::export_with_records`
(accountant files).

## The design choice: a record channel beside the audit trail

There were two options:

- **(a)** a second, syncable channel next to the trail: one row per key,
  last writer wins;
- **(b)** a generic "annotation" entity in the trail, with a format bump so
  old clients refuse it.

**We chose (a).** The reasons:

1. **Old copies keep working.** `Book::apply_entry` refuses entity kinds it
   doesn't know. With (b), the first note anyone wrote would stop every older
   app and every older server from replaying the books. Even "refuse cleanly
   and say update KhataOS" means the books stop syncing until everyone
   updates. With (a), the trail is byte for byte what it was, so old
   clients and servers sync the books as before and simply don't see the
   records.
2. **Edits to a note don't conflict with the books.** The trail is a single
   line: if two devices both append offline, that is a conflict that needs a
   rebase. A to-do ticked offline would then block a colleague's vouchers.
   Records merge per key without conflict, whatever happened to the books,
   and they sync even while the books are in conflict.
3. **The trail is a statutory edit log, not a store for logos.** The trail
   is append-only and permanent: India's Companies (Accounts) Rule 3(1) and
   GST Rule 56(8) require that the edit log can't be switched off. Each logo
   change would add up to about 2 MB (base64, plus the PNG copy of an SVG) to
   every copy forever. A deleted note could never really be deleted. Its
   400 KB to 2 MB values are also over the relay's limit for one entry
   (512 KiB of hex).
4. **Delete semantics fit.** A tombstone per key gives "a deleted note
   stays deleted". The trail has no clean way to express that across
   devices.

The cost of (a) is that records are not tamper-evident like the trail. That
is acceptable for non-accounting data. The server logs which token pushed
each change (`record_log`). Anything that decides tax figures still comes
from the books.

## Model

One row per key in the company file's `records` table:

| Field | Meaning |
|---|---|
| `key` | e.g. `desk.note.<uuid>`, `invoice.logo` |
| `value` | the JSON the feature stores; `NULL` is a tombstone (deleted) |
| `at` | the last-writer-wins clock, in milliseconds |
| `author` | the app user who made the change |
| `device` | a random id per company file (`records.device`, device-local) |
| `forever` | a deletion that no later write undoes (notes) |
| `rev` | local revision: the order rows changed in *this* file (the sync cursor) |

**Merge rule** (`Record::wins_over`). Versions are compared in one total
order: a deletion forever first, then `at`, then `device`, `author` and
`value` as tie-breakers. A merge keeps the greater version. Every copy that
has seen the same writes therefore holds the same rows, in any order and
however often a version arrives.

**Clocks.** A write's `at` is `max(now, previous.at + 1)`. An edit made after
seeing someone's change always wins over it, even when this computer's clock
is behind.

**Conflicts.** Suppose two people edit the same note offline. The later edit
(by `at`) wins on every copy; there is no conflict to resolve. Suppose one
person deletes a note while another edits it offline. The deletion is
`forever`, so it wins over the edit, even a later one, and the note never
reappears. Only notes delete forever: their keys are new UUIDs and never
reused. Other keys can be cleared and set again, such as a logo removed and
added, or a "filed" mark cleared. Their deletions are plain tombstones
ordered by `at`.

**No version vector.** Per-key last-writer-wins with tombstones converges
without one. Each remote keeps a cursor instead (`record_sync`):

- `pulled`: the remote's revision this file has pulled up to;
- `pushed`: this file's revision it has pushed up to;
- `supported`: whether the remote speaks protocol 2.

## Key classification

Every key the app writes to a company file, and where it lives now.

### Shared company data (syncs as records)

| Key | Written by | What |
|---|---|---|
| `desk.note.<uuid>` | `desk.rs` | a note or to-do (deleted forever) |
| `desk.filed.<code>.<period end>` | `desk.rs` | a return marked filed or paid |
| `desk.calendar` | `desk.rs` | compliance profile and filing pattern |
| `invoice.design` | `invoice_design.rs` | template, colours, terms, titles… |
| `invoice.logo`, `invoice.signature` | `invoice_design.rs` | images, at most 300 KB each, plus an SVG's PNG copy |
| `invoice.voucher.<uuid>` | `invoice_design.rs` | a voucher's ship-to and export details |
| `vat_gb.schemes` | `vat_gb.rs` | UK Flat Rate, Cash and Annual Accounting schemes |
| `gst.gstr1_filed.<YYYY-MM>` | `gst.rs` | GSTR-1 periods marked filed (GSTR-1A candidates) |
| `tds.tan`, `tds.deductor` | `tds.rs` | the deductor's TAN and details for TDS statements |
| `tds.challan.<voucher>` | `tds.rs` | a challan's BSR code, serial and deposit date |
| `us.exempt.<ledger>` | `tax_us.rs` | a customer's resale and exemption certificates |
| `us.use_tax.jurisdiction` | `tax_us.rs` | where purchased goods are used (use tax) |
| `mx.emisor` | `tax_mx.rs` | CFDI issuer details (régimen, lugar de expedición, serie) |
| `zatca.seller` | `zatca.rs` | the seller's CRN and Saudi National Address |
| `irp.irn.<voucher>` | `irp_client.rs` | the IRP's answer (IRN, ack, signed QR, status) |
| `mx.cfdi.<voucher>` | `pac_client.rs` | the PAC's stamp (UUID, seals, XML, status) |
| `mtd.receipt.<env>.<period>` | `mtd_client.rs` | HMRC's VAT submission receipt |

Authority answers are shared so that every copy can print the IRN or the
stamp, and nobody registers or files the same document twice. They hold no
secrets: the credentials that obtained them stay in app data.

### Device-local state (the `settings` table; never synced)

| Key | Why it stays |
|---|---|
| `sync.url`, `sync.company_id`, `sync.last` | where *this* file syncs, and its last status |
| `sync.token` | **credential**: this device's bearer token |
| `share.hosted`, `share.role`, `share.via`, `share.relay_url`, `share.connect_on_load`, `share.interval_minutes` | how this computer shares the company |
| `share.key` | **secret**: the relay company's end-to-end key |
| `package.sent_to`, `package.company_id`, `package.records_sent_to` | accountant-file cursors of this file |
| `records.device` | this file's device id for records |
| `zatca.device`, `zatca.chain`, `zatca.doc.<id>`, `zatca.submission.<id>`, `zatca.queue` | the device's ZATCA EGS unit: CSID record, invoice counter and hash chain, signed documents, queue. Each computer that issues invoices is its own EGS, and sharing these would fork chains. |
| `verifactu.system`, `verifactu.chain`, `verifactu.doc.<id>`, `verifactu.cancel.<id>` | VeriFactu: each installation keeps its own record chain |
| `tds.file_sequence` | a counter of the TDS statement files this computer made |

These prefixes are `khata_store::records::DEVICE_LOCAL_KEYS`. A record with
one of these keys is refused at every step:

- when it is written;
- when it is pushed;
- by the server (422);
- when it is pulled or read from a package (ignored).

### Credentials (outside the company file entirely)

These are kept in app-data files, never in the company file, the trail,
sync or packages:

- IRP API credentials: `irp-credentials.json`;
- PAC credentials: `mx-pac.json`;
- the CSD: `mx-csd/`;
- ZATCA private key and CSIDs: the secrets file;
- MTD OAuth tokens: `<app data>/mtd/`;
- Peppol Access Point settings;
- crash-report settings: `settings.json`;
- device sharing settings: `sharing.json`;
- the embedded server's admin key: `server/admin.key`.

The only secrets in the company file are `sync.token` and `share.key`, and
they are device-local settings.

Tests check this on every path:

- `device_local_and_credential_keys_never_leave_the_device` (`khata-sync/tests/records.rs`) covers the server, the relay and `.khatapkg`;
- the LAN test in `lan.rs` checks that no sync token or device-local key reaches the server PC's store;
- the ZATCA, IRP and accountant-file tests check that their secrets stay out of the company file, its records and packages.

## Protocol

The sync protocol is now **version 2**
(`khata_server::protocol::PROTOCOL_VERSION`). Version 1 was the trail only.

| Route | Who | What |
|---|---|---|
| `GET /v1/companies/{id}/records?since=<rev>` | any token | versions changed after `since`, oldest first, as `{rev, key, value, at, author, device, forever}`; pages of at most 500 records or about 4 MB of values, with `more` |
| `POST /v1/companies/{id}/records` | owner, accountant | `{records: [...]}`, at most 1,000 per push. Each is checked (`Record::check`) and merged with the same rule as on devices. Answers `{rev, changed, protocol}`. A viewer gets **403**; a device-local key or a malformed record gets **422** for the whole push. |
| `GET /v1/relay/{id}/records?since=<rev>` | any token | sealed blobs `{rev, blob}` |
| `POST /v1/relay/{id}/records` | owner, accountant | `{records: ["<hex>", ...]}`, each at most `MAX_RECORD_HEX` (about 6 MiB). The blobs count towards the relay's storage limits. |

`pull` answers (books and relay) now include `"protocol": 2`. Older clients
ignore the field.

**One sync** (`khata_sync::sync_all`, used by `sync`, `sync_sealed`,
rebase, the office server and the background loop):

1. Push the records changed here since `pushed`, in batches of about 4 MB.
2. Pull the records changed on the remote since `pulled`, and merge them.
3. Sync the books, as before.

The records go first, so they reach colleagues even when the books are in
conflict.

**Through the relay** every record is sealed with the company key:
`khata_seal::seal(key, record_context(company_id), record JSON)`. The context
is the company id, a zero byte and `record`, so a record can't pass for an
entry or for another company's record. The relay sees neither keys nor
values, so it can't merge. It appends each blob to its own log
(`relay_records`, append-only), and devices merge what they pull. A relay
that replays an old blob changes nothing, because the newer version wins.
A blob that doesn't open with the company key can't carry data, so it is
skipped and counted (`RecordsOutcome::ignored`) rather than stopping the
channel. A wrong key is still caught, because the books can't be opened
either. Any records error other than the network is reported after the
books have synced, so it never holds them up.

**Size.** A record's value is at most 3 MiB (`MAX_RECORD_VALUE`). The largest
real value is a 300 KB logo plus its 1.2 MB PNG copy, both in base64, about
2 MB. Sealed and in hex, a 3 MiB value is about 6 MiB, under the server's
8 MiB body limit. Pages stay under the client's 10 MB response limit. One
test pushes a record of the full 3 MiB through the relay.

## Older and newer copies

**The audit trail and its hashes are unchanged**, so existing company files,
trails and hashes keep verifying, and old trails replay as before.

- **Existing company files** (schema 1) are migrated once, when they are
  opened (`Store::open`). Their shared rows move from `settings` to `records`
  with clock 0, so any edit made after the upgrade wins over them. The trail
  is not touched. The schema version becomes 2.
- **New app, older server or relay.** The `records` routes are missing, so
  the server answers 404 without a KhataOS error body. The app syncs the
  books as before and keeps the records waiting (`supported = false`). The
  background loop doesn't retry them every second: it waits for the normal
  interval. Once the server is updated, the next sync sends them.
- **Older app, newer server.** It speaks protocol 1 and syncs the books
  unchanged. It doesn't see notes or designs. The extra `protocol` field in
  pull answers is ignored.
- **Older app opening a migrated company file.** The books open as before.
  Notes and designs are no longer in its `settings`, so it shows none. They
  are still in the file, and a newer app shows them again.
- **Accountant files.** A `.khatapkg` keeps its format (`khata-package/1`),
  header and entries. The records are an optional extra field (`records`),
  sealed with the same passphrase key, with the context "header ‖ `\0records`".
  An older app opens a newer file and applies the books, and skips the field
  it doesn't know. A newer app opens older files, which have no records. A
  file with no records is byte for byte the old format.
- **Rebase.** The rebuilt file takes the records with it. They are not
  rebased; they merge.
- **Moving relays.** The records are sealed again under the new key and pushed
  to the new relay, along with any a colleague changed on the old one
  meanwhile.

## Permissions

The app's rules are the same as before:

- **Notes and filed marks:** Data Entry and up can write them. Deleting
  someone else's note needs Accountant.
- **The calendar profile and the TDS details:** Accountant.
- **The invoice design, the images and the VAT schemes:** Owner.
- **The Auditor role** (view only) can read and can't write.
- **A company shared as Viewer** is read-only (`lan::check_writable`), and
  that now also covers US certificates.

The server enforces the token's role for records just as it does for books:

- owner and accountant tokens push;
- viewer tokens pull only, and a push answers 403;
- on the relay, records are refused like entries.

A viewer's copy that changes a record anyway keeps it to itself:
`RecordsOutcome::read_only`.

App users (Owner, Accountant, Data Entry, Auditor) live in the company's
books and are checked by each copy of the app, as for vouchers. The server
knows only tokens.

## Tests

| Path | Test |
|---|---|
| store | `records_last_writer_wins_and_a_note_deleted_forever_stays_deleted`, `merging_converges_in_any_order`, `revisions_page_and_cursors_count_what_is_pending`, `an_older_company_file_moves_its_shared_settings_once_and_keeps_its_trail` (`khata-store`); `keys_are_classified`, `the_order_is_total_and_a_deletion_forever_wins`, `records_are_checked` (`records.rs`) |
| self-host server | `notes_designs_and_a_logo_sync_between_two_devices_through_a_server`, `offline_edits_of_one_note_last_writer_wins_and_a_deleted_note_never_comes_back`, `viewers_read_records_but_cannot_change_them_on_the_server` (`khata-sync/tests/records.rs`); `shared_records_merge_last_writer_wins_and_only_writers_push`, `record_pulls_page_by_count_and_bytes` (`khata-server/tests/api.rs`) |
| relay | `the_relay_sees_nothing_readable_in_shared_records`, `rebasing_and_moving_relays_keep_the_shared_records` (`khata-sync`); `sealed_records_count_towards_the_limits_and_viewers_cannot_push_them` (`relay_limits.rs`); `a_sealed_record_cannot_pass_for_an_entry_or_another_company` (`khata-seal`) |
| office server (LAN) | `notes_and_the_invoice_design_reach_colleagues_over_the_office_server` (`lan.rs`) |
| `.khatapkg` | `shared_records_travel_sealed_and_merge_idempotently`, `older_and_newer_packages_open_both_ways` (`khata-package`); `notes_and_the_invoice_design_travel_in_accountant_files` (`package.rs`) |
| older copies | `an_older_server_syncs_the_books_and_keeps_the_records_waiting`, `an_older_client_syncs_the_books_unchanged_with_a_newer_server` (`khata-sync`), `older_and_newer_packages_open_both_ways`, the migration test above |
| secrets | `device_local_and_credential_keys_never_leave_the_device` (`khata-sync`) |

## Backups

A backup (`.khatabak`, `crates/khata-backup`, `docs/guide/backup.md`) takes
the same line: it carries the books and every shared record, and none of the
device-local state. The snapshot (`Store::snapshot_to`) empties the
`settings`, `sync_state` and `record_sync` tables and drops any record with a
device-local key, then rebuilds the file so no deleted value stays in its
free pages. A restored company is therefore not connected to any server or
relay, and counts as a new device for ZATCA and VeriFactu.

Tests: `a_backup_contains_no_device_local_or_credential_keys` (`khata-backup`)
checks the backup file's bytes, plain and encrypted, for the sync token, the
relay key, server addresses and the device chains.

## Not done yet

- The browser portal (`/portal`) doesn't show notes.
- The connectivity screen counts waiting book entries, not waiting records.
- Records aren't compacted on the relay: each version stays as a sealed
  blob, within the company's storage limit.

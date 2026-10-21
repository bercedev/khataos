# Contributing to KhataOS

Thanks for helping. KhataOS is accounting software, so correctness comes first: a wrong tax figure or a lost voucher hurts real businesses. These rules keep it right.

You can help without writing code too:
- **Test it** as an accountant or a business: see [`docs/testing.md`](docs/testing.md).
- **Check a tax rule** against the official source, and report it with the "Tax rule" issue form.
- **Review translations**: see [`docs/i18n.md`](docs/i18n.md).

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). Report security problems privately, as [`SECURITY.md`](SECURITY.md) explains, never in a public issue.

## Dev setup

You need:
- Rust 1.85 or newer (edition 2024), from [rustup](https://rustup.rs);
- Node 22 and [pnpm](https://pnpm.io);
- on macOS, the Xcode command-line tools;
- on Linux, the WebKitGTK packages Tauri lists, for example on Ubuntu:
  `sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.

```sh
git clone https://github.com/khataos/khataos.git
cd khataos/apps/desktop
pnpm install
pnpm app:dev          # tauri dev with Vite hot reload
```

`cargo test` and `cargo build` work without Node: if `apps/desktop/dist` is missing, `build.rs` writes a placeholder page. Run `pnpm build` to embed the real frontend.

For Android and iOS, see [`docs/mobile.md`](docs/mobile.md).

## Before you open a pull request

Run these from the repository root unless noted. CI runs the Rust checks on Linux, macOS and Windows, and the frontend checks on Linux.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd apps/desktop
pnpm typecheck
pnpm strings --missing     # lists t("…") texts with no Arabic translation
```

The code isn't run through `rustfmt` yet, so don't run `cargo fmt` over the workspace: it would reformat every file. Match the style of the file you're editing, and keep reformatting out of your diff.

Add tests with your change. For tax rules, test against the official examples where the authority publishes them (sample invoices, worked calculations, check-digit examples).

## Code rules

**Money is `rust_decimal::Decimal`.** Never use floats for amounts, rates or quantities. Amounts cross the Tauri boundary as decimal strings, and the frontend never does arithmetic on them as JavaScript numbers.

**The audit trail is the storage format.** Every change to the books is an `AuditEntry`, and every entry goes through `Book::apply_entry`, whether it's a local edit, a replay, a rebase or a server push. Don't write to the store around it, and don't add a second path that changes state. The trail is append-only: history is never edited, only added to. See [`docs/architecture.md`](docs/architecture.md).

**Country rules live in compliance packs, never in the core.** `khata-core` never matches on a country. Financial years, tax IDs, tax computation, invoice numbering rules, returns and e-invoicing belong in a pack crate behind the `CompliancePack` trait in `khata-compliance`. Rates come from an effective-dated `RateSchedule`, not from constants in code.

**Keyboard-first, with Tally key-map parity.** Every action must work from the keyboard. Key bindings are data in `crates/khata-keymap`, resolved per screen context. There is no key table in TypeScript. New screens follow [`docs/ux/interaction-spec.md`](docs/ux/interaction-spec.md): Enter moves on, Esc goes back, Ctrl+A accepts, and so on. Don't break a key that a TallyPrime user expects. If you have to diverge, say why in the PR. After changing `crates/khata-keymap/src/tally.rs`, run `scripts/keymap-doc` to regenerate the key reference in [`docs/guide/keyboard.md`](docs/guide/keyboard.md); a `khata-keymap` test fails until you do. When you build a key that was reserved, change its status in `crates/khata-keymap/examples/keymap-doc.rs` and run the script again. User-facing changes also update the matching page in [`docs/guide/`](docs/guide/README.md).

**Every UI string goes through `t()`.**
- Write the English text in the code as `t("…")`, with `{name}` placeholders for the variable parts. Never build sentences from fragments.
- Add the Arabic to `apps/desktop/src/locales/ar.ts`, one entry per line, with the English character for character. Mark a term you're unsure of with `// ar: check`. Messages from Rust go in `src/locales/ar-errors.ts`.
- Never pass user data (ledger or item names) through `t()`.
- Right-to-left layout comes from CSS logical properties: `margin-inline-start`, `padding-inline`, `inset-inline-end`, `text-align: start`. Don't use `left`, `right` or `margin-left` for anything that follows the reading direction.

The details are in [`docs/i18n.md`](docs/i18n.md).

**Secrets stay out of company files.** Credentials for tax authorities and access points live in owner-only files in the app data folder, never in the company file, its backups or the audit trail.

## Adding a country pack

1. Research first. Write up the rules in `docs/research/<country>-compliance.md`, and tag each fact:
   - **[PRIMARY]**: read in the law, a regulation, or the tax authority's own guidance, form or API specification;
   - **[SECONDARY]**: reported by professional or other non-official sources;
   - **[UNCERTAIN]**: not confirmed.

   Link the official source for every rate, threshold, check digit and form box. Secondary sources are fine for leads, but not as the only basis for a rule.
2. Create a crate, `crates/khata-<iso code>`, that implements `khata_compliance::CompliancePack`: country, currency, regions, financial year, tax-ID label and validation, invoice-number validation, and tax on a line. Add it to `[workspace.dependencies]` in the root `Cargo.toml` and to `apps/desktop/src-tauri/Cargo.toml`.
3. Register it with **one line** in `apps/desktop/src-tauri/src/packs.rs`. The order of `PACKS` is the order of the country picker.
4. Returns, e-invoices and reports for the country go in the pack crate (the rules) and in a module in `apps/desktop/src-tauri/src` (turning vouchers into the pack's documents).
5. Write the user guide, `docs/<country>-<tax>.md`: setting up a company, what is computed, what isn't supported yet, and what still needs checking against the official tools.

## Clean-room rule

KhataOS reproduces **behaviour** that users rely on, such as workflows and key bindings. It never copies TallyPrime, or any other product:
- no code, decompiled or otherwise;
- no screenshots, screen layouts, icons, text, help pages or branding;
- data migration uses only Tally's published XML interface.

Don't contribute anything you saw under a licence or NDA that forbids it, such as a vendor's internal documentation or a paid standard's text. Describe behaviour in your own words. Mention TallyPrime only to describe compatibility ("works like", "imports from").

## Sign your commits (DCO)

KhataOS is licensed under the AGPL-3.0-or-later. To keep the provenance of every contribution clear, each commit must carry a sign-off under the [Developer Certificate of Origin 1.1](https://developercertificate.org):

```sh
git commit -s -m "Check GSTIN state codes against the 2026 master"
```

This adds a line like `Signed-off-by: Your Name <you@example.com>`. It means you wrote the change, or have the right to submit it under the project's licence. Use your real name or a name you're known by, and an e-mail address you can be reached at. To sign off commits you've already made, run `git rebase --signoff main`.

## Pull requests

- Keep one change per PR, and explain what it does and why.
- For a tax rule, link the official source in the PR description and in the code or doc comment.
- Fill in the checklist in the PR template.

## Good first issues

Look for the [`good first issue`](https://github.com/khataos/khataos/labels/good%20first%20issue) label. Some starting points:

- **Silent keys**: a few Tally-compatible keys do nothing and show no message, such as `Ctrl+F` (Autofill) in a voucher, `Ctrl+F` (Filter) in most reports, and `F5`/`F6` in reports. Add them to `NOT_BUILT` in `apps/desktop/src/shell.ts` so they say "isn't available in this version yet", then run `scripts/keymap-doc`.
- **Arabic review**: check the entries marked `// ar: check` in `src/locales/ar.ts` (run `pnpm strings > ar.csv`).
- **Make `pnpm strings --missing` fail CI**: give it a non-zero exit code when a string is missing.
- **Validators in CI**: run the KoSIT validator on generated XRechnung samples, or the PINT Schematron on PINT SG/AE output (see [`docs/germany-vat.md`](docs/germany-vat.md) and [`docs/peppol.md`](docs/peppol.md)).
- **Check the user guide**: follow a page in [`docs/guide/`](docs/guide/README.md) step by step and fix anything that doesn't match the app.
- **Inventory**: reorder levels (see [`docs/roadmap.md`](docs/roadmap.md)).

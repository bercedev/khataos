# KhataOS desktop

Tauri 2 app for the desktop, Android and iOS: a Rust backend (`src-tauri`) over
`khata-core`, `khata-store`, `khata-keymap`, `khata-sync`, `khata-server` (the
built-in office server), `khata-seal`, `khata-package`, `khata-print` and the
country packs, and a Vite + TypeScript frontend with no UI framework.
Behaviour follows [`docs/ux/interaction-spec.md`](../../docs/ux/interaction-spec.md).

## Prerequisites

- Rust (workspace toolchain), Node 22, pnpm.
- macOS: Xcode command-line tools. Linux: the WebKitGTK packages Tauri lists.

## Develop

```sh
cd apps/desktop
pnpm install
pnpm app:dev          # tauri dev with Vite hot reload on http://localhost:1420
```

`app:dev` merges `src-tauri/tauri.dev.conf.json` (devUrl and the Vite dev
server). The base `tauri.conf.json` has no devUrl, so a plain cargo build
embeds `dist/` and runs without a dev server.

## Build

```sh
pnpm build                                  # type-check + Vite bundle into dist/
cargo build -p khataos-desktop              # from the repo root; embeds dist/
./target/debug/khataos-desktop

pnpm app:build                              # installers/bundles via tauri-cli
```

If `dist/` is missing, `build.rs` writes a placeholder page so workspace-wide
`cargo test` works without Node. Run `pnpm build` and rebuild to get the real UI.

## Test

```sh
cargo test -p khataos-desktop               # command-layer tests (temp-file stores)
pnpm typecheck
```

## Features and keys

The user guide ([`docs/guide/`](../../docs/guide/README.md)) describes every screen; its key
reference is generated from the key map. In short:

- **Masters:** C on Home → Ledger, Group, Stock Item, Stock Group, Unit, Godown or Tax Rates;
  A on Home alters them (and voucher numbering). Alt+C in a list field creates a master inline.
- **Vouchers:** F4–F9, Alt+F5, Alt+F6, Alt+F7 switch type; Ctrl+H switches single/double entry
  or item invoice; F2 date.
  A line for a Sundry Debtors/Creditors ledger opens **Bill-wise details** (New Ref, Agst Ref,
  Advance, On Account); the rows must add up to the line. Backspace on the first row returns.
- **Saved vouchers:** Enter on a Day Book or ledger row opens the voucher for alteration;
  Ctrl+A saves, Alt+X cancels (the number stays used), Alt+D deletes (each after a Y/N prompt).
  Alt+X also works on a Day Book row. Ctrl+P prints; Ctrl+E (or Alt+E) opens the Export menu
  (PDF, e-invoice files).
- **Reports:** Day Book (K), Balance Sheet (B), Profit & Loss (P), Report Directory (D) and Go To
  (Alt+G) for Trial Balance, Outstanding Receivables/Payables, Ageing, MSME overdue payables
  (over 45 days) and ledger statements. Ctrl+P prints (system dialog, so Save as PDF works on
  every OS) and Ctrl+E exports CSV.
- **Import (Alt+O):** Tally XML (masters and Day Book together), or an accountant file
  (`.khatapkg`). The Tally result screen lists what was created and every skipped item with its
  reason.
- **Company (Alt+K):** Sync settings (server URL, company UUID, token, kept in this company file
  only), Sync now (also **Ctrl+Alt+S** anywhere). On a conflict you can rebase; a review
  screen lists renumbered vouchers, items needing attention and the archived file.

## Layout

- `src-tauri/src/api.rs`: plain functions behind every command. Amounts are decimal strings.
- `src-tauri/src/commands.rs`: thin `#[tauri::command]` wrappers (snake_case arguments).
- `src/keys.ts`: turns key events into khata-keymap chord strings, and resolves them against
  the binding list from the `keymap` command. There is no key table in TypeScript.
- `src/shell.ts`: zones, screen stack, the single keydown handler, buffering during
  transitions, button bars, messages, prompts.
- `src-tauri/src/sync.rs`: sync settings, Sync now, rebase. `src-tauri/src/export.rs`: Tally
  import, print HTML (khata-print), CSV.
- `src/picker.ts`: the one ListPicker. `src/screens/*`: one file per area, for example startup,
  home, launcher (menus, Find, Go To), ledger, group, inventory, godown, voucher, bills,
  stockjournal, reports, analysis, banking, desk, sharing, signin, settings, invoice_design,
  einvoice, and the country screens (gst, gstr2b, gst_rcm, irp, tds, schedule3, vat*, mtd,
  zatca, gst_sg, peppol, tax_us, tax_mx).
- `src/calc.ts`, `src/calcpanel.ts`: the calculator (Ctrl+N). `src/platform.ts`, `src/mobile.css`:
  the phone layout.

Company files are SQLite (`*.khata`), one per company, by default in the app data
directory under `companies/`.

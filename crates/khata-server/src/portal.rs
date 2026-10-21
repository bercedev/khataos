//! Read-only accountant portal: one self-contained page served at `/portal`.
//!
//! External accountants and advisors open it in a browser, paste the
//! company id and the viewer token they were given, and read the reports.
//! The token stays in this tab's sessionStorage and is sent only to this
//! server. All data is rendered with `textContent` (never HTML).
//!
//! Reports: Trial Balance, P&L, Balance Sheet, Day Book, Ledgers,
//! Outstanding (receivables/payables), Ageing and MSME overdue payables.
//! Keyboard-first like the desktop app: arrows move the row cursor, Enter
//! drills down (P&L/Balance Sheet group → group summary → ledger statement),
//! Backspace/Esc goes back, 1–9 and 0 switch report, `/` searches. Downloads are
//! fetched with the token in a header and handed to the browser as blob:
//! URLs, so the token never appears in a URL or history.

use std::sync::OnceLock;

use axum::http::header;
use axum::response::{Html, IntoResponse};
use sha2::{Digest, Sha256};

/// Serve the page with real security headers. Inline script and style are
/// allowed by hash only, so injected markup could not run.
pub(crate) async fn page() -> impl IntoResponse {
    static CSP: OnceLock<String> = OnceLock::new();
    let csp = CSP.get_or_init(|| {
        // The print stylesheet is allowed too: "Download PDF" opens the
        // print HTML from a blob: URL, which inherits this policy.
        format!(
            "default-src 'none'; script-src '{}'; style-src '{}' '{}'; connect-src 'self'; base-uri 'none'; \
             form-action 'none'; frame-ancestors 'none'",
            hash_of(PAGE, "<script>", "</script>"),
            hash_of(PAGE, "<style>", "</style>"),
            print_style_source(),
        )
    });
    (
        [
            (header::CONTENT_SECURITY_POLICY, csp.as_str()),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(PAGE),
    )
}

/// CSP source (`sha256-…`) for the stylesheet every `khata-print` page
/// embeds. Computed from real output, so it follows changes to that crate.
pub(crate) fn print_style_source() -> &'static str {
    static SRC: OnceLock<String> = OnceLock::new();
    SRC.get_or_init(|| {
        let header = khata_print::Header { company: "", address: "", grouping: khata_core::format::Grouping::Indian };
        let sample = khata_print::trial_balance_html(&header, chrono::NaiveDate::default(), &[], khata_core::Amount::ZERO);
        hash_of(&sample, "<style>", "</style>")
    })
}

/// CSP source for the content between `open` and `close`.
fn hash_of(page: &str, open: &str, close: &str) -> String {
    let start = page.find(open).expect("tag present") + open.len();
    let end = start + page[start..].find(close).expect("closing tag present");
    format!("sha256-{}", base64(&Sha256::digest(&page.as_bytes()[start..end])))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer">
<title>KhataOS Accountant Portal</title>
<style>
:root { --bg:#f7f8fa; --fg:#14181f; --muted:#5b6472; --line:#d9dde4; --card:#fff; --accent:#0f766e; --neg:#b42318; --cur:#e6f4f1; }
@media (prefers-color-scheme: dark) { :root { --bg:#0f1216; --fg:#e6e9ee; --muted:#9aa3b2; --line:#2a313b; --card:#171b21; --accent:#2dd4bf; --neg:#f97066; --cur:#16312d; } }
* { box-sizing: border-box; }
[hidden] { display:none !important; }
body { margin:0; font:14px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; background:var(--bg); color:var(--fg); }
header { padding:10px 16px; border-bottom:1px solid var(--line); display:flex; gap:8px 12px; align-items:center; flex-wrap:wrap; }
header h1 { font-size:16px; margin:0 4px 0 0; }
header .src { margin-left:auto; font-size:12px; color:var(--muted); }
header .ro { font-size:12px; color:var(--muted); border:1px solid var(--line); border-radius:999px; padding:2px 8px; }
main { padding:12px 16px 24px; max-width:1180px; margin:0 auto; }
form, .bar { display:flex; gap:8px; flex-wrap:wrap; align-items:end; margin-bottom:10px; }
.grow { flex:1 1 auto; }
#opts { margin:0; }
label { display:flex; flex-direction:column; font-size:12px; color:var(--muted); gap:4px; }
input { font:inherit; padding:6px 8px; border:1px solid var(--line); border-radius:6px; background:var(--card); color:var(--fg); min-width:0; min-height:34px; }
input[type=number] { width:6em; }
button { font:inherit; min-height:34px; padding:6px 12px; border:1px solid var(--accent); border-radius:6px; background:var(--accent); color:#fff; cursor:pointer; }
button.ghost, button.tab, .crumbs button { background:transparent; color:var(--fg); border-color:var(--line); }
button[aria-selected=true], button[aria-pressed=true] { border-color:var(--accent); color:var(--accent); font-weight:600; }
button:disabled { opacity:.45; cursor:default; }
:focus-visible { outline:2px solid var(--accent); outline-offset:1px; }
.tabs { display:flex; gap:6px; overflow-x:auto; padding-bottom:4px; margin-bottom:10px; scrollbar-width:thin; }
.tabs button { flex:0 0 auto; }
.tabs kbd { font:11px ui-monospace, monospace; color:var(--muted); margin-right:4px; }
.crumbs { display:flex; flex-wrap:wrap; align-items:center; gap:4px; margin:0 0 8px; font-size:13px; }
.crumbs button { min-height:28px; padding:2px 8px; }
.crumbs .here { font-weight:600; }
.crumbs .sep { color:var(--muted); }
.crumbs .period { margin-left:auto; color:var(--muted); }
.card { background:var(--card); border:1px solid var(--line); border-radius:8px; overflow-x:auto; margin-bottom:12px; }
table { width:100%; border-collapse:collapse; }
th, td { padding:6px 10px; border-bottom:1px solid var(--line); text-align:left; white-space:nowrap; }
th { font-size:12px; color:var(--muted); font-weight:600; position:sticky; top:0; background:var(--card); }
td.num, th.num { text-align:right; font-variant-numeric: tabular-nums; }
tr.row { cursor:default; }
tr.go { cursor:pointer; }
tr.go td:first-child { text-decoration:underline dotted var(--muted); text-underline-offset:3px; }
tr.cur td { background:var(--cur); }
tr.cur td:first-child { box-shadow:inset 3px 0 0 var(--accent); }
tr.total td { font-weight:700; border-top:2px solid var(--fg); }
tr.muted td { color:var(--muted); }
#out:focus { outline:none; }
.err { color:var(--neg); margin:8px 0; }
.muted { color:var(--muted); }
.note { color:var(--muted); font-size:12px; margin:0 0 8px; }
.hint { color:var(--muted); font-size:12px; margin-top:8px; }
.split { display:grid; grid-template-columns:1fr 1fr; gap:12px; }
@media (max-width:720px) {
  .split { grid-template-columns:1fr; }
  main { padding:10px 12px 20px; }
  th, td { padding:6px 8px; }
  header .src { margin-left:0; }
}
@media (hover:none) { .hint, .tabs kbd { display:none; } }
@media print {
  header, .tabs, .bar, .hint, #error { display:none !important; }
  .crumbs button { border:none; padding:0; }
  .card { border:none; overflow:visible; }
  tr.cur td { background:none; box-shadow:none; }
  th { position:static; }
}
</style>
</head>
<body>
<header><h1>KhataOS</h1><span class="ro">Read-only accountant access</span><span id="company" class="muted"></span><a class="src" href="/source" rel="noopener">Source code (AGPL-3.0)</a></header>
<main>
  <form id="login">
    <label>Company ID<input id="cid" autocomplete="off" required size="38"></label>
    <label>Access token<input id="tok" type="password" autocomplete="off" required size="40"></label>
    <button type="submit">Open</button>
  </form>
  <div id="app" hidden>
    <nav class="tabs" role="tablist" aria-label="Reports">
      <button class="tab" role="tab" data-r="tb"><kbd>1</kbd>Trial Balance</button>
      <button class="tab" role="tab" data-r="pl"><kbd>2</kbd>Profit &amp; Loss</button>
      <button class="tab" role="tab" data-r="bs"><kbd>3</kbd>Balance Sheet</button>
      <button class="tab" role="tab" data-r="db"><kbd>4</kbd>Day Book</button>
      <button class="tab" role="tab" data-r="ledgers"><kbd>5</kbd>Ledgers</button>
      <button class="tab" role="tab" data-r="out"><kbd>6</kbd>Outstanding</button>
      <button class="tab" role="tab" data-r="ageing"><kbd>7</kbd>Ageing</button>
      <button class="tab" role="tab" data-r="msme"><kbd>8</kbd>MSME</button>
      <button class="tab" role="tab" data-r="stock"><kbd>9</kbd>Stock Summary</button>
      <button class="tab" role="tab" data-r="ratios"><kbd>0</kbd>Ratios</button>
    </nav>
    <div class="bar">
      <label>From<input id="from" type="date"></label>
      <label>To / as at<input id="to" type="date"></label>
      <label>Find<input id="filter" type="search" placeholder="/ to search" autocomplete="off" size="14"></label>
      <span id="opts" class="bar"></span>
      <span class="grow"></span>
      <button id="pdf" type="button" class="ghost">Download PDF</button>
      <button id="csv" type="button" class="ghost">Download CSV</button>
      <button id="audit" type="button" class="ghost" title="Every change with author, time and pushing token">Audit trail CSV</button>
      <button id="signout" type="button" class="ghost">Sign out</button>
    </div>
    <nav id="crumbs" class="crumbs" aria-label="Breadcrumb"></nav>
  </div>
  <div id="error" class="err" role="alert"></div>
  <div id="out" tabindex="-1" aria-live="polite"></div>
  <p id="hint" class="hint" hidden>↑ ↓ move · Enter or click drills down · Backspace / Esc goes back · 1–9, 0 switch report · / search</p>
</main>
<script>
"use strict";
const $ = (id) => document.getElementById(id);
const el = (tag, text, cls) => { const e = document.createElement(tag); if (text != null) e.textContent = text; if (cls) e.className = cls; return e; };

// Indian digit grouping on decimal strings (no floating point).
function amt(s) {
  if (s == null) return "";
  let str = String(s), neg = str.startsWith("-");
  if (neg) str = str.slice(1);
  let [i, f = ""] = str.split(".");
  f = (f + "00").slice(0, 2);
  if (i.length > 3) { const last = i.slice(-3); let rest = i.slice(0, -3); const parts = []; while (rest.length > 2) { parts.unshift(rest.slice(-2)); rest = rest.slice(0, -2); } if (rest) parts.unshift(rest); i = parts.join(",") + "," + last; }
  return (neg ? "-" : "") + i + "." + f;
}
const isZero = (s) => /^-?0*\.?0*$/.test(String(s));
const abs = (s) => String(s).replace(/^-/, "");
const blank = (s) => (isZero(s) ? "" : amt(s));
// Signed balance (debit positive) as "2,50,000.00 Dr" / "Cr" / "Nil".
const bal = (s) => (isZero(s) ? "Nil" : String(s).startsWith("-") ? amt(abs(s)) + " Cr" : amt(s) + " Dr");
const sum = (xs) => { let neg = 0n; for (const x of xs) { const [i, f = ""] = String(x).replace("-", "").split("."); const v = BigInt(i || "0") * 100n + BigInt((f + "00").slice(0, 2)); neg += String(x).startsWith("-") ? -v : v; } const s = neg < 0n ? "-" : ""; const a = neg < 0n ? -neg : neg; return s + (a / 100n) + "." + String(a % 100n).padStart(2, "0"); };
const fmtDate = (d) => { if (!d) return ""; const [y, m, day] = d.split("-"); return `${+day}-${["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"][+m - 1]}-${y}`; };
const kindName = (k) => String(k).replace(/([a-z])([A-Z])/g, "$1 $2");

const TABS = ["tb", "pl", "bs", "db", "ledgers", "out", "ageing", "msme", "stock", "ratios"];
const TITLES = { tb: "Trial Balance", pl: "Profit & Loss", bs: "Balance Sheet", db: "Day Book", ledgers: "Ledgers", out: "Outstanding", ageing: "Ageing", msme: "MSME Payables", stock: "Stock Summary", ratios: "Ratio Analysis" };
const state = { cid: sessionStorage.getItem("cid") || "", tok: sessionStorage.getItem("tok") || "", stack: [], cursor: 0 };
let sel = []; // selectable rows of the current view: { tr, go, text }
let gen = 0;  // ignores responses for views the user already left

const pad = (n) => String(n).padStart(2, "0");
const iso = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const today = new Date();
$("from").value = `${today.getMonth() >= 3 ? today.getFullYear() : today.getFullYear() - 1}-04-01`;
$("to").value = iso(today);

const query = (o) => Object.entries(o).filter(([, v]) => v != null && v !== "").map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&");
const apiUrl = (path, params) => `/v1/companies/${encodeURIComponent(state.cid)}/${path}?${query(params || {})}`;
const authHeaders = () => ({ Authorization: `Bearer ${state.tok}` });
class ApiErr extends Error { constructor(status, msg) { super(msg); this.status = status; } }
async function failure(r) {
  const body = await r.json().catch(() => ({}));
  return new ApiErr(r.status, r.status === 401 ? "Access token is invalid, expired or revoked." : (body.error || `HTTP ${r.status}`));
}
async function api(path, params) {
  const r = await fetch(apiUrl(path, params), { headers: authHeaders(), cache: "no-store" });
  if (!r.ok) throw await failure(r);
  return r.json();
}

// What each view fetches; `pdf` marks reports with a print layout.
function endpoint(v) {
  const from = $("from").value, to = $("to").value, id = encodeURIComponent(v.id || "");
  switch (v.kind) {
    case "tb": return { path: "reports/trial-balance", params: { as_at: to }, pdf: true, asAt: true };
    case "pl": return { path: "reports/profit-and-loss", params: { from, to }, pdf: true };
    case "bs": return { path: "reports/balance-sheet", params: { as_at: to }, pdf: true, asAt: true };
    case "db": return { path: "reports/day-book", params: { from, to }, pdf: true };
    case "ledgers": return { path: "ledgers", params: { as_at: to }, asAt: true };
    case "out": return { path: "reports/outstanding", params: { as_at: to, side: v.side }, asAt: true };
    case "ageing": return { path: "reports/ageing", params: { as_at: to, receivable: v.side === "receivable" }, asAt: true };
    case "msme": return { path: "reports/msme-overdue", params: { as_at: to, days: v.days }, asAt: true };
    case "stock": return { path: "reports/stock-summary", params: { from, to } };
    case "ratios": return { path: "reports/ratios", params: { from, to } };
    case "group": return v.period ? { path: `groups/${id}/summary`, params: { from, to } } : { path: `groups/${id}/summary`, params: { as_at: to }, asAt: true };
    case "ledger": return { path: `ledgers/${id}/statement`, params: { from, to }, pdf: true };
  }
  return null;
}

const ledgerView = (id, title) => ({ kind: "ledger", id, title });
const groupView = (id, title, period) => ({ kind: "group", id, title, period });
const current = () => state.stack[state.stack.length - 1];

// A table whose body rows join the keyboard cursor; `go` drills down.
function table(heads, num, items, total) {
  const t = el("table"), thead = el("thead"), hr = el("tr"), tb = el("tbody");
  heads.forEach((h, i) => hr.append(el("th", h, num.includes(i) ? "num" : "")));
  thead.append(hr); t.append(thead, tb);
  const add = (cells, cls) => { const r = el("tr", null, cls); cells.forEach((c, i) => r.append(el("td", c, num.includes(i) ? "num" : ""))); tb.append(r); return r; };
  for (const it of items) {
    if (it.plain) { add(it.cells, it.cls || ""); continue; }
    const tr = add(it.cells, it.go ? "row go" : "row");
    const entry = { tr, go: it.go, text: it.cells.join(" ").toLowerCase() };
    sel.push(entry);
    tr.addEventListener("click", () => { setCursor(visible().indexOf(entry)); $("out").focus({ preventScroll: true }); if (entry.go) entry.go(); });
  }
  if (total) add(total, "total");
  const card = el("div", null, "card"); card.append(t); return card;
}
const empty = (text) => el("p", text, "muted");

function renderView(v, d) {
  switch (v.kind) {
    case "tb":
      if (!d.rows.length && isZero(d.opening_stock)) return empty("No balances.");
      {
        // Opening stock is a debit of its own: part of the opening balances, no ledger's.
        const rows = d.rows.map((r) => ({ cells: [r.name, blank(r.debit), blank(r.credit)], go: () => push(ledgerView(r.ledger, r.name)) }));
        if (!isZero(d.opening_stock)) rows.push({ cells: ["Opening Stock", amt(d.opening_stock), ""] });
        return table(["Particulars", "Debit", "Credit"], [1, 2], rows,
          ["Grand Total", amt(sum([...d.rows.map((r) => r.debit), d.opening_stock])), amt(sum(d.rows.map((r) => r.credit)))]);
      }
    case "pl": {
      // Opening stock heads the expenses, closing stock ends the incomes.
      const side = (title, rows, stockLabel, stock, first) => {
        const s = isZero(stock) ? [] : [{ cells: [stockLabel, amt(stock)] }];
        const groups = rows.map((r) => ({ cells: [r.name, amt(r.amount)], go: () => push(groupView(r.group, r.name, true)) }));
        return table([title, "Amount"], [1], first ? [...s, ...groups] : [...groups, ...s],
          ["Total", amt(sum([stock, ...rows.map((r) => r.amount)]))]);
      };
      const node = el("div"), g = el("div", null, "split");
      g.append(side("Expenses", d.expenses, "Opening Stock", d.opening_stock, true),
        side("Incomes", d.incomes, "Closing Stock", d.closing_stock, false));
      const net = String(d.net_profit).startsWith("-") ? ["Net Loss", amt(abs(d.net_profit))] : ["Net Profit", amt(d.net_profit)];
      node.append(g, table(["Result", "Amount"], [1], [{ plain: true, cells: ["Gross profit", amt(d.gross_profit)] }], net));
      return node;
    }
    case "bs": {
      const liab = d.liabilities.map((r) => ({ cells: [r.name, amt(r.amount)], go: () => push(groupView(r.group, r.name, false)) }));
      liab.push({ cells: ["Profit & Loss A/c", amt(d.profit_and_loss)], go: () => push({ kind: "pl", title: "Profit & Loss" }) });
      if (!isZero(d.opening_difference)) liab.push({ cells: ["Difference in opening balances", amt(d.opening_difference)] });
      const assets = d.assets.map((r) => ({ cells: [r.name, amt(r.amount)], go: () => push(groupView(r.group, r.name, false)) }));
      if (!isZero(d.closing_stock)) assets.push({ cells: ["Closing Stock", amt(d.closing_stock)] });
      const node = el("div", null, "split");
      node.append(table(["Liabilities", "Amount"], [1], liab, ["Total", amt(d.total_liabilities)]),
        table(["Assets", "Amount"], [1], assets, ["Total", amt(d.total_assets)]));
      return node;
    }
    case "db":
      if (!d.length) return empty("No vouchers in this period.");
      return table(["Date", "Particulars", "Vch Type", "Vch No.", "Debit", "Credit"], [4, 5],
        d.map((r) => ({ cells: [fmtDate(r.date), r.cancelled ? `${r.particulars} (cancelled)` : r.particulars, kindName(r.kind), r.number, blank(r.debit), blank(r.credit)] })));
    case "ledgers":
      if (!d.length) return empty("No ledgers.");
      return table(["Ledger", "Group", "Closing Balance"], [2], d.map((r) => ({ cells: [r.name, r.group_name, bal(r.balance)], go: () => push(ledgerView(r.id, r.name)) })));
    case "group": {
      const node = el("div");
      node.append(el("p", `${d.nature} · amounts on the group's natural side`, "note"));
      node.append(d.rows.length ? table(["Particulars", "", "Amount"], [2],
        d.rows.map((r) => ({ cells: [r.name, r.kind === "group" ? "Group" : "", amt(r.amount)], go: () => push(r.kind === "group" ? groupView(r.id, r.name, v.period) : ledgerView(r.id, r.name)) })),
        ["Total", "", amt(d.total)]) : empty("Nothing under this group for the period."));
      return node;
    }
    case "ledger": {
      const items = [{ cells: [fmtDate($("from").value), "Opening Balance", "", "", "", "", bal(d.opening)] }];
      for (const r of d.rows) items.push({ cells: [fmtDate(r.date), r.particulars, kindName(r.kind), r.number, blank(r.debit), blank(r.credit), bal(r.balance)] });
      const node = el("div");
      node.append(el("p", `Ledger: ${d.name}`, "note"), table(["Date", "Particulars", "Vch Type", "Vch No.", "Debit", "Credit", "Balance"], [4, 5, 6], items,
        ["", "Closing Balance", "", "", amt(sum(d.rows.map((r) => r.debit))), amt(sum(d.rows.map((r) => r.credit))), bal(d.closing)]));
      return node;
    }
    case "out": case "msme": {
      if (!d.length) return empty(v.kind === "msme" ? `No payables older than ${v.days} days.` : "No open bills.");
      const node = el("div");
      if (v.kind === "msme") node.append(el("p", `Payable bills unpaid more than ${v.days} days after the bill date (India, Income-tax Act s.43B(h): 15 days, or up to 45 by written agreement). Check which suppliers are micro or small enterprises.`, "note"));
      node.append(table(["Party", "Reference", "Bill Date", "Due Date", "Amount", "Overdue Days"], [4, 5],
        d.map((b) => ({ cells: [b.party, b.reference, fmtDate(b.date), fmtDate(b.due), amt(abs(b.amount)), String(b.overdue_days)], go: () => push(ledgerView(b.ledger, b.party)) })),
        ["Total", "", "", "", amt(abs(sum(d.map((b) => b.amount)))), ""]));
      return node;
    }
    case "stock": {
      if (!d.rows.length) return empty("No stock items.");
      const qty = (q, u) => (isZero(q) ? "" : `${q} ${u}`);
      return table(["Stock Group", "Item", "Opening", "Inward", "Outward", "Closing Qty", "Closing Value"], [2, 3, 4, 5, 6],
        d.rows.map((r) => ({ cells: [r.group || "Primary", r.name, qty(r.opening_qty, r.unit), qty(r.inward_qty, r.unit), qty(r.outward_qty, r.unit), qty(r.closing_qty, r.unit), blank(r.closing_value)] })),
        ["Grand Total", "", "", "", "", "", amt(d.closing_value)]);
    }
    case "ratios": {
      const r = (x, suffix = "") => (x == null ? "—" : `${x}${suffix}`);
      const node = el("div", null, "split");
      node.append(
        table(["Principal Groups", "Amount"], [1], [
          ["Working Capital", d.working_capital], ["Cash-in-Hand", d.cash_in_hand], ["Bank Accounts", d.bank_accounts],
          ["Bank OD A/c", d.bank_od], ["Sundry Debtors", d.sundry_debtors], ["Sundry Creditors", d.sundry_creditors],
          ["Sales Accounts", d.sales], ["Purchase Accounts", d.purchases], ["Stock-in-Hand", d.stock_in_hand], ["Net Profit", d.net_profit],
        ].map(([n, v]) => ({ cells: [n, amt(v)] }))),
        table(["Principal Ratios", "Value"], [1], [
          ["Current Ratio", r(d.current_ratio, " : 1")], ["Quick Ratio", r(d.quick_ratio, " : 1")], ["Debt/Equity Ratio", r(d.debt_equity, " : 1")],
          ["Gross Profit %", r(d.gross_profit_pct, " %")], ["Net Profit %", r(d.net_profit_pct, " %")], ["Operating Cost %", r(d.operating_cost_pct, " %")],
          ["Receivable Turnover", r(d.receivable_days, " days")], ["Return on Investment %", r(d.return_on_investment_pct, " %")],
          ["Return on Working Capital %", r(d.return_on_working_capital_pct, " %")],
          ["Working Capital Turnover", r(d.working_capital_turnover)], ["Inventory Turnover", r(d.inventory_turnover)],
        ].map(([n, v]) => ({ cells: [n, v] }))));
      return node;
    }
    case "ageing":
      if (!d.length) return empty("Nothing outstanding.");
      return table(["Party", "0–30 days", "31–60 days", "61–90 days", "Over 90 days", "Total"], [1, 2, 3, 4, 5],
        d.map((r) => ({ cells: [r.party, ...r.buckets.map(blank), amt(r.total)], go: () => push(ledgerView(r.ledger, r.party)) })),
        ["Total", ...[0, 1, 2, 3].map((i) => blank(sum(d.map((r) => r.buckets[i])))), amt(sum(d.map((r) => r.total)))]);
  }
  return empty("");
}

// Per-view options: receivable/payable switch, MSME window.
function renderOpts(v) {
  const opts = $("opts"); opts.replaceChildren();
  if (v.kind === "out" || v.kind === "ageing") {
    for (const [side, label] of [["receivable", "Receivables"], ["payable", "Payables"]]) {
      const b = el("button", label, "ghost"); b.type = "button"; b.setAttribute("aria-pressed", v.side === side);
      b.addEventListener("click", () => { v.side = side; load(); });
      opts.append(b);
    }
  } else if (v.kind === "msme") {
    const l = el("label", "Days after bill"), i = el("input"); i.type = "number"; i.min = "0"; i.max = "3650"; i.value = v.days;
    i.addEventListener("change", () => { const n = Number.parseInt(i.value, 10); if (n >= 0 && n <= 3650) { v.days = n; load(); } });
    l.append(i); opts.append(l);
  }
}

function renderCrumbs(ep) {
  const c = $("crumbs"); c.replaceChildren();
  state.stack.forEach((v, i) => {
    if (i) c.append(el("span", "›", "sep"));
    if (i === state.stack.length - 1) { c.append(el("span", v.title, "here")); return; }
    const b = el("button", v.title); b.type = "button";
    b.addEventListener("click", () => { state.stack.length = i + 1; state.cursor = v.cursor || 0; load(); });
    c.append(b);
  });
  const from = $("from").value, to = $("to").value;
  c.append(el("span", ep.asAt ? `As at ${fmtDate(to)}` : `${fmtDate(from)} to ${fmtDate(to)}`, "period"));
}

async function load() {
  const v = current(), ep = endpoint(v), my = ++gen;
  $("error").textContent = "";
  document.querySelectorAll("button.tab").forEach((b) => b.setAttribute("aria-selected", b.dataset.r === state.stack[0].kind));
  $("pdf").disabled = !ep.pdf;
  renderOpts(v); renderCrumbs(ep);
  const out = $("out");
  out.replaceChildren(el("p", "Loading…", "muted"));
  try {
    const data = await api(ep.path, ep.params);
    if (my !== gen) return;
    sel = [];
    out.replaceChildren(renderView(v, data));
    applyFilter(state.cursor);
  } catch (e) {
    if (my !== gen) return;
    out.replaceChildren(); $("error").textContent = e.message;
    if (e.status === 401) signOut();
  }
}

// Keyboard cursor over the visible rows.
const visible = () => sel.filter((s) => !s.tr.hidden);
function setCursor(i) {
  const vis = visible();
  sel.forEach((s) => { s.tr.classList.remove("cur"); s.tr.removeAttribute("aria-current"); });
  if (!vis.length) { state.cursor = 0; return; }
  state.cursor = Math.max(0, Math.min(i, vis.length - 1));
  const tr = vis[state.cursor].tr;
  tr.classList.add("cur"); tr.setAttribute("aria-current", "true");
  tr.scrollIntoView({ block: "nearest" });
}
function applyFilter(cursor) {
  const q = $("filter").value.trim().toLowerCase();
  sel.forEach((s) => { s.tr.hidden = q !== "" && !s.text.includes(q); });
  document.querySelectorAll("#out tr.total").forEach((t) => { t.hidden = q !== ""; });
  setCursor(cursor || 0);
}

function push(v) { current().cursor = state.cursor; state.stack.push(v); state.cursor = 0; $("filter").value = ""; load(); }
function back() { if (state.stack.length < 2) return; state.stack.pop(); state.cursor = current().cursor || 0; $("filter").value = ""; load(); }
function openTab(kind) {
  const v = { kind, title: TITLES[kind] };
  if (kind === "out" || kind === "ageing") v.side = "receivable";
  if (kind === "msme") v.days = 45;
  state.stack = [v]; state.cursor = 0; $("filter").value = ""; load(); $("out").focus({ preventScroll: true });
}

// Downloads go through fetch so the token stays in a header, never a URL.
async function download(format, ep, fallbackName) {
  $("error").textContent = "";
  const pdf = format === "pdf-html";
  const w = pdf ? window.open("", "_blank") : null; // opened now, while the click still counts
  if (pdf && !w) { $("error").textContent = "Allow pop-ups for this page to open the printable report."; return; }
  if (w) w.document.title = "Preparing report…";
  try {
    const r = await fetch(apiUrl(ep.path, { ...ep.params, format }), { headers: authHeaders(), cache: "no-store" });
    if (!r.ok) throw await failure(r);
    const blob = new Blob([await r.arrayBuffer()], { type: pdf ? "text/html;charset=utf-8" : "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    setTimeout(() => URL.revokeObjectURL(url), 120000);
    if (w) {
      w.location.replace(url);
      let tries = 0;
      const timer = setInterval(() => {
        try {
          if (w.closed || ++tries > 150) { clearInterval(timer); return; }
          if (w.location.href === url && w.document.readyState === "complete") { clearInterval(timer); w.focus(); w.print(); }
        } catch (_) { clearInterval(timer); }
      }, 100);
    } else {
      const m = /filename="([^"]+)"/.exec(r.headers.get("content-disposition") || "");
      const a = el("a"); a.href = url; a.download = m ? m[1] : fallbackName; a.hidden = true;
      document.body.append(a); a.click(); a.remove();
    }
  } catch (e) {
    if (w) w.close();
    $("error").textContent = e.message;
    if (e.status === 401) signOut();
  }
}

function showApp() { $("login").hidden = true; $("app").hidden = false; $("hint").hidden = false; $("company").textContent = "Company " + state.cid; openTab("tb"); }
function signOut() { sessionStorage.clear(); state.tok = ""; gen++; $("login").hidden = false; $("app").hidden = true; $("hint").hidden = true; $("company").textContent = ""; }

$("login").addEventListener("submit", (e) => { e.preventDefault(); state.cid = $("cid").value.trim(); state.tok = $("tok").value.trim(); sessionStorage.setItem("cid", state.cid); sessionStorage.setItem("tok", state.tok); $("tok").value = ""; showApp(); });
$("signout").addEventListener("click", () => { signOut(); $("out").replaceChildren(); $("error").textContent = ""; });
document.querySelectorAll("button.tab").forEach((b) => b.addEventListener("click", () => openTab(b.dataset.r)));
$("from").addEventListener("change", load); $("to").addEventListener("change", load);
$("filter").addEventListener("input", () => applyFilter(0));
$("pdf").addEventListener("click", () => download("pdf-html", endpoint(current()), "report.html"));
$("csv").addEventListener("click", () => download("csv", endpoint(current()), "report.csv"));
$("audit").addEventListener("click", () => download("csv", { path: "reports/audit-trail", params: {} }, "audit-trail.csv"));

document.addEventListener("keydown", (e) => {
  if ($("app").hidden || e.altKey || e.ctrlKey || e.metaKey) return;
  const t = e.target;
  if (t.matches("input, select, textarea")) {
    if (t.id === "filter" && (e.key === "ArrowDown" || e.key === "Enter")) { e.preventDefault(); $("out").focus({ preventScroll: true }); applyFilter(0); }
    else if (e.key === "Escape") { e.preventDefault(); if (t.id === "filter" && t.value) { t.value = ""; applyFilter(0); } else { t.blur(); $("out").focus({ preventScroll: true }); } }
    return;
  }
  if ((e.key === "Enter" || e.key === " ") && t.matches("button, a")) return;
  const vis = visible();
  switch (e.key) {
    case "ArrowDown": setCursor(state.cursor + 1); break;
    case "ArrowUp": setCursor(state.cursor - 1); break;
    case "PageDown": setCursor(state.cursor + 10); break;
    case "PageUp": setCursor(state.cursor - 10); break;
    case "Home": setCursor(0); break;
    case "End": setCursor(vis.length - 1); break;
    case "Enter": { const s = vis[state.cursor]; if (s && s.go) s.go(); break; }
    case "Backspace": case "Escape": back(); break;
    case "/": $("filter").focus(); break;
    default:
      if (/^[0-9]$/.test(e.key)) { openTab(TABS[(Number(e.key) + 9) % 10]); break; }
      return;
  }
  e.preventDefault();
});

if (state.cid && state.tok) showApp();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}

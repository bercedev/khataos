#!/usr/bin/env node
// Contrast check for every theme in both modes (src/themes.css).
//
//   node scripts/theme-check.mjs          table of results; exits 1 on a failure
//   node scripts/theme-check.mjs --json   the same results as JSON (branding/themes/index.html)
//
// It reads the real stylesheet: a small cascade emulator applies the rules
// whose selector matches each theme / mode / region (":root", attribute
// selectors and :not(...) only), most specific and latest last, then
// resolves var(), color-mix(in srgb, …) and rgba() and computes WCAG 2.x
// contrast. Translucent colours are composited over the surface they sit on.
//
// Required (a failure exits 1):
//   WCAG 2.2 AA. Text 4.5:1 (Comfort: 7:1 for body text). 1.4.11: UI parts
//   3:1, which covers field borders against every surface a field sits on,
//   the top-bar search box, chart series and the brand mark. 2.4.11 / 2.4.13:
//   a focus ring at least 2 px wide and 3:1 against what it sits on. Every
//   colour token must be set in both modes, and Dr/Cr, profit/loss and chart
//   series must still be told apart under protan, deutan and tritan simulation.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const CSS = readFileSync(join(here, "../src/themes.css"), "utf8");
/** The brand symbol's fills (src/brand/): fixed colours, not tokens. */
const fills = (f) => [...readFileSync(join(here, "../src/brand", f), "utf8").matchAll(/fill="(#[0-9a-f]{3,6})"/gi)].map((m) => m[1]);
const MARK = { light: fills("symbol.svg"), dark: fills("symbol-dark.svg") };
const asJson = process.argv.includes("--json");

// ------------------------------------------------------------ parse

/** Top-level rules (not inside @media: those are first-paint fallbacks only). */
function parseRules(src) {
  const text = src.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [];
  let i = 0;
  let order = 0;
  while (i < text.length) {
    const open = text.indexOf("{", i);
    if (open < 0) break;
    const head = text.slice(i, open).trim();
    // Find the matching close brace.
    let depth = 1;
    let j = open + 1;
    while (j < text.length && depth) {
      if (text[j] === "{") depth++;
      else if (text[j] === "}") depth--;
      j++;
    }
    const body = text.slice(open + 1, j - 1);
    if (!head.startsWith("@")) {
      const decls = {};
      for (const m of body.matchAll(/(--[\w-]+|color-scheme)\s*:\s*([^;]+);?/g)) decls[m[1]] = m[2].trim();
      for (const sel of head.split(",").map((s) => s.trim())) rules.push({ sel, decls, order: order++ });
    }
    i = j;
  }
  return rules;
}

/** Does `sel` match a root with these attributes? Returns specificity or -1. */
function match(sel, attrs) {
  if (!sel.startsWith(":root")) return -1;
  let rest = sel.slice(5);
  let spec = 1;
  const attrOk = (name, val) => (val === undefined ? attrs[name] !== undefined : attrs[name] === val);
  while (rest.length) {
    let m;
    if ((m = rest.match(/^\[([\w-]+)(?:="([^"]*)")?\]/))) {
      if (!attrOk(m[1], m[2])) return -1;
    } else if ((m = rest.match(/^:not\(\[([\w-]+)(?:="([^"]*)")?\]\)/))) {
      if (attrOk(m[1], m[2])) return -1;
    } else return -1; // :lang(), descendants and the rest: not a theme rule
    spec++;
    rest = rest.slice(m[0].length);
  }
  return spec;
}

function tokensFor(rules, attrs) {
  const hits = rules
    .map((r) => ({ r, spec: match(r.sel, attrs) }))
    .filter((x) => x.spec >= 0)
    .sort((a, b) => a.spec - b.spec || a.r.order - b.r.order);
  const out = {};
  const from = {};
  for (const { r } of hits) {
    Object.assign(out, r.decls);
    for (const k of Object.keys(r.decls)) from[k] = r.sel;
  }
  return { tok: out, from };
}

// ------------------------------------------------------------ colour

function splitArgs(s) {
  const out = [];
  let depth = 0;
  let cur = "";
  for (const ch of s) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (ch === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

/** A CSS colour as [r, g, b, a] (0–255, alpha 0–1), resolving var() against `tok`. */
function color(value, tok, seen = new Set()) {
  const v = value.trim();
  let m;
  if ((m = v.match(/^var\((--[\w-]+)\s*(?:,\s*(.+))?\)$/))) {
    if (seen.has(m[1])) throw new Error(`var loop at ${m[1]}`);
    if (tok[m[1]] !== undefined) return color(tok[m[1]], tok, new Set([...seen, m[1]]));
    if (m[2]) return color(m[2], tok, seen);
    throw new Error(`undefined ${m[1]}`);
  }
  if (v === "transparent") return [0, 0, 0, 0];
  if ((m = v.match(/^#([0-9a-f]{3,8})$/i))) {
    let h = m[1];
    if (h.length <= 4) h = [...h].map((c) => c + c).join("");
    const n = [0, 2, 4, 6].map((k) => (k < h.length ? parseInt(h.slice(k, k + 2), 16) : 255));
    return [n[0], n[1], n[2], n[3] / 255];
  }
  if ((m = v.match(/^rgba?\((.+)\)$/))) {
    const p = m[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return [p[0], p[1], p[2], p[3] ?? 1];
  }
  if ((m = v.match(/^color-mix\(in srgb,\s*(.+)\)$/))) {
    // A percentage may itself be a token (--tint-mix).
    const args = m[1].replace(/var\((--[\w-]+)\)/g, (all, n) => (/^[\d.]+%$/.test(tok[n] ?? "") ? tok[n] : all));
    const [a, b] = splitArgs(args);
    const pa = a.match(/^(.*?)\s+([\d.]+)%$/);
    const pb = b.match(/^(.*?)\s+([\d.]+)%$/);
    const ca = color(pa ? pa[1] : a, tok, seen);
    const cb = color(pb ? pb[1] : b, tok, seen);
    let wa = pa ? +pa[2] / 100 : pb ? 1 - +pb[2] / 100 : 0.5;
    let wb = pb ? +pb[2] / 100 : 1 - wa;
    const sum = wa + wb;
    wa /= sum;
    wb /= sum;
    // Premultiplied alpha, as the spec mixes.
    const alpha = ca[3] * wa + cb[3] * wb;
    const ch = [0, 1, 2].map((k) => (alpha ? (ca[k] * ca[3] * wa + cb[k] * cb[3] * wb) / alpha : 0));
    return [...ch, alpha];
  }
  throw new Error(`can't read colour "${v}"`);
}

const over = (fg, bg) => [0, 1, 2].map((k) => fg[k] * fg[3] + bg[k] * (1 - fg[3])).concat(1);
const lin = (c) => {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};
const lum = (c) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};
const hex = (c) => "#" + c.slice(0, 3).map((n) => Math.round(n).toString(16).padStart(2, "0")).join("");

// Colour-vision simulation (Machado, Oliveira & Fernandes 2009, severity 1), in linear RGB.
const CVD = {
  protan: [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
  deutan: [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.01182, 0.04294, 0.968881]],
  tritan: [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.3039]],
};
const unlin = (s) => 255 * (s <= 0.0031308 ? 12.92 * s : 1.055 * s ** (1 / 2.4) - 0.055);
const simulate = (c, m) => {
  const l = [lin(c[0]), lin(c[1]), lin(c[2])];
  return m.map((row) => unlin(Math.min(1, Math.max(0, row[0] * l[0] + row[1] * l[1] + row[2] * l[2]))));
};
function lab(c) {
  const [r, g, b] = [lin(c[0]), lin(c[1]), lin(c[2])];
  const xyz = [
    (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047,
    0.2126 * r + 0.7152 * g + 0.0722 * b,
    (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883,
  ];
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  const [fx, fy, fz] = xyz.map(f);
  return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)];
}
const dE = (a, b) => {
  const [p, q] = [lab(a), lab(b)];
  return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]);
};
/** Smallest colour difference between two colours, normal vision and the three simulations. */
function worstDelta(a, b) {
  let worst = { dE: dE(a, b), vision: "normal" };
  for (const [name, m] of Object.entries(CVD)) {
    const d = dE(simulate(a, m), simulate(b, m));
    if (d < worst.dE) worst = { dE: d, vision: name };
  }
  return worst;
}

// ------------------------------------------------------------ scenarios

const rules = parseRules(CSS);
const themeIds = [...new Set([...CSS.matchAll(/\[data-theme="([\w-]+)"\]/g)].map((m) => m[1]))];
const regionIds = [...new Set([...CSS.matchAll(/\[data-region="([\w-]+)"\]/g)].map((m) => m[1]))];

const scenarios = [];
for (const theme of themeIds) {
  const regions = theme === "regional" ? regionIds : [undefined];
  for (const region of regions)
    for (const mode of ["light", "dark"]) {
      const attrs = { "data-theme": theme, "data-mode": mode, "data-density": "comfortable" };
      if (region) attrs["data-region"] = region;
      scenarios.push({ id: region ? `${theme}-${region}` : theme, theme, region, mode, attrs });
    }
}

const KINDS = ["Payment", "Receipt", "Contra", "Journal", "Sales", "Purchase", "CreditNote", "DebitNote", "StockJournal"];
/** Every colour token a theme must set in both modes (missing ones would fall back to the other mode). */
const COLOUR_TOKENS = [
  "--bg", "--surface", "--surface-2", "--ink", "--muted", "--line", "--field-line",
  "--brand", "--brand-2", "--brand-ink", "--topbar", "--topbar-ink", "--topbar-key", "--topbar-muted", "--topbar-hover", "--search-bg",
  "--accent", "--accent-text", "--accent-ink", "--focus", "--sel", "--sel-ink",
  "--danger", "--warn", "--ok", "--pos", "--neg",
  "--series-1", "--series-2", "--series-3", "--series-4", "--series-5", "--series-6",
  "--sel-accent-text", "--search-line", "--badge-ink", "--scrim", "--selection",
  ...KINDS.map((k) => `--kind-${k}`), ...KINDS.map((k) => `--tint-${k}`),
];

function checks(tok, sc) {
  const aaa = sc.theme === "comfort";
  const body = aaa ? 7 : 4.5;
  const text = 4.5;
  const out = [];
  const add = (group, fg, bg, min, opts = {}) => out.push({ group, fg, bg, min, ...opts });
  const grounds = ["--bg", "--surface", "--surface-2"];
  for (const g of grounds) {
    add("Body text", "--ink", g, body);
    add("Secondary text", "--muted", g, body);
    add("Headings, key letters", "--brand", g, text);
    add("Accent text", "--accent-text", g, text);
    add("Error text", "--danger", g, text);
    add("Warning text", "--warn", g, text);
    add("Success text", "--ok", g, text);
    add("Profit / positive", "--pos", g, text);
    add("Loss / negative", "--neg", g, text);
  }
  add("Cursor row text", "--sel-ink", "--sel", body);
  add("Cursor row key letter", "--sel-accent-text", "--sel", text);
  add("Cursor row heading", "--brand", "--sel", text);
  add("Cursor row secondary", "--muted", "--sel", text);
  add("Text on brand", "--brand-ink", "--brand", text);
  add("Text on accent (large)", "--accent-ink", "--accent", 3);
  add("Top bar text", "--topbar-ink", "--topbar", body);
  add("Top bar key letters", "--topbar-key", "--topbar", text);
  add("Top bar unavailable", "--topbar-muted", "--topbar", text);
  add("Top bar hover", "--topbar-ink", ["--topbar-hover", "--topbar"], text);
  add("Search box text", "--topbar-ink", ["--search-bg", "--topbar"], text);
  for (const k of KINDS) {
    add("Voucher badge", "--badge-ink", `--kind-${k}`, text);
    add("Text on voucher tint", "--ink", `--tint-${k}`, body);
    add("Secondary on voucher tint", "--muted", `--tint-${k}`, text);
    add("Focus ring on voucher tint", "--focus", `--tint-${k}`, 3);
  }
  // Non-text: 3:1 against what they sit on.
  add("Focus ring", "--focus", "--surface", 3);
  add("Focus ring", "--focus", "--bg", 3);
  add("Focus ring", "--focus", "--sel", 3);
  add("Cursor edge", "--accent", "--sel", 3);
  add("Cursor edge", "--accent", "--surface", 3);
  for (const s of [1, 2, 3, 4, 5, 6]) add("Chart series", `--series-${s}`, "--surface", 3);
  // 1.4.11: a field's border against its own fill (--surface) and the ground around it.
  for (const g of grounds) add("Field border", "--field-line", g, 3);
  add("Search box border", ["--search-line", "--topbar"], "--topbar", 3);
  // The Spine mark: the dark colourway on every top bar; on the page, the mode's colourway.
  for (const f of MARK.dark) add("Brand mark on top bar", f, "--topbar", 3);
  for (const f of MARK[sc.mode]) add("Brand mark on page", f, "--bg", 3);
  return out;
}

// The cash line is also a different mark (a line with dots, not bars), so it needs less.
const DISTINCT = [
  ["Profit vs loss", "--pos", "--neg", 15],
  ["Chart: money in vs out", "--series-1", "--series-2", 15],
  ["Chart: in vs cash line", "--series-1", "--series-3", 10],
  ["Chart: out vs cash line", "--series-2", "--series-3", 10],
];

const results = [];
let failures = 0;
let advisories = 0;
for (const sc of scenarios) {
  const { tok, from } = tokensFor(rules, sc.attrs);
  const row = { id: sc.id, theme: sc.theme, region: sc.region ?? null, mode: sc.mode, checks: [], distinct: [], missing: [], scheme: tok["color-scheme"] };
  // Completeness: the token must come from a rule for this theme and mode (or the shared base).
  // In dark, the winning rule must be a dark one (or a shared, mode-free one with :not()),
  // else a token the theme forgot in dark would quietly keep its light value.
  for (const t of COLOUR_TOKENS) {
    if (tok[t] === undefined) row.missing.push(t);
    else if (sc.mode === "dark" && !from[t].includes('[data-mode="dark"]') && !from[t].includes(":not(")) row.missing.push(`${t} in dark (light value from ${from[t]})`);
  }
  if (tok["color-scheme"] !== sc.mode) row.missing.push(`color-scheme (${tok["color-scheme"]})`);
  for (const c of checks(tok, sc)) {
    try {
      const bgs = Array.isArray(c.bg) ? c.bg : [c.bg];
      // Translucent layers sit on the last colour (opaque).
      const ref = (x) => (x.startsWith("#") ? x : `var(${x})`);
      let bg = color(ref(bgs[bgs.length - 1]), tok);
      for (let k = bgs.length - 2; k >= 0; k--) bg = over(color(ref(bgs[k]), tok), bg);
      if (bg[3] < 1) bg = over(bg, color("var(--surface)", tok));
      const fgs = Array.isArray(c.fg) ? c.fg : [c.fg];
      let fg = color(ref(fgs[fgs.length - 1]), tok);
      for (let k = fgs.length - 2; k >= 0; k--) fg = over(color(ref(fgs[k]), tok), fg);
      fg = over(fg, bg);
      const r = ratio(fg, bg);
      const pass = r >= c.min - 1e-9;
      if (!pass) c.advisory ? advisories++ : failures++;
      row.checks.push({ group: c.group, fg: fgs.join(" over "), bg: bgs.join(" over "), fgHex: hex(fg), bgHex: hex(bg), ratio: +r.toFixed(2), min: c.min, pass, advisory: !!c.advisory });
    } catch (e) {
      failures++;
      row.checks.push({ group: c.group, fg: c.fg, bg: String(c.bg), ratio: 0, min: c.min, pass: false, error: e.message });
    }
  }
  for (const [label, a, b, min] of DISTINCT) {
    const surf = color("var(--surface)", tok);
    const ca = over(color(`var(${a})`, tok), surf);
    const cb = over(color(`var(${b})`, tok), surf);
    const w = worstDelta(ca, cb);
    const pass = w.dE >= min;
    if (!pass) failures++;
    row.distinct.push({ label, a: hex(ca), b: hex(cb), dE: +w.dE.toFixed(1), vision: w.vision, min, pass });
  }
  // 2.4.13: the focus ring is at least 2 px.
  const fw = parseFloat(tok["--focus-width"] ?? "0");
  row.focusWidth = fw;
  if (!(fw >= 2)) row.missing.push(`--focus-width ${tok["--focus-width"]} (needs 2px or more)`);
  failures += row.missing.length;
  // Swatches for the comparison page.
  row.swatches = {};
  for (const t of ["--bg", "--surface", "--surface-2", "--ink", "--muted", "--brand", "--topbar", "--topbar-key", "--accent", "--accent-text", "--sel", "--focus", "--pos", "--neg", "--warn", "--danger", "--series-1", "--series-2", "--series-3"]) {
    try {
      row.swatches[t] = hex(over(color(`var(${t})`, tok), color("var(--surface)", tok)));
    } catch {
      /* reported above */
    }
  }
  results.push(row);
}

if (asJson) {
  process.stdout.write(JSON.stringify({ failures, advisories, results }, null, 2) + "\n");
} else {
  for (const r of results) {
    const bad = r.checks.filter((c) => !c.pass);
    const worst = r.checks.filter((c) => !c.advisory).reduce((m, c) => (c.ratio / c.min < m.ratio / m.min ? c : m), r.checks[0]);
    const flag = bad.some((c) => !c.advisory) || r.missing.length || r.distinct.some((d) => !d.pass) ? "FAIL" : "ok  ";
    console.log(`${flag} ${r.id.padEnd(18)} ${r.mode.padEnd(5)}  ${r.checks.length} pairs, tightest ${worst.group} ${worst.ratio}:1 (min ${worst.min})`);
    for (const m of r.missing) console.log(`       missing ${m}`);
    for (const c of bad) console.log(`       ${c.advisory ? "advisory" : "FAIL    "} ${c.group}: ${c.fg} on ${c.bg} = ${c.ratio}:1 < ${c.min}${c.error ? ` (${c.error})` : ""} ${c.fgHex ?? ""} / ${c.bgHex ?? ""}`);
    for (const d of r.distinct.filter((d) => !d.pass)) console.log(`       FAIL     ${d.label}: ΔE ${d.dE} (${d.vision}) < ${d.min}  ${d.a} vs ${d.b}`);
  }
  console.log(`\n${scenarios.length} theme/mode combinations, ${failures} failure(s), ${advisories} advisory note(s).`);
}
process.exitCode = failures ? 1 : 0; // not exit(): a piped --json would be cut short

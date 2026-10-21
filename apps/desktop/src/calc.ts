// The calculator's arithmetic (Ctrl+N; the panel is calcpanel.ts). A small
// recursive-descent parser, never eval or Function. Numbers are exact
// fractions of BigInts while the expression is worked out, so 1180 / 1.18 is
// exactly 1000 and 100 / 3 * 3 exactly 100; the answer rounds once to
// money.ts units (6 places), and the panel rounds that to the currency's
// minor units for display.
//
// Grammar (usual precedence, left to right):
//   expr    = term (("+" | "-") term)*
//   term    = unary (("*" | "/") unary)*
//   unary   = ("-" | "+") unary | postfix
//   postfix = primary "%"?
//   primary = number | "(" expr ")"
// Percent: "a + p%" and "a - p%" add or take p percent of a (1000 + 18% =
// 1180); elsewhere p% is p / 100 (500 * 18% = 90, 200 / 50% = 400).
// Numbers may carry Indian (1,00,000) or Western (100,000) grouping, Arabic-
// Indic digits (١٢٠٠) and the Arabic separators (٬ ٫), and × ÷ − ٪ work.
import { latinDigits } from "./i18n";
import * as M from "./money";

/** One money unit (1.000000), as money.ts holds it. */
const ONE: M.Units = M.of("1");
/** Decimal places in a money unit (6). */
const PLACES = ONE.toString().length - 1;

/** Why an expression has no value. `at` is the 0-based position in the typed text. */
export type CalcError =
  | { error: "empty" }
  /** Ends on an operator or an open bracket: more is coming. */
  | { error: "incomplete" }
  | { error: "unexpected"; at: number; text: string }
  | { error: "brackets" }
  /** Commas that are neither 1,00,000 nor 100,000. */
  | { error: "grouping"; at: number; text: string }
  | { error: "divide-by-zero" };

export type CalcResult =
  /** `value` is rounded to money units; `exact` is false when that rounding lost digits (1 / 3). */
  | { ok: true; value: M.Units; exact: boolean }
  | ({ ok: false } & CalcError);

// ------------------------------------------------------------ fractions

interface Q {
  n: bigint;
  /** Always positive. */
  d: bigint;
}

function gcd(a: bigint, b: bigint): bigint {
  if (a < 0n) a = -a;
  while (b) [a, b] = [b, a % b];
  return a || 1n;
}

function q(n: bigint, d = 1n): Q {
  if (d < 0n) [n, d] = [-n, -d];
  const g = gcd(n, d);
  return { n: n / g, d: d / g };
}

const add = (a: Q, b: Q): Q => q(a.n * b.d + b.n * a.d, a.d * b.d);
const sub = (a: Q, b: Q): Q => q(a.n * b.d - b.n * a.d, a.d * b.d);
const mul = (a: Q, b: Q): Q => q(a.n * b.n, a.d * b.d);
const HUNDRED = q(100n);

class Fail {
  constructor(readonly e: CalcError) {}
}

function div(a: Q, b: Q): Q {
  if (b.n === 0n) throw new Fail({ error: "divide-by-zero" });
  return q(a.n * b.d, a.d * b.n);
}

/** n / d rounded half away from zero (commercial rounding). */
function roundDiv(n: bigint, d: bigint): bigint {
  if (d < 0n) [n, d] = [-n, -d];
  const quo = n / d;
  const rem = n % d;
  const twice = rem < 0n ? -2n * rem : 2n * rem;
  return twice >= d ? quo + (n < 0n ? -1n : 1n) : quo;
}

// ------------------------------------------------------------ tokens

type Tok =
  | { k: "num"; v: Q; at: number; text: string }
  | { k: "op"; op: "+" | "-" | "*" | "/" | "%" | "(" | ")"; at: number }
  | { k: "end"; at: number };

const OPS: Record<string, "+" | "-" | "*" | "/" | "%" | "(" | ")"> = {
  "+": "+",
  "-": "-",
  "−": "-", // − minus sign
  "–": "-", // – en dash, from a word processor
  "*": "*",
  "×": "*", // ×
  "/": "/",
  "÷": "/", // ÷
  "%": "%",
  "٪": "%", // ٪ Arabic percent sign
  "(": "(",
  ")": ")",
};

/** Integer digits grouped as 100,000 (thousands) or 1,00,000 (Indian lakh/crore). */
const WESTERN = /^\d{1,3}(?:,\d{3})+$/;
const INDIAN = /^\d{1,2}(?:,\d{2})*,\d{3}$/;
const NUMBER = /\d[\d,]*(?:\.\d*)?|\.\d+/y;

function tokenize(text: string): Tok[] {
  // latinDigits maps one character to one, so positions match what was typed.
  const s = latinDigits(text);
  const out: Tok[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (/\s/.test(c)) {
      i++;
      continue;
    }
    // A trailing "=" (typed out of habit) is ignored.
    if (c === "=" && /^=\s*$/.test(s.slice(i))) break;
    const op = OPS[c];
    if (op) {
      out.push({ k: "op", op, at: i });
      i++;
      continue;
    }
    NUMBER.lastIndex = i;
    const m = NUMBER.exec(s);
    if (!m) throw new Fail({ error: "unexpected", at: i, text: text.slice(i, i + 1) });
    const raw = m[0];
    const [int, frac = ""] = raw.split(".");
    if (int.includes(",") && !WESTERN.test(int) && !INDIAN.test(int)) {
      throw new Fail({ error: "grouping", at: i, text: text.slice(i, i + raw.length) });
    }
    const digits = int.replace(/,/g, "") + frac;
    out.push({ k: "num", v: q(BigInt(digits || "0"), 10n ** BigInt(frac.length)), at: i, text: raw });
    i += raw.length;
  }
  out.push({ k: "end", at: s.length });
  return out;
}

// ------------------------------------------------------------ parser

/** A value, and whether it is a bare "p%" (so "a + p%" means a plus p percent of a). */
interface V {
  v: Q;
  pct: boolean;
}

const MAX_DEPTH = 64;

class Parser {
  private i = 0;
  /** Brackets open at the cursor. */
  private open = 0;
  /** Nesting of brackets and signs, capped so "((((…" can't overflow the stack. */
  private nest = 0;

  constructor(
    private readonly toks: Tok[],
    private readonly text: string,
  ) {}

  run(): Q {
    if (this.peek().k === "end") throw new Fail({ error: "empty" });
    const v = this.expr();
    const t = this.peek();
    if (t.k !== "end") {
      if (t.k === "op" && t.op === ")") throw new Fail({ error: "brackets" });
      this.unexpected(t);
    }
    return v.v;
  }

  private peek(): Tok {
    return this.toks[this.i];
  }

  private isOp(...ops: string[]): string | null {
    const t = this.peek();
    return t.k === "op" && ops.includes(t.op) ? t.op : null;
  }

  private unexpected(t: Tok): never {
    if (t.k === "end") throw new Fail({ error: this.open > 0 ? "brackets" : "incomplete" });
    throw new Fail({ error: "unexpected", at: t.at, text: t.k === "num" ? t.text : this.text.slice(t.at, t.at + 1) });
  }

  private expr(): V {
    let a = this.term();
    for (let op; (op = this.isOp("+", "-")); ) {
      this.i++;
      const b = this.term();
      // a ± p% = a × (1 ± p/100); otherwise plain addition.
      const rhs = b.pct ? mul(a.v, b.v) : b.v;
      a = { v: op === "+" ? add(a.v, rhs) : sub(a.v, rhs), pct: false };
    }
    return a;
  }

  private term(): V {
    let a = this.unary();
    for (let op; (op = this.isOp("*", "/")); ) {
      this.i++;
      const b = this.unary();
      a = { v: op === "*" ? mul(a.v, b.v) : div(a.v, b.v), pct: false };
    }
    return a;
  }

  private unary(): V {
    const op = this.isOp("+", "-");
    if (op) {
      this.i++;
      if (++this.nest > MAX_DEPTH) this.unexpected(this.peek());
      const v = this.unary();
      this.nest--;
      return op === "-" ? { v: q(-v.v.n, v.v.d), pct: v.pct } : v;
    }
    const v = this.primary();
    if (this.isOp("%")) {
      this.i++;
      return { v: div(v, HUNDRED), pct: true };
    }
    return { v, pct: false };
  }

  private primary(): Q {
    const t = this.peek();
    if (t.k === "num") {
      this.i++;
      return t.v;
    }
    if (t.k === "op" && t.op === "(") {
      this.i++;
      if (++this.nest > MAX_DEPTH) this.unexpected(t);
      this.open++;
      const v = this.expr().v;
      if (!this.isOp(")")) {
        if (this.peek().k === "end") throw new Fail({ error: "brackets" });
        this.unexpected(this.peek());
      }
      this.i++;
      this.open--;
      this.nest--;
      return v;
    }
    return this.unexpected(t);
  }
}

/** Work out a typed expression: "1,00,000 + 18%", "1180 / 1.18", "١٢٠٠ × ٥٪". */
export function evaluate(text: string): CalcResult {
  try {
    const v = new Parser(tokenize(text), text).run();
    const scaled = v.n * ONE;
    return { ok: true, value: roundDiv(scaled, v.d), exact: scaled % v.d === 0n };
  } catch (e) {
    if (e instanceof Fail) return { ok: false, ...e.e };
    throw e;
  }
}

// ------------------------------------------------------------ rounding

/** Round money units to `places` decimals (0–6), half away from zero: 2.345 → 2.35, -2.345 → -2.35. */
export function roundTo(u: M.Units, places: number): M.Units {
  const p = Math.max(0, Math.min(PLACES, Math.trunc(places)));
  const step = 10n ** BigInt(PLACES - p);
  return roundDiv(u, step) * step;
}

/** Currencies whose minor unit is not the cent (ISO 4217). */
const MINOR: Record<string, number> = { BHD: 3, IQD: 3, JOD: 3, KWD: 3, LYD: 3, OMR: 3, TND: 3, JPY: 0, KRW: 0, VND: 0, CLP: 0, ISK: 0 };

/** Decimal places of a currency's minor unit: 2 for INR, AED, SAR, SGD, GBP; 3 for KWD; 0 for JPY. */
export function minorUnits(currency: string | null | undefined): number {
  return MINOR[(currency ?? "").toUpperCase()] ?? 2;
}

// ------------------------------------------------------------ tax

export interface TaxPreset {
  /** English name, shown through t(): "GST", "VAT". */
  name: string;
  /** Rates in percent, as decimal text ("18", "5.5"). */
  rates: string[];
  /** The rate picked first. */
  standard: string;
  /** India: the tax shows as CGST + SGST halves (intra-state supply). */
  halves: boolean;
}

/**
 * Tax rates by country pack. India: the GST 2.0 slabs (5, 18, 40, from
 * 22 Sep 2025) and 12 and 28, still met on older documents (khata-in
 * `standard_rates`). The US has no national rate, so no preset.
 */
const TAX: Record<string, TaxPreset> = {
  IN: { name: "GST", rates: ["5", "12", "18", "28", "40"], standard: "18", halves: true },
  AE: { name: "VAT", rates: ["5"], standard: "5", halves: false },
  SA: { name: "VAT", rates: ["15"], standard: "15", halves: false },
  SG: { name: "GST", rates: ["9"], standard: "9", halves: false },
  GB: { name: "VAT", rates: ["20", "5"], standard: "20", halves: false },
  DE: { name: "VAT", rates: ["19", "7"], standard: "19", halves: false },
  FR: { name: "VAT", rates: ["20", "10", "5.5", "2.1"], standard: "20", halves: false },
  ES: { name: "VAT", rates: ["21", "10", "4"], standard: "21", halves: false },
  NL: { name: "VAT", rates: ["21", "9"], standard: "21", halves: false },
  MX: { name: "VAT", rates: ["16", "8"], standard: "16", halves: false },
};

/** The open company's tax preset, or null (no country yet, or no national rate). */
export function taxPreset(country: string | null | undefined): TaxPreset | null {
  return TAX[(country ?? "").toUpperCase()] ?? null;
}

/** "add": the amount is before tax. "remove": the amount already includes the tax. */
export type TaxMode = "add" | "remove";

export interface TaxSplit {
  taxable: M.Units;
  tax: M.Units;
  total: M.Units;
  /** CGST and SGST (India), each at half the rate; they add up to `tax`. */
  halves: [M.Units, M.Units] | null;
}

/** a × b / c, rounded to `places`. */
function mulDiv(a: M.Units, b: bigint, c: bigint, places: number): M.Units {
  return roundTo(roundDiv(a * b, c), places);
}

/**
 * Add tax at `rate` percent to an amount, or take it out of an amount that
 * includes it, rounded to `places`. Adding with halves works each half on the
 * taxable value, as a GST invoice shows them (CGST = SGST = taxable × rate/2);
 * removing keeps the total as typed, and the halves split the tax.
 */
export function taxSplit(amount: M.Units, rate: M.Units, mode: TaxMode, places: number, halves: boolean): TaxSplit {
  const hundred = 100n * ONE;
  if (mode === "add") {
    const taxable = roundTo(amount, places);
    if (halves) {
      const half = mulDiv(taxable, rate, 2n * hundred, places);
      return { taxable, tax: 2n * half, total: taxable + 2n * half, halves: [half, half] };
    }
    const tax = mulDiv(taxable, rate, hundred, places);
    return { taxable, tax, total: taxable + tax, halves: null };
  }
  const total = roundTo(amount, places);
  const taxable = mulDiv(total, hundred, hundred + rate, places);
  const tax = total - taxable;
  if (!halves) return { taxable, tax, total, halves: null };
  const first = mulDiv(tax, 1n, 2n, places);
  return { taxable, tax, total, halves: [first, tax - first] };
}

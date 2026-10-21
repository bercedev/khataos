// Exact money arithmetic on decimal strings. Amounts never become JS floats:
// they are held as BigInt "units" with a fixed number of decimal places.
// Every language shows Latin digits with the company's grouping (docs/i18n.md);
// only the Dr/Cr word is translated.
import { latinDigits, t } from "./i18n";

const SCALE = 6;
const ONE = 10n ** BigInt(SCALE);

export type Units = bigint;
export const ZERO: Units = 0n;

/** Parse "2,50,000.50", "-1200", "١٢٠٠" (Arabic keyboard) or "" (zero). Returns null when invalid. */
export function parse(text: string): Units | null {
  const s = latinDigits(text).trim().replace(/[,_\s]/g, "");
  if (s === "") return ZERO;
  const m = /^([+-]?)(\d*)(?:\.(\d*))?$/.exec(s);
  if (!m || (m[2] === "" && (m[3] ?? "") === "")) return null;
  const frac = m[3] ?? "";
  if (frac.length > SCALE) return null;
  const units = BigInt(m[2] || "0") * ONE + BigInt((frac + "0".repeat(SCALE)).slice(0, SCALE) || "0");
  return m[1] === "-" ? -units : units;
}

/** Units from a backend decimal string (always valid). */
export function of(text: string): Units {
  return parse(text) ?? ZERO;
}

/** Plain decimal string for the backend: "250000.5". */
export function toDecimal(u: Units): string {
  const neg = u < 0n;
  const a = neg ? -u : u;
  const int = (a / ONE).toString();
  const frac = (a % ONE).toString().padStart(SCALE, "0").replace(/0+$/, "");
  return (neg ? "-" : "") + int + (frac ? "." + frac : "");
}

/** Indian digit grouping: 2,50,000 (last three, then pairs). */
function groupIndian(int: string): string {
  if (int.length <= 3) return int;
  const head = int.slice(0, -3);
  const tail = int.slice(-3);
  return head.replace(/\B(?=(\d{2})+(?!\d))/g, ",") + "," + tail;
}

/** Thousands grouping: 250,000. */
function groupThousands(int: string): string {
  return int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Indian grouping for an Indian company (and before a country is known), thousands elsewhere. */
let group = groupIndian;

/** Group digits the way the company's country writes them ("IN": 2,50,000; others: 250,000). */
export function useGroupingOf(country: string | null | undefined): void {
  group = !country || country === "IN" ? groupIndian : groupThousands;
}

/** "2,50,000.00" — at least two decimals, more only when present. */
export function format(u: Units): string {
  const neg = u < 0n;
  const a = neg ? -u : u;
  const int = group((a / ONE).toString());
  let frac = (a % ONE).toString().padStart(SCALE, "0").replace(/0+$/, "");
  if (frac.length < 2) frac = frac.padEnd(2, "0");
  return (neg ? "-" : "") + int + "." + frac;
}

export function abs(u: Units): Units {
  return u < 0n ? -u : u;
}

/** "2,50,000.00 Dr" ("… مدين") for a debit-positive balance; "" for zero when `blankZero`. */
export function drCr(u: Units, blankZero = false): string {
  if (u === 0n) return blankZero ? "" : "0.00";
  return `${format(abs(u))} ${t(u > 0n ? "Dr" : "Cr")}`;
}

export function sum(values: Units[]): Units {
  return values.reduce((a, b) => a + b, ZERO);
}

/** Format a backend string, blank for zero when asked. */
export function fmt(text: string, blankZero = false): string {
  const u = of(text);
  return blankZero && u === 0n ? "" : format(u);
}

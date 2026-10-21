// Self-check for the calculator's parser (src/calc.ts). There is no frontend
// test runner: this transpiles calc.ts and the modules it imports with the
// installed typescript package into a temporary folder, then runs the cases.
//   node scripts/calc-check.mjs        (or: pnpm calc-check)
// Exits 1 when a case fails.
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";

const src = join(import.meta.dirname, "..", "src");
const out = mkdtempSync(join(tmpdir(), "khata-calc-"));

/** Transpile `file` and every relative module it imports, as .mjs files under `out`. */
function build(file, seen = new Set()) {
  if (seen.has(file)) return;
  seen.add(file);
  const code = ts.transpileModule(readFileSync(file, "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, verbatimModuleSyntax: false },
    fileName: file,
  }).outputText;
  const js = code.replace(/(from\s+|import\s*\(\s*)(["'])(\.{1,2}\/[^"']+)\2/g, (m, head, q, spec) => {
    if (spec.endsWith(".css")) return m;
    build(resolve(dirname(file), spec + ".ts"), seen);
    return `${head}${q}${spec}.mjs${q}`;
  });
  // A bare CSS import (none today) would not load in node.
  const target = join(out, relative(src, file)).replace(/\.ts$/, ".mjs");
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, js.replace(/^import\s+["'][^"']+\.css["'];?$/gm, ""));
}

let C, M;
try {
  build(join(src, "calc.ts"));
  C = await import(pathToFileURL(join(out, "calc.mjs")).href);
  M = await import(pathToFileURL(join(out, "money.mjs")).href);
} finally {
  rmSync(out, { recursive: true, force: true });
}

let failed = 0;
let passed = 0;
function check(name, got, want) {
  if (got === want) passed++;
  else {
    failed++;
    console.error(`FAIL ${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
  }
}

/** The value as a plain decimal ("1180", "0.333333"), or the error code. */
function calc(text) {
  const r = C.evaluate(text);
  return r.ok ? M.toDecimal(r.value) : `error:${r.error}`;
}

const cases = [
  // precedence and brackets
  ["2 + 3 * 4", "14"],
  ["(2 + 3) * 4", "20"],
  ["10 - 4 - 3", "3"],
  ["100 / 10 / 2", "5"],
  ["2 * (3 + (4 - 1)) / 3", "4"],
  ["8 - 2 * 3 + 1", "3"],
  // decimals and unary minus
  ["0.1 + 0.2", "0.3"],
  [".5 + 5.", "5.5"],
  ["-5 + 2", "-3"],
  ["2 * -3", "-6"],
  ["10 - -2", "12"],
  ["-(2 + 3)", "-5"],
  ["+7", "7"],
  ["1180 / 1.18", "1000"],
  ["1 / 3", "0.333333"],
  ["2 / 3", "0.666667"],
  ["100 / 3 * 3", "100"],
  ["-2 / 3", "-0.666667"],
  // percent
  ["1000 + 18%", "1180"],
  ["1000 - 10%", "900"],
  ["500 * 18%", "90"],
  ["200 / 50%", "400"],
  ["18%", "0.18"],
  ["1000 + 18% * 2", "1000.36"],
  ["(1000 + 5%) + 18%", "1239"],
  ["1000 + -10%", "900"],
  // separators
  ["1,00,000 + 100,000", "200000"],
  ["12,34,567.50", "1234567.5"],
  ["1,000.25 * 4", "4001"],
  ["1,2", "error:grouping"],
  ["1,0000", "error:grouping"],
  ["10,00", "error:grouping"],
  ["1,000,", "error:grouping"],
  // Arabic-Indic and Persian digits, Arabic separators and signs
  ["١٢٠٠ + ١٨٪", "1416"],
  ["١٬٠٠٠٫٥ × ٢", "2001"],
  ["۱۰۰ ÷ ۴", "25"],
  ["١٠٠ − ٣٠", "70"],
  // trailing "=" and spaces
  ["  12 + 3 =  ", "15"],
  // divide by zero
  ["1 / 0", "error:divide-by-zero"],
  ["5 / (2 - 2)", "error:divide-by-zero"],
  ["5 / 0%", "error:divide-by-zero"],
  // malformed input
  ["", "error:empty"],
  ["   ", "error:empty"],
  ["1 +", "error:incomplete"],
  ["1 + -", "error:incomplete"],
  ["(1 + 2", "error:brackets"],
  ["1 + 2)", "error:brackets"],
  ["()", "error:unexpected"],
  ["1 + * 2", "error:unexpected"],
  ["1 2", "error:unexpected"],
  ["2(3)", "error:unexpected"],
  ["1..2", "error:unexpected"],
  ["1.2.3", "error:unexpected"],
  [".", "error:unexpected"],
  ["18%%", "error:unexpected"],
  ["1e5", "error:unexpected"],
  ["abc", "error:unexpected"],
  ["2 ** 3", "error:unexpected"],
  ["alert(1)", "error:unexpected"],
  ["=", "error:empty"],
  ["(".repeat(200) + "1" + ")".repeat(200), "error:unexpected"],
  ["-".repeat(200) + "1", "error:unexpected"],
  ["(".repeat(20) + "1" + ")".repeat(20), "1"],
];
for (const [text, want] of cases) check(JSON.stringify(text.length > 40 ? text.slice(0, 40) + "…" : text), calc(text), want);

// Where the problem is, for the panel's message.
const bad = C.evaluate("12 + 3 $ 4");
check("unexpected position", bad.ok ? "ok" : `${bad.at}:${bad.text}`, "7:$");
const grp = C.evaluate("5 + 1,2");
check("grouping text", grp.ok ? "ok" : grp.text, "1,2");
check("exact flag 1/4", C.evaluate("1/4").exact, true);
check("exact flag 1/3", C.evaluate("1/3").exact, false);

// Rounding to the currency's minor units, half away from zero.
const r = (text, places) => M.toDecimal(C.roundTo(M.of(text), places));
check("round 2.345", r("2.345", 2), "2.35");
check("round -2.345", r("-2.345", 2), "-2.35");
check("round 2.344", r("2.344", 2), "2.34");
check("round 0.333333", r("0.333333", 2), "0.33");
check("round 1.0005 to 3", r("1.0005", 3), "1.001");
check("round 99.5 to 0", r("99.5", 0), "100");
check("minor INR", C.minorUnits("INR"), 2);
check("minor KWD", C.minorUnits("KWD"), 3);
check("minor JPY", C.minorUnits("JPY"), 0);
check("minor unknown", C.minorUnits(null), 2);

// Tax helpers.
const split = (amount, rate, mode, halves) => {
  const s = C.taxSplit(M.of(amount), M.of(rate), mode, 2, halves);
  return [s.taxable, s.tax, s.total, ...(s.halves ?? [])].map(M.toDecimal).join(" ");
};
check("add 18% GST, halves", split("1000", "18", "add", true), "1000 180 1180 90 90");
check("remove 18% GST, halves", split("1180", "18", "remove", true), "1000 180 1180 90 90");
check("add 5% VAT", split("200", "5", "add", false), "200 10 210");
check("remove 5% VAT", split("105", "5", "remove", false), "100 5 105");
check("remove 15% VAT", split("100", "15", "remove", false), "86.96 13.04 100");
check("add 5% halves on odd paise", split("100.10", "5", "add", true), "100.1 5 105.1 2.5 2.5");
check("remove 5% halves split", split("100", "5", "remove", true), "95.24 4.76 100 2.38 2.38");
check("remove 12% halves uneven", split("100.01", "12", "remove", true), "89.29 10.72 100.01 5.36 5.36");
check("remove 9% SG", split("109", "9", "remove", false), "100 9 109");
check("IN preset", C.taxPreset("IN")?.rates.join(","), "5,12,18,28,40");
check("IN halves", C.taxPreset("IN")?.halves, true);
check("AE preset", C.taxPreset("AE")?.rates.join(","), "5");
check("SA preset", C.taxPreset("SA")?.rates.join(","), "15");
check("SG preset", C.taxPreset("SG")?.rates.join(","), "9");
check("GB preset", C.taxPreset("GB")?.standard, "20");
check("US preset", C.taxPreset("US"), null);

console.log(`${passed} passed, ${failed} failed`);
if (failed) process.exit(1);

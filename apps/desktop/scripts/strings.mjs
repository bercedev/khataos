// Translation review (docs/i18n.md).
//   node scripts/strings.mjs [ar] > ar.csv   every string: English, translation, status, note
//   node scripts/strings.mjs [ar] --missing  literal t("…") texts in src/ with no translation
// Reads src/locales/<lang>.ts as text: one `"English": "translation",` entry per line.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const src = join(import.meta.dirname, "..", "src");
const args = process.argv.slice(2);
const lang = args.find((a) => !a.startsWith("--")) ?? "ar";

const table = new Map();
const entry = /^\s*("(?:[^"\\]|\\.)*"):\s*("(?:[^"\\]|\\.)*"),\s*(?:\/\/\s*(.*))?$/;
for (const line of readFileSync(join(src, "locales", `${lang}.ts`), "utf8").split("\n")) {
  const m = entry.exec(line);
  if (m) table.set(JSON.parse(m[1]), { text: JSON.parse(m[2]), note: m[3] ?? "" });
}

const csv = (s) => (/[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);

if (args.includes("--missing")) {
  const files = readdirSync(src, { recursive: true }).filter((f) => f.endsWith(".ts") && !f.startsWith("locales"));
  const call = /\btr?\(\s*("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')/g;
  const missing = new Map();
  for (const f of files) {
    for (const m of readFileSync(join(src, f), "utf8").matchAll(call)) {
      const text = m[1].startsWith("'") ? m[1].slice(1, -1).replace(/\\'/g, "'") : JSON.parse(m[1]);
      if (!table.has(text)) missing.set(text, [...(missing.get(text) ?? []), f]);
    }
  }
  for (const [text, where] of missing) console.log(`${text}\t${[...new Set(where)].join(", ")}`);
  console.error(`${missing.size} untranslated literal string(s). Labels held in tables (shell LABELS, home SECTIONS, menus) aren't t("…") calls: check those in the app.`);
} else {
  console.log("english,translation,status,note");
  for (const [en, { text, note }] of table) {
    console.log([en, text, /ar: check/.test(note) ? "check" : "ok", note.replace(/^ar: check\s*/, "")].map(csv).join(","));
  }
}

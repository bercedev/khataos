// Display language. English is the source: every string in the code is the
// English text, and t() looks it up in the active language's table, falling
// back to the English. Arabic switches the whole window to right-to-left.
// Shortcuts never change with the language (keys.ts matches physical keys).
// See docs/i18n.md.
import { AR } from "./locales/ar";

export type Lang = "en" | "ar";

export const LANGUAGES: { code: Lang; name: string; english: string; dir: "ltr" | "rtl" }[] = [
  { code: "en", name: "English", english: "English", dir: "ltr" },
  { code: "ar", name: "العربية", english: "Arabic", dir: "rtl" },
];

const TABLES: Record<Lang, Record<string, string>> = { en: {}, ar: AR };
const STORE = "khataos.language";

let lang: Lang = "en";

/**
 * The text in the display language. `{name}` placeholders are filled from
 * `params`, in the translation as in the English:
 * t("{kind} No. {number} saved.", { kind, number }).
 */
export function t(text: string, params?: Record<string, string | number>): string {
  return fill(TABLES[lang][text] ?? text, params);
}

/** Fill `{name}` placeholders (and ICU plural blocks, by `l`'s rules) in an already translated text. */
export function fill(s: string, params?: Record<string, string | number>, l: Lang = lang): string {
  if (!params) return s;
  return plurals(s, params, l).replace(/\{(\w+)\}/g, (m, k: string) => (k in params ? String(params[k]) : m));
}

const RULES = new Map<string, Intl.PluralRules>();

/** The plural category of `n` in the display language: zero, one, two, few, many or other. */
export function pluralOf(n: number, l: Lang = lang): Intl.LDMLPluralRule {
  let r = RULES.get(l);
  if (!r) RULES.set(l, (r = new Intl.PluralRules(l)));
  return r.select(n);
}

/**
 * ICU plural blocks in a translation: "{n, plural, one {قيد واحد} two {قيدان}
 * few {# قيود} many {# قيدًا} other {# قيد}}". The branch is picked by the
 * language's plural rules (`=0`, `=1`… first); `#` is the number. Other
 * `{name}` placeholders inside a branch are filled as usual.
 */
function plurals(s: string, params: Record<string, string | number>, l: Lang): string {
  const head = /\{(\w+),\s*plural,/g;
  let out = "";
  let from = 0;
  for (let m: RegExpExecArray | null; (m = head.exec(s)); ) {
    const n = Number(params[m[1]]);
    const branches = new Map<string, string>();
    let i = head.lastIndex;
    for (;;) {
      while (/\s/.test(s[i] ?? "")) i++;
      if (s[i] === "}" || i >= s.length) break;
      const key = /^[=\w]+/.exec(s.slice(i))?.[0];
      if (!key) break;
      i += key.length;
      while (/\s/.test(s[i] ?? "")) i++;
      if (s[i] !== "{") break;
      let depth = 1;
      const start = ++i;
      for (; i < s.length && depth; i++) depth += s[i] === "{" ? 1 : s[i] === "}" ? -1 : 0;
      branches.set(key, s.slice(start, i - 1));
    }
    if (s[i] !== "}" || !(m[1] in params) || Number.isNaN(n)) continue; // not a plural block: left as written
    const branch = branches.get(`=${n}`) ?? branches.get(pluralOf(n, l)) ?? branches.get("other") ?? "";
    out += s.slice(from, m.index) + plurals(branch, params, l).replace(/#/g, String(params[m[1]]));
    from = i + 1;
    head.lastIndex = from;
  }
  return out + s.slice(from);
}

/**
 * A message with a count. English picks `forms.one` or `forms.other` by its
 * plural rule; a translation is looked up under `forms.other` and may hold an
 * ICU plural block (see `plurals`), so Arabic gets its zero, one, two, few,
 * many and other forms. `{n}` is the count:
 * tn(3, { one: "Added {n} entry.", other: "Added {n} entries." }).
 */
export function tn(count: number, forms: { one: string; other: string }, params?: Record<string, string | number>): string {
  const p = { n: count, ...params };
  if (forms.other in TABLES[lang]) return t(forms.other, p);
  return fill(pluralOf(count, "en") === "one" ? forms.one : forms.other, p);
}

/**
 * A word whose translation depends on its use. `translated` is the lookup of
 * the English word with its use in brackets, e.g. t("Sales (figure)") —
 * المبيعات, where plain "Sales" is the voucher type. With no entry for that
 * key, the plain English word shows.
 */
export function inUse(en: string, translated: string): string {
  return /\s\([a-z ]+\)$/.test(translated) ? en : translated;
}

export function language(): Lang {
  return lang;
}

export function dir(): "ltr" | "rtl" {
  return LANGUAGES.find((l) => l.code === lang)!.dir;
}

/** Switch the language and the window's direction (`<html lang dir>`). */
export function setLanguage(l: Lang): void {
  lang = l;
  document.documentElement.lang = l;
  document.documentElement.dir = dir();
}

function isLang(v: string | null): v is Lang {
  return LANGUAGES.some((l) => l.code === v);
}

/**
 * The language saved for a signed-in user (`scope`), else for this computer.
 * Kept in the webview's storage: a preference, not company data.
 */
export function savedLanguage(scope?: string): Lang | null {
  // The shell asks with the signed-in user's scope: their Hijri choice applies from then on.
  loadHijri(scope);
  try {
    const v = (scope && localStorage.getItem(`${STORE}:${scope}`)) || localStorage.getItem(STORE);
    return isLang(v) ? v : null;
  } catch {
    return null;
  }
}

/** Remember the choice for this computer and, when someone is signed in, for them. */
export function saveLanguage(l: Lang, scope?: string): void {
  try {
    localStorage.setItem(STORE, l);
    if (scope) localStorage.setItem(`${STORE}:${scope}`, l);
  } catch {
    /* storage unavailable: the choice lasts until the app closes */
  }
}

// ------------------------------------------------------- Hijri dates

const HIJRI = "khataos.hijri";
let hijri = false;
let hijriScope: string | undefined;

/** "Show Hijri dates": the Umm al-Qura date beside each Gregorian one (dates.ts `show`). */
export function hijriOn(): boolean {
  return hijri;
}

/** The Hijri choice saved for a signed-in user (`scope`), else for this computer. */
function loadHijri(scope?: string): void {
  hijriScope = scope;
  try {
    const v = (scope && localStorage.getItem(`${HIJRI}:${scope}`)) || localStorage.getItem(HIJRI);
    hijri = v === "on";
  } catch {
    /* storage unavailable: keep the current choice */
  }
}

/**
 * Switch Hijri dates on or off, for this computer and the user signed in when
 * the language was last read. A display preference only: books keep
 * Gregorian ISO dates, and nothing Hijri is ever saved in them.
 */
export function setHijri(on: boolean, scope: string | undefined = hijriScope): void {
  hijri = on;
  try {
    localStorage.setItem(HIJRI, on ? "on" : "off");
    if (scope) localStorage.setItem(`${HIJRI}:${scope}`, on ? "on" : "off");
  } catch {
    /* storage unavailable: the choice lasts until the app closes */
  }
}

/**
 * An identifier such as a voucher number ("26-27/4") kept left to right as
 * one piece inside Arabic text; unchanged in English.
 */
export function ltr(s: string): string {
  return lang === "ar" ? `\u2066${s}\u2069` : s;
}

/** Typed Arabic-Indic (٠–٩) or Persian (۰–۹) digits and separators as ASCII, for parsing. */
export function latinDigits(s: string): string {
  return s
    .replace(/[٠-٩]/g, (d) => String(d.charCodeAt(0) - 0x0660))
    .replace(/[۰-۹]/g, (d) => String(d.charCodeAt(0) - 0x06f0))
    .replace(/٫/g, ".")
    .replace(/٬/g, ",");
}

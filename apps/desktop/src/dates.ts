// Dates travel as ISO "YYYY-MM-DD" strings; users see "19-Oct-2026", or
// "19-أكتوبر-2026" in Arabic: same order, Latin digits, Gregorian calendar.
// With "Show Hijri dates" on, the Umm al-Qura date follows in brackets; it is
// display only, never parsed into or stored in the books.
import { hijriOn, language, latinDigits, type Lang } from "./i18n";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
/** Gulf usage (as in Saudi and UAE business documents). */
const MONTHS_AR = ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"];
const DAYS_AR = ["الأحد", "الاثنين", "الثلاثاء", "الأربعاء", "الخميس", "الجمعة", "السبت"];

/** Arabic month names as typed: hamza and madda forms of alef count as plain alef. */
function plainAlef(s: string): string {
  return s.replace(/[أإآ]/g, "ا");
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export function iso(y: number, m: number, d: number): string | null {
  const dt = new Date(Date.UTC(y, m - 1, d));
  if (dt.getUTCFullYear() !== y || dt.getUTCMonth() !== m - 1 || dt.getUTCDate() !== d) return null;
  return `${y}-${pad(m)}-${pad(d)}`;
}

function parts(isoDate: string): [number, number, number] {
  const [y, m, d] = isoDate.split("-").map(Number);
  return [y, m, d];
}

export function todayIso(): string {
  const now = new Date();
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** "19-Oct-2026" ("19-أكتوبر-2026"); with Hijri dates on, "19-Oct-2026 (8 Jumada I 1448 AH)". */
export function show(isoDate: string): string {
  const [y, m, d] = parts(isoDate);
  const shown = `${d}-${(language() === "ar" ? MONTHS_AR : MONTHS)[m - 1]}-${y}`;
  const h = hijriOn() ? hijri(isoDate) : null;
  return h ? `${shown} (${h})` : shown;
}

const HIJRI_FORMATS = new Map<Lang, Intl.DateTimeFormat | null>();

function hijriFormat(l: Lang): Intl.DateTimeFormat | null {
  if (!HIJRI_FORMATS.has(l)) {
    let f: Intl.DateTimeFormat | null = null;
    try {
      // Latin digits (-nu-latn), as everywhere else in the app.
      f = new Intl.DateTimeFormat(l === "ar" ? "ar-SA-u-ca-islamic-umalqura-nu-latn" : "en-u-ca-islamic-umalqura-nu-latn", {
        day: "numeric",
        month: "long",
        year: "numeric",
        timeZone: "UTC",
      });
      // An engine without the calendar falls back to Gregorian: show nothing rather than a wrong date.
      if (!f.resolvedOptions().calendar.startsWith("islamic")) f = null;
    } catch {
      f = null;
    }
    HIJRI_FORMATS.set(l, f);
  }
  return HIJRI_FORMATS.get(l) ?? null;
}

/**
 * The Umm al-Qura (Saudi official) Hijri date of an ISO date, from the
 * webview's Intl calendar: "8 Jumada I 1448 AH", "8 جمادى الأولى 1448 هـ".
 * null when the engine has no Islamic calendar.
 */
export function hijri(isoDate: string, l: Lang = language()): string | null {
  const f = hijriFormat(l);
  if (!f) return null;
  const [y, m, d] = parts(isoDate);
  const p = f.formatToParts(new Date(Date.UTC(y, m - 1, d)));
  const get = (type: string) => p.find((x) => x.type === type)?.value ?? "";
  const year = get("year") || get("relatedYear");
  return `${get("day")} ${get("month")} ${year} ${l === "ar" ? "هـ" : "AH"}`;
}

export function weekday(isoDate: string): string {
  const [y, m, d] = parts(isoDate);
  return (language() === "ar" ? DAYS_AR : DAYS)[new Date(Date.UTC(y, m - 1, d)).getUTCDay()];
}

/** The first day of the financial year containing the date, for years
 *  starting in `startMonth` (April in India, January in most countries). */
export function fyStart(isoDate: string, startMonth = 4): string {
  const [y, m] = parts(isoDate);
  return `${m >= startMonth ? y : y - 1}-${pad(startMonth)}-01`;
}

/** The last day of the financial year that starts on `fyStartDate`. */
export function fyEnd(fyStartDate: string): string {
  const [y, m, d] = parts(fyStartDate);
  return addDays(`${y + 1}-${pad(m)}-${pad(d)}`, -1);
}

/**
 * Parse what accountants type: "19", "19-10", "19/10/26", "19.10.2026",
 * "19-Oct", "19 oct 2026" or ISO "2026-10-19"; in any language also Arabic
 * month names ("19 أكتوبر") and Arabic-Indic digits. Missing parts come from `ref`.
 */
export function parseUser(text: string, ref: string): string | null {
  // A shown date fed back keeps its Gregorian part: "19-Oct-2026 (8 Jumada I 1448 AH)".
  const s = latinDigits(text).replace(/\s*\([^)]*\)\s*$/, "").trim().toLowerCase();
  if (!s) return null;
  const [ry, rm] = parts(ref);
  const isoM = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(s);
  if (isoM) return iso(+isoM[1], +isoM[2], +isoM[3]);
  const bits = s.split(/[-/.\s]+/).filter(Boolean);
  if (bits.length === 0 || bits.length > 3) return null;
  const d = Number(bits[0]);
  if (!Number.isInteger(d)) return null;
  let m = rm;
  if (bits.length >= 2) {
    let name = MONTHS.findIndex((mn) => bits[1].startsWith(mn.toLowerCase()));
    if (name < 0) name = MONTHS_AR.findIndex((mn) => plainAlef(mn) === plainAlef(bits[1]));
    m = /^\d+$/.test(bits[1]) ? Number(bits[1]) : name >= 0 ? name + 1 : NaN;
  }
  let y = ry;
  if (bits.length === 3) {
    if (!/^\d{2}(\d{2})?$/.test(bits[2])) return null;
    y = Number(bits[2]);
    if (y < 100) y += 2000;
  }
  if (!Number.isInteger(m)) return null;
  return iso(y, m, d);
}

/** Editable form of a date: "19-10-2026". */
export function editable(isoDate: string): string {
  const [y, m, d] = parts(isoDate);
  return `${pad(d)}-${pad(m)}-${y}`;
}

/** ISO date `days` after `isoDate`. */
export function addDays(isoDate: string, days: number): string {
  const [y, m, d] = parts(isoDate);
  const dt = new Date(Date.UTC(y, m - 1, d + days));
  return `${dt.getUTCFullYear()}-${pad(dt.getUTCMonth() + 1)}-${pad(dt.getUTCDate())}`;
}

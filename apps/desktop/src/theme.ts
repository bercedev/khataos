// Display themes (themes.css): theme, light or dark, density and text size.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isMobile } from "./platform";
//
// A display preference, like the language: kept for this device and, when
// someone is signed in, for that user (the webview's storage, keyed like
// i18n.ts). Never written to the company's books. First run: Ledger, Auto.

export type ThemeId = "ledger" | "chartered" | "executive" | "terminal" | "fresh" | "comfort" | "regional";
export type Mode = "auto" | "light" | "dark";
export type Density = "comfortable" | "compact";
/** Regional palettes; "auto" takes the open company's country. */
export type Region = "auto" | "in" | "gulf" | "uk" | "ea";

export interface ThemeInfo {
  id: ThemeId;
  name: string;
  /** Who it is for, one line (menus, the comparison page). */
  audience: string;
  /** Larger type and rows built into the theme, on top of the text size. */
  zoom?: number;
}

/** English names; menus show them through t(). */
export const THEMES: ThemeInfo[] = [
  { id: "ledger", name: "Ledger", audience: "calm and trustworthy, for everyone" },
  { id: "chartered", name: "Chartered", audience: "print-like statements, for accountants and auditors" },
  { id: "executive", name: "Executive", audience: "graphite with large key figures, for owners and CxOs" },
  { id: "terminal", name: "Terminal", audience: "dense and data-first, monospace figures" },
  { id: "fresh", name: "Fresh", audience: "bright and friendly, still easy to read" },
  { id: "comfort", name: "Comfort", audience: "larger text, strongest contrast, wide focus ring", zoom: 1.2 },
  { id: "regional", name: "Regional", audience: "your region's colours" },
];

export const REGIONS: { id: Region; name: string; sub: string }[] = [
  { id: "auto", name: "Regional: the company's country", sub: "India, Gulf, UK or East Asia; else Ledger" },
  { id: "in", name: "Regional: India", sub: "haldi and neel (turmeric and indigo)" },
  { id: "gulf", name: "Regional: Gulf", sub: "emerald and sand" },
  { id: "uk", name: "Regional: United Kingdom", sub: "heritage navy and brick" },
  { id: "ea", name: "Regional: East Asia and Singapore", sub: "lacquer red and teal" },
];

export const MODES: { id: Mode; name: string }[] = [
  { id: "auto", name: "Auto (follow system)" },
  { id: "light", name: "Light" },
  { id: "dark", name: "Dark" },
];

export const DENSITIES: { id: Density; name: string; sub: string }[] = [
  { id: "comfortable", name: "Comfortable", sub: "the usual row height" },
  { id: "compact", name: "Compact", sub: "more rows on screen" },
];

/** Percent of the usual text size. */
export const TEXT_SIZES = [90, 100, 110, 120, 135];

export interface DisplayPrefs {
  theme: ThemeId;
  region: Region;
  mode: Mode;
  density: Density;
  text: number;
}

export const DEFAULTS: DisplayPrefs = { theme: "ledger", region: "auto", mode: "auto", density: "comfortable", text: 100 };

const STORE = "khataos.display";

/** Which countries take which regional palette. */
const COUNTRY_REGION: Record<string, Exclude<Region, "auto">> = {
  IN: "in",
  SA: "gulf", AE: "gulf", QA: "gulf", KW: "gulf", BH: "gulf", OM: "gulf",
  GB: "uk",
  SG: "ea", MY: "ea", HK: "ea", CN: "ea", TW: "ea", JP: "ea", KR: "ea",
};

let kept: DisplayPrefs = { ...DEFAULTS };
let shown: DisplayPrefs = { ...DEFAULTS };
let country: string | undefined;
let scheme: MediaQueryList | null = null;
/** Last mode sent to the window's title bar; undefined: never sent. */
let windowMode: Mode | undefined;

function systemDark(): boolean {
  try {
    scheme ??= window.matchMedia("(prefers-color-scheme: dark)");
    return scheme.matches;
  } catch {
    return false;
  }
}

function valid(v: unknown): Partial<DisplayPrefs> {
  if (!v || typeof v !== "object") return {};
  const o = v as Record<string, unknown>;
  const out: Partial<DisplayPrefs> = {};
  if (THEMES.some((x) => x.id === o.theme)) out.theme = o.theme as ThemeId;
  if (REGIONS.some((x) => x.id === o.region)) out.region = o.region as Region;
  if (MODES.some((x) => x.id === o.mode)) out.mode = o.mode as Mode;
  if (DENSITIES.some((x) => x.id === o.density)) out.density = o.density as Density;
  if (typeof o.text === "number" && TEXT_SIZES.includes(o.text)) out.text = o.text;
  return out;
}

export function themeInfo(id: ThemeId): ThemeInfo {
  return THEMES.find((x) => x.id === id) ?? THEMES[0];
}

/** The regional palette for a country, if it has one. */
export function regionOfCountry(code: string | undefined): Exclude<Region, "auto"> | null {
  return (code && COUNTRY_REGION[code]) || null;
}

/** The regional palette in use: the chosen one, else the company's country's (none outside the four). */
export function regionOf(p: DisplayPrefs): Exclude<Region, "auto"> | null {
  if (p.region !== "auto") return p.region;
  return (country && COUNTRY_REGION[country]) || null;
}

/** Largest zoom on screen, theme scale and text size together. */
export const MAX_ZOOM = 1.35;

/**
 * Text size × the theme's own scale, never above MAX_ZOOM. Above 100 % the
 * steps share what is left under the cap, so with Comfort (120 %) the five
 * sizes still differ: 108, 120, 124, 129 and 135 %.
 */
export function zoomOf(p: DisplayPrefs): number {
  const base = themeInfo(p.theme).zoom ?? 1;
  const t = p.text / 100;
  const top = TEXT_SIZES[TEXT_SIZES.length - 1] / 100;
  const z = t <= 1 ? base * t : base + ((t - 1) / (top - 1)) * (MAX_ZOOM - base);
  return Math.round(Math.min(MAX_ZOOM, z) * 100) / 100;
}

function apply(p: DisplayPrefs): void {
  shown = p;
  const root = document.documentElement;
  root.dataset.theme = p.theme;
  root.dataset.mode = p.mode === "auto" ? (systemDark() ? "dark" : "light") : p.mode;
  root.dataset.density = p.density;
  const region = p.theme === "regional" ? regionOf(p) : null;
  if (region) root.dataset.region = region;
  else delete root.dataset.region;
  const zoom = zoomOf(p);
  if (zoom === 1) root.style.removeProperty("--zoom");
  else root.style.setProperty("--zoom", String(zoom));
  root.classList.toggle("zoomed", zoom !== 1);
  syncTitleBar(p.mode);
}

/**
 * The desktop window's title bar follows Light or Dark; Auto hands it back to
 * the OS (null). Phones have no title bar; outside Tauri the call just fails.
 */
function syncTitleBar(mode: Mode): void {
  if (mode === windowMode || isMobile()) return;
  windowMode = mode;
  try {
    void getCurrentWindow()
      .setTheme(mode === "auto" ? null : mode)
      .catch(() => undefined);
  } catch {
    /* not running in Tauri (a browser preview) */
  }
}

/** The choice in effect (not a preview). */
export function displayPrefs(): DisplayPrefs {
  return { ...kept };
}

/** Show a choice without keeping it (the menus' live preview). */
export function previewDisplay(p: Partial<DisplayPrefs>): void {
  const next = { ...kept, ...p };
  if (JSON.stringify(next) !== JSON.stringify(shown)) apply(next);
}

/** Back to the kept choice (Esc in a preview menu). */
export function revertDisplay(): void {
  if (JSON.stringify(kept) !== JSON.stringify(shown)) apply(kept);
}

/** Keep a choice: for this device and, when someone is signed in (`scope`), for them. */
export function setDisplay(p: Partial<DisplayPrefs>, scope?: string): void {
  kept = { ...kept, ...p };
  apply(kept);
  try {
    const v = JSON.stringify(kept);
    localStorage.setItem(STORE, v);
    if (scope) localStorage.setItem(`${STORE}:${scope}`, v);
  } catch {
    /* storage unavailable: the choice lasts until the app closes */
  }
}

/** The choice saved for a signed-in user (`scope`), else for this device, else the defaults; applied at once. */
export function loadDisplay(scope?: string): void {
  let saved: Partial<DisplayPrefs> = {};
  try {
    const v = (scope && localStorage.getItem(`${STORE}:${scope}`)) || localStorage.getItem(STORE);
    if (v) saved = valid(JSON.parse(v));
  } catch {
    /* unreadable or unavailable: the defaults */
  }
  kept = { ...DEFAULTS, ...saved };
  apply(kept);
}

/** The open company's country, for Regional on "the company's country". */
export function setDisplayCountry(code: string | undefined): void {
  if (code === country) return;
  country = code;
  apply(shown);
}

/** Start: apply the saved choice before the first screen, and follow the system in Auto. */
export function initDisplay(): void {
  loadDisplay();
  try {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (shown.mode === "auto") apply(shown);
    });
  } catch {
    /* no media queries: Auto stays as first read */
  }
}

// Phones and tablets (Tauri 2 Android / iOS): the same keyboard-first app,
// made usable by touch. Nothing here runs on the desktop: `initPlatform()`
// returns at once unless `isMobile()`, and every mobile style in mobile.css
// is scoped under `html.mobile`.
//
// - `html.mobile` (plus `android` / `ios`) on <html>.
// - A Back button (= Esc) and a function-key menu button in the title strip;
//   the F-key side panel opens as a drawer from it.
// - The Android back gesture is Esc too.
// - Amount fields (`input.amount`) get the decimal keypad.
// - A calculator button in the title strip opens the calculator panel (Ctrl+N,
//   calcpanel.ts), which brings its own keypad.
// - A focused field scrolls above a list picker's bottom sheet.
// - Touch: chord hints on buttons ("Ctrl+A: Accept") hide until a hardware
//   keyboard is used (`hwkeys`); the keys themselves always work.
import { onBackButtonPress } from "@tauri-apps/api/app";
import { t } from "./i18n";

function forced(): boolean {
  // Preview the phone layout in a desktop browser: `?mobile` in the URL.
  try {
    return new URLSearchParams(window.location.search).has("mobile");
  } catch {
    return false;
  }
}

/** Running on Android or iOS (phone or tablet), not a desktop OS. */
export function isMobile(): boolean {
  const ua = navigator.userAgent;
  if (/Android|iPhone|iPad|iPod/i.test(ua)) return true;
  // iPadOS reports a Mac user agent; real Macs have no touch points.
  if (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1) return true;
  return forced();
}

/** A touch screen is the main pointer. */
export function isTouch(): boolean {
  return window.matchMedia?.("(pointer: coarse)").matches === true || navigator.maxTouchPoints > 0;
}

/** Send a key to the shell as if typed (it listens on window, capture phase). */
function sendKey(key: string): void {
  const target = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
  target.dispatchEvent(new KeyboardEvent("keydown", { key, code: key, bubbles: true, cancelable: true }));
}

function setFkeys(open: boolean): void {
  document.documentElement.classList.toggle("fkeys-open", open);
  document.querySelector(".m-fkeys")?.setAttribute("aria-expanded", String(open));
}

function chromeButton(cls: string, onTap: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = cls;
  b.tabIndex = -1;
  // Keep the keyboard focus (and the soft keyboard) in the current field.
  b.addEventListener("pointerdown", (e) => e.preventDefault());
  b.addEventListener("click", onTap);
  return b;
}

/** The top-bar search shows "Alt+F" as its placeholder; with no keyboard it shows "Find" instead. */
function searchHint(): void {
  const s = document.querySelector<HTMLInputElement>(".topbar .search");
  if (!s) return;
  const html = document.documentElement;
  s.dataset.keyHint ??= s.placeholder;
  const keys = !html.classList.contains("touch") || html.classList.contains("hwkeys");
  s.placeholder = keys ? s.dataset.keyHint : s.getAttribute("aria-label") ?? s.dataset.keyHint;
}

function label(): void {
  searchHint();
  const back = document.querySelector<HTMLElement>(".m-back");
  if (back) {
    back.textContent = t("Back");
    back.setAttribute("aria-label", t("Back"));
  }
  const menu = document.querySelector<HTMLElement>(".m-fkeys");
  if (menu) {
    menu.textContent = "F1–F12";
    menu.setAttribute("aria-label", t("Function keys"));
  }
  const calc = document.querySelector<HTMLElement>(".m-calc");
  if (calc) {
    calc.textContent = "±÷";
    calc.setAttribute("aria-label", t("Calculator"));
  }
}

/** Add the Back and function-key buttons once the shell has rendered. */
function addChrome(): void {
  const strip = document.querySelector(".shell > .titlestrip");
  if (!strip || strip.querySelector(".m-back")) return;
  strip.prepend(chromeButton("m-back", () => sendKey("Escape")));
  const menu = chromeButton("m-fkeys", () => setFkeys(!document.documentElement.classList.contains("fkeys-open")));
  menu.setAttribute("aria-expanded", "false");
  // The shell listens for this; the button keeps the focus in the field the result goes back to.
  const calc = chromeButton("m-calc", () => window.dispatchEvent(new CustomEvent("khataos:calculator")));
  calc.setAttribute("aria-pressed", "false");
  strip.append(calc, menu);
  const scrim = document.createElement("div");
  scrim.className = "m-scrim";
  scrim.addEventListener("click", () => setFkeys(false));
  strip.parentElement?.append(scrim);
  // Choosing a function key closes the drawer.
  strip.parentElement?.querySelector(".rightbar")?.addEventListener("click", (e) => {
    if ((e.target as Element).closest("button")) setFkeys(false);
  });
  label();
}

function decimalKeypad(root: ParentNode): void {
  for (const el of root.querySelectorAll<HTMLInputElement>("input.amount:not([inputmode])")) {
    el.inputMode = "decimal";
  }
}

const SEP_HOSTS = ".menu-item, .bkey";

/** "Ctrl+A: Accept": put the ": " after the chord in span.m-sep, so mobile.css can hide it with the chord. */
function wrapSeparators(root: Element): void {
  const hosts = root.matches(SEP_HOSTS) ? [root] : Array.from(root.querySelectorAll(SEP_HOSTS));
  for (const el of hosts) {
    for (const node of Array.from(el.childNodes)) {
      const prev = node.previousSibling;
      if (!(node instanceof Text) || !(prev instanceof Element) || !prev.classList.contains("key") || !node.data.startsWith(": ")) continue;
      const sep = document.createElement("span");
      sep.className = "m-sep";
      sep.textContent = ": ";
      node.data = node.data.slice(2);
      el.insertBefore(sep, node);
    }
  }
}

/** Mark the page as mobile and wire the touch helpers. Desktop: does nothing. */
export function initPlatform(): void {
  if (!isMobile()) return;
  const html = document.documentElement;
  const ua = navigator.userAgent;
  html.classList.add("mobile", /Android/i.test(ua) ? "android" : "ios");
  if (isTouch()) html.classList.add("touch");

  // Draw under the notch and home indicator; mobile.css pads with the safe-area insets.
  const viewport = document.querySelector<HTMLMetaElement>('meta[name="viewport"]');
  viewport?.setAttribute("content", "width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content");

  new MutationObserver((records) => {
    for (const r of records) {
      if (r.type === "attributes") {
        if (r.target === html && r.attributeName === "lang") label();
        else if (r.target instanceof HTMLInputElement && r.target.classList.contains("amount") && !r.target.hasAttribute("inputmode")) {
          r.target.inputMode = "decimal";
        }
        continue;
      }
      for (const n of r.addedNodes) {
        if (!(n instanceof Element)) continue;
        if (n instanceof HTMLInputElement) decimalKeypad(n.parentNode ?? n);
        else decimalKeypad(n);
        wrapSeparators(n);
      }
    }
    addChrome();
  }).observe(html, { subtree: true, childList: true, attributes: true, attributeFilter: ["class", "lang"] });

  // Keep the field being typed in above a list picker's bottom sheet.
  document.addEventListener("focusin", (e) => {
    const el = e.target;
    if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return;
    window.setTimeout(() => {
      const sheet = document.querySelector(".screen:not(.hidden) > .picker.side");
      el.scrollIntoView({ block: sheet ? "start" : "nearest" });
    }, 60);
  });

  // A hardware keyboard (modifiers, F-keys, Esc never come from a soft
  // keyboard) brings the chord hints back. Synthetic keys from the chrome
  // buttons are not trusted, so they don't count.
  window.addEventListener(
    "keydown",
    (e) => {
      if (e.isTrusted && (e.ctrlKey || e.altKey || e.metaKey || e.key === "Escape" || /^F\d{1,2}$/.test(e.key))) {
        if (!html.classList.contains("hwkeys")) {
          html.classList.add("hwkeys");
          searchHint();
        }
      }
    },
    { capture: true, passive: true },
  );

  // Android back gesture/button: Esc, the app's own "go back".
  if (html.classList.contains("android")) {
    onBackButtonPress(() => sendKey("Escape")).catch(() => undefined);
  }
}

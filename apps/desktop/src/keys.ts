// Keyboard events → khata-keymap chord strings, and resolution against the
// binding list fetched from Rust (`keymap` command). There is no key table
// in TypeScript: every binding comes from crates/khata-keymap.
import type { Binding, Context } from "./types";

const NAMED: Record<string, string> = {
  Enter: "Enter",
  Escape: "Esc",
  Esc: "Esc",
  Backspace: "Backspace",
  Tab: "Tab",
  " ": "Space",
  Spacebar: "Space",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Home: "Home",
  End: "End",
};

function keyName(e: KeyboardEvent): string | null {
  if (NAMED[e.key]) return NAMED[e.key];
  if (/^F([1-9]|1[0-2])$/.test(e.key)) return e.key;
  // Without modifiers, trust the layout's character; with Alt (which on
  // macOS produces symbols such as "ç"), or on a non-Latin layout (Arabic
  // types "ؤ" on the C key), fall back to the physical key.
  if (!e.altKey && !e.ctrlKey && /^[a-zA-Z0-9]$/.test(e.key)) return e.key.toUpperCase();
  const m = /^(?:Key([A-Z])|Digit([0-9])|Numpad([0-9]))$/.exec(e.code);
  if (m) return m[1] ?? m[2] ?? m[3];
  return null;
}

/** "Ctrl+Alt+U", "F8", "Shift+Enter", "K" — the same text as Rust's `Chord` Display. */
export function chordOf(e: KeyboardEvent): string | null {
  if (["Control", "Alt", "Shift", "Meta", "AltGraph"].includes(e.key)) return null;
  if (e.metaKey) return null; // leave Cmd shortcuts (copy, paste, quit) to the OS
  const key = keyName(e);
  if (!key) return null;
  return (e.ctrlKey ? "Ctrl+" : "") + (e.altKey ? "Alt+" : "") + (e.shiftKey ? "Shift+" : "") + key;
}

export class Keymap {
  private byKey = new Map<string, string>();
  private byAction = new Map<string, string[]>();

  constructor(readonly bindings: Binding[]) {
    for (const b of bindings) {
      this.byKey.set(`${b.context}|${b.chord}`, b.action);
      const k = `${b.context}|${b.action}`;
      this.byAction.set(k, [...(this.byAction.get(k) ?? []), b.chord]);
    }
  }

  /** Most specific context first, e.g. ["ListField", "Voucher", "Global"]. */
  resolve(chord: string, contexts: Context[]): string | null {
    for (const c of contexts) {
      const a = this.byKey.get(`${c}|${chord}`);
      if (a) return a;
    }
    return null;
  }

  /** Chords bound to an action in a context, for labels. */
  chordsFor(action: string, context: Context): string[] {
    return this.byAction.get(`${context}|${action}`) ?? [];
  }

  /** First chord for an action across contexts. */
  chordIn(action: string, contexts: Context[]): string | null {
    for (const c of contexts) {
      const k = this.chordsFor(action, c);
      if (k.length) return k[0];
    }
    return null;
  }

  /** All bindings in a context. */
  inContext(context: Context): Binding[] {
    return this.bindings.filter((b) => b.context === context);
  }
}

/**
 * The Latin letter a key stands for, lower case, for single-letter answers
 * (Y/N, D/C) and hotkeys: the typed character on Latin layouts (AZERTY and
 * Dvorak keep their own letters), the physical key on others, so an Arabic
 * layout's "غ" on the Y key still answers Yes. Other keys: the typed text.
 */
export function letterOf(e: { key: string; code?: string }): string {
  const typed = e.key.toLowerCase();
  if (/^[a-z]$/.test(typed) || e.key.length !== 1) return typed;
  const m = /^Key([A-Z])$/.exec(e.code ?? "");
  return m ? m[1].toLowerCase() : typed;
}

/** Is this a printable character the browser would type into a field? */
export function printable(e: { key: string; ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean }): boolean {
  return e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey;
}

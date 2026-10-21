// The calculator panel (Ctrl+N anywhere, Alt+C in an amount field). It docks
// at the inline end of the work area and the screen narrows beside it, so
// what is behind stays visible and keeps its state; on a phone it is a bottom
// sheet with a keypad. Keys go to the panel while it has the focus: Tab goes
// back to the screen and leaves the panel open, Esc or Ctrl+N closes it and
// the focus returns to the field it came from. Ctrl+Enter puts the result
// into that field when it takes a number (else copies it). The arithmetic is
// calc.ts; the tape is kept on this computer only, never in the company file.
import { evaluate, minorUnits, roundTo, taxPreset, taxSplit, type CalcResult, type TaxMode, type TaxPreset } from "./calc";
import "./calc.css";
import { clear, h } from "./dom";
import { language, t, type Lang } from "./i18n";
import { printable } from "./keys";
import * as M from "./money";
import { isMobile } from "./platform";
import type { KeyEv, Shell } from "./shell";

const STORE = "khataos.calc.tape";
/** Tape lines kept (per company and user on this computer). */
const KEEP = 50;

interface TapeEntry {
  expr: string;
  /** The answer as a plain decimal ("1180", "33.33"). */
  value: string;
  /** Tax worked on it: "GST" at "18" percent, added or taken out. */
  tax?: { mode: TaxMode; rate: string; name: string };
}

function isEntry(v: unknown): v is TapeEntry {
  const e = v as TapeEntry;
  return typeof e?.expr === "string" && typeof e.value === "string" && M.parse(e.value) !== null;
}

/** The field the answer can go into: an amount, quantity or rate input that can be typed in. */
function numericField(el: Element | null): HTMLInputElement | null {
  if (!(el instanceof HTMLInputElement) || !el.isConnected || el.readOnly || el.disabled) return null;
  if (el.getClientRects().length === 0) return null; // on a screen no longer shown
  const numeric = el.classList.contains("amount") || el.type === "number" || el.inputMode === "decimal" || el.inputMode === "numeric";
  return numeric ? el : null;
}

/** Only digits and separators: a figure, not a calculation. */
const BARE_NUMBER = /^[\s\d.,٠-٩۰-۹٫٬]+$/;
/** Keys that carry on from the answer when it is selected (1180, then "+"). */
const CARRY_ON = /^[+\-*/%×÷−٪)]$/;

export class CalcPanel {
  private el!: HTMLElement;
  private input!: HTMLInputElement;
  private valueEl!: HTMLElement;
  private resultEl!: HTMLElement;
  private exactEl!: HTMLElement;
  private msgEl!: HTMLElement;
  private taxEl!: HTMLElement;
  private tapeEl!: HTMLOListElement;
  private keysEl!: HTMLElement;
  private builtIn: Lang | null = null;

  private shown = false;
  private tape: TapeEntry[] = [];
  private scope: string | null = null;
  /** The tape line recalled with ↑/↓, or null while typing a new one. */
  private recall: number | null = null;
  private draft = "";
  private mode: TaxMode | null = null;
  private rateAt = -1;
  /** Where the focus was before the panel took it. */
  private prev: HTMLElement | null = null;
  private result: CalcResult = { ok: false, error: "empty" };
  private lastGood: M.Units | null = null;

  constructor(private readonly shell: Shell) {}

  // ------------------------------------------------------------ open / close

  isOpen(): boolean {
    return this.shown;
  }

  /** The panel is open and the focus is in it, so keys are its own. */
  hasFocus(): boolean {
    return this.shown && this.el.contains(document.activeElement);
  }

  /** Ctrl+N: open; with the panel open but the focus on the screen, come back to it; else close. */
  toggle(): void {
    if (!this.shown) this.open();
    else if (!this.hasFocus()) this.takeFocus();
    else this.close();
  }

  open(): void {
    if (this.builtIn !== language()) this.build();
    this.load();
    this.shown = true;
    this.el.hidden = false;
    if (!this.el.isConnected) this.shell.work.append(this.el);
    this.shell.root.classList.add("calc-open");
    document.querySelector(".m-calc")?.setAttribute("aria-pressed", "true");
    this.takeFocus();
    // Opened from an amount: start from it.
    const field = numericField(this.prev);
    const u = field?.value.trim() ? M.parse(field.value) : null;
    if (u !== null && u !== 0n) this.input.value = M.toDecimal(u);
    this.recall = null;
    this.update();
    this.paintTape();
    this.input.select();
  }

  /** Close and give the focus back where it was. */
  close(): void {
    if (!this.shown) return;
    this.shown = false;
    this.el.hidden = true;
    this.shell.root.classList.remove("calc-open");
    document.querySelector(".m-calc")?.setAttribute("aria-pressed", "false");
    this.giveBackFocus();
  }

  private takeFocus(): void {
    const active = document.activeElement;
    if (active instanceof HTMLElement && !this.el.contains(active) && active !== document.body) this.prev = active;
    this.load();
    this.input.focus();
    this.paintKeys();
  }

  /** Back to the field the panel was opened from, or the screen's own focus. */
  private giveBackFocus(): void {
    const p = this.prev;
    if (p?.isConnected && p.getClientRects().length > 0) p.focus();
    else this.shell.top()?.focus?.();
  }

  // ------------------------------------------------------------ keys

  /** Every key while the panel has the focus. True when handled (the browser's default is stopped). */
  key(e: KeyEv): boolean {
    const chord = e.chord;
    if (chord === "Esc" || (chord && this.shell.km.resolve(chord, ["AmountField", "Global"]) === "Calculator")) {
      this.close();
      return true;
    }
    switch (chord) {
      case "Enter":
        this.commit();
        return true;
      case "Ctrl+Enter":
        this.use();
        return true;
      case "Shift+Enter":
        void this.copy();
        return true;
      case "Up":
        this.step(-1);
        return true;
      case "Down":
        this.step(1);
        return true;
      case "Alt+T":
        this.cycleMode();
        return true;
      case "Alt+R":
        this.nextRate(1);
        return true;
      case "Alt+Shift+R":
        this.nextRate(-1);
        return true;
      case "Alt+D":
        this.clearTape();
        return true;
      case "Tab":
      case "Shift+Tab":
        // Back to the screen; the panel stays open beside it.
        this.giveBackFocus();
        return true;
    }
    if (printable(e) && CARRY_ON.test(e.key)) this.carryOn();
    // Editing keys work as in any field; other shortcuts (Ctrl+P, Ctrl+R…) must not reach the browser.
    const editing = /^Ctrl\+(?:Shift\+)?[ACVXZY]$|^(?:Ctrl|Alt)\+(?:Shift\+)?(?:Left|Right|Home|End|Backspace)$/;
    if (chord && (e.ctrlKey || e.altKey) && !editing.test(chord)) return true;
    return false;
  }

  /** An operator typed over the selected answer adds to it instead of replacing it. */
  private carryOn(): void {
    const i = this.input;
    if (i.value && i.selectionStart === 0 && i.selectionEnd === i.value.length) i.setSelectionRange(i.value.length, i.value.length);
  }

  /** Type text at the cursor (keypad). */
  private insert(text: string): void {
    const i = this.input;
    if (CARRY_ON.test(text)) this.carryOn();
    const from = i.selectionStart ?? i.value.length;
    const to = i.selectionEnd ?? from;
    i.setRangeText(text, from, to, "end");
    i.dispatchEvent(new Event("input", { bubbles: true }));
  }

  private backspace(): void {
    const i = this.input;
    const to = i.selectionEnd ?? i.value.length;
    const from = (i.selectionStart ?? to) === to ? Math.max(0, to - 1) : (i.selectionStart ?? to);
    i.setRangeText("", from, to, "end");
    i.dispatchEvent(new Event("input", { bubbles: true }));
  }

  private press(chord: string): void {
    this.input.focus();
    const last = chord.split("+").pop()!;
    this.key({
      chord,
      key: last === "Esc" ? "Escape" : last,
      ctrlKey: chord.includes("Ctrl+"),
      altKey: chord.includes("Alt+"),
      shiftKey: chord.includes("Shift+"),
      native: null,
    });
  }

  // ------------------------------------------------------------ working out

  private country(): string | null {
    return this.shell.company?.country?.code ?? null;
  }

  private places(): number {
    return minorUnits(this.shell.company?.country?.currency);
  }

  private preset(): TaxPreset | null {
    return taxPreset(this.country());
  }

  private rate(p: TaxPreset): string {
    if (this.rateAt < 0 || this.rateAt >= p.rates.length) this.rateAt = Math.max(0, p.rates.indexOf(p.standard));
    return p.rates[this.rateAt];
  }

  /** What Ctrl+Enter puts in the field: the result, the total with tax, or the value before tax. */
  private answer(): M.Units | null {
    if (!this.result.ok) return null;
    const places = this.places();
    const p = this.preset();
    if (!this.mode || !p) return roundTo(this.result.value, places);
    const s = taxSplit(this.result.value, M.of(this.rate(p)), this.mode, places, p.halves);
    return this.mode === "add" ? s.total : s.taxable;
  }

  private message(r: CalcResult, typing: boolean): string {
    if (r.ok) return "";
    switch (r.error) {
      case "empty":
        return "";
      case "incomplete":
        return typing ? "" : t("Finish the calculation first.");
      case "brackets": {
        const open = (this.input.value.match(/\(/g) ?? []).length - (this.input.value.match(/\)/g) ?? []).length;
        return typing && open > 0 ? "" : t("Brackets don't match.");
      }
      case "unexpected":
        return t("Unexpected \"{text}\".", { text: r.text });
      case "grouping":
        return t("\"{text}\": group digits as 1,00,000 or 100,000.", { text: r.text });
      case "divide-by-zero":
        return t("Can't divide by zero.");
    }
  }

  /** Work out the expression as it is typed. */
  private update(typing = true): void {
    const r = evaluate(this.input.value);
    this.result = r;
    const places = this.places();
    if (r.ok) this.lastGood = r.value;
    else if (r.error === "empty") this.lastGood = null;
    const shown = r.ok ? r.value : this.lastGood;
    const rounded = shown === null ? M.ZERO : roundTo(shown, places);
    this.valueEl.textContent = M.format(rounded);
    this.resultEl.classList.toggle("stale", !r.ok);
    this.resultEl.classList.toggle("blank", shown === null);
    this.exactEl.textContent =
      r.ok && rounded !== r.value ? t("Before rounding: {value}", { value: M.format(r.value) + (r.exact ? "" : "…") }) : "";
    this.msgEl.textContent = this.message(r, typing);
    this.paintTax();
  }

  private paintTax(): void {
    clear(this.taxEl);
    const p = this.preset();
    this.taxEl.hidden = !this.mode || !p;
    if (!this.mode || !p) return;
    const rate = this.rate(p);
    const tax = t(p.name);
    const head =
      this.mode === "add"
        ? t("Add {tax} at {rate}%", { tax, rate })
        : t("Take {tax} at {rate}% out (the amount includes tax)", { tax, rate });
    const chips = h(
      "div.calc-rates",
      { role: "radiogroup", "aria-label": t("Tax rate") },
      ...p.rates.map((r, i) =>
        h(
          "button.calc-rate",
          {
            type: "button",
            tabIndex: -1,
            class: r === rate ? "cur" : "",
            role: "radio",
            "aria-checked": String(r === rate),
            onpointerdown: (e: Event) => e.preventDefault(),
            onclick: () => {
              this.rateAt = i;
              this.update();
            },
          },
          `${r}%`,
        ),
      ),
    );
    this.taxEl.append(h("div.calc-tax-head", head), chips);
    if (!this.result.ok) return;
    const s = taxSplit(this.result.value, M.of(rate), this.mode, this.places(), p.halves);
    const row = (label: string, u: M.Units, cls = "") => [h("dt", { class: cls }, label), h("dd", { class: cls }, M.format(u))];
    const half = M.toDecimal(M.of(rate) / 2n);
    const taxRows = [
      ...row(`${tax} ${rate}%`, s.tax),
      ...(s.halves ? [...row(t("CGST {rate}%", { rate: half }), s.halves[0], "sub"), ...row(t("SGST {rate}%", { rate: half }), s.halves[1], "sub")] : []),
    ];
    const rows =
      this.mode === "add"
        ? [...row(t("Taxable value"), s.taxable), ...taxRows, ...row(t("Total with tax"), s.total, "ans")]
        : [...row(t("Amount with tax"), s.total), ...taxRows, ...row(t("Taxable value"), s.taxable, "ans")];
    this.taxEl.append(h("dl.calc-split", ...rows));
  }

  // ------------------------------------------------------------ actions

  /** Enter: the result goes on the tape and stays in the box, selected, to carry on from. */
  private commit(): void {
    this.update(false);
    const r = this.result;
    if (!r.ok) {
      if (r.error !== "empty") this.flash();
      return;
    }
    const answer = this.answer()!;
    const p = this.preset();
    const expr = this.input.value.trim();
    // A plain figure with no tax is not worth a tape line (Enter pressed twice).
    if (this.mode || !BARE_NUMBER.test(expr)) {
      const tax = this.mode && p ? { mode: this.mode, rate: this.rate(p), name: p.name } : undefined;
      this.tape.push({ expr, value: M.toDecimal(answer), tax });
      if (this.tape.length > KEEP) this.tape.splice(0, this.tape.length - KEEP);
      this.save();
    }
    // Carry on from the full value (100 / 3 stays 33.333333), or from the tax answer.
    this.input.value = M.toDecimal(this.mode ? answer : r.value);
    if (this.mode) this.mode = null;
    this.recall = null;
    this.update();
    this.paintTape();
    this.input.select();
  }

  /** Ctrl+Enter: into the field the panel came from when it takes a number, else onto the clipboard. */
  private use(): void {
    this.update(false);
    const answer = this.answer();
    if (answer === null) {
      this.flash();
      return;
    }
    const field = numericField(this.prev);
    if (!field) {
      void this.copy();
      return;
    }
    this.commit();
    this.close();
    // The screen's own focus handler has run (close gave the field the focus); now type the
    // answer in the way a key would, so the screen recalculates as for typing.
    if (document.activeElement !== field) field.focus();
    field.value = M.toDecimal(answer);
    field.dispatchEvent(new Event("input", { bubbles: true }));
    field.dispatchEvent(new Event("change", { bubbles: true }));
    field.select();
  }

  /** Shift+Enter (and Ctrl+Enter with no number field): copy the answer. */
  private async copy(): Promise<void> {
    this.update(false);
    const answer = this.answer();
    if (answer === null) {
      this.flash();
      return;
    }
    const text = M.toDecimal(answer);
    try {
      await navigator.clipboard.writeText(text);
      this.shell.info(t("Copied {value}.", { value: M.format(answer) }));
    } catch {
      this.shell.warn(t("Couldn't copy to the clipboard."));
    }
  }

  private cycleMode(): void {
    if (!this.preset()) {
      this.shell.warn(t("There are no tax rates for this company's country."));
      return;
    }
    this.mode = this.mode === null ? "add" : this.mode === "add" ? "remove" : null;
    this.update();
  }

  private nextRate(by: number): void {
    const p = this.preset();
    if (!p) return this.cycleMode();
    if (!this.mode) this.mode = "add";
    this.rate(p);
    this.rateAt = (this.rateAt + by + p.rates.length) % p.rates.length;
    this.update();
  }

  /** ↑/↓: walk the tape; past the newest line, back to what was being typed. */
  private step(by: number): void {
    if (!this.tape.length) return;
    if (this.recall === null) {
      if (by > 0) return;
      this.draft = this.input.value;
      this.recall = this.tape.length - 1;
    } else {
      const next = this.recall + by;
      if (next >= this.tape.length) {
        this.recall = null;
        this.input.value = this.draft;
        this.update();
        this.paintTape();
        return;
      }
      this.recall = Math.max(0, next);
    }
    this.input.value = this.tape[this.recall].expr;
    this.input.setSelectionRange(this.input.value.length, this.input.value.length);
    this.update();
    this.paintTape();
  }

  private clearTape(): void {
    if (!this.tape.length) return;
    this.tape = [];
    this.recall = null;
    this.save();
    this.paintTape();
    this.shell.info(t("Calculator history cleared."));
  }

  private flash(): void {
    this.input.classList.remove("flash");
    void this.input.offsetWidth; // restart the animation
    this.input.classList.add("flash");
  }

  // ------------------------------------------------------------ tape storage

  /** One tape per company and user on this computer (a different user shouldn't see it). */
  private scopeKey(): string {
    const c = this.shell.company;
    const user = this.shell.session?.user;
    return c ? `${STORE}:${c.path}${user ? `:${user}` : ""}` : STORE;
  }

  private load(): void {
    const key = this.scopeKey();
    if (key === this.scope) return;
    this.scope = key;
    this.tape = [];
    this.recall = null;
    try {
      const v: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
      if (Array.isArray(v)) this.tape = v.filter(isEntry).slice(-KEEP);
    } catch {
      /* storage unavailable or damaged: start an empty tape */
    }
  }

  private save(): void {
    try {
      localStorage.setItem(this.scope ?? STORE, JSON.stringify(this.tape));
    } catch {
      /* storage unavailable: the tape lasts until the app closes */
    }
  }

  // ------------------------------------------------------------ painting

  private paintTape(): void {
    clear(this.tapeEl);
    if (!this.tape.length) {
      this.tapeEl.append(h("li.calc-empty", t("Results you work out with Enter are listed here.")));
      return;
    }
    this.tape.forEach((e, i) => {
      const p = e.tax && { tax: t(e.tax.name), rate: e.tax.rate };
      const note = !e.tax || !p ? "" : e.tax.mode === "add" ? t("{tax} {rate}% added", p) : t("{tax} {rate}% taken out", p);
      const li = h(
        "li.calc-row",
        {
          class: i === this.recall ? "cur" : "",
          onpointerdown: (ev: Event) => ev.preventDefault(),
          onclick: () => {
            if (this.recall === null) this.draft = this.input.value;
            this.recall = i;
            this.input.value = e.expr;
            this.input.focus();
            this.update();
            this.paintTape();
          },
        },
        h("span.calc-row-expr", e.expr),
        h("span.calc-row-val", `= ${M.format(M.of(e.value))}`),
        note ? h("span.calc-row-note", note) : null,
      );
      this.tapeEl.append(li);
    });
    const cur = this.recall === null ? this.tapeEl.lastElementChild : this.tapeEl.children[this.recall];
    cur?.scrollIntoView({ block: "nearest" });
  }

  /** The key hints under the panel; they work as buttons too. */
  private paintKeys(): void {
    clear(this.keysEl);
    const p = this.preset();
    const into = numericField(this.prev) !== null;
    const keys: [string, string][] = [
      ["Enter", t("Calculate")],
      ["Ctrl+Enter", into ? t("Put in field") : t("Copy")],
      ...(into ? ([["Shift+Enter", t("Copy")]] as [string, string][]) : []),
      ...(p ? ([["Alt+T", t("Add or take out {tax}", { tax: t(p.name) })]] as [string, string][]) : []),
      ...(p && p.rates.length > 1 ? ([["Alt+R", t("Tax rate")]] as [string, string][]) : []),
      ["Alt+D", t("Clear history")],
      ["Esc", t("Close")],
    ];
    for (const [chord, label] of keys) {
      this.keysEl.append(
        h(
          "button.bkey",
          { type: "button", tabIndex: -1, onpointerdown: (e: Event) => e.preventDefault(), onclick: () => this.press(chord) },
          h("span.key", chord),
          `: ${label}`,
        ),
      );
    }
    this.keysEl.append(h("span.calc-hint", h("span.key", "↑ ↓"), `: ${t("History")}`));
  }

  private build(): void {
    const old = this.el;
    this.builtIn = language();
    const chord = this.shell.km.chordIn("Calculator", ["Global"]) ?? "Esc";
    this.input = h("input.field.calc-expr", {
      spellcheck: false,
      autocomplete: "off",
      dir: "ltr",
      value: old ? this.input.value : "",
      "aria-label": t("Calculation"),
      placeholder: "1000 + 18%",
    });
    // A phone types with the panel's own keypad, not the system keyboard (a hardware one still works).
    if (isMobile()) this.input.inputMode = "none";
    this.input.addEventListener("input", () => {
      this.recall = null;
      this.update();
      this.paintTape();
    });
    this.valueEl = h("span.calc-value", "0.00");
    this.resultEl = h("div.calc-result", { "aria-live": "polite", "aria-label": t("Result") }, h("span.calc-eq", "="), this.valueEl);
    this.exactEl = h("div.calc-exact");
    this.msgEl = h("div.calc-msg", { role: "status" });
    this.taxEl = h("div.calc-tax");
    this.taxEl.hidden = true;
    this.tapeEl = h("ol.calc-tape", { "aria-label": t("History") });
    this.keysEl = h("div.calc-keys");
    const close = h(
      "button.calc-close",
      { type: "button", tabIndex: -1, "aria-label": t("Close"), onpointerdown: (e: Event) => e.preventDefault(), onclick: () => this.close() },
      h("span.key", chord),
      ` ${t("Close")}`,
    );
    this.el = h(
      "aside.calc",
      { "aria-label": t("Calculator") },
      h("div.calc-head", h("span.calc-title", t("Calculator")), close),
      this.tapeEl,
      h("div.calc-now", this.input, this.resultEl, this.exactEl, this.msgEl),
      this.taxEl,
      this.keypad(),
      this.keysEl,
    );
    this.el.hidden = !this.shown;
    if (old?.isConnected) old.replaceWith(this.el);
  }

  /** Phones: digits, operators, and the panel's actions as large buttons. */
  private keypad(): HTMLElement {
    const key = (label: string, run: () => void, cls = "", aria?: string) =>
      h(
        "button.calc-pad",
        { type: "button", tabIndex: -1, class: cls, "aria-label": aria, onpointerdown: (e: Event) => e.preventDefault(), onclick: run },
        label,
      );
    const put = (s: string, cls = "") => key(s, () => this.insert(s), cls);
    return h(
      "div.calc-keypad",
      key("C", () => {
        this.input.value = "";
        this.input.dispatchEvent(new Event("input", { bubbles: true }));
      }, "fn", t("Clear")),
      put("(", "fn"),
      put(")", "fn"),
      key("⌫", () => this.backspace(), "fn", t("Delete the last character")),
      put("7"),
      put("8"),
      put("9"),
      put("÷", "op"),
      put("4"),
      put("5"),
      put("6"),
      put("×", "op"),
      put("1"),
      put("2"),
      put("3"),
      put("−", "op"),
      put("0"),
      put("."),
      put("%", "op"),
      put("+", "op"),
      key(t("Tax"), () => this.cycleMode(), "fn"),
      key("↑", () => this.step(-1), "fn", t("History")),
      key("=", () => this.commit(), "eq"),
      key("↵", () => this.use(), "use", t("Put in field")),
    );
  }
}

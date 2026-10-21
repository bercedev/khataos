// The app shell: the four fixed zones (L1), the screen stack (L7), the one
// global keydown handler, keystroke buffering during transitions (F8),
// button bars with fixed F-key slots (L2–L4), messages (M1) and prompts (F5, F6).
import { api } from "./api";
import { CalcPanel } from "./calcpanel";
import * as D from "./dates";
import { clear, h, hotkeyLabel } from "./dom";
// `tr`, not `t`: this file's locals call the top screen `t`.
import { language, saveLanguage, savedLanguage, setLanguage, t as tr, type Lang } from "./i18n";
import { chordOf, Keymap, letterOf, printable } from "./keys";
import { arabicMessage } from "./locales/ar-errors";
import { useGroupingOf } from "./money";
import { ROLE_LABEL, SignInScreen } from "./screens/signin";
import { displayPrefs, loadDisplay, regionOfCountry, setDisplay, setDisplayCountry, type DisplayPrefs } from "./theme";
import type { CompanyInfo, Context, Session } from "./types";

export interface KeyEv {
  chord: string | null;
  key: string;
  /** Physical key ("KeyY"), for letter answers on non-Latin layouts (keys.ts letterOf). */
  code?: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  /** null when the key is replayed from the transition buffer. */
  native: KeyboardEvent | null;
}

export interface Btn {
  chord: string;
  label: string;
  enabled?: boolean;
}

/** One screen (or overlay, or modal) in the stack. */
export interface Screen {
  title: string;
  el: HTMLElement;
  /** Stack over the parent, which stays visible but faded (L7). */
  overlay?: boolean;
  /** CSS class for a background tint (L5). */
  tint?: string;
  /** Most specific first; the shell appends Global. */
  contexts(): Context[];
  /** Whether a resolved action does something here (enables bar buttons). */
  handles?(action: string): boolean;
  /** "You are here" (L4): show this action's button greyed. */
  current?(action: string): boolean;
  /** This screen's English name for an action on the function-key bar (F2 is "Period" on a report over a range). */
  keyLabel?(action: string): string | undefined;
  bottom?(): Btn[];
  /** Load data. Keys typed meanwhile are buffered and replayed (F8). */
  init?(): Promise<void>;
  focus?(): void;
  /** Raw key, before the keymap. Return true when handled. */
  key?(e: KeyEv): boolean;
  /** A resolved keymap action. Return true when handled. */
  action?(action: string, e: KeyEv): boolean;
  /** A child screen above this one closed. */
  resume?(): void;
  dispose?(): void;
  /** A fresh copy built in the new display language (Ctrl+K), for screens that can. */
  rebuild?(): Screen;
}

/** Human labels for actions on buttons. Keys come from the keymap. */
const LABELS: Record<string, string> = {
  Accept: "Accept",
  Back: "Back",
  Quit: "Quit",
  QuitApp: "Quit",
  Help: "Help",
  GoTo: "Go To",
  SwitchTo: "Switch To",
  Find: "Find",
  CompanyMenu: "Company",
  DataMenu: "Data",
  ExchangeMenu: "Exchange",
  ImportMenu: "Import",
  ExportMenu: "Export",
  ShareMenu: "Share",
  DisplayLanguage: "Language",
  PrintMenu: "Print",
  ChangeDate: "Date",
  ChangePeriod: "Period",
  SwitchCompany: "Company",
  OpenCompany: "Open Company",
  ShutCompany: "Shut Company",
  CompanyFeatures: "Features",
  Configure: "Configure",
  ChangeMode: "Change Mode",
  RemoveLine: "Remove Line",
  DeleteRecord: "Delete",
  CancelVoucher: "Cancel Vch",
  CreateMasterInline: "Create",
  AlterMasterInline: "Alter",
  DrillDown: "Open",
  ToggleDetailed: "Detailed",
  ChangeContext: "Context",
  AltView1: "View 1",
  AltView2: "View 2",
  "OpenVoucher:Contra": "Contra",
  "OpenVoucher:Payment": "Payment",
  "OpenVoucher:Receipt": "Receipt",
  "OpenVoucher:Journal": "Journal",
  "OpenVoucher:Sales": "Sales",
  "OpenVoucher:Purchase": "Purchase",
  "OpenVoucher:CreditNote": "Credit Note",
  "OpenVoucher:DebitNote": "Debit Note",
  "OpenVoucher:Other": "Other Vouchers",
  "OpenVoucher:SalesOrder": "Sales Order",
  "OpenVoucher:PurchaseOrder": "Purchase Order",
  "OpenVoucher:StockJournal": "Stock Journal",
};

/** The label in the display language. */
export function labelFor(action: string | null): string {
  if (!action) return "";
  if (LABELS[action]) return tr(LABELS[action]);
  const tail = action.includes(":") ? action.split(":")[1] : action;
  return tr(tail.replace(/([a-z])([A-Z])/g, "$1 $2"));
}

// Ctrl+K (language) stays off this bar: it is full at 1360 px. The company list and Company menu show it.
const TOP_MENU = ["CompanyMenu", "DataMenu", "ExchangeMenu", "GoTo", "ImportMenu", "ExportMenu", "ShareMenu", "PrintMenu", "Help"];
const SHELL_ACTIONS = new Set([
  "Back", "Quit", "QuitApp", "SwitchCompany", "OpenCompany", "ShutCompany", "CompanyMenu", "DataMenu", "GoTo", "Find", "ImportMenu", "SyncNow",
  "DisplayLanguage", "ChangeDate", "ChangePeriod", "Calculator",
]);
const NOT_YET = new Set([
  "ExchangeMenu", "ExportMenu", "ShareMenu", "PrintMenu", "Help", "ContextHelp",
  "ExportCurrent", "EmailCurrent", "WhatsAppCurrent", "PrintCurrent", "CompanyFeatures", "Configure",
  "EntryLanguage", "Notifications", "SwitchTo",
]);
/** Bound in the Tally key map but not built yet (docs/research/tally-gap-*.md). Pressing one
 *  says so instead of doing nothing. Ctrl+Home/End and Shift+Enter are left out: text fields use them. */
const NOT_BUILT = new Set([
  "AlterColumn", "AutoColumns", "BasisOfValues", "ChangeContext", "DuplicateVoucher", "EditLog", "ExceptionReports",
  "FetchByTaxId", "FilterDetails", "HideLine", "InsertVoucherBefore", "InvertSelection", "MarkOptional", "MarkPostDated",
  "RelatedReports", "RestoreLine", "SaveView", "StatAdjustment", "StockQuery", "TaxAnalysis", "UnusedVoucherNumber",
]);
/** Shell actions that use the open company's books, so need a sign-in. */
const NEEDS_SIGN_IN = new Set(["ChangeDate", "ChangePeriod", "CompanyMenu", "DataMenu", "GoTo", "Find", "ImportMenu", "SyncNow", "ExchangeMenu", "ShareMenu"]);
const FSLOTS = ["F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

type Hooks = {
  startup: () => Screen;
  home: () => Screen;
  /** First open of a company with no country yet (rule C2). */
  country: () => Screen;
  find: () => Screen;
  goTo: () => Screen;
  companyMenu: () => Screen;
  importMenu: () => Screen;
  /** Alt+Y: Backup, Restore, Automatic backups. Restore works with no company open. */
  dataMenu?: () => Screen;
  syncNow: () => Promise<void>;
  /** Alt+Z (sharing: screens/sharing.ts). Works with no company open. */
  exchangeMenu?: () => Screen;
  /** Alt+M. */
  shareMenu?: () => Screen;
  /** Ctrl+K: English or Arabic. Works with no company open. */
  languageMenu?: () => Screen;
  /** The top bar's "More": the actions that didn't fit, each with its chord. */
  moreMenu?: (items: { action: string; label: string; chord: string }[]) => Screen;
};

interface Toast {
  el: HTMLElement;
  timer?: number;
}

export class Shell {
  readonly km: Keymap;
  company: CompanyInfo | null = null;
  /** F2 at the top level: the current date until the company closes (null: the computer's date). */
  private currentDate: string | null = null;
  /** Alt+F2 at the top level: the current period until the company closes (null: the financial year to date). */
  private currentPeriod: { from: string; to: string } | null = null;
  /** Who is signed in to the open company; null with no company open. */
  session: Session | null = null;
  private stack: Screen[] = [];
  private modals: Screen[] = [];
  private busy = 0;
  private buffer: KeyEv[] = [];
  private toasts: Toast[] = [];
  private hooks!: Hooks;
  /** Ctrl+N: docked beside the screens, made on first use (calcpanel.ts). */
  private calc: CalcPanel | null = null;

  readonly root: HTMLElement;
  readonly searchInput: HTMLInputElement;
  private menuEl: HTMLElement;
  private titleLeft: HTMLElement;
  private titleCentre: HTMLElement;
  private titleRight: HTMLElement;
  readonly work: HTMLElement;
  private rightBar: HTMLElement;
  private bottomBar: HTMLElement;
  private toastLayer: HTMLElement;
  private modalLayer: HTMLElement;

  constructor(km: Keymap, mount: HTMLElement) {
    this.km = km;
    this.searchInput = h("input.search", { placeholder: "Alt+F", spellcheck: false, "aria-label": tr("Find") });
    this.searchInput.addEventListener("focus", () => {
      if (!this.top()?.title.startsWith("Find")) this.runShellAction("Find");
    });
    this.menuEl = h("nav.menu", { "aria-label": tr("Action menu") });
    this.titleLeft = h("div.title-left");
    this.titleCentre = h("div.title-centre");
    this.titleRight = h("div.title-right");
    this.work = h("main.work");
    this.rightBar = h("aside.rightbar", { "aria-label": tr("Function keys") });
    this.bottomBar = h("footer.bottombar");
    this.toastLayer = h("div.toasts", { "aria-live": "polite" });
    this.modalLayer = h("div.modals");
    this.root = h(
      "div.shell",
      h("header.topbar", h("div.brand", h("span.brand-mark"), h("span", "Khata", h("b", "OS"))), this.searchInput, this.menuEl),
      h("div.titlestrip", this.titleLeft, this.titleCentre, this.titleRight),
      this.work,
      this.rightBar,
      this.bottomBar,
      this.toastLayer,
      this.modalLayer,
    );
    mount.replaceChildren(this.root);
    // The top bar's width changes with the window and the text size: refit its menu.
    new ResizeObserver(() => this.fitMenu()).observe(this.menuEl.parentElement!);
    window.addEventListener("keydown", (e) => this.onKeyDown(e), true);
    window.addEventListener("blur", () => document.body.classList.add("inactive"));
    window.addEventListener("focus", () => document.body.classList.remove("inactive"));
    // The phone's calculator button (platform.ts) has no chord to send.
    window.addEventListener("khataos:calculator", () => this.toggleCalculator());
    this.renderMenu();
  }

  setHooks(hooks: Hooks): void {
    this.hooks = hooks;
  }

  top(): Screen | undefined {
    return this.stack[this.stack.length - 1];
  }

  depth(): number {
    return this.stack.length;
  }

  // ------------------------------------------------------------ stack

  /** Open a screen. Keys typed while it loads are buffered (F8). */
  async push(s: Screen): Promise<boolean> {
    this.busy++;
    this.document(s, "loading");
    try {
      await s.init?.();
    } catch (e) {
      s.el.remove();
      this.busy--;
      this.error(e);
      this.flush();
      return false;
    }
    this.busy--;
    this.stack.push(s);
    s.el.classList.remove("loading");
    this.layer();
    this.activate();
    this.flush();
    return true;
  }

  /**
   * Run slow work (sync, import) as a transition: keys typed meanwhile are
   * buffered and replayed afterwards (rule F8).
   */
  async transition<T>(work: Promise<T>): Promise<T> {
    this.busy++;
    document.body.classList.add("working");
    try {
      return await work;
    } finally {
      this.busy--;
      if (this.busy === 0) document.body.classList.remove("working");
      this.flush();
    }
  }

  /** Close the top screen and return to the one below, cursor intact. */
  pop(): void {
    const s = this.stack.pop();
    if (!s) return;
    s.dispose?.();
    s.el.remove();
    this.layer();
    this.top()?.resume?.();
    this.activate();
  }

  /**
   * Show the top screen; under an overlay, the screens it floats over stay
   * visible but faded, down to the first full screen (L7). The rest hide.
   */
  private layer(): void {
    let shown = true;
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const x = this.stack[i];
      x.el.classList.toggle("hidden", !shown);
      x.el.classList.toggle("behind", shown && i < this.stack.length - 1);
      if (!x.overlay) shown = false;
    }
  }

  /** Replace the whole stack (company opened or closed). */
  async reset(s: Screen): Promise<void> {
    while (this.stack.length) {
      const x = this.stack.pop()!;
      x.dispose?.();
      x.el.remove();
    }
    await this.push(s);
  }

  private document(s: Screen, cls: string): void {
    s.el.classList.add("screen", cls);
    if (s.overlay) s.el.classList.add("overlay");
    this.work.append(s.el);
  }

  private activate(): void {
    const t = this.top();
    this.refresh();
    t?.focus?.();
  }

  /** Re-render title strip and both button bars for the top screen. */
  refresh(): void {
    const t = this.top();
    // Titles are fixed English text, so they translate here; most screens build theirs in English.
    this.titleLeft.textContent = t ? tr(t.title) : "";
    this.titleCentre.textContent = this.company?.name ?? tr("No company open");
    const sync = this.company?.sync_status;
    this.titleRight.textContent = this.company ? [this.userText(), sync].filter(Boolean).join(" · ") : "";
    this.titleRight.title = sync ? tr("Last sync: {sync}", { sync }) : "";
    this.work.className = `work ${this.tintOf()}`;
    this.renderRightBar();
    this.renderBottomBar();
  }

  /** The signed-in user and role; the computer's user when nobody signs in. */
  private userText(): string {
    const s = this.session;
    if (s?.user) return s.role ? `${s.user} (${tr(ROLE_LABEL[s.role])})` : s.user;
    return s?.required ? tr("Not signed in") : this.company?.user ?? "";
  }

  private tintOf(): string {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      if (this.stack[i].tint) return this.stack[i].tint!;
      if (!this.stack[i].overlay) break;
    }
    return "";
  }

  // ----------------------------------------------------------- keys

  private onKeyDown(native: KeyboardEvent): void {
    const chord = chordOf(native);
    if (native.isComposing) return;
    const e: KeyEv = {
      chord,
      key: native.key,
      code: native.code,
      ctrlKey: native.ctrlKey,
      altKey: native.altKey,
      shiftKey: native.shiftKey,
      native,
    };
    if (this.busy > 0) {
      // A screen is opening: keep the key for when it is ready.
      if (chord || printable(native)) {
        native.preventDefault();
        this.buffer.push({ ...e, native: null });
      }
      return;
    }
    const handled = this.dispatch(e);
    // The app owns the function keys and Tab (no browser reload, no focus escape).
    if (handled || /^(?:.*\+)?F\d+$/.test(chord ?? "") || native.key === "Tab") native.preventDefault();
  }

  /** Route one key: modal → top screen raw key → keymap action → shell action. */
  dispatch(e: KeyEv): boolean {
    this.dismissToasts();
    // The calculator panel isn't modal: it has every key while it has the focus.
    if (this.calc?.hasFocus()) return this.calc.key(e);
    const modal = this.modals[this.modals.length - 1];
    const target = modal ?? this.top();
    if (!target) return false;
    // Ctrl+N (Alt+C in an amount field) works over any screen or prompt, before the screen sees the key.
    if (e.chord && this.km.resolve(e.chord, [...target.contexts(), "Global"]) === "Calculator") return this.toggleCalculator();
    if (target.key?.(e)) return true;
    if (!e.chord) return false;
    const action = this.km.resolve(e.chord, [...target.contexts(), "Global"]);
    if (!action) return false;
    if (target.action?.(action, e)) return true;
    if (modal) return false;
    return this.runShellAction(action);
  }

  /** Play buffered keys once nothing is loading. */
  private flush(): void {
    while (this.busy === 0 && this.buffer.length) {
      const e = this.buffer.shift()!;
      if (!this.dispatch(e)) this.typeInto(e);
    }
  }

  /** A buffered printable key that nobody handled: type it into the field. */
  private typeInto(e: KeyEv): void {
    const el = document.activeElement;
    if (!(el instanceof HTMLInputElement) || el.readOnly) return;
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? start;
    if (printable(e)) {
      el.value = el.value.slice(0, start) + e.key + el.value.slice(end);
      el.setSelectionRange(start + 1, start + 1);
    } else if (e.key === "Backspace") {
      const from = start === end ? Math.max(0, start - 1) : start;
      el.value = el.value.slice(0, from) + el.value.slice(end);
      el.setSelectionRange(from, from);
    } else return;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }

  /** Simulate a chord (button clicks use the same path as the keyboard). */
  press(chord: string): void {
    const last = chord.split("+").pop()!;
    const e: KeyEv = {
      chord,
      key: last === "Esc" ? "Escape" : last,
      ctrlKey: chord.includes("Ctrl+"),
      altKey: chord.includes("Alt+"),
      shiftKey: chord.includes("Shift+"),
      native: null,
    };
    if (this.busy > 0) this.buffer.push(e);
    else this.dispatch(e);
  }

  /** A company with users is open and nobody has signed in yet. */
  private locked(): boolean {
    return Boolean(this.company && this.session?.required && !this.session.user);
  }

  private shellHandles(action: string): boolean {
    if (!SHELL_ACTIONS.has(action)) return false;
    if (this.locked() && NEEDS_SIGN_IN.has(action)) return false;
    if (["SwitchCompany", "ShutCompany", "GoTo", "Find", "CompanyMenu", "ImportMenu", "SyncNow", "ChangeDate", "ChangePeriod"].includes(action)) {
      return this.company !== null && this.company.country !== null;
    }
    return true;
  }

  runShellAction(action: string): boolean {
    if (this.locked() && NEEDS_SIGN_IN.has(action)) {
      this.warn(tr("Sign in first, or press Esc to go back to the company list."));
      return true;
    }
    switch (action) {
      case "Back":
      case "Quit":
        if (this.stack.length > 1) this.pop();
        else this.confirmQuit();
        return true;
      case "QuitApp":
        this.confirmQuit();
        return true;
      case "SwitchCompany":
      case "OpenCompany":
      case "ShutCompany":
        void this.closeCompany();
        return true;
      case "CompanyMenu":
        if (!this.company) void this.closeCompany();
        else void this.push(this.hooks.companyMenu());
        return true;
      case "ImportMenu":
        if (!this.company) this.warn(tr("Open a company first, then import into it."));
        else void this.push(this.hooks.importMenu());
        return true;
      case "DataMenu":
        if (!this.hooks.dataMenu) break;
        void this.push(this.hooks.dataMenu());
        return true;
      case "SyncNow":
        if (!this.company) return false;
        void this.hooks.syncNow();
        return true;
      case "ExchangeMenu":
        if (!this.hooks.exchangeMenu) break;
        void this.push(this.hooks.exchangeMenu());
        return true;
      case "ShareMenu":
        if (!this.hooks.shareMenu) break;
        if (!this.company) this.warn(tr("Open a company first, then share it."));
        else void this.push(this.hooks.shareMenu());
        return true;
      case "Find":
        if (!this.company) return false;
        void this.push(this.hooks.find());
        return true;
      case "GoTo":
        if (!this.company) return false;
        void this.push(this.hooks.goTo());
        return true;
      case "ChangeDate":
        if (!this.company?.country) return false;
        void this.askCurrentDate();
        return true;
      case "ChangePeriod":
        if (!this.company?.country) return false;
        void this.askCurrentPeriod();
        return true;
      case "DisplayLanguage":
        if (!this.hooks.languageMenu) break;
        void this.push(this.hooks.languageMenu());
        return true;
      case "Calculator":
        return this.toggleCalculator();
    }
    if (NOT_YET.has(action) || NOT_BUILT.has(action)) {
      this.warn(tr("{what} isn't available in this version yet.", { what: labelFor(action) }));
      return true;
    }
    return false;
  }

  /** Ctrl+N: open the calculator panel, return to it, or close it (calcpanel.ts). Needs no company. */
  toggleCalculator(): boolean {
    this.calc ??= new CalcPanel(this);
    this.calc.toggle();
    return true;
  }

  /** The first day of the current period (Alt+F2); by default the financial year of the current date. */
  periodFrom(): string {
    if (this.currentPeriod) return this.currentPeriod.from;
    return D.fyStart(this.company?.today ?? D.todayIso(), this.fyStartMonth());
  }

  /** The last day of the current period (Alt+F2); by default the current date. */
  periodTo(): string {
    return this.currentPeriod?.to ?? this.company?.today ?? D.todayIso();
  }

  /** Where a statement drilled from a report "as at" `asAt` starts: the current period's
   *  start, or the financial year of `asAt` when that falls before the period. */
  periodFromFor(asAt: string): string {
    const from = this.periodFrom();
    return from <= asAt ? from : D.fyStart(asAt, this.fyStartMonth());
  }

  /** Whether Alt+F2 has set a period (the home screen says so). */
  periodSet(): boolean {
    return this.currentPeriod !== null;
  }

  private fyStartMonth(): number {
    return Number(this.company?.fy_start.slice(5, 7)) || 4;
  }

  /** F2 outside a voucher or report: the current date, the default for new vouchers and
   *  return periods, until the company is closed. */
  private async askCurrentDate(): Promise<void> {
    const ref = this.company?.today ?? D.todayIso();
    const v = await this.dialog("Change current date", [{ label: "Current date", value: D.editable(ref) }], ([a]) =>
      D.parseUser(a, ref) ? null : tr('"{text}" is not a date.', { text: a }),
    );
    const date = v && D.parseUser(v[0], ref);
    if (!date || !this.company) return;
    this.currentDate = date;
    this.company.today = date;
    this.info(tr("Current date: {date}.", { date: D.show(date) }));
    await this.reloadAfterDateChange();
  }

  /** Alt+F2 outside a report: the current period every report opens with, until the company
   *  is closed. Leave To blank for the whole financial year that From falls in, so "1-4-25"
   *  alone moves to 2025-26. */
  private async askCurrentPeriod(): Promise<void> {
    const ref = this.periodTo();
    const v = await this.dialog(
      "Change current period",
      [
        { label: "From", value: D.editable(this.periodFrom()) },
        { label: "To (blank: end of that financial year)", value: D.editable(this.periodTo()) },
      ],
      ([a, b]) => {
        const f = D.parseUser(a, ref);
        if (!f) return tr('"{text}" is not a date.', { text: a });
        if (!b.trim()) return null;
        const t = D.parseUser(b, ref);
        if (!t) return tr('"{text}" is not a date.', { text: b });
        return f > t ? tr("The period ends before it starts.") : null;
      },
    );
    const from = v && D.parseUser(v[0], ref);
    if (!v || !from || !this.company) return;
    const to = v[1].trim() ? D.parseUser(v[1], ref) : D.fyEnd(D.fyStart(from, this.fyStartMonth()));
    if (!to) return;
    this.currentPeriod = { from, to };
    this.info(tr("Current period: {from} to {to}.", { from: D.show(from), to: D.show(to) }));
    await this.reloadAfterDateChange();
  }

  /** The home screen shows the new date or period at once; other screens keep theirs. */
  private async reloadAfterDateChange(): Promise<void> {
    if (this.stack.length === 1) await this.reset(this.hooks.home());
    else this.refresh();
  }

  private async confirmQuit(): Promise<void> {
    if (await this.ask(tr("Quit KhataOS?"), { centred: true })) await api.quitApp();
  }

  // ------------------------------------------------------- language

  /** Who the language choice is saved for: the signed-in user of this company. */
  private languageScope(): string | undefined {
    const user = this.session?.user;
    return user && this.company ? `${this.company.path}:${user}` : undefined;
  }

  /**
   * Ctrl+K: switch the display language. The bars, the menu and a screen that
   * can rebuild itself (home, company list, sign-in) change at once; other
   * open screens change when they are opened again.
   */
  async setLanguage(l: Lang): Promise<void> {
    saveLanguage(l, this.languageScope());
    if (l === language()) return;
    setLanguage(l);
    this.relabel();
    const fresh = this.stack.length === 1 ? this.top()?.rebuild?.() : undefined;
    if (fresh) await this.reset(fresh);
    else this.refresh();
    if (!fresh && this.stack.length > 1) this.info(tr("Screens already open keep their language until you open them again."));
  }

  /** Keep a theme, mode, density or text size: for this device and the signed-in user (theme.ts). */
  setDisplay(p: Partial<DisplayPrefs>): void {
    setDisplay(p, this.languageScope());
  }

  /** The shell's own labels in the display language. */
  private relabel(): void {
    this.renderMenu();
    this.searchInput.setAttribute("aria-label", tr("Find"));
    this.menuEl.setAttribute("aria-label", tr("Action menu"));
    this.rightBar.setAttribute("aria-label", tr("Function keys"));
  }

  /** The signed-in user's own language, when they chose one on this computer. */
  private applyUserLanguage(): void {
    const l = savedLanguage(this.languageScope());
    if (l && l !== language()) {
      setLanguage(l);
      this.relabel();
    }
  }

  /** Once per company: a company in Saudi Arabia or the UAE hears that Arabic exists. True when it was shown. */
  suggestLanguage(): boolean {
    const c = this.company;
    // Anyone who has picked a language here already knows where the choice is.
    if (!c || !["SA", "AE"].includes(c.country?.code ?? "") || savedLanguage(this.languageScope())) return false;
    const key = `khataos.languageHint:${c.path}`;
    try {
      if (localStorage.getItem(key)) return false;
      localStorage.setItem(key, "1");
    } catch {
      return false; // without storage the hint would come back on every open
    }
    // Both languages; the Arabic half is isolated right to left (RLI … PDI) inside English text.
    this.info("Arabic is available: press Ctrl+K to switch. · \u2067العربية متاحة: اضغط Ctrl+K للتبديل.\u2069", 9000);
    return true;
  }

  /**
   * Once per device and user: a company in India, the Gulf, the UK or East
   * Asia hears that a Regional theme exists. Not for someone on Regional already.
   */
  suggestTheme(): void {
    const c = this.company;
    if (!c?.country || !regionOfCountry(c.country.code) || displayPrefs().theme === "regional") return;
    const key = `khataos.themeHint:${this.languageScope() ?? "device"}`;
    try {
      if (localStorage.getItem(key)) return;
      localStorage.setItem(key, "1");
    } catch {
      return; // without storage the hint would come back on every open
    }
    this.info(tr("A Regional theme for {country} is available: Ctrl+K → Theme.", { country: tr(c.country.name) }), 9000);
  }

  async openCompany(info: CompanyInfo): Promise<void> {
    this.company = info;
    this.currentDate = null;
    this.currentPeriod = null;
    useGroupingOf(info.country?.code);
    setDisplayCountry(info.country?.code);
    try {
      localStorage.setItem("khataos.lastCompany", info.path);
    } catch {
      /* storage unavailable: only a convenience */
    }
    // A company with users asks who is there before anything else.
    await this.loadSession();
    if (this.session?.required && !this.session.user) await this.reset(new SignInScreen(this));
    else await this.enter();
  }

  /** Home, or the country pick on a company's first open. */
  private async enter(): Promise<void> {
    const info = this.company;
    if (!info) return;
    // A Viewer cannot pick the country; the owner's pick arrives with sync.
    await this.reset(info.country || info.read_only ? this.hooks.home() : this.hooks.country());
  }

  /** The sign-in screen succeeded. */
  async signedIn(session: Session): Promise<void> {
    this.session = session;
    this.applyUserLanguage();
    loadDisplay(this.languageScope());
    await this.refreshCompany();
    await this.enter();
  }

  /** Company → Sign out: back to the sign-in screen, the company stays open. */
  async signOut(): Promise<void> {
    try {
      await api.signOut();
    } catch (e) {
      return this.error(e);
    }
    await this.loadSession();
    if (this.session?.required) await this.reset(new SignInScreen(this));
    else await this.enter();
  }

  /** Ask the backend who is signed in (users may have been added or removed). */
  async loadSession(): Promise<void> {
    if (!this.company) {
      this.session = null;
      return;
    }
    try {
      this.session = await api.sessionInfo();
    } catch {
      this.session = null;
    }
  }

  async closeCompany(): Promise<void> {
    if (this.company) await api.closeCompany();
    this.company = null;
    this.session = null;
    this.currentDate = null;
    this.currentPeriod = null;
    setDisplayCountry(undefined);
    loadDisplay();
    await this.reset(this.hooks.startup());
  }

  async refreshCompany(): Promise<void> {
    if (!this.company) return;
    try {
      this.company = await api.companyInfo();
      if (this.currentDate) this.company.today = this.currentDate;
      useGroupingOf(this.company.country?.code);
      setDisplayCountry(this.company.country?.code);
    } catch {
      /* keep the last known info */
    }
  }

  // ---------------------------------------------------------- bars

  private contextsOfTop(): Context[] {
    const t = this.modals[this.modals.length - 1] ?? this.top();
    return [...(t?.contexts() ?? []), "Global"];
  }

  private enabled(action: string | null): boolean {
    if (!action) return false;
    const t = this.top();
    return Boolean(t?.handles?.(action)) || this.shellHandles(action);
  }

  private renderMenu(): void {
    clear(this.menuEl);
    for (const action of TOP_MENU) {
      const chord = this.km.chordIn(action, ["Global"]);
      if (!chord) continue;
      const usable = ["CompanyMenu", "GoTo", "ImportMenu", "ExchangeMenu", "ShareMenu"].includes(action);
      this.menuEl.append(
        h(
          "button.menu-item",
          { class: usable ? "" : "muted", tabIndex: -1, "data-action": action, "data-chord": chord, onclick: () => this.press(chord) },
          h("span.key", chord),
          `: ${labelFor(action)}`,
        ),
      );
    }
    this.menuEl.append(h("button.menu-item.more", { tabIndex: -1, hidden: true, "aria-haspopup": "menu", onclick: () => this.openMore() }, tr("More…")));
    this.fitMenu();
  }

  /**
   * Narrow windows and larger text: the actions that don't fit leave the top
   * bar from the end and wait under "More…". Their chords work as always.
   * Phones scroll the bar sideways instead (mobile.css).
   */
  private fitMenu(): void {
    if (document.documentElement.classList.contains("mobile")) return;
    const items = [...this.menuEl.querySelectorAll<HTMLElement>(".menu-item:not(.more)")];
    const more = this.menuEl.querySelector<HTMLElement>(".menu-item.more");
    if (!more) return;
    for (const i of items) i.hidden = false;
    more.hidden = true;
    const over = () => this.menuEl.scrollWidth > this.menuEl.clientWidth + 1;
    if (!over()) return;
    more.hidden = false;
    for (let k = items.length - 1; k >= 0 && over(); k--) items[k].hidden = true;
  }

  private openMore(): void {
    const hidden = [...this.menuEl.querySelectorAll<HTMLElement>(".menu-item[hidden]:not(.more)")];
    const items = hidden.map((b) => ({ action: b.dataset.action!, label: labelFor(b.dataset.action!), chord: b.dataset.chord! }));
    if (items.length && this.hooks.moreMenu) void this.push(this.hooks.moreMenu(items));
  }

  private renderRightBar(): void {
    clear(this.rightBar);
    const ctx = this.contextsOfTop();
    const t = this.top();
    for (const slot of FSLOTS) {
      const action = this.km.resolve(slot, ctx);
      const on = this.enabled(action);
      const here = Boolean(action && t?.current?.(action));
      const btn = h(
        "button.fkey",
        {
          class: on && !here ? "" : "off",
          tabIndex: -1,
          disabled: !on || here,
          "aria-current": here ? "page" : undefined,
          onclick: () => this.press(slot),
        },
        h("span.key", slot),
        h("span.lbl", action ? `: ${t?.keyLabel?.(action) ? tr(t.keyLabel(action) as string) : labelFor(action)}` : ""),
      );
      const cell = h("div.fslot", btn);
      for (const mod of ["Alt+", "Ctrl+"]) {
        const chord = mod + slot;
        const alt = this.km.resolve(chord, ctx);
        // Variants appear only for the screen's own actions, so slots stay readable.
        if (alt && t?.handles?.(alt)) {
          const altHere = Boolean(t?.current?.(alt));
          cell.append(
            h(
              "button.fkey.sub",
              { class: altHere ? "off" : "", disabled: altHere, tabIndex: -1, onclick: () => this.press(chord) },
              h("span.key", chord),
              h("span.lbl", `: ${labelFor(alt)}`),
            ),
          );
        }
      }
      this.rightBar.append(cell);
    }
  }

  private renderBottomBar(): void {
    clear(this.bottomBar);
    const t = this.top();
    for (const b of t?.bottom?.() ?? []) {
      this.bottomBar.append(
        h(
          "button.bkey",
          { class: b.enabled === false ? "off" : "", disabled: b.enabled === false, tabIndex: -1, onclick: () => this.press(b.chord) },
          h("span.key", b.chord),
          `: ${tr(b.label)}`,
        ),
      );
    }
    // A long bar (a saved invoice has many keys) packs its buttons tighter rather than hide the last ones.
    this.bottomBar.classList.remove("tight");
    this.bottomBar.classList.toggle("tight", this.bottomBar.scrollWidth > this.bottomBar.clientWidth);
  }

  /** A bottom-bar button for an action, keyed from the keymap. */
  btn(action: string, contexts: Context[], label?: string, enabled = true): Btn | null {
    const chord = this.km.chordIn(action, [...contexts, "Global"]);
    return chord ? { chord, label: label ?? labelFor(action), enabled } : null;
  }

  // ------------------------------------------------------- messages

  private toast(kind: "error" | "warn" | "info", text: string, ms = 3500): void {
    // The shell's own text is in the Arabic table; a message from Rust (English) in ar-errors.ts.
    const own = tr(text);
    const shown = own === text && language() === "ar" ? (arabicMessage(text) ?? text) : own;
    // A message with no translation reads left to right as one piece.
    const english = language() === "ar" && !/[\u0600-\u06FF]/.test(shown);
    const el = h(`div.toast`, { class: kind, role: kind === "error" ? "alert" : "status", dir: english ? "ltr" : undefined }, shown);
    const t: Toast = { el };
    // Fresh messages survive the key that caused them.
    el.dataset.fresh = "1";
    setTimeout(() => delete el.dataset.fresh, 150);
    if (kind === "info") t.timer = window.setTimeout(() => this.removeToast(t), ms);
    this.toasts.push(t);
    this.toastLayer.append(el);
  }

  private removeToast(t: Toast): void {
    t.el.remove();
    if (t.timer) clearTimeout(t.timer);
    this.toasts = this.toasts.filter((x) => x !== t);
  }

  /** Messages never block: any key dismisses them (M1). */
  private dismissToasts(): void {
    for (const t of [...this.toasts]) if (!t.el.dataset.fresh) this.removeToast(t);
  }

  error(e: unknown): void {
    this.toast("error", e instanceof Error ? e.message : String(e));
  }

  warn(text: string): void {
    this.toast("warn", text);
  }

  info(text: string, ms?: number): void {
    this.toast("info", text, ms);
  }

  // -------------------------------------------------------- modals

  /** Push a modal (prompt or small dialog) that takes every key first. */
  openModal(m: Screen): void {
    m.el.classList.add("modal");
    this.modals.push(m);
    this.modalLayer.append(m.el);
    this.renderRightBar();
    m.focus?.();
  }

  closeModal(m: Screen): void {
    this.modals = this.modals.filter((x) => x !== m);
    m.el.remove();
    this.renderRightBar();
    (this.modals[this.modals.length - 1] ?? this.top())?.focus?.();
  }

  /** "Accept? Yes / No" — a two-row list answered with Y or N (F5, P5). */
  ask(question: string, opts: { centred?: boolean; yesFirst?: boolean } = {}): Promise<boolean> {
    return new Promise((resolve) => {
      let cursor = opts.yesFirst === false ? 1 : 0;
      // Y and N answer in every language; a translation shows them as key caps.
      const rows = [h("div.yn-row", hotkeyLabel(tr("Yes"), "Y")), h("div.yn-row", hotkeyLabel(tr("No"), "N"))];
      const paint = () => rows.forEach((r, i) => r.classList.toggle("cur", i === cursor));
      const done = (v: boolean) => {
        this.closeModal(m);
        resolve(v);
      };
      rows.forEach((r, i) => r.addEventListener("click", () => done(i === 0)));
      const m: Screen = {
        title: question,
        el: h("div.prompt", { class: opts.centred ? "centred" : "corner", role: "dialog", "aria-label": tr(question) }, h("div.prompt-q", tr(question)), ...rows),
        contexts: () => ["Dialog"],
        key: (e) => {
          const k = letterOf(e);
          if (!e.ctrlKey && !e.altKey && k === "y") done(true);
          else if (!e.ctrlKey && !e.altKey && k === "n") done(false);
          else if (e.chord === "Up" || e.chord === "Down") {
            cursor = 1 - cursor;
            paint();
          } else if (e.chord === "Enter") done(cursor === 0);
          else if (e.chord === "Esc") done(false);
          return true; // a prompt swallows every other key
        },
      };
      paint();
      this.openModal(m);
    });
  }

  /**
   * A small field dialog (F2 date, period). Enter moves to the next field and
   * accepts on the last one; Esc clears a field first, then cancels.
   */
  dialog(
    title: string,
    fields: { label: string; value: string }[],
    validate: (values: string[]) => string | null,
  ): Promise<string[] | null> {
    return new Promise((resolve) => {
      const inputs = fields.map((f) => h("input.field", { value: f.value, spellcheck: false }));
      let at = 0;
      const go = (i: number) => {
        at = i;
        inputs[i].focus();
        inputs[i].select();
      };
      const finish = (v: string[] | null) => {
        this.closeModal(m);
        resolve(v);
      };
      const accept = () => {
        const values = inputs.map((i) => i.value);
        const problem = validate(values);
        if (problem) this.warn(problem);
        else finish(values);
      };
      inputs.forEach((inp, i) => inp.addEventListener("focus", () => (at = i)));
      const m: Screen = {
        title,
        el: h(
          "div.dialog",
          { role: "dialog", "aria-label": tr(title) },
          h("div.dialog-title", tr(title)),
          ...fields.map((f, i) => h("label.dialog-row", h("span", tr(f.label)), inputs[i])),
        ),
        contexts: () => ["Dialog"],
        focus: () => go(at),
        key: (e) => {
          if (e.chord === "Esc") {
            if (inputs[at].value) {
              inputs[at].value = "";
              return true;
            }
            finish(null);
            return true;
          }
          if (e.chord === "Up" || e.chord === "Shift+Tab") {
            if (at > 0) go(at - 1);
            return true;
          }
          return false;
        },
        action: (a) => {
          if (a === "Accept") accept();
          else if (a === "NextField") {
            if (at < inputs.length - 1) go(at + 1);
            else accept();
          } else if (a === "PreviousField") {
            if (at > 0) go(at - 1);
          } else return false;
          return true;
        },
      };
      this.openModal(m);
    });
  }
}

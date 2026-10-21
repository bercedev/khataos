// Settings: voucher numbering (Alter → Voucher Type), the company's details
// for tax invoices (tax ID, state, address) and the tax rate list. The
// forms follow the master forms' field flow (F1–F7): Enter moves on, Up goes
// back, Esc asks before dropping changes, Ctrl+A saves.
import "../settings.css";
import { api, saApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import {
  KIND_LABEL,
  type CompanyTax,
  type Context,
  type Numbering,
  type Region,
  type Restart,
  type TaxIdCheck,
  type TaxRate,
  type VoucherKind,
  type ZatcaSeller,
  type ZatcaSellerInfo,
} from "../types";
import { Report } from "./reports";

/** What a form needs from a list picker (any ListPicker<T>). */
export interface FormPicker {
  readonly isOpen: boolean;
  open(): Promise<void>;
  close(): void;
  reload(): Promise<void>;
  key(e: KeyEv): boolean;
  action(a: string): boolean;
}

type Input = HTMLInputElement | HTMLTextAreaElement;

export function frow(label: string, field: HTMLElement, sub?: string): HTMLElement {
  return h("label.frow", h("span.flabel", label, sub ? h("small.fsub", sub) : null), field);
}

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function fyStart(shell: Shell): string {
  return shell.company?.fy_start ?? D.fyStart(today(shell));
}

// ------------------------------------------------------------ form base

/**
 * Field flow shared by the small forms here and the godown master. Fields
 * may come and go (`fields()`); a textarea field takes Enter as a new line
 * and moves on with Enter on a blank last line.
 */
export abstract class FieldForm<F extends string> implements Screen {
  abstract title: string;
  el = h("section.ledger");
  overlay: boolean;
  protected at = 0;
  protected inputs = {} as Record<F, Input>;
  protected pickers: Partial<Record<F, FormPicker>> = {};
  /** Values when the form was last loaded or saved ("Quit without saving?"). */
  private snapshot = "";

  constructor(
    protected shell: Shell,
    names: readonly F[],
    o: { inline?: boolean; multiline?: readonly F[] } = {},
  ) {
    this.overlay = Boolean(o.inline);
    for (const f of names) {
      const inp = o.multiline?.includes(f)
        ? h("textarea.field.multiline", { spellcheck: false, rows: 4 })
        : h("input.field", { spellcheck: false, autocomplete: "off" });
      inp.addEventListener("focus", () => this.focusField(this.fields().indexOf(f)));
      this.inputs[f] = inp;
    }
  }

  /** Fields in Enter order. */
  protected abstract fields(): F[];
  /** Enter on a field (when its list didn't take the key). */
  protected abstract next(f: F): void | Promise<void>;
  protected abstract save(): Promise<void>;
  /** Fill the form from the saved record. */
  protected async load(): Promise<void> {}
  /** The chosen value's text, put back in a list field when it loses focus. */
  protected pickedText(_f: F): string {
    return "";
  }
  /** Label of Alt+C on a list field, when it creates. */
  protected createLabel(_f: F): string | null {
    return null;
  }
  /** Whether Alt+D deletes the record shown. */
  protected canDelete(): boolean {
    return false;
  }
  protected async remove(): Promise<void> {}

  /** Lay out the rows (and an optional side panel), as a card when inline. */
  protected layout(cardTitle: string, rows: (HTMLElement | null)[], panel?: HTMLElement): void {
    this.el.append(
      h(
        "div.form.master",
        { class: this.overlay ? "card" : "" },
        this.overlay ? h("div.card-title", cardTitle) : null,
        h("div.form-main", ...rows),
        panel ?? null,
      ),
    );
  }

  async init(): Promise<void> {
    await this.load();
    this.markClean();
  }

  protected markClean(): void {
    this.snapshot = this.state();
  }

  protected state(): string {
    return (Object.keys(this.inputs) as F[])
      .map((f) => (this.pickers[f] ? this.pickedText(f) : this.inputs[f].value.trim()))
      .join("\u0000");
  }

  protected dirty(): boolean {
    return this.state() !== this.snapshot;
  }

  protected field(): F {
    const all = this.fields();
    return all[Math.min(this.at, all.length - 1)];
  }

  contexts(): Context[] {
    return this.pickers[this.field()] ? ["ListField", "Master"] : ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back" || (a === "DeleteRecord" && this.canDelete());
  }

  bottom() {
    const ctx = this.contexts();
    const create = this.createLabel(this.field());
    return [
      this.shell.btn("Accept", ctx),
      create ? this.shell.btn("CreateMasterInline", ctx, create) : null,
      this.canDelete() ? this.shell.btn("DeleteRecord", ctx, "Delete") : null,
      this.shell.btn("Back", ctx, this.overlay ? "Cancel" : "Back"),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    this.go(this.at);
  }

  resume(): void {
    void this.pickers[this.field()]?.reload();
  }

  protected focusField(i: number): void {
    const all = this.fields();
    if (i < 0 || i >= all.length) return;
    const f = all[i];
    this.at = i;
    for (const k of Object.keys(this.inputs) as F[]) {
      const p = this.pickers[k];
      if (k !== f && p?.isOpen) {
        p.close();
        this.inputs[k].value = this.pickedText(k);
      }
    }
    const p = this.pickers[f];
    if (p && !p.isOpen) void p.open();
    this.shell.refresh();
  }

  protected go(i: number): void {
    const all = this.fields();
    i = Math.max(0, Math.min(i, all.length - 1));
    const inp = this.inputs[all[i]];
    inp.focus();
    this.focusField(i);
    if (inp instanceof HTMLTextAreaElement) inp.setSelectionRange(inp.value.length, inp.value.length);
    else inp.select();
  }

  protected goTo(f: F): void {
    this.go(this.fields().indexOf(f));
  }

  key(e: KeyEv): boolean {
    const f = this.field();
    if (this.pickers[f]?.key(e)) return true;
    const inp = this.inputs[f];
    if (inp instanceof HTMLTextAreaElement) {
      // Up and Down move inside the text; Enter starts a new line.
      if (e.chord !== "Enter") return false;
      const atEnd = inp.selectionStart === inp.value.length && inp.selectionEnd === inp.value.length;
      if (atEnd && (inp.value === "" || inp.value.endsWith("\n"))) {
        inp.value = inp.value.replace(/\s+$/, "");
        void this.next(f);
        return true;
      }
      inp.setRangeText("\n", inp.selectionStart, inp.selectionEnd, "end");
      inp.dispatchEvent(new Event("input"));
      return true;
    }
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true; // Up on the first field does nothing (F3)
    }
    return false;
  }

  action(a: string): boolean {
    if (this.pickers[this.field()]?.action(a)) return true;
    switch (a) {
      case "Accept":
        void this.save();
        return true;
      case "NextField":
        void this.next(this.field());
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Back":
        void this.escape();
        return true;
      case "DeleteRecord":
        if (!this.canDelete()) return false;
        void this.remove();
        return true;
    }
    return false;
  }

  /** Enter past the last field: "Accept? Yes / No" (F5). */
  protected async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.go(0);
  }

  private async escape(): Promise<void> {
    const f = this.field();
    const inp = this.inputs[f];
    // F4: clear the field first (never a whole address, nor a read-only value).
    if (!this.pickers[f] && !inp.readOnly && !(inp instanceof HTMLTextAreaElement) && inp.value !== "") {
      inp.value = "";
      inp.dispatchEvent(new Event("input"));
      return;
    }
    if (this.dirty() && !(await this.shell.ask("Quit without saving?"))) return;
    this.shell.pop();
  }

  dispose(): void {
    for (const f of Object.keys(this.inputs) as F[]) this.pickers[f]?.close();
  }
}

// ---------------------------------------------------- voucher numbering

/** Shown through t(). */
const RESTARTS: { id: Restart; label: string; hint: string }[] = [
  { id: "Never", label: "Never", hint: "one running series" },
  { id: "Yearly", label: "Every financial year", hint: "back to 1 each year" },
  { id: "Monthly", label: "Every month", hint: "back to 1 each month" },
];

const TOKENS = ["{FY}", "{YYYY}", "{YY}", "{MM}"];
const MAX_AFFIX = 16;
const MAX_WIDTH = 9;

function two(n: number): string {
  return String(n % 100).padStart(2, "0");
}

/** Month the company's financial year starts in (4: April). */
function fyStartMonth(shell: Shell): number {
  return Number(fyStart(shell).slice(5, 7)) || 4;
}

/** "26-27" for an April–March year, "2026" for a calendar year. */
export function fyLabel(date: string, startMonth: number): string {
  const [y, m] = date.split("-").map(Number);
  const start = m >= startMonth ? y : y - 1;
  return startMonth === 1 ? String(start) : `${two(start)}-${two(start + 1)}`;
}

/** A prefix or suffix with its tokens filled in for `date`. */
export function renderTokens(text: string, date: string, startMonth: number): string {
  const [y, m] = date.split("-").map(Number);
  return text
    .split("{FY}").join(fyLabel(date, startMonth))
    .split("{YYYY}").join(String(y))
    .split("{YY}").join(two(y))
    .split("{MM}").join(String(m).padStart(2, "0"));
}

/** Why a prefix or suffix can't be used; null when fine. */
function affixProblem(what: "prefix" | "suffix", text: string): string | null {
  if ([...text].length > MAX_AFFIX)
    return what === "prefix" ? t("The prefix can be at most {n} characters.", { n: MAX_AFFIX }) : t("The suffix can be at most {n} characters.", { n: MAX_AFFIX });
  if (/[{}]/.test(TOKENS.reduce((s, tok) => s.split(tok).join(""), text)))
    return what === "prefix" ? t("Only {FY}, {YYYY}, {YY} and {MM} can go in braces in the prefix.") : t("Only {FY}, {YYYY}, {YY} and {MM} can go in braces in the suffix.");
  return null;
}

type NumField = "restart" | "prefix" | "suffix" | "width";

/**
 * How one voucher type numbers its vouchers: when the count restarts, the
 * text around the number and zero padding. The preview is worked out here
 * from the tokens; the server re-checks that saved vouchers keep unique
 * numbers under the new setting.
 */
export class VoucherTypeForm extends FieldForm<NumField> {
  title = "Voucher Type Alteration";
  private n: Numbering | null = null;
  private restart: Restart = "Never";
  private restartLine = h("div.nature");
  private preview = h("div.vt-preview");
  private warnEl = h("div.vt-warn");
  private panel = h("div.opening-panel");
  private note = h("div.note");

  constructor(shell: Shell, private kind: VoucherKind) {
    super(shell, ["restart", "prefix", "suffix", "width"]);
    const i = this.inputs;
    i.prefix.placeholder = t("e.g. {FY}/ or INV-");
    i.suffix.placeholder = t("e.g. /A");
    i.width.classList.add("amount");
    const india = shell.company?.country?.code === "IN";
    this.note.append(
      india
        ? t("Invoice numbers must be unique within a financial year (India, GST Rule 46). Including {FY} keeps bill references unique across years.")
        : t("Invoice numbers usually must be unique within a financial year. Including {FY} keeps bill references unique across years."),
    );
    this.layout(
      t("Alter Voucher Type"),
      [
        frow(t("Voucher type"), h("div.fstatic", t(KIND_LABEL[kind]))),
        h(
          "div.fsection",
          h("div.fsection-title", t("Numbering")),
          frow(t("Restart numbering"), h("div.fstack", i.restart, this.restartLine)),
          frow(t("Prefix"), i.prefix, t("before the number")),
          frow(t("Suffix"), i.suffix, t("after the number")),
          frow(t("Pad to digits"), h("div.famount.narrow", i.width), t("0 to 9; 0: no padding")),
        ),
        this.preview,
        this.warnEl,
        this.note,
      ],
      this.panel,
    );
    for (const f of ["prefix", "suffix", "width"] as const) i[f].addEventListener("input", () => this.paint());
    this.pickers.restart = new ListPicker<Restart>({
      input: i.restart as HTMLInputElement,
      host: this.el,
      title: "Restart numbering",
      source: (q) =>
        RESTARTS.filter((r) => t(r.label).toLowerCase().includes(q.toLowerCase()) || r.label.toLowerCase().includes(q.toLowerCase())).map<PickItem<Restart>>((r) => ({
          id: r.id,
          label: t(r.label),
          sub: t(r.hint),
          data: r.id,
        })),
      selectedId: () => this.restart,
      onPick: (it) => {
        this.setRestart(it.data);
        this.goTo("prefix");
      },
    });
  }

  protected fields(): NumField[] {
    return ["restart", "prefix", "suffix", "width"];
  }

  protected override async load(): Promise<void> {
    const n = (await api.listNumbering()).find((x) => x.kind === this.kind);
    if (!n) throw new Error(t("No numbering found for {kind} vouchers.", { kind: t(KIND_LABEL[this.kind]) }));
    this.n = n;
    this.title = t("Voucher Type Alteration: {kind}", { kind: t(KIND_LABEL[n.kind]) });
    this.inputs.prefix.value = n.prefix;
    this.inputs.suffix.value = n.suffix;
    this.inputs.width.value = String(n.width);
    this.setRestart(n.restart);
  }

  private setRestart(r: Restart): void {
    this.restart = r;
    const def = RESTARTS.find((x) => x.id === r)!;
    this.inputs.restart.value = t(def.label);
    this.restartLine.textContent = `(${t(def.hint)})`;
    this.paint();
  }

  protected override pickedText(): string {
    return t(RESTARTS.find((x) => x.id === this.restart)!.label);
  }

  private width(): number | null {
    const v = this.inputs.width.value.trim() || "0";
    return /^\d$/.test(v) && Number(v) <= MAX_WIDTH ? Number(v) : null;
  }

  /** The running part of the next number under the saved setting. */
  private running(date: string, month: number): string {
    const n = this.n;
    if (!n) return "1";
    const pre = renderTokens(n.prefix, date, month).toLowerCase();
    const suf = renderTokens(n.suffix, date, month).toLowerCase();
    const num = n.next_number.toLowerCase();
    if (!num.startsWith(pre) || !num.endsWith(suf)) return "1";
    const mid = num.slice(pre.length, num.length - suf.length);
    return /^\d+$/.test(mid) ? String(BigInt(mid)) : "1";
  }

  private paint(): void {
    const date = today(this.shell);
    const month = fyStartMonth(this.shell);
    const prefix = this.inputs.prefix.value.trim();
    const suffix = this.inputs.suffix.value.trim();
    const width = this.width() ?? 0;
    const problem = affixProblem("prefix", prefix) ?? affixProblem("suffix", suffix);
    const number = `${renderTokens(prefix, date, month)}${this.running(date, month).padStart(width, "0")}${renderTokens(suffix, date, month)}`;
    this.preview.replaceChildren(t("Next number: "), h("b", { dir: "ltr" }, problem ? "–" : number));
    const both = prefix + suffix;
    const hasYear = ["{FY}", "{YYYY}", "{YY}"].some((tok) => both.includes(tok));
    let warn = problem ?? "";
    if (!warn && this.restart === "Yearly" && !hasYear)
      warn = t("Numbers start again at 1 each year without the year in them, so two bills can share a number. Add {FY} to keep them apart.");
    else if (!warn && this.restart === "Monthly" && !both.includes("{MM}"))
      warn = t("Numbers start again at 1 each month without the month in them, so bills in one year can share a number. Add {MM} (and {FY}).");
    else if (!warn && this.n && this.restart !== this.n.restart)
      warn = t("The running number shown is from the current setting; it is worked out again when you save.");
    this.warnEl.textContent = warn;
    const n = this.n;
    this.panel.replaceChildren(
      h("div.panel-title", t(KIND_LABEL[this.kind])),
      h("div.prow", h("span", t("Vouchers saved")), h("span.num", String(n?.used ?? 0))),
      h("div.prow", h("span", t("Next number now")), h("span.num", n?.next_number ?? "")),
      h("div.panel-title", { style: { marginBlockStart: "10px" } }, t("Tokens, for today")),
      ...[
        ["{FY}", fyLabel(date, month), "financial year"],
        ["{YYYY}", date.slice(0, 4), "year"],
        ["{YY}", date.slice(2, 4), "year, 2 digits"],
        ["{MM}", date.slice(5, 7), "month"],
      ].map(([tok, v, what]) => h("div.prow", h("span", h("code", { dir: "ltr" }, tok), ` ${t(what)}`), h("span.num", v))),
    );
  }

  protected next(f: NumField): void {
    const i = this.inputs;
    if (f === "restart") return this.goTo("prefix");
    if (f === "prefix" || f === "suffix") {
      const problem = affixProblem(f, i[f].value.trim());
      if (problem) return this.shell.warn(problem);
      return this.goTo(f === "prefix" ? "suffix" : "width");
    }
    if (this.width() === null) return this.shell.warn(t("Pad to 0 to 9 digits."));
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const i = this.inputs;
    const prefix = i.prefix.value.trim();
    const suffix = i.suffix.value.trim();
    for (const [f, text] of [["prefix", prefix], ["suffix", suffix]] as const) {
      const problem = affixProblem(f, text);
      if (problem) {
        this.goTo(f);
        return this.shell.warn(problem);
      }
    }
    const width = this.width();
    if (width === null) {
      this.goTo("width");
      return this.shell.warn(t("Pad to 0 to 9 digits."));
    }
    try {
      const n = await api.setNumbering(this.kind, this.restart, prefix, suffix, width);
      this.n = n;
      this.markClean();
      this.shell.info(t("{kind} numbering saved. Next number: {number}.", { kind: t(KIND_LABEL[n.kind]), number: n.next_number }));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ------------------------------------------------------ company details

type CoField = "taxId" | "region" | "address" | "hsn";

/** HSN/SAC digits an Indian invoice must show (by turnover); null: not set. */
/** Shown through t(). */
const HSN_CHOICES: { digits: number | null; label: string }[] = [
  { digits: 4, label: "4 (turnover up to ₹5 crore)" },
  { digits: 6, label: "6 (above ₹5 crore)" },
  { digits: 8, label: "8" },
  { digits: null, label: "Not set" },
];

function hsnLabel(d: number | null): string {
  const c = HSN_CHOICES.find((x) => x.digits === d);
  return c ? t(c.label) : String(d);
}

/**
 * The company's own details for tax invoices: its tax ID (GSTIN…), the
 * state or region it is registered in and its mailing address. A tax ID
 * that encodes the state decides it; the state then shows read-only. An
 * Indian company also says how many HSN/SAC digits its invoices carry.
 */
export class CompanyDetailsForm extends FieldForm<CoField> {
  title = "Company Details";
  private ct: CompanyTax | null = null;
  private region: Region | null = null;
  /** The last check of the tax ID field; null once it is edited. */
  private check: TaxIdCheck | null = null;
  private taxLabel = h("span.flabel", t("Tax ID"), h("small.fsub", t("leave blank if unregistered")));
  private taxLine = h("div.nature");
  private regionLine = h("div.nature");
  private stateRow: HTMLElement;
  /** HSN/SAC digits on invoices (India only). */
  private hsn: number | null = null;
  private india: boolean;
  private note = h("div.note");

  constructor(shell: Shell) {
    super(shell, ["taxId", "region", "address", "hsn"], { multiline: ["address"] });
    this.india = shell.company?.country?.code === "IN";
    const i = this.inputs;
    this.stateRow = frow(t("State"), h("div.fstack", i.region, this.regionLine));
    i.taxId.classList.add("taxid");
    i.address.placeholder = t("Building, street\nCity, PIN");
    i.taxId.addEventListener("input", () => this.setCheck(null));
    this.layout(
      t("Company Details"),
      [
        frow(t("Company name"), h("div.fstatic", shell.company?.name ?? "")),
        h("div.fsection", h("div.fsection-title", t("Tax registration")), h("label.frow", this.taxLabel, h("div.fstack", i.taxId, this.taxLine)), this.stateRow),
        h(
          "div.fsection",
          h("div.fsection-title", t("Address")),
          frow(t("Address"), i.address, t("printed on invoices; Enter on a blank line moves on")),
        ),
        this.india
          ? h(
              "div.fsection",
              h("div.fsection-title", t("Invoices")),
              frow(t("HSN/SAC digits on invoices"), h("div.fstack", i.hsn, h("div.nature", t("By last year's turnover; e-invoices and e-way bills check codes against it.")))),
            )
          : null,
        this.note,
      ],
    );
    this.pickers.region = new ListPicker<Region>({
      input: i.region as HTMLInputElement,
      host: this.el,
      title: "States",
      columns: ["Name", "Code"],
      source: (q) => {
        const s = q.toLowerCase();
        return (this.ct?.regions ?? [])
          .filter((r) => r.name.toLowerCase().includes(s) || r.code.toLowerCase() === s)
          .map<PickItem<Region>>((r) => ({ id: r.code, label: r.name, sub: r.code, data: r }));
      },
      selectedId: () => this.region?.code ?? null,
      onPick: (it) => {
        this.setRegion(it.data);
        this.goTo("address");
      },
    });
    this.pickers.hsn = new ListPicker<number | null>({
      input: i.hsn as HTMLInputElement,
      host: this.el,
      title: "HSN/SAC digits",
      source: (q) => {
        const s = q.trim().toLowerCase();
        return HSN_CHOICES.filter((c) => !s || t(c.label).toLowerCase().includes(s) || c.label.toLowerCase().includes(s)).map<PickItem<number | null>>((c) => ({
          id: String(c.digits ?? ""),
          label: t(c.label),
          data: c.digits,
        }));
      },
      selectedId: () => String(this.hsn ?? ""),
      onPick: (it) => {
        this.hsn = it.data;
        i.hsn.value = hsnLabel(this.hsn);
        void this.confirm();
      },
    });
  }

  protected override async load(): Promise<void> {
    const ct = await api.companyTax();
    this.ct = ct;
    this.taxLabel.firstChild!.textContent = ct.label;
    this.inputs.taxId.value = ct.tax_id ?? "";
    this.inputs.address.value = ct.address;
    this.region = ct.regions.find((r) => r.code === ct.region) ?? null;
    // Countries without tax regions (Singapore) have no State field.
    this.stateRow.hidden = ct.regions.length === 0;
    this.note.textContent =
      ct.regions.length === 0
        ? t("Tax invoices show the supplier's name, address and tax ID.")
        : t("Tax invoices show the supplier's name, address and tax ID. The state decides whether a sale is within the state or to another state.");
    this.hsn = ct.hsn_digits;
    this.inputs.hsn.value = hsnLabel(this.hsn);
    if (ct.tax_id) {
      try {
        await this.checkTaxId();
      } catch {
        /* checked again on save */
      }
    }
    this.paintRegion();
  }

  /** The state comes from the tax ID (a GSTIN's first two digits). */
  private derived(): boolean {
    return Boolean(this.check && !this.check.error && this.check.region);
  }

  protected fields(): CoField[] {
    const noRegions = this.ct !== null && this.ct.regions.length === 0;
    const fields: CoField[] = this.derived() || noRegions ? ["taxId", "address"] : ["taxId", "region", "address"];
    return this.india ? [...fields, "hsn"] : fields;
  }

  private setCheck(c: TaxIdCheck | null): void {
    this.check = c;
    this.taxLine.classList.toggle("bad", Boolean(c?.error));
    this.taxLine.textContent = !c ? "" : c.error ?? [c.region, c.entity_id && `PAN ${c.entity_id}`].filter(Boolean).join(" · ");
    this.paintRegion();
  }

  private setRegion(r: Region | null): void {
    this.region = r;
    this.paintRegion();
  }

  private paintRegion(): void {
    const inp = this.inputs.region;
    if (this.derived()) {
      inp.readOnly = true;
      inp.tabIndex = -1;
      inp.classList.add("locked");
      inp.value = this.check!.region!;
      this.regionLine.textContent = t("(from the {label})", { label: this.ct?.label ?? t("tax ID") });
    } else {
      inp.readOnly = false;
      inp.tabIndex = 0;
      inp.classList.remove("locked");
      if (!this.pickers.region?.isOpen) inp.value = this.region?.name ?? "";
      this.regionLine.textContent = this.region ? t("(code {code})", { code: this.region.code }) : t("Where the business is registered");
    }
  }

  protected override pickedText(f: CoField): string {
    if (f === "hsn") return hsnLabel(this.hsn);
    if (f !== "region") return "";
    return this.derived() ? this.check!.region! : this.region?.name ?? "";
  }

  private async checkTaxId(): Promise<TaxIdCheck | null> {
    const raw = this.inputs.taxId.value.trim();
    if (!raw) {
      this.setCheck(null);
      return null;
    }
    if (this.check) return this.check;
    const c = await api.checkTaxId(raw);
    this.inputs.taxId.value = c.value;
    this.setCheck(c);
    return c;
  }

  protected async next(f: CoField): Promise<void> {
    if (f === "taxId") {
      let c: TaxIdCheck | null;
      try {
        c = await this.checkTaxId();
      } catch (e) {
        return this.shell.error(e);
      }
      if (c?.error) return this.shell.error(`${c.error}.`);
      return this.goTo(this.derived() ? "address" : "region");
    }
    if (f === "region") return this.goTo("address");
    if (f === "address" && this.india) return this.goTo("hsn");
    return this.confirm();
  }

  protected async save(): Promise<void> {
    const ct = this.ct;
    if (!ct) return;
    let c: TaxIdCheck | null;
    try {
      c = await this.checkTaxId();
    } catch (e) {
      return this.shell.error(e);
    }
    if (c?.error) {
      this.goTo("taxId");
      return this.shell.error(`${c.error}.`);
    }
    const taxId = c?.value ?? "";
    // A tax ID that encodes the state sets it on the server.
    const region = this.derived() ? "" : this.region?.code ?? "";
    const address = this.inputs.address.value.replace(/\s+$/, "");
    const taxChanged = taxId !== (ct.tax_id ?? "") || (!this.derived() && region !== (ct.region ?? ""));
    const addressChanged = address !== ct.address;
    const hsnChanged = this.india && this.hsn !== ct.hsn_digits;
    try {
      if (taxChanged) this.ct = await api.setCompanyTax(taxId, region);
      if (addressChanged) this.ct = await api.setCompanyAddress(address);
      if (hsnChanged) this.ct = await api.setHsnDigits(this.hsn);
      this.markClean();
      this.shell.info(t(taxChanged || addressChanged || hsnChanged ? "Company details saved." : "No changes to save."));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ------------------------------------------------- ZATCA seller details

type SellerField = keyof ZatcaSeller;

const SELLER_FIELDS: SellerField[] = ["crn", "building_number", "street", "additional_street", "district", "city", "postal_code", "additional_number"];

/**
 * Saudi Arabia: what ZATCA e-invoices need about the seller beyond the
 * company details: the commercial registration number (CRN) and the
 * National Address. A shared record of the company (synced) until the
 * company profile holds them. Blank street, city and postal code are taken
 * from the company address; the placeholders show what will be used.
 */
export class ZatcaSellerForm extends FieldForm<SellerField> {
  title = "Seller details (ZATCA)";
  private info: ZatcaSellerInfo | null = null;

  constructor(shell: Shell) {
    super(shell, SELLER_FIELDS);
    const i = this.inputs;
    i.building_number.classList.add("narrow");
    i.building_number.maxLength = 4;
    i.postal_code.maxLength = 5;
    i.additional_number.maxLength = 4;
    this.layout(t("Seller details (ZATCA)"), [
      frow(t("Company name"), h("div.fstatic", shell.company?.name ?? "")),
      frow(t("Commercial registration (CRN)"), i.crn, t("on every e-invoice")),
      h(
        "div.fsection",
        h("div.fsection-title", t("National Address")),
        frow(t("Building number"), i.building_number, t("4 digits")),
        frow(t("Street"), i.street),
        frow(t("Additional street"), i.additional_street, t("optional")),
        frow(t("District"), i.district),
        frow(t("City"), i.city),
        frow(t("Postal code"), i.postal_code, t("5 digits")),
        frow(t("Additional number"), i.additional_number, t("optional")),
      ),
      h(
        "div.note",
        t("Blank street, city and postal code are taken from the company address (Company details), as shown in grey. These details sync with the company's books."),
      ),
    ]);
  }

  protected fields(): SellerField[] {
    return SELLER_FIELDS;
  }

  protected override async load(): Promise<void> {
    const info = await saApi.seller();
    this.info = info;
    for (const f of SELLER_FIELDS) this.inputs[f].value = info.stored[f];
    this.paintFallbacks();
  }

  /** Placeholders: what a blank field will use, from the company address. */
  private paintFallbacks(): void {
    const e = this.info?.effective;
    for (const f of SELLER_FIELDS) {
      const v = e && !this.info!.stored[f] ? e[f] : "";
      this.inputs[f].placeholder = v ? t("{value} (from the company address)", { value: v }) : "";
    }
  }

  private problem(): { field: SellerField; text: string } | null {
    const v = (f: SellerField) => this.inputs[f].value.trim();
    if (v("building_number") && !/^\d{4}$/.test(v("building_number"))) return { field: "building_number", text: t("The building number has 4 digits.") };
    if (v("postal_code") && !/^\d{5}$/.test(v("postal_code"))) return { field: "postal_code", text: t("A Saudi postal code has 5 digits.") };
    return null;
  }

  protected next(f: SellerField): void {
    const bad = this.problem();
    if (bad && bad.field === f) return this.shell.warn(bad.text);
    const at = SELLER_FIELDS.indexOf(f);
    if (at < SELLER_FIELDS.length - 1) return this.goTo(SELLER_FIELDS[at + 1]);
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const bad = this.problem();
    if (bad) {
      this.goTo(bad.field);
      return this.shell.warn(bad.text);
    }
    const seller = Object.fromEntries(SELLER_FIELDS.map((f) => [f, this.inputs[f].value.trim()])) as unknown as ZatcaSeller;
    try {
      this.info = await saApi.setSeller(seller);
      this.markClean();
      this.shell.info(t("Seller details saved. E-invoices generated from now on carry them."));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ------------------------------------------------------------ tax rates

/** "8471 30" → "847130", as the server compares classes. */
export function normaliseClass(t: string): string {
  return t.replace(/\s+/g, "").toUpperCase();
}

/**
 * The rate in force on `on` for an item's class: the longest class in the
 * list that the code starts with and that has a rate by then (so "8471"
 * covers "84713010" unless "847130" has its own rate).
 */
export function rateFor(rates: TaxRate[], cls: string, on: string): TaxRate | null {
  const code = normaliseClass(cls);
  if (!code) return null;
  let best: TaxRate | null = null;
  for (const r of rates) {
    if (!code.startsWith(r.class) || r.from > on) continue;
    if (!best || r.class.length > best.class.length || (r.class === best.class && r.from > best.from)) best = r;
  }
  return best;
}

/** "18", "0.25". */
export function percentText(p: string): string {
  return M.toDecimal(M.of(p));
}

type RateField = "cls" | "from" | "percent";

export interface TaxRateFormOptions {
  /** Alter this rate change instead of adding one. */
  rate?: TaxRate;
  /** After a save, just before the form closes. */
  onSaved?: (r: TaxRate) => void;
}

/** Add or alter one rate change: class, the date it applies from, rate %. */
export class TaxRateForm extends FieldForm<RateField> {
  title: string;
  private keyNote = h("div.vt-warn");

  constructor(shell: Shell, private opts: TaxRateFormOptions = {}) {
    super(shell, ["cls", "from", "percent"], { inline: true });
    const r = opts.rate;
    this.title = r ? "Tax Rate Alteration" : "Tax Rate Creation";
    const i = this.inputs;
    i.cls.placeholder = t("e.g. 8471, 9983 or a category");
    i.percent.classList.add("amount");
    i.cls.value = r?.class ?? "";
    i.from.value = D.show(r?.from ?? fyStart(shell));
    i.percent.value = r ? percentText(r.percent) : "";
    i.cls.addEventListener("input", () => this.paintKey());
    i.from.addEventListener("input", () => this.paintKey());
    this.layout(t(r ? "Alter Tax Rate" : "Add Tax Rate"), [
      frow(t("HSN/SAC or category"), i.cls),
      frow(t("Applicable from"), i.from),
      frow(t("Rate"), h("div.famount.narrow", i.percent), t("percent; 0 for exempt or nil-rated")),
      this.keyNote,
    ]);
  }

  protected fields(): RateField[] {
    return ["cls", "from", "percent"];
  }

  protected override canDelete(): boolean {
    return Boolean(this.opts.rate);
  }

  private fromDate(): string | null {
    return D.parseUser(this.inputs.from.value, today(this.shell));
  }

  /** Altering: a new class or date is a separate rate change; say so. */
  private paintKey(): void {
    const r = this.opts.rate;
    const moved = r && (normaliseClass(this.inputs.cls.value) !== r.class || this.fromDate() !== r.from);
    this.keyNote.textContent = moved
      ? t("This is saved as a new rate change. The {percent}% rate for {code} from {date} stays; delete it from the list (Alt+D) if it no longer applies.", {
          percent: percentText(r.percent),
          code: r.class,
          date: D.show(r.from),
        })
      : "";
  }

  private percent(): M.Units | null {
    const v = this.inputs.percent.value.trim().replace(/%$/, "");
    if (!v) return null;
    const u = M.parse(v);
    return u !== null && u >= 0n && u <= M.of("100") ? u : null;
  }

  protected next(f: RateField): void {
    const i = this.inputs;
    if (f === "cls") {
      if (!normaliseClass(i.cls.value)) return this.shell.warn(t("Type an HSN/SAC code or a tax category."));
      i.cls.value = normaliseClass(i.cls.value);
      return this.goTo("from");
    }
    if (f === "from") {
      const d = this.fromDate();
      if (!d) return this.shell.warn(t('"{text}" is not a date.', { text: i.from.value }));
      i.from.value = D.show(d);
      return this.goTo("percent");
    }
    if (this.percent() === null) return this.shell.warn(t("Enter the rate as a percent from 0 to 100, such as 18."));
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const cls = normaliseClass(this.inputs.cls.value);
    if (!cls) {
      this.goTo("cls");
      return this.shell.warn(t("Type an HSN/SAC code or a tax category."));
    }
    const from = this.fromDate();
    if (!from) {
      this.goTo("from");
      return this.shell.warn(t('"{text}" is not a date.', { text: this.inputs.from.value }));
    }
    const p = this.percent();
    if (p === null) {
      this.goTo("percent");
      return this.shell.warn(t("Enter the rate as a percent from 0 to 100, such as 18."));
    }
    try {
      await api.setTaxRate(cls, from, M.toDecimal(p));
      this.markClean();
      const saved: TaxRate = { class: cls, from, percent: M.toDecimal(p) };
      this.shell.info(t("{code}: {percent}% from {date} saved.", { code: cls, percent: M.toDecimal(p), date: D.show(from) }));
      // Before closing, so the list reloads with its cursor on this rate.
      this.opts.onSaved?.(saved);
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected override async remove(): Promise<void> {
    const r = this.opts.rate;
    if (!r) return;
    if (await deleteRate(this.shell, r)) this.shell.pop();
  }
}

/** Delete one rate change after a Y/N prompt; true when deleted. */
async function deleteRate(shell: Shell, r: TaxRate): Promise<boolean> {
  const what = { percent: percentText(r.percent), code: r.class, date: D.show(r.from) };
  if (!(await shell.ask(t("Delete the {percent}% rate for {code} from {date}?", what), { yesFirst: false }))) return false;
  try {
    await api.setTaxRate(r.class, r.from, "");
    shell.info(t("Deleted the {percent}% rate for {code} from {date}.", what));
    return true;
  } catch (e) {
    shell.error(e);
    return false;
  }
}

/**
 * The rate list: each HSN/SAC code or category with the dates its rates
 * apply from. Enter alters a rate (or adds one on the last row), Alt+C adds,
 * Alt+D deletes. (Alt+C and Alt+D are column keys on other reports; here
 * the rows are the records.)
 */
export class TaxRatesReport extends Report {
  title = "Tax Rates";
  private rates: TaxRate[] = [];
  /** Put the cursor on this rate after the next load ("class|from"). */
  private aim: string | null = null;

  async load(): Promise<void> {
    this.rates = await api.listTaxRates();
    const now = today(this.shell);
    this.heading(t("Tax Rates"), t("Rates in force on {date} are marked", { date: D.show(now) }));
    this.resetPicks();
    // The latest change on or before today is the one in force.
    const inForce = new Map<string, string>();
    for (const r of this.rates) if (r.from <= now && (inForce.get(r.class) ?? "") < r.from) inForce.set(r.class, r.from);
    const tbody = h("tbody");
    let last = "";
    for (const r of this.rates) {
      const status = inForce.get(r.class) === r.from ? "in force" : r.from > now ? "upcoming" : "replaced";
      tbody.append(
        this.pick(
          h(
            "tr",
            h("td.strong", { class: r.class === last ? "muted" : "" }, r.class),
            h("td", D.show(r.from), h("span.status", ` (${t(status)})`)),
            h("td.num", percentText(r.percent)),
          ),
          () => this.alter(r),
        ),
      );
      last = r.class;
    }
    tbody.append(
      this.pick(
        h("tr.empty.addrow", h("td", { colSpan: 3 }, t(this.rates.length ? "Add a rate…" : "No rates yet. Press Enter or Alt+C to add the first one."))),
        () => this.add(),
      ),
    );
    if (this.aim) {
      const at = this.rates.findIndex((r) => `${r.class}|${r.from}` === this.aim);
      if (at >= 0) this.cursor = at;
      this.aim = null;
    }
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", t("HSN/SAC or category")), h("th", t("Applicable from")), h("th.num", t("Rate %")))),
        tbody,
      ),
      h(
        "div.note",
        t("An item takes the rate of the longest code in this list that its own code starts with: 8471 covers 84713010 unless 847130 has a rate of its own. A rate applies from its date until the next change for the same code."),
      ),
    );
  }

  private rateAt(): TaxRate | null {
    return this.rates[this.cursor] ?? null;
  }

  private add(): void {
    void this.shell.push(new TaxRateForm(this.shell, { onSaved: (r) => (this.aim = `${r.class}|${r.from}`) }));
  }

  private alter(r: TaxRate): void {
    void this.shell.push(new TaxRateForm(this.shell, { rate: r, onSaved: (s) => (this.aim = `${s.class}|${s.from}`) }));
  }

  override handles(a: string): boolean {
    return a === "NewColumn" || a === "DeleteColumn" || super.handles(a);
  }

  override bottom() {
    const ctx = this.contexts();
    return [
      this.shell.btn("DrillDown", ctx, "Alter"),
      this.shell.btn("NewColumn", ctx, "Add Rate"),
      this.shell.btn("DeleteColumn", ctx, "Delete"),
      this.shell.btn("PrintCurrent", ctx, "Print"),
      this.shell.btn("ExportCurrent", ctx, "Export CSV"),
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  override action(a: string): boolean {
    if (a === "NewColumn") {
      this.add();
      return true;
    }
    if (a === "DeleteColumn") {
      const r = this.rateAt();
      if (!r) this.shell.warn(t("Move to a rate to delete it."));
      else
        void deleteRate(this.shell, r).then(async (done) => {
          if (done) await this.reload();
        });
      return true;
    }
    return super.action(a);
  }
}

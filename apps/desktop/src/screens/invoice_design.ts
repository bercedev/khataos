// Company → Invoice design: template, accent colour, font, paper, logo and
// signature, column toggles, bank details, terms, footer, declaration and
// titles, with a live preview of the latest sale (or a sample). Also the
// per-voucher "Ship-to and export details" form and direct PDF export
// (Ctrl+E → PDF on a voucher; Alt+E on the Day Book for a period).
// See docs/invoice-design.md. Loaded on demand.
//
// Keys: Enter moves on (and accepts on the last field), Up/Shift+Tab go
// back, Left/Right or Space change a choice, Enter on Logo or Signature
// opens its menu, PageDown/PageUp scroll the preview, Ctrl+A saves, Esc
// leaves (asking first when changed).
import "../invoice-design.css";
import { open, save } from "@tauri-apps/plugin-dialog";
import { peppolApi } from "../api";
import * as D from "../dates";
import { h, hotkeyLabel } from "../dom";
import { hijriOn, language, ltr, t } from "../i18n";
import {
  invoiceApi,
  type InvoiceDesign,
  type InvoiceDesignInfo,
  type InvoiceImageKind,
  type PdfFile,
  type VoucherPrintDetails,
  type VoucherPrintDetailsInfo,
} from "../invoice-api";
import { letterOf } from "../keys";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, type Context, type VoucherKind } from "../types";

// ------------------------------------------------------------ small modal

interface Option<T extends string> {
  key: T;
  label: string;
  hot: string;
  hint?: string;
}

/** A short list answered with Enter or the option's letter; Esc gives null. */
function pick<T extends string>(shell: Shell, title: string, options: Option<T>[]): Promise<T | null> {
  return new Promise((resolve) => {
    let cursor = 0;
    title = t(title);
    const rows = options.map((o) => h("div.choice-row", h("span", hotkeyLabel(t(o.label), o.hot)), o.hint ? h("span.muted", t(o.hint)) : null));
    const paint = () => rows.forEach((r, i) => r.classList.toggle("cur", i === cursor));
    const done = (v: T | null) => {
      shell.closeModal(m);
      resolve(v);
    };
    rows.forEach((r, i) => r.addEventListener("click", () => done(options[i].key)));
    const m: Screen = {
      title,
      el: h("div.choice", { role: "dialog", "aria-label": title }, h("div.prompt-q", title), ...rows),
      contexts: () => ["Dialog"],
      key: (e) => {
        if (e.chord === "Up") cursor = (cursor + options.length - 1) % options.length;
        else if (e.chord === "Down") cursor = (cursor + 1) % options.length;
        else if (e.chord === "Enter") done(options[cursor].key);
        else if (e.chord === "Esc") done(null);
        else if (!e.ctrlKey && !e.altKey && e.key.length === 1) {
          const i = options.findIndex((o) => o.hot.toLowerCase() === letterOf(e));
          if (i >= 0) done(options[i].key);
        }
        paint();
        return true;
      },
    };
    paint();
    shell.openModal(m);
  });
}

// ------------------------------------------------------------ keyboard form

type FieldType = "text" | "area" | "choice" | "action";

interface Field {
  key: string;
  label: string;
  hint?: string;
  type: FieldType;
  /** Choices, labels in English (shown with t()). */
  choices?: () => { value: string; label: string }[];
  get: () => string;
  set?: (v: string) => void;
  /** Enter on an action field. */
  run?: () => void | Promise<void>;
  /** Shown (and in the Enter order) only while this holds. */
  visible?: () => boolean;
  /** A note under the field (why the law keeps it). */
  note?: () => string;
  placeholder?: string;
  dir?: "ltr" | "auto";
}

interface Built {
  f: Field;
  row: HTMLElement;
  input: HTMLInputElement | HTMLTextAreaElement | HTMLButtonElement;
  note: HTMLElement;
}

/**
 * A form of text, multi-line, choice and action fields with the master
 * forms' field flow. Subclasses give the fields, save and (optionally)
 * react to changes.
 */
abstract class KeyForm implements Screen {
  abstract title: string;
  el = h("section.ledger.idesign-screen");
  protected built: Built[] = [];
  protected at = 0;
  private snapshot = "";

  constructor(protected shell: Shell) {}

  protected abstract save(): Promise<void>;
  protected changed(): void {}

  protected build(fields: (Field | string)[]): HTMLElement {
    const main = h("div.form-main");
    for (const f of fields) {
      if (typeof f === "string") {
        main.append(h("div.fsection-title", t(f)));
        continue;
      }
      let input: Built["input"];
      if (f.type === "area") {
        input = h("textarea.field.multiline", { spellcheck: false, rows: 3, dir: f.dir ?? "auto" });
      } else if (f.type === "choice" || f.type === "action") {
        input = h("button.field.idesign-choice", { type: "button" });
      } else {
        input = h("input.field", { spellcheck: false, autocomplete: "off", dir: f.dir ?? "auto" });
      }
      if (f.placeholder && !(input instanceof HTMLButtonElement)) input.placeholder = t(f.placeholder);
      const note = h("div.nature");
      const row = h("label.frow", h("span.flabel", t(f.label), f.hint ? h("small.fsub", t(f.hint)) : null), h("div.fstack", input, note));
      const b: Built = { f, row, input, note };
      input.addEventListener("focus", () => {
        this.at = this.order().indexOf(b);
        this.shell.refresh();
      });
      if (input instanceof HTMLButtonElement) {
        input.addEventListener("click", () => {
          if (f.type === "choice") this.step(b, 1);
          else void f.run?.();
        });
      } else {
        input.addEventListener("input", () => {
          f.set?.(input.value);
          this.changed();
        });
      }
      this.built.push(b);
      main.append(row);
    }
    return main;
  }

  /** Put the values into the fields. */
  protected paint(): void {
    for (const b of this.built) {
      const shown = b.f.visible ? b.f.visible() : true;
      b.row.hidden = !shown;
      b.note.textContent = b.f.note?.() ?? "";
      if (b.input instanceof HTMLButtonElement) {
        const v = b.f.get();
        const c = b.f.choices?.().find((x) => x.value === v);
        b.input.textContent = b.f.type === "choice" ? `‹ ${c ? t(c.label) : v} ›` : v;
      } else if (document.activeElement !== b.input) {
        b.input.value = b.f.get();
      }
    }
  }

  protected order(): Built[] {
    return this.built.filter((b) => (b.f.visible ? b.f.visible() : true));
  }

  protected markClean(): void {
    this.snapshot = this.state();
  }

  protected state(): string {
    return this.built.map((b) => b.f.get()).join("\u0000");
  }

  protected dirty(): boolean {
    return this.state() !== this.snapshot;
  }

  contexts(): Context[] {
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    const ctx = this.contexts();
    return [this.shell.btn("Accept", ctx), this.shell.btn("Back", ctx)].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    this.go(this.at);
  }

  protected go(i: number): void {
    const all = this.order();
    if (!all.length) return;
    i = Math.max(0, Math.min(i, all.length - 1));
    this.at = i;
    const inp = all[i].input;
    inp.focus();
    if (inp instanceof HTMLInputElement) inp.select();
    else if (inp instanceof HTMLTextAreaElement) inp.setSelectionRange(inp.value.length, inp.value.length);
    inp.scrollIntoView({ block: "nearest" });
    this.shell.refresh();
  }

  private here(): Built | undefined {
    return this.order()[this.at];
  }

  /** Change a choice by `d` steps (wrapping). */
  protected step(b: Built, d: number): void {
    const cs = b.f.choices?.() ?? [];
    if (!cs.length || !b.f.set) return;
    const i = cs.findIndex((c) => c.value === b.f.get());
    b.f.set(cs[(i + d + cs.length) % cs.length].value);
    this.paint();
    this.changed();
  }

  key(e: KeyEv): boolean {
    const b = this.here();
    if (!b) return false;
    if (b.input instanceof HTMLTextAreaElement) {
      if (e.chord !== "Enter") return false;
      const inp = b.input;
      const atEnd = inp.selectionStart === inp.value.length && inp.selectionEnd === inp.value.length;
      if (atEnd && (inp.value === "" || inp.value.endsWith("\n"))) {
        inp.value = inp.value.replace(/\s+$/, "");
        b.f.set?.(inp.value);
        this.next();
        return true;
      }
      inp.setRangeText("\n", inp.selectionStart, inp.selectionEnd, "end");
      inp.dispatchEvent(new Event("input"));
      return true;
    }
    if (b.f.type === "choice") {
      if (e.chord === "Right" || e.chord === "Space") return this.step(b, 1), true;
      if (e.chord === "Left") return this.step(b, -1), true;
      // A letter jumps to the first choice starting with it.
      if (!e.ctrlKey && !e.altKey && e.key.length === 1 && e.key !== " ") {
        const l = e.key.toLowerCase();
        const cs = b.f.choices?.() ?? [];
        const c = cs.find((x) => t(x.label).toLowerCase().startsWith(l) || x.label.toLowerCase().startsWith(l));
        if (c && b.f.set) {
          b.f.set(c.value);
          this.paint();
          this.changed();
        }
        return true;
      }
    }
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true;
    }
    if (e.chord === "Down") {
      this.go(this.at + 1);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    switch (a) {
      case "Accept":
        void this.save();
        return true;
      case "NextField": {
        const b = this.here();
        if (b?.f.type === "action") void b.f.run?.();
        else this.next();
        return true;
      }
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Back":
        void this.escape();
        return true;
    }
    return false;
  }

  private next(): void {
    if (this.at < this.order().length - 1) this.go(this.at + 1);
    else void this.confirm();
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.go(0);
  }

  private async escape(): Promise<void> {
    const b = this.here();
    if (b && b.input instanceof HTMLInputElement && b.input.value !== "") {
      b.input.value = "";
      b.input.dispatchEvent(new Event("input"));
      return;
    }
    if (this.dirty() && !(await this.shell.ask("Quit without saving?", { yesFirst: false }))) return;
    this.shell.pop();
  }
}

// ------------------------------------------------------------ preview

/** The page size of each paper, in mm, as the print uses it. */
const PAGE: Record<string, [number, number]> = { A4: [210, 297], Letter: [215.9, 279.4], A5: [148, 210], Thermal80: [80, 200] };

/**
 * Show a khata-print document inside `host` (a shadow root, so its CSS
 * stays its own), scaled to the host's width. The document's text is
 * escaped by the backend and its images are `data:` URLs.
 */
export function showPreview(host: HTMLElement, html: string, paper: string): void {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const css = Array.from(doc.querySelectorAll("style"))
    .map((s) => s.textContent ?? "")
    .join("\n")
    .replace(/@page\s*\{[^}]*\}/g, "")
    .replace(/(^|[\s,}])body(?=[\s,{.:[])/g, "$1.pv-page");
  const [w, hgt] = PAGE[paper] ?? PAGE.A4;
  const pad = paper === "Thermal80" ? "3mm" : paper === "A5" ? "9mm 8mm" : "14mm 12mm";
  const root = host.shadowRoot ?? host.attachShadow({ mode: "open" });
  const style = document.createElement("style");
  style.textContent =
    `:host { display: block; }\n.pv-wrap { overflow: hidden; }\n` +
    `.pv-page { background: #fff; color: #111; width: ${w}mm; min-height: ${hgt}mm; padding: ${pad}; box-sizing: border-box; ` +
    `box-shadow: 0 2px 10px rgba(0,0,0,.18); transform-origin: 0 0; }\n${css}`;
  const page = document.createElement("div");
  page.className = "pv-page";
  const bodyDir = doc.body.getAttribute("dir") ?? doc.documentElement.getAttribute("dir");
  if (bodyDir) page.dir = bodyDir;
  page.lang = doc.documentElement.lang || "en";
  page.append(...Array.from(doc.body.childNodes).map((n) => document.importNode(n, true)));
  const wrap = document.createElement("div");
  wrap.className = "pv-wrap";
  wrap.append(page);
  root.replaceChildren(style, wrap);
  const fit = () => {
    const scale = Math.min(1, (host.clientWidth || 400) / page.offsetWidth);
    page.style.transform = `scale(${scale})`;
    wrap.style.height = `${page.offsetHeight * scale}px`;
  };
  requestAnimationFrame(fit);
}

// ------------------------------------------------------------ the design screen

const TOGGLE = [
  { value: "y", label: "Yes" },
  { value: "n", label: "No" },
];

const KINDS_SHOWN: VoucherKind[] = ["Sales", "CreditNote", "DebitNote", "Purchase", "Receipt", "Payment"];

/** Company → Invoice design. */
export class InvoiceDesignScreen extends KeyForm {
  title = "Invoice design";
  private info: InvoiceDesignInfo | null = null;
  private d: InvoiceDesign | null = null;
  private previewHost = h("div.idesign-preview-page");
  private previewNote = h("div.nature");
  private receipt = false;
  private timer = 0;
  private seq = 0;

  async init(): Promise<void> {
    this.info = await invoiceApi.design();
    this.d = structuredClone(this.info.design);
    await this.rasterise();
    this.layout();
    this.paint();
    this.markClean();
    void this.refreshPreview();
  }

  private design(): InvoiceDesign {
    return this.d!;
  }

  private yesNo(get: () => boolean, set: (v: boolean) => void, label: string, required?: () => boolean): Field {
    return {
      key: label,
      label,
      type: "choice",
      choices: () => TOGGLE,
      get: () => (get() ? "y" : "n"),
      set: (v) => set(v === "y"),
      note: () => (required?.() && !get() ? t("Printed anyway: the law of this company's country asks for it.") : ""),
    };
  }

  private text(label: string, get: () => string, set: (v: string) => void, extra: Partial<Field> = {}): Field {
    return { key: label, label, type: "text", get, set, ...extra };
  }

  private layout(): void {
    const info = this.info!;
    const d = () => this.design();
    const india = info.country === "IN";
    const bank = () => d().bank;
    const titleFields: Field[] = info.titles
      .filter((ti) => KINDS_SHOWN.includes(ti.kind as VoucherKind))
      .map((ti) => {
        const label = t("Title: {kind}", { kind: t(KIND_LABEL[ti.kind as VoucherKind] ?? ti.kind) });
        if (ti.rule === "fixed")
          return { key: `title.${ti.kind}`, label, type: "action", get: () => ti.title, note: () => t("Set by law: {reason}", { reason: ti.reason }), run: () => this.shell.info(ti.reason) } as Field;
        if (ti.rule === "choice")
          return {
            key: `title.${ti.kind}`,
            label,
            type: "choice",
            choices: () => ti.choices.map((c) => ({ value: c, label: c })),
            get: () => d().titles[ti.kind] ?? ti.default,
            set: (v: string) => {
              if (v === ti.default) delete d().titles[ti.kind];
              else d().titles[ti.kind] = v;
            },
            note: () => ti.reason,
          } as Field;
        return this.text(label, () => d().titles[ti.kind] ?? "", (v) => {
          if (v.trim()) d().titles[ti.kind] = v;
          else delete d().titles[ti.kind];
        }, { placeholder: ti.default, key: `title.${ti.kind}` });
      });
    const image = (which: InvoiceImageKind, label: string): Field => ({
      key: which,
      label,
      type: "action",
      hint: "PNG, JPEG or SVG",
      get: () => {
        const img = which === "logo" ? this.info!.logo : this.info!.signature;
        return img ? t("{kind}, {kb} KB — Enter to change or remove", { kind: img.kind.toUpperCase(), kb: Math.ceil(img.bytes / 1024) }) : t("None — Enter to add");
      },
      run: () => this.imageMenu(which),
    });
    const form = this.build([
      "Look",
      { key: "template", label: "Template", type: "choice", choices: () => info.templates, get: () => d().template, set: (v) => (d().template = v as InvoiceDesign["template"]) },
      this.text("Accent colour", () => d().accent, (v) => (d().accent = v), { hint: "#rrggbb", dir: "ltr", note: () => (/^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.test(d().accent.trim()) ? "" : t("Use #rrggbb, e.g. #1f4e79")) }),
      { key: "font", label: "Font", type: "choice", choices: () => info.fonts, get: () => d().font, set: (v) => (d().font = v as InvoiceDesign["font"]), note: () => t("PDF files use their own embedded font.") },
      { key: "paper", label: "Paper", type: "choice", choices: () => info.papers, get: () => d().paper, set: (v) => (d().paper = v as InvoiceDesign["paper"]) },
      this.yesNo(() => d().receipt_for_b2c, (v) => (d().receipt_for_b2c = v), "B2C invoices on 80 mm receipts"),
      image("logo", "Logo"),
      "Show on invoices",
      this.yesNo(() => d().show_hsn, (v) => (d().show_hsn = v), india ? "HSN/SAC column" : "Tax class column", () => info.required.tax_class),
      this.yesNo(() => d().show_qty_rate, (v) => (d().show_qty_rate = v), "Quantity and rate columns", () => info.required.qty_rate),
      this.yesNo(() => d().show_amount_in_words, (v) => (d().show_amount_in_words = v), "Amount in words"),
      this.yesNo(() => d().show_signatory, (v) => (d().show_signatory = v), "Authorised signatory", () => info.required.signatory),
      image("signature", "Signature image"),
      "Bank details",
      this.text("Account name", () => bank().account_name, (v) => (bank().account_name = v)),
      this.text("Bank", () => bank().bank_name, (v) => (bank().bank_name = v)),
      this.text("Branch", () => bank().branch, (v) => (bank().branch = v)),
      this.text(info.bank_labels.account_number, () => bank().account_number, (v) => (bank().account_number = v), { dir: "ltr" }),
      this.text("IBAN", () => bank().iban, (v) => (bank().iban = v), { dir: "ltr", visible: () => info.bank_labels.iban }),
      this.text(info.bank_labels.local_code ?? "Bank code", () => bank().local_code, (v) => (bank().local_code = v), { dir: "ltr", visible: () => info.bank_labels.local_code !== null }),
      this.text("SWIFT / BIC", () => bank().swift, (v) => (bank().swift = v), { dir: "ltr" }),
      "Texts",
      { key: "terms", label: "Terms and conditions", type: "area", get: () => d().terms, set: (v) => (d().terms = v), hint: "Enter on a blank line moves on" },
      { key: "declaration", label: "Declaration", type: "area", get: () => d().declaration, set: (v) => (d().declaration = v) },
      { key: "footer", label: "Footer note", type: "area", get: () => d().footer, set: (v) => (d().footer = v) },
      ...(india ? [this.text("LUT ARN", () => d().lut_arn, (v) => (d().lut_arn = v), { dir: "ltr", hint: "exports and SEZ supplies without IGST" })] : []),
      "Titles",
      ...titleFields,
      "Preview",
      {
        key: "preview",
        label: "Preview",
        type: "choice",
        choices: () => [
          { value: "invoice", label: "Latest invoice" },
          { value: "receipt", label: "B2C receipt" },
        ],
        get: () => (this.receipt ? "receipt" : "invoice"),
        set: (v) => (this.receipt = v === "receipt"),
        note: () => (this.receipt && !d().receipt_for_b2c ? t("Switch on B2C invoices on 80 mm receipts to print them on receipts.") : ""),
      },
    ]);
    if (!info.can_edit) form.prepend(h("div.note", t("Only the company's owner can change the invoice design.")));
    const panel = h("div.idesign-preview", h("div.fsection-title", t("Preview")), this.previewNote, this.previewHost);
    this.el.append(h("div.idesign", h("div.form.master", h("div.card-title", t("Invoice design")), form), panel));
  }

  /** An SVG logo needs a PNG copy for PDFs: drawn here, by the webview. */
  private async rasterise(): Promise<void> {
    for (const which of ["logo", "signature"] as const) {
      const img = which === "logo" ? this.info?.logo : this.info?.signature;
      if (!img?.needs_raster) continue;
      try {
        const png = await svgToPng(img.data_url, which === "logo" ? 600 : 400);
        this.info = await invoiceApi.setImageRaster(which, png);
      } catch {
        /* the print still shows the SVG; PDFs go without it */
      }
    }
  }

  private async imageMenu(which: InvoiceImageKind): Promise<void> {
    if (!this.info?.can_edit) return this.shell.warn(t("Only the company's owner can change the invoice design."));
    const has = which === "logo" ? this.info.logo : this.info.signature;
    const options: Option<"pick" | "remove">[] = [{ key: "pick", label: "Choose an image file", hot: "C", hint: t("PNG, JPEG or SVG, up to {kb} KB", { kb: this.info.max_image_kb }) }];
    if (has) options.push({ key: "remove", label: "Remove", hot: "R" });
    const what = await pick(this.shell, which === "logo" ? "Logo" : "Signature image", options);
    if (!what) return;
    try {
      if (what === "remove") {
        this.info = await invoiceApi.removeImage(which);
      } else {
        const path = await open({ multiple: false, title: t("Choose an image (PNG, JPEG or SVG)"), filters: [{ name: t("Images"), extensions: ["png", "jpg", "jpeg", "svg", "PNG", "JPG", "JPEG", "SVG"] }] });
        if (!path || Array.isArray(path)) return;
        this.info = await this.shell.transition(invoiceApi.importImage(which, path));
        await this.rasterise();
        this.shell.info(which === "logo" ? t("Logo saved.") : t("Signature saved."));
      }
    } catch (e) {
      this.shell.error(e);
    }
    this.paint();
    void this.refreshPreview();
  }

  /** PageDown / PageUp scroll the preview, wherever the cursor is. */
  override key(e: KeyEv): boolean {
    if (!e.ctrlKey && !e.altKey && (e.key === "PageDown" || e.key === "PageUp")) {
      const step = Math.max(80, this.previewHost.clientHeight * 0.8);
      this.previewHost.scrollBy({ top: e.key === "PageDown" ? step : -step });
      return true;
    }
    return super.key(e);
  }

  protected override changed(): void {
    this.paint();
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.refreshPreview(), 250);
  }

  private async refreshPreview(): Promise<void> {
    const seq = ++this.seq;
    try {
      const html = await invoiceApi.preview(this.design(), null, this.receipt, language());
      if (seq !== this.seq) return;
      const paper = this.receipt && this.design().receipt_for_b2c ? "Thermal80" : this.design().paper;
      showPreview(this.previewHost, html, paper);
      this.previewNote.textContent = t("Your latest sale (or a sample) with the settings above. Not saved yet.");
    } catch (e) {
      if (seq === this.seq) this.previewNote.textContent = e instanceof Error ? e.message : String(e);
    }
  }

  protected async save(): Promise<void> {
    if (!this.info?.can_edit) return this.shell.warn(t("Only the company's owner can change the invoice design."));
    try {
      this.info = await invoiceApi.setDesign(this.design());
      this.d = structuredClone(this.info.design);
      this.paint();
      this.markClean();
      this.shell.info(t("Invoice design saved. Prints and PDFs use it from now on."));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  dispose(): void {
    window.clearTimeout(this.timer);
  }
}

/** Draw an SVG (as an <img>, so nothing in it runs) onto a canvas: a PNG data URL. */
function svgToPng(dataUrl: string, width: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const ratio = img.naturalWidth && img.naturalHeight ? img.naturalHeight / img.naturalWidth : 0.5;
      const c = document.createElement("canvas");
      c.width = width;
      c.height = Math.max(1, Math.round(width * ratio));
      const g = c.getContext("2d");
      if (!g) return reject(new Error("no canvas"));
      g.drawImage(img, 0, 0, c.width, c.height);
      resolve(c.toDataURL("image/png"));
    };
    img.onerror = () => reject(new Error("the SVG can't be drawn"));
    img.src = dataUrl;
  });
}

// ------------------------------------------------------------ voucher print details

/** A saved voucher, as the export menu passes it. */
export interface PdfVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
  date: string;
}

const EXPORT_CHOICES = [
  { value: "", label: "From the party's country" },
  { value: "yes", label: "Yes, an export" },
  { value: "no", label: "No" },
];

const INDIA_TAX_CHOICES = [
  { value: "", label: "From the voucher (IGST charged or not)" },
  { value: "lut", label: "Under LUT or bond, without IGST" },
  { value: "igst", label: "With payment of IGST" },
];

/** Ship-to address and export shipping details of one voucher. */
export class VoucherPrintDetailsForm extends KeyForm {
  title = "Ship-to and export details";
  overlay = true;
  private info: VoucherPrintDetailsInfo | null = null;
  private d: VoucherPrintDetails | null = null;

  constructor(shell: Shell, private v: PdfVoucher) {
    super(shell);
  }

  async init(): Promise<void> {
    this.info = await invoiceApi.voucherDetails(this.v.id);
    this.d = { ...this.info.details };
    const d = () => this.d!;
    const exportOn = () => d().export === "yes" || (d().export === "" && this.info!.party_abroad);
    const india = this.info.country === "IN";
    const text = (key: keyof VoucherPrintDetails, label: string, extra: Partial<Field> = {}): Field => ({
      key,
      label,
      type: "text",
      get: () => d()[key],
      set: (v) => ((d() as unknown as Record<string, string>)[key] = v),
      ...extra,
    });
    const form = this.build([
      "Ship to",
      text("ship_to_name", "Name", { hint: "blank: deliver to the buyer" }),
      { key: "ship_to_address", label: "Address", type: "area", get: () => d().ship_to_address, set: (v) => (d().ship_to_address = v), hint: "Enter on a blank line moves on" },
      "Export",
      {
        key: "export",
        label: "Export",
        type: "choice",
        choices: () => EXPORT_CHOICES,
        get: () => d().export,
        set: (v) => (d().export = v as VoucherPrintDetails["export"]),
        note: () => (d().export === "" ? (this.info!.party_abroad ? t("The party is abroad: printed as an export.") : t("The party is in this country: not an export.")) : ""),
      },
      ...(india
        ? [
            {
              key: "india_tax",
              label: "IGST",
              type: "choice",
              choices: () => INDIA_TAX_CHOICES,
              get: () => d().india_tax,
              set: (v: string) => (d().india_tax = v as VoucherPrintDetails["india_tax"]),
              visible: exportOn,
              note: () => (d().india_tax === "" ? (this.info!.charges_igst ? t("IGST is charged on this voucher.") : t("No IGST on this voucher: under LUT.")) : ""),
            } as Field,
            text("lut_arn", "LUT ARN", { dir: "ltr", hint: "blank: the one in Invoice design", visible: exportOn }),
          ]
        : []),
      text("port_of_loading", "Port of loading", { visible: exportOn }),
      text("port_of_discharge", "Port of discharge", { visible: exportOn }),
      text("incoterms", "Incoterms", { hint: "e.g. FOB Nhava Sheva", visible: exportOn }),
      text("destination_country", "Country of destination", { visible: exportOn }),
      text("shipping_bill_no", india ? "Shipping bill no." : "Export declaration no.", { dir: "ltr", visible: exportOn }),
      {
        key: "shipping_bill_date",
        label: india ? "Shipping bill date" : "Export declaration date",
        type: "text",
        dir: "ltr",
        get: () => (d().shipping_bill_date ? D.editable(d().shipping_bill_date) : ""),
        set: (v) => (d().shipping_bill_date = v.trim() ? D.parseUser(v, this.v.date) ?? v : ""),
        visible: exportOn,
        placeholder: "19-10-2026",
      },
    ]);
    const label = `${t(KIND_LABEL[this.v.kind])} ${ltr(this.v.number)}`;
    this.el.append(
      h(
        "div.form.master.card",
        h("div.card-title", t("Ship-to and export details: {voucher}", { voucher: label })),
        h("div.note", t("Printed on the invoice and its PDF. The books don't change.")),
        form,
      ),
    );
    this.paint();
    this.markClean();
  }

  protected override changed(): void {
    this.paint();
  }

  protected async save(): Promise<void> {
    const d = this.d!;
    if (d.shipping_bill_date && !/^\d{4}-\d{2}-\d{2}$/.test(d.shipping_bill_date))
      return this.shell.warn(t('"{text}" is not a date. Try 19-10-2026.', { text: d.shipping_bill_date }));
    try {
      this.info = await invoiceApi.setVoucherDetails(this.v.id, d);
      this.markClean();
      this.shell.info(t("Ship-to and export details saved."));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ------------------------------------------------------------ PDF export

/** Ask where to save the PDF and write it (desktop paths and phone URIs). */
async function savePdf(shell: Shell, f: PdfFile): Promise<void> {
  const path = await save({ defaultPath: f.file_name, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (!path) return;
  // A phone's content:// or file:// URI is used as the dialog gave it.
  const file = /^[a-z]+:\/\//i.test(path) || /\.pdf$/i.test(path) ? path : `${path}.pdf`;
  await peppolApi.writeBinaryFile(file, f.pdf_base64);
  shell.info(f.count > 1 ? t("{n} vouchers saved to {file}.", { n: f.count, file: ltr(file) }) : t("PDF saved to {file}.", { file: ltr(file) }));
}

/** Ctrl+E → PDF: the voucher in the company's invoice design. */
export async function exportPdf(shell: Shell, v: PdfVoucher): Promise<void> {
  try {
    const hijri = hijriOn() ? D.hijri(v.date, "ar") : null;
    await savePdf(shell, await shell.transition(invoiceApi.voucherPdf(v.id, language(), hijri)));
  } catch (e) {
    shell.error(e);
  }
}

/** With "Show Hijri dates" on: the Hijri date of every day of the period (at most ten years), for a batch PDF. */
function hijriDates(from: string, to: string): Record<string, string> | null {
  if (!hijriOn() || !/^\d{4}-\d{2}-\d{2}$/.test(from) || !/^\d{4}-\d{2}-\d{2}$/.test(to)) return null;
  const out: Record<string, string> = {};
  for (let d = from, n = 0; d <= to && n < 3700; d = D.addDays(d, 1), n++) {
    const h = D.hijri(d, "ar");
    if (h) out[d] = h;
  }
  return out;
}

/** Every invoice and note of a period (or the vouchers given) in one PDF. */
export async function exportPdfBatch(shell: Shell, from: string, to: string, ids: string[] = []): Promise<void> {
  try {
    await savePdf(shell, await shell.transition(invoiceApi.vouchersPdf(ids, from, to, language(), hijriDates(from, to))));
  } catch (e) {
    shell.error(e);
  }
}

/** The options a voucher's export menu adds: PDF, and the print details of invoices and notes. */
export function pdfOptions(kind: VoucherKind): Option<"pdf" | "details">[] {
  const out: Option<"pdf" | "details">[] = [{ key: "pdf", label: "PDF", hot: "P", hint: "this voucher in the invoice design" }];
  if (kind === "Sales" || kind === "CreditNote" || kind === "DebitNote") out.push({ key: "details", label: "Ship-to and export details", hot: "D", hint: "printed on the invoice" });
  return out;
}

/** Run a PDF option; false when `what` isn't one. */
export function runPdfOption(shell: Shell, v: PdfVoucher, what: string): boolean {
  if (what === "pdf") {
    void exportPdf(shell, v);
    return true;
  }
  if (what === "details") {
    void shell.push(new VoucherPrintDetailsForm(shell, v));
    return true;
  }
  return false;
}

/** The export menu of a voucher with no e-invoice: PDF and details only. */
export async function exportMenu(shell: Shell, v: PdfVoucher): Promise<void> {
  const what = await pick(shell, "Export", pdfOptions(v.kind));
  if (what) runPdfOption(shell, v, what);
}

/** Alt+E on the Day Book: this period's invoices and notes, or the voucher under the cursor. */
export async function dayBookPdf(shell: Shell, from: string, to: string, current: PdfVoucher | null): Promise<void> {
  const options: Option<"period" | "one">[] = [{ key: "period", label: "PDF of this period's invoices and notes", hot: "P", hint: "one file, each from a new page" }];
  if (current) options.push({ key: "one", label: "PDF of this voucher", hot: "V" });
  const what = await pick(shell, "Export", options);
  if (what === "period") await exportPdfBatch(shell, from, to);
  else if (what === "one" && current) await exportPdf(shell, current);
}

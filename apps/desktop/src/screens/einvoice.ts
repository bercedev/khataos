// e-Invoice (INV-01) and e-way bill JSON from saved vouchers, for the
// portals' offline and bulk tools (docs/einvoice.md). Nothing is sent to an
// IRP or the e-way bill system: the JSON is saved to a file. Saudi Arabia:
// the ZATCA e-invoice XML of a saved Sales, Credit Note or Debit Note
// (docs/ksa-zatca.md); nothing is cleared or reported to ZATCA either. Problems come
// back per document and are listed on their own screen, Enter opening the
// voucher. Loaded on demand by the voucher screen (it builds on reports.ts,
// which opens vouchers).
import "../sharing.css";
import "../einvoice.css";
import { api, zatcaApi } from "../api";
import { h, hotkeyLabel } from "../dom";
import { ltr, t } from "../i18n";
import * as D from "../dates";
import { letterOf } from "../keys";
import { save } from "@tauri-apps/plugin-dialog";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, type ZatcaStampedInvoice, type EinvoiceBuyerDetails, type EinvoiceSupplyType, type EwayDetails, type EwayTransMode, type HsnDigits, type VoucherKind } from "../types";
import { Report } from "./reports";
import { savedVoucherScreen } from "./stockjournal";
import { exportMenu, pdfOptions, runPdfOption } from "./invoice_design";

/** A saved voucher to export. */
export interface ExportVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
  date: string;
}

/** Kinds with an e-invoice (Sales, Credit Note) or an e-way bill (Sales, Purchase). */
export function exportable(kind: VoucherKind): boolean {
  return kind === "Sales" || kind === "CreditNote" || kind === "Purchase";
}

// ------------------------------------------------------------ small modals

interface Option<T extends string> {
  key: T;
  label: string;
  /** The letter that picks it. */
  hot: string;
  hint?: string;
}

/** A short list answered with Enter or the option's letter; Esc gives null. Shown in the display language. */
function pick<T extends string>(shell: Shell, title: string, options: Option<T>[]): Promise<T | null> {
  return new Promise((resolve) => {
    let cursor = 0;
    // A translated label has no Latin letter to mark: hotkeyLabel shows the key as a cap.
    const label = (o: Option<T>) => h("span", hotkeyLabel(t(o.label), o.hot));
    const rows = options.map((o) => h("div.choice-row", label(o), o.hint ? h("span.muted", t(o.hint)) : null));
    title = t(title);
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

interface FormField {
  key: string;
  label: string;
  value?: string;
  placeholder?: string;
  hint?: string;
  /** A choice: Space or Right/Left step through, a letter jumps. */
  choices?: { value: string; label: string }[];
  /** Shown only while this holds for the values so far. */
  show?: (v: Record<string, string>) => boolean;
}

/**
 * A details form in a modal: Enter moves on and accepts on the last field,
 * Ctrl+A accepts from anywhere, Up goes back, Esc clears a field and then
 * cancels. Returns the values by key, or null.
 */
function detailsForm(
  shell: Shell,
  title: string,
  note: string,
  fields: FormField[],
  validate: (v: Record<string, string>) => { key: string; problem: string } | null,
): Promise<Record<string, string> | null> {
  return new Promise((resolve) => {
    const values: Record<string, string> = {};
    const inputs = new Map<string, HTMLInputElement>();
    const rows = new Map<string, HTMLElement>();
    for (const f of fields) {
      values[f.key] = f.value ?? f.choices?.[0]?.value ?? "";
      const inp = h("input.field", { spellcheck: false, autocomplete: "off", placeholder: f.placeholder ?? "", "aria-label": f.label });
      if (f.choices) {
        inp.readOnly = true;
        inp.classList.add("choice");
      }
      inp.addEventListener("input", () => (values[f.key] = inp.value));
      inp.addEventListener("focus", () => (at = visible().indexOf(f)));
      inputs.set(f.key, inp);
      rows.set(
        f.key,
        h("label.edialog-row", h("span.flabel", f.label, f.choices ? h("small.fsub", t("Space changes")) : null), h("div.fstack", inp, f.hint ? h("div.nature", f.hint) : null)),
      );
    }
    const visible = () => fields.filter((f) => !f.show || f.show(values));
    const paint = () => {
      for (const f of fields) {
        rows.get(f.key)!.hidden = Boolean(f.show && !f.show(values));
        const inp = inputs.get(f.key)!;
        // Choice labels are English (a letter jumps to them), shown through t().
        const c = f.choices?.find((x) => x.value === values[f.key]);
        inp.value = f.choices ? (c ? t(c.label) : values[f.key]) : values[f.key];
      }
    };
    let at = 0;
    const go = (i: number) => {
      const list = visible();
      at = Math.max(0, Math.min(i, list.length - 1));
      const inp = inputs.get(list[at].key)!;
      inp.focus();
      inp.select();
    };
    const finish = (v: Record<string, string> | null) => {
      shell.closeModal(m);
      resolve(v);
    };
    const accept = () => {
      const shown = new Set(visible().map((f) => f.key));
      const out: Record<string, string> = {};
      for (const f of fields) out[f.key] = shown.has(f.key) ? values[f.key].trim() : "";
      const bad = validate(out);
      if (!bad) return finish(out);
      shell.warn(bad.problem);
      const i = visible().findIndex((f) => f.key === bad.key);
      if (i >= 0) go(i);
    };
    const step = (f: FormField, dir: number) => {
      const c = f.choices!;
      const i = c.findIndex((x) => x.value === values[f.key]);
      values[f.key] = c[(i + dir + c.length) % c.length].value;
      paint();
      inputs.get(f.key)!.select();
    };
    const m: Screen = {
      title,
      el: h(
        "div.dialog.edialog",
        { role: "dialog", "aria-label": title },
        h("div.dialog-title", title),
        h("p.edialog-note", note),
        ...fields.map((f) => rows.get(f.key)!),
      ),
      contexts: () => ["Dialog"],
      focus: () => go(at),
      key: (e: KeyEv) => {
        const f = visible()[at];
        if (e.chord === "Esc") {
          if (!f.choices && values[f.key]) {
            values[f.key] = "";
            paint();
            return true;
          }
          finish(null);
          return true;
        }
        if (e.chord === "Up" || e.chord === "Shift+Tab") {
          if (at > 0) go(at - 1);
          return true;
        }
        if (f.choices) {
          if (e.chord === "Space" || e.chord === "Right" || e.chord === "Down") step(f, 1);
          else if (e.chord === "Left") step(f, -1);
          else if (!e.ctrlKey && !e.altKey && e.key.length === 1) {
            // A letter: the next choice starting with it.
            const c = f.choices;
            const i = c.findIndex((x) => x.value === values[f.key]);
            for (let n = 1; n <= c.length; n++) {
              const x = c[(i + n) % c.length];
              if (x.label.toLowerCase().startsWith(letterOf(e))) {
                values[f.key] = x.value;
                break;
              }
            }
            paint();
            inputs.get(f.key)!.select();
          } else return false;
          return true;
        }
        return false;
      },
      action: (a) => {
        if (a === "Accept") accept();
        else if (a === "NextField") {
          if (at < visible().length - 1) go(at + 1);
          else accept();
        } else if (a === "PreviousField") {
          if (at > 0) go(at - 1);
        } else return false;
        return true;
      },
    };
    paint();
    shell.openModal(m);
  });
}

// ------------------------------------------------------------ problems

interface DocProblems {
  /** "Sales 26-27/1"; empty for a problem that names no document. */
  label: string;
  problems: string[];
}

/** "Sales 1: a; b\nCredit Note 2: c" → per document; null when the error names no document. */
export function parseProblems(message: string): DocProblems[] | null {
  const docs: DocProblems[] = [];
  let named = false;
  for (const line of message.split("\n").map((l) => l.trim()).filter(Boolean)) {
    const m = /^((?:Sales|Purchase|Credit Note|Debit Note|Voucher) .+?): (.+)$/.exec(line);
    if (m) {
      named = true;
      docs.push({ label: m[1], problems: m[2].split("; ").filter(Boolean) });
    } else docs.push({ label: "", problems: [line] });
  }
  return named ? docs : null;
}

/** What stopped an export: one row per problem, grouped by document. Enter opens the document. */
class ExportProblems extends Report {
  constructor(
    shell: Shell,
    public title: string,
    private docs: DocProblems[],
    private open: (label: string) => (() => void) | undefined,
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    const n = this.docs.filter((d) => d.label).length;
    const count = this.docs.reduce((a, d) => a + d.problems.length, 0);
    const problems = count === 1 ? t("1 problem") : t("{n} problems", { n: count });
    const docs = n === 1 ? t("in 1 document") : t("in {n} documents", { n });
    this.heading(t(this.title), `${problems}${n ? ` ${docs}` : ""} · ${t("nothing was saved")}`);
    this.resetPicks();
    const tbody = h("tbody");
    for (const d of this.docs) {
      d.problems.forEach((p, i) => {
        const text = p.charAt(0).toUpperCase() + p.slice(1);
        // Problems come from Rust in English.
        tbody.append(this.pick(h("tr", h("td.strong", i === 0 ? docLabel(d.label) : ""), h("td", { dir: "auto" }, text)), d.label ? this.open(d.label) : undefined));
      });
    }
    this.bodyEl.replaceChildren(
      h("table.rtable.eproblems", h("thead", h("tr", h("th", t("Document")), h("th", t("Problem")))), tbody),
      h(
        "p.gst-note",
        t("Fix these in the voucher, the party ledger (mailing details: address, State, PIN, country), the HSN/SAC of the items or service ledgers, or Company Details, then export again. Enter opens the document."),
      ),
    );
  }
}

/** "Sales 26-27/1" (as Rust names it) with the voucher type in the display language. */
function docLabel(label: string): string {
  const kind = ["Sales", "Purchase", "Credit Note", "Debit Note", "Voucher"].find((k) => label.startsWith(`${k} `));
  return kind ? `${t(kind)} ${ltr(label.slice(kind.length + 1))}` : label;
}

/** Show an export error: per document on its own screen, else as a message. */
function showError(shell: Shell, title: string, e: unknown, open: (label: string) => (() => void) | undefined): void {
  const message = e instanceof Error ? e.message : String(e);
  const docs = parseProblems(message);
  if (!docs) return shell.error(message);
  void shell.push(new ExportProblems(shell, title, docs, open));
}

// ------------------------------------------------------------ saving

/** Ask where to save, then write the JSON (the GST returns' writer). */
async function saveJson(shell: Shell, name: string, json: string, what: string): Promise<void> {
  const path = await save({ defaultPath: name, filters: [{ name: "JSON", extensions: ["json"] }] });
  if (!path) return;
  const file = path.toLowerCase().endsWith(".json") ? path : `${path}.json`;
  await api.writeGstJson(file, json);
  shell.info(t("{what} saved to {file}. Nothing was sent: upload it with the portal's own tool.", { what: t(what), file: ltr(file) }));
}

/** A voucher number as part of a file name ("26-27/1" → "26-27-1"). */
function fileSafe(s: string): string {
  return s.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "voucher";
}

/** Asked this session when the company could not keep the answer (view-only). */
const askedDigits = new Map<string, HsnDigits>();

/**
 * The minimum HSN/SAC digits from Company Details; when not set, asked once
 * (4 or 6, by turnover) and kept there.
 */
async function hsnDigits(shell: Shell): Promise<HsnDigits | null> {
  try {
    const ct = await api.companyTax();
    // The core also takes 8 (the contract's type names the two minimums).
    if (ct.hsn_digits) return ct.hsn_digits as HsnDigits;
  } catch (e) {
    shell.error(e);
    return null;
  }
  const key = shell.company?.path ?? "";
  const known = askedDigits.get(key);
  if (known) return known;
  const c = await pick(shell, "HSN/SAC digits on invoices", [
    { key: "4", label: "4 digits", hot: "4", hint: "turnover up to ₹5 crore" },
    { key: "6", label: "6 digits", hot: "6", hint: "above ₹5 crore" },
  ]);
  if (!c) return null;
  const d: HsnDigits = c === "4" ? 4 : 6;
  try {
    await api.setHsnDigits(d);
    shell.info(t("HSN/SAC digits set to {n} in Company Details.", { n: d }));
  } catch {
    askedDigits.set(key, d);
  }
  return d;
}

// ------------------------------------------------------------ one voucher

/** Values last typed per voucher and export, so a retry after fixing something keeps them. */
const lastValues = new Map<string, Record<string, string>>();

const SUPPLY_TYPES: { value: EinvoiceSupplyType | ""; label: string }[] = [
  { value: "", label: "From the party (B2B, SEZ or export)" },
  { value: "B2B", label: "B2B" },
  { value: "SEZWP", label: "SEZ with payment of IGST" },
  { value: "SEZWOP", label: "SEZ without payment (LUT)" },
  { value: "EXPWP", label: "Export with payment of IGST" },
  { value: "EXPWOP", label: "Export without payment (LUT)" },
  { value: "DEXP", label: "Deemed export" },
];

const NO_YES = [
  { value: "", label: "No" },
  { value: "y", label: "Yes" },
];

const MODES: { value: EwayTransMode | ""; label: string }[] = [
  { value: "Road", label: "Road" },
  { value: "Rail", label: "Rail" },
  { value: "Air", label: "Air" },
  { value: "Ship", label: "Ship" },
  { value: "", label: "Later (Part B on the portal)" },
];

function dateProblem(key: string, text: string, ref: string): { key: string; problem: string } | null {
  return text && !D.parseUser(text, ref) ? { key, problem: t('"{text}" is not a date. Try 19-10-2026.', { text }) } : null;
}

/**
 * Ctrl+E / Alt+E on a saved voucher: the country's e-invoice formats (JSON,
 * XML, IRN…) where it has them, and for every voucher a PDF in the invoice
 * design and its ship-to / export details (screens/invoice_design.ts).
 */
export async function exportVoucher(shell: Shell, v: ExportVoucher): Promise<void> {
  const extra: Option<string>[] = pdfOptions(v.kind);
  const country = shell.company?.country?.code;
  const invoiceOrNote = v.kind === "Sales" || v.kind === "CreditNote";
  if (country === "SA") return exportZatca(shell, v, extra);
  if ((country === "SG" || country === "AE") && invoiceOrNote) {
    const fmt = country === "SG" ? "pint-sg" : "pint-ae";
    const what = await pick(shell, "Export", [
      { key: "xml", label: country === "SG" ? "InvoiceNow e-invoice (PINT SG)" : "UAE e-invoice (PINT AE)", hot: "X", hint: "Peppol UBL XML file" },
      { key: "send", label: "Send via Access Point", hot: "S", hint: "your Peppol Access Point / ASP" },
      { key: "setup", label: "Access Point settings", hot: "A", hint: "address and key" },
      ...extra,
    ]);
    if (!what || runPdfOption(shell, v, what)) return;
    const m = await import("./peppol");
    if (what === "setup") return void m.setUpAccessPoint(shell);
    if (what === "send") return m.sendViaAccessPoint(shell, v, fmt);
    return country === "SG" ? m.exportPintSg(shell, v) : m.exportPintAe(shell, v);
  }
  if ((country === "DE" || country === "ES") && invoiceOrNote) {
    const own = country === "DE" ? { key: "own", label: "XRechnung (UBL or CII)", hot: "X", hint: "German e-invoice XML" } : { key: "own", label: "VeriFactu record", hot: "V", hint: "hash chain and QR" };
    const what = await pick(shell, "Export", [own, { key: "facturx", label: "Factur-X / ZUGFeRD PDF", hot: "F", hint: "PDF with the e-invoice inside" }, ...extra]);
    if (!what || runPdfOption(shell, v, what)) return;
    if (what === "facturx") return void import("./peppol").then((m) => m.exportFacturX(shell, v));
    return country === "DE"
      ? void import("./vat_de").then((m) => m.exportXRechnung(shell, v))
      : void import("./vat_es").then((m) => m.exportVerifactu(shell, v));
  }
  if ((country === "FR" || country === "NL") && invoiceOrNote) {
    const what = await pick(shell, "Export", [{ key: "facturx", label: "Factur-X / ZUGFeRD PDF", hot: "F", hint: "PDF with the e-invoice inside" }, ...extra]);
    if (!what || runPdfOption(shell, v, what)) return;
    return void import("./peppol").then((m) => m.exportFacturX(shell, v));
  }
  if (country === "MX" && invoiceOrNote) {
    const what = await pick(shell, "Export", [{ key: "cfdi", label: "CFDI 4.0", hot: "C", hint: "the XML, stamped by your PAC" }, ...extra]);
    if (!what || runPdfOption(shell, v, what)) return;
    return void import("./tax_mx").then((m) => m.exportCfdi(shell, v.id, v.number, v.kind === "CreditNote"));
  }
  if (country !== "IN") return exportMenu(shell, v);
  const options: Option<string>[] = [];
  if (v.kind === "Sales" || v.kind === "CreditNote") {
    options.push({ key: "irn", label: "Generate IRN (e-invoice portal)", hot: "G", hint: "sends it to the IRP; needs IRP settings" });
    options.push({ key: "einvoice", label: "e-Invoice JSON", hot: "I", hint: "INV-01, for the IRP" });
  }
  if (v.kind === "Sales" || v.kind === "Purchase") options.push({ key: "eway", label: "e-Way Bill JSON", hot: "W", hint: "for the bulk upload" });
  const what = await pick(shell, "Export", [...options, ...extra]);
  if (!what || runPdfOption(shell, v, what)) return;
  if (what === "irn") return void import("./irp").then((m) => m.openIrn(shell, v));
  const digits = await hsnDigits(shell);
  if (!digits) return;
  if (what === "einvoice") await einvoice(shell, v, digits);
  else await ewayBill(shell, v, digits);
}

// ------------------------------------------------------------ ZATCA (KSA)

/** Saved vouchers with a ZATCA e-invoice: Sales, Credit Notes, and Debit Notes to customers. */
export const ZATCA_KINDS: VoucherKind[] = ["Sales", "CreditNote", "DebitNote"];

type ZatcaKind = "standard" | "simplified";

/** The type each voucher's e-invoice was generated as, this session: asked once. */
const zatcaKinds = new Map<string, ZatcaKind>();

/**
 * Ctrl+E / Alt+E on a saved Sales, Credit Note or Debit Note of a Saudi
 * company: the e-invoice XML. The first time it asks Standard (B2B) or
 * Simplified (B2C); that takes the device's next counter and hash, and later
 * exports return the same document. From then on the invoice prints with its
 * QR code (Ctrl+P).
 */
async function exportZatca(shell: Shell, v: ExportVoucher, extra: Option<string>[] = []): Promise<void> {
  if (!ZATCA_KINDS.includes(v.kind)) return exportMenu(shell, v);
  const what = await pick(shell, "Export", [
    { key: "submit", label: "Submit to ZATCA", hot: "S", hint: "clear (B2B) or report (B2C); needs onboarding" },
    { key: "zatca", label: "ZATCA e-invoice XML", hot: "Z", hint: "UBL 2.1 with its hash, counter and QR" },
    ...extra,
  ]);
  if (!what || runPdfOption(shell, v, what)) return;
  if (what === "submit") {
    const kindLabel = t(KIND_LABEL[v.kind]);
    const known = zatcaKinds.get(v.id);
    return void import("./zatca").then((m) =>
      shell.push(new m.ZatcaSubmitReport(shell, v.id, t("{kind} No. {number}", { kind: kindLabel, number: v.number }), known)),
    );
  }
  let kind = zatcaKinds.get(v.id) ?? null;
  if (!kind) {
    kind = await pick<ZatcaKind>(shell, "Type of e-invoice", [
      { key: "standard", label: "Standard (B2B)", hot: "B", hint: "tax invoice to a VAT-registered business" },
      { key: "simplified", label: "Simplified (B2C)", hot: "C", hint: "simplified tax invoice to a consumer" },
    ]);
    if (!kind) return;
  }
  let inv: ZatcaStampedInvoice;
  try {
    inv = await shell.transition(zatcaApi.invoiceXml(v.id, kind));
  } catch (e) {
    // Generated before as the other type: there is one document per voucher, so export that one.
    const was = /already generated as an? (standard|simplified)/.exec(e instanceof Error ? e.message : String(e))?.[1] as ZatcaKind | undefined;
    if (!was || was === kind) return shell.error(e);
    kind = was;
    try {
      inv = await shell.transition(zatcaApi.invoiceXml(v.id, kind));
    } catch (e2) {
      return shell.error(e2);
    }
  }
  zatcaKinds.set(v.id, kind);
  const label = t(kind === "standard" ? "standard tax invoice" : "simplified tax invoice");
  let path: string | null;
  try {
    path = await save({ defaultPath: inv.file_name, filters: [{ name: "XML", extensions: ["xml"] }] });
  } catch (e) {
    return shell.error(e);
  }
  if (path) {
    const file = path.toLowerCase().endsWith(".xml") ? path : `${path}.xml`;
    try {
      await api.writeGstJson(file, inv.xml);
      const note = inv.signed
        ? t("Signed. Use Submit to ZATCA to clear or report it.")
        : t("Not signed yet: onboard this computer with ZATCA first (Company → ZATCA onboarding).");
      shell.info(t("ZATCA e-invoice ({label}, counter {icv}) saved to {file}.", { label, icv: inv.icv, file: ltr(file) }) + " " + note, 8000);
    } catch (e) {
      // Keep the document within reach: the clipboard, when the file can't be written.
      const why = e instanceof Error ? e.message : String(e);
      const copied = await navigator.clipboard?.writeText(inv.xml).then(
        () => true,
        () => false,
      );
      shell.error(
        copied
          ? t("The XML could not be saved ({why}). It was copied to the clipboard instead.", { why })
          : t("The XML could not be saved: {why}.", { why }),
      );
    }
  }
  if (inv.regenerated) shell.info(t("This is the e-invoice as first generated; a later change to the voucher needs a credit or debit note."), 8000);
  else shell.info(t("The e-invoice is recorded: Ctrl+P now prints the invoice with its QR code."), 8000);
  if (inv.warnings.length) shell.warn(inv.warnings.join(" "));
}

async function einvoice(shell: Shell, v: ExportVoucher, digits: HsnDigits): Promise<void> {
  const memo = `einvoice:${v.id}`;
  const was = lastValues.get(memo) ?? {};
  const from = t("from the ledger");
  const vals = await detailsForm(
    shell,
    t("e-Invoice: {kind} No. {number}", { kind: t(KIND_LABEL[v.kind]), number: ltr(v.number) }),
    t("Blank fields come from the party ledger and Company Details; nothing typed here is saved. Ctrl+A exports."),
    [
      { key: "supply", label: t("Supply type"), choices: SUPPLY_TYPES, value: was.supply },
      { key: "reverse", label: t("Reverse charge"), choices: NO_YES, value: was.reverse },
      { key: "igst", label: t("IGST within the State"), choices: NO_YES, value: was.igst, hint: t("Only where the law says so (e.g. supplies to an SEZ).") },
      { key: "legal_name", label: t("Buyer's legal name"), value: was.legal_name, placeholder: from },
      { key: "address1", label: t("Buyer's address line 1"), value: was.address1, placeholder: from },
      { key: "address2", label: t("Address line 2"), value: was.address2 },
      { key: "location", label: t("City or town"), value: was.location, placeholder: from },
      { key: "pin", label: t("PIN"), value: was.pin, placeholder: from },
      { key: "state_code", label: t("State code"), value: was.state_code, placeholder: t("from the ledger (e.g. 27)") },
      { key: "place_of_supply", label: t("Place of supply"), value: was.place_of_supply, placeholder: t("from the GSTIN or State") },
    ],
    (x) => {
      if (x.pin && !/^[1-9]\d{5}$/.test(x.pin)) return { key: "pin", problem: t("A PIN code has six digits.") };
      for (const k of ["state_code", "place_of_supply"]) if (x[k] && !/^\d{2}$/.test(x[k])) return { key: k, problem: t("A state code has two digits, such as 27.") };
      return null;
    },
  );
  if (!vals) return;
  lastValues.set(memo, vals);
  const buyer: EinvoiceBuyerDetails = {};
  for (const k of ["legal_name", "address1", "address2", "location", "pin", "state_code", "place_of_supply"] as const) if (vals[k]) buyer[k] = vals[k];
  if (vals.supply) buyer.supply_type = vals.supply as EinvoiceSupplyType;
  if (vals.reverse) buyer.reverse_charge = true;
  if (vals.igst) buyer.igst_on_intra = true;
  let json: string;
  try {
    json = await shell.transition(api.einvoiceJson(v.id, buyer, digits));
  } catch (e) {
    return showError(shell, "e-Invoice problems", e, () => () => shell.pop());
  }
  try {
    await saveJson(shell, `einvoice-${fileSafe(v.number)}.json`, json, "e-Invoice JSON");
  } catch (e) {
    shell.error(e);
  }
}

async function ewayBill(shell: Shell, v: ExportVoucher, digits: HsnDigits): Promise<void> {
  const memo = `eway:${v.id}`;
  const was = lastValues.get(memo) ?? {};
  const inward = v.kind === "Purchase";
  const moving = (x: Record<string, string>) => x.mode !== "";
  const vals = await detailsForm(
    shell,
    t("e-Way Bill: {kind} No. {number}", { kind: t(KIND_LABEL[v.kind]), number: ltr(v.number) }),
    t("Goods lines come from the voucher; freight and services count as other value. Transport details (Part B) can also be added on the portal later. Ctrl+A exports."),
    [
      ...(inward
        ? [
            { key: "doc_no", label: t("Supplier's invoice no."), value: was.doc_no, placeholder: t("blank: {value}", { value: ltr(v.number) }) },
            { key: "doc_date", label: t("Supplier's invoice date"), value: was.doc_date, placeholder: t("blank: {value}", { value: D.show(v.date) }) },
          ]
        : []),
      { key: "distance", label: t("Distance (km)"), value: was.distance, placeholder: t("approximate, required") },
      { key: "mode", label: t("Transport mode"), choices: MODES, value: was.mode ?? "Road" },
      { key: "vehicle", label: t("Vehicle number"), value: was.vehicle, placeholder: t("e.g. MH12AB1234"), show: (x) => x.mode === "Road" },
      { key: "trans_doc_no", label: t("Transport document no."), value: was.trans_doc_no, placeholder: t("LR, RR, airway bill or bill of lading"), show: moving },
      { key: "trans_doc_date", label: t("Transport document date"), value: was.trans_doc_date, show: moving },
      { key: "transporter_id", label: t("Transporter ID"), value: was.transporter_id, placeholder: t("GSTIN or TRANSIN, 15 characters") },
      { key: "transporter_name", label: t("Transporter name"), value: was.transporter_name },
    ],
    (x) => {
      if (!/^\d{1,4}$/.test(x.distance) || Number(x.distance) < 1 || Number(x.distance) > 4000)
        return { key: "distance", problem: t("Enter the approximate distance in whole kilometres (1 to 4000).") };
      if (x.transporter_id && !/^[0-9A-Z]{15}$/i.test(x.transporter_id))
        return { key: "transporter_id", problem: t("A transporter ID (GSTIN or TRANSIN) has 15 letters and digits.") };
      return dateProblem("doc_date", x.doc_date ?? "", v.date) ?? dateProblem("trans_doc_date", x.trans_doc_date, v.date);
    },
  );
  if (!vals) return;
  lastValues.set(memo, vals);
  const iso = (s: string | undefined) => (s ? D.parseUser(s, v.date) ?? "" : "");
  const details: EwayDetails = {
    distance_km: Number(vals.distance),
    trans_mode: (vals.mode || null) as EwayTransMode | null,
    vehicle_no: vals.vehicle.replace(/[\s-]+/g, "").toUpperCase(),
    trans_doc_no: vals.trans_doc_no,
    trans_doc_date: iso(vals.trans_doc_date),
    transporter_id: vals.transporter_id.toUpperCase(),
    transporter_name: vals.transporter_name,
  };
  if (inward) {
    details.doc_no = vals.doc_no;
    details.doc_date = iso(vals.doc_date);
  }
  let json: string;
  try {
    json = await shell.transition(api.ewaybillJson(v.id, details, digits));
  } catch (e) {
    return showError(shell, "e-Way Bill problems", e, () => () => shell.pop());
  }
  try {
    await saveJson(shell, `ewaybill-${fileSafe(v.number)}.json`, json, "e-Way Bill JSON");
  } catch (e) {
    shell.error(e);
  }
}

// ------------------------------------------------------------ bulk

/** Every B2B, SEZ and export Sales and Credit Note of a period, as one file for bulk upload. */
export async function einvoiceBulk(shell: Shell, from: string, to: string): Promise<void> {
  if (shell.company?.country?.code !== "IN") return shell.warn(t("e-Invoices are for companies in India."));
  const digits = await hsnDigits(shell);
  if (!digits) return;
  let json: string;
  try {
    json = await shell.transition(api.einvoiceBulkJson(from, to, {}, digits));
  } catch (e) {
    // Problems name documents by type and number: find them in the Day Book to open them.
    const rows = await api.dayBook(from, to).catch(() => []);
    const open = (label: string) => {
      const r = rows.find((x) => `${KIND_LABEL[x.kind]} ${x.number}` === label);
      return r ? () => void shell.push(savedVoucherScreen(shell, r.kind, r.date, r.voucher)) : undefined;
    };
    return showError(shell, "e-Invoice problems", e, open);
  }
  try {
    await saveJson(shell, `einvoice-${from}-to-${to}.json`, json, "e-Invoice JSON (bulk)");
  } catch (e) {
    shell.error(e);
  }
}

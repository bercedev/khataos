// Mexico: the monthly IVA summary (khata-mx), CFDI 4.0 XML export, payment
// receipts (CFDI de tipo P with the Complemento de Pagos 2.0) and PAC
// timbrado. The CFDI is sealed with the issuer's CSD when its password is
// typed (the password is used once and never stored) and saved as .xml.
// The CFDI list shows the period's Sales, Credit Notes and Receipts with
// their folio fiscal: Enter exports the one under the cursor, Ctrl+Alt+T
// stamps it through the PAC, Ctrl+Alt+X asks the PAC to cancel it,
// Ctrl+Alt+V records the SAT's answer to a cancellation, Ctrl+Alt+P sets up
// the PAC, Ctrl+Alt+E edits the issuer details, Ctrl+Alt+K installs the CSD.
// See docs/mexico-cfdi.md.
import "../gst.css";
import { api, mxApi, mxPacApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { KeyEv, Shell } from "../shell";
import type { DayBookRow, MxActivities, MxCfdi, MxCfdiOptions, MxIvaSummary, MxPacStamp } from "../types";
import { DayBook, Report } from "./reports";

const ISSUER_KEY = "Ctrl+Alt+E";
const CSD_KEY = "Ctrl+Alt+K";
const STAMP_KEY = "Ctrl+Alt+T";
const CANCEL_KEY = "Ctrl+Alt+X";
const CONFIRM_KEY = "Ctrl+Alt+V";
const PAC_KEY = "Ctrl+Alt+P";

/** What a CFDI list row is. */
export type MxDocKind = "invoice" | "credit" | "payment";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function monthStart(iso: string): string {
  return `${iso.slice(0, 7)}-01`;
}

function num(text: string): HTMLElement {
  return h("td.num", M.fmt(text, true));
}

function head(...cols: [string, boolean][]): HTMLElement {
  return h("thead", h("tr", ...cols.map(([c, n]) => h(n ? "th.num" : "th", t(c)))));
}

function warnings(list: string[]): HTMLElement | null {
  if (list.length === 0) return null;
  return h(
    "div.gst-warnings",
    { role: "status" },
    h("div.gst-warn-title", list.length === 1 ? t("1 thing to check") : t("{n} things to check", { n: list.length })),
    h("ul", ...list.map((w) => h("li", { dir: "auto" }, w))),
  );
}

/** Make the last dialog field a password field (the shell dialog has text fields only). */
function maskLastField(): void {
  const fields = document.querySelectorAll<HTMLInputElement>("input.field");
  const last = fields[fields.length - 1];
  if (last) {
    last.type = "password";
    last.autocomplete = "off";
  }
}

/** Ask for the issuer's régimen fiscal, lugar de expedición (CP), serie and name. */
export async function editIssuer(shell: Shell): Promise<void> {
  try {
    const cur = await mxApi.issuer();
    const v = await shell.dialog(
      "CFDI issuer",
      [
        { label: "Régimen fiscal (c_RegimenFiscal, e.g. 601)", value: cur.regimen_fiscal },
        { label: "Lugar de expedición (postal code)", value: cur.lugar_expedicion },
        { label: "Serie (optional)", value: cur.serie },
        { label: "Name as on the Constancia (blank: company name)", value: cur.nombre },
      ],
      ([r, cp]) => (/^\d{3}$/.test(r.trim()) ? (/^\d{5}$/.test(cp.trim()) ? null : t("The postal code has 5 digits.")) : t("The régimen fiscal is a 3-digit code.")),
    );
    if (!v) return;
    await mxApi.setIssuer({ regimen_fiscal: v[0], lugar_expedicion: v[1], serie: v[2], nombre: v[3] });
    shell.info(t("CFDI issuer details saved."));
  } catch (e) {
    shell.error(e);
  }
}

/** Pick the CSD's .cer and .key and check the password; the files are copied to this computer's app data, never into the books. */
export async function installCsd(shell: Shell): Promise<void> {
  try {
    const cer = await open({ multiple: false, filters: [{ name: "CSD certificate", extensions: ["cer"] }] });
    if (typeof cer !== "string") return;
    const key = await open({ multiple: false, filters: [{ name: "CSD private key", extensions: ["key"] }] });
    if (typeof key !== "string") return;
    const asked = shell.dialog("CSD password", [{ label: "Private key password", value: "" }], ([p]) => (p ? null : t("Type the key's password.")));
    maskLastField();
    const v = await asked;
    if (!v) return;
    const st = await mxApi.setCsd(cer, key, v[0]);
    shell.info(t("CSD {number} installed for {rfc}. The password is not stored.", { number: st.no_certificado ?? "", rfc: st.rfc }));
  } catch (e) {
    shell.error(e);
  }
}

async function saveXml(number: string, prefix: string, xml: string): Promise<string | null> {
  const path = await save({ defaultPath: `${prefix}-${number.replace(/[^\w-]+/g, "-")}.xml`, filters: [{ name: "XML", extensions: ["xml"] }] });
  if (!path) return null;
  const file = path.toLowerCase().endsWith(".xml") ? path : `${path}.xml`;
  await api.writeGstJson(file, xml);
  return file;
}

/** "F1=UUID, F2=UUID" → { F1: UUID, F2: UUID }. */
function parseUuids(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const part of text.split(/[,;\n]+/)) {
    const [ref, uuid] = part.split("=").map((x) => x.trim());
    if (ref && uuid) out[ref] = uuid;
  }
  return out;
}

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Ask for an invoice's or credit note's CFDI details; null when cancelled. */
async function askCfdi(shell: Shell, number: string, creditNote: boolean, needPassword: boolean): Promise<MxCfdiOptions | null> {
  const fields = [
    { label: "Forma de pago (01, 03, 04, 28… ; 99 with PPD)", value: "03" },
    { label: "Método de pago (PUE or PPD)", value: "PUE" },
    { label: "Uso del CFDI (G01, G03, S01…)", value: "G03" },
    { label: "Customer's régimen fiscal (601, 612, 626…)", value: "601" },
    { label: creditNote ? "UUID of the invoice it corrects" : "UUID of the CFDI it replaces (none)", value: "" },
    { label: "Withholdings (blank: the customer's class; RET:ISR10+IVA2/3, RET:NONE…)", value: "" },
    { label: needPassword ? "CSD password" : "CSD password (blank: unsealed draft)", value: "" },
  ];
  const asked = shell.dialog(t("CFDI for {number}", { number }), fields, ([, m, , , rel, , pw]) => {
    if (!/^(PUE|PPD)$/i.test(m.trim())) return t("The método de pago is PUE or PPD.");
    if (rel.trim() && !UUID_RE.test(rel.trim())) return t("A UUID looks like 5FB2822E-396D-4725-8521-CDC4BDD20CCF.");
    if (needPassword && !pw) return t("Type the CSD password: the PAC stamps sealed CFDIs only.");
    return null;
  });
  maskLastField();
  const v = await asked;
  if (!v) return null;
  return {
    forma_pago: v[0].trim(),
    metodo_pago: v[1].trim().toUpperCase() as "PUE" | "PPD",
    uso_cfdi: v[2].trim().toUpperCase(),
    regimen_fiscal_receptor: v[3].trim(),
    related_uuid: v[4].trim(),
    retenciones: v[5].trim(),
    password: v[6],
  };
}

/** Ask for a payment receipt's details; null when cancelled. */
async function askPago(shell: Shell, number: string, needPassword: boolean): Promise<MxCfdiOptions | null> {
  const fields = [
    { label: "How it was received (FormaDePagoP: 01 efectivo, 02 cheque, 03 transferencia…)", value: "03" },
    { label: "Customer's régimen fiscal (601, 612, 626…)", value: "601" },
    { label: "Operation number (cheque, SPEI key; optional)", value: "" },
    { label: "UUID of the receipt it replaces (none)", value: "" },
    { label: "Invoices stamped elsewhere: bill=UUID, … (optional)", value: "" },
    { label: needPassword ? "CSD password" : "CSD password (blank: unsealed draft)", value: "" },
  ];
  const asked = shell.dialog(t("Payment receipt (CFDI P) for {number}", { number }), fields, ([f, , , rel, , pw]) => {
    if (!/^\d{2}$/.test(f.trim()) || f.trim() === "99") return t("The forma de pago is a 2-digit code, never 99.");
    if (rel.trim() && !UUID_RE.test(rel.trim())) return t("A UUID looks like 5FB2822E-396D-4725-8521-CDC4BDD20CCF.");
    if (needPassword && !pw) return t("Type the CSD password: the PAC stamps sealed CFDIs only.");
    return null;
  });
  maskLastField();
  const v = await asked;
  if (!v) return null;
  return {
    forma_pago: v[0].trim(),
    metodo_pago: "",
    uso_cfdi: "CP01",
    regimen_fiscal_receptor: v[1].trim(),
    num_operacion: v[2].trim(),
    related_uuid: v[3].trim(),
    uuids: parseUuids(v[4]),
    password: v[5],
  };
}

function notes(cfdi: MxCfdi, what: string): string {
  const note = t("It is not stamped: a PAC must certify it before it is a valid CFDI.");
  return cfdi.warnings.length > 0 ? `${what} ${note} ${cfdi.warnings.join(" · ")}` : `${what} ${note}`;
}

/** Ask for the CFDI's payment and receptor details and the CSD password, then save the XML. */
export async function exportCfdi(shell: Shell, voucherId: string, number: string, creditNote: boolean): Promise<void> {
  try {
    const options = await askCfdi(shell, number, creditNote, false);
    if (!options) return;
    const cfdi = await mxApi.cfdiXml(voucherId, options);
    const file = await saveXml(number, "CFDI", cfdi.xml);
    if (!file) return;
    const what = cfdi.sealed ? t("Sealed CFDI saved to {file}.", { file: ltr(file) }) : t("Unsealed CFDI draft saved to {file}.", { file: ltr(file) });
    shell.info(notes(cfdi, what));
  } catch (e) {
    shell.error(e);
  }
}

/** The payment receipt (CFDI P) of a Receipt voucher that settles PPD invoices bill by bill: save the XML. */
export async function exportPago(shell: Shell, voucherId: string, number: string): Promise<void> {
  try {
    const options = await askPago(shell, number, false);
    if (!options) return;
    const cfdi = await mxPacApi.pagoXml(voucherId, options);
    const file = await saveXml(number, "REP", cfdi.xml);
    if (!file) return;
    const what = cfdi.sealed ? t("Sealed payment receipt saved to {file}.", { file: ltr(file) }) : t("Unsealed payment receipt draft saved to {file}.", { file: ltr(file) });
    shell.info(notes(cfdi, what));
  } catch (e) {
    shell.error(e);
  }
}

function stampSummary(s: MxPacStamp): string {
  const head = s.test ? t("MOCK STAMP — NOT VALID.") : "";
  const status = s.status === "vigente" ? t("in force") : s.status === "cancelado" ? t("cancelled") : t("cancellation requested");
  return [head, t("Folio fiscal {uuid}, stamped {date} ({status}).", { uuid: s.uuid, date: s.fecha_timbrado, status }), ...s.messages].filter((x) => x).join(" ");
}

/** Seal the voucher's CFDI with the CSD and have the PAC stamp it; offer to save the stamped XML. */
export async function stampWithPac(shell: Shell, voucherId: string, number: string, kind: MxDocKind): Promise<void> {
  try {
    const existing = await mxPacApi.record(voucherId);
    if (existing && existing.status !== "cancelado") {
      shell.info(stampSummary(existing));
      return;
    }
    const options = kind === "payment" ? await askPago(shell, number, true) : await askCfdi(shell, number, kind === "credit", true);
    if (!options) return;
    shell.info(t("Sending {number} to the PAC…", { number }));
    const stamped = await mxPacApi.stamp(voucherId, options);
    const file = await saveXml(number, kind === "payment" ? "REP" : "CFDI", stamped.xml);
    shell.info(file ? `${stampSummary(stamped)} ${t("Saved to {file}.", { file: ltr(file) })}` : stampSummary(stamped));
  } catch (e) {
    shell.error(e);
  }
}

/** Ask the PAC to cancel the voucher's CFDI (motivo 01–04). */
export async function cancelWithPac(shell: Shell, voucherId: string, number: string): Promise<void> {
  try {
    const rec = await mxPacApi.record(voucherId);
    if (!rec) {
      shell.warn(t("{number} has no stamped CFDI.", { number }));
      return;
    }
    const asked = shell.dialog(
      t("Cancel CFDI {uuid}", { uuid: rec.uuid }),
      [
        { label: "Motivo (01 errors, replaced · 02 errors · 03 not carried out · 04 in a factura global)", value: "02" },
        { label: "UUID of the replacement CFDI (motivo 01 only)", value: "" },
        { label: "CSD password (blank: the PAC holds your CSD)", value: "" },
      ],
      ([m, f]) => {
        if (!/^0?[1-4]$/.test(m.trim())) return t("The motivo is 01, 02, 03 or 04.");
        if (/^0?1$/.test(m.trim()) && !UUID_RE.test(f.trim())) return t("Motivo 01 needs the UUID of the CFDI that replaces this one; issue it first.");
        return null;
      },
    );
    maskLastField();
    const v = await asked;
    if (!v) return;
    const motivo = v[0].trim().padStart(2, "0");
    const r = await mxPacApi.cancel(voucherId, motivo, motivo === "01" ? v[1].trim() : "", v[2]);
    shell.info(stampSummary(r));
  } catch (e) {
    shell.error(e);
  }
}

/** Record what the SAT shows after a cancellation request. */
export async function confirmCancellation(shell: Shell, voucherId: string, number: string): Promise<void> {
  try {
    const v = await shell.dialog(t("SAT status of {number}", { number }), [{ label: "Cancelled at the SAT? (Y = cancelled, N = still in force)", value: "Y" }], ([a]) =>
      /^[YN]$/i.test(a.trim()) ? null : t("Type Y or N."),
    );
    if (!v) return;
    shell.info(stampSummary(await mxPacApi.confirmCancellation(voucherId, v[0].trim().toUpperCase() === "Y")));
  } catch (e) {
    shell.error(e);
  }
}

/** The PAC connection: provider, environment, user and password or account token (kept on this computer only). */
export async function editPacSettings(shell: Shell): Promise<void> {
  try {
    const cur = await mxPacApi.settings();
    const asked = shell.dialog(
      t("PAC for {rfc}", { rfc: cur.rfc }),
      [
        { label: `Provider (${cur.providers.map((p) => p.id).join(" or ")})`, value: cur.provider },
        { label: "Environment (test, production or custom)", value: cur.environment },
        { label: "Base URL (custom only)", value: cur.environment === "custom" ? cur.base_url : "" },
        { label: "User", value: cur.user },
        { label: "Account token (optional; blank keeps the stored one)", value: "" },
        { label: cur.has_password ? "Password (blank keeps the stored one)" : "Password", value: "" },
      ],
      ([p, e]) => {
        const prov = cur.providers.find((x) => x.id === p.trim());
        if (!prov) return t("Choose the PAC: {list}.", { list: cur.providers.map((x) => x.id).join(", ") });
        if (!prov.environments.some(([id]) => id === e.trim())) return t("{name} has the environments {list}.", { name: prov.name, list: prov.environments.map(([id]) => id).join(", ") });
        return null;
      },
    );
    maskLastField();
    const v = await asked;
    if (!v) return;
    const saved = await mxPacApi.setSettings({ provider: v[0].trim(), environment: v[1].trim(), base_url: v[2].trim(), user: v[3].trim(), token: v[4].trim(), password: v[5], clear_token: false });
    shell.info(t("PAC settings saved to {file} (readable by your user only). {env}", { file: ltr(saved.stored_in), env: saved.production ? t("Production: stamps are real CFDIs.") : t("Not production: stamps have no fiscal validity.") }));
  } catch (e) {
    shell.error(e);
  }
}

function activities(label: string, a: MxActivities): HTMLElement[] {
  const row = (what: string, v: string) => h("tr", h("td", t(label), h("span.muted", ` · ${t(what)}`)), num(v));
  return [row("taxed at 16%", a.taxed_16), row("taxed at 8% (border region)", a.taxed_8), row("taxed at another rate", a.taxed_other), row("0%", a.zero_rated), row("exempt", a.exempt)];
}

export class MxIvaSummaryReport extends Report {
  title = "IVA Summary";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render(await mxApi.ivaSummary(this.from, this.to));
  }

  private render(r: MxIvaSummary): void {
    const span = t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("IVA Summary"), `${r.rfc ? t("RFC {rfc}", { rfc: r.rfc }) : t("No RFC")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    const pick = (el: HTMLElement) => this.pick(el, dayBook);
    const acts = h("tbody", ...activities("Sales", r.sales).map(pick), ...activities("Purchases", r.purchases).map(pick));
    const iva = h(
      "tbody",
      pick(h("tr", h("td", t("IVA trasladado (on sales)")), num(r.iva_trasladado))),
      pick(h("tr", h("td", t("IVA retenido por clientes (withheld by customers)")), num(r.iva_retenido_por_clientes))),
      pick(h("tr", h("td", t("IVA acreditable (on purchases)")), num(r.iva_acreditable))),
      pick(h("tr", h("td", t("IVA a cargo (to pay)")), num(r.iva_a_cargo))),
      pick(h("tr", h("td", t("Saldo a favor")), h("td.num", { class: M.of(r.saldo_a_favor) > 0n ? "gst-credit" : "" }, M.fmt(r.saldo_a_favor, true)))),
    );
    const withheld = h(
      "tbody",
      pick(h("tr", h("td", t("IVA retenido a enterar (withheld from suppliers)")), num(r.iva_retenido_a_enterar))),
      pick(h("tr", h("td", t("ISR retenido a enterar (withheld from suppliers)")), num(r.isr_retenido_a_enterar))),
      pick(h("tr", h("td", t("ISR retenido por clientes (for the ISR provisional payment)")), num(r.isr_retenido_por_clientes))),
    );
    const side = (title: string, table: HTMLElement) => h("div.tside", h("div.tside-title", t(title)), table);
    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "div.gst-3b",
        side("Actos o actividades (MXN, before IVA)", h("table.rtable", head(["", false], ["$", true]), acts)),
        side("IVA", h("table.rtable", head(["", false], ["$", true]), iva)),
        side("Retenciones", h("table.rtable", head(["", false], ["$", true]), withheld)),
      ),
      h(
        "p.gst-note",
        t(
          "Worked out on document dates for checking before the monthly declaration. IVA is due when payments are actually collected or made, so PPD invoices paid in a later month belong to that month. Withholdings come from the RET-IVA and RET-ISR ledgers booked on the vouchers (or the party's RET: class); IVA withheld by customers comes off the IVA to pay, and what you withheld from suppliers is paid over separately by the 17th. Earlier balances and proportional crediting are not worked out. Enter opens the Day Book.",
        ),
      ),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }
}

function kindOf(r: DayBookRow): MxDocKind | null {
  if (r.kind === "Sales") return "invoice";
  if (r.kind === "CreditNote") return "credit";
  if (r.kind === "Receipt") return "payment";
  return null;
}

/** The period's Sales, Credit Notes and Receipts with their folio fiscal; Enter exports a CFDI. */
export class MxCfdiList extends Report {
  title = "CFDI";
  protected override period = "range" as const;
  private rows: DayBookRow[] = [];

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.rows = (await api.dayBook(this.from, this.to)).filter((r) => kindOf(r) !== null && !r.cancelled);
    const stamps = await Promise.all(this.rows.map((r) => mxPacApi.record(r.voucher).catch(() => null)));
    this.render(stamps);
  }

  private render(stamps: (MxPacStamp | null)[]): void {
    const span = t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("CFDI 4.0"), span);
    this.resetPicks();
    const label = { invoice: t("Ingreso"), credit: t("Egreso"), payment: t("Pago") };
    const status = (s: MxPacStamp | null) =>
      !s ? t("not stamped") : `${s.uuid.slice(0, 8)}… ${s.test ? t("(mock)") : ""} ${s.status === "vigente" ? "" : s.status === "cancelado" ? t("cancelled") : t("cancellation requested")}`;
    const body = h(
      "tbody",
      ...this.rows.map((r, i) => {
        const kind = kindOf(r) as MxDocKind;
        return this.pick(
          h(
            "tr",
            h("td", D.show(r.date)),
            h("td", label[kind]),
            h("td", r.number),
            h("td", r.particulars),
            num(M.of(r.debit) !== 0n ? r.debit : r.credit),
            h("td", { dir: "ltr" }, status(stamps[i])),
          ),
          () => void (kind === "payment" ? exportPago(this.shell, r.voucher, r.number) : exportCfdi(this.shell, r.voucher, r.number, kind === "credit")),
        );
      }),
    );
    this.bodyEl.replaceChildren(
      h("table.rtable", head(["Date", false], ["Type", false], ["Folio", false], ["Party", false], ["Amount", true], ["Folio fiscal", false]), body),
      h(
        "p.gst-note",
        t(
          "Enter saves the CFDI 4.0 XML of the voucher under the cursor (a Receipt that settles PPD invoices bill by bill gives the payment receipt, CFDI P). {stamp} seals it and has your PAC stamp it; {cancel} asks the PAC to cancel it; {confirm} records the SAT's answer. {pac} sets up the PAC, {issuer} the issuer details, {csd} installs the CSD (.cer and .key), kept on this computer only. Withholdings come from the customer's tax class (RET:ISR10+IVA2/3, RET:RESICO, RET:IVA4).",
          { stamp: STAMP_KEY, cancel: CANCEL_KEY, confirm: CONFIRM_KEY, pac: PAC_KEY, issuer: ISSUER_KEY, csd: CSD_KEY },
        ),
      ),
    );
  }

  private current(): DayBookRow | undefined {
    return this.rows[this.cursor];
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }

  override bottom() {
    const b = super.bottom();
    b.splice(
      b.length - 1,
      0,
      { chord: STAMP_KEY, label: "Stamp (PAC)", enabled: this.rows.length > 0 },
      { chord: CANCEL_KEY, label: "Cancel CFDI", enabled: this.rows.length > 0 },
      { chord: CONFIRM_KEY, label: "SAT status", enabled: this.rows.length > 0 },
      { chord: PAC_KEY, label: "PAC", enabled: true },
      { chord: ISSUER_KEY, label: "Issuer", enabled: true },
      { chord: CSD_KEY, label: "Install CSD", enabled: true },
    );
    return b;
  }

  override key(e: KeyEv): boolean {
    const row = this.current();
    const after = (p: Promise<void>) => void p.then(() => this.reload());
    if (e.chord === STAMP_KEY && row) {
      after(stampWithPac(this.shell, row.voucher, row.number, kindOf(row) as MxDocKind));
      return true;
    }
    if (e.chord === CANCEL_KEY && row) {
      after(cancelWithPac(this.shell, row.voucher, row.number));
      return true;
    }
    if (e.chord === CONFIRM_KEY && row) {
      after(confirmCancellation(this.shell, row.voucher, row.number));
      return true;
    }
    if (e.chord === PAC_KEY) {
      void editPacSettings(this.shell);
      return true;
    }
    if (e.chord === ISSUER_KEY) {
      void editIssuer(this.shell);
      return true;
    }
    if (e.chord === CSD_KEY) {
      void installCsd(this.shell);
      return true;
    }
    return super.key(e);
  }
}

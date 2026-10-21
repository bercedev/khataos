// India e-invoicing through an IRP (docs/einvoice.md): the connection
// settings and a voucher's IRN screen (generate, fetch, cancel within 24
// hours, e-way bill by IRN). Credentials stay in the app data folder on this
// computer, never in the books; the IRP's answer is kept in this company
// file on this computer and printed on the invoice.
//
// Opened from the voucher's export (Ctrl+E / Alt+E on a saved Sales or
// Credit Note → Generate IRN → openIrn, in einvoice.ts) and from Go To and
// the Report Directory ("e-Invoice portal (IRP) settings" → openIrpSettings,
// in launcher.ts).
import { open } from "@tauri-apps/plugin-dialog";
import { api, irpApi } from "../api";
import { h } from "../dom";
import { ltr, t } from "../i18n";
import { letterOf } from "../keys";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context, EwayTransMode, IrnResult, IrpCancelReason, IrpEwbDetails, IrpSettings, IrpSettingsInput, VoucherKind } from "../types";

// ------------------------------------------------------------ small modals

interface Choice<T extends string> {
  key: T;
  label: string;
  /** The letter that picks it. */
  hot: string;
}

/** A short list: Up/Down and Enter, or the option's letter; Esc gives null. */
function choose<T extends string>(shell: Shell, title: string, options: Choice<T>[]): Promise<T | null> {
  return new Promise((resolve) => {
    let cursor = 0;
    title = t(title);
    const rows = options.map((o) => h("div.choice-row", h("span", `${o.hot.toUpperCase()}  ${t(o.label)}`)));
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

interface Field {
  key: string;
  label: string;
  value?: string;
  placeholder?: string;
}

/** Text fields in a modal: Enter moves on and accepts on the last, Up goes back, Esc cancels. */
function form(shell: Shell, title: string, note: string, fields: Field[]): Promise<Record<string, string> | null> {
  return new Promise((resolve) => {
    title = t(title);
    const inputs = fields.map((f) => {
      const inp = h("input.field", { spellcheck: false, autocomplete: "off", placeholder: f.placeholder ? t(f.placeholder) : "", "aria-label": t(f.label) });
      inp.value = f.value ?? "";
      return inp;
    });
    let at = 0;
    const go = (i: number) => {
      at = Math.max(0, Math.min(i, inputs.length - 1));
      inputs[at].focus();
      inputs[at].select();
    };
    inputs.forEach((inp, i) => inp.addEventListener("focus", () => (at = i)));
    const finish = (v: Record<string, string> | null) => {
      shell.closeModal(m);
      resolve(v);
    };
    const accept = () => finish(Object.fromEntries(fields.map((f, i) => [f.key, inputs[i].value.trim()])));
    const m: Screen = {
      title,
      el: h(
        "div.dialog.edialog",
        { role: "dialog", "aria-label": title },
        h("div.dialog-title", title),
        note ? h("p.edialog-note", t(note)) : null,
        ...fields.map((f, i) => h("label.edialog-row", h("span.flabel", t(f.label)), inputs[i])),
      ),
      contexts: () => ["Dialog"],
      focus: () => go(at),
      key: (e: KeyEv) => {
        if (e.chord === "Esc") {
          finish(null);
          return true;
        }
        if (e.chord === "Up" || e.chord === "Shift+Tab") {
          go(at - 1);
          return true;
        }
        return false;
      },
      action: (a) => {
        if (a === "Accept") accept();
        else if (a === "NextField") {
          if (at < inputs.length - 1) go(at + 1);
          else accept();
        } else if (a === "PreviousField") go(at - 1);
        else return false;
        return true;
      },
    };
    shell.openModal(m);
  });
}

// ------------------------------------------------------------ settings

/** Which IRP, the API credentials and the IRP public key. Ctrl+O on a key field loads a .pem/.cer file. */
export class IrpSettingsScreen implements Screen {
  title = "IRP Settings";
  el = h("section.ledger");
  private s: IrpSettings | null = null;
  private endpoint = "nic-sandbox";
  private inputs: Record<string, HTMLInputElement> = {};
  private order = ["endpoint", "base_url", "client_id", "client_secret", "user_name", "password", "public_key_pem", "verify_cert_pem"];
  private rows: Record<string, HTMLElement> = {};
  private at = 0;
  private note = h("p.note");
  private clearCert = false;

  constructor(private shell: Shell) {
    const field = (key: string, attrs: Record<string, unknown> = {}) => {
      const inp = h("input.field", { spellcheck: false, autocomplete: "off", ...attrs });
      inp.addEventListener("focus", () => (this.at = this.visible().indexOf(key)));
      this.inputs[key] = inp;
      return inp;
    };
    field("endpoint", { readOnly: true, class: "field choice" });
    field("base_url", { placeholder: "https://" });
    field("client_id");
    field("client_secret", { type: "password" });
    field("user_name");
    field("password", { type: "password" });
    field("public_key_pem", { placeholder: t("Paste the IRP public key, or Ctrl+O to load the .pem file") });
    field("verify_cert_pem", { placeholder: t("Optional: NIC's certificate for checking signed QR codes (Ctrl+O)") });
    const labels: Record<string, string> = {
      endpoint: "IRP",
      base_url: "API base URL",
      client_id: "Client id",
      client_secret: "Client secret",
      user_name: "API user name",
      password: "API password",
      public_key_pem: "IRP public key",
      verify_cert_pem: "Verification certificate",
    };
    for (const k of this.order) {
      this.rows[k] = h("label.frow", h("span.flabel", t(labels[k]), k === "endpoint" ? h("small.fsub", t("Space changes")) : null), this.inputs[k]);
    }
    this.el.append(h("div.form.master", h("div.form-main", ...this.order.map((k) => this.rows[k])), this.note));
  }

  private visible(): string[] {
    return this.order.filter((k) => k !== "base_url" || this.endpoint === "custom");
  }

  private paint(): void {
    const ep = this.s?.endpoints.find((e) => e.id === this.endpoint);
    this.inputs.endpoint.value = ep ? t(ep.label) : this.endpoint;
    this.rows.base_url.hidden = this.endpoint !== "custom";
  }

  async init(): Promise<void> {
    const s = await irpApi.settings();
    this.s = s;
    this.endpoint = s.endpoint || "nic-sandbox";
    this.inputs.base_url.value = s.endpoint === "custom" ? s.base_url : "";
    this.inputs.client_id.value = s.client_id;
    this.inputs.user_name.value = s.user_name;
    const saved = t("Saved. Leave blank to keep it");
    this.inputs.client_secret.placeholder = s.has_client_secret ? saved : "";
    this.inputs.password.placeholder = s.has_password ? saved : "";
    if (s.public_key) this.inputs.public_key_pem.placeholder = `${ltr(s.public_key)} · ${saved}`;
    if (s.verify_cert) this.inputs.verify_cert_pem.placeholder = `${ltr(s.verify_cert)} · ${t("Saved. Type - to remove it")}`;
    this.note.textContent = t(
      "For GSTIN {gstin}. These details are kept in {file}, readable only by you on this computer; never in the company file, sync or backups. The sandbox and production portals have different keys and credentials.",
      { gstin: ltr(s.gstin), file: ltr(s.stored_in) },
    );
    this.paint();
  }

  contexts(): Context[] {
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    return [this.shell.btn("Accept", this.contexts(), "Save"), this.shell.btn("Back", this.contexts())].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    const k = this.visible()[Math.max(0, this.at)] ?? "endpoint";
    this.inputs[k].focus();
    this.inputs[k].select();
  }

  private go(i: number): void {
    this.at = Math.max(0, Math.min(i, this.visible().length - 1));
    this.focus();
  }

  private step(dir: number): void {
    const ids = this.s?.endpoints.map((e) => e.id) ?? ["nic-sandbox"];
    const i = ids.indexOf(this.endpoint);
    this.endpoint = ids[(i + dir + ids.length) % ids.length];
    this.paint();
  }

  private async load(key: string): Promise<void> {
    try {
      const picked = await open({ multiple: false, title: t("Choose the key or certificate file"), filters: [{ name: "PEM / CER", extensions: ["pem", "cer", "crt", "txt", "key"] }] });
      if (typeof picked !== "string") return;
      this.inputs[key].value = (await api.readTextFile(picked)).trim();
      this.shell.info(t("Loaded {file}.", { file: ltr(picked) }));
    } catch (e) {
      this.shell.error(e);
    }
  }

  key(e: KeyEv): boolean {
    const k = this.visible()[this.at];
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true;
    }
    if (k === "endpoint" && (e.chord === "Space" || e.chord === "Right" || e.chord === "Down")) {
      this.step(1);
      return true;
    }
    if (k === "endpoint" && e.chord === "Left") {
      this.step(-1);
      return true;
    }
    if ((k === "public_key_pem" || k === "verify_cert_pem") && e.chord === "Ctrl+O") {
      void this.load(k);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    switch (a) {
      case "NextField":
        if (this.at < this.visible().length - 1) this.go(this.at + 1);
        else void this.confirm();
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Accept":
        void this.save();
        return true;
      case "Back": {
        const inp = this.inputs[this.visible()[this.at]];
        if (inp && !inp.readOnly && inp.value) inp.value = "";
        else this.shell.pop();
        return true;
      }
    }
    return false;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask(t("Save IRP settings?"))) await this.save();
    else this.go(0);
  }

  private async save(): Promise<void> {
    const v = (k: string) => this.inputs[k].value;
    const cert = v("verify_cert_pem").trim();
    this.clearCert = cert === "-";
    const input: IrpSettingsInput = {
      endpoint: this.endpoint,
      base_url: v("base_url").trim(),
      client_id: v("client_id").trim(),
      client_secret: v("client_secret"),
      user_name: v("user_name").trim(),
      password: v("password"),
      public_key_pem: v("public_key_pem"),
      verify_cert_pem: this.clearCert ? "" : cert,
      clear_verify_cert: this.clearCert,
    };
    try {
      const s = await irpApi.setSettings(input);
      const missing = [!s.client_id && "client id", !s.has_client_secret && "client secret", !s.user_name && "API user name", !s.has_password && "API password", !s.public_key && "IRP public key"].filter(
        (x): x is string => Boolean(x),
      );
      if (missing.length) this.shell.warn(t("Saved. Still needed before generating IRNs: {list}.", { list: missing.map((m) => t(m)).join(", ") }));
      else this.shell.info(s.production ? t("IRP settings saved (production).") : t("IRP settings saved (sandbox)."));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

export async function openIrpSettings(shell: Shell): Promise<void> {
  await shell.push(new IrpSettingsScreen(shell));
}

// ------------------------------------------------------------ the IRN screen

/** A saved voucher to register. */
export interface IrnVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
}

const STATUS: Record<string, string> = { ACT: "Active", CNL: "Cancelled", REJ: "Rejected" };
const SIGNATURE: Record<string, string> = {
  verified: "Signature verified with NIC's certificate",
  invalid: "Signature does NOT match the verification certificate",
  not_checked: "Signature not checked (no verification certificate in IRP settings)",
};

/**
 * One voucher's IRN: G generate, D fetch details, C cancel (24 hours), E
 * e-way bill by IRN (invoices), S IRP settings, Esc back.
 */
export class IrnScreen implements Screen {
  title = "e-Invoice (IRN)";
  el = h("section.report");
  private body = h("div.rbody");
  private r: IrnResult | null = null;
  private busy = false;

  constructor(
    private shell: Shell,
    private v: IrnVoucher,
  ) {
    this.el.append(this.body);
  }

  async init(): Promise<void> {
    this.r = await irpApi.record(this.v.id);
    this.paint();
  }

  contexts(): Context[] {
    return ["Report"];
  }

  handles(a: string): boolean {
    return a === "Back";
  }

  bottom() {
    return [this.shell.btn("Back", this.contexts())].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  action(a: string): boolean {
    if (a === "Back") {
      this.shell.pop();
      return true;
    }
    return false;
  }

  key(e: KeyEv): boolean {
    if (e.ctrlKey || e.altKey || e.key.length !== 1 || this.busy) return false;
    switch (letterOf(e)) {
      case "g":
        void this.run(() => irpApi.generate(this.v.id));
        return true;
      case "d":
        void this.run(() => irpApi.details(this.v.id));
        return true;
      case "c":
        void this.cancel();
        return true;
      case "e":
        void this.ewaybill();
        return true;
      case "s":
        void openIrpSettings(this.shell);
        return true;
    }
    return false;
  }

  resume(): void {
    this.paint();
  }

  private async run(f: () => Promise<IrnResult>): Promise<void> {
    this.busy = true;
    this.shell.info(t("Contacting the IRP…"));
    try {
      this.r = await this.shell.transition(f());
      this.paint();
      for (const m of this.r.messages) this.shell.info(m);
    } catch (e) {
      this.shell.error(e);
    } finally {
      this.busy = false;
    }
  }

  private async cancel(): Promise<void> {
    const r = this.r;
    if (!r || r.status !== "ACT") return this.shell.warn(t("There is no active IRN to cancel."));
    if (!r.cancel_until) return this.shell.warn(t("The 24-hour cancellation period is over. Issue a credit note instead."));
    const reason = await choose<IrpCancelReason>(this.shell, "Why cancel the IRN?", [
      { key: "1", label: "Duplicate", hot: "1" },
      { key: "2", label: "Data entry mistake", hot: "2" },
      { key: "3", label: "Order cancelled", hot: "3" },
      { key: "4", label: "Other", hot: "4" },
    ]);
    if (!reason) return;
    const f = await form(this.shell, "Cancel IRN", "The whole document is cancelled and its number can never get an IRN again.", [{ key: "remark", label: "Remark", placeholder: "Up to 100 characters" }]);
    if (!f) return;
    if (!(await this.shell.ask(t("Cancel IRN for {no}? This can't be undone.", { no: ltr(this.v.number) }), { yesFirst: false }))) return;
    await this.run(() => irpApi.cancel(this.v.id, reason, f.remark));
  }

  private async ewaybill(): Promise<void> {
    const r = this.r;
    if (!r || r.status !== "ACT") return this.shell.warn(t("Generate the IRN first."));
    if (this.v.kind !== "Sales") return this.shell.warn(t("e-Way bills by IRN are for invoices."));
    const mode = await choose<EwayTransMode | "none">(this.shell, "Mode of transport", [
      { key: "Road", label: "Road", hot: "r" },
      { key: "Rail", label: "Rail", hot: "l" },
      { key: "Air", label: "Air", hot: "a" },
      { key: "Ship", label: "Ship", hot: "s" },
      { key: "none", label: "None (Part A only, with a transporter id)", hot: "n" },
    ]);
    if (!mode) return;
    const fields: Field[] = [{ key: "distance", label: "Distance (km)", placeholder: "0: worked out from the PIN codes", value: "0" }];
    if (mode === "Road" || mode === "Ship") fields.push({ key: "vehicle", label: "Vehicle number" });
    if (mode !== "Road" && mode !== "none") fields.push({ key: "doc_no", label: "Transport document number" }, { key: "doc_date", label: "Transport document date", placeholder: "dd/mm/yyyy" });
    fields.push({ key: "tid", label: "Transporter id (GSTIN)" }, { key: "tname", label: "Transporter name" });
    const f = await form(this.shell, "e-Way Bill by IRN", "Part A comes from the IRN.", fields);
    if (!f) return;
    const distance = Number(f.distance || "0");
    if (!Number.isInteger(distance) || distance < 0) return this.shell.warn(t("The distance is a whole number of km."));
    const details: IrpEwbDetails = {
      distance_km: distance,
      trans_mode: mode === "none" ? null : mode,
      transporter_id: f.tid ?? "",
      transporter_name: f.tname ?? "",
      trans_doc_no: f.doc_no ?? "",
      trans_doc_date: f.doc_date ?? "",
      vehicle_no: f.vehicle ?? "",
      over_dimensional: false,
    };
    await this.run(() => irpApi.ewaybill(this.v.id, details));
  }

  private paint(): void {
    const r = this.r;
    const row = (label: string, value: string | null | undefined) => (value ? h("tr", h("th", t(label)), h("td", { dir: "ltr" }, value)) : null);
    const head = h("div.rhead", h("h2", `${t("e-Invoice (IRN)")} · ${ltr(this.v.number)}`));
    if (!r) {
      this.body.replaceChildren(
        head,
        h("p", t("This voucher has no IRN yet.")),
        h("p.gst-note", t("G generates the IRN with the IRP in IRP settings (S). D looks the document up on the IRP if it was registered elsewhere. Esc goes back.")),
      );
      return;
    }
    const qr = h("div.irn-qr");
    if (r.qr_svg) qr.innerHTML = r.qr_svg; // SVG built in Rust from the IRP's signed QR (no user markup).
    const table = h(
      "table.rtable",
      h(
        "tbody",
        row("Status", t(STATUS[r.status] ?? r.status)),
        row("IRN", r.irn),
        row("Ack No", r.ack_no),
        row("Ack date", r.ack_dt),
        row("Document", `${r.doc_type} ${r.doc_no} · ${r.doc_date}`),
        row("Cancellable until", r.cancel_until),
        row("Cancelled on", r.cancel_date),
        row("e-Way bill", r.ewb_no ? `${r.ewb_no}${r.ewb_valid_till ? ` · ${t("valid till")} ${r.ewb_valid_till}` : ""}` : null),
        row("Remarks", r.remarks),
        row("Seller GSTIN (QR)", r.qr?.seller_gstin),
        row("Buyer GSTIN (QR)", r.qr?.buyer_gstin),
        row("Invoice value (QR)", r.qr?.total_invoice_value),
      ),
    );
    const notes = [...r.messages, ...r.notes];
    const keys = [
      r.status === "ACT" ? null : "G generate",
      "D fetch details",
      r.status === "ACT" && r.cancel_until ? "C cancel" : null,
      r.status === "ACT" && !r.ewb_no && this.v.kind === "Sales" ? "E e-way bill" : null,
      "S IRP settings",
    ].filter((x): x is string => Boolean(x));
    this.body.replaceChildren(
      head,
      h("div.irn-result", { style: "display:flex;gap:16px;align-items:flex-start" }, table, qr),
      h("p.note", t(SIGNATURE[r.signature] ?? r.signature)),
      ...notes.map((n) => h("p.gst-note", { dir: "auto" }, n)),
      h("p.gst-note", keys.map((k) => t(k)).join(" · ")),
    );
  }
}

/** Open the IRN screen for a saved Sales or Credit Note voucher. */
export async function openIrn(shell: Shell, v: IrnVoucher): Promise<void> {
  if (v.kind !== "Sales" && v.kind !== "CreditNote") return shell.warn(t("IRNs are generated for Sales and Credit Note vouchers."));
  await shell.push(new IrnScreen(shell, v));
}

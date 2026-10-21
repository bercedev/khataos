// United Kingdom: Making Tax Digital for VAT through HMRC's API.
//
// - MtdSettingsScreen: the user's own HMRC developer application (client ID
//   and secret, sandbox or production, redirect port). Kept in the app data
//   folder, never in the books.
// - MtdObligationsReport: HMRC's open and fulfilled VAT periods. Enter on an
//   open period opens the 9-box return for HMRC's dates (MtdReturnReport,
//   which is UkVatReturnReport with a Submit key); on a fulfilled one it shows
//   the return HMRC holds and, if KhataOS sent it, the receipt.
// - Submitting asks for HMRC's declaration with an explicit tick box; the
//   return is sent with "finalised" true only then.
// - MtdAccountReport (liabilities and payments) and MtdHeaderCheckReport
//   (HMRC's Test Fraud Prevention Headers API, sandbox).
//
// Every HMRC call sends the fraud-prevention headers; the screens and window
// come from clientInfo() at the time of the call.
import "../gst.css";
import { availableMonitors } from "@tauri-apps/api/window";
import { mtdApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context, MtdClientInfo, MtdEnvironment, MtdHeaderCheck, MtdObligation, MtdReceipt, MtdSettings, MtdViewedReturn } from "../types";
import { Report } from "./reports";
import { UkVatReturnReport } from "./vat_gb";

/** HMRC's wording for a business (VAT (MTD) end-to-end service guide, Obligations). */
const DECLARATION =
  "When you submit this VAT information you are making a legal declaration that the information is true and complete. A false declaration can result in prosecution.";

const SETTINGS_KEY = "Ctrl+Alt+M";
const CONNECT_KEY = "Ctrl+Alt+C";
const HEADERS_KEY = "Ctrl+Alt+H";
const ACCOUNT_KEY = "Ctrl+Alt+L";
const SUBMIT_KEY = "Ctrl+Alt+T";

/**
 * Screens and window for the fraud-prevention headers. Every monitor Tauri
 * reports, in logical pixels (physical ÷ scale, as a browser's
 * window.screen reports them) with its scaling factor; the colour depth is
 * the webview's (screen.colorDepth). Falls back to window.screen.
 */
export async function clientInfo(): Promise<MtdClientInfo> {
  const depth = window.screen.colorDepth || 24;
  let screens: MtdClientInfo["screens"] = [];
  try {
    screens = (await availableMonitors()).map((m) => ({
      width: Math.max(1, Math.round(m.size.width / (m.scaleFactor || 1))),
      height: Math.max(1, Math.round(m.size.height / (m.scaleFactor || 1))),
      scaling_factor: m.scaleFactor || 1,
      colour_depth: depth,
    }));
  } catch {
    /* no window API: the current screen only */
  }
  if (!screens.length) {
    screens = [
      {
        width: Math.max(1, Math.round(window.screen.width)),
        height: Math.max(1, Math.round(window.screen.height)),
        scaling_factor: window.devicePixelRatio || 1,
        colour_depth: depth,
      },
    ];
  }
  return { screens, window: { width: Math.max(1, Math.round(window.innerWidth)), height: Math.max(1, Math.round(window.innerHeight)) } };
}

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function span(from: string, to: string): string {
  return t("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function envLabel(e: MtdEnvironment): string {
  return e === "production" ? t("HMRC (live)") : t("HMRC sandbox (test)");
}

function status(s: MtdSettings): string {
  const vrn = s.vrn ? t("VRN {vrn}", { vrn: ltr(s.vrn) }) : t("No valid VAT Reg No");
  const conn = s.connected ? t("connected") : t("not connected");
  return `${envLabel(s.environment)} · ${vrn} · ${conn}`;
}

/** Sign in to HMRC in the browser; resolves when access is granted. */
async function connect(shell: Shell): Promise<MtdSettings | null> {
  shell.info(t("Sign in to HMRC in your web browser and grant KhataOS access. Waiting up to 5 minutes…"), 15000);
  try {
    const s = await shell.transition(mtdApi.connect());
    shell.info(t("Connected to HMRC."));
    return s;
  } catch (e) {
    shell.error(e);
    return null;
  }
}

// ------------------------------------------------------------- settings

/** The user's own HMRC application: KhataOS is open source and can't ship a client secret. */
export class MtdSettingsScreen implements Screen {
  title = "MTD Settings";
  el = h("section.ledger");
  private inputs: HTMLInputElement[];
  private at = 0;
  private hasSecret = false;
  private note = h("p.note");

  constructor(private shell: Shell) {
    const field = (attrs: Record<string, unknown> = {}) => h("input.field", { spellcheck: false, autocomplete: "off", ...attrs });
    this.inputs = [
      field({ placeholder: "sandbox / production" }),
      field({ placeholder: t("From the HMRC Developer Hub") }),
      field({ type: "password" }),
      field({ inputmode: "numeric" }),
    ];
    this.inputs.forEach((inp, i) => inp.addEventListener("focus", () => (this.at = i)));
    this.el.append(
      h(
        "div.form.master",
        h(
          "div.form-main",
          h("label.frow", h("span.flabel", t("Environment")), this.inputs[0]),
          h("label.frow", h("span.flabel", t("Client ID")), this.inputs[1]),
          h("label.frow", h("span.flabel", t("Client secret")), this.inputs[2]),
          h("label.frow", h("span.flabel", t("Redirect port")), this.inputs[3]),
        ),
        this.note,
      ),
    );
  }

  async init(): Promise<void> {
    const s = await mtdApi.settings();
    this.inputs[0].value = s.environment;
    this.inputs[1].value = s.client_id;
    this.inputs[3].value = String(s.redirect_port);
    this.hasSecret = s.has_secret;
    this.inputs[2].placeholder = s.has_secret ? t("Saved. Leave blank to keep it") : t("From the HMRC Developer Hub");
    this.note.textContent = t(
      "Register your own application on the HMRC Developer Hub (developer.service.hmrc.gov.uk), subscribe it to the VAT (MTD) API and add the redirect URI {uri}. These settings are kept on this computer, outside the books. {status}.",
      { uri: ltr(s.redirect_uri), status: status(s) },
    );
  }

  contexts(): Context[] {
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    return [this.shell.btn("Accept", this.contexts(), "Save"), this.shell.btn("Back", this.contexts())].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }

  focus(): void {
    this.inputs[this.at].focus();
    this.inputs[this.at].select();
  }

  private go(i: number): void {
    this.at = i;
    this.focus();
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    switch (a) {
      case "NextField":
        if (this.at < this.inputs.length - 1) this.go(this.at + 1);
        else void this.confirm();
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Accept":
        void this.save();
        return true;
      case "Back":
        if (this.inputs[this.at].value) this.inputs[this.at].value = "";
        else this.shell.pop();
        return true;
    }
    return false;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask(t("Save MTD settings?"))) await this.save();
    else this.go(0);
  }

  private async save(): Promise<void> {
    const env = this.inputs[0].value.trim().toLowerCase();
    const environment: MtdEnvironment | null = env.startsWith("s") ? "sandbox" : env.startsWith("p") ? "production" : null;
    if (!environment) {
      this.go(0);
      return this.shell.warn(t("Type sandbox (HMRC's test service) or production."));
    }
    if (!this.inputs[1].value.trim()) {
      this.go(1);
      return this.shell.warn(t("Enter your HMRC application's client ID."));
    }
    if (!this.hasSecret && !this.inputs[2].value.trim()) {
      this.go(2);
      return this.shell.warn(t("Enter your HMRC application's client secret."));
    }
    const port = Number(this.inputs[3].value.trim() || "0");
    if (!Number.isInteger(port) || port < 0 || port > 65535) {
      this.go(3);
      return this.shell.warn(t("The redirect port is a number from 1024 to 65535."));
    }
    try {
      const s = await mtdApi.setSettings(environment, this.inputs[1].value, this.inputs[2].value, port);
      this.shell.info(t("MTD settings saved. Register {uri} as a redirect URI of your HMRC application.", { uri: ltr(s.redirect_uri) }), 8000);
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ---------------------------------------------------------- obligations

/** HMRC's VAT periods for the company: open ones and those fulfilled in the period. */
export class MtdObligationsReport extends Report {
  title = "MTD VAT Obligations";
  protected override period = "range" as const;
  private settings: MtdSettings | null = null;
  private rows: MtdObligation[] = [];

  constructor(shell: Shell, private from = D.addDays(today(shell), -365), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.settings = await mtdApi.settings();
    if (!this.settings.connected) {
      this.rows = [];
      return this.render();
    }
    const info = await clientInfo();
    const open = await mtdApi.obligations("", "", "O", info);
    const done = await mtdApi.obligations(this.from, this.to, "F", info);
    const seen = new Set(open.map((o) => o.period_key));
    this.rows = [...open, ...done.filter((o) => !seen.has(o.period_key))].sort((a, b) => b.start.localeCompare(a.start));
    this.render();
  }

  private render(): void {
    const s = this.settings!;
    this.heading(t("MTD VAT Obligations"), `${status(s)} · ${t("fulfilled {span}", { span: span(this.from, this.to) })}`);
    this.resetPicks();
    if (!s.connected) {
      this.bodyEl.replaceChildren(
        h(
          "p.gst-note",
          s.has_secret
            ? t("Not connected to HMRC. {key} opens HMRC's sign-in in your browser.", { key: CONNECT_KEY })
            : t("Set up your HMRC application first: {key} opens MTD settings.", { key: SETTINGS_KEY }),
        ),
      );
      return;
    }
    const body = h(
      "tbody",
      ...this.rows.map((o) =>
        this.pick(
          h(
            "tr",
            h("td", ltr(o.period_key)),
            h("td", span(o.start, o.end)),
            h("td", D.show(o.due)),
            h("td", o.status === "O" ? t("Open") : t("Fulfilled")),
            h("td", o.receipt ? t("Sent by KhataOS, form bundle {n}", { n: ltr(o.receipt.form_bundle_number) }) : o.received ? t("Received {date}", { date: D.show(o.received) }) : ""),
          ),
          () => void this.open(o),
        ),
      ),
    );
    if (!this.rows.length) body.append(h("tr.empty", h("td", { colSpan: 5 }, t("HMRC has no obligations for this period."))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", ...["Period key", "Period", "Due", "Status", "Submission"].map((c) => h("th", t(c))))),
        body,
      ),
      h(
        "p.gst-note",
        t("Enter on an open period opens its VAT return from the books, to check and submit. Enter on a fulfilled one shows the return HMRC holds. {account}: liabilities and payments. {headers}: test the fraud-prevention headers (sandbox).", {
          account: ACCOUNT_KEY,
          headers: HEADERS_KEY,
        }),
      ),
    );
  }

  private async open(o: MtdObligation): Promise<void> {
    if (o.status === "O") {
      await this.shell.push(new MtdReturnReport(this.shell, o, this.settings!));
      return;
    }
    await this.shell.push(new MtdViewedReturnReport(this.shell, o));
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    if (D.addDays(r[0], 365) < r[1]) return this.shell.warn(t("HMRC gives obligations for at most 366 days at a time."));
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }

  override bottom() {
    const b = super.bottom();
    const connected = this.settings?.connected ?? false;
    b.splice(
      b.length - 1,
      0,
      { chord: CONNECT_KEY, label: connected ? "Reconnect" : "Connect", enabled: this.settings?.has_secret ?? false },
      { chord: SETTINGS_KEY, label: "MTD Settings", enabled: true },
      { chord: ACCOUNT_KEY, label: "Liabilities", enabled: connected },
      { chord: HEADERS_KEY, label: "Test Headers", enabled: this.settings?.environment === "sandbox" && (this.settings?.has_secret ?? false) },
    );
    return b;
  }

  override key(e: KeyEv): boolean {
    switch (e.chord) {
      case CONNECT_KEY:
        void connect(this.shell).then((s) => s && this.reload());
        return true;
      case SETTINGS_KEY:
        void this.shell.push(new MtdSettingsScreen(this.shell));
        return true;
      case ACCOUNT_KEY:
        void this.shell.push(new MtdAccountReport(this.shell, D.addDays(this.to, -364), this.to));
        return true;
      case HEADERS_KEY:
        void this.shell.push(new MtdHeaderCheckReport(this.shell));
        return true;
    }
    return super.key(e);
  }
}

// --------------------------------------------------- return and submit

/** The 9-box return from the books for an open obligation, with Submit. */
export class MtdReturnReport extends UkVatReturnReport {
  override title = "MTD VAT Return";
  private sent: MtdReceipt | null;

  constructor(shell: Shell, private ob: MtdObligation, private settings: MtdSettings) {
    super(shell, ob.start, ob.end);
    this.sent = ob.receipt;
  }

  override async load(): Promise<void> {
    await super.load();
    const env = this.settings.environment === "sandbox" ? t(" Sandbox: nothing is filed for real.") : "";
    const line = this.sent
      ? t("Period {key} was submitted from this computer: form bundle {n}, processed {date}.", {
          key: ltr(this.ob.period_key),
          n: ltr(this.sent.form_bundle_number),
          date: ltr(this.sent.processing_date),
        })
      : t("HMRC period {key}, due {due}. {submit} submits this return to {env}.", {
          key: ltr(this.ob.period_key),
          due: D.show(this.ob.due),
          submit: SUBMIT_KEY,
          env: envLabel(this.settings.environment),
        }) + env;
    this.bodyEl.prepend(h("p.gst-note", { role: "status" }, line));
  }

  protected override async askPeriod(): Promise<void> {
    this.shell.warn(t("The period is HMRC's obligation ({span}); it can't be changed here.", { span: span(this.ob.start, this.ob.end) }));
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: SUBMIT_KEY, label: "Submit to HMRC", enabled: !this.sent });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === SUBMIT_KEY) {
      if (this.sent) this.shell.warn(t("This return has already been submitted."));
      else void this.submit();
      return true;
    }
    return super.key(e);
  }

  private async submit(): Promise<void> {
    const ok = await declare(this.shell, this.ob, this.settings);
    if (!ok) return;
    try {
      this.shell.info(t("Submitting to HMRC…"));
      const r = await this.shell.transition(mtdApi.submit(this.ob.period_key, true, await clientInfo()));
      this.sent = r;
      await this.shell.push(new MtdReceiptReport(this.shell, r));
    } catch (e) {
      this.shell.error(e);
    }
  }
}

/**
 * The declaration: HMRC's text and a tick box. Space (or a click) ticks it;
 * Enter submits only when it is ticked; Esc cancels.
 */
function declare(shell: Shell, ob: MtdObligation, s: MtdSettings): Promise<boolean> {
  return new Promise((resolve) => {
    const box = h("input", { type: "checkbox", id: "mtd-declare" });
    const text = t(s.declaration || DECLARATION);
    const finish = (v: boolean) => {
      shell.closeModal(m);
      resolve(v);
    };
    const accept = () => {
      if (box.checked) finish(true);
      else shell.warn(t("Tick the declaration to submit, or press Esc to cancel."));
    };
    const title = t("Submit VAT return to HMRC");
    const m: Screen = {
      title,
      el: h(
        "div.dialog",
        { role: "dialog", "aria-label": title, style: { maxWidth: "36em" } },
        h("div.dialog-title", title),
        h(
          "p",
          t("Period {key}, {span}, to {env}.", { key: ltr(ob.period_key), span: span(ob.start, ob.end), env: envLabel(s.environment) }),
        ),
        h("label", { style: { display: "flex", gap: "0.6em", alignItems: "flex-start" } }, box, h("span", text)),
        h(
          "div",
          { style: { display: "flex", gap: "1em", marginTop: "1em" } },
          h("button", { onclick: accept }, t("Submit")),
          h("button", { onclick: () => finish(false) }, t("Cancel")),
        ),
        h("p.muted", t("Space ticks the box · Enter submits · Esc cancels")),
      ),
      contexts: () => ["Dialog"],
      focus: () => box.focus(),
      key: (e) => {
        if (e.chord === "Esc") finish(false);
        else if (e.chord === "Space") box.checked = !box.checked;
        else if (e.chord === "Enter") accept();
        return true;
      },
    };
    shell.openModal(m);
  });
}

function receiptRows(r: MtdReceipt): [string, string][] {
  const rows: [string, string | null][] = [
    ["Environment", envLabel(r.environment)],
    ["VAT Reg No", r.vrn],
    ["Period key", r.period_key],
    ["Period", span(r.from, r.to)],
    ["Processing date", r.processing_date],
    ["Form bundle number", r.form_bundle_number],
    ["Charge reference", r.charge_ref_number],
    ["Payment indicator", r.payment_indicator],
    ["Receipt ID", r.receipt_id],
    ["Receipt timestamp", r.receipt_timestamp],
    ["HMRC correlation ID", r.correlation_id],
    ["Submitted at", r.submitted_at],
    ["Submitted by", r.submitted_by],
  ];
  return rows.filter((x): x is [string, string] => !!x[1]);
}

/** HMRC's receipt for a submission, as recorded in the company file. */
export class MtdReceiptReport extends Report {
  title = "MTD Receipt";

  constructor(shell: Shell, private r: MtdReceipt) {
    super(shell);
  }

  async load(): Promise<void> {
    this.heading(t("HMRC receipt"), t("VAT return for period {key}", { key: ltr(this.r.period_key) }));
    this.resetPicks();
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", t("Particulars")), h("th", t("Value")))),
        h("tbody", ...receiptRows(this.r).map(([k, v]) => this.pick(h("tr", h("td", t(k)), h("td", { dir: "auto" }, ltr(v)))))),
      ),
      h("p.gst-note", t("Keep this receipt. It is saved with the company and syncs with its books (MTD settings list every submission); print or export it with the usual keys.")),
    );
  }
}

/** A return HMRC holds (View VAT Return), with KhataOS's receipt if it sent it. */
export class MtdViewedReturnReport extends Report {
  title = "MTD Return (HMRC)";
  private v: MtdViewedReturn | null = null;

  constructor(shell: Shell, private ob: MtdObligation) {
    super(shell);
  }

  async load(): Promise<void> {
    this.v = await mtdApi.viewReturn(this.ob.period_key, await clientInfo());
    const v = this.v;
    this.heading(t("VAT Return held by HMRC"), `${ltr(this.ob.period_key)} · ${span(this.ob.start, this.ob.end)}`);
    this.resetPicks();
    const row = (box: string, label: string, amount: string) =>
      this.pick(h("tr", h("td", h("span.muted", `${box}  `), t(label)), h("td.num", M.fmt(amount))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", t("Box")), h("th.num", "£"))),
        h(
          "tbody",
          row("1", "VAT due on sales and other outputs", v.vat_due_sales),
          row("2", "VAT due on acquisitions of goods from the EU", v.vat_due_acquisitions),
          row("3", "Total VAT due", v.total_vat_due),
          row("4", "VAT reclaimed on purchases and other inputs", v.vat_reclaimed_curr_period),
          row("5", "Net VAT due (HMRC works out the direction)", v.net_vat_due),
          row("6", "Total value of sales and all other outputs excluding VAT", v.total_value_sales_ex_vat),
          row("7", "Total value of purchases and all other inputs excluding VAT", v.total_value_purchases_ex_vat),
          row("8", "Total value of supplies of goods to EU member states", v.total_value_goods_supplied_ex_vat),
          row("9", "Total value of acquisitions of goods from EU member states", v.total_acquisitions_ex_vat),
        ),
      ),
      this.ob.receipt
        ? h("p.gst-note", t("Sent with KhataOS: form bundle {n}, processed {date}. Enter opens the receipt.", { n: ltr(this.ob.receipt.form_bundle_number), date: ltr(this.ob.receipt.processing_date) }))
        : h("p.gst-note", t("Received by HMRC {date}.", { date: this.ob.received ? D.show(this.ob.received) : "—" })),
    );
    if (this.ob.receipt) {
      const r = this.ob.receipt;
      for (const p of this.picks) p.open = () => void this.shell.push(new MtdReceiptReport(this.shell, r));
    }
  }
}

// ------------------------------------------------ liabilities and payments

export class MtdAccountReport extends Report {
  title = "MTD Liabilities and Payments";
  protected override period = "range" as const;

  constructor(shell: Shell, private from: string, private to: string) {
    super(shell);
  }

  async load(): Promise<void> {
    const info = await clientInfo();
    const [liabilities, payments] = [await mtdApi.liabilities(this.from, this.to, info), await mtdApi.payments(this.from, this.to, info)];
    this.heading(t("VAT liabilities and payments (HMRC)"), span(this.from, this.to));
    this.resetPicks();
    const l = h(
      "tbody",
      ...liabilities.map((x) =>
        this.pick(
          h(
            "tr",
            h("td", { dir: "auto" }, x.kind),
            h("td", x.from && x.to ? span(x.from, x.to) : ""),
            h("td", x.due ? D.show(x.due) : ""),
            h("td.num", M.fmt(x.original_amount)),
            h("td.num", x.outstanding_amount ? M.fmt(x.outstanding_amount) : ""),
          ),
        ),
      ),
    );
    if (!liabilities.length) l.append(h("tr.empty", h("td", { colSpan: 5 }, t("No liabilities in this period."))));
    const p = h("tbody", ...payments.map((x) => this.pick(h("tr", h("td", x.received ? D.show(x.received) : ""), h("td.num", M.fmt(x.amount))))));
    if (!payments.length) p.append(h("tr.empty", h("td", { colSpan: 2 }, t("No payments in this period."))));
    this.bodyEl.replaceChildren(
      h("div.tside", h("div.tside-title", t("Liabilities")), h("table.rtable", h("thead", h("tr", ...[["Charge", false], ["Period", false], ["Due", false], ["Original", true], ["Outstanding", true]].map(([c, n]) => h(n ? "th.num" : "th", t(String(c)))))), l)),
      h("div.tside", h("div.tside-title", t("Payments")), h("table.rtable", h("thead", h("tr", h("th", t("Received")), h("th.num", t("Amount")))), p)),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    if (D.addDays(r[0], 364) < r[1]) return this.shell.warn(t("HMRC gives liabilities and payments for at most 365 days at a time."));
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }
}

// ------------------------------------------------- fraud-prevention check

/** HMRC's Test Fraud Prevention Headers API (sandbox): the verdict, its feedback and what was sent. */
export class MtdHeaderCheckReport extends Report {
  title = "MTD Header Check";
  private r: MtdHeaderCheck | null = null;

  async load(): Promise<void> {
    this.r = await this.shell.transition(mtdApi.testHeaders(await clientInfo()));
    const feedback = await mtdApi.headerFeedback().catch(() => []);
    const r = this.r;
    this.heading(t("Fraud-prevention headers"), `${r.code} · ${t("specification {v}", { v: r.spec_version ?? "?" })}`);
    this.resetPicks();
    const problems = [...r.errors.map((p) => ["Error", p] as const), ...r.warnings.map((p) => ["Advisory", p] as const)];
    const list = h(
      "tbody",
      ...problems.map(([kind, p]) => this.pick(h("tr", h("td", t(kind)), h("td", ltr(p.headers.join(", "))), h("td", { dir: "ltr" }, `${p.code}: ${p.message}`)))),
    );
    if (!problems.length) list.append(h("tr.empty", h("td", { colSpan: 3 }, t("No errors or advisories."))));
    const fb = h(
      "tbody",
      ...feedback.map((f) => this.pick(h("tr", h("td", { dir: "ltr" }, `${f.method} ${f.path}`), h("td", f.code), h("td", { dir: "ltr" }, f.problems.join("; "))))),
    );
    if (!feedback.length) fb.append(h("tr.empty", h("td", { colSpan: 3 }, t("No sandbox calls to the VAT API yet."))));
    const sent = h("tbody", ...r.sent.map(([k, v]) => this.pick(h("tr", h("td", { dir: "ltr" }, k), h("td", { dir: "ltr" }, v || t("(empty)"))))));
    this.bodyEl.replaceChildren(
      h("p.gst-note", { dir: "auto" }, r.message),
      h("table.rtable", h("thead", h("tr", h("th", t("Kind")), h("th", t("Headers")), h("th", t("Message")))), list),
      h("div.tside", h("div.tside-title", t("Feedback on the latest sandbox calls")), h("table.rtable", h("thead", h("tr", h("th", t("Request")), h("th", t("Result")), h("th", t("Problems")))), fb)),
      h("div.tside", h("div.tside-title", t("Headers sent")), h("table.rtable", h("thead", h("tr", h("th", t("Header")), h("th", t("Value")))), sent)),
      h(
        "p.gst-note",
        t("Gov-Vendor-License-IDs and Gov-Client-Multi-Factor are sent empty: KhataOS has no licence keys and signs in with a password only. HMRC asks software makers to tell them about headers they can't collect (see docs/uk-vat.md)."),
      ),
    );
  }
}

// Saudi Arabia: ZATCA Phase 2 onboarding and submission (docs/ksa-zatca.md).
//
// - ZatcaOnboardingScreen: onboard this computer as a ZATCA device (EGS).
//   Pick the environment (sandbox, simulation or production), type the OTP
//   from the Fatoora portal (valid one hour; the developer portal's test OTP
//   is 123345) and Accept: KhataOS makes the key and CSR, gets the
//   compliance CSID, runs ZATCA's compliance checks and gets the production
//   CSID. The panel shows the device, the CSR, each step, the check results
//   and where the key and secrets are kept (a user-only file on this
//   computer, outside the books). Accept with an empty OTP resumes after a
//   failed step; Ctrl+Alt+R starts again with a new device.
// - ZatcaSubmitReport: one voucher's e-invoice sent to ZATCA. It shows what
//   ZATCA said last time (if anything); Ctrl+Alt+T stamps and sends it
//   (clearance for a standard invoice, reporting for a simplified one) and
//   shows ZATCA's validation results; Ctrl+Alt+S saves the cleared XML.
// - Ctrl+Alt+N on the onboarding screen renews the production CSID with a
//   new OTP before it expires (warned 30 days ahead) or after ZATCA refused
//   it; a revoked CSID needs onboarding again (Ctrl+Alt+R).
// - ZatcaQueueReport: the documents waiting for ZATCA (simplified ones
//   must be reported within 24 hours of generation; standard ones cleared
//   before they are shared), with their deadline, attempts and last error.
//   Ctrl+Alt+T is Submit all (also generates e-invoices not generated yet,
//   and clears standard invoices in bulk); Enter opens one document.
// - startZatcaQueue(shell): the background retry, every 90 seconds while
//   the app is online and a Saudi company is open (the backend backs off
//   each document: 1, 2, 4… minutes, at most an hour).
//
// Where they open: the Company menu (Alt+K: ZATCA onboarding, ZATCA queue,
// in launcher.ts), the voucher's export (Ctrl+E / Alt+E → Submit to ZATCA,
// in einvoice.ts) and the Desk's ZATCA row; main.ts starts startZatcaQueue.
import "../gst.css";
import { save } from "@tauri-apps/plugin-dialog";
import { api, zatcaApi, zatcaQueueApi } from "../api";
import { h } from "../dom";
import { ltr, t } from "../i18n";
import type { KeyEv, Screen, Shell } from "../shell";
import type {
  Context,
  ZatcaEnvironment,
  ZatcaQueue,
  ZatcaQueueItem,
  ZatcaStatus,
  ZatcaStatusRenewal,
  ZatcaSubmission,
} from "../types";
import { Report } from "./reports";

const RESTART_KEY = "Ctrl+Alt+R";
const SUBMIT_KEY = "Ctrl+Alt+T";
const SAVE_KEY = "Ctrl+Alt+S";
const RENEW_KEY = "Ctrl+Alt+N";

function envLabel(e: string | null): string {
  switch (e) {
    case "production":
      return t("ZATCA production");
    case "simulation":
      return t("ZATCA simulation (test)");
    case "sandbox":
      return t("ZATCA developer portal (sandbox, test)");
    default:
      return t("not onboarded");
  }
}

function parseEnv(text: string): ZatcaEnvironment | null {
  const v = text.trim().toLowerCase();
  if (v.startsWith("sa") || v.startsWith("d")) return "sandbox";
  if (v.startsWith("si")) return "simulation";
  if (v.startsWith("p")) return "production";
  return null;
}

function list(title: string, items: string[]): HTMLElement | null {
  if (!items.length) return null;
  return h(
    "div",
    h("p.gst-note", h("strong", title)),
    h("ul", ...items.map((i) => h("li", { dir: "auto" }, i))),
  );
}

/** Children without the absent ones. */
function present(...xs: (Node | null)[]): Node[] {
  return xs.filter((x): x is Node => x !== null);
}

// ----------------------------------------------------------- onboarding

/** Onboard this computer with ZATCA, and show the device's state. */
export class ZatcaOnboardingScreen implements Screen {
  title = "ZATCA Onboarding";
  el = h("section.ledger");
  private inputs: HTMLInputElement[];
  private at = 0;
  private panel = h("div.zatca-status");
  private busy = false;
  private status: ZatcaStatusRenewal | null = null;

  constructor(private shell: Shell) {
    const field = (attrs: Record<string, unknown> = {}) =>
      h("input.field", { spellcheck: false, autocomplete: "off", ...attrs });
    this.inputs = [
      field({ placeholder: "sandbox / simulation / production" }),
      field({
        inputmode: "numeric",
        type: "password",
        placeholder: t("From the Fatoora portal (valid 1 hour)"),
      }),
      field({ placeholder: t("Main Branch") }),
      field({ placeholder: t("From the seller's National Address") }),
      field({ placeholder: t("Trading") }),
      field({ inputmode: "numeric", placeholder: "1100" }),
    ];
    this.inputs.forEach((inp, i) =>
      inp.addEventListener("focus", () => (this.at = i)),
    );
    this.el.append(
      h(
        "div.form.master",
        h(
          "div.form-main",
          h("label.frow", h("span.flabel", t("Environment")), this.inputs[0]),
          h("label.frow", h("span.flabel", t("OTP")), this.inputs[1]),
          h("label.frow", h("span.flabel", t("Branch")), this.inputs[2]),
          h("label.frow", h("span.flabel", t("Location")), this.inputs[3]),
          h("label.frow", h("span.flabel", t("Industry")), this.inputs[4]),
          h(
            "label.frow",
            h("span.flabel", t("Invoice types (TSCZ)")),
            this.inputs[5],
          ),
        ),
        h(
          "p.note",
          t(
            "Invoice types: 1100 issues standard (B2B) and simplified (B2C) invoices, 1000 standard only, 0100 simplified only. Branch, location and industry go into the certificate request; blanks are filled from the company.",
          ),
        ),
        this.panel,
      ),
    );
  }

  async init(): Promise<void> {
    await this.refresh();
    this.inputs[0].value = this.status?.environment ?? "sandbox";
    if (this.status?.invoice_types)
      this.inputs[5].value = this.status.invoice_types;
  }

  private async refresh(): Promise<void> {
    try {
      this.status = await zatcaQueueApi.status();
    } catch (e) {
      this.status = null;
      this.panel.replaceChildren(
        h("p.gst-note", e instanceof Error ? e.message : String(e)),
      );
      return;
    }
    this.render();
  }

  private render(): void {
    const s = this.status!;
    const yes = (b: boolean) => (b ? t("done") : t("not yet"));
    const rows: [string, string][] = [
      [t("Environment"), envLabel(s.environment)],
      [t("Device id"), s.device_id ? ltr(s.device_id) : "—"],
      [t("Device name (CN)"), s.common_name ? ltr(s.common_name) : "—"],
      [t("EGS serial number"), s.egs_serial ? ltr(s.egs_serial) : "—"],
      [t("Key and CSR"), s.csr_created ? t("created") : t("not yet")],
      [t("Compliance CSID"), yes(s.compliance_csid)],
      [
        t("Compliance checks"),
        s.checks.length
          ? t("{passed} of {n} passed", {
              passed: s.checks.filter((c) => c.status !== "ERROR").length,
              n: s.checks.length,
            })
          : t("not yet"),
      ],
      [t("Production CSID"), yes(s.production_csid)],
      [
        t("Certificate expires"),
        s.certificate_expiry
          ? ltr(s.certificate_expiry) +
            (s.days_to_expiry === null
              ? ""
              : s.expired
                ? ` (${t("expired")})`
                : ` (${t("in {n} days", { n: s.days_to_expiry })})`)
          : "—",
      ],
      [t("CSID renewed"), s.renewed_at ? ltr(s.renewed_at) : "—"],
      [
        t("Waiting for ZATCA"),
        s.queue_pending
          ? t("{n} document(s): see ZATCA Queue", { n: s.queue_pending })
          : t("nothing"),
      ],
      [
        t("Key and secrets file"),
        s.credentials_path
          ? ltr(s.credentials_path) +
            (s.credentials_present ? "" : ` (${t("missing on this computer")})`)
          : "—",
      ],
      [t("Invoice counter (ICV)"), String(s.icv)],
    ];
    const checks = s.checks.length
      ? h(
          "table.rtable",
          h(
            "thead",
            h(
              "tr",
              ...["Document", "Result", "Messages"].map((c) => h("th", t(c))),
            ),
          ),
          h(
            "tbody",
            ...s.checks.map((c) =>
              h(
                "tr",
                h("td", t(c.document)),
                h("td", c.status),
                h(
                  "td",
                  { dir: "auto" },
                  [...c.errors, ...c.warnings].join("; ") || "—",
                ),
              ),
            ),
          ),
        )
      : null;
    this.panel.replaceChildren(
      ...present(
        h(
          "p.gst-note",
          { role: "status" },
          h("strong", s.onboarded ? t("Onboarded.") : t("Not onboarded yet.")),
          " ",
          t(s.next_step),
        ),
        h(
          "table.rtable",
          h("tbody", ...rows.map(([k, v]) => h("tr", h("td", k), h("td", v)))),
        ),
        checks,
        s.last_error
          ? h(
              "p.gst-note",
              { role: "alert" },
              t("Last error: {e}", { e: s.last_error }),
            )
          : null,
        s.renewal_due || s.expired || s.csid_refused
          ? h(
              "p.gst-note",
              { role: "alert" },
              t(
                "Renew the CSID: get a new OTP from the Fatoora portal, type it in the OTP field and press {key}. A revoked CSID cannot be renewed: start again ({restart}).",
                { key: RENEW_KEY, restart: RESTART_KEY },
              ),
            )
          : null,
        list(t("Notes"), s.warnings),
        h(
          "p.note",
          t(
            "The private key and the CSID secrets are kept only in the file above, readable by your user account alone: never in the company file, its backups, the audit trail or sync. A copied or restored company must be onboarded again. {restart}: start again with a new device.",
            { restart: RESTART_KEY },
          ),
        ),
      ),
    );
  }

  contexts(): Context[] {
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    return [
      this.shell.btn("Accept", this.contexts(), "Onboard"),
      {
        chord: RENEW_KEY,
        label: "Renew CSID",
        enabled: !!this.status?.production_csid && !this.busy,
      },
      {
        chord: RESTART_KEY,
        label: "Start Again",
        enabled: !!this.status?.device_id && !this.busy,
      },
      this.shell.btn("Back", this.contexts()),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
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
    if (e.chord === RESTART_KEY) {
      void this.onboard(true);
      return true;
    }
    if (e.chord === RENEW_KEY) {
      void this.renew();
      return true;
    }
    return false;
  }

  /** Renew the production CSID with the OTP typed in the OTP field. */
  private async renew(): Promise<void> {
    if (this.busy) return;
    if (!this.status?.production_csid)
      return this.shell.warn(t("This computer has no production CSID to renew: onboard it first."));
    const otp = this.inputs[1].value.trim();
    if (!/^\d+$/.test(otp)) {
      this.go(1);
      return this.shell.warn(t("Type the new OTP from the Fatoora portal in the OTP field, then renew."));
    }
    if (
      !(await this.shell.ask(
        t("Renew this computer's CSID with ZATCA {env}? A new key and certificate replace the current ones; the invoice counter carries on.", {
          env: envLabel(this.status.environment),
        }),
        { yesFirst: false },
      ))
    )
      return;
    this.busy = true;
    try {
      this.shell.info(t("Renewing the CSID…"), 15000);
      this.status = await this.shell.transition(
        zatcaQueueApi.renew(otp, {
          branch: this.inputs[2].value,
          location: this.inputs[3].value,
          industry: this.inputs[4].value,
        }),
      );
      this.inputs[1].value = "";
      this.render();
      this.shell.info(t("The CSID is renewed until {date}.", { date: ltr(this.status.certificate_expiry ?? "—") }), 8000);
    } catch (e) {
      this.shell.error(e);
      await this.refresh();
    } finally {
      this.busy = false;
      this.shell.refresh();
    }
  }

  action(a: string): boolean {
    switch (a) {
      case "NextField":
        if (this.at < this.inputs.length - 1) this.go(this.at + 1);
        else void this.onboard(false);
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Accept":
        void this.onboard(false);
        return true;
      case "Back":
        if (this.inputs[this.at].value && this.at > 0)
          this.inputs[this.at].value = "";
        else this.shell.pop();
        return true;
    }
    return false;
  }

  private async onboard(restart: boolean): Promise<void> {
    if (this.busy) return;
    const env = parseEnv(this.inputs[0].value);
    if (!env) {
      this.go(0);
      return this.shell.warn(
        t("Type sandbox (developer portal), simulation or production."),
      );
    }
    const otp = this.inputs[1].value.trim();
    if (otp && !/^\d+$/.test(otp)) {
      this.go(1);
      return this.shell.warn(
        t("The OTP is the number shown on the Fatoora portal."),
      );
    }
    const types = this.inputs[5].value.trim();
    if (types && !/^[01]{4}$/.test(types)) {
      this.go(5);
      return this.shell.warn(
        t("Invoice types are four digits of 0 and 1, such as 1100."),
      );
    }
    if (
      env === "production" &&
      !(await this.shell.ask(
        t(
          "Onboard this computer with ZATCA production? Its invoices will then be legally issued through ZATCA.",
        ),
        { yesFirst: false },
      ))
    )
      return;
    if (
      restart &&
      !(await this.shell.ask(
        t(
          "Start again with a new device id and key? This computer must then be onboarded with a new OTP.",
        ),
        { yesFirst: false },
      ))
    )
      return;
    this.busy = true;
    try {
      this.shell.info(
        t("Onboarding with {env}…", { env: envLabel(env) }),
        15000,
      );
      // The backend's status carries the renewal fields too.
      const status = (await this.shell.transition(
        zatcaApi.onboard(env, otp, {
          branch: this.inputs[2].value,
          location: this.inputs[3].value,
          industry: this.inputs[4].value,
          invoice_types: types,
          restart,
        }),
      )) as ZatcaStatusRenewal;
      this.status = status;
      this.inputs[1].value = "";
      this.render();
      this.shell.info(
        status.onboarded
          ? t("This computer is onboarded with {env}.", { env: envLabel(env) })
          : t(status.next_step),
        8000,
      );
    } catch (e) {
      this.shell.error(e);
      await this.refresh();
    } finally {
      this.busy = false;
      // Start Again depends on the device id the answer brings.
      this.shell.refresh();
    }
  }
}

// ------------------------------------------------------------- submit

/** One voucher's e-invoice and what ZATCA said about it. */
export class ZatcaSubmitReport extends Report {
  title = "ZATCA Submission";
  private sub: ZatcaSubmission | null = null;
  private status: ZatcaStatus | null = null;
  private busy = false;

  /** `kind` is asked only for a voucher never generated before; otherwise the buyer's VAT number decides. */
  constructor(
    shell: Shell,
    private voucherId: string,
    private label = "",
    private kind?: "standard" | "simplified",
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    [this.status, this.sub] = await Promise.all([
      zatcaApi.status(),
      zatcaApi.submission(this.voucherId),
    ]);
    this.render();
  }

  private render(): void {
    const s = this.sub;
    this.heading(
      t("ZATCA Submission"),
      `${this.label ? `${this.label} · ` : ""}${envLabel(this.status?.environment ?? null)}`,
    );
    this.resetPicks();
    if (!this.status?.onboarded) {
      this.bodyEl.replaceChildren(
        h(
          "p.gst-note",
          t(
            "This computer is not onboarded with ZATCA yet: open ZATCA Onboarding first.",
          ),
        ),
      );
      return;
    }
    if (!s) {
      this.bodyEl.replaceChildren(
        h(
          "p.gst-note",
          t(
            "Not sent to ZATCA yet. {key} stamps the e-invoice and sends it: a standard invoice for clearance, a simplified one for reporting.",
            { key: SUBMIT_KEY },
          ),
        ),
      );
      return;
    }
    const result = s.accepted
      ? s.clearance_status === "CLEARED"
        ? t("Cleared by ZATCA")
        : t("Reported to ZATCA")
      : s.endpoint === "clearance"
        ? t("Not cleared: ZATCA refused it")
        : t("Not reported: ZATCA refused it");
    const rows: [string, string][] = [
      [
        t("Result"),
        result + (s.warnings.length ? ` (${t("with warnings")})` : ""),
      ],
      [
        t("Document"),
        t(
          s.kind === "standard"
            ? "standard tax invoice"
            : "simplified tax invoice",
        ),
      ],
      [
        t("Sent to"),
        s.endpoint === "clearance" ? t("clearance") : t("reporting"),
      ],
      [t("Validation status"), s.status],
      [t("HTTP status"), String(s.http_status)],
      [t("Submitted"), ltr(s.submitted_at)],
    ];
    this.bodyEl.replaceChildren(
      ...present(
        h(
          "p.gst-note",
          { role: "status" },
          h("strong", result),
          s.already_submitted
            ? ` · ${t("as recorded; nothing was sent again")}`
            : "",
        ),
        h(
          "table.rtable",
          h("tbody", ...rows.map(([k, v]) => h("tr", h("td", k), h("td", v)))),
        ),
        list(t("Errors"), s.errors),
        list(t("Warnings"), s.warnings),
        list(t("Information"), s.info),
        list(t("Notes"), s.notes),
        s.cleared_xml
          ? h(
              "p.gst-note",
              t(
                "ZATCA returned the cleared invoice with its own stamp and QR code: give the buyer this XML ({key} saves it). The print uses ZATCA's QR code.",
                { key: SAVE_KEY },
              ),
            )
          : null,
        !s.accepted
          ? h(
              "p.gst-note",
              t(
                "Correct the problem and send again ({key}). A refused invoice keeps its counter; ZATCA accepts gaps.",
                { key: SUBMIT_KEY },
              ),
            )
          : null,
      ),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(
      b.length - 1,
      0,
      {
        chord: SUBMIT_KEY,
        label: "Submit to ZATCA",
        enabled: !!this.status?.onboarded && !this.sub?.accepted && !this.busy,
      },
      {
        chord: SAVE_KEY,
        label: "Save Cleared XML",
        enabled: !!this.sub?.cleared_xml,
      },
    );
    return b;
  }

  override key(e: KeyEv): boolean {
    switch (e.chord) {
      case SUBMIT_KEY:
        if (this.sub?.accepted)
          this.shell.warn(t("ZATCA has already accepted this e-invoice."));
        else void this.submit();
        return true;
      case SAVE_KEY:
        void this.saveCleared();
        return true;
    }
    return super.key(e);
  }

  private async submit(): Promise<void> {
    if (this.busy) return;
    if (
      this.status?.environment === "production" &&
      !(await this.shell.ask(t("Send this e-invoice to ZATCA production?")))
    )
      return;
    this.busy = true;
    try {
      this.shell.info(t("Sending to ZATCA…"), 15000);
      this.sub = await this.shell.transition(
        zatcaApi.submit(this.voucherId, this.kind),
      );
      this.render();
      this.shell.info(
        this.sub.accepted
          ? t("ZATCA accepted the e-invoice.")
          : t("ZATCA refused the e-invoice; see its errors."),
        8000,
      );
    } catch (e) {
      this.shell.error(e);
    } finally {
      this.busy = false;
      // The keys follow the answer: Submit off once accepted, Save Cleared XML on.
      this.shell.refresh();
    }
  }

  private async saveCleared(): Promise<void> {
    const xml = this.sub?.cleared_xml;
    if (!xml) return this.shell.warn(t("There is no cleared invoice to save."));
    try {
      const path = await save({
        defaultPath: this.sub?.file_name || "cleared-invoice.xml",
        filters: [{ name: "XML", extensions: ["xml"] }],
      });
      if (!path) return;
      const file = path.toLowerCase().endsWith(".xml") ? path : `${path}.xml`;
      await api.writeGstJson(file, xml);
      this.shell.info(
        t("Cleared invoice saved to {file}.", { file: ltr(file) }),
        8000,
      );
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// --------------------------------------------------------------- queue

function stateLabel(i: ZatcaQueueItem): string {
  switch (i.state) {
    case "pending":
      return t("waiting");
    case "retrying":
      return t("retrying (attempt {n})", { n: i.attempts + 1 });
    case "failed":
      return t("refused: correct and submit again");
    default:
      return t("not generated yet");
  }
}

function deadlineLabel(i: ZatcaQueueItem): string {
  if (!i.deadline) return i.kind === "standard" ? t("clear before sharing") : "—";
  if (i.overdue) return t("late: report now");
  return t("{h} h left", { h: i.hours_left ?? 0 });
}

/** The documents waiting for ZATCA, and Submit all. */
export class ZatcaQueueReport extends Report {
  title = "ZATCA Queue";
  private q: ZatcaQueue | null = null;
  private status: ZatcaStatusRenewal | null = null;
  private busy = false;

  async load(): Promise<void> {
    [this.status, this.q] = await Promise.all([zatcaQueueApi.status(), zatcaQueueApi.queue()]);
    this.render();
  }

  private render(): void {
    const q = this.q!;
    this.heading(
      t("ZATCA Queue"),
      `${envLabel(this.status?.environment ?? null)} · ${t("{n} waiting, {f} refused, {g} not generated", { n: q.pending, f: q.failed, g: q.not_generated })}`,
    );
    this.resetPicks();
    const rows = q.items.map((i) =>
      this.pick(
        h(
          "tr",
          { class: i.overdue || i.state === "failed" ? "gst-credit" : "" },
          h("td", { dir: "auto" }, i.number),
          h("td", ltr(i.date)),
          h("td", { dir: "auto" }, i.party),
          h("td", i.kind ? t(i.kind === "standard" ? "standard" : "simplified") : "—"),
          h("td", stateLabel(i)),
          h("td", deadlineLabel(i)),
          h("td", { dir: "auto" }, i.last_error ?? ""),
        ),
        () => void this.shell.push(new ZatcaSubmitReport(this.shell, i.voucher_id, i.number, i.kind === "standard" || i.kind === "simplified" ? i.kind : undefined)),
      ),
    );
    this.bodyEl.replaceChildren(
      ...present(
        q.notes.length ? h("div.gst-warnings", { role: "status" }, h("ul", ...q.notes.map((n) => h("li", { dir: "auto" }, n)))) : null,
        q.items.length
          ? h(
              "table.rtable",
              h("thead", h("tr", ...["Number", "Date", "Party", "Kind", "State", "Report by", "Last error"].map((c) => h("th", t(c))))),
              h("tbody", ...rows),
            )
          : h("p.gst-note", t("Nothing is waiting: ZATCA has every e-invoice of this computer.")),
        h(
          "p.gst-note",
          t(
            "Simplified invoices and their notes must be reported within 24 hours of generation, standard ones cleared before they are shared with the buyer. KhataOS retries on its own while the app is online (after 1, 2, 4… minutes, at most an hour); {key} sends everything now, generating and stamping the e-invoices not generated yet. Enter opens a document.",
            { key: SUBMIT_KEY },
          ),
        ),
      ),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, {
      chord: SUBMIT_KEY,
      label: "Submit All",
      enabled: !!this.q?.items.length && !!this.q?.ready && !this.busy,
    });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === SUBMIT_KEY) {
      void this.submitAll();
      return true;
    }
    return super.key(e);
  }

  private async submitAll(): Promise<void> {
    if (this.busy || !this.q) return;
    if (!this.q.ready) return this.shell.warn(t("This computer cannot submit yet: see ZATCA Onboarding."));
    if (
      this.status?.environment === "production" &&
      !(await this.shell.ask(t("Send {n} document(s) to ZATCA production?", { n: this.q.items.length })))
    )
      return;
    this.busy = true;
    try {
      this.shell.info(t("Sending to ZATCA…"), 15000);
      const r = await this.shell.transition(zatcaQueueApi.run(true));
      this.q = r.queue;
      this.render();
      if (r.stopped) this.shell.warn(r.stopped);
      else
        this.shell.info(
          t("{a} accepted, {f} refused, {l} to try again later.", { a: r.accepted, f: r.refused, l: r.retry_later }),
          8000,
        );
    } catch (e) {
      this.shell.error(e);
    } finally {
      this.busy = false;
      this.shell.refresh();
    }
  }
}

/** Retry the ZATCA queue in the background every 90 seconds while online
 *  and a Saudi company is open, and when the connection comes back. The
 *  backend decides what is due (each document backs off on its own).
 *  Returns a function that stops it. The lead calls this once at start. */
export function startZatcaQueue(shell: Shell, everyMs = 90_000): () => void {
  let running = false;
  const tick = async () => {
    if (running || !navigator.onLine || shell.company?.country?.code !== "SA") return;
    running = true;
    try {
      const s = await zatcaQueueApi.status();
      if (s.onboarded && s.queue_pending > 0 && !s.expired && !s.csid_refused) await zatcaQueueApi.run(false);
    } catch {
      // Offline or no company: the next tick tries again.
    } finally {
      running = false;
    }
  };
  const timer = window.setInterval(() => void tick(), everyMs);
  const online = () => void tick();
  window.addEventListener("online", online);
  return () => {
    window.clearInterval(timer);
    window.removeEventListener("online", online);
  };
}

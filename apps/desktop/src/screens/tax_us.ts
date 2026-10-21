// United States: the sales tax liability report by jurisdiction and state
// (khata-us), the use tax report (purchases to self-assess), customers'
// resale and exemption certificates with an expiring-certificates report,
// and the Form 1099 vendor list for a calendar year. All are preparation
// aids: nothing is filed with a state or the IRS. Enter on a row opens the
// Day Book for the period. See docs/us-sales-tax.md.
import "../gst.css";
import { api, usApi, usExtraApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { KeyEv, Shell } from "../shell";
import type { Us1099Report, UsCertificate, UsCertificateKind, UsCertificateReport, UsSalesTaxReport, UsSalesTaxTotals, UsUseTaxReport } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the calendar quarter: many states file quarterly. */
function quarterStart(iso: string): string {
  const m = Number(iso.slice(5, 7));
  const q = Math.floor((m - 1) / 3) * 3 + 1;
  return `${iso.slice(0, 4)}-${String(q).padStart(2, "0")}-01`;
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

const TOTAL_COLS: [string, boolean][] = [
  ["Gross sales", true],
  ["Exempt", true],
  ["of which by certificate", true],
  ["Not collected", true],
  ["Taxable", true],
  ["Tax collected", true],
];

function totals(x: UsSalesTaxTotals): HTMLElement[] {
  return [num(x.gross_sales), num(x.exempt_sales), num(x.certificate_exempt_sales ?? "0"), num(x.not_collected_sales), num(x.taxable_sales), num(x.tax_collected)];
}

export class UsSalesTaxReportScreen extends Report {
  title = "Sales Tax Report";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render(await usApi.salesTaxReport(this.from, this.to));
  }

  private render(r: UsSalesTaxReport): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("Sales Tax Report"), `${r.ein ? t("EIN {number}", { number: r.ein }) : t("No EIN")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    const byJurisdiction = h(
      "tbody",
      ...r.jurisdictions.map((j) => this.pick(h("tr", h("td", j.jurisdiction, h("span.muted", ` · ${j.state_name}`)), ...totals(j)), dayBook)),
      h("tr.rtotal", h("td", t("Total")), ...totals(r.total)),
    );
    const byState = h(
      "tbody",
      ...r.states.map((s) => {
        const nexus = s.nexus
          ? h("span.muted", ` · ${s.nexus.met ? t("nexus threshold met this year") : t("below the nexus threshold this year")}`)
          : "";
        return this.pick(h("tr", h("td", s.state_name, nexus), ...totals(s)), dayBook);
      }),
    );
    const exports = h("tbody", this.pick(h("tr", h("td", t("Sales abroad (exports)")), num(r.export_sales)), dayBook));
    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h("table.rtable", head(["Jurisdiction", false], ...TOTAL_COLS), byJurisdiction),
      h("table.rtable", head(["State", false], ...TOTAL_COLS), byState),
      h("table.rtable", head(["Outside the US", false], ["Value", true]), exports),
      h(
        "p.gst-note",
        t(
          "Worked out from the books with the jurisdictions and rates you set up (STATE, COUNTY, CITY and DISTRICT components). Each state has its own return; use these figures to fill it. Exempt by certificate: sales to customers whose resale or exemption certificate for the state was in force on the sale's date. Not collected: sales into a state with no rate on file. The nexus note compares this calendar year's sales with the state's economic nexus threshold where KhataOS has a sourced figure; it is an early warning, not a determination. Enter opens the Day Book.",
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

export class Us1099ReportScreen extends Report {
  title = "Form 1099 Vendors";
  protected override period = "range" as const;

  constructor(shell: Shell, private year = Number(today(shell).slice(0, 4))) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render(await usApi.form1099(this.year));
  }

  private render(r: Us1099Report): void {
    this.heading(t("Form 1099 Vendors"), t("Calendar year {year} · threshold {amount}", { year: String(r.year), amount: M.fmt(r.threshold) }));
    this.resetPicks();
    const from = `${r.year}-01-01`;
    const to = `${r.year}-12-31`;
    const dayBook = () => void this.shell.push(new DayBook(this.shell, from, to));
    const rows = h(
      "tbody",
      ...r.rows.map((x) => this.pick(h("tr", h("td", x.party_name), h("td", x.tin ?? t("no TIN — get a W-9")), h("td", x.form), num(x.total)), dayBook)),
    );
    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h("table.rtable", head(["Payee", false], ["TIN", false], ["Form and box", false], ["Paid", true]), rows),
      h(
        "p.gst-note",
        t(
          "Payments (Payment vouchers) to Sundry Creditors in the year that reach the threshold: $600 for payments through 2025, $2,000 from 2026 (indexed for inflation after 2026). {below} payee(s) stayed below it. Payments to corporations and payments by card or payment app (Form 1099-K) are generally not reported; KhataOS cannot tell them apart. Nothing is filed.",
          { below: String(r.below) },
        ),
      ),
    );
  }

  /** The period is a calendar year. */
  protected override async askPeriod(): Promise<void> {
    const v = await this.shell.dialog("Calendar year", [{ label: "Year", value: String(this.year) }], ([y]) =>
      /^\d{4}$/.test(y.trim()) ? null : t("Type a year such as 2026."),
    );
    if (!v) return;
    this.year = Number(v[0].trim());
    this.cursor = 0;
    await this.reload();
  }
}

// ------------------------------------------------------------ use tax

const JURISDICTION_KEY = "Ctrl+Alt+J";

export class UsUseTaxReportScreen extends Report {
  title = "Use Tax";
  protected override period = "range" as const;
  private jurisdiction: string | undefined;
  private shownJurisdiction = "";

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render(await usExtraApi.useTaxReport(this.from, this.to, this.jurisdiction));
  }

  private render(r: UsUseTaxReport): void {
    this.shownJurisdiction = r.jurisdiction;
    const span = t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("Use Tax"), `${t("Used in {jurisdiction}", { jurisdiction: r.jurisdiction })} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    const rows = h(
      "tbody",
      ...r.rows.map((x) =>
        this.pick(
          h("tr", h("td", D.show(x.date)), h("td", x.number), h("td", x.vendor_name, h("span.muted", ` · ${x.vendor_location}`)), num(x.taxable_value), num(x.exempt_value), num(x.use_tax)),
          dayBook,
        ),
      ),
      h("tr.rtotal", h("td", t("Total")), h("td"), h("td"), num(r.taxable_value), h("td"), num(r.use_tax)),
    );
    const components = h("tbody", ...r.by_component.map(([c, a]) => h("tr", h("td", c), num(a))));
    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h("table.rtable", head(["Date", false], ["Purchase", false], ["Vendor", false], ["Subject to use tax", true], ["Resale or exempt", true], ["Use tax", true]), rows),
      h("table.rtable", head(["Component", false], ["Use tax", true]), components),
      h(
        "p.gst-note",
        t(
          "Purchases from vendors outside your state that charged no sales tax, at the rates of the jurisdiction where the goods are used ({key} changes it). Stock items are taken as bought for resale; other lines take their ledger's tax class. {taxed} out-of-state purchase(s) were taxed by the vendor; {local} in-state purchase(s) carry no tax. This is the liability to self-assess on your sales and use tax return: book it (expense or asset to a use tax payable ledger) and pay it with the return. Enter opens the Day Book.",
          { key: JURISDICTION_KEY, taxed: String(r.taxed_by_vendor), local: String(r.in_state_untaxed) },
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

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: JURISDICTION_KEY, label: "Jurisdiction", enabled: true });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === JURISDICTION_KEY) {
      void (async () => {
        const v = await this.shell.dialog("Where the goods are used", [{ label: "Jurisdiction in your state (CA, or CA-LA…)", value: this.shownJurisdiction }], ([j]) =>
          /^[A-Za-z]{2}(-[A-Za-z0-9]{1,12})?$/.test(j.trim()) ? null : t("A state code, optionally followed by - and your local code."),
        );
        if (!v) return;
        this.jurisdiction = v[0].trim().toUpperCase();
        try {
          await this.reload();
        } catch (err) {
          this.jurisdiction = undefined;
          this.shell.error(err);
        }
      })();
      return true;
    }
    return super.key(e);
  }
}

// ------------------------------------------------------------ exemption certificates

const KINDS: Record<string, UsCertificateKind> = { R: "Resale", E: "ExemptOrganization", G: "Government", O: "Other" };
const KIND_LABEL: Record<UsCertificateKind, string> = { Resale: "resale", ExemptOrganization: "exempt organization", Government: "government", Other: "other" };

function isoOrNull(text: string): string | null {
  const v = text.trim();
  return v ? v : null;
}

/** Add a resale or exemption certificate to a customer (a shared record of the company, synced). */
export async function addCertificate(shell: Shell, ledgerId: string, customerName: string): Promise<boolean> {
  try {
    const v = await shell.dialog(
      t("Certificate for {name}", { name: customerName }),
      [
        { label: "State (CA, TX…)", value: "" },
        { label: "Certificate or permit number", value: "" },
        { label: "Type (R resale · E exempt organization · G government · O other)", value: "R" },
        { label: "Given on (YYYY-MM-DD, optional)", value: today(shell) },
        { label: "Expires on (YYYY-MM-DD; blank: no expiry date)", value: "" },
        { label: "Note (optional)", value: "" },
      ],
      ([st, n, k, i, x]) => {
        if (!/^[A-Za-z]{2}$/.test(st.trim())) return t("Type the state's 2-letter code.");
        if (!n.trim()) return t("Type the certificate number.");
        if (!KINDS[k.trim().toUpperCase()]) return t("The type is R, E, G or O.");
        for (const d of [i, x]) if (d.trim() && !/^\d{4}-\d{2}-\d{2}$/.test(d.trim())) return t("Dates are YYYY-MM-DD.");
        return null;
      },
    );
    if (!v) return false;
    const current = await usExtraApi.certificates(ledgerId);
    const cert: UsCertificate = { state: v[0].trim().toUpperCase(), number: v[1].trim(), kind: KINDS[v[2].trim().toUpperCase()], issued: isoOrNull(v[3]), expires: isoOrNull(v[4]), note: v[5].trim() };
    await usExtraApi.setCertificates(ledgerId, [...current, cert]);
    shell.info(t("Certificate {number} ({state}) saved for {name}.", { number: cert.number, state: cert.state, name: customerName }));
    return true;
  } catch (e) {
    shell.error(e);
    return false;
  }
}

const ADD_KEY = "Ctrl+Alt+A";
const DELETE_KEY = "Ctrl+Alt+D";

/** Every certificate on file, expired and expiring first; Ctrl+Alt+A adds one, Ctrl+Alt+D removes the one under the cursor. */
export class UsCertificatesScreen extends Report {
  title = "Exemption Certificates";
  protected override period = "range" as const;
  private report: UsCertificateReport | null = null;

  constructor(shell: Shell, private asOf = today(shell), private within = 60) {
    super(shell);
  }

  async load(): Promise<void> {
    this.report = await usExtraApi.certificateReport(this.asOf, this.within);
    this.render(this.report);
  }

  private render(r: UsCertificateReport): void {
    this.heading(t("Exemption Certificates"), t("As of {date} · expiring within {days} days", { date: D.show(r.as_of), days: String(r.within_days) }));
    this.resetPicks();
    const status = (x: UsCertificateReport["rows"][number]) => {
      switch (x.status) {
        case "Expired":
          return t("expired {days} days ago", { days: String(-(x.days_left ?? 0)) });
        case "Expiring":
          return t("expires in {days} days", { days: String(x.days_left ?? 0) });
        case "Valid":
          return t("valid");
        case "NoExpiry":
          return t("no expiry date");
        default:
          return t("not yet given");
      }
    };
    const rows = h(
      "tbody",
      ...r.rows.map((x) =>
        this.pick(
          h(
            "tr",
            { class: x.status === "Expired" ? "gst-credit" : "" },
            h("td", x.customer_name),
            h("td", x.state, h("span.muted", ` · ${x.state_name}`)),
            h("td", x.number),
            h("td", t(KIND_LABEL[x.kind])),
            h("td", x.expires ? D.show(x.expires) : "—"),
            h("td", status(x)),
          ),
        ),
      ),
    );
    const summary = t("{expired} expired, {expiring} expiring", { expired: String(r.expired), expiring: String(r.expiring) });
    this.bodyEl.replaceChildren(
      h("table.rtable", head(["Customer", false], ["State", false], ["Number", false], ["Type", false], ["Expires", false], ["Status", false]), rows),
      h(
        "p.gst-note",
        `${summary}. ${t(
          "A certificate in force for the sale's state makes that customer's sales exempt in the Sales Tax Report. Renew expired ones before the next sale. {add} adds a certificate, {del} removes the one under the cursor. Certificates sync with the books to the company's other devices and its accountant.",
          { add: ADD_KEY, del: DELETE_KEY },
        )}`,
      ),
    );
  }

  /** The period is the report date and the warning window. */
  protected override async askPeriod(): Promise<void> {
    const v = await this.shell.dialog(
      "Certificates",
      [
        { label: "As of (YYYY-MM-DD)", value: this.asOf },
        { label: "Warn this many days before expiry", value: String(this.within) },
      ],
      ([d, w]) => (/^\d{4}-\d{2}-\d{2}$/.test(d.trim()) ? (/^\d{1,4}$/.test(w.trim()) ? null : t("Type a number of days.")) : t("Dates are YYYY-MM-DD.")),
    );
    if (!v) return;
    this.asOf = v[0].trim();
    this.within = Number(v[1].trim());
    this.cursor = 0;
    await this.reload();
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: ADD_KEY, label: "Add certificate", enabled: true }, { chord: DELETE_KEY, label: "Remove", enabled: (this.report?.rows.length ?? 0) > 0 });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === ADD_KEY) {
      void (async () => {
        const row = this.report?.rows[this.cursor];
        let id = row?.customer_id;
        let name = row?.customer_name ?? "";
        const v = await this.shell.dialog("Customer", [{ label: "Customer ledger name", value: name }], ([n]) => (n.trim() ? null : t("Type the customer's name.")));
        if (!v) return;
        if (v[0].trim() !== name) {
          const found = (await api.listLedgers(v[0].trim(), "party")).find((l) => l.name.toLowerCase() === v[0].trim().toLowerCase());
          if (!found) {
            this.shell.warn(t("No party ledger is named {name}.", { name: v[0].trim() }));
            return;
          }
          id = found.id;
          name = found.name;
        }
        if (id && (await addCertificate(this.shell, id, name))) await this.reload();
      })();
      return true;
    }
    if (e.chord === DELETE_KEY) {
      const row = this.report?.rows[this.cursor];
      if (!row) return true;
      void (async () => {
        try {
          const current = await usExtraApi.certificates(row.customer_id);
          await usExtraApi.setCertificates(
            row.customer_id,
            current.filter((c) => !(c.state === row.state && c.number === row.number)),
          );
          this.shell.info(t("Certificate {number} removed.", { number: row.number }));
          await this.reload();
        } catch (err) {
          this.shell.error(err);
        }
      })();
      return true;
    }
    return super.key(e);
  }
}

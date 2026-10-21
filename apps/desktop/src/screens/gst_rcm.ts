// GST (India), the advanced parts: reverse-charge register with
// self-invoices, GSTR-3B Table 4 in full (ITC, reversals, net), the Rule 37
// report (suppliers unpaid after 180 days), the Rule 42 calculator, the
// reverse-charge / blocked-credit flags on ledgers and items, and GSTR-1A
// candidates (documents changed after their GSTR-1 was marked filed).
// The rules are in khata-in rcm.rs and returns.rs; the books side in
// src-tauri gst_rcm.rs and gst.rs. A preparation aid, not legal advice.
import "../gst.css";
import { api, gstAdvApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t as tr } from "../i18n";
import * as M from "../money";
import type { KeyEv, Shell } from "../shell";
import type { GstAmounts, GstDocSnapshot, GstFlagRow, GstNetRowRc, Gstr1Amendment, Gstr3b, Rule42Result } from "../types";
import { Report } from "./reports";
import { savedVoucherScreen } from "./stockjournal";

const SELF_INVOICE_KEY = "Ctrl+Alt+P";
const RCM_KEY = "Alt+R";
const BLOCKED_KEY = "Alt+B";
const MARK_FILED_KEY = "Alt+F";
const RULE42_KEY = "Alt+4";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function monthStart(iso: string): string {
  return `${iso.slice(0, 8)}01`;
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function num(text: string): HTMLElement {
  return h("td.num", M.fmt(text, true));
}

function taxCells(a: GstAmounts): HTMLElement[] {
  return [num(a.taxable), num(a.igst), num(a.cgst), num(a.sgst), num(a.cess)];
}

function taxOnly(a: GstAmounts): HTMLElement[] {
  return [num(a.igst), num(a.cgst), num(a.sgst), num(a.cess)];
}

const TAX_HEADS = ["Taxable Value", "IGST", "CGST", "SGST/UTGST", "Cess"];

function head(cols: [string, boolean][]): HTMLElement {
  return h("thead", h("tr", ...cols.map(([c, n]) => h(n ? "th.num" : "th", tr(c)))));
}

function taxHead(...first: string[]): HTMLElement {
  return head([...first.map((c): [string, boolean] => [c, false]), ...TAX_HEADS.map((c): [string, boolean] => [c, true])]);
}

function warnings(list: string[]): HTMLElement | null {
  if (!list.length) return null;
  return h(
    "div.gst-warnings",
    { role: "status" },
    h("div.gst-warn-title", list.length === 1 ? tr("1 thing to check") : tr("{n} things to check", { n: list.length })),
    h("ul", ...list.map((w) => h("li", { dir: "auto" }, w))),
  );
}

function note(text: string): HTMLElement {
  return h("p.gst-note", text);
}

function zero(): GstAmounts {
  return { taxable: "0", igst: "0", cgst: "0", sgst: "0", cess: "0" };
}

// ------------------------------------------------- reverse-charge register

export class RcmRegisterReport extends Report {
  title = "Reverse Charge Register";
  protected override period = "range" as const;
  private ids: (string | null)[] = [];

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = await gstAdvApi.rcmRegister(this.from, this.to);
    this.heading(tr("Reverse Charge Register"), range(this.from, this.to));
    this.resetPicks();
    this.ids = [];
    const tbody = h("tbody");
    for (const row of r.rows) {
      const due = row.self_invoice_due ? tr("due {date}", { date: D.show(row.self_invoice_due) }) : "";
      const late = row.self_invoice_due !== null && row.self_invoice_due < today(this.shell);
      tbody.append(
        this.pick(
          h(
            "tr",
            { class: row.cancelled ? "muted" : "" },
            h("td", D.show(row.date)),
            h("td", ltr(row.number), row.kind === "DebitNote" ? h("span.muted", ` (${tr("Debit Note")})`) : null),
            h("td.strong", row.party, h("span.muted", ` · ${row.party_gstin ? ltr(row.party_gstin) : tr("Unregistered")}`), row.blocked ? h("span.muted", ` · ${tr("blocked credit")}`) : null),
            h("td", row.self_invoice ? ltr(row.self_invoice) : tr("Supplier's invoice"), due ? h("div.muted", { class: late ? "gst-credit" : "" }, due) : null),
            ...taxCells(row.amounts),
          ),
          () => void this.shell.push(savedVoucherScreen(this.shell, row.kind, row.date, row.voucher_id)),
        ),
      );
      this.ids.push(row.self_invoice ? row.voucher_id : null);
    }
    if (!r.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 9 }, tr("No purchases on reverse charge in this period."))));
    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h("table.rtable", taxHead("Date", "No.", "Supplier", "Self-invoice"), tbody, h("tfoot", h("tr", h("td", { colSpan: 4 }, tr("3.1(d) Inward supplies liable to reverse charge")), ...taxCells(r.total)))),
      note(
        tr(
          "A ledger or stock item is on reverse charge when its HSN/SAC starts with RCM: (set it in GST Flags). The tax owed goes to output-tax ledgers and the credit to input-tax ledgers. From an unregistered supplier you issue a self-invoice within 30 days of receipt (Rule 47A); {key} prints it. Self-invoice numbers run in the order purchases were entered, per financial year: cancel such a purchase rather than delete it.",
          { key: ltr(SELF_INVOICE_KEY) },
        ),
      ),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: SELF_INVOICE_KEY, label: "Print Self-Invoice", enabled: this.ids.some((x) => x) });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === SELF_INVOICE_KEY) {
      const id = this.ids[this.cursor];
      if (id) void gstAdvApi.printSelfInvoice(id).catch((err) => this.shell.error(err));
      else this.shell.warn(tr("This purchase needs no self-invoice: the supplier is registered and issues the invoice."));
      return true;
    }
    return super.key(e);
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }
}

// ------------------------------------------- GSTR-3B Table 4 in full

export class ItcReport extends Report {
  title = "GSTR-3B ITC";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    const g: Gstr3b = await api.gstr3b(this.from, this.to);
    this.heading(tr("GSTR-3B: reverse charge and ITC"), `${g.gstin ? ltr(g.gstin) : ""} · ${range(this.from, this.to)}`);
    this.resetPicks();
    const rcm = () => void this.shell.push(new RcmRegisterReport(this.shell, this.from, this.to));
    const r37 = () => void this.shell.push(new Rule37Screen(this.shell, this.to));
    const t31 = h("tbody", this.pick(h("tr", h("td", tr("3.1(d) Inward supplies (liable to reverse charge)")), ...taxCells(g.reverse_charge_inward ?? zero())), rcm));
    const t4 = h("tbody");
    const rows: [string, GstAmounts | undefined, (() => void) | undefined][] = [
      ["(A)(3) Inward supplies liable to reverse charge", g.itc_reverse_charge, rcm],
      ["(A)(5) All other ITC", g.itc, undefined],
      ["(B)(1) Reversed: s.17(5) blocked credit (add Rules 38, 42, 43 by hand)", g.itc_reversed_permanent, undefined],
      ["(B)(2) Reversed: others (Rule 37, suppliers unpaid after 180 days)", g.itc_reversed_other, r37],
      ["(C) Net ITC available (A) − (B)", g.itc_net, undefined],
    ];
    for (const [label, a, open] of rows) t4.append(this.pick(h("tr", h("td", tr(label)), ...taxOnly(a ?? zero())), open));
    const net = h("tbody");
    for (const r of g.net as GstNetRowRc[]) {
      const n = M.of(r.net);
      net.append(this.pick(h("tr", h("td", r.component), num(r.output), num(r.reverse_charge ?? "0"), num(r.itc), h("td.num", { class: n < 0n ? "gst-credit" : "" }, M.fmt(r.net)))));
    }
    this.bodyEl.replaceChildren(
      ...[warnings(g.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "div.gst-3b",
        h("div.tside", h("div.tside-title", tr("3.1 Outward and reverse-charge supplies")), h("table.rtable", taxHead("Nature of supplies"), t31)),
        h("div.tside", h("div.tside-title", tr("4 Eligible ITC")), h("table.rtable", head([["Details", false], ["IGST", true], ["CGST", true], ["SGST/UTGST", true], ["Cess", true]]), t4)),
        h("div.tside", h("div.tside-title", tr("Tax payable")), h("table.rtable", head([["Tax", false], ["Output tax", true], ["of which reverse charge", true], ["Net ITC", true], ["Net", true]]), net)),
      ),
      note(
        tr(
          "Blocked credit stays in 4(A) and is reversed in 4(B)(1); Rule 37 reversals go in 4(B)(2) and come back in 4(A)(5) and 4(D)(1) once paid (Circular 170/02/2022-GST). Tax on reverse charge is paid in cash: credit cannot pay it. Imports (4(A)(1), (2)), ISD credit and 4(D) are not worked out. Enter on 3.1(d) opens the register; on 4(B)(2) the Rule 37 report.",
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

// -------------------------------------------------------------- Rule 37

export class Rule37Screen extends Report {
  title = "Rule 37 ITC Reversal";
  protected override period = "asat" as const;

  constructor(shell: Shell, private asAt = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = await gstAdvApi.rule37(this.asAt);
    this.heading(tr("Rule 37: suppliers unpaid after 180 days"), tr("As at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const tbody = h("tbody");
    for (const row of r.rows) {
      tbody.append(
        this.pick(
          h(
            "tr",
            h("td", D.show(row.date)),
            h("td", ltr(row.number), h("div.muted", ltr(row.reference))),
            h("td.strong", row.party),
            num(row.value),
            num(row.unpaid),
            h("td.num", String(row.days)),
            h("td", D.show(row.reverse_from)),
            ...taxOnly(row.reversal),
          ),
          () => void this.shell.push(savedVoucherScreen(this.shell, "Purchase", row.date, row.voucher_id)),
        ),
      );
    }
    if (!r.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 11 }, tr("No bill is unpaid more than 180 days after its invoice."))));
    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "table.rtable",
        head([["Date", false], ["No. / Bill", false], ["Supplier", false], ["Value", true], ["Unpaid", true], ["Days", true], ["Reverse from", false], ["IGST", true], ["CGST", true], ["SGST/UTGST", true], ["Cess", true]]),
        tbody,
        h("tfoot", h("tr", h("td", { colSpan: 7 }, tr("ITC to reverse")), ...taxOnly(r.total))),
      ),
      note(
        tr(
          "Rule 37: ITC on a purchase (not on reverse charge) is reversed, in proportion to what is unpaid, with interest under s.50, in the GSTR-3B for the period after 180 days from the invoice; it is claimed again once you pay. Unpaid amounts come from the bill-wise details on the supplier's line. The voucher date is taken as the invoice date. GSTR-3B for that month puts these in 4(B)(2). Rule 37A (supplier did not file GSTR-3B) needs the supplier's filing status and is not worked out.",
        ),
      ),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const d = await this.dateDialog(this.asAt);
    if (!d) return;
    this.asAt = d;
    this.cursor = 0;
    await this.reload();
  }
}

/** Rule 42 calculator for one tax (a dialog, then the result). */
export async function rule42Dialog(shell: Shell): Promise<Rule42Result | null> {
  const v = await shell.dialog(
    "Rule 42 common credit (one tax)",
    [
      { label: "T: total input tax", value: "" },
      { label: "T1: non-business only", value: "0" },
      { label: "T2: exempt supplies only", value: "0" },
      { label: "T3: blocked s.17(5)", value: "0" },
      { label: "T4: taxable supplies only", value: "0" },
      { label: "E: exempt turnover", value: "" },
      { label: "F: total turnover", value: "" },
      { label: "Partly non-business use (Y/N)", value: "N" },
    ],
    (vals) => {
      for (const x of vals.slice(0, 7)) if (M.parse(x) === null) return tr('"{text}" is not an amount.', { text: x });
      return /^[YyNn]$/.test(vals[7].trim()) ? null : tr("Type Y or N.");
    },
  );
  if (!v) return null;
  try {
    const r = await gstAdvApi.rule42({
      total: v[0],
      non_business: v[1],
      exempt_only: v[2],
      blocked: v[3],
      taxable_only: v[4],
      exempt_turnover: v[5],
      total_turnover: v[6],
      non_business_use: /^[Yy]$/.test(v[7].trim()),
    });
    shell.info(
      tr("C1 {c1} · common credit C2 {c2} · D1 (exempt) {d1} · D2 (non-business) {d2} · eligible C3 {c3} · reverse D1 + D2 = {rev} in 4(B)(1).", {
        c1: M.fmt(r.c1),
        c2: M.fmt(r.c2),
        d1: M.fmt(r.d1),
        d2: M.fmt(r.d2),
        c3: M.fmt(r.c3),
        rev: M.fmt(r.reverse),
      }),
      15000,
    );
    return r;
  } catch (e) {
    shell.error(e);
    return null;
  }
}

// ------------------------------------------------------------ GST flags

export class GstFlagsScreen extends Report {
  title = "GST Flags";
  private rows: GstFlagRow[] = [];

  async load(): Promise<void> {
    this.rows = await gstAdvApi.flags();
    this.heading(tr("GST Flags: reverse charge and blocked credit"), tr("Ledgers and stock items"));
    this.resetPicks();
    const tbody = h("tbody");
    for (const r of this.rows) {
      tbody.append(
        this.pick(
          h(
            "tr",
            h("td.strong", r.name),
            h("td", tr(r.kind === "ledger" ? "Ledger" : "Stock Item")),
            h("td", r.class ? ltr(r.class) : tr("(none)")),
            h("td", r.reverse_charge ? tr("Yes") : ""),
            h("td", r.blocked ? tr("Yes") : ""),
          ),
        ),
      );
    }
    if (!this.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 5 }, tr("No ledgers or stock items yet."))));
    this.bodyEl.replaceChildren(
      h("table.rtable", head([["Name", false], ["Type", false], ["HSN/SAC", false], ["Reverse charge", false], ["Blocked credit", false]]), tbody),
      note(
        tr(
          "{rcm} turns reverse charge on or off (CGST s.9(3), 9(4): you pay the tax, not the supplier). {blocked} turns blocked credit on or off (s.17(5): motor vehicles, food, club membership and others). The flag is kept in the HSN/SAC as a prefix (RCM:, BLOCKED:), so you can also type it there. {r42} opens the Rule 42 calculator.",
          { rcm: ltr(RCM_KEY), blocked: ltr(BLOCKED_KEY), r42: ltr(RULE42_KEY) },
        ),
      ),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: RCM_KEY, label: "Reverse Charge", enabled: this.rows.length > 0 }, { chord: BLOCKED_KEY, label: "Blocked Credit", enabled: this.rows.length > 0 }, { chord: RULE42_KEY, label: "Rule 42", enabled: true });
    return b;
  }

  override key(e: KeyEv): boolean {
    const r = this.rows[this.cursor];
    if ((e.chord === RCM_KEY || e.chord === BLOCKED_KEY) && r) {
      const rcm = e.chord === RCM_KEY ? !r.reverse_charge : r.reverse_charge;
      const blocked = e.chord === BLOCKED_KEY ? !r.blocked : r.blocked;
      void gstAdvApi
        .setFlags(r.kind, r.id, rcm, blocked)
        .then(() => this.reload())
        .catch((err) => this.shell.error(err));
      return true;
    }
    if (e.chord === RULE42_KEY) {
      void rule42Dialog(this.shell);
      return true;
    }
    return super.key(e);
  }
}

// ----------------------------------------------------- GSTR-1A candidates

function snap(s: GstDocSnapshot | null): HTMLElement {
  if (!s) return h("td.muted", "—");
  return h(
    "td",
    h("div", `${ltr(s.number)} · ${D.show(s.date)}${s.cancelled ? ` · ${tr("Cancelled")}` : ""}`),
    h("div.muted", `${s.party}${s.party_gstin ? ` · ${ltr(s.party_gstin)}` : ""}`),
    h("div", `${tr("Value")} ${M.fmt(s.value)} · ${tr("Taxable")} ${M.fmt(s.taxable)} · ${tr("Tax")} ${M.fmt(s.tax)}`),
  );
}

export class Gstr1AmendmentsReport extends Report {
  title = "GSTR-1A Candidates";

  async load(): Promise<void> {
    const [filed, rows] = await Promise.all([gstAdvApi.gstr1FiledPeriods(), gstAdvApi.gstr1Amendments()]);
    const periods = filed.map((f) => ltr(f.period)).join(", ");
    this.heading(tr("GSTR-1A candidates"), filed.length ? tr("Filed periods: {list}", { list: periods }) : tr("No GSTR-1 period is marked filed yet"));
    this.resetPicks();
    const tbody = h("tbody");
    for (const r of rows as Gstr1Amendment[]) {
      const date = r.after?.date ?? r.before?.date ?? "";
      tbody.append(
        this.pick(
          h("tr", h("td", ltr(r.period)), h("td.strong", tr(r.change)), h("td", D.show(r.changed_at.slice(0, 10)), h("div.muted", r.user)), snap(r.before), snap(r.after)),
          r.after ? () => void this.shell.push(savedVoucherScreen(this.shell, r.kind, date, r.voucher_id)) : undefined,
        ),
      );
    }
    if (!rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 5 }, tr("No sales or credit notes changed after their GSTR-1 was marked filed."))));
    this.bodyEl.replaceChildren(
      h("table.rtable", head([["Period", false], ["Change", false], ["Changed on", false], ["As filed", false], ["Now", false]]), tbody),
      note(
        tr(
          "{key} marks a month's GSTR-1 as filed (as of now) on this computer. Sales and credit notes of that month added, altered, cancelled or deleted afterwards are listed from the audit trail: report them in GSTR-1A before filing GSTR-3B for the month, or as amendments in a later GSTR-1.",
          { key: ltr(MARK_FILED_KEY) },
        ),
      ),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: MARK_FILED_KEY, label: "Mark GSTR-1 Filed", enabled: true });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === MARK_FILED_KEY) {
      void this.markFiled();
      return true;
    }
    return super.key(e);
  }

  private async markFiled(): Promise<void> {
    const last = new Date();
    last.setDate(1);
    last.setMonth(last.getMonth() - 1);
    const suggestion = `${last.getFullYear()}-${String(last.getMonth() + 1).padStart(2, "0")}`;
    const v = await this.shell.dialog(
      "Mark GSTR-1 filed",
      [
        { label: "Return period (YYYY-MM)", value: suggestion },
        { label: "Filed (Y) or clear (N)", value: "Y" },
      ],
      ([p, f]) => (!/^\d{4}-\d{2}$/.test(p.trim()) ? tr('"{text}" is not a return period (YYYY-MM).', { text: p }) : /^[YyNn]$/.test(f.trim()) ? null : tr("Type Y or N.")),
    );
    if (!v) return;
    try {
      await gstAdvApi.markGstr1Filed(v[0].trim(), /^[Yy]$/.test(v[1].trim()));
      await this.reload();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

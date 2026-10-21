// TDS and TCS (India): the payable reports (deductions or collections,
// challans, what is owed and when it is due), the quarterly statement data
// for Form 140 (ex-26Q), Form 143 (ex-27EQ, TCS) and Form 144 (ex-27Q,
// non-residents) with the official statement text file, the deductor's
// details, the challans' bank details, and a salary TDS estimate. The figures
// come from the books (src-tauri/src/tds.rs) and khata-in (tds.rs, tcs.rs,
// salary.rs, tds_file.rs); docs/india-tds.md says what is covered and what to
// check in the FVU. Also small PAN / TDS / TCS editors for ledgers.
import "../gst.css";
import { api, tcsApi, tdsApi, tdsStatementApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t as tr } from "../i18n";
import * as M from "../money";
import { save } from "@tauri-apps/plugin-dialog";
import type { KeyEv, Shell } from "../shell";
import {
  DEDUCTEE_LABEL,
  KIND_LABEL,
  TCS_ALL_NATURES,
  TDS_ALL_NATURES,
  type CodeName,
  type DeducteeKind,
  type Ledger,
  type LedgerTcs,
  type LedgerTds,
  type NonResidentQuarterly,
  type NonResidentTerms,
  type SalaryEstimate,
  type SalaryEstimateInput,
  type TcsNature,
  type TcsQuarterly,
  type TdsChallanDetails,
  type TdsChallanRow,
  type TdsDeductionRow,
  type TdsDeductorSettings,
  type TdsNature,
  type TdsPayable,
  type TdsQuarterly,
  type VoucherKind,
} from "../types";
import { Report } from "./reports";
import { savedVoucherScreen } from "./stockjournal";

/** Saves the quarterly statement CSV; the same "export the filing file" key as GSTR-1's JSON. */
const EXPORT_STATEMENT = "Ctrl+Alt+J";
/** Saves the official statement text file (for the FVU). */
const EXPORT_FILE = "Ctrl+Alt+X";
/** Opens the deductor's details (TAN, responsible person). */
const DEDUCTOR_KEY = "Ctrl+Alt+D";

type Kind = "tds" | "tcs";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function monthStart(isoDate: string): string {
  return `${isoDate.slice(0, 8)}01`;
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function num(text: string | null, blankZero = true): HTMLElement {
  return h("td.num", text === null ? "" : M.fmt(text, blankZero));
}

function head(...cols: [string, boolean][]): HTMLElement {
  return h("thead", h("tr", ...cols.map(([t, n]) => h(n ? "th.num" : "th", t ? tr(t) : ""))));
}

function empty(cols: number, text: string): HTMLElement {
  return h("tr.empty", h("td", { colSpan: cols }, tr(text)));
}

function total(values: string[]): string {
  return M.toDecimal(M.sum(values.map((v) => M.of(v))));
}

function warnings(list: string[], title?: string): HTMLElement | null {
  if (!list.length) return null;
  return h(
    "div.gst-warnings",
    { role: "status" },
    h("div.gst-warn-title", title ? tr(title) : list.length === 1 ? tr("1 thing to check") : tr("{n} things to check", { n: list.length })),
    h("ul", ...list.map((w) => h("li", { dir: "auto" }, w))),
  );
}

function note(text: string): HTMLElement {
  return h("p.gst-note", tr(text));
}

function side(title: string, table: HTMLElement): HTMLElement {
  return h("div.tside", h("div.tside-title", tr(title)), table);
}

function openVoucher(shell: Shell, row: { kind: VoucherKind; date: string; voucher_id: string }): void {
  void shell.push(savedVoucherScreen(shell, row.kind, row.date, row.voucher_id));
}

function voucherLabel(r: { kind: VoucherKind; number: string }): string {
  return `${tr(KIND_LABEL[r.kind])} ${ltr(r.number)}`;
}

/** "2026-05" → "May 2026". */
function monthLabel(ym: string): string {
  // "1-May-2026" without the day, in the display language's month names.
  return D.show(`${ym}-01`).replace(/^\d+-/, "");
}

/** Overdue when the due date has passed and something is still owed. */
function dueCell(due: string, ref: string, owed: boolean): HTMLElement {
  const late = owed && due < ref;
  return h("td", { class: late ? "gst-credit" : "" }, D.show(due) + (late ? tr(" (overdue)") : ""));
}

function allNatures(kind: Kind): string {
  return kind === "tds" ? TDS_ALL_NATURES : TCS_ALL_NATURES;
}

// ------------------------------------------------------------ payable reports

const PAYABLE_TEXT: Record<Kind, { title: string; amount: string; tax: string; none: string; noLedger: string; noTax: string; noPaid: string; note: string }> = {
  tds: {
    title: "TDS Payable",
    amount: "Paid / Credited",
    tax: "TDS Deducted",
    none: "No TDS in this period. Press F2 to change it.",
    noLedger: "Mark a ledger under Duties & Taxes as TDS payable.",
    noTax: "Nothing deducted in this period.",
    noPaid: "No TDS paid in this period.",
    note:
      "Paid / credited is the amount booked to ledgers with a nature of payment for a party; TDS deducted is what was " +
      "credited to TDS payable ledgers in those vouchers; deposited is the payments out of those ledgers. Outstanding " +
      "is the closing balance of the ledgers marked for that nature. Pay-by dates are the old Rule 30's (7th of the " +
      "next month; 30 April for March), reported unchanged under the Income-tax Rules 2026 by secondary sources. Enter " +
      "on a nature lists its deductions.",
  },
  tcs: {
    title: "TCS Payable",
    amount: "Received / Debited",
    tax: "TCS Collected",
    none: "No TCS in this period. Press F2 to change it.",
    noLedger: "Mark a ledger under Duties & Taxes as TCS payable.",
    noTax: "Nothing collected in this period.",
    noPaid: "No TCS paid in this period.",
    note:
      "Received / debited is what buyers pay through ledgers with a nature of collection, GST included (the Act taxes " +
      "the amount payable by the buyer; whether GST counts is not settled). TCS collected is what was credited to TCS " +
      "payable ledgers; deposited is the payments out of those ledgers. TCS is due within seven days from the end of " +
      "the month (old Rule 37CA; the 2026 rules' date is from secondary sources). Enter on a nature lists its rows.",
  },
};

export class TdsPayableReport extends Report {
  title = "TDS Payable";
  protected override period = "range" as const;
  protected kind: Kind = "tds";

  constructor(shell: Shell, protected from = monthStart(today(shell)), protected to = today(shell)) {
    super(shell);
  }

  protected fetch(): Promise<TdsPayable> {
    return this.kind === "tds" ? tdsApi.payable(this.from, this.to) : tcsApi.payable(this.from, this.to);
  }

  async load(): Promise<void> {
    this.render(await this.fetch());
  }

  private render(r: TdsPayable): void {
    const text = PAYABLE_TEXT[this.kind];
    const all = allNatures(this.kind);
    this.heading(tr(text.title), `${this.shell.company?.name ?? ""} · ${range(this.from, this.to)}`);
    this.resetPicks();
    const ref = today(this.shell);

    const natures = h("tbody");
    for (const n of r.natures) {
      const open = () => void this.shell.push(new TdsNatureReport(this.shell, this.kind, n.nature, n.name, this.from, this.to));
      natures.append(
        this.pick(
          h("tr", h("td.strong", n.name), h("td", n.section), num(n.paid_credited), num(n.deducted), num(n.deposited), num(n.outstanding)),
          n.nature === all ? undefined : open,
        ),
      );
    }
    if (!r.natures.length) natures.append(empty(6, text.none));

    const holds = (code: string) => (code === all ? tr("All natures") : (r.natures.find((n) => n.nature === code)?.name ?? code));
    const ledgers = h("tbody");
    for (const l of r.ledgers) {
      const owed = M.of(l.closing);
      ledgers.append(
        this.pick(
          h("tr", h("td.strong", l.name), h("td", holds(l.payable_for)), num(l.opening), h("td.num", { class: owed < 0n ? "gst-credit" : "" }, M.fmt(l.closing, true))),
        ),
      );
    }
    if (!r.ledgers.length) ledgers.append(empty(4, text.noLedger));

    const dues = h("tbody");
    for (const n of r.natures) {
      const owed = n.outstanding === null || M.of(n.outstanding) > 0n;
      for (const m of n.by_month) {
        dues.append(this.pick(h("tr", h("td", n.name), h("td", monthLabel(m.month)), num(m.deducted), dueCell(m.due, ref, owed))));
      }
    }
    if (!dues.children.length) dues.append(empty(4, text.noTax));

    const challans = h("tbody");
    for (const c of r.challans) {
      challans.append(
        this.pick(
          h("tr", h("td", D.show(c.date)), h("td", voucherLabel(c)), h("td.strong", c.ledger_name), h("td", holds(c.nature)), num(c.amount)),
          () => openVoucher(this.shell, c),
        ),
      );
    }
    if (!r.challans.length) challans.append(empty(5, text.noPaid));

    this.bodyEl.replaceChildren(
      ...[warnings(r.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "div.gst-3b",
        side(
          "By nature",
          h(
            "table.rtable",
            head(["Nature", false], ["Section", false], [text.amount, true], [text.tax, true], ["Deposited", true], ["Outstanding", true]),
            natures,
            h("tfoot", h("tr", h("td", { colSpan: 2 }, tr("Total")), num(r.paid_credited, false), num(r.deducted, false), num(r.deposited, false), h("td.num", ""))),
          ),
        ),
        side("Due dates", h("table.rtable", head(["Nature", false], ["Month", false], [this.kind === "tds" ? "TDS" : "TCS", true], ["Pay by", false]), dues)),
        side("Challan payments", h("table.rtable", head(["Date", false], ["Voucher", false], ["Ledger", false], ["Nature", false], ["Amount", true]), challans)),
        side(this.kind === "tds" ? "TDS payable ledgers" : "TCS payable ledgers", h("table.rtable", head(["Ledger", false], ["Holds", false], ["Opening", true], ["Closing", true]), ledgers)),
      ),
      note(text.note),
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

/** TCS collected, challans and due dates in a period. */
export class TcsPayableReport extends TdsPayableReport {
  override title = "TCS Payable";
  protected override kind: Kind = "tcs";
}

/** One nature's rows in the period; Enter opens the voucher. */
class TdsNatureReport extends Report {
  protected override period = "range" as const;

  constructor(
    shell: Shell,
    private kind: Kind,
    private nature: string,
    public title: string,
    private from: string,
    private to: string,
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = this.kind === "tds" ? await tdsApi.payable(this.from, this.to) : await tcsApi.payable(this.from, this.to);
    const rows = r.rows.filter((x) => x.nature === this.nature);
    const section = r.natures.find((n) => n.nature === this.nature)?.section ?? "";
    this.heading(`${this.kind.toUpperCase()} · ${this.title}`, `${ltr(section)} · ${range(this.from, this.to)}`);
    this.resetPicks();
    const ref = today(this.shell);
    const tbody = h("tbody");
    for (const x of rows) tbody.append(this.row(x, ref));
    if (!rows.length) tbody.append(empty(8, "Nothing for this nature in the period."));
    const text = PAYABLE_TEXT[this.kind];
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        head(["Date", false], ["Voucher", false], ["Party", false], ["PAN", false], [text.amount, true], [this.kind === "tds" ? "TDS" : "TCS", true], ["Rate", true], ["Pay by", false]),
        tbody,
        h(
          "tfoot",
          h("tr", h("td", { colSpan: 4 }, tr("Total")), num(total(rows.map((x) => x.amount)), false), num(total(rows.map((x) => x.tds)), false), h("td.num", ""), h("td", "")),
        ),
      ),
    );
  }

  private row(x: TdsDeductionRow, ref: string): HTMLElement {
    const amount = M.of(x.amount);
    const tds = M.of(x.tds);
    const rate = amount > 0n && tds !== 0n ? `${(Number((tds * 10000n) / amount) / 100).toFixed(2)}%` : "";
    return this.pick(
      h(
        "tr",
        h("td", D.show(x.date)),
        h("td", voucherLabel(x)),
        h("td.strong", x.party_name),
        h("td", { class: x.pan ? "" : "warnrow" }, x.pan ?? tr("No PAN")),
        num(x.amount),
        num(x.tds),
        h("td.num", rate),
        tds === 0n ? h("td", "") : dueCell(x.deposit_due, ref, false),
      ),
      () => openVoucher(this.shell, x),
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

// ------------------------------------------------------- quarterly statements

/** The financial year start (2026 for 2026-27) and quarter (1 to 4) containing a date. */
function quarterOf(isoDate: string): [number, number] {
  const y = Number(isoDate.slice(0, 4));
  const m = Number(isoDate.slice(5, 7));
  return [m >= 4 ? y : y - 1, m >= 4 ? Math.floor((m - 4) / 3) + 1 : 4];
}

function yearLabel(start: number): string {
  return `${start}-${String((start + 1) % 100).padStart(2, "0")}`;
}

function wholeRupees(v: string): boolean {
  if (!v.trim()) return true;
  const u = M.parse(v);
  return u !== null && u >= 0n && M.toDecimal(u).indexOf(".") < 0;
}

/** Enter or change a challan voucher's bank details (BSR code, serial, deposit date). */
export async function editChallanDetails(shell: Shell, c: TdsChallanRow): Promise<boolean> {
  const cur = await tdsStatementApi.challan(c.voucher_id);
  const ref = today(shell);
  const v = await shell.dialog(
    tr("Challan · {voucher} · {amount}", { voucher: voucherLabel(c), amount: M.fmt(c.amount) }),
    [
      { label: "BSR code (7 digits)", value: cur?.bsr ?? "" },
      { label: "Challan serial no.", value: cur?.serial ?? "" },
      { label: "Date of deposit (bank)", value: cur?.date ? D.editable(cur.date) : D.editable(c.date) },
      { label: "Of which interest", value: cur?.interest ?? "" },
      { label: "Of which fee", value: cur?.fee ?? "" },
      { label: "Of which others / penalty", value: cur?.others ?? "" },
      { label: "Minor head (200 or 400)", value: cur?.minor_head ?? "200" },
      { label: "Clear these details (y/n)", value: "n" },
    ],
    ([bsr, serial, date, interest, fee, others, minor, clear]) => {
      if (clear.trim().toLowerCase().startsWith("y")) return null;
      if (!/^\d{7}$/.test(bsr.trim())) return "The BSR code has 7 digits.";
      if (!/^\d{1,5}$/.test(serial.trim())) return "The challan serial number is 1 to 5 digits.";
      if (date.trim() && !D.parseUser(date, ref)) return tr('"{text}" is not a date.', { text: ltr(date) });
      for (const x of [interest, fee, others]) if (!wholeRupees(x)) return tr('"{text}" is not whole rupees.', { text: ltr(x) });
      if (!["200", "400"].includes(minor.trim())) return "The minor head is 200 (TDS payable by taxpayer) or 400.";
      return null;
    },
  );
  if (!v) return false;
  const [bsr, serial, date, interest, fee, others, minor, clear] = v;
  const details: TdsChallanDetails | null = clear.trim().toLowerCase().startsWith("y")
    ? null
    : {
        bsr: bsr.trim(),
        serial: serial.trim(),
        date: date.trim() ? (D.parseUser(date, ref) ?? "") : "",
        interest: interest.trim(),
        fee: fee.trim(),
        others: others.trim(),
        minor_head: minor.trim(),
      };
  try {
    await tdsStatementApi.setChallan(c.voucher_id, details);
    shell.info(details ? "Challan details saved." : "Challan details cleared.");
    return true;
  } catch (e) {
    shell.error(e);
    return false;
  }
}

/** Shared by the three quarterly reports: the quarter, the file export and the deductor key. */
abstract class QuarterReport extends Report {
  protected override period = "range" as const;
  protected year: number;
  protected quarter: number;
  protected abstract form: "140" | "143" | "144";
  /** "Form 140" etc., once loaded. */
  protected formName = "";

  constructor(shell: Shell, year?: number, quarter?: number) {
    super(shell);
    const [y, q] = quarterOf(today(shell));
    this.year = year ?? y;
    this.quarter = quarter ?? q;
  }

  protected override async askPeriod(): Promise<void> {
    const v = await this.shell.dialog(
      "Change quarter",
      [
        { label: "Financial year", value: yearLabel(this.year) },
        { label: "Quarter (1-4)", value: String(this.quarter) },
      ],
      ([y, q]) => {
        if (!/^\s*\d{4}(-\d{2})?\s*$/.test(y)) return tr('"{text}" is not a financial year (e.g. 2026-27).', { text: ltr(y) });
        if (!/^\s*[1-4]\s*$/.test(q)) return "The quarter is 1 (Apr-Jun) to 4 (Jan-Mar).";
        return null;
      },
    );
    if (!v) return;
    this.year = Number(v[0].trim().slice(0, 4));
    this.quarter = Number(v[1].trim());
    this.cursor = 0;
    await this.reload();
  }

  protected challanTable(challans: TdsChallanRow[]): HTMLElement {
    const body = h("tbody");
    for (const c of challans) {
      body.append(
        this.pick(h("tr", h("td", D.show(c.date)), h("td", voucherLabel(c)), h("td.strong", c.ledger_name), num(c.amount)), () => {
          void editChallanDetails(this.shell, c).then((saved) => (saved ? this.reload() : undefined));
        }),
      );
    }
    if (!challans.length) body.append(empty(4, "No challan paid in this quarter."));
    return side(
      "Challan payments (Enter: bank details for the statement file)",
      h("table.rtable", head(["Date", false], ["Voucher", false], ["Ledger", false], ["Amount", true]), body),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(
      b.length - 1,
      0,
      { chord: EXPORT_FILE, label: "Statement file", enabled: this.year >= 2026 },
      { chord: DEDUCTOR_KEY, label: "Deductor", enabled: true },
    );
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === EXPORT_FILE) {
      void this.exportFile();
      return true;
    }
    if (e.chord === DEDUCTOR_KEY) {
      void editTdsDeductor(this.shell).then((saved) => (saved ? this.reload() : undefined));
      return true;
    }
    return super.key(e);
  }

  private async exportFile(): Promise<void> {
    try {
      const token = await this.shell.dialog(
        tr("{form} statement file · Q{q} {year}", { form: ltr(this.formName || `Form ${this.form}`), q: this.quarter, year: ltr(yearLabel(this.year)) }),
        [{ label: "Token no. of the previous regular statement (blank if none)", value: "" }],
        ([t]) => (t.trim() === "" || /^\d{15}$/.test(t.trim()) ? null : "The token number has 15 digits."),
      );
      if (!token) return;
      const f = await tdsStatementApi.file(this.form, this.year, this.quarter, token[0].trim() || null);
      if (!f.text) {
        await this.shell.push(new StatementProblems(this.shell, f.form, f.problems, f.notes));
        return;
      }
      const path = await save({ defaultPath: f.file_name, filters: [{ name: "Text", extensions: ["txt"] }] });
      if (!path) return;
      const file = path.toLowerCase().endsWith(".txt") ? path : `${path}.txt`;
      await tdsStatementApi.writeFile(file, f.text);
      this.shell.info(tr("{form} saved to {file}. Validate it with the official FVU before filing.", { form: ltr(f.form), file: ltr(file) }), 9000);
    } catch (e) {
      this.shell.error(e);
    }
  }
}

/** Why a statement file could not be made. */
class StatementProblems extends Report {
  title = "Statement file";

  constructor(
    shell: Shell,
    private form: string,
    private problems: string[],
    private notes: string[],
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    this.heading(tr("{form}: the file cannot be made yet", { form: ltr(this.form) }), tr("{n} to fix", { n: this.problems.length }));
    this.resetPicks();
    this.bodyEl.replaceChildren(
      ...[warnings(this.problems, "Fix these first")].filter((x): x is HTMLElement => x !== null),
      h(
        "p.gst-note",
        tr("Deductor details: {key} on the quarterly report. Challan bank details: Enter on a challan there. PANs and party details: the ledger.", { key: ltr(DEDUCTOR_KEY) }),
      ),
      h("ul.gst-note", ...this.notes.map((n) => h("li", n))),
    );
  }
}

export class TdsQuarterlyReport extends QuarterReport {
  title = "TDS Quarterly Statement";
  protected form = "140" as const;
  private data: TdsQuarterly | null = null;

  async load(): Promise<void> {
    const d = await tdsApi.quarterly(this.year, this.quarter);
    this.data = d;
    this.formName = d.form;
    const due = d.due ? tr("due {date}", { date: D.show(d.due) }) : tr("due date not known");
    this.heading(tr("{form} data · Q{q} {year}", { form: ltr(d.form), q: d.quarter, year: ltr(d.year) }), `${d.deductor_name}${d.deductor_pan ? ` · ${tr("PAN {pan}", { pan: d.deductor_pan })}` : ""} · ${range(d.from, d.to)} · ${due}`);
    this.resetPicks();
    const rows = h("tbody");
    for (const r of d.rows) {
      rows.append(
        this.pick(
          h(
            "tr",
            h("td.num", String(r.serial)),
            h("td.strong", r.party_name, r.deductee ? h("span.muted", ` · ${tr(DEDUCTEE_LABEL[r.deductee])}`) : null),
            h("td", { class: r.pan ? "" : "warnrow" }, r.pan ?? tr("No PAN")),
            h("td", r.section),
            h("td", r.tax_code ?? ""),
            h("td", D.show(r.date)),
            num(r.amount),
            num(r.tds, false),
            h("td.num", `${r.rate}%`),
            h("td", r.reason ?? ""),
          ),
          () => openVoucher(this.shell, r),
        ),
      );
    }
    if (!d.rows.length) rows.append(empty(10, "No deductions in this quarter. Press F2 to pick another."));
    this.bodyEl.replaceChildren(
      ...[warnings(d.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "table.rtable",
        head(["Sr", true], ["Deductee", false], ["PAN", false], ["Section", false], ["Code", false], ["Date", false], ["Paid / Credited", true], ["TDS", true], ["Rate", true], ["Reason", false]),
        rows,
        h("tfoot", h("tr", h("td", { colSpan: 6 }, tr("Total")), num(d.amount, false), num(d.tds, false), h("td.num", ""), h("td", ""))),
      ),
      this.challanTable(d.challans),
      h(
        "div.gst-note",
        h("div", tr("{csv} saves this data as CSV; {file} saves the statement text file (2026-27 onward). Notes:", { csv: ltr(EXPORT_STATEMENT), file: ltr(EXPORT_FILE) })),
        h("ul", ...d.gaps.map((g) => h("li", g))),
      ),
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: EXPORT_STATEMENT, label: "Statement CSV", enabled: true });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === EXPORT_STATEMENT) {
      void this.exportStatement();
      return true;
    }
    return super.key(e);
  }

  private async exportStatement(): Promise<void> {
    try {
      const csv = await tdsApi.quarterlyCsv(this.year, this.quarter);
      const name = `${(this.data?.form ?? "TDS").replace(/\s+/g, "")}-Q${this.quarter}-${yearLabel(this.year)}`;
      const path = await save({ defaultPath: `${name}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      const file = path.toLowerCase().endsWith(".csv") ? path : `${path}.csv`;
      await api.writeCsv(file, csv);
      this.shell.info(tr("Statement data saved to {file}. For filing, save the statement file ({key}) and validate it with the FVU.", { file: ltr(file), key: ltr(EXPORT_FILE) }));
    } catch (e) {
      this.shell.error(e);
    }
  }
}

const REMARK_LABEL: Record<string, string> = { A: "Certificate", B: "Declaration", C: "No PAN" };

/** TCS statement data (Form 143, ex-27EQ). */
export class TcsQuarterlyReport extends QuarterReport {
  title = "TCS Quarterly Statement";
  protected form = "143" as const;

  async load(): Promise<void> {
    const d: TcsQuarterly = await tcsApi.quarterly(this.year, this.quarter);
    this.formName = d.form;
    const due = d.due ? tr("due {date}", { date: D.show(d.due) }) : tr("due date not known");
    this.heading(tr("{form} data · Q{q} {year}", { form: ltr(d.form), q: d.quarter, year: ltr(d.year) }), `${d.deductor_name}${d.deductor_pan ? ` · ${tr("PAN {pan}", { pan: d.deductor_pan })}` : ""} · ${range(d.from, d.to)} · ${due}`);
    this.resetPicks();
    const rows = h("tbody");
    for (const r of d.rows) {
      rows.append(
        this.pick(
          h(
            "tr",
            h("td.num", String(r.serial)),
            h("td.strong", r.party_name, r.collectee ? h("span.muted", ` · ${tr(DEDUCTEE_LABEL[r.collectee])}`) : null, r.non_resident ? h("span.muted", ` · ${tr("non-resident")}`) : null),
            h("td", { class: r.pan ? "" : "warnrow" }, r.pan ?? tr("No PAN")),
            h("td", `${r.tax_code} ${r.nature_name}`),
            h("td", D.show(r.date)),
            num(r.amount),
            num(r.tcs, false),
            h("td.num", `${r.rate}%`),
            h("td", r.remark ? `${r.remark}: ${REMARK_LABEL[r.remark] ? tr(REMARK_LABEL[r.remark]) : (r.reason ?? "")}` : ""),
          ),
          () => openVoucher(this.shell, r),
        ),
      );
    }
    if (!d.rows.length) rows.append(empty(9, "No collections in this quarter. Press F2 to pick another."));
    this.bodyEl.replaceChildren(
      ...[warnings(d.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "table.rtable",
        head(["Sr", true], ["Collectee", false], ["PAN", false], ["Code · nature", false], ["Date", false], ["Received / Debited", true], ["TCS", true], ["Rate", true], ["Flag", false]),
        rows,
        h("tfoot", h("tr", h("td", { colSpan: 5 }, tr("Total")), num(d.amount, false), num(d.tcs, false), h("td.num", ""), h("td", ""))),
      ),
      this.challanTable(d.challans),
      h(
        "p.gst-note",
        tr(
          "{file} saves the Form 143 text file once the deductor's details ({deductor}) and the challans' bank details are in. Rates are the Income-tax Act 2025 s.394 Table as amended by the Finance Act 2026 (2% for liquor, tendu leaves, timber, forest produce, scrap and minerals; 1% for a motor vehicle over ₹10 lakh). Validate the file with the official FVU.",
          { file: ltr(EXPORT_FILE), deductor: ltr(DEDUCTOR_KEY) },
        ),
      ),
    );
  }
}

/** Payments to non-residents (Form 144, ex-27Q). */
export class TdsNonResidentReport extends QuarterReport {
  title = "TDS Non-resident Statement";
  protected form = "144" as const;

  async load(): Promise<void> {
    const d: NonResidentQuarterly = await tdsStatementApi.nonResident(this.year, this.quarter);
    this.formName = d.form;
    const due = d.due ? tr("due {date}", { date: D.show(d.due) }) : tr("due date not known");
    this.heading(tr("{form} data · Q{q} {year}", { form: ltr(d.form), q: d.quarter, year: ltr(d.year) }), `${range(d.from, d.to)} · ${due}`);
    this.resetPicks();
    const rows = h("tbody");
    for (const r of d.rows) {
      rows.append(
        this.pick(
          h(
            "tr",
            h("td.num", String(r.serial)),
            h("td.strong", r.party_name),
            h("td", { class: r.pan ? "" : "warnrow" }, r.pan ?? tr("No PAN")),
            h("td", { class: r.country ? "" : "warnrow" }, r.country_name ?? tr("No country")),
            h("td", r.remittance ?? ""),
            h("td", D.show(r.date)),
            num(r.amount),
            num(r.tds, false),
            h("td.num", `${r.rate}%`),
            h("td", r.treaty ? tr("Treaty") : tr("Act")),
          ),
          () => openVoucher(this.shell, r),
        ),
      );
    }
    if (!d.rows.length) rows.append(empty(10, "No payments to non-residents in this quarter."));
    this.bodyEl.replaceChildren(
      ...[warnings(d.reminders, "Rates are yours")].filter((x): x is HTMLElement => x !== null),
      ...[warnings(d.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "table.rtable",
        head(["Sr", true], ["Deductee", false], ["PAN", false], ["Country", false], ["Remittance", false], ["Date", false], ["Paid / Credited", true], ["TDS", true], ["Rate", true], ["Basis", false]),
        rows,
        h("tfoot", h("tr", h("td", { colSpan: 6 }, tr("Total")), num(d.amount, false), num(d.tds, false), h("td.num", ""), h("td", ""))),
      ),
      this.challanTable(d.challans),
      note("Enter the rate, country and nature of remittance in the party ledger's Non-resident terms (alter the ledger; they show for a party abroad). Below 20% the file needs the payee's foreign address, email, phone and tax number."),
    );
  }
}

// ------------------------------------------------------- deductor details

/**
 * Edit the deductor's TAN, address and the responsible person, in two field
 * dialogs. Blank address, state, PIN, email and phone of the responsible
 * person fall back to the deductor's. Returns true when saved.
 */
export async function editTdsDeductor(shell: Shell): Promise<boolean> {
  try {
    const cur = await tdsStatementApi.deductor();
    const s = cur.stored;
    const types = cur.deductor_types.map((t) => `${t.code} ${t.name}`).join(", ");
    const a = await shell.dialog(
      "Deductor (TDS / TCS statements)",
      [
        { label: "TAN", value: s.tan },
        { label: "PAN (blank: from the GSTIN)", value: s.pan },
        { label: tr("Deductor type ({types})", { types }), value: s.deductor_type },
        { label: "Address (blank: the company's)", value: s.address },
        { label: "State (GST code, blank: the company's)", value: s.state },
        { label: "PIN", value: s.pin },
        { label: "Email", value: s.email },
        { label: "Phone", value: s.phone },
      ],
      ([tan, pan, type, , state, pin, email, phone]) => {
        if (tan.trim() && !/^[A-Za-z]{4}\d{5}[A-Za-z]$/.test(tan.trim())) return "A TAN is 4 letters, 5 digits and a letter.";
        if (pan.trim() && !/^[A-Za-z]{5}\d{4}[A-Za-z]$/.test(pan.trim()) && pan.trim().toUpperCase() !== "PANNOTREQD") return "A PAN is 5 letters, 4 digits and a letter.";
        if (type.trim() && !cur.deductor_types.some((t) => t.code === type.trim().toUpperCase())) return tr("Pick a deductor type: {types}.", { types });
        if (state.trim() && !/^\d{2}$/.test(state.trim())) return "The state is the 2-digit GST state code.";
        if (pin.trim() && !/^\d{6}$/.test(pin.trim())) return "The PIN has 6 digits.";
        if (email.trim() && !/^[^@\s^]+@[^@\s^]+\.[^@\s^]+$/.test(email.trim())) return "That is not an email address.";
        if (phone.trim() && !/^\d{6,10}$/.test(phone.trim())) return "The phone is 6 to 10 digits, without the country code.";
        return null;
      },
    );
    if (!a) return false;
    const b = await shell.dialog(
      "Person responsible for deduction",
      [
        { label: "Name", value: s.responsible_name },
        { label: "Designation (up to 20)", value: s.responsible_designation },
        { label: "PAN", value: s.responsible_pan },
        { label: "Address (blank: the deductor's)", value: s.responsible_address },
        { label: "State (GST code, blank: the deductor's)", value: s.responsible_state },
        { label: "PIN (blank: the deductor's)", value: s.responsible_pin },
        { label: "Email (blank: the deductor's)", value: s.responsible_email },
        { label: "Phone (blank: the deductor's)", value: s.responsible_phone },
      ],
      ([, designation, pan, , state, pin, , phone]) => {
        if (designation.trim().length > 20) return "The designation is at most 20 characters.";
        if (pan.trim() && !/^[A-Za-z]{5}\d{4}[A-Za-z]$/.test(pan.trim())) return "A PAN is 5 letters, 4 digits and a letter.";
        if (state.trim() && !/^\d{2}$/.test(state.trim())) return "The state is the 2-digit GST state code.";
        if (pin.trim() && !/^\d{6}$/.test(pin.trim())) return "The PIN has 6 digits.";
        if (phone.trim() && !/^\d{6,10}$/.test(phone.trim())) return "The phone is 6 to 10 digits.";
        return null;
      },
    );
    if (!b) return false;
    const settings: TdsDeductorSettings = {
      ...s,
      tan: a[0].trim().toUpperCase(),
      pan: a[1].trim().toUpperCase(),
      deductor_type: a[2].trim().toUpperCase(),
      address: a[3].trim(),
      state: a[4].trim(),
      pin: a[5].trim(),
      email: a[6].trim(),
      phone: a[7].trim(),
      responsible_name: b[0].trim(),
      responsible_designation: b[1].trim(),
      responsible_pan: b[2].trim().toUpperCase(),
      responsible_address: b[3].trim(),
      responsible_state: b[4].trim(),
      responsible_pin: b[5].trim(),
      responsible_email: b[6].trim(),
      responsible_phone: b[7].trim(),
    };
    const saved = await tdsStatementApi.setDeductor(settings);
    if (saved.problems.length) shell.info(tr("Deductor saved. Still needed for the statement file: {list}", { list: ltr(saved.problems.join("; ")) }), 12000);
    else shell.info("Deductor saved.");
    return true;
  } catch (e) {
    shell.error(e);
    return false;
  }
}

// ------------------------------------------------------- salary TDS estimate

/** The new-regime salary TDS estimate (s.392, s.202(1)); F2 changes the inputs. Not payroll. */
export class SalaryTdsEstimateReport extends Report {
  title = "Salary TDS Estimate";
  protected override period = "range" as const;
  private input: SalaryEstimateInput = {
    tax_year: 0,
    salary: "",
    other_salary: "",
    other_income: "",
    tds_so_far: "",
    months_remaining: 0,
    non_resident: false,
  };
  private asked = false;

  async load(): Promise<void> {
    if (!this.asked) {
      this.asked = true;
      this.heading(tr("Salary TDS estimate"), tr("Press F2 to enter the salary"));
      this.bodyEl.replaceChildren(note("An estimate for one employee under the new tax regime. Press F2 to enter the year's salary."));
      // Ask once the screen is showing, rather than over the screen it was opened from.
      window.setTimeout(() => void this.askPeriod(), 0);
      return;
    }
    const e = await tdsStatementApi.salaryEstimate(this.input);
    this.render(e);
  }

  private render(e: SalaryEstimate): void {
    this.heading(tr("Salary TDS estimate"), `${tr(e.regime)} · ${tr(e.months_remaining === 1 ? "{n} month left" : "{n} months left", { n: e.months_remaining })}`);
    this.resetPicks();
    const line = (label: string, value: string, strong = false) => this.pick(h("tr", h(strong ? "td.strong" : "td", tr(label)), num(value, false)));
    const slabs = h("tbody");
    for (const s of e.slabs) {
      const band = s.to === null ? tr("Above {amount}", { amount: `₹${M.fmt(s.from)}` }) : tr("{from} to {to}", { from: `₹${M.fmt(s.from, false)}`, to: `₹${M.fmt(s.to)}` });
      slabs.append(this.pick(h("tr", h("td", band), h("td.num", `${s.rate}%`), num(s.income, false), num(s.tax, false))));
    }
    this.bodyEl.replaceChildren(
      ...[warnings(e.surcharge_left_out ? ["Surcharge is not included (total income over ₹50 lakh): this tax is too low."] : [])].filter((x): x is HTMLElement => x !== null),
      h(
        "div.gst-3b",
        side(
          "For the year",
          h(
            "table.rtable",
            head(["", false], ["Amount", true]),
            h(
              "tbody",
              line("Gross salary and declared income", e.gross),
              line("Less standard deduction (s.19)", e.standard_deduction),
              line("Total income (rounded to ₹10)", e.total_income, true),
              line("Tax at the s.202(1) slabs", e.tax_on_income),
              line("Less rebate (s.156(2))", e.rebate),
              line("Health and Education Cess 4%", e.cess),
              line("Tax for the year (rounded to ₹10)", e.annual_tax, true),
              line("Deducted so far", e.tds_so_far),
              line("Still to deduct", e.remaining),
              line(tr(e.months_remaining === 1 ? "Each month, for {n} month" : "Each month, for {n} months", { n: e.months_remaining }), e.monthly, true),
            ),
          ),
        ),
        side("Slabs", h("table.rtable", head(["Income band", false], ["Rate", true], ["Income", true], ["Tax", true]), slabs)),
      ),
      h("ul.gst-note", ...e.notes.map((n) => h("li", { dir: "auto" }, n))),
    );
  }

  private async ask(): Promise<boolean> {
    const i = this.input;
    const ref = today(this.shell);
    const [fy] = quarterOf(ref);
    const v = await this.shell.dialog(
      "Salary TDS estimate (new regime)",
      [
        { label: "Tax year", value: yearLabel(i.tax_year || fy) },
        { label: "Salary from you for the year", value: i.salary },
        { label: "Salary from earlier employers", value: i.other_salary },
        { label: "Other income declared", value: i.other_income },
        { label: "TDS deducted so far this year", value: i.tds_so_far },
        { label: "Months left, this month included (blank: from today)", value: i.months_remaining ? String(i.months_remaining) : "" },
        { label: "Resident in India (y/n)", value: i.non_resident ? "n" : "y" },
      ],
      ([y, salary, other, income, tds, months, resident]) => {
        if (!/^\s*\d{4}(-\d{2})?\s*$/.test(y)) return tr('"{text}" is not a tax year (e.g. 2026-27).', { text: ltr(y) });
        for (const x of [salary, other, income, tds]) if (x.trim() && M.parse(x) === null) return tr('"{text}" is not an amount.', { text: ltr(x) });
        if (!salary.trim()) return "Enter the salary for the year.";
        if (months.trim() && !/^\s*([1-9]|1[0-2])\s*$/.test(months)) return "Months left is 1 to 12.";
        if (!/^\s*[yn]/i.test(resident)) return "Answer y or n.";
        return null;
      },
    );
    if (!v) return false;
    const amount = (x: string) => (x.trim() ? M.toDecimal(M.parse(x)!) : "");
    this.input = {
      tax_year: Number(v[0].trim().slice(0, 4)),
      salary: amount(v[1]),
      other_salary: amount(v[2]),
      other_income: amount(v[3]),
      tds_so_far: amount(v[4]),
      months_remaining: v[5].trim() ? Number(v[5].trim()) : 0,
      non_resident: /^\s*n/i.test(v[6]),
    };
    return true;
  }

  override bottom() {
    return super.bottom().map((b) => (b && b.label === "Period" ? { ...b, label: "Inputs" } : b));
  }

  protected override async askPeriod(): Promise<void> {
    if (await this.ask()) await this.reload();
  }
}

// ------------------------------------------------------- ledger settings

const DEDUCTEE_KEYS = Object.keys(DEDUCTEE_LABEL) as DeducteeKind[];

/** A deductee type from what the user typed: the key, the label, or its start ("huf", "firm"). */
function deducteeOf(text: string): DeducteeKind | null | undefined {
  const t = text.trim().toLowerCase();
  if (!t) return null;
  return DEDUCTEE_KEYS.find((k) => k.toLowerCase() === t || DEDUCTEE_LABEL[k].toLowerCase().startsWith(t));
}

function natureOf(natures: TdsNature[], text: string): TdsNature | null | undefined {
  const t = text.trim().toLowerCase();
  if (!t) return null;
  return natures.find((n) => n.code === t || n.old_section.toLowerCase() === t || n.name.toLowerCase().startsWith(t));
}

function tcsNatureOf(natures: TcsNature[], text: string): TcsNature | null | undefined {
  const t = text.trim().toLowerCase();
  if (!t) return null;
  return natures.find((n) => n.code === t || n.tax_code === t || n.name.toLowerCase().startsWith(t) || n.name.toLowerCase().includes(t));
}

function codeOf(list: CodeName[], text: string): CodeName | null | undefined {
  const t = text.trim().toLowerCase();
  if (!t) return null;
  return list.find((c) => c.code === t.padStart(2, "0") || c.code === t) ?? list.find((c) => c.name.toLowerCase().startsWith(t));
}

function yes(text: string): boolean {
  return /^\s*y/i.test(text);
}

/** One line about a ledger's TDS settings, for a ledger form or list ("194C contractors · Firm · PAN …"). */
export function tdsSummary(ledger: Ledger, natures: TdsNature[]): string {
  const parts: string[] = [];
  const t = ledger.tds;
  if (t?.kind === "Payable") {
    const nature = t.payable_for === TDS_ALL_NATURES ? tr("all natures") : (natures.find((n) => n.code === t.payable_for)?.name ?? t.payable_for);
    parts.push(tr("TDS payable: {nature}", { nature }));
  } else if (t?.kind === "Deductible") {
    const n = natures.find((x) => x.code === t.nature);
    if (n) parts.push(`${n.old_section} ${n.name}`);
    if (t.deductee) parts.push(tr(DEDUCTEE_LABEL[t.deductee]));
    if (t.lower_rate != null) parts.push(tr("lower rate {rate}%", { rate: M.toDecimal(M.of(t.lower_rate)) }) + (t.certificate ? ` (${t.certificate})` : ""));
    if (t.non_resident) parts.push(tr("non-resident at {rate}%", { rate: M.toDecimal(M.of(t.non_resident.rate)) }));
  }
  if (ledger.pan) parts.push(tr("PAN {pan}", { pan: ledger.pan }));
  return parts.join(" · ");
}

/**
 * Edit a ledger's PAN and TDS settings in a field dialog. A ledger under
 * Duties & Taxes (or already a TDS payable ledger) is asked which nature it
 * holds; any other is asked for its nature of payment, deductee type and a
 * lower-deduction certificate. Natures can be typed as a code, an old
 * section ("194C") or the start of the name. Non-resident terms already on
 * the ledger are kept (edit them with editLedgerNonResident). Returns the
 * saved ledger, or null when cancelled.
 */
export async function editLedgerTds(shell: Shell, ledger: Ledger): Promise<Ledger | null> {
  const natures = await tdsApi.natures();
  const payable = ledger.tds?.kind === "Payable" || (ledger.tds === null && ledger.group_name === "Duties & Taxes");
  const ref = today(shell);
  if (payable) {
    const current = ledger.tds?.kind === "Payable" ? ledger.tds.payable_for : "";
    const v = await shell.dialog(
      `TDS payable · ${ledger.name}`,
      [{ label: `Holds TDS for (nature, "${TDS_ALL_NATURES}", or empty for none)`, value: current }],
      ([n]) => (n.trim() === "" || n.trim().toLowerCase() === TDS_ALL_NATURES || natureOf(natures, n) ? null : `"${n}" is not a nature of payment.`),
    );
    if (!v) return null;
    const n = v[0].trim().toLowerCase();
    const tds: LedgerTds | null = n === "" ? null : { kind: "Payable", payable_for: n === TDS_ALL_NATURES ? n : natureOf(natures, n)!.code };
    return save_(shell, ledger, ledger.pan, tds);
  }
  const t = ledger.tds?.kind === "Deductible" ? ledger.tds : null;
  const v = await shell.dialog(
    `PAN and TDS · ${ledger.name}`,
    [
      { label: "PAN", value: ledger.pan ?? "" },
      { label: "Nature of payment", value: t?.nature ?? "" },
      { label: "Deductee type", value: t?.deductee ? DEDUCTEE_LABEL[t.deductee] : "" },
      { label: "Lower deduction rate %", value: t?.lower_rate ?? "" },
      { label: "Certificate no.", value: t?.certificate ?? "" },
      { label: "Valid from", value: t?.valid_from ? D.editable(t.valid_from) : "" },
      { label: "Valid to", value: t?.valid_to ? D.editable(t.valid_to) : "" },
    ],
    ([, nature, deductee, rate, cert, from, to]) => {
      if (natureOf(natures, nature) === undefined) return `"${nature}" is not a nature of payment.`;
      if (deducteeOf(deductee) === undefined) return `"${deductee}" is not a deductee type (${DEDUCTEE_KEYS.map((k) => DEDUCTEE_LABEL[k]).join(", ")}).`;
      if (rate.trim() && M.parse(rate) === null) return `"${rate}" is not a rate.`;
      if (!rate.trim() && (cert.trim() || from.trim() || to.trim())) return "A certificate needs its rate.";
      if (rate.trim() && !natureOf(natures, nature) && !deducteeOf(deductee)) return "Give a nature of payment or a deductee type with the certificate.";
      if (from.trim() && !D.parseUser(from, ref)) return `"${from}" is not a date.`;
      if (to.trim() && !D.parseUser(to, ref)) return `"${to}" is not a date.`;
      return null;
    },
  );
  if (!v) return null;
  const [pan, nature, deductee, rate, cert, from, to] = v;
  const n = natureOf(natures, nature) ?? null;
  const k = deducteeOf(deductee) ?? null;
  const tds: LedgerTds | null =
    n || k
      ? {
          kind: "Deductible",
          nature: n?.code ?? null,
          deductee: k,
          lower_rate: rate.trim() ? M.toDecimal(M.parse(rate)!) : null,
          certificate: cert.trim() || null,
          valid_from: from.trim() ? D.parseUser(from, ref) : null,
          valid_to: to.trim() ? D.parseUser(to, ref) : null,
          non_resident: t?.non_resident ?? null,
        }
      : null;
  return save_(shell, ledger, pan.trim() || null, tds);
}

/**
 * Edit a party's non-resident terms: the rate (always the user's, from the
 * treaty or the Act), whether it is the treaty's, the Form 144 country and
 * nature of remittance, and the foreign tax number. Clearing the rate removes
 * the terms. Returns the saved ledger, or null when cancelled.
 */
export async function editLedgerNonResident(shell: Shell, ledger: Ledger): Promise<Ledger | null> {
  const [countries, remittances] = await tdsStatementApi.nrCodes();
  const t = ledger.tds?.kind === "Deductible" ? ledger.tds : null;
  const nr = t?.non_resident ?? null;
  const v = await shell.dialog(
    `Non-resident TDS · ${ledger.name}`,
    [
      { label: "Rate % (from the treaty or the Act; blank: none)", value: nr?.rate ?? "" },
      { label: "Treaty rate (y/n)", value: nr?.treaty ? "y" : "n" },
      { label: "Country (code or name)", value: nr?.country ? (countries.find((c) => c.code === nr.country)?.name ?? nr.country) : "" },
      { label: "Nature of remittance (code: 21 fees for technical services, 49 royalty, 27 interest, 99 other…)", value: nr?.remittance ?? "" },
      { label: "Tax number in the country of residence", value: nr?.foreign_tin ?? "" },
    ],
    ([rate, , country, remittance, tin]) => {
      if (!rate.trim()) return null;
      const r = M.parse(rate);
      if (r === null || r < 0n || r > M.parse("100")!) return `"${rate}" is not a rate from 0 to 100.`;
      if (codeOf(countries, country) === undefined) return `"${country}" is not in the Form 144 country list.`;
      if (codeOf(remittances, remittance) === undefined) return `"${remittance}" is not a nature-of-remittance code (${remittances.map((x) => x.code).join(", ")}).`;
      if (tin.trim().length > 25) return "The tax number is at most 25 characters.";
      return null;
    },
  );
  if (!v) return null;
  const [rate, treaty, country, remittance, tin] = v;
  const terms: NonResidentTerms | null = rate.trim()
    ? {
        rate: M.toDecimal(M.parse(rate)!),
        treaty: yes(treaty),
        country: codeOf(countries, country)?.code ?? null,
        remittance: codeOf(remittances, remittance)?.code ?? null,
        foreign_tin: tin.trim() || null,
      }
    : null;
  const base = t ?? { kind: "Deductible" as const, nature: null, deductee: null };
  const tds: LedgerTds | null = terms || base.nature || base.deductee ? { ...base, kind: "Deductible", non_resident: terms } : null;
  const saved = await save_(shell, ledger, ledger.pan, tds);
  if (saved && terms) shell.info("Non-resident terms saved. Before remitting, file Form 145 (ex-15CA) and, where required, Form 146 (ex-15CB).", 9000);
  return saved;
}

/**
 * Edit a ledger's TCS settings. A ledger under Duties & Taxes (or already a
 * TCS payable ledger) is asked which nature it holds; a sales ledger takes a
 * nature of collection; a party takes the buyer's terms (type, declaration,
 * excluded buyer, non-resident, lower-collection certificate). Returns the
 * saved settings (null when cleared), or undefined when cancelled.
 */
export async function editLedgerTcs(shell: Shell, ledger: Ledger): Promise<LedgerTcs | null | undefined> {
  const natures = await tcsApi.natures();
  const cur = await tcsApi.ledgerTcs(ledger.id);
  const ref = today(shell);
  const payable = cur?.kind === "Payable" || (cur === null && ledger.group_name === "Duties & Taxes");
  let tcs: LedgerTcs | null;
  if (payable) {
    const current = cur?.kind === "Payable" ? cur.payable_for : "";
    const v = await shell.dialog(
      `TCS payable · ${ledger.name}`,
      [{ label: `Holds TCS for (nature, "${TCS_ALL_NATURES}", or empty for none)`, value: current }],
      ([n]) => (n.trim() === "" || n.trim().toLowerCase() === TCS_ALL_NATURES || tcsNatureOf(natures, n) ? null : `"${n}" is not a nature of collection.`),
    );
    if (!v) return undefined;
    const n = v[0].trim().toLowerCase();
    tcs = n === "" ? null : { kind: "Payable", payable_for: n === TCS_ALL_NATURES ? n : tcsNatureOf(natures, n)!.code };
  } else {
    const c = cur?.kind === "Collectible" ? cur : null;
    const v = await shell.dialog(
      `TCS · ${ledger.name}`,
      [
        { label: `Nature of collection (${natures.map((n) => n.code).join(", ")})`, value: c?.nature ?? "" },
        { label: "Buyer type", value: c?.collectee ? DEDUCTEE_LABEL[c.collectee] : "" },
        { label: "Buyer's declaration: not for trading (y/n)", value: c?.declaration ? "y" : "n" },
        { label: "Not a buyer under the Act: government, embassy, PSU, club… (y/n)", value: c?.excluded ? "y" : "n" },
        { label: "Non-resident (y/n)", value: c?.non_resident ? "y" : "n" },
        { label: "Permanent establishment in India (y/n)", value: c?.permanent_establishment ? "y" : "n" },
        { label: "Lower collection rate %", value: c?.lower_rate ?? "" },
        { label: "Certificate no.", value: c?.certificate ?? "" },
        { label: "Valid from", value: c?.valid_from ? D.editable(c.valid_from) : "" },
        { label: "Valid to", value: c?.valid_to ? D.editable(c.valid_to) : "" },
      ],
      ([nature, buyer, , , , , rate, cert, from, to]) => {
        if (tcsNatureOf(natures, nature) === undefined) return `"${nature}" is not a nature of collection.`;
        if (deducteeOf(buyer) === undefined) return `"${buyer}" is not a buyer type (${DEDUCTEE_KEYS.map((k) => DEDUCTEE_LABEL[k]).join(", ")}).`;
        if (rate.trim() && M.parse(rate) === null) return `"${rate}" is not a rate.`;
        if (!rate.trim() && (cert.trim() || from.trim() || to.trim())) return "A certificate needs its rate.";
        if (from.trim() && !D.parseUser(from, ref)) return `"${from}" is not a date.`;
        if (to.trim() && !D.parseUser(to, ref)) return `"${to}" is not a date.`;
        return null;
      },
    );
    if (!v) return undefined;
    const [nature, buyer, declaration, excluded, nonResident, pe, rate, cert, from, to] = v;
    const terms = {
      nature: tcsNatureOf(natures, nature)?.code ?? null,
      collectee: deducteeOf(buyer) ?? null,
      declaration: yes(declaration),
      excluded: yes(excluded),
      non_resident: yes(nonResident),
      permanent_establishment: yes(nonResident) && yes(pe),
      lower_rate: rate.trim() ? M.toDecimal(M.parse(rate)!) : null,
      certificate: cert.trim() || null,
      valid_from: from.trim() ? D.parseUser(from, ref) : null,
      valid_to: to.trim() ? D.parseUser(to, ref) : null,
    };
    const any = terms.nature || terms.collectee || terms.declaration || terms.excluded || terms.non_resident;
    tcs = any ? { kind: "Collectible", ...terms } : null;
  }
  try {
    const saved = await tcsApi.setLedgerTcs(ledger.id, tcs);
    shell.info(`${ledger.name}: TCS saved.`);
    return saved;
  } catch (e) {
    shell.error(e);
    return undefined;
  }
}

async function save_(shell: Shell, ledger: Ledger, pan: string | null, tds: LedgerTds | null): Promise<Ledger | null> {
  try {
    const saved = await tdsApi.setLedgerTds(ledger.id, pan, tds);
    shell.info(`${saved.name}: PAN and TDS saved.`);
    return saved;
  } catch (e) {
    shell.error(e);
    return null;
  }
}

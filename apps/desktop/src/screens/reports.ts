// Reports (rules R1, R3, R5): Day Book, Trial Balance, Profit & Loss,
// Balance Sheet, ledger statement, group summary and the voucher view.
// Enter drills down; Esc goes back to the parent with its cursor intact.
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
// `tr`: some reports keep a total in a local `t`.
import { language, ltr, t as tr } from "../i18n";
import * as M from "../money";
import { save } from "@tauri-apps/plugin-dialog";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, type Context, type DayBookRow, type ExportTable, type Group, type GroupTotal, type Ledger, type ReportSpec, type VoucherKind } from "../types";
import { savedVoucherScreen } from "./stockjournal";
import type { NoteLink } from "../desk-api";
import { allNotes, editNote, notesBlock } from "./desk";

/** R1: the last drill-down level opens the voucher itself, for alteration. */
function openVoucher(shell: Shell, row: { kind: VoucherKind; date: string; voucher: string }): void {
  void shell.push(savedVoucherScreen(shell, row.kind, row.date, row.voucher));
}

interface Pick {
  el: HTMLElement;
  open?: () => void;
  col?: number;
  /** What Alt+W puts a note on (the row's ledger or voucher). */
  note?: NoteLink;
}

/** Alt+W on a voucher row. */
function voucherNote(r: { kind: VoucherKind; number: string; voucher: string }): NoteLink {
  return { kind: "voucher", id: r.voucher, label: `${tr(KIND_LABEL[r.kind])} ${r.number}` };
}

/** Shared cursor, drill-down and period handling for report screens. */
export abstract class Report implements Screen {
  abstract title: string;
  el = h("section.report");
  protected head = h("div.rhead");
  protected bodyEl = h("div.rbody");
  protected picks: Pick[] = [];
  protected cursor = 0;
  /** Which period keys this report accepts. */
  protected period: "range" | "asat" | null = null;
  /** Alt+W adds a note on the row's record (or `defaultNote`). */
  protected noteable = false;

  constructor(protected shell: Shell) {
    this.el.append(this.head, this.bodyEl);
  }

  abstract load(): Promise<void>;

  async init(): Promise<void> {
    await this.load();
  }

  contexts(): Context[] {
    return ["Report"];
  }

  handles(a: string): boolean {
    if (a === "ChangeDate" || a === "ChangePeriod") return this.period !== null;
    if (a === "AddNote") return this.noteable;
    return ["DrillDown", "Back", "PrintCurrent", "ExportCurrent"].includes(a);
  }

  /** The record Alt+W notes when the cursor row has none (a ledger statement's ledger). */
  protected defaultNote(): NoteLink | null {
    return null;
  }

  /** Alt+W: a note or to-do on the record under the cursor, kept on the desk. */
  private async addNote(): Promise<void> {
    const link = this.picks[this.cursor]?.note ?? this.defaultNote();
    if (!link) {
      this.shell.warn(tr("Move to a ledger or voucher row first."));
      return;
    }
    if (await editNote(this.shell, { link })) await this.reload();
  }

  /** Reload when a child (an altered or deleted voucher) closes; the cursor stays. */
  resume(): void {
    void this.reload();
  }

  /** khata-print renders the core reports; others print the table shown. */
  protected printSpec(): ReportSpec | null {
    return null;
  }

  /** Trial Balance exports through khata-print's CSV; others as the table shown. */
  protected csvSpec(): ReportSpec {
    return { report: "table", ...this.table() };
  }

  /** The table(s) on screen, as plain text, for printing and CSV. */
  protected table(): ExportTable {
    const columns: string[] = [];
    const numeric = new Set<number>();
    const rows: string[][] = [];
    for (const t of this.bodyEl.querySelectorAll("table.rtable")) {
      const side = t.closest(".tside")?.querySelector(".tside-title")?.textContent?.trim();
      const head = [...t.querySelectorAll("thead th")].map((th) => th.textContent?.trim() ?? "");
      if (!columns.length) columns.push(...(head.length ? head : [tr("Particulars"), tr("Amount")]));
      if (side) rows.push([side.toUpperCase()]);
      // A table with other columns than the first (GSTR-3B's ITC and net tax) brings its own heading row.
      if (head.length && head.join("\n") !== columns.join("\n")) rows.push(head);
      for (const tr of t.querySelectorAll("tbody tr, tfoot tr")) {
        if (tr.classList.contains("empty")) continue;
        const row: string[] = [];
        for (const td of tr.children) {
          if (td.classList.contains("num")) numeric.add(row.length);
          row.push(td.textContent?.trim() ?? "");
          for (let k = 1; k < ((td as HTMLTableCellElement).colSpan || 1); k++) row.push("");
        }
        rows.push(row);
      }
    }
    return {
      title: this.head.querySelector(".rtitle")?.textContent ?? this.title,
      subtitle: this.head.querySelector(".rsub")?.textContent ?? "",
      columns,
      numeric: [...numeric],
      rows: rows.map((r) => (r.length < columns.length ? [...r, ...Array(columns.length - r.length).fill("")] : r)),
    };
  }

  private async print(): Promise<void> {
    try {
      await api.printReport(this.printSpec() ?? { report: "table", ...this.table() }, language());
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async exportCsv(): Promise<void> {
    try {
      const csv = await api.reportCsv(this.csvSpec());
      const name = (this.head.querySelector(".rtitle")?.textContent ?? this.title).replace(/[^\w]+/g, "-").replace(/^-|-$/g, "");
      const path = await save({ defaultPath: `${name || "report"}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      await api.writeCsv(path.toLowerCase().endsWith(".csv") ? path : `${path}.csv`, csv);
      this.shell.info(tr("Exported to {path}", { path: ltr(path) }));
    } catch (e) {
      this.shell.error(e);
    }
  }

  keyLabel(a: string): string | undefined {
    return a === "ChangeDate" && this.period === "range" ? "Period" : undefined;
  }

  bottom() {
    const ctx = this.contexts();
    return [
      this.shell.btn("DrillDown", ctx, "Open", this.picks.some((p) => p.open)),
      this.period ? this.shell.btn("ChangeDate", ctx, this.period === "range" ? "Period" : "Date") : null,
      this.shell.btn("PrintCurrent", ctx, "Print"),
      this.shell.btn("ExportCurrent", ctx, "Export CSV"),
      this.handles("CancelVoucher") ? this.shell.btn("CancelVoucher", ctx, "Cancel Vch") : null,
      this.noteable ? this.shell.btn("AddNote", ctx, "Note") : null,
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    (document.activeElement as HTMLElement | null)?.blur?.();
    this.paint();
  }

  /** Register a selectable row. */
  protected pick(el: HTMLElement, open?: () => void, col = 0, note?: NoteLink): HTMLElement {
    el.classList.add("rrow");
    if (open) el.classList.add("drill");
    const index = this.picks.length;
    this.picks.push({ el, open, col, note });
    el.addEventListener("click", () => {
      this.cursor = index;
      this.paint();
    });
    el.addEventListener("dblclick", () => open?.());
    return el;
  }

  protected resetPicks(): void {
    this.picks = [];
  }

  protected paint(): void {
    this.cursor = Math.max(0, Math.min(this.cursor, this.picks.length - 1));
    this.picks.forEach((p, i) => p.el.classList.toggle("cur", i === this.cursor));
    this.picks[this.cursor]?.el.scrollIntoView({ block: "nearest" });
    // On the last row, also bring a note below the tables into view (keyboard users can't scroll).
    if (this.picks.length > 1 && this.cursor === this.picks.length - 1) this.bodyEl.lastElementChild?.scrollIntoView({ block: "nearest" });
  }

  private move(delta: number): void {
    if (!this.picks.length) return;
    this.cursor = Math.max(0, Math.min(this.picks.length - 1, this.cursor + delta));
    this.paint();
  }

  /** Jump to the same position in the other column (T-format reports). */
  private side(dir: number): void {
    const cur = this.picks[this.cursor];
    if (!cur) return;
    const col = (cur.col ?? 0) + dir;
    const mine = this.picks.filter((p) => p.col === cur.col);
    const there = this.picks.filter((p) => p.col === col);
    if (!there.length) return;
    const target = there[Math.min(mine.indexOf(cur), there.length - 1)];
    this.cursor = this.picks.indexOf(target);
    this.paint();
  }

  key(e: KeyEv): boolean {
    switch (e.chord) {
      case "Up":
        this.move(-1);
        return true;
      case "Down":
        this.move(1);
        return true;
      case "Left":
        this.side(-1);
        return true;
      case "Right":
        this.side(1);
        return true;
      case "Home":
      case "Ctrl+Home":
        this.move(-this.picks.length);
        return true;
      case "End":
      case "Ctrl+End":
        this.move(this.picks.length);
        return true;
    }
    return false;
  }

  action(a: string): boolean {
    if (a === "AddNote" && this.noteable) {
      void this.addNote();
      return true;
    }
    if (a === "PrintCurrent") {
      void this.print();
      return true;
    }
    if (a === "ExportCurrent") {
      void this.exportCsv();
      return true;
    }
    if (a === "DrillDown") {
      this.picks[this.cursor]?.open?.();
      return true;
    }
    if ((a === "ChangeDate" || a === "ChangePeriod") && this.period) {
      void this.askPeriod();
      return true;
    }
    if (a === "PageDown" || a === "NextRecord") {
      this.move(10);
      return true;
    }
    if (a === "PreviousRecord") {
      this.move(-10);
      return true;
    }
    return false;
  }

  protected askPeriod(): Promise<void> {
    return Promise.resolve();
  }

  protected async rangeDialog(from: string, to: string): Promise<[string, string] | null> {
    const ref = to;
    const v = await this.shell.dialog(
      "Change period",
      [
        { label: "From", value: D.editable(from) },
        { label: "To", value: D.editable(to) },
      ],
      ([a, b]) => {
        const f = D.parseUser(a, ref);
        const t = D.parseUser(b, ref);
        if (!f) return tr('"{text}" is not a date.', { text: a });
        if (!t) return tr('"{text}" is not a date.', { text: b });
        return f > t ? tr("The period ends before it starts.") : null;
      },
    );
    return v ? [D.parseUser(v[0], ref)!, D.parseUser(v[1], ref)!] : null;
  }

  protected async dateDialog(asAt: string): Promise<string | null> {
    const v = await this.shell.dialog("Change date", [{ label: "As at", value: D.editable(asAt) }], ([a]) =>
      D.parseUser(a, asAt) ? null : tr('"{text}" is not a date.', { text: a }),
    );
    return v ? D.parseUser(v[0], asAt) : null;
  }

  protected async reload(): Promise<void> {
    try {
      await this.load();
      this.paint();
      this.shell.refresh();
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected heading(title: string, sub: string): void {
    this.head.replaceChildren(h("div.rtitle", title), h("div.rsub", sub));
  }
}

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function amountCell(text: string, blankZero = true): HTMLElement {
  return h("td.num", M.fmt(text, blankZero));
}

// --------------------------------------------------------------- Day Book

export class DayBook extends Report {
  title = "Day Book";
  protected override period = "range" as const;
  protected override noteable = true;
  constructor(shell: Shell, private from = today(shell), private to = today(shell)) {
    super(shell);
  }

  private rows: DayBookRow[] = [];

  protected override printSpec(): ReportSpec {
    return { report: "day_book", from: this.from, to: this.to };
  }

  override handles(a: string): boolean {
    return a === "CancelVoucher" || a === "ExportMenu" || super.handles(a);
  }

  override action(a: string): boolean {
    if (a === "ExportMenu") {
      // Alt+E: the period's invoices and notes (or the voucher under the cursor) as one PDF.
      const r = this.rows[this.cursor];
      const current = r && !r.cancelled ? { id: r.voucher, kind: r.kind, number: r.number, date: r.date } : null;
      void import("./invoice_design").then((m) => m.dayBookPdf(this.shell, this.from, this.to, current));
      return true;
    }
    if (a !== "CancelVoucher") return super.action(a);
    const row = this.rows[this.cursor];
    if (!row) return true;
    if (row.cancelled) this.shell.warn(tr("{kind} No. {number} is already cancelled.", { kind: tr(KIND_LABEL[row.kind]), number: ltr(row.number) }));
    else void this.cancel(row);
    return true;
  }

  /** Alt+X on a row: cancel after a Y/N prompt; the number stays used (rule V7). */
  private async cancel(row: DayBookRow): Promise<void> {
    const label = tr("{kind} No. {number}", { kind: tr(KIND_LABEL[row.kind]), number: ltr(row.number) });
    if (!(await this.shell.ask(tr("Cancel {label}? It keeps its number but no longer counts.", { label }), { yesFirst: false }))) return;
    try {
      await api.cancelVoucher(row.voucher);
      await this.shell.refreshCompany();
      this.shell.info(tr("{label} cancelled.", { label }));
      await this.reload();
    } catch (e) {
      this.shell.error(e);
    }
  }

  async load(): Promise<void> {
    const rows = await api.dayBook(this.from, this.to);
    this.rows = rows;
    this.heading(tr("Day Book"), range(this.from, this.to));
    this.resetPicks();
    const tbody = h("tbody");
    for (const r of rows) {
      tbody.append(
        this.pick(
          h(
            "tr",
            { class: r.cancelled ? "cancelled" : "" },
            h("td", D.show(r.date)),
            h("td.strong", r.particulars, r.cancelled ? h("span.muted", ` (${tr("Cancelled")})`) : null),
            h("td", tr(KIND_LABEL[r.kind])),
            h("td", h("bdi", r.number)),
            amountCell(r.debit),
            amountCell(r.credit),
          ),
          () => openVoucher(this.shell, r),
          0,
          voucherNote(r),
        ),
      );
    }
    if (!rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 6 }, tr("No vouchers in this period. Press F2 to change it."))));
    const dr = M.sum(rows.map((r) => M.of(r.debit)));
    const cr = M.sum(rows.map((r) => M.of(r.credit)));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Date")), h("th", tr("Particulars")), h("th", tr("Vch Type")), h("th", tr("Vch No.")), h("th.num", tr("Debit")), h("th.num", tr("Credit")))),
        tbody,
        h("tfoot", h("tr", h("td", { colSpan: 4 }, tr("Total")), h("td.num", M.format(dr)), h("td.num", M.format(cr)))),
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

// ---------------------------------------------------------- Trial Balance

export class TrialBalanceReport extends Report {
  title = "Trial Balance";
  protected override period = "asat" as const;
  protected override noteable = true;
  constructor(shell: Shell, private asAt = shell.periodTo()) {
    super(shell);
  }

  protected override printSpec(): ReportSpec {
    return { report: "trial_balance", as_at: this.asAt };
  }

  protected override csvSpec(): ReportSpec {
    return { report: "trial_balance", as_at: this.asAt };
  }

  async load(): Promise<void> {
    const tb = await api.trialBalance(this.asAt);
    this.heading(tr("Trial Balance"), tr("As at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const tbody = h("tbody");
    const from = this.shell.periodFromFor(this.asAt);
    for (const r of tb.rows) {
      tbody.append(
        this.pick(
          h("tr", h("td.strong", r.name, h("span.muted", ` · ${r.group_name}`)), amountCell(r.debit), amountCell(r.credit)),
          () => void this.shell.push(new LedgerStatementReport(this.shell, r.ledger_id, from, this.asAt)),
          0,
          { kind: "ledger", id: r.ledger_id, label: r.name },
        ),
      );
    }
    // Opening stock is part of the opening balances but no ledger's.
    if (M.of(tb.opening_stock) !== 0n) {
      tbody.append(
        this.pick(h("tr", h("td.strong", tr("Opening Stock"), h("span.muted", ` · ${tr("Stock-in-Hand")}`)), amountCell(tb.opening_stock), amountCell("0")), () =>
          openStockSummary(this.shell),
        ),
      );
    }
    if (!tb.rows.length && M.of(tb.opening_stock) === 0n) tbody.append(h("tr.empty", h("td", { colSpan: 3 }, tr("No balances yet."))));
    const diff = M.of(tb.opening_difference);
    const foot = h("tfoot");
    if (diff !== 0n) {
      // Openings that don't tally: show the balancing figure (Ma7).
      foot.append(
        h(
          "tr.warnrow",
          h("td", tr("Difference in opening balances")),
          h("td.num", diff < 0n ? M.format(-diff) : ""),
          h("td.num", diff > 0n ? M.format(diff) : ""),
        ),
      );
    }
    const dr = M.of(tb.total_debit) + (diff < 0n ? -diff : 0n);
    const cr = M.of(tb.total_credit) + (diff > 0n ? diff : 0n);
    foot.append(h("tr", h("td", tr("Grand Total")), h("td.num", M.format(dr)), h("td.num", M.format(cr))));
    this.bodyEl.replaceChildren(
      h("table.rtable", h("thead", h("tr", h("th", tr("Particulars")), h("th.num", tr("Debit")), h("th.num", tr("Credit")))), tbody, foot),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const d = await this.dateDialog(this.asAt);
    if (!d) return;
    this.asAt = d;
    await this.reload();
  }
}

// ----------------------------------------------------------- P&L and BS

/** Trading groups come first, so the stock rows sit beside them. */

function tradingFirst(rows: GroupTotal[]): GroupTotal[] {
  return [...rows.filter((g) => g.affects_gross_profit), ...rows.filter((g) => !g.affects_gross_profit)];
}

/** Stock rows open the Stock Summary (loaded lazily: inventory.ts builds on this file). */
function openStockSummary(shell: Shell): void {
  void import("./inventory").then((m) => shell.push(new m.StockSummaryReport(shell)));
}

function tside(title: string, rows: HTMLElement[], total: string): HTMLElement {
  return h(
    "div.tside",
    h("div.tside-title", tr(title)),
    h("table.rtable", h("tbody", ...rows), h("tfoot", h("tr", h("td", tr("Total")), h("td.num", total)))),
  );
}

export class ProfitLossReport extends Report {
  title = "Profit & Loss A/c";
  protected override period = "range" as const;
  constructor(shell: Shell, private from = shell.periodFrom(), private to = shell.periodTo()) {
    super(shell);
  }

  protected override printSpec(): ReportSpec {
    return { report: "profit_and_loss", from: this.from, to: this.to };
  }

  async load(): Promise<void> {
    const pl = await api.profitAndLoss(this.from, this.to);
    this.heading(tr("Profit & Loss A/c"), range(this.from, this.to));
    this.resetPicks();
    const drill = (g: { group: string }) => () =>
      void this.shell.push(new GroupSummaryReport(this.shell, g.group, this.from, this.to));
    const net = M.of(pl.net_profit);
    const opening = M.of(pl.opening_stock);
    const closing = M.of(pl.closing_stock);
    // Stock belongs to the trading part: opening stock heads the expenses,
    // closing stock follows the trading incomes (rows only when non-zero).
    const expenses = tradingFirst(pl.expenses);
    const incomes = tradingFirst(pl.incomes);
    const stockRow = (label: string, u: M.Units, col: number) =>
      this.pick(h("tr", h("td", tr(label)), h("td.num", M.format(u))), () => openStockSummary(this.shell), col);
    const left: HTMLElement[] = [];
    if (opening !== 0n) left.push(stockRow("Opening Stock", opening, 0));
    left.push(...expenses.map((g) => this.pick(h("tr", h("td", g.name), amountCell(g.amount, false)), drill(g), 0)));
    const right: HTMLElement[] = [];
    const stockAt = incomes.filter((g) => g.affects_gross_profit).length;
    for (const [i, g] of incomes.entries()) {
      if (i === stockAt && closing !== 0n) right.push(stockRow("Closing Stock", closing, 1));
      right.push(this.pick(h("tr", h("td", g.name), amountCell(g.amount, false)), drill(g), 1));
    }
    if (stockAt === incomes.length && closing !== 0n) right.push(stockRow("Closing Stock", closing, 1));
    if (net > 0n) left.push(h("tr.result", h("td", tr("Net Profit")), h("td.num", M.format(net))));
    if (net < 0n) right.push(h("tr.result.loss", h("td", tr("Net Loss")), h("td.num", M.format(-net))));
    const totalE = opening + M.sum(pl.expenses.map((g) => M.of(g.amount))) + (net > 0n ? net : 0n);
    const totalI = closing + M.sum(pl.incomes.map((g) => M.of(g.amount))) + (net < 0n ? -net : 0n);
    if (!left.length && !right.length) left.push(h("tr.empty", h("td", { colSpan: 2 }, tr("No income or expenses in this period."))));
    this.bodyEl.replaceChildren(
      h("div.gross", tr(M.of(pl.gross_profit) >= 0n ? "Gross profit: {amount}" : "Gross loss: {amount}", { amount: ltr(M.format(M.abs(M.of(pl.gross_profit)))) })),
      h("div.tformat", tside("Expenses", left, M.format(totalE)), tside("Income", right, M.format(totalI))),
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

export class BalanceSheetReport extends Report {
  title = "Balance Sheet";
  protected override period = "asat" as const;
  constructor(shell: Shell, private asAt = shell.periodTo()) {
    super(shell);
  }

  protected override printSpec(): ReportSpec {
    return { report: "balance_sheet", as_at: this.asAt };
  }

  private stockRow(u: M.Units): HTMLElement {
    return this.pick(h("tr", h("td", tr("Closing Stock"), h("span.muted", ` · ${tr("Stock-in-Hand")}`)), h("td.num", M.format(u))), () => openStockSummary(this.shell), 1);
  }

  async load(): Promise<void> {
    const bs = await api.balanceSheet(this.asAt);
    this.heading(tr("Balance Sheet"), tr("As at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const drill = (g: { group: string }) => () => void this.shell.push(new GroupSummaryReport(this.shell, g.group, null, this.asAt));
    const left = bs.liabilities.map((g) => this.pick(h("tr", h("td", g.name), amountCell(g.amount, false)), drill(g), 0));
    const pl = M.of(bs.profit_and_loss);
    if (pl !== 0n) {
      const from = D.fyStart(this.asAt, Number(this.shell.company?.fy_start.slice(5, 7)) || 4);
      left.push(
        this.pick(
          h("tr", h("td", tr("Profit & Loss A/c")), h("td.num", M.format(pl))),
          () => void this.shell.push(new ProfitLossReport(this.shell, from, this.asAt)),
          0,
        ),
      );
    }
    const diff = M.of(bs.opening_difference);
    if (diff !== 0n) left.push(h("tr.warnrow", h("td", tr("Difference in opening balances")), h("td.num", M.format(diff))));
    // Closing stock (valued by inventory, not a ledger) follows Current Assets.
    const stock = M.of(bs.closing_stock);
    const stockAt = bs.assets.findIndex((g) => g.name === "Current Assets") + 1 || bs.assets.length;
    const right: HTMLElement[] = [];
    for (const [i, g] of bs.assets.entries()) {
      if (i === stockAt && stock !== 0n) right.push(this.stockRow(stock));
      right.push(this.pick(h("tr", h("td", g.name), amountCell(g.amount, false)), drill(g), 1));
    }
    if (stockAt === bs.assets.length && stock !== 0n) right.push(this.stockRow(stock));
    if (!left.length && !right.length) left.push(h("tr.empty", h("td", { colSpan: 2 }, tr("Nothing to show yet."))));
    this.bodyEl.replaceChildren(
      h("div.tformat", tside("Liabilities", left, M.fmt(bs.total_liabilities)), tside("Assets", right, M.fmt(bs.total_assets))),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const d = await this.dateDialog(this.asAt);
    if (!d) return;
    this.asAt = d;
    await this.reload();
  }
}

// ------------------------------------------------- group summary, ledgers

export class GroupSummaryReport extends Report {
  title = "Group Summary";
  protected override noteable = true;
  constructor(shell: Shell, private groupId: string, private from: string | null, private to: string) {
    super(shell);
  }

  async load(): Promise<void> {
    const gs = await api.groupSummary(this.groupId, this.from, this.to);
    this.title = gs.group_name;
    this.heading(gs.group_name, this.from ? range(this.from, this.to) : tr("As at {date}", { date: D.show(this.to) }));
    this.resetPicks();
    const stmtFrom = this.from ?? this.shell.periodFromFor(this.to);
    const tbody = h("tbody");
    for (const r of gs.rows) {
      const u = M.of(r.amount);
      tbody.append(
        this.pick(
          h("tr", h("td.strong", r.name, h("span.muted", ` · ${r.group_name}`)), h("td.num", u > 0n ? M.format(u) : ""), h("td.num", u < 0n ? M.format(-u) : "")),
          () => void this.shell.push(new LedgerStatementReport(this.shell, r.ledger_id, stmtFrom, this.to)),
          0,
          { kind: "ledger", id: r.ledger_id, label: r.name },
        ),
      );
    }
    if (!gs.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 3 }, tr("No ledgers with balances."))));
    const t = M.of(gs.total);
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Particulars")), h("th.num", tr("Debit")), h("th.num", tr("Credit")))),
        tbody,
        h("tfoot", h("tr", h("td", tr("Total")), h("td.num", t > 0n ? M.format(t) : ""), h("td.num", t < 0n ? M.format(-t) : ""))),
      ),
    );
  }
}

export class LedgerStatementReport extends Report {
  title = "Ledger Vouchers";
  protected override period = "range" as const;
  protected override noteable = true;
  private ledgerName = "";
  constructor(shell: Shell, private ledgerId: string, private from: string, private to: string) {
    super(shell);
  }

  /** Alt+W notes the ledger (a party's "called on 3 Oct, will pay Friday"); a voucher's own note goes on the voucher (Alt+W there). */
  protected override defaultNote(): NoteLink {
    return { kind: "ledger", id: this.ledgerId, label: this.ledgerName };
  }

  protected override printSpec(): ReportSpec {
    return { report: "ledger_statement", ledger_id: this.ledgerId, from: this.from, to: this.to };
  }

  async load(): Promise<void> {
    const [st, notes] = await Promise.all([api.ledgerStatement(this.ledgerId, this.from, this.to), allNotes()]);
    this.ledgerName = st.ledger_name;
    const inRows = new Set(st.rows.map((r) => r.voucher));
    const mine = notes.filter((n) => n.link && !n.link.missing && (n.link.kind === "ledger" ? n.link.id === this.ledgerId : n.link.kind === "voucher" && inRows.has(n.link.id)));
    const noted = new Set(mine.filter((n) => n.link?.kind === "voucher").map((n) => n.link!.id));
    this.title = tr("Ledger: {name}", { name: st.ledger_name });
    this.heading(st.ledger_name, range(this.from, this.to));
    this.resetPicks();
    const tbody = h("tbody");
    const open = M.of(st.opening);
    // A ledger kept in a foreign currency: its balance in that currency too.
    const fx = st.forex;
    const fxCell = (text: string | undefined) => (fx ? h("td.num", text ? M.drCr(M.of(text), true) : "") : null);
    tbody.append(
      h("tr.opening", h("td"), h("td.strong", tr("Opening Balance")), h("td"), h("td"), h("td.num", open > 0n ? M.format(open) : ""), h("td.num", open < 0n ? M.format(-open) : ""), h("td.num", M.drCr(open)), fxCell(fx?.opening)),
    );
    for (const r of st.rows) {
      tbody.append(
        this.pick(
          h(
            "tr",
            h("td", D.show(r.date)),
            h("td.strong", r.particulars, noted.has(r.voucher) ? h("span.note-mark", { title: tr("Has a note") }, "✎") : null),
            h("td", tr(KIND_LABEL[r.kind])),
            h("td", h("bdi", r.number)),
            amountCell(r.debit),
            amountCell(r.credit),
            h("td.num", M.drCr(M.of(r.balance))),
            fxCell(r.forex),
          ),
          () => openVoucher(this.shell, r),
        ),
      );
    }
    if (!st.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: fx ? 8 : 7 }, tr("No vouchers in this period. Press F2 to change it."))));
    const dr = M.sum(st.rows.map((r) => M.of(r.debit)));
    const cr = M.sum(st.rows.map((r) => M.of(r.credit)));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Date")), h("th", tr("Particulars")), h("th", tr("Vch Type")), h("th", tr("Vch No.")), h("th.num", tr("Debit")), h("th.num", tr("Credit")), h("th.num", tr("Balance")), fx ? h("th.num", tr("In {currency}", { currency: fx.currency })) : null)),
        tbody,
        h(
          "tfoot",
          h("tr", h("td", { colSpan: 4 }, tr("Current Total")), h("td.num", M.format(dr)), h("td.num", M.format(cr)), h("td"), fx ? h("td") : null),
          h("tr", h("td", { colSpan: 4 }, tr("Closing Balance")), h("td"), h("td"), h("td.num", M.drCr(M.of(st.closing))), fxCell(fx?.closing)),
        ),
      ),
      ...[notesBlock(mine)].filter((x): x is HTMLElement => x !== null),
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

// ------------------------------------------------------ chart of accounts

export class ChartOfAccounts extends Report {
  title = "Chart of Accounts";

  async load(): Promise<void> {
    const [groups, ledgers] = await Promise.all([api.searchGroups(""), api.listLedgers("", "all")]);
    this.heading(tr("Chart of Accounts"), tr("{groups} groups · {ledgers} ledgers", { groups: groups.length, ledgers: ledgers.length }));
    this.resetPicks();
    const byParent = new Map<string | null, Group[]>();
    for (const g of groups) byParent.set(g.parent, [...(byParent.get(g.parent) ?? []), g]);
    const ledgersOf = new Map<string, Ledger[]>();
    for (const l of ledgers) ledgersOf.set(l.group_id, [...(ledgersOf.get(l.group_id) ?? []), l]);
    const tbody = h("tbody");
    const from = this.shell.periodFrom();
    const walk = (parent: string | null, depth: number) => {
      for (const g of byParent.get(parent) ?? []) {
        tbody.append(this.pick(h("tr.group", h("td", { style: { paddingInlineStart: `${8 + depth * 18}px` } }, g.name), h("td.muted", tr(g.nature)), h("td"))));
        walk(g.id, depth + 1);
        for (const l of ledgersOf.get(g.id) ?? []) {
          tbody.append(
            this.pick(
              h("tr", h("td", { style: { paddingInlineStart: `${8 + (depth + 1) * 18}px` } }, h("span.strong", l.name)), h("td.muted", tr("Ledger")), h("td.num", M.drCr(M.of(l.balance), true))),
              () => void this.shell.push(new LedgerStatementReport(this.shell, l.id, from, this.shell.periodTo())),
            ),
          );
        }
      }
    };
    walk(null, 0);
    this.bodyEl.replaceChildren(h("table.rtable", h("thead", h("tr", h("th", tr("Name")), h("th", tr("Type")), h("th.num", tr("Balance")))), tbody));
  }
}

// ------------------------------------------------ bills: outstanding, ageing

function partyStatement(shell: Shell, ledgerId: string, asAt: string): void {
  void shell.push(new LedgerStatementReport(shell, ledgerId, shell.periodFromFor(asAt), asAt));
}

/** Bill-wise receivables or payables, with overdue days. */
export class OutstandingReport extends Report {
  title: string;
  protected override period = "asat" as const;
  protected override noteable = true;
  constructor(shell: Shell, private receivable: boolean, private asAt = shell.periodTo()) {
    super(shell);
    this.title = receivable ? "Outstanding Receivables" : "Outstanding Payables";
  }

  async load(): Promise<void> {
    const bills = await api.outstanding(this.asAt, this.receivable ? "receivable" : "payable");
    this.heading(tr(this.title), tr("As at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const tbody = h("tbody");
    // Bills raised in a foreign currency: pending in it too.
    const anyFx = bills.some((b) => b.forex_amount);
    for (const b of bills) {
      tbody.append(
        this.pick(
          h(
            "tr",
            h("td", b.date ? D.show(b.date) : tr("Opening")),
            h("td.strong", b.reference),
            h("td", b.party),
            h("td", b.due ? D.show(b.due) : ""),
            h("td.num", M.format(M.abs(M.of(b.amount)))),
            anyFx ? h("td.num", b.forex_amount ? ltr(`${b.forex_currency} ${M.format(M.abs(M.of(b.forex_amount)))}`) : "") : null,
            h("td.num", { class: b.overdue_days > 0 && b.due ? "late" : "" }, b.overdue_days ? String(b.overdue_days) : ""),
          ),
          () => partyStatement(this.shell, b.ledger_id, this.asAt),
          0,
          { kind: "ledger", id: b.ledger_id, label: b.party },
        ),
      );
    }
    if (!bills.length) tbody.append(h("tr.empty", h("td", { colSpan: 6 }, tr("Nothing outstanding."))));
    const total = M.sum(bills.map((b) => M.abs(M.of(b.amount))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Date")), h("th", tr("Ref. No.")), h("th", tr("Party")), h("th", tr("Due on")), h("th.num", tr("Pending")), anyFx ? h("th.num", tr("Pending (foreign)")) : null, h("th.num", tr("Overdue days")))),
        tbody,
        h("tfoot", h("tr", h("td", { colSpan: 4 }, tr("Total")), h("td.num", M.format(total)), anyFx ? h("td") : null, h("td"))),
      ),
      h("div.note", tr("Overdue days count from the due date, or from the bill date when a bill has no due date.")),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const d = await this.dateDialog(this.asAt);
    if (!d) return;
    this.asAt = d;
    await this.reload();
  }
}

/** Party-wise ageing in 30-day buckets. */
export class AgeingReport extends Report {
  title: string;
  protected override period = "asat" as const;
  protected override noteable = true;
  constructor(shell: Shell, private receivable: boolean, private asAt = shell.periodTo()) {
    super(shell);
    this.title = receivable ? "Ageing: Receivables" : "Ageing: Payables";
  }

  async load(): Promise<void> {
    const rows = await api.ageing(this.asAt, this.receivable);
    this.heading(tr(this.title), tr("As at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const tbody = h("tbody");
    for (const r of rows) {
      tbody.append(
        this.pick(
          h("tr", h("td.strong", r.party), ...r.buckets.map((b) => amountCell(b)), h("td.num.strong", M.fmt(r.total))),
          () => partyStatement(this.shell, r.ledger_id, this.asAt),
          0,
          { kind: "ledger", id: r.ledger_id, label: r.party },
        ),
      );
    }
    if (!rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 6 }, tr("Nothing outstanding."))));
    const col = (i: number) => M.format(M.sum(rows.map((r) => M.of(r.buckets[i]))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Party")), h("th.num", tr("0–30 days")), h("th.num", tr("31–60 days")), h("th.num", tr("61–90 days")), h("th.num", tr("Over 90 days")), h("th.num", tr("Total")))),
        tbody,
        h("tfoot", h("tr", h("td", tr("Total")), ...[0, 1, 2, 3].map((i) => h("td.num", col(i))), h("td.num", M.format(M.sum(rows.map((r) => M.of(r.total))))))),
      ),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const d = await this.dateDialog(this.asAt);
    if (!d) return;
    this.asAt = d;
    await this.reload();
  }
}

/** Supplier bills unpaid for more than 45 days. */
export class MsmeOverdueReport extends Report {
  title = "MSME Overdue Payables";
  protected override period = "asat" as const;
  protected override noteable = true;
  constructor(shell: Shell, private asAt = shell.periodTo(), private days = 45) {
    super(shell);
  }

  async load(): Promise<void> {
    const bills = await api.overduePayables(this.asAt, this.days);
    this.heading(tr("MSME overdue payables (over 45 days)"), tr("As at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const tbody = h("tbody");
    for (const b of bills) {
      const age = b.date ? Math.round((Date.parse(this.asAt) - Date.parse(b.date)) / 86_400_000) : 0;
      tbody.append(
        this.pick(
          h("tr", h("td.strong", b.party), h("td", b.reference), h("td", b.date ? D.show(b.date) : ""), h("td.num.late", String(age)), h("td.num", M.format(M.abs(M.of(b.amount))))),
          () => partyStatement(this.shell, b.ledger_id, this.asAt),
          0,
          { kind: "ledger", id: b.ledger_id, label: b.party },
        ),
      );
    }
    if (!bills.length) tbody.append(h("tr.empty", h("td", { colSpan: 5 }, tr("No supplier bill is older than 45 days."))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Supplier")), h("th", tr("Bill")), h("th", tr("Bill date")), h("th.num", tr("Days since bill")), h("th.num", tr("Pending")))),
        tbody,
        h("tfoot", h("tr", h("td", { colSpan: 4 }, tr("Total")), h("td.num", M.format(M.sum(bills.map((b) => M.abs(M.of(b.amount)))))))),
      ),
      h(
        "div.note",
        tr("Every creditor bill unpaid after 45 days. For suppliers registered as micro or small enterprises, payment is due within 45 days, so check these first."),
      ),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const d = await this.dateDialog(this.asAt);
    if (!d) return;
    this.asAt = d;
    await this.reload();
  }
}

// ------------------------------------------------ import and rebase results

/** What a Tally import created and what it skipped, with reasons. */
export class ImportReportScreen extends Report {
  title = "Tally Import";
  constructor(shell: Shell, private r: import("../types").ImportSummary) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = this.r;
    this.heading(tr("Import from Tally"), r.files.map(ltr).join(tr(", ")));
    this.resetPicks();
    // `label` is English, shown through tr().
    const count = (label: string, n: number) => this.pick(h("tr", h("td", tr(label)), h("td.num", String(n))));
    const skipped = h("tbody");
    for (const s of r.skipped) skipped.append(this.pick(h("tr", h("td", tr(s.kind)), h("td.strong", s.name), h("td.wrap", { dir: "auto" }, s.reason))));
    if (!r.skipped.length) skipped.append(h("tr.empty", h("td", { colSpan: 3 }, tr("Nothing was skipped."))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Imported")), h("th.num", tr("Count")))),
        h(
          "tbody",
          count("Groups created", r.groups_created),
          count("Groups matched to the default chart", r.groups_matched),
          count("Ledgers created", r.ledgers_created),
          count("Opening balances set", r.opening_balances),
          count("Vouchers posted", r.vouchers_posted),
          count("Vouchers numbered by KhataOS", r.vouchers_numbered),
          count("Units created", r.units_created),
          count("Stock groups created", r.stock_groups_created),
          count("Stock items created", r.stock_items_created),
          count("Godowns created", r.godowns_created),
          count("Stock lines on vouchers", r.inventory_lines),
        ),
      ),
      h("div.tside-title", tr("Skipped ({n})", { n: r.skipped.length })),
      h("table.rtable", h("thead", h("tr", h("th", tr("Kind")), h("th", tr("Name")), h("th", tr("Reason")))), skipped),
      h("div.note", tr("Everything imported is saved and recorded in the audit trail. Fix the skipped items in Tally, or create them here.")),
    );
  }
}

/** After a rebase: what moved, what needs a look, where the old file went. */
export class RebaseReviewScreen extends Report {
  title = "Sync: Rebase Review";
  constructor(shell: Shell, private r: import("../types").RebaseReport) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = this.r;
    this.heading(tr("Your offline changes were rebased"), tr("Sent {n} change(s) to the server", { n: r.pushed }));
    this.resetPicks();
    const ren = h("tbody");
    for (const x of r.renumbered) ren.append(this.pick(h("tr", h("td", tr(KIND_LABEL[x.kind])), h("td", h("bdi", x.old)), h("td.strong", h("bdi", x.new)))));
    if (!r.renumbered.length) ren.append(h("tr.empty", h("td", { colSpan: 3 }, tr("No voucher needed a new number."))));
    const att = h("tbody");
    for (const x of r.needs_attention) att.append(this.pick(h("tr", h("td", String(x.seq)), h("td.wrap", { dir: "auto" }, x.reason))));
    if (!r.needs_attention.length) att.append(h("tr.empty", h("td", { colSpan: 2 }, tr("Every change applied cleanly."))));
    this.bodyEl.replaceChildren(
      h("div.tside-title", tr("Renumbered vouchers")),
      h("table.rtable", h("thead", h("tr", h("th", tr("Voucher type")), h("th", tr("Old number")), h("th", tr("New number")))), ren),
      h("div.tside-title", tr("Needs attention")),
      h("table.rtable", h("thead", h("tr", h("th", tr("Entry")), h("th", tr("Why it was not applied")))), att),
      h("div.note", tr("The file as it was before the rebase is kept unchanged at:")),
      h("div.pathline", r.archived),
    );
  }
}

// Schedule III (Companies Act 2013): Balance Sheet and Statement of Profit
// and Loss in the vertical format, current and previous period side by
// side, in the company's division (I, or II for Ind AS) and rounding unit;
// the notes to accounts; and the mapping of groups to line items. The
// statements and notes are khata_in::schedule3 and schedule3_notes
// (docs/india-schedule-3.md). Enter on a line drills into its group or its
// ledgers; F2 changes the dates (both periods); Ctrl+P prints.
import { api, schedule3Api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { inUse, t as tr } from "../i18n";
import * as M from "../money";
import type { Shell } from "../shell";
import type {
  Schedule3BalanceSheetV2 as BsData,
  Schedule3Figures,
  Schedule3LineOption,
  Schedule3Mapping as MappingData,
  Schedule3Note,
  Schedule3NoteRow,
  Schedule3ProfitAndLossV2 as PlData,
  Schedule3Source,
  Schedule3Statements,
  Schedule3StatementRow as Schedule3Row,
} from "../types";
import { GroupSummaryReport, LedgerStatementReport, Report } from "./reports";

/** The same date a year earlier (29 Feb becomes 28 Feb). */
function yearBefore(isoDate: string): string {
  const [y, m, d] = isoDate.split("-").map(Number);
  return D.iso(y - 1, m, d) ?? D.iso(y - 1, m, d - 1) ?? isoDate;
}

/** The previous balance-sheet date: the day before this financial year began. */
function previousYearEnd(asAt: string): string {
  return D.addDays(D.fyStart(asAt), -1);
}

function range(from: string, to: string): string {
  return tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

/** Nil figures show as "-", as on the printed statement. */
function fig(text: string): string {
  return M.of(text) === 0n ? "-" : M.fmt(text);
}

/** Stock rows open the Stock Summary (loaded lazily, as reports.ts does). */
function openStockSummary(shell: Shell, from: string, to: string): void {
  void import("./inventory").then((m) => shell.push(new m.StockSummaryReport(shell, from, to)));
}

const STOCK = new Set(["closing_stock", "opening_stock"]);

/** Period a statement covers: `from` null for a balance sheet (balances as at `to`). */
interface Span {
  from: string | null;
  to: string;
}

/** "Division II" in titles when the company follows Ind AS. */
function divisionTag(division: number): string {
  return division === 2 ? tr("Schedule III, Division II") : tr("Schedule III");
}

abstract class Schedule3Report extends Report {
  protected warnings: string[] = [];
  protected unit = "₹";

  /** Where Enter on a line goes: its group, the stock summary, or its ledgers. */
  protected drill(r: Schedule3Row, span: Span, heads: [string, string | null]): (() => void) | undefined {
    const sources = r.sources;
    const groups = sources.filter((s) => s.kind === "group");
    if (sources.length === 1 && groups.length === 1 && groups[0].whole_group && groups[0].group) {
      const g = groups[0].group;
      return () => void this.shell.push(new GroupSummaryReport(this.shell, g, span.from, span.to));
    }
    if (sources.length && sources.every((s) => STOCK.has(s.kind))) {
      return () => openStockSummary(this.shell, span.from ?? D.fyStart(span.to), span.to);
    }
    if (!r.ledgers.length && sources.every((s) => s.kind === "opening_difference")) return undefined;
    return () => void this.shell.push(new Schedule3LineDetail(this.shell, r, span, heads));
  }

  /** The vertical statement: Particulars, Note No., current and previous figures. */
  protected render(rows: Schedule3Row[], span: Span, heads: [string, string | null]): void {
    this.resetPicks();
    const cols = heads[1] === null ? 3 : 4;
    const tbody = h("tbody");
    for (const r of rows) {
      const indent = { style: { paddingInlineStart: `${8 + r.level * 16}px` } };
      // Line, heading and total labels are fixed text from khata_in::schedule3, so they have translations.
      const label = tr(r.label);
      const figures = (amount: string, previous: string) =>
        heads[1] === null ? [h("td.num", fig(amount))] : [h("td.num", fig(amount)), h("td.num", fig(previous))];
      if (r.kind === "heading") {
        tbody.append(h("tr.group", h("td", indent, label), h("td"), ...Array.from({ length: cols - 2 }, () => h("td"))));
      } else if (r.kind === "total") {
        // A loss (a negative profit or total) shows as one.
        tbody.append(h("tr.result", { class: M.of(r.amount) < 0n ? "loss" : "" }, h("td", indent, label), h("td"), ...figures(r.amount, r.previous)));
      } else {
        const odd = r.sources.some((s) => s.kind === "opening_difference");
        const line = h(
          "tr",
          { class: odd ? "warnrow" : r.kind === "detail" ? "muted" : "" },
          h("td", indent, label),
          h("td", { style: { textAlign: "center" } }, r.note === null ? "" : String(r.note)),
          ...figures(r.amount, r.previous),
        );
        tbody.append(this.pick(line, this.drill(r, span, heads)));
      }
    }
    const head = h(
      "tr",
      h("th", inUse("Particulars", tr("Particulars (figure)"))),
      h("th", { style: { textAlign: "center" } }, tr("Note No.")),
      h("th.num", heads[0]),
      heads[1] === null ? null : h("th.num", heads[1]),
    );
    const notes = this.warnings.length
      ? h("div.note", h("strong", tr("To classify before filing")), h("ul", ...this.warnings.map((w) => h("li", { dir: "auto" }, w))))
      : null;
    this.bodyEl.replaceChildren(h("div.note", tr("Figures in {unit}", { unit: this.unit })), h("table.rtable", h("thead", head), tbody), notes ?? "");
  }

  protected async printing(run: () => Promise<void>): Promise<void> {
    try {
      await run();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ------------------------------------------------------------ Balance Sheet

export class Schedule3BalanceSheet extends Schedule3Report {
  title = "Balance Sheet (Schedule III)";
  protected override period = "asat" as const;

  constructor(
    shell: Shell,
    private asAt = shell.periodTo(),
    /** Null: no comparative (a first year). */
    private previousAsAt: string | null = previousYearEnd(asAt),
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    const bs = (await api.schedule3BalanceSheet(this.asAt, this.previousAsAt)) as unknown as BsData;
    this.warnings = bs.warnings;
    this.unit = bs.unit;
    const asAt = (d: string) => tr("As at {date}", { date: D.show(d) });
    const sub = asAt(bs.as_at) + (bs.previous_as_at ? ` · ${tr("previous {date}", { date: D.show(bs.previous_as_at) })}` : "");
    this.heading(`${tr("Balance Sheet")} (${divisionTag(bs.division)})`, sub);
    const heads: [string, string | null] = [asAt(bs.as_at), bs.previous_as_at ? asAt(bs.previous_as_at) : null];
    this.render(bs.rows, { from: null, to: bs.as_at }, heads);
    if (!balanced(bs.total_equity_and_liabilities, bs.total_assets)) {
      this.shell.warn(tr("Equity and liabilities do not equal assets; check the opening balances."));
    }
  }

  override action(a: string): boolean {
    if (a !== "PrintCurrent") return super.action(a);
    void this.printing(() => api.printSchedule3BalanceSheet(this.asAt, this.previousAsAt));
    return true;
  }

  protected override async askPeriod(): Promise<void> {
    const ref = this.asAt;
    const v = await this.shell.dialog(
      "Change dates",
      [
        { label: "As at", value: D.editable(this.asAt) },
        { label: "Previous year as at (blank: none)", value: this.previousAsAt ? D.editable(this.previousAsAt) : "" },
      ],
      ([a, p]) => {
        const at = D.parseUser(a, ref);
        if (!at) return tr('"{text}" is not a date.', { text: a });
        if (p.trim() === "") return null;
        const prev = D.parseUser(p, at);
        if (!prev) return tr('"{text}" is not a date.', { text: p });
        return prev >= at ? tr("The previous date must be before the current one.") : null;
      },
    );
    if (!v) return;
    this.asAt = D.parseUser(v[0], ref)!;
    this.previousAsAt = v[1].trim() === "" ? null : D.parseUser(v[1], this.asAt);
    this.cursor = 0;
    await this.reload();
  }
}

function balanced(a: Schedule3Figures, b: Schedule3Figures): boolean {
  return M.of(a.current) === M.of(b.current) && M.of(a.previous) === M.of(b.previous);
}

// ------------------------------------------- Statement of Profit and Loss

export class Schedule3ProfitLoss extends Schedule3Report {
  title = "Statement of Profit and Loss (Schedule III)";
  protected override period = "range" as const;
  private prevFrom: string | null;
  private prevTo: string | null;

  constructor(
    shell: Shell,
    private from = shell.periodFrom(),
    private to = shell.periodTo(),
    previous: [string, string] | null = [yearBefore(from), yearBefore(to)],
  ) {
    super(shell);
    [this.prevFrom, this.prevTo] = previous ?? [null, null];
  }

  async load(): Promise<void> {
    const pl = (await api.schedule3ProfitAndLoss(this.from, this.to, this.prevFrom, this.prevTo)) as unknown as PlData;
    this.warnings = pl.warnings;
    this.unit = pl.unit;
    const prev = pl.previous_from && pl.previous_to ? range(pl.previous_from, pl.previous_to) : null;
    this.heading(`${tr("Statement of Profit and Loss")} (${divisionTag(pl.division)})`, range(pl.from, pl.to) + (prev ? ` · ${tr("previous {date}", { date: prev })}` : ""));
    this.render(pl.rows, { from: pl.from, to: pl.to }, [range(pl.from, pl.to), prev]);
  }

  override action(a: string): boolean {
    if (a !== "PrintCurrent") return super.action(a);
    void this.printing(() => api.printSchedule3ProfitAndLoss(this.from, this.to, this.prevFrom, this.prevTo));
    return true;
  }

  protected override async askPeriod(): Promise<void> {
    const ref = this.to;
    const show = (d: string | null) => (d ? D.editable(d) : "");
    const v = await this.shell.dialog(
      "Change periods",
      [
        { label: "From", value: D.editable(this.from) },
        { label: "To", value: D.editable(this.to) },
        { label: "Previous from (blank: none)", value: show(this.prevFrom) },
        { label: "Previous to", value: show(this.prevTo) },
      ],
      ([a, b, c, d]) => {
        const f = D.parseUser(a, ref);
        const t = D.parseUser(b, ref);
        if (!f) return tr('"{text}" is not a date.', { text: a });
        if (!t) return tr('"{text}" is not a date.', { text: b });
        if (f > t) return tr("The period ends before it starts.");
        if (c.trim() === "" && d.trim() === "") return null;
        const pf = D.parseUser(c, f);
        const pt = D.parseUser(d, t);
        if (!pf) return tr('"{text}" is not a date.', { text: c });
        if (!pt) return tr('"{text}" is not a date.', { text: d });
        return pf > pt ? tr("The previous period ends before it starts.") : null;
      },
    );
    if (!v) return;
    this.from = D.parseUser(v[0], ref)!;
    this.to = D.parseUser(v[1], ref)!;
    const none = v[2].trim() === "" && v[3].trim() === "";
    this.prevFrom = none ? null : D.parseUser(v[2], this.from);
    this.prevTo = none ? null : D.parseUser(v[3], this.to);
    this.cursor = 0;
    await this.reload();
  }
}

// ----------------------------------------------------- line drill-down

/** The ledgers (and stock or profit figures) behind one Schedule III line. */
class Schedule3LineDetail extends Report {
  title: string;

  constructor(
    shell: Shell,
    private row: Schedule3Row,
    private span: Span,
    private heads: [string, string | null],
  ) {
    super(shell);
    this.title = row.label;
  }

  private openSource(s: Schedule3Source): (() => void) | undefined {
    const { from, to } = this.span;
    if (STOCK.has(s.kind)) return () => openStockSummary(this.shell, from ?? D.fyStart(to), to);
    if (s.kind === "profit_for_the_year") return () => void this.shell.push(new Schedule3ProfitLoss(this.shell, D.fyStart(to), to));
    return undefined;
  }

  async load(): Promise<void> {
    const r = this.row;
    const sub = this.span.from ? range(this.span.from, this.span.to) : tr("As at {date}", { date: D.show(this.span.to) });
    this.heading(r.note === null ? tr(r.label) : tr("Note {n}: {label}", { n: r.note, label: tr(r.label) }), sub);
    this.resetPicks();
    const two = this.heads[1] !== null;
    const figures = (a: string, p: string) => (two ? [h("td.num", fig(a)), h("td.num", fig(p))] : [h("td.num", fig(a))]);
    const tbody = h("tbody");
    for (const s of r.sources.filter((s) => s.kind !== "group")) {
      // Not "group": a computed figure with a fixed name ("Closing stock", "Surplus: profit for the year").
      tbody.append(this.pick(h("tr", h("td.strong", tr(s.name)), h("td.muted", ""), ...figures(s.amount, s.previous)), this.openSource(s)));
    }
    const stmtFrom = this.span.from ?? D.fyStart(this.span.to);
    for (const l of r.ledgers) {
      tbody.append(
        this.pick(
          h("tr", h("td.strong", l.name), h("td.muted", l.group_name), ...figures(l.amount, l.previous)),
          () => void this.shell.push(new LedgerStatementReport(this.shell, l.ledger, stmtFrom, this.span.to)),
        ),
      );
    }
    if (!tbody.children.length) tbody.append(h("tr.empty", h("td", { colSpan: two ? 4 : 3 }, tr("Nothing behind this line."))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Particulars")), h("th", tr("Group")), h("th.num", this.heads[0]), two ? h("th.num", this.heads[1]!) : null)),
        tbody,
        h("tfoot", h("tr", h("td", tr("Total")), h("td"), ...figures(r.amount, r.previous))),
      ),
    );
  }
}

// ------------------------------------------------------ notes to accounts

function cellText(c: Schedule3NoteRow["cells"][number]): string {
  switch (c.kind) {
    case "amount":
      return fig(c.value);
    case "ratio":
      return `${M.fmt(c.value)} ${tr("times")}`;
    case "percent":
      return `${M.fmt(c.value)}%`;
    case "text":
      return c.value;
    default:
      return "";
  }
}

/**
 * Notes to accounts as at a date (the year is the financial year to that
 * date), numbered as on the statements, then the ratios and the additional
 * regulatory information. Enter opens a note; F2 changes the dates; Ctrl+P
 * prints every note.
 */
export class Schedule3Notes extends Report {
  title = "Notes to Accounts (Schedule III)";
  protected override period = "asat" as const;

  constructor(
    shell: Shell,
    private asAt = shell.periodTo(),
    private previousAsAt: string | null = previousYearEnd(asAt),
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    const s: Schedule3Statements = await schedule3Api.statements(this.asAt, this.previousAsAt);
    const sub = tr("As at {date}", { date: D.show(s.as_at) }) + (s.previous_as_at ? ` · ${tr("previous {date}", { date: D.show(s.previous_as_at) })}` : "");
    this.heading(`${tr("Notes to Accounts")} (${divisionTag(s.division)})`, sub);
    this.resetPicks();
    const of = { balance_sheet: tr("Balance Sheet"), profit_and_loss: tr("Statement of Profit and Loss"), other: tr("Other disclosures") };
    const tbody = h("tbody");
    for (const n of s.notes) {
      const total = n.tables[0]?.rows.filter((r) => r.kind === "total" && r.level === 0).pop();
      const amount = total?.cells[0]?.kind === "amount" ? fig(total.cells[0].value) : "";
      const row = h("tr", h("td", { style: { textAlign: "center" } }, String(n.number)), h("td.strong", tr(n.title)), h("td.muted", of[n.of]), h("td.num", amount));
      tbody.append(this.pick(row, () => void this.shell.push(new Schedule3NoteDetail(this.shell, n, s, this.asAt))));
    }
    const warnings = s.warnings.length
      ? h("div.note", h("strong", tr("To classify before filing")), h("ul", ...s.warnings.map((w) => h("li", { dir: "auto" }, w))))
      : "";
    this.bodyEl.replaceChildren(
      h("div.note", tr("Figures in {unit}", { unit: s.unit })),
      h("table.rtable", h("thead", h("tr", h("th", tr("Note No.")), h("th", tr("Particulars")), h("th", tr("Statement")), h("th.num", tr("Amount")))), tbody),
      warnings,
    );
  }

  override action(a: string): boolean {
    if (a !== "PrintCurrent") return super.action(a);
    void schedule3Api.printNotes(this.asAt, this.previousAsAt).catch((e) => this.shell.error(e));
    return true;
  }

  protected override async askPeriod(): Promise<void> {
    const ref = this.asAt;
    const v = await this.shell.dialog(
      "Change dates",
      [
        { label: "As at", value: D.editable(this.asAt) },
        { label: "Previous year as at (blank: none)", value: this.previousAsAt ? D.editable(this.previousAsAt) : "" },
      ],
      ([a, p]) => {
        const at = D.parseUser(a, ref);
        if (!at) return tr('"{text}" is not a date.', { text: a });
        if (p.trim() === "") return null;
        const prev = D.parseUser(p, at);
        if (!prev) return tr('"{text}" is not a date.', { text: p });
        return prev >= at ? tr("The previous date must be before the current one.") : null;
      },
    );
    if (!v) return;
    this.asAt = D.parseUser(v[0], ref)!;
    this.previousAsAt = v[1].trim() === "" ? null : D.parseUser(v[1], this.asAt);
    this.cursor = 0;
    await this.reload();
  }
}

/** One note: its tables and what the company must still supply. Enter on a ledger row opens the ledger. */
class Schedule3NoteDetail extends Report {
  title: string;

  constructor(
    shell: Shell,
    private note: Schedule3Note,
    private s: Schedule3Statements,
    private asAt: string,
  ) {
    super(shell);
    this.title = note.title;
  }

  async load(): Promise<void> {
    const n = this.note;
    this.heading(tr("Note {n}: {label}", { n: n.number, label: tr(n.title) }), tr("Figures in {unit}", { unit: this.s.unit }));
    this.resetPicks();
    const from = D.fyStart(this.asAt);
    const parts: (HTMLElement | string)[] = [];
    for (const t of n.tables) {
      if (t.caption) parts.push(h("div.note", h("strong", t.caption)));
      const tbody = h("tbody");
      for (const r of t.rows) {
        const indent = { style: { paddingInlineStart: `${8 + r.level * 16}px` } };
        const cls = r.kind === "heading" ? "tr.group" : r.kind === "total" ? "tr.result" : "tr";
        const cells = r.kind === "heading" ? t.columns.map(() => h("td")) : r.cells.map((c) => (c.kind === "text" ? h("td", { dir: "auto" }, c.value) : h("td.num", cellText(c))));
        const row = h(cls, h("td", indent, tr(r.label)), ...cells);
        const ledger = r.ledger;
        tbody.append(this.pick(row, ledger ? () => void this.shell.push(new LedgerStatementReport(this.shell, ledger, from, this.asAt)) : undefined));
      }
      parts.push(h("table.rtable", h("thead", h("tr", h("th", tr("Particulars")), ...t.columns.map((c) => h("th.num", tr(c))))), tbody));
    }
    if (n.remarks.length) parts.push(h("div.note", h("strong", tr("To be supplied by the company")), h("ul", ...n.remarks.map((m) => h("li", { dir: "auto" }, m)))));
    if (!parts.length) parts.push(h("div.note", tr("Nothing behind this note.")));
    this.bodyEl.replaceChildren(...parts);
  }
}

// ---------------------------------------------------------------- mapping

/** A list to choose one entry from; Enter picks it and closes the list. */
class Schedule3Choice extends Report {
  constructor(
    shell: Shell,
    public title: string,
    private sub: string,
    private items: { label: string; sub?: string; current?: boolean; run: () => Promise<unknown> }[],
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    this.heading(tr(this.title), this.sub);
    this.resetPicks();
    const tbody = h("tbody");
    this.items.forEach((it, i) => {
      if (it.current) this.cursor = i;
      const row = h("tr", h("td.strong", (it.current ? "● " : "") + tr(it.label)), h("td.muted", it.sub ?? ""));
      tbody.append(
        this.pick(row, () => {
          void it
            .run()
            .then(() => this.shell.pop())
            .catch((e) => this.shell.error(e));
        }),
      );
    });
    this.bodyEl.replaceChildren(h("table.rtable", tbody));
  }
}

/**
 * Where each group goes on the Schedule III statements, with the company's
 * division and rounding unit and its MSME suppliers. Enter on a group
 * chooses its line (or restores the default); on a party toggles MSME; on
 * the first two rows changes the division or the rounding unit.
 */
export class Schedule3Mapping extends Report {
  title = "Schedule III Mapping";

  async load(): Promise<void> {
    const m: MappingData = await schedule3Api.mapping();
    this.heading(tr("Schedule III Mapping"), `${divisionTag(m.division)} · ${tr("Figures in {unit}", { unit: m.unit })}`);
    this.resetPicks();
    const tbody = h("tbody");
    const division = m.division === 2 ? tr("Division II (Ind AS)") : tr("Division I (Accounting Standards)");
    tbody.append(
      this.pick(h("tr.group", h("td", tr("Format")), h("td", division), h("td")), () => {
        const next: 1 | 2 = m.division === 2 ? 1 : 2;
        void schedule3Api
          .setFormat(next, m.rounding)
          .then(() => this.reload())
          .catch((e) => this.shell.error(e));
      }),
      this.pick(h("tr.group", h("td", tr("Rounding")), h("td", m.unit), h("td.muted", tr("Total income under ₹100 crore: hundreds to millions; ₹100 crore or more: lakhs to crores"))), () => {
        const items = m.roundings.map((r) => ({
          label: r.unit,
          current: r.code === m.rounding,
          run: () => schedule3Api.setFormat(m.division, r.code),
        }));
        void this.shell.push(new Schedule3Choice(this.shell, "Rounding", tr("Schedule III requires rounding by total income"), items));
      }),
    );
    tbody.append(h("tr.group", h("td", tr("Group")), h("td", tr("Division I line")), h("td", tr("Division II line"))));
    for (const g of m.groups) {
      const indent = { style: { paddingInlineStart: `${8 + g.depth * 16}px` } };
      const how = g.code ? tr("(own)") : g.inherited_from ? tr("(from {group})", { group: g.inherited_from }) : g.fallback ? tr("(by nature)") : "";
      const row = h("tr", h("td", indent, g.name), h("td", `${tr(g.line)} ${how}`.trim()), h("td.muted", tr(g.line2)));
      tbody.append(this.pick(row, () => void this.chooseLine(g.group, g.name, g.code)));
    }
    if (m.parties.length) {
      tbody.append(h("tr.group", h("td", tr("Supplier")), h("td", tr("Micro or small enterprise (MSME)")), h("td", tr("Group"))));
      for (const p of m.parties) {
        const row = h("tr", h("td", p.name), h("td", p.msme ? tr("Yes") : tr("No")), h("td.muted", p.group_name));
        tbody.append(
          this.pick(row, () => {
            void schedule3Api
              .setMsme(p.ledger, !p.msme)
              .then(() => this.reload())
              .catch((e) => this.shell.error(e));
          }),
        );
      }
    }
    const warnings = m.warnings.length ? h("div.note", h("ul", ...m.warnings.map((w) => h("li", { dir: "auto" }, w)))) : "";
    this.bodyEl.replaceChildren(h("table.rtable", tbody), warnings);
  }

  private async chooseLine(group: string, name: string, code: string | null): Promise<void> {
    try {
      const options: Schedule3LineOption[] = await schedule3Api.lineOptions(group);
      const items = [
        { label: "Default (as the parent group or the standard mapping)", current: code === null, run: () => schedule3Api.setMapping(group, null) },
        ...options.map((o) => ({
          label: o.label,
          sub: o.division === 2 ? tr("Division II only; Division I shows its equivalent") : "",
          current: o.code === code,
          run: () => schedule3Api.setMapping(group, o.code),
        })),
      ];
      await this.shell.push(new Schedule3Choice(this.shell, "Schedule III line", name, items));
    } catch (e) {
      this.shell.error(e);
    }
  }
}

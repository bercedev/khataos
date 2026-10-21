// Ratio Analysis and the Dashboard: key figures as tiles and four charts
// drawn as inline SVG (no chart library). Charts redraw to the window width;
// every chart also has a table of the same figures (Ctrl+H shows it).
import "../analysis.css";
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { dir, inUse, language, ltr, t as tr } from "../i18n";
import * as M from "../money";
import type { Btn, KeyEv, Shell } from "../shell";
import type { Dashboard, ExportTable, MonthPoint, NamedAmount } from "../types";
import { StockSummaryReport } from "./inventory";
import { BalanceSheetReport, GroupSummaryReport, OutstandingReport, ProfitLossReport, Report, TrialBalanceReport } from "./reports";

function range(from: string, to: string): string {
  return from === to ? D.show(from) : tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

/** Right-to-left text that mixes a number with Arabic words ("45 يومًا"), kept in order inside a left-to-right amount cell. */
function rtlIsland(s: string): string {
  return language() === "ar" ? `\u2067${s}\u2069` : s;
}

/** Default group ids by lower-cased name, for drilling into a group summary. */
async function groupIds(): Promise<Map<string, string>> {
  const groups = await api.searchGroups("");
  return new Map(groups.map((g) => [g.name.toLowerCase(), g.id]));
}

// ---------------------------------------------------------- Ratio Analysis

/** "1.53" → "1.53"; null (nothing to divide by) → "—". */
function ratioText(text: string | null, suffix = ""): string {
  return text === null ? "—" : `${M.format(M.of(text))}${suffix}`;
}

function daysText(text: string | null): string {
  return text === null ? "—" : rtlIsland(tr("{n} days", { n: M.toDecimal(M.of(text)).split(".")[0] }));
}

export class RatioAnalysisReport extends Report {
  title = "Ratio Analysis";
  protected override period = "range" as const;
  constructor(shell: Shell, private from = shell.periodFrom(), private to = shell.periodTo()) {
    super(shell);
  }

  async load(): Promise<void> {
    const [r, ids] = await Promise.all([api.ratioAnalysis(this.from, this.to), groupIds()]);
    this.heading(tr("Ratio Analysis"), range(this.from, this.to));
    this.resetPicks();
    const sh = this.shell;
    // Balances are as at the end of the period; sales and purchases are flows.
    const group = (name: string, flow = false) => {
      const id = ids.get(name.toLowerCase());
      return id ? () => void sh.push(new GroupSummaryReport(sh, id, flow ? this.from : null, this.to)) : undefined;
    };
    const pl = () => void sh.push(new ProfitLossReport(sh, this.from, this.to));
    const row = (col: number, label: string, value: string, hint: string, open?: () => void) =>
      this.pick(h("tr", h("td", tr(label), hint ? h("span.muted", ` · ${tr(hint)}`) : null), h("td.num", { class: value === "—" ? "muted" : "" }, value)), open, col);
    const amount = (text: string) => M.format(M.of(text));
    const net = M.of(r.net_profit);

    const groups = [
      row(0, "Working Capital", amount(r.working_capital), "Current Assets less Current Liabilities", () => void sh.push(new BalanceSheetReport(sh, this.to))),
      row(0, "Cash-in-Hand", amount(r.cash_in_hand), "", group("Cash-in-Hand")),
      row(0, "Bank Accounts", amount(r.bank_accounts), "", group("Bank Accounts")),
      row(0, "Bank OD A/c", amount(r.bank_od), "", group("Bank OD A/c")),
      row(0, "Sundry Debtors", amount(r.sundry_debtors), "", group("Sundry Debtors")),
      row(0, "Sundry Creditors", amount(r.sundry_creditors), "", group("Sundry Creditors")),
      row(0, "Sales Accounts", amount(r.sales), "", group("Sales Accounts", true)),
      row(0, "Purchase Accounts", amount(r.purchases), "", group("Purchase Accounts", true)),
      row(0, "Stock-in-Hand", amount(r.stock_in_hand), "", () => void sh.push(new StockSummaryReport(sh))),
      row(0, net < 0n ? "Net Loss" : "Net Profit", M.format(M.abs(net)), "", pl),
      row(0, "Working Capital Turnover", ratioText(r.working_capital_turnover), "Sales ÷ Working Capital"),
      row(0, "Inventory Turnover", ratioText(r.inventory_turnover), "Sales ÷ Closing Stock"),
    ];
    const ratios = [
      row(1, "Current Ratio", ratioText(r.current_ratio, " : 1"), "Current Assets : Current Liabilities"),
      row(1, "Quick Ratio", ratioText(r.quick_ratio, " : 1"), "Current Assets less Stock : Current Liabilities"),
      row(1, "Debt/Equity Ratio", ratioText(r.debt_equity, " : 1"), "Loans : Capital and Profit"),
      row(1, "Gross Profit %", ratioText(r.gross_profit_pct, " %"), "of Sales", pl),
      row(1, "Net Profit %", ratioText(r.net_profit_pct, " %"), "of Sales", pl),
      row(1, "Operating Cost %", ratioText(r.operating_cost_pct, " %"), "Sales less Net Profit, of Sales"),
      row(1, "Receivable Turnover", daysText(r.receivable_days), "Debtors ÷ Sales × days", () => void sh.push(new OutstandingReport(sh, true, this.to))),
      row(1, "Return on Investment %", ratioText(r.return_on_investment_pct, " %"), "Net Profit of Capital and Profit"),
      row(1, "Return on Working Capital %", ratioText(r.return_on_working_capital_pct, " %"), "Net Profit of Working Capital"),
    ];
    const side = (title: string, rows: HTMLElement[]) =>
      h(
        "div.tside",
        h("div.tside-title", tr(title)),
        h("table.rtable.ratios", h("thead", h("tr", h("th", inUse("Particulars", tr("Particulars (figure)"))), h("th.num", tr("Value")))), h("tbody", ...rows)),
      );
    this.bodyEl.replaceChildren(
      h("div.tformat", side("Principal Groups", groups), side("Principal Ratios", ratios)),
      h(
        "div.note",
        tr("Balances are as at {date}; sales, purchases and profit are for the period. A ratio shows — when the figure it divides by is zero.", { date: D.show(this.to) }),
      ),
    );
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    await this.reload();
  }
}

// ------------------------------------------------------- number formatting

/** Lakh/crore abbreviations for Indian companies (money.ts groups Indian-style). */
function indianNumbers(shell: Shell): boolean {
  const code = shell.company?.country?.code;
  return !code || code === "IN";
}

/**
 * Axis and bar labels: 1.2L / 3.4Cr (Indian) or 1.2K / 3.4M. Floats are for drawing only.
 * Arabic writes the unit as a word after the number ("1.2 ألف"), the number
 * kept left to right so a minus stays in front.
 */
function compact(v: number, indian: boolean): string {
  const a = Math.abs(v);
  const sign = v < 0 ? "-" : "";
  const units: [number, string, string][] = indian
    ? [[1e7, "Cr", tr("Cr (crore, short)")], [1e5, "L", tr("L (lakh, short)")], [1e3, "K", tr("K (thousand, short)")]]
    : [[1e9, "B", tr("B (billion, short)")], [1e6, "M", tr("M (million, short)")], [1e3, "K", tr("K (thousand, short)")]];
  const trim = (x: number) => (x >= 100 ? Math.round(x).toString() : x.toFixed(1).replace(/\.0$/, ""));
  const word = language() === "ar";
  for (const [n, s, w] of units) if (a >= n) return word && !w.includes("(") ? `${ltr(sign + trim(a / n))} ${w}` : sign + trim(a / n) + s;
  return word ? ltr(sign + trim(a)) : sign + trim(a);
}

/** Drawing value of a decimal string (charts only; figures stay exact elsewhere). */
function num(text: string): number {
  return Number(text) || 0;
}

/** Round axis bounds and 3–5 tick values covering [lo, hi]. */
function niceScale(lo: number, hi: number): { min: number; max: number; ticks: number[] } {
  lo = Math.min(0, lo);
  hi = Math.max(0, hi);
  if (hi === lo) hi = lo + 1;
  const raw = (hi - lo) / 4;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const f = raw / mag;
  const step = (f >= 7.5 ? 10 : f >= 3.5 ? 5 : f >= 1.5 ? 2 : 1) * mag;
  const min = Math.floor(lo / step) * step;
  const max = Math.ceil(hi / step) * step;
  const ticks: number[] = [];
  for (let t = min; t <= max + step / 2; t += step) ticks.push(Math.abs(t) < step / 1e6 ? 0 : t);
  return { min, max, ticks };
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
/** Arabic has no short month names: the full names, as in dates.ts. */
const MONTHS_AR = ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"];

function monthNames(): string[] {
  return language() === "ar" ? MONTHS_AR : MONTHS;
}

function monthLabel(isoMonth: string, withYear: boolean): string {
  const [y, m] = isoMonth.split("-").map(Number);
  return withYear ? `${monthNames()[m - 1]} ${String(y).slice(2)}` : monthNames()[m - 1];
}

function monthName(isoMonth: string): string {
  const [y, m] = isoMonth.split("-").map(Number);
  return `${monthNames()[m - 1]} ${y}`;
}

/** "Sales 1.2L" in a tooltip; the figure kept left to right in Arabic. */
function tipLine(label: string, value: string): string {
  return `${label} ${ltr(value)}`;
}

/** Per-character width of chart text, for sizing margins and cutting labels (Arabic text is 14px). */
function charW(): number {
  return language() === "ar" ? 7.8 : 7.2;
}

// ------------------------------------------------------------- SVG helpers

const SVG_NS = "http://www.w3.org/2000/svg";
type SvgChild = SVGElement | string | null | false;

function s<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number> = {}, ...children: SvgChild[]): SVGElementTagNameMap[K] {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, String(v));
  for (const c of children) if (c) el.append(c);
  return el;
}

/** Native hover tooltip for a mark. */
function tip(text: string): SVGTitleElement {
  return s("title", {}, text);
}

/**
 * A bar from the baseline `base` to `end` along y (columns) or x (bars):
 * 4px rounded at the data end, square at the baseline.
 */
function barPath(horizontal: boolean, across: number, thick: number, base: number, end: number): string {
  const len = Math.abs(end - base);
  if (len < 0.5) return "";
  const r = Math.min(4, thick / 2, len);
  const dir = end >= base ? 1 : -1;
  if (horizontal) {
    const [y0, y1] = [across, across + thick];
    const e = end - dir * r;
    return `M${base},${y0}H${e}Q${end},${y0} ${end},${y0 + r}V${y1 - r}Q${end},${y1} ${e},${y1}H${base}Z`;
  }
  const [x0, x1] = [across, across + thick];
  const e = end - dir * r;
  return `M${x0},${base}V${e}Q${x0},${end} ${x0 + r},${end}H${x1 - r}Q${x1},${end} ${x1},${e}V${base}Z`;
}

/** Cut a label to roughly `px` wide at 13px (≈7.2px per character; 14px in Arabic). */
function fit(text: string, px: number): string {
  const n = Math.max(3, Math.floor(px / charW()));
  const cut = text.length <= n ? text : `${text.slice(0, n - 1)}…`;
  // A name from the books keeps its own direction (and its "…" at its end) in Arabic.
  return language() === "ar" ? `⁨${cut}⁩` : cut;
}

interface Series {
  key: string;
  label: string;
  /** CSS class carrying the colour (analysis.css). */
  cls: string;
  values: number[];
}

/**
 * The chart's x for a left-to-right layout position: mirrored in Arabic, so
 * the value axis sits on the right (the inline start), months run right to
 * left and bars grow leftward, like the rest of the window. Text anchors are
 * left as they are: on an svg marked `direction="rtl"`, "start" is the
 * text's right edge and "end" its left edge, so they mirror too.
 */
function mirror(width: number): { x: (x: number) => number; attrs: Record<string, string>; rtl: boolean } {
  const rtl = dir() === "rtl";
  return { x: (x) => (rtl ? width - x : x), attrs: rtl ? { direction: "rtl" } : {}, rtl };
}

/** Visible characters of a label (the left-to-right isolates take no room). */
function visibleLength(text: string): number {
  return text.replace(/[\u2066-\u2069]/g, "").length;
}

/**
 * Month columns (grouped bars) with an optional line on the same money axis.
 * One axis only: every series is an amount.
 */
function monthChart(width: number, months: MonthPoint[], bars: Series[], line: Series | null, indian: boolean, fmt: (v: number) => string): SVGSVGElement {
  const H = 260;
  const all = [...bars, ...(line ? [line] : [])].flatMap((x) => x.values);
  const sc = niceScale(Math.min(...all), Math.max(...all));
  const labels = sc.ticks.map((t) => compact(t, indian));
  const ar = language() === "ar";
  const left = Math.max(...labels.map(visibleLength)) * (ar ? 8.2 : 7.5) + 14;
  const [top, right, bottom] = [14, line ? 52 : 12, 30];
  const pw = Math.max(40, width - left - right);
  const ph = H - top - bottom;
  const y = (v: number) => top + ph - ((v - sc.min) / (sc.max - sc.min)) * ph;
  const band = pw / Math.max(1, months.length);
  const { x: X, attrs, rtl } = mirror(width);
  const svg = s("svg", { width, height: H, viewBox: `0 0 ${width} ${H}`, class: "chart", "aria-hidden": "true", ...attrs });

  for (const [i, t] of sc.ticks.entries()) {
    svg.append(
      s("line", { x1: X(left), x2: X(left + pw), y1: y(t), y2: y(t), class: t === 0 ? "axis" : "grid" }),
      s("text", { x: X(left - 8), y: y(t), class: "tick", "text-anchor": "end", "dominant-baseline": "middle" }, labels[i]),
    );
  }
  const withYear = months.length > 12 || new Set(months.map((m) => m.month.slice(0, 4))).size > 1;
  // Arabic month names are full words ("سبتمبر"), so they need more room.
  const every = Math.max(1, Math.ceil((withYear ? (ar ? 76 : 52) : ar ? 60 : 36) / band));
  const thick = Math.max(2, Math.min(24, (band * 0.72 - 2 * (bars.length - 1)) / Math.max(1, bars.length)));
  const groupW = bars.length * thick + (bars.length - 1) * 2;
  for (const [i, m] of months.entries()) {
    const x0 = left + i * band;
    const g = s("g", { class: "band" });
    g.append(s("rect", { x: rtl ? X(x0 + band) : x0, y: top, width: band, height: ph, class: "hit" }));
    for (const [k, b] of bars.entries()) {
      const across = x0 + (band - groupW) / 2 + k * (thick + 2);
      const d = barPath(false, rtl ? X(across + thick) : across, thick, y(0), y(b.values[i]));
      if (d) g.append(s("path", { d, class: b.cls }));
    }
    const parts = [...bars, ...(line ? [line] : [])].map((x) => tipLine(x.label, fmt(x.values[i])));
    g.append(tip(`${monthName(m.month)}\n${parts.join("\n")}`));
    svg.append(g);
    if (i % every === 0) {
      svg.append(s("text", { x: X(x0 + band / 2), y: H - 10, class: "tick", "text-anchor": "middle" }, monthLabel(m.month, withYear)));
    }
  }
  if (line && months.length) {
    const pts = line.values.map((v, i) => [X(left + i * band + band / 2), y(v)] as const);
    svg.append(s("path", { d: pts.map(([px, py], i) => `${i ? "L" : "M"}${px},${py}`).join(""), class: `line ${line.cls}` }));
    for (const [i, [px, py]] of pts.entries()) {
      svg.append(s("circle", { cx: px, cy: py, r: 4, class: `dot ${line.cls}` }, tip(`${monthName(months[i].month)}\n${tipLine(line.label, fmt(line.values[i]))}`)));
    }
    const [lx, ly] = pts[pts.length - 1];
    svg.append(s("text", { x: rtl ? lx - 8 : lx + 8, y: ly, class: "value", "dominant-baseline": "middle" }, compact(line.values[line.values.length - 1], indian)));
  }
  return svg;
}

/** Horizontal bars, one row per name, largest first; the value sits at the tip. */
function barList(width: number, rows: { name: string; value: number; exact: string }[], cls: string, indian: boolean): SVGSVGElement {
  const rowH = 30;
  const H = rows.length * rowH + 8;
  const labelW = Math.min(200, Math.max(90, width * 0.36));
  // Arabic values carry a unit word ("1.2 مليون").
  const valueW = language() === "ar" ? 84 : 64;
  const pw = Math.max(20, width - labelW - valueW - 12);
  const max = Math.max(1, ...rows.map((r) => r.value));
  const { x: X, attrs } = mirror(width);
  const svg = s("svg", { width, height: H, viewBox: `0 0 ${width} ${H}`, class: "chart", "aria-hidden": "true", ...attrs });
  const x0 = labelW + 8;
  svg.append(s("line", { x1: X(x0), x2: X(x0), y1: 2, y2: H - 4, class: "axis" }));
  for (const [i, r] of rows.entries()) {
    const yy = 4 + i * rowH;
    const len = (Math.max(0, r.value) / max) * pw;
    const g = s("g", { class: "band" });
    g.append(
      s("rect", { x: 0, y: yy, width, height: rowH, class: "hit" }),
      s("text", { x: X(labelW), y: yy + rowH / 2, class: "label", "text-anchor": "end", "dominant-baseline": "middle" }, fit(r.name, labelW)),
    );
    const d = barPath(true, yy + (rowH - 16) / 2, 16, X(x0), X(x0 + len));
    if (d) g.append(s("path", { d, class: cls }));
    g.append(
      s("text", { x: X(x0 + len + 6), y: yy + rowH / 2, class: "value", "dominant-baseline": "middle" }, compact(r.value, indian)),
      tip(`${r.name}\n${ltr(r.exact)}`),
    );
    svg.append(g);
  }
  return svg;
}

// ------------------------------------------------------------- Dashboard

interface Chart {
  id: string;
  title: string;
  /** Sentence read out instead of the drawing. */
  summary: string;
  legend: { label: string; cls: string }[];
  /** True when there is nothing to plot. */
  empty: string | null;
  draw(width: number): SVGSVGElement;
  table(): HTMLTableElement;
  /** Rows for print and CSV. */
  rows(): string[][];
  open: () => void;
  /** Wider charts take half the row, the others a third. */
  wide: boolean;
  el?: HTMLElement;
  plot?: HTMLElement;
  drawnAt?: number;
}

export class DashboardScreen extends Report {
  title = "Dashboard";
  protected override period = "range" as const;
  private charts: Chart[] = [];
  private tiles: { label: string; value: string }[] = [];
  /** Charts switched to their table view (kept across reloads). */
  private asTable = new Set<string>();
  // Redraw on the next frame: drawing inside the callback resizes what is
  // observed and makes the browser report a "ResizeObserver loop" error.
  private observer = new ResizeObserver((entries) => {
    const charts = entries.map((e) => this.charts.find((x) => x.plot === e.target));
    requestAnimationFrame(() => {
      for (const c of charts) if (c) this.drawChart(c);
    });
  });

  constructor(shell: Shell, private from = shell.periodFrom(), private to = shell.periodTo()) {
    super(shell);
    this.el.classList.add("dash");
  }

  async load(): Promise<void> {
    const [d, ids] = await Promise.all([api.dashboard(this.from, this.to), groupIds()]);
    this.heading(tr("Dashboard"), range(this.from, this.to));
    this.resetPicks();
    this.observer.disconnect();
    const sh = this.shell;
    const balanceOf = (name: string) => {
      const id = ids.get(name.toLowerCase());
      return () => void sh.push(id ? new GroupSummaryReport(sh, id, null, this.to) : new TrialBalanceReport(sh, this.to));
    };
    const pl = () => void sh.push(new ProfitLossReport(sh, this.from, this.to));
    const receivables = () => void sh.push(new OutstandingReport(sh, true, this.to));
    const payables = () => void sh.push(new OutstandingReport(sh, false, this.to));
    const gross = M.of(d.gross_profit);
    const net = M.of(d.net_profit);

    // First row: position at the end of the period; second: the period's results.
    const asAt = tr("As at {date}", { date: D.show(this.to) });
    const period = tr("For the period");
    const plName = tr("Profit & Loss");
    const tiles: [string, string, string, () => void, string, boolean][] = [
      [tr("Cash-in-Hand"), d.cash_in_hand, asAt, balanceOf("Cash-in-Hand"), tr("Cash ledgers"), false],
      [tr("Bank balance"), d.bank_balance, asAt, balanceOf("Bank Accounts"), tr("Bank ledgers"), false],
      [tr("Receivables"), d.receivables, tr("Owed by debtors"), receivables, tr("Outstanding receivables"), false],
      [tr("Payables"), d.payables, tr("Owed to creditors"), payables, tr("Outstanding payables"), false],
      [tr("Closing stock"), d.closing_stock, asAt, () => void sh.push(new StockSummaryReport(sh)), tr("Stock Summary"), false],
      [inUse("Sales", tr("Sales (figure)")), d.sales, period, pl, plName, false],
      [tr("Purchases"), d.purchases, period, pl, plName, false],
      [gross < 0n ? tr("Gross loss") : tr("Gross profit"), M.toDecimal(M.abs(gross)), period, pl, plName, gross < 0n],
      [net < 0n ? tr("Net loss") : tr("Net profit"), M.toDecimal(M.abs(net)), period, pl, plName, net < 0n],
    ];
    this.tiles = tiles.map(([label, value]) => ({ label, value: M.format(M.of(value)) }));
    const tileEls = tiles.map(([label, value, sub, open, target, loss]) => {
      return this.pick(
        h(
          "div.dash-tile",
          { role: "group", "aria-label": tr("{label}: {value}", { label, value: ltr(M.format(M.of(value))) }) },
          h("div.dash-label", label),
          h("div.dash-value", { class: loss ? "loss" : "" }, M.format(M.of(value))),
          h("div.dash-sub", sub),
          h("div.dash-hint", tr("Enter: {report}", { report: target })),
        ),
        open,
      );
    });

    this.charts = this.buildCharts(d, pl, receivables, payables, () => void sh.push(new TrialBalanceReport(sh, this.to)));
    const viewKey = sh.km.chordIn("ChangeView", ["Report"]) ?? "Ctrl+H";
    const chartEls = this.charts.map((c) => {
      c.plot = h("div.dash-plot");
      const table = c.table();
      table.classList.add("dash-table");
      c.el = this.pick(
        h(
          "figure.dash-chart",
          { class: c.wide ? "wide" : "", "aria-label": c.title },
          h(
            "figcaption.dash-chead",
            h("span.dash-ctitle", c.title),
            c.legend.length > 1 ? h("span.dash-legend", ...c.legend.map((l) => h("span.dash-key", h("i", { class: `sw ${l.cls}` }), l.label))) : null,
          ),
          c.plot,
          table,
          h("div.dash-hint", tr("Enter: open the report · {key}: chart or table", { key: ltr(viewKey) })),
        ),
        c.open,
      );
      this.applyView(c);
      return c.el;
    });
    this.bodyEl.replaceChildren(h("div.dash-tiles", ...tileEls), h("div.dash-charts", ...chartEls));
    for (const c of this.charts) this.observer.observe(c.plot!);
  }

  private buildCharts(d: Dashboard, pl: () => void, receivables: () => void, payables: () => void, cashBank: () => void): Chart[] {
    const indian = indianNumbers(this.shell);
    const exact = (v: string) => M.format(M.of(v));
    const fmtNum = (v: number) => compact(v, indian);
    const months = d.months;
    const monthTable = (cols: [string, (m: MonthPoint) => string][]) =>
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Month")), ...cols.map(([c]) => h("th.num", c)))),
        h("tbody", ...months.map((m) => h("tr", h("td", monthName(m.month)), ...cols.map(([, f]) => h("td.num", exact(f(m))))))),
      );
    const listTable = (head: string, rows: { name: string; amount: string }[], none: string) =>
      h(
        "table.rtable",
        h("thead", h("tr", h("th", head), h("th.num", tr("Amount")))),
        h("tbody", ...(rows.length ? rows.map((r) => h("tr", h("td", r.name), h("td.num", exact(r.amount)))) : [h("tr.empty", h("td", { colSpan: 2 }, none))])),
      );
    const total = (xs: string[]) => M.format(M.sum(xs.map((x) => M.of(x))));
    const nonZero = (xs: string[]) => xs.some((x) => M.of(x) !== 0n);

    const [salesL, purchL, inL, outL, balL] = [inUse("Sales", tr("Sales (figure)")), tr("Purchases"), tr("Money in"), tr("Money out"), tr("Balance")];
    const salesS: Series = { key: "sales", label: salesL, cls: "s1", values: months.map((m) => num(m.sales)) };
    const purchS: Series = { key: "purchases", label: purchL, cls: "s2", values: months.map((m) => num(m.purchases)) };
    const inS: Series = { key: "in", label: inL, cls: "s1", values: months.map((m) => num(m.inflow)) };
    const outS: Series = { key: "out", label: outL, cls: "s2", values: months.map((m) => num(m.outflow)) };
    const balS: Series = { key: "bal", label: tr("Cash & bank balance"), cls: "s3", values: months.map((m) => num(m.cash_bank_balance)) };
    const at = (m: MonthPoint, series: string) => tr("{month}: {series}", { month: monthName(m.month), series });
    // "Rent 12,000.00; Salaries 8,000.00": names from the books, figures kept left to right.
    const listed = (xs: { name: string; amount: string }[]) => xs.map((e) => `${e.name} ${ltr(exact(e.amount))}`).join(tr("; "));
    const noExpenses = tr("No expenses in this period.");
    const noDebtors = tr("No debtor owes anything.");
    const noCreditors = tr("Nothing is owed to creditors.");
    const named = (xs: NamedAmount[]) => xs.map((x) => ({ name: x.name, value: num(x.amount), exact: exact(x.amount) }));
    const last = months[months.length - 1];

    return [
      {
        id: "sales",
        title: tr("Sales and purchases by month"),
        summary: tr("Sales {sales}, purchases {purchases} over {n} month(s).", {
          sales: ltr(total(months.map((m) => m.sales))),
          purchases: ltr(total(months.map((m) => m.purchases))),
          n: months.length,
        }),
        legend: [salesS, purchS],
        empty: nonZero(months.flatMap((m) => [m.sales, m.purchases])) ? null : tr("No sales or purchases in this period."),
        draw: (w) => monthChart(w, months, [salesS, purchS], null, indian, fmtNum),
        table: () => monthTable([[salesL, (m) => m.sales], [purchL, (m) => m.purchases]]),
        rows: () => months.flatMap((m) => [[at(m, salesL), exact(m.sales)], [at(m, purchL), exact(m.purchases)]]),
        open: pl,
        wide: true,
      },
      {
        id: "cash",
        title: tr("Cash and bank: money in, money out, balance"),
        summary: tr("Money in {in}, money out {out}; balance at the end {balance}.", {
          in: ltr(total(months.map((m) => m.inflow))),
          out: ltr(total(months.map((m) => m.outflow))),
          balance: ltr(last ? exact(last.cash_bank_balance) : "0.00"),
        }),
        legend: [inS, outS, balS],
        empty: nonZero(months.flatMap((m) => [m.inflow, m.outflow, m.cash_bank_balance])) ? null : tr("No cash or bank movement in this period."),
        draw: (w) => monthChart(w, months, [inS, outS], balS, indian, fmtNum),
        table: () => monthTable([[inL, (m) => m.inflow], [outL, (m) => m.outflow], [balL, (m) => m.cash_bank_balance]]),
        rows: () =>
          months.flatMap((m) => [
            [at(m, inL), exact(m.inflow)],
            [at(m, outL), exact(m.outflow)],
            [at(m, balL), exact(m.cash_bank_balance)],
          ]),
        open: cashBank,
        wide: true,
      },
      {
        id: "expenses",
        title: tr("Expenses by group"),
        summary: d.expenses.length ? listed(d.expenses) : noExpenses,
        legend: [{ label: tr("Expenses"), cls: "s2" }],
        empty: d.expenses.length ? null : noExpenses,
        draw: (w) => barList(w, d.expenses.map((x) => ({ name: x.name, value: num(x.amount), exact: exact(x.amount) })), "s2", indian),
        table: () => listTable(tr("Group"), d.expenses, noExpenses),
        rows: () => d.expenses.map((e) => [e.name, exact(e.amount)]),
        open: pl,
        wide: false,
      },
      {
        id: "debtors",
        title: tr("Top debtors"),
        summary: d.top_debtors.length ? listed(d.top_debtors) : noDebtors,
        legend: [{ label: tr("Owed to us"), cls: "s1" }],
        empty: d.top_debtors.length ? null : noDebtors,
        draw: (w) => barList(w, named(d.top_debtors), "s1", indian),
        table: () => listTable(tr("Debtor"), d.top_debtors, noDebtors),
        rows: () => d.top_debtors.map((e) => [e.name, exact(e.amount)]),
        open: receivables,
        wide: false,
      },
      {
        id: "creditors",
        title: tr("Top creditors"),
        summary: d.top_creditors.length ? listed(d.top_creditors) : noCreditors,
        legend: [{ label: tr("Owed by us"), cls: "s2" }],
        empty: d.top_creditors.length ? null : noCreditors,
        draw: (w) => barList(w, named(d.top_creditors), "s2", indian),
        table: () => listTable(tr("Creditor"), d.top_creditors, noCreditors),
        rows: () => d.top_creditors.map((e) => [e.name, exact(e.amount)]),
        open: payables,
        wide: false,
      },
    ];
  }

  /** Draw at the plot's current width; skipped while the screen is hidden. */
  private drawChart(c: Chart): void {
    const plot = c.plot;
    if (!plot || this.asTable.has(c.id)) return;
    const w = Math.floor(plot.clientWidth);
    if (w <= 0 || (w === c.drawnAt && plot.firstChild)) return;
    c.drawnAt = w;
    if (c.empty) {
      plot.replaceChildren(h("div.dash-empty", c.empty));
      return;
    }
    const svg = c.draw(w);
    plot.replaceChildren(svg);
  }

  /** Chart or table: the table stays available to screen readers either way. */
  private applyView(c: Chart): void {
    const table = this.asTable.has(c.id);
    c.el?.classList.toggle("as-table", table);
    c.plot?.setAttribute("role", "img");
    c.plot?.setAttribute("aria-label", `${c.title}. ${c.summary}`);
    if (!table) {
      c.drawnAt = undefined;
      this.drawChart(c);
    }
  }

  private toggleView(): void {
    const c = this.charts.find((x) => x.el === this.picks[this.cursor]?.el);
    if (!c) {
      this.shell.info("Move to a chart to switch between chart and table.");
      return;
    }
    if (this.asTable.has(c.id)) this.asTable.delete(c.id);
    else this.asTable.add(c.id);
    this.applyView(c);
    this.paint();
  }

  /** Key figures and each chart's figures, as two columns. */
  protected override table(): ExportTable {
    const rows: string[][] = [[tr("Key figures").toUpperCase(), ""], ...this.tiles.map((t) => [t.label, t.value])];
    for (const c of this.charts) rows.push([c.title.toUpperCase(), ""], ...c.rows());
    return { title: tr("Dashboard"), subtitle: range(this.from, this.to), columns: [inUse("Particulars", tr("Particulars (figure)")), tr("Amount")], numeric: [1], rows };
  }

  override handles(a: string): boolean {
    return a === "ChangeView" || super.handles(a);
  }

  override bottom(): Btn[] {
    const bar = super.bottom();
    const view = this.shell.btn("ChangeView", this.contexts(), "Chart / Table");
    if (view) bar.splice(Math.max(0, bar.length - 1), 0, view);
    return bar;
  }

  override action(a: string): boolean {
    if (a === "ChangeView") {
      this.toggleView();
      return true;
    }
    return super.action(a);
  }

  override key(e: KeyEv): boolean {
    switch (e.chord) {
      case "Tab":
        this.step(1);
        return true;
      case "Shift+Tab":
        this.step(-1);
        return true;
      case "Up":
        return this.toward(0, -1);
      case "Down":
        return this.toward(0, 1);
      case "Left":
        return this.toward(-1, 0);
      case "Right":
        return this.toward(1, 0);
    }
    return super.key(e);
  }

  private step(delta: number): void {
    if (!this.picks.length) return;
    this.cursor = (this.cursor + delta + this.picks.length) % this.picks.length;
    this.paint();
  }

  /** Move to the nearest tile or chart in a direction, by on-screen position. */
  private toward(dx: number, dy: number): boolean {
    const cur = this.picks[this.cursor]?.el.getBoundingClientRect();
    if (!cur) return true;
    const [cx, cy] = [cur.left + cur.width / 2, cur.top + cur.height / 2];
    let best = -1;
    let bestScore = Infinity;
    this.picks.forEach((p, i) => {
      if (i === this.cursor) return;
      const r = p.el.getBoundingClientRect();
      // Only items wholly beyond the current one's edge in that direction.
      const along = dx > 0 ? r.left - cur.right : dx < 0 ? cur.left - r.right : dy > 0 ? r.top - cur.bottom : cur.top - r.bottom;
      if (along < -2) return;
      // Gap sideways (zero when the two overlap), then distance between centres.
      const across = dx
        ? Math.max(0, r.top - cur.bottom, cur.top - r.bottom)
        : Math.max(0, r.left - cur.right, cur.left - r.right);
      const centre = dx ? Math.abs(r.top + r.height / 2 - cy) : Math.abs(r.left + r.width / 2 - cx);
      const score = Math.max(0, along) + across * 3 + centre * 0.1;
      if (score < bestScore) [best, bestScore] = [i, score];
    });
    if (best >= 0) {
      this.cursor = best;
      this.paint();
    }
    return true;
  }

  dispose(): void {
    this.observer.disconnect();
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    await this.reload();
  }
}

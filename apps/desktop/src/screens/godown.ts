// Godowns (stock locations) and the stock they hold: the Godown master
// (Create / Alter), and the Godown and Batch summaries. Stock that was never
// put in a godown sits at the Main Location.
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { dir, inUse, ltr, t } from "../i18n";
import * as M from "../money";
import { ListPicker, type PickItem } from "../picker";
import type { Shell } from "../shell";
import type { Godown, LocationRow } from "../types";
import { StockItemVouchersReport } from "./inventory";
import { Report } from "./reports";
import { FieldForm, frow } from "./settings";
import { qtyText } from "./voucher";

export interface GodownFormOptions {
  /** Alter this godown instead of creating one. */
  alterId?: string;
  /** Stacked over the screen that asked for it (Alt+C), parent visible (L7). */
  inline?: boolean;
  /** Text typed in the calling list field. */
  name?: string;
  /** Called after save in inline mode; the form closes first. */
  onCreated?: (g: Godown) => void;
}

/** The Under line: where the chosen parent sits (Ma2). */
function parentLine(g: Godown | null): string {
  if (!g) return t("(top level)");
  return `(${g.parent_name ?? t("Primary")})`;
}

type Field = "name" | "under";

export class GodownForm extends FieldForm<Field> {
  title: string;
  private parent: Godown | null = null;
  private parentEl = h("div.nature");
  /** Altering: the godown itself and everything under it can't be its parent. */
  private excluded = new Set<string>();

  constructor(shell: Shell, private opts: GodownFormOptions = {}) {
    super(shell, ["name", "under"], { inline: opts.inline });
    this.title = opts.alterId ? "Godown Alteration" : "Godown Creation";
    this.inputs.name.value = opts.name ?? "";
    this.inputs.name.placeholder = t("e.g. Warehouse, Shop, Factory");
    this.inputs.under.value = t("Primary");
    this.parentEl.textContent = parentLine(null);
    this.layout(
      t(opts.alterId ? "Alter Godown" : "Create Godown"),
      [
        frow(t("Name"), this.inputs.name),
        frow(t("Under"), h("div.fstack", this.inputs.under, this.parentEl)),
        h("div.note", t("A godown is a place stock is kept. Stock not put in any godown stays at the Main Location.")),
      ],
    );
    this.pickers.under = new ListPicker<Godown>({
      input: this.inputs.under as HTMLInputElement,
      host: this.el,
      title: "Godowns",
      columns: ["Name", "Under"],
      source: async (q) =>
        (await api.listGodowns(q))
          .filter((g) => !this.excluded.has(g.id))
          .map<PickItem<Godown>>((g) => ({ id: g.id, label: g.name, sub: g.parent_name ?? t("Primary"), data: g })),
      actions: () => [{ id: "primary", label: t("Primary"), hint: t("top level"), run: () => this.pickParent(null) }],
      create: { noun: "godown", run: (t) => this.createParentInline(t) },
      selectedId: () => this.parent?.id ?? null,
      preferAction: () => (this.parent ? null : "primary"),
      onPick: (it) => this.pickParent(it.data),
    });
  }

  protected fields(): Field[] {
    return ["name", "under"];
  }

  protected override async load(): Promise<void> {
    if (!this.opts.alterId) return;
    const all = await api.listGodowns("");
    const me = all.find((g) => g.id === this.opts.alterId);
    if (!me) throw new Error(t("This godown no longer exists."));
    this.excluded = new Set([me.id]);
    for (let grew = true; grew; ) {
      grew = false;
      for (const g of all) {
        if (g.parent_id && this.excluded.has(g.parent_id) && !this.excluded.has(g.id)) {
          this.excluded.add(g.id);
          grew = true;
        }
      }
    }
    this.inputs.name.value = me.name;
    this.setParent(all.find((g) => g.id === me.parent_id) ?? null);
  }

  private setParent(g: Godown | null): void {
    this.parent = g;
    this.inputs.under.value = g?.name ?? t("Primary");
    this.parentEl.textContent = parentLine(g);
  }

  private pickParent(g: Godown | null): void {
    this.setParent(g);
    void this.confirm();
  }

  private createParentInline(text: string): void {
    void this.shell.push(new GodownForm(this.shell, { name: text, inline: true, onCreated: (g) => this.setParent(g) }));
  }

  protected override pickedText(): string {
    return this.parent?.name ?? t("Primary");
  }

  protected override createLabel(f: Field): string | null {
    return f === "under" ? "Create Godown" : null;
  }

  protected override canDelete(): boolean {
    return Boolean(this.opts.alterId);
  }

  protected next(f: Field): void {
    if (f === "name") {
      if (!this.inputs.name.value.trim()) return this.shell.warn(t("Enter a name for the godown."));
      return this.goTo("under");
    }
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const name = this.inputs.name.value.trim();
    if (!name) {
      this.goTo("name");
      return this.shell.warn(t("Enter a name for the godown."));
    }
    try {
      const g = await api.saveGodown(this.opts.alterId ?? "", name, this.parent?.id ?? "");
      this.shell.info(
        t(this.opts.alterId ? 'Godown "{name}" altered under {under}.' : 'Godown "{name}" created under {under}.', { name: g.name, under: g.parent_name ?? t("Primary") }),
      );
      if (this.opts.onCreated) {
        const cb = this.opts.onCreated;
        this.shell.pop();
        cb(g);
        return;
      }
      if (this.opts.alterId) {
        this.markClean();
        this.shell.pop();
        return;
      }
      // F7: a blank form for the next godown; the parent carries over.
      this.inputs.name.value = "";
      this.setParent(this.parent);
      this.markClean();
      this.go(0);
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected override async remove(): Promise<void> {
    const id = this.opts.alterId;
    if (!id) return;
    const name = this.inputs.name.value.trim();
    if (!(await this.shell.ask(t('Delete godown "{name}"?', { name }), { yesFirst: false }))) return;
    try {
      await api.deleteGodown(id);
      this.shell.info(t('Godown "{name}" deleted.', { name }));
      this.shell.pop();
    } catch (e) {
      const why = e instanceof Error ? e.message : String(e);
      this.shell.error(t('Can\'t delete godown "{name}": {why}.', { name, why: ltr(why) }));
    }
  }
}

// --------------------------------------------------------------- report

interface Totals {
  qty: M.Units;
  value: M.Units;
  /** The one unit every row shares, null when mixed (quantities not added). */
  unit: string | null;
}

function totalsOf(rows: LocationRow[]): Totals {
  const units = new Set(rows.map((r) => r.unit));
  return {
    qty: M.sum(rows.map((r) => M.of(r.qty))),
    value: M.sum(rows.map((r) => M.of(r.value))),
    unit: units.size === 1 ? [...units][0] : null,
  };
}

function qtyCell(t: Totals): HTMLElement {
  return h("td.num", t.unit === null || t.qty === 0n ? "" : `${qtyText(t.qty)}${t.unit ? ` ${t.unit}` : ""}`);
}

function valueCell(u: M.Units): HTMLElement {
  return h("td.num", u === 0n ? "" : M.format(u));
}

/** Where the row is: a godown's name, or the Main Location (a fixed label). */
function placeOf(r: LocationRow): string {
  return r.godown === null ? t(r.godown_name) : r.godown_name;
}

/** Rows grouped by a key, in first-seen order. */
function groupBy(rows: LocationRow[], key: (r: LocationRow) => string): Map<string, LocationRow[]> {
  const out = new Map<string, LocationRow[]>();
  for (const r of rows) {
    const k = key(r);
    out.set(k, [...(out.get(k) ?? []), r]);
  }
  return out;
}

/**
 * Stock on hand by location as at a date. "godown": each godown (Main
 * Location first) with its items and their batches; "batch": each item with
 * its batches and where they are. Enter on a godown or item folds it; Enter
 * on an item's line under it opens the item's vouchers.
 */
export class GodownSummaryReport extends Report {
  title: string;
  protected override period = "asat" as const;
  private rows: LocationRow[] = [];
  /** Folded heads (Enter on a head toggles). */
  private collapsed = new Set<string>();

  constructor(
    shell: Shell,
    private mode: "godown" | "batch" = "godown",
    private asAt = shell.periodTo(),
  ) {
    super(shell);
    this.title = mode === "godown" ? "Godown Summary" : "Batch Summary";
  }

  async load(): Promise<void> {
    this.rows = await api.stockByLocation(this.asAt);
    this.render();
  }

  private openItem(item: string): void {
    const from = this.shell.periodFromFor(this.asAt);
    void this.shell.push(new StockItemVouchersReport(this.shell, item, from < this.asAt ? from : this.asAt, this.asAt));
  }

  /** A head row that folds the rows under it. */
  private headRow(key: string, label: string, t: Totals, cols: number): HTMLElement {
    const folded = this.collapsed.has(key);
    const row = h("tr.group", h("td", { colSpan: cols - 2 }, `${folded ? (dir() === "rtl" ? "◂" : "▸") : "▾"} ${label}`), qtyCell(t), valueCell(t.value));
    row.dataset.key = key;
    row.setAttribute("aria-expanded", String(!folded));
    return this.pick(row, () => {
      if (folded) this.collapsed.delete(key);
      else this.collapsed.add(key);
      this.rerender();
    });
  }

  private rerender(): void {
    const at = this.picks[this.cursor]?.el.dataset.key;
    this.render();
    const back = this.picks.findIndex((p) => p.el.dataset.key === at);
    if (back >= 0) this.cursor = back;
    this.paint();
  }

  private render(): void {
    this.heading(t(this.title), t("Stock on hand as at {date}", { date: D.show(this.asAt) }));
    this.resetPicks();
    const tbody = h("tbody");
    const indent = (depth: number) => ({ style: { paddingInlineStart: `${10 + depth * 18}px` } });
    const cols = this.mode === "godown" ? 3 : 4;
    if (this.mode === "godown") {
      // Server order: Main Location first, then godowns by name, items by name.
      for (const [g, inGodown] of groupBy(this.rows, (r) => r.godown ?? "")) {
        const key = `g:${g}`;
        tbody.append(this.headRow(key, placeOf(inGodown[0]), totalsOf(inGodown), cols));
        if (this.collapsed.has(key)) continue;
        for (const [item, lines] of groupBy(inGodown, (r) => r.item)) {
          const row = h("tr", h("td.strong", indent(1), lines[0].item_name), qtyCell(totalsOf(lines)), valueCell(totalsOf(lines).value));
          row.dataset.key = `${key}:i:${item}`;
          tbody.append(this.pick(row, () => this.openItem(item)));
          if (!lines.some((l) => l.batch)) continue;
          for (const l of lines) {
            const sub = h("tr.sub", h("td", indent(2), l.batch ? t("Batch {batch}", { batch: ltr(l.batch) }) : t("No batch")), qtyCell(totalsOf([l])), valueCell(M.of(l.value)));
            tbody.append(this.pick(sub, () => this.openItem(item)));
          }
        }
      }
    } else {
      const byItem = [...groupBy(this.rows, (r) => r.item)].sort(([, a], [, b]) =>
        a[0].item_name.toLowerCase().localeCompare(b[0].item_name.toLowerCase()),
      );
      for (const [item, lines] of byItem) {
        const key = `i:${item}`;
        tbody.append(this.headRow(key, lines[0].item_name, totalsOf(lines), cols));
        if (this.collapsed.has(key)) continue;
        const sorted = [...lines].sort((a, b) => (a.batch ?? "").localeCompare(b.batch ?? "") || Number(a.godown !== null) - Number(b.godown !== null));
        for (const l of sorted) {
          const row = h(
            "tr",
            h("td", indent(1), l.batch ? l.batch : h("span.muted", t("No batch"))),
            h("td", placeOf(l)),
            qtyCell(totalsOf([l])),
            valueCell(M.of(l.value)),
          );
          tbody.append(this.pick(row, () => this.openItem(item)));
        }
      }
    }
    if (!this.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: cols }, t("No stock on hand on this date. Press F2 to change it."))));
    const grand = totalsOf(this.rows);
    const headCells =
      this.mode === "godown"
        ? [h("th", inUse("Particulars", t("Particulars (figure)")))]
        : [h("th", t("Item / Batch")), h("th", t("Location"))];
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", ...headCells, h("th.num", t("Quantity")), h("th.num", t("Value")))),
        tbody,
        h("tfoot", h("tr", h("td", { colSpan: cols - 2 }, t("Grand Total")), qtyCell(grand), valueCell(grand.value))),
      ),
      h("div.note", t("Values are at each item's weighted average cost. Quantities of different units are not added up.")),
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

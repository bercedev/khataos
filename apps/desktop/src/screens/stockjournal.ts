// Stock Journal (Alt+F7): stock out of one place and into another, with no
// party and usually no ledger. Two panels of item lines:
// - Source: what is consumed, or moved out of a godown.
// - Destination: what is produced, or moved into a godown.
// A transfer is the same item out of one godown and into another;
// manufacturing consumes raw materials and produces goods.
//
// Stock is valued at weighted average cost. Source lines default to the
// item's average cost; a single destination line carries the whole value
// consumed (so production costs what went into it) until the user types
// its rate or amount. Clearing a typed rate brings the default back.
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { language, ltr, t } from "../i18n";
import * as M from "../money";
import { ListPicker, type PickAction, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import {
  KIND_LABEL,
  KINDS,
  type Context,
  type EntryInput,
  type Godown,
  type StockItem,
  type StockMoveInput,
  type StockMoveView,
  type VoucherKind,
} from "../types";
import { VoucherScreen, lineAmount, placesOf, qtyText, rateOf } from "./voucher";
import "../inventory.css";
import "../voucher-extra.css";
import "../stockjournal.css";

/** "50 kg" as one left-to-right piece in Arabic, so the unit neither wraps nor moves before the figure. */
function qtyUnit(qty: string, unit: string): string {
  return language() === "ar" ? ltr(`${qty}\u00a0${unit}`) : `${qty} ${unit}`;
}

type Panel = "src" | "dst";
type LineKey = "item" | "godown" | "batch" | "qty" | "rate" | "value";
type Field = { k: LineKey; p: Panel; i: number } | { k: "narration" };

interface MoveLine {
  item: StockItem | null;
  qty: string;
  rate: string;
  amount: string;
  /** Godown the stock moves out of or into; "" is the main location. */
  godownId: string;
  godownName: string;
  batch: string;
  /** Rate not typed: it follows the default (average cost, or the value consumed). */
  auto: boolean;
}

const KIND: VoucherKind = "StockJournal";
const PANELS: Panel[] = ["src", "dst"];
const PANEL_LABEL: Record<Panel, string> = {
  src: "Source (consumption / transfer from)",
  dst: "Destination (production / transfer to)",
};
const MAIN_LOCATION = "Main Location";

function blankLine(): MoveLine {
  return { item: null, qty: "", rate: "", amount: "", godownId: "", godownName: "", batch: "", auto: true };
}

function lineDone(l: MoveLine): boolean {
  return l.item !== null && (M.parse(l.qty) ?? M.ZERO) > 0n;
}

function amountOf(l: MoveLine): M.Units {
  const u = M.parse(l.amount);
  return u === null ? M.ZERO : M.abs(u);
}

/** Weighted average cost of the stock on hand (exact, and to two decimals for display). */
function averageCost(s: StockItem): { exact: M.Units; shown: M.Units } | null {
  const q = M.of(s.closing_qty);
  const v = M.of(s.closing_value);
  if (q > 0n && v > 0n) return { exact: (v * M.of("1")) / q, shown: rateOf(v, q) };
  const opening = M.of(s.opening_rate);
  return opening > 0n ? { exact: opening, shown: opening } : null;
}

/** A saved voucher opened from a report: its own screen for a Stock Journal. */
export function savedVoucherScreen(shell: Shell, kind: VoucherKind, date: string, id: string): Screen {
  return kind === KIND ? new StockJournalScreen(shell, id) : new VoucherScreen(shell, kind, date, id);
}

export class StockJournalScreen implements Screen {
  el: HTMLElement;
  private number = "";
  private date: string;
  private lines: Record<Panel, MoveLine[]> = { src: [blankLine()], dst: [blankLine()] };
  /** The company's godowns; the Godown column shows when there is one. */
  private godowns: Godown[] = [];
  /** Batch column turned on (Ctrl+I); it also shows while any line has a batch. */
  private batchOn = false;
  /** Stock per item and location ("item|godown") as at the voucher date; null: unknown. */
  private located: Map<string, M.Units> | null = null;

  private at = 0;
  private picker: ListPicker<StockItem> | ListPicker<Godown> | null = null;
  private inputs = new Map<string, HTMLInputElement>();
  private hints = new Map<string, HTMLElement>();
  private panelTotal: Record<Panel, HTMLElement> = { src: h("span.num"), dst: h("span.num") };
  private totalsEl = h("div.vtotals");
  private narration: HTMLInputElement;
  private body = h("div.vbody.sj-body");
  private numberEl = h("span.vnum");
  private dateEl = h("span.vdate");
  private dayEl = h("span.vday");
  private badge = h("span.badge.kind-StockJournal", t(KIND_LABEL[KIND]));

  /** Set when altering a saved voucher (opened from a report). */
  private alterId: string | null;
  /** A cancelled voucher opens read-only. */
  private cancelled = false;
  /** Ledger lines saved with the voucher (an additional cost): kept as they are. */
  private kept: EntryInput[] = [];
  /** What the saved version moved, per "item|godown" (in positive), and its date. */
  private savedMoves = new Map<string, M.Units>();
  private savedDate: string | null = null;
  /** Altering: the voucher as loaded, so Esc without changes just leaves. */
  private loaded: string | null = null;

  constructor(
    private shell: Shell,
    alterId?: string,
    date?: string,
  ) {
    this.alterId = alterId ?? null;
    this.date = date ?? shell.company?.today ?? D.todayIso();
    this.narration = h("input.field.narration", { spellcheck: true, autocomplete: "off", placeholder: t("Narration") });
    this.narration.addEventListener("focus", () => this.onFocus(this.fields().length - 1));
    this.el = h(
      "section.voucher.stockjournal",
      h(
        "div.vmain",
        h("div.vhead", h("div.vhead-left", this.badge, this.numberEl), h("div.vhead-right", this.dateEl, this.dayEl)),
        this.body,
        h("div.vfoot", h("label.vnarr", h("span.flabel", t("Narration")), this.narration), this.totalsEl),
      ),
    );
  }

  get title(): string {
    const kind = t(KIND_LABEL[KIND]);
    return t(this.cancelled ? "{kind} Voucher (Cancelled)" : this.alterId ? "{kind} Voucher (Alteration)" : "{kind} Voucher", { kind });
  }

  get tint(): string {
    return `tint-${KIND}`;
  }

  async init(): Promise<void> {
    if (this.alterId) {
      await this.loadSaved(this.alterId);
      this.loaded = this.state();
    } else {
      const [number, godowns] = await Promise.all([api.nextVoucherNumber(KIND, this.date), this.loadGodowns(), this.loadLocations()]);
      this.number = number;
      this.godowns = godowns;
    }
    this.render();
  }

  /** Godowns, for the Godown column; none (or an older backend) hides it. */
  private loadGodowns(): Promise<Godown[]> {
    return api.listGodowns("").catch(() => []);
  }

  /** Stock by location as at the voucher date, for the closing stock hints. */
  private async loadLocations(): Promise<void> {
    try {
      const rows = await api.stockByLocation(this.date);
      const map = new Map<string, M.Units>();
      for (const r of rows) {
        const key = `${r.item}|${r.godown ?? ""}`;
        map.set(key, (map.get(key) ?? M.ZERO) + M.of(r.qty));
      }
      this.located = map;
    } catch {
      this.located = null; // hints fall back to the item's total stock
    }
  }

  private async loadSaved(id: string): Promise<void> {
    const [v, stock, godowns] = await Promise.all([api.getVoucher(id), api.listStockItems(""), this.loadGodowns()]);
    const byItem = new Map(stock.map((s) => [s.id, s]));
    this.godowns = godowns;
    this.number = v.number;
    this.date = v.date;
    this.narration.value = v.narration;
    this.cancelled = v.cancelled;
    this.batchOn = v.stock.some((m) => m.batch);
    for (const p of PANELS) {
      const moves = v.stock.filter((m) => m.inward === (p === "dst")).map((m) => this.lineOf(m, byItem.get(m.item_id)));
      this.lines[p] = moves.length ? moves : [blankLine()];
    }
    // A single product that carried exactly the value consumed keeps following it.
    const made = this.lines.dst.filter(lineDone);
    const used = this.total("src");
    if (made.length === 1 && used > 0n && amountOf(made[0]) === used) made[0].auto = true;
    this.kept = v.entries.map((e) => ({
      ledger_id: e.ledger_id,
      side: e.side,
      amount: e.amount,
      bills: e.bills,
      items: e.items.map((s) => ({
        item_id: s.item_id,
        qty: s.qty,
        rate: s.rate,
        amount: s.amount,
        ...(s.godown_id ? { godown_id: s.godown_id } : {}),
        ...(s.batch ? { batch: s.batch } : {}),
      })),
    }));
    if (!v.cancelled) {
      this.savedDate = v.date;
      for (const m of v.stock) {
        const key = `${m.item_id}|${m.godown_id ?? ""}`;
        const q = m.inward ? M.of(m.qty) : -M.of(m.qty);
        this.savedMoves.set(key, (this.savedMoves.get(key) ?? M.ZERO) + q);
      }
    }
    await this.loadLocations();
    this.at = 0;
  }

  /** A saved line, with its item master (a stand-in if it has gone). */
  private lineOf(m: StockMoveView, item: StockItem | undefined): MoveLine {
    const fallback: StockItem = {
      id: m.item_id,
      tax_class: null,
      costing: "AverageCost",
      name: m.item_name,
      group_id: null,
      group_name: "",
      unit_id: "",
      unit: m.unit,
      decimals: 4,
      opening_qty: "0",
      opening_value: "0",
      opening_rate: "0",
      closing_qty: "0",
      closing_value: "0",
    };
    return {
      item: item ?? fallback,
      qty: m.qty,
      rate: m.rate,
      amount: m.amount,
      godownId: m.godown_id ?? "",
      godownName: m.godown_name ?? "",
      batch: m.batch ?? "",
      auto: false,
    };
  }

  // ------------------------------------------------------------ state

  private reset(): void {
    this.lines = { src: [blankLine()], dst: [blankLine()] };
    this.narration.value = "";
    this.at = 0;
  }

  private allLines(): MoveLine[] {
    return [...this.lines.src, ...this.lines.dst];
  }

  /** The Godown column: when the company has godowns, or a line already names one. */
  private showGodown(): boolean {
    return this.godowns.length > 0 || this.allLines().some((l) => l.godownId);
  }

  /** The Batch column: turned on with Ctrl+I, or while any line has a batch. */
  private showBatch(): boolean {
    return this.batchOn || this.allLines().some((l) => l.batch.trim());
  }

  private fields(): Field[] {
    const out: Field[] = [];
    const godown = this.showGodown();
    const batch = this.showBatch();
    for (const p of PANELS) {
      this.lines[p].forEach((_, i) => {
        out.push({ k: "item", p, i });
        if (godown) out.push({ k: "godown", p, i });
        if (batch) out.push({ k: "batch", p, i });
        out.push({ k: "qty", p, i }, { k: "rate", p, i }, { k: "value", p, i });
      });
    }
    out.push({ k: "narration" });
    return out;
  }

  private field(): Field {
    const f = this.fields();
    return f[Math.min(this.at, f.length - 1)];
  }

  private keyOf(f: Field): string {
    return f.k === "narration" ? f.k : `${f.p}:${f.k}:${f.i}`;
  }

  private indexOf(k: LineKey, p: Panel, i: number): number {
    return this.fields().findIndex((f) => f.k === k && f.p === p && f.i === i);
  }

  /** Value of a panel's complete lines. */
  private total(p: Panel): M.Units {
    return M.sum(this.lines[p].filter(lineDone).map(amountOf));
  }

  /**
   * Fill the rate and amount of lines whose rate the user hasn't typed: a
   * single destination line carries the value consumed, any other line is
   * valued at the item's average cost.
   */
  private fillAuto(): void {
    const atCost = (l: MoveLine, q: M.Units) => {
      const avg = averageCost(l.item!);
      l.rate = avg ? M.toDecimal(avg.shown) : "";
      l.amount = avg && q > 0n ? M.toDecimal(lineAmount(q, avg.exact)) : "";
    };
    // Source first: the destination may carry its total.
    for (const l of this.lines.src) if (l.auto && l.item) atCost(l, M.parse(l.qty) ?? M.ZERO);
    const used = this.total("src");
    const made = this.lines.dst.filter((l) => l.item);
    const carry = made.length === 1 && used > 0n ? made[0] : null;
    for (const l of this.lines.dst) {
      if (!l.auto || !l.item) continue;
      const q = M.parse(l.qty) ?? M.ZERO;
      if (l !== carry) atCost(l, q);
      else {
        l.amount = q > 0n ? M.toDecimal(used) : "";
        l.rate = q > 0n ? M.toDecimal(rateOf(used, q)) : "";
      }
    }
  }

  /** qty × rate → amount, for a line with a typed rate. */
  private recompute(l: MoveLine): void {
    if (l.auto) return;
    const q = M.parse(l.qty);
    const r = M.parse(l.rate);
    if (q !== null && r !== null && l.rate.trim() && l.qty.trim()) l.amount = M.toDecimal(lineAmount(M.abs(q), M.abs(r)));
  }

  /** Stock of an item at a location after this voucher; without location data, its total stock. */
  private closingAt(item: StockItem, godownId: string): M.Units {
    const byLocation = this.located !== null;
    let effect = M.ZERO;
    for (const p of PANELS) {
      for (const l of this.lines[p]) {
        if (l.item?.id !== item.id || (byLocation && l.godownId !== godownId)) continue;
        const q = M.parse(l.qty) ?? M.ZERO;
        effect += p === "dst" ? q : -q;
      }
    }
    if (!byLocation) {
      const saved = M.sum([...this.savedMoves].filter(([k]) => k.startsWith(`${item.id}|`)).map(([, q]) => q));
      return M.of(item.closing_qty) - saved + effect;
    }
    const key = `${item.id}|${godownId}`;
    const saved = this.savedDate !== null && this.savedDate <= this.date ? (this.savedMoves.get(key) ?? M.ZERO) : M.ZERO;
    return (this.located!.get(key) ?? M.ZERO) - saved + effect;
  }

  /** "Closing stock" or "At Shop Floor after this", for a line's hint and warning. */
  private placeOf(l: MoveLine): string {
    if (this.located === null || !this.showGodown()) return t("Closing stock");
    return t("At {godown} after this", { godown: l.godownName || t(MAIN_LOCATION) });
  }

  /** What would be saved, for spotting changes to a loaded voucher. */
  private state(): string {
    const moves = (p: Panel) =>
      this.lines[p].filter(lineDone).map((l) => [l.item!.id, l.qty, l.rate, M.toDecimal(amountOf(l)), l.godownId, l.batch.trim()]);
    return JSON.stringify([this.date, this.narration.value, moves("src"), moves("dst")]);
  }

  private dirty(): boolean {
    if (this.loaded !== null) return this.state() !== this.loaded;
    return this.narration.value.trim() !== "" || this.allLines().some((l) => l.item || l.qty.trim());
  }

  // ----------------------------------------------------------- render

  private render(): void {
    this.picker?.close();
    this.picker = null;
    this.inputs.clear();
    this.hints.clear();
    this.numberEl.textContent = t("No. {number}", { number: ltr(this.number) });
    this.dateEl.textContent = D.show(this.date);
    this.dayEl.textContent = D.weekday(this.date);
    this.fillAuto();
    const rows: HTMLElement[] = PANELS.map((p) => this.renderPanel(p));
    if (this.kept.length)
      rows.push(
        h(
          "div.sj-kept",
          this.kept.length === 1
            ? t("1 ledger line saved with this voucher is kept as it is.")
            : t("{n} ledger lines saved with this voucher are kept as they are.", { n: this.kept.length }),
        ),
      );
    this.body.replaceChildren(...rows);
    if (this.cancelled) {
      for (const inp of this.inputs.values()) inp.readOnly = true;
      this.narration.readOnly = true;
      this.el.classList.add("readonly");
    }
    this.paintLive();
    this.shell.refresh();
  }

  private renderPanel(p: Panel): HTMLElement {
    const godown = this.showGodown();
    const batch = this.showBatch();
    const cols = `${godown ? "wg" : ""} ${batch ? "wb" : ""}`.trim();
    const rows: HTMLElement[] = [
      h("div.sj-head", h("span.sj-title", t(PANEL_LABEL[p])), this.panelTotal[p]),
      h(
        "div.vgrid.items.vcols",
        { class: cols },
        h("span", t("Name of item")),
        godown ? h("span", t("Godown")) : null,
        batch ? h("span", t("Batch")) : null,
        h("span.num", t("Quantity")),
        h("span.num", t("Rate")),
        h("span.num", t("Amount")),
      ),
    ];
    this.lines[p].forEach((l, i) => {
      const itemIn = this.input(`${p}:item:${i}`, l.item?.name ?? "");
      let godownIn: HTMLInputElement | null = null;
      if (godown) {
        godownIn = this.input(`${p}:godown:${i}`, l.godownName);
        godownIn.placeholder = t(MAIN_LOCATION);
      }
      let batchIn: HTMLInputElement | null = null;
      if (batch) {
        const inp = this.input(`${p}:batch:${i}`, l.batch);
        inp.addEventListener("input", () => (l.batch = inp.value));
        batchIn = inp;
      }
      const qtyIn = this.input(`${p}:qty:${i}`, l.qty, "amount");
      const rateIn = this.input(`${p}:rate:${i}`, "", "amount");
      const valueIn = this.input(`${p}:value:${i}`, "", "amount");
      qtyIn.placeholder = l.item?.unit ?? "";
      qtyIn.addEventListener("input", () => {
        l.qty = qtyIn.value;
        this.changed(l);
      });
      rateIn.addEventListener("input", () => {
        l.rate = rateIn.value;
        l.auto = false; // typed: the user's own rate from now on
        this.changed(l);
      });
      valueIn.addEventListener("input", () => {
        l.amount = valueIn.value;
        l.auto = false; // typed: the rate follows it on Enter
        this.fillAuto();
        this.paintLive();
      });
      for (const inp of [rateIn, valueIn]) {
        inp.addEventListener("blur", () => {
          const u = M.parse(inp.value);
          if (u !== null && inp.value.trim()) inp.value = M.format(M.abs(u));
        });
      }
      const hint = h("span.curbal");
      this.hints.set(`${p}:${i}`, hint);
      rows.push(h("div.vgrid.items.vline", { class: cols }, h("div.fstack", itemIn, hint), godownIn, batchIn, qtyIn, rateIn, valueIn));
    });
    return h("div.sj-panel", { class: `sj-${p}` }, ...rows);
  }

  /** A quantity, rate or amount was typed: recalculate and repaint. */
  private changed(l: MoveLine): void {
    this.recompute(l);
    this.fillAuto();
    this.paintLive();
  }

  private input(key: string, value: string, cls = ""): HTMLInputElement {
    const inp = h("input.field", { class: cls, value, spellcheck: false, autocomplete: "off" });
    this.inputs.set(key, inp);
    inp.addEventListener("focus", () => {
      const idx = this.fields().findIndex((f) => this.keyOf(f) === key);
      if (idx >= 0) this.onFocus(idx);
    });
    return inp;
  }

  /** Rates, amounts (except the field being typed in), hints and totals. */
  private paintLive(): void {
    const active = document.activeElement;
    for (const p of PANELS) {
      this.lines[p].forEach((l, i) => {
        const rateIn = this.inputs.get(`${p}:rate:${i}`);
        const valueIn = this.inputs.get(`${p}:value:${i}`);
        if (rateIn && rateIn !== active) rateIn.value = l.rate.trim() && M.parse(l.rate) !== null ? M.format(M.of(l.rate)) : l.rate;
        if (valueIn && valueIn !== active) valueIn.value = l.amount.trim() && M.parse(l.amount) !== null ? M.format(amountOf(l)) : l.amount;
        const hint = this.hints.get(`${p}:${i}`);
        if (!hint || this.cancelled) return; // a cancelled voucher moves no stock
        const q = l.item ? this.closingAt(l.item, l.godownId) : M.ZERO;
        hint.textContent = l.item ? `${this.placeOf(l)}: ${qtyUnit(qtyText(q), l.item.unit)}` : "";
        hint.classList.toggle("neg", Boolean(l.item) && q < 0n);
      });
      this.panelTotal[p].textContent = M.format(this.total(p));
    }
    const used = this.total("src");
    const made = this.total("dst");
    const gap = made - used;
    this.totalsEl.replaceChildren(
      h("span.flabel", t("Source")),
      h("span.num.total", M.format(used)),
      h("span.flabel", t("Destination")),
      h("span.num.total", M.format(made)),
      ...(used > 0n && made > 0n && gap !== 0n
        ? [h("span.diff", t(gap > 0n ? "Value added {amount}" : "Value lost {amount}", { amount: ltr(M.format(M.abs(gap))) }))]
        : []),
    );
  }

  // ------------------------------------------------------------ focus

  focus(): void {
    this.go(this.at);
  }

  resume(): void {
    void this.picker?.reload();
    // A stock item or godown may have just been created (with opening stock).
    void this.loadLocations().then(() => this.paintLive());
  }

  private inputFor(f: Field): HTMLInputElement {
    return f.k === "narration" ? this.narration : this.inputs.get(this.keyOf(f))!;
  }

  private go(i: number): void {
    const fields = this.fields();
    this.at = Math.max(0, Math.min(i, fields.length - 1));
    const inp = this.inputFor(fields[this.at]);
    if (document.activeElement !== inp) inp.focus();
    else this.onFocus(this.at);
    inp.select();
  }

  /** Field gained focus (keyboard or mouse): attach the list picker for item and godown fields. */
  private onFocus(i: number): void {
    const f = this.fields()[i];
    const prev = this.picker;
    this.at = i;
    if (prev) {
      prev.close();
      this.picker = null;
      this.restoreText();
    }
    if (f.k === "item" && !this.cancelled) {
      this.picker = this.makeItemPicker(f.p, f.i);
      void this.picker.open();
    }
    if (f.k === "godown" && !this.cancelled) {
      this.picker = this.makeGodownPicker(f.p, f.i);
      void this.picker.open();
    }
    if (f.k === "rate" || f.k === "value") {
      const inp = this.inputFor(f);
      const u = M.parse(inp.value);
      if (u !== null && u !== 0n) inp.value = M.toDecimal(M.abs(u));
    }
    this.shell.refresh();
  }

  /** Put back the names in item and godown fields left with filter text. */
  private restoreText(): void {
    for (const [key, inp] of this.inputs) {
      const [p, k, i] = key.split(":");
      const l = this.lines[p as Panel]?.[Number(i)];
      if (k === "item") inp.value = l?.item?.name ?? "";
      else if (k === "godown") inp.value = l?.godownName ?? "";
    }
  }

  private makeItemPicker(p: Panel, i: number): ListPicker<StockItem> {
    const actions = (): PickAction[] => {
      const chord = this.shell.km.chordIn("CreateMasterInline", ["ListField"]);
      return [
        { id: "end", label: t("End of List"), run: () => this.endOfList(p, i) },
        { id: "create", label: t("Create Stock Item"), hint: chord ?? undefined, run: (text) => this.createItemInline(p, i, text) },
      ];
    };
    return new ListPicker<StockItem>({
      input: this.inputFor({ k: "item", p, i }),
      host: this.el,
      title: p === "src" ? "Stock items: source" : "Stock items: destination",
      columns: ["Name", "In stock"],
      source: async (q) =>
        (await api.listStockItems(q)).map<PickItem<StockItem>>((s) => ({
          id: s.id,
          label: s.name,
          sub: qtyUnit(qtyText(M.of(s.closing_qty)), s.unit),
          data: s,
        })),
      actions,
      create: { noun: "stock item", run: (text) => this.createItemInline(p, i, text) },
      selectedId: () => this.lines[p][i]?.item?.id ?? null,
      // The first source line usually names an item; later blank lines end the list.
      preferAction: () => (i > 0 && !this.lines[p][i]?.item ? "end" : null),
      onPick: (it) => this.setItem(p, i, it.data),
      onAlter: (it) =>
        void import("./inventory").then(({ StockItemForm }) =>
          this.shell.push(new StockItemForm(this.shell, { alterId: it.data.id, inline: true, onCreated: (s) => this.setItem(p, i, s) })),
        ),
    });
  }

  /** Where the stock moves out of or into; "Main Location" sits pinned at the top. */
  private makeGodownPicker(p: Panel, i: number): ListPicker<Godown> {
    const l = this.lines[p][i];
    const stockAt = (godownId: string): string | undefined =>
      l?.item && this.located ? qtyUnit(qtyText(this.located.get(`${l.item.id}|${godownId}`) ?? M.ZERO), l.item.unit) : undefined;
    return new ListPicker<Godown>({
      input: this.inputFor({ k: "godown", p, i }),
      host: this.el,
      title: p === "src" ? "Godowns: from" : "Godowns: to",
      columns: ["Name", l?.item && this.located ? t("{item} in stock", { item: l.item.name }) : "Under"],
      source: async (q) =>
        (await api.listGodowns(q)).map<PickItem<Godown>>((g) => ({ id: g.id, label: g.name, sub: stockAt(g.id) ?? g.parent_name ?? "", data: g })),
      actions: () => {
        const chord = this.shell.km.chordIn("CreateMasterInline", ["ListField"]);
        return [
          { id: "main", label: t(MAIN_LOCATION), hint: stockAt(""), run: () => this.setGodown(p, i, null) },
          { id: "create", label: t("Create Godown"), hint: chord ?? undefined, run: (text) => this.createGodownInline(p, i, text) },
        ];
      },
      create: { noun: "godown", run: (text) => this.createGodownInline(p, i, text) },
      selectedId: () => this.lines[p][i]?.godownId || null,
      preferAction: () => (this.lines[p][i]?.godownId ? null : "main"),
      onPick: (it) => this.setGodown(p, i, it.data),
      onAlter: (it) =>
        void import("./godown").then(({ GodownForm }) =>
          this.shell.push(new GodownForm(this.shell, { alterId: it.data.id, inline: true, onCreated: (g) => this.setGodown(p, i, g) })),
        ),
    });
  }

  private setItem(p: Panel, i: number, s: StockItem): void {
    const l = this.lines[p][i];
    if (!l) return;
    if (l.item?.id !== s.id || !l.rate.trim()) l.auto = true;
    l.item = s;
    // A transfer: the destination takes the quantity moved out of the source.
    if (p === "dst" && !l.qty.trim()) {
      const from = this.lines.src.find((x) => x.item?.id === s.id && lineDone(x));
      if (from) l.qty = from.qty;
    }
    this.recompute(l);
    this.render();
    this.go(this.indexOf("item", p, i) + 1);
  }

  private setGodown(p: Panel, i: number, g: Godown | null): void {
    const l = this.lines[p][i];
    if (!l) return;
    l.godownId = g?.id ?? "";
    l.godownName = g?.name ?? "";
    const inp = this.inputs.get(`${p}:godown:${i}`);
    if (inp) inp.value = l.godownName;
    this.paintLive();
    this.go(this.indexOf("godown", p, i) + 1);
  }

  // Loaded on demand: inventory.ts and godown.ts build on the reports, which open vouchers.
  private createItemInline(p: Panel, i: number, text: string): void {
    void import("./inventory").then(({ StockItemForm }) =>
      this.shell.push(new StockItemForm(this.shell, { name: text, inline: true, onCreated: (s) => this.setItem(p, i, s) })),
    );
  }

  private createGodownInline(p: Panel, i: number, text: string): void {
    void import("./godown").then(({ GodownForm }) =>
      this.shell.push(
        new GodownForm(this.shell, {
          name: text,
          inline: true,
          onCreated: (g) => {
            if (!this.godowns.some((x) => x.id === g.id)) this.godowns.push(g);
            this.setGodown(p, i, g);
          },
        }),
      ),
    );
  }

  // ------------------------------------------------------------- keys

  contexts(): Context[] {
    const f = this.field();
    if (f.k === "item" || f.k === "godown") return ["ListField", "Voucher"];
    if (f.k === "qty" || f.k === "rate" || f.k === "value") return ["AmountField", "Voucher"];
    return ["Voucher"];
  }

  handles(a: string): boolean {
    if (a.startsWith("OpenVoucher:")) return !this.alterId && (a === `OpenVoucher:${KIND}` || (KINDS as string[]).includes(a.slice(12)));
    if (a === "DeleteRecord") return Boolean(this.alterId);
    if (a === "CancelVoucher") return Boolean(this.alterId) && !this.cancelled;
    if (this.cancelled) return a === "Back";
    return ["ChangeDate", "Accept", "Back", "RemoveLine", "MoreDetails"].includes(a);
  }

  current(a: string): boolean {
    return a === `OpenVoucher:${KIND}`;
  }

  bottom() {
    const ctx = this.contexts();
    const f = this.field();
    return [
      this.shell.btn("Accept", ctx),
      this.shell.btn("RemoveLine", ctx, "Remove Line", f.k !== "narration" && !this.cancelled),
      f.k === "item" && !this.cancelled ? this.shell.btn("CreateMasterInline", ctx, "Create Stock Item") : null,
      f.k === "godown" && !this.cancelled ? this.shell.btn("CreateMasterInline", ctx, "Create Godown") : null,
      !this.cancelled ? this.shell.btn("MoreDetails", ctx, this.showBatch() ? "Hide Batch" : "Batch Column") : null,
      this.alterId && !this.cancelled ? this.shell.btn("CancelVoucher", ctx, "Cancel Vch") : null,
      this.alterId ? this.shell.btn("DeleteRecord", ctx, "Delete") : null,
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  key(e: KeyEv): boolean {
    if (this.cancelled) {
      if (e.chord === "Up") {
        if (this.at > 0) this.go(this.at - 1);
        return true;
      }
      return e.key.length === 1 && !e.ctrlKey && !e.altKey; // read-only
    }
    if (this.picker?.key(e)) return true;
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true;
    }
    if (e.chord === "Backspace" && this.field().k === "narration" && this.narration.value === "") {
      this.go(this.at - 1);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    if (this.picker?.action(a)) return true;
    if (a.startsWith("OpenVoucher:") && this.alterId) {
      this.shell.warn("A saved voucher keeps its type. Delete it and enter a new one to change the type.");
      return true;
    }
    if (a === "DeleteRecord" && this.alterId) {
      void this.deleteSaved();
      return true;
    }
    if (a === "CancelVoucher" && this.alterId) {
      void this.cancelSaved();
      return true;
    }
    if (this.cancelled) {
      // Read-only: move between fields, leave with Esc, nothing else.
      if (a === "Back") this.shell.pop();
      else if (a === "NextField") this.go(this.at + 1);
      else if (a === "PreviousField") this.go(this.at - 1);
      else if (a === "Accept" || a === "RemoveLine" || a === "ChangeDate" || a === "MoreDetails")
        this.shell.warn("This voucher is cancelled and can't be changed. Alt+D deletes it.");
      else return false;
      return true;
    }
    if (a.startsWith("OpenVoucher:")) {
      const k = a.slice(12);
      if ((KINDS as string[]).includes(k)) void this.switchKind(k as VoucherKind);
      else if (k !== KIND) this.shell.warn(t("{kind} vouchers aren't available yet.", { kind: t(k.replace(/([a-z])([A-Z])/g, "$1 $2")) }));
      return true;
    }
    switch (a) {
      case "ChangeDate":
        void this.changeDate();
        return true;
      case "MoreDetails":
        this.toggleBatch();
        return true;
      case "RemoveLine":
        this.removeLine();
        return true;
      case "Accept":
        void this.save();
        return true;
      case "NextField":
        this.next();
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Back":
        void this.escape();
        return true;
    }
    return false;
  }

  private next(): void {
    const f = this.field();
    if (f.k === "narration") return void this.confirm();
    const l = this.lines[f.p][f.i];
    if (f.k === "item") {
      if (!l.item) return this.shell.warn("Pick a stock item, or End of List.");
      return this.go(this.at + 1);
    }
    if (f.k === "godown" || f.k === "batch") return this.go(this.at + 1);
    if (f.k === "qty") return this.commitQty(f.p, f.i);
    if (f.k === "rate") return this.commitRate(f.p, f.i);
    return this.commitValue(f.p, f.i);
  }

  /** The quantity, within the unit's decimal places. */
  private commitQty(p: Panel, i: number): void {
    const l = this.lines[p][i];
    if (!l.item) {
      this.shell.warn("Pick a stock item first.");
      return this.go(this.indexOf("item", p, i));
    }
    const problem = this.qtyProblem(l);
    if (problem) return this.shell.warn(problem);
    l.qty = qtyText(M.parse(l.qty)!);
    this.inputFor({ k: "qty", p, i }).value = l.qty;
    this.changed(l);
    // Warn, never block: stock may arrive on a later voucher.
    const after = this.closingAt(l.item, l.godownId);
    if (p === "src" && after < 0n) {
      const params = { item: l.item.name, quantity: ltr(qtyText(after)), unit: l.item.unit };
      this.shell.warn(
        this.located !== null && this.showGodown()
          ? t("Stock of {item} at {godown} goes below zero: {quantity} {unit} after this.", { ...params, godown: l.godownName || t(MAIN_LOCATION) })
          : t("Stock of {item} goes below zero: {quantity} {unit} after this.", params),
      );
    }
    this.go(this.at + 1);
  }

  private qtyProblem(l: MoveLine): string | null {
    const u = M.parse(l.qty);
    if (u === null) return t('"{text}" is not a quantity.', { text: l.qty });
    if (u <= 0n) return t("Enter a quantity of {item} greater than zero.", { item: l.item!.name });
    const places = l.item!.decimals;
    const unit = l.item!.unit;
    if (placesOf(l.qty) > places)
      return places === 0
        ? t("{unit} is counted in whole numbers.", { unit })
        : places === 1
          ? t("{unit} takes up to 1 decimal place.", { unit })
          : t("{unit} takes up to {n} decimal places.", { unit, n: places });
    return null;
  }

  /** A typed rate; cleared, the line goes back to its default rate. */
  private commitRate(p: Panel, i: number): void {
    const l = this.lines[p][i];
    const u = M.parse(l.rate);
    if (u === null) return this.shell.error(t('"{text}" is not a rate.', { text: l.rate }));
    if (u < 0n) return this.shell.warn("A rate can't be negative.");
    if (!l.rate.trim()) l.auto = true;
    else l.rate = M.toDecimal(u);
    this.changed(l);
    this.go(this.at + 1);
  }

  /** The line's amount: qty × rate, or typed (the rate then follows it). */
  private commitValue(p: Panel, i: number): void {
    const l = this.lines[p][i];
    const u = M.parse(l.amount);
    if (u === null) return this.shell.error(t('"{text}" is not an amount.', { text: l.amount }));
    if (u < 0n) return this.shell.warn("An amount can't be negative.");
    if (!l.auto) {
      l.amount = M.toDecimal(u);
      const q = M.parse(l.qty) ?? M.ZERO;
      if (q > 0n && lineAmount(q, M.parse(l.rate) ?? M.ZERO) !== u) l.rate = M.toDecimal(rateOf(u, q));
    }
    const list = this.lines[p];
    if (i === list.length - 1) list.push(blankLine());
    this.render();
    this.go(this.indexOf("item", p, i + 1));
  }

  /** End of List: source → destination, destination → narration. */
  private endOfList(p: Panel, i: number): void {
    const list = this.lines[p];
    if (!list[i]?.item && list.length > 1) list.splice(i, 1);
    const open = list.findIndex((l) => l.item && this.qtyProblem(l));
    this.render();
    if (open >= 0) {
      this.shell.warn(this.qtyProblem(list[open])!);
      return this.go(this.indexOf("qty", p, open));
    }
    if (p === "src") return this.go(this.indexOf("item", "dst", 0));
    this.go(this.fields().length - 1);
  }

  /** Why the voucher can't be saved yet, and the field to fix; null when it can. */
  private problem(): { text: string; at: number } | null {
    for (const p of PANELS) {
      const i = this.lines[p].findIndex((l) => l.item && this.qtyProblem(l));
      if (i >= 0) return { text: this.qtyProblem(this.lines[p][i])!, at: this.indexOf("qty", p, i) };
      const bad = this.lines[p].findIndex((l) => l.item && (M.parse(l.rate) === null || M.parse(l.amount) === null));
      if (bad >= 0) return { text: t("Check the rate and amount of {item}.", { item: this.lines[p][bad].item!.name }), at: this.indexOf("rate", p, bad) };
    }
    if (!this.allLines().some(lineDone))
      return { text: t("Add at least one stock line: what goes out under Source, or what comes in under Destination."), at: 0 };
    return null;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.go(0);
  }

  private async escape(): Promise<void> {
    const f = this.field();
    const inp = this.inputFor(f);
    if ((f.k === "narration" || f.k === "qty" || f.k === "rate" || f.k === "value" || f.k === "batch") && inp.value !== "") {
      inp.value = ""; // F4
      if (f.k !== "narration") {
        const l = this.lines[f.p][f.i];
        if (f.k === "qty") l.qty = "";
        if (f.k === "batch") l.batch = "";
        if (f.k === "rate") {
          l.rate = "";
          l.auto = true; // back to the default rate
        }
        if (f.k === "value") {
          l.amount = "";
          l.auto = false;
        }
        this.changed(l);
        if (f.k === "rate") inp.value = l.rate ? M.toDecimal(M.of(l.rate)) : "";
      }
      return;
    }
    if (this.dirty() && !(await this.shell.ask("Quit without saving?"))) return;
    this.shell.pop();
  }

  private removeLine(): void {
    const f = this.field();
    if (f.k === "narration") return;
    const list = this.lines[f.p];
    list.splice(f.i, 1);
    if (!list.length) list.push(blankLine());
    this.render();
    this.go(this.indexOf("item", f.p, Math.min(f.i, list.length - 1)));
  }

  /** Ctrl+I: show or hide the Batch column. */
  private toggleBatch(): void {
    if (this.batchOn && this.allLines().some((l) => l.batch.trim())) {
      this.shell.warn("Some lines have a batch. Clear them to hide the Batch column.");
      return;
    }
    const f = this.field(); // before the column count changes
    this.batchOn = !this.batchOn;
    this.render();
    const now = this.fields().findIndex((x) => this.keyOf(x) === this.keyOf(f));
    // On the Batch column as it goes: the item of that line.
    this.go(now >= 0 ? now : f.k === "narration" ? this.fields().length - 1 : this.indexOf("item", f.p, f.i));
  }

  private async changeDate(): Promise<void> {
    const v = await this.shell.dialog("Voucher date", [{ label: "Date", value: D.editable(this.date) }], ([text]) =>
      D.parseUser(text, this.date) ? null : t('"{text}" is not a date. Try 19-10-2026.', { text }),
    );
    if (!v) return;
    const was = this.date;
    this.date = D.parseUser(v[0], this.date)!;
    if (this.date !== was) {
      // A new voucher's number follows its date (series can restart each year);
      // a saved voucher keeps its number.
      try {
        if (!this.alterId) this.number = await api.nextVoucherNumber(KIND, this.date);
        await this.loadLocations();
      } catch (e) {
        this.shell.error(e);
      }
    }
    this.render();
    this.go(this.at);
  }

  /** F4–F9 and friends: an accounting voucher takes this screen's place. */
  private async switchKind(kind: VoucherKind): Promise<void> {
    if (this.dirty() && !(await this.shell.ask(t("Discard this voucher and switch to {kind}?", { kind: t(KIND_LABEL[kind]) })))) return;
    this.shell.pop();
    void this.shell.push(new VoucherScreen(this.shell, kind, this.date));
  }

  private moves(p: Panel): StockMoveInput[] {
    return this.lines[p].filter(lineDone).map((l) => {
      const m: StockMoveInput = {
        inward: p === "dst",
        item_id: l.item!.id,
        qty: M.toDecimal(M.parse(l.qty)!),
        rate: M.toDecimal(M.abs(M.parse(l.rate) ?? M.ZERO)),
        amount: M.toDecimal(amountOf(l)),
      };
      if (l.godownId) m.godown_id = l.godownId;
      if (l.batch.trim()) m.batch = l.batch.trim();
      return m;
    });
  }

  private async save(): Promise<void> {
    const problem = this.problem();
    if (problem) {
      this.shell.warn(problem.text);
      return this.go(problem.at);
    }
    const input = {
      kind: KIND,
      date: this.date,
      number: this.number,
      entries: this.kept,
      stock: [...this.moves("src"), ...this.moves("dst")],
      narration: this.narration.value,
    };
    try {
      if (this.alterId) {
        await api.alterVoucher(this.alterId, input);
        await this.shell.refreshCompany();
        this.shell.info(t("{kind} No. {number} altered.", { kind: t(KIND_LABEL[KIND]), number: ltr(this.number) }));
        this.shell.pop(); // back to the report, which reloads with its cursor kept
        return;
      }
      const posted = await api.postVoucher(input);
      this.shell.info(t("{kind} No. {number} saved.", { kind: t(KIND_LABEL[KIND]), number: ltr(posted.number) }));
      await this.shell.refreshCompany();
      // A blank voucher of the same type, numbered next.
      this.number = posted.next_number;
      this.reset();
      await this.loadLocations();
      this.render();
      this.go(0);
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async cancelSaved(): Promise<void> {
    if (!this.alterId || this.cancelled) return;
    const label = t("{kind} No. {number}", { kind: t(KIND_LABEL[KIND]), number: ltr(this.number) });
    if (!(await this.shell.ask(t("Cancel {label}? It keeps its number but no longer counts.", { label }), { yesFirst: false }))) return;
    try {
      await api.cancelVoucher(this.alterId);
      await this.shell.refreshCompany();
      this.shell.info(t("{label} cancelled.", { label }));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async deleteSaved(): Promise<void> {
    if (!this.alterId || !(await this.shell.ask(t("Delete {kind} No. {number}?", { kind: t(KIND_LABEL[KIND]), number: ltr(this.number) }), { yesFirst: false }))) return;
    try {
      await api.deleteVoucher(this.alterId);
      await this.shell.refreshCompany();
      this.shell.info(t("{kind} No. {number} deleted. The audit trail keeps a full copy.", { kind: t(KIND_LABEL[KIND]), number: ltr(this.number) }));
      this.alterId = null;
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  dispose(): void {
    this.picker?.close();
  }
}

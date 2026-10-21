// Inventory masters (Unit, Stock Group, Stock Item) and the Stock Summary
// with its Stock Item Vouchers drill-down. The forms follow the ledger
// form's field flow (F1–F7) and list rules (P6–P8); Alt+D deletes a record
// being altered, after a Y/N prompt.
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { dir, inUse, ltr, t } from "../i18n";
import * as M from "../money";
import { ListPicker, type PickAction, type PickItem } from "../picker";
import type { KeyEv, Shell, Screen } from "../shell";
import { KIND_LABEL, type Context, type Costing, type StockGroup, type StockItem, type StockSummaryRow, type TaxRate, type Unit } from "../types";
import { Report } from "./reports";
import { normaliseClass, percentText, rateFor } from "./settings";
import { savedVoucherScreen } from "./stockjournal";
import { lineAmount, placesOf, qtyText, rateOf } from "./voucher";
import "../inventory.css";

export interface InventoryFormOptions<T = unknown> {
  /** Alter this record instead of creating one. */
  alterId?: string;
  /** Text typed in the calling list field: a unit's symbol, else the name. */
  name?: string;
  /** Stacked over the screen that asked for it (Alt+C), parent visible (L7). */
  inline?: boolean;
  /** Called after save in inline mode; the form closes first. */
  onCreated?: (record: T) => void;
}

/** What a master form needs from a list picker (any ListPicker<T>). */
interface FieldPicker {
  readonly isOpen: boolean;
  open(): Promise<void>;
  close(): void;
  reload(): Promise<void>;
  key(e: KeyEv): boolean;
  action(a: string): boolean;
}

function frow(label: string, field: HTMLElement, sub?: string): HTMLElement {
  return h("label.frow", h("span.flabel", label, sub ? h("small.fsub", sub) : null), field);
}

/** "Can't delete…" from the core's reason. */
function deleteError(noun: string, name: string, e: unknown): string {
  const why = e instanceof Error ? e.message : String(e);
  return t('Can\'t delete {what} "{name}": {why}.', { what: t(noun), name, why });
}

/** A quantity with at most `places` decimals, or a message saying why not. */
function checkQty(text: string, unit: string, places: number): string | null {
  const u = M.parse(text);
  if (u === null) return t('"{text}" is not a quantity.', { text });
  if (u < 0n) return t("A quantity can't be negative.");
  if (placesOf(text) > places)
    return places === 0
      ? t("{unit} is counted in whole numbers.", { unit })
      : places === 1
        ? t("{unit} takes up to 1 decimal place.", { unit })
        : t("{unit} takes up to {n} decimal places.", { unit, n: places });
  return null;
}

// ---------------------------------------------------------- master forms

/** Shared field flow for the inventory masters. */
abstract class MasterForm<F extends string, T> implements Screen {
  el = h("section.ledger");
  overlay: boolean;
  protected at = 0;
  protected inputs = {} as Record<F, HTMLInputElement>;
  protected pickers: Partial<Record<F, FieldPicker>> = {};
  /** Values when the form was last loaded or blanked ("Quit without saving?"). */
  private snapshot = "";
  /** "Unit", "Stock Group", "Stock Item". */
  protected abstract label: string;

  constructor(
    protected shell: Shell,
    protected opts: InventoryFormOptions<T>,
    protected fields: readonly F[],
  ) {
    this.overlay = Boolean(opts.inline);
    for (const f of fields) {
      const inp = h("input.field", { spellcheck: false, autocomplete: "off" });
      inp.addEventListener("focus", () => this.focusField(this.fields.indexOf(f)));
      this.inputs[f] = inp;
    }
  }

  /** "Stock Item Creation": English, translated by the shell. */
  get title(): string {
    return `${this.label} ${this.opts.alterId ? "Alteration" : "Creation"}`;
  }

  /** Lay out the rows (and an optional side panel), as a card when inline. */
  protected layout(rows: HTMLElement[], panel?: HTMLElement): void {
    this.el.append(
      h(
        "div.form.master",
        { class: this.overlay ? "card" : "" },
        this.overlay ? h("div.card-title", t(`${this.opts.alterId ? "Alter" : "Create"} ${this.label}`)) : null,
        h("div.form-main", ...rows),
        panel ?? null,
      ),
    );
  }

  async init(): Promise<void> {
    await this.load();
    this.snapshot = this.state();
  }

  /** Fill the form from the record being altered. */
  protected async load(): Promise<void> {}

  /** Enter on a field (when its list didn't take the key). */
  protected abstract next(f: F): void | Promise<void>;
  protected abstract save(): Promise<void>;
  /** Clear for the next record (F7); chosen lists carry over. */
  protected abstract blank(): void;
  /** Name of the record, for the delete prompt. */
  protected abstract recordName(): string;
  protected abstract deleteRecord(id: string): Promise<void>;
  /** The chosen value's text, put back in a list field when it loses focus. */
  protected pickedText(_f: F): string {
    return "";
  }
  /** Label of Alt+C on a list field, when it creates. */
  protected createLabel(_f: F): string | null {
    return null;
  }

  protected state(): string {
    return this.fields.map((f) => (this.pickers[f] ? this.pickedText(f) : this.inputs[f].value.trim())).join("\u0000");
  }

  private dirty(): boolean {
    return this.state() !== this.snapshot;
  }

  protected field(): F {
    return this.fields[this.at];
  }

  contexts(): Context[] {
    return this.pickers[this.field()] ? ["ListField", "Master"] : ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back" || (a === "DeleteRecord" && Boolean(this.opts.alterId));
  }

  bottom() {
    const ctx = this.contexts();
    const create = this.createLabel(this.field());
    return [
      this.shell.btn("Accept", ctx),
      create ? this.shell.btn("CreateMasterInline", ctx, create) : null,
      this.opts.alterId ? this.shell.btn("DeleteRecord", ctx, "Delete") : null,
      this.shell.btn("Back", ctx, this.overlay ? "Cancel" : "Back"),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    this.go(this.at);
  }

  resume(): void {
    void this.pickers[this.field()]?.reload();
  }

  protected focusField(i: number): void {
    const f = this.fields[i];
    this.at = i;
    for (const k of this.fields) {
      const p = this.pickers[k];
      if (k !== f && p?.isOpen) {
        p.close();
        this.inputs[k].value = this.pickedText(k);
      }
    }
    const p = this.pickers[f];
    if (p && !p.isOpen) void p.open();
    this.shell.refresh();
  }

  protected go(i: number): void {
    i = Math.max(0, Math.min(i, this.fields.length - 1));
    const inp = this.inputs[this.fields[i]];
    inp.focus();
    this.focusField(i);
    inp.select();
  }

  protected goTo(f: F): void {
    this.go(this.fields.indexOf(f));
  }

  key(e: KeyEv): boolean {
    if (this.pickers[this.field()]?.key(e)) return true;
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true; // Up on the first field does nothing (F3)
    }
    return false;
  }

  action(a: string): boolean {
    if (this.pickers[this.field()]?.action(a)) return true;
    switch (a) {
      case "Accept":
        void this.save();
        return true;
      case "NextField":
        void this.next(this.field());
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Back":
        void this.escape();
        return true;
      case "DeleteRecord":
        if (!this.opts.alterId) return false;
        void this.remove();
        return true;
    }
    return false;
  }

  /** Enter past the last field: "Accept? Yes / No" (F5). */
  protected async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.go(0);
  }

  private async escape(): Promise<void> {
    const f = this.field();
    const inp = this.inputs[f];
    if (!this.pickers[f] && inp.value !== "") {
      inp.value = ""; // F4: clear the field first
      inp.dispatchEvent(new Event("input"));
      return;
    }
    if (this.dirty() && !(await this.shell.ask("Quit without saving?"))) return;
    this.shell.pop();
  }

  private async remove(): Promise<void> {
    const id = this.opts.alterId;
    if (!id) return;
    const noun = this.label.toLowerCase();
    const name = this.recordName();
    if (!(await this.shell.ask(t('Delete {what} "{name}"?', { what: t(noun), name }), { yesFirst: false }))) return;
    try {
      await this.deleteRecord(id);
      this.shell.info(t('{what} "{name}" deleted.', { what: t(this.label), name }));
      this.shell.pop();
    } catch (e) {
      this.shell.error(deleteError(noun, name, e));
    }
  }

  /** After a save: hand back (inline), close (alter), or a blank form (F7). */
  protected done(record: T, message: string): void {
    this.shell.info(message);
    if (this.opts.onCreated) {
      const cb = this.opts.onCreated;
      this.shell.pop();
      cb(record);
      return;
    }
    if (this.opts.alterId) {
      this.shell.pop();
      return;
    }
    this.blank();
    this.snapshot = this.state();
    this.go(0);
  }

  dispose(): void {
    for (const f of this.fields) this.pickers[f]?.close();
  }
}

// ------------------------------------------------------------------ unit

type UnitField = "symbol" | "name" | "decimals";

export class UnitForm extends MasterForm<UnitField, Unit> {
  protected label = "Unit";

  constructor(shell: Shell, opts: InventoryFormOptions<Unit> = {}) {
    super(shell, opts, ["symbol", "name", "decimals"]);
    const i = this.inputs;
    i.symbol.value = opts.name ?? "";
    i.symbol.placeholder = t("e.g. Nos, kg, m");
    i.name.placeholder = t("e.g. Numbers, Kilograms");
    i.decimals.value = "0";
    i.decimals.classList.add("amount");
    this.layout([
      frow(t("Symbol"), i.symbol),
      frow(t("Formal name"), i.name),
      frow(t("Number of decimal places"), h("div.famount.narrow", i.decimals), t("0 to 4; 0 for whole numbers")),
    ]);
  }

  protected override async load(): Promise<void> {
    if (!this.opts.alterId) return;
    const u = (await api.listUnits()).find((x) => x.id === this.opts.alterId);
    if (!u) throw new Error(t("This unit no longer exists."));
    this.inputs.symbol.value = u.symbol;
    this.inputs.name.value = u.name;
    this.inputs.decimals.value = String(u.decimals);
  }

  private decimals(): number | null {
    const v = this.inputs.decimals.value.trim() || "0";
    return /^[0-4]$/.test(v) ? Number(v) : null;
  }

  protected next(f: UnitField): void {
    if (f === "symbol") {
      if (!this.inputs.symbol.value.trim()) return this.shell.warn(t("Enter the unit's symbol, such as Nos or kg."));
      return this.goTo("name");
    }
    if (f === "name") return this.goTo("decimals");
    if (this.decimals() === null) return this.shell.warn(t("Enter the number of decimal places, from 0 to 4."));
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const symbol = this.inputs.symbol.value.trim();
    if (!symbol) {
      this.goTo("symbol");
      return this.shell.warn(t("Enter the unit's symbol, such as Nos or kg."));
    }
    const decimals = this.decimals();
    if (decimals === null) {
      this.goTo("decimals");
      return this.shell.warn(t("Enter the number of decimal places, from 0 to 4."));
    }
    try {
      const u = await api.saveUnit(this.opts.alterId ?? "", symbol, this.inputs.name.value.trim(), decimals);
      this.done(u, t(this.opts.alterId ? 'Unit "{name}" altered.' : 'Unit "{name}" created.', { name: u.symbol }));
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected blank(): void {
    this.inputs.symbol.value = "";
    this.inputs.name.value = ""; // the decimal places carry over (F7)
  }

  protected recordName(): string {
    return this.inputs.symbol.value.trim();
  }

  protected deleteRecord(id: string): Promise<void> {
    return api.deleteUnit(id);
  }
}

// ----------------------------------------------------------- stock group

/** The Under line: where the chosen parent sits (Ma2). */
function parentLine(g: StockGroup | null): string {
  if (!g) return t("(top level)");
  return `(${g.parent_name || t("Primary")})`;
}

/** The "Under" list: stock groups plus a pinned Primary row (and Alt+C). */
function groupPicker(
  form: { shell: Shell; host: HTMLElement; input: HTMLInputElement },
  o: {
    exclude: () => Set<string>;
    selected: () => StockGroup | null;
    onPick: (g: StockGroup | null) => void;
    create: (text: string) => void;
  },
): ListPicker<StockGroup> {
  return new ListPicker<StockGroup>({
    input: form.input,
    host: form.host,
    title: "Stock groups",
    columns: ["Name", "Under"],
    source: async (q) =>
      (await api.listStockGroups(q))
        .filter((g) => !o.exclude().has(g.id))
        .map<PickItem<StockGroup>>((g) => ({ id: g.id, label: g.name, sub: g.parent_name || t("Primary"), data: g })),
    actions: (): PickAction[] => [{ id: "primary", label: t("Primary"), hint: t("top level"), run: () => o.onPick(null) }],
    create: { noun: "stock group", run: o.create },
    selectedId: () => o.selected()?.id ?? null,
    preferAction: () => (o.selected() ? null : "primary"),
    onPick: (it) => o.onPick(it.data),
  });
}

type GroupField = "name" | "under";

export class StockGroupForm extends MasterForm<GroupField, StockGroup> {
  protected label = "Stock Group";
  private parent: StockGroup | null = null;
  private parentEl = h("div.nature");
  /** Altering: the group itself and everything under it can't be its parent. */
  private excluded = new Set<string>();

  constructor(shell: Shell, opts: InventoryFormOptions<StockGroup> = {}) {
    super(shell, opts, ["name", "under"]);
    this.inputs.name.value = opts.name ?? "";
    this.inputs.under.value = t("Primary");
    this.parentEl.textContent = parentLine(null);
    this.layout([frow(t("Name"), this.inputs.name), frow(t("Under"), h("div.fstack", this.inputs.under, this.parentEl))]);
    this.pickers.under = groupPicker(
      { shell, host: this.el, input: this.inputs.under },
      {
        exclude: () => this.excluded,
        selected: () => this.parent,
        onPick: (g) => {
          this.setParent(g);
          void this.confirm();
        },
        create: (text) => this.createParentInline(text),
      },
    );
  }

  protected override async load(): Promise<void> {
    if (!this.opts.alterId) return;
    const all = await api.listStockGroups("");
    const me = all.find((g) => g.id === this.opts.alterId);
    if (!me) throw new Error(t("This stock group no longer exists."));
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

  private setParent(g: StockGroup | null): void {
    this.parent = g;
    this.inputs.under.value = g?.name ?? t("Primary");
    this.parentEl.textContent = parentLine(g);
  }

  private createParentInline(text: string): void {
    void this.shell.push(new StockGroupForm(this.shell, { name: text, inline: true, onCreated: (g) => this.setParent(g) }));
  }

  protected override pickedText(): string {
    return this.parent?.name ?? t("Primary");
  }

  protected override createLabel(f: GroupField): string | null {
    return f === "under" ? "Create Stock Group" : null;
  }

  protected next(f: GroupField): void {
    if (f === "name") {
      if (!this.inputs.name.value.trim()) return this.shell.warn(t("Enter a name for the stock group."));
      return this.goTo("under");
    }
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const name = this.inputs.name.value.trim();
    if (!name) {
      this.goTo("name");
      return this.shell.warn(t("Enter a name for the stock group."));
    }
    try {
      const g = await api.saveStockGroup(this.opts.alterId ?? "", name, this.parent?.id ?? "");
      const under = g.parent_name || t("Primary");
      this.done(g, t(this.opts.alterId ? 'Stock group "{name}" altered under {under}.' : 'Stock group "{name}" created under {under}.', { name: g.name, under }));
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected blank(): void {
    this.inputs.name.value = ""; // the parent carries over (F7)
    this.setParent(this.parent);
  }

  protected recordName(): string {
    return this.inputs.name.value.trim();
  }

  protected deleteRecord(id: string): Promise<void> {
    return api.deleteStockGroup(id);
  }
}

// ------------------------------------------------------------ stock item

type ItemField = "name" | "under" | "unit" | "tax" | "costing" | "qty" | "rate" | "value";

interface CostingMethod {
  id: Costing;
  label: string;
  /** Fits the list's right-hand column. */
  short: string;
  /** Shown under the field. */
  hint: string;
}

/** Stock valuation methods, in list order, with what each one means (shown through t()). */
const COSTING: CostingMethod[] = [
  { id: "AverageCost", label: "Average cost", short: "weighted average rate", hint: "weighted average rate of all stock in the period" },
  { id: "Fifo", label: "FIFO (first in, first out)", short: "oldest stock out first", hint: "stock left is valued at the latest purchases" },
  { id: "LastPurchase", label: "Last purchase cost", short: "latest purchase rate", hint: "stock valued at the most recent purchase rate" },
];

function costingOf(c: Costing): CostingMethod {
  return COSTING.find((x) => x.id === c) ?? COSTING[0];
}

export class StockItemForm extends MasterForm<ItemField, StockItem> {
  protected label = "Stock Item";
  private group: StockGroup | null = null;
  private unit: Unit | null = null;
  private groupEl = h("div.nature");
  private unitEl = h("div.nature");
  private panel = h("div.opening-panel");
  /** Altering: stock on hand now, from the saved item. */
  private saved: StockItem | null = null;
  /** The rate list, to show the rate in force for the HSN/SAC typed. */
  private rates: TaxRate[] = [];
  private taxEl = h("div.nature");
  /** How closing stock is valued; new items use average cost. */
  private costing: Costing = "AverageCost";
  private costingEl = h("div.nature");

  constructor(shell: Shell, opts: InventoryFormOptions<StockItem> = {}) {
    super(shell, opts, ["name", "under", "unit", "tax", "costing", "qty", "rate", "value"]);
    const i = this.inputs;
    i.name.value = opts.name ?? "";
    i.under.value = t("Primary");
    this.groupEl.textContent = parentLine(null);
    for (const f of ["qty", "rate", "value"] as const) i[f].classList.add("amount");
    const fy = this.shell.company ? D.show(this.shell.company.fy_start) : "";
    this.layout(
      [
        frow(t("Name"), i.name),
        frow(t("Under"), h("div.fstack", i.under, this.groupEl)),
        frow(t("Units"), h("div.fstack", i.unit, this.unitEl)),
        frow(t("HSN/SAC or tax category"), h("div.fstack", i.tax, this.taxEl), t("optional")),
        frow(t("Costing method"), h("div.fstack", i.costing, this.costingEl), t("values closing stock")),
        h(
          "div.fsection",
          h("div.fsection-title", fy ? t("Opening balance as on {date}", { date: fy }) : t("Opening balance")),
          frow(t("Quantity"), h("div.famount", i.qty)),
          frow(t("Rate"), h("div.famount", i.rate), t("per unit")),
          frow(t("Value"), h("div.famount", i.value), t("quantity × rate")),
        ),
      ],
      this.panel,
    );
    i.qty.addEventListener("input", () => this.recompute());
    i.rate.addEventListener("input", () => this.recompute());
    i.value.addEventListener("input", () => this.paintPanel());
    i.tax.placeholder = t("e.g. 8471 or 9983");
    i.tax.addEventListener("input", () => this.paintTax());
    for (const f of ["rate", "value"] as const) {
      i[f].addEventListener("blur", () => {
        const u = M.parse(i[f].value);
        if (u !== null && i[f].value.trim()) i[f].value = u === 0n ? "" : M.format(u);
      });
      i[f].addEventListener("focus", () => {
        const u = M.parse(i[f].value);
        if (u !== null && u !== 0n) i[f].value = M.toDecimal(u);
      });
    }
    this.pickers.under = groupPicker(
      { shell, host: this.el, input: i.under },
      {
        exclude: () => new Set(),
        selected: () => this.group,
        onPick: (g) => {
          this.setGroup(g);
          this.goTo("unit");
        },
        create: (text) => this.createGroupInline(text),
      },
    );
    this.pickers.unit = new ListPicker<Unit>({
      input: i.unit,
      host: this.el,
      title: "Units",
      columns: ["Symbol", "Formal name"],
      source: async (q) => {
        const s = q.toLowerCase();
        return (await api.listUnits())
          .filter((u) => u.symbol.toLowerCase().includes(s) || u.name.toLowerCase().includes(s))
          .map<PickItem<Unit>>((u) => ({ id: u.id, label: u.symbol, aliases: u.name ? [u.name] : [], sub: u.name, data: u }));
      },
      actions: () => {
        const chord = this.shell.km.chordIn("CreateMasterInline", ["ListField"]);
        return [{ id: "create", label: t("Create Unit"), hint: chord ?? undefined, run: (text) => this.createUnitInline(text) }];
      },
      create: { noun: "unit", run: (text) => this.createUnitInline(text) },
      selectedId: () => this.unit?.id ?? null,
      onPick: (it) => {
        this.setUnit(it.data);
        this.goTo("tax");
      },
    });
    this.pickers.costing = new ListPicker<Costing>({
      input: i.costing,
      host: this.el,
      title: "Costing methods",
      source: (q) => {
        const s = q.trim().toLowerCase();
        return COSTING.filter((c) => !s || [c.label, c.short, t(c.label), t(c.short)].some((x) => x.toLowerCase().includes(s))).map<PickItem<Costing>>((c) => ({
          id: c.id,
          label: t(c.label),
          sub: t(c.short),
          data: c.id,
        }));
      },
      selectedId: () => this.costing,
      onPick: (it) => {
        this.setCosting(it.data);
        this.goTo("qty");
      },
    });
    this.setCosting("AverageCost");
    this.paintPanel();
  }

  protected override async load(): Promise<void> {
    // No country yet (or no rate list): the field still takes a code.
    this.rates = await api.listTaxRates().catch(() => []);
    if (!this.opts.alterId) return;
    const [item, groups, units] = await Promise.all([api.getStockItem(this.opts.alterId), api.listStockGroups(""), api.listUnits()]);
    this.saved = item;
    this.inputs.name.value = item.name;
    this.setGroup(groups.find((g) => g.id === item.group_id) ?? null);
    this.setUnit(units.find((u) => u.id === item.unit_id) ?? null);
    this.inputs.tax.value = item.tax_class ?? "";
    this.paintTax();
    this.setCosting(item.costing ?? "AverageCost");
    const q = M.of(item.opening_qty);
    this.inputs.qty.value = q === 0n ? "" : qtyText(q);
    this.inputs.rate.value = q === 0n ? "" : M.format(M.of(item.opening_rate));
    this.inputs.value.value = M.fmt(item.opening_value, true);
    this.paintPanel();
  }

  private setGroup(g: StockGroup | null): void {
    this.group = g;
    this.inputs.under.value = g?.name ?? t("Primary");
    this.groupEl.textContent = parentLine(g);
  }

  private setUnit(u: Unit | null): void {
    this.unit = u;
    this.inputs.unit.value = u?.symbol ?? "";
    const places = !u ? "" : u.decimals === 0 ? t("whole numbers") : t("up to {n} decimal places", { n: u.decimals });
    this.unitEl.textContent = u ? `(${u.name ? `${u.name}, ` : ""}${places})` : "";
    this.inputs.qty.placeholder = u?.symbol ?? "";
    this.paintPanel();
  }

  private setCosting(c: Costing): void {
    const m = costingOf(c);
    this.costing = m.id;
    this.inputs.costing.value = t(m.label);
    this.costingEl.textContent = `(${t(m.hint)})`;
  }

  private createGroupInline(text: string): void {
    void this.shell.push(
      new StockGroupForm(this.shell, {
        name: text,
        inline: true,
        onCreated: (g) => {
          this.setGroup(g);
          this.goTo("unit");
        },
      }),
    );
  }

  private createUnitInline(text: string): void {
    void this.shell.push(
      new UnitForm(this.shell, {
        name: text,
        inline: true,
        onCreated: (u) => {
          this.setUnit(u);
          this.goTo("tax");
        },
      }),
    );
  }

  protected override pickedText(f: ItemField): string {
    if (f === "under") return this.group?.name ?? t("Primary");
    if (f === "unit") return this.unit?.symbol ?? "";
    if (f === "costing") return t(costingOf(this.costing).label);
    return "";
  }

  protected override createLabel(f: ItemField): string | null {
    if (f === "under") return "Create Stock Group";
    if (f === "unit") return "Create Unit";
    return null;
  }

  /** The rate in force today for the code typed, from the rate list. */
  private paintTax(): void {
    const v = this.inputs.tax.value;
    const r = rateFor(this.rates, v, today(this.shell));
    this.taxEl.textContent = !normaliseClass(v)
      ? ""
      : r
        ? r.class !== normaliseClass(v)
          ? t("({percent}% from {date}, as {code})", { percent: percentText(r.percent), date: D.show(r.from), code: r.class })
          : t("({percent}% from {date})", { percent: percentText(r.percent), date: D.show(r.from) })
        : t("(no rate in the tax rate list for this code yet)");
  }

  /** Value follows quantity × rate as they are typed. */
  private recompute(): void {
    const q = M.parse(this.inputs.qty.value);
    const r = M.parse(this.inputs.rate.value);
    if (q !== null && r !== null && this.inputs.rate.value.trim()) {
      const v = lineAmount(M.abs(q), M.abs(r));
      this.inputs.value.value = v === 0n ? "" : M.format(v);
    }
    this.paintPanel();
  }

  private paintPanel(): void {
    const u = this.unit;
    const value = M.parse(this.inputs.value.value) ?? M.ZERO;
    const rows = [
      h("div.panel-title", t("Stock")),
      h("div.prow", h("span", t("Unit")), h("span", u ? u.symbol : "–")),
      h("div.prow", h("span", t("Opening value")), h("span.num", M.format(value))),
    ];
    const s = this.saved;
    if (s) {
      rows.push(
        h("div.prow", h("span", t("In stock now")), h("span.num", `${qtyText(M.of(s.closing_qty))} ${s.unit}`)),
        h("div.prow", h("span", t("Stock value now")), h("span.num", M.fmt(s.closing_value))),
      );
    }
    this.panel.replaceChildren(...rows);
  }

  private qtyProblem(): string | null {
    const v = this.inputs.qty.value.trim();
    if (!v) return null;
    return checkQty(v, this.unit?.symbol ?? t("This unit"), this.unit?.decimals ?? 4);
  }

  protected next(f: ItemField): void {
    const i = this.inputs;
    switch (f) {
      case "name":
        if (!i.name.value.trim()) return this.shell.warn(t("Enter a name for the stock item."));
        return this.goTo("under");
      case "under":
        return this.goTo("unit");
      case "unit":
        if (!this.unit) return this.shell.warn(t("Pick the unit the item is counted in, or press Alt+C to create one."));
        return this.goTo("tax");
      case "tax":
        i.tax.value = normaliseClass(i.tax.value);
        this.paintTax();
        return this.goTo("costing");
      case "costing":
        return this.goTo("qty");
      case "qty": {
        const problem = this.qtyProblem();
        if (problem) return this.shell.warn(problem);
        // No opening stock: rate and value pass straight through (F2).
        if ((M.parse(i.qty.value) ?? M.ZERO) === 0n) {
          i.qty.value = "";
          i.rate.value = "";
          i.value.value = "";
          this.paintPanel();
          return void this.confirm();
        }
        i.qty.value = qtyText(M.parse(i.qty.value)!);
        return this.goTo("rate");
      }
      case "rate": {
        const r = M.parse(i.rate.value);
        if (r === null || r < 0n) return this.shell.warn(t('"{text}" is not a rate.', { text: i.rate.value }));
        this.recompute();
        return this.goTo("value");
      }
      case "value": {
        const v = M.parse(i.value.value);
        if (v === null || v < 0n) return this.shell.warn(t('"{text}" is not an amount.', { text: i.value.value }));
        // A typed value sets the rate (value ÷ quantity).
        const q = M.parse(i.qty.value) ?? M.ZERO;
        const r = M.parse(i.rate.value) ?? M.ZERO;
        if (q > 0n && lineAmount(q, r) !== v) i.rate.value = M.format(rateOf(v, q));
        this.paintPanel();
        return void this.confirm();
      }
    }
  }

  protected async save(): Promise<void> {
    const i = this.inputs;
    const name = i.name.value.trim();
    if (!name) {
      this.goTo("name");
      return this.shell.warn(t("Enter a name for the stock item."));
    }
    if (!this.unit) {
      this.goTo("unit");
      return this.shell.warn(t("Pick the unit the item is counted in, or press Alt+C to create one."));
    }
    const problem = this.qtyProblem();
    if (problem) {
      this.goTo("qty");
      return this.shell.warn(problem);
    }
    const q = M.parse(i.qty.value) ?? M.ZERO;
    const r = M.parse(i.rate.value);
    const v = M.parse(i.value.value);
    if (r === null || r < 0n) {
      this.goTo("rate");
      return this.shell.warn(t('"{text}" is not a rate.', { text: i.rate.value }));
    }
    if (v === null || v < 0n) {
      this.goTo("value");
      return this.shell.warn(t('"{text}" is not an amount.', { text: i.value.value }));
    }
    if (q === 0n && v !== 0n) {
      this.goTo("qty");
      return this.shell.warn(t("Enter the opening quantity for this value, or clear the value."));
    }
    try {
      const item = await api.saveStockItem({
        id: this.opts.alterId ?? "",
        // As shown; an untouched field keeps the saved code.
        tax_class: normaliseClass(i.tax.value),
        name,
        group_id: this.group?.id ?? "",
        unit_id: this.unit.id,
        opening_qty: M.toDecimal(q),
        opening_rate: M.toDecimal(r),
        opening_value: M.toDecimal(v),
        costing: this.costing,
      });
      const under = item.group_name || t("Primary");
      this.done(item, t(this.opts.alterId ? 'Stock item "{name}" altered under {under}.' : 'Stock item "{name}" created under {under}.', { name: item.name, under }));
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected blank(): void {
    // The group, unit and costing method carry over (F7).
    for (const f of ["name", "tax", "qty", "rate", "value"] as const) this.inputs[f].value = "";
    this.paintTax();
    this.setGroup(this.group);
    this.setUnit(this.unit);
  }

  protected recordName(): string {
    return this.inputs.name.value.trim();
  }

  protected deleteRecord(id: string): Promise<void> {
    return api.deleteStockItem(id);
  }
}

// --------------------------------------------------------------- reports

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : t("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function qtyCell(u: M.Units, unit: string): HTMLElement {
  return h("td.num", u === 0n ? "" : `${qtyText(u)}${unit ? ` ${unit}` : ""}`);
}

function valueCell(u: M.Units): HTMLElement {
  return h("td.num", u === 0n ? "" : M.format(u));
}

/** Closing rate: value ÷ quantity (weighted average cost). */
function rateCell(value: M.Units, qty: M.Units): HTMLElement {
  return h("td.num", qty > 0n ? M.format(rateOf(value, qty)) : "");
}

interface Totals {
  opening_qty: M.Units;
  opening_value: M.Units;
  inward_qty: M.Units;
  inward_value: M.Units;
  outward_qty: M.Units;
  outward_value: M.Units;
  closing_qty: M.Units;
  closing_value: M.Units;
  /** The one unit every item shares, null when mixed (quantities not added). */
  unit: string | null;
  count: number;
}

const MOVEMENT = ["opening_qty", "opening_value", "inward_qty", "inward_value", "outward_qty", "outward_value", "closing_qty", "closing_value"] as const;

function totalsOf(rows: StockSummaryRow[]): Totals {
  const t = { unit: null, count: 0 } as unknown as Totals;
  for (const k of MOVEMENT) t[k] = M.sum(rows.map((r) => M.of(r[k])));
  const units = new Set(rows.map((r) => r.unit));
  t.unit = units.size === 1 ? [...units][0] : null;
  t.count = rows.length;
  return t;
}

interface GroupNode {
  name: string;
  groups: Map<string, GroupNode>;
  items: StockSummaryRow[];
  /** Every item at or below this group. */
  all: StockSummaryRow[];
}

/** Items by stock group path ("Raw Material / Steel"), as a tree. */
function groupTree(rows: StockSummaryRow[]): GroupNode {
  const root: GroupNode = { name: "", groups: new Map(), items: [], all: [] };
  for (const r of rows) {
    let at = root;
    root.all.push(r);
    for (const part of r.group ? r.group.split(" / ") : []) {
      let next = at.groups.get(part);
      if (!next) {
        next = { name: part, groups: new Map(), items: [], all: [] };
        at.groups.set(part, next);
      }
      next.all.push(r);
      at = next;
    }
    at.items.push(r);
  }
  return root;
}

/**
 * Stock Summary: closing quantity, rate and value of every item, grouped by
 * stock group with subtotals. Alt+F1 (or Alt+F5) shows opening, inwards and
 * outwards too; Enter on an item opens its vouchers.
 */
export class StockSummaryReport extends Report {
  title = "Stock Summary";
  protected override period = "range" as const;
  protected override noteable = true;
  private detailed = false;
  private rows: StockSummaryRow[] = [];
  /** Stock groups folded to their total row (Enter on a group toggles). */
  private collapsed = new Set<string>();

  constructor(
    shell: Shell,
    private from = shell.periodFrom(),
    private to = shell.periodTo(),
  ) {
    super(shell);
  }

  override handles(a: string): boolean {
    return a === "ToggleDetailed" || super.handles(a);
  }

  override bottom() {
    const out = super.bottom();
    const toggle = this.shell.btn("ToggleDetailed", this.contexts(), this.detailed ? "Condensed" : "Detailed");
    if (toggle) out.splice(Math.max(0, out.length - 1), 0, toggle);
    return out;
  }

  override action(a: string): boolean {
    if (a !== "ToggleDetailed") return super.action(a);
    this.detailed = !this.detailed;
    this.rerender();
    this.shell.refresh();
    return true;
  }

  /** Render again, keeping the cursor on the same row. */
  private rerender(): void {
    const at = this.picks[this.cursor]?.el.dataset.key;
    this.render();
    const back = this.picks.findIndex((p) => p.el.dataset.key === at);
    if (back >= 0) this.cursor = back;
    this.paint();
  }

  async load(): Promise<void> {
    const s = await api.stockSummary(this.from, this.to);
    this.rows = s.rows;
    this.render();
  }

  private cells(t: Totals | StockSummaryRow, unit: string | null): HTMLElement[] {
    const v = (k: (typeof MOVEMENT)[number]) => {
      const x = t[k];
      return typeof x === "string" ? M.of(x) : x;
    };
    // Quantities of different units are never added up.
    const q = (k: (typeof MOVEMENT)[number]) => (unit === null ? h("td.num") : qtyCell(v(k), unit));
    const closing = [q("closing_qty"), rateCell(v("closing_value"), unit === null ? M.ZERO : v("closing_qty")), valueCell(v("closing_value"))];
    if (!this.detailed) return closing;
    return [
      q("opening_qty"),
      valueCell(v("opening_value")),
      q("inward_qty"),
      valueCell(v("inward_value")),
      q("outward_qty"),
      valueCell(v("outward_value")),
      ...closing,
    ];
  }

  private render(): void {
    this.heading(t("Stock Summary"), range(this.from, this.to));
    this.resetPicks();
    const tbody = h("tbody");
    const indent = (depth: number) => ({ style: { paddingInlineStart: `${10 + depth * 18}px` } });
    const walk = (node: GroupNode, depth: number) => {
      type Child = { name: string; group?: GroupNode; item?: StockSummaryRow };
      const children: Child[] = [
        ...[...node.groups.values()].map((g) => ({ name: g.name, group: g })),
        ...node.items.map((r) => ({ name: r.name, item: r })),
      ].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
      for (const c of children) {
        if (c.group) {
          const t = totalsOf(c.group.all);
          const key = `g:${depth}:${c.group.name}`;
          const folded = this.collapsed.has(key);
          const row = h("tr.group", h("td", indent(depth), `${folded ? (dir() === "rtl" ? "◂" : "▸") : "▾"} ${c.group.name}`), ...this.cells(t, t.unit));
          row.dataset.key = key;
          row.setAttribute("aria-expanded", String(!folded));
          const toggle = () => {
            if (folded) this.collapsed.delete(key);
            else this.collapsed.add(key);
            this.rerender();
          };
          tbody.append(this.pick(row, toggle));
          if (!folded) walk(c.group, depth + 1);
        } else if (c.item) {
          const r = c.item;
          const row = h("tr", h("td.strong", indent(depth), r.name), ...this.cells(r, r.unit));
          row.dataset.key = `i:${r.item}`;
          tbody.append(
            this.pick(row, () => void this.shell.push(new StockItemVouchersReport(this.shell, r.item, this.from, this.to)), 0, { kind: "stock_item", id: r.item, label: r.name }),
          );
        }
      }
    };
    const tree = groupTree(this.rows);
    walk(tree, 0);
    const cols = this.detailed ? 10 : 4;
    if (!this.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: cols }, t("No stock items yet. Create one from Create → Stock Item."))));
    const head = this.detailed
      ? ["Opening Qty", "Opening Value", "Inward Qty", "Inward Value", "Outward Qty", "Outward Value", "Closing Qty", "Rate", "Closing Value"]
      : ["Closing Qty", "Rate", "Value"];
    const grand = totalsOf(this.rows);
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        { class: this.detailed ? "detailed" : "" },
        h("thead", h("tr", h("th", inUse("Particulars", t("Particulars (figure)"))), ...head.map((c) => h("th.num", t(c))))),
        tbody,
        h("tfoot", h("tr", h("td", t("Grand Total")), ...this.cells(grand, grand.unit))),
      ),
      h("div.note", t("Closing stock is valued at weighted average cost. Outward value is the sales value of goods sent out.")),
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

/** One item's movements in a period; Enter opens the voucher (R1). */
export class StockItemVouchersReport extends Report {
  title = "Stock Item Vouchers";
  protected override period = "range" as const;

  constructor(
    shell: Shell,
    private itemId: string,
    private from: string,
    private to: string,
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = await api.stockItemVouchers(this.itemId, this.from, this.to);
    const s = r.summary;
    this.title = t("Stock Item: {name}", { name: r.item_name });
    this.heading(r.item_name, t("{period} · quantities in {unit}", { period: range(this.from, this.to), unit: r.unit }));
    this.resetPicks();
    const qty = (q: string) => h("td.num", M.of(q) === 0n ? "" : qtyText(M.of(q)));
    const tbody = h("tbody");
    tbody.append(
      h(
        "tr.opening",
        h("td"),
        h("td.strong", t("Opening Balance")),
        h("td"),
        h("td"),
        h("td.num"),
        valueCell(M.of(s.opening_value)),
        h("td.num"),
        h("td.num"),
        h("td.num", qtyText(M.of(s.opening_qty))),
      ),
    );
    for (const v of r.rows) {
      tbody.append(
        this.pick(
          h(
            "tr",
            h("td", D.show(v.date)),
            h("td.strong", v.particulars),
            h("td", t(KIND_LABEL[v.kind])),
            h("td", ltr(v.number)),
            qty(v.inward_qty),
            valueCell(M.of(v.inward_value)),
            qty(v.outward_qty),
            valueCell(M.of(v.outward_value)),
            h("td.num", qtyText(M.of(v.closing_qty))),
          ),
          () => void this.shell.push(savedVoucherScreen(this.shell, v.kind, v.date, v.voucher)),
        ),
      );
    }
    if (!r.rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 9 }, t("No vouchers in this period. Press F2 to change it."))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h(
          "thead",
          h(
            "tr",
            h("th", t("Date")),
            h("th", t("Particulars")),
            h("th", t("Vch Type")),
            h("th", t("Vch No.")),
            h("th.num", t("Inward Qty")),
            h("th.num", t("Inward Value")),
            h("th.num", t("Outward Qty")),
            h("th.num", t("Outward Value")),
            h("th.num", t("Closing Qty")),
          ),
        ),
        tbody,
        h(
          "tfoot",
          h(
            "tr",
            h("td", { colSpan: 4 }, t("Totals")),
            qty(s.inward_qty),
            valueCell(M.of(s.inward_value)),
            qty(s.outward_qty),
            valueCell(M.of(s.outward_value)),
            h("td.num"),
          ),
          h(
            "tr",
            h("td", { colSpan: 4 }, t("Closing Balance · value {value}", { value: M.fmt(s.closing_value) })),
            h("td.num"),
            h("td.num"),
            h("td.num"),
            h("td.num"),
            h("td.num", qtyText(M.of(s.closing_qty))),
          ),
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

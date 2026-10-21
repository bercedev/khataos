// ListPicker: the one list component used by every list field (rules P1–P8).
//
// - A panel with a header, optional column titles, action rows pinned at the
//   top and (optionally sectioned) items; hidden-row counts at each end (P1).
// - Sits on the right of its host, or inline (P2).
// - Filters on every keystroke, matching anywhere, with highlights; the
//   cursor jumps to the first match (P3).
// - Up/Down wrap, Enter selects (P4).
// - Unmatched text never creates anything silently: it shows a
//   `Create "<text>"…` row that must itself be chosen (P6).
// - Alt+C creates inline (P7); Ctrl+Enter would alter in place (P8).
import { clear, h, highlight } from "./dom";
import { t } from "./i18n";
import type { KeyEv } from "./shell";

export interface PickItem<T> {
  id: string;
  label: string;
  aliases?: string[];
  /** Right-hand column (balance, group…). */
  sub?: string;
  section?: string;
  disabled?: boolean;
  data: T;
}

export interface PickAction {
  id: string;
  label: string;
  /** Shown at the right of the row, e.g. "Alt+C". */
  hint?: string;
  run: (text: string) => void;
}

export interface PickerOptions<T> {
  input: HTMLInputElement;
  host: HTMLElement;
  title: string;
  columns?: [string, string];
  inline?: boolean;
  /** Items for a query. May be async; stale answers are dropped. */
  source: (query: string) => PickItem<T>[] | Promise<PickItem<T>[]>;
  /** Pinned rows at the top ("End of List", "Create"…). */
  actions?: () => PickAction[];
  /** Offer `Create "<text>"…` for unmatched text, and Alt+C. */
  create?: { noun: string; run: (text: string) => void };
  /** Item id to put the cursor on when the list opens. */
  selectedId?: () => string | null;
  /** Initial cursor on this pinned action id when the text is empty. */
  preferAction?: () => string | null;
  onPick: (item: PickItem<T>) => void;
  onAlter?: (item: PickItem<T>) => void;
  /** Text to show when nothing matches and nothing can be created. */
  noMatch?: (text: string) => string;
  /** After each redraw, with the item under the cursor: live previews (theme menu). */
  onCursor?: (item: PickItem<T> | null) => void;
}

type Row<T> =
  | { kind: "action"; action: PickAction }
  | { kind: "create"; text: string }
  | { kind: "item"; item: PickItem<T> };

/** Fallback row height; the real one is the CSS `--row` variable (in px). */
const ROW_PX = 26;

/** Row height in px from `--row` at `el`: 26 on the desktop, taller for touch (mobile.css). */
function rowHeight(el: Element): number {
  const px = parseFloat(getComputedStyle(el).getPropertyValue("--row"));
  return px > 0 ? px : ROW_PX;
}

export class ListPicker<T> {
  private panel: HTMLElement;
  private list: HTMLElement;
  private pinned: HTMLElement;
  private upCount: HTMLElement;
  private downCount: HTMLElement;
  private items: PickItem<T>[] = [];
  private query = "";
  private cursor = 0;
  private top = 0;
  private seq = 0;
  private opened = false;
  private onInput = () => void this.refilter(this.opts.input.value, true);

  constructor(private opts: PickerOptions<T>) {
    this.pinned = h("div.pick-pinned");
    this.list = h("div.pick-list", { role: "listbox" });
    this.upCount = h("div.pick-count.up");
    this.downCount = h("div.pick-count.down");
    // Titles and column names are fixed English text, shown in the display language.
    const cols = opts.columns
      ? h("div.pick-cols", h("span", t(opts.columns[0])), h("span", t(opts.columns[1])))
      : null;
    this.panel = h(
      "div.picker",
      { class: opts.inline ? "inline" : "side" },
      h("div.pick-head", t(opts.title)),
      cols,
      this.pinned,
      this.upCount,
      this.list,
      this.downCount,
    );
  }

  get isOpen(): boolean {
    return this.opened;
  }

  /** Open with the full list; the field's current text is selected so typing replaces it. */
  async open(): Promise<void> {
    if (!this.opened) {
      this.opened = true;
      this.opts.host.append(this.panel);
      this.opts.input.addEventListener("input", this.onInput);
    }
    await this.refilter("", false);
  }

  close(): void {
    if (!this.opened) return;
    this.opened = false;
    this.seq++;
    this.panel.remove();
    this.opts.input.removeEventListener("input", this.onInput);
  }

  /** Reload the items (after a master was created elsewhere). */
  async reload(): Promise<void> {
    if (this.opened) await this.refilter(this.query, this.query !== "");
  }

  get text(): string {
    return this.query;
  }

  private rows(): Row<T>[] {
    const out: Row<T>[] = (this.opts.actions?.() ?? []).map((action) => ({ kind: "action" as const, action }));
    const q = this.query.trim();
    if (q && this.opts.create && !this.items.some((i) => i.label.toLowerCase() === q.toLowerCase())) {
      if (this.items.length === 0) out.push({ kind: "create", text: q });
    }
    for (const item of this.items) out.push({ kind: "item", item });
    return out;
  }

  private pinnedCount(): number {
    return this.rows().filter((r) => r.kind !== "item").length;
  }

  private async refilter(text: string, typed: boolean): Promise<void> {
    const seq = ++this.seq;
    const items = await this.opts.source(text.trim());
    if (seq !== this.seq || !this.opened) return;
    this.items = items;
    this.query = text;
    const rows = this.rows();
    const pinned = this.pinnedCount();
    let cursor = -1;
    if (typed && text.trim()) {
      // Cursor jumps to the first match, else to the Create row (P3, P6).
      cursor = items.length ? pinned : rows.findIndex((r) => r.kind === "create");
    } else {
      const sel = this.opts.selectedId?.();
      const pref = this.opts.preferAction?.();
      if (sel) cursor = rows.findIndex((r) => r.kind === "item" && r.item.id === sel);
      if (cursor < 0 && pref) cursor = rows.findIndex((r) => r.kind === "action" && r.action.id === pref);
      if (cursor < 0) cursor = items.length ? pinned : 0;
    }
    this.cursor = Math.max(0, cursor);
    this.top = 0;
    this.render();
  }

  private visibleCount(): number {
    const row = rowHeight(this.list);
    const hgt = this.list.clientHeight || 12 * row;
    return Math.max(3, Math.floor(hgt / row));
  }

  private render(): void {
    const rows = this.rows();
    const pinned = this.pinnedCount();
    clear(this.pinned);
    clear(this.list);
    rows.slice(0, pinned).forEach((r, i) => this.pinned.append(this.rowEl(r, i)));
    const items = rows.length - pinned;
    const vis = this.visibleCount();
    const ci = this.cursor - pinned;
    if (ci >= 0) {
      if (ci < this.top) this.top = ci;
      if (ci >= this.top + vis) this.top = ci - vis + 1;
    }
    this.top = Math.max(0, Math.min(this.top, Math.max(0, items - vis)));
    let lastSection: string | undefined;
    let shown = 0;
    let itemsShown = 0;
    for (let i = this.top; i < items && shown < vis; i++) {
      const r = rows[pinned + i];
      if (r.kind === "item" && r.item.section && r.item.section !== lastSection) {
        lastSection = r.item.section;
        this.list.append(h("div.pick-section", r.item.section));
        shown++;
        if (shown >= vis) break;
      }
      this.list.append(this.rowEl(r, pinned + i));
      shown++;
      itemsShown++;
    }
    const below = Math.max(0, items - this.top - itemsShown);
    this.upCount.textContent = this.top > 0 ? `${this.top} ▲` : "";
    this.downCount.textContent = below > 0 ? `${below} ▼` : "";
    if (items === 0 && pinned === 0) {
      const text = this.query.trim();
      this.list.append(h("div.pick-empty", text ? this.opts.noMatch?.(text) ?? t('Nothing matches "{text}"', { text }) : t("Nothing here yet")));
    } else if (items === 0 && this.query.trim() && !this.opts.create) {
      const text = this.query.trim();
      this.list.append(h("div.pick-empty", this.opts.noMatch?.(text) ?? t('Nothing matches "{text}"', { text })));
    }
    this.opts.onCursor?.(this.current());
  }

  private rowEl(r: Row<T>, index: number): HTMLElement {
    const cur = index === this.cursor;
    const pick = () => {
      this.cursor = index;
      this.choose();
    };
    if (r.kind === "action") {
      return h(
        "div.pick-row.action",
        { class: cur ? "cur" : "", role: "option", "aria-selected": cur ? "true" : "false", onmousedown: (e: Event) => e.preventDefault(), onclick: pick },
        h("span", r.action.label),
        r.action.hint ? h("span.pick-sub", r.action.hint) : null,
      );
    }
    if (r.kind === "create") {
      return h(
        "div.pick-row.create",
        { class: cur ? "cur" : "", role: "option", onmousedown: (e: Event) => e.preventDefault(), onclick: pick },
        h("span", t('Create {noun} "{text}"…', { noun: t(this.opts.create!.noun), text: r.text })),
        h("span.pick-sub", "Enter"),
      );
    }
    const it = r.item;
    const q = this.query.trim();
    const alias = q && !it.label.toLowerCase().includes(q.toLowerCase())
      ? it.aliases?.find((a) => a.toLowerCase().includes(q.toLowerCase()))
      : undefined;
    return h(
      "div.pick-row",
      {
        class: `${cur ? "cur" : ""} ${it.disabled ? "off" : ""}`,
        role: "option",
        "aria-selected": cur ? "true" : "false",
        onmousedown: (e: Event) => e.preventDefault(),
        onclick: pick,
      },
      h("span.pick-label", highlight(it.label, q), alias ? h("span.pick-alias", " · ", highlight(alias, q)) : null),
      it.sub ? h("span.pick-sub", it.sub) : null,
    );
  }

  private move(delta: number): void {
    const n = this.rows().length;
    if (!n) return;
    this.cursor = (((this.cursor + delta) % n) + n) % n; // wraps (P4)
    this.render();
  }

  private choose(): boolean {
    const r = this.rows()[this.cursor];
    if (!r) return false;
    if (r.kind === "action") r.action.run(this.query.trim());
    else if (r.kind === "create") this.opts.create!.run(r.text);
    else if (!r.item.disabled) this.opts.onPick(r.item);
    return true;
  }

  /** The highlighted item, if the cursor is on one. */
  current(): PickItem<T> | null {
    const r = this.rows()[this.cursor];
    return r && r.kind === "item" ? r.item : null;
  }

  /** Raw keys while open. Returns true when handled. */
  key(e: KeyEv): boolean {
    if (!this.opened) return false;
    switch (e.chord) {
      case "Up":
        this.move(-1);
        return true;
      case "Down":
        this.move(1);
        return true;
      case "PageUp":
        this.move(-Math.min(this.visibleCount(), Math.max(1, this.cursor)));
        return true;
      case "PageDown":
        this.move(Math.min(this.visibleCount(), Math.max(1, this.rows().length - 1 - this.cursor)));
        return true;
      case "Enter":
        return this.choose();
      case "Esc":
        // F4: Esc clears typed text first; a second Esc goes to the screen.
        if (this.opts.input.value !== "") {
          this.opts.input.value = "";
          void this.refilter("", false);
          return true;
        }
        return false;
    }
    return false;
  }

  /** Keymap actions in the ListField context. */
  action(a: string): boolean {
    if (!this.opened) return false;
    if (a === "CreateMasterInline" && this.opts.create) {
      this.opts.create.run(this.query.trim());
      return true;
    }
    if (a === "AlterMasterInline") {
      const it = this.current();
      if (it && this.opts.onAlter) this.opts.onAlter(it);
      return true;
    }
    return false;
  }
}

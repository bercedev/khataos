// The accountant's desk (T on the home menu, "Accountant's Desk" in Go To):
// overdue items, today, the next 7 days and a month calendar, with the
// company's notes and to-dos. Statutory dates come from the compliance pack
// and are estimates; reminders are read from the books. Notes are shared
// records of the company: they sync with the books to co-workers and the
// accountant (src-tauri/src/desk.rs, docs/sync-settings.md).
//
// Keys: ↑ ↓ move, Enter opens, Space ticks a to-do done (or marks a return
// filed), Alt+C or Alt+W adds a to-do, Ctrl+Enter alters it, Alt+D deletes
// it, Tab moves between the lists and the calendar, where the arrows move
// by day and week, PgUp/PgDn by month and Home goes to today. F12 sets the
// filing pattern.
import { zatcaQueueApi } from "../api";
import { deskApi, type BillDue, type CalendarSettings, type Category, type Desk, type DueDto, type Entity, type Frequency, type MsmeDue, type NoteDto, type NoteLink, type PostDated } from "../desk-api";
import { h } from "../dom";
import * as D from "../dates";
import { dir, hijriOn, language, ltr, t } from "../i18n";
import * as M from "../money";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, type Context } from "../types";
import { LedgerStatementReport } from "./reports";
import { savedVoucherScreen } from "./stockjournal";
import "../desk.css";

const NEW_KEY = "Alt+C";
const DELETE_KEY = "Alt+D";
const LOOK_AHEAD_DAYS = 400;

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

// ------------------------------------------------------------ notes

/** Alt+W anywhere, Alt+C on the desk: a note or to-do, on a record or not. Resolves with the saved note. */
export async function editNote(shell: Shell, opts: { link?: NoteLink; note?: NoteDto; due?: string } = {}): Promise<NoteDto | null> {
  const n = opts.note;
  const ref = today(shell);
  const label = n?.link?.label || opts.link?.label;
  const v = await shell.dialog(
    n ? "Alter note" : label ? t("Note on {what}", { what: ltr(label) }) : "New to-do",
    [
      { label: "Title", value: n?.title ?? "" },
      { label: "Details", value: n?.text ?? "" },
      { label: "Due on (blank: none)", value: n?.due ? D.editable(n.due) : opts.due ? D.editable(opts.due) : "" },
    ],
    ([title, , due]) => {
      if (!title.trim()) return t("A note needs a title.");
      if (due.trim() && !D.parseUser(due, ref)) return t('"{text}" is not a date.', { text: due });
      return null;
    },
  );
  if (!v) return null;
  const [title, text, due] = v;
  try {
    const saved = await deskApi.saveNote({
      id: n?.id ?? null,
      title,
      text,
      due: due.trim() ? D.parseUser(due, ref) : null,
      done: n?.done ?? false,
      link_kind: n?.link?.kind ?? opts.link?.kind ?? null,
      link_id: n?.link?.id ?? opts.link?.id ?? null,
    });
    shell.info(t("Note saved. Your accountant and co-workers get it at the next sync."));
    return saved;
  } catch (e) {
    shell.error(e);
    return null;
  }
}

/** Notes on one record (a ledger statement, a voucher); none when the desk can't be read. */
export function notesFor(kind: NoteLink["kind"], id: string): Promise<NoteDto[]> {
  return deskApi.notes(kind, id).catch(() => []);
}

/** Every note, for a screen that marks several records (the vouchers on a ledger statement). */
export function allNotes(): Promise<NoteDto[]> {
  return deskApi.notes().catch(() => []);
}

/** The notes block shown on a ledger statement or a voucher. */
export function notesBlock(notes: NoteDto[]): HTMLElement | null {
  if (!notes.length) return null;
  return h(
    "div.desk-notes",
    h("div.desk-notes-title", t("Notes ({n})", { n: notes.length })),
    ...notes.map((n) =>
      h(
        "div.desk-note",
        { class: n.done ? "done" : "" },
        h("span.desk-note-title", { dir: "auto" }, n.title),
        n.text ? h("span.desk-note-text", { dir: "auto" }, n.text) : null,
        h(
          "span.muted",
          [n.due ? t("due {date}", { date: D.show(n.due) }) : "", n.done ? t("done") : "", n.link?.kind === "voucher" && n.link.label ? ltr(voucherLabel(n)) : "", ltr(n.updated_by)]
            .filter(Boolean)
            .join(" · "),
        ),
      ),
    ),
  );
}

function voucherLabel(n: NoteDto): string {
  const l = n.link;
  if (!l?.voucher_kind) return l?.label ?? "";
  // "Payment 26-27/4" from Rust; shown with the translated type.
  return `${t(KIND_LABEL[l.voucher_kind])} ${l.label.slice(l.label.indexOf(" ") + 1)}`;
}

function linkText(n: NoteDto): string {
  if (!n.link) return "";
  if (n.link.missing) return t("(deleted record)");
  return n.link.kind === "voucher" ? voucherLabel(n) : n.link.label;
}

// ------------------------------------------------------------- items

type Item =
  | { k: "note"; date: string | null; note: NoteDto }
  | { k: "due"; date: string; due: DueDto }
  | { k: "bill"; date: string; bill: BillDue }
  | { k: "pdc"; date: string; pdc: PostDated }
  | { k: "msme"; date: string; msme: MsmeDue }
  | { k: "zatca"; date: string; waiting: number; overdue: number }
  /** "Set up your compliance profile", until it is saved. */
  | { k: "setup"; date: null };

interface Row {
  el: HTMLElement;
  item: Item;
}

const TAG: Record<Item["k"], string> = {
  note: "To-do",
  due: "Return",
  bill: "Bill",
  pdc: "Post-dated",
  msme: "MSME 45 days",
  zatca: "ZATCA",
  setup: "Setup",
};

/** How the desk groups and filters (Ctrl+F): the rule's category, reminders from the books, to-dos. */
type Group = Category | "reminder" | "todo";
const GROUPS: Group[] = ["return", "payment", "filing", "audit", "reminder", "todo"];
const GROUP_LABEL: Record<Group, string> = {
  return: "Tax returns",
  payment: "Payments",
  filing: "Company filings",
  audit: "Audit",
  reminder: "Reminders",
  todo: "To-dos",
};
const CATEGORY_TAG: Record<Category, string> = { return: "Return", payment: "Payment", filing: "Filing", audit: "Audit" };

function groupOf(i: Item): Group | null {
  if (i.k === "due") return i.due.category;
  if (i.k === "note") return "todo";
  if (i.k === "setup") return null;
  return "reminder";
}

function dueTitle(d: DueDto): string {
  return d.subject ? `${t(d.title)} (${d.subject})` : t(d.title);
}

/** What happens to the date on a weekend, after "Estimate". */
function shiftText(d: DueDto): string {
  if (d.shift.kind === "fixed") return t("the date doesn't move for weekends");
  if (d.shift.kind === "next_working_day")
    return d.unadjusted !== d.due ? t("moved from {date}, a weekend", { date: D.show(d.unadjusted) }) : t("moves to the next working day after a weekend; holidays not counted");
  return t("may move for weekends, holidays or extensions");
}

const ENTITY_LETTER: Record<Entity, string> = { company: "C", llp: "L", partnership: "P", sole_proprietor: "S", s_corporation: "X" };
const ENTITY_LABEL: Record<Entity, string> = { company: "company", llp: "LLP", partnership: "partnership", sole_proprietor: "sole proprietor", s_corporation: "S corporation" };
const FREQ_LETTER: Record<Frequency, string> = { monthly: "M", quarterly: "Q", annual: "A" };

/** Whether an item still needs doing (open to-do, unfiled return; reminders always). */
function open(i: Item): boolean {
  if (i.k === "note") return !i.note.done;
  if (i.k === "due") return !i.due.filed;
  return true;
}

/** One field of the profile dialog: its text, and how to read it back into the settings. */
interface Field {
  label: string;
  value: string;
  read: (v: string, out: CalendarSettings) => string | null;
}

const MONTH = /^(1[0-2]|[1-9])$/;
const yes = (v: string) => /^[yY]/.test(v.trim());
const yn = (v: string) => (/^[yYnN]/.test(v.trim()) ? null : t("Type Y or N."));

function period(d: DueDto): string {
  return d.period_from === d.period_to ? D.show(d.period_to) : t("{from} to {to}", { from: D.show(d.period_from), to: D.show(d.period_to) });
}

/** Monday first; Sunday first for Gulf companies, as their calendars run. */
function weekStart(shell: Shell): number {
  const c = shell.company?.country?.code;
  return c === "SA" || c === "AE" ? 0 : 1;
}

function monthOf(iso: string): string {
  return `${iso.slice(0, 7)}-01`;
}

function addMonths(first: string, n: number): string {
  const [y, m] = first.split("-").map(Number);
  const k = y * 12 + (m - 1) + n;
  return `${Math.floor(k / 12)}-${String((k % 12) + 1).padStart(2, "0")}-01`;
}

function lastOfMonth(first: string): string {
  return D.addDays(addMonths(first, 1), -1);
}

function dayOfWeek(iso: string): number {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d)).getUTCDay();
}

const MONTH_FORMATS = new Map<string, Intl.DateTimeFormat>();

/** "October 2026", "أكتوبر 2026": Gregorian, Latin digits. */
function monthTitle(first: string): string {
  const l = language();
  let f = MONTH_FORMATS.get(l);
  if (!f) {
    f = new Intl.DateTimeFormat(l === "ar" ? "ar-AE-u-ca-gregory-nu-latn" : "en-GB", { month: "long", year: "numeric", timeZone: "UTC" });
    MONTH_FORMATS.set(l, f);
  }
  const [y, m] = first.split("-").map(Number);
  return f.format(new Date(Date.UTC(y, m - 1, 1)));
}

/** "Rabi II – Jumada I 1448 AH" for the Hijri months a Gregorian month spans. */
function hijriSpan(first: string): string | null {
  const a = D.hijri(first);
  const b = D.hijri(lastOfMonth(first));
  if (!a || !b) return null;
  const rest = (s: string) => s.slice(s.indexOf(" ") + 1);
  return rest(a) === rest(b) ? rest(a) : `${rest(a)} – ${rest(b)}`;
}

function hijriDay(iso: string): string | null {
  const x = D.hijri(iso);
  return x ? x.slice(0, x.indexOf(" ")) : null;
}

// ------------------------------------------------------------ screen

type Pane = "list" | "cal" | "day";

export class DeskScreen implements Screen {
  title = "Accountant's Desk";
  el = h("section.report.desk");
  private head = h("div.rhead");
  private banner = h("div.desk-banner");
  private listEl = h("div.desk-lists");
  private calEl = h("div.desk-cal");
  private dayEl = h("div.desk-day");
  private desk: Desk | null = null;
  private zatca: { waiting: number; overdue: number } | null = null;
  /** Statutory dates by month shown ("2026-10-01"), fetched as the calendar moves. */
  private monthDues = new Map<string, DueDto[]>();
  private month: string;
  private sel: string;
  private pane: Pane = "list";
  private listRows: Row[] = [];
  private dayRows: Row[] = [];
  private cursor = { list: 0, day: 0 };
  private dayCells = new Map<string, HTMLElement>();
  /** Ctrl+F: one category only (null: everything). */
  private filter: Group | null = null;

  constructor(private shell: Shell) {
    this.sel = today(shell);
    this.month = monthOf(this.sel);
    this.el.append(this.head, this.banner, h("div.desk-grid", this.listEl, h("div.desk-side", this.calEl, this.dayEl)));
  }

  rebuild(): Screen {
    return new DeskScreen(this.shell);
  }

  async init(): Promise<void> {
    await this.load();
  }

  resume(): void {
    void this.reload();
  }

  private async load(): Promise<void> {
    const asAt = today(this.shell);
    const [desk, queue] = await Promise.all([
      deskApi.overview(asAt, D.addDays(asAt, LOOK_AHEAD_DAYS)),
      this.shell.company?.country?.code === "SA" ? zatcaQueueApi.queue().catch(() => null) : Promise.resolve(null),
    ]);
    this.desk = desk;
    this.zatca = queue ? { waiting: queue.pending + queue.not_generated + queue.failed, overdue: queue.overdue } : null;
    this.monthDues.clear();
    await this.loadMonth();
    this.render();
  }

  private async reload(): Promise<void> {
    try {
      await this.load();
      this.shell.refresh();
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** The visible grid's statutory dates (six weeks around the month). */
  private async loadMonth(): Promise<void> {
    if (this.monthDues.has(this.month)) return;
    const [from, to] = this.gridRange();
    this.monthDues.set(this.month, await deskApi.dueDates(from, to).catch(() => []));
  }

  private gridRange(): [string, string] {
    const back = (dayOfWeek(this.month) - weekStart(this.shell) + 7) % 7;
    const from = D.addDays(this.month, -back);
    return [from, D.addDays(from, 41)];
  }

  // ---------------------------------------------------------- items

  /** Everything with a date, for the lists (statutory from the overview) or the calendar (from the month). */
  private dated(statutory: DueDto[]): Item[] {
    const d = this.desk;
    if (!d) return [];
    const out: Item[] = [];
    for (const n of d.notes) if (n.due) out.push({ k: "note", date: n.due, note: n });
    for (const s of statutory) out.push({ k: "due", date: s.due, due: s });
    for (const b of d.bills) out.push({ k: "bill", date: b.due, bill: b });
    for (const p of d.post_dated) out.push({ k: "pdc", date: p.date, pdc: p });
    for (const m of d.msme) out.push({ k: "msme", date: m.limit, msme: m });
    return out.sort((a, b) => (a.date ?? "").localeCompare(b.date ?? ""));
  }

  private onDay(iso: string): Item[] {
    const dues = this.monthDues.get(this.month) ?? [];
    const items = this.dated(dues).filter((i) => i.date === iso && this.shown(i));
    if (this.zatca && iso === today(this.shell) && this.zatca.waiting + this.zatca.overdue > 0) items.push({ k: "zatca", date: iso, ...this.zatca });
    return items;
  }

  // --------------------------------------------------------- render

  private render(): void {
    const d = this.desk;
    if (!d) return;
    const asAt = today(this.shell);
    const showing = this.filter ? t(GROUP_LABEL[this.filter]) : t("everything");
    this.head.replaceChildren(
      h("div.rtitle", t("Accountant's Desk")),
      h("div.rsub", t("As at {date}", { date: D.show(asAt) }), h("span.muted", ` · ${t("Showing: {what} (Ctrl+F)", { what: showing })}`)),
    );
    this.banner.replaceChildren(
      h("p.desk-estimate", t("Due dates are estimates worked out from the published rules. Governments extend deadlines: check the official portal before you file.")),
      ...(d.can_write ? [] : [h("p.warn", t("You can read the desk; adding or changing notes needs Data Entry or above."))]),
    );
    this.renderLists();
    this.renderCalendar();
    this.renderDay();
    this.paint();
  }

  private shown(i: Item): boolean {
    return this.filter === null || i.k === "setup" || groupOf(i) === this.filter;
  }

  private renderLists(): void {
    const d = this.desk!;
    const asAt = today(this.shell);
    const soon = D.addDays(asAt, 7);
    const later = D.addDays(asAt, 90);
    const all = this.dated(d.statutory).filter(open);
    const zatca = this.zatca;
    const overdue = all.filter((i) => i.date! < asAt);
    const todays = all.filter((i) => i.date === asAt);
    const next = all.filter((i) => i.date! > asAt && i.date! <= soon);
    // Annual filings come round rarely: show returns and to-dos up to 90 days ahead.
    const ahead = all.filter((i) => (i.k === "due" || i.k === "note") && i.date! > soon && i.date! <= later);
    if (zatca?.overdue) overdue.unshift({ k: "zatca", date: asAt, waiting: 0, overdue: zatca.overdue });
    if (zatca?.waiting) todays.unshift({ k: "zatca", date: asAt, waiting: zatca.waiting, overdue: 0 });
    this.listRows = [];
    // By due date, grouped by category (tax returns, payments, company filings, audit, reminders, to-dos).
    const grouped = (items: Item[]) => {
      const out: (HTMLElement | null)[] = [];
      for (const g of GROUPS) {
        const of = items.filter((i) => groupOf(i) === g).sort((a, b) => (a.date ?? "9999").localeCompare(b.date ?? "9999"));
        if (!of.length) continue;
        out.push(this.filter ? null : h("div.desk-group", t(GROUP_LABEL[g])), ...of.map((i) => this.row(i, "list")));
      }
      return out;
    };
    const section = (title: string, items: Item[], empty: string) => {
      const mine = items.filter((i) => this.shown(i));
      return h(
        "div.desk-section",
        h("div.tside-title", t(title), h("span.desk-count", String(mine.length))),
        mine.length ? h("div.desk-rows", ...grouped(mine)) : h("p.muted.desk-empty", t(empty)),
      );
    };
    const setup = !d.settings.profile_set && d.settings.defaults !== null;
    const notes = d.notes.map((n) => ({ k: "note", date: n.due, note: n }) as Item);
    this.listEl.replaceChildren(
      ...(setup ? [h("div.desk-section.desk-setup", h("div.desk-rows", this.row({ k: "setup", date: null }, "list")))] : []),
      section("Overdue", overdue, "Nothing overdue."),
      section("Today", todays, "Nothing due today."),
      section("Next 7 days", next, "Nothing due in the next 7 days."),
      section("Next 90 days", ahead, "No returns or filings in the next 90 days."),
      ...(this.filter === null || this.filter === "todo" ? [section("To-dos and notes", notes, "No to-dos yet. Press Alt+C to add one.")] : []),
    );
    this.cursor.list = Math.min(this.cursor.list, Math.max(0, this.listRows.length - 1));
  }

  /** Ctrl+F: the next category, then everything again. */
  private nextFilter(): void {
    const order: (Group | null)[] = [null, ...GROUPS];
    this.filter = order[(order.indexOf(this.filter) + 1) % order.length];
    this.cursor = { list: 0, day: 0 };
    this.render();
    this.shell.refresh();
  }

  private row(i: Item, pane: "list" | "day"): HTMLElement {
    const asAt = today(this.shell);
    const late = i.date !== null && i.date < asAt && open(i);
    let text: (Node | string | null)[] = [];
    let sub = "";
    let amount = "";
    switch (i.k) {
      case "note":
        text = [h("span.strong", { dir: "auto" }, i.note.title), i.note.text ? h("span.muted", { dir: "auto" }, ` · ${i.note.text}`) : null];
        sub = [linkText(i.note), i.note.done ? t("done") : ""].filter(Boolean).join(" · ");
        break;
      case "due":
        text = [h("span.strong", dueTitle(i.due)), h("span.muted", ` · ${period(i.due)}`)];
        sub = i.due.filed
          ? t("Marked filed by {user}", { user: ltr(i.due.filed_by ?? "") })
          : `${t(i.due.optional ? "Estimate · if it applies to you" : "Estimate")} · ${shiftText(i.due)}`;
        break;
      case "bill":
        text = [h("span.strong", i.bill.party), h("span.muted", ` · ${i.bill.reference}`)];
        sub = t(i.bill.receivable ? "Receivable" : "Payable");
        amount = M.fmt(i.bill.amount);
        break;
      case "pdc":
        text = [h("span.strong", i.pdc.particulars), h("span.muted", ` · ${t(KIND_LABEL[i.pdc.kind])} `, h("bdi", i.pdc.number))];
        sub = t("Dated after today");
        amount = M.fmt(i.pdc.amount);
        break;
      case "msme":
        text = [h("span.strong", i.msme.party), h("span.muted", ` · ${i.msme.reference}`)];
        sub = t(i.msme.msme_party ? "MSME supplier: pay within 45 days of {date}" : "Supplier bill of {date}: pay MSME suppliers within 45 days", { date: D.show(i.msme.date) });
        amount = M.fmt(i.msme.amount);
        break;
      case "zatca":
        text = [h("span.strong", i.overdue ? t("{n} simplified invoice(s) past the 24-hour reporting limit", { n: i.overdue }) : t("{n} document(s) waiting in the ZATCA queue", { n: i.waiting }))];
        sub = t("Simplified invoices are reported within 24 hours");
        break;
      case "setup":
        text = [h("span.strong", t("Set up your compliance profile"))];
        sub = t("Press Enter: incorporation date, business form, year end, registrations and tax periods decide which dates the desk shows.");
        break;
    }
    const el = h(
      "div.desk-row",
      { class: `k-${i.k}${late ? " late" : ""}${open(i) ? "" : " done"}` },
      h("span.desk-date", i.date ? D.show(i.date) : ""),
      h("span.desk-tag", t(i.k === "note" && !i.note.due ? "Note" : i.k === "due" ? CATEGORY_TAG[i.due.category] : TAG[i.k])),
      h("span.desk-text", ...text, sub ? h("span.desk-sub", sub) : null),
      h("span.desk-amt.num", amount),
    );
    const rows = pane === "list" ? this.listRows : this.dayRows;
    const index = rows.length;
    rows.push({ el, item: i });
    el.addEventListener("click", () => {
      this.pane = pane;
      this.cursor[pane] = index;
      this.paint();
    });
    el.addEventListener("dblclick", () => void this.openItem(i));
    return el;
  }

  private renderCalendar(): void {
    const asAt = today(this.shell);
    const [from] = this.gridRange();
    const start = weekStart(this.shell);
    const grid = h("div.desk-month", { role: "grid", "aria-label": monthTitle(this.month) });
    for (let k = 0; k < 7; k++) {
      // 4 Jan 1970 was a Sunday.
      const name = new Intl.DateTimeFormat(language() === "ar" ? "ar-AE" : "en-GB", { weekday: "short", timeZone: "UTC" }).format(new Date(Date.UTC(1970, 0, 4 + ((start + k) % 7))));
      grid.append(h("div.desk-wd", name));
    }
    this.dayCells.clear();
    const hijri = hijriOn();
    for (let k = 0; k < 42; k++) {
      const iso = D.addDays(from, k);
      const items = this.onDay(iso);
      const counts = new Map<string, number>();
      for (const i of items) if (open(i)) counts.set(i.k, (counts.get(i.k) ?? 0) + 1);
      const cell = h(
        "div.desk-cell",
        {
          role: "gridcell",
          class: [iso.slice(0, 7) === this.month.slice(0, 7) ? "" : "other", iso === asAt ? "today" : "", items.some((i) => open(i) && iso < asAt) ? "late" : ""].filter(Boolean).join(" "),
          title: D.show(iso),
          onclick: () => {
            this.sel = iso;
            this.pane = "cal";
            if (monthOf(iso) !== this.month) void this.goMonth(monthOf(iso));
            else {
              this.renderDay();
              this.paint();
            }
          },
        },
        h("span.desk-dn", String(Number(iso.slice(8)))),
        hijri ? h("span.desk-hd", hijriDay(iso) ?? "") : null,
        h("span.desk-dots", ...[...counts].map(([kind, n]) => h("span.desk-dot", { class: `k-${kind}`, title: t(TAG[kind as Item["k"]]) }, String(n)))),
      );
      this.dayCells.set(iso, cell);
      grid.append(cell);
    }
    const span = hijri ? hijriSpan(this.month) : null;
    this.calEl.replaceChildren(
      h("div.desk-cal-head", h("span.strong", monthTitle(this.month)), span ? h("span.muted", ` · ${span}`) : null, h("span.muted.desk-cal-keys", t("PgUp/PgDn: month · Home: today"))),
      grid,
    );
  }

  private renderDay(): void {
    this.dayRows = [];
    const items = this.onDay(this.sel);
    this.dayEl.replaceChildren(
      h("div.tside-title", t("On {date}", { date: D.show(this.sel) })),
      items.length ? h("div.desk-rows", ...items.map((i) => this.row(i, "day"))) : h("p.muted.desk-empty", t("Nothing on this day. Alt+C adds a to-do for it.")),
    );
    this.cursor.day = Math.min(this.cursor.day, Math.max(0, this.dayRows.length - 1));
  }

  private paint(): void {
    this.el.dataset.pane = this.pane;
    for (const [pane, rows] of [["list", this.listRows], ["day", this.dayRows]] as const) {
      rows.forEach((r, i) => r.el.classList.toggle("cur", this.pane === pane && i === this.cursor[pane]));
    }
    for (const [iso, cell] of this.dayCells) cell.classList.toggle("sel", iso === this.sel);
    const cur = this.curRow();
    if (cur) cur.el.scrollIntoView({ block: "nearest" });
    else if (this.pane === "cal") this.dayCells.get(this.sel)?.scrollIntoView({ block: "nearest" });
  }

  private curRow(): Row | null {
    if (this.pane === "list") return this.listRows[this.cursor.list] ?? null;
    if (this.pane === "day") return this.dayRows[this.cursor.day] ?? null;
    return null;
  }

  // ---------------------------------------------------------- moves

  private move(delta: number): void {
    if (this.pane === "cal") return;
    const rows = this.pane === "list" ? this.listRows : this.dayRows;
    if (!rows.length) return;
    this.cursor[this.pane] = Math.max(0, Math.min(rows.length - 1, this.cursor[this.pane] + delta));
    this.paint();
  }

  private async goMonth(first: string): Promise<void> {
    this.month = first;
    if (monthOf(this.sel) !== first) this.sel = first;
    try {
      await this.loadMonth();
    } catch (e) {
      this.shell.error(e);
    }
    this.renderCalendar();
    this.renderDay();
    this.paint();
  }

  private moveDay(days: number): void {
    const next = D.addDays(this.sel, days);
    this.sel = next;
    if (monthOf(next) !== this.month) void this.goMonth(monthOf(next));
    else {
      this.renderDay();
      this.paint();
    }
  }

  private shiftMonth(n: number): void {
    const day = Number(this.sel.slice(8));
    const first = addMonths(this.month, n);
    const last = Number(lastOfMonth(first).slice(8));
    this.sel = `${first.slice(0, 8)}${String(Math.min(day, last)).padStart(2, "0")}`;
    void this.goMonth(first);
  }

  // -------------------------------------------------------- actions

  private async openItem(i: Item): Promise<void> {
    const s = this.shell;
    const asAt = today(s);
    switch (i.k) {
      case "note": {
        const l = i.note.link;
        if (l && !l.missing && l.kind === "ledger") void s.push(new LedgerStatementReport(s, l.id, s.periodFromFor(asAt), asAt));
        else if (l && !l.missing && l.kind === "voucher" && l.voucher_kind && l.date) void s.push(savedVoucherScreen(s, l.voucher_kind, l.date, l.id));
        else await this.alter(i.note);
        return;
      }
      case "bill":
      case "msme": {
        const id = i.k === "bill" ? i.bill.ledger_id : i.msme.ledger_id;
        void s.push(new LedgerStatementReport(s, id, s.periodFromFor(asAt), asAt));
        return;
      }
      case "pdc":
        void s.push(savedVoucherScreen(s, i.pdc.kind, i.pdc.date, i.pdc.voucher_id));
        return;
      case "zatca":
        void import("./zatca").then((m) => s.push(new m.ZatcaQueueReport(s)));
        return;
      case "due":
        this.openReturn(i.due);
        return;
      case "setup":
        await this.configure();
        return;
    }
  }

  /** Enter on a return: the matching report for its period, else the rule and where it is published. */
  private openReturn(d: DueDto): void {
    const s = this.shell;
    const [from, to] = [d.period_from, d.period_to];
    const fyYear = Number(to.slice(0, 4)) - (Number(to.slice(5, 7)) < 4 ? 1 : 0);
    const quarter = Math.floor(((Number(to.slice(5, 7)) + 8) % 12) / 3) + 1;
    const go = (p: Promise<Screen>) => void p.then((x) => s.push(x)).catch((e) => s.error(e));
    switch (d.code) {
      case "IN.GSTR1":
      case "IN.GSTR1Q":
      case "IN.IFF":
        return go(import("./gst").then((m) => new m.Gstr1Report(s, from, to)));
      case "IN.GSTR3B":
      case "IN.GSTR3BQ":
      case "IN.PMT06":
        return go(import("./gst").then((m) => new m.Gstr3bReport(s, from, to)));
      case "IN.TDSPAY":
        return go(import("./tds").then((m) => new m.TdsPayableReport(s, from, to)));
      case "IN.TCSPAY":
        return go(import("./tds").then((m) => new m.TcsPayableReport(s, from, to)));
      case "IN.TDSQ":
        return go(import("./tds").then((m) => new m.TdsQuarterlyReport(s, fyYear, quarter)));
      case "IN.TCSQ":
        return go(import("./tds").then((m) => new m.TcsQuarterlyReport(s, fyYear, quarter)));
      case "AE.VAT201":
        return go(import("./vat").then((m) => new m.Vat201Report(s, from, to)));
      case "SA.VAT":
        return go(import("./vat_sa").then((m) => new m.KsaVatReturnReport(s, from, to)));
      case "GB.VAT":
        return go(import("./vat_gb").then((m) => new m.UkVatReturnReport(s, from, to)));
      case "SG.F5":
        return go(import("./gst_sg").then((m) => new m.GstF5Report(s, from, to)));
    }
    s.info(t("{title}: {rule}. Source: {source}", { title: dueTitle(d), rule: d.rule, source: ltr(d.source) }), 9000);
  }

  private async alter(n: NoteDto): Promise<void> {
    if (!this.desk?.can_write) return this.readOnly();
    if (await editNote(this.shell, { note: n })) await this.reload();
  }

  private readOnly(): void {
    this.shell.warn(t("You can read the desk; adding or changing notes needs Data Entry or above."));
  }

  private async add(): Promise<void> {
    if (!this.desk?.can_write) return this.readOnly();
    const due = this.pane === "list" ? undefined : this.sel;
    if (await editNote(this.shell, { due })) await this.reload();
  }

  /** Space: tick a to-do done (or open it again), mark a return filed (or not). */
  private async toggle(): Promise<void> {
    const i = this.curRow()?.item;
    if (!i || (i.k !== "note" && i.k !== "due")) return;
    if (!this.desk?.can_write) return this.readOnly();
    try {
      if (i.k === "note") {
        await deskApi.setDone(i.note.id, !i.note.done);
        this.shell.info(t(i.note.done ? "To-do opened again." : "To-do done."));
      } else {
        await deskApi.markFiled(i.due.filed_code, i.due.period_to, !i.due.filed);
        this.shell.info(t(i.due.filed ? "Filed mark removed." : "Marked filed. Keep the acknowledgement from the portal."));
      }
      await this.reload();
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async remove(): Promise<void> {
    const i = this.curRow()?.item;
    if (i?.k !== "note") return;
    if (!this.desk?.can_write) return this.readOnly();
    if (!(await this.shell.ask(t('Delete the note "{title}"?', { title: i.note.title }), { yesFirst: false }))) return;
    try {
      await deskApi.deleteNote(i.note.id);
      this.shell.info(t("Note deleted."));
      await this.reload();
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** F12: the compliance profile, asking only what the country's rules use. */
  private async configure(): Promise<void> {
    const s = this.desk?.settings;
    if (!s) return;
    const d = s.defaults;
    if (!d) {
      this.shell.warn(t("This country has no compliance calendar yet."));
      return;
    }
    const country = this.shell.company?.country?.code;
    const c = s.company;
    const ref = today(this.shell);
    const fields: Field[] = [];
    const freq = c.frequency ?? d.frequency;
    const qe = c.quarter_end ?? d.quarter_end;
    for (const f of d.fields) {
      switch (f) {
        case "incorporated":
          fields.push({
            label: "Incorporated on (blank: not known)",
            value: c.incorporated ? D.editable(c.incorporated) : "",
            read: (v, out) => {
              if (!v.trim()) return (out.incorporated = null), null;
              const x = D.parseUser(v, ref);
              if (!x) return t('"{text}" is not a date.', { text: v });
              out.incorporated = x;
              return null;
            },
          });
          break;
        case "entity": {
          const names = d.entities.map((e) => `${ENTITY_LETTER[e]} ${country === "US" && e === "company" ? t("C corporation") : t(ENTITY_LABEL[e])}`).join(", ");
          fields.push({
            label: `${t("Business form")}: ${names}`,
            value: ENTITY_LETTER[c.entity ?? d.entities[0]],
            read: (v, out) => {
              const e = d.entities.find((x) => ENTITY_LETTER[x] === v.trim().toUpperCase().slice(0, 1));
              if (!e) return t("Type one of the letters shown.");
              out.entity = e;
              return null;
            },
          });
          break;
        }
        case "fy_end":
          fields.push({
            label: "Financial year ends in month (1–12)",
            value: String(s.fy_end_month),
            read: (v, out) => (MONTH.test(v.trim()) ? ((out.fy_end_month = Number(v)), null) : t("Type a month number from 1 to 12.")),
          });
          break;
        case "vat_from":
          fields.push({
            label: "VAT/GST registered from (date; blank: from the start; N: not registered)",
            value: c.vat_unregistered ? "N" : c.vat_from ? D.editable(c.vat_from) : "",
            read: (v, out) => {
              const x = v.trim();
              out.vat_registered = /^[nN]/.test(x) ? false : null;
              out.vat_from = null;
              if (!x || out.vat_registered === false) return null;
              const date = D.parseUser(x, ref);
              if (!date) return t('"{text}" is not a date.', { text: v });
              out.vat_from = date;
              return null;
            },
          });
          break;
        case "tax_period":
          fields.push(this.periodField(d.period, freq, qe, country));
          break;
        case "corporate_tax":
          fields.push({
            label: "Registered for corporate tax (Y/N)",
            value: (c.corporate_tax ?? (c.entity ?? d.entities[0]) === "company") ? "Y" : "N",
            read: (v, out) => yn(v) ?? ((out.corporate_tax = yes(v)), null),
          });
          break;
        case "audit":
          fields.push({ label: "Tax audit applies (Y/N)", value: c.audit ? "Y" : "N", read: (v, out) => yn(v) ?? ((out.audit = yes(v)), null) });
          break;
        case "withholding":
          fields.push(
            { label: "Deducts TDS (Y/N)", value: c.withholding ? "Y" : "N", read: (v, out) => yn(v) ?? ((out.withholding = yes(v)), null) },
            { label: "Collects TCS (Y/N)", value: c.collection ? "Y" : "N", read: (v, out) => yn(v) ?? ((out.collection = yes(v)), null) },
          );
          break;
        case "states":
          fields.push({
            label: "Sales tax states, e.g. CA:Q, TX:M (M monthly, Q quarterly, A annual)",
            value: c.states.map((x) => `${x.state}:${FREQ_LETTER[x.frequency]}`).join(", "),
            read: (v, out) => {
              const states = [];
              for (const part of v.split(/[,;\s]+/).filter(Boolean)) {
                const m = /^([A-Za-z]{2}):([MQAmqa])$/.exec(part);
                if (!m) return t('"{text}" should look like CA:Q.', { text: part });
                const f = m[2].toUpperCase();
                states.push({ state: m[1].toUpperCase(), frequency: (f === "M" ? "monthly" : f === "Q" ? "quarterly" : "annual") as Frequency });
              }
              out.states = states;
              return null;
            },
          });
          break;
      }
    }
    const read = (vals: string[]): [CalendarSettings, string | null] => {
      const out: CalendarSettings = { ...s.saved, states: s.saved.states ?? [] };
      for (const [k, f] of fields.entries()) {
        const problem = f.read(vals[k], out);
        if (problem) return [out, problem];
      }
      return [out, null];
    };
    const v = await this.shell.dialog("Compliance profile", fields.map((f) => ({ label: f.label, value: f.value })), (vals) => read(vals)[1]);
    if (!v) return;
    try {
      await deskApi.setCalendarSettings(read(v)[0]);
      this.shell.info(t("Compliance profile saved."));
      await this.reload();
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** The VAT/GST period field, as the country assigns periods. */
  private periodField(choice: NonNullable<Desk["settings"]["defaults"]>["period"], freq: Frequency, qe: number, country: string | undefined): Field {
    const mq = (out: CalendarSettings, v: string, allowed: string): string | null => {
      const k = v.trim().toUpperCase().slice(0, 1);
      if (!v.trim() || !allowed.includes(k)) return allowed.length === 3 ? t("Type M, Q or A.") : t("Type M or Q.");
      out.frequency = k === "M" ? "monthly" : k === "Q" ? "quarterly" : "annual";
      return null;
    };
    switch (choice) {
      case "monthly_quarterly":
        return {
          label: country === "IN" ? "GST returns: M monthly, Q quarterly (QRMP)" : country === "SA" ? "VAT returns: M monthly (supplies over SAR 40 million), Q quarterly" : "Returns: M monthly, Q quarterly",
          value: FREQ_LETTER[freq === "annual" ? "quarterly" : freq],
          read: (v, out) => mq(out, v, "MQ"),
        };
      case "monthly_quarterly_annual":
        return { label: "Returns: M monthly, Q quarterly, A annual", value: FREQ_LETTER[freq], read: (v, out) => mq(out, v, "MQA") };
      case "uk_stagger":
        return {
          label: "VAT stagger: 1, 2 or 3 (quarters ending March, April or May), or M monthly",
          value: freq === "monthly" ? "M" : String(((qe - 3 + 12) % 3) + 1),
          read: (v, out) => {
            const x = v.trim().toUpperCase();
            if (x.startsWith("M")) return (out.frequency = "monthly"), null;
            if (!/^[123]$/.test(x)) return t("Type 1, 2, 3 or M.");
            out.frequency = "quarterly";
            out.quarter_end = Number(x) + 2;
            return null;
          },
        };
      case "quarter_start":
      case "quarter_end": {
        const start = choice === "quarter_start";
        return {
          label: start ? "VAT periods: M monthly, or the month a quarter starts (1–12)" : "GST periods: M monthly, or the month a quarter ends (1–12)",
          value: freq === "monthly" ? "M" : String(start ? ((qe - 3 + 12) % 12) + 1 : qe),
          read: (v, out) => {
            const x = v.trim().toUpperCase();
            if (x.startsWith("M")) return (out.frequency = "monthly"), null;
            if (!MONTH.test(x)) return t("Type M or a month number from 1 to 12.");
            out.frequency = "quarterly";
            out.quarter_end = start ? ((Number(x) + 1) % 12) + 1 : Number(x);
            return null;
          },
        };
      }
    }
  }

  // ------------------------------------------------------- Screen

  contexts(): Context[] {
    return ["Report"];
  }

  handles(a: string): boolean {
    return ["Back", "AddNote", "Configure", "Filter"].includes(a);
  }

  bottom() {
    const ctx = this.contexts();
    const i = this.curRow()?.item;
    const write = Boolean(this.desk?.can_write);
    return [
      { chord: "Enter", label: "Open", enabled: Boolean(i) },
      { chord: NEW_KEY, label: "New To-do", enabled: write },
      { chord: "Space", label: i?.k === "due" ? "Filed" : "Done", enabled: write && (i?.k === "note" || i?.k === "due") },
      { chord: "Ctrl+Enter", label: "Alter", enabled: write && i?.k === "note" },
      { chord: DELETE_KEY, label: "Delete", enabled: write && i?.k === "note" },
      { chord: "Tab", label: this.pane === "list" ? "Calendar" : "Lists" },
      this.shell.btn("Filter", ctx, this.filter ? GROUP_LABEL[this.filter] : "Filter"),
      this.shell.btn("Configure", ctx, "Profile"),
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    (document.activeElement as HTMLElement | null)?.blur?.();
    this.paint();
  }

  key(e: KeyEv): boolean {
    const rtl = dir() === "rtl";
    switch (e.chord) {
      case "Tab":
      case "Shift+Tab":
        this.pane = this.pane === "list" ? "cal" : "list";
        break;
      case NEW_KEY:
        void this.add();
        return true;
      case DELETE_KEY:
        void this.remove();
        return true;
      case "Space":
        void this.toggle();
        return true;
      case "Ctrl+Enter": {
        const i = this.curRow()?.item;
        if (i?.k === "note") void this.alter(i.note);
        return true;
      }
      case "Enter":
        if (this.pane === "cal") {
          if (this.onDay(this.sel).length) {
            this.pane = "day";
            this.cursor.day = 0;
          }
          break;
        }
        if (this.curRow()) void this.openItem(this.curRow()!.item);
        return true;
      case "Esc":
        if (this.pane !== "day") return false;
        this.pane = "cal";
        break;
      case "Up":
        if (this.pane === "cal") this.moveDay(-7);
        else this.move(-1);
        return true;
      case "Down":
        if (this.pane === "cal") this.moveDay(7);
        else this.move(1);
        return true;
      case "Left":
      case "Right":
        if (this.pane !== "cal") return true;
        this.moveDay((e.chord === "Right") !== rtl ? 1 : -1);
        return true;
      case "PageUp":
      case "PageDown":
        if (this.pane === "list") this.move(e.chord === "PageUp" ? -10 : 10);
        else this.shiftMonth(e.chord === "PageUp" ? -1 : 1);
        return true;
      case "Home":
        if (this.pane === "list") this.move(-this.listRows.length);
        else {
          this.sel = today(this.shell);
          void this.goMonth(monthOf(this.sel));
        }
        return true;
      case "End":
        if (this.pane !== "cal") this.move(this.listRows.length + this.dayRows.length);
        return true;
      default:
        return false;
    }
    this.paint();
    this.shell.refresh();
    return true;
  }

  action(a: string): boolean {
    if (a === "AddNote") {
      void this.add();
      return true;
    }
    if (a === "Configure") {
      void this.configure();
      return true;
    }
    if (a === "Filter") {
      this.nextFilter();
      return true;
    }
    return false;
  }
}

// ------------------------------------------------------------- home

/** The home screen's "due soon" line; null when the desk can't be read. */
export async function dueSoon(shell: Shell): Promise<{ overdue: number; soon: number } | null> {
  return deskApi.dueCount(today(shell)).catch(() => null);
}

// Banking (N on the home menu): bank reconciliation and the cheque register.
// Reconciliation lists a bank ledger's entries with the date the bank cleared
// each one; typing that date in the Bank Date column moves the entry from
// "not reflected in bank" into the bank's balance. A bank statement (CSV)
// can be imported instead: its lines are paired with uncleared entries,
// reviewed, and the ticked ones get the statement's date as their bank date.
import "../banking.css";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, type BankLine, type BankStatementRow, type Context, type VoucherKind } from "../types";
import { RecordList } from "./alter";
import { MenuScreen } from "./launcher";
import { Report } from "./reports";
import { savedVoucherScreen } from "./stockjournal";
import { VoucherScreen } from "./voucher";

function fyStart(shell: Shell): string {
  return shell.periodFrom();
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : t("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function amountCell(text: string): HTMLElement {
  return h("td.num", M.fmt(text, true));
}

function openVoucher(shell: Shell, l: { kind: VoucherKind; date: string; voucher: string }): void {
  void shell.push(savedVoucherScreen(shell, l.kind, l.date, l.voucher));
}

/** Import statement, in Bank Reconciliation (not a Tally key; free in reports). */
const IMPORT_KEY = "Alt+S";

/** Is the line still uncleared at `asAt`? */
function uncleared(l: BankLine, asAt: string): boolean {
  return !l.bank_date || l.bank_date > asAt;
}

// ------------------------------------------------------ reconciliation

/**
 * One bank ledger: its entries for the period plus older ones the bank has
 * not cleared yet. The cursor sits in the Bank Date column; Left moves it to
 * Particulars, where Enter opens the voucher (Ctrl+Enter does so anywhere).
 */
export class BankReconciliationReport extends Report {
  title = "Bank Reconciliation";
  protected override period = "range" as const;
  private lines: BankLine[] = [];
  private col: "date" | "particulars" = "date";
  private editor = h("input.field.bankdate", { spellcheck: false, autocomplete: "off", placeholder: "dd-mm-yyyy", "aria-label": t("Bank date") });
  private saving = false;
  private ledgerName = "";

  constructor(shell: Shell, private ledgerId: string, private from = fyStart(shell), private to = shell.periodTo()) {
    super(shell);
  }

  async load(): Promise<void> {
    const r = await api.bankReconciliation(this.ledgerId, this.from, this.to);
    this.lines = r.lines;
    this.ledgerName = r.ledger_name;
    this.title = t("Bank Reconciliation: {bank}", { bank: r.ledger_name });
    this.heading(this.title, range(this.from, this.to));
    this.resetPicks();
    const tbody = h("tbody");
    for (const l of r.lines) {
      tbody.append(
        this.pick(
          h(
            "tr",
            { class: l.date < this.from ? "carried" : "" },
            h("td", D.show(l.date)),
            h("td.strong.part", l.particulars),
            h("td", t(KIND_LABEL[l.kind])),
            h("td", l.number),
            amountCell(l.deposit),
            amountCell(l.withdrawal),
            h("td.bcell", h("span.bdate", l.bank_date ? D.show(l.bank_date) : "")),
          ),
          () => openVoucher(this.shell, l),
        ),
      );
    }
    if (!r.lines.length) tbody.append(h("tr.empty", h("td", { colSpan: 7 }, t("No entries in this period. Press F2 to change it."))));
    const pending = r.lines.filter((l) => uncleared(l, this.to)).length;
    this.bodyEl.replaceChildren(
      h(
        "table.rtable.brs",
        h(
          "thead",
          h("tr", h("th", t("Date")), h("th", t("Particulars")), h("th", t("Vch Type")), h("th", t("Vch No.")), h("th.num", t("Deposit")), h("th.num", t("Withdrawal")), h("th", t("Bank Date"))),
        ),
        tbody,
        h(
          "tfoot",
          h("tr", h("td", { colSpan: 4 }, t("Balance as per company books")), h("td.num", { colSpan: 2 }, M.drCr(M.of(r.balance_per_books))), h("td")),
          h("tr", h("td", { colSpan: 4 }, t("Amounts not reflected in bank")), amountCell(r.uncleared_deposits), amountCell(r.uncleared_withdrawals), h("td")),
          h("tr", h("td", { colSpan: 4 }, t("Balance as per bank")), h("td.num", { colSpan: 2 }, M.drCr(M.of(r.balance_per_bank))), h("td")),
        ),
      ),
      h(
        "div.note",
        t(
          "{pending} of {total} entries not cleared by {date}. Type the date the bank cleared an entry; Enter saves it and moves down, and an empty date clears it. Greyed dates are older entries still uncleared. {key} imports the bank's statement (CSV) and matches it for you.",
          { pending, total: r.lines.length, date: D.show(this.to), key: IMPORT_KEY },
        ),
      ),
    );
  }

  override handles(a: string): boolean {
    return a === "AlterMasterInline" || super.handles(a);
  }

  override bottom() {
    const ctx = this.contexts();
    return [
      this.shell.btn("AlterMasterInline", ctx, "Open Voucher", this.lines.length > 0),
      { chord: IMPORT_KEY, label: "Import Statement" },
      this.shell.btn("ChangeDate", ctx, "Period"),
      this.shell.btn("PrintCurrent", ctx, "Print"),
      this.shell.btn("ExportCurrent", ctx, "Export CSV"),
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  /** The cursor row carries the editor in its Bank Date cell. */
  protected override paint(): void {
    super.paint();
    for (const c of this.bodyEl.querySelectorAll(".bcell.editing")) c.classList.remove("editing");
    for (const r of this.bodyEl.querySelectorAll("tr.on-part")) r.classList.remove("on-part");
    const row = this.picks[this.cursor]?.el;
    const line = this.lines[this.cursor];
    if (!row || !line || this.col === "particulars") {
      this.editor.remove();
      row?.classList.add("on-part");
      return;
    }
    const cell = row.querySelector(".bcell")!;
    if (this.editor.parentElement !== cell) {
      cell.append(this.editor);
      this.editor.value = line.bank_date ? D.editable(line.bank_date) : "";
    }
    cell.classList.add("editing");
    this.editor.focus();
    this.editor.select();
  }

  private setCol(col: "date" | "particulars"): void {
    this.col = col;
    this.paint();
  }

  private saved(): string {
    const d = this.lines[this.cursor]?.bank_date;
    return d ? D.editable(d) : "";
  }

  private editing(): boolean {
    return this.col === "date" && this.editor.isConnected && this.editor.value.trim() !== this.saved();
  }

  /** Save the typed bank date (if changed), then move the cursor by `delta`. */
  private async commit(delta: number): Promise<void> {
    if (this.saving) return;
    const n = this.lines.length;
    const line = this.lines[this.cursor];
    const target = this.lines[Math.max(0, Math.min(n - 1, this.cursor + delta))];
    if (!line || !this.editing()) {
      this.cursor = target ? this.lines.indexOf(target) : 0;
      this.paint();
      return;
    }
    const text = this.editor.value.trim();
    let date = "";
    if (text) {
      const d = D.parseUser(text, line.date);
      if (!d) return this.shell.warn(t('"{text}" is not a date.', { text }));
      if (d < line.date) return this.shell.warn(t("The bank date can't be before the voucher date ({date}).", { date: D.show(line.date) }));
      date = d;
    }
    if (date === (line.bank_date ?? "")) {
      this.editor.value = this.saved(); // the same date, typed another way
      return this.commit(delta);
    }
    this.saving = true;
    try {
      await api.setBankDate(line.voucher, this.ledgerId, date);
      await this.load();
      // An older line cleared within the period drops out; aim by voucher.
      const at = this.lines.findIndex((l) => l.voucher === target.voucher);
      this.cursor = at >= 0 ? at : Math.min(this.cursor, this.lines.length - 1);
      this.paint();
      this.shell.refresh();
    } catch (e) {
      this.shell.error(e);
    } finally {
      this.saving = false;
    }
  }

  private openCurrent(): void {
    const l = this.lines[this.cursor];
    if (l) openVoucher(this.shell, l);
  }

  /** Pick a statement file, match it, and open the review. */
  private async importStatement(): Promise<void> {
    let path: string | string[] | null;
    try {
      path = await open({
        multiple: false,
        title: t("Choose the bank statement for {bank} (CSV)", { bank: this.ledgerName }),
        filters: [{ name: t("Bank statement"), extensions: ["csv", "CSV", "txt"] }],
      });
    } catch (e) {
      return this.shell.error(e);
    }
    const file = Array.isArray(path) ? path[0] : path;
    if (!file) return;
    try {
      const csv = await this.shell.transition(api.readTextFile(file));
      const rows = await this.shell.transition(api.matchStatement(this.ledgerId, csv));
      await this.shell.push(new StatementReview(this.shell, this.ledgerId, this.ledgerName, file, csv, rows));
    } catch (e) {
      this.shell.error(e);
    }
  }

  override key(e: KeyEv): boolean {
    if (e.chord === IMPORT_KEY) {
      void this.importStatement();
      return true;
    }
    if (!this.lines.length) return super.key(e);
    switch (e.chord) {
      case "Enter":
        if (this.col === "particulars") return false; // DrillDown opens the voucher
        void this.commit(1);
        return true;
      case "Alt+Enter":
        this.openCurrent();
        return true;
      case "Up":
        void this.commit(-1);
        return true;
      case "Down":
        void this.commit(1);
        return true;
      case "Home":
      case "Ctrl+Home":
        void this.commit(-this.lines.length);
        return true;
      case "End":
      case "Ctrl+End":
        void this.commit(this.lines.length);
        return true;
      case "Left":
        if (this.col === "date" && this.editor.selectionStart !== 0) return false; // within the text
        this.setCol("particulars");
        return true;
      case "Right":
        if (this.col === "date") return false;
        this.setCol("date");
        return true;
      case "Esc":
        if (!this.editing()) return false;
        this.editor.value = this.saved(); // F4: undo the typing first
        this.editor.select();
        return true;
    }
    return super.key(e);
  }

  override action(a: string): boolean {
    switch (a) {
      case "AlterMasterInline":
        this.openCurrent();
        return true;
      case "DrillDown":
        if (this.col === "particulars") this.openCurrent();
        else void this.commit(1);
        return true;
      case "PageDown":
      case "NextRecord":
        void this.commit(10);
        return true;
      case "PreviousRecord":
        void this.commit(-10);
        return true;
    }
    return super.action(a);
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }
}

// ------------------------------------------------------ statement import

function net(r: BankStatementRow): bigint {
  return M.of(r.deposit) - M.of(r.withdrawal);
}

/**
 * A bank statement's lines beside the uncleared entries they match. Matches
 * start ticked when the cheque/ref number agrees or no other line has the
 * same amount; Space ticks and unticks, Ctrl+A sets the ticked bank dates
 * (all or nothing). Enter on a line with no match opens a new Receipt or
 * Payment on its date; coming back matches the statement again.
 */
export class StatementReview extends Report {
  title: string;
  private ticked: boolean[] = [];
  private busy = false;

  constructor(
    shell: Shell,
    private ledgerId: string,
    private ledgerName: string,
    private file: string,
    private csv: string,
    private rows: BankStatementRow[],
  ) {
    super(shell);
    this.title = t("Statement: {bank}", { bank: ledgerName });
    this.ticked = rows.map((r) => this.startTicked(r));
  }

  private startTicked(r: BankStatementRow): boolean {
    if (!r.voucher_id) return false;
    if (r.reference_matched) return true;
    const n = net(r);
    return this.rows.filter((x) => net(x) === n).length === 1;
  }

  /** Back from a new voucher: match again, keeping ticks on unchanged matches. */
  async rematch(): Promise<void> {
    const before = new Map(this.rows.map((r, i) => [r.voucher_id ?? `-${i}`, this.ticked[i]]));
    this.rows = await api.matchStatement(this.ledgerId, this.csv);
    this.ticked = this.rows.map((r) => (r.voucher_id && before.has(r.voucher_id) ? before.get(r.voucher_id)! : this.startTicked(r)));
  }

  override resume(): void {
    void this.rematch()
      .then(() => this.reload())
      .catch((e) => this.shell.error(e));
  }

  async load(): Promise<void> {
    const name = this.file.split(/[\\/]/).pop() ?? this.file;
    this.heading(t("Bank statement: {bank}", { bank: this.ledgerName }), t("{file} · {n} lines", { file: ltr(name), n: this.rows.length }));
    this.resetPicks();
    const tbody = h("tbody");
    this.rows.forEach((r, i) => {
      const matched = r.voucher_id && r.voucher_kind
        ? h(
            "td.match",
            `${t(KIND_LABEL[r.voucher_kind])} ${ltr(r.voucher_number ?? "")} · ${r.voucher_date ? D.show(r.voucher_date) : ""}`,
            r.particulars ? h("span.muted", ` · ${r.particulars}`) : null,
            r.reference_matched ? h("span.byref", ` ${t("(ref)")}`) : null,
          )
        : h("td.match.none", t("No match"));
      const tick = h("td.tick", { onclick: () => this.toggle(i) }, this.ticked[i] ? "✓" : "");
      tbody.append(
        this.pick(
          h(
            "tr",
            { class: r.voucher_id ? "" : "unmatched" },
            h("td", D.show(r.date)),
            h("td", r.description),
            h("td", r.reference),
            amountCell(r.deposit),
            amountCell(r.withdrawal),
            matched,
            tick,
          ),
          () => this.openLine(i),
        ),
      );
    });
    const sumOf = (pick: (r: BankStatementRow, i: number) => boolean, f: "deposit" | "withdrawal") =>
      M.format(M.sum(this.rows.filter(pick).map((r) => M.of(r[f]))));
    const isMatched = (r: BankStatementRow) => Boolean(r.voucher_id);
    const isTicked = (r: BankStatementRow, i: number) => Boolean(r.voucher_id) && this.ticked[i];
    const nMatched = this.rows.filter(isMatched).length;
    const nTicked = this.rows.filter(isTicked).length;
    const nOpen = this.rows.length - nMatched;
    this.bodyEl.replaceChildren(
      h(
        "table.rtable.stmt",
        h(
          "thead",
          h("tr", h("th", t("Date")), h("th", t("Description")), h("th", t("Ref")), h("th.num", t("Deposit")), h("th.num", t("Withdrawal")), h("th", t("Matched voucher")), h("th.tick", "✓")),
        ),
        tbody,
        h(
          "tfoot",
          h("tr", h("td", { colSpan: 3 }, t("Matched ({n})", { n: nMatched })), h("td.num", sumOf(isMatched, "deposit")), h("td.num", sumOf(isMatched, "withdrawal")), h("td", { colSpan: 2 })),
          h("tr", h("td", { colSpan: 3 }, t("No match ({n})", { n: nOpen })), h("td.num", sumOf((r) => !isMatched(r), "deposit")), h("td.num", sumOf((r) => !isMatched(r), "withdrawal")), h("td", { colSpan: 2 })),
          h("tr", h("td", { colSpan: 3 }, t("Ticked to apply ({n})", { n: nTicked })), h("td.num", sumOf(isTicked, "deposit")), h("td.num", sumOf(isTicked, "withdrawal")), h("td", { colSpan: 2 })),
        ),
      ),
      h(
        "div.note",
        t("{matched} matched, {open} with no match. Matches with the same amount as another line start unticked: check them.", { matched: nMatched, open: nOpen }),
        " ",
        t("Space ticks or unticks a line; Ctrl+A sets the statement date as the bank date of every ticked entry."),
        " ",
        t("Enter on a line with no match opens a new Receipt or Payment on its date; the statement is matched again when you come back."),
      ),
    );
  }

  override contexts(): Context[] {
    return ["Report"];
  }

  override handles(a: string): boolean {
    return a === "Accept" || a === "SelectLine" || a === "SelectAll" || super.handles(a);
  }

  override bottom() {
    const ctx = this.contexts();
    const r = this.rows[this.cursor];
    return [
      { chord: "Space", label: "Tick", enabled: Boolean(r?.voucher_id) },
      this.shell.btn("Accept", ctx, "Apply"),
      { chord: "Enter", label: r && !r.voucher_id ? "New Voucher" : "Open Voucher", enabled: Boolean(r) },
      this.shell.btn("PrintCurrent", ctx, "Print"),
      this.shell.btn("ExportCurrent", ctx, "Export CSV"),
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  protected override paint(): void {
    super.paint();
    this.shell.refresh();
  }

  private toggle(i: number): void {
    const r = this.rows[i];
    if (!r) return;
    if (!r.voucher_id) return this.shell.warn(t("This line has no matching voucher. Press Enter to enter one."));
    this.ticked[i] = !this.ticked[i];
    this.cursor = i;
    void this.reload();
  }

  private openLine(i: number): void {
    const r = this.rows[i];
    if (!r) return;
    if (r.voucher_id && r.voucher_kind && r.voucher_date) {
      openVoucher(this.shell, { kind: r.voucher_kind, date: r.voucher_date, voucher: r.voucher_id });
      return;
    }
    const kind: VoucherKind = M.of(r.deposit) > 0n ? "Receipt" : "Payment";
    this.shell.info(t('Enter the {kind} for "{line}" ({amount}).', { kind: t(KIND_LABEL[kind]), line: r.description || r.reference, amount: M.format(M.abs(net(r))) }));
    void this.shell.push(new VoucherScreen(this.shell, kind, r.date));
  }

  override key(e: KeyEv): boolean {
    if (e.chord === "Space") {
      this.toggle(this.cursor);
      return true;
    }
    return super.key(e);
  }

  override action(a: string): boolean {
    switch (a) {
      case "SelectLine":
        this.toggle(this.cursor);
        return true;
      case "SelectAll": {
        const all = this.rows.every((r, i) => !r.voucher_id || this.ticked[i]);
        this.ticked = this.rows.map((r) => Boolean(r.voucher_id) && !all);
        void this.reload();
        return true;
      }
      case "Accept":
        void this.apply();
        return true;
    }
    return super.action(a);
  }

  private async apply(): Promise<void> {
    if (this.busy) return;
    const dates = this.rows
      .map((r, i) => (r.voucher_id && this.ticked[i] ? { voucher_id: r.voucher_id, date: r.date } : null))
      .filter((d): d is { voucher_id: string; date: string } => d !== null);
    if (!dates.length) return this.shell.warn(t("Nothing is ticked. Space ticks a matched line."));
    const n = dates.length;
    if (!(await this.shell.ask(n === 1 ? t("Set the bank date for 1 entry?") : t("Set bank dates for {n} entries?", { n })))) return;
    this.busy = true;
    try {
      const done = await this.shell.transition(api.applyBankDates(this.ledgerId, dates));
      this.shell.info(done === 1 ? t("1 bank date set from the statement.") : t("{n} bank dates set from the statement.", { n: done }));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    } finally {
      this.busy = false;
    }
  }
}

// ------------------------------------------------------ cheque register

interface Uncleared extends BankLine {
  bank: string;
}

/** Withdrawals (cheques issued, transfers out) not yet cleared, across banks. */
export class ChequeRegister extends Report {
  title = "Cheque Register";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = fyStart(shell), private to = shell.periodTo()) {
    super(shell);
  }

  async load(): Promise<void> {
    const banks = (await api.listLedgers("", "cash_bank")).filter((l) => !l.is_cash);
    const recs = await Promise.all(banks.map((l) => api.bankReconciliation(l.id, this.from, this.to)));
    const rows: Uncleared[] = recs
      .flatMap((r) => r.lines.filter((l) => M.of(l.withdrawal) > 0n && uncleared(l, this.to)).map((l) => ({ ...l, bank: r.ledger_name })))
      .sort((a, b) => a.date.localeCompare(b.date) || a.bank.localeCompare(b.bank));
    this.heading(t("Cheque Register"), t("Not cleared by {date} · {period}", { date: D.show(this.to), period: range(this.from, this.to) }));
    this.resetPicks();
    const tbody = h("tbody");
    for (const r of rows) {
      tbody.append(
        this.pick(
          h(
            "tr",
            { class: r.date < this.from ? "carried" : "" },
            h("td", D.show(r.date)),
            h("td", r.bank),
            h("td.strong", r.particulars),
            h("td", t(KIND_LABEL[r.kind])),
            h("td", r.number),
            amountCell(r.withdrawal),
          ),
          () => openVoucher(this.shell, r),
        ),
      );
    }
    if (!rows.length) tbody.append(h("tr.empty", h("td", { colSpan: 6 }, t("Every withdrawal has cleared the bank."))));
    this.bodyEl.replaceChildren(
      h(
        "table.rtable",
        h("thead", h("tr", h("th", t("Date")), h("th", t("Bank")), h("th", t("Particulars")), h("th", t("Vch Type")), h("th", t("Vch No.")), h("th.num", t("Amount")))),
        tbody,
        h("tfoot", h("tr", h("td", { colSpan: 5 }, t("Total")), h("td.num", M.format(M.sum(rows.map((r) => M.of(r.withdrawal))))))),
      ),
      h("div.note", t("Set bank dates in Bank Reconciliation; cleared entries leave this list.")),
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

// ------------------------------------------------------ menu

function bankList(shell: Shell): Screen {
  return new RecordList(shell, "Select Bank", ["Name", "Under"], async (q) =>
    (await api.listLedgers(q, "cash_bank")).map((l) => ({
      id: l.id,
      label: l.name,
      sub: l.group_name,
      data: () => new BankReconciliationReport(shell, l.id),
    })),
  );
}

export function bankingMenu(shell: Shell): Screen {
  return new MenuScreen(shell, "Banking", [
    { id: "brs", label: "Bank Reconciliation", section: "Reconcile", data: () => bankList(shell) },
    { id: "cheques", label: "Cheque Register", sub: "uncleared withdrawals", section: "Reports", data: () => new ChequeRegister(shell) },
  ]);
}

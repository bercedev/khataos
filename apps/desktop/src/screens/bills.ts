// Bill-wise details: a sub-screen stacked over the voucher (rule L7) for a
// party line. Rows of Type, Name, Due date and Amount must add up to the
// line amount before Enter leaves; Backspace in the first field returns to
// the voucher (rule V5).
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import type { BillKind, Context, Ledger, OutstandingBill, Side } from "../types";

export interface BillRow {
  kind: BillKind;
  reference: string;
  /** ISO date or "". */
  due: string;
  /** Decimal string, positive. */
  amount: string;
}

export const BILL_LABEL: Record<BillKind, string> = {
  New: "New Ref",
  Against: "Agst Ref",
  Advance: "Advance",
  OnAccount: "On Account",
};
const KINDS = Object.keys(BILL_LABEL) as BillKind[];

type Col = "kind" | "reference" | "due" | "amount";

/** A foreign amount (positive) at a rate of exchange, rounded half to even to two decimals, as the core rounds. */
function atRate(fx: M.Units, rate: M.Units): M.Units {
  const cent = M.of("0.01");
  const d = M.of("1") * cent;
  let q = (fx * rate) / d;
  const r = (fx * rate) % d;
  if (r * 2n > d || (r * 2n === d && q % 2n === 1n)) q += 1n;
  return q * cent;
}

export interface BillOptions {
  party: Ledger;
  side: Side;
  lineAmount: M.Units;
  /** Voucher date, the base for due dates. */
  date: string;
  /** Suggested name for a new bill (the voucher number). */
  suggested: string;
  existing: BillRow[];
  /** A line in a foreign currency: its rate of exchange, so a foreign bill is settled in full at today's rate. */
  fxRate?: M.Units | null;
  onDone: (rows: BillRow[]) => void;
  onCancel: () => void;
}

/** Fields a row skips because their value is decided (rule F2). */
function cols(kind: BillKind): Col[] {
  if (kind === "OnAccount") return ["kind", "amount"];
  if (kind === "Against") return ["kind", "reference", "amount"];
  return ["kind", "reference", "due", "amount"];
}

export class BillScreen implements Screen {
  title = "Bill-wise Details";
  overlay = true;
  el = h("section.bills");
  private rows: BillRow[];
  private open: OutstandingBill[] = [];
  private r = 0;
  private c: Col = "kind";
  private inputs = new Map<string, HTMLInputElement>();
  private grid = h("div.blines");
  private status = h("div.bstatus");
  private picker: ListPicker<BillKind | OutstandingBill> | null = null;

  constructor(private shell: Shell, private o: BillOptions) {
    this.rows = o.existing.length ? o.existing.map((r) => ({ ...r })) : [];
    this.el.append(
      h(
        "div.card.bcard",
        h("div.card-title", t("Bill-wise details for {party}", { party: o.party.name })),
        h("div.bsub", `${ltr(M.format(o.lineAmount))} ${t(o.side)}`),
        h("div.bgrid.bhead", h("span", t("Type")), h("span", t("Name")), h("span", t("Due date")), h("span.num", t("Amount"))),
        this.grid,
        this.status,
      ),
    );
  }

  async init(): Promise<void> {
    // Bills this line can settle: the party's open bills on the other side.
    const all = await api.outstanding(this.o.date, "all", this.o.party.id);
    this.open = all.filter((b) => (this.o.side === "Cr" ? M.of(b.amount) > 0n : M.of(b.amount) < 0n) && b.reference !== "On Account");
    if (!this.rows.length) this.rows.push(this.blank());
    this.render();
  }

  /** New Ref when the line raises the party's balance, else Agst Ref / On Account. */
  private defaultKind(): BillKind {
    const raises = (this.o.party.nature === "Assets") === (this.o.side === "Dr");
    if (raises) return "New";
    return this.open.length ? "Against" : "OnAccount";
  }

  private remaining(except = -1): M.Units {
    return this.o.lineAmount - M.sum(this.rows.filter((_, i) => i !== except).map((r) => M.parse(r.amount) ?? M.ZERO));
  }

  /** New bills fall due after the party's credit period, when it has one. */
  private defaultDue(kind: BillKind): string {
    const days = this.o.party.credit_days;
    return kind === "New" && days ? D.addDays(this.o.date, days) : "";
  }

  private blank(): BillRow {
    const kind = this.defaultKind();
    const first = this.rows.length === 0;
    return {
      kind,
      reference: kind === "New" && first ? this.o.suggested : "",
      due: this.defaultDue(kind),
      amount: M.toDecimal(this.remaining() > 0n ? this.remaining() : M.ZERO),
    };
  }

  contexts(): Context[] {
    if (this.c === "kind" || (this.c === "reference" && this.rows[this.r]?.kind === "Against")) return ["ListField", "Voucher"];
    if (this.c === "amount") return ["AmountField", "Voucher"];
    return ["Voucher"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    const ctx = this.contexts();
    return [this.shell.btn("Accept", ctx, "Done"), { chord: "Backspace", label: "Back to voucher" }, this.shell.btn("Back", ctx, "Cancel")].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }

  private slot(row: number, col: Col): string {
    return `${row}:${col}`;
  }

  private render(): void {
    this.picker?.close();
    this.picker = null;
    this.inputs.clear();
    const lines = this.rows.map((row, i) => {
      const mk = (col: Col, value: string, cls = "") => {
        const inp = h("input.field", { class: cls, value, spellcheck: false, autocomplete: "off" });
        inp.addEventListener("focus", () => this.onFocus(i, col));
        this.inputs.set(this.slot(i, col), inp);
        return inp;
      };
      const kind = mk("kind", t(BILL_LABEL[row.kind]));
      const ref = row.kind === "OnAccount" ? h("span.amount-blank") : mk("reference", row.reference);
      const due = row.kind === "New" || row.kind === "Advance" ? mk("due", row.due ? D.editable(row.due) : "") : h("span.amount-blank");
      const amount = mk("amount", row.amount ? M.format(M.of(row.amount)) : "", "amount");
      amount.addEventListener("input", () => {
        row.amount = amount.value;
        this.paintStatus();
      });
      return h("div.bgrid.bline", kind, ref, due, amount);
    });
    this.grid.replaceChildren(...lines);
    this.paintStatus();
  }

  private paintStatus(): void {
    const left = this.remaining();
    this.status.replaceChildren(
      h("span", t("Allocated {done} of {total}", { done: ltr(M.format(this.o.lineAmount - left)), total: ltr(M.format(this.o.lineAmount)) })),
      left === 0n
        ? h("span.ok", t("Matches the line"))
        : h("span.warn", t(left > 0n ? "Remaining {amount}" : "Over by {amount}", { amount: ltr(M.format(M.abs(left))) })),
    );
  }

  focus(): void {
    this.go(this.r, this.c);
  }

  private go(r: number, c: Col): void {
    this.r = r;
    this.c = c;
    const inp = this.inputs.get(this.slot(r, c));
    if (!inp) return;
    if (document.activeElement !== inp) inp.focus();
    else this.onFocus(r, c);
    inp.select();
  }

  private onFocus(r: number, c: Col): void {
    this.picker?.close();
    this.picker = null;
    this.r = r;
    this.c = c;
    const row = this.rows[r];
    const inp = this.inputs.get(this.slot(r, c))!;
    if (c === "kind") {
      this.picker = new ListPicker<BillKind>({
        input: inp,
        host: this.el,
        title: "Type of reference",
        source: (q) =>
          // Typed text matches the shown name or the English one.
          KINDS.filter((k) => [BILL_LABEL[k], t(BILL_LABEL[k])].some((l) => l.toLowerCase().includes(q.toLowerCase()))).map((k) => ({
            id: k,
            label: t(BILL_LABEL[k]),
            data: k,
          })),
        selectedId: () => row.kind,
        onPick: (it) => this.setKind(r, it.data as BillKind),
      }) as ListPicker<BillKind | OutstandingBill>;
      void this.picker.open();
    } else if (c === "reference" && row.kind === "Against") {
      this.picker = new ListPicker<OutstandingBill>({
        input: inp,
        host: this.el,
        title: t("Open bills of {party}", { party: this.o.party.name }),
        columns: ["Bill", "Pending"],
        source: (q) =>
          this.open
            .filter((b) => b.reference.toLowerCase().includes(q.toLowerCase()))
            .map<PickItem<OutstandingBill>>((b) => ({
              id: b.reference,
              label: b.reference,
              sub: `${b.date ? D.show(b.date) : t("Opening")} · ${M.drCr(M.of(b.amount))}${b.forex_amount ? ltr(` · ${b.forex_currency} ${M.format(M.abs(M.of(b.forex_amount)))}`) : ""}`,
              data: b,
            })),
        selectedId: () => row.reference || null,
        onPick: (it) => this.setAgainst(r, it.data as OutstandingBill),
        noMatch: (text) => t('{party} has no open bill "{text}". Use New Ref or On Account.', { party: this.o.party.name, text }),
      }) as ListPicker<BillKind | OutstandingBill>;
      void this.picker.open();
    } else if (c === "amount") {
      const u = M.parse(inp.value);
      if (u !== null && u !== 0n) inp.value = M.toDecimal(u);
    }
    this.shell.refresh();
  }

  private next(): void {
    const order = cols(this.rows[this.r].kind);
    this.go(this.r, order[order.indexOf(this.c) + 1]);
  }

  private setKind(r: number, kind: BillKind): void {
    const row = this.rows[r];
    if (row.kind !== kind) {
      row.kind = kind;
      row.reference = kind === "New" && r === 0 ? this.o.suggested : "";
      row.due = this.defaultDue(kind);
      this.render();
    }
    this.go(r, cols(kind)[1]);
  }

  private setAgainst(r: number, bill: OutstandingBill): void {
    const row = this.rows[r];
    row.reference = bill.reference;
    // A foreign bill is pending in its currency: at this voucher's rate, that is what settles it.
    const rate = this.o.fxRate;
    const pending = bill.forex_amount && rate ? atRate(M.abs(M.of(bill.forex_amount)), rate) : M.abs(M.of(bill.amount));
    const left = this.remaining(r);
    row.amount = M.toDecimal(left < pending ? left : pending);
    this.render();
    this.go(r, "amount");
  }

  key(e: KeyEv): boolean {
    if (this.picker?.key(e)) return true;
    const inp = this.inputs.get(this.slot(this.r, this.c));
    if (e.chord === "Backspace" && this.r === 0 && this.c === "kind") {
      this.close(false); // V5
      return true;
    }
    if (e.chord === "Backspace" && inp && inp.value === "" && this.c !== "kind") {
      this.previous();
      return true;
    }
    if (e.chord === "Up") {
      this.previous();
      return true;
    }
    return false;
  }

  private previous(): void {
    const order = cols(this.rows[this.r].kind);
    const i = order.indexOf(this.c);
    if (i > 0) this.go(this.r, order[i - 1]);
    else if (this.r > 0) this.go(this.r - 1, "amount");
  }

  action(a: string): boolean {
    if (this.picker?.action(a)) return true;
    switch (a) {
      case "NextField":
        this.enter();
        return true;
      case "PreviousField":
        this.previous();
        return true;
      case "Accept":
        this.finish();
        return true;
      case "Back":
        void this.escape();
        return true;
    }
    return false;
  }

  private enter(): void {
    const row = this.rows[this.r];
    const inp = this.inputs.get(this.slot(this.r, this.c))!;
    if (this.c === "reference") {
      row.reference = inp.value.trim();
      if (!row.reference) return this.shell.warn("Give the bill a name, usually the invoice number.");
      return this.next();
    }
    if (this.c === "due") {
      if (inp.value.trim()) {
        const d = D.parseUser(inp.value, this.o.date);
        if (!d) return this.shell.warn(t('"{text}" is not a date. Leave it blank if there is no due date.', { text: inp.value }));
        row.due = d;
        inp.value = D.editable(d);
      } else row.due = "";
      return this.next();
    }
    if (this.c === "amount") {
      const u = M.parse(inp.value);
      if (u === null || u <= 0n) return this.shell.warn("Enter an amount greater than zero.");
      row.amount = M.toDecimal(u);
      const left = this.remaining();
      if (left < 0n) return this.shell.error(t("Allocated more than the line: over by {amount}.", { amount: ltr(M.format(-left)) }));
      if (left === 0n) return this.finish();
      if (this.r === this.rows.length - 1) {
        this.rows.push(this.blank());
        this.render();
      }
      return this.go(this.r + 1, "kind");
    }
  }

  private finish(): void {
    const left = this.remaining();
    if (left !== 0n) {
      this.shell.warn(
        t(left > 0n ? "Bill-wise details must add up to {total}: remaining {amount}." : "Bill-wise details must add up to {total}: over by {amount}.", {
          total: ltr(M.format(this.o.lineAmount)),
          amount: ltr(M.format(M.abs(left))),
        }),
      );
      return;
    }
    const bad = this.rows.findIndex((r) => r.kind !== "OnAccount" && !r.reference.trim());
    if (bad >= 0) {
      this.shell.warn("Every bill needs a name.");
      this.go(bad, "reference");
      return;
    }
    this.close(true);
  }

  private async escape(): Promise<void> {
    const inp = this.inputs.get(this.slot(this.r, this.c));
    if (inp && this.c !== "kind" && inp.value !== "") {
      inp.value = ""; // F4
      if (this.c === "amount") this.rows[this.r].amount = "";
      this.paintStatus();
      return;
    }
    if (await this.shell.ask("Leave bill-wise details without changes?")) this.close(false);
  }

  private close(done: boolean): void {
    const rows = this.rows.filter((r) => (M.parse(r.amount) ?? 0n) > 0n);
    this.shell.pop();
    if (done) this.o.onDone(rows);
    else this.o.onCancel();
  }

  dispose(): void {
    this.picker?.close();
  }
}

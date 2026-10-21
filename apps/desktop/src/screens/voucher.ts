// Voucher entry (rules V1–V3, V6, V8, L4, L5; field flow F1–F7; P6–P7).
//
// Three entry modes:
// - "double" (as voucher): Dr/Cr lines, each a ledger and an amount; all types.
// - "single" (Payment, Receipt, Contra): an Account header plus particulars.
// - "item" (item invoice; Sales, Purchase, Credit Note, Debit Note): party,
//   sales or purchase ledger, item lines (qty × rate = amount), then other
//   ledgers such as tax or discount. Ctrl+H switches with "double".
//
// Item invoices fill their tax lines from the company's tax setup once the
// item lines are done ("auto" lines); a line the user changes becomes their own.
// A trading voucher as voucher (double entry) does the same for its sales or
// purchase lines whose ledger has an HSN/SAC (services), once such a line has
// its amount; the party line keeps the amount typed.
//
// India: a Journal, Purchase or Payment whose expense ledger (or party) has a
// TDS nature of payment gets its TDS suggested (tds_for): an auto credit line
// to the TDS payable ledger, with the party's credit cut by the same amount so
// the voucher stays balanced. It follows the GST auto lines' rules.
//
// TCS (India) mirrors it on Sales and Receipt: when the sales or income
// ledger (or the buyer) has a nature of collection, tcs_for suggests TCS on
// what the buyer pays, GST included. The auto line credits the TCS payable
// ledger and the party's debit grows by the same amount, since TCS is
// collected on top of the invoice.
//
// Foreign currency (docs/multi-currency.md): a line whose ledger is kept in
// a foreign currency takes its amount in that currency; a "Rate of
// exchange" field (the last rate used, by default) turns it into the base
// amount live. Other lines stay in the base currency. On saving, every line
// of a Sales, Purchase or note carries its foreign equivalent too (so the
// invoice prints in the party's currency); a gain or loss on foreign bills
// settled at a new rate is posted to Forex Gain/Loss by the backend.
import { api, tcsApi, tdsApi } from "../api";
import { fxApi } from "../forex-api";
import { h } from "../dom";
import * as D from "../dates";
import { hijriOn, language, ltr, t } from "../i18n";
import { letterOf } from "../keys";
import * as M from "../money";
import { ListPicker, type PickAction, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import {
  KIND_LABEL,
  KINDS,
  type Context,
  type EntryInput,
  type ForexInput,
  type Godown,
  type Group,
  type InvoiceTax,
  type InvoiceTaxInput,
  type InvoiceTaxRow,
  type Ledger,
  type LedgerFilter,
  type Side,
  type StockItem,
  type StockLineInput,
  type StockLineView,
  type TcsSuggestion,
  type TdsNature,
  type TdsSuggestion,
  type VoucherKind,
} from "../types";
import { LedgerForm } from "./ledger";
import { BillScreen, type BillRow } from "./bills";
import { StockJournalScreen } from "./stockjournal";
import { editNote, notesBlock, notesFor } from "./desk";
import "../inventory.css";
import "../voucher-extra.css";

type Mode = "double" | "single" | "item";
/** One stock item on an item invoice (or carried on a double-entry line). */
interface ItemLine {
  item: StockItem | null;
  qty: string;
  rate: string;
  amount: string;
  /** Godown the stock moves in or out of; "" is the main location. */
  godownId: string;
  godownName: string;
  batch: string;
}
/** A tax line filled from the tax calculation, until the user changes it. */
interface AutoTax {
  /** Ledger, component and percent: what a recalculation matches on. */
  key: string;
  component: string;
  percent: string;
  /** TDS (India), not tax on the invoice: kept apart from the GST lines. */
  tds?: boolean;
  /** TCS (India), collected on top of the invoice: kept apart from the GST lines too. */
  tcs?: boolean;
}

/** An auto line filled by TDS or TCS rather than the invoice's tax. */
function withheld(a: AutoTax | undefined): boolean {
  return Boolean(a?.tds || a?.tcs);
}
interface Line {
  side: Side;
  ledger: Ledger | null;
  amount: string;
  /** Bill-wise allocations for a party line. */
  bills?: BillRow[];
  /** Stock items on a sales or purchase line (kept in double entry). */
  items?: ItemLine[];
  auto?: AutoTax;
  /** On a ledger kept in a foreign currency: the amount typed in it (`amount` is then the base amount). */
  fx?: string;
}
type Field =
  | { k: "account" }
  | { k: "party" }
  | { k: "sales" }
  | { k: "item"; i: number }
  | { k: "godown"; i: number }
  | { k: "batch"; i: number }
  | { k: "qty"; i: number }
  | { k: "rate"; i: number }
  | { k: "value"; i: number }
  | { k: "side"; i: number }
  | { k: "ledger"; i: number }
  | { k: "amount"; i: number }
  | { k: "fx" }
  | { k: "narration" };
type LedgerField = Field & { k: "ledger" | "account" | "party" | "sales" };
/** One entry the voucher would post; `fx` is its foreign amount (typed, or derived for the party). */
type VEntry = { ledger: Ledger; side: Side; amount: M.Units; bills?: BillRow[]; items?: ItemLine[]; fx?: M.Units };

const SINGLE_OK: VoucherKind[] = ["Payment", "Receipt", "Contra"];
const PARTY_FIRST: VoucherKind[] = ["Sales", "Purchase", "CreditNote", "DebitNote"];
/** Voucher types that can carry stock items. */
const TRADING = PARTY_FIRST;
const ITEM_FIELDS: Field["k"][] = ["item", "godown", "batch", "qty", "rate", "value"];
/** Item invoices that can be printed as a tax invoice or note right after saving. */
const PRINTABLE: VoucherKind[] = ["Sales", "CreditNote", "DebitNote"];

/** Voucher types that can carry TDS (India): an expense booked or paid. */
const TDS_KINDS: VoucherKind[] = ["Journal", "Purchase", "Payment"];
/** Wait after a change to the expense, party, amount or date before asking for TDS. */
const TDS_DELAY_MS = 300;

let naturesOnce: Promise<TdsNature[]> | null = null;
/** TDS natures of payment, asked once (for the section on the auto line's tag). */
function tdsNatures(): Promise<TdsNature[]> {
  naturesOnce ??= tdsApi.natures().catch(() => {
    naturesOnce = null;
    return [];
  });
  return naturesOnce;
}

/** Voucher types that can carry TCS (India): goods sold or a sum received. */
const TCS_KINDS: VoucherKind[] = ["Sales", "Receipt"];
/** Wait after a change to the sales ledger, party, amount or date before asking for TCS. */
const TCS_DELAY_MS = 300;

/** A TCS collectee: a party ledger, one with a PAN, or one with the buyer's terms but no nature. */
function isCollectee(l: Ledger): boolean {
  return l.is_party || Boolean(l.pan) || (l.tcs?.kind === "Collectible" && !l.tcs.nature);
}

/** A ledger's TCS nature of collection, when it has one. */
function tcsNatureOf(l: Ledger | null | undefined): string | null {
  return l?.tcs?.kind === "Collectible" ? l.tcs.nature ?? null : null;
}

/** A TDS deductee: a party ledger, or one with a PAN or a deductee type. */
function isDeductee(l: Ledger): boolean {
  return l.is_party || Boolean(l.pan) || (l.tds?.kind === "Deductible" && Boolean(l.tds.deductee));
}

/** A ledger's TDS nature of payment, when it has one. */
function natureOf(l: Ledger | null | undefined): string | null {
  return l?.tds?.kind === "Deductible" ? l.tds.nature ?? null : null;
}

/** A bank account (not cash), so a payment from it can be a cheque. */
function isBank(l: Ledger | null | undefined): boolean {
  return Boolean(l && l.is_cash_or_bank && !l.is_cash);
}

const MAIN_LOCATION = "Main Location";
/** Wait after a change to items, party or date before recalculating tax. */
const TAX_DELAY_MS = 250;
const FIRST_SIDE: Record<VoucherKind, Side> = {
  Contra: "Dr",
  Payment: "Dr",
  Receipt: "Cr",
  Journal: "Dr",
  Sales: "Dr",
  Purchase: "Cr",
  CreditNote: "Cr",
  DebitNote: "Dr",
  StockJournal: "Dr",
};

/** Side of the particulars lines in single-entry mode; the account takes the other. */
function particularSide(kind: VoucherKind): Side {
  return kind === "Receipt" ? "Cr" : "Dr";
}

function other(s: Side): Side {
  return s === "Dr" ? "Cr" : "Dr";
}

/** A voucher type's name in the display language: "Payment", "سند صرف". */
function kindName(kind: VoucherKind): string {
  return t(KIND_LABEL[kind]);
}

/** "sales" for Sales and Credit Note (returns), else "purchase". */
function tradeNoun(kind: VoucherKind): "sales" | "purchase" {
  return kind === "Sales" || kind === "CreditNote" ? "sales" : "purchase";
}

/** "Pick the sales ledger …" / "Pick the purchase ledger …", whole sentences for translation. */
function pickTradeLedger(kind: VoucherKind): string {
  return tradeNoun(kind) === "sales" ? t("Pick the sales ledger the items are booked to.") : t("Pick the purchase ledger the items are booked to.");
}

const ONE = M.of("1");
const CENT = ONE / 100n;

/** n ÷ d rounded half to even, the way the core rounds (d > 0). */
function divEven(n: bigint, d: bigint): bigint {
  const neg = n < 0n;
  const a = neg ? -n : n;
  let q = a / d;
  const r = a % d;
  if (r * 2n > d || (r * 2n === d && q % 2n === 1n)) q += 1n;
  return neg ? -q : q;
}

/** qty × rate rounded to two decimals, exactly as the core computes it. */
export function lineAmount(qty: M.Units, rate: M.Units): M.Units {
  return divEven(qty * rate, ONE * CENT) * CENT;
}

/** amount ÷ qty rounded to two decimals (a rate from a typed amount). */
export function rateOf(amount: M.Units, qty: M.Units): M.Units {
  return qty === 0n ? M.ZERO : divEven(amount * ONE, qty * CENT) * CENT;
}

/** Decimal places typed in a quantity ("2.50" → 1). */
export function placesOf(text: string): number {
  const frac = text.trim().split(".")[1] ?? "";
  return frac.replace(/0+$/, "").length;
}

/** Quantity for display: "12.5". */
export function qtyText(u: M.Units): string {
  return M.toDecimal(u);
}

/** A tax percent for display: "9.000" → "9". */
function percentText(p: string): string {
  const u = M.parse(p);
  return u === null ? p : M.toDecimal(u);
}

function blankItem(): ItemLine {
  return { item: null, qty: "", rate: "", amount: "", godownId: "", godownName: "", batch: "" };
}

function itemDone(l: ItemLine): boolean {
  return l.item !== null && (M.parse(l.qty) ?? M.ZERO) > 0n;
}

function itemAmount(l: ItemLine): M.Units {
  const u = M.parse(l.amount);
  return u === null ? M.ZERO : M.abs(u);
}

export class VoucherScreen implements Screen {
  el: HTMLElement;
  private mode: Mode;
  private number = "";
  private account: Ledger | null = null;
  private lines: Line[] = [];
  /** Item invoice: the party, the sales or purchase ledger and the item lines. */
  private party: Ledger | null = null;
  private partyBills: BillRow[] | undefined;
  private salesLedger: Ledger | null = null;
  private items: ItemLine[] = [blankItem()];
  /** The company has stock items: trading vouchers open as item invoices. */
  private hasItems = false;
  /** The company's godowns; the Godown column shows when there is one. */
  private godowns: Godown[] = [];
  /** Batch column turned on (Ctrl+I); it also shows while any line has a batch. */
  private batchOn = false;
  /** Last rates asked for, per item, voucher type and party. */
  private rates = new Map<string, Promise<string>>();

  // Automatic tax on an item invoice.
  /** On once the user reaches the other ledgers (or the voucher already has some). */
  private taxOn = false;
  private taxTimer: number | undefined;
  private taxRun: Promise<void> | null = null;
  private taxSeq = 0;
  private lastTaxKey = "";
  private lastTax: InvoiceTax | null = null;
  /** Tax that no duty ledger collects yet: shown, but can't be posted. */
  private taxMissing: InvoiceTaxRow[] = [];
  /** Auto lines removed with Ctrl+D; they stay away until the item (or taxed sales) lines change. */
  private dismissed = new Set<string>();
  private dismissedFor = "";
  /** Next calculation takes over existing lines that match it (loaded or converted vouchers). */
  private adopt = false;
  private taxWarned = "";
  private taxHinted = false;
  private taxLedgers = new Map<string, Ledger>();

  // TDS (India) on Journal, Purchase and Payment.
  private tdsTimer: number | undefined;
  private tdsRun: Promise<void> | null = null;
  private tdsSeq = 0;
  /** The last request and its answer, so an unchanged base isn't asked again. */
  private tdsKey = "";
  private tdsLast: TdsSuggestion | null = null;
  /** Auto TDS removed with Ctrl+D: it stays away while the request is this one. */
  private tdsDismissed: string | null = null;
  /** The explanation and warnings last shown, so each is shown once. */
  private tdsShown = "";
  /** Next suggestion takes over a matching TDS line of the user's own (a loaded or converted voucher). */
  private tdsAdopt = false;

  // TCS (India) on Sales and Receipt; the same rules as TDS.
  private tcsTimer: number | undefined;
  private tcsRun: Promise<void> | null = null;
  private tcsSeq = 0;
  private tcsKey = "";
  private tcsLast: TcsSuggestion | null = null;
  private tcsDismissed: string | null = null;
  private tcsShown = "";
  private tcsAdopt = false;

  private at = 0;
  private picker: ListPicker<Ledger> | ListPicker<StockItem> | ListPicker<Godown> | null = null;
  private inputs = new Map<string, HTMLInputElement>();
  private curBal = new Map<number, HTMLElement>();
  private itemBal = new Map<number, HTMLElement>();
  private accountBal = h("span.curbal");
  /** Under the rate of exchange: the settlement note, when it applies. */
  private fxNote = h("span.curbal");
  private partyBal = h("span.curbal");
  private salesBal = h("span.curbal");
  private totalsEl = h("div.vtotals");
  private narration: HTMLInputElement;
  private body = h("div.vbody");
  private numberEl = h("span.vnum");
  private dateEl = h("span.vdate");
  private dayEl = h("span.vday");
  private badge = h("span.badge");

  /** Set when altering a saved voucher (opened from a report). */
  private alterId: string | null = null;
  /** A cancelled voucher opens read-only. */
  private cancelled = false;
  /** The saved voucher is a Payment from a bank: Ctrl+P can print a cheque. */
  private chequeable = false;
  /** Signed effect of the saved version on each ledger, removed from projections. */
  private savedEffect = new Map<string, M.Units>();
  /** Quantity the saved version moved per stock item (in positive). */
  private savedStock = new Map<string, M.Units>();
  /** Altering: the voucher as loaded, so Esc without changes just leaves (F6). */
  private loaded: string | null = null;
  /** The company's own currency ("INR"); "" until known. */
  private fxBase = "";
  /** Rate of exchange for this voucher's foreign currency, as typed (base per unit). */
  private fxRate = "";
  /** The currency `fxRate` was looked up or typed for. */
  private fxRateFor = "";
  /** A saved voucher's notes from the desk (Alt+W adds one). */
  private notesEl = h("div.vnotes");

  constructor(
    private shell: Shell,
    private kind: VoucherKind,
    private date: string = shell.company?.today ?? D.todayIso(),
    alterId: string | null = null,
  ) {
    this.alterId = alterId;
    this.mode = SINGLE_OK.includes(kind) ? "single" : "double";
    this.narration = h("input.field.narration", { spellcheck: true, autocomplete: "off", placeholder: t("Narration") });
    this.narration.addEventListener("focus", () => this.onFocus(this.fields().length - 1));
    this.el = h(
      "section.voucher",
      h(
        "div.vmain",
        h("div.vhead", h("div.vhead-left", this.badge, this.numberEl), h("div.vhead-right", this.dateEl, this.dayEl)),
        this.body,
        h("div.vfoot", h("label.vnarr", h("span.flabel", t("Narration")), this.narration), this.totalsEl),
        this.notesEl,
      ),
    );
    this.reset();
  }

  get title(): string {
    const kind = kindName(this.kind);
    return t(this.cancelled ? "{kind} Voucher (Cancelled)" : this.alterId ? "{kind} Voucher (Alteration)" : "{kind} Voucher", { kind });
  }

  get tint(): string {
    return `tint-${this.kind}`;
  }

  async init(): Promise<void> {
    this.fxBase = (await fxApi.currencies().catch(() => null))?.base?.code ?? "";
    if (this.alterId) {
      await this.loadSaved(this.alterId);
      if (this.taxMode() && !this.cancelled && this.lines.length && (this.mode === "item" || this.taxLines().length)) {
        // Saved tax lines that still match the calculation follow later changes;
        // nothing is added to a saved voucher just by opening it.
        this.taxOn = true;
        this.adopt = true;
        await this.refreshTax(false, true);
      }
      if (this.tdsMode() && !this.cancelled) {
        // A saved TDS line that still matches follows later changes, as tax lines do.
        this.tdsAdopt = true;
        await this.refreshTds(false, true);
      }
      if (this.tcsMode()) {
        this.tcsAdopt = true;
        await this.refreshTcs(false, true);
      }
      this.loaded = this.state();
      void this.loadNotes();
    } else {
      const [number, stock, godowns] = await Promise.all([
        api.nextVoucherNumber(this.kind, this.date),
        api.listStockItems(""),
        this.loadGodowns(),
      ]);
      this.number = number;
      this.godowns = godowns;
      this.hasItems = stock.length > 0;
      if (this.hasItems && TRADING.includes(this.kind)) {
        this.mode = "item";
        this.reset();
      }
    }
    this.render();
  }

  /** The desk's notes on this saved voucher, under the narration. */
  private async loadNotes(): Promise<void> {
    if (!this.alterId) return;
    const block = notesBlock(await notesFor("voucher", this.alterId));
    this.notesEl.replaceChildren(...(block ? [block] : []));
  }

  /** Alt+W on a saved voucher: a note or to-do about it, kept on the desk. */
  private async addNote(id: string): Promise<void> {
    const label = `${kindName(this.kind)} ${this.number}`;
    if (await editNote(this.shell, { link: { kind: "voucher", id, label } })) await this.loadNotes();
  }

  /** Godowns, for the Godown column; none (or an older backend) hides it. */
  private loadGodowns(): Promise<Godown[]> {
    return api.listGodowns("").catch(() => []);
  }

  /** Load a saved voucher for alteration, in single entry or as an item invoice when it fits. */
  private async loadSaved(id: string): Promise<void> {
    const [v, ledgers, stock, godowns] = await Promise.all([
      api.getVoucher(id),
      api.listLedgers("", "all"),
      api.listStockItems(""),
      this.loadGodowns(),
    ]);
    const byId = new Map(ledgers.map((l) => [l.id, l]));
    const byItem = new Map(stock.map((s) => [s.id, s]));
    this.hasItems = stock.length > 0;
    this.godowns = godowns;
    this.kind = v.kind;
    this.number = v.number;
    this.date = v.date;
    this.narration.value = v.narration;
    this.cancelled = v.cancelled;
    this.chequeable = v.kind === "Payment" && v.entries.some((e) => e.side === "Cr" && isBank(byId.get(e.ledger_id)));
    this.mode = "double";
    // The realised forex gain/loss line is worked out again on saving.
    const settled = v.entries.some((e) => e.forex_gain);
    const own = v.entries.find((e) => e.forex && byId.get(e.ledger_id)?.currency === e.forex.currency) ?? v.entries.find((e) => e.forex);
    if (own?.forex) {
      this.fxRate = own.forex.rate;
      this.fxRateFor = own.forex.currency;
    }
    this.lines = v.entries.filter((e) => !e.forex_gain).map((e) => ({
      fx: e.forex && byId.get(e.ledger_id)?.currency === e.forex.currency ? e.forex.amount : undefined,
      side: e.side,
      ledger: byId.get(e.ledger_id) ?? null,
      amount: e.amount,
      bills: e.bills.length ? e.bills.map((b) => ({ kind: b.kind, reference: b.reference, due: b.due ?? "", amount: b.amount })) : undefined,
      items: e.items.length ? e.items.map((s) => this.itemLineOf(s, byItem.get(s.item_id))) : undefined,
    }));
    // A settlement shows its foreign lines (and their bills) at the voucher's rate, as typed.
    if (settled) {
      this.syncFx();
      v.entries
        .filter((e) => !e.forex_gain)
        .forEach((e, i) => {
          const l = this.lines[i];
          const fxBills = e.forex?.bills ?? [];
          if (!l?.bills || l.fx === undefined || fxBills.length !== l.bills.length) return;
          let left = M.of(l.amount);
          l.bills.forEach((b, j) => {
            const base = j === l.bills!.length - 1 ? left : M.of(this.toBase(fxBills[j]) || "0");
            b.amount = M.toDecimal(base);
            left -= base;
          });
        });
    }
    for (const e of v.entries) {
      const signed = e.side === "Dr" ? M.of(e.amount) : -M.of(e.amount);
      this.savedEffect.set(e.ledger_id, (this.savedEffect.get(e.ledger_id) ?? M.ZERO) + signed);
      for (const s of e.items) {
        const q = e.side === "Dr" ? M.of(s.qty) : -M.of(s.qty);
        this.savedStock.set(s.item_id, (this.savedStock.get(s.item_id) ?? M.ZERO) + q);
      }
    }
    if (SINGLE_OK.includes(this.kind)) this.toggleMode(true);
    else if (TRADING.includes(this.kind) && this.lines.some((l) => l.items?.length)) this.toggleItemMode(true);
    this.at = 0;
  }

  /** A saved stock line, with its item master (a stand-in if it has gone). */
  private itemLineOf(s: StockLineView, item: StockItem | undefined): ItemLine {
    const fallback: StockItem = {
      id: s.item_id,
      tax_class: null,
      costing: "AverageCost",
      name: s.item_name,
      group_id: null,
      group_name: "",
      unit_id: "",
      unit: s.unit,
      decimals: 4,
      opening_qty: "0",
      opening_value: "0",
      opening_rate: "0",
      closing_qty: "0",
      closing_value: "0",
    };
    return {
      item: item ?? fallback,
      qty: s.qty,
      rate: s.rate,
      amount: s.amount,
      godownId: s.godown_id ?? "",
      godownName: s.godown_name ?? "",
      batch: s.batch ?? "",
    };
  }

  // ------------------------------------------------------------ state

  private reset(): void {
    this.lines = this.mode === "item" ? [] : [this.blankLine(0)];
    this.fxRate = "";
    this.fxRateFor = "";
    this.party = null;
    this.partyBills = undefined;
    this.salesLedger = null;
    this.items = [blankItem()];
    this.narration.value = "";
    this.at = 0;
    this.stopTax();
    this.dismissed.clear();
    this.dismissedFor = "";
    this.lastTaxKey = "";
    this.lastTax = null;
    this.stopTds();
    this.stopTcs();
  }

  /** Forget any TCS suggestion; a pending or in-flight one no longer lands. */
  private stopTcs(): void {
    clearTimeout(this.tcsTimer);
    this.tcsTimer = undefined;
    this.tcsSeq++;
    this.tcsKey = "";
    this.tcsLast = null;
    this.tcsDismissed = null;
    this.tcsShown = "";
    this.tcsAdopt = false;
  }

  /** Forget any TDS suggestion; a pending or in-flight one no longer lands. */
  private stopTds(): void {
    clearTimeout(this.tdsTimer);
    this.tdsTimer = undefined;
    this.tdsSeq++;
    this.tdsKey = "";
    this.tdsLast = null;
    this.tdsDismissed = null;
    this.tdsShown = "";
    this.tdsAdopt = false;
  }

  /** Item invoice tax off: no pending or in-flight calculation lands any more. */
  private stopTax(): void {
    clearTimeout(this.taxTimer);
    this.taxTimer = undefined;
    this.taxSeq++;
    this.taxOn = false;
    this.adopt = false;
    this.taxMissing = [];
  }

  /** The Godown column: when the company has godowns, or a line already names one. */
  private showGodown(): boolean {
    return this.godowns.length > 0 || this.items.some((l) => l.godownId);
  }

  /** The Batch column: turned on with Ctrl+I, or while any line has a batch. */
  private showBatch(): boolean {
    return this.batchOn || this.items.some((l) => l.batch.trim());
  }

  /** Item invoice: the party's side; the sales or purchase ledger takes the other. */
  private partySide(): Side {
    return FIRST_SIDE[this.kind];
  }

  private salesSide(): Side {
    return other(FIRST_SIDE[this.kind]);
  }

  private blankLine(i: number): Line {
    if (this.mode === "single") return { side: particularSide(this.kind), ledger: null, amount: "" };
    if (this.mode === "item") return { side: this.salesSide(), ledger: null, amount: "" };
    if (i === 0) return { side: FIRST_SIDE[this.kind], ledger: null, amount: "" };
    const [dr, cr] = this.totals();
    const side: Side = dr > cr ? "Cr" : cr > dr ? "Dr" : other(this.lines[i - 1]?.side ?? FIRST_SIDE[this.kind]);
    return { side, ledger: null, amount: "" };
  }

  private fields(): Field[] {
    const out: Field[] = [];
    // The rate of exchange comes right after the first ledger in a foreign currency.
    const fxAt = this.fxOwner();
    if (this.mode === "single") out.push({ k: "account" });
    if (fxAt === "account") out.push({ k: "fx" });
    if (this.mode === "item") {
      out.push({ k: "party" });
      if (fxAt === "party") out.push({ k: "fx" });
      out.push({ k: "sales" });
      const godown = this.showGodown();
      const batch = this.showBatch();
      this.items.forEach((_, i) => {
        out.push({ k: "item", i });
        if (godown) out.push({ k: "godown", i });
        if (batch) out.push({ k: "batch", i });
        out.push({ k: "qty", i }, { k: "rate", i }, { k: "value", i });
      });
    }
    this.lines.forEach((_, i) => {
      if (this.mode === "double") out.push({ k: "side", i });
      out.push({ k: "ledger", i });
      if (fxAt === i) out.push({ k: "fx" });
      out.push({ k: "amount", i });
    });
    out.push({ k: "narration" });
    return out;
  }

  private field(): Field {
    const f = this.fields();
    return f[Math.min(this.at, f.length - 1)];
  }

  private keyOf(f: Field): string {
    return "i" in f ? `${f.k}:${f.i}` : f.k;
  }

  private indexOf(k: Field["k"], i?: number): number {
    return this.fields().findIndex((f) => f.k === k && (i === undefined || ("i" in f && f.i === i)));
  }

  private amountOf(l: Line): M.Units {
    const u = M.parse(l.amount);
    return u === null ? M.ZERO : M.abs(u);
  }

  /** Every entry the voucher would post right now (complete lines only). */
  private entries(): VEntry[] {
    const lines = this.lines.filter((l) => l.ledger && this.amountOf(l) > 0n);
    const out: VEntry[] = lines.map((l) => ({
      ledger: l.ledger!,
      side: l.side,
      amount: this.amountOf(l),
      bills: l.bills,
      items: l.items,
      fx: this.isFx(l.ledger) ? M.abs(M.parse(l.fx ?? "") ?? M.ZERO) : undefined,
    }));
    if (this.mode === "single" && this.account) {
      const total = M.sum(out.map((e) => e.amount));
      if (total > 0n) out.push({ ledger: this.account, side: other(particularSide(this.kind)), amount: total, fx: this.fxBalance(this.account, out) });
    }
    if (this.mode === "item") {
      const stocked = this.items.filter(itemDone);
      const goods = this.itemsTotal();
      if (this.salesLedger && goods > 0n) out.unshift({ ledger: this.salesLedger, side: this.salesSide(), amount: goods, items: stocked });
      const total = this.invoiceTotal();
      if (this.party && total > 0n) out.unshift({ ledger: this.party, side: this.partySide(), amount: total, bills: this.partyBills, fx: this.fxBalance(this.party, out) });
    }
    return out;
  }

  // ---------------------------------------------------- foreign currency

  /** The voucher's foreign currency: that of the first ledger kept in one. */
  private fxCode(): string | null {
    return this.fxCodes()[0] ?? null;
  }

  /** Every foreign currency on the voucher's ledgers (one is allowed). */
  private fxCodes(): string[] {
    const ledgers = [this.party, this.account, ...this.lines.map((l) => l.ledger)];
    const codes = ledgers.map((l) => l?.currency ?? "").filter((c) => c && c !== this.fxBase);
    return [...new Set(codes)];
  }

  /** A ledger whose amounts are typed in the voucher's foreign currency. */
  private isFx(l: Ledger | null | undefined): boolean {
    const code = this.fxCode();
    return Boolean(code && l?.currency === code);
  }

  /** Where the rate field goes: after the party, the account or a line's ledger. */
  private fxOwner(): "party" | "account" | number | null {
    if (!this.fxCode()) return null;
    if (this.mode === "item" && this.isFx(this.party)) return "party";
    if (this.mode === "single" && this.isFx(this.account)) return "account";
    const i = this.lines.findIndex((l) => this.isFx(l.ledger));
    if (i >= 0) return i;
    return this.mode === "item" ? "party" : this.mode === "single" ? "account" : 0;
  }

  /** The rate as units, or null when not (yet) a positive number. */
  private fxUnits(): M.Units | null {
    const r = M.parse(this.fxRate);
    return r !== null && r > 0n ? r : null;
  }

  /** A foreign amount in the base currency, at the voucher's rate. */
  private toBase(fx: string): string {
    const rate = this.fxUnits();
    const u = M.parse(fx);
    if (rate === null || u === null || !fx.trim()) return "";
    return M.toDecimal(lineAmount(M.abs(u), rate));
  }

  /** A base amount in the foreign currency, at the voucher's rate. */
  private toForeign(base: M.Units): M.Units {
    const rate = this.fxUnits();
    return rate === null ? M.ZERO : rateOf(base, rate);
  }

  /** The foreign amount of an entry: as typed, else its base amount converted. */
  private fxOf(e: VEntry): M.Units {
    return e.fx ?? this.toForeign(e.amount);
  }

  /** A derived line (the item invoice's party, single entry's account) in a foreign currency: the others' foreign amounts, so the invoice adds up in that currency. */
  private fxBalance(l: Ledger, others: VEntry[]): M.Units | undefined {
    if (!this.isFx(l) || this.fxUnits() === null) return undefined;
    const side = this.mode === "item" ? this.partySide() : other(particularSide(this.kind));
    return M.abs(M.sum(others.map((e) => (e.side === side ? -this.fxOf(e) : this.fxOf(e)))));
  }

  /** Base amounts of the foreign lines, after the rate or an amount changed. */
  private syncFx(): void {
    for (const l of this.lines) if (this.isFx(l.ledger) && l.fx !== undefined) l.amount = this.toBase(l.fx);
  }

  /** Offer the last rate used for the voucher's currency once it appears or changes. */
  private async refreshFx(): Promise<void> {
    const code = this.fxCode();
    if (!code || code === this.fxRateFor) return;
    this.fxRateFor = code;
    this.fxRate = "";
    const r = await fxApi.rate(code, this.date).catch(() => null);
    if (this.fxCode() !== code || this.fxRate) return;
    this.fxRate = r ? r.rate : "";
    const inp = this.inputs.get("fx");
    if (inp) {
      inp.value = this.fxRate;
      if (document.activeElement === inp) inp.select();
    }
    this.syncFx();
    this.paintLive();
  }

  /** The foreign side of an entry for the backend (lines of trading vouchers carry one too). */
  private forexOf(e: VEntry): ForexInput | undefined {
    const code = this.fxCode();
    const rate = this.fxUnits();
    if (!code || rate === null) return undefined;
    if (e.fx === undefined && !TRADING.includes(this.kind)) return undefined;
    const amount = this.fxOf(e);
    if (amount <= 0n) return undefined;
    return { currency: code, amount: M.toDecimal(amount), rate: M.toDecimal(rate) };
  }

  /** Item invoice: the value of the item lines. */
  private itemsTotal(): M.Units {
    return M.sum(this.items.filter(itemDone).map(itemAmount));
  }

  /** Item invoice: what the party owes (or is owed): items plus other ledgers. */
  private invoiceTotal(): M.Units {
    let total = this.itemsTotal();
    for (const l of this.lines) {
      if (!l.ledger) continue;
      total += l.side === this.salesSide() ? this.amountOf(l) : -this.amountOf(l);
    }
    return total;
  }

  /** Stock of an item after this voucher (in positive, out negative). */
  private projectedQty(item: StockItem): M.Units {
    let effect = M.ZERO;
    const add = (side: Side, list: ItemLine[] | undefined) => {
      for (const l of list ?? []) {
        if (l.item?.id !== item.id) continue;
        const q = M.parse(l.qty) ?? M.ZERO;
        effect += side === "Dr" ? q : -q;
      }
    };
    if (this.mode === "item") add(this.salesSide(), this.items);
    for (const l of this.lines) if (l.ledger) add(l.side, l.items);
    return M.of(item.closing_qty) - (this.savedStock.get(item.id) ?? M.ZERO) + effect;
  }

  private totals(): [M.Units, M.Units] {
    let dr = M.ZERO;
    let cr = M.ZERO;
    for (const l of this.lines) {
      if (!l.ledger) continue;
      if (l.side === "Dr") dr += this.amountOf(l);
      else cr += this.amountOf(l);
    }
    return [dr, cr];
  }

  /** Balance as it will be after this voucher (V3). */
  private projected(l: Ledger): M.Units {
    const effect = M.sum(this.entries().filter((e) => e.ledger.id === l.id).map((e) => (e.side === "Dr" ? e.amount : -e.amount)));
    return M.of(l.balance) - (this.savedEffect.get(l.id) ?? M.ZERO) + effect;
  }

  /** What would be saved, for spotting changes to a loaded voucher. */
  private state(): string {
    const entries = this.entries().map((e) => [
      e.ledger.id,
      e.side,
      M.toDecimal(e.amount),
      e.bills ?? [],
      (e.items ?? []).map((l) => [l.item?.id, l.qty, l.rate, M.toDecimal(itemAmount(l)), l.godownId, l.batch.trim()]),
    ]);
    return JSON.stringify([this.date, this.narration.value, this.fxCode() ? this.fxRate : "", entries.sort()]);
  }

  private dirty(): boolean {
    if (this.loaded !== null) return this.state() !== this.loaded;
    return (
      this.lines.some((l) => l.ledger || l.amount.trim()) ||
      this.narration.value.trim() !== "" ||
      (this.mode === "item" && (this.party !== null || this.items.some((l) => l.item || l.qty.trim())))
    );
  }

  private filterFor(f: Field): LedgerFilter {
    if (this.kind === "Contra") return "cash_bank";
    if (f.k === "account") return "cash_bank";
    if (f.k === "party") return "party";
    if (this.mode === "double" && "i" in f && f.i === 0 && PARTY_FIRST.includes(this.kind)) return "party";
    return "all";
  }

  /** Groups offered when creating a ledger from this field (P7). */
  private groupFilterFor(f: Field): ((g: Group) => boolean) | undefined {
    if (f.k === "sales") return (g) => g.nature === this.salesNature();
    const filter = this.filterFor(f);
    if (filter === "cash_bank") return (g) => g.is_cash_or_bank;
    if (filter === "party") return (g) => g.is_cash_or_bank || g.is_party;
    if (this.mode === "single" && (this.kind === "Payment" || this.kind === "Receipt")) return (g) => !g.is_cash_or_bank;
    return undefined;
  }

  /** Sales ledgers are income, purchase ledgers expenses. */
  private salesNature(): Group["nature"] {
    return tradeNoun(this.kind) === "sales" ? "Income" : "Expenses";
  }

  private salesLabel(): string {
    return t(tradeNoun(this.kind) === "sales" ? "Sales ledger" : "Purchase ledger");
  }

  // ----------------------------------------------------------- render

  private render(): void {
    this.picker?.close();
    this.picker = null;
    this.inputs.clear();
    this.curBal.clear();
    this.itemBal.clear();
    this.badge.textContent = this.mode === "item" ? t("{kind} · Item Invoice", { kind: kindName(this.kind) }) : kindName(this.kind);
    this.badge.className = `badge kind-${this.kind}`;
    this.numberEl.textContent = t("No. {number}", { number: ltr(this.number) });
    this.dateEl.textContent = D.show(this.date);
    this.dayEl.textContent = D.weekday(this.date);
    const rows: HTMLElement[] = [];
    const fxCode = this.fxCode();
    if (fxCode) {
      const rateIn = this.input("fx", this.fxRate, "amount");
      rateIn.addEventListener("input", () => {
        this.fxRate = rateIn.value;
        this.fxRateFor = fxCode;
        this.syncFx();
        this.paintLive();
        if (this.mode === "double") this.scheduleTax();
      });
      rows.push(
        h(
          "div.vaccount.vfx",
          h("span.flabel", t("Rate of exchange")),
          h("div.fstack", h("span.vfx-rate", ltr(`1 ${fxCode} =`), rateIn, ltr(this.fxBase)), this.fxNote),
        ),
      );
    }
    if (this.mode === "single") {
      const inp = this.input("account", this.account?.name ?? "");
      rows.push(h("div.vaccount", h("span.flabel", t("Account")), h("div.fstack", inp, this.accountBal)));
      rows.push(h("div.vgrid.single.vcols", h("span", t("Particulars")), h("span.num", t("Amount"))));
    } else if (this.mode === "item") {
      rows.push(...this.renderItems());
      if (this.lines.length || this.taxMissing.length) rows.push(h("div.vgrid.single.vcols.vextra", h("span", t("Other ledgers (tax, discount…)")), h("span.num", t("Amount"))));
    } else {
      rows.push(h("div.vgrid.double.vcols", h("span", t("Dr/Cr")), h("span", t("Particulars")), h("span.num", t("Debit")), h("span.num", t("Credit"))));
    }
    this.lines.forEach((l, i) => {
      const ledgerIn = this.input(`ledger:${i}`, l.ledger?.name ?? "");
      const foreign = this.isFx(l.ledger);
      const fxText = l.fx?.trim() ? M.format(M.abs(M.parse(l.fx) ?? M.ZERO)) : "";
      const amountIn = this.input(`amount:${i}`, foreign ? fxText : this.amountText(l), "amount");
      if (foreign) amountIn.placeholder = fxCode ?? "";
      amountIn.addEventListener("input", () => {
        // A ledger in a foreign currency takes its amount in it; the base amount follows the rate.
        if (this.isFx(l.ledger)) {
          l.fx = amountIn.value;
          l.amount = this.toBase(l.fx);
        } else l.amount = amountIn.value;
        // Typed over: the user's own line from now on.
        if (l.auto) amountIn.closest(".vline")?.classList.remove("vtax-auto");
        delete l.auto;
        // Item invoice: a minus amount (a discount) takes the party's side.
        const u = M.parse(l.amount);
        if (this.mode === "item" && u !== null && u !== 0n) l.side = u < 0n ? this.partySide() : this.salesSide();
        this.paintLive();
        // As voucher, a sales or service line's amount is the taxable value.
        if (this.mode === "double") this.scheduleTax();
        else this.scheduleTds();
      });
      amountIn.addEventListener("blur", () => {
        const u = M.parse(amountIn.value);
        if (u !== null && amountIn.value.trim()) amountIn.value = u === 0n ? "" : M.format(this.mode === "item" ? u : M.abs(u));
      });
      const bal = h("span.curbal");
      this.curBal.set(i, bal);
      const particulars = h("div.fstack", ledgerIn, bal);
      if (this.mode === "single" || this.mode === "item") {
        rows.push(h("div.vgrid.single.vline", { class: l.auto ? "vtax-auto" : "" }, particulars, amountIn));
      } else {
        // Shown in the display language; D and C (physical keys) still set it.
        const sideIn = this.input(`side:${i}`, t(l.side), "drcr");
        sideIn.readOnly = true;
        const blank = h("span.amount-blank");
        rows.push(
          h(
            "div.vgrid.double.vline",
            { class: l.auto ? "vtax-auto" : "" },
            sideIn,
            particulars,
            l.side === "Dr" ? amountIn : blank,
            l.side === "Cr" ? amountIn : h("span.amount-blank"),
          ),
        );
      }
    });
    if (this.taxMode()) rows.push(...this.taxMissing.map((r) => this.missingRow(r)));
    this.body.replaceChildren(...rows);
    if (this.cancelled) {
      for (const inp of this.inputs.values()) inp.readOnly = true;
      this.narration.readOnly = true;
      this.el.classList.add("readonly");
    }
    this.paintLive();
    this.shell.refresh();
  }

  /** Tax no duty ledger collects yet: a reminder in place of a line. */
  private missingRow(r: InvoiceTaxRow): HTMLElement {
    const side = t(tradeNoun(this.kind) === "sales" ? "output" : "input");
    const text = h("span", t("No ledger for {side} {component} {percent}% — create one under Duties & Taxes", { side, component: r.component, percent: percentText(r.percent) }));
    const amount = h("span.num", { title: t(this.mode === "item" ? "Not in the invoice total" : "Not in the totals") }, M.format(M.abs(M.parse(r.amount) ?? M.ZERO)));
    if (this.mode === "item") return h("div.vgrid.single.vline.vtax-missing", text, amount);
    // As voucher: the amount sits in the column it would be posted to.
    const dr = this.salesSide() === "Dr";
    return h("div.vgrid.double.vline.vtax-missing", h("span"), text, dr ? amount : h("span"), dr ? h("span") : amount);
  }

  /** Item invoice rows: party, sales or purchase ledger, then the item lines. */
  private renderItems(): HTMLElement[] {
    const party = this.input("party", this.party?.name ?? "");
    const sales = this.input("sales", this.salesLedger?.name ?? "");
    const godown = this.showGodown();
    const batch = this.showBatch();
    const cols = `${godown ? "wg" : ""} ${batch ? "wb" : ""}`.trim();
    const rows: HTMLElement[] = [
      h(
        "div.vparty",
        h("div.vaccount", h("span.flabel", t("Party")), h("div.fstack", party, this.partyBal)),
        h("div.vaccount", h("span.flabel", this.salesLabel()), h("div.fstack", sales, this.salesBal)),
      ),
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
    this.items.forEach((l, i) => {
      const itemIn = this.input(`item:${i}`, l.item?.name ?? "");
      let godownIn: HTMLInputElement | null = null;
      if (godown) {
        godownIn = this.input(`godown:${i}`, l.godownName);
        godownIn.placeholder = t(MAIN_LOCATION);
      }
      let batchIn: HTMLInputElement | null = null;
      if (batch) {
        const inp = this.input(`batch:${i}`, l.batch);
        inp.addEventListener("input", () => (l.batch = inp.value));
        batchIn = inp;
      }
      const qtyIn = this.input(`qty:${i}`, l.qty, "amount");
      const rateIn = this.input(`rate:${i}`, l.rate.trim() ? M.format(M.of(l.rate)) : "", "amount");
      const valueIn = this.input(`value:${i}`, l.amount.trim() ? M.format(itemAmount(l)) : "", "amount");
      qtyIn.placeholder = l.item?.unit ?? "";
      const changed = () => {
        this.recompute(l);
        valueIn.value = l.amount.trim() ? M.format(itemAmount(l)) : "";
        this.paintLive();
        this.scheduleTax();
      };
      qtyIn.addEventListener("input", () => {
        l.qty = qtyIn.value;
        changed();
      });
      rateIn.addEventListener("input", () => {
        l.rate = rateIn.value;
        changed();
      });
      valueIn.addEventListener("input", () => {
        l.amount = valueIn.value;
        this.paintLive();
        this.scheduleTax();
      });
      for (const inp of [rateIn, valueIn]) {
        inp.addEventListener("blur", () => {
          const u = M.parse(inp.value);
          if (u !== null && inp.value.trim()) inp.value = M.format(M.abs(u));
        });
      }
      const stock = h("span.curbal");
      this.itemBal.set(i, stock);
      rows.push(h("div.vgrid.items.vline", { class: cols }, h("div.fstack", itemIn, stock), godownIn, batchIn, qtyIn, rateIn, valueIn));
    });
    return rows;
  }

  /** qty × rate → amount, when both are given. */
  private recompute(l: ItemLine): void {
    const q = M.parse(l.qty);
    const r = M.parse(l.rate);
    if (q !== null && r !== null && l.rate.trim() && l.qty.trim()) l.amount = M.toDecimal(lineAmount(M.abs(q), M.abs(r)));
  }

  /** A line's amount as shown; in an item invoice a minus marks the party's side. */
  private amountText(l: Line, edit = false): string {
    if (this.mode !== "item") return l.amount ? (edit ? M.toDecimal(this.amountOf(l)) : M.format(this.amountOf(l))) : "";
    if (!l.amount.trim()) return "";
    const a = this.amountOf(l);
    if (a === 0n) return "";
    const neg = l.side === this.partySide();
    return (neg ? "-" : "") + (edit ? M.toDecimal(a) : M.format(a));
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

  private paintLive(): void {
    const label = (l: Ledger | null) => (l ? t("Cur Bal: {balance}", { balance: M.drCr(this.projected(l)) }) : "");
    this.lines.forEach((l, i) => {
      const el = this.curBal.get(i);
      const n = l.items?.length ?? 0;
      if (!el) return;
      const fxPart = this.isFx(l.ledger) && l.amount.trim() ? ` · ${t("{amount} in {currency}", { amount: M.format(this.amountOf(l)), currency: this.fxBase })}` : "";
      const text = label(l.ledger) + (l.ledger && n ? t(n === 1 ? " · {n} stock item" : " · {n} stock items", { n }) : "") + fxPart;
      if (l.auto) el.replaceChildren(h("span.vtax-tag", t("auto")), `${ltr(`${l.auto.component} ${percentText(l.auto.percent)}%`)} · ${text}`);
      else el.textContent = text;
    });
    const code = this.fxCode();
    const derived = (l: Ledger | null) => {
      const e = l && this.isFx(l) ? this.entries().find((x) => x.ledger.id === l.id) : undefined;
      return e?.fx !== undefined ? ` · ${ltr(`${code} ${M.format(e.fx)}`)}` : "";
    };
    this.accountBal.textContent = this.account ? t("Current balance: {balance}", { balance: M.drCr(this.projected(this.account)) }) + derived(this.account) : "";
    const settles = this.lines.some((l) => this.isFx(l.ledger) && l.bills?.some((b) => b.kind === "Against"));
    this.fxNote.textContent = !code
      ? ""
      : this.fxCodes().length > 1
        ? t("One voucher can be in one foreign currency only.")
        : this.fxUnits() === null
          ? t("Enter the rate of exchange for {currency}.", { currency: code })
          : settles
            ? t("A gain or loss on bills settled at this rate goes to Forex Gain/Loss when saved.")
            : "";
    if (this.mode === "item") {
      this.partyBal.textContent = label(this.party) + derived(this.party);
      this.salesBal.textContent = label(this.salesLedger);
      this.items.forEach((l, i) => {
        const el = this.itemBal.get(i);
        if (!el) return;
        const q = l.item ? this.projectedQty(l.item) : M.ZERO;
        el.textContent = l.item ? t("Closing stock: {quantity} {unit}", { quantity: qtyText(q), unit: l.item.unit }) : "";
        el.classList.toggle("neg", Boolean(l.item) && q < 0n);
      });
      this.totalsEl.replaceChildren(
        h("span.flabel", t("Items")),
        h("span.num", M.format(this.itemsTotal())),
        h("span.flabel", t("Invoice total")),
        h("span.num.total", M.format(this.invoiceTotal())),
      );
    } else if (this.mode === "single") {
      const total = M.sum(this.lines.map((l) => (l.ledger ? this.amountOf(l) : M.ZERO)));
      this.totalsEl.replaceChildren(h("span.flabel", t("Total")), h("span.num.total", M.format(total)));
    } else {
      const [dr, cr] = this.totals();
      this.totalsEl.replaceChildren(
        h("span.flabel", t("Total")),
        h("span.num.total", M.format(dr)),
        h("span.num.total", M.format(cr)),
        ...(dr !== cr ? [h("span.diff", t("Difference {amount}", { amount: M.format(M.abs(dr - cr)) }))] : []),
      );
    }
  }

  // ------------------------------------------------------------ focus

  focus(): void {
    this.go(this.at);
  }

  resume(): void {
    void this.picker?.reload();
    // A duty ledger may have just been created for tax that had none.
    if (this.taxMissing.length) this.scheduleTax();
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

  /** Field gained focus (keyboard or mouse): attach the list picker for ledger fields. */
  private onFocus(i: number): void {
    const f = this.fields()[i];
    const prev = this.picker;
    this.at = i;
    if (prev) {
      prev.close();
      this.picker = null;
      this.restoreLedgerText();
    }
    if ((f.k === "ledger" || f.k === "account" || f.k === "party" || f.k === "sales") && !this.cancelled) {
      this.picker = this.makePicker(f);
      void this.picker.open();
    }
    if (f.k === "item" && !this.cancelled) {
      this.picker = this.makeItemPicker(f.i);
      void this.picker.open();
    }
    if (f.k === "godown" && !this.cancelled) {
      this.picker = this.makeGodownPicker(f.i);
      void this.picker.open();
    }
    if (f.k === "amount" || f.k === "rate" || f.k === "value") {
      const inp = this.inputFor(f);
      const u = M.parse(inp.value);
      if (u !== null && u !== 0n) inp.value = M.toDecimal(f.k === "amount" && this.mode === "item" ? u : M.abs(u));
    }
    this.shell.refresh();
  }

  private restoreLedgerText(): void {
    for (const [key, inp] of this.inputs) {
      if (key === "account") inp.value = this.account?.name ?? "";
      else if (key === "party") inp.value = this.party?.name ?? "";
      else if (key === "sales") inp.value = this.salesLedger?.name ?? "";
      else if (key.startsWith("item:")) inp.value = this.items[Number(key.slice(5))]?.item?.name ?? "";
      else if (key.startsWith("godown:")) inp.value = this.items[Number(key.slice(7))]?.godownName ?? "";
      else if (key.startsWith("ledger:")) inp.value = this.lines[Number(key.slice(7))]?.ledger?.name ?? "";
    }
  }

  private makePicker(f: LedgerField): ListPicker<Ledger> {
    const filter = this.filterFor(f);
    const line = f.k === "ledger" ? f.i : -1;
    // An item invoice's other ledgers are optional: End of List from the first.
    const endAt = this.mode === "item" ? 0 : 1;
    const actions = (): PickAction[] => {
      const out: PickAction[] = [];
      if (line >= endAt) out.push({ id: "end", label: t("End of List"), run: () => void this.endOfList(line) });
      const chord = this.shell.km.chordIn("CreateMasterInline", ["ListField"]);
      out.push({ id: "create", label: t("Create Ledger"), hint: chord ?? undefined, run: (text) => this.createInline(f, text) });
      return out;
    };
    const title =
      f.k === "sales"
        ? `${tradeNoun(this.kind) === "sales" ? "Sales" : "Purchase"} ledgers`
        : filter === "cash_bank"
          ? "Cash and bank ledgers"
          : filter === "party"
            ? "Party ledgers"
            : "Ledgers";
    return new ListPicker<Ledger>({
      input: this.inputFor(f),
      host: this.el,
      title,
      columns: ["Name", "Balance"],
      source: async (q) => {
        const ledgers = await api.listLedgers(q, filter);
        return ledgers
          .filter((l) => f.k === "account" || this.mode !== "single" || l.id !== this.account?.id)
          .filter((l) => f.k !== "sales" || l.nature === this.salesNature())
          .map<PickItem<Ledger>>((l) => ({ id: l.id, label: l.name, sub: M.drCr(M.of(l.balance)), data: l }));
      },
      actions,
      create: { noun: "ledger", run: (t) => this.createInline(f, t) },
      selectedId: () =>
        (f.k === "account" ? this.account?.id : f.k === "party" ? this.party?.id : f.k === "sales" ? this.salesLedger?.id : this.lines[line]?.ledger?.id) ??
        null,
      preferAction: () => {
        if (line < endAt || this.lines[line]?.ledger) return null;
        if (this.mode !== "double") return "end";
        const [dr, cr] = this.totals();
        return dr === cr ? "end" : null;
      },
      onPick: (it) => this.setLedger(f, it.data),
      onAlter: (it) =>
        void this.shell.push(
          new LedgerForm(this.shell, { alterId: it.data.id, inline: true, onCreated: (l) => this.setLedger(f, l) }),
        ),
    });
  }

  /** Item invoice: the stock item picker, with closing stock beside each item. */
  private makeItemPicker(i: number): ListPicker<StockItem> {
    const actions = (): PickAction[] => {
      const out: PickAction[] = [];
      if (i > 0) out.push({ id: "end", label: t("End of List"), run: () => void this.endOfItems(i) });
      const chord = this.shell.km.chordIn("CreateMasterInline", ["ListField"]);
      out.push({ id: "create", label: t("Create Stock Item"), hint: chord ?? undefined, run: (text) => this.createItemInline(i, text) });
      return out;
    };
    return new ListPicker<StockItem>({
      input: this.inputFor({ k: "item", i }),
      host: this.el,
      title: "Stock items",
      columns: ["Name", "In stock"],
      source: async (q) =>
        (await api.listStockItems(q)).map<PickItem<StockItem>>((s) => ({
          id: s.id,
          label: s.name,
          sub: ltr(`${qtyText(this.projectedQty(s))} ${s.unit}`),
          data: s,
        })),
      actions,
      create: { noun: "stock item", run: (t) => this.createItemInline(i, t) },
      selectedId: () => this.items[i]?.item?.id ?? null,
      preferAction: () => (i > 0 && !this.items[i]?.item ? "end" : null),
      onPick: (it) => void this.setItem(i, it.data),
      onAlter: (it) => this.alterItemInline(i, it.data),
    });
  }

  /** Item line: where the stock moves; "Main Location" sits pinned at the top. */
  private makeGodownPicker(i: number): ListPicker<Godown> {
    return new ListPicker<Godown>({
      input: this.inputFor({ k: "godown", i }),
      host: this.el,
      title: "Godowns",
      columns: ["Name", "Under"],
      source: async (q) =>
        (await api.listGodowns(q)).map<PickItem<Godown>>((g) => ({ id: g.id, label: g.name, sub: g.parent_name ?? "", data: g })),
      actions: () => {
        const chord = this.shell.km.chordIn("CreateMasterInline", ["ListField"]);
        return [
          { id: "main", label: t(MAIN_LOCATION), run: () => this.setGodown(i, null) },
          { id: "create", label: t("Create Godown"), hint: chord ?? undefined, run: (text) => this.createGodownInline(i, text) },
        ];
      },
      create: { noun: "godown", run: (t) => this.createGodownInline(i, t) },
      selectedId: () => this.items[i]?.godownId || null,
      preferAction: () => (this.items[i]?.godownId ? null : "main"),
      onPick: (it) => this.setGodown(i, it.data),
      onAlter: (it) =>
        void import("./godown").then(({ GodownForm }) =>
          this.shell.push(new GodownForm(this.shell, { alterId: it.data.id, inline: true, onCreated: (g) => this.setGodown(i, g) })),
        ),
    });
  }

  private createGodownInline(i: number, text: string): void {
    // Loaded on demand: godown.ts imports from this module.
    void import("./godown").then(({ GodownForm }) =>
      this.shell.push(
        new GodownForm(this.shell, {
          name: text,
          inline: true,
          onCreated: (g) => {
            if (!this.godowns.some((x) => x.id === g.id)) this.godowns.push(g);
            this.setGodown(i, g);
          },
        }),
      ),
    );
  }

  private setGodown(i: number, g: Godown | null): void {
    const l = this.items[i];
    if (!l) return;
    l.godownId = g?.id ?? "";
    l.godownName = g?.name ?? "";
    const inp = this.inputs.get(`godown:${i}`);
    if (inp) inp.value = l.godownName;
    this.go(this.indexOf("godown", i) + 1);
  }

  /** Last rate of an item, asked once per item, voucher type and party while the screen is open. */
  private lastRate(s: StockItem): Promise<string> {
    const party = this.party?.id ?? "";
    const key = `${s.id}:${this.kind}:${party}`;
    let rate = this.rates.get(key);
    if (!rate) {
      rate = api.lastRate(s.id, this.kind, party).catch(() => "");
      this.rates.set(key, rate);
    }
    return rate;
  }

  /**
   * Ctrl+R repeats the narration of the last voucher of this type; Alt+R that of
   * the last voucher of any type with a line for `narrationLedger()`. The text
   * comes in selected, so typing replaces it; a different narration already
   * typed is replaced only after asking.
   */
  private async retrieveNarration(sameLedger: boolean): Promise<void> {
    const ledger = sameLedger ? this.narrationLedger() : null;
    if (sameLedger && !ledger) return this.shell.warn("Pick a ledger first.");
    let text: string;
    try {
      text = await api.lastNarration(ledger ? null : this.kind, ledger?.id ?? "");
    } catch (e) {
      return this.shell.error(e);
    }
    if (!text) {
      return this.shell.info(
        ledger
          ? t("No earlier narration for {ledger}.", { ledger: ledger.name })
          : t("No earlier narration for {kind} vouchers.", { kind: kindName(this.kind) }),
      );
    }
    const typed = this.narration.value.trim();
    if (typed && typed !== text && !(await this.shell.ask("Replace the narration?"))) return;
    this.narration.value = text;
    this.go(this.indexOf("narration"));
  }

  /**
   * Alt+R's ledger: the one in the field under the cursor, else the voucher's
   * first ledger: the party on an invoice, the first line otherwise.
   */
  private narrationLedger(): Ledger | null {
    const f = this.field();
    const here =
      f.k === "account" ? this.account
      : f.k === "party" ? this.party
      : f.k === "side" || f.k === "ledger" || f.k === "amount" ? this.lines[f.i]?.ledger
      : null;
    return here ?? (this.mode === "item" ? this.party : (this.lines.find((l) => l.ledger)?.ledger ?? this.account));
  }

  /** The item's default rate: the last one traded at, else (purchases) its cost. */
  private async defaultRate(s: StockItem): Promise<string> {
    const last = await this.lastRate(s);
    if (last && (M.parse(last) ?? M.ZERO) > 0n) return last;
    if (tradeNoun(this.kind) === "sales") return "";
    const q = M.of(s.closing_qty);
    const v = M.of(s.closing_value);
    if (q > 0n && v > 0n) return M.toDecimal(rateOf(v, q));
    return M.of(s.opening_rate) > 0n ? s.opening_rate : "";
  }

  private async setItem(i: number, s: StockItem): Promise<void> {
    const l = this.items[i];
    if (!l) return;
    const fill = async (): Promise<void> => {
      if (l.item?.id !== s.id || !l.rate.trim()) {
        const rate = await this.defaultRate(s);
        if (this.items[i] !== l) return;
        l.rate = rate;
      }
      l.item = s;
      this.recompute(l);
      this.render();
      this.go(this.indexOf("item", i) + 1);
      this.scheduleTax();
    };
    // Keys typed while the rate loads wait until the cursor has moved on (F8).
    await this.shell.transition(fill());
  }

  private createItemInline(i: number, text: string): void {
    // Loaded on demand: inventory.ts builds on the reports, which open vouchers.
    void import("./inventory").then(({ StockItemForm }) =>
      this.shell.push(new StockItemForm(this.shell, { name: text, inline: true, onCreated: (s) => this.setItem(i, s) })),
    );
  }

  /** Ctrl+Enter on an item: alter it in place, then carry on (P8). */
  private alterItemInline(i: number, s: StockItem): void {
    void import("./inventory").then(({ StockItemForm }) =>
      this.shell.push(new StockItemForm(this.shell, { alterId: s.id, inline: true, onCreated: (fresh) => this.setItem(i, fresh) })),
    );
  }

  private setLedger(f: LedgerField, l: Ledger): void {
    const fxBefore = this.fxCode();
    const fxField = this.fxOwner();
    // A ledger in a foreign currency brings the rate field (and the last rate) in.
    const fxMoved = () => {
      if (this.fxCode() === fxBefore && this.fxOwner() === fxField) return false;
      this.render();
      void this.refreshFx();
      return true;
    };
    if (f.k === "account") {
      this.account = l;
      fxMoved();
      this.inputFor(f).value = l.name;
      this.paintLive();
      this.go(this.at + 1);
      return;
    }
    if (f.k === "party" || f.k === "sales") {
      const changed = f.k === "party" && this.party?.id !== l.id;
      if (f.k === "party") {
        if (changed) this.partyBills = undefined;
        this.party = l;
      } else this.salesLedger = l;
      fxMoved();
      this.inputFor(f).value = l.name;
      this.paintLive();
      this.go(this.at + 1);
      // The party's place decides which taxes apply.
      if (changed) this.scheduleTax();
      return;
    }
    const line = this.lines[f.i];
    if (line.auto && line.ledger?.id !== l.id) delete line.auto;
    line.ledger = l;
    if (!this.isFx(l)) delete line.fx;
    fxMoved();
    // As voucher, the party or a taxed sales line may have changed.
    if (this.mode === "double") this.scheduleTax();
    else this.scheduleTds();
    this.inputFor(f).value = l.name;
    const fill = () => {
      // A TDS line may have come in meanwhile: find the line again.
      const i = this.lines.indexOf(line);
      if (i < 0) return;
      const foreign = this.isFx(line.ledger);
      if (this.mode === "double" && i > 0 && !line.amount.trim()) {
        const [dr, cr] = this.totals();
        const gap = line.side === "Dr" ? cr - dr : dr - cr;
        // In a foreign currency: the gap at the voucher's rate, once there is one.
        if (gap > 0n && foreign && this.fxUnits() !== null) {
          line.fx = M.toDecimal(this.toForeign(gap));
          line.amount = M.toDecimal(gap);
        } else if (gap > 0n && !foreign) line.amount = M.toDecimal(gap);
      }
      const amt = this.inputs.get(`amount:${i}`)!;
      amt.value = foreign ? (line.fx?.trim() ? M.toDecimal(M.abs(M.parse(line.fx) ?? M.ZERO)) : "") : line.amount ? M.toDecimal(this.amountOf(line)) : "";
      this.paintLive();
      // The rate of exchange first, when it comes after this ledger.
      this.go(this.fxOwner() === i && this.fields()[this.indexOf("ledger", i) + 1]?.k === "fx" ? this.indexOf("fx") : this.indexOf("amount", i));
    };
    // A party picked for an expense line: its TDS first, so the amount offered is net of it.
    if (this.mode === "double" && this.tdsMode() && !line.amount.trim() && isDeductee(l)) {
      void this.shell.transition(this.flushTds().then(fill));
      return;
    }
    fill();
  }

  private createInline(f: LedgerField, text: string): void {
    void this.shell.push(
      new LedgerForm(this.shell, {
        name: text,
        inline: true,
        groupFilter: this.groupFilterFor(f),
        onCreated: (l) => this.setLedger(f, l),
      }),
    );
  }

  // ------------------------------------------------------------- keys

  contexts(): Context[] {
    const f = this.field();
    if (f.k === "ledger" || f.k === "account" || f.k === "party" || f.k === "sales" || f.k === "item" || f.k === "godown")
      return ["ListField", "Voucher"];
    if (f.k === "amount" || f.k === "qty" || f.k === "rate" || f.k === "value") return ["AmountField", "Voucher"];
    return ["Voucher"];
  }

  handles(a: string): boolean {
    if (a.startsWith("OpenVoucher:")) return !this.alterId && ((KINDS as string[]).includes(a.slice(12)) || a === "OpenVoucher:StockJournal");
    if (a === "DeleteRecord") return Boolean(this.alterId);
    if (a === "CancelVoucher" || a === "PrintCurrent") return Boolean(this.alterId) && !this.cancelled;
    if (a === "ExportCurrent" || a === "ExportMenu") return this.canExport();
    if (a === "AddNote") return Boolean(this.alterId);
    if (this.cancelled) return a === "Back";
    if (a === "MoreDetails") return this.mode === "item";
    return ["ChangeDate", "Accept", "Back", "ChangeMode", "RemoveLine"].includes(a);
  }

  current(a: string): boolean {
    return a === `OpenVoucher:${this.kind}`;
  }

  bottom() {
    const ctx = this.contexts();
    const f = this.field();
    return [
      this.shell.btn("Accept", ctx),
      this.shell.btn("ChangeMode", ctx, this.modeLabel(), SINGLE_OK.includes(this.kind) || TRADING.includes(this.kind)),
      this.shell.btn("RemoveLine", ctx, "Remove Line", "i" in f),
      f.k === "ledger" || f.k === "account" || f.k === "party" || f.k === "sales" ? this.shell.btn("CreateMasterInline", ctx, "Create Ledger") : null,
      f.k === "item" ? this.shell.btn("CreateMasterInline", ctx, "Create Stock Item") : null,
      f.k === "godown" ? this.shell.btn("CreateMasterInline", ctx, "Create Godown") : null,
      this.mode === "item" && !this.cancelled ? this.shell.btn("MoreDetails", ctx, this.showBatch() ? "Hide Batch" : "Batch Column") : null,
      this.alterId && !this.cancelled ? this.shell.btn("PrintCurrent", ctx, this.chequeable ? "Print / Cheque" : "Print") : null,
      this.canExport() ? this.shell.btn("ExportCurrent", ctx, "Export") : null,
      this.alterId && !this.cancelled ? this.shell.btn("CancelVoucher", ctx, "Cancel Vch") : null,
      this.alterId ? this.shell.btn("DeleteRecord", ctx, "Delete") : null,
      this.alterId ? this.shell.btn("AddNote", ctx, "Note") : null,
      this.shell.btn("Back", ctx),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  /**
   * Any saved, uncancelled voucher: a PDF in the invoice design, plus the
   * country's e-invoice formats where it has them (screens/einvoice.ts
   * exportVoucher: IRN and JSON in India, ZATCA, PINT, XRechnung, CFDI…).
   */
  private canExport(): boolean {
    return Boolean(this.alterId) && !this.cancelled;
  }

  /** What Ctrl+H switches to. */
  private modeLabel(): string {
    if (this.mode === "single") return "Double Entry";
    if (this.mode === "item") return "As Voucher";
    return TRADING.includes(this.kind) ? "Item Invoice" : "Single Entry";
  }

  key(e: KeyEv): boolean {
    if (this.cancelled) {
      if (e.chord === "Up") {
        if (this.at > 0) this.go(this.at - 1);
        return true;
      }
      return e.key.length === 1 && !e.ctrlKey && !e.altKey; // read-only
    }
    const f = this.field();
    if (this.picker?.key(e)) return true;
    if (f.k === "side") {
      const k = letterOf(e);
      if (!e.ctrlKey && !e.altKey && (k === "d" || k === "c" || e.chord === "Space")) {
        const line = this.lines[f.i];
        line.side = e.chord === "Space" ? other(line.side) : k === "d" ? "Dr" : "Cr";
        this.render();
        this.go(this.at);
        this.scheduleTax();
        return true;
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.altKey) return true;
    }
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true;
    }
    if (e.chord === "Backspace" && f.k === "narration" && this.narration.value === "") {
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
    if (a === "AddNote" && this.alterId) {
      void this.addNote(this.alterId);
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
    if (a === "PrintCurrent" && this.alterId && !this.cancelled) {
      if (this.dirty()) this.shell.info("Printing the saved version. Accept your changes to print them.");
      void (this.chequeable ? this.printPayment(this.alterId) : this.print(this.alterId));
      return true;
    }
    if ((a === "ExportCurrent" || a === "ExportMenu") && this.canExport()) {
      if (this.dirty()) this.shell.info("Exporting the saved version. Accept your changes to export them.");
      const v = { id: this.alterId!, kind: this.kind, number: this.number, date: this.date };
      // Loaded when needed: it builds on the reports, which open vouchers.
      void import("./einvoice").then((m) => m.exportVoucher(this.shell, v));
      return true;
    }
    if (this.cancelled) {
      // Read-only: move between fields, leave with Esc, nothing else.
      if (a === "Back") this.shell.pop();
      else if (a === "NextField") this.go(this.at + 1);
      else if (a === "PreviousField") this.go(this.at - 1);
      else if (a === "Accept" || a === "ChangeMode" || a === "RemoveLine" || a === "ChangeDate")
        this.shell.warn("This voucher is cancelled and can't be changed. Alt+D deletes it.");
      else return false;
      return true;
    }
    if (a.startsWith("OpenVoucher:")) {
      const k = a.slice(12);
      if ((KINDS as string[]).includes(k)) void this.switchKind(k as VoucherKind);
      else if (k === "StockJournal") void this.openStockJournal();
      else this.shell.warn(t("{kind} vouchers aren't available yet.", { kind: t(k.replace(/([a-z])([A-Z])/g, "$1 $2")) }));
      return true;
    }
    switch (a) {
      case "ChangeDate":
        void this.changeDate();
        return true;
      case "ChangeMode":
        this.toggleMode();
        return true;
      case "MoreDetails":
        this.toggleBatch();
        return true;
      case "RemoveLine":
        this.removeLine();
        return true;
      case "RetrieveNarrationSameType":
        void this.retrieveNarration(false);
        return true;
      case "RetrieveNarrationSameLedger":
        void this.retrieveNarration(true);
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
    if (f.k === "side") return this.go(this.at + 1);
    if (f.k === "fx") {
      if (this.fxUnits() === null) return this.shell.warn(t("Enter the rate of exchange for {currency}.", { currency: this.fxCode() ?? "" }));
      return this.go(this.at + 1);
    }
    if (f.k === "amount" && this.isFx(this.lines[f.i].ledger) && this.fxUnits() === null) {
      this.shell.warn(t("Enter the rate of exchange for {currency}.", { currency: this.fxCode() ?? "" }));
      return this.go(this.indexOf("fx"));
    }
    if (f.k === "account") {
      if (!this.account) return this.shell.warn("Pick the cash or bank account.");
      return this.go(this.at + 1);
    }
    if (f.k === "party") {
      if (!this.party) return this.shell.warn("Pick the party ledger.");
      return this.go(this.at + 1);
    }
    if (f.k === "sales") {
      if (!this.salesLedger) return this.shell.warn(pickTradeLedger(this.kind));
      return this.go(this.at + 1);
    }
    if (f.k === "item") {
      if (!this.items[f.i].item) return this.shell.warn(f.i > 0 ? "Pick a stock item, or End of List." : "Pick a stock item.");
      return this.go(this.at + 1);
    }
    if (f.k === "godown" || f.k === "batch") return this.go(this.at + 1);
    if (f.k === "qty") return this.commitQty(f.i);
    if (f.k === "rate") return this.commitRate(f.i);
    if (f.k === "value") return this.commitValue(f.i);
    if (f.k === "ledger") {
      if (!this.lines[f.i].ledger) return this.shell.warn("Pick a ledger, or End of List.");
      return this.go(this.at + 1);
    }
    if (f.k === "amount") {
      const line = this.lines[f.i];
      const u = M.parse(line.amount);
      if (u === null) return this.shell.error(t('"{text}" is not an amount.', { text: ltr(line.amount) }));
      if (this.mode === "item") {
        // A minus amount (a discount) goes on the party's side.
        if (u === 0n) return this.shell.warn("Enter an amount. A minus amount, such as a discount, reduces the total.");
        line.side = u < 0n ? this.partySide() : this.salesSide();
      } else if (u <= 0n) return this.shell.warn("Enter an amount greater than zero.");
      line.amount = M.toDecimal(M.abs(u));
      // A party line always gets its bill-wise details (Tally behaviour).
      if (line.ledger?.is_party) return this.openBills(f.i);
      // As voucher: a taxed sales or service line brings its tax lines in before moving on.
      if (this.mode === "double" && this.taxMode() && (this.taxOn || this.taxable(line))) {
        this.taxOn = true;
        clearTimeout(this.taxTimer);
        this.taxTimer = undefined;
        void this.shell.transition(
          this.refreshTax(false).then(() => {
            // Past the auto lines that came in right after this one.
            let j = this.lines.indexOf(line);
            while (j >= 0 && this.lines[j + 1]?.auto) j++;
            this.render();
            this.afterAmount(j >= 0 ? j : f.i);
          }),
        );
        return;
      }
      return this.afterAmount(f.i);
    }
    void this.confirm();
  }

  private openBills(i: number): void {
    const line = this.lines[i];
    void this.shell.push(
      new BillScreen(this.shell, {
        party: line.ledger!,
        side: line.side,
        lineAmount: this.amountOf(line),
        date: this.date,
        suggested: this.number,
        existing: line.bills ?? [],
        fxRate: this.isFx(line.ledger) ? this.fxUnits() : null,
        onDone: (rows) => {
          line.bills = rows;
          this.afterAmount(i);
        },
        onCancel: () => this.go(this.indexOf("amount", i)),
      }),
    );
  }

  /** Item line: the quantity, within the unit's decimal places. */
  private commitQty(i: number): void {
    const l = this.items[i];
    if (!l.item) {
      this.shell.warn("Pick a stock item first.");
      return this.go(this.indexOf("item", i));
    }
    const u = M.parse(l.qty);
    if (u === null) return this.shell.error(t('"{text}" is not a quantity.', { text: ltr(l.qty) }));
    if (u <= 0n) return this.shell.warn("Enter a quantity greater than zero.");
    const places = l.item.decimals;
    if (placesOf(l.qty) > places)
      return this.shell.warn(
        places === 0
          ? t("{unit} is counted in whole numbers.", { unit: l.item.unit })
          : places === 1
            ? t("{unit} takes up to 1 decimal place.", { unit: l.item.unit })
            : t("{unit} takes up to {n} decimal places.", { unit: l.item.unit, n: places }),
      );
    l.qty = qtyText(u);
    this.recompute(l);
    this.inputFor({ k: "qty", i }).value = l.qty;
    this.inputFor({ k: "value", i }).value = l.amount.trim() ? M.format(itemAmount(l)) : "";
    this.paintLive();
    // Warn, never block: stock may arrive on a later voucher.
    const after = this.projectedQty(l.item);
    if (after < 0n) this.shell.warn(t("Stock of {item} goes below zero: {qty} {unit} after this.", { item: l.item.name, qty: ltr(qtyText(after)), unit: l.item.unit }));
    this.go(this.at + 1);
  }

  private commitRate(i: number): void {
    const l = this.items[i];
    const u = M.parse(l.rate);
    if (u === null) return this.shell.error(t('"{text}" is not a rate.', { text: ltr(l.rate) }));
    if (u < 0n) return this.shell.warn("A rate can't be negative.");
    l.rate = l.rate.trim() ? M.toDecimal(u) : "";
    this.recompute(l);
    this.inputFor({ k: "value", i }).value = l.amount.trim() ? M.format(itemAmount(l)) : "";
    this.paintLive();
    this.go(this.at + 1);
  }

  /** Item line amount: qty × rate, or typed (the rate then follows it). */
  private commitValue(i: number): void {
    const l = this.items[i];
    const u = M.parse(l.amount);
    if (u === null) return this.shell.error(t('"{text}" is not an amount.', { text: ltr(l.amount) }));
    if (u < 0n) return this.shell.warn("An item amount can't be negative. Use a separate ledger line for a discount.");
    if (u === 0n && !l.rate.trim()) return this.shell.warn("Enter a rate or an amount.");
    l.amount = M.toDecimal(u);
    const q = M.parse(l.qty) ?? M.ZERO;
    if (q > 0n && lineAmount(q, M.parse(l.rate) ?? M.ZERO) !== u) l.rate = M.toDecimal(rateOf(u, q));
    if (i === this.items.length - 1) this.items.push(blankItem());
    this.render();
    this.go(this.indexOf("item", i + 1));
  }

  /** End of List on the items: on to the other ledgers, with the tax filled in. */
  private async endOfItems(i: number): Promise<void> {
    if (!this.items[i]?.item && this.items.length > 1) this.items.splice(i, 1);
    const open = this.items.findIndex((l) => l.item && !itemDone(l));
    if (open >= 0) {
      this.render();
      this.shell.warn(t("Enter the quantity of {item}.", { item: this.items[open].item!.name }));
      return this.go(this.indexOf("qty", open));
    }
    if (!this.items.some(itemDone)) {
      this.render();
      this.shell.warn("Add at least one stock item.");
      return this.go(this.indexOf("item", 0));
    }
    if (!this.lines.length || this.lines[this.lines.length - 1].ledger) this.lines.push(this.blankLine(this.lines.length));
    this.taxOn = true;
    clearTimeout(this.taxTimer);
    this.taxTimer = undefined;
    // Keys typed meanwhile land on the first other-ledger line, not the items.
    clearTimeout(this.tdsTimer);
    this.tdsTimer = undefined;
    await this.shell.transition(
      Promise.all([this.refreshTax(false), this.refreshTds(false)])
        .then(() => this.flushTcs(false))
        .then(() => {
        this.render();
        this.go(this.indexOf("ledger", 0));
      }),
    );
  }

  /** Item invoice: the party's bill-wise details, for the invoice total. */
  private openPartyBills(): void {
    const party = this.party!;
    void this.shell.push(
      new BillScreen(this.shell, {
        party,
        side: this.partySide(),
        lineAmount: this.invoiceTotal(),
        date: this.date,
        suggested: this.number,
        existing: this.partyBills ?? [],
        fxRate: this.isFx(party) ? this.fxUnits() : null,
        onDone: (rows) => {
          this.partyBills = rows;
          this.go(this.fields().length - 1);
        },
        onCancel: () => {
          this.lines.push(this.blankLine(this.lines.length));
          this.render();
          this.go(this.indexOf("ledger", this.lines.length - 1));
        },
      }),
    );
  }

  /** Move on from a line's amount: next line, or End of List once balanced. */
  private afterAmount(i: number): void {
    // Past the auto lines (tax, TDS) that sit right after this one.
    while (this.lines[i + 1]?.auto) i++;
    {
      const f = { i };
      if (f.i === this.lines.length - 1) {
        this.lines.push(this.blankLine(f.i + 1));
        this.render();
      }
      const [dr, cr] = this.totals();
      // Once balanced, skip the Dr/Cr cell so Enter lands on "End of List".
      if (this.mode === "double" && dr === cr) return this.go(this.indexOf("ledger", f.i + 1));
      return this.go(this.indexOf(this.mode === "double" ? "side" : "ledger", f.i + 1));
    }
  }

  private async cancelSaved(): Promise<void> {
    if (!this.alterId || this.cancelled) return;
    const label = t("{kind} No. {number}", { kind: kindName(this.kind), number: ltr(this.number) });
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
    const kind = kindName(this.kind);
    if (!this.alterId || !(await this.shell.ask(t("Delete {kind} No. {number}?", { kind, number: ltr(this.number) }), { yesFirst: false }))) return;
    try {
      await api.deleteVoucher(this.alterId);
      await this.shell.refreshCompany();
      this.shell.info(t("{kind} No. {number} deleted. The audit trail keeps a full copy.", { kind, number: ltr(this.number) }));
      this.alterId = null;
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async endOfList(i: number): Promise<void> {
    // Totals must include the latest tax; keys typed meanwhile wait (F8).
    await this.shell.transition(
      this.flushTax()
        .then(() => this.flushTds())
        .then(() => this.flushTcs())
        .then(() => this.finishLines(i)),
    );
  }

  private finishLines(i: number): void {
    const line = this.lines[i];
    if (line && !line.ledger && !line.amount.trim() && (this.lines.length > 1 || this.mode === "item")) {
      this.lines.splice(i, 1);
      this.render();
    }
    const problem = this.problem();
    if (problem) {
      this.shell.warn(problem);
      if (this.lines.length <= i) {
        this.lines.push(this.blankLine(i));
        this.render();
      }
      return this.go(this.indexOf(this.mode === "double" ? "side" : "ledger", Math.min(i, this.lines.length - 1)));
    }
    // An item invoice's party gets its bill-wise details once the total is known.
    if (this.mode === "item" && this.party?.is_party) return this.openPartyBills();
    this.go(this.fields().length - 1);
  }

  private problem(): string | null {
    const code = this.fxCode();
    if (code && this.fxCodes().length > 1) return t("One voucher can be in one foreign currency only.");
    if (code && this.fxUnits() === null) return t("Enter the rate of exchange for {currency}.", { currency: code });
    const badFx = this.lines.find((l) => this.isFx(l.ledger) && l.fx?.trim() && M.parse(l.fx) === null);
    if (badFx) return t('"{text}" is not an amount ({ledger}).', { text: ltr(badFx.fx!.trim()), ledger: badFx.ledger!.name });
    // A line with a ledger but an unreadable amount would otherwise be left out of the voucher silently.
    const bad = this.lines.find((l) => l.ledger && l.amount.trim() && M.parse(l.amount) === null);
    if (bad) return t('"{text}" is not an amount ({ledger}).', { text: ltr(bad.amount.trim()), ledger: bad.ledger!.name });
    const stale = this.lines.find(
      (l) => l.ledger && l.bills?.length && M.sum(l.bills.map((b) => M.of(b.amount))) !== this.amountOf(l),
    );
    if (stale) return t("Bill-wise details of {ledger} don't add up to its amount. Press Enter on the amount to fix them.", { ledger: stale.ledger!.name });
    const off = this.lines.find((l) => l.ledger && l.items?.length && M.sum(l.items.map(itemAmount)) !== this.amountOf(l));
    if (off)
      return t("The stock items on {ledger} add up to {amount}. Put that amount back, or press Ctrl+H to change the items.", {
        ledger: off.ledger!.name,
        amount: ltr(M.format(M.sum(off.items!.map(itemAmount)))),
      });
    if (this.mode === "item") return this.itemProblem();
    if (this.mode === "single" && !this.account) return "Pick the cash or bank account first.";
    const complete = this.lines.filter((l) => l.ledger && this.amountOf(l) > 0n);
    if (this.mode === "single") return complete.length ? null : "Add at least one particulars line.";
    if (complete.length < 2) return "A voucher needs at least one debit and one credit line.";
    const [dr, cr] = this.totals();
    if (dr !== cr) return t("Debit {debit} and credit {credit} don't match yet.", { debit: M.format(dr), credit: M.format(cr) });
    if (!complete.some((l) => l.side === "Dr") || !complete.some((l) => l.side === "Cr"))
      return "A voucher needs at least one debit and one credit line.";
    return null;
  }

  private itemProblem(): string | null {
    if (!this.party) return "Pick the party ledger first.";
    if (!this.salesLedger) return pickTradeLedger(this.kind);
    if (this.salesLedger.id === this.party.id)
      return tradeNoun(this.kind) === "sales"
        ? t("The party and the sales ledger must be different ledgers.")
        : t("The party and the purchase ledger must be different ledgers.");
    const open = this.items.find((l) => l.item && !itemDone(l));
    if (open) return t("Enter the quantity of {item}.", { item: open.item!.name });
    if (!this.items.some(itemDone)) return "Add at least one stock item.";
    if (this.itemsTotal() <= 0n) return "The items add up to zero. Enter their rates or amounts.";
    const total = this.invoiceTotal();
    if (total <= 0n) return "The invoice total must be more than zero.";
    const bills = this.partyBills;
    if (bills?.length && M.sum(bills.map((b) => M.of(b.amount))) !== total)
      return t("Bill-wise details of {party} don't add up to the invoice total. Go to End of List to fix them.", { party: this.party.name });
    return null;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.go(0);
  }

  private async escape(): Promise<void> {
    const f = this.field();
    const inp = this.inputFor(f);
    if ((f.k === "amount" || f.k === "narration" || f.k === "qty" || f.k === "rate" || f.k === "value" || f.k === "batch") && inp.value !== "") {
      inp.value = ""; // F4
      if (f.k === "amount") {
        this.lines[f.i].amount = "";
        delete this.lines[f.i].auto;
      }
      if (f.k === "qty") this.items[f.i].qty = "";
      if (f.k === "rate") this.items[f.i].rate = "";
      if (f.k === "value") this.items[f.i].amount = "";
      if (f.k === "batch") this.items[f.i].batch = "";
      this.paintLive();
      if (f.k === "qty" || f.k === "rate" || f.k === "value") this.scheduleTax();
      return;
    }
    if (this.dirty() && !(await this.shell.ask("Quit without saving?"))) return;
    this.shell.pop();
  }

  private removeLine(): void {
    const f = this.field();
    if (!("i" in f)) return;
    if (ITEM_FIELDS.includes(f.k)) {
      this.items.splice(f.i, 1);
      if (this.items.length === 0) this.items.push(blankItem());
      this.render();
      this.go(this.indexOf("item", Math.min(f.i, this.items.length - 1)));
      return this.scheduleTax();
    }
    // A removed auto tax line stays away until the item lines change.
    const auto = this.lines[f.i]?.auto;
    if (auto?.tcs) {
      // Off the party's debit it was added to; it stays away until the sales ledger, party, amount or date change.
      this.tcsDismissed = this.tcsKey;
      this.adjustTcsTarget(-this.amountOf(this.lines[f.i]));
      this.shell.info(t("TCS removed. It comes back if the amount, party or date change."));
    } else if (auto?.tds) {
      // Back on the line it was cut from; it stays away until the expense, party, amount or date change.
      this.tdsDismissed = this.tdsKey;
      this.adjustTdsTarget(this.amountOf(this.lines[f.i]));
      this.shell.info(t("TDS removed. It comes back if the amount, party or date change."));
    } else if (auto) {
      this.dismissed.add(auto.key);
      this.dismissedFor = this.taxKeyOfItems();
      const component = ltr(auto.component);
      this.shell.info(
        this.mode === "item"
          ? t("{component} removed. It comes back if the items change.", { component })
          : tradeNoun(this.kind) === "sales"
            ? t("{component} removed. It comes back if the sales lines change.", { component })
            : t("{component} removed. It comes back if the purchase lines change.", { component }),
      );
    }
    const [removed] = this.lines.splice(f.i, 1);
    if (this.lines.length === 0 && this.mode !== "item") this.lines.push(this.blankLine(0));
    // As voucher, a sales line or the party going changes the tax; a tax line going doesn't.
    if (this.mode === "double" && removed && (this.taxable(removed) || removed.side === this.partySide())) this.scheduleTax();
    if (!withheld(auto)) this.scheduleTds();
    this.render();
    if (!this.lines.length) return this.go(this.fields().length - 1);
    this.go(this.indexOf("ledger", Math.min(f.i, this.lines.length - 1)));
  }

  private async changeDate(): Promise<void> {
    const v = await this.shell.dialog("Voucher date", [{ label: "Date", value: D.editable(this.date) }], ([text]) =>
      D.parseUser(text, this.date) ? null : t('"{text}" is not a date. Try {example}.', { text: ltr(text), example: ltr("19-10-2026") }),
    );
    if (!v) return;
    const was = this.date;
    this.date = D.parseUser(v[0], this.date)!;
    // A new voucher's number follows its date (series can restart each year);
    // a saved voucher keeps its number.
    if (!this.alterId && this.date !== was) {
      try {
        this.number = await api.nextVoucherNumber(this.kind, this.date);
      } catch (e) {
        this.shell.error(e);
      }
    }
    this.render();
    this.go(this.at);
    if (this.date !== was) {
      this.scheduleTax(); // rates can change by date
      this.scheduleTds(); // and so can the year's TDS totals (and TCS)
    }
  }

  private async switchKind(kind: VoucherKind): Promise<void> {
    if (kind === this.kind) return;
    if (this.dirty() && !(await this.shell.ask(t("Discard this voucher and switch to {kind}?", { kind: kindName(kind) })))) return;
    try {
      this.number = await api.nextVoucherNumber(kind, this.date);
    } catch (e) {
      return this.shell.error(e);
    }
    this.kind = kind;
    this.mode = SINGLE_OK.includes(kind) ? "single" : this.hasItems && TRADING.includes(kind) ? "item" : "double";
    this.account = null;
    this.reset();
    this.render();
    this.go(0);
  }

  /** Alt+F7: the Stock Journal has its own screen, which takes this one's place. */
  private async openStockJournal(): Promise<void> {
    if (this.dirty() && !(await this.shell.ask(t("Discard this voucher and switch to {kind}?", { kind: kindName("StockJournal") })))) return;
    this.shell.pop();
    void this.shell.push(new StockJournalScreen(this.shell, undefined, this.date));
  }

  /** Ctrl+H. `quiet` converts while loading a saved voucher, without messages. */
  private toggleMode(quiet = false): void {
    if (TRADING.includes(this.kind)) return this.toggleItemMode(quiet);
    if (!SINGLE_OK.includes(this.kind)) {
      if (quiet) return;
      this.shell.warn(t("{kind} vouchers use double entry only.", { kind: kindName(this.kind) }));
      return;
    }
    const complete = this.lines.filter((l) => l.ledger);
    if (this.mode === "single") {
      const lines: Line[] = complete.map((l) => ({ ...l }));
      if (this.account) {
        const total = M.sum(complete.map((l) => this.amountOf(l)));
        lines.push({ side: other(particularSide(this.kind)), ledger: this.account, amount: total > 0n ? M.toDecimal(total) : "" });
      }
      this.mode = "double";
      this.lines = lines.length ? lines : [this.blankLine(0)];
      this.account = null;
    } else {
      const acctSide = other(particularSide(this.kind));
      const accounts = complete.filter((l) => l.side === acctSide);
      const ok = accounts.length <= 1 && (accounts.length === 0 || accounts[0].ledger!.is_cash_or_bank);
      if (!ok) {
        if (quiet) return;
        this.shell.warn("This voucher has more than one account line; it can't be shown in single entry.");
        return;
      }
      this.mode = "single";
      this.account = accounts[0]?.ledger ?? null;
      const rest = complete.filter((l) => l.side !== acctSide).map((l) => ({ ...l }));
      this.lines = rest.length ? rest : [this.blankLine(0)];
    }
    this.at = 0;
    if (quiet) return;
    this.tdsAdopt = true;
    this.tcsAdopt = true;
    this.scheduleTds();
    this.render();
    this.go(0);
  }

  /** Ctrl+H on a trading voucher: item invoice ⇄ as voucher (double entry). */
  private toggleItemMode(quiet = false): void {
    const partySide = this.partySide();
    const salesSide = this.salesSide();
    if (this.mode === "item") {
      const lines: Line[] = this.entries().map((e) => ({
        side: e.side,
        ledger: e.ledger,
        amount: M.toDecimal(e.amount),
        bills: e.bills,
        items: e.items?.map((l) => ({ ...l })),
      }));
      this.mode = "double";
      this.party = null;
      this.partyBills = undefined;
      this.salesLedger = null;
      this.items = [blankItem()];
      this.lines = lines.length ? lines : [this.blankLine(0)];
      this.stopTax();
      if (this.taxLines().length) {
        // The tax lines carry on following the sales lines as voucher.
        this.taxOn = true;
        this.adopt = true;
        this.scheduleTax();
      }
    } else {
      const complete = this.lines.filter((l) => l.ledger);
      const stocked = complete.filter((l) => l.items?.length);
      if (stocked.length > 1 || stocked.some((l) => l.side !== salesSide)) {
        if (!quiet) this.shell.warn("This voucher has stock items on more than one line, so it can't be shown as an item invoice.");
        return;
      }
      const partyLine = complete.find((l) => l.side === partySide && !l.items?.length) ?? null;
      let salesLine: Line | null = stocked[0] ?? null;
      if (quiet && (!salesLine || !partyLine)) return;
      if (!salesLine) {
        // No items yet: the first sales or purchase line becomes the ledger the items go to.
        salesLine = complete.find((l) => l.side === salesSide) ?? null;
        if (salesLine && this.amountOf(salesLine) > 0n)
          this.shell.info(t("Enter the items for {ledger}; their total replaces its amount.", { ledger: salesLine.ledger!.name }));
      }
      this.stopTax();
      this.mode = "item";
      this.party = partyLine?.ledger ?? null;
      this.partyBills = partyLine?.bills;
      this.salesLedger = salesLine?.ledger ?? null;
      this.items = salesLine?.items?.map((l) => ({ ...l })) ?? [];
      if (!this.items.length) this.items.push(blankItem());
      this.lines = complete.filter((l) => l !== partyLine && l !== salesLine).map((l) => ({ ...l }));
      if (!quiet && this.lines.length) {
        // Tax lines already there that match the calculation become auto lines.
        this.taxOn = true;
        this.adopt = true;
        this.scheduleTax();
      }
    }
    this.at = 0;
    if (quiet) return;
    // A TDS or TCS line already there that matches the suggestion becomes the auto line again.
    this.tdsAdopt = true;
    this.tcsAdopt = true;
    this.scheduleTds();
    this.render();
    this.go(0);
  }

  /** Ctrl+I: show or hide the Batch column on the item lines. */
  private toggleBatch(): void {
    if (this.mode !== "item") return;
    if (this.batchOn && this.items.some((l) => l.batch.trim())) {
      this.shell.warn("Some item lines have a batch. Clear them to hide the Batch column.");
      return;
    }
    // Note the field before the columns change, so the cursor stays on it.
    const before = this.fields()[this.at];
    this.batchOn = !this.batchOn;
    this.renderKeep(before);
  }

  private async save(): Promise<void> {
    await this.flushTax();
    await this.flushTds();
    await this.flushTcs();
    const problem = this.problem();
    if (problem) return this.shell.warn(problem);
    const entries: EntryInput[] = this.entries().map((e) => ({
      ledger_id: e.ledger.id,
      side: e.side,
      amount: M.toDecimal(e.amount),
      forex: this.forexOf(e) ?? null,
      bills: (e.bills ?? []).map((b) => ({ kind: b.kind, reference: b.reference, amount: b.amount, due: b.due || null })),
      items: (e.items ?? []).map((l) => {
        const s: StockLineInput = {
          item_id: l.item!.id,
          qty: M.toDecimal(M.parse(l.qty) ?? M.ZERO),
          rate: M.toDecimal(M.parse(l.rate) ?? M.ZERO),
          amount: M.toDecimal(itemAmount(l)),
        };
        if (l.godownId) s.godown_id = l.godownId;
        if (l.batch.trim()) s.batch = l.batch.trim();
        return s;
      }),
    }));
    const input = { kind: this.kind, date: this.date, number: this.number, entries, narration: this.narration.value };
    const fromBank = this.kind === "Payment" && this.entries().some((e) => e.side === "Cr" && isBank(e.ledger));
    try {
      if (this.alterId) {
        await api.alterVoucher(this.alterId, input);
        await this.shell.refreshCompany();
        this.shell.info(t("{kind} No. {number} altered.", { kind: kindName(this.kind), number: ltr(this.number) }));
        this.shell.pop(); // back to the report, which reloads with its cursor kept
        return;
      }
      const posted = await api.postVoucher(input);
      this.shell.info(t("{kind} No. {number} saved.", { kind: kindName(posted.kind), number: ltr(posted.number) }));
      await this.shell.refreshCompany();
      this.rates.clear(); // this voucher's rates are now the last ones
      this.taxLedgers.clear(); // balances moved
      // F7: a blank voucher of the same type; the account (or sales ledger) carries over.
      this.number = posted.next_number;
      if (this.account) {
        const fresh = await api.listLedgers(this.account.name, "cash_bank");
        this.account = fresh.find((l) => l.id === this.account!.id) ?? null;
      }
      let sales = this.mode === "item" ? this.salesLedger : null;
      if (sales) sales = (await api.listLedgers(sales.name, "all")).find((l) => l.id === sales!.id) ?? null;
      this.reset();
      this.salesLedger = sales;
      this.render();
      this.go(this.mode === "single" && this.account ? 1 : 0);
      if (PRINTABLE.includes(posted.kind) && (await this.shell.ask("Print?", { yesFirst: false }))) await this.print(posted.id);
      else if (fromBank && (await this.shell.ask("Print cheque?", { yesFirst: false }))) await this.printCheque(posted.id);
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** Print a saved voucher (tax invoice, credit or debit note, or plain voucher). */
  private async print(id: string): Promise<void> {
    try {
      // Hijri dates on: the voucher's saved date, in Hijri, prints beside the Gregorian one.
      const hijri = hijriOn() ? D.hijri((await api.getVoucher(id)).date, "ar") : null;
      await api.printVoucher(id, language(), hijri);
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** Ctrl+P on a bank payment: a cheque or the voucher itself. */
  private async printPayment(id: string): Promise<void> {
    // Loaded when needed: a static import would close an import cycle.
    const { choose } = await import("./sharing");
    const what = await choose(
      this.shell,
      "Print",
      [
        { key: "cheque", label: "Cheque", hint: "the payment on a cheque leaf" },
        { key: "voucher", label: "Voucher", hint: "the payment voucher" },
      ],
      "cheque",
    );
    if (what === "cheque") await this.printCheque(id);
    else if (what === "voucher") await this.print(id);
  }

  /** Print a saved bank payment as a cheque, crossed "A/c Payee" unless told otherwise. Esc prints nothing. */
  private async printCheque(id: string): Promise<void> {
    const { choose } = await import("./sharing");
    const crossed = await choose(
      this.shell,
      "Crossed A/c Payee?",
      [
        { key: "yes", label: "Yes", hint: "pays only into the payee's account" },
        { key: "no", label: "No", hint: "an open cheque" },
      ],
      "yes",
    );
    if (!crossed) return;
    try {
      await api.printCheque(id, crossed === "yes");
    } catch (e) {
      this.shell.error(e);
    }
  }

  // -------------------------------------------------------------- tax

  /** Automatic tax applies: an item invoice, or a trading voucher as voucher. */
  private taxMode(): boolean {
    return this.mode === "item" || (this.mode === "double" && TRADING.includes(this.kind));
  }

  /** As voucher: a sales or purchase side line taxed by its items or its ledger's HSN/SAC. */
  private taxable(l: Line): boolean {
    return Boolean(l.ledger && !l.auto && l.side === this.salesSide() && (l.items?.length || l.ledger.tax_class));
  }

  /** The party tax is worked out for: as voucher, the first party (else other) line on the party's side. */
  private taxParty(): Ledger | null {
    if (this.mode === "item") return this.party;
    const mine = this.lines.filter((l) => l.ledger && !l.auto && l.side === this.partySide());
    return (mine.find((l) => l.ledger!.is_party) ?? mine.find((l) => !l.ledger!.tax_component))?.ledger ?? null;
  }

  /**
   * Lines the tax is worked out on, each amount its taxable value: the item
   * lines, or as voucher the stock items or HSN/SAC ledger of each sales line.
   */
  private taxLines(): InvoiceTaxInput["lines"] {
    const items = (list: ItemLine[]): InvoiceTaxInput["lines"] =>
      list.filter((l) => itemDone(l) && itemAmount(l) > 0n).map((l) => ({ item_id: l.item!.id, taxable_value: M.toDecimal(itemAmount(l)) }));
    if (this.mode === "item") return items(this.items);
    if (!this.taxMode()) return [];
    return this.lines
      .filter((l) => this.taxable(l) && this.amountOf(l) > 0n)
      .flatMap((l): InvoiceTaxInput["lines"] => (l.items?.length ? items(l.items) : [{ ledger_id: l.ledger!.id, taxable_value: M.toDecimal(this.amountOf(l)) }]));
  }

  private taxKeyOfItems(): string {
    return JSON.stringify(this.taxLines());
  }

  /** Recalculate tax shortly after a change to the items, party or date. */
  private scheduleTax(): void {
    this.scheduleTds();
    if (!this.taxOn || !this.taxMode() || this.cancelled) return;
    clearTimeout(this.taxTimer);
    this.taxTimer = window.setTimeout(() => {
      this.taxTimer = undefined;
      void this.refreshTax();
    }, TAX_DELAY_MS);
  }

  /** Run a pending recalculation now and wait for any in flight (before totals are used). */
  private async flushTax(): Promise<void> {
    if (this.taxTimer !== undefined) {
      clearTimeout(this.taxTimer);
      this.taxTimer = undefined;
      await this.refreshTax();
    } else if (this.taxRun) await this.taxRun;
  }

  /**
   * Ask for the invoice's tax and fill the auto lines. `render` redraws when
   * the lines change; `adoptOnly` (a saved voucher just opened) only takes
   * over matching lines and adds none.
   */
  private refreshTax(render = true, adoptOnly = false): Promise<void> {
    const run = this.computeTax(render, adoptOnly).finally(() => {
      if (this.taxRun === run) this.taxRun = null;
    });
    this.taxRun = run;
    return run;
  }

  private async computeTax(render: boolean, adoptOnly: boolean): Promise<void> {
    const party = this.taxParty();
    if (!this.taxOn || !this.taxMode() || this.cancelled || !party) return;
    const seq = ++this.taxSeq;
    const input: InvoiceTaxInput = { kind: this.kind, date: this.date, party_id: party.id, lines: this.taxLines() };
    const key = JSON.stringify(input);
    let res: InvoiceTax;
    if (!input.lines.length) res = { place_of_supply: null, rows: [], warnings: [] };
    else if (key === this.lastTaxKey && this.lastTax) res = this.lastTax;
    else {
      try {
        res = await api.invoiceTax(input);
      } catch {
        if (seq === this.taxSeq) this.taxFailed(render);
        return;
      }
      if (seq !== this.taxSeq) return;
      this.lastTaxKey = key;
      this.lastTax = res;
    }
    const unknown = [...new Set(res.rows.map((r) => r.ledger_id).filter((id): id is string => !!id && !this.taxLedgers.has(id)))];
    await Promise.all(unknown.map((id) => api.getLedger(id).then((l) => void this.taxLedgers.set(id, l), () => undefined)));
    if (seq !== this.taxSeq || !this.taxMode()) return;
    this.applyTax(res, render, adoptOnly);
  }

  /** No tax calculation (no registration or country yet): manual tax lines, as before. */
  private taxFailed(render: boolean): void {
    if (!this.taxHinted) {
      this.taxHinted = true;
      const company = this.shell.km.chordIn("CompanyMenu", ["Global"]) ?? "Alt+K";
      const country = this.shell.company?.country;
      this.shell.info(
        country
          ? t("Set the company's {taxId} under Company ({key}) → Tax registration to fill tax automatically.", { taxId: ltr(country.tax_id_label), key: ltr(company) })
          : t("Pick the company's country and tax registration under Company ({key}) to fill tax automatically.", { key: ltr(company) }),
      );
    }
    // Drop what an earlier calculation filled in rather than leave it stale.
    this.applyTax({ place_of_supply: null, rows: [], warnings: [] }, render, false);
  }

  private applyTax(res: InvoiceTax, render: boolean, adoptOnly: boolean): void {
    const held = this.holdField();
    const items = this.taxKeyOfItems();
    if (items !== this.dismissedFor) {
      this.dismissed.clear();
      this.dismissedFor = items;
    }
    const before = this.lines;
    // TDS and TCS lines are not tax on the invoice: they stay as they are.
    const old = new Map(before.filter((l) => l.auto && !withheld(l.auto)).map((l) => [l.auto!.key, l]));
    const rest = before.filter((l) => !l.auto || withheld(l.auto));
    const fresh: Line[] = [];
    const missing: InvoiceTaxRow[] = [];
    for (const row of res.rows) {
      const amount = M.parse(row.amount) ?? M.ZERO;
      if (amount === 0n) continue;
      if (!row.ledger_id) {
        missing.push(row);
        continue;
      }
      const auto: AutoTax = { key: `${row.ledger_id}|${row.component}|${row.percent}`, component: row.component, percent: row.percent };
      const side = amount < 0n ? this.partySide() : this.salesSide();
      const value = M.abs(amount);
      // A line of the user's own for this ledger wins. A loaded or converted
      // one with the same amount is taken over, so it follows later changes.
      const mine = rest.find((l) => !l.auto && l.ledger?.id === row.ledger_id);
      if (mine) {
        if (this.adopt && mine.side === side && this.amountOf(mine) === value) mine.auto = auto;
        continue;
      }
      if (adoptOnly || this.dismissed.has(auto.key)) continue;
      const ledger = this.taxLedgers.get(row.ledger_id);
      if (!ledger) continue;
      const line = old.get(auto.key) ?? { side, ledger, amount: "" };
      Object.assign(line, { side, ledger, amount: M.toDecimal(value), auto });
      fresh.push(line);
    }
    this.adopt = false;
    // Auto lines go where they were, else before the blank line at the end.
    const first = before.findIndex((l) => !rest.includes(l));
    let at = first >= 0 ? before.slice(0, first).filter((l) => rest.includes(l)).length : rest.length;
    if (first < 0) while (at > 0 && !rest[at - 1].ledger && !rest[at - 1].amount.trim()) at--;
    // New tax lines go before the TDS or TCS line at the end (TCS is worked out on them).
    if (first < 0) while (at > 0 && withheld(rest[at - 1].auto)) at--;
    const lines = [...rest.slice(0, at), ...fresh, ...rest.slice(at)];
    const missingWas = JSON.stringify(this.taxMissing);
    this.taxMissing = adoptOnly ? [] : missing;
    const warnings = [...new Set(res.warnings)].join(" ");
    if (warnings && warnings !== this.taxWarned && !adoptOnly) this.shell.warn(warnings);
    this.taxWarned = warnings;
    const same = lines.length === before.length && lines.every((l, i) => l === before[i]) && missingWas === JSON.stringify(this.taxMissing);
    this.lines = lines;
    // TCS is worked out on what the buyer pays, GST included.
    if (!adoptOnly) this.scheduleTcs();
    if (!render) return;
    if (!same) return this.renderKeep(held.f, held.line);
    // Only amounts moved: update them in place, leaving the cursor be.
    this.lines.forEach((l, i) => {
      const inp = this.inputs.get(`amount:${i}`);
      if (!l.auto || !inp) return;
      const focused = document.activeElement === inp;
      inp.value = this.amountText(l, focused);
      if (focused) inp.select();
    });
    this.paintLive();
  }

  // -------------------------------------------------------------- TDS

  /** TDS applies: an Indian company's Journal, Purchase or Payment. */
  private tdsMode(): boolean {
    return this.shell.company?.country?.code === "IN" && TDS_KINDS.includes(this.kind) && !this.cancelled;
  }

  /** The deductee: the item invoice's party, else the first party (or PAN) line. */
  private tdsParty(): Ledger | null {
    if (this.mode === "item") return this.party;
    return this.lines.find((l) => l.ledger && !l.auto && isDeductee(l.ledger))?.ledger ?? null;
  }

  /**
   * What TDS is worked out on: the expense ledger with a nature of payment
   * (or, without one, the party's), and the net debit of the lines of that
   * nature. Party, cash, bank and duty lines never count.
   */
  private tdsBase(party: Ledger | null): { ledger: Ledger; amount: M.Units } | null {
    const lines: { ledger: Ledger; side: Side; amount: M.Units }[] = [];
    if (this.mode === "item" && this.salesLedger) lines.push({ ledger: this.salesLedger, side: this.salesSide(), amount: this.itemsTotal() });
    for (const l of this.lines) if (l.ledger && !l.auto) lines.push({ ledger: l.ledger, side: l.side, amount: this.amountOf(l) });
    const fallback = natureOf(party);
    const by = new Map<string, { ledger: Ledger; amount: M.Units; top: M.Units }>();
    for (const x of lines) {
      const l = x.ledger;
      if (l.id === party?.id || isDeductee(l) || l.is_cash_or_bank || l.tax_component || l.tds?.kind === "Payable") continue;
      const nature = natureOf(l) ?? fallback;
      if (!nature) continue;
      const signed = x.side === "Dr" ? x.amount : -x.amount;
      const cur = by.get(nature);
      if (!cur) by.set(nature, { ledger: l, amount: signed, top: signed });
      else {
        cur.amount += signed;
        if (signed > cur.top) Object.assign(cur, { ledger: l, top: signed });
      }
    }
    const best = [...by.values()].filter((b) => b.amount > 0n).sort((a, b) => (a.amount > b.amount ? -1 : a.amount < b.amount ? 1 : 0))[0];
    return best ? { ledger: best.ledger, amount: best.amount } : null;
  }

  /** The side TDS is credited on: the party's side of an item invoice, else Cr. */
  private tdsSide(): Side {
    return this.mode === "item" ? this.partySide() : "Cr";
  }

  /**
   * The line the TDS is cut from, as voucher: the party's credit, else a
   * cash or bank credit (an expense paid without a party ledger). An item
   * invoice's party total follows by itself.
   */
  private tdsTarget(withAmount = true): Line | null {
    if (this.mode !== "double") return null;
    const ok = (l: Line) => l.ledger && !l.auto && l.side === "Cr" && (!withAmount || this.amountOf(l) > 0n);
    const party = this.tdsParty();
    return this.lines.find((l) => ok(l) && l.ledger!.id === party?.id) ?? this.lines.find((l) => ok(l) && l.ledger!.is_cash_or_bank) ?? null;
  }

  /** Move the TDS cut on its line by `delta` (positive gives back), and a single bill with it. */
  private adjustTdsTarget(delta: M.Units): void {
    if (delta === 0n) return;
    const line = this.tdsTarget();
    if (!line) return;
    const was = this.amountOf(line);
    const now = was + delta;
    if (now <= 0n) return;
    line.amount = M.toDecimal(now);
    const bills = line.bills;
    if (bills?.length === 1 && M.of(bills[0].amount) === was) bills[0] = { ...bills[0], amount: M.toDecimal(now) };
  }

  /** Ask for TDS shortly after a change to the expense, party, amount or date (and TCS, on its own kinds). */
  private scheduleTds(): void {
    this.scheduleTcs();
    if (!this.tdsMode()) return;
    clearTimeout(this.tdsTimer);
    this.tdsTimer = window.setTimeout(() => {
      this.tdsTimer = undefined;
      void this.refreshTds();
    }, TDS_DELAY_MS);
  }

  /** Run a pending suggestion now and wait for any in flight (before totals are used). */
  private async flushTds(): Promise<void> {
    if (this.tdsTimer !== undefined) {
      clearTimeout(this.tdsTimer);
      this.tdsTimer = undefined;
      await this.refreshTds();
    } else if (this.tdsRun) await this.tdsRun;
  }

  /** Ask for the suggestion and fill (or remove) the auto TDS line. `adoptOnly`: a saved voucher just opened. */
  private refreshTds(render = true, adoptOnly = false): Promise<void> {
    const run = this.computeTds(render, adoptOnly).finally(() => {
      if (this.tdsRun === run) this.tdsRun = null;
    });
    this.tdsRun = run;
    return run;
  }

  private async computeTds(render: boolean, adoptOnly: boolean): Promise<void> {
    if (!this.tdsMode()) return;
    const seq = ++this.tdsSeq;
    const party = this.tdsParty();
    const base = this.tdsBase(party);
    // Without a party ledger, only once the expense is paid from cash or bank.
    const ready =
      Boolean(party) ||
      (this.mode === "single" && Boolean(this.account)) ||
      this.lines.some((l) => l.ledger && !l.auto && l.side === "Cr" && l.ledger.is_cash_or_bank);
    if (!base || !ready) return this.applyTds(null, "", null, "", render, adoptOnly);
    const amount = M.toDecimal(base.amount);
    const key = JSON.stringify([this.date, party?.id ?? "", base.ledger.id, amount, this.alterId]);
    let s: TdsSuggestion;
    if (key === this.tdsKey && this.tdsLast) s = this.tdsLast;
    else {
      try {
        s = await tdsApi.suggest(this.date, party?.id ?? "", base.ledger.id, amount, this.alterId);
      } catch (e) {
        if (seq !== this.tdsSeq) return;
        const why = e instanceof Error ? e.message : String(e);
        if (why !== this.tdsShown) this.shell.warn(t("No TDS worked out: {why}", { why: ltr(why) }));
        this.tdsShown = why;
        return this.applyTds(null, key, null, "", render, adoptOnly);
      }
      if (seq !== this.tdsSeq) return;
      this.tdsKey = key;
      this.tdsLast = s;
    }
    const id = s.applies ? s.payable_ledger_id : null;
    if (id && !this.taxLedgers.has(id)) {
      const l = await api.getLedger(id).catch(() => null);
      if (l) this.taxLedgers.set(id, l);
    }
    const nature = (await tdsNatures()).find((n) => n.code === s.nature);
    if (seq !== this.tdsSeq || !this.tdsMode()) return;
    this.applyTds(s, key, id ? this.taxLedgers.get(id) ?? null : null, nature?.old_section ?? s.section ?? "", render, adoptOnly);
  }

  private applyTds(s: TdsSuggestion | null, key: string, payable: Ledger | null, section: string, render: boolean, adoptOnly: boolean): void {
    const held = this.holdField();
    const before = [...this.lines];
    const amounts = before.map((l) => l.amount);
    const cur = this.lines.find((l) => l.auto?.tds) ?? null;
    const side = this.tdsSide();
    let want = s?.applies && payable && this.mode !== "single" && this.tdsDismissed !== key ? M.of(s.amount) : M.ZERO;
    const tag: AutoTax | null = s && payable ? { key: `tds|${payable.id}`, component: `TDS ${section}`.trim(), percent: s.rate ?? "0", tds: true } : null;
    // A line of the user's own on the payable ledger wins; a loaded or converted one with the same amount is taken over.
    const own = payable ? this.lines.find((l) => !l.auto && l.ledger?.id === payable.id) : undefined;
    if (own) {
      if (this.tdsAdopt && tag && want > 0n && own.side === side && this.amountOf(own) === want) own.auto = tag;
      want = M.ZERO;
    }
    this.tdsAdopt = false;
    if (!adoptOnly) {
      const had = cur ? this.amountOf(cur) : M.ZERO;
      if (want === 0n && cur) {
        this.lines.splice(this.lines.indexOf(cur), 1);
        this.adjustTdsTarget(had);
      } else if (want > 0n && payable && tag) {
        if (cur) Object.assign(cur, { side, ledger: payable, amount: M.toDecimal(want), auto: tag });
        else {
          // Right after the party's line, else before the blank line at the end.
          const target = this.tdsTarget(false);
          let at = target ? this.lines.indexOf(target) + 1 : this.lines.length;
          if (!target) while (at > 0 && !this.lines[at - 1].ledger && !this.lines[at - 1].amount.trim()) at--;
          this.lines.splice(at, 0, { side, ledger: payable, amount: M.toDecimal(want), auto: tag });
        }
        this.adjustTdsTarget(had - want);
      }
    }
    // The explanation and any warnings, once each; nothing for a voucher just opened.
    const note = s ? [s.explanation, ...s.warnings].filter(Boolean).join(" ") : "";
    const worth = s && (s.applies || s.warnings.length > 0 || cur !== null);
    if (note && worth && note !== this.tdsShown && !adoptOnly) {
      const hint = this.mode === "single" && s.applies ? ` ${t("Press Ctrl+H for double entry to deduct it.")}` : "";
      // The explanation comes from the core in English: one left-to-right piece in Arabic.
      const text = t("TDS: {note}", { note: ltr(note) }) + hint;
      // A non-resident payee: the rate asked for and the Form 145/146 reminder stand out.
      if (s.nature === "non_resident" && s.warnings.length) this.shell.warn(text);
      else this.shell.info(text, 8000);
    }
    if (note) this.tdsShown = note;
    const same = this.lines.length === before.length && this.lines.every((l, i) => l === before[i] && l.amount === amounts[i]);
    if (!render || same) return;
    this.renderKeep(held.f, held.line);
  }

  // -------------------------------------------------------------- TCS

  /** TCS applies: an Indian company's Sales or Receipt. */
  private tcsMode(): boolean {
    return this.shell.company?.country?.code === "IN" && TCS_KINDS.includes(this.kind) && !this.cancelled;
  }

  /** The collectee: the item invoice's party, else the first party (or PAN) line. */
  private tcsParty(): Ledger | null {
    if (this.mode === "item") return this.party;
    return this.lines.find((l) => l.ledger && !l.auto && isCollectee(l.ledger))?.ledger ?? null;
  }

  /**
   * What TCS is worked out on: the sales or income ledger with a nature of
   * collection (or, without one, the buyer's), and the net credit of the lines
   * of that nature plus the invoice's GST, which the buyer pays too
   * (docs/india-tds.md). Party, cash and bank lines never count.
   */
  private tcsBase(party: Ledger | null): { ledger: Ledger; amount: M.Units } | null {
    const lines: { ledger: Ledger; side: Side; amount: M.Units }[] = [];
    if (this.mode === "item" && this.salesLedger) lines.push({ ledger: this.salesLedger, side: this.salesSide(), amount: this.itemsTotal() });
    // GST auto lines count; TDS and TCS lines don't.
    for (const l of this.lines) if (l.ledger && !withheld(l.auto)) lines.push({ ledger: l.ledger, side: l.side, amount: this.amountOf(l) });
    const fallback = tcsNatureOf(party);
    const by = new Map<string, { ledger: Ledger; amount: M.Units; top: M.Units }>();
    let gst = M.ZERO;
    for (const x of lines) {
      const l = x.ledger;
      const signed = x.side === "Cr" ? x.amount : -x.amount;
      if (l.tax_component) {
        gst += signed;
        continue;
      }
      if (l.id === party?.id || isCollectee(l) || l.is_cash_or_bank || l.tcs?.kind === "Payable" || l.tds?.kind === "Payable") continue;
      const nature = tcsNatureOf(l) ?? fallback;
      if (!nature) continue;
      const cur = by.get(nature);
      if (!cur) by.set(nature, { ledger: l, amount: signed, top: signed });
      else {
        cur.amount += signed;
        if (signed > cur.top) Object.assign(cur, { ledger: l, top: signed });
      }
    }
    const best = [...by.values()].filter((b) => b.amount > 0n).sort((a, b) => (a.amount > b.amount ? -1 : a.amount < b.amount ? 1 : 0))[0];
    if (!best) return null;
    // Like the reports: the voucher's GST goes with its largest nature.
    return { ledger: best.ledger, amount: best.amount + (gst > 0n ? gst : M.ZERO) };
  }

  /**
   * The line TCS is added to, as voucher: the party's debit, else a cash or
   * bank debit (a cash sale). An item invoice's party total, and single
   * entry's account, follow by themselves.
   */
  private tcsTarget(withAmount = true): Line | null {
    if (this.mode !== "double") return null;
    const ok = (l: Line) => l.ledger && !l.auto && l.side === "Dr" && (!withAmount || this.amountOf(l) > 0n);
    const party = this.tcsParty();
    return this.lines.find((l) => ok(l) && l.ledger!.id === party?.id) ?? this.lines.find((l) => ok(l) && l.ledger!.is_cash_or_bank) ?? null;
  }

  /** Move the party's debit by `delta` (positive adds TCS), and a single bill with it. */
  private adjustTcsTarget(delta: M.Units): void {
    if (delta === 0n) return;
    const line = this.tcsTarget();
    if (!line) return;
    const was = this.amountOf(line);
    const now = was + delta;
    if (now <= 0n) return;
    line.amount = M.toDecimal(now);
    const bills = line.bills;
    if (bills?.length === 1 && M.of(bills[0].amount) === was) bills[0] = { ...bills[0], amount: M.toDecimal(now) };
  }

  /** Ask for TCS shortly after a change to the sales ledger, party, amount or date. */
  private scheduleTcs(): void {
    if (!this.tcsMode()) return;
    clearTimeout(this.tcsTimer);
    this.tcsTimer = window.setTimeout(() => {
      this.tcsTimer = undefined;
      void this.refreshTcs();
    }, TCS_DELAY_MS);
  }

  /** Run a pending suggestion now and wait for any in flight (before totals are used). */
  private async flushTcs(render = true): Promise<void> {
    if (this.tcsTimer !== undefined) {
      clearTimeout(this.tcsTimer);
      this.tcsTimer = undefined;
      await this.refreshTcs(render);
    } else if (this.tcsRun) await this.tcsRun;
  }

  /** Ask for the suggestion and fill (or remove) the auto TCS line. `adoptOnly`: a saved voucher just opened. */
  private refreshTcs(render = true, adoptOnly = false): Promise<void> {
    const run = this.computeTcs(render, adoptOnly).finally(() => {
      if (this.tcsRun === run) this.tcsRun = null;
    });
    this.tcsRun = run;
    return run;
  }

  private async computeTcs(render: boolean, adoptOnly: boolean): Promise<void> {
    // An item invoice gets its TCS once its items are done, with the tax lines.
    if (!this.tcsMode() || (this.mode === "item" && !this.taxOn)) return;
    const seq = ++this.tcsSeq;
    const party = this.tcsParty();
    const base = this.tcsBase(party);
    // Without a party ledger, only once the sale is paid in cash or bank.
    const ready =
      Boolean(party) ||
      (this.mode === "single" && Boolean(this.account)) ||
      this.lines.some((l) => l.ledger && !l.auto && l.side === "Dr" && l.ledger.is_cash_or_bank);
    if (!base || !ready) return this.applyTcs(null, "", null, render, adoptOnly);
    const amount = M.toDecimal(base.amount);
    const key = JSON.stringify([this.date, party?.id ?? "", base.ledger.id, amount]);
    let s: TcsSuggestion;
    if (key === this.tcsKey && this.tcsLast) s = this.tcsLast;
    else {
      try {
        s = await tcsApi.suggest(this.date, party?.id ?? "", base.ledger.id, amount);
      } catch (e) {
        if (seq !== this.tcsSeq) return;
        const why = e instanceof Error ? e.message : String(e);
        if (why !== this.tcsShown) this.shell.warn(t("No TCS worked out: {why}", { why: ltr(why) }));
        this.tcsShown = why;
        return this.applyTcs(null, key, null, render, adoptOnly);
      }
      if (seq !== this.tcsSeq) return;
      this.tcsKey = key;
      this.tcsLast = s;
    }
    const id = s.applies ? s.payable_ledger_id : null;
    if (id && !this.taxLedgers.has(id)) {
      const l = await api.getLedger(id).catch(() => null);
      if (l) this.taxLedgers.set(id, l);
    }
    if (seq !== this.tcsSeq || !this.tcsMode()) return;
    this.applyTcs(s, key, id ? this.taxLedgers.get(id) ?? null : null, render, adoptOnly);
  }

  private applyTcs(s: TcsSuggestion | null, key: string, payable: Ledger | null, render: boolean, adoptOnly: boolean): void {
    const held = this.holdField();
    const before = [...this.lines];
    const amounts = before.map((l) => l.amount);
    const cur = this.lines.find((l) => l.auto?.tcs) ?? null;
    // Credited, on the sales side: an item invoice's total and single entry's account grow by it.
    const side: Side = this.mode === "single" ? particularSide(this.kind) : "Cr";
    let want = s?.applies && payable && side === "Cr" && this.tcsDismissed !== key ? M.of(s.amount) : M.ZERO;
    const code = s?.tax_code || s?.nature || "";
    const tag: AutoTax | null = s && payable ? { key: `tcs|${payable.id}`, component: `TCS ${code}`.trim(), percent: s.rate ?? "0", tcs: true } : null;
    // A line of the user's own on the payable ledger wins; a loaded or converted one with the same amount is taken over.
    const own = payable ? this.lines.find((l) => !l.auto && l.ledger?.id === payable.id) : undefined;
    if (own) {
      if (this.tcsAdopt && tag && want > 0n && own.side === side && this.amountOf(own) === want) own.auto = tag;
      want = M.ZERO;
    }
    this.tcsAdopt = false;
    if (!adoptOnly) {
      const had = cur ? this.amountOf(cur) : M.ZERO;
      if (want === 0n && cur) {
        this.lines.splice(this.lines.indexOf(cur), 1);
        this.adjustTcsTarget(-had);
      } else if (want > 0n && payable && tag) {
        if (cur) Object.assign(cur, { side, ledger: payable, amount: M.toDecimal(want), auto: tag });
        else {
          // After the GST lines, before the blank line at the end.
          let at = this.lines.length;
          while (at > 0 && !this.lines[at - 1].ledger && !this.lines[at - 1].amount.trim()) at--;
          this.lines.splice(at, 0, { side, ledger: payable, amount: M.toDecimal(want), auto: tag });
        }
        this.adjustTcsTarget(want - had);
      }
    }
    // The explanation and any warnings, once each; nothing for a voucher just opened.
    const note = s ? [s.explanation, ...s.warnings].filter(Boolean).join(" ") : "";
    const worth = s && (s.applies || s.warnings.length > 0 || cur !== null);
    if (note && worth && note !== this.tcsShown && !adoptOnly) {
      // The explanation comes from the core in English: one left-to-right piece in Arabic.
      this.shell.info(t("TCS: {note}", { note: ltr(note) }), 8000);
    }
    if (note) this.tcsShown = note;
    const same = this.lines.length === before.length && this.lines.every((l, i) => l === before[i] && l.amount === amounts[i]);
    if (!render || same) return;
    this.renderKeep(held.f, held.line);
  }

  /**
   * The field being edited and its line, taken before a change to the lines
   * (an auto line added or removed before it) so renderKeep can follow it.
   */
  private holdField(): { f: Field; line?: Line } {
    const fields = this.fields();
    const f = fields[Math.min(this.at, fields.length - 1)];
    return "i" in f && !ITEM_FIELDS.includes(f.k) ? { f, line: this.lines[f.i] } : { f };
  }

  /** Redraw without disturbing the field being typed in (its text, selection and picker). */
  /** Re-render, keeping the cursor on `before` (default: the current field). */
  private renderKeep(before?: Field, heldLine?: Line): void {
    const fields = this.fields();
    const f = before ?? fields[Math.min(this.at, fields.length - 1)];
    const inp = this.inputFor(f);
    const focused = Boolean(inp) && document.activeElement === inp;
    const text = inp?.value ?? "";
    const [from, to] = [inp?.selectionStart ?? null, inp?.selectionEnd ?? null];
    const isItem = "i" in f && ITEM_FIELDS.includes(f.k);
    const line = "i" in f && !isItem ? (heldLine ?? this.lines[f.i]) : undefined;
    const item = "i" in f && isItem ? this.items[f.i] : undefined;
    this.render();
    const now = this.fields();
    const idx = now.findIndex(
      (x) => x.k === f.k && (!("i" in x) || (line ? this.lines[x.i] === line : item ? this.items[x.i] === item : true)),
    );
    this.at = idx >= 0 ? idx : Math.min(this.at, now.length - 1);
    if (!focused) return;
    if (idx < 0) return this.go(this.at); // that line went away
    const el = this.inputFor(now[idx]);
    el.focus();
    const list = f.k === "ledger" || f.k === "account" || f.k === "party" || f.k === "sales" || f.k === "item" || f.k === "godown";
    if (list && el.value !== text) {
      // Text typed to filter the list: put it back and let the picker filter again.
      el.value = text;
      el.dispatchEvent(new Event("input"));
    }
    if (from !== null && to !== null) el.setSelectionRange(from, to);
  }

  dispose(): void {
    this.picker?.close();
    clearTimeout(this.taxTimer);
    this.taxSeq++;
    clearTimeout(this.tdsTimer);
    this.tdsSeq++;
    clearTimeout(this.tcsTimer);
    this.tcsSeq++;
  }
}

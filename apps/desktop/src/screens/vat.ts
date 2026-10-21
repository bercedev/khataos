// UAE VAT 201 return: boxes worked out from the books (khata-ae), laid out
// like the GSTR-3B screen. Enter on a supplies or expenses box opens the
// Day Book for the period.
import "../gst.css";
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { Shell } from "../shell";
import type { Vat201Rc, VatBoxAmounts } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the calendar quarter: VAT periods are usually quarters. */
function quarterStart(iso: string): string {
  const m = Number(iso.slice(5, 7));
  const q = Math.floor((m - 1) / 3) * 3 + 1;
  return `${iso.slice(0, 4)}-${String(q).padStart(2, "0")}-01`;
}

function num(text: string): HTMLElement {
  return h("td.num", M.fmt(text, true));
}

function head(...cols: [string, boolean][]): HTMLElement {
  return h("thead", h("tr", ...cols.map(([c, n]) => h(n ? "th.num" : "th", t(c)))));
}

export class Vat201Report extends Report {
  title = "VAT 201";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render((await api.vat201(this.from, this.to)) as Vat201Rc);
  }

  private render(r: Vat201Rc): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("VAT 201"), `${r.trn ? t("TRN {number}", { number: r.trn }) : t("No TRN")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // `label` is English, shown through t().
    const row = (box: string, label: string, a: VatBoxAmounts | null, amountOnly?: string) =>
      this.pick(
        h("tr", h("td", h("span.muted", `${box}  `), t(label)), num(a ? a.amount : (amountOnly ?? "0")), a ? num(a.vat) : h("td.num", "")),
        dayBook,
      );
    const side = (title: string, table: HTMLElement) => h("div.tside", h("div.tside-title", t(title)), table);

    const sales = h("tbody");
    for (const e of r.standard_rated) sales.append(this.pick(h("tr", h("td", h("span.muted", `${e.box}  `), t("Standard-rated supplies in {emirate}", { emirate: t(e.name) })), num(e.amounts.amount), num(e.amounts.vat)), dayBook));
    sales.append(
      row("2", "Tax refunds to tourists", r.tourist_refunds),
      row("3", "Supplies subject to reverse charge", r.reverse_charge_supplies),
      row("4", "Zero-rated supplies", null, r.zero_rated),
      row("5", "Exempt supplies", null, r.exempt),
      row("6", "Goods imported into the UAE", r.imports),
      row("7", "Adjustments to goods imported", r.import_adjustments),
    );
    if (M.of(r.reverse_charge_sales) !== 0n)
      sales.append(
        this.pick(
          h("tr", h("td", h("span.muted", "–  "), t("Domestic reverse-charge supplies (in Box 1, no VAT)")), num(r.reverse_charge_sales), h("td.num", "")),
          dayBook,
        ),
      );
    const salesFoot = h("tfoot", h("tr", h("td", h("span.muted", "8  "), t("Totals")), num(r.total_supplies.amount), num(r.total_supplies.vat)));

    const expenses = h("tbody", row("9", "Standard-rated expenses", r.standard_expenses), row("10", "Supplies subject to reverse charge", r.reverse_charge_recovery));
    const expensesFoot = h("tfoot", h("tr", h("td", h("span.muted", "11  "), t("Totals")), num(r.total_expenses.amount), num(r.total_expenses.vat)));

    const net = M.of(r.net);
    const due = h(
      "tbody",
      this.pick(h("tr", h("td", h("span.muted", "12  "), t("Total value of due tax for the period")), num(r.total_due))),
      this.pick(h("tr", h("td", h("span.muted", "13  "), t("Total value of recoverable tax for the period")), num(r.total_recoverable))),
      this.pick(h("tr", h("td", h("span.muted", "14  "), t(net < 0n ? "Net VAT recoverable" : "Payable tax for the period")), h("td.num", { class: net < 0n ? "gst-credit" : "" }, M.fmt(net < 0n ? M.toDecimal(-net) : r.net)))),
    );

    const warn: HTMLElement | null =
      r.warnings.length > 0
        ? h(
            "div.gst-warnings",
            { role: "status" },
            h("div.gst-warn-title", r.warnings.length === 1 ? t("1 thing to check") : t("{n} things to check", { n: r.warnings.length })),
            h("ul", ...r.warnings.map((w) => h("li", { dir: "auto" }, w))),
          )
        : null;
    this.bodyEl.replaceChildren(
      ...[warn].filter((x): x is HTMLElement => x !== null),
      h(
        "div.gst-3b",
        side("VAT on sales and all other outputs", h("table.rtable", head(["Box", false], ["Amount (AED)", true], ["VAT (AED)", true]), sales, salesFoot)),
        side("VAT on expenses and all other inputs", h("table.rtable", head(["Box", false], ["Amount (AED)", true], ["Recoverable VAT (AED)", true]), expenses, expensesFoot)),
        side("Net VAT due", h("table.rtable", head(["Box", false], ["AED", true]), due)),
      ),
      h(
        "p.gst-note",
        t("Worked out from the books for checking before you file on EmaraTax. Every standard-rated sale is shown in the company's own emirate; all VAT on expenses, the reverse charge and imports is treated as recoverable; tourist refunds are not worked out. Mark reverse-charge purchases (services and goods from abroad, and domestic reverse-charge goods) with an RC: tax class such as RC:STANDARD, and goods imported through UAE Customs with IMPORT:STANDARD: their VAT is worked out at that rate for Boxes 3, 6 and 10. Box 6 is pre-filled by the FTA from the customs declarations: compare it. Enter opens the Day Book."),
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

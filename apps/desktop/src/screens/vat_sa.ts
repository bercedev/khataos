// Saudi Arabia VAT return: lines worked out from the books (khata-sa), laid
// out like the VAT 201 screen. The portal's line numbers were not checked, so
// lines are shown by name (docs/ksa-zatca.md). Enter on a line opens the Day
// Book for the period.
import "../gst.css";
import { saApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { Shell } from "../shell";
import type { KsaAmounts, KsaVatReturn } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the calendar quarter: most businesses file quarterly. */
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

export class KsaVatReturnReport extends Report {
  title = "VAT Return (Saudi Arabia)";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render(await saApi.vatReturn(this.from, this.to));
  }

  private render(r: KsaVatReturn): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    const vat = r.vat_number ? t("VAT No. {number}", { number: r.vat_number }) : t("No VAT number");
    this.heading(t("VAT Return (Saudi Arabia)"), `${vat} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // A line with its amount and VAT, or an amount only; `muted` marks a line that is not worked out.
    const row = (label: string, a: KsaAmounts | null, amountOnly?: string, muted = false) =>
      this.pick(
        h(
          "tr",
          h("td", t(label), muted ? h("span.muted", ` · ${t("not worked out")}`) : null),
          num(a ? a.amount : (amountOnly ?? "0")),
          a ? num(a.vat) : h("td.num", ""),
        ),
        dayBook,
      );
    const side = (title: string, table: HTMLElement) => h("div.tside", h("div.tside-title", t(title)), table);

    const sales = h(
      "tbody",
      row("Standard-rated sales", r.standard_rated_sales),
      row("Private healthcare and education to citizens", r.citizen_healthcare_education_sales, undefined, true),
      row("Sales to registered customers in other GCC states", r.gcc_registered_customer_sales, undefined, true),
      row("Zero-rated domestic sales", null, r.zero_rated_domestic_sales),
      row("Exports", null, r.exports),
      row("Exempt sales", null, r.exempt_sales),
    );
    const salesFoot = h("tfoot", h("tr", h("td", t("Total sales")), num(r.total_sales.amount), num(r.total_sales.vat)));

    const purchases = h(
      "tbody",
      row("Standard-rated domestic purchases", r.standard_rated_purchases),
      row("Imports subject to VAT paid at customs (field 8)", r.imports_paid_at_customs),
      row("Imports subject to VAT under the reverse charge (field 9)", r.imports_reverse_charge),
      row("Zero-rated purchases", null, r.zero_rated_purchases),
      row("Exempt purchases", null, r.exempt_purchases),
    );
    const purchasesFoot = h("tfoot", h("tr", h("td", t("Total purchases")), num(r.total_purchases.amount), num(r.total_purchases.vat)));

    const net = M.of(r.net);
    const due = h(
      "tbody",
      this.pick(h("tr", h("td", t("Total VAT due for the period")), num(r.total_vat_due))),
      this.pick(h("tr", h("td", t("Corrections from previous periods"), h("span.muted", ` · ${t("not worked out")}`)), num(r.corrections))),
      this.pick(h("tr", h("td", t("VAT credit carried forward"), h("span.muted", ` · ${t("not tracked")}`)), num(r.credit_carried_forward))),
      this.pick(
        h(
          "tr",
          h("td", t(net < 0n ? "Net VAT credit" : "Net VAT due")),
          h("td.num", { class: net < 0n ? "gst-credit" : "" }, M.fmt(net < 0n ? M.toDecimal(-net) : r.net)),
        ),
      ),
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
        side("VAT on sales", h("table.rtable", head(["Line", false], ["Amount (SAR)", true], ["VAT (SAR)", true]), sales, salesFoot)),
        side("VAT on purchases", h("table.rtable", head(["Line", false], ["Amount (SAR)", true], ["VAT (SAR)", true]), purchases, purchasesFoot)),
        side("Net VAT", h("table.rtable", head(["Line", false], ["SAR", true]), due)),
      ),
      h(
        "p.gst-note",
        t(
          "Worked out from the books for checking before you file with ZATCA. All VAT on purchases is treated as recoverable; purchases without the supplier's VAT number are left out. Services bought from abroad: give them an RC: tax class such as RC:STANDARD; field 9 shows them with the VAT worked out, which the return treats as both due and deductible. Goods imported through customs: give them an IMPORT: class (value in field 8) and post the customs declaration's VAT to a VAT ledger classed IMPORT. Corrections and carried-forward credit are not worked out. Enter opens the Day Book.",
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

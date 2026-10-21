// Singapore GST F5 return: boxes worked out from the books (khata-sg), laid
// out like the VAT 201 screen. Enter on a box opens the Day Book for the period.
import "../gst.css";
import { sgApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { Shell } from "../shell";
import type { GstF5, GstYesNoBox } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the calendar quarter: GST periods are usually quarters. */
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

export class GstF5Report extends Report {
  title = "GST F5";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.render(await sgApi.gstF5(this.from, this.to));
  }

  private render(r: GstF5): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("GST F5"), `${r.gst_no ? t("GST Reg No {number}", { number: r.gst_no }) : t("No GST Reg No")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // Labels are English, shown through t().
    const row = (box: string, label: string, amount: string) =>
      this.pick(h("tr", h("td", h("span.muted", `${box}  `), t(label)), num(amount)), dayBook);
    const yesNo = (box: string, label: string, b: GstYesNoBox) =>
      this.pick(h("tr", h("td", h("span.muted", `${box}  `), t(label)), h("td.num", b.yes ? M.fmt(b.amount, true) : t("No"))));
    const side = (title: string, table: HTMLElement) => h("div.tside", h("div.tside-title", t(title)), table);
    const amountHead = head(["Box", false], ["S$", true]);

    const supplies = h(
      "tbody",
      row("1", "Total value of standard-rated supplies", r.standard_rated),
      row("2", "Total value of zero-rated supplies", r.zero_rated),
      row("3", "Total value of exempt supplies", r.exempt),
    );
    const suppliesFoot = h("tfoot", h("tr", h("td", h("span.muted", "4  "), t("Total value of (1) + (2) + (3)")), num(r.total_supplies)));

    const net = M.of(r.net_gst);
    const tax = h(
      "tbody",
      row("5", "Total value of taxable purchases", r.taxable_purchases),
      row("6", "Output tax due", r.output_tax),
      row("7", "Input tax and refunds claimed", r.input_tax),
      this.pick(
        h(
          "tr",
          h("td", h("span.muted", "8  "), t(net < 0n ? "Net GST to be claimed from IRAS" : "Net GST to be paid to IRAS")),
          h("td.num", { class: net < 0n ? "gst-credit" : "" }, M.fmt(net < 0n ? M.toDecimal(-net) : r.net_gst)),
        ),
      ),
    );

    const other = h(
      "tbody",
      row("9", "Goods imported under MES / A3PL / other approved schemes", r.scheme_imports),
      yesNo("10", "Did you claim GST you refunded to tourists?", r.tourist_refunds),
      yesNo("11", "Bad debt relief and/or reverse charge refund claims?", r.bad_debt_relief),
      yesNo("12", "Did you make any pre-registration claims?", r.pre_registration),
      row("13", "Revenue (estimate from Sales and Credit Notes)", r.revenue),
      yesNo("14", "Imported services / low-value goods under reverse charge?", r.reverse_charge),
      yesNo("15", "Remote services as an electronic marketplace operator?", r.marketplace_remote_services),
      yesNo("16", "Low-value goods as a redeliverer or marketplace operator?", r.marketplace_low_value_goods),
      yesNo("17", "Did you supply imported low-value goods subject to GST?", r.own_low_value_goods),
      row("–", "Out-of-scope supplies (not reported)", r.out_of_scope),
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
        side("Supplies", h("table.rtable", amountHead, supplies, suppliesFoot)),
        side("Purchases and tax", h("table.rtable", head(["Box", false], ["S$", true]), tax)),
        side("Other boxes", h("table.rtable", head(["Box", false], ["S$", true]), other)),
      ),
      h(
        "p.gst-note",
        t("Worked out from the books for checking before you file on myTax Portal. All GST on purchases from GST-registered suppliers is treated as claimable. Reverse charge (RC businesses only): give imported services and low-value goods an RC: tax class such as RC:STANDARD; their value goes in Boxes 1, 5 and 14 and their GST in Boxes 6 and 7 (apply the normal recovery rules). Imports through Singapore Customs: give the goods an IMPORT: class (value in Box 5) and post the import permit's GST to a GST ledger classed IMPORT (Box 7). MES, IGDS and claims (Boxes 9–12, 15–21) are not worked out, and Box 13 is an estimate. Enter opens the Day Book."),
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

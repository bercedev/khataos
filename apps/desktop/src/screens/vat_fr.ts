// France: the CA3 (déclaration de TVA 3310-CA3) worked out from the books
// (khata-fr), laid out by line code like the UStVA screen. Enter on a line
// opens the Day Book for the period. Nothing is filed: the figures are for
// keying into impots.gouv.fr. E-invoices (Factur-X, UBL, CII) come from
// the shared EN 16931 export, not from this screen.
import "../gst.css";
import { frApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { Shell } from "../shell";
import type { Ca3, Ca3RateLine } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the month: the réel normal files monthly. */
function monthStart(iso: string): string {
  return `${iso.slice(0, 7)}-01`;
}

function num(text: string | null): HTMLElement {
  return h("td.num", text === null ? "" : M.fmt(text, true));
}

function head(): HTMLElement {
  return h("thead", h("tr", h("th", t("Line")), h("th", t("Operation")), h("th.num", t("Base HT (€)")), h("th.num", t("TVA (€)"))));
}

export class Ca3Report extends Report {
  title = "CA3";
  protected override period = "range" as const;
  private data: Ca3 | null = null;

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.data = await frApi.ca3(this.from, this.to);
    this.render(this.data);
  }

  private render(r: Ca3): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("CA3 (3310-CA3)"), `${r.tax_id ? t("TVA number {number}", { number: r.tax_id }) : t("No TVA number")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // `label` is English, shown through t(); `code` is the form's line code.
    const row = (code: string, label: string, base: string | null, tax: string | null) =>
      this.pick(h("tr", h("td", h("span.muted", code)), h("td", t(label)), num(base), num(tax)), dayBook);
    const rate = (code: string, label: string, l: Ca3RateLine) => row(code, label, l.base, l.tax);
    const section = (title: string, ...rows: HTMLElement[]) =>
      h("div.tside", h("div.tside-title", t(title)), h("table.rtable", head(), h("tbody", ...rows)));

    const rates: HTMLElement[] = [rate("08", "Standard rate 20%", r.l08), rate("09", "Reduced rate 5.5%", r.l09)];
    if (r.territory === "Continent") rates.push(rate("9B", "Reduced rate 10%", r.l9b), rate("T6", "2.1% (continental France)", r.t6));
    if (r.territory === "Corse")
      rates.push(rate("TC", "Corse 13%", r.tc), rate("T3", "Corse 10%", r.t3), rate("T4", "Corse 2.1%", r.t4), rate("T5", "Corse 0.9%", r.t5));
    if (r.territory === "Dom")
      rates.push(rate("10", "DOM standard rate 8.5%", r.l10), rate("11", "DOM reduced rate 2.1%", r.l11), rate("T1", "DOM 1.75%", r.t1), rate("T2", "DOM 1.05%", r.t2));
    rates.push(rate("13", "Former rates", r.l13));
    for (const o of r.other_rates) rates.push(row("", t("{p}% (no line on this CA3)", { p: o.percent }), o.base, o.tax));
    rates.push(row("15", "TVA previously deducted to repay", null, r.l15), row("16", "Total gross TVA due", null, r.l16), row("17", "of which TVA on intra-EU acquisitions", null, r.l17));

    const credit = M.of(r.l25) > 0n;
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
      section(
        "A. Taxable operations",
        row("A1", "Sales and services", r.a1, null),
        row("A2", "Other taxable operations (autoliquidation received)", r.a2, null),
        row("A3", "Services bought from suppliers not established in France", r.a3, null),
        row("B2", "Intra-EU acquisitions", r.b2, null),
        row("B5", "Regularisations (credit notes)", r.b5, null),
      ),
      section(
        "Non-taxable operations",
        row("E1", "Exports outside the EU", r.e1, null),
        row("E2", "Other non-taxable operations", r.e2, null),
        row("F2", "Intra-EU supplies to a taxable person", r.f2, null),
        row("F8", "Regularisations (credit notes)", r.f8, null),
      ),
      section("B. Gross TVA", ...rates),
      section(
        "Deductible TVA",
        row("20", "Other goods and services", null, r.l20),
        row("21", "Other TVA to deduct (your credit notes)", null, r.l21),
        row("23", "Total deductible TVA", null, r.l23),
      ),
      h(
        "div.tside",
        h(
          "table.rtable",
          h(
            "tbody",
            credit ? row("25", "TVA credit", null, r.l25) : row("TD", "TVA due", null, r.td),
            credit ? row("27", "Credit to carry forward", null, r.l27) : row("28", "Net TVA due", null, r.l28),
            this.pick(
              h("tr", h("td", h("span.muted", "32")), h("td", t("Total to pay")), h("td.num", ""), h("td.num", M.fmt(r.l32))),
              dayBook,
            ),
          ),
        ),
      ),
      h(
        "p.gst-note",
        t(
          "Worked out from the books for checking before you key it into impots.gouv.fr; KhataOS does not file it. Amounts are whole euros (nearest euro). All input TVA is treated as deductible on line 20; imports, fixed assets (line 19), the credit carried from the last return (line 22), refunds and taxes assimilées are not worked out. Enter opens the Day Book.",
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

// Netherlands: the aangifte omzetbelasting worked out from the books
// (khata-nl), laid out by rubriek like the UStVA screen. Enter on a line
// opens the Day Book for the period. Nothing is sent to the Belastingdienst.
import "../gst.css";
import { nlApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { Shell } from "../shell";
import type { Aangifte } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the calendar quarter: quarterly filing is the default. */
function quarterStart(iso: string): string {
  const m = Number(iso.slice(5, 7));
  const q = Math.floor((m - 1) / 3) * 3 + 1;
  return `${iso.slice(0, 4)}-${String(q).padStart(2, "0")}-01`;
}

function num(text: string | null): HTMLElement {
  return h("td.num", text === null ? "" : M.fmt(text, true));
}

function head(): HTMLElement {
  return h("thead", h("tr", h("th", t("Rubriek")), h("th", t("Line")), h("th.num", t("Omzet (whole €)")), h("th.num", t("Btw (whole €)"))));
}

export class AangifteReport extends Report {
  title = "Aangifte omzetbelasting";
  protected override period = "range" as const;
  private data: Aangifte | null = null;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.data = await nlApi.aangifte(this.from, this.to);
    this.render(this.data);
  }

  private render(r: Aangifte): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("Aangifte omzetbelasting"), `${r.tax_id ? t("Btw-id {number}", { number: r.tax_id }) : t("No btw-id")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // `label` is English, shown through t(); `code` is the form's rubriek.
    const row = (code: string, label: string, base: string | null, tax: string | null) =>
      this.pick(h("tr", h("td", h("span.muted", code)), h("td", t(label)), num(base), num(tax)), dayBook);
    const section = (title: string, ...rows: HTMLElement[]) =>
      h("div.tside", h("div.tside-title", t(title)), h("table.rtable", head(), h("tbody", ...rows)));

    const due = M.of(r.r5g);
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
        "1 Supplies and services in the Netherlands",
        row("1a", "Taxed at the standard rate (21%)", r.r1a, r.r1a_tax),
        row("1b", "Taxed at the reduced rate (9%)", r.r1b, r.r1b_tax),
        row("1c", "Taxed at other rates, except 0%", r.r1c, r.r1c_tax),
        row("1d", "Private use (not worked out)", r.r1d, r.r1d_tax),
        row("1e", "Taxed at 0% or not taxed by you (btw verlegd)", r.r1e, null),
      ),
      section("2 Domestic reverse charge", row("2a", "Supplies and services where the btw is shifted to you", r.r2a, r.r2a_tax)),
      section(
        "3 Supplies to other countries",
        row("3a", "Goods to countries outside the EU (exports)", r.r3a, null),
        row("3b", "Goods to and services in EU countries", r.r3b, null),
        row("3c", "Installation and distance sales within the EU (not worked out)", r.r3c, null),
      ),
      section(
        "4 Supplies and services from abroad",
        row("4a", "From countries outside the EU", r.r4a, r.r4a_tax),
        row("4b", "From EU countries", r.r4b, r.r4b_tax),
      ),
      h(
        "div.tside",
        h(
          "table.rtable",
          h(
            "tbody",
            row("5a", "Btw due (rubrieken 1 to 4)", null, r.r5a),
            row("5b", "Input tax (voorbelasting)", null, r.r5b),
            row("5c", "Subtotal", null, r.r5c),
            this.pick(
              h(
                "tr",
                h("td", h("span.muted", "5g")),
                h("td", t(due < 0n ? "Total to reclaim" : "Total to pay")),
                h("td.num", ""),
                h("td.num", { class: due < 0n ? "gst-credit" : "" }, M.fmt(due < 0n ? M.toDecimal(-due) : r.r5g)),
              ),
              dayBook,
            ),
          ),
        ),
      ),
      h(
        "p.gst-note",
        t(
          "Worked out from the books for checking before you file in Mijn Belastingdienst Zakelijk; KhataOS does not submit it. Amounts are whole euros, rounded in your favour (omzet and btw down, voorbelasting up). All input tax is treated as deductible; private use (1d), installation and distance sales (3c), import VAT and the KOR are not worked out. Enter opens the Day Book.",
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

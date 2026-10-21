// United Kingdom 9-box VAT return: boxes worked out from the books
// (khata-gb), laid out like the VAT 201 screen. Enter on a box opens the
// Day Book for the period. Ctrl+Alt+J saves the Making Tax Digital (MTD)
// VAT API body as JSON; submission to HMRC is on the Making Tax Digital
// screen (VAT periods). Ctrl+Alt+F opens the VAT scheme settings: Flat
// Rate Scheme (sector rate, first-year 1% off, limited cost business,
// cash-based turnover), Cash Accounting and Annual Accounting. The schemes
// change the figures; they are a shared record of the company and sync
// with the books (docs/sync-settings.md).
import "../gst.css";
import { api, gbApi, gbSchemesApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import { save } from "@tauri-apps/plugin-dialog";
import type { KeyEv, Shell } from "../shell";
import type { UkFlatRateDetails, UkLimitedCost, UkVatReturnSchemes, UkVatSchemes, UkVatSchemesInfo } from "../types";
import { DayBook, Report } from "./reports";

/** The MTD JSON export key, as on the GSTR-1 screen. */
const EXPORT_MTD = "Ctrl+Alt+J";
/** The VAT scheme settings dialog. */
const SCHEMES_KEY = "Ctrl+Alt+F";

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

function schemeName(s: UkVatReturnSchemes["scheme"]): string {
  switch (s) {
    case "flat_rate":
      return t("Flat Rate Scheme");
    case "cash":
      return t("Cash Accounting Scheme");
    default:
      return t("Standard accounting (invoice basis)");
  }
}

/** A sector from a number in HMRC's list or (part of) its name. */
function findSector(info: UkVatSchemesInfo, text: string): { name: string; percent: string } | null {
  const v = text.trim();
  if (!v) return null;
  const n = Number(v);
  if (Number.isInteger(n) && n >= 1 && n <= info.sectors.length) return info.sectors[n - 1];
  const low = v.toLowerCase();
  return (
    info.sectors.find((s) => s.name.toLowerCase() === low) ??
    info.sectors.find((s) => s.name.toLowerCase().startsWith(low)) ??
    info.sectors.find((s) => s.name.toLowerCase().includes(low)) ??
    null
  );
}

function yesNo(text: string): boolean | null {
  const v = text.trim().toLowerCase();
  if (v.startsWith("y")) return true;
  if (v.startsWith("n") || v === "") return false;
  return null;
}

export class UkVatReturnReport extends Report {
  title = "VAT Return";
  protected override period = "range" as const;
  private data: UkVatReturnSchemes | null = null;
  private schemes: UkVatSchemesInfo | null = null;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    [this.data, this.schemes] = await Promise.all([gbSchemesApi.vatReturn(this.from, this.to), gbSchemesApi.schemes()]);
    this.render(this.data);
  }

  private render(r: UkVatReturnSchemes): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("VAT Return"), `${r.vrn ? t("VAT Reg No {number}", { number: r.vrn }) : t("No VAT number")} · ${span} · ${schemeName(r.scheme)}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // `label` and `note` are English, shown through t().
    const row = (box: string, label: string, amount: string, note?: string) =>
      this.pick(h("tr", h("td", h("span.muted", `${box}  `), t(label), note ? h("span.muted", ` · ${t(note)}`) : ""), num(amount)), dayBook);
    const flat = r.scheme === "flat_rate";

    const net = M.of(r.net_vat_due);
    const vat = h(
      "tbody",
      row("1", "VAT due on sales and other outputs", r.vat_due_sales, flat ? "flat rate × turnover, plus reverse charges and postponed import VAT" : undefined),
      row("2", "VAT due on acquisitions of goods from the EU", r.vat_due_acquisitions, "Northern Ireland only; not worked out"),
      row("3", "Total VAT due", r.total_vat_due),
      row("4", "VAT reclaimed on purchases and other inputs", r.vat_reclaimed, flat ? "capital goods of £2,000 or more and reverse charges only" : undefined),
      this.pick(
        h(
          "tr",
          h("td", h("span.muted", "5  "), t(net < 0n ? "Net VAT to be reclaimed" : "Net VAT to pay to HMRC")),
          h("td.num", { class: net < 0n ? "gst-credit" : "" }, M.fmt(net < 0n ? M.toDecimal(-net) : r.net_vat_due)),
        ),
        dayBook,
      ),
    );
    const values = h(
      "tbody",
      row("6", "Total value of sales and all other outputs excluding VAT", r.total_sales_ex_vat, flat ? "flat rate turnover including VAT" : r.scheme === "cash" ? "on payments received" : undefined),
      row("7", "Total value of purchases and all other inputs excluding VAT", r.total_purchases_ex_vat, r.scheme === "cash" ? "on payments made" : undefined),
      row("8", "Total value of supplies of goods to EU member states", r.goods_supplied_to_eu, r.northern_ireland ? "from Northern Ireland" : "Northern Ireland only"),
      row("9", "Total value of acquisitions of goods from EU member states", r.goods_acquired_from_eu, r.northern_ireland ? "into Northern Ireland" : "Northern Ireland only"),
    );
    const side = (title: string, table: HTMLElement) => h("div.tside", h("div.tside-title", t(title)), table);

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
        side("VAT (pounds and pence)", h("table.rtable", head(["Box", false], ["£", true]), vat)),
        side("Values excluding VAT (whole pounds)", h("table.rtable", head(["Box", false], ["£", true]), values)),
        ...(r.flat_rate ? [side("Flat Rate Scheme", this.flatRateTable(r.flat_rate))] : []),
      ),
      h(
        "p.gst-note",
        t(
          'Worked out from the books for checking before you submit to HMRC. VAT on purchases is treated as reclaimable in full; acquisition VAT (Box 2) is not worked out. Mark reverse charges and imports on the tax class: RC: for services from abroad, DRC: for the domestic reverse charge (construction, mobile phones and chips), IMPORT: for imported goods; post C79 import VAT to a VAT ledger classed IMPORT and the monthly postponed import VAT statement as a journal between VAT ledgers classed IMPORT:PVA. {schemes} sets the VAT scheme. {key} saves the Making Tax Digital JSON body for the period (with "finalised" false); to submit the return, use Making Tax Digital → VAT periods. Enter opens the Day Book.',
          { key: EXPORT_MTD, schemes: SCHEMES_KEY },
        ),
      ),
      ...(r.annual_accounting ? [h("p.gst-note", t("Annual Accounting Scheme: pick the whole accounting year as the period; the return is due 2 months after it ends, less the interim payments made."))] : []),
      ...(this.schemes ? [this.sectorList(this.schemes)] : []),
    );
  }

  /** HMRC's flat rates, numbered for the settings dialog. */
  private sectorList(info: UkVatSchemesInfo): HTMLElement {
    return h(
      "details.gst-note",
      h("summary", t("Flat Rate Scheme sectors ({n}); limited cost business {lcb}%", { n: info.sectors.length, lcb: M.fmt(info.limited_cost_percent) })),
      h(
        "table.rtable",
        head(["No.", false], ["Sector", false], ["%", true]),
        h("tbody", ...info.sectors.map((x, i) => h("tr", h("td", String(i + 1)), h("td", x.name), h("td.num", M.fmt(x.percent))))),
      ),
      h("p.note", t("Rates from HMRC's \"VAT Flat Rate Scheme: how much you pay\" (checked 21 Oct 2026). Use the rate HMRC confirmed for your business.")),
    );
  }

  private flatRateTable(x: UkFlatRateDetails): HTMLElement {
    const row = (label: string, value: string, isNum = true) => h("tr", h("td", label), isNum ? num(value) : h("td.num", value));
    return h(
      "table.rtable",
      head(["Flat rate", false], ["£", true]),
      h(
        "tbody",
        row(t("Sector"), `${x.sector} (${M.fmt(x.sector_percent)}%)`, false),
        row(t("Flat rate turnover (VAT included)"), x.turnover),
        ...x.segments.map((s) => row(t("Turnover at {rate}%", { rate: M.fmt(s.percent) }), s.turnover)),
        ...x.segments.map((s) => row(t("VAT at {rate}%", { rate: M.fmt(s.percent) }), s.vat)),
        row(t("Limited cost business"), x.limited_cost ? t("yes: 16.5%") : t("no"), false),
        row(t("Goods bought (limited cost test)"), x.relevant_goods),
        row(t("Limited cost test limit"), x.limited_cost_limit),
        row(t("Capital goods reclaimed (value)"), x.capital_value),
        row(t("Capital goods reclaimed (VAT)"), x.capital_vat),
        row(t("Turnover basis"), x.cash_turnover ? t("cash-based (payments received)") : t("basic (invoices)"), false),
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

  override bottom() {
    const b = super.bottom();
    b.splice(
      b.length - 1,
      0,
      { chord: SCHEMES_KEY, label: "VAT Schemes", enabled: true },
      { chord: EXPORT_MTD, label: "Export MTD JSON", enabled: true },
    );
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === EXPORT_MTD) {
      void this.exportMtd();
      return true;
    }
    if (e.chord === SCHEMES_KEY) {
      void this.editSchemes();
      return true;
    }
    return super.key(e);
  }

  /** The VAT scheme settings: one dialog, plain text answers. */
  private async editSchemes(): Promise<void> {
    try {
      const info = this.schemes ?? (await gbSchemesApi.schemes());
      const s = info.settings;
      const f = s.flat_rate;
      const current = f ? "flat rate" : s.cash_accounting ? "cash" : "standard";
      this.shell.info(t("Flat rate sectors: type a number from the list below the return, or part of the name."), 10000);
      const v = await this.shell.dialog(
        "VAT Schemes",
        [
          { label: "Scheme (standard, cash or flat rate)", value: current },
          { label: "Flat rate sector (number or name)", value: f?.sector ?? "" },
          { label: "Flat rate % (blank: the sector's)", value: f?.percent ?? "" },
          { label: "VAT registration date (first-year 1% off; blank: none)", value: f?.registered_on ?? "" },
          { label: "Limited cost business (auto, yes or no)", value: f?.limited_cost ?? "auto" },
          { label: "Flat rate turnover (invoices or cash)", value: f?.cash_turnover ? "cash" : "invoices" },
          { label: "Annual Accounting Scheme (yes or no)", value: s.annual_accounting ? "yes" : "no" },
        ],
        ([scheme, sector, percent, reg, lcb, turnover, annual]) => {
          const sc = scheme.trim().toLowerCase();
          if (!/^(s|c|f)/.test(sc)) return t("The scheme is standard, cash or flat rate.");
          if (sc.startsWith("f")) {
            if (!findSector(info, sector) && !percent.trim()) return t("Pick a sector from HMRC's list (its number or name), or type the flat rate.");
            if (percent.trim() && !(Number(percent) > 0 && Number(percent) < 100)) return t("The flat rate is a percentage between 0 and 100.");
            if (reg.trim() && !/^\d{4}-\d{2}-\d{2}$/.test(reg.trim())) return t("Type the registration date as YYYY-MM-DD, or leave it blank.");
            if (!/^(a|y|n)/i.test(lcb.trim())) return t("Limited cost business is auto, yes or no.");
            if (!/^(i|c)/i.test(turnover.trim())) return t("Flat rate turnover is invoices or cash.");
          }
          if (yesNo(annual) === null) return t("Annual Accounting is yes or no.");
          return null;
        },
      );
      if (!v) return;
      const [scheme, sector, percent, reg, lcb, turnover, annual] = v;
      const sc = scheme.trim().toLowerCase();
      const found = findSector(info, sector);
      const limited: UkLimitedCost = /^y/i.test(lcb.trim()) ? "yes" : /^n/i.test(lcb.trim()) ? "no" : "auto";
      const settings: UkVatSchemes = {
        flat_rate: sc.startsWith("f")
          ? {
              sector: found?.name ?? sector.trim(),
              percent: percent.trim() || found?.percent || "",
              registered_on: reg.trim() || null,
              limited_cost: limited,
              cash_turnover: /^c/i.test(turnover.trim()),
            }
          : null,
        cash_accounting: sc.startsWith("c"),
        annual_accounting: yesNo(annual) ?? false,
      };
      this.schemes = await gbSchemesApi.setSchemes(settings);
      this.shell.info(t("VAT scheme saved. It syncs to the company's other computers with the books."), 8000);
      await this.reload();
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** Ask for HMRC's period key (from the VAT obligations), then save the JSON body. */
  private async exportMtd(): Promise<void> {
    try {
      const v = await this.shell.dialog("MTD period key", [{ label: "Period key (e.g. 26A1)", value: "" }], ([k]) =>
        /^[A-Za-z0-9#]{4}$/.test(k.trim()) ? null : t("The period key is the 4 characters HMRC gives for the period."),
      );
      if (!v) return;
      const key = v[0].trim();
      const json = await gbApi.mtdVatJson(this.from, this.to, key);
      const name = `VAT-MTD-${this.data?.vrn ?? "draft"}-${key.replace("#", "")}`;
      const path = await save({ defaultPath: `${name}.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const file = path.toLowerCase().endsWith(".json") ? path : `${path}.json`;
      await api.writeGstJson(file, json);
      this.shell.info(t("MTD VAT JSON saved to {file}. To submit the return to HMRC, use Making Tax Digital → VAT periods.", { file: ltr(file) }));
    } catch (e) {
      this.shell.error(e);
    }
  }
}

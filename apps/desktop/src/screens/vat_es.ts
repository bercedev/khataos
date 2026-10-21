// Spain: the modelo 303 worked out from the books (khata-es), laid out by
// casilla like the UStVA screen; Enter on a line opens the Day Book for the
// period. Nothing is sent to the AEAT.
//
// Also the VeriFactu record export for a saved Sales or Credit Note voucher
// (exportVerifactu), meant for the voucher screen's export key next to
// XRechnung (see screens/einvoice.ts exportVoucher).
import "../gst.css";
import { api, esApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import { save } from "@tauri-apps/plugin-dialog";
import type { Shell } from "../shell";
import type { Modelo303, VoucherKind } from "../types";
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
  return h("thead", h("tr", h("th", t("Casilla")), h("th", t("Line")), h("th.num", t("Base (€)")), h("th.num", t("Cuota (€)"))));
}

export class Modelo303Report extends Report {
  title = "Modelo 303";
  protected override period = "range" as const;
  private data: Modelo303 | null = null;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.data = await esApi.modelo303(this.from, this.to);
    this.render(this.data);
  }

  private render(r: Modelo303): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("Modelo 303"), `${r.tax_id ? t("NIF {number}", { number: r.tax_id }) : t("No NIF")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // `label` is English, shown through t(); `code` is the casilla.
    const row = (code: string, label: string, base: string | null, tax: string | null) =>
      this.pick(h("tr", h("td", h("span.muted", code)), h("td", t(label)), num(base), num(tax)), dayBook);
    const section = (title: string, ...rows: HTMLElement[]) =>
      h("div.tside", h("div.tside-title", t(title)), h("table.rtable", head(), h("tbody", ...rows)));

    const result = M.of(r.c71);
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
        "IVA devengado (output tax)",
        row("01/03", "General scheme 4%", r.c01, r.c03),
        row("04/06", "General scheme 10%", r.c04, r.c06),
        row("07/09", "General scheme 21%", r.c07, r.c09),
        row("150/152", "0% (temporary)", r.c150, r.c152),
        row("153/155", "5% (temporary)", r.c153, r.c155),
        row("10/11", "Intra-EU acquisitions of goods and services", r.c10, r.c11),
        row("12/13", "Other reverse-charge operations (inversión del sujeto pasivo)", r.c12, r.c13),
        row("14/15", "Changes to bases and cuotas (your rectificativas)", r.c14, r.c15),
        row("27", "Total cuota devengada", null, r.c27),
      ),
      section(
        "IVA deducible (input tax)",
        row("28/29", "Domestic purchases (current goods and services)", r.c28, r.c29),
        row("36/37", "Intra-EU acquisitions of current goods", r.c36, r.c37),
        row("40/41", "Corrections of deductions (suppliers' rectificativas)", r.c40, r.c41),
        row("45", "Total to deduct", null, r.c45),
        row("46", "Result of the general scheme (27 − 45)", null, r.c46),
      ),
      section(
        "Información adicional",
        row("59", "Intra-EU supplies of goods and services", r.c59, null),
        row("60", "Exports and similar operations", r.c60, null),
        row("120", "Not subject under the place-of-supply rules", r.c120, null),
        row("122", "Subject with inversión del sujeto pasivo", r.c122, null),
      ),
      h(
        "div.tside",
        h(
          "table.rtable",
          h(
            "tbody",
            row("64", "Sum of results", null, r.c64),
            row("65", "% attributable to the State", null, r.c65),
            row("66", "Attributable to the State", null, r.c66),
            row("69", "Result", null, r.c69),
            this.pick(
              h(
                "tr",
                h("td", h("span.muted", "71")),
                h("td", t(result < 0n ? "Result: to refund or carry forward" : "Result: to pay")),
                h("td.num", ""),
                h("td.num", { class: result < 0n ? "gst-credit" : "" }, M.fmt(result < 0n ? M.toDecimal(-result) : r.c71)),
              ),
              dayBook,
            ),
          ),
        ),
      ),
      h(
        "p.gst-note",
        t(
          "Worked out from the books for checking before you file at the AEAT's sede electrónica; KhataOS does not submit it. All input IVA is treated as deductible and as current goods and services; the recargo de equivalencia, imports, investment goods, the prorrata and amounts to carry over from earlier periods (110, 78) are not worked out. Enter opens the Day Book.",
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

/** A saved voucher to export (the shape screens/einvoice.ts passes). */
export interface VerifactuVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
}

/**
 * The VeriFactu record of a saved Sales or Credit Note voucher of a
 * Spanish company: generated once (the chain moves on), then saved as XML.
 * The QR URL is shown for the printed invoice. Nothing is sent to the AEAT.
 */
export async function exportVerifactu(shell: Shell, v: VerifactuVoucher): Promise<void> {
  if (v.kind !== "Sales" && v.kind !== "CreditNote") return shell.warn(t("VeriFactu records are made for Sales and Credit Note vouchers."));
  try {
    const x = await shell.transition(esApi.verifactuRecord(v.id));
    const path = await save({ defaultPath: x.file_name, filters: [{ name: "XML", extensions: ["xml"] }] });
    if (path) {
      const file = path.toLowerCase().endsWith(".xml") ? path : `${path}.xml`;
      await api.writeGstJson(file, x.xml);
      shell.info(
        t("VeriFactu record {n} ({kind}) saved to {file}. Huella {hash}. QR: {url}", {
          n: x.numero,
          kind: x.kind,
          file: ltr(file),
          hash: x.huella.slice(0, 16),
          url: ltr(x.qr_url),
        }),
        10000,
      );
    }
    if (x.warnings.length) shell.warn(x.warnings.join(" "));
  } catch (e) {
    shell.error(e);
  }
}

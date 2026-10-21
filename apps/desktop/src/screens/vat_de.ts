// Germany: the UStVA (Umsatzsteuer-Voranmeldung) worked out from the books
// (khata-de), laid out by Kennzahl like the UK VAT return screen. Enter on
// a line opens the Day Book for the period. Nothing is sent to ELSTER.
//
// Also the XRechnung export for a saved Sales or Credit Note voucher
// (exportXRechnung), meant for the voucher screen's export key (Ctrl+E /
// Alt+E, see screens/einvoice.ts exportVoucher).
import "../gst.css";
import { api, deApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t } from "../i18n";
import * as M from "../money";
import { save } from "@tauri-apps/plugin-dialog";
import type { Shell } from "../shell";
import type { Ustva, VoucherKind } from "../types";
import { DayBook, Report } from "./reports";

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

/** First day of the calendar quarter: quarterly filing is the default (§18 Abs. 2 UStG). */
function quarterStart(iso: string): string {
  const m = Number(iso.slice(5, 7));
  const q = Math.floor((m - 1) / 3) * 3 + 1;
  return `${iso.slice(0, 4)}-${String(q).padStart(2, "0")}-01`;
}

function num(text: string | null): HTMLElement {
  return h("td.num", text === null ? "" : M.fmt(text, true));
}

function head(): HTMLElement {
  return h("thead", h("tr", h("th", t("Kz")), h("th", t("Line")), h("th.num", t("Base (whole €)")), h("th.num", t("Tax (€)"))));
}

export class UstvaReport extends Report {
  title = "UStVA";
  protected override period = "range" as const;
  private data: Ustva | null = null;

  constructor(shell: Shell, private from = quarterStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.data = await deApi.ustva(this.from, this.to);
    this.render(this.data);
  }

  private render(r: Ustva): void {
    const span = this.from === this.to ? D.show(this.from) : t("{from} to {to}", { from: D.show(this.from), to: D.show(this.to) });
    this.heading(t("UStVA"), `${r.tax_id ? t("USt-IdNr / Steuernummer {number}", { number: r.tax_id }) : t("No tax number")} · ${span}`);
    this.resetPicks();
    const dayBook = () => void this.shell.push(new DayBook(this.shell, this.from, this.to));
    // `label` is English, shown through t(); `kz` is the form's Kennzahl.
    const row = (kz: string, label: string, base: string | null, tax: string | null) =>
      this.pick(h("tr", h("td", h("span.muted", kz)), h("td", t(label)), num(base), num(tax)), dayBook);
    const section = (title: string, ...rows: HTMLElement[]) =>
      h("div.tside", h("div.tside-title", t(title)), h("table.rtable", head(), h("tbody", ...rows)));

    const due = M.of(r.kz83);
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
        "Supplies and services",
        row("81", "Taxable at 19%", r.kz81, r.kz81_tax),
        row("86", "Taxable at 7%", r.kz86, r.kz86_tax),
        row("35/36", "Taxable at other rates", r.kz35, r.kz36),
        row("87", "Taxable at 0% (§12 Abs. 3)", r.kz87, null),
        row("41", "Tax-free intra-EU supplies to buyers with a VAT id", r.kz41, null),
        row("43", "Other tax-free supplies with input-tax deduction (exports)", r.kz43, null),
        row("48", "Tax-free supplies without input-tax deduction", r.kz48, null),
        row("60", "Supplies where the recipient owes the tax (§13b)", r.kz60, null),
        row("21", "Services taxable in another EU state (§18b)", r.kz21, null),
        row("45", "Other supplies not taxable in Germany", r.kz45, null),
      ),
      section(
        "Purchases with tax owed by the company",
        row("89", "Intra-EU acquisitions at 19%", r.kz89, r.kz89_tax),
        row("93", "Intra-EU acquisitions at 7%", r.kz93, r.kz93_tax),
        row("46/47", "Services from EU businesses (§13b Abs. 1)", r.kz46, r.kz47),
        row("84/85", "Other supplies received under §13b", r.kz84, r.kz85),
      ),
      section(
        "Input tax",
        row("66", "Input tax from invoices", null, r.kz66),
        row("61", "Input tax on intra-EU acquisitions", null, r.kz61),
        row("67", "Input tax on §13b supplies", null, r.kz67),
      ),
      h(
        "div.tside",
        h(
          "table.rtable",
          h(
            "tbody",
            row("", "Output tax", null, r.output_tax),
            row("", "Deductible input tax", null, r.input_tax),
            this.pick(
              h(
                "tr",
                h("td", h("span.muted", "83")),
                h("td", t(due < 0n ? "Refund (Überschuss)" : "Prepayment due (Vorauszahlung)")),
                h("td.num", ""),
                h("td.num", { class: due < 0n ? "gst-credit" : "" }, M.fmt(due < 0n ? M.toDecimal(-due) : r.kz83)),
              ),
              dayBook,
            ),
          ),
        ),
      ),
      h(
        "p.gst-note",
        t(
          "Worked out from the books for checking before you file through ELSTER; KhataOS does not submit it. Bases are whole euros (cents dropped), taxes euros and cents. All input tax is treated as deductible; import VAT (Kz 62), triangular trade, new vehicles, the Sondervorauszahlung and corrections are not worked out. Enter opens the Day Book.",
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
export interface XRechnungVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
}

/**
 * XRechnung 3.0 for a saved Sales or Credit Note voucher of a German
 * company: asks UBL or CII, then saves the XML. Broken rules are listed;
 * the file is still saved so it can be checked with the KoSIT validator.
 */
export async function exportXRechnung(shell: Shell, v: XRechnungVoucher): Promise<void> {
  if (v.kind !== "Sales" && v.kind !== "CreditNote") return shell.warn(t("XRechnung invoices are made from Sales and Credit Note vouchers."));
  const answer = await shell.dialog("XRechnung", [{ label: "Syntax (UBL or CII)", value: "UBL" }], ([s]) =>
    /^(ubl|cii)$/i.test(s.trim()) ? null : t("Type UBL or CII."),
  );
  if (!answer) return;
  const syntax = answer[0].trim().toLowerCase() as "ubl" | "cii";
  try {
    const x = await shell.transition(deApi.xrechnungXml(v.id, syntax));
    const path = await save({ defaultPath: x.file_name, filters: [{ name: "XML", extensions: ["xml"] }] });
    if (!path) return;
    const file = path.toLowerCase().endsWith(".xml") ? path : `${path}.xml`;
    await api.writeGstJson(file, x.xml);
    if (x.errors.length) {
      shell.warn(t("Saved to {file}, but it is not a valid XRechnung yet: {errors}", { file: ltr(file), errors: x.errors.join(" ") }));
    } else {
      shell.info(t("XRechnung ({syntax}) saved to {file}. Check it with the KoSIT validator before you send it.", { syntax: syntax.toUpperCase(), file: ltr(file) }), 8000);
    }
    if (x.warnings.length) shell.warn(x.warnings.join(" "));
  } catch (e) {
    shell.error(e);
  }
}

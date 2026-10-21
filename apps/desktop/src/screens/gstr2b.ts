// GSTR-2B reconciliation: import the GSTR-2B JSON from the GST portal
// (Alt+S), match it with the purchase register for the period, and drill
// down from a row to its purchase voucher. The rules are in
// khata-in gstr2b.rs; the books side in src-tauri gstr2b.rs.
import "../gst.css";
import { open } from "@tauri-apps/plugin-dialog";
import { gstAdvApi } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { ltr, t as tr } from "../i18n";
import * as M from "../money";
import type { KeyEv, Shell } from "../shell";
import type { GstAmounts, Gstr2bRecon, Gstr2bRow, Gstr2bStatus } from "../types";
import { Report } from "./reports";
import { savedVoucherScreen } from "./stockjournal";

const IMPORT_KEY = "Alt+S";
/** Tolerances (days, ₹). */
const TOLERANCE_KEY = "Ctrl+Alt+T";

const STATUS_LABEL: Record<Gstr2bStatus, string> = {
  Matched: "Matched",
  Mismatched: "Mismatched",
  OnlyIn2b: "In GSTR-2B, not in books",
  OnlyInBooks: "In books, not in GSTR-2B",
};

/** Known `rsn` codes. "P" is from GSTN's schema samples; the rest are shown as given. */
const REASONS: Record<string, string> = {
  P: "Place of supply: recipient in another State",
  C: "Time limit under s.16(4)",
};

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function num(text: string): HTMLElement {
  return h("td.num", M.fmt(text, true));
}

function taxCells(a: GstAmounts): HTMLElement[] {
  return [num(a.taxable), num(a.igst), num(a.cgst), num(a.sgst), num(a.cess)];
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function warnings(list: string[]): HTMLElement | null {
  if (!list.length) return null;
  return h(
    "div.gst-warnings",
    { role: "status" },
    h("div.gst-warn-title", list.length === 1 ? tr("1 thing to check") : tr("{n} things to check", { n: list.length })),
    h("ul", ...list.map((w) => h("li", { dir: "auto" }, w))),
  );
}

const HEADS = ["Taxable Value", "IGST", "CGST", "SGST/UTGST", "Cess"];

export class Gstr2bReport extends Report {
  title = "GSTR-2B Reconciliation";
  protected override period = "range" as const;
  private json: string | null = null;
  private file = "";
  private days: number | null = null;
  private amount: string | null = null;

  constructor(shell: Shell, private from = `${today(shell).slice(0, 8)}01`, private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    this.resetPicks();
    if (!this.json) {
      this.heading(tr("GSTR-2B Reconciliation"), range(this.from, this.to));
      this.bodyEl.replaceChildren(
        h(
          "p.gst-note",
          tr(
            "Press {key} to import the GSTR-2B JSON downloaded from the GST portal (Returns → GSTR-2B → Download JSON). Its documents are matched with the purchases of the period on supplier GSTIN, invoice number (or the bill reference on the supplier's line), date and amounts.",
            { key: ltr(IMPORT_KEY) },
          ),
        ),
      );
      return;
    }
    const r = await gstAdvApi.gstr2bReconcile(this.json, this.from, this.to, this.days, this.amount);
    this.render(r);
  }

  private render(r: Gstr2bRecon): void {
    const period = r.return_period ? ` · ${tr("Return period {period}", { period: ltr(`${r.return_period.slice(0, 2)}/${r.return_period.slice(2)}`) })}` : "";
    this.heading(tr("GSTR-2B Reconciliation"), `${r.gstin ? ltr(r.gstin) : ""}${period} · ${tr("Books")}: ${range(this.from, this.to)}`);
    const summary = h("tbody");
    for (const s of r.summary) {
      summary.append(this.pick(h("tr", h("td.strong", tr(STATUS_LABEL[s.status])), h("td.num", String(s.count || "")), ...taxCells(s.amounts))));
    }
    const rows = h("tbody");
    for (const row of r.rows) rows.append(this.row(row));
    if (!r.rows.length) rows.append(h("tr.empty", h("td", { colSpan: 9 }, tr("Nothing to reconcile for the period."))));
    const parts: (HTMLElement | null)[] = [
      warnings(r.warnings),
      h("table.rtable", h("thead", h("tr", h("th", tr("Status")), h("th.num", tr("Count")), ...HEADS.map((c) => h("th.num", tr(c))))), summary),
      h(
        "table.rtable",
        h("thead", h("tr", h("th", tr("Status")), h("th", tr("Supplier")), h("th", tr("No.")), h("th", tr("Date")), ...HEADS.map((c) => h("th.num", tr(c))))),
        rows,
      ),
    ];
    if (r.itc_unavailable.length) {
      const t = h("tbody");
      for (const p of r.itc_unavailable) {
        const reason = p.reason ? (REASONS[p.reason] ? `${p.reason}: ${tr(REASONS[p.reason])}` : p.reason) : "";
        t.append(this.pick(h("tr", h("td", p.supplier_name || ltr(p.supplier_gstin)), h("td", ltr(p.number)), h("td", p.date ? D.show(p.date) : ""), h("td", reason), num(p.amounts.igst), num(p.amounts.cgst), num(p.amounts.sgst))));
      }
      parts.push(
        h("div.tside-title", tr("ITC not available (as GSTR-2B says)")),
        h("table.rtable", h("thead", h("tr", h("th", tr("Supplier")), h("th", tr("No.")), h("th", tr("Date")), h("th", tr("Reason")), h("th.num", "IGST"), h("th.num", "CGST"), h("th.num", tr("SGST/UTGST")))), t),
      );
    }
    if (r.imports.length) {
      const t = h("tbody");
      for (const b of r.imports) {
        t.append(this.pick(h("tr", h("td", ltr(b.boe_number)), h("td", b.boe_date ? D.show(b.boe_date) : ""), h("td", ltr(b.port)), num(b.taxable), num(b.igst), num(b.cess))));
      }
      parts.push(
        h("div.tside-title", tr("Imports of goods (bills of entry, not matched)")),
        h("table.rtable", h("thead", h("tr", h("th", tr("Bill of entry")), h("th", tr("Date")), h("th", tr("Port")), h("th.num", tr("Taxable Value")), h("th.num", "IGST"), h("th.num", tr("Cess")))), t),
      );
    }
    parts.push(
      h(
        "p.gst-note",
        tr(
          "File: {file}. Amounts in the rows are GSTR-2B's, or the books' when the document is only in the books. Enter opens the purchase. {key} imports another file; {tol} changes the tolerances (default 3 days and ₹1). A preparation aid, not legal advice: act on mismatches in IMS before GSTR-2B is final.",
          { file: ltr(this.file), key: ltr(IMPORT_KEY), tol: ltr(TOLERANCE_KEY) },
        ),
      ),
    );
    this.bodyEl.replaceChildren(...parts.filter((x): x is HTMLElement => x !== null));
  }

  private row(r: Gstr2bRow): HTMLElement {
    const p = r.portal;
    const b = r.books;
    const supplier = p ? p.supplier_name || p.supplier_gstin : b?.supplier_name ?? "";
    const number = p && b && p.number !== b.number ? `${p.number} / ${b.number}` : (p?.number ?? b?.number ?? "");
    const date = p?.date ?? b?.date ?? "";
    const amounts = p?.amounts ?? b!.amounts;
    const flags = [p?.reverse_charge ? tr("reverse charge") : "", p && !p.itc_available ? tr("ITC not available") : "", p?.section.endsWith("a") ? tr("amended") : ""].filter(Boolean);
    const open = b ? () => void this.shell.push(savedVoucherScreen(this.shell, b.kind === "Invoice" ? "Purchase" : "DebitNote", b.date, b.id)) : undefined;
    return this.pick(
      h(
        "tr",
        { class: r.status === "Matched" ? "" : "warnrow" },
        h("td", tr(STATUS_LABEL[r.status]), r.differences.length ? h("div.muted", { dir: "auto" }, r.differences.join("; ")) : null),
        h("td.strong", supplier, flags.length ? h("span.muted", ` · ${flags.join(", ")}`) : null),
        h("td", ltr(number)),
        h("td", date ? D.show(date) : ""),
        ...taxCells(amounts),
      ),
      open,
    );
  }

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: IMPORT_KEY, label: "Import GSTR-2B", enabled: true }, { chord: TOLERANCE_KEY, label: "Tolerance", enabled: !!this.json });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === IMPORT_KEY) {
      void this.importFile();
      return true;
    }
    if (e.chord === TOLERANCE_KEY && this.json) {
      void this.askTolerance();
      return true;
    }
    return super.key(e);
  }

  private async importFile(): Promise<void> {
    let path: string | string[] | null;
    try {
      path = await open({ multiple: false, title: tr("Choose the GSTR-2B JSON file"), filters: [{ name: "GSTR-2B", extensions: ["json", "JSON"] }] });
    } catch (e) {
      return this.shell.error(e);
    }
    const file = Array.isArray(path) ? path[0] : path;
    if (!file) return;
    try {
      this.json = await this.shell.transition(gstAdvApi.readGstr2bFile(file));
      this.file = file.split(/[\\/]/).pop() ?? file;
      // A GSTR-2B for MMYYYY reconciles that month's books by default.
      const m = /"rtnprd"\s*:\s*"(\d{2})(\d{4})"/.exec(this.json);
      if (m) {
        const last = new Date(Date.UTC(Number(m[2]), Number(m[1]), 0)).getUTCDate();
        this.from = `${m[2]}-${m[1]}-01`;
        this.to = `${m[2]}-${m[1]}-${String(last).padStart(2, "0")}`;
      }
      this.cursor = 0;
      await this.reload();
    } catch (e) {
      this.json = null;
      this.shell.error(e);
    }
  }

  private async askTolerance(): Promise<void> {
    const v = await this.shell.dialog(
      "Tolerance",
      [
        { label: "Days", value: String(this.days ?? 3) },
        { label: "Amount (₹)", value: this.amount ?? "1" },
      ],
      ([d, a]) => (!/^\d{1,3}$/.test(d.trim()) ? tr('"{text}" is not a number of days.', { text: d }) : M.parse(a) === null ? tr('"{text}" is not an amount.', { text: a }) : null),
    );
    if (!v) return;
    this.days = Number(v[0].trim());
    this.amount = v[1].trim();
    await this.reload();
  }

  protected override async askPeriod(): Promise<void> {
    const r = await this.rangeDialog(this.from, this.to);
    if (!r) return;
    [this.from, this.to] = r;
    this.cursor = 0;
    await this.reload();
  }
}

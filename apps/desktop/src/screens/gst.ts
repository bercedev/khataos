// GST returns: GSTR-1 (section summary → documents → voucher) and GSTR-3B.
// The figures come from the books (gst.rs / khata-in returns.rs) and are a
// preparation aid: check them, and validate the JSON with the GST offline
// tool, before filing.
import "../gst.css";
import { api } from "../api";
import { h } from "../dom";
import * as D from "../dates";
import { inUse, ltr, t as tr } from "../i18n";
import * as M from "../money";
import { save } from "@tauri-apps/plugin-dialog";
import type { KeyEv, Shell } from "../shell";
import type { GstAmounts, GstReturnDoc, Gstr1, Gstr1Section, Gstr3b } from "../types";
import { Report } from "./reports";
import { VoucherScreen } from "./voucher";

/** The JSON export key; not bound to anything else in reports. */
const EXPORT_JSON = "Ctrl+Alt+J";
/** e-Invoice JSON for the period's B2B, SEZ and export documents; also unbound elsewhere. */
const EINVOICE_BULK = "Ctrl+Alt+E";

const DOC_SECTIONS: Gstr1Section[] = ["b2b", "b2cl", "cdnr", "cdnur", "exp"];

function today(shell: Shell): string {
  return shell.company?.today ?? D.todayIso();
}

function monthStart(isoDate: string): string {
  return `${isoDate.slice(0, 8)}01`;
}

function range(from: string, to: string): string {
  return from === to ? D.show(from) : tr("{from} to {to}", { from: D.show(from), to: D.show(to) });
}

function num(text: string): HTMLElement {
  return h("td.num", M.fmt(text, true));
}

function taxCells(a: GstAmounts): HTMLElement[] {
  return [num(a.taxable), num(a.igst), num(a.cgst), num(a.sgst), num(a.cess)];
}

const TAX_HEADS = ["Taxable Value", "IGST", "CGST", "SGST/UTGST", "Cess"];

/** Column titles are English, shown through tr(). */
function head(...cols: [string, boolean][]): HTMLElement {
  // "Section" here is a part of the return, not a section of the Act.
  const label = (c: string) => (c === "Section" ? inUse(c, tr("Section (return table)")) : tr(c));
  return h("thead", h("tr", ...cols.map(([c, n]) => h(n ? "th.num" : "th", label(c)))));
}

function taxHead(...first: string[]): HTMLElement {
  return head(...first.map((t): [string, boolean] => [t, false]), ...TAX_HEADS.map((t): [string, boolean] => [t, true]));
}

function docTotals(d: GstReturnDoc): GstAmounts {
  const add = (k: keyof GstAmounts) => M.toDecimal(M.sum(d.items.map((i) => M.of(i.amounts[k]))));
  return { taxable: add("taxable"), igst: add("igst"), cgst: add("cgst"), sgst: add("sgst"), cess: add("cess") };
}

/** "18.00" → "18", "0.250" → "0.25". */
function plain(n: string): string {
  return n.includes(".") ? n.replace(/\.?0+$/, "") : n;
}

function rate(r: string): string {
  return `${plain(r)}%`;
}

/** State names by code, for places of supply ("29 Karnataka"). */
async function stateNames(): Promise<Map<string, string>> {
  try {
    const t = await api.companyTax();
    return new Map(t.regions.map((r) => [r.code, r.name]));
  } catch {
    return new Map();
  }
}

function place(names: Map<string, string>, code: string): string {
  // State names come from the country pack (English), kept left to right with their code.
  if (code === "96") return `96 ${tr("Other countries")}`;
  const n = names.get(code);
  return n ? ltr(`${code} ${n}`) : code;
}

function warnings(list: string[]): HTMLElement | null {
  if (!list.length) return null;
  return h(
    "div.gst-warnings",
    { role: "status" },
    h("div.gst-warn-title", list.length === 1 ? tr("1 thing to check") : tr("{n} things to check", { n: list.length })),
    // Warnings come from Rust in English.
    h("ul", ...list.map((w) => h("li", { dir: "auto" }, w))),
  );
}

function gstinOf(gstin: string | null): string {
  return gstin ? ltr(gstin) : tr("No GSTIN");
}

function note(text: string): HTMLElement {
  return h("p.gst-note", text);
}

// ------------------------------------------------------------------ GSTR-1

export class Gstr1Report extends Report {
  title = "GSTR-1";
  protected override period = "range" as const;
  private data: Gstr1 | null = null;

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    const [g, names] = await Promise.all([api.gstr1(this.from, this.to), stateNames()]);
    this.data = g;
    const state = place(names, g.state);
    const fp = tr("Return period {period}", { period: ltr(`${g.fp.slice(0, 2)}/${g.fp.slice(2)}`) });
    this.heading(tr("GSTR-1"), `${gstinOf(g.gstin)} · ${state} · ${range(this.from, this.to)} · ${fp}`);
    this.resetPicks();
    const tbody = h("tbody");
    for (const s of g.summary) {
      tbody.append(
        this.pick(
          // Section labels are fixed text from returns.rs ("B2B invoices (4A, 4B)"), so they have translations.
          h("tr", h("td.strong", tr(s.label)), h("td.num", String(s.count || "")), ...taxCells(s.amounts)),
          () => void this.shell.push(new Gstr1SectionReport(this.shell, s.section, s.label, this.from, this.to)),
        ),
      );
    }
    this.bodyEl.replaceChildren(
      ...[warnings(g.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "table.rtable",
        head(["Section", false], ["Count", true], ...TAX_HEADS.map((t): [string, boolean] => [t, true])),
        tbody,
        h("tfoot", h("tr", h("td", tr("Net outward supplies")), h("td.num", ""), ...taxCells(g.net))),
      ),
      note(
        tr(
          "Credit and debit notes are shown at their own values; B2C small and the HSN summary are net of credit notes. {json} saves a draft GSTR-1 JSON in the offline-tool layout. It is not validated against the GST portal schema: open it in the GST Returns Offline Tool and check it before uploading. {bulk} saves the period's B2B, SEZ and export invoices and credit notes as one e-invoice JSON file for the IRP's bulk tool.",
          { json: ltr(EXPORT_JSON), bulk: ltr(EINVOICE_BULK) },
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

  override bottom() {
    const b = super.bottom();
    b.splice(b.length - 1, 0, { chord: EXPORT_JSON, label: "Export JSON", enabled: true }, { chord: EINVOICE_BULK, label: "e-Invoice JSON (bulk)", enabled: true });
    return b;
  }

  override key(e: KeyEv): boolean {
    if (e.chord === EXPORT_JSON) {
      void this.exportJson();
      return true;
    }
    if (e.chord === EINVOICE_BULK) {
      void import("./einvoice").then((m) => m.einvoiceBulk(this.shell, this.from, this.to));
      return true;
    }
    return super.key(e);
  }

  private async exportJson(): Promise<void> {
    try {
      const json = await api.gstr1Json(this.from, this.to);
      const name = `GSTR1-${this.data?.gstin ?? "draft"}-${this.data?.fp ?? ""}`.replace(/-$/, "");
      const path = await save({ defaultPath: `${name}.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const file = path.toLowerCase().endsWith(".json") ? path : `${path}.json`;
      await api.writeGstJson(file, json);
      this.shell.info(tr("Draft GSTR-1 saved to {file}. Check it in the GST offline tool before filing.", { file: ltr(file) }));
    } catch (e) {
      this.shell.error(e);
    }
  }
}

/** One GSTR-1 section: documents (Enter opens the voucher) or summary rows. */
class Gstr1SectionReport extends Report {
  constructor(
    shell: Shell,
    private section: Gstr1Section,
    public title: string,
    private from: string,
    private to: string,
  ) {
    super(shell);
  }

  async load(): Promise<void> {
    const [g, names] = await Promise.all([api.gstr1(this.from, this.to), stateNames()]);
    this.heading(`${ltr("GSTR-1")} · ${tr(this.title)}`, `${gstinOf(g.gstin)} · ${range(this.from, this.to)}`);
    this.resetPicks();
    const tbody = h("tbody");
    let thead: HTMLElement;
    let cols: number;
    if (DOC_SECTIONS.includes(this.section)) {
      const docs = g[this.section as "b2b" | "b2cl" | "cdnr" | "cdnur" | "exp"];
      thead = head(["Date", false], ["No.", false], ["Party", false], ["Place of Supply", false], ["Rate", false], ["Value", true], ...TAX_HEADS.map((t): [string, boolean] => [t, true]));
      cols = 11;
      for (const d of docs) {
        const kind = d.kind === "Invoice" ? "Sales" : d.kind;
        const label = d.kind === "Invoice" ? "" : ` (${tr(d.kind === "CreditNote" ? "Credit Note" : "Debit Note")})`;
        tbody.append(
          this.pick(
            h(
              "tr",
              h("td", D.show(d.date)),
              h("td", ltr(d.number) + label),
              h("td.strong", d.party_name, d.party_gstin ? h("span.muted", ` · ${ltr(d.party_gstin)}`) : null, d.typ ? h("span.muted", ` · ${ltr(d.typ)}`) : null),
              h("td", place(names, d.place_of_supply)),
              h("td", d.items.map((i) => rate(i.rate)).join(", ")),
              num(d.value),
              ...taxCells(docTotals(d)),
            ),
            () => void this.shell.push(new VoucherScreen(this.shell, kind, d.date, d.id)),
          ),
        );
      }
    } else if (this.section === "b2cs") {
      thead = taxHead("Supply", "Place of Supply", "Rate");
      cols = 8;
      for (const r of g.b2cs) {
        tbody.append(this.pick(h("tr", h("td", tr(r.inter_state ? "Inter-State" : "Intra-State")), h("td", place(names, r.place_of_supply)), h("td", rate(r.rate)), ...taxCells(r.amounts))));
      }
    } else if (this.section === "nil") {
      thead = head(["Supply", false], ["Nil-rated value", true]);
      cols = 2;
      for (const r of g.nil) {
        const what = r.inter_state
          ? tr(r.registered ? "Inter-State to registered persons" : "Inter-State to unregistered persons")
          : tr(r.registered ? "Intra-State to registered persons" : "Intra-State to unregistered persons");
        tbody.append(this.pick(h("tr", h("td", what), num(r.value))));
      }
    } else if (this.section === "hsn_b2b" || this.section === "hsn_b2c") {
      thead = head(["HSN/SAC", false], ["UQC", false], ["Quantity", true], ["Rate", false], ...TAX_HEADS.map((t): [string, boolean] => [t, true]));
      cols = 9;
      for (const r of g[this.section]) {
        tbody.append(
          this.pick(
            h(
              "tr",
              { class: r.hsn ? "" : "warnrow" },
              h("td", r.hsn || tr("(none)")),
              h("td", r.uqc),
              h("td.num", M.of(r.qty) === 0n ? "" : plain(r.qty)),
              h("td", rate(r.rate)),
              ...taxCells(r.amounts),
            ),
          ),
        );
      }
    } else {
      thead = head(["Document type", false], ["From", false], ["To", false], ["Total", true], ["Cancelled", true], ["Net issued", true]);
      cols = 6;
      for (const r of g.docs_issued) {
        tbody.append(
          // Document types are fixed text from returns.rs ("Invoices for outward supply").
          this.pick(h("tr", h("td", tr(r.doc_type)), h("td", ltr(r.from)), h("td", ltr(r.to)), h("td.num", String(r.total)), h("td.num", String(r.cancelled || "")), h("td.num", String(r.net_issued)))),
        );
      }
    }
    if (!tbody.children.length) tbody.append(h("tr.empty", h("td", { colSpan: cols }, tr("Nothing in this section for the period."))));
    this.bodyEl.replaceChildren(h("table.rtable", thead, tbody));
  }
}

// ----------------------------------------------------------------- GSTR-3B

export class Gstr3bReport extends Report {
  title = "GSTR-3B";
  protected override period = "range" as const;

  constructor(shell: Shell, private from = monthStart(today(shell)), private to = today(shell)) {
    super(shell);
  }

  async load(): Promise<void> {
    const [g, names] = await Promise.all([api.gstr3b(this.from, this.to), stateNames()]);
    this.render(g, names);
  }

  private render(g: Gstr3b, names: Map<string, string>): void {
    this.heading(tr("GSTR-3B"), `${gstinOf(g.gstin)} · ${place(names, g.state)} · ${range(this.from, this.to)}`);
    this.resetPicks();
    const gstr1 = () => void this.shell.push(new Gstr1Report(this.shell, this.from, this.to));
    const side = (title: string, table: HTMLElement) => h("div.tside", h("div.tside-title", tr(title)), table);

    const outward = h("tbody");
    for (const [label, a] of [
      ["(a) Outward taxable supplies (other than zero-rated, nil-rated and exempted)", g.outward_taxable],
      ["(b) Outward taxable supplies (zero-rated)", g.zero_rated],
      ["(c) Other outward supplies (nil-rated, exempted)", g.nil_exempt],
    ] as [string, GstAmounts][]) {
      outward.append(this.pick(h("tr", h("td", tr(label)), ...taxCells(a)), gstr1));
    }

    const places = h("tbody");
    for (const r of g.inter_state_unregistered) {
      places.append(this.pick(h("tr", h("td", place(names, r.place_of_supply)), num(r.taxable), num(r.igst))));
    }
    if (!g.inter_state_unregistered.length) places.append(h("tr.empty", h("td", { colSpan: 3 }, tr("None in this period."))));

    const itc = h("tbody", this.pick(h("tr", h("td", tr("(A)(5) All other ITC")), ...taxCells(g.itc).slice(1))));

    const net = h("tbody");
    for (const r of g.net) {
      const n = M.of(r.net);
      net.append(this.pick(h("tr", h("td", r.component), num(r.output), num(r.itc), h("td.num", { class: n < 0n ? "gst-credit" : "" }, M.fmt(r.net)))));
    }

    this.bodyEl.replaceChildren(
      ...[warnings(g.warnings)].filter((x): x is HTMLElement => x !== null),
      h(
        "div.gst-3b",
        side("3.1 Outward supplies", h("table.rtable", taxHead("Nature of supplies"), outward)),
        side("3.2 Inter-State supplies to unregistered persons", h("table.rtable", head(["Place of Supply", false], ["Taxable Value", true], ["IGST", true]), places)),
        side("4 Eligible ITC", h("table.rtable", head(["Details", false], ["IGST", true], ["CGST", true], ["SGST/UTGST", true], ["Cess", true]), itc)),
        side("Tax payable (output tax − ITC)", h("table.rtable", head(["Tax", false], ["Output tax", true], ["ITC", true], ["Net", true]), net)),
      ),
      note(
        tr(
          "Net is output tax less ITC for each tax on its own. The order of using IGST, CGST and SGST credit against each other (CGST Act s.49, Rule 88A) is not applied. Reverse-charge tax (3.1(d)) is paid in cash; reversals and the full Table 4 are in the report Input tax credit (GSTR-3B Table 4). A negative net is credit left over. Enter on 3.1 opens GSTR-1.",
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

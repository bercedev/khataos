// Overlay launchers built on the ListPicker: Find (Alt+F, in the top bar),
// Go To (Alt+G) and the Report Directory.
import { api } from "../api";
import { h } from "../dom";
import { hijriOn, inUse, LANGUAGES, language, setHijri, t } from "../i18n";
import * as M from "../money";
import { ListPicker, type PickerOptions, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, KINDS, type Context } from "../types";
import { LedgerForm } from "./ledger";
import {
  AgeingReport,
  BalanceSheetReport,
  ChartOfAccounts,
  DayBook,
  LedgerStatementReport,
  MsmeOverdueReport,
  OutstandingReport,
  ProfitLossReport,
  TrialBalanceReport,
} from "./reports";
import { GroupForm } from "./group";
import { SyncSettingsScreen, importFromTally } from "./company";
import { onlineAccessItems, openAccountantFile } from "./sharing";
import { VoucherScreen } from "./voucher";
import { StockJournalScreen } from "./stockjournal";
import { DashboardScreen, RatioAnalysisReport } from "./analysis";
import { Gstr1Report, Gstr3bReport } from "./gst";
import { Gstr2bReport } from "./gstr2b";
import { GstFlagsScreen, Gstr1AmendmentsReport, ItcReport, RcmRegisterReport, Rule37Screen } from "./gst_rcm";
import { Schedule3BalanceSheet, Schedule3Mapping, Schedule3Notes, Schedule3ProfitLoss } from "./schedule3";
import { Vat201Report } from "./vat";
import { KsaVatReturnReport } from "./vat_sa";
import { UkVatReturnReport } from "./vat_gb";
import { UstvaReport } from "./vat_de";
import { Ca3Report } from "./vat_fr";
import { AangifteReport } from "./vat_nl";
import { Modelo303Report } from "./vat_es";
import { Us1099ReportScreen, UsCertificatesScreen, UsSalesTaxReportScreen, UsUseTaxReportScreen } from "./tax_us";
import { MxCfdiList, MxIvaSummaryReport } from "./tax_mx";
import { MtdObligationsReport, MtdSettingsScreen } from "./mtd";
import { openIrpSettings } from "./irp";
import { ZatcaOnboardingScreen, ZatcaQueueReport } from "./zatca";
import { GstF5Report } from "./gst_sg";
import { SalaryTdsEstimateReport, TcsPayableReport, TcsQuarterlyReport, TdsNonResidentReport, TdsPayableReport, TdsQuarterlyReport, editTdsDeductor } from "./tds";
import { StockGroupForm, StockItemForm, StockSummaryReport, UnitForm } from "./inventory";
import { GodownForm, GodownSummaryReport } from "./godown";
import { CompanyDetailsForm, TaxRatesReport, ZatcaSellerForm } from "./settings";
import { UsersScreen } from "./signin";
import { reportProblem } from "../crash";
import { DeskScreen } from "./desk";
import {
  DENSITIES, MODES, REGIONS, TEXT_SIZES, THEMES, displayPrefs, previewDisplay, revertDisplay, themeInfo, zoomOf,
  type DisplayPrefs,
} from "../theme";

/** Opens a screen, or just does something (sync, import) and returns nothing. */
type Go = () => Screen | void;

/** A screen that is just a list picker. */
abstract class PickerScreen implements Screen {
  abstract title: string;
  el: HTMLElement;
  overlay = true;
  protected picker!: ListPicker<Go>;

  constructor(protected shell: Shell, protected input: HTMLInputElement, cls: string) {
    this.el = h(`section.launcher`, { class: cls });
  }

  protected build(
    title: string,
    source: (q: string) => Promise<PickItem<Go>[]> | PickItem<Go>[],
    extra: Partial<PickerOptions<Go>> = {},
  ): void {
    this.picker = new ListPicker<Go>({
      input: this.input,
      host: this.el,
      title,
      inline: true,
      columns: [t("Name"), ""],
      source,
      onPick: (it) => {
        this.close();
        const next = it.data();
        if (next) void this.shell.push(next);
      },
      ...extra,
    });
  }

  async init(): Promise<void> {
    await this.picker.open();
  }

  contexts(): Context[] {
    return ["ListField"];
  }

  focus(): void {
    this.input.focus();
    this.input.select();
  }

  key(e: KeyEv): boolean {
    return this.picker.key(e);
  }

  action(a: string): boolean {
    if (a === "Back") {
      this.close();
      return true;
    }
    return this.picker.action(a);
  }

  protected close(): void {
    this.shell.pop();
  }

  dispose(): void {
    this.picker.close();
  }
}

function fy(shell: Shell): [string, string] {
  return [shell.periodFrom(), shell.periodTo()];
}

/**
 * Fixed menu rows in the display language. The English label stays as an
 * alias, so typing the English name still finds a translated row.
 */
function localize(items: PickItem<Go>[]): PickItem<Go>[] {
  return items.map((i) => {
    const label = t(i.label);
    return {
      ...i,
      label,
      aliases: label === i.label ? i.aliases : [...(i.aliases ?? []), i.label],
      sub: i.sub && t(i.sub),
      section: i.section && t(i.section),
    };
  });
}

/** Tax and statutory rows that belong to one country's pack. They show only
 *  for a company of that country; every other row shows everywhere. */
const REPORT_COUNTRY: Record<string, string> = {
  ...Object.fromEntries(
    ["gstr1", "gstr3b", "gstr2b", "gstitc", "gstrcm", "gstr37", "gstflags", "gstr1a", "irpset", "tdspay", "tdsq", "tdsnr", "tcspay", "tcsq", "salary", "deductor", "s3bs", "s3pl", "s3notes", "s3map", "msme"].map((id) => [`r:${id}`, "IN"]),
  ),
  "r:vat201": "AE",
  "r:vatgb": "GB",
  "r:mtd": "GB",
  "r:mtdset": "GB",
  "r:ustva": "DE",
  "r:ca3": "FR",
  "r:aangifte": "NL",
  "r:modelo303": "ES",
  "r:ussales": "US",
  "r:us1099": "US",
  "r:ususe": "US",
  "r:uscert": "US",
  "r:mxiva": "MX",
  "r:mxcfdi": "MX",
  "r:vatsa": "SA",
  "r:gstf5": "SG",
};

function reportItems(shell: Shell): PickItem<Go>[] {
  const section = "Reports";
  const country = shell.company?.country?.code;
  const forCountry = (i: PickItem<Go>) => !REPORT_COUNTRY[i.id] || REPORT_COUNTRY[i.id] === country;
  return localize([
    { id: "r:tb", label: "Trial Balance", section, data: () => new TrialBalanceReport(shell) },
    { id: "r:db", label: "Day Book", section, data: () => new DayBook(shell) },
    { id: "r:pl", label: "Profit & Loss A/c", section, data: () => new ProfitLossReport(shell) },
    { id: "r:bs", label: "Balance Sheet", section, data: () => new BalanceSheetReport(shell) },
    { id: "r:coa", label: "Chart of Accounts", section, data: () => new ChartOfAccounts(shell) },
    { id: "r:ratio", label: "Ratio Analysis", section, data: () => new RatioAnalysisReport(shell) },
    { id: "r:gstr1", label: "GSTR-1 (outward supplies)", section: "GST", data: () => new Gstr1Report(shell) },
    { id: "r:gstr3b", label: "GSTR-3B (summary return)", section: "GST", data: () => new Gstr3bReport(shell) },
    { id: "r:gstr2b", label: "GSTR-2B reconciliation", section: "GST", data: () => new Gstr2bReport(shell) },
    { id: "r:gstitc", label: "Input tax credit (GSTR-3B Table 4)", section: "GST", data: () => new ItcReport(shell) },
    { id: "r:gstrcm", label: "Reverse charge register and self-invoices", section: "GST", data: () => new RcmRegisterReport(shell) },
    { id: "r:gstr37", label: "ITC to reverse for unpaid bills (Rule 37)", section: "GST", data: () => new Rule37Screen(shell) },
    { id: "r:gstflags", label: "Reverse charge and blocked credit flags", section: "GST", data: () => new GstFlagsScreen(shell) },
    { id: "r:gstr1a", label: "GSTR-1A: changes after filing", section: "GST", data: () => new Gstr1AmendmentsReport(shell) },
    { id: "r:vat201", label: "VAT 201 (UAE VAT return)", section: "VAT", data: () => new Vat201Report(shell) },
    { id: "r:vatgb", label: "VAT Return (UK, 9 boxes)", section: "VAT", data: () => new UkVatReturnReport(shell) },
    { id: "r:ustva", label: "UStVA (Germany VAT return)", section: "VAT", data: () => new UstvaReport(shell) },
    { id: "r:ca3", label: "CA3 (France VAT return)", section: "VAT", data: () => new Ca3Report(shell) },
    { id: "r:aangifte", label: "Aangifte omzetbelasting (Netherlands VAT return)", section: "VAT", data: () => new AangifteReport(shell) },
    { id: "r:modelo303", label: "Modelo 303 (Spain VAT return)", section: "VAT", data: () => new Modelo303Report(shell) },
    { id: "r:ussales", label: "Sales tax by jurisdiction (US)", section: "Sales tax", data: () => new UsSalesTaxReportScreen(shell) },
    { id: "r:us1099", label: "Form 1099 vendors (US)", section: "Sales tax", data: () => new Us1099ReportScreen(shell) },
    { id: "r:ususe", label: "Use tax on untaxed purchases (US)", section: "Sales tax", data: () => new UsUseTaxReportScreen(shell) },
    { id: "r:uscert", label: "Exemption certificates (US)", section: "Sales tax", data: () => new UsCertificatesScreen(shell) },
    { id: "r:mxiva", label: "IVA summary (Mexico)", section: "VAT", data: () => new MxIvaSummaryReport(shell) },
    { id: "r:mxcfdi", label: "CFDI invoices (Mexico)", section: "VAT", data: () => new MxCfdiList(shell) },
    { id: "r:mtd", label: "Making Tax Digital: VAT periods (UK)", section: "VAT", data: () => new MtdObligationsReport(shell) },
    { id: "r:mtdset", label: "Making Tax Digital settings (UK)", section: "VAT", data: () => new MtdSettingsScreen(shell) },
    { id: "r:irpset", label: "e-Invoice portal (IRP) settings", section: "GST", data: () => void openIrpSettings(shell) },
    { id: "r:vatsa", label: "VAT Return (Saudi Arabia)", section: "VAT", data: () => new KsaVatReturnReport(shell) },
    { id: "r:gstf5", label: "GST F5 (Singapore GST return)", section: "GST", data: () => new GstF5Report(shell) },
    { id: "r:tdspay", label: "TDS Payable", section: "TDS", data: () => new TdsPayableReport(shell) },
    { id: "r:tdsq", label: "TDS Quarterly Statement", section: "TDS", data: () => new TdsQuarterlyReport(shell) },
    { id: "r:tdsnr", label: "TDS on payments to non-residents", section: "TDS", data: () => new TdsNonResidentReport(shell) },
    { id: "r:tcspay", label: "TCS Payable", section: "TDS", data: () => new TcsPayableReport(shell) },
    { id: "r:tcsq", label: "TCS Quarterly Statement", section: "TDS", data: () => new TcsQuarterlyReport(shell) },
    { id: "r:salary", label: "Salary TDS estimate", section: "TDS", data: () => new SalaryTdsEstimateReport(shell) },
    { id: "r:deductor", label: "Deductor details (TAN)", section: "TDS", data: () => void editTdsDeductor(shell) },
    { id: "r:s3bs", label: "Balance Sheet (Schedule III)", section: "Statutory", data: () => new Schedule3BalanceSheet(shell) },
    { id: "r:s3pl", label: "Statement of Profit and Loss (Schedule III)", section: "Statutory", data: () => new Schedule3ProfitLoss(shell) },
    { id: "r:s3notes", label: "Notes to Accounts (Schedule III)", section: "Statutory", data: () => new Schedule3Notes(shell) },
    { id: "r:s3map", label: "Schedule III mapping", section: "Statutory", data: () => new Schedule3Mapping(shell) },
    { id: "r:dash", label: "Dashboard", section, data: () => new DashboardScreen(shell) },
    { id: "r:stock", label: "Stock Summary", section: "Inventory", data: () => new StockSummaryReport(shell) },
    { id: "r:godown", label: "Godown Summary", section: "Inventory", data: () => new GodownSummaryReport(shell, "godown") },
    { id: "r:batch", label: "Batch Summary", section: "Inventory", data: () => new GodownSummaryReport(shell, "batch") },
    { id: "r:or", label: "Outstanding Receivables", section: "Bills", data: () => new OutstandingReport(shell, true) },
    { id: "r:op", label: "Outstanding Payables", section: "Bills", data: () => new OutstandingReport(shell, false) },
    { id: "r:ar", label: "Ageing: Receivables", section: "Bills", data: () => new AgeingReport(shell, true) },
    { id: "r:ap", label: "Ageing: Payables", section: "Bills", data: () => new AgeingReport(shell, false) },
    { id: "r:msme", label: "MSME overdue payables (over 45 days)", section: "Bills", data: () => new MsmeOverdueReport(shell) },
  ].filter(forCountry));
}

/** Rows whose label (or an alias) holds every typed word, in any order: "vat saudi" finds "VAT Return (Saudi Arabia)". */
function matches(items: PickItem<Go>[], q: string): PickItem<Go>[] {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const hit = (text: string) => words.every((w) => text.toLowerCase().includes(w));
  return items.filter((i) => hit(i.label) || i.aliases?.some(hit));
}

async function ledgerItems(shell: Shell, q: string, section?: string): Promise<PickItem<Go>[]> {
  const [from, to] = fy(shell);
  const ledgers = await api.listLedgers(q, "all");
  return ledgers.map((l) => ({
    id: `l:${l.id}`,
    label: l.name,
    sub: M.drCr(M.of(l.balance), true) || l.group_name,
    section: section && t(section),
    data: () => new LedgerStatementReport(shell, l.id, from, to),
  }));
}

/** Alt+F: find a ledger by any part of its name and open its statement. */
export class FindScreen extends PickerScreen {
  title = "Find";
  constructor(shell: Shell) {
    super(shell, shell.searchInput, "find");
    this.build(t("Ledgers"), (q) => ledgerItems(shell, q));
  }

  protected override close(): void {
    this.input.value = "";
    this.input.blur();
    super.close();
  }
}

/** Alt+G: jump to any report, voucher type, master form or ledger. */
export class GoToScreen extends PickerScreen {
  title = "Go To";
  constructor(shell: Shell) {
    super(shell, h("input.field.launcher-input", { placeholder: t("Type to search reports, vouchers, ledgers"), spellcheck: false }), "goto");
    this.el.prepend(h("div.launcher-title", t("Go To")), this.input);
    const voucher = (k: keyof typeof KIND_LABEL) => ({
      label: t("{kind} Voucher", { kind: t(KIND_LABEL[k]) }),
      aliases: [`${KIND_LABEL[k]} Voucher`],
      section: t("Vouchers"),
    });
    const fixed = (): PickItem<Go>[] => [
      ...reportItems(shell),
      ...KINDS.map((k) => ({ id: `v:${k}`, ...voucher(k), data: () => new VoucherScreen(shell, k) })),
      { id: "v:StockJournal", ...voucher("StockJournal"), data: () => new StockJournalScreen(shell) },
      ...localize([
      { id: "m:ledger", label: "Create Ledger", section: "Masters", data: () => new LedgerForm(shell) },
      { id: "m:group", label: "Create Group", section: "Masters", data: () => new GroupForm(shell) },
      { id: "c:details", label: "Company details", sub: "tax ID, state, address", section: "Company", data: () => new CompanyDetailsForm(shell) },
      { id: "c:taxreg", label: "Tax registration", sub: "company details", section: "Company", data: () => new CompanyDetailsForm(shell) },
      { id: "c:rates", label: "Tax Rates", sub: "HSN/SAC rate list", section: "Company", data: () => new TaxRatesReport(shell) },
      { id: "c:invoice-design", label: "Invoice design", sub: "logo, template, bank details, terms", section: "Company", aliases: ["invoice template", "logo", "print design", "bank details"], data: () => void import("./invoice_design").then((m) => shell.push(new m.InvoiceDesignScreen(shell))) },
      { id: "u:import", label: "Import from Tally XML", section: "Utilities", data: () => void importFromTally(shell) },
      { id: "u:sync", label: "Sync settings", section: "Utilities", data: () => new SyncSettingsScreen(shell) },
      { id: "u:desk", label: "Accountant's Desk", sub: "to-dos, notes, due dates", section: "Utilities", aliases: ["desk", "to-do", "todo", "notes", "reminders", "calendar", "due dates", "compliance calendar"], data: () => new DeskScreen(shell) },
      { id: "h:report", label: "Report a problem", sub: "opens a GitHub issue you review first", section: "Help", aliases: ["bug", "crash", "feedback"], data: () => void reportProblem(shell) },
      ]),
    ];
    this.build(t("Go To"), async (q) => [...matches(fixed(), q), ...(await ledgerItems(shell, q, "Ledger statements"))]);
  }
}

/** "D" on the home menu: every report, ledger statements included. */
export class ReportDirectory extends PickerScreen {
  title = "Report Directory";
  constructor(shell: Shell) {
    super(shell, h("input.field.launcher-input", { placeholder: t("Type to filter"), spellcheck: false }), "directory");
    this.overlay = false;
    this.el.prepend(h("div.launcher-title", t("Report Directory")), this.input);
    this.build(t("Reports"), async (q) => [...matches(reportItems(shell), q), ...(await ledgerItems(shell, q, "Ledger statements"))]);
  }
}

/** A short menu of fixed choices, as an overlay picker, in the display language. */
export class MenuScreen extends PickerScreen {
  constructor(shell: Shell, readonly title: string, items: PickItem<Go>[]) {
    super(shell, h("input.field.launcher-input", { placeholder: t("Type to filter"), spellcheck: false }), "goto");
    this.el.prepend(h("div.launcher-title", t(title)), this.input);
    const rows = localize(items);
    this.build(t(title), (q) => matches(rows, q));
  }
}

/**
 * A menu whose cursor previews its choice at once (↑/↓, or typing to filter):
 * Enter keeps it, Esc puts back what was there. Items carry the choice as
 * `data`, which keeps it when called.
 */
export class PreviewMenu extends PickerScreen {
  private keptChoice = false;
  constructor(shell: Shell, readonly title: string, items: PickItem<Go>[], selectedId: string, preview: (id: string) => void) {
    super(shell, h("input.field.launcher-input", { placeholder: t("Type to filter"), spellcheck: false }), "goto");
    this.el.classList.add("preview");
    this.el.prepend(h("div.launcher-title", t(title)), this.input);
    this.el.append(h("div.launcher-hint", t("↑ ↓ to preview · Enter to keep · Esc to go back")));
    const rows = localize(items);
    this.build(t(title), (q) => matches(rows, q), {
      selectedId: () => selectedId,
      onCursor: (it) => {
        if (it) preview(it.id);
      },
      onPick: (it) => {
        this.keptChoice = true;
        it.data();
        this.close();
      },
    });
  }

  override dispose(): void {
    if (!this.keptChoice) revertDisplay();
    super.dispose();
  }
}

const percent = (n: number) => `${n}%`;

/** A theme's name in the display language: looked up as "Ledger (theme)", since plain "Ledger" is the account. */
const themeLabel = (name: string) => inUse(name, t(`${name} (theme)`));

/** The theme's name, with the region for Regional. */
function themeName(p: DisplayPrefs): string {
  if (p.theme === "regional") return t(REGIONS.find((r) => r.id === p.region)!.name);
  return themeLabel(themeInfo(p.theme).name);
}

/** Ctrl+K → Theme: every theme, then the regional palettes. */
export function themeMenu(shell: Shell): Screen {
  const p = displayPrefs();
  const ids = (id: string): Partial<DisplayPrefs> => {
    const [theme, region] = id.split(":");
    return region ? { theme: "regional", region: region as DisplayPrefs["region"] } : { theme: theme as DisplayPrefs["theme"] };
  };
  const items: PickItem<Go>[] = [
    ...THEMES.filter((x) => x.id !== "regional").map((x) => ({
      id: x.id,
      label: themeLabel(x.name),
      aliases: [x.name],
      sub: x.audience,
      section: "Themes",
      data: () => void shell.setDisplay(ids(x.id)),
    })),
    ...REGIONS.map((r) => ({
      id: `regional:${r.id}`,
      label: r.name,
      sub: r.sub,
      section: "Regional",
      aliases: ["regional", "region", "country"],
      data: () => void shell.setDisplay(ids(`regional:${r.id}`)),
    })),
  ];
  const selected = p.theme === "regional" ? `regional:${p.region}` : p.theme;
  return new PreviewMenu(shell, "Theme", items, selected, (id) => previewDisplay(ids(id)));
}

/** Ctrl+K → Light or dark. */
export function modeMenu(shell: Shell): Screen {
  const items: PickItem<Go>[] = MODES.map((m) => ({ id: m.id, label: m.name, data: () => void shell.setDisplay({ mode: m.id }) }));
  return new PreviewMenu(shell, "Light or dark", items, displayPrefs().mode, (id) => previewDisplay({ mode: id as DisplayPrefs["mode"] }));
}

/** Ctrl+K → Density: row height and spacing, with any theme. */
export function densityMenu(shell: Shell): Screen {
  const items: PickItem<Go>[] = DENSITIES.map((d) => ({ id: d.id, label: d.name, sub: d.sub, data: () => void shell.setDisplay({ density: d.id }) }));
  return new PreviewMenu(shell, "Density", items, displayPrefs().density, (id) => previewDisplay({ density: id as DisplayPrefs["density"] }));
}

/** Ctrl+K → Text size: everything grows together, rows and keys too. */
export function textSizeMenu(shell: Shell): Screen {
  const p = displayPrefs();
  const items: PickItem<Go>[] = TEXT_SIZES.map((n) => {
    // Comfort adds its own 120 % (capped at 135 % in all): say what the screen will really show.
    const shown = Math.round(zoomOf({ ...p, text: n }) * 100);
    return {
      id: String(n),
      label: percent(n),
      aliases: [String(n)],
      sub: shown !== n ? t("{size} with {theme}", { size: percent(shown), theme: themeLabel(themeInfo(p.theme).name) }) : n === 100 ? t("usual size") : undefined,
      data: () => void shell.setDisplay({ text: n }),
    };
  });
  return new PreviewMenu(shell, "Text size", items, String(p.text), (id) => previewDisplay({ text: Number(id) }));
}

/** The four display rows of Ctrl+K, each with what is in use. */
function displayItems(shell: Shell): PickItem<Go>[] {
  const p = displayPrefs();
  return [
    { id: "theme", label: "Theme", sub: themeName(p), section: "Display", aliases: ["colours", "colors", "appearance", "look"], data: () => themeMenu(shell) },
    { id: "mode", label: "Light or dark", sub: t(MODES.find((m) => m.id === p.mode)!.name), section: "Display", aliases: ["dark mode", "light mode", "night"], data: () => modeMenu(shell) },
    { id: "density", label: "Density", sub: t(DENSITIES.find((d) => d.id === p.density)!.name), section: "Display", aliases: ["compact", "rows", "spacing"], data: () => densityMenu(shell) },
    { id: "text-size", label: "Text size", sub: percent(p.text), section: "Display", aliases: ["font size", "zoom", "larger text", "bigger"], data: () => textSizeMenu(shell) },
  ];
}

/** C on the home menu: which master to create. */
export function createMenu(shell: Shell): Screen {
  return new MenuScreen(shell, "Create", [
    { id: "ledger", label: "Ledger", section: "Accounting Masters", data: () => new LedgerForm(shell) },
    { id: "group", label: "Group", section: "Accounting Masters", data: () => new GroupForm(shell) },
    { id: "stock-item", label: "Stock Item", section: "Inventory Masters", data: () => new StockItemForm(shell) },
    { id: "stock-group", label: "Stock Group", section: "Inventory Masters", data: () => new StockGroupForm(shell) },
    { id: "unit", label: "Unit", section: "Inventory Masters", data: () => new UnitForm(shell) },
    { id: "godown", label: "Godown", section: "Inventory Masters", data: () => new GodownForm(shell) },
    { id: "tax-rates", label: "Tax Rates", sub: "HSN/SAC rate list", section: "Tax", data: () => new TaxRatesReport(shell) },
  ]);
}

/** Alt+K: company actions, including sync. */
export function companyMenu(shell: Shell): Screen {
  const chord = (a: string) => shell.km.chordIn(a, ["Global"]) ?? undefined;
  return new MenuScreen(shell, "Company", [
    { id: "sync", label: "Sync now", sub: chord("SyncNow"), data: () => void shell.runShellAction("SyncNow") },
    { id: "sync-settings", label: "Sync settings", data: () => new SyncSettingsScreen(shell) },
    { id: "switch", label: "Switch company", sub: chord("SwitchCompany"), data: () => void shell.runShellAction("SwitchCompany") },
    { id: "shut", label: "Shut company", sub: chord("ShutCompany"), data: () => void shell.runShellAction("ShutCompany") },
    { id: "details", label: "Company details", sub: "tax registration, address", section: "Tax", data: () => new CompanyDetailsForm(shell) },
    { id: "invoice-design", label: "Invoice design", sub: "logo, template, bank details, terms", section: "Invoices", data: () => void import("./invoice_design").then((m) => shell.push(new m.InvoiceDesignScreen(shell))) },
    { id: "tax-rates", label: "Tax Rates", sub: "HSN/SAC rate list", section: "Tax", data: () => new TaxRatesReport(shell) },
    ...(shell.company?.country?.code === "SA"
      ? [
          { id: "zatca-seller", label: "Seller details (ZATCA)", sub: "CRN, national address", section: "Tax", data: () => new ZatcaSellerForm(shell) },
          { id: "zatca-onboard", label: "ZATCA onboarding", sub: "connect this computer (OTP from Fatoora)", section: "Tax", data: () => new ZatcaOnboardingScreen(shell) },
          { id: "zatca-queue", label: "ZATCA queue", sub: "documents waiting to be reported or cleared", section: "Tax", data: () => new ZatcaQueueReport(shell) },
        ]
      : []),
    ...onlineAccessItems(shell),
    { id: "users", label: "Users and passwords", sub: "who may open this company", section: "Security", data: () => new UsersScreen(shell) },
    ...(shell.session?.user
      ? [{ id: "sign-out", label: "Sign out", sub: shell.session.user, section: "Security", data: () => void shell.signOut() }]
      : []),
    { id: "language", label: "Language: English / العربية", sub: chord("DisplayLanguage"), section: "Display", data: () => languageMenu(shell) },
    { id: "theme", label: "Theme, text size and density", sub: chord("DisplayLanguage"), section: "Display", aliases: ["dark mode", "colours", "colors", "font size"], data: () => languageMenu(shell) },
    { id: "report", label: "Report a problem", sub: "opens a GitHub issue you review first", section: "Help", aliases: ["bug", "crash", "feedback"], data: () => void reportProblem(shell) },
  ]);
}

/** Ctrl+K (and Company → Language): the display language, each named in itself, then theme and text size. */
export function languageMenu(shell: Shell): Screen {
  return new MenuScreen(
    shell,
    "Language and display",
    [...LANGUAGES.map((l) => ({
      id: l.code,
      label: l.name,
      aliases: [l.english],
      sub: l.code === language() ? t("in use") : l.english,
      data: () => void shell.setLanguage(l.code),
    })),
    {
      // A display preference, kept like the language; the books keep Gregorian dates.
      id: "hijri",
      label: "Show Hijri dates",
      aliases: ["hijri", "هجري"],
      sub: hijriOn() ? t("On") : t("Off"),
      data: () => {
        setHijri(!hijriOn());
        shell.info(hijriOn() ? "Hijri dates on. Screens already open change when you open them again." : "Hijri dates off.");
      },
    },
    ...displayItems(shell),
    // Ctrl+K works with no company open, so a problem can be reported from the company list too.
    { id: "report", label: "Report a problem", sub: "opens a GitHub issue you review first", section: "Help", aliases: ["bug", "crash", "feedback"], data: () => void reportProblem(shell) },
  ]);
}

/** Alt+O: import. */
export function importMenu(shell: Shell): Screen {
  return new MenuScreen(shell, "Import", [
    { id: "tally", label: "Import from Tally XML", sub: "masters + Day Book", data: () => void importFromTally(shell) },
    { id: "khatapkg", label: "Open accountant file (.khatapkg)", sub: "encrypted, from KhataOS", data: () => void openAccountantFile(shell) },
  ]);
}

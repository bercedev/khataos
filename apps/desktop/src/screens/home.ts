// Home menu (the Gateway equivalent). Single-letter hotkeys come from the
// keymap's Gateway context and are highlighted inside each word (L2).
import { h, hotkeyLabel } from "../dom";
import * as D from "../dates";
import { t } from "../i18n";
import * as M from "../money";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context } from "../types";
import { BalanceSheetReport, ChartOfAccounts, DayBook, ProfitLossReport } from "./reports";
import { ReportDirectory, createMenu } from "./launcher";
import { alterMenu } from "./alter";
import { DashboardScreen, RatioAnalysisReport } from "./analysis";
import { bankingMenu } from "./banking";
import { StockSummaryReport } from "./inventory";
import { VoucherScreen } from "./voucher";
import { DeskScreen, dueSoon } from "./desk";
import { backupApi, type BackupStatus } from "../backup-api";
import { backupTime } from "./backup";

interface Item {
  id: string; // GatewayItem name
  label: string;
  run?: (shell: Shell) => void;
}

const SECTIONS: { title: string; items: Item[] }[] = [
  {
    title: "Masters",
    items: [
      { id: "Create", label: "Create", run: (s) => void s.push(createMenu(s)) },
      { id: "Alter", label: "Alter", run: (s) => void s.push(alterMenu(s)) },
      { id: "ChartOfAccounts", label: "Chart of Accounts", run: (s) => void s.push(new ChartOfAccounts(s)) },
    ],
  },
  {
    title: "Transactions",
    items: [
      { id: "Vouchers", label: "Vouchers", run: (s) => void s.push(new VoucherScreen(s, "Payment")) },
      { id: "DayBook", label: "Day Book", run: (s) => void s.push(new DayBook(s)) },
    ],
  },
  {
    title: "Utilities",
    items: [
      { id: "Banking", label: "Banking", run: (s) => void s.push(bankingMenu(s)) },
      { id: "Desk", label: "To-dos and Due Dates", run: (s) => void s.push(new DeskScreen(s)) },
    ],
  },
  {
    title: "Reports",
    items: [
      { id: "BalanceSheet", label: "Balance Sheet", run: (s) => void s.push(new BalanceSheetReport(s)) },
      { id: "ProfitAndLoss", label: "Profit & Loss A/c", run: (s) => void s.push(new ProfitLossReport(s)) },
      { id: "StockSummary", label: "Stock Summary", run: (s) => void s.push(new StockSummaryReport(s)) },
      { id: "RatioAnalysis", label: "Ratio Analysis", run: (s) => void s.push(new RatioAnalysisReport(s)) },
      { id: "MoreReports", label: "Report Directory", run: (s) => void s.push(new ReportDirectory(s)) },
      { id: "Dashboard", label: "Dashboard", run: (s) => void s.push(new DashboardScreen(s)) },
    ],
  },
];

const QUIT: Item = { id: "Quit", label: "Quit", run: (s) => s.runShellAction("QuitApp") };

export class HomeScreen implements Screen {
  title = "Home";
  el: HTMLElement;
  private rows: { item: Item; el: HTMLElement }[] = [];
  private cursor = 0;
  private infoEl = h("div.home-info");
  /** The desk's "due soon" count (null: not readable). */
  private due: { overdue: number; soon: number } | null = null;
  /** Backups of this company on this computer (null: not readable). */
  private backup: BackupStatus | null = null;

  constructor(private shell: Shell) {
    const menu = h("nav.home-menu", { "aria-label": t("Home menu") });
    for (const section of [...SECTIONS, { title: "", items: [QUIT] }]) {
      if (section.title) menu.append(h("div.home-section", t(section.title)));
      for (const item of section.items) {
        // The letter is a physical key: on an Arabic layout C still opens Create (keys.ts).
        const chord = this.shell.km.chordsFor(`Menu:${item.id}`, "Gateway")[0] ?? "";
        const el = h(
          "button.home-item",
          { class: item.run ? "" : "off", tabIndex: -1, onclick: () => this.open(item), title: item.run ? undefined : t("Not available yet") },
          h("span", hotkeyLabel(t(item.label), chord)),
          item.run ? null : h("span.soon", t("soon")),
        );
        this.rows.push({ item, el });
        menu.append(el);
      }
    }
    this.el = h("section.home", this.infoEl, h("div.home-menu-wrap", h("div.home-menu-title", t("Menu")), menu));
    this.cursor = this.rows.findIndex((r) => r.item.run);
  }

  async init(): Promise<void> {
    await this.shell.refreshCompany();
    this.due = await dueSoon(this.shell);
    this.backup = await backupApi.status().catch(() => null);
    this.paintInfo();
    // One hint at a time: the theme hint waits for the next open when the Arabic one shows.
    if (!this.shell.suggestLanguage()) this.shell.suggestTheme();
  }

  rebuild(): Screen {
    return new HomeScreen(this.shell);
  }

  resume(): void {
    void this.shell
      .refreshCompany()
      .then(async () => {
        this.due = await dueSoon(this.shell);
        this.backup = await backupApi.status().catch(() => null);
      })
      .then(() => this.paintInfo());
  }

  private paintInfo(): void {
    const c = this.shell.company;
    if (!c) return;
    const diff = M.of(c.opening_difference);
    this.infoEl.replaceChildren(
      h("div.home-company", c.name),
      h("dl.home-facts",
        h("dt", t("Country")), h("dd", c.country ? `${t(c.country.name)} · ${c.country.currency}` : t(c.read_only ? "Not set by the owner yet" : "Not set")),
        h("dt", t("Current period")), h("dd", `${D.show(this.shell.periodFrom())} – ${D.show(this.shell.periodSet() ? this.shell.periodTo() : D.fyEnd(this.shell.periodFrom()))}`, h("span.muted", " · Alt+F2")),
        h("dt", t("Current date")), h("dd", t("{date}, {weekday}", { date: D.show(c.today), weekday: D.weekday(c.today) }), h("span.muted", " · F2")),
        h("dt", t("Ledgers")), h("dd", String(c.ledger_count)),
        h("dt", t("Vouchers")), h("dd", String(c.voucher_count)),
        ...(this.due
          ? [h("dt", t("Due soon")), h("dd", { class: this.due.overdue ? "warn" : "" }, t("{soon} in the next 7 days · {overdue} overdue", { soon: this.due.soon, overdue: this.due.overdue }), h("span.muted", " · T"))]
          : []),
        ...(this.backup
          ? [
              h("dt", t("Last backup")),
              h(
                "dd",
                { class: this.backup.remind ? "warn" : "" },
                this.backup.last ? backupTime(this.backup.last.at) : t("None yet"),
                h("span.muted", this.backup.remind ? ` · ${t("Alt+Y to back up now")}` : " · Alt+Y"),
              ),
            ]
          : []),
        h("dt", t("Opening balances")), h("dd", { class: diff === 0n ? "" : "warn" }, diff === 0n ? t("Balanced") : t("Difference {amount}", { amount: M.drCr(diff) })),
        h("dt", t("Signed in as")), h("dd", c.user),
        h("dt", t("Company file")), h("dd.path", c.path),
      ),
      h("p.home-hint", t("Press a highlighted letter, or move with ↑ ↓ and press Enter.")),
    );
  }

  contexts(): Context[] {
    return ["Gateway"];
  }

  focus(): void {
    (document.activeElement as HTMLElement | null)?.blur?.();
    this.paint();
  }

  private paint(): void {
    this.rows.forEach((r, i) => r.el.classList.toggle("cur", i === this.cursor));
  }

  private open(item: Item): void {
    this.cursor = this.rows.findIndex((r) => r.item === item);
    this.paint();
    if (item.run) item.run(this.shell);
    else this.shell.warn(t("{what} isn't available in this version yet.", { what: t(item.label) }));
  }

  private move(delta: number): void {
    const n = this.rows.length;
    let i = this.cursor;
    for (let k = 0; k < n; k++) {
      i = (((i + delta) % n) + n) % n;
      if (this.rows[i].item.run) break;
    }
    this.cursor = i;
    this.paint();
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Up") this.move(-1);
    else if (e.chord === "Down") this.move(1);
    else if (e.chord === "Enter") this.open(this.rows[this.cursor].item);
    else return false;
    return true;
  }

  action(a: string): boolean {
    if (!a.startsWith("Menu:")) return false;
    const row = this.rows.find((r) => r.item.id === a.slice(5));
    if (row) this.open(row.item);
    return Boolean(row);
  }

  bottom() {
    return [this.shell.btn("Back", this.contexts(), "Quit"), this.shell.btn("SwitchCompany", this.contexts(), "Company")].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }
}

// Startup: pick a company file, create a new one, or open one by path.
// Files default to the app data directory; one SQLite file per company.
import { api } from "../api";
import { h } from "../dom";
import { t } from "../i18n";
import { isMobile } from "../platform";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import type { CompanyFile, Context } from "../types";

type Mode = "list" | "create" | "open";

function lastPath(): string | null {
  try {
    return localStorage.getItem("khataos.lastCompany");
  } catch {
    return null;
  }
}

export class StartupScreen implements Screen {
  title = "Select Company";
  el: HTMLElement;
  private mode: Mode = "list";
  private dir = "";
  private files: CompanyFile[] = [];
  private listInput = h("input.field", { placeholder: t("Type to filter companies"), spellcheck: false });
  private nameIn = h("input.field", { spellcheck: false, autocomplete: "off" });
  private pathIn = h("input.field.path", { spellcheck: false, autocomplete: "off" });
  private pathEdited = false;
  private at = 0;
  private panel = h("div.startup-panel");
  private picker: ListPicker<CompanyFile>;

  constructor(private shell: Shell) {
    this.el = h(
      "section.startup",
      h(
        "div.startup-hero",
        h("div.startup-mark"),
        h("h1", "Khata", h("b", "OS")),
        h(
          "p",
          isMobile()
            ? t("Accounting that works offline. Your books live in one file on this device.")
            : t("Accounting that works offline. Your books live in one file on this computer."),
        ),
      ),
      this.panel,
    );
    this.nameIn.addEventListener("input", () => void this.suggestPath());
    this.pathIn.addEventListener("input", () => (this.pathEdited = true));
    this.nameIn.addEventListener("focus", () => (this.at = 0));
    this.pathIn.addEventListener("focus", () => (this.at = 1));
    this.picker = new ListPicker<CompanyFile>({
      input: this.listInput,
      host: this.panel,
      title: "Companies",
      inline: true,
      columns: ["Company", "File"],
      source: (q) => {
        const s = q.toLowerCase();
        return this.files
          .filter((f) => f.name.toLowerCase().includes(s) || f.path.toLowerCase().includes(s))
          .map<PickItem<CompanyFile>>((f) => ({ id: f.path, label: f.name, sub: f.path.split(/[\\/]/).pop(), data: f }));
      },
      actions: () => [
        { id: "create", label: t("Create Company"), hint: this.shell.km.chordIn("CreateMasterInline", ["ListField"]) ?? "", run: (t) => this.show("create", t) },
        { id: "open", label: t("Open Company File…"), run: () => this.show("open") },
        { id: "language", label: t("Language: English / العربية"), hint: this.shell.km.chordIn("DisplayLanguage", ["Global"]) ?? "", run: () => this.shell.runShellAction("DisplayLanguage") },
      ],
      create: { noun: "company", run: (t) => this.show("create", t) },
      selectedId: () => lastPath(),
      onPick: (it) => void this.openPath(it.data.path),
      noMatch: (text) => t('No company matches "{text}"', { text }),
    });
  }

  async init(): Promise<void> {
    this.dir = await api.defaultCompanyDir();
    this.files = await api.listCompanyFiles(this.dir);
    const last = lastPath();
    if (last && !this.files.some((f) => f.path === last)) {
      const parent = last.replace(/[\\/][^\\/]*$/, "");
      const there = await api.listCompanyFiles(parent);
      this.files.push(...there.filter((f) => f.path === last));
    }
    this.show(this.files.length ? "list" : "create");
  }

  private show(mode: Mode, text = ""): void {
    this.mode = mode;
    this.picker.close();
    this.at = 0;
    if (mode === "list") {
      this.listInput.value = "";
      this.panel.replaceChildren(h("label.frow", h("span.flabel", t("Company")), this.listInput));
      void this.picker.open();
    } else if (mode === "create") {
      this.nameIn.value = text;
      this.pathEdited = false;
      this.panel.replaceChildren(
        h("div.panel-title", t("Create Company")),
        h("label.frow", h("span.flabel", t("Company name")), this.nameIn),
        // Phones keep company files in the app's own storage, so there is no path to choose.
        ...(isMobile() ? [] : [h("label.frow", h("span.flabel", t("Company file")), this.pathIn)]),
        h("p.note", isMobile()
          ? t("Your company is saved on this device. Press Enter or Accept to create it.")
          : t("Enter moves to the next field. Accept with Ctrl+A, or Enter on the last field.")),
      );
      void this.suggestPath();
    } else {
      this.pathIn.value = lastPath() ?? this.join("");
      this.pathEdited = true;
      this.panel.replaceChildren(h("div.panel-title", t("Open Company File")), h("label.frow", h("span.flabel", t("Company file")), this.pathIn));
    }
    this.focus();
    this.shell.refresh();
  }

  private join(file: string): string {
    const sep = this.dir.includes("\\") ? "\\" : "/";
    return this.dir.replace(/[\\/]$/, "") + sep + file;
  }

  private async suggestPath(): Promise<void> {
    if (this.pathEdited) return;
    this.pathIn.value = this.join(await api.companyFileName(this.nameIn.value || "company"));
  }

  contexts(): Context[] {
    return this.mode === "list" ? ["ListField"] : ["Dialog"];
  }

  rebuild(): Screen {
    return new StartupScreen(this.shell);
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    const ctx = this.contexts();
    return [this.shell.btn("Accept", ctx, "Accept", this.mode !== "list"), this.shell.btn("Back", ctx, this.mode === "list" ? "Quit" : "Back")].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }

  focus(): void {
    const inp = this.mode === "list" ? this.listInput : this.mode === "open" || this.at === 1 ? this.pathIn : this.nameIn;
    inp.focus();
    inp.select();
  }

  key(e: KeyEv): boolean {
    if (this.mode === "list") return this.picker.key(e);
    if (e.chord === "Up" && this.mode === "create" && this.at === 1) {
      this.at = 0;
      this.focus();
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    if (this.mode === "list") return this.picker.action(a);
    switch (a) {
      case "Accept":
        void this.submit();
        return true;
      case "NextField":
        if (this.mode === "create" && this.at === 0) {
          if (!this.nameIn.value.trim()) this.shell.warn(t("Enter the company name."));
          else if (isMobile()) void this.confirm();
          else {
            this.at = 1;
            this.focus();
          }
        } else void this.confirm();
        return true;
      case "PreviousField":
        if (this.at === 1 && this.mode === "create") {
          this.at = 0;
          this.focus();
        }
        return true;
      case "Back": {
        const inp = this.mode === "open" || this.at === 1 ? this.pathIn : this.nameIn;
        if (inp.value) {
          inp.value = "";
          if (inp === this.nameIn) void this.suggestPath();
        } else if (this.files.length) this.show("list");
        else this.shell.runShellAction("QuitApp");
        return true;
      }
    }
    return false;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask(t(this.mode === "create" ? "Create this company?" : "Open this company?"))) await this.submit();
    else {
      this.at = 0;
      this.focus();
    }
  }

  private async submit(): Promise<void> {
    if (this.mode === "open") return this.openPath(this.pathIn.value.trim());
    const name = this.nameIn.value.trim();
    if (!name) return this.shell.warn(t("Enter the company name."));
    if (isMobile()) await this.suggestPath();
    try {
      const info = await api.createCompany(this.pathIn.value.trim(), name);
      this.shell.info(t("Created {name}.", { name: info.name }));
      await this.shell.openCompany(info);
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async openPath(path: string): Promise<void> {
    try {
      await this.shell.openCompany(await api.openCompany(path));
    } catch (e) {
      this.shell.error(e);
    }
  }

  dispose(): void {
    this.picker.close();
  }
}

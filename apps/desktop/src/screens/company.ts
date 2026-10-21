// Company-level flows: sync settings, Sync now (with rebase on conflict)
// and the Tally migration wizard.
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { h } from "../dom";
import { ltr, t } from "../i18n";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context } from "../types";
import { HomeScreen } from "./home";
import { ImportReportScreen, RebaseReviewScreen } from "./reports";

/** Server URL, company id and token, stored in this company file only. */
export class SyncSettingsScreen implements Screen {
  title = "Sync Settings";
  el = h("section.ledger");
  private inputs: HTMLInputElement[];
  private at = 0;
  private hasToken = false;
  private note = h("p.note");

  constructor(private shell: Shell) {
    const field = (attrs: Record<string, unknown> = {}) => h("input.field", { spellcheck: false, autocomplete: "off", ...attrs });
    this.inputs = [field({ placeholder: "https://books.example.in" }), field({ placeholder: t("UUID from your server") }), field({ type: "password" })];
    this.inputs.forEach((inp, i) => inp.addEventListener("focus", () => (this.at = i)));
    this.el.append(
      h(
        "div.form.master",
        h(
          "div.form-main",
          h("label.frow", h("span.flabel", t("Server URL")), this.inputs[0]),
          h("label.frow", h("span.flabel", t("Company id")), this.inputs[1]),
          h("label.frow", h("span.flabel", t("Access token")), this.inputs[2]),
        ),
        this.note,
      ),
    );
  }

  async init(): Promise<void> {
    const s = await api.syncSettings();
    this.inputs[0].value = s.url;
    this.inputs[1].value = s.company_id;
    this.hasToken = s.has_token;
    this.inputs[2].placeholder = s.has_token ? t("Saved. Leave blank to keep it") : t("Paste the token your server issued");
    this.note.textContent = s.last_status
      ? t("These settings stay in this company file on this computer; they are never synced. Last sync: {status}.", { status: ltr(s.last_status) })
      : t("These settings stay in this company file on this computer; they are never synced. This company has not been synced yet.");
  }

  contexts(): Context[] {
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    return [this.shell.btn("Accept", this.contexts(), "Save"), this.shell.btn("Back", this.contexts())].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }

  focus(): void {
    this.inputs[this.at].focus();
    this.inputs[this.at].select();
  }

  private go(i: number): void {
    this.at = i;
    this.focus();
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Up") {
      if (this.at > 0) this.go(this.at - 1);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    switch (a) {
      case "NextField":
        if (this.at < this.inputs.length - 1) this.go(this.at + 1);
        else void this.confirm();
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "Accept":
        void this.save();
        return true;
      case "Back":
        if (this.inputs[this.at].value) this.inputs[this.at].value = "";
        else this.shell.pop();
        return true;
    }
    return false;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask(t("Save sync settings?"))) await this.save();
    else this.go(0);
  }

  private async save(): Promise<void> {
    if (!this.hasToken && !this.inputs[2].value.trim()) {
      this.go(2);
      return this.shell.warn(t("Paste the access token from your server."));
    }
    try {
      await api.setSyncSettings(this.inputs[0].value, this.inputs[1].value, this.inputs[2].value);
      this.shell.info(t("Sync settings saved. Press Ctrl+Alt+S to sync now."));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

/** Sync now; on a conflict offer to rebase, then show what happened. */
export async function syncNow(shell: Shell): Promise<void> {
  if (!shell.company) return;
  shell.info(t("Syncing…"));
  try {
    const r = await shell.transition(api.syncNow());
    await shell.refreshCompany();
    shell.refresh();
    if (r.status !== "conflict") {
      shell.info(r.message);
      // New entries from the server: reopen Home when nothing is being edited.
      if (r.status === "pulled" && shell.depth() === 1) await shell.reset(new HomeScreen(shell));
      else if (r.status === "pulled") shell.warn(t("New entries arrived. Reopen reports to see them."));
      return;
    }
    const yes = await shell.ask(t("{result}. Rebase your changes on the server's?", { result: ltr(r.message) }), { yesFirst: false });
    if (!yes) {
      shell.warn(t("Nothing was changed. Your offline entries stay on this computer until you rebase."));
      return;
    }
    const report = await shell.transition(api.rebase());
    await shell.refreshCompany();
    await shell.reset(new HomeScreen(shell));
    await shell.push(new RebaseReviewScreen(shell, report));
  } catch (e) {
    await shell.refreshCompany();
    shell.refresh();
    shell.error(e);
  }
}

/** Alt+O → Import from Tally XML: choose files, import, show the report. */
export async function importFromTally(shell: Shell): Promise<void> {
  if (!shell.company) return shell.warn(t("Open a company first, then import into it."));
  let picked: string | string[] | null;
  try {
    picked = await open({
      multiple: true,
      title: t("Choose Tally XML exports (masters and Day Book)"),
      filters: [{ name: "Tally XML", extensions: ["xml", "XML"] }],
    });
  } catch (e) {
    return shell.error(e);
  }
  const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
  if (!paths.length) return;
  shell.info(t("Importing {n} file(s)…", { n: paths.length }));
  try {
    const summary = await shell.transition(api.importTally(paths));
    await shell.refreshCompany();
    shell.refresh();
    await shell.push(new ImportReportScreen(shell, summary));
  } catch (e) {
    shell.error(e);
  }
}

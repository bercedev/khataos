// Data menu (Alt+Y): Backup, Restore and Automatic backups
// (src-tauri/src/backup.rs, crates/khata-backup; docs/guide/backup.md).
import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { backupApi, type BackupStatus, type RestorePreview } from "../backup-api";
import * as D from "../dates";
import { h } from "../dom";
import { ltr, t } from "../i18n";
import { isMobile } from "../platform";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context } from "../types";
import { choose, form } from "./sharing";
import "../sharing.css";

const pad = (n: number) => String(n).padStart(2, "0");

/** A backup's time in this computer's time zone: "21-Oct-2026 14:30". */
export function backupTime(at: string): string {
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return at;
  return `${D.show(`${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`)} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "840 KB", "12.4 MB". */
export function fileSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** "Sharma & Sons" → "sharma-sons", as the backend names backup files. */
function slug(name: string): string {
  const s = name
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-+|-+$/g, "");
  return s || "company";
}

function stamp(): string {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}_${pad(d.getHours())}${pad(d.getMinutes())}`;
}

const checkPassword = ([a, b]: string[]) =>
  a.length < 8 ? t("Use at least 8 characters.") : a !== b ? t("The two passwords differ.") : null;

/** Data → Backup: a checked copy of the open company, in its backup folder (desktop) or where the user saves it (phones). */
export async function backupNow(shell: Shell): Promise<void> {
  const company = shell.company;
  if (!company) return shell.warn(t("Open a company first, then back it up."));
  let st: BackupStatus;
  try {
    st = await backupApi.status();
  } catch (e) {
    return shell.error(e);
  }
  const phone = isMobile();
  for (;;) {
    type Pick = "now" | "encrypted" | "folder";
    const options: { key: Pick; label: string; hint?: string }[] = phone
      ? [
          { key: "now", label: "Back up now", hint: "choose where to save it" },
          { key: "encrypted", label: "Encrypted with a password", hint: "choose where to save it" },
        ]
      : [
          { key: "now", label: "Back up now", hint: ltr(st.folder) },
          { key: "encrypted", label: "Encrypted with a password", hint: "into the same folder" },
          ...(st.can_change ? [{ key: "folder" as Pick, label: "Folder: choose another", hint: "remembered on this computer" }] : []),
        ];
    const what = await choose(shell, "Backup", options, "now");
    if (!what) return;
    if (what === "folder") {
      try {
        const dir = await open({ directory: true, title: t("Choose the backup folder"), defaultPath: st.folder });
        if (typeof dir === "string" && dir) st = await backupApi.setPrefs({ ...st.prefs, folder: dir });
      } catch (e) {
        shell.error(e);
      }
      continue;
    }
    let password: string | null = null;
    if (what === "encrypted") {
      const p = await form(
        shell,
        "Password for the backup",
        [
          { label: "Password", secret: true },
          { label: "Repeat", secret: true },
        ],
        checkPassword,
        "Keep the password safe: without it nobody can restore this backup, and KhataOS doesn't keep it.",
      );
      if (!p) continue;
      password = p[0];
    }
    let target: string | null = null;
    if (phone) {
      try {
        target = await save({
          title: t("Save the backup"),
          defaultPath: `${slug(company.name)}_${stamp()}.khatabak`,
          filters: [{ name: t("KhataOS backup"), extensions: ["khatabak"] }],
        });
      } catch (e) {
        return shell.error(e);
      }
      if (!target) return;
    }
    shell.info(t("Backing up and checking the backup…"));
    try {
      const d = await shell.transition(backupApi.now(target, password));
      const where = d.folder ? t("{file} ({size}) in {folder}", { file: ltr(d.file), size: fileSize(d.size), folder: ltr(d.folder) }) : t("{file} ({size})", { file: ltr(d.file), size: fileSize(d.size) });
      shell.info(t("Backup saved and checked: {where}. It has all {n} entries of the audit trail.", { where, n: d.summary.entries }), 12000);
      shell.refresh();
    } catch (e) {
      shell.error(e);
    }
    return;
  }
}

/** Data → Restore: pick a backup, check it, show what it holds, then restore. */
export async function restoreBackup(shell: Shell): Promise<void> {
  let picked: string | string[] | null;
  try {
    picked = await open({ multiple: false, title: t("Choose a backup to restore"), filters: [{ name: t("KhataOS backup"), extensions: ["khatabak"] }] });
  } catch (e) {
    return shell.error(e);
  }
  const path = Array.isArray(picked) ? picked[0] : picked;
  if (!path) return;
  let password: string | null = null;
  try {
    if (await shell.transition(backupApi.encrypted(path))) {
      const p = await form(shell, "This backup is encrypted", [{ label: "Password", secret: true }], ([x]) => (x ? null : t("Enter the backup's password.")));
      if (!p) return;
      password = p[0];
    }
    shell.info(t("Checking the backup…"));
    const preview = await shell.transition(backupApi.inspect(path, password));
    await shell.push(new RestoreScreen(shell, path, password, preview));
  } catch (e) {
    shell.error(e);
  }
}

// ------------------------------------------------------------- screens

interface Row {
  label: string;
  value: () => string;
  cls?: () => string;
  /** Rows that do something on Enter; the others are facts the cursor skips. */
  run?: () => Promise<void>;
  section?: string;
}

/** A card of rows like the configuration screens (sharing.css `.cfg`): ↑ ↓ and Enter. */
abstract class RowsScreen implements Screen {
  abstract title: string;
  el = h("section.cfg");
  protected cursor = 0;
  protected hint = h("div.cfg-hint");
  private card = h("div.cfg-card");
  private rows: Row[] = [];
  private rowEls: HTMLElement[] = [];

  constructor(protected shell: Shell) {
    this.el.append(this.card, this.hint);
  }

  protected abstract heading(): string;
  protected abstract buildRows(): Row[];

  async init(): Promise<void> {
    this.render();
  }

  protected render(): void {
    this.rows = this.buildRows();
    this.rowEls = [];
    const items: HTMLElement[] = [h("div.cfg-title", this.heading())];
    this.rows.forEach((r, i) => {
      if (r.section) items.push(h("div.cfg-section", r.section));
      const el = h("div.cfg-row", { class: r.run ? "" : "info", onclick: () => void this.pick(i) }, h("span", r.label), h("span.cfg-value", { class: r.cls?.() ?? "" }, r.value()));
      this.rowEls.push(el);
      items.push(el);
    });
    this.card.replaceChildren(...items);
    if (!this.rows[this.cursor]?.run) this.cursor = Math.max(0, this.rows.findIndex((r) => r.run));
    this.paint();
  }

  private paint(): void {
    this.rowEls.forEach((el, i) => el.classList.toggle("cur", i === this.cursor && Boolean(this.rows[i].run)));
  }

  private move(delta: number): void {
    const n = this.rows.length;
    let i = this.cursor;
    for (let k = 0; k < n; k++) {
      i = (i + delta + n) % n;
      if (this.rows[i].run) break;
    }
    this.cursor = i;
    this.paint();
  }

  protected async pick(i: number): Promise<void> {
    const r = this.rows[i];
    if (!r?.run) return;
    this.cursor = i;
    this.paint();
    await r.run();
    if (this.shell.top() === this) {
      this.render();
      this.shell.refresh();
    }
  }

  contexts(): Context[] {
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Back";
  }

  bottom() {
    return [this.shell.btn("Back", this.contexts())].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    (document.activeElement as HTMLElement | null)?.blur?.();
    this.paint();
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Up") this.move(-1);
    else if (e.chord === "Down") this.move(1);
    else if (e.chord === "Enter" || e.chord === "Space") void this.pick(this.cursor);
    else return false;
    return true;
  }

  action(a: string): boolean {
    if (a === "Back") {
      this.shell.pop();
      return true;
    }
    return false;
  }
}

const onOff = (on: boolean) => (on ? t("On") : t("Off"));

/** Data → Automatic backups: on close and daily, how many to keep, the folder. Saved as soon as changed. */
export class AutoBackupScreen extends RowsScreen {
  title = "Automatic Backups";
  private st!: BackupStatus;

  protected heading(): string {
    return t("Automatic backups of {company}", { company: this.shell.company?.name ?? "" });
  }

  override async init(): Promise<void> {
    this.st = await backupApi.status();
    this.render();
  }

  private async change(prefs: Partial<BackupStatus["prefs"]>): Promise<void> {
    try {
      this.st = await backupApi.setPrefs({ ...this.st.prefs, ...prefs });
    } catch (e) {
      this.shell.error(e);
    }
  }

  protected buildRows(): Row[] {
    const p = this.st.prefs;
    const can = this.st.can_change;
    const edit = (f: () => Promise<void>) => (can ? f : undefined);
    const phone = isMobile();
    const last = this.st.last;
    this.hint.replaceChildren(
      h("p", t("Automatic backups are checked like the ones you make. To keep the newest {n}, older automatic backups are deleted; your own backups and other files in the folder are never touched.", { n: p.keep })),
      h("p", t("Before a Tally import into a company with books, a restore that replaces, a sync rebase or a move to another relay, KhataOS always backs up first, even with automatic backups off.")),
      ...(phone ? [h("p.warn", t("On a phone these backups stay inside the app, and uninstalling KhataOS deletes them. Use Data → Backup to save a copy somewhere else."))] : []),
      ...(can ? [] : [h("p.warn", t("Only an Accountant or the Owner can change these settings."))]),
    );
    return [
      { label: t("Automatic backups"), value: () => onOff(p.automatic), cls: () => (p.automatic ? "ok" : "warn"), run: edit(() => this.change({ automatic: !p.automatic })) },
      { label: t("When the company closes"), value: () => onOff(p.on_close), run: edit(() => this.change({ on_close: !p.on_close })) },
      { label: t("Once a day while it is open"), value: () => onOff(p.daily), run: edit(() => this.change({ daily: !p.daily })) },
      {
        label: t("Automatic backups to keep"),
        value: () => String(p.keep),
        run: edit(async () => {
          const v = await this.shell.dialog("Automatic backups to keep", [{ label: "How many (1 to 100)", value: String(p.keep) }], ([x]) => {
            const n = Number(x.trim());
            return Number.isInteger(n) && n >= 1 && n <= 100 ? null : t("Enter a number from 1 to 100.");
          });
          if (v) await this.change({ keep: Number(v[0].trim()) });
        }),
      },
      {
        label: t("Folder"),
        value: () => ltr(this.st.folder),
        run:
          phone || !can
            ? undefined
            : async () => {
                const dir = await open({ directory: true, title: t("Choose the backup folder"), defaultPath: this.st.folder }).catch((e) => {
                  this.shell.error(e);
                  return null;
                });
                if (typeof dir === "string" && dir) await this.change({ folder: dir });
              },
      },
      ...(p.folder && can
        ? [{ label: t("Use the default folder"), value: () => ltr(this.st.default_folder), run: () => this.change({ folder: null }) }]
        : []),
      {
        label: t("Last backup"),
        value: () => (last ? `${backupTime(last.at)} · ${fileSize(last.size)}${last.automatic ? ` · ${t("automatic")}` : ""}` : t("None yet")),
        cls: () => (this.st.remind ? "warn" : ""),
        section: t("Status"),
      },
      { label: t("Where"), value: () => (last ? ltr(last.path) : "—") },
    ];
  }
}

/** What a backup holds, checked, and the two ways to restore it. */
class RestoreScreen extends RowsScreen {
  title = "Restore Backup";

  constructor(
    shell: Shell,
    private path: string,
    private password: string | null,
    private p: RestorePreview,
  ) {
    super(shell);
  }

  protected heading(): string {
    return t("Restore {company}", { company: this.p.summary.company });
  }

  protected buildRows(): Row[] {
    const s = this.p.summary;
    const range = s.first_voucher && s.last_voucher ? t("{from} to {to}", { from: D.show(s.first_voucher), to: D.show(s.last_voucher) }) : t("no vouchers");
    const warn = this.warning();
    this.hint.replaceChildren(
      h("p", t("Checked: the backup opened, its audit trail replayed and every entry's hash verified.")),
      ...(warn ? [h("p.warn", warn)] : []),
      h("p", t("A new company file is the safest: nothing is overwritten, and you can compare the two. Replacing keeps the current file as a copy beside it, and backs it up first.")),
    );
    return [
      { label: t("Company"), value: () => s.company },
      { label: t("Backed up"), value: () => `${backupTime(s.made_at)}${s.automatic ? ` · ${t("automatic")}` : ""}${s.encrypted ? ` · ${t("encrypted")}` : ""}` },
      { label: t("Vouchers"), value: () => `${s.vouchers} · ${range}` },
      { label: t("Last voucher date"), value: () => (s.last_voucher ? D.show(s.last_voucher) : "—") },
      { label: t("Audit trail entries"), value: () => String(s.entries) },
      { label: t("Notes and settings"), value: () => String(s.records) },
      { label: t("File"), value: () => `${ltr(this.p.file)} · ${fileSize(this.p.size)}` },
      { label: t("Restore as a new company file"), value: () => t("safest"), cls: () => "ok", section: t("Restore"), run: () => this.restoreNew() },
      ...(this.p.can_replace
        ? [{ label: t("Replace the open company"), value: () => this.p.open_company ?? "", cls: () => "warn", run: () => this.replace() }]
        : []),
    ];
  }

  private warning(): string | null {
    const p = this.p;
    switch (p.relation) {
      case "older":
        return t("This backup is older than the open books: {n} later entries are not in it.", { n: p.later_entries });
      case "newer":
        return t("This backup has entries the open books don't have.");
      case "diverged":
        return t("This backup and the open books have both changed since they were last the same.");
      case "other_company":
        return t("This is a backup of {company}, not of the open company {open}. It can only be restored as a new company file.", { company: p.summary.company, open: p.open_company ?? "" });
      default:
        return null;
    }
  }

  private async restoreNew(): Promise<void> {
    const shell = this.shell;
    try {
      shell.info(t("Restoring…"));
      const r = await shell.transition(backupApi.restore(this.path, this.password, "new", null));
      await shell.closeCompany();
      await shell.openCompany(await api.openCompany(r.path));
      shell.info(t("Restored as a new company file: {path}", { path: ltr(r.path) }), 12000);
    } catch (e) {
      shell.error(e);
    }
  }

  private async replace(): Promise<void> {
    const shell = this.shell;
    const name = this.p.open_company ?? "";
    const later = this.p.relation === "older" ? ` ${t("{n} later entries will not be in the restored books.", { n: this.p.later_entries })}` : "";
    const typed = await form(
      shell,
      t("Replace {company} with this backup?", { company: name }),
      [{ label: "Type the company name", placeholder: name }],
      ([x]) => (x.trim().toLowerCase() === name.trim().toLowerCase() ? null : t("Type the company name exactly: {company}", { company: name })),
      t("The current file is kept beside it as a copy, and backed up first.") + later,
    );
    if (!typed) return;
    try {
      shell.info(t("Backing up, then restoring…"));
      const r = await shell.transition(backupApi.restore(this.path, this.password, "replace", typed[0]));
      if (r.info) await shell.openCompany(r.info);
      shell.info(t("Restored. The previous company file is kept as {file}.", { file: ltr(r.kept ?? "") }), 15000);
    } catch (e) {
      shell.error(e);
    }
  }
}

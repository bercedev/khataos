// Sharing, the TallyPrime way (see docs/sharing.md):
// - Exchange (Alt+Z) → Configure: "KhataOS acts as None / Client / Server / Both" + port;
// - Company (Alt+K) → Online Access: Connect, Disconnect, Connectivity Status,
//   Remote Access (invite people, revoke), Browser Access, Configure;
// - "Connect to company…": paste an invite code, get the books;
// - Share (Alt+M) → Send to accountant (file) and Import (Alt+O) → Open accountant file;
// - Exchange → Configure → Relay address, and "Move to another relay…" (docs/deploy-relay.md).
// Commands live in src-tauri/src/lan.rs and package.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import * as D from "../dates";
import { h } from "../dom";
import { language, ltr, t } from "../i18n";
import { letterOf } from "../keys";
import type { KeyEv, Screen, Shell } from "../shell";
import type { CompanyInfo, Context, SyncResult } from "../types";
import { MenuScreen } from "./launcher";
import type { PickItem } from "../picker";
import { HomeScreen } from "./home";
import "../sharing.css";

// ------------------------------------------------------------------ API

type ActsAs = "none" | "client" | "server" | "both";

interface SharingConfig {
  acts_as: ActsAs;
  port: number;
  server_running: boolean;
  server_port: number | null;
  server_error: string | null;
  addresses: string[];
  /** The relay offered for internet sharing; "" when none is set. */
  relay_url: string;
  relay_builtin: boolean;
}

interface OnlineAccess {
  connect_on_load: boolean;
  interval_minutes: number;
  role: string;
  hosted: boolean;
  linked: boolean;
  url: string;
  via: "lan" | "relay";
  relay_url: string;
  encrypted: boolean;
}

interface HostReport {
  company_id: string;
  created: boolean;
  sync: SyncResult;
}

interface CompanyLink {
  name: string;
  access: OnlineAccess;
  paused: boolean;
  last_status: string | null;
  local_len: number;
  synced_len: number;
  pending: number;
  server_len: number | null;
  reach_error: string | null;
}

interface Connectivity extends SharingConfig {
  company: CompanyLink | null;
}

interface AccessRow {
  id: string;
  role: string;
  label: string;
  expires_at: number | null;
  revoked: boolean;
  expired: boolean;
}

interface AccessList {
  company: string;
  hosted: boolean;
  default_url: string;
  rows: AccessRow[];
}

interface Invite {
  code: string;
  role: string;
  label: string;
  url: string;
  company_id: string;
  portal_url: string;
  token: string;
}

interface MoveReport {
  from: string;
  to: string;
  entries: number;
  revoked: number | null;
}

interface InvitePreview {
  company_name: string;
  url: string;
  role: string;
  encrypted: boolean;
}

interface Sent {
  path: string;
  from_seq: number;
  to_seq: number;
  entries: number;
  /** Notes and settings (shared records) in the file. */
  records: number;
}

interface PackageInfo {
  company: string;
  from_seq: number;
  to_seq: number;
  entries: number;
  for_open_company: boolean;
}

interface Received {
  info: CompanyInfo;
  added: number;
  already_had: number;
  /** Notes and settings (shared records) that changed. */
  records: number;
  created: boolean;
  switched: boolean;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
  }
}

export const sharingApi = {
  config: () => call<SharingConfig>("sharing_config"),
  setConfig: (acts_as: ActsAs, port: number) => call<SharingConfig>("set_sharing_config", { acts_as, port }),
  setRelayAddress: (relay_url: string) => call<SharingConfig>("set_relay_address", { relay_url }),
  moveRelay: (relay_url: string, signup_key: string | null) => call<MoveReport>("move_relay", { relay_url, signup_key }),
  hostCompany: () => call<HostReport>("host_company"),
  connectRelay: (relay_url: string, signup_key: string | null = null) =>
    call<HostReport>("connect_relay", { relay_url, signup_key }),
  onlineAccess: () => call<OnlineAccess>("online_access"),
  setOnlineAccess: (connect_on_load: boolean, interval_minutes: number, via: string, relay_url: string) =>
    call<OnlineAccess>("set_online_access", { connect_on_load, interval_minutes, via, relay_url }),
  setConnected: (connected: boolean) => call<void>("set_connected", { connected }),
  connectivity: () => call<Connectivity>("connectivity_status"),
  listAccess: () => call<AccessList>("list_access"),
  createInvite: (role: string, label: string, expires_in_days: number | null, url: string) =>
    call<Invite>("create_invite", { role, label, expires_in_days, url }),
  revokeAccess: (id: string) => call<void>("revoke_access", { id }),
  inspectInvite: (code: string) => call<InvitePreview>("inspect_invite", { code }),
  joinCompany: (code: string) => call<CompanyInfo>("join_company", { code }),
  packageSend: (path: string, passphrase: string, full: boolean) => call<Sent>("package_send", { path, passphrase, full }),
  packageNextFrom: () => call<number>("package_next_from"),
  packageInspect: (path: string) => call<PackageInfo>("package_inspect", { path }),
  packageReceive: (path: string, passphrase: string) => call<Received>("package_receive", { path, passphrase }),
};

// ------------------------------------------------------------- wording

const ACTS_AS: { key: ActsAs; label: string; hint: string }[] = [
  { key: "none", label: "None", hint: "this computer only" },
  { key: "client", label: "Client", hint: "connects to the office server" },
  { key: "server", label: "Server", hint: "hosts books for the office" },
  { key: "both", label: "Both", hint: "hosts, and connects to others" },
];

const actsAsLabel = (a: ActsAs) => t(ACTS_AS.find((x) => x.key === a)?.label ?? a);
/** A role code ("viewer") as a word; the code itself is never user data. */
const roleLabel = (r: string) => t(r ? r[0].toUpperCase() + r.slice(1) : "Not shared");
const yesNo = (b: boolean) => (b ? t("Yes") : t("No"));
const bold = (s: string) => h("b", s);

/**
 * A translated sentence with elements in it: "{name}" placeholders become
 * the given nodes, so word order stays the translation's own.
 */
function rich(text: string, parts: Record<string, Node | string>): (Node | string)[] {
  return text
    .split(/(\{\w+\})/)
    .map((s) => {
      const k = /^\{(\w+)\}$/.exec(s)?.[1];
      return k && k in parts ? parts[k] : s;
    })
    .filter((s) => s !== "");
}

function showDate(unix: number): string {
  const d = new Date(unix * 1000);
  if (language() === "ar") return D.show(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`);
  return d.toLocaleDateString("en-IN", { day: "2-digit", month: "short", year: "numeric" });
}

function serverLine(c: SharingConfig): { text: string; cls: string } {
  if (c.server_error) return { text: t("Not running: {error}", { error: ltr(c.server_error) }), cls: "warn" };
  if (c.server_running) return { text: t("Running on port {port}", { port: ltr(String(c.server_port)) }), cls: "ok" };
  return { text: c.acts_as === "server" || c.acts_as === "both" ? t("Starting…") : t("Off"), cls: "" };
}

function relayLine(c: SharingConfig): { text: string; cls: string } {
  if (!c.relay_url) return { text: t("Not set"), cls: "warn" };
  return { text: c.relay_builtin ? t("{url} (built in)", { url: ltr(c.relay_url) }) : ltr(c.relay_url), cls: "" };
}

/** Where self-hosting is explained (docs/deploy-relay.md in the repository). */
const DEPLOY_DOC = "https://github.com/khataos/khataos/blob/main/docs/deploy-relay.md";

function noRelayHint(): HTMLElement {
  return h(
    "p",
    ...rich(
      t(
        "{lead} To share over the internet, either enter the address of a relay someone runs for you (your CA, IT person or KhataOS), or host your own on any always-on PC (no Docker needed; see {doc}). Or send accountant files instead (Alt+M).",
      ),
      { lead: bold(t("No relay address is set.")), doc: bold(ltr("docs/deploy-relay.md")) },
    ),
  );
}

/** Blank is allowed where blank means "none"; the app checks the rest (https). */
function relayProblem(u: string, blankOk: boolean): string | null {
  return u.trim() || blankOk ? null : t("Enter the relay address, e.g. {example}", { example: ltr("https://relay.your-firm.in") });
}

/**
 * No relay configured: say so and offer the ways forward. Returns the
 * relay address entered (now this computer's default), or null.
 */
async function noRelay(shell: Shell): Promise<string | null> {
  const pick = await choose(shell, "No relay address is set", [
    { key: "enter", label: "Enter a relay address", hint: "someone runs a relay for you" },
    { key: "host", label: "Host your own relay", hint: "any always-on PC; how-to guide" },
    { key: "files", label: "Send accountant files instead", hint: "no relay needed" },
  ]);
  if (pick === "enter") {
    const v = await form(shell, "Relay address", [{ label: "Address", placeholder: "https://relay.your-firm.in" }], ([u]) => relayProblem(u, false));
    if (!v) return null;
    const c = await sharingApi.setRelayAddress(v[0]);
    shell.info(t("Relay address saved: {url}. Change it in Exchange (Alt+Z) → Configure.", { url: ltr(c.relay_url) }));
    return c.relay_url;
  }
  if (pick === "host") {
    const copied = await copyText(DEPLOY_DOC);
    const params = { doc: ltr("docs/deploy-relay.md"), url: ltr(DEPLOY_DOC) };
    shell.info(
      copied
        ? t("Hosting your own relay is explained in {doc} ({url}), link copied. When it runs, enter its address in Exchange (Alt+Z) → Configure → Relay address.", params)
        : t("Hosting your own relay is explained in {doc} ({url}). When it runs, enter its address in Exchange (Alt+Z) → Configure → Relay address.", params),
    );
  } else if (pick === "files") await sendToAccountant(shell);
  return null;
}

function addressLine(c: SharingConfig): string {
  const port = c.server_port ?? c.port;
  return c.addresses.length ? ltr(c.addresses.map((ip) => `${ip}:${port}`).join(", ")) : t("No office network found");
}

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const ta = h("textarea", { style: { position: "fixed", opacity: "0" } });
    ta.value = text;
    document.body.append(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  }
}

// ---------------------------------------------------------- small modals

/**
 * A list of choices answered by its first letter or arrows + Enter (rule P5).
 * Title, labels and hints are English and shown through t(). The answer
 * letter is always the English label's first letter (letterOf follows the
 * physical key); a translated label shows that letter as a key cap.
 */
export function choose<T extends string>(
  shell: Shell,
  title: string,
  options: { key: T; label: string; hint?: string }[],
  current?: T,
): Promise<T | null> {
  return new Promise((resolve) => {
    let cursor = Math.max(0, options.findIndex((o) => o.key === current));
    title = t(title);
    const rows = options.map((o) => {
      const shown = t(o.label);
      const label =
        shown === o.label
          ? h("span", h("span.hk", o.label[0]), o.label.slice(1))
          : h("span", h("kbd.hk-key", o.label[0].toUpperCase()), shown);
      return h("div.choice-row", label, o.hint ? h("span.muted", t(o.hint)) : null);
    });
    const paint = () => rows.forEach((r, i) => r.classList.toggle("cur", i === cursor));
    const done = (v: T | null) => {
      shell.closeModal(m);
      resolve(v);
    };
    rows.forEach((r, i) => r.addEventListener("click", () => done(options[i].key)));
    const m: Screen = {
      title,
      el: h("div.choice", { role: "dialog", "aria-label": title }, h("div.prompt-q", title), ...rows),
      contexts: () => ["Dialog"],
      key: (e) => {
        if (e.chord === "Up") cursor = (cursor + options.length - 1) % options.length;
        else if (e.chord === "Down") cursor = (cursor + 1) % options.length;
        else if (e.chord === "Enter") done(options[cursor].key);
        else if (e.chord === "Esc") done(null);
        else if (!e.ctrlKey && !e.altKey && e.key.length === 1) {
          const i = options.findIndex((o) => o.label[0].toLowerCase() === letterOf(e));
          if (i >= 0) done(options[i].key);
        }
        paint();
        return true;
      },
    };
    paint();
    shell.openModal(m);
  });
}

/**
 * Like `shell.dialog`, with hidden fields for passphrases. Title, labels,
 * placeholders and the note are shown through t(); pass a sentence with
 * names in it already translated.
 */
export function form(
  shell: Shell,
  title: string,
  fields: { label: string; value?: string; secret?: boolean; placeholder?: string }[],
  validate: (values: string[]) => string | null,
  note?: string,
): Promise<string[] | null> {
  title = t(title);
  fields = fields.map((f) => ({ ...f, label: t(f.label), placeholder: f.placeholder && t(f.placeholder) }));
  note = note && t(note);
  return new Promise((resolve) => {
    const inputs = fields.map((f) =>
      h("input.field", {
        value: f.value ?? "",
        type: f.secret ? "password" : "text",
        placeholder: f.placeholder ?? "",
        spellcheck: false,
        autocomplete: "off",
      }),
    );
    let at = 0;
    const go = (i: number) => {
      at = i;
      inputs[i].focus();
      inputs[i].select();
    };
    const finish = (v: string[] | null) => {
      shell.closeModal(m);
      resolve(v);
    };
    const accept = () => {
      const values = inputs.map((i) => i.value);
      const problem = validate(values);
      if (problem) shell.warn(problem);
      else finish(values);
    };
    inputs.forEach((inp, i) => inp.addEventListener("focus", () => (at = i)));
    const m: Screen = {
      title,
      el: h(
        "div.dialog",
        { role: "dialog", "aria-label": title, style: { minWidth: "420px" } },
        h("div.dialog-title", title),
        ...fields.map((f, i) => h("label.dialog-row", h("span", f.label), inputs[i])),
        note ? h("p.note", note) : null,
      ),
      contexts: () => ["Dialog"],
      focus: () => go(at),
      key: (e) => {
        if (e.chord === "Esc") {
          if (inputs[at].value) inputs[at].value = "";
          else finish(null);
          return true;
        }
        if (e.chord === "Up" || e.chord === "Shift+Tab") {
          if (at > 0) go(at - 1);
          return true;
        }
        return false;
      },
      action: (a) => {
        if (a === "Accept") accept();
        else if (a === "NextField") {
          if (at < inputs.length - 1) go(at + 1);
          else accept();
        } else if (a === "PreviousField") {
          if (at > 0) go(at - 1);
        } else return false;
        return true;
      },
    };
    shell.openModal(m);
  });
}

// ------------------------------------------------- configuration lists

interface Row {
  label: string;
  value: () => string;
  cls?: () => string;
  /** Editable rows; info rows are skipped by the cursor. */
  edit?: () => Promise<void>;
  section?: string;
}

/** TallyPrime's "List of Configurations": setting | current value (italic). */
abstract class ConfigList implements Screen {
  abstract title: string;
  el = h("section.cfg");
  protected rows: Row[] = [];
  protected cursor = 0;
  protected dirty = false;
  private rowEls: HTMLElement[] = [];
  private card = h("div.cfg-card");
  protected hint = h("div.cfg-hint");

  constructor(protected shell: Shell, private heading: string) {
    this.el.append(this.card, this.hint);
  }

  protected abstract load(): Promise<void>;
  protected abstract buildRows(): Row[];
  /** Ctrl+A. */
  protected abstract save(): Promise<void>;

  async init(): Promise<void> {
    await this.load();
    this.render();
  }

  protected render(): void {
    this.rows = this.buildRows();
    this.rowEls = [];
    const items: HTMLElement[] = [h("div.cfg-title", this.heading)];
    this.rows.forEach((r, i) => {
      if (r.section) items.push(h("div.cfg-section", r.section));
      const el = h(
        "div.cfg-row",
        { class: r.edit ? "" : "info", onclick: () => this.pick(i) },
        h("span", r.label),
        h("span.cfg-value", { class: r.cls?.() ?? "" }, r.value()),
      );
      this.rowEls.push(el);
      items.push(el);
    });
    this.card.replaceChildren(...items);
    if (!this.rows[this.cursor]?.edit) this.cursor = Math.max(0, this.rows.findIndex((r) => r.edit));
    this.paint();
  }

  private paint(): void {
    this.rowEls.forEach((el, i) => el.classList.toggle("cur", i === this.cursor && Boolean(this.rows[i].edit)));
  }

  private move(delta: number): void {
    const n = this.rows.length;
    let i = this.cursor;
    for (let k = 0; k < n; k++) {
      i = (i + delta + n) % n;
      if (this.rows[i].edit) break;
    }
    this.cursor = i;
    this.paint();
  }

  private async pick(i: number): Promise<void> {
    const r = this.rows[i];
    if (!r?.edit) return;
    this.cursor = i;
    this.paint();
    await r.edit();
    this.render();
    this.shell.refresh();
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
    (document.activeElement as HTMLElement | null)?.blur?.();
    this.paint();
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Up") this.move(-1);
    else if (e.chord === "Down") this.move(1);
    else if (e.chord === "Enter") void this.pick(this.cursor);
    else return false;
    return true;
  }

  action(a: string): boolean {
    if (a === "Accept") {
      void this.save();
      return true;
    }
    if (a === "Back") {
      void this.leave();
      return true;
    }
    return false;
  }

  private async leave(): Promise<void> {
    if (!this.dirty || (await this.shell.ask("Quit without saving?", { yesFirst: false }))) this.shell.pop();
  }
}

/** A read-only screen of facts with a refresh key. */
abstract class InfoList extends ConfigList {
  protected async save(): Promise<void> {
    await this.init();
    this.shell.info(t("Refreshed."));
  }

  override bottom() {
    return [this.shell.btn("Accept", this.contexts(), "Refresh"), this.shell.btn("Back", this.contexts())].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }
}

// --------------------------------------- Exchange → Configure (Client/Server)

/** Alt+Z → Configure → Data Synchronisation: Client/Server configuration. */
export class ClientServerConfigScreen extends ConfigList {
  title = "Client/Server Configuration";
  private cfg!: SharingConfig;
  private actsAs: ActsAs = "none";
  private port = 9090;
  /** Edited relay address; null = unchanged. */
  private relay: string | null = null;

  constructor(shell: Shell) {
    super(shell, t("Data Synchronisation · Client/Server configuration"));
  }

  protected async load(): Promise<void> {
    this.cfg = await sharingApi.config();
    this.actsAs = this.cfg.acts_as;
    this.port = this.cfg.port;
    this.relay = null;
    this.hint.replaceChildren(
      h(
        "p",
        ...rich(t("{server}: this computer keeps the books for the office. Keep KhataOS open on it; colleagues connect with an invite code (Company → Remote Access)."), {
          server: bold(t("Server")),
        }),
      ),
      h(
        "p",
        ...rich(t("{client}: this computer connects to a server on the office network. {both} does both."), {
          client: bold(t("Client")),
          both: bold(t("Both")),
        }),
      ),
      h("p", t("Port 9090 is the default (TallyPrime uses 9000, so both can run). If Windows asks, allow KhataOS on private networks.")),
      this.cfg.relay_url
        ? h(
            "p",
            ...rich(
              t("{relay}: used to share over the internet (Company → Configure Online Access → Sync via relay). The relay can't read the books; they are encrypted on this computer."),
              { relay: bold(t("Relay address")) },
            ),
          )
        : noRelayHint(),
    );
  }

  protected buildRows(): Row[] {
    const s = serverLine(this.cfg);
    return [
      {
        label: t("KhataOS acts as"),
        value: () => actsAsLabel(this.actsAs),
        edit: async () => {
          const v = await choose(this.shell, "KhataOS acts as", ACTS_AS, this.actsAs);
          if (v && v !== this.actsAs) {
            this.actsAs = v;
            this.dirty = true;
          }
        },
      },
      {
        label: t("Port"),
        value: () => String(this.port),
        edit: async () => {
          const v = await form(this.shell, "Port", [{ label: "Port", value: String(this.port) }], ([p]) =>
            /^\d+$/.test(p.trim()) && +p >= 1024 && +p <= 65535 ? null : t("Enter a port from 1024 to 65535 (9090 is the default)."),
          );
          if (v && +v[0] !== this.port) {
            this.port = +v[0];
            this.dirty = true;
          }
        },
      },
      {
        label: t("Relay address"),
        section: t("Internet sharing"),
        value: () => (this.relay === null ? relayLine(this.cfg).text : this.relay ? ltr(this.relay) : t("Not set")),
        cls: () => (this.relay === null ? relayLine(this.cfg).cls : this.relay ? "" : "warn"),
        edit: async () => {
          const current = this.relay ?? (this.cfg.relay_builtin ? "" : this.cfg.relay_url);
          const v = await form(
            this.shell,
            "Relay address",
            [{ label: "Address", value: current, placeholder: this.cfg.relay_builtin ? this.cfg.relay_url : "https://relay.your-firm.in" }],
            ([u]) => relayProblem(u, true),
            this.cfg.relay_builtin ? "Leave blank to use the relay built into this copy of KhataOS." : "Leave blank for none.",
          );
          if (v && v[0].trim() !== current) {
            this.relay = v[0].trim();
            this.dirty = true;
          }
        },
      },
      { label: t("Server"), value: () => s.text, cls: () => s.cls, section: t("This computer") },
      { label: t("Office network address"), value: () => addressLine(this.cfg) },
    ];
  }

  protected async save(): Promise<void> {
    try {
      if (this.relay !== null) {
        this.cfg = await sharingApi.setRelayAddress(this.relay);
        this.relay = null;
      }
      this.cfg = await this.shell.transition(sharingApi.setConfig(this.actsAs, this.port));
      this.dirty = false;
      this.render();
      if (this.cfg.server_error) return this.shell.error(this.cfg.server_error);
      this.shell.info(t("KhataOS acts as {mode}.", { mode: actsAsLabel(this.cfg.acts_as) }));
      if (this.cfg.server_running && this.shell.company) await offerToHost(this.shell);
    } catch (e) {
      this.shell.error(e);
    }
  }
}

/** After Server mode is turned on: host the open company if it isn't shared yet. */
async function offerToHost(shell: Shell): Promise<void> {
  const access = await sharingApi.onlineAccess();
  if (access.linked || access.role === "viewer") return;
  if (!(await shell.ask(t("Share {company} on the office network now?", { company: String(shell.company?.name) })))) return;
  await hostHere(shell);
}

async function hostHere(shell: Shell): Promise<void> {
  shell.info(t("Sharing on the office network…"));
  try {
    const r = await shell.transition(sharingApi.hostCompany());
    await shell.refreshCompany();
    shell.refresh();
    shell.info(
      t("{company} is shared on the office network. {result}. Invite colleagues from Company → Remote Access.", {
        company: String(shell.company?.name),
        result: ltr(r.sync.message),
      }),
    );
  } catch (e) {
    shell.error(e);
  }
}

// ------------------------------------------ Company → Configure Online Access

/** Alt+K → Configure → Online Access (per company, on this computer). */
export class OnlineAccessConfigScreen extends ConfigList {
  title = "Online Access";
  private a!: OnlineAccess;

  constructor(shell: Shell) {
    super(shell, t("Online Access · this company on this computer"));
  }

  protected async load(): Promise<void> {
    this.a = await sharingApi.onlineAccess();
    this.hint.replaceChildren(
      h("p", ...rich(t("{server}: one PC in the office runs KhataOS in Server mode (Exchange → Configure)."), { server: bold(t("Office server")) })),
      h(
        "p",
        ...rich(t("{relay}: share over the internet. The books are encrypted on this computer before they leave it; the relay can't read them."), {
          relay: bold(t("Relay")),
        }),
      ),
    );
    if (this.a.via === "relay" && !this.a.relay_url) this.hint.append(noRelayHint());
  }

  protected buildRows(): Row[] {
    const a = this.a;
    const rows: Row[] = [
      {
        label: t("Connect Company on loading"),
        value: () => yesNo(a.connect_on_load),
        edit: async () => {
          const v = await choose(this.shell, "Connect Company on loading", [{ key: "yes", label: "Yes" }, { key: "no", label: "No" }], a.connect_on_load ? "yes" : "no");
          if (v) {
            this.dirty ||= (v === "yes") !== a.connect_on_load;
            a.connect_on_load = v === "yes";
          }
        },
      },
      {
        label: t("Sync every (minutes)"),
        value: () => String(a.interval_minutes),
        edit: async () => {
          const v = await form(this.shell, "Sync every", [{ label: "Minutes", value: String(a.interval_minutes) }], ([m]) =>
            /^\d+$/.test(m.trim()) && +m >= 1 && +m <= 1440 ? null : t("Enter 1 to 1440 minutes."),
          );
          if (v) {
            this.dirty ||= +v[0] !== a.interval_minutes;
            a.interval_minutes = +v[0];
          }
        },
      },
    ];
    const locked = a.linked;
    rows.push({
      label: t("Sync via"),
      value: () => (a.via === "relay" ? t("Relay (internet)") : t("Office server (LAN)")),
      edit: locked
        ? undefined
        : async () => {
            const v = await choose(
              this.shell,
              "Sync via",
              [
                { key: "lan", label: "Office server (LAN)", hint: "one PC in the office" },
                { key: "relay", label: "Relay (internet)", hint: "end-to-end encrypted" },
              ],
              a.via,
            );
            if (v) {
              this.dirty ||= v !== a.via;
              a.via = v;
            }
          },
    });
    if (a.via === "relay")
      rows.push({
        label: t("Relay address"),
        value: () => (a.relay_url ? ltr(a.relay_url) : t("Not set")),
        cls: () => (a.relay_url ? "" : "warn"),
        edit: locked
          ? undefined
          : async () => {
              const v = await form(
                this.shell,
                "Relay address",
                [{ label: "Address", value: a.relay_url, placeholder: "https://relay.your-firm.in" }],
                ([u]) => relayProblem(u, true),
                "Leave blank to use this computer's relay address (Exchange → Configure).",
              );
              if (v) {
                this.dirty ||= v[0].trim() !== a.relay_url;
                a.relay_url = v[0].trim();
              }
            },
      });
    rows.push(
      { label: t("Shared as"), value: () => (a.hosted ? t("Owner (hosted on this computer)") : roleLabel(a.role)), section: t("Status") },
      { label: t("Server"), value: () => (a.url ? ltr(a.url) : t("Not connected yet")) },
      { label: t("Encryption"), value: () => (a.encrypted ? t("End-to-end (relay)") : a.linked ? t("Office network") : "—") },
    );
    return rows;
  }

  protected async save(): Promise<void> {
    try {
      this.a = await sharingApi.setOnlineAccess(this.a.connect_on_load, this.a.interval_minutes, this.a.via, this.a.relay_url);
      this.dirty = false;
      this.render();
      this.shell.info(this.a.linked ? t("Online Access saved.") : t("Online Access saved. Use Company → Connect to share this company."));
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ----------------------------------------------- Connect / Disconnect

/** Alt+K → Connect: share the company the way Online Access says, or sync it. */
export async function connectCompany(shell: Shell): Promise<void> {
  if (!shell.company) return;
  try {
    const a = await sharingApi.onlineAccess();
    await sharingApi.setConnected(true);
    if (a.linked) {
      shell.runShellAction("SyncNow");
      return;
    }
    if (a.via === "relay") {
      const relay = a.relay_url || (await noRelay(shell));
      if (!relay) return;
      const yes = await shell.ask(t("Share {company} through {relay}? It is encrypted end to end.", { company: shell.company.name, relay: ltr(relay) }));
      if (!yes) return;
      shell.info(t("Connecting to the relay…"));
      let r: HostReport;
      try {
        r = await shell.transition(sharingApi.connectRelay(relay));
      } catch (e) {
        // A private relay: its operator gives out a sign-up key.
        if (!String(e instanceof Error ? e.message : e).includes("sign-up key")) throw e;
        const k = await form(
          shell,
          "Sign-up key",
          [{ label: "Sign-up key", secret: true }],
          ([v]) => (v.trim() ? null : t("Enter the sign-up key the relay's operator gave you.")),
          t("{relay} accepts new companies only with its sign-up key. It is used once and not saved.", { relay: ltr(relay) }),
        );
        if (!k) return;
        r = await shell.transition(sharingApi.connectRelay(relay, k[0]));
      }
      await shell.refreshCompany();
      shell.refresh();
      shell.info(
        t("{company} is shared through the relay. {result}. Invite people from Company → Remote Access.", {
          company: String(shell.company?.name),
          result: ltr(r.sync.message),
        }),
      );
      return;
    }
    const cfg = await sharingApi.config();
    if (cfg.server_running) return hostHere(shell);
    shell.warn(
      t(
        "To share this company on the office network, set \"KhataOS acts as\" to Server in Exchange (Alt+Z) → Configure. To use someone else's books, choose Connect to company… and paste their invite code.",
      ),
    );
  } catch (e) {
    shell.error(e);
  }
}

// ------------------------------------------------ Move to another relay

/**
 * Company → Move to another relay…: e.g. from a relay hosted for a while by
 * someone else to the firm's own. Everything is copied, sealed with a new
 * key; colleagues get new invite codes.
 */
export async function moveToAnotherRelay(shell: Shell): Promise<void> {
  if (!shell.company) return shell.warn(t("Open a company first."));
  try {
    const a = await sharingApi.onlineAccess();
    if (!a.encrypted)
      return shell.warn(t("Only a company shared through a relay can be moved. To share this one over the internet, use Company → Configure Online Access → Sync via relay."));
    if (a.role === "viewer" || a.role === "accountant") return shell.warn(t("Only the company's owner can move it to another relay."));
    const v = await form(
      shell,
      "Move to another relay",
      [
        { label: "New relay address", placeholder: "https://relay.your-firm.in" },
        { label: "Sign-up key", secret: true, placeholder: "only if that relay needs one" },
      ],
      ([u]) => relayProblem(u, false),
      t("Now on {url}. Every entry is copied to the new relay, encrypted with a new key. Ask colleagues to sync first. Afterwards their invite codes stop working and you invite them again.", {
        url: ltr(a.url),
      }),
    );
    if (!v) return;
    const yes = await shell.ask(t("Move {company} to {relay}? Colleagues will need new invite codes.", { company: shell.company.name, relay: ltr(v[0].trim()) }), {
      yesFirst: false,
    });
    if (!yes) return;
    shell.info(t("Copying the books to the new relay…"));
    const r = await shell.transition(sharingApi.moveRelay(v[0], v[1].trim() || null));
    await shell.refreshCompany();
    shell.refresh();
    const old =
      r.revoked === null
        ? " " + t("The old relay couldn't be reached to cancel the old invite codes: tell colleagues to stop using them.")
        : r.revoked
          ? " " + t(r.revoked === 1 ? "{n} old invite code cancelled." : "{n} old invite codes cancelled.", { n: r.revoked })
          : "";
    shell.info(t("Moved to {url}: {n} entries copied, encrypted with a new key.{old} Colleagues need new invite codes.", { url: ltr(r.to), n: r.entries, old }));
    if (await shell.ask(t("Invite colleagues again now (Remote Access)?"))) await shell.push(new RemoteAccessScreen(shell));
  } catch (e) {
    shell.error(e);
  }
}

export async function disconnectCompany(shell: Shell): Promise<void> {
  await sharingApi.setConnected(false);
  shell.info(t("Disconnected: no background sync until you choose Connect. Ctrl+Alt+S still syncs once."));
}

// -------------------------------------------------- Connectivity Status

export class ConnectivityStatusScreen extends InfoList {
  title = "Connectivity Status";
  private c!: Connectivity;

  constructor(shell: Shell) {
    super(shell, t("Connectivity Status"));
  }

  protected async load(): Promise<void> {
    this.c = await this.shell.transition(sharingApi.connectivity());
    this.hint.textContent = t("Press Ctrl+A to check again. Ctrl+Alt+S syncs now.");
  }

  protected buildRows(): Row[] {
    const c = this.c;
    const s = serverLine(c);
    const rows: Row[] = [
      { label: t("KhataOS acts as"), value: () => actsAsLabel(c.acts_as), section: t("This computer") },
      { label: t("Server"), value: () => s.text, cls: () => s.cls },
      { label: t("Office network address"), value: () => addressLine(c) },
    ];
    const l = c.company;
    if (!l) return rows;
    const state = !l.access.linked
      ? { text: t("Not shared"), cls: "" }
      : l.paused
        ? { text: t("Disconnected (Company → Connect)"), cls: "warn" }
        : l.reach_error
          ? { text: t("Server not reachable"), cls: "warn" }
          : { text: t("Connected"), cls: "ok" };
    rows.push(
      { label: t("Status"), value: () => state.text, cls: () => state.cls, section: l.name },
      { label: t("Server URL"), value: () => (l.access.url ? ltr(l.access.url) : "—") },
      { label: t("Access"), value: () => (l.access.hosted ? t("Owner (hosted on this computer)") : roleLabel(l.access.role)) },
      { label: t("Encryption"), value: () => (l.access.encrypted ? t("End-to-end (relay)") : l.access.linked ? t("Office network") : "—") },
      { label: t("Connect Company on loading"), value: () => (l.access.hosted ? t("Always (hosted here)") : yesNo(l.access.connect_on_load)) },
      { label: t("Sync every"), value: () => (l.access.hosted ? t("few seconds") : t("{n} min", { n: l.access.interval_minutes })) },
      { label: t("Last sync"), value: () => (l.last_status !== null ? ltr(l.last_status) : t("Never")) },
      { label: t("Entries on this computer"), value: () => String(l.local_len) },
      { label: t("Not yet sent"), value: () => String(l.pending), cls: () => (l.pending ? "warn" : "") },
      {
        label: t("Entries on the server"),
        value: () => (l.server_len !== null ? String(l.server_len) : l.reach_error !== null ? ltr(l.reach_error) : "—"),
        cls: () => (l.reach_error ? "warn" : ""),
      },
    );
    if (l.server_len !== null && l.server_len > l.synced_len)
      rows.push({ label: t("Waiting to be received"), value: () => String((l.server_len ?? 0) - l.synced_len), cls: () => "warn" });
    return rows;
  }
}

// -------------------------------------------------------- Remote Access

/** Alt+K → Remote Access: who can open this company, with invite and revoke. */
export class RemoteAccessScreen implements Screen {
  title = "Remote Access";
  el = h("section.cfg");
  private list: AccessList | null = null;
  private problem: string | null = null;
  private cursor = 0;
  private body = h("div.cfg-card");
  private hint = h("div.cfg-hint");

  constructor(private shell: Shell) {
    this.el.append(this.body, this.hint);
  }

  async init(): Promise<void> {
    await this.load();
  }

  private async load(): Promise<void> {
    try {
      this.list = await this.shell.transition(sharingApi.listAccess());
      this.problem = null;
    } catch (e) {
      this.list = null;
      this.problem = e instanceof Error ? e.message : String(e);
    }
    this.render();
  }

  private rows(): AccessRow[] {
    return this.list?.rows ?? [];
  }

  private render(): void {
    if (!this.list) {
      this.body.replaceChildren(h("div.cfg-title", t("Remote Access")), h("div.cfg-row.info", h("span", this.problem ? ltr(this.problem) : "")));
      this.hint.replaceChildren(h("p", t("Press Enter to share this company (Company → Connect), or Esc to go back.")));
      return;
    }
    const head = h("div.cfg-row.info", h("span", h("b", t("Name"))), h("span", h("b", t("Access · until"))));
    const rows = this.rows().map((r, i) => {
      const status = r.revoked ? t("revoked") : r.expired ? t("expired") : r.expires_at ? t("until {date}", { date: showDate(r.expires_at) }) : t("no expiry");
      return h(
        "div.cfg-row",
        { class: i === this.cursor ? "cur" : "", onclick: () => ((this.cursor = i), this.render()) },
        h("span", r.label),
        h("span.cfg-value", { class: r.revoked || r.expired ? "warn" : "" }, `${roleLabel(r.role)} · ${status}`),
      );
    });
    this.body.replaceChildren(h("div.cfg-title", t("Remote Access · {company}", { company: this.list.company })), head, ...rows);
    this.hint.replaceChildren(
      h("p", ...rich(t("{invite}: invite someone (one code to send them). {revoke}: revoke the selected access."), { invite: bold("Alt+C"), revoke: bold("Alt+D") })),
      h("p", t("Accountants can enter vouchers; Viewers can only see reports. Everyone's entries are in the audit trail.")),
    );
  }

  contexts(): Context[] {
    return ["Report"];
  }

  bottom() {
    return [
      { chord: "Alt+C", label: "Invite", enabled: Boolean(this.list) },
      { chord: "Alt+D", label: "Revoke", enabled: Boolean(this.list && this.rows().length) },
      this.shell.btn("Back", this.contexts()),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    (document.activeElement as HTMLElement | null)?.blur?.();
  }

  key(e: KeyEv): boolean {
    const n = this.rows().length;
    if (e.chord === "Up" && n) this.cursor = (this.cursor + n - 1) % n;
    else if (e.chord === "Down" && n) this.cursor = (this.cursor + 1) % n;
    else if (e.chord === "Alt+C") void this.invite();
    else if (e.chord === "Alt+D") void this.revoke();
    else if (e.chord === "Enter" && !this.list) void connectCompany(this.shell).then(() => this.load());
    else return false;
    this.render();
    return true;
  }

  async resume(): Promise<void> {
    await this.load();
  }

  private async invite(): Promise<void> {
    if (!this.list) return;
    const role = await choose(this.shell, "Invite as", [
      { key: "accountant", label: "Accountant", hint: "can enter and change vouchers" },
      { key: "viewer", label: "Viewer", hint: "reports only (auditors, advisors)" },
    ]);
    if (!role) return;
    const v = await form(
      this.shell,
      t("Invite a {role}", { role: roleLabel(role) }),
      [
        { label: "Who is it for", placeholder: "Ravi (accounts)" },
        { label: "Expires in days", value: role === "viewer" ? "30" : "", placeholder: "blank = never" },
        { label: "Server address", value: this.list.default_url },
      ],
      ([who, days, url]) =>
        !who.trim()
          ? t("Type who the invite is for.")
          : days.trim() && !(/^\d+$/.test(days.trim()) && +days >= 1 && +days <= 3650)
            ? t("Expiry: leave blank, or enter 1 to 3650 days.")
            : !url.trim()
              ? t("Enter the server address colleagues will use.")
              : null,
      this.list.hosted ? "The address must be reachable from the other computer (same office network)." : undefined,
    );
    if (!v) return;
    try {
      const inv = await this.shell.transition(sharingApi.createInvite(role, v[0], v[1].trim() ? +v[1] : null, v[2]));
      await this.shell.push(new InviteCodeScreen(this.shell, inv));
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async revoke(): Promise<void> {
    const r = this.rows()[this.cursor];
    if (!r) return;
    if (r.role === "owner") return this.shell.warn(t("The owner's own access can't be revoked here."));
    if (r.revoked) return this.shell.warn(t("{name}'s access is already revoked.", { name: r.label }));
    if (!(await this.shell.ask(t("Revoke {name}'s access?", { name: r.label }), { yesFirst: false }))) return;
    try {
      await this.shell.transition(sharingApi.revokeAccess(r.id));
      this.shell.info(t("{name} can no longer open this company.", { name: r.label }));
      await this.load();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

/** The new invite, shown once in large type with a Copy button. */
class InviteCodeScreen implements Screen {
  title = "Invite Code";
  el: HTMLElement;

  constructor(private shell: Shell, private inv: Invite) {
    const copy = h("button.invite-copy", { tabIndex: -1, onclick: () => void this.copy() }, t("Enter: Copy code"));
    const browser =
      inv.role === "viewer" && inv.portal_url
        ? h(
            "div.cfg-hint",
            h(
              "p",
              ...rich(t("{lead} They can open {url} in a browser on the office network and enter:"), {
                lead: bold(t("No KhataOS on their computer?")),
                url: bold(ltr(inv.portal_url)),
              }),
            ),
            h("p", ...rich(t("Company id: {id}"), { id: bold(ltr(inv.company_id)) })),
            h("p", ...rich(t("Token: {token}"), { token: bold(ltr(inv.token)) })),
          )
        : null;
    this.el = h(
      "section.cfg",
      h(
        "div.cfg-card",
        h("div.cfg-title", t("Invite for {name} · {role}", { name: inv.label, role: roleLabel(inv.role) })),
        h(
          "div",
          { style: { padding: "14px", display: "flex", flexDirection: "column", gap: "12px" } },
          h("div.invite-code", inv.code),
          copy,
          h(
            "div.cfg-hint",
            h("p", ...rich(t("Send this code to them (WhatsApp, e-mail). In KhataOS they choose {connect} and paste it."), { connect: bold(t("Connect to company…")) })),
            h("p", t("It works until it expires or you revoke it. Treat it like a key: anyone with it can open the books. It is shown only once.")),
          ),
          browser,
        ),
      ),
    );
  }

  contexts(): Context[] {
    return ["Report"];
  }

  bottom() {
    return [{ chord: "Enter", label: "Copy" }, this.shell.btn("Back", this.contexts())].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Enter" || e.chord === "Ctrl+C") {
      void this.copy();
      return true;
    }
    return false;
  }

  private async copy(): Promise<void> {
    if (await copyText(this.inv.code)) this.shell.info(t("Invite code copied. Paste it into WhatsApp or e-mail."));
    else this.shell.warn(t("Couldn't copy: select the code with the mouse and copy it."));
  }
}

// ------------------------------------------------- Connect to company…

/** Paste an invite code: KhataOS makes the company file, pulls the books and opens them. */
export class ConnectCompanyScreen implements Screen {
  title = "Connect to Company";
  el: HTMLElement;
  private input: HTMLTextAreaElement;
  private preview = h("div.cfg-hint");
  private seq = 0;

  constructor(private shell: Shell) {
    this.input = h("textarea.code-input", { placeholder: t("Paste the invite code here (it starts with khata1:)"), spellcheck: false });
    this.input.addEventListener("input", () => void this.inspect());
    this.el = h(
      "section.cfg",
      h(
        "div.cfg-card",
        h("div.cfg-title", t("Connect to a shared company")),
        h("div", { style: { padding: "14px", display: "flex", flexDirection: "column", gap: "10px" } }, this.input, this.preview),
      ),
      h(
        "div.cfg-hint",
        h("p", t("Ask the company's owner for an invite code (Company → Remote Access → Invite on their computer).")),
        h("p", t("Office network: this computer must be on the same network as the server PC, and KhataOS must be open there.")),
      ),
    );
  }

  contexts(): Context[] {
    return ["Dialog"];
  }

  bottom() {
    return [
      { chord: "Ctrl+A", label: "Connect" },
      { chord: "Esc", label: "Back" },
    ];
  }

  focus(): void {
    this.input.focus();
  }

  private async inspect(): Promise<InvitePreview | null> {
    const n = ++this.seq;
    const code = this.input.value.trim();
    if (!code) {
      this.preview.textContent = "";
      return null;
    }
    try {
      const p = await sharingApi.inspectInvite(code);
      if (n === this.seq)
        this.preview.replaceChildren(
          h("p", ...rich(t("Company: {name}"), { name: bold(p.company_name) })),
          h("p", ...rich(p.encrypted ? t("Server: {url} (end-to-end encrypted)") : t("Server: {url}"), { url: bold(ltr(p.url)) })),
          h("p", ...rich(p.role === "viewer" ? t("Access: {role} (read-only)") : t("Access: {role}"), { role: bold(roleLabel(p.role)) })),
          h("p", t("Press Enter or Ctrl+A to connect.")),
        );
      return p;
    } catch (e) {
      if (n === this.seq) this.preview.replaceChildren(h("p", { style: { color: "var(--warn)" } }, ltr(e instanceof Error ? e.message : String(e))));
      return null;
    }
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Enter" || e.chord === "Ctrl+A") {
      void this.connect();
      return true;
    }
    if (e.chord === "Esc") {
      if (this.input.value) {
        this.input.value = "";
        this.preview.textContent = "";
      } else this.shell.pop();
      return true;
    }
    return false;
  }

  private async connect(): Promise<void> {
    const p = await this.inspect();
    if (!p) return this.shell.warn(t("Paste the whole invite code first."));
    this.shell.info(t("Connecting to {company}…", { company: p.company_name }));
    try {
      const info = await this.shell.transition(sharingApi.joinCompany(this.input.value));
      await this.shell.openCompany(info);
      this.shell.info(
        p.role === "viewer"
          ? t("{company} is open read-only: you can see every report but not make entries.", { company: info.name })
          : t("{company} is connected. Your entries sync automatically.", { company: info.name }),
      );
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ------------------------------------------------------ Browser Access

class BrowserAccessScreen extends InfoList {
  title = "Browser Access";
  private c!: SharingConfig;
  private a!: OnlineAccess;

  constructor(shell: Shell) {
    super(shell, t("Browser Access · read-only reports without installing KhataOS"));
  }

  protected async load(): Promise<void> {
    [this.c, this.a] = await Promise.all([sharingApi.config(), sharingApi.onlineAccess()]);
    this.hint.replaceChildren(
      this.a.encrypted
        ? h("p", t("This company syncs through the encrypted relay, which can't read the books, so there is no browser access. Send a Viewer invite or an accountant file instead."))
        : h(
            "p",
            ...rich(t("Create a {viewer} invite in Remote Access. The invite screen shows the company id and token to enter on this page. Viewers see reports only."), {
              viewer: bold(t("Viewer")),
            }),
          ),
    );
  }

  protected buildRows(): Row[] {
    const port = this.c.server_port ?? this.c.port;
    const urls = this.a.hosted ? this.c.addresses.map((ip) => `http://${ip}:${port}/portal`) : this.a.url && !this.a.encrypted ? [`${this.a.url}/portal`] : [];
    return [
      { label: t("Open in a browser"), value: () => (urls.length ? ltr(urls.join(", ")) : t("Share this company first (Company → Connect)")) },
      { label: t("Company id"), value: () => (this.a.linked ? t("shown on the invite screen") : "—") },
    ];
  }
}

// -------------------------------------------- accountant files (.khatapkg)

/** Alt+M → Send to accountant (file). */
export async function sendToAccountant(shell: Shell): Promise<void> {
  if (!shell.company) return shell.warn(t("Open a company first."));
  try {
    const from = await sharingApi.packageNextFrom();
    let full = true;
    if (from > 0) {
      const what = await choose(shell, "Send", [
        { key: "new", label: "New entries only", hint: "since the last file" },
        { key: "all", label: "Everything", hint: "the whole books" },
      ]);
      if (!what) return;
      full = what === "all";
    }
    const pass = await form(
      shell,
      "Passphrase for the file",
      [
        { label: "Passphrase", secret: true },
        { label: "Repeat", secret: true },
      ],
      ([a, b]) => (a.length < 8 ? t("Use at least 8 characters.") : a !== b ? t("The two passphrases differ.") : null),
      "Tell the accountant the passphrase by phone or in person, not in the same message as the file.",
    );
    if (!pass) return;
    const stamp = new Date().toISOString().slice(0, 10);
    const slug = shell.company.path.split(/[\\/]/).pop()?.replace(/\.khata$/, "") || "company";
    const path = await save({
      title: t("Save the accountant file"),
      defaultPath: `${slug}-${stamp}.khatapkg`,
      filters: [{ name: t("KhataOS accountant file"), extensions: ["khatapkg"] }],
    });
    if (!path) return;
    shell.info(t("Encrypting…"));
    const s = await shell.transition(sharingApi.packageSend(path, pass[0], full));
    const file = ltr(String(s.path.split(/[\\/]/).pop()));
    const notes = s.records ? ` ${t("It also carries {n} update(s) to notes and settings.", { n: s.records })}` : "";
    shell.info(
      t(s.entries === 1 ? "Saved {n} entry to {file}. Send it any way you like; it is encrypted." : "Saved {n} entries to {file}. Send it any way you like; it is encrypted.", {
        n: s.entries,
        file,
      }) + notes,
    );
  } catch (e) {
    shell.error(e);
  }
}

/** Alt+O → Open accountant file (.khatapkg): into the open company, or a new one. */
export async function openAccountantFile(shell: Shell): Promise<void> {
  let picked: string | string[] | null;
  try {
    picked = await open({ multiple: false, title: t("Open an accountant file"), filters: [{ name: t("KhataOS accountant file"), extensions: ["khatapkg"] }] });
  } catch (e) {
    return shell.error(e);
  }
  const path = Array.isArray(picked) ? picked[0] : picked;
  if (!path) return;
  try {
    const info = await sharingApi.packageInspect(path);
    const target = info.for_open_company
      ? t("They will be checked, then added into the open company.")
      : info.from_seq === 0
        ? t("They will be checked, then added as a company on this computer.")
        : t("They will be checked, then added into its company.");
    const pass = await form(
      shell,
      t(info.entries === 1 ? "{company}: {n} entry" : "{company}: {n} entries", { company: info.company, n: info.entries }),
      [{ label: "Passphrase", secret: true }],
      ([p]) => (p ? null : t("Enter the passphrase the sender gave you.")),
      target,
    );
    if (!pass) return;
    shell.info(t("Checking the file…"));
    const r = await shell.transition(sharingApi.packageReceive(path, pass[0]));
    const counts = { n: r.added, had: r.already_had };
    const what =
      r.added === 0
        ? t("Nothing new: this computer already had all {had} entries.", counts)
        : r.already_had
          ? t(r.added === 1 ? "Added {n} entry ({had} already here)." : "Added {n} entries ({had} already here).", counts)
          : t(r.added === 1 ? "Added {n} entry." : "Added {n} entries.", counts);
    const notes = r.records ? ` ${t("Updated {n} note(s) and setting(s).", { n: r.records })}` : "";
    if (r.switched || !shell.company) await shell.openCompany(r.info);
    else {
      await shell.refreshCompany();
      if (shell.depth() === 1) await shell.reset(new HomeScreen(shell));
      else shell.refresh();
    }
    shell.info((r.created ? t("{company} created from the file. {result}", { company: r.info.name, result: what }) : what) + notes);
  } catch (e) {
    shell.error(e);
  }
}

// ----------------------------------------------------------------- menus

/** Items for Company (Alt+K): the ONLINE ACCESS section and its configuration. */
export function onlineAccessItems(shell: Shell) {
  const section = "Online Access";
  return [
    { id: "oa-connect", label: "Connect", section, data: () => void connectCompany(shell) },
    { id: "oa-disconnect", label: "Disconnect", section, data: () => void disconnectCompany(shell) },
    { id: "oa-status", label: "Connectivity Status", section, data: () => new ConnectivityStatusScreen(shell) },
    { id: "oa-remote", label: "Remote Access", sub: "invite, revoke", section, data: () => new RemoteAccessScreen(shell) },
    { id: "oa-browser", label: "Browser Access", section, data: () => new BrowserAccessScreen(shell) },
    { id: "oa-join", label: "Connect to company…", sub: "paste an invite code", section, data: () => new ConnectCompanyScreen(shell) },
    { id: "oa-move", label: "Move to another relay…", sub: "e.g. to your own relay", section, data: () => void moveToAnotherRelay(shell) },
    { id: "oa-config", label: "Configure Online Access", section: "Configure", data: () => new OnlineAccessConfigScreen(shell) },
  ];
}

/** Alt+Z: Exchange. Works with no company open (Server mode, joining). */
export function exchangeMenu(shell: Shell): Screen {
  const items: PickItem<() => Screen | void>[] = [
    { id: "x-config", label: "Configure: Client/Server", sub: "None · Client · Server · Both", section: "Data Synchronisation", data: () => new ClientServerConfigScreen(shell) },
    { id: "x-join", label: "Connect to company…", sub: "paste an invite code", section: "Data Synchronisation", data: () => new ConnectCompanyScreen(shell) },
    { id: "x-open-pkg", label: "Open accountant file (.khatapkg)", section: "Files", data: () => void openAccountantFile(shell) },
  ];
  if (shell.company)
    items.push(
      { id: "x-sync", label: "Sync now", sub: "Ctrl+Alt+S", section: "Data Synchronisation", data: () => void shell.runShellAction("SyncNow") },
      { id: "x-status", label: "Connectivity Status", sub: "", section: "Data Synchronisation", data: () => new ConnectivityStatusScreen(shell) },
      { id: "x-move", label: "Move to another relay…", sub: "e.g. to your own relay", section: "Data Synchronisation", data: () => void moveToAnotherRelay(shell) },
      { id: "x-send-pkg", label: "Send to accountant (file)", sub: "", section: "Files", data: () => void sendToAccountant(shell) },
    );
  return new MenuScreen(shell, "Exchange", items);
}

/** Alt+M: Share. */
export function shareMenu(shell: Shell): Screen {
  return new MenuScreen(shell, "Share", [
    { id: "s-pkg", label: "Send to accountant (file)", sub: "encrypted .khatapkg", data: () => void sendToAccountant(shell) },
    { id: "s-invite", label: "Invite someone", sub: "Remote Access", data: () => new RemoteAccessScreen(shell) },
  ]);
}

// ----------------------------------------------------- background sync

/**
 * Listen for background syncs (src-tauri/src/lan.rs emits `khata-sync`):
 * keep the title strip current and say when colleagues' entries arrive.
 */
export function installSharing(shell: Shell): void {
  let lastError = "";
  void listen<{ status: string; count: number; message: string }>("khata-sync", async (ev) => {
    const { status, message } = ev.payload;
    if (!shell.company) return;
    await shell.refreshCompany();
    shell.refresh();
    if (status === "pulled") {
      lastError = "";
      if (shell.depth() === 1 && shell.top()?.title === "Home") await shell.reset(new HomeScreen(shell));
      else shell.info(t("{result} from colleagues. Reopen reports to see them.", { result: ltr(message) }));
    } else if (status === "conflict") {
      shell.warn(t("{result}. Press Ctrl+Alt+S to sync and rebase your changes.", { result: ltr(message) }));
    } else if (status === "error") {
      if (message !== lastError) shell.warn(t("Sync: {error}", { error: ltr(message) }));
      lastError = message;
    } else lastError = "";
  });
}

// Sign-in and users (security control). A company with users asks who is
// there when it opens; each user's role limits what they may change. The
// rules are checked by the Rust command layer, so these screens only ask and
// explain. Passwords are typed into hidden fields, sent once and never kept.
import "../sharing.css";
import "../signin.css";
import { api } from "../api";
import { h } from "../dom";
import { t } from "../i18n";
import { letterOf } from "../keys";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import type { CompanyUserRow, Context, UserRole } from "../types";
import { FieldForm, frow } from "./settings";

export const ROLE_LABEL: Record<UserRole, string> = {
  Owner: "Owner",
  Accountant: "Accountant",
  DataEntry: "Data Entry",
  Auditor: "Auditor",
};

/** Highest first; each role can do everything the ones below it can. Labels and hints are shown through t(). */
const ROLES: { id: UserRole; hint: string }[] = [
  { id: "Owner", hint: "everything, including users, company details, tax rates and numbering" },
  { id: "Accountant", hint: "also alters, cancels and deletes saved entries and sets bank dates" },
  { id: "DataEntry", hint: "enters new vouchers and masters; can't change saved ones" },
  { id: "Auditor", hint: "sees reports and vouchers; changes nothing" },
];

const MIN_PASSWORD = 8;

function lastUserKey(shell: Shell): string {
  return `khataos.lastUser:${shell.company?.path ?? ""}`;
}

// ------------------------------------------------------------- sign in

/** Shown when a company with users opens: user name, password, Enter. */
export class SignInScreen implements Screen {
  title = "Sign In";
  el: HTMLElement;
  private nameIn = h("input.field", { spellcheck: false, autocomplete: "off", "aria-label": t("User name") });
  private passIn = h("input.field", { type: "password", spellcheck: false, autocomplete: "off", "aria-label": t("Password") });
  private at = 0;
  private busy = false;

  constructor(private shell: Shell) {
    const company = shell.company?.name ?? t("This company");
    this.el = h(
      "section.startup.signin",
      h(
        "div.startup-hero",
        h("div.startup-mark"),
        h("h1", "Khata", h("b", "OS")),
        h("p", t("{company} has users. Sign in with the user name and password its owner gave you.", { company })),
      ),
      h(
        "div.startup-panel",
        h("div.panel-title", t("Sign in to {company}", { company })),
        h("label.frow", h("span.flabel", t("User name")), this.nameIn),
        h("label.frow", h("span.flabel", t("Password")), this.passIn),
        h("p.note", t("Enter moves to the next field and signs in on the last. Esc goes back to the company list.")),
      ),
    );
    this.nameIn.addEventListener("focus", () => (this.at = 0));
    this.passIn.addEventListener("focus", () => (this.at = 1));
    try {
      this.nameIn.value = localStorage.getItem(lastUserKey(shell)) ?? "";
    } catch {
      /* storage unavailable: only a convenience */
    }
    if (this.nameIn.value) this.at = 1;
  }

  contexts(): Context[] {
    return ["Dialog"];
  }

  rebuild(): Screen {
    return new SignInScreen(this.shell);
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back";
  }

  bottom() {
    const ctx = this.contexts();
    return [this.shell.btn("Accept", ctx, "Sign in"), this.shell.btn("Back", ctx, "Companies")].filter(
      (b): b is NonNullable<typeof b> => b !== null,
    );
  }

  focus(): void {
    const inp = this.at === 0 ? this.nameIn : this.passIn;
    inp.focus();
    inp.select();
  }

  private go(i: number): void {
    this.at = i;
    this.focus();
  }

  key(e: KeyEv): boolean {
    if (e.chord === "Up" || e.chord === "Shift+Tab") {
      if (this.at === 1) this.go(0);
      return true;
    }
    if (e.chord === "Down" && this.at === 0) {
      this.go(1);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    switch (a) {
      case "Accept":
        void this.submit();
        return true;
      case "NextField":
        if (this.at === 0) {
          if (!this.nameIn.value.trim()) this.shell.warn(t("Enter your user name."));
          else this.go(1);
        } else void this.submit();
        return true;
      case "PreviousField":
        if (this.at === 1) this.go(0);
        return true;
      case "Back": {
        // F4: clear the field first; then back to the company list.
        const inp = this.at === 0 ? this.nameIn : this.passIn;
        if (inp.value) inp.value = "";
        else void this.shell.closeCompany();
        return true;
      }
    }
    return false;
  }

  private async submit(): Promise<void> {
    if (this.busy) return;
    const name = this.nameIn.value.trim();
    if (!name) {
      this.go(0);
      return this.shell.warn(t("Enter your user name."));
    }
    if (!this.passIn.value) {
      this.go(1);
      return this.shell.warn(t("Enter your password."));
    }
    this.busy = true;
    try {
      const session = await api.signIn(name, this.passIn.value);
      this.passIn.value = "";
      try {
        localStorage.setItem(lastUserKey(this.shell), session.user ?? name);
      } catch {
        /* storage unavailable: only a convenience */
      }
      await this.shell.signedIn(session);
    } catch (e) {
      this.passIn.value = "";
      this.go(1);
      this.shell.error(e);
    } finally {
      this.busy = false;
    }
  }

  dispose(): void {
    this.passIn.value = "";
  }
}

// --------------------------------------------------------------- users

/** Company → Users and passwords (Owner): the users, their roles, on or off. */
export class UsersScreen implements Screen {
  title = "Users and Passwords";
  el = h("section.cfg.users");
  private users: CompanyUserRow[] = [];
  private cursor = 0;
  private body = h("div.cfg-card");
  private hint = h("div.cfg-hint");

  constructor(private shell: Shell) {
    this.el.append(this.body, this.hint);
  }

  async init(): Promise<void> {
    this.users = await api.listUsers();
    this.render();
  }

  async resume(): Promise<void> {
    try {
      this.users = await api.listUsers();
    } catch (e) {
      this.shell.error(e);
    }
    await this.shell.loadSession();
    this.render();
    this.shell.refresh();
  }

  private render(): void {
    this.cursor = Math.max(0, Math.min(this.cursor, this.users.length - 1));
    const me = this.shell.session?.user?.toLowerCase();
    const head = h("div.cfg-row.info", h("span", h("b", t("Name"))), h("span", h("b", t("Role"))));
    const rows = this.users.map((u, i) =>
      h(
        "div.cfg-row",
        {
          class: i === this.cursor ? "cur" : "",
          onclick: () => ((this.cursor = i), this.render()),
          ondblclick: () => void this.alter(),
        },
        h("span", u.name, u.name.toLowerCase() === me ? h("span.muted", ` ${t("(you)")}`) : null),
        h("span.cfg-value", { class: u.disabled ? "warn" : "" }, u.disabled ? t("{role} · switched off", { role: t(ROLE_LABEL[u.role]) }) : t(ROLE_LABEL[u.role])),
      ),
    );
    const empty = this.users.length
      ? []
      : [h("div.cfg-row.info", h("span", t("No users yet")), h("span", t("anyone who opens this company can do everything")))];
    this.body.replaceChildren(h("div.cfg-title", t("Users · {company}", { company: this.shell.company?.name ?? "" })), head, ...rows, ...empty);
    this.hint.replaceChildren(
      this.users.length
        ? h("p", h("b", "Enter"), `: ${t("alter the selected user.")} `, h("b", "Alt+C"), `: ${t("add a user.")} `, h("b", "Alt+D"), `: ${t("remove the selected user.")}`)
        : h("p", h("b", "Alt+C"), `: ${t("adds the first user, who must be an Owner. From then on everyone signs in to open this company.")}`),
      ...ROLES.map((r) => h("p.role-line", h("b", t(ROLE_LABEL[r.id])), `: ${t(r.hint)}.`)),
    );
  }

  contexts(): Context[] {
    return ["Report"];
  }

  bottom() {
    const any = this.users.length > 0;
    return [
      { chord: "Enter", label: "Alter", enabled: any },
      { chord: "Alt+C", label: "Add User" },
      { chord: "Alt+D", label: "Remove", enabled: any },
      this.shell.btn("Back", this.contexts()),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    (document.activeElement as HTMLElement | null)?.blur?.();
  }

  key(e: KeyEv): boolean {
    const n = this.users.length;
    if (e.chord === "Up" && n) this.cursor = (this.cursor + n - 1) % n;
    else if (e.chord === "Down" && n) this.cursor = (this.cursor + 1) % n;
    else if (e.chord === "Home") this.cursor = 0;
    else if (e.chord === "End") this.cursor = n - 1;
    else if (e.chord === "Enter") void (n ? this.alter() : this.add());
    else if (e.chord === "Alt+C") void this.add();
    else if (e.chord === "Alt+D") void this.remove();
    else return false;
    this.render();
    return true;
  }

  private async add(): Promise<void> {
    await this.shell.push(new UserForm(this.shell, null, this.users.length === 0));
  }

  private async alter(): Promise<void> {
    const u = this.users[this.cursor];
    if (u) await this.shell.push(new UserForm(this.shell, u, false));
  }

  private async remove(): Promise<void> {
    const u = this.users[this.cursor];
    if (!u) return;
    const last = this.users.length === 1;
    const q = last
      ? t("Remove {name}? With no users left, anyone who opens this company can do everything.", { name: u.name })
      : t("Remove {name}? They will no longer be able to sign in.", { name: u.name });
    if (!(await this.shell.ask(q, { yesFirst: false }))) return;
    try {
      this.users = await api.removeUser(u.name);
      await this.shell.loadSession();
      this.shell.info(last ? t("{name} removed. This company no longer asks anyone to sign in.", { name: u.name }) : t("{name} removed.", { name: u.name }));
      this.render();
      this.shell.refresh();
    } catch (e) {
      this.shell.error(e);
    }
  }
}

// ----------------------------------------------------------- user form

type UserField = "name" | "role" | "password" | "confirm" | "off";

/** Add or alter one user. Altering keeps the name (it is how they sign in). */
class UserForm extends FieldForm<UserField> {
  title: string;
  private role: UserRole;
  private roleLine = h("div.nature");
  private off = false;

  constructor(
    shell: Shell,
    private user: CompanyUserRow | null,
    /** The company's first user: an Owner, and signed in once saved. */
    private first: boolean,
  ) {
    super(shell, ["name", "role", "password", "confirm", "off"]);
    this.title = user ? t("User Alteration: {name}", { name: user.name }) : "User Creation";
    this.role = user?.role ?? (first ? "Owner" : "DataEntry");
    this.off = user?.disabled ?? false;
    const i = this.inputs;
    i.name.placeholder = t("the name they sign in with");
    for (const f of ["password", "confirm"] as const) (i[f] as HTMLInputElement).type = "password";
    i.password.placeholder = user ? t("blank keeps the current password") : t("at least {n} characters", { n: MIN_PASSWORD });
    i.confirm.placeholder = t("type it again");
    const offIn = i.off as HTMLInputElement;
    offIn.readOnly = true;
    offIn.classList.add("yesno");
    offIn.setAttribute("aria-label", t("Switched off, Y or N"));
    this.layout(t(user ? "Alter User" : "Add User"), [
      user ? frow(t("Name"), h("div.fstatic", user.name)) : frow(t("Name"), i.name),
      frow(t("Role"), h("div.fstack", i.role, this.roleLine)),
      frow(t("Password"), i.password, user ? t("blank: unchanged") : undefined),
      frow(t("Confirm password"), i.confirm),
      this.first ? null : frow(t("Switched off"), h("div.fstack", i.off, h("div.nature", t("Yes: they can't sign in until switched back on"))), t("Y or N")),
      this.first
        ? h(
            "div.note",
            t("This is the company's first user, so they must be an Owner. Once saved, everyone signs in to open this company."),
            " ",
            t("A forgotten password can only be reset by an Owner, so consider adding a second Owner."),
          )
        : null,
    ]);
    this.pickers.role = new ListPicker<UserRole>({
      input: i.role as HTMLInputElement,
      host: this.el,
      title: "Role",
      columns: ["Role", "Can"],
      source: (q) =>
        ROLES.filter((r) => !this.first || r.id === "Owner")
          .filter((r) => [ROLE_LABEL[r.id], t(ROLE_LABEL[r.id])].some((x) => x.toLowerCase().includes(q.trim().toLowerCase())))
          .map<PickItem<UserRole>>((r) => ({ id: r.id, label: t(ROLE_LABEL[r.id]), aliases: [ROLE_LABEL[r.id]], sub: t(r.hint), data: r.id })),
      selectedId: () => this.role,
      onPick: (it) => {
        this.setRole(it.data);
        this.goTo("password");
      },
    });
    this.setRole(this.role);
    this.setOff(this.off);
  }

  protected fields(): UserField[] {
    const head: UserField[] = this.user ? [] : ["name"];
    return [...head, "role", "password", "confirm", ...(this.first ? [] : (["off"] as const))];
  }

  private setRole(r: UserRole): void {
    this.role = r;
    this.inputs.role.value = t(ROLE_LABEL[r]);
    this.roleLine.textContent = `(${t(ROLES.find((x) => x.id === r)!.hint)})`;
  }

  private setOff(off: boolean): void {
    this.off = off;
    this.inputs.off.value = t(off ? "Yes" : "No");
  }

  protected override pickedText(f: UserField): string {
    return f === "role" ? t(ROLE_LABEL[this.role]) : "";
  }

  override key(e: KeyEv): boolean {
    if (this.field() === "off") {
      const k = letterOf(e);
      if (!e.ctrlKey && !e.altKey && (k === "y" || k === "n")) {
        this.setOff(k === "y");
        return true;
      }
      if (e.chord === "Space") {
        this.setOff(!this.off);
        return true;
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.altKey) return true;
    }
    return super.key(e);
  }

  /** Why the passwords can't be saved, or null. */
  private passwordProblem(): string | null {
    const p = this.inputs.password.value;
    const c = this.inputs.confirm.value;
    if (!p && !this.user) return t("Give the new user a password.");
    if (p && p.length < MIN_PASSWORD) return t("Passwords need at least {n} characters.", { n: MIN_PASSWORD });
    if (p !== c) return t("The two passwords don't match. Type them again.");
    return null;
  }

  protected next(f: UserField): void {
    const i = this.inputs;
    if (f === "name") {
      if (!i.name.value.trim()) return this.shell.warn("Enter the user's name.");
      return this.goTo("role");
    }
    if (f === "role") return this.goTo("password");
    if (f === "password") {
      const p = i.password.value;
      if (!p && !this.user) return this.shell.warn("Give the new user a password.");
      if (p && p.length < MIN_PASSWORD) return this.shell.warn(t("Passwords need at least {n} characters.", { n: MIN_PASSWORD }));
      // Blank when altering: keep the current one, nothing to confirm.
      if (!p) return this.first ? void this.confirm() : this.goTo("off");
      return this.goTo("confirm");
    }
    if (f === "confirm") {
      if (i.confirm.value !== i.password.value) {
        i.password.value = "";
        i.confirm.value = "";
        this.goTo("password");
        return this.shell.warn("The two passwords don't match. Type them again.");
      }
      return this.first ? void this.confirm() : this.goTo("off");
    }
    void this.confirm();
  }

  protected async save(): Promise<void> {
    const name = this.user?.name ?? this.inputs.name.value.trim();
    if (!name) {
      this.goTo("name");
      return this.shell.warn("Enter the user's name.");
    }
    const problem = this.passwordProblem();
    if (problem) {
      this.goTo("password");
      return this.shell.warn(problem);
    }
    try {
      await api.saveUser(name, this.first ? "Owner" : this.role, this.inputs.password.value, this.first ? false : this.off);
      this.inputs.password.value = "";
      this.inputs.confirm.value = "";
      this.markClean();
      await this.shell.loadSession();
      this.shell.info(
        this.first
          ? t("{name} is the Owner and is signed in. From now on everyone signs in to open this company.", { name })
          : this.user
            ? t("{name} saved.", { name })
            : t("{name} added as {role}.", { name, role: t(ROLE_LABEL[this.role]) }),
      );
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  override dispose(): void {
    this.inputs.password.value = "";
    this.inputs.confirm.value = "";
    super.dispose();
  }
}

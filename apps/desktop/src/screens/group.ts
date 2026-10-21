// Group creation (rules Ma2, F1–F7, P6–P7): Name, then Under. A sub-group
// inherits its parent's nature, which shows in italics under the parent.
// Alteration (`alterId`) adds the alias field; a primary group stays primary.
import { api } from "../api";
import { h } from "../dom";
import { t } from "../i18n";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context, Group } from "../types";
import { natureLine } from "./ledger";

type Field = "name" | "alias" | "under";

export interface GroupFormOptions {
  name?: string;
  inline?: boolean;
  /** Parents that fit the calling field (P7). */
  parentFilter?: (g: Group) => boolean;
  onCreated?: (g: Group) => void;
  /** Alter this group instead of creating one. */
  alterId?: string;
}

/** "Bank Charges, Charges" → ["Bank Charges", "Charges"]. */
function splitAliases(text: string): string[] {
  return text.split(",").map((a) => a.trim()).filter(Boolean);
}

export class GroupForm implements Screen {
  title = "Group Creation";
  el = h("section.ledger");
  overlay: boolean;
  private at = 0;
  private parent: Group | null = null;
  private lastParent: Group | null = null;
  private inputs: Record<Field, HTMLInputElement>;
  private natureEl = h("div.nature");
  private inheritEl = h("div.opening-panel");
  private picker: ListPicker<Group>;
  /** The group being altered; null when creating. */
  private group: Group | null = null;
  /** The group and its sub-groups: never a parent for it. */
  private ownTree = new Set<string>();
  /** Field values as loaded, to tell whether an alteration changed anything. */
  private loaded = "";

  constructor(private shell: Shell, private opts: GroupFormOptions = {}) {
    this.overlay = Boolean(opts.inline);
    if (opts.alterId) this.title = "Group Alteration";
    const field = () => h("input.field", { spellcheck: false, autocomplete: "off" });
    this.inputs = { name: field(), alias: field(), under: field() };
    this.inputs.name.value = opts.name ?? "";
    this.inputs.alias.placeholder = t("Other names, separated by commas");
    (Object.keys(this.inputs) as Field[]).forEach((f) =>
      this.inputs[f].addEventListener("focus", () => this.focusField(this.fields().indexOf(f))),
    );
    this.el.append(
      h(
        "div.form.master",
        { class: this.overlay ? "card" : "" },
        this.overlay ? h("div.card-title", t("Create Group")) : null,
        h(
          "div.form-main",
          h("label.frow", h("span.flabel", t("Name")), this.inputs.name),
          opts.alterId
            ? h("label.frow", h("span.flabel", t("Alias"), h("small.fsub", t("found by these names too"))), this.inputs.alias)
            : null,
          h("label.frow", h("span.flabel", t("Under")), h("div.fstack", this.inputs.under, this.natureEl)),
        ),
        this.inheritEl,
      ),
    );
    this.paintInherit();
    this.picker = new ListPicker<Group>({
      input: this.inputs.under,
      host: this.el,
      title: "Groups",
      columns: ["Name", "Under"],
      source: async (q) =>
        (await api.searchGroups(q))
          .filter((g) => !this.opts.parentFilter || this.opts.parentFilter(g))
          .filter((g) => this.fitsAsParent(g))
          .map<PickItem<Group>>((g) => ({ id: g.id, label: g.name, aliases: g.aliases, sub: g.parent_name ?? t("Primary"), data: g })),
      selectedId: () => this.parent?.id ?? null,
      onPick: (it) => {
        this.setParent(it.data);
        void this.confirm();
      },
      noMatch: (text) => t('No group matches "{text}". Pick the group this one sits under.', { text }),
    });
  }

  /** Alteration: load the group, its parent and its own sub-groups. */
  async init(): Promise<void> {
    if (!this.opts.alterId) return;
    const [g, all] = await Promise.all([api.getGroup(this.opts.alterId), api.searchGroups("")]);
    this.group = g;
    this.ownTree = new Set([g.id]);
    for (let grew = true; grew; ) {
      grew = false;
      for (const x of all) {
        if (x.parent && this.ownTree.has(x.parent) && !this.ownTree.has(x.id)) {
          this.ownTree.add(x.id);
          grew = true;
        }
      }
    }
    this.inputs.name.value = g.name;
    this.inputs.alias.value = g.aliases.join(", ");
    if (g.parent) {
      this.setParent(all.find((x) => x.id === g.parent) ?? (await api.getGroup(g.parent)));
      this.lastParent = this.parent;
    } else {
      // A primary group can be renamed but never moved.
      this.inputs.under.value = t("Primary");
      this.inputs.under.readOnly = true;
      this.inputs.under.tabIndex = -1;
      this.natureEl.textContent = natureLine(g);
      this.paintInherit();
    }
    this.loaded = this.snapshot();
  }

  /** Enter order: alteration adds aliases; a primary group's Under is fixed. */
  private fields(): Field[] {
    if (!this.opts.alterId) return ["name", "under"];
    return this.group && !this.group.parent ? ["name", "alias"] : ["name", "alias", "under"];
  }

  private field(): Field {
    return this.fields()[this.at];
  }

  /** A sub-group moves only under a group of the same kind, never into itself. */
  private fitsAsParent(g: Group): boolean {
    const me = this.group;
    if (!me) return true;
    return !this.ownTree.has(g.id) && g.nature === me.nature && g.is_cash_or_bank === me.is_cash_or_bank;
  }

  contexts(): Context[] {
    return this.field() === "under" ? ["ListField", "Master"] : ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back" || (a === "DeleteRecord" && Boolean(this.group));
  }

  bottom() {
    const ctx = this.contexts();
    return [
      this.shell.btn("Accept", ctx),
      this.group ? this.shell.btn("DeleteRecord", ctx, "Delete") : null,
      this.shell.btn("Back", ctx, this.overlay ? "Cancel" : "Back"),
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    this.go(this.at);
  }

  private focusField(i: number): void {
    if (i < 0) return; // a primary group's fixed Under
    const f = this.fields()[i];
    if (this.at === i && this.picker.isOpen === (f === "under")) return;
    this.at = i;
    if (f === "under") void this.picker.open();
    else {
      this.picker.close();
      if (this.parent) this.inputs.under.value = this.parent.name;
    }
    this.shell.refresh();
  }

  private go(i: number): void {
    const f = this.fields()[i];
    this.inputs[f].focus();
    this.focusField(i);
    this.inputs[f].select();
  }

  private setParent(g: Group): void {
    this.parent = g;
    this.inputs.under.value = g.name;
    this.natureEl.textContent = natureLine(g);
    this.paintInherit();
  }

  private paintInherit(): void {
    const primary = this.group && !this.group.parent ? this.group : null;
    const g = primary ?? this.parent;
    this.inheritEl.replaceChildren(
      h("div.panel-title", t(primary ? "Set by this primary group" : "Inherited from the parent")),
      h("div.prow", h("span", t("Nature")), h("span", g ? t(g.nature) : "–")),
      h("div.prow", h("span", t("Cash or bank")), h("span", g ? t(g.is_cash_or_bank ? "Yes" : "No") : "–")),
      h("div.prow", h("span", t("Party (bill-wise)")), h("span", g ? t(g.is_party ? "Yes" : "No") : "–")),
    );
  }

  key(e: KeyEv): boolean {
    if (this.field() === "under" && this.picker.key(e)) return true;
    if (e.chord === "Up" && this.at === 0) return true; // F3
    if (e.chord === "Up") {
      this.go(this.at - 1);
      return true;
    }
    return false;
  }

  action(a: string): boolean {
    switch (a) {
      case "Accept":
        void this.save();
        return true;
      case "NextField": {
        const f = this.field();
        if (f === "name" && !this.inputs.name.value.trim()) this.shell.warn("Enter a name for the group.");
        else if (this.at < this.fields().length - 1) this.go(this.at + 1);
        else if (f !== "under" || this.parent) void this.confirm();
        else this.shell.warn("Pick the group this one sits under.");
        return true;
      }
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "DeleteRecord":
        if (!this.group) return false;
        void this.remove();
        return true;
      case "CreateMasterInline":
        return this.field() === "under";
      case "Back":
        void this.escape();
        return true;
    }
    return false;
  }

  private async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.go(0);
  }

  private snapshot(): string {
    return JSON.stringify([this.inputs.name.value.trim(), splitAliases(this.inputs.alias.value), this.parent?.id ?? null]);
  }

  private async escape(): Promise<void> {
    if (this.group) {
      if (this.snapshot() !== this.loaded && !(await this.shell.ask("Quit without saving?"))) return;
      this.shell.pop();
      return;
    }
    if (this.field() === "name" && this.inputs.name.value) {
      this.inputs.name.value = "";
      return;
    }
    const dirty = this.inputs.name.value.trim() !== (this.opts.name ?? "") || (this.parent && this.parent !== this.lastParent);
    if (dirty && !(await this.shell.ask("Quit without saving?"))) return;
    this.shell.pop();
  }

  /** Alt+D in alteration: delete after a Y/N prompt (refused while in use). */
  private async remove(): Promise<void> {
    const g = this.group;
    if (!g || !(await this.shell.ask(t('Delete group "{name}"?', { name: g.name }), { yesFirst: false }))) return;
    try {
      await api.deleteGroup(g.id);
      this.shell.info(t('Group "{name}" deleted.', { name: g.name }));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async save(): Promise<void> {
    const name = this.inputs.name.value.trim();
    if (!name) {
      this.go(0);
      return this.shell.warn("Enter a name for the group.");
    }
    if (this.group) return this.saveAlteration(this.group, name);
    if (!this.parent) {
      this.go(1);
      return this.shell.warn("Pick the group this one sits under.");
    }
    try {
      const g = await api.createGroup(name, this.parent.id);
      this.shell.info(t('Group "{name}" created under {group}.', { name: g.name, group: String(g.parent_name) }));
      if (this.opts.onCreated) {
        const cb = this.opts.onCreated;
        this.shell.pop();
        cb(g);
        return;
      }
      this.lastParent = this.parent; // F7: blank form, last parent carried over
      this.inputs.name.value = "";
      this.setParent(this.parent);
      this.go(0);
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async saveAlteration(g: Group, name: string): Promise<void> {
    // An empty parent keeps the current one; only a real move sends an id.
    const moved = g.parent && this.parent && this.parent.id !== g.parent ? this.parent.id : "";
    try {
      const saved = await api.alterGroup(g.id, name, moved, splitAliases(this.inputs.alias.value));
      this.shell.info(t('Group "{name}" altered.', { name: saved.name }));
      this.loaded = this.snapshot();
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  dispose(): void {
    this.picker.close();
  }
}

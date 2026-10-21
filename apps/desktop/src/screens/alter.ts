// Alter (A on the home menu): pick a master type, then the record, which
// opens in its form for alteration. The list stays open underneath, so Esc
// from the form comes back to it for the next record.
import { api } from "../api";
import { h } from "../dom";
import { t } from "../i18n";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import { KIND_LABEL, type Context } from "../types";
import { GodownForm } from "./godown";
import { GroupForm } from "./group";
import { StockGroupForm, StockItemForm, UnitForm } from "./inventory";
import { MenuScreen } from "./launcher";
import { LedgerForm } from "./ledger";
import { VoucherTypeForm } from "./settings";

type Open = () => Screen;

/** A type-to-filter list of records; Enter opens one, the list stays below. */
export class RecordList implements Screen {
  el: HTMLElement;
  overlay = true;
  private input = h("input.field.launcher-input", { placeholder: t("Type to filter"), spellcheck: false });
  private picker: ListPicker<Open>;

  constructor(
    private shell: Shell,
    readonly title: string,
    columns: [string, string],
    source: (q: string) => Promise<PickItem<Open>[]>,
  ) {
    this.el = h("section.launcher.goto", h("div.launcher-title", t(title)), this.input);
    this.picker = new ListPicker<Open>({
      input: this.input,
      host: this.el,
      title,
      inline: true,
      columns,
      source,
      onPick: (it) => void this.shell.push(it.data()),
      noMatch: (text) => t('Nothing matches "{text}".', { text }),
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

  /** Back from a form: names may have changed, or the record gone. */
  resume(): void {
    void this.picker.reload();
  }

  key(e: KeyEv): boolean {
    return this.picker.key(e);
  }

  action(a: string): boolean {
    if (a === "Back") {
      this.shell.pop();
      return true;
    }
    return this.picker.action(a);
  }

  dispose(): void {
    this.picker.close();
  }
}

function groupList(shell: Shell): Screen {
  return new RecordList(shell, "List of Groups", ["Name", "Under"], async (q) =>
    (await api.searchGroups(q)).map((g) => ({
      id: g.id,
      label: g.name,
      aliases: g.aliases,
      sub: g.parent_name ?? t("Primary"),
      data: () => new GroupForm(shell, { alterId: g.id }),
    })),
  );
}

function ledgerList(shell: Shell): Screen {
  return new RecordList(shell, "List of Ledgers", ["Name", "Under"], async (q) =>
    (await api.listLedgers(q, "all")).map((l) => ({
      id: l.id,
      label: l.name,
      sub: l.group_name,
      data: () => new LedgerForm(shell, { alterId: l.id }),
    })),
  );
}

function stockItemList(shell: Shell): Screen {
  return new RecordList(shell, "List of Stock Items", ["Name", "Under"], async (q) =>
    (await api.listStockItems(q)).map((s) => ({
      id: s.id,
      label: s.name,
      sub: s.group_name || t("Primary"),
      data: () => new StockItemForm(shell, { alterId: s.id }),
    })),
  );
}

function stockGroupList(shell: Shell): Screen {
  return new RecordList(shell, "List of Stock Groups", ["Name", "Under"], async (q) =>
    (await api.listStockGroups(q)).map((g) => ({
      id: g.id,
      label: g.name,
      sub: g.parent_name || t("Primary"),
      data: () => new StockGroupForm(shell, { alterId: g.id }),
    })),
  );
}

function unitList(shell: Shell): Screen {
  return new RecordList(shell, "List of Units", ["Symbol", "Name"], async (q) => {
    const s = q.toLowerCase();
    return (await api.listUnits())
      .filter((u) => u.symbol.toLowerCase().includes(s) || u.name.toLowerCase().includes(s))
      .map((u) => ({ id: u.id, label: u.symbol, sub: u.name, data: () => new UnitForm(shell, { alterId: u.id }) }));
  });
}

/** The eight voucher types, with the next number each would give today. */
function voucherTypeList(shell: Shell): Screen {
  return new RecordList(shell, "List of Voucher Types", ["Voucher type", "Numbering"], async (q) => {
    const s = q.toLowerCase();
    return (await api.listNumbering())
      .filter((n) => KIND_LABEL[n.kind].toLowerCase().includes(s) || t(KIND_LABEL[n.kind]).includes(q))
      .map((n) => ({
        id: n.kind,
        label: t(KIND_LABEL[n.kind]),
        sub: t("next {number} · {used} saved", { number: n.next_number, used: n.used }),
        data: () => new VoucherTypeForm(shell, n.kind),
      }));
  });
}

function godownList(shell: Shell): Screen {
  return new RecordList(shell, "List of Godowns", ["Name", "Under"], async (q) =>
    (await api.listGodowns(q)).map((g) => ({
      id: g.id,
      label: g.name,
      sub: g.parent_name ?? t("Primary"),
      data: () => new GodownForm(shell, { alterId: g.id }),
    })),
  );
}

export function alterMenu(shell: Shell): Screen {
  return new MenuScreen(shell, "Alter", [
    { id: "group", label: "Group", section: "Accounting Masters", data: () => groupList(shell) },
    { id: "ledger", label: "Ledger", section: "Accounting Masters", data: () => ledgerList(shell) },
    { id: "voucher-type", label: "Voucher Type", sub: "numbering", section: "Accounting Masters", data: () => voucherTypeList(shell) },
    { id: "stock-item", label: "Stock Item", section: "Inventory Masters", data: () => stockItemList(shell) },
    { id: "stock-group", label: "Stock Group", section: "Inventory Masters", data: () => stockGroupList(shell) },
    { id: "unit", label: "Unit", section: "Inventory Masters", data: () => unitList(shell) },
    { id: "godown", label: "Godown", section: "Inventory Masters", data: () => godownList(shell) },
  ]);
}

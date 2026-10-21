// Country pick (rule C2): the first time a company opens, choose the
// country pack. It sets the financial year, currency and tax registration
// rules (GST for India). The choice is part of the books, so it syncs.
import { api } from "../api";
import { h } from "../dom";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import type { Context, CountryInfo } from "../types";
import * as D from "../dates";
import { ltr, t } from "../i18n";

export class CountryScreen implements Screen {
  title = "Select Country";
  el: HTMLElement;
  private input = h("input.field", { placeholder: t("Type to filter countries"), spellcheck: false });
  private panel = h("div.startup-panel");
  private picker: ListPicker<CountryInfo>;
  private countries: CountryInfo[] = [];

  constructor(private shell: Shell) {
    const company = shell.company?.name ?? t("this company");
    this.el = h(
      "section.startup",
      h(
        "div.startup-hero",
        h("div.startup-mark"),
        h("h1", "Khata", h("b", "OS")),
        h("p", t("Where does {company} keep its books? The country sets the financial year, the currency and the tax rules.", { company })),
      ),
      this.panel,
    );
    this.picker = new ListPicker<CountryInfo>({
      input: this.input,
      host: this.panel,
      title: "Countries",
      inline: true,
      columns: ["Country", "Currency · Year · Tax ID"],
      source: (q) => {
        const s = q.toLowerCase();
        // Names that start with the typed text come first ("S" puts Saudi Arabia before the UAE).
        const starts = (c: CountryInfo) => c.name.toLowerCase().startsWith(s) || t(c.name).toLowerCase().startsWith(s) || c.code.toLowerCase() === s;
        return this.countries
          .filter((c) => c.name.toLowerCase().includes(s) || t(c.name).toLowerCase().includes(s) || c.code.toLowerCase() === s)
          .sort((a, b) => Number(starts(b)) - Number(starts(a)))
          .map<PickItem<CountryInfo>>((c) => ({
            id: c.code,
            label: t(c.name),
            aliases: [c.name],
            sub: `${c.currency} · ${D.show(c.fy_start)} – ${D.show(c.fy_end)} · ${c.tax_id_label}`,
            data: c,
          }));
      },
      onPick: (it) => void this.pick(it.data),
      noMatch: (text) => t('No country pack matches "{text}" yet', { text }),
    });
  }

  async init(): Promise<void> {
    this.countries = await api.listCountries();
    this.panel.replaceChildren(
      h("label.frow", h("span.flabel", t("Country")), this.input),
      h("p.note", t("This is set once per company. Esc goes back to the company list.")),
    );
    void this.picker.open();
  }

  contexts(): Context[] {
    return ["ListField"];
  }

  handles(a: string): boolean {
    return a === "Back";
  }

  bottom() {
    return [this.shell.btn("Back", this.contexts(), "Companies")].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    this.input.focus();
  }

  key(e: KeyEv): boolean {
    return this.picker.key(e);
  }

  action(a: string): boolean {
    // The picker clears typed text on the first Esc (F4); this is the second.
    if (a === "Back") {
      void this.shell.closeCompany();
      return true;
    }
    return this.picker.action(a);
  }

  private async pick(c: CountryInfo): Promise<void> {
    try {
      const info = await api.setCountry(c.code);
      this.shell.info(t("{company} keeps its books in {country}: {currency}, {taxId} for parties.", { company: info.name, country: t(c.name), currency: ltr(c.currency), taxId: ltr(c.tax_id_label) }));
      await this.shell.openCompany(info);
    } catch (e) {
      this.shell.error(e);
    }
  }

  dispose(): void {
    this.picker.close();
  }
}

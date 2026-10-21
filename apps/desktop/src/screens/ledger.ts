// Ledger creation (rules Ma2, Ma6, Ma7; field flow F1–F7; inline creation P7;
// party tax registration M2, M3). Alteration (`alterId`) uses the same form.
// A ledger under Duties & Taxes also says which tax it collects (CGST…) and
// whether on sales or purchases, so invoices can find it. A party ledger
// also keeps mailing details; its state is the place of supply when the
// party has no tax ID, and a country other than the company's marks a party
// abroad (exports). A sales, income, purchase or expense ledger can carry an
// HSN/SAC code or tax category, so invoices booked to it get their tax.
//
// India also has TDS (docs/india-tds.md): a party ledger keeps its PAN,
// deductee type and any lower-deduction certificate; an expense, purchase or
// fixed asset ledger its nature of payment; a duty ledger can hold TDS
// payable instead of collecting a tax on invoices.
//
// TCS too: a party keeps the buyer's terms (type, declaration, excluded
// buyer, lower-collection certificate), a sales or income ledger its nature
// of collection, and a duty ledger can hold TCS payable. A party abroad also
// gets its non-resident TDS terms (the user's rate, treaty or Act, Form 144
// country and nature of remittance, foreign tax number).
import { api, tcsApi, tdsApi, tdsStatementApi } from "../api";
import { h } from "../dom";
import { t } from "../i18n";
import { letterOf } from "../keys";
import * as M from "../money";
import { ListPicker, type PickItem } from "../picker";
import type { KeyEv, Screen, Shell } from "../shell";
import {
  DEDUCTEE_LABEL,
  TCS_ALL_NATURES,
  TDS_ALL_NATURES,
  type CodeName,
  type Context,
  type DeducteeKind,
  type Group,
  type Ledger,
  type LedgerInput,
  type LedgerTcs,
  type LedgerTds,
  type Mailing,
  type NonResidentTerms,
  type Region,
  type Side,
  type TaxIdCheck,
  type TaxRate,
  type TaxSide,
  type TcsNature,
  type TcsTerms,
  type TdsNature,
  type TdsTerms,
} from "../types";
import * as D from "../dates";
import { GroupForm } from "./group";
import { fxApi } from "../forex-api";
import { normaliseClass, percentText, rateFor } from "./settings";
import "../settings.css";
import "../ledger-extra.css";

type Field =
  | "name"
  | "under"
  | "currency"
  | "credit"
  | "taxId"
  | "override"
  | "pan"
  | "deductee"
  | "lowerRate"
  | "certNo"
  | "certFrom"
  | "certTo"
  | "address"
  | "country"
  | "sez"
  | "region"
  | "postal"
  | "phone"
  | "email"
  | "taxType"
  | "taxSide"
  | "tdsPayable"
  | "taxClass"
  | "tdsNature"
  | "tcsBuyer"
  | "tcsDecl"
  | "tcsExcluded"
  | "tcsRate"
  | "tcsCertNo"
  | "tcsFrom"
  | "tcsTo"
  | "nrRate"
  | "nrTreaty"
  | "nrCountry"
  | "nrRemit"
  | "nrTin"
  | "tcsPe"
  | "tcsNature"
  | "tcsPayable"
  | "amount"
  | "side";
/** Every field but the address, which is a textarea. */
type LineField = Exclude<Field, "address">;

/** Taxes offered for a duty ledger when the company has no country yet; any other name can be typed. */
const COMPONENTS = ["CGST", "SGST", "UTGST", "IGST", "Cess", "VAT"];
/**
 * The taxes each country pack puts on invoice lines (its line_tax), so a
 * duty ledger's Type of tax offers exactly those: India's GST heads,
 * VAT (UAE, Saudi Arabia, UK), USt (Germany), GST (Singapore), the US
 * sales tax levels (tax_us.rs COMPONENTS) and IVA (Mexico).
 */
const COMPONENTS_BY_COUNTRY: Record<string, string[]> = {
  IN: ["CGST", "SGST", "UTGST", "IGST", "Cess"],
  AE: ["VAT"],
  SA: ["VAT"],
  GB: ["VAT"],
  DE: ["USt"],
  SG: ["GST"],
  US: ["STATE", "COUNTY", "CITY", "DISTRICT"],
  MX: ["IVA", "RET-IVA", "RET-ISR"],
};
/** What each US sales tax level is, for the Type of tax list. */
const US_LEVELS: Record<string, string> = {
  STATE: "state sales tax",
  COUNTY: "county sales tax",
  CITY: "city sales tax",
  DISTRICT: "special district sales tax",
};
/** The core keeps tax names to 16 characters. */
const MAX_COMPONENT = 16;
const SIDE_LABEL: Record<TaxSide, string> = { Output: "Sales (output)", Input: "Purchases (input)" };
/** The Type of tax choice that makes a duty ledger a TDS payable ledger (India). */
const TDS_CHOICE = "\u0000tds";
/** The Type of tax choice that makes a duty ledger a TCS payable ledger (India). */
const TCS_CHOICE = "\u0000tcs";
/** Yes/No fields of the TCS and non-resident terms. */
const YN_FIELDS = ["tcsDecl", "tcsExcluded", "nrTreaty", "tcsPe"] as const;
type YnField = (typeof YN_FIELDS)[number];
function isYn(f: Field): f is YnField {
  return (YN_FIELDS as readonly string[]).includes(f);
}

/** Groups whose ledgers may carry an HSN/SAC: sales and income, purchases and expenses (input services). */
const CLASSED_GROUPS = ["Sales Accounts", "Direct Incomes", "Indirect Incomes", "Purchase Accounts", "Direct Expenses", "Indirect Expenses"];
/** Groups whose ledgers may carry a TDS nature of payment (India). */
const TDS_EXPENSE_GROUPS = ["Direct Expenses", "Indirect Expenses", "Purchase Accounts", "Fixed Assets"];
/** Groups whose ledgers may carry a TCS nature of collection (India): sales and other income. */
const TCS_SALES_GROUPS = ["Sales Accounts", "Direct Incomes", "Indirect Incomes"];
/** Form 144 country codes whose name differs from the country's English name. */
const FORM144_COUNTRY: Record<string, string> = { US: "267", KR: "133", MX: "160" };

/** Countries offered for a party abroad (ISO 3166-1 alpha-2), by name; any other code can be typed. */
const COUNTRIES: [string, string][] = [
  ["AU", "Australia"],
  ["BH", "Bahrain"],
  ["BD", "Bangladesh"],
  ["BR", "Brazil"],
  ["CA", "Canada"],
  ["CN", "China"],
  ["EG", "Egypt"],
  ["FR", "France"],
  ["DE", "Germany"],
  ["HK", "Hong Kong"],
  ["IN", "India"],
  ["ID", "Indonesia"],
  ["IT", "Italy"],
  ["JP", "Japan"],
  ["KE", "Kenya"],
  ["KW", "Kuwait"],
  ["MY", "Malaysia"],
  ["NP", "Nepal"],
  ["NL", "Netherlands"],
  ["NZ", "New Zealand"],
  ["NG", "Nigeria"],
  ["OM", "Oman"],
  ["PH", "Philippines"],
  ["QA", "Qatar"],
  ["SA", "Saudi Arabia"],
  ["SG", "Singapore"],
  ["ZA", "South Africa"],
  ["KR", "South Korea"],
  ["ES", "Spain"],
  ["LK", "Sri Lanka"],
  ["CH", "Switzerland"],
  ["TH", "Thailand"],
  ["TR", "Turkey"],
  ["AE", "United Arab Emirates"],
  ["GB", "United Kingdom"],
  ["US", "United States"],
  ["VN", "Vietnam"],
];

/** The country's name in the display language; an unknown code as it is. */
function countryName(code: string): string {
  const name = COUNTRIES.find(([c]) => c === code)?.[1];
  return name ? t(name) : code;
}

/** The core's namespace UUID for default groups, as its 16 bytes. */
const GROUP_NAMESPACE = "khata-os-groups1";

/**
 * Id of a default group (a UUID v5 of its original name, as the core makes
 * it), so a renamed default group is still found. Null without Web Crypto.
 */
async function defaultGroupId(name: string): Promise<string | null> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return null;
  const enc = new TextEncoder();
  const bytes = new Uint8Array([...enc.encode(GROUP_NAMESPACE), ...enc.encode(name)]);
  const b = new Uint8Array(await subtle.digest("SHA-1", bytes)).slice(0, 16);
  b[6] = (b[6] & 0x0f) | 0x50; // version 5
  b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
  const hex = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** "CESS" → "Cess", "UST" → "USt"; other names as stored. */
function componentLabel(c: string): string {
  const known = [...COMPONENTS, ...Object.values(COMPONENTS_BY_COUNTRY).flat()];
  return known.find((x) => x.toUpperCase() === c.toUpperCase()) ?? c.toUpperCase();
}

/** "s.394(1) … Scrap" for a TCS nature of collection. */
function tcsNatureLabel(natures: TcsNature[], code: string): string {
  if (code === TCS_ALL_NATURES) return t("All natures");
  const n = natures.find((x) => x.code === code);
  return n ? `${n.old_section} ${n.name}` : code;
}

// ------------------------------------------------------------ PAN (India)

const DEDUCTEE_KEYS = Object.keys(DEDUCTEE_LABEL) as DeducteeKind[];

/** The 4th character of a PAN names its holder type. */
const PAN_HOLDER: Record<string, DeducteeKind> = {
  P: "Individual",
  H: "Huf",
  C: "Company",
  F: "Firm",
  A: "Aop",
  B: "Boi",
  T: "Trust",
  L: "LocalAuthority",
  J: "Juridical",
  G: "Government",
};

/** Five letters, four digits, a letter, with a known holder type (AAAAA9999A). */
function panShape(p: string): boolean {
  return /^[A-Z]{5}\d{4}[A-Z]$/.test(p) && p[3] in PAN_HOLDER;
}

/** The PAN inside a GSTIN (characters 3 to 12), when it has the PAN's shape. */
function panOfGstin(gstin: string | null | undefined): string | null {
  const p = (gstin ?? "").trim().toUpperCase().slice(2, 12);
  return panShape(p) ? p : null;
}

function deducteeLabel(k: DeducteeKind): string {
  return t(DEDUCTEE_LABEL[k]);
}

/** "194C Contractors and sub-contractors". */
function natureLabel(natures: TdsNature[], code: string): string {
  if (code === TDS_ALL_NATURES) return t("All natures");
  const n = natures.find((x) => x.code === code);
  return n ? `${n.old_section} ${n.name}` : code;
}

/** TCS settings in a canonical form, to tell whether they changed. */
function tcsKey(tcs: LedgerTcs | null | undefined): string {
  if (!tcs) return "";
  if (tcs.kind === "Payable") return `P|${tcs.payable_for}`;
  const pct = (v: string | null | undefined) => (v == null || v === "" ? "" : M.toDecimal(M.of(v)));
  return JSON.stringify([
    tcs.nature ?? null,
    tcs.collectee ?? null,
    Boolean(tcs.declaration),
    Boolean(tcs.excluded),
    Boolean(tcs.non_resident),
    Boolean(tcs.non_resident && tcs.permanent_establishment),
    pct(tcs.lower_rate),
    tcs.lower_rate ? tcs.certificate ?? null : null,
    tcs.lower_rate ? tcs.valid_from ?? null : null,
    tcs.lower_rate ? tcs.valid_to ?? null : null,
  ]);
}

export interface LedgerFormOptions {
  name?: string;
  /** Stacked over the screen that asked for it (Alt+C), parent visible (L7). */
  inline?: boolean;
  /** Groups that fit the calling field (P7). */
  groupFilter?: (g: Group) => boolean;
  /** Called after save (create or alter) in inline mode; the form closes first. */
  onCreated?: (l: Ledger) => void;
  /** Alter this ledger instead of creating one. */
  alterId?: string;
}

export function natureLine(g: Group | null): string {
  if (!g) return "";
  return g.parent_name ? `(${g.parent_name})` : t("(Primary group · {nature})", { nature: t(g.nature) });
}

export function defaultSide(g: Group | null): Side {
  return g && (g.nature === "Liabilities" || g.nature === "Income") ? "Cr" : "Dr";
}

export class LedgerForm implements Screen {
  title = "Ledger Creation";
  el: HTMLElement;
  overlay: boolean;
  private at = 0;
  private group: Group | null = null;
  private lastGroup: Group | null = null;
  private side: Side | null = null;
  private inputs: Record<LineField, HTMLInputElement>;
  /** Mailing address, several lines: Enter adds a line, Enter on a blank last line moves on. */
  private address: HTMLTextAreaElement;
  private natureEl = h("div.nature");
  private picker: ListPicker<Group>;
  private baseDr = M.ZERO;
  private baseCr = M.ZERO;
  private panel = h("div.opening-panel");
  /** Party section (rule Ma1): shown only for Sundry Debtors/Creditors. */
  private partyRow: HTMLElement;
  /** Party, cash and bank ledgers: the currency the ledger is kept in (docs/multi-currency.md). */
  private currencyRow: HTMLElement;
  private currencyLine = h("div.nature");
  /** Currency codes the company knows (built-in master plus its own), and its own currency. */
  private currencyCodes = new Map<string, string>();
  private baseCurrency = "";
  /** "GSTIN" etc. from the company's country pack; null: no tax ID field. */
  private taxLabel: string | null;
  /** State and PAN of a valid ID, or why it is invalid. */
  private taxLine = h("div.nature");
  private overrideRow: HTMLElement;
  /** The last check of the tax ID field; null once it is edited. */
  private taxCheck: TaxIdCheck | null = null;
  /** The ledger being altered; null when creating. */
  private ledger: Ledger | null = null;
  /** Field values as loaded, to tell whether an alteration changed anything. */
  private loaded = "";
  /** Every group by id, for the Duties & Taxes ancestry check. */
  private groupsById = new Map<string, Group>();
  /** The Duties & Taxes group (by id: it may have been renamed). */
  private dutiesId: string | null = null;
  /** The chosen group is Duties & Taxes or under it. */
  private duty = false;
  /** Duty ledgers: which tax ("" none) and on which side. */
  private taxRow: HTMLElement;
  private taxPicker: ListPicker<string>;
  private taxComponent = "";
  private taxSide: TaxSide = "Output";
  /** The side was chosen by hand (else it follows the name: "Input CGST"). */
  private sideTouched = false;
  private taxTypeLine = h("div.nature");
  /** Mailing details (rule Ma1: parties only). */
  private mailingRow: HTMLElement;
  /** The country pack's states; empty: State is typed as text. */
  private regions: Region[] = [];
  private regionPicker: ListPicker<Region | null>;
  /** Chosen state (picker mode); a code the pack doesn't know is kept as is. */
  private region: Region | null = null;
  /** The state was filled in from the tax ID, so a new tax ID may replace it. */
  private regionFromTax = false;
  private regionLine = h("div.nature");
  private regionRow: HTMLElement;
  /** The company's own country code ("IN"); null before it is picked. */
  private home: string | null;
  /** A party abroad: its country code; null is the company's own country. */
  private country: string | null = null;
  private countryPicker: ListPicker<string | null>;
  private countryLine = h("div.nature");
  /** The party is an SEZ unit or developer (India) or in a designated zone (UAE). */
  private sez = false;
  private sezRow: HTMLElement;
  /** What the zone is called in the company's country; null: not asked. */
  private sezLabel: string | null;
  /** Sales, income, purchase and expense groups (by id), whose ledgers carry an HSN/SAC. */
  private classedIds = new Set<string>();
  /** The chosen group is one of those, or under one. */
  private classed = false;
  private classRow: HTMLElement;
  private classLine = h("div.nature");
  /** The tax rate list, for the rate a code gets today. */
  private rates: TaxRate[] = [];

  // TDS (India only).
  private india: boolean;
  /** Natures of payment from the pack; empty until loaded (or outside India). */
  private natures: TdsNature[] = [];
  /** Expense, purchase and fixed asset groups (by id), whose ledgers carry a nature of payment. */
  private tdsExpenseIds = new Set<string>();
  /** The chosen group is one of those, or under one. */
  private tdsExpense = false;
  /** Party ledgers: PAN, deductee type and lower-deduction certificate. */
  private tdsRow: HTMLElement;
  private panLine = h("div.nature");
  private deductee: DeducteeKind | null = null;
  private deducteePicker: ListPicker<DeducteeKind | null>;
  private deducteeLine = h("div.nature");
  private certRows: HTMLElement[] = [];
  /** Expense ledgers: the nature of payment. */
  private natureRow: HTMLElement;
  private tdsNature: string | null = null;
  private naturePicker: ListPicker<string | null>;
  private natureHint = h("div.nature");
  /** Duty ledgers: TDS payable instead of a tax on invoices, and for which nature. */
  private tdsDuty = false;
  private payableFor: string | null = null;
  private payablePicker: ListPicker<string | null>;
  private payableRow: HTMLElement;
  /** "Used on" (sales or purchases): not for a TDS payable ledger. */
  private sideRow: HTMLElement;
  private payableLine = h("div.nature");
  /** Alteration: the saved TDS settings in one line. */
  private tdsSummaryEl = h("div.nature");

  // TCS and non-resident terms (India only).
  /** Natures of collection from the pack; empty until loaded. */
  private tcsNatures: TcsNature[] = [];
  /** Form 144 countries and natures of remittance, for a non-resident payee. */
  private nrCountries: CodeName[] = [];
  private nrRemittances: CodeName[] = [];
  /** Sales and income groups (by id), whose ledgers carry a nature of collection. */
  private tcsSalesIds = new Set<string>();
  private tcsSales = false;
  /** Party ledgers: the buyer's terms. */
  private tcsRow: HTMLElement;
  private tcsBuyer: DeducteeKind | null = null;
  private tcsBuyerPicker: ListPicker<DeducteeKind | null>;
  private tcsBuyerLine = h("div.nature");
  private tcsCertRows: HTMLElement[] = [];
  /** Yes/No answers: declaration, excluded buyer, treaty rate, permanent establishment. */
  private yn: Record<YnField, boolean> = { tcsDecl: false, tcsExcluded: false, nrTreaty: false, tcsPe: false };
  /** A party abroad (or saved as a non-resident): rate, treaty, country, remittance, foreign tax number. */
  private nrRow: HTMLElement;
  private nrCountry: string | null = null;
  private nrCountryPicker: ListPicker<string | null>;
  private nrRemit: string | null = null;
  private nrRemitPicker: ListPicker<string | null>;
  private nrLine = h("div.nature");
  /** Sales and income ledgers: the nature of collection. */
  private tcsNatureRow: HTMLElement;
  private tcsNature: string | null = null;
  private tcsNaturePicker: ListPicker<string | null>;
  private tcsNatureHint = h("div.nature");
  /** Duty ledgers: TCS payable instead of a tax on invoices, and for which nature. */
  private tcsDuty = false;
  private tcsPayableFor: string | null = null;
  private tcsPayablePicker: ListPicker<string | null>;
  private tcsPayableRow: HTMLElement;

  constructor(private shell: Shell, private opts: LedgerFormOptions = {}) {
    this.overlay = Boolean(opts.inline);
    if (opts.alterId) this.title = "Ledger Alteration";
    const field = (cls = "") => h("input.field", { class: cls, spellcheck: false, autocomplete: "off" });
    this.inputs = {
      name: field(),
      under: field(),
      currency: field("taxid"),
      credit: field("amount"),
      taxId: field("taxid"),
      override: field(),
      pan: field("taxid"),
      deductee: field(),
      lowerRate: field("amount"),
      certNo: field(),
      certFrom: field(),
      certTo: field(),
      taxType: field(),
      taxSide: h("input.field.choice", { readOnly: true, value: t(SIDE_LABEL.Output), "aria-label": t("Sales or purchases") }),
      tdsPayable: field(),
      country: field(),
      sez: h("input.field.choice", { readOnly: true, value: t("No"), "aria-label": t("Yes or no") }),
      region: field(),
      postal: field(),
      taxClass: field(),
      tdsNature: field(),
      tcsBuyer: field(),
      tcsDecl: h("input.field.choice", { readOnly: true, value: t("No"), "aria-label": t("Yes or no") }),
      tcsExcluded: h("input.field.choice", { readOnly: true, value: t("No"), "aria-label": t("Yes or no") }),
      tcsRate: field("amount"),
      tcsCertNo: field(),
      tcsFrom: field(),
      tcsTo: field(),
      nrRate: field("amount"),
      nrTreaty: h("input.field.choice", { readOnly: true, value: t("No"), "aria-label": t("Yes or no") }),
      nrCountry: field(),
      nrRemit: field(),
      nrTin: field("taxid"),
      tcsPe: h("input.field.choice", { readOnly: true, value: t("No"), "aria-label": t("Yes or no") }),
      tcsNature: field(),
      tcsPayable: field(),
      phone: field(),
      email: field(),
      amount: field("amount"),
      side: h("input.field.drcr", { readOnly: true, value: t("Dr"), "aria-label": t("Dr or Cr") }),
    };
    this.address = h("textarea.field.multiline", { spellcheck: false, rows: 3, maxLength: 400 });
    for (const f of ["postal", "phone", "email"] as const) this.inputs[f].maxLength = 120;
    this.inputs.pan.maxLength = 10;
    this.inputs.certNo.maxLength = 40;
    this.inputs.tcsCertNo.maxLength = 40;
    this.inputs.nrTin.maxLength = 25;
    this.inputs.name.value = opts.name ?? "";
    (Object.keys(this.inputs) as LineField[]).forEach((f) =>
      this.inputs[f].addEventListener("focus", () => this.focusField(this.fields().indexOf(f), false)),
    );
    this.address.addEventListener("focus", () => this.focusField(this.fields().indexOf("address"), false));
    this.inputs.credit.placeholder = t("e.g. 30");
    this.taxLabel = this.shell.company?.country?.tax_id_label ?? null;
    this.inputs.taxId.placeholder = t("Leave blank if unregistered");
    this.inputs.override.placeholder = t("Why keep an ID that fails the check");
    this.inputs.taxId.addEventListener("input", () => {
      this.setTaxCheck(null);
      this.paintPan();
    });
    // M2: an invalid ID needs a reason to be kept; this row appears only then.
    this.overrideRow = h(
      "label.frow",
      h("span.flabel", t("Reason to keep it"), h("small.fsub", t("recorded in the audit trail"))),
      this.inputs.override,
    );
    this.overrideRow.hidden = true;
    this.partyRow = h(
      "div.fsection",
      h("div.fsection-title", t("Party details")),
      h("label.frow", h("span.flabel", t("Default credit period"), h("small.fsub", t("days, for new bills"))), h("div.famount", this.inputs.credit)),
      this.taxLabel
        ? h("label.frow", h("span.flabel", this.taxLabel), h("div.fstack", this.inputs.taxId, this.taxLine))
        : null,
      this.taxLabel ? this.overrideRow : null,
    );
    this.partyRow.hidden = true;
    this.inputs.currency.maxLength = 3;
    this.inputs.currency.addEventListener("input", () => this.paintCurrency());
    this.currencyRow = h(
      "label.frow",
      h("span.flabel", t("Currency"), h("small.fsub", t("blank: the company's own"))),
      h("div.fstack", this.inputs.currency, this.currencyLine),
    );
    this.currencyRow.hidden = true;
    void fxApi
      .currencies()
      .then((c) => {
        this.baseCurrency = c.base?.code ?? "";
        this.currencyCodes = new Map(c.currencies.map((x) => [x.code, x.name]));
        this.inputs.currency.placeholder = this.baseCurrency;
        this.paintCurrency();
      })
      .catch(() => {});
    const co = this.shell.company?.country ?? null;
    this.home = co?.code ?? null;
    this.india = this.home === "IN";
    // TDS on a party: PAN, deductee type, lower-deduction certificate.
    this.inputs.pan.placeholder = t("e.g. ABCDE1234F");
    this.inputs.pan.addEventListener("input", () => this.paintPan());
    this.inputs.lowerRate.placeholder = t("blank: none");
    this.inputs.lowerRate.addEventListener("input", () => this.paintCert());
    this.inputs.certFrom.placeholder = t("e.g. 1-4-2026");
    this.inputs.certTo.placeholder = t("e.g. 31-3-2027");
    this.certRows = [
      h("label.frow", h("span.flabel", t("Certificate no.")), this.inputs.certNo),
      h("label.frow", h("span.flabel", t("Valid from")), this.inputs.certFrom),
      h("label.frow", h("span.flabel", t("Valid to")), this.inputs.certTo),
    ];
    this.tdsRow = h(
      "div.fsection",
      h("div.fsection-title", t("TDS (tax deducted at source)")),
      h("label.frow", h("span.flabel", t("PAN")), h("div.fstack", this.inputs.pan, this.panLine)),
      h("label.frow", h("span.flabel", t("Deductee type")), h("div.fstack", this.inputs.deductee, this.deducteeLine)),
      h(
        "label.frow",
        h("span.flabel", t("Lower deduction rate %"), h("small.fsub", t("from a certificate"))),
        h("div.famount", this.inputs.lowerRate),
      ),
      ...this.certRows,
    );
    this.tdsRow.hidden = true;
    // TCS on a party: the buyer's terms.
    this.inputs.tcsRate.placeholder = t("blank: none");
    this.inputs.tcsRate.addEventListener("input", () => this.paintTcsCert());
    this.inputs.tcsFrom.placeholder = t("e.g. 1-4-2026");
    this.inputs.tcsTo.placeholder = t("e.g. 31-3-2027");
    this.tcsCertRows = [
      h("label.frow", h("span.flabel", t("Certificate no.")), this.inputs.tcsCertNo),
      h("label.frow", h("span.flabel", t("Valid from")), this.inputs.tcsFrom),
      h("label.frow", h("span.flabel", t("Valid to")), this.inputs.tcsTo),
    ];
    this.tcsRow = h(
      "div.fsection",
      h("div.fsection-title", t("TCS (tax collected at source)")),
      h("label.frow", h("span.flabel", t("Buyer type")), h("div.fstack", this.inputs.tcsBuyer, this.tcsBuyerLine)),
      h(
        "label.frow",
        h("span.flabel", t("Buyer's declaration"), h("small.fsub", t("Y or N"))),
        h("div.fstack", this.inputs.tcsDecl, h("div.nature", t("Goods bought for manufacturing, not trading (s.394(2)): nothing is collected."))),
      ),
      h(
        "label.frow",
        h("span.flabel", t("Not a buyer under the Act"), h("small.fsub", t("Y or N"))),
        h("div.fstack", this.inputs.tcsExcluded, h("div.nature", t("Government, embassy, public sector company, club…: nothing is collected."))),
      ),
      h(
        "label.frow",
        h("span.flabel", t("Lower collection rate %"), h("small.fsub", t("from a certificate"))),
        h("div.famount", this.inputs.tcsRate),
      ),
      ...this.tcsCertRows,
    );
    this.tcsRow.hidden = true;
    // A non-resident payee: the rate is always the user's.
    this.inputs.nrRate.placeholder = t("blank: none");
    this.inputs.nrRate.addEventListener("input", () => this.paintNr());
    this.inputs.nrTin.placeholder = t("e.g. 12-3456789");
    this.nrRow = h(
      "div.fsection",
      h("div.fsection-title", t("Non-resident terms")),
      h(
        "label.frow",
        h("span.flabel", t("TDS rate %"), h("small.fsub", t("from the treaty or the Act"))),
        h("div.fstack", h("div.famount", this.inputs.nrRate), this.nrLine),
      ),
      h("label.frow", h("span.flabel", t("Treaty rate"), h("small.fsub", t("Y or N"))), this.inputs.nrTreaty),
      h("label.frow", h("span.flabel", t("Country of residence"), h("small.fsub", t("Form 144 list"))), this.inputs.nrCountry),
      h("label.frow", h("span.flabel", t("Nature of remittance")), this.inputs.nrRemit),
      h("label.frow", h("span.flabel", t("Tax number abroad")), this.inputs.nrTin),
      h(
        "label.frow",
        h("span.flabel", t("Permanent establishment in India"), h("small.fsub", t("Y or N"))),
        h("div.fstack", this.inputs.tcsPe, h("div.nature", t("For TCS: without one, the no-PAN rate doesn't apply."))),
      ),
    );
    this.nrRow.hidden = true;
    this.address.placeholder = t("Building, street\nCity");
    this.inputs.region.addEventListener("input", () => {
      if (!this.regions.length) this.regionFromTax = false;
    });
    this.inputs.country.placeholder = co ? t("{country} (the company's country)", { country: t(co.name) }) : "";
    this.sezLabel = this.home === "IN" ? "SEZ unit or developer" : this.home === "AE" ? "Designated zone" : null;
    this.regionRow = h("label.frow", h("span.flabel", t("State")), h("div.fstack", this.inputs.region, this.regionLine));
    this.sezRow = h(
      "label.frow",
      h("span.flabel", this.sezLabel ? t(this.sezLabel) : "", h("small.fsub", t("Y or N"))),
      h(
        "div.fstack",
        this.inputs.sez,
        h(
          "div.nature",
          this.home === "AE"
            ? t("Taxed as usual; goods supplied within designated zones can be outside the scope of VAT.")
            : t("Sales to it are zero-rated: IGST or without tax under LUT."),
        ),
      ),
    );
    this.mailingRow = h(
      "div.fsection",
      h("div.fsection-title", t("Mailing details")),
      h("label.frow", h("span.flabel", t("Address"), h("small.fsub", t("Enter on a blank line moves on"))), this.address),
      h("label.frow", h("span.flabel", t("Country")), h("div.fstack", this.inputs.country, this.countryLine)),
      this.sezLabel ? this.sezRow : null,
      this.regionRow,
      h("label.frow", h("span.flabel", t("PIN / postal code")), this.inputs.postal),
      h("label.frow", h("span.flabel", t("Phone")), this.inputs.phone),
      h("label.frow", h("span.flabel", t("E-mail")), this.inputs.email),
    );
    this.mailingRow.hidden = true;
    this.inputs.taxType.placeholder = t("e.g. CGST");
    this.sideRow = h(
      "label.frow",
      h("span.flabel", t("Used on"), h("small.fsub", t("Space switches"))),
      h("div.fstack", this.inputs.taxSide, h("div.nature", t("Output tax is collected on sales; input tax is paid on purchases."))),
    );
    this.payableRow = h("label.frow", h("span.flabel", t("TDS payable for")), h("div.fstack", this.inputs.tdsPayable, this.payableLine));
    this.payableRow.hidden = true;
    this.tcsPayableRow = h(
      "label.frow",
      h("span.flabel", t("TCS payable for")),
      h("div.fstack", this.inputs.tcsPayable, h("div.nature", t("TCS collected in vouchers is credited here; challan payments debit it."))),
    );
    this.tcsPayableRow.hidden = true;
    this.taxRow = h(
      "div.fsection",
      h("div.fsection-title", t("Tax on invoices")),
      h("label.frow", h("span.flabel", t("Type of tax")), h("div.fstack", this.inputs.taxType, this.taxTypeLine)),
      this.sideRow,
      this.payableRow,
      this.tcsPayableRow,
    );
    this.taxRow.hidden = true;
    this.inputs.taxClass.placeholder = t("e.g. 998314");
    this.inputs.taxClass.maxLength = 40;
    this.inputs.taxClass.addEventListener("input", () => this.paintClass());
    this.classRow = h(
      "div.fsection",
      h("div.fsection-title", t("Tax on invoices")),
      h("label.frow", h("span.flabel", t("HSN/SAC or tax category"), h("small.fsub", t("blank: no tax worked out"))), h("div.fstack", this.inputs.taxClass, this.classLine)),
    );
    this.classRow.hidden = true;
    this.natureRow = h(
      "div.fsection",
      h("div.fsection-title", t("TDS (tax deducted at source)")),
      h("label.frow", h("span.flabel", t("Nature of payment (TDS)")), h("div.fstack", this.inputs.tdsNature, this.natureHint)),
    );
    this.natureRow.hidden = true;
    this.tcsNatureRow = h(
      "div.fsection",
      h("div.fsection-title", t("TCS (tax collected at source)")),
      h("label.frow", h("span.flabel", t("Nature of collection (TCS)")), h("div.fstack", this.inputs.tcsNature, this.tcsNatureHint)),
    );
    this.tcsNatureRow.hidden = true;
    this.tdsSummaryEl.hidden = true;
    this.inputs.amount.addEventListener("input", () => this.paintPanel());
    this.inputs.amount.addEventListener("blur", () => this.formatAmount());
    const fy = this.shell.company ? D.show(this.shell.company.fy_start) : "";
    this.el = h("section.ledger");
    const form = h(
      "div.form.master",
      { class: this.overlay ? "card" : "" },
      this.overlay ? h("div.card-title", t("Create Ledger")) : null,
      h(
        "div.form-main",
        h("label.frow", h("span.flabel", t("Name")), this.inputs.name),
        h("label.frow", h("span.flabel", t("Under")), h("div.fstack", this.inputs.under, this.natureEl, this.tdsSummaryEl)),
        this.currencyRow,
        this.partyRow,
        this.tdsRow,
        this.tcsRow,
        this.mailingRow,
        this.nrRow,
        this.taxRow,
        this.classRow,
        this.natureRow,
        this.tcsNatureRow,
        h(
          "label.frow",
          h("span.flabel", t("Opening balance"), fy ? h("small.fsub", t("as on {date}", { date: fy })) : null),
          h("div.famount", this.inputs.amount, this.inputs.side),
        ),
      ),
      this.panel,
    );
    this.el.append(form);
    this.picker = new ListPicker<Group>({
      input: this.inputs.under,
      host: this.el,
      title: "Groups",
      columns: ["Name", "Under"],
      source: async (q) => {
        const groups = await api.searchGroups(q);
        return groups
          .filter((g) => !this.opts.groupFilter || this.opts.groupFilter(g))
          .map<PickItem<Group>>((g) => ({ id: g.id, label: g.name, aliases: g.aliases, sub: g.parent_name ?? t("Primary"), data: g }));
      },
      selectedId: () => this.group?.id ?? null,
      onPick: (it) => {
        this.setGroup(it.data);
        this.afterUnder();
      },
      create: { noun: "group", run: (text) => this.createGroupInline(text) },
    });
    this.taxPicker = new ListPicker<string>({
      input: this.inputs.taxType,
      host: this.el,
      title: "Type of tax",
      source: (q) => {
        const s = q.trim().toLowerCase();
        const names = [...((this.home && COMPONENTS_BY_COUNTRY[this.home]) || COMPONENTS)];
        if (this.taxComponent && !names.some((c) => c.toUpperCase() === this.taxComponent.toUpperCase())) names.push(componentLabel(this.taxComponent));
        const all: PickItem<string>[] = [
          { id: "", label: t("None"), aliases: ["None"], sub: t("not a tax on invoices"), data: "" },
          ...names.map((c) => ({ id: c.toUpperCase(), label: c, sub: US_LEVELS[c] ? t(US_LEVELS[c]) : undefined, data: c })),
          ...(this.india
            ? [
                { id: "tds", label: t("TDS payable"), aliases: ["TDS payable"], sub: t("tax deducted at source, not on invoices"), data: TDS_CHOICE },
                { id: "tcs", label: t("TCS payable"), aliases: ["TCS payable"], sub: t("tax collected at source, on top of invoices"), data: TCS_CHOICE },
              ]
            : []),
          { id: "other", label: t("Other…"), aliases: ["Other…"], sub: t("type its name"), data: "\u0000other" },
        ];
        const items = all.filter((i) => !s || i.label.toLowerCase().includes(s) || i.aliases?.some((a) => a.toLowerCase().includes(s)));
        // Free text: any other tax's name, used as typed.
        if (s && !items.some((i) => i.label.toLowerCase() === s)) items.push({ id: "typed", label: q.trim().toUpperCase(), sub: t("as typed"), data: q.trim().toUpperCase() });
        return items;
      },
      selectedId: () => (this.tdsDuty ? "tds" : this.tcsDuty ? "tcs" : this.taxComponent.toUpperCase()),
      onPick: (it) => this.pickTax(it.data),
    });
    this.regionPicker = new ListPicker<Region | null>({
      input: this.inputs.region,
      host: this.el,
      title: "States",
      columns: ["Name", "Code"],
      source: (q) => {
        const s = q.trim().toLowerCase();
        const items: PickItem<Region | null>[] = [{ id: "", label: t("Not set"), sub: "", data: null }];
        for (const r of this.regions) items.push({ id: r.code, label: r.name, sub: r.code, data: r });
        return items.filter((i) => !s || i.label.toLowerCase().includes(s) || i.id.toLowerCase() === s);
      },
      selectedId: () => this.region?.code ?? "",
      onPick: (it) => {
        this.setRegion(it.data, false);
        this.goName("postal");
      },
    });
    this.countryPicker = new ListPicker<string | null>({
      input: this.inputs.country,
      host: this.el,
      title: "Countries",
      columns: ["Name", "Code"],
      source: (q) => {
        const s = q.trim().toLowerCase();
        const own = co ? t("Same as the company ({country})", { country: t(co.name) }) : t("Same as the company");
        const items: PickItem<string | null>[] = [{ id: "", label: own, sub: this.home ?? "", data: null }];
        // Shown in the display language; the English name stays an alias, so typing "germ" still finds it.
        for (const [code, name] of COUNTRIES) if (code !== this.home) items.push({ id: code, label: t(name), aliases: [name], sub: code, data: code });
        if (this.country && !items.some((i) => i.id === this.country)) items.push({ id: this.country, label: this.country, sub: this.country, data: this.country });
        const shown = items.filter(
          (i) => !s || i.label.toLowerCase().includes(s) || i.id.toLowerCase() === s || Boolean(i.aliases?.some((a) => a.toLowerCase().includes(s))),
        );
        // Any other ISO code, typed as two letters.
        if (/^[a-z]{2}$/.test(s) && !shown.some((i) => i.id.toLowerCase() === s))
          shown.push({ id: s.toUpperCase(), label: s.toUpperCase(), sub: t("code as typed"), data: s.toUpperCase() });
        return shown;
      },
      selectedId: () => this.country ?? "",
      onPick: (it) => {
        this.setCountry(it.data);
        this.goName(this.abroad() ? "postal" : this.sezLabel ? "sez" : "region");
      },
    });
    this.deducteePicker = new ListPicker<DeducteeKind | null>({
      input: this.inputs.deductee,
      host: this.el,
      title: "Deductee type",
      source: (q) => {
        const s = q.trim().toLowerCase();
        const fromPan = this.panHolder();
        const items: PickItem<DeducteeKind | null>[] = [
          { id: "", label: t("Not set"), sub: fromPan ? t("from the PAN: {type}", { type: deducteeLabel(fromPan) }) : "", data: null },
          ...DEDUCTEE_KEYS.map((k) => ({ id: k, label: deducteeLabel(k), aliases: [DEDUCTEE_LABEL[k]], data: k })),
        ];
        return items.filter((i) => !s || i.label.toLowerCase().includes(s) || i.aliases?.some((a) => a.toLowerCase().includes(s)));
      },
      selectedId: () => this.deductee ?? "",
      onPick: (it) => {
        this.setDeductee(it.data);
        this.goName("lowerRate");
      },
    });
    this.naturePicker = new ListPicker<string | null>({
      input: this.inputs.tdsNature,
      host: this.el,
      title: "Nature of payment",
      columns: ["Name", "Section"],
      source: (q) => this.natureItems(q, false),
      selectedId: () => this.tdsNature ?? "",
      onPick: (it) => {
        this.setNature(it.data);
        this.goName("amount");
      },
    });
    this.payablePicker = new ListPicker<string | null>({
      input: this.inputs.tdsPayable,
      host: this.el,
      title: "TDS payable for",
      columns: ["Name", "Section"],
      source: (q) => this.natureItems(q, true),
      selectedId: () => this.payableFor ?? "",
      onPick: (it) => {
        if (!it.data) {
          // Not a TDS ledger after all: back to the type of tax.
          this.tdsDuty = false;
          this.setPayable(null);
          this.setTax("");
          return this.goName("amount");
        }
        this.setPayable(it.data);
        this.goName("amount");
      },
    });
    this.tcsBuyerPicker = new ListPicker<DeducteeKind | null>({
      input: this.inputs.tcsBuyer,
      host: this.el,
      title: "Buyer type",
      source: (q) => {
        const s = q.trim().toLowerCase();
        const from = this.deductee ?? this.panHolder();
        const items: PickItem<DeducteeKind | null>[] = [
          { id: "", label: t("Not set"), sub: from ? t("as for TDS: {type}", { type: deducteeLabel(from) }) : "", data: null },
          ...DEDUCTEE_KEYS.map((k) => ({ id: k, label: deducteeLabel(k), aliases: [DEDUCTEE_LABEL[k]], data: k })),
        ];
        return items.filter((i) => !s || i.label.toLowerCase().includes(s) || i.aliases?.some((a) => a.toLowerCase().includes(s)));
      },
      selectedId: () => this.tcsBuyer ?? "",
      onPick: (it) => {
        this.setTcsBuyer(it.data);
        this.goName("tcsDecl");
      },
    });
    this.tcsNaturePicker = new ListPicker<string | null>({
      input: this.inputs.tcsNature,
      host: this.el,
      title: "Nature of collection",
      columns: ["Name", "Section"],
      source: (q) => this.tcsNatureItems(q, false),
      selectedId: () => this.tcsNature ?? "",
      onPick: (it) => {
        this.setTcsNature(it.data);
        this.goName("amount");
      },
    });
    this.tcsPayablePicker = new ListPicker<string | null>({
      input: this.inputs.tcsPayable,
      host: this.el,
      title: "TCS payable for",
      columns: ["Name", "Section"],
      source: (q) => this.tcsNatureItems(q, true),
      selectedId: () => this.tcsPayableFor ?? "",
      onPick: (it) => {
        if (!it.data) {
          // Not a TCS ledger after all: back to the type of tax.
          this.tcsDuty = false;
          this.setTcsPayable(null);
          this.setTax("");
          return this.goName("amount");
        }
        this.setTcsPayable(it.data);
        this.goName("amount");
      },
    });
    const codeItems = (list: CodeName[], q: string, none: string) => {
      const s = q.trim().toLowerCase();
      const items: PickItem<string | null>[] = [
        { id: "", label: t("Not set"), sub: none, data: null },
        ...list.map((c) => ({ id: c.code, label: c.name, sub: c.code, data: c.code })),
      ];
      return items.filter((i) => !s || i.label.toLowerCase().includes(s) || i.id === s || (i.id !== "" && i.id === s.padStart(2, "0")));
    };
    this.nrCountryPicker = new ListPicker<string | null>({
      input: this.inputs.nrCountry,
      host: this.el,
      title: "Country of residence",
      columns: ["Name", "Code"],
      source: (q) => codeItems(this.nrCountries, q, ""),
      selectedId: () => this.nrCountry ?? "",
      onPick: (it) => {
        this.setNrCountry(it.data);
        this.goName("nrRemit");
      },
    });
    this.nrRemitPicker = new ListPicker<string | null>({
      input: this.inputs.nrRemit,
      host: this.el,
      title: "Nature of remittance",
      columns: ["Name", "Code"],
      source: (q) => codeItems(this.nrRemittances, q, ""),
      selectedId: () => this.nrRemit ?? "",
      onPick: (it) => {
        this.setNrRemit(it.data);
        this.goName("nrTin");
      },
    });
    this.paintRegion();
    this.paintCountry();
    this.paintPan();
    this.paintCert();
    this.setDeductee(null);
    this.setNature(null);
    this.setPayable(null);
    this.setTcsBuyer(null);
    this.setTcsNature(null);
    this.setTcsPayable(null);
    this.paintTcsCert();
    this.paintNr();
  }

  async init(): Promise<void> {
    const groups = await api.searchGroups("");
    this.groupsById = new Map(groups.map((g) => [g.id, g]));
    // By id, as the core checks it; by name only when hashing isn't available.
    const id = await defaultGroupId("Duties & Taxes");
    this.dutiesId =
      (id && this.groupsById.has(id) ? id : null) ??
      groups.find((g) => g.name === "Duties & Taxes" && g.primary_name !== g.name)?.id ??
      null;
    const findAll = async (names: string[], into: Set<string>) => {
      for (const name of names) {
        const gid = await defaultGroupId(name);
        const found = (gid && this.groupsById.has(gid) ? gid : null) ?? groups.find((g) => g.name === name && g.parent === null)?.id;
        if (found) into.add(found);
      }
    };
    await findAll(CLASSED_GROUPS, this.classedIds);
    if (this.india) {
      await findAll(TDS_EXPENSE_GROUPS, this.tdsExpenseIds);
      await findAll(TCS_SALES_GROUPS, this.tcsSalesIds);
      const [natures, tcsNatures, codes] = await Promise.all([
        tdsApi.natures().catch(() => []),
        tcsApi.natures().catch(() => []),
        tdsStatementApi.nrCodes().catch((): [CodeName[], CodeName[]] => [[], []]),
      ]);
      this.natures = natures;
      this.tcsNatures = tcsNatures;
      [this.nrCountries, this.nrRemittances] = codes;
    }
    this.rates = await api.listTaxRates().catch(() => []);
    // The panel's base totals leave out the ledger being altered.
    const ledgers = (await api.listLedgers("", "all")).filter((l) => l.id !== this.opts.alterId);
    this.baseDr = M.sum(ledgers.map((l) => M.of(l.opening)).filter((u) => u > 0n));
    this.baseCr = M.sum(ledgers.map((l) => -M.of(l.opening)).filter((u) => u > 0n));
    // No country yet: State is plain text.
    this.regions = (await api.companyTax().catch(() => null))?.regions ?? [];
    this.paintRegion();
    if (this.opts.alterId) await this.load(this.opts.alterId);
    this.paintPanel();
  }

  /** Alteration: fill the form from the saved ledger. */
  private async load(id: string): Promise<void> {
    const l = await api.getLedger(id);
    this.ledger = l;
    this.inputs.name.value = l.name;
    this.setGroup(await api.getGroup(l.group_id));
    this.lastGroup = this.group;
    this.inputs.currency.value = l.currency ?? "";
    this.paintCurrency();
    if (l.is_party) {
      this.inputs.credit.value = l.credit_days === null ? "" : String(l.credit_days);
      this.inputs.taxId.value = l.tax_id ?? "";
      try {
        await this.checkTaxId();
      } catch {
        /* checked again on save */
      }
      if (this.taxInvalid()) this.inputs.override.value = l.tax_id_override ?? "";
    }
    this.setMailing(l.mailing);
    this.inputs.taxClass.value = l.tax_class ?? "";
    this.paintClass();
    if (this.duty && l.tax_component) {
      this.taxSide = l.tax_side ?? "Output";
      this.sideTouched = true;
      this.setTax(l.tax_component);
    }
    if (this.india) this.loadTds(l);
    const u = M.of(l.opening);
    if (u !== 0n) {
      this.side = u > 0n ? "Dr" : "Cr";
      this.inputs.side.value = t(this.side);
      this.inputs.amount.value = M.format(M.abs(u));
    }
    this.loaded = this.snapshot();
  }

  /** The saved PAN and TDS settings into the form, and their summary line. */
  private loadTds(l: Ledger): void {
    this.inputs.pan.value = l.pan ?? "";
    const terms = l.tds?.kind === "Deductible" ? l.tds : null;
    this.setDeductee(terms?.deductee ?? null);
    this.inputs.lowerRate.value = terms?.lower_rate ? percentText(terms.lower_rate) : "";
    this.inputs.certNo.value = terms?.certificate ?? "";
    this.inputs.certFrom.value = terms?.valid_from ? D.editable(terms.valid_from) : "";
    this.inputs.certTo.value = terms?.valid_to ? D.editable(terms.valid_to) : "";
    this.setNature(terms?.nature ?? null);
    if (l.tds?.kind === "Payable" && this.duty) {
      this.tdsDuty = true;
      this.setPayable(l.tds.payable_for);
      this.setTax("");
    }
    // Non-resident terms.
    const nr = terms?.non_resident ?? null;
    this.inputs.nrRate.value = nr ? percentText(nr.rate) : "";
    this.setYn("nrTreaty", Boolean(nr?.treaty));
    this.setNrCountry(nr?.country ?? null);
    this.setNrRemit(nr?.remittance ?? null);
    this.inputs.nrTin.value = nr?.foreign_tin ?? "";
    // TCS: the buyer's terms, a nature of collection, or TCS payable.
    const tcs = l.tcs?.kind === "Collectible" ? l.tcs : null;
    this.setTcsBuyer(tcs?.collectee ?? null);
    this.setYn("tcsDecl", Boolean(tcs?.declaration));
    this.setYn("tcsExcluded", Boolean(tcs?.excluded));
    this.setYn("tcsPe", Boolean(tcs?.permanent_establishment));
    this.inputs.tcsRate.value = tcs?.lower_rate ? percentText(tcs.lower_rate) : "";
    this.inputs.tcsCertNo.value = tcs?.certificate ?? "";
    this.inputs.tcsFrom.value = tcs?.valid_from ? D.editable(tcs.valid_from) : "";
    this.inputs.tcsTo.value = tcs?.valid_to ? D.editable(tcs.valid_to) : "";
    this.setTcsNature(tcs?.nature ?? null);
    if (l.tcs?.kind === "Payable" && this.duty && !this.tdsDuty) {
      this.tcsDuty = true;
      this.setTcsPayable(l.tcs.payable_for);
      this.setTax("");
    }
    this.paintPan();
    this.paintCert();
    this.paintTcsCert();
    this.paintNr();
    // tds.ts is loaded when needed: it builds on the reports, which open vouchers (and so this form).
    void import("./tds").then(({ tdsSummary }) => {
      const text = tdsSummary(l, this.natures);
      const tcsText = this.tcsSummary(l.tcs ?? null);
      const lines = [text ? t("Saved TDS: {summary}", { summary: text }) : "", tcsText ? t("Saved TCS: {summary}", { summary: tcsText }) : ""].filter(Boolean);
      this.tdsSummaryEl.textContent = lines.join(" · ");
      this.tdsSummaryEl.hidden = !lines.length;
    });
  }

  /** One line about a ledger's TCS settings ("206C(1) Scrap · Company · declaration"). */
  private tcsSummary(tcs: LedgerTcs | null): string {
    if (!tcs) return "";
    if (tcs.kind === "Payable") return t("TCS payable: {nature}", { nature: tcsNatureLabel(this.tcsNatures, tcs.payable_for) });
    const parts: string[] = [];
    if (tcs.nature) parts.push(tcsNatureLabel(this.tcsNatures, tcs.nature));
    if (tcs.collectee) parts.push(deducteeLabel(tcs.collectee));
    if (tcs.declaration) parts.push(t("buyer's declaration"));
    if (tcs.excluded) parts.push(t("not a buyer under the Act"));
    if (tcs.non_resident) parts.push(t(tcs.permanent_establishment ? "non-resident with a permanent establishment" : "non-resident"));
    if (tcs.lower_rate != null) parts.push(t("lower rate {rate}%", { rate: percentText(tcs.lower_rate) }) + (tcs.certificate ? ` (${tcs.certificate})` : ""));
    return parts.join(" · ");
  }

  private snapshot(): string {
    const v = (f: LineField) => this.inputs[f].value.trim();
    const tax = this.duty && this.taxComponent ? `${this.taxComponent.toUpperCase()}/${this.taxSide}` : "";
    return JSON.stringify([
      v("name"),
      this.group?.id ?? null,
      v("currency").toUpperCase(),
      v("credit"),
      v("taxId"),
      v("override"),
      this.group?.is_party ? this.mailing() : null,
      tax,
      this.classed ? normaliseClass(this.inputs.taxClass.value) : "",
      M.toDecimal(this.signedAmount() ?? M.ZERO),
      this.india ? [v("pan").toUpperCase(), this.deductee, v("lowerRate"), v("certNo"), v("certFrom"), v("certTo"), this.tdsNature, this.tdsDuty, this.payableFor] : null,
      this.india
        ? [
            this.tcsBuyer,
            this.yn,
            v("tcsRate"),
            v("tcsCertNo"),
            v("tcsFrom"),
            v("tcsTo"),
            v("nrRate"),
            this.nrCountry,
            this.nrRemit,
            v("nrTin"),
            this.tcsNature,
            this.tcsDuty,
            this.tcsPayableFor,
          ]
        : null,
    ]);
  }

  /** The input or textarea of a field. */
  private input(f: Field): HTMLInputElement | HTMLTextAreaElement {
    return f === "address" ? this.address : this.inputs[f];
  }

  /** The mailing details as typed; all empty clears them. */
  private mailing(): Mailing {
    const m: Mailing = {};
    const address = this.address.value
      .split("\n")
      .map((s) => s.trim())
      .join("\n")
      .replace(/^\n+|\n+$/g, "");
    if (address) m.address = address;
    // A party abroad has no state of the company's country, and no SEZ.
    const region = this.abroad() ? "" : this.regions.length ? this.region?.code ?? "" : this.inputs.region.value.trim();
    if (region) m.region = region;
    for (const [f, k] of [["postal", "postal_code"], ["phone", "phone"], ["email", "email"]] as const) {
      const v = this.inputs[f].value.trim();
      if (v) m[k] = v;
    }
    if (this.abroad()) m.country = this.country;
    else if (this.sez && this.sezLabel) m.special_zone = true;
    return m;
  }

  /** The party is in another country than the company. */
  private abroad(): boolean {
    return this.country !== null && this.country !== this.home;
  }

  private setCountry(code: string | null): void {
    const c = code?.trim().toUpperCase() || null;
    this.country = c === this.home ? null : c;
    this.paintCountry();
  }

  private paintCountry(): void {
    if (!this.countryPicker?.isOpen) this.inputs.country.value = this.country ? countryName(this.country) : "";
    this.countryLine.textContent =
      this.country && countryName(this.country) !== this.country
        ? t("Code {code}. A party abroad: sales without tax count as exports.", { code: this.country })
        : t("A party abroad: sales without tax count as exports.");
    // State and SEZ apply only within the company's own country.
    this.regionRow.hidden = this.abroad();
    this.sezRow.hidden = this.abroad() || !this.sezLabel;
    // A party abroad gets the non-resident terms (India).
    if (this.nrRow) this.paintNr();
  }

  private setSez(on: boolean): void {
    this.sez = on;
    this.inputs.sez.value = t(on ? "Yes" : "No");
  }

  /** The rate in force today for the HSN/SAC typed, from the rate list. */
  private paintClass(): void {
    const v = this.inputs.taxClass.value;
    const r = rateFor(this.rates, v, this.shell.company?.today ?? D.todayIso());
    this.classLine.textContent = !normaliseClass(v)
      ? t("Invoices booked to this ledger get tax by this code.")
      : r
        ? r.class !== normaliseClass(v)
          ? t("({percent}% from {date}, as {code})", { percent: percentText(r.percent), date: D.show(r.from), code: r.class })
          : t("({percent}% from {date})", { percent: percentText(r.percent), date: D.show(r.from) })
        : t("(no rate in the tax rate list for this code yet)");
  }

  private setMailing(m: Mailing | null): void {
    this.address.value = m?.address ?? "";
    this.inputs.postal.value = m?.postal_code ?? "";
    this.inputs.phone.value = m?.phone ?? "";
    this.inputs.email.value = m?.email ?? "";
    this.setCountry(m?.country ?? null);
    this.setSez(Boolean(m?.special_zone));
    const code = m?.region ?? "";
    if (this.regions.length) this.setRegion(code ? this.regions.find((r) => r.code === code) ?? { code, name: code } : null, false);
    else {
      this.inputs.region.value = code;
      this.regionFromTax = false;
    }
  }

  private setRegion(r: Region | null, fromTax: boolean): void {
    this.region = r;
    this.regionFromTax = fromTax && r !== null;
    this.paintRegion();
  }

  private paintRegion(): void {
    if (this.regions.length && !this.regionPicker.isOpen) this.inputs.region.value = this.region?.name ?? "";
    this.inputs.region.placeholder = this.regions.length ? "" : t("State or region");
    this.regionLine.textContent = t("Used as the place of supply when the party has no {label}", { label: this.taxLabel ?? t("tax ID") });
  }

  /** A valid tax ID names its state (a GSTIN's first two digits): fill State from it unless one was chosen. */
  private regionFromCheck(c: TaxIdCheck): void {
    if (c.error || !c.region) return;
    const name = c.region.toLowerCase();
    if (this.regions.length) {
      if (this.region && !this.regionFromTax) return;
      const r = this.regions.find((x) => x.name.toLowerCase() === name) ?? this.regions.find((x) => x.code === c.value.slice(0, 2));
      if (r) this.setRegion(r, true);
    } else if (!this.inputs.region.value.trim() || this.regionFromTax) {
      this.inputs.region.value = c.region;
      this.regionFromTax = true;
    }
  }

  /** E-mail: blank, or something@somewhere. */
  private emailProblem(): string | null {
    const v = this.inputs.email.value.trim();
    return !v || /^[^\s@]+@[^\s@]+$/.test(v) ? null : t('"{text}" is not an e-mail address.', { text: v });
  }

  // ------------------------------------------------------------- TDS

  /** The PAN as typed, upper-cased; "" when blank. */
  private pan(): string {
    return this.inputs.pan.value.trim().toUpperCase();
  }

  /** The PAN inside the party's GSTIN (India), when it has one. */
  private gstinPan(): string | null {
    return this.india ? panOfGstin(this.inputs.taxId.value) : null;
  }

  /** The holder type of the PAN on record (typed, else the GSTIN's). */
  private panHolder(): DeducteeKind | null {
    const p = this.pan() || this.gstinPan() || "";
    return panShape(p) ? PAN_HOLDER[p[3]] : null;
  }

  /** Why the PAN can't be saved; null when fine (blank is fine). */
  private panProblem(): string | null {
    const p = this.pan();
    if (!p) return null;
    if (!panShape(p)) return t('"{pan}" is not a PAN: five letters, four digits and a letter, such as ABCDE1234F.', { pan: p });
    const inside = this.taxInvalid() ? null : this.gstinPan();
    if (inside && inside !== p) return t("The PAN {pan} is not the one in the {label} ({inside}).", { pan: p, label: this.taxLabel ?? "GSTIN", inside });
    return null;
  }

  private paintPan(): void {
    const inside = this.gstinPan();
    this.inputs.pan.placeholder = inside ?? t("e.g. ABCDE1234F");
    const p = this.pan();
    this.panLine.classList.toggle("bad", Boolean(this.panProblem()));
    this.panLine.textContent =
      this.panProblem() ??
      (p
        ? panShape(p)
          ? t("Holder type: {type}", { type: deducteeLabel(PAN_HOLDER[p[3]]) })
          : ""
        : inside
          ? t("Blank: the PAN inside the {label} ({pan}) is used.", { label: this.taxLabel ?? "GSTIN", pan: inside })
          : t("Without a PAN, TDS is deducted at 20% or more."));
    this.paintDeductee();
  }

  private setDeductee(k: DeducteeKind | null): void {
    this.deductee = k;
    this.paintDeductee();
  }

  private paintDeductee(): void {
    if (!this.deducteePicker?.isOpen) this.inputs.deductee.value = this.deductee ? deducteeLabel(this.deductee) : t("Not set");
    const fromPan = this.panHolder();
    this.deducteeLine.textContent =
      this.deductee && fromPan && fromPan !== this.deductee
        ? t("The PAN says {type}; this type is used.", { type: deducteeLabel(fromPan) })
        : !this.deductee && fromPan
          ? t("Not set: taken from the PAN ({type}).", { type: deducteeLabel(fromPan) })
          : t("Rates can differ by deductee type (194C: 1% individual or HUF, 2% others).");
  }

  /** The certificate's number and dates show once a lower rate is typed. */
  private paintCert(): void {
    const on = this.inputs.lowerRate.value.trim() !== "";
    for (const r of this.certRows) r.hidden = !on;
  }

  /** The lower rate typed: null when blank, undefined when not a percent. */
  private lowerRate(): M.Units | null | undefined {
    const v = this.inputs.lowerRate.value.trim().replace(/%$/, "");
    if (!v) return null;
    const u = M.parse(v);
    return u !== null && u >= 0n && u <= M.of("100") ? u : undefined;
  }

  private certDate(f: "certFrom" | "certTo"): string | null | undefined {
    const v = this.inputs[f].value.trim();
    if (!v) return null;
    return D.parseUser(v, this.shell.company?.today ?? D.todayIso()) ?? undefined;
  }

  /** Why the certificate can't be saved; null when fine. */
  private certProblem(): { field: Field; text: string } | null {
    const rate = this.lowerRate();
    if (rate === undefined) return { field: "lowerRate", text: t('"{text}" is not a rate: give a percent from 0 to 100.', { text: this.inputs.lowerRate.value.trim() }) };
    if (rate === null) return null;
    for (const f of ["certFrom", "certTo"] as const) {
      if (this.certDate(f) === undefined) return { field: f, text: t('"{text}" is not a date.', { text: this.inputs[f].value.trim() }) };
    }
    const from = this.certDate("certFrom");
    const to = this.certDate("certTo");
    if (from && to && from > to) return { field: "certTo", text: t("The certificate's validity ends before it starts.") };
    if (!this.deductee && !this.panHolder() && !this.tdsNatureOfParty())
      return { field: "deductee", text: t("Pick the deductee type for the certificate, or enter the PAN.") };
    return null;
  }

  /** A party keeps the nature it already had (set earlier for parties without expense lines). */
  private tdsNatureOfParty(): string | null {
    const terms = this.ledger?.tds?.kind === "Deductible" ? this.ledger.tds : null;
    return terms?.nature ?? null;
  }

  /** Nature-of-payment rows: "None" (or "All natures" for a payable ledger), then each nature. */
  private natureItems(q: string, payable: boolean): PickItem<string | null>[] {
    const s = q.trim().toLowerCase();
    const items: PickItem<string | null>[] = [
      { id: "", label: t("None"), aliases: ["None"], sub: payable ? t("not a TDS ledger") : t("no TDS on this ledger"), data: null },
      ...(payable ? [{ id: TDS_ALL_NATURES, label: t("All natures"), aliases: ["All natures", "all"], sub: t("one ledger for all TDS"), data: TDS_ALL_NATURES }] : []),
      ...this.natures.map((n) => ({ id: n.code, label: n.name, aliases: [n.code], sub: n.old_section, data: n.code })),
    ];
    return items.filter((i) => !s || i.label.toLowerCase().includes(s) || i.sub?.toLowerCase().includes(s) || i.aliases?.some((a) => a.toLowerCase().includes(s)));
  }

  private setNature(code: string | null): void {
    this.tdsNature = code;
    if (!this.naturePicker?.isOpen) this.inputs.tdsNature.value = code ? natureLabel(this.natures, code) : t("None");
    const n = this.natures.find((x) => x.code === code);
    this.natureHint.textContent = n
      ? t("{rate}% ({others}% others) · {threshold}", { rate: percentText(n.rate_individual_huf), others: percentText(n.rate_others), threshold: n.threshold })
      : t("Vouchers booked to this ledger get TDS suggested for this nature.");
  }

  private setPayable(code: string | null): void {
    this.payableFor = code;
    if (!this.payablePicker?.isOpen) this.inputs.tdsPayable.value = code ? natureLabel(this.natures, code) : t("None");
    this.payableLine.textContent = t("TDS deducted in vouchers is credited here; challan payments debit it.");
    this.payableRow.hidden = !this.tdsDuty;
  }

  /** The TDS settings to save; undefined keeps what the ledger has. */
  private tdsInput(): LedgerTds | null | undefined {
    if (!this.india || !this.group) return undefined;
    const saved = this.ledger?.tds ?? null;
    if (this.duty) return this.tdsDuty && this.payableFor ? { kind: "Payable", payable_for: this.payableFor } : saved ? null : undefined;
    const kept: TdsTerms = saved?.kind === "Deductible" ? { ...saved } : {};
    delete (kept as { kind?: string }).kind;
    let terms: TdsTerms;
    if (this.group.is_party) {
      const rate = this.lowerRate();
      const on = rate !== null && rate !== undefined;
      const nr = this.nrInput();
      // A certificate (or non-resident terms) needs a deductee type: the PAN's, when none was picked.
      const deductee = this.deductee ?? (on || nr ? this.panHolder() : null);
      terms = {
        nature: kept.nature ?? null,
        deductee,
        lower_rate: on ? M.toDecimal(rate) : null,
        certificate: on ? this.inputs.certNo.value.trim() || null : null,
        valid_from: on ? this.certDate("certFrom") ?? null : null,
        valid_to: on ? this.certDate("certTo") ?? null : null,
        non_resident: nr,
      };
    } else if (this.tdsExpense) terms = { ...kept, nature: this.tdsNature };
    else return undefined;
    if (!terms.nature && !terms.deductee) return saved ? null : undefined;
    return { kind: "Deductible", ...terms };
  }

  /** The PAN to save ("" clears); undefined keeps the ledger's. */
  private panInput(): string | undefined {
    if (!this.india || !this.group?.is_party) return undefined;
    return this.pan();
  }

  // ------------------------------------------------------------- TCS

  private setYn(f: YnField, on: boolean): void {
    this.yn[f] = on;
    this.inputs[f].value = t(on ? "Yes" : "No");
  }

  private setTcsBuyer(k: DeducteeKind | null): void {
    this.tcsBuyer = k;
    this.paintTcsBuyer();
  }

  private paintTcsBuyer(): void {
    if (!this.tcsBuyerPicker?.isOpen) this.inputs.tcsBuyer.value = this.tcsBuyer ? deducteeLabel(this.tcsBuyer) : t("Not set");
    const from = this.deductee ?? this.panHolder();
    this.tcsBuyerLine.textContent =
      !this.tcsBuyer && from
        ? t("Not set: the TDS deductee type or the PAN's ({type}) is used.", { type: deducteeLabel(from) })
        : t("Without a PAN, TCS is collected at twice the rate or 5%, whichever is higher.");
  }

  /** The certificate's number and dates show once a lower collection rate is typed. */
  private paintTcsCert(): void {
    const on = this.inputs.tcsRate.value.trim() !== "";
    for (const r of this.tcsCertRows) r.hidden = !on;
  }

  /** A percent field: null when blank, undefined when not a percent from 0 to 100. */
  private percentOf(f: "tcsRate" | "nrRate"): M.Units | null | undefined {
    const v = this.inputs[f].value.trim().replace(/%$/, "");
    if (!v) return null;
    const u = M.parse(v);
    return u !== null && u >= 0n && u <= M.of("100") ? u : undefined;
  }

  private dateOf(f: "tcsFrom" | "tcsTo"): string | null | undefined {
    const v = this.inputs[f].value.trim();
    if (!v) return null;
    return D.parseUser(v, this.shell.company?.today ?? D.todayIso()) ?? undefined;
  }

  /** Why the TCS terms can't be saved; null when fine. */
  private tcsProblem(): { field: Field; text: string } | null {
    const rate = this.percentOf("tcsRate");
    if (rate === undefined) return { field: "tcsRate", text: t('"{text}" is not a rate: give a percent from 0 to 100.', { text: this.inputs.tcsRate.value.trim() }) };
    if (rate === null) return null;
    for (const f of ["tcsFrom", "tcsTo"] as const) {
      if (this.dateOf(f) === undefined) return { field: f, text: t('"{text}" is not a date.', { text: this.inputs[f].value.trim() }) };
    }
    const from = this.dateOf("tcsFrom");
    const to = this.dateOf("tcsTo");
    if (from && to && from > to) return { field: "tcsTo", text: t("The certificate's validity ends before it starts.") };
    return null;
  }

  /** Nature-of-collection rows: "None" (or "All natures" for a payable ledger), then each nature. */
  private tcsNatureItems(q: string, payable: boolean): PickItem<string | null>[] {
    const s = q.trim().toLowerCase();
    const items: PickItem<string | null>[] = [
      { id: "", label: t("None"), aliases: ["None"], sub: payable ? t("not a TCS ledger") : t("no TCS on this ledger"), data: null },
      ...(payable ? [{ id: TCS_ALL_NATURES, label: t("All natures"), aliases: ["All natures", "all"], sub: t("one ledger for all TCS"), data: TCS_ALL_NATURES }] : []),
      ...this.tcsNatures.map((n) => ({ id: n.code, label: n.name, aliases: [n.code, n.tax_code], sub: n.old_section, data: n.code })),
    ];
    return items.filter((i) => !s || i.label.toLowerCase().includes(s) || i.sub?.toLowerCase().includes(s) || i.aliases?.some((a) => a.toLowerCase().includes(s)));
  }

  private setTcsNature(code: string | null): void {
    this.tcsNature = code;
    if (!this.tcsNaturePicker?.isOpen) this.inputs.tcsNature.value = code ? tcsNatureLabel(this.tcsNatures, code) : t("None");
    const n = this.tcsNatures.find((x) => x.code === code);
    this.tcsNatureHint.textContent = n
      ? t("{rate}% · {threshold}", { rate: percentText(n.rate), threshold: n.threshold })
      : t("Sales booked to this ledger get TCS suggested for this nature.");
  }

  private setTcsPayable(code: string | null): void {
    this.tcsPayableFor = code;
    if (!this.tcsPayablePicker?.isOpen) this.inputs.tcsPayable.value = code ? tcsNatureLabel(this.tcsNatures, code) : t("None");
    this.tcsPayableRow.hidden = !this.tcsDuty;
  }

  // ------------------------------------------------------ non-residents

  /** The party's saved non-resident TDS terms, if any. */
  private savedNr(): NonResidentTerms | null {
    return this.ledger?.tds?.kind === "Deductible" ? this.ledger.tds.non_resident ?? null : null;
  }

  /**
   * The party is a non-resident for TCS: abroad, or saved as one when the
   * ledger had no country on record (so its flag isn't lost).
   */
  private tcsNonResident(): boolean {
    if (this.abroad()) return true;
    const saved = this.ledger?.tcs?.kind === "Collectible" && Boolean(this.ledger.tcs.non_resident);
    return saved && !this.ledger?.mailing?.country;
  }

  /** The non-resident terms show for a party abroad, or one that already has them. */
  private nrShown(): boolean {
    return this.india && Boolean(this.group?.is_party) && (this.abroad() || Boolean(this.savedNr()) || this.tcsNonResident());
  }

  private setNrCountry(code: string | null): void {
    this.nrCountry = code;
    if (!this.nrCountryPicker?.isOpen) this.inputs.nrCountry.value = code ? (this.nrCountries.find((c) => c.code === code)?.name ?? code) : t("Not set");
  }

  private setNrRemit(code: string | null): void {
    this.nrRemit = code;
    if (!this.nrRemitPicker?.isOpen) this.inputs.nrRemit.value = code ? `${code} ${this.nrRemittances.find((c) => c.code === code)?.name ?? ""}`.trim() : t("Not set");
  }

  /** The Form 144 code of the party's country, when the list has it. */
  private form144Country(): string | null {
    if (!this.country) return null;
    if (FORM144_COUNTRY[this.country]) return FORM144_COUNTRY[this.country];
    const name = (COUNTRIES.find(([c]) => c === this.country)?.[1] ?? "").toUpperCase();
    return this.nrCountries.find((c) => c.name === name)?.code ?? null;
  }

  private paintNr(): void {
    const shown = this.nrShown();
    this.nrRow.hidden = !shown;
    // The country follows the mailing country until one is chosen.
    if (shown && !this.nrCountry) this.setNrCountry(this.form144Country());
    this.nrLine.textContent = this.inputs.nrRate.value.trim()
      ? t("Include any surcharge and cess. File Form 145 (ex-15CA), and Form 146 (ex-15CB) where required, before remitting.")
      : t("Blank: no non-resident terms; TDS asks for a rate.");
  }

  /** Why the non-resident terms can't be saved; null when fine. */
  private nrProblem(): { field: Field; text: string } | null {
    if (!this.nrShown()) return null;
    const rate = this.percentOf("nrRate");
    if (rate === undefined) return { field: "nrRate", text: t('"{text}" is not a rate: give a percent from 0 to 100.', { text: this.inputs.nrRate.value.trim() }) };
    if (rate === null) return null;
    if (!this.deductee && !this.panHolder() && !this.tdsNatureOfParty())
      return { field: "deductee", text: t("Pick the deductee type for the non-resident terms, or enter the PAN.") };
    return null;
  }

  /** The non-resident terms to save: from the fields when shown, else as saved. */
  private nrInput(): NonResidentTerms | null {
    if (!this.nrShown()) return this.savedNr();
    const rate = this.percentOf("nrRate");
    if (rate === null || rate === undefined) return null;
    return {
      rate: M.toDecimal(rate),
      treaty: this.yn.nrTreaty,
      country: this.nrCountry,
      remittance: this.nrRemit,
      foreign_tin: this.inputs.nrTin.value.trim() || null,
    };
  }

  /** The TCS settings to save; undefined keeps what the ledger has (or there is nothing to change). */
  private tcsInput(): LedgerTcs | null | undefined {
    if (!this.india || !this.group) return undefined;
    const saved = this.ledger?.tcs ?? null;
    let want: LedgerTcs | null = null;
    if (this.duty) want = this.tcsDuty && this.tcsPayableFor ? { kind: "Payable", payable_for: this.tcsPayableFor } : null;
    else if (this.group.is_party) {
      const kept: TcsTerms = saved?.kind === "Collectible" ? saved : {};
      const rate = this.percentOf("tcsRate");
      const on = rate !== null && rate !== undefined;
      const nr = this.tcsNonResident();
      const terms: TcsTerms = {
        nature: kept.nature ?? null,
        collectee: this.tcsBuyer,
        declaration: this.yn.tcsDecl,
        excluded: this.yn.tcsExcluded,
        non_resident: nr,
        permanent_establishment: nr && this.yn.tcsPe,
        lower_rate: on ? M.toDecimal(rate) : null,
        certificate: on ? this.inputs.tcsCertNo.value.trim() || null : null,
        valid_from: on ? this.dateOf("tcsFrom") ?? null : null,
        valid_to: on ? this.dateOf("tcsTo") ?? null : null,
      };
      if (terms.nature || terms.collectee || terms.declaration || terms.excluded || terms.non_resident || on) want = { kind: "Collectible", ...terms };
    } else if (this.tcsSales) want = this.tcsNature ? { kind: "Collectible", nature: this.tcsNature } : null;
    else return undefined;
    return tcsKey(want) === tcsKey(saved) ? undefined : want;
  }

  // ------------------------------------------------------------ flow

  /** Fields in Enter order; party details only for party ledgers (Ma1, M3), tax only for duty ledgers. */
  private fields(): Field[] {
    if (this.duty) {
      const then: Field[] = this.taxComponent ? ["taxSide"] : this.tdsDuty ? ["tdsPayable"] : this.tcsDuty ? ["tcsPayable"] : [];
      return ["name", "under", "taxType", ...then, "amount", "side"];
    }
    const nature: Field[] = [...(this.tdsExpense ? (["tdsNature"] as const) : []), ...(this.tcsSales ? (["tcsNature"] as const) : [])];
    if (this.classed) return ["name", "under", "taxClass", ...nature, "amount", "side"];
    if (!this.group?.is_party) return ["name", "under", ...(this.takesCurrency() ? (["currency"] as const) : []), ...nature, "amount", "side"];
    const tax: Field[] = !this.taxLabel ? [] : this.taxInvalid() ? ["taxId", "override"] : ["taxId"];
    const tds: Field[] = !this.india ? [] : ["pan", "deductee", "lowerRate", ...(this.inputs.lowerRate.value.trim() ? (["certNo", "certFrom", "certTo"] as const) : [])];
    const tcs: Field[] = !this.india
      ? []
      : ["tcsBuyer", "tcsDecl", "tcsExcluded", "tcsRate", ...(this.inputs.tcsRate.value.trim() ? (["tcsCertNo", "tcsFrom", "tcsTo"] as const) : [])];
    const local: Field[] = this.abroad() ? [] : this.sezLabel ? ["sez", "region"] : ["region"];
    const nr: Field[] = this.nrShown() ? ["nrRate", "nrTreaty", "nrCountry", "nrRemit", "nrTin", "tcsPe"] : [];
    return ["name", "under", "currency", "credit", ...tax, ...tds, ...tcs, "address", "country", ...local, "postal", "phone", "email", ...nr, "amount", "side"];
  }

  /** After the tax ID: PAN (India), else the address. */
  private afterTaxId(): Field {
    return this.india ? "pan" : "address";
  }

  private taxInvalid(): boolean {
    return Boolean(this.taxCheck?.error);
  }

  private setTaxCheck(c: TaxIdCheck | null): void {
    this.taxCheck = c;
    this.overrideRow.hidden = !c?.error;
    if (!c?.error) this.inputs.override.value = "";
    this.taxLine.classList.toggle("bad", Boolean(c?.error));
    this.taxLine.textContent = !c ? "" : c.error ?? [c.region, c.entity_id && `PAN ${c.entity_id}`].filter(Boolean).join(" · ");
  }

  /** Check the tax ID field against the country pack (empty is fine). */
  private async checkTaxId(): Promise<TaxIdCheck | null> {
    const raw = this.inputs.taxId.value.trim();
    if (!this.taxLabel || !this.group?.is_party || !raw) {
      this.setTaxCheck(null);
      return null;
    }
    if (this.taxCheck) return this.taxCheck;
    const c = await api.checkTaxId(raw);
    this.inputs.taxId.value = c.value;
    this.setTaxCheck(c);
    this.regionFromCheck(c);
    this.paintPan();
    return c;
  }

  /** M2: an invalid ID blocks the form until it is fixed or a reason is given. */
  private taxBlocked(): string | null {
    if (!this.taxInvalid() || this.inputs.override.value.trim()) return null;
    return t("{error}. Correct it, or give a reason to keep it.", { error: this.taxCheck?.error ?? "" });
  }

  private goName(f: Field): void {
    this.go(this.fields().indexOf(f));
  }

  private afterUnder(): void {
    if (this.takesCurrency()) return this.goName("currency");
    this.goName(
      this.group?.is_party
        ? "credit"
        : this.duty
          ? "taxType"
          : this.classed
            ? "taxClass"
            : this.tdsExpense
              ? "tdsNature"
              : this.tcsSales
                ? "tcsNature"
                : "amount",
    );
  }

  /** Is the group Duties & Taxes, or under it? */
  private isDuty(g: Group | null): boolean {
    return this.dutiesId !== null && this.under(g, new Set([this.dutiesId]));
  }

  /** Is the group one of `ids`, or under one of them? */
  private under(g: Group | null, ids: Set<string>): boolean {
    if (!g) return false;
    let at: string | null = g.id;
    for (let n = 0; at && n < 64; n++) {
      if (ids.has(at)) return true;
      at = at === g.id ? g.parent : this.groupsById.get(at)?.parent ?? null;
    }
    return false;
  }

  /** What the Type of tax field shows: the tax, "TDS payable" or "None". */
  private taxTypeText(): string {
    return this.taxComponent ? componentLabel(this.taxComponent) : this.tdsDuty ? t("TDS payable") : this.tcsDuty ? t("TCS payable") : t("None");
  }

  private setTax(component: string): void {
    this.taxComponent = component.trim();
    if (this.taxComponent) {
      this.tdsDuty = false;
      this.tcsDuty = false;
    }
    this.inputs.taxType.value = this.taxTypeText();
    this.taxTypeLine.textContent = this.taxComponent
      ? t("(invoices put {tax} on this ledger)", { tax: componentLabel(this.taxComponent) })
      : this.tdsDuty
        ? t("(holds TDS deducted, not a tax on invoices)")
        : this.tcsDuty
          ? t("(holds TCS collected, not a tax on invoices)")
          : t("(not used for tax on invoices)");
    this.payableRow.hidden = !this.tdsDuty;
    this.tcsPayableRow.hidden = !this.tcsDuty;
    this.sideRow.hidden = this.tdsDuty || this.tcsDuty;
    this.setSide(this.taxSide);
  }

  private setSide(side: TaxSide): void {
    this.taxSide = side;
    this.inputs.taxSide.value = t(SIDE_LABEL[side]);
  }

  /** A choice in the Type of tax list. */
  private pickTax(choice: string): void {
    if (choice === "\u0000other") {
      this.inputs.taxType.value = "";
      this.inputs.taxType.dispatchEvent(new Event("input"));
      return this.shell.info(t("Type the tax's name, then press Enter."));
    }
    if (choice === TDS_CHOICE) {
      this.tdsDuty = true;
      this.tcsDuty = false;
      this.setTax("");
      this.setPayable(this.payableFor);
      return this.goName("tdsPayable");
    }
    if (choice === TCS_CHOICE) {
      this.tcsDuty = true;
      this.tdsDuty = false;
      this.setTax("");
      this.setTcsPayable(this.tcsPayableFor);
      return this.goName("tcsPayable");
    }
    if (choice.trim().length > MAX_COMPONENT) return this.shell.warn(t("A tax's name can be at most {n} characters.", { n: MAX_COMPONENT }));
    this.tdsDuty = false;
    this.tcsDuty = false;
    this.setTax(choice);
    if (!this.taxComponent) return this.goName("amount");
    // A ledger named "Input CGST" is for purchases, unless chosen otherwise.
    if (!this.sideTouched) this.setSide(/\b(input|itc)\b/i.test(this.inputs.name.value) ? "Input" : "Output");
    this.goName("taxSide");
  }

  /** List pickers by field, each with what to put back in its field when it closes. */
  private lists(): [Field, { isOpen: boolean; open(): Promise<void>; close(): void; key(e: KeyEv): boolean; action(a: string): boolean }, () => void][] {
    return [
      ["taxType", this.taxPicker, () => (this.inputs.taxType.value = this.taxTypeText())],
      ["country", this.countryPicker, () => this.paintCountry()],
      ["deductee", this.deducteePicker, () => this.paintDeductee()],
      ["tdsNature", this.naturePicker, () => this.setNature(this.tdsNature)],
      ["tdsPayable", this.payablePicker, () => this.setPayable(this.payableFor)],
      ["tcsBuyer", this.tcsBuyerPicker, () => this.paintTcsBuyer()],
      ["tcsNature", this.tcsNaturePicker, () => this.setTcsNature(this.tcsNature)],
      ["tcsPayable", this.tcsPayablePicker, () => this.setTcsPayable(this.tcsPayableFor)],
      ["nrCountry", this.nrCountryPicker, () => this.setNrCountry(this.nrCountry)],
      ["nrRemit", this.nrRemitPicker, () => this.setNrRemit(this.nrRemit)],
    ];
  }

  contexts(): Context[] {
    const f = this.fields()[this.at];
    if (f === "under" || this.lists().some(([k]) => k === f) || (f === "region" && this.regions.length)) return ["ListField", "Master"];
    if (f === "amount") return ["AmountField", "Master"];
    return ["Master"];
  }

  handles(a: string): boolean {
    return a === "Accept" || a === "Back" || (a === "DeleteRecord" && Boolean(this.ledger));
  }

  bottom() {
    const ctx = this.contexts();
    return [
      this.shell.btn("Accept", ctx, "Accept"),
      this.ledger ? this.shell.btn("DeleteRecord", ctx, "Delete") : null,
      this.shell.btn("Back", ctx, this.overlay ? "Cancel" : "Back"),
      this.fields()[this.at] === "under" ? this.shell.btn("CreateMasterInline", ctx, "Create Group") : null,
    ].filter((b): b is NonNullable<typeof b> => b !== null);
  }

  focus(): void {
    this.go(this.at);
  }

  private focusField(i: number, select = true): void {
    if (i < 0) return;
    if (this.at === i && document.activeElement === this.input(this.fields()[i])) return;
    this.at = i;
    const f = this.fields()[i];
    if (f === "under") void this.picker.open();
    else this.picker.close();
    for (const [k, p, restore] of this.lists()) {
      if (f === k) void p.open();
      else if (p.isOpen) {
        p.close();
        restore();
      }
    }
    if (f === "region" && this.regions.length) void this.regionPicker.open();
    else if (this.regionPicker.isOpen) {
      this.regionPicker.close();
      this.paintRegion();
    }
    if (f === "amount") {
      const u = M.parse(this.inputs.amount.value);
      if (u !== null && u !== 0n) this.inputs.amount.value = M.toDecimal(u);
    }
    if (select) this.selectField(f);
    this.shell.refresh();
  }

  /** Select the field's text; a textarea puts the cursor at its end instead. */
  private selectField(f: Field): void {
    const inp = this.input(f);
    if (inp instanceof HTMLTextAreaElement) inp.setSelectionRange(inp.value.length, inp.value.length);
    else inp.select();
  }

  private go(i: number): void {
    const f = this.fields()[i];
    if (this.fields()[this.at] === "under" && f !== "under" && this.group) this.inputs.under.value = this.group.name;
    this.input(f).focus();
    this.focusField(i);
    this.selectField(f);
  }

  /** Alt+C in Under: create the group inline, then fill the field (P7). */
  private createGroupInline(text: string): void {
    void this.shell.push(
      new GroupForm(this.shell, {
        name: text,
        inline: true,
        parentFilter: this.opts.groupFilter,
        onCreated: (g) => {
          this.setGroup(g);
          this.afterUnder();
        },
      }),
    );
  }

  private setGroup(g: Group): void {
    this.group = g;
    this.inputs.under.value = g.name;
    this.natureEl.textContent = natureLine(g);
    this.inputs.side.value = t(this.side ?? defaultSide(g));
    this.partyRow.hidden = !g.is_party;
    this.currencyRow.hidden = !this.takesCurrency();
    if (!this.takesCurrency()) this.inputs.currency.value = "";
    this.mailingRow.hidden = !g.is_party;
    this.tdsRow.hidden = !(this.india && g.is_party);
    this.tcsRow.hidden = !(this.india && g.is_party);
    this.duty = this.isDuty(g);
    this.taxRow.hidden = !this.duty;
    this.classed = !this.duty && !g.is_party && this.under(g, this.classedIds);
    this.classRow.hidden = !this.classed;
    this.tdsExpense = this.india && !this.duty && !g.is_party && this.under(g, this.tdsExpenseIds);
    this.natureRow.hidden = !this.tdsExpense;
    this.tcsSales = this.india && !this.duty && !g.is_party && this.under(g, this.tcsSalesIds);
    this.tcsNatureRow.hidden = !this.tcsSales;
    this.paintNr();
    this.paintClass();
    if (!this.duty) {
      this.tdsDuty = false;
      this.tcsDuty = false;
    }
    if (!this.duty) this.setTax("");
    else if (!this.taxComponent) this.setTax("");
    if (!g.is_party) {
      this.inputs.credit.value = "";
      this.inputs.taxId.value = "";
      this.setTaxCheck(null);
    }
  }

  /** Credit period: blank, or whole days up to ten years. */
  private creditDays(): number | null | undefined {
    const v = this.inputs.credit.value.trim();
    if (!v || !this.group?.is_party) return null;
    return /^\d{1,4}$/.test(v) && Number(v) <= 3650 ? Number(v) : undefined;
  }

  private signedAmount(): M.Units | null {
    const u = M.parse(this.inputs.amount.value);
    if (u === null) return null;
    const a = M.abs(u);
    return (this.side ?? defaultSide(this.group)) === "Dr" ? a : -a;
  }

  private formatAmount(): void {
    const u = M.parse(this.inputs.amount.value);
    if (u !== null) this.inputs.amount.value = u === 0n ? "" : M.format(M.abs(u));
  }

  private paintPanel(): void {
    const mine = this.signedAmount() ?? M.ZERO;
    const dr = this.baseDr + (mine > 0n ? mine : M.ZERO);
    const cr = this.baseCr + (mine < 0n ? -mine : M.ZERO);
    const diff = dr - cr;
    this.panel.replaceChildren(
      h("div.panel-title", t("Opening balances (company)")),
      h("div.prow", h("span", t("Total debit")), h("span.num", M.format(dr))),
      h("div.prow", h("span", t("Total credit")), h("span.num", M.format(cr))),
      h(
        "div.prow",
        { class: diff === 0n ? "ok" : "warn" },
        h("span", t(diff === 0n ? "Balanced" : "Difference")),
        h("span.num", M.drCr(diff)),
      ),
    );
  }

  private dirty(): boolean {
    if (this.ledger) return this.snapshot() !== this.loaded;
    return Boolean(this.inputs.name.value.trim() && this.inputs.name.value !== (this.opts.name ?? "")) ||
      (this.group !== null && this.group !== this.lastGroup) ||
      this.inputs.taxId.value.trim() !== "" ||
      (Boolean(this.group?.is_party) && Object.keys(this.mailing()).length > 0) ||
      this.taxComponent !== "" ||
      (this.classed && this.inputs.taxClass.value.trim() !== "") ||
      (this.india && (this.pan() !== "" || this.deductee !== null || this.inputs.lowerRate.value.trim() !== "" || this.tdsNature !== null || this.tdsDuty)) ||
      (this.india &&
        (this.tcsBuyer !== null ||
          Object.values(this.yn).some(Boolean) ||
          this.inputs.tcsRate.value.trim() !== "" ||
          this.inputs.nrRate.value.trim() !== "" ||
          this.tcsNature !== null ||
          this.tcsDuty)) ||
      (M.parse(this.inputs.amount.value) ?? 0n) !== 0n;
  }

  /** Dr / Cr on the opening balance, shown in the display language. */
  private setDrCr(side: Side): void {
    this.side = side;
    this.inputs.side.value = t(side);
    this.paintPanel();
  }

  key(e: KeyEv): boolean {
    const f = this.fields()[this.at];
    if (f === "under" && this.picker.key(e)) return true;
    for (const [k, p] of this.lists()) if (f === k && p.key(e)) return true;
    if (f === "region" && this.regions.length && this.regionPicker.key(e)) return true;
    if (f === "sez") {
      const k = letterOf(e);
      if (!e.ctrlKey && !e.altKey && (k === "y" || k === "n")) {
        this.setSez(k === "y");
        void this.next();
        return true;
      }
      if (e.chord === "Space") {
        this.setSez(!this.sez);
        return true;
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.altKey) return true;
    }
    if (isYn(f)) {
      const k = letterOf(e);
      if (!e.ctrlKey && !e.altKey && (k === "y" || k === "n")) {
        this.setYn(f, k === "y");
        void this.next();
        return true;
      }
      if (e.chord === "Space") {
        this.setYn(f, !this.yn[f]);
        return true;
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.altKey) return true;
    }
    if (f === "address") {
      // Up and Down move inside the text; Enter starts a new line.
      if (e.chord !== "Enter") return false;
      const a = this.address;
      const atEnd = a.selectionStart === a.value.length && a.selectionEnd === a.value.length;
      if (atEnd && (a.value === "" || a.value.endsWith("\n"))) {
        a.value = a.value.replace(/\s+$/, "");
        void this.next();
        return true;
      }
      a.setRangeText("\n", a.selectionStart, a.selectionEnd, "end");
      a.dispatchEvent(new Event("input"));
      return true;
    }
    if (f === "taxSide") {
      const k = letterOf(e);
      if (!e.ctrlKey && !e.altKey && "spoi".includes(k) && k.length === 1) {
        this.sideTouched = true;
        this.setSide(k === "s" || k === "o" ? "Output" : "Input");
        return true;
      }
      if (e.chord === "Up" || e.chord === "Down" || e.chord === "Space") {
        this.sideTouched = true;
        this.setSide(this.taxSide === "Output" ? "Input" : "Output");
        return true;
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.altKey) return true;
    }
    if (f === "side") {
      const k = letterOf(e);
      if (!e.ctrlKey && !e.altKey && (k === "d" || k === "c")) {
        this.setDrCr(k === "d" ? "Dr" : "Cr");
        return true;
      }
      if (e.chord === "Up" || e.chord === "Down" || e.chord === "Space") {
        this.setDrCr((this.side ?? defaultSide(this.group)) === "Dr" ? "Cr" : "Dr");
        return true;
      }
      if (e.key.length === 1 && !e.ctrlKey && !e.altKey) return true;
    }
    if (e.chord === "Up" && f !== "under") {
      if (this.at > 0) this.go(this.at - 1);
      return true; // Up on the first field does nothing (F3)
    }
    return false;
  }

  action(a: string): boolean {
    const f = this.fields()[this.at];
    if (f === "under" && this.picker.action(a)) return true;
    for (const [k, p] of this.lists()) if (f === k && p.action(a)) return true;
    if (f === "region" && this.regions.length && this.regionPicker.action(a)) return true;
    switch (a) {
      case "Accept":
        void this.save();
        return true;
      case "NextField":
        void this.next();
        return true;
      case "PreviousField":
        if (this.at > 0) this.go(this.at - 1);
        return true;
      case "DeleteRecord":
        if (!this.ledger) return false;
        void this.remove();
        return true;
      case "Back":
        void this.escape();
        return true;
    }
    return false;
  }

  private async next(): Promise<void> {
    const f = this.fields()[this.at];
    if (f === "name") {
      if (!this.inputs.name.value.trim()) return this.shell.warn(t("Enter a name for the ledger."));
      return this.goName("under");
    }
    if (f === "under") {
      if (!this.group) return this.shell.warn(t("Pick the group this ledger belongs under."));
      return this.afterUnder();
    }
    if (f === "taxType") {
      const v = this.inputs.taxType.value;
      return this.pickTax(v === t("None") ? "" : v === t("TDS payable") ? TDS_CHOICE : v === t("TCS payable") ? TCS_CHOICE : v);
    }
    if (f === "taxSide") return this.goName("amount");
    if (f === "taxClass") return this.goName(this.tdsExpense ? "tdsNature" : this.tcsSales ? "tcsNature" : "amount");
    if (f === "tdsNature" || f === "tdsPayable" || f === "tcsNature" || f === "tcsPayable") return this.goName("amount");
    if (f === "currency") {
      const bad = this.currencyProblem();
      if (bad) return this.shell.warn(bad);
      const order = this.fields();
      return this.goName(order[order.indexOf("currency") + 1]);
    }
    if (f === "credit") {
      if (this.creditDays() === undefined) return this.shell.warn(t("Enter the credit period as whole days, or leave it blank."));
      return this.goName(this.taxLabel ? "taxId" : this.afterTaxId());
    }
    if (f === "taxId") {
      try {
        await this.checkTaxId();
      } catch (e) {
        return this.shell.error(e);
      }
      if (!this.taxInvalid()) return this.goName(this.afterTaxId());
      const blocked = this.taxBlocked();
      if (blocked) this.shell.error(blocked);
      return this.goName("override");
    }
    if (f === "override") {
      const blocked = this.taxBlocked();
      if (blocked) {
        this.shell.error(blocked);
        return this.goName("taxId");
      }
      return this.goName(this.afterTaxId());
    }
    if (f === "pan") {
      this.inputs.pan.value = this.pan();
      const bad = this.panProblem();
      if (bad) return this.shell.warn(bad);
      this.paintPan();
      return this.goName("deductee");
    }
    if (f === "deductee") return this.goName("lowerRate");
    if (f === "lowerRate") {
      const rate = this.lowerRate();
      if (rate === undefined) return this.shell.warn(t('"{text}" is not a rate: give a percent from 0 to 100.', { text: this.inputs.lowerRate.value.trim() }));
      this.inputs.lowerRate.value = rate === null ? "" : M.toDecimal(rate);
      this.paintCert();
      return this.goName(rate === null ? "tcsBuyer" : "certNo");
    }
    if (f === "certNo") return this.goName("certFrom");
    if (f === "certFrom" || f === "certTo") {
      const d = this.certDate(f);
      if (d === undefined) return this.shell.warn(t('"{text}" is not a date.', { text: this.inputs[f].value.trim() }));
      if (d) this.inputs[f].value = D.editable(d);
      if (f === "certFrom") return this.goName("certTo");
      const bad = this.certProblem();
      if (bad) return this.shell.warn(bad.text);
      return this.goName("tcsBuyer");
    }
    if (f === "tcsBuyer") return this.goName("tcsDecl");
    if (f === "tcsDecl") return this.goName("tcsExcluded");
    if (f === "tcsExcluded") return this.goName("tcsRate");
    if (f === "tcsRate") {
      const rate = this.percentOf("tcsRate");
      if (rate === undefined) return this.shell.warn(t('"{text}" is not a rate: give a percent from 0 to 100.', { text: this.inputs.tcsRate.value.trim() }));
      this.inputs.tcsRate.value = rate === null ? "" : M.toDecimal(rate);
      this.paintTcsCert();
      return this.goName(rate === null ? "address" : "tcsCertNo");
    }
    if (f === "tcsCertNo") return this.goName("tcsFrom");
    if (f === "tcsFrom" || f === "tcsTo") {
      const d = this.dateOf(f);
      if (d === undefined) return this.shell.warn(t('"{text}" is not a date.', { text: this.inputs[f].value.trim() }));
      if (d) this.inputs[f].value = D.editable(d);
      if (f === "tcsFrom") return this.goName("tcsTo");
      const bad = this.tcsProblem();
      if (bad) return this.shell.warn(bad.text);
      return this.goName("address");
    }
    if (f === "nrRate") {
      const rate = this.percentOf("nrRate");
      if (rate === undefined) return this.shell.warn(t('"{text}" is not a rate: give a percent from 0 to 100.', { text: this.inputs.nrRate.value.trim() }));
      this.inputs.nrRate.value = rate === null ? "" : M.toDecimal(rate);
      this.paintNr();
      return this.goName("nrTreaty");
    }
    if (f === "nrTreaty") return this.goName("nrCountry");
    if (f === "nrCountry") return this.goName("nrRemit");
    if (f === "nrRemit") return this.goName("nrTin");
    if (f === "nrTin") return this.goName("tcsPe");
    if (f === "tcsPe") return this.goName("amount");
    if (f === "address") return this.goName("country");
    if (f === "country") {
      // Enter on the field as typed: a two-letter code, a country's name, or blank.
      const v = this.inputs.country.value.trim();
      const hit = COUNTRIES.find(([c, n]) => c === v.toUpperCase() || n.toLowerCase() === v.toLowerCase());
      if (!v) this.setCountry(null);
      else if (hit) this.setCountry(hit[0]);
      else if (/^[A-Za-z]{2}$/.test(v)) this.setCountry(v);
      else if (v !== (this.country ? countryName(this.country) : "")) return this.shell.warn(t("Pick a country from the list, or type its two-letter code."));
      return this.goName(this.abroad() ? "postal" : this.sezLabel ? "sez" : "region");
    }
    if (f === "sez") return this.goName("region");
    if (f === "region") return this.goName("postal");
    if (f === "postal") return this.goName("phone");
    if (f === "phone") return this.goName("email");
    if (f === "email") {
      const bad = this.emailProblem();
      if (bad) return this.shell.warn(bad);
      return this.goName(this.nrShown() ? "nrRate" : "amount");
    }
    if (f === "amount") {
      const u = M.parse(this.inputs.amount.value);
      if (u === null) return this.shell.error(t('"{text}" is not an amount.', { text: this.inputs.amount.value }));
      this.formatAmount();
      // An empty opening balance passes straight through (F2).
      if (u === 0n) return this.confirm();
      return this.goName("side");
    }
    return this.confirm();
  }

  /** Enter past the last field: "Accept? Yes / No" (F5). */
  private async confirm(): Promise<void> {
    if (await this.shell.ask("Accept?")) await this.save();
    else this.goName("name");
  }

  private async escape(): Promise<void> {
    const f = this.fields()[this.at];
    const inp = this.input(f);
    const list =
      f === "side" || f === "under" || f === "taxSide" || f === "sez" || isYn(f) || this.lists().some(([k]) => k === f) || (f === "region" && this.regions.length > 0);
    if (!this.ledger && !list && inp.value !== "") {
      inp.value = ""; // F4: clear the field first
      inp.dispatchEvent(new Event("input"));
      this.paintPanel();
      return;
    }
    if (this.dirty() && !(await this.shell.ask("Quit without saving?"))) return;
    this.shell.pop();
  }

  private async save(): Promise<void> {
    const name = this.inputs.name.value.trim();
    if (!name) {
      this.goName("name");
      return this.shell.warn(t("Enter a name for the ledger."));
    }
    if (!this.group) {
      this.goName("under");
      return this.shell.warn(t("Pick the group this ledger belongs under."));
    }
    const amount = this.signedAmount();
    if (amount === null) {
      this.goName("amount");
      return this.shell.error(t('"{text}" is not an amount.', { text: this.inputs.amount.value }));
    }
    const badCurrency = this.currencyProblem();
    if (badCurrency) {
      this.goName("currency");
      return this.shell.warn(badCurrency);
    }
    const credit = this.creditDays();
    if (credit === undefined) {
      this.goName("credit");
      return this.shell.warn(t("Enter the credit period as whole days, or leave it blank."));
    }
    let taxId: string | null = null;
    try {
      taxId = (await this.checkTaxId())?.value ?? null;
    } catch (e) {
      return this.shell.error(e);
    }
    const blocked = this.taxBlocked();
    if (blocked) {
      this.goName("override");
      return this.shell.error(blocked);
    }
    const reason = this.taxInvalid() ? this.inputs.override.value.trim() : null;
    const badEmail = this.group.is_party ? this.emailProblem() : null;
    if (badEmail) {
      this.goName("email");
      return this.shell.warn(badEmail);
    }
    if (this.india && this.group.is_party) {
      const badPan = this.panProblem();
      if (badPan) {
        this.goName("pan");
        return this.shell.warn(badPan);
      }
      const badCert = this.certProblem() ?? this.tcsProblem() ?? this.nrProblem();
      if (badCert) {
        this.goName(badCert.field);
        return this.shell.warn(badCert.text);
      }
    }
    if (this.duty && this.tdsDuty && !this.payableFor) {
      this.goName("tdsPayable");
      return this.shell.warn(t("Pick the nature of payment this ledger holds TDS for, or All natures."));
    }
    if (this.duty && this.tcsDuty && !this.tcsPayableFor) {
      this.goName("tcsPayable");
      return this.shell.warn(t("Pick the nature of collection this ledger holds TCS for, or All natures."));
    }
    // Parties only (Ma1); other ledgers keep whatever they have.
    const mailing = this.group.is_party ? this.mailing() : undefined;
    // Sales and service ledgers; a ledger moved out of those groups loses its code.
    const taxClass = this.classed ? normaliseClass(this.inputs.taxClass.value) : this.ledger?.tax_class ? "" : undefined;
    if (this.duty && this.taxComponent.length > MAX_COMPONENT) {
      this.goName("taxType");
      return this.shell.warn(t("A tax's name can be at most {n} characters.", { n: MAX_COMPONENT }));
    }
    const pan = this.panInput();
    const tds = this.tdsInput();
    const withTds = { ...(pan !== undefined ? { pan } : {}), ...(tds !== undefined ? { tds } : {}) };
    const tcs = this.tcsInput();
    if (this.ledger) {
      return this.saveAlteration(this.ledger, tcs, {
        name,
        group_id: this.group.id,
        opening: M.toDecimal(amount),
        credit_days: credit,
        tax_id: taxId,
        tax_id_override: reason,
        // Group and tax type change in one step; "" clears the tax type.
        tax_component: this.duty ? this.taxComponent.toUpperCase() : "",
        tax_side: this.duty && this.taxComponent ? this.taxSide : null,
        mailing,
        tax_class: taxClass,
        ...withTds,
      });
    }
    try {
      const made = await api.createLedger(name, this.group.id, M.toDecimal(amount), credit, taxId, reason);
      let ledger = made.ledger;
      await this.shell.refreshCompany();
      this.shell.info(t('Ledger "{name}" created under {group}.', { name: ledger.name, group: ledger.group_name }));
      if (this.duty && this.taxComponent) {
        try {
          await api.setLedgerTax(ledger.id, this.taxComponent, this.taxSide);
          ledger = { ...ledger, tax_component: this.taxComponent.toUpperCase(), tax_side: this.taxSide };
        } catch (e) {
          const why = e instanceof Error ? e.message : String(e);
          this.shell.error(t("The ledger was saved, but not its type of tax: {why}. Alter the ledger to set it.", { why }));
        }
      }
      // Create takes no mailing details, HSN/SAC, PAN or TDS: a second step adds them.
      const withMailing = Boolean(mailing && Object.keys(mailing).length);
      const withPanTds = Boolean(pan) || Boolean(tds);
      if (withMailing || taxClass || withPanTds) {
        try {
          const saved = await api.alterLedger(ledger.id, {
            name: ledger.name,
            group_id: ledger.group_id,
            opening: ledger.opening,
            credit_days: ledger.credit_days,
            tax_id: ledger.tax_id,
            tax_id_override: ledger.tax_id_override,
            ...(withMailing ? { mailing } : {}),
            ...(taxClass ? { tax_class: taxClass } : {}),
            ...(pan ? { pan } : {}),
            ...(tds ? { tds } : {}),
          });
          ledger = saved.ledger;
        } catch (e) {
          const why = e instanceof Error ? e.message : String(e);
          this.shell.error(t("The ledger was saved, but not its other details: {why}. Alter the ledger to add them.", { why }));
        }
      }
      if (tcs) ledger = await this.saveTcs(ledger, tcs);
      ledger = await this.saveCurrency(ledger);
      if (this.opts.onCreated) {
        const cb = this.opts.onCreated;
        this.shell.pop();
        cb(ledger);
        return;
      }
      // F7: a new blank form, with the last-used group carried over.
      if (amount > 0n) this.baseDr += amount;
      else this.baseCr += -amount;
      this.lastGroup = this.group;
      this.inputs.name.value = "";
      this.inputs.amount.value = "";
      this.inputs.taxId.value = "";
      this.inputs.currency.value = "";
      this.paintCurrency();
      this.setTaxCheck(null);
      this.setMailing(null);
      this.inputs.taxClass.value = "";
      for (const f of ["pan", "lowerRate", "certNo", "certFrom", "certTo", "tcsRate", "tcsCertNo", "tcsFrom", "tcsTo", "nrRate", "nrTin"] as const) this.inputs[f].value = "";
      this.setDeductee(null);
      this.setNature(null);
      this.tdsDuty = false;
      this.setPayable(null);
      this.setTcsBuyer(null);
      for (const f of YN_FIELDS) this.setYn(f, false);
      this.setTcsNature(null);
      this.tcsDuty = false;
      this.setTcsPayable(null);
      this.setNrCountry(null);
      this.setNrRemit(null);
      this.paintPan();
      this.paintCert();
      this.paintTcsCert();
      this.paintNr();
      this.side = null;
      this.sideTouched = false;
      this.taxComponent = "";
      this.setGroup(this.group);
      this.paintPanel();
      this.goName("name");
    } catch (e) {
      this.shell.error(e);
    }
  }

  private async saveAlteration(l: Ledger, tcs: LedgerTcs | null | undefined, input: LedgerInput): Promise<void> {
    try {
      const saved = await api.alterLedger(l.id, input);
      // After the alteration: a TCS payable ledger must already be under Duties & Taxes.
      let ledger = tcs !== undefined ? await this.saveTcs(saved.ledger, tcs) : saved.ledger;
      ledger = await this.saveCurrency(ledger);
      await this.shell.refreshCompany();
      this.shell.info(t('Ledger "{name}" altered.', { name: ledger.name }));
      this.loaded = this.snapshot();
      this.shell.pop();
      this.opts.onCreated?.(ledger);
    } catch (e) {
      this.shell.error(e);
    }
  }

  /** Party, cash and bank ledgers can be kept in a foreign currency. */
  private takesCurrency(): boolean {
    return Boolean(this.group && (this.group.is_party || this.group.is_cash_or_bank));
  }

  /** The currency typed, upper-cased; "" for the company's own. */
  private currencyCode(): string {
    const c = this.inputs.currency.value.trim().toUpperCase();
    return c === this.baseCurrency ? "" : c;
  }

  private currencyProblem(): string | null {
    const c = this.currencyCode();
    if (!c || !this.takesCurrency() || this.currencyCodes.size === 0 || this.currencyCodes.has(c)) return null;
    return t("{code} is not in the currency master. Use an ISO code such as USD, EUR or AED.", { code: c });
  }

  private paintCurrency(): void {
    const c = this.currencyCode();
    this.currencyLine.textContent = c ? (this.currencyCodes.get(c) ?? "") : "";
  }

  /** Save the ledger's currency (a separate command) when it changed; a failure is reported, the ledger stays saved. */
  private async saveCurrency(l: Ledger): Promise<Ledger> {
    const want = this.takesCurrency() ? this.currencyCode() : "";
    if (want === (l.currency ?? "")) return l;
    try {
      await fxApi.setLedgerCurrency(l.id, want);
      return { ...l, currency: want || null };
    } catch (e) {
      const why = e instanceof Error ? e.message : String(e);
      this.shell.error(t("The ledger was saved, but not its currency: {why}. Alter the ledger to set it.", { why }));
      return l;
    }
  }

  /** Save the TCS settings (a separate command); a failure is reported, the ledger stays saved. */
  private async saveTcs(l: Ledger, tcs: LedgerTcs | null): Promise<Ledger> {
    try {
      const saved = await tcsApi.setLedgerTcs(l.id, tcs);
      return { ...l, tcs: saved };
    } catch (e) {
      const why = e instanceof Error ? e.message : String(e);
      this.shell.error(t("The ledger was saved, but not its TCS settings: {why}. Alter the ledger to set them.", { why }));
      return l;
    }
  }

  /** Alt+D in alteration: delete after a Y/N prompt (refused once vouchers use it). */
  private async remove(): Promise<void> {
    const l = this.ledger;
    if (!l || !(await this.shell.ask(t('Delete ledger "{name}"?', { name: l.name }), { yesFirst: false }))) return;
    try {
      await api.deleteLedger(l.id);
      await this.shell.refreshCompany();
      this.shell.info(t('Ledger "{name}" deleted.', { name: l.name }));
      this.shell.pop();
    } catch (e) {
      this.shell.error(e);
    }
  }

  dispose(): void {
    this.picker.close();
    this.regionPicker.close();
    for (const [, p] of this.lists()) p.close();
  }
}

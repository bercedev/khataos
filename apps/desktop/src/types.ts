// Shapes returned by the Rust command layer (src-tauri/src/api.rs).
// Money is always a decimal string; see money.ts.

export type Nature = "Assets" | "Liabilities" | "Income" | "Expenses";
export type Side = "Dr" | "Cr";
export type VoucherKind =
  | "Contra"
  | "Payment"
  | "Receipt"
  | "Journal"
  | "Sales"
  | "Purchase"
  | "CreditNote"
  | "DebitNote"
  | "StockJournal";
export type Context =
  | "Global"
  | "Gateway"
  | "Voucher"
  | "Master"
  | "Report"
  | "ListField"
  | "AmountField"
  | "Dialog";
export type LedgerFilter = "all" | "cash_bank" | "party" | "non_cash_bank";

export interface Binding {
  chord: string;
  context: Context;
  action: string;
}

export interface CompanyInfo {
  name: string;
  path: string;
  user: string;
  today: string;
  fy_start: string;
  /** The country pack in force; null until it is picked (rule C2). */
  country: CountryInfo | null;
  ledger_count: number;
  voucher_count: number;
  opening_difference: string;
  sync_status: string | null;
  /** Shared with this computer as Viewer: no changes, not even the country. */
  read_only: boolean;
}

export interface CountryInfo {
  code: string;
  name: string;
  dialing_code: string;
  currency: string;
  currency_symbol: string;
  /** What a party's tax registration is called ("GSTIN"). */
  tax_id_label: string;
  fy_start: string;
  fy_end: string;
}

export interface TaxIdCheck {
  value: string;
  /** Why it is invalid; null when valid. */
  error: string | null;
  region: string | null;
  entity_id: string | null;
}

export interface CompanyFile {
  path: string;
  name: string;
}

export interface Group {
  id: string;
  name: string;
  parent: string | null;
  parent_name: string | null;
  primary_name: string;
  nature: Nature;
  aliases: string[];
  is_cash_or_bank: boolean;
  is_party: boolean;
}

export interface Ledger {
  id: string;
  name: string;
  group_id: string;
  group_name: string;
  nature: Nature;
  is_cash_or_bank: boolean;
  /** Under Cash-in-Hand rather than a bank group. */
  is_cash: boolean;
  is_party: boolean;
  opening: string;
  balance: string;
  credit_days: number | null;
  tax_id: string | null;
  /** Why an invalid tax ID was kept (rule M2). */
  tax_id_override: string | null;
  /** Duty ledgers: the tax collected ("CGST") and on which side. */
  tax_component: string | null;
  tax_side: TaxSide | null;
  /** Postal and contact details (parties). */
  mailing: Mailing | null;
  /** HSN/SAC or tax category, for sales and service ledgers. */
  tax_class: string | null;
}

export type TaxSide = "Output" | "Input";

/** A party's postal and contact details. Empty fields may be omitted. */
export interface Mailing {
  address?: string;
  /** Region code from the pack (an Indian state code, e.g. "27"). */
  region?: string | null;
  postal_code?: string;
  /** ISO 3166-1 alpha-2, when abroad. */
  country?: string | null;
  phone?: string;
  email?: string;
  /** A unit or developer in a special economic / free zone (SEZ): supplies are zero-rated. */
  special_zone?: boolean;
}

export interface CreatedLedger {
  ledger: Ledger;
  opening_difference: string;
}

export type BillKind = "New" | "Against" | "Advance" | "OnAccount";

export interface BillInput {
  kind: BillKind;
  reference: string;
  amount: string;
  due?: string | null;
}

/** One stock item on a voucher line: qty × rate = amount (amount optional). */
export interface StockLineInput {
  item_id: string;
  qty: string;
  rate: string;
  amount?: string;
  /** Empty or absent: main location. */
  godown_id?: string;
  batch?: string;
}

export interface StockLineView {
  item_id: string;
  item_name: string;
  unit: string;
  qty: string;
  rate: string;
  amount: string;
  godown_id: string | null;
  godown_name: string | null;
  batch: string | null;
}

export interface EntryInput {
  ledger_id: string;
  side: Side;
  amount: string;
  bills?: BillInput[];
  /** Item invoice: under a debit stock comes in, under a credit it goes out. */
  items?: StockLineInput[];
}

/** One Stock Journal line: out of stock (consumed, moved from) or into it. */
export interface StockMoveInput extends StockLineInput {
  inward: boolean;
}

export interface StockMoveView extends StockLineView {
  inward: boolean;
}

export interface VoucherInput {
  kind: VoucherKind;
  date: string;
  number?: string | null;
  entries: EntryInput[];
  /** Stock Journal lines. */
  stock?: StockMoveInput[];
  narration: string;
}

export interface PostedVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
  next_number: string;
}

export interface VoucherView {
  id: string;
  kind: VoucherKind;
  number: string;
  date: string;
  narration: string;
  entries: {
    ledger_id: string;
    ledger_name: string;
    side: Side;
    amount: string;
    bills: BillInput[];
    items: StockLineView[];
    /** Bank reconciliation date (bank ledgers). */
    bank_date: string | null;
    /** Foreign amount and rate of exchange (multi-currency). */
    forex?: ForexView;
    /** The realised forex gain/loss line, worked out again on saving. */
    forex_gain?: boolean;
  }[];
  total: string;
  cancelled: boolean;
  /** Stock Journal lines. */
  stock: StockMoveView[];
}

export interface DayBookRow {
  date: string;
  voucher: string;
  kind: VoucherKind;
  number: string;
  particulars: string;
  debit: string;
  credit: string;
  cancelled?: boolean;
}

export interface StatementRow extends DayBookRow {
  balance: string;
}

export interface LedgerStatement {
  ledger: string;
  ledger_name: string;
  from: string;
  to: string;
  opening: string;
  rows: StatementRow[];
  closing: string;
}

export interface TrialBalance {
  as_at: string | null;
  rows: { ledger_id: string; name: string; group_name: string; debit: string; credit: string }[];
  total_debit: string;
  total_credit: string;
  opening_difference: string;
  /** Stock items' opening value: its own debit row when non-zero. */
  opening_stock: string;
}

export interface GroupTotal {
  group: string;
  name: string;
  amount: string;
  /** Part of the trading account (sales, purchases, direct income/expense). */
  affects_gross_profit: boolean;
}

export interface ProfitAndLoss {
  from: string;
  to: string;
  incomes: GroupTotal[];
  expenses: GroupTotal[];
  /** Stock at the start / end of the period (zero without inventory). */
  opening_stock: string;
  closing_stock: string;
  gross_profit: string;
  net_profit: string;
}

export interface BalanceSheet {
  as_at: string;
  liabilities: GroupTotal[];
  assets: GroupTotal[];
  /** Stock on hand at as_at (zero without inventory). */
  closing_stock: string;
  profit_and_loss: string;
  opening_difference: string;
  total_liabilities: string;
  total_assets: string;
}

export interface GroupSummary {
  group_name: string;
  nature: Nature;
  rows: { ledger_id: string; name: string; group_name: string; amount: string }[];
  total: string;
}

export const KIND_LABEL: Record<VoucherKind, string> = {
  Contra: "Contra",
  Payment: "Payment",
  Receipt: "Receipt",
  Journal: "Journal",
  Sales: "Sales",
  Purchase: "Purchase",
  CreditNote: "Credit Note",
  DebitNote: "Debit Note",
  StockJournal: "Stock Journal",
};

/** Accounting voucher types (the voucher screen); Stock Journal has its own screen. */
export const KINDS = (Object.keys(KIND_LABEL) as VoucherKind[]).filter((k) => k !== "StockJournal");

export interface OutstandingBill {
  ledger_id: string;
  party: string;
  reference: string;
  date: string | null;
  due: string | null;
  /** Positive = receivable (Dr), negative = payable (Cr). */
  amount: string;
  overdue_days: number;
}

export interface AgeingRow {
  ledger_id: string;
  party: string;
  buckets: [string, string, string, string];
  total: string;
}

export interface ImportSummary {
  files: string[];
  groups_created: number;
  groups_matched: number;
  ledgers_created: number;
  opening_balances: number;
  vouchers_posted: number;
  vouchers_numbered: number;
  units_created: number;
  stock_groups_created: number;
  stock_items_created: number;
  godowns_created: number;
  inventory_lines: number;
  skipped: { kind: string; name: string; reason: string }[];
}

export interface SyncSettings {
  url: string;
  company_id: string;
  has_token: boolean;
  last_status: string | null;
}

export interface SyncResult {
  status: "up_to_date" | "pushed" | "pulled" | "conflict";
  count: number;
  message: string;
}

export interface RebaseReport {
  archived: string;
  renumbered: { kind: VoucherKind; old: string; new: string }[];
  needs_attention: { seq: number; reason: string }[];
  pushed: number;
}

export interface ExportTable {
  title: string;
  subtitle: string;
  columns: string[];
  numeric: number[];
  rows: string[][];
}

export type ReportSpec =
  | { report: "trial_balance"; as_at: string }
  | { report: "day_book"; from: string; to: string }
  | { report: "ledger_statement"; ledger_id: string; from: string; to: string }
  | { report: "profit_and_loss"; from: string; to: string }
  | { report: "balance_sheet"; as_at: string }
  | ({ report: "table" } & ExportTable);

// ------------------------------------------------------- inventory masters

export interface Unit {
  id: string;
  symbol: string;
  name: string;
  /** Decimal places a quantity may have (0–4). */
  decimals: number;
}

export interface StockGroup {
  id: string;
  name: string;
  parent_id: string | null;
  /** Parent path ("Raw Material / Steel"); empty: Primary. */
  parent_name: string;
}

export interface StockItem {
  id: string;
  name: string;
  group_id: string | null;
  /** Group path; empty: Primary. */
  group_name: string;
  unit_id: string;
  unit: string;
  decimals: number;
  opening_qty: string;
  opening_value: string;
  opening_rate: string;
  closing_qty: string;
  closing_value: string;
  /** HSN/SAC code or tax category. */
  tax_class: string | null;
  costing: Costing;
}

/** Stock valuation method, per item. */
export type Costing = "AverageCost" | "Fifo" | "LastPurchase";

export interface StockItemInput {
  /** Empty: create. */
  id: string;
  name: string;
  /** Empty: Primary. */
  group_id: string;
  unit_id: string;
  opening_qty: string;
  opening_rate: string;
  /** Empty: qty × rate. */
  opening_value: string;
  /** HSN/SAC code or tax category; empty: none. */
  tax_class: string;
  /** Omitted: keep the item's current method (average cost when new). */
  costing?: Costing;
}

export interface LedgerInput {
  name: string;
  group_id: string;
  opening: string;
  credit_days: number | null;
  tax_id: string | null;
  tax_id_override: string | null;
  /** Duty ledgers: the tax collected; "" clears; omitted keeps the current one. */
  tax_component?: string | null;
  tax_side?: TaxSide | null;
  /** Omitted: keep; all fields empty: clear. */
  mailing?: Mailing | null;
  /** HSN/SAC for sales or service ledgers; omitted keeps, "" clears. */
  tax_class?: string | null;
}

// ------------------------------------------------------- inventory reports

export interface StockMovement {
  opening_qty: string;
  opening_value: string;
  inward_qty: string;
  inward_value: string;
  outward_qty: string;
  /** Sales value of outward lines. */
  outward_value: string;
  closing_qty: string;
  /** At weighted average cost. */
  closing_value: string;
}

export interface StockSummaryRow extends StockMovement {
  item: string;
  name: string;
  /** Stock group path; empty: Primary. */
  group: string;
  unit: string;
}

export interface StockSummary {
  from: string;
  to: string;
  rows: StockSummaryRow[];
  opening_value: string;
  inward_value: string;
  outward_value: string;
  closing_value: string;
}

export interface StockVoucherRow {
  date: string;
  voucher: string;
  kind: VoucherKind;
  number: string;
  particulars: string;
  inward_qty: string;
  inward_value: string;
  outward_qty: string;
  outward_value: string;
  closing_qty: string;
}

export interface StockItemVouchers {
  item_name: string;
  unit: string;
  summary: StockMovement;
  rows: StockVoucherRow[];
}

// ------------------------------------------------------- banking

export interface BankLine {
  date: string;
  voucher: string;
  kind: VoucherKind;
  number: string;
  particulars: string;
  deposit: string;
  withdrawal: string;
  bank_date: string | null;
}

export interface BankReconciliation {
  ledger: string;
  ledger_name: string;
  from: string;
  to: string;
  lines: BankLine[];
  /** Debit (money in the bank) positive. */
  balance_per_books: string;
  uncleared_deposits: string;
  uncleared_withdrawals: string;
  balance_per_bank: string;
}

// ------------------------------------------------------- analysis

export interface RatioAnalysis {
  from: string;
  to: string;
  working_capital: string;
  cash_in_hand: string;
  bank_accounts: string;
  bank_od: string;
  sundry_debtors: string;
  sundry_creditors: string;
  sales: string;
  purchases: string;
  stock_in_hand: string;
  net_profit: string;
  working_capital_turnover: string | null;
  inventory_turnover: string | null;
  current_ratio: string | null;
  quick_ratio: string | null;
  debt_equity: string | null;
  gross_profit_pct: string | null;
  net_profit_pct: string | null;
  operating_cost_pct: string | null;
  receivable_days: string | null;
  return_on_investment_pct: string | null;
  return_on_working_capital_pct: string | null;
}

export interface MonthPoint {
  /** First day of the month. */
  month: string;
  sales: string;
  purchases: string;
  inflow: string;
  outflow: string;
  cash_bank_balance: string;
}

export interface NamedAmount {
  ledger: string;
  name: string;
  amount: string;
}

export interface Dashboard {
  from: string;
  to: string;
  cash_in_hand: string;
  bank_balance: string;
  receivables: string;
  payables: string;
  sales: string;
  purchases: string;
  gross_profit: string;
  net_profit: string;
  closing_stock: string;
  months: MonthPoint[];
  top_debtors: NamedAmount[];
  top_creditors: NamedAmount[];
  expenses: { name: string; amount: string }[];
}

// ------------------------------------------------------- numbering, godowns, tax

export type Restart = "Never" | "Yearly" | "Monthly";

/** A voucher type's numbering series. Prefix/suffix tokens: {FY} {YYYY} {YY} {MM}. */
export interface Numbering {
  kind: VoucherKind;
  restart: Restart;
  prefix: string;
  suffix: string;
  /** Zero-pad to this many digits; 0: none. */
  width: number;
  /** Next number for a voucher dated today. */
  next_number: string;
  /** Vouchers of this type already saved. */
  used: number;
}

export interface Godown {
  id: string;
  name: string;
  parent_id: string | null;
  parent_name: string | null;
}

export interface LocationRow {
  /** null: main location. */
  godown: string | null;
  godown_name: string;
  batch: string | null;
  item: string;
  item_name: string;
  unit: string;
  qty: string;
  value: string;
}

export interface Region {
  code: string;
  name: string;
}

export interface CompanyTax {
  /** "GSTIN", "TRN"… */
  label: string;
  tax_id: string | null;
  region: string | null;
  region_name: string | null;
  regions: Region[];
  /** Mailing address printed on invoices (several lines). */
  address: string;
  /** HSN/SAC digits required on invoices (4, 6 or 8), when set. */
  hsn_digits: number | null;
}

export interface TaxRate {
  /** HSN/SAC code or category, normalised (no spaces, upper case). */
  class: string;
  from: string;
  percent: string;
}

export interface InvoiceTaxInput {
  kind: VoucherKind;
  date: string;
  party_id: string;
  /** Item lines (taxed by the item's class) or service lines (tax_class). */
  /** Item lines (the item's class), ledger lines (the ledger's class) or a class given directly. */
  lines: { item_id?: string; ledger_id?: string; tax_class?: string; taxable_value: string }[];
}

export interface InvoiceTaxRow {
  component: string;
  percent: string;
  amount: string;
  /** null: no duty ledger collects this tax yet (see warnings). */
  ledger_id: string | null;
  ledger_name: string | null;
}

export interface BankStatementRow {
  date: string;
  description: string;
  reference: string;
  deposit: string;
  withdrawal: string;
  /** The uncleared voucher it matches; null: no match. */
  voucher_id: string | null;
  voucher_date: string | null;
  voucher_number: string | null;
  voucher_kind: VoucherKind | null;
  particulars: string | null;
  /** The statement's cheque/ref number matched the voucher. */
  reference_matched: boolean;
}

export interface InvoiceTax {
  place_of_supply: string | null;
  rows: InvoiceTaxRow[];
  warnings: string[];
}

// ------------------------------------------------------- users and roles

/** Each role includes the ones before it. */
export type UserRole = "Auditor" | "DataEntry" | "Accountant" | "Owner";

export interface Session {
  /** The company has users, so someone must sign in. */
  required: boolean;
  user: string | null;
  role: UserRole | null;
}

export interface CompanyUserRow {
  name: string;
  role: UserRole;
  disabled: boolean;
}

// ------------------------------------------------------------ GST returns

/** Taxable value and tax; SGST includes UTGST. */
export interface GstAmounts {
  taxable: string;
  igst: string;
  cgst: string;
  sgst: string;
  cess: string;
}

export type GstDocKind = "Invoice" | "CreditNote" | "DebitNote";

/** A document reported invoice-wise (B2B, B2CL, CDNR, CDNUR, exports). */
export interface GstReturnDoc {
  /** The voucher id. */
  id: string;
  kind: GstDocKind;
  number: string;
  date: string;
  party_name: string;
  party_gstin: string | null;
  place_of_supply: string;
  value: string;
  items: { rate: string; amounts: GstAmounts }[];
  /** CDNUR and exports: "B2CL", "EXPWP" or "EXPWOP". */
  typ: string | null;
}

export interface GstB2csRow {
  inter_state: boolean;
  place_of_supply: string;
  rate: string;
  amounts: GstAmounts;
}

export interface GstNilRow {
  inter_state: boolean;
  registered: boolean;
  value: string;
}

export interface GstHsnRow {
  /** "" when the lines had no HSN/SAC code. */
  hsn: string;
  uqc: string;
  qty: string;
  rate: string;
  amounts: GstAmounts;
}

export interface GstDocIssueRow {
  doc_num: number;
  doc_type: string;
  from: string;
  to: string;
  total: number;
  cancelled: number;
  net_issued: number;
}

export type Gstr1Section = "b2b" | "b2cl" | "b2cs" | "cdnr" | "cdnur" | "exp" | "nil" | "hsn_b2b" | "hsn_b2c" | "docs";

export interface Gstr1 {
  gstin: string | null;
  /** The company's state code. */
  state: string;
  period: { from: string; to: string };
  /** Return period, MMYYYY. */
  fp: string;
  b2b: GstReturnDoc[];
  b2cl: GstReturnDoc[];
  b2cs: GstB2csRow[];
  cdnr: GstReturnDoc[];
  cdnur: GstReturnDoc[];
  exp: GstReturnDoc[];
  nil: GstNilRow[];
  hsn_b2b: GstHsnRow[];
  hsn_b2c: GstHsnRow[];
  docs_issued: GstDocIssueRow[];
  summary: { section: Gstr1Section; label: string; count: number; amounts: GstAmounts }[];
  /** All outward supplies, net of credit notes. */
  net: GstAmounts;
  warnings: string[];
}

export interface Gstr3b {
  gstin: string | null;
  state: string;
  period: { from: string; to: string };
  /** 3.1(a), (b), (c). */
  outward_taxable: GstAmounts;
  zero_rated: GstAmounts;
  nil_exempt: GstAmounts;
  /** 3.2 */
  inter_state_unregistered: { place_of_supply: string; taxable: string; igst: string }[];
  /** 4(A)(5) */
  itc: GstAmounts;
  /** Output − ITC per component, without set-off between components. */
  net: { component: string; output: string; itc: string; net: string }[];
  /** 3.1(a) + 3.1(b). */
  output: GstAmounts;
  warnings: string[];
}

// ------------------------------------------------------------ UAE VAT 201

/** An amount and its VAT, in AED (decimal strings). */
export interface VatBoxAmounts {
  amount: string;
  vat: string;
}

/** VAT 201 from the books. Boxes 2, 3, 6, 7 and 10 are not modelled and stay 0. */
export interface Vat201 {
  trn: string | null;
  /** The company's emirate code ("DXB"). */
  emirate: string;
  period: { from: string; to: string };
  /** Boxes 1a to 1g: standard-rated supplies by emirate, all seven in box order. */
  standard_rated: { box: string; emirate: string; name: string; amounts: VatBoxAmounts }[];
  /** Box 2 */
  tourist_refunds: VatBoxAmounts;
  /** Box 3 */
  reverse_charge_supplies: VatBoxAmounts;
  /** Box 4 (amount only) */
  zero_rated: string;
  /** Box 5 (amount only) */
  exempt: string;
  /** Box 6 */
  imports: VatBoxAmounts;
  /** Box 7 */
  import_adjustments: VatBoxAmounts;
  /** Box 8 */
  total_supplies: VatBoxAmounts;
  /** Box 9: no input-tax blocking or apportionment applied. */
  standard_expenses: VatBoxAmounts;
  /** Box 10 */
  reverse_charge_recovery: VatBoxAmounts;
  /** Box 11 */
  total_expenses: VatBoxAmounts;
  /** Box 12 */
  total_due: string;
  /** Box 13 */
  total_recoverable: string;
  /** Box 14: positive is payable, negative is recoverable. */
  net: string;
  warnings: string[];
}

// ------------------------------------- Schedule III (India, Division I)

/** A Schedule III line item (khata_in::schedule3::LineItem). */
export type Schedule3Item =
  | "share_capital"
  | "reserves_and_surplus"
  | "money_received_against_share_warrants"
  | "share_application_money_pending_allotment"
  | "long_term_borrowings"
  | "deferred_tax_liabilities"
  | "other_long_term_liabilities"
  | "long_term_provisions"
  | "short_term_borrowings"
  | "trade_payables"
  | "other_current_liabilities"
  | "short_term_provisions"
  | "property_plant_and_equipment"
  | "intangible_assets"
  | "capital_work_in_progress"
  | "intangible_assets_under_development"
  | "non_current_investments"
  | "deferred_tax_assets"
  | "long_term_loans_and_advances"
  | "other_non_current_assets"
  | "current_investments"
  | "inventories"
  | "trade_receivables"
  | "cash_and_cash_equivalents"
  | "short_term_loans_and_advances"
  | "other_current_assets"
  | "revenue_from_operations"
  | "other_income"
  | "cost_of_materials_consumed"
  | "purchases_of_stock_in_trade"
  | "changes_in_inventories"
  | "employee_benefits_expense"
  | "finance_costs"
  | "depreciation_and_amortisation"
  | "other_expenses"
  | "current_tax"
  | "deferred_tax";

export type Schedule3SourceKind =
  | "group"
  | "closing_stock"
  | "opening_stock"
  | "profit_brought_forward"
  | "profit_for_the_year"
  | "opening_difference";

/** What a line's figure is made of (for drill-down). */
export interface Schedule3Source {
  kind: Schedule3SourceKind;
  /** The group whose mapping placed its ledgers here (kind "group"). */
  group: string | null;
  name: string;
  amount: string;
  previous: string;
  /** Every ledger under the group is in this line: its Group Summary shows the same figure. */
  whole_group: boolean;
}

export interface Schedule3Ledger {
  ledger: string;
  name: string;
  group: string;
  group_name: string;
  amount: string;
  previous: string;
}

export interface Schedule3Row {
  kind: "heading" | "line" | "total";
  /** Indent: 0 main headings, 1 sections, 2–3 lines. */
  level: number;
  label: string;
  item: Schedule3Item | null;
  note: number | null;
  amount: string;
  previous: string;
  sources: Schedule3Source[];
  ledgers: Schedule3Ledger[];
}

export interface Schedule3Figures {
  current: string;
  previous: string;
}

export interface Schedule3BalanceSheet {
  as_at: string;
  previous_as_at: string | null;
  rows: Schedule3Row[];
  total_equity_and_liabilities: Schedule3Figures;
  total_assets: Schedule3Figures;
  /** What the user must still classify or supply. */
  warnings: string[];
}

export interface Schedule3ProfitAndLoss {
  from: string;
  to: string;
  previous_from: string | null;
  previous_to: string | null;
  rows: Schedule3Row[];
  total_income: Schedule3Figures;
  total_expenses: Schedule3Figures;
  profit_before_tax: Schedule3Figures;
  tax_expense: Schedule3Figures;
  /** Negative for a loss. */
  profit_for_the_period: Schedule3Figures;
  warnings: string[];
}

// ------------------------------------------------- e-invoice and e-way bill

/** Minimum HSN digits: 4 up to ₹5 crore AATO, 6 above (Notif. 78/2020-CT). */
export type HsnDigits = 4 | 6 | 8;

export type EinvoiceSupplyType = "B2B" | "SEZWP" | "SEZWOP" | "EXPWP" | "EXPWOP" | "DEXP";

/**
 * What an e-invoice needs about the buyer that the books may lack. Every field is optional:
 * empty falls back to the party ledger (name, GSTIN, mailing details). Nothing is saved.
 */
export interface EinvoiceBuyerDetails {
  legal_name?: string;
  /** Replaces the ledger's street lines (with address2). */
  address1?: string;
  address2?: string;
  /** City or town. */
  location?: string;
  pin?: string;
  /** Two-digit GST state code of the address. */
  state_code?: string;
  /** Default: the GSTIN's state, else the mailing state. */
  place_of_supply?: string;
  /** Empty: export when the party's mailing country is not India, else B2B. */
  supply_type?: EinvoiceSupplyType | "";
  reverse_charge?: boolean;
  igst_on_intra?: boolean;
}

export type EwayDocType = "Invoice" | "BillOfSupply" | "Challan";
export type EwaySubSupply =
  | "Supply"
  | "Import"
  | "Export"
  | "JobWork"
  | "ForOwnUse"
  | "JobWorkReturns"
  | "SalesReturn"
  | "Others"
  | "SkdCkd"
  | "LineSales"
  | "RecipientNotKnown"
  | "ExhibitionOrFairs";
export type EwayTransactionType = "Regular" | "BillToShipTo" | "BillFromDispatchFrom" | "Combination";
export type EwayTransMode = "Road" | "Rail" | "Air" | "Ship";

/** e-Way bill inputs beyond the voucher: the other party's address when missing, and Part B. */
export interface EwayDetails {
  /** The other party: consignee on a sale, consignor on a purchase. */
  name?: string;
  address1?: string;
  address2?: string;
  location?: string;
  pin?: string;
  state_code?: string;
  actual_from_state?: string;
  actual_to_state?: string;
  /** Supplier's invoice number and date (YYYY-MM-DD) on a purchase. */
  doc_no?: string;
  doc_date?: string;
  doc_type?: EwayDocType | null;
  sub_supply?: EwaySubSupply | null;
  transaction_type?: EwayTransactionType | null;
  /** Approximate distance in km (required). */
  distance_km?: number | null;
  // Part B (optional; can be added on the portal later)
  trans_mode?: EwayTransMode | null;
  transporter_id?: string;
  transporter_name?: string;
  trans_doc_no?: string;
  /** YYYY-MM-DD */
  trans_doc_date?: string;
  vehicle_no?: string;
  over_dimensional?: boolean;
}

// ---------------------------------------------------------------- TDS (India)
// See docs/india-tds.md. Rates and limits are research seed data, to verify.

export type DeducteeKind =
  | "Individual"
  | "Huf"
  | "Company"
  | "Firm"
  | "Aop"
  | "Boi"
  | "Trust"
  | "LocalAuthority"
  | "Juridical"
  | "Government";

export const DEDUCTEE_LABEL: Record<DeducteeKind, string> = {
  Individual: "Individual",
  Huf: "HUF",
  Company: "Company",
  Firm: "Firm / LLP",
  Aop: "Association of persons",
  Boi: "Body of individuals",
  Trust: "Trust",
  LocalAuthority: "Local authority",
  Juridical: "Artificial juridical person",
  Government: "Government",
};

/** What a deductible ledger tells the TDS calculator (all optional, but a nature or a deductee is needed). */
export interface TdsTerms {
  /** Nature-of-payment code from tdsNatures() ("contract", "professional"…). */
  nature?: string | null;
  deductee?: DeducteeKind | null;
  /** Lower- or nil-deduction certificate: rate in percent, number, validity (YYYY-MM-DD). */
  lower_rate?: string | null;
  certificate?: string | null;
  valid_from?: string | null;
  valid_to?: string | null;
}

/** TDS settings on a ledger: deductible (expense or party), or a TDS payable duty ledger. */
export type LedgerTds = ({ kind: "Deductible" } & TdsTerms) | { kind: "Payable"; payable_for: string };

/** `payable_for` of a TDS payable ledger that holds every nature. */
export const TDS_ALL_NATURES = "all";

// Declaration merging: PAN and TDS on the ledger view and the ledger form.
export interface Ledger {
  /** The party's PAN as stored (a GSTIN's embedded PAN is used when this is empty). */
  pan: string | null;
  tds: LedgerTds | null;
}

export interface LedgerInput {
  /** Omitted: keep; "": clear. */
  pan?: string | null;
  /** Omitted: keep; null: clear. */
  tds?: LedgerTds | null;
}

export interface TdsNature {
  code: string;
  name: string;
  /** 1961 Act section ("194C"). */
  old_section: string;
  /** 2025 Act provision ("s.393(1)"). */
  new_section: string;
  rate_individual_huf: string;
  rate_others: string;
  /** The limit in words. */
  threshold: string;
  statement: "Quarterly" | "ChallanCumStatement";
  condition: string | null;
  caution: string | null;
}

export type TdsRateBasis = "Normal" | "NoPan" | "LowerCertificate";
export type TdsThresholdHit = "Below" | "Single" | "AggregateCrossed" | "AggregateAlready" | "Excess" | "Monthly" | "Transaction";

/** Suggested TDS for a voucher being entered. */
export interface TdsSuggestion {
  applies: boolean;
  /** TDS to deduct, rounded to the rupee. */
  amount: string;
  nature: string | null;
  nature_name: string | null;
  section: string | null;
  rate: string | null;
  rate_basis: TdsRateBasis | null;
  base: string;
  threshold: TdsThresholdHit | null;
  explanation: string;
  ytd_amount: string;
  ytd_tds: string;
  pan: string | null;
  deductee: DeducteeKind | null;
  /** Where to credit the TDS; null: none exists yet (see warnings). */
  payable_ledger_id: string | null;
  payable_ledger_name: string | null;
  warnings: string[];
}

export interface TdsDeductionRow {
  voucher_id: string;
  kind: VoucherKind;
  number: string;
  date: string;
  party_id: string | null;
  party_name: string;
  pan: string | null;
  nature: string;
  nature_name: string;
  section: string;
  amount: string;
  tds: string;
  deposit_due: string;
}

export interface TdsChallanRow {
  voucher_id: string;
  kind: VoucherKind;
  number: string;
  date: string;
  ledger_id: string;
  ledger_name: string;
  /** Nature code or "all". */
  nature: string;
  amount: string;
}

export interface TdsNatureTotals {
  nature: string;
  name: string;
  section: string;
  paid_credited: string;
  deducted: string;
  deposited: string;
  /** Closing balance of ledgers tagged for this nature; null when held in an all-natures ledger. */
  outstanding: string | null;
  by_month: { month: string; deducted: string; due: string }[];
}

export interface TdsPayable {
  from: string;
  to: string;
  natures: TdsNatureTotals[];
  rows: TdsDeductionRow[];
  challans: TdsChallanRow[];
  ledgers: { id: string; name: string; payable_for: string; opening: string; closing: string }[];
  paid_credited: string;
  deducted: string;
  deposited: string;
  warnings: string[];
}

export interface TdsStatementRow {
  serial: number;
  party_id: string | null;
  party_name: string;
  pan: string | null;
  deductee: DeducteeKind | null;
  nature: string;
  nature_name: string;
  section: string;
  date: string;
  amount: string;
  tds: string;
  rate: string;
  reason: string | null;
  voucher_id: string;
  kind: VoucherKind;
  number: string;
}

export interface TdsQuarterly {
  /** "2026-27". */
  year: string;
  quarter: number;
  from: string;
  to: string;
  /** "Form 140" or "Form 26Q". */
  form: string;
  act: "Ita1961" | "Ita2025";
  due: string | null;
  deductor_name: string;
  deductor_gstin: string | null;
  deductor_pan: string | null;
  rows: TdsStatementRow[];
  challans: TdsChallanRow[];
  amount: string;
  tds: string;
  deposited: string;
  warnings: string[];
  gaps: string[];
}

// ------------------------------------------------------- Singapore GST F5

/** A yes/no question with an amount (GST F5 Boxes 10–12 and 14–17). */
export interface GstYesNoBox {
  yes: boolean;
  amount: string;
}

/**
 * GST F5 from the books (khata_sg::returns), in SGD decimal strings. Boxes 9–12
 * and 14–17 are not modelled and stay "No" / 0; the IGDS section (18–21) is not
 * shown. See docs/singapore-gst.md.
 */
export interface GstF5 {
  gst_no: string | null;
  period: { from: string; to: string };
  /** Box 1 */
  standard_rated: string;
  /** Box 2 */
  zero_rated: string;
  /** Box 3 */
  exempt: string;
  /** Box 4 = 1 + 2 + 3 */
  total_supplies: string;
  /** Box 5: no input tax disallowance applied. */
  taxable_purchases: string;
  /** Box 6 */
  output_tax: string;
  /** Box 7: all GST on Box 5 taken as claimable. */
  input_tax: string;
  /** Box 8 = 6 − 7: positive is payable, negative is claimable. */
  net_gst: string;
  /** Box 9 */
  scheme_imports: string;
  /** Box 10 */
  tourist_refunds: GstYesNoBox;
  /** Box 11 */
  bad_debt_relief: GstYesNoBox;
  /** Box 12 */
  pre_registration: GstYesNoBox;
  /** Box 13: an estimate from Sales and Credit Notes. */
  revenue: string;
  /** Box 14 */
  reverse_charge: GstYesNoBox;
  /** Box 15 */
  marketplace_remote_services: GstYesNoBox;
  /** Box 16 */
  marketplace_low_value_goods: GstYesNoBox;
  /** Box 17 */
  own_low_value_goods: GstYesNoBox;
  /** In no box; shown for reference. */
  out_of_scope: string;
  warnings: string[];
}

/** UK 9-box VAT return (khata-gb returns.rs; a preparation aid, see docs/uk-vat.md). Amounts are decimal strings in GBP. */
export interface UkVatReturn {
  /** The company's VAT registration number as stored. */
  vrn: string | null;
  /** Nation NIR or an XI number: Boxes 8 and 9 can be non-zero. */
  northern_ireland: boolean;
  period: { from: string; to: string };
  /** Box 1: VAT due on sales. */
  vat_due_sales: string;
  /** Box 2: VAT due on acquisitions from the EU into Northern Ireland (not modelled, 0). */
  vat_due_acquisitions: string;
  /** Box 3: Boxes 1 + 2. */
  total_vat_due: string;
  /** Box 4: VAT reclaimed on purchases. */
  vat_reclaimed: string;
  /** Box 5: Box 3 − Box 4; negative is a repayment. */
  net_vat_due: string;
  /** Box 6: total sales excluding VAT, whole pounds. */
  total_sales_ex_vat: string;
  /** Box 7: total purchases excluding VAT, whole pounds. */
  total_purchases_ex_vat: string;
  /** Box 8: goods supplied from Northern Ireland to the EU, whole pounds. */
  goods_supplied_to_eu: string;
  /** Box 9: goods acquired into Northern Ireland from the EU, whole pounds. */
  goods_acquired_from_eu: string;
  warnings: string[];
}

/** A ZATCA e-invoice as generated (desktop zatca.rs; see docs/ksa-zatca.md). */
export interface ZatcaInvoice {
  /** UBL 2.1 XML with its declaration. */
  xml: string;
  uuid: string;
  /** Invoice counter value on this device; never reused. */
  icv: number;
  /** This document's hash (Base64): the next document's PIH. */
  hash: string;
  /** The previous invoice hash this document carries. */
  pih: string;
  /** QR payload: Base64 TLV, tags 1 to 5. */
  qr: string;
  /** `<VAT number>_<yyyymmdd>T<hhmmss>_<number>.xml` */
  file_name: string;
  /** Already generated: this is the recorded document. */
  regenerated: boolean;
  warnings: string[];
}

/** The seller's CRN and Saudi National Address, kept outside the books until the company profile holds them. */
export interface ZatcaSeller {
  crn: string;
  /** 4 digits. */
  building_number: string;
  street: string;
  additional_street: string;
  additional_number: string;
  district: string;
  city: string;
  postal_code: string;
}

export interface ZatcaSellerInfo {
  stored: ZatcaSeller;
  /** What an e-invoice will use: stored fields, gaps filled from the company address. */
  effective: ZatcaSeller;
}

/** An amount and its VAT, decimal strings in SAR. */
export interface KsaAmounts {
  amount: string;
  vat: string;
}

/** KSA VAT return (khata-sa returns.rs; a preparation aid, see docs/ksa-zatca.md). Lines not modelled are always 0. */
export interface KsaVatReturn {
  vat_number: string | null;
  period: { from: string; to: string };
  standard_rated_sales: KsaAmounts;
  /** Not modelled, 0. */
  citizen_healthcare_education_sales: KsaAmounts;
  /** Not modelled, 0. */
  gcc_registered_customer_sales: KsaAmounts;
  zero_rated_domestic_sales: string;
  exports: string;
  exempt_sales: string;
  total_sales: KsaAmounts;
  standard_rated_purchases: KsaAmounts;
  /** Not modelled, 0. */
  imports_paid_at_customs: KsaAmounts;
  /** Not modelled, 0. */
  imports_reverse_charge: KsaAmounts;
  zero_rated_purchases: string;
  exempt_purchases: string;
  total_purchases: KsaAmounts;
  /** Sales VAT − purchase VAT. */
  total_vat_due: string;
  /** Not modelled, 0. */
  corrections: string;
  /** Not tracked, 0. */
  credit_carried_forward: string;
  /** Positive: payable; negative: a credit. */
  net: string;
  warnings: string[];
}

// ------------------------------------------------ UK Making Tax Digital (HMRC)

export type MtdEnvironment = "sandbox" | "production";

/** MTD settings (src-tauri/src/mtd_client.rs). The client secret and the tokens never reach the webview. */
export interface MtdSettings {
  environment: MtdEnvironment;
  client_id: string;
  has_secret: boolean;
  redirect_port: number;
  /** What to register as the redirect URI of the user's HMRC application. */
  redirect_uri: string;
  /** The open company's VRN (nine digits), when valid. */
  vrn: string | null;
  connected: boolean;
  access_expires_at: string | null;
  /** HMRC needs a new sign-in after this date (18 months from granting). */
  reauthorise_by: string | null;
  /** HMRC's declaration text, shown before submitting. */
  declaration: string;
  receipts: MtdReceipt[];
}

/** Screens and window for the fraud-prevention headers, read at the time of each call. */
export interface MtdClientInfo {
  screens: { width: number; height: number; scaling_factor: number; colour_depth: number }[];
  window: { width: number; height: number };
}

export interface MtdObligation {
  start: string;
  end: string;
  due: string;
  /** "O" open, "F" fulfilled. */
  status: string;
  period_key: string;
  received: string | null;
  /** This company file's receipt for the period, if KhataOS submitted it. */
  receipt: MtdReceipt | null;
}

export interface MtdReceipt {
  environment: MtdEnvironment;
  vrn: string;
  period_key: string;
  from: string;
  to: string;
  processing_date: string;
  form_bundle_number: string;
  payment_indicator: string | null;
  charge_ref_number: string | null;
  receipt_id: string | null;
  receipt_timestamp: string | null;
  correlation_id: string | null;
  submitted_at: string;
  submitted_by: string;
  /** The body sent to HMRC. */
  return_json: string;
}

/** A return as HMRC holds it; amounts are HMRC's numbers as text. */
export interface MtdViewedReturn {
  period_key: string;
  vat_due_sales: string;
  vat_due_acquisitions: string;
  total_vat_due: string;
  vat_reclaimed_curr_period: string;
  net_vat_due: string;
  total_value_sales_ex_vat: string;
  total_value_purchases_ex_vat: string;
  total_value_goods_supplied_ex_vat: string;
  total_acquisitions_ex_vat: string;
}

export interface MtdLiability {
  from: string | null;
  to: string | null;
  kind: string;
  original_amount: string;
  outstanding_amount: string | null;
  due: string | null;
}

export interface MtdPayment {
  amount: string;
  received: string | null;
}

export interface MtdHeaderProblem {
  code: string;
  message: string;
  headers: string[];
}

/** HMRC's Test Fraud Prevention Headers API verdict. */
export interface MtdHeaderCheck {
  spec_version: string | null;
  code: string;
  message: string;
  errors: MtdHeaderProblem[];
  warnings: MtdHeaderProblem[];
  /** [header, value] as sent. */
  sent: [string, string][];
}

export interface MtdFeedback {
  method: string;
  path: string;
  request_timestamp: string;
  code: string;
  problems: string[];
}

// ------------------------------------------------- IRP connector (India e-invoice API)

/** A known IRP API host (khata-in irp.rs ENDPOINTS); "custom" takes a base URL (a GSP or IRP 3 to 6). */
export interface IrpEndpoint {
  id: string;
  label: string;
  base_url: string;
  production: boolean;
}

/** The IRP connection for the company's GSTIN. Secrets are never returned, only whether they are set. */
export interface IrpSettings {
  gstin: string;
  endpoint: string;
  base_url: string;
  endpoints: IrpEndpoint[];
  production: boolean;
  client_id: string;
  user_name: string;
  has_client_secret: boolean;
  has_password: boolean;
  /** "RSA 2048-bit" when a usable key is stored, an error text when not usable, null when none. */
  public_key: string | null;
  verify_cert: string | null;
  session_active: boolean;
  /** The credentials file (app data folder, this OS user only). */
  stored_in: string;
}

/** Saving IRP settings: empty client_secret, password, public_key_pem or verify_cert_pem keep the stored value. */
export interface IrpSettingsInput {
  endpoint: string;
  base_url: string;
  client_id: string;
  client_secret: string;
  user_name: string;
  password: string;
  public_key_pem: string;
  verify_cert_pem: string;
  clear_verify_cert: boolean;
}

/** The fields of the IRP's signed QR code (decoded, khata-in irp.rs SignedQr). */
export interface IrpSignedQr {
  seller_gstin: string;
  buyer_gstin: string;
  doc_no: string;
  doc_type: string;
  doc_date: string;
  total_invoice_value: string;
  item_count: string;
  main_hsn: string;
  irn: string;
  irn_date: string | null;
  issuer: string | null;
}

/** A voucher's IRN as the IRP returned it: a shared record of the company (synced with the books). */
export interface IrnResult {
  voucher_id: string;
  voucher_number: string;
  /** INV, CRN or DBN, and the document number and date (dd/mm/yyyy) as registered. */
  doc_type: string;
  doc_no: string;
  doc_date: string;
  gstin: string;
  endpoint: string;
  irn: string;
  ack_no: string;
  /** yyyy-MM-dd HH:mm:ss, IST. */
  ack_dt: string;
  /** ACT active, CNL cancelled, REJ rejected. */
  status: string;
  signed_invoice: string;
  signed_qr_code: string;
  ewb_no: string | null;
  ewb_dt: string | null;
  ewb_valid_till: string | null;
  remarks: string | null;
  cancel_date: string | null;
  cancel_reason: string | null;
  cancel_remark: string | null;
  basis: unknown;
  notes: string[];
  updated_at: string;
  /** The decrypted IRP answers, verbatim: { api, at, data }. */
  responses: unknown[];
  qr: IrpSignedQr | null;
  /** "verified", "invalid", or "not_checked" (no NIC certificate in IRP settings). */
  signature: "verified" | "invalid" | "not_checked";
  /** End of the 24-hour cancellation window (IST) while it is open. */
  cancel_until: string | null;
  /** The signed QR code as SVG, for the screen. */
  qr_svg: string | null;
  /** What this call did, and warnings. */
  messages: string[];
}

/** Cancel IRN reasons: 1 duplicate, 2 data entry mistake, 3 order cancelled, 4 other. */
export type IrpCancelReason = "1" | "2" | "3" | "4";

/** Generate e-way bill by IRN: Part B, or a transporter id alone for Part A only. */
export interface IrpEwbDetails {
  /** 0: the portal works it out from the PIN codes; at most 4000. */
  distance_km: number;
  trans_mode: EwayTransMode | null;
  transporter_id: string;
  transporter_name: string;
  trans_doc_no: string;
  /** dd/mm/yyyy or yyyy-mm-dd. */
  trans_doc_date: string;
  vehicle_no: string;
  over_dimensional: boolean;
}

/** Get GSTIN details from the e-invoice system. */
export interface IrpGstinDetails {
  gstin: string;
  trade_name: string | null;
  legal_name: string | null;
  address: string[];
  location: string | null;
  state_code: string | null;
  pin: string | null;
  /** REG, SEZ, COM, ... */
  taxpayer_type: string | null;
  /** ACT, CNL, INA or PRO. */
  status: string | null;
  /** "B": e-way bills blocked. */
  block_status: string | null;
  registered_on: string | null;
  deregistered_on: string | null;
}


// ------------------------------------------------------------ United States (khata-us)

export type UsTaxability = "Taxable" | "Exempt" | "NotCollected";

/** Sales tax totals for a jurisdiction or a state; amounts are decimal strings. */
export interface UsSalesTaxTotals {
  gross_sales: string;
  exempt_sales: string;
  not_collected_sales: string;
  taxable_sales: string;
  tax_collected: string;
  documents: number;
}

export interface UsJurisdictionRow extends UsSalesTaxTotals {
  /** "CA" or a company-defined "CA-LA". */
  jurisdiction: string;
  state: string;
  state_name: string;
}

export interface UsNexusThreshold {
  state: string;
  sales: number;
  transactions_and: number | null;
  strictly_more: boolean;
  measured: string;
  source: string;
}

export interface UsNexusCheck {
  threshold: UsNexusThreshold;
  sales: string;
  transactions: number;
  met: boolean;
}

export interface UsStateRow extends UsSalesTaxTotals {
  state: string;
  state_name: string;
  /** Economic nexus early warning (calendar year to date) for states with a sourced threshold. */
  nexus: UsNexusCheck | null;
}

export interface UsSalesTaxReport {
  ein: string | null;
  state: string | null;
  period: { from: string; to: string };
  jurisdictions: UsJurisdictionRow[];
  states: UsStateRow[];
  export_sales: string;
  total: UsSalesTaxTotals;
  warnings: string[];
}

export type Us1099Kind = "Nonemployee" | "Rents" | "Royalties" | "OtherIncome";

export interface Us1099Row {
  party_id: string;
  party_name: string;
  tin: string | null;
  kind: Us1099Kind;
  /** "1099-NEC box 1a"… */
  form: string;
  total: string;
  payments: number;
}

export interface Us1099Report {
  year: number;
  threshold: string;
  rows: Us1099Row[];
  below: number;
  warnings: string[];
}

// ------------------------------------------------------------ Mexico (khata-mx)

export interface MxActivities {
  taxed_16: string;
  taxed_8: string;
  taxed_other: string;
  zero_rated: string;
  exempt: string;
}

export interface MxIvaSummary {
  rfc: string | null;
  period: { from: string; to: string };
  sales: MxActivities;
  purchases: MxActivities;
  iva_trasladado: string;
  iva_acreditable: string;
  iva_a_cargo: string;
  saldo_a_favor: string;
  warnings: string[];
}

/** Issuer details a CFDI needs (stored in the company file; not secret). */
export interface MxIssuer {
  regimen_fiscal: string;
  lugar_expedicion: string;
  serie: string;
  nombre: string;
}

/** The installed CSD (kept in the app data directory, never in the books). */
export interface MxCsdStatus {
  rfc: string;
  installed: boolean;
  no_certificado: string | null;
}

export interface MxCfdiOptions {
  forma_pago: string;
  metodo_pago: "PUE" | "PPD" | "";
  uso_cfdi: string;
  regimen_fiscal_receptor: string;
  related_uuid: string;
  /** The CSD key's password, used once and never stored; blank for an unsealed draft. */
  password: string;
}

export interface MxCfdi {
  xml: string;
  cadena_original: string;
  sealed: boolean;
  no_certificado: string;
  total: string;
  warnings: string[];
}

/** ZATCA environment: the developer portal (sandbox), simulation, or production (docs/ksa-zatca.md). */
export type ZatcaEnvironment = "sandbox" | "simulation" | "production";

/** One of ZATCA's compliance checks during onboarding. */
export interface ZatcaComplianceCheck {
  /** "standard invoice", "simplified credit note"… */
  document: string;
  /** PASS, WARNING or ERROR. */
  status: string;
  warnings: string[];
  errors: string[];
}

/** This computer's ZATCA device (EGS). Secrets are never sent to the screen: only whether they exist. */
export interface ZatcaStatus {
  /** Ready: documents are stamped and can be submitted. */
  onboarded: boolean;
  environment: ZatcaEnvironment | null;
  device_id: string | null;
  common_name: string | null;
  /** 1-KhataOS|2-<version>|3-<device id> */
  egs_serial: string | null;
  invoice_types: string | null;
  csr_created: boolean;
  /** The CSR sent to ZATCA (public). */
  csr_pem: string | null;
  compliance_csid: boolean;
  checks: ZatcaComplianceCheck[];
  production_csid: boolean;
  /** yyyy-mm-dd */
  certificate_expiry: string | null;
  /** The user-only file on this computer holding the key and CSID secrets. */
  credentials_path: string | null;
  credentials_present: boolean;
  /** The device's last invoice counter and the next PIH. */
  icv: number;
  pih: string;
  next_step: string;
  last_error: string | null;
  warnings: string[];
}

/** CSR details for onboarding; blanks are filled from the company. */
export interface ZatcaOnboardOptions {
  /** OU: branch name (a VAT group member's 10-digit TIN). */
  branch: string;
  /** National Address short address or branch address. */
  location: string;
  industry: string;
  /** "1100" standard and simplified, "1000" standard only, "0100" simplified only. */
  invoice_types: string;
  /** A new device id and key. */
  restart: boolean;
}

/** ZATCA's answer to a clearance or reporting call, as kept per voucher. */
export interface ZatcaSubmission {
  voucher_id: string;
  kind: "standard" | "simplified";
  /** "clearance" or "reporting". */
  endpoint: string;
  environment: string;
  submitted_at: string;
  accepted: boolean;
  /** The stored answer of an earlier submission (nothing was sent now). */
  already_submitted: boolean;
  http_status: number;
  /** PASS, WARNING or ERROR. */
  status: string;
  reporting_status: string | null;
  clearance_status: string | null;
  info: string[];
  warnings: string[];
  errors: string[];
  /** The XML ZATCA cleared (standard documents): the one to give the buyer. */
  cleared_xml: string | null;
  file_name: string;
  notes: string[];
}

/** A ZATCA e-invoice that may carry the cryptographic stamp. */
export interface ZatcaStampedInvoice extends ZatcaInvoice {
  /** XAdES signature and QR tags 6–9 present. */
  signed: boolean;
}

/** Germany: the UStVA (khata-de returns.rs; a preparation aid, see docs/germany-vat.md). Bases are whole euros, taxes euros and cents. */
export interface Ustva {
  /** The company's USt-IdNr or Steuernummer as stored. */
  tax_id: string | null;
  period: { from: string; to: string } | null;
  kz81: string;
  kz81_tax: string;
  kz86: string;
  kz86_tax: string;
  kz35: string;
  kz36: string;
  kz87: string;
  kz41: string;
  kz43: string;
  kz48: string;
  kz60: string;
  kz21: string;
  kz45: string;
  kz89: string;
  kz89_tax: string;
  kz93: string;
  kz93_tax: string;
  kz46: string;
  kz47: string;
  kz84: string;
  kz85: string;
  kz66: string;
  kz61: string;
  kz67: string;
  output_tax: string;
  input_tax: string;
  /** Positive: prepayment due; negative: refund (Überschuss). */
  kz83: string;
  warnings: string[];
}

/** An XRechnung 3.0 document for a Sales or Credit Note voucher. */
export interface XRechnung {
  xml: string;
  syntax: "ubl" | "cii";
  file_name: string;
  /** Broken EN 16931 / XRechnung rules: not a valid XRechnung until fixed. */
  errors: string[];
  warnings: string[];
}

// ------------------------------------------- TCS, non-residents, statement file, salary (India)
// See docs/india-tds.md and docs/research/india-tds-tcs.md.

/** Where a TDS nature is reported (TdsNature.statement may also be "NonResident"). */
export type TdsStatementKind = "Quarterly" | "ChallanCumStatement" | "NonResident";

/** A non-resident payee's terms; the rate is always the user's (treaty or Act). */
export interface NonResidentTerms {
  /** Percent, including any surcharge and cess. */
  rate: string;
  /** The rate is the treaty's (Form 144 "B") rather than the Act's ("A"). */
  treaty?: boolean;
  /** Form 144 country code (Annexure 10), e.g. "267" for the USA. */
  country?: string | null;
  /** Form 144 nature-of-remittance code (Annexure 9), e.g. "21". */
  remittance?: string | null;
  foreign_tin?: string | null;
}

// Declaration merging: the fields added for non-residents and 4-digit codes.
export interface TdsTerms {
  non_resident?: NonResidentTerms | null;
}

export interface TdsNature {
  /** Row of the 2025 Act's table ("Sl. No. 6(i)"). */
  table: string;
  /** 4-digit statement codes from tax year 2026-27. */
  tax_code_individual_huf: string;
  tax_code_others: string;
  /** The rate is the user's (non-residents). */
  user_rate: boolean;
}

export interface TdsStatementRow {
  tax_code: string;
}

export interface TcsNature {
  code: string;
  name: string;
  /** "s.394(1) [Table: Sl. No. 4] (ex-206C(1))". */
  section: string;
  old_section: string;
  tax_code: string;
  rate: string;
  no_pan_rate: string;
  threshold: string;
  collector: string;
  declaration_allowed: boolean;
  condition: string | null;
  caution: string | null;
}

/** What a collectible ledger tells the TCS calculator (a nature or the buyer's terms). */
export interface TcsTerms {
  nature?: string | null;
  collectee?: DeducteeKind | null;
  lower_rate?: string | null;
  certificate?: string | null;
  valid_from?: string | null;
  valid_to?: string | null;
  /** The buyer's s.394(2) declaration (goods for manufacturing, not trading). */
  declaration?: boolean;
  /** Not a "buyer" under the Act (government, embassy, PSU…): nothing is collected. */
  excluded?: boolean;
  non_resident?: boolean;
  permanent_establishment?: boolean;
}

/** TCS settings on a ledger (separate from `tds`). */
export type LedgerTcs = ({ kind: "Collectible" } & TcsTerms) | { kind: "Payable"; payable_for: string };

export const TCS_ALL_NATURES = "all";

export type TcsBasis = "Normal" | "NoPan" | "LowerCertificate" | "Declaration" | "Excluded" | "BelowLimit";

/** Suggested TCS for a Sales or Receipt voucher being entered. */
export interface TcsSuggestion {
  applies: boolean;
  amount: string;
  nature: string | null;
  nature_name: string | null;
  section: string | null;
  tax_code: string | null;
  rate: string | null;
  basis: TcsBasis | null;
  base: string;
  explanation: string;
  pan: string | null;
  collectee: DeducteeKind | null;
  payable_ledger_id: string | null;
  payable_ledger_name: string | null;
  warnings: string[];
}

export interface TcsStatementRow {
  serial: number;
  party_id: string | null;
  party_name: string;
  pan: string | null;
  collectee: DeducteeKind | null;
  nature: string;
  nature_name: string;
  tax_code: string;
  date: string;
  amount: string;
  tcs: string;
  rate: string;
  /** Form 143 flag: A certificate, B declaration, C no PAN. */
  remark: string | null;
  reason: string | null;
  non_resident: boolean;
  voucher_id: string;
  kind: VoucherKind;
  number: string;
}

export interface TcsQuarterly {
  year: string;
  quarter: number;
  from: string;
  to: string;
  /** "Form 143" or "Form 27EQ". */
  form: string;
  due: string | null;
  deductor_name: string;
  deductor_pan: string | null;
  rows: TcsStatementRow[];
  challans: TdsChallanRow[];
  amount: string;
  tcs: string;
  deposited: string;
  warnings: string[];
}

export interface NonResidentRow {
  serial: number;
  party_id: string | null;
  party_name: string;
  pan: string | null;
  deductee: DeducteeKind | null;
  tax_code: string;
  date: string;
  amount: string;
  tds: string;
  rate: string;
  user_rate: string | null;
  treaty: boolean;
  country: string | null;
  country_name: string | null;
  remittance: string | null;
  foreign_tin: string | null;
  remark: string | null;
  voucher_id: string;
  kind: VoucherKind;
  number: string;
}

export interface NonResidentQuarterly {
  year: string;
  quarter: number;
  from: string;
  to: string;
  /** "Form 144" or "Form 27Q". */
  form: string;
  due: string | null;
  rows: NonResidentRow[];
  challans: TdsChallanRow[];
  amount: string;
  tds: string;
  warnings: string[];
  reminders: string[];
}

export interface CodeName {
  code: string;
  name: string;
}

/** Deductor details as stored (blank fields fall back to the company's). */
export interface TdsDeductorSettings {
  tan: string;
  pan: string;
  /** K company, F firm, Q individual/HUF, P AOP, T trust, B BOI, J AJP, M branch. */
  deductor_type: string;
  address: string;
  /** GST state code ("27"). */
  state: string;
  pin: string;
  email: string;
  phone_country: string;
  phone: string;
  responsible_name: string;
  responsible_designation: string;
  responsible_pan: string;
  responsible_address: string;
  responsible_state: string;
  responsible_pin: string;
  responsible_email: string;
  responsible_phone_country: string;
  responsible_phone: string;
  responsible_country: string;
}

export interface TdsDeductor {
  stored: TdsDeductorSettings;
  /** What the files use: statement state codes, address lines of 25 characters. */
  effective: { tan: string; pan: string; name: string; address: string[]; state: string; pin: string; email: string; deductor_type: string; gstin: string | null };
  responsible: { name: string; designation: string; pan: string; address: string[]; state: string; pin: string; email: string; country: string };
  deductor_types: CodeName[];
  problems: string[];
}

/** A challan voucher's bank details. */
export interface TdsChallanDetails {
  bsr: string;
  serial: string;
  /** YYYY-MM-DD; empty: the voucher's date. */
  date: string;
  interest: string;
  fee: string;
  others: string;
  /** "200" or "400". */
  minor_head: string;
}

export interface TdsStatementFile {
  form: string;
  file_name: string;
  /** The file; null when there are problems. */
  text: string | null;
  problems: string[];
  notes: string[];
}

export interface SalaryEstimateInput {
  /** First year of the tax year (2026); 0 for the current one. */
  tax_year: number;
  salary: string;
  other_salary: string;
  other_income: string;
  tds_so_far: string;
  /** 0: the months left from today. */
  months_remaining: number;
  non_resident: boolean;
}

export interface SalaryEstimate {
  regime: string;
  gross: string;
  standard_deduction: string;
  total_income: string;
  slabs: { from: string; to: string | null; rate: string; income: string; tax: string }[];
  tax_on_income: string;
  rebate: string;
  cess: string;
  annual_tax: string;
  tds_so_far: string;
  remaining: string;
  months_remaining: number;
  monthly: string;
  surcharge_left_out: boolean;
  notes: string[];
}

// Declaration merging: TCS settings on the ledger view (LedgerDto.tcs).
export interface Ledger {
  /** TCS settings: a nature of collection or the buyer's terms, or a TCS payable tag. Older backends omit it. */
  tcs?: LedgerTcs | null;
}

// ------------------------------------- GST: reverse charge, ITC, 2B, 1A

// Declaration merging: reverse charge on invoice tax rows (tax.rs).
export interface InvoiceTaxRow {
  /** Reverse charge on a purchase: the tax owed (output-tax ledger, negative amount). */
  reverse_charge?: boolean;
}

// Declaration merging: the rest of GSTR-3B Table 3.1 and 4 (returns.rs).
export interface Gstr3b {
  /** 3.1(d) inward supplies liable to reverse charge. */
  reverse_charge_inward?: GstAmounts;
  /** 4(A)(3) ITC on reverse charge. */
  itc_reverse_charge?: GstAmounts;
  /** 4(B)(1) s.17(5) blocked credit (and Rules 38/42/43, entered by hand). */
  itc_reversed_permanent?: GstAmounts;
  /** 4(B)(2) Rule 37 reversals. */
  itc_reversed_other?: GstAmounts;
  /** 4(C) net ITC available. */
  itc_net?: GstAmounts;
}

/** GSTR-3B net row with the reverse-charge part (paid in cash). */
export interface GstNetRowRc {
  component: string;
  output: string;
  itc: string;
  net: string;
  reverse_charge?: string;
}

export interface RcmRow {
  voucher_id: string;
  kind: VoucherKind;
  number: string;
  date: string;
  party: string;
  party_gstin: string | null;
  amounts: GstAmounts;
  blocked: boolean;
  needs_self_invoice: boolean;
  /** "SI/26-27/3" */
  self_invoice: string | null;
  /** Rule 47A: 30 days from receipt. */
  self_invoice_due: string | null;
  cancelled: boolean;
}

export interface RcmRegister {
  rows: RcmRow[];
  total: GstAmounts;
  warnings: string[];
}

export interface Rule37Row {
  voucher_id: string;
  number: string;
  date: string;
  party: string;
  party_gstin: string | null;
  reference: string;
  value: string;
  unpaid: string;
  days: number;
  period_ends: string;
  reverse_from: string;
  itc: GstAmounts;
  reversal: GstAmounts;
}

export interface Rule37Report {
  as_at: string;
  rows: Rule37Row[];
  total: GstAmounts;
  warnings: string[];
}

export interface Rule42Input {
  total: string;
  non_business?: string;
  exempt_only?: string;
  blocked?: string;
  taxable_only?: string;
  exempt_turnover: string;
  total_turnover: string;
  non_business_use?: boolean;
}

export interface Rule42Result {
  c1: string;
  c2: string;
  d1: string;
  d2: string;
  c3: string;
  reverse: string;
}

export interface GstFlagRow {
  kind: "ledger" | "item";
  id: string;
  name: string;
  class: string | null;
  reverse_charge: boolean;
  blocked: boolean;
}

export interface Gstr1FiledPeriod {
  /** YYYY-MM */
  period: string;
  filed_at: string;
}

export interface GstDocSnapshot {
  number: string;
  date: string;
  party: string;
  party_gstin: string | null;
  value: string;
  taxable: string;
  tax: string;
  cancelled: boolean;
}

export interface Gstr1Amendment {
  period: string;
  filed_at: string;
  voucher_id: string;
  kind: VoucherKind;
  change: "added" | "altered" | "cancelled" | "deleted";
  changed_at: string;
  user: string;
  before: GstDocSnapshot | null;
  after: GstDocSnapshot | null;
}

export type Gstr2bStatus = "Matched" | "Mismatched" | "OnlyIn2b" | "OnlyInBooks";

export interface Gstr2bPortalDoc {
  section: "b2b" | "b2ba" | "cdnr" | "cdnra";
  supplier_gstin: string;
  supplier_name: string;
  supplier_period: string | null;
  kind: GstDocKind;
  number: string;
  date: string | null;
  value: string;
  amounts: GstAmounts;
  reverse_charge: boolean;
  itc_available: boolean;
  reason: string | null;
  original_number: string | null;
  ims_status: string | null;
}

export interface Gstr2bBookDoc {
  id: string;
  kind: GstDocKind;
  number: string;
  references: string[];
  date: string;
  supplier_gstin: string | null;
  supplier_name: string;
  value: string | null;
  amounts: GstAmounts;
}

export interface Gstr2bRow {
  status: Gstr2bStatus;
  books: Gstr2bBookDoc | null;
  portal: Gstr2bPortalDoc | null;
  differences: string[];
}

export interface Gstr2bImport {
  boe_number: string;
  boe_date: string | null;
  port: string;
  amended: boolean;
  taxable: string;
  igst: string;
  cess: string;
}

export interface Gstr2bRecon {
  gstin: string | null;
  return_period: string | null;
  generated: string | null;
  other_gstin: boolean;
  rows: Gstr2bRow[];
  summary: { status: Gstr2bStatus; count: number; amounts: GstAmounts }[];
  itc_unavailable: Gstr2bPortalDoc[];
  imports: Gstr2bImport[];
  warnings: string[];
}

// ------------------- Schedule III: divisions, rounding, notes and mapping

/** A statement row as now served: Division I or II items, and "detail" rows (MSME split). */
export interface Schedule3StatementRow extends Omit<Schedule3Row, "kind" | "item" | "ledgers"> {
  kind: "heading" | "line" | "total" | "detail";
  /** A Division I item, or a Division II (Ind AS) line code. */
  item: string | null;
  ledgers: (Schedule3Ledger & { tags: string[] })[];
}

export interface Schedule3Statement {
  division: 1 | 2;
  /** "₹", "₹ in lakhs"… */
  unit: string;
  rows: Schedule3StatementRow[];
  warnings: string[];
}

export interface Schedule3BalanceSheetV2 extends Schedule3Statement {
  as_at: string;
  previous_as_at: string | null;
  total_equity_and_liabilities: Schedule3Figures;
  total_assets: Schedule3Figures;
}

export interface Schedule3ProfitAndLossV2 extends Schedule3Statement {
  from: string;
  to: string;
  previous_from: string | null;
  previous_to: string | null;
  total_income: Schedule3Figures;
  profit_for_the_period: Schedule3Figures;
  other_comprehensive_income: Schedule3Figures;
  total_comprehensive_income: Schedule3Figures;
}

export type Schedule3Cell =
  | { kind: "empty" }
  | { kind: "amount"; value: string }
  | { kind: "ratio"; value: string }
  | { kind: "percent"; value: string }
  | { kind: "text"; value: string };

export interface Schedule3NoteRow {
  kind: "heading" | "line" | "total" | "detail";
  level: number;
  label: string;
  cells: Schedule3Cell[];
  ledger: string | null;
}

export interface Schedule3NoteTable {
  caption: string | null;
  columns: string[];
  rows: Schedule3NoteRow[];
}

export interface Schedule3Note {
  number: number;
  title: string;
  of: "balance_sheet" | "profit_and_loss" | "other";
  code: string | null;
  tables: Schedule3NoteTable[];
  remarks: string[];
}

export interface Schedule3Ratio {
  name: string;
  numerator: string;
  denominator: string;
  unit: "times" | "%";
  current: string | null;
  previous: string | null;
  variance_pct: string | null;
  explain: boolean;
  remark: string | null;
}

/** khata_in::schedule3_notes::Statements */
export interface Schedule3Statements {
  division: 1 | 2;
  rounding: string;
  unit: string;
  as_at: string;
  previous_as_at: string | null;
  balance_sheet: Schedule3BalanceSheetV2;
  profit_and_loss: Schedule3ProfitAndLossV2;
  notes: Schedule3Note[];
  ratios: Schedule3Ratio[];
  warnings: string[];
}

export interface Schedule3GroupMap {
  group: string;
  name: string;
  depth: number;
  nature: "Assets" | "Liabilities" | "Income" | "Expenses";
  /** The company's own line code for this group, if any. */
  code: string | null;
  /** Division I line (with classifications). */
  line: string;
  /** Division II line. */
  line2: string;
  inherited_from: string | null;
  fallback: boolean;
}

export interface Schedule3Party {
  ledger: string;
  name: string;
  group_name: string;
  msme: boolean;
}

export interface Schedule3Mapping {
  division: 1 | 2;
  rounding: string;
  unit: string;
  roundings: { code: string; unit: string }[];
  groups: Schedule3GroupMap[];
  parties: Schedule3Party[];
  warnings: string[];
}

export interface Schedule3LineOption {
  code: string;
  label: string;
  /** 2: a Division II (Ind AS) only line. */
  division: 1 | 2;
}

// ------------------------------------- Mexico: withholdings, payment receipts, PAC (khata-mx)

/** Fields added to the IVA summary for withholdings (declaration merging). */
export interface MxIvaSummary {
  /** IVA customers withheld (comes off what is owed). */
  iva_retenido_por_clientes: string;
  /** ISR customers withheld (for the ISR provisional payment). */
  isr_retenido_por_clientes: string;
  /** IVA withheld from suppliers: to pay over by the 17th. */
  iva_retenido_a_enterar: string;
  /** ISR withheld from suppliers: to pay over by the 17th. */
  isr_retenido_a_enterar: string;
}

/** Fields added to the CFDI options (declaration merging). */
export interface MxCfdiOptions {
  /** "RET:ISR10+IVA2/3", "RET:NONE"…; blank: the customer's class. */
  retenciones?: string;
  /** Payment receipts: cheque number, SPEI key… */
  num_operacion?: string;
  /** Payment receipts: UUIDs of invoices stamped outside KhataOS, by bill reference. */
  uuids?: Record<string, string>;
}

export interface MxPacProvider {
  id: string;
  name: string;
  /** [environment id, base URL]. */
  environments: [string, string][];
  note: string;
}

/** The PAC connection (kept in app data, 0600; never in the books). */
export interface MxPacSettings {
  rfc: string;
  provider: string;
  environment: string;
  base_url: string;
  production: boolean;
  user: string;
  has_password: boolean;
  has_token: boolean;
  providers: MxPacProvider[];
  stored_in: string;
}

export interface MxPacSettingsInput {
  provider: string;
  environment: string;
  base_url: string;
  user: string;
  /** Empty keeps the stored one. */
  password: string;
  token: string;
  clear_token: boolean;
}

export type MxCfdiStatus = "vigente" | "cancelacion_solicitada" | "cancelado";

/** A stamped CFDI (TimbreFiscalDigital) for a voucher. */
export interface MxPacStamp {
  voucher_id: string;
  voucher_number: string;
  /** "I", "E" or "P". */
  tipo: string;
  provider: string;
  environment: string;
  /** Stamped by the mock PAC: not a valid CFDI. */
  test: boolean;
  uuid: string;
  fecha_timbrado: string;
  rfc_prov_certif: string;
  leyenda: string | null;
  no_certificado_sat: string;
  sello_sat: string;
  sello_cfd: string;
  no_certificado: string;
  rfc_emisor: string;
  rfc_receptor: string;
  total: string;
  metodo_pago: string | null;
  serie: string | null;
  folio: string;
  /** The stamped CFDI XML. */
  xml: string;
  status: MxCfdiStatus;
  cancel_motivo: string | null;
  cancel_folio_sustitucion: string | null;
  cancel_code: string | null;
  cancel_requested_at: string | null;
  stamped_at: string;
  updated_at: string;
  tfd_cadena: string;
  qr_text: string;
  qr_svg: string | null;
  messages: string[];
}

// ------------------------------------- United States: use tax, exemption certificates (khata-us)

/** Field added to the sales tax totals (declaration merging). */
export interface UsSalesTaxTotals {
  /** The part of exempt_sales exempt by a customer's certificate. */
  certificate_exempt_sales: string;
}

export interface UsUseTaxRow {
  id: string;
  number: string;
  date: string;
  vendor_name: string;
  vendor_location: string;
  taxable_value: string;
  exempt_value: string;
  use_tax: string;
  /** [component, percent, amount]. */
  components: [string, string, string][];
}

export interface UsUseTaxReport {
  ein: string | null;
  state: string | null;
  period: { from: string; to: string };
  jurisdiction: string;
  rows: UsUseTaxRow[];
  taxable_value: string;
  use_tax: string;
  by_component: [string, string][];
  taxed_by_vendor: number;
  in_state_untaxed: number;
  warnings: string[];
}

export type UsCertificateKind = "Resale" | "ExemptOrganization" | "Government" | "Other";

/** A customer's resale or exemption certificate (a shared record of the company, synced). */
export interface UsCertificate {
  state: string;
  number: string;
  kind: UsCertificateKind;
  issued: string | null;
  expires: string | null;
  note: string;
}

export type UsCertificateStatus = "Expired" | "Expiring" | "Valid" | "NoExpiry" | "NotYetValid";

export interface UsCertificateRow extends UsCertificate {
  customer_id: string;
  customer_name: string;
  state_name: string;
  status: UsCertificateStatus;
  days_left: number | null;
}

export interface UsCertificateReport {
  as_of: string;
  within_days: number;
  rows: UsCertificateRow[];
  expired: number;
  expiring: number;
}

// ------------------------------------------- Peppol PINT (SG, AE), Factur-X, Access Point
// See docs/peppol.md.

/** A PINT SG or PINT AE UBL document for a Sales or Credit Note voucher. */
export interface PintDocument {
  xml: string;
  file_name: string;
  /** BT-24, e.g. "urn:peppol:pint:billing-1@sg-1". */
  customization_id: string;
  /** Participant ids ("0195:SGUEN…", "0235:…"); null when not known. */
  receiver: string | null;
  sender: string | null;
  /** Broken PINT rules: not valid until fixed. */
  errors: string[];
  warnings: string[];
}

/** A Peppol participant identifier. */
export interface PeppolParticipant {
  scheme: string;
  value: string;
  /** "scheme:value". */
  id: string;
  source: string;
}

/** A Factur-X / ZUGFeRD PDF (EN 16931) with the embedded CII XML. */
export interface FacturX {
  pdf_base64: string;
  xml: string;
  file_name: string;
  errors: string[];
  warnings: string[];
  /** What the PDF writer does not check for full PDF/A-3 / Factur-X conformance. */
  conformance_gaps: string[];
}

/** Access Point settings; the key only as "set or not". */
export interface PeppolApSettings {
  endpoint: string;
  auth_header: string;
  auth_prefix: string;
  api_key_set: boolean;
  note: string;
}

/** `api_key`: undefined/null keeps the stored key, "" clears it. */
export interface PeppolApSettingsInput {
  endpoint: string;
  api_key?: string | null;
  auth_header?: string | null;
  auth_prefix?: string | null;
}

export interface PeppolApSend {
  status: number;
  accepted: boolean;
  response: string;
  file_name: string;
  sender: string | null;
  receiver: string | null;
  document_type_id: string;
}

// ------------------------------------------- France, Netherlands, Spain VAT
// See docs/france-vat.md, docs/netherlands-vat.md, docs/spain-vat.md.

/** One CA3 rate line: base HT and tax, whole euros. */
export interface Ca3RateLine {
  base: string;
  tax: string;
}

/** France: the CA3 (khata-fr returns.rs; a preparation aid). Whole euros, never negative on the form. */
export interface Ca3 {
  /** The company's TVA number as stored. */
  tax_id: string | null;
  /** Which rate lines apply. */
  territory: "Continent" | "Corse" | "Dom";
  period: { from: string; to: string } | null;
  a1: string;
  a2: string;
  a3: string;
  b2: string;
  b5: string;
  e1: string;
  e2: string;
  f2: string;
  f8: string;
  l08: Ca3RateLine;
  l09: Ca3RateLine;
  l9b: Ca3RateLine;
  l10: Ca3RateLine;
  l11: Ca3RateLine;
  t1: Ca3RateLine;
  t2: Ca3RateLine;
  tc: Ca3RateLine;
  t3: Ca3RateLine;
  t4: Ca3RateLine;
  t5: Ca3RateLine;
  t6: Ca3RateLine;
  l13: Ca3RateLine;
  /** Rates without a line on this company's CA3 (left out of line 16). */
  other_rates: { percent: string; base: string; tax: string }[];
  l15: string;
  l16: string;
  l17: string;
  l20: string;
  l21: string;
  l23: string;
  l25: string;
  td: string;
  l27: string;
  l28: string;
  l32: string;
  warnings: string[];
}

/** Netherlands: the aangifte omzetbelasting (khata-nl returns.rs). Whole euros; `*_tax` is the btw column. */
export interface Aangifte {
  /** The company's btw-id as stored. */
  tax_id: string | null;
  period: { from: string; to: string } | null;
  r1a: string;
  r1a_tax: string;
  r1b: string;
  r1b_tax: string;
  r1c: string;
  r1c_tax: string;
  r1d: string;
  r1d_tax: string;
  r1e: string;
  r2a: string;
  r2a_tax: string;
  r3a: string;
  r3b: string;
  r3c: string;
  r4a: string;
  r4a_tax: string;
  r4b: string;
  r4b_tax: string;
  r5a: string;
  r5b: string;
  r5c: string;
  r5d: string;
  r5e: string;
  r5f: string;
  /** Positive: to pay; negative: to reclaim. */
  r5g: string;
  warnings: string[];
}

/** Spain: the modelo 303 (khata-es returns.rs). Euros and cents; fields are the casillas. */
export interface Modelo303 {
  /** The company's NIF as stored. */
  tax_id: string | null;
  period: { from: string; to: string } | null;
  c01: string;
  c03: string;
  c04: string;
  c06: string;
  c07: string;
  c09: string;
  c150: string;
  c152: string;
  c153: string;
  c155: string;
  c10: string;
  c11: string;
  c12: string;
  c13: string;
  c14: string;
  c15: string;
  c27: string;
  c28: string;
  c29: string;
  c36: string;
  c37: string;
  c40: string;
  c41: string;
  c45: string;
  c46: string;
  c59: string;
  c60: string;
  c120: string;
  c122: string;
  c64: string;
  c65: string;
  c66: string;
  c69: string;
  /** Positive: to pay; negative: to refund or carry forward. */
  c71: string;
  warnings: string[];
}

/** A VeriFactu billing record (registro de alta or anulación); generated, never sent. */
export interface VerifactuRecord {
  kind: "alta" | "anulacion";
  /** TipoFactura: "F1", "F2", "R1", "R5". */
  tipo: string;
  num_serie: string;
  xml: string;
  /** SHA-256, 64 upper-case hex characters. */
  huella: string;
  huella_anterior: string | null;
  /** Place in this installation's chain, from 1. */
  numero: number;
  /** FechaHoraHusoGenRegistro. */
  generado: string;
  /** The AEAT cotejo URL for the invoice's QR code. */
  qr_url: string;
  file_name: string;
  /** Already generated; this is the recorded one. */
  regenerated: boolean;
  warnings: string[];
}

/** The billing software (SistemaInformatico) as VeriFactu records it. */
export interface VerifactuSystem {
  sistema: {
    nombre_razon: string;
    nif: string;
    nombre_sistema: string;
    id_sistema: string;
    version: string;
    numero_instalacion: string;
    solo_verifactu: boolean;
    multi_ot: boolean;
    multiples_ot: boolean;
  };
  /** The QR code points at the AEAT's test environment. */
  pruebas: boolean;
}

// --- VAT packs: reverse charge, imports, UK schemes, ZATCA renewal and queue
// (vat_ae.rs, vat_gb.rs, gst_sg.rs, zatca.rs; docs/uae-vat.md, uk-vat.md, singapore-gst.md, ksa-zatca.md)

/** VAT 201 with the reverse-charge figures (Boxes 3, 6, 7 and 10 are on Vat201). */
export interface Vat201Rc extends Vat201 {
  /** Domestic reverse-charge supplies made (in their Box 1 row, no VAT), for reference. */
  reverse_charge_sales: string;
}

export type UkLimitedCost = "auto" | "yes" | "no";

/** The company's Flat Rate Scheme settings (Notice 733). */
export interface UkFlatRate {
  sector: string;
  /** The sector's flat rate (%), a decimal string. */
  percent: string;
  /** VAT registration date: 1% off for the first 12 months. */
  registered_on: string | null;
  limited_cost: UkLimitedCost;
  /** Cash-based turnover (§9). */
  cash_turnover: boolean;
}

export interface UkVatSchemes {
  flat_rate: UkFlatRate | null;
  cash_accounting: boolean;
  annual_accounting: boolean;
}

export interface UkFlatRateSector {
  name: string;
  percent: string;
}

export interface UkVatSchemesInfo {
  settings: UkVatSchemes;
  sectors: UkFlatRateSector[];
  limited_cost_percent: string;
}

export interface UkFlatRateSegment {
  percent: string;
  turnover: string;
  vat: string;
}

export interface UkFlatRateDetails {
  sector: string;
  sector_percent: string;
  limited_cost: boolean;
  relevant_goods: string;
  limited_cost_limit: string;
  turnover: string;
  segments: UkFlatRateSegment[];
  flat_rate_vat: string;
  capital_value: string;
  capital_vat: string;
  cash_turnover: boolean;
}

/** The UK return with the scheme it was worked out under. */
export interface UkVatReturnSchemes extends UkVatReturn {
  scheme: "standard" | "flat_rate" | "cash";
  flat_rate: UkFlatRateDetails | null;
  annual_accounting: boolean;
}

/** ZATCA status with the CSID's expiry, refusal and the queue size. */
export interface ZatcaStatusRenewal extends ZatcaStatus {
  days_to_expiry: number | null;
  /** Expires within 30 days. */
  renewal_due: boolean;
  expired: boolean;
  /** ZATCA refused the CSID (401): when and why. */
  csid_refused: string | null;
  renewed_at: string | null;
  queue_pending: number;
}

export type ZatcaQueueState = "pending" | "retrying" | "failed" | "not_generated";

export interface ZatcaQueueItem {
  voucher_id: string;
  number: string;
  date: string;
  party: string;
  /** "standard" or "simplified"; empty when not generated. */
  kind: string;
  state: ZatcaQueueState;
  generated_at: string | null;
  /** Simplified documents: generation + 24 hours (UTC). */
  deadline: string | null;
  hours_left: number | null;
  overdue: boolean;
  attempts: number;
  next_attempt: string | null;
  last_error: string | null;
}

export interface ZatcaQueue {
  items: ZatcaQueueItem[];
  pending: number;
  failed: number;
  overdue: number;
  not_generated: number;
  ready: boolean;
  notes: string[];
}

export interface ZatcaQueueRun {
  sent: number;
  accepted: number;
  refused: number;
  retry_later: number;
  stopped: string | null;
  queue: ZatcaQueue;
}

// ------------------------------------------------- foreign currency (forex.rs)

/** The foreign side of a voucher line; `amount` on the line stays the base amount. */
export interface ForexInput {
  currency: string;
  amount: string;
  rate: string;
  /** Foreign amount per bill allocation, in order; empty: shared out by base amount. */
  bills?: string[];
}

export interface ForexView {
  currency: string;
  amount: string;
  rate: string;
  bills: string[];
}

// Declaration merging: foreign currency on ledgers, voucher input and reports.
export interface Ledger {
  /** A party or bank ledger kept in a foreign currency (ISO code). Older backends omit it. */
  currency?: string | null;
}

export interface EntryInput {
  forex?: ForexInput | null;
}

export interface OutstandingBill {
  /** The bill in its foreign currency, signed like `amount`. */
  forex_currency?: string;
  forex_amount?: string;
}

export interface StatementRow {
  /** Net foreign amount of the row (debit positive). */
  forex?: string;
}

export interface LedgerStatement {
  /** A ledger kept in a foreign currency: its opening and closing balances in it. */
  forex?: { currency: string; opening: string; closing: string };
}

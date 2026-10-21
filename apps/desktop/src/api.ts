// Typed wrappers over the Tauri commands. Errors arrive as strings.
import { invoke } from "@tauri-apps/api/core";
import type {
  AgeingRow,
  ExportTable,
  ImportSummary,
  OutstandingBill,
  RebaseReport,
  ReportSpec,
  SyncResult,
  SyncSettings,
  BalanceSheet,
  Binding,
  CompanyFile,
  CompanyInfo,
  CountryInfo,
  CreatedLedger,
  DayBookRow,
  Group,
  GroupSummary,
  Ledger,
  LedgerFilter,
  LedgerStatement,
  PostedVoucher,
  TaxIdCheck,
  ProfitAndLoss,
  TrialBalance,
  VoucherInput,
  VoucherKind,
  VoucherView,
  BankReconciliation,
  Dashboard,
  LedgerInput,
  RatioAnalysis,
  StockGroup,
  StockItem,
  StockItemInput,
  StockItemVouchers,
  StockSummary,
  Unit,
  CompanyTax,
  Godown,
  InvoiceTax,
  InvoiceTaxInput,
  LocationRow,
  Numbering,
  Restart,
  BankStatementRow,
  CompanyUserRow,
  Session,
  UserRole,
  TaxRate,
  TaxSide,
} from "./types";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
  }
}

export const api = {
  keymap: () => call<Binding[]>("keymap"),
  reportError: (message: string) => call<void>("report_error", { message }),
  defaultCompanyDir: () => call<string>("default_company_dir"),
  listCompanyFiles: (dir: string) => call<CompanyFile[]>("list_company_files", { dir }),
  companyFileName: (name: string) => call<string>("company_file_name", { name }),
  createCompany: (path: string, name: string) => call<CompanyInfo>("create_company", { path, name }),
  openCompany: (path: string) => call<CompanyInfo>("open_company", { path }),
  closeCompany: () => call<void>("close_company"),
  companyInfo: () => call<CompanyInfo>("company_info"),
  searchGroups: (query: string) => call<Group[]>("search_groups", { query }),
  listLedgers: (query: string, filter: LedgerFilter) => call<Ledger[]>("list_ledgers", { query, filter }),
  listCountries: () => call<CountryInfo[]>("list_countries"),
  setCountry: (code: string) => call<CompanyInfo>("set_country", { code }),
  checkTaxId: (value: string) => call<TaxIdCheck>("check_tax_id", { value }),
  createLedger: (
    name: string,
    group_id: string,
    opening: string,
    credit_days: number | null = null,
    tax_id: string | null = null,
    tax_id_override: string | null = null,
  ) => call<CreatedLedger>("create_ledger", { name, group_id, opening, credit_days, tax_id, tax_id_override }),
  postVoucher: (voucher: VoucherInput) => call<PostedVoucher>("post_voucher", { voucher }),
  getVoucher: (id: string) => call<VoucherView>("get_voucher", { id }),
  /** Next number in the type's series for a voucher dated `date` (today when omitted). */
  nextVoucherNumber: (kind: VoucherKind, date: string | null = null) => call<string>("next_voucher_number", { kind, date }),
  dayBook: (from: string, to: string) => call<DayBookRow[]>("day_book", { from, to }),
  ledgerStatement: (ledger_id: string, from: string, to: string) =>
    call<LedgerStatement>("ledger_statement", { ledger_id, from, to }),
  trialBalance: (as_at: string | null) => call<TrialBalance>("trial_balance", { as_at }),
  profitAndLoss: (from: string, to: string) => call<ProfitAndLoss>("profit_and_loss", { from, to }),
  balanceSheet: (as_at: string) => call<BalanceSheet>("balance_sheet", { as_at }),
  groupSummary: (group_id: string, from: string | null, to: string) =>
    call<GroupSummary>("group_summary", { group_id, from, to }),
  quitApp: () => call<void>("quit_app"),
  createGroup: (name: string, parent_id: string) => call<Group>("create_group", { name, parent_id }),
  alterVoucher: (id: string, voucher: VoucherInput) => call<PostedVoucher>("alter_voucher", { id, voucher }),
  deleteVoucher: (id: string) => call<void>("delete_voucher", { id }),
  cancelVoucher: (id: string) => call<void>("cancel_voucher", { id }),
  outstanding: (as_at: string, side: "receivable" | "payable" | "all", ledger_id: string | null = null) =>
    call<OutstandingBill[]>("outstanding", { as_at, side, ledger_id }),
  ageing: (as_at: string, receivable: boolean) => call<AgeingRow[]>("ageing", { as_at, receivable }),
  overduePayables: (as_at: string, days: number) => call<OutstandingBill[]>("overdue_payables", { as_at, days }),
  importTally: (paths: string[]) => call<ImportSummary>("import_tally", { paths }),
  syncSettings: () => call<SyncSettings>("sync_settings"),
  setSyncSettings: (url: string, company_id: string, token: string) =>
    call<SyncSettings>("set_sync_settings", { url, company_id, token }),
  syncNow: () => call<SyncResult>("sync_now"),
  rebase: () => call<RebaseReport>("rebase"),
  /** `language`: the print's language ("ar" prints right to left in Arabic); English when left out. */
  printReport: (spec: ReportSpec, language?: string) => call<void>("print_report", { spec: { ...spec, language } }),
  takePrintHtml: () => call<string | null>("take_print_html"),
  printWindow: () => call<void>("print_window"),
  reportCsv: (spec: ReportSpec) => call<string>("report_csv", { spec }),
  writeCsv: (path: string, contents: string) => call<void>("write_csv", { path, contents }),
  /** Save a Tally import report (full or counts only) as .csv or .txt. */
  saveImportReport: (path: string, contents: string) => call<void>("save_import_report", { path, contents }),

  // Alter / delete masters
  getGroup: (id: string) => call<Group>("get_group", { id }),
  alterGroup: (id: string, name: string, parent_id: string, aliases: string[]) =>
    call<Group>("alter_group", { id, name, parent_id, aliases }),
  deleteGroup: (id: string) => call<void>("delete_group", { id }),
  getLedger: (id: string) => call<Ledger>("get_ledger", { id }),
  alterLedger: (id: string, ledger: LedgerInput) => call<CreatedLedger>("alter_ledger", { id, ledger }),
  deleteLedger: (id: string) => call<void>("delete_ledger", { id }),

  // Inventory masters (id "" creates)
  listUnits: () => call<Unit[]>("list_units"),
  saveUnit: (id: string, symbol: string, name: string, decimals: number) =>
    call<Unit>("save_unit", { id, symbol, name, decimals }),
  deleteUnit: (id: string) => call<void>("delete_unit", { id }),
  listStockGroups: (query: string) => call<StockGroup[]>("list_stock_groups", { query }),
  saveStockGroup: (id: string, name: string, parent_id: string) =>
    call<StockGroup>("save_stock_group", { id, name, parent_id }),
  deleteStockGroup: (id: string) => call<void>("delete_stock_group", { id }),
  listStockItems: (query: string) => call<StockItem[]>("list_stock_items", { query }),
  getStockItem: (id: string) => call<StockItem>("get_stock_item", { id }),
  saveStockItem: (item: StockItemInput) => call<StockItem>("save_stock_item", { item }),
  deleteStockItem: (id: string) => call<void>("delete_stock_item", { id }),

  // Inventory reports, banking, analysis
  stockSummary: (from: string, to: string) => call<StockSummary>("stock_summary", { from, to }),
  stockItemVouchers: (item_id: string, from: string, to: string) =>
    call<StockItemVouchers>("stock_item_vouchers", { item_id, from, to }),
  bankReconciliation: (ledger_id: string, from: string, to: string) =>
    call<BankReconciliation>("bank_reconciliation", { ledger_id, from, to }),
  /** Empty date clears the bank date. */
  setBankDate: (voucher_id: string, ledger_id: string, date: string) =>
    call<void>("set_bank_date", { voucher_id, ledger_id, date }),
  ratioAnalysis: (from: string, to: string) => call<RatioAnalysis>("ratio_analysis", { from, to }),
  dashboard: (from: string, to: string) => call<Dashboard>("dashboard", { from, to }),

  // Voucher numbering (Voucher Type alteration)
  listNumbering: () => call<Numbering[]>("list_numbering"),
  setNumbering: (kind: VoucherKind, restart: Restart, prefix: string, suffix: string, width: number) =>
    call<Numbering>("set_numbering", { kind, restart, prefix, suffix, width }),

  // Godowns and stock by location (id "" creates)
  listGodowns: (query: string) => call<Godown[]>("list_godowns", { query }),
  saveGodown: (id: string, name: string, parent_id: string) => call<Godown>("save_godown", { id, name, parent_id }),
  deleteGodown: (id: string) => call<void>("delete_godown", { id }),
  stockByLocation: (as_at: string) => call<LocationRow[]>("stock_by_location", { as_at }),
  /** Last sale (or purchase) rate of an item, same party first; "" when never traded. */
  lastRate: (item_id: string, kind: VoucherKind, party_id: string) => call<string>("last_rate", { item_id, kind, party_id }),
  lastNarration: (kind: VoucherKind | null, ledger_id: string) => call<string>("last_narration", { kind, ledger_id }),

  // Tax: company registration, rate list, duty ledgers, invoice tax
  companyTax: () => call<CompanyTax>("company_tax"),
  setCompanyTax: (tax_id: string, region: string) => call<CompanyTax>("set_company_tax", { tax_id, region }),
  listTaxRates: () => call<TaxRate[]>("list_tax_rates"),
  /** Empty percent removes that rate change. */
  setTaxRate: (tax_class: string, from: string, percent: string) =>
    call<TaxRate[]>("set_tax_rate", { class: tax_class, from, percent }),
  /** Empty component clears the ledger's tax type. */
  setLedgerTax: (ledger_id: string, component: string, side: TaxSide) =>
    call<void>("set_ledger_tax", { ledger_id, component, side }),
  invoiceTax: (input: InvoiceTaxInput) => call<InvoiceTax>("invoice_tax", { input }),
  setCompanyAddress: (address: string) => call<CompanyTax>("set_company_address", { address }),
  /** HSN/SAC digits invoices must show: 4, 6 or 8; null unsets. */
  setHsnDigits: (digits: number | null) => call<CompanyTax>("set_hsn_digits", { digits }),
  /** Open the print window with a saved voucher as an invoice / note / voucher. */
  /** `hijri`: the voucher date in the Hijri calendar, already formatted (Rust has no Hijri calendar). */
  printVoucher: (id: string, language?: string, hijri?: string | null) =>
    call<void>("print_voucher", { id, language, hijri: hijri ?? null }),
  /** Open the print window with a saved bank Payment as a cheque. */
  printCheque: (id: string, account_payee: boolean) => call<void>("print_cheque", { id, account_payee }),

  // Users and roles (security control)
  sessionInfo: () => call<Session>("session_info"),
  signIn: (name: string, password: string) => call<Session>("sign_in", { name, password }),
  signOut: () => call<void>("sign_out"),
  listUsers: () => call<CompanyUserRow[]>("list_users"),
  /** Empty password keeps the current one; the first user must be an Owner. */
  saveUser: (name: string, role: UserRole, password: string, disabled: boolean) =>
    call<CompanyUserRow[]>("save_user", { name, role, password, disabled }),
  removeUser: (name: string) => call<CompanyUserRow[]>("remove_user", { name }),

  // Bank statement import (CSV) and matching
  readTextFile: (path: string) => call<string>("read_text_file", { path }),
  matchStatement: (ledger_id: string, csv: string) => call<BankStatementRow[]>("match_statement", { ledger_id, csv }),
  /** Record bank dates for confirmed matches, all or nothing; returns how many. */
  applyBankDates: (ledger_id: string, dates: { voucher_id: string; date: string }[]) =>
    call<number>("apply_bank_dates", { ledger_id, dates }),

  // GST returns (GSTR-1 sections, GSTR-3B summary, draft GSTR-1 JSON)
  gstr1: (from: string, to: string) => call<Gstr1>("gstr1", { from, to }),
  /** Draft in the offline-tool shape; not validated against the portal schema. */
  gstr1Json: (from: string, to: string) => call<string>("gstr1_json", { from, to }),
  gstr3b: (from: string, to: string) => call<Gstr3b>("gstr3b", { from, to }),
  writeGstJson: (path: string, contents: string) => call<void>("write_gst_json", { path, contents }),

  // UAE VAT 201 (a preparation aid; see docs/uae-vat.md for what is not modelled)
  vat201: (from: string, to: string) => call<Vat201>("vat201", { from, to }),

  // Schedule III (Companies Act 2013, Division I); empty/null previous dates: no comparative
  schedule3BalanceSheet: (as_at: string, previous_as_at: string | null) =>
    call<Schedule3BalanceSheet>("schedule3_balance_sheet", { as_at, previous_as_at }),
  schedule3ProfitAndLoss: (from: string, to: string, prev_from: string | null, prev_to: string | null) =>
    call<Schedule3ProfitAndLoss>("schedule3_profit_and_loss", { from, to, prev_from, prev_to }),
  printSchedule3BalanceSheet: (as_at: string, previous_as_at: string | null) =>
    call<void>("print_schedule3_balance_sheet", { as_at, previous_as_at }),
  printSchedule3ProfitAndLoss: (from: string, to: string, prev_from: string | null, prev_to: string | null) =>
    call<void>("print_schedule3_profit_and_loss", { from, to, prev_from, prev_to }),

  // e-Invoice (INV-01) and e-way bill JSON for the portals' offline/bulk tools; nothing is sent
  // to an IRP. hsn_digits: 4 up to ₹5 cr AATO, 6 above. Save the result with writeGstJson.
  einvoiceJson: (voucher_id: string, buyer: EinvoiceBuyerDetails, hsn_digits: HsnDigits) =>
    call<string>("einvoice_json", { voucher_id, buyer, hsn_digits }),
  /** B2B, SEZ and export Sales/Credit Notes of a period; `buyers` is keyed by party ledger id. */
  einvoiceBulkJson: (from: string, to: string, buyers: Record<string, EinvoiceBuyerDetails>, hsn_digits: HsnDigits) =>
    call<string>("einvoice_bulk_json", { from, to, buyers, hsn_digits }),
  ewaybillJson: (voucher_id: string, details: EwayDetails, hsn_digits: HsnDigits) =>
    call<string>("ewaybill_json", { voucher_id, details, hsn_digits }),
};

export type { ExportTable };

import type { Gstr1, Gstr3b } from "./types";
import type { Vat201 } from "./types";
import type { Schedule3BalanceSheet, Schedule3ProfitAndLoss } from "./types";
import type { EinvoiceBuyerDetails, EwayDetails, HsnDigits } from "./types";

import type { LedgerTds, TdsNature, TdsPayable, TdsQuarterly, TdsSuggestion } from "./types";

/** TDS (India): natures, the voucher suggestion, reports and ledger settings. See docs/india-tds.md. */
export const tdsApi = {
  natures: () => call<TdsNature[]>("tds_natures"),
  /** `voucher_id`: the voucher being altered (left out of the year's totals); null for a new one. */
  suggest: (date: string, party_id: string, expense_ledger_id: string, amount: string, voucher_id: string | null = null) =>
    call<TdsSuggestion>("tds_for", { date, party_id, expense_ledger_id, amount, voucher_id }),
  payable: (from: string, to: string) => call<TdsPayable>("tds_payable", { from, to }),
  /** `year`: the year the financial year starts in (2026 for 2026-27); quarter 1 to 4. */
  quarterly: (year: number, quarter: number) => call<TdsQuarterly>("tds_quarterly", { year, quarter }),
  /** Statement data as CSV (not the FVU file); save it with api.writeCsv. */
  quarterlyCsv: (year: number, quarter: number) => call<string>("tds_quarterly_csv", { year, quarter }),
  /** Replace the ledger's PAN and TDS settings; null clears each. */
  setLedgerTds: (id: string, pan: string | null, tds: LedgerTds | null) => call<Ledger>("set_ledger_tds", { id, pan, tds }),
};

import type { GstF5 } from "./types";

/** Singapore GST (a preparation aid; see docs/singapore-gst.md for what is not modelled). */
export const sgApi = {
  gstF5: (from: string, to: string) => call<GstF5>("gst_f5", { from, to }),
};

import type { UkVatReturn } from "./types";

/** United Kingdom VAT (a preparation aid; see docs/uk-vat.md for what is not modelled). */
export const gbApi = {
  vatReturn: (from: string, to: string) => call<UkVatReturn>("vat_return_gb", { from, to }),
  /** The MTD VAT API body for the period (not submitted; `finalised` is false). Save it with api.writeGstJson. */
  mtdVatJson: (from: string, to: string, period_key: string) => call<string>("mtd_vat_json", { from, to, period_key }),
};

import type { KsaVatReturn, ZatcaInvoice, ZatcaSeller, ZatcaSellerInfo } from "./types";

/** Saudi Arabia: ZATCA e-invoices and the VAT return (see docs/ksa-zatca.md for what is not modelled). */
export const saApi = {
  /** The e-invoice XML for a Sales, Credit Note or customer Debit Note voucher. The first call takes the
   *  device's next counter and hash; later calls return the same document. Save it with api.writeGstJson. */
  invoiceXml: (voucher_id: string, kind: "standard" | "simplified") => call<ZatcaInvoice>("zatca_invoice_xml", { voucher_id, kind }),
  vatReturn: (from: string, to: string) => call<KsaVatReturn>("ksa_vat_return", { from, to }),
  seller: () => call<ZatcaSellerInfo>("zatca_seller"),
  setSeller: (seller: ZatcaSeller) => call<ZatcaSellerInfo>("set_zatca_seller", { seller }),
};

import type {
  MtdClientInfo,
  MtdEnvironment,
  MtdFeedback,
  MtdHeaderCheck,
  MtdLiability,
  MtdObligation,
  MtdPayment,
  MtdReceipt,
  MtdSettings,
  MtdViewedReturn,
} from "./types";

/** United Kingdom: Making Tax Digital for VAT through HMRC's API (see docs/uk-vat.md, "Submitting to HMRC").
 *  Every HMRC call takes the screens and window for the fraud-prevention headers. `test_scenario` is a
 *  sandbox Gov-Test-Scenario (ignored in production). */
export const mtdApi = {
  settings: () => call<MtdSettings>("mtd_settings"),
  /** An empty secret keeps the saved one; port 0 is the default. */
  setSettings: (environment: MtdEnvironment, client_id: string, client_secret: string, redirect_port: number) =>
    call<MtdSettings>("set_mtd_settings", { environment, client_id, client_secret, redirect_port }),
  /** Opens HMRC's sign-in in the browser and resolves once the user has granted access (up to 5 minutes). */
  connect: () => call<MtdSettings>("mtd_connect"),
  disconnect: () => call<MtdSettings>("mtd_disconnect"),
  /** status "O" needs no dates; otherwise give from and to (at most 366 days). */
  obligations: (from: string, to: string, status: "O" | "F" | null, client_info: MtdClientInfo, test_scenario: string | null = null) =>
    call<MtdObligation[]>("mtd_obligations", { from, to, status, client_info, test_scenario }),
  /** Sends the books' return for an open obligation with finalised true; refused unless declaration_accepted. */
  submit: (period_key: string, declaration_accepted: boolean, client_info: MtdClientInfo, test_scenario: string | null = null) =>
    call<MtdReceipt>("mtd_submit", { period_key, declaration_accepted, client_info, test_scenario }),
  viewReturn: (period_key: string, client_info: MtdClientInfo) => call<MtdViewedReturn>("mtd_view_return", { period_key, client_info }),
  liabilities: (from: string, to: string, client_info: MtdClientInfo, test_scenario: string | null = null) =>
    call<MtdLiability[]>("mtd_liabilities", { from, to, client_info, test_scenario }),
  payments: (from: string, to: string, client_info: MtdClientInfo, test_scenario: string | null = null) =>
    call<MtdPayment[]>("mtd_payments", { from, to, client_info, test_scenario }),
  /** Sandbox only: HMRC's verdict on this device's fraud-prevention headers. */
  testHeaders: (client_info: MtdClientInfo) => call<MtdHeaderCheck>("mtd_test_headers", { client_info }),
  /** Sandbox only: HMRC's feedback on the latest call to each VAT endpoint. */
  headerFeedback: () => call<MtdFeedback[]>("mtd_header_feedback"),
};

import type { EinvoiceBuyerDetails as IrpBuyer, IrnResult, IrpCancelReason, IrpEwbDetails, IrpGstinDetails, IrpSettings, IrpSettingsInput } from "./types";

/** India: the IRP connector (docs/einvoice.md). Calls go to the IRP set in IRP settings; credentials stay outside the books. */
export const irpApi = {
  settings: () => call<IrpSettings>("irp_settings"),
  setSettings: (settings: IrpSettingsInput) => call<IrpSettings>("set_irp_settings", { settings }),
  /** The stored IRN for a voucher, without calling the IRP; null when none. */
  record: (voucher_id: string) => call<IrnResult | null>("irp_record", { voucher_id }),
  /** Register a Sales or Credit Note voucher; one with an active IRN is not sent again. */
  generate: (voucher_id: string, buyer: IrpBuyer | null = null) => call<IrnResult>("irp_generate", { voucher_id, buyer }),
  /** Whole document, within 24 hours of the acknowledgement; remark up to 100 characters. */
  cancel: (voucher_id: string, reason: IrpCancelReason, remark: string) => call<IrnResult>("irp_cancel", { voucher_id, reason, remark }),
  details: (voucher_id: string) => call<IrnResult>("irp_details", { voucher_id }),
  ewaybill: (voucher_id: string, details: IrpEwbDetails) => call<IrnResult>("irp_ewaybill", { voucher_id, details }),
  gstin: (gstin: string) => call<IrpGstinDetails>("irp_gstin", { gstin }),
};


import type { MxCfdi, MxCfdiOptions, MxCsdStatus, MxIssuer, MxIvaSummary, Us1099Report, UsSalesTaxReport } from "./types";

/** United States: sales tax liability by jurisdiction and the Form 1099 list (see docs/us-sales-tax.md). */
export const usApi = {
  salesTaxReport: (from: string, to: string) => call<UsSalesTaxReport>("sales_tax_report_us", { from, to }),
  form1099: (year: number) => call<Us1099Report>("form1099_us", { year }),
};

/** Mexico: IVA summary and CFDI 4.0 XML (sealed with the CSD, never stamped; see docs/mexico-cfdi.md). */
export const mxApi = {
  ivaSummary: (from: string, to: string) => call<MxIvaSummary>("iva_summary_mx", { from, to }),
  issuer: () => call<MxIssuer>("mx_issuer"),
  setIssuer: (issuer: MxIssuer) => call<MxIssuer>("set_mx_issuer", { issuer }),
  csdStatus: () => call<MxCsdStatus>("mx_csd_status"),
  /** Copies the .cer and encrypted .key to the app data directory; the password is only checked, never stored. */
  setCsd: (cer_path: string, key_path: string, password: string) => call<MxCsdStatus>("set_mx_csd", { cer_path, key_path, password }),
  clearCsd: () => call<MxCsdStatus>("clear_mx_csd"),
  /** Save the result's xml with api.writeGstJson (".xml"). */
  cfdiXml: (voucher_id: string, options: MxCfdiOptions) => call<MxCfdi>("cfdi_xml", { voucher_id, options }),
};

import type { ZatcaEnvironment, ZatcaOnboardOptions, ZatcaStampedInvoice, ZatcaStatus, ZatcaSubmission } from "./types";

/** Saudi Arabia: ZATCA onboarding, the stamp, clearance and reporting (docs/ksa-zatca.md). */
export const zatcaApi = {
  status: () => call<ZatcaStatus>("zatca_status"),
  /** The OTP is from the Fatoora portal (sandbox: 123345); empty to resume after a failed step. */
  onboard: (environment: ZatcaEnvironment, otp: string, options?: Partial<ZatcaOnboardOptions>) =>
    call<ZatcaStatus>("zatca_onboard", {
      environment,
      otp,
      options: { branch: "", location: "", industry: "", invoice_types: "", restart: false, ...options },
    }),
  /** Stamp if needed, then clearance (standard) or reporting (simplified). `kind` only matters the first time. */
  submit: (voucher_id: string, kind?: "standard" | "simplified") => call<ZatcaSubmission>("zatca_submit", { voucher_id, kind: kind ?? null }),
  submission: (voucher_id: string) => call<ZatcaSubmission | null>("zatca_submission", { voucher_id }),
  /** Like saApi.invoiceXml, stamped once the device is onboarded. */
  invoiceXml: (voucher_id: string, kind: "standard" | "simplified") => call<ZatcaStampedInvoice>("zatca_invoice_xml_stamped", { voucher_id, kind }),
};

import type { Ustva, XRechnung } from "./types";

/** Germany: UStVA and XRechnung (preparation aids; see docs/germany-vat.md for what is not modelled). */
export const deApi = {
  ustva: (from: string, to: string) => call<Ustva>("ustva", { from, to }),
  /** XRechnung 3.0 XML for a Sales or Credit Note voucher. Save it with api.writeGstJson. */
  xrechnungXml: (voucher_id: string, syntax: "ubl" | "cii") => call<XRechnung>("xrechnung_xml", { voucher_id, syntax }),
};

import type {
  CodeName,
  LedgerTcs,
  NonResidentQuarterly,
  SalaryEstimate,
  SalaryEstimateInput,
  TcsNature,
  TcsQuarterly,
  TcsSuggestion,
  TdsChallanDetails,
  TdsDeductor,
  TdsDeductorSettings,
  TdsStatementFile,
} from "./types";

/** TCS (India): natures, the voucher suggestion, reports and ledger settings. Payable rows reuse TDS shapes. */
export const tcsApi = {
  natures: () => call<TcsNature[]>("tcs_natures"),
  /** `amount`: what the buyer pays (GST included, see docs/india-tds.md). */
  suggest: (date: string, party_id: string, sales_ledger_id: string, amount: string) =>
    call<TcsSuggestion>("tcs_for", { date, party_id, sales_ledger_id, amount }),
  payable: (from: string, to: string) => call<TdsPayable>("tcs_payable", { from, to }),
  quarterly: (year: number, quarter: number) => call<TcsQuarterly>("tcs_quarterly", { year, quarter }),
  ledgerTcs: (id: string) => call<LedgerTcs | null>("ledger_tcs", { id }),
  /** Replace the ledger's TCS settings; null clears them. */
  setLedgerTcs: (id: string, tcs: LedgerTcs | null) => call<LedgerTcs | null>("set_ledger_tcs", { id, tcs }),
};

/** TDS/TCS statements (India): Form 144 data, deductor and challan details, the text file, salary estimate. */
export const tdsStatementApi = {
  nonResident: (year: number, quarter: number) => call<NonResidentQuarterly>("tds_quarterly_nr", { year, quarter }),
  /** [countries, natures of remittance] for Form 144. */
  nrCodes: () => call<[CodeName[], CodeName[]]>("tds_nr_codes"),
  deductor: () => call<TdsDeductor>("tds_deductor"),
  setDeductor: (settings: TdsDeductorSettings) => call<TdsDeductor>("set_tds_deductor", { settings }),
  challan: (voucher_id: string) => call<TdsChallanDetails | null>("tds_challan", { voucher_id }),
  setChallan: (voucher_id: string, details: TdsChallanDetails | null) =>
    call<TdsChallanDetails | null>("set_tds_challan", { voucher_id, details }),
  /** `form`: "140", "143" or "144". */
  file: (form: "140" | "143" | "144", year: number, quarter: number, previous_token: string | null = null) =>
    call<TdsStatementFile>("tds_statement_file", { form, year, quarter, previous_token }),
  writeFile: (path: string, contents: string) => call<void>("write_tds_file", { path, contents }),
  salaryEstimate: (input: SalaryEstimateInput) => call<SalaryEstimate>("salary_estimate", { input }),
};

// GST (India): reverse charge, blocked credit, Rule 37/42, GSTR-2B, GSTR-1A.
import type {
  GstFlagRow,
  Gstr1Amendment,
  Gstr1FiledPeriod,
  Gstr2bRecon,
  RcmRegister,
  Rule37Report,
  Rule42Input,
  Rule42Result,
} from "./types";

export const gstAdvApi = {
  rcmRegister: (from: string, to: string) => call<RcmRegister>("rcm_register", { from, to }),
  printSelfInvoice: (id: string) => call<void>("print_self_invoice", { id }),
  selfInvoiceHtml: (id: string) => call<string>("self_invoice_html", { id }),
  rule37: (as_at: string) => call<Rule37Report>("rule37_report", { as_at }),
  rule42: (input: Rule42Input) => call<Rule42Result>("rule42_calc", { input }),
  flags: () => call<GstFlagRow[]>("gst_flag_list"),
  setFlags: (kind: "ledger" | "item", id: string, reverse_charge: boolean, blocked: boolean) =>
    call<GstFlagRow>("set_gst_flags", { kind, id, reverse_charge, blocked }),
  /** `period`: YYYY-MM. `filed` false clears the mark. */
  markGstr1Filed: (period: string, filed = true) => call<Gstr1FiledPeriod[]>("gstr1_mark_filed", { period, filed }),
  gstr1FiledPeriods: () => call<Gstr1FiledPeriod[]>("gstr1_filed_periods"),
  gstr1Amendments: () => call<Gstr1Amendment[]>("gstr1_amendments"),
  gstr2bReconcile: (json_text: string, from: string, to: string, days: number | null = null, amount: string | null = null) =>
    call<Gstr2bRecon>("gstr2b_reconcile", { json_text, from, to, days, amount }),
  /** The GSTR-2B .json (`readTextFile` takes .csv/.txt only). */
  readGstr2bFile: (path: string) => call<string>("read_gstr2b_file", { path }),
};

import type { Gstr9, Gstr9DocRef, Gstr9Recon } from "./types";

/** GSTR-9, India's annual GST return (src-tauri gstr9.rs). `year`: the year the financial year starts (2025 for 2025-26). */
export const gstr9Api = {
  get: (year: number) => call<Gstr9>("gstr9", { year }),
  /** Column key → amount as typed; null (or every value blank) clears the row's entries. */
  setRow: (year: number, row: string, values: Record<string, string> | null) => call<Gstr9>("gstr9_set_row", { year, row, values }),
  /** The vouchers behind a row ("4B") or an HSN line's key. */
  docs: (year: number, key: string) => call<Gstr9DocRef[]>("gstr9_docs", { year, key }),
  /** GSTN JSON layout; needs validation in the GST offline tool. Save it with api.writeGstJson. */
  json: (year: number) => call<string>("gstr9_json", { year }),
  /** One table as CSV ("4" … "19", "9C-5", "9C-7", "all"). Save it with api.writeCsv. */
  csv: (year: number, table: string) => call<string>("gstr9_csv", { year, table }),
  /** GSTR-1 Table 12 (HSN summary) for a period as CSV; `which` "b2b", "b2c" or "all". */
  gstr1HsnCsv: (from: string, to: string, which: string) => call<string>("gstr1_hsn_csv", { from, to, which }),
  reconcile: (year: number, files: { name: string; text: string }[], tolerance: string | null = null) =>
    call<Gstr9Recon>("gstr9_reconcile", { year, files, tolerance }),
  /** A return .json the user picked (GSTR-9, GSTR-1, GSTR-3B or GSTR-2B). */
  readFile: (path: string) => call<string>("read_gst_return_file", { path }),
};

import type { Schedule3LineOption, Schedule3Mapping, Schedule3Statements } from "./types";

/** Schedule III notes, mapping, division and rounding (khata_in::schedule3_notes). */
export const schedule3Api = {
  statements: (as_at: string, previous_as_at: string | null) =>
    call<Schedule3Statements>("schedule3_statements", { as_at, previous_as_at }),
  printNotes: (as_at: string, previous_as_at: string | null) => call<void>("print_schedule3_notes", { as_at, previous_as_at }),
  mapping: () => call<Schedule3Mapping>("schedule3_mapping"),
  lineOptions: (group_id: string) => call<Schedule3LineOption[]>("schedule3_line_options", { group_id }),
  /** `code` null restores the default mapping. */
  setMapping: (group_id: string, code: string | null) => call<Schedule3Mapping>("set_schedule3_mapping", { group_id, code }),
  setMsme: (ledger_id: string, msme: boolean) => call<Schedule3Mapping>("set_schedule3_msme", { ledger_id, msme }),
  setFormat: (division: 1 | 2, rounding: string) => call<Schedule3Mapping>("set_schedule3_format", { division, rounding }),
};

import type {
  MxCfdi as MxCfdiDoc,
  MxCfdiOptions as MxCfdiOpts,
  MxPacSettings,
  MxPacSettingsInput,
  MxPacStamp,
  UsCertificate,
  UsCertificateReport,
  UsUseTaxReport,
} from "./types";

/** Mexico: payment receipts and PAC timbrado/cancellation (docs/mexico-cfdi.md). */
export const mxPacApi = {
  /** The CFDI de tipo P (Complemento de Pagos 2.0) for a Receipt voucher. */
  pagoXml: (voucher_id: string, options: MxCfdiOpts) => call<MxCfdiDoc>("pago_xml", { voucher_id, options }),
  settings: () => call<MxPacSettings>("pac_settings"),
  setSettings: (settings: MxPacSettingsInput) => call<MxPacSettings>("set_pac_settings", { settings }),
  record: (voucher_id: string) => call<MxPacStamp | null>("pac_record", { voucher_id }),
  /** Seals with the CSD (options.password) and sends to the PAC. */
  stamp: (voucher_id: string, options: MxCfdiOpts) => call<MxPacStamp>("pac_stamp", { voucher_id, options }),
  cancel: (voucher_id: string, motivo: string, folio_sustitucion: string, password: string) =>
    call<MxPacStamp>("pac_cancel", { voucher_id, motivo, folio_sustitucion, password }),
  confirmCancellation: (voucher_id: string, cancelled: boolean) => call<MxPacStamp>("pac_confirm_cancellation", { voucher_id, cancelled }),
};

/** United States: use tax and exemption certificates (docs/us-sales-tax.md). */
export const usExtraApi = {
  useTaxReport: (from: string, to: string, jurisdiction?: string) =>
    call<UsUseTaxReport>("use_tax_report_us", { from, to, jurisdiction: jurisdiction ?? null }),
  certificates: (ledger_id: string) => call<UsCertificate[]>("us_certificates", { ledger_id }),
  setCertificates: (ledger_id: string, certificates: UsCertificate[]) => call<UsCertificate[]>("set_us_certificates", { ledger_id, certificates }),
  certificateReport: (as_of: string, within_days: number) => call<UsCertificateReport>("certificate_report_us", { as_of, within_days }),
};

import type { FacturX, PeppolApSend, PeppolApSettings, PeppolApSettingsInput, PeppolParticipant, PintDocument } from "./types";

/** Peppol PINT (Singapore InvoiceNow, UAE), Factur-X and the generic Access Point client (docs/peppol.md). */
export const peppolApi = {
  /** PINT SG UBL for a Sales or Credit Note voucher. Save the xml with api.writeGstJson. */
  pintSgXml: (voucher_id: string) => call<PintDocument>("pint_sg_xml", { voucher_id }),
  /** PINT AE UBL for a Sales or Credit Note voucher. Save the xml with api.writeGstJson. */
  pintAeXml: (voucher_id: string) => call<PintDocument>("pint_ae_xml", { voucher_id }),
  participantId: () => call<PeppolParticipant>("peppol_participant_id"),
  /** Factur-X / ZUGFeRD PDF (EN 16931) for an EU company. Save pdf_base64 with writeBinaryFile. */
  facturxPdf: (voucher_id: string) => call<FacturX>("facturx_pdf", { voucher_id }),
  /** Write base64 PDF bytes to a path from the save dialog (".pdf" only). */
  writeBinaryFile: (path: string, contents_base64: string) => call<void>("write_binary_file", { path, contents_base64 }),
  apSettings: () => call<PeppolApSettings>("peppol_ap_settings"),
  setApSettings: (settings: PeppolApSettingsInput) => call<PeppolApSettings>("set_peppol_ap_settings", { settings }),
  /** POST the voucher's PINT document to the configured Access Point (each provider's API differs). */
  apSend: (voucher_id: string, document: "pint-sg" | "pint-ae") => call<PeppolApSend>("peppol_ap_send", { voucher_id, document }),
};

import type { Aangifte, Ca3, Modelo303, VerifactuRecord, VerifactuSystem } from "./types";

/** France: the CA3 (a preparation aid; see docs/france-vat.md). */
export const frApi = {
  ca3: (from: string, to: string) => call<Ca3>("ca3_fr", { from, to }),
};

/** Netherlands: the aangifte omzetbelasting (see docs/netherlands-vat.md). */
export const nlApi = {
  aangifte: (from: string, to: string) => call<Aangifte>("aangifte_nl", { from, to }),
};

/** Spain: the modelo 303 and VeriFactu records (see docs/spain-vat.md). Nothing is sent to the AEAT. */
export const esApi = {
  modelo303: (from: string, to: string) => call<Modelo303>("modelo_303_es", { from, to }),
  /** The voucher's record (generated once; an anulación for a cancelled voucher). Save the XML with api.writeGstJson. */
  verifactuRecord: (voucher_id: string) => call<VerifactuRecord>("verifactu_record", { voucher_id }),
  verifactuSystem: () => call<VerifactuSystem>("verifactu_system"),
  setVerifactuSystem: (settings: VerifactuSystem) => call<VerifactuSystem>("set_verifactu_system", { settings }),
};

import type { UkVatReturnSchemes, UkVatSchemes, UkVatSchemesInfo, ZatcaQueue, ZatcaQueueRun, ZatcaStatusRenewal } from "./types";

/** UK VAT schemes (Flat Rate, Cash and Annual Accounting; docs/uk-vat.md). A shared record of the company, synced with the books. */
export const gbSchemesApi = {
  schemes: () => call<UkVatSchemesInfo>("vat_gb_schemes"),
  setSchemes: (settings: UkVatSchemes) => call<UkVatSchemesInfo>("set_vat_gb_schemes", { settings }),
  /** gbApi.vatReturn with the scheme fields typed. */
  vatReturn: (from: string, to: string) => call<UkVatReturnSchemes>("vat_return_gb", { from, to }),
};

/** ZATCA: the reporting/clearance queue and CSID renewal (docs/ksa-zatca.md). */
export const zatcaQueueApi = {
  status: () => call<ZatcaStatusRenewal>("zatca_status"),
  queue: () => call<ZatcaQueue>("zatca_queue"),
  /** `all`: Submit all (pending, refused and not yet generated); else only what is due for a retry. */
  run: (all: boolean) => call<ZatcaQueueRun>("zatca_queue_run", { all }),
  renew: (otp: string, options?: { branch?: string; location?: string; industry?: string }) =>
    call<ZatcaStatusRenewal>("zatca_renew", {
      otp,
      options: options ? { branch: "", location: "", industry: "", invoice_types: "", restart: false, ...options } : null,
    }),
};

/** What the page tells the crash reporter: codes and screen class names only. */
export interface CrashContext {
  language: string;
  country: string;
  screens: string[];
}

export interface CrashSummary {
  id: string;
  kind: "panic" | "js";
  at: string;
  message: string;
}

export interface CrashPending {
  /** False after "Don't ask again". */
  ask: boolean;
  /** Newest first. */
  crashes: CrashSummary[];
  can_open_folder: boolean;
}

/** The GitHub issue as it will be filled in, and the link that fills it. */
export interface CrashReport {
  title: string;
  version: string;
  os: string;
  platform: string;
  country: string;
  language: string;
  details: string;
  url: string;
}

/** Crash reports (docs/crash-reporting.md): saved on this device, never sent by the app. */
export const crashApi = {
  pending: () => call<CrashPending>("crash_pending"),
  context: (context: CrashContext) => call<void>("crash_context", { context }),
  recordJs: (message: string, stack: string, context: CrashContext) => call<boolean>("crash_record_js", { message, stack, context }),
  /** `id`: a saved crash; null: a blank problem report. */
  reportUrl: (id: string | null, context: CrashContext) => call<CrashReport>("crash_report_url", { id, context }),
  openUrl: (url: string) => call<void>("crash_open_url", { url }),
  /** `never_ask`: "Don't ask again" (also stops saving crashes). */
  dismiss: (ids: string[], never_ask: boolean) => call<void>("crash_dismiss", { ids, never_ask }),
  openFolder: () => call<void>("crash_open_folder"),
};

// Invoice design, per-voucher print details and direct PDF export
// (src-tauri/src/invoice_design.rs, docs/invoice-design.md). Kept apart from
// api.ts like desk-api.ts. Save PDFs with api.ts's peppolApi.writeBinaryFile.
import { invoke } from "@tauri-apps/api/core";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
  }
}

export type Template = "Classic" | "Modern" | "Compact";
export type Paper = "A4" | "Letter" | "A5";
export type FontChoice = "System" | "Arial" | "Serif" | "Mono";

export interface BankDetails {
  account_name: string;
  bank_name: string;
  branch: string;
  account_number: string;
  iban: string;
  /** IFSC, sort code, routing number… (bank_labels.local_code). */
  local_code: string;
  swift: string;
}

export interface InvoiceDesign {
  template: Template;
  /** #rrggbb */
  accent: string;
  font: FontChoice;
  paper: Paper;
  receipt_for_b2c: boolean;
  show_hsn: boolean;
  show_qty_rate: boolean;
  show_amount_in_words: boolean;
  show_signatory: boolean;
  bank: BankDetails;
  terms: string;
  footer: string;
  declaration: string;
  lut_arn: string;
  /** Voucher type ("Sales") → title, where the law allows a choice. */
  titles: Record<string, string>;
}

export interface InvoiceImage {
  kind: "Png" | "Jpeg" | "Svg";
  data_url: string;
  /** An SVG whose PNG rendition (for PDFs) is still to be made here. */
  needs_raster: boolean;
  bytes: number;
}

export type TitleRule =
  | { rule: "fixed"; title: string; reason: string }
  | { rule: "choice"; choices: string[]; reason: string }
  | { rule: "free" };

export type TitleInfo = TitleRule & { kind: string; title: string; default: string };

export interface InvoiceDesignInfo {
  design: InvoiceDesign;
  logo: InvoiceImage | null;
  signature: InvoiceImage | null;
  country: string | null;
  bank_labels: { account_number: string; local_code: string | null; iban: boolean };
  titles: TitleInfo[];
  templates: { value: Template; label: string }[];
  papers: { value: Paper; label: string }[];
  fonts: { value: FontChoice; label: string }[];
  /** Toggles the country keeps on whatever the design. */
  required: { tax_class: boolean; qty_rate: boolean; signatory: boolean };
  can_edit: boolean;
  max_image_kb: number;
}

/** A voucher's ship-to and export details (nothing in the books changes). */
export interface VoucherPrintDetails {
  ship_to_name: string;
  ship_to_address: string;
  /** "" (from the party's country), "yes" or "no". */
  export: "" | "yes" | "no";
  /** India: "" (from the voucher's IGST), "lut" or "igst". */
  india_tax: "" | "lut" | "igst";
  lut_arn: string;
  port_of_loading: string;
  port_of_discharge: string;
  incoterms: string;
  destination_country: string;
  shipping_bill_no: string;
  /** YYYY-MM-DD */
  shipping_bill_date: string;
}

export interface VoucherPrintDetailsInfo {
  details: VoucherPrintDetails;
  party_abroad: boolean;
  charges_igst: boolean;
  country: string | null;
}

export interface PdfFile {
  file_name: string;
  pdf_base64: string;
  count: number;
}

export type InvoiceImageKind = "logo" | "signature";

export const invoiceApi = {
  design: () => call<InvoiceDesignInfo>("invoice_design"),
  setDesign: (design: InvoiceDesign) => call<InvoiceDesignInfo>("set_invoice_design", { design }),
  importImage: (which: InvoiceImageKind, path: string) => call<InvoiceDesignInfo>("import_invoice_image", { which, path }),
  setImageRaster: (which: InvoiceImageKind, png_base64: string) => call<InvoiceDesignInfo>("set_invoice_image_raster", { which, png_base64 }),
  removeImage: (which: InvoiceImageKind) => call<InvoiceDesignInfo>("remove_invoice_image", { which }),
  preview: (design: InvoiceDesign, voucher_id: string | null, receipt: boolean, language: string) =>
    call<string>("invoice_design_preview", { design, voucher_id, receipt, language }),
  voucherDetails: (voucher_id: string) => call<VoucherPrintDetailsInfo>("voucher_print_details", { voucher_id }),
  setVoucherDetails: (voucher_id: string, details: VoucherPrintDetails) =>
    call<VoucherPrintDetailsInfo>("set_voucher_print_details", { voucher_id, details }),
  voucherPdf: (id: string, language: string, hijri: string | null) => call<PdfFile>("export_voucher_pdf", { id, language, hijri }),
  /** `hijri`: ISO date → Hijri date (as `voucherPdf`'s), printed under each voucher's date. */
  vouchersPdf: (ids: string[], from: string, to: string, language: string, hijri: Record<string, string> | null = null) =>
    call<PdfFile>("export_vouchers_pdf", { ids, from, to, language, hijri }),
};

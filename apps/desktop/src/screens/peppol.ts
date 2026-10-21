// Peppol e-invoices for a saved Sales or Credit Note voucher (docs/peppol.md):
// - exportPintSg: PINT SG UBL for Singapore's InvoiceNow;
// - exportPintAe: PINT AE UBL for the UAE e-invoicing system;
// - exportFacturX: a Factur-X / ZUGFeRD PDF (EN 16931) for companies in the EU.
// Each saves through the save dialog. Broken rules are listed; the file is
// still saved so it can be checked with the official validators. Sending
// goes through the company's Access Point / ASP (sendViaAccessPoint, generic
// HTTP; every provider's API differs).
//
// Meant for the voucher screen's export key (Ctrl+E / Alt+E, see
// screens/einvoice.ts exportVoucher): SG → exportPintSg, AE → exportPintAe,
// EU countries → exportFacturX.
import { api, peppolApi } from "../api";
import { ltr, t } from "../i18n";
import { save } from "@tauri-apps/plugin-dialog";
import type { Shell } from "../shell";
import type { PintDocument, VoucherKind } from "../types";

/** A saved voucher to export (the shape screens/einvoice.ts passes). */
export interface PeppolVoucher {
  id: string;
  kind: VoucherKind;
  number: string;
}

function salesOrCredit(shell: Shell, v: PeppolVoucher, what: string): boolean {
  if (v.kind === "Sales" || v.kind === "CreditNote") return true;
  shell.warn(t("{what} documents are made from Sales and Credit Note vouchers.", { what }));
  return false;
}

/** Ask where to save, with the given extension forced. */
async function pickPath(fileName: string, filter: string, ext: string): Promise<string | null> {
  const path = await save({ defaultPath: fileName, filters: [{ name: filter, extensions: [ext] }] });
  if (!path) return null;
  return path.toLowerCase().endsWith(`.${ext}`) ? path : `${path}.${ext}`;
}

async function savePint(shell: Shell, what: string, doc: PintDocument): Promise<void> {
  const file = await pickPath(doc.file_name, "XML", "xml");
  if (!file) return;
  await api.writeGstJson(file, doc.xml);
  if (doc.errors.length) {
    shell.warn(t("Saved to {file}, but it is not a valid {what} document yet: {errors}", { file: ltr(file), what, errors: doc.errors.join(" ") }));
  } else {
    const to = doc.receiver ? t(" Receiver: {id}.", { id: ltr(doc.receiver) }) : "";
    shell.info(t("{what} saved to {file}.{to} Validate it with your Access Point before sending.", { what, file: ltr(file), to }), 8000);
  }
  if (doc.warnings.length) shell.warn(doc.warnings.join(" "));
}

/** PINT SG (InvoiceNow) UBL for a Singapore company's Sales or Credit Note voucher. */
export async function exportPintSg(shell: Shell, v: PeppolVoucher): Promise<void> {
  if (!salesOrCredit(shell, v, "PINT SG")) return;
  try {
    await savePint(shell, "PINT SG", await shell.transition(peppolApi.pintSgXml(v.id)));
  } catch (e) {
    shell.error(e);
  }
}

/** PINT AE UBL for a UAE company's Sales or Credit Note voucher (hand it to your ASP). */
export async function exportPintAe(shell: Shell, v: PeppolVoucher): Promise<void> {
  if (!salesOrCredit(shell, v, "PINT AE")) return;
  try {
    await savePint(shell, "PINT AE", await shell.transition(peppolApi.pintAeXml(v.id)));
  } catch (e) {
    shell.error(e);
  }
}

/** Factur-X / ZUGFeRD PDF (EN 16931 profile) for an EU company's Sales or Credit Note voucher. */
export async function exportFacturX(shell: Shell, v: PeppolVoucher): Promise<void> {
  if (!salesOrCredit(shell, v, "Factur-X")) return;
  try {
    const x = await shell.transition(peppolApi.facturxPdf(v.id));
    const file = await pickPath(x.file_name, "PDF", "pdf");
    if (!file) return;
    await peppolApi.writeBinaryFile(file, x.pdf_base64);
    if (x.errors.length) {
      shell.warn(t("Saved to {file}, but its embedded invoice breaks EN 16931 rules: {errors}", { file: ltr(file), errors: x.errors.join(" ") }));
    } else {
      shell.info(
        t("Factur-X PDF saved to {file}. It has not been checked with a PDF/A validator; check it before relying on it.", { file: ltr(file) }),
        8000,
      );
    }
    if (x.warnings.length) shell.warn(x.warnings.join(" "));
  } catch (e) {
    shell.error(e);
  }
}

/**
 * Send a voucher's PINT document to the configured Access Point (generic
 * HTTP POST). Asks for the URL and key first when none are set.
 */
export async function sendViaAccessPoint(shell: Shell, v: PeppolVoucher, document: "pint-sg" | "pint-ae"): Promise<void> {
  if (!salesOrCredit(shell, v, document === "pint-sg" ? "PINT SG" : "PINT AE")) return;
  try {
    const s = await peppolApi.apSettings();
    if (!s.endpoint || !s.api_key_set) {
      const ok = await setUpAccessPoint(shell);
      if (!ok) return;
    }
    const r = await shell.transition(peppolApi.apSend(v.id, document));
    if (r.accepted) {
      shell.info(t("The Access Point took {file} (HTTP {status}). Check delivery with your provider.", { file: r.file_name, status: r.status }), 8000);
    } else {
      shell.warn(t("The Access Point refused {file} (HTTP {status}): {response}", { file: r.file_name, status: r.status, response: r.response }));
    }
  } catch (e) {
    shell.error(e);
  }
}

/** Ask for the Access Point URL, key header and API key; true when saved. */
export async function setUpAccessPoint(shell: Shell): Promise<boolean> {
  const s = await peppolApi.apSettings();
  shell.info(t(s.note), 9000);
  const answer = await shell.dialog(
    t("Access Point"),
    [
      { label: t("URL for sending documents (https://…)"), value: s.endpoint },
      { label: t("Key header (e.g. Authorization or X-API-Key)"), value: s.auth_header },
      { label: t("Key prefix (e.g. \"Bearer \", or empty)"), value: s.auth_header === "Authorization" && !s.auth_prefix ? "Bearer " : s.auth_prefix },
      { label: s.api_key_set ? t("API key (empty keeps the saved one)") : t("API key"), value: "" },
    ],
    ([url]) => (/^https:\/\/\S+$|^http:\/\/(127\.0\.0\.1|localhost):\d+/i.test(url.trim()) ? null : t("Enter an https:// URL.")),
  );
  if (!answer) return false;
  const [endpoint, auth_header, auth_prefix, key] = answer;
  try {
    await peppolApi.setApSettings({ endpoint, auth_header, auth_prefix, api_key: key.trim() ? key : null });
    shell.info(t("Access Point settings saved on this computer (not in the company file)."));
    return true;
  } catch (e) {
    shell.error(e);
    return false;
  }
}

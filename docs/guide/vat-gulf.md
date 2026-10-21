---
description: "VAT accounting software for the UAE and Saudi Arabia: VAT invoices, the VAT 201 and KSA VAT returns, PINT AE e-invoices and ZATCA e-invoicing in KhataOS."
---

# UAE and Saudi Arabia: VAT and ZATCA

**What you'll do:** set up a company in the UAE or Saudi Arabia, charge VAT on invoices, prepare the VAT return, and make e-invoices: PINT AE in the UAE, and ZATCA clearance and reporting in Saudi Arabia.

> The return figures are a preparation aid, not a filing. Check them with a registered tax agent before you file on EmaraTax or ZATCA's portal.

Both countries use Gregorian calendar years in the books. For the Arabic display and Hijri dates, see [Arabic](arabic.md). Invoices and notes always print in English and Arabic.

## Set up

1. Pick **United Arab Emirates** (AED) or **Saudi Arabia** (SAR) when the company first opens.
2. `Alt+K`, **Company details**: the company's **TRN** (UAE, 15 digits) or **VAT number** (Saudi, 15 digits starting and ending with 3), and the **emirate** or **region**. In the UAE the emirate decides which row of the VAT return a sale goes in.
3. **Tax rates** (`C`, **Tax Rates**): add the classes you use, for example `STANDARD` at 5% (UAE) or 15% (Saudi, from 1 July 2020), `ZERO` at 0% and `EXEMPT` at 0%. A class whose name starts with `EXEMPT` is exempt; other 0% classes are zero-rated. Give each stock item (or sales ledger) its class.
4. Create **Output VAT** and **Input VAT** ledgers under Duties & Taxes, with Type of tax **VAT**.
5. On party ledgers, add the TRN or VAT number. A party in another **Country** is an export (zero-rated). UAE parties can be marked as in a designated zone.

**Reverse charge and imports (UAE):** use classes such as `RC:STANDARD` for purchases under reverse charge and `IMPORT:STANDARD` for goods imported through UAE Customs. Detail: [UAE VAT](../uae-vat.md).

**Saudi exemption reasons:** add the VATEX code to a zero-rated or exempt class name, such as `ZERO VATEX-SA-35`, so e-invoices carry the reason. Detail: [KSA VAT and ZATCA](../ksa-zatca.md).

## The VAT return

| Country | Go To (`Alt+G`) |
|---|---|
| UAE | **VAT 201 (UAE VAT return)** |
| Saudi Arabia | **VAT Return (Saudi Arabia)** |

`F2` changes the period. `Ctrl+P` prints, `Ctrl+E` saves a CSV. Type the figures into the authority's portal yourself: KhataOS doesn't submit returns. The [Desk](desk.md) shows the due dates once you set your return periods in the compliance profile.

## UAE: PINT AE e-invoices

On a saved Sales invoice or Credit Note, press `Ctrl+E`:

- **UAE e-invoice (PINT AE)** saves the e-invoice XML.
- **Send via Access Point** sends it to your Peppol access point.
- **Access Point settings** sets the access point's address and API key (kept on this computer). You'll also be asked for them on the first send.

> The access point connection works with any provider, and has been tested **only against our own test server**. No particular provider has been certified. The PINT AE XML hasn't been checked with the official Schematron rules yet. Detail: [Peppol](../peppol.md).

## Saudi Arabia: ZATCA (Phase 2)

### Connect this computer to ZATCA

1. `Alt+K`, **Seller details (ZATCA)**: the commercial registration number (CRN) and the National Address.
2. `Alt+K`, **ZATCA onboarding**: choose the environment (sandbox, simulation or production), type the one time password (OTP) from the Fatoora portal, and accept. KhataOS then holds this computer's certificate (CSID). `Ctrl+Alt+N` renews it; `Ctrl+Alt+R` starts again as a new device.

A copy of the company file on another computer is a new device and must be onboarded there.

### Send invoices

1. Save the invoice, credit note or debit note.
2. Press `Ctrl+E`, then **Submit to ZATCA**. The first time, choose **Standard (B2B)** or **Simplified (B2C)**.
3. On the submit screen, `Ctrl+Alt+T` clears (standard) or reports (simplified) the invoice. `Ctrl+Alt+S` saves the cleared XML.

The invoice then prints with ZATCA's QR code. **ZATCA e-invoice XML** in the same menu just saves the XML.

**The ZATCA queue** (`Alt+K`, **ZATCA queue**) lists documents waiting to be cleared or reported; `Ctrl+Alt+T` submits them all. KhataOS also retries in the background while it's open. The Desk warns about simplified invoices not reported within 24 hours.

> ZATCA is **tested live on the sandbox in ZATCA's developer portal**. It hasn't been run against simulation or production yet, which need a real taxpayer's OTP. Invoices in a foreign currency can't be sent to ZATCA.

## Not there yet

UAE: blocked input tax, the FTA audit file, Corporate Tax, and filing on EmaraTax. Saudi: Zakat, and checks against the ZATCA SDK. Both: WPS salary files and end of service gratuity (no payroll yet).

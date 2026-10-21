# Testing KhataOS: a guide for accountants and businesses

KhataOS is in **alpha**. We need people who keep books for a living (accountants, CAs, bookkeepers, business owners) to try it and tell us where it is wrong, slow or confusing. You don't need to be technical, and you don't need to write code. If you know TallyPrime well, your comparison is especially useful.

The [user guide](guide/README.md) explains every screen step by step, with the exact keys. Keep it open while you test: where the app and the guide disagree, tell us.

This guide covers:
1. [Before you start](#1-before-you-start)
2. [Installing](#2-installing)
3. [Your first test company](#3-your-first-test-company)
4. [What to test, for every country](#4-what-to-test-for-every-country)
5. [What to test, per country](#5-what-to-test-per-country)
6. [How to report](#6-how-to-report)
7. [What never to share](#7-what-never-to-share)

## 1. Before you start

- **Never use a real company file.** Don't open, import or copy a client's or your own live books into KhataOS while it is in alpha. Make a new test company with made-up names and figures.
- **Never connect a test company to a live tax service.** An IRN, a ZATCA clearance or an MTD submission sent to production is a legal filing. Use only the authorities' sandboxes, and only if you know how to.
- **Your data stays on your computer.** KhataOS keeps each company in one file on your device. It has no accounts, no analytics and no tracking. Nothing is sent anywhere unless you set up sharing or a tax-authority connection yourself.
- **Figures are a preparation aid.** Returns and e-invoice files have not all been checked with the official tools yet (see the country table in the [README](../README.md#countries)). Comparing them with your own working is exactly the testing we need.

## 2. Installing

Once the first release is out, get the installer for your system from the [download page](https://khataos.github.io/khataos/download/) and follow its steps. Until then, or to test the newest code, build KhataOS from source. If you're not comfortable with that, ask a developer friend.

1. Install [Rust](https://rustup.rs) (1.85 or newer), Node 22 and [pnpm](https://pnpm.io).
   - Windows: also install the Microsoft C++ Build Tools and WebView2, as Tauri's prerequisites page explains.
   - macOS: run `xcode-select --install`.
   - Linux (Ubuntu): `sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
2. Download and run it:

   ```sh
   git clone https://github.com/khataos/khataos.git
   cd khataos/apps/desktop
   pnpm install
   pnpm app:dev
   ```

   The first build takes several minutes. Later starts are quick.
3. To build an installer for your own computer instead, run `pnpm app:build`.

Phones and tablets (Android, iOS): see [`docs/mobile.md`](mobile.md). A Bluetooth keyboard works there as on a PC.

To report which version you tested, run `git rev-parse --short HEAD` in the `khataos` folder and copy the result.

## 3. Your first test company

KhataOS has no ready-made demo company. Create a new company and enter a few test vouchers:

1. On the first screen, choose **Create Company**. Give it a made-up name (for example "Test Traders"), and save the company file somewhere you'll recognise, such as a `KhataOS test` folder.
2. Pick the **country**. It sets the financial year, the currency and the tax rules for this company.
3. In **Company → Company details** (Alt+K), enter a test tax number, not a real business's. The country guides below explain the checks: a number with a wrong check digit should be refused.
4. Create a few ledgers (**C** on the home menu → Ledger): a bank, two customers, two suppliers, a sales and a purchase ledger, and the tax ledgers.
5. Enter 10 to 20 vouchers: sales, purchases, receipts, payments, a contra entry and a journal. Include a credit note and a debit note.

Want to test the Tally import? Use a **test company** you created in TallyPrime with made-up data, export it as XML, and import it with **Alt+O**. Don't import a real client's data.

## 4. What to test, for every country

KhataOS is keyboard-first and follows TallyPrime's keys. Try to work without the mouse.

| Area | Try this |
|---|---|
| Keys | F4 Contra, F5 Payment, F6 Receipt, F7 Journal, F8 Sales, F9 Purchase, Alt+F5 Debit Note, Alt+F6 Credit Note. Enter moves on, Esc goes back, Ctrl+A accepts, Alt+G Go To. Which keys did your hands expect that didn't work? |
| Vouchers | Alter and delete a saved voucher (Enter on it in the Day Book). Cancel one. Check the numbering. |
| Bill-wise | Raise invoices against customers, then receive part payments against them. Check Outstanding and Ageing. |
| Inventory | Create stock items with opening stock, sell and buy them on item invoices, and check the Stock Summary. |
| Banking | Bank reconciliation, and importing a CSV bank statement made from test data. |
| Reports | Day Book (K), Trial Balance, Profit & Loss (P), Balance Sheet (B), ledger statements, Ratio Analysis. Do the figures tie up with your own working? |
| Printing | Print an invoice and a report (Ctrl+P), and export a report to CSV (Ctrl+E). |
| Language | Ctrl+K opens the language list (English or Arabic). Arabic readers: does the terminology read naturally? |
| Sharing (optional) | Share a test company with a second computer: office server, relay or an accountant file. See [`docs/sharing.md`](sharing.md). |

## 5. What to test, per country

Most tax reports are under **Go To (Alt+G)** or the Report Directory (**D**): type part of the name, such as "GSTR" or "VAT".

| Country | Set up | Check |
|---|---|---|
| India | Company GSTIN and state; GSTIN on party ledgers; HSN/SAC on items; GST rates. | CGST + SGST on an intra-State sale and IGST on an inter-State one; SEZ and exports. GSTR-1, GSTR-3B and GSTR-2B reconciliation against your own working. Validate the GSTR-1 JSON with the GST portal's Returns Offline Tool, and the e-invoice and e-way bill JSON with the portals' offline tools. TDS and TCS on payments, and the quarterly statement file with the FVU. Schedule III statements. See [GST returns](india-gst-returns.md), [e-Invoice](einvoice.md), [TDS](india-tds.md), [Schedule III](india-schedule-3.md). |
| UAE | TRN and emirate; the VAT rate list. | 5% VAT on invoices, zero-rated exports, exempt supplies. VAT 201 boxes against your working. Bilingual Arabic/English invoices. See [UAE VAT](uae-vat.md). |
| Saudi Arabia | VAT number and region; seller details for ZATCA. | 15% VAT, the VAT return, Arabic/English invoices with the QR code. ZATCA onboarding and submission **only on ZATCA's developer-portal sandbox**. See [KSA and ZATCA](ksa-zatca.md). |
| Singapore | GST registration number. | GST at the rate in force on the invoice date (for example an invoice dated 2023 vs 2024), zero-rated and exempt supplies, the GST F5. See [Singapore GST](singapore-gst.md). |
| UK | VAT registration number and nation. | 20/5/0% VAT, reverse charge, the VAT schemes, the 9-box return. MTD **only with an HMRC sandbox application**. See [UK VAT](uk-vat.md). |
| Germany, France, Netherlands, Spain | Tax number. | VAT rates, reverse charge, and the UStVA, CA3, aangifte or modelo 303 against your working. E-invoice files (XRechnung, Factur-X) with the official validators. See [Germany](germany-vat.md), [France](france-vat.md), [Netherlands](netherlands-vat.md), [Spain](spain-vat.md). |
| US | EIN; your own sales tax jurisdictions and rates. | The sales tax report by jurisdiction, use tax, exemption certificates, the Form 1099 list. See [US](us-sales-tax.md). |
| Mexico | RFC; issuer details. | IVA, the monthly IVA summary, CFDI XML. Use the **mock PAC** only. See [Mexico](mexico-cfdi.md). |

A wrong rate or rule is very valuable to us. Please include the official source when you report it.

## 6. How to report

Open an issue on [GitHub](https://github.com/khataos/khataos/issues/new/choose) (a free account is enough), and pick the form that fits:

- **Tester feedback**: your overall impressions. What you tried, your type of business, what was confusing, and how it compares with TallyPrime.
- **Bug report**: something broke or gave a wrong result. Write the steps so that we can repeat them in a new company.
- **Tax rule**: a rate, threshold, check or return box is wrong or missing. Link the official source (the law, a notification, or the tax authority's own guidance).
- **Feature request**: something you need that isn't there.

Questions and general discussion go in [Discussions](https://github.com/khataos/khataos/discussions). A security problem goes to the [private advisory form](https://github.com/khataos/khataos/security/advisories/new), never to a public issue.

Screenshots of your **test** company are welcome. Crop out anything that isn't made up.

## 7. What never to share

Issues and discussions are public, and they stay public. Never post or attach:

- company files (`.khata`), accountant files (`.khatapkg`) or Tally exports of a real company;
- real names, addresses, phone numbers or e-mail addresses of customers, suppliers or employees;
- real tax IDs: GSTIN, PAN, TAN, TRN, VAT numbers, RFC, EIN, SSN and the like;
- bank account numbers, cheque images or bank statements;
- passwords, invite codes (`khata1:…`), sync tokens, relay sign-up keys;
- tax-authority credentials and keys: IRP, ZATCA OTPs or CSIDs, HMRC client secrets, PAC passwords, CSD files and their passwords, Access Point keys;
- real invoice, return or salary figures.

If you post something by mistake, edit it out at once and tell us, so a maintainer can delete it from the history.

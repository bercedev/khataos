# Singapore: GST

**What you'll do:** set up a Singapore company for GST, prepare the GST F5 return, and send InvoiceNow e-invoices.

> The F5 figures are a preparation aid, not a filing. Check them before you file with IRAS.

## Set up

1. Pick **Singapore** when the company first opens. The books are in SGD, with calendar financial years (the GST periods are set in the compliance profile).
2. `Alt+K`, **Company details**: the **GST registration number** (UEN or M2 format; the format is checked). Singapore has no regions.
3. **Tax rates** (`C`, **Tax Rates**): your standard-rated class at 9% from 2024 (earlier rates are kept by date), and zero-rated and exempt classes as needed.
4. Create **Output GST** and **Input GST** ledgers under Duties & Taxes, with Type of tax **GST**.

Reverse charge on imported services and GST paid at customs are handled through tax classes. Supplies to parties abroad aren't marked as zero-rated automatically yet: give those lines a zero-rated class. Detail: [Singapore GST](../singapore-gst.md).

## The GST F5 return

1. `Alt+G`, **GST F5 (Singapore GST return)**.
2. `F2` sets the quarter.
3. `Ctrl+P` prints, `Ctrl+E` saves a CSV. Enter the boxes on myTax Portal yourself.

The [Desk](desk.md) shows the F5, ECI, Form C and annual return due dates once you set your GST quarters in the compliance profile.

## InvoiceNow (Peppol PINT SG)

On a saved Sales invoice or Credit Note, press `Ctrl+E`:

- **InvoiceNow e-invoice (PINT SG)** saves the e-invoice XML.
- **Send via Access Point** sends it to your Peppol access point.
- **Access Point settings** sets the access point's address and API key (kept on this computer).

> The access-point connection is generic and has been tested **only against a mock**. The PINT SG XML hasn't been checked with the official Schematron rules yet. Foreign-currency invoices can't be sent as PINT SG. Detail: [Peppol](../peppol.md).

## Not there yet

Filing the F5 directly, the IRAS audit file, the MES, A3PL and IGDS import schemes, and CPF payroll.

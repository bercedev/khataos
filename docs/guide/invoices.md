# Invoices

**What you'll do:** design how your invoices look, print them or save them as PDF, produce your country's e-invoice, and bill a customer in a foreign currency.

## Design your invoice

1. Press `Alt+K` (Company), then **Invoice design**. Or press `Alt+G` and type "invoice design" (or "logo").
2. Set what you want. A preview of your latest invoice updates as you go.
   - **Look:** template (Classic, Modern or Compact), accent colour, font, paper (A4, Letter or A5), and whether cash (B2C) invoices print on 80 mm receipts.
   - **Logo** and **signature** images (PNG, JPEG or SVG): press `Enter` on the field for its menu.
   - **What to show:** the HSN/SAC or tax-class column, quantity and rate columns, the amount in words, the authorised signatory. Columns your country's law requires stay on.
   - **Bank details:** account name, bank, branch, account number, IBAN, local bank code, SWIFT/BIC.
   - **Texts:** terms, declaration and footer. India also has the LUT ARN for exports.
   - **Titles** for each voucher type. Some countries fix the title by law (for example "Tax Invoice"), and then it can't be changed.
3. Press `Ctrl+A` to save. `PgUp` and `PgDn` scroll the preview.

Only an Owner can change the design. The design is kept in this company file and doesn't sync to other computers yet. More detail: [Invoice design](../invoice-design.md).

## Print or save a PDF

After you save a Sales invoice, Credit Note or Debit Note, KhataOS asks "Print?". Or, on any saved voucher:

- **Print:** press `Ctrl+P`. A print window opens with the invoice in your design, then your system's print dialog. Choose a printer, or "Save as PDF".
- **PDF file:** press `Ctrl+E` (Export), then **PDF**. KhataOS writes the PDF itself and asks where to save it. This also works on phones.
- **Many invoices at once:** in the Day Book (`K`), press `Alt+E` and choose the PDF of the period's invoices and notes. Each starts on a new page.

If some letters in a name don't come out right in the PDF file, print with `Ctrl+P` and save as PDF from the print dialog instead.

**Ship-to and export details:** on a saved Sales invoice or note, press `Ctrl+E`, then **Ship-to and export details**. Add the ship-to address, and for exports the ports, Incoterms, destination, shipping bill and (India) IGST or LUT. These print on the invoice; they don't change the books.

## E-invoices

Every e-invoice comes from a saved invoice's **Export** menu: open the voucher and press `Ctrl+E` (or `Alt+E`).

| Country | On the Export menu | Sent to the authority? |
|---|---|---|
| India | **Generate IRN (e-invoice portal)**, **e-Invoice JSON**, **e-Way Bill JSON** | IRP connection: tested only against a mock so far |
| Saudi Arabia | **Submit to ZATCA**, **ZATCA e-invoice XML** | Yes: tested on ZATCA's sandbox |
| UAE | **UAE e-invoice (PINT AE)**, **Send via Access Point** | Access point: tested only against a mock |
| Singapore | **InvoiceNow e-invoice (PINT SG)**, **Send via Access Point** | Access point: tested only against a mock |
| Germany | **XRechnung (UBL or CII)**, **Factur-X / ZUGFeRD PDF** | No: you send the file |
| France, Netherlands | **Factur-X / ZUGFeRD PDF** | No |
| Spain | **VeriFactu record**, **Factur-X / ZUGFeRD PDF** | No |
| Mexico | **CFDI 4.0** (stamping is on the CFDI invoices screen) | PAC connection: tested only against a mock |
| UK, US | PDF only | No e-invoice |

None of these files has been checked with the authority's official validator yet. Check them before you rely on them. The set-up for each is on your country page: [India](gst-india.md), [UAE and Saudi Arabia](vat-gulf.md), [Singapore](singapore.md), [Europe](europe.md), [Mexico](mexico.md).

The Factur-X PDF is a plain text layout, not your invoice design, and has no logo.

## Bilingual invoices

Saudi Arabia and UAE invoices and notes always print in English and Arabic. Saudi invoices carry the ZATCA QR code once the e-invoice has been made. With Hijri dates on (`Ctrl+K`), the Hijri date prints too. See [Arabic](arabic.md).

## Foreign currency

To bill a customer (or record a supplier's bill) in another currency:

1. Alter the party's ledger (`A`, **Ledger**) and set **Currency** to the ISO code, such as `USD` or `EUR`. A cash or bank ledger can have a currency too.
2. Enter the invoice as usual. A **Rate of exchange** field appears (for example 1 USD = … INR), filled in with the last rate you used. Type the rate for this invoice.
3. Type amounts in the foreign currency. KhataOS works out the base-currency amounts as you type.
4. Save. The invoice prints in the customer's currency, with the rate and the base-currency totals.

When the customer pays at a different rate, settle the bill against it (Agst Ref) in a Receipt with that day's rate. The gain or loss goes to **Forex Gain/Loss** when you save. Ledger statements and outstanding bills show both currencies.

Limits today:

- one foreign currency per voucher;
- rates are typed in, never fetched online;
- there is no screen to edit the currency list, keep a table of rates, or revalue open balances at year end;
- ZATCA, PINT (UAE and Singapore), XRechnung and Factur-X e-invoices refuse foreign-currency invoices.

More detail: [Multi-currency](../multi-currency.md).

## Not built yet

E-mailing an invoice (`Ctrl+M`), sending it on WhatsApp (`Ctrl+Alt+W`), print preview settings, printing many invoices directly (other than the Day Book PDF), and cheque layouts other than CTS-2010.

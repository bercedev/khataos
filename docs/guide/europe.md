# Germany, France, the Netherlands and Spain

**What you'll do:** set up a company in one of these four countries, charge VAT on invoices, prepare the VAT return, and make the e-invoice files each country uses.

> These packs are newer and less tested than the launch markets. **Nothing is sent to any tax authority**: KhataOS prepares the return figures and the files, and you file them. None of the files has been checked with the official validators yet.

All four use euros and calendar financial years.

## Set up

1. Pick the country when the company first opens.
2. `Alt+K`, **Company details**: the company's VAT number and its region:

   | Country | Tax ID checked | Region |
   |---|---|---|
   | Germany | USt-IdNr, or the Steuernummer | Bundesland |
   | France | numéro de TVA, SIREN and SIRET | région (overseas departments have their own rates) |
   | Netherlands | btw-id, or the omzetbelastingnummer | provincie |
   | Spain | NIF in all its forms | comunidad autónoma |

3. **Tax rates** (`C`, **Tax Rates**): add the classes you use, for example in Germany `STANDARD` 19% and `REDUCED` 7%, `EXEMPT`, and `REVERSE` for the §13b reverse charge. The class names matter; the country pages list them.
4. Create output and input VAT ledgers under Duties & Taxes, with Type of tax **VAT** (Germany: **USt**). For France, the Netherlands and Spain the Type of tax list also shows India's GST names; pick **VAT**.
5. Add customers' and suppliers' VAT numbers on their ledgers.

Reverse charge is supported in each country: §13b in Germany, autoliquidation in France, btw verlegd in the Netherlands and inversión del sujeto pasivo in Spain.

## The VAT return

| Country | Go To (`Alt+G`) |
|---|---|
| Germany | **UStVA (Germany VAT return)** |
| France | **CA3 (France VAT return)** |
| Netherlands | **Aangifte omzetbelasting (Netherlands VAT return)** |
| Spain | **Modelo 303 (Spain VAT return)** |

`F2` changes the period, `Ctrl+P` prints and `Ctrl+E` saves a CSV. Enter the figures in ELSTER, impots.gouv, Mijn Belastingdienst or the AEAT yourself.

## E-invoices

On a saved Sales invoice or Credit Note, press `Ctrl+E`:

| Country | Choices |
|---|---|
| Germany | **XRechnung (UBL or CII)** (asks which syntax), **Factur-X / ZUGFeRD PDF** |
| France, Netherlands | **Factur-X / ZUGFeRD PDF** |
| Spain | **VeriFactu record** (saves the record and shows its hash and QR address), **Factur-X / ZUGFeRD PDF** |

Things to know:

- **Germany (XRechnung):** the seller's contact, e-mail, IBAN and BIC are read from extra lines in the company **Address**, such as `IBAN: DE02 …` and `E-Mail: rechnung@…`. A public buyer's **Leitweg-ID** goes in the voucher narration as a line `Leitweg-ID: …`. See [Germany](../germany-vat.md).
- **Factur-X** is a PDF/A-3 with the invoice data inside it, in the EN 16931 profile. The PDF is a plain text layout, not your invoice design, and has no logo.
- **Spain (VeriFactu):** no record is made automatically when you save, the VeriFactu QR doesn't print on the invoice, and there are no VeriFactu system settings yet.
- **France:** the legal mentions (autoliquidation, the 293 B franchise, SIREN) aren't printed on invoices yet.
- **Netherlands:** there is no Peppol BIS or NLCIUS export.
- Foreign-currency invoices can't be exported as XRechnung or Factur-X.

The shared e-invoice model is described in [EN 16931](../en16931.md) and [Peppol](../peppol.md).

## More detail

[Germany](../germany-vat.md) · [France](../france-vat.md) · [Netherlands](../netherlands-vat.md) · [Spain](../spain-vat.md)

## Not there yet

Filing with ELSTER or the other authorities, sending through a Peppol access point (only Singapore and the UAE have one), ViDA, and the annual returns (they show as due dates on the [Desk](desk.md)).

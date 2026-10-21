# Mexico: IVA and CFDI

**What you'll do:** set up a Mexican company, charge IVA and withholdings, and issue CFDI 4.0 invoices stamped by a PAC.

> The PAC connection has been tested **only against a mock** so far, and the CFDI XML hasn't been checked with SAT's tools. KhataOS also has a built-in "Mock PAC" for practice: its stamps have **no fiscal validity**, and invoices stamped with it say so ("SIN VALIDEZ FISCAL").

## Set up

1. Pick **Mexico** when the company first opens. The books are in MXN, with calendar financial years.
2. `Alt+K`, **Company details**: the company's **RFC** (the check digit is verified) and its federal entity.
3. **Tax rates** (`C`, **Tax Rates**): IVA at 16%, 8% for the border region, 0% and exempt as needed. Withholdings (retención de ISR and IVA) use `RET:` classes.
4. Duty ledgers under Duties & Taxes with Type of tax **IVA**, **RET-IVA** or **RET-ISR**.
5. On customers' ledgers, add their RFC.
6. Open the CFDI screen (`Alt+G`, **CFDI invoices (Mexico)**) and set up:
   - `Ctrl+Alt+E` **issuer details**: régimen fiscal, lugar de expedición, serie and name;
   - `Ctrl+Alt+K` install your **CSD** (the `.cer` and `.key` files, and the key's password, used once);
   - `Ctrl+Alt+P` **PAC settings**: the provider (SW Sapien, or the mock), test or production, and your user, token or password.

Detail: [Mexico CFDI](../mexico-cfdi.md).

## Issue a CFDI

1. Save the Sales invoice or Credit Note.
2. To save the XML only: press `Ctrl+E`, then **CFDI 4.0**. KhataOS asks the forma de pago, the método (PUE or PPD) and the uso CFDI.
3. To stamp it: open **CFDI invoices (Mexico)** in Go To, move to the invoice, and press `Ctrl+Alt+T`. The stamp block, the timbre fiscal and the QR code then print on the invoice.

On the CFDI screen, `Enter` saves the XML (for a Receipt, the CFDI de Pago, Complemento de Pagos), `Ctrl+Alt+X` cancels a stamped CFDI (it then prints "CFDI CANCELADO"), and `Ctrl+Alt+V` records SAT's answer to a cancellation.

## Reports

`Alt+G`, **IVA summary (Mexico)**: the month's IVA charged and paid, and withholdings. `Ctrl+P` prints, `Ctrl+E` saves a CSV. The [Desk](desk.md) shows the monthly and annual due dates.

## Not there yet

The DIOT file and the electronic accounting (contabilidad electrónica) files, filing with SAT, and payroll CFDI.

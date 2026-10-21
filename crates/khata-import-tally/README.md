# khata-import-tally

Imports data from TallyPrime and Tally.ERP 9 into a KhataOS `Book`. It reads the XML that Tally produces under Export → XML (Data Interchange), and the XML its server returns on port 9000. The crate exists so people can migrate and interoperate. It works only from Tally's published XML envelope.

```rust
let masters = std::fs::read("Master.xml")?;
let day_book = std::fs::read("DayBook.xml")?;
let report = khata_import_tally::import_all([&masters[..], &day_book[..]], &mut book, Utc::now(), "admin")?;
for s in &report.skipped {
    eprintln!("skipped {:?} {}: {}", s.kind, s.name, s.reason);
}
```

## Supported

- **Envelopes:** the import-shaped file that Tally's Export writes (`ENVELOPE/BODY/IMPORTDATA/REQUESTDATA/TALLYMESSAGE`), the `BODY/DATA/TALLYMESSAGE` shape, and Collection exports (`BODY/DATA/COLLECTION`). A response with `STATUS` 0 is reported as an error.
- **Encodings:** UTF-8, and UTF-16 LE or BE with or without a BOM. Control characters are stripped, including the `&#4;` that Tally writes before reserved names such as "&#4; Primary".
- **Groups:**
  - Tally's predefined groups are matched to the KhataOS defaults by `RESERVEDNAME`, by name, or by alias; for example, "Suspense A/c" becomes Suspense Account.
  - Custom groups are created under their parent, in dependency order, so a parent can appear after its children in the file.
- **Ledgers:** name, aliases, parent, and `OPENINGBALANCE`. Tally's "Profit & Loss A/c", which sits at Primary, is filed under Reserves & Surplus.
- **Inventory masters:**
  - `UNIT`: simple units become KhataOS units. The symbol is `NAME` and the formal name is `ORIGINALNAME`. Decimal places come from `DECIMALPLACES`; when Tally gives none, they come from the most decimals the data uses for that unit, capped at 4. Compound units are skipped. A unit that items name but that has no `UNIT` master is created from the data.
  - `STOCKGROUP`: created under their parent in dependency order. "Primary" is the top level. Parent loops are reported.
  - `STOCKITEM`: `PARENT` stock group, `BASEUNITS` unit, `OPENINGBALANCE` quantity ("100 Nos"), and `OPENINGVALUE` made positive. When the value is blank, it is `OPENINGRATE` × quantity. The HSN code becomes the item's tax class. Opening stock is kept as one total at the main location.
  - `GODOWN`: created under their parent. Tally's "Main Location" is KhataOS's main location, which has no godown.
- **Item invoices:** stock lines from `ALLINVENTORYENTRIES.LIST` / `INVENTORYENTRIES.LIST` go onto the entry of the ledger in their `ACCOUNTINGALLOCATIONS.LIST`. Stock lines from `INVENTORYALLOCATIONS.LIST` (voucher mode) go onto the ledger line they sit in. Several entries for that ledger are merged into one entry carrying all its items. The direction follows Tally's sign: negative (debit, `ISDEEMEDPOSITIVE` Yes) brings stock in, as on purchases and sales returns. The quantity is `ACTUALQTY`, falling back to `BILLEDQTY`. Tally's `AMOUNT` is kept exactly.
- **Godowns and batches:** each `BATCHALLOCATIONS.LIST` that names a godown or batch becomes its own line. If the allocations do not add up to the item line, they are merged into it, with a note. When item totals are missing, they are summed from the allocations.
- **Vouchers:**
  - Accounting voucher types are imported: Payment, Receipt, Contra, Journal, Sales, Purchase, Credit Note and Debit Note. Custom voucher types are followed through the `VOUCHERTYPE` masters to one of these.
  - The voucher fields read are `DATE` (YYYYMMDD), `VOUCHERNUMBER` and `NARRATION`, plus the ledger lines in `ALLLEDGERENTRIES.LIST` and `LEDGERENTRIES.LIST`. For invoice-mode vouchers, the sales or purchase ledger inside each inventory line's `ACCOUNTINGALLOCATIONS.LIST` is also used.
  - A voucher without a number gets the next free KhataOS number.
- **Amounts:** exact decimals, never floats. Tally signs amounts **negative = debit** (those lines carry `ISDEEMEDPOSITIVE` = Yes), and they are negated into KhataOS's debit-positive convention. The parser also accepts:
  - digit grouping (`1,00,000.00`)
  - currency symbols
  - `Dr`/`Cr` suffixes
  - parentheses
  - the forex form `… = -₹ 830.00`, where the base-currency value after the last `=` is used
- **Skip reporting:** an import never stops on a single bad object. Each skipped object goes into `ImportReport::skipped` with its reason, for example:
  - cancelled or optional voucher
  - unknown ledger
  - unbalanced voucher
  - unknown or non-accounting voucher type, and stock vouchers (Stock Journal, Delivery Note, Receipt Note, Physical Stock, Material In/Out, Rejections) that are not imported yet
  - duplicate number
  - contra voucher touching a ledger that is not cash or bank
  - bad date or amount
  - missing parent group

  Notes of kind `Inventory` describe stock detail that could not be kept on an object that *was* imported. For example, a voucher may be posted without its stock lines because an item is unknown, is split across ledgers, has too many decimals, or its amounts do not add up. Its accounting is still imported. Stock lines are never dropped silently.

  A document that fails to parse is rejected before the book is changed.
- **Tally server client:** with the `http` feature, `http::fetch` POSTs an `ExportRequest` (`ExportRequest::masters()`, which includes the inventory masters, `ExportRequest::inventory_masters()`, or `ExportRequest::day_book(from, to)`) to `http://localhost:9000` and returns the response bytes.

## Exporting from Tally

- **TallyPrime:**
  1. Open the company and go to Gateway of Tally → Display More Reports → List of Accounts.
  2. Press Alt+E (Export) and choose Format = XML (Data Interchange). This file holds the masters.
  3. Export the Day Book for the whole period the same way, with Show voucher details set to Yes. This file holds the vouchers.
  4. Import both files together with `import_all`, so the vouchers can use the voucher types and ledger aliases defined in the masters file.
- **Tally.ERP 9:** use Display → List of Accounts and Display → Day Book, then Alt+E with format XML.
- **Over HTTP:** in F1 Help → Settings → Connectivity, set TallyPrime to act as Server (or Both) on port 9000, then build with `--features http`.

## Limitations

- **Some inventory and tax detail is skipped.** The importer skips compound units and stock vouchers (Stock Journal and the other non-invoice stock types). It keeps opening stock only as one total per item. It also ignores cost centres, bill-wise allocations, bank allocations, and all tax or statutory detail except the item's HSN code.
- **Some Tally objects have no KhataOS equivalent.** Vouchers from non-accounting types (orders, notes, stock journals, memorandum, reversing journals, payroll) are reported and skipped. So are custom groups placed directly under Primary.
- **Repeated voucher numbers are skipped.** KhataOS needs voucher numbers to be unique within each voucher kind. If Tally restarts numbering each financial year, or two custom types that map to the same kind share numbers, the second voucher is skipped as a duplicate.
- **Foreign-currency amounts are converted.** Only the base-currency amount is imported.
- **The `http` client uses guessed defaults.** The request IDs ("List of Accounts" with `ACCOUNTTYPE` = All Masters or All Inventory Masters, and "Day Book") and the date format (`1-Apr-2024`) follow common practice. Confirm them against your Tally release.

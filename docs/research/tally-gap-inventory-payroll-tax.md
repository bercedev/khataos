# TallyPrime feature gap: inventory, orders, manufacturing, payroll, banking and tax

> **Purpose.** This file lists what TallyPrime offers in six areas and says, for each item, whether KhataOS already has it. It is a planning input for the India, UAE, KSA, Singapore and UK launch.
>
> **Researched:** 2026-10-21, against TallyPrime 7.x. TallyPrime 7.0 shipped on 19 Dec 2025 ([product update](https://tallysolutions.com/darpan/product-update-tallyprime-7-0/)).
>
> **Clean-room rules followed.** Every description below is my own summary of what a feature does. No Tally text, screen layout, image or binary was copied or used. I used only public help pages, product pages, release notes and a few reputable tutorials. Nothing here is a spec of Tally's screens; it only describes what each feature does for the user.
>
> **How KhataOS was checked.** I grepped the Go To and menu items in `apps/desktop/src/screens/launcher.ts`, every screen in `apps/desktop/src/screens/*.ts`, the key map in `crates/khata-keymap/src/tally.rs`, the Tauri backend in `apps/desktop/src-tauri/src/*.rs`, and `crates/*`. A key bound in the key map but with no screen behind it counts as **missing**.
>
> **Source caveats.** Several help pages are release-specific mirrors (`/article/te9relNN/…`, `/docs/te9relNN/…`). Where the search returned no current `/tally-prime/…` page, I cite the newest mirror I found. Most pages were read as search-engine summaries, not fetched in full. The payroll pages for other countries cite official sites where I could reach them, and vendor guides (marked *vendor*) where I could not.

## Legend

| Field | Meaning |
|---|---|
| **Status** | **have**: works end to end in KhataOS. **partial**: part of the job is done, and the row says what is missing. **missing**: nothing usable yet. |
| **Effort** | Size of the remaining work. **S**: up to about 1 week. **M**: 1–3 weeks. **L**: more than 3 weeks, or it needs an outside partner such as a GSP, bank or access point. **—**: nothing left to build. |
| **Priority** | **P1**: Tally users expect it on day one in at least one launch market, and its absence blocks a switch. **P2**: needed for parity within a few releases. **P3**: niche, or it can wait. **—**: already built. |

File paths are relative to the repository root.

---

## 1. Inventory masters

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| INV-01 | Stock groups | Nest items in a tree so reports can roll up totals by group. | **have** | `crates/khata-core/src/inventory.rs` (`StockGroup`), `apps/desktop/src/screens/inventory.ts` (`StockGroupForm`) | — | — | <https://help.tallysolutions.com/tally-prime/inventory/manage-stock-item-tally/> |
| INV-02 | Stock categories | A second classification of items that runs across groups (for example brand or size), so substitutes can be reported together. | **missing** | — | S | P3 | <https://help.tallysolutions.com/article/te9rel61/Creating_Masters/Inventory_Info/Creating_a_Stock_Category.htm> |
| INV-03 | Simple units | A named unit of measure, with the number of decimal places allowed. | **have** | `crates/khata-core/src/inventory.rs` (`Unit`), `apps/desktop/src/screens/inventory.ts` (`UnitForm`) | — | — | <https://help.tallysolutions.com/tally-prime/inventory/manage-stock-item-tally/> |
| INV-04 | Compound units | A unit defined as a fixed multiple of another unit (for example a box of 12 pieces). Compound units can also be nested. | **missing**. The Tally importer reports and skips them (`SkipReason::CompoundUnit`). | `crates/khata-import-tally/src/apply.rs` | M | P2 | <https://help.tallysolutions.com/stock-item-management-tally/> |
| INV-05 | Alternate units | A second unit per item with its own conversion, so one line can carry both figures (for example bags and kg). It is meant for cases where the conversion varies. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/inventory/manage-stock-item-tally/> |
| INV-06 | Godowns / locations | A tree of stores, warehouses or branches. Every item line can name one. | **have** | `crates/khata-core/src/inventory.rs` (`Godown`), `apps/desktop/src/screens/godown.ts` (`GodownForm`) | — | — | <https://help.tallysolutions.com/tally-prime/inventory-reports/godown-summary-tally/> |
| INV-07 | Batches | A batch or lot name on item lines, with stock tracked per batch. | **have** (name only) | `crates/khata-core/src/inventory.rs` (`StockLine.batch`) | — | — | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-batch-wise-tally/> |
| INV-08 | Batch manufacturing and expiry dates | Batches carry a made-on date and an expiry date (or a shelf life). The dates are checked for order, and use of expired batches can be blocked. | **missing** | — | M | P1 | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-batch-wise-tally/> |
| INV-09 | Price levels and price lists | Selling rates per customer class (wholesale, retail…) with dated lists, quantity slabs and discounts. The level is set on the party so invoices fill the rate. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/inventory/price-lists-and-price-levels-faq/> |
| INV-10 | Standard cost and standard selling price | A dated standard buying and selling rate per item, used as the default rate on vouchers. | **missing**. Only the last rate is remembered (see INV-11). | — | S | P2 | <https://help.tallysolutions.com/tally/selling-buying-prices-tally/> |
| INV-11 | Rate memory on invoices | A new line starts at the rate this party last paid or was charged. | **have** | `crates/khata-core/src/inventory.rs` (`last_rate`) | — | — | <https://help.tallysolutions.com/tally/selling-buying-prices-tally/> |
| INV-12 | Reorder level and minimum order quantity | A stock level per item, group or category below which the item should be reordered, plus a minimum lot size. | **missing**. The roadmap lists it as "next". | `docs/roadmap.md` | S | P1 | <https://help.tallysolutions.com/tally-prime/accounting-and-inventory/reorder-stock-items-reorder-status-and-reorder-quantity> |
| INV-13 | Valuation (costing) methods | The way closing stock is valued for each item. | **partial**. Average cost, FIFO and last purchase cost are done. Standard cost, LIFO and monthly average are not. | `crates/khata-core/src/inventory.rs` (`Costing`), `apps/desktop/src/screens/inventory.ts` (`COSTING`) | S | P3 | <https://help.tallysolutions.com/tally-prime/inventory/stock-valuation-faq/> |
| INV-14 | Opening stock per item | Quantity and value on hand at the date the books begin. These figures feed the opening trial balance. | **have** | `crates/khata-core/src/inventory.rs` (`opening_qty`, `opening_value`) | — | — | <https://help.tallysolutions.com/tally-prime/inventory/manage-stock-item-tally/> |
| INV-15 | Physical stock verification | Record a counted quantity. The difference from the books posts as a gain or loss, and a register lists past counts. | **missing**. `Ctrl+F7` is bound, but no screen is behind it. | `crates/khata-keymap/src/tally.rs` (`PhysicalStock`) | S | P2 | <https://help.tallysolutions.com/tally-prime/inventory-reports/physical-stock-register-tally> |

## 2. Order processing

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| ORD-01 | Sales Order | Records what a customer has ordered: items, quantities, rates and due dates. It moves no stock and posts nothing to the books. | **missing**. `Ctrl+F8` is bound with no screen. The importer skips these vouchers. `VoucherKind` has no order type. | `crates/khata-keymap/src/tally.rs`, `crates/khata-import-tally/src/apply.rs`, `crates/khata-core/src/voucher.rs` | M | P1 | <https://help.tallysolutions.com/docs/te9rel64/Voucher_Entry/Order_Processing/Creating_a_Sales_Order.htm> |
| ORD-02 | Purchase Order | Records what has been ordered from a supplier. It can be closed early, in part or in full. | **missing**. `Ctrl+F9` is bound with no screen. | `crates/khata-keymap/src/tally.rs` | M | P1 | <https://help.tallysolutions.com/purchase-order-tally/> |
| ORD-03 | Delivery Note | Sends goods out before the invoice is raised. Stock drops, and the note waits to be billed. | **missing**. `Alt+F8` is bound with no screen. | `crates/khata-keymap/src/tally.rs` | M | P1 | <https://help.tallysolutions.com/tally-prime/sales-process/record-sales-of-goods-dispatched/> |
| ORD-04 | Receipt Note (GRN) | Takes goods in before the supplier's bill arrives. Stock rises, and a provisional liability waits for the bill. | **missing**. `Alt+F9` is bound with no screen. | `crates/khata-keymap/src/tally.rs` | M | P1 | <https://tallysolutions.com/business-guides/purchase-to-sale-tallyprime-gst-inventory/> |
| ORD-05 | Rejections In | Goods a customer sends back against a delivery, before or apart from a credit note. | **missing**. `Ctrl+F6` is bound with no screen. | `crates/khata-keymap/src/tally.rs` | S | P2 | <https://help.tallysolutions.com/tally-prime/sales-process/sales-process-tally/> |
| ORD-06 | Rejections Out | Goods returned to a supplier against a receipt, before the bill is booked. | **missing**. `Ctrl+F5` is bound with no screen. | `crates/khata-keymap/src/tally.rs` | S | P2 | <https://tallysolutions.com/business-guides/purchase-to-sale-tallyprime-gst-inventory/> |
| ORD-07 | Order-to-invoice tracking | Links order, note and invoice by order and tracking numbers. The invoice pulls the pending lines, and partial fulfilment is tracked. | **missing** | — | L | P1 | <https://help.tallysolutions.com/purchase-order-tally/> |
| ORD-08 | Order outstandings | Pending sales and purchase orders by party, item and due date, with order summaries and early closing. | **missing** | — | M | P1 | <https://tallysolutions.com/business-guides/how-small-trading-businesses-can-replace-separate-order-sheets-with-tallyprimes-built-in-order-management/> |
| ORD-09 | Sales Bills Pending / Purchase Bills Pending | Lists notes that are not yet invoiced, and invoices whose goods have not yet moved. | **missing** | — | S | P2 | <https://help.tallysolutions.com/pending-bills-tally/> |
| ORD-10 | Order position in stock reports | Stock summary and reorder views that show quantities on order and due out. | **missing** | — | S | P2 | <https://help.tallysolutions.com/purchase-order-tally/> |

## 3. Manufacturing and job work

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| MFG-01 | Bill of materials | A recipe per finished item: components, by-products, scrap and quantities. An item can have one recipe or several. | **missing**. `bom` appears only in unrelated code. | — | M | P2 | <https://help.tallysolutions.com/article/te9rel63/Creating_Masters/Inventory_Info/Bill_of_Materials.htm> |
| MFG-02 | Manufacturing journal | Records production. Choosing the finished item and quantity fills the components from its recipe, and the product's cost comes from what was consumed. | **partial**. The Stock Journal has consumed and produced panels and carries the consumed value to the product. There is no recipe-driven fill. | `apps/desktop/src/screens/stockjournal.ts` | M | P2 | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-in-manufacturing-tally/> |
| MFG-03 | Additional cost of manufacture | Adds labour and overheads, as amounts or percentages, into the cost of the product. | **partial**. A Stock Journal can carry balanced ledger lines, but they are not allocated into the product's rate. | `crates/khata-core/src/voucher.rs` (Stock Journal validation), `apps/desktop/src/screens/stockjournal.ts` | S | P3 | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-in-manufacturing-tally/> |
| MFG-04 | By-products, co-products and scrap | Several outputs from one production run, each taking a share of the cost. | **partial**. Several produced lines are allowed, but the user types each cost share. There is no cost-share percentage or scrap valuation. | `apps/desktop/src/screens/stockjournal.ts` | S | P3 | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-in-manufacturing-tally/> |
| MFG-05 | Stock transfer between godowns | Moves stock from one location to another at cost. | **have** | `apps/desktop/src/screens/stockjournal.ts` | — | — | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-in-manufacturing-tally/> |
| MFG-06 | Job work out (principal) | An order to an outside processor, material sent and goods received back. It also covers stock lying with the job worker, and the variance between expected and actual output. | **missing**. Job work appears only as an e-way bill sub-supply code. | `crates/khata-in/src/ewaybill.rs` | L | P2 | <https://help.tallysolutions.com/tally-prime/job-work/principal-manufacturer-job-work-out-tally/> |
| MFG-07 | Job work in (job worker) | The processor's side: the principal's material received, finished goods returned, and the principal's stock tracked apart from own stock. | **missing** | — | L | P3 | <https://help.tallysolutions.com/tally-prime/job-work/job-worker-job-work-in-tally/> |
| MFG-08 | Additional cost of purchase (landed cost) | Spreads freight, duty or handling on a purchase across its items, by quantity or by value. The items' costs rise to match. | **missing** | — | M | P1 | <https://help.tallysolutions.com/tally-prime/purchase-process/additional-cost-of-purchase/> |
| MFG-09 | Item cost tracking | Gathers procurement, landing and production cost against a cost-tracking number (an order or job) to show its cost or profit. | **missing** | — | L | P3 | <https://help.tallysolutions.com/article/te9rel64/Advanced_Features/Advanced_Inventory_Features/Item_Cost_Tracking.htm> |

## 4. Point of sale

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| POS-01 | POS invoice | A counter-billing sales type for walk-in customers, with a short receipt print. | **partial**. An item invoice can be a cash or bank sale with no party ledger. There is no counter mode and no receipt-printer layout. | `apps/desktop/src/screens/voucher.ts` (cash-sale path) | M | P2 | <https://help.tallysolutions.com/tally-prime/sales-process/point-of-sale-tally/> |
| POS-02 | Split payment | One bill paid across cash, card (with card number), cheque and gift voucher. | **missing** | — | M | P2 | <https://help.tallysolutions.com/docs/te9rel52/Advanced_Features/Advanced_Inventory_Features/POS_Invoice_MultiMode_Pymt.htm> |
| POS-03 | POS register | A day or period list of counter bills, by payment mode. | **missing** | — | S | P3 | <https://help.tallysolutions.com/tally-prime/sales-process/point-of-sale-tally/> |

## 5. Stock reports

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| RPT-01 | Stock Summary | Opening, inward, outward and closing quantity and value by group and item, with drill-down. | **have** | `apps/desktop/src/screens/inventory.ts` (`StockSummaryReport`), `crates/khata-core/src/inventory.rs` (`stock_summary`), `launcher.ts` `r:stock` | — | — | <https://help.tallysolutions.com/tally-prime/inventory-reports/stock-summary-faq-tally/> |
| RPT-02 | Stock item vouchers | The movement list of one item, with a running balance. | **have** | `apps/desktop/src/screens/inventory.ts` (`StockItemVouchersReport`), `crates/khata-core/src/inventory.rs` (`item_vouchers`) | — | — | <https://help.tallysolutions.com/tally-prime/inventory-reports/stock-summary-faq-tally/> |
| RPT-03 | Godown summary | Stock on hand by location, rolled up through the location tree. | **have** | `apps/desktop/src/screens/godown.ts` (`GodownSummaryReport`), `crates/khata-core/src/inventory.rs` (`stock_by_location`), `launcher.ts` `r:godown` | — | — | <https://help.tallysolutions.com/tally-prime/inventory-reports/godown-summary-tally/> |
| RPT-04 | Batch summary | Stock on hand by batch. | **have**, without expiry columns (see INV-08). | `apps/desktop/src/screens/godown.ts`, `launcher.ts` `r:batch` | — | — | <https://help.tallysolutions.com/tally-prime/inventory/manage-inventory-batch-wise-tally/> |
| RPT-05 | Expiry and near-expiry batches | Batches past or near their expiry date. | **missing** (depends on INV-08) | — | S | P2 | <https://help.tallysolutions.com/tally-prime/inventory-reports/stock-ageing-analysis-report-tally/> |
| RPT-06 | Movement analysis | Who bought or sold each item, how much and at what rates, from account-affecting vouchers only. It also flags slow movers. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/inventory/inventory-reports/> |
| RPT-07 | Stock ageing analysis | Stock on hand split into age slabs the user sets, by inward date or batch. | **missing**. `ageing` exists only for bills. | `crates/khata-core/src/reports.rs` (bills only) | M | P2 | <https://help.tallysolutions.com/tally-prime/inventory-reports/stock-ageing-analysis-report-tally/> |
| RPT-08 | Reorder status | Items at or below their reorder level, with the shortfall, quantities on order and due, and a suggested order quantity. | **missing** (depends on INV-12) | — | S | P1 | <https://help.tallysolutions.com/tally-prime/accounting-and-inventory/reorder-stock-items-reorder-status-and-reorder-quantity> |
| RPT-09 | Negative stock | A warning when a line takes stock below zero, plus a report of items with negative closing quantities. | **partial**. The warning is shown in vouchers and the Stock Journal. There is no exception report. | `apps/desktop/src/screens/voucher.ts` (projected-quantity warning), `apps/desktop/src/screens/stockjournal.ts` | S | P2 | <https://help.tallysolutions.com/article/te9rel65/Reports/MIS_Reports/Negative_Stock.htm> |
| RPT-10 | Valuation basis switch in reports | Re-values a stock report under another method to compare results. | **missing** | — | S | P3 | <https://help.tallysolutions.com/tally-prime/inventory-reports/godown-summary-tally/> |
| RPT-11 | Monthly item summary and cost analysis | Month-by-month movement of one item, and the cost build-up of produced items. | **missing** | — | S | P3 | <https://help.tallysolutions.com/tally-prime/inventory/inventory-reports/> |
| RPT-12 | Sales/purchase bills pending | See ORD-09. | **missing** | — | S | P2 | <https://help.tallysolutions.com/pending-bills-tally/> |

## 6. Payroll: India

Today KhataOS has no payroll module. `Ctrl+F4` (Payroll voucher) is bound in `crates/khata-keymap/src/tally.rs` with no screen behind it, and the Tally importer skips Payroll and Attendance vouchers (`crates/khata-import-tally/src/apply.rs`). The only salary feature is the TDS estimate (PAY-12).

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| PAY-01 | Employee masters | Employees grouped by department or category, with joining and leaving dates, bank account, PAN, and the PF, UAN and ESI numbers. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-02 | Pay heads | User-defined earnings and deductions. Each one is calculated as a flat amount, by attendance, by production, as a percentage of other heads, or by slab. | **missing** | — | L | P2 | <https://help.tallysolutions.com/tally-prime/payroll/payroll-features-of-tally-erp-tally/> |
| PAY-03 | Salary structure | Pay heads and amounts per employee or group, with the date each takes effect. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-04 | Attendance and production types, attendance voucher | Units such as paid days, leave without pay, overtime hours or pieces made, entered each period per employee. | **missing**. The importer skips Attendance vouchers. | `crates/khata-import-tally/src/apply.rs` | M | P2 | <https://help.tallysolutions.com/article/te9rel52/Payroll/Attendance_Production_Type.htm> |
| PAY-05 | Payroll voucher | Works out a period's salary for chosen employees from their structure and attendance, then posts the expense and liability ledgers. | **missing**. `Ctrl+F4` is bound with no screen. | `crates/khata-keymap/src/tally.rs` (`Payroll`) | L | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-06 | Payslips | A per-employee statement of attendance, earnings, deductions and net pay, printed or e-mailed one at a time or in bulk. | **missing** | — | S | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-07 | Pay sheet, payroll register, attendance register | Period grids of everyone's pay heads, and of attendance. | **missing** | — | S | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-08 | Salary advances and arrears | Pay out an advance and recover it in instalments. Work out back pay after a revision. | **missing** | — | M | P3 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-09 | Provident Fund | Employee and employer PF, the EPS split and wage ceilings. Produces the monthly statement, the statutory forms and the ECR text file for the EPFO portal. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/provident-fund-reports/payroll-e-challan-returns-tally> |
| PAY-10 | ESI | Employee and employer ESI under the wage threshold, with the monthly statement and return data. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/payroll/payroll-features-of-tally-erp-tally/> |
| PAY-11 | Professional Tax | PT by state slab, with the monthly statement and the computation. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/payroll/payroll-features-of-tally-erp-tally/> |
| PAY-12 | Income tax on salary | Projects the year's taxable salary and spreads the tax over the remaining months. Also covers the quarterly salary TDS return (Form 138, formerly 24Q) and the annual certificate (Form 16). | **partial**. A slab-based salary TDS estimate per employee exists. The salary quarterly return and Form 16 are not covered (the backend says so). | `crates/khata-in/src/salary.rs`, `apps/desktop/src/screens/tds.ts` (`SalaryTdsEstimateReport`), `apps/desktop/src-tauri/src/tds.rs` | M | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-13 | Gratuity | A monthly gratuity provision and a summary per employee, for India and the UAE. The UAE end-of-service rules differ from India's. | **missing** | — | M | P2 | <https://help.tallysolutions.com/gratuity-summary-payroll/> |
| PAY-14 | NPS | Employer and employee NPS contributions, with the subscriber summary. | **missing** | — | S | P3 | <https://help.tallysolutions.com/tally-prime/payroll/> |
| PAY-15 | Salary payment advice / bank file | A letter or file telling the bank whom to credit, and how much, for a pay period. | **missing** | — | S | P2 | <https://help.tallysolutions.com/payment-advice/> |
| PAY-16 | Statutory challan auto-fill | A Payment voucher that fills the period's PF, ESI, PT, NPS or TDS liability. | **partial**. A TDS payable report with due dates exists. There are no payroll challans. | `apps/desktop/src/screens/tds.ts` (`TdsPayableReport`) | S | P3 | <https://help.tallysolutions.com/tally-prime/payroll-transactions/pf-payment-tally/> |

### 6a. Payroll in the other launch markets (research for later)

I found **no public evidence that TallyPrime produces any of these files**. The rows record what each country expects so the payroll design (PAY-01 to PAY-07) does not close these options off. **Every rule here must be checked against the official spec before anyone builds it.**

| ID | Feature | What the country expects | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| PAY-UAE | UAE WPS salary file (SIF) | Private employers pay wages through an approved agent bank or exchange house. Each month they send a salary information file with one record per employee: labour-card ID, agent routing code, IBAN, period dates and amounts. The bank relays it to MOHRE. The exact layout varies by agent, so get each bank's template. End-of-service gratuity also follows UAE rules. | **missing** | — | M | P2 | *vendor*: <https://www.zoho.com/en-ae/payroll/academy/compliance/wps-sif-file-format.html>; Tally gratuity (UAE): <https://help.tallysolutions.com/gratuity-summary-payroll/> |
| PAY-KSA | KSA GOSI contributions and Mudad wage file | GOSI is due on basic pay plus housing, up to a monthly ceiling. Saudi nationals pay at one of two rate schedules, depending on whether they registered before or after 3 Jul 2024. Expatriates carry an employer-only occupational-hazard rate. Each month a wage file goes through Mudad (the wage protection system), and its figures must match GOSI records. Sources disagree on the new-system rates. | **missing** | — | M | P2 | *vendor*: <https://blog.zenhr.com/en/guide-to-the-gosi-social-insurance-system-in-saudi-arabia> |
| PAY-UK | UK PAYE under RTI | Every payday, the employer sends HMRC a Full Payment Submission on or before payment. An Employer Payment Summary claims reductions by the 19th of the next tax month. This also needs tax codes, NI categories, student loans, statutory pay and pension auto-enrolment. In practice the software has to be HMRC-recognised. | **missing** | — | L | P3 | <https://www.gov.uk/running-payroll/reporting-to-hmrc> |
| PAY-SG | Singapore CPF, SDL and year-end IR8A | Employers pay CPF for citizens and permanent residents earning over S$50 a month. Rates vary by age band, wage band and PR year, and change over time (37% total at age 55 and below from 1 Jan 2026). The Skills Development Levy is also due. Employment income goes to IRAS each year (IR8A and its appendices, electronically under the Auto-Inclusion Scheme). | **missing** | — | M | P3 | <https://www.cpf.gov.sg/employer/employer-obligations/how-much-cpf-contributions-to-pay>; *vendor*: <https://sbo.sg/business/tax-accounting/ir8a-auto-inclusion-scheme-ais-employer-filing-guide/> |

## 7. Banking

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| BNK-01 | Bank reconciliation | Records the bank's clearing date on each entry, and shows the book balance against the bank balance with the uncleared items. | **have** | `crates/khata-core/src/banking.rs` (`bank_reconciliation`), `apps/desktop/src/screens/banking.ts` (`BankReconciliationReport`) | — | — | <https://help.tallysolutions.com/bank-reconciliation-tally/> |
| BNK-02 | Bank statement import | Loads the bank's statement for reconciliation. Tally reads Excel, CSV and MT940, with templates for many banks. | **partial**. CSV only. There is no Excel or MT940 reader and no per-bank templates. | `apps/desktop/src/screens/banking.ts` (`IMPORT_KEY` = `Alt+S`) | S | P2 | <https://help.tallysolutions.com/article/te9rel62/Banking/import_bank_stmt_in_bank_recon.htm> |
| BNK-03 | Auto-reconciliation | Pairs statement lines with book entries: exact matches first, then likely ones by amount, date and instrument number. The user reviews the pairs. | **have** | `crates/khata-core/src/banking.rs` (`match_statement`), `apps/desktop/src/screens/banking.ts` (`StatementReview`) | — | — | <https://help.tallysolutions.com/article/te9rel64/Banking/auto_bank_reconciliation.htm> |
| BNK-04 | Vouchers from unmatched bank lines | Creates the missing entry, such as charges or interest, straight from a statement line. | **have** | `apps/desktop/src/screens/banking.ts` (`StatementReview`) | — | — | <https://help.tallysolutions.com/bank-reconciliation-tally/> |
| BNK-05 | Cheque printing | Prints payee, amount, amount in words, date and crossing on the bank's cheque stationery. | **partial**. One CTS-2010 layout. There is no per-bank layout editor or format library. | `crates/khata-print/src/lib.rs` (`ChequeLayout`), `apps/desktop/src-tauri/src/invoice.rs` (`cheque_html`) | S | P3 | <https://help.tallysolutions.com/tally-prime/banking-utilities/list-of-banks-supported-by-tallyprime-for-e-payments-auto-brs-and-cheque-formats/> |
| BNK-06 | Cheque register / cheque books | Cheque-book ranges per bank, showing used, unused, cancelled, blank and reconciled cheques. | **partial**. A register of withdrawals not yet cleared, across banks. There are no cheque books or ranges. | `apps/desktop/src/screens/banking.ts` (`ChequeRegister`) | M | P2 | <https://help.tallysolutions.com/cheque-register/> |
| BNK-07 | Post-dated cheque management | Marks a voucher post-dated so it touches no balance until its date. Includes lists and a monthly summary of PDCs received and issued, and moves a PDC from a holding bank to the real bank when presented. | **missing**. `Ctrl+T` (`MarkPostDated`) is bound with no handler. | `crates/khata-keymap/src/tally.rs` | M | P1 | <https://help.tallysolutions.com/tally-prime/banking-utilities/post-dated-cheque-managment-tally> |
| BNK-08 | Deposit slip | Prints the list of cheques and cash paid into a bank on a day, for the counter. | **missing** | — | S | P2 | <https://help.tallysolutions.com/tally-prime/banking-utilities/> (banking utilities index; no dedicated page found) |
| BNK-09 | Payment advice to suppliers | A remittance note per payment, with invoice-wise amounts, deductions, TDS and mode. Release 7.0 can send it automatically. | **missing** | — | S | P2 | <https://help.tallysolutions.com/payment-advice/> |
| BNK-10 | e-Payments (bank file) | Exports payment instructions as the bank's bulk-upload file, then reads the bank's reverse file to mark payments done. | **missing** | — | M | P2 | <https://help.tallysolutions.com/e-payments-tally/> |
| BNK-11 | Connected banking | A live link to supported banks for statements and for payments sent from the app with OTP approval and limits. | **missing** | — | L | P3 | <https://help.tallysolutions.com/online-payments/> |
| BNK-12 | Payment request links / UPI QR | Puts a payment link or UPI QR on invoices and tracks the links still pending. | **missing** | — | S | P2 | <https://help.tallysolutions.com/tally-prime/banking-utilities/> |

## 8. Statutory and tax

### 8a. India GST

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| GST-01 | GSTR-1 from the books | Builds the outward-supply return tables and the JSON to upload. | **have**. A draft JSON for the offline tool, not yet validated against the portal schema. | `apps/desktop/src-tauri/src/gst.rs` (`gstr1_json`), `crates/khata-in/src/returns.rs`, `launcher.ts` `r:gstr1` | — | — | <https://tallysolutions.com/tally/tallyprime-gst-returns-updates> |
| GST-02 | GSTR-3B from the books | Builds the summary return: outward tax, ITC and tax payable. | **have** (report only, no JSON) | `apps/desktop/src-tauri/src/gst.rs` (`gstr3b`), `launcher.ts` `r:gstr3b` | — | — | <https://tallysolutions.com/connected-gst/> |
| GST-03 | Connected GST filing | Signs in to the GST portal through a GSP from inside the app. Uploads GSTR-1, 3B and CMP-08, files with EVC or DSC, and downloads returns. | **missing**. KhataOS uses file export and import only. | — | L | P2 | <https://tallysolutions.com/connected-gst/> |
| GST-04 | GSTR-2B reconciliation | Matches the purchase register against the supplier-reported ITC statement, and lists matches, mismatches and missing items on each side. | **have**. Works on a JSON file downloaded from the portal; there is no direct download. | `apps/desktop/src-tauri/src/gstr2b.rs`, `crates/khata-in/src/gstr2b.rs`, `apps/desktop/src/screens/gstr2b.ts` | — | — | <https://help.tallysolutions.com/tally-prime/gst-reports/gstr-2b-concept-and-key-highlights-tally/> |
| GST-05 | GSTR-2A reconciliation | The same matching against the dynamic 2A statement. | **partial**. The parser accepts 2A-style field names. There is no 2A-specific flow. | `crates/khata-in/src/gstr2b.rs` | S | P3 | <https://help.tallysolutions.com/gstr-2b-reconciliation/> |
| GST-06 | ITC, reverse charge, blocked credit, Rule 37 | ITC report, reverse-charge register and self-invoices, blocked-credit flags, and ITC to reverse for unpaid bills. | **have** | `apps/desktop/src-tauri/src/gst_rcm.rs`, `crates/khata-in/src/rcm.rs`, `launcher.ts` `r:gstitc`, `r:gstrcm`, `r:gstr37`, `r:gstflags` | — | — | <https://tallysolutions.com/tally/tallyprime-gst-returns-updates> |
| GST-07 | GSTR-1A / amendments after filing | Lists outward vouchers changed after the period was filed. | **have** | `apps/desktop/src-tauri/src/gst.rs` (`gstr1_amendments`), `launcher.ts` `r:gstr1a` | — | — | <https://tallysolutions.com/tally/tallyprime-gst-returns-updates> |
| GST-08 | e-Invoice from the voucher | Sends the invoice to the IRP and gets back the IRN and signed QR. It can also cancel, and bulk JSON is available. | **have** | `apps/desktop/src-tauri/src/irp_client.rs`, `apps/desktop/src-tauri/src/einvoice.rs`, `crates/khata-in/src/irp.rs`, `apps/desktop/src/screens/irp.ts` | — | — | <https://help.tallysolutions.com/tally-prime/e-invoice-tallyprime/e-invoicing-in-tallyprime/> |
| GST-09 | e-Way bill from the voucher | Generates the e-way bill online when the invoice is saved, or in bulk. It covers notes and job-work vouchers too, and handles Part B updates, cancellation and extension. | **partial**. There is JSON for the portal's bulk tool, and an e-way bill can be generated by IRN through the IRP. There is no standalone e-way bill API, Part B update, cancel or extend. | `crates/khata-in/src/ewaybill.rs`, `apps/desktop/src-tauri/src/irp_client.rs` (`EwbByIrn`) | M | P1 | <https://help.tallysolutions.com/generate-e-way-bill-online/> |
| GST-10 | GSTR-9 / 9C annual return | Builds the annual computation from the year's books and filed returns, as the basis for GSTR-9 and the 9C reconciliation. | **missing** | — | M | P2 | <https://help.tallysolutions.com/tally-prime/gstr-9-9c/file-gstr-9-tally/> |
| GST-11 | Composition: CMP-08 and GSTR-4 | Quarterly self-assessed tax and the annual return for composition dealers. | **missing**. There is no composition scheme at all. | — | M | P2 | <https://help.tallysolutions.com/tally-prime/india-gst-composition/gstr-4-tally/> |
| GST-12 | ITC-04 (job work) | Reports goods sent to and received from job workers. | **missing** (depends on MFG-06) | — | S | P3 | <https://help.tallysolutions.com/tally-prime/inventory/job-work/> |

### 8b. India TDS and TCS

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| TDS-01 | TDS on payments | Payment natures, rates and thresholds, the no-PAN rate, lower-deduction certificates, and TDS payable with due dates. | **have** | `apps/desktop/src-tauri/src/tds.rs`, `crates/khata-in/src/tds.rs`, `apps/desktop/src/screens/tds.ts` | — | — | <https://help.tallysolutions.com/tally-prime/india-tds/tds-filing-returns-tally/> |
| TDS-02 | Quarterly TDS/TCS e-return (FVU file) | Exports the quarterly statement for residents, non-residents and TCS (Forms 140, 144 and 143 from tax year 2026-27; formerly 26Q, 27Q and 27EQ) as the text file the FVU checks. | **have** | `crates/khata-in/src/tds_file.rs`, `launcher.ts` `r:tdsq`, `r:tdsnr`, `r:tcsq` | — | — | <https://help.tallysolutions.com/tds-and-tcs-fvu-tool-9-2-errors-and-resolutions/> |
| TDS-03 | TCS on sales and receipts | Collection natures, thresholds and TCS payable. | **have** | `crates/khata-in/src/tcs.rs`, `launcher.ts` `r:tcspay` | — | — | <https://help.tallysolutions.com/tally-prime/india-tds/tds-filing-returns-tally/> |
| TDS-04 | TDS certificates and challan reconciliation | Issues certificates to deductees, and checks the challans paid against the bank's challan-status file. | **missing** | — | S | P2 | <https://help.tallysolutions.com/tally-prime/india-tds/tds-filing-returns-tally/> |
| TDS-05 | Salary TDS return and Form 16 | See PAY-12. | **partial** | `crates/khata-in/src/salary.rs` | M | P2 | <https://help.tallysolutions.com/tally-prime/payroll/> |

### 8c. Gulf, UK and Singapore VAT/GST

| ID | Feature | What it does | KhataOS status | KhataOS file | Effort | Priority | Source |
|---|---|---|---|---|---|---|---|
| VAT-01 | UAE VAT 201 return | The return boxes by emirate from the books, including reverse charge on both sides. | **have** | `apps/desktop/src-tauri/src/vat_ae.rs`, `crates/khata-ae/src/returns.rs`, `launcher.ts` `r:vat201` | — | — | <https://help.tallysolutions.com/uae-vat-201-report-return-view/> |
| VAT-02 | UAE reverse charge and imports | Tax the buyer accounts for itself, on imports and designated supplies. | **have** (`RC:` and `IMPORT:` classes) | `apps/desktop/src-tauri/src/vat_ae.rs` | — | — | <https://help.tallysolutions.com/tally-prime/ksa-vat/uae-vat-returns-tally/> |
| VAT-03 | UAE connected filing (EmaraTax) | Uploads and submits the VAT 201 to the FTA portal, according to Tally's product page. | **missing** | — | M | P3 | <https://tallysolutions.com/mena/features/taxation/> |
| VAT-04 | UAE FTA Audit File | The FTA's standard export of transactions, used for audits. | **missing**. `khata-ae` lists it as next. Tally's help has no steps for it either. | `crates/khata-ae/src/lib.rs` | S | P2 | <https://tallysolutions.com/mena/features/taxation/> |
| VAT-05 | UAE e-invoicing (PINT AE) | A structured e-invoice sent through an accredited service provider, under the coming UAE mandate. | **have**. PINT AE UBL plus a generic access-point send. No particular provider has been certified. | `apps/desktop/src-tauri/src/peppol.rs` (`pint_ae_xml`, `ap_send`) | — | — | <https://www.tallysolutions.com/mena/uae/e-invoicing/> |
| VAT-06 | KSA VAT return | The return boxes from the books. | **have** | `crates/khata-sa/src/returns.rs`, `apps/desktop/src/screens/vat_sa.ts`, `launcher.ts` `r:vatsa` | — | — | <https://tallysolutions.com/mena/features/e-invoicing/> |
| VAT-07 | KSA ZATCA Phase 1 and 2 | QR on invoices. The device is onboarded with a CSID, and each invoice is signed and cleared or reported to FATOORA. | **have** | `apps/desktop/src-tauri/src/zatca.rs`, `apps/desktop/src-tauri/src/zatca_api.rs`, `crates/khata-sa/src/*`, `apps/desktop/src/screens/zatca.ts` | — | — | <https://tallysolutions.com/mena/saudi-vat/faq-on-integration-phase-of-einvoicing/> |
| VAT-08 | UK VAT return and MTD submission | The 9-box return sent to HMRC over MTD, with the fraud-prevention headers. I found no current TallyPrime MTD page; Tally's UK help is from the ERP 9 era. **KhataOS is ahead here.** | **have** | `apps/desktop/src-tauri/src/vat_gb.rs`, `apps/desktop/src-tauri/src/mtd_client.rs`, `apps/desktop/src/screens/mtd.ts` | — | — | <https://help.tallysolutions.com/article/te9rel52/Tax_International/United_Kingdom/creating_a_company.htm> |
| VAT-09 | UK VAT schemes | The Flat Rate and Cash Accounting schemes. | **have** | `crates/khata-gb/src/schemes.rs` | — | — | <https://www.gov.uk/vat-flat-rate-scheme> |
| VAT-10 | Singapore GST F5 | The quarterly return boxes from the books. | **have** | `apps/desktop/src-tauri/src/gst_sg.rs`, `crates/khata-sg/src/returns.rs`, `launcher.ts` `r:gstf5` | — | — | <https://help.tallysolutions.com/article/te9rel52/Tax_International/Singapore/GST_Return_F5.htm> |
| VAT-11 | Singapore InvoiceNow (PINT SG) | A Peppol e-invoice through an IMDA-accredited access point. I found no public evidence that TallyPrime supports it. | **have** (UBL plus a generic access-point send) | `apps/desktop/src-tauri/src/peppol.rs` (`pint_sg_xml`, `ap_send`) | — | — | <https://help.tallysolutions.com/article/te9rel52/Tax_International/Singapore/GST_Return_F5.htm> |
| VAT-12 | Singapore F5 e-filing and the IRAS audit file | Submits the F5 directly and exports the IRAS audit file. Tally shows neither publicly. | **missing** | — | M | P3 | <https://help.tallysolutions.com/article/te9rel52/Tax_International/Singapore/GST_Return_F5.htm> |

---

## Counts

| Status | Rows |
|---|---|
| **have** | 32 |
| **partial** | 14 |
| **missing** | 64 |
| **Total** | 110 |

The counts include the four research rows for other countries' payroll (all missing). They also include two cross-reference rows that repeat another row: RPT-12 (missing) repeats ORD-09, and TDS-05 (partial) repeats PAY-12. Without those two rows, there are 108 distinct features: 32 have, 13 partial and 63 missing.

By area:

| Area | have | partial | missing |
|---|---|---|---|
| Inventory masters (15) | 6 | 1 | 8 |
| Orders (10) | 0 | 0 | 10 |
| Manufacturing and job work (9) | 1 | 3 | 5 |
| Point of sale (3) | 0 | 1 | 2 |
| Stock reports (12) | 4 | 1 | 7 |
| Payroll India (16) + other countries (4) | 0 | 2 | 18 |
| Banking (12) | 3 | 3 | 6 |
| GST (12) | 6 | 2 | 4 |
| TDS/TCS (5) | 3 | 1 | 1 |
| Gulf/UK/SG VAT (12) | 9 | 0 | 3 |

**Summary.** Tax compliance, the strongest area, is near or above parity: KhataOS has UK MTD and InvoiceNow, which Tally does not show publicly. Banking reconciliation is solid. The big gaps are **order processing (nothing yet)**, **payroll (nothing yet)**, and the inventory masters traders depend on: units, batch dates, price lists and reorder levels. The key map already reserves Tally's keys for orders, notes, rejections, payroll, physical stock and post-dated vouchers, so a Tally user who presses them today gets nothing.

**Side note.** `docs/roadmap.md` is behind the code. For example, it still lists GSTR-2B reconciliation, TCS and the FVU file as "next", but all three exist. *(Fixed 2026-10-21: the roadmap was rewritten from the code.)*

---

## Top 15: build next (ranked)

The ranking weighs three things: how often a switching SMB hits the gap in daily work, how many launch markets need it, and how much later work it unlocks.

| Rank | Build | Rows | Effort | Why now |
|---|---|---|---|---|
| 1 | **Sales Order and Purchase Order vouchers**, with order outstandings and early closing | ORD-01, ORD-02, ORD-08 | M | Every trading and distribution user works this way, in all five markets. The keys are already bound. Delivery notes and reorder views build on it. |
| 2 | **Delivery Note and Receipt Note, with order-to-invoice tracking** and Sales/Purchase Bills Pending | ORD-03, ORD-04, ORD-07, ORD-09 | L | Goods move before invoices in most Indian and Gulf trade. Without notes, stock and GRN liabilities are wrong between dispatch and billing. |
| 3 | **Post-dated cheque management** | BNK-07 | M | PDCs are routine in the UAE and KSA (rent, supplier terms) and common in India. `Ctrl+T` is bound but does nothing. |
| 4 | **Batch manufacturing and expiry dates**, blocking expired batches, and the expiry report | INV-08, RPT-05 | M | Pharma, FMCG and food distributors, a core Tally segment in India and the Gulf, cannot switch without it. |
| 5 | **Reorder level, minimum order quantity and the reorder status report** | INV-12, RPT-08 | S | Small and already on the roadmap. It gets better once orders (rank 1) feed "on order". |
| 6 | **Additional cost of purchase (landed cost)** | MFG-08 | M | Importers in the UAE, KSA and India need freight and duty in item cost, or margins and closing stock are wrong. |
| 7 | **e-Way bill online**: standalone API, generate on save, Part B, cancel and extend | GST-09 | M | Goods movement in India over the threshold needs it. Today only IRN-linked or file-based generation works. |
| 8 | **Alternate and compound units** | INV-04, INV-05 | M | Many traders buy in one unit and sell in another. The Tally import currently drops compound units. |
| 9 | **Price levels and price lists, plus standard rates** | INV-09, INV-10 | M | Wholesale and retail pricing per customer class is a common Tally habit and a frequent migration blocker. |
| 10 | **Payroll core**: employees, pay heads, salary structure, attendance, Payroll voucher, payslips and pay sheet | PAY-01 to PAY-07 | L | This is the base for every market's payroll. Design it so rank 15 and the PAY-UAE, PAY-KSA, PAY-UK and PAY-SG research rows can plug in. |
| 11 | **India statutory payroll**: PF with the ECR file, ESI and PT | PAY-09, PAY-10, PAY-11 | M | Indian employers with 20 or more staff cannot run payroll in KhataOS without it. Depends on rank 10. |
| 12 | **Bill of materials and a manufacturing journal driven by it**, with by-products and scrap | MFG-01, MFG-02, MFG-04 | M | It turns today's manual Stock Journal into real light-manufacturing support. Small Indian manufacturers are a big Tally segment. |
| 13 | **Physical stock voucher and a negative-stock exception report** | INV-15, RPT-09 | S | Cheap. It closes two Tally keys and audit habits, and the stock-count adjustment is needed at year end. |
| 14 | **Banking completeness**: Excel and MT940 statement import, supplier payment advice, deposit slip, cheque-book ranges | BNK-02, BNK-09, BNK-08, BNK-06 | S–M | Small pieces on a strong base. MT940 matters for Gulf and UK banks, and payment advice for Indian AP teams. |
| 15 | **UAE WPS SIF and gratuity**, after research is confirmed | PAY-UAE, PAY-13 | M | Every UAE employer must pay through WPS, and Tally shows no SIF support publicly, so this is a chance to lead. It needs bank templates to be confirmed first. |

Just below the cut: GSTR-9/9C (GST-10), stock ageing and movement analysis (RPT-06, RPT-07), POS split payment (POS-01, POS-02), composition dealers (GST-11), and job work out (MFG-06, then ITC-04).

//! Inventory import: units, stock groups, stock items, godowns and item
//! invoices, from hand-written Tally XML samples.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::inventory::{stock_by_location, stock_summary};
use khata_core::reports::{balance_sheet, profit_and_loss};
use khata_core::{Amount, Book, DrCr, VoucherKind};
use khata_import_tally::{ImportReport, ItemKind, SkipReason, import, import_all};
use rust_decimal_macros::dec;

const MASTERS: &[u8] = include_bytes!("fixtures/stock_masters.xml");
const VOUCHERS: &[u8] = include_bytes!("fixtures/stock_vouchers.xml");

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn full() -> (Book, ImportReport) {
    let mut book = Book::new("Kiran Hardware");
    let report = import_all([MASTERS, VOUCHERS], &mut book, now(), "importer").unwrap();
    (book, report)
}

fn reasons(report: &ImportReport, kind: ItemKind) -> Vec<(&str, &SkipReason)> {
    report.skipped_of(kind).map(|s| (s.name.as_str(), &s.reason)).collect()
}

#[test]
fn counts() {
    let (_, r) = full();
    assert_eq!(r.ledgers_created, 8);
    assert_eq!(r.units_created, 5, "Nos, kg, mtr, Box, and ltr from an item");
    assert_eq!(r.stock_groups_created, 4);
    assert_eq!(r.stock_items_created, 4);
    assert_eq!(r.godowns_created, 2);
    assert_eq!(r.vouchers_posted, 7);
    assert_eq!(r.inventory_lines, 8, "P-1 3 (wire split in two), S-1 2, CN-1 1, S-2 1, P-2 1");
}

#[test]
fn units_with_decimals() {
    let (book, _) = full();
    let unit =
        |symbol: &str| book.units().find(|u| u.symbol == symbol).unwrap_or_else(|| panic!("no unit {symbol}")).clone();
    assert_eq!((unit("Nos").name.as_str(), unit("Nos").decimals), ("Numbers", 0));
    assert_eq!(unit("kg").decimals, 3, "DECIMALPLACES");
    assert_eq!(unit("mtr").decimals, 1, "no DECIMALPLACES: 12.5 and 2.5 in the data");
    assert_eq!(unit("ltr").decimals, 1, "no UNIT master: created from the item, 4.5 in the data");
    assert!(book.units().all(|u| u.symbol != "Box of 10 Nos"));
}

#[test]
fn master_skip_reasons() {
    let (_, r) = full();
    assert_eq!(reasons(&r, ItemKind::Unit), [("Box of 10 Nos", &SkipReason::CompoundUnit)]);
    assert_eq!(
        reasons(&r, ItemKind::StockGroup),
        [
            ("Loop A", &SkipReason::StockGroupCycle("Loop B".into())),
            ("Loop B", &SkipReason::StockGroupCycle("Loop A".into()))
        ]
    );
    assert_eq!(
        reasons(&r, ItemKind::StockItem),
        [
            ("Hinge Box", &SkipReason::CompoundItemUnit("Box of 10 Nos".into())),
            ("Orphan Item", &SkipReason::UnknownParentStockGroup("Loop A".into())),
        ]
    );
    assert_eq!(reasons(&r, ItemKind::Godown), []);
}

#[test]
fn nested_stock_groups_and_items() {
    let (book, _) = full();
    let group = |name: &str| book.stock_groups().find(|g| g.name == name).unwrap().clone();
    assert_eq!(group("Screws").parent, Some(group("Fasteners").id));
    assert_eq!(group("Fasteners").parent, Some(group("Hardware").id));
    assert_eq!(group("Hardware").parent, None);
    assert_eq!(group("Paint").parent, None, "&#4; Primary");

    let item = |name: &str| book.stock_items().find(|i| i.name == name).unwrap().clone();
    let screw = item("Wood Screw 25mm");
    assert_eq!(screw.group, Some(group("Screws").id));
    assert_eq!((screw.opening_qty, screw.opening_value), (dec!(1000), dec!(2000)), "Tally -2000.00 made positive");
    assert_eq!(screw.tax_class.as_deref(), Some("7318"), "HSN from GSTDETAILS.LIST");
    assert_eq!((item("Steel Wire").opening_qty, item("Steel Wire").opening_value), (dec!(50.5), dec!(5050)));
    assert_eq!(item("Copper Pipe").opening_value, dec!(1000), "12.5 mtr x 80.00 when OPENINGVALUE is blank");
    assert_eq!(item("Wall Paint").group, Some(group("Paint").id), "parent by the group's alias");

    let shelf = book.godowns().find(|g| g.name == "Shelf 2").unwrap();
    let back = book.godowns().find(|g| g.name == "Back Store").unwrap();
    assert_eq!(shelf.parent, Some(back.id));
    assert!(book.godowns().all(|g| g.name != "Main Location"), "Tally's default godown is the main location");

    // Opening stock is part of the opening trial balance, as in Tally.
    assert_eq!(book.opening_difference(), Amount::ZERO);
}

#[test]
fn item_invoices_carry_stock_lines() {
    let (book, _) = full();
    let ledger = |name: &str| book.ledger_by_name(name).unwrap().id;
    let voucher = |n: &str| book.vouchers().find(|v| v.number == n).unwrap();

    let p1 = voucher("P-1");
    assert_eq!(p1.kind, VoucherKind::Purchase);
    let purchase = p1.entries.iter().find(|e| e.ledger == ledger("Purchase")).unwrap();
    assert_eq!((purchase.side, purchase.amount), (DrCr::Dr, dec!(3075)), "two items merged onto one entry");
    assert_eq!(purchase.items.len(), 3);
    let wire: Vec<_> = purchase.items.iter().filter(|l| l.batch.is_some()).collect();
    assert_eq!(wire.len(), 2, "split by batch allocation");
    assert_eq!((wire[0].qty, wire[0].amount, wire[0].batch.as_deref()), (dec!(10.25), dec!(1025), Some("LOT-A")));
    assert_eq!(book.godown(wire[1].godown.unwrap()).unwrap().name, "Shelf 2");
    let input = p1.entries.iter().find(|e| e.ledger == ledger("Input GST")).unwrap();
    assert_eq!((input.side, input.amount, input.items.len()), (DrCr::Dr, dec!(553.50), 0));

    let s1 = voucher("S-1");
    assert_eq!(s1.narration, "Screws and pipe");
    let sales = s1.entries.iter().find(|e| e.ledger == ledger("Sales")).unwrap();
    assert_eq!((sales.side, sales.amount), (DrCr::Cr, dec!(1575)));
    let lines: Vec<_> = sales.items.iter().map(|l| (l.qty, l.rate, l.amount, l.godown, l.batch.clone())).collect();
    assert_eq!(lines, [(dec!(300), dec!(4), dec!(1200), None, None), (dec!(2.5), dec!(150), dec!(375), None, None)]);
    assert_eq!(s1.entries.len(), 3, "party, sales and tax");

    let cn = voucher("CN-1");
    let sales = cn.entries.iter().find(|e| e.ledger == ledger("Sales")).unwrap();
    assert_eq!((sales.side, sales.amount, sales.items[0].qty), (DrCr::Dr, dec!(200), dec!(50)), "a return comes in");

    let p2 = voucher("P-2");
    let line = &p2.entries.iter().find(|e| e.ledger == ledger("Purchase")).unwrap().items[0];
    assert_eq!((line.qty, line.amount), (dec!(7.5), dec!(2250)), "summed from batch allocations");

    for n in ["S-3", "S-4"] {
        assert!(voucher(n).entries.iter().all(|e| e.items.is_empty()), "{n} posted without stock lines");
    }
}

#[test]
fn stock_detail_notes() {
    let (_, r) = full();
    let notes = reasons(&r, ItemKind::Inventory);
    assert_eq!(notes.len(), 4, "{notes:#?}");
    assert_eq!(notes[0], ("Steel Wire", &SkipReason::OpeningByGodownMerged));
    assert_eq!(
        notes[1],
        (
            "Sales S-3 on 2024-04-22",
            &SkipReason::StockLinesDropped("stock item \"Hinge Box\" is not in the import or the book".into())
        )
    );
    assert_eq!(notes[2].0, "Sales S-4 on 2024-04-23");
    assert!(
        matches!(notes[2].1, SkipReason::StockLinesDropped(why) if why.contains("decimal places")),
        "{:?}",
        notes[2]
    );
    assert_eq!(notes[3], ("Steel Wire", &SkipReason::BatchDetailMerged));
}

#[test]
fn stock_vouchers_are_skipped_with_reason() {
    let (_, r) = full();
    assert_eq!(
        reasons(&r, ItemKind::Voucher),
        [
            ("Stock Journal SJ-1 on 2024-04-25", &SkipReason::InventoryVoucherType("Stock Journal".into())),
            ("Factory Transfer FT-1 on 2024-04-26", &SkipReason::InventoryVoucherType("Stock Journal".into())),
        ]
    );
    assert_eq!(
        SkipReason::InventoryVoucherType("Stock Journal".into()).to_string(),
        "Stock Journal vouchers aren't imported yet; stock moves only through Sales, Purchase, Credit Note and Debit Note"
    );
}

#[test]
fn stock_summary_matches() {
    let (book, _) = full();
    let s = stock_summary(&book, date(2024, 4, 1), date(2025, 3, 31));
    let row = |name: &str| s.rows.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no row {name}"));

    let screw = row("Wood Screw 25mm");
    assert_eq!(screw.group, "Hardware / Fasteners / Screws");
    assert_eq!(screw.unit, "Nos");
    let m = &screw.movement;
    assert_eq!((m.opening_qty, m.opening_value), (dec!(1000), dec!(2000)));
    assert_eq!((m.inward_qty, m.inward_value), (dec!(550), dec!(1250)), "purchase and sales return");
    assert_eq!((m.outward_qty, m.outward_value), (dec!(300), dec!(1200)), "S-4's line was dropped");
    assert_eq!((m.closing_qty, m.closing_value), (dec!(1250), dec!(2620.97)));

    let wire = &row("Steel Wire").movement;
    assert_eq!((wire.inward_qty, wire.outward_qty, wire.closing_qty), (dec!(20.25), dec!(5), dec!(65.75)));
    assert_eq!(wire.closing_value, dec!(6575));
    let pipe = &row("Copper Pipe").movement;
    assert_eq!((pipe.closing_qty, pipe.closing_value), (dec!(10), dec!(800)));
    let paint = &row("Wall Paint").movement;
    assert_eq!((paint.closing_qty, paint.closing_value), (dec!(7.5), dec!(2250)));

    assert_eq!(s.opening_value, dec!(8050));
    assert_eq!(s.closing_value, dec!(12245.97));

    // Stock by godown and batch.
    let at = stock_by_location(&book, date(2025, 3, 31));
    let wire_at = |godown: &str, batch: Option<&str>| {
        at.iter()
            .find(|r| r.item_name == "Steel Wire" && r.godown_name == godown && r.batch.as_deref() == batch)
            .map(|r| r.qty)
    };
    assert_eq!(wire_at("Back Store", Some("LOT-A")), Some(dec!(10.25)));
    assert_eq!(wire_at("Shelf 2", Some("LOT-B")), Some(dec!(10)));
    assert_eq!(wire_at("Main Location", None), Some(dec!(45.5)), "opening 50.5 less the merged 5 kg sale");
}

#[test]
fn profit_and_loss_and_balance_sheet_tally() {
    let (book, _) = full();
    let pl = profit_and_loss(&book, date(2024, 4, 1), date(2025, 3, 31));
    assert_eq!((pl.opening_stock, pl.closing_stock), (dec!(8050), dec!(12245.97)));
    // Sales 1575 + 750 + 500 + 10 - 200 return; purchases 3075 + 2250.
    assert_eq!(pl.gross_profit, dec!(2635) - dec!(5325) + dec!(12245.97) - dec!(8050));
    assert_eq!(pl.net_profit, pl.gross_profit);

    let bs = balance_sheet(&book, date(2025, 3, 31));
    assert_eq!(bs.closing_stock, dec!(12245.97));
    assert_eq!(bs.opening_difference, Amount::ZERO);
    assert_eq!(bs.profit_and_loss, pl.net_profit);
    assert_eq!(bs.total_assets, bs.total_liabilities);

    let tb = book.trial_balance();
    let dr: Amount = tb.iter().map(|r| r.debit).sum();
    let cr: Amount = tb.iter().map(|r| r.credit).sum();
    assert_eq!(dr + dec!(8050), cr, "ledger openings plus opening stock tally");
}

#[test]
fn reimport_reuses_masters() {
    let (mut book, _) = full();
    let again = import(MASTERS, &mut book, now(), "importer").unwrap();
    assert_eq!(
        (again.units_created, again.stock_groups_created, again.stock_items_created, again.godowns_created),
        (0, 0, 0, 0)
    );
    assert_eq!(again.skipped_of(ItemKind::StockItem).filter(|s| s.reason == SkipReason::DuplicateStockItem).count(), 4);
}

#[test]
fn vouchers_alone_keep_accounting_and_report_stock() {
    // Without the masters file the items are unknown: every voucher whose
    // ledgers exist keeps its accounting and says its stock was dropped.
    let (book, _) = full();
    let mut other = Book::new("Kiran Hardware");
    let ledgers_only = String::from_utf8_lossy(MASTERS).split("<!-- Units").next().unwrap().to_owned()
        + "</REQUESTDATA></IMPORTDATA></BODY></ENVELOPE>";
    import(ledgers_only.as_bytes(), &mut other, now(), "u").unwrap();
    let r = import(VOUCHERS, &mut other, now(), "u").unwrap();
    assert_eq!(r.vouchers_posted, 7);
    assert_eq!(r.inventory_lines, 0);
    assert!(other.vouchers().all(|v| v.entries.iter().all(|e| e.items.is_empty())));
    assert_eq!(
        r.skipped_of(ItemKind::Inventory).filter(|s| matches!(s.reason, SkipReason::StockLinesDropped(_))).count(),
        7
    );
    // The book with stock and the one without agree on every ledger.
    let tb = |b: &Book| b.trial_balance().into_iter().map(|r| (r.name, r.debit - r.credit)).collect::<Vec<_>>();
    assert_eq!(tb(&book), tb(&other));
}

#[test]
fn import_is_replayable_from_the_audit_trail() {
    let (book, _) = full();
    let replayed = Book::replay("Kiran Hardware", book.audit().clone()).unwrap();
    assert_eq!(replayed.trial_balance(), book.trial_balance());
    let summary = |b: &Book| stock_summary(b, date(2024, 4, 1), date(2025, 3, 31)).closing_value;
    assert_eq!(summary(&replayed), summary(&book));
}

fn day_book(vouchers: &str) -> Vec<u8> {
    format!(
        "<ENVELOPE><HEADER><TALLYREQUEST>Import Data</TALLYREQUEST></HEADER><BODY><IMPORTDATA><REQUESTDATA>\
         <TALLYMESSAGE xmlns:UDF=\"TallyUDF\">{vouchers}</TALLYMESSAGE></REQUESTDATA></IMPORTDATA></BODY></ENVELOPE>"
    )
    .into_bytes()
}

#[test]
fn voucher_mode_inventory_allocations() {
    // "As Voucher" entry: items nested in the purchase ledger's line.
    let xml = day_book(
        r#"<VOUCHER VCHTYPE="Purchase"><DATE>20240501</DATE><VOUCHERNUMBER>PV-1</VOUCHERNUMBER>
            <ALLLEDGERENTRIES.LIST><LEDGERNAME>Purchase</LEDGERNAME><ISDEEMEDPOSITIVE>Yes</ISDEEMEDPOSITIVE><AMOUNT>-500.00</AMOUNT>
              <INVENTORYALLOCATIONS.LIST><STOCKITEMNAME>Copper Pipe</STOCKITEMNAME><ISDEEMEDPOSITIVE>Yes</ISDEEMEDPOSITIVE>
                <RATE>100.00/mtr</RATE><AMOUNT>-500.00</AMOUNT><ACTUALQTY> 5 mtr</ACTUALQTY><BILLEDQTY> 5 mtr</BILLEDQTY>
              </INVENTORYALLOCATIONS.LIST>
            </ALLLEDGERENTRIES.LIST>
            <ALLLEDGERENTRIES.LIST><LEDGERNAME>Cash</LEDGERNAME><ISDEEMEDPOSITIVE>No</ISDEEMEDPOSITIVE><AMOUNT>500.00</AMOUNT></ALLLEDGERENTRIES.LIST>
          </VOUCHER>
          <VOUCHER VCHTYPE="Sales"><DATE>20240502</DATE><VOUCHERNUMBER>SV-1</VOUCHERNUMBER>
            <ALLINVENTORYENTRIES.LIST><STOCKITEMNAME>Copper Pipe</STOCKITEMNAME><ISDEEMEDPOSITIVE>No</ISDEEMEDPOSITIVE>
              <RATE>200.00/mtr</RATE><AMOUNT>200.00</AMOUNT><ACTUALQTY> 1 mtr</ACTUALQTY>
              <ACCOUNTINGALLOCATIONS.LIST><LEDGERNAME>Sales</LEDGERNAME><AMOUNT>150.00</AMOUNT></ACCOUNTINGALLOCATIONS.LIST>
              <ACCOUNTINGALLOCATIONS.LIST><LEDGERNAME>Output GST</LEDGERNAME><AMOUNT>50.00</AMOUNT></ACCOUNTINGALLOCATIONS.LIST>
            </ALLINVENTORYENTRIES.LIST>
            <ALLLEDGERENTRIES.LIST><LEDGERNAME>Cash</LEDGERNAME><ISDEEMEDPOSITIVE>Yes</ISDEEMEDPOSITIVE><AMOUNT>-200.00</AMOUNT></ALLLEDGERENTRIES.LIST>
          </VOUCHER>"#,
    );
    let mut book = Book::new("Kiran Hardware");
    let r = import_all([MASTERS, &xml[..]], &mut book, now(), "u").unwrap();
    assert_eq!((r.vouchers_posted, r.inventory_lines), (2, 1));
    let pv = book.vouchers().find(|v| v.number == "PV-1").unwrap();
    let line = &pv.entries.iter().find(|e| e.side == DrCr::Dr).unwrap().items[0];
    assert_eq!((line.qty, line.rate, line.amount), (dec!(5), dec!(100), dec!(500)));
    assert_eq!(
        reasons(&r, ItemKind::Inventory).last().copied(),
        Some((
            "Sales SV-1 on 2024-05-02",
            &SkipReason::StockLinesDropped("\"Copper Pipe\" is split across 2 ledgers".into())
        ))
    );
}

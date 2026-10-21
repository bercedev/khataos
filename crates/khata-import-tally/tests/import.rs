//! End-to-end imports of hand-written Tally XML samples.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{Amount, Book, DrCr, Nature, VoucherKind};
use khata_import_tally::{ImportError, ImportReport, ItemKind, SkipReason, import, import_all};
use rust_decimal_macros::dec;

const MASTERS: &[u8] = include_bytes!("fixtures/masters.xml");
const VOUCHERS: &[u8] = include_bytes!("fixtures/vouchers.xml");

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn masters() -> (Book, ImportReport) {
    let mut book = Book::new("Shah & Sons");
    let report = import(MASTERS, &mut book, now(), "importer").unwrap();
    (book, report)
}

fn opening(book: &Book, ledger: &str) -> Amount {
    book.ledger_by_name(ledger).unwrap_or_else(|| panic!("missing ledger {ledger}")).opening
}

fn reasons(report: &ImportReport, kind: ItemKind) -> Vec<(&str, &SkipReason)> {
    report.skipped_of(kind).map(|s| (s.name.as_str(), &s.reason)).collect()
}

#[test]
fn masters_create_groups_and_ledgers() {
    let (_, report) = masters();
    assert_eq!(report.groups_created, 4);
    assert_eq!(report.groups_matched, 2);
    assert_eq!(report.ledgers_created, 10);
    assert_eq!(report.opening_balances, 6);
    assert_eq!(report.vouchers_posted, 0);
}

#[test]
fn group_hierarchy_resolves_out_of_order_parents() {
    let (book, _) = masters();
    let mumbai = book.group_by_name("Mumbai Debtors").unwrap();
    let west = book.group_by_name("West Debtors").unwrap();
    assert_eq!(mumbai.parent, Some(west.id));
    assert_eq!(west.parent, Some(book.group_by_name("Sundry Debtors").unwrap().id));
    assert_eq!(book.primary_group(mumbai.id).unwrap().name, "Current Assets");
    assert_eq!(mumbai.nature, Nature::Assets);

    // Named from NAME.LIST; a child refers to it before it appears.
    let current = book.group_by_name("Current A/cs").unwrap();
    let salary = book.group_by_name("Salary Accounts").unwrap();
    assert_eq!(salary.parent, Some(current.id));
    assert!(salary.is_cash_or_bank, "inherits from Bank Accounts");

    // Ledger parent given by the group's alias.
    let hdfc = book.ledger_by_name("HDFC Bank").unwrap();
    assert_eq!(hdfc.group, current.id);
    assert!(book.is_cash_or_bank(hdfc.id));
}

#[test]
fn reserved_groups_map_to_defaults() {
    let (book, _) = masters();
    let before = Book::new("x").groups().count();
    assert_eq!(book.groups().count(), before + 4, "Sundry Debtors and Suspense A/c were not duplicated");
    assert!(book.group_by_name("Suspense A/c").is_none(), "mapped onto Suspense Account");
    let pl = book.ledger_by_name("Profit & Loss A/c").unwrap();
    assert_eq!(book.group(pl.group).unwrap().name, "Reserves & Surplus");
}

#[test]
fn opening_balances_convert_sign_and_tally() {
    let (book, _) = masters();
    assert_eq!(opening(&book, "Cash"), dec!(25000), "Tally -25000.00 is a debit");
    assert_eq!(opening(&book, "HDFC Bank"), dec!(100000), "Indian digit grouping");
    assert_eq!(opening(&book, "Ramesh Traders"), dec!(15000), "explicit Dr suffix");
    assert_eq!(opening(&book, "Suresh Suppliers"), dec!(-20000), "positive is a credit");
    assert_eq!(opening(&book, "Capital A/c"), dec!(-130000), "currency symbol");
    assert_eq!(opening(&book, "Profit & Loss A/c"), dec!(10000));
    assert_eq!(opening(&book, "Purchase"), Amount::ZERO);
    assert_eq!(book.opening_difference(), Amount::ZERO);
}

#[test]
fn master_skip_reasons() {
    let (book, report) = masters();
    assert_eq!(
        reasons(&report, ItemKind::Group),
        [
            ("Branch Assets", &SkipReason::CustomPrimaryGroup),
            ("Orphans", &SkipReason::UnknownParentGroup("No Such Group".into())),
        ]
    );
    assert_eq!(
        reasons(&report, ItemKind::Ledger),
        [("Lost Ledger", &SkipReason::UnknownParentGroup("Orphans".into()))]
    );
    assert!(book.ledger_by_name("Lost Ledger").is_none());
}

/// Masters and Day Book exports imported together, as a migration would.
fn full() -> (Book, ImportReport) {
    let mut book = Book::new("Shah & Sons");
    let report = import_all([MASTERS, VOUCHERS], &mut book, now(), "importer").unwrap();
    (book, report)
}

#[test]
fn vouchers_post_and_trial_balance_is_correct() {
    let (book, report) = full();
    assert_eq!(report.vouchers_posted, 6);
    assert_eq!(report.vouchers_numbered, 1);

    let tb = book.trial_balance();
    let row = |name: &str| {
        let r = tb.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no row for {name}"));
        r.debit - r.credit
    };
    assert_eq!(row("Cash"), dec!(13000));
    assert_eq!(row("HDFC Bank"), dec!(115000));
    assert_eq!(row("Ramesh Traders"), dec!(21800));
    assert_eq!(row("Suresh Suppliers"), dec!(-28500));
    assert_eq!(row("Capital A/c"), dec!(-130000));
    assert_eq!(row("Profit & Loss A/c"), dec!(10000));
    assert_eq!(row("Sales"), dec!(-10000));
    assert_eq!(row("Output Tax"), dec!(-1800));
    assert_eq!(row("Rent"), dec!(2500));
    assert_eq!(row("Purchase"), dec!(8000));
    let dr: Amount = tb.iter().map(|r| r.debit).sum();
    let cr: Amount = tb.iter().map(|r| r.credit).sum();
    assert_eq!((dr, cr), (dec!(170300), dec!(170300)));
}

#[test]
fn voucher_fields_are_kept() {
    let (book, _) = full();
    let sale = book.vouchers().find(|v| v.number == "INV-1").unwrap();
    assert_eq!(sale.kind, VoucherKind::Sales, "custom type mapped through its parent");
    assert_eq!(sale.date, NaiveDate::from_ymd_opt(2024, 4, 10).unwrap());
    assert_eq!(sale.narration, "10 cartons");
    assert_eq!(sale.entries.len(), 3, "party, tax and the sales ledger from the inventory line");
    let party = book.ledger_by_name("Ramesh Traders").unwrap().id;
    let party_entry = sale.entries.iter().find(|e| e.ledger == party).unwrap();
    assert_eq!((party_entry.side, party_entry.amount), (DrCr::Dr, dec!(11800)));

    let petty = book.vouchers().find(|v| v.kind == VoucherKind::Payment).unwrap();
    assert_eq!(petty.narration, "April rent\r\npaid in cash");

    let journal = book.vouchers().find(|v| v.kind == VoucherKind::Journal).unwrap();
    assert_eq!(journal.number, "1", "unnumbered voucher numbered by KhataOS");
}

#[test]
fn voucher_skip_reasons() {
    let (_, report) = full();
    assert_eq!(
        reasons(&report, ItemKind::Voucher),
        [
            ("Journal J-BAD on 20241345", &SkipReason::InvalidDate("20241345".into())),
            ("Sales GST INV-2 on 2024-04-11", &SkipReason::Cancelled),
            ("Journal J-OPT on 2024-04-12", &SkipReason::Optional),
            ("Payment 2 on 2024-04-13", &SkipReason::UnknownLedger("Electricity".into())),
            ("Payment 3 on 2024-04-14", &SkipReason::Unbalanced { dr: dec!(300), cr: dec!(299) }),
            ("Mystery 1 on 2024-04-16", &SkipReason::UnknownVoucherType("Mystery".into())),
            ("Customer Order SO-1 on 2024-04-17", &SkipReason::UnsupportedVoucherType("Customer Order".into())),
            ("Receipt 1 on 2024-04-18", &SkipReason::DuplicateNumber("1".into())),
            ("Contra 2 on 2024-04-19", &SkipReason::ContraNonCash),
        ]
    );
}

#[test]
fn reimporting_masters_keeps_existing_ledgers() {
    let (mut book, _) = masters();
    let again = import(MASTERS, &mut book, now(), "importer").unwrap();
    assert_eq!((again.groups_created, again.ledgers_created, again.opening_balances), (0, 0, 0));
    assert_eq!(again.skipped_of(ItemKind::Ledger).filter(|s| s.reason == SkipReason::DuplicateLedger).count(), 10);
    assert_eq!(book.opening_difference(), Amount::ZERO);
}

#[test]
fn separate_imports_lose_voucher_types_and_aliases() {
    // Imported one file at a time, the Day Book cannot see the masters
    // file's VOUCHERTYPE parents or ledger aliases: those vouchers are
    // reported, not guessed.
    let (mut book, _) = masters();
    let report = import(VOUCHERS, &mut book, now(), "importer").unwrap();
    let skipped = reasons(&report, ItemKind::Voucher);
    assert!(skipped.contains(&("Sales GST INV-1 on 2024-04-10", &SkipReason::UnknownVoucherType("Sales GST".into()))));
    assert!(skipped.contains(&("Petty Cash 1 on 2024-04-20", &SkipReason::UnknownVoucherType("Petty Cash".into()))));
    assert!(skipped.contains(&("Receipt 1 on 2024-04-15", &SkipReason::UnknownLedger("HDFC".into()))));
    assert_eq!(report.vouchers_posted, 4, "contra, purchase, journal and the second receipt");
}

#[test]
fn utf16_exports_import() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n<ENVELOPE><HEADER><TALLYREQUEST>Import Data</TALLYREQUEST></HEADER>\
        <BODY><IMPORTDATA><REQUESTDATA><TALLYMESSAGE xmlns:UDF=\"TallyUDF\">\
        <LEDGER NAME=\"नकद\" RESERVEDNAME=\"\"><PARENT>&#4; Cash-in-Hand</PARENT><OPENINGBALANCE>-₹ 1,000.50</OPENINGBALANCE></LEDGER>\
        <LEDGER NAME=\"Owner&apos;s Capital\" RESERVEDNAME=\"\"><PARENT>Capital Account</PARENT><OPENINGBALANCE>1000.50</OPENINGBALANCE></LEDGER>\
        </TALLYMESSAGE></REQUESTDATA></IMPORTDATA></BODY></ENVELOPE>\r\n";

    let mut le_bom = vec![0xFF, 0xFE];
    le_bom.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    let be: Vec<u8> = xml.encode_utf16().flat_map(u16::to_be_bytes).collect();

    for bytes in [le_bom, be] {
        let mut book = Book::new("Unicode");
        let report = import(&bytes, &mut book, now(), "u").unwrap();
        assert_eq!(report.ledgers_created, 2, "{report:?}");
        assert_eq!(opening(&book, "नकद"), dec!(1000.50));
        assert_eq!(opening(&book, "Owner's Capital"), dec!(-1000.50));
        assert_eq!(book.opening_difference(), Amount::ZERO);
    }
}

#[test]
fn control_characters_are_stripped() {
    let xml = "<ENVELOPE><BODY><DATA><TALLYMESSAGE>\
        <LEDGER NAME=\"Petty\u{1}Cash\"><PARENT>&#4; Cash-in-Hand&#x1F;</PARENT></LEDGER>\
        </TALLYMESSAGE></DATA></BODY></ENVELOPE>";
    let mut book = Book::new("Ctl");
    import(xml.as_bytes(), &mut book, now(), "u").unwrap();
    let l = book.ledger_by_name("PettyCash").unwrap();
    assert_eq!(book.group(l.group).unwrap().name, "Cash-in-Hand");
}

#[test]
fn bad_documents_abort_without_touching_the_book() {
    let mut book = Book::new("Untouched");
    assert_eq!(import(b"<html></html>", &mut book, now(), "u"), Err(ImportError::NotTallyXml("html".into())));
    let truncated = &MASTERS[..MASTERS.len() / 2];
    assert!(matches!(import(truncated, &mut book, now(), "u"), Err(ImportError::Xml { .. })));
    assert_eq!(book.ledgers().count(), 0);
    assert!(book.audit().entries().is_empty());
}

#[test]
fn import_is_replayable_from_the_audit_trail() {
    let (book, _) = full();
    let replayed = Book::replay("Shah & Sons", book.audit().clone()).unwrap();
    assert_eq!(replayed.trial_balance(), book.trial_balance());
    assert_eq!(replayed.groups().count(), book.groups().count());
}

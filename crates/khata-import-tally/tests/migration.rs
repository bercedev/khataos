//! A migration from TallyPrime, case by case, on invented exports:
//! bill-wise details, GST masters, Bank OD and OCC accounts, custom voucher
//! types and their numbering, a Day Book that starts late, closing balances
//! checked against Tally's Trial Balance, cancelled and post-dated
//! vouchers, and the report that can be shared.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::reports::outstanding_bills;
use khata_core::{Amount, Book, BookError, Entry, Restart, TaxSide, Voucher, VoucherId, VoucherKind};
use khata_import_tally::{ImportReport, ItemKind, NoteKind, SkipReason, import, import_all};
use rust_decimal_macros::dec;

const MASTERS: &[u8] = include_bytes!("fixtures/gst_masters.xml");
const BILLWISE: &[u8] = include_bytes!("fixtures/billwise_daybook.xml");
const BANKING: &[u8] = include_bytes!("fixtures/banking_daybook.xml");
const NUMBERING: &[u8] = include_bytes!("fixtures/numbering_daybook.xml");
const LATE: &[u8] = include_bytes!("fixtures/late_daybook.xml");
const TRIAL_BALANCE: &[u8] = include_bytes!("fixtures/trial_balance.xml");
const CANCELLED: &[u8] = include_bytes!("fixtures/cancelled_daybook.xml");

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// A new Indian company as the desktop app creates one: country, an
/// April year, and (by default here) numbering that never restarts.
fn company() -> Book {
    let mut book = Book::new("Saffron Lane Traders");
    book.set_country(now(), "u", "IN").unwrap();
    book.set_fy_start_month(now(), "u", 4).unwrap();
    book
}

fn import_into(book: &mut Book, docs: &[&[u8]]) -> ImportReport {
    import_all(docs.iter().copied(), book, now(), "importer").unwrap()
}

fn balance(book: &Book, ledger: &str) -> Amount {
    let id = book.ledger_by_name(ledger).unwrap_or_else(|| panic!("no ledger {ledger}")).id;
    book.balances(None)[&id]
}

fn notes(report: &ImportReport, kind: NoteKind) -> Vec<&str> {
    report.notes_of(kind).map(|n| n.name.as_str()).collect()
}

fn skipped(report: &ImportReport, kind: ItemKind) -> Vec<(&str, &SkipReason)> {
    report.skipped_of(kind).map(|s| (s.name.as_str(), &s.reason)).collect()
}

#[test]
fn bill_wise_vouchers_and_opening_bills_give_tallys_outstandings() {
    let mut book = company();
    let report = import_into(&mut book, &[MASTERS, BILLWISE]);
    assert_eq!(report.vouchers_posted, 8, "{:#?}", report.skipped);
    assert!(skipped(&report, ItemKind::Voucher).is_empty());

    // Opening bills on the two bill-wise parties.
    let kavya = book.ledger_by_name("Kavya Fabrics").unwrap();
    let refs: Vec<_> = kavya.opening_bills.iter().map(|b| (b.reference.as_str(), b.amount, b.date, b.due)).collect();
    assert_eq!(
        refs,
        [
            ("OB/101", dec!(100000), Some(d(2024, 2, 10)), Some(d(2024, 3, 11))),
            ("OB/102", dec!(50000), Some(d(2024, 3, 15)), None)
        ]
    );
    let sahyadri = book.ledger_by_name("Sahyadri Supplies").unwrap();
    assert_eq!(sahyadri.opening_bills[1].due, Some(d(2024, 4, 15)), "a credit period given as a date");
    assert_eq!(notes(&report, NoteKind::OpeningBills), ["Sahyadri Supplies"], "bills cover 55,000 of 60,000");

    // Bill counts, and the line whose bills mix debits and credits.
    let b = report.bills;
    assert_eq!((b.new_ref, b.against, b.advance, b.on_account), (3, 4, 1, 1));
    assert_eq!((b.opening, b.opening_ledgers), (4, 2));
    assert_eq!(notes(&report, NoteKind::BillsDropped), ["Journal 1 on 2024-05-02"]);

    // Outstandings bill by bill, as Tally's Bills Receivable and Payable show them.
    let open = outstanding_bills(&book, d(2024, 5, 31));
    let rows: Vec<_> = open.iter().map(|o| (o.party.as_str(), o.reference.as_str(), o.amount, o.due)).collect();
    assert_eq!(
        rows,
        [
            ("Kavya Fabrics", "OB/102", dec!(50000), None),
            ("Kavya Fabrics", "SG/1", dec!(6200), Some(d(2024, 5, 5))),
            ("Kavya Fabrics", "On Account", dec!(-1000), None),
            ("Nilgiri Exports", "SE/1", dec!(3800), Some(d(2024, 6, 24))),
            ("Sahyadri Supplies", "Opening Balance", dec!(-5000), None),
            ("Sahyadri Supplies", "PB-12", dec!(-15000), Some(d(2024, 4, 15))),
            ("Sahyadri Supplies", "SS/778", dec!(-22400), Some(d(2024, 5, 15))),
        ]
    );
    // Outstandings add up to the ledger balances.
    for party in ["Kavya Fabrics", "Nilgiri Exports", "Sahyadri Supplies"] {
        let total: Amount = open.iter().filter(|o| o.party == party).map(|o| o.amount).sum();
        assert_eq!(total, balance(&book, party), "{party}");
    }

    // Bank dates come across; the cost centre split is reported.
    let receipt = book.vouchers().find(|v| v.kind == VoucherKind::Receipt && v.number == "1").unwrap();
    let bank = book.ledger_by_name("SBI Current").unwrap().id;
    assert_eq!(receipt.entries.iter().find(|e| e.ledger == bank).unwrap().bank_date, Some(d(2024, 4, 22)));
    let paid = book.vouchers().find(|v| v.series.as_deref() == Some("Bank Payment")).unwrap();
    assert!(paid.entries.iter().any(|e| e.bank_date == Some(d(2024, 5, 2))), "a bank date on the cash credit account");
    assert_eq!(report.notes_of(NoteKind::CostCentres).map(|n| n.count).sum::<usize>(), 1);

    let replayed = Book::replay("Saffron Lane Traders", book.audit().clone()).unwrap();
    assert_eq!(outstanding_bills(&replayed, d(2024, 5, 31)), open);
}

#[test]
fn gst_masters_tag_parties_tax_ledgers_and_rates() {
    let mut book = company();
    let report = import_into(&mut book, &[MASTERS, BILLWISE]);
    let ledger = |name: &str| book.ledger_by_name(name).unwrap_or_else(|| panic!("no ledger {name}"));

    let kavya = ledger("Kavya Fabrics");
    assert_eq!(kavya.tax_id.as_deref(), Some("27AAKFK1234M1Z5"));
    assert_eq!(kavya.tax_id_override, None);
    assert_eq!(kavya.pan.as_deref(), Some("AAKFK1234M"));
    assert_eq!(kavya.credit_days, Some(30));
    let mail = kavya.mailing.as_ref().unwrap();
    assert_eq!((mail.region.as_deref(), mail.postal_code.as_str()), (Some("27"), "421302"));
    assert_eq!(mail.address, "Shop 4, Cloth Market\nBhiwandi");
    // The newer layout: dated registration and mailing lists.
    let nilgiri = ledger("Nilgiri Exports");
    assert_eq!(nilgiri.tax_id.as_deref(), Some("24AAGCN5678P1ZO"));
    assert_eq!(nilgiri.mailing.as_ref().unwrap().region.as_deref(), Some("24"));
    assert_eq!(ledger("Sahyadri Supplies").mailing.as_ref().unwrap().region.as_deref(), Some("29"));

    // Problems are kept or left out, and said.
    let ojas = ledger("Ojas Retail");
    assert_eq!(ojas.tax_id.as_deref(), Some("27AAACP9876R1ZB"), "kept, as Tally had it");
    assert!(ojas.tax_id_override.as_deref().unwrap().contains("check character"));
    assert_eq!(ojas.pan, None);
    assert_eq!(notes(&report, NoteKind::GstinInvalid), ["Ojas Retail"]);
    assert_eq!(notes(&report, NoteKind::StateUnknown), ["Ojas Retail"]);
    assert_eq!(notes(&report, NoteKind::PanInvalid), ["Ojas Retail"]);
    assert!(notes(&report, NoteKind::DetailNotKept).contains(&"Lumen Trading LLC"), "a country outside India");

    // Duty ledgers: by name, by use, or guessed when used on both sides.
    let tax = |name: &str| ledger(name).tax.as_ref().map(|t| (t.component.as_str(), t.side));
    assert_eq!(tax("Output SGST"), Some(("SGST", TaxSide::Output)));
    assert_eq!(tax("Input SGST"), Some(("SGST", TaxSide::Input)));
    assert_eq!(tax("IGST"), Some(("IGST", TaxSide::Output)), "used on sales only");
    assert_eq!(tax("CGST"), Some(("CGST", TaxSide::Input)), "used on both sides");
    assert_eq!(notes(&report, NoteKind::TaxSideGuessed), ["CGST"]);
    assert_eq!(tax("GST Cess"), None, "not under Duties & Taxes");
    assert_eq!(notes(&report, NoteKind::DutyLedgerNotTagged), ["GST Cess"]);
    assert_eq!(report.gst.duty_ledgers, 4);

    // HSN/SAC on ledgers, and rates by code and date.
    assert_eq!(ledger("Sales Local").tax_class.as_deref(), Some("5208"));
    assert_eq!(ledger("Freight Charges").tax_class.as_deref(), Some("996511"));
    assert_eq!(book.tax_rate_from("5208", d(2017, 7, 1)), Some(dec!(5)), "the first master's rate is kept");
    assert_eq!(notes(&report, NoteKind::TaxRateConflict), ["Cotton Fabric"]);
    assert_eq!(book.tax_rate_from("996511", d(2017, 7, 1)), Some(dec!(18)), "CGST plus SGST");
    assert_eq!(book.tax_rate_from("5007", d(2022, 1, 1)), Some(dec!(5)), "the latest rate");
    assert_eq!(notes(&report, NoteKind::GstClassification), ["Gift Box"]);
    assert_eq!(report.gst.rates, 3);
    assert_eq!((report.gst.gstins, report.gst.states, report.gst.pans), (4, 3, 1));
    assert_eq!(book.stock_items().find(|i| i.name == "Silk Saree").unwrap().tax_class.as_deref(), Some("5007"));

    assert_eq!(book.opening_difference(), Amount::ZERO);
    Book::replay("Saffron Lane Traders", book.audit().clone()).unwrap();
}

#[test]
fn overdraft_and_cash_credit_accounts_work_as_banks() {
    let mut book = company();
    let report = import_into(&mut book, &[MASTERS, BANKING]);
    assert_eq!(report.vouchers_posted, 4);
    assert_eq!(skipped(&report, ItemKind::Voucher), [("Contra 3 on 2024-04-21", &SkipReason::ContraNonCash)]);
    for name in ["SBI Cash Credit", "HDFC Overdraft"] {
        assert!(book.is_cash_or_bank(book.ledger_by_name(name).unwrap().id), "{name}");
    }
    assert_eq!(balance(&book, "SBI Cash Credit"), dec!(-330000));
    assert_eq!(balance(&book, "HDFC Overdraft"), dec!(-60500));
    assert_eq!(balance(&book, "SBI Current"), dec!(250000));
    assert_eq!(balance(&book, "Cash"), dec!(35000));
    // A bank date before its voucher's date is dropped and counted.
    assert_eq!(report.notes_of(NoteKind::BankDateDropped).map(|n| n.count).sum::<usize>(), 1);
    let cc = book.ledger_by_name("SBI Cash Credit").unwrap().id;
    let receipt = book.vouchers().find(|v| v.kind == VoucherKind::Receipt).unwrap();
    assert_eq!(receipt.entries.iter().find(|e| e.ledger == cc).unwrap().bank_date, Some(d(2024, 4, 20)));
    Book::replay("Saffron Lane Traders", book.audit().clone()).unwrap();
}

#[test]
fn custom_voucher_types_keep_their_own_numbers() {
    let mut book = company();
    assert_eq!(book.numbering(VoucherKind::Sales).restart, Restart::Never);
    let report = import_into(&mut book, &[MASTERS, NUMBERING]);
    assert_eq!(report.vouchers_posted, 8, "{:#?}", report.skipped);
    assert!(skipped(&report, ItemKind::Voucher).is_empty());

    // Each custom type is its own series; Tally's own Sales type has none.
    let ones: Vec<_> = book
        .vouchers()
        .filter(|v| v.number == "1" && v.date < d(2025, 4, 1))
        .map(|v| v.series.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(ones, [None, Some("Cash Sales".into()), Some("Sales Export".into()), Some("Sales GST".into())]);

    // Tally restarted Sales GST at 1 the next year: numbering now restarts yearly.
    assert_eq!(notes(&report, NoteKind::YearlyNumbering), ["Sales"]);
    assert_eq!(book.numbering(VoucherKind::Sales).restart, Restart::Yearly);
    // A number Tally repeated in one year gets a new one, and the report says so.
    assert_eq!(notes(&report, NoteKind::Renumbered), ["Cash Sales 2 on 2024-05-02"]);
    let again = book.vouchers().find(|v| v.date == d(2024, 5, 2)).unwrap();
    assert_eq!((again.number.as_str(), again.series.as_deref()), ("2 (2)", Some("Cash Sales")));
    assert_eq!(report.vouchers_numbered, 1);
    let types: Vec<_> = report.by_type.iter().map(|(name, t)| (name.as_str(), t.kind, t.posted)).collect();
    assert_eq!(
        types,
        [
            ("Cash Sales", Some(VoucherKind::Sales), 3),
            ("Sales", Some(VoucherKind::Sales), 1),
            ("Sales Export", Some(VoucherKind::Sales), 1),
            ("Sales GST", Some(VoucherKind::Sales), 3),
        ]
    );

    // Importing the same files again posts nothing numbered twice.
    let second = import_into(&mut book, &[MASTERS, NUMBERING]);
    let reasons: Vec<_> = second.skipped_of(ItemKind::Voucher).map(|s| &s.reason).collect();
    assert_eq!(reasons.len(), 7);
    assert!(reasons.iter().all(|r| matches!(r, SkipReason::AlreadyInBooks(_))), "{reasons:?}");
    assert_eq!(second.vouchers_posted, 1, "only the voucher Tally left unnumbered");
    Book::replay("Saffron Lane Traders", book.audit().clone()).unwrap();
}

#[test]
fn without_a_country_repeats_across_years_are_renumbered_and_said() {
    let mut book = Book::new("No Profile Traders");
    let report = import_into(&mut book, &[MASTERS, NUMBERING]);
    assert_eq!(report.vouchers_posted, 8);
    let renumbered: Vec<_> = report.notes_of(NoteKind::Renumbered).map(|n| n.name.as_str()).collect();
    assert_eq!(renumbered, ["Cash Sales 2 on 2024-05-02", "Sales GST 1 on 2025-04-02"]);
    assert!(report.notes_of(NoteKind::YearlyNumbering).next().unwrap().detail.contains("could not be set"));
    assert_eq!(report.notes_of(NoteKind::TaxRatesNotKept).map(|n| n.count).sum::<usize>(), 3);
}

#[test]
fn a_late_day_book_is_warned_and_balances_are_checked_against_tally() {
    let mut book = company();
    let report = import_into(&mut book, &[MASTERS, LATE, TRIAL_BALANCE]);
    assert_eq!(report.vouchers_posted, 2);
    assert_eq!((report.first_date, report.last_date), (Some(d(2024, 10, 15)), Some(d(2024, 11, 2))));
    let late = report.notes_of(NoteKind::DayBookStartsLate).next().expect("warned");
    assert!(late.detail.contains("15-Oct-2024") && late.detail.contains("01-Apr-2024"), "{}", late.detail);

    // Tally's Trial Balance: Cash differs because April to September is missing.
    let check = &report.balances;
    assert_eq!((check.compared, check.matched), (4, 3), "Cash, Kavya Fabrics, Sundry Creditors (a group), Bank Charges");
    assert_eq!(check.differences.len(), 1);
    assert_eq!((check.differences[0].name.as_str(), check.differences[0].tally, check.differences[0].khataos), ("Cash", dec!(31250), dec!(29750)));
    assert_eq!(notes(&report, NoteKind::BalanceDiffers), ["Cash"]);
    assert_eq!(notes(&report, NoteKind::BalanceNotFound), ["Opening Stock"]);
    assert!(report.balance_warnings() >= 2);
    assert_eq!(report.opening_difference, Amount::ZERO, "the Trial Balance still agrees");
}

#[test]
fn closing_balances_on_ledger_masters_are_compared_too() {
    let xml = br#"<ENVELOPE><HEADER><TALLYREQUEST>Import Data</TALLYREQUEST></HEADER><BODY><IMPORTDATA><REQUESTDATA><TALLYMESSAGE>
        <LEDGER NAME="Cash"><PARENT>Cash-in-Hand</PARENT><OPENINGBALANCE>-500.00</OPENINGBALANCE><CLOSINGBALANCE>-700.00</CLOSINGBALANCE></LEDGER>
        <LEDGER NAME="Capital"><PARENT>Capital Account</PARENT><OPENINGBALANCE>500.00</OPENINGBALANCE><CLOSINGBALANCE>500.00</CLOSINGBALANCE></LEDGER>
        </TALLYMESSAGE></REQUESTDATA></IMPORTDATA></BODY></ENVELOPE>"#;
    let mut book = company();
    let report = import(xml, &mut book, now(), "u").unwrap();
    assert_eq!((report.balances.compared, report.balances.matched), (2, 1));
    assert_eq!(notes(&report, NoteKind::BalanceDiffers), ["Cash"]);
}

#[test]
fn a_day_book_without_masters_or_openings_is_warned() {
    let mut book = company();
    let cash = book.group_by_name("Cash-in-Hand").unwrap().id;
    let expenses = book.group_by_name("Indirect Expenses").unwrap().id;
    let debtors = book.group_by_name("Sundry Debtors").unwrap().id;
    book.create_ledger(now(), "u", "Cash", cash).unwrap();
    book.create_ledger(now(), "u", "Bank Charges", expenses).unwrap();
    book.create_ledger(now(), "u", "Kavya Fabrics", debtors).unwrap();
    let report = import_into(&mut book, &[LATE]);
    assert_eq!(report.vouchers_posted, 2);
    assert_eq!(report.notes_of(NoteKind::NoLedgerMasters).count(), 1);
}

#[test]
fn cancelled_post_dated_deleted_and_optional_vouchers() {
    let mut book = company();
    let report = import_into(&mut book, &[MASTERS, CANCELLED]);
    assert_eq!((report.vouchers_posted, report.vouchers_cancelled), (2, 2));
    assert_eq!(
        skipped(&report, ItemKind::Voucher),
        [("Payment 5 on 2024-04-12", &SkipReason::Deleted), ("Journal J-9 on 2024-04-13", &SkipReason::Optional)]
    );
    let five = book.vouchers().find(|v| v.number == "5" && v.kind == VoucherKind::Sales).unwrap();
    assert!(five.cancelled && five.entries.is_empty());
    assert_eq!((five.narration.as_str(), five.series.as_deref()), ("Cancelled: wrong party", Some("Sales GST")));
    let six = book.vouchers().find(|v| v.number == "6").unwrap();
    assert!(six.cancelled && six.entries.len() == 2, "kept with its lines, for the record");
    assert_eq!(balance(&book, "Kavya Fabrics"), dec!(150700), "only Sales GST 7 counts");
    let t = &report.by_type["Sales GST"];
    assert_eq!((t.posted, t.cancelled, t.skipped), (1, 2, 0));

    // The cancelled numbers stay taken.
    let kavya = book.ledger_by_name("Kavya Fabrics").unwrap().id;
    let sales = book.ledger_by_name("Sales Local").unwrap().id;
    let reuse = Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Sales,
        number: "5".into(),
        date: d(2024, 4, 20),
        entries: vec![Entry::dr(kavya, dec!(1)), Entry::cr(sales, dec!(1))],
        narration: String::new(),
        cancelled: false,
        series: Some("Sales GST".into()),
        stock: Vec::new(),
    };
    assert_eq!(book.post_voucher(now(), "u", reuse), Err(BookError::DuplicateVoucherNumber("5".into())));

    // The post-dated cheque is posted on its date, with a note.
    assert_eq!(notes(&report, NoteKind::PostDated), ["Payment 4 on 2024-06-30"]);
    let charges = book.ledger_by_name("Bank Charges").unwrap().id;
    assert_eq!(book.balances(Some(d(2024, 6, 29)))[&charges], Amount::ZERO);
    assert_eq!(book.balances(Some(d(2024, 6, 30)))[&charges], dec!(300));
    Book::replay("Saffron Lane Traders", book.audit().clone()).unwrap();
}

#[test]
fn the_shareable_summary_has_counts_but_no_names_or_amounts() {
    let mut book = company();
    let report = import_into(&mut book, &[MASTERS, BILLWISE, TRIAL_BALANCE]);
    let files = ["saffron masters.xml".to_owned(), "saffron daybook.xml".to_owned()];
    let full = report.to_text(&files, false);
    for secret in ["Kavya Fabrics", "Sales GST", "saffron", "27AAACP9876R1ZB"] {
        assert!(full.contains(secret), "the full report names {secret}");
    }
    for shared in [report.to_text(&files, true), report.to_csv(&files, true)] {
        for secret in ["Kavya", "Sahyadri", "Nilgiri", "Ojas", "Sales GST", "Bank Payment", "SG/1", "OB/1", "saffron", "27AAKFK", "27AAACP", "Cash"] {
            assert!(!shared.contains(secret), "{secret} in:\n{shared}");
        }
        assert!(!shared.chars().zip(shared.chars().skip(1)).any(|(a, b)| a == '.' && b.is_ascii_digit()), "an amount in:\n{shared}");
        assert!(shared.contains("Sales: posted"), "vouchers by KhataOS type");
        assert!(shared.contains("Voucher lines whose bill allocations could not be kept"));
    }
    let csv = report.to_csv(&files, false);
    assert!(csv.starts_with("Section,Item,Count,Name,Detail\r\n"));
    assert!(csv.contains("\"Shop 4") || csv.contains("Kavya Fabrics"));
}

#[test]
fn compound_unit_openings_say_so() {
    let xml = br#"<ENVELOPE><BODY><DATA><TALLYMESSAGE>
        <STOCKITEM NAME="Tile"><BASEUNITS>Box</BASEUNITS><OPENINGBALANCE>5 Box of 10 Nos</OPENINGBALANCE></STOCKITEM>
        </TALLYMESSAGE></DATA></BODY></ENVELOPE>"#;
    let mut book = Book::new("Tiles");
    let report = import(xml, &mut book, now(), "u").unwrap();
    assert_eq!(skipped(&report, ItemKind::StockItem), [("Tile", &SkipReason::CompoundItemUnit("Box".into()))]);
}

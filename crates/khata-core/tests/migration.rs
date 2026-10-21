//! Rules that a migration from Tally relies on: overdraft and cash credit
//! accounts count as bank, named number series within a voucher type,
//! opening bills on party ledgers, and cancelled vouchers without lines.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::reports::{ageing, outstanding_bills};
use khata_core::{Book, BookError, Entry, LedgerId, OpeningBill, Voucher, VoucherId, VoucherKind};
use rust_decimal_macros::dec;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn ledger(book: &mut Book, name: &str, group: &str) -> LedgerId {
    let g = book.group_by_name(group).unwrap().id;
    book.create_ledger(now(), "u", name, g).unwrap()
}

fn voucher(kind: VoucherKind, number: &str, date: NaiveDate, entries: Vec<Entry>) -> Voucher {
    Voucher {
        id: VoucherId::new(),
        kind,
        number: number.into(),
        date,
        entries,
        narration: String::new(),
        cancelled: false,
        series: None,
        stock: Vec::new(),
    }
}

#[test]
fn overdraft_and_cash_credit_accounts_count_as_bank() {
    let mut book = Book::new("CC Traders");
    let current = ledger(&mut book, "SBI Current", "Bank Accounts");
    let cc = ledger(&mut book, "SBI Cash Credit", "Bank OCC A/c");
    let od = ledger(&mut book, "HDFC Overdraft", "Bank OD A/c");
    let term = ledger(&mut book, "Term Loan", "Secured Loans");
    let rent = ledger(&mut book, "Rent", "Indirect Expenses");
    // A sub-group of Bank OCC A/c counts too.
    let occ = book.group_by_name("Bank OCC A/c").unwrap().id;
    let sub = book.create_group(now(), "u", "Hypothecation Limits", occ).unwrap();
    let limit = book.create_ledger(now(), "u", "PNB Limit", sub).unwrap();

    for id in [current, cc, od, limit] {
        assert!(book.is_cash_or_bank(id));
    }
    assert!(!book.is_cash_or_bank(term), "other loan groups are unchanged");
    assert!(!book.is_cash_or_bank(rent));
    assert!(!book.group_is_cash_or_bank(book.group_by_name("Secured Loans").unwrap().id));
    assert!(!book.group_is_cash_or_bank(book.group_by_name("Loans (Liability)").unwrap().id));
    // Nature and the stored flag stay as they were, so old files replay.
    let od_group = book.group_by_name("Bank OD A/c").unwrap();
    assert!(!od_group.is_cash_or_bank);
    assert_eq!(od_group.nature, khata_core::Nature::Liabilities);

    // Contra between the current account and the CC and OD accounts.
    book.post_voucher(now(), "u", voucher(VoucherKind::Contra, "1", d(2024, 4, 2), vec![Entry::dr(current, dec!(5000)), Entry::cr(cc, dec!(5000))]))
        .unwrap();
    book.post_voucher(now(), "u", voucher(VoucherKind::Contra, "2", d(2024, 4, 3), vec![Entry::dr(od, dec!(700)), Entry::cr(limit, dec!(700))]))
        .unwrap();
    // A payment from the CC account, reconciled with a bank date.
    let mut pay = voucher(VoucherKind::Payment, "1", d(2024, 4, 4), vec![Entry::dr(rent, dec!(900)), Entry::cr(cc, dec!(900))]);
    pay.entries[1].bank_date = Some(d(2024, 4, 6));
    book.post_voucher(now(), "u", pay).unwrap();
    // A term loan is still not a bank account.
    let bad = voucher(VoucherKind::Contra, "3", d(2024, 4, 5), vec![Entry::dr(current, dec!(1)), Entry::cr(term, dec!(1))]);
    assert_eq!(book.post_voucher(now(), "u", bad), Err(BookError::ContraNonCashLedger));

    let replayed = Book::replay("CC Traders", book.audit().clone()).unwrap();
    assert_eq!(replayed.trial_balance(), book.trial_balance());
}

#[test]
fn named_series_each_have_their_own_numbers() {
    let mut book = Book::new("Series Traders");
    let party = ledger(&mut book, "Asha", "Sundry Debtors");
    let sales = ledger(&mut book, "Sales", "Sales Accounts");
    let sale = |number: &str, series: Option<&str>, date| Voucher {
        series: series.map(str::to_owned),
        ..voucher(VoucherKind::Sales, number, date, vec![Entry::dr(party, dec!(100)), Entry::cr(sales, dec!(100))])
    };
    book.post_voucher(now(), "u", sale("1", Some("Sales GST"), d(2024, 4, 1))).unwrap();
    book.post_voucher(now(), "u", sale("1", Some("Cash Sales"), d(2024, 4, 1))).unwrap();
    book.post_voucher(now(), "u", sale("1", None, d(2024, 4, 1))).unwrap();
    // The same series, ignoring case, still refuses a repeat.
    assert_eq!(
        book.post_voucher(now(), "u", sale("1", Some("sales gst"), d(2024, 5, 1))),
        Err(BookError::DuplicateVoucherNumber("1".into()))
    );
    assert!(matches!(book.post_voucher(now(), "u", sale("2", Some(" "), d(2024, 5, 1))), Err(BookError::InvalidNumbering(_))));
    assert!(matches!(book.post_voucher(now(), "u", sale("2", Some(&"x".repeat(61)), d(2024, 5, 1))), Err(BookError::InvalidNumbering(_))));
    // With yearly numbering, each series restarts each year.
    book.set_country(now(), "u", "IN").unwrap();
    book.set_fy_start_month(now(), "u", 4).unwrap();
    let yearly = khata_core::NumberSeries { restart: khata_core::Restart::Yearly, ..Default::default() };
    book.set_numbering(now(), "u", VoucherKind::Sales, yearly).unwrap();
    book.post_voucher(now(), "u", sale("1", Some("Sales GST"), d(2025, 4, 1))).unwrap();
    // Without a series name the voucher's JSON is what it was before series existed.
    let plain = book.vouchers().find(|v| v.series.is_none()).unwrap();
    assert!(serde_json::to_value(plain).unwrap().get("series").is_none());

    let replayed = Book::replay("Series Traders", book.audit().clone()).unwrap();
    assert_eq!(replayed.vouchers().count(), 4);
    assert_eq!(replayed.vouchers().filter(|v| v.series.as_deref() == Some("Sales GST")).count(), 2);
}

#[test]
fn opening_bills_feed_outstandings_and_are_settled_by_reference() {
    let mut book = Book::new("Bills Traders");
    let party = ledger(&mut book, "Mehta Stores", "Sundry Debtors");
    let capital = ledger(&mut book, "Capital", "Capital Account");
    let bank = ledger(&mut book, "Bank", "Bank Accounts");
    book.set_opening_balance(now(), "u", party, dec!(15000)).unwrap();
    book.set_opening_balance(now(), "u", capital, dec!(-15000)).unwrap();
    let bills = vec![
        OpeningBill { reference: "OB-1".into(), date: Some(d(2024, 2, 1)), due: Some(d(2024, 3, 2)), amount: dec!(10000), advance: false },
        OpeningBill { reference: "OB-2".into(), date: Some(d(2024, 3, 20)), due: None, amount: dec!(4000), advance: false },
    ];
    let with_bills = khata_core::Ledger { opening_bills: bills, ..book.ledger(party).unwrap().clone() };
    book.alter_ledger(now(), "u", with_bills).unwrap();

    // Part of OB-1 is received after the books begin.
    let receipt = voucher(
        VoucherKind::Receipt,
        "1",
        d(2024, 4, 10),
        vec![Entry::dr(bank, dec!(6000)), Entry::cr(party, dec!(6000)).bill(khata_core::BillKind::Against, "OB-1", dec!(6000), None)],
    );
    book.post_voucher(now(), "u", receipt).unwrap();

    let open = outstanding_bills(&book, d(2024, 4, 30));
    let rows: Vec<(&str, _, _)> = open.iter().map(|b| (b.reference.as_str(), b.amount, b.overdue_days)).collect();
    assert_eq!(
        rows,
        [("Opening Balance", dec!(1000), 0), ("OB-1", dec!(4000), 59), ("OB-2", dec!(4000), 41)],
        "the 1,000 the bills don't cover stays as one opening line"
    );
    let age = ageing(&book, d(2024, 4, 30), true);
    assert_eq!(age[0].buckets, [dec!(1000), dec!(8000), dec!(0), dec!(0)]);

    // Shape checks.
    let bad = |b: OpeningBill| {
        let l = khata_core::Ledger { opening_bills: vec![b], ..book.ledger(party).unwrap().clone() };
        book.clone().alter_ledger(now(), "u", l)
    };
    let ok = OpeningBill { reference: "X".into(), date: Some(d(2024, 3, 1)), due: None, amount: dec!(1), advance: false };
    assert_eq!(bad(OpeningBill { reference: " ".into(), ..ok.clone() }), Err(BookError::InvalidOpeningBill));
    assert_eq!(bad(OpeningBill { amount: dec!(0), ..ok.clone() }), Err(BookError::InvalidOpeningBill));
    assert_eq!(bad(OpeningBill { due: Some(d(2024, 2, 1)), ..ok.clone() }), Err(BookError::InvalidOpeningBill));
    assert!(bad(ok).is_ok());

    let replayed = Book::replay("Bills Traders", book.audit().clone()).unwrap();
    assert_eq!(outstanding_bills(&replayed, d(2024, 4, 30)), open);
}

#[test]
fn a_cancelled_voucher_may_keep_its_number_without_lines() {
    let mut book = Book::new("Cancel Traders");
    let party = ledger(&mut book, "Asha", "Sundry Debtors");
    let sales = ledger(&mut book, "Sales", "Sales Accounts");
    let empty = Voucher { cancelled: true, ..voucher(VoucherKind::Sales, "7", d(2024, 4, 1), Vec::new()) };
    book.post_voucher(now(), "u", empty).unwrap();
    let again = voucher(VoucherKind::Sales, "7", d(2024, 4, 2), vec![Entry::dr(party, dec!(1)), Entry::cr(sales, dec!(1))]);
    assert_eq!(book.post_voucher(now(), "u", again), Err(BookError::DuplicateVoucherNumber("7".into())), "the number stays taken");
    let not_cancelled = voucher(VoucherKind::Sales, "8", d(2024, 4, 1), Vec::new());
    assert!(matches!(book.post_voucher(now(), "u", not_cancelled), Err(BookError::Voucher(_))));
    assert!(book.trial_balance().is_empty());
    Book::replay("Cancel Traders", book.audit().clone()).unwrap();
}

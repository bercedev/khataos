//! Voucher numbering series: per-year uniqueness, safe changes, replay.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{AuditLog, Book, BookError, Entry, LedgerId, NumberSeries, Restart, Voucher, VoucherId, VoucherKind};
use rust_decimal_macros::dec;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn setup() -> (Book, LedgerId, LedgerId) {
    let mut book = Book::new("Series Co");
    let party = book.create_ledger(now(), "u", "Asha", book.group_by_name("Sundry Debtors").unwrap().id).unwrap();
    let sales = book.create_ledger(now(), "u", "Sales", book.group_by_name("Sales Accounts").unwrap().id).unwrap();
    book.set_country(now(), "u", "IN").unwrap();
    book.set_fy_start_month(now(), "u", 4).unwrap();
    (book, party, sales)
}

fn sale(book: &mut Book, party: LedgerId, sales: LedgerId, number: &str, date: NaiveDate) -> Result<VoucherId, BookError> {
    book.post_voucher(now(), "u", Voucher {
        id: VoucherId::new(),
        kind: VoucherKind::Sales,
        number: number.into(),
        date,
        entries: vec![Entry::dr(party, dec!(100)), Entry::cr(sales, dec!(100))],
        narration: String::new(),
        cancelled: false, series: None, stock: Vec::new(),
    })
}

#[test]
fn plain_numbers_stay_unique_forever_until_a_yearly_series_is_set() {
    let (mut book, party, sales) = setup();
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2026, 3, 1)), "1");
    sale(&mut book, party, sales, "1", d(2026, 3, 1)).unwrap();
    // Without a series, "1" can't be reused in the next financial year.
    assert_eq!(sale(&mut book, party, sales, "1", d(2026, 4, 2)), Err(BookError::DuplicateVoucherNumber("1".into())));

    // Plain numbers restarting each April (India: unique per financial year).
    let yearly = NumberSeries { restart: Restart::Yearly, ..NumberSeries::default() };
    book.set_numbering(now(), "u", VoucherKind::Sales, yearly).unwrap();
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2026, 4, 2)), "1");
    sale(&mut book, party, sales, "1", d(2026, 4, 2)).unwrap();
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2026, 4, 3)), "2");
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2026, 3, 31)), "2", "last year's series continues");
    assert_eq!(sale(&mut book, party, sales, "1", d(2027, 3, 31)), Err(BookError::DuplicateVoucherNumber("1".into())));

    // Going back to one series would make the two "1"s clash: refused.
    assert!(matches!(
        book.set_numbering(now(), "u", VoucherKind::Sales, NumberSeries::default()),
        Err(BookError::InvalidNumbering(_))
    ));
}

#[test]
fn year_prefixed_series_and_replay() {
    let (mut book, party, sales) = setup();
    book.set_numbering(now(), "u", VoucherKind::Sales, NumberSeries::yearly_with_year()).unwrap();
    let n = book.next_voucher_number(VoucherKind::Sales, d(2026, 4, 1));
    assert_eq!(n, "26-27/1");
    sale(&mut book, party, sales, &n, d(2026, 4, 1)).unwrap();
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2026, 9, 1)), "26-27/2");
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2027, 4, 1)), "27-28/1");
    // Numbers typed by hand that don't follow the series don't break it.
    sale(&mut book, party, sales, "SPECIAL-9", d(2026, 5, 1)).unwrap();
    assert_eq!(book.next_voucher_number(VoucherKind::Sales, d(2026, 9, 1)), "26-27/2");
    // Other voucher types keep their own plain series.
    assert_eq!(book.next_voucher_number(VoucherKind::Receipt, d(2026, 9, 1)), "1");

    let log: AuditLog = serde_json::from_value(serde_json::to_value(book.audit()).unwrap()).unwrap();
    let again = Book::replay("Series Co", log).unwrap();
    assert_eq!(again.numbering(VoucherKind::Sales), NumberSeries::yearly_with_year());
    assert_eq!(again.next_voucher_number(VoucherKind::Sales, d(2026, 9, 1)), "26-27/2");

    // Bad settings are refused.
    let bad = NumberSeries { prefix: "{WEEK}".into(), ..NumberSeries::default() };
    assert!(matches!(book.set_numbering(now(), "u", VoucherKind::Sales, bad), Err(BookError::InvalidNumbering(_))));
    assert!(matches!(book.set_fy_start_month(now(), "u", 13), Err(BookError::InvalidNumbering(_))));
}

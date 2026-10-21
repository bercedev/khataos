//! The narration `Ctrl+R` and `Alt+R` repeat in a new voucher.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::{Book, Entry, LedgerId, Voucher, VoucherId, VoucherKind};
use rust_decimal_macros::dec;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

fn ledger(book: &mut Book, name: &str, group: &str) -> LedgerId {
    let g = book.group_by_name(group).unwrap().id;
    book.create_ledger(now(), "owner", name, g).unwrap()
}

fn post(book: &mut Book, kind: VoucherKind, date: NaiveDate, dr: LedgerId, cr: LedgerId, narration: &str) -> VoucherId {
    let v = Voucher {
        id: VoucherId::new(),
        kind,
        number: (book.active_vouchers().count() + 1).to_string(),
        date,
        entries: vec![Entry::dr(dr, dec!(100)), Entry::cr(cr, dec!(100))],
        narration: narration.into(),
        cancelled: false,
        series: None,
        stock: Vec::new(),
    };
    book.post_voucher(now(), "owner", v).unwrap()
}

#[test]
fn last_narration_is_the_latest_by_date_then_entry_order() {
    use VoucherKind::{Journal, Payment, Receipt};
    let mut book = Book::new("Narrations");
    let bank = ledger(&mut book, "HDFC Bank", "Bank Accounts");
    let rent = ledger(&mut book, "Rent", "Indirect Expenses");
    let salary = ledger(&mut book, "Salary", "Indirect Expenses");
    let customer = ledger(&mut book, "Asha Stores", "Sundry Debtors");

    // In entry order.
    post(&mut book, Payment, d(4, 1), rent, bank, "Rent for April");
    post(&mut book, Payment, d(4, 5), salary, bank, "Salary for April");
    post(&mut book, Journal, d(4, 10), rent, salary, "Rent provision");
    post(&mut book, Payment, d(4, 3), rent, bank, "Entered late for an earlier date");
    post(&mut book, Payment, d(4, 5), salary, bank, "Same day, entered later");
    post(&mut book, Payment, d(4, 20), rent, bank, "  ");
    let cancelled = post(&mut book, Payment, d(4, 25), rent, bank, "Cancelled later");
    book.cancel_voucher(now(), "owner", cancelled).unwrap();

    // Ctrl+R: by type. The later date wins; on the same date, the later entry. Blank
    // and cancelled vouchers don't count.
    assert_eq!(book.last_narration(Some(Payment), None), Some("Same day, entered later"));
    assert_eq!(book.last_narration(Some(Journal), None), Some("Rent provision"));
    // Alt+R: by ledger, any type.
    assert_eq!(book.last_narration(None, Some(rent)), Some("Rent provision"));
    assert_eq!(book.last_narration(None, Some(salary)), Some("Rent provision"));
    // Both, if asked.
    assert_eq!(book.last_narration(Some(Payment), Some(rent)), Some("Entered late for an earlier date"));
    // Nothing earlier.
    assert_eq!(book.last_narration(Some(Receipt), None), None);
    assert_eq!(book.last_narration(None, Some(customer)), None);
}

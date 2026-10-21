//! A large synthetic import, to catch lookups that grow with the square of
//! the chart. Ignored by default (it takes a while in a debug build); run
//! `cargo test --release -p khata-import-tally --test large -- --ignored --nocapture`.

use std::fmt::Write as _;
use std::time::Instant;

use chrono::{DateTime, Utc};
use khata_core::Book;
use khata_import_tally::import;

fn envelope(body: &str) -> String {
    format!(
        "<ENVELOPE><HEADER><TALLYREQUEST>Import Data</TALLYREQUEST></HEADER><BODY><IMPORTDATA><REQUESTDATA>\
         <TALLYMESSAGE>{body}</TALLYMESSAGE></REQUESTDATA></IMPORTDATA></BODY></ENVELOPE>"
    )
}

#[test]
#[ignore]
fn ten_thousand_ledgers_and_a_hundred_thousand_vouchers() {
    const LEDGERS: usize = 10_000;
    const VOUCHERS: usize = 100_000;
    let mut body = String::from("<LEDGER NAME=\"Cash\"><PARENT>Cash-in-Hand</PARENT></LEDGER>");
    for i in 0..LEDGERS {
        let _ = write!(body, "<LEDGER NAME=\"Party {i}\"><PARENT>Sundry Debtors</PARENT></LEDGER>");
    }
    for n in 0..VOUCHERS {
        let party = n % LEDGERS;
        let day = 1 + n % 28;
        let _ = write!(
            body,
            "<VOUCHER VCHTYPE=\"Receipt\"><DATE>202404{day:02}</DATE><VOUCHERNUMBER>{n}</VOUCHERNUMBER>\
             <ALLLEDGERENTRIES.LIST><LEDGERNAME>Cash</LEDGERNAME><AMOUNT>-10.00</AMOUNT></ALLLEDGERENTRIES.LIST>\
             <ALLLEDGERENTRIES.LIST><LEDGERNAME>Party {party}</LEDGERNAME><AMOUNT>10.00</AMOUNT>\
             <BILLALLOCATIONS.LIST><NAME>B{n}</NAME><BILLTYPE>On Account</BILLTYPE><AMOUNT>10.00</AMOUNT></BILLALLOCATIONS.LIST>\
             </ALLLEDGERENTRIES.LIST></VOUCHER>"
        );
    }
    let xml = envelope(&body);
    let mut book = Book::new("Large");
    let at: DateTime<Utc> = DateTime::from_timestamp(1_760_000_000, 0).unwrap();
    let start = Instant::now();
    let report = import(xml.as_bytes(), &mut book, at, "u").unwrap();
    let took = start.elapsed();
    println!("{LEDGERS} ledgers, {VOUCHERS} vouchers ({} MB): {took:?}", xml.len() / 1_000_000);
    assert_eq!(report.vouchers_posted, VOUCHERS);
    assert_eq!(report.ledgers_created, LEDGERS + 1);
}

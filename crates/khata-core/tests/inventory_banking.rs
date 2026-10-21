//! Inventory valuation, its effect on the final accounts, bank
//! reconciliation, group alteration and ratio analysis.

use chrono::{DateTime, NaiveDate, Utc};
use khata_core::analysis::{dashboard, ratio_analysis};
use khata_core::banking::bank_reconciliation;
use khata_core::inventory::{item_vouchers, stock_summary};
use khata_core::reports::{balance_sheet, profit_and_loss};
use khata_core::{
    AuditLog, Book, BookError, Entry, LedgerId, StockGroup, StockGroupId, StockItem, StockItemId, StockLine, Unit,
    UnitId, Voucher, VoucherId, VoucherKind,
};
use rust_decimal_macros::dec;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

fn d(m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, day).unwrap()
}

struct Shop {
    book: Book,
    bank: LedgerId,
    sales: LedgerId,
    purchase: LedgerId,
    customer: LedgerId,
    supplier: LedgerId,
    capital: LedgerId,
    widget: StockItemId,
}

fn ledger(book: &mut Book, name: &str, group: &str) -> LedgerId {
    let g = book.group_by_name(group).unwrap().id;
    book.create_ledger(now(), "owner", name, g).unwrap()
}

fn voucher(kind: VoucherKind, number: &str, date: NaiveDate, entries: Vec<Entry>) -> Voucher {
    Voucher { id: VoucherId::new(), kind, number: number.into(), date, entries, narration: String::new(), cancelled: false, series: None, stock: Vec::new() }
}

/// 10 widgets in stock at 100 each; capital 50,000 in the bank.
fn shop() -> Shop {
    let mut book = Book::new("Widget Shop");
    let bank = ledger(&mut book, "HDFC Bank", "Bank Accounts");
    let sales = ledger(&mut book, "Sales", "Sales Accounts");
    let purchase = ledger(&mut book, "Purchases", "Purchase Accounts");
    let customer = ledger(&mut book, "Asha Stores", "Sundry Debtors");
    let supplier = ledger(&mut book, "Bharat Supply", "Sundry Creditors");
    let capital = ledger(&mut book, "Owner Capital", "Capital Account");
    book.set_opening_balance(now(), "owner", bank, dec!(50000)).unwrap();
    book.set_opening_balance(now(), "owner", capital, dec!(-51000)).unwrap();
    let nos = book.save_unit(now(), "owner", Unit { id: UnitId::new(), symbol: "Nos".into(), name: "Numbers".into(), decimals: 0 }).unwrap();
    let group = book.save_stock_group(now(), "owner", StockGroup { id: StockGroupId::new(), name: "Hardware".into(), parent: None }).unwrap();
    let widget = book
        .save_stock_item(now(), "owner", StockItem {
            id: StockItemId::new(),
            name: "Widget".into(),
            group: Some(group),
            unit: nos,
            opening_qty: dec!(10),
            opening_value: dec!(1000),
            tax_class: None,
            costing: Default::default(),
        })
        .unwrap();
    Shop { book, bank, sales, purchase, customer, supplier, capital, widget }
}

/// Buy 30 at 120, sell 25 at 200.
fn trade(s: &mut Shop) {
    let buy = StockLine::new(s.widget, dec!(30), dec!(120));
    s.book
        .post_voucher(now(), "owner", voucher(VoucherKind::Purchase, "P1", d(4, 5), vec![
            Entry::dr(s.purchase, dec!(3600)).item(buy),
            Entry::cr(s.supplier, dec!(3600)),
        ]))
        .unwrap();
    let sell = StockLine::new(s.widget, dec!(25), dec!(200));
    s.book
        .post_voucher(now(), "owner", voucher(VoucherKind::Sales, "1", d(4, 20), vec![
            Entry::dr(s.customer, dec!(5000)),
            Entry::cr(s.sales, dec!(5000)).item(sell),
        ]))
        .unwrap();
}

#[test]
fn stock_is_valued_at_weighted_average_and_flows_into_the_final_accounts() {
    let mut s = shop();
    // Opening stock is part of the opening trial balance: 50,000 + 1,000 = 51,000.
    assert_eq!(s.book.opening_difference(), dec!(0));
    trade(&mut s);

    let summary = stock_summary(&s.book, d(4, 1), d(4, 30));
    let row = &summary.rows[0];
    assert_eq!((row.name.as_str(), row.group.as_str(), row.unit.as_str()), ("Widget", "Hardware", "Nos"));
    // (1,000 + 3,600) / 40 = 115 per widget; 15 left = 1,725.
    assert_eq!(row.movement.average_cost(), dec!(115));
    assert_eq!((row.movement.closing_qty, row.movement.closing_value), (dec!(15), dec!(1725)));
    assert_eq!(row.movement.outward_value, dec!(5000));

    let pl = profit_and_loss(&s.book, d(4, 1), d(4, 30));
    assert_eq!((pl.opening_stock, pl.closing_stock), (dec!(1000), dec!(1725)));
    // 5,000 sales − 3,600 purchases + 1,725 closing − 1,000 opening.
    assert_eq!(pl.gross_profit, dec!(2125));
    assert_eq!(pl.net_profit, dec!(2125));

    let bs = balance_sheet(&s.book, d(4, 30));
    assert_eq!(bs.closing_stock, dec!(1725));
    assert_eq!(bs.profit_and_loss, dec!(2125));
    assert_eq!(bs.total_assets, bs.total_liabilities, "{bs:#?}");

    let (_, rows) = item_vouchers(&s.book, s.widget, d(4, 1), d(4, 30)).unwrap();
    let closing: Vec<_> = rows.iter().map(|r| (r.particulars.as_str(), r.closing_qty)).collect();
    assert_eq!(closing, [("Bharat Supply", dec!(40)), ("Asha Stores", dec!(15))]);

    // Movements before the period roll into its opening at average cost.
    let may = stock_summary(&s.book, d(5, 1), d(5, 31));
    assert_eq!((may.rows[0].movement.opening_qty, may.opening_value), (dec!(15), dec!(1725)));
    let may_pl = profit_and_loss(&s.book, d(5, 1), d(5, 31));
    assert_eq!((may_pl.opening_stock, may_pl.closing_stock, may_pl.gross_profit), (dec!(1725), dec!(1725), dec!(0)));
}

#[test]
fn stock_rules_hold_on_replay() {
    let mut s = shop();
    trade(&mut s);
    let log: AuditLog = serde_json::from_value(serde_json::to_value(s.book.audit()).unwrap()).unwrap();
    let again = Book::replay("Widget Shop", log).unwrap();
    assert_eq!(stock_summary(&again, d(4, 1), d(4, 30)), stock_summary(&s.book, d(4, 1), d(4, 30)));

    // Items on a Payment, fractional numbers, lines that don't add up, and
    // deleting an item in use are all refused.
    let line = StockLine::new(s.widget, dec!(1), dec!(10));
    let pay = voucher(VoucherKind::Payment, "1", d(4, 6), vec![Entry::dr(s.purchase, dec!(10)).item(line), Entry::cr(s.bank, dec!(10))]);
    assert_eq!(s.book.post_voucher(now(), "owner", pay), Err(BookError::ItemsNotAllowed));
    let half = StockLine::new(s.widget, dec!(0.5), dec!(10));
    let v = voucher(VoucherKind::Purchase, "P2", d(4, 6), vec![Entry::dr(s.purchase, dec!(5)).item(half), Entry::cr(s.supplier, dec!(5))]);
    assert!(matches!(s.book.post_voucher(now(), "owner", v), Err(BookError::QuantityPrecision(_))));
    let wrong = StockLine::new(s.widget, dec!(2), dec!(10));
    let v = voucher(VoucherKind::Purchase, "P3", d(4, 6), vec![Entry::dr(s.purchase, dec!(25)).item(wrong), Entry::cr(s.supplier, dec!(25))]);
    assert!(s.book.post_voucher(now(), "owner", v).is_err());
    assert_eq!(s.book.delete_stock_item(now(), "owner", s.widget), Err(BookError::StockInUse));
    let dup = StockGroup { id: StockGroupId::new(), name: "widget".into(), parent: None };
    assert!(matches!(s.book.save_stock_group(now(), "owner", dup), Err(BookError::DuplicateStockName(_))));
}

#[test]
fn bank_reconciliation_backs_out_uncleared_lines() {
    let mut s = shop();
    let rent = ledger(&mut s.book, "Rent", "Indirect Expenses");
    let cheque = voucher(VoucherKind::Payment, "1", d(4, 28), vec![Entry::dr(rent, dec!(8000)), Entry::cr(s.bank, dec!(8000))]);
    let cheque_id = s.book.post_voucher(now(), "owner", cheque).unwrap();
    let deposit = voucher(VoucherKind::Receipt, "1", d(4, 29), vec![Entry::dr(s.bank, dec!(3000)), Entry::cr(s.customer, dec!(3000))]);
    let deposit_id = s.book.post_voucher(now(), "owner", deposit).unwrap();

    let brs = bank_reconciliation(&s.book, s.bank, d(4, 1), d(4, 30)).unwrap();
    assert_eq!(brs.balance_per_books, dec!(45000));
    assert_eq!((brs.uncleared_withdrawals, brs.uncleared_deposits), (dec!(8000), dec!(3000)));
    assert_eq!(brs.balance_per_bank, dec!(50000));

    // The deposit clears on 30 April, the cheque only on 3 May.
    s.book.set_bank_date(now(), "owner", deposit_id, s.bank, Some(d(4, 30))).unwrap();
    s.book.set_bank_date(now(), "owner", cheque_id, s.bank, Some(d(5, 3))).unwrap();
    let brs = bank_reconciliation(&s.book, s.bank, d(4, 1), d(4, 30)).unwrap();
    assert_eq!(brs.balance_per_bank, dec!(53000));
    let may = bank_reconciliation(&s.book, s.bank, d(5, 1), d(5, 31)).unwrap();
    assert_eq!(may.balance_per_bank, may.balance_per_books);
    assert!(may.lines.is_empty(), "lines cleared before the period are not repeated");

    // A bank date before the voucher, or on a non-bank ledger, is refused.
    assert_eq!(s.book.set_bank_date(now(), "owner", cheque_id, s.bank, Some(d(4, 1))), Err(BookError::InvalidBankDate));
    assert_eq!(s.book.set_bank_date(now(), "owner", cheque_id, rent, Some(d(5, 1))), Err(BookError::InvalidBankDate));
}

#[test]
fn groups_can_be_renamed_and_moved_within_their_kind() {
    let mut s = shop();
    let debtors = s.book.group_by_name("Sundry Debtors").unwrap().id;
    let local = s.book.create_group(now(), "owner", "Local Debtors", debtors).unwrap();
    let mut g = s.book.group(local).unwrap().clone();
    g.name = "City Debtors".into();
    g.aliases = vec!["Town".into()];
    s.book.alter_group(now(), "owner", g.clone()).unwrap();
    assert_eq!(s.book.group_by_name("town").unwrap().id, local);

    // Moving under Deposits (same kind: asset, not cash/bank) is allowed…
    g = s.book.group(local).unwrap().clone();
    g.parent = Some(s.book.group_by_name("Deposits (Asset)").unwrap().id);
    s.book.alter_group(now(), "owner", g).unwrap();
    // …under a liability group, under itself, or a clashing name is not.
    let mut bad = s.book.group(local).unwrap().clone();
    bad.parent = Some(s.book.group_by_name("Sundry Creditors").unwrap().id);
    assert_eq!(s.book.alter_group(now(), "owner", bad), Err(BookError::InvalidGroupParent));
    let mut cyc = s.book.group(local).unwrap().clone();
    cyc.parent = Some(local);
    assert_eq!(s.book.alter_group(now(), "owner", cyc), Err(BookError::InvalidGroupParent));
    let mut clash = s.book.group(local).unwrap().clone();
    clash.name = "Sundry Creditors".into();
    assert!(matches!(s.book.alter_group(now(), "owner", clash), Err(BookError::DuplicateGroup(_))));
    // Primary groups can be renamed but not moved.
    let mut primary = s.book.group_by_name("Capital Account").unwrap().clone();
    primary.parent = Some(debtors);
    assert_eq!(s.book.alter_group(now(), "owner", primary), Err(BookError::InvalidGroupParent));
    let mut renamed = s.book.group_by_name("Capital Account").unwrap().clone();
    renamed.name = "Proprietor's Capital".into();
    s.book.alter_group(now(), "owner", renamed).unwrap();
    let _ = s.capital;
}

#[test]
fn ratios_and_dashboard_follow_the_books() {
    let mut s = shop();
    trade(&mut s);
    let r = ratio_analysis(&s.book, d(4, 1), d(4, 30));
    // Current assets: bank 50,000 + debtors 5,000 + stock 1,725; liabilities: creditors 3,600.
    assert_eq!(r.working_capital, dec!(53125));
    assert_eq!((r.sales, r.purchases, r.stock_in_hand), (dec!(5000), dec!(3600), dec!(1725)));
    assert_eq!(r.current_ratio, Some(dec!(15.76)));
    assert_eq!(r.quick_ratio, Some(dec!(15.28)));
    assert_eq!(r.gross_profit_pct, Some(dec!(42.50)));
    assert_eq!(r.receivable_days, Some(dec!(30)));
    assert_eq!(r.debt_equity, Some(dec!(0)));

    let dash = dashboard(&s.book, d(4, 1), d(5, 31));
    assert_eq!(dash.months.len(), 2);
    assert_eq!((dash.months[0].sales, dash.months[0].purchases), (dec!(5000), dec!(3600)));
    assert_eq!(dash.months[1].cash_bank_balance, dec!(50000));
    assert_eq!(dash.top_debtors[0].name, "Asha Stores");
    assert_eq!(dash.top_creditors[0].amount, dec!(3600));
    assert_eq!((dash.receivables, dash.payables), (dec!(5000), dec!(3600)));
}

#[test]
fn godowns_batches_and_last_rates() {
    use khata_core::inventory::{last_rate, stock_by_location};
    use khata_core::{Godown, GodownId};
    let mut s = shop();
    let shop_floor = s.book.save_godown(now(), "owner", Godown { id: GodownId::new(), name: "Shop Floor".into(), parent: None }).unwrap();
    // Buy 30 into the shop floor in two batches, sell 5 from batch B1.
    s.book
        .post_voucher(now(), "owner", voucher(VoucherKind::Purchase, "P1", d(4, 5), vec![
            Entry::dr(s.purchase, dec!(3600))
                .item(StockLine::new(s.widget, dec!(20), dec!(120)).at(shop_floor).batch("B1"))
                .item(StockLine::new(s.widget, dec!(10), dec!(120)).at(shop_floor).batch("B2")),
            Entry::cr(s.supplier, dec!(3600)),
        ]))
        .unwrap();
    s.book
        .post_voucher(now(), "owner", voucher(VoucherKind::Sales, "1", d(4, 8), vec![
            Entry::dr(s.customer, dec!(1000)),
            Entry::cr(s.sales, dec!(1000)).item(StockLine::new(s.widget, dec!(5), dec!(200)).at(shop_floor).batch("B1")),
        ]))
        .unwrap();
    let rows = stock_by_location(&s.book, d(4, 30));
    let got: Vec<(&str, Option<&str>, _)> = rows.iter().map(|r| (r.godown_name.as_str(), r.batch.as_deref(), r.qty)).collect();
    assert_eq!(got, [("Main Location", None, dec!(10)), ("Shop Floor", Some("B1"), dec!(15)), ("Shop Floor", Some("B2"), dec!(10))]);
    // Values at average cost (1000 + 3600) / 40 = 115.
    assert_eq!(rows[1].value, dec!(1725));

    assert_eq!(last_rate(&s.book, s.widget, false, Some(s.customer)), Some(dec!(200)));
    assert_eq!(last_rate(&s.book, s.widget, true, None), Some(dec!(120)));
    assert_eq!(s.book.delete_godown(now(), "owner", shop_floor), Err(BookError::StockInUse));
    let bad = voucher(VoucherKind::Purchase, "P2", d(4, 9), vec![
        Entry::dr(s.purchase, dec!(120)).item(StockLine::new(s.widget, dec!(1), dec!(120)).batch(" ")),
        Entry::cr(s.supplier, dec!(120)),
    ]);
    assert_eq!(s.book.post_voucher(now(), "owner", bad), Err(BookError::InvalidBatch));
}

#[test]
fn tax_settings_are_validated() {
    use khata_core::{LedgerTax, TaxSide};
    let mut s = shop();
    s.book.set_country(now(), "owner", "IN").unwrap();
    s.book.set_company_tax(now(), "owner", Some("27AAPFU0939F1ZV".into()), Some("27".into())).unwrap();
    s.book.set_tax_rate(now(), "owner", "8471 30", d(4, 1), Some(dec!(18))).unwrap();
    assert_eq!(s.book.profile().unwrap().tax_rates["847130"][&d(4, 1)], dec!(18));
    assert!(matches!(s.book.set_tax_rate(now(), "owner", "8471", d(4, 1), Some(dec!(101))), Err(BookError::InvalidTax(_))));

    // Only duty ledgers collect a tax.
    let duties = s.book.group_by_name("Duties & Taxes").unwrap().id;
    let cgst = s.book.create_ledger(now(), "owner", "Output CGST", duties).unwrap();
    let mut l = s.book.ledger(cgst).unwrap().clone();
    l.tax = Some(LedgerTax { component: "CGST".into(), side: TaxSide::Output });
    s.book.alter_ledger(now(), "owner", l).unwrap();
    let mut wrong = s.book.ledger(s.sales).unwrap().clone();
    wrong.tax = Some(LedgerTax { component: "CGST".into(), side: TaxSide::Output });
    assert!(matches!(s.book.alter_ledger(now(), "owner", wrong), Err(BookError::InvalidTax(_))));
}

#[test]
fn statement_lines_match_uncleared_vouchers() {
    use khata_core::banking::{StatementLine, match_statement};
    let mut s = shop();
    let rent = ledger(&mut s.book, "Rent", "Indirect Expenses");
    let mut pay = |n: &str, date, amt| {
        let v = voucher(VoucherKind::Payment, n, date, vec![Entry::dr(rent, amt), Entry::cr(s.bank, amt)]);
        s.book.post_voucher(now(), "owner", v).unwrap()
    };
    let a = pay("1", d(4, 2), dec!(5000));
    let b = pay("2", d(4, 3), dec!(5000));
    let c = pay("3", d(4, 4), dec!(700));
    let line = |date, reference: &str, out| StatementLine { date, description: "CHQ".into(), reference: reference.into(), deposit: dec!(0), withdrawal: out };
    let m = match_statement(&s.book, s.bank, &[
        line(d(4, 6), "", dec!(5000)),
        line(d(4, 6), "000002", dec!(5000)),
        line(d(4, 6), "", dec!(999)),
        line(d(5, 30), "", dec!(700)),
    ]);
    // The reference picks voucher 2 even though voucher 1 is older; the
    // other 5,000 then goes to voucher 1; 999 and a far-off 700 don't match.
    assert_eq!(m[1].voucher, Some(b));
    assert!(m[1].reference_matched);
    assert_eq!(m[0].voucher, Some(a));
    assert_eq!(m[2].voucher, None);
    assert_eq!(m[3].voucher, None, "outside the window");
    let _ = c;
}

#[test]
fn stock_journal_transfers_and_manufactures() {
    use khata_core::inventory::stock_by_location;
    use khata_core::{Godown, GodownId, StockMove, VoucherError};
    let mut s = shop();
    trade(&mut s); // 15 widgets at the main location, average cost 115.
    let store_room = s.book.save_godown(now(), "owner", Godown { id: GodownId::new(), name: "Store Room".into(), parent: None }).unwrap();
    let nos = s.book.stock_item(s.widget).unwrap().unit;
    let kit = s.book
        .save_stock_item(now(), "owner", StockItem { id: StockItemId::new(), name: "Widget Kit".into(), group: None, unit: nos, opening_qty: dec!(0), opening_value: dec!(0), tax_class: None, costing: Default::default() })
        .unwrap();
    let journal = |moves: Vec<StockMove>| Voucher {
        stock: moves,
        ..voucher(VoucherKind::StockJournal, "", d(4, 25), Vec::new())
    };
    let out = |item, q, r| StockMove { inward: false, line: StockLine::new(item, q, r) };
    let inn = |item, q, r| StockMove { inward: true, line: StockLine::new(item, q, r) };

    // Move 5 widgets to the store room: quantity and value unchanged.
    let mut transfer = journal(vec![out(s.widget, dec!(5), dec!(115)), inn(s.widget, dec!(5), dec!(115))]);
    transfer.stock[1].line.godown = Some(store_room);
    transfer.number = "1".into();
    s.book.post_voucher(now(), "owner", transfer).unwrap();
    let rows = stock_by_location(&s.book, d(4, 30));
    assert_eq!(rows.iter().map(|r| (r.godown_name.as_str(), r.qty)).collect::<Vec<_>>(), [("Main Location", dec!(10)), ("Store Room", dec!(5))]);
    let before = profit_and_loss(&s.book, d(4, 1), d(4, 30)).closing_stock;

    // Make 2 kits from 4 widgets (cost 460) valued at 600: closing stock rises.
    let mut make = journal(vec![out(s.widget, dec!(4), dec!(115)), inn(kit, dec!(2), dec!(300))]);
    make.number = "2".into();
    s.book.post_voucher(now(), "owner", make).unwrap();
    let summary = stock_summary(&s.book, d(4, 1), d(4, 30));
    let kits = summary.rows.iter().find(|r| r.name == "Widget Kit").unwrap();
    assert_eq!((kits.movement.closing_qty, kits.movement.closing_value), (dec!(2), dec!(600)));
    let after = profit_and_loss(&s.book, d(4, 1), d(4, 30)).closing_stock;
    assert!(after > before);
    let bs = balance_sheet(&s.book, d(4, 30));
    assert_eq!(bs.total_assets, bs.total_liabilities);

    // Stock lines only on a Stock Journal, and a Stock Journal needs them.
    let empty = Voucher { number: "3".into(), ..journal(Vec::new()) };
    assert_eq!(s.book.post_voucher(now(), "owner", empty), Err(BookError::Voucher(VoucherError::StockLines)));
    let mut sale = voucher(VoucherKind::Sales, "9", d(4, 26), vec![Entry::dr(s.customer, dec!(1)), Entry::cr(s.sales, dec!(1))]);
    sale.stock = vec![out(s.widget, dec!(1), dec!(1))];
    assert_eq!(s.book.post_voucher(now(), "owner", sale), Err(BookError::Voucher(VoucherError::StockLines)));
    assert_eq!(s.book.delete_godown(now(), "owner", store_room), Err(BookError::StockInUse));
}

#[test]
fn fifo_and_last_purchase_valuation() {
    use khata_core::inventory::Costing;
    // Opening 10 @ 100; buy 30 @ 120 on 5 Apr and 10 @ 150 on 10 Apr; sell 25.
    let value_with = |costing: Costing| {
        let mut s = shop();
        let mut w = s.book.stock_item(s.widget).unwrap().clone();
        w.costing = costing;
        s.book.save_stock_item(now(), "owner", w).unwrap();
        trade(&mut s);
        let buy = StockLine::new(s.widget, dec!(10), dec!(150));
        s.book
            .post_voucher(now(), "owner", voucher(VoucherKind::Purchase, "P2", d(4, 10), vec![
                Entry::dr(s.purchase, dec!(1500)).item(buy),
                Entry::cr(s.supplier, dec!(1500)),
            ]))
            .unwrap();
        let m = &stock_summary(&s.book, d(4, 1), d(4, 30)).rows[0].movement;
        assert_eq!(m.closing_qty, dec!(25));
        let may = &stock_summary(&s.book, d(5, 1), d(5, 31)).rows[0].movement;
        assert_eq!(may.opening_value, m.closing_value, "the next period opens at the same value");
        let bs = balance_sheet(&s.book, d(4, 30));
        assert_eq!(bs.total_assets, bs.total_liabilities);
        m.closing_value
    };
    // Average: (1000 + 3600 + 1500) / 50 = 122 → 25 × 122.
    assert_eq!(value_with(Costing::AverageCost), dec!(3050));
    // FIFO: the 25 left are the newest: 10 @ 150 + 15 @ 120.
    assert_eq!(value_with(Costing::Fifo), dec!(3300));
    // Last purchase: 25 @ 150.
    assert_eq!(value_with(Costing::LastPurchase), dec!(3750));
}

#[test]
fn party_mailing_details_are_kept_and_checked() {
    use khata_core::Mailing;
    let mut s = shop();
    let mut c = s.book.ledger(s.customer).unwrap().clone();
    c.mailing = Some(Mailing { address: "4 Station Rd".into(), region: Some("27".into()), postal_code: "411001".into(), ..Mailing::default() });
    s.book.alter_ledger(now(), "owner", c.clone()).unwrap();
    let log: AuditLog = serde_json::from_value(serde_json::to_value(s.book.audit()).unwrap()).unwrap();
    let again = Book::replay("Widget Shop", log).unwrap();
    assert_eq!(again.ledger(s.customer).unwrap().mailing.as_ref().unwrap().postal_code, "411001");
    c.mailing = Some(Mailing { country: Some("india".into()), ..Mailing::default() });
    assert!(matches!(s.book.alter_ledger(now(), "owner", c), Err(BookError::InvalidTax(_))));
}

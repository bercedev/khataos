//! Stock Summary, Banking (bank reconciliation), Ratio Analysis and the
//! Dashboard: thin wrappers over the core reports.

use khata_core::analysis::{self, Dashboard, RatioAnalysis};
use khata_core::banking::{self, BankReconciliation};
use khata_core::inventory::{self, LocationRow, StockMovement, StockSummary, StockVoucherRow};
use khata_core::{LedgerId, StockItemId, UserRole, VoucherId};
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, now, open_mut, open_ref, os_user, parse_date, parse_uuid};

pub fn stock_summary(slot: &Slot, from: &str, to: &str) -> ApiResult<StockSummary> {
    Ok(inventory::stock_summary(open_ref(slot)?, parse_date(from)?, parse_date(to)?))
}

#[derive(Debug, Clone, Serialize)]
pub struct StockItemVouchersDto {
    pub item_name: String,
    pub unit: String,
    pub summary: StockMovement,
    pub rows: Vec<StockVoucherRow>,
}

/// Stock Summary drill-down: one item's movements.
pub fn stock_item_vouchers(slot: &Slot, item_id: &str, from: &str, to: &str) -> ApiResult<StockItemVouchersDto> {
    let book = open_ref(slot)?;
    let id = StockItemId(parse_uuid(item_id, "stock item")?);
    let (summary, rows) =
        inventory::item_vouchers(book, id, parse_date(from)?, parse_date(to)?).ok_or("unknown stock item")?;
    let item = book.stock_item(id).expect("checked above");
    Ok(StockItemVouchersDto {
        item_name: item.name.clone(),
        unit: book.unit(item.unit).map(|u| u.symbol.clone()).unwrap_or_default(),
        summary,
        rows,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct BankReconciliationDto {
    pub ledger_name: String,
    #[serde(flatten)]
    pub statement: BankReconciliation,
}

pub fn bank_reconciliation(slot: &Slot, ledger_id: &str, from: &str, to: &str) -> ApiResult<BankReconciliationDto> {
    let book = open_ref(slot)?;
    let id = LedgerId(parse_uuid(ledger_id, "ledger")?);
    if !book.is_cash_or_bank(id) {
        return Err("bank reconciliation is for bank and cash ledgers".into());
    }
    let statement =
        banking::bank_reconciliation(book, id, parse_date(from)?, parse_date(to)?).ok_or("unknown ledger")?;
    Ok(BankReconciliationDto { ledger_name: book.ledger(id).map(|l| l.name.clone()).unwrap_or_default(), statement })
}

/// Record the date the bank cleared a voucher's line (empty `date` clears it).
pub fn set_bank_date(slot: &mut Slot, voucher_id: &str, ledger_id: &str, date: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let date = if date.trim().is_empty() { None } else { Some(parse_date(date)?) };
    book.set_bank_date(
        now(),
        &os_user(),
        VoucherId(parse_uuid(voucher_id, "voucher")?),
        LedgerId(parse_uuid(ledger_id, "ledger")?),
        date,
    )
    .map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

/// Godown and batch summary: stock on hand by location and batch.
pub fn stock_by_location(slot: &Slot, as_at: &str) -> ApiResult<Vec<LocationRow>> {
    Ok(inventory::stock_by_location(open_ref(slot)?, parse_date(as_at)?))
}

/// The rate a new invoice line for this item starts with: the last sale
/// (or purchase) rate, preferring the same party. Empty when never traded.
pub fn last_rate(slot: &Slot, item_id: &str, kind: khata_core::VoucherKind, party_id: &str) -> ApiResult<String> {
    use khata_core::VoucherKind::{CreditNote, Purchase};
    let book = open_ref(slot)?;
    let item = StockItemId(parse_uuid(item_id, "stock item")?);
    let party = if party_id.trim().is_empty() { None } else { Some(LedgerId(parse_uuid(party_id, "party")?)) };
    // Purchases and sales returns bring stock in.
    let inward = matches!(kind, Purchase | CreditNote);
    Ok(inventory::last_rate(book, item, inward, party).map(crate::api::amt).unwrap_or_default())
}

/// The narration `Ctrl+R` (of `kind`) or `Alt+R` (for `ledger_id`) repeats.
/// Empty when no earlier voucher matches.
pub fn last_narration(slot: &Slot, kind: Option<khata_core::VoucherKind>, ledger_id: &str) -> ApiResult<String> {
    let book = open_ref(slot)?;
    let ledger = if ledger_id.trim().is_empty() { None } else { Some(LedgerId(parse_uuid(ledger_id, "ledger")?)) };
    Ok(book.last_narration(kind, ledger).unwrap_or_default().to_string())
}

pub fn ratio_analysis(slot: &Slot, from: &str, to: &str) -> ApiResult<RatioAnalysis> {
    Ok(analysis::ratio_analysis(open_ref(slot)?, parse_date(from)?, parse_date(to)?))
}

pub fn dashboard(slot: &Slot, from: &str, to: &str) -> ApiResult<Dashboard> {
    Ok(analysis::dashboard(open_ref(slot)?, parse_date(from)?, parse_date(to)?))
}

// ---------------------------------------------------------------- bank statements

/// Split CSV text into rows (RFC 4180: quoted fields, doubled quotes, CRLF).
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let (mut row, mut field, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.into_iter().filter(|r| r.iter().any(|f| !f.trim().is_empty())).collect()
}

/// Dates as banks print them: 05/04/2026, 05-04-26, 05-Apr-2026, 2026-04-05.
fn statement_date(s: &str) -> Option<chrono::NaiveDate> {
    let s = s.trim();
    ["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%d/%m/%y", "%d-%m-%y", "%d-%b-%Y", "%d %b %Y", "%d-%b-%y", "%Y-%m-%d"]
        .iter()
        .find_map(|f| chrono::NaiveDate::parse_from_str(s, f).ok())
}

fn statement_amount(s: &str) -> Option<khata_core::Amount> {
    let t: String = s.trim().chars().filter(|c| !matches!(c, ',' | ' ' | '₹')).collect();
    let t = t.trim_end_matches("Cr").trim_end_matches("CR").trim_end_matches("Dr").trim_end_matches("DR");
    if t.is_empty() || t == "-" { Some(khata_core::Amount::ZERO) } else { t.parse().ok() }
}

/// Read a bank's CSV statement. Columns are found by their headings:
/// date (txn/value date), description/narration/particulars,
/// cheque/ref no., withdrawal/debit, deposit/credit — or a single signed
/// amount column. Rows that don't parse (titles, totals) are skipped.
pub fn parse_statement(text: &str) -> ApiResult<Vec<khata_core::banking::StatementLine>> {
    let rows = csv_rows(text);
    let find = |row: &[String], words: &[&str]| {
        row.iter().position(|h| {
            let h = h.to_lowercase();
            words.iter().any(|w| h.contains(w))
        })
    };
    // The heading row is the first one naming a date and an amount column.
    let (head_at, head) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| find(r, &["date"]).is_some() && find(r, &["debit", "withdrawal", "credit", "deposit", "amount"]).is_some())
        .ok_or("no heading row with a date and amount columns was found in this file")?;
    let date = find(head, &["txn date", "transaction date", "date"]).expect("checked");
    let desc = find(head, &["description", "narration", "particulars", "remarks", "details"]);
    let reference = find(head, &["chq", "cheque", "ref"]);
    let out_col = find(head, &["withdrawal", "debit"]);
    let in_col = find(head, &["deposit", "credit"]);
    let amount_col = find(head, &["amount"]).filter(|_| out_col.is_none() && in_col.is_none());
    let cell = |r: &[String], i: Option<usize>| i.and_then(|i| r.get(i)).map(|s| s.trim().to_owned()).unwrap_or_default();
    let mut lines = Vec::new();
    for r in &rows[head_at + 1..] {
        let Some(d) = statement_date(&cell(r, Some(date))) else { continue };
        let (deposit, withdrawal) = match amount_col {
            Some(_) => {
                let a = statement_amount(&cell(r, amount_col)).unwrap_or_default();
                (a.max(khata_core::Amount::ZERO), (-a).max(khata_core::Amount::ZERO))
            }
            None => (
                statement_amount(&cell(r, in_col)).unwrap_or_default().abs(),
                statement_amount(&cell(r, out_col)).unwrap_or_default().abs(),
            ),
        };
        if deposit.is_zero() && withdrawal.is_zero() {
            continue;
        }
        lines.push(khata_core::banking::StatementLine {
            date: d,
            description: cell(r, desc),
            reference: cell(r, reference),
            deposit,
            withdrawal,
        });
    }
    if lines.is_empty() {
        return Err("no transactions were found under the heading row".into());
    }
    Ok(lines)
}

#[derive(Debug, Clone, Serialize)]
pub struct StatementRowDto {
    #[serde(flatten)]
    pub line: khata_core::banking::StatementLine,
    pub voucher_id: Option<String>,
    pub voucher_date: Option<chrono::NaiveDate>,
    pub voucher_number: Option<String>,
    pub voucher_kind: Option<khata_core::VoucherKind>,
    /// The other ledger(s) on the matched voucher.
    pub particulars: Option<String>,
    pub reference_matched: bool,
}

/// Read a statement file's text and pair its lines with uncleared vouchers.
pub fn match_statement(slot: &Slot, ledger_id: &str, csv: &str) -> ApiResult<Vec<StatementRowDto>> {
    let book = open_ref(slot)?;
    let ledger = LedgerId(parse_uuid(ledger_id, "ledger")?);
    if !book.is_cash_or_bank(ledger) {
        return Err("bank statements are matched against a bank ledger".into());
    }
    let lines = parse_statement(csv)?;
    let matches = banking::match_statement(book, ledger, &lines);
    Ok(lines
        .into_iter()
        .zip(matches)
        .map(|(line, m)| {
            let v = m.voucher.and_then(|id| book.voucher(id));
            StatementRowDto {
                voucher_id: v.map(|v| v.id.0.to_string()),
                voucher_date: v.map(|v| v.date),
                voucher_number: v.map(|v| v.number.clone()),
                voucher_kind: v.map(|v| v.kind),
                particulars: v.map(|v| {
                    v.entries
                        .iter()
                        .filter(|e| e.ledger != ledger)
                        .filter_map(|e| book.ledger(e.ledger).map(|l| l.name.clone()))
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
                reference_matched: m.reference_matched,
                line,
            }
        })
        .collect())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BankDateInput {
    pub voucher_id: String,
    pub date: String,
}

/// Record the bank dates of confirmed matches, all or nothing.
pub fn apply_bank_dates(slot: &mut Slot, ledger_id: &str, dates: &[BankDateInput]) -> ApiResult<usize> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let ledger = LedgerId(parse_uuid(ledger_id, "ledger")?);
    let mut next = book.clone();
    for d in dates {
        next.set_bank_date(now(), &os_user(), VoucherId(parse_uuid(&d.voucher_id, "voucher")?), ledger, Some(parse_date(&d.date)?))
            .map_err(err)?;
    }
    *book = next;
    store.save(book).map_err(err)?;
    Ok(dates.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_from_common_bank_layouts() {
        let hdfc = "Statement of account\n\
            Date,Narration,Chq./Ref.No.,Value Dt,Withdrawal Amt.,Deposit Amt.,Closing Balance\n\
            02/04/26,\"NEFT-ASHA STORES, PUNE\",0000412,02/04/26,,\"12,500.00\",\"62,500.00\"\n\
            03/04/26,CHQ PAID-RENT,000002,03/04/26,5000.00,,57500.00\n\
            ,Total,,,5000.00,12500.00,\n";
        let lines = parse_statement(hdfc).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].description, "NEFT-ASHA STORES, PUNE");
        assert_eq!(lines[0].deposit, khata_core::Amount::from(12500));
        assert_eq!((lines[1].reference.as_str(), lines[1].withdrawal), ("000002", khata_core::Amount::from(5000)));

        let signed = "Transaction Date;x\nTransaction Date,Description,Amount\n2026-04-05,Card,-120.50\n2026-04-06,Refund,20\n";
        let lines = parse_statement(signed).unwrap();
        assert_eq!(lines[0].withdrawal.to_string(), "120.50");
        assert_eq!(lines[1].deposit, khata_core::Amount::from(20));
        assert!(parse_statement("hello,world\n1,2\n").is_err());
    }
}

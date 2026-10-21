//! Read-only views for external accountants and auditors who do not run the
//! app: reports, drill-down (group summary, ledger statement), receivables
//! and payables, and downloads.
//!
//! Every handler accepts any role and reads the company's cached, verified
//! books. Reports that can be printed or exported take `?format=`:
//! - `json` (default);
//! - `pdf-html`: print-ready HTML from `khata-print` (the browser's "Save as
//!   PDF" turns it into a PDF);
//! - `csv`: RFC 4180 CSV with plain decimal amounts.
//!
//! Downloads are sent as attachments with a descriptive file name. The HTML
//! carries its own CSP that allows only its stylesheet, never scripts.

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, NaiveDate, Utc};
use khata_core::format::Grouping;
use khata_core::reports::{self, OutstandingBill};
use khata_core::{Amount, Book, GroupId, LedgerId, Nature, TrialBalanceRow};
use khata_print::Header;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::csv::{self, Csv};
use crate::{ApiError, SharedState, auth};

/// Longest window accepted for the MSME overdue rule, in days.
const MAX_OVERDUE_DAYS: u32 = 3_650;
/// Default MSME window: s.43B(h) allows at most 45 days by agreement.
const DEFAULT_MSME_DAYS: u32 = 45;
/// Deepest group nesting walked when summarising (the chart has no cycles;
/// this only bounds the walk).
const MAX_GROUP_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Format {
    #[default]
    Json,
    PdfHtml,
    Csv,
}

#[derive(Deserialize)]
pub(crate) struct Fmt {
    #[serde(default)]
    format: Format,
}

#[derive(Deserialize)]
pub(crate) struct Period {
    from: NaiveDate,
    to: NaiveDate,
}

#[derive(Deserialize)]
pub(crate) struct AsAt {
    as_at: NaiveDate,
}

#[derive(Deserialize)]
pub(crate) struct MaybeAsAt {
    as_at: Option<NaiveDate>,
}

/// Run `f` on the company's books after checking the token (any role).
fn read<T>(
    state: &SharedState,
    headers: &HeaderMap,
    id: Uuid,
    f: impl FnOnce(&Book) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    auth::authorize(state, headers, id)?;
    state.with_company(id, |c| f(&c.book))
}

fn print_header(book: &Book) -> Header<'_> {
    Header { company: &book.name, address: "", grouping: Grouping::Indian }
}

/// File-name-safe slug: ASCII letters and digits, other runs become `-`.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    let out: String = out.chars().take(60).collect();
    if out.is_empty() { "company".into() } else { out }
}

/// A download: `<company>_<report>.<ext>` as an attachment, never cached.
fn download(book: &Book, report: &str, format: Format, body: String) -> Result<Response, ApiError> {
    let (ext, content_type) = match format {
        Format::Csv => ("csv", "text/csv; charset=utf-8"),
        Format::PdfHtml => ("html", "text/html; charset=utf-8"),
        Format::Json => return Err(ApiError::internal("download called for json")),
    };
    let disposition = format!("attachment; filename=\"{}_{}.{ext}\"", slug(&book.name), slug(report));
    let mut resp = body.into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).map_err(ApiError::internal)?);
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if format == Format::PdfHtml {
        let csp = format!("default-src 'none'; style-src '{}'; base-uri 'none'; form-action 'none'", crate::portal::print_style_source());
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_str(&csp).map_err(ApiError::internal)?);
    }
    Ok(resp)
}

fn no_print(report: &str) -> ApiError {
    ApiError::BadRequest(format!("{report} has no print layout; use format=csv"))
}

/// Plain decimal string, as `khata_print::trial_balance_csv` writes them.
fn amount(a: Amount) -> String {
    a.to_string()
}

fn opt_date(d: Option<NaiveDate>) -> String {
    d.map(|d| d.to_string()).unwrap_or_default()
}

// ---------------------------------------------------------------- Trial balance

/// Ledger balances plus the stock items' opening value, which is a debit
/// of its own in the Trial Balance.
#[derive(Serialize)]
pub(crate) struct TrialBalanceView {
    rows: Vec<TrialBalanceLine>,
    opening_stock: Amount,
}

#[derive(Serialize)]
pub(crate) struct TrialBalanceLine {
    ledger: LedgerId,
    name: String,
    debit: Amount,
    credit: Amount,
}

/// Trial balance as at a date (all vouchers when `None`), by ledger name.
fn trial_balance_rows(book: &Book, as_at: Option<NaiveDate>) -> Vec<TrialBalanceRow> {
    let Some(as_at) = as_at else { return book.trial_balance() };
    let mut rows: Vec<TrialBalanceRow> = book
        .balances(Some(as_at))
        .into_iter()
        .filter(|(_, bal)| !bal.is_zero())
        .filter_map(|(ledger, bal)| {
            Some(TrialBalanceRow {
                ledger,
                name: book.ledger(ledger)?.name.clone(),
                debit: bal.max(Amount::ZERO),
                credit: (-bal).max(Amount::ZERO),
            })
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

/// Date printed on an undated trial balance: today, or the last voucher
/// date if post-dated vouchers are included.
fn latest_date(book: &Book) -> NaiveDate {
    let today = Utc::now().date_naive();
    book.vouchers().map(|v| v.date).max().map_or(today, |d| d.max(today))
}

pub(crate) async fn trial_balance(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<MaybeAsAt>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let rows = trial_balance_rows(book, q.as_at);
        let as_at = q.as_at.unwrap_or_else(|| latest_date(book));
        let name = format!("trial-balance-{as_at}");
        let opening_stock = khata_core::inventory::opening_stock_value(book);
        match f.format {
            Format::Json => Ok(Json(TrialBalanceView {
                rows: rows
                    .into_iter()
                    .map(|r| TrialBalanceLine { ledger: r.ledger, name: r.name, debit: r.debit, credit: r.credit })
                    .collect(),
                opening_stock,
            })
            .into_response()),
            Format::PdfHtml => download(
                book,
                &name,
                f.format,
                khata_print::trial_balance_html(&print_header(book), as_at, &rows, opening_stock),
            ),
            Format::Csv => download(book, &name, f.format, khata_print::trial_balance_csv(&rows, opening_stock)),
        }
    })
}

// ---------------------------------------------------------------- Day book

pub(crate) async fn day_book(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(p): Query<Period>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let rows = reports::day_book(book, p.from, p.to);
        let name = format!("day-book-{}-to-{}", p.from, p.to);
        match f.format {
            Format::Json => Ok(Json(rows).into_response()),
            Format::PdfHtml => download(book, &name, f.format, khata_print::day_book_html(&print_header(book), p.from, p.to, &rows)),
            Format::Csv => {
                let mut c = Csv::new(&["Date", "Particulars", "Vch Type", "Vch No.", "Debit", "Credit", "Cancelled"]);
                for r in &rows {
                    c.row([
                        r.date.to_string(),
                        csv::text(&r.particulars),
                        format!("{:?}", r.kind),
                        csv::text(&r.number),
                        amount(r.debit),
                        amount(r.credit),
                        if r.cancelled { "Yes".into() } else { String::new() },
                    ]);
                }
                download(book, &name, f.format, c.finish())
            }
        }
    })
}

// ---------------------------------------------------------------- Profit & Loss, Balance Sheet

pub(crate) async fn profit_and_loss(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(p): Query<Period>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let pl = reports::profit_and_loss(book, p.from, p.to);
        let name = format!("profit-and-loss-{}-to-{}", p.from, p.to);
        match f.format {
            Format::Json => Ok(Json(pl).into_response()),
            Format::PdfHtml => download(book, &name, f.format, khata_print::profit_and_loss_html(&print_header(book), &pl)),
            Format::Csv => {
                let mut c = Csv::new(&["Side", "Particulars", "Amount"]);
                if !pl.opening_stock.is_zero() {
                    c.row(["Expenses".into(), "Opening Stock".into(), amount(pl.opening_stock)]);
                }
                for r in &pl.expenses {
                    c.row(["Expenses".into(), csv::text(&r.name), amount(r.amount)]);
                }
                for r in &pl.incomes {
                    c.row(["Incomes".into(), csv::text(&r.name), amount(r.amount)]);
                }
                if !pl.closing_stock.is_zero() {
                    c.row(["Incomes".into(), "Closing Stock".into(), amount(pl.closing_stock)]);
                }
                c.row(["Result".into(), "Gross Profit".into(), amount(pl.gross_profit)]);
                c.row(["Result".into(), "Net Profit".into(), amount(pl.net_profit)]);
                download(book, &name, f.format, c.finish())
            }
        }
    })
}

pub(crate) async fn balance_sheet(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<AsAt>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let bs = reports::balance_sheet(book, q.as_at);
        let name = format!("balance-sheet-{}", q.as_at);
        match f.format {
            Format::Json => Ok(Json(bs).into_response()),
            Format::PdfHtml => download(book, &name, f.format, khata_print::balance_sheet_html(&print_header(book), &bs)),
            Format::Csv => {
                let mut c = Csv::new(&["Side", "Particulars", "Amount"]);
                for r in &bs.liabilities {
                    c.row(["Liabilities".into(), csv::text(&r.name), amount(r.amount)]);
                }
                c.row(["Liabilities".into(), "Profit & Loss A/c".into(), amount(bs.profit_and_loss)]);
                if !bs.opening_difference.is_zero() {
                    c.row(["Liabilities".into(), "Difference in opening balances".into(), amount(bs.opening_difference)]);
                }
                c.row(["Liabilities".into(), "Total".into(), amount(bs.total_liabilities)]);
                for r in &bs.assets {
                    c.row(["Assets".into(), csv::text(&r.name), amount(r.amount)]);
                }
                if !bs.closing_stock.is_zero() {
                    c.row(["Assets".into(), "Closing Stock".into(), amount(bs.closing_stock)]);
                }
                c.row(["Assets".into(), "Total".into(), amount(bs.total_assets)]);
                download(book, &name, f.format, c.finish())
            }
        }
    })
}

// ---------------------------------------------------------------- Ledgers

#[derive(Serialize)]
pub(crate) struct LedgerLine {
    id: LedgerId,
    name: String,
    group: GroupId,
    group_name: String,
    /// Closing balance, debit positive.
    balance: Amount,
}

pub(crate) async fn ledgers(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<MaybeAsAt>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let balances = book.balances(q.as_at);
        let mut rows: Vec<LedgerLine> = book
            .ledgers()
            .map(|l| LedgerLine {
                id: l.id,
                name: l.name.clone(),
                group: l.group,
                group_name: book.group(l.group).map(|g| g.name.clone()).unwrap_or_default(),
                balance: balances.get(&l.id).copied().unwrap_or_default(),
            })
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let name = match q.as_at {
            Some(d) => format!("ledgers-{d}"),
            None => "ledgers".into(),
        };
        match f.format {
            Format::Json => Ok(Json(rows).into_response()),
            Format::PdfHtml => Err(no_print("the ledger list")),
            Format::Csv => {
                let mut c = Csv::new(&["Ledger", "Group", "Closing Balance", "Dr/Cr"]);
                for r in &rows {
                    let side = if r.balance.is_zero() { "" } else if r.balance.is_sign_positive() { "Dr" } else { "Cr" };
                    c.row([csv::text(&r.name), csv::text(&r.group_name), amount(r.balance.abs()), side.into()]);
                }
                download(book, &name, f.format, c.finish())
            }
        }
    })
}

#[derive(Serialize)]
pub(crate) struct Statement {
    name: String,
    group: GroupId,
    #[serde(flatten)]
    statement: reports::LedgerStatement,
}

pub(crate) async fn ledger_statement(
    State(state): State<SharedState>,
    Path((id, ledger)): Path<(Uuid, Uuid)>,
    Query(p): Query<Period>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let l = book.ledger(LedgerId(ledger)).ok_or(ApiError::NotFound)?;
        let s = reports::ledger_statement(book, l.id, p.from, p.to).ok_or(ApiError::NotFound)?;
        let name = format!("ledger-{}-{}-to-{}", l.name, p.from, p.to);
        match f.format {
            Format::Json => Ok(Json(Statement { name: l.name.clone(), group: l.group, statement: s }).into_response()),
            Format::PdfHtml => {
                download(book, &name, f.format, khata_print::ledger_statement_html(&print_header(book), &l.name, p.from, p.to, &s))
            }
            Format::Csv => {
                let mut c = Csv::new(&["Date", "Particulars", "Vch Type", "Vch No.", "Debit", "Credit", "Balance"]);
                c.row([p.from.to_string(), "Opening Balance".into(), String::new(), String::new(), String::new(), String::new(), amount(s.opening)]);
                for r in &s.rows {
                    c.row([
                        r.date.to_string(),
                        csv::text(&r.particulars),
                        format!("{:?}", r.kind),
                        csv::text(&r.number),
                        amount(r.debit),
                        amount(r.credit),
                        amount(r.balance),
                    ]);
                }
                c.row([p.to.to_string(), "Closing Balance".into(), String::new(), String::new(), String::new(), String::new(), amount(s.closing)]);
                download(book, &name, f.format, c.finish())
            }
        }
    })
}

// ---------------------------------------------------------------- Group summary

#[derive(Deserialize)]
pub(crate) struct GroupQuery {
    /// With `to`: movements within the period (drill-down from P&L).
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    /// Closing balances as at a date (drill-down from the Balance Sheet).
    as_at: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum RowKind {
    Group,
    Ledger,
}

#[derive(Serialize)]
pub(crate) struct SummaryRow {
    kind: RowKind,
    id: Uuid,
    name: String,
    /// Shown positive on the group's natural side, like `GroupTotal`.
    amount: Amount,
}

#[derive(Serialize)]
pub(crate) struct GroupSummary {
    group: GroupId,
    name: String,
    nature: Nature,
    parent: Option<GroupId>,
    rows: Vec<SummaryRow>,
    total: Amount,
}

/// Sub-groups (with everything beneath them) and ledgers directly under
/// `group`, from signed ledger balances.
fn group_summary(book: &Book, group: GroupId, balances: &BTreeMap<LedgerId, Amount>) -> Option<GroupSummary> {
    let g = book.group(group)?;
    let natural = |a: Amount| if matches!(g.nature, Nature::Assets | Nature::Expenses) { a } else { -a };
    let mut sub: BTreeMap<GroupId, Amount> = BTreeMap::new();
    let mut rows = Vec::new();
    for (&ledger, &bal) in balances {
        let Some(l) = book.ledger(ledger) else { continue };
        if l.group == group {
            rows.push(SummaryRow { kind: RowKind::Ledger, id: l.id.0, name: l.name.clone(), amount: natural(bal) });
            continue;
        }
        // Find the child of `group` on this ledger's path to the root.
        let mut at = l.group;
        for _ in 0..MAX_GROUP_DEPTH {
            match book.group(at).and_then(|x| x.parent) {
                Some(p) if p == group => {
                    *sub.entry(at).or_default() += bal;
                    break;
                }
                Some(p) => at = p,
                None => break,
            }
        }
    }
    rows.retain(|r| !r.amount.is_zero());
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    let mut groups: Vec<SummaryRow> = sub
        .into_iter()
        .filter(|(_, a)| !a.is_zero())
        .filter_map(|(id, a)| Some(SummaryRow { kind: RowKind::Group, id: id.0, name: book.group(id)?.name.clone(), amount: natural(a) }))
        .collect();
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    groups.extend(rows);
    let total = groups.iter().map(|r| r.amount).sum();
    Some(GroupSummary { group, name: g.name.clone(), nature: g.nature, parent: g.parent, rows: groups, total })
}

/// Net movement per ledger from vouchers within `[from, to]`.
fn movements(book: &Book, from: NaiveDate, to: NaiveDate) -> BTreeMap<LedgerId, Amount> {
    let mut net = BTreeMap::new();
    for v in book.active_vouchers().filter(|v| v.date >= from && v.date <= to) {
        for e in &v.entries {
            *net.entry(e.ledger).or_default() += e.signed();
        }
    }
    net
}

pub(crate) async fn group(
    State(state): State<SharedState>,
    Path((id, group)): Path<(Uuid, Uuid)>,
    Query(q): Query<GroupQuery>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let (balances, period) = match (q.from, q.to, q.as_at) {
            (Some(from), Some(to), None) => (movements(book, from, to), format!("{from}-to-{to}")),
            (None, None, Some(d)) => (book.balances(Some(d)), d.to_string()),
            (None, None, None) => (book.balances(None), "all".into()),
            _ => return Err(ApiError::BadRequest("give from and to, or as_at".into())),
        };
        let s = group_summary(book, GroupId(group), &balances).ok_or(ApiError::NotFound)?;
        match f.format {
            Format::Json => Ok(Json(s).into_response()),
            Format::PdfHtml => Err(no_print("the group summary")),
            Format::Csv => {
                let mut c = Csv::new(&["Particulars", "Type", "Amount"]);
                for r in &s.rows {
                    let kind = if r.kind == RowKind::Group { "Group" } else { "Ledger" };
                    c.row([csv::text(&r.name), kind.into(), amount(r.amount)]);
                }
                c.row(["Total".into(), String::new(), amount(s.total)]);
                download(book, &format!("group-{}-{period}", s.name), f.format, c.finish())
            }
        }
    })
}

// ---------------------------------------------------------------- Bills

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Side {
    Receivable,
    Payable,
}

#[derive(Deserialize)]
pub(crate) struct OutstandingQuery {
    as_at: NaiveDate,
    side: Option<Side>,
}

fn bills_csv(bills: &[OutstandingBill], days_label: &str) -> String {
    let mut c = Csv::new(&["Party", "Reference", "Bill Date", "Due Date", "Side", "Amount", days_label]);
    for b in bills {
        let side = if b.amount.is_sign_negative() { "Payable" } else { "Receivable" };
        c.row([
            csv::text(&b.party),
            csv::text(&b.reference),
            opt_date(b.date),
            opt_date(b.due),
            side.into(),
            amount(b.amount.abs()),
            b.overdue_days.to_string(),
        ]);
    }
    c.finish()
}

pub(crate) async fn outstanding(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<OutstandingQuery>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let mut bills = reports::outstanding_bills(book, q.as_at);
        let label = match q.side {
            Some(Side::Receivable) => {
                bills.retain(|b| b.amount.is_sign_positive());
                "receivables"
            }
            Some(Side::Payable) => {
                bills.retain(|b| b.amount.is_sign_negative());
                "payables"
            }
            None => "outstanding",
        };
        match f.format {
            Format::Json => Ok(Json(bills).into_response()),
            Format::PdfHtml => Err(no_print("outstanding bills")),
            Format::Csv => download(book, &format!("{label}-{}", q.as_at), f.format, bills_csv(&bills, "Overdue Days")),
        }
    })
}

#[derive(Deserialize)]
pub(crate) struct AgeingQuery {
    as_at: NaiveDate,
    #[serde(default = "yes")]
    receivable: bool,
}

fn yes() -> bool {
    true
}

pub(crate) async fn ageing(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<AgeingQuery>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let rows = reports::ageing(book, q.as_at, q.receivable);
        let which = if q.receivable { "receivables" } else { "payables" };
        match f.format {
            Format::Json => Ok(Json(rows).into_response()),
            Format::PdfHtml => Err(no_print("ageing")),
            Format::Csv => {
                let mut c = Csv::new(&["Party", "0-30 days", "31-60 days", "61-90 days", "Over 90 days", "Total"]);
                for r in &rows {
                    let [a, b, cc, d] = r.buckets;
                    c.row([csv::text(&r.party), amount(a), amount(b), amount(cc), amount(d), amount(r.total)]);
                }
                download(book, &format!("ageing-{which}-{}", q.as_at), f.format, c.finish())
            }
        }
    })
}

#[derive(Deserialize)]
pub(crate) struct MsmeQuery {
    as_at: NaiveDate,
    days: Option<u32>,
}

pub(crate) async fn msme_overdue(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(q): Query<MsmeQuery>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let days = q.days.unwrap_or(DEFAULT_MSME_DAYS);
    if days > MAX_OVERDUE_DAYS {
        return Err(ApiError::BadRequest(format!("days is limited to {MAX_OVERDUE_DAYS}")));
    }
    read(&state, &headers, id, |book| {
        let bills = reports::overdue_payables(book, q.as_at, days.into());
        match f.format {
            Format::Json => Ok(Json(bills).into_response()),
            Format::PdfHtml => Err(no_print("MSME overdue payables")),
            Format::Csv => {
                download(book, &format!("msme-overdue-{days}-days-{}", q.as_at), f.format, bills_csv(&bills, "Days Past Due"))
            }
        }
    })
}

// ---------------------------------------------------------------- Audit trail

#[derive(Serialize)]
pub(crate) struct AuditLine {
    seq: u64,
    at: DateTime<Utc>,
    user: String,
    action: khata_core::AuditAction,
    entity: String,
    /// Label of the access token that pushed this entry to the server
    /// (server-side provenance; `None` for entries created on the server).
    pushed_by: Option<String>,
    /// When the server received it.
    received_at: Option<DateTime<Utc>>,
    hash: String,
}

/// The edit log for auditors (India: Companies (Accounts) Rule 3(1), GST
/// Rule 56(8)): every change with its author, time and the token that
/// delivered it. Snapshots stay out of the export; `/pull` has them.
pub(crate) async fn audit_trail(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    auth::authorize(&state, &headers, id)?;
    let pushes = auth::push_log(&state, id)?;
    let provenance = |seq: u64| {
        // Ranges never overlap: each push starts where the trail ended.
        let i = pushes.partition_point(|p| p.from_seq <= seq);
        let p = pushes.get(i.checked_sub(1)?)?;
        (seq < p.to_seq).then(|| (p.label.clone(), DateTime::from_timestamp(p.received_at, 0)))
    };
    state.with_company(id, |c| {
        let book = &c.book;
        let lines: Vec<AuditLine> = book
            .audit()
            .entries()
            .iter()
            .map(|e| {
                let (pushed_by, received_at) = provenance(e.seq).map_or((None, None), |(l, t)| (Some(l), t));
                AuditLine {
                    seq: e.seq,
                    at: e.at,
                    user: e.user.clone(),
                    action: e.action,
                    entity: e.entity.clone(),
                    pushed_by,
                    received_at,
                    hash: e.hash.clone(),
                }
            })
            .collect();
        match f.format {
            Format::Json => Ok(Json(lines).into_response()),
            Format::PdfHtml => Err(no_print("the audit trail")),
            Format::Csv => {
                let mut c = Csv::new(&["Seq", "At (UTC)", "User", "Action", "Entity", "Pushed By", "Received At (UTC)", "Hash"]);
                for l in &lines {
                    c.row([
                        l.seq.to_string(),
                        l.at.to_rfc3339(),
                        csv::text(&l.user),
                        format!("{:?}", l.action),
                        csv::text(&l.entity),
                        l.pushed_by.as_deref().map(csv::text).unwrap_or_default(),
                        l.received_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
                        l.hash.clone(),
                    ]);
                }
                download(book, "audit-trail", f.format, c.finish())
            }
        }
    })
}

// ---------------------------------------------------------------- Stock Summary, Ratio Analysis

pub(crate) async fn stock_summary(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(p): Query<Period>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let s = khata_core::inventory::stock_summary(book, p.from, p.to);
        match f.format {
            Format::Json => Ok(Json(s).into_response()),
            Format::PdfHtml => Err(no_print("stock summary")),
            Format::Csv => {
                let mut c = Csv::new(&[
                    "Stock Group", "Item", "Unit", "Opening Qty", "Opening Value", "Inward Qty", "Inward Value",
                    "Outward Qty", "Outward Value", "Closing Qty", "Closing Value",
                ]);
                for r in &s.rows {
                    let m = &r.movement;
                    c.row([
                        csv::text(&r.group),
                        csv::text(&r.name),
                        csv::text(&r.unit),
                        m.opening_qty.normalize().to_string(),
                        amount(m.opening_value),
                        m.inward_qty.normalize().to_string(),
                        amount(m.inward_value),
                        m.outward_qty.normalize().to_string(),
                        amount(m.outward_value),
                        m.closing_qty.normalize().to_string(),
                        amount(m.closing_value),
                    ]);
                }
                download(book, &format!("stock-summary-{}-to-{}", p.from, p.to), f.format, c.finish())
            }
        }
    })
}

pub(crate) async fn ratios(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Query(p): Query<Period>,
    Query(f): Query<Fmt>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    read(&state, &headers, id, |book| {
        let r = khata_core::analysis::ratio_analysis(book, p.from, p.to);
        match f.format {
            Format::Json => Ok(Json(r).into_response()),
            Format::PdfHtml => Err(no_print("ratio analysis")),
            Format::Csv => {
                let opt = |x: Option<Amount>| x.map(amount).unwrap_or_default();
                let mut c = Csv::new(&["Section", "Particulars", "Value"]);
                for (name, v) in [
                    ("Working Capital", r.working_capital),
                    ("Cash-in-Hand", r.cash_in_hand),
                    ("Bank Accounts", r.bank_accounts),
                    ("Bank OD A/c", r.bank_od),
                    ("Sundry Debtors", r.sundry_debtors),
                    ("Sundry Creditors", r.sundry_creditors),
                    ("Sales Accounts", r.sales),
                    ("Purchase Accounts", r.purchases),
                    ("Stock-in-Hand", r.stock_in_hand),
                    ("Net Profit", r.net_profit),
                ] {
                    c.row(["Principal Groups".into(), name.into(), amount(v)]);
                }
                for (name, v) in [
                    ("Working Capital Turnover", r.working_capital_turnover),
                    ("Inventory Turnover", r.inventory_turnover),
                    ("Current Ratio", r.current_ratio),
                    ("Quick Ratio", r.quick_ratio),
                    ("Debt/Equity Ratio", r.debt_equity),
                    ("Gross Profit %", r.gross_profit_pct),
                    ("Net Profit %", r.net_profit_pct),
                    ("Operating Cost %", r.operating_cost_pct),
                    ("Receivable Turnover (days)", r.receivable_days),
                    ("Return on Investment %", r.return_on_investment_pct),
                    ("Return on Working Capital %", r.return_on_working_capital_pct),
                ] {
                    c.row(["Principal Ratios".into(), name.into(), opt(v)]);
                }
                download(book, &format!("ratio-analysis-{}-to-{}", p.from, p.to), f.format, c.finish())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("Demo Traders"), "demo-traders");
        assert_eq!(slug("Mehta & Co. \"Ltd\"\r\n"), "mehta-co-ltd");
        assert_eq!(slug("मेहता"), "company");
        assert_eq!(slug("ledger-Rent-2026-04-01-to-2027-03-31"), "ledger-rent-2026-04-01-to-2027-03-31");
    }
}

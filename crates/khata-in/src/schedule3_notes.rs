//! Notes to accounts for the Schedule III statements ([`crate::schedule3`]),
//! built from the books: one note per numbered line of the Balance Sheet and
//! the Statement of Profit and Loss (numbered on from the Balance Sheet's),
//! then the ratios and the additional regulatory information.
//!
//! What each note holds and where it comes from is in
//! `docs/india-schedule-3.md`; the Schedule III requirements behind them
//! (2021 amendment) are cited in `docs/research/india-schedule-3.md`.
//! Every note's total ties to its statement line, because a note is built
//! from that line's own ledgers and computed figures:
//!
//! - share capital: the line's ledgers, with the share particulars and
//!   promoter shareholding to be supplied;
//! - reserves and surplus (other equity): each ledger's movement (opening,
//!   additions, deductions, closing) and the surplus (brought forward,
//!   profit for the year, closing);
//! - borrowings: secured, unsecured and unclassified, with current
//!   maturities of long-term borrowings apart;
//! - trade payables and receivables: the classification and the ageing
//!   schedule from bill-wise outstanding, for both dates;
//! - PPE and intangibles: opening, additions, disposals, depreciation (a
//!   credit in a voucher that debits a depreciation ledger) and closing;
//!   capital work-in-progress with its ageing (oldest debits used first);
//! - inventories by stock group; cash on hand and balances with banks;
//!   changes in inventories; every other line by ledger.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Months, NaiveDate};
use khata_compliance::FinancialYear;
use khata_core::chart::{GroupId, LedgerId, Nature};
use khata_core::voucher::{BillKind, DrCr};
use khata_core::{Amount, Book};
use serde::Serialize;

use crate::FY_START_MONTH;
use crate::schedule3::{
    self, BalanceSheet, Either, Figures, IndAsLine, LedgerAmount, LineItem, Layout, Mapper, Mapping, NoteKind, ProfitAndLoss,
    Rounding, Row, RowKind, SourceKind, StatementLine, Tag,
};

/// One cell of a note's table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Cell {
    Empty,
    /// Money, in the statements' unit.
    Amount(Amount),
    /// A ratio in times ("1.52").
    Ratio(Amount),
    Percent(Amount),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteRow {
    pub kind: RowKind,
    pub level: u8,
    pub label: String,
    pub cells: Vec<Cell>,
    /// The ledger behind the row, for drill-down.
    pub ledger: Option<LedgerId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteTable {
    pub caption: Option<String>,
    /// Titles of the cell columns (the label column has none).
    pub columns: Vec<String>,
    pub rows: Vec<NoteRow>,
}

/// Which statement a note belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteOf {
    BalanceSheet,
    ProfitAndLoss,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub number: u32,
    pub title: String,
    pub of: NoteOf,
    /// Line code of the statement line ("trade_payables"), if any.
    pub code: Option<String>,
    pub tables: Vec<NoteTable>,
    /// What the books cannot give and the company must supply.
    pub remarks: Vec<String>,
}

/// One of the eleven ratios Schedule III asks for (2021 amendment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ratio {
    pub name: String,
    pub numerator: String,
    pub denominator: String,
    /// "times" or "%".
    pub unit: String,
    pub current: Option<Amount>,
    pub previous: Option<Amount>,
    /// Change against the previous year, in per cent.
    pub variance_pct: Option<Amount>,
    /// More than 25% change: Schedule III asks for the reason.
    pub explain: bool,
    /// What is missing or assumed.
    pub remark: Option<String>,
}

/// The statements, their notes and the ratios, as at one date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Statements<K = LineItem> {
    pub division: u8,
    pub rounding: Rounding,
    pub unit: String,
    pub as_at: NaiveDate,
    pub previous_as_at: Option<NaiveDate>,
    pub balance_sheet: BalanceSheet<K>,
    pub profit_and_loss: ProfitAndLoss<K>,
    pub notes: Vec<Note>,
    pub ratios: Vec<Ratio>,
    pub warnings: Vec<String>,
}

pub type AnyStatements = Either<Statements<LineItem>, Statements<IndAsLine>>;

impl<K> Statements<K> {
    /// Show every money figure in `r`'s unit (ratios are unchanged).
    pub fn round(&mut self, r: Rounding) {
        self.balance_sheet.round(r);
        self.profit_and_loss.round(r);
        for cell in self.notes.iter_mut().flat_map(|n| &mut n.tables).flat_map(|t| &mut t.rows).flat_map(|r| &mut r.cells) {
            if let Cell::Amount(a) = cell {
                *a = r.round(*a);
            }
        }
        self.rounding = r;
        self.unit = r.unit().to_owned();
    }
}

fn show(d: NaiveDate) -> String {
    d.format("%-d-%b-%Y").to_string()
}

fn fy(d: NaiveDate) -> FinancialYear {
    FinancialYear::containing(d, FY_START_MONTH)
}

fn amounts(a: Amount, b: Amount) -> Vec<Cell> {
    vec![Cell::Amount(a), Cell::Amount(b)]
}

fn nr(kind: RowKind, level: u8, label: impl Into<String>, cells: Vec<Cell>) -> NoteRow {
    NoteRow { kind, level, label: label.into(), cells, ledger: None }
}

fn heading(level: u8, label: impl Into<String>) -> NoteRow {
    nr(RowKind::Heading, level, label, Vec::new())
}

fn ledger_row(l: &LedgerAmount, level: u8) -> NoteRow {
    NoteRow { kind: RowKind::Line, level, label: l.name.clone(), cells: amounts(l.amount, l.previous), ledger: Some(l.ledger) }
}

fn table(caption: Option<String>, columns: Vec<String>, rows: Vec<NoteRow>) -> NoteTable {
    NoteTable { caption, columns, rows }
}

/// The date opening balances are taken to date from: the start of the
/// financial year of the first voucher (the books keep no other date).
fn books_begin(book: &Book, as_at: NaiveDate) -> NaiveDate {
    let first = book.active_vouchers().map(|v| v.date).min().unwrap_or(as_at);
    fy(first).start
}

struct Ctx<'a> {
    book: &'a Book,
    mapper: Mapper<'a>,
    mapping: &'a Mapping,
    as_at: NaiveDate,
    previous_as_at: Option<NaiveDate>,
    division: u8,
    /// Column titles: balance-sheet dates and profit-and-loss years.
    bs_heads: Vec<String>,
    pl_heads: Vec<String>,
    begin: NaiveDate,
    warnings: Vec<String>,
}

impl Ctx<'_> {
    fn dates(&self) -> [Option<NaiveDate>; 2] {
        [Some(self.as_at), self.previous_as_at]
    }

    fn under(&self, ledger: LedgerId, default_group: &str) -> bool {
        let target = GroupId::default_named(default_group);
        self.book.ledger(ledger).is_some_and(|l| schedule3::chain(self.book, l.group).contains(&target))
    }

    fn is_depreciation(&self, ledger: LedgerId) -> bool {
        let Some(l) = self.book.ledger(ledger) else { return false };
        if self.mapper.of_ledger(ledger).is_some_and(|p| p.item == LineItem::DepreciationAndAmortisation) {
            return true;
        }
        let name = l.name.to_lowercase();
        self.mapper.nature_of(ledger) == Some(Nature::Expenses) && (name.contains("depreciation") || name.contains("amortis") || name.contains("amortiz"))
    }
}

// ----------------------------------------------------------------- notes

fn ledgers_note<K: StatementLine>(r: &Row<K>, heads: &[String]) -> NoteTable {
    let mut rows = Vec::new();
    for s in r.sources.iter().filter(|s| s.kind != SourceKind::Group) {
        rows.push(nr(RowKind::Line, 1, s.name.clone(), amounts(s.amount, s.previous)));
    }
    rows.extend(r.ledgers.iter().map(|l| ledger_row(l, 1)));
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    table(None, heads.to_vec(), rows)
}

fn sign<K: StatementLine>(k: K) -> Amount {
    if k.debit_positive() { Amount::ONE } else { -Amount::ONE }
}

/// Opening (start of `date`'s financial year), increases, decreases and
/// closing of a ledger, in the line's sign.
fn movement(book: &Book, ledger: LedgerId, s: Amount, date: NaiveDate) -> [Amount; 4] {
    let start = fy(date).start;
    let opening = start.pred_opt().map_or(Amount::ZERO, |d| book.balances(Some(d)).get(&ledger).copied().unwrap_or_default()) * s;
    let (mut inc, mut dec) = (Amount::ZERO, Amount::ZERO);
    for v in book.active_vouchers().filter(|v| v.date >= start && v.date <= date) {
        for e in v.entries.iter().filter(|e| e.ledger == ledger) {
            let a = e.signed() * s;
            if a.is_sign_negative() { dec -= a } else { inc += a }
        }
    }
    [opening, inc, dec, opening + inc - dec]
}

fn reserves_note<K: StatementLine>(c: &Ctx, r: &Row<K>, item: K) -> NoteTable {
    let s = sign(item);
    let mut rows = Vec::new();
    for l in &r.ledgers {
        let m = c.dates().map(|d| d.map(|d| movement(c.book, l.ledger, s, d)).unwrap_or_default());
        rows.push(NoteRow { ledger: Some(l.ledger), ..heading(1, l.name.clone()) });
        for (i, label) in ["Opening balance", "Add: additions during the year", "Less: deductions during the year"].iter().enumerate() {
            rows.push(nr(RowKind::Line, 2, *label, amounts(m[0][i], m[1][i])));
        }
        rows.push(nr(RowKind::Total, 2, "Closing balance", amounts(m[0][3], m[1][3])));
    }
    let bf = r.sources.iter().find(|x| x.kind == SourceKind::ProfitBroughtForward);
    let year = r.sources.iter().find(|x| x.kind == SourceKind::ProfitForTheYear);
    if bf.is_some() || year.is_some() {
        let f = |x: Option<&schedule3::Source>| x.map_or(Figures::default(), |x| Figures { current: x.amount, previous: x.previous });
        let (bf, year) = (f(bf), f(year));
        rows.push(heading(1, "Surplus in the Statement of Profit and Loss"));
        rows.push(nr(RowKind::Line, 2, "Opening balance", amounts(bf.current, bf.previous)));
        rows.push(nr(RowKind::Line, 2, "Add: Profit (loss) for the year", amounts(year.current, year.previous)));
        let close = schedule3::add(bf, year);
        rows.push(nr(RowKind::Total, 2, "Closing balance", amounts(close.current, close.previous)));
    }
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    table(None, c.bs_heads.clone(), rows)
}

/// Secured (`Some(true)`), unsecured or not known.
fn security(c: &Ctx, l: &LedgerAmount) -> Option<bool> {
    if l.tags.has(Tag::Secured) {
        return Some(true);
    }
    if l.tags.has(Tag::Unsecured) || c.under(l.ledger, "Unsecured Loans") {
        return Some(false);
    }
    // Assumption: cash credit is secured on current assets.
    if c.under(l.ledger, "Secured Loans") || c.under(l.ledger, "Bank OCC A/c") {
        return Some(true);
    }
    None
}

fn borrowings_note<K: StatementLine>(c: &mut Ctx, r: &Row<K>) -> (NoteTable, Vec<String>) {
    let mut rows = Vec::new();
    let block = |rows: &mut Vec<NoteRow>, label: &str, ls: Vec<&LedgerAmount>| {
        if ls.is_empty() {
            return;
        }
        rows.push(heading(1, label));
        rows.extend(ls.iter().map(|l| ledger_row(l, 2)));
        let f = ls.iter().fold(Figures::default(), |a, l| schedule3::add(a, Figures { current: l.amount, previous: l.previous }));
        rows.push(nr(RowKind::Total, 1, format!("Total {}", label.to_lowercase()), amounts(f.current, f.previous)));
    };
    let (cm, rest): (Vec<&LedgerAmount>, Vec<&LedgerAmount>) = r.ledgers.iter().partition(|l| l.tags.has(Tag::CurrentMaturities));
    block(&mut rows, "Current maturities of long-term borrowings", cm);
    let (secured, rest): (Vec<&LedgerAmount>, Vec<&LedgerAmount>) = rest.into_iter().partition(|l| security(c, l) == Some(true));
    let (unsecured, unknown): (Vec<&LedgerAmount>, Vec<&LedgerAmount>) = rest.into_iter().partition(|l| security(c, l) == Some(false));
    block(&mut rows, "Secured", secured);
    block(&mut rows, "Unsecured", unsecured);
    if !unknown.is_empty() {
        let names: Vec<&str> = unknown.iter().map(|l| l.name.as_str()).collect();
        c.warnings.push(format!(
            "{}: {} not classified as secured or unsecured; map their groups with \":secured\" or \":unsecured\".",
            r.label,
            names.join(", ")
        ));
    }
    block(&mut rows, "Not classified as secured or unsecured", unknown);
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    let remarks = vec![
        "To be supplied: nature of security for each secured borrowing; terms of repayment and rate of interest; \
         loans guaranteed by directors or others; period and amount of any continuing default in repayment of \
         principal or interest."
            .into(),
    ];
    (table(None, c.bs_heads.clone(), rows), remarks)
}

/// An open item of a ledger at a date: the date it ages from, its due
/// date, and its amount (debit positive).
struct Item {
    ledger: LedgerId,
    date: Option<NaiveDate>,
    due: Option<NaiveDate>,
    amount: Amount,
}

/// Bill-wise outstanding of `ledgers` at `as_at`, as the core's
/// outstanding-bills report builds it, but for any ledger and with each
/// unallocated entry kept on its own date. Whatever the bills do not
/// explain is added as of `as_at`, so the items always sum to the balance.
fn open_items(book: &Book, ledgers: &BTreeSet<LedgerId>, as_at: NaiveDate) -> Vec<Item> {
    type Open = (Option<NaiveDate>, Option<NaiveDate>, Amount);
    let mut open: BTreeMap<(LedgerId, String), Open> = BTreeMap::new();
    for l in book.ledgers().filter(|l| ledgers.contains(&l.id) && !l.opening.is_zero()) {
        open.insert((l.id, "\u{0}opening".into()), (None, None, l.opening));
    }
    let mut vs: Vec<_> = book.active_vouchers().filter(|v| v.date <= as_at).collect();
    vs.sort_by_key(|v| (v.date, v.id));
    for v in vs {
        for e in v.entries.iter().filter(|e| ledgers.contains(&e.ledger)) {
            if e.bills.is_empty() {
                let slot = open.entry((e.ledger, format!("\u{0}{}", v.id.0))).or_insert((Some(v.date), None, Amount::ZERO));
                slot.2 += e.signed();
                continue;
            }
            for b in &e.bills {
                let signed = if e.side == DrCr::Dr { b.amount } else { -b.amount };
                let key = if b.kind == BillKind::OnAccount { format!("\u{0}{}", v.id.0) } else { b.reference.clone() };
                let slot = open.entry((e.ledger, key)).or_insert((Some(v.date), b.due, Amount::ZERO));
                if matches!(b.kind, BillKind::New | BillKind::Advance) && slot.1.is_none() {
                    slot.1 = b.due;
                }
                slot.2 += signed;
            }
        }
    }
    let mut items: Vec<Item> = open
        .into_iter()
        .filter(|(_, (_, _, a))| !a.is_zero())
        .map(|((ledger, _), (date, due, amount))| Item { ledger, date, due, amount })
        .collect();
    let balances = book.balances(Some(as_at));
    for l in ledgers {
        let explained: Amount = items.iter().filter(|i| i.ledger == *l).map(|i| i.amount).sum();
        let rest = balances.get(l).copied().unwrap_or_default() - explained;
        if !rest.is_zero() {
            items.push(Item { ledger: *l, date: Some(as_at), due: None, amount: rest });
        }
    }
    items
}

/// Column of an item: 0 "Not due", then one per age band (`edges` in
/// months, newest first) and a last one for anything older.
fn age_column(item: &Item, as_at: NaiveDate, begin: NaiveDate, edges: &[u32]) -> usize {
    if item.due.is_some_and(|d| d > as_at) {
        return 0;
    }
    let from = item.due.or(item.date).unwrap_or(begin);
    for (i, m) in edges.iter().enumerate() {
        let edge = as_at.checked_sub_months(Months::new(*m)).unwrap_or(NaiveDate::MIN);
        if from > edge {
            return i + 1;
        }
    }
    edges.len() + 1
}

/// The shape of an ageing schedule: age bands (months, newest first),
/// column titles and row categories.
struct Bands<'a> {
    edges: &'a [u32],
    columns: &'a [&'a str],
    categories: &'a [&'a str],
}

/// An ageing schedule: one row per category, a column per age band.
fn ageing(c: &Ctx, ledgers: &[LedgerAmount], s: Amount, date: NaiveDate, bands: &Bands, category: &dyn Fn(&LedgerAmount) -> usize) -> NoteTable {
    let Bands { edges, columns, categories } = *bands;
    let set: BTreeSet<LedgerId> = ledgers.iter().map(|l| l.ledger).collect();
    let by_id: BTreeMap<LedgerId, &LedgerAmount> = ledgers.iter().map(|l| (l.ledger, l)).collect();
    let width = edges.len() + 2;
    let mut grid = vec![vec![Amount::ZERO; width]; categories.len()];
    for item in open_items(c.book, &set, date) {
        let row = category(by_id[&item.ledger]);
        grid[row][age_column(&item, date, c.begin, edges)] += item.amount * s;
    }
    let mut rows: Vec<NoteRow> = Vec::new();
    let mut totals = vec![Amount::ZERO; width + 1];
    for (label, cells) in categories.iter().zip(&grid) {
        let total: Amount = cells.iter().copied().sum();
        for (t, v) in totals.iter_mut().zip(cells.iter().chain(std::iter::once(&total))) {
            *t += *v;
        }
        let mut cs: Vec<Cell> = cells.iter().map(|a| Cell::Amount(*a)).collect();
        cs.push(Cell::Amount(total));
        rows.push(nr(RowKind::Line, 1, *label, cs));
    }
    rows.push(nr(RowKind::Total, 0, "Total", totals.into_iter().map(Cell::Amount).collect()));
    let mut cols: Vec<String> = columns.iter().map(|s| (*s).to_owned()).collect();
    cols.push("Total".into());
    table(Some(format!("Ageing as at {} (outstanding from the due date of payment)", show(date))), cols, rows)
}

fn sum_by<'a>(ls: impl Iterator<Item = &'a LedgerAmount>) -> Figures {
    ls.fold(Figures::default(), |a, l| schedule3::add(a, Figures { current: l.amount, previous: l.previous }))
}

fn payables_note<K: StatementLine>(c: &Ctx, r: &Row<K>, item: K) -> (Vec<NoteTable>, Vec<String>) {
    let msme = |l: &LedgerAmount| schedule3::is_msme(c.mapping, l);
    let small = sum_by(r.ledgers.iter().filter(|l| msme(l)));
    let others = sum_by(r.ledgers.iter().filter(|l| !msme(l)));
    let mut rows = vec![
        nr(RowKind::Line, 1, "(A) Total outstanding dues of micro enterprises and small enterprises", amounts(small.current, small.previous)),
        nr(RowKind::Line, 1, "(B) Total outstanding dues of creditors other than micro enterprises and small enterprises", amounts(others.current, others.previous)),
    ];
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    let mut tables = vec![table(None, c.bs_heads.clone(), rows)];
    let category = |l: &LedgerAmount| match (msme(l), l.tags.has(Tag::Disputed)) {
        (true, false) => 0,
        (false, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    };
    for date in c.dates().into_iter().flatten() {
        tables.push(ageing(
            c,
            &r.ledgers,
            sign(item),
            date,
            &Bands {
                edges: &[12, 24, 36],
                columns: &["Not due", "Less than 1 year", "1-2 years", "2-3 years", "More than 3 years"],
                categories: &["(i) MSME", "(ii) Others", "(iii) Disputed dues – MSME", "(iv) Disputed dues – Others"],
            },
            &category,
        ));
    }
    let remarks = vec![
        "Ageing is from the due date of each bill (bill date when there is none); opening balances age from the \
         start of the books. Unbilled dues are not tracked and must be added separately."
            .into(),
        "To be supplied (MSMED Act, s.22): principal and interest due and unpaid to micro and small enterprises at \
         the year end, interest paid under s.16 with payments made after the appointed day, interest due for the \
         delay, and interest accrued and remaining unpaid."
            .into(),
    ];
    (tables, remarks)
}

fn receivables_note<K: StatementLine>(c: &Ctx, r: &Row<K>, item: K) -> (Vec<NoteTable>, Vec<String>) {
    let doubtful = |l: &LedgerAmount| l.tags.has(Tag::Doubtful);
    let secured = |l: &LedgerAmount| l.tags.has(Tag::Secured) && !doubtful(l);
    let unsecured = |l: &LedgerAmount| !l.tags.has(Tag::Secured) && !doubtful(l);
    let f = |p: &dyn Fn(&LedgerAmount) -> bool| sum_by(r.ledgers.iter().filter(|l| p(l)));
    let (sg, ug, dd) = (f(&secured), f(&unsecured), f(&doubtful));
    let d2 = c.division == 2;
    let mut rows = vec![
        nr(RowKind::Line, 1, if d2 { "Considered good – Secured" } else { "Secured, considered good" }, amounts(sg.current, sg.previous)),
        nr(RowKind::Line, 1, if d2 { "Considered good – Unsecured" } else { "Unsecured, considered good" }, amounts(ug.current, ug.previous)),
    ];
    if d2 {
        rows.push(nr(RowKind::Line, 1, "Trade receivables which have significant increase in credit risk", amounts(Amount::ZERO, Amount::ZERO)));
    }
    rows.push(nr(RowKind::Line, 1, if d2 { "Trade receivables – credit impaired" } else { "Doubtful" }, amounts(dd.current, dd.previous)));
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    let mut tables = vec![table(None, c.bs_heads.clone(), rows)];
    let cats: &[&str] = if d2 {
        &[
            "(i) Undisputed Trade receivables – considered good",
            "(ii) Undisputed Trade Receivables – which have significant increase in credit risk",
            "(iii) Undisputed Trade Receivables – credit impaired",
            "(iv) Disputed Trade Receivables – considered good",
            "(v) Disputed Trade Receivables – which have significant increase in credit risk",
            "(vi) Disputed Trade Receivables – credit impaired",
        ]
    } else {
        &[
            "(i) Undisputed Trade receivables – considered good",
            "(ii) Undisputed Trade Receivables – considered doubtful",
            "(iii) Disputed Trade Receivables considered good",
            "(iv) Disputed Trade Receivables considered doubtful",
        ]
    };
    let category = |l: &LedgerAmount| {
        let (disputed, bad) = (l.tags.has(Tag::Disputed), doubtful(l));
        match (d2, disputed, bad) {
            (false, false, false) => 0,
            (false, false, true) => 1,
            (false, true, false) => 2,
            (false, true, true) => 3,
            (true, false, false) => 0,
            (true, false, true) => 2,
            (true, true, false) => 3,
            (true, true, true) => 5,
        }
    };
    for date in c.dates().into_iter().flatten() {
        tables.push(ageing(
            c,
            &r.ledgers,
            sign(item),
            date,
            &Bands {
                edges: &[6, 12, 24, 36],
                columns: &["Not due", "Less than 6 months", "6 months - 1 year", "1-2 years", "2-3 years", "More than 3 years"],
                categories: cats,
            },
            &category,
        ));
    }
    let remarks = vec![
        "Ageing is from the due date of each bill (bill date when there is none); opening balances age from the \
         start of the books. Unbilled dues are not tracked and must be added separately."
            .into(),
        "To be supplied: allowance for bad and doubtful debts; debts due by directors or officers, or by firms or \
         private companies in which a director is a partner, director or member."
            .into(),
    ];
    (tables, remarks)
}

fn fixed_assets_note<K: StatementLine>(c: &Ctx, r: &Row<K>) -> (Vec<NoteTable>, Vec<String>) {
    let mut tables = Vec::new();
    for date in c.dates().into_iter().flatten() {
        let start = fy(date).start;
        let opening = start.pred_opt().map(|d| c.book.balances(Some(d))).unwrap_or_default();
        let mut rows = Vec::new();
        let mut total = [Amount::ZERO; 5];
        for l in &r.ledgers {
            let mut m = [opening.get(&l.ledger).copied().unwrap_or_default(), Amount::ZERO, Amount::ZERO, Amount::ZERO, Amount::ZERO];
            for v in c.book.active_vouchers().filter(|v| v.date >= start && v.date <= date) {
                let depreciation = v.entries.iter().any(|e| e.side == DrCr::Dr && c.is_depreciation(e.ledger));
                for e in v.entries.iter().filter(|e| e.ledger == l.ledger) {
                    match (e.side, depreciation) {
                        (DrCr::Dr, _) => m[1] += e.amount,
                        (DrCr::Cr, true) => m[3] += e.amount,
                        (DrCr::Cr, false) => m[2] += e.amount,
                    }
                }
            }
            m[4] = m[0] + m[1] - m[2] - m[3];
            for (t, v) in total.iter_mut().zip(m) {
                *t += v;
            }
            rows.push(NoteRow { ledger: Some(l.ledger), ..nr(RowKind::Line, 1, l.name.clone(), m.iter().map(|a| Cell::Amount(*a)).collect()) });
        }
        rows.push(nr(RowKind::Total, 0, "Total", total.iter().map(|a| Cell::Amount(*a)).collect()));
        tables.push(table(
            Some(format!("Year ended {}", show(date))),
            ["Opening", "Additions", "Disposals", "Depreciation / amortisation", "Closing"].map(String::from).to_vec(),
            rows,
        ));
    }
    let remarks = vec![
        "Depreciation is a credit to the asset in a voucher that debits a depreciation ledger (a ledger under a \
         group mapped to depreciation and amortisation expense, or named \"Depreciation\"); other credits are \
         disposals. Figures are net of depreciation charged directly to the asset."
            .into(),
        "To be supplied: gross block and accumulated depreciation if kept outside the books; title deeds of immovable \
         property not held in the company's name; revaluation, and whether by a registered valuer."
            .into(),
    ];
    (tables, remarks)
}

fn work_in_progress_note<K: StatementLine>(c: &Ctx, r: &Row<K>) -> (Vec<NoteTable>, Vec<String>) {
    let mut tables = vec![ledgers_note(r, &c.bs_heads)];
    for date in c.dates().into_iter().flatten() {
        let bal = c.book.balances(Some(date));
        let mut cells = [Amount::ZERO; 4];
        for l in &r.ledgers {
            // What is left of the balance is the newest debits.
            let mut left = bal.get(&l.ledger).copied().unwrap_or_default();
            let mut debits: Vec<(NaiveDate, Amount)> = c
                .book
                .active_vouchers()
                .filter(|v| v.date <= date)
                .flat_map(|v| v.entries.iter().filter(|e| e.ledger == l.ledger && e.side == DrCr::Dr).map(move |e| (v.date, e.amount)))
                .collect();
            if let Some(o) = c.book.ledger(l.ledger).map(|x| x.opening).filter(|o| o.is_sign_positive() && !o.is_zero()) {
                debits.push((c.begin, o));
            }
            debits.sort_by_key(|d| std::cmp::Reverse(d.0));
            let column = |d: NaiveDate| {
                let item = Item { ledger: l.ledger, date: Some(d), due: None, amount: Amount::ZERO };
                age_column(&item, date, c.begin, &[12, 24, 36]) - 1
            };
            for (d, a) in debits {
                if left <= Amount::ZERO {
                    break;
                }
                let take = left.min(a);
                cells[column(d)] += take;
                left -= take;
            }
            // Negative or unexplained balances go in the newest band so the total ties.
            if !left.is_zero() {
                cells[if left.is_sign_negative() { 0 } else { 3 }] += left;
            }
        }
        let total: Amount = cells.iter().copied().sum();
        let mut row: Vec<Cell> = cells.iter().map(|a| Cell::Amount(*a)).collect();
        row.push(Cell::Amount(total));
        let rows = vec![
            nr(RowKind::Line, 1, "Projects in progress", row.clone()),
            nr(RowKind::Line, 1, "Projects temporarily suspended", vec![Cell::Amount(Amount::ZERO); 5]),
            nr(RowKind::Total, 0, "Total", row),
        ];
        tables.push(table(
            Some(format!("Ageing as at {} (amount for a period of)", show(date))),
            ["Less than 1 year", "1-2 years", "2-3 years", "More than 3 years", "Total"].map(String::from).to_vec(),
            rows,
        ));
    }
    let remarks = vec![
        "Ageing takes the oldest debits as used up first. All work is shown as projects in progress: which projects \
         are temporarily suspended, and the completion schedule of projects that are overdue or over their \
         original cost, must be supplied."
            .into(),
    ];
    (tables, remarks)
}

fn inventories_note<K: StatementLine>(c: &Ctx, r: &Row<K>) -> (NoteTable, Vec<String>) {
    let mut by_group: BTreeMap<String, [Amount; 2]> = BTreeMap::new();
    for (p, date) in c.dates().into_iter().enumerate() {
        let Some(date) = date else { continue };
        if c.book.stock_items().next().is_none() {
            continue;
        }
        for (id, m) in khata_core::inventory::movements(c.book, NaiveDate::MIN, date) {
            let group = c.book.stock_item(id).and_then(|i| i.group);
            let path = khata_core::inventory::stock_group_path(c.book, group);
            let top = path.split(" / ").next().filter(|s| !s.is_empty()).unwrap_or("Stock items not in a group").to_owned();
            by_group.entry(top).or_default()[p] += m.closing_value;
        }
    }
    let mut rows: Vec<NoteRow> =
        by_group.into_iter().filter(|(_, v)| v.iter().any(|a| !a.is_zero())).map(|(g, v)| nr(RowKind::Line, 1, g, amounts(v[0], v[1]))).collect();
    rows.extend(r.ledgers.iter().map(|l| ledger_row(l, 1)));
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    let remarks = vec![
        "Closing stock by top-level stock group, valued as in the Stock Summary. To be supplied: the mode of \
         valuation, and the split into raw materials, work-in-progress, finished goods, stock-in-trade, stores and \
         spares, and loose tools (name stock groups after these heads to get it here), with goods in transit."
            .into(),
    ];
    (table(None, c.bs_heads.clone(), rows), remarks)
}

fn cash_note<K: StatementLine>(c: &Ctx, r: &Row<K>) -> NoteTable {
    let (cash, banks): (Vec<&LedgerAmount>, Vec<&LedgerAmount>) = r.ledgers.iter().partition(|l| c.under(l.ledger, "Cash-in-Hand"));
    let mut rows = Vec::new();
    for (label, ls) in [("Cash on hand", cash), ("Balances with banks", banks)] {
        if ls.is_empty() {
            continue;
        }
        rows.push(heading(1, label));
        rows.extend(ls.iter().map(|l| ledger_row(l, 2)));
        let f = sum_by(ls.into_iter());
        rows.push(nr(RowKind::Total, 1, format!("Total {}", label.to_lowercase()), amounts(f.current, f.previous)));
    }
    rows.push(nr(RowKind::Total, 0, "Total", amounts(r.amount, r.previous)));
    table(None, c.bs_heads.clone(), rows)
}

fn changes_note<K: StatementLine>(c: &Ctx, r: &Row<K>) -> NoteTable {
    let f = |k: SourceKind| r.sources.iter().find(|s| s.kind == k).map_or(Figures::default(), |s| Figures { current: s.amount, previous: s.previous });
    let (open, close) = (f(SourceKind::OpeningStock), f(SourceKind::ClosingStock));
    let rows = vec![
        nr(RowKind::Line, 1, "Inventories at the beginning of the year", amounts(open.current, open.previous)),
        nr(RowKind::Line, 1, "Less: Inventories at the end of the year", amounts(-close.current, -close.previous)),
        nr(RowKind::Total, 0, "(Increase)/decrease in inventories", amounts(r.amount, r.previous)),
    ];
    table(None, c.pl_heads.clone(), rows)
}

fn share_capital_note<K: StatementLine>(c: &Ctx, r: &Row<K>) -> (Vec<NoteTable>, Vec<String>) {
    let promoters = table(
        Some("Shares held by promoters at the end of the year".into()),
        ["No. of shares", "% of total shares", "% change during the year"].map(String::from).to_vec(),
        vec![nr(RowKind::Line, 1, "To be supplied by the company", vec![Cell::Empty, Cell::Empty, Cell::Empty])],
    );
    let remarks = vec![
        "The books hold the amount only. To be supplied for each class of shares: authorised, issued, subscribed \
         and fully paid, and subscribed but not fully paid shares, with their number and par value; a \
         reconciliation of shares at the beginning and end of the year; rights and restrictions; shares held by \
         the holding company; shareholders holding more than 5%; shares reserved for options; and the promoter \
         shareholding above (2021 amendment)."
            .into(),
    ];
    (vec![ledgers_note(r, &c.bs_heads), promoters], remarks)
}

fn line_note<K: StatementLine>(c: &mut Ctx, r: &Row<K>, of: NoteOf) -> Option<Note> {
    let item = r.item?;
    let number = r.note?;
    let heads = if of == NoteOf::BalanceSheet { c.bs_heads.clone() } else { c.pl_heads.clone() };
    let (tables, remarks) = match item.note_kind() {
        NoteKind::ShareCapital => share_capital_note(c, r),
        NoteKind::Reserves => (vec![reserves_note(c, r, item)], Vec::new()),
        NoteKind::Borrowings => {
            let (t, rem) = borrowings_note(c, r);
            (vec![t], rem)
        }
        NoteKind::TradePayables => payables_note(c, r, item),
        NoteKind::TradeReceivables => receivables_note(c, r, item),
        NoteKind::FixedAssets => fixed_assets_note(c, r),
        NoteKind::WorkInProgress => work_in_progress_note(c, r),
        NoteKind::Inventories => {
            let (t, rem) = inventories_note(c, r);
            (vec![t], rem)
        }
        NoteKind::Cash => (vec![cash_note(c, r)], Vec::new()),
        NoteKind::ChangesInInventories => (vec![changes_note(c, r)], Vec::new()),
        NoteKind::Ledgers => (vec![ledgers_note(r, &heads)], Vec::new()),
    };
    Some(Note { number, title: item.note_title(), of, code: Some(item.code()), tables, remarks })
}

// ----------------------------------------------------------------- ratios

/// Figures of a Division I line on a statement (zero when not shown).
fn line_figures<K: StatementLine>(rows: &[Row<K>], items: &[K]) -> Figures {
    rows.iter()
        .filter(|r| r.kind == RowKind::Line && r.item.is_some_and(|i| items.contains(&i)))
        .fold(Figures::default(), |a, r| schedule3::add(a, Figures { current: r.amount, previous: r.previous }))
}

fn div(n: Amount, d: Amount) -> Option<Amount> {
    if d.is_zero() { None } else { n.checked_div(d).map(|r| r.round_dp(2)) }
}

/// The eleven ratios, from the Division I statements (the same totals as
/// Division II). Averages use the opening and closing balance sheets; a
/// first year uses the closing one.
fn ratios(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>, mapping: &Mapping, mapper: &Mapper) -> Vec<Ratio> {
    use LineItem as L;
    let bs = schedule3::balance_sheet(book, as_at, previous_as_at, mapping);
    let start = fy(as_at).start;
    let prev_period = previous_as_at.map(|p| (fy(p).start, p));
    let pl = schedule3::profit_and_loss(book, start, as_at, prev_period, mapping);
    let older = previous_as_at.and_then(|p| fy(p).start.pred_opt()).map(|d| schedule3::balance_sheet(book, d, None, mapping));
    let (ca_items, cl_items) = schedule3::current_assets_and_liabilities();
    // [current closing, previous closing], and the opening of each year.
    let closing = |items: &[L]| line_figures(&bs.rows, items);
    let opening = |items: &[L]| -> [Option<Amount>; 2] {
        let f = closing(items);
        [previous_as_at.map(|_| f.previous), older.as_ref().map(|o| line_figures(&o.rows, items).current)]
    };
    let both = |f: Figures| [f.current, f.previous];
    let average = |items: &[L]| -> [Amount; 2] {
        let c = both(closing(items));
        let o = opening(items);
        [0, 1].map(|p| o[p].map_or(c[p], |o| (c[p] + o) / Amount::TWO))
    };
    let flow = |items: &[L]| both(line_figures(&pl.rows, items));
    let equity_items = [L::ShareCapital, L::ReservesAndSurplus, L::MoneyReceivedAgainstShareWarrants];
    let debt_items = [L::LongTermBorrowings, L::ShortTermBorrowings];
    let ca = both(closing(&ca_items));
    let cl = both(closing(&cl_items));
    let debt = both(closing(&debt_items));
    let equity = both(closing(&equity_items));
    let pat = both(pl.profit_for_the_period);
    let pbt = both(pl.profit_before_tax);
    let revenue = flow(&[L::RevenueFromOperations]);
    let finance = flow(&[L::FinanceCosts]);
    let depreciation = flow(&[L::DepreciationAndAmortisation]);
    let cogs = flow(&[L::CostOfMaterialsConsumed, L::PurchasesOfStockInTrade, L::ChangesInInventories]);
    let purchases = flow(&[L::CostOfMaterialsConsumed, L::PurchasesOfStockInTrade]);
    let intangibles = both(closing(&[L::IntangibleAssets, L::IntangibleAssetsUnderDevelopment]));
    let dtl = both(closing(&[L::DeferredTaxLiabilities]));
    let avg_equity = average(&equity_items);
    let avg_inventory = average(&[L::Inventories]);
    let avg_receivables = average(&[L::TradeReceivables]);
    let avg_payables = average(&[L::TradePayables]);
    let wc_open = {
        let a = opening(&ca_items);
        let l = opening(&cl_items);
        [0, 1].map(|p| a[p].zip(l[p]).map(|(a, l)| a - l))
    };
    let avg_wc = [0, 1].map(|p| wc_open[p].map_or(ca[p] - cl[p], |o| (ca[p] - cl[p] + o) / Amount::TWO));
    // Principal repaid: debits to long-term borrowings (and current
    // maturities) in vouchers that move no other borrowing.
    let is_debt = |l: LedgerId| mapper.of_ledger(l).is_some_and(|p| p.item == L::LongTermBorrowings || p.tags.has(Tag::CurrentMaturities));
    let periods = [Some((start, as_at)), prev_period];
    let repaid = periods.map(|p| {
        let Some((f, t)) = p else { return Amount::ZERO };
        book.active_vouchers()
            .filter(|v| v.date >= f && v.date <= t && !v.entries.iter().any(|e| e.side == DrCr::Cr && is_debt(e.ledger)))
            .flat_map(|v| v.entries.iter().filter(|e| e.side == DrCr::Dr && is_debt(e.ledger)).map(|e| e.amount))
            .sum::<Amount>()
    });
    let hundred = Amount::ONE_HUNDRED;
    let has_prev = previous_as_at.is_some();
    let pair = |f: &dyn Fn(usize) -> Option<Amount>| [f(0), if has_prev { f(1) } else { None }];
    let mut out = Vec::new();
    let mut push = |name: &str, num: &str, den: &str, pct: bool, v: [Option<Amount>; 2], remark: Option<&str>| {
        let variance = v[0].zip(v[1]).and_then(|(c, p)| div((c - p) * hundred, p.abs()));
        let explain = variance.is_some_and(|x| x.abs() > Amount::from(25));
        out.push(Ratio {
            name: name.into(),
            numerator: num.into(),
            denominator: den.into(),
            unit: if pct { "%".into() } else { "times".into() },
            current: v[0],
            previous: v[1],
            variance_pct: variance,
            explain,
            remark: remark.map(str::to_owned),
        });
    };
    push("Current ratio", "Current assets", "Current liabilities", false, pair(&|p| div(ca[p], cl[p])), None);
    push("Debt-equity ratio", "Total debt (long-term and short-term borrowings)", "Shareholders' equity", false, pair(&|p| div(debt[p], equity[p])), None);
    let service = [0, 1].map(|p| finance[p] + repaid[p]);
    let dscr = pair(&|p| div(pbt[p] + depreciation[p] + finance[p], service[p]));
    let dscr_note = if service.iter().all(|s| s.is_zero()) {
        Some("No interest (finance costs) or principal repayment is identified in the books; map finance costs and borrowings.")
    } else {
        Some("Principal repaid is the debits to long-term borrowings; lease payments and other adjustments are not included.")
    };
    push(
        "Debt service coverage ratio",
        "Earnings available for debt service (profit before tax + depreciation and amortisation + finance costs)",
        "Debt service (finance costs + principal repaid)",
        false,
        dscr,
        dscr_note,
    );
    push("Return on equity ratio", "Profit after tax (less preference dividend, if any)", "Average shareholders' equity", true, pair(&|p| div(pat[p] * hundred, avg_equity[p])), None);
    push("Inventory turnover ratio", "Cost of goods sold (materials + purchases + changes in inventories)", "Average inventory", false, pair(&|p| div(cogs[p], avg_inventory[p])), None);
    push(
        "Trade receivables turnover ratio",
        "Revenue from operations",
        "Average trade receivables",
        false,
        pair(&|p| div(revenue[p], avg_receivables[p])),
        Some("Credit sales are not separated in the books: all revenue from operations is used."),
    );
    push(
        "Trade payables turnover ratio",
        "Purchases (stock-in-trade and materials consumed)",
        "Average trade payables",
        false,
        pair(&|p| div(purchases[p], avg_payables[p])),
        Some("Credit purchases are not separated in the books: all purchases are used."),
    );
    push("Net capital turnover ratio", "Revenue from operations", "Average working capital (current assets − current liabilities)", false, pair(&|p| div(revenue[p], avg_wc[p])), None);
    push("Net profit ratio", "Profit after tax", "Revenue from operations", true, pair(&|p| div(pat[p] * hundred, revenue[p])), None);
    push(
        "Return on capital employed",
        "Earnings before interest and tax (profit before tax + finance costs)",
        "Capital employed (tangible net worth + total debt + deferred tax liability)",
        true,
        pair(&|p| div((pbt[p] + finance[p]) * hundred, equity[p] - intangibles[p] + debt[p] + dtl[p])),
        None,
    );
    push(
        "Return on investment",
        "Income from investments (time-weighted)",
        "Time-weighted average investments",
        true,
        [None, None],
        Some("Needs the market value of investments and the timing of cash flows, which the books do not hold."),
    );
    out
}

fn ratios_note(number: u32, ratios: &[Ratio], heads: &[String]) -> Note {
    let value = |v: Option<Amount>, pct: bool| match v {
        Some(v) if pct => Cell::Percent(v),
        Some(v) => Cell::Ratio(v),
        None => Cell::Text("n.a.".into()),
    };
    let rows = ratios
        .iter()
        .map(|r| {
            let pct = r.unit == "%";
            let reason = match (&r.remark, r.explain) {
                (Some(m), true) => format!("Change over 25%: explain. {m}"),
                (None, true) => "Change over 25%: explain.".into(),
                (Some(m), false) => m.clone(),
                (None, false) => String::new(),
            };
            nr(
                RowKind::Line,
                1,
                r.name.clone(),
                vec![
                    Cell::Text(r.numerator.clone()),
                    Cell::Text(r.denominator.clone()),
                    value(r.current, pct),
                    value(r.previous, pct),
                    r.variance_pct.map_or(Cell::Empty, Cell::Percent),
                    Cell::Text(reason),
                ],
            )
        })
        .collect();
    let mut columns = vec!["Numerator".to_owned(), "Denominator".to_owned()];
    columns.extend(heads.iter().cloned());
    columns.extend(["% variance".to_owned(), "Reason for variance".to_owned()]);
    Note {
        number,
        title: "Ratios".into(),
        of: NoteOf::Other,
        code: None,
        tables: vec![table(None, columns, rows)],
        remarks: vec!["Explain every ratio that changed by more than 25% against the previous year.".into()],
    }
}

/// The 2021 disclosures that need facts outside the books.
pub const ADDITIONAL_INFORMATION: &[&str] = &[
    "Title deeds of immovable property not held in the name of the company.",
    "Revaluation of property, plant and equipment (and, under Ind AS, right-of-use assets, intangibles and the fair value of investment property), and whether by a registered valuer.",
    "Loans or advances to promoters, directors, KMPs and related parties repayable on demand or without terms of repayment.",
    "Benami property: any proceedings under the Prohibition of Benami Property Transactions Act, 1988.",
    "Borrowings secured on current assets: whether the quarterly returns filed with banks agree with the books.",
    "Wilful defaulter declared by any bank, financial institution or lender.",
    "Transactions and balances with companies struck off under section 248.",
    "Charges or satisfaction yet to be registered with the Registrar of Companies.",
    "Compliance with the number of layers of companies (section 2(87)).",
    "Schemes of arrangement approved under sections 230 to 237.",
    "Utilisation of borrowed funds and share premium through intermediaries.",
    "Undisclosed income surrendered or disclosed in tax assessments.",
    "Corporate social responsibility (section 135): amount required, spent, shortfall and its reasons.",
    "Crypto currency or virtual currency: profit or loss, amount held, and deposits or advances for trading in it.",
];

/// The statements in layout `K` with every note, as at `as_at` (the year
/// is the financial year to `as_at`; the comparative year ends on
/// `previous_as_at`).
pub fn statements<K: Layout>(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>, mapping: &Mapping) -> Statements<K> {
    let bs = K::balance_sheet(book, as_at, previous_as_at, mapping);
    let start = fy(as_at).start;
    let mut pl = K::profit_and_loss(book, start, as_at, previous_as_at.map(|p| (fy(p).start, p)), mapping);
    let last = bs.rows.iter().filter_map(|r| r.note).max().unwrap_or(0);
    pl.renumber(last + 1);
    let as_at_head = |d: Option<NaiveDate>| d.map_or("Previous year".to_owned(), |d| format!("As at {}", show(d)));
    let year_head = |d: Option<NaiveDate>| d.map_or("Previous year".to_owned(), |d| format!("Year ended {}", show(d)));
    let mut c = Ctx {
        book,
        mapper: Mapper::new(book, mapping),
        mapping,
        as_at,
        previous_as_at,
        division: K::DIVISION,
        bs_heads: vec![as_at_head(Some(as_at)), as_at_head(previous_as_at)],
        pl_heads: vec![year_head(Some(as_at)), year_head(previous_as_at)],
        begin: books_begin(book, as_at),
        warnings: Vec::new(),
    };
    let mut notes = Vec::new();
    for r in &bs.rows {
        notes.extend(line_note(&mut c, r, NoteOf::BalanceSheet));
    }
    for r in &pl.rows {
        notes.extend(line_note(&mut c, r, NoteOf::ProfitAndLoss));
    }
    let ratios = ratios(book, as_at, previous_as_at, mapping, &c.mapper);
    let mut next = notes.iter().map(|n| n.number).max().unwrap_or(0) + 1;
    notes.push(ratios_note(next, &ratios, &c.pl_heads));
    next += 1;
    notes.push(Note {
        number: next,
        title: "Additional regulatory information".into(),
        of: NoteOf::Other,
        code: None,
        tables: Vec::new(),
        remarks: ADDITIONAL_INFORMATION.iter().map(|s| format!("To be supplied (not in the books): {s}")).collect(),
    });
    // A depreciation ledger outside the depreciation line.
    for l in book.ledgers() {
        let unmapped = c.is_depreciation(l.id) && c.mapper.of_ledger(l.id).is_some_and(|p| p.item != LineItem::DepreciationAndAmortisation);
        if unmapped {
            c.warnings.push(format!(
                "Ledger \"{}\" looks like depreciation but is shown under other expenses; map its group to depreciation and amortisation expense.",
                l.name
            ));
        }
    }
    let mut warnings = bs.warnings.clone();
    warnings.extend(pl.warnings.iter().cloned());
    warnings.extend(c.warnings);
    warnings.dedup();
    Statements {
        division: K::DIVISION,
        rounding: Rounding::Rupees,
        unit: Rounding::Rupees.unit().to_owned(),
        as_at,
        previous_as_at,
        balance_sheet: bs,
        profit_and_loss: pl,
        notes,
        ratios,
        warnings,
    }
}

/// The company's statements: its mapping, division and rounding unit from
/// the profile, with a warning when the unit does not suit the turnover
/// rule.
pub fn company_statements(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>) -> AnyStatements {
    let mapping = Mapping::of_book(book);
    let rounding = Rounding::of_book(book);
    fn finish<K>(mut s: Statements<K>, r: Rounding) -> Statements<K> {
        if let Some(w) = r.warning(s.profit_and_loss.total_income.current) {
            s.warnings.push(w);
        }
        s.round(r);
        s
    }
    match schedule3::division_of(book) {
        2 => Either::Division2(finish(statements::<IndAsLine>(book, as_at, previous_as_at, &mapping), rounding)),
        _ => Either::Division1(finish(statements::<LineItem>(book, as_at, previous_as_at, &mapping), rounding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule3::{RowKind, ind_as_balance_sheet, ind_as_profit_and_loss, parse_code};
    use chrono::{DateTime, Utc};
    use khata_core::{BillKind, Entry, Voucher, VoucherId, VoucherKind, reports};
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_760_000_000, 0).unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn g(name: &str) -> GroupId {
        GroupId::default_named(name)
    }

    struct F {
        book: Book,
        n: u32,
        l: BTreeMap<&'static str, LedgerId>,
    }

    impl F {
        fn ledger(&mut self, name: &'static str, group: GroupId, opening: Amount) {
            let id = self.book.create_ledger(now(), "u", name, group).unwrap();
            if !opening.is_zero() {
                self.book.set_opening_balance(now(), "u", id, opening).unwrap();
            }
            self.l.insert(name, id);
        }

        fn post(&mut self, date: NaiveDate, entries: Vec<Entry>) {
            self.n += 1;
            let v = Voucher {
                id: VoucherId::new(),
                kind: VoucherKind::Journal,
                number: self.n.to_string(),
                date,
                entries,
                narration: String::new(),
                cancelled: false,
                series: None,
                stock: Vec::new(),
            };
            self.book.post_voucher(now(), "u", v).unwrap();
        }

        fn pay(&mut self, date: NaiveDate, dr: &str, cr: &str, a: Amount) {
            let (dr, cr) = (self.l[dr], self.l[cr]);
            self.post(date, vec![Entry::dr(dr, a), Entry::cr(cr, a)]);
        }

        /// A bill to (debtor, Dr) or from (creditor, Cr) a party.
        #[allow(clippy::too_many_arguments)]
        fn bill(&mut self, date: NaiveDate, party: &str, other: &str, a: Amount, reference: &str, due: Option<NaiveDate>, receivable: bool) {
            let (p, o) = (self.l[party], self.l[other]);
            let entry = if receivable { Entry::dr(p, a) } else { Entry::cr(p, a) };
            let entry = entry.bill(BillKind::New, reference, a, due);
            let contra = if receivable { Entry::cr(o, a) } else { Entry::dr(o, a) };
            self.post(date, vec![entry, contra]);
        }
    }

    /// A company with mapped groups, MSME and disputed parties, bills of
    /// every age, fixed assets with additions, disposals and depreciation,
    /// capital work-in-progress and an OCI item, over two years.
    fn company() -> F {
        let mut f = F { book: Book::new("Notes Pvt Ltd"), n: 0, l: BTreeMap::new() };
        f.book.set_country(now(), "u", "IN").unwrap();
        let current = f.book.create_group(now(), "u", "Term Loan Current", g("Loans (Liability)")).unwrap();
        let disputed = f.book.create_group(now(), "u", "Disputed Creditors", g("Sundry Creditors")).unwrap();
        let cwip = f.book.create_group(now(), "u", "Capital WIP", g("Fixed Assets")).unwrap();
        let oci = f.book.create_group(now(), "u", "OCI Items", g("Indirect Incomes")).unwrap();
        let dep = f.book.create_group(now(), "u", "Depreciation", g("Indirect Expenses")).unwrap();
        let spec: &[(&'static str, GroupId, Amount)] = &[
            ("Equity", g("Capital Account"), dec!(-1000000)),
            ("General Reserve", g("Reserves & Surplus"), dec!(-100000)),
            ("Term Loan", g("Secured Loans"), dec!(-300000)),
            ("Director Loan", g("Unsecured Loans"), dec!(-50000)),
            ("Term Loan (current)", current, dec!(0)),
            ("Acme MSME", g("Sundry Creditors"), dec!(0)),
            ("Beta Supplies", g("Sundry Creditors"), dec!(0)),
            ("Gamma Disputed", disputed, dec!(0)),
            ("Delta Retail", g("Sundry Debtors"), dec!(50000)),
            ("Plant", g("Fixed Assets"), dec!(600000)),
            ("Building WIP", cwip, dec!(0)),
            ("SBI", g("Bank Accounts"), dec!(750000)),
            ("Cash", g("Cash-in-Hand"), dec!(50000)),
            ("Rent Deposit", g("Deposits (Asset)"), dec!(0)),
            ("Sales", g("Sales Accounts"), dec!(0)),
            ("Purchases", g("Purchase Accounts"), dec!(0)),
            ("Salaries", g("Indirect Expenses"), dec!(0)),
            ("Depreciation", dep, dec!(0)),
            ("Actuarial Gain", oci, dec!(0)),
        ];
        for (name, group, opening) in spec {
            f.ledger(name, *group, *opening);
        }
        let b = &mut f.book;
        b.set_schedule3_mapping(now(), "u", current, Some("short_term_borrowings:current_maturities")).unwrap();
        b.set_schedule3_mapping(now(), "u", disputed, Some("trade_payables:disputed")).unwrap();
        b.set_schedule3_mapping(now(), "u", cwip, Some("capital_work_in_progress")).unwrap();
        b.set_schedule3_mapping(now(), "u", oci, Some("oci_not_reclassified")).unwrap();
        b.set_schedule3_mapping(now(), "u", dep, Some("depreciation_and_amortisation")).unwrap();
        let acme = f.l["Acme MSME"];
        f.book.set_msme_party(now(), "u", acme, true).unwrap();

        // Previous year (2025-26).
        f.bill(d(2025, 10, 1), "Delta Retail", "Sales", dec!(200000), "S1", Some(d(2025, 10, 31)), true);
        f.bill(d(2025, 11, 1), "Beta Supplies", "Purchases", dec!(80000), "B1", Some(d(2025, 12, 1)), false);
        f.pay(d(2025, 12, 1), "Building WIP", "SBI", dec!(40000));
        f.pay(d(2026, 1, 1), "Salaries", "SBI", dec!(30000));
        // Current year (2026-27).
        f.pay(d(2026, 4, 5), "Delta Retail", "SBI", dec!(1)); // on account, no bill
        f.bill(d(2026, 5, 1), "Delta Retail", "Sales", dec!(300000), "S2", None, true);
        f.bill(d(2027, 1, 15), "Delta Retail", "Sales", dec!(100000), "S3", Some(d(2027, 2, 14)), true);
        f.bill(d(2027, 3, 20), "Delta Retail", "Sales", dec!(60000), "S4", Some(d(2027, 5, 1)), true);
        f.bill(d(2027, 2, 1), "Acme MSME", "Purchases", dec!(90000), "A1", Some(d(2027, 3, 3)), false);
        f.bill(d(2026, 6, 1), "Gamma Disputed", "Purchases", dec!(25000), "G1", Some(d(2026, 7, 1)), false);
        f.pay(d(2026, 6, 1), "Plant", "SBI", dec!(100000));
        f.pay(d(2026, 9, 1), "SBI", "Plant", dec!(20000));
        f.pay(d(2027, 3, 31), "Depreciation", "Plant", dec!(70000));
        f.pay(d(2026, 8, 1), "Building WIP", "SBI", dec!(60000));
        f.pay(d(2026, 10, 1), "Salaries", "SBI", dec!(45000));
        f.pay(d(2026, 11, 1), "Rent Deposit", "SBI", dec!(15000));
        f.pay(d(2027, 3, 31), "Term Loan", "Term Loan (current)", dec!(60000));
        f.pay(d(2026, 12, 1), "Term Loan", "SBI", dec!(30000));
        f.pay(d(2027, 2, 1), "SBI", "Actuarial Gain", dec!(5000));
        f
    }

    const AS_AT: (i32, u32, u32) = (2027, 3, 31);

    fn div1(f: &F) -> Statements {
        statements::<LineItem>(&f.book, d(AS_AT.0, AS_AT.1, AS_AT.2), Some(d(2026, 3, 31)), &Mapping::of_book(&f.book))
    }

    fn note<'a>(s: &'a Statements, title: &str) -> &'a Note {
        s.notes.iter().find(|n| n.title == title).unwrap_or_else(|| panic!("no note {title}"))
    }

    fn total(t: &NoteTable) -> &[Cell] {
        &t.rows.iter().rev().find(|r| r.kind == RowKind::Total && r.level == 0).expect("a total row").cells
    }

    fn amount(c: &Cell) -> Amount {
        match c {
            Cell::Amount(a) => *a,
            other => panic!("not an amount: {other:?}"),
        }
    }

    #[test]
    fn every_note_ties_to_its_statement_line() {
        let f = company();
        let s = div1(&f);
        assert_eq!(s.balance_sheet.total_assets, s.balance_sheet.total_equity_and_liabilities);
        let bs_notes: Vec<u32> = s.balance_sheet.rows.iter().filter_map(|r| r.note).collect();
        let pl_notes: Vec<u32> = s.profit_and_loss.rows.iter().filter_map(|r| r.note).collect();
        assert_eq!(pl_notes.first(), Some(&(bs_notes.last().unwrap() + 1)), "P&L notes follow the Balance Sheet's");
        let rows = s.balance_sheet.rows.iter().chain(&s.profit_and_loss.rows).filter(|r| r.note.is_some());
        for r in rows {
            let n = s.notes.iter().find(|n| Some(n.number) == r.note).unwrap_or_else(|| panic!("no note for {}", r.label));
            let item = r.item.unwrap();
            match item.note_kind() {
                NoteKind::FixedAssets => {
                    assert_eq!(amount(&total(&n.tables[0])[4]), r.amount, "{}", r.label);
                    assert_eq!(amount(&total(&n.tables[1])[4]), r.previous, "{}", r.label);
                }
                NoteKind::WorkInProgress => {
                    assert_eq!(amount(&total(&n.tables[0])[0]), r.amount);
                    assert_eq!(amount(&total(&n.tables[1])[4]), r.amount, "CWIP ageing ties");
                    assert_eq!(amount(&total(&n.tables[2])[4]), r.previous);
                }
                NoteKind::TradePayables | NoteKind::TradeReceivables => {
                    assert_eq!(amount(&total(&n.tables[0])[0]), r.amount, "{}", r.label);
                    let last = |t: &NoteTable| amount(total(t).last().unwrap());
                    assert_eq!(last(&n.tables[1]), r.amount, "{} ageing", r.label);
                    assert_eq!(last(&n.tables[2]), r.previous, "{} previous ageing", r.label);
                }
                _ => {
                    assert_eq!(amount(&total(&n.tables[0])[0]), r.amount, "{}", r.label);
                    assert_eq!(amount(&total(&n.tables[0])[1]), r.previous, "{}", r.label);
                }
            }
        }
        assert!(s.notes.iter().any(|n| n.title == "Ratios") && s.notes.iter().any(|n| n.title == "Additional regulatory information"));
    }

    #[test]
    fn notes_break_lines_down() {
        let f = company();
        let s = div1(&f);
        // Reserves: the reserve's movement and the surplus.
        let rs = note(&s, "Reserves and surplus");
        let labels: Vec<&str> = rs.tables[0].rows.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"General Reserve") && labels.contains(&"Surplus in the Statement of Profit and Loss"));
        // Borrowings: current maturities apart from secured and unsecured.
        let st = note(&s, "Short-term borrowings");
        assert_eq!(st.tables[0].rows[0].label, "Current maturities of long-term borrowings");
        assert_eq!(amount(&total(&st.tables[0])[0]), dec!(60000));
        let lt = note(&s, "Long-term borrowings");
        let secured = lt.tables[0].rows.iter().find(|r| r.label == "Total secured").unwrap();
        assert_eq!(amount(&secured.cells[0]), dec!(210000), "3,00,000 less 60,000 moved and 30,000 repaid");
        assert!(lt.tables[0].rows.iter().any(|r| r.label == "Total unsecured"));
        // PPE: opening, additions, disposals, depreciation, closing.
        let ppe = note(&s, "Property, Plant and Equipment");
        let plant = &ppe.tables[0].rows[0];
        assert_eq!(plant.cells.iter().map(amount).collect::<Vec<_>>(), [dec!(600000), dec!(100000), dec!(20000), dec!(70000), dec!(610000)]);
        // Cash on hand and balances with banks.
        let cash = note(&s, "Cash and cash equivalents");
        assert!(cash.tables[0].rows.iter().any(|r| r.label == "Cash on hand") && cash.tables[0].rows.iter().any(|r| r.label == "Balances with banks"));
        // Depreciation is its own line, mapped.
        assert!(s.profit_and_loss.rows.iter().any(|r| r.item == Some(LineItem::DepreciationAndAmortisation) && r.amount == dec!(70000)));
    }

    #[test]
    fn ageing_buckets_follow_schedule_three() {
        let f = company();
        let s = div1(&f);
        let tr = note(&s, "Trade receivables");
        let ageing = &tr.tables[1];
        assert_eq!(ageing.columns, ["Not due", "Less than 6 months", "6 months - 1 year", "1-2 years", "2-3 years", "More than 3 years", "Total"]);
        let good = ageing.rows[0].cells.iter().map(amount).collect::<Vec<_>>();
        // S4 not due; S3 (due Feb 2027) under 6 months; S2 (May 2026, no due
        // date) and the ₹1 on account (Apr 2026) 6-12 months; S1 (due Oct
        // 2025) and the opening balance (aged from 1 Apr 2025) 1-2 years.
        assert_eq!(good, [dec!(60000), dec!(100000), dec!(300001), dec!(250000), dec!(0), dec!(0), dec!(710001)]);
        let tp = note(&s, "Trade payables");
        let a = &tp.tables[1];
        assert_eq!(a.columns, ["Not due", "Less than 1 year", "1-2 years", "2-3 years", "More than 3 years", "Total"]);
        let row = |label: &str| a.rows.iter().find(|r| r.label == label).unwrap().cells.iter().map(amount).collect::<Vec<_>>();
        assert_eq!(row("(i) MSME"), [dec!(0), dec!(90000), dec!(0), dec!(0), dec!(0), dec!(90000)]);
        assert_eq!(row("(ii) Others"), [dec!(0), dec!(0), dec!(80000), dec!(0), dec!(0), dec!(80000)]);
        assert_eq!(row("(iv) Disputed dues – Others"), [dec!(0), dec!(25000), dec!(0), dec!(0), dec!(0), dec!(25000)]);
        // The face shows the MSME split under trade payables.
        let details: Vec<&Row> = s.balance_sheet.rows.iter().filter(|r| r.kind == RowKind::Detail).collect();
        assert_eq!(details.iter().map(|r| r.amount).collect::<Vec<_>>(), [dec!(90000), dec!(105000)]);
        // CWIP: 60,000 under a year, 40,000 one to two years.
        let cwip = note(&s, "Capital work-in-progress");
        assert_eq!(cwip.tables[1].rows[0].cells.iter().map(amount).collect::<Vec<_>>(), [dec!(60000), dec!(40000), dec!(0), dec!(0), dec!(100000)]);
    }

    #[test]
    fn the_mapping_survives_replay() {
        let f = company();
        let replayed = Book::replay("Notes Pvt Ltd", f.book.audit().clone()).unwrap();
        assert_eq!(Mapping::of_book(&replayed), Mapping::of_book(&f.book));
        assert_eq!(Mapping::of_book(&replayed).targets.len(), 5);
        let again = F { book: replayed, n: 0, l: BTreeMap::new() };
        assert_eq!(div1(&again), div1(&f));
    }

    #[test]
    fn division_two_totals_equal_division_one() {
        let f = company();
        let m = Mapping::of_book(&f.book);
        let (at, prev) = (d(2027, 3, 31), Some(d(2026, 3, 31)));
        let one = schedule3::balance_sheet(&f.book, at, prev, &m);
        let two = ind_as_balance_sheet(&f.book, at, prev, &m);
        assert_eq!(two.total_assets, one.total_assets);
        assert_eq!(two.total_equity_and_liabilities, one.total_equity_and_liabilities);
        assert_eq!(two.total_assets.current, reports::balance_sheet(&f.book, at).total_assets);
        assert_eq!(two.rows[0].label, "ASSETS", "Division II starts with assets");
        let line = |i: IndAsLine| two.rows.iter().find(|r| r.item == Some(i)).unwrap_or_else(|| panic!("{i:?}"));
        assert_eq!(line(IndAsLine::COtherFinancialAssets).amount, dec!(15000), "deposits are financial assets");
        assert!(two.rows.iter().any(|r| r.label == "Financial liabilities"));
        let reserves = schedule3::balance_sheet(&f.book, at, prev, &m).rows.into_iter().find(|r| r.item == Some(LineItem::ReservesAndSurplus)).unwrap();
        assert_eq!(line(IndAsLine::OtherEquity).amount, reserves.amount);

        let period = (d(2026, 4, 1), at);
        let p1 = schedule3::profit_and_loss(&f.book, period.0, period.1, None, &m);
        let p2 = ind_as_profit_and_loss(&f.book, period.0, period.1, None, &m);
        assert_eq!(p2.total_comprehensive_income, p1.profit_for_the_period);
        assert_eq!(p1.profit_for_the_period.current, reports::profit_and_loss(&f.book, period.0, period.1).net_profit);
        assert_eq!(p2.other_comprehensive_income.current, dec!(5000));
        assert_eq!(p2.profit_for_the_period.current, p1.profit_for_the_period.current - dec!(5000));
        let oci = p2.rows.iter().find(|r| r.item == Some(IndAsLine::OciNotReclassified)).unwrap();
        assert_eq!(oci.amount, dec!(5000));
        let empty = p2.rows.iter().find(|r| r.item == Some(IndAsLine::OciReclassified)).unwrap();
        assert!(empty.amount.is_zero() && empty.note.is_none(), "OCI part B shown, empty, without a note");

        let s2 = statements::<IndAsLine>(&f.book, at, prev, &m);
        assert!(s2.notes.iter().any(|n| n.title == "Other Equity"));
        let tr = s2.notes.iter().find(|n| n.title == "Trade receivables (current)").unwrap();
        assert_eq!(tr.tables[1].rows.len(), 7, "six Division II categories and the total");
    }

    #[test]
    fn rounding_follows_total_income() {
        assert!(Rounding::Thousands.allowed(dec!(50000000)));
        assert!(!Rounding::Crores.allowed(dec!(50000000)));
        assert!(Rounding::Crores.allowed(dec!(1000000000)) && !Rounding::Hundreds.allowed(dec!(1000000000)));
        assert!(!Rounding::Rupees.allowed(dec!(1)));
        assert_eq!(Rounding::Lakhs.round(dec!(1234567)), dec!(12.35));
        assert_eq!(Rounding::Crores.unit(), "₹ in crores");

        let mut f = company();
        f.book.set_schedule3_format(now(), "u", Some(2), Some("lakhs")).unwrap();
        let Either::Division2(s) = company_statements(&f.book, d(2027, 3, 31), Some(d(2026, 3, 31))) else { panic!("division 2") };
        assert_eq!(s.unit, "₹ in lakhs");
        assert_eq!(s.balance_sheet.unit, "₹ in lakhs");
        assert!(s.warnings.iter().all(|w| !w.starts_with("Rounding")));
        f.book.set_schedule3_format(now(), "u", None, None).unwrap();
        let Either::Division1(s) = company_statements(&f.book, d(2027, 3, 31), None) else { panic!("division 1") };
        assert!(s.warnings.iter().any(|w| w.starts_with("Rounding")), "rupees are not a Schedule III unit");
    }

    #[test]
    fn codes_parse_and_check_their_line() {
        assert!(parse_code("trade_payables:msme,disputed").is_ok());
        assert_eq!(parse_code("trade_payables:msme").unwrap().code(), "trade_payables:msme");
        assert!(parse_code("trade_receivables:msme").is_err());
        assert!(parse_code("long_term_borrowings:current_maturities").is_err());
        assert!(parse_code("changes_in_inventories").is_err());
        assert!(parse_code("borrowings_current:secured").is_ok());
        assert!(parse_code("nonsense").is_err());
        let f = company();
        assert!(schedule3::validate(&f.book, g("Sundry Debtors"), "trade_payables").is_err(), "wrong side");
        assert!(schedule3::validate(&f.book, g("Indirect Expenses"), "oci_reclassified").is_ok(), "OCI takes income or expenses");
        let opts = schedule3::line_options(Nature::Liabilities);
        assert!(opts.iter().any(|o| o.code == "short_term_borrowings:current_maturities"));
        assert!(opts.iter().any(|o| o.code == "lease_liabilities_current" && o.division == 2));
        assert!(!opts.iter().any(|o| o.code == "trade_receivables"));
    }

    #[test]
    fn ratios_come_from_the_statements() {
        let f = company();
        let s = div1(&f);
        let r = |name: &str| s.ratios.iter().find(|r| r.name == name).unwrap();
        assert_eq!(s.ratios.len(), 11);
        let (ca, cl) = schedule3::current_assets_and_liabilities();
        let sum = |items: &[LineItem]| line_figures(&s.balance_sheet.rows, items).current;
        assert_eq!(r("Current ratio").current, div(sum(&ca), sum(&cl)));
        assert!(r("Return on investment").current.is_none() && r("Return on investment").remark.is_some());
        let np = r("Net profit ratio");
        assert_eq!(np.current, div(s.profit_and_loss.profit_for_the_period.current * dec!(100), dec!(460000)));
        assert!(r("Debt service coverage ratio").current.is_some(), "principal was repaid");
    }
}

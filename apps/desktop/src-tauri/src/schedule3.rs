//! Schedule III (Companies Act 2013) Balance Sheet, Statement of Profit
//! and Loss and notes to accounts, in the company's chosen division
//! (Division I, or Division II for Ind AS) and rounding unit, plus the
//! company's mapping of groups to line items. The statements and notes are
//! `khata_in::schedule3` and `khata_in::schedule3_notes`; printing is
//! khata-print; the mapping, MSME parties, division and rounding are stored
//! in the company profile (so they sync and are in the audit trail). See
//! `docs/india-schedule-3.md`.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use khata_core::format::Grouping;
use khata_core::{Book, GroupId, LedgerId, Nature, UserRole};
use khata_in::schedule3::{
    self, AnyBalanceSheet, AnyProfitAndLoss, BalanceSheet, Either, IndAsLine, LineItem, LineOption, Mapping, ProfitAndLoss,
    Rounding, Row, RowKind,
};
use khata_in::schedule3_notes::{self, AnyStatements, Cell, Note, Statements};
use khata_print::{Header, Schedule3Cell, Schedule3Note, Schedule3NoteRow, Schedule3NoteTable, Schedule3Row, Schedule3RowKind};
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, now, open_mut, open_ref, os_user, parse_date, parse_uuid, require};

/// An optional date: empty or missing means none.
fn optional(s: Option<&str>) -> ApiResult<Option<NaiveDate>> {
    match s.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse_date(s).map(Some),
        None => Ok(None),
    }
}

fn period(from: &str, to: &str) -> ApiResult<(NaiveDate, NaiveDate)> {
    let (f, t) = (parse_date(from)?, parse_date(to)?);
    if f > t {
        return Err("the period ends before it starts".into());
    }
    Ok((f, t))
}

/// The comparative period: both dates or neither.
fn previous_period(from: Option<&str>, to: Option<&str>) -> ApiResult<Option<(NaiveDate, NaiveDate)>> {
    match (optional(from)?, optional(to)?) {
        (Some(f), Some(t)) if f <= t => Ok(Some((f, t))),
        (Some(_), Some(_)) => Err("the previous period ends before it starts".into()),
        (None, None) => Ok(None),
        _ => Err("give both dates of the previous period, or neither".into()),
    }
}

fn rounded_bs<K>(mut bs: BalanceSheet<K>, r: Rounding) -> BalanceSheet<K> {
    bs.round(r);
    bs
}

/// Balance Sheet as at `as_at`, with `previous_as_at` alongside (none for a
/// first year), in the company's division and rounding unit.
pub fn balance_sheet(slot: &Slot, as_at: &str, previous_as_at: Option<&str>) -> ApiResult<AnyBalanceSheet> {
    let book = open_ref(slot)?;
    let (d, prev) = (parse_date(as_at)?, optional(previous_as_at)?);
    let (m, r) = (Mapping::of_book(book), Rounding::of_book(book));
    Ok(match schedule3::division_of(book) {
        2 => Either::Division2(rounded_bs(schedule3::ind_as_balance_sheet(book, d, prev, &m), r)),
        _ => Either::Division1(rounded_bs(schedule3::balance_sheet(book, d, prev, &m), r)),
    })
}

/// Number a statement of profit and loss's notes after the Balance Sheet's
/// as at its end date, as the notes are.
fn after_balance_sheet<K: schedule3::Layout>(book: &Book, pl: &mut ProfitAndLoss<K>, m: &Mapping) {
    let bs = K::balance_sheet(book, pl.to, pl.previous_to, m);
    pl.renumber(bs.rows.iter().filter_map(|r| r.note).max().unwrap_or(0) + 1);
}

fn finish_pl<K: schedule3::Layout>(book: &Book, mut pl: ProfitAndLoss<K>, m: &Mapping, r: Rounding) -> ProfitAndLoss<K> {
    after_balance_sheet(book, &mut pl, m);
    if let Some(w) = r.warning(pl.total_income.current) {
        pl.warnings.push(w);
    }
    pl.round(r);
    pl
}

/// Statement of Profit and Loss for `[from, to]`, with the previous period
/// alongside when both of its dates are given.
pub fn profit_and_loss(
    slot: &Slot,
    from: &str,
    to: &str,
    prev_from: Option<&str>,
    prev_to: Option<&str>,
) -> ApiResult<AnyProfitAndLoss> {
    let book = open_ref(slot)?;
    let (f, t) = period(from, to)?;
    let prev = previous_period(prev_from, prev_to)?;
    let (m, r) = (Mapping::of_book(book), Rounding::of_book(book));
    Ok(match schedule3::division_of(book) {
        2 => Either::Division2(finish_pl(book, schedule3::ind_as_profit_and_loss(book, f, t, prev, &m), &m, r)),
        _ => Either::Division1(finish_pl(book, schedule3::profit_and_loss(book, f, t, prev, &m), &m, r)),
    })
}

/// The statements with all their notes and the ratios, as at `as_at` (the
/// year is the financial year to that date).
pub fn statements(slot: &Slot, as_at: &str, previous_as_at: Option<&str>) -> ApiResult<AnyStatements> {
    let book = open_ref(slot)?;
    let (d, prev) = (parse_date(as_at)?, optional(previous_as_at)?);
    if prev.is_some_and(|p| p >= d) {
        return Err("the previous date must be before the current one".into());
    }
    Ok(schedule3_notes::company_statements(book, d, prev))
}

// ------------------------------------------------------------------ print

fn print_rows<K>(rows: &[Row<K>]) -> Vec<Schedule3Row> {
    rows.iter()
        .map(|r| Schedule3Row {
            kind: match r.kind {
                RowKind::Heading => Schedule3RowKind::Heading,
                RowKind::Line | RowKind::Detail => Schedule3RowKind::Line,
                RowKind::Total => Schedule3RowKind::Total,
            },
            level: r.level,
            label: r.label.clone(),
            note: r.note,
            current: r.amount,
            previous: r.previous,
        })
        .collect()
}

fn header(book: &Book) -> Header<'_> {
    Header {
        company: &book.name,
        address: book.profile().map_or("", |p| p.address.as_str()),
        grouping: Grouping::Indian,
    }
}

fn date(d: NaiveDate) -> String {
    d.format("%-d-%b-%Y").to_string()
}

fn bs_html<K>(book: &Book, bs: &BalanceSheet<K>) -> String {
    let current = format!("As at {}", date(bs.as_at));
    let previous = bs.previous_as_at.map(|p| format!("As at {}", date(p)));
    khata_print::schedule3_statement_html(
        &header(book),
        "Balance Sheet",
        &format!("as at {}", date(bs.as_at)),
        (&current, previous.as_deref()),
        &bs.unit,
        &print_rows(&bs.rows),
    )
}

fn pl_html<K>(book: &Book, pl: &ProfitAndLoss<K>) -> String {
    let label = |(f, t): (NaiveDate, NaiveDate)| format!("{} to {}", date(f), date(t));
    let current = label((pl.from, pl.to));
    let prev = pl.previous_from.zip(pl.previous_to).map(label);
    khata_print::schedule3_statement_html(
        &header(book),
        "Statement of Profit and Loss",
        &format!("for the period {current}"),
        (&current, prev.as_deref()),
        &pl.unit,
        &print_rows(&pl.rows),
    )
}

pub fn balance_sheet_html(slot: &Slot, as_at: &str, previous_as_at: Option<&str>) -> ApiResult<String> {
    let bs = balance_sheet(slot, as_at, previous_as_at)?;
    let book = open_ref(slot)?;
    Ok(match &bs {
        Either::Division1(b) => bs_html(book, b),
        Either::Division2(b) => bs_html(book, b),
    })
}

pub fn profit_and_loss_html(
    slot: &Slot,
    from: &str,
    to: &str,
    prev_from: Option<&str>,
    prev_to: Option<&str>,
) -> ApiResult<String> {
    let pl = profit_and_loss(slot, from, to, prev_from, prev_to)?;
    let book = open_ref(slot)?;
    Ok(match &pl {
        Either::Division1(p) => pl_html(book, p),
        Either::Division2(p) => pl_html(book, p),
    })
}

fn amount_text(a: rust_decimal::Decimal, suffix: &str) -> String {
    format!("{}{suffix}", khata_core::format::format_amount(a, Grouping::Indian, 2))
}

fn print_notes(notes: &[Note]) -> Vec<Schedule3Note> {
    notes
        .iter()
        .map(|n| Schedule3Note {
            number: n.number,
            title: n.title.clone(),
            tables: n
                .tables
                .iter()
                .map(|t| Schedule3NoteTable {
                    caption: t.caption.clone(),
                    columns: t.columns.clone(),
                    rows: t
                        .rows
                        .iter()
                        .map(|r| Schedule3NoteRow {
                            kind: match r.kind {
                                RowKind::Heading => Schedule3RowKind::Heading,
                                RowKind::Total => Schedule3RowKind::Total,
                                RowKind::Line | RowKind::Detail => Schedule3RowKind::Line,
                            },
                            level: r.level,
                            label: r.label.clone(),
                            cells: r
                                .cells
                                .iter()
                                .map(|c| match c {
                                    Cell::Empty => Schedule3Cell::Empty,
                                    Cell::Amount(a) => Schedule3Cell::Amount(*a),
                                    Cell::Ratio(a) => Schedule3Cell::Text(amount_text(*a, " times")),
                                    Cell::Percent(a) => Schedule3Cell::Text(amount_text(*a, "%")),
                                    Cell::Text(s) => Schedule3Cell::Text(s.clone()),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            remarks: n.remarks.clone(),
        })
        .collect()
}

fn notes_doc<K>(book: &Book, s: &Statements<K>) -> String {
    khata_print::schedule3_notes_html(&header(book), &format!("for the year ended {}", date(s.as_at)), &s.unit, &print_notes(&s.notes))
}

/// The notes to accounts, ready to print.
pub fn notes_html(slot: &Slot, as_at: &str, previous_as_at: Option<&str>) -> ApiResult<String> {
    let s = statements(slot, as_at, previous_as_at)?;
    let book = open_ref(slot)?;
    Ok(match &s {
        Either::Division1(s) => notes_doc(book, s),
        Either::Division2(s) => notes_doc(book, s),
    })
}

// ---------------------------------------------------------------- mapping

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupMapDto {
    pub group: GroupId,
    pub name: String,
    /// 0 for primary groups.
    pub depth: u8,
    pub nature: Nature,
    /// The company's own code for this group, if any.
    pub code: Option<String>,
    /// Where its ledgers go in Division I (with classifications).
    pub line: String,
    /// Where they go in Division II.
    pub line2: String,
    /// The group whose mapping applies, when it is not this one.
    pub inherited_from: Option<String>,
    /// No mapping reaches the group: its nature decides.
    pub fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PartyDto {
    pub ledger: LedgerId,
    pub name: String,
    pub group_name: String,
    pub msme: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoundingDto {
    pub code: String,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MappingDto {
    pub division: u8,
    pub rounding: String,
    pub unit: String,
    pub roundings: Vec<RoundingDto>,
    /// Every group, in tree order.
    pub groups: Vec<GroupMapDto>,
    /// Parties shown under trade payables, and any marked MSME.
    pub parties: Vec<PartyDto>,
    /// Problems with the stored mapping.
    pub warnings: Vec<String>,
}

fn tags_text(p: &schedule3::Placement) -> String {
    let tags: Vec<&str> = p.tags.iter().map(schedule3::Tag::label).collect();
    if tags.is_empty() { String::new() } else { format!(": {}", tags.join(", ")) }
}

/// The company's mapping, group by group, with where each group ends up.
pub fn mapping(slot: &Slot) -> ApiResult<MappingDto> {
    let book = open_ref(slot)?;
    let m = Mapping::of_book(book);
    let stored = book.profile().map(|p| p.schedule3_map.clone()).unwrap_or_default();
    let placements = schedule3::placements(book, &m);
    let mut children: BTreeMap<Option<GroupId>, Vec<&khata_core::Group>> = BTreeMap::new();
    for g in book.groups() {
        children.entry(g.parent).or_default().push(g);
    }
    for v in children.values_mut() {
        v.sort_by_key(|g| g.name.to_lowercase());
    }
    let mut groups = Vec::new();
    let mut stack: Vec<(&khata_core::Group, u8)> = children.get(&None).into_iter().flatten().rev().map(|g| (*g, 0)).collect();
    while let Some((g, depth)) = stack.pop() {
        let nature = book.primary_group(g.id).map_or(g.nature, |p| p.nature);
        let p = placements.get(&g.id).copied();
        groups.push(GroupMapDto {
            group: g.id,
            name: g.name.clone(),
            depth,
            nature,
            code: stored.get(&g.id).cloned(),
            line: p.map_or_else(String::new, |p| format!("{}{}", p.item.label(), tags_text(&p))),
            line2: p.map_or_else(String::new, |p| p.ind_as.long_label()),
            inherited_from: p.filter(|p| p.group != g.id && !p.fallback).and_then(|p| book.group(p.group)).map(|x| x.name.clone()),
            fallback: p.is_some_and(|p| p.fallback),
        });
        for c in children.get(&Some(g.id)).into_iter().flatten().rev() {
            stack.push((c, depth.saturating_add(1)));
        }
    }
    let mut parties: Vec<PartyDto> = book
        .ledgers()
        .filter(|l| {
            let p = placements.get(&l.group);
            m.msme_parties.contains(&l.id)
                || p.is_some_and(|p| {
                    p.item == LineItem::TradePayables || matches!(p.ind_as, IndAsLine::CTradePayables | IndAsLine::NcTradePayables)
                })
        })
        .map(|l| PartyDto {
            ledger: l.id,
            name: l.name.clone(),
            group_name: book.group(l.group).map(|g| g.name.clone()).unwrap_or_default(),
            msme: m.msme_parties.contains(&l.id) || placements.get(&l.group).is_some_and(|p| p.tags.has(schedule3::Tag::Msme)),
        })
        .collect();
    parties.sort_by_key(|p| p.name.to_lowercase());
    let mut warnings: Vec<String> = m.unreadable.iter().map(|(_, code, why)| format!("{code:?}: {why}")).collect();
    warnings.extend(stored.keys().filter(|g| book.group(**g).is_none()).map(|_| "A mapping names a group that no longer exists.".to_owned()));
    let rounding = Rounding::of_book(book);
    Ok(MappingDto {
        division: schedule3::division_of(book),
        rounding: rounding.code(),
        unit: rounding.unit().to_owned(),
        roundings: Rounding::ALL.iter().map(|r| RoundingDto { code: r.code(), unit: r.unit().to_owned() }).collect(),
        groups,
        parties,
        warnings,
    })
}

/// The lines `group` can map to.
pub fn line_options(slot: &Slot, group: &str) -> ApiResult<Vec<LineOption>> {
    let book = open_ref(slot)?;
    let id = GroupId(parse_uuid(group, "group")?);
    let g = book.group(id).ok_or("unknown group")?;
    let nature = book.primary_group(g.id).map_or(g.nature, |p| p.nature);
    Ok(schedule3::line_options(nature))
}

/// Map `group` to the line `code` (validated by the India pack); empty or
/// missing removes the company's own mapping, so the default applies.
pub fn set_mapping(slot: &mut Slot, group: &str, code: Option<&str>) -> ApiResult<MappingDto> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    let id = GroupId(parse_uuid(group, "group")?);
    let code = code.map(str::trim).filter(|c| !c.is_empty());
    let code = match code {
        Some(c) => Some(schedule3::validate(book, id, c)?.code()),
        None => None,
    };
    book.set_schedule3_mapping(now(), &os_user(), id, code.as_deref()).map_err(err)?;
    store.save(book).map_err(err)?;
    mapping(slot)
}

/// Mark a party as a micro or small enterprise, or clear the mark.
pub fn set_msme(slot: &mut Slot, ledger: &str, msme: bool) -> ApiResult<MappingDto> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    let id = LedgerId(parse_uuid(ledger, "ledger")?);
    book.set_msme_party(now(), &os_user(), id, msme).map_err(err)?;
    store.save(book).map_err(err)?;
    mapping(slot)
}

/// Choose the division (1 or 2) and the rounding unit code ("lakhs").
pub fn set_format(slot: &mut Slot, division: u8, rounding: &str) -> ApiResult<MappingDto> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    if ![1, 2].contains(&division) {
        return Err("Schedule III has Division I (Accounting Standards) and Division II (Ind AS)".into());
    }
    let r = Rounding::from_code(rounding.trim()).ok_or_else(|| format!("{rounding:?} is not a rounding unit"))?;
    // Division I and rupees are the defaults: store nothing for them.
    let division = (division == 2).then_some(2);
    let rounding = (r != Rounding::Rupees).then(|| r.code());
    book.set_schedule3_format(now(), &os_user(), division, rounding.as_deref()).map_err(err)?;
    store.save(book).map_err(err)?;
    mapping(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, demo};
    use crate::api::{balance_sheet as core_bs, profit_and_loss as core_pl};
    use rust_decimal_macros::dec;

    fn one<A, B>(e: Either<A, B>) -> A {
        match e {
            Either::Division1(a) => a,
            Either::Division2(_) => panic!("expected Division I"),
        }
    }

    #[test]
    fn statements_tie_to_the_core_reports_and_print() {
        let f = TempFile::new("schedule3");
        let (slot, ..) = demo(&f);
        let bs = one(balance_sheet(&slot, "2027-03-31", Some("2026-03-31")).unwrap());
        let core = core_bs(&slot, "2027-03-31").unwrap();
        assert_eq!(bs.total_assets.current, core.total_assets);
        assert_eq!(bs.total_equity_and_liabilities, bs.total_assets);
        assert_eq!(bs.total_assets.previous, dec!(250000));
        assert_eq!(bs.unit, "₹");

        let pl = one(profit_and_loss(&slot, "2026-04-01", "2027-03-31", Some(""), None).unwrap());
        assert_eq!(pl.previous_from, None);
        let core = core_pl(&slot, "2026-04-01", "2027-03-31").unwrap();
        assert_eq!(pl.profit_for_the_period.current, core.net_profit);
        assert_eq!(pl.profit_for_the_period.current, dec!(15000));
        let first = pl.rows.iter().find_map(|r| r.note).unwrap();
        assert!(first > 1, "P&L notes follow the Balance Sheet's");

        let html = balance_sheet_html(&slot, "2027-03-31", Some("2026-03-31")).unwrap();
        assert!(html.contains("Demo Traders") && html.contains("As at 31-Mar-2026"));
        assert!(html.contains("Trade receivables") && html.contains("Note No.") && html.contains("(Figures in ₹)"));
        let html = profit_and_loss_html(&slot, "2026-04-01", "2027-03-31", Some("2025-04-01"), Some("2026-03-31")).unwrap();
        assert!(html.contains("Statement of Profit and Loss") && html.contains("Revenue from operations"));

        assert!(profit_and_loss(&slot, "2026-04-01", "2027-03-31", Some("2025-04-01"), None).is_err());
        assert!(profit_and_loss(&slot, "2027-04-01", "2027-03-31", None, None).is_err());
    }

    #[test]
    fn mapping_division_and_rounding_are_saved_and_used() {
        let f = TempFile::new("schedule3-map");
        let (mut slot, ..) = demo(&f);
        crate::api::set_country(&mut slot, None, "IN").unwrap();
        let m = mapping(&slot).unwrap();
        let debtors = m.groups.iter().find(|g| g.name == "Sundry Debtors").unwrap();
        assert_eq!((debtors.line.as_str(), debtors.code.as_ref()), ("Trade receivables", None));
        let id = debtors.group.0.to_string();
        assert!(line_options(&slot, &id).unwrap().iter().any(|o| o.code == "trade_receivables:disputed"));
        assert!(set_mapping(&mut slot, &id, Some("trade_payables")).is_err(), "an asset group cannot be a payable");
        let m = set_mapping(&mut slot, &id, Some("trade_receivables:doubtful")).unwrap();
        let debtors = m.groups.iter().find(|g| g.name == "Sundry Debtors").unwrap();
        assert_eq!(debtors.code.as_deref(), Some("trade_receivables:doubtful"));
        assert_eq!(debtors.line, "Trade receivables: considered doubtful");

        let m = set_format(&mut slot, 2, "thousands").unwrap();
        assert_eq!((m.division, m.unit.as_str()), (2, "₹ in thousands"));
        assert!(set_format(&mut slot, 3, "lakhs").is_err() && set_format(&mut slot, 1, "dozens").is_err());
        let Either::Division2(bs) = balance_sheet(&slot, "2027-03-31", None).unwrap() else { panic!("division 2") };
        assert_eq!(bs.unit, "₹ in thousands");
        let core = core_bs(&slot, "2027-03-31").unwrap().total_assets;
        assert_eq!(bs.total_assets.current, Rounding::Thousands.round(core));
        assert_eq!(Rounding::Thousands.round(dec!(250000)), dec!(250));
        let Either::Division2(s) = statements(&slot, "2027-03-31", Some("2026-03-31")).unwrap() else { panic!("division 2") };
        assert!(s.notes.iter().any(|n| n.title == "Trade receivables (current)"));
        let html = notes_html(&slot, "2027-03-31", Some("2026-03-31")).unwrap();
        assert!(html.contains("Notes to the financial statements") && html.contains("(Figures in ₹ in thousands)"));
        assert!(html.contains("Ratios") && html.contains("credit impaired"), "Division II wording");

        // Stored in the books: a reopened company keeps it.
        let book = Book::replay("Demo Traders", crate::api::open_ref(&slot).unwrap().audit().clone()).unwrap();
        assert_eq!(Mapping::of_book(&book), Mapping::of_book(crate::api::open_ref(&slot).unwrap()));
        let m = set_mapping(&mut slot, &id, None).unwrap();
        assert_eq!(m.groups.iter().find(|g| g.name == "Sundry Debtors").unwrap().code, None);
    }
}

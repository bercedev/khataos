//! Getting data in and out: Tally XML import, printable HTML and CSV export.

use std::path::Path;

use khata_core::format::Grouping;
use khata_core::reports;
use khata_core::{Amount, Book, LedgerId, TrialBalanceRow};
use khata_print::{Header, PrintLanguage};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, err, now, open_mut, open_ref, os_user, parse_date, parse_uuid};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkippedItem {
    /// "Group", "Ledger", "Voucher", "Unit", "Stock group", "Stock item",
    /// "Godown", or "Inventory" (stock detail of something that was
    /// imported, e.g. a voucher posted without its stock lines).
    pub kind: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportSummary {
    pub files: Vec<String>,
    pub groups_created: usize,
    pub groups_matched: usize,
    pub ledgers_created: usize,
    pub opening_balances: usize,
    pub vouchers_posted: usize,
    pub vouchers_numbered: usize,
    pub units_created: usize,
    pub stock_groups_created: usize,
    pub stock_items_created: usize,
    pub godowns_created: usize,
    /// Stock item lines posted on vouchers.
    pub inventory_lines: usize,
    pub skipped: Vec<SkippedItem>,
    /// Vouchers posted as cancelled, keeping their numbers.
    pub vouchers_cancelled: usize,
    /// Vouchers by Tally voucher type, as named in the files.
    pub by_type: Vec<ImportTypeCount>,
    pub bills: ImportBills,
    pub gst: ImportGst,
    /// First and last date of the vouchers posted (ISO).
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    /// Zero when the opening balances agree.
    pub opening_difference: String,
    pub balances: ImportBalances,
    pub notes: Vec<ImportNote>,
    /// Notes that mean balances may not match Tally's.
    pub balance_warnings: usize,
    /// The whole report, to save as text or CSV.
    pub report_text: String,
    pub report_csv: String,
    /// Counts only: no names, amounts, dates or file names.
    pub shareable_text: String,
    pub shareable_csv: String,
    /// The automatic backup taken before importing into a company that
    /// already had books (src/backup.rs).
    pub backup: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportTypeCount {
    /// The Tally voucher type.
    pub name: String,
    /// The KhataOS type it was posted as ("Sales"); `None` when not imported.
    pub kind: Option<String>,
    pub posted: usize,
    pub cancelled: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportBills {
    pub new_ref: usize,
    pub against: usize,
    pub advance: usize,
    pub on_account: usize,
    pub opening: usize,
    pub opening_ledgers: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportGst {
    pub gstins: usize,
    pub states: usize,
    pub pans: usize,
    pub addresses: usize,
    pub duty_ledgers: usize,
    pub ledger_classes: usize,
    pub rates: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportBalances {
    pub compared: usize,
    pub matched: usize,
    pub differences: Vec<ImportBalanceDifference>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportBalanceDifference {
    pub name: String,
    /// Debit positive.
    pub tally: String,
    pub khataos: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportNote {
    /// Stable code: the importer's `NoteKind` name ("DayBookStartsLate").
    pub kind: String,
    /// It means balances may not match Tally's.
    pub warning: bool,
    pub name: String,
    pub detail: String,
    pub count: usize,
}

fn kind_label(kind: khata_import_tally::ItemKind) -> &'static str {
    use khata_import_tally::ItemKind as K;
    match kind {
        K::Group => "Group",
        K::Ledger => "Ledger",
        K::Voucher => "Voucher",
        K::Unit => "Unit",
        K::StockGroup => "Stock group",
        K::StockItem => "Stock item",
        K::Godown => "Godown",
        K::Inventory => "Inventory",
    }
}

/// Import one or more Tally XML exports (masters and Day Book together)
/// into the open company, then save.
pub fn import_tally(slot: &mut Slot, paths: &[String]) -> ApiResult<ImportSummary> {
    let (store, book) = open_mut(slot)?;
    if paths.is_empty() {
        return Err("choose at least one Tally XML file".into());
    }
    // Each file is parsed and its bytes dropped before the next is read, so
    // a large Day Book isn't held twice. A file that isn't Tally XML aborts
    // here, before the books change.
    let mut data = khata_import_tally::TallyData::default();
    for p in paths {
        let bytes = std::fs::read(p).map_err(|e| format!("cannot read {p}: {e}"))?;
        data.extend(khata_import_tally::parse(&bytes).map_err(err)?);
    }
    // Should saving fail, the books go back to how they were, so the open
    // company never holds an import that isn't on disk.
    let before = book.clone();
    let report = khata_import_tally::import_data(data, book, now(), &os_user());
    if let Err(e) = store.save(book) {
        *book = before;
        return Err(err(e));
    }
    drop(before);
    let files: Vec<String> = paths
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .map_or(p.clone(), |f| f.to_string_lossy().into_owned())
        })
        .collect();
    let money = |a: Amount| format!("{:.2}", a);
    Ok(ImportSummary {
        report_text: report.to_text(&files, false),
        report_csv: report.to_csv(&files, false),
        shareable_text: report.to_text(&files, true),
        shareable_csv: report.to_csv(&files, true),
        files,
        groups_created: report.groups_created,
        groups_matched: report.groups_matched,
        ledgers_created: report.ledgers_created,
        opening_balances: report.opening_balances,
        vouchers_posted: report.vouchers_posted,
        vouchers_numbered: report.vouchers_numbered,
        units_created: report.units_created,
        stock_groups_created: report.stock_groups_created,
        stock_items_created: report.stock_items_created,
        godowns_created: report.godowns_created,
        inventory_lines: report.inventory_lines,
        vouchers_cancelled: report.vouchers_cancelled,
        by_type: report
            .by_type
            .iter()
            .map(|(name, t)| ImportTypeCount {
                name: name.clone(),
                kind: t.kind.map(|k| khata_import_tally::voucher_kind_label(k).to_owned()),
                posted: t.posted,
                cancelled: t.cancelled,
                skipped: t.skipped,
            })
            .collect(),
        bills: ImportBills {
            new_ref: report.bills.new_ref,
            against: report.bills.against,
            advance: report.bills.advance,
            on_account: report.bills.on_account,
            opening: report.bills.opening,
            opening_ledgers: report.bills.opening_ledgers,
        },
        gst: ImportGst {
            gstins: report.gst.gstins,
            states: report.gst.states,
            pans: report.gst.pans,
            addresses: report.gst.addresses,
            duty_ledgers: report.gst.duty_ledgers,
            ledger_classes: report.gst.ledger_classes,
            rates: report.gst.rates,
        },
        first_date: report.first_date.map(|d| d.to_string()),
        last_date: report.last_date.map(|d| d.to_string()),
        opening_difference: money(report.opening_difference),
        balances: ImportBalances {
            compared: report.balances.compared,
            matched: report.balances.matched,
            differences: report
                .balances
                .differences
                .iter()
                .map(|d| ImportBalanceDifference {
                    name: d.name.clone(),
                    tally: money(d.tally),
                    khataos: money(d.khataos),
                })
                .collect(),
        },
        notes: report
            .notes
            .iter()
            .map(|n| ImportNote {
                kind: format!("{:?}", n.kind),
                warning: n.kind.affects_balances(),
                name: n.name.clone(),
                detail: n.detail.clone(),
                count: n.count,
            })
            .collect(),
        balance_warnings: report.balance_warnings(),
        skipped: report
            .skipped
            .iter()
            .map(|s| SkippedItem {
                kind: kind_label(s.kind).to_owned(),
                name: s.name.clone(),
                reason: s.reason.to_string(),
            })
            .collect(),
        backup: None,
    })
}

/// A table the frontend already shows, for reports without a dedicated
/// printer (outstanding, ageing, group summary…).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Table {
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    pub columns: Vec<String>,
    /// Indexes of right-aligned (amount) columns.
    #[serde(default)]
    pub numeric: Vec<usize>,
    pub rows: Vec<Vec<String>>,
}

/// What to print or export.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "report", rename_all = "snake_case")]
pub enum ReportSpec {
    TrialBalance {
        as_at: String,
    },
    DayBook {
        from: String,
        to: String,
    },
    LedgerStatement {
        ledger_id: String,
        from: String,
        to: String,
    },
    ProfitAndLoss {
        from: String,
        to: String,
    },
    BalanceSheet {
        as_at: String,
    },
    Table(Table),
}

/// What `print_report` receives: the report and, beside its fields, the
/// language to print in (`"language": "ar"` prints right to left in
/// Arabic). The frontend passes its display language; English when absent.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PrintSpec {
    #[serde(flatten)]
    pub report: ReportSpec,
    #[serde(default)]
    pub language: Option<String>,
}

fn trial_rows(book: &Book, as_at: chrono::NaiveDate) -> Vec<TrialBalanceRow> {
    let mut rows: Vec<TrialBalanceRow> = book
        .balances(Some(as_at))
        .into_iter()
        .filter(|(_, b)| !b.is_zero())
        .filter_map(|(id, b)| {
            Some(TrialBalanceRow {
                ledger: id,
                name: book.ledger(id)?.name.clone(),
                debit: b.max(Amount::ZERO),
                credit: (-b).max(Amount::ZERO),
            })
        })
        .collect();
    rows.sort_by_key(|r| r.name.to_lowercase());
    rows
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const TABLE_CSS: &str = "@page{size:A4;margin:14mm 12mm}body{font:10.5pt/1.35 system-ui,-apple-system,'Segoe UI','Noto Sans',sans-serif;color:#111;margin:0}\
header{text-align:center;margin-bottom:10px}h1{font-size:14pt;margin:0}h2{font-size:11.5pt;margin:8px 0 0}.period{font-size:9.5pt;color:#333}\
table{width:100%;border-collapse:collapse}th,td{padding:3px 6px;border-bottom:.5pt solid #bbb;text-align:left;vertical-align:top}\
th{border-bottom:1pt solid #111;font-size:9pt}.num{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}thead{display:table-header-group}tr{break-inside:avoid}";

fn table_html(company: &str, t: &Table) -> String {
    let cell = |i: usize, tag: &str, v: &str| {
        let cls = if t.numeric.contains(&i) {
            " class=\"num\""
        } else {
            ""
        };
        format!("<{tag}{cls}>{}</{tag}>", esc(v))
    };
    let head: String = t
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| cell(i, "th", c))
        .collect();
    let body: String = t
        .rows
        .iter()
        .map(|r| {
            format!(
                "<tr>{}</tr>",
                r.iter()
                    .enumerate()
                    .map(|(i, v)| cell(i, "td", v))
                    .collect::<String>()
            )
        })
        .collect();
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>{TABLE_CSS}</style></head><body>\
         <header><h1>{company}</h1><h2>{title}</h2><div class=\"period\">{sub}</div></header>\
         <table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></body></html>",
        title = esc(&t.title),
        company = esc(company),
        sub = esc(&t.subtitle),
    )
}

/// Printable HTML for a report (khata-print for the core reports), in English.
pub fn report_html(slot: &Slot, spec: &ReportSpec) -> ApiResult<String> {
    report_html_in(slot, spec, PrintLanguage::English)
}

/// Printable HTML for a report in `lang`: Arabic prints right to left with
/// Arabic labels (khata-print `*_html_ar`); figures stay Latin.
pub fn report_html_in(slot: &Slot, spec: &ReportSpec, lang: PrintLanguage) -> ApiResult<String> {
    let ar = lang == PrintLanguage::Arabic;
    let book = open_ref(slot)?;
    let address = book.profile().map(|p| p.address.clone()).unwrap_or_default();
    let grouping = if book.country().is_none_or(|c| c == "IN") { Grouping::Indian } else { Grouping::Western };
    let h = Header {
        company: &book.name,
        address: &address,
        grouping,
    };
    Ok(match spec {
        ReportSpec::TrialBalance { as_at } => {
            let d = parse_date(as_at)?;
            let (rows, stock) = (trial_rows(book, d), khata_core::inventory::opening_stock_value(book));
            if ar { khata_print::trial_balance_html_ar(&h, d, &rows, stock) } else { khata_print::trial_balance_html(&h, d, &rows, stock) }
        }
        ReportSpec::DayBook { from, to } => {
            let (f, t) = (parse_date(from)?, parse_date(to)?);
            let rows = reports::day_book(book, f, t);
            if ar { khata_print::day_book_html_ar(&h, f, t, &rows) } else { khata_print::day_book_html(&h, f, t, &rows) }
        }
        ReportSpec::LedgerStatement {
            ledger_id,
            from,
            to,
        } => {
            let id = LedgerId(parse_uuid(ledger_id, "ledger")?);
            let (f, t) = (parse_date(from)?, parse_date(to)?);
            let s = reports::ledger_statement(book, id, f, t).ok_or("unknown ledger")?;
            let name = book.ledger(id).map(|l| l.name.clone()).unwrap_or_default();
            if ar { khata_print::ledger_statement_html_ar(&h, &name, f, t, &s) } else { khata_print::ledger_statement_html(&h, &name, f, t, &s) }
        }
        ReportSpec::ProfitAndLoss { from, to } => {
            let p = reports::profit_and_loss(book, parse_date(from)?, parse_date(to)?);
            if ar { khata_print::profit_and_loss_html_ar(&h, &p) } else { khata_print::profit_and_loss_html(&h, &p) }
        }
        ReportSpec::BalanceSheet { as_at } => {
            let b = reports::balance_sheet(book, parse_date(as_at)?);
            if ar { khata_print::balance_sheet_html_ar(&h, &b) } else { khata_print::balance_sheet_html(&h, &b) }
        }
        ReportSpec::Table(t) if ar => khata_print::table_html_ar(&book.name, &t.title, &t.subtitle, &t.columns, &t.numeric, &t.rows),
        ReportSpec::Table(t) => table_html(&book.name, t),
    })
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// RFC 4180 CSV. Amount columns lose their digit grouping so spreadsheets
/// read them as numbers ("2,50,000.00 Dr" → "250000.00 Dr").
pub fn table_csv(t: &Table) -> String {
    let mut out = String::new();
    let line = |cells: Vec<String>| cells.join(",") + "\r\n";
    out.push_str(&line(t.columns.iter().map(|c| csv_field(c)).collect()));
    for r in &t.rows {
        out.push_str(&line(
            r.iter()
                .enumerate()
                .map(|(i, v)| {
                    if t.numeric.contains(&i) {
                        csv_field(&v.replace(',', ""))
                    } else {
                        csv_field(v)
                    }
                })
                .collect(),
        ));
    }
    out
}

pub fn report_csv(slot: &Slot, spec: &ReportSpec) -> ApiResult<String> {
    match spec {
        ReportSpec::TrialBalance { as_at } => {
            let book = open_ref(slot)?;
            Ok(khata_print::trial_balance_csv(
                &trial_rows(book, parse_date(as_at)?),
                khata_core::inventory::opening_stock_value(book),
            ))
        }
        ReportSpec::Table(t) => Ok(table_csv(t)),
        _ => Err("export this report as a table".into()),
    }
}

/// Save a Tally import report chosen in the save dialog: `.csv` or `.txt`.
pub fn save_import_report(path: &str, contents: &str) -> ApiResult<()> {
    let p = Path::new(path);
    let ok = p
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv") || e.eq_ignore_ascii_case("txt"));
    if !ok {
        return Err("save the report as a .csv or .txt file".into());
    }
    std::fs::write(p, contents).map_err(|e| format!("cannot write {path}: {e}"))
}

/// Write an exported CSV chosen in the save dialog. Only `.csv` files.
pub fn write_csv(path: &str, contents: &str) -> ApiResult<()> {
    let p = Path::new(path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")) {
        return Err("exports must be saved as .csv files".into());
    }
    std::fs::write(p, contents).map_err(|e| format!("cannot write {path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, demo};
    use crate::api::{create_company, list_ledgers, trial_balance};

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../crates/khata-import-tally/tests/fixtures"
    );

    #[test]
    fn imports_tally_masters_and_vouchers_and_saves() {
        let f = TempFile::new("import");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Shah & Sons").unwrap();
        let files = [
            format!("{FIXTURES}/masters.xml"),
            format!("{FIXTURES}/vouchers.xml"),
        ];
        let r = import_tally(&mut slot, &files).unwrap();
        assert_eq!(r.files, ["masters.xml", "vouchers.xml"]);
        assert_eq!(r.ledgers_created, 10);
        assert!(r.vouchers_posted > 0);
        assert!(r.skipped.iter().all(|s| !s.reason.is_empty()));
        let tb = trial_balance(&slot, None).unwrap();
        drop(slot);
        let mut reopened = None;
        crate::api::open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(
            trial_balance(&reopened, None).unwrap(),
            tb,
            "the import was saved"
        );
        assert_eq!(list_ledgers(&reopened, "", "all").unwrap().len(), 10);
    }

    #[test]
    fn shareable_import_summary_has_counts_only() {
        let f = TempFile::new("import-share");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Shah & Sons").unwrap();
        let files = [
            format!("{FIXTURES}/masters.xml"),
            format!("{FIXTURES}/vouchers.xml"),
        ];
        let r = import_tally(&mut slot, &files).unwrap();
        // Names and amounts the fixture puts in skip reasons and notes.
        let private = ["Lost Ledger", "Electricity", "299", "Receipt 1 on", "masters.xml", "2024"];
        for full in [&r.report_text, &r.report_csv] {
            for p in private {
                assert!(full.contains(p), "{p:?} missing from {full}");
            }
        }
        for shared in [&r.shareable_text, &r.shareable_csv] {
            for private in private.iter().chain(&["Ramesh", "Suresh", "HDFC", "Shah", "11800", "INV-"]) {
                assert!(!shared.contains(private), "{private:?} in {shared}");
            }
            assert!(shared.contains("Ledgers created"));
        }
        assert!(r.report_csv.starts_with("Section,Item,Count,Name,Detail"));
        assert!(r.by_type.iter().any(|t| t.name == "Sales GST" && t.kind.as_deref() == Some("Sales")));
    }

    #[test]
    fn saves_import_reports_only_as_csv_or_text() {
        let dir = std::env::temp_dir();
        let txt = dir.join(format!("khataos-import-{}.txt", std::process::id()));
        save_import_report(&txt.display().to_string(), "x").unwrap();
        assert_eq!(std::fs::read_to_string(&txt).unwrap(), "x");
        let _ = std::fs::remove_file(txt);
        let html = dir.join("khataos-import.html").display().to_string();
        assert!(save_import_report(&html, "x").is_err());
    }

    #[test]
    fn imports_tally_inventory_and_saves() {
        let f = TempFile::new("import-stock");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Kiran Hardware").unwrap();
        let files = [
            format!("{FIXTURES}/stock_masters.xml"),
            format!("{FIXTURES}/stock_vouchers.xml"),
        ];
        let r = import_tally(&mut slot, &files).unwrap();
        assert_eq!(
            (
                r.units_created,
                r.stock_groups_created,
                r.stock_items_created,
                r.godowns_created,
                r.inventory_lines
            ),
            (5, 4, 4, 2, 8)
        );
        let kinds: Vec<&str> = r.skipped.iter().map(|s| s.kind.as_str()).collect();
        for k in ["Unit", "Stock group", "Stock item", "Inventory", "Voucher"] {
            assert!(kinds.contains(&k), "{k} in {kinds:?}");
        }
        let tb = trial_balance(&slot, None).unwrap();
        drop(slot);
        let mut reopened = None;
        crate::api::open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(
            trial_balance(&reopened, None).unwrap(),
            tb,
            "the import was saved"
        );
    }

    #[test]
    fn import_rejects_bad_files_without_changing_the_books() {
        let f = TempFile::new("import-bad");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let junk = std::env::temp_dir().join(format!("khataos-junk-{}.xml", std::process::id()));
        std::fs::write(&junk, "<HTML>nope</HTML>").unwrap();
        assert!(
            import_tally(&mut slot, &[junk.display().to_string()])
                .unwrap_err()
                .contains("not a Tally XML")
        );
        assert!(
            import_tally(&mut slot, &["/no/such/file.xml".into()])
                .unwrap_err()
                .contains("cannot read")
        );
        assert!(import_tally(&mut slot, &[]).is_err());
        assert_eq!(list_ledgers(&slot, "", "all").unwrap().len(), 0);
        let _ = std::fs::remove_file(junk);
    }

    #[test]
    fn prints_core_reports_and_tables() {
        let f = TempFile::new("print");
        let (slot, bank, ..) = demo(&f);
        let tb = report_html(
            &slot,
            &ReportSpec::TrialBalance {
                as_at: "2026-04-30".into(),
            },
        )
        .unwrap();
        assert!(tb.contains("Demo Traders") && tb.contains("2,90,000.00"));
        let st = report_html(
            &slot,
            &ReportSpec::LedgerStatement {
                ledger_id: bank,
                from: "2026-04-01".into(),
                to: "2026-04-30".into(),
            },
        )
        .unwrap();
        assert!(st.contains("HDFC Bank"));
        for spec in [
            ReportSpec::DayBook {
                from: "2026-04-01".into(),
                to: "2026-04-30".into(),
            },
            ReportSpec::ProfitAndLoss {
                from: "2026-04-01".into(),
                to: "2026-04-30".into(),
            },
            ReportSpec::BalanceSheet {
                as_at: "2026-04-30".into(),
            },
        ] {
            assert!(
                report_html(&slot, &spec)
                    .unwrap()
                    .starts_with("<!doctype html>")
            );
        }
        let t = Table {
            title: "Outstanding <Receivables>".into(),
            subtitle: "As at 30-Apr-2026".into(),
            columns: vec!["Party".into(), "Amount".into()],
            numeric: vec![1],
            rows: vec![vec!["Sharma & Co".into(), "40,000.00 Dr".into()]],
        };
        let html = report_html(&slot, &ReportSpec::Table(t)).unwrap();
        assert!(
            html.contains("Sharma &amp; Co")
                && html.contains("&lt;Receivables&gt;")
                && html.contains("class=\"num\"")
        );
    }

    #[test]
    fn csv_exports() {
        let f = TempFile::new("csv");
        let (slot, ..) = demo(&f);
        let tb = report_csv(
            &slot,
            &ReportSpec::TrialBalance {
                as_at: "2026-04-30".into(),
            },
        )
        .unwrap();
        assert!(tb.starts_with("Particulars,Debit,Credit\r\n"));
        assert!(tb.contains("HDFC Bank,225000,0"));
        let t = Table {
            title: "x".into(),
            subtitle: String::new(),
            columns: vec!["Party".into(), "Amount".into()],
            numeric: vec![1],
            rows: vec![vec!["Shah, \"Sons\"".into(), "2,50,000.00 Dr".into()]],
        };
        assert_eq!(
            table_csv(&t),
            "Party,Amount\r\n\"Shah, \"\"Sons\"\"\",250000.00 Dr\r\n"
        );
        assert!(
            report_csv(
                &slot,
                &ReportSpec::BalanceSheet {
                    as_at: "2026-04-30".into()
                }
            )
            .is_err()
        );
        let out = std::env::temp_dir().join(format!("khataos-export-{}.csv", std::process::id()));
        write_csv(&out.display().to_string(), &tb).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), tb);
        assert!(write_csv("/tmp/evil.sh", "x").is_err());
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn a_print_spec_carries_the_language_beside_the_report() {
        let spec: PrintSpec = serde_json::from_str(r#"{"report":"trial_balance","as_at":"2026-04-30","language":"ar"}"#).unwrap();
        assert_eq!(spec.report, ReportSpec::TrialBalance { as_at: "2026-04-30".into() });
        assert_eq!(spec.language.as_deref(), Some("ar"));
        let spec: PrintSpec = serde_json::from_str(r#"{"report":"day_book","from":"2026-04-01","to":"2026-04-30"}"#).unwrap();
        assert_eq!(spec.language, None, "English when the frontend leaves it out");
        let spec: PrintSpec = serde_json::from_str(
            r#"{"report":"table","title":"الذمم","subtitle":"","columns":["الطرف","المبلغ"],"numeric":[1],"rows":[["Sharma","1.00"]],"language":"ar"}"#,
        )
        .unwrap();
        assert!(matches!(&spec.report, ReportSpec::Table(t) if t.columns.len() == 2 && t.rows[0][0] == "Sharma"));
        assert_eq!(spec.language.as_deref(), Some("ar"));
        // The CSV command still takes a bare spec; a language beside it is ignored.
        let bare: ReportSpec = serde_json::from_str(r#"{"report":"balance_sheet","as_at":"2026-04-30","language":"ar"}"#).unwrap();
        assert_eq!(bare, ReportSpec::BalanceSheet { as_at: "2026-04-30".into() });
    }

    #[test]
    fn prints_core_reports_and_tables_in_arabic() {
        let f = TempFile::new("print-ar");
        let (slot, bank, ..) = demo(&f);
        let ar = PrintLanguage::Arabic;
        let specs = [
            (ReportSpec::TrialBalance { as_at: "2026-04-30".into() }, "ميزان المراجعة"),
            (ReportSpec::DayBook { from: "2026-04-01".into(), to: "2026-04-30".into() }, "دفتر اليومية"),
            (ReportSpec::LedgerStatement { ledger_id: bank, from: "2026-04-01".into(), to: "2026-04-30".into() }, "الحساب: <bdi>HDFC Bank</bdi>"),
            (ReportSpec::ProfitAndLoss { from: "2026-04-01".into(), to: "2026-04-30".into() }, "قائمة الأرباح والخسائر"),
            (ReportSpec::BalanceSheet { as_at: "2026-04-30".into() }, "الميزانية العمومية"),
        ];
        for (spec, title) in specs {
            let html = report_html_in(&slot, &spec, ar).unwrap();
            assert!(html.starts_with("<!doctype html><html lang=\"ar\" dir=\"rtl\">"), "{spec:?}");
            assert!(html.contains(title), "{spec:?} has {title}");
            assert!(html.contains("<bdi>Demo Traders</bdi>"));
            let en = report_html_in(&slot, &spec, PrintLanguage::English).unwrap();
            assert_eq!(en, report_html(&slot, &spec).unwrap(), "English is the default");
            assert!(en.contains("<html lang=\"en\">"));
        }
        let tb = report_html_in(&slot, &ReportSpec::TrialBalance { as_at: "2026-04-30".into() }, ar).unwrap();
        assert!(tb.contains("2,90,000.00"), "Indian grouping and Latin digits stay");
        let t = Table {
            title: "الذمم المدينة".into(),
            subtitle: "كما في 30-أبريل-2026".into(),
            columns: vec!["الطرف".into(), "المبلغ".into()],
            numeric: vec![1],
            rows: vec![vec!["Sharma & Co".into(), "40,000.00 مدين".into()]],
        };
        let html = report_html_in(&slot, &ReportSpec::Table(t), ar).unwrap();
        assert!(html.contains("dir=\"rtl\"") && html.contains("<h2>الذمم المدينة</h2>"));
        assert!(html.contains("<td><bdi>Sharma &amp; Co</bdi></td><td class=\"num\">40,000.00 مدين</td>"));
    }

    /// The Arabic table for messages from Rust (`src/locales/ar-errors.ts`)
    /// is keyed by the English text. Every key, and every fixed piece of a
    /// pattern, must still be written somewhere in the Rust sources: reword a
    /// message here and this test asks for the table to follow.
    #[test]
    fn the_arabic_message_table_matches_rust() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let table = std::fs::read_to_string(root.join("apps/desktop/src/locales/ar-errors.ts")).unwrap();
        let mut rust = String::new();
        let mut dirs = vec![root.join("apps/desktop/src-tauri/src")];
        dirs.extend(std::fs::read_dir(root.join("crates")).unwrap().flatten().map(|e| e.path().join("src")));
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    rust.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
                }
            }
        }
        // A JS string literal at the start of `s` (after `"` or `'`), unescaped.
        let literal = |s: &str| -> Option<String> {
            let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let mut out = String::new();
            let mut it = s[1..].chars();
            while let Some(c) = it.next() {
                match c {
                    '\\' => out.push(it.next()?),
                    c if c == q => return Some(out),
                    c => out.push(c),
                }
            }
            None
        };
        let (mut exact, mut patterns) = (0, 0);
        let mut section = "";
        for line in table.lines() {
            let l = line.trim_start();
            if l.starts_with("export const EXACT") {
                section = "exact";
            } else if l.starts_with("export const PATTERNS") {
                section = "patterns";
            } else if l.starts_with("};") || l.starts_with("];") {
                section = "";
            } else if section == "exact" {
                let Some(en) = literal(l) else { continue };
                assert!(rust.contains(&en), "ar-errors.ts: {en:?} is no longer a message in the Rust code");
                exact += 1;
            } else if section == "patterns" {
                let Some(en) = l.strip_prefix('[').and_then(literal) else { continue };
                for piece in en.split(['{', '}']).step_by(2) {
                    let piece = piece.trim_matches(|c: char| c == '"' || c.is_whitespace());
                    if piece.chars().count() >= 3 {
                        assert!(rust.contains(piece), "ar-errors.ts pattern {en:?}: {piece:?} is not in the Rust code");
                    }
                }
                patterns += 1;
            }
        }
        assert!(exact >= 60 && patterns >= 30, "{exact} messages, {patterns} patterns read");
    }
}

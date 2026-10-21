//! GSTR-9, the annual GST return (CGST s.44, Rule 80), worked out from the
//! year's documents, with the GSTR-9C turnover reconciliation (Part A,
//! Tables 5 and 7) as groundwork.
//!
//! The sources are in `docs/research/india-gstr9.md`. In short:
//! - Part II (Tables 4 and 5) comes from the outward documents, classified
//!   as GSTR-1 classifies them (rates are settled by
//!   [`crate::returns`]'s resolver), and from reverse-charge purchases (4G);
//! - Part III (Tables 6 to 8): ITC by kind (inputs, capital goods, input
//!   services) and supplier (registered, unregistered, abroad), blocked
//!   credit (7E) and Rule 37 reversals (7A, supplied by the caller); Table
//!   8A is the portal's (GSTR-2B), so it is entered or imported;
//! - Part IV (Table 9): tax payable from 4N; what was paid in cash or by
//!   credit is entered;
//! - Parts V and VI (Tables 10 to 16 and 19) are entered: the books cannot
//!   know amendments made next year, demands, refunds or late fees;
//! - Tables 17 and 18: HSN summaries of outward and inward supplies,
//!   grouped by HSN, UQC and rate, net of credit notes.
//!
//! Any row that is not a total may be entered by hand ([`Entries`]); an
//! entry replaces the books' figure for that row, and the totals follow.
//! [`gstr9_json`] writes the figures in the shape of the GSTN GSTR-9 JSON as
//! far as public sources show it: it is NOT validated and must be checked
//! in the GST offline tool. [`parse_portal`] reads files downloaded from the
//! portal (the GSTR-9 JSON with system-computed values, GSTR-1, GSTR-3B and
//! GSTR-2B JSON) and [`reconcile`] compares them with the books, row by row.
//! None of this is legal advice.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::gstr2b;
use crate::returns::{self, Amounts, DocKind, InwardDoc, OutwardDoc, Period, num};

// ------------------------------------------------------------------ model

/// Where a row's figure comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Worked out from the books (it may still be entered by hand).
    Books,
    /// The books cannot know it: entered by hand, else blank.
    Manual,
    /// Only the GST portal knows it (Table 8A): imported or entered.
    Portal,
    /// Worked out from other rows; never entered.
    Total,
}

/// The column set of a table. Each column key is also the field name in
/// the GSTN JSON (see [`JSON_ROWS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cols {
    /// Taxable value and the four taxes.
    Tax,
    /// Taxable value only.
    Value,
    /// The four taxes (ITC).
    Itc,
    /// Table 9: payable, paid in cash, paid by each credit.
    Paid,
    /// Tables 14 and 19: payable and paid.
    PayablePaid,
    /// Table 15: the four taxes, interest, penalty, late fee and others.
    Demand,
}

impl Cols {
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            Cols::Tax => &["txval", "iamt", "camt", "samt", "csamt"],
            Cols::Value => &["txval"],
            Cols::Itc => &["iamt", "camt", "samt", "csamt"],
            Cols::Paid => &["txpyble", "txpaid_cash", "tax_paid_itc_iamt", "tax_paid_itc_camt", "tax_paid_itc_samt", "tax_paid_itc_csamt"],
            Cols::PayablePaid => &["txpyble", "txpaid"],
            Cols::Demand => &["iamt", "camt", "samt", "csamt", "intr", "pen", "fee"],
        }
    }

    /// Column titles, in English (the app translates them).
    pub fn labels(self) -> &'static [&'static str] {
        match self {
            Cols::Tax => &["Taxable Value", "IGST", "CGST", "SGST/UTGST", "Cess"],
            Cols::Value => &["Taxable Value"],
            Cols::Itc => &["IGST", "CGST", "SGST/UTGST", "Cess"],
            Cols::Paid => &["Tax payable", "Paid in cash", "Paid by ITC: IGST", "Paid by ITC: CGST", "Paid by ITC: SGST/UTGST", "Paid by ITC: Cess"],
            Cols::PayablePaid => &["Payable", "Paid"],
            Cols::Demand => &["IGST", "CGST", "SGST/UTGST", "Cess", "Interest", "Penalty", "Late fee and others"],
        }
    }

    fn of(self, a: &Amounts) -> Vec<Decimal> {
        match self {
            Cols::Tax => vec![a.taxable, a.igst, a.cgst, a.sgst, a.cess],
            Cols::Value => vec![a.taxable],
            Cols::Itc => vec![a.igst, a.cgst, a.sgst, a.cess],
            _ => vec![Decimal::ZERO; self.keys().len()],
        }
    }
}

/// One row of the form, as defined (not yet worked out).
#[derive(Debug, Clone, Copy)]
pub struct RowDef {
    /// "4A", "6B-IP" (inputs), "9-CAMT" (central tax), "9C-5B".
    pub id: &'static str,
    pub table: &'static str,
    pub label: &'static str,
    pub source: Source,
    /// Totals: the rows added (+1) or taken away (−1), by column key.
    pub formula: &'static [(&'static str, i8)],
    /// Why the books cannot fill it, or how they do.
    pub note: &'static str,
}

const fn row(id: &'static str, table: &'static str, label: &'static str, source: Source) -> RowDef {
    RowDef { id, table, label, source, formula: &[], note: "" }
}

const fn noted(id: &'static str, table: &'static str, label: &'static str, source: Source, note: &'static str) -> RowDef {
    RowDef { id, table, label, source, formula: &[], note }
}

const fn total(id: &'static str, table: &'static str, label: &'static str, formula: &'static [(&'static str, i8)]) -> RowDef {
    RowDef { id, table, label, source: Source::Total, formula, note: "" }
}

use Source::{Books, Manual, Portal};

/// A table of the form.
#[derive(Debug, Clone, Copy)]
pub struct TableDef {
    pub form: &'static str,
    pub id: &'static str,
    pub part: &'static str,
    pub title: &'static str,
    pub cols: Cols,
}

pub const TABLES: &[TableDef] = &[
    TableDef { form: "GSTR-9", id: "4", part: "II", title: "Outward supplies, advances and inward supplies on which tax is payable", cols: Cols::Tax },
    TableDef { form: "GSTR-9", id: "5", part: "II", title: "Outward supplies on which tax is not payable", cols: Cols::Value },
    TableDef { form: "GSTR-9", id: "6", part: "III", title: "ITC availed during the year", cols: Cols::Itc },
    TableDef { form: "GSTR-9", id: "7", part: "III", title: "ITC reversed and ineligible", cols: Cols::Itc },
    TableDef { form: "GSTR-9", id: "8", part: "III", title: "Other ITC information", cols: Cols::Itc },
    TableDef { form: "GSTR-9", id: "9", part: "IV", title: "Tax paid as declared in returns", cols: Cols::Paid },
    TableDef { form: "GSTR-9", id: "10", part: "V", title: "Supplies and tax declared through amendments (+), net of debit notes", cols: Cols::Tax },
    TableDef { form: "GSTR-9", id: "11", part: "V", title: "Supplies and tax reduced through amendments (−), net of credit notes", cols: Cols::Tax },
    TableDef { form: "GSTR-9", id: "12", part: "V", title: "Reversal of ITC availed during the previous year", cols: Cols::Itc },
    TableDef { form: "GSTR-9", id: "13", part: "V", title: "ITC availed for the previous year", cols: Cols::Itc },
    TableDef { form: "GSTR-9", id: "14", part: "V", title: "Differential tax paid on account of Tables 10 and 11", cols: Cols::PayablePaid },
    TableDef { form: "GSTR-9", id: "15", part: "VI", title: "Demands and refunds", cols: Cols::Demand },
    TableDef { form: "GSTR-9", id: "16", part: "VI", title: "Supplies from composition taxpayers, deemed supply and goods sent on approval", cols: Cols::Tax },
    TableDef { form: "GSTR-9", id: "19", part: "VI", title: "Late fee payable and paid", cols: Cols::PayablePaid },
    TableDef { form: "GSTR-9C", id: "9C-5", part: "II", title: "GSTR-9C: reconciliation of turnover", cols: Cols::Value },
    TableDef { form: "GSTR-9C", id: "9C-7", part: "II", title: "GSTR-9C: reconciliation of taxable turnover", cols: Cols::Value },
];

const NOT_IN_BOOKS: &str = "Not in the books: enter it";

/// Every row of GSTR-9, in the order of the form. Rows only in some years
/// carry a year range in [`row_years`].
pub const ROWS: &[RowDef] = &[
    // Table 4
    noted("4A", "4", "Supplies to unregistered persons (B2C)", Books, "Net of credit notes, as the form asks"),
    row("4B", "4", "Supplies to registered persons (B2B)", Books),
    row("4C", "4", "Exports on payment of tax (other than to SEZs)", Books),
    row("4D", "4", "Supplies to SEZs on payment of tax", Books),
    noted("4E", "4", "Deemed exports", Manual, "The books do not mark deemed exports"),
    noted("4F", "4", "Advances on which tax was paid but no invoice was issued", Manual, NOT_IN_BOOKS),
    row("4G", "4", "Inward supplies on which tax is paid on reverse charge", Books),
    total("4H", "4", "Subtotal (A to G)", &[("4A", 1), ("4B", 1), ("4C", 1), ("4D", 1), ("4E", 1), ("4F", 1), ("4G", 1)]),
    row("4I", "4", "Credit notes for B to E (−)", Books),
    row("4J", "4", "Debit notes for B to E (+)", Books),
    noted("4K", "4", "Supplies and tax declared through amendments (+)", Manual, NOT_IN_BOOKS),
    noted("4L", "4", "Supplies and tax reduced through amendments (−)", Manual, NOT_IN_BOOKS),
    total("4M", "4", "Subtotal (I to L)", &[("4I", -1), ("4J", 1), ("4K", 1), ("4L", -1)]),
    total("4N", "4", "Supplies and advances on which tax is payable (H + M)", &[("4H", 1), ("4M", 1)]),
    // Table 5
    row("5A", "5", "Exports without payment of tax", Books),
    row("5B", "5", "Supplies to SEZs without payment of tax", Books),
    noted("5C", "5", "Supplies on which the recipient pays tax on reverse charge", Manual, "The books do not mark outward supplies on reverse charge"),
    noted("5D", "5", "Exempted supplies", Manual, "The books do not tell nil rated, exempt and non-GST supplies apart: every 0% line is in 5E"),
    noted("5E", "5", "Nil rated supplies", Books, "Every 0% line in the books; move exempt and non-GST supplies to 5D and 5F"),
    noted("5F", "5", "Non-GST supplies (including no supply)", Manual, "The books do not tell nil rated, exempt and non-GST supplies apart: every 0% line is in 5E"),
    total("5G", "5", "Subtotal (A to F)", &[("5A", 1), ("5B", 1), ("5C", 1), ("5D", 1), ("5E", 1), ("5F", 1)]),
    row("5H", "5", "Credit notes for A to F (−)", Books),
    row("5I", "5", "Debit notes for A to F (+)", Books),
    noted("5J", "5", "Supplies declared through amendments (+)", Manual, NOT_IN_BOOKS),
    noted("5K", "5", "Supplies reduced through amendments (−)", Manual, NOT_IN_BOOKS),
    total("5L", "5", "Subtotal (H to K)", &[("5H", -1), ("5I", 1), ("5J", 1), ("5K", -1)]),
    total("5M", "5", "Turnover on which tax is not payable (G + L)", &[("5G", 1), ("5L", 1)]),
    total("5N", "5", "Total turnover, including advances (4N + 5M − 4G)", &[("4N", 1), ("5M", 1), ("4G", -1)]),
    // Table 6
    noted("6A", "6", "Total ITC availed through GSTR-3B (Table 4A)", Books, "The books' own figure; the portal fills it from the GSTR-3B returns filed"),
    noted("6A1", "6", "ITC of the previous year availed in this year", Manual, NOT_IN_BOOKS),
    noted("6A2", "6", "Net ITC of this year (A − A1)", Source::Total, ""),
    row("6B-IP", "6", "Other inward supplies: inputs", Books),
    row("6B-CG", "6", "Other inward supplies: capital goods", Books),
    row("6B-IS", "6", "Other inward supplies: input services", Books),
    row("6C-IP", "6", "Reverse charge, unregistered suppliers: inputs", Books),
    row("6C-CG", "6", "Reverse charge, unregistered suppliers: capital goods", Books),
    row("6C-IS", "6", "Reverse charge, unregistered suppliers: input services", Books),
    row("6D-IP", "6", "Reverse charge, registered suppliers: inputs", Books),
    row("6D-CG", "6", "Reverse charge, registered suppliers: capital goods", Books),
    row("6D-IS", "6", "Reverse charge, registered suppliers: input services", Books),
    noted("6E-IP", "6", "Import of goods (including from SEZs): inputs", Books, "Purchases from suppliers abroad that carry IGST; bills of entry are not in the books"),
    noted("6E-CG", "6", "Import of goods (including from SEZs): capital goods", Books, "Purchases from suppliers abroad that carry IGST; bills of entry are not in the books"),
    noted("6F", "6", "Import of services (other than from SEZs)", Books, "Reverse-charge purchases from suppliers abroad"),
    noted("6G", "6", "ITC received from an Input Service Distributor", Manual, NOT_IN_BOOKS),
    noted("6H", "6", "ITC reclaimed (other than B above)", Manual, NOT_IN_BOOKS),
    total("6I", "6", "Subtotal (B to H)", &[
        ("6B-IP", 1), ("6B-CG", 1), ("6B-IS", 1), ("6C-IP", 1), ("6C-CG", 1), ("6C-IS", 1), ("6D-IP", 1), ("6D-CG", 1), ("6D-IS", 1),
        ("6E-IP", 1), ("6E-CG", 1), ("6F", 1), ("6G", 1), ("6H", 1),
    ]),
    total("6J", "6", "Difference (I − A)", &[("6I", 1), ("6A", -1)]),
    noted("6K", "6", "Transition credit through TRAN-1", Manual, NOT_IN_BOOKS),
    noted("6L", "6", "Transition credit through TRAN-2", Manual, NOT_IN_BOOKS),
    noted("6M", "6", "Any other ITC availed", Manual, NOT_IN_BOOKS),
    total("6N", "6", "Subtotal (K to M)", &[("6K", 1), ("6L", 1), ("6M", 1)]),
    total("6O", "6", "Total ITC availed (I + N)", &[("6I", 1), ("6N", 1)]),
    // Table 7
    noted("7A", "7", "Rule 37 (supplier not paid within 180 days)", Books, "From the Rule 37 report: reversals falling due in the year"),
    noted("7A1", "7", "Rule 37 (supplier not paid within 180 days)", Books, "From the Rule 37 report: reversals falling due in the year"),
    noted("7A2", "7", "Rule 37A (supplier did not file GSTR-3B)", Manual, NOT_IN_BOOKS),
    noted("7B", "7", "Rule 39 (Input Service Distributor)", Manual, NOT_IN_BOOKS),
    noted("7C", "7", "Rule 42 (inputs and input services for exempt supplies)", Manual, "Work it out with the Rule 42 calculator and enter it"),
    noted("7D", "7", "Rule 43 (capital goods for exempt supplies)", Manual, NOT_IN_BOOKS),
    noted("7E", "7", "Section 17(5) (blocked credit)", Books, "Lines flagged BLOCKED"),
    noted("7F", "7", "Reversal of TRAN-1 credit", Manual, NOT_IN_BOOKS),
    noted("7G", "7", "Reversal of TRAN-2 credit", Manual, NOT_IN_BOOKS),
    noted("7H", "7", "Other reversals", Manual, NOT_IN_BOOKS),
    total("7I", "7", "Total ITC reversed (A to H)", &[("7A", 1), ("7A1", 1), ("7A2", 1), ("7B", 1), ("7C", 1), ("7D", 1), ("7E", 1), ("7F", 1), ("7G", 1), ("7H", 1)]),
    total("7J", "7", "Net ITC available for use (6O − 7I)", &[("6O", 1), ("7I", -1)]),
    // Table 8
    noted("8A", "8", "ITC as per GSTR-2B", Portal, "Only the portal knows it: import the GSTR-9 or GSTR-2B files, or enter it"),
    total("8B", "8", "ITC as per 6B and 6H", &[("6B-IP", 1), ("6B-CG", 1), ("6B-IS", 1), ("6H", 1)]),
    noted("8C", "8", "ITC on inward supplies of this year availed in the next year", Manual, NOT_IN_BOOKS),
    total("8D", "8", "Difference A − (B + C)", &[("8A", 1), ("8B", -1), ("8C", -1)]),
    noted("8E", "8", "ITC available but not availed", Manual, NOT_IN_BOOKS),
    noted("8F", "8", "ITC available but ineligible", Manual, NOT_IN_BOOKS),
    noted("8G", "8", "IGST paid on import of goods (including from SEZs)", Manual, "Bills of entry are not in the books: import GSTR-2B or enter it"),
    total("8H", "8", "IGST credit availed on import of goods (6E)", &[("6E-IP", 1), ("6E-CG", 1)]),
    total("8I", "8", "Difference (G − H)", &[("8G", 1), ("8H", -1)]),
    total("8J", "8", "ITC available but not availed on import of goods (I)", &[("8I", 1)]),
    total("8K", "8", "Total ITC to lapse this year (E + F + J)", &[("8E", 1), ("8F", 1), ("8J", 1)]),
    // Table 9: payable from 4N; payments entered.
    noted("9-IAMT", "9", "Integrated Tax", Books, "Payable from 4N; enter what was paid"),
    noted("9-CAMT", "9", "Central Tax", Books, "Payable from 4N; enter what was paid"),
    noted("9-SAMT", "9", "State/UT Tax", Books, "Payable from 4N; enter what was paid"),
    noted("9-CSAMT", "9", "Cess", Books, "Payable from 4N; enter what was paid"),
    noted("9-INTR", "9", "Interest", Manual, NOT_IN_BOOKS),
    noted("9-FEE", "9", "Late fee", Manual, NOT_IN_BOOKS),
    noted("9-PEN", "9", "Penalty", Manual, NOT_IN_BOOKS),
    noted("9-OTH", "9", "Other", Manual, NOT_IN_BOOKS),
    // Part V
    noted("10", "10", "Supplies and tax declared through amendments (+), net of debit notes", Manual, NOT_IN_BOOKS),
    noted("11", "11", "Supplies and tax reduced through amendments (−), net of credit notes", Manual, NOT_IN_BOOKS),
    noted("12", "12", "Reversal of ITC availed during the previous year", Manual, NOT_IN_BOOKS),
    noted("13", "13", "ITC availed for the previous year", Manual, NOT_IN_BOOKS),
    noted("14-IAMT", "14", "Integrated Tax", Manual, NOT_IN_BOOKS),
    noted("14-CAMT", "14", "Central Tax", Manual, NOT_IN_BOOKS),
    noted("14-SAMT", "14", "State/UT Tax", Manual, NOT_IN_BOOKS),
    noted("14-CSAMT", "14", "Cess", Manual, NOT_IN_BOOKS),
    noted("14-INTR", "14", "Interest", Manual, NOT_IN_BOOKS),
    // Part VI
    noted("15A", "15", "Total refund claimed", Manual, NOT_IN_BOOKS),
    noted("15B", "15", "Total refund sanctioned", Manual, NOT_IN_BOOKS),
    noted("15C", "15", "Total refund rejected", Manual, NOT_IN_BOOKS),
    noted("15D", "15", "Total refund pending", Manual, NOT_IN_BOOKS),
    noted("15E", "15", "Total demand of taxes", Manual, NOT_IN_BOOKS),
    noted("15F", "15", "Total taxes paid against the demand in E", Manual, NOT_IN_BOOKS),
    noted("15G", "15", "Total demands pending out of E", Manual, NOT_IN_BOOKS),
    noted("16A", "16", "Supplies received from composition taxpayers", Manual, NOT_IN_BOOKS),
    noted("16B", "16", "Deemed supply under section 143 (job work)", Manual, NOT_IN_BOOKS),
    noted("16C", "16", "Goods sent on approval and not returned", Manual, NOT_IN_BOOKS),
    noted("19-CAMT", "19", "Central Tax", Manual, NOT_IN_BOOKS),
    noted("19-SAMT", "19", "State Tax", Manual, NOT_IN_BOOKS),
    // GSTR-9C Part A (groundwork): Table 5, reconciliation of gross turnover.
    noted("9C-5A", "9C-5", "Turnover (including exports) as per the audited financial statements", Books, "Sales accounts in the profit and loss account for the year"),
    noted("9C-5B", "9C-5", "Unbilled revenue at the beginning of the year (+)", Manual, NOT_IN_BOOKS),
    noted("9C-5C", "9C-5", "Unadjusted advances at the end of the year (+)", Manual, NOT_IN_BOOKS),
    noted("9C-5D", "9C-5", "Deemed supply under Schedule I (+)", Manual, NOT_IN_BOOKS),
    noted("9C-5E", "9C-5", "Credit notes issued after the year but reflected in the annual return (+)", Manual, NOT_IN_BOOKS),
    noted("9C-5F", "9C-5", "Trade discounts in the accounts but not permissible under GST (+)", Manual, NOT_IN_BOOKS),
    noted("9C-5H", "9C-5", "Unbilled revenue at the end of the year (−)", Manual, NOT_IN_BOOKS),
    noted("9C-5I", "9C-5", "Unadjusted advances at the beginning of the year (−)", Manual, NOT_IN_BOOKS),
    noted("9C-5J", "9C-5", "Credit notes in the accounts but not permissible under GST (−)", Manual, NOT_IN_BOOKS),
    noted("9C-5K", "9C-5", "Adjustments for supplies of goods by SEZ units to DTA units (−)", Manual, NOT_IN_BOOKS),
    noted("9C-5L", "9C-5", "Turnover for the period under the composition scheme (−)", Manual, NOT_IN_BOOKS),
    noted("9C-5M", "9C-5", "Adjustments in turnover under section 15 and its rules (+/−)", Manual, NOT_IN_BOOKS),
    noted("9C-5N", "9C-5", "Adjustments for foreign exchange differences (+/−)", Manual, NOT_IN_BOOKS),
    noted("9C-5O", "9C-5", "Adjustments for other reasons (+/−)", Manual, NOT_IN_BOOKS),
    total("9C-5P", "9C-5", "Annual turnover after adjustments", &[
        ("9C-5A", 1), ("9C-5B", 1), ("9C-5C", 1), ("9C-5D", 1), ("9C-5E", 1), ("9C-5F", 1), ("9C-5H", -1), ("9C-5I", -1), ("9C-5J", -1),
        ("9C-5K", -1), ("9C-5L", -1), ("9C-5M", 1), ("9C-5N", 1), ("9C-5O", 1),
    ]),
    total("9C-5Q", "9C-5", "Turnover as declared in GSTR-9 (5N)", &[("5N", 1)]),
    total("9C-5R", "9C-5", "Unreconciled turnover (Q − P)", &[("9C-5Q", 1), ("9C-5P", -1)]),
    // Table 7, reconciliation of taxable turnover.
    total("9C-7A", "9C-7", "Annual turnover after adjustments (5P)", &[("9C-5P", 1)]),
    total("9C-7B", "9C-7", "Exempted, nil rated, non-GST and no-supply turnover (5D, 5E, 5F)", &[("5D", 1), ("5E", 1), ("5F", 1)]),
    total("9C-7C", "9C-7", "Zero rated supplies without payment of tax (5A, 5B)", &[("5A", 1), ("5B", 1)]),
    total("9C-7D", "9C-7", "Supplies on reverse charge (5C)", &[("5C", 1)]),
    total("9C-7E", "9C-7", "Taxable turnover after adjustments (A − B − C − D)", &[("9C-7A", 1), ("9C-7B", -1), ("9C-7C", -1), ("9C-7D", -1)]),
    total("9C-7F", "9C-7", "Taxable turnover as declared in GSTR-9 (4N − 4G)", &[("4N", 1), ("4G", -1)]),
    total("9C-7G", "9C-7", "Unreconciled taxable turnover (F − E)", &[("9C-7F", 1), ("9C-7E", -1)]),
];

/// The financial years (by the year they start) a row is on the form,
/// both ends inclusive; `None`: every year. FY 2024-25 split 6A into 6A1
/// and 6A2 and 7A into 7A1 and 7A2 (Notif. 13/2025-CT; see the research
/// note for what is settled).
pub fn row_years(id: &str) -> (Option<i32>, Option<i32>) {
    match id {
        "6A1" | "6A2" | "7A1" | "7A2" => (Some(2024), None),
        "7A" => (None, Some(2023)),
        _ => (None, None),
    }
}

fn on_form(id: &str, fy_start: i32) -> bool {
    let (from, to) = row_years(id);
    from.is_none_or(|f| fy_start >= f) && to.is_none_or(|t| fy_start <= t)
}

/// The years' extra totals: 6A2 = 6A − 6A1.
fn formula_for(def: &RowDef) -> &'static [(&'static str, i8)] {
    match def.id {
        "6A2" => &[("6A", 1), ("6A1", -1)],
        _ => def.formula,
    }
}

fn table_def(id: &str) -> &'static TableDef {
    TABLES.iter().find(|t| t.id == id).expect("every row's table is defined")
}

/// Hand-entered figures: row id → column key → amount. Kept per company
/// and year by the app.
pub type Entries = BTreeMap<String, BTreeMap<String, Decimal>>;

/// A document behind a row, for drill-down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocRef {
    /// The app's voucher id.
    pub id: String,
    /// A purchase-side document.
    pub inward: bool,
    pub kind: DocKind,
    pub number: String,
    pub date: NaiveDate,
    pub party: String,
    /// What this document adds to the row (signed as the row counts it).
    pub amounts: Amounts,
}

/// One row as worked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub id: String,
    pub label: String,
    pub source: Source,
    /// The books' figure, by column (`None`: the books cannot know it).
    pub books: Option<Vec<Decimal>>,
    /// Entered by hand, by column.
    pub entered: Option<Vec<Decimal>>,
    /// What the return carries: entered, else the books', else zero.
    pub value: Vec<Decimal>,
    /// Documents behind the books' figure.
    pub docs: usize,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Table {
    pub form: String,
    pub id: String,
    pub part: String,
    pub title: String,
    pub cols: Cols,
    pub columns: Vec<String>,
    pub keys: Vec<String>,
    pub rows: Vec<Row>,
}

/// One HSN summary row (Tables 17 and 18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HsnLine {
    /// "hsn|uqc|rate", for drill-down.
    pub key: String,
    /// Empty when the lines had no HSN/SAC code.
    pub hsn: String,
    pub uqc: String,
    pub qty: Decimal,
    pub rate: Decimal,
    pub amounts: Amounts,
    pub docs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gstr9 {
    /// "2025-26".
    pub fy: String,
    /// The year the financial year starts in (2025 for 2025-26).
    pub fy_start: i32,
    pub period: Period,
    pub tables: Vec<Table>,
    /// Table 17.
    pub hsn_outward: Vec<HsnLine>,
    /// Table 18.
    pub hsn_inward: Vec<HsnLine>,
    /// HSN digits Tables 17 and 18 need (4 or 6).
    pub hsn_digits: u8,
    /// 5N: total turnover in the books (plus what was entered).
    pub turnover: Decimal,
    /// What applies for this year and turnover (optional tables, the
    /// ₹2 crore exemption, GSTR-9C).
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
    /// Row id or HSN key → the documents behind it.
    #[serde(skip)]
    pub docs: BTreeMap<String, Vec<DocRef>>,
}

impl Gstr9 {
    pub fn row(&self, id: &str) -> Option<&Row> {
        self.tables.iter().flat_map(|t| &t.rows).find(|r| r.id == id)
    }

    /// A row's figure for one column key (zero when absent).
    pub fn value(&self, id: &str, key: &str) -> Decimal {
        self.tables
            .iter()
            .find_map(|t| {
                let r = t.rows.iter().find(|r| r.id == id)?;
                let i = t.keys.iter().position(|k| k == key)?;
                r.value.get(i).copied()
            })
            .unwrap_or_default()
    }
}

/// What the app gives the builder, beyond the documents.
#[derive(Debug, Clone, Default)]
pub struct Gstr9Input<'a> {
    /// The financial year, 1 April to 31 March.
    pub period: Option<Period>,
    pub supplier_state: &'a str,
    pub outward: &'a [OutwardDoc],
    pub inward: &'a [InwardDoc],
    /// Outward documents to an SEZ unit or developer (by id).
    pub sez: BTreeSet<String>,
    /// Inward documents from suppliers outside India (by id).
    pub abroad: BTreeSet<String>,
    /// Rule 37 reversals falling due in the year (7A).
    pub rule37: Amounts,
    /// Sales accounts in the profit and loss account (GSTR-9C 5A).
    pub books_turnover: Option<Decimal>,
    /// HSN digits the company set (4, 6 or 8); `None`: by turnover.
    pub hsn_digits: Option<u8>,
}

// ----------------------------------------------------------- the books

/// Row id → amounts and documents.
#[derive(Default)]
struct Acc {
    rows: BTreeMap<&'static str, Amounts>,
    docs: BTreeMap<String, BTreeMap<String, DocRef>>,
}

impl Acc {
    fn add(&mut self, row: &'static str, a: &Amounts, d: DocRef) {
        self.rows.entry(row).or_default().add(a);
        let e = self.docs.entry(row.to_owned()).or_default().entry(d.id.clone()).or_insert(DocRef { amounts: Amounts::default(), ..d });
        e.amounts.add(a);
    }
}

fn outward_ref(d: &OutwardDoc) -> DocRef {
    DocRef { id: d.id.clone(), inward: false, kind: d.kind, number: d.number.clone(), date: d.date, party: d.party_name.clone(), amounts: Amounts::default() }
}

fn inward_ref(d: &InwardDoc) -> DocRef {
    DocRef { id: d.id.clone(), inward: true, kind: d.kind, number: d.number.clone(), date: d.date, party: d.party_name.clone(), amounts: Amounts::default() }
}

/// HSN grouping: (hsn, uqc, rate) → qty, amounts, documents.
type HsnAcc = BTreeMap<(String, String, Decimal), (Decimal, Amounts, BTreeMap<String, DocRef>)>;

fn hsn_add(acc: &mut HsnAcc, hsn: &Option<String>, uqc: &Option<String>, rate: Decimal, qty: Decimal, a: &Amounts, d: &DocRef) {
    let key = (hsn.clone().unwrap_or_default(), uqc.clone().unwrap_or_else(|| "NA".into()), rate);
    let e = acc.entry(key).or_default();
    e.0 += qty;
    e.1.add(a);
    let doc = e.2.entry(d.id.clone()).or_insert_with(|| d.clone());
    doc.amounts.add(a);
}

fn hsn_lines(acc: HsnAcc, docs: &mut BTreeMap<String, Vec<DocRef>>, prefix: &str) -> Vec<HsnLine> {
    acc.into_iter()
        .map(|((hsn, uqc, rate), (qty, amounts, refs))| {
            let key = format!("{prefix}|{hsn}|{uqc}|{}", rate.normalize());
            let n = refs.len();
            docs.insert(key.clone(), refs.into_values().collect());
            HsnLine { key, hsn, uqc, qty, rate, amounts, docs: n }
        })
        .collect()
}

/// The FY label "2025-26" for the year starting in `start`.
pub fn fy_label(start: i32) -> String {
    format!("{start}-{:02}", (start + 1).rem_euclid(100))
}

/// 1 April `start` to 31 March of the next year.
pub fn fy_period(start: i32) -> Period {
    Period {
        from: NaiveDate::from_ymd_opt(start, 4, 1).expect("valid date"),
        to: NaiveDate::from_ymd_opt(start + 1, 3, 31).expect("valid date"),
    }
}

/// ₹ crore.
fn crore(n: i64) -> Decimal {
    Decimal::new(n * 10_000_000, 0)
}

/// What applies to the year at this turnover (see the research note for
/// sources and what is uncertain). The aggregate turnover is the PAN's
/// across all GSTINs; the books only know this company's, so the notes say
/// "in the books".
pub fn applicability(fy_start: i32, turnover: Decimal) -> Vec<String> {
    let mut out = Vec::new();
    if turnover <= crore(2) {
        out.push(
            "Turnover in the books is up to ₹2 crore: GSTR-9 is optional for this year if the aggregate turnover across all GSTINs on the PAN is too (Notif. 15/2025-CT and earlier exemptions)".into(),
        );
    }
    if turnover <= crore(5) {
        out.push(
            "Up to ₹5 crore aggregate turnover, HSN codes need 4 digits, and several rows may be left out or reported together: 5D, 5E and 5F together; 6B, 6C, 6D and 6E as inputs only or in total; 7A to 7E together; 8C to 8K; Tables 12, 13, 15, 16, 17 and 18 (as notified for each year)".into(),
        );
    } else {
        out.push("Above ₹5 crore aggregate turnover, HSN codes need 6 digits; GSTR-9C (self-certified reconciliation) is due with GSTR-9".into());
    }
    if fy_start >= 2023 {
        out.push("Table 8A comes from GSTR-2B from FY 2023-24 (GSTR-2A before)".into());
    }
    if fy_start >= 2024 {
        out.push("From FY 2024-25, 6A1 and 6A2 split out ITC of the previous year availed in this year, and 7A splits into Rule 37 (7A1) and Rule 37A (7A2)".into());
    }
    out.push("Due by 31 December after the year (Rule 80). Late fee: section 47 as reduced by Notif. 7/2023-CT; enter it in Table 19".into());
    out
}

/// Work out GSTR-9 from the year's documents and the hand entries.
pub fn gstr9(input: &Gstr9Input<'_>, entries: &Entries) -> Gstr9 {
    let period = input.period.unwrap_or_else(|| fy_period(Decimal::ZERO.to_string().len() as i32 + 2024));
    let fy_start = if period.from.month() >= 4 { period.from.year() } else { period.from.year() - 1 };
    let state = input.supplier_state;
    let mut warnings: Vec<String> = Vec::new();
    // GSTR-1 and GSTR-3B for the year give the same warnings as the monthly
    // returns, and 4G, 6A and 7E (the same rules as those returns).
    let g1 = returns::gstr1(input.outward, period, state);
    warnings.extend(g1.warnings.iter().filter(|w| !w.starts_with("The period spans")).cloned());
    let g3 = returns::gstr3b(input.outward, input.inward, period, state);
    warnings.extend(g3.warnings.iter().filter(|w| !w.starts_with("The period spans") && !g1.warnings.contains(w)).cloned());

    let mut acc = Acc::default();
    let mut hsn_out: HsnAcc = BTreeMap::new();
    let mut hsn_in: HsnAcc = BTreeMap::new();
    let mut ignored = Vec::new();

    for d in input.outward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = returns::label(d.kind, &d.number);
        let lines = returns::resolve(&name, &d.lines, &mut ignored);
        let r = outward_ref(d);
        let sign = d.kind.sign();
        let registered = d.party_gstin.as_deref().is_some_and(|g| !g.trim().is_empty());
        let export = d.place_of_supply == returns::EXPORT_POS;
        let sez = input.sez.contains(&d.id);
        let mut sum = Amounts::default();
        for l in &lines {
            sum.add(&l.amounts);
            hsn_add(&mut hsn_out, &l.hsn, &l.uqc, l.rate, l.qty * sign, &l.amounts.scaled(sign), &r);
        }
        let invoice = d.kind == DocKind::Invoice;
        let note = |with_tax: bool| match (d.kind, with_tax) {
            (DocKind::CreditNote, true) => "4I",
            (DocKind::CreditNote, false) => "5H",
            (_, true) => "4J",
            (_, false) => "5I",
        };
        if export || sez {
            let with_tax = !sum.tax().is_zero();
            let row = match (invoice, export, with_tax) {
                (true, true, true) => "4C",
                (true, true, false) => "5A",
                (true, false, true) => "4D",
                (true, false, false) => "5B",
                (false, _, w) => note(w),
            };
            let a = if with_tax { sum } else { Amounts { taxable: sum.taxable, ..Amounts::default() } };
            acc.add(row, &a, r.clone());
            continue;
        }
        for l in &lines {
            let nil = l.rate.is_zero();
            let row = match (nil, invoice, registered) {
                (true, true, _) => "5E",
                (true, false, _) => note(false),
                (false, _, false) => "4A",
                (false, true, true) => "4B",
                (false, false, true) => note(true),
            };
            // 4A is net of B2C notes; notes elsewhere are their own rows.
            let a = if row == "4A" { l.amounts.scaled(sign) } else if nil { Amounts { taxable: l.amounts.taxable, ..Amounts::default() } } else { l.amounts };
            acc.add(row, &a, r.clone());
        }
    }

    for d in input.inward.iter().filter(|d| period.contains(d.date) && !d.cancelled) {
        let name = returns::label(d.kind, &d.number);
        let lines = returns::resolve(&name, &d.lines, &mut ignored);
        let r = inward_ref(d);
        let sign = d.kind.sign();
        let registered = d.party_gstin.as_deref().is_some_and(|g| !g.trim().is_empty());
        let abroad = input.abroad.contains(&d.id);
        for (l, raw) in lines.iter().zip(&d.lines) {
            let a = l.amounts.scaled(sign);
            hsn_add(&mut hsn_in, &l.hsn, &l.uqc, l.rate, l.qty * sign, &a, &r);
            let kind = if raw.capital_goods {
                "CG"
            } else if l.hsn.as_deref().is_some_and(|h| h.starts_with("99")) {
                "IS"
            } else {
                "IP"
            };
            let itc = Amounts { taxable: Decimal::ZERO, ..a };
            let row: Option<&'static str> = if raw.reverse_charge {
                acc.add("4G", &a, r.clone());
                Some(match (abroad, registered, kind) {
                    (true, _, _) => "6F",
                    (false, false, "IP") => "6C-IP",
                    (false, false, "CG") => "6C-CG",
                    (false, false, _) => "6C-IS",
                    (false, true, "IP") => "6D-IP",
                    (false, true, "CG") => "6D-CG",
                    (false, true, _) => "6D-IS",
                })
            } else if registered {
                Some(match kind {
                    "IP" => "6B-IP",
                    "CG" => "6B-CG",
                    _ => "6B-IS",
                })
            } else if abroad && !a.igst.is_zero() {
                Some(if kind == "CG" { "6E-CG" } else { "6E-IP" })
            } else {
                None
            };
            if let Some(row) = row {
                acc.add(row, &itc, r.clone());
                if raw.blocked {
                    acc.add("7E", &itc, r.clone());
                }
            }
        }
    }
    // 6A: the ITC the books' GSTR-3B returns would show for the year (4A).
    let mut a6 = g3.itc;
    a6.add(&g3.itc_reverse_charge);
    acc.rows.insert("6A", Amounts { taxable: Decimal::ZERO, ..a6 });
    let mut docs6a: BTreeMap<String, DocRef> = BTreeMap::new();
    for k in ["6B-IP", "6B-CG", "6B-IS", "6C-IP", "6C-CG", "6C-IS", "6D-IP", "6D-CG", "6D-IS", "6F"] {
        for (id, d) in acc.docs.get(k).into_iter().flatten() {
            docs6a.entry(id.clone()).or_insert_with(|| d.clone()).amounts.add(&Amounts::default());
        }
    }
    acc.docs.insert("6A".into(), docs6a);
    let rule37_row = if on_form("7A1", fy_start) { "7A1" } else { "7A" };
    if !input.rule37.tax().is_zero() {
        acc.rows.insert(rule37_row, Amounts { taxable: Decimal::ZERO, ..input.rule37 });
    }
    if let Some(t) = input.books_turnover {
        acc.rows.insert("9C-5A", Amounts { taxable: t, ..Amounts::default() });
    }
    // Cross-checks with GSTR-3B: the same rules, so the same figures.
    debug_assert_eq!(acc.rows.get("4G").copied().unwrap_or_default(), g3.reverse_charge_inward);

    let mut docs: BTreeMap<String, Vec<DocRef>> =
        acc.docs.into_iter().map(|(k, v)| (k, v.into_values().filter(|d| !d.id.is_empty()).collect())).collect();

    // Work out every row: books, entries, then totals in the form's order.
    let mut values: BTreeMap<&'static str, BTreeMap<&'static str, Decimal>> = BTreeMap::new();
    let mut tables: Vec<Table> = Vec::new();
    for t in TABLES {
        let keys = t.cols.keys();
        let mut rows = Vec::new();
        for def in ROWS.iter().filter(|r| r.table == t.id && on_form(r.id, fy_start)) {
            let formula = formula_for(def);
            let source = if formula.is_empty() { def.source } else { Source::Total };
            let books: Option<Vec<Decimal>> = match source {
                Source::Total => None,
                Source::Books if t.id == "9" => {
                    // Payable from 4N, by tax.
                    let tax = match def.id {
                        "9-IAMT" => "iamt",
                        "9-CAMT" => "camt",
                        "9-SAMT" => "samt",
                        _ => "csamt",
                    };
                    let payable = values.get("4N").and_then(|v| v.get(tax)).copied().unwrap_or_default();
                    let mut v = vec![Decimal::ZERO; keys.len()];
                    v[0] = payable;
                    Some(v)
                }
                Source::Books => Some(t.cols.of(&acc.rows.get(def.id).copied().unwrap_or_default())),
                _ => None,
            };
            let entered = (source != Source::Total)
                .then(|| entries.get(def.id))
                .flatten()
                .map(|e| keys.iter().map(|k| e.get(*k).copied().unwrap_or_default()).collect::<Vec<_>>());
            let value: Vec<Decimal> = if source == Source::Total {
                keys.iter()
                    .map(|k| formula.iter().map(|(id, s)| values.get(id).and_then(|v| v.get(k)).copied().unwrap_or_default() * Decimal::from(*s)).sum())
                    .collect()
            } else {
                entered.clone().or_else(|| books.clone()).unwrap_or_else(|| vec![Decimal::ZERO; keys.len()])
            };
            values.insert(def.id, keys.iter().copied().zip(value.iter().copied()).collect());
            let note = if def.note.is_empty() && source == Source::Manual { NOT_IN_BOOKS } else { def.note };
            rows.push(Row {
                id: def.id.into(),
                label: def.label.into(),
                source,
                books,
                entered,
                value,
                docs: docs.get(def.id).map_or(0, Vec::len),
                note: note.into(),
            });
        }
        tables.push(Table {
            form: t.form.into(),
            id: t.id.into(),
            part: t.part.into(),
            title: t.title.into(),
            cols: t.cols,
            columns: t.cols.labels().iter().map(|s| (*s).to_owned()).collect(),
            keys: keys.iter().map(|s| (*s).to_owned()).collect(),
            rows,
        });
    }
    let turnover = values.get("5N").and_then(|v| v.get("txval")).copied().unwrap_or_default();
    let hsn_digits = match input.hsn_digits {
        Some(d) if d >= 4 => d.min(8),
        _ if turnover > crore(5) => 6,
        _ => 4,
    };
    let hsn_outward = hsn_lines(hsn_out, &mut docs, "17");
    let hsn_inward = hsn_lines(hsn_in, &mut docs, "18");
    for (table, list) in [("17", &hsn_outward), ("18", &hsn_inward)] {
        let short: Vec<&str> = list.iter().filter(|h| !h.hsn.is_empty() && h.hsn.len() < usize::from(hsn_digits)).map(|h| h.hsn.as_str()).collect();
        if !short.is_empty() {
            warnings.push(format!("Table {table}: HSN/SAC {} has fewer than {hsn_digits} digits", short.join(", ")));
        }
        if list.iter().any(|h| h.hsn.is_empty()) {
            warnings.push(format!("Table {table} has lines without an HSN/SAC code"));
        }
    }
    let mut seen = BTreeSet::new();
    warnings.retain(|w| seen.insert(w.clone()));
    Gstr9 {
        fy: fy_label(fy_start),
        fy_start,
        period,
        tables,
        hsn_outward,
        hsn_inward,
        hsn_digits,
        turnover,
        notes: applicability(fy_start, turnover),
        warnings,
        docs,
    }
}

/// Check and tidy hand entries for one row: known row, not a total, known
/// columns. Zero columns are kept (an entry of zero overrides the books).
pub fn check_entry(id: &str, values: &BTreeMap<String, Decimal>) -> Result<(), String> {
    let def = ROWS.iter().find(|r| r.id == id).ok_or_else(|| format!("{id} is not a row of GSTR-9"))?;
    if def.source == Source::Total || !formula_for(def).is_empty() {
        return Err(format!("{id} is a total; it is worked out from other rows"));
    }
    let keys = table_def(def.table).cols.keys();
    if let Some(k) = values.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(format!("{id} has no column {k}"));
    }
    Ok(())
}

// ------------------------------------------------------------ JSON export

/// Where each row goes in the GSTN GSTR-9 JSON: (row id, table key, row
/// key, `itc_typ` for the lists in Table 6). From the GSTR-9 offline tool
/// and the GSTN API as far as public sources show them; the research note
/// rates each one. NEEDS VALIDATION IN THE GST OFFLINE TOOL.
pub const JSON_ROWS: &[(&str, &str, &str, Option<&str>)] = &[
    ("4A", "table4", "b2c", None),
    ("4B", "table4", "b2b", None),
    ("4C", "table4", "exp", None),
    ("4D", "table4", "sez", None),
    ("4E", "table4", "deemed", None),
    ("4F", "table4", "at", None),
    ("4G", "table4", "rchrg", None),
    ("4H", "table4", "sub_totalAG", None),
    ("4I", "table4", "cr_nt", None),
    ("4J", "table4", "dr_nt", None),
    ("4K", "table4", "amd_pos", None),
    ("4L", "table4", "amd_neg", None),
    ("4M", "table4", "sub_totalIL", None),
    ("4N", "table4", "sup_adv", None),
    ("5A", "table5", "zero_rtd", None),
    ("5B", "table5", "sez", None),
    ("5C", "table5", "rchrg", None),
    ("5D", "table5", "exmt", None),
    ("5E", "table5", "nil", None),
    ("5F", "table5", "non_gst", None),
    ("5G", "table5", "sub_totalAF", None),
    ("5H", "table5", "cr_nt", None),
    ("5I", "table5", "dr_nt", None),
    ("5J", "table5", "amd_pos", None),
    ("5K", "table5", "amd_neg", None),
    ("5L", "table5", "sub_totalHK", None),
    ("5M", "table5", "tover_tax_np", None),
    ("5N", "table5", "total_tover", None),
    ("6A", "table6", "itc_3b", None),
    ("6A1", "table6", "itc_3b_prev_fy", None),
    ("6A2", "table6", "itc_3b_net", None),
    ("6B-IP", "table6", "supp_non_rchrg", Some("ip")),
    ("6B-CG", "table6", "supp_non_rchrg", Some("cg")),
    ("6B-IS", "table6", "supp_non_rchrg", Some("is")),
    ("6C-IP", "table6", "supp_rchrg_unreg", Some("ip")),
    ("6C-CG", "table6", "supp_rchrg_unreg", Some("cg")),
    ("6C-IS", "table6", "supp_rchrg_unreg", Some("is")),
    ("6D-IP", "table6", "supp_rchrg_reg", Some("ip")),
    ("6D-CG", "table6", "supp_rchrg_reg", Some("cg")),
    ("6D-IS", "table6", "supp_rchrg_reg", Some("is")),
    ("6E-IP", "table6", "iog", Some("ip")),
    ("6E-CG", "table6", "iog", Some("cg")),
    ("6F", "table6", "ios", None),
    ("6G", "table6", "isd", None),
    ("6H", "table6", "itc_clmd", None),
    ("6I", "table6", "sub_totalBH", None),
    ("6J", "table6", "difference", None),
    ("6K", "table6", "tran1", None),
    ("6L", "table6", "tran2", None),
    ("6M", "table6", "other", None),
    ("6N", "table6", "sub_totalKM", None),
    ("6O", "table6", "total_itc_availed", None),
    ("7A", "table7", "rule37", None),
    ("7A1", "table7", "rule37", None),
    ("7A2", "table7", "rule37a", None),
    ("7B", "table7", "rule39", None),
    ("7C", "table7", "rule42", None),
    ("7D", "table7", "rule43", None),
    ("7E", "table7", "sec17", None),
    ("7F", "table7", "revsl_tran1", None),
    ("7G", "table7", "revsl_tran2", None),
    ("7H", "table7", "other", None),
    ("7I", "table7", "tot_itc_revd", None),
    ("7J", "table7", "net_itc_aval", None),
    ("8A", "table8", "itc_2a", None),
    ("8B", "table8", "itc_tot", None),
    ("8C", "table8", "itc_inwd_supp", None),
    ("8D", "table8", "differenceABC", None),
    ("8E", "table8", "itc_nt_availd", None),
    ("8F", "table8", "itc_nt_eleg", None),
    ("8G", "table8", "iog_taxpaid", None),
    ("8H", "table8", "iog_itc_availd", None),
    ("8I", "table8", "differenceGH", None),
    ("8J", "table8", "iog_itc_ntavaild", None),
    ("8K", "table8", "tot_itc_lapsed", None),
    ("9-IAMT", "table9", "iamt", None),
    ("9-CAMT", "table9", "camt", None),
    ("9-SAMT", "table9", "samt", None),
    ("9-CSAMT", "table9", "csamt", None),
    ("9-INTR", "table9", "intr", None),
    ("9-FEE", "table9", "fee", None),
    ("9-PEN", "table9", "pen", None),
    ("9-OTH", "table9", "other", None),
    ("10", "table10", "dbn_amd", None),
    ("11", "table10", "cdn_amd", None),
    ("12", "table10", "itc_rvsl", None),
    ("13", "table10", "itc_availd", None),
    ("14-IAMT", "table14", "iamt", None),
    ("14-CAMT", "table14", "camt", None),
    ("14-SAMT", "table14", "samt", None),
    ("14-CSAMT", "table14", "csamt", None),
    ("14-INTR", "table14", "intr", None),
    ("15A", "table15", "rfd_clmd", None),
    ("15B", "table15", "rfd_sanc", None),
    ("15C", "table15", "rfd_rejt", None),
    ("15D", "table15", "rfd_pend", None),
    ("15E", "table15", "tax_dmnd", None),
    ("15F", "table15", "tax_paid", None),
    ("15G", "table15", "dmnd_pend", None),
    ("16A", "table16", "comp_supp", None),
    ("16B", "table16", "deemed_supp", None),
    ("16C", "table16", "not_returned", None),
    ("19-CAMT", "table19", "camt", None),
    ("19-SAMT", "table19", "samt", None),
];

/// Key aliases seen in portal files for the same row (older or newer
/// spellings), read by [`parse_portal`] only.
const JSON_ALIASES: &[(&str, &str, &str)] = &[("8A", "table8", "itc_2b"), ("9-FEE", "table9", "tee")];

fn figures_json(keys: &[String], value: &[Decimal]) -> Map<String, Value> {
    keys.iter().zip(value).map(|(k, v)| (k.clone(), num(*v))).collect()
}

fn hsn_items(list: &[HsnLine]) -> Value {
    list.iter()
        .map(|h| {
            json!({
                "hsn_sc": h.hsn,
                "uqc": h.uqc,
                "desc": "",
                "isconcesstional": "N",
                "qty": num(h.qty),
                "txval": num(h.amounts.taxable),
                "rt": num(h.rate),
                "iamt": num(h.amounts.igst),
                "camt": num(h.amounts.cgst),
                "samt": num(h.amounts.sgst),
                "csamt": num(h.amounts.cess),
            })
        })
        .collect()
}

/// GSTR-9 in the shape of the GSTN JSON (offline tool and API), as far as
/// public sources show it. **Needs validation in the GST offline tool**
/// before it is uploaded: open it there and check every table. Totals are
/// written as worked out; the portal may compute its own. GSTR-9C rows are
/// not part of this file.
pub fn gstr9_json(g: &Gstr9, gstin: &str) -> Value {
    let mut out = Map::new();
    out.insert("gstin".into(), json!(gstin));
    // The return period of an annual return: March of the year's end (MMYYYY).
    out.insert("fp".into(), json!(format!("03{}", g.fy_start + 1)));
    for t in g.tables.iter().filter(|t| t.form == "GSTR-9") {
        for r in &t.rows {
            let Some((_, table, key, typ)) = JSON_ROWS.iter().find(|(id, ..)| *id == r.id) else { continue };
            let figures = figures_json(&t.keys, &r.value);
            let obj = out.entry(table.to_string()).or_insert_with(|| Value::Object(Map::new()));
            let Some(obj) = obj.as_object_mut() else { continue };
            match typ {
                Some(typ) => {
                    let mut f = Map::new();
                    f.insert("itc_typ".into(), json!(typ));
                    f.extend(figures);
                    let list = obj.entry(key.to_string()).or_insert_with(|| Value::Array(Vec::new()));
                    if let Some(a) = list.as_array_mut() {
                        a.push(Value::Object(f));
                    }
                }
                None => {
                    obj.insert(key.to_string(), Value::Object(figures));
                }
            }
        }
    }
    out.insert("table17".into(), json!({ "items": hsn_items(&g.hsn_outward) }));
    out.insert("table18".into(), json!({ "items": hsn_items(&g.hsn_inward) }));
    Value::Object(out)
}

// ------------------------------------------------------------- CSV export

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() }
}

fn csv_line(fields: &[String]) -> String {
    let mut s = fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(",");
    s.push_str("\r\n");
    s
}

fn plain(d: Decimal) -> String {
    d.round_dp(2).normalize().to_string()
}

/// One table as CSV: row, description, then its columns. `table` "all"
/// writes every table of GSTR-9 one after another.
pub fn table_csv(g: &Gstr9, table: &str) -> Result<String, String> {
    match table {
        "17" => return Ok(hsn_csv_gstr9(&g.hsn_outward)),
        "18" => return Ok(hsn_csv_gstr9(&g.hsn_inward)),
        _ => {}
    }
    let picked: Vec<&Table> = g.tables.iter().filter(|t| t.id == table || (table == "all" && t.form == "GSTR-9")).collect();
    if picked.is_empty() {
        return Err(format!("GSTR-9 has no table {table}"));
    }
    let mut out = String::new();
    for t in picked {
        out.push_str(&csv_line(&[format!("{} Table {}", t.form, t.id.trim_start_matches("9C-")), t.title.clone()]));
        let mut head = vec!["Row".to_owned(), "Description".to_owned()];
        head.extend(t.columns.iter().cloned());
        out.push_str(&csv_line(&head));
        for r in &t.rows {
            let mut line = vec![r.id.trim_start_matches("9C-").to_owned(), r.label.clone()];
            line.extend(r.value.iter().map(|v| plain(*v)));
            out.push_str(&csv_line(&line));
        }
        out.push_str("\r\n");
    }
    Ok(out)
}

/// Tables 17 and 18 as CSV, in the order of the offline tool's HSN sheet:
/// HSN, description, UQC, total quantity, taxable value, concessional rate
/// (Y/N), rate, and the four taxes.
pub fn hsn_csv_gstr9(list: &[HsnLine]) -> String {
    let mut out = csv_line(&[
        "HSN".into(),
        "Description".into(),
        "UQC".into(),
        "Total Quantity".into(),
        "Taxable Value".into(),
        "Concessional rate (Y/N)".into(),
        "Rate of Tax (%)".into(),
        "Integrated Tax".into(),
        "Central Tax".into(),
        "State/UT Tax".into(),
        "Cess".into(),
    ]);
    for h in list {
        out.push_str(&csv_line(&[
            h.hsn.clone(),
            String::new(),
            h.uqc.clone(),
            plain(h.qty),
            plain(h.amounts.taxable),
            "N".into(),
            plain(h.rate),
            plain(h.amounts.igst),
            plain(h.amounts.cgst),
            plain(h.amounts.sgst),
            plain(h.amounts.cess),
        ]));
    }
    out
}

/// GSTR-1 Table 12 (HSN summary) as CSV, in the order of the offline
/// tool's HSN sheet: HSN, description, UQC, total quantity, total value,
/// rate, taxable value and the four taxes.
pub fn hsn_csv_gstr1(rows: &[returns::HsnRow]) -> String {
    let mut out = csv_line(&[
        "HSN".into(),
        "Description".into(),
        "UQC".into(),
        "Total Quantity".into(),
        "Total Value".into(),
        "Rate".into(),
        "Taxable Value".into(),
        "Integrated Tax Amount".into(),
        "Central Tax Amount".into(),
        "State/UT Tax Amount".into(),
        "Cess Amount".into(),
    ]);
    for h in rows {
        let a = &h.amounts;
        out.push_str(&csv_line(&[
            h.hsn.clone(),
            String::new(),
            h.uqc.clone(),
            plain(h.qty),
            plain(a.taxable + a.tax()),
            plain(h.rate),
            plain(a.taxable),
            plain(a.igst),
            plain(a.cgst),
            plain(a.sgst),
            plain(a.cess),
        ]));
    }
    out
}

// --------------------------------------------------- portal files (import)

/// What kind of portal file was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PortalKind {
    Gstr9,
    Gstr1,
    Gstr3b,
    Gstr2b,
}

impl PortalKind {
    pub fn label(self) -> &'static str {
        match self {
            PortalKind::Gstr9 => "GSTR-9",
            PortalKind::Gstr1 => "GSTR-1",
            PortalKind::Gstr3b => "GSTR-3B",
            PortalKind::Gstr2b => "GSTR-2B",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortalFile {
    pub name: String,
    pub kind: PortalKind,
    /// The return period in the file (MMYYYY or the year), when given.
    pub period: Option<String>,
    pub gstin: Option<String>,
}

/// The portal's figures, by row and column key, from the files given.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Portal {
    pub files: Vec<PortalFile>,
    pub rows: BTreeMap<String, BTreeMap<String, Decimal>>,
    /// Table 17 (from GSTR-9) or GSTR-1 Table 12 added up.
    pub hsn_outward: Option<Vec<HsnLine>>,
    /// Table 18 (from GSTR-9 only).
    pub hsn_inward: Option<Vec<HsnLine>>,
    /// Which rows each kind of file gave, for the comparison's notes.
    pub row_kinds: BTreeMap<String, PortalKind>,
    pub warnings: Vec<String>,
}

fn dec(v: &Value) -> Decimal {
    match v {
        Value::Number(n) => n.to_string().parse::<Decimal>().unwrap_or_default(),
        Value::String(s) => s.trim().replace(',', "").parse::<Decimal>().unwrap_or_default(),
        _ => Decimal::ZERO,
    }
}

fn field(v: &Value, keys: &[&str]) -> Decimal {
    keys.iter().find_map(|k| v.get(*k)).map(dec).unwrap_or_default()
}

fn text(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match v.get(*k)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// The object holding one of `keys`, at the top or under `data`, `gstr9`,
/// `gstr1`, `gstr3b` or `retdata` keys (up to four deep).
fn find_with<'a>(v: &'a Value, keys: &[&str], depth: usize) -> Option<&'a Value> {
    let o = v.as_object()?;
    if keys.iter().any(|k| o.contains_key(*k)) {
        return Some(v);
    }
    if depth == 0 {
        return None;
    }
    ["data", "gstr9", "GSTR9", "gstr1", "gstr3b", "retdata", "detail", "summary"]
        .iter()
        .filter_map(|k| o.get(*k))
        .find_map(|c| find_with(c, keys, depth - 1))
}

fn amounts(v: &Value) -> Amounts {
    Amounts {
        taxable: field(v, &["txval"]),
        igst: field(v, &["iamt", "igst"]),
        cgst: field(v, &["camt", "cgst"]),
        sgst: field(v, &["samt", "sgst"]),
        cess: field(v, &["csamt", "cess"]),
    }
}

fn list<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v.get(key).and_then(Value::as_array).into_iter().flatten()
}

fn items_sum(doc: &Value) -> Amounts {
    let mut a = Amounts::default();
    for it in list(doc, "itms") {
        a.add(&amounts(it.get("itm_det").unwrap_or(it)));
    }
    if a == Amounts::default() {
        a = amounts(doc);
    }
    a
}

fn hsn_from_items(items: &[&Value], prefix: &str) -> Vec<HsnLine> {
    let mut acc: BTreeMap<(String, String, Decimal), (Decimal, Amounts)> = BTreeMap::new();
    for it in items {
        let hsn = text(it, &["hsn_sc", "hsn"]).unwrap_or_default();
        let uqc = text(it, &["uqc"]).unwrap_or_else(|| "NA".into());
        let rate = field(it, &["rt"]).normalize();
        let e = acc.entry((hsn, uqc, rate)).or_default();
        e.0 += field(it, &["qty"]);
        e.1.add(&amounts(it));
    }
    acc.into_iter()
        .map(|((hsn, uqc, rate), (qty, amounts))| HsnLine { key: format!("{prefix}|{hsn}|{uqc}|{rate}"), hsn, uqc, qty, rate, amounts, docs: 0 })
        .collect()
}

/// Add a GSTR-9 JSON (the portal's system-computed download, the offline
/// tool's file, or this app's export).
fn read_gstr9(root: &Value, rows: &mut BTreeMap<String, BTreeMap<String, Decimal>>, portal: &mut Portal) {
    let all = JSON_ROWS.iter().map(|(id, t, k, typ)| (*id, *t, *k, *typ)).chain(JSON_ALIASES.iter().map(|(id, t, k)| (*id, *t, *k, None)));
    for (id, table, key, typ) in all {
        let Some(def) = ROWS.iter().find(|r| r.id == id) else { continue };
        let Some(node) = root.get(table).and_then(|t| t.get(key)) else { continue };
        let node = match (typ, node) {
            (Some(typ), Value::Array(a)) => {
                match a.iter().find(|e| text(e, &["itc_typ"]).is_some_and(|t| t.eq_ignore_ascii_case(typ))) {
                    Some(e) => e,
                    None => continue,
                }
            }
            (None, Value::Array(a)) => {
                // A list where one object is expected ("other" rows): add them up.
                let keys = table_def(def.table).cols.keys();
                let mut sum: BTreeMap<String, Decimal> = BTreeMap::new();
                for e in a {
                    for k in keys {
                        *sum.entry((*k).to_owned()).or_default() += field(e, &[k]);
                    }
                }
                rows.insert(id.to_owned(), sum);
                continue;
            }
            (_, n) => n,
        };
        if !node.is_object() {
            continue;
        }
        let keys = table_def(def.table).cols.keys();
        let figures: BTreeMap<String, Decimal> = keys.iter().map(|k| ((*k).to_owned(), field(node, &[k]))).collect();
        rows.insert(id.to_owned(), figures);
    }
    // 7A and 7A1 are the same key: keep the one this year has.
    if let Some(items) = root.get("table17").and_then(|t| t.get("items")).and_then(Value::as_array) {
        portal.hsn_outward = Some(hsn_from_items(&items.iter().collect::<Vec<_>>(), "17"));
    }
    if let Some(items) = root.get("table18").and_then(|t| t.get("items")).and_then(Value::as_array) {
        portal.hsn_inward = Some(hsn_from_items(&items.iter().collect::<Vec<_>>(), "18"));
    }
}

fn add_row(rows: &mut BTreeMap<String, BTreeMap<String, Decimal>>, id: &str, a: &Amounts, sign: Decimal) {
    let Some(def) = ROWS.iter().find(|r| r.id == id) else { return };
    let cols = table_def(def.table).cols;
    let e = rows.entry(id.to_owned()).or_default();
    for (k, v) in cols.keys().iter().zip(cols.of(a)) {
        *e.entry((*k).to_owned()).or_default() += v * sign;
    }
}

/// Add a GSTR-1 JSON (a month's or quarter's return as the offline tool
/// or the portal gives it): Tables 4 and 5 by section, and Table 12 (HSN)
/// for Table 17.
fn read_gstr1(root: &Value, rows: &mut BTreeMap<String, BTreeMap<String, Decimal>>, hsn: &mut Vec<Value>, warnings: &mut Vec<String>) {
    let one = Decimal::ONE;
    for ctin in list(root, "b2b") {
        for inv in list(ctin, "inv") {
            let a = items_sum(inv);
            let rc = text(inv, &["rchrg"]).is_some_and(|r| r.eq_ignore_ascii_case("Y"));
            let typ = text(inv, &["inv_typ"]).unwrap_or_else(|| "R".into()).to_ascii_uppercase();
            let id = match (rc, typ.as_str()) {
                (true, _) => "5C",
                (_, "SEWP") => "4D",
                (_, "SEWOP") => "5B",
                (_, "DE") => "4E",
                _ => "4B",
            };
            add_row(rows, id, &a, one);
        }
    }
    for pos in list(root, "b2cl") {
        for inv in list(pos, "inv") {
            add_row(rows, "4A", &items_sum(inv), one);
        }
    }
    for r in list(root, "b2cs") {
        add_row(rows, "4A", &amounts(r), one);
    }
    for ctin in list(root, "cdnr") {
        for nt in list(ctin, "nt") {
            let credit = !text(nt, &["ntty"]).is_some_and(|t| t.eq_ignore_ascii_case("D"));
            let typ = text(nt, &["inv_typ"]).unwrap_or_default().to_ascii_uppercase();
            let without_tax = typ == "SEWOP";
            let id = match (credit, without_tax) {
                (true, false) => "4I",
                (false, false) => "4J",
                (true, true) => "5H",
                (false, true) => "5I",
            };
            add_row(rows, id, &items_sum(nt), one);
        }
    }
    for nt in list(root, "cdnur") {
        let credit = !text(nt, &["ntty"]).is_some_and(|t| t.eq_ignore_ascii_case("D"));
        let typ = text(nt, &["typ"]).unwrap_or_default().to_ascii_uppercase();
        let a = items_sum(nt);
        match typ.as_str() {
            "B2CL" => add_row(rows, "4A", &a, if credit { -one } else { one }),
            "EXPWOP" => add_row(rows, if credit { "5H" } else { "5I" }, &a, one),
            _ => add_row(rows, if credit { "4I" } else { "4J" }, &a, one),
        }
    }
    for e in list(root, "exp") {
        let with_pay = text(e, &["exp_typ"]).is_some_and(|t| t.eq_ignore_ascii_case("WPAY"));
        for inv in list(e, "inv") {
            add_row(rows, if with_pay { "4C" } else { "5A" }, &items_sum(inv), one);
        }
    }
    if let Some(nil) = root.get("nil") {
        for r in list(nil, "inv") {
            for (k, id) in [("nil_amt", "5E"), ("expt_amt", "5D"), ("ngsup_amt", "5F")] {
                add_row(rows, id, &Amounts { taxable: field(r, &[k]), ..Amounts::default() }, one);
            }
        }
    }
    if let Some(h) = root.get("hsn") {
        for k in ["data", "hsn_b2b", "hsn_b2c"] {
            hsn.extend(list(h, k).cloned());
        }
    }
    let amended: Vec<&str> =
        ["b2ba", "b2cla", "b2csa", "cdnra", "cdnura", "expa", "ata", "txpda"].into_iter().filter(|k| root.get(*k).and_then(Value::as_array).is_some_and(|a| !a.is_empty())).collect();
    if !amended.is_empty() {
        warnings.push(format!("GSTR-1 amendments ({}) are not added in; compare them with 4K, 4L, 5J and 5K by hand", amended.join(", ")));
    }
}

/// Add a GSTR-3B JSON (`sup_details`, `itc_elg`): 4G from inward supplies
/// on reverse charge, 6A from the ITC availed, and tax payable for Table 9.
fn read_gstr3b(root: &Value, rows: &mut BTreeMap<String, BTreeMap<String, Decimal>>) {
    let one = Decimal::ONE;
    let sup = root.get("sup_details");
    let part = |k: &str| sup.and_then(|s| s.get(k)).map(amounts).unwrap_or_default();
    let rc = part("isup_rev");
    add_row(rows, "4G", &rc, one);
    let mut payable = part("osup_det");
    payable.add(&part("osup_zero"));
    payable.add(&rc);
    for (id, v) in [("9-IAMT", payable.igst), ("9-CAMT", payable.cgst), ("9-SAMT", payable.sgst), ("9-CSAMT", payable.cess)] {
        *rows.entry(id.into()).or_default().entry("txpyble".into()).or_default() += v;
    }
    let mut itc = Amounts::default();
    if let Some(elg) = root.get("itc_elg") {
        for r in list(elg, "itc_avl") {
            itc.add(&amounts(r));
        }
    }
    add_row(rows, "6A", &itc, one);
}

/// Add a GSTR-2B JSON (`docdata`): 8A from B2B and CDNR documents on which
/// ITC is available (not on reverse charge; credit notes reduce it), and
/// 8G from bills of entry (imports of goods).
fn read_gstr2b(g: &gstr2b::Gstr2b, rows: &mut BTreeMap<String, BTreeMap<String, Decimal>>, warnings: &mut Vec<String>) {
    let mut itc = Amounts::default();
    let mut amended = 0;
    for d in &g.docs {
        match d.section {
            gstr2b::Section::B2ba | gstr2b::Section::Cdnra => amended += 1,
            _ if d.itc_available && !d.reverse_charge => itc.add(&d.amounts.scaled(d.kind.sign())),
            _ => {}
        }
    }
    add_row(rows, "8A", &Amounts { taxable: Decimal::ZERO, ..itc }, Decimal::ONE);
    let imports = g.imports.iter().fold(Amounts::default(), |mut a, b| {
        a.igst += b.igst;
        a.cess += b.cess;
        a
    });
    if !g.imports.is_empty() {
        add_row(rows, "8G", &imports, Decimal::ONE);
    }
    if amended > 0 {
        warnings.push(format!("GSTR-2B {}: {amended} amended document(s) (B2BA, CDNRA) are not added to 8A", g.period.clone().unwrap_or_default()));
    }
    warnings.extend(g.warnings.iter().cloned());
}

/// The year a portal period (MMYYYY) falls in, by its start.
fn fy_of_period(p: &str) -> Option<i32> {
    if p.len() != 6 {
        return None;
    }
    let m: u32 = p[..2].parse().ok()?;
    let y: i32 = p[2..].parse().ok()?;
    Some(if m >= 4 { y } else { y - 1 })
}

/// Read the files the user picked: (file name, text). Each is recognised
/// by its content. A GSTR-9 file's figures win over those added up from
/// GSTR-1, GSTR-3B and GSTR-2B files for the same row.
pub fn parse_portal(files: &[(String, String)], fy_start: i32) -> Result<Portal, String> {
    let mut p = Portal::default();
    let mut from9: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
    let mut from1: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
    let mut from3b: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
    let mut from2b: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
    let mut hsn1: Vec<Value> = Vec::new();
    for (name, text_) in files {
        let root: Value = serde_json::from_str(text_.trim_start_matches('\u{feff}')).map_err(|e| format!("{name} is not a JSON file: {e}"))?;
        let (kind, node) = if let Some(n) = find_with(&root, &["table4", "table5", "table6", "table8", "table17"], 4) {
            (PortalKind::Gstr9, n)
        } else if let Some(n) = find_with(&root, &["docdata"], 4) {
            (PortalKind::Gstr2b, n)
        } else if let Some(n) = find_with(&root, &["sup_details", "itc_elg"], 4) {
            (PortalKind::Gstr3b, n)
        } else if let Some(n) = find_with(&root, &["b2b", "b2cl", "b2cs", "cdnr", "cdnur", "exp", "nil", "hsn"], 4) {
            (PortalKind::Gstr1, n)
        } else {
            return Err(format!("{name} is not a GSTR-9, GSTR-1, GSTR-3B or GSTR-2B JSON file this app can read"));
        };
        let period = text(node, &["fp", "ret_period", "rtnprd", "fy"]).or_else(|| text(&root, &["fp", "ret_period", "rtnprd", "fy"]));
        let gstin = text(node, &["gstin"]).or_else(|| text(&root, &["gstin"]));
        match kind {
            PortalKind::Gstr9 => read_gstr9(node, &mut from9, &mut p),
            PortalKind::Gstr1 => read_gstr1(node, &mut from1, &mut hsn1, &mut p.warnings),
            PortalKind::Gstr3b => read_gstr3b(node, &mut from3b),
            PortalKind::Gstr2b => {
                let g = gstr2b::parse(text_)?;
                read_gstr2b(&g, &mut from2b, &mut p.warnings);
            }
        }
        if kind != PortalKind::Gstr9
            && let Some(fy) = period.as_deref().and_then(fy_of_period)
            && fy != fy_start
        {
            p.warnings.push(format!("{name} is for {}, which is not in {}", period.clone().unwrap_or_default(), fy_label(fy_start)));
        }
        p.files.push(PortalFile { name: name.clone(), kind, period, gstin });
    }
    let mut dup: BTreeMap<(PortalKind, String), usize> = BTreeMap::new();
    for f in p.files.iter().filter(|f| f.kind != PortalKind::Gstr9) {
        *dup.entry((f.kind, f.period.clone().unwrap_or_default())).or_default() += 1;
    }
    for ((kind, period), n) in dup.into_iter().filter(|(k, n)| *n > 1 && !k.1.is_empty()) {
        p.warnings.push(format!("{n} {} files are for {period}; each is added in", kind.label()));
    }
    for (kind, rows) in [(PortalKind::Gstr2b, from2b), (PortalKind::Gstr3b, from3b), (PortalKind::Gstr1, from1), (PortalKind::Gstr9, from9)] {
        for (id, figures) in rows {
            p.row_kinds.insert(id.clone(), kind);
            p.rows.insert(id, figures);
        }
    }
    // 7A and 7A1 share a key: give the figure to the row this year has.
    let rule37 = if on_form("7A1", fy_start) { ("7A", "7A1") } else { ("7A1", "7A") };
    if let Some(v) = p.rows.remove(rule37.0) {
        p.rows.entry(rule37.1.into()).or_insert(v);
    }
    if p.hsn_outward.is_none() && !hsn1.is_empty() {
        p.hsn_outward = Some(hsn_from_items(&hsn1.iter().collect::<Vec<_>>(), "17"));
    }
    let gstins: BTreeSet<&str> = p.files.iter().filter_map(|f| f.gstin.as_deref()).collect();
    if gstins.len() > 1 {
        p.warnings.push(format!("the files are for more than one GSTIN: {}", gstins.into_iter().collect::<Vec<_>>().join(", ")));
    }
    Ok(p)
}

// ------------------------------------------------------- reconciliation

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ReconStatus {
    Matched,
    Differs,
    /// The books have a figure; the files given do not cover the row.
    NotInFiles,
}

/// One column of one row: books against the portal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconLine {
    pub table: String,
    pub row: String,
    pub label: String,
    pub column: String,
    pub key: String,
    pub books: Decimal,
    pub portal: Option<Decimal>,
    /// Books − portal.
    pub difference: Decimal,
    pub status: ReconStatus,
    /// Which file gave the portal figure.
    pub from: Option<PortalKind>,
    pub docs: usize,
}

/// One HSN summary row (by HSN and rate): books against the portal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HsnRecon {
    pub table: String,
    pub hsn: String,
    pub rate: Decimal,
    /// Drill-down keys of the books' rows (one per UQC).
    pub keys: Vec<String>,
    pub books: Option<Amounts>,
    pub portal: Option<Amounts>,
    pub books_qty: Decimal,
    pub portal_qty: Decimal,
    pub differences: Vec<String>,
    pub status: ReconStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gstr9Recon {
    pub fy: String,
    pub files: Vec<PortalFile>,
    pub lines: Vec<ReconLine>,
    pub hsn: Vec<HsnRecon>,
    pub matched: usize,
    pub differs: usize,
    pub warnings: Vec<String>,
}

fn hsn_recon(table: &str, books: &[HsnLine], portal: &[HsnLine], tol: Decimal) -> Vec<HsnRecon> {
    type Side = (Vec<String>, Amounts, Decimal);
    let mut by: BTreeMap<(String, Decimal), (Option<Side>, Option<Side>)> = BTreeMap::new();
    for h in books {
        let e = by.entry((h.hsn.clone(), h.rate.normalize())).or_default();
        let s = e.0.get_or_insert_with(|| (Vec::new(), Amounts::default(), Decimal::ZERO));
        s.0.push(h.key.clone());
        s.1.add(&h.amounts);
        s.2 += h.qty;
    }
    for h in portal {
        let e = by.entry((h.hsn.clone(), h.rate.normalize())).or_default();
        let s = e.1.get_or_insert_with(|| (Vec::new(), Amounts::default(), Decimal::ZERO));
        s.1.add(&h.amounts);
        s.2 += h.qty;
    }
    by.into_iter()
        .map(|((hsn, rate), (b, p))| {
            let mut differences = Vec::new();
            if let (Some(b), Some(p)) = (&b, &p) {
                for (label, x, y) in [
                    ("taxable value", b.1.taxable, p.1.taxable),
                    ("IGST", b.1.igst, p.1.igst),
                    ("CGST", b.1.cgst, p.1.cgst),
                    ("SGST/UTGST", b.1.sgst, p.1.sgst),
                    ("cess", b.1.cess, p.1.cess),
                    ("quantity", b.2, p.2),
                ] {
                    if (x - y).abs() > tol {
                        differences.push(format!("{label} {} in books, {} on the portal", plain(x), plain(y)));
                    }
                }
            }
            let status = match (&b, &p) {
                (Some(_), Some(_)) if differences.is_empty() => ReconStatus::Matched,
                (Some(_), None) | (None, Some(_)) | (Some(_), Some(_)) => ReconStatus::Differs,
                (None, None) => ReconStatus::NotInFiles,
            };
            if b.is_none() {
                differences.push("on the portal, not in the books".into());
            } else if p.is_none() {
                differences.push("in the books, not on the portal".into());
            }
            HsnRecon {
                table: table.into(),
                hsn,
                rate,
                keys: b.as_ref().map(|s| s.0.clone()).unwrap_or_default(),
                books: b.as_ref().map(|s| s.1),
                portal: p.as_ref().map(|s| s.1),
                books_qty: b.as_ref().map_or(Decimal::ZERO, |s| s.2),
                portal_qty: p.as_ref().map_or(Decimal::ZERO, |s| s.2),
                differences,
                status,
            }
        })
        .collect()
}

/// Compare the return worked out from the books with the portal's figures,
/// row by row and column by column (a difference up to `tolerance` counts
/// as matched). Rows the files do not cover are listed only when the books
/// have a figure in a table the files cover.
pub fn reconcile(g: &Gstr9, portal: &Portal, tolerance: Decimal) -> Gstr9Recon {
    let covered: BTreeSet<&str> = portal.rows.keys().filter_map(|id| ROWS.iter().find(|r| r.id == id).map(|r| r.table)).collect();
    let mut lines = Vec::new();
    for t in g.tables.iter().filter(|t| t.form == "GSTR-9") {
        for r in &t.rows {
            let theirs = portal.rows.get(&r.id);
            if theirs.is_none() && !covered.contains(t.id.as_str()) {
                continue;
            }
            for (i, key) in t.keys.iter().enumerate() {
                let books = r.value.get(i).copied().unwrap_or_default();
                let p = theirs.map(|m| m.get(key).copied().unwrap_or_default());
                if books.is_zero() && p.is_none_or(|v| v.is_zero()) {
                    continue;
                }
                let difference = books - p.unwrap_or_default();
                let status = match p {
                    None => ReconStatus::NotInFiles,
                    Some(_) if difference.abs() <= tolerance => ReconStatus::Matched,
                    Some(_) => ReconStatus::Differs,
                };
                lines.push(ReconLine {
                    table: t.id.clone(),
                    row: r.id.clone(),
                    label: r.label.clone(),
                    column: t.columns[i].clone(),
                    key: key.clone(),
                    books,
                    portal: p,
                    difference,
                    status,
                    from: portal.row_kinds.get(&r.id).copied(),
                    docs: r.docs,
                });
            }
        }
    }
    let mut hsn = Vec::new();
    if let Some(p) = &portal.hsn_outward {
        hsn.extend(hsn_recon("17", &g.hsn_outward, p, tolerance));
    }
    if let Some(p) = &portal.hsn_inward {
        hsn.extend(hsn_recon("18", &g.hsn_inward, p, tolerance));
    }
    let matched = lines.iter().filter(|l| l.status == ReconStatus::Matched).count() + hsn.iter().filter(|h| h.status == ReconStatus::Matched).count();
    let differs = lines.iter().filter(|l| l.status == ReconStatus::Differs).count() + hsn.iter().filter(|h| h.status == ReconStatus::Differs).count();
    let mut warnings = portal.warnings.clone();
    if portal.files.iter().all(|f| f.kind != PortalKind::Gstr9) {
        warnings.push("No GSTR-9 file was given: the portal side is added up from the GSTR-1, GSTR-3B and GSTR-2B files, so rows they do not cover are not compared".into());
    }
    Gstr9Recon { fy: g.fy.clone(), files: portal.files.clone(), lines, hsn, matched, differs, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::returns::DocLine;
    use rust_decimal_macros::dec;

    const MH: &str = "27";
    const KA: &str = "29AAGCB7383J1Z4";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn line(hsn: &str, uqc: &str, qty: Decimal, taxable: Decimal, rate: Decimal, inter: bool) -> DocLine {
        let tax = (taxable * rate / dec!(100)).round_dp(2);
        DocLine {
            hsn: Some(hsn.into()),
            uqc: Some(uqc.into()),
            qty: Some(qty),
            taxable_value: taxable,
            rate: Some(rate),
            igst: if inter { tax } else { dec!(0) },
            cgst: if inter { dec!(0) } else { tax / dec!(2) },
            sgst: if inter { dec!(0) } else { tax / dec!(2) },
            ..DocLine::default()
        }
    }

    fn sale(id: &str, kind: DocKind, date: NaiveDate, gstin: Option<&str>, pos: &str, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc {
            id: id.into(),
            kind,
            number: id.into(),
            date,
            party_name: format!("Party {id}"),
            party_gstin: gstin.map(Into::into),
            place_of_supply: pos.into(),
            value: None,
            cancelled: false,
            lines,
        }
    }

    fn purchase(id: &str, kind: DocKind, date: NaiveDate, gstin: Option<&str>, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc { id: id.into(), kind, number: id.into(), date, party_name: format!("Supplier {id}"), party_gstin: gstin.map(Into::into), cancelled: false, lines }
    }

    /// A year of books for FY 2025-26 (a Maharashtra business).
    fn books() -> (Vec<OutwardDoc>, Vec<InwardDoc>) {
        let outward = vec![
            // B2B inter-State, 2 laptops at 18%.
            sale("S1", DocKind::Invoice, d(2025, 5, 2), Some(KA), "29", vec![line("847130", "NOS", dec!(2), dec!(100000), dec!(18), true)]),
            // B2C within the State: one laptop and a nil-rated line.
            sale("S2", DocKind::Invoice, d(2025, 6, 3), None, MH, vec![
                line("847130", "NOS", dec!(1), dec!(40000), dec!(18), false),
                line("0401", "LTR", dec!(10), dec!(500), dec!(0), false),
            ]),
            // A sales return from the B2B buyer: one laptop.
            sale("C1", DocKind::CreditNote, d(2025, 7, 10), Some(KA), "29", vec![line("847130", "NOS", dec!(1), dec!(50000), dec!(18), true)]),
            // A B2C credit note, netted into 4A.
            sale("C2", DocKind::CreditNote, d(2025, 7, 11), None, MH, vec![line("847130", "NOS", dec!(0), dec!(4000), dec!(18), false)]),
            // Export under LUT and export with IGST.
            sale("E1", DocKind::Invoice, d(2025, 8, 1), None, "96", vec![DocLine { rate: Some(dec!(0)), ..line("847130", "NOS", dec!(1), dec!(60000), dec!(0), true) }]),
            sale("E2", DocKind::Invoice, d(2025, 8, 2), None, "96", vec![line("847130", "NOS", dec!(1), dec!(60000), dec!(18), true)]),
            // SEZ under LUT.
            sale("Z1", DocKind::Invoice, d(2025, 9, 1), Some("27AAPFU0939F1ZV"), MH, vec![line("998314", "OTH", dec!(0), dec!(20000), dec!(0), true)]),
            // Next year: ignored. A cancelled one: ignored.
            sale("N1", DocKind::Invoice, d(2026, 4, 1), Some(KA), "29", vec![line("847130", "NOS", dec!(1), dec!(1000), dec!(18), true)]),
            OutwardDoc { cancelled: true, ..sale("X1", DocKind::Invoice, d(2025, 9, 9), Some(KA), "29", vec![line("847130", "NOS", dec!(1), dec!(1000), dec!(18), true)]) },
        ];
        let inward = vec![
            // Goods from a registered supplier, 18% IGST.
            purchase("P1", DocKind::Invoice, d(2025, 5, 5), Some(KA), vec![line("847130", "NOS", dec!(3), dec!(90000), dec!(18), true)]),
            // A machine (capital goods) and a service.
            purchase("P2", DocKind::Invoice, d(2025, 6, 5), Some(KA), vec![
                DocLine { capital_goods: true, ..line("8462", "NOS", dec!(1), dec!(50000), dec!(18), true) },
                line("998313", "OTH", dec!(0), dec!(10000), dec!(18), true),
            ]),
            // A purchase return of one laptop (the supplier's credit note).
            purchase("R1", DocKind::CreditNote, d(2025, 7, 5), Some(KA), vec![line("847130", "NOS", dec!(1), dec!(30000), dec!(18), true)]),
            // Reverse charge: legal services from an unregistered advocate, within the State.
            purchase("P3", DocKind::Invoice, d(2025, 8, 5), None, vec![DocLine { reverse_charge: true, ..line("998211", "OTH", dec!(0), dec!(10000), dec!(18), false) }]),
            // A car: blocked credit.
            purchase("P4", DocKind::Invoice, d(2025, 9, 5), Some(KA), vec![DocLine { blocked: true, capital_goods: true, ..line("8703", "NOS", dec!(1), dec!(500000), dec!(28), true) }]),
            // Software bought from abroad on reverse charge (import of services).
            purchase("P5", DocKind::Invoice, d(2025, 10, 5), None, vec![DocLine { reverse_charge: true, ..line("997331", "OTH", dec!(0), dec!(20000), dec!(18), true) }]),
        ];
        (outward, inward)
    }

    fn input<'a>(outward: &'a [OutwardDoc], inward: &'a [InwardDoc]) -> Gstr9Input<'a> {
        Gstr9Input {
            period: Some(fy_period(2025)),
            supplier_state: MH,
            outward,
            inward,
            sez: ["Z1".to_owned()].into(),
            abroad: ["P5".to_owned()].into(),
            rule37: Amounts { igst: dec!(900), ..Amounts::default() },
            books_turnover: Some(dec!(226500)),
            hsn_digits: None,
        }
    }

    fn v(g: &Gstr9, id: &str, key: &str) -> Decimal {
        g.value(id, key)
    }

    #[test]
    fn a_year_of_books_gives_the_expected_tables() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        assert_eq!((g.fy.as_str(), g.fy_start), ("2025-26", 2025));
        // Table 4.
        assert_eq!((v(&g, "4A", "txval"), v(&g, "4A", "camt")), (dec!(36000), dec!(3240)), "B2C net of its credit note");
        assert_eq!((v(&g, "4B", "txval"), v(&g, "4B", "iamt")), (dec!(100000), dec!(18000)));
        assert_eq!((v(&g, "4C", "txval"), v(&g, "4C", "iamt")), (dec!(60000), dec!(10800)));
        assert_eq!(v(&g, "4D", "txval"), dec!(0));
        assert_eq!((v(&g, "4G", "txval"), v(&g, "4G", "camt"), v(&g, "4G", "iamt")), (dec!(30000), dec!(900), dec!(3600)));
        assert_eq!(v(&g, "4H", "txval"), dec!(226000));
        assert_eq!((v(&g, "4I", "txval"), v(&g, "4I", "iamt")), (dec!(50000), dec!(9000)));
        assert_eq!(v(&g, "4M", "txval"), dec!(-50000));
        assert_eq!((v(&g, "4N", "txval"), v(&g, "4N", "iamt")), (dec!(176000), dec!(23400)));
        // Table 5.
        assert_eq!(v(&g, "5A", "txval"), dec!(60000));
        assert_eq!(v(&g, "5B", "txval"), dec!(20000));
        assert_eq!(v(&g, "5E", "txval"), dec!(500));
        assert_eq!(v(&g, "5M", "txval"), dec!(80500));
        assert_eq!(v(&g, "5N", "txval"), dec!(226500), "4N + 5M − 4G");
        assert_eq!(g.turnover, dec!(226500));
        // Table 6: 18% of 90,000 − 30,000 = 10,800 on inputs; capital goods include the blocked car.
        assert_eq!(v(&g, "6B-IP", "iamt"), dec!(10800));
        assert_eq!(v(&g, "6B-CG", "iamt"), dec!(9000) + dec!(140000));
        assert_eq!(v(&g, "6B-IS", "iamt"), dec!(1800));
        assert_eq!((v(&g, "6C-IS", "camt"), v(&g, "6C-IS", "samt")), (dec!(900), dec!(900)));
        assert_eq!(v(&g, "6F", "iamt"), dec!(3600));
        assert_eq!(v(&g, "6A", "iamt"), dec!(10800) + dec!(149000) + dec!(1800) + dec!(3600));
        assert_eq!(v(&g, "6J", "iamt"), dec!(0), "the books' 6A is 6B to 6F");
        // Table 7: Rule 37 (7A1 from FY 2024-25) and blocked credit.
        assert!(g.row("7A").is_none() && g.row("7A1").is_some());
        assert_eq!(v(&g, "7A1", "iamt"), dec!(900));
        assert_eq!(v(&g, "7E", "iamt"), dec!(140000));
        assert_eq!(v(&g, "7I", "iamt"), dec!(140900));
        assert_eq!(v(&g, "7J", "iamt"), v(&g, "6O", "iamt") - dec!(140900));
        // Table 8: B from 6B; A is the portal's.
        assert_eq!(v(&g, "8B", "iamt"), dec!(10800) + dec!(149000) + dec!(1800));
        assert_eq!(v(&g, "8D", "iamt"), -v(&g, "8B", "iamt"));
        // Table 9: payable from 4N.
        assert_eq!(v(&g, "9-IAMT", "txpyble"), dec!(23400));
        assert_eq!(v(&g, "9-CAMT", "txpyble"), dec!(3240) + dec!(900));
        // GSTR-9C groundwork.
        assert_eq!(v(&g, "9C-5Q", "txval"), dec!(226500));
        assert_eq!(v(&g, "9C-5R", "txval"), dec!(0));
        assert_eq!(v(&g, "9C-7C", "txval"), dec!(80000));
        assert_eq!(v(&g, "9C-7F", "txval"), dec!(146000));
        // Drill-down.
        let ids = |row: &str| g.docs.get(row).map(|v| v.iter().map(|d| d.id.as_str()).collect::<Vec<_>>()).unwrap_or_default();
        assert_eq!(ids("4A"), ["C2", "S2"]);
        assert_eq!(ids("4I"), ["C1"]);
        assert_eq!(ids("6F"), ["P5"]);
        assert_eq!(g.row("4A").unwrap().docs, 2);
        assert_eq!(g.docs["4A"].iter().find(|d| d.id == "C2").unwrap().amounts.taxable, dec!(-4000));
        assert!(!ids("4B").contains(&"N1") && !ids("4B").contains(&"X1"));
        assert!(g.notes.iter().any(|n| n.contains("₹2 crore")));
    }

    #[test]
    fn tables_agree_with_the_monthly_returns() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let g3 = returns::gstr3b(&o, &i, fy_period(2025), MH);
        // Output tax: 4N without 4G = GSTR-3B 3.1(a) + (b).
        let mut out = g3.outward_taxable;
        out.add(&g3.zero_rated);
        assert_eq!(v(&g, "4N", "iamt") - v(&g, "4G", "iamt"), out.igst);
        assert_eq!(v(&g, "4N", "camt") - v(&g, "4G", "camt"), out.cgst);
        assert_eq!(v(&g, "7E", "iamt"), g3.itc_reversed_permanent.igst);
    }

    #[test]
    fn hsn_summaries_group_by_hsn_uqc_and_rate() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let t17: Vec<_> = g.hsn_outward.iter().map(|h| (h.hsn.as_str(), h.uqc.as_str(), h.rate, h.qty, h.amounts.taxable)).collect();
        assert_eq!(t17, [
            ("0401", "LTR", dec!(0), dec!(10), dec!(500)),
            ("847130", "NOS", dec!(0), dec!(1), dec!(60000)),
            // 2 + 1 + 1 sold, 1 returned; 100,000 + 40,000 + 60,000 − 50,000 − 4,000.
            ("847130", "NOS", dec!(18), dec!(3), dec!(146000)),
            ("998314", "OTH", dec!(0), dec!(0), dec!(20000)),
        ]);
        let laptop = &g.hsn_outward[2];
        assert_eq!((laptop.amounts.igst, laptop.amounts.cgst, laptop.docs), (dec!(18000) + dec!(10800) - dec!(9000), dec!(3600) - dec!(360), 5));
        let t18: Vec<_> = g.hsn_inward.iter().map(|h| (h.hsn.as_str(), h.uqc.as_str(), h.rate, h.qty, h.amounts.taxable)).collect();
        assert_eq!(t18, [
            ("8462", "NOS", dec!(18), dec!(1), dec!(50000)),
            ("847130", "NOS", dec!(18), dec!(2), dec!(60000)),
            ("8703", "NOS", dec!(28), dec!(1), dec!(500000)),
            ("997331", "OTH", dec!(18), dec!(0), dec!(20000)),
            ("998211", "OTH", dec!(18), dec!(0), dec!(10000)),
            ("998313", "OTH", dec!(18), dec!(0), dec!(10000)),
        ]);
        assert_eq!(g.hsn_digits, 4);
        assert!(g.warnings.iter().any(|w| w.contains("Table 17") || w.contains("Table 18")) == false, "{:?}", g.warnings);
        // A 6-digit requirement flags 4-digit codes.
        let mut inp = input(&o, &i);
        inp.hsn_digits = Some(6);
        let g6 = gstr9(&inp, &Entries::new());
        assert!(g6.warnings.iter().any(|w| w == "Table 17: HSN/SAC 0401 has fewer than 6 digits"), "{:?}", g6.warnings);
        assert!(g6.warnings.iter().any(|w| w.contains("Table 18: HSN/SAC 8462, 8703")), "{:?}", g6.warnings);
        // CSV in the offline tool's order.
        let csv = table_csv(&g, "17").unwrap();
        assert!(csv.starts_with("HSN,Description,UQC,Total Quantity,Taxable Value,Concessional rate (Y/N),Rate of Tax (%)"));
        assert!(csv.contains("\r\n847130,,NOS,3,146000,N,18,19800,3240,3240,0\r\n"), "{csv}");
        let g1 = returns::gstr1(&o, fy_period(2025), MH);
        let csv1 = hsn_csv_gstr1(&g1.hsn_b2b);
        assert!(csv1.starts_with("HSN,Description,UQC,Total Quantity,Total Value,Rate,Taxable Value"));
    }

    #[test]
    fn entries_replace_book_figures_and_totals_follow() {
        let (o, i) = books();
        let mut e = Entries::new();
        e.insert("5D".into(), [("txval".to_owned(), dec!(1000))].into());
        e.insert("4B".into(), [("txval".to_owned(), dec!(99000)), ("iamt".to_owned(), dec!(17820))].into());
        e.insert("8A".into(), [("iamt".to_owned(), dec!(170000))].into());
        e.insert("9-IAMT".into(), [("txpyble".to_owned(), dec!(23400)), ("txpaid_cash".to_owned(), dec!(400)), ("tax_paid_itc_iamt".to_owned(), dec!(23000))].into());
        let g = gstr9(&input(&o, &i), &e);
        let b4 = g.row("4B").unwrap();
        assert_eq!(b4.books.as_ref().unwrap()[0], dec!(100000));
        assert_eq!(b4.value[..2], [dec!(99000), dec!(17820)]);
        assert_eq!(b4.value[2], dec!(0), "a column left out of an entry is zero");
        assert_eq!(v(&g, "4N", "txval"), dec!(175000));
        assert_eq!(v(&g, "5G", "txval"), dec!(81500));
        assert_eq!(v(&g, "8D", "iamt"), dec!(170000) - v(&g, "8B", "iamt"));
        assert_eq!(v(&g, "9-IAMT", "txpaid_cash"), dec!(400));
        assert!(check_entry("4N", &BTreeMap::new()).unwrap_err().contains("total"));
        assert!(check_entry("4Z", &BTreeMap::new()).is_err());
        assert!(check_entry("5D", &[("iamt".to_owned(), dec!(1))].into()).unwrap_err().contains("no column iamt"));
        assert!(check_entry("19-CAMT", &[("txpyble".to_owned(), dec!(100))].into()).is_ok());
    }

    #[test]
    fn rows_follow_the_year() {
        let (o, i) = books();
        let mut inp = input(&o, &i);
        inp.period = Some(fy_period(2023));
        let g = gstr9(&inp, &Entries::new());
        assert!(g.row("7A").is_some() && g.row("7A1").is_none() && g.row("6A1").is_none());
        assert_eq!(g.fy, "2023-24");
        // Nothing of the FY 2025-26 books falls in 2023-24, except the Rule 37 figure given.
        assert_eq!((v(&g, "4N", "txval"), v(&g, "7A", "iamt")), (dec!(0), dec!(900)));
        let mut e = Entries::new();
        e.insert("6A1".into(), [("iamt".to_owned(), dec!(100))].into());
        let g = gstr9(&input(&o, &i), &e);
        assert_eq!(v(&g, "6A2", "iamt"), v(&g, "6A", "iamt") - dec!(100));
    }

    #[test]
    fn json_has_the_gstn_shape() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let j = gstr9_json(&g, "27AAPFU0939F1ZV");
        assert_eq!((j["gstin"].as_str(), j["fp"].as_str()), (Some("27AAPFU0939F1ZV"), Some("032026")));
        assert_eq!(j["table4"]["b2b"]["txval"], json!(100000.0));
        assert_eq!(j["table4"]["b2b"]["iamt"], json!(18000.0));
        assert_eq!(j["table4"]["sup_adv"]["txval"], json!(176000.0));
        assert_eq!(j["table5"]["total_tover"]["txval"], json!(226500.0));
        let t6 = j["table6"]["supp_non_rchrg"].as_array().unwrap();
        assert_eq!(t6.iter().map(|x| x["itc_typ"].as_str().unwrap()).collect::<Vec<_>>(), ["ip", "cg", "is"]);
        assert_eq!(t6[0]["iamt"], json!(10800.0));
        assert!(t6[0].get("txval").is_none(), "ITC rows carry taxes only");
        assert_eq!(j["table7"]["rule37"]["iamt"], json!(900.0));
        assert_eq!(j["table9"]["iamt"]["txpyble"], json!(23400.0));
        assert_eq!(j["table17"]["items"][2]["hsn_sc"], "847130");
        assert_eq!(j["table17"]["items"][2]["qty"], json!(3.0));
        assert_eq!(j["table17"]["items"][2]["rt"], json!(18.0));
        assert_eq!(j["table18"]["items"].as_array().unwrap().len(), 6);
        assert!(j.get("9C-5").is_none() && j["table5"].get("9C-5A").is_none(), "GSTR-9C is not in the file");
        // Every object of a table has the table's columns.
        for (id, table, key, typ) in JSON_ROWS {
            let Some(r) = g.row(id) else { continue };
            let node = &j[*table][*key];
            let node = match typ {
                Some(t) => node.as_array().unwrap().iter().find(|x| x["itc_typ"] == *t).unwrap(),
                None => node,
            };
            assert_eq!(node.as_object().unwrap().len() - usize::from(typ.is_some()), r.value.len(), "{id}");
        }
    }

    #[test]
    fn the_exported_json_reads_back_as_the_portal_side() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let text = serde_json::to_string(&gstr9_json(&g, "27AAPFU0939F1ZV")).unwrap();
        let p = parse_portal(&[("own.json".into(), text)], 2025).unwrap();
        assert_eq!(p.files[0].kind, PortalKind::Gstr9);
        let r = reconcile(&g, &p, dec!(1));
        assert_eq!(r.differs, 0, "{:?}", r.lines.iter().filter(|l| l.status != ReconStatus::Matched).collect::<Vec<_>>());
        assert!(r.matched > 20);
        assert!(r.hsn.iter().all(|h| h.status == ReconStatus::Matched));
    }

    /// An invented portal download shaped like the GSTN GSTR-9 JSON, with
    /// the system-computed figures under `data`.
    fn portal_gstr9() -> String {
        json!({
            "data": {
                "gstin": "27AAPFU0939F1ZV",
                "fp": "032026",
                "table4": {
                    "b2c": { "txval": 36000, "iamt": 0, "camt": 3240, "samt": 3240, "csamt": 0 },
                    "b2b": { "txval": 101000, "iamt": 18180, "camt": 0, "samt": 0, "csamt": 0 },
                    "exp": { "txval": 60000, "iamt": 10800, "camt": 0, "samt": 0, "csamt": 0 },
                    "cr_nt": { "txval": 50000, "iamt": 9000, "camt": 0, "samt": 0, "csamt": 0 }
                },
                "table6": {
                    "itc_3b": { "iamt": 165200, "camt": 900, "samt": 900, "csamt": 0 }
                },
                "table8": {
                    "itc_2b": { "iamt": 160000, "camt": 0, "samt": 0, "csamt": 0 }
                },
                "table17": { "items": [
                    { "hsn_sc": "847130", "uqc": "NOS", "qty": 3, "txval": 146000, "rt": 18, "iamt": 19800, "camt": 3240, "samt": 3240, "csamt": 0 },
                    { "hsn_sc": "9983", "uqc": "OTH", "qty": 0, "txval": 5000, "rt": 18, "iamt": 900, "camt": 0, "samt": 0, "csamt": 0 }
                ] }
            }
        })
        .to_string()
    }

    #[test]
    fn reconciles_with_a_portal_gstr9_file() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let p = parse_portal(&[("GSTR9_27AAPFU0939F1ZV_032026.json".into(), portal_gstr9())], 2025).unwrap();
        assert_eq!(p.rows["8A"]["iamt"], dec!(160000), "the newer itc_2b key is read as 8A");
        let r = reconcile(&g, &p, dec!(1));
        let line = |row: &str, key: &str| r.lines.iter().find(|l| l.row == row && l.key == key).unwrap();
        assert_eq!(line("4A", "txval").status, ReconStatus::Matched);
        let b2b = line("4B", "txval");
        assert_eq!((b2b.status, b2b.difference, b2b.portal), (ReconStatus::Differs, dec!(-1000), Some(dec!(101000))));
        assert_eq!(b2b.docs, 1);
        assert_eq!(line("8A", "iamt").books, dec!(0));
        assert_eq!(line("8A", "iamt").status, ReconStatus::Differs);
        // 4C matched; 4G is in the books, and Table 4 is covered, but the file has no 4G.
        assert_eq!(line("4C", "iamt").status, ReconStatus::Matched);
        assert_eq!(line("4G", "txval").status, ReconStatus::NotInFiles);
        // Table 5 is not in the file at all: not listed.
        assert!(!r.lines.iter().any(|l| l.table == "5"));
        let hsn: Vec<_> = r.hsn.iter().map(|h| (h.hsn.as_str(), h.status)).collect();
        assert!(hsn.contains(&("847130", ReconStatus::Matched)));
        assert!(hsn.contains(&("9983", ReconStatus::Differs)));
        assert!(hsn.contains(&("0401", ReconStatus::Differs)));
        assert!(r.differs >= 4);
    }

    #[test]
    fn adds_up_gstr1_gstr3b_and_gstr2b_files() {
        let gstr1_may = json!({
            "gstin": "27AAPFU0939F1ZV", "fp": "052025",
            "b2b": [{ "ctin": KA, "inv": [{ "inum": "S1", "idt": "02-05-2025", "val": 118000, "pos": "29", "rchrg": "N", "inv_typ": "R",
                "itms": [{ "num": 1, "itm_det": { "txval": 100000, "rt": 18, "iamt": 18000, "csamt": 0 } }] }] }],
            "b2cs": [{ "sply_ty": "INTRA", "pos": "27", "typ": "OE", "rt": 18, "txval": 40000, "camt": 3600, "samt": 3600, "csamt": 0 }],
            "nil": { "inv": [{ "sply_ty": "INTRAB2C", "nil_amt": 500, "expt_amt": 0, "ngsup_amt": 0 }] },
            "hsn": { "data": [{ "num": 1, "hsn_sc": "847130", "uqc": "NOS", "qty": 3, "rt": 18, "txval": 140000, "iamt": 18000, "camt": 3600, "samt": 3600, "csamt": 0 }] }
        });
        let gstr1_jul = json!({
            "gstin": "27AAPFU0939F1ZV", "fp": "072025",
            "cdnr": [{ "ctin": KA, "nt": [{ "ntty": "C", "nt_num": "C1", "nt_dt": "10-07-2025", "val": 59000, "pos": "29", "rchrg": "N", "inv_typ": "R",
                "itms": [{ "num": 1, "itm_det": { "txval": 50000, "rt": 18, "iamt": 9000, "csamt": 0 } }] }] }],
            "cdnur": [{ "typ": "B2CL", "ntty": "C", "nt_num": "C9", "nt_dt": "11-07-2025", "val": 1180, "pos": "29", "itms": [{ "num": 1, "itm_det": { "txval": 1000, "rt": 18, "iamt": 180, "csamt": 0 } }] }],
            "exp": [{ "exp_typ": "WOPAY", "inv": [{ "inum": "E1", "idt": "01-08-2025", "val": 60000, "itms": [{ "txval": 60000, "rt": 0, "iamt": 0, "csamt": 0 }] }] }],
            "b2ba": [{ "ctin": KA, "inv": [] }, { "ctin": KA }]
        });
        let gstr3b = json!({
            "gstin": "27AAPFU0939F1ZV", "ret_period": "082025",
            "sup_details": {
                "osup_det": { "txval": 1000, "iamt": 180, "camt": 0, "samt": 0, "csamt": 0 },
                "isup_rev": { "txval": 10000, "iamt": 0, "camt": 900, "samt": 900, "csamt": 0 }
            },
            "itc_elg": { "itc_avl": [
                { "ty": "IMPG", "iamt": 0, "camt": 0, "samt": 0, "csamt": 0 },
                { "ty": "ISRC", "iamt": 0, "camt": 900, "samt": 900, "csamt": 0 },
                { "ty": "OTH", "iamt": 10800, "camt": 0, "samt": 0, "csamt": 0 }
            ] }
        });
        let gstr2b = json!({ "data": { "gstin": "27AAPFU0939F1ZV", "rtnprd": "052025", "docdata": {
            "b2b": [{ "ctin": KA, "trdnm": "KA Supplier", "inv": [
                { "inum": "P1", "dt": "05-05-2025", "val": 106200, "rev": "N", "itcavl": "Y", "txval": 90000, "igst": 16200, "cgst": 0, "sgst": 0, "cess": 0 },
                { "inum": "P9", "dt": "06-05-2025", "val": 1180, "rev": "Y", "itcavl": "Y", "txval": 1000, "igst": 180, "cgst": 0, "sgst": 0, "cess": 0 },
                { "inum": "P8", "dt": "07-05-2025", "val": 1180, "rev": "N", "itcavl": "N", "txval": 1000, "igst": 180, "cgst": 0, "sgst": 0, "cess": 0 }
            ] }],
            "cdnr": [{ "ctin": KA, "nt": [{ "ntnum": "R1", "typ": "C", "dt": "05-07-2025", "val": 35400, "rev": "N", "itcavl": "Y", "txval": 30000, "igst": 5400, "cgst": 0, "sgst": 0, "cess": 0 }] }],
            "impg": [{ "boenum": "1234567", "boedt": "10-05-2025", "portcode": "INNSA1", "txval": 10000, "igst": 1800, "cess": 0 }]
        } } });
        let files = [
            ("may.json".to_owned(), gstr1_may.to_string()),
            ("jul.json".to_owned(), gstr1_jul.to_string()),
            ("3b.json".to_owned(), gstr3b.to_string()),
            ("2b.json".to_owned(), gstr2b.to_string()),
        ];
        let p = parse_portal(&files, 2025).unwrap();
        let kinds: Vec<_> = p.files.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, [PortalKind::Gstr1, PortalKind::Gstr1, PortalKind::Gstr3b, PortalKind::Gstr2b]);
        assert_eq!(p.rows["4B"]["txval"], dec!(100000));
        assert_eq!(p.rows["4A"]["txval"], dec!(39000), "B2CS plus a B2CL credit note taken off");
        assert_eq!(p.rows["4I"]["iamt"], dec!(9000));
        assert_eq!(p.rows["5A"]["txval"], dec!(60000));
        assert_eq!(p.rows["5E"]["txval"], dec!(500));
        assert_eq!(p.rows["4G"]["camt"], dec!(900));
        assert_eq!(p.rows["6A"]["iamt"], dec!(10800));
        assert_eq!(p.rows["9-IAMT"]["txpyble"], dec!(180));
        assert_eq!(p.rows["8A"]["iamt"], dec!(16200) - dec!(5400), "available, forward charge, net of credit notes");
        assert_eq!(p.rows["8G"]["iamt"], dec!(1800));
        assert_eq!(p.row_kinds["8A"], PortalKind::Gstr2b);
        assert_eq!(p.hsn_outward.as_ref().unwrap()[0].amounts.taxable, dec!(140000));
        assert!(p.warnings.iter().any(|w| w.contains("amendments (b2ba)")), "{:?}", p.warnings);

        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let r = reconcile(&g, &p, dec!(1));
        assert!(r.warnings.iter().any(|w| w.starts_with("No GSTR-9 file")));
        let b2b = r.lines.iter().find(|l| l.row == "4B" && l.key == "txval").unwrap();
        assert_eq!((b2b.status, b2b.from), (ReconStatus::Matched, Some(PortalKind::Gstr1)));

        // A file for another year is flagged; junk is refused.
        let p = parse_portal(&[("old.json".into(), gstr1_may.to_string().replace("052025", "052024"))], 2025).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("is for 052024, which is not in 2025-26")));
        assert!(parse_portal(&[("x.json".into(), "{\"a\":1}".into())], 2025).unwrap_err().contains("not a GSTR-9"));
        assert!(parse_portal(&[("x.json".into(), "nope".into())], 2025).unwrap_err().contains("not a JSON file"));
    }

    #[test]
    fn table_csv_lists_rows_with_their_columns() {
        let (o, i) = books();
        let g = gstr9(&input(&o, &i), &Entries::new());
        let csv = table_csv(&g, "4").unwrap();
        let mut lines = csv.lines();
        assert!(lines.next().unwrap().starts_with("GSTR-9 Table 4,"));
        assert_eq!(lines.next().unwrap(), "Row,Description,Taxable Value,IGST,CGST,SGST/UTGST,Cess");
        assert_eq!(lines.next().unwrap(), "4A,Supplies to unregistered persons (B2C),36000,0,3240,3240,0");
        assert!(table_csv(&g, "9C-5").unwrap().contains("\r\n5A,"));
        assert!(table_csv(&g, "all").unwrap().contains("GSTR-9 Table 19"));
        assert!(!table_csv(&g, "all").unwrap().contains("GSTR-9C"));
        assert!(table_csv(&g, "99").is_err());
    }
}

//! What an import did, beyond the counts: notes on things imported with a
//! caveat, checks of the result against Tally, and the report as text or
//! CSV, in full or as a shareable summary with counts only.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use chrono::NaiveDate;
use khata_core::{Amount, VoucherKind};

use crate::{ImportReport, ItemKind};

/// What a [`Note`] is about. Each kind has a plain description without
/// names or amounts, used by the shareable summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NoteKind {
    /// The Day Book starts well after its financial year begins.
    DayBookStartsLate,
    /// Vouchers came without any ledger masters.
    NoLedgerMasters,
    /// Ledgers were imported, but none had an opening balance.
    NoOpeningBalances,
    /// The opening balances don't add up to zero.
    OpeningDifference,
    /// A closing balance differs from the one Tally gave.
    BalanceDiffers,
    /// A balance Tally gave matches no ledger or group here.
    BalanceNotFound,
    /// A voucher got a new number because Tally used its number twice.
    Renumbered,
    /// A voucher type's numbering was set to restart each financial year.
    YearlyNumbering,
    /// A post-dated voucher, posted on its date.
    PostDated,
    /// A voucher line's bill allocations could not be kept.
    BillsDropped,
    /// A ledger's opening bills could not all be kept, or don't cover its
    /// opening balance.
    OpeningBills,
    /// A GSTIN that fails the format or check character test, kept anyway.
    GstinInvalid,
    /// A state name that matches no GST state.
    StateUnknown,
    /// A PAN that isn't 10 letters and digits, left out.
    PanInvalid,
    /// A GST duty ledger used on both sales and purchases (or neither), so
    /// its input or output side was guessed.
    TaxSideGuessed,
    /// A duty ledger whose tax could not be tagged.
    DutyLedgerNotTagged,
    /// Two masters give one HSN/SAC code different rates.
    TaxRateConflict,
    /// GST rates could not be kept (the company's country isn't set).
    TaxRatesNotKept,
    /// A master takes its GST rate from a GST classification, not read.
    GstClassification,
    /// A detail KhataOS doesn't keep, or one that was too long.
    DetailNotKept,
    /// Ledger lines with cost centre allocations (KhataOS has none yet).
    CostCentres,
    /// Bank reconciliation dates that could not be kept.
    BankDateDropped,
    /// A detail the book refused; the ledger was kept without it.
    DetailRefused,
}

impl NoteKind {
    /// A plain description, without names or amounts.
    pub fn summary(self) -> &'static str {
        match self {
            Self::DayBookStartsLate => "The Day Book starts well after the start of its financial year, so balances may not match Tally",
            Self::NoLedgerMasters => "The files have vouchers but no ledger masters, so no opening balances were imported",
            Self::NoOpeningBalances => "No ledger has an opening balance; check that the books begin with the first voucher",
            Self::OpeningDifference => "Opening balances don't add up to zero",
            Self::BalanceDiffers => "Closing balances that differ from Tally's",
            Self::BalanceNotFound => "Balances in the Tally file that match no ledger or group",
            Self::Renumbered => "Vouchers given a new number because Tally used the number twice",
            Self::YearlyNumbering => "Voucher types whose numbering now restarts each financial year, as in Tally",
            Self::PostDated => "Post-dated vouchers, posted on their dates",
            Self::BillsDropped => "Voucher lines whose bill allocations could not be kept",
            Self::OpeningBills => "Ledgers whose opening bills could not all be kept or don't cover the opening balance",
            Self::GstinInvalid => "GSTINs that fail the GSTIN check, kept with a note",
            Self::StateUnknown => "State names that match no GST state",
            Self::PanInvalid => "PANs that are not valid, left out",
            Self::TaxSideGuessed => "Tax ledgers whose input or output side was guessed",
            Self::DutyLedgerNotTagged => "Tax ledgers that could not be tagged with their tax",
            Self::TaxRateConflict => "HSN or SAC codes given different rates by different masters",
            Self::TaxRatesNotKept => "GST rates not kept because the company's country isn't set",
            Self::GstClassification => "Masters that take their GST rate from a GST classification, which isn't imported",
            Self::DetailNotKept => "Details KhataOS doesn't keep, or that were too long",
            Self::CostCentres => "Ledger lines with cost centre allocations, which aren't imported yet",
            Self::BankDateDropped => "Bank reconciliation dates that could not be kept",
            Self::DetailRefused => "Ledger details the books refused",
        }
    }

    /// Whether it means balances may differ from Tally's.
    pub fn affects_balances(self) -> bool {
        matches!(
            self,
            Self::DayBookStartsLate
                | Self::NoLedgerMasters
                | Self::NoOpeningBalances
                | Self::OpeningDifference
                | Self::BalanceDiffers
        )
    }
}

/// Something imported with a caveat, or a check to look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub kind: NoteKind,
    /// The ledger, voucher or item it is about; empty for a general note.
    pub name: String,
    /// Plain English; may name ledgers and give amounts.
    pub detail: String,
    /// How many things it covers (cost centre lines…); 1 for one item.
    pub count: usize,
}

/// Vouchers of one Tally voucher type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeCount {
    /// The KhataOS type it was posted as; `None` when not imported.
    pub kind: Option<VoucherKind>,
    pub posted: usize,
    /// Posted as cancelled, keeping their numbers.
    pub cancelled: usize,
    pub skipped: usize,
}

/// Bill-wise details imported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BillCounts {
    pub new_ref: usize,
    pub against: usize,
    pub advance: usize,
    pub on_account: usize,
    /// Bills in ledgers' opening balances, and the ledgers that have them.
    pub opening: usize,
    pub opening_ledgers: usize,
}

/// GST and party details imported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GstCounts {
    pub gstins: usize,
    pub states: usize,
    pub pans: usize,
    pub addresses: usize,
    /// Duty ledgers tagged with their tax (CGST, SGST, IGST, cess).
    pub duty_ledgers: usize,
    /// Sales, purchase and service ledgers given an HSN/SAC code.
    pub ledger_classes: usize,
    /// HSN/SAC codes given a rate.
    pub rates: usize,
}

/// Closing balances compared with the ones Tally gave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BalanceCheck {
    pub compared: usize,
    pub matched: usize,
    pub differences: Vec<BalanceDifference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceDifference {
    /// The ledger or group.
    pub name: String,
    /// Debit positive.
    pub tally: Amount,
    pub khataos: Amount,
}

fn kind_label(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Group => "Group",
        ItemKind::Ledger => "Ledger",
        ItemKind::Voucher => "Voucher",
        ItemKind::Unit => "Unit",
        ItemKind::StockGroup => "Stock group",
        ItemKind::StockItem => "Stock item",
        ItemKind::Godown => "Godown",
        ItemKind::Inventory => "Inventory",
    }
}

/// KhataOS's name for a voucher type.
pub fn voucher_kind_label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Contra => "Contra",
        VoucherKind::Payment => "Payment",
        VoucherKind::Receipt => "Receipt",
        VoucherKind::Journal => "Journal",
        VoucherKind::Sales => "Sales",
        VoucherKind::Purchase => "Purchase",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        VoucherKind::StockJournal => "Stock Journal",
    }
}

fn amount(a: Amount) -> String {
    let side = if a < Amount::ZERO { "Cr" } else { "Dr" };
    format!("{:.2} {side}", a.abs())
}

fn date(d: Option<NaiveDate>) -> String {
    d.map_or_else(|| "none".into(), |d| d.format("%d-%b-%Y").to_string())
}

/// One CSV field, quoted when needed (RFC 4180).
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() }
}

impl ImportReport {
    /// Rows of `(section, item, count, name, detail)`: the whole report,
    /// or with `shareable` only sections, items and counts, with no names,
    /// amounts, dates or file names.
    pub fn rows(&self, files: &[String], shareable: bool) -> Vec<[String; 5]> {
        let mut rows: Vec<[String; 5]> = Vec::new();
        let mut row = |section: &str, item: &str, count: usize, name: &str, detail: &str| {
            let (name, detail) = if shareable { ("", "") } else { (name, detail) };
            rows.push([section.into(), item.into(), count.to_string(), name.into(), detail.into()]);
        };
        row("Files", "Files imported", files.len(), "", &if shareable { String::new() } else { files.join(", ") });
        let counts = [
            ("Groups created", self.groups_created),
            ("Groups matched to the default chart", self.groups_matched),
            ("Ledgers created", self.ledgers_created),
            ("Opening balances set", self.opening_balances),
            ("Units created", self.units_created),
            ("Stock groups created", self.stock_groups_created),
            ("Stock items created", self.stock_items_created),
            ("Godowns created", self.godowns_created),
        ];
        for (item, n) in counts {
            row("Masters", item, n, "", "");
        }
        row("Vouchers", "Vouchers posted", self.vouchers_posted, "", "");
        row("Vouchers", "Posted as cancelled, keeping their numbers", self.vouchers_cancelled, "", "");
        row("Vouchers", "Numbered by KhataOS (no number in Tally)", self.vouchers_numbered, "", "");
        row("Vouchers", "Stock lines on vouchers", self.inventory_lines, "", "");
        // Tally's type names are the company's own; the shareable summary
        // groups them by the KhataOS type instead.
        if shareable {
            let mut by_kind: BTreeMap<&str, TypeCount> = BTreeMap::new();
            for t in self.by_type.values() {
                let slot = by_kind.entry(t.kind.map_or("Not imported", voucher_kind_label)).or_default();
                slot.posted += t.posted;
                slot.cancelled += t.cancelled;
                slot.skipped += t.skipped;
            }
            for (kind, t) in by_kind {
                row("Vouchers by type", &format!("{kind}: posted"), t.posted, "", "");
                row("Vouchers by type", &format!("{kind}: cancelled"), t.cancelled, "", "");
                row("Vouchers by type", &format!("{kind}: skipped"), t.skipped, "", "");
            }
            row("Vouchers by type", "Tally voucher types", self.by_type.len(), "", "");
        } else {
            for (name, t) in &self.by_type {
                let kind = t.kind.map_or("not imported", voucher_kind_label);
                let detail = format!("as {kind}: {} posted, {} cancelled, {} skipped", t.posted, t.cancelled, t.skipped);
                row("Vouchers by type", "Tally voucher type", t.posted + t.cancelled + t.skipped, name, &detail);
            }
        }
        let b = &self.bills;
        for (item, n) in [
            ("New bills (New Ref)", b.new_ref),
            ("Settlements (Agst Ref)", b.against),
            ("Advances", b.advance),
            ("On account", b.on_account),
            ("Opening bills", b.opening),
            ("Ledgers with opening bills", b.opening_ledgers),
        ] {
            row("Bills", item, n, "", "");
        }
        let g = &self.gst;
        for (item, n) in [
            ("GSTINs", g.gstins),
            ("States", g.states),
            ("PANs", g.pans),
            ("Addresses", g.addresses),
            ("Tax ledgers tagged", g.duty_ledgers),
            ("Ledgers given an HSN or SAC code", g.ledger_classes),
            ("HSN or SAC codes given a rate", g.rates),
        ] {
            row("GST and party details", item, n, "", "");
        }
        let span = match (self.first_date, self.last_date) {
            (Some(a), Some(b)) => format!("{} to {}", date(Some(a)), date(Some(b))),
            _ => "no vouchers".into(),
        };
        let years = match (self.first_date, self.last_date) {
            (Some(a), Some(b)) => usize::try_from((b - a).num_days()).unwrap_or(0) / 365 + 1,
            _ => 0,
        };
        row("Checks", "Day Book dates (years covered)", years, "", &span);
        let tallies = usize::from(self.opening_difference.is_zero());
        row("Checks", "Opening balances add up to zero (1 = yes)", tallies, "", &amount(self.opening_difference));
        row("Checks", "Closing balances compared with Tally", self.balances.compared, "", "");
        row("Checks", "Closing balances that match", self.balances.matched, "", "");
        row("Checks", "Closing balances that differ", self.balances.differences.len(), "", "");
        for d in &self.balances.differences {
            let detail = format!("Tally {}, KhataOS {}, difference {}", amount(d.tally), amount(d.khataos), amount(d.khataos - d.tally));
            row("Balance differences", "Differs", 1, &d.name, &detail);
        }
        let mut by_note: BTreeMap<NoteKind, usize> = BTreeMap::new();
        for n in &self.notes {
            *by_note.entry(n.kind).or_default() += n.count;
        }
        if shareable {
            for (kind, n) in by_note {
                let section = if kind.affects_balances() { "Warnings" } else { "Notes" };
                row(section, kind.summary(), n, "", "");
            }
        } else {
            for n in &self.notes {
                let section = if n.kind.affects_balances() { "Warnings" } else { "Notes" };
                row(section, n.kind.summary(), n.count, &n.name, &n.detail);
            }
        }
        let mut by_reason: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for s in &self.skipped {
            *by_reason.entry((kind_label(s.kind), s.reason.summary())).or_default() += 1;
        }
        if shareable {
            for ((kind, reason), n) in by_reason {
                row("Skipped", &format!("{kind}: {reason}"), n, "", "");
            }
        } else {
            for s in &self.skipped {
                row("Skipped", kind_label(s.kind), 1, &s.name, &s.reason.to_string());
            }
        }
        row("Skipped", "Skipped in all", self.skipped.len(), "", "");
        rows
    }

    /// The report as CSV, with a header row.
    pub fn to_csv(&self, files: &[String], shareable: bool) -> String {
        let mut out = String::from("Section,Item,Count,Name,Detail\r\n");
        for r in self.rows(files, shareable) {
            out.push_str(&r.iter().map(|c| field(c)).collect::<Vec<_>>().join(","));
            out.push_str("\r\n");
        }
        out
    }

    /// The report as plain text, one section after another.
    pub fn to_text(&self, files: &[String], shareable: bool) -> String {
        let mut out = String::from(if shareable {
            "KhataOS: Tally import summary (counts only, no names or amounts)\n"
        } else {
            "KhataOS: Tally import report\n"
        });
        let mut section = String::new();
        for [s, item, count, name, detail] in self.rows(files, shareable) {
            if s != section {
                let _ = write!(out, "\n{s}\n");
                section = s;
            }
            let mut line = format!("  {item}: {count}");
            if !name.is_empty() {
                let _ = write!(line, " | {name}");
            }
            if !detail.is_empty() {
                let _ = write!(line, " | {detail}");
            }
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

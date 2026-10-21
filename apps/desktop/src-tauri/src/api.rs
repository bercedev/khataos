//! The command layer: plain functions over the open company.
//!
//! Every Tauri command in [`crate::commands`] is a thin wrapper around a
//! function here, so the behaviour is unit-tested without a webview.
//! Money crosses the boundary as decimal strings, never as floats.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use khata_compliance::{CompliancePack, IssuedDocument};
use khata_core::reports::{self, BalanceSheet, DayBookRow, LedgerStatement, ProfitAndLoss};
use khata_core::{
    Amount, BillAllocation, BillKind, Book, DrCr, Entry, Group, GroupId, Ledger, LedgerId, Nature,
    GodownId, NumberSeries, Restart, StockItemId, StockMove, UserRole, StockLine, Voucher, VoucherId, VoucherKind,
};
use khata_keymap::{Action, Context, Keymap};
use khata_store::Store;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The open company, if any. Held in Tauri state behind a mutex.
pub type Slot = Option<(Store, Book)>;

/// Why a company won't open: another KhataOS holds its [`CompanyLock`].
pub const COMPANY_IN_USE: &str = "This company is open in another KhataOS window or on another user account on this computer. Close it there first.";

/// An exclusive advisory lock on `<company>.khata.lock`, held while the
/// company is open (beside the [`Slot`], in `AppState::company_lock`), so a
/// second KhataOS can't load its own copy of the books and diverge.
///
/// The lock is on a sidecar file, never on the SQLite file: on Windows that
/// would block SQLite's own byte-range locks. The OS releases it when the
/// process ends, crash included, so a lock file left behind never blocks;
/// only a live lock does. The file is not deleted on release (deleting a
/// lock file races with a process about to lock it).
#[derive(Debug)]
pub struct CompanyLock {
    company: PathBuf,
    /// `None` where no lock can be taken (a read-only folder, a file system
    /// without locks): the company opens unguarded, as before.
    _file: Option<std::fs::File>,
}

impl CompanyLock {
    /// The sidecar lock file for `company`.
    pub fn file_for(company: &Path) -> PathBuf {
        let mut p = company.as_os_str().to_owned();
        p.push(".lock");
        PathBuf::from(p)
    }

    /// Lock `company`, or fail with [`COMPANY_IN_USE`] while another holds it.
    pub fn acquire(company: &Path) -> ApiResult<Self> {
        let at = Self::file_for(company);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&at);
        let file = match file {
            // fs4 (flock on Unix, Android and iOS; LockFileEx on Windows):
            // std's `File::try_lock` is newer than the workspace's rust-version.
            Ok(f) => match fs4::FileExt::try_lock(&f) {
                Ok(()) => Some(f),
                Err(fs4::TryLockError::WouldBlock) => return Err(COMPANY_IN_USE.to_owned()),
                Err(fs4::TryLockError::Error(e)) => {
                    eprintln!("cannot lock {}: {e}; opening without a lock", at.display());
                    None
                }
            },
            Err(e) => {
                eprintln!("cannot create {}: {e}; opening without a lock", at.display());
                None
            }
        };
        Ok(Self { company: canonical(company), _file: file })
    }

    /// The company file this lock is for.
    pub fn company(&self) -> &Path {
        &self.company
    }

    /// The lock to take before opening `path` in a window holding the lock
    /// for `open`: none (keep the one held) when it is the same file.
    pub fn for_switch(open: Option<&Path>, path: &Path) -> ApiResult<Option<Self>> {
        if open.is_some_and(|o| canonical(o) == canonical(path)) {
            return Ok(None);
        }
        Self::acquire(path).map(Some)
    }
}

/// `path` with its folder resolved (the file itself may not exist yet).
pub(crate) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        match (path.parent().and_then(|d| d.canonicalize().ok()), path.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => path.to_owned(),
        }
    })
}

pub type ApiResult<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

const NO_COMPANY: &str = "no company is open";

/// The open company for reading; a company with users needs a sign-in.
pub(crate) fn open_ref(slot: &Slot) -> ApiResult<&Book> {
    let book = open_ref_unchecked(slot)?;
    crate::security::require(book, UserRole::Auditor)?;
    Ok(book)
}

/// The open company without the sign-in check (company info, sign-in itself).
pub(crate) fn open_ref_unchecked(slot: &Slot) -> ApiResult<&Book> {
    slot.as_ref()
        .map(|(_, b)| b)
        .ok_or_else(|| NO_COMPANY.to_owned())
}

/// The open company for a change: creating vouchers and masters needs at
/// least Data Entry; actions needing more check again with [`require`].
pub(crate) fn open_mut(slot: &mut Slot) -> ApiResult<(&mut Store, &mut Book)> {
    let (store, book) = open_mut_unchecked(slot)?;
    crate::security::require(book, UserRole::DataEntry)?;
    Ok((store, book))
}

/// [`open_mut`] without the role check (managing users).
pub(crate) fn open_mut_unchecked(slot: &mut Slot) -> ApiResult<(&mut Store, &mut Book)> {
    let (store, book) = slot
        .as_mut()
        .map(|(s, b)| (s, b))
        .ok_or_else(|| NO_COMPANY.to_owned())?;
    // Every change goes through here: a company shared as Viewer is read-only.
    crate::lan::check_writable(store)?;
    Ok((store, book))
}

/// Check the signed-in user has at least `role` for this company.
pub(crate) fn require(book: &Book, role: UserRole) -> ApiResult<()> {
    crate::security::require(book, role)
}

/// The OS user name, recorded as the audit-trail `user`.
pub fn os_user() -> String {
    if let Some(name) = crate::security::signed_in_name() {
        return name;
    }
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "user".to_owned())
}

pub(crate) fn now() -> DateTime<Utc> {
    Utc::now()
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Start of the financial year containing `d` (1 April, the Indian default),
/// for companies whose country has not been picked yet.
pub fn fy_start(d: NaiveDate) -> NaiveDate {
    let year = if d.month() >= 4 {
        d.year()
    } else {
        d.year() - 1
    };
    NaiveDate::from_ymd_opt(year, 4, 1).expect("1 April exists")
}

pub fn parse_date(s: &str) -> ApiResult<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| format!("invalid date {s:?} (expected YYYY-MM-DD)"))
}

/// Parse a decimal string such as "2,50,000.00" or "-1200.5". Empty is zero.
pub fn parse_amount(s: &str) -> ApiResult<Amount> {
    let clean: String = s
        .trim()
        .chars()
        .filter(|c| *c != ',' && *c != '_')
        .collect();
    if clean.is_empty() {
        return Ok(Amount::ZERO);
    }
    if clean.contains(['e', 'E']) {
        return Err(format!("invalid amount {s:?}"));
    }
    Amount::from_str(&clean).map_err(|_| format!("invalid amount {s:?}"))
}

pub(crate) fn parse_uuid(s: &str, what: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(s.trim()).map_err(|_| format!("invalid {what} id {s:?}"))
}

fn parse_kind(s: &str) -> ApiResult<VoucherKind> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|_| format!("unknown voucher type {s:?}"))
}

pub(crate) fn amt(a: Amount) -> String {
    a.normalize().to_string()
}

// ---------------------------------------------------------------- DTOs

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompanyInfo {
    pub name: String,
    pub path: String,
    pub user: String,
    pub today: NaiveDate,
    pub fy_start: NaiveDate,
    /// The country pack in force; `None` until the country is picked (C2).
    pub country: Option<CountryDto>,
    pub ledger_count: usize,
    pub voucher_count: usize,
    /// Sum of opening balances, debit positive; non-zero means they do not tally.
    pub opening_difference: String,
    /// Result of the last sync attempt on this device, if any.
    pub sync_status: Option<String>,
    /// Shared with this computer as Viewer: nothing can be changed here,
    /// including the country pick.
    pub read_only: bool,
}

/// A country with a compliance pack, for the country pick (rule C2).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CountryDto {
    pub code: &'static str,
    pub name: &'static str,
    pub dialing_code: &'static str,
    pub currency: &'static str,
    pub currency_symbol: &'static str,
    /// What a party's tax registration is called ("GSTIN").
    pub tax_id_label: &'static str,
    /// The financial year containing today, under this country's rules.
    pub fy_start: NaiveDate,
    pub fy_end: NaiveDate,
}

/// The result of checking a tax ID as it is typed (rule M2).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TaxIdCheck {
    /// Normalised (trimmed, upper case).
    pub value: String,
    /// Why it is invalid; `None` when it is valid.
    pub error: Option<String>,
    /// The state or region the ID belongs to.
    pub region: Option<String>,
    /// The entity ID inside it (the PAN in a GSTIN).
    pub entity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompanyFile {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GroupDto {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    /// Name of the parent group, or `None` for a primary group.
    pub parent_name: Option<String>,
    pub primary_name: String,
    pub nature: Nature,
    pub aliases: Vec<String>,
    pub is_cash_or_bank: bool,
    pub is_party: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LedgerDto {
    pub id: String,
    pub name: String,
    pub group_id: String,
    pub group_name: String,
    pub nature: Nature,
    pub is_cash_or_bank: bool,
    /// Under Cash-in-Hand (as opposed to a bank), found by id so a rename is safe.
    pub is_cash: bool,
    pub is_party: bool,
    /// Opening balance, debit positive.
    pub opening: String,
    /// Current closing balance (all vouchers), debit positive.
    pub balance: String,
    /// Default credit period for a party's new bills.
    pub credit_days: Option<u32>,
    /// The party's tax registration (GSTIN…).
    pub tax_id: Option<String>,
    /// Why an invalid tax ID was kept (rule M2), if it was.
    pub tax_id_override: Option<String>,
    /// For a duty ledger: the tax it collects ("CGST") and on which side.
    pub tax_component: Option<String>,
    pub tax_side: Option<khata_core::TaxSide>,
    /// Postal and contact details (parties).
    pub mailing: Option<khata_core::Mailing>,
    /// HSN/SAC or tax category, for sales and service ledgers.
    pub tax_class: Option<String>,
    /// The party's PAN (India), as stored.
    pub pan: Option<String>,
    /// TDS settings: a nature of payment / deductee, or a TDS payable tag.
    pub tds: Option<khata_core::LedgerTds>,
    /// TCS settings: a nature of collection / collectee's terms, or a TCS payable tag.
    pub tcs: Option<khata_core::chart::LedgerTcs>,
    /// A party or bank ledger kept in a foreign currency (ISO code).
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreatedLedger {
    pub ledger: LedgerDto,
    pub opening_difference: String,
}

/// One bill-wise allocation of a party line (New Ref, Agst Ref…).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BillInput {
    pub kind: BillKind,
    #[serde(default)]
    pub reference: String,
    pub amount: String,
    #[serde(default)]
    pub due: Option<String>,
}

/// One stock item on a line (item invoice): quantity × rate = amount.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockLineInput {
    pub item_id: String,
    pub qty: String,
    pub rate: String,
    /// Defaults to quantity × rate, rounded to paise.
    #[serde(default)]
    pub amount: String,
    /// Empty: main location.
    #[serde(default)]
    pub godown_id: String,
    #[serde(default)]
    pub batch: String,
}

/// One Stock Journal line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockMoveInput {
    /// Into stock (produced, transferred to); out of stock otherwise.
    pub inward: bool,
    #[serde(flatten)]
    pub line: StockLineInput,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntryInput {
    pub ledger_id: String,
    pub side: DrCr,
    pub amount: String,
    #[serde(default)]
    pub bills: Vec<BillInput>,
    #[serde(default)]
    pub items: Vec<StockLineInput>,
    /// The line in a foreign currency; `amount` is still the base amount.
    #[serde(default)]
    pub forex: Option<crate::forex::ForexInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VoucherInput {
    pub kind: String,
    pub date: String,
    #[serde(default)]
    pub number: Option<String>,
    pub entries: Vec<EntryInput>,
    /// Stock Journal lines (stock out of / into a location).
    #[serde(default)]
    pub stock: Vec<StockMoveInput>,
    #[serde(default)]
    pub narration: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PostedVoucher {
    pub id: String,
    pub kind: VoucherKind,
    pub number: String,
    pub next_number: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoucherEntryView {
    pub ledger_id: String,
    pub ledger_name: String,
    pub side: DrCr,
    pub amount: String,
    pub bills: Vec<BillInput>,
    pub items: Vec<StockLineView>,
    /// Bank reconciliation date, for bank ledgers.
    pub bank_date: Option<NaiveDate>,
    /// Foreign amount and rate of exchange, if the line has them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forex: Option<crate::forex::ForexView>,
    /// The realised forex gain/loss line, worked out again on saving.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub forex_gain: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockLineView {
    pub item_id: String,
    pub item_name: String,
    pub unit: String,
    pub qty: String,
    pub rate: String,
    pub amount: String,
    pub godown_id: Option<String>,
    pub godown_name: Option<String>,
    pub batch: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoucherView {
    pub id: String,
    pub kind: VoucherKind,
    pub number: String,
    pub date: NaiveDate,
    pub narration: String,
    pub entries: Vec<VoucherEntryView>,
    pub total: String,
    /// Cancelled vouchers keep their number and lines but count for nothing.
    pub cancelled: bool,
    /// Stock Journal lines.
    pub stock: Vec<StockMoveView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StockMoveView {
    pub inward: bool,
    #[serde(flatten)]
    pub line: StockLineView,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrialBalanceLine {
    pub ledger_id: String,
    pub name: String,
    pub group_name: String,
    pub debit: String,
    pub credit: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrialBalanceDto {
    pub as_at: Option<NaiveDate>,
    pub rows: Vec<TrialBalanceLine>,
    pub total_debit: String,
    pub total_credit: String,
    /// Opening balances that do not tally, debit positive.
    pub opening_difference: String,
    /// Stock items' opening value: a debit of its own (no ledger), so the
    /// Trial Balance agrees with the opening balances.
    pub opening_stock: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LedgerStatementDto {
    pub ledger_name: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(flatten)]
    pub statement: LedgerStatement,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GroupSummaryLine {
    pub ledger_id: String,
    pub name: String,
    pub group_name: String,
    /// Debit positive.
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GroupSummaryDto {
    pub group_name: String,
    pub nature: Nature,
    pub rows: Vec<GroupSummaryLine>,
    pub total: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BindingDto {
    /// Same text as `khata_keymap::Chord`'s `Display`, e.g. "Ctrl+Alt+U".
    pub chord: String,
    pub context: Context,
    /// Flattened action id: "Accept", "Menu:DayBook", "OpenVoucher:Sales".
    pub action: String,
}

// ------------------------------------------------------------ helpers

pub(crate) fn ancestors<'a>(book: &'a Book, mut id: Option<GroupId>) -> impl Iterator<Item = &'a Group> + 'a {
    std::iter::from_fn(move || {
        let g = book.group(id?)?;
        id = g.parent;
        Some(g)
    })
}

pub(crate) fn group_is_party(book: &Book, id: GroupId) -> bool {
    // By id, not name: the default groups can be renamed (Alter).
    let parties = [GroupId::default_named("Sundry Debtors"), GroupId::default_named("Sundry Creditors")];
    ancestors(book, Some(id)).any(|g| parties.contains(&g.id))
}

pub(crate) fn group_dto(book: &Book, g: &Group) -> GroupDto {
    let parent_name = g.parent.and_then(|p| book.group(p)).map(|p| p.name.clone());
    let primary_name = book
        .primary_group(g.id)
        .map_or_else(|| g.name.clone(), |p| p.name.clone());
    GroupDto {
        id: g.id.0.to_string(),
        name: g.name.clone(),
        parent: g.parent.map(|p| p.0.to_string()),
        parent_name,
        primary_name,
        nature: g.nature,
        aliases: g.aliases.clone(),
        is_cash_or_bank: ancestors(book, Some(g.id)).any(|a| a.is_cash_or_bank),
        is_party: group_is_party(book, g.id),
    }
}

pub(crate) fn ledger_dto(book: &Book, l: &Ledger, balances: &BTreeMap<LedgerId, Amount>) -> LedgerDto {
    let group = book.group(l.group);
    LedgerDto {
        id: l.id.0.to_string(),
        name: l.name.clone(),
        group_id: l.group.0.to_string(),
        group_name: group.map(|g| g.name.clone()).unwrap_or_default(),
        nature: group.map_or(Nature::Assets, |g| g.nature),
        is_cash_or_bank: book.is_cash_or_bank(l.id),
        is_cash: ancestors(book, Some(l.group)).any(|g| g.id == GroupId::default_named("Cash-in-Hand")),
        is_party: group_is_party(book, l.group),
        opening: amt(l.opening),
        balance: amt(balances.get(&l.id).copied().unwrap_or(l.opening)),
        tax_component: l.tax.as_ref().map(|t| t.component.clone()),
        tax_side: l.tax.as_ref().map(|t| t.side),
        mailing: l.mailing.clone(),
        tax_class: l.tax_class.clone(),
        credit_days: l.credit_days,
        tax_id: l.tax_id.clone(),
        tax_id_override: l.tax_id_override.clone(),
        pan: l.pan.clone(),
        tds: l.tds.clone(),
        tcs: l.tcs.clone(),
        currency: l.currency.clone(),
    }
}

// --------------------------------------------------------- compliance

/// Every compliance pack this build ships.
/// Every country pack this build ships (see `packs.rs`).
pub fn packs() -> impl Iterator<Item = &'static dyn CompliancePack> {
    crate::packs::PACKS.iter().copied()
}

/// The pack for the company's country, once it is picked.
pub fn pack(book: &Book) -> Option<&'static dyn CompliancePack> {
    let code = book.country()?;
    packs().find(|p| p.country().code == code)
}

fn country_dto(p: &dyn CompliancePack) -> CountryDto {
    let (country, currency, fy) = (p.country(), p.currency(), p.financial_year(today()));
    CountryDto {
        code: country.code,
        name: country.name,
        dialing_code: country.dialing_code,
        currency: currency.code,
        currency_symbol: currency.symbol,
        tax_id_label: p.tax_id_label(),
        fy_start: fy.start,
        fy_end: fy.end,
    }
}

/// The countries a company can be set up for (rule C2).
pub fn list_countries() -> Vec<CountryDto> {
    packs().map(country_dto).collect()
}

/// Pick the company's country. The choice is in the audit trail, so it
/// syncs; the app sets it once, on first open (C2).
pub fn set_country(slot: &mut Slot, path: Option<&Path>, code: &str) -> ApiResult<CompanyInfo> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Owner)?;
    let code = code.trim().to_ascii_uppercase();
    let chosen = packs()
        .into_iter()
        .find(|p| p.country().code == code)
        .ok_or_else(|| format!("there is no country pack for {code:?}"))?;
    if let Some(current) = pack(book) {
        return Err(format!(
            "this company is already set up for {}",
            current.country().name
        ));
    }
    let (at, user) = (now(), os_user());
    book.set_country(at, &user, chosen.country().code)
        .map_err(err)?;
    book.set_fy_start_month(at, &user, chosen.financial_year(today()).start.month())
        .map_err(err)?;
    // A new company gets a fresh series every financial year (what invoice
    // rules ask for and Tally recommends). Invoices and notes carry the year
    // ("26-27/1") so bill references stay unique across years.
    if book.vouchers().next().is_none() {
        for kind in [
            VoucherKind::Contra,
            VoucherKind::Payment,
            VoucherKind::Receipt,
            VoucherKind::Journal,
            VoucherKind::Purchase,
        ] {
            let yearly = NumberSeries { restart: Restart::Yearly, ..NumberSeries::default() };
            book.set_numbering(at, &user, kind, yearly).map_err(err)?;
        }
        for kind in [VoucherKind::Sales, VoucherKind::CreditNote, VoucherKind::DebitNote] {
            book.set_numbering(at, &user, kind, NumberSeries::yearly_with_year())
                .map_err(err)?;
        }
    }
    store.save(book).map_err(err)?;
    Ok(info(store, book, path.unwrap_or(Path::new(""))))
}

/// Check a tax ID against the company's pack while it is typed.
pub fn check_tax_id(slot: &Slot, value: &str) -> ApiResult<TaxIdCheck> {
    let pack = pack(open_ref(slot)?).ok_or(NO_COUNTRY)?;
    Ok(match pack.validate_tax_id(value) {
        Ok(d) => TaxIdCheck {
            value: d.value,
            error: None,
            region: d.region.map(|r| r.name.to_owned()),
            entity_id: d.entity_id,
        },
        Err(e) => TaxIdCheck {
            value: value.trim().to_ascii_uppercase(),
            error: Some(format!("{} is invalid: {e}", pack.tax_id_label())),
            region: None,
            entity_id: None,
        },
    })
}

const NO_COUNTRY: &str = "pick the company's country first";

/// The tax ID to store on a new party ledger, with the override reason if
/// it failed validation. Rule M2: an invalid ID blocks the save unless a
/// reason is given, and the reason is kept in the ledger, so it is in the
/// audit trail with its author. Rule M3: only parties carry one.
pub(crate) fn party_tax_id(
    book: &Book,
    group: GroupId,
    tax_id: Option<&str>,
    reason: Option<&str>,
) -> ApiResult<Option<(String, Option<String>)>> {
    let Some(raw) = tax_id.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if !group_is_party(book, group) {
        return Err(
            "only party ledgers (Sundry Debtors and Sundry Creditors) have a tax registration"
                .into(),
        );
    }
    let pack = pack(book).ok_or(NO_COUNTRY)?;
    match pack.validate_tax_id(raw) {
        Ok(d) => Ok(Some((d.value, None))),
        Err(e) => match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) => {
                // Keeping an invalid ID is an accountant's call (rule M2).
                require(book, UserRole::Accountant)?;
                Ok(Some((raw.to_ascii_uppercase(), Some(r.to_owned()))))
            }
            None => Err(format!(
                "{} {raw} is invalid: {e}. Correct it, or give a reason to keep it.",
                pack.tax_id_label()
            )),
        },
    }
}

/// Sales invoice numbers must follow the pack's rule (India: CGST Rule
/// 46(b), at most 16 characters of letters, digits, `-` and `/`, unique in
/// the financial year). `previous` is the number and date before an
/// alteration: an unchanged number in the same year is not re-checked, so
/// older or imported invoices can still be corrected.
fn check_invoice_number(
    book: &Book,
    v: &Voucher,
    previous: Option<(&str, NaiveDate)>,
) -> ApiResult<()> {
    let Some(pack) = pack(book).filter(|_| v.kind == VoucherKind::Sales) else {
        return Ok(());
    };
    if previous.is_some_and(|(number, date)| {
        number == v.number && pack.financial_year(date) == pack.financial_year(v.date)
    }) {
        return Ok(());
    }
    // Cancelled invoices keep their numbers, so they count as issued. Packs
    // only look for a clash, so pass just the invoices whose number could
    // clash (same number ignoring case and punctuation, in any year) instead
    // of every invoice.
    let issued: Vec<IssuedDocument<'_>> = book
        .vouchers_numbered(v.kind, &v.number)
        .filter(|o| o.id != v.id)
        .map(|o| IssuedDocument {
            number: &o.number,
            date: o.date,
        })
        .collect();
    pack.validate_invoice_number(&v.number, v.date, &issued)
        .map_err(err)
}

fn info(store: &Store, book: &Book, path: &Path) -> CompanyInfo {
    let t = today();
    let pack = pack(book);
    CompanyInfo {
        name: book.name.clone(),
        path: path.display().to_string(),
        user: os_user(),
        today: t,
        fy_start: pack.map_or_else(|| fy_start(t), |p| p.financial_year(t).start),
        country: pack.map(country_dto),
        ledger_count: book.ledgers().count(),
        voucher_count: book.vouchers().count(),
        opening_difference: amt(book.opening_difference()),
        sync_status: store.setting(crate::sync::LAST).ok().flatten(),
        read_only: crate::lan::check_writable(store).is_err(),
    }
}

// ----------------------------------------------------------- commands

pub fn create_company(slot: &mut Slot, path: &Path, name: &str) -> ApiResult<CompanyInfo> {
    let name = name.trim();
    if name.is_empty() {
        return Err("company name is required".into());
    }
    if path.exists() {
        return Err(format!("a file already exists at {}", path.display()));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    crate::security::sign_out();
    let mut store = Store::create(path, name).map_err(err)?;
    let book = Book::new(name);
    store.save(&book).map_err(err)?;
    let out = info(&store, &book, path);
    *slot = Some((store, book));
    Ok(out)
}

pub fn open_company(slot: &mut Slot, path: &Path) -> ApiResult<CompanyInfo> {
    if !path.is_file() {
        return Err(format!("no company file at {}", path.display()));
    }
    let store = Store::open(path).map_err(err)?;
    let book = store.load().map_err(err)?;
    crate::security::sign_out();
    let out = info(&store, &book, path);
    *slot = Some((store, book));
    Ok(out)
}

/// [`create_company`] for a window: the new company's [`CompanyLock`]
/// replaces `held`, releasing the previous company's.
pub fn create_company_locked(
    slot: &mut Slot,
    held: &mut Option<CompanyLock>,
    path: &Path,
    name: &str,
) -> ApiResult<CompanyInfo> {
    let out = create_company(slot, path, name)?;
    // A brand-new file: only another KhataOS creating it this instant holds it.
    match CompanyLock::acquire(path) {
        Ok(lock) => {
            *held = Some(lock);
            Ok(out)
        }
        Err(e) => {
            *slot = None;
            *held = None;
            Err(e)
        }
    }
}

/// [`open_company`] for a window: takes the company's [`CompanyLock`] first
/// (refused while another KhataOS holds it), then releases the previous
/// company's. Reopening the open company keeps its lock.
pub fn open_company_locked(
    slot: &mut Slot,
    held: &mut Option<CompanyLock>,
    path: &Path,
) -> ApiResult<CompanyInfo> {
    if !path.is_file() {
        return Err(format!("no company file at {}", path.display()));
    }
    let lock = CompanyLock::for_switch(held.as_ref().map(CompanyLock::company), path)?;
    let out = open_company(slot, path)?;
    if let Some(lock) = lock {
        *held = Some(lock);
    }
    Ok(out)
}

pub fn company_info(slot: &Slot, path: Option<&Path>) -> ApiResult<CompanyInfo> {
    let (store, book) = slot.as_ref().ok_or(NO_COMPANY)?;
    Ok(info(store, book, path.unwrap_or(Path::new(""))))
}

/// KhataOS company files (`*.khata`) in a directory, with their names.
pub fn list_company_files(dir: &Path) -> Vec<CompanyFile> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<CompanyFile> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "khata") && p.is_file())
        .filter_map(|p| {
            let name = Store::open(&p).ok()?.company_name().ok()?;
            Some(CompanyFile {
                path: p.display().to_string(),
                name,
            })
        })
        .collect();
    out.sort_by_key(|x| x.name.to_lowercase());
    out
}

/// A file name for a new company: "Sharma & Sons" → "sharma-sons.khata".
pub fn company_file_name(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    format!("{}.khata", if slug.is_empty() { "company" } else { slug })
}

pub fn search_groups(slot: &Slot, query: &str) -> ApiResult<Vec<GroupDto>> {
    let book = open_ref(slot)?;
    Ok(book
        .search_groups(query.trim())
        .into_iter()
        .map(|g| group_dto(book, g))
        .collect())
}

/// Ledgers whose name contains `query`, filtered for the field they fill:
/// `cash_bank` (Contra, payment accounts), `party` (Sales/Purchase party:
/// cash, bank and debtor/creditor ledgers — rule V8), `non_cash_bank`, or `all`.
pub fn list_ledgers(slot: &Slot, query: &str, filter: &str) -> ApiResult<Vec<LedgerDto>> {
    let book = open_ref(slot)?;
    let q = query.trim().to_lowercase();
    let balances = book.balances(None);
    let keep = |l: &Ledger| -> ApiResult<bool> {
        let cb = book.is_cash_or_bank(l.id);
        Ok(match filter {
            "" | "all" => true,
            "cash_bank" => cb,
            "non_cash_bank" => !cb,
            "party" => cb || group_is_party(book, l.group),
            other => return Err(format!("unknown ledger filter {other:?}")),
        })
    };
    let mut out = Vec::new();
    for l in book.ledgers() {
        if l.name.to_lowercase().contains(&q) && keep(l)? {
            out.push(ledger_dto(book, l, &balances));
        }
    }
    out.sort_by_key(|x| x.name.to_lowercase());
    Ok(out)
}

pub fn create_ledger(
    slot: &mut Slot,
    name: &str,
    group_id: &str,
    opening: &str,
) -> ApiResult<CreatedLedger> {
    create_ledger_with(slot, name, group_id, opening, None)
}

/// Create a ledger; `credit_days` applies to party ledgers only (rule Ma1).
pub fn create_ledger_with(
    slot: &mut Slot,
    name: &str,
    group_id: &str,
    opening: &str,
    credit_days: Option<u32>,
) -> ApiResult<CreatedLedger> {
    create_party_ledger(slot, name, group_id, opening, credit_days, None, None)
}

/// Create a ledger with party details: credit period and tax registration.
/// An invalid tax ID is refused unless `tax_id_override` gives a reason
/// (rule M2); the reason is stored with it.
pub fn create_party_ledger(
    slot: &mut Slot,
    name: &str,
    group_id: &str,
    opening: &str,
    credit_days: Option<u32>,
    tax_id: Option<&str>,
    tax_id_override: Option<&str>,
) -> ApiResult<CreatedLedger> {
    let (store, book) = open_mut(slot)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("ledger name is required".into());
    }
    let group = GroupId(parse_uuid(group_id, "group")?);
    let opening = parse_amount(opening)?;
    let tax = party_tax_id(book, group, tax_id, tax_id_override)?;
    let user = os_user();
    let at = now();
    let id = book.create_ledger(at, &user, name, group).map_err(err)?;
    if !opening.is_zero() {
        book.set_opening_balance(at, &user, id, opening)
            .map_err(err)?;
    }
    if let Some(days) = credit_days.filter(|_| group_is_party(book, group)) {
        book.set_credit_days(at, &user, id, Some(days))
            .map_err(err)?;
    }
    if let Some((tax_id, reason)) = tax {
        book.set_tax_id(at, &user, id, Some(tax_id), reason)
            .map_err(err)?;
    }
    store.save(book).map_err(err)?;
    let balances = book.balances(None);
    let ledger = book.ledger(id).expect("just created");
    Ok(CreatedLedger {
        ledger: ledger_dto(book, ledger, &balances),
        opening_difference: amt(book.opening_difference()),
    })
}

/// Next number in the type's series for a voucher dated `date` (today when empty).
pub fn next_voucher_number(slot: &Slot, kind: &str, date: Option<&str>) -> ApiResult<String> {
    let date = match date.filter(|d| !d.trim().is_empty()) {
        Some(d) => parse_date(d)?,
        None => today(),
    };
    Ok(open_ref(slot)?.next_voucher_number(parse_kind(kind)?, date))
}

/// A stock line from frontend input; the amount defaults to qty × rate.
fn stock_line(l: &StockLineInput) -> ApiResult<StockLine> {
    let item = StockItemId(parse_uuid(&l.item_id, "stock item")?);
    let qty = parse_amount(&l.qty)?;
    let rate = parse_amount(&l.rate)?;
    let amount = if l.amount.trim().is_empty() { (qty * rate).round_dp(2) } else { parse_amount(&l.amount)? };
    let godown = if l.godown_id.trim().is_empty() { None } else { Some(GodownId(parse_uuid(&l.godown_id, "godown")?)) };
    let batch = Some(l.batch.trim().to_owned()).filter(|b| !b.is_empty());
    Ok(StockLine { item, qty, rate, amount, godown, batch })
}

fn stock_line_view(book: &Book, l: &StockLine) -> StockLineView {
    let item = book.stock_item(l.item);
    StockLineView {
        item_id: l.item.0.to_string(),
        item_name: item.map(|i| i.name.clone()).unwrap_or_default(),
        unit: item.and_then(|i| book.unit(i.unit)).map(|u| u.symbol.clone()).unwrap_or_default(),
        qty: l.qty.normalize().to_string(),
        rate: amt(l.rate),
        amount: amt(l.amount),
        godown_id: l.godown.map(|g| g.0.to_string()),
        godown_name: l.godown.and_then(|g| book.godown(g)).map(|g| g.name.clone()),
        batch: l.batch.clone(),
    }
}

/// Build a voucher from frontend input; `number` defaults to the next free one.
fn build_voucher(book: &Book, id: VoucherId, input: &VoucherInput) -> ApiResult<Voucher> {
    let kind = parse_kind(&input.kind)?;
    let date = parse_date(&input.date)?;
    let number = match input.number.as_deref().map(str::trim) {
        Some(n) if !n.is_empty() => n.to_owned(),
        _ => book.next_voucher_number(kind, date),
    };
    let mut entries = Vec::with_capacity(input.entries.len());
    for (i, e) in input.entries.iter().enumerate() {
        let line = |m: String| format!("line {}: {m}", i + 1);
        let ledger = LedgerId(parse_uuid(&e.ledger_id, "ledger")?);
        let amount = parse_amount(&e.amount).map_err(line)?;
        let mut bills = Vec::with_capacity(e.bills.len());
        for b in &e.bills {
            let due = b
                .due
                .as_deref()
                .filter(|d| !d.trim().is_empty())
                .map(parse_date)
                .transpose()
                .map_err(line)?;
            let reference = b.reference.trim().to_owned();
            if reference.is_empty() && b.kind != BillKind::OnAccount {
                return Err(line("every bill needs a name".into()));
            }
            bills.push(BillAllocation {
                kind: b.kind,
                reference,
                amount: parse_amount(&b.amount).map_err(line)?,
                due,
                forex_amount: None,
            });
        }
        let items = e
            .items
            .iter()
            .map(|l| stock_line(l).map_err(line))
            .collect::<ApiResult<Vec<_>>>()?;
        let forex = e.forex.as_ref().map(|f| crate::forex::entry_forex(book, f, &mut bills).map_err(line)).transpose()?;
        entries.push(Entry {
            ledger,
            side: e.side,
            amount,
            bills,
            items,
            bank_date: None,
            forex,
        });
    }
    let stock = input
        .stock
        .iter()
        .enumerate()
        .map(|(i, m)| {
            stock_line(&m.line).map(|line| StockMove { inward: m.inward, line }).map_err(|e| format!("stock line {}: {e}", i + 1))
        })
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Voucher {
        id,
        kind,
        number,
        date,
        entries,
        narration: input.narration.trim().to_owned(),
        cancelled: false,
        stock,
    })
}

pub fn post_voucher(slot: &mut Slot, input: VoucherInput) -> ApiResult<PostedVoucher> {
    let (store, book) = open_mut(slot)?;
    let mut voucher = build_voucher(book, VoucherId::new(), &input)?;
    crate::forex::settle_voucher(book, &mut voucher)?;
    check_invoice_number(book, &voucher, None)?;
    let (kind, number, date) = (voucher.kind, voucher.number.clone(), voucher.date);
    let id = book.post_voucher(now(), &os_user(), voucher).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(PostedVoucher {
        id: id.0.to_string(),
        kind,
        number,
        next_number: book.next_voucher_number(kind, date),
    })
}

pub fn get_voucher(slot: &Slot, id: &str) -> ApiResult<VoucherView> {
    let book = open_ref(slot)?;
    let v = book
        .voucher(VoucherId(parse_uuid(id, "voucher")?))
        .ok_or("unknown voucher")?;
    let entries = v
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| VoucherEntryView {
            forex: crate::forex::forex_view(e),
            forex_gain: crate::forex::is_gain_line(book, v, i),
            ledger_id: e.ledger.0.to_string(),
            ledger_name: book
                .ledger(e.ledger)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            side: e.side,
            amount: amt(e.amount),
            bills: e
                .bills
                .iter()
                .map(|b| BillInput {
                    kind: b.kind,
                    reference: b.reference.clone(),
                    amount: amt(b.amount),
                    due: b.due.map(|d| d.to_string()),
                })
                .collect(),
            items: e.items.iter().map(|l| stock_line_view(book, l)).collect(),
            bank_date: e.bank_date,
        })
        .collect();
    Ok(VoucherView {
        id: v.id.0.to_string(),
        kind: v.kind,
        number: v.number.clone(),
        date: v.date,
        narration: v.narration.clone(),
        entries,
        total: amt(v.totals().0),
        cancelled: v.cancelled,
        stock: v
            .stock
            .iter()
            .map(|m| StockMoveView { inward: m.inward, line: stock_line_view(book, &m.line) })
            .collect(),
    })
}

/// Cancel a saved voucher: its number stays used (no gaps in a series),
/// its values stop counting (rule V7).
pub fn cancel_voucher(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    book.cancel_voucher(now(), &os_user(), VoucherId(parse_uuid(id, "voucher")?))
        .map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

/// Alter a saved voucher in place (same id); the audit trail keeps both versions.
pub fn alter_voucher(slot: &mut Slot, id: &str, input: VoucherInput) -> ApiResult<PostedVoucher> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    let id = VoucherId(parse_uuid(id, "voucher")?);
    let old = book.voucher(id).ok_or("unknown voucher")?;
    let previous = (old.number.clone(), old.date);
    let input = VoucherInput {
        number: input.number.clone().or_else(|| Some(old.number.clone())),
        ..input
    };
    let bank_dates: BTreeMap<LedgerId, NaiveDate> = old
        .entries
        .iter()
        .filter_map(|e| Some((e.ledger, e.bank_date?)))
        .collect();
    let mut voucher = build_voucher(book, id, &input)?;
    // Altering a voucher keeps its bank reconciliation, unless the bank line
    // was removed or the new date is after the bank date.
    for e in &mut voucher.entries {
        e.bank_date = bank_dates.get(&e.ledger).copied().filter(|d| *d >= voucher.date);
    }
    crate::forex::settle_voucher(book, &mut voucher)?;
    check_invoice_number(book, &voucher, Some((&previous.0, previous.1)))?;
    let (kind, number, date) = (voucher.kind, voucher.number.clone(), voucher.date);
    book.alter_voucher(now(), &os_user(), voucher)
        .map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(PostedVoucher {
        id: id.0.to_string(),
        kind,
        number,
        next_number: book.next_voucher_number(kind, date),
    })
}

/// Delete a saved voucher; the audit trail keeps a full snapshot (rule V7).
pub fn delete_voucher(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    require(book, UserRole::Accountant)?;
    book.delete_voucher(now(), &os_user(), VoucherId(parse_uuid(id, "voucher")?))
        .map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

/// Create a sub-group (it inherits its parent's nature).
pub fn create_group(slot: &mut Slot, name: &str, parent_id: &str) -> ApiResult<GroupDto> {
    let (store, book) = open_mut(slot)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("group name is required".into());
    }
    let parent = GroupId(parse_uuid(parent_id, "group")?);
    let id = book
        .create_group(now(), &os_user(), name, parent)
        .map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(group_dto(book, book.group(id).expect("just created")))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OutstandingDto {
    pub ledger_id: String,
    pub party: String,
    pub reference: String,
    pub date: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    /// Signed: positive = receivable (Dr), negative = payable (Cr).
    pub amount: String,
    pub overdue_days: i64,
    /// The bill in its foreign currency (signed like `amount`), if it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forex_currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forex_amount: Option<String>,
}

fn outstanding_dto(b: reports::OutstandingBill) -> OutstandingDto {
    OutstandingDto {
        ledger_id: b.ledger.0.to_string(),
        party: b.party,
        reference: b.reference,
        date: b.date,
        due: b.due,
        amount: amt(b.amount),
        overdue_days: b.overdue_days,
        forex_currency: b.forex.as_ref().map(|f| f.0.clone()),
        forex_amount: b.forex.as_ref().map(|f| amt(f.1)),
    }
}

/// Open bills as at a date. `side` is "receivable", "payable" or "all";
/// `ledger_id` narrows to one party (the Agst Ref list).
pub fn outstanding(
    slot: &Slot,
    as_at: &str,
    side: &str,
    ledger_id: Option<&str>,
) -> ApiResult<Vec<OutstandingDto>> {
    let book = open_ref(slot)?;
    let ledger = ledger_id
        .map(|l| parse_uuid(l, "ledger").map(LedgerId))
        .transpose()?;
    let keep = |a: Amount| match side {
        "receivable" => a.is_sign_positive(),
        "payable" => a.is_sign_negative(),
        "all" | "" => true,
        _ => false,
    };
    if !matches!(side, "receivable" | "payable" | "all" | "") {
        return Err(format!("unknown side {side:?}"));
    }
    Ok(reports::outstanding_bills(book, parse_date(as_at)?)
        .into_iter()
        .filter(|b| ledger.is_none_or(|l| b.ledger == l) && keep(b.amount))
        .map(outstanding_dto)
        .collect())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgeingDto {
    pub ledger_id: String,
    pub party: String,
    /// 0–30, 31–60, 61–90 and over 90 days, positive.
    pub buckets: [String; 4],
    pub total: String,
}

pub fn ageing(slot: &Slot, as_at: &str, receivable: bool) -> ApiResult<Vec<AgeingDto>> {
    let book = open_ref(slot)?;
    Ok(reports::ageing(book, parse_date(as_at)?, receivable)
        .into_iter()
        .map(|r| AgeingDto {
            ledger_id: r.ledger.0.to_string(),
            party: r.party,
            buckets: r.buckets.map(amt),
            total: amt(r.total),
        })
        .collect())
}

/// Payables open for more than `days` (45 for MSME suppliers in India).
pub fn overdue_payables(slot: &Slot, as_at: &str, days: i64) -> ApiResult<Vec<OutstandingDto>> {
    let book = open_ref(slot)?;
    Ok(reports::overdue_payables(book, parse_date(as_at)?, days)
        .into_iter()
        .map(outstanding_dto)
        .collect())
}

pub fn day_book(slot: &Slot, from: &str, to: &str) -> ApiResult<Vec<DayBookRow>> {
    Ok(reports::day_book(
        open_ref(slot)?,
        parse_date(from)?,
        parse_date(to)?,
    ))
}

pub fn ledger_statement(
    slot: &Slot,
    ledger_id: &str,
    from: &str,
    to: &str,
) -> ApiResult<LedgerStatementDto> {
    let book = open_ref(slot)?;
    let id = LedgerId(parse_uuid(ledger_id, "ledger")?);
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    let statement = reports::ledger_statement(book, id, from, to).ok_or("unknown ledger")?;
    let ledger_name = book.ledger(id).map(|l| l.name.clone()).unwrap_or_default();
    Ok(LedgerStatementDto {
        ledger_name,
        from,
        to,
        statement,
    })
}

/// Closing balance of every ledger as at a date (all vouchers when `None`).
pub fn trial_balance(slot: &Slot, as_at: Option<&str>) -> ApiResult<TrialBalanceDto> {
    let book = open_ref(slot)?;
    let as_at = as_at
        .filter(|s| !s.trim().is_empty())
        .map(parse_date)
        .transpose()?;
    let mut rows: Vec<TrialBalanceLine> = book
        .balances(as_at)
        .into_iter()
        .filter(|(_, bal)| !bal.is_zero())
        .filter_map(|(id, bal)| {
            let l = book.ledger(id)?;
            Some(TrialBalanceLine {
                ledger_id: id.0.to_string(),
                name: l.name.clone(),
                group_name: book
                    .group(l.group)
                    .map(|g| g.name.clone())
                    .unwrap_or_default(),
                debit: amt(bal.max(Amount::ZERO)),
                credit: amt((-bal).max(Amount::ZERO)),
            })
        })
        .collect();
    rows.sort_by_key(|x| x.name.to_lowercase());
    let sum = |f: fn(&TrialBalanceLine) -> &String| -> Amount {
        rows.iter()
            .map(|r| Amount::from_str(f(r)).unwrap_or_default())
            .sum()
    };
    let opening_stock = khata_core::inventory::opening_stock_value(book);
    let (total_debit, total_credit) = (sum(|r| &r.debit) + opening_stock, sum(|r| &r.credit));
    Ok(TrialBalanceDto {
        as_at,
        opening_stock: amt(opening_stock),
        total_debit: amt(total_debit),
        total_credit: amt(total_credit),
        opening_difference: amt(book.opening_difference()),
        rows,
    })
}

pub fn profit_and_loss(slot: &Slot, from: &str, to: &str) -> ApiResult<ProfitAndLoss> {
    Ok(reports::profit_and_loss(
        open_ref(slot)?,
        parse_date(from)?,
        parse_date(to)?,
    ))
}

pub fn balance_sheet(slot: &Slot, as_at: &str) -> ApiResult<BalanceSheet> {
    Ok(reports::balance_sheet(open_ref(slot)?, parse_date(as_at)?))
}

/// Ledgers under a primary group, for drilling down from P&L (movement in
/// `[from, to]`) or Balance Sheet (closing balance as at `to`, `from` empty).
pub fn group_summary(
    slot: &Slot,
    group_id: &str,
    from: Option<&str>,
    to: &str,
) -> ApiResult<GroupSummaryDto> {
    let book = open_ref(slot)?;
    let gid = GroupId(parse_uuid(group_id, "group")?);
    let group = book.group(gid).ok_or("unknown group")?;
    let to = parse_date(to)?;
    let amounts: BTreeMap<LedgerId, Amount> = match from.filter(|s| !s.trim().is_empty()) {
        Some(from) => {
            let from = parse_date(from)?;
            let mut net = BTreeMap::new();
            for v in book
                .active_vouchers()
                .filter(|v| v.date >= from && v.date <= to)
            {
                for e in &v.entries {
                    *net.entry(e.ledger).or_default() += e.signed();
                }
            }
            net
        }
        None => book.balances(Some(to)),
    };
    let mut rows: Vec<GroupSummaryLine> = book
        .ledgers()
        .filter(|l| ancestors(book, Some(l.group)).any(|g| g.id == gid))
        .filter_map(|l| {
            let a = amounts.get(&l.id).copied().unwrap_or_default();
            (!a.is_zero()).then(|| GroupSummaryLine {
                ledger_id: l.id.0.to_string(),
                name: l.name.clone(),
                group_name: book
                    .group(l.group)
                    .map(|g| g.name.clone())
                    .unwrap_or_default(),
                amount: amt(a),
            })
        })
        .collect();
    rows.sort_by_key(|x| x.name.to_lowercase());
    let total = rows
        .iter()
        .map(|r| Amount::from_str(&r.amount).unwrap_or_default())
        .sum();
    Ok(GroupSummaryDto {
        group_name: group.name.clone(),
        nature: group.nature,
        rows,
        total: amt(total),
    })
}

/// Flatten an action to a stable id: "Accept", "Menu:DayBook", "OpenVoucher:Sales".
pub fn action_id(action: &Action) -> String {
    match serde_json::to_value(action).expect("actions serialize") {
        serde_json::Value::String(s) => s,
        serde_json::Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| {
                format!(
                    "{k}:{}",
                    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    }
}

/// The full default binding list, so the frontend resolves keys from the
/// same data as Rust.
pub fn keymap() -> Vec<BindingDto> {
    Keymap::tally_prime()
        .bindings()
        .iter()
        .map(|b| BindingDto {
            chord: b.chord.to_string(),
            context: b.context,
            action: action_id(&b.action),
        })
        .collect()
}

/// Where new company files go by default.
pub fn default_company_dir(app_data: &Path) -> PathBuf {
    app_data.join("companies")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use khata_keymap::Chord;
    use rust_decimal_macros::dec;

    pub(crate) struct TempFile(pub(crate) PathBuf);

    impl TempFile {
        pub(crate) fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("khataos-desktop-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let p = dir.join(format!("{name}.khata"));
            let _ = std::fs::remove_file(&p);
            Self(p)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    pub(crate) fn group_id(slot: &Slot, name: &str) -> String {
        search_groups(slot, name)
            .unwrap()
            .into_iter()
            .find(|g| g.name == name)
            .unwrap()
            .id
    }

    pub(crate) fn ledger(slot: &mut Slot, name: &str, group: &str, opening: &str) -> String {
        let g = group_id(slot, group);
        create_ledger(slot, name, &g, opening).unwrap().ledger.id
    }

    pub(crate) fn entry(ledger_id: &str, side: DrCr, amount: &str) -> EntryInput {
        EntryInput {
            ledger_id: ledger_id.into(),
            side,
            amount: amount.into(),
            bills: Vec::new(),
            items: Vec::new(),
            forex: None,
        }
    }

    pub(crate) fn voucher(kind: &str, date: &str, entries: Vec<EntryInput>) -> VoucherInput {
        VoucherInput {
            kind: kind.into(),
            date: date.into(),
            number: None,
            entries,
            stock: Vec::new(),
            narration: String::new(),
        }
    }

    /// A company with HDFC 2,50,000 Dr against capital, then rent paid and a sale.
    pub(crate) fn demo(file: &TempFile) -> (Slot, String, String, String, String, String) {
        let mut slot = None;
        create_company(&mut slot, &file.0, "Demo Traders").unwrap();
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "250000");
        let capital = ledger(
            &mut slot,
            "Owner's Capital",
            "Capital Account",
            "-2,50,000.00",
        );
        let rent = ledger(&mut slot, "Office Rent", "Indirect Expenses", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "0");
        let party = ledger(&mut slot, "Sharma Enterprises", "Sundry Debtors", "");
        post_voucher(
            &mut slot,
            voucher(
                "Payment",
                "2026-04-01",
                vec![
                    entry(&rent, DrCr::Dr, "25000"),
                    entry(&bank, DrCr::Cr, "25000"),
                ],
            ),
        )
        .unwrap();
        post_voucher(
            &mut slot,
            voucher(
                "Sales",
                "2026-04-02",
                vec![
                    entry(&party, DrCr::Dr, "40,000"),
                    entry(&sales, DrCr::Cr, "40000.00"),
                ],
            ),
        )
        .unwrap();
        (slot, bank, capital, rent, sales, party)
    }

    #[test]
    fn create_then_reopen_round_trips_through_the_file() {
        let f = TempFile::new("roundtrip");
        let (slot, ..) = demo(&f);
        let before = trial_balance(&slot, None).unwrap();
        drop(slot);

        let mut reopened = None;
        let info = open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(info.name, "Demo Traders");
        assert_eq!((info.ledger_count, info.voucher_count), (5, 2));
        assert_eq!(info.opening_difference, "0");
        assert_eq!(trial_balance(&reopened, None).unwrap(), before);
    }

    #[test]
    fn a_company_open_in_one_window_is_refused_in_another_until_closed() {
        let (f, other) = (TempFile::new("lock-a"), TempFile::new("lock-b"));
        let (mut one, mut one_lock): (Slot, _) = (None, None);
        let (mut two, mut two_lock): (Slot, _) = (None, None);
        create_company_locked(&mut one, &mut one_lock, &f.0, "Locked").unwrap();
        assert!(CompanyLock::file_for(&f.0).is_file());

        // A second window (or process) is refused and keeps what it had.
        let refused = open_company_locked(&mut two, &mut two_lock, &f.0).unwrap_err();
        assert_eq!(refused, COMPANY_IN_USE);
        assert!(two.is_none() && two_lock.is_none());
        // Reopening it in the window that holds it keeps the lock.
        open_company_locked(&mut one, &mut one_lock, &f.0).unwrap();
        assert!(open_company_locked(&mut two, &mut two_lock, &f.0).is_err());

        // Closing releases it.
        (one, one_lock) = (None, None);
        assert_eq!(open_company_locked(&mut two, &mut two_lock, &f.0).unwrap().name, "Locked");

        // Switching company releases the old one too.
        create_company(&mut None, &other.0, "Other").unwrap();
        assert!(open_company_locked(&mut one, &mut one_lock, &f.0).is_err());
        open_company_locked(&mut two, &mut two_lock, &other.0).unwrap();
        open_company_locked(&mut one, &mut one_lock, &f.0).unwrap();
        for t in [&f, &other] {
            let _ = std::fs::remove_file(CompanyLock::file_for(&t.0));
        }
    }

    #[test]
    fn a_leftover_lock_file_without_a_live_lock_does_not_block() {
        let f = TempFile::new("lock-stale");
        create_company(&mut None, &f.0, "Stale").unwrap();
        // What a crashed KhataOS leaves behind: the file, but no lock.
        std::fs::write(CompanyLock::file_for(&f.0), b"").unwrap();
        let (mut slot, mut held) = (None, None);
        assert_eq!(open_company_locked(&mut slot, &mut held, &f.0).unwrap().name, "Stale");
        drop((slot, held));
        let (mut slot, mut held) = (None, None);
        open_company_locked(&mut slot, &mut held, &f.0).unwrap();
        let _ = std::fs::remove_file(CompanyLock::file_for(&f.0));
    }

    #[test]
    fn create_refuses_existing_file_and_open_refuses_missing_file() {
        let f = TempFile::new("exists");
        let mut slot = None;
        create_company(&mut slot, &f.0, "One").unwrap();
        assert!(
            create_company(&mut slot, &f.0, "Two")
                .unwrap_err()
                .contains("already exists")
        );
        let missing = TempFile::new("missing");
        assert!(
            open_company(&mut slot, &missing.0)
                .unwrap_err()
                .contains("no company file")
        );
        assert!(!missing.0.exists(), "opening must not create a file");
        assert!(create_company(&mut None, &TempFile::new("blank").0, "  ").is_err());
    }

    #[test]
    fn commands_need_an_open_company() {
        let slot: Slot = None;
        assert_eq!(search_groups(&slot, "").unwrap_err(), NO_COMPANY);
        assert_eq!(
            day_book(&slot, "2026-04-01", "2026-04-30").unwrap_err(),
            NO_COMPANY
        );
    }

    #[test]
    fn opening_balances_and_difference() {
        let f = TempFile::new("opening");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let g = group_id(&slot, "Bank Accounts");
        let made = create_ledger(&mut slot, "HDFC Bank", &g, "2,50,000.50").unwrap();
        assert_eq!(made.ledger.opening, "250000.5");
        assert_eq!(made.ledger.balance, "250000.5");
        assert!(made.ledger.is_cash_or_bank);
        assert_eq!(made.opening_difference, "250000.5");
        let g = group_id(&slot, "Capital Account");
        let made = create_ledger(&mut slot, "Capital", &g, "-250000.50").unwrap();
        assert_eq!(made.opening_difference, "0");
        assert!(
            create_ledger(&mut slot, "hdfc bank", &g, "")
                .unwrap_err()
                .contains("already exists")
        );
        assert!(
            create_ledger(&mut slot, "X", &g, "1.2.3")
                .unwrap_err()
                .contains("invalid amount")
        );
        assert!(
            create_ledger(&mut slot, "Y", &g, "1e5").is_err(),
            "no float notation for money"
        );
    }

    #[test]
    fn groups_carry_parent_and_nature_for_the_form() {
        let f = TempFile::new("groups");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let debtors = search_groups(&slot, "debtors").unwrap();
        assert_eq!(debtors.len(), 1);
        assert_eq!(debtors[0].parent_name.as_deref(), Some("Current Assets"));
        assert_eq!(debtors[0].nature, Nature::Assets);
        assert!(debtors[0].is_party);
        let aliased = search_groups(&slot, "Expenses (Direct)").unwrap();
        assert_eq!(aliased[0].name, "Direct Expenses");
    }

    #[test]
    fn ledger_filters() {
        let f = TempFile::new("filters");
        let (slot, ..) = demo(&f);
        let names = |filter| -> Vec<String> {
            list_ledgers(&slot, "", filter)
                .unwrap()
                .into_iter()
                .map(|l| l.name)
                .collect()
        };
        assert_eq!(names("cash_bank"), ["HDFC Bank"]);
        assert_eq!(names("party"), ["HDFC Bank", "Sharma Enterprises"]);
        assert_eq!(
            names("non_cash_bank"),
            [
                "Office Rent",
                "Owner's Capital",
                "Sales",
                "Sharma Enterprises"
            ]
        );
        assert_eq!(names("all").len(), 5);
        assert_eq!(
            list_ledgers(&slot, "sharma", "all").unwrap()[0].balance,
            "40000"
        );
        assert_eq!(
            list_ledgers(&slot, "HDFC", "all").unwrap()[0].balance,
            "225000"
        );
        assert!(list_ledgers(&slot, "", "bogus").is_err());
    }

    #[test]
    fn vouchers_number_per_type_and_validate() {
        let f = TempFile::new("vouchers");
        let (mut slot, bank, _, rent, sales, party) = demo(&f);
        assert_eq!(next_voucher_number(&slot, "Payment", None).unwrap(), "2");
        assert_eq!(next_voucher_number(&slot, "Receipt", None).unwrap(), "1");
        let posted = post_voucher(
            &mut slot,
            VoucherInput {
                narration: "  May rent ".into(),
                ..voucher(
                    "Payment",
                    "2026-05-01",
                    vec![entry(&rent, DrCr::Dr, "100"), entry(&bank, DrCr::Cr, "100")],
                )
            },
        )
        .unwrap();
        assert_eq!(
            (posted.number.as_str(), posted.next_number.as_str()),
            ("2", "3")
        );
        assert_eq!(
            get_voucher(&slot, &posted.id).unwrap().narration,
            "May rent"
        );

        let unbalanced = voucher(
            "Sales",
            "2026-04-03",
            vec![
                entry(&party, DrCr::Dr, "10"),
                entry(&sales, DrCr::Cr, "9.99"),
            ],
        );
        assert!(
            post_voucher(&mut slot, unbalanced)
                .unwrap_err()
                .contains("do not equal")
        );
        let contra = voucher(
            "Contra",
            "2026-04-03",
            vec![entry(&bank, DrCr::Dr, "5"), entry(&rent, DrCr::Cr, "5")],
        );
        assert!(
            post_voucher(&mut slot, contra)
                .unwrap_err()
                .contains("cash and bank")
        );
        let dup = VoucherInput {
            number: Some("1".into()),
            ..voucher(
                "Sales",
                "2026-04-03",
                vec![entry(&party, DrCr::Dr, "1"), entry(&sales, DrCr::Cr, "1")],
            )
        };
        assert!(
            post_voucher(&mut slot, dup)
                .unwrap_err()
                .contains("already used")
        );
        assert!(
            post_voucher(&mut slot, voucher("Memo", "2026-04-03", vec![]))
                .unwrap_err()
                .contains("unknown voucher type")
        );
        assert!(
            post_voucher(&mut slot, voucher("Sales", "03-04-2026", vec![]))
                .unwrap_err()
                .contains("invalid date")
        );

        // Every successful mutation was saved: a reopened file sees it.
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(next_voucher_number(&reopened, "Payment", None).unwrap(), "3");
    }

    #[test]
    fn reports_through_the_command_layer() {
        let f = TempFile::new("reports");
        let (slot, bank, ..) = demo(&f);
        let rows = day_book(&slot, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(rows.len(), 2);
        let json = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(json["debit"], "25000", "amounts cross as strings");
        assert_eq!(json["kind"], "Payment");

        let st = ledger_statement(&slot, &bank, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(st.ledger_name, "HDFC Bank");
        let json = serde_json::to_value(&st).unwrap();
        assert_eq!(json["opening"], "250000");
        assert_eq!(json["closing"], "225000");

        let tb = trial_balance(&slot, None).unwrap();
        assert_eq!(tb.total_debit, tb.total_credit);
        assert_eq!(tb.total_debit, "290000");
        let early = trial_balance(&slot, Some("2026-03-31")).unwrap();
        assert_eq!(early.total_debit, "250000");

        let pl = profit_and_loss(&slot, "2026-04-01", "2026-04-30").unwrap();
        assert_eq!(pl.net_profit, dec!(15000));
        let bs = balance_sheet(&slot, "2026-04-30").unwrap();
        assert_eq!(bs.total_assets, bs.total_liabilities);

        let indirect = group_id(&slot, "Indirect Expenses");
        let gs = group_summary(&slot, &indirect, Some("2026-04-01"), "2026-04-30").unwrap();
        assert_eq!((gs.rows.len(), gs.total.as_str()), (1, "25000"));
        let current = group_id(&slot, "Current Assets");
        let gs = group_summary(&slot, &current, None, "2026-04-30").unwrap();
        assert_eq!(gs.total, "265000", "bank 2,25,000 + debtor 40,000");
    }

    #[test]
    fn keymap_dto_round_trips_and_resolves_like_rust() {
        let km = Keymap::tally_prime();
        let dto = keymap();
        assert_eq!(dto.len(), km.bindings().len());
        for b in &dto {
            let chord: Chord = b.chord.parse().unwrap();
            assert_eq!(chord.to_string(), b.chord, "display is canonical");
            let rust = km.resolve(chord, &[b.context]).map(|a| action_id(&a));
            assert_eq!(rust.as_deref(), Some(b.action.as_str()));
        }
        let find = |chord: &str, ctx: Context| {
            dto.iter()
                .find(|b| b.chord == chord && b.context == ctx)
                .map(|b| b.action.as_str())
        };
        assert_eq!(find("F8", Context::Voucher), Some("OpenVoucher:Sales"));
        assert_eq!(find("K", Context::Gateway), Some("Menu:DayBook"));
        assert_eq!(find("Ctrl+A", Context::Global), Some("Accept"));
        assert_eq!(serde_json::to_value(&dto[0]).unwrap()["context"], "Global");
    }

    #[test]
    fn helpers() {
        assert_eq!(
            company_file_name("Sharma & Sons Pvt. Ltd."),
            "sharma-sons-pvt-ltd.khata"
        );
        assert_eq!(company_file_name("!!"), "company.khata");
        assert_eq!(
            fy_start(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap()),
            NaiveDate::from_ymd_opt(2025, 4, 1).unwrap()
        );
        assert_eq!(
            fy_start(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()),
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
        );
        assert_eq!(parse_amount("").unwrap(), Amount::ZERO);
        assert_eq!(parse_amount(" -1,20,000.25 ").unwrap(), dec!(-120000.25));
        assert!(!os_user().is_empty());
    }

    #[test]
    fn lists_company_files_in_a_directory() {
        let f = TempFile::new("listing");
        create_company(&mut None, &f.0, "Listing Co").unwrap();
        let dir = f.0.parent().unwrap();
        assert!(
            list_company_files(dir)
                .iter()
                .any(|c| c.name == "Listing Co" && c.path == f.0.display().to_string())
        );
        assert!(list_company_files(Path::new("/definitely/not/here")).is_empty());
    }

    fn bill(kind: BillKind, reference: &str, amount: &str, due: Option<&str>) -> BillInput {
        BillInput {
            kind,
            reference: reference.into(),
            amount: amount.into(),
            due: due.map(Into::into),
        }
    }

    #[test]
    fn groups_can_be_created_under_a_parent() {
        let f = TempFile::new("groups-create");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let parent = group_id(&slot, "Sundry Debtors");
        let g = create_group(&mut slot, "Mumbai Debtors", &parent).unwrap();
        assert_eq!(g.parent_name.as_deref(), Some("Sundry Debtors"));
        assert!(g.is_party && g.nature == Nature::Assets);
        assert!(
            create_group(&mut slot, "mumbai debtors", &parent)
                .unwrap_err()
                .contains("exists")
        );
        assert!(create_group(&mut slot, " ", &parent).is_err());
        let made = create_ledger(&mut slot, "Patel Stores", &g.id, "")
            .unwrap()
            .ledger;
        assert!(made.is_party);
        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(
            search_groups(&reopened, "mumbai").unwrap().len(),
            1,
            "group saved and replayed"
        );
    }

    #[test]
    fn bill_wise_lines_feed_outstanding_and_ageing() {
        let f = TempFile::new("bills");
        let (mut slot, bank, _, _, sales, party) = demo(&f);
        // Sales No. 2 raises bill INV-2 due in 30 days.
        let mut dr = entry(&party, DrCr::Dr, "10000");
        dr.bills = vec![bill(BillKind::New, "INV-2", "10000", Some("2026-05-05"))];
        post_voucher(
            &mut slot,
            voucher(
                "Sales",
                "2026-04-05",
                vec![dr, entry(&sales, DrCr::Cr, "10000")],
            ),
        )
        .unwrap();
        // A receipt settles 4,000 against it and leaves 1,000 on account.
        let mut cr = entry(&party, DrCr::Cr, "5000");
        cr.bills = vec![
            bill(BillKind::Against, "INV-2", "4000", None),
            bill(BillKind::OnAccount, "", "1000", None),
        ];
        post_voucher(
            &mut slot,
            voucher(
                "Receipt",
                "2026-04-10",
                vec![entry(&bank, DrCr::Dr, "5000"), cr],
            ),
        )
        .unwrap();

        let open = outstanding(&slot, "2026-04-30", "receivable", Some(&party)).unwrap();
        let inv = open.iter().find(|b| b.reference == "INV-2").unwrap();
        assert_eq!(
            (inv.amount.as_str(), inv.due.map(|d| d.to_string())),
            ("6000", Some("2026-05-05".into()))
        );
        assert!(
            outstanding(&slot, "2026-04-30", "payable", None)
                .unwrap()
                .is_empty()
        );
        assert!(outstanding(&slot, "2026-04-30", "sideways", None).is_err());

        let rows = ageing(&slot, "2026-06-30", true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].party, "Sharma Enterprises");
        assert_eq!(
            rows[0].total, "45000",
            "40,000 on account from Sales 1 + 6,000 − 1,000 on account"
        );

        // Allocations must add up to the line.
        let mut bad = entry(&party, DrCr::Dr, "100");
        bad.bills = vec![bill(BillKind::New, "INV-3", "90", None)];
        assert!(
            post_voucher(
                &mut slot,
                voucher(
                    "Sales",
                    "2026-04-11",
                    vec![bad, entry(&sales, DrCr::Cr, "100")]
                )
            )
            .unwrap_err()
            .contains("do not add up")
        );
        let mut unnamed = entry(&party, DrCr::Dr, "100");
        unnamed.bills = vec![bill(BillKind::New, " ", "100", None)];
        assert!(
            post_voucher(
                &mut slot,
                voucher(
                    "Sales",
                    "2026-04-11",
                    vec![unnamed, entry(&sales, DrCr::Cr, "100")]
                )
            )
            .unwrap_err()
            .contains("needs a name")
        );

        // The voucher view carries the allocations back for alteration.
        let rows = day_book(&slot, "2026-04-05", "2026-04-05").unwrap();
        let v = get_voucher(&slot, &rows[0].voucher.0.to_string()).unwrap();
        assert_eq!(
            v.entries[0].bills,
            [bill(BillKind::New, "INV-2", "10000", Some("2026-05-05"))]
        );
    }

    #[test]
    fn msme_overdue_payables() {
        let f = TempFile::new("msme");
        let (mut slot, bank, _, rent, ..) = demo(&f);
        let supplier = ledger(&mut slot, "Kumar Supplies", "Sundry Creditors", "");
        let mut cr = entry(&supplier, DrCr::Cr, "8000");
        cr.bills = vec![bill(BillKind::New, "KS-17", "8000", None)];
        post_voucher(
            &mut slot,
            voucher(
                "Purchase",
                "2026-04-01",
                vec![entry(&rent, DrCr::Dr, "8000"), cr],
            ),
        )
        .unwrap();
        assert!(
            overdue_payables(&slot, "2026-05-10", 45)
                .unwrap()
                .is_empty(),
            "39 days"
        );
        let late = overdue_payables(&slot, "2026-05-20", 45).unwrap();
        assert_eq!(
            (late[0].reference.as_str(), late[0].amount.as_str()),
            ("KS-17", "-8000")
        );
        let _ = bank;
    }

    #[test]
    fn saved_vouchers_can_be_altered_and_deleted() {
        let f = TempFile::new("alter");
        let (mut slot, bank, _, rent, ..) = demo(&f);
        let id = day_book(&slot, "2026-04-01", "2026-04-01").unwrap()[0]
            .voucher
            .0
            .to_string();
        let altered = alter_voucher(
            &mut slot,
            &id,
            VoucherInput {
                narration: "corrected".into(),
                ..voucher(
                    "Payment",
                    "2026-04-01",
                    vec![
                        entry(&rent, DrCr::Dr, "26000"),
                        entry(&bank, DrCr::Cr, "26000"),
                    ],
                )
            },
        )
        .unwrap();
        assert_eq!(altered.number, "1", "keeps its number");
        let v = get_voucher(&slot, &id).unwrap();
        assert_eq!(
            (v.total.as_str(), v.narration.as_str()),
            ("26000", "corrected")
        );
        assert!(
            alter_voucher(
                &mut slot,
                &id,
                voucher("Payment", "2026-04-01", vec![entry(&rent, DrCr::Dr, "1")])
            )
            .is_err()
        );
        delete_voucher(&mut slot, &id).unwrap();
        assert!(get_voucher(&slot, &id).is_err());
        assert!(delete_voucher(&mut slot, &id).is_err());
        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(
            day_book(&reopened, "2026-04-01", "2026-04-01")
                .unwrap()
                .len(),
            0,
            "delete was saved"
        );
    }

    #[test]
    fn cancelled_vouchers_keep_their_number_but_not_their_values() {
        let f = TempFile::new("cancel");
        let (mut slot, ..) = demo(&f);
        let before = trial_balance(&slot, None).unwrap();
        let row = day_book(&slot, "2026-04-02", "2026-04-02")
            .unwrap()
            .remove(0);
        let id = row.voucher.0.to_string();
        cancel_voucher(&mut slot, &id).unwrap();
        let after = trial_balance(&slot, None).unwrap();
        assert_ne!(after, before);
        assert_eq!(after.total_debit, "250000", "the sale no longer counts");
        let listed = day_book(&slot, "2026-04-02", "2026-04-02").unwrap();
        let json = serde_json::to_value(&listed[0]).unwrap();
        assert_eq!(
            (json["cancelled"].as_bool(), json["debit"].as_str()),
            (Some(true), Some("0"))
        );
        assert!(get_voucher(&slot, &id).unwrap().cancelled);
        assert_eq!(
            next_voucher_number(&slot, "Sales", None).unwrap(),
            "2",
            "the number stays used"
        );
        assert!(cancel_voucher(&mut slot, &id).is_err());
        let v = get_voucher(&slot, &id).unwrap();
        let again = VoucherInput {
            kind: "Sales".into(),
            date: "2026-04-02".into(),
            number: None,
            entries: v
                .entries
                .iter()
                .map(|e| EntryInput {
                    ledger_id: e.ledger_id.clone(),
                    side: e.side,
                    amount: e.amount.clone(),
                    bills: Vec::new(),
                    items: Vec::new(),
                    forex: None,
                })
                .collect(),
            stock: Vec::new(),
            narration: String::new(),
        };
        assert!(
            alter_voucher(&mut slot, &id, again)
                .unwrap_err()
                .contains("cancel")
        );
        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert!(
            get_voucher(&reopened, &id).unwrap().cancelled,
            "cancel was saved"
        );
    }

    #[test]
    fn party_ledgers_keep_a_credit_period() {
        let f = TempFile::new("credit-days");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let made = create_ledger_with(&mut slot, "Patel Stores", &debtors, "", Some(30))
            .unwrap()
            .ledger;
        assert_eq!(made.credit_days, Some(30));
        let bank = group_id(&slot, "Bank Accounts");
        let not_party = create_ledger_with(&mut slot, "HDFC", &bank, "", Some(30))
            .unwrap()
            .ledger;
        assert_eq!(
            not_party.credit_days, None,
            "only party ledgers have a credit period"
        );
        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(
            list_ledgers(&reopened, "patel", "party").unwrap()[0].credit_days,
            Some(30)
        );
    }

    pub(crate) fn indian_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        set_country(&mut slot, Some(&f.0), "IN").unwrap();
        (f, slot)
    }

    #[test]
    fn country_is_picked_once_and_kept_in_the_trail() {
        let f = TempFile::new("country");
        let mut slot = None;
        let fresh = create_company(&mut slot, &f.0, "Demo").unwrap();
        assert_eq!(
            fresh.country, None,
            "a new company has no country until it is picked"
        );
        let india = list_countries();
        // Every shipped pack is listed (packs.rs), India first.
        let listed: Vec<_> = india.iter().map(|c| (c.code, c.currency, c.tax_id_label)).collect();
        assert_eq!(listed[0], ("IN", "INR", "GSTIN"));
        assert!(listed.contains(&("AE", "AED", "TRN")));
        assert_eq!(listed.len(), crate::packs::PACKS.len());
        assert_eq!(india[0].fy_start, fy_start(today()));
        assert!(
            set_country(&mut slot, Some(&f.0), "ZZ")
                .unwrap_err()
                .contains("no country pack")
        );
        let info = set_country(&mut slot, Some(&f.0), " in ").unwrap();
        assert_eq!(info.country.as_ref().map(|c| c.name), Some("India"));
        assert_eq!(info.fy_start.month(), 4);
        assert!(
            set_country(&mut slot, Some(&f.0), "IN")
                .unwrap_err()
                .contains("already set up for India")
        );
        drop(slot);
        let mut reopened = None;
        let info = open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(info.country.map(|c| c.code), Some("IN"));
        let (_, book) = reopened.as_ref().unwrap();
        assert_eq!(
            book.audit().entries().last().unwrap().entity,
            "profile:company"
        );
    }

    #[test]
    fn gstins_are_checked_as_typed() {
        let (_f, slot) = indian_company("check-gstin");
        let ok = check_tax_id(&slot, " 27aapfu0939f1zv ").unwrap();
        assert_eq!(ok.value, "27AAPFU0939F1ZV");
        assert_eq!(ok.error, None);
        assert_eq!(ok.region.as_deref(), Some("Maharashtra"));
        assert_eq!(ok.entity_id.as_deref(), Some("AAPFU0939F"));
        let bad = check_tax_id(&slot, "27AAPFU0939F1ZX").unwrap();
        assert_eq!(
            bad.error.as_deref(),
            Some("GSTIN is invalid: the check character should be V, not X")
        );
        let mut none = None;
        let f = TempFile::new("check-gstin-no-country");
        create_company(&mut none, &f.0, "Demo").unwrap();
        assert!(
            check_tax_id(&none, "27AAPFU0939F1ZV")
                .unwrap_err()
                .contains("country")
        );
    }

    #[test]
    fn an_invalid_gstin_blocks_the_party_ledger() {
        let (f, mut slot) = indian_company("gstin-party");
        let debtors = group_id(&slot, "Sundry Debtors");
        let party = |slot: &mut Slot, name: &str, gstin: &str, reason: Option<&str>| {
            create_party_ledger(slot, name, &debtors, "", None, Some(gstin), reason)
        };
        let made = party(&mut slot, "Urban Traders", "27aapfu0939f1zv", None)
            .unwrap()
            .ledger;
        assert_eq!(
            made.tax_id.as_deref(),
            Some("27AAPFU0939F1ZV"),
            "stored normalised"
        );
        assert_eq!(made.tax_id_override, None);

        // Rule M2: a wrong check character is a blocking error, and nothing is saved.
        let entries = slot.as_ref().unwrap().1.audit().entries().len();
        let e = party(&mut slot, "Typo Traders", "27AAPFU0939F1ZX", None).unwrap_err();
        assert_eq!(
            e,
            "GSTIN 27AAPFU0939F1ZX is invalid: the check character should be V, not X. \
             Correct it, or give a reason to keep it."
        );
        assert!(
            party(&mut slot, "Short Traders", "27AAPFU", Some("  ")).is_err(),
            "a blank reason is no reason"
        );
        assert_eq!(slot.as_ref().unwrap().1.audit().entries().len(), entries);
        assert!(
            list_ledgers(&slot, "traders", "all")
                .unwrap()
                .iter()
                .all(|l| l.name == "Urban Traders")
        );

        // Overriding needs a reason, which lands in the audit trail.
        let kept = party(
            &mut slot,
            "Embassy",
            "0717UNO00157UNX",
            Some("UIN of a UN body"),
        )
        .unwrap()
        .ledger;
        assert_eq!(kept.tax_id.as_deref(), Some("0717UNO00157UNX"));
        assert_eq!(kept.tax_id_override.as_deref(), Some("UIN of a UN body"));
        let (_, book) = slot.as_ref().unwrap();
        let last = book.audit().entries().last().unwrap();
        assert_eq!(last.user, os_user());
        assert_eq!(
            last.after.as_ref().unwrap()["tax_id_override"],
            "UIN of a UN body"
        );

        // A valid GSTIN ignores a stray reason.
        let clean = party(&mut slot, "Clean Co", "29AAGCB7383J1Z4", Some("not needed"))
            .unwrap()
            .ledger;
        assert_eq!(clean.tax_id_override, None);

        // Rule M3: no tax registration on a bank ledger.
        let bank = group_id(&slot, "Bank Accounts");
        let e = create_party_ledger(
            &mut slot,
            "HDFC",
            &bank,
            "",
            None,
            Some("27AAPFU0939F1ZV"),
            None,
        )
        .unwrap_err();
        assert!(e.contains("only party ledgers"), "{e}");
        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        let embassy = list_ledgers(&reopened, "embassy", "party")
            .unwrap()
            .remove(0);
        assert_eq!(embassy.tax_id_override.as_deref(), Some("UIN of a UN body"));
    }

    #[test]
    fn tax_ids_need_a_country() {
        let f = TempFile::new("gstin-no-country");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let e = create_party_ledger(
            &mut slot,
            "Urban",
            &debtors,
            "",
            None,
            Some("27AAPFU0939F1ZV"),
            None,
        )
        .unwrap_err();
        assert!(e.contains("country"), "{e}");
        // Without a tax ID nothing changes for companies set up earlier.
        assert!(
            create_party_ledger(&mut slot, "Urban", &debtors, "", None, Some(" "), None).is_ok()
        );
    }

    #[test]
    fn sales_invoice_numbers_follow_rule_46_per_financial_year() {
        let (_f, mut slot) = indian_company("rule46");
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let sale = |number: &str, date: &str| VoucherInput {
            number: Some(number.into()),
            ..voucher(
                "Sales",
                date,
                vec![
                    entry(&cash, DrCr::Dr, "100"),
                    entry(&sales, DrCr::Cr, "100"),
                ],
            )
        };
        post_voucher(&mut slot, sale("INV/2026-27/0001", "2026-04-01")).unwrap();
        let e = post_voucher(&mut slot, sale("INV/2026-27/00002", "2026-04-02")).unwrap_err();
        assert!(e.contains("at most 16 characters"), "{e}");
        let e = post_voucher(&mut slot, sale("INV 2", "2026-04-02")).unwrap_err();
        assert!(e.contains("contains ' '"), "{e}");
        let e = post_voucher(&mut slot, sale("inv/2026-27/0001", "2027-03-31")).unwrap_err();
        assert_eq!(
            e,
            "invoice number \"inv/2026-27/0001\" is already used in financial year 2026-27"
        );
        // Automatic numbers pass; other voucher types are not invoices.
        let auto = post_voucher(
            &mut slot,
            voucher(
                "Sales",
                "2026-04-03",
                vec![entry(&cash, DrCr::Dr, "5"), entry(&sales, DrCr::Cr, "5")],
            ),
        )
        .unwrap();
        // New companies number invoices in a yearly series carrying the year.
        assert_eq!(auto.number, "26-27/1");
        assert_eq!(auto.next_number, "26-27/2");
        assert_eq!(next_voucher_number(&slot, "Sales", Some("2027-04-01")).unwrap(), "27-28/1");
        assert_eq!(next_voucher_number(&slot, "Receipt", Some("2027-04-01")).unwrap(), "1");
        let journal = VoucherInput {
            number: Some("JV 2026 #1 with notes".into()),
            ..voucher(
                "Journal",
                "2026-04-03",
                vec![entry(&cash, DrCr::Dr, "5"), entry(&sales, DrCr::Cr, "5")],
            )
        };
        post_voucher(&mut slot, journal).unwrap();
    }

    #[test]
    fn invoices_numbered_before_the_country_was_picked_can_still_be_altered() {
        let f = TempFile::new("rule46-legacy");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let lines = || {
            vec![
                entry(&cash, DrCr::Dr, "100"),
                entry(&sales, DrCr::Cr, "100"),
            ]
        };
        let long = VoucherInput {
            number: Some("SALES/2026-27/000123".into()),
            ..voucher("Sales", "2026-04-01", lines())
        };
        let id = post_voucher(&mut slot, long).unwrap().id;
        set_country(&mut slot, Some(&f.0), "IN").unwrap();

        let same = VoucherInput {
            narration: "corrected".into(),
            ..voucher("Sales", "2026-05-01", lines())
        };
        alter_voucher(&mut slot, &id, same).unwrap();
        let renumbered = VoucherInput {
            number: Some("SALES/2026-27/0001234".into()),
            ..voucher("Sales", "2026-05-01", lines())
        };
        assert!(
            alter_voucher(&mut slot, &id, renumbered)
                .unwrap_err()
                .contains("at most 16")
        );
        let next_year = voucher("Sales", "2027-04-01", lines());
        assert!(
            alter_voucher(&mut slot, &id, next_year)
                .unwrap_err()
                .contains("at most 16"),
            "moving to another financial year re-checks the number"
        );
        let fixed = VoucherInput {
            number: Some("S/2026-27/123".into()),
            ..voucher("Sales", "2026-05-01", lines())
        };
        alter_voucher(&mut slot, &id, fixed).unwrap();
    }

    pub(crate) fn uae_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo FZE").unwrap();
        set_country(&mut slot, Some(&f.0), "AE").unwrap();
        (f, slot)
    }

    #[test]
    fn a_uae_company_checks_trns_and_charges_vat() {
        use crate::tax::{InvoiceTaxInput, InvoiceTaxLine, company_tax, invoice_tax, set_company_tax, set_ledger_tax, set_tax_rate};
        use khata_core::TaxSide;

        let f = TempFile::new("uae");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo FZE").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "ae").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("AE", "AED", "TRN"));
        assert_eq!((info.fy_start.month(), info.fy_start.day()), (1, 1), "calendar years");

        let ok = check_tax_id(&slot, " 100-1234-5670-0003 ").unwrap();
        assert_eq!((ok.value.as_str(), ok.error, ok.region, ok.entity_id.as_deref()), ("100123456700003", None, None, Some("1001234567")));
        let bad = check_tax_id(&slot, "10012345670000").unwrap();
        assert_eq!(bad.error.as_deref(), Some("TRN is invalid: it must be 15 characters long, not 14"));

        assert!(set_company_tax(&mut slot, "100123456700003", "XYZ").is_err(), "the emirate must be known");
        let tax = set_company_tax(&mut slot, "100123456700003", "DXB").unwrap();
        assert_eq!((tax.label.as_str(), tax.region_name.as_deref(), tax.regions.len()), ("TRN", Some("Dubai"), 7));
        assert_eq!(company_tax(&slot).unwrap().region.as_deref(), Some("DXB"));
        set_tax_rate(&mut slot, "STANDARD", "2018-01-01", "5").unwrap();

        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Abu Dhabi Co", &debtors, "", None, Some("100987654300003"), None)
            .unwrap()
            .ledger
            .id;
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        let t = invoice_tax(&slot, &InvoiceTaxInput {
            kind: VoucherKind::Sales,
            date: "2026-05-01".into(),
            party_id: buyer,
            lines: vec![InvoiceTaxLine { item_id: String::new(), ledger_id: String::new(), tax_class: "STANDARD".into(), taxable_value: "1999.90".into() }],
        })
        .unwrap();
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.place_of_supply.as_deref(), Some("DXB"), "a TRN gives no emirate: the company's own");
        let rows: Vec<_> = t.rows.iter().map(|r| (r.component.as_str(), r.percent.as_str(), r.amount.as_str(), r.ledger_name.as_deref())).collect();
        assert_eq!(rows, [("VAT", "5", "100", Some("Output VAT"))], "99.995 rounds to 100.00");

        // Invoice numbers are unique across years, unlike India's yearly series.
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let sale = |date: &str, number: &str| VoucherInput {
            number: Some(number.into()),
            ..voucher("Sales", date, vec![entry(&cash, DrCr::Dr, "100"), entry(&sales, DrCr::Cr, "100")])
        };
        post_voucher(&mut slot, sale("2026-12-31", "TI-1")).unwrap();
        assert!(post_voucher(&mut slot, sale("2027-01-01", "ti-1")).unwrap_err().contains("already used"));
        assert!(post_voucher(&mut slot, sale("2027-01-01", "TI 2")).unwrap_err().contains("hyphen"));
    }

    #[test]
    fn invoice_duplicate_checks_see_every_number_that_could_clash() {
        // The pack gets only the invoices with a loosely equal number; that
        // must still include case and punctuation variants, other years and
        // cancelled invoices.
        let f = TempFile::new("issued-prefilter");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo").unwrap();
        set_country(&mut slot, Some(&f.0), "SA").unwrap();
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let sale = |date: &str, number: &str| VoucherInput {
            number: Some(number.into()),
            ..voucher("Sales", date, vec![entry(&cash, DrCr::Dr, "100"), entry(&sales, DrCr::Cr, "100")])
        };
        let first = post_voucher(&mut slot, sale("2026-12-31", "INV/1")).unwrap();
        // ZATCA file names turn "/" into "-": the Saudi pack refuses the clash.
        assert!(post_voucher(&mut slot, sale("2027-02-01", "inv-1")).unwrap_err().contains("already used"));
        cancel_voucher(&mut slot, &first.id).unwrap();
        assert!(post_voucher(&mut slot, sale("2028-02-01", "INV/1")).unwrap_err().contains("already used"));
        post_voucher(&mut slot, sale("2027-02-01", "INV/10")).unwrap();
    }
}

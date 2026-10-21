//! The accountant's desk: notes and to-dos, the compliance calendar and
//! reminders worked out from the books (`docs/research/accountant-desk.md`).
//!
//! **Storage.** A note is not an accounting entry, so it is not in the
//! audit trail (`Book::apply_entry` refuses entity kinds it doesn't know,
//! and older copies and servers would refuse a new one). Notes, "filed"
//! marks and the calendar settings are the company's **shared records**
//! (`khata_store::records`): kept beside the trail and synced with
//! co-workers and the accountant on every path the books take (office
//! server, own server, relay, `.khatapkg`), last writer wins per row
//! (`docs/sync-settings.md`). One row per note (`desk.note.<uuid>`), so
//! saving one never rewrites the others; deleting a note is final, so an
//! edit made offline elsewhere never brings it back.
//!
//! **Rights.** Reading needs a sign-in (Auditor and up, so viewers and
//! auditors can read). Writing needs Data Entry and a writable company (a
//! company shared as Viewer is read-only). Deleting someone else's note,
//! and the calendar settings, need an Accountant.
//!
//! **Due dates** come from the company's compliance pack
//! (`CompliancePack::due_dates`); they are estimates and the screen says
//! to check the official portal. Reminders (bills falling due, post-dated
//! vouchers, MSME 45-day payables) are read from the books, never stored.

use std::collections::BTreeMap;

use chrono::{DateTime, Days, NaiveDate, Utc};
use khata_compliance::{CalendarCompany, CalendarDefaults, Category, Entity, Frequency, Shift, StateFiling};
use khata_core::reports;
use khata_core::{Book, LedgerId, StockItemId, UserRole, VoucherId};
use khata_store::Store;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::{self, ApiResult, Slot, amt, err, now, open_mut, open_ref, os_user, parse_date, parse_uuid};

const NOTE_PREFIX: &str = "desk.note.";
const FILED_PREFIX: &str = "desk.filed.";
const CALENDAR_KEY: &str = "desk.calendar";
const MAX_TITLE: usize = 200;
const MAX_TEXT: usize = 4000;
/// The furthest back unfiled statutory dates stay on the desk as overdue. They
/// also stop at the start of the financial year the books begin in, so a new
/// company is not shown years of dates from before it used KhataOS.
const OVERDUE_DAYS: u64 = 365;
/// "Due soon": the next seven days.
const SOON_DAYS: u64 = 7;
/// MSMED Act s.15: micro and small suppliers are paid within 45 days.
const MSME_DAYS: u64 = 45;

// ------------------------------------------------------------ notes

/// What a note is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Ledger,
    Voucher,
    StockItem,
}

/// A note or to-do as stored (JSON in the settings table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Note {
    id: String,
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    due: Option<NaiveDate>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    link_kind: Option<LinkKind>,
    #[serde(default)]
    link_id: Option<String>,
    created_at: DateTime<Utc>,
    created_by: String,
    updated_at: DateTime<Utc>,
    updated_by: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct NoteInput {
    /// Empty: a new note.
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub text: String,
    /// YYYY-MM-DD, or empty for none.
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub link_kind: Option<LinkKind>,
    #[serde(default)]
    pub link_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkDto {
    pub kind: LinkKind,
    pub id: String,
    /// The ledger or item name, or "Sales 26-27/4" for a voucher.
    pub label: String,
    /// A voucher's type and date, to open it.
    pub voucher_kind: Option<khata_core::VoucherKind>,
    pub date: Option<NaiveDate>,
    /// The record was deleted since.
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteDto {
    pub id: String,
    pub title: String,
    pub text: String,
    pub due: Option<NaiveDate>,
    pub done: bool,
    pub link: Option<LinkDto>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

fn store_of(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

fn read_notes(store: &Store) -> ApiResult<Vec<Note>> {
    let mut out = Vec::new();
    for (_, json) in store.records(NOTE_PREFIX).map_err(err)? {
        // A damaged row is skipped rather than hiding every other note.
        if let Ok(n) = serde_json::from_str::<Note>(&json) {
            out.push(n);
        }
    }
    Ok(out)
}

fn link_dto(book: &Book, kind: LinkKind, id: &str) -> LinkDto {
    let uuid = Uuid::parse_str(id).ok();
    let (label, voucher_kind, date) = match kind {
        LinkKind::Ledger => (uuid.and_then(|u| book.ledger(LedgerId(u))).map(|l| l.name.clone()), None, None),
        LinkKind::StockItem => (uuid.and_then(|u| book.stock_item(StockItemId(u))).map(|i| i.name.clone()), None, None),
        LinkKind::Voucher => match uuid.and_then(|u| book.voucher(VoucherId(u))) {
            Some(v) => (Some(format!("{:?} {}", v.kind, v.number)), Some(v.kind), Some(v.date)),
            None => (None, None, None),
        },
    };
    LinkDto { kind, id: id.to_owned(), missing: label.is_none(), label: label.unwrap_or_default(), voucher_kind, date }
}

fn note_dto(book: &Book, n: Note) -> NoteDto {
    let link = match (n.link_kind, n.link_id.as_deref()) {
        (Some(k), Some(id)) => Some(link_dto(book, k, id)),
        _ => None,
    };
    NoteDto {
        id: n.id,
        title: n.title,
        text: n.text,
        due: n.due,
        done: n.done,
        link,
        created_at: n.created_at,
        created_by: n.created_by,
        updated_at: n.updated_at,
        updated_by: n.updated_by,
    }
}

/// Open to-dos first by due date (no date last), then done ones.
fn sort_notes(notes: &mut [NoteDto]) {
    notes.sort_by(|a, b| {
        (a.done, a.due.is_none(), a.due, std::cmp::Reverse(a.updated_at)).cmp(&(b.done, b.due.is_none(), b.due, std::cmp::Reverse(b.updated_at)))
    });
}

/// Every note, or those linked to one record (`link_kind` and `link_id`).
pub fn notes(slot: &Slot, link_kind: Option<LinkKind>, link_id: Option<&str>) -> ApiResult<Vec<NoteDto>> {
    let book = open_ref(slot)?;
    let mut out: Vec<NoteDto> = read_notes(store_of(slot)?)?
        .into_iter()
        .filter(|n| link_kind.is_none_or(|k| n.link_kind == Some(k)) && link_id.is_none_or(|id| n.link_id.as_deref() == Some(id)))
        .map(|n| note_dto(book, n))
        .collect();
    sort_notes(&mut out);
    Ok(out)
}

fn check_link(book: &Book, kind: LinkKind, id: &str) -> ApiResult<String> {
    let what = match kind {
        LinkKind::Ledger => "ledger",
        LinkKind::Voucher => "voucher",
        LinkKind::StockItem => "stock item",
    };
    let u = parse_uuid(id, what)?;
    let exists = match kind {
        LinkKind::Ledger => book.ledger(LedgerId(u)).is_some(),
        LinkKind::Voucher => book.voucher(VoucherId(u)).is_some(),
        LinkKind::StockItem => book.stock_item(StockItemId(u)).is_some(),
    };
    if !exists {
        return Err(format!("that {what} doesn't exist"));
    }
    Ok(u.to_string())
}

/// Create or change a note (Data Entry and up).
pub fn save_note(slot: &mut Slot, input: &NoteInput) -> ApiResult<NoteDto> {
    let (store, book) = open_mut(slot)?;
    let title = input.title.trim();
    if title.is_empty() {
        return Err("a note needs a title".into());
    }
    if title.chars().count() > MAX_TITLE {
        return Err(format!("a note's title can have at most {MAX_TITLE} characters"));
    }
    let text = input.text.trim();
    if text.chars().count() > MAX_TEXT {
        return Err(format!("a note can have at most {MAX_TEXT} characters"));
    }
    let due = match input.due.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(d) => Some(parse_date(d)?),
    };
    let link_id = match (input.link_kind, input.link_id.as_deref().map(str::trim)) {
        (Some(k), Some(id)) if !id.is_empty() => Some(check_link(book, k, id)?),
        (None, None | Some("")) => None,
        _ => return Err("a link needs both its kind and its id".into()),
    };
    let user = os_user();
    let at = now();
    let id = input.id.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let note = match id {
        Some(id) => {
            let id = parse_uuid(id, "note")?.to_string();
            let json = store.record(&format!("{NOTE_PREFIX}{id}")).map_err(err)?.ok_or("that note doesn't exist any more")?;
            let old: Note = serde_json::from_str(&json).map_err(|e| format!("that note is damaged: {e}"))?;
            Note {
                title: title.to_owned(),
                text: text.to_owned(),
                due,
                done: input.done,
                link_kind: link_id.as_ref().and(input.link_kind),
                link_id,
                updated_at: at,
                updated_by: user,
                ..old
            }
        }
        None => Note {
            id: Uuid::now_v7().to_string(),
            title: title.to_owned(),
            text: text.to_owned(),
            due,
            done: input.done,
            link_kind: link_id.as_ref().and(input.link_kind),
            link_id,
            created_at: at,
            created_by: user.clone(),
            updated_at: at,
            updated_by: user,
        },
    };
    let json = serde_json::to_string(&note).map_err(err)?;
    store.set_record(&format!("{NOTE_PREFIX}{}", note.id), Some(&json), &note.updated_by).map_err(err)?;
    let book = open_ref(slot)?;
    Ok(note_dto(book, note))
}

/// Tick a to-do done, or open it again.
pub fn set_note_done(slot: &mut Slot, id: &str, done: bool) -> ApiResult<NoteDto> {
    let current = notes(slot, None, None)?.into_iter().find(|n| n.id == id.trim()).ok_or("that note doesn't exist any more")?;
    let input = NoteInput {
        id: Some(current.id),
        title: current.title,
        text: current.text,
        due: current.due.map(|d| d.to_string()),
        done,
        link_kind: current.link.as_ref().map(|l| l.kind),
        link_id: current.link.map(|l| l.id),
    };
    save_note(slot, &input)
}

/// Delete a note: its author with Data Entry, anyone else's with Accountant.
pub fn delete_note(slot: &mut Slot, id: &str) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    let id = parse_uuid(id, "note")?.to_string();
    let key = format!("{NOTE_PREFIX}{id}");
    let json = store.record(&key).map_err(err)?.ok_or("that note doesn't exist any more")?;
    let author = serde_json::from_str::<Note>(&json).map(|n| n.created_by).unwrap_or_default();
    if !author.eq_ignore_ascii_case(&os_user()) {
        api::require(book, UserRole::Accountant)?;
    }
    store.delete_record_forever(&key, &os_user()).map(|_| ()).map_err(err)
}

// --------------------------------------------------------- calendar

/// The company's compliance profile and filing pattern for the calendar.
/// `None` fields take the pack's default (or, for TDS/TCS, what the books
/// show; for the year end, the books' financial year). Kept in the company
/// file (`desk.calendar`), not in the books: the core profile has no place
/// for these and isn't changed for them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSettings {
    pub frequency: Option<Frequency>,
    pub quarter_end: Option<u32>,
    pub withholding: Option<bool>,
    pub collection: Option<bool>,
    /// Date of incorporation (or of starting the business).
    pub incorporated: Option<NaiveDate>,
    pub entity: Option<Entity>,
    /// The month (1 to 12) the financial year ends in.
    pub fy_end_month: Option<u32>,
    /// VAT/GST registration date; `None` with `vat_registered` unset: for as long as the books show.
    pub vat_from: Option<NaiveDate>,
    /// `Some(false)`: not registered for VAT/GST.
    pub vat_registered: Option<bool>,
    pub corporate_tax: Option<bool>,
    pub audit: Option<bool>,
    /// US: sales tax returns per state.
    pub states: Vec<StateFiling>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarSettingsDto {
    pub saved: CalendarSettings,
    /// What the calendar uses now.
    pub frequency: Frequency,
    pub quarter_end: u32,
    pub withholding: bool,
    pub collection: bool,
    pub defaults: Option<CalendarDefaults>,
    /// The country has TDS/TCS (India).
    pub has_withholding: bool,
    /// What the pack is told now: the profile with defaults filled in.
    pub company: CalendarCompany,
    /// The month the financial year ends in.
    pub fy_end_month: u32,
    /// The profile has been saved at least once (else the desk asks for it).
    pub profile_set: bool,
}

fn read_settings(store: &Store) -> ApiResult<CalendarSettings> {
    match store.record(CALENDAR_KEY).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("the calendar settings are damaged: {e}")),
        None => Ok(CalendarSettings::default()),
    }
}

fn calendar_company(book: &Book, s: &CalendarSettings) -> CalendarCompany {
    let defaults = api::pack(book).and_then(|p| p.calendar_defaults());
    let region = book.profile().and_then(|p| p.tax_region.clone());
    // A US company with no states set files in its home state, quarterly.
    let states = if s.states.is_empty() && book.country() == Some(khata_us::COUNTRY.code) {
        region.as_deref().and_then(khata_us::states::state).map(|st| vec![StateFiling { state: st.code.to_owned(), frequency: Frequency::Quarterly }]).unwrap_or_default()
    } else {
        s.states.clone()
    };
    CalendarCompany {
        frequency: s.frequency.or(defaults.map(|d| d.frequency)),
        quarter_end: s.quarter_end.or(defaults.map(|d| d.quarter_end)),
        region,
        fy_start_month: s.fy_end_month.map_or_else(|| book.fy_start_month(), |m| m % 12 + 1),
        withholding: s.withholding.unwrap_or_else(|| book.ledgers().any(|l| l.tds.is_some())),
        collection: s.collection.unwrap_or_else(|| book.ledgers().any(|l| l.tcs.is_some())),
        incorporated: s.incorporated,
        entity: s.entity.or(defaults.and_then(|d| d.entities.first().copied())),
        vat_from: s.vat_from,
        vat_unregistered: s.vat_registered == Some(false),
        corporate_tax: s.corporate_tax,
        audit: s.audit.unwrap_or(false),
        states,
    }
}

fn settings_dto(book: &Book, saved: CalendarSettings, profile_set: bool) -> CalendarSettingsDto {
    let c = calendar_company(book, &saved);
    CalendarSettingsDto {
        fy_end_month: if c.fy_start_month == 1 { 12 } else { c.fy_start_month - 1 },
        profile_set,
        company: c.clone(),
        frequency: c.frequency.unwrap_or_default(),
        quarter_end: c.quarter_end.unwrap_or(3),
        withholding: c.withholding,
        collection: c.collection,
        defaults: api::pack(book).and_then(|p| p.calendar_defaults()),
        has_withholding: book.country() == Some(khata_in::COUNTRY.code),
        saved,
    }
}

fn profile_set(store: &Store) -> ApiResult<bool> {
    Ok(store.record(CALENDAR_KEY).map_err(err)?.is_some())
}

pub fn calendar_settings(slot: &Slot) -> ApiResult<CalendarSettingsDto> {
    let book = open_ref(slot)?;
    let store = store_of(slot)?;
    Ok(settings_dto(book, read_settings(store)?, profile_set(store)?))
}

/// Save the filing pattern (Accountant and up).
pub fn set_calendar_settings(slot: &mut Slot, settings: &CalendarSettings) -> ApiResult<CalendarSettingsDto> {
    let (store, book) = open_mut(slot)?;
    api::require(book, UserRole::Accountant)?;
    if settings.quarter_end.is_some_and(|m| !(1..=12).contains(&m)) {
        return Err("a quarter ends in a month from 1 to 12".into());
    }
    if settings.fy_end_month.is_some_and(|m| !(1..=12).contains(&m)) {
        return Err("the financial year ends in a month from 1 to 12".into());
    }
    let defaults = api::pack(book).and_then(|p| p.calendar_defaults());
    if let (Some(e), Some(d)) = (settings.entity, defaults) {
        if !d.entities.contains(&e) {
            return Err(format!("{e:?} is not a business form this country's calendar knows"));
        }
    }
    let mut clean = settings.clone();
    let mut seen = std::collections::BTreeSet::new();
    for st in &mut clean.states {
        let s = khata_us::states::state(&st.state).ok_or_else(|| format!("{:?} is not a US state code", st.state))?;
        st.state = s.code.to_owned();
        if !seen.insert(st.state.clone()) {
            return Err(format!("{} is listed twice", st.state));
        }
    }
    let json = serde_json::to_string(&clean).map_err(err)?;
    store.set_record(CALENDAR_KEY, Some(&json), &os_user()).map_err(err)?;
    calendar_settings(slot)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Filed {
    by: String,
    at: DateTime<Utc>,
}

fn filed_key(code: &str, period_to: NaiveDate) -> String {
    format!("{FILED_PREFIX}{code}.{period_to}")
}

/// The code a "filed" mark is kept under: the rule's, plus the subject (a US state).
fn filed_code(d: &khata_compliance::DueDate) -> String {
    match &d.subject {
        Some(s) => format!("{}.{s}", d.rule.code),
        None => d.rule.code.to_owned(),
    }
}

/// A statutory due date on the desk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DueDto {
    pub code: &'static str,
    pub title: &'static str,
    pub rule: &'static str,
    pub source: &'static str,
    pub optional: bool,
    pub period_from: NaiveDate,
    pub period_to: NaiveDate,
    pub due: NaiveDate,
    /// The rule's date before a weekend move (equal to `due` when not moved).
    pub unadjusted: NaiveDate,
    pub category: Category,
    /// What a weekend or holiday does to the date.
    pub shift: Shift,
    /// A US state, for its sales tax return.
    pub subject: Option<String>,
    /// What `mark_filed` takes for this date.
    pub filed_code: String,
    /// Marked filed or paid on the desk, and by whom.
    pub filed: bool,
    pub filed_by: Option<String>,
}

fn statutory(book: &Book, store: &Store, from: NaiveDate, to: NaiveDate) -> ApiResult<Vec<DueDto>> {
    let Some(pack) = api::pack(book) else { return Ok(Vec::new()) };
    let company = calendar_company(book, &read_settings(store)?);
    let filed: BTreeMap<String, Filed> = store
        .records(FILED_PREFIX)
        .map_err(err)?
        .into_iter()
        .filter_map(|(k, v)| serde_json::from_str(&v).ok().map(|f| (k, f)))
        .collect();
    Ok(pack
        .due_dates(&company, from, to)
        .into_iter()
        .map(|d| {
            let code = filed_code(&d);
            let f = filed.get(&filed_key(&code, d.period_to));
            DueDto {
                unadjusted: d.unadjusted,
                category: d.rule.category,
                shift: d.rule.shift,
                subject: d.subject.clone(),
                filed_code: code,
                code: d.rule.code,
                title: d.rule.title,
                rule: d.rule.rule,
                source: d.rule.source,
                optional: d.rule.optional,
                period_from: d.period_from,
                period_to: d.period_to,
                due: d.due,
                filed: f.is_some(),
                filed_by: f.map(|f| f.by.clone()),
            }
        })
        .collect())
}

/// Statutory due dates in a range (for the calendar's other months).
pub fn due_dates(slot: &Slot, from: &str, to: &str) -> ApiResult<Vec<DueDto>> {
    let book = open_ref(slot)?;
    let (from, to) = (parse_date(from)?, parse_date(to)?);
    if to < from {
        return Err("the range ends before it starts".into());
    }
    statutory(book, store_of(slot)?, from, to)
}

/// Mark a return filed (or paid), or clear the mark (Data Entry and up).
pub fn mark_filed(slot: &mut Slot, code: &str, period_to: &str, filed: bool) -> ApiResult<()> {
    let (store, _) = open_mut(slot)?;
    let code = code.trim();
    if code.is_empty() || code.len() > 32 || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        return Err(format!("unknown return {code:?}"));
    }
    let key = filed_key(code, parse_date(period_to)?);
    let value = filed.then(|| serde_json::to_string(&Filed { by: os_user(), at: now() })).transpose().map_err(err)?;
    store.set_record(&key, value.as_deref(), &os_user()).map(|_| ()).map_err(err)
}

// -------------------------------------------------------- reminders

/// A party's bill with a due date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BillDueDto {
    pub ledger_id: String,
    pub party: String,
    pub reference: String,
    pub date: Option<NaiveDate>,
    pub due: NaiveDate,
    /// Pending, positive.
    pub amount: String,
    pub receivable: bool,
}

/// A voucher dated after today (a post-dated cheque or bill).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostDatedDto {
    pub voucher_id: String,
    pub kind: khata_core::VoucherKind,
    pub number: String,
    pub date: NaiveDate,
    pub particulars: String,
    pub amount: String,
}

/// A supplier bill against the 45-day limit (India, MSMED Act s.15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MsmeDto {
    pub ledger_id: String,
    pub party: String,
    pub reference: String,
    pub date: NaiveDate,
    /// Bill date + 45 days.
    pub limit: NaiveDate,
    pub amount: String,
    /// The supplier is marked MSME (Schedule III mapping).
    pub msme_party: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeskDto {
    pub as_at: NaiveDate,
    pub notes: Vec<NoteDto>,
    /// From [`overdue_from`] (unfiled ones are overdue) to `to`.
    pub statutory: Vec<DueDto>,
    /// Open bills with a due date up to `to`, overdue ones included.
    pub bills: Vec<BillDueDto>,
    pub post_dated: Vec<PostDatedDto>,
    /// Supplier bills past, or within a week of, 45 days (India only).
    pub msme: Vec<MsmeDto>,
    pub settings: CalendarSettingsDto,
    /// The signed-in user may add and change notes.
    pub can_write: bool,
    /// No backup for a week (or never) while the books changed: a gentle
    /// reminder (src/backup.rs). Filled in by the command, which knows
    /// this computer's backup folder.
    pub backup: Option<crate::backup::ReminderDto>,
}

fn days(d: NaiveDate, n: u64) -> NaiveDate {
    d.checked_add_days(Days::new(n)).unwrap_or(d)
}

fn bills_due(book: &Book, as_at: NaiveDate, to: NaiveDate) -> Vec<BillDueDto> {
    reports::outstanding_bills(book, as_at)
        .into_iter()
        .filter_map(|b| {
            let due = b.due.filter(|d| *d <= to)?;
            Some(BillDueDto {
                ledger_id: b.ledger.0.to_string(),
                party: b.party,
                reference: b.reference,
                date: b.date,
                due,
                amount: amt(b.amount.abs()),
                receivable: b.amount.is_sign_positive(),
            })
        })
        .collect()
}

fn post_dated(book: &Book, as_at: NaiveDate) -> Vec<PostDatedDto> {
    let Some(from) = as_at.succ_opt() else { return Vec::new() };
    let last = book.active_vouchers().map(|v| v.date).max().unwrap_or(as_at);
    if last < from {
        return Vec::new();
    }
    reports::day_book(book, from, last)
        .into_iter()
        .filter(|r| !r.cancelled)
        .map(|r| PostDatedDto {
            voucher_id: r.voucher.0.to_string(),
            kind: r.kind,
            number: r.number,
            date: r.date,
            particulars: r.particulars,
            amount: amt(r.debit.max(r.credit)),
        })
        .collect()
}

fn msme(book: &Book, as_at: NaiveDate) -> Vec<MsmeDto> {
    if book.country() != Some(khata_in::COUNTRY.code) {
        return Vec::new();
    }
    let marked = book.profile().map(|p| p.msme_parties.clone()).unwrap_or_default();
    let soon = days(as_at, SOON_DAYS);
    reports::outstanding_bills(book, as_at)
        .into_iter()
        .filter(|b| b.amount.is_sign_negative())
        .filter_map(|b| {
            let date = b.date?;
            let limit = days(date, MSME_DAYS);
            (limit <= soon).then(|| MsmeDto {
                ledger_id: b.ledger.0.to_string(),
                msme_party: marked.contains(&b.ledger),
                party: b.party,
                reference: b.reference,
                date,
                limit,
                amount: amt(b.amount.abs()),
            })
        })
        .collect()
}

fn can_write(slot: &Slot) -> bool {
    let Some((store, book)) = slot.as_ref() else { return false };
    crate::lan::check_writable(store).is_ok() && api::require(book, UserRole::DataEntry).is_ok()
}

/// Where overdue statutory dates start: the start of the financial year the
/// books begin in (first voucher, else the company's first entry), but at
/// most [`OVERDUE_DAYS`] back. A filing missed months ago stays on the desk
/// until it is marked filed.
fn overdue_from(book: &Book, as_at: NaiveDate) -> NaiveDate {
    let year_back = as_at.checked_sub_days(Days::new(OVERDUE_DAYS)).unwrap_or(as_at);
    let first = book
        .vouchers()
        .map(|v| v.date)
        .min()
        .or_else(|| book.audit().entries().first().map(|e| e.at.date_naive()))
        .unwrap_or(as_at);
    let books_start = api::pack(book).map_or(first, |p| p.financial_year(first).start);
    books_start.max(year_back).min(as_at)
}

/// Everything the desk shows as at a date, with due dates up to `to`.
pub fn overview(slot: &Slot, as_at: &str, to: &str) -> ApiResult<DeskDto> {
    let book = open_ref(slot)?;
    let store = store_of(slot)?;
    let (as_at, to) = (parse_date(as_at)?, parse_date(to)?);
    let from = overdue_from(book, as_at);
    Ok(DeskDto {
        as_at,
        notes: notes(slot, None, None)?,
        statutory: statutory(book, store, from, to.max(as_at))?,
        bills: bills_due(book, as_at, to.max(as_at)),
        post_dated: post_dated(book, as_at),
        msme: msme(book, as_at),
        settings: settings_dto(book, read_settings(store)?, profile_set(store)?),
        can_write: can_write(slot),
        backup: None,
    })
}

/// The home screen's "due soon" line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DueCount {
    /// Open to-dos and unfiled returns past their date.
    pub overdue: usize,
    /// To-dos, returns, bills and post-dated vouchers in the next 7 days.
    pub soon: usize,
}

pub fn due_count(slot: &Slot, as_at: &str) -> ApiResult<DueCount> {
    let today = parse_date(as_at)?;
    let soon = days(today, SOON_DAYS);
    let d = overview(slot, as_at, &soon.to_string())?;
    let mut c = DueCount::default();
    for due in d.notes.iter().filter(|n| !n.done).filter_map(|n| n.due) {
        if due < today {
            c.overdue += 1;
        } else if due <= soon {
            c.soon += 1;
        }
    }
    for s in d.statutory.iter().filter(|s| !s.filed) {
        if s.due < today {
            c.overdue += 1;
        } else {
            c.soon += 1;
        }
    }
    c.soon += d.bills.iter().filter(|b| b.due >= today).count();
    c.soon += d.post_dated.iter().filter(|p| p.date <= soon).count();
    Ok(c)
}

pub mod cmd {
    //! Tauri commands for the desk (kept here so `commands.rs` stays small).
    use tauri::State;

    use super::*;
    use crate::AppState;

    fn lock<'a>(state: &'a State<'_, AppState>) -> ApiResult<std::sync::MutexGuard<'a, Slot>> {
        state.company.lock().map_err(|_| "company state is poisoned".to_owned())
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_overview(app: tauri::AppHandle, state: State<'_, AppState>, as_at: String, to: String) -> ApiResult<DeskDto> {
        let path = state.path.lock().ok().and_then(|p| p.clone());
        let slot = lock(&state)?;
        let mut desk = overview(&slot, &as_at, &to)?;
        if let (Some(path), Ok(env)) = (path, crate::backup::cmd::env(&app)) {
            desk.backup = crate::backup::reminder(&slot, &env, &path, chrono::Utc::now());
        }
        Ok(desk)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_due_count(state: State<'_, AppState>, as_at: String) -> ApiResult<DueCount> {
        due_count(&*lock(&state)?, &as_at)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_due_dates(state: State<'_, AppState>, from: String, to: String) -> ApiResult<Vec<DueDto>> {
        due_dates(&*lock(&state)?, &from, &to)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_notes(state: State<'_, AppState>, link_kind: Option<LinkKind>, link_id: Option<String>) -> ApiResult<Vec<NoteDto>> {
        notes(&*lock(&state)?, link_kind, link_id.as_deref())
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_save_note(state: State<'_, AppState>, note: NoteInput) -> ApiResult<NoteDto> {
        save_note(&mut *lock(&state)?, &note)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_set_done(state: State<'_, AppState>, id: String, done: bool) -> ApiResult<NoteDto> {
        set_note_done(&mut *lock(&state)?, &id, done)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_delete_note(state: State<'_, AppState>, id: String) -> ApiResult<()> {
        delete_note(&mut *lock(&state)?, &id)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_mark_filed(state: State<'_, AppState>, code: String, period_to: String, filed: bool) -> ApiResult<()> {
        mark_filed(&mut *lock(&state)?, &code, &period_to, filed)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_calendar_settings(state: State<'_, AppState>) -> ApiResult<CalendarSettingsDto> {
        calendar_settings(&*lock(&state)?)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn desk_set_calendar_settings(state: State<'_, AppState>, settings: CalendarSettings) -> ApiResult<CalendarSettingsDto> {
        set_calendar_settings(&mut *lock(&state)?, &settings)
    }
}

#[cfg(test)]
mod tests {
    use khata_core::{BillKind, DrCr};

    use super::*;
    use crate::api::tests::{TempFile, demo, entry, voucher};
    use crate::api::{BillInput, open_company, post_voucher, set_country};
    use crate::security::{save_user, sign_in, sign_out};

    fn input(title: &str) -> NoteInput {
        NoteInput { title: title.into(), ..Default::default() }
    }

    #[test]
    fn notes_round_trip_outside_the_audit_trail() {
        let f = TempFile::new("desk-notes");
        let (mut slot, bank, _, rent, _, party) = demo(&f);
        set_country(&mut slot, Some(&f.0), "IN").unwrap();
        let trail = slot.as_ref().unwrap().1.audit().entries().len();
        let hash = slot.as_ref().unwrap().1.audit().entries().last().unwrap().hash.clone();

        let call = save_note(
            &mut slot,
            &NoteInput {
                title: " Called Sharma ".into(),
                text: "Called on 3 Oct, will pay Friday".into(),
                due: Some("2026-10-09".into()),
                link_kind: Some(LinkKind::Ledger),
                link_id: Some(party.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(call.title, "Called Sharma");
        assert_eq!(call.link.as_ref().unwrap().label, "Sharma Enterprises");
        let v = post_voucher(&mut slot, voucher("Payment", "2026-04-09", vec![entry(&rent, DrCr::Dr, "10"), entry(&bank, DrCr::Cr, "10")])).unwrap();
        let vnote = save_note(&mut slot, &NoteInput { link_kind: Some(LinkKind::Voucher), link_id: Some(v.id.clone()), ..input("Ask for the rent receipt") }).unwrap();
        assert!(vnote.link.as_ref().unwrap().label.starts_with("Payment "));
        save_note(&mut slot, &input("Month-end close")).unwrap();

        // Bad input.
        assert!(save_note(&mut slot, &input("  ")).is_err());
        assert!(save_note(&mut slot, &NoteInput { due: Some("9/10".into()), ..input("x") }).is_err());
        assert!(save_note(&mut slot, &NoteInput { link_kind: Some(LinkKind::Ledger), link_id: Some(Uuid::nil().to_string()), ..input("x") }).unwrap_err().contains("doesn't exist"));
        assert!(save_note(&mut slot, &NoteInput { link_kind: Some(LinkKind::StockItem), ..input("x") }).is_err());

        // Done, edited, filtered.
        set_note_done(&mut slot, &call.id, true).unwrap();
        let edited = save_note(&mut slot, &NoteInput { id: Some(vnote.id.clone()), due: Some("2026-10-30".into()), link_kind: Some(LinkKind::Voucher), link_id: Some(v.id.clone()), ..input("Receipt asked") }).unwrap();
        assert_eq!((edited.id.as_str(), edited.created_at), (vnote.id.as_str(), vnote.created_at));
        assert_eq!(notes(&slot, Some(LinkKind::Ledger), Some(&party)).unwrap().len(), 1);
        assert_eq!(notes(&slot, Some(LinkKind::Voucher), Some(&v.id)).unwrap()[0].title, "Receipt asked");

        // Nothing reached the books (only the voucher above did).
        let book = &slot.as_ref().unwrap().1;
        assert_eq!(book.audit().entries().len(), trail + 1);
        assert_eq!(book.audit().entries()[trail - 1].hash, hash);

        // Reopen: the notes are in the file, open to-dos first, done ones last.
        drop(slot);
        let mut slot = None;
        open_company(&mut slot, &f.0).unwrap();
        let all = notes(&slot, None, None).unwrap();
        assert_eq!(all.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["Receipt asked", "Month-end close", "Called Sharma"]);
        assert!(all[2].done);
        delete_note(&mut slot, &all[1].id).unwrap();
        assert_eq!(notes(&slot, None, None).unwrap().len(), 2);
        // A deleted voucher leaves its note, marked missing.
        crate::api::delete_voucher(&mut slot, &v.id).unwrap();
        assert!(notes(&slot, Some(LinkKind::Voucher), Some(&v.id)).unwrap()[0].link.as_ref().unwrap().missing);
    }

    #[test]
    fn auditors_read_data_entry_writes_accountants_delete_others() {
        let f = TempFile::new("desk-roles");
        let (mut slot, ..) = demo(&f);
        set_country(&mut slot, Some(&f.0), "IN").unwrap();
        save_user(&mut slot, "Meera", UserRole::Owner, "owner-pass-1", false).unwrap();
        save_user(&mut slot, "Ravi", UserRole::DataEntry, "clerk-pass-1", false).unwrap();
        save_user(&mut slot, "Asha", UserRole::Auditor, "audit-pass-1", false).unwrap();
        let mine = save_note(&mut slot, &input("Owner's note")).unwrap();
        assert_eq!(mine.created_by, "Meera");

        sign_in(&slot, "Asha", "audit-pass-1").unwrap();
        assert_eq!(notes(&slot, None, None).unwrap().len(), 1, "auditors read");
        assert!(!overview(&slot, "2026-10-21", "2026-10-28").unwrap().can_write);
        assert!(save_note(&mut slot, &input("x")).unwrap_err().contains("Data Entry"));
        assert!(mark_filed(&mut slot, "IN.GSTR1", "2026-09-30", true).is_err());

        sign_in(&slot, "Ravi", "clerk-pass-1").unwrap();
        assert!(overview(&slot, "2026-10-21", "2026-10-28").unwrap().can_write);
        let his = save_note(&mut slot, &input("Ravi's to-do")).unwrap();
        set_note_done(&mut slot, &mine.id, true).unwrap();
        assert!(delete_note(&mut slot, &mine.id).unwrap_err().contains("Accountant"), "not someone else's");
        assert!(set_calendar_settings(&mut slot, &CalendarSettings::default()).unwrap_err().contains("Accountant"));
        delete_note(&mut slot, &his.id).unwrap();

        sign_in(&slot, "Meera", "owner-pass-1").unwrap();
        delete_note(&mut slot, &mine.id).unwrap();
        sign_out();
        drop(slot);
        let slot = {
            let mut s = None;
            open_company(&mut s, &f.0).unwrap();
            s
        };
        assert!(notes(&slot, None, None).unwrap_err().contains("sign in"));
    }

    #[test]
    fn calendar_reminders_and_filed_marks() {
        let f = TempFile::new("desk-calendar");
        let (mut slot, bank, _, _, sales, party) = demo(&f);
        set_country(&mut slot, Some(&f.0), "IN").unwrap();
        let creditor = crate::api::tests::ledger(&mut slot, "Gupta Supplies", "Sundry Creditors", "");
        let purchase = crate::api::tests::ledger(&mut slot, "Purchases", "Purchase Accounts", "");
        let bill = |r: &str, amount: &str, due: &str| vec![BillInput { kind: BillKind::New, reference: r.into(), amount: amount.into(), due: Some(due.into()) }];
        let mut sale = voucher("Sales", "2026-09-01", vec![entry(&party, DrCr::Dr, "500"), entry(&sales, DrCr::Cr, "500")]);
        sale.entries[0].bills = bill("S-9", "500", "2026-10-25");
        post_voucher(&mut slot, sale).unwrap();
        let mut buy = voucher("Purchase", "2026-09-01", vec![entry(&purchase, DrCr::Dr, "300"), entry(&creditor, DrCr::Cr, "300")]);
        buy.entries[1].bills = bill("P-1", "300", "2026-10-01");
        post_voucher(&mut slot, buy).unwrap();
        // A post-dated cheque.
        post_voucher(&mut slot, voucher("Receipt", "2026-11-05", vec![entry(&bank, DrCr::Dr, "200"), entry(&party, DrCr::Cr, "200")])).unwrap();

        let d = overview(&slot, "2026-10-21", "2026-11-30").unwrap();
        assert!(d.can_write);
        let gstr1: Vec<_> = d.statutory.iter().filter(|s| s.code == "IN.GSTR1").map(|s| s.due.to_string()).collect();
        assert_eq!(
            gstr1,
            ["2026-04-11", "2026-05-11", "2026-06-11", "2026-07-11", "2026-08-11", "2026-09-11", "2026-10-11", "2026-11-11"],
            "unfiled ones stay back to the start of the financial year the books begin in (2026-27), not before"
        );
        assert!(d.statutory.iter().all(|s| s.code != "IN.TDSPAY"), "no TDS ledgers");
        let refs: Vec<_> = d.bills.iter().map(|b| (b.reference.as_str(), b.receivable, b.amount.as_str())).collect();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&("S-9", true, "500")) && refs.contains(&("P-1", false, "300")));
        assert_eq!(d.post_dated.len(), 1);
        assert_eq!((d.post_dated[0].date.to_string(), d.post_dated[0].amount.as_str()), ("2026-11-05".into(), "200"));
        // The purchase of 1 Sep reaches 45 days on 16 Oct.
        assert_eq!(d.msme.len(), 1);
        assert_eq!((d.msme[0].reference.as_str(), d.msme[0].limit.to_string(), d.msme[0].msme_party), ("P-1", "2026-10-16".into(), false));

        // Filing GSTR-1 for September takes it off the overdue list; the count follows.
        let before = due_count(&slot, "2026-10-21").unwrap();
        mark_filed(&mut slot, "IN.GSTR1", "2026-09-30", true).unwrap();
        let d2 = overview(&slot, "2026-10-21", "2026-11-30").unwrap();
        let sep = d2.statutory.iter().find(|s| s.code == "IN.GSTR1" && s.period_to.to_string() == "2026-09-30").unwrap();
        assert!(sep.filed && sep.filed_by.is_some());
        assert_eq!(due_count(&slot, "2026-10-21").unwrap().overdue, before.overdue - 1);
        assert!(mark_filed(&mut slot, "bad code!", "2026-09-30", true).is_err());

        // Quarterly (QRMP) from the settings; TDS switched on by hand.
        let s = set_calendar_settings(&mut slot, &CalendarSettings { frequency: Some(Frequency::Quarterly), withholding: Some(true), ..Default::default() }).unwrap();
        assert_eq!((s.frequency, s.withholding, s.has_withholding), (Frequency::Quarterly, true, true));
        let q = due_dates(&slot, "2026-11-01", "2026-11-30").unwrap();
        let codes: Vec<_> = q.iter().map(|x| x.code).collect();
        assert!(codes.contains(&"IN.TDSPAY") && codes.contains(&"IN.PMT06") && !codes.contains(&"IN.GSTR1"));
        assert!(set_calendar_settings(&mut slot, &CalendarSettings { quarter_end: Some(13), ..Default::default() }).is_err());
    }

    #[test]
    fn the_compliance_profile_drives_annual_dates() {
        let f = TempFile::new("desk-profile");
        let (mut slot, ..) = demo(&f);
        set_country(&mut slot, Some(&f.0), "GB").unwrap();
        assert!(!calendar_settings(&slot).unwrap().profile_set, "the desk asks for it");
        let profile = CalendarSettings { incorporated: Some(NaiveDate::from_ymd_opt(2025, 10, 10).unwrap()), fy_end_month: Some(10), quarter_end: Some(4), ..Default::default() };
        let s = set_calendar_settings(&mut slot, &profile).unwrap();
        assert!(s.profile_set);
        assert_eq!((s.fy_end_month, s.company.fy_start_month, s.company.entity), (10, 11, Some(Entity::Company)));
        let dates = due_dates(&slot, "2026-01-01", "2027-12-31").unwrap();
        let on = |code: &str| dates.iter().filter(|d| d.code == code).map(|d| d.due.to_string()).collect::<Vec<_>>();
        assert_eq!(on("GB.ACCOUNTS"), ["2027-07-10"], "first accounts 21 months after incorporation");
        assert_eq!(on("GB.CS01"), ["2026-10-24", "2027-10-24"]);
        assert!(dates.iter().all(|d| d.code != "GB.VAT" || d.period_to.to_string() != "2026-03-31"), "stagger 2: quarters end in April");
        assert!(dates.iter().any(|d| d.category == Category::Filing) && dates.iter().all(|d| d.shift == Shift::Fixed));
        // Not VAT registered: no VAT returns.
        set_calendar_settings(&mut slot, &CalendarSettings { vat_registered: Some(false), ..profile.clone() }).unwrap();
        assert!(due_dates(&slot, "2026-01-01", "2027-12-31").unwrap().iter().all(|d| d.code != "GB.VAT"));
        // Bad input.
        assert!(set_calendar_settings(&mut slot, &CalendarSettings { fy_end_month: Some(0), ..Default::default() }).is_err());
        assert!(set_calendar_settings(&mut slot, &CalendarSettings { entity: Some(Entity::SCorporation), ..Default::default() }).unwrap_err().contains("business form"));
        // Nothing reached the books.
        assert!(slot.as_ref().unwrap().1.audit().entries().iter().all(|e| !e.entity.starts_with("desk")));
    }

    #[test]
    fn us_states_are_checked_and_filed_per_state() {
        let f = TempFile::new("desk-us");
        let (mut slot, ..) = demo(&f);
        set_country(&mut slot, Some(&f.0), "US").unwrap();
        let bad = CalendarSettings { states: vec![StateFiling { state: "ZZ".into(), frequency: Frequency::Monthly }], ..Default::default() };
        assert!(set_calendar_settings(&mut slot, &bad).unwrap_err().contains("US state"));
        let twice = CalendarSettings { states: vec![StateFiling { state: "ca".into(), frequency: Frequency::Monthly }, StateFiling { state: "CA".into(), frequency: Frequency::Quarterly }], ..Default::default() };
        assert!(set_calendar_settings(&mut slot, &twice).unwrap_err().contains("twice"));
        let ok = CalendarSettings { states: vec![StateFiling { state: "ca".into(), frequency: Frequency::Quarterly }, StateFiling { state: "TX".into(), frequency: Frequency::Monthly }], ..Default::default() };
        assert_eq!(set_calendar_settings(&mut slot, &ok).unwrap().company.states[0].state, "CA");
        let oct = due_dates(&slot, "2026-10-01", "2026-10-31").unwrap();
        let ca = oct.iter().find(|d| d.subject.as_deref() == Some("CA")).unwrap();
        assert_eq!(ca.filed_code, "US.SALESTAX.CA");
        mark_filed(&mut slot, &ca.filed_code, &ca.period_to.to_string(), true).unwrap();
        let again = due_dates(&slot, "2026-10-01", "2026-10-31").unwrap();
        let filed: Vec<_> = again.iter().filter(|d| d.code == "US.SALESTAX").map(|d| (d.subject.clone().unwrap(), d.filed)).collect();
        assert!(filed.contains(&("CA".into(), true)) && filed.contains(&("TX".into(), false)), "one state's mark leaves the other open");
    }

    #[test]
    fn a_uk_company_gets_vat_dates_and_no_msme_list() {
        let f = TempFile::new("desk-uk");
        let (mut slot, ..) = demo(&f);
        set_country(&mut slot, Some(&f.0), "GB").unwrap();
        let d = overview(&slot, "2026-04-20", "2026-05-31").unwrap();
        assert!(d.msme.is_empty());
        assert!(d.statutory.iter().any(|s| s.code == "GB.VAT" && s.due.to_string() == "2026-05-07"));
        assert_eq!(d.settings.frequency, Frequency::Quarterly);
        assert!(!d.settings.has_withholding);
    }
}

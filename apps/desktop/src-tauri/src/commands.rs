//! Tauri command wrappers. Each one locks the state and calls into [`api`].
//! Arguments use snake_case on the JavaScript side too (`ledger_id`, `as_at`).

use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use khata_core::reports::{BalanceSheet, DayBookRow, ProfitAndLoss};
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::api::{self, ApiResult, Slot};
use crate::{export, insights, masters, sync, tax};

fn lock<'a>(state: &'a State<'_, AppState>) -> ApiResult<MutexGuard<'a, Slot>> {
    state
        .company
        .lock()
        .map_err(|_| "company state is poisoned".to_owned())
}

/// The open company's lock. Take it after [`lock`], never before.
fn held<'a>(state: &'a State<'_, AppState>) -> ApiResult<MutexGuard<'a, Option<api::CompanyLock>>> {
    state
        .company_lock
        .lock()
        .map_err(|_| "company state is poisoned".to_owned())
}

fn set_path(state: &State<'_, AppState>, path: Option<PathBuf>) {
    if let Ok(mut p) = state.path.lock() {
        *p = path;
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_company(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> ApiResult<api::CompanyInfo> {
    let mut slot = lock(&state)?;
    let out = api::create_company_locked(&mut slot, &mut *held(&state)?, Path::new(&path), &name)?;
    set_path(&state, Some(PathBuf::from(path)));
    Ok(out)
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_company(state: State<'_, AppState>, path: String) -> ApiResult<api::CompanyInfo> {
    let mut slot = lock(&state)?;
    let out = api::open_company_locked(&mut slot, &mut *held(&state)?, Path::new(&path))?;
    set_path(&state, Some(PathBuf::from(path)));
    Ok(out)
}

#[tauri::command(rename_all = "snake_case")]
pub fn close_company(app: AppHandle, state: State<'_, AppState>) -> ApiResult<()> {
    let mut slot = lock(&state)?;
    let job = closing_backup(&app, &state, &slot);
    *slot = None;
    *held(&state)? = None;
    crate::security::sign_out();
    set_path(&state, None);
    // The automatic backup on close runs from the closed file, off the window's thread.
    if let Some(job) = job {
        job.spawn();
    }
    Ok(())
}

/// The automatic backup due as the open company closes, if any.
fn closing_backup(app: &AppHandle, state: &AppState, slot: &Slot) -> Option<crate::backup::CloseJob> {
    let path = state.path.lock().ok().and_then(|p| p.clone())?;
    let env = crate::backup::cmd::env(app).ok()?;
    crate::backup::due_on_close(slot, &env, &path)
}

/// The app is closing with a company open: close it and take its automatic
/// backup now, when one is due.
pub(crate) fn backup_before_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let job = {
        let Ok(mut slot) = state.company.lock() else { return };
        let job = closing_backup(app, &state, &slot);
        if job.is_some() {
            *slot = None;
        }
        job
    };
    if let Some(job) = job {
        match job.run() {
            Ok(d) => eprintln!("automatic backup (on exit): {}", d.path),
            Err(e) => eprintln!("automatic backup (on exit) failed: {e}"),
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn company_info(state: State<'_, AppState>) -> ApiResult<api::CompanyInfo> {
    let path = state.path.lock().ok().and_then(|p| p.clone());
    api::company_info(&*lock(&state)?, path.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn default_company_dir(app: AppHandle) -> ApiResult<String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir = api::default_company_dir(&base);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.display().to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_company_files(dir: String) -> Vec<api::CompanyFile> {
    api::list_company_files(Path::new(&dir))
}

#[tauri::command(rename_all = "snake_case")]
pub fn company_file_name(name: String) -> String {
    api::company_file_name(&name)
}

#[tauri::command(rename_all = "snake_case")]
pub fn search_groups(state: State<'_, AppState>, query: String) -> ApiResult<Vec<api::GroupDto>> {
    api::search_groups(&*lock(&state)?, &query)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_ledgers(
    state: State<'_, AppState>,
    query: String,
    filter: String,
) -> ApiResult<Vec<api::LedgerDto>> {
    api::list_ledgers(&*lock(&state)?, &query, &filter)
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_ledger(
    state: State<'_, AppState>,
    name: String,
    group_id: String,
    opening: String,
    credit_days: Option<u32>,
    tax_id: Option<String>,
    tax_id_override: Option<String>,
) -> ApiResult<api::CreatedLedger> {
    api::create_party_ledger(
        &mut *lock(&state)?,
        &name,
        &group_id,
        &opening,
        credit_days,
        tax_id.as_deref(),
        tax_id_override.as_deref(),
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_countries() -> Vec<api::CountryDto> {
    api::list_countries()
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_country(state: State<'_, AppState>, code: String) -> ApiResult<api::CompanyInfo> {
    let path = state.path.lock().ok().and_then(|p| p.clone());
    api::set_country(&mut *lock(&state)?, path.as_deref(), &code)
}

#[tauri::command(rename_all = "snake_case")]
pub fn check_tax_id(state: State<'_, AppState>, value: String) -> ApiResult<api::TaxIdCheck> {
    api::check_tax_id(&*lock(&state)?, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn post_voucher(
    state: State<'_, AppState>,
    voucher: api::VoucherInput,
) -> ApiResult<api::PostedVoucher> {
    api::post_voucher(&mut *lock(&state)?, voucher)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_voucher(state: State<'_, AppState>, id: String) -> ApiResult<api::VoucherView> {
    api::get_voucher(&*lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn next_voucher_number(
    state: State<'_, AppState>,
    kind: String,
    date: Option<String>,
) -> ApiResult<String> {
    api::next_voucher_number(&*lock(&state)?, &kind, date.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn day_book(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> ApiResult<Vec<DayBookRow>> {
    api::day_book(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn ledger_statement(
    state: State<'_, AppState>,
    ledger_id: String,
    from: String,
    to: String,
) -> ApiResult<api::LedgerStatementDto> {
    api::ledger_statement(&*lock(&state)?, &ledger_id, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn trial_balance(
    state: State<'_, AppState>,
    as_at: Option<String>,
) -> ApiResult<api::TrialBalanceDto> {
    api::trial_balance(&*lock(&state)?, as_at.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn profit_and_loss(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> ApiResult<ProfitAndLoss> {
    api::profit_and_loss(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn balance_sheet(state: State<'_, AppState>, as_at: String) -> ApiResult<BalanceSheet> {
    api::balance_sheet(&*lock(&state)?, &as_at)
}

#[tauri::command(rename_all = "snake_case")]
pub fn group_summary(
    state: State<'_, AppState>,
    group_id: String,
    from: Option<String>,
    to: String,
) -> ApiResult<api::GroupSummaryDto> {
    api::group_summary(&*lock(&state)?, &group_id, from.as_deref(), &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn keymap() -> Vec<api::BindingDto> {
    api::keymap()
}

#[tauri::command(rename_all = "snake_case")]
pub fn quit_app(app: AppHandle, state: State<'_, AppState>) {
    // Android finishes the activity; iOS apps never quit themselves (the
    // Home gesture does), so Quit does nothing there.
    #[cfg(not(target_os = "ios"))]
    {
        backup_before_exit(&app);
        // Release the company's lock now (the OS would at exit anyway).
        if let Ok(mut held) = state.company_lock.lock() {
            *held = None;
        }
        app.exit(0);
    }
    #[cfg(target_os = "ios")]
    let _ = (app, state);
}

/// Uncaught frontend errors, written to stderr so they reach logs and bug reports.
#[tauri::command(rename_all = "snake_case")]
pub fn report_error(message: String) {
    eprintln!("[frontend] {message}");
}

// ------------------------------------------------- masters and vouchers

#[tauri::command(rename_all = "snake_case")]
pub fn create_group(
    state: State<'_, AppState>,
    name: String,
    parent_id: String,
) -> ApiResult<api::GroupDto> {
    api::create_group(&mut *lock(&state)?, &name, &parent_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn alter_voucher(
    state: State<'_, AppState>,
    id: String,
    voucher: api::VoucherInput,
) -> ApiResult<api::PostedVoucher> {
    api::alter_voucher(&mut *lock(&state)?, &id, voucher)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_voucher(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    api::delete_voucher(&mut *lock(&state)?, &id)
}

// ------------------------------------------------------------- bills

#[tauri::command(rename_all = "snake_case")]
pub fn outstanding(
    state: State<'_, AppState>,
    as_at: String,
    side: String,
    ledger_id: Option<String>,
) -> ApiResult<Vec<api::OutstandingDto>> {
    api::outstanding(&*lock(&state)?, &as_at, &side, ledger_id.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn ageing(
    state: State<'_, AppState>,
    as_at: String,
    receivable: bool,
) -> ApiResult<Vec<api::AgeingDto>> {
    api::ageing(&*lock(&state)?, &as_at, receivable)
}

#[tauri::command(rename_all = "snake_case")]
pub fn overdue_payables(
    state: State<'_, AppState>,
    as_at: String,
    days: i64,
) -> ApiResult<Vec<api::OutstandingDto>> {
    api::overdue_payables(&*lock(&state)?, &as_at, days)
}

// ------------------------------------------------------------ import

/// Runs off the window's thread: a large Day Book takes a while to parse
/// and post, and the window keeps painting meanwhile.
#[tauri::command(rename_all = "snake_case")]
pub async fn import_tally(app: AppHandle, paths: Vec<String>) -> ApiResult<export::ImportSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        // On a phone the picker returns URIs: read them through the OS first.
        let local = paths
            .iter()
            .map(|p| crate::phone_files::local_input(&app, p, &["xml"], 200 * 1024 * 1024).map(|l| l.display().to_string()))
            .collect::<ApiResult<Vec<_>>>()?;
        let state = app.state::<AppState>();
        let mut slot = state.company.lock().map_err(|_| "company state is poisoned".to_owned())?;
        // Into a company that already has books: an automatic backup first.
        let path = state.path.lock().ok().and_then(|p| p.clone());
        let env = crate::backup::cmd::env(&app)?;
        let (mut summary, safety) = crate::backup::import_tally(&mut slot, &env, path.as_deref(), &local)?;
        summary.backup = safety.map(|d| d.path);
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

// -------------------------------------------------------------- sync

#[tauri::command(rename_all = "snake_case")]
pub fn sync_settings(state: State<'_, AppState>) -> ApiResult<sync::SyncSettings> {
    sync::settings(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_sync_settings(
    state: State<'_, AppState>,
    url: String,
    company_id: String,
    token: String,
) -> ApiResult<sync::SyncSettings> {
    sync::set_settings(&*lock(&state)?, &url, &company_id, &token)
}

/// Network calls run off the main thread so the window keeps painting.
#[tauri::command(rename_all = "snake_case")]
pub async fn sync_now(app: AppHandle) -> ApiResult<sync::SyncResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut slot = state
            .company
            .lock()
            .map_err(|_| "company state is poisoned".to_owned())?;
        sync::sync_now(&mut slot)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn rebase(app: AppHandle) -> ApiResult<sync::RebaseReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let path = state
            .path
            .lock()
            .ok()
            .and_then(|p| p.clone())
            .ok_or("no company is open")?;
        let mut slot = state
            .company
            .lock()
            .map_err(|_| "company state is poisoned".to_owned())?;
        crate::backup::before_risky(&slot, &crate::backup::cmd::env(&app)?, &path, "sync rebase")?;
        sync::rebase(&mut slot, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------------------------------------------------- print and export

const PRINT_WINDOW: &str = "print";

/// Render a report with khata-print and open it in a print window, which
/// calls `print_window` once the document is in place.
#[tauri::command(rename_all = "snake_case")]
pub fn print_report(
    app: AppHandle,
    state: State<'_, AppState>,
    spec: export::PrintSpec,
) -> ApiResult<()> {
    let lang = khata_print::PrintLanguage::from_code(spec.language.as_deref());
    let html = export::report_html_in(&*lock(&state)?, &spec.report, lang)?;
    open_print(&app, &state, html)
}

/// Print a saved voucher as an invoice, note or voucher. `language` is the
/// frontend's display language ("ar": Arabic, right to left); `hijri` the
/// voucher date already formatted in the Hijri calendar, when the user shows
/// Hijri dates.
#[tauri::command(rename_all = "snake_case")]
pub fn print_voucher(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    language: Option<String>,
    hijri: Option<String>,
) -> ApiResult<()> {
    let lang = khata_print::PrintLanguage::from_code(language.as_deref());
    let html = crate::invoice::voucher_html_in(&*lock(&state)?, &id, lang, hijri.as_deref())?;
    open_print(&app, &state, html)
}

/// Print a saved bank Payment as a cheque.
#[tauri::command(rename_all = "snake_case")]
pub fn print_cheque(app: AppHandle, state: State<'_, AppState>, id: String, account_payee: bool) -> ApiResult<()> {
    let html = crate::invoice::cheque_html(&*lock(&state)?, &id, account_payee)?;
    open_print(&app, &state, html)
}

/// Hand `html` to the print window, opening it if needed.
fn open_print(app: &AppHandle, state: &State<'_, AppState>, html: String) -> ApiResult<()> {
    *state
        .print_html
        .lock()
        .map_err(|_| "print state is poisoned".to_owned())? = Some(html);
    if let Some(w) = app.get_webview_window(PRINT_WINDOW) {
        w.eval("window.location.reload()")
            .map_err(|e| e.to_string())?;
        let _ = w.set_focus();
        return Ok(());
    }
    open_print_window(app)
}

/// Phones have one webview and no print dialog from Tauri: printing (and
/// Save as PDF) stays on the desktop app for now.
#[cfg(mobile)]
fn open_print_window(_app: &AppHandle) -> ApiResult<()> {
    Err("Printing isn't available on phones yet: use Export (CSV) here, or print from KhataOS on a computer".to_owned())
}

#[cfg(desktop)]
fn open_print_window(app: &AppHandle) -> ApiResult<()> {
    tauri::WebviewWindowBuilder::new(
        app,
        PRINT_WINDOW,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .initialization_script("window.__KHATA_PRINT__ = true;")
    .title("KhataOS · Print")
    .inner_size(860.0, 1000.0)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn take_print_html(state: State<'_, AppState>) -> Option<String> {
    state.print_html.lock().ok().and_then(|mut h| h.take())
}

#[tauri::command(rename_all = "snake_case")]
pub fn print_window(window: tauri::WebviewWindow) -> ApiResult<()> {
    print_webview(&window)
}

#[cfg(desktop)]
fn print_webview(window: &tauri::WebviewWindow) -> ApiResult<()> {
    window.print().map_err(|e| e.to_string())
}

#[cfg(mobile)]
fn print_webview(_window: &tauri::WebviewWindow) -> ApiResult<()> {
    Err("Printing isn't available on phones yet".to_owned())
}

#[tauri::command(rename_all = "snake_case")]
pub fn report_csv(state: State<'_, AppState>, spec: export::ReportSpec) -> ApiResult<String> {
    export::report_csv(&*lock(&state)?, &spec)
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_csv(app: AppHandle, path: String, contents: String) -> ApiResult<()> {
    // Phones: the save dialog returns a content:// or file:// URI.
    #[cfg(mobile)]
    if let Some(uri) = crate::phone_files::uri(&path) {
        if !crate::phone_files::name_allowed(&uri, &["csv"]) {
            return Err("exports must be saved as .csv files".into());
        }
        return crate::phone_files::write(&app, uri, &contents);
    }
    let _ = &app;
    export::write_csv(&path, &contents)
}

/// Save a Tally import report (full or shareable) as `.csv` or `.txt`.
#[tauri::command(rename_all = "snake_case")]
pub fn save_import_report(app: AppHandle, path: String, contents: String) -> ApiResult<()> {
    // Phones: the save dialog returns a content:// or file:// URI.
    #[cfg(mobile)]
    if let Some(uri) = crate::phone_files::uri(&path) {
        if !crate::phone_files::name_allowed(&uri, &["csv", "txt"]) {
            return Err("save the report as a .csv or .txt file".into());
        }
        return crate::phone_files::write(&app, uri, &contents);
    }
    let _ = &app;
    export::save_import_report(&path, &contents)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cancel_voucher(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    api::cancel_voucher(&mut *lock(&state)?, &id)
}

// ------------------------------------------- masters, inventory, insights

#[tauri::command(rename_all = "snake_case")]
pub fn get_group(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<api::GroupDto> {
    masters::get_group(&*lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn alter_group(
    state: State<'_, AppState>,
    id: String,
    name: String,
    parent_id: String,
    aliases: Vec<String>,
) -> ApiResult<api::GroupDto> {
    masters::alter_group(&mut *lock(&state)?, &id, &name, &parent_id, &aliases)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_group(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    masters::delete_group(&mut *lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_ledger(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<api::LedgerDto> {
    masters::get_ledger(&*lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn alter_ledger(
    state: State<'_, AppState>,
    id: String,
    ledger: masters::LedgerInput,
) -> ApiResult<api::CreatedLedger> {
    masters::alter_ledger(&mut *lock(&state)?, &id, &ledger)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_ledger(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    masters::delete_ledger(&mut *lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_units(
    state: State<'_, AppState>,
) -> ApiResult<Vec<masters::UnitDto>> {
    masters::list_units(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_unit(
    state: State<'_, AppState>,
    id: String,
    symbol: String,
    name: String,
    decimals: u32,
) -> ApiResult<masters::UnitDto> {
    masters::save_unit(&mut *lock(&state)?, &id, &symbol, &name, decimals)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_unit(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    masters::delete_unit(&mut *lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_stock_groups(
    state: State<'_, AppState>,
    query: String,
) -> ApiResult<Vec<masters::StockGroupDto>> {
    masters::list_stock_groups(&*lock(&state)?, &query)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_stock_group(
    state: State<'_, AppState>,
    id: String,
    name: String,
    parent_id: String,
) -> ApiResult<masters::StockGroupDto> {
    masters::save_stock_group(&mut *lock(&state)?, &id, &name, &parent_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_stock_group(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    masters::delete_stock_group(&mut *lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_stock_items(
    state: State<'_, AppState>,
    query: String,
) -> ApiResult<Vec<masters::StockItemDto>> {
    masters::list_stock_items(&*lock(&state)?, &query)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_stock_item(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<masters::StockItemDto> {
    masters::get_stock_item(&*lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_stock_item(
    state: State<'_, AppState>,
    item: masters::StockItemInput,
) -> ApiResult<masters::StockItemDto> {
    masters::save_stock_item(&mut *lock(&state)?, &item)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_stock_item(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    masters::delete_stock_item(&mut *lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn stock_summary(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> ApiResult<khata_core::inventory::StockSummary> {
    insights::stock_summary(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn stock_item_vouchers(
    state: State<'_, AppState>,
    item_id: String,
    from: String,
    to: String,
) -> ApiResult<insights::StockItemVouchersDto> {
    insights::stock_item_vouchers(&*lock(&state)?, &item_id, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn bank_reconciliation(
    state: State<'_, AppState>,
    ledger_id: String,
    from: String,
    to: String,
) -> ApiResult<insights::BankReconciliationDto> {
    insights::bank_reconciliation(&*lock(&state)?, &ledger_id, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_bank_date(
    state: State<'_, AppState>,
    voucher_id: String,
    ledger_id: String,
    date: String,
) -> ApiResult<()> {
    insights::set_bank_date(&mut *lock(&state)?, &voucher_id, &ledger_id, &date)
}

#[tauri::command(rename_all = "snake_case")]
pub fn ratio_analysis(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> ApiResult<khata_core::analysis::RatioAnalysis> {
    insights::ratio_analysis(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn dashboard(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> ApiResult<khata_core::analysis::Dashboard> {
    insights::dashboard(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_numbering(state: State<'_, AppState>) -> ApiResult<Vec<masters::NumberingDto>> {
    masters::list_numbering(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_numbering(
    state: State<'_, AppState>,
    kind: khata_core::VoucherKind,
    restart: khata_core::Restart,
    prefix: String,
    suffix: String,
    width: u8,
) -> ApiResult<masters::NumberingDto> {
    masters::set_numbering(&mut *lock(&state)?, kind, restart, &prefix, &suffix, width)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_godowns(
    state: State<'_, AppState>,
    query: String,
) -> ApiResult<Vec<masters::GodownDto>> {
    masters::list_godowns(&*lock(&state)?, &query)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_godown(
    state: State<'_, AppState>,
    id: String,
    name: String,
    parent_id: String,
) -> ApiResult<masters::GodownDto> {
    masters::save_godown(&mut *lock(&state)?, &id, &name, &parent_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_godown(
    state: State<'_, AppState>,
    id: String,
) -> ApiResult<()> {
    masters::delete_godown(&mut *lock(&state)?, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn stock_by_location(
    state: State<'_, AppState>,
    as_at: String,
) -> ApiResult<Vec<khata_core::inventory::LocationRow>> {
    insights::stock_by_location(&*lock(&state)?, &as_at)
}

#[tauri::command(rename_all = "snake_case")]
pub fn last_rate(
    state: State<'_, AppState>,
    item_id: String,
    kind: khata_core::VoucherKind,
    party_id: String,
) -> ApiResult<String> {
    insights::last_rate(&*lock(&state)?, &item_id, kind, &party_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn last_narration(
    state: State<'_, AppState>,
    kind: Option<khata_core::VoucherKind>,
    ledger_id: String,
) -> ApiResult<String> {
    insights::last_narration(&*lock(&state)?, kind, &ledger_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn company_tax(
    state: State<'_, AppState>,
) -> ApiResult<tax::CompanyTaxDto> {
    tax::company_tax(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_company_tax(
    state: State<'_, AppState>,
    tax_id: String,
    region: String,
) -> ApiResult<tax::CompanyTaxDto> {
    tax::set_company_tax(&mut *lock(&state)?, &tax_id, &region)
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_tax_rates(
    state: State<'_, AppState>,
) -> ApiResult<Vec<tax::TaxRateDto>> {
    tax::list_tax_rates(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_tax_rate(
    state: State<'_, AppState>,
    class: String,
    from: String,
    percent: String,
) -> ApiResult<Vec<tax::TaxRateDto>> {
    tax::set_tax_rate(&mut *lock(&state)?, &class, &from, &percent)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_ledger_tax(
    state: State<'_, AppState>,
    ledger_id: String,
    component: String,
    side: khata_core::TaxSide,
) -> ApiResult<()> {
    tax::set_ledger_tax(&mut *lock(&state)?, &ledger_id, &component, side)
}

#[tauri::command(rename_all = "snake_case")]
pub fn invoice_tax(
    state: State<'_, AppState>,
    input: tax::InvoiceTaxInput,
) -> ApiResult<tax::InvoiceTaxDto> {
    tax::invoice_tax(&*lock(&state)?, &input)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_company_address(state: State<'_, AppState>, address: String) -> ApiResult<tax::CompanyTaxDto> {
    tax::set_company_address(&mut *lock(&state)?, &address)
}

#[tauri::command(rename_all = "snake_case")]
pub fn match_statement(
    state: State<'_, AppState>,
    ledger_id: String,
    csv: String,
) -> ApiResult<Vec<insights::StatementRowDto>> {
    insights::match_statement(&*lock(&state)?, &ledger_id, &csv)
}

#[tauri::command(rename_all = "snake_case")]
pub fn apply_bank_dates(
    state: State<'_, AppState>,
    ledger_id: String,
    dates: Vec<insights::BankDateInput>,
) -> ApiResult<usize> {
    insights::apply_bank_dates(&mut *lock(&state)?, &ledger_id, &dates)
}

/// Read a text file the user picked (bank statement CSV).
#[tauri::command(rename_all = "snake_case")]
pub fn read_text_file(app: AppHandle, path: String) -> ApiResult<String> {
    // Phones: the open dialog returns a content:// or file:// URI.
    #[cfg(mobile)]
    if let Some(uri) = crate::phone_files::uri(&path) {
        if !crate::phone_files::name_allowed(&uri, &["csv", "txt"]) {
            return Err("pick a .csv or .txt file".into());
        }
        return crate::phone_files::read_text(&app, uri, 20 * 1024 * 1024);
    }
    let _ = &app;
    let ext = Path::new(&path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("csv" | "txt")) {
        return Err("pick a .csv or .txt file".into());
    }
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 20 * 1024 * 1024 {
        return Err("the file is larger than 20 MB".into());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))
}

#[tauri::command(rename_all = "snake_case")]
pub fn session_info(state: State<'_, AppState>) -> ApiResult<crate::security::SessionDto> {
    crate::security::session_info(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn sign_in(state: State<'_, AppState>, name: String, password: String) -> ApiResult<crate::security::SessionDto> {
    crate::security::sign_in(&*lock(&state)?, &name, &password)
}

#[tauri::command(rename_all = "snake_case")]
pub fn sign_out() {
    crate::security::sign_out();
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_users(state: State<'_, AppState>) -> ApiResult<Vec<crate::security::UserDto>> {
    crate::security::list_users(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_user(
    state: State<'_, AppState>,
    name: String,
    role: khata_core::UserRole,
    password: String,
    disabled: bool,
) -> ApiResult<Vec<crate::security::UserDto>> {
    crate::security::save_user(&mut *lock(&state)?, &name, role, &password, disabled)
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_user(state: State<'_, AppState>, name: String) -> ApiResult<Vec<crate::security::UserDto>> {
    crate::security::remove_user(&mut *lock(&state)?, &name)
}

// ------------------------------------------------------------ GST returns

#[tauri::command(rename_all = "snake_case")]
pub fn gstr1(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::gst::Gstr1Dto> {
    crate::gst::gstr1(&*lock(&state)?, &from, &to)
}

/// Draft GSTR-1 JSON (not validated against the portal schema).
#[tauri::command(rename_all = "snake_case")]
pub fn gstr1_json(state: State<'_, AppState>, from: String, to: String) -> ApiResult<String> {
    crate::gst::gstr1_json(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn gstr3b(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::gst::Gstr3bDto> {
    crate::gst::gstr3b(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_gst_json(app: AppHandle, path: String, contents: String) -> ApiResult<()> {
    // Phones: the save dialog returns a content:// or file:// URI.
    #[cfg(mobile)]
    if let Some(uri) = crate::phone_files::uri(&path) {
        if !crate::phone_files::name_allowed(&uri, &["json", "xml"]) {
            return Err("save this file with a .json or .xml name".into());
        }
        return crate::phone_files::write(&app, uri, &contents);
    }
    let _ = &app;
    crate::gst::write_json(&path, &contents)
}

// ------------------------------------------------------------ UAE VAT 201

#[tauri::command(rename_all = "snake_case")]
pub fn vat201(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::vat_ae::Vat201Dto> {
    crate::vat_ae::vat201(&*lock(&state)?, &from, &to)
}

// ------------------------------------------------- Schedule III (India)

/// Schedule III Balance Sheet; `previous_as_at` empty or missing for none.
#[tauri::command(rename_all = "snake_case")]
pub fn schedule3_balance_sheet(
    state: State<'_, AppState>,
    as_at: String,
    previous_as_at: Option<String>,
) -> ApiResult<khata_in::schedule3::AnyBalanceSheet> {
    crate::schedule3::balance_sheet(&*lock(&state)?, &as_at, previous_as_at.as_deref())
}

/// Schedule III Statement of Profit and Loss; the previous period needs
/// both dates (or neither).
#[tauri::command(rename_all = "snake_case")]
pub fn schedule3_profit_and_loss(
    state: State<'_, AppState>,
    from: String,
    to: String,
    prev_from: Option<String>,
    prev_to: Option<String>,
) -> ApiResult<khata_in::schedule3::AnyProfitAndLoss> {
    crate::schedule3::profit_and_loss(
        &*lock(&state)?,
        &from,
        &to,
        prev_from.as_deref(),
        prev_to.as_deref(),
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn print_schedule3_balance_sheet(
    app: AppHandle,
    state: State<'_, AppState>,
    as_at: String,
    previous_as_at: Option<String>,
) -> ApiResult<()> {
    let html = crate::schedule3::balance_sheet_html(
        &*lock(&state)?,
        &as_at,
        previous_as_at.as_deref(),
    )?;
    open_print(&app, &state, html)
}

#[tauri::command(rename_all = "snake_case")]
pub fn print_schedule3_profit_and_loss(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: String,
    prev_from: Option<String>,
    prev_to: Option<String>,
) -> ApiResult<()> {
    let html = crate::schedule3::profit_and_loss_html(
        &*lock(&state)?,
        &from,
        &to,
        prev_from.as_deref(),
        prev_to.as_deref(),
    )?;
    open_print(&app, &state, html)
}

// ------------------------------------------------- e-invoice and e-way bill

/// INV-01 JSON for one Sales or Credit Note voucher (not validated by an
/// IRP). Save it with `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn einvoice_json(
    state: State<'_, AppState>,
    voucher_id: String,
    buyer: crate::einvoice::BuyerDetails,
    hsn_digits: usize,
) -> ApiResult<String> {
    crate::einvoice::einvoice_json(&*lock(&state)?, &voucher_id, &buyer, hsn_digits)
}

/// INV-01 JSON array for the B2B, SEZ and export documents of a period;
/// `buyers` maps a party ledger id to its details.
#[tauri::command(rename_all = "snake_case")]
pub fn einvoice_bulk_json(
    state: State<'_, AppState>,
    from: String,
    to: String,
    buyers: std::collections::HashMap<String, crate::einvoice::BuyerDetails>,
    hsn_digits: usize,
) -> ApiResult<String> {
    crate::einvoice::einvoice_bulk_json(&*lock(&state)?, &from, &to, &buyers, hsn_digits)
}

/// e-Way bill bulk JSON for a Sales or Purchase voucher (Part B optional).
#[tauri::command(rename_all = "snake_case")]
pub fn ewaybill_json(
    state: State<'_, AppState>,
    voucher_id: String,
    details: crate::einvoice::EwayDetails,
    hsn_digits: usize,
) -> ApiResult<String> {
    crate::einvoice::ewaybill_json(&*lock(&state)?, &voucher_id, &details, hsn_digits)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_hsn_digits(state: State<'_, AppState>, digits: Option<u8>) -> ApiResult<tax::CompanyTaxDto> {
    tax::set_hsn_digits(&mut *lock(&state)?, digits)
}

// ------------------------------------------------------------ TDS (India)

/// Natures of payment (contractors, professional fees, rent…) for pickers.
#[tauri::command(rename_all = "snake_case")]
pub fn tds_natures() -> Vec<crate::tds::NatureDto> {
    crate::tds::tds_natures()
}

/// Suggested TDS while entering a voucher; `voucher_id` is the voucher
/// being altered (left out of the year's totals), empty or missing for new.
#[tauri::command(rename_all = "snake_case")]
pub fn tds_for(
    state: State<'_, AppState>,
    date: String,
    party_id: String,
    expense_ledger_id: String,
    amount: String,
    voucher_id: Option<String>,
) -> ApiResult<crate::tds::TdsSuggestion> {
    crate::tds::tds_for(&*lock(&state)?, &date, &party_id, &expense_ledger_id, &amount, voucher_id.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn tds_payable(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::tds::TdsPayableDto> {
    crate::tds::tds_payable(&*lock(&state)?, &from, &to)
}

/// `year`: the year the financial year starts in (2026 for 2026-27).
#[tauri::command(rename_all = "snake_case")]
pub fn tds_quarterly(state: State<'_, AppState>, year: i32, quarter: u32) -> ApiResult<crate::tds::TdsQuarterlyDto> {
    crate::tds::tds_quarterly(&*lock(&state)?, year, quarter)
}

/// The quarterly statement data as CSV (not the FVU file); save it with `write_csv`.
#[tauri::command(rename_all = "snake_case")]
pub fn tds_quarterly_csv(state: State<'_, AppState>, year: i32, quarter: u32) -> ApiResult<String> {
    crate::tds::tds_quarterly_csv(&*lock(&state)?, year, quarter)
}

/// Replace a ledger's PAN and TDS settings (`None` clears each).
#[tauri::command(rename_all = "snake_case")]
pub fn set_ledger_tds(
    state: State<'_, AppState>,
    id: String,
    pan: Option<String>,
    tds: Option<khata_core::LedgerTds>,
) -> ApiResult<api::LedgerDto> {
    crate::tds::set_ledger_tds(&mut *lock(&state)?, &id, pan, tds)
}

// ---------------------------------------------------- Singapore GST F5

#[tauri::command(rename_all = "snake_case")]
pub fn gst_f5(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::gst_sg::GstF5Dto> {
    crate::gst_sg::gst_f5(&*lock(&state)?, &from, &to)
}

// ------------------------------------------------ United Kingdom VAT return

#[tauri::command(rename_all = "snake_case")]
pub fn vat_return_gb(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::vat_gb::VatReturnGbDto> {
    crate::vat_gb::vat_return_gb(&*lock(&state)?, &from, &to)
}

/// The MTD VAT API body for the period (not submitted; `finalised` false).
/// Save it with `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn mtd_vat_json(state: State<'_, AppState>, from: String, to: String, period_key: String) -> ApiResult<String> {
    crate::vat_gb::mtd_vat_json(&*lock(&state)?, &from, &to, &period_key)
}

// ------------------------------------------------ Saudi Arabia (ZATCA, VAT)

/// The ZATCA e-invoice XML for a Sales, Credit Note or customer Debit Note
/// voucher; `kind` is "standard" (B2B) or "simplified" (B2C). The first
/// call takes the device's next ICV and hash; later calls return the same
/// document. Save the XML with `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn zatca_invoice_xml(state: State<'_, AppState>, voucher_id: String, kind: String) -> ApiResult<crate::zatca::ZatcaInvoiceDto> {
    crate::zatca::zatca_invoice_xml(&mut *lock(&state)?, &voucher_id, &kind)
}

#[tauri::command(rename_all = "snake_case")]
pub fn ksa_vat_return(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::zatca::KsaVatReturnDto> {
    crate::zatca::ksa_vat_return(&*lock(&state)?, &from, &to)
}

/// The seller's CRN and National Address for ZATCA (kept in this file's
/// settings until the company profile holds them).
#[tauri::command(rename_all = "snake_case")]
pub fn zatca_seller(state: State<'_, AppState>) -> ApiResult<crate::zatca::ZatcaSellerDto> {
    crate::zatca::zatca_seller(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_zatca_seller(state: State<'_, AppState>, seller: crate::zatca::ZatcaSeller) -> ApiResult<crate::zatca::ZatcaSellerDto> {
    crate::zatca::set_zatca_seller(&mut *lock(&state)?, &seller)
}

// ------------------------------- United Kingdom: Making Tax Digital (HMRC)
//
// Settings, tokens and the device ID live in `<app data>/mtd`, never in the
// books (src/mtd_client.rs). Calls to HMRC run off the main thread; the
// company lock is held only to read the VRN and books and to record receipts.

fn mtd(app: &AppHandle) -> ApiResult<crate::mtd_client::Mtd> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(crate::mtd_client::Mtd::new(base.join("mtd")))
}

async fn mtd_blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&crate::mtd_client::Mtd, &std::sync::Mutex<Slot>) -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let m = mtd(&app)?;
        let state = app.state::<AppState>();
        f(&m, &state.company)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub fn mtd_settings(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::mtd_client::MtdSettingsDto> {
    crate::mtd_client::settings_cmd(&mtd(&app)?, &state.company)
}

/// An empty `client_secret` keeps the saved one; `redirect_port` 0 is the default.
#[tauri::command(rename_all = "snake_case")]
pub fn set_mtd_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    environment: crate::mtd_client::Environment,
    client_id: String,
    client_secret: String,
    redirect_port: u16,
) -> ApiResult<crate::mtd_client::MtdSettingsDto> {
    crate::mtd_client::set_settings_cmd(&mtd(&app)?, &state.company, environment, &client_id, &client_secret, redirect_port)
}

/// Open HMRC's sign-in in the browser and wait (up to 5 minutes) for it.
#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_connect(app: AppHandle) -> ApiResult<crate::mtd_client::MtdSettingsDto> {
    mtd_blocking(app, |m, slot| crate::mtd_client::connect_cmd(m, slot, crate::mtd_client::open_in_browser)).await
}

#[tauri::command(rename_all = "snake_case")]
pub fn mtd_disconnect(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::mtd_client::MtdSettingsDto> {
    crate::mtd_client::disconnect_cmd(&mtd(&app)?, &state.company)
}

/// `status`: "O", "F" or none (then `from` and `to` are needed).
#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_obligations(
    app: AppHandle,
    from: String,
    to: String,
    status: Option<String>,
    client_info: crate::mtd_client::ClientInfo,
    test_scenario: Option<String>,
) -> ApiResult<Vec<crate::mtd_client::ObligationDto>> {
    mtd_blocking(app, move |m, slot| {
        crate::mtd_client::obligations_cmd(m, slot, &from, &to, status.as_deref(), client_info, test_scenario)
    })
    .await
}

/// Submit the books' return for an open obligation (`finalised: true`);
/// refused unless `declaration_accepted`.
#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_submit(
    app: AppHandle,
    period_key: String,
    declaration_accepted: bool,
    client_info: crate::mtd_client::ClientInfo,
    test_scenario: Option<String>,
) -> ApiResult<crate::mtd_client::SubmissionReceipt> {
    mtd_blocking(app, move |m, slot| {
        crate::mtd_client::submit_cmd(m, slot, &period_key, declaration_accepted, client_info, test_scenario)
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_view_return(
    app: AppHandle,
    period_key: String,
    client_info: crate::mtd_client::ClientInfo,
) -> ApiResult<crate::mtd_client::ViewedReturnDto> {
    mtd_blocking(app, move |m, slot| crate::mtd_client::view_return_cmd(m, slot, &period_key, client_info)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_liabilities(
    app: AppHandle,
    from: String,
    to: String,
    client_info: crate::mtd_client::ClientInfo,
    test_scenario: Option<String>,
) -> ApiResult<Vec<crate::mtd_client::LiabilityDto>> {
    mtd_blocking(app, move |m, slot| crate::mtd_client::liabilities_cmd(m, slot, &from, &to, client_info, test_scenario)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_payments(
    app: AppHandle,
    from: String,
    to: String,
    client_info: crate::mtd_client::ClientInfo,
    test_scenario: Option<String>,
) -> ApiResult<Vec<crate::mtd_client::PaymentDto>> {
    mtd_blocking(app, move |m, slot| crate::mtd_client::payments_cmd(m, slot, &from, &to, client_info, test_scenario)).await
}

/// HMRC's Test Fraud Prevention Headers API (sandbox only).
#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_test_headers(
    app: AppHandle,
    client_info: crate::mtd_client::ClientInfo,
) -> ApiResult<crate::mtd_client::HeaderCheckDto> {
    mtd_blocking(app, move |m, _| crate::mtd_client::test_headers_cmd(m, client_info)).await
}

/// HMRC's feedback on the latest sandbox call to each VAT endpoint.
#[tauri::command(rename_all = "snake_case")]
pub async fn mtd_header_feedback(app: AppHandle) -> ApiResult<Vec<crate::mtd_client::FeedbackDto>> {
    mtd_blocking(app, |m, _| m.header_feedback()).await
}

// ------------------------------------------------ United States (sales tax, 1099)

/// The sales tax liability report by jurisdiction and state.
#[tauri::command(rename_all = "snake_case")]
pub fn sales_tax_report_us(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::tax_us::SalesTaxReportUsDto> {
    crate::tax_us::sales_tax_report_us(&*lock(&state)?, &from, &to)
}

/// Payees at or over the Form 1099 threshold for a calendar year.
#[tauri::command(rename_all = "snake_case")]
pub fn form1099_us(state: State<'_, AppState>, year: i32) -> ApiResult<khata_us::form1099::Form1099Report> {
    crate::tax_us::form1099_us(&*lock(&state)?, year)
}

// ------------------------------------------------ Mexico (IVA, CFDI 4.0)

/// The CSD lives in the app data directory, never in the books.
fn mx_dir(app: &AppHandle) -> ApiResult<PathBuf> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn iva_summary_mx(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::tax_mx::IvaSummaryMxDto> {
    crate::tax_mx::iva_summary_mx(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn mx_issuer(state: State<'_, AppState>) -> ApiResult<crate::tax_mx::MxIssuer> {
    crate::tax_mx::mx_issuer(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_mx_issuer(state: State<'_, AppState>, issuer: crate::tax_mx::MxIssuer) -> ApiResult<crate::tax_mx::MxIssuer> {
    crate::tax_mx::set_mx_issuer(&mut *lock(&state)?, &issuer)
}

#[tauri::command(rename_all = "snake_case")]
pub fn mx_csd_status(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::tax_mx::CsdStatus> {
    crate::tax_mx::mx_csd_status(&mx_dir(&app)?, &*lock(&state)?)
}

/// Install the CSD (.cer and encrypted .key); the password is only checked.
#[tauri::command(rename_all = "snake_case")]
pub fn set_mx_csd(
    app: AppHandle,
    state: State<'_, AppState>,
    cer_path: String,
    key_path: String,
    password: String,
) -> ApiResult<crate::tax_mx::CsdStatus> {
    crate::tax_mx::set_mx_csd(&mx_dir(&app)?, &*lock(&state)?, &cer_path, &key_path, &password)
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_mx_csd(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::tax_mx::CsdStatus> {
    crate::tax_mx::clear_mx_csd(&mx_dir(&app)?, &*lock(&state)?)
}

/// The CFDI 4.0 XML for a Sales or Credit Note voucher (sealed when the
/// CSD password is given; never stamped). Save it with `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn cfdi_xml(
    app: AppHandle,
    state: State<'_, AppState>,
    voucher_id: String,
    options: crate::tax_mx::CfdiOptions,
) -> ApiResult<crate::tax_mx::CfdiDto> {
    crate::tax_mx::cfdi_xml(&mx_dir(&app)?, &*lock(&state)?, &voucher_id, &options)
}

// ------------------------------------------- IRP connector (India e-invoice API)

fn irp_dir(app: &AppHandle) -> ApiResult<PathBuf> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// IRP calls run off the main thread; the company lock is taken only to
/// build the request and to store the answer.
async fn irp_blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Path, &std::sync::Mutex<Slot>) -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = irp_dir(&app)?;
        let state = app.state::<AppState>();
        f(&dir, &state.company)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The IRP connection for the company's GSTIN; secrets only as "set or not".
#[tauri::command(rename_all = "snake_case")]
pub fn irp_settings(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::irp_client::IrpSettingsDto> {
    crate::irp_client::settings(&irp_dir(&app)?, &*lock(&state)?)
}

/// Save the IRP connection (outside the books); empty secrets keep the stored ones.
#[tauri::command(rename_all = "snake_case")]
pub fn set_irp_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: crate::irp_client::IrpSettingsInput,
) -> ApiResult<crate::irp_client::IrpSettingsDto> {
    crate::irp_client::set_settings(&irp_dir(&app)?, &*lock(&state)?, &settings)
}

/// The stored IRN for a voucher, without calling the IRP (null when none).
#[tauri::command(rename_all = "snake_case")]
pub fn irp_record(app: AppHandle, state: State<'_, AppState>, voucher_id: String) -> ApiResult<Option<crate::irp_client::IrnDto>> {
    crate::irp_client::record(&irp_dir(&app)?, &*lock(&state)?, &voucher_id)
}

/// Register a Sales or Credit Note voucher with the IRP and keep its IRN.
#[tauri::command(rename_all = "snake_case")]
pub async fn irp_generate(
    app: AppHandle,
    voucher_id: String,
    buyer: Option<crate::einvoice::BuyerDetails>,
) -> ApiResult<crate::irp_client::IrnDto> {
    irp_blocking(app, move |dir, company| crate::irp_client::generate(dir, company, &voucher_id, &buyer.unwrap_or_default())).await
}

/// Cancel the voucher's IRN (reason "1" to "4", remark up to 100 characters; within 24 hours).
#[tauri::command(rename_all = "snake_case")]
pub async fn irp_cancel(app: AppHandle, voucher_id: String, reason: String, remark: String) -> ApiResult<crate::irp_client::IrnDto> {
    irp_blocking(app, move |dir, company| crate::irp_client::cancel(dir, company, &voucher_id, &reason, &remark)).await
}

/// Fetch the IRN's details from the IRP again.
#[tauri::command(rename_all = "snake_case")]
pub async fn irp_details(app: AppHandle, voucher_id: String) -> ApiResult<crate::irp_client::IrnDto> {
    irp_blocking(app, move |dir, company| crate::irp_client::details(dir, company, &voucher_id)).await
}

/// Generate an e-way bill for the voucher's IRN.
#[tauri::command(rename_all = "snake_case")]
pub async fn irp_ewaybill(app: AppHandle, voucher_id: String, details: khata_in::irp::EwbByIrn) -> ApiResult<crate::irp_client::IrnDto> {
    irp_blocking(app, move |dir, company| crate::irp_client::ewaybill(dir, company, &voucher_id, &details)).await
}

/// A GSTIN as the e-invoice system has it.
#[tauri::command(rename_all = "snake_case")]
pub async fn irp_gstin(app: AppHandle, gstin: String) -> ApiResult<khata_in::irp::GstinDetails> {
    irp_blocking(app, move |dir, company| crate::irp_client::gstin_details(dir, company, &gstin)).await
}

// ------------------------------------- Saudi Arabia: ZATCA onboarding and submission

fn zatca_dir(app: &AppHandle) -> ApiResult<PathBuf> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Network calls to ZATCA run off the main thread; the company lock is
/// taken only around the local reads and writes.
async fn zatca_blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Path, &std::sync::Mutex<Slot>) -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = zatca_dir(&app)?;
        let state = app.state::<AppState>();
        f(&dir, &state.company)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// This computer's ZATCA device: environment, CSR, CSIDs (as present or
/// not, never the secrets), compliance checks, the credentials file path.
#[tauri::command(rename_all = "snake_case")]
pub fn zatca_status(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::zatca::ZatcaStatusDto> {
    crate::zatca::zatca_status(&*lock(&state)?, &zatca_dir(&app)?)
}

/// Onboard this computer: `environment` is "sandbox", "simulation" or
/// "production"; `otp` from the Fatoora portal (empty to resume).
#[tauri::command(rename_all = "snake_case")]
pub async fn zatca_onboard(
    app: AppHandle,
    environment: String,
    otp: String,
    options: Option<crate::zatca::OnboardOptions>,
) -> ApiResult<crate::zatca::ZatcaStatusDto> {
    zatca_blocking(app, move |dir, company| {
        crate::zatca::zatca_onboard(company, dir, &environment, &otp, &options.unwrap_or_default())
    })
    .await
}

/// Stamp (if needed) and send a voucher's e-invoice: clearance for a
/// standard invoice, reporting for a simplified one. `kind` is used only
/// the first time (else the buyer's VAT number decides).
#[tauri::command(rename_all = "snake_case")]
pub async fn zatca_submit(app: AppHandle, voucher_id: String, kind: Option<String>) -> ApiResult<crate::zatca::ZatcaSubmissionDto> {
    zatca_blocking(app, move |dir, company| crate::zatca::zatca_submit(company, dir, &voucher_id, kind.as_deref())).await
}

/// ZATCA's stored answer for a voucher (null when never submitted).
#[tauri::command(rename_all = "snake_case")]
pub fn zatca_submission(state: State<'_, AppState>, voucher_id: String) -> ApiResult<Option<crate::zatca::ZatcaSubmissionDto>> {
    crate::zatca::zatca_submission(&*lock(&state)?, &voucher_id)
}

/// [`zatca_invoice_xml`], stamped once the device is onboarded.
#[tauri::command(rename_all = "snake_case")]
pub fn zatca_invoice_xml_stamped(
    app: AppHandle,
    state: State<'_, AppState>,
    voucher_id: String,
    kind: String,
) -> ApiResult<crate::zatca::ZatcaInvoiceDto> {
    crate::zatca::zatca_invoice_xml_stamped(&mut *lock(&state)?, &zatca_dir(&app)?, &voucher_id, &kind)
}

// ------------------------------------------------ Germany (UStVA, XRechnung)

/// The UStVA Kennzahlen for a period (a preparation aid; not sent to ELSTER).
#[tauri::command(rename_all = "snake_case")]
pub fn ustva(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::vat_de::UstvaDto> {
    crate::vat_de::ustva(&*lock(&state)?, &from, &to)
}

/// The XRechnung 3.0 XML for a Sales or Credit Note voucher; `syntax` is
/// "ubl" or "cii". Save it with `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn xrechnung_xml(state: State<'_, AppState>, voucher_id: String, syntax: String) -> ApiResult<crate::vat_de::XRechnungDto> {
    crate::vat_de::xrechnung_xml(&*lock(&state)?, &voucher_id, &syntax)
}

// ------------------------------------------------ TCS, Forms 143/144, salary (India)

#[tauri::command(rename_all = "snake_case")]
pub fn tcs_natures() -> Vec<crate::tds::TcsNatureDto> {
    crate::tds::tcs_natures()
}

/// Suggested TCS for a Sales or Receipt voucher being entered.
#[tauri::command(rename_all = "snake_case")]
pub fn tcs_for(
    state: State<'_, AppState>,
    date: String,
    party_id: String,
    sales_ledger_id: String,
    amount: String,
) -> ApiResult<crate::tds::TcsSuggestion> {
    crate::tds::tcs_for(&*lock(&state)?, &date, &party_id, &sales_ledger_id, &amount)
}

#[tauri::command(rename_all = "snake_case")]
pub fn tcs_payable(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::tds::TdsPayableDto> {
    crate::tds::tcs_payable(&*lock(&state)?, &from, &to)
}

#[tauri::command(rename_all = "snake_case")]
pub fn tcs_quarterly(state: State<'_, AppState>, year: i32, quarter: u32) -> ApiResult<crate::tds::TcsQuarterlyDto> {
    crate::tds::tcs_quarterly(&*lock(&state)?, year, quarter)
}

#[tauri::command(rename_all = "snake_case")]
pub fn tds_quarterly_nr(state: State<'_, AppState>, year: i32, quarter: u32) -> ApiResult<crate::tds::NonResidentQuarterlyDto> {
    crate::tds::tds_quarterly_nr(&*lock(&state)?, year, quarter)
}

/// Form 144 country codes and natures of remittance, as `[countries, remittances]`.
#[tauri::command(rename_all = "snake_case")]
pub fn tds_nr_codes() -> (Vec<crate::tds::CodeName>, Vec<crate::tds::CodeName>) {
    crate::tds::tds_nr_codes()
}

#[tauri::command(rename_all = "snake_case")]
pub fn ledger_tcs(state: State<'_, AppState>, id: String) -> ApiResult<Option<khata_core::chart::LedgerTcs>> {
    crate::tds::ledger_tcs(&*lock(&state)?, &id)
}

/// Replace a ledger's TCS settings (`None` clears them).
#[tauri::command(rename_all = "snake_case")]
pub fn set_ledger_tcs(
    state: State<'_, AppState>,
    id: String,
    tcs: Option<khata_core::chart::LedgerTcs>,
) -> ApiResult<Option<khata_core::chart::LedgerTcs>> {
    crate::tds::set_ledger_tcs(&mut *lock(&state)?, &id, tcs)
}

/// The deductor's TAN, address and responsible person (this file's settings).
#[tauri::command(rename_all = "snake_case")]
pub fn tds_deductor(state: State<'_, AppState>) -> ApiResult<crate::tds::DeductorDto> {
    crate::tds::tds_deductor(&*lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_tds_deductor(state: State<'_, AppState>, settings: crate::tds::DeductorSettings) -> ApiResult<crate::tds::DeductorDto> {
    crate::tds::set_tds_deductor(&mut *lock(&state)?, settings)
}

/// A TDS/TCS challan voucher's bank details (BSR code, serial, date).
#[tauri::command(rename_all = "snake_case")]
pub fn tds_challan(state: State<'_, AppState>, voucher_id: String) -> ApiResult<Option<crate::tds::ChallanDetails>> {
    crate::tds::tds_challan(&*lock(&state)?, &voucher_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_tds_challan(
    state: State<'_, AppState>,
    voucher_id: String,
    details: Option<crate::tds::ChallanDetails>,
) -> ApiResult<Option<crate::tds::ChallanDetails>> {
    crate::tds::set_tds_challan(&mut *lock(&state)?, &voucher_id, details)
}

/// The statement text file for Form "140", "143" or "144" (problems instead
/// of a file when details are missing). Save it with `write_tds_file`.
#[tauri::command(rename_all = "snake_case")]
pub fn tds_statement_file(
    state: State<'_, AppState>,
    form: String,
    year: i32,
    quarter: u32,
    previous_token: Option<String>,
) -> ApiResult<crate::tds::StatementFileDto> {
    crate::tds::tds_statement_file(&mut *lock(&state)?, &form, year, quarter, previous_token.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_tds_file(path: String, contents: String) -> ApiResult<()> {
    crate::tds::write_statement_file(&path, &contents)
}

/// Salary TDS estimate (new regime); an estimate, not payroll.
#[tauri::command(rename_all = "snake_case")]
pub fn salary_estimate(input: crate::tds::SalaryEstimateInput) -> ApiResult<khata_in::salary::SalaryEstimate> {
    crate::tds::salary_estimate(&input)
}

// ---------------------------------------- GST: reverse charge, ITC, 2B, 1A

/// Reverse-charge purchases of a period, with self-invoice numbers (3.1(d)).
#[tauri::command(rename_all = "snake_case")]
pub fn rcm_register(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::gst_rcm::RcmRegisterDto> {
    crate::gst_rcm::rcm_register(&*lock(&state)?, &from, &to)
}

/// Print the self-invoice for a reverse-charge purchase from an unregistered supplier.
#[tauri::command(rename_all = "snake_case")]
pub fn print_self_invoice(app: AppHandle, state: State<'_, AppState>, id: String) -> ApiResult<()> {
    let html = crate::gst_rcm::self_invoice_html(&*lock(&state)?, &id)?;
    open_print(&app, &state, html)
}

/// The self-invoice as HTML (for sharing as a file).
#[tauri::command(rename_all = "snake_case")]
pub fn self_invoice_html(state: State<'_, AppState>, id: String) -> ApiResult<String> {
    crate::gst_rcm::self_invoice_html(&*lock(&state)?, &id)
}

/// Purchases unpaid 180 days after the invoice, with ITC to reverse (Rule 37).
#[tauri::command(rename_all = "snake_case")]
pub fn rule37_report(state: State<'_, AppState>, as_at: String) -> ApiResult<crate::gst_rcm::Rule37Dto> {
    crate::gst_rcm::rule37_report(&*lock(&state)?, &as_at)
}

/// Rule 42 common-credit split for one tax.
#[tauri::command(rename_all = "snake_case")]
pub fn rule42_calc(input: crate::gst_rcm::Rule42Strings) -> ApiResult<khata_in::rcm::Rule42> {
    crate::gst_rcm::rule42_calc(&input)
}

/// Ledgers and stock items with their reverse-charge and blocked-credit flags.
#[tauri::command(rename_all = "snake_case")]
pub fn gst_flag_list(state: State<'_, AppState>) -> ApiResult<Vec<crate::gst_rcm::GstFlagRow>> {
    crate::gst_rcm::gst_flag_list(&*lock(&state)?)
}

/// `kind`: "ledger" or "item".
#[tauri::command(rename_all = "snake_case")]
pub fn set_gst_flags(
    state: State<'_, AppState>,
    kind: String,
    id: String,
    reverse_charge: bool,
    blocked: bool,
) -> ApiResult<crate::gst_rcm::GstFlagRow> {
    crate::gst_rcm::set_gst_flags(&mut *lock(&state)?, &kind, &id, reverse_charge, blocked)
}

/// Mark a GSTR-1 period (YYYY-MM) filed now, or clear it (`filed` false; default true).
#[tauri::command(rename_all = "snake_case")]
pub fn gstr1_mark_filed(state: State<'_, AppState>, period: String, filed: Option<bool>) -> ApiResult<Vec<crate::gst::FiledPeriodDto>> {
    crate::gst::gstr1_mark_filed(&mut *lock(&state)?, &period, filed.unwrap_or(true))
}

#[tauri::command(rename_all = "snake_case")]
pub fn gstr1_filed_periods(state: State<'_, AppState>) -> ApiResult<Vec<crate::gst::FiledPeriodDto>> {
    crate::gst::gstr1_filed_periods(&*lock(&state)?)
}

/// Outward documents changed after their GSTR-1 period was marked filed (GSTR-1A candidates).
#[tauri::command(rename_all = "snake_case")]
pub fn gstr1_amendments(state: State<'_, AppState>) -> ApiResult<Vec<crate::gst::AmendmentDto>> {
    crate::gst::gstr1_amendments(&*lock(&state)?)
}

/// Reconcile a GSTR-2B JSON (its text) with the purchases of a period.
#[tauri::command(rename_all = "snake_case")]
pub fn gstr2b_reconcile(
    state: State<'_, AppState>,
    json_text: String,
    from: String,
    to: String,
    days: Option<i64>,
    amount: Option<String>,
) -> ApiResult<crate::gstr2b::Gstr2bDto> {
    crate::gstr2b::gstr2b_reconcile(&*lock(&state)?, &json_text, &from, &to, days, amount.as_deref())
}

/// Read the GSTR-2B .json the user picked (`read_text_file` takes .csv/.txt only).
#[tauri::command(rename_all = "snake_case")]
pub fn read_gstr2b_file(app: AppHandle, path: String) -> ApiResult<String> {
    #[cfg(mobile)]
    if let Some(uri) = crate::phone_files::uri(&path) {
        if !crate::phone_files::name_allowed(&uri, &["json"]) {
            return Err("pick the GSTR-2B .json file".into());
        }
        return crate::phone_files::read_text(&app, uri, 20 * 1024 * 1024);
    }
    let _ = &app;
    crate::gstr2b::read_json_file(&path)
}

// ------------------------------------------------------- GSTR-9 (India)

/// GSTR-9 for the financial year starting in April `year` (2025 for 2025-26).
#[tauri::command(rename_all = "snake_case")]
pub fn gstr9(state: State<'_, AppState>, year: i32) -> ApiResult<crate::gstr9::Gstr9Dto> {
    crate::gstr9::gstr9(&*lock(&state)?, year)
}

/// Save a row's hand entries (column key → amount), or clear them (`values` null or blank).
#[tauri::command(rename_all = "snake_case")]
pub fn gstr9_set_row(
    state: State<'_, AppState>,
    year: i32,
    row: String,
    values: Option<std::collections::BTreeMap<String, String>>,
) -> ApiResult<crate::gstr9::Gstr9Dto> {
    crate::gstr9::gstr9_set_row(&mut *lock(&state)?, year, &row, values)
}

/// The vouchers behind a GSTR-9 row ("4B") or an HSN line's key.
#[tauri::command(rename_all = "snake_case")]
pub fn gstr9_docs(state: State<'_, AppState>, year: i32, key: String) -> ApiResult<Vec<khata_in::gstr9::DocRef>> {
    crate::gstr9::gstr9_docs(&*lock(&state)?, year, &key)
}

/// GSTR-9 in the GSTN JSON layout (needs validation in the GST offline tool). Save it with write_gst_json.
#[tauri::command(rename_all = "snake_case")]
pub fn gstr9_json(state: State<'_, AppState>, year: i32) -> ApiResult<String> {
    crate::gstr9::gstr9_json(&*lock(&state)?, year)
}

/// One GSTR-9 table as CSV ("4" … "19", "9C-5", "9C-7", "all"). Save it with write_csv.
#[tauri::command(rename_all = "snake_case")]
pub fn gstr9_csv(state: State<'_, AppState>, year: i32, table: String) -> ApiResult<String> {
    crate::gstr9::gstr9_csv(&*lock(&state)?, year, &table)
}

/// GSTR-1 Table 12 (HSN summary) for a period as CSV; `which` "b2b", "b2c" or "all".
#[tauri::command(rename_all = "snake_case")]
pub fn gstr1_hsn_csv(state: State<'_, AppState>, from: String, to: String, which: String) -> ApiResult<String> {
    crate::gstr9::gstr1_hsn_csv(&*lock(&state)?, &from, &to, &which)
}

/// Compare the books' GSTR-9 with files downloaded from the GST portal (their names and text).
#[tauri::command(rename_all = "snake_case")]
pub fn gstr9_reconcile(
    state: State<'_, AppState>,
    year: i32,
    files: Vec<crate::gstr9::PickedFile>,
    tolerance: Option<String>,
) -> ApiResult<crate::gstr9::Gstr9ReconDto> {
    crate::gstr9::gstr9_reconcile(&*lock(&state)?, year, &files, tolerance.as_deref())
}

/// Read a return .json the user picked (GSTR-9, GSTR-1, GSTR-3B or GSTR-2B), on phones too.
#[tauri::command(rename_all = "snake_case")]
pub fn read_gst_return_file(app: AppHandle, path: String) -> ApiResult<String> {
    #[cfg(mobile)]
    if let Some(uri) = crate::phone_files::uri(&path) {
        if !crate::phone_files::name_allowed(&uri, &["json"]) {
            return Err("pick a .json file downloaded from the GST portal".into());
        }
        return crate::phone_files::read_text(&app, uri, 20 * 1024 * 1024);
    }
    let _ = &app;
    crate::gstr9::read_json_file(&path)
}

// ------------------------------- Schedule III notes and mapping (India)

/// Schedule III statements with their notes and ratios, as at `as_at`.
#[tauri::command(rename_all = "snake_case")]
pub fn schedule3_statements(
    state: State<'_, AppState>,
    as_at: String,
    previous_as_at: Option<String>,
) -> ApiResult<khata_in::schedule3_notes::AnyStatements> {
    crate::schedule3::statements(&*lock(&state)?, &as_at, previous_as_at.as_deref())
}

#[tauri::command(rename_all = "snake_case")]
pub fn print_schedule3_notes(
    app: AppHandle,
    state: State<'_, AppState>,
    as_at: String,
    previous_as_at: Option<String>,
) -> ApiResult<()> {
    let html = crate::schedule3::notes_html(&*lock(&state)?, &as_at, previous_as_at.as_deref())?;
    open_print(&app, &state, html)
}

/// Where every group goes on the Schedule III statements.
#[tauri::command(rename_all = "snake_case")]
pub fn schedule3_mapping(state: State<'_, AppState>) -> ApiResult<crate::schedule3::MappingDto> {
    crate::schedule3::mapping(&*lock(&state)?)
}

/// The Schedule III lines a group can map to.
#[tauri::command(rename_all = "snake_case")]
pub fn schedule3_line_options(
    state: State<'_, AppState>,
    group_id: String,
) -> ApiResult<Vec<khata_in::schedule3::LineOption>> {
    crate::schedule3::line_options(&*lock(&state)?, &group_id)
}

/// Map a group to a Schedule III line code; none restores the default.
#[tauri::command(rename_all = "snake_case")]
pub fn set_schedule3_mapping(
    state: State<'_, AppState>,
    group_id: String,
    code: Option<String>,
) -> ApiResult<crate::schedule3::MappingDto> {
    crate::schedule3::set_mapping(&mut *lock(&state)?, &group_id, code.as_deref())
}

/// Mark a party as a micro or small enterprise (or clear the mark).
#[tauri::command(rename_all = "snake_case")]
pub fn set_schedule3_msme(
    state: State<'_, AppState>,
    ledger_id: String,
    msme: bool,
) -> ApiResult<crate::schedule3::MappingDto> {
    crate::schedule3::set_msme(&mut *lock(&state)?, &ledger_id, msme)
}

/// Schedule III division (1 or 2) and rounding unit ("rupees", "lakhs"…).
#[tauri::command(rename_all = "snake_case")]
pub fn set_schedule3_format(
    state: State<'_, AppState>,
    division: u8,
    rounding: String,
) -> ApiResult<crate::schedule3::MappingDto> {
    crate::schedule3::set_format(&mut *lock(&state)?, division, &rounding)
}

// ------------------------------- Mexico: PAC timbrado, cancellation, payment receipts

/// The payment receipt (CFDI de tipo P, Complemento de Pagos 2.0) for a
/// Receipt voucher settling PPD invoices bill by bill.
#[tauri::command(rename_all = "snake_case")]
pub fn pago_xml(
    app: AppHandle,
    state: State<'_, AppState>,
    voucher_id: String,
    options: crate::tax_mx::CfdiOptions,
) -> ApiResult<crate::tax_mx::CfdiDto> {
    crate::tax_mx::pago_xml(&mx_dir(&app)?, &*lock(&state)?, &voucher_id, &options)
}

/// The PAC connection for the company's RFC; secrets only as "set or not".
#[tauri::command(rename_all = "snake_case")]
pub fn pac_settings(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::pac_client::PacSettingsDto> {
    crate::pac_client::settings(&mx_dir(&app)?, &*lock(&state)?)
}

/// Save the PAC connection (app data, 0600; never in the books).
#[tauri::command(rename_all = "snake_case")]
pub fn set_pac_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: crate::pac_client::PacSettingsInput,
) -> ApiResult<crate::pac_client::PacSettingsDto> {
    crate::pac_client::set_settings(&mx_dir(&app)?, &*lock(&state)?, &settings)
}

/// The stored stamp of a voucher (null when not stamped).
#[tauri::command(rename_all = "snake_case")]
pub fn pac_record(state: State<'_, AppState>, voucher_id: String) -> ApiResult<Option<crate::pac_client::PacDto>> {
    crate::pac_client::record(&*lock(&state)?, &voucher_id)
}

/// Seal a Sales, Credit Note or Receipt voucher's CFDI and have the PAC stamp it.
#[tauri::command(rename_all = "snake_case")]
pub async fn pac_stamp(app: AppHandle, voucher_id: String, options: crate::tax_mx::CfdiOptions) -> ApiResult<crate::pac_client::PacDto> {
    irp_blocking(app, move |dir, company| crate::pac_client::stamp(dir, company, &voucher_id, &options)).await
}

/// Ask the PAC to cancel a stamped CFDI (motivo 01–04; folio de sustitución with 01).
#[tauri::command(rename_all = "snake_case")]
pub async fn pac_cancel(
    app: AppHandle,
    voucher_id: String,
    motivo: String,
    folio_sustitucion: String,
    password: String,
) -> ApiResult<crate::pac_client::PacDto> {
    irp_blocking(app, move |dir, company| crate::pac_client::cancel(dir, company, &voucher_id, &motivo, &folio_sustitucion, &password)).await
}

/// Record the SAT's answer to a cancellation request (cancelled, or still in force).
#[tauri::command(rename_all = "snake_case")]
pub fn pac_confirm_cancellation(state: State<'_, AppState>, voucher_id: String, cancelled: bool) -> ApiResult<crate::pac_client::PacDto> {
    crate::pac_client::confirm_cancellation(&state.company, &voucher_id, cancelled)
}

// ------------------------------- United States: use tax, exemption certificates

/// Use tax to self-assess on untaxed purchases from vendors outside the state.
#[tauri::command(rename_all = "snake_case")]
pub fn use_tax_report_us(
    state: State<'_, AppState>,
    from: String,
    to: String,
    jurisdiction: Option<String>,
) -> ApiResult<crate::tax_us::UseTaxReportUsDto> {
    crate::tax_us::use_tax_report_us(&*lock(&state)?, &from, &to, jurisdiction.as_deref())
}

/// A customer's resale and exemption certificates.
#[tauri::command(rename_all = "snake_case")]
pub fn us_certificates(state: State<'_, AppState>, ledger_id: String) -> ApiResult<Vec<khata_us::exemptions::Certificate>> {
    crate::tax_us::us_certificates(&*lock(&state)?, &ledger_id)
}

/// Replace a customer's certificates (kept on this computer; not synced).
#[tauri::command(rename_all = "snake_case")]
pub fn set_us_certificates(
    state: State<'_, AppState>,
    ledger_id: String,
    certificates: Vec<khata_us::exemptions::Certificate>,
) -> ApiResult<Vec<khata_us::exemptions::Certificate>> {
    crate::tax_us::set_us_certificates(&mut *lock(&state)?, &ledger_id, &certificates)
}

/// Every certificate on file, expired and expiring first.
#[tauri::command(rename_all = "snake_case")]
pub fn certificate_report_us(state: State<'_, AppState>, as_of: String, within_days: i64) -> ApiResult<khata_us::exemptions::CertificateReport> {
    crate::tax_us::certificate_report_us(&*lock(&state)?, &as_of, within_days)
}

// ------------------------------------------------ France, Netherlands, Spain VAT

/// France: the CA3 lines for a period (a preparation aid; nothing is filed).
#[tauri::command(rename_all = "snake_case")]
pub fn ca3_fr(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::vat_fr::Ca3Dto> {
    crate::vat_fr::ca3_fr(&*lock(&state)?, &from, &to)
}

/// Netherlands: the aangifte omzetbelasting rubrieken for a period.
#[tauri::command(rename_all = "snake_case")]
pub fn aangifte_nl(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::vat_nl::AangifteDto> {
    crate::vat_nl::aangifte_nl(&*lock(&state)?, &from, &to)
}

/// Spain: the modelo 303 casillas for a period.
#[tauri::command(rename_all = "snake_case")]
pub fn modelo_303_es(state: State<'_, AppState>, from: String, to: String) -> ApiResult<crate::vat_es::Modelo303Dto> {
    crate::vat_es::modelo_303_es(&*lock(&state)?, &from, &to)
}

/// Spain: the VeriFactu record (alta, or anulación for a cancelled voucher)
/// of a Sales or Credit Note voucher; generated once, never sent.
#[tauri::command(rename_all = "snake_case")]
pub fn verifactu_record(state: State<'_, AppState>, voucher_id: String) -> ApiResult<crate::vat_es::VerifactuRecordDto> {
    crate::vat_es::verifactu_record(&mut *lock(&state)?, &voucher_id)
}

/// Spain: the VeriFactu billing-software details of this installation.
#[tauri::command(rename_all = "snake_case")]
pub fn verifactu_system(state: State<'_, AppState>) -> ApiResult<crate::vat_es::VerifactuSystem> {
    crate::vat_es::verifactu_system(&mut *lock(&state)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_verifactu_system(
    state: State<'_, AppState>,
    settings: crate::vat_es::VerifactuSystem,
) -> ApiResult<crate::vat_es::VerifactuSystem> {
    crate::vat_es::set_verifactu_system(&mut *lock(&state)?, &settings)
}

// ------------------- Peppol PINT (SG, AE), Factur-X and the Access Point client
//
// See src/peppol.rs and docs/peppol.md. Access Point settings (URL, key)
// live in `<app data>/peppol`, never in the books; sending runs off the
// main thread and holds the company lock only while the XML is built.

/// The PINT SG (InvoiceNow) UBL for a Sales or Credit Note voucher. Save
/// it with `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn pint_sg_xml(state: State<'_, AppState>, voucher_id: String) -> ApiResult<crate::peppol::PintDto> {
    crate::peppol::pint_sg_xml(&*lock(&state)?, &voucher_id)
}

/// The PINT AE UBL for a Sales or Credit Note voucher. Save it with
/// `write_gst_json`.
#[tauri::command(rename_all = "snake_case")]
pub fn pint_ae_xml(state: State<'_, AppState>, voucher_id: String) -> ApiResult<crate::peppol::PintDto> {
    crate::peppol::pint_ae_xml(&*lock(&state)?, &voucher_id)
}

/// The company's Peppol participant id ("0195:SGUEN…", "0235:…", "9930:DE…").
#[tauri::command(rename_all = "snake_case")]
pub fn peppol_participant_id(state: State<'_, AppState>) -> ApiResult<crate::peppol::ParticipantDto> {
    crate::peppol::peppol_participant_id(&*lock(&state)?)
}

/// A Factur-X / ZUGFeRD PDF (EN 16931) for a voucher of an EU company.
/// Save `pdf_base64` with `write_binary_file`.
#[tauri::command(rename_all = "snake_case")]
pub fn facturx_pdf(state: State<'_, AppState>, voucher_id: String) -> ApiResult<crate::peppol::FacturxDto> {
    crate::peppol::facturx_pdf(&*lock(&state)?, &voucher_id)
}

/// Write a base64 PDF to the path the save dialog returned (`.pdf` only).
#[tauri::command(rename_all = "snake_case")]
pub fn write_binary_file(app: AppHandle, path: String, contents_base64: String) -> ApiResult<()> {
    #[cfg(mobile)]
    if let Some(r) = crate::peppol::write_binary_uri(&app, &path, &contents_base64) {
        return r;
    }
    let _ = &app;
    crate::peppol::write_binary_file(&path, &contents_base64)
}

fn peppol_dir(app: &AppHandle) -> ApiResult<PathBuf> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("peppol"))
}

/// The Access Point settings; the API key only as "set or not".
#[tauri::command(rename_all = "snake_case")]
pub fn peppol_ap_settings(app: AppHandle) -> ApiResult<crate::peppol::ApSettingsDto> {
    crate::peppol::ap_settings(&peppol_dir(&app)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_peppol_ap_settings(
    app: AppHandle,
    settings: crate::peppol::ApSettingsInput,
) -> ApiResult<crate::peppol::ApSettingsDto> {
    crate::peppol::set_ap_settings(&peppol_dir(&app)?, &settings)
}

/// POST the voucher's PINT document (`document`: "pint-sg" or "pint-ae")
/// to the configured Access Point.
#[tauri::command(rename_all = "snake_case")]
pub async fn peppol_ap_send(app: AppHandle, voucher_id: String, document: String) -> ApiResult<crate::peppol::ApSendDto> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = peppol_dir(&app)?;
        let state = app.state::<AppState>();
        crate::peppol::ap_send(&dir, &state.company, &voucher_id, &document)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ------------------------- VAT packs: UK schemes, ZATCA queue and renewal

/// The company's UK VAT schemes (Flat Rate, Cash and Annual Accounting) and
/// HMRC's sector list. Kept in the company file, not synced yet.
#[tauri::command(rename_all = "snake_case")]
pub fn vat_gb_schemes(state: State<'_, AppState>) -> ApiResult<crate::vat_gb::UkVatSchemesDto> {
    crate::vat_gb::vat_gb_schemes(&*lock(&state)?)
}

/// Save the company's UK VAT schemes (owner only).
#[tauri::command(rename_all = "snake_case")]
pub fn set_vat_gb_schemes(state: State<'_, AppState>, settings: crate::vat_gb::UkVatSchemes) -> ApiResult<crate::vat_gb::UkVatSchemesDto> {
    crate::vat_gb::set_vat_gb_schemes(&mut *lock(&state)?, &settings)
}

/// ZATCA documents waiting to be reported or cleared on this computer.
#[tauri::command(rename_all = "snake_case")]
pub fn zatca_queue(app: AppHandle, state: State<'_, AppState>) -> ApiResult<crate::zatca::ZatcaQueueDto> {
    crate::zatca::zatca_queue(&*lock(&state)?, &zatca_dir(&app)?)
}

/// Send the ZATCA queue: `all` for Submit all (pending, refused and not yet
/// generated), else only the documents whose retry time has come (call it
/// every minute or two while online).
#[tauri::command(rename_all = "snake_case")]
pub async fn zatca_queue_run(app: AppHandle, all: bool) -> ApiResult<crate::zatca::ZatcaQueueRunDto> {
    zatca_blocking(app, move |dir, company| crate::zatca::zatca_queue_run(company, dir, all)).await
}

/// Renew this computer's production CSID with a new OTP.
#[tauri::command(rename_all = "snake_case")]
pub async fn zatca_renew(app: AppHandle, otp: String, options: Option<crate::zatca::OnboardOptions>) -> ApiResult<crate::zatca::ZatcaStatusDto> {
    zatca_blocking(app, move |dir, company| crate::zatca::zatca_renew(company, dir, &otp, &options.unwrap_or_default())).await
}

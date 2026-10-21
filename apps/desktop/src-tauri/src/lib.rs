//! KhataOS desktop shell: Tauri 2 commands over the open company.

pub mod api;
pub mod backup;
mod commands;
pub mod crash;
pub mod desk;
pub mod einvoice;
pub mod export;
pub mod forex;
pub mod gst;
pub mod gst_sg;
pub mod insights;
pub mod invoice;
pub mod invoice_design;
pub mod irp_client;
pub mod lan;
pub mod masters;
pub mod mtd_client;
pub mod package;
pub mod packs;
mod phone_files;
pub mod schedule3;
pub mod security;
pub mod sync;
pub mod tax;
pub mod tax_mx;
pub mod tax_us;
pub mod tds;
pub mod vat_ae;
pub mod vat_de;
pub mod vat_gb;
pub mod zatca;
pub mod zatca_api;
pub mod vat_lines;
pub mod pac_client;
pub mod gst_rcm;
pub mod gstr2b;
pub mod gstr9;
pub mod peppol;
pub mod vat_fr;
pub mod vat_nl;
pub mod vat_es;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;

/// Shared state: the open company and the file it lives in.
#[derive(Default)]
pub struct AppState {
    pub company: Mutex<api::Slot>,
    pub path: Mutex<Option<PathBuf>>,
    /// The open company's lock: one KhataOS per company file. Lock `company`
    /// first when holding both.
    pub company_lock: Mutex<Option<api::CompanyLock>>,
    /// HTML waiting for the print window to pick it up.
    pub print_html: Mutex<Option<String>>,
}

/// Desktop: called from `main.rs`. Android and iOS: the mobile entry point
/// (the generated `gen/android` and `gen/apple` projects load this library).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // Phones: the file dialogs return content:// (Android) and security-scoped
    // file:// (iOS) URIs, which src/phone_files.rs opens through this plugin.
    // It is used from Rust only; no capability exposes its commands to the page.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_fs::init());
    // Phones: opens the crash-report issue link in the browser (src/crash.rs).
    // Rust only, like the fs plugin; the desktop opens links itself.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_opener::init());
    builder
        .manage(AppState::default())
        .manage(lan::LanState::default())
        .setup(|app| {
            // Crash reports: saved on this device, never sent (src/crash.rs).
            if let Ok(dir) = app.path().app_data_dir() {
                // PDF export: fonts the user adds here (a CJK font on a
                // machine without one) fill in what the bundled fonts lack.
                khata_print::pdf::add_font_dir(dir.join("fonts"));
                crash::install(dir);
            }
            // A keyboard-first app must have keyboard focus as soon as it opens.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
            // Office-network sharing: start the embedded server in Server
            // mode and the background sync loop.
            if let Ok(dir) = app.path().app_data_dir() {
                lan::init(app.handle(), dir);
            }
            // Automatic backups: once a day while a company is open (src/backup.rs).
            backup::start_scheduler(app.handle().clone());
            Ok(())
        })
        // Closing the main window closes the company: take its automatic
        // backup first, when one is due (the process is about to end, so
        // not on a thread of its own).
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event
                && window.label() == "main"
            {
                commands::backup_before_exit(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_company,
            commands::open_company,
            commands::company_info,
            commands::default_company_dir,
            commands::list_company_files,
            commands::company_file_name,
            commands::search_groups,
            commands::list_ledgers,
            commands::create_ledger,
            commands::list_countries,
            commands::set_country,
            commands::check_tax_id,
            commands::post_voucher,
            commands::get_voucher,
            commands::next_voucher_number,
            commands::day_book,
            commands::ledger_statement,
            commands::trial_balance,
            commands::profit_and_loss,
            commands::balance_sheet,
            commands::group_summary,
            commands::keymap,
            commands::close_company,
            backup::cmd::backup_status,
            backup::cmd::backup_set_prefs,
            backup::cmd::backup_now,
            backup::cmd::backup_encrypted,
            backup::cmd::backup_inspect,
            backup::cmd::backup_restore,
            commands::quit_app,
            commands::report_error,
            commands::create_group,
            commands::alter_voucher,
            commands::delete_voucher,
            commands::cancel_voucher,
            commands::outstanding,
            commands::ageing,
            commands::overdue_payables,
            commands::import_tally,
            commands::sync_settings,
            commands::set_sync_settings,
            commands::sync_now,
            commands::rebase,
            commands::print_report,
            commands::print_voucher,
            commands::print_cheque,
            commands::take_print_html,
            commands::print_window,
            commands::report_csv,
            commands::write_csv,
            commands::save_import_report,
            commands::get_group,
            commands::alter_group,
            commands::delete_group,
            commands::get_ledger,
            commands::alter_ledger,
            commands::delete_ledger,
            commands::list_units,
            commands::save_unit,
            commands::delete_unit,
            commands::list_stock_groups,
            commands::save_stock_group,
            commands::delete_stock_group,
            commands::list_stock_items,
            commands::get_stock_item,
            commands::save_stock_item,
            commands::delete_stock_item,
            commands::stock_summary,
            commands::stock_item_vouchers,
            commands::bank_reconciliation,
            commands::set_bank_date,
            commands::ratio_analysis,
            commands::dashboard,
            commands::list_numbering,
            commands::set_numbering,
            commands::list_godowns,
            commands::save_godown,
            commands::delete_godown,
            commands::stock_by_location,
            commands::last_rate,
            commands::last_narration,
            commands::company_tax,
            commands::set_company_tax,
            commands::list_tax_rates,
            commands::set_tax_rate,
            commands::set_ledger_tax,
            commands::invoice_tax,
            commands::set_company_address,
            commands::set_hsn_digits,
            commands::match_statement,
            commands::apply_bank_dates,
            commands::read_text_file,
            commands::session_info,
            commands::sign_in,
            commands::sign_out,
            commands::list_users,
            commands::save_user,
            commands::remove_user,
            lan::cmd::sharing_config,
            lan::cmd::set_sharing_config,
            lan::cmd::set_relay_address,
            lan::cmd::move_relay,
            lan::cmd::host_company,
            lan::cmd::connect_relay,
            lan::cmd::online_access,
            lan::cmd::set_online_access,
            lan::cmd::set_connected,
            lan::cmd::connectivity_status,
            lan::cmd::list_access,
            lan::cmd::create_invite,
            lan::cmd::revoke_access,
            lan::cmd::inspect_invite,
            lan::cmd::join_company,
            package::cmd::package_send,
            package::cmd::package_next_from,
            package::cmd::package_inspect,
            package::cmd::package_receive,
            commands::gstr1,
            commands::gstr1_json,
            commands::gstr3b,
            commands::write_gst_json,
            commands::vat201,
            commands::schedule3_balance_sheet,
            commands::schedule3_profit_and_loss,
            commands::print_schedule3_balance_sheet,
            commands::print_schedule3_profit_and_loss,
            commands::einvoice_json,
            commands::einvoice_bulk_json,
            commands::ewaybill_json,
            commands::tds_natures,
            commands::tds_for,
            commands::tds_payable,
            commands::tds_quarterly,
            commands::tds_quarterly_csv,
            commands::set_ledger_tds,
            commands::tcs_natures,
            commands::tcs_for,
            commands::tcs_payable,
            commands::tcs_quarterly,
            commands::tds_quarterly_nr,
            commands::tds_nr_codes,
            commands::ledger_tcs,
            commands::set_ledger_tcs,
            commands::tds_deductor,
            commands::set_tds_deductor,
            commands::tds_challan,
            commands::set_tds_challan,
            commands::tds_statement_file,
            commands::write_tds_file,
            commands::salary_estimate,
            commands::gst_f5,
            commands::vat_return_gb,
            commands::mtd_vat_json,
            commands::zatca_invoice_xml,
            commands::ksa_vat_return,
            commands::zatca_seller,
            commands::set_zatca_seller,
            commands::mtd_settings,
            commands::set_mtd_settings,
            commands::mtd_connect,
            commands::mtd_disconnect,
            commands::mtd_obligations,
            commands::mtd_submit,
            commands::mtd_view_return,
            commands::mtd_liabilities,
            commands::mtd_payments,
            commands::mtd_test_headers,
            commands::mtd_header_feedback,
            commands::sales_tax_report_us,
            commands::form1099_us,
            commands::iva_summary_mx,
            commands::mx_issuer,
            commands::set_mx_issuer,
            commands::mx_csd_status,
            commands::set_mx_csd,
            commands::clear_mx_csd,
            commands::cfdi_xml,
            commands::irp_settings,
            commands::set_irp_settings,
            commands::irp_record,
            commands::irp_generate,
            commands::irp_cancel,
            commands::irp_details,
            commands::irp_ewaybill,
            commands::irp_gstin,
            commands::zatca_status,
            commands::zatca_onboard,
            commands::zatca_submit,
            commands::zatca_submission,
            commands::zatca_invoice_xml_stamped,
            commands::ustva,
            commands::xrechnung_xml,
            commands::rcm_register,
            commands::print_self_invoice,
            commands::self_invoice_html,
            commands::rule37_report,
            commands::rule42_calc,
            commands::gst_flag_list,
            commands::set_gst_flags,
            commands::gstr1_mark_filed,
            commands::gstr1_filed_periods,
            commands::gstr1_amendments,
            commands::gstr2b_reconcile,
            commands::read_gstr2b_file,
            commands::gstr9,
            commands::gstr9_set_row,
            commands::gstr9_docs,
            commands::gstr9_json,
            commands::gstr9_csv,
            commands::gstr1_hsn_csv,
            commands::gstr9_reconcile,
            commands::read_gst_return_file,
            commands::schedule3_statements,
            commands::print_schedule3_notes,
            commands::schedule3_mapping,
            commands::schedule3_line_options,
            commands::set_schedule3_mapping,
            commands::set_schedule3_msme,
            commands::set_schedule3_format,
            commands::pago_xml,
            commands::pac_settings,
            commands::set_pac_settings,
            commands::pac_record,
            commands::pac_stamp,
            commands::pac_cancel,
            commands::pac_confirm_cancellation,
            commands::use_tax_report_us,
            commands::us_certificates,
            commands::set_us_certificates,
            commands::certificate_report_us,
            commands::ca3_fr,
            commands::aangifte_nl,
            commands::modelo_303_es,
            commands::verifactu_record,
            commands::verifactu_system,
            commands::set_verifactu_system,
            commands::pint_sg_xml,
            commands::pint_ae_xml,
            commands::peppol_participant_id,
            commands::facturx_pdf,
            commands::write_binary_file,
            commands::peppol_ap_settings,
            commands::set_peppol_ap_settings,
            commands::peppol_ap_send,
            commands::vat_gb_schemes,
            commands::set_vat_gb_schemes,
            commands::zatca_queue,
            commands::zatca_queue_run,
            commands::zatca_renew,
            forex::cmd::fx_list_currencies,
            forex::cmd::fx_save_currency,
            forex::cmd::fx_set_ledger_currency,
            forex::cmd::fx_rate,
            forex::cmd::fx_set_rate,
            forex::cmd::fx_revaluation,
            forex::cmd::fx_post_revaluation,
            desk::cmd::desk_overview,
            desk::cmd::desk_due_count,
            desk::cmd::desk_due_dates,
            desk::cmd::desk_notes,
            desk::cmd::desk_save_note,
            desk::cmd::desk_set_done,
            desk::cmd::desk_delete_note,
            desk::cmd::desk_mark_filed,
            desk::cmd::desk_calendar_settings,
            desk::cmd::desk_set_calendar_settings,
            crash::cmd::crash_pending,
            crash::cmd::crash_context,
            crash::cmd::crash_record_js,
            crash::cmd::crash_report_url,
            crash::cmd::crash_open_url,
            crash::cmd::crash_dismiss,
            crash::cmd::crash_open_folder,
            invoice_design::invoice_design,
            invoice_design::set_invoice_design,
            invoice_design::import_invoice_image,
            invoice_design::set_invoice_image_raster,
            invoice_design::remove_invoice_image,
            invoice_design::invoice_design_preview,
            invoice_design::voucher_print_details,
            invoice_design::set_voucher_print_details,
            invoice_design::export_voucher_pdf,
            invoice_design::export_vouchers_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("error while running KhataOS");
}

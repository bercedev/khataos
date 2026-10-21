//! TallyPrime 7.x compatible default bindings.

use crate::{Action, Binding, Context, GatewayItem, VoucherKey};
use Action::*;
use Context::*;

pub(crate) fn bindings() -> Vec<Binding> {
    let table: &[(&str, Context, Action)] = &[
        // Global
        ("Ctrl+A", Global, Accept),
        ("Esc", Global, Back),
        ("Ctrl+Q", Global, Quit),
        ("Alt+F4", Global, QuitApp),
        ("F1", Global, Help),
        ("Ctrl+F1", Global, ContextHelp),
        ("Alt+G", Global, GoTo),
        ("Ctrl+G", Global, SwitchTo),
        ("Alt+F", Global, Find),
        ("Alt+K", Global, CompanyMenu),
        ("Alt+Y", Global, DataMenu),
        ("Alt+Z", Global, ExchangeMenu),
        ("Alt+O", Global, ImportMenu),
        ("Alt+E", Global, ExportMenu),
        ("Alt+M", Global, ShareMenu),
        ("Alt+P", Global, PrintMenu),
        ("Ctrl+E", Global, ExportCurrent),
        ("Ctrl+M", Global, EmailCurrent),
        ("Ctrl+Alt+W", Global, WhatsAppCurrent),
        ("Ctrl+P", Global, PrintCurrent),
        ("F2", Global, ChangeDate),
        ("Alt+F2", Global, ChangePeriod),
        ("F3", Global, SwitchCompany),
        ("Alt+F3", Global, OpenCompany),
        ("Ctrl+F3", Global, ShutCompany),
        ("F11", Global, CompanyFeatures),
        ("F12", Global, Configure),
        ("Ctrl+N", Global, Calculator),
        ("Ctrl+K", Global, DisplayLanguage),
        ("Ctrl+W", Global, EntryLanguage),
        ("Ctrl+Alt+N", Global, Notifications),
        // KhataOS addition: sync with the company's server (not a Tally key).
        ("Ctrl+Alt+S", Global, SyncNow),
        ("Enter", Global, NextField),
        ("Shift+Tab", Global, PreviousField),
        ("Ctrl+Home", Global, FirstLine),
        ("Ctrl+End", Global, LastLine),
        // Gateway letters (verified on TallyPrime 7.2)
        ("C", Gateway, Menu(GatewayItem::Create)),
        ("A", Gateway, Menu(GatewayItem::Alter)),
        ("H", Gateway, Menu(GatewayItem::ChartOfAccounts)),
        ("V", Gateway, Menu(GatewayItem::Vouchers)),
        ("K", Gateway, Menu(GatewayItem::DayBook)),
        ("N", Gateway, Menu(GatewayItem::Banking)),
        ("B", Gateway, Menu(GatewayItem::BalanceSheet)),
        ("P", Gateway, Menu(GatewayItem::ProfitAndLoss)),
        ("S", Gateway, Menu(GatewayItem::StockSummary)),
        ("R", Gateway, Menu(GatewayItem::RatioAnalysis)),
        ("D", Gateway, Menu(GatewayItem::MoreReports)),
        ("O", Gateway, Menu(GatewayItem::Dashboard)),
        ("Q", Gateway, Menu(GatewayItem::Quit)),
        // KhataOS additions (no Tally meaning): the accountant's desk on the
        // home menu, and a note or to-do on the record under the cursor.
        ("T", Gateway, Menu(GatewayItem::Desk)),
        ("Alt+W", Report, AddNote),
        ("Alt+W", Voucher, AddNote),
        // Voucher types
        ("F4", Voucher, OpenVoucher(VoucherKey::Contra)),
        ("F5", Voucher, OpenVoucher(VoucherKey::Payment)),
        ("F6", Voucher, OpenVoucher(VoucherKey::Receipt)),
        ("F7", Voucher, OpenVoucher(VoucherKey::Journal)),
        ("F8", Voucher, OpenVoucher(VoucherKey::Sales)),
        ("F9", Voucher, OpenVoucher(VoucherKey::Purchase)),
        ("Alt+F5", Voucher, OpenVoucher(VoucherKey::DebitNote)),
        ("Alt+F6", Voucher, OpenVoucher(VoucherKey::CreditNote)),
        ("Ctrl+F8", Voucher, OpenVoucher(VoucherKey::SalesOrder)),
        ("Ctrl+F9", Voucher, OpenVoucher(VoucherKey::PurchaseOrder)),
        ("Alt+F7", Voucher, OpenVoucher(VoucherKey::StockJournal)),
        ("Ctrl+F7", Voucher, OpenVoucher(VoucherKey::PhysicalStock)),
        ("Alt+F8", Voucher, OpenVoucher(VoucherKey::DeliveryNote)),
        ("Alt+F9", Voucher, OpenVoucher(VoucherKey::ReceiptNote)),
        ("Ctrl+F6", Voucher, OpenVoucher(VoucherKey::RejectionIn)),
        ("Ctrl+F5", Voucher, OpenVoucher(VoucherKey::RejectionOut)),
        ("Ctrl+F4", Voucher, OpenVoucher(VoucherKey::Payroll)),
        ("F10", Voucher, OpenVoucher(VoucherKey::Other)),
        // Voucher entry
        ("Alt+D", Voucher, DeleteRecord),
        ("Alt+X", Voucher, CancelVoucher),
        ("Ctrl+D", Voucher, RemoveLine),
        ("Ctrl+H", Voucher, ChangeMode),
        ("Ctrl+I", Voucher, MoreDetails),
        ("Ctrl+T", Voucher, MarkPostDated),
        ("Ctrl+L", Voucher, MarkOptional),
        ("Alt+J", Voucher, StatAdjustment),
        ("Ctrl+F", Voucher, Autofill),
        ("Alt+S", Voucher, StockQuery),
        ("Alt+A", Voucher, TaxAnalysis),
        ("Alt+R", Voucher, RetrieveNarrationSameLedger),
        ("Ctrl+R", Voucher, RetrieveNarrationSameType),
        ("Ctrl+O", Voucher, RelatedReports),
        ("Ctrl+Alt+U", Voucher, UnusedVoucherNumber),
        ("Alt+Q", Voucher, EditLog),
        ("PageUp", Voucher, PreviousRecord),
        ("PageDown", Voucher, NextRecord),
        // Masters
        ("Alt+D", Master, DeleteRecord),
        ("Ctrl+I", Master, MoreDetails),
        ("Alt+Q", Master, EditLog),
        ("Alt+L", Master, FetchByTaxId),
        ("F10", Master, OpenVoucher(VoucherKey::Other)),
        ("PageUp", Master, PreviousRecord),
        ("PageDown", Master, NextRecord),
        // Field-level
        ("Alt+C", ListField, CreateMasterInline),
        ("Ctrl+Enter", ListField, AlterMasterInline),
        ("Alt+C", AmountField, Calculator),
        // Reports
        ("Enter", Report, DrillDown),
        ("Ctrl+Enter", Report, AlterMasterInline),
        ("Alt+F5", Report, ToggleDetailed),
        ("Alt+F1", Report, ToggleDetailed),
        ("Shift+Enter", Report, ExpandLine),
        ("F4", Report, ChangeContext),
        ("F5", Report, AltView1),
        ("F6", Report, AltView2),
        ("Ctrl+B", Report, BasisOfValues),
        ("Ctrl+H", Report, ChangeView),
        ("Ctrl+J", Report, ExceptionReports),
        ("Ctrl+F", Report, Filter),
        ("Ctrl+Alt+F", Report, FilterDetails),
        ("Alt+C", Report, NewColumn),
        ("Alt+A", Report, AlterColumn),
        ("Alt+D", Report, DeleteColumn),
        ("Alt+N", Report, AutoColumns),
        ("Ctrl+L", Report, SaveView),
        ("Space", Report, SelectLine),
        ("Shift+Space", Report, SelectLine),
        ("Ctrl+Space", Report, SelectAll),
        ("Ctrl+Alt+I", Report, InvertSelection),
        ("Alt+I", Report, InsertVoucherBefore),
        ("Alt+2", Report, DuplicateVoucher),
        ("Alt+X", Report, CancelVoucher),
        ("Ctrl+R", Report, HideLine),
        ("Ctrl+U", Report, RestoreLine),
        ("PageUp", Report, PreviousRecord),
        ("PageDown", Report, NextRecord),
    ];
    // Day Book's bottom bar adds vouchers with Alt+A; on other reports Alt+A
    // alters a column. Day Book uses its own context in the UI layer, so the
    // default report map keeps the column meaning.
    let _ = AddVoucherAfter;

    table
        .iter()
        .map(|&(chord, context, action)| Binding {
            chord: chord
                .parse()
                .unwrap_or_else(|e| panic!("bad chord {chord:?}: {e}")),
            context,
            action,
        })
        .collect()
}

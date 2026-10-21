//! Keyboard bindings as data.
//!
//! Users moving from TallyPrime rely on muscle memory, so the default map
//! ([`Keymap::tally_prime`]) reproduces TallyPrime 7.x key behaviour. Keys are
//! resolved by screen context: the same chord can mean different things on a
//! voucher and on a report (Ctrl+L marks a voucher optional but saves a
//! report view), exactly as users expect.
//!
//! Sources: `docs/research/tally-shortcuts.md` and hands-on verification
//! against a TallyPrime 7.2 install (see that document's last section).

mod chord;
mod tally;

use serde::{Deserialize, Serialize};

pub use chord::{Chord, ChordParseError, Key, Mods};

/// Where the cursor is. More specific contexts win over less specific ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Context {
    /// Anywhere in the app.
    Global,
    /// The home menu.
    Gateway,
    /// Creating or altering a voucher.
    Voucher,
    /// Creating or altering a master (ledger, group, item…).
    Master,
    /// A report such as Day Book or Balance Sheet.
    Report,
    /// A field whose value is picked from a list (ledger, group, state…).
    ListField,
    /// An amount field.
    AmountField,
    /// A dialog with labelled buttons ("A: Accept", "Y: Yes").
    Dialog,
}

/// Voucher types reachable by a direct key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoucherKey {
    Contra,
    Payment,
    Receipt,
    Journal,
    Sales,
    Purchase,
    CreditNote,
    DebitNote,
    SalesOrder,
    PurchaseOrder,
    StockJournal,
    PhysicalStock,
    DeliveryNote,
    ReceiptNote,
    RejectionIn,
    RejectionOut,
    Payroll,
    /// F10: list of all voucher types.
    Other,
}

/// Home-menu destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GatewayItem {
    Create,
    Alter,
    ChartOfAccounts,
    Vouchers,
    DayBook,
    Banking,
    BalanceSheet,
    ProfitAndLoss,
    StockSummary,
    RatioAnalysis,
    MoreReports,
    Dashboard,
    /// The accountant's desk: notes, to-dos and the compliance calendar
    /// (a KhataOS addition).
    Desk,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    // Global
    Accept,
    Back,
    Quit,
    QuitApp,
    Help,
    ContextHelp,
    GoTo,
    SwitchTo,
    Find,
    CompanyMenu,
    DataMenu,
    ExchangeMenu,
    ImportMenu,
    ExportMenu,
    ShareMenu,
    PrintMenu,
    ExportCurrent,
    EmailCurrent,
    WhatsAppCurrent,
    PrintCurrent,
    ChangeDate,
    ChangePeriod,
    SwitchCompany,
    OpenCompany,
    ShutCompany,
    CompanyFeatures,
    Configure,
    Calculator,
    DisplayLanguage,
    EntryLanguage,
    Notifications,
    /// Sync the open company with its server now.
    SyncNow,
    // Navigation
    NextField,
    PreviousField,
    FirstLine,
    LastLine,
    PreviousRecord,
    NextRecord,
    // Gateway
    Menu(GatewayItem),
    // Voucher entry
    OpenVoucher(VoucherKey),
    ChangeMode,
    MoreDetails,
    MarkPostDated,
    MarkOptional,
    StatAdjustment,
    Autofill,
    StockQuery,
    TaxAnalysis,
    RetrieveNarrationSameLedger,
    RetrieveNarrationSameType,
    RelatedReports,
    UnusedVoucherNumber,
    RemoveLine,
    DeleteRecord,
    CancelVoucher,
    EditLog,
    // Masters and list fields
    CreateMasterInline,
    AlterMasterInline,
    FetchByTaxId,
    // Reports
    DrillDown,
    ToggleDetailed,
    ExpandLine,
    ChangeContext,
    AltView1,
    AltView2,
    BasisOfValues,
    ChangeView,
    ExceptionReports,
    Filter,
    FilterDetails,
    NewColumn,
    AlterColumn,
    DeleteColumn,
    AutoColumns,
    SaveView,
    SelectLine,
    SelectAll,
    InvertSelection,
    AddVoucherAfter,
    InsertVoucherBefore,
    DuplicateVoucher,
    HideLine,
    RestoreLine,
    /// Add a note or to-do on the record under the cursor (a KhataOS
    /// addition; Alt+W has no TallyPrime meaning).
    AddNote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub chord: Chord,
    pub context: Context,
    pub action: Action,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    pub fn new(bindings: Vec<Binding>) -> Self {
        Self { bindings }
    }

    /// TallyPrime 7.x compatible defaults.
    pub fn tally_prime() -> Self {
        Self::new(tally::bindings())
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Resolve a chord against the active contexts, most specific first
    /// (e.g. `[ListField, Voucher, Global]`).
    pub fn resolve(&self, chord: Chord, active: &[Context]) -> Option<Action> {
        active.iter().find_map(|ctx| {
            self.bindings
                .iter()
                .find(|b| b.chord == chord && b.context == *ctx)
                .map(|b| b.action)
        })
    }

    /// All chords bound to an action in a context, for button-bar labels.
    pub fn chords_for(&self, action: Action, context: Context) -> impl Iterator<Item = Chord> + '_ {
        self.bindings
            .iter()
            .filter(move |b| b.action == action && b.context == context)
            .map(|b| b.chord)
    }

    /// Replace or add a binding (user customisation).
    pub fn rebind(&mut self, binding: Binding) {
        self.bindings
            .retain(|b| !(b.chord == binding.chord && b.context == binding.context));
        self.bindings.push(binding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Context::*;

    fn k(s: &str) -> Chord {
        s.parse().unwrap()
    }

    #[test]
    fn voucher_keys_match_tallyprime_not_erp9() {
        let km = Keymap::tally_prime();
        let ctx = [Voucher, Global];
        assert_eq!(
            km.resolve(k("F8"), &ctx),
            Some(Action::OpenVoucher(VoucherKey::Sales))
        );
        assert_eq!(
            km.resolve(k("Alt+F6"), &ctx),
            Some(Action::OpenVoucher(VoucherKey::CreditNote))
        );
        assert_eq!(
            km.resolve(k("Alt+F5"), &ctx),
            Some(Action::OpenVoucher(VoucherKey::DebitNote))
        );
        assert_eq!(
            km.resolve(k("Ctrl+F8"), &ctx),
            Some(Action::OpenVoucher(VoucherKey::SalesOrder))
        );
        assert_eq!(
            km.resolve(k("Alt+F7"), &ctx),
            Some(Action::OpenVoucher(VoucherKey::StockJournal))
        );
    }

    #[test]
    fn same_chord_depends_on_context() {
        let km = Keymap::tally_prime();
        assert_eq!(
            km.resolve(k("Ctrl+L"), &[Voucher, Global]),
            Some(Action::MarkOptional)
        );
        assert_eq!(
            km.resolve(k("Ctrl+L"), &[Report, Global]),
            Some(Action::SaveView)
        );
        assert_eq!(
            km.resolve(k("Alt+C"), &[ListField, Voucher, Global]),
            Some(Action::CreateMasterInline)
        );
        assert_eq!(
            km.resolve(k("Alt+C"), &[AmountField, Voucher, Global]),
            Some(Action::Calculator)
        );
        assert_eq!(
            km.resolve(k("Alt+C"), &[Report, Global]),
            Some(Action::NewColumn)
        );
    }

    #[test]
    fn global_keys_fall_through() {
        let km = Keymap::tally_prime();
        assert_eq!(
            km.resolve(k("Ctrl+A"), &[ListField, Master, Global]),
            Some(Action::Accept)
        );
        assert_eq!(
            km.resolve(k("Alt+G"), &[Report, Global]),
            Some(Action::GoTo)
        );
        assert_eq!(km.resolve(k("Esc"), &[Voucher, Global]), Some(Action::Back));
    }

    #[test]
    fn gateway_letters_verified_hands_on() {
        let km = Keymap::tally_prime();
        let cases = [
            ("C", GatewayItem::Create),
            ("A", GatewayItem::Alter),
            ("H", GatewayItem::ChartOfAccounts),
            ("V", GatewayItem::Vouchers),
            ("K", GatewayItem::DayBook),
            ("N", GatewayItem::Banking),
            ("B", GatewayItem::BalanceSheet),
            ("P", GatewayItem::ProfitAndLoss),
            ("S", GatewayItem::StockSummary),
            ("R", GatewayItem::RatioAnalysis),
            ("D", GatewayItem::MoreReports),
            ("O", GatewayItem::Dashboard),
            ("Q", GatewayItem::Quit),
        ];
        for (key, item) in cases {
            assert_eq!(
                km.resolve(k(key), &[Gateway, Global]),
                Some(Action::Menu(item)),
                "{key}"
            );
        }
    }

    #[test]
    fn desk_keys_are_free_tally_keys() {
        let km = Keymap::tally_prime();
        assert_eq!(
            km.resolve(k("T"), &[Gateway, Global]),
            Some(Action::Menu(GatewayItem::Desk))
        );
        assert_eq!(
            km.resolve(k("Alt+W"), &[Report, Global]),
            Some(Action::AddNote)
        );
        assert_eq!(
            km.resolve(k("Alt+W"), &[Voucher, Global]),
            Some(Action::AddNote)
        );
        assert_eq!(
            km.resolve(k("Alt+W"), &[ListField, Voucher, Global]),
            Some(Action::AddNote),
            "falls through from a field"
        );
        assert_eq!(
            km.resolve(k("Alt+W"), &[Global]),
            None,
            "only where a record is under the cursor"
        );
    }

    #[test]
    fn sync_now_is_global_and_never_shadowed() {
        let km = Keymap::tally_prime();
        let chord = k("Ctrl+Alt+S");
        assert_eq!(km.resolve(chord, &[Global]), Some(Action::SyncNow));
        for ctx in [
            Gateway,
            Voucher,
            Master,
            Report,
            ListField,
            AmountField,
            Dialog,
        ] {
            assert_eq!(
                km.resolve(chord, &[ctx, Global]),
                Some(Action::SyncNow),
                "{ctx:?}"
            );
        }
    }

    #[test]
    fn no_chord_is_bound_twice_in_one_context() {
        let km = Keymap::tally_prime();
        let b = km.bindings();
        for (i, x) in b.iter().enumerate() {
            for y in &b[i + 1..] {
                assert!(
                    !(x.chord == y.chord && x.context == y.context),
                    "{} bound twice in {:?}: {:?} and {:?}",
                    x.chord,
                    x.context,
                    x.action,
                    y.action
                );
            }
        }
    }

    #[test]
    fn rebinding_replaces_only_that_context() {
        let mut km = Keymap::tally_prime();
        km.rebind(Binding {
            chord: k("F8"),
            context: Voucher,
            action: Action::OpenVoucher(VoucherKey::Purchase),
        });
        assert_eq!(
            km.resolve(k("F8"), &[Voucher]),
            Some(Action::OpenVoucher(VoucherKey::Purchase))
        );
        assert_eq!(
            km.chords_for(Action::OpenVoucher(VoucherKey::Sales), Voucher)
                .count(),
            0
        );
    }
}

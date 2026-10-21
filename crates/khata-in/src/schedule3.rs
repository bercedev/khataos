//! Schedule III to the Companies Act 2013: the Balance Sheet and the
//! Statement of Profit and Loss in the vertical format with the previous
//! period alongside, in Division I (companies following Accounting
//! Standards) or Division II (Ind AS) layout. The notes to accounts built
//! from the same figures are in [`crate::schedule3_notes`].
//!
//! Requirements come from `docs/research/india-compliance.md` §3 and
//! `docs/research/india-schedule-3.md` (line items, the comparative column,
//! the 2021 amendments: MSME split of trade payables, ageing schedules,
//! current maturities of long-term borrowings under short-term borrowings,
//! "Total Income", rounding by turnover; Division II's other equity, OCI and
//! financial assets and liabilities). The mapping, its assumptions and what
//! users must classify themselves are in `docs/india-schedule-3.md`.
//!
//! How a ledger is placed: walk up from its group; at each group, the
//! company's own mapping ([`Mapping`], stored in the profile as line codes)
//! comes first, then the default mapping of that default group. The nearest
//! group with either wins, so mapping a group moves its own ledgers and those
//! of sub-groups without a mapping of their own. A ledger under no mapped
//! group goes by its nature (other current liabilities or assets, other
//! income or expenses) and a warning says so. Default groups are recognised
//! by [`GroupId::default_named`], never by their display name: users can
//! rename them.
//!
//! One placement serves both divisions: a Division I line item, the
//! Division II line it becomes (by a fixed translation, or a Division II
//! code the company chose) and classification tags (MSME, disputed,
//! doubtful, secured, current maturities). So Division II totals equal
//! Division I totals by construction.
//!
//! Figures tie to the core reports by construction: the same balances, the
//! same closing-stock valuation, the same accumulated profit and opening
//! difference ([`khata_core::reports::balance_sheet`],
//! [`khata_core::reports::profit_and_loss`]).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use khata_compliance::FinancialYear;
use khata_core::chart::{GroupId, LedgerId, Nature};
use khata_core::reports;
use khata_core::{Amount, Book};
use serde::{Deserialize, Serialize};

use crate::FY_START_MONTH;

/// A Schedule III, Division I line item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineItem {
    // Equity and liabilities
    ShareCapital,
    ReservesAndSurplus,
    MoneyReceivedAgainstShareWarrants,
    ShareApplicationMoneyPendingAllotment,
    LongTermBorrowings,
    DeferredTaxLiabilities,
    OtherLongTermLiabilities,
    LongTermProvisions,
    ShortTermBorrowings,
    TradePayables,
    OtherCurrentLiabilities,
    ShortTermProvisions,
    // Assets
    PropertyPlantAndEquipment,
    IntangibleAssets,
    CapitalWorkInProgress,
    IntangibleAssetsUnderDevelopment,
    NonCurrentInvestments,
    DeferredTaxAssets,
    LongTermLoansAndAdvances,
    OtherNonCurrentAssets,
    CurrentInvestments,
    Inventories,
    TradeReceivables,
    CashAndCashEquivalents,
    ShortTermLoansAndAdvances,
    OtherCurrentAssets,
    // Statement of Profit and Loss
    RevenueFromOperations,
    OtherIncome,
    CostOfMaterialsConsumed,
    PurchasesOfStockInTrade,
    /// Computed from stock (opening − closing); groups cannot map here.
    ChangesInInventories,
    EmployeeBenefitsExpense,
    FinanceCosts,
    DepreciationAndAmortisation,
    OtherExpenses,
    CurrentTax,
    DeferredTax,
}

impl LineItem {
    pub const ALL: &'static [LineItem] = &[
        Self::ShareCapital,
        Self::ReservesAndSurplus,
        Self::MoneyReceivedAgainstShareWarrants,
        Self::ShareApplicationMoneyPendingAllotment,
        Self::LongTermBorrowings,
        Self::DeferredTaxLiabilities,
        Self::OtherLongTermLiabilities,
        Self::LongTermProvisions,
        Self::ShortTermBorrowings,
        Self::TradePayables,
        Self::OtherCurrentLiabilities,
        Self::ShortTermProvisions,
        Self::PropertyPlantAndEquipment,
        Self::IntangibleAssets,
        Self::CapitalWorkInProgress,
        Self::IntangibleAssetsUnderDevelopment,
        Self::NonCurrentInvestments,
        Self::DeferredTaxAssets,
        Self::LongTermLoansAndAdvances,
        Self::OtherNonCurrentAssets,
        Self::CurrentInvestments,
        Self::Inventories,
        Self::TradeReceivables,
        Self::CashAndCashEquivalents,
        Self::ShortTermLoansAndAdvances,
        Self::OtherCurrentAssets,
        Self::RevenueFromOperations,
        Self::OtherIncome,
        Self::CostOfMaterialsConsumed,
        Self::PurchasesOfStockInTrade,
        Self::ChangesInInventories,
        Self::EmployeeBenefitsExpense,
        Self::FinanceCosts,
        Self::DepreciationAndAmortisation,
        Self::OtherExpenses,
        Self::CurrentTax,
        Self::DeferredTax,
    ];

    pub fn label(self) -> &'static str {
        use LineItem::*;
        match self {
            ShareCapital => "Share capital",
            ReservesAndSurplus => "Reserves and surplus",
            MoneyReceivedAgainstShareWarrants => "Money received against share warrants",
            ShareApplicationMoneyPendingAllotment => "Share application money pending allotment",
            LongTermBorrowings => "Long-term borrowings",
            DeferredTaxLiabilities => "Deferred tax liabilities (net)",
            OtherLongTermLiabilities => "Other long-term liabilities",
            LongTermProvisions => "Long-term provisions",
            ShortTermBorrowings => "Short-term borrowings",
            TradePayables => "Trade payables",
            OtherCurrentLiabilities => "Other current liabilities",
            ShortTermProvisions => "Short-term provisions",
            PropertyPlantAndEquipment => "Property, Plant and Equipment",
            IntangibleAssets => "Intangible assets",
            CapitalWorkInProgress => "Capital work-in-progress",
            IntangibleAssetsUnderDevelopment => "Intangible assets under development",
            NonCurrentInvestments => "Non-current investments",
            DeferredTaxAssets => "Deferred tax assets (net)",
            LongTermLoansAndAdvances => "Long-term loans and advances",
            OtherNonCurrentAssets => "Other non-current assets",
            CurrentInvestments => "Current investments",
            Inventories => "Inventories",
            TradeReceivables => "Trade receivables",
            CashAndCashEquivalents => "Cash and cash equivalents",
            ShortTermLoansAndAdvances => "Short-term loans and advances",
            OtherCurrentAssets => "Other current assets",
            RevenueFromOperations => "Revenue from operations",
            OtherIncome => "Other income",
            CostOfMaterialsConsumed => "Cost of materials consumed",
            PurchasesOfStockInTrade => "Purchases of stock-in-trade",
            ChangesInInventories => "Changes in inventories of finished goods, work-in-progress and stock-in-trade",
            EmployeeBenefitsExpense => "Employee benefits expense",
            FinanceCosts => "Finance costs",
            DepreciationAndAmortisation => "Depreciation and amortisation expense",
            OtherExpenses => "Other expenses",
            CurrentTax => "Current tax",
            DeferredTax => "Deferred tax",
        }
    }

    /// The nature of the groups that may map here; `None` for a computed line.
    pub fn nature(self) -> Option<Nature> {
        use LineItem::*;
        Some(match self {
            ShareCapital
            | ReservesAndSurplus
            | MoneyReceivedAgainstShareWarrants
            | ShareApplicationMoneyPendingAllotment
            | LongTermBorrowings
            | DeferredTaxLiabilities
            | OtherLongTermLiabilities
            | LongTermProvisions
            | ShortTermBorrowings
            | TradePayables
            | OtherCurrentLiabilities
            | ShortTermProvisions => Nature::Liabilities,
            PropertyPlantAndEquipment
            | IntangibleAssets
            | CapitalWorkInProgress
            | IntangibleAssetsUnderDevelopment
            | NonCurrentInvestments
            | DeferredTaxAssets
            | LongTermLoansAndAdvances
            | OtherNonCurrentAssets
            | CurrentInvestments
            | Inventories
            | TradeReceivables
            | CashAndCashEquivalents
            | ShortTermLoansAndAdvances
            | OtherCurrentAssets => Nature::Assets,
            RevenueFromOperations | OtherIncome => Nature::Income,
            ChangesInInventories => return None,
            CostOfMaterialsConsumed
            | PurchasesOfStockInTrade
            | EmployeeBenefitsExpense
            | FinanceCosts
            | DepreciationAndAmortisation
            | OtherExpenses
            | CurrentTax
            | DeferredTax => Nature::Expenses,
        })
    }

    /// The stored code ("trade_payables").
    pub fn code(self) -> String {
        code_of(self)
    }

    pub fn from_code(code: &str) -> Option<Self> {
        from_code(code)
    }

    /// The Division II line this item becomes when no Division II code is
    /// chosen for the group.
    pub fn ind_as(self) -> IndAsLine {
        use IndAsLine as I;
        use LineItem::*;
        match self {
            ShareCapital => I::EquityShareCapital,
            ReservesAndSurplus | MoneyReceivedAgainstShareWarrants | ShareApplicationMoneyPendingAllotment => I::OtherEquity,
            LongTermBorrowings => I::NcBorrowings,
            DeferredTaxLiabilities => I::DeferredTaxLiabilities,
            OtherLongTermLiabilities => I::OtherNonCurrentLiabilities,
            LongTermProvisions => I::NcProvisions,
            ShortTermBorrowings => I::CBorrowings,
            TradePayables => I::CTradePayables,
            OtherCurrentLiabilities => I::OtherCurrentLiabilities,
            ShortTermProvisions => I::CProvisions,
            PropertyPlantAndEquipment => I::PropertyPlantAndEquipment,
            IntangibleAssets => I::OtherIntangibleAssets,
            CapitalWorkInProgress => I::CapitalWorkInProgress,
            IntangibleAssetsUnderDevelopment => I::IntangibleAssetsUnderDevelopment,
            NonCurrentInvestments => I::NcInvestments,
            DeferredTaxAssets => I::DeferredTaxAssets,
            LongTermLoansAndAdvances => I::NcLoans,
            OtherNonCurrentAssets => I::OtherNonCurrentAssets,
            CurrentInvestments => I::CInvestments,
            Inventories => I::Inventories,
            TradeReceivables => I::CTradeReceivables,
            CashAndCashEquivalents => I::CashAndCashEquivalents,
            ShortTermLoansAndAdvances => I::CLoans,
            OtherCurrentAssets => I::OtherCurrentAssets,
            RevenueFromOperations => I::RevenueFromOperations,
            OtherIncome => I::OtherIncome,
            CostOfMaterialsConsumed => I::CostOfMaterialsConsumed,
            PurchasesOfStockInTrade => I::PurchasesOfStockInTrade,
            ChangesInInventories => I::ChangesInInventories,
            EmployeeBenefitsExpense => I::EmployeeBenefitsExpense,
            FinanceCosts => I::FinanceCosts,
            DepreciationAndAmortisation => I::DepreciationAndAmortisation,
            OtherExpenses => I::OtherExpenses,
            CurrentTax => I::CurrentTax,
            DeferredTax => I::DeferredTax,
        }
    }

    fn family(self) -> Option<Family> {
        match self {
            LineItem::LongTermBorrowings => Some(Family::Borrowings { current: false }),
            LineItem::ShortTermBorrowings => Some(Family::Borrowings { current: true }),
            LineItem::TradePayables => Some(Family::Payables),
            LineItem::TradeReceivables => Some(Family::Receivables),
            _ => None,
        }
    }
}

fn code_of<T: Serialize>(t: T) -> String {
    serde_json::to_value(t).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn from_code<T: for<'de> Deserialize<'de>>(code: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(code.to_owned())).ok()
}

/// A Schedule III, Division II (Ind AS) line item. Each one stands for a
/// Division I line ([`IndAsLine::division1`]), so a company that maps a
/// group to a Division II line still gets a Division I statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndAsLine {
    // Assets, non-current
    PropertyPlantAndEquipment,
    CapitalWorkInProgress,
    InvestmentProperty,
    Goodwill,
    OtherIntangibleAssets,
    IntangibleAssetsUnderDevelopment,
    #[serde(rename = "investments_non_current")]
    NcInvestments,
    #[serde(rename = "trade_receivables_non_current")]
    NcTradeReceivables,
    #[serde(rename = "loans_non_current")]
    NcLoans,
    #[serde(rename = "other_financial_assets_non_current")]
    NcOtherFinancialAssets,
    DeferredTaxAssets,
    OtherNonCurrentAssets,
    // Assets, current
    Inventories,
    #[serde(rename = "investments_current")]
    CInvestments,
    #[serde(rename = "trade_receivables_current")]
    CTradeReceivables,
    CashAndCashEquivalents,
    OtherBankBalances,
    #[serde(rename = "loans_current")]
    CLoans,
    #[serde(rename = "other_financial_assets_current")]
    COtherFinancialAssets,
    CurrentTaxAssets,
    OtherCurrentAssets,
    // Equity
    EquityShareCapital,
    OtherEquity,
    // Liabilities, non-current
    #[serde(rename = "borrowings_non_current")]
    NcBorrowings,
    #[serde(rename = "lease_liabilities_non_current")]
    NcLeaseLiabilities,
    #[serde(rename = "trade_payables_non_current")]
    NcTradePayables,
    #[serde(rename = "other_financial_liabilities_non_current")]
    NcOtherFinancialLiabilities,
    #[serde(rename = "provisions_non_current")]
    NcProvisions,
    DeferredTaxLiabilities,
    OtherNonCurrentLiabilities,
    // Liabilities, current
    #[serde(rename = "borrowings_current")]
    CBorrowings,
    #[serde(rename = "lease_liabilities_current")]
    CLeaseLiabilities,
    #[serde(rename = "trade_payables_current")]
    CTradePayables,
    #[serde(rename = "other_financial_liabilities_current")]
    COtherFinancialLiabilities,
    OtherCurrentLiabilities,
    #[serde(rename = "provisions_current")]
    CProvisions,
    CurrentTaxLiabilities,
    // Statement of Profit and Loss
    RevenueFromOperations,
    OtherIncome,
    CostOfMaterialsConsumed,
    PurchasesOfStockInTrade,
    ChangesInInventories,
    EmployeeBenefitsExpense,
    FinanceCosts,
    DepreciationAndAmortisation,
    OtherExpenses,
    CurrentTax,
    DeferredTax,
    /// Other comprehensive income, part A (shown net of its tax).
    OciNotReclassified,
    /// Other comprehensive income, part B (shown net of its tax).
    OciReclassified,
}

impl IndAsLine {
    /// Lines that exist only in Division II, offered as mapping targets.
    pub const ONLY_DIVISION2: &'static [IndAsLine] = &[
        Self::InvestmentProperty,
        Self::Goodwill,
        Self::NcTradeReceivables,
        Self::NcOtherFinancialAssets,
        Self::OtherBankBalances,
        Self::COtherFinancialAssets,
        Self::CurrentTaxAssets,
        Self::NcLeaseLiabilities,
        Self::NcTradePayables,
        Self::NcOtherFinancialLiabilities,
        Self::CLeaseLiabilities,
        Self::COtherFinancialLiabilities,
        Self::CurrentTaxLiabilities,
        Self::OciNotReclassified,
        Self::OciReclassified,
    ];

    pub fn label(self) -> &'static str {
        use IndAsLine::*;
        match self {
            PropertyPlantAndEquipment => "Property, Plant and Equipment",
            CapitalWorkInProgress => "Capital work-in-progress",
            InvestmentProperty => "Investment Property",
            Goodwill => "Goodwill",
            OtherIntangibleAssets => "Other Intangible assets",
            IntangibleAssetsUnderDevelopment => "Intangible assets under development",
            NcInvestments | CInvestments => "Investments",
            NcTradeReceivables | CTradeReceivables => "Trade receivables",
            NcLoans | CLoans => "Loans",
            NcOtherFinancialAssets | COtherFinancialAssets => "Other financial assets",
            DeferredTaxAssets => "Deferred tax assets (net)",
            OtherNonCurrentAssets => "Other non-current assets",
            Inventories => "Inventories",
            CashAndCashEquivalents => "Cash and cash equivalents",
            OtherBankBalances => "Bank balances other than cash and cash equivalents",
            CurrentTaxAssets => "Current Tax Assets (Net)",
            OtherCurrentAssets => "Other current assets",
            EquityShareCapital => "Equity Share capital",
            OtherEquity => "Other Equity",
            NcBorrowings | CBorrowings => "Borrowings",
            NcLeaseLiabilities | CLeaseLiabilities => "Lease liabilities",
            NcTradePayables | CTradePayables => "Trade payables",
            NcOtherFinancialLiabilities | COtherFinancialLiabilities => "Other financial liabilities",
            NcProvisions | CProvisions => "Provisions",
            DeferredTaxLiabilities => "Deferred tax liabilities (Net)",
            OtherNonCurrentLiabilities => "Other non-current liabilities",
            OtherCurrentLiabilities => "Other current liabilities",
            CurrentTaxLiabilities => "Current Tax Liabilities (Net)",
            RevenueFromOperations => "Revenue from operations",
            OtherIncome => "Other income",
            CostOfMaterialsConsumed => "Cost of materials consumed",
            PurchasesOfStockInTrade => "Purchases of stock-in-trade",
            ChangesInInventories => "Changes in inventories of finished goods, work-in-progress and stock-in-trade",
            EmployeeBenefitsExpense => "Employee benefits expense",
            FinanceCosts => "Finance costs",
            DepreciationAndAmortisation => "Depreciation and amortization expense",
            OtherExpenses => "Other expenses",
            CurrentTax => "Current tax",
            DeferredTax => "Deferred tax",
            OciNotReclassified => "Items that will not be reclassified to profit or loss",
            OciReclassified => "Items that will be reclassified to profit or loss",
        }
    }

    /// The label with its section, for lists and note titles where the
    /// face's caption is not in view ("Borrowings (current)").
    pub fn long_label(self) -> String {
        use IndAsLine::*;
        let side = match self {
            NcInvestments | NcTradeReceivables | NcLoans | NcOtherFinancialAssets | NcBorrowings | NcLeaseLiabilities
            | NcTradePayables | NcOtherFinancialLiabilities | NcProvisions => " (non-current)",
            CInvestments | CTradeReceivables | CLoans | COtherFinancialAssets | CBorrowings | CLeaseLiabilities
            | CTradePayables | COtherFinancialLiabilities | CProvisions => " (current)",
            OciNotReclassified | OciReclassified => " (other comprehensive income)",
            _ => "",
        };
        format!("{}{side}", self.label())
    }

    pub fn code(self) -> String {
        code_of(self)
    }

    pub fn from_code(code: &str) -> Option<Self> {
        from_code(code)
    }

    pub fn is_oci(self) -> bool {
        matches!(self, Self::OciNotReclassified | Self::OciReclassified)
    }

    /// The Division I line a ledger of `nature` placed here is shown under;
    /// `None` when a group of that nature cannot map here.
    pub fn division1(self, nature: Nature) -> Option<LineItem> {
        use IndAsLine::*;
        use LineItem as L;
        if self.is_oci() {
            return match nature {
                Nature::Income => Some(L::OtherIncome),
                Nature::Expenses => Some(L::OtherExpenses),
                _ => None,
            };
        }
        let item = match self {
            PropertyPlantAndEquipment => L::PropertyPlantAndEquipment,
            CapitalWorkInProgress => L::CapitalWorkInProgress,
            InvestmentProperty | NcInvestments => L::NonCurrentInvestments,
            Goodwill | OtherIntangibleAssets => L::IntangibleAssets,
            IntangibleAssetsUnderDevelopment => L::IntangibleAssetsUnderDevelopment,
            // Division I: long-term trade receivables are other non-current assets.
            NcTradeReceivables | NcOtherFinancialAssets | OtherNonCurrentAssets => L::OtherNonCurrentAssets,
            NcLoans => L::LongTermLoansAndAdvances,
            DeferredTaxAssets => L::DeferredTaxAssets,
            Inventories => L::Inventories,
            CInvestments => L::CurrentInvestments,
            CTradeReceivables => L::TradeReceivables,
            CashAndCashEquivalents | OtherBankBalances => L::CashAndCashEquivalents,
            // Assumption: advance tax sits with loans and advances in Division I.
            CLoans | CurrentTaxAssets => L::ShortTermLoansAndAdvances,
            COtherFinancialAssets | OtherCurrentAssets => L::OtherCurrentAssets,
            EquityShareCapital => L::ShareCapital,
            OtherEquity => L::ReservesAndSurplus,
            // Division I lists long-term maturities of finance lease
            // obligations under long-term borrowings.
            NcBorrowings | NcLeaseLiabilities => L::LongTermBorrowings,
            // Division I: long-term trade payables are other long-term liabilities.
            NcTradePayables | NcOtherFinancialLiabilities | OtherNonCurrentLiabilities => L::OtherLongTermLiabilities,
            NcProvisions => L::LongTermProvisions,
            DeferredTaxLiabilities => L::DeferredTaxLiabilities,
            CBorrowings => L::ShortTermBorrowings,
            CTradePayables => L::TradePayables,
            // [UNCERTAIN] where Division I puts the current part of lease liabilities.
            CLeaseLiabilities | COtherFinancialLiabilities | OtherCurrentLiabilities => L::OtherCurrentLiabilities,
            CProvisions | CurrentTaxLiabilities => L::ShortTermProvisions,
            RevenueFromOperations => L::RevenueFromOperations,
            OtherIncome => L::OtherIncome,
            CostOfMaterialsConsumed => L::CostOfMaterialsConsumed,
            PurchasesOfStockInTrade => L::PurchasesOfStockInTrade,
            ChangesInInventories => return None,
            EmployeeBenefitsExpense => L::EmployeeBenefitsExpense,
            FinanceCosts => L::FinanceCosts,
            DepreciationAndAmortisation => L::DepreciationAndAmortisation,
            OtherExpenses => L::OtherExpenses,
            CurrentTax => L::CurrentTax,
            DeferredTax => L::DeferredTax,
            OciNotReclassified | OciReclassified => unreachable!("handled above"),
        };
        (item.nature() == Some(nature)).then_some(item)
    }

    fn family(self) -> Option<Family> {
        use IndAsLine::*;
        match self {
            NcBorrowings => Some(Family::Borrowings { current: false }),
            CBorrowings => Some(Family::Borrowings { current: true }),
            NcTradePayables | CTradePayables => Some(Family::Payables),
            NcTradeReceivables | CTradeReceivables => Some(Family::Receivables),
            _ => None,
        }
    }
}

// ------------------------------------------------------------- mapping codes

/// Lines that classification tags can qualify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Borrowings { current: bool },
    Payables,
    Receivables,
}

/// A classification a mapping can add to its line ("trade_payables:msme").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tag {
    /// Short-term borrowings: current maturities of long-term borrowings
    /// (shown separately since the 2021 amendment).
    CurrentMaturities,
    /// Borrowings or receivables: secured.
    Secured,
    /// Borrowings or receivables: unsecured.
    Unsecured,
    /// Trade payables: dues of micro and small enterprises.
    Msme,
    /// Trade payables or receivables: disputed (ageing schedule rows).
    Disputed,
    /// Trade receivables: considered doubtful (Division II: credit impaired).
    Doubtful,
}

impl Tag {
    pub const ALL: &'static [Tag] = &[Self::CurrentMaturities, Self::Secured, Self::Unsecured, Self::Msme, Self::Disputed, Self::Doubtful];

    pub fn code(self) -> String {
        code_of(self)
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }

    fn fits(self, family: Option<Family>) -> bool {
        match self {
            Tag::CurrentMaturities => family == Some(Family::Borrowings { current: true }),
            Tag::Secured | Tag::Unsecured => matches!(family, Some(Family::Borrowings { .. } | Family::Receivables)),
            Tag::Msme => family == Some(Family::Payables),
            Tag::Disputed => matches!(family, Some(Family::Payables | Family::Receivables)),
            Tag::Doubtful => family == Some(Family::Receivables),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tag::CurrentMaturities => "current maturities of long-term borrowings",
            Tag::Secured => "secured",
            Tag::Unsecured => "unsecured",
            Tag::Msme => "micro and small enterprises",
            Tag::Disputed => "disputed",
            Tag::Doubtful => "considered doubtful",
        }
    }
}

/// A set of [`Tag`]s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tags(u8);

impl Tags {
    pub fn has(self, t: Tag) -> bool {
        self.0 & t.bit() != 0
    }

    pub fn with(self, t: Tag) -> Self {
        Self(self.0 | t.bit())
    }

    pub fn iter(self) -> impl Iterator<Item = Tag> {
        Tag::ALL.iter().copied().filter(move |t| self.has(*t))
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl Serialize for Tags {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

/// The line a stored code names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLine {
    Division1(LineItem),
    Division2(IndAsLine),
}

/// A parsed line code: a line and its tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub line: TargetLine,
    pub tags: Tags,
}

impl Target {
    /// The Division I line and (when chosen) Division II line for a group of
    /// `nature`; `None` when such a group cannot map here.
    pub fn resolve(&self, nature: Nature) -> Option<(LineItem, Option<IndAsLine>)> {
        match self.line {
            TargetLine::Division1(item) => (item.nature() == Some(nature)).then_some((item, None)),
            TargetLine::Division2(line) => line.division1(nature).map(|item| (item, Some(line))),
        }
    }

    pub fn label(&self) -> String {
        let base = match self.line {
            TargetLine::Division1(i) => i.label().to_owned(),
            TargetLine::Division2(l) => format!("{} [Division II]", l.long_label()),
        };
        if self.tags.is_empty() {
            return base;
        }
        let tags: Vec<&str> = self.tags.iter().map(Tag::label).collect();
        format!("{base}: {}", tags.join(", "))
    }

    pub fn code(&self) -> String {
        let base = match self.line {
            TargetLine::Division1(i) => i.code(),
            TargetLine::Division2(l) => l.code(),
        };
        if self.tags.is_empty() {
            return base;
        }
        let tags: Vec<String> = self.tags.iter().map(Tag::code).collect();
        format!("{base}:{}", tags.join(","))
    }

    fn family(&self) -> Option<Family> {
        match self.line {
            TargetLine::Division1(i) => i.family(),
            TargetLine::Division2(l) => l.family(),
        }
    }
}

/// Read a stored line code: `<line>` or `<line>:<tag>,<tag>`, where the
/// line is a Division I code ("trade_payables") or a Division II one
/// ("other_financial_liabilities_current").
pub fn parse_code(code: &str) -> Result<Target, String> {
    let (line, tags) = match code.split_once(':') {
        Some((l, t)) => (l, Some(t)),
        None => (code, None),
    };
    let line = match (LineItem::from_code(line), IndAsLine::from_code(line)) {
        (Some(LineItem::ChangesInInventories), _) => return Err("changes in inventories is computed from stock and cannot be mapped".into()),
        (Some(i), _) => TargetLine::Division1(i),
        (None, Some(IndAsLine::ChangesInInventories)) => return Err("changes in inventories cannot be mapped".into()),
        (None, Some(l)) => TargetLine::Division2(l),
        (None, None) => return Err(format!("{line:?} is not a Schedule III line")),
    };
    let mut target = Target { line, tags: Tags::default() };
    for t in tags.into_iter().flat_map(|t| t.split(',')) {
        let tag: Tag = from_code(t).ok_or_else(|| format!("{t:?} is not a classification"))?;
        if !tag.fits(target.family()) {
            return Err(format!("\"{}\" does not apply to this line", tag.label()));
        }
        target.tags = target.tags.with(tag);
    }
    if target.tags.has(Tag::Secured) && target.tags.has(Tag::Unsecured) {
        return Err("a line cannot be both secured and unsecured".into());
    }
    Ok(target)
}

/// Check a code for `group` before it is stored: it must parse and suit
/// the group's nature.
pub fn validate(book: &Book, group: GroupId, code: &str) -> Result<Target, String> {
    let g = book.group(group).ok_or("unknown group")?;
    let nature = book.primary_group(g.id).map_or(g.nature, |p| p.nature);
    let target = parse_code(code)?;
    if target.resolve(nature).is_none() {
        return Err(format!("group \"{}\" ({nature:?}) cannot be shown under \"{}\"", g.name, target.label()));
    }
    Ok(target)
}

/// A choice offered when mapping a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineOption {
    pub code: String,
    pub label: String,
    /// 1 for Division I lines (used by both layouts), 2 for Division II only.
    pub division: u8,
}

/// The lines a group of `nature` can map to, with their tag variants.
pub fn line_options(nature: Nature) -> Vec<LineOption> {
    let mut out = Vec::new();
    let mut push = |t: Target, division: u8| {
        if t.resolve(nature).is_some() {
            out.push(LineOption { code: t.code(), label: t.label(), division });
        }
    };
    let tagsets: &[&[Tag]] = &[
        &[],
        &[Tag::CurrentMaturities],
        &[Tag::Secured],
        &[Tag::Unsecured],
        &[Tag::CurrentMaturities, Tag::Secured],
        &[Tag::CurrentMaturities, Tag::Unsecured],
        &[Tag::Msme],
        &[Tag::Disputed],
        &[Tag::Msme, Tag::Disputed],
        &[Tag::Doubtful],
        &[Tag::Disputed, Tag::Doubtful],
    ];
    for item in LineItem::ALL.iter().filter(|i| **i != LineItem::ChangesInInventories) {
        for set in tagsets {
            if set.iter().all(|t| t.fits(item.family())) {
                let tags = set.iter().fold(Tags::default(), |acc, t| acc.with(*t));
                push(Target { line: TargetLine::Division1(*item), tags }, 1);
            }
        }
    }
    for line in IndAsLine::ONLY_DIVISION2 {
        push(Target { line: TargetLine::Division2(*line), tags: Tags::default() }, 2);
    }
    out
}

/// The company's own mapping, read from its profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mapping {
    pub targets: BTreeMap<GroupId, Target>,
    /// Parties whose dues are shown as dues to micro and small enterprises.
    pub msme_parties: BTreeSet<LedgerId>,
    /// Stored codes that could not be read: group, code, why.
    pub unreadable: Vec<(GroupId, String, String)>,
}

impl Mapping {
    /// The mapping stored in the company profile (empty without one).
    pub fn of_book(book: &Book) -> Self {
        let mut m = Self::default();
        let Some(p) = book.profile() else { return m };
        for (g, code) in &p.schedule3_map {
            match parse_code(code) {
                Ok(t) => {
                    m.targets.insert(*g, t);
                }
                Err(e) => m.unreadable.push((*g, code.clone(), e)),
            }
        }
        m.msme_parties = p.msme_parties.clone();
        m
    }

    /// Plain Division I lines, without tags.
    pub fn of_lines<I: IntoIterator<Item = (GroupId, LineItem)>>(lines: I) -> Self {
        Self {
            targets: lines.into_iter().map(|(g, i)| (g, Target { line: TargetLine::Division1(i), tags: Tags::default() })).collect(),
            ..Self::default()
        }
    }
}

/// The statement layout a company chose: Division I (Accounting Standards,
/// the default) or Division II (Ind AS).
pub fn division_of(book: &Book) -> u8 {
    match book.profile().and_then(|p| p.schedule3_division) {
        Some(2) => 2,
        _ => 1,
    }
}

// ------------------------------------------------------------------ rounding

/// The unit figures are shown in. Schedule III (2021 amendment) requires
/// rounding: below ₹100 crore total income to hundreds, thousands, lakhs or
/// millions (or decimals thereof); at or above it to lakhs, millions or
/// crores. `Rupees` (exact) is kept for working papers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    #[default]
    Rupees,
    Hundreds,
    Thousands,
    Lakhs,
    Millions,
    Crores,
}

/// ₹100 crore: the total-income threshold of the rounding rule.
pub const ROUNDING_THRESHOLD: i64 = 1_000_000_000;

impl Rounding {
    pub const ALL: &'static [Rounding] = &[Self::Rupees, Self::Hundreds, Self::Thousands, Self::Lakhs, Self::Millions, Self::Crores];

    pub fn code(self) -> String {
        code_of(self)
    }

    pub fn from_code(code: &str) -> Option<Self> {
        from_code(code)
    }

    pub fn divisor(self) -> Amount {
        Amount::from(match self {
            Rounding::Rupees => 1i64,
            Rounding::Hundreds => 100,
            Rounding::Thousands => 1_000,
            Rounding::Lakhs => 100_000,
            Rounding::Millions => 1_000_000,
            Rounding::Crores => 10_000_000,
        })
    }

    /// "₹ in lakhs"; "₹" for rupees.
    pub fn unit(self) -> &'static str {
        match self {
            Rounding::Rupees => "₹",
            Rounding::Hundreds => "₹ in hundreds",
            Rounding::Thousands => "₹ in thousands",
            Rounding::Lakhs => "₹ in lakhs",
            Rounding::Millions => "₹ in millions",
            Rounding::Crores => "₹ in crores",
        }
    }

    /// `amount` in this unit, to two decimals.
    pub fn round(self, amount: Amount) -> Amount {
        if self == Rounding::Rupees {
            return amount;
        }
        (amount / self.divisor()).round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
    }

    /// Whether Schedule III allows this unit for a company with `total_income`.
    pub fn allowed(self, total_income: Amount) -> bool {
        let large = total_income >= Amount::from(ROUNDING_THRESHOLD);
        match self {
            Rounding::Rupees => false,
            Rounding::Hundreds | Rounding::Thousands => !large,
            Rounding::Lakhs | Rounding::Millions => true,
            Rounding::Crores => large,
        }
    }

    /// The company's choice (rupees when none is stored).
    pub fn of_book(book: &Book) -> Self {
        book.profile().and_then(|p| p.schedule3_rounding.as_deref()).and_then(Self::from_code).unwrap_or_default()
    }

    /// A warning when this unit does not suit the rule for `total_income`.
    pub fn warning(self, total_income: Amount) -> Option<String> {
        if self.allowed(total_income) {
            return None;
        }
        let units = if total_income >= Amount::from(ROUNDING_THRESHOLD) {
            "lakhs, millions or crores"
        } else {
            "hundreds, thousands, lakhs or millions"
        };
        Some(format!(
            "Rounding: Schedule III requires figures rounded to {units} for this company's total income; choose a unit in the Schedule III mapping."
        ))
    }
}

// ------------------------------------------------------------ default mapping

/// Where each default group goes, by default-group name (resolved to its
/// stable id). Every default group is listed, so a company that keeps the
/// default chart never falls back.
pub const DEFAULT_MAPPING: &[(&str, LineItem)] = &[
    ("Capital Account", LineItem::ShareCapital),
    ("Reserves & Surplus", LineItem::ReservesAndSurplus),
    // Assumption: loans are long-term until the user says otherwise.
    ("Loans (Liability)", LineItem::LongTermBorrowings),
    ("Secured Loans", LineItem::LongTermBorrowings),
    ("Unsecured Loans", LineItem::LongTermBorrowings),
    ("Bank OD A/c", LineItem::ShortTermBorrowings),
    ("Bank OCC A/c", LineItem::ShortTermBorrowings),
    // Assumption: inter-branch balances are settled within the year.
    ("Branch / Divisions", LineItem::OtherCurrentLiabilities),
    ("Current Liabilities", LineItem::OtherCurrentLiabilities),
    ("Duties & Taxes", LineItem::OtherCurrentLiabilities),
    ("Provisions", LineItem::ShortTermProvisions),
    ("Sundry Creditors", LineItem::TradePayables),
    // Should be nil when accounts are finalised; a warning flags a balance.
    ("Suspense Account", LineItem::OtherCurrentLiabilities),
    ("Fixed Assets", LineItem::PropertyPlantAndEquipment),
    ("Investments", LineItem::NonCurrentInvestments),
    ("Current Assets", LineItem::OtherCurrentAssets),
    ("Bank Accounts", LineItem::CashAndCashEquivalents),
    ("Cash-in-Hand", LineItem::CashAndCashEquivalents),
    ("Deposits (Asset)", LineItem::ShortTermLoansAndAdvances),
    ("Loans & Advances (Asset)", LineItem::ShortTermLoansAndAdvances),
    ("Stock-in-Hand", LineItem::Inventories),
    ("Sundry Debtors", LineItem::TradeReceivables),
    ("Misc. Expenses (Asset)", LineItem::OtherCurrentAssets),
    ("Sales Accounts", LineItem::RevenueFromOperations),
    // Direct incomes are taken as other operating revenue.
    ("Direct Incomes", LineItem::RevenueFromOperations),
    ("Indirect Incomes", LineItem::OtherIncome),
    // Assumption: a trading business. Manufacturers map raw-material
    // purchases to cost of materials consumed.
    ("Purchase Accounts", LineItem::PurchasesOfStockInTrade),
    ("Direct Expenses", LineItem::OtherExpenses),
    ("Indirect Expenses", LineItem::OtherExpenses),
];

/// Default groups whose Division II line differs from the translation of
/// their Division I line.
pub const DIVISION2_DEFAULTS: &[(&str, IndAsLine)] = &[
    // Security and other deposits given are financial assets under Ind AS.
    ("Deposits (Asset)", IndAsLine::COtherFinancialAssets),
];

/// Where a ledger under no mapped group goes. Defensive: the core keeps
/// every group under a default primary group, all of which are mapped.
fn fallback(nature: Nature) -> LineItem {
    match nature {
        Nature::Liabilities => LineItem::OtherCurrentLiabilities,
        Nature::Assets => LineItem::OtherCurrentAssets,
        Nature::Income => LineItem::OtherIncome,
        Nature::Expenses => LineItem::OtherExpenses,
    }
}

// --------------------------------------------------------------- statements

/// A line set a statement can be laid out in.
pub trait StatementLine: Copy + Ord + std::fmt::Debug + Serialize + 'static {
    /// Schedule III division (1 or 2).
    const DIVISION: u8;
    fn label(self) -> &'static str;
    /// Title for its note ("Borrowings (current)").
    fn note_title(self) -> String;
    /// Debit balances show positive (assets, expenses); credit otherwise.
    fn debit_positive(self) -> bool;
    /// The line a placed ledger is shown under.
    fn of(p: &Placement) -> Self;
    fn note_kind(self) -> NoteKind;
    fn code(self) -> String {
        code_of(self)
    }
}

/// Which format a line's note takes (see [`crate::schedule3_notes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    ShareCapital,
    Reserves,
    Borrowings,
    TradePayables,
    TradeReceivables,
    FixedAssets,
    WorkInProgress,
    Inventories,
    Cash,
    ChangesInInventories,
    /// The line's ledgers, one row each.
    Ledgers,
}

impl StatementLine for LineItem {
    const DIVISION: u8 = 1;
    fn label(self) -> &'static str {
        LineItem::label(self)
    }
    fn note_title(self) -> String {
        LineItem::label(self).to_owned()
    }
    fn debit_positive(self) -> bool {
        !matches!(self.nature(), Some(Nature::Liabilities | Nature::Income))
    }
    fn of(p: &Placement) -> Self {
        p.item
    }
    fn note_kind(self) -> NoteKind {
        use LineItem::*;
        match self {
            ShareCapital => NoteKind::ShareCapital,
            ReservesAndSurplus => NoteKind::Reserves,
            LongTermBorrowings | ShortTermBorrowings => NoteKind::Borrowings,
            TradePayables => NoteKind::TradePayables,
            TradeReceivables => NoteKind::TradeReceivables,
            PropertyPlantAndEquipment | IntangibleAssets => NoteKind::FixedAssets,
            CapitalWorkInProgress | IntangibleAssetsUnderDevelopment => NoteKind::WorkInProgress,
            Inventories => NoteKind::Inventories,
            CashAndCashEquivalents => NoteKind::Cash,
            ChangesInInventories => NoteKind::ChangesInInventories,
            _ => NoteKind::Ledgers,
        }
    }
}

impl StatementLine for IndAsLine {
    const DIVISION: u8 = 2;
    fn label(self) -> &'static str {
        IndAsLine::label(self)
    }
    fn note_title(self) -> String {
        self.long_label()
    }
    fn debit_positive(self) -> bool {
        // OCI shows gains positive, like income.
        if self.is_oci() {
            return false;
        }
        self.division1(Nature::Assets).is_some() || self.division1(Nature::Expenses).is_some() || self == IndAsLine::ChangesInInventories
    }
    fn of(p: &Placement) -> Self {
        p.ind_as
    }
    fn note_kind(self) -> NoteKind {
        use IndAsLine::*;
        match self {
            EquityShareCapital => NoteKind::ShareCapital,
            OtherEquity => NoteKind::Reserves,
            NcBorrowings | CBorrowings => NoteKind::Borrowings,
            NcTradePayables | CTradePayables => NoteKind::TradePayables,
            NcTradeReceivables | CTradeReceivables => NoteKind::TradeReceivables,
            PropertyPlantAndEquipment | InvestmentProperty | Goodwill | OtherIntangibleAssets => NoteKind::FixedAssets,
            CapitalWorkInProgress | IntangibleAssetsUnderDevelopment => NoteKind::WorkInProgress,
            Inventories => NoteKind::Inventories,
            CashAndCashEquivalents | OtherBankBalances => NoteKind::Cash,
            ChangesInInventories => NoteKind::ChangesInInventories,
            _ => NoteKind::Ledgers,
        }
    }
}

/// What a line's figure is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Ledgers under `group` (the group whose mapping placed them).
    Group,
    /// Closing stock as valued by inventory.
    ClosingStock,
    /// Opening stock as valued by inventory.
    OpeningStock,
    /// Accumulated profit up to the start of the financial year.
    ProfitBroughtForward,
    /// Profit from the start of the financial year to the balance-sheet date.
    ProfitForTheYear,
    /// Opening balances that do not tally.
    OpeningDifference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    pub kind: SourceKind,
    pub group: Option<GroupId>,
    pub name: String,
    pub amount: Amount,
    pub previous: Amount,
    /// For a group: every ledger under it (at any depth) is in this line,
    /// so the group's own summary shows exactly this figure.
    pub whole_group: bool,
}

/// One ledger's part of a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerAmount {
    pub ledger: LedgerId,
    pub name: String,
    pub group: GroupId,
    pub group_name: String,
    pub amount: Amount,
    pub previous: Amount,
    /// Classifications from the mapping (MSME, disputed, secured…).
    pub tags: Tags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Heading,
    Line,
    Total,
    /// A breakdown printed under a line ("(A) dues of micro and small
    /// enterprises"); not added into any total.
    Detail,
}

/// One row of the vertical statement, in print order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row<K = LineItem> {
    pub kind: RowKind,
    /// Indent: 0 for the main headings, 1 for sections, 2 to 4 for lines.
    pub level: u8,
    pub label: String,
    /// `None` for headings, totals, details and the opening-difference line.
    pub item: Option<K>,
    /// Note number, for lines with an item (numbered in order shown).
    pub note: Option<u32>,
    pub amount: Amount,
    pub previous: Amount,
    pub sources: Vec<Source>,
    pub ledgers: Vec<LedgerAmount>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Figures {
    pub current: Amount,
    pub previous: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BalanceSheet<K = LineItem> {
    /// Schedule III division of the layout (1 or 2).
    pub division: u8,
    /// Unit of every figure ("₹", "₹ in lakhs"), after [`BalanceSheet::round`].
    pub unit: String,
    pub as_at: NaiveDate,
    /// The comparative date; `None` for a first year (previous figures are zero).
    pub previous_as_at: Option<NaiveDate>,
    pub rows: Vec<Row<K>>,
    pub total_equity_and_liabilities: Figures,
    pub total_assets: Figures,
    /// What the user must still classify or supply.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfitAndLoss<K = LineItem> {
    pub division: u8,
    pub unit: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub previous_from: Option<NaiveDate>,
    pub previous_to: Option<NaiveDate>,
    pub rows: Vec<Row<K>>,
    pub total_income: Figures,
    pub total_expenses: Figures,
    pub profit_before_tax: Figures,
    pub tax_expense: Figures,
    /// Negative for a loss. Division I: equals the core report's net profit.
    pub profit_for_the_period: Figures,
    /// Division II only (zero in Division I).
    pub other_comprehensive_income: Figures,
    /// Profit plus other comprehensive income: equals the core report's net
    /// profit in both divisions.
    pub total_comprehensive_income: Figures,
    pub warnings: Vec<String>,
}

fn round_figures(f: &mut Figures, r: Rounding) {
    f.current = r.round(f.current);
    f.previous = r.round(f.previous);
}

fn round_rows<K>(rows: &mut [Row<K>], r: Rounding) {
    for row in rows {
        row.amount = r.round(row.amount);
        row.previous = r.round(row.previous);
        for s in &mut row.sources {
            s.amount = r.round(s.amount);
            s.previous = r.round(s.previous);
        }
        for l in &mut row.ledgers {
            l.amount = r.round(l.amount);
            l.previous = r.round(l.previous);
        }
    }
}

impl<K> BalanceSheet<K> {
    /// Show every figure in `r`'s unit. Each figure is rounded on its own,
    /// so a rounded total can differ from the sum of rounded lines.
    pub fn round(&mut self, r: Rounding) {
        round_rows(&mut self.rows, r);
        round_figures(&mut self.total_equity_and_liabilities, r);
        round_figures(&mut self.total_assets, r);
        self.unit = r.unit().to_owned();
    }
}

impl<K> ProfitAndLoss<K> {
    pub fn round(&mut self, r: Rounding) {
        round_rows(&mut self.rows, r);
        for f in [
            &mut self.total_income,
            &mut self.total_expenses,
            &mut self.profit_before_tax,
            &mut self.tax_expense,
            &mut self.profit_for_the_period,
            &mut self.other_comprehensive_income,
            &mut self.total_comprehensive_income,
        ] {
            round_figures(f, r);
        }
        self.unit = r.unit().to_owned();
    }

    /// Number the notes from `first` on (they follow the Balance Sheet's).
    pub fn renumber(&mut self, first: u32) {
        for (n, row) in (first..).zip(self.rows.iter_mut().filter(|r| r.note.is_some())) {
            row.note = Some(n);
        }
    }
}

// ------------------------------------------------------------- placement

/// Where a group's ledgers go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub item: LineItem,
    pub ind_as: IndAsLine,
    pub tags: Tags,
    /// The group whose mapping applied (the primary group on fallback).
    pub group: GroupId,
    pub fallback: bool,
}

/// `id` and its ancestors, nearest first (bounded against parent cycles).
pub(crate) fn chain(book: &Book, id: GroupId) -> Vec<GroupId> {
    let mut out = Vec::new();
    let mut at = Some(id);
    for _ in 0..=book.groups().count() {
        let Some(g) = at.and_then(|g| book.group(g)) else { break };
        out.push(g.id);
        at = g.parent;
    }
    out
}

/// Every group's placement under `mapping`, with warnings about mappings
/// that could not be used.
pub(crate) struct Mapper<'a> {
    pub(crate) book: &'a Book,
    pub(crate) placements: BTreeMap<GroupId, Placement>,
    pub(crate) warnings: Vec<String>,
    msme: BTreeSet<LedgerId>,
}

impl<'a> Mapper<'a> {
    pub(crate) fn new(book: &'a Book, mapping: &Mapping) -> Self {
        let defaults: BTreeMap<GroupId, LineItem> =
            DEFAULT_MAPPING.iter().map(|(name, item)| (GroupId::default_named(name), *item)).collect();
        let defaults2: BTreeMap<GroupId, IndAsLine> =
            DIVISION2_DEFAULTS.iter().map(|(name, line)| (GroupId::default_named(name), *line)).collect();
        let mut warnings = Vec::new();
        for (g, code, why) in &mapping.unreadable {
            let name = book.group(*g).map_or("?", |g| g.name.as_str());
            warnings.push(format!("The mapping of group \"{name}\" ({code:?}) cannot be read: {why}; it was ignored."));
        }
        for (id, t) in &mapping.targets {
            let Some(g) = book.group(*id) else {
                warnings.push(format!("A mapping to \"{}\" names a group that no longer exists; it was ignored.", t.label()));
                continue;
            };
            let nature = book.primary_group(g.id).map_or(g.nature, |p| p.nature);
            if t.resolve(nature).is_none() {
                warnings.push(format!("Group \"{}\" ({nature:?}) cannot be shown under \"{}\"; that mapping was ignored.", g.name, t.label()));
            }
        }
        let mut placements = BTreeMap::new();
        for g in book.groups() {
            let ids = chain(book, g.id);
            let primary = ids.last().and_then(|p| book.group(*p)).unwrap_or(g);
            let nature = primary.nature;
            let found = ids.iter().find_map(|id| {
                if let Some(t) = mapping.targets.get(id) {
                    if let Some((item, line)) = t.resolve(nature) {
                        let ind_as = line.unwrap_or(item.ind_as());
                        return Some(Placement { item, ind_as, tags: t.tags, group: *id, fallback: false });
                    }
                }
                defaults.get(id).filter(|i| i.nature() == Some(nature)).map(|item| Placement {
                    item: *item,
                    ind_as: defaults2.get(id).copied().unwrap_or(item.ind_as()),
                    tags: Tags::default(),
                    group: *id,
                    fallback: false,
                })
            });
            let p = found.unwrap_or_else(|| {
                let item = fallback(nature);
                Placement { item, ind_as: item.ind_as(), tags: Tags::default(), group: primary.id, fallback: true }
            });
            placements.insert(g.id, p);
        }
        Self { book, placements, warnings, msme: mapping.msme_parties.clone() }
    }

    pub(crate) fn nature_of(&self, ledger: LedgerId) -> Option<Nature> {
        let l = self.book.ledger(ledger)?;
        self.book.primary_group(l.group).map(|g| g.nature)
    }

    /// Where `ledger` is shown.
    pub(crate) fn of_ledger(&self, ledger: LedgerId) -> Option<Placement> {
        self.book.ledger(ledger).and_then(|l| self.placements.get(&l.group)).copied()
    }

    /// Whether every group under `group` lands in `item`.
    fn whole<K: StatementLine>(&self, group: GroupId, item: K) -> bool {
        self.book
            .groups()
            .filter(|g| chain(self.book, g.id).contains(&group))
            .all(|g| self.placements.get(&g.id).is_some_and(|p| K::of(p) == item))
    }
}

/// Every group's placement under `mapping` (for screens that show where
/// each group goes).
pub fn placements(book: &Book, mapping: &Mapping) -> BTreeMap<GroupId, Placement> {
    Mapper::new(book, mapping).placements
}

// ----------------------------------------------------------- accumulation

struct Line {
    figures: [Amount; 2],
    sources: Vec<Source>,
    ledgers: BTreeMap<LedgerId, LedgerAmount>,
}

impl Default for Line {
    fn default() -> Self {
        Self { figures: [Amount::ZERO; 2], sources: Vec::new(), ledgers: BTreeMap::new() }
    }
}

impl Line {
    fn source(&mut self, kind: SourceKind, group: Option<GroupId>, name: &str) -> &mut Source {
        let at = match self.sources.iter().position(|s| s.kind == kind && s.group == group) {
            Some(i) => i,
            None => {
                self.sources.push(Source {
                    kind,
                    group,
                    name: name.to_owned(),
                    amount: Amount::ZERO,
                    previous: Amount::ZERO,
                    whole_group: false,
                });
                self.sources.len() - 1
            }
        };
        &mut self.sources[at]
    }
}

fn bump(s: &mut Source, p: usize, a: Amount) {
    if p == 0 { s.amount += a } else { s.previous += a }
}

struct Builder<'a, K> {
    m: Mapper<'a>,
    lines: BTreeMap<K, Line>,
    fallbacks: BTreeMap<GroupId, LineItem>,
}

impl<'a, K: StatementLine> Builder<'a, K> {
    fn new(book: &'a Book, mapping: &Mapping) -> Self {
        Self { m: Mapper::new(book, mapping), lines: BTreeMap::new(), fallbacks: BTreeMap::new() }
    }

    /// Add signed (debit positive) ledger amounts of the given natures to
    /// period `p` (0 current, 1 previous).
    fn ledgers(&mut self, p: usize, amounts: &BTreeMap<LedgerId, Amount>, natures: &[Nature]) {
        let book = self.m.book;
        for (&id, &signed) in amounts {
            if signed.is_zero() || !self.m.nature_of(id).is_some_and(|n| natures.contains(&n)) {
                continue;
            }
            let Some(ledger) = book.ledger(id) else { continue };
            let Some(place) = self.m.placements.get(&ledger.group).copied() else { continue };
            if place.fallback {
                self.fallbacks.insert(place.group, place.item);
            }
            let key = K::of(&place);
            let amount = if key.debit_positive() { signed } else { -signed };
            let line = self.lines.entry(key).or_default();
            line.figures[p] += amount;
            let group_name = book.group(place.group).map(|g| g.name.clone()).unwrap_or_default();
            bump(line.source(SourceKind::Group, Some(place.group), &group_name), p, amount);
            let row = line.ledgers.entry(id).or_insert_with(|| LedgerAmount {
                ledger: id,
                name: ledger.name.clone(),
                group: ledger.group,
                group_name: book.group(ledger.group).map(|g| g.name.clone()).unwrap_or_default(),
                amount: Amount::ZERO,
                previous: Amount::ZERO,
                tags: place.tags,
            });
            if p == 0 { row.amount += amount } else { row.previous += amount }
        }
    }

    /// Add a figure that is not a ledger balance (stock, profit).
    fn computed(&mut self, item: K, p: usize, kind: SourceKind, name: &str, amount: Amount) {
        let line = self.lines.entry(item).or_default();
        line.figures[p] += amount;
        bump(line.source(kind, None, name), p, amount);
    }

    fn figures(&self, item: K) -> Figures {
        self.lines.get(&item).map_or_else(Figures::default, |l| Figures { current: l.figures[0], previous: l.figures[1] })
    }

    fn sum(&self, items: &[K]) -> Figures {
        items.iter().fold(Figures::default(), |acc, i| add(acc, self.figures(*i)))
    }

    fn shown(&self, item: K, always: &[K]) -> bool {
        always.contains(&item) || nonzero(self.figures(item))
    }

    /// The row for `item`, with its drill-down detail.
    fn line_row(&self, item: K, level: u8, note: &mut u32) -> Row<K> {
        let f = self.figures(item);
        let (mut sources, mut ledgers) = match self.lines.get(&item) {
            Some(l) => (
                l.sources.iter().filter(|s| !s.amount.is_zero() || !s.previous.is_zero()).cloned().collect::<Vec<_>>(),
                l.ledgers.values().filter(|r| !r.amount.is_zero() || !r.previous.is_zero()).cloned().collect::<Vec<_>>(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        for s in &mut sources {
            if let (SourceKind::Group, Some(g)) = (s.kind, s.group) {
                s.whole_group = self.m.whole(g, item);
            }
        }
        ledgers.sort_by_key(|a| a.name.to_lowercase());
        *note += 1;
        Row {
            kind: RowKind::Line,
            level,
            label: item.label().to_owned(),
            item: Some(item),
            note: Some(*note),
            amount: f.current,
            previous: f.previous,
            sources,
            ledgers,
        }
    }

    fn warnings(&self) -> Vec<String> {
        let book = self.m.book;
        let mut out = self.m.warnings.clone();
        for (g, item) in &self.fallbacks {
            let name = book.group(*g).map(|g| g.name.as_str()).unwrap_or("?");
            out.push(format!("Group \"{name}\" has no Schedule III mapping; its ledgers are shown under \"{}\".", item.label()));
        }
        out
    }
}

fn row<K>(kind: RowKind, level: u8, label: &str, f: Figures) -> Row<K> {
    Row {
        kind,
        level,
        label: label.to_owned(),
        item: None,
        note: None,
        amount: f.current,
        previous: f.previous,
        sources: Vec::new(),
        ledgers: Vec::new(),
    }
}

/// Part of a balance-sheet section: a line, or a captioned set of lines.
enum Part<K: 'static> {
    Item(K),
    Caption(&'static str, &'static [K]),
}

fn items<K: Copy>(parts: &[Part<K>]) -> Vec<K> {
    parts
        .iter()
        .flat_map(|p| match p {
            Part::Item(i) => vec![*i],
            Part::Caption(_, is) => is.to_vec(),
        })
        .collect()
}

/// Rows for one section: its heading at `base` (when any line shows) and
/// its lines below it. Trade payables get their MSME breakdown.
fn section<K: StatementLine>(b: &Builder<K>, rows: &mut Vec<Row<K>>, note: &mut u32, label: &str, parts: &[Part<K>], always: &[K], base: u8) {
    let all = items(parts);
    if !all.iter().any(|i| b.shown(*i, always)) {
        return;
    }
    let mut line = |rows: &mut Vec<Row<K>>, i: K, level: u8| {
        let r = b.line_row(i, level, note);
        let details = msme_details(b, &r, level + 1);
        rows.push(r);
        rows.extend(details);
    };
    // A section that is a single line of the same name prints as that line.
    if let [Part::Item(only)] = parts {
        if only.label() == label {
            line(rows, *only, base);
            return;
        }
    }
    rows.push(row(RowKind::Heading, base, label, Figures::default()));
    for p in parts {
        match p {
            Part::Item(i) if b.shown(*i, always) => line(rows, *i, base + 1),
            Part::Item(_) => {}
            Part::Caption(cap, is) if is.iter().any(|i| b.shown(*i, always)) => {
                rows.push(row(RowKind::Heading, base + 1, cap, Figures::default()));
                for i in is.iter().filter(|i| b.shown(**i, always)) {
                    line(rows, *i, base + 2);
                }
            }
            Part::Caption(..) => {}
        }
    }
}

/// Whether a ledger's dues are to a micro or small enterprise.
pub(crate) fn is_msme(mapping: &Mapping, l: &LedgerAmount) -> bool {
    l.tags.has(Tag::Msme) || mapping.msme_parties.contains(&l.ledger)
}

/// The face breakdown of trade payables: (A) dues of micro and small
/// enterprises, (B) dues of others.
fn msme_details<K: StatementLine>(b: &Builder<K>, r: &Row<K>, level: u8) -> Vec<Row<K>> {
    if r.item.map(StatementLine::note_kind) != Some(NoteKind::TradePayables) {
        return Vec::new();
    }
    let msme = |l: &&LedgerAmount| l.tags.has(Tag::Msme) || b.m.msme.contains(&l.ledger);
    let (small, others): (Vec<&LedgerAmount>, Vec<&LedgerAmount>) = r.ledgers.iter().partition(msme);
    let detail = |label: &str, ls: Vec<&LedgerAmount>| {
        let f = ls.iter().fold(Figures::default(), |acc, l| add(acc, Figures { current: l.amount, previous: l.previous }));
        let mut d = row::<K>(RowKind::Detail, level, label, f);
        d.ledgers = ls.into_iter().cloned().collect();
        d
    };
    vec![
        detail("(A) total outstanding dues of micro enterprises and small enterprises", small),
        detail("(B) total outstanding dues of creditors other than micro enterprises and small enterprises", others),
    ]
}

fn nonzero(f: Figures) -> bool {
    !f.current.is_zero() || !f.previous.is_zero()
}

pub(crate) fn add(a: Figures, b: Figures) -> Figures {
    Figures { current: a.current + b.current, previous: a.previous + b.previous }
}

fn sub(a: Figures, b: Figures) -> Figures {
    Figures { current: a.current - b.current, previous: a.previous - b.previous }
}

// ---------------------------------------------------------- balance sheet

use LineItem as L;

const SHAREHOLDERS: &[Part<LineItem>] =
    &[Part::Item(L::ShareCapital), Part::Item(L::ReservesAndSurplus), Part::Item(L::MoneyReceivedAgainstShareWarrants)];
const SHARE_APPLICATION: &[Part<LineItem>] = &[Part::Item(L::ShareApplicationMoneyPendingAllotment)];
const NON_CURRENT_LIABILITIES: &[Part<LineItem>] = &[
    Part::Item(L::LongTermBorrowings),
    Part::Item(L::DeferredTaxLiabilities),
    Part::Item(L::OtherLongTermLiabilities),
    Part::Item(L::LongTermProvisions),
];
const CURRENT_LIABILITIES: &[Part<LineItem>] = &[
    Part::Item(L::ShortTermBorrowings),
    Part::Item(L::TradePayables),
    Part::Item(L::OtherCurrentLiabilities),
    Part::Item(L::ShortTermProvisions),
];
const NON_CURRENT_ASSETS: &[Part<LineItem>] = &[
    Part::Caption(
        "Property, Plant and Equipment and Intangible assets",
        &[L::PropertyPlantAndEquipment, L::IntangibleAssets, L::CapitalWorkInProgress, L::IntangibleAssetsUnderDevelopment],
    ),
    Part::Item(L::NonCurrentInvestments),
    Part::Item(L::DeferredTaxAssets),
    Part::Item(L::LongTermLoansAndAdvances),
    Part::Item(L::OtherNonCurrentAssets),
];
const CURRENT_ASSETS: &[Part<LineItem>] = &[
    Part::Item(L::CurrentInvestments),
    Part::Item(L::Inventories),
    Part::Item(L::TradeReceivables),
    Part::Item(L::CashAndCashEquivalents),
    Part::Item(L::ShortTermLoansAndAdvances),
    Part::Item(L::OtherCurrentAssets),
];

/// Division I lines that are current assets and current liabilities (for
/// the ratios).
pub(crate) fn current_assets_and_liabilities() -> (Vec<LineItem>, Vec<LineItem>) {
    (items(CURRENT_ASSETS), items(CURRENT_LIABILITIES))
}

/// Balances, stock and profit for both dates, in a fresh builder.
fn balance_builder<'a, K: StatementLine>(
    book: &'a Book,
    as_at: NaiveDate,
    previous_as_at: Option<NaiveDate>,
    mapping: &Mapping,
    inventories: K,
    reserves: K,
) -> (Builder<'a, K>, Figures) {
    let mut b = Builder::new(book, mapping);
    let mut diff = Figures::default();
    for (p, date) in [(0, Some(as_at)), (1, previous_as_at)] {
        let Some(d) = date else { continue };
        b.ledgers(p, &book.balances(Some(d)), &[Nature::Assets, Nature::Liabilities]);
        let core = reports::balance_sheet(book, d);
        b.computed(inventories, p, SourceKind::ClosingStock, "Closing stock", core.closing_stock);
        // Surplus in the Statement of Profit and Loss: the year's profit and
        // what was brought forward (accumulated profit less the year's).
        let year_start = FinancialYear::containing(d, FY_START_MONTH).start;
        let year = reports::profit_and_loss(book, year_start, d).net_profit;
        b.computed(reserves, p, SourceKind::ProfitBroughtForward, "Surplus: balance brought forward", core.profit_and_loss - year);
        b.computed(reserves, p, SourceKind::ProfitForTheYear, "Surplus: profit for the year", year);
        if p == 0 { diff.current = core.opening_difference } else { diff.previous = core.opening_difference }
    }
    (b, diff)
}

fn opening_difference_row<K>(diff: Figures) -> Row<K> {
    let mut r = row(RowKind::Line, 1, "Difference in opening balances", diff);
    r.sources.push(Source {
        kind: SourceKind::OpeningDifference,
        group: None,
        name: "Difference in opening balances".into(),
        amount: diff.current,
        previous: diff.previous,
        whole_group: false,
    });
    r
}

/// Warnings common to both divisions' balance sheets.
fn balance_warnings<K: StatementLine>(b: &Builder<K>, diff: Figures) -> Vec<String> {
    let mut warnings = b.warnings();
    let by_kind = |kind: NoteKind| b.lines.iter().filter(move |(k, _)| k.note_kind() == kind).map(|(_, l)| l);
    let payables: Vec<&LedgerAmount> = by_kind(NoteKind::TradePayables).flat_map(|l| l.ledgers.values()).collect();
    if payables.iter().any(|l| !l.amount.is_zero()) && !payables.iter().any(|l| l.tags.has(Tag::Msme) || b.m.msme.contains(&l.ledger)) {
        warnings.push(
            "Trade payables: no supplier is marked MSME (micro and small enterprises), so all dues are shown as \
             dues to others. Mark MSME suppliers in the Schedule III mapping (Schedule III requires the split)."
                .into(),
        );
    }
    let placed = |pred: &dyn Fn(&Placement) -> bool| {
        b.lines
            .values()
            .flat_map(|l| l.ledgers.values())
            .any(|l| nonzero(Figures { current: l.amount, previous: l.previous }) && b.m.of_ledger(l.ledger).is_some_and(|p| pred(&p)))
    };
    if placed(&|p| p.item == L::LongTermBorrowings) && !b.m.placements.values().any(|p| p.tags.has(Tag::CurrentMaturities)) {
        warnings.push(
            "Loans are shown as long-term borrowings. Current maturities of long-term borrowings belong under \
             short-term borrowings (2021 amendment): map the groups that hold them as current maturities."
                .into(),
        );
    }
    if placed(&|p| p.item == L::NonCurrentInvestments) {
        warnings.push("Investments are shown as non-current; map current investments separately.".into());
    }
    let suspense = GroupId::default_named("Suspense Account");
    if b.lines.values().flat_map(|l| &l.sources).any(|s| s.group == Some(suspense) && (!s.amount.is_zero() || !s.previous.is_zero())) {
        warnings.push("Suspense Account has a balance (shown in other current liabilities); clear it before finalising.".into());
    }
    if nonzero(diff) {
        warnings.push("Opening balances do not tally; correct them before finalising.".into());
    }
    warnings
}

/// The Schedule III (Division I) Balance Sheet as at `as_at`, with
/// `previous_as_at` as the comparative (Schedule III requires one; `None`
/// for a first year), under the company's `mapping`.
pub fn balance_sheet(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>, mapping: &Mapping) -> BalanceSheet {
    let (b, diff) = balance_builder(book, as_at, previous_as_at, mapping, L::Inventories, L::ReservesAndSurplus);
    let always = [L::ShareCapital, L::ReservesAndSurplus];
    let mut rows = Vec::new();
    let mut note = 0;
    rows.push(row(RowKind::Heading, 0, "EQUITY AND LIABILITIES", Figures::default()));
    section(&b, &mut rows, &mut note, "Shareholders' funds", SHAREHOLDERS, &always, 1);
    section(&b, &mut rows, &mut note, "Share application money pending allotment", SHARE_APPLICATION, &always, 1);
    section(&b, &mut rows, &mut note, "Non-current liabilities", NON_CURRENT_LIABILITIES, &always, 1);
    section(&b, &mut rows, &mut note, "Current liabilities", CURRENT_LIABILITIES, &always, 1);
    if nonzero(diff) {
        rows.push(opening_difference_row(diff));
    }
    let liabilities: Vec<LineItem> =
        [SHAREHOLDERS, SHARE_APPLICATION, NON_CURRENT_LIABILITIES, CURRENT_LIABILITIES].iter().flat_map(|p| items(p)).collect();
    let total_el = add(b.sum(&liabilities), diff);
    rows.push(row(RowKind::Total, 0, "Total", total_el));

    rows.push(row(RowKind::Heading, 0, "ASSETS", Figures::default()));
    section(&b, &mut rows, &mut note, "Non-current assets", NON_CURRENT_ASSETS, &always, 1);
    section(&b, &mut rows, &mut note, "Current assets", CURRENT_ASSETS, &always, 1);
    let assets: Vec<LineItem> = [NON_CURRENT_ASSETS, CURRENT_ASSETS].iter().flat_map(|p| items(p)).collect();
    let total_assets = b.sum(&assets);
    rows.push(row(RowKind::Total, 0, "Total", total_assets));

    let warnings = balance_warnings(&b, diff);
    BalanceSheet {
        division: 1,
        unit: Rounding::Rupees.unit().to_owned(),
        as_at,
        previous_as_at,
        rows,
        total_equity_and_liabilities: total_el,
        total_assets,
        warnings,
    }
}

use IndAsLine as I;

const D2_NON_CURRENT_ASSETS: &[Part<IndAsLine>] = &[
    Part::Item(I::PropertyPlantAndEquipment),
    Part::Item(I::CapitalWorkInProgress),
    Part::Item(I::InvestmentProperty),
    Part::Item(I::Goodwill),
    Part::Item(I::OtherIntangibleAssets),
    Part::Item(I::IntangibleAssetsUnderDevelopment),
    Part::Caption("Financial assets", &[I::NcInvestments, I::NcTradeReceivables, I::NcLoans, I::NcOtherFinancialAssets]),
    Part::Item(I::DeferredTaxAssets),
    Part::Item(I::OtherNonCurrentAssets),
];
const D2_CURRENT_ASSETS: &[Part<IndAsLine>] = &[
    Part::Item(I::Inventories),
    Part::Caption(
        "Financial assets",
        &[I::CInvestments, I::CTradeReceivables, I::CashAndCashEquivalents, I::OtherBankBalances, I::CLoans, I::COtherFinancialAssets],
    ),
    Part::Item(I::CurrentTaxAssets),
    Part::Item(I::OtherCurrentAssets),
];
const D2_EQUITY: &[Part<IndAsLine>] = &[Part::Item(I::EquityShareCapital), Part::Item(I::OtherEquity)];
const D2_NON_CURRENT_LIABILITIES: &[Part<IndAsLine>] = &[
    Part::Caption("Financial liabilities", &[I::NcBorrowings, I::NcLeaseLiabilities, I::NcTradePayables, I::NcOtherFinancialLiabilities]),
    Part::Item(I::NcProvisions),
    Part::Item(I::DeferredTaxLiabilities),
    Part::Item(I::OtherNonCurrentLiabilities),
];
const D2_CURRENT_LIABILITIES: &[Part<IndAsLine>] = &[
    Part::Caption("Financial liabilities", &[I::CBorrowings, I::CLeaseLiabilities, I::CTradePayables, I::COtherFinancialLiabilities]),
    Part::Item(I::OtherCurrentLiabilities),
    Part::Item(I::CProvisions),
    Part::Item(I::CurrentTaxLiabilities),
];

/// The Schedule III Division II (Ind AS) Balance Sheet: assets first, then
/// equity (share capital and other equity) and liabilities, with financial
/// assets and liabilities captioned apart. Built from the same placements
/// as Division I, so its totals equal Division I's.
pub fn ind_as_balance_sheet(
    book: &Book,
    as_at: NaiveDate,
    previous_as_at: Option<NaiveDate>,
    mapping: &Mapping,
) -> BalanceSheet<IndAsLine> {
    let (b, diff) = balance_builder(book, as_at, previous_as_at, mapping, I::Inventories, I::OtherEquity);
    let always = [I::EquityShareCapital, I::OtherEquity];
    let mut rows = Vec::new();
    let mut note = 0;
    rows.push(row(RowKind::Heading, 0, "ASSETS", Figures::default()));
    section(&b, &mut rows, &mut note, "Non-current assets", D2_NON_CURRENT_ASSETS, &always, 1);
    section(&b, &mut rows, &mut note, "Current assets", D2_CURRENT_ASSETS, &always, 1);
    let assets: Vec<IndAsLine> = [D2_NON_CURRENT_ASSETS, D2_CURRENT_ASSETS].iter().flat_map(|p| items(p)).collect();
    let total_assets = b.sum(&assets);
    rows.push(row(RowKind::Total, 0, "Total Assets", total_assets));

    rows.push(row(RowKind::Heading, 0, "EQUITY AND LIABILITIES", Figures::default()));
    section(&b, &mut rows, &mut note, "Equity", D2_EQUITY, &always, 1);
    let liab: Vec<IndAsLine> = [D2_NON_CURRENT_LIABILITIES, D2_CURRENT_LIABILITIES].iter().flat_map(|p| items(p)).collect();
    if liab.iter().any(|i| b.shown(*i, &always)) {
        rows.push(row(RowKind::Heading, 1, "Liabilities", Figures::default()));
        section(&b, &mut rows, &mut note, "Non-current liabilities", D2_NON_CURRENT_LIABILITIES, &always, 2);
        section(&b, &mut rows, &mut note, "Current liabilities", D2_CURRENT_LIABILITIES, &always, 2);
    }
    if nonzero(diff) {
        rows.push(opening_difference_row(diff));
    }
    let total_el = add(add(b.sum(&items(D2_EQUITY)), b.sum(&liab)), diff);
    rows.push(row(RowKind::Total, 0, "Total Equity and Liabilities", total_el));

    let mut warnings = balance_warnings(&b, diff);
    if nonzero(b.figures(I::OtherCurrentLiabilities)) {
        warnings.push(
            "Ind AS: payables that are financial liabilities (expenses payable, capital creditors, interest accrued) \
             belong under other financial liabilities, not other current liabilities; map their groups."
                .into(),
        );
    }
    BalanceSheet {
        division: 2,
        unit: Rounding::Rupees.unit().to_owned(),
        as_at,
        previous_as_at,
        rows,
        total_equity_and_liabilities: total_el,
        total_assets,
        warnings,
    }
}

// ------------------------------------------------- statement of profit and loss

const INCOME: &[LineItem] = &[L::RevenueFromOperations, L::OtherIncome];
const EXPENSES: &[LineItem] = &[
    L::CostOfMaterialsConsumed,
    L::PurchasesOfStockInTrade,
    L::ChangesInInventories,
    L::EmployeeBenefitsExpense,
    L::FinanceCosts,
    L::DepreciationAndAmortisation,
    L::OtherExpenses,
];
const TAX: &[LineItem] = &[L::CurrentTax, L::DeferredTax];

/// Net movement per ledger from vouchers dated within `[from, to]`.
pub(crate) fn movements(book: &Book, from: NaiveDate, to: NaiveDate) -> BTreeMap<LedgerId, Amount> {
    let mut net = BTreeMap::new();
    for v in book.active_vouchers().filter(|v| v.date >= from && v.date <= to) {
        for e in &v.entries {
            *net.entry(e.ledger).or_default() += e.signed();
        }
    }
    net
}

fn income_builder<'a, K: StatementLine>(
    book: &'a Book,
    from: NaiveDate,
    to: NaiveDate,
    previous: Option<(NaiveDate, NaiveDate)>,
    mapping: &Mapping,
    changes: K,
) -> Builder<'a, K> {
    let mut b = Builder::new(book, mapping);
    for (p, period) in [(0, Some((from, to))), (1, previous)] {
        let Some((f, t)) = period else { continue };
        b.ledgers(p, &movements(book, f, t), &[Nature::Income, Nature::Expenses]);
        let core = reports::profit_and_loss(book, f, t);
        b.computed(changes, p, SourceKind::OpeningStock, "Opening stock", core.opening_stock);
        b.computed(changes, p, SourceKind::ClosingStock, "Closing stock", -core.closing_stock);
    }
    b
}

/// Rows from revenue to profit for the period; returns the totals.
fn income_rows<K: StatementLine>(
    b: &Builder<K>,
    rows: &mut Vec<Row<K>>,
    note: &mut u32,
    lines: (&[K], &[K], &[K]),
    division: u8,
) -> [Figures; 5] {
    let (income, expenses, tax) = lines;
    let always = [income[0]];
    for i in income.iter().filter(|i| b.shown(**i, &always)) {
        rows.push(b.line_row(*i, 1, note));
    }
    let total_income = b.sum(income);
    rows.push(row(RowKind::Total, 0, "Total income", total_income));
    rows.push(row(RowKind::Heading, 0, "Expenses", Figures::default()));
    for i in expenses.iter().filter(|i| b.shown(**i, &always)) {
        rows.push(b.line_row(*i, 1, note));
    }
    let total_expenses = b.sum(expenses);
    rows.push(row(RowKind::Total, 0, "Total expenses", total_expenses));
    let profit_before_tax = sub(total_income, total_expenses);
    if division == 2 {
        rows.push(row(RowKind::Total, 0, "Profit/(loss) before exceptional items and tax", profit_before_tax));
    }
    rows.push(row(RowKind::Total, 0, "Profit before tax", profit_before_tax));
    let tax_expense = b.sum(tax);
    if tax.iter().any(|i| b.shown(*i, &always)) {
        rows.push(row(RowKind::Heading, 0, "Tax expense", Figures::default()));
        for i in tax.iter().filter(|i| b.shown(**i, &always)) {
            rows.push(b.line_row(*i, 1, note));
        }
    }
    let profit = sub(profit_before_tax, tax_expense);
    rows.push(row(RowKind::Total, 0, "Profit (Loss) for the period", profit));
    [total_income, total_expenses, profit_before_tax, tax_expense, profit]
}

fn income_warnings<K: StatementLine>(b: &Builder<K>, other_expenses: K, purchases: K, tax: Figures, pbt: Figures) -> Vec<String> {
    let mut warnings = b.warnings();
    if nonzero(b.figures(other_expenses)) {
        warnings.push(
            "Employee benefits expense, finance costs and depreciation are separate Schedule III heads that the \
             default groups do not identify; they are inside other expenses until their groups are mapped."
                .into(),
        );
    }
    if nonzero(b.figures(purchases)) {
        warnings.push(
            "Purchases are shown as purchases of stock-in-trade (a trading business); a manufacturer maps \
             raw-material purchases to cost of materials consumed."
                .into(),
        );
    }
    if !nonzero(tax) && nonzero(pbt) {
        warnings.push("No tax expense is mapped: profit for the period is before tax.".into());
    }
    warnings
}

/// The Schedule III (Division I) Statement of Profit and Loss for
/// `[from, to]`, with `previous` (`(from, to)`) as the comparative period.
pub fn profit_and_loss(
    book: &Book,
    from: NaiveDate,
    to: NaiveDate,
    previous: Option<(NaiveDate, NaiveDate)>,
    mapping: &Mapping,
) -> ProfitAndLoss {
    let b = income_builder(book, from, to, previous, mapping, L::ChangesInInventories);
    let mut rows = Vec::new();
    let mut note = 0;
    let [total_income, total_expenses, profit_before_tax, tax_expense, profit] = income_rows(&b, &mut rows, &mut note, (INCOME, EXPENSES, TAX), 1);
    let warnings = income_warnings(&b, L::OtherExpenses, L::PurchasesOfStockInTrade, tax_expense, profit_before_tax);
    ProfitAndLoss {
        division: 1,
        unit: Rounding::Rupees.unit().to_owned(),
        from,
        to,
        previous_from: previous.map(|p| p.0),
        previous_to: previous.map(|p| p.1),
        rows,
        total_income,
        total_expenses,
        profit_before_tax,
        tax_expense,
        profit_for_the_period: profit,
        other_comprehensive_income: Figures::default(),
        total_comprehensive_income: profit,
        warnings,
    }
}

const D2_INCOME: &[IndAsLine] = &[I::RevenueFromOperations, I::OtherIncome];
const D2_EXPENSES: &[IndAsLine] = &[
    I::CostOfMaterialsConsumed,
    I::PurchasesOfStockInTrade,
    I::ChangesInInventories,
    I::EmployeeBenefitsExpense,
    I::FinanceCosts,
    I::DepreciationAndAmortisation,
    I::OtherExpenses,
];
const D2_TAX: &[IndAsLine] = &[I::CurrentTax, I::DeferredTax];
const D2_OCI: &[IndAsLine] = &[I::OciNotReclassified, I::OciReclassified];

/// The Schedule III Division II (Ind AS) Statement of Profit and Loss.
/// Ledgers of groups mapped to other comprehensive income leave profit for
/// the period and appear in the OCI section (empty unless mapped); total
/// comprehensive income equals Division I's profit for the period.
pub fn ind_as_profit_and_loss(
    book: &Book,
    from: NaiveDate,
    to: NaiveDate,
    previous: Option<(NaiveDate, NaiveDate)>,
    mapping: &Mapping,
) -> ProfitAndLoss<IndAsLine> {
    let b = income_builder(book, from, to, previous, mapping, I::ChangesInInventories);
    let mut rows = Vec::new();
    let mut note = 0;
    let [total_income, total_expenses, profit_before_tax, tax_expense, profit] =
        income_rows(&b, &mut rows, &mut note, (D2_INCOME, D2_EXPENSES, D2_TAX), 2);
    rows.push(row(RowKind::Heading, 0, "Other Comprehensive Income", Figures::default()));
    for (cap, line) in [("A", I::OciNotReclassified), ("B", I::OciReclassified)] {
        let mut r = b.line_row(line, 1, &mut note);
        r.label = format!("{cap} (i) {}", line.label());
        if !nonzero(Figures { current: r.amount, previous: r.previous }) {
            r.note = None;
            note -= 1;
        }
        rows.push(r);
    }
    let oci = b.sum(D2_OCI);
    let total = add(profit, oci);
    rows.push(row(RowKind::Total, 0, "Total Comprehensive Income for the period", total));
    let mut warnings = income_warnings(&b, I::OtherExpenses, I::PurchasesOfStockInTrade, tax_expense, profit_before_tax);
    if nonzero(oci) {
        warnings.push("Other comprehensive income is shown net of its income tax; map the tax on OCI items to the same OCI line.".into());
    }
    ProfitAndLoss {
        division: 2,
        unit: Rounding::Rupees.unit().to_owned(),
        from,
        to,
        previous_from: previous.map(|p| p.0),
        previous_to: previous.map(|p| p.1),
        rows,
        total_income,
        total_expenses,
        profit_before_tax,
        tax_expense,
        profit_for_the_period: profit,
        other_comprehensive_income: oci,
        total_comprehensive_income: total,
        warnings,
    }
}

/// A line set with its statements, so the notes can be built for either
/// division.
pub trait Layout: StatementLine {
    fn balance_sheet(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>, mapping: &Mapping) -> BalanceSheet<Self>;
    fn profit_and_loss(
        book: &Book,
        from: NaiveDate,
        to: NaiveDate,
        previous: Option<(NaiveDate, NaiveDate)>,
        mapping: &Mapping,
    ) -> ProfitAndLoss<Self>;
}

impl Layout for LineItem {
    fn balance_sheet(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>, mapping: &Mapping) -> BalanceSheet<Self> {
        balance_sheet(book, as_at, previous_as_at, mapping)
    }
    fn profit_and_loss(book: &Book, from: NaiveDate, to: NaiveDate, previous: Option<(NaiveDate, NaiveDate)>, mapping: &Mapping) -> ProfitAndLoss<Self> {
        profit_and_loss(book, from, to, previous, mapping)
    }
}

impl Layout for IndAsLine {
    fn balance_sheet(book: &Book, as_at: NaiveDate, previous_as_at: Option<NaiveDate>, mapping: &Mapping) -> BalanceSheet<Self> {
        ind_as_balance_sheet(book, as_at, previous_as_at, mapping)
    }
    fn profit_and_loss(book: &Book, from: NaiveDate, to: NaiveDate, previous: Option<(NaiveDate, NaiveDate)>, mapping: &Mapping) -> ProfitAndLoss<Self> {
        ind_as_profit_and_loss(book, from, to, previous, mapping)
    }
}

/// A statement in the company's chosen division.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Either<A, B> {
    Division1(A),
    Division2(B),
}

pub type AnyBalanceSheet = Either<BalanceSheet<LineItem>, BalanceSheet<IndAsLine>>;
pub type AnyProfitAndLoss = Either<ProfitAndLoss<LineItem>, ProfitAndLoss<IndAsLine>>;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use khata_core::{Entry, StockItem, StockLine, Unit, UnitId, Voucher, VoucherId, VoucherKind};
    use rust_decimal_macros::dec;

    fn now() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_760_000_000, 0).unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn g(name: &str) -> GroupId {
        GroupId::default_named(name)
    }

    struct Fixture {
        book: Book,
        n: u32,
    }

    impl Fixture {
        fn new() -> Self {
            Self { book: Book::new("Schedule Three Pvt Ltd"), n: 0 }
        }

        fn ledger(&mut self, name: &str, group: GroupId, opening: Amount) -> LedgerId {
            let id = self.book.create_ledger(now(), "u", name, group).unwrap();
            if !opening.is_zero() {
                self.book.set_opening_balance(now(), "u", id, opening).unwrap();
            }
            id
        }

        fn post(&mut self, date: NaiveDate, entries: Vec<Entry>) {
            self.n += 1;
            let v = Voucher {
                id: VoucherId::new(),
                kind: VoucherKind::Journal,
                number: self.n.to_string(),
                date,
                entries,
                narration: String::new(),
                cancelled: false,
                series: None,
                stock: Vec::new(),
            };
            self.book.post_voucher(now(), "u", v).unwrap();
        }

        fn pay(&mut self, date: NaiveDate, dr: LedgerId, cr: LedgerId, amount: Amount) {
            self.post(date, vec![Entry::dr(dr, amount), Entry::cr(cr, amount)]);
        }
    }

    /// A company touching every default group, over two financial years.
    fn company() -> (Fixture, BTreeMap<&'static str, LedgerId>) {
        let mut f = Fixture::new();
        let mut l = BTreeMap::new();
        let spec: &[(&str, &str, Amount)] = &[
            ("Equity", "Capital Account", dec!(-500000)),
            ("General Reserve", "Reserves & Surplus", dec!(-100000)),
            ("Term Loan", "Secured Loans", dec!(-300000)),
            ("Director Loan", "Unsecured Loans", dec!(-50000)),
            ("HDFC OD", "Bank OD A/c", dec!(0)),
            ("Branch Pune", "Branch / Divisions", dec!(-10000)),
            ("Salary Payable", "Current Liabilities", dec!(0)),
            ("Output CGST", "Duties & Taxes", dec!(0)),
            ("Audit Fee Provision", "Provisions", dec!(0)),
            ("Acme Supplies", "Sundry Creditors", dec!(0)),
            ("Plant", "Fixed Assets", dec!(600000)),
            ("Mutual Fund", "Investments", dec!(100000)),
            ("SBI", "Bank Accounts", dec!(210000)),
            ("Cash", "Cash-in-Hand", dec!(10000)),
            ("Rent Deposit", "Deposits (Asset)", dec!(30000)),
            ("Staff Advance", "Loans & Advances (Asset)", dec!(5000)),
            ("Preliminary Exp", "Misc. Expenses (Asset)", dec!(5000)),
            ("Beta Retail", "Sundry Debtors", dec!(0)),
            ("Sales", "Sales Accounts", dec!(0)),
            ("Job Work Income", "Direct Incomes", dec!(0)),
            ("Interest Received", "Indirect Incomes", dec!(0)),
            ("Purchases", "Purchase Accounts", dec!(0)),
            ("Freight Inward", "Direct Expenses", dec!(0)),
            ("Salaries", "Indirect Expenses", dec!(0)),
            ("Bank Interest", "Indirect Expenses", dec!(0)),
        ];
        for (name, group, opening) in spec {
            l.insert(*name, f.ledger(name, g(group), *opening));
        }
        // FY 2025-26 (previous year)
        f.pay(d(2025, 5, 1), l["Beta Retail"], l["Sales"], dec!(400000));
        f.pay(d(2025, 6, 1), l["Purchases"], l["Acme Supplies"], dec!(250000));
        f.pay(d(2025, 7, 1), l["Salaries"], l["SBI"], dec!(60000));
        f.pay(d(2025, 8, 1), l["SBI"], l["Interest Received"], dec!(4000));
        // FY 2026-27 (current year)
        f.pay(d(2026, 4, 10), l["Beta Retail"], l["Sales"], dec!(700000));
        f.pay(d(2026, 4, 11), l["Beta Retail"], l["Job Work Income"], dec!(20000));
        f.pay(d(2026, 4, 12), l["Purchases"], l["Acme Supplies"], dec!(380000));
        f.pay(d(2026, 4, 13), l["Freight Inward"], l["Cash"], dec!(6000));
        f.pay(d(2026, 4, 14), l["Salaries"], l["Salary Payable"], dec!(90000));
        f.pay(d(2026, 4, 15), l["Bank Interest"], l["HDFC OD"], dec!(12000));
        f.pay(d(2026, 4, 16), l["SBI"], l["Output CGST"], dec!(18000));
        f.pay(d(2026, 4, 17), l["Freight Inward"], l["Audit Fee Provision"], dec!(15000));
        f.pay(d(2026, 4, 18), l["SBI"], l["Interest Received"], dec!(3000));
        (f, l)
    }

    fn line(rows: &[Row], item: LineItem) -> &Row {
        rows.iter().find(|r| r.item == Some(item)).unwrap_or_else(|| panic!("{item:?} not shown"))
    }

    fn none() -> Mapping {
        Mapping::default()
    }

    #[test]
    fn every_default_group_is_mapped() {
        let book = Book::new("x");
        for grp in book.groups() {
            assert!(
                DEFAULT_MAPPING.iter().any(|(n, _)| GroupId::default_named(n) == grp.id),
                "{} has no default mapping",
                grp.name
            );
        }
        for (name, item) in DEFAULT_MAPPING {
            let grp = book.group(GroupId::default_named(name)).unwrap_or_else(|| panic!("{name} is not a default group"));
            let nature = book.primary_group(grp.id).unwrap().nature;
            assert_eq!(item.nature(), Some(nature), "{name}");
        }
    }

    #[test]
    fn balance_sheet_ties_to_the_core_report_in_both_years() {
        let (f, l) = company();
        let (cur, prev) = (d(2027, 3, 31), d(2026, 3, 31));
        let bs = balance_sheet(&f.book, cur, Some(prev), &none());
        let core = reports::balance_sheet(&f.book, cur);
        let core_prev = reports::balance_sheet(&f.book, prev);
        assert_eq!(bs.total_equity_and_liabilities.current, core.total_liabilities);
        assert_eq!(bs.total_assets.current, core.total_assets);
        assert_eq!(bs.total_equity_and_liabilities.previous, core_prev.total_liabilities);
        assert_eq!(bs.total_assets.previous, core_prev.total_assets);
        assert_eq!(bs.total_equity_and_liabilities, bs.total_assets, "Equity and liabilities = Assets");

        let r = &bs.rows;
        assert_eq!(line(r, L::ShareCapital).amount, dec!(500000));
        // Reserves: 1,00,000 general reserve + last year's profit 94,000
        // brought forward + this year's 2,20,000.
        let reserves = line(r, L::ReservesAndSurplus);
        assert_eq!((reserves.amount, reserves.previous), (dec!(414000), dec!(194000)));
        let bf = reserves.sources.iter().find(|s| s.kind == SourceKind::ProfitBroughtForward).unwrap();
        let year = reserves.sources.iter().find(|s| s.kind == SourceKind::ProfitForTheYear).unwrap();
        assert_eq!((bf.amount, year.amount, year.previous), (dec!(94000), dec!(220000), dec!(94000)));
        assert_eq!(line(r, L::LongTermBorrowings).amount, dec!(350000));
        assert_eq!(line(r, L::ShortTermBorrowings).amount, dec!(12000));
        assert_eq!(line(r, L::TradePayables).amount, dec!(630000));
        assert_eq!(line(r, L::TradePayables).previous, dec!(250000));
        // Branch, salary payable and duties.
        assert_eq!(line(r, L::OtherCurrentLiabilities).amount, dec!(118000));
        assert_eq!(line(r, L::ShortTermProvisions).amount, dec!(15000));
        assert_eq!(line(r, L::PropertyPlantAndEquipment).amount, dec!(600000));
        assert_eq!(line(r, L::NonCurrentInvestments).amount, dec!(100000));
        assert_eq!(line(r, L::TradeReceivables).amount, dec!(1120000));
        assert_eq!(line(r, L::CashAndCashEquivalents).amount, dec!(179000));
        assert_eq!(line(r, L::ShortTermLoansAndAdvances).amount, dec!(35000));
        assert_eq!(line(r, L::OtherCurrentAssets).amount, dec!(5000));

        // Drill-down: the ledgers and the group behind a line.
        let cash = line(r, L::CashAndCashEquivalents);
        assert_eq!(cash.ledgers.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Cash", "SBI"]);
        assert!(cash.sources.iter().all(|s| s.whole_group));
        let ocl = line(r, L::OtherCurrentLiabilities);
        let cl = ocl.sources.iter().find(|s| s.group == Some(g("Current Liabilities"))).unwrap();
        assert!(!cl.whole_group, "Sundry Creditors and Provisions sit elsewhere");
        assert!(ocl.ledgers.iter().any(|x| x.ledger == l["Output CGST"]));

        // Notes are numbered in order; nil lines are left out.
        let notes: Vec<u32> = r.iter().filter_map(|x| x.note).collect();
        assert_eq!(notes, (1..=notes.len() as u32).collect::<Vec<_>>());
        assert!(!r.iter().any(|x| x.item == Some(L::DeferredTaxLiabilities)));
        assert!(bs.warnings.iter().any(|w| w.contains("micro and small")));
        assert!(bs.warnings.iter().any(|w| w.contains("Current maturities")));
    }

    #[test]
    fn statement_of_profit_and_loss_ties_to_net_profit() {
        let (f, _) = company();
        let pl = profit_and_loss(&f.book, d(2026, 4, 1), d(2027, 3, 31), Some((d(2025, 4, 1), d(2026, 3, 31))), &none());
        let core = reports::profit_and_loss(&f.book, d(2026, 4, 1), d(2027, 3, 31));
        let core_prev = reports::profit_and_loss(&f.book, d(2025, 4, 1), d(2026, 3, 31));
        assert_eq!(pl.profit_for_the_period.current, core.net_profit);
        assert_eq!(pl.profit_for_the_period.previous, core_prev.net_profit);
        assert_eq!((pl.profit_for_the_period.current, pl.profit_for_the_period.previous), (dec!(220000), dec!(94000)));
        let r = &pl.rows;
        let rev = line(r, L::RevenueFromOperations);
        assert_eq!((rev.amount, rev.previous), (dec!(720000), dec!(400000)));
        assert_eq!(line(r, L::OtherIncome).amount, dec!(3000));
        assert_eq!(pl.total_income.current, dec!(723000));
        assert_eq!(line(r, L::PurchasesOfStockInTrade).amount, dec!(380000));
        assert_eq!(line(r, L::OtherExpenses).amount, dec!(123000));
        assert_eq!(pl.profit_before_tax, pl.profit_for_the_period, "no tax mapped");
        assert!(!r.iter().any(|x| x.item == Some(L::ChangesInInventories)), "no stock, no line");
        assert!(pl.warnings.iter().any(|w| w.contains("Employee benefits")));
    }

    #[test]
    fn overrides_move_groups_without_changing_the_result() {
        let (mut f, l) = company();
        let staff = f.book.create_group(now(), "u", "Staff Costs", g("Indirect Expenses")).unwrap();
        let finance = f.book.create_group(now(), "u", "Finance Charges", g("Indirect Expenses")).unwrap();
        let tax = f.book.create_group(now(), "u", "Income Tax", g("Indirect Expenses")).unwrap();
        let mut salaries = f.book.ledger(l["Salaries"]).unwrap().clone();
        salaries.group = staff;
        f.book.alter_ledger(now(), "u", salaries).unwrap();
        let mut interest = f.book.ledger(l["Bank Interest"]).unwrap().clone();
        interest.group = finance;
        f.book.alter_ledger(now(), "u", interest).unwrap();
        let it = f.ledger("Provision for Tax", tax, dec!(0));
        f.pay(d(2026, 4, 20), it, l["Audit Fee Provision"], dec!(50000));

        let mut o = BTreeMap::new();
        o.insert(staff, L::EmployeeBenefitsExpense);
        o.insert(finance, L::FinanceCosts);
        o.insert(tax, L::CurrentTax);
        o.insert(g("Secured Loans"), L::ShortTermBorrowings);
        o.insert(g("Fixed Assets"), L::RevenueFromOperations); // wrong side: ignored
        let o = Mapping::of_lines(o);
        let pl = profit_and_loss(&f.book, d(2026, 4, 1), d(2027, 3, 31), None, &o);
        let r = &pl.rows;
        assert_eq!(line(r, L::EmployeeBenefitsExpense).amount, dec!(90000));
        assert_eq!(line(r, L::FinanceCosts).amount, dec!(12000));
        assert_eq!(line(r, L::OtherExpenses).amount, dec!(21000));
        assert_eq!(pl.tax_expense.current, dec!(50000));
        assert_eq!(pl.profit_before_tax.current - dec!(50000), pl.profit_for_the_period.current);
        assert_eq!(pl.profit_for_the_period.current, reports::profit_and_loss(&f.book, d(2026, 4, 1), d(2027, 3, 31)).net_profit);
        let emp = line(r, L::EmployeeBenefitsExpense);
        assert!(emp.sources.iter().all(|s| s.group == Some(staff) && s.whole_group));
        assert!(pl.warnings.iter().any(|w| w.contains("Fixed Assets") && w.contains("ignored")));

        let bs = balance_sheet(&f.book, d(2027, 3, 31), None, &o);
        assert_eq!(line(&bs.rows, L::ShortTermBorrowings).amount, dec!(312000));
        assert_eq!(line(&bs.rows, L::LongTermBorrowings).amount, dec!(50000));
        assert_eq!(bs.total_equity_and_liabilities, bs.total_assets);
        assert_eq!(bs.total_assets.previous, Amount::ZERO, "no comparative date");
    }

    #[test]
    fn renamed_default_groups_still_map() {
        let (mut f, _) = company();
        let mut debtors = f.book.group(g("Sundry Debtors")).unwrap().clone();
        debtors.name = "Customers".into();
        f.book.alter_group(now(), "u", debtors).unwrap();
        let bs = balance_sheet(&f.book, d(2027, 3, 31), None, &none());
        let tr = line(&bs.rows, L::TradeReceivables);
        assert_eq!(tr.amount, dec!(1120000));
        assert_eq!(tr.sources[0].name, "Customers");
    }

    #[test]
    fn stock_feeds_inventories_and_changes_in_inventories() {
        let (mut f, l) = company();
        let unit: Unit = serde_json::from_value(serde_json::json!({ "id": UnitId::new(), "symbol": "Nos" })).unwrap();
        let unit = f.book.save_unit(now(), "u", unit).unwrap();
        let item: StockItem = serde_json::from_value(serde_json::json!({
            "id": khata_core::StockItemId::new(), "name": "Widget", "group": null, "unit": unit,
            "opening_qty": "10", "opening_value": "1000",
        }))
        .unwrap();
        let item = f.book.save_stock_item(now(), "u", item).unwrap();
        // Buy 20 at 100 in the current year: 30 on hand at 100 = 3,000.
        f.n += 1;
        let v = Voucher {
            id: VoucherId::new(),
            kind: VoucherKind::Purchase,
            number: f.n.to_string(),
            date: d(2026, 5, 1),
            entries: vec![
                Entry::dr(l["Purchases"], dec!(2000)).item(StockLine::new(item, dec!(20), dec!(100))),
                Entry::cr(l["Acme Supplies"], dec!(2000)),
            ],
            narration: String::new(),
            cancelled: false,
            series: None,
            stock: Vec::new(),
        };
        f.book.post_voucher(now(), "u", v).unwrap();

        let (from, to) = (d(2026, 4, 1), d(2027, 3, 31));
        let pl = profit_and_loss(&f.book, from, to, Some((d(2025, 4, 1), d(2026, 3, 31))), &none());
        let core = reports::profit_and_loss(&f.book, from, to);
        let change = line(&pl.rows, L::ChangesInInventories);
        assert_eq!(change.amount, core.opening_stock - core.closing_stock);
        assert!(!change.amount.is_zero());
        assert_eq!(pl.profit_for_the_period.current, core.net_profit);
        assert_eq!(pl.profit_for_the_period.previous, reports::profit_and_loss(&f.book, d(2025, 4, 1), d(2026, 3, 31)).net_profit);

        let bs = balance_sheet(&f.book, to, Some(d(2026, 3, 31)), &none());
        let inv = line(&bs.rows, L::Inventories);
        assert_eq!(inv.amount, reports::balance_sheet(&f.book, to).closing_stock);
        assert!(inv.sources.iter().any(|s| s.kind == SourceKind::ClosingStock));
        assert_eq!(bs.total_equity_and_liabilities, bs.total_assets);
        assert_eq!(bs.total_assets.current, reports::balance_sheet(&f.book, to).total_assets);
    }

    #[test]
    fn opening_difference_and_suspense_are_flagged() {
        let mut f = Fixture::new();
        let cash = f.ledger("Cash", g("Cash-in-Hand"), dec!(1000));
        let sus = f.ledger("Suspense", g("Suspense Account"), dec!(0));
        f.pay(d(2026, 5, 1), cash, sus, dec!(200));
        let bs = balance_sheet(&f.book, d(2027, 3, 31), None, &none());
        let diff = bs.rows.iter().find(|r| r.label == "Difference in opening balances").unwrap();
        assert_eq!(diff.amount, dec!(1000));
        assert_eq!(diff.note, None);
        assert_eq!(bs.total_equity_and_liabilities, bs.total_assets);
        assert_eq!(bs.total_assets.current, dec!(1200));
        assert!(bs.warnings.iter().any(|w| w.contains("Suspense")));
        assert!(bs.warnings.iter().any(|w| w.contains("do not tally")));
    }
}

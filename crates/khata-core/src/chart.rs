//! Chart of accounts: groups form a tree, ledgers hang off groups.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GroupId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LedgerId(pub Uuid);

/// Namespace for deterministic default-group ids.
const GROUP_NAMESPACE: Uuid = Uuid::from_u128(0x6b68_6174_612d_6f73_2d67_726f_7570_7331);

impl GroupId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Stable id for a default group, identical in every company and on
    /// every device, so replicas and replays agree on group references.
    pub fn default_named(name: &str) -> Self {
        Self(Uuid::new_v5(&GROUP_NAMESPACE, name.as_bytes()))
    }
}

impl LedgerId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for GroupId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for LedgerId {
    fn default() -> Self {
        Self::new()
    }
}

/// The four primary natures every group ultimately belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Nature {
    Assets,
    Liabilities,
    Income,
    Expenses,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    /// `None` for a primary group.
    pub parent: Option<GroupId>,
    pub nature: Nature,
    /// Alternative names that also find this group in search.
    pub aliases: Vec<String>,
    /// Income/expense groups that feed gross profit (trading account).
    pub affects_gross_profit: bool,
    /// Cash-in-hand and bank groups; Contra vouchers may only touch these.
    pub is_cash_or_bank: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub id: LedgerId,
    pub name: String,
    pub group: GroupId,
    /// Opening balance on the books-beginning date, signed: debit positive.
    #[serde(default)]
    pub opening: crate::Amount,
    /// Default credit period for bills of this party, in days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_days: Option<u32>,
    /// The party's tax registration (GSTIN, TRN, VAT number). Whether it is
    /// valid is a compliance-pack question; the core only stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    /// Why a tax ID that failed the pack's validation was kept anyway. It is
    /// part of the ledger, so the override and its author are in the trail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id_override: Option<String>,
    /// For a duty ledger (under Duties & Taxes): which tax it collects, so
    /// invoices can fill in tax lines automatically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax: Option<LedgerTax>,
    /// Postal details of a party, printed on invoices and needed for
    /// e-invoices and the place of supply of unregistered buyers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailing: Option<Mailing>,
    /// HSN/SAC code or tax category for sales or service ledgers used
    /// without stock items (an accounting invoice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_class: Option<String>,
    /// The party's income-tax number (an Indian PAN). The core only checks
    /// its length and characters; its structure is the pack's business.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<String>,
    /// Tax deducted at source: what a payment through this ledger is for,
    /// or which deductions a duty ledger holds until they are paid over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tds: Option<LedgerTds>,
    /// Tax collected at source: what a sale or receipt through this ledger
    /// is for, the buyer's terms, or which collections a duty ledger holds.
    /// Separate from `tds`, as one party can be both deductee and collectee.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcs: Option<LedgerTcs>,
    /// A party or bank ledger kept in a foreign currency (ISO 4217 code).
    /// Its lines carry foreign amounts; balances are still posted in the
    /// base currency. `None`: the base currency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// The bills the opening balance is made of (invoices and advances from
    /// before the books begin), so outstandings and ageing start from real
    /// bills that later vouchers settle by reference. Whatever part of the
    /// opening balance they don't cover shows as "Opening Balance".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opening_bills: Vec<OpeningBill>,
}

/// Longest reference of an opening bill.
pub const MAX_BILL_REFERENCE: usize = 100;

/// One bill in a party's opening balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningBill {
    /// The bill's name, usually the invoice number; vouchers settle it with
    /// an against-reference allocation of the same name.
    pub reference: String,
    /// Date the bill was raised, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<chrono::NaiveDate>,
    /// Signed like the opening balance: debit positive (receivable).
    pub amount: crate::Amount,
    /// Money received or paid ahead of a bill, rather than a bill.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub advance: bool,
}

/// Longest PAN-like identifier the core stores.
pub const PAN_LEN: usize = 10;
/// Longest nature-of-payment code (the pack defines the codes).
pub const MAX_TDS_NATURE: usize = 16;
/// Longest lower-deduction certificate number.
pub const MAX_TDS_CERTIFICATE: usize = 40;

/// TDS settings on a ledger. The nature codes, rates and thresholds belong
/// to the country's pack; the core only stores them and checks their shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LedgerTds {
    /// An expense or party ledger: amounts paid or credited through it are
    /// subject to TDS. An expense ledger usually carries the nature, a party
    /// ledger the deductee type and any lower-deduction certificate.
    Deductible(TdsTerms),
    /// A duty ledger (under Duties & Taxes) holding TDS deducted for one
    /// nature of payment, or for every nature when `payable_for` is "all".
    Payable { payable_for: String },
}

/// What a deductible ledger tells the TDS calculator.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TdsTerms {
    /// Nature-of-payment code from the pack's list ("contract", "rent_building"…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nature: Option<String>,
    /// Who receives the payment; rates can differ by deductee type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deductee: Option<DeducteeKind>,
    /// Rate (percent) on a lower- or nil-deduction certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower_rate: Option<crate::Amount>,
    /// The certificate's number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    /// First and last day the certificate applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<chrono::NaiveDate>,
    /// For a payee outside the country: the rate the user takes from the
    /// treaty or the law, and the statement details the pack needs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_resident: Option<NonResidentTerms>,
}

/// Longest code the core stores for a country or a nature of remittance
/// (the pack defines the code lists).
pub const MAX_REMITTANCE_CODE: usize = 4;
/// Longest foreign tax identification number.
pub const MAX_FOREIGN_TIN: usize = 25;

/// Withholding on a payment to a non-resident. Rates depend on the treaty
/// and the kind of income, so the rate is the user's, never the software's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonResidentTerms {
    /// Percent to deduct, as the user found it (treaty, certificate or Act).
    pub rate: crate::Amount,
    /// The rate is the treaty's rather than the domestic law's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub treaty: bool,
    /// Country of residence, as the pack's statement code list codes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Nature of the remittance, as the pack's code list codes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remittance: Option<String>,
    /// Tax identification number in the country of residence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_tin: Option<String>,
}

/// TCS settings on a ledger. Codes, rates and thresholds belong to the
/// country's pack; the core only checks their shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LedgerTcs {
    /// A sales or party ledger: amounts received or debited through it are
    /// subject to TCS. A sales ledger usually carries the nature, a party
    /// ledger the collectee's terms.
    Collectible(TcsTerms),
    /// A duty ledger (under Duties & Taxes) holding TCS collected for one
    /// nature, or for every nature when `payable_for` is "all".
    Payable { payable_for: String },
}

/// What a collectible ledger tells the TCS calculator.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcsTerms {
    /// Nature-of-collection code from the pack's list ("scrap"…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nature: Option<String>,
    /// Who pays the tax collected (the buyer, licensee or lessee).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collectee: Option<DeducteeKind>,
    /// Rate (percent) on a lower-collection certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower_rate: Option<crate::Amount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<chrono::NaiveDate>,
    /// The buyer gave the declaration that the goods are for manufacturing
    /// or power, not trading, so nothing is collected where the law allows.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub declaration: bool,
    /// The party is outside the law's definition of a buyer (a government,
    /// an embassy…), so nothing is collected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub excluded: bool,
    /// The collectee is not resident in the country.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub non_resident: bool,
    /// A non-resident collectee has a permanent establishment here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permanent_establishment: bool,
}

/// The kind of person a deduction is made from, as income-tax law groups
/// them (an Indian PAN's fourth character encodes it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeducteeKind {
    Individual,
    Huf,
    Company,
    Firm,
    /// Association of persons.
    Aop,
    /// Body of individuals.
    Boi,
    Trust,
    LocalAuthority,
    /// Artificial juridical person.
    Juridical,
    Government,
}

/// A party's postal and contact details.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mailing {
    /// Street lines, several lines allowed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address: String,
    /// Region code as the country's pack codes it (an Indian state code).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// PIN / postal code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub postal_code: String,
    /// ISO 3166-1 alpha-2, when abroad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// The party is a unit or developer in a special economic or free zone
    /// (an Indian SEZ, a UAE designated zone); supplies to it are zero-rated.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub special_zone: bool,
}

/// Which tax a duty ledger collects and on which side of trade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerTax {
    /// The component as the country's pack names it: "CGST", "SGST",
    /// "UTGST", "IGST", "VAT"…
    pub component: String,
    pub side: TaxSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaxSide {
    /// Tax charged on sales (payable).
    Output,
    /// Tax paid on purchases (claimed as credit).
    Input,
}

/// Default primary groups, familiar to anyone moving from Tally-style
/// accounting. Returned as `(name, nature, affects_gross_profit, is_cash_or_bank, parent_name)`.
pub(crate) const DEFAULT_GROUPS: &[(&str, Nature, bool, bool, Option<&str>)] = &[
    ("Capital Account", Nature::Liabilities, false, false, None),
    ("Reserves & Surplus", Nature::Liabilities, false, false, Some("Capital Account")),
    ("Loans (Liability)", Nature::Liabilities, false, false, None),
    ("Secured Loans", Nature::Liabilities, false, false, Some("Loans (Liability)")),
    ("Unsecured Loans", Nature::Liabilities, false, false, Some("Loans (Liability)")),
    ("Bank OD A/c", Nature::Liabilities, false, false, Some("Loans (Liability)")),
    ("Bank OCC A/c", Nature::Liabilities, false, false, Some("Loans (Liability)")),
    ("Branch / Divisions", Nature::Liabilities, false, false, None),
    ("Current Liabilities", Nature::Liabilities, false, false, None),
    ("Duties & Taxes", Nature::Liabilities, false, false, Some("Current Liabilities")),
    ("Provisions", Nature::Liabilities, false, false, Some("Current Liabilities")),
    ("Sundry Creditors", Nature::Liabilities, false, false, Some("Current Liabilities")),
    ("Suspense Account", Nature::Liabilities, false, false, None),
    ("Fixed Assets", Nature::Assets, false, false, None),
    ("Investments", Nature::Assets, false, false, None),
    ("Current Assets", Nature::Assets, false, false, None),
    ("Bank Accounts", Nature::Assets, false, true, Some("Current Assets")),
    ("Cash-in-Hand", Nature::Assets, false, true, Some("Current Assets")),
    ("Deposits (Asset)", Nature::Assets, false, false, Some("Current Assets")),
    ("Loans & Advances (Asset)", Nature::Assets, false, false, Some("Current Assets")),
    ("Stock-in-Hand", Nature::Assets, false, false, Some("Current Assets")),
    ("Sundry Debtors", Nature::Assets, false, false, Some("Current Assets")),
    ("Misc. Expenses (Asset)", Nature::Assets, false, false, None),
    ("Sales Accounts", Nature::Income, true, false, None),
    ("Direct Incomes", Nature::Income, true, false, None),
    ("Indirect Incomes", Nature::Income, false, false, None),
    ("Purchase Accounts", Nature::Expenses, true, false, None),
    ("Direct Expenses", Nature::Expenses, true, false, None),
    ("Indirect Expenses", Nature::Expenses, false, false, None),
];

/// Aliases users search by, as `(group, alias)`.
pub(crate) const DEFAULT_ALIASES: &[(&str, &str)] = &[
    ("Direct Expenses", "Expenses (Direct)"),
    ("Indirect Expenses", "Expenses (Indirect)"),
    ("Direct Incomes", "Income (Direct)"),
    ("Indirect Incomes", "Income (Indirect)"),
];

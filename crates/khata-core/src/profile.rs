//! Company-wide settings that every copy of the books must agree on.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::chart::{GroupId, LedgerId};
use crate::numbering::NumberSeries;
use crate::voucher::VoucherKind;

/// Entity id of the one profile record in the audit trail ("profile:company").
pub(crate) const PROFILE_ENTITY: &str = "profile:company";

/// Settings that belong to the company rather than to a device. They live in
/// the audit trail, so they sync and every change is logged. The core only
/// stores them: the rules a country switches on live in compliance packs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// ISO 3166-1 alpha-2 code ("IN") of the country whose pack applies.
    pub country: String,
    /// First month (1–12) of the financial year, from the country's pack.
    /// Numbering that restarts every year follows it; January when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fy_start_month: Option<u32>,
    /// How each voucher type is numbered. A type that is not listed counts
    /// up forever with plain numbers ("1", "2", …).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub numbering: BTreeMap<VoucherKind, NumberSeries>,
    /// The company's own tax registration (GSTIN, TRN, VAT number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    /// The region (state, emirate) the company is registered in, as the
    /// pack codes it ("27" for Maharashtra). Decides local vs inter-region tax.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_region: Option<String>,
    /// The company's rate list: tax class (HSN/SAC code or category) →
    /// percentage in force from each date.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tax_rates: BTreeMap<String, BTreeMap<NaiveDate, crate::Amount>>,
    /// Mailing address printed on invoices and reports (several lines).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address: String,
    /// People who may open the company on a shared computer, with their
    /// roles. Empty: no sign-in (a single-user company).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<CompanyUser>,
    /// Digits of HSN/SAC codes the company must show on invoices (India:
    /// 4 up to ₹5 crore aggregate turnover, 6 above). `None`: not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hsn_digits: Option<u8>,
    /// Statutory financial-statement mapping (India: Schedule III): group →
    /// line code. The codes belong to the country's pack, which validates
    /// them; the core stores them as opaque strings and checks their shape.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub schedule3_map: BTreeMap<GroupId, String>,
    /// Parties (ledgers) the pack treats as micro or small enterprises (India:
    /// MSMED Act suppliers, shown separately under trade payables).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub msme_parties: BTreeSet<LedgerId>,
    /// Which statement format applies, as the pack numbers them (India:
    /// Schedule III Division 1 or 2). `None`: the pack's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule3_division: Option<u8>,
    /// Unit the statements are rounded to, as a pack code ("lakhs").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule3_rounding: Option<String>,
    /// The company's own currencies, or changes to built-in ones (symbol,
    /// name, decimals), by ISO 4217 code. See `crate::forex`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub currencies: BTreeMap<String, crate::forex::Currency>,
    /// Rates of exchange the company set: currency code → base units per
    /// unit, from each date. Rates typed on vouchers are remembered there.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fx_rates: BTreeMap<String, BTreeMap<NaiveDate, crate::Amount>>,
}

/// What a user may do. Each role includes everything of the ones below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UserRole {
    /// Reads books and reports; changes nothing.
    Auditor,
    /// Creates vouchers and masters; can't alter, cancel or delete them.
    DataEntry,
    /// Everything with the books, including alter, cancel, delete and
    /// keeping an invalid tax ID with a reason.
    Accountant,
    /// Also company settings: users, numbering, tax registration and rates.
    Owner,
}

/// A user of the company. The password is stored only as an Argon2id hash
/// (PHC string); the app checks it, the core just keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyUser {
    pub name: String,
    pub role: UserRole,
    pub password_hash: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

/// Longest company address accepted (characters).
pub const MAX_ADDRESS: usize = 400;

/// Longest tax class (HSN/SAC code or category name) accepted.
pub const MAX_TAX_CLASS: usize = 24;

/// Longest statement line code (a pack code such as "trade_payables:msme").
pub const MAX_STATEMENT_CODE: usize = 64;

/// Whether `code` has the shape of a pack line or rounding code: 1 to
/// [`MAX_STATEMENT_CODE`] characters of `a-z`, `0-9`, `_`, `:` and `,`.
/// What the codes mean is the pack's business.
pub fn is_statement_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= MAX_STATEMENT_CODE
        && code.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_:,".contains(&b))
}

impl Profile {
    /// Two upper-case ASCII letters; which countries have a pack is not the
    /// core's business.
    pub fn is_valid_country(code: &str) -> bool {
        code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase())
    }
}

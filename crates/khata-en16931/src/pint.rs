//! Peppol PINT (Peppol International invoice model) billing documents, with
//! the Singapore (PINT SG, InvoiceNow) and UAE (PINT AE) specialisations.
//!
//! PINT reuses the EN 16931 semantic model (its terms are "IBT-n", the same
//! numbers as EN 16931's BT-n), but drops the EU VAT rules: each
//! jurisdiction brings its own tax category codes and rules. So a
//! [`PintInvoice`] wraps an EN 16931 [`Invoice`] for the parties, lines,
//! payment and totals, and carries the PINT tax category of each line and
//! allowance/charge itself ([`LineTax`]); [`PintInvoice::finalise`] works
//! out the tax breakdown from those, and [`PintInvoice::write`] writes UBL
//! 2.1. The [`VatCategory`](crate::VatCategory) fields of the wrapped
//! invoice are not used.
//!
//! Sources [PRIMARY unless marked]: docs.peppol.eu, *PINT SG Billing
//! v1.4.1* (25 May 2026; "Singapore specific PINT rules 1.4.0", its shared
//! PINT rules, code lists and example files) and *PINT AE Billing 1.0.4*
//! (PDK 1.4.4, 29 Jul 2026; Schematron `PINT-jurisdiction-aligned-rules.sch`,
//! code lists and examples). See `docs/research/einvoicing-peppol.md`.
//!
//! What is written:
//! - `cbc:CustomizationID` = [`Jurisdiction::customization_id`],
//!   `cbc:ProfileID` = `urn:peppol:bis:billing`; AE adds
//!   `cbc:ProfileExecutionID` (BTAE-02, the 8-flag transaction type).
//! - `cbc:UUID` (SG BT-SG-003, AE BTAE-07).
//! - Both parties' `cbc:EndpointID` (mandatory in PINT, ibr-080/081): SG
//!   `0195` + `SGUEN<UEN>`, AE `0235` + the 10-digit TIN.
//! - Tax scheme `GST` (SG) or `VAT` (AE) on every tax category and party
//!   tax scheme.
//! - One `cbc:Note` (ibr-sr-51): the invoice notes joined by new lines.
//! - AE: the gross price and base quantity on every line (ibr-126), the
//!   item description (ibr-125, the item name when there is none), the line
//!   amount with VAT and the line VAT (`cac:ItemPriceExtension`, BTAE-10 and
//!   BTAE-08, ibr-104/194), the legal registration type and authority
//!   (`@schemeAgencyID` / `@schemeAgencyName`, BTAE-15/12, 16/11), the
//!   credit note reason (`cac:DiscrepancyResponse`, BTAE-03), and for an
//!   export the buyer's address as the deliver-to address (ibr-152).
//!
//! Not modelled: foreign-currency invoices (SG BT-SG-001/002 and the second
//! tax total in SGD; AE BTAE-04 exchange rate and AED totals); the SG
//! payment means Z01/Z02 details; AE reverse-charge supplies (they need
//! the goods type BTAE-09 and a GTIN, ibr-166/174), margin scheme, free
//! zone beneficiary, summary invoices, disclosed agents, e-commerce
//! delivery addresses and item classification (HS / service accounting
//! codes); line allowances and charges; self-billing.
//!
//! [`PintInvoice::validate`] checks the shared and jurisdiction rules that
//! concern what is modelled (see its docs). It is not the Schematron: run
//! the official PINT validation artefacts (XSLT 2.0) or the Access Point's
//! validator before sending.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::model::{Invoice, Party, round2};
use crate::rules::Violation;
use crate::xml::{Xml, amount, number, some};

/// The PINT billing core (the specification jurisdictions specialise).
pub const PINT_BILLING_ID: &str = "urn:peppol:pint:billing-1";
/// BT-24 for PINT SG Billing (PEPPOL-EN16931-R004-SG).
pub const PINT_SG_ID: &str = "urn:peppol:pint:billing-1@sg-1";
/// BT-24 for PINT AE Billing (aligned-ibrp-001-ae).
pub const PINT_AE_ID: &str = "urn:peppol:pint:billing-1@ae-1";
/// BT-23 for PINT billing (PEPPOL-EN16931-R007-SG, aligned-ibrp-002-ae).
pub const PINT_PROFILE_ID: &str = "urn:peppol:bis:billing";
/// EAS code of the Singapore UEN (code list `eas`: "Singapore UEN identifier").
pub const SG_UEN_SCHEME: &str = "0195";
/// EAS code of the UAE TIN ("UAE Tax Identification Number (TIN)").
pub const AE_TIN_SCHEME: &str = "0235";
/// AE reserved endpoint: deemed supplies, reported only (MoF e-invoicing
/// guidelines §10.4, `docs/research/mena-compliance.md` §3).
pub const AE_DEEMED_SUPPLY_ENDPOINT: &str = "9900000097";
/// AE reserved endpoint: the buyer is not yet on the e-invoicing system (a
/// paper or PDF tax invoice must also be issued).
pub const AE_NOT_ON_EINVOICING_ENDPOINT: &str = "9900000098";
/// AE reserved endpoint: an export buyer without a Peppol participant id.
pub const AE_EXPORT_ENDPOINT: &str = "9900000099";
/// The emirate codes PINT AE allows as country subdivision when the
/// country is AE (ibr-128-ae).
pub const AE_EMIRATES: &[&str] = &["AUH", "DXB", "SHJ", "UAQ", "FUJ", "AJM", "RAK"];
/// AE credit note reasons (BTAE-03, code list `CreditReason`).
pub const AE_CREDIT_REASONS: &[(&str, &str)] = &[
    ("DL8.61.1.A", "The supply was cancelled"),
    ("DL8.61.1.B", "The tax treatment changed because the nature of the supply changed"),
    ("DL8.61.1.C", "The agreed consideration was altered (e.g. bad debt relief)"),
    ("DL8.61.1.D", "The goods or services were returned in full or in part"),
    ("DL8.61.1.E", "Tax was charged or a tax treatment applied in error"),
    ("VD", "Volume discount"),
];
/// AE exemption reasons for category E (code list `Aligned-TaxExemptionCodes`).
pub const AE_EXEMPTION_REASONS: &[(&str, &str)] = &[
    ("DL8.46.1", "Certain financial services"),
    ("DL8.46.2", "Supply of residential units (lease or sale)"),
    ("DL8.46.3", "Bare land"),
    ("DL8.46.4", "Local passenger transport"),
];

/// The PINT specialisation an invoice follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Jurisdiction {
    /// Singapore, PINT SG (InvoiceNow), GST.
    Sg,
    /// United Arab Emirates, PINT AE, VAT.
    Ae,
}

impl Jurisdiction {
    /// BT-24.
    pub fn customization_id(self) -> &'static str {
        match self {
            Jurisdiction::Sg => PINT_SG_ID,
            Jurisdiction::Ae => PINT_AE_ID,
        }
    }

    /// BT-23.
    pub fn profile_id(self) -> &'static str {
        PINT_PROFILE_ID
    }

    /// The tax scheme code on tax categories and party tax schemes.
    pub fn tax_scheme(self) -> &'static str {
        match self {
            Jurisdiction::Sg => "GST",
            Jurisdiction::Ae => "VAT",
        }
    }

    /// ISO 3166-1 alpha-2.
    pub fn country(self) -> &'static str {
        match self {
            Jurisdiction::Sg => "SG",
            Jurisdiction::Ae => "AE",
        }
    }

    /// The tax accounting currency (SG BR-113, AE ibr-140).
    pub fn currency(self) -> &'static str {
        match self {
            Jurisdiction::Sg => "SGD",
            Jurisdiction::Ae => "AED",
        }
    }

    /// The EAS scheme of the jurisdiction's participant identifiers.
    pub fn endpoint_scheme(self) -> &'static str {
        match self {
            Jurisdiction::Sg => SG_UEN_SCHEME,
            Jurisdiction::Ae => AE_TIN_SCHEME,
        }
    }

    /// The Peppol document type identifier of an Invoice or CreditNote in
    /// this specification, in the `busdox-docid-qns` form an Access Point
    /// usually asks for [UNCERTAIN: PINT also registers wildcard ids
    /// (`peppol-doctype-wildcard`); check the receiver's SMP entry].
    pub fn document_type_id(self, credit_note: bool) -> String {
        let (ns, root) = if credit_note {
            ("urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2", "CreditNote")
        } else {
            ("urn:oasis:names:specification:ubl:schema:xsd:Invoice-2", "Invoice")
        };
        format!("{ns}::{root}##{}::2.1", self.customization_id())
    }
}

/// The Singapore participant id of a UEN: `0195:SGUEN<UEN>` (IMDA's
/// convention, as in the PINT SG examples' `SGUEN200212345Z`).
pub fn sg_participant_id(uen: &str) -> String {
    format!("{SG_UEN_SCHEME}:{}", sg_endpoint(uen))
}

/// The SG endpoint value of a UEN: `SGUEN` + the UEN in capitals, without
/// spaces or hyphens.
pub fn sg_endpoint(uen: &str) -> String {
    let uen: String = uen.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    let uen = uen.strip_prefix("SGUEN").unwrap_or(&uen).to_owned();
    format!("SGUEN{uen}")
}

/// The UAE participant id of a TIN (the first ten digits of a TRN):
/// `0235:<TIN>`.
pub fn ae_participant_id(tin: &str) -> String {
    format!("{AE_TIN_SCHEME}:{}", tin.trim())
}

/// Split a participant id "scheme:value" (as an SMP lists it, optionally
/// prefixed `iso6523-actorid-upis::`) into an electronic address.
pub fn parse_participant_id(id: &str) -> Option<crate::ElectronicAddress> {
    let id = id.trim();
    let id = id.strip_prefix("iso6523-actorid-upis::").unwrap_or(id);
    let (scheme, value) = id.split_once(':')?;
    let (scheme, value) = (scheme.trim(), value.trim());
    (scheme.len() == 4 && scheme.bytes().all(|b| b.is_ascii_digit()) && !value.is_empty())
        .then(|| crate::ElectronicAddress { value: value.to_owned(), scheme: scheme.to_owned() })
}

/// Singapore GST category codes (PINT SG code list `GST-cat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum SgGst {
    /// SR: standard-rated local supply.
    Sr,
    /// SRCA-S: customer accounting supply made by the supplier.
    SrcaS,
    /// SRCA-C: customer accounting supply made by the customer on the
    /// supplier's behalf.
    SrcaC,
    /// SRRC: reverse charge on imported services (B2B).
    Srrc,
    /// SROVR-RS: remote services under the Overseas Vendor Registration regime.
    SrovrRs,
    /// SROVR-LVG: low-value goods by a redeliverer or electronic marketplace.
    SrovrLvg,
    /// SRLVG: own supply of low-value goods.
    Srlvg,
    /// ZR: zero-rated (exports, international services).
    Zr,
    /// ES33: exempt supplies listed under regulation 33 of the GST
    /// (General) Regulations.
    Es33,
    /// ESN33: other exempt supplies.
    Esn33,
    /// DS: deemed supplies.
    Ds,
    /// OS: out-of-scope supplies.
    Os,
    /// NA: taxable supplies where GST need not be charged (gross margin,
    /// approved schemes).
    Na,
    /// NG: supplies from a company not registered for GST.
    Ng,
}

impl SgGst {
    pub const ALL: [SgGst; 14] = [
        SgGst::Sr,
        SgGst::SrcaS,
        SgGst::SrcaC,
        SgGst::Srrc,
        SgGst::SrovrRs,
        SgGst::SrovrLvg,
        SgGst::Srlvg,
        SgGst::Zr,
        SgGst::Es33,
        SgGst::Esn33,
        SgGst::Ds,
        SgGst::Os,
        SgGst::Na,
        SgGst::Ng,
    ];

    pub fn code(self) -> &'static str {
        match self {
            SgGst::Sr => "SR",
            SgGst::SrcaS => "SRCA-S",
            SgGst::SrcaC => "SRCA-C",
            SgGst::Srrc => "SRRC",
            SgGst::SrovrRs => "SROVR-RS",
            SgGst::SrovrLvg => "SROVR-LVG",
            SgGst::Srlvg => "SRLVG",
            SgGst::Zr => "ZR",
            SgGst::Es33 => "ES33",
            SgGst::Esn33 => "ESN33",
            SgGst::Ds => "DS",
            SgGst::Os => "OS",
            SgGst::Na => "NA",
            SgGst::Ng => "NG",
        }
    }

    /// The code from its text (any case, spaces ignored).
    pub fn parse(code: &str) -> Option<Self> {
        let c: String = code.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
        Self::ALL.into_iter().find(|g| g.code() == c)
    }

    /// The categories that make BR-105 to BR-113 apply (an invoice with one
    /// of them is a GST tax invoice for IRAS).
    pub fn is_gst_invoice_category(self) -> bool {
        matches!(
            self,
            SgGst::Sr
                | SgGst::SrcaS
                | SgGst::SrcaC
                | SgGst::Zr
                | SgGst::Srrc
                | SgGst::SrovrRs
                | SgGst::SrovrLvg
                | SgGst::Srlvg
                | SgGst::Na
        )
    }
}

/// UAE VAT category codes (PINT AE code list `Aligned-TaxCategoryCodes`).
/// "N" (standard rate on an additional base, the margin scheme) is not
/// modelled; note that the code list spells it with a Greek capital Nu
/// (U+039D) [UNCERTAIN: looks like an artefact bug].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum AeVat {
    /// S: standard rate (5%, ibr-190-ae).
    S,
    /// Z: zero rated.
    Z,
    /// E: exempt; needs an exemption reason code (ibr-167-ae).
    E,
    /// O: out of scope / not subject to VAT.
    O,
    /// AE: VAT reverse charge.
    Ae,
}

impl AeVat {
    pub fn code(self) -> &'static str {
        match self {
            AeVat::S => "S",
            AeVat::Z => "Z",
            AeVat::E => "E",
            AeVat::O => "O",
            AeVat::Ae => "AE",
        }
    }
}

/// What a category asks of its rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RateRule {
    /// Greater than zero.
    Positive,
    /// Present and 0.
    Zero,
    /// Not written.
    Absent,
    /// Present (any value).
    Present,
}

/// A PINT tax category of either jurisdiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum TaxCategory {
    Sg(SgGst),
    Ae(AeVat),
}

impl TaxCategory {
    pub fn code(self) -> &'static str {
        match self {
            TaxCategory::Sg(c) => c.code(),
            TaxCategory::Ae(c) => c.code(),
        }
    }

    pub fn jurisdiction(self) -> Jurisdiction {
        match self {
            TaxCategory::Sg(_) => Jurisdiction::Sg,
            TaxCategory::Ae(_) => Jurisdiction::Ae,
        }
    }

    /// The rate a line (or a breakdown) of this category carries. SG: every
    /// category has a rate except NG (BR-48-GST-SG; the guide's Table 3
    /// gives 9% or 0%). AE: aligned-ibrp-*-05 and ibr-119/120/121.
    pub fn rate_rule(self) -> RateRule {
        match self {
            TaxCategory::Sg(SgGst::Ng) => RateRule::Absent,
            TaxCategory::Sg(c) if self.charges_tax() || c == SgGst::Srrc => RateRule::Positive,
            TaxCategory::Sg(_) => RateRule::Zero,
            TaxCategory::Ae(AeVat::S) => RateRule::Positive,
            TaxCategory::Ae(AeVat::Z) => RateRule::Zero,
            TaxCategory::Ae(AeVat::E | AeVat::O) => RateRule::Absent,
            TaxCategory::Ae(AeVat::Ae) => RateRule::Present,
        }
    }

    /// Whether the seller charges the tax on the invoice (tax = taxable ×
    /// rate). SG: the "SR…" categories that carry 9% in Table 3 and DS;
    /// SRCA-S (the customer accounts for it) and the 0% categories do not.
    /// AE: only S (the AE breakdown's tax is 0, aligned-ibrp-ae-09-ae).
    pub fn charges_tax(self) -> bool {
        matches!(
            self,
            TaxCategory::Sg(
                SgGst::Sr | SgGst::SrcaC | SgGst::Srrc | SgGst::SrovrRs | SgGst::SrovrLvg | SgGst::Srlvg | SgGst::Ds
            ) | TaxCategory::Ae(AeVat::S)
        )
    }
}

/// The tax of one line or document allowance/charge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineTax {
    pub category: TaxCategory,
    /// `None` when the category has no rate ([`RateRule::Absent`]).
    pub rate: Option<Decimal>,
    /// Exemption reason code (IBT-121 / IBT-186); AE E needs one.
    pub exemption_code: Option<String>,
    /// Exemption reason text (IBT-120 / IBT-185).
    pub exemption_reason: Option<String>,
}

impl LineTax {
    /// A line's tax: the rate is dropped for categories without one and
    /// forced to 0 for the 0% ones.
    pub fn new(category: TaxCategory, rate: Decimal) -> Self {
        let rate = match category.rate_rule() {
            RateRule::Absent => None,
            RateRule::Zero => Some(Decimal::ZERO),
            RateRule::Positive | RateRule::Present => Some(rate.normalize()),
        };
        LineTax { category, rate, exemption_code: None, exemption_reason: None }
    }

    /// A document allowance or charge's tax. Like [`LineTax::new`], except
    /// that an AE exempt or reverse-charge allowance/charge carries 0%
    /// (aligned-ibrp-e-06/07, ae-06/07), unlike its lines.
    pub fn for_allowance(category: TaxCategory, rate: Decimal) -> Self {
        let mut t = LineTax::new(category, rate);
        if matches!(category, TaxCategory::Ae(AeVat::E | AeVat::Ae)) {
            t.rate = Some(Decimal::ZERO);
        }
        t
    }

    pub fn with_exemption(mut self, code: Option<&str>, reason: Option<&str>) -> Self {
        self.exemption_code = code.map(str::to_owned);
        self.exemption_reason = reason.map(str::to_owned);
        self
    }
}

/// IBG-23: the tax breakdown for one category and rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Breakdown {
    pub category: TaxCategory,
    pub rate: Option<Decimal>,
    /// IBT-116.
    pub taxable: Decimal,
    /// IBT-117.
    pub tax: Decimal,
    pub exemption_code: Option<String>,
    pub exemption_reason: Option<String>,
}

/// AE BTAE-02, the invoice transaction type: eight 0/1 flags (code list
/// `transactiontype`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AeTransactionType {
    pub free_zone: bool,
    pub deemed_supply: bool,
    pub margin_scheme: bool,
    pub summary: bool,
    pub continuous_supply: bool,
    pub disclosed_agent: bool,
    pub ecommerce: bool,
    pub export: bool,
}

impl AeTransactionType {
    /// "00000001" for an export.
    pub fn code(&self) -> String {
        [
            self.free_zone,
            self.deemed_supply,
            self.margin_scheme,
            self.summary,
            self.continuous_supply,
            self.disclosed_agent,
            self.ecommerce,
            self.export,
        ]
        .iter()
        .map(|&b| if b { '1' } else { '0' })
        .collect()
    }
}

/// AE BTAE-15 / BTAE-16: what a legal registration identifier (IBT-030 /
/// IBT-047) is, written as `@schemeAgencyID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LegalIdKind {
    /// TL: commercial / trade licence (needs the issuing authority).
    TradeLicence,
    /// EID: Emirates ID.
    EmiratesId,
    /// PAS: passport (needs the issuing country).
    Passport,
    /// CD: Cabinet decision.
    CabinetDecision,
}

impl LegalIdKind {
    pub fn code(self) -> &'static str {
        match self {
            LegalIdKind::TradeLicence => "TL",
            LegalIdKind::EmiratesId => "EID",
            LegalIdKind::Passport => "PAS",
            LegalIdKind::CabinetDecision => "CD",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Some(match code.trim().to_ascii_uppercase().as_str() {
            "TL" => LegalIdKind::TradeLicence,
            "EID" => LegalIdKind::EmiratesId,
            "PAS" => LegalIdKind::Passport,
            "CD" => LegalIdKind::CabinetDecision,
            _ => return None,
        })
    }
}

/// AE: the type of a party's legal registration identifier, and the
/// authority (licensing authority for TL, issuing country for PAS) as
/// `@schemeAgencyName` (BTAE-12/11, BTAE-18/19).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegalRegistration {
    pub kind: LegalIdKind,
    pub authority: Option<String>,
}

/// The UAE-only terms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AeExtension {
    /// BTAE-02.
    pub transaction: AeTransactionType,
    /// BTAE-03, credit notes: a code of [`AE_CREDIT_REASONS`].
    pub credit_reason: Option<String>,
    /// BTAE-15 / BTAE-12 for the seller's legal id (IBT-030).
    pub seller_legal: Option<LegalRegistration>,
    /// BTAE-16 / BTAE-11 for the buyer's legal id (IBT-047).
    pub buyer_legal: Option<LegalRegistration>,
}

/// A PINT invoice or credit note (type code 381).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PintInvoice {
    pub jurisdiction: Jurisdiction,
    /// The EN 16931 terms. Its `vat_breakdown` and its lines' and
    /// allowances' VAT categories are ignored; `totals` is filled by
    /// [`PintInvoice::finalise`].
    pub invoice: Invoice,
    /// SG BT-SG-003 / AE BTAE-07: an RFC 4122 UUID in its 8-4-4-4-12 form.
    pub uuid: String,
    /// One per `invoice.lines`.
    pub line_tax: Vec<LineTax>,
    /// One per `invoice.allowances_charges`.
    pub allowance_tax: Vec<LineTax>,
    /// IBG-23, from [`PintInvoice::finalise`].
    pub breakdown: Vec<Breakdown>,
    /// UAE terms (ignored for SG).
    pub ae: AeExtension,
}

fn key(t: &LineTax) -> (TaxCategory, Option<Decimal>) {
    (t.category, t.rate.map(|r| r.normalize()))
}

/// The tax of an amount at a category's rate, rounded to two decimals.
fn tax_of(t: &LineTax, taxable: Decimal) -> Decimal {
    match t.rate {
        Some(r) if t.category.charges_tax() => round2(taxable * r / Decimal::ONE_HUNDRED),
        _ => round2(Decimal::ZERO),
    }
}

impl PintInvoice {
    pub fn new(jurisdiction: Jurisdiction, invoice: Invoice, uuid: &str) -> Self {
        PintInvoice {
            jurisdiction,
            invoice,
            uuid: uuid.trim().to_ascii_lowercase(),
            line_tax: Vec::new(),
            allowance_tax: Vec::new(),
            breakdown: Vec::new(),
            ae: AeExtension::default(),
        }
    }

    fn credit(&self) -> bool {
        self.invoice.type_code.is_credit_note()
    }

    /// Work out the breakdown (one per category and rate, in the order they
    /// first appear; tax = taxable × rate ÷ 100 rounded to two decimals for
    /// the categories that charge tax, else 0; the first exemption reason
    /// given on a line or allowance of the group) and the totals (BG-22) as
    /// [`Invoice::finalise`] does. Line net amounts and allowance amounts
    /// are rounded to two decimals first.
    pub fn finalise(&mut self) {
        let inv = &mut self.invoice;
        for l in &mut inv.lines {
            l.net_amount = round2(l.net_amount);
        }
        for a in &mut inv.allowances_charges {
            a.amount = round2(a.amount);
        }
        let mut groups: Vec<(LineTax, Decimal)> = Vec::new();
        let items = inv
            .lines
            .iter()
            .map(|l| l.net_amount)
            .zip(&self.line_tax)
            .chain(inv.allowances_charges.iter().map(|a| a.signed()).zip(&self.allowance_tax));
        for (value, t) in items {
            match groups.iter_mut().find(|(g, _)| key(g) == key(t)) {
                Some((g, sum)) => {
                    *sum += value;
                    if g.exemption_code.is_none() && g.exemption_reason.is_none() {
                        g.exemption_code.clone_from(&t.exemption_code);
                        g.exemption_reason.clone_from(&t.exemption_reason);
                    }
                }
                None => groups.push((t.clone(), value)),
            }
        }
        self.breakdown = groups
            .into_iter()
            .map(|(t, taxable)| {
                let taxable = round2(taxable);
                // A breakdown's rate follows the line rule, so an AE exempt
                // allowance's 0% does not put a rate on an E breakdown.
                let rate = match t.category.rate_rule() {
                    RateRule::Absent => None,
                    _ => t.rate.map(|r| r.normalize()),
                };
                Breakdown {
                    category: t.category,
                    rate,
                    taxable,
                    tax: tax_of(&t, taxable),
                    exemption_code: t.exemption_code,
                    exemption_reason: t.exemption_reason,
                }
            })
            .collect();
        inv.vat_breakdown.clear();
        let t = &mut inv.totals;
        t.line_net = round2(inv.lines.iter().map(|l| l.net_amount).sum());
        t.allowances = round2(inv.allowances_charges.iter().filter(|a| !a.charge).map(|a| a.amount).sum());
        t.charges = round2(inv.allowances_charges.iter().filter(|a| a.charge).map(|a| a.amount).sum());
        t.without_vat = round2(t.line_net - t.allowances + t.charges);
        t.vat = round2(self.breakdown.iter().map(|b| b.tax).sum());
        t.with_vat = round2(t.without_vat + t.vat);
        t.paid = round2(t.paid);
        t.rounding = round2(t.rounding);
        t.due = round2(t.with_vat - t.paid + t.rounding);
    }

    /// The tax of line `i` (AE BTAE-08): its net amount × rate, rounded.
    pub fn line_tax_amount(&self, i: usize) -> Decimal {
        match (self.invoice.lines.get(i), self.line_tax.get(i)) {
            (Some(l), Some(t)) => tax_of(t, round2(l.net_amount)),
            _ => round2(Decimal::ZERO),
        }
    }

    /// Check the invoice against the PINT rules that concern what is
    /// modelled: the shared PINT rules (ibr-*: presence, endpoints, totals,
    /// decimals) and the jurisdiction's (SG: BR-*-SG, PEPPOL-EN16931-R003-SG;
    /// AE: ibr-*-ae, aligned-ibrp-*). Rule ids are the Schematron's; checks
    /// KhataOS adds have `KHATA-PINT-*` ids. Every PINT SG and AE rule is
    /// flagged "fatal" in the artefacts, so only the KhataOS checks can be
    /// warnings.
    pub fn validate(&self) -> Vec<Violation> {
        let mut o = Out(Vec::new());
        self.shared(&mut o);
        match self.jurisdiction {
            Jurisdiction::Sg => self.singapore(&mut o),
            Jurisdiction::Ae => self.uae(&mut o),
        }
        o.0
    }

    fn shared(&self, o: &mut Out) {
        let inv = &self.invoice;
        o.check(!blank(&inv.number), "ibr-002", || "An Invoice MUST have an Invoice number (ibt-001).".into());
        let c = inv.currency.as_bytes();
        o.check(c.len() == 3 && c.iter().all(u8::is_ascii_uppercase), "ibr-cl-04", || {
            format!("The Invoice currency code (ibt-005) {:?} MUST be ISO 4217 alpha-3.", inv.currency)
        });
        o.check(!blank(&inv.seller.name), "ibr-006", || "An Invoice MUST contain the Seller name (ibt-027).".into());
        o.check(!blank(&inv.buyer.name), "ibr-007", || "An Invoice MUST contain the Buyer name (ibt-044).".into());
        o.check(inv.seller.address.country.trim().len() == 2, "ibr-009", || {
            "The Seller postal address (ibg-05) MUST contain a Seller country code (ibt-040).".into()
        });
        o.check(inv.buyer.address.country.trim().len() == 2, "ibr-011", || {
            "The Buyer postal address (ibg-08) MUST contain a Buyer country code (ibt-055).".into()
        });
        for (rule, scheme_rule, who, p) in [("ibr-081", "ibr-062", "Seller", &inv.seller), ("ibr-080", "ibr-063", "Buyer", &inv.buyer)] {
            let e = p.electronic_address.as_ref().filter(|e| !blank(&e.value));
            o.check(e.is_some(), rule, || format!("The {who} electronic address (Peppol participant id) MUST be provided."));
            if let Some(e) = e {
                o.check(!blank(&e.scheme), scheme_rule, || format!("The {who} electronic address MUST have a Scheme identifier."));
            }
        }
        o.check(!inv.lines.is_empty(), "ibr-016", || "An Invoice MUST have at least one Invoice line (ibg-25).".into());
        o.check(self.line_tax.len() == inv.lines.len(), "ibr-sr-58", || {
            "Every Invoice line MUST have an Invoiced item tax category code (ibt-151).".into()
        });
        o.check(self.allowance_tax.len() == inv.allowances_charges.len(), "KHATA-PINT-ALLOWANCE-TAX", || {
            "Every document allowance or charge needs a tax category.".into()
        });
        for t in self.line_tax.iter().chain(&self.allowance_tax) {
            o.check(t.category.jurisdiction() == self.jurisdiction, "KHATA-PINT-CATEGORY", || {
                format!("Tax category {} is not one of this jurisdiction's codes.", t.category.code())
            });
        }
        for (i, l) in inv.lines.iter().enumerate() {
            let n = i + 1;
            o.check(!blank(&l.id), "ibr-021", || format!("Invoice line {n} MUST have an Invoice line identifier (ibt-126)."));
            o.check(!blank(&l.unit_code), "ibr-023", || format!("Invoice line {n} MUST have a unit of measure code (ibt-130)."));
            o.check(!blank(&l.item_name), "ibr-025", || format!("Invoice line {n} MUST contain the Item name (ibt-153)."));
            o.check(l.net_price >= Decimal::ZERO, "ibr-027", || format!("The Item net price (ibt-146) of line {n} MUST NOT be negative."));
        }
        if let Some((start, end)) = inv.period {
            o.check(start.is_some() || end.is_some(), "ibr-co-19", || "An Invoicing period needs a start or an end date.".into());
            if let (Some(s), Some(e)) = (start, end) {
                o.check(e >= s, "ibr-029", || format!("The Invoicing period end date {e} is before its start date {s}."));
            }
        }
        for a in &inv.allowances_charges {
            let reason = given(&a.reason) || given(&a.reason_code);
            let (rule, what) = if a.charge { ("ibr-038", "charge") } else { ("ibr-033", "allowance") };
            o.check(reason, rule, || format!("Each Document level {what} MUST have a reason or a reason code."));
        }
        if let Some(p) = &inv.payment {
            o.check(!blank(&p.means_code), "ibr-049", || "A Payment instruction MUST specify the Payment means type code (ibt-081).".into());
        }
        // Totals (ibr-co-10 to 16), from the lines, allowances and breakdown.
        let t = &inv.totals;
        let line_net: Decimal = inv.lines.iter().map(|l| l.net_amount).sum();
        let allowances: Decimal = inv.allowances_charges.iter().filter(|a| !a.charge).map(|a| a.amount).sum();
        let charges: Decimal = inv.allowances_charges.iter().filter(|a| a.charge).map(|a| a.amount).sum();
        let tax: Decimal = self.breakdown.iter().map(|b| b.tax).sum();
        o.check(t.line_net == line_net, "ibr-co-10", || format!("Sum of Invoice line net amount (ibt-106) {} ≠ Σ line net amounts {line_net}.", t.line_net));
        o.check(t.allowances == allowances, "ibr-co-11", || format!("Sum of allowances (ibt-107) {} ≠ {allowances}.", t.allowances));
        o.check(t.charges == charges, "ibr-co-12", || format!("Sum of charges (ibt-108) {} ≠ {charges}.", t.charges));
        o.check(t.without_vat == line_net - t.allowances + t.charges, "ibr-co-13", || {
            format!("Invoice total without tax (ibt-109) {} ≠ ibt-106 − ibt-107 + ibt-108.", t.without_vat)
        });
        o.check(t.vat == tax, "ibr-co-14", || format!("Invoice total tax (ibt-110) {} ≠ Σ category tax amounts {tax}.", t.vat));
        o.check(t.with_vat == round2(t.without_vat + t.vat), "ibr-co-15", || {
            format!("Invoice total with tax (ibt-112) {} ≠ ibt-109 + ibt-110.", t.with_vat)
        });
        o.check(t.due == t.with_vat - t.paid + t.rounding, "ibr-co-16", || {
            format!("Amount due (ibt-115) {} ≠ ibt-112 − ibt-113 + ibt-114.", t.due)
        });
        for (rule, a) in [
            ("ibr-091", t.due),
            ("ibr-121", t.allowances),
            ("ibr-122", t.charges),
            ("ibr-123", t.without_vat),
            ("ibr-124", t.vat),
            ("ibr-125", t.with_vat),
        ] {
            o.check(decimals_ok(a), rule, || format!("The amount {a} MUST have no more than 2 decimals."));
        }
        o.check(given(&inv.seller.vat_id) || given(&inv.seller.legal_id), "ibr-co-26", || {
            "The Seller identifier, legal registration identifier (ibt-030) or tax identifier (ibt-031) MUST be present.".into()
        });
        // Each line's category and rate has its breakdown; each breakdown's
        // sum and tax hold (aligned-ibrp-*-01, -08 and -09).
        for t in self.line_tax.iter().chain(&self.allowance_tax) {
            o.check(self.breakdown.iter().any(|b| (b.category, b.rate) == key(t) || b.category == t.category && t.category.rate_rule() == RateRule::Absent), "KHATA-PINT-BREAKDOWN", || {
                format!("Tax category {} has no tax breakdown: call finalise.", t.category.code())
            });
        }
        for b in &self.breakdown {
            let lt = LineTax { category: b.category, rate: b.rate, exemption_code: None, exemption_reason: None };
            let sum: Decimal = inv
                .lines
                .iter()
                .zip(&self.line_tax)
                .filter(|(_, t)| t.category == b.category && (b.rate.is_none() || t.rate.map(|r| r.normalize()) == b.rate))
                .map(|(l, _)| l.net_amount)
                .chain(
                    inv.allowances_charges
                        .iter()
                        .zip(&self.allowance_tax)
                        .filter(|(_, t)| t.category == b.category && (b.rate.is_none() || t.rate.map(|r| r.normalize()) == b.rate))
                        .map(|(a, _)| a.signed()),
                )
                .sum();
            o.check(b.taxable == round2(sum), "KHATA-PINT-TAXABLE", || {
                format!("The {} breakdown's taxable amount {} ≠ Σ its lines − allowances + charges {}.", b.category.code(), b.taxable, round2(sum))
            });
            let expected = tax_of(&lt, b.taxable);
            let ok = if b.category.charges_tax() { (b.tax - expected).abs() < Decimal::ONE } else { b.tax.is_zero() };
            let rule = match self.jurisdiction {
                Jurisdiction::Sg => "BR-CO-17-GST-SG",
                Jurisdiction::Ae if b.category.charges_tax() => "aligned-ibrp-s-09",
                Jurisdiction::Ae => "aligned-ibrp-z-09",
            };
            o.check(ok, rule, || format!("The {} breakdown's tax amount {} ≠ {} (taxable × rate).", b.category.code(), b.tax, expected));
            o.check(decimals_ok(b.taxable) && decimals_ok(b.tax), "BR-DEC-19-SG", || "Breakdown amounts have at most 2 decimals.".into());
        }
        o.check(!self.breakdown.is_empty(), "BR-CO-18-GST-SG", || "An Invoice shall have at least one tax breakdown (ibg-23).".into());
    }

    fn rates(&self, o: &mut Out, line_rules: [&'static str; 4], allowance_rules: [&'static str; 4]) {
        let check = |o: &mut Out, t: &LineTax, rules: [&'static str; 4], what: &str| {
            let (ok, rule, want) = match t.category.rate_rule() {
                RateRule::Positive => (t.rate.is_some_and(|r| r > Decimal::ZERO), rules[0], "greater than zero"),
                RateRule::Zero => (t.rate.is_some_and(|r| r.is_zero()), rules[1], "0"),
                RateRule::Absent => (t.rate.is_none(), rules[2], "absent"),
                RateRule::Present => (t.rate.is_some(), rules[3], "present"),
            };
            o.check(ok, rule, || format!("The tax rate of {what} in category {} shall be {want}.", t.category.code()));
        };
        for (i, t) in self.line_tax.iter().enumerate() {
            check(o, t, line_rules, &format!("line {}", i + 1));
        }
        for t in &self.allowance_tax {
            let mut t = t.clone();
            // AE E and AE allowances carry 0% (aligned-ibrp-e-06, ae-06).
            if matches!(t.category, TaxCategory::Ae(AeVat::E | AeVat::Ae)) {
                o.check(t.rate.is_some_and(|r| r.is_zero()), allowance_rules[1], || {
                    format!("A document allowance/charge in category {} shall have a rate of 0.", t.category.code())
                });
                continue;
            }
            t.exemption_code = None;
            check(o, &t, allowance_rules, "a document allowance/charge");
        }
    }

    fn singapore(&self, o: &mut Out) {
        let inv = &self.invoice;
        self.rates(o, ["KHATA-PINT-SG-RATE", "KHATA-PINT-SG-RATE", "BR-NG-02-GST-SG", "KHATA-PINT-SG-RATE"], [
            "KHATA-PINT-SG-RATE",
            "KHATA-PINT-SG-RATE",
            "BR-NG-03-GST-SG",
            "KHATA-PINT-SG-RATE",
        ]);
        for b in &self.breakdown {
            o.check(b.rate.is_some() || b.category == TaxCategory::Sg(SgGst::Ng), "BR-48-GST-SG", || {
                format!("The {} GST breakdown shall have a GST category rate (BT-119-GST).", b.category.code())
            });
        }
        o.check(given(&inv.buyer_reference) || given(&inv.order_reference), "PEPPOL-EN16931-R003-SG", || {
            "A buyer reference (BT-10) or purchase order reference (BT-13) MUST be provided.".into()
        });
        o.check(uuid_ok(&self.uuid), "BR-109-GST-SG", || {
            format!("The Universally unique Invoice identifier (BT-SG-003) {:?} shall be formatted according to the UUID standard.", self.uuid)
        });
        if !self.credit() && inv.totals.due > Decimal::ZERO {
            o.check(inv.due_date.is_some() || given(&inv.payment_terms), "BR-CO-25-SG", || {
                "When the Amount due for payment (IBT-115) is positive, the Payment due date (IBT-009) or the Payment terms (IBT-020) shall be present.".into()
            });
        }
        let gst_invoice = self.breakdown.iter().any(|b| matches!(b.category, TaxCategory::Sg(c) if c.is_gst_invoice_category()));
        if gst_invoice {
            const SET: &str = "an SR, SRCA-S, SRCA-C, ZR, SRRC, SROVR-RS, SROVR-LVG, SRLVG or NA category";
            o.check(given(&inv.seller.vat_id), "BR-105-GST-SG", || {
                format!("An Invoice with {SET} shall contain the Seller GST identifier (BT-31-GST).")
            });
            o.check(!blank(&inv.seller.address.line1) && !blank(&inv.seller.address.postal_code), "BR-106-GST-SG", || {
                format!("An Invoice with {SET} shall contain the Seller address line 1 (IBT-35) and post code (IBT-38).")
            });
            o.check(!blank(&inv.buyer.address.line1) && !blank(&inv.buyer.address.postal_code), "BR-107-GST-SG", || {
                format!("An Invoice with {SET} shall contain the Buyer address line 1 (IBT-50) and post code (IBT-53).")
            });
            o.check(!blank(&self.uuid), "BR-108-GST-SG", || format!("An Invoice with {SET} shall contain a UUID (BT-SG-003)."));
            o.check(!self.credit() || inv.notes.iter().any(|n| !blank(n)), "BR-111-GST-SG", || {
                format!("A CreditNote with {SET} shall contain an Invoice note with the reason for credit (IBT-022).")
            });
            o.check(given(&inv.seller.legal_id), "BR-112-GST-SG", || {
                format!("An Invoice with {SET} shall contain the Seller legal registration identifier (IBT-030), the UEN.")
            });
            o.check(inv.currency.trim() == "SGD", "BR-113-GST-SG", || {
                "An Invoice not in SGD needs the GST in SGD (IBT-006, IBT-111, BT-SG-001/002); KhataOS writes SGD invoices only.".into()
            });
        }
        // NG: a supplier not registered for GST, alone on the invoice.
        let ng = |t: &LineTax| t.category == TaxCategory::Sg(SgGst::Ng);
        if self.line_tax.iter().chain(&self.allowance_tax).any(ng) {
            o.check(self.breakdown.iter().filter(|b| b.category == TaxCategory::Sg(SgGst::Ng)).count() == 1, "BR-NG-01-GST-SG", || {
                "An Invoice with NG lines shall contain exactly one NG GST breakdown.".into()
            });
            o.check(!given(&inv.seller.vat_id) && !given(&inv.buyer.vat_id), "BR-NG-02-GST-SG", || {
                "An Invoice with NG lines shall not contain the Seller or Buyer GST identifier.".into()
            });
            o.check(self.breakdown.iter().all(|b| b.category == TaxCategory::Sg(SgGst::Ng)), "BR-NG-11-GST-SG", || {
                "An Invoice with an NG breakdown shall not contain other GST breakdowns.".into()
            });
            o.check(self.line_tax.iter().all(ng), "BR-NG-12-GST-SG", || "An Invoice with an NG breakdown shall not contain lines of another category.".into());
            o.check(self.allowance_tax.iter().all(ng), "BR-NG-13-GST-SG", || {
                "An Invoice with an NG breakdown shall not contain allowances or charges of another category.".into()
            });
        }
        if let Some(e) = &inv.seller.electronic_address {
            o.warn(e.scheme.trim() != SG_UEN_SCHEME || e.value.trim().starts_with("SGUEN"), "KHATA-PINT-SG-ENDPOINT", || {
                format!("A Singapore 0195 participant id is \"SGUEN\" + the UEN; found {:?}.", e.value)
            });
        }
    }

    fn uae(&self, o: &mut Out) {
        let inv = &self.invoice;
        self.rates(o, ["aligned-ibrp-s-05", "aligned-ibrp-z-05", "aligned-ibrp-e-05", "aligned-ibrp-ae-05-ae"], [
            "aligned-ibrp-s-06",
            "aligned-ibrp-z-06",
            "aligned-ibrp-o-06",
            "aligned-ibrp-ae-06",
        ]);
        for t in self.line_tax.iter().chain(&self.allowance_tax) {
            if t.category == TaxCategory::Ae(AeVat::S) {
                o.check(t.rate.is_some_and(|r| r == Decimal::new(5, 0)), "ibr-190-ae", || {
                    format!("A standard-rated VAT rate must be 5.00; found {}.", t.rate.map(|r| r.normalize().to_string()).unwrap_or_default())
                });
            }
        }
        for b in &self.breakdown {
            let none = matches!(b.category, TaxCategory::Ae(AeVat::E | AeVat::O));
            o.check(none == b.rate.is_none(), "ibr-119-ae", || {
                format!("The {} VAT breakdown's rate (IBT-119) is wrong: only O and E have none.", b.category.code())
            });
            if b.category == TaxCategory::Ae(AeVat::S) {
                o.check(b.exemption_code.is_none() && b.exemption_reason.is_none(), "aligned-ibrp-s-10", || {
                    "A standard-rated VAT breakdown MUST not have a VAT exemption reason.".into()
                });
            }
        }
        for (i, t) in self.line_tax.iter().enumerate() {
            let n = i + 1;
            match t.category {
                TaxCategory::Ae(AeVat::E) => {
                    let code = t.exemption_code.as_deref().map(str::trim).unwrap_or("");
                    o.check(!code.is_empty(), "ibr-167-ae", || {
                        format!("Exempt line {n} MUST have a VAT exemption reason code (IBT-186), one of DL8.46.1 to DL8.46.4.")
                    });
                    if !code.is_empty() {
                        o.check(AE_EXEMPTION_REASONS.iter().any(|(c, _)| *c == code), "KHATA-PINT-AE-EXEMPTION", || {
                            format!("Line {n}: {code:?} is not an exemption code of the PINT AE list (DL8.46.1 to DL8.46.4).")
                        });
                    }
                }
                TaxCategory::Ae(AeVat::Ae) => {
                    o.check(given(&inv.buyer.vat_id), "ibr-103-ae", || {
                        format!("Reverse-charge line {n} needs the Buyer VAT identifier (IBT-048).")
                    });
                    o.check(false, "ibr-166-ae", || {
                        format!("Reverse-charge line {n} needs the type of goods or services (BTAE-09) and a GTIN (ibr-174-ae), which KhataOS does not record yet.")
                    });
                }
                _ => {}
            }
        }
        let only_e_o = self.line_tax.iter().all(|t| matches!(t.category, TaxCategory::Ae(AeVat::E | AeVat::O)));
        o.check(self.line_tax.is_empty() || !only_e_o, "ibr-151-ae", || {
            "A commercial invoice or credit note (380, 381) should not contain only exempt and out-of-scope lines; use the out-of-scope invoice type (480/81), which KhataOS does not write yet.".into()
        });
        o.check(!blank(&self.uuid), "ibr-193-ae", || "The unique identifier number (BTAE-07) must be provided.".into());
        o.warn(uuid_ok(&self.uuid), "KHATA-PINT-UUID", || format!("The UUID {:?} is not in the 8-4-4-4-12 form.", self.uuid));
        o.check(inv.currency.trim() == "AED", "ibr-159-ae", || {
            "An invoice not in AED needs the exchange rate (BTAE-04) and AED totals; KhataOS writes AED invoices only.".into()
        });
        for (who, p, street_rule) in [("Seller", &inv.seller, "ibr-143-ae"), ("Buyer", &inv.buyer, "ibr-144-ae")] {
            let a = &p.address;
            o.check(!blank(&a.line1) && !blank(&a.city) && !blank(&a.subdivision), street_rule, || {
                format!("The {who} postal address must have address line 1, city and country subdivision (the emirate code in the UAE).")
            });
            if a.country.trim().eq_ignore_ascii_case("AE") {
                o.check(AE_EMIRATES.contains(&a.subdivision.trim()), "ibr-128-ae", || {
                    format!("The {who} country subdivision {:?} must be one of AUH, DXB, SHJ, UAQ, FUJ, AJM, RAK.", a.subdivision)
                });
            }
            if a.country.trim().eq_ignore_ascii_case("AE")
                && let Some(trn) = p.vat_id.as_deref().filter(|s| !blank(s))
            {
                o.check(trn_ok(trn), "ibr-132-ae", || format!("The {who} TRN {trn:?} must be 15 digits, starting with 1 and ending with 03."));
            }
        }
        // Seller.
        let seller_0235 = inv.seller.electronic_address.as_ref().is_some_and(|e| e.scheme.trim() == AE_TIN_SCHEME);
        o.check(given(&inv.seller.vat_id), "ibr-134-ae", || "The Seller VAT identifier (IBT-031), the TRN, MUST be there.".into());
        if seller_0235 {
            o.check(given(&inv.seller.legal_id), "ibr-150-ae", || {
                "The Seller legal registration identifier (IBT-030), e.g. the trade licence number, MUST be provided.".into()
            });
            if given(&inv.seller.legal_id) {
                o.check(self.ae.seller_legal.is_some(), "ibr-181-ae", || {
                    "The Seller legal registration identifier type (BTAE-15: TL, EID, PAS or CD) MUST be present.".into()
                });
            }
        }
        if let Some(l) = &self.ae.seller_legal {
            o.check(l.kind != LegalIdKind::TradeLicence || l.authority.as_deref().is_some_and(|a| !blank(a)), "ibr-172-ae", || {
                "The licensing authority name (BTAE-12) MUST be there for a trade licence.".into()
            });
            o.check(l.kind != LegalIdKind::Passport || l.authority.as_deref().is_some_and(|a| !blank(a)), "ibr-012-ae", || {
                "The passport issuing country (BTAE-18) MUST be there for a passport.".into()
            });
        }
        // Buyer.
        let buyer_endpoint = inv.buyer.electronic_address.as_ref().filter(|e| e.scheme.trim() == AE_TIN_SCHEME);
        if let Some(e) = buyer_endpoint {
            let v = e.value.trim();
            o.check(v.starts_with('1') || v.starts_with('9') || given(&inv.buyer.vat_id), "ibr-149-ae", || {
                "The Buyer TRN (IBT-048) MUST be provided for this buyer endpoint.".into()
            });
            let tin = v.len() == 10 && v.starts_with('1') && v.bytes().all(|b| b.is_ascii_digit());
            o.check(self.ae.transaction.export || tin || given(&inv.buyer.vat_id), "ibr-135-ae", || {
                format!("The buyer endpoint {v} is not a TIN: the Buyer VAT identifier (IBT-048) or Buyer identifier (IBT-046) MUST be present.")
            });
            if given(&inv.buyer.legal_id) {
                o.check(self.ae.buyer_legal.is_some(), "ibr-180-ae", || {
                    "The Buyer legal registration identifier type (BTAE-16) MUST be present.".into()
                });
            }
        }
        if let Some(l) = &self.ae.buyer_legal {
            o.check(l.kind != LegalIdKind::TradeLicence || l.authority.as_deref().is_some_and(|a| !blank(a)), "ibr-101-ae", || {
                "The licensing authority name (BTAE-11) MUST be there for the buyer's trade licence.".into()
            });
            o.check(l.kind != LegalIdKind::Passport || l.authority.as_deref().is_some_and(|a| !blank(a)), "ibr-010-ae", || {
                "The passport issuing country (BTAE-19) MUST be there for the buyer's passport.".into()
            });
        }
        // Document.
        if self.credit() {
            let reason = self.ae.credit_reason.as_deref().map(str::trim).unwrap_or("");
            o.check(!reason.is_empty(), "ibr-158-ae", || "A credit note MUST have a credit note reason code (BTAE-03).".into());
            if !reason.is_empty() {
                o.check(AE_CREDIT_REASONS.iter().any(|(c, _)| *c == reason), "ibr-001-ae", || {
                    format!("The credit note reason code {reason:?} is not in the Reasons for credit note list.")
                });
            }
            o.check(inv.preceding.is_some() || reason == "VD", "ibr-055-ae", || {
                "A credit note MUST reference the preceding invoice (IBG-03), except for a volume discount (VD).".into()
            });
            o.check(inv.tax_point_date.is_none(), "ibr-124-ae", || "A credit note MUST not have a VAT point date (IBT-007).".into());
        } else {
            if inv.totals.due > Decimal::ZERO && !self.ae.transaction.deemed_supply {
                o.check(inv.due_date.is_some(), "ibr-127-ae", || {
                    "The Payment due date (IBT-009) MUST be present when the amount due is greater than 0.".into()
                });
            }
            if !self.ae.transaction.deemed_supply {
                o.check(inv.payment.as_ref().is_some_and(|p| !blank(&p.means_code)), "ibr-191-ae", || {
                    "The Payment means type code (IBT-081) must be provided.".into()
                });
            }
        }
        if let Some(p) = &inv.payment
            && p.means_code.trim() == "30"
        {
            o.check(p.credit_transfer.as_ref().is_some_and(|c| !blank(&c.account)), "ibr-192-ae", || {
                "A credit transfer (30) must have the Payment account identifier (IBT-084).".into()
            });
        }
        if let (Some(tp), d) = (inv.tax_point_date, inv.issue_date) {
            o.check(tp <= d, "ibr-141-ae", || format!("The VAT point date {tp} should be before the issue date {d}."));
        }
        if self.ae.transaction.export && !inv.buyer.address.country.trim().eq_ignore_ascii_case("AE") {
            let a = &inv.buyer.address;
            o.check(!blank(&a.line1) && !blank(&a.city) && !blank(&a.subdivision), "ibr-152-ae", || {
                "An export needs the deliver-to address line 1, city and country subdivision (the buyer's address is used).".into()
            });
        }
        for flag in [
            (self.ae.transaction.free_zone, "ibr-007-ae", "free zone (needs the beneficiary id, BTAE-01)"),
            (self.ae.transaction.margin_scheme, "ibr-116-ae", "margin scheme (needs category N)"),
            (self.ae.transaction.summary && inv.period.is_none(), "ibr-138-ae", "summary invoice without an invoicing period"),
            (self.ae.transaction.disclosed_agent, "ibr-137-ae", "disclosed agent billing (needs the principal id, BTAE-14)"),
            (self.ae.transaction.ecommerce, "ibr-142-ae", "e-commerce supply (needs a deliver-to address)"),
        ] {
            o.check(!flag.0, flag.1, || format!("Transaction type {} is not supported by KhataOS yet.", flag.2));
        }
    }

    /// The document in UBL 2.1 (`Invoice`, or `CreditNote` for type 381),
    /// in the UBL 2.1 schema order. Does not validate: check
    /// [`PintInvoice::validate`] first.
    pub fn write(&self) -> String {
        let inv = &self.invoice;
        let j = self.jurisdiction;
        let ae = j == Jurisdiction::Ae;
        let credit = self.credit();
        let (root, ns, line_tag, qty_tag) = if credit {
            ("CreditNote", NS_CREDIT_NOTE, "cac:CreditNoteLine", "cbc:CreditedQuantity")
        } else {
            ("Invoice", NS_INVOICE, "cac:InvoiceLine", "cbc:InvoicedQuantity")
        };
        let cur = inv.currency.trim();
        let mut x = Xml::new();
        x.open(root, &[("xmlns", ns), ("xmlns:cac", NS_CAC), ("xmlns:cbc", NS_CBC)]);
        x.text("cbc:CustomizationID", j.customization_id());
        x.text("cbc:ProfileID", j.profile_id());
        if ae {
            x.text("cbc:ProfileExecutionID", &self.ae.transaction.code());
        }
        x.text("cbc:ID", inv.number.trim());
        x.opt("cbc:UUID", Some(&self.uuid));
        x.text("cbc:IssueDate", &date(inv.issue_date));
        if !credit && let Some(d) = inv.due_date {
            x.text("cbc:DueDate", &date(d));
        }
        if credit {
            if let Some(d) = inv.tax_point_date {
                x.text("cbc:TaxPointDate", &date(d));
            }
            x.text("cbc:CreditNoteTypeCode", inv.type_code.code());
        } else {
            x.text("cbc:InvoiceTypeCode", inv.type_code.code());
        }
        let notes: Vec<&str> = inv.notes.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).collect();
        if !notes.is_empty() {
            x.text("cbc:Note", &notes.join("\n"));
        }
        if !credit && let Some(d) = inv.tax_point_date {
            x.text("cbc:TaxPointDate", &date(d));
        }
        x.text("cbc:DocumentCurrencyCode", cur);
        x.opt("cbc:BuyerReference", inv.buyer_reference.as_deref());
        if let Some((start, end)) = inv.period {
            x.open("cac:InvoicePeriod", &[]);
            if let Some(s) = start {
                x.text("cbc:StartDate", &date(s));
            }
            if let Some(e) = end {
                x.text("cbc:EndDate", &date(e));
            }
            x.close("cac:InvoicePeriod");
        }
        if credit && ae && let Some(r) = self.ae.credit_reason.as_deref().and_then(some) {
            x.open("cac:DiscrepancyResponse", &[]);
            x.text("cbc:ResponseCode", r);
            x.close("cac:DiscrepancyResponse");
        }
        if let Some(o) = inv.order_reference.as_deref().and_then(some) {
            x.open("cac:OrderReference", &[]);
            x.text("cbc:ID", o);
            x.close("cac:OrderReference");
        }
        if let Some(p) = &inv.preceding {
            x.open("cac:BillingReference", &[]);
            x.open("cac:InvoiceDocumentReference", &[]);
            x.text("cbc:ID", p.number.trim());
            if let Some(d) = p.date {
                x.text("cbc:IssueDate", &date(d));
            }
            x.close("cac:InvoiceDocumentReference");
            x.close("cac:BillingReference");
        }
        self.party(&mut x, "cac:AccountingSupplierParty", &inv.seller, self.ae.seller_legal.as_ref());
        self.party(&mut x, "cac:AccountingCustomerParty", &inv.buyer, self.ae.buyer_legal.as_ref());
        let export_address = ae && self.ae.transaction.export && !inv.buyer.address.country.trim().eq_ignore_ascii_case("AE");
        let country = inv.deliver_to_country.as_deref().and_then(some);
        if inv.delivery_date.is_some() || country.is_some() || export_address {
            x.open("cac:Delivery", &[]);
            if let Some(d) = inv.delivery_date {
                x.text("cbc:ActualDeliveryDate", &date(d));
            }
            if export_address {
                let a = &inv.buyer.address;
                x.open("cac:DeliveryLocation", &[]);
                x.open("cac:Address", &[]);
                address_lines(&mut x, a);
                x.open("cac:Country", &[]);
                x.text("cbc:IdentificationCode", country.unwrap_or(a.country.trim()));
                x.close("cac:Country");
                x.close("cac:Address");
                x.close("cac:DeliveryLocation");
            } else if let Some(c) = country {
                x.open("cac:DeliveryLocation", &[]);
                x.open("cac:Address", &[]);
                x.open("cac:Country", &[]);
                x.text("cbc:IdentificationCode", c);
                x.close("cac:Country");
                x.close("cac:Address");
                x.close("cac:DeliveryLocation");
            }
            x.close("cac:Delivery");
        }
        if let Some(p) = &inv.payment {
            x.open("cac:PaymentMeans", &[]);
            match p.means_text.as_deref().and_then(some) {
                Some(t) => x.leaf("cbc:PaymentMeansCode", &[("name", t)], p.means_code.trim()),
                None => x.text("cbc:PaymentMeansCode", p.means_code.trim()),
            }
            if credit && let Some(d) = inv.due_date {
                x.text("cbc:PaymentDueDate", &date(d));
            }
            x.opt("cbc:PaymentID", p.remittance.as_deref());
            if let Some(ct) = &p.credit_transfer {
                x.open("cac:PayeeFinancialAccount", &[]);
                x.text("cbc:ID", ct.account.trim());
                x.opt("cbc:Name", ct.name.as_deref());
                if let Some(bic) = ct.bic.as_deref().and_then(some) {
                    x.open("cac:FinancialInstitutionBranch", &[]);
                    x.text("cbc:ID", bic);
                    x.close("cac:FinancialInstitutionBranch");
                }
                x.close("cac:PayeeFinancialAccount");
            }
            x.close("cac:PaymentMeans");
        }
        if let Some(t) = inv.payment_terms.as_deref().and_then(some) {
            x.open("cac:PaymentTerms", &[]);
            x.text("cbc:Note", t);
            x.close("cac:PaymentTerms");
        }
        for (a, t) in inv.allowances_charges.iter().zip(&self.allowance_tax) {
            x.open("cac:AllowanceCharge", &[]);
            x.text("cbc:ChargeIndicator", if a.charge { "true" } else { "false" });
            x.opt("cbc:AllowanceChargeReasonCode", a.reason_code.as_deref());
            x.opt("cbc:AllowanceChargeReason", a.reason.as_deref());
            if let Some(p) = a.percent {
                x.text("cbc:MultiplierFactorNumeric", &number(p));
            }
            money(&mut x, "cbc:Amount", cur, a.amount);
            if let Some(b) = a.base {
                money(&mut x, "cbc:BaseAmount", cur, b);
            }
            self.tax_category(&mut x, "cac:TaxCategory", t.category, t.rate, None);
            x.close("cac:AllowanceCharge");
        }
        x.open("cac:TaxTotal", &[]);
        money(&mut x, "cbc:TaxAmount", cur, inv.totals.vat);
        for b in &self.breakdown {
            x.open("cac:TaxSubtotal", &[]);
            money(&mut x, "cbc:TaxableAmount", cur, b.taxable);
            money(&mut x, "cbc:TaxAmount", cur, b.tax);
            self.tax_category(&mut x, "cac:TaxCategory", b.category, b.rate, Some((b.exemption_code.as_deref(), b.exemption_reason.as_deref())));
            x.close("cac:TaxSubtotal");
        }
        x.close("cac:TaxTotal");
        let t = &inv.totals;
        x.open("cac:LegalMonetaryTotal", &[]);
        money(&mut x, "cbc:LineExtensionAmount", cur, t.line_net);
        money(&mut x, "cbc:TaxExclusiveAmount", cur, t.without_vat);
        money(&mut x, "cbc:TaxInclusiveAmount", cur, t.with_vat);
        if !t.allowances.is_zero() {
            money(&mut x, "cbc:AllowanceTotalAmount", cur, t.allowances);
        }
        if !t.charges.is_zero() {
            money(&mut x, "cbc:ChargeTotalAmount", cur, t.charges);
        }
        if !t.paid.is_zero() {
            money(&mut x, "cbc:PrepaidAmount", cur, t.paid);
        }
        if !t.rounding.is_zero() {
            money(&mut x, "cbc:PayableRoundingAmount", cur, t.rounding);
        }
        money(&mut x, "cbc:PayableAmount", cur, t.due);
        x.close("cac:LegalMonetaryTotal");
        for (i, (l, lt)) in inv.lines.iter().zip(&self.line_tax).enumerate() {
            x.open(line_tag, &[]);
            x.text("cbc:ID", l.id.trim());
            x.opt("cbc:Note", l.note.as_deref());
            x.leaf(qty_tag, &[("unitCode", l.unit_code.trim())], &number(l.quantity));
            money(&mut x, "cbc:LineExtensionAmount", cur, l.net_amount);
            x.open("cac:Item", &[]);
            let description = l.item_description.as_deref().and_then(some);
            // ibr-125-ae: AE asks for a description on every item.
            x.opt("cbc:Description", if ae { description.or(Some(l.item_name.as_str())) } else { description });
            x.text("cbc:Name", l.item_name.trim());
            if let Some(id) = l.seller_item_id.as_deref().and_then(some) {
                x.open("cac:SellersItemIdentification", &[]);
                x.text("cbc:ID", id);
                x.close("cac:SellersItemIdentification");
            }
            let exemption = (ae && lt.category == TaxCategory::Ae(AeVat::E))
                .then_some((lt.exemption_code.as_deref(), lt.exemption_reason.as_deref()));
            self.tax_category(&mut x, "cac:ClassifiedTaxCategory", lt.category, lt.rate, exemption);
            x.close("cac:Item");
            x.open("cac:Price", &[]);
            x.leaf("cbc:PriceAmount", &[("currencyID", cur)], &number(l.net_price));
            let base = l.price_base_quantity.or(ae.then_some(Decimal::ONE));
            if let Some(b) = base {
                x.leaf("cbc:BaseQuantity", &[("unitCode", l.unit_code.trim())], &number(b));
            }
            if ae {
                // ibr-126-ae: the gross price, with no price discount.
                x.open("cac:AllowanceCharge", &[]);
                x.text("cbc:ChargeIndicator", "false");
                money(&mut x, "cbc:Amount", cur, Decimal::ZERO);
                x.leaf("cbc:BaseAmount", &[("currencyID", cur)], &number(l.net_price));
                x.close("cac:AllowanceCharge");
            }
            x.close("cac:Price");
            if ae {
                let tax = self.line_tax_amount(i);
                x.open("cac:ItemPriceExtension", &[]);
                money(&mut x, "cbc:Amount", cur, round2(l.net_amount) + tax);
                // ibr-163-ae: no line VAT on an exempt line.
                if lt.category != TaxCategory::Ae(AeVat::E) {
                    x.open("cac:TaxTotal", &[]);
                    money(&mut x, "cbc:TaxAmount", cur, tax);
                    x.close("cac:TaxTotal");
                }
                x.close("cac:ItemPriceExtension");
            }
            x.close(line_tag);
        }
        x.close(root);
        x.finish()
    }

    fn tax_category(&self, x: &mut Xml, tag: &str, category: TaxCategory, rate: Option<Decimal>, exemption: Option<(Option<&str>, Option<&str>)>) {
        x.open(tag, &[]);
        x.text("cbc:ID", category.code());
        if let Some(r) = rate {
            x.text("cbc:Percent", &number(r));
        }
        if let Some((code, text)) = exemption {
            x.opt("cbc:TaxExemptionReasonCode", code);
            x.opt("cbc:TaxExemptionReason", text);
        }
        x.open("cac:TaxScheme", &[]);
        x.text("cbc:ID", self.jurisdiction.tax_scheme());
        x.close("cac:TaxScheme");
        x.close(tag);
    }

    fn party(&self, x: &mut Xml, wrapper: &str, p: &Party, legal: Option<&LegalRegistration>) {
        x.open(wrapper, &[]);
        x.open("cac:Party", &[]);
        if let Some(e) = p.electronic_address.as_ref().filter(|e| some(&e.value).is_some()) {
            x.leaf("cbc:EndpointID", &[("schemeID", e.scheme.trim())], e.value.trim());
        }
        if let Some(t) = p.trading_name.as_deref().and_then(some) {
            x.open("cac:PartyName", &[]);
            x.text("cbc:Name", t);
            x.close("cac:PartyName");
        }
        x.open("cac:PostalAddress", &[]);
        address_lines(x, &p.address);
        x.open("cac:Country", &[]);
        x.text("cbc:IdentificationCode", p.address.country.trim());
        x.close("cac:Country");
        x.close("cac:PostalAddress");
        if let Some(id) = p.vat_id.as_deref().and_then(some) {
            x.open("cac:PartyTaxScheme", &[]);
            x.text("cbc:CompanyID", id);
            x.open("cac:TaxScheme", &[]);
            x.text("cbc:ID", self.jurisdiction.tax_scheme());
            x.close("cac:TaxScheme");
            x.close("cac:PartyTaxScheme");
        }
        x.open("cac:PartyLegalEntity", &[]);
        x.text("cbc:RegistrationName", p.name.trim());
        if let Some(id) = p.legal_id.as_deref().and_then(some) {
            match legal.filter(|_| self.jurisdiction == Jurisdiction::Ae) {
                Some(l) => {
                    let mut attrs = vec![("schemeAgencyID", l.kind.code())];
                    if let Some(a) = l.authority.as_deref().and_then(some) {
                        attrs.push(("schemeAgencyName", a));
                    }
                    x.leaf("cbc:CompanyID", &attrs, id);
                }
                None => x.text("cbc:CompanyID", id),
            }
        }
        x.close("cac:PartyLegalEntity");
        if !p.contact.is_empty() {
            x.open("cac:Contact", &[]);
            x.opt("cbc:Name", some(&p.contact.name));
            x.opt("cbc:Telephone", some(&p.contact.phone));
            x.opt("cbc:ElectronicMail", some(&p.contact.email));
            x.close("cac:Contact");
        }
        x.close("cac:Party");
        x.close(wrapper);
    }
}

const NS_INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
const NS_CREDIT_NOTE: &str = "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2";
const NS_CAC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2";
const NS_CBC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2";

fn date(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn money(x: &mut Xml, tag: &str, cur: &str, a: Decimal) {
    x.leaf(tag, &[("currencyID", cur)], &amount(a));
}

fn address_lines(x: &mut Xml, a: &crate::Address) {
    x.opt("cbc:StreetName", some(&a.line1));
    x.opt("cbc:AdditionalStreetName", some(&a.line2));
    x.opt("cbc:CityName", some(&a.city));
    x.opt("cbc:PostalZone", some(&a.postal_code));
    x.opt("cbc:CountrySubentity", some(&a.subdivision));
    if let Some(l3) = some(&a.line3) {
        x.open("cac:AddressLine", &[]);
        x.text("cbc:Line", l3);
        x.close("cac:AddressLine");
    }
}

struct Out(Vec<Violation>);

impl Out {
    fn check(&mut self, ok: bool, rule: &'static str, message: impl FnOnce() -> String) {
        if !ok {
            self.0.push(Violation { rule, message: message(), warning: false });
        }
    }

    fn warn(&mut self, ok: bool, rule: &'static str, message: impl FnOnce() -> String) {
        if !ok {
            self.0.push(Violation { rule, message: message(), warning: true });
        }
    }
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn given(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|s| !blank(s))
}

fn decimals_ok(a: Decimal) -> bool {
    a.normalize().scale() <= 2
}

/// The 8-4-4-4-12 hexadecimal form (BR-109-GST-SG's pattern).
pub fn uuid_ok(s: &str) -> bool {
    let parts: Vec<&str> = s.trim().split('-').collect();
    parts.len() == 5
        && parts.iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// A UAE TRN as ibr-132-ae tests it: 15 digits, starting with 1, ending with 03.
pub fn trn_ok(trn: &str) -> bool {
    let t = trn.trim();
    t.len() == 15 && t.bytes().all(|b| b.is_ascii_digit()) && t.starts_with('1') && t.ends_with("03")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::rules::tests::{d, line};
    use crate::ubl::tests::well_formed;
    use rust_decimal_macros::dec;

    fn rules(v: &[Violation]) -> Vec<&'static str> {
        v.iter().map(|v| v.rule).collect()
    }

    const UUID: &str = "2523149b-cb40-4b32-9004-debca57b80ee";

    fn sg_party(name: &str, uen: &str, gst: Option<&str>, street: &str, postal: &str) -> Party {
        Party {
            name: name.into(),
            legal_id: Some(uen.into()),
            vat_id: gst.map(str::to_owned),
            electronic_address: Some(ElectronicAddress { value: sg_endpoint(uen), scheme: SG_UEN_SCHEME.into() }),
            address: Address { line1: street.into(), city: "Singapore".into(), postal_code: postal.into(), country: "SG".into(), ..Address::default() },
            ..Party::default()
        }
    }

    /// A Singapore tax invoice: one SR 9% line and one ZR line.
    fn sg_sample() -> PintInvoice {
        let mut inv = Invoice::new("INV-001", d(2026, 5, 4), TypeCode::Commercial, "SGD");
        inv.due_date = Some(d(2026, 6, 3));
        inv.buyer_reference = Some("PO-778".into());
        inv.notes = vec!["Thank you".into(), "Goods sold are not returnable".into()];
        inv.seller = sg_party("Lion Trading Pte. Ltd.", "201912345K", Some("M2-1234567-K"), "1 Raffles Place", "048616");
        inv.buyer = sg_party("Merlion Buyer Pte. Ltd.", "200254321Z", None, "10 Anson Road", "079903");
        inv.lines = vec![
            line("1", dec!(2), dec!(50), VatCategory::S, Some(dec!(9))),
            line("2", dec!(1), dec!(30.555), VatCategory::Z, Some(dec!(0))),
        ];
        let mut p = PintInvoice::new(Jurisdiction::Sg, inv, UUID);
        p.line_tax = vec![LineTax::new(TaxCategory::Sg(SgGst::Sr), dec!(9.00)), LineTax::new(TaxCategory::Sg(SgGst::Zr), dec!(5))];
        p.finalise();
        p
    }

    fn ae_party(name: &str, trn: &str, emirate: &str) -> Party {
        Party {
            name: name.into(),
            legal_id: Some("112345678900003".into()),
            vat_id: Some(trn.into()),
            electronic_address: Some(ElectronicAddress { value: trn[..10].into(), scheme: AE_TIN_SCHEME.into() }),
            address: Address {
                line1: "Sheikh Zayed Road".into(),
                city: "Dubai".into(),
                subdivision: emirate.into(),
                country: "AE".into(),
                ..Address::default()
            },
            ..Party::default()
        }
    }

    /// A UAE standard tax invoice at 5% and an exempt line.
    fn ae_sample() -> PintInvoice {
        let mut inv = Invoice::new("AE-1001", d(2027, 1, 12), TypeCode::Commercial, "AED");
        inv.due_date = Some(d(2027, 2, 11));
        inv.seller = ae_party("Falcon Supplies LLC", "100123456700003", "DXB");
        inv.buyer = ae_party("Oasis Retail LLC", "100765432100003", "AUH");
        inv.payment = Some(PaymentInstructions {
            means_code: "30".into(),
            means_text: None,
            remittance: Some("AE-1001".into()),
            credit_transfer: Some(CreditTransfer { account: "AE070331234567890123456".into(), name: None, bic: None }),
        });
        inv.lines = vec![
            line("1", dec!(100), dec!(10), VatCategory::S, Some(dec!(5))),
            line("2", dec!(1), dec!(200), VatCategory::E, Some(dec!(0))),
        ];
        let mut p = PintInvoice::new(Jurisdiction::Ae, inv, "4B6000CA-0128-4BDC-99A6-406F2909247F");
        p.line_tax = vec![
            LineTax::new(TaxCategory::Ae(AeVat::S), dec!(5)),
            LineTax::new(TaxCategory::Ae(AeVat::E), dec!(0)).with_exemption(Some("DL8.46.2"), Some("Residential lease")),
        ];
        p.ae.seller_legal = Some(LegalRegistration { kind: LegalIdKind::TradeLicence, authority: Some("Dubai DET".into()) });
        p.ae.buyer_legal = Some(LegalRegistration { kind: LegalIdKind::TradeLicence, authority: Some("Abu Dhabi DED".into()) });
        p.finalise();
        p
    }

    #[test]
    fn identifiers_and_participants() {
        assert_eq!(Jurisdiction::Sg.customization_id(), "urn:peppol:pint:billing-1@sg-1");
        assert_eq!(Jurisdiction::Ae.customization_id(), "urn:peppol:pint:billing-1@ae-1");
        assert_eq!(Jurisdiction::Ae.profile_id(), "urn:peppol:bis:billing");
        assert!(PINT_SG_ID.starts_with(PINT_BILLING_ID));
        assert_eq!(sg_participant_id(" 2019-12345k "), "0195:SGUEN201912345K");
        assert_eq!(sg_endpoint("SGUEN200212345Z"), "SGUEN200212345Z");
        assert_eq!(ae_participant_id("1001234567"), "0235:1001234567");
        let e = parse_participant_id("iso6523-actorid-upis::0195:SGUEN200212345Z").unwrap();
        assert_eq!((e.scheme.as_str(), e.value.as_str()), ("0195", "SGUEN200212345Z"));
        assert!(parse_participant_id("SGUEN200212345Z").is_none());
        assert!(parse_participant_id("01x5:abc").is_none());
        assert_eq!(
            Jurisdiction::Sg.document_type_id(false),
            "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##urn:peppol:pint:billing-1@sg-1::2.1"
        );
        assert_eq!(AeTransactionType { export: true, ..Default::default() }.code(), "00000001");
        assert_eq!(AeTransactionType { free_zone: true, summary: true, ..Default::default() }.code(), "10010000");
        assert!(uuid_ok(UUID) && !uuid_ok("2523149b-cb40-4b32-9004") && !uuid_ok("2523149b-cb40-4b32-9004-debca57b80eg"));
        assert!(trn_ok("100123456700003") && !trn_ok("300123456700003") && !trn_ok("10012345670003"));
        assert_eq!(SgGst::parse("esn 33"), Some(SgGst::Esn33));
        assert_eq!(SgGst::parse("SROVR-LVG"), Some(SgGst::SrovrLvg));
        assert_eq!(SgGst::parse("S"), None);
        assert_eq!(LegalIdKind::parse("eid"), Some(LegalIdKind::EmiratesId));
    }

    #[test]
    fn categories_and_rates() {
        assert_eq!(LineTax::new(TaxCategory::Sg(SgGst::Ng), dec!(9)).rate, None);
        assert_eq!(LineTax::new(TaxCategory::Sg(SgGst::Es33), dec!(9)).rate, Some(dec!(0)));
        assert_eq!(LineTax::new(TaxCategory::Sg(SgGst::Sr), dec!(9.00)).rate.unwrap().to_string(), "9");
        assert_eq!(LineTax::new(TaxCategory::Ae(AeVat::E), dec!(0)).rate, None);
        assert_eq!(LineTax::for_allowance(TaxCategory::Ae(AeVat::E), dec!(0)).rate, Some(dec!(0)));
        assert!(TaxCategory::Sg(SgGst::Ds).charges_tax() && !TaxCategory::Sg(SgGst::SrcaS).charges_tax());
        assert!(!TaxCategory::Ae(AeVat::Ae).charges_tax());
        assert!(SgGst::Zr.is_gst_invoice_category() && !SgGst::Os.is_gst_invoice_category());
    }

    #[test]
    fn singapore_breakdown_totals_and_rules() {
        let p = sg_sample();
        assert_eq!(p.validate(), [], "{:#?}", p.validate());
        let b: Vec<_> = p.breakdown.iter().map(|b| (b.category.code(), b.rate, b.taxable, b.tax)).collect();
        assert_eq!(b, [("SR", Some(dec!(9)), dec!(100.00), dec!(9.00)), ("ZR", Some(dec!(0)), dec!(30.56), dec!(0.00))]);
        let t = &p.invoice.totals;
        assert_eq!((t.line_net, t.vat, t.with_vat, t.due), (dec!(130.56), dec!(9.00), dec!(139.56), dec!(139.56)));
    }

    #[test]
    fn singapore_rules_catch_what_is_missing() {
        let mut p = sg_sample();
        p.invoice.buyer_reference = None;
        p.invoice.due_date = None;
        p.invoice.seller.vat_id = None;
        p.invoice.seller.legal_id = None;
        p.invoice.buyer.address.postal_code.clear();
        p.invoice.buyer.electronic_address = None;
        p.invoice.currency = "USD".into();
        p.uuid = "not-a-uuid".into();
        let r = rules(&p.validate());
        for rule in [
            "PEPPOL-EN16931-R003-SG",
            "BR-CO-25-SG",
            "BR-105-GST-SG",
            "BR-112-GST-SG",
            "BR-107-GST-SG",
            "BR-109-GST-SG",
            "BR-113-GST-SG",
            "ibr-080",
            "ibr-co-26",
        ] {
            assert!(r.contains(&rule), "{rule} missing from {r:?}");
        }
        // A credit note needs its reason as a note.
        let mut p = sg_sample();
        p.invoice.type_code = TypeCode::CreditNote;
        p.invoice.notes.clear();
        assert!(rules(&p.validate()).contains(&"BR-111-GST-SG"));
    }

    #[test]
    fn singapore_not_registered_stands_alone() {
        let mut p = sg_sample();
        p.invoice.seller.vat_id = None;
        p.line_tax = vec![LineTax::new(TaxCategory::Sg(SgGst::Ng), dec!(0)), LineTax::new(TaxCategory::Sg(SgGst::Ng), dec!(0))];
        p.finalise();
        assert_eq!(p.validate(), [], "{:#?}", p.validate());
        assert_eq!((p.breakdown.len(), p.breakdown[0].rate, p.invoice.totals.vat), (1, None, dec!(0.00)));
        let xml = p.write();
        assert!(xml.contains("<cbc:ID>NG</cbc:ID>\n        <cac:TaxScheme>"), "{xml}");
        p.line_tax[1] = LineTax::new(TaxCategory::Sg(SgGst::Sr), dec!(9));
        p.invoice.buyer.vat_id = Some("M2-7654321-A".into());
        p.finalise();
        let r = rules(&p.validate());
        assert!(r.contains(&"BR-NG-11-GST-SG") && r.contains(&"BR-NG-12-GST-SG") && r.contains(&"BR-NG-02-GST-SG"), "{r:?}");
    }

    #[test]
    fn singapore_golden_snippets() {
        let xml = sg_sample().write();
        well_formed(&xml);
        for snippet in [
            "<Invoice xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\" xmlns:cac=\"urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2\" xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\">\n  <cbc:CustomizationID>urn:peppol:pint:billing-1@sg-1</cbc:CustomizationID>\n  <cbc:ProfileID>urn:peppol:bis:billing</cbc:ProfileID>\n  <cbc:ID>INV-001</cbc:ID>\n  <cbc:UUID>2523149b-cb40-4b32-9004-debca57b80ee</cbc:UUID>\n  <cbc:IssueDate>2026-05-04</cbc:IssueDate>\n  <cbc:DueDate>2026-06-03</cbc:DueDate>\n  <cbc:InvoiceTypeCode>380</cbc:InvoiceTypeCode>\n  <cbc:Note>Thank you\nGoods sold are not returnable</cbc:Note>\n  <cbc:DocumentCurrencyCode>SGD</cbc:DocumentCurrencyCode>\n  <cbc:BuyerReference>PO-778</cbc:BuyerReference>\n",
            "    <cac:Party>\n      <cbc:EndpointID schemeID=\"0195\">SGUEN201912345K</cbc:EndpointID>\n      <cac:PostalAddress>\n        <cbc:StreetName>1 Raffles Place</cbc:StreetName>\n        <cbc:CityName>Singapore</cbc:CityName>\n        <cbc:PostalZone>048616</cbc:PostalZone>\n        <cac:Country>\n          <cbc:IdentificationCode>SG</cbc:IdentificationCode>\n        </cac:Country>\n      </cac:PostalAddress>\n      <cac:PartyTaxScheme>\n        <cbc:CompanyID>M2-1234567-K</cbc:CompanyID>\n        <cac:TaxScheme>\n          <cbc:ID>GST</cbc:ID>\n        </cac:TaxScheme>\n      </cac:PartyTaxScheme>\n      <cac:PartyLegalEntity>\n        <cbc:RegistrationName>Lion Trading Pte. Ltd.</cbc:RegistrationName>\n        <cbc:CompanyID>201912345K</cbc:CompanyID>\n      </cac:PartyLegalEntity>\n",
            "<cbc:EndpointID schemeID=\"0195\">SGUEN200254321Z</cbc:EndpointID>",
            "  <cac:TaxTotal>\n    <cbc:TaxAmount currencyID=\"SGD\">9.00</cbc:TaxAmount>\n    <cac:TaxSubtotal>\n      <cbc:TaxableAmount currencyID=\"SGD\">100.00</cbc:TaxableAmount>\n      <cbc:TaxAmount currencyID=\"SGD\">9.00</cbc:TaxAmount>\n      <cac:TaxCategory>\n        <cbc:ID>SR</cbc:ID>\n        <cbc:Percent>9</cbc:Percent>\n        <cac:TaxScheme>\n          <cbc:ID>GST</cbc:ID>\n        </cac:TaxScheme>\n      </cac:TaxCategory>\n    </cac:TaxSubtotal>\n    <cac:TaxSubtotal>\n      <cbc:TaxableAmount currencyID=\"SGD\">30.56</cbc:TaxableAmount>\n      <cbc:TaxAmount currencyID=\"SGD\">0.00</cbc:TaxAmount>\n      <cac:TaxCategory>\n        <cbc:ID>ZR</cbc:ID>\n        <cbc:Percent>0</cbc:Percent>\n",
            "    <cbc:PayableAmount currencyID=\"SGD\">139.56</cbc:PayableAmount>\n",
            "      <cac:ClassifiedTaxCategory>\n        <cbc:ID>SR</cbc:ID>\n        <cbc:Percent>9</cbc:Percent>\n        <cac:TaxScheme>\n          <cbc:ID>GST</cbc:ID>\n        </cac:TaxScheme>\n      </cac:ClassifiedTaxCategory>\n    </cac:Item>\n    <cac:Price>\n      <cbc:PriceAmount currencyID=\"SGD\">50</cbc:PriceAmount>\n    </cac:Price>\n  </cac:InvoiceLine>\n",
        ] {
            assert!(xml.contains(snippet), "missing:\n{snippet}\nin:\n{xml}");
        }
        assert!(!xml.contains("ProfileExecutionID") && !xml.contains("ItemPriceExtension") && !xml.contains(">VAT<"));
    }

    #[test]
    fn singapore_credit_note() {
        let mut p = sg_sample();
        p.invoice.type_code = TypeCode::CreditNote;
        p.invoice.preceding = Some(PrecedingInvoice { number: "INV-000".into(), date: Some(d(2026, 4, 1)) });
        p.invoice.notes = vec!["Goods returned".into()];
        assert_eq!(p.validate(), []);
        let xml = p.write();
        well_formed(&xml);
        assert!(xml.contains("<CreditNote xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2\""));
        assert!(xml.contains("  <cbc:UUID>2523149b-cb40-4b32-9004-debca57b80ee</cbc:UUID>\n  <cbc:IssueDate>2026-05-04</cbc:IssueDate>\n  <cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>\n  <cbc:Note>Goods returned</cbc:Note>\n"));
        assert!(xml.contains("<cac:BillingReference>\n    <cac:InvoiceDocumentReference>\n      <cbc:ID>INV-000</cbc:ID>"));
        assert!(xml.contains("<cbc:CreditedQuantity unitCode=\"H87\">2</cbc:CreditedQuantity>"));
        assert!(!xml.contains("<cbc:DueDate>") && !xml.contains("DiscrepancyResponse"));
    }

    #[test]
    fn uae_breakdown_and_rules() {
        let p = ae_sample();
        assert_eq!(p.validate(), [], "{:#?}", p.validate());
        let b: Vec<_> = p.breakdown.iter().map(|b| (b.category.code(), b.rate, b.taxable, b.tax)).collect();
        assert_eq!(b, [("S", Some(dec!(5)), dec!(1000.00), dec!(50.00)), ("E", None, dec!(200.00), dec!(0.00))]);
        assert_eq!(p.breakdown[1].exemption_code.as_deref(), Some("DL8.46.2"));
        assert_eq!((p.invoice.totals.vat, p.invoice.totals.due), (dec!(50.00), dec!(1250.00)));
        assert_eq!(p.uuid, "4b6000ca-0128-4bdc-99a6-406f2909247f", "lower-cased");
        assert_eq!((p.line_tax_amount(0), p.line_tax_amount(1)), (dec!(50.00), dec!(0.00)));
    }

    #[test]
    fn uae_rules_catch_what_is_missing() {
        let mut p = ae_sample();
        p.invoice.seller.address.subdivision = "Dubai".into();
        p.invoice.buyer.address.city.clear();
        p.invoice.seller.vat_id = Some("100123456700004".into());
        p.ae.seller_legal = Some(LegalRegistration { kind: LegalIdKind::TradeLicence, authority: None });
        p.ae.buyer_legal = None;
        p.invoice.due_date = None;
        p.invoice.payment = None;
        p.line_tax[0].rate = Some(dec!(9));
        p.line_tax[1].exemption_code = None;
        p.finalise();
        let r = rules(&p.validate());
        for rule in ["ibr-128-ae", "ibr-144-ae", "ibr-132-ae", "ibr-172-ae", "ibr-180-ae", "ibr-127-ae", "ibr-191-ae", "ibr-190-ae", "ibr-167-ae"] {
            assert!(r.contains(&rule), "{rule} missing from {r:?}");
        }
        // Credit notes: reason and preceding invoice.
        let mut p = ae_sample();
        p.invoice.type_code = TypeCode::CreditNote;
        let r = rules(&p.validate());
        assert!(r.contains(&"ibr-158-ae") && r.contains(&"ibr-055-ae"), "{r:?}");
        p.ae.credit_reason = Some("DL8.61.1.D".into());
        p.invoice.preceding = Some(PrecedingInvoice { number: "AE-1000".into(), date: None });
        assert_eq!(p.validate(), []);
        p.ae.credit_reason = Some("XX".into());
        assert!(rules(&p.validate()).contains(&"ibr-001-ae"));
        // Reverse charge is not supported; only E/O lines need type 480.
        let mut p = ae_sample();
        p.line_tax[0] = LineTax::new(TaxCategory::Ae(AeVat::Ae), dec!(5));
        p.finalise();
        assert!(rules(&p.validate()).contains(&"ibr-166-ae"));
        p.line_tax[0] = LineTax::new(TaxCategory::Ae(AeVat::O), dec!(0));
        p.finalise();
        assert!(rules(&p.validate()).contains(&"ibr-151-ae"));
    }

    #[test]
    fn uae_golden_snippets() {
        let mut p = ae_sample();
        p.invoice.lines[0].item_description = None;
        let xml = p.write();
        well_formed(&xml);
        for snippet in [
            "  <cbc:CustomizationID>urn:peppol:pint:billing-1@ae-1</cbc:CustomizationID>\n  <cbc:ProfileID>urn:peppol:bis:billing</cbc:ProfileID>\n  <cbc:ProfileExecutionID>00000000</cbc:ProfileExecutionID>\n  <cbc:ID>AE-1001</cbc:ID>\n  <cbc:UUID>4b6000ca-0128-4bdc-99a6-406f2909247f</cbc:UUID>\n",
            "      <cbc:EndpointID schemeID=\"0235\">1001234567</cbc:EndpointID>\n      <cac:PostalAddress>\n        <cbc:StreetName>Sheikh Zayed Road</cbc:StreetName>\n        <cbc:CityName>Dubai</cbc:CityName>\n        <cbc:CountrySubentity>DXB</cbc:CountrySubentity>\n",
            "      <cac:PartyTaxScheme>\n        <cbc:CompanyID>100123456700003</cbc:CompanyID>\n        <cac:TaxScheme>\n          <cbc:ID>VAT</cbc:ID>\n",
            "        <cbc:CompanyID schemeAgencyID=\"TL\" schemeAgencyName=\"Dubai DET\">112345678900003</cbc:CompanyID>\n",
            "  <cac:PaymentMeans>\n    <cbc:PaymentMeansCode>30</cbc:PaymentMeansCode>\n    <cbc:PaymentID>AE-1001</cbc:PaymentID>\n    <cac:PayeeFinancialAccount>\n      <cbc:ID>AE070331234567890123456</cbc:ID>\n",
            "      <cac:TaxCategory>\n        <cbc:ID>E</cbc:ID>\n        <cbc:TaxExemptionReasonCode>DL8.46.2</cbc:TaxExemptionReasonCode>\n        <cbc:TaxExemptionReason>Residential lease</cbc:TaxExemptionReason>\n        <cac:TaxScheme>\n          <cbc:ID>VAT</cbc:ID>\n",
            "    <cac:Item>\n      <cbc:Description>Artikel 1</cbc:Description>\n      <cbc:Name>Artikel 1</cbc:Name>\n      <cac:ClassifiedTaxCategory>\n        <cbc:ID>S</cbc:ID>\n        <cbc:Percent>5</cbc:Percent>\n",
            "    <cac:Price>\n      <cbc:PriceAmount currencyID=\"AED\">10</cbc:PriceAmount>\n      <cbc:BaseQuantity unitCode=\"H87\">1</cbc:BaseQuantity>\n      <cac:AllowanceCharge>\n        <cbc:ChargeIndicator>false</cbc:ChargeIndicator>\n        <cbc:Amount currencyID=\"AED\">0.00</cbc:Amount>\n        <cbc:BaseAmount currencyID=\"AED\">10</cbc:BaseAmount>\n      </cac:AllowanceCharge>\n    </cac:Price>\n    <cac:ItemPriceExtension>\n      <cbc:Amount currencyID=\"AED\">1050.00</cbc:Amount>\n      <cac:TaxTotal>\n        <cbc:TaxAmount currencyID=\"AED\">50.00</cbc:TaxAmount>\n      </cac:TaxTotal>\n    </cac:ItemPriceExtension>\n  </cac:InvoiceLine>\n",
            "      <cac:ClassifiedTaxCategory>\n        <cbc:ID>E</cbc:ID>\n        <cbc:TaxExemptionReasonCode>DL8.46.2</cbc:TaxExemptionReasonCode>\n",
            "    <cac:ItemPriceExtension>\n      <cbc:Amount currencyID=\"AED\">200.00</cbc:Amount>\n    </cac:ItemPriceExtension>\n",
        ] {
            assert!(xml.contains(snippet), "missing:\n{snippet}\nin:\n{xml}");
        }
        assert!(!xml.contains("<cbc:ID>E</cbc:ID>\n        <cbc:Percent>"), "exempt has no rate");
    }

    #[test]
    fn uae_export_and_credit_note() {
        let mut p = ae_sample();
        p.invoice.buyer = Party {
            name: "Mumbai Imports Pvt Ltd".into(),
            electronic_address: Some(ElectronicAddress { value: AE_EXPORT_ENDPOINT.into(), scheme: AE_TIN_SCHEME.into() }),
            address: Address {
                line1: "1 Marine Drive".into(),
                city: "Mumbai".into(),
                postal_code: "400001".into(),
                subdivision: "MH".into(),
                country: "IN".into(),
                ..Address::default()
            },
            ..Party::default()
        };
        p.ae.buyer_legal = None;
        p.ae.transaction.export = true;
        p.line_tax = vec![LineTax::new(TaxCategory::Ae(AeVat::Z), dec!(0)), LineTax::new(TaxCategory::Ae(AeVat::Z), dec!(0))];
        p.finalise();
        assert_eq!(p.validate(), [], "{:#?}", p.validate());
        let xml = p.write();
        well_formed(&xml);
        assert!(xml.contains("<cbc:ProfileExecutionID>00000001</cbc:ProfileExecutionID>"));
        assert!(xml.contains("<cbc:EndpointID schemeID=\"0235\">9900000099</cbc:EndpointID>"));
        assert!(xml.contains("  <cac:Delivery>\n    <cac:DeliveryLocation>\n      <cac:Address>\n        <cbc:StreetName>1 Marine Drive</cbc:StreetName>\n        <cbc:CityName>Mumbai</cbc:CityName>\n        <cbc:PostalZone>400001</cbc:PostalZone>\n        <cbc:CountrySubentity>MH</cbc:CountrySubentity>\n        <cac:Country>\n          <cbc:IdentificationCode>IN</cbc:IdentificationCode>\n"), "{xml}");
        assert!(xml.contains("<cbc:ID>Z</cbc:ID>\n        <cbc:Percent>0</cbc:Percent>"));

        let mut p = ae_sample();
        p.invoice.type_code = TypeCode::CreditNote;
        p.ae.credit_reason = Some("DL8.61.1.D".into());
        p.invoice.preceding = Some(PrecedingInvoice { number: "AE-1000".into(), date: Some(d(2027, 1, 2)) });
        let xml = p.write();
        well_formed(&xml);
        assert!(xml.contains("  <cbc:DocumentCurrencyCode>AED</cbc:DocumentCurrencyCode>\n  <cac:DiscrepancyResponse>\n    <cbc:ResponseCode>DL8.61.1.D</cbc:ResponseCode>\n  </cac:DiscrepancyResponse>\n  <cac:BillingReference>\n"), "{xml}");
        assert!(xml.contains("<cbc:PaymentDueDate>2027-02-11</cbc:PaymentDueDate>"));
    }

    /// `PINT_SAMPLE_DIR=/some/dir cargo test -p khata-en16931 pint` writes
    /// the sample documents there, for checks with the UBL XSDs or the
    /// official Schematron.
    #[test]
    fn sample_files_on_request() {
        let Some(dir) = std::env::var_os("PINT_SAMPLE_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let mut sg_cn = sg_sample();
        sg_cn.invoice.type_code = TypeCode::CreditNote;
        sg_cn.invoice.notes = vec!["Goods returned".into()];
        let mut ae_cn = ae_sample();
        ae_cn.invoice.type_code = TypeCode::CreditNote;
        ae_cn.ae.credit_reason = Some("DL8.61.1.D".into());
        ae_cn.invoice.preceding = Some(PrecedingInvoice { number: "AE-1000".into(), date: Some(d(2027, 1, 2)) });
        for (name, p) in [("pint-sg-invoice", sg_sample()), ("pint-sg-creditnote", sg_cn), ("pint-ae-invoice", ae_sample()), ("pint-ae-creditnote", ae_cn)] {
            std::fs::write(dir.join(format!("{name}.xml")), p.write()).unwrap();
        }
    }

    #[test]
    fn allowances_carry_their_category() {
        let mut p = sg_sample();
        p.invoice.allowances_charges.push(AllowanceCharge {
            charge: false,
            amount: dec!(10),
            base: None,
            percent: None,
            category: VatCategory::S,
            rate: Some(dec!(9)),
            reason: Some("Discount".into()),
            reason_code: Some("95".into()),
        });
        p.allowance_tax.push(LineTax::for_allowance(TaxCategory::Sg(SgGst::Sr), dec!(9)));
        p.finalise();
        assert_eq!(p.validate(), []);
        assert_eq!((p.breakdown[0].taxable, p.breakdown[0].tax, p.invoice.totals.due), (dec!(90.00), dec!(8.10), dec!(128.66)));
        let xml = p.write();
        assert!(xml.contains("  <cac:AllowanceCharge>\n    <cbc:ChargeIndicator>false</cbc:ChargeIndicator>\n    <cbc:AllowanceChargeReasonCode>95</cbc:AllowanceChargeReasonCode>\n    <cbc:AllowanceChargeReason>Discount</cbc:AllowanceChargeReason>\n    <cbc:Amount currencyID=\"SGD\">10.00</cbc:Amount>\n    <cac:TaxCategory>\n      <cbc:ID>SR</cbc:ID>\n      <cbc:Percent>9</cbc:Percent>\n      <cac:TaxScheme>\n        <cbc:ID>GST</cbc:ID>\n"), "{xml}");
        p.allowance_tax.clear();
        assert!(rules(&p.validate()).contains(&"KHATA-PINT-ALLOWANCE-TAX"));
    }
}

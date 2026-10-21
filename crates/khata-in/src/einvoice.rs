//! e-Invoice JSON (FORM GST INV-01) for upload to an Invoice Registration
//! Portal (IRP), built from a neutral document model.
//!
//! This module only **generates** the JSON a taxpayer uploads (through the
//! portal's offline or bulk tool, or later through a GSP). It does not talk
//! to an IRP, does not produce an IRN or a signed QR code, and does not
//! claim the IRP will accept the result: the IRP's own validations are the
//! only authority. What it checks locally is what the books are most likely
//! to lack (GSTINs, addresses, PIN codes, HSN codes, units, tax split,
//! totals), so the user can fix them before uploading.
//!
//! Sources (`docs/research/india-compliance.md` §4.5, §4.3):
//! - CGST Rule 48(4): notified persons report B2B invoices, credit and debit
//!   notes, exports and SEZ supplies to an IRP [PRIMARY];
//! - threshold: aggregate turnover above ₹5 crore in any FY from 2017-18,
//!   from 1 Aug 2023 (Notif. 10/2023-CT) [SECONDARY]; a ₹2 crore proposal is
//!   not notified [UNCERTAIN];
//! - 30-day reporting limit for AATO of ₹10 crore or more, from 1 Apr 2025
//!   (GSTN advisory, 5 Nov 2024) [PRIMARY];
//! - schema: INV-01 version 1.1 [SECONDARY; a "1.11" is claimed, UNCERTAIN];
//! - HSN: 4 digits up to ₹5 crore AATO, 6 digits above (Notif. 78/2020-CT)
//!   [SECONDARY]. The minimum is a parameter, because the books don't hold
//!   the AATO.
//!
//! **The field list below must be verified against the official INV-01
//! schema** (e-invoice portal developer documentation), which could not be
//! retrieved: field names follow the published schema as commonly
//! reproduced, but types (number vs string), optional fields and length
//! limits are not confirmed. See `docs/einvoice.md`. The whole shape lives
//! in [`to_json`], so a correction is made in one place.

use std::fmt;

use chrono::{Days, NaiveDate};
use khata_compliance::FinancialYear;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::states::{self, Kind};
use crate::{gst, gstin};

/// INV-01 schema version written into `Version`. [SECONDARY; verify]
pub const SCHEMA_VERSION: &str = "1.1";

/// e-Invoicing applies above this aggregate turnover (₹5 crore) in any FY
/// from 2017-18 (Notif. 10/2023-CT, from 1 Aug 2023) [SECONDARY].
pub fn applicability_threshold() -> Decimal {
    Decimal::new(50_000_000, 0)
}

/// Days after the document date within which taxpayers with AATO of
/// ₹10 crore or more must report to the IRP (GSTN advisory, 5 Nov 2024,
/// from 1 Apr 2025) [PRIMARY].
pub const REPORTING_LIMIT_DAYS: u64 = 30;

/// The AATO (₹10 crore) from which the 30-day limit applies.
pub fn reporting_limit_turnover() -> Decimal {
    Decimal::new(100_000_000, 0)
}

/// The last day a document can still be reported to the IRP, when the
/// 30-day limit applies to the taxpayer (`aato` ≥ ₹10 crore); `None` when
/// it does not.
pub fn last_reporting_day(doc_date: NaiveDate, aato: Decimal) -> Option<NaiveDate> {
    if aato < reporting_limit_turnover() {
        return None;
    }
    doc_date.checked_add_days(Days::new(REPORTING_LIMIT_DAYS))
}

/// Buyer GSTIN, state code and PIN written for exports, where the buyer
/// has none. [UNCERTAIN: the "URP" / 96 / 999999 convention is from NIC
/// e-invoice FAQs as recalled, not re-verified; 96 is the GSTR-1 place of
/// supply for exports.]
pub const EXPORT_BUYER_GSTIN: &str = "URP";
pub const EXPORT_STATE: &str = crate::returns::EXPORT_POS;
pub const EXPORT_PIN: &str = "999999";

/// How far a line's tax may be from assessable value × rate, and how far
/// quantity × unit price may be from the line amount (₹1). [UNCERTAIN: the
/// IRP's own tolerances are not confirmed; this is a local sanity check.]
pub fn line_tolerance() -> Decimal {
    Decimal::ONE
}

/// `TranDtls.SupTyp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplyType {
    /// To a registered person.
    B2b,
    /// To an SEZ unit or developer, with IGST.
    SezWithPayment,
    /// To an SEZ unit or developer under LUT or bond, without IGST.
    SezWithoutPayment,
    /// Export with IGST.
    ExportWithPayment,
    /// Export under LUT or bond, without IGST.
    ExportWithoutPayment,
    /// Deemed export.
    DeemedExport,
}

impl SupplyType {
    pub const ALL: [SupplyType; 6] = [
        SupplyType::B2b,
        SupplyType::SezWithPayment,
        SupplyType::SezWithoutPayment,
        SupplyType::ExportWithPayment,
        SupplyType::ExportWithoutPayment,
        SupplyType::DeemedExport,
    ];

    pub fn code(self) -> &'static str {
        match self {
            SupplyType::B2b => "B2B",
            SupplyType::SezWithPayment => "SEZWP",
            SupplyType::SezWithoutPayment => "SEZWOP",
            SupplyType::ExportWithPayment => "EXPWP",
            SupplyType::ExportWithoutPayment => "EXPWOP",
            SupplyType::DeemedExport => "DEXP",
        }
    }

    /// Parse a `SupTyp` code, ignoring case.
    pub fn parse(code: &str) -> Option<SupplyType> {
        Self::ALL.into_iter().find(|t| t.code().eq_ignore_ascii_case(code.trim()))
    }

    pub fn is_export(self) -> bool {
        matches!(self, SupplyType::ExportWithPayment | SupplyType::ExportWithoutPayment)
    }

    /// Exports and SEZ supplies: zero-rated, always inter-State (IGST Act
    /// s.16; s.7(5)).
    pub fn zero_rated(self) -> bool {
        !matches!(self, SupplyType::B2b | SupplyType::DeemedExport)
    }

    /// Under LUT or bond, so no tax is charged.
    pub fn without_payment(self) -> bool {
        matches!(self, SupplyType::SezWithoutPayment | SupplyType::ExportWithoutPayment)
    }
}

/// `DocDtls.Typ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocType {
    Invoice,
    CreditNote,
    DebitNote,
}

impl DocType {
    pub fn code(self) -> &'static str {
        match self {
            DocType::Invoice => "INV",
            DocType::CreditNote => "CRN",
            DocType::DebitNote => "DBN",
        }
    }
}

/// A postal address as the e-invoice wants it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    /// `Addr1`: the first street line.
    pub line1: String,
    /// `Addr2`: the rest of the street lines (optional).
    pub line2: String,
    /// `Loc`: city or town.
    pub location: String,
    /// `Pin`: six-digit PIN code.
    pub pin: Option<String>,
}

/// The last standalone run of exactly six digits in `text` (the PIN code
/// at the end of an Indian address).
pub fn find_pin(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut found = None;
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 6 {
                found = Some(text[start..i].to_owned());
            }
        } else {
            i += 1;
        }
    }
    found
}

/// Remove `pin` and a "PIN"/"PIN code" label from a line, with the
/// separators left dangling ("Pune - 411001" → "Pune").
fn strip_pin(line: &str, pin: Option<&str>) -> String {
    let mut s = line.to_owned();
    if let Some(p) = pin {
        if let Some(at) = s.rfind(p) {
            s.replace_range(at..at + p.len(), "");
        }
    }
    let mut s = s.trim().trim_end_matches([',', '-', ':', '.', ' ']).trim().to_owned();
    for label in ["pin code", "pincode", "pin"] {
        if s.to_ascii_lowercase().ends_with(label) {
            s.truncate(s.len() - label.len());
            s = s.trim().trim_end_matches([',', '-', ':', '.', ' ']).trim().to_owned();
        }
    }
    s
}

/// Split a free-text address (several lines, or one line with commas) into
/// `Addr1`, `Addr2` and `Loc`, with the PIN taken as the last six-digit
/// number. The last line (or the last comma part of a single line) is the
/// location; the first is `Addr1`; anything between is `Addr2`.
pub fn parse_address(text: &str) -> Address {
    let pin = find_pin(text);
    let mut lines: Vec<String> =
        text.lines().map(|l| strip_pin(l, pin.as_deref())).filter(|l| !l.is_empty()).collect();
    if lines.len() == 1 {
        let parts: Vec<String> = lines[0].split(',').map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()).collect();
        if parts.len() > 1 {
            lines = vec![parts[..parts.len() - 1].join(", "), parts[parts.len() - 1].clone()];
        }
    }
    let mut a = Address { pin, ..Address::default() };
    match lines.len() {
        0 => {}
        1 => a.line1 = lines.remove(0),
        n => {
            a.location = lines.pop().unwrap_or_default();
            a.line1 = lines.remove(0);
            if n > 2 {
                a.line2 = lines.join(", ");
            }
        }
    }
    a
}

/// Seller or buyer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Party {
    pub gstin: Option<String>,
    /// `LglNm`: legal name.
    pub legal_name: String,
    pub address: Address,
    /// `Stcd`: the state code of the address.
    pub state_code: Option<String>,
}

/// One `ItemList` entry. Amounts are positive, on credit notes too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub description: String,
    pub hsn: Option<String>,
    pub qty: Option<Decimal>,
    /// UQC ("NOS", "KGS"); `None` when the books' unit has no mapping.
    pub unit: Option<String>,
    pub unit_price: Option<Decimal>,
    /// `TotAmt`: before discount.
    pub gross: Decimal,
    pub discount: Decimal,
    /// `GstRt`: the full GST rate (IGST, or CGST + SGST).
    pub gst_rate: Decimal,
    pub igst: Decimal,
    pub cgst: Decimal,
    /// SGST or UTGST.
    pub sgst: Decimal,
    pub cess_rate: Decimal,
    pub cess: Decimal,
}

impl Item {
    /// `AssAmt`: taxable value.
    pub fn assessable(&self) -> Decimal {
        self.gross - self.discount
    }

    pub fn tax(&self) -> Decimal {
        self.igst + self.cgst + self.sgst
    }

    /// `TotItemVal`.
    pub fn total(&self) -> Decimal {
        self.assessable() + self.tax() + self.cess
    }

    /// `IsServc`: SAC codes start with 99 (chapter 99 of the scheme).
    pub fn is_service(&self) -> bool {
        self.hsn.as_deref().is_some_and(|h| h.starts_with("99"))
    }
}

/// One invoice, credit note or debit note to report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// How errors name the document ("Sales 26-27/1").
    pub label: String,
    pub supply_type: SupplyType,
    pub doc_type: DocType,
    pub number: String,
    pub date: NaiveDate,
    /// `RegRev`: tax payable on reverse charge.
    pub reverse_charge: bool,
    /// `IgstOnIntra`: IGST on an intra-State supply (IGST Act s.7(5)).
    pub igst_on_intra: bool,
    pub seller: Party,
    pub buyer: Party,
    /// `Pos`: state code; ignored for exports (always 96).
    pub place_of_supply: Option<String>,
    pub items: Vec<Item>,
    /// `RndOffAmt`: signed.
    pub round_off: Decimal,
    /// The invoice total in the books; `TotInvVal` must match it.
    pub total: Decimal,
}

/// What stops one document from being reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocErrors {
    pub document: String,
    pub problems: Vec<String>,
}

impl fmt::Display for DocErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.document, self.problems.join("; "))
    }
}

impl std::error::Error for DocErrors {}

fn place_code(code: &str) -> bool {
    states::state(code).is_some_and(|s| s.kind != Kind::CentreJurisdiction && !s.legacy)
}

fn valid_pin(pin: &str) -> bool {
    pin.len() == 6 && pin.bytes().all(|b| b.is_ascii_digit()) && !pin.starts_with('0')
}

/// The place of supply the JSON carries.
fn pos(doc: &Document) -> Option<String> {
    if doc.supply_type.is_export() {
        return Some(EXPORT_STATE.to_owned());
    }
    doc.place_of_supply.clone().or_else(|| doc.buyer.state_code.clone())
}

/// Whether CGST + SGST (rather than IGST) is expected.
fn intra_state(doc: &Document) -> bool {
    !doc.supply_type.zero_rated()
        && !doc.igst_on_intra
        && doc.seller.state_code.is_some()
        && doc.seller.state_code == pos(doc)
}

fn party_problems(who: &str, p: &Party, export_buyer: bool, out: &mut Vec<String>) {
    if p.legal_name.trim().is_empty() {
        out.push(format!("the {who}'s legal name is missing"));
    }
    if p.address.line1.trim().is_empty() {
        out.push(format!("the {who}'s address (first line) is missing"));
    }
    if p.address.location.trim().is_empty() {
        out.push(format!("the {who}'s city or town (location) is missing"));
    }
    match p.address.pin.as_deref().map(str::trim) {
        Some(pin) if valid_pin(pin) => {}
        None | Some("") if export_buyer => {}
        None | Some("") => out.push(format!("the {who}'s PIN code is missing")),
        Some(pin) => out.push(format!("the {who}'s PIN code {pin:?} is not six digits")),
    }
    match p.state_code.as_deref() {
        None if export_buyer => {}
        None => out.push(format!("the {who}'s state is missing")),
        Some(c) if export_buyer && c == EXPORT_STATE => {}
        Some(c) if !place_code(c) => out.push(format!("the {who}'s state code {c:?} is not a GST state code")),
        Some(_) => {}
    }
}

/// Everything that stops `doc` from being reported, in plain words. Empty
/// when the local checks pass (which does not mean the IRP will accept it).
/// `hsn_digits` is the minimum HSN length for the seller: 4 up to ₹5 crore
/// AATO, 6 above (Notif. 78/2020-CT).
pub fn validate(doc: &Document, hsn_digits: usize) -> Vec<String> {
    let mut out = Vec::new();
    let st = doc.supply_type;
    let fy = FinancialYear::containing(doc.date, crate::FY_START_MONTH);
    if let Err(e) = gst::validate_invoice_number(&doc.number, fy, &[]) {
        out.push(format!("document number: {e}"));
    }

    // Seller.
    match doc.seller.gstin.as_deref().map(gstin::validate) {
        None => out.push("the company's GSTIN is missing".into()),
        Some(Err(e)) => out.push(format!("the company's GSTIN is invalid: {e}")),
        Some(Ok(d)) => {
            let reg = d.region.map(|r| r.code);
            if doc.seller.state_code.is_some() && doc.seller.state_code.as_deref() != reg {
                out.push(format!(
                    "the company's state ({}) is not the state of its GSTIN ({})",
                    doc.seller.state_code.as_deref().unwrap_or_default(),
                    reg.unwrap_or_default()
                ));
            }
        }
    }
    party_problems("company", &doc.seller, false, &mut out);

    // Buyer.
    match doc.buyer.gstin.as_deref().filter(|g| !g.trim().is_empty()) {
        None if st.is_export() => {}
        None => out.push(format!("the buyer's GSTIN is missing; a {} e-invoice needs a registered buyer", st.code())),
        Some(g) if st.is_export() && g == EXPORT_BUYER_GSTIN => {}
        Some(g) => {
            if let Err(e) = gstin::validate(g) {
                out.push(format!("the buyer's GSTIN {g} is invalid: {e}"));
            }
        }
    }
    party_problems("buyer", &doc.buyer, st.is_export(), &mut out);
    match pos(doc) {
        None => out.push("the place of supply is missing".into()),
        Some(p) if !st.is_export() && !place_code(&p) => {
            out.push(format!("the place of supply {p:?} is not a GST state code"))
        }
        Some(_) => {}
    }

    // Items.
    if doc.items.is_empty() {
        out.push("there are no items".into());
    }
    let tol = line_tolerance();
    let intra = intra_state(doc);
    for (i, it) in doc.items.iter().enumerate() {
        let name = format!("item {} ({})", i + 1, it.description);
        if it.description.trim().is_empty() {
            out.push(format!("item {}: the description is missing", i + 1));
        }
        match it.hsn.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
            None => out.push(format!("{name}: the HSN/SAC code is missing")),
            Some(h) if !h.bytes().all(|b| b.is_ascii_digit()) => {
                out.push(format!("{name}: the HSN/SAC code {h:?} must be digits only"))
            }
            Some(h) if h.len() < hsn_digits => {
                out.push(format!("{name}: the HSN/SAC code {h} has {} digits; at least {hsn_digits} are needed", h.len()))
            }
            Some(h) if ![4, 6, 8].contains(&h.len()) => {
                out.push(format!("{name}: the HSN/SAC code {h} must have 4, 6 or 8 digits"))
            }
            Some(_) => {}
        }
        if !it.is_service() {
            if it.qty.is_none() {
                out.push(format!("{name}: goods need a quantity"));
            }
            if it.unit.is_none() {
                out.push(format!("{name}: the unit has no GST unit code (UQC); map it or pick a standard unit"));
            }
        }
        if let (Some(q), Some(p)) = (it.qty, it.unit_price) {
            if (q * p - it.gross).abs() > tol {
                out.push(format!(
                    "{name}: quantity × price is {}, but the amount is {}",
                    (q * p).round_dp(2).normalize(),
                    it.gross.normalize()
                ));
            }
        }
        if it.gross < Decimal::ZERO || it.discount < Decimal::ZERO || it.assessable() < Decimal::ZERO {
            out.push(format!("{name}: amounts must not be negative"));
        }
        if st.without_payment() && !(it.tax() + it.cess).is_zero() {
            out.push(format!("{name}: a supply under LUT or bond ({}) carries no tax, but {} is charged", st.code(), (it.tax() + it.cess).normalize()));
            continue;
        }
        if doc.reverse_charge || st.without_payment() {
            continue;
        }
        if intra && !it.igst.is_zero() {
            out.push(format!("{name}: an intra-State supply takes CGST + SGST, not IGST"));
        }
        if !intra && !(it.cgst + it.sgst).is_zero() {
            out.push(format!("{name}: an inter-State, SEZ or export supply takes IGST, not CGST + SGST"));
        }
        if (it.cgst - it.sgst).abs() > Decimal::new(1, 2) {
            out.push(format!("{name}: CGST {} and SGST {} should be equal", it.cgst.normalize(), it.sgst.normalize()));
        }
        let expected = it.assessable() * it.gst_rate / Decimal::ONE_HUNDRED;
        if (expected - it.tax()).abs() > tol {
            out.push(format!(
                "{name}: tax is {}, but {}% of {} is {}",
                it.tax().normalize(),
                it.gst_rate.normalize(),
                it.assessable().normalize(),
                expected.round_dp(2).normalize()
            ));
        }
    }

    // Totals.
    let sum: Decimal = doc.items.iter().map(Item::total).sum::<Decimal>() + doc.round_off;
    if (sum - doc.total).abs() > Decimal::new(1, 2) {
        out.push(format!(
            "the items, tax and round-off add up to {}, but the document total is {}",
            sum.round_dp(2).normalize(),
            doc.total.normalize()
        ));
    }
    out
}

fn num(d: Decimal, dp: u32) -> Value {
    serde_json::Number::from_f64(d.round_dp(dp).to_f64().unwrap_or_default()).map_or(Value::Null, Value::Number)
}

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

/// `Pin` as a number. [verify: number in the schema as reproduced]
fn pin_value(pin: Option<&str>) -> Value {
    pin.and_then(|p| p.trim().parse::<u64>().ok()).map_or(Value::Null, |p| json!(p))
}

fn party_json(p: &Party, gstin: &str, state: &str, pin: Option<&str>) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("Gstin".into(), json!(gstin));
    m.insert("LglNm".into(), json!(p.legal_name.trim()));
    m.insert("Addr1".into(), json!(p.address.line1.trim()));
    if !p.address.line2.trim().is_empty() {
        m.insert("Addr2".into(), json!(p.address.line2.trim()));
    }
    m.insert("Loc".into(), json!(p.address.location.trim()));
    m.insert("Pin".into(), pin_value(pin));
    m.insert("Stcd".into(), json!(state));
    m
}

/// The INV-01 JSON for one document, or why it cannot be built. This is the
/// only place that knows the field names. [verify against the official
/// schema; see the module docs]
pub fn to_json(doc: &Document, hsn_digits: usize) -> Result<Value, DocErrors> {
    let problems = validate(doc, hsn_digits);
    if !problems.is_empty() {
        return Err(DocErrors { document: doc.label.clone(), problems });
    }
    let st = doc.supply_type;
    let export = st.is_export();
    let seller_gstin = doc.seller.gstin.as_deref().map(gstin::normalise).unwrap_or_default();
    let buyer_gstin = doc
        .buyer
        .gstin
        .as_deref()
        .filter(|g| !g.trim().is_empty())
        .map_or_else(|| EXPORT_BUYER_GSTIN.to_owned(), gstin::normalise);
    let seller = party_json(
        &doc.seller,
        &seller_gstin,
        doc.seller.state_code.as_deref().unwrap_or_default(),
        doc.seller.address.pin.as_deref(),
    );
    let buyer_state = doc.buyer.state_code.as_deref().filter(|_| !export).unwrap_or(EXPORT_STATE);
    let buyer_pin = doc.buyer.address.pin.as_deref().filter(|p| !p.trim().is_empty()).or(export.then_some(EXPORT_PIN));
    let mut buyer = party_json(&doc.buyer, &buyer_gstin, buyer_state, buyer_pin);
    // Pos sits after LglNm in the schema; JSON objects are unordered, but
    // keep it readable.
    buyer.insert("Pos".into(), json!(pos(doc).unwrap_or_default()));

    let items: Vec<Value> = doc
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let mut m = Map::new();
            m.insert("SlNo".into(), json!((i + 1).to_string()));
            m.insert("PrdDesc".into(), json!(it.description.trim()));
            m.insert("IsServc".into(), json!(yn(it.is_service())));
            m.insert("HsnCd".into(), json!(it.hsn.as_deref().unwrap_or_default().trim()));
            if let Some(q) = it.qty {
                m.insert("Qty".into(), num(q, 3));
            }
            if let Some(u) = &it.unit {
                m.insert("Unit".into(), json!(u));
            }
            m.insert("UnitPrice".into(), num(it.unit_price.unwrap_or(it.gross), 3));
            m.insert("TotAmt".into(), num(it.gross, 2));
            m.insert("Discount".into(), num(it.discount, 2));
            m.insert("AssAmt".into(), num(it.assessable(), 2));
            m.insert("GstRt".into(), num(it.gst_rate, 3));
            m.insert("IgstAmt".into(), num(it.igst, 2));
            m.insert("CgstAmt".into(), num(it.cgst, 2));
            m.insert("SgstAmt".into(), num(it.sgst, 2));
            m.insert("CesRt".into(), num(it.cess_rate, 3));
            m.insert("CesAmt".into(), num(it.cess, 2));
            m.insert("TotItemVal".into(), num(it.total(), 2));
            Value::Object(m)
        })
        .collect();
    let sum = |f: fn(&Item) -> Decimal| doc.items.iter().map(f).sum::<Decimal>();
    let total = sum(Item::total) + doc.round_off;
    Ok(json!({
        "Version": SCHEMA_VERSION,
        "TranDtls": {
            "TaxSch": "GST",
            "SupTyp": st.code(),
            "RegRev": yn(doc.reverse_charge),
            "IgstOnIntra": yn(doc.igst_on_intra),
        },
        "DocDtls": {
            "Typ": doc.doc_type.code(),
            "No": doc.number,
            "Dt": doc.date.format("%d/%m/%Y").to_string(),
        },
        "SellerDtls": seller,
        "BuyerDtls": buyer,
        "ItemList": items,
        "ValDtls": {
            "AssVal": num(sum(Item::assessable), 2),
            "CgstVal": num(sum(|i| i.cgst), 2),
            "SgstVal": num(sum(|i| i.sgst), 2),
            "IgstVal": num(sum(|i| i.igst), 2),
            "CesVal": num(sum(|i| i.cess), 2),
            "RndOffAmt": num(doc.round_off, 2),
            "TotInvVal": num(total, 2),
        },
    }))
}

/// Several documents as one JSON array, for the portal's bulk upload.
/// Nothing is produced unless every document passes; the error lists each
/// failing document. A document number used twice for the same type is an
/// error too. [UNCERTAIN: that the bulk tool takes a plain array of INV-01
/// objects; verify with the portal's offline tool]
pub fn bulk_json(docs: &[Document], hsn_digits: usize) -> Result<Value, Vec<DocErrors>> {
    let mut out = Vec::with_capacity(docs.len());
    let mut errors = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let dup = docs[..i]
            .iter()
            .any(|o| o.doc_type == d.doc_type && o.number.eq_ignore_ascii_case(&d.number) && FinancialYear::containing(o.date, crate::FY_START_MONTH) == FinancialYear::containing(d.date, crate::FY_START_MONTH));
        match to_json(d, hsn_digits) {
            Ok(_) if dup => errors.push(DocErrors {
                document: d.label.clone(),
                problems: vec![format!("document number {} is used twice", d.number)],
            }),
            Ok(v) => out.push(v),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() { Ok(Value::Array(out)) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn seller() -> Party {
        Party {
            gstin: Some("27AAPFU0939F1ZV".into()),
            legal_name: "Demo Traders".into(),
            address: parse_address("12 MG Road\nCamp\nPune 411001"),
            state_code: Some("27".into()),
        }
    }

    fn buyer(gstin: &str) -> Party {
        Party {
            gstin: Some(gstin.into()),
            legal_name: "Bengaluru Co".into(),
            address: parse_address("4 Residency Road, Bengaluru - 560025"),
            state_code: gstin.get(..2).map(str::to_owned),
        }
    }

    fn item(gross: Decimal, rate: Decimal, igst: Decimal, cgst: Decimal) -> Item {
        Item {
            description: "Laptop".into(),
            hsn: Some("847130".into()),
            qty: Some(dec!(2)),
            unit: Some("NOS".into()),
            unit_price: Some(gross / dec!(2)),
            gross,
            gst_rate: rate,
            igst,
            cgst,
            sgst: cgst,
            ..Item::default()
        }
    }

    fn doc(buyer: Party, items: Vec<Item>, total: Decimal) -> Document {
        Document {
            label: "Sales 1".into(),
            supply_type: SupplyType::B2b,
            doc_type: DocType::Invoice,
            number: "INV/26-27/1".into(),
            date: d(2026, 5, 2),
            reverse_charge: false,
            igst_on_intra: false,
            seller: seller(),
            place_of_supply: buyer.state_code.clone(),
            buyer,
            items,
            round_off: Decimal::ZERO,
            total,
        }
    }

    #[test]
    fn addresses_split_into_lines_location_and_pin() {
        assert_eq!(parse_address("12 MG Road\nCamp\nPune 411001"), Address {
            line1: "12 MG Road".into(),
            line2: "Camp".into(),
            location: "Pune".into(),
            pin: Some("411001".into()),
        });
        let a = parse_address("4 Residency Road, Shanti Nagar, Bengaluru - 560025");
        assert_eq!((a.line1.as_str(), a.line2.as_str(), a.location.as_str()), ("4 Residency Road, Shanti Nagar", "", "Bengaluru"));
        assert_eq!(a.pin.as_deref(), Some("560025"));
        let a = parse_address("Plot 1234567\nMIDC\nNagpur\nPIN: 440010");
        assert_eq!((a.location.as_str(), a.pin.as_deref()), ("Nagpur", Some("440010")), "7 digits are not a PIN");
        assert_eq!(find_pin("Shop 12, Pune"), None);
        assert_eq!(parse_address("").line1, "");
    }

    #[test]
    fn b2b_within_a_state_is_cgst_plus_sgst() {
        let mut b = buyer("27AAPFU0939F1ZV");
        b.legal_name = "Mumbai Co".into();
        let doc = doc(b, vec![item(dec!(100000), dec!(18), dec!(0), dec!(9000))], dec!(118000));
        let j = to_json(&doc, 6).unwrap();
        assert_eq!(j["Version"], "1.1");
        assert_eq!(j["TranDtls"], json!({"TaxSch": "GST", "SupTyp": "B2B", "RegRev": "N", "IgstOnIntra": "N"}));
        assert_eq!(j["DocDtls"], json!({"Typ": "INV", "No": "INV/26-27/1", "Dt": "02/05/2026"}));
        assert_eq!(j["SellerDtls"]["Addr1"], "12 MG Road");
        assert_eq!(j["SellerDtls"]["Addr2"], "Camp");
        assert_eq!(j["SellerDtls"]["Loc"], "Pune");
        assert_eq!(j["SellerDtls"]["Pin"], 411001);
        assert_eq!(j["SellerDtls"]["Stcd"], "27");
        assert_eq!(j["BuyerDtls"]["Pos"], "27");
        let it = &j["ItemList"][0];
        assert_eq!((it["SlNo"].as_str(), it["IsServc"].as_str(), it["HsnCd"].as_str()), (Some("1"), Some("N"), Some("847130")));
        assert_eq!((it["CgstAmt"].as_f64(), it["SgstAmt"].as_f64(), it["IgstAmt"].as_f64()), (Some(9000.0), Some(9000.0), Some(0.0)));
        assert_eq!(it["TotItemVal"].as_f64(), Some(118000.0));
        assert_eq!(j["ValDtls"]["AssVal"].as_f64(), Some(100000.0));
        assert_eq!(j["ValDtls"]["TotInvVal"].as_f64(), Some(118000.0));
    }

    #[test]
    fn b2b_across_states_is_igst() {
        let doc = doc(buyer("29AAGCB7383J1Z4"), vec![item(dec!(100000), dec!(18), dec!(18000), dec!(0))], dec!(118000));
        let j = to_json(&doc, 6).unwrap();
        assert_eq!((j["BuyerDtls"]["Gstin"].as_str(), j["BuyerDtls"]["Pos"].as_str()), (Some("29AAGCB7383J1Z4"), Some("29")));
        assert_eq!((j["BuyerDtls"]["Loc"].as_str(), j["BuyerDtls"]["Pin"].as_u64()), (Some("Bengaluru"), Some(560025)));
        assert_eq!(j["ValDtls"]["IgstVal"].as_f64(), Some(18000.0));
        // The wrong split for the place of supply is caught.
        let wrong = super::tests::doc(buyer("29AAGCB7383J1Z4"), vec![item(dec!(100000), dec!(18), dec!(0), dec!(9000))], dec!(118000));
        assert!(validate(&wrong, 6).iter().any(|p| p.contains("takes IGST")), "{:?}", validate(&wrong, 6));
    }

    #[test]
    fn credit_notes_are_crn_with_positive_amounts() {
        let mut doc = doc(buyer("29AAGCB7383J1Z4"), vec![item(dec!(50000), dec!(18), dec!(9000), dec!(0))], dec!(59000));
        doc.doc_type = DocType::CreditNote;
        doc.number = "CN-1".into();
        let j = to_json(&doc, 6).unwrap();
        assert_eq!(j["DocDtls"]["Typ"], "CRN");
        assert_eq!(j["ValDtls"]["TotInvVal"].as_f64(), Some(59000.0));
    }

    #[test]
    fn exports_without_payment_carry_no_tax_and_urp() {
        let foreign = Party {
            gstin: None,
            legal_name: "Acme GmbH".into(),
            address: parse_address("Hauptstrasse 5\nBerlin"),
            state_code: None,
        };
        let mut doc = doc(foreign, vec![item(dec!(100000), dec!(18), dec!(0), dec!(0))], dec!(100000));
        doc.supply_type = SupplyType::ExportWithoutPayment;
        doc.place_of_supply = None;
        let j = to_json(&doc, 6).unwrap();
        assert_eq!(j["TranDtls"]["SupTyp"], "EXPWOP");
        assert_eq!(j["BuyerDtls"]["Gstin"], "URP");
        assert_eq!((j["BuyerDtls"]["Pos"].as_str(), j["BuyerDtls"]["Stcd"].as_str()), (Some("96"), Some("96")));
        assert_eq!(j["BuyerDtls"]["Pin"], 999999);
        assert_eq!(j["ValDtls"]["TotInvVal"].as_f64(), Some(100000.0));
        // Tax on a supply under LUT is an error.
        doc.items[0].igst = dec!(18000);
        doc.total = dec!(118000);
        assert!(validate(&doc, 6).iter().any(|p| p.contains("LUT")));
        doc.supply_type = SupplyType::ExportWithPayment;
        assert_eq!(validate(&doc, 6), Vec::<String>::new(), "EXPWP pays IGST");
        assert_eq!(SupplyType::parse("expwop"), Some(SupplyType::ExportWithoutPayment));
    }

    #[test]
    fn round_off_is_part_of_the_total() {
        // 18% of 1000.40 = 180.07 (90.04 + 90.04 booked = 180.08).
        let mut it = item(dec!(1000.40), dec!(18), dec!(0), dec!(90.04));
        it.qty = Some(dec!(1));
        it.unit_price = Some(dec!(1000.40));
        let mut b = buyer("27AAPFU0939F1ZV");
        b.legal_name = "Local Co".into();
        let mut doc = doc(b, vec![it], dec!(1180));
        doc.round_off = dec!(-0.48);
        let j = to_json(&doc, 6).unwrap();
        assert_eq!(j["ValDtls"]["RndOffAmt"].as_f64(), Some(-0.48));
        assert_eq!(j["ValDtls"]["TotInvVal"].as_f64(), Some(1180.0));
        doc.round_off = Decimal::ZERO;
        let p = validate(&doc, 6);
        assert!(p.iter().any(|p| p.contains("add up to 1180.48, but the document total is 1180")), "{p:?}");
    }

    #[test]
    fn what_the_books_lack_is_reported_per_document() {
        let mut b = buyer("29AAGCB7383J1Z4");
        b.gstin = None;
        b.address = parse_address("Somewhere");
        b.state_code = Some("29".into());
        let mut it = item(dec!(100000), dec!(18), dec!(18000), dec!(0));
        it.hsn = Some("8471".into());
        it.unit = None;
        it.qty = Some(dec!(3));
        let mut doc = doc(b, vec![it], dec!(118000));
        doc.seller.address.pin = None;
        doc.number = "INV 1".into();
        let e = to_json(&doc, 6).unwrap_err();
        assert_eq!(e.document, "Sales 1");
        let all = e.problems.join("\n");
        for needle in [
            "document number",
            "company's PIN code is missing",
            "buyer's GSTIN is missing",
            "buyer's city or town",
            "buyer's PIN code is missing",
            "8471 has 4 digits; at least 6",
            "no GST unit code",
            "quantity × price is 150000",
        ] {
            assert!(all.contains(needle), "missing {needle:?} in\n{all}");
        }
        assert!(e.to_string().starts_with("Sales 1: "));
        // Four digits are enough for a smaller seller.
        assert!(!validate(&doc, 4).iter().any(|p| p.contains("HSN")));
        // A bad tax amount for the rate.
        let mut doc = super::tests::doc(buyer("29AAGCB7383J1Z4"), vec![item(dec!(100000), dec!(5), dec!(18000), dec!(0))], dec!(118000));
        assert!(validate(&doc, 6).iter().any(|p| p.contains("5% of 100000 is 5000")));
        doc.seller.state_code = Some("29".into());
        assert!(validate(&doc, 6).iter().any(|p| p.contains("not the state of its GSTIN")));
    }

    #[test]
    fn services_need_no_unit() {
        let mut it = item(dec!(10000), dec!(18), dec!(1800), dec!(0));
        it.hsn = Some("998311".into());
        (it.qty, it.unit, it.unit_price) = (None, None, None);
        let doc = doc(buyer("29AAGCB7383J1Z4"), vec![it], dec!(11800));
        let j = to_json(&doc, 6).unwrap();
        assert_eq!(j["ItemList"][0]["IsServc"], "Y");
        assert!(j["ItemList"][0].get("Unit").is_none());
        assert_eq!(j["ItemList"][0]["UnitPrice"].as_f64(), Some(10000.0));
    }

    #[test]
    fn bulk_is_all_or_nothing() {
        let ok = doc(buyer("29AAGCB7383J1Z4"), vec![item(dec!(100000), dec!(18), dec!(18000), dec!(0))], dec!(118000));
        let mut other = ok.clone();
        other.number = "INV/26-27/2".into();
        assert_eq!(bulk_json(&[ok.clone(), other], 6).unwrap().as_array().unwrap().len(), 2);
        let mut bad = ok.clone();
        bad.label = "Sales 3".into();
        bad.number = "INV/26-27/3".into();
        bad.buyer.gstin = None;
        let errors = bulk_json(&[ok.clone(), bad, ok.clone()], 6).unwrap_err();
        let docs: Vec<_> = errors.iter().map(|e| e.document.as_str()).collect();
        assert_eq!(docs, ["Sales 3", "Sales 1"], "the bad one and the duplicate");
        assert!(errors[1].problems[0].contains("used twice"));
    }

    #[test]
    fn the_30_day_limit_needs_10_crore() {
        assert_eq!(last_reporting_day(d(2026, 5, 2), dec!(100000000)), Some(d(2026, 6, 1)));
        assert_eq!(last_reporting_day(d(2026, 5, 2), dec!(99999999)), None);
        assert_eq!(applicability_threshold(), dec!(50000000));
    }
}

//! e-Way bill JSON (Part A, with optional Part B) for the e-way bill
//! portal's bulk generation tool, built from a neutral model. Items reuse
//! [`einvoice::Item`].
//!
//! Like [`crate::einvoice`], this only **generates** JSON. It does not
//! connect to the e-way bill system, does not produce an e-way bill number,
//! and does not claim the portal will accept the file.
//!
//! Sources (`docs/research/india-compliance.md` §4.6):
//! - CGST Rule 138(1): an e-way bill is needed before moving goods of
//!   consignment value over ₹50,000 (including tax, excluding exempt
//!   goods); some states set other intra-State limits [UNCERTAIN];
//! - Part A (consignor, consignee, goods, HSN, value, document) and Part B
//!   (transporter or vehicle); Part A alone stays valid for 15 days for
//!   updating Part B (Rule 138(1), (3)) [PRIMARY];
//! - validity: one day per 200 km or part of it, 20 km for over-dimensional
//!   cargo (Rule 138(10)) [PRIMARY];
//! - no e-way bill for documents dated more than 180 days earlier (GSTN
//!   advisory, 17 Dec 2024, from 1 Jan 2025) [PRIMARY].
//!
//! **The field names, code lists (sub-supply types, transport modes,
//! transaction types) and the `version` value must be verified against the
//! portal's current bulk-generation tool**, which could not be retrieved;
//! they follow the format as commonly reproduced. See `docs/einvoice.md`.
//! The whole shape lives in [`bill_json`].

use chrono::NaiveDate;
use khata_compliance::FinancialYear;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::einvoice::{Address, DocErrors, Item};
use crate::states::{self, Kind};
use crate::{gst, gstin};

/// The `version` written at the top of the bulk file. [UNCERTAIN: as seen
/// in bulk-tool samples; copy the value from the current tool]
pub const JSON_VERSION: &str = "1.0.0621";

/// Rule 138(1): an e-way bill is needed above this consignment value
/// (₹50,000) [PRIMARY]; state-specific intra-State limits are not modelled
/// [UNCERTAIN].
pub fn consignment_threshold() -> Decimal {
    Decimal::new(50_000, 0)
}

/// Whether a consignment of this value needs an e-way bill under the
/// general limit (inter-State job work and handicrafts need one at any
/// value; the caller decides those).
pub fn needed(consignment_value: Decimal) -> bool {
    consignment_value > consignment_threshold()
}

/// Days of validity for a distance (Rule 138(10)): one per 200 km or part
/// of it, or per 20 km for over-dimensional cargo; at least one.
pub fn validity_days(distance_km: u32, over_dimensional: bool) -> u32 {
    let per = if over_dimensional { 20 } else { 200 };
    distance_km.div_ceil(per).max(1)
}

/// Documents older than this many days cannot get an e-way bill (GSTN
/// advisory, 17 Dec 2024, from 1 Jan 2025) [PRIMARY].
pub const MAX_DOC_AGE_DAYS: i64 = 180;

/// `supplyType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Outward,
    Inward,
}

impl Direction {
    pub fn code(self) -> &'static str {
        match self {
            Direction::Outward => "O",
            Direction::Inward => "I",
        }
    }
}

/// `subSupplyType`. [UNCERTAIN: codes as commonly reproduced; verify]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubSupply {
    Supply,
    Import,
    Export,
    JobWork,
    ForOwnUse,
    JobWorkReturns,
    SalesReturn,
    Others,
    SkdCkd,
    LineSales,
    RecipientNotKnown,
    ExhibitionOrFairs,
}

impl SubSupply {
    pub fn code(self) -> u8 {
        match self {
            SubSupply::Supply => 1,
            SubSupply::Import => 2,
            SubSupply::Export => 3,
            SubSupply::JobWork => 4,
            SubSupply::ForOwnUse => 5,
            SubSupply::JobWorkReturns => 6,
            SubSupply::SalesReturn => 7,
            SubSupply::Others => 8,
            SubSupply::SkdCkd => 9,
            SubSupply::LineSales => 10,
            SubSupply::RecipientNotKnown => 11,
            SubSupply::ExhibitionOrFairs => 12,
        }
    }
}

/// `docType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocType {
    /// Tax invoice.
    Invoice,
    /// Bill of supply.
    BillOfSupply,
    /// Delivery challan.
    Challan,
}

impl DocType {
    pub fn code(self) -> &'static str {
        match self {
            DocType::Invoice => "INV",
            DocType::BillOfSupply => "BIL",
            DocType::Challan => "CHL",
        }
    }

    pub fn parse(code: &str) -> Option<DocType> {
        [DocType::Invoice, DocType::BillOfSupply, DocType::Challan]
            .into_iter()
            .find(|t| t.code().eq_ignore_ascii_case(code.trim()))
    }
}

/// `transactionType`. [UNCERTAIN: codes; verify]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    #[default]
    Regular,
    BillToShipTo,
    BillFromDispatchFrom,
    Combination,
}

impl TransactionType {
    pub fn code(self) -> u8 {
        match self {
            TransactionType::Regular => 1,
            TransactionType::BillToShipTo => 2,
            TransactionType::BillFromDispatchFrom => 3,
            TransactionType::Combination => 4,
        }
    }
}

/// `transMode`. [UNCERTAIN: codes; verify]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransMode {
    Road,
    Rail,
    Air,
    Ship,
}

impl TransMode {
    pub fn code(self) -> &'static str {
        match self {
            TransMode::Road => "1",
            TransMode::Rail => "2",
            TransMode::Air => "3",
            TransMode::Ship => "4",
        }
    }
}

/// `vehicleType`: regular, or over-dimensional cargo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VehicleType {
    #[default]
    Regular,
    OverDimensional,
}

impl VehicleType {
    pub fn code(self) -> &'static str {
        match self {
            VehicleType::Regular => "R",
            VehicleType::OverDimensional => "O",
        }
    }
}

/// Consignor or consignee.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// `None`: unregistered ("URP").
    pub gstin: Option<String>,
    pub name: String,
    pub address: Address,
    /// The state of the registration or billing address.
    pub state_code: Option<String>,
    /// Where the goods actually leave from or go to, when different.
    pub actual_state_code: Option<String>,
}

/// Part B and the distance. All optional except the distance: Part B can
/// be added on the portal later (Rule 138(3)).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transport {
    pub mode: Option<TransMode>,
    /// Approximate distance in km (PIN to PIN).
    pub distance_km: Option<u32>,
    pub transporter_id: String,
    pub transporter_name: String,
    /// Railway receipt, airway bill or bill of lading (non-road modes).
    pub doc_no: String,
    pub doc_date: Option<NaiveDate>,
    pub vehicle_no: String,
    pub vehicle_type: VehicleType,
}

/// One e-way bill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EwayBill {
    /// How errors name it ("Sales 26-27/1").
    pub label: String,
    /// The GSTIN generating the bill (the company).
    pub user_gstin: String,
    pub direction: Direction,
    pub sub_supply: SubSupply,
    pub doc_type: DocType,
    pub number: String,
    pub date: NaiveDate,
    pub from: Place,
    pub to: Place,
    pub transaction_type: TransactionType,
    /// Lines; service lines (SAC 99…) are not goods, so their value goes to
    /// `otherValue`.
    pub items: Vec<Item>,
    /// Round-off and other charges outside the goods lines, signed.
    pub other_value: Decimal,
    /// The document total in the books; `totInvValue` must match it.
    pub total: Decimal,
    pub transport: Transport,
}

fn place_code(code: &str) -> bool {
    states::state(code).is_some_and(|s| s.kind != Kind::CentreJurisdiction && !s.legacy)
}

/// Upper case, without spaces and hyphens ("MH 12 AB-1234" → "MH12AB1234").
pub fn normalise_vehicle(no: &str) -> String {
    no.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_ascii_uppercase()
}

fn goods(bill: &EwayBill) -> impl Iterator<Item = &Item> {
    bill.items.iter().filter(|i| !i.is_service())
}

/// IGST rather than CGST + SGST: from the tax booked, else by comparing the
/// states.
fn inter_state(bill: &EwayBill) -> bool {
    if goods(bill).any(|i| !i.igst.is_zero()) {
        return true;
    }
    if goods(bill).any(|i| !(i.cgst + i.sgst).is_zero()) {
        return false;
    }
    let state = |p: &Place| p.actual_state_code.clone().or_else(|| p.state_code.clone());
    state(&bill.from) != state(&bill.to)
}

fn place_problems(who: &str, p: &Place, out: &mut Vec<String>) {
    if let Some(g) = p.gstin.as_deref().filter(|g| !g.trim().is_empty() && *g != "URP") {
        if let Err(e) = gstin::validate(g) {
            out.push(format!("the {who} GSTIN {g} is invalid: {e}"));
        }
    }
    match p.address.pin.as_deref().map(str::trim) {
        None | Some("") => out.push(format!("the {who} PIN code is missing")),
        Some(pin) if pin.len() != 6 || !pin.bytes().all(|b| b.is_ascii_digit()) || pin.starts_with('0') => {
            out.push(format!("the {who} PIN code {pin:?} is not six digits"))
        }
        Some(_) => {}
    }
    match p.state_code.as_deref() {
        None => out.push(format!("the {who} state is missing")),
        Some(c) if !place_code(c) => out.push(format!("the {who} state code {c:?} is not a GST state code")),
        Some(_) => {}
    }
    if let Some(c) = p.actual_state_code.as_deref().filter(|c| !place_code(c)) {
        out.push(format!("the {who} actual state code {c:?} is not a GST state code"));
    }
}

/// Everything that stops `bill` from being generated, in plain words.
/// `hsn_digits` is the minimum HSN length (4 or 6 by AATO, as on invoices).
pub fn validate(bill: &EwayBill, hsn_digits: usize) -> Vec<String> {
    let mut out = Vec::new();
    let fy = FinancialYear::containing(bill.date, crate::FY_START_MONTH);
    if let Err(e) = gst::validate_invoice_number(&bill.number, fy, &[]) {
        out.push(format!("document number: {e}"));
    }
    match gstin::validate(&bill.user_gstin) {
        Err(e) => out.push(format!("the company's GSTIN is missing or invalid: {e}")),
        Ok(d) => {
            let own = match bill.direction {
                Direction::Outward => &bill.from,
                Direction::Inward => &bill.to,
            };
            if own.gstin.as_deref().map(gstin::normalise) != Some(d.value) {
                let side = if bill.direction == Direction::Outward { "consignor (from)" } else { "consignee (to)" };
                out.push(format!("the {side} GSTIN must be the company's own"));
            }
        }
    }
    place_problems("consignor (from)", &bill.from, &mut out);
    place_problems("consignee (to)", &bill.to, &mut out);

    if goods(bill).next().is_none() {
        out.push("there are no goods to move (service lines don't need an e-way bill)".into());
    }
    for (i, it) in bill.items.iter().enumerate().filter(|(_, it)| !it.is_service()) {
        let name = format!("item {} ({})", i + 1, it.description);
        match it.hsn.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
            None => out.push(format!("{name}: the HSN code is missing")),
            Some(h) if !h.bytes().all(|b| b.is_ascii_digit()) => out.push(format!("{name}: the HSN code {h:?} must be digits only")),
            Some(h) if h.len() < hsn_digits => {
                out.push(format!("{name}: the HSN code {h} has {} digits; at least {hsn_digits} are needed", h.len()))
            }
            Some(_) => {}
        }
        if it.qty.is_none() {
            out.push(format!("{name}: goods need a quantity"));
        }
        if it.unit.is_none() {
            out.push(format!("{name}: the unit has no GST unit code (UQC); map it or pick a standard unit"));
        }
    }

    let t = &bill.transport;
    match t.distance_km {
        None => out.push("the approximate distance (km) is missing".into()),
        Some(0) => out.push("the distance must be at least 1 km".into()),
        Some(_) => {}
    }
    let vehicle = normalise_vehicle(&t.vehicle_no);
    if !vehicle.is_empty() {
        if !vehicle.chars().all(|c| c.is_ascii_alphanumeric()) {
            out.push(format!("vehicle number {:?} must be letters and digits only", t.vehicle_no));
        }
        if t.mode.is_some_and(|m| m != TransMode::Road) {
            out.push("a vehicle number goes with road transport".into());
        }
    }
    if t.mode.is_some_and(|m| m != TransMode::Road) && t.doc_no.trim().is_empty() {
        out.push("rail, air and ship transport need the transport document number".into());
    }
    let tid = t.transporter_id.trim();
    if !tid.is_empty() && (tid.len() != 15 || !tid.chars().all(|c| c.is_ascii_alphanumeric())) {
        out.push(format!("transporter ID {tid:?} must have 15 letters and digits (GSTIN or TRANSIN)"));
    }

    let sum: Decimal = goods(bill).map(Item::total).sum::<Decimal>() + other_value(bill);
    if (sum - bill.total).abs() > Decimal::new(1, 2) {
        out.push(format!(
            "the goods, tax and other charges add up to {}, but the document total is {}",
            sum.round_dp(2).normalize(),
            bill.total.normalize()
        ));
    }
    out
}

/// `otherValue`: the bill's other charges plus any service lines.
fn other_value(bill: &EwayBill) -> Decimal {
    bill.other_value + bill.items.iter().filter(|i| i.is_service()).map(Item::total).sum::<Decimal>()
}

fn num(d: Decimal, dp: u32) -> Value {
    serde_json::Number::from_f64(d.round_dp(dp).to_f64().unwrap_or_default()).map_or(Value::Null, Value::Number)
}

fn int(code: Option<&str>) -> Value {
    code.and_then(|c| c.trim().parse::<u64>().ok()).map_or(Value::Null, |n| json!(n))
}

fn gstin_or_urp(g: Option<&str>) -> String {
    g.map(str::trim).filter(|g| !g.is_empty()).map_or_else(|| "URP".to_owned(), gstin::normalise)
}

/// One `billLists` entry, or why it cannot be built. This is the only place
/// that knows the field names. [verify against the bulk tool]
pub fn bill_json(bill: &EwayBill, hsn_digits: usize) -> Result<Value, DocErrors> {
    let problems = validate(bill, hsn_digits);
    if !problems.is_empty() {
        return Err(DocErrors { document: bill.label.clone(), problems });
    }
    let inter = inter_state(bill);
    let items: Vec<Value> = goods(bill)
        .enumerate()
        .map(|(i, it)| {
            let half = it.gst_rate / Decimal::TWO;
            let (igst, cgst) = if inter { (it.gst_rate, Decimal::ZERO) } else { (Decimal::ZERO, half) };
            json!({
                "itemNo": i + 1,
                "productName": it.description.trim(),
                "productDesc": it.description.trim(),
                "hsnCode": int(it.hsn.as_deref()),
                "quantity": num(it.qty.unwrap_or_default(), 3),
                "qtyUnit": it.unit.as_deref().unwrap_or_default(),
                "taxableAmount": num(it.assessable(), 2),
                "sgstRate": num(cgst, 3),
                "cgstRate": num(cgst, 3),
                "igstRate": num(igst, 3),
                "cessRate": num(it.cess_rate, 3),
            })
        })
        .collect();
    let sum = |f: fn(&Item) -> Decimal| goods(bill).map(f).sum::<Decimal>();
    let other = other_value(bill);
    let t = &bill.transport;
    fn place(p: &Place) -> (Option<&str>, Option<&str>) {
        (p.state_code.as_deref(), p.actual_state_code.as_deref().or(p.state_code.as_deref()))
    }
    let (from_state, act_from) = place(&bill.from);
    let (to_state, act_to) = place(&bill.to);
    Ok(json!({
        "userGstin": gstin::normalise(&bill.user_gstin),
        "supplyType": bill.direction.code(),
        "subSupplyType": bill.sub_supply.code(),
        "docType": bill.doc_type.code(),
        "docNo": bill.number,
        "docDate": bill.date.format("%d/%m/%Y").to_string(),
        "fromGstin": gstin_or_urp(bill.from.gstin.as_deref()),
        "fromTrdName": bill.from.name.trim(),
        "fromAddr1": bill.from.address.line1.trim(),
        "fromAddr2": bill.from.address.line2.trim(),
        "fromPlace": bill.from.address.location.trim(),
        "fromPincode": int(bill.from.address.pin.as_deref()),
        "fromStateCode": int(from_state),
        "actFromStateCode": int(act_from),
        "toGstin": gstin_or_urp(bill.to.gstin.as_deref()),
        "toTrdName": bill.to.name.trim(),
        "toAddr1": bill.to.address.line1.trim(),
        "toAddr2": bill.to.address.line2.trim(),
        "toPlace": bill.to.address.location.trim(),
        "toPincode": int(bill.to.address.pin.as_deref()),
        "toStateCode": int(to_state),
        "actToStateCode": int(act_to),
        "transactionType": bill.transaction_type.code(),
        "otherValue": num(other, 2),
        "totalValue": num(sum(Item::assessable), 2),
        "cgstValue": num(sum(|i| i.cgst), 2),
        "sgstValue": num(sum(|i| i.sgst), 2),
        "igstValue": num(sum(|i| i.igst), 2),
        "cessValue": num(sum(|i| i.cess), 2),
        "totInvValue": num(sum(Item::total) + other, 2),
        "transMode": t.mode.map_or("", TransMode::code),
        "transDistance": t.distance_km.unwrap_or_default().to_string(),
        "transporterId": t.transporter_id.trim().to_ascii_uppercase(),
        "transporterName": t.transporter_name.trim(),
        "transDocNo": t.doc_no.trim(),
        "transDocDate": t.doc_date.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default(),
        "vehicleNo": normalise_vehicle(&t.vehicle_no),
        "vehicleType": t.vehicle_type.code(),
        "itemList": items,
    }))
}

/// The bulk file: `{ version, billLists: [...] }`. Nothing is produced
/// unless every bill passes.
pub fn to_json(bills: &[EwayBill], hsn_digits: usize) -> Result<Value, Vec<DocErrors>> {
    let mut lists = Vec::with_capacity(bills.len());
    let mut errors = Vec::new();
    for b in bills {
        match bill_json(b, hsn_digits) {
            Ok(v) => lists.push(v),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() { Ok(json!({ "version": JSON_VERSION, "billLists": lists })) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::einvoice::parse_address;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn laptop(gross: Decimal, igst: Decimal, cgst: Decimal) -> Item {
        Item {
            description: "Laptop".into(),
            hsn: Some("847130".into()),
            qty: Some(dec!(2)),
            unit: Some("NOS".into()),
            unit_price: Some(gross / dec!(2)),
            gross,
            gst_rate: dec!(18),
            igst,
            cgst,
            sgst: cgst,
            ..Item::default()
        }
    }

    fn bill(to_gstin: Option<&str>, to_state: &str, items: Vec<Item>, total: Decimal) -> EwayBill {
        EwayBill {
            label: "Sales 1".into(),
            user_gstin: "27AAPFU0939F1ZV".into(),
            direction: Direction::Outward,
            sub_supply: SubSupply::Supply,
            doc_type: DocType::Invoice,
            number: "INV/26-27/1".into(),
            date: d(2026, 5, 2),
            from: Place {
                gstin: Some("27AAPFU0939F1ZV".into()),
                name: "Demo Traders".into(),
                address: parse_address("12 MG Road\nPune 411001"),
                state_code: Some("27".into()),
                actual_state_code: None,
            },
            to: Place {
                gstin: to_gstin.map(str::to_owned),
                name: "Buyer".into(),
                address: parse_address("4 Residency Road\nBengaluru 560025"),
                state_code: Some(to_state.into()),
                actual_state_code: None,
            },
            transaction_type: TransactionType::Regular,
            items,
            other_value: Decimal::ZERO,
            total,
            transport: Transport { distance_km: Some(840), ..Transport::default() },
        }
    }

    #[test]
    fn inter_state_part_a_only() {
        let b = bill(Some("29AAGCB7383J1Z4"), "29", vec![laptop(dec!(100000), dec!(18000), dec!(0))], dec!(118000));
        let j = to_json(&[b], 6).unwrap();
        assert_eq!(j["version"], JSON_VERSION);
        let x = &j["billLists"][0];
        assert_eq!((x["supplyType"].as_str(), x["subSupplyType"].as_u64(), x["docType"].as_str()), (Some("O"), Some(1), Some("INV")));
        assert_eq!((x["fromPincode"].as_u64(), x["fromStateCode"].as_u64()), (Some(411001), Some(27)));
        assert_eq!((x["toGstin"].as_str(), x["toPincode"].as_u64(), x["actToStateCode"].as_u64()), (Some("29AAGCB7383J1Z4"), Some(560025), Some(29)));
        assert_eq!(x["docDate"], "02/05/2026");
        let it = &x["itemList"][0];
        assert_eq!((it["igstRate"].as_f64(), it["cgstRate"].as_f64(), it["hsnCode"].as_u64()), (Some(18.0), Some(0.0), Some(847130)));
        assert_eq!((x["totalValue"].as_f64(), x["igstValue"].as_f64(), x["totInvValue"].as_f64()), (Some(100000.0), Some(18000.0), Some(118000.0)));
        assert_eq!((x["transMode"].as_str(), x["vehicleNo"].as_str(), x["transDistance"].as_str()), (Some(""), Some(""), Some("840")));
    }

    #[test]
    fn intra_state_with_vehicle_and_unregistered_buyer() {
        let mut b = bill(None, "27", vec![laptop(dec!(100000), dec!(0), dec!(9000))], dec!(118000.50));
        b.other_value = dec!(0.50);
        b.transport = Transport {
            mode: Some(TransMode::Road),
            distance_km: Some(150),
            vehicle_no: "mh 12 ab-1234".into(),
            ..Transport::default()
        };
        let j = to_json(&[b], 6).unwrap();
        let x = &j["billLists"][0];
        assert_eq!(x["toGstin"], "URP");
        assert_eq!((x["itemList"][0]["cgstRate"].as_f64(), x["itemList"][0]["sgstRate"].as_f64()), (Some(9.0), Some(9.0)));
        assert_eq!((x["transMode"].as_str(), x["vehicleNo"].as_str(), x["vehicleType"].as_str()), (Some("1"), Some("MH12AB1234"), Some("R")));
        assert_eq!((x["otherValue"].as_f64(), x["totInvValue"].as_f64()), (Some(0.5), Some(118000.5)));
    }

    #[test]
    fn services_are_not_goods() {
        let mut svc = laptop(dec!(1000), dec!(180), dec!(0));
        svc.hsn = Some("9965".into());
        (svc.qty, svc.unit) = (None, None);
        let b = bill(Some("29AAGCB7383J1Z4"), "29", vec![laptop(dec!(100000), dec!(18000), dec!(0)), svc.clone()], dec!(119180));
        let x = &to_json(&[b], 6).unwrap()["billLists"][0];
        assert_eq!(x["itemList"].as_array().unwrap().len(), 1);
        assert_eq!((x["otherValue"].as_f64(), x["totInvValue"].as_f64()), (Some(1180.0), Some(119180.0)));
        let only = bill(Some("29AAGCB7383J1Z4"), "29", vec![svc], dec!(1180));
        assert!(validate(&only, 6).iter().any(|p| p.contains("no goods")));
    }

    #[test]
    fn missing_data_is_reported() {
        let mut it = laptop(dec!(100000), dec!(18000), dec!(0));
        it.unit = None;
        it.hsn = Some("8471".into());
        let mut b = bill(Some("29AAGCB7383J1Z4"), "29", vec![it], dec!(118000));
        b.to.address.pin = None;
        b.from.gstin = Some("29AAGCB7383J1Z4".into());
        b.transport = Transport { mode: Some(TransMode::Rail), vehicle_no: "KA01X1".into(), transporter_id: "ABC".into(), ..Transport::default() };
        let e = to_json(&[b], 6).unwrap_err();
        let all = e[0].problems.join("\n");
        for needle in [
            "consignor (from) GSTIN must be the company's own",
            "consignee (to) PIN code is missing",
            "8471 has 4 digits",
            "no GST unit code",
            "distance (km) is missing",
            "vehicle number goes with road",
            "transport document number",
            "transporter ID",
        ] {
            assert!(all.contains(needle), "missing {needle:?} in\n{all}");
        }
    }

    #[test]
    fn thresholds_and_validity() {
        assert!(!needed(dec!(50000)));
        assert!(needed(dec!(50000.01)));
        assert_eq!([validity_days(0, false), validity_days(200, false), validity_days(201, false), validity_days(45, true)], [1, 1, 2, 3]);
        assert_eq!(DocType::parse("chl"), Some(DocType::Challan));
    }
}

//! Tax on invoices: the company's registration, its rate list, which tax
//! each duty ledger collects, and working out an invoice's tax lines with
//! the country's pack (place of supply, components such as CGST + SGST or
//! IGST, rate in force on the invoice date).

use std::collections::BTreeMap;

use khata_compliance::{PlaceOfSupply, RateSchedule, Supply, TaxLine};
use khata_core::book::normalise_tax_class;
use khata_core::{Amount, LedgerId, LedgerTax, StockItemId, TaxSide, UserRole, VoucherKind};
use serde::{Deserialize, Serialize};

use crate::api::{ApiResult, Slot, amt, err, now, open_mut, open_ref, os_user, pack, parse_amount, parse_date, parse_uuid};

const NO_COUNTRY: &str = "pick the company's country first";

/// The region a party is in: the one its tax ID encodes (a GSTIN's state),
/// else the state in its mailing details.
pub fn party_region(pack: &dyn khata_compliance::CompliancePack, party: &khata_core::Ledger) -> Option<String> {
    party
        .tax_id
        .as_deref()
        .and_then(|t| pack.validate_tax_id(t).ok())
        .and_then(|d| d.region.map(|r| r.code.to_owned()))
        .or_else(|| party.mailing.as_ref().and_then(|m| m.region.clone()).filter(|r| !r.trim().is_empty()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RegionDto {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompanyTaxDto {
    /// What the registration is called ("GSTIN").
    pub label: String,
    pub tax_id: Option<String>,
    pub region: Option<String>,
    pub region_name: Option<String>,
    /// Regions to pick from (states, emirates).
    pub regions: Vec<RegionDto>,
    /// Mailing address printed on invoices (several lines).
    pub address: String,
    /// HSN/SAC digits required on invoices (4, 6 or 8), when set.
    pub hsn_digits: Option<u8>,
}

pub fn company_tax(slot: &Slot) -> ApiResult<CompanyTaxDto> {
    let book = open_ref(slot)?;
    let pack = pack(book).ok_or(NO_COUNTRY)?;
    let p = book.profile().ok_or(NO_COUNTRY)?;
    let regions: Vec<RegionDto> =
        pack.regions().into_iter().map(|r| RegionDto { code: r.code.to_owned(), name: r.name.to_owned() }).collect();
    Ok(CompanyTaxDto {
        label: pack.tax_id_label().to_owned(),
        tax_id: p.tax_id.clone(),
        region: p.tax_region.clone(),
        region_name: p.tax_region.as_ref().and_then(|c| regions.iter().find(|r| &r.code == c)).map(|r| r.name.clone()),
        regions,
        address: p.address.clone(),
        hsn_digits: p.hsn_digits,
    })
}

/// How many HSN/SAC digits invoices must show (India: 4 up to ₹5 crore
/// aggregate turnover, 6 above); `None` unsets.
pub fn set_hsn_digits(slot: &mut Slot, digits: Option<u8>) -> ApiResult<CompanyTaxDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Owner)?;
    book.set_hsn_digits(now(), &os_user(), digits).map_err(err)?;
    store.save(book).map_err(err)?;
    company_tax(slot)
}

/// The company's mailing address, printed on invoices and reports.
pub fn set_company_address(slot: &mut Slot, address: &str) -> ApiResult<CompanyTaxDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Owner)?;
    book.set_company_address(now(), &os_user(), address).map_err(err)?;
    store.save(book).map_err(err)?;
    company_tax(slot)
}

/// Set the company's own registration. A tax ID that encodes a region (a
/// GSTIN's state code) decides the region; it must be valid.
pub fn set_company_tax(slot: &mut Slot, tax_id: &str, region: &str) -> ApiResult<CompanyTaxDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Owner)?;
    let pack = pack(book).ok_or(NO_COUNTRY)?;
    let (tax_id, from_id) = match tax_id.trim() {
        "" => (None, None),
        raw => {
            let d = pack
                .validate_tax_id(raw)
                .map_err(|e| format!("{} {raw} is invalid: {e}", pack.tax_id_label()))?;
            (Some(d.value), d.region.map(|r| r.code.to_owned()))
        }
    };
    let region = match (from_id, region.trim()) {
        (Some(code), "") => Some(code),
        (Some(code), r) if r == code => Some(code),
        (Some(code), r) => return Err(format!("the {} is registered in region {code}, not {r}", pack.tax_id_label())),
        (None, "") => None,
        (None, r) => {
            if !pack.regions().iter().any(|x| x.code == r) {
                return Err(format!("{r:?} is not a known region"));
            }
            Some(r.to_owned())
        }
    };
    book.set_company_tax(now(), &os_user(), tax_id, region).map_err(err)?;
    store.save(book).map_err(err)?;
    company_tax(slot)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TaxRateDto {
    pub class: String,
    pub from: chrono::NaiveDate,
    pub percent: String,
}

/// The rate list, by class then date.
pub fn list_tax_rates(slot: &Slot) -> ApiResult<Vec<TaxRateDto>> {
    let book = open_ref(slot)?;
    let p = book.profile().ok_or(NO_COUNTRY)?;
    Ok(p.tax_rates
        .iter()
        .flat_map(|(class, changes)| {
            changes.iter().map(move |(from, r)| TaxRateDto { class: class.clone(), from: *from, percent: amt(*r) })
        })
        .collect())
}

/// Set a class's rate from a date; an empty `percent` removes that change.
pub fn set_tax_rate(slot: &mut Slot, class: &str, from: &str, percent: &str) -> ApiResult<Vec<TaxRateDto>> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Owner)?;
    let class = normalise_tax_class(class);
    if class.is_empty() {
        return Err("type an HSN/SAC code or a tax category".into());
    }
    let percent = if percent.trim().is_empty() { None } else { Some(parse_amount(percent)?) };
    book.set_tax_rate(now(), &os_user(), &class, parse_date(from)?, percent).map_err(err)?;
    store.save(book).map_err(err)?;
    list_tax_rates(slot)
}

/// Mark a duty ledger as collecting a tax (empty `component` clears it).
pub fn set_ledger_tax(slot: &mut Slot, ledger_id: &str, component: &str, side: TaxSide) -> ApiResult<()> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let id = LedgerId(parse_uuid(ledger_id, "ledger")?);
    let mut l = book.ledger(id).ok_or("unknown ledger")?.clone();
    let component = component.trim().to_ascii_uppercase();
    l.tax = (!component.is_empty()).then_some(LedgerTax { component, side });
    book.alter_ledger(now(), &os_user(), l).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvoiceTaxLine {
    /// Taxed by the item's tax class…
    #[serde(default)]
    pub item_id: String,
    /// …or by a sales or service ledger's tax class…
    #[serde(default)]
    pub ledger_id: String,
    /// …or by this class directly (a service line).
    #[serde(default)]
    pub tax_class: String,
    /// Value after discount, before tax.
    pub taxable_value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvoiceTaxInput {
    pub kind: VoucherKind,
    pub date: String,
    pub party_id: String,
    pub lines: Vec<InvoiceTaxLine>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InvoiceTaxRow {
    /// "CGST", "IGST"…
    pub component: String,
    pub percent: String,
    pub amount: String,
    /// The duty ledger to post it to; `None` when there is none yet.
    pub ledger_id: Option<String>,
    pub ledger_name: Option<String>,
    /// India, reverse charge on a purchase: this row is the tax the company
    /// owes (an output-tax ledger). Its amount is negative, so it goes on
    /// the supplier's side and does not add to what the supplier is paid;
    /// the matching input-tax row (the credit) is positive.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reverse_charge: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InvoiceTaxDto {
    /// Within the company's region (CGST + SGST) or across (IGST).
    pub place_of_supply: Option<String>,
    pub rows: Vec<InvoiceTaxRow>,
    /// Problems the user should fix (a missing rate or ledger).
    pub warnings: Vec<String>,
}

/// Work out the tax lines for an invoice. Output tax on Sales and Credit
/// Notes, input tax on Purchases and Debit Notes. Amounts are positive;
/// the invoice adds them on the party's side.
///
/// India: a line whose tax class carries the `RCM:` flag
/// (`khata_in::rcm`) is taxed on reverse charge. On a Purchase or Debit
/// Note it gives two rows per tax: the credit on an input-tax ledger
/// (positive) and the tax owed on an output-tax ledger (negative, flagged
/// `reverse_charge`), so the supplier's total is the taxable value. On a
/// Sale nothing is charged (the buyer pays the tax). `BLOCKED:` (s.17(5))
/// posts as usual and warns that GSTR-3B reverses the credit.
pub fn invoice_tax(slot: &Slot, input: &InvoiceTaxInput) -> ApiResult<InvoiceTaxDto> {
    let book = open_ref(slot)?;
    let pack = pack(book).ok_or(NO_COUNTRY)?;
    let profile = book.profile().ok_or(NO_COUNTRY)?;
    let side = match input.kind {
        VoucherKind::Sales | VoucherKind::CreditNote => TaxSide::Output,
        VoucherKind::Purchase | VoucherKind::DebitNote => TaxSide::Input,
        _ => return Err("tax is worked out on Sales, Purchase, Credit Note and Debit Note".into()),
    };
    // A pack without regions (Singapore) taxes the whole country alike: its
    // country code stands in for the region on both sides of the supply.
    let no_regions = pack.regions().is_empty();
    let supplier_region = if no_regions { Some(pack.country().code.to_owned()) } else { profile.tax_region.clone() };
    let supplier_region = supplier_region.ok_or_else(|| {
        format!("set the company's {} and state first (Company → Tax registration)", pack.tax_id_label())
    })?;
    let date = parse_date(&input.date)?;
    let party = book.ledger(LedgerId(parse_uuid(&input.party_id, "party")?)).ok_or("unknown party")?;
    // Where the party is: its registration, else its address; else local.
    let party_region = if no_regions { None } else { party_region(pack, party) };
    let place = party_region.clone().unwrap_or_else(|| supplier_region.clone());
    // US: a sale to a customer holding an exemption certificate for the
    // state is not taxed (the sales tax report counts it exempt too).
    if book.country() == Some("US") && side == TaxSide::Output {
        if let Ok(Some(c)) = crate::tax_us::exempting_certificate(slot, &input.party_id, &place, date) {
            return Ok(InvoiceTaxDto {
                place_of_supply: Some(place),
                rows: Vec::new(),
                warnings: vec![format!("no sales tax: the customer's exemption certificate {} covers {}", c.number, c.state)],
            });
        }
    }
    let (supplier, place) = match side {
        TaxSide::Output => (supplier_region.clone(), place),
        // On a purchase the party supplies to us.
        TaxSide::Input => (place, supplier_region.clone()),
    };
    let mut rates = RateSchedule::default();
    for (class, changes) in &profile.tax_rates {
        for (from, r) in changes {
            rates.set(class, *from, *r).map_err(err)?;
        }
    }
    let mut warnings = Vec::new();
    let india = pack.country().code == "IN";
    // (reverse-charge liability, component, percent) → amount.
    let mut totals: BTreeMap<(bool, String, String), Amount> = BTreeMap::new();
    for (i, l) in input.lines.iter().enumerate() {
        let (class, name) = if !l.ledger_id.trim().is_empty() && l.item_id.trim().is_empty() {
            let ledger = book.ledger(LedgerId(parse_uuid(&l.ledger_id, "ledger")?)).ok_or("unknown ledger")?;
            if ledger.tax_class.is_none() {
                warnings.push(format!("{} has no HSN/SAC code or tax category, so no tax was worked out for it", ledger.name));
            }
            (ledger.tax_class.clone(), ledger.name.clone())
        } else if l.item_id.trim().is_empty() {
            (Some(l.tax_class.clone()).filter(|c| !c.trim().is_empty()), format!("line {}", i + 1))
        } else {
            let item = book.stock_item(StockItemId(parse_uuid(&l.item_id, "stock item")?)).ok_or("unknown stock item")?;
            if item.tax_class.is_none() {
                warnings.push(format!("{} has no HSN/SAC code or tax category, so no tax was worked out for it", item.name));
            }
            (item.tax_class.clone(), item.name.clone())
        };
        let Some(class) = class else { continue };
        let flags = if india {
            khata_in::rcm::parse_class(&class)
        } else {
            khata_in::rcm::ClassFlags { class: &class, reverse_charge: false, blocked: false }
        };
        if flags.reverse_charge && side == TaxSide::Output {
            warnings.push(format!(
                "{name} is on reverse charge: the buyer pays its tax, so none is charged; the invoice must say tax is payable on reverse charge (Rule 46(p))"
            ));
            continue;
        }
        if flags.blocked && side == TaxSide::Input {
            warnings.push(format!(
                "{name}: input tax credit on it is blocked (CGST s.17(5)); GSTR-3B reverses it in 4(B)(1)"
            ));
        }
        let line = TaxLine {
            date,
            tax_class: flags.class.to_owned(),
            taxable_value: parse_amount(&l.taxable_value).map_err(|e| format!("line {}: {e}", i + 1))?,
            supply: Supply {
                supplier_region: supplier.clone(),
                place_of_supply: PlaceOfSupply::Region(place.clone()),
                // Supplies to an SEZ unit are zero-rated inter-region supplies.
                special_zone: side == TaxSide::Output && party.mailing.as_ref().is_some_and(|m| m.special_zone),
                without_payment: false,
            },
        };
        match pack.line_tax(&line, &rates) {
            Ok(t) => {
                for c in t.components {
                    *totals.entry((false, c.tax.to_owned(), amt(c.percent))).or_default() += c.amount;
                    if flags.reverse_charge {
                        *totals.entry((true, c.tax.to_owned(), amt(c.percent))).or_default() -= c.amount;
                    }
                }
            }
            Err(e) => warnings.push(format!("line {}: {e}", i + 1)),
        }
    }
    let mut rows = Vec::new();
    for ((reverse_charge, component, percent), amount) in totals {
        // The tax owed on reverse charge goes to an output-tax ledger.
        let row_side = if reverse_charge { TaxSide::Output } else { side };
        let mut ledgers: Vec<_> = book
            .ledgers()
            .filter(|l| l.tax.as_ref().is_some_and(|t| t.side == row_side && t.component.eq_ignore_ascii_case(&component)))
            .collect();
        ledgers.sort_by_key(|l| l.name.to_lowercase());
        let ledger = ledgers.first();
        if ledger.is_none() {
            let which = if row_side == TaxSide::Output { "output" } else { "input" };
            let why = if reverse_charge { " (the tax owed on reverse charge)" } else { "" };
            warnings.push(format!(
                "no ledger collects {which} {component}{why}: create one under Duties & Taxes and set its type of tax to {component}"
            ));
        }
        rows.push(InvoiceTaxRow {
            component,
            percent,
            amount: amt(amount),
            ledger_id: ledger.map(|l| l.id.0.to_string()),
            ledger_name: ledger.map(|l| l.name.clone()),
            reverse_charge,
        });
    }
    Ok(InvoiceTaxDto { place_of_supply: Some(place), rows, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{group_id, indian_company, ledger};
    use crate::api::{create_party_ledger, open_company};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};

    #[test]
    fn invoice_tax_splits_by_place_of_supply_and_finds_ledgers() {
        let (f, mut slot) = indian_company("tax");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        assert_eq!(company_tax(&slot).unwrap().region.as_deref(), Some("27"));
        assert!(set_company_tax(&mut slot, "27AAPFU0939F1ZV", "29").is_err(), "the GSTIN decides the state");
        set_tax_rate(&mut slot, "8471", "2026-04-01", "18").unwrap();

        let debtors = group_id(&slot, "Sundry Debtors");
        let local = create_party_ledger(&mut slot, "Local Co", &debtors, "", None, Some("27AAPFU0939F1ZV"), None).unwrap().ledger.id;
        let karnataka = create_party_ledger(&mut slot, "Bengaluru Co", &debtors, "", None, Some("29AAGCB7383J1Z4"), None).unwrap().ledger.id;
        let walk_in = ledger(&mut slot, "Walk-in", "Sundry Debtors", "");
        for (name, component) in [("Output CGST", "CGST"), ("Output SGST", "SGST"), ("Output IGST", "IGST")] {
            let id = ledger(&mut slot, name, "Duties & Taxes", "");
            set_ledger_tax(&mut slot, &id, component, TaxSide::Output).unwrap();
        }
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap();
        let laptop = save_stock_item(&mut slot, &StockItemInput {
            id: String::new(),
            name: "Laptop".into(),
            group_id: String::new(),
            unit_id: nos.id,
            opening_qty: String::new(),
            opening_rate: String::new(),
            opening_value: String::new(),
            tax_class: "8471 30 10".into(),
            costing: None,
        })
        .unwrap();
        let ask = |party: &str| InvoiceTaxInput {
            kind: VoucherKind::Sales,
            date: "2026-05-01".into(),
            party_id: party.into(),
            lines: vec![InvoiceTaxLine { item_id: laptop.id.clone(), ledger_id: String::new(), tax_class: String::new(), taxable_value: "50000".into() }],
        };
        let rows = |t: InvoiceTaxDto| t.rows.into_iter().map(|r| (r.component, r.amount, r.ledger_name.unwrap_or_default())).collect::<Vec<_>>();

        let same_state = invoice_tax(&slot, &ask(&local)).unwrap();
        assert!(same_state.warnings.is_empty(), "{:?}", same_state.warnings);
        assert_eq!(rows(same_state), [("CGST".into(), "4500".into(), "Output CGST".into()), ("SGST".into(), "4500".into(), "Output SGST".into())]);
        assert_eq!(rows(invoice_tax(&slot, &ask(&karnataka)).unwrap()), [("IGST".into(), "9000".into(), "Output IGST".into())]);
        // An unregistered buyer is treated as local.
        assert_eq!(invoice_tax(&slot, &ask(&walk_in)).unwrap().rows.len(), 2);
        // …unless its address is in another state.
        let mut input = crate::masters::LedgerInput {
            name: "Walk-in".into(),
            group_id: debtors.clone(),
            opening: String::new(),
            credit_days: None,
            tax_id: None,
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: Some(khata_core::Mailing { region: Some("29".into()), ..Default::default() }),
            tax_class: None,
            pan: None,
            tds: None,
        };
        crate::masters::alter_ledger(&mut slot, &walk_in, &input).unwrap();
        assert_eq!(rows(invoice_tax(&slot, &ask(&walk_in)).unwrap())[0].0, "IGST");
        // An SEZ unit in our own state still gets IGST (inter-State by law).
        input.mailing = Some(khata_core::Mailing { region: Some("27".into()), special_zone: true, ..Default::default() });
        crate::masters::alter_ledger(&mut slot, &walk_in, &input).unwrap();
        assert_eq!(rows(invoice_tax(&slot, &ask(&walk_in)).unwrap())[0].0, "IGST");
        input.mailing = Some(khata_core::Mailing::default());
        crate::masters::alter_ledger(&mut slot, &walk_in, &input).unwrap();
        assert!(crate::masters::get_ledger(&slot, &walk_in).unwrap().mailing.is_none(), "empty details clear");
        // Purchases need input-tax ledgers; their absence is a warning, not an error.
        let purchase = InvoiceTaxInput { kind: VoucherKind::Purchase, ..ask(&karnataka) };
        let t = invoice_tax(&slot, &purchase).unwrap();
        assert!(t.warnings.iter().any(|w| w.contains("input IGST")), "{:?}", t.warnings);

        drop(slot);
        let mut reopened = None;
        open_company(&mut reopened, &f.0).unwrap();
        assert_eq!(list_tax_rates(&reopened).unwrap()[0].class, "8471");
    }

    #[test]
    fn reverse_charge_purchases_owe_and_claim_the_tax() {
        let (_f, mut slot) = indian_company("tax-rcm");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_tax_rate(&mut slot, "9982", "2017-07-01", "18").unwrap();
        let creditors = group_id(&slot, "Sundry Creditors");
        let advocate = ledger(&mut slot, "Advocate", "Sundry Creditors", "");
        let ka = create_party_ledger(&mut slot, "KA Advocate", &creditors, "", None, Some("29AAGCB7383J1Z4"), None).unwrap().ledger.id;
        for (name, component, side) in [
            ("Input CGST", "CGST", TaxSide::Input),
            ("Input SGST", "SGST", TaxSide::Input),
            ("Output CGST", "CGST", TaxSide::Output),
            ("Output SGST", "SGST", TaxSide::Output),
        ] {
            let id = ledger(&mut slot, name, "Duties & Taxes", "");
            set_ledger_tax(&mut slot, &id, component, side).unwrap();
        }
        let ask = |kind, party: &str, class: &str| InvoiceTaxInput {
            kind,
            date: "2026-05-01".into(),
            party_id: party.into(),
            lines: vec![InvoiceTaxLine { item_id: String::new(), ledger_id: String::new(), tax_class: class.into(), taxable_value: "10000".into() }],
        };
        let t = invoice_tax(&slot, &ask(VoucherKind::Purchase, &advocate, "RCM:9982")).unwrap();
        let rows: Vec<_> = t.rows.iter().map(|r| (r.component.as_str(), r.amount.as_str(), r.ledger_name.as_deref().unwrap_or(""), r.reverse_charge)).collect();
        assert_eq!(rows, [
            ("CGST", "900", "Input CGST", false),
            ("SGST", "900", "Input SGST", false),
            ("CGST", "-900", "Output CGST", true),
            ("SGST", "-900", "Output SGST", true),
        ]);
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        // Inter-State: IGST, and no ledger for it yet.
        let t = invoice_tax(&slot, &ask(VoucherKind::Purchase, &ka, "RCM:9982")).unwrap();
        assert!(t.warnings.iter().any(|w| w.contains("output IGST (the tax owed on reverse charge)")), "{:?}", t.warnings);
        // A sale on reverse charge charges nothing.
        let t = invoice_tax(&slot, &ask(VoucherKind::Sales, &advocate, "RCM:9982")).unwrap();
        assert!(t.rows.is_empty() && t.warnings[0].contains("Rule 46(p)"), "{:?}", t);
        // Blocked credit posts as usual, with a warning.
        let t = invoice_tax(&slot, &ask(VoucherKind::Purchase, &ka, "BLOCKED:9982")).unwrap();
        assert_eq!(t.rows.len(), 1);
        assert!(t.warnings.iter().any(|w| w.contains("s.17(5)")), "{:?}", t.warnings);
    }
}

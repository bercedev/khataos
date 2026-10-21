//! Germany: the UStVA and XRechnung invoices from the books. The rules
//! live in `khata_de` (`returns`, `xrechnung`) and the EN 16931 model and
//! writers in `khata_en16931`; this module turns vouchers into their
//! documents.
//!
//! Which vouchers feed which side (KhataOS records a sales return as a
//! Credit Note and a purchase return as a Debit Note):
//! - outward: Sales → invoice, Credit Note → credit note;
//! - inward: Purchase → invoice, Debit Note → the supplier's credit note.
//!
//! Other voucher types are left out (a warning counts those that post to
//! tax ledgers). A voucher's lines split as in `vat_gb.rs`: the party line
//! carries the total; item lines (goods) and other ledgers on the opposite
//! side (services) are the value; ledgers whose type of tax is "USt" give
//! the USt; ledgers on the party's side are discounts; a ledger named
//! "Round off" is left out.
//!
//! Each line's treatment comes from its tax class and the rate on file for
//! the voucher date (`khata_de::ust::treatment`); a line without a class is
//! taken as STANDARD. Where the party is (its mailing country) decides the
//! rest when the voucher has no USt:
//! - a sale to an EU business (country in the EU, not DE, with a VAT id):
//!   goods → intra-EU supply (Kz 41), services → not taxable here (Kz 21);
//! - a sale outside the EU: goods → export (Kz 43), services → Kz 45;
//! - a purchase from an EU supplier: goods → intra-EU acquisition
//!   (Kz 89/93 and 61), services → §13b (Kz 46/47 and 67);
//! - a purchase of services from outside the EU → §13b (Kz 84/85 and 67);
//!   goods from outside the EU (import VAT, Kz 62) are not modelled.
//!
//! Reverse-charge and acquisition tax is self-assessed at the class's
//! rate, or at the STANDARD rate on file when the class is 0% (REVERSE).
//!
//! XRechnung: the seller is the company (its name, USt-IdNr or
//! Steuernummer, and address); contact, e-mail and bank details are read
//! from "Tel.:", "E-Mail:", "Kontakt:", "IBAN:" and "BIC:" lines of the
//! company address until the profile holds them. The buyer is the party
//! ledger (tax id, mailing address, e-mail); the Buyer reference (BT-10)
//! is a "Leitweg-ID: …" or "Buyer reference: …" (also "Käuferreferenz:",
//! "Ihre Referenz:") line of the voucher narration; the other narration
//! lines become invoice notes.

use chrono::Days;
use khata_compliance::RateSchedule;
use khata_core::{Amount, BillKind, Book, DrCr, Ledger, Voucher, VoucherId, VoucherKind};
use khata_de::returns::{self, DocKind, DocLine, InwardDoc, OutwardDoc, Period, Ustva};
use khata_de::ust::{self, Treatment};
use khata_de::xrechnung::{self, Buyer, DeInvoice, DeLine, Seller};
use khata_de::{laender, steuernummer, ustid};
use khata_en16931::{Address, Specification, cii, ubl, validate_for};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, group_is_party, open_ref, parse_date, parse_uuid};

fn germany_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_de::COUNTRY.code) {
        return Err("the UStVA and XRechnung are for companies in Germany".into());
    }
    Ok(())
}

fn rates(book: &Book) -> ApiResult<RateSchedule> {
    let mut rates = RateSchedule::default();
    if let Some(p) = book.profile() {
        for (class, changes) in &p.tax_rates {
            for (from, r) in changes {
                rates.set(class, *from, *r).map_err(err)?;
            }
        }
    }
    Ok(rates)
}

fn label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Sales",
        VoucherKind::Purchase => "Purchase",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        _ => "Voucher",
    }
}

/// The party line: a debtor/creditor, else the cash or bank line on the
/// party's side, else the first line on that side.
fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = match v.kind {
        VoucherKind::Sales | VoucherKind::DebitNote => DrCr::Dr,
        _ => DrCr::Cr,
    };
    let is_party = |e: &khata_core::Entry| book.ledger(e.ledger).is_some_and(|l| group_is_party(book, l.group));
    v.entries
        .iter()
        .position(is_party)
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

/// One voucher line before its treatment.
struct RawLine {
    name: String,
    qty: Decimal,
    price: Decimal,
    unit: String,
    /// Signed: negative when on the party's side.
    amount: Amount,
    class: Option<String>,
    goods: bool,
}

struct Parsed {
    party: Option<usize>,
    lines: Vec<RawLine>,
    discounts: Vec<(String, Amount)>,
    ust: Amount,
    warnings: Vec<String>,
}

fn parse(book: &Book, v: &Voucher) -> Parsed {
    let name = format!("{} {}", label(v.kind), v.number);
    let party = party_line(book, v);
    let party_side = party.map(|i| v.entries[i].side);
    let mut p = Parsed { party, lines: Vec::new(), discounts: Vec::new(), ust: Amount::ZERO, warnings: Vec::new() };
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if t.component.eq_ignore_ascii_case(ust::UST) || t.component.eq_ignore_ascii_case("VAT") {
                p.ust += signed;
            } else {
                p.warnings.push(format!("{name}: {ledger_name} collects {}, which is not USt; it is left out", t.component));
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                p.lines.push(RawLine {
                    name: item.map(|i| i.name.clone()).unwrap_or_default(),
                    qty: l.qty,
                    price: l.rate,
                    unit: item.and_then(|i| book.unit(i.unit)).map(|u| u.symbol.clone()).unwrap_or_default(),
                    amount: if on_party_side { -l.amount } else { l.amount },
                    class: item.and_then(|i| i.tax_class.clone()),
                    goods: true,
                });
            }
        } else if ledger_name.to_lowercase().contains("round") {
            p.warnings.push(format!("{name}: {ledger_name} (rounding) is left out"));
        } else if on_party_side {
            p.discounts.push((ledger_name, e.amount));
        } else {
            p.lines.push(RawLine {
                name: ledger_name,
                qty: Decimal::ONE,
                price: e.amount,
                unit: String::new(),
                amount: e.amount,
                class: ledger.and_then(|l| l.tax_class.clone()),
                goods: false,
            });
        }
    }
    p
}

/// Where a party is: its mailing country, `None` for Germany or unknown.
fn country(ledger: Option<&Ledger>) -> Option<String> {
    ledger
        .and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| !c.is_empty() && !laender::is_germany(c))
}

fn tax_id(ledger: Option<&Ledger>) -> Option<String> {
    ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty())
}

/// A VAT identifier (BT-48), not a Steuernummer.
fn vat_id(id: Option<String>) -> Option<String> {
    let id = id?;
    let n = ustid::normalise(&id);
    (n.len() > 2 && n.as_bytes()[..2].iter().all(u8::is_ascii_uppercase)).then_some(n)
}

/// The treatment and rate of each line, and the USt spread over the taxed
/// lines by value × rate.
fn treat(
    v: &Voucher,
    p: &Parsed,
    outward: bool,
    party: Option<&Ledger>,
    rates: &RateSchedule,
    warnings: &mut Vec<String>,
) -> ApiResult<Vec<(Treatment, Decimal, Decimal)>> {
    let name = format!("{} {}", label(v.kind), v.number);
    let abroad = country(party);
    let eu = abroad.as_deref().is_some_and(laender::in_eu);
    let party_vat = vat_id(tax_id(party)).is_some();
    let standard = rates.rate(ust::STANDARD_CLASS, v.date);
    let mut out = Vec::new();
    for l in &p.lines {
        let class = l.class.clone().unwrap_or_else(|| ust::STANDARD_CLASS.to_owned());
        let rate = rates.rate(&class, v.date).ok_or_else(|| {
            format!("{name}: no USt rate for {class:?} on {}: add it to the company's rate list", v.date)
        })?;
        let mut t = ust::treatment(&class, rate);
        if p.ust.is_zero() && t.taxed() && let Some(c) = abroad.as_deref() {
            t = match (outward, eu, l.goods) {
                (true, true, true) if party_vat => Treatment::IntraEu,
                (true, true, false) if party_vat => Treatment::OutOfScope,
                (true, false, true) => Treatment::Export,
                (true, false, false) => Treatment::OutOfScope,
                (false, true, true) => Treatment::IntraEu,
                (false, _, false) => Treatment::ReverseCharge,
                (false, false, true) => {
                    warnings.push(format!("{name}: goods from {c} (import VAT, Kz 62) are not worked out"));
                    Treatment::OutOfScope
                }
                _ => {
                    warnings.push(format!("{name}: a sale to {c} without the buyer's VAT id and without USt is reported as taxable at {}%", rate.normalize()));
                    t
                }
            };
        }
        let percent = match t {
            Treatment::ReverseCharge | Treatment::IntraEu if !outward => {
                if rate.is_zero() {
                    standard.ok_or_else(|| format!("{name}: no STANDARD rate on {} to self-assess the tax", v.date))?
                } else {
                    rate
                }
            }
            _ => rate,
        };
        out.push((t, percent, Decimal::ZERO));
    }
    // Spread the booked USt over the taxed lines by value.
    let taxed: Vec<usize> = (0..out.len()).filter(|&i| out[i].0.taxed()).collect();
    if !p.ust.is_zero() && taxed.is_empty() {
        warnings.push(format!("{name} has USt but no taxed lines; its USt is put on its lines as STANDARD"));
        for o in &mut out {
            o.0 = Treatment::Standard;
            o.1 = standard.unwrap_or(ust::STANDARD_PERCENT);
        }
        return spread(p, out, (0..p.lines.len()).collect(), warnings, &name);
    }
    spread(p, out, taxed, warnings, &name)
}

fn spread(
    p: &Parsed,
    mut out: Vec<(Treatment, Decimal, Decimal)>,
    taxed: Vec<usize>,
    warnings: &mut Vec<String>,
    name: &str,
) -> ApiResult<Vec<(Treatment, Decimal, Decimal)>> {
    // Weight by value × rate, so that 19% and 7% lines each get their own tax.
    let weights: Vec<Decimal> =
        out.iter().zip(&p.lines).map(|(o, l)| l.amount * if o.1.is_zero() { Decimal::ONE } else { o.1 }).collect();
    let w = |i: usize| weights[i];
    let weight: Decimal = taxed.iter().map(|&i| w(i)).sum();
    let last = taxed.iter().rev().copied().find(|&i| !p.lines[i].amount.is_zero());
    let mut left = p.ust;
    for &i in &taxed {
        let share = match last {
            Some(k) if i == k => left,
            Some(_) if !weight.is_zero() => (p.ust * w(i) / weight).round_dp(2),
            _ => Decimal::ZERO,
        };
        left -= share;
        out[i].2 = share;
    }
    if !taxed.is_empty() {
        let expected: Decimal = taxed.iter().map(|&i| p.lines[i].amount * out[i].1).sum::<Decimal>() / Decimal::ONE_HUNDRED;
        if (expected - p.ust).abs() > Decimal::ONE {
            warnings.push(format!("{name}: USt booked is {}, but the rates on file give {}", p.ust.normalize(), expected.round_dp(2).normalize()));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- UStVA

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UstvaDto {
    /// The company's USt-IdNr or Steuernummer as stored.
    pub tax_id: Option<String>,
    #[serde(flatten)]
    pub ret: Ustva,
}

/// The UStVA Kennzahlen for a period (a month or a quarter).
pub fn ustva(slot: &Slot, from: &str, to: &str) -> ApiResult<UstvaDto> {
    let book = open_ref(slot)?;
    germany_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let rates = rates(book)?;
    let mut warnings = Vec::new();
    let (mut outward, mut inward) = (Vec::new(), Vec::new());
    let mut other_kinds = 0;
    let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| period.contains(v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let is_out = match v.kind {
            VoucherKind::Sales | VoucherKind::CreditNote => true,
            VoucherKind::Purchase | VoucherKind::DebitNote => false,
            _ => {
                let taxed = v.entries.iter().any(|e| book.ledger(e.ledger).is_some_and(|l| l.tax.is_some()));
                other_kinds += usize::from(taxed && !v.cancelled);
                continue;
            }
        };
        let p = parse(book, v);
        warnings.extend(p.warnings.iter().cloned());
        let party = p.party.and_then(|i| book.ledger(v.entries[i].ledger));
        let treated = treat(v, &p, is_out, party, &rates, &mut warnings)?;
        let lines: Vec<DocLine> = p
            .lines
            .iter()
            .zip(treated)
            .map(|(l, (treatment, percent, ust))| DocLine { treatment, goods: l.goods, taxable_value: l.amount, percent, ust })
            .collect();
        let kind = match v.kind {
            VoucherKind::Sales | VoucherKind::Purchase => DocKind::Invoice,
            _ => DocKind::CreditNote,
        };
        let party_name = party.map(|l| l.name.clone()).unwrap_or_default();
        if is_out {
            outward.push(OutwardDoc {
                id: v.id.0.to_string(),
                kind,
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_country: country(party),
                party_vat_id: vat_id(tax_id(party)),
                cancelled: v.cancelled,
                lines,
            });
        } else {
            inward.push(InwardDoc {
                id: v.id.0.to_string(),
                kind,
                number: v.number.clone(),
                date: v.date,
                party_name,
                party_country: country(party),
                party_vat_id: vat_id(tax_id(party)),
                cancelled: v.cancelled,
                lines,
            });
        }
    }
    if other_kinds > 0 {
        warnings.push(format!("{other_kinds} journal or other voucher(s) post to tax ledgers; they are not in this UStVA"));
    }
    let mut ret = returns::ustva(&outward, &inward, period);
    ret.warnings.splice(0..0, warnings);
    Ok(UstvaDto { tax_id: profile.tax_id.clone(), ret })
}

// ------------------------------------------------------------ XRechnung

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct XRechnungDto {
    pub xml: String,
    /// "ubl" or "cii".
    pub syntax: String,
    /// `<number>.xml` with characters unsafe in file names replaced.
    pub file_name: String,
    /// Broken EN 16931 or XRechnung rules ("[BR-DE-15] …"): the file is not
    /// a valid XRechnung until they are fixed.
    pub errors: Vec<String>,
    /// Rules flagged as warnings, and what the books could not say.
    pub warnings: Vec<String>,
}

/// "Key: value" lines of a narration, with the rest as notes.
fn narration_reference(narration: &str) -> (Option<String>, Vec<String>) {
    let mut reference = None;
    let mut notes = Vec::new();
    for line in narration.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_lowercase();
            if matches!(k.as_str(), "leitweg-id" | "leitweg id" | "buyer reference" | "käuferreferenz" | "ihre referenz") && !v.trim().is_empty() {
                reference = Some(v.trim().to_owned());
                continue;
            }
        }
        notes.push(line.to_owned());
    }
    (reference, notes)
}

fn de_invoice(book: &Book, v: &Voucher, warnings: &mut Vec<String>) -> ApiResult<DeInvoice> {
    let credit_note = match v.kind {
        VoucherKind::Sales => false,
        VoucherKind::CreditNote => true,
        _ => return Err("XRechnung invoices are made from Sales and Credit Note vouchers".into()),
    };
    if v.cancelled {
        return Err("a cancelled voucher has no XRechnung".into());
    }
    let profile = book.profile().ok_or("pick the company's country first")?;
    let rates = rates(book)?;
    let p = parse(book, v);
    warnings.extend(p.warnings.iter().cloned());
    let party = p.party.and_then(|i| book.ledger(v.entries[i].ledger)).filter(|l| group_is_party(book, l.group));
    let treated = treat(v, &p, true, party, &rates, warnings)?;
    let mut lines = Vec::new();
    for (i, (l, (treatment, percent, _))) in p.lines.iter().zip(treated).enumerate() {
        if l.amount < Decimal::ZERO {
            return Err(format!("line {} is negative; put discounts on a ledger on the party's side", i + 1));
        }
        lines.push(DeLine {
            name: l.name.clone(),
            quantity: l.qty,
            unit: l.unit.clone(),
            unit_price: l.price,
            net_amount: l.amount,
            treatment,
            percent: if treatment.taxed() { percent } else { Decimal::ZERO },
            exemption_note: None,
            goods: l.goods,
        });
    }

    let parsed = xrechnung::parse_address(&profile.address);
    let company_id = profile.tax_id.clone().filter(|t| !t.trim().is_empty());
    let (seller_vat, steuernummer) = match company_id {
        Some(id) if ustid::validate(&id).is_ok() => (Some(ustid::normalise(&id)), None),
        Some(id) if steuernummer::validate(&id).is_ok() => (None, Some(id)),
        Some(id) => {
            warnings.push(format!("the company's tax id {id:?} is neither a USt-IdNr nor a Steuernummer"));
            (None, None)
        }
        None => (None, None),
    };
    let seller = Seller {
        name: book.name.clone(),
        vat_id: seller_vat,
        steuernummer,
        address: parsed.address,
        contact: parsed.contact,
        email: parsed.email,
        iban: parsed.iban,
        bic: parsed.bic,
        kleinunternehmer: false,
    };
    let (reference, notes) = narration_reference(&v.narration);
    let buyer = match party {
        Some(l) => {
            let m = l.mailing.clone().unwrap_or_default();
            let mut a: Address = xrechnung::parse_address(&m.address).address;
            if !m.postal_code.trim().is_empty() {
                a.postal_code = m.postal_code.trim().to_owned();
            }
            // No "PLZ Ort" line (the post code is its own field, or the
            // address is abroad): the last street line is the city.
            if a.city.is_empty() {
                let last = if a.line3.is_empty() { &mut a.line2 } else { &mut a.line3 };
                a.city = std::mem::take(last);
            }
            a.country = m.country.clone().filter(|c| !c.trim().is_empty()).unwrap_or_else(|| "DE".into()).to_ascii_uppercase();
            Buyer {
                name: l.name.clone(),
                vat_id: vat_id(l.tax_id.clone()),
                address: a,
                email: Some(m.email.trim().to_owned()).filter(|e| !e.is_empty()),
                reference,
            }
        }
        None => {
            warnings.push("the voucher has no party ledger: the buyer has no address".into());
            Buyer { name: "Barverkauf".into(), reference, ..Buyer::default() }
        }
    };
    let party_entry = p.party.map(|i| &v.entries[i]);
    let due = party_entry
        .and_then(|e| e.bills.iter().find_map(|b| b.due))
        .or_else(|| party.and_then(|l| l.credit_days).and_then(|d| v.date.checked_add_days(Days::new(u64::from(d)))));
    let preceding = if credit_note {
        party_entry
            .and_then(|e| e.bills.iter().find(|b| b.kind == BillKind::Against && !b.reference.trim().is_empty()))
            .map(|b| {
                let date = book.vouchers().find(|x| x.kind == VoucherKind::Sales && x.number == b.reference).map(|x| x.date);
                (b.reference.clone(), date)
            })
    } else {
        None
    };
    Ok(DeInvoice {
        number: v.number.clone(),
        date: v.date,
        credit_note,
        supply_date: None,
        due_date: due,
        payment_terms: None,
        preceding,
        seller,
        buyer,
        lines,
        discounts: p.discounts,
        notes,
    })
}

/// The XRechnung XML for a Sales or Credit Note voucher, in UBL ("ubl") or
/// CII ("cii"), with the rules it breaks. Save the XML with `write_gst_json`.
pub fn xrechnung_xml(slot: &Slot, voucher_id: &str, syntax: &str) -> ApiResult<XRechnungDto> {
    let book = open_ref(slot)?;
    germany_only(book)?;
    let syntax = syntax.trim().to_ascii_lowercase();
    if syntax != "ubl" && syntax != "cii" {
        return Err(format!("unknown syntax {syntax:?} (ubl or cii)"));
    }
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    crate::forex::refuse_foreign_einvoice(v, "An XRechnung")?;
    let mut warnings = Vec::new();
    let inv = de_invoice(book, v, &mut warnings)?;
    if inv.supply_date.is_none() {
        warnings.push("the supply date (Leistungsdatum) is taken to be the invoice date".into());
    }
    let (e, w) = xrechnung::to_en16931(&inv);
    warnings.extend(w);
    let booked: Decimal = parse(book, v).ust;
    if (e.totals.vat - booked).abs() > Decimal::new(5, 2) {
        warnings.push(format!("the invoice's USt is {} by its VAT breakdown, but {} is booked", e.totals.vat, booked.round_dp(2)));
    }
    let mut errors = Vec::new();
    for x in validate_for(&e, Specification::XRechnung3) {
        if x.warning { warnings.push(x.to_string()) } else { errors.push(x.to_string()) }
    }
    let xml = if syntax == "ubl" { ubl::write(&e, Specification::XRechnung3) } else { cii::write(&e, Specification::XRechnung3) };
    let safe: String = v.number.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    Ok(XRechnungDto { xml, file_name: format!("XRechnung-{safe}-{syntax}.xml"), syntax, errors, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, indian_company, ledger, voucher};
    use crate::api::{StockLineInput, cancel_voucher, check_tax_id, create_company, post_voucher, set_country};
    use crate::masters::{LedgerInput, StockItemInput, alter_ledger, get_ledger, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax, set_tax_rate};
    use chrono::Datelike;
    use khata_core::{Mailing, TaxSide};
    use rust_decimal_macros::dec;

    fn de_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Muster GmbH").unwrap();
        set_country(&mut slot, Some(&f.0), "DE").unwrap();
        set_company_tax(&mut slot, "DE136695976", "BE").unwrap();
        for (class, from, percent) in [
            ("STANDARD", "2007-01-01", "19"),
            ("REDUCED", "2007-01-01", "7"),
            ("EXEMPT", "2007-01-01", "0"),
            ("REVERSE", "2007-01-01", "0"),
        ] {
            set_tax_rate(&mut slot, class, from, percent).unwrap();
        }
        (f, slot)
    }

    fn items(item_id: &str, qty: &str, rate: &str) -> Vec<StockLineInput> {
        vec![StockLineInput {
            item_id: item_id.into(),
            qty: qty.into(),
            rate: rate.into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }]
    }

    fn item(slot: &mut Slot, name: &str, unit: &str, class: &str) -> String {
        let input: StockItemInput =
            serde_json::from_value(serde_json::json!({ "name": name, "unit_id": unit, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    fn party(slot: &mut Slot, name: &str, group: &str, tax_id: Option<&str>, m: Mailing) -> String {
        let id = ledger(slot, name, group, "");
        let l = get_ledger(slot, &id).unwrap();
        alter_ledger(slot, &id, &LedgerInput {
            name: l.name,
            group_id: group_id(slot, group),
            opening: String::new(),
            credit_days: Some(30),
            tax_id: tax_id.map(str::to_owned),
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: Some(m),
            tax_class: None,
            pan: None,
            tds: None,
        })
        .unwrap();
        id
    }

    struct Books {
        slot: Slot,
        _f: TempFile,
        sale_id: String,
    }

    fn books(name: &str) -> Books {
        let (f, mut slot) = de_company(name);
        let city = |a: &str, plz: &str, c: &str| Mailing {
            address: a.into(),
            postal_code: plz.into(),
            country: Some(c.into()),
            email: "rechnung@kunde.de".into(),
            ..Mailing::default()
        };
        let kunde = party(&mut slot, "Kunde AG", "Sundry Debtors", Some("DE129304291"), city("Marktplatz 5\nMünchen", "80331", "DE"));
        let paris = party(&mut slot, "Paris SARL", "Sundry Debtors", Some("FR40303265045"), city("1 Rue de Rivoli\nParis", "75001", "FR"));
        let zurich = party(&mut slot, "Zürich AG", "Sundry Debtors", None, city("Bahnhofstrasse 1\nZürich", "8001", "CH"));
        let wien = party(&mut slot, "Wien GmbH", "Sundry Creditors", Some("ATU13585627"), city("Ring 1\nWien", "1010", "AT"));
        let lieferant = party(&mut slot, "Lieferant KG", "Sundry Creditors", Some("DE118619592"), city("Weg 2\nHamburg", "20095", "DE"));
        let sales = ledger(&mut slot, "Umsatzerlöse", "Sales Accounts", "");
        let beratung = ledger(&mut slot, "Beratung", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Wareneingang", "Purchase Accounts", "");
        let software = ledger(&mut slot, "Software-Lizenz", "Indirect Expenses", "");
        let output = ledger(&mut slot, "Umsatzsteuer 19%", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Vorsteuer 19%", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "USt", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "USt", TaxSide::Input).unwrap();
        let stk = save_unit(&mut slot, "", "Stk", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &stk, "STANDARD");
        let buch = item(&mut slot, "Fachbuch", &stk, "REDUCED");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str, qty: &str, rate: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, qty, rate);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, narration: &str, entries| {
            let mut v = voucher(kind, date, entries);
            v.narration = narration.into();
            post_voucher(slot, v).unwrap().id
        };
        // A domestic sale at 19% and 7%.
        let mut lines = line(&sales, DrCr::Cr, "2039.90", &laptop, "2", "1000");
        lines.items.extend(items(&buch, "1", "39.90"));
        let sale_id = post(&mut slot, "Sales", "2026-03-02", "Leitweg-ID: 04011000-1234512345-06\nDanke für Ihren Auftrag.", vec![
            entry(&kunde, DrCr::Dr, "2422.69"),
            lines,
            entry(&output, DrCr::Cr, "382.79"),
        ]);
        // A credit note, an intra-EU supply, an export and an EU service.
        post(&mut slot, "CreditNote", "2026-03-10", "", vec![
            entry(&kunde, DrCr::Cr, "1190"),
            line(&sales, DrCr::Dr, "1000", &laptop, "1", "1000"),
            entry(&output, DrCr::Dr, "190"),
        ]);
        post(&mut slot, "Sales", "2026-03-11", "", vec![entry(&paris, DrCr::Dr, "3000"), line(&sales, DrCr::Cr, "3000", &laptop, "3", "1000")]);
        post(&mut slot, "Sales", "2026-03-12", "", vec![entry(&zurich, DrCr::Dr, "1000"), line(&sales, DrCr::Cr, "1000", &laptop, "1", "1000")]);
        post(&mut slot, "Sales", "2026-03-13", "", vec![entry(&paris, DrCr::Dr, "500"), entry(&beratung, DrCr::Cr, "500")]);
        // Purchases: domestic with Vorsteuer, an EU service (§13b), EU goods (acquisition).
        post(&mut slot, "Purchase", "2026-03-05", "", vec![
            entry(&lieferant, DrCr::Cr, "1190"),
            line(&purchase, DrCr::Dr, "1000", &laptop, "1", "1000"),
            entry(&input, DrCr::Dr, "190"),
        ]);
        post(&mut slot, "Purchase", "2026-03-06", "", vec![entry(&wien, DrCr::Cr, "200"), entry(&software, DrCr::Dr, "200")]);
        post(&mut slot, "Purchase", "2026-03-07", "", vec![entry(&wien, DrCr::Cr, "800"), line(&purchase, DrCr::Dr, "800", &laptop, "1", "800")]);
        let cancelled = post(&mut slot, "Sales", "2026-03-20", "", vec![
            entry(&kunde, DrCr::Dr, "119"),
            line(&sales, DrCr::Cr, "100", &laptop, "1", "100"),
            entry(&output, DrCr::Cr, "19"),
        ]);
        cancel_voucher(&mut slot, &cancelled).unwrap();
        set_company_address(
            &mut slot,
            "Hauptstraße 1\n10115 Berlin\nTel.: +49 30 123456\nE-Mail: rechnung@muster.de\nKontakt: Erika Muster\nIBAN: DE02120300000000202051",
        )
        .unwrap();
        Books { slot, _f: f, sale_id }
    }

    #[test]
    fn a_german_company_checks_ids_and_charges_19_percent() {
        let f = TempFile::new("de-ids");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Muster GmbH").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "de").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("DE", "EUR", "USt-IdNr"));
        assert_eq!((info.fy_start.month(), info.fy_start.day()), (1, 1), "calendar years");
        let ok = check_tax_id(&slot, "de 136 695 976").unwrap();
        assert_eq!((ok.value.as_str(), ok.error), ("DE136695976", None));
        assert!(check_tax_id(&slot, "DE136695977").unwrap().error.is_some());
        let st = check_tax_id(&slot, "9198081508152").unwrap();
        assert_eq!((st.error, st.region.as_deref()), (None, Some("Bayern")));
        assert!(set_company_tax(&mut slot, "DE136695976", "XX").is_err(), "the Bundesland must be known");
        let tax = set_company_tax(&mut slot, "DE136695976", "BE").unwrap();
        assert_eq!((tax.label.as_str(), tax.region_name.as_deref(), tax.regions.len()), ("USt-IdNr", Some("Berlin"), 16));
    }

    #[test]
    fn ustva_from_the_books() {
        let b = books("de-ustva");
        let r = ustva(&b.slot, "2026-03-01", "2026-03-31").unwrap();
        assert_eq!(r.tax_id.as_deref(), Some("DE136695976"));
        let k = &r.ret;
        // 81: 2000 − 1000 = 1000 → 190.00; 86: 39.90 → 39 → 2.73.
        assert_eq!((k.kz81, k.kz81_tax, k.kz86, k.kz86_tax), (dec!(1000), dec!(190.00), dec!(39), dec!(2.73)));
        assert_eq!((k.kz41, k.kz43, k.kz21), (dec!(3000), dec!(1000), dec!(500)));
        assert_eq!((k.kz46, k.kz47, k.kz89, k.kz89_tax), (dec!(200), dec!(38.00), dec!(800), dec!(152.00)));
        assert_eq!((k.kz66, k.kz61, k.kz67), (dec!(190.00), dec!(152.00), dec!(38.00)));
        // Output 190 + 2.73 + 152 + 38 = 382.73; input 190 + 152 + 38 = 380.
        assert_eq!((k.output_tax, k.input_tax, k.kz83), (dec!(382.73), dec!(380.00), dec!(2.73)));
        assert!(!k.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", k.warnings);
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["kz83"], "2.73");
        assert!(ustva(&b.slot, "2026-03-31", "2026-03-01").unwrap_err().contains("before it starts"));
    }

    #[test]
    fn xrechnung_from_a_sale_in_both_syntaxes() {
        let b = books("de-xr");
        let x = xrechnung_xml(&b.slot, &b.sale_id, "ubl").unwrap();
        assert!(x.errors.is_empty(), "{:?}", x.errors);
        let number = crate::api::get_voucher(&b.slot, &b.sale_id).unwrap().number;
        assert_eq!(x.file_name, format!("XRechnung-{}-ubl.xml", number.replace('/', "_")), "no slash in the file name");
        for s in [
            "<cbc:CustomizationID>urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0</cbc:CustomizationID>",
            "<cbc:BuyerReference>04011000-1234512345-06</cbc:BuyerReference>",
            "<cbc:CompanyID>DE136695976</cbc:CompanyID>",
            "<cbc:CompanyID>DE129304291</cbc:CompanyID>",
            "<cbc:CityName>München</cbc:CityName>",
            "<cbc:PostalZone>80331</cbc:PostalZone>",
            "<cbc:Note>Danke für Ihren Auftrag.</cbc:Note>",
            "<cbc:InvoicedQuantity unitCode=\"H87\">2</cbc:InvoicedQuantity>",
            "<cbc:TaxAmount currencyID=\"EUR\">382.79</cbc:TaxAmount>",
            "<cbc:PayableAmount currencyID=\"EUR\">2422.69</cbc:PayableAmount>",
            "<cbc:DueDate>2026-04-01</cbc:DueDate>",
        ] {
            assert!(x.xml.contains(s), "missing {s} in\n{}", x.xml);
        }
        let c = xrechnung_xml(&b.slot, &b.sale_id, "CII").unwrap();
        assert!(c.errors.is_empty(), "{:?}", c.errors);
        assert!(c.xml.contains("<ram:GrandTotalAmount>2422.69</ram:GrandTotalAmount>"));
        assert!(c.xml.contains("<ram:IBANID>DE02120300000000202051</ram:IBANID>"));
        assert!(xrechnung_xml(&b.slot, &b.sale_id, "pdf").unwrap_err().contains("ubl or cii"));
    }

    #[test]
    fn germany_only() {
        let (_f, slot) = indian_company("de-india");
        assert!(ustva(&slot, "2026-03-01", "2026-03-31").unwrap_err().contains("Germany"));
    }
}

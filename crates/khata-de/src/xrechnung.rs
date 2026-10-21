//! A German invoice from the books, mapped to the EN 16931 model for
//! XRechnung (UBL or CII) or ZUGFeRD's XML.
//!
//! The input ([`DeInvoice`]) is neutral: the app fills it from a voucher,
//! the company profile and the party ledger. [`to_en16931`] maps it:
//! - treatments to VAT categories: 19%/7% → S, 0% (§12 Abs. 3) → Z,
//!   tax-free (§4) → E, §13b → AE, intra-EU supply → K, export → G; a
//!   service whose place of supply is with an EU business abroad (§3a
//!   Abs. 2) → AE (the buyer owes the tax there), any other supply not
//!   taxable in Germany → O;
//! - the §14 Abs. 4 Nr. 8 and §14a notes: an exemption reason on every
//!   untaxed breakdown (BT-120, with the CEF VATEX code where there is
//!   one), and the same text as an invoice note (BT-22);
//! - a Kleinunternehmer (§19) invoice: every line E with the §19 note;
//! - discounts on the party's side: document allowances (BG-20) in the
//!   category of the first line, with a warning when lines differ;
//! - payment: SEPA credit transfer (58) when the seller has an IBAN, else
//!   "1" (not defined) with a warning.
//!
//! The note texts are KhataOS's wording of the legal requirement (the
//! law asks for a "Hinweis", not fixed words), except the reverse-charge
//! note: "Steuerschuldnerschaft des Leistungsempfängers" is the wording of
//! §14a Abs. 5 UStG, also required for §3a Abs. 2 services to EU
//! businesses (§14a Abs. 1) [PRIMARY, research §3]. The exemption note for
//! §4 supplies should name the provision; the default text does not, and
//! a warning says so.

use chrono::NaiveDate;
use khata_en16931::{
    Address, AllowanceCharge, Contact, CreditTransfer, ElectronicAddress, Invoice, Line, Party, PaymentInstructions,
    PrecedingInvoice, TypeCode, VatBreakdown, VatCategory, units,
};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::laender;
use crate::ust::Treatment;

/// §14a Abs. 5 UStG: the required wording on a §13b invoice.
pub const NOTE_REVERSE_CHARGE: &str = "Steuerschuldnerschaft des Leistungsempfängers";
/// The note on a tax-free intra-EU supply of goods (§4 Nr. 1 Buchst. b,
/// §6a; KhataOS wording).
pub const NOTE_INTRA_EU: &str = "Steuerfreie innergemeinschaftliche Lieferung";
/// An export of goods (§4 Nr. 1 Buchst. a, §6 UStG).
pub const NOTE_EXPORT: &str = "Steuerfreie Ausfuhrlieferung (§ 4 Nr. 1 Buchst. a UStG)";
/// A supply whose place is not in Germany.
pub const NOTE_NOT_TAXABLE: &str = "Im Inland nicht steuerbarer Umsatz";
/// The default note on a §4 tax-free supply (§14 Abs. 4 Nr. 8): name the
/// provision ("§ 4 Nr. 12 UStG") in the class or ledger where possible.
pub const NOTE_EXEMPT: &str = "Steuerfreie Leistung nach § 4 UStG";
/// A Kleinunternehmer (§19 UStG) invoice.
pub const NOTE_KLEINUNTERNEHMER: &str = "Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.";

/// The seller: the company.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Seller {
    pub name: String,
    /// USt-IdNr ("DE…"), when the company has one.
    pub vat_id: Option<String>,
    /// Steuernummer, when it invoices with that instead (or as well).
    pub steuernummer: Option<String>,
    pub address: Address,
    pub contact: Contact,
    /// The electronic address (BT-34): an email address.
    pub email: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    /// The company is a Kleinunternehmer (§19 UStG) and charges no USt.
    pub kleinunternehmer: bool,
}

/// The buyer: the party ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Buyer {
    pub name: String,
    pub vat_id: Option<String>,
    pub address: Address,
    /// The electronic address (BT-49): an email address.
    pub email: Option<String>,
    /// BT-10: the Leitweg-ID of a public buyer, else the buyer's reference.
    pub reference: Option<String>,
}

/// One invoice line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeLine {
    pub name: String,
    pub quantity: Decimal,
    /// The unit symbol as kept in the books ("Stk", "kg", "Std").
    pub unit: String,
    pub unit_price: Decimal,
    /// Net amount (quantity × price), EUR.
    pub net_amount: Decimal,
    pub treatment: Treatment,
    /// The rate charged; 0 on untaxed lines.
    pub percent: Decimal,
    /// The exemption note for an untaxed line, when the class names one.
    pub exemption_note: Option<String>,
    pub goods: bool,
}

/// An invoice or credit note from the books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeInvoice {
    pub number: String,
    pub date: NaiveDate,
    pub credit_note: bool,
    /// Lieferdatum / Leistungsdatum (§14 Abs. 4 Nr. 6); the invoice date
    /// when the books have none.
    pub supply_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub payment_terms: Option<String>,
    /// For a credit note: the invoice it corrects.
    pub preceding: Option<(String, Option<NaiveDate>)>,
    pub seller: Seller,
    pub buyer: Buyer,
    pub lines: Vec<DeLine>,
    /// Discounts (reason, amount) on the party's side.
    pub discounts: Vec<(String, Decimal)>,
    pub notes: Vec<String>,
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// The EN 16931 category, rate and (VATEX code, note) of a line.
fn category(inv: &DeInvoice, l: &DeLine) -> (VatCategory, Decimal, Option<(Option<&'static str>, String)>) {
    if inv.seller.kleinunternehmer {
        return (VatCategory::E, Decimal::ZERO, Some((None, NOTE_KLEINUNTERNEHMER.to_owned())));
    }
    let buyer_country = inv.buyer.address.country.trim();
    let eu_business = !laender::is_germany(buyer_country) && laender::in_eu(buyer_country) && non_empty(&inv.buyer.vat_id).is_some();
    let note = |default: &str| l.exemption_note.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| default.to_owned());
    match l.treatment {
        Treatment::Standard | Treatment::Reduced => (VatCategory::S, l.percent, None),
        Treatment::Zero => (VatCategory::Z, Decimal::ZERO, None),
        Treatment::Exempt => (VatCategory::E, Decimal::ZERO, Some((None, note(NOTE_EXEMPT)))),
        Treatment::ReverseCharge => (VatCategory::AE, Decimal::ZERO, Some((Some("VATEX-EU-AE"), NOTE_REVERSE_CHARGE.to_owned()))),
        Treatment::IntraEu => (VatCategory::K, Decimal::ZERO, Some((Some("VATEX-EU-IC"), note(NOTE_INTRA_EU)))),
        Treatment::Export => (VatCategory::G, Decimal::ZERO, Some((Some("VATEX-EU-G"), note(NOTE_EXPORT)))),
        Treatment::OutOfScope if eu_business && !l.goods => {
            (VatCategory::AE, Decimal::ZERO, Some((Some("VATEX-EU-AE"), NOTE_REVERSE_CHARGE.to_owned())))
        }
        Treatment::OutOfScope => (VatCategory::O, Decimal::ZERO, Some((Some("VATEX-EU-O"), note(NOTE_NOT_TAXABLE)))),
    }
}

/// The EN 16931 invoice for a German invoice, finalised (breakdown and
/// totals worked out), with warnings about what the books could not say.
/// Validate the result with `khata_en16931::validate_for` before writing.
pub fn to_en16931(inv: &DeInvoice) -> (Invoice, Vec<String>) {
    let mut w = Vec::new();
    let type_code = if inv.credit_note { TypeCode::CreditNote } else { TypeCode::Commercial };
    let mut e = Invoice::new(&inv.number, inv.date, type_code, "EUR");
    e.due_date = inv.due_date;
    e.payment_terms = non_empty(&inv.payment_terms);
    e.buyer_reference = non_empty(&inv.buyer.reference);
    if let Some(r) = e.buyer_reference.as_deref().filter(|r| crate::leitweg::looks_like(r))
        && let Err(err) = crate::leitweg::validate(r)
    {
        w.push(format!("the buyer reference {r:?} looks like a Leitweg-ID but {err}"));
    }
    e.delivery_date = Some(inv.supply_date.unwrap_or(inv.date));
    e.preceding = inv.preceding.clone().map(|(number, date)| PrecedingInvoice { number, date });
    e.notes = inv.notes.iter().filter(|n| !n.trim().is_empty()).cloned().collect();

    let s = &inv.seller;
    e.seller = Party {
        name: s.name.clone(),
        vat_id: non_empty(&s.vat_id),
        tax_registration: non_empty(&s.steuernummer),
        electronic_address: non_empty(&s.email).map(|v| ElectronicAddress { value: v, scheme: "EM".into() }),
        address: Address { country: if s.address.country.trim().is_empty() { "DE".into() } else { s.address.country.clone() }, ..s.address.clone() },
        contact: s.contact.clone(),
        ..Party::default()
    };
    let b = &inv.buyer;
    e.buyer = Party {
        name: b.name.clone(),
        vat_id: non_empty(&b.vat_id),
        electronic_address: non_empty(&b.email).map(|v| ElectronicAddress { value: v, scheme: "EM".into() }),
        address: Address { country: if b.address.country.trim().is_empty() { "DE".into() } else { b.address.country.clone() }, ..b.address.clone() },
        ..Party::default()
    };
    e.payment = Some(match non_empty(&s.iban) {
        Some(iban) => PaymentInstructions {
            means_code: "58".into(),
            means_text: None,
            remittance: Some(inv.number.clone()),
            credit_transfer: Some(CreditTransfer { account: iban.replace(' ', ""), name: Some(s.name.clone()), bic: non_empty(&s.bic) }),
        },
        None => {
            w.push("the company has no IBAN on file: the payment means is \"not defined\" (add \"IBAN: …\" to the company address)".into());
            PaymentInstructions { means_code: "1".into(), means_text: None, remittance: Some(inv.number.clone()), credit_transfer: None }
        }
    });

    let mut notes: Vec<String> = Vec::new();
    let mut exemptions: Vec<VatBreakdown> = Vec::new();
    let mut first: Option<(VatCategory, Decimal)> = None;
    for (i, l) in inv.lines.iter().enumerate() {
        let (cat, rate, reason) = category(inv, l);
        first.get_or_insert((cat, rate));
        if let Some((code, text)) = reason {
            if !notes.contains(&text) {
                notes.push(text.clone());
            }
            if !exemptions.iter().any(|x| x.category == cat) {
                exemptions.push(VatBreakdown::exemption(cat, Some(Decimal::ZERO), code, Some(&text)));
            }
            if l.treatment == Treatment::Exempt && l.exemption_note.is_none() && !s.kleinunternehmer {
                w.push(format!("line {}: the tax-free note does not name the provision of § 4 UStG; give the tax class a note", i + 1));
            }
        }
        let unit_code = units::rec20(&l.unit).unwrap_or_else(|| {
            if !l.unit.trim().is_empty() {
                w.push(format!("line {}: unit {:?} has no UN/ECE code; \"C62\" (one) is used", i + 1, l.unit));
            }
            units::ONE
        });
        e.lines.push(Line {
            id: (i + 1).to_string(),
            note: None,
            quantity: l.quantity,
            unit_code: unit_code.into(),
            net_amount: l.net_amount,
            net_price: l.unit_price,
            price_base_quantity: None,
            item_name: l.name.clone(),
            item_description: None,
            seller_item_id: None,
            category: cat,
            rate: if cat == VatCategory::O { None } else { Some(rate) },
        });
    }
    if let Some((cat, rate)) = first {
        for (reason, amount) in &inv.discounts {
            e.allowances_charges.push(AllowanceCharge {
                charge: false,
                amount: *amount,
                base: None,
                percent: None,
                category: cat,
                rate: if cat == VatCategory::O { None } else { Some(rate) },
                reason: Some(if reason.trim().is_empty() { "Rabatt".into() } else { reason.clone() }),
                reason_code: None,
            });
        }
        let mixed = e.lines.iter().any(|l| (l.category, l.rate) != (cat, Some(rate)) && l.category != VatCategory::O);
        if !inv.discounts.is_empty() && mixed {
            w.push(format!("discounts are put on the {} {}% lines; lines at other rates are not reduced", cat.code(), rate.normalize()));
        }
    }
    for n in notes {
        if !e.notes.contains(&n) {
            e.notes.push(n);
        }
    }
    if e.lines.iter().any(|l| l.category == VatCategory::K) {
        e.deliver_to_country = Some(e.buyer.address.country.clone());
        if inv.supply_date.is_none() {
            w.push("an intra-EU supply needs its delivery date; the invoice date is used".into());
        }
    }
    e.vat_breakdown = exemptions;
    e.finalise();
    (e, w)
}

/// Contact and bank details picked out of an address written as lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedAddress {
    pub address: Address,
    pub contact: Contact,
    pub email: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
}

/// Read a German address block: street lines, a "PLZ Ort" line (five
/// digits and the city), and optional "Tel.:", "E-Mail:", "Kontakt:" /
/// "Ansprechpartner:", "IBAN:" and "BIC:" lines (in German or English),
/// until the company profile holds these fields. A last line naming
/// "Deutschland"/"Germany" is dropped (the country is DE).
pub fn parse_address(text: &str) -> ParsedAddress {
    let mut p = ParsedAddress::default();
    let mut street: Vec<String> = Vec::new();
    for raw in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some((key, value)) = raw.split_once(':') {
            let k: String = key.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
            let v = value.trim().to_owned();
            let matched = match k.as_str() {
                "tel" | "telefon" | "phone" | "fon" => {
                    p.contact.phone = v;
                    true
                }
                "email" | "mail" => {
                    p.contact.email = v.clone();
                    p.email = Some(v);
                    true
                }
                "kontakt" | "ansprechpartner" | "contact" => {
                    p.contact.name = v;
                    true
                }
                "iban" => {
                    p.iban = Some(v.replace(' ', ""));
                    true
                }
                "bic" | "swift" => {
                    p.bic = Some(v);
                    true
                }
                _ => false,
            };
            if matched {
                continue;
            }
        }
        let lower = raw.to_lowercase();
        if lower == "deutschland" || lower == "germany" {
            continue;
        }
        let (head, rest) = raw.split_at(raw.find(char::is_whitespace).unwrap_or(raw.len()));
        let head = head.strip_prefix("D-").unwrap_or(head);
        if p.address.postal_code.is_empty() && head.len() == 5 && head.bytes().all(|b| b.is_ascii_digit()) && !rest.trim().is_empty() {
            p.address.postal_code = head.to_owned();
            p.address.city = rest.trim().to_owned();
            continue;
        }
        street.push(raw.to_owned());
    }
    let mut it = street.into_iter();
    p.address.line1 = it.next().unwrap_or_default();
    p.address.line2 = it.next().unwrap_or_default();
    p.address.line3 = it.collect::<Vec<_>>().join(", ");
    p.address.country = "DE".into();
    if p.contact.email.is_empty() && p.email.is_none() {
        p.email = None;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use khata_en16931::{Specification, cii, ubl, validate_for};
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn line(name: &str, qty: Decimal, price: Decimal, treatment: Treatment, percent: Decimal) -> DeLine {
        DeLine {
            name: name.into(),
            quantity: qty,
            unit: "Stk".into(),
            unit_price: price,
            net_amount: qty * price,
            treatment,
            percent,
            exemption_note: None,
            goods: true,
        }
    }

    fn invoice() -> DeInvoice {
        let seller = parse_address(
            "Hauptstraße 1\nHinterhaus\n10115 Berlin\nDeutschland\nTel.: +49 30 123456\nE-Mail: rechnung@muster.de\nKontakt: Erika Muster\nIBAN: DE02 1203 0000 0000 2020 51\nBIC: BYLADEM1001",
        );
        DeInvoice {
            number: "RE-2026-7".into(),
            date: d(2026, 3, 2),
            credit_note: false,
            supply_date: Some(d(2026, 2, 27)),
            due_date: Some(d(2026, 4, 1)),
            payment_terms: Some("Zahlbar innerhalb von 30 Tagen.".into()),
            preceding: None,
            seller: Seller {
                name: "Muster GmbH".into(),
                vat_id: Some("DE136695976".into()),
                steuernummer: None,
                address: seller.address,
                contact: seller.contact,
                email: seller.email,
                iban: seller.iban,
                bic: seller.bic,
                kleinunternehmer: false,
            },
            buyer: Buyer {
                name: "Stadt Musterhausen".into(),
                vat_id: None,
                address: Address { line1: "Rathausplatz 1".into(), city: "Köln".into(), postal_code: "50667".into(), country: "DE".into(), ..Address::default() },
                email: Some("rechnung@musterhausen.de".into()),
                reference: Some("04011000-1234512345-06".into()),
            },
            lines: vec![
                line("Beratung", dec!(4), dec!(120), Treatment::Standard, dec!(19)),
                line("Fachbuch", dec!(1), dec!(39.90), Treatment::Reduced, dec!(7)),
            ],
            discounts: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[test]
    fn reads_a_german_address_block() {
        let p = parse_address("Hauptstraße 1\nHinterhaus\nD-10115 Berlin\nDeutschland\nTel.: +49 30 123456\nE-Mail: a@b.de\nIBAN: DE02 1203 0000 0000 2020 51");
        assert_eq!((p.address.line1.as_str(), p.address.line2.as_str(), p.address.postal_code.as_str(), p.address.city.as_str()), (
            "Hauptstraße 1",
            "Hinterhaus",
            "10115",
            "Berlin"
        ));
        assert_eq!((p.contact.phone.as_str(), p.email.as_deref(), p.iban.as_deref()), ("+49 30 123456", Some("a@b.de"), Some("DE02120300000000202051")));
        assert_eq!(p.address.country, "DE");
        assert_eq!(parse_address("").email, None);
    }

    #[test]
    fn a_domestic_invoice_is_a_valid_xrechnung() {
        let (e, w) = to_en16931(&invoice());
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(validate_for(&e, Specification::XRechnung3), [], "{:#?}", validate_for(&e, Specification::XRechnung3));
        assert_eq!((e.totals.without_vat, e.totals.vat, e.totals.due), (dec!(519.90), dec!(93.99), dec!(613.89)));
        let p = e.payment.as_ref().unwrap();
        assert_eq!((p.means_code.as_str(), p.credit_transfer.as_ref().unwrap().account.as_str()), ("58", "DE02120300000000202051"));
        let xml = ubl::write(&e, Specification::XRechnung3);
        assert!(xml.contains("<cbc:BuyerReference>04011000-1234512345-06</cbc:BuyerReference>"));
        assert!(xml.contains("<cbc:ActualDeliveryDate>2026-02-27</cbc:ActualDeliveryDate>"));
        let xml = cii::write(&e, Specification::XRechnung3);
        assert!(xml.contains("<ram:IBANID>DE02120300000000202051</ram:IBANID>"));
        assert!(xml.contains("<ram:BICID>BYLADEM1001</ram:BICID>"));
    }

    #[test]
    fn reverse_charge_and_intra_eu_carry_the_legal_notes() {
        let mut inv = invoice();
        inv.buyer.vat_id = Some("DE811569869".into());
        inv.lines = vec![line("Bauleistung", dec!(1), dec!(5000), Treatment::ReverseCharge, dec!(0))];
        let (e, _) = to_en16931(&inv);
        assert_eq!(e.vat_breakdown[0].category, VatCategory::AE);
        assert_eq!(e.vat_breakdown[0].exemption_reason.as_deref(), Some(NOTE_REVERSE_CHARGE));
        assert!(e.notes.iter().any(|n| n == NOTE_REVERSE_CHARGE));
        assert_eq!(validate_for(&e, Specification::XRechnung3), []);

        let mut inv = invoice();
        inv.buyer.vat_id = Some("FR40303265045".into());
        inv.buyer.address.country = "FR".into();
        inv.lines = vec![line("Maschine", dec!(1), dec!(9000), Treatment::IntraEu, dec!(0))];
        let (e, _) = to_en16931(&inv);
        assert_eq!((e.lines[0].category, e.deliver_to_country.as_deref()), (VatCategory::K, Some("FR")));
        assert_eq!(e.vat_breakdown[0].exemption_code.as_deref(), Some("VATEX-EU-IC"));
        assert_eq!(validate_for(&e, Specification::XRechnung3), []);

        // A service to an EU business is reverse-charged in its country.
        let mut svc = line("Software-Wartung", dec!(1), dec!(800), Treatment::OutOfScope, dec!(0));
        svc.goods = false;
        inv.lines = vec![svc];
        let (e, _) = to_en16931(&inv);
        assert_eq!(e.lines[0].category, VatCategory::AE);
    }

    #[test]
    fn kleinunternehmer_exempt_and_discounts() {
        let mut inv = invoice();
        inv.seller.kleinunternehmer = true;
        inv.seller.vat_id = None;
        inv.seller.steuernummer = Some("1197081508154".into());
        let (e, w) = to_en16931(&inv);
        assert!(w.is_empty(), "{w:?}");
        assert!(e.lines.iter().all(|l| l.category == VatCategory::E));
        assert_eq!(e.vat_breakdown.len(), 1);
        assert_eq!(e.vat_breakdown[0].exemption_reason.as_deref(), Some(NOTE_KLEINUNTERNEHMER));
        assert_eq!((e.totals.vat, e.totals.due), (dec!(0.00), dec!(519.90)));
        // A Steuernummer (BT-32) satisfies BR-E-02 and BR-DE-16, but BR-CO-26 asks for BT-29, BT-30 or BT-31.
        let rules: Vec<_> = validate_for(&e, Specification::XRechnung3).iter().map(|v| v.rule).collect();
        assert_eq!(rules, ["BR-CO-26"]);

        let mut inv = invoice();
        inv.lines.push(line("Miete", dec!(1), dec!(100), Treatment::Exempt, dec!(0)));
        inv.discounts.push((String::new(), dec!(20)));
        let (e, w) = to_en16931(&inv);
        assert!(w.iter().any(|x| x.contains("§ 4")), "{w:?}");
        assert!(w.iter().any(|x| x.contains("discounts are put on the S 19% lines")), "{w:?}");
        assert_eq!(e.allowances_charges[0].reason.as_deref(), Some("Rabatt"));
        assert_eq!((e.totals.allowances, e.totals.without_vat), (dec!(20.00), dec!(599.90)));
        assert_eq!(validate_for(&e, Specification::XRechnung3), []);
    }

    #[test]
    fn credit_notes_reference_the_invoice_and_missing_iban_is_warned() {
        let mut inv = invoice();
        inv.buyer.reference = Some("04011000-1234512345-07".into());
        assert!(to_en16931(&inv).1.iter().any(|w| w.contains("Leitweg-ID")));
        inv.credit_note = true;
        inv.buyer.reference = Some("PO 4711".into());
        inv.preceding = Some(("RE-2026-6".into(), Some(d(2026, 2, 1))));
        inv.seller.iban = None;
        inv.lines[0].unit = "Sack".into();
        let (e, w) = to_en16931(&inv);
        assert_eq!(e.type_code, TypeCode::CreditNote);
        assert_eq!(e.preceding.as_ref().map(|p| p.number.as_str()), Some("RE-2026-6"));
        assert!(w.iter().any(|x| x.contains("IBAN")) && w.iter().any(|x| x.contains("C62")), "{w:?}");
        assert_eq!(e.payment.as_ref().map(|p| p.means_code.as_str()), Some("1"));
        assert_eq!(validate_for(&e, Specification::XRechnung3), []);
        assert!(ubl::write(&e, Specification::XRechnung3).contains("<CreditNote "));
    }
}

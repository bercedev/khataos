//! OASIS UBL 2.1 syntax: `Invoice` (type codes other than 381) or
//! `CreditNote` (381), in the element order of the UBL 2.1 schemas and with
//! the EN 16931 UBL binding of the CEN/TC 434 artefacts
//! (`ubl/schematron/UBL/EN16931-UBL-model.sch`, example
//! `ubl-tc434-creditnote1.xml`) [PRIMARY].
//!
//! Binding notes:
//! - BT-31 is `PartyTaxScheme/CompanyID` with tax scheme `VAT`; BT-32 (a
//!   national tax number) is a second `PartyTaxScheme` whose scheme is
//!   "FC" (as in KoSIT's XRechnung examples [SECONDARY]).
//! - BT-9 is `cbc:DueDate` on an Invoice and `PaymentMeans/PaymentDueDate`
//!   on a CreditNote (UBL's CreditNote has no DueDate).
//! - Amounts carry `currencyID` = BT-5. Category O writes no `Percent`,
//!   except on the VAT breakdown of an XRechnung (BR-DE-14: 0).

use rust_decimal::Decimal;

use crate::Specification;
use crate::model::{Invoice, Party, VatBreakdown, VatCategory};
use crate::xml::{Xml, amount, number, some};

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

fn tax_category(x: &mut Xml, tag: &str, category: VatCategory, rate: Option<Decimal>, exemption: Option<(Option<&str>, Option<&str>)>) {
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
    x.text("cbc:ID", "VAT");
    x.close("cac:TaxScheme");
    x.close(tag);
}

/// BT-119 as written: XRechnung wants a rate on every breakdown (BR-DE-14),
/// so an O breakdown gets 0 there (as in the CEN artefacts' XRechnung-O
/// example); elsewhere O has none (BR-O-05 ff.).
pub(crate) fn breakdown_rate(b: &VatBreakdown, spec: Specification) -> Option<Decimal> {
    match (b.category, b.rate) {
        (VatCategory::O, None) if spec == Specification::XRechnung3 => Some(Decimal::ZERO),
        (_, r) => r,
    }
}

fn party(x: &mut Xml, wrapper: &str, p: &Party, seller: bool) {
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
    let a = &p.address;
    x.open("cac:PostalAddress", &[]);
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
    x.open("cac:Country", &[]);
    x.text("cbc:IdentificationCode", a.country.trim());
    x.close("cac:Country");
    x.close("cac:PostalAddress");
    let mut scheme = |id: Option<&str>, scheme: &str| {
        if let Some(id) = id.and_then(some) {
            x.open("cac:PartyTaxScheme", &[]);
            x.text("cbc:CompanyID", id);
            x.open("cac:TaxScheme", &[]);
            x.text("cbc:ID", scheme);
            x.close("cac:TaxScheme");
            x.close("cac:PartyTaxScheme");
        }
    };
    scheme(p.vat_id.as_deref(), "VAT");
    if seller {
        scheme(p.tax_registration.as_deref(), "FC");
    }
    x.open("cac:PartyLegalEntity", &[]);
    x.text("cbc:RegistrationName", p.name.trim());
    x.opt("cbc:CompanyID", p.legal_id.as_deref());
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

/// The invoice in UBL 2.1, claiming `spec` (BT-24).
pub fn write(inv: &Invoice, spec: Specification) -> String {
    let credit = inv.type_code.is_credit_note();
    let (root, ns, line_tag, qty_tag) = if credit {
        ("CreditNote", NS_CREDIT_NOTE, "cac:CreditNoteLine", "cbc:CreditedQuantity")
    } else {
        ("Invoice", NS_INVOICE, "cac:InvoiceLine", "cbc:InvoicedQuantity")
    };
    let cur = inv.currency.trim();
    let mut x = Xml::new();
    x.open(root, &[("xmlns", ns), ("xmlns:cac", NS_CAC), ("xmlns:cbc", NS_CBC)]);
    x.text("cbc:CustomizationID", spec.customization_id());
    x.opt("cbc:ProfileID", spec.profile_id());
    x.text("cbc:ID", inv.number.trim());
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
    for n in &inv.notes {
        x.opt("cbc:Note", Some(n));
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
    party(&mut x, "cac:AccountingSupplierParty", &inv.seller, true);
    party(&mut x, "cac:AccountingCustomerParty", &inv.buyer, false);
    let country = inv.deliver_to_country.as_deref().and_then(some);
    if inv.delivery_date.is_some() || country.is_some() {
        x.open("cac:Delivery", &[]);
        if let Some(d) = inv.delivery_date {
            x.text("cbc:ActualDeliveryDate", &date(d));
        }
        if let Some(c) = country {
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
    } else if credit && let Some(d) = inv.due_date {
        // BT-9 on a CreditNote needs a PaymentMeans to live in.
        x.open("cac:PaymentMeans", &[]);
        x.text("cbc:PaymentMeansCode", "1");
        x.text("cbc:PaymentDueDate", &date(d));
        x.close("cac:PaymentMeans");
    }
    if let Some(t) = inv.payment_terms.as_deref().and_then(some) {
        x.open("cac:PaymentTerms", &[]);
        x.text("cbc:Note", t);
        x.close("cac:PaymentTerms");
    }
    for a in &inv.allowances_charges {
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
        tax_category(&mut x, "cac:TaxCategory", a.category, a.rate, None);
        x.close("cac:AllowanceCharge");
    }
    x.open("cac:TaxTotal", &[]);
    money(&mut x, "cbc:TaxAmount", cur, inv.totals.vat);
    for b in &inv.vat_breakdown {
        x.open("cac:TaxSubtotal", &[]);
        money(&mut x, "cbc:TaxableAmount", cur, b.taxable);
        money(&mut x, "cbc:TaxAmount", cur, b.tax);
        let rate = breakdown_rate(b, spec);
        tax_category(&mut x, "cac:TaxCategory", b.category, rate, Some((b.exemption_code.as_deref(), b.exemption_reason.as_deref())));
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
    for l in &inv.lines {
        x.open(line_tag, &[]);
        x.text("cbc:ID", l.id.trim());
        x.opt("cbc:Note", l.note.as_deref());
        x.leaf(qty_tag, &[("unitCode", l.unit_code.trim())], &number(l.quantity));
        money(&mut x, "cbc:LineExtensionAmount", cur, l.net_amount);
        x.open("cac:Item", &[]);
        x.opt("cbc:Description", l.item_description.as_deref());
        x.text("cbc:Name", l.item_name.trim());
        if let Some(id) = l.seller_item_id.as_deref().and_then(some) {
            x.open("cac:SellersItemIdentification", &[]);
            x.text("cbc:ID", id);
            x.close("cac:SellersItemIdentification");
        }
        tax_category(&mut x, "cac:ClassifiedTaxCategory", l.category, l.rate, None);
        x.close("cac:Item");
        x.open("cac:Price", &[]);
        x.leaf("cbc:PriceAmount", &[("currencyID", cur)], &number(l.net_price));
        if let Some(b) = l.price_base_quantity {
            x.leaf("cbc:BaseQuantity", &[("unitCode", l.unit_code.trim())], &number(b));
        }
        x.close("cac:Price");
        x.close(line_tag);
    }
    x.close(root);
    x.finish()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{PrecedingInvoice, TypeCode};
    use crate::rules::tests::{d, sample};

    pub(crate) fn well_formed(xml: &str) {
        let mut r = quick_xml::Reader::from_str(xml);
        let mut depth = 0i32;
        loop {
            match r.read_event().expect("well-formed XML") {
                quick_xml::events::Event::Start(_) => depth += 1,
                quick_xml::events::Event::End(_) => depth -= 1,
                quick_xml::events::Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(depth, 0);
    }

    #[test]
    fn xrechnung_invoice_golden_snippets() {
        let xml = write(&sample(), Specification::XRechnung3);
        well_formed(&xml);
        for snippet in [
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Invoice xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\" xmlns:cac=\"urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2\" xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\">\n  <cbc:CustomizationID>urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0</cbc:CustomizationID>\n  <cbc:ProfileID>urn:fdc:peppol.eu:2017:poacc:billing:01:1.0</cbc:ProfileID>\n  <cbc:ID>RE-2026-001</cbc:ID>\n  <cbc:IssueDate>2026-03-02</cbc:IssueDate>\n  <cbc:DueDate>2026-04-01</cbc:DueDate>\n  <cbc:InvoiceTypeCode>380</cbc:InvoiceTypeCode>\n  <cbc:Note>Vielen Dank für Ihren Auftrag &amp; Ihr Vertrauen.</cbc:Note>\n  <cbc:DocumentCurrencyCode>EUR</cbc:DocumentCurrencyCode>\n  <cbc:BuyerReference>04011000-12345-34</cbc:BuyerReference>\n",
            "      <cbc:EndpointID schemeID=\"EM\">rechnung@muster.de</cbc:EndpointID>\n      <cac:PostalAddress>\n        <cbc:StreetName>Hauptstraße 1</cbc:StreetName>\n        <cbc:CityName>Berlin</cbc:CityName>\n        <cbc:PostalZone>10115</cbc:PostalZone>\n        <cac:Country>\n          <cbc:IdentificationCode>DE</cbc:IdentificationCode>\n        </cac:Country>\n      </cac:PostalAddress>\n      <cac:PartyTaxScheme>\n        <cbc:CompanyID>DE136695976</cbc:CompanyID>\n        <cac:TaxScheme>\n          <cbc:ID>VAT</cbc:ID>\n        </cac:TaxScheme>\n      </cac:PartyTaxScheme>\n      <cac:PartyTaxScheme>\n        <cbc:CompanyID>2893081508152</cbc:CompanyID>\n        <cac:TaxScheme>\n          <cbc:ID>FC</cbc:ID>\n",
            "  <cac:PaymentMeans>\n    <cbc:PaymentMeansCode>58</cbc:PaymentMeansCode>\n    <cbc:PaymentID>RE-2026-001</cbc:PaymentID>\n    <cac:PayeeFinancialAccount>\n      <cbc:ID>DE02120300000000202051</cbc:ID>\n      <cbc:Name>Muster GmbH</cbc:Name>\n    </cac:PayeeFinancialAccount>\n  </cac:PaymentMeans>\n  <cac:PaymentTerms>\n    <cbc:Note>Zahlbar innerhalb von 30 Tagen ohne Abzug.</cbc:Note>\n  </cac:PaymentTerms>\n",
            "  <cac:TaxTotal>\n    <cbc:TaxAmount currencyID=\"EUR\">21.12</cbc:TaxAmount>\n    <cac:TaxSubtotal>\n      <cbc:TaxableAmount currencyID=\"EUR\">100.00</cbc:TaxableAmount>\n      <cbc:TaxAmount currencyID=\"EUR\">19.00</cbc:TaxAmount>\n      <cac:TaxCategory>\n        <cbc:ID>S</cbc:ID>\n        <cbc:Percent>19</cbc:Percent>\n        <cac:TaxScheme>\n          <cbc:ID>VAT</cbc:ID>\n        </cac:TaxScheme>\n      </cac:TaxCategory>\n    </cac:TaxSubtotal>\n",
            "  <cac:LegalMonetaryTotal>\n    <cbc:LineExtensionAmount currencyID=\"EUR\">130.30</cbc:LineExtensionAmount>\n    <cbc:TaxExclusiveAmount currencyID=\"EUR\">130.30</cbc:TaxExclusiveAmount>\n    <cbc:TaxInclusiveAmount currencyID=\"EUR\">151.42</cbc:TaxInclusiveAmount>\n    <cbc:PayableAmount currencyID=\"EUR\">151.42</cbc:PayableAmount>\n  </cac:LegalMonetaryTotal>\n",
            "  <cac:InvoiceLine>\n    <cbc:ID>2</cbc:ID>\n    <cbc:InvoicedQuantity unitCode=\"H87\">3</cbc:InvoicedQuantity>\n    <cbc:LineExtensionAmount currencyID=\"EUR\">30.30</cbc:LineExtensionAmount>\n    <cac:Item>\n      <cbc:Name>Artikel 2</cbc:Name>\n      <cac:ClassifiedTaxCategory>\n        <cbc:ID>S</cbc:ID>\n        <cbc:Percent>7</cbc:Percent>\n        <cac:TaxScheme>\n          <cbc:ID>VAT</cbc:ID>\n        </cac:TaxScheme>\n      </cac:ClassifiedTaxCategory>\n    </cac:Item>\n    <cac:Price>\n      <cbc:PriceAmount currencyID=\"EUR\">10.1</cbc:PriceAmount>\n    </cac:Price>\n  </cac:InvoiceLine>\n</Invoice>\n",
        ] {
            assert!(xml.contains(snippet), "missing:\n{snippet}\nin:\n{xml}");
        }
        assert!(!xml.contains("cbc:AllowanceTotalAmount"), "zero sums are left out");
    }

    #[test]
    fn credit_note_with_reference_and_exemption() {
        let mut inv = sample();
        inv.type_code = TypeCode::CreditNote;
        inv.preceding = Some(PrecedingInvoice { number: "RE-2026-001".into(), date: Some(d(2026, 3, 2)) });
        inv.lines.truncate(1);
        inv.lines[0].category = VatCategory::AE;
        inv.lines[0].rate = Some(Decimal::ZERO);
        inv.notes = vec!["Steuerschuldnerschaft des Leistungsempfängers".into()];
        inv.finalise();
        let xml = write(&inv, Specification::En16931);
        well_formed(&xml);
        assert!(xml.contains("<CreditNote xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2\""));
        assert!(xml.contains("  <cbc:CustomizationID>urn:cen.eu:en16931:2017</cbc:CustomizationID>\n  <cbc:ID>"), "no ProfileID for plain EN 16931");
        assert!(xml.contains("  <cbc:IssueDate>2026-03-02</cbc:IssueDate>\n  <cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>\n"));
        assert!(!xml.contains("<cbc:DueDate>"), "a CreditNote has no DueDate");
        assert!(xml.contains("    <cbc:PaymentMeansCode>58</cbc:PaymentMeansCode>\n    <cbc:PaymentDueDate>2026-04-01</cbc:PaymentDueDate>\n"));
        assert!(xml.contains("  <cac:BillingReference>\n    <cac:InvoiceDocumentReference>\n      <cbc:ID>RE-2026-001</cbc:ID>\n      <cbc:IssueDate>2026-03-02</cbc:IssueDate>\n"));
        assert!(xml.contains("        <cbc:ID>AE</cbc:ID>\n        <cbc:Percent>0</cbc:Percent>\n        <cbc:TaxExemptionReasonCode>VATEX-EU-AE</cbc:TaxExemptionReasonCode>\n        <cbc:TaxExemptionReason>Reverse charge</cbc:TaxExemptionReason>\n"));
        assert!(xml.contains("<cac:CreditNoteLine>\n    <cbc:ID>1</cbc:ID>\n    <cbc:CreditedQuantity unitCode=\"H87\">2</cbc:CreditedQuantity>"));
        assert!(xml.contains("<cbc:ActualDeliveryDate>2026-02-27</cbc:ActualDeliveryDate>"));
    }

    #[test]
    fn out_of_scope_lines_have_no_percent() {
        let mut inv = sample();
        inv.lines.truncate(1);
        inv.lines[0].category = VatCategory::O;
        inv.lines[0].rate = None;
        inv.finalise();
        let xml = write(&inv, Specification::PeppolBis3);
        well_formed(&xml);
        assert!(xml.contains("<cbc:ID>O</cbc:ID>\n        <cac:TaxScheme>"), "{xml}");
        assert!(!xml.contains("<cbc:ID>O</cbc:ID>\n        <cbc:Percent>"), "{xml}");
        assert!(xml.contains("billing:3.0</cbc:CustomizationID>"));
        let xml = write(&inv, Specification::XRechnung3);
        assert!(xml.contains("<cbc:ID>O</cbc:ID>\n        <cbc:Percent>0</cbc:Percent>\n        <cbc:TaxExemptionReasonCode>VATEX-EU-O"), "BR-DE-14: {xml}");
        assert!(xml.contains("<cbc:ID>O</cbc:ID>\n        <cac:TaxScheme>"), "no rate on the line: {xml}");
    }
}

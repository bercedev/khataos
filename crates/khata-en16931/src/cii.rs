//! UN/CEFACT Cross Industry Invoice (CII) D16B syntax, in the element
//! order of the D16B subset schemas shipped with the CEN/TC 434 artefacts
//! (`cii/schema/D16B SCRDM (Subset)`), with their EN 16931 binding
//! (`cii/schematron`, example `XRechnung-O.xml`) [PRIMARY].
//!
//! The same structure is the XML inside a ZUGFeRD / Factur-X PDF (profile
//! EN 16931 / XRECHNUNG); [`crate::facturx`] embeds it in a PDF/A-3.
//!
//! Binding notes: BT-31 is `SpecifiedTaxRegistration/ID@schemeID="VA"`,
//! BT-32 the same with `schemeID="FC"`; dates are `DateTimeString`
//! format 102 (`YYYYMMDD`); BT-110 carries `currencyID`; BT-84 is `IBANID`
//! when it looks like an IBAN, else `ProprietaryID`.

use rust_decimal::Decimal;

use crate::Specification;
use crate::model::{Invoice, Party, VatCategory};
use crate::xml::{Xml, amount, number, some};

const NS_RSM: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
const NS_RAM: &str = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";
const NS_QDT: &str = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100";
const NS_UDT: &str = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100";

fn date(x: &mut Xml, tag: &str, d: chrono::NaiveDate, ns: &str) {
    x.open(tag, &[]);
    x.leaf(&format!("{ns}:DateTimeString"), &[("format", "102")], &d.format("%Y%m%d").to_string());
    x.close(tag);
}

fn looks_like_iban(s: &str) -> bool {
    let b = s.trim().as_bytes();
    b.len() >= 15 && b[0].is_ascii_alphabetic() && b[1].is_ascii_alphabetic() && b[2].is_ascii_digit() && b[3].is_ascii_digit()
}

fn party(x: &mut Xml, tag: &str, p: &Party, seller: bool) {
    x.open(tag, &[]);
    x.text("ram:Name", p.name.trim());
    let legal = p.legal_id.as_deref().and_then(some);
    let trading = p.trading_name.as_deref().and_then(some);
    if legal.is_some() || trading.is_some() {
        x.open("ram:SpecifiedLegalOrganization", &[]);
        x.opt("ram:ID", legal);
        x.opt("ram:TradingBusinessName", trading);
        x.close("ram:SpecifiedLegalOrganization");
    }
    let c = &p.contact;
    if !c.is_empty() {
        x.open("ram:DefinedTradeContact", &[]);
        x.opt("ram:PersonName", some(&c.name));
        if let Some(phone) = some(&c.phone) {
            x.open("ram:TelephoneUniversalCommunication", &[]);
            x.text("ram:CompleteNumber", phone);
            x.close("ram:TelephoneUniversalCommunication");
        }
        if let Some(email) = some(&c.email) {
            x.open("ram:EmailURIUniversalCommunication", &[]);
            x.text("ram:URIID", email);
            x.close("ram:EmailURIUniversalCommunication");
        }
        x.close("ram:DefinedTradeContact");
    }
    let a = &p.address;
    x.open("ram:PostalTradeAddress", &[]);
    x.opt("ram:PostcodeCode", some(&a.postal_code));
    x.opt("ram:LineOne", some(&a.line1));
    x.opt("ram:LineTwo", some(&a.line2));
    x.opt("ram:LineThree", some(&a.line3));
    x.opt("ram:CityName", some(&a.city));
    x.text("ram:CountryID", a.country.trim());
    x.opt("ram:CountrySubDivisionName", some(&a.subdivision));
    x.close("ram:PostalTradeAddress");
    if let Some(e) = p.electronic_address.as_ref().filter(|e| some(&e.value).is_some()) {
        x.open("ram:URIUniversalCommunication", &[]);
        x.leaf("ram:URIID", &[("schemeID", e.scheme.trim())], e.value.trim());
        x.close("ram:URIUniversalCommunication");
    }
    let mut reg = |id: Option<&str>, scheme: &str| {
        if let Some(id) = id.and_then(some) {
            x.open("ram:SpecifiedTaxRegistration", &[]);
            x.leaf("ram:ID", &[("schemeID", scheme)], id);
            x.close("ram:SpecifiedTaxRegistration");
        }
    };
    reg(p.vat_id.as_deref(), "VA");
    if seller {
        reg(p.tax_registration.as_deref(), "FC");
    }
    x.close(tag);
}

fn line_tax(x: &mut Xml, tag: &str, category: VatCategory, rate: Option<Decimal>) {
    x.open(tag, &[]);
    x.text("ram:TypeCode", "VAT");
    x.text("ram:CategoryCode", category.code());
    if let Some(r) = rate {
        x.text("ram:RateApplicablePercent", &number(r));
    }
    x.close(tag);
}

/// The invoice in CII D16B, claiming `spec` (BT-24).
pub fn write(inv: &Invoice, spec: Specification) -> String {
    let cur = inv.currency.trim();
    let mut x = Xml::new();
    x.open("rsm:CrossIndustryInvoice", &[("xmlns:rsm", NS_RSM), ("xmlns:ram", NS_RAM), ("xmlns:qdt", NS_QDT), ("xmlns:udt", NS_UDT)]);

    x.open("rsm:ExchangedDocumentContext", &[]);
    if let Some(p) = spec.profile_id() {
        x.open("ram:BusinessProcessSpecifiedDocumentContextParameter", &[]);
        x.text("ram:ID", p);
        x.close("ram:BusinessProcessSpecifiedDocumentContextParameter");
    }
    x.open("ram:GuidelineSpecifiedDocumentContextParameter", &[]);
    x.text("ram:ID", spec.customization_id());
    x.close("ram:GuidelineSpecifiedDocumentContextParameter");
    x.close("rsm:ExchangedDocumentContext");

    x.open("rsm:ExchangedDocument", &[]);
    x.text("ram:ID", inv.number.trim());
    x.text("ram:TypeCode", inv.type_code.code());
    date(&mut x, "ram:IssueDateTime", inv.issue_date, "udt");
    for n in inv.notes.iter().filter_map(|n| some(n)) {
        x.open("ram:IncludedNote", &[]);
        x.text("ram:Content", n);
        x.close("ram:IncludedNote");
    }
    x.close("rsm:ExchangedDocument");

    x.open("rsm:SupplyChainTradeTransaction", &[]);
    for l in &inv.lines {
        x.open("ram:IncludedSupplyChainTradeLineItem", &[]);
        x.open("ram:AssociatedDocumentLineDocument", &[]);
        x.text("ram:LineID", l.id.trim());
        if let Some(n) = l.note.as_deref().and_then(some) {
            x.open("ram:IncludedNote", &[]);
            x.text("ram:Content", n);
            x.close("ram:IncludedNote");
        }
        x.close("ram:AssociatedDocumentLineDocument");
        x.open("ram:SpecifiedTradeProduct", &[]);
        x.opt("ram:SellerAssignedID", l.seller_item_id.as_deref());
        x.text("ram:Name", l.item_name.trim());
        x.opt("ram:Description", l.item_description.as_deref());
        x.close("ram:SpecifiedTradeProduct");
        x.open("ram:SpecifiedLineTradeAgreement", &[]);
        x.open("ram:NetPriceProductTradePrice", &[]);
        x.text("ram:ChargeAmount", &number(l.net_price));
        if let Some(b) = l.price_base_quantity {
            x.leaf("ram:BasisQuantity", &[("unitCode", l.unit_code.trim())], &number(b));
        }
        x.close("ram:NetPriceProductTradePrice");
        x.close("ram:SpecifiedLineTradeAgreement");
        x.open("ram:SpecifiedLineTradeDelivery", &[]);
        x.leaf("ram:BilledQuantity", &[("unitCode", l.unit_code.trim())], &number(l.quantity));
        x.close("ram:SpecifiedLineTradeDelivery");
        x.open("ram:SpecifiedLineTradeSettlement", &[]);
        line_tax(&mut x, "ram:ApplicableTradeTax", l.category, l.rate);
        x.open("ram:SpecifiedTradeSettlementLineMonetarySummation", &[]);
        x.text("ram:LineTotalAmount", &amount(l.net_amount));
        x.close("ram:SpecifiedTradeSettlementLineMonetarySummation");
        x.close("ram:SpecifiedLineTradeSettlement");
        x.close("ram:IncludedSupplyChainTradeLineItem");
    }

    x.open("ram:ApplicableHeaderTradeAgreement", &[]);
    x.opt("ram:BuyerReference", inv.buyer_reference.as_deref());
    party(&mut x, "ram:SellerTradeParty", &inv.seller, true);
    party(&mut x, "ram:BuyerTradeParty", &inv.buyer, false);
    if let Some(o) = inv.order_reference.as_deref().and_then(some) {
        x.open("ram:BuyerOrderReferencedDocument", &[]);
        x.text("ram:IssuerAssignedID", o);
        x.close("ram:BuyerOrderReferencedDocument");
    }
    x.close("ram:ApplicableHeaderTradeAgreement");

    let country = inv.deliver_to_country.as_deref().and_then(some);
    if inv.delivery_date.is_none() && country.is_none() {
        x.empty("ram:ApplicableHeaderTradeDelivery");
    } else {
        x.open("ram:ApplicableHeaderTradeDelivery", &[]);
        if let Some(c) = country {
            x.open("ram:ShipToTradeParty", &[]);
            x.open("ram:PostalTradeAddress", &[]);
            x.text("ram:CountryID", c);
            x.close("ram:PostalTradeAddress");
            x.close("ram:ShipToTradeParty");
        }
        if let Some(d) = inv.delivery_date {
            x.open("ram:ActualDeliverySupplyChainEvent", &[]);
            date(&mut x, "ram:OccurrenceDateTime", d, "udt");
            x.close("ram:ActualDeliverySupplyChainEvent");
        }
        x.close("ram:ApplicableHeaderTradeDelivery");
    }

    x.open("ram:ApplicableHeaderTradeSettlement", &[]);
    if let Some(p) = &inv.payment {
        x.opt("ram:PaymentReference", p.remittance.as_deref());
    }
    x.text("ram:InvoiceCurrencyCode", cur);
    if let Some(p) = &inv.payment {
        x.open("ram:SpecifiedTradeSettlementPaymentMeans", &[]);
        x.text("ram:TypeCode", p.means_code.trim());
        x.opt("ram:Information", p.means_text.as_deref());
        if let Some(ct) = &p.credit_transfer {
            x.open("ram:PayeePartyCreditorFinancialAccount", &[]);
            let tag = if looks_like_iban(&ct.account) { "ram:IBANID" } else { "ram:ProprietaryID" };
            x.text(tag, ct.account.trim());
            x.opt("ram:AccountName", ct.name.as_deref());
            x.close("ram:PayeePartyCreditorFinancialAccount");
            if let Some(bic) = ct.bic.as_deref().and_then(some) {
                x.open("ram:PayeeSpecifiedCreditorFinancialInstitution", &[]);
                x.text("ram:BICID", bic);
                x.close("ram:PayeeSpecifiedCreditorFinancialInstitution");
            }
        }
        x.close("ram:SpecifiedTradeSettlementPaymentMeans");
    }
    for (i, b) in inv.vat_breakdown.iter().enumerate() {
        x.open("ram:ApplicableTradeTax", &[]);
        x.text("ram:CalculatedAmount", &amount(b.tax));
        x.text("ram:TypeCode", "VAT");
        x.opt("ram:ExemptionReason", b.exemption_reason.as_deref());
        x.text("ram:BasisAmount", &amount(b.taxable));
        x.text("ram:CategoryCode", b.category.code());
        x.opt("ram:ExemptionReasonCode", b.exemption_code.as_deref());
        // BT-7 belongs to the document; CII carries it in the first VAT breakdown.
        if i == 0
            && let Some(d) = inv.tax_point_date
        {
            x.open("ram:TaxPointDate", &[]);
            x.leaf("udt:DateString", &[("format", "102")], &d.format("%Y%m%d").to_string());
            x.close("ram:TaxPointDate");
        }
        if let Some(r) = crate::ubl::breakdown_rate(b, spec) {
            x.text("ram:RateApplicablePercent", &number(r));
        }
        x.close("ram:ApplicableTradeTax");
    }
    if let Some((start, end)) = inv.period {
        x.open("ram:BillingSpecifiedPeriod", &[]);
        if let Some(s) = start {
            date(&mut x, "ram:StartDateTime", s, "udt");
        }
        if let Some(e) = end {
            date(&mut x, "ram:EndDateTime", e, "udt");
        }
        x.close("ram:BillingSpecifiedPeriod");
    }
    for a in &inv.allowances_charges {
        x.open("ram:SpecifiedTradeAllowanceCharge", &[]);
        x.open("ram:ChargeIndicator", &[]);
        x.text("udt:Indicator", if a.charge { "true" } else { "false" });
        x.close("ram:ChargeIndicator");
        if let Some(p) = a.percent {
            x.text("ram:CalculationPercent", &number(p));
        }
        if let Some(b) = a.base {
            x.text("ram:BasisAmount", &amount(b));
        }
        x.text("ram:ActualAmount", &amount(a.amount));
        x.opt("ram:ReasonCode", a.reason_code.as_deref());
        x.opt("ram:Reason", a.reason.as_deref());
        line_tax(&mut x, "ram:CategoryTradeTax", a.category, a.rate);
        x.close("ram:SpecifiedTradeAllowanceCharge");
    }
    let terms = inv.payment_terms.as_deref().and_then(some);
    if terms.is_some() || inv.due_date.is_some() {
        x.open("ram:SpecifiedTradePaymentTerms", &[]);
        x.opt("ram:Description", terms);
        if let Some(d) = inv.due_date {
            date(&mut x, "ram:DueDateDateTime", d, "udt");
        }
        x.close("ram:SpecifiedTradePaymentTerms");
    }
    let t = &inv.totals;
    x.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation", &[]);
    x.text("ram:LineTotalAmount", &amount(t.line_net));
    if !t.charges.is_zero() {
        x.text("ram:ChargeTotalAmount", &amount(t.charges));
    }
    if !t.allowances.is_zero() {
        x.text("ram:AllowanceTotalAmount", &amount(t.allowances));
    }
    x.text("ram:TaxBasisTotalAmount", &amount(t.without_vat));
    x.leaf("ram:TaxTotalAmount", &[("currencyID", cur)], &amount(t.vat));
    if !t.rounding.is_zero() {
        x.text("ram:RoundingAmount", &amount(t.rounding));
    }
    x.text("ram:GrandTotalAmount", &amount(t.with_vat));
    if !t.paid.is_zero() {
        x.text("ram:TotalPrepaidAmount", &amount(t.paid));
    }
    x.text("ram:DuePayableAmount", &amount(t.due));
    x.close("ram:SpecifiedTradeSettlementHeaderMonetarySummation");
    if let Some(p) = &inv.preceding {
        x.open("ram:InvoiceReferencedDocument", &[]);
        x.text("ram:IssuerAssignedID", p.number.trim());
        if let Some(d) = p.date {
            date(&mut x, "ram:FormattedIssueDateTime", d, "qdt");
        }
        x.close("ram:InvoiceReferencedDocument");
    }
    x.close("ram:ApplicableHeaderTradeSettlement");
    x.close("rsm:SupplyChainTradeTransaction");
    x.close("rsm:CrossIndustryInvoice");
    x.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AllowanceCharge, PrecedingInvoice, TypeCode};
    use crate::rules::tests::{d, sample};
    use crate::ubl::tests::well_formed;
    use rust_decimal_macros::dec;

    #[test]
    fn xrechnung_cii_golden_snippets() {
        let xml = write(&sample(), Specification::XRechnung3);
        well_formed(&xml);
        for snippet in [
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rsm:CrossIndustryInvoice xmlns:rsm=\"urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100\" xmlns:ram=\"urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100\" xmlns:qdt=\"urn:un:unece:uncefact:data:standard:QualifiedDataType:100\" xmlns:udt=\"urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100\">\n  <rsm:ExchangedDocumentContext>\n    <ram:BusinessProcessSpecifiedDocumentContextParameter>\n      <ram:ID>urn:fdc:peppol.eu:2017:poacc:billing:01:1.0</ram:ID>\n    </ram:BusinessProcessSpecifiedDocumentContextParameter>\n    <ram:GuidelineSpecifiedDocumentContextParameter>\n      <ram:ID>urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0</ram:ID>\n    </ram:GuidelineSpecifiedDocumentContextParameter>\n  </rsm:ExchangedDocumentContext>\n  <rsm:ExchangedDocument>\n    <ram:ID>RE-2026-001</ram:ID>\n    <ram:TypeCode>380</ram:TypeCode>\n    <ram:IssueDateTime>\n      <udt:DateTimeString format=\"102\">20260302</udt:DateTimeString>\n    </ram:IssueDateTime>\n",
            "    <ram:IncludedSupplyChainTradeLineItem>\n      <ram:AssociatedDocumentLineDocument>\n        <ram:LineID>1</ram:LineID>\n      </ram:AssociatedDocumentLineDocument>\n      <ram:SpecifiedTradeProduct>\n        <ram:Name>Artikel 1</ram:Name>\n      </ram:SpecifiedTradeProduct>\n      <ram:SpecifiedLineTradeAgreement>\n        <ram:NetPriceProductTradePrice>\n          <ram:ChargeAmount>50</ram:ChargeAmount>\n        </ram:NetPriceProductTradePrice>\n      </ram:SpecifiedLineTradeAgreement>\n      <ram:SpecifiedLineTradeDelivery>\n        <ram:BilledQuantity unitCode=\"H87\">2</ram:BilledQuantity>\n      </ram:SpecifiedLineTradeDelivery>\n      <ram:SpecifiedLineTradeSettlement>\n        <ram:ApplicableTradeTax>\n          <ram:TypeCode>VAT</ram:TypeCode>\n          <ram:CategoryCode>S</ram:CategoryCode>\n          <ram:RateApplicablePercent>19</ram:RateApplicablePercent>\n        </ram:ApplicableTradeTax>\n        <ram:SpecifiedTradeSettlementLineMonetarySummation>\n          <ram:LineTotalAmount>100.00</ram:LineTotalAmount>\n",
            "      <ram:BuyerReference>04011000-12345-34</ram:BuyerReference>\n      <ram:SellerTradeParty>\n        <ram:Name>Muster GmbH</ram:Name>\n        <ram:DefinedTradeContact>\n          <ram:PersonName>Erika Muster</ram:PersonName>\n          <ram:TelephoneUniversalCommunication>\n            <ram:CompleteNumber>+49 30 123456</ram:CompleteNumber>\n          </ram:TelephoneUniversalCommunication>\n          <ram:EmailURIUniversalCommunication>\n            <ram:URIID>erika@muster.de</ram:URIID>\n          </ram:EmailURIUniversalCommunication>\n        </ram:DefinedTradeContact>\n        <ram:PostalTradeAddress>\n          <ram:PostcodeCode>10115</ram:PostcodeCode>\n          <ram:LineOne>Hauptstraße 1</ram:LineOne>\n          <ram:CityName>Berlin</ram:CityName>\n          <ram:CountryID>DE</ram:CountryID>\n        </ram:PostalTradeAddress>\n        <ram:URIUniversalCommunication>\n          <ram:URIID schemeID=\"EM\">rechnung@muster.de</ram:URIID>\n        </ram:URIUniversalCommunication>\n        <ram:SpecifiedTaxRegistration>\n          <ram:ID schemeID=\"VA\">DE136695976</ram:ID>\n        </ram:SpecifiedTaxRegistration>\n        <ram:SpecifiedTaxRegistration>\n          <ram:ID schemeID=\"FC\">2893081508152</ram:ID>\n        </ram:SpecifiedTaxRegistration>\n      </ram:SellerTradeParty>\n",
            "      <ram:ActualDeliverySupplyChainEvent>\n        <ram:OccurrenceDateTime>\n          <udt:DateTimeString format=\"102\">20260227</udt:DateTimeString>\n",
            "      <ram:PaymentReference>RE-2026-001</ram:PaymentReference>\n      <ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>\n      <ram:SpecifiedTradeSettlementPaymentMeans>\n        <ram:TypeCode>58</ram:TypeCode>\n        <ram:PayeePartyCreditorFinancialAccount>\n          <ram:IBANID>DE02120300000000202051</ram:IBANID>\n          <ram:AccountName>Muster GmbH</ram:AccountName>\n        </ram:PayeePartyCreditorFinancialAccount>\n      </ram:SpecifiedTradeSettlementPaymentMeans>\n      <ram:ApplicableTradeTax>\n        <ram:CalculatedAmount>19.00</ram:CalculatedAmount>\n        <ram:TypeCode>VAT</ram:TypeCode>\n        <ram:BasisAmount>100.00</ram:BasisAmount>\n        <ram:CategoryCode>S</ram:CategoryCode>\n        <ram:RateApplicablePercent>19</ram:RateApplicablePercent>\n      </ram:ApplicableTradeTax>\n",
            "      <ram:SpecifiedTradePaymentTerms>\n        <ram:Description>Zahlbar innerhalb von 30 Tagen ohne Abzug.</ram:Description>\n        <ram:DueDateDateTime>\n          <udt:DateTimeString format=\"102\">20260401</udt:DateTimeString>\n        </ram:DueDateDateTime>\n      </ram:SpecifiedTradePaymentTerms>\n      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>\n        <ram:LineTotalAmount>130.30</ram:LineTotalAmount>\n        <ram:TaxBasisTotalAmount>130.30</ram:TaxBasisTotalAmount>\n        <ram:TaxTotalAmount currencyID=\"EUR\">21.12</ram:TaxTotalAmount>\n        <ram:GrandTotalAmount>151.42</ram:GrandTotalAmount>\n        <ram:DuePayableAmount>151.42</ram:DuePayableAmount>\n      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>\n    </ram:ApplicableHeaderTradeSettlement>\n  </rsm:SupplyChainTradeTransaction>\n</rsm:CrossIndustryInvoice>\n",
        ] {
            assert!(xml.contains(snippet), "missing:\n{snippet}\nin:\n{xml}");
        }
    }

    #[test]
    fn credit_note_allowance_and_exemption_in_cii() {
        let mut inv = sample();
        inv.type_code = TypeCode::CreditNote;
        inv.preceding = Some(PrecedingInvoice { number: "RE-2026-001".into(), date: Some(d(2026, 3, 2)) });
        inv.lines[1].category = VatCategory::K;
        inv.lines[1].rate = Some(Decimal::ZERO);
        inv.deliver_to_country = Some("FR".into());
        inv.allowances_charges.push(AllowanceCharge {
            charge: false,
            amount: dec!(10),
            base: Some(dec!(100)),
            percent: Some(dec!(10)),
            category: VatCategory::S,
            rate: Some(dec!(19)),
            reason: Some("Rabatt".into()),
            reason_code: Some("95".into()),
        });
        inv.finalise();
        let xml = write(&inv, Specification::En16931);
        well_formed(&xml);
        assert!(!xml.contains("BusinessProcessSpecifiedDocumentContextParameter"));
        assert!(xml.contains("<ram:TypeCode>381</ram:TypeCode>"));
        assert!(xml.contains("        <ram:CalculatedAmount>0.00</ram:CalculatedAmount>\n        <ram:TypeCode>VAT</ram:TypeCode>\n        <ram:ExemptionReason>Intra-community supply</ram:ExemptionReason>\n        <ram:BasisAmount>30.30</ram:BasisAmount>\n        <ram:CategoryCode>K</ram:CategoryCode>\n        <ram:ExemptionReasonCode>VATEX-EU-IC</ram:ExemptionReasonCode>\n        <ram:RateApplicablePercent>0</ram:RateApplicablePercent>\n"));
        assert!(xml.contains("      <ram:SpecifiedTradeAllowanceCharge>\n        <ram:ChargeIndicator>\n          <udt:Indicator>false</udt:Indicator>\n        </ram:ChargeIndicator>\n        <ram:CalculationPercent>10</ram:CalculationPercent>\n        <ram:BasisAmount>100.00</ram:BasisAmount>\n        <ram:ActualAmount>10.00</ram:ActualAmount>\n        <ram:ReasonCode>95</ram:ReasonCode>\n        <ram:Reason>Rabatt</ram:Reason>\n        <ram:CategoryTradeTax>\n"));
        assert!(xml.contains("        <ram:AllowanceTotalAmount>10.00</ram:AllowanceTotalAmount>\n        <ram:TaxBasisTotalAmount>120.30</ram:TaxBasisTotalAmount>\n        <ram:TaxTotalAmount currencyID=\"EUR\">17.10</ram:TaxTotalAmount>\n"));
        assert!(xml.contains("      <ram:InvoiceReferencedDocument>\n        <ram:IssuerAssignedID>RE-2026-001</ram:IssuerAssignedID>\n        <ram:FormattedIssueDateTime>\n          <qdt:DateTimeString format=\"102\">20260302</qdt:DateTimeString>\n"));
        assert!(xml.contains("<ram:ShipToTradeParty>\n        <ram:PostalTradeAddress>\n          <ram:CountryID>FR</ram:CountryID>"));
        assert_eq!(crate::rules::validate(&inv), [], "{:?}", crate::rules::validate(&inv));
    }
}

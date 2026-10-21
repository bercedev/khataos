//! The EN 16931 semantic invoice model. Field comments give the business
//! term (BT-n) or group (BG-n) each field carries. Amounts are in the
//! invoice currency (BT-5); EN 16931 allows at most two decimals on every
//! amount except prices (BR-DEC-*), which [`Invoice::finalise`] respects.
//!
//! Only what KhataOS fills is modelled: no payee (BG-10), tax
//! representative (BG-11), line allowances and charges (BG-27/28), item
//! attributes (BG-32), attachments (BG-24) or accounting-currency VAT
//! (BT-6/BT-111).

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

/// BT-3, Invoice type code (UNTDID 1001). EN 16931 allows a longer list
/// (BR-CL-01); these are the ones KhataOS issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum TypeCode {
    /// 380, Commercial invoice.
    Commercial,
    /// 381, Credit note. Written as a UBL `CreditNote`.
    CreditNote,
    /// 384, Corrected invoice.
    Corrected,
    /// 389, Self-billed invoice.
    SelfBilled,
    /// 326, Partial invoice.
    Partial,
}

impl TypeCode {
    pub fn code(self) -> &'static str {
        match self {
            TypeCode::Commercial => "380",
            TypeCode::CreditNote => "381",
            TypeCode::Corrected => "384",
            TypeCode::SelfBilled => "389",
            TypeCode::Partial => "326",
        }
    }

    /// Whether UBL writes it as a `CreditNote` document (UBL's credit-note
    /// type codes, BR-CL-01).
    pub fn is_credit_note(self) -> bool {
        self == TypeCode::CreditNote
    }
}

/// BT-118 / BT-151 / BT-95 / BT-102: VAT category code (the UNTDID 5305
/// subset EN 16931 allows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum VatCategory {
    /// S, Standard rate (any positive rate, including reduced rates).
    S,
    /// Z, Zero rated goods.
    Z,
    /// E, Exempt from tax.
    E,
    /// AE, VAT reverse charge.
    AE,
    /// K, VAT exempt for EEA intra-community supply of goods and services.
    K,
    /// G, Free export item, tax not charged (export outside the EU).
    G,
    /// O, Services outside scope of tax (not subject to VAT).
    O,
    /// L, Canary Islands general indirect tax (IGIC).
    L,
    /// M, Tax for production, services and importation in Ceuta and Melilla (IPSI).
    M,
}

impl VatCategory {
    pub fn code(self) -> &'static str {
        match self {
            VatCategory::S => "S",
            VatCategory::Z => "Z",
            VatCategory::E => "E",
            VatCategory::AE => "AE",
            VatCategory::K => "K",
            VatCategory::G => "G",
            VatCategory::O => "O",
            VatCategory::L => "L",
            VatCategory::M => "M",
        }
    }

    /// The name the business rules use ("Standard rated", "Reverse charge"…).
    pub fn name(self) -> &'static str {
        match self {
            VatCategory::S => "Standard rated",
            VatCategory::Z => "Zero rated",
            VatCategory::E => "Exempt from VAT",
            VatCategory::AE => "Reverse charge",
            VatCategory::K => "Intra-community supply",
            VatCategory::G => "Export outside the EU",
            VatCategory::O => "Not subject to VAT",
            VatCategory::L => "IGIC",
            VatCategory::M => "IPSI",
        }
    }

    /// Categories whose rate is part of the breakdown key (BR-S-08 and the
    /// IGIC/IPSI equivalents): one breakdown per rate. The others have one
    /// breakdown per category at 0% (or no rate, for O).
    pub fn per_rate(self) -> bool {
        matches!(self, VatCategory::S | VatCategory::L | VatCategory::M)
    }

    /// Categories that carry no VAT amount.
    pub fn zero_tax(self) -> bool {
        !self.per_rate()
    }

    /// The CEF VATEX code and EN text a breakdown of this category carries
    /// when nothing more specific is given (BR-AE-10, BR-IC-10, BR-G-10,
    /// BR-O-10). Exempt (E) has no generic code: its reason must be given.
    pub fn default_exemption(self) -> Option<(&'static str, &'static str)> {
        match self {
            VatCategory::AE => Some(("VATEX-EU-AE", "Reverse charge")),
            VatCategory::K => Some(("VATEX-EU-IC", "Intra-community supply")),
            VatCategory::G => Some(("VATEX-EU-G", "Export outside the EU")),
            VatCategory::O => Some(("VATEX-EU-O", "Not subject to VAT")),
            _ => None,
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Some(match code.trim().to_ascii_uppercase().as_str() {
            "S" => VatCategory::S,
            "Z" => VatCategory::Z,
            "E" => VatCategory::E,
            "AE" => VatCategory::AE,
            "K" => VatCategory::K,
            "G" => VatCategory::G,
            "O" => VatCategory::O,
            "L" => VatCategory::L,
            "M" => VatCategory::M,
            _ => return None,
        })
    }
}

/// BG-5 / BG-8: a postal address.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Address {
    /// BT-35 / BT-50, Address line 1 (street).
    pub line1: String,
    /// BT-36 / BT-51, Address line 2.
    pub line2: String,
    /// BT-162 / BT-163, Address line 3.
    pub line3: String,
    /// BT-37 / BT-52, City.
    pub city: String,
    /// BT-38 / BT-53, Post code.
    pub postal_code: String,
    /// BT-39 / BT-54, Country subdivision.
    pub subdivision: String,
    /// BT-40 / BT-55, Country code (ISO 3166-1 alpha-2).
    pub country: String,
}

/// BG-6 / BG-9: a contact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Contact {
    /// BT-41 / BT-56, Contact point.
    pub name: String,
    /// BT-42 / BT-57, Telephone number.
    pub phone: String,
    /// BT-43 / BT-58, Email address.
    pub email: String,
}

impl Contact {
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty() && self.phone.trim().is_empty() && self.email.trim().is_empty()
    }
}

/// BT-34 / BT-49: an electronic address and its scheme (CEF EAS code list;
/// "EM" is an email address, "9930" a German VAT number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElectronicAddress {
    pub value: String,
    pub scheme: String,
}

/// BG-4 Seller / BG-7 Buyer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Party {
    /// BT-27 / BT-44, (legal) name.
    pub name: String,
    /// BT-28 / BT-45, Trading name.
    pub trading_name: Option<String>,
    /// BT-30 / BT-47, Legal registration identifier (commercial register…).
    pub legal_id: Option<String>,
    /// BT-31 / BT-48, VAT identifier, with its country prefix (BR-CO-09).
    pub vat_id: Option<String>,
    /// BT-32, Seller tax registration identifier (a national tax number,
    /// such as a German Steuernummer). Seller only.
    pub tax_registration: Option<String>,
    /// BT-34 / BT-49, Electronic address.
    pub electronic_address: Option<ElectronicAddress>,
    /// BG-5 / BG-8.
    pub address: Address,
    /// BG-6 / BG-9.
    pub contact: Contact,
}

/// BG-17: credit transfer account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreditTransfer {
    /// BT-84, Payment account identifier (an IBAN for SEPA).
    pub account: String,
    /// BT-85, Payment account name.
    pub name: Option<String>,
    /// BT-86, Payment service provider identifier (BIC).
    pub bic: Option<String>,
}

/// BG-16: payment instructions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PaymentInstructions {
    /// BT-81, Payment means type code (UNTDID 4461): "58" SEPA credit
    /// transfer, "30" credit transfer, "10" in cash, "59" SEPA direct debit,
    /// "1" not defined.
    pub means_code: String,
    /// BT-82, Payment means text.
    pub means_text: Option<String>,
    /// BT-83, Remittance information (what the payer quotes).
    pub remittance: Option<String>,
    /// BG-17.
    pub credit_transfer: Option<CreditTransfer>,
}

/// BG-3: a preceding invoice (what a credit note corrects).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrecedingInvoice {
    /// BT-25.
    pub number: String,
    /// BT-26.
    pub date: Option<NaiveDate>,
}

/// BG-20 (allowance) / BG-21 (charge) on the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AllowanceCharge {
    /// `true` for a charge (BG-21), `false` for an allowance (BG-20).
    pub charge: bool,
    /// BT-92 / BT-99.
    pub amount: Decimal,
    /// BT-93 / BT-100, Base amount.
    pub base: Option<Decimal>,
    /// BT-94 / BT-101, Percentage.
    pub percent: Option<Decimal>,
    /// BT-95 / BT-102.
    pub category: VatCategory,
    /// BT-96 / BT-103; `None` only for category O.
    pub rate: Option<Decimal>,
    /// BT-97 / BT-104.
    pub reason: Option<String>,
    /// BT-98 / BT-105 (UNTDID 5189 for allowances, 7161 for charges).
    pub reason_code: Option<String>,
}

impl AllowanceCharge {
    /// The amount with its effect on the total: negative for an allowance.
    pub fn signed(&self) -> Decimal {
        if self.charge { self.amount } else { -self.amount }
    }
}

/// BG-25: an invoice line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// BT-126, Invoice line identifier.
    pub id: String,
    /// BT-127, Invoice line note.
    pub note: Option<String>,
    /// BT-129, Invoiced quantity.
    pub quantity: Decimal,
    /// BT-130, unit of measure code (UN/ECE Rec 20 or 21, see [`crate::units`]).
    pub unit_code: String,
    /// BT-131, Invoice line net amount (quantity × price ÷ base quantity,
    /// after line allowances and charges).
    pub net_amount: Decimal,
    /// BT-146, Item net price.
    pub net_price: Decimal,
    /// BT-149, Item price base quantity.
    pub price_base_quantity: Option<Decimal>,
    /// BT-153, Item name.
    pub item_name: String,
    /// BT-154, Item description.
    pub item_description: Option<String>,
    /// BT-155, Item Seller's identifier.
    pub seller_item_id: Option<String>,
    /// BT-151, Invoiced item VAT category code.
    pub category: VatCategory,
    /// BT-152, Invoiced item VAT rate; `None` only for category O (BR-O-05).
    pub rate: Option<Decimal>,
}

/// BG-23: the VAT breakdown for one category (and rate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatBreakdown {
    /// BT-116, VAT category taxable amount.
    pub taxable: Decimal,
    /// BT-117, VAT category tax amount.
    pub tax: Decimal,
    /// BT-118.
    pub category: VatCategory,
    /// BT-119; `None` only for category O.
    pub rate: Option<Decimal>,
    /// BT-120, VAT exemption reason text.
    pub exemption_reason: Option<String>,
    /// BT-121, VAT exemption reason code (CEF VATEX).
    pub exemption_code: Option<String>,
}

impl VatBreakdown {
    /// An empty breakdown carrying only an exemption reason, for
    /// [`Invoice::finalise`] to keep when it fills in the amounts.
    pub fn exemption(category: VatCategory, rate: Option<Decimal>, code: Option<&str>, text: Option<&str>) -> Self {
        VatBreakdown {
            taxable: Decimal::ZERO,
            tax: Decimal::ZERO,
            category,
            rate,
            exemption_reason: text.map(str::to_owned),
            exemption_code: code.map(str::to_owned),
        }
    }
}

/// BG-22: document totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    /// BT-106, Sum of Invoice line net amount.
    pub line_net: Decimal,
    /// BT-107, Sum of allowances on document level.
    pub allowances: Decimal,
    /// BT-108, Sum of charges on document level.
    pub charges: Decimal,
    /// BT-109, Invoice total amount without VAT.
    pub without_vat: Decimal,
    /// BT-110, Invoice total VAT amount.
    pub vat: Decimal,
    /// BT-112, Invoice total amount with VAT.
    pub with_vat: Decimal,
    /// BT-113, Paid amount.
    pub paid: Decimal,
    /// BT-114, Rounding amount.
    pub rounding: Decimal,
    /// BT-115, Amount due for payment.
    pub due: Decimal,
}

/// The invoice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Invoice {
    /// BT-1, Invoice number.
    pub number: String,
    /// BT-2, Invoice issue date.
    pub issue_date: NaiveDate,
    /// BT-3.
    pub type_code: TypeCode,
    /// BT-5, Invoice currency code (ISO 4217).
    pub currency: String,
    /// BT-7, Value added tax point date.
    pub tax_point_date: Option<NaiveDate>,
    /// BT-9, Payment due date.
    pub due_date: Option<NaiveDate>,
    /// BT-10, Buyer reference (in XRechnung: the Leitweg-ID for public
    /// buyers, else any reference the buyer gave).
    pub buyer_reference: Option<String>,
    /// BT-13, Purchase order reference.
    pub order_reference: Option<String>,
    /// BT-20, Payment terms.
    pub payment_terms: Option<String>,
    /// BG-1 / BT-22, Invoice notes.
    pub notes: Vec<String>,
    /// BG-3.
    pub preceding: Option<PrecedingInvoice>,
    /// BG-4.
    pub seller: Party,
    /// BG-7.
    pub buyer: Party,
    /// BT-72, Actual delivery date.
    pub delivery_date: Option<NaiveDate>,
    /// BT-80, Deliver to country code.
    pub deliver_to_country: Option<String>,
    /// BG-14, Invoicing period (BT-73 start, BT-74 end).
    pub period: Option<(Option<NaiveDate>, Option<NaiveDate>)>,
    /// BG-16.
    pub payment: Option<PaymentInstructions>,
    /// BG-20 / BG-21.
    pub allowances_charges: Vec<AllowanceCharge>,
    /// BG-25.
    pub lines: Vec<Line>,
    /// BG-23.
    pub vat_breakdown: Vec<VatBreakdown>,
    /// BG-22.
    pub totals: Totals,
}

/// Round to two decimals, half away from zero (commercial rounding; the
/// CEN artefacts use XPath `round(x * 100) div 100`, which rounds halves
/// up, so they differ only on negative halves).
pub fn round2(a: Decimal) -> Decimal {
    let mut r = a.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    r.rescale(2);
    r
}

/// A breakdown key: category, and the rate for per-rate categories.
pub(crate) fn key(category: VatCategory, rate: Option<Decimal>) -> (VatCategory, Option<Decimal>) {
    if category.per_rate() { (category, rate.map(|r| r.normalize())) } else { (category, None) }
}

impl Invoice {
    /// A new invoice with no lines; fill the rest by field.
    pub fn new(number: &str, issue_date: NaiveDate, type_code: TypeCode, currency: &str) -> Self {
        Invoice {
            number: number.to_owned(),
            issue_date,
            type_code,
            currency: currency.to_owned(),
            tax_point_date: None,
            due_date: None,
            buyer_reference: None,
            order_reference: None,
            payment_terms: None,
            notes: Vec::new(),
            preceding: None,
            seller: Party::default(),
            buyer: Party::default(),
            delivery_date: None,
            deliver_to_country: None,
            period: None,
            payment: None,
            allowances_charges: Vec::new(),
            lines: Vec::new(),
            vat_breakdown: Vec::new(),
            totals: Totals::default(),
        }
    }

    /// Work out the VAT breakdown (BG-23) and the totals (BG-22) from the
    /// lines and document allowances and charges, so that BR-CO-10 to
    /// BR-CO-17 and the category rules' "-08"/"-09" sums hold:
    /// - one breakdown per category, and per rate for S, L and M, in the
    ///   order the categories first appear;
    /// - taxable amount: Σ line net − Σ allowances + Σ charges of the group;
    /// - tax: taxable × rate ÷ 100 rounded to two decimals (BR-CO-17), 0
    ///   for the zero-tax categories;
    /// - exemption reasons: kept from any breakdown already present for the
    ///   same group (see [`VatBreakdown::exemption`]); else the category's
    ///   [`VatCategory::default_exemption`] for AE, K, G and O.
    ///
    /// Line net amounts are rounded to two decimals first (BR-DEC-23).
    /// The paid (BT-113) and rounding (BT-114) amounts are kept.
    pub fn finalise(&mut self) {
        for l in &mut self.lines {
            l.net_amount = round2(l.net_amount);
        }
        for a in &mut self.allowances_charges {
            a.amount = round2(a.amount);
        }
        let mut groups: Vec<((VatCategory, Option<Decimal>), Decimal)> = Vec::new();
        let mut add = |k: (VatCategory, Option<Decimal>), v: Decimal| match groups.iter_mut().find(|(g, _)| *g == k) {
            Some((_, sum)) => *sum += v,
            None => groups.push((k, v)),
        };
        for l in &self.lines {
            add(key(l.category, l.rate), l.net_amount);
        }
        for a in &self.allowances_charges {
            add(key(a.category, a.rate), a.signed());
        }
        let old = std::mem::take(&mut self.vat_breakdown);
        for ((category, rate), taxable) in groups {
            let rate = match category {
                VatCategory::O => None,
                c if c.per_rate() => rate,
                _ => Some(Decimal::ZERO),
            };
            let tax = match rate {
                Some(r) if category.per_rate() => round2(taxable * r / Decimal::ONE_HUNDRED),
                _ => round2(Decimal::ZERO),
            };
            let given = old.iter().find(|b| key(b.category, b.rate) == key(category, rate));
            let default = category.default_exemption();
            let (exemption_code, exemption_reason) = match given {
                Some(b) if b.exemption_code.is_some() || b.exemption_reason.is_some() => {
                    (b.exemption_code.clone(), b.exemption_reason.clone())
                }
                _ => (default.map(|d| d.0.to_owned()), default.map(|d| d.1.to_owned())),
            };
            self.vat_breakdown.push(VatBreakdown {
                taxable: round2(taxable),
                tax,
                category,
                rate: rate.map(|r| r.normalize()),
                exemption_reason,
                exemption_code,
            });
        }
        let t = &mut self.totals;
        t.line_net = round2(self.lines.iter().map(|l| l.net_amount).sum());
        t.allowances = round2(self.allowances_charges.iter().filter(|a| !a.charge).map(|a| a.amount).sum());
        t.charges = round2(self.allowances_charges.iter().filter(|a| a.charge).map(|a| a.amount).sum());
        t.without_vat = round2(t.line_net - t.allowances + t.charges);
        t.vat = round2(self.vat_breakdown.iter().map(|b| b.tax).sum());
        t.with_vat = round2(t.without_vat + t.vat);
        t.paid = round2(t.paid);
        t.rounding = round2(t.rounding);
        t.due = round2(t.with_vat - t.paid + t.rounding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn line(id: &str, net: Decimal, category: VatCategory, rate: Option<Decimal>) -> Line {
        Line {
            id: id.into(),
            note: None,
            quantity: dec!(1),
            unit_code: "C62".into(),
            net_amount: net,
            net_price: net,
            price_base_quantity: None,
            item_name: format!("Item {id}"),
            item_description: None,
            seller_item_id: None,
            category,
            rate,
        }
    }

    #[test]
    fn finalise_groups_by_category_and_rate() {
        let mut inv = Invoice::new("1", NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(), TypeCode::Commercial, "EUR");
        inv.lines = vec![
            line("1", dec!(100), VatCategory::S, Some(dec!(19))),
            line("2", dec!(50.005), VatCategory::S, Some(dec!(7))),
            line("3", dec!(10), VatCategory::S, Some(dec!(19.00))),
            line("4", dec!(30), VatCategory::E, Some(dec!(0))),
        ];
        inv.allowances_charges.push(AllowanceCharge {
            charge: false,
            amount: dec!(10),
            base: None,
            percent: None,
            category: VatCategory::S,
            rate: Some(dec!(19)),
            reason: Some("Rabatt".into()),
            reason_code: None,
        });
        inv.vat_breakdown.push(VatBreakdown::exemption(VatCategory::E, Some(dec!(0)), None, Some("Steuerfrei")));
        inv.finalise();
        let b: Vec<_> = inv.vat_breakdown.iter().map(|b| (b.category, b.rate, b.taxable, b.tax)).collect();
        assert_eq!(b, [
            (VatCategory::S, Some(dec!(19)), dec!(100.00), dec!(19.00)),
            (VatCategory::S, Some(dec!(7)), dec!(50.01), dec!(3.50)),
            (VatCategory::E, Some(dec!(0)), dec!(30.00), dec!(0.00)),
        ]);
        assert_eq!(inv.vat_breakdown[2].exemption_reason.as_deref(), Some("Steuerfrei"));
        let t = &inv.totals;
        assert_eq!((t.line_net, t.allowances, t.without_vat, t.vat, t.with_vat, t.due), (
            dec!(190.01),
            dec!(10.00),
            dec!(180.01),
            dec!(22.50),
            dec!(202.51),
            dec!(202.51)
        ));
        assert_eq!(t.due.to_string(), "202.51");
    }

    #[test]
    fn default_exemptions_and_out_of_scope_without_a_rate() {
        let mut inv = Invoice::new("1", NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(), TypeCode::Commercial, "EUR");
        inv.lines = vec![line("1", dec!(100), VatCategory::AE, Some(dec!(0))), line("2", dec!(5), VatCategory::O, None)];
        inv.finalise();
        let ae = &inv.vat_breakdown[0];
        assert_eq!((ae.exemption_code.as_deref(), ae.exemption_reason.as_deref()), (Some("VATEX-EU-AE"), Some("Reverse charge")));
        assert_eq!(inv.vat_breakdown[1].rate, None);
        assert_eq!(VatCategory::parse("ae"), Some(VatCategory::AE));
        assert_eq!(round2(dec!(-0.005)), dec!(-0.01));
    }
}

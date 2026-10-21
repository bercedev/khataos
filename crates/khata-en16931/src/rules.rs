//! EN 16931 business rules as checks on an [`Invoice`].
//!
//! The rule identifiers and their meaning are those of the CEN/TC 434
//! validation artefacts (Schematron, `github.com/ConnectingEurope/
//! eInvoicing-EN16931`, files `ubl/schematron/abstract/EN16931-model.sch`
//! and `ubl/schematron/UBL/EN16931-UBL-model.sch`) [PRIMARY]. Where the
//! artefacts are more tolerant than the rule text, the artefacts are
//! followed so that an invoice they accept is not refused here:
//! BR-CO-17 and BR-S-09 accept a category tax amount within ±1 (strictly
//! less) of taxable × rate ÷ 100 rounded to two decimals.
//!
//! What is checked:
//! - presence: BR-02, 05 to 11, 16, 21, 23, 25, 27, 29, 33, 38, 48, 49, 50,
//!   61 (BR-01 is the writer's: it always writes BT-24; BR-03, 04, 12 to 15,
//!   22, 24, 26, 45 to 47 hold by construction of the model);
//! - BR-CO-09 (VAT identifier prefixes), 10 to 18, 19, 21, 22, 26;
//! - BR-DEC-*: at most two decimals on every amount except prices;
//! - the VAT category rules -01, -02, -05 to -10 for S, Z, E, AE, K (the
//!   artefacts' BR-IC-*), G and O, BR-O-11 to 14, BR-IC-11 and 12; for L
//!   and M (IGIC, IPSI) only the rate sign.
//!
//! Not checked: the code lists (BR-CL-*) beyond the currency's shape, and
//! everything about groups the model does not carry (payee, tax
//! representative, line allowances, attachments).
//!
//! [`validate_for`] adds the checks of a CIUS: for XRechnung 3.0 a subset
//! of KoSIT's BR-DE-* rules (see [`xrechnung`]).

use std::collections::BTreeSet;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::Specification;
use crate::model::{Invoice, TypeCode, VatBreakdown, VatCategory, key, round2};

/// One broken rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    /// The rule identifier ("BR-CO-15", "BR-DE-15").
    pub rule: &'static str,
    pub message: String,
    /// The rule's flag is "warning" (or "information"), not "fatal": the
    /// document is still valid.
    pub warning: bool,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}{}] {}", self.rule, if self.warning { ", warning" } else { "" }, self.message)
    }
}

/// Only the fatal violations: what makes the document invalid.
pub fn fatal(v: &[Violation]) -> Vec<&Violation> {
    v.iter().filter(|v| !v.warning).collect()
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

/// BR-CO-17 / BR-S-09 as the artefacts test them.
fn tax_close(taxable: Decimal, tax: Decimal, rate: Decimal) -> bool {
    let expected = round2(taxable.abs() * rate / Decimal::ONE_HUNDRED);
    (tax.abs() - expected).abs() < Decimal::ONE
}

/// The two-letter prefix BR-CO-09 asks of a VAT identifier: ISO 3166-1
/// alpha-2 letters (checked for shape only), or "EL" for Greece.
fn vat_prefix_ok(id: &str) -> bool {
    let b = id.trim().as_bytes();
    b.len() > 2 && b[0].is_ascii_uppercase() && b[1].is_ascii_uppercase()
}

/// Check an invoice against the EN 16931 core rules.
pub fn validate(inv: &Invoice) -> Vec<Violation> {
    let mut o = Out(Vec::new());
    core(inv, &mut o);
    categories(inv, &mut o);
    o.0
}

/// [`validate`] plus the rules of the specification's CIUS.
pub fn validate_for(inv: &Invoice, spec: Specification) -> Vec<Violation> {
    let mut v = validate(inv);
    if spec == Specification::XRechnung3 {
        v.extend(xrechnung(inv));
    }
    v
}

fn core(inv: &Invoice, o: &mut Out) {
    o.check(!blank(&inv.number), "BR-02", || "An Invoice shall have an Invoice number (BT-1).".into());
    let c = inv.currency.as_bytes();
    o.check(c.len() == 3 && c.iter().all(u8::is_ascii_uppercase), "BR-05", || {
        format!("An Invoice shall have an Invoice currency code (BT-5), ISO 4217 alpha-3; found {:?}.", inv.currency)
    });
    o.check(!blank(&inv.seller.name), "BR-06", || "An Invoice shall contain the Seller name (BT-27).".into());
    o.check(!blank(&inv.buyer.name), "BR-07", || "An Invoice shall contain the Buyer name (BT-44).".into());
    o.check(inv.seller.address.country.trim().len() == 2, "BR-09", || {
        "The Seller postal address (BG-5) shall contain a Seller country code (BT-40).".into()
    });
    o.check(inv.buyer.address.country.trim().len() == 2, "BR-11", || {
        "The Buyer postal address shall contain a Buyer country code (BT-55).".into()
    });
    o.check(!inv.lines.is_empty(), "BR-16", || "An Invoice shall have at least one Invoice line (BG-25).".into());
    for (i, l) in inv.lines.iter().enumerate() {
        let n = i + 1;
        o.check(!blank(&l.id), "BR-21", || format!("Invoice line {n} shall have an Invoice line identifier (BT-126)."));
        o.check(!blank(&l.unit_code), "BR-23", || {
            format!("Invoice line {n} shall have an Invoiced quantity unit of measure code (BT-130).")
        });
        o.check(!blank(&l.item_name), "BR-25", || format!("Invoice line {n} shall contain the Item name (BT-153)."));
        o.check(l.net_price >= Decimal::ZERO, "BR-27", || {
            format!("The Item net price (BT-146) shall NOT be negative (line {n}: {}).", l.net_price)
        });
        o.check(decimals_ok(l.net_amount), "BR-DEC-23", || {
            format!("The Invoice line net amount (BT-131) has at most 2 decimals (line {n}: {}).", l.net_amount)
        });
    }
    if let Some((start, end)) = inv.period {
        o.check(start.is_some() || end.is_some(), "BR-CO-19", || {
            "If Invoicing period (BG-14) is used, its start date (BT-73) or end date (BT-74) shall be filled.".into()
        });
        if let (Some(s), Some(e)) = (start, end) {
            o.check(e >= s, "BR-29", || format!("The Invoicing period end date {e} is before its start date {s}."));
        }
    }
    for a in &inv.allowances_charges {
        let has_reason = given(&a.reason) || given(&a.reason_code);
        if a.charge {
            o.check(has_reason, "BR-CO-22", || {
                "Each Document level charge (BG-21) shall contain a reason (BT-104) or a reason code (BT-105).".into()
            });
        } else {
            o.check(has_reason, "BR-CO-21", || {
                "Each Document level allowance (BG-20) shall contain a reason (BT-97) or a reason code (BT-98).".into()
            });
        }
        let (amount_rule, base_rule) = if a.charge { ("BR-DEC-05", "BR-DEC-06") } else { ("BR-DEC-01", "BR-DEC-02") };
        o.check(decimals_ok(a.amount), amount_rule, || format!("Allowance/charge amount {} has more than 2 decimals.", a.amount));
        o.check(a.base.is_none_or(decimals_ok), base_rule, || "Allowance/charge base amount has more than 2 decimals.".into());
    }
    for b in &inv.vat_breakdown {
        o.check(b.rate.is_some() || b.category == VatCategory::O, "BR-48", || {
            format!("Each VAT breakdown (BG-23) shall have a VAT category rate (BT-119); category {} has none.", b.category.code())
        });
        o.check(decimals_ok(b.taxable), "BR-DEC-19", || format!("VAT category taxable amount {} has more than 2 decimals.", b.taxable));
        o.check(decimals_ok(b.tax), "BR-DEC-20", || format!("VAT category tax amount {} has more than 2 decimals.", b.tax));
        let rate = b.rate.unwrap_or_default();
        let ok = if rate.is_zero() { round2(b.tax).is_zero() } else { tax_close(b.taxable, b.tax, rate) };
        o.check(ok, "BR-CO-17", || {
            format!(
                "VAT category tax amount (BT-117) {} ≠ taxable amount (BT-116) {} × rate (BT-119) {}% ÷ 100, rounded to two decimals.",
                b.tax,
                b.taxable,
                rate.normalize()
            )
        });
    }
    o.check(!inv.vat_breakdown.is_empty(), "BR-CO-18", || "An Invoice shall at least have one VAT breakdown group (BG-23).".into());
    if let Some(p) = &inv.payment {
        o.check(!blank(&p.means_code), "BR-49", || "A Payment instruction (BG-16) shall specify the Payment means type code (BT-81).".into());
        if let Some(ct) = &p.credit_transfer {
            o.check(!blank(&ct.account), "BR-50", || {
                "A Payment account identifier (BT-84) shall be present if Credit transfer (BG-17) information is provided.".into()
            });
        }
        let code = p.means_code.trim();
        o.check(!(code == "30" || code == "58") || p.credit_transfer.as_ref().is_some_and(|c| !blank(&c.account)), "BR-61", || {
            format!("Payment means {code} is a credit transfer: the Payment account identifier (BT-84) shall be present.")
        });
    }
    for (who, id) in [("Seller", &inv.seller.vat_id), ("Buyer", &inv.buyer.vat_id)] {
        if let Some(id) = id.as_deref().filter(|s| !blank(s)) {
            o.check(vat_prefix_ok(id), "BR-CO-09", || {
                format!("The {who} VAT identifier {id:?} shall start with an ISO 3166-1 alpha-2 country prefix (Greece: EL).")
            });
        }
    }
    o.check(given(&inv.seller.vat_id) || given(&inv.seller.legal_id), "BR-CO-26", || {
        "The Seller legal registration identifier (BT-30) and/or the Seller VAT identifier (BT-31) shall be present.".into()
    });

    // Totals (BR-CO-10 to 16) and their decimals (BR-DEC-09 to 18).
    let t = &inv.totals;
    let line_net: Decimal = inv.lines.iter().map(|l| l.net_amount).sum();
    let allowances: Decimal = inv.allowances_charges.iter().filter(|a| !a.charge).map(|a| a.amount).sum();
    let charges: Decimal = inv.allowances_charges.iter().filter(|a| a.charge).map(|a| a.amount).sum();
    let vat: Decimal = inv.vat_breakdown.iter().map(|b| b.tax).sum();
    o.check(t.line_net == line_net, "BR-CO-10", || {
        format!("Sum of Invoice line net amount (BT-106) {} ≠ Σ Invoice line net amount (BT-131) {line_net}.", t.line_net)
    });
    o.check(t.allowances == allowances, "BR-CO-11", || {
        format!("Sum of allowances on document level (BT-107) {} ≠ Σ allowance amounts (BT-92) {allowances}.", t.allowances)
    });
    o.check(t.charges == charges, "BR-CO-12", || {
        format!("Sum of charges on document level (BT-108) {} ≠ Σ charge amounts (BT-99) {charges}.", t.charges)
    });
    o.check(t.without_vat == line_net - t.allowances + t.charges, "BR-CO-13", || {
        format!("Invoice total amount without VAT (BT-109) {} ≠ BT-106 − BT-107 + BT-108.", t.without_vat)
    });
    o.check(t.vat == vat, "BR-CO-14", || format!("Invoice total VAT amount (BT-110) {} ≠ Σ VAT category tax amount (BT-117) {vat}.", t.vat));
    o.check(t.with_vat == round2(t.without_vat + t.vat), "BR-CO-15", || {
        format!("Invoice total amount with VAT (BT-112) {} ≠ BT-109 {} + BT-110 {}.", t.with_vat, t.without_vat, t.vat)
    });
    o.check(t.due == t.with_vat - t.paid + t.rounding, "BR-CO-16", || {
        format!("Amount due for payment (BT-115) {} ≠ BT-112 − BT-113 + BT-114.", t.due)
    });
    for (rule, a) in [
        ("BR-DEC-09", t.line_net),
        ("BR-DEC-10", t.allowances),
        ("BR-DEC-11", t.charges),
        ("BR-DEC-12", t.without_vat),
        ("BR-DEC-13", t.vat),
        ("BR-DEC-14", t.with_vat),
        ("BR-DEC-16", t.paid),
        ("BR-DEC-17", t.rounding),
        ("BR-DEC-18", t.due),
    ] {
        o.check(decimals_ok(a), rule, || format!("The amount {a} has more than 2 decimals."));
    }
}

/// Rule identifiers of a category, in the artefacts' naming.
fn prefix(c: VatCategory) -> &'static str {
    match c {
        VatCategory::S => "S",
        VatCategory::Z => "Z",
        VatCategory::E => "E",
        VatCategory::AE => "AE",
        VatCategory::K => "IC",
        VatCategory::G => "G",
        VatCategory::O => "O",
        VatCategory::L => "IG",
        VatCategory::M => "IP",
    }
}

/// `"BR-{prefix}-{n}"` as a static string: the rule ids are a fixed set.
fn rule(c: VatCategory, n: u8) -> &'static str {
    macro_rules! table {
        ($($p:literal),*) => {
            match (prefix(c), n) {
                $(
                    ($p, 1) => concat!("BR-", $p, "-01"),
                    ($p, 2) => concat!("BR-", $p, "-02"),
                    ($p, 5) => concat!("BR-", $p, "-05"),
                    ($p, 6) => concat!("BR-", $p, "-06"),
                    ($p, 8) => concat!("BR-", $p, "-08"),
                    ($p, 9) => concat!("BR-", $p, "-09"),
                    ($p, 10) => concat!("BR-", $p, "-10"),
                )*
                _ => "BR-CO-04",
            }
        };
    }
    table!("S", "Z", "E", "AE", "IC", "G", "O", "IG", "IP")
}

fn categories(inv: &Invoice, o: &mut Out) {
    let used: BTreeSet<VatCategory> =
        inv.lines.iter().map(|l| l.category).chain(inv.allowances_charges.iter().map(|a| a.category)).collect();
    let in_breakdown = |c: VatCategory| inv.vat_breakdown.iter().filter(|b| b.category == c).count();
    let seller_vat = given(&inv.seller.vat_id);
    let seller_tax = seller_vat || given(&inv.seller.tax_registration);
    let buyer_id = given(&inv.buyer.vat_id) || given(&inv.buyer.legal_id);

    for &c in &used {
        let name = c.name();
        // -01: the breakdown has the category (exactly once, except S, L, M).
        let n = in_breakdown(c);
        let ok = if c.per_rate() { n >= 1 } else { n == 1 };
        o.check(ok, rule(c, 1), || {
            format!(
                "Lines or allowances/charges with VAT category \"{name}\" need {} VAT breakdown (BG-23) with that category; found {n}.",
                if c.per_rate() { "at least one" } else { "exactly one" }
            )
        });
        // -02: who must be identified.
        let has_lines = inv.lines.iter().any(|l| l.category == c);
        if has_lines {
            match c {
                VatCategory::S | VatCategory::Z | VatCategory::E | VatCategory::L | VatCategory::M => {
                    o.check(seller_tax, rule(c, 2), || {
                        format!("An Invoice with \"{name}\" lines shall contain the Seller VAT identifier (BT-31) or tax registration identifier (BT-32).")
                    })
                }
                VatCategory::AE => o.check(seller_tax && buyer_id, rule(c, 2), || {
                    "A reverse-charge invoice shall contain the Seller VAT identifier (BT-31) or tax registration identifier (BT-32), and the Buyer VAT identifier (BT-48) or legal registration identifier (BT-47).".into()
                }),
                VatCategory::K => o.check(seller_vat && given(&inv.buyer.vat_id), rule(c, 2), || {
                    "An intra-community supply shall contain the Seller VAT identifier (BT-31) and the Buyer VAT identifier (BT-48).".into()
                }),
                VatCategory::G => o.check(seller_vat, rule(c, 2), || {
                    "An export invoice shall contain the Seller VAT identifier (BT-31).".into()
                }),
                VatCategory::O => o.check(!seller_vat && !given(&inv.buyer.vat_id), rule(c, 2), || {
                    "An Invoice with \"Not subject to VAT\" lines shall not contain the Seller VAT identifier (BT-31) or the Buyer VAT identifier (BT-48).".into()
                }),
            }
        }
    }

    // -05 / -06: rates on lines and allowances/charges.
    let rate_ok = |c: VatCategory, r: Option<Decimal>| match c {
        VatCategory::S => r.is_some_and(|r| r > Decimal::ZERO),
        VatCategory::O => r.is_none(),
        VatCategory::L | VatCategory::M => r.is_some_and(|r| r >= Decimal::ZERO),
        _ => r.is_some_and(|r| r.is_zero()),
    };
    let rate_rule = |c: VatCategory| match c {
        VatCategory::S => "greater than zero",
        VatCategory::O => "absent",
        VatCategory::L | VatCategory::M => "zero or more",
        _ => "0 (zero)",
    };
    for (i, l) in inv.lines.iter().enumerate() {
        o.check(rate_ok(l.category, l.rate), rule(l.category, 5), || {
            format!("Line {}: the VAT rate (BT-152) of a \"{}\" line shall be {}.", i + 1, l.category.name(), rate_rule(l.category))
        });
    }
    for a in &inv.allowances_charges {
        o.check(rate_ok(a.category, a.rate), rule(a.category, 6), || {
            format!("A document allowance/charge in \"{}\" shall have a VAT rate that is {}.", a.category.name(), rate_rule(a.category))
        });
    }

    // -08, -09, -10 on every breakdown.
    for b in &inv.vat_breakdown {
        let c = b.category;
        let k = key(c, b.rate);
        let sum: Decimal = inv.lines.iter().filter(|l| key(l.category, l.rate) == k).map(|l| l.net_amount).sum::<Decimal>()
            + inv.allowances_charges.iter().filter(|a| key(a.category, a.rate) == k).map(|a| a.signed()).sum::<Decimal>();
        if !matches!(c, VatCategory::L | VatCategory::M) {
            o.check(b.taxable == sum, rule(c, 8), || {
                format!("The \"{}\" VAT breakdown's taxable amount (BT-116) {} ≠ Σ its lines − allowances + charges {sum}.", c.name(), b.taxable)
            });
        }
        tax_and_reason(b, o);
    }

    // BR-O-11 to 14: "Not subject to VAT" stands alone.
    if in_breakdown(VatCategory::O) > 0 {
        o.check(inv.vat_breakdown.iter().all(|b| b.category == VatCategory::O), "BR-O-11", || {
            "An Invoice with a \"Not subject to VAT\" breakdown shall not contain other VAT breakdowns.".into()
        });
        o.check(inv.lines.iter().all(|l| l.category == VatCategory::O), "BR-O-12", || {
            "An Invoice with a \"Not subject to VAT\" breakdown shall not contain lines of another category.".into()
        });
        o.check(inv.allowances_charges.iter().all(|a| a.category == VatCategory::O), "BR-O-13", || {
            "An Invoice with a \"Not subject to VAT\" breakdown shall not contain allowances or charges of another category.".into()
        });
    }
    // BR-IC-11 and 12: an intra-community supply says when and where to.
    if in_breakdown(VatCategory::K) > 0 {
        o.check(inv.delivery_date.is_some() || inv.period.is_some(), "BR-IC-11", || {
            "An intra-community supply shall have the Actual delivery date (BT-72) or the Invoicing period (BG-14).".into()
        });
        o.check(inv.deliver_to_country.as_deref().is_some_and(|c| !blank(c)), "BR-IC-12", || {
            "An intra-community supply shall have the Deliver to country code (BT-80).".into()
        });
    }
}

fn tax_and_reason(b: &VatBreakdown, o: &mut Out) {
    let c = b.category;
    let has_reason = given(&b.exemption_reason) || given(&b.exemption_code);
    match c {
        VatCategory::S => {
            o.check(tax_close(b.taxable, b.tax, b.rate.unwrap_or_default()), rule(c, 9), || {
                format!("The standard-rated tax amount (BT-117) {} ≠ taxable amount {} × rate.", b.tax, b.taxable)
            });
            o.check(!has_reason, rule(c, 10), || "A \"Standard rated\" VAT breakdown shall not have a VAT exemption reason.".into());
        }
        VatCategory::L | VatCategory::M => {}
        _ => {
            o.check(b.tax.is_zero(), rule(c, 9), || format!("The \"{}\" VAT breakdown's tax amount (BT-117) shall be 0; found {}.", c.name(), b.tax));
            if c == VatCategory::Z {
                o.check(!has_reason, rule(c, 10), || "A \"Zero rated\" VAT breakdown shall not have a VAT exemption reason.".into());
            } else {
                o.check(has_reason, rule(c, 10), || {
                    format!("A \"{}\" VAT breakdown shall have a VAT exemption reason code (BT-121) or text (BT-120).", c.name())
                });
            }
        }
    }
}

/// Invoice type codes XRechnung 3.0 expects (BR-DE-17, a warning).
pub const XRECHNUNG_TYPE_CODES: &[&str] = &["326", "380", "384", "389", "381", "875", "876", "877"];

/// The IBAN check (ISO 13616, mod 97 = 1): two letters, two digits, up to
/// 30 letters and digits.
pub fn iban_ok(iban: &str) -> bool {
    let s: String = iban.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    let b = s.as_bytes();
    if !(15..=34).contains(&b.len())
        || !b[..2].iter().all(u8::is_ascii_uppercase)
        || !b[2..4].iter().all(u8::is_ascii_digit)
        || !b.iter().all(u8::is_ascii_alphanumeric)
    {
        return false;
    }
    let mut r: u32 = 0;
    for c in s[4..].chars().chain(s[..4].chars()) {
        let v = c.to_digit(36).expect("alphanumeric");
        r = (r * if v >= 10 { 100 } else { 10 } + v) % 97;
    }
    r == 1
}

/// KoSIT's XRechnung 3.0 national rules (BR-DE-*) that concern what the
/// model carries, with the flags of the XRechnung Schematron v2.6.0
/// (`docs/research/germany-compliance.md` §9) [PRIMARY]. Not checked:
/// BR-DE-10/11 (no deliver-to address), BR-DE-18 (Skonto syntax; KhataOS
/// writes no Skonto), BR-DE-22 (no attachments), BR-DE-24/25/30/31 (no
/// card or direct debit). The KoSIT validator
/// (`github.com/itplr-kosit/validator-configuration-xrechnung`) is the
/// reference; run it before sending.
///
/// Also warned, under KhataOS ids: the Seller and Buyer electronic
/// addresses (BT-34, BT-49), which XRechnung 3.0 is reported to require
/// through its schema cardinalities rather than a BR-DE rule [UNCERTAIN:
/// not found in the research].
pub fn xrechnung(inv: &Invoice) -> Vec<Violation> {
    let mut o = Out(Vec::new());
    o.check(inv.payment.is_some(), "BR-DE-1", || "An XRechnung shall contain Payment instructions (BG-16).".into());
    let c = &inv.seller.contact;
    o.check(!c.is_empty(), "BR-DE-2", || "An XRechnung shall contain the Seller contact (BG-6).".into());
    o.check(!blank(&inv.seller.address.city), "BR-DE-3", || "An XRechnung shall contain the Seller city (BT-37).".into());
    o.check(!blank(&inv.seller.address.postal_code), "BR-DE-4", || "An XRechnung shall contain the Seller post code (BT-38).".into());
    o.check(!blank(&c.name), "BR-DE-5", || "An XRechnung shall contain the Seller contact point (BT-41).".into());
    o.check(!blank(&c.phone), "BR-DE-6", || "An XRechnung shall contain the Seller contact telephone number (BT-42).".into());
    o.check(!blank(&c.email), "BR-DE-7", || "An XRechnung shall contain the Seller contact email address (BT-43).".into());
    o.check(!blank(&inv.buyer.address.city), "BR-DE-8", || "An XRechnung shall contain the Buyer city (BT-52).".into());
    o.check(!blank(&inv.buyer.address.postal_code), "BR-DE-9", || "An XRechnung shall contain the Buyer post code (BT-53).".into());
    // O breakdowns get BT-119 = 0 from the writers (see `crate::ubl`).
    o.check(inv.vat_breakdown.iter().all(|b| b.rate.is_some() || b.category == VatCategory::O), "BR-DE-14", || {
        "Every VAT breakdown of an XRechnung shall have a VAT category rate (BT-119).".into()
    });
    o.check(given(&inv.buyer_reference), "BR-DE-15", || {
        "An XRechnung shall contain the Buyer reference (BT-10): the Leitweg-ID for a public buyer, else the reference the buyer gave.".into()
    });
    let taxed = inv.lines.iter().any(|l| l.category != VatCategory::O);
    o.check(!taxed || given(&inv.seller.vat_id) || given(&inv.seller.tax_registration), "BR-DE-16", || {
        "An XRechnung with VAT categories S, Z, E, AE, K, G, L or M shall contain the Seller VAT identifier (BT-31) or tax registration identifier (BT-32).".into()
    });
    let code = inv.type_code.code();
    o.warn(XRECHNUNG_TYPE_CODES.contains(&code), "BR-DE-17", || format!("Invoice type code {code} is not one XRechnung expects."));
    if let Some(p) = &inv.payment {
        let code = p.means_code.trim();
        let ct = p.credit_transfer.as_ref();
        o.check(!(code == "30" || code == "58") || ct.is_some(), "BR-DE-23-a", || {
            format!("Payment means {code} (credit transfer) shall have the Credit transfer group (BG-17).")
        });
        o.check((code == "30" || code == "58") || ct.is_none(), "BR-DE-23-b", || {
            format!("Only payment means 30 and 58 may carry the Credit transfer group (BG-17); found {code}.")
        });
        if code == "58" && let Some(ct) = ct {
            o.warn(iban_ok(&ct.account), "BR-DE-19", || format!("The payment account (BT-84) {:?} should be a valid IBAN for SEPA credit transfer.", ct.account));
        }
    }
    o.warn(inv.type_code != TypeCode::Corrected || inv.preceding.is_some(), "BR-DE-26", || {
        "A corrected invoice (384) should reference the preceding invoice (BG-3).".into()
    });
    if !blank(&c.phone) {
        o.warn(c.phone.chars().filter(char::is_ascii_digit).count() >= 3, "BR-DE-27", || "The Seller contact telephone number (BT-42) should have at least three digits.".into());
    }
    if !blank(&c.email) {
        let e = c.email.trim();
        let shape = e.matches('@').count() == 1 && !e.starts_with('@') && !e.ends_with('@') && !e.contains(char::is_whitespace) && e.split('@').nth(1).is_some_and(|d| d.contains('.') && !d.starts_with('.') && !d.ends_with('.'));
        o.warn(shape, "BR-DE-28", || format!("The Seller contact email address (BT-43) {e:?} should have exactly one @ and a domain."));
    }
    o.warn(inv.seller.electronic_address.as_ref().is_some_and(|e| !blank(&e.value)), "KHATA-XR-BT-34", || {
        "XRechnung 3.0 is reported to require the Seller electronic address (BT-34) [UNCERTAIN].".into()
    });
    o.warn(inv.buyer.electronic_address.as_ref().is_some_and(|e| !blank(&e.value)), "KHATA-XR-BT-49", || {
        "XRechnung 3.0 is reported to require the Buyer electronic address (BT-49) [UNCERTAIN].".into()
    });
    o.0
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::*;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    pub(crate) fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    pub(crate) fn line(id: &str, qty: Decimal, price: Decimal, category: VatCategory, rate: Option<Decimal>) -> Line {
        Line {
            id: id.into(),
            note: None,
            quantity: qty,
            unit_code: "H87".into(),
            net_amount: qty * price,
            net_price: price,
            price_base_quantity: None,
            item_name: format!("Artikel {id}"),
            item_description: None,
            seller_item_id: None,
            category,
            rate,
        }
    }

    /// A small German B2B invoice: 19% and 7% lines, SEPA transfer.
    pub(crate) fn sample() -> Invoice {
        let mut inv = Invoice::new("RE-2026-001", d(2026, 3, 2), TypeCode::Commercial, "EUR");
        inv.due_date = Some(d(2026, 4, 1));
        inv.buyer_reference = Some("04011000-12345-34".into());
        inv.delivery_date = Some(d(2026, 2, 27));
        inv.notes.push("Vielen Dank für Ihren Auftrag & Ihr Vertrauen.".into());
        inv.seller = Party {
            name: "Muster GmbH".into(),
            trading_name: None,
            legal_id: None,
            vat_id: Some("DE136695976".into()),
            tax_registration: Some("2893081508152".into()),
            electronic_address: Some(ElectronicAddress { value: "rechnung@muster.de".into(), scheme: "EM".into() }),
            address: Address {
                line1: "Hauptstraße 1".into(),
                city: "Berlin".into(),
                postal_code: "10115".into(),
                country: "DE".into(),
                ..Address::default()
            },
            contact: Contact { name: "Erika Muster".into(), phone: "+49 30 123456".into(), email: "erika@muster.de".into() },
        };
        inv.buyer = Party {
            name: "Kunde AG".into(),
            vat_id: Some("DE811569869".into()),
            electronic_address: Some(ElectronicAddress { value: "ap@kunde.de".into(), scheme: "EM".into() }),
            address: Address { line1: "Marktplatz 5".into(), city: "München".into(), postal_code: "80331".into(), country: "DE".into(), ..Address::default() },
            ..Party::default()
        };
        inv.payment = Some(PaymentInstructions {
            means_code: "58".into(),
            means_text: None,
            remittance: Some("RE-2026-001".into()),
            credit_transfer: Some(CreditTransfer { account: "DE02120300000000202051".into(), name: Some("Muster GmbH".into()), bic: None }),
        });
        inv.payment_terms = Some("Zahlbar innerhalb von 30 Tagen ohne Abzug.".into());
        inv.lines = vec![
            line("1", dec!(2), dec!(50.00), VatCategory::S, Some(dec!(19))),
            line("2", dec!(3), dec!(10.10), VatCategory::S, Some(dec!(7))),
        ];
        inv.finalise();
        inv
    }

    fn rules(v: &[Violation]) -> Vec<&'static str> {
        v.iter().map(|v| v.rule).collect()
    }

    #[test]
    fn a_finalised_invoice_passes_core_and_xrechnung() {
        let inv = sample();
        assert_eq!(validate_for(&inv, Specification::XRechnung3), [], "{:#?}", validate_for(&inv, Specification::XRechnung3));
        assert_eq!((inv.totals.without_vat, inv.totals.vat, inv.totals.due), (dec!(130.30), dec!(21.12), dec!(151.42)));
    }

    #[test]
    fn totals_rules_catch_each_broken_sum() {
        let mut inv = sample();
        inv.totals.line_net += dec!(1);
        inv.totals.vat = dec!(21.13);
        inv.totals.with_vat = dec!(1);
        inv.totals.due = dec!(151.415);
        let r = rules(&validate(&inv));
        for id in ["BR-CO-10", "BR-CO-14", "BR-CO-15", "BR-CO-16", "BR-DEC-18"] {
            assert!(r.contains(&id), "{id} in {r:?}");
        }
        assert!(!r.contains(&"BR-CO-13"), "BR-CO-13 uses the Σ of lines, which still matches BT-109");

        let mut inv = sample();
        inv.allowances_charges.push(AllowanceCharge {
            charge: true,
            amount: dec!(5),
            base: None,
            percent: None,
            category: VatCategory::S,
            rate: Some(dec!(19)),
            reason: None,
            reason_code: None,
        });
        let r = rules(&validate(&inv));
        for id in ["BR-CO-12", "BR-CO-22", "BR-S-08"] {
            assert!(r.contains(&id), "{id} in {r:?}");
        }
        inv.allowances_charges[0].reason = Some("Fracht".into());
        inv.finalise();
        assert_eq!(validate(&inv), []);
        assert_eq!((inv.totals.charges, inv.totals.without_vat), (dec!(5.00), dec!(135.30)));
    }

    #[test]
    fn category_tax_follows_the_artefacts_tolerance() {
        let mut inv = sample();
        // 100 × 19% = 19.00; the artefacts accept anything strictly within ±1.
        inv.vat_breakdown[0].tax = dec!(19.99);
        inv.totals.vat = dec!(22.11);
        inv.totals.with_vat = dec!(152.41);
        inv.totals.due = dec!(152.41);
        assert_eq!(validate(&inv), []);
        inv.vat_breakdown[0].tax = dec!(20.00);
        inv.totals.vat = dec!(22.12);
        inv.totals.with_vat = dec!(152.42);
        inv.totals.due = dec!(152.42);
        assert_eq!(rules(&validate(&inv)), ["BR-CO-17", "BR-S-09"]);
    }

    #[test]
    fn standard_rate_rules() {
        let mut inv = sample();
        inv.lines[0].rate = Some(dec!(0));
        inv.vat_breakdown[0].exemption_reason = Some("no".into());
        inv.seller.vat_id = None;
        inv.seller.tax_registration = None;
        let r = rules(&validate(&inv));
        for id in ["BR-S-05", "BR-S-08", "BR-S-02", "BR-S-10", "BR-CO-26"] {
            assert!(r.contains(&id), "{id} in {r:?}");
        }
        let mut inv = sample();
        inv.vat_breakdown.remove(1);
        let r = rules(&validate(&inv));
        assert!(r.contains(&"BR-CO-14") && !r.contains(&"BR-S-01"), "one S breakdown is enough for BR-S-01: {r:?}");
        inv.vat_breakdown.clear();
        assert!(rules(&validate(&inv)).contains(&"BR-S-01"));
        assert!(rules(&validate(&inv)).contains(&"BR-CO-18"));
    }

    fn single(category: VatCategory, rate: Option<Decimal>) -> Invoice {
        let mut inv = sample();
        inv.lines = vec![line("1", dec!(4), dec!(25), category, rate)];
        inv.finalise();
        inv
    }

    #[test]
    fn zero_exempt_reverse_charge_export_and_intra_community() {
        let z = single(VatCategory::Z, Some(dec!(0)));
        assert_eq!(validate(&z), []);
        let mut bad = z.clone();
        bad.vat_breakdown[0].exemption_code = Some("VATEX-EU-79-C".into());
        assert_eq!(rules(&validate(&bad)), ["BR-Z-10"]);

        let e = single(VatCategory::E, Some(dec!(0)));
        assert_eq!(rules(&validate(&e)), ["BR-E-10"], "an exemption needs its reason");
        let mut e = sample();
        e.lines = vec![line("1", dec!(1), dec!(80), VatCategory::E, Some(dec!(0)))];
        e.vat_breakdown = vec![VatBreakdown::exemption(VatCategory::E, Some(dec!(0)), None, Some("Kleinunternehmer gemäß § 19 UStG"))];
        e.finalise();
        assert_eq!(validate(&e), []);

        let ae = single(VatCategory::AE, Some(dec!(0)));
        assert_eq!(validate(&ae), []);
        let mut bad = ae.clone();
        bad.buyer.vat_id = None;
        bad.lines[0].rate = Some(dec!(19));
        let r = rules(&validate(&bad));
        assert!(r.contains(&"BR-AE-02") && r.contains(&"BR-AE-05"), "{r:?}");

        let mut k = single(VatCategory::K, Some(dec!(0)));
        k.buyer.vat_id = Some("FR40303265045".into());
        assert_eq!(rules(&validate(&k)), ["BR-IC-12"], "the delivery date is set; the country is not");
        k.deliver_to_country = Some("FR".into());
        assert_eq!(validate(&k), []);
        k.seller.vat_id = None;
        assert!(rules(&validate(&k)).contains(&"BR-IC-02"), "a Steuernummer is not enough for an intra-community supply");

        let mut g = single(VatCategory::G, Some(dec!(0)));
        assert_eq!(validate(&g), []);
        g.vat_breakdown[0].tax = dec!(1);
        assert!(rules(&validate(&g)).contains(&"BR-G-09"));
    }

    #[test]
    fn not_subject_to_vat_stands_alone() {
        let mut o = single(VatCategory::O, None);
        assert_eq!(rules(&validate(&o)), ["BR-O-02"], "no VAT identifiers on an out-of-scope invoice");
        o.seller.vat_id = None;
        o.seller.legal_id = Some("HRB 12345".into());
        o.buyer.vat_id = None;
        assert_eq!(validate(&o), []);
        o.lines.push(line("2", dec!(1), dec!(1), VatCategory::S, Some(dec!(19))));
        o.lines[0].rate = Some(dec!(0));
        o.finalise();
        let r = rules(&validate(&o));
        for id in ["BR-O-05", "BR-O-11", "BR-O-12"] {
            assert!(r.contains(&id), "{id} in {r:?}");
        }
    }

    #[test]
    fn iban_check() {
        assert!(iban_ok("DE02 1203 0000 0000 2020 51") && iban_ok("GB33BUKB20201555555555"));
        assert!(!iban_ok("DE02120300000000202052") && !iban_ok("DE0212") && !iban_ok("1234567890123456"));
    }

    #[test]
    fn presence_and_payment_rules() {
        let mut inv = sample();
        inv.number = " ".into();
        inv.currency = "eur".into();
        inv.buyer.address.country = String::new();
        inv.lines[0].unit_code = String::new();
        inv.lines[1].net_price = dec!(-1);
        inv.payment.as_mut().unwrap().credit_transfer = None;
        inv.buyer.vat_id = Some("811569869".into());
        inv.period = Some((Some(d(2026, 3, 1)), Some(d(2026, 2, 1))));
        let r = rules(&validate(&inv));
        for id in ["BR-02", "BR-05", "BR-11", "BR-23", "BR-27", "BR-61", "BR-CO-09", "BR-29"] {
            assert!(r.contains(&id), "{id} in {r:?}");
        }
    }

    #[test]
    fn xrechnung_adds_its_national_rules() {
        let mut inv = sample();
        inv.buyer_reference = None;
        inv.seller.contact.phone = String::new();
        inv.buyer.address.postal_code = String::new();
        inv.payment = None;
        inv.buyer.electronic_address = None;
        assert_eq!(validate(&inv), [], "core EN 16931 does not ask for these");
        let v = validate_for(&inv, Specification::XRechnung3);
        assert_eq!(rules(&v), ["BR-DE-1", "BR-DE-6", "BR-DE-9", "BR-DE-15", "KHATA-XR-BT-49"]);
        assert_eq!(fatal(&v).len(), 4, "BT-49 is only a warning");
        assert_eq!(rules(&validate_for(&inv, Specification::PeppolBis3)), Vec::<&str>::new());

        let mut inv = sample();
        inv.type_code = TypeCode::Corrected;
        inv.seller.contact.email = "erika@@muster".into();
        inv.seller.contact.phone = "12".into();
        inv.payment.as_mut().unwrap().credit_transfer.as_mut().unwrap().account = "DE02120300000000202052".into();
        let v = validate_for(&inv, Specification::XRechnung3);
        assert_eq!(rules(&v), ["BR-DE-19", "BR-DE-26", "BR-DE-27", "BR-DE-28"]);
        assert!(fatal(&v).is_empty(), "all warnings");
        inv.payment.as_mut().unwrap().means_code = "10".into();
        assert!(rules(&validate_for(&inv, Specification::XRechnung3)).contains(&"BR-DE-23-b"));
    }
}

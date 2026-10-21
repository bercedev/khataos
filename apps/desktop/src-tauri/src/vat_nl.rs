//! Netherlands: the aangifte omzetbelasting from the books. The rules live
//! in `khata_nl::returns`; the vouchers are read as in `vat_de.rs`, with
//! the reader shared by France and Spain (`vat_fr::eu`): Sales and Credit
//! Notes are outward, Purchases and Debit Notes inward; ledgers whose type
//! of tax is "BTW" (or "VAT") give the btw.
//!
//! Where the party is decides the rubriek of an untaxed line: a sale of
//! goods to an EU business → 3b, services to an EU business → 3b, goods
//! out of the EU → 3a, services to customers outside the EU → not on the
//! aangifte; a purchase from an EU supplier → 4b, services from outside
//! the EU → 4a; a domestic "btw verlegd" purchase (a REVERSE class) → 2a.

use khata_core::Book;
use khata_nl::btw::{self, Treatment};
use khata_nl::provincies;
use khata_nl::returns::{self, Aangifte, DocKind, DocLine, InwardDoc, OutwardDoc, Period};
use serde::Serialize;

use crate::api::{ApiResult, Slot, open_ref, parse_date};
use crate::vat_fr::eu::{self, Kind};

fn classify(class: &str, rate: rust_decimal::Decimal) -> Kind {
    match btw::treatment(class, rate) {
        Treatment::Taxed => Kind::Taxed,
        Treatment::Zero => Kind::Zero,
        Treatment::Exempt => Kind::Exempt,
        Treatment::ReverseCharge => Kind::Reverse,
        Treatment::IntraEu => Kind::IntraEu,
        Treatment::Export => Kind::Export,
        Treatment::OutOfScope => Kind::OutOfScope,
    }
}

pub(crate) const NETHERLANDS: eu::Country = eu::Country {
    code: "NL",
    components: &[btw::BTW, "VAT"],
    in_eu: provincies::in_eu,
    also_home: &[],
    standard_class: btw::STANDARD_CLASS,
    classify,
};

fn treatment(k: Kind) -> Treatment {
    match k {
        Kind::Taxed => Treatment::Taxed,
        Kind::Zero => Treatment::Zero,
        Kind::Exempt => Treatment::Exempt,
        Kind::Reverse => Treatment::ReverseCharge,
        Kind::IntraEu => Treatment::IntraEu,
        Kind::Export => Treatment::Export,
        Kind::OutOfScope => Treatment::OutOfScope,
    }
}

fn netherlands_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_nl::COUNTRY.code) {
        return Err("the aangifte omzetbelasting is for companies in the Netherlands".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AangifteDto {
    /// The company's btw-id as stored.
    pub tax_id: Option<String>,
    #[serde(flatten)]
    pub ret: Aangifte,
}

/// The aangifte omzetbelasting rubrieken for a period (a quarter, a month
/// or a year).
pub fn aangifte_nl(slot: &Slot, from: &str, to: &str) -> ApiResult<AangifteDto> {
    let book = open_ref(slot)?;
    netherlands_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let mut warnings = Vec::new();
    let docs = eu::documents(book, period.from, period.to, &NETHERLANDS, &mut warnings)?;
    let (mut outward, mut inward) = (Vec::new(), Vec::new());
    for d in docs {
        let kind = if d.credit { DocKind::CreditNote } else { DocKind::Invoice };
        let lines = d
            .lines
            .iter()
            .map(|l| DocLine { treatment: treatment(l.kind), goods: l.goods, taxable_value: l.value, percent: l.percent, btw: l.tax })
            .collect();
        if d.outward {
            outward.push(OutwardDoc {
                id: d.id,
                kind,
                number: d.number,
                date: d.date,
                party_name: d.party_name,
                party_country: d.party_country,
                party_vat_id: d.party_vat_id,
                cancelled: d.cancelled,
                lines,
            });
        } else {
            inward.push(InwardDoc {
                id: d.id,
                kind,
                number: d.number,
                date: d.date,
                party_name: d.party_name,
                party_country: d.party_country,
                party_vat_id: d.party_vat_id,
                cancelled: d.cancelled,
                lines,
            });
        }
    }
    let mut ret = returns::aangifte(&outward, &inward, period);
    ret.warnings.splice(0..0, warnings);
    Ok(AangifteDto { tax_id: profile.tax_id.clone(), ret })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, indian_company, ledger, voucher};
    use crate::api::{StockLineInput, cancel_voucher, check_tax_id, create_company, post_voucher, set_country};
    use crate::masters::{LedgerInput, StockItemInput, alter_ledger, get_ledger, save_stock_item, save_unit};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use chrono::Datelike;
    use khata_core::{DrCr, Mailing, TaxSide};
    use rust_decimal_macros::dec;

    pub(crate) fn items(item_id: &str, qty: &str, rate: &str) -> Vec<StockLineInput> {
        vec![StockLineInput {
            item_id: item_id.into(),
            qty: qty.into(),
            rate: rate.into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }]
    }

    pub(crate) fn item(slot: &mut Slot, name: &str, unit: &str, class: &str) -> String {
        let input: StockItemInput =
            serde_json::from_value(serde_json::json!({ "name": name, "unit_id": unit, "tax_class": class })).unwrap();
        save_stock_item(slot, &input).unwrap().id
    }

    pub(crate) fn party(slot: &mut Slot, name: &str, group: &str, tax_id: Option<&str>, country: &str) -> String {
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
            mailing: Some(Mailing { country: Some(country.into()), ..Mailing::default() }),
            tax_class: None,
            pan: None,
            tds: None,
        })
        .unwrap();
        id
    }

    #[test]
    fn a_dutch_company_checks_btw_ids() {
        let f = TempFile::new("nl-ids");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Voorbeeld BV").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "nl").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("NL", "EUR", "Btw-id"));
        assert_eq!((info.fy_start.month(), info.fy_start.day()), (1, 1), "calendar years");
        let ok = check_tax_id(&slot, "nl 0024.55.799.b11").unwrap();
        assert_eq!((ok.value.as_str(), ok.error), ("NL002455799B11", None));
        assert!(check_tax_id(&slot, "NL002455799B12").unwrap().error.is_some());
        assert!(set_company_tax(&mut slot, "NL004495445B01", "XX").is_err(), "the province must be known");
        let tax = set_company_tax(&mut slot, "NL004495445B01", "NH").unwrap();
        assert_eq!((tax.label.as_str(), tax.region_name.as_deref(), tax.regions.len()), ("Btw-id", Some("Noord-Holland"), 12));
    }

    #[test]
    fn aangifte_from_the_books() {
        let f = TempFile::new("nl-aangifte");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Voorbeeld BV").unwrap();
        set_country(&mut slot, Some(&f.0), "NL").unwrap();
        set_company_tax(&mut slot, "NL004495445B01", "NH").unwrap();
        for (class, percent) in [("STANDARD", "21"), ("REDUCED", "9"), ("EXEMPT", "0"), ("REVERSE", "0")] {
            set_tax_rate(&mut slot, class, "2019-01-01", percent).unwrap();
        }
        let klant = party(&mut slot, "Klant BV", "Sundry Debtors", Some("NL002455799B11"), "NL");
        let berlin = party(&mut slot, "Berlin GmbH", "Sundry Debtors", Some("DE136695976"), "DE");
        let ny = party(&mut slot, "New York Inc", "Sundry Debtors", None, "US");
        let leverancier = party(&mut slot, "Leverancier BV", "Sundry Creditors", Some("NL000099998B57"), "NL");
        let wien = party(&mut slot, "Wien GmbH", "Sundry Creditors", Some("ATU13585627"), "AT");
        let sales = ledger(&mut slot, "Omzet", "Sales Accounts", "");
        let advies = ledger(&mut slot, "Advies", "Sales Accounts", "");
        let purchase = ledger(&mut slot, "Inkoop", "Purchase Accounts", "");
        let software = ledger(&mut slot, "Software", "Indirect Expenses", "");
        let output = ledger(&mut slot, "Af te dragen btw", "Duties & Taxes", "");
        let input = ledger(&mut slot, "Voorbelasting", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "BTW", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &input, "BTW", TaxSide::Input).unwrap();
        let st = save_unit(&mut slot, "", "St", "", 0).unwrap().id;
        let laptop = item(&mut slot, "Laptop", &st, "STANDARD");
        let boek = item(&mut slot, "Boek", &st, "REDUCED");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str, qty: &str, rate: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, qty, rate);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap().id;
        let mut l = line(&sales, DrCr::Cr, "2040", &laptop, "2", "1000");
        l.items.extend(items(&boek, "1", "40"));
        post(&mut slot, "Sales", "2026-01-10", vec![entry(&klant, DrCr::Dr, "2463.60"), l, entry(&output, DrCr::Cr, "423.60")]);
        post(&mut slot, "CreditNote", "2026-01-20", vec![
            entry(&klant, DrCr::Cr, "1210"),
            line(&sales, DrCr::Dr, "1000", &laptop, "1", "1000"),
            entry(&output, DrCr::Dr, "210"),
        ]);
        post(&mut slot, "Sales", "2026-02-01", vec![entry(&berlin, DrCr::Dr, "3000"), line(&sales, DrCr::Cr, "3000", &laptop, "3", "1000")]);
        post(&mut slot, "Sales", "2026-02-02", vec![entry(&ny, DrCr::Dr, "1000"), line(&sales, DrCr::Cr, "1000", &laptop, "1", "1000")]);
        post(&mut slot, "Sales", "2026-02-03", vec![entry(&berlin, DrCr::Dr, "500"), entry(&advies, DrCr::Cr, "500")]);
        post(&mut slot, "Purchase", "2026-02-05", vec![
            entry(&leverancier, DrCr::Cr, "1210"),
            line(&purchase, DrCr::Dr, "1000", &laptop, "1", "1000"),
            entry(&input, DrCr::Dr, "210"),
        ]);
        post(&mut slot, "Purchase", "2026-02-06", vec![entry(&wien, DrCr::Cr, "200"), entry(&software, DrCr::Dr, "200")]);
        post(&mut slot, "Purchase", "2026-02-07", vec![entry(&wien, DrCr::Cr, "800"), line(&purchase, DrCr::Dr, "800", &laptop, "1", "800")]);
        let cancelled = post(&mut slot, "Sales", "2026-03-20", vec![
            entry(&klant, DrCr::Dr, "121"),
            line(&sales, DrCr::Cr, "100", &laptop, "1", "100"),
            entry(&output, DrCr::Cr, "21"),
        ]);
        cancel_voucher(&mut slot, &cancelled).unwrap();

        let r = aangifte_nl(&slot, "2026-01-01", "2026-03-31").unwrap();
        assert_eq!(r.tax_id.as_deref(), Some("NL004495445B01"));
        let a = &r.ret;
        // 1a: 2000 − 1000 = 1000 → 210; 1b: 40 → 3.60 → 3.
        assert_eq!((a.r1a, a.r1a_tax, a.r1b, a.r1b_tax), (dec!(1000), dec!(210), dec!(40), dec!(3)));
        assert_eq!((a.r3a, a.r3b), (dec!(1000), dec!(3500)), "goods to the US; goods and a service to Germany");
        assert_eq!((a.r4b, a.r4b_tax), (dec!(1000), dec!(210)), "the Austrian service and goods");
        // 5a: 210 + 3 + 210 = 423; 5b: 210 + 210 = 420.
        assert_eq!((a.r5a, a.r5b, a.r5g), (dec!(423), dec!(420), dec!(3)));
        assert!(!a.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", a.warnings);
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["r5g"], "3");
        assert!(aangifte_nl(&slot, "2026-03-31", "2026-01-01").unwrap_err().contains("before it starts"));
    }

    #[test]
    fn netherlands_only() {
        let (_f, slot) = indian_company("nl-india");
        assert!(aangifte_nl(&slot, "2026-01-01", "2026-03-31").unwrap_err().contains("Netherlands"));
    }
}

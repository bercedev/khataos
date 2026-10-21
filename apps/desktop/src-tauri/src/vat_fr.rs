//! France: the CA3 (déclaration de TVA 3310-CA3) from the books. The rules
//! live in `khata_fr::returns`; this module turns vouchers into its
//! documents.
//!
//! It also holds [`eu`], the voucher reading shared with `vat_nl.rs` and
//! `vat_es.rs`: the same split as `vat_de.rs` (which vouchers feed which
//! side, which lines are the value and which the tax, where the party is),
//! with each country's own tax-component name, rates and treatments.

use khata_core::Book;
use khata_fr::regions::{self, Territory as RegionTerritory};
use khata_fr::returns::{self, Ca3, DocKind, DocLine, InwardDoc, OutwardDoc, Period, Territory};
use khata_fr::tva::{self, Treatment};
use serde::Serialize;

use crate::api::{ApiResult, Slot, open_ref, parse_date};
use eu::Kind;

fn classify(class: &str, rate: rust_decimal::Decimal) -> Kind {
    match tva::treatment(class, rate) {
        Treatment::Taxed => Kind::Taxed,
        Treatment::Zero => Kind::Zero,
        Treatment::Exempt => Kind::Exempt,
        Treatment::ReverseCharge => Kind::Reverse,
        Treatment::IntraEu => Kind::IntraEu,
        Treatment::Export => Kind::Export,
        Treatment::OutOfScope => Kind::OutOfScope,
    }
}

/// France for the shared reader: "TVA" ledgers, Monaco counted as France.
pub(crate) const FRANCE: eu::Country = eu::Country {
    code: "FR",
    components: &[tva::TVA, "VAT"],
    in_eu: regions::in_eu,
    also_home: &["MC"],
    standard_class: tva::STANDARD_CLASS,
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

fn france_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_fr::COUNTRY.code) {
        return Err("the CA3 is for companies in France".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Ca3Dto {
    /// The company's TVA number as stored.
    pub tax_id: Option<String>,
    /// Which rate lines apply: "Continent", "Corse" or "Dom".
    pub territory: Territory,
    #[serde(flatten)]
    pub ret: Ca3,
}

/// The CA3 lines for a period (a month, or a quarter for small filers).
/// The company's région picks the rate lines (Corse, the DOM).
pub fn ca3_fr(slot: &Slot, from: &str, to: &str) -> ApiResult<Ca3Dto> {
    let book = open_ref(slot)?;
    france_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let mut warnings = Vec::new();
    let region = profile.tax_region.as_deref().and_then(regions::region);
    let territory = match region.map(|r| (r.code, r.territory)) {
        Some((_, RegionTerritory::NoTva)) => return Err("TVA does not apply in Guyane and Mayotte (art. 294 CGI): there is no CA3".into()),
        Some((_, RegionTerritory::Dom)) => Territory::Dom,
        Some(("20R", _)) => Territory::Corse,
        Some(_) => Territory::Continent,
        None => {
            warnings.push("the company has no région (Company → Tax registration); the continental rate lines are used".into());
            Territory::Continent
        }
    };
    let docs = eu::documents(book, period.from, period.to, &FRANCE, &mut warnings)?;
    let (mut outward, mut inward) = (Vec::new(), Vec::new());
    for d in docs {
        let kind = if d.credit { DocKind::CreditNote } else { DocKind::Invoice };
        let lines = d
            .lines
            .iter()
            .map(|l| DocLine { treatment: treatment(l.kind), goods: l.goods, taxable_value: l.value, percent: l.percent, tva: l.tax })
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
    let mut ret = returns::ca3(&outward, &inward, period, territory);
    ret.warnings.splice(0..0, warnings);
    Ok(Ca3Dto { tax_id: profile.tax_id.clone(), territory, ret })
}

/// Vouchers as EU VAT documents, shared by the France, Netherlands and
/// Spain returns.
///
/// Which vouchers feed which side (KhataOS records a sales return as a
/// Credit Note and a purchase return as a Debit Note):
/// - outward: Sales → invoice, Credit Note → credit note;
/// - inward: Purchase → invoice, Debit Note → the supplier's credit note.
///
/// Other voucher types are left out (a warning counts those that post to
/// tax ledgers). The party line carries the total; item lines (goods) and
/// other ledgers on the opposite side (services) are the value; ledgers
/// whose type of tax is the country's component (or "VAT") give the tax;
/// ledgers on the party's side are discounts and are not deducted (a
/// warning says so); a ledger named "Round off" is left out.
///
/// Each line's treatment comes from its tax class and the rate on file for
/// the voucher date; a line without a class is taken as STANDARD. Where the
/// party is (its mailing country) decides the rest when the voucher has no
/// tax:
/// - a sale to an EU business (an EU country other than home, with a VAT
///   id): goods → intra-EU supply, services → not taxable here;
/// - a sale outside the EU: goods → export, services → not taxable here;
/// - a purchase from an EU supplier: goods → intra-EU acquisition, services
///   → reverse charge; services from outside the EU → reverse charge;
///   goods from outside the EU (import VAT) are not modelled.
///
/// Self-assessed tax is at the class's rate, or at the STANDARD rate on
/// file when the class is 0%.
pub(crate) mod eu {
    use chrono::NaiveDate;
    use khata_compliance::RateSchedule;
    use khata_core::{Amount, Book, DrCr, Ledger, Voucher, VoucherKind};
    use rust_decimal::Decimal;

    use crate::api::{ApiResult, err, group_is_party};

    /// The treatments every pack shares; each maps them to its own enum.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Kind {
        Taxed,
        Zero,
        Exempt,
        Reverse,
        IntraEu,
        Export,
        OutOfScope,
    }

    impl Kind {
        pub(crate) fn taxed(self) -> bool {
            self == Kind::Taxed
        }
    }

    /// What differs between the countries.
    pub(crate) struct Country {
        /// ISO code of the home country ("FR").
        pub code: &'static str,
        /// The tax-ledger component names that count ("TVA", "VAT").
        pub components: &'static [&'static str],
        pub in_eu: fn(&str) -> bool,
        /// Countries that count as home besides `code` (Monaco for France).
        pub also_home: &'static [&'static str],
        pub standard_class: &'static str,
        /// The pack's treatment of a class at a rate.
        pub classify: fn(&str, Decimal) -> Kind,
    }

    impl Country {
        fn home(&self, c: &str) -> bool {
            c.eq_ignore_ascii_case(self.code) || self.also_home.iter().any(|h| c.eq_ignore_ascii_case(h))
        }
    }

    pub(crate) fn rates(book: &Book) -> ApiResult<RateSchedule> {
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

    pub(crate) fn label(kind: VoucherKind) -> &'static str {
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
    pub(crate) fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
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
    pub(crate) struct RawLine {
        pub name: String,
        /// Signed: negative when on the party's side.
        pub amount: Amount,
        pub class: Option<String>,
        pub goods: bool,
    }

    pub(crate) struct Parsed {
        pub party: Option<usize>,
        pub lines: Vec<RawLine>,
        pub tax: Amount,
        pub warnings: Vec<String>,
    }

    pub(crate) fn parse(book: &Book, v: &Voucher, c: &Country) -> Parsed {
        let name = format!("{} {}", label(v.kind), v.number);
        let party = party_line(book, v);
        let party_side = party.map(|i| v.entries[i].side);
        let mut p = Parsed { party, lines: Vec::new(), tax: Amount::ZERO, warnings: Vec::new() };
        for (i, e) in v.entries.iter().enumerate() {
            if Some(i) == party {
                continue;
            }
            let on_party_side = Some(e.side) == party_side;
            let signed = if on_party_side { -e.amount } else { e.amount };
            let ledger = book.ledger(e.ledger);
            let ledger_name = ledger.map(|l| l.name.clone()).unwrap_or_default();
            if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
                if c.components.iter().any(|n| t.component.eq_ignore_ascii_case(n)) {
                    p.tax += signed;
                } else {
                    p.warnings.push(format!("{name}: {ledger_name} collects {}, which is not {}; it is left out", t.component, c.components[0]));
                }
                continue;
            }
            if !e.items.is_empty() {
                for l in &e.items {
                    let item = book.stock_item(l.item);
                    p.lines.push(RawLine {
                        name: item.map(|i| i.name.clone()).unwrap_or_default(),
                        amount: if on_party_side { -l.amount } else { l.amount },
                        class: item.and_then(|i| i.tax_class.clone()),
                        goods: true,
                    });
                }
            } else if ledger_name.to_lowercase().contains("round") {
                p.warnings.push(format!("{name}: {ledger_name} (rounding) is left out"));
            } else if on_party_side {
                p.warnings.push(format!("{name}: {ledger_name} is on the party's side; it is not deducted from the value"));
            } else {
                p.lines.push(RawLine { name: ledger_name, amount: e.amount, class: ledger.and_then(|l| l.tax_class.clone()), goods: false });
            }
        }
        p
    }

    /// Where a party is: its mailing country, `None` for home or unknown.
    pub(crate) fn country(ledger: Option<&Ledger>, c: &Country) -> Option<String> {
        ledger
            .and_then(|l| l.mailing.as_ref())
            .and_then(|m| m.country.as_deref())
            .map(|x| x.trim().to_ascii_uppercase())
            .filter(|x| !x.is_empty() && !c.home(x))
    }

    pub(crate) fn tax_id(ledger: Option<&Ledger>) -> Option<String> {
        ledger.and_then(|l| l.tax_id.clone()).filter(|t| !t.trim().is_empty())
    }

    /// A VAT identifier with its two-letter prefix (a bare national number
    /// is not one), upper-cased and without spaces.
    pub(crate) fn vat_id(id: Option<String>) -> Option<String> {
        let n: String = id?.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
        (n.len() > 2 && n.as_bytes()[..2].iter().all(u8::is_ascii_uppercase)).then_some(n)
    }

    /// One treated line: treatment, goods, signed value, rate, tax share.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct Line {
        pub name: String,
        pub kind: Kind,
        pub goods: bool,
        pub value: Amount,
        pub percent: Decimal,
        pub tax: Amount,
    }

    /// The treatment and rate of each line, and the booked tax spread over
    /// the taxed lines by value × rate.
    pub(crate) fn treat(
        v: &Voucher,
        p: &Parsed,
        outward: bool,
        party: Option<&Ledger>,
        rates: &RateSchedule,
        c: &Country,
        warnings: &mut Vec<String>,
    ) -> ApiResult<Vec<Line>> {
        let name = format!("{} {}", label(v.kind), v.number);
        let abroad = country(party, c);
        let eu = abroad.as_deref().is_some_and(|x| (c.in_eu)(x));
        // A home-country VAT number (FR… for a French buyer) is not an EU one.
        let party_vat = vat_id(tax_id(party)).is_some_and(|id| !c.home(&id[..2]));
        let standard = rates.rate(c.standard_class, v.date);
        let mut out = Vec::new();
        for l in &p.lines {
            let class = l.class.clone().unwrap_or_else(|| c.standard_class.to_owned());
            let rate = rates
                .rate(&class, v.date)
                .ok_or_else(|| format!("{name}: no tax rate for {class:?} on {}: add it to the company's rate list", v.date))?;
            let mut k = (c.classify)(&class, rate);
            if p.tax.is_zero() && k.taxed() && let Some(x) = abroad.as_deref() {
                k = match (outward, eu, l.goods) {
                    (true, true, true) if party_vat => Kind::IntraEu,
                    (true, true, false) if party_vat => Kind::OutOfScope,
                    (true, false, true) => Kind::Export,
                    (true, false, false) => Kind::OutOfScope,
                    (false, true, true) => Kind::IntraEu,
                    (false, _, false) => Kind::Reverse,
                    (false, false, true) => {
                        warnings.push(format!("{name}: goods from {x} (import VAT) are not worked out"));
                        Kind::OutOfScope
                    }
                    _ => {
                        warnings.push(format!(
                            "{name}: a sale to {x} without the buyer's VAT id and without tax is reported as taxable at {}%",
                            rate.normalize()
                        ));
                        k
                    }
                };
            }
            let percent = match k {
                Kind::Reverse | Kind::IntraEu if !outward && rate.is_zero() => {
                    standard.ok_or_else(|| format!("{name}: no STANDARD rate on {} to self-assess the tax", v.date))?
                }
                _ => rate,
            };
            out.push(Line { name: l.name.clone(), kind: k, goods: l.goods, value: l.amount, percent, tax: Decimal::ZERO });
        }
        let mut taxed: Vec<usize> = (0..out.len()).filter(|&i| out[i].kind.taxed()).collect();
        if !p.tax.is_zero() && taxed.is_empty() {
            warnings.push(format!("{name} has tax but no taxed lines; its tax is put on its lines at the STANDARD rate"));
            for o in &mut out {
                o.kind = Kind::Taxed;
                o.percent = standard.unwrap_or(o.percent);
            }
            taxed = (0..out.len()).collect();
        }
        // Weight by value × rate, so that each rate gets its own tax.
        let weight = |o: &Line| o.value * if o.percent.is_zero() { Decimal::ONE } else { o.percent };
        let total: Decimal = taxed.iter().map(|&i| weight(&out[i])).sum();
        let last = taxed.iter().rev().copied().find(|&i| !out[i].value.is_zero());
        let mut left = p.tax;
        for &i in &taxed {
            let share = match last {
                Some(k) if i == k => left,
                Some(_) if !total.is_zero() => (p.tax * weight(&out[i]) / total).round_dp(2),
                _ => Decimal::ZERO,
            };
            left -= share;
            out[i].tax = share;
        }
        if !taxed.is_empty() {
            let expected: Decimal = taxed.iter().map(|&i| out[i].value * out[i].percent).sum::<Decimal>() / Decimal::ONE_HUNDRED;
            if (expected - p.tax).abs() > Decimal::ONE {
                warnings.push(format!("{name}: tax booked is {}, but the rates on file give {}", p.tax.normalize(), expected.round_dp(2).normalize()));
            }
        }
        Ok(out)
    }

    /// A voucher as a document of the return.
    pub(crate) struct Doc {
        pub id: String,
        pub outward: bool,
        /// A credit note (from the supplier's point of view).
        pub credit: bool,
        pub number: String,
        pub date: NaiveDate,
        pub party_name: String,
        pub party_country: Option<String>,
        pub party_vat_id: Option<String>,
        pub cancelled: bool,
        pub lines: Vec<Line>,
    }

    /// One voucher as a document; `None` for a voucher type the returns
    /// don't read.
    pub(crate) fn document(book: &Book, v: &Voucher, rates: &RateSchedule, c: &Country, warnings: &mut Vec<String>) -> ApiResult<Option<Doc>> {
        let outward = match v.kind {
            VoucherKind::Sales | VoucherKind::CreditNote => true,
            VoucherKind::Purchase | VoucherKind::DebitNote => false,
            _ => return Ok(None),
        };
        let p = parse(book, v, c);
        warnings.extend(p.warnings.iter().cloned());
        let party = p.party.and_then(|i| book.ledger(v.entries[i].ledger));
        let lines = treat(v, &p, outward, party, rates, c, warnings)?;
        Ok(Some(Doc {
            id: v.id.0.to_string(),
            outward,
            credit: matches!(v.kind, VoucherKind::CreditNote | VoucherKind::DebitNote),
            number: v.number.clone(),
            date: v.date,
            party_name: party.map(|l| l.name.clone()).unwrap_or_default(),
            party_country: country(party, c),
            party_vat_id: vat_id(tax_id(party)),
            cancelled: v.cancelled,
            lines,
        }))
    }

    /// Every Sales, Credit Note, Purchase and Debit Note voucher dated in
    /// the period, by date and number, and a warning for other vouchers
    /// that post to tax ledgers.
    pub(crate) fn documents(
        book: &Book,
        from: NaiveDate,
        to: NaiveDate,
        c: &Country,
        warnings: &mut Vec<String>,
    ) -> ApiResult<Vec<Doc>> {
        let rates = rates(book)?;
        let mut vouchers: Vec<&Voucher> = book.vouchers().filter(|v| from <= v.date && v.date <= to).collect();
        vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
        let mut out = Vec::new();
        let mut other = 0;
        for v in vouchers {
            match document(book, v, &rates, c, warnings)? {
                Some(d) => out.push(d),
                None => {
                    let taxed = v.entries.iter().any(|e| book.ledger(e.ledger).is_some_and(|l| l.tax.is_some()));
                    other += usize::from(taxed && !v.cancelled);
                }
            }
        }
        if other > 0 {
            warnings.push(format!("{other} journal or other voucher(s) post to tax ledgers; they are not in this return"));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, indian_company, ledger, voucher};
    use crate::api::{cancel_voucher, check_tax_id, create_company, post_voucher, set_country};
    use crate::masters::save_unit;
    use crate::tax::{InvoiceTaxInput, InvoiceTaxLine, invoice_tax, set_company_tax, set_ledger_tax, set_tax_rate};
    use crate::vat_nl::tests::{item, items, party};
    use chrono::Datelike;
    use khata_core::{DrCr, TaxSide, VoucherKind};
    use rust_decimal_macros::dec;

    fn fr_company(name: &str, region: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Exemple SAS").unwrap();
        set_country(&mut slot, Some(&f.0), "FR").unwrap();
        set_company_tax(&mut slot, "FR44732829320", region).unwrap();
        for (class, percent) in
            [("STANDARD", "20"), ("INTERMEDIATE", "10"), ("REDUCED", "5.5"), ("SPECIAL", "2.1"), ("DOM-STANDARD", "8.5"), ("EXEMPT", "0"), ("REVERSE", "0")]
        {
            set_tax_rate(&mut slot, class, "2014-01-01", percent).unwrap();
        }
        (f, slot)
    }

    #[test]
    fn a_french_company_checks_ids_and_charges_tva_by_region() {
        let f = TempFile::new("fr-ids");
        let mut slot = None;
        create_company(&mut slot, &f.0, "Exemple SAS").unwrap();
        let info = set_country(&mut slot, Some(&f.0), "fr").unwrap();
        let country = info.country.unwrap();
        assert_eq!((country.code, country.currency, country.tax_id_label), ("FR", "EUR", "N° TVA"));
        assert_eq!((info.fy_start.month(), info.fy_start.day()), (1, 1), "calendar years");
        assert_eq!(check_tax_id(&slot, "fr 40 303 265 045").unwrap().error, None);
        assert!(check_tax_id(&slot, "FR41303265045").unwrap().error.is_some());
        assert!(set_company_tax(&mut slot, "FR44732829320", "75").is_err(), "a département is not a région");
        let tax = set_company_tax(&mut slot, "FR44732829320", "974").unwrap();
        assert_eq!((tax.region_name.as_deref(), tax.regions.len()), (Some("La Réunion"), 18));

        let (_g, mut slot) = fr_company("fr-reunion", "974");
        let client = party(&mut slot, "Client Réunion", "Sundry Debtors", None, "FR");
        let t = invoice_tax(&slot, &InvoiceTaxInput {
            kind: VoucherKind::Sales,
            date: "2026-05-01".into(),
            party_id: client,
            lines: vec![InvoiceTaxLine { item_id: String::new(), ledger_id: String::new(), tax_class: "STANDARD".into(), taxable_value: "1000".into() }],
        })
        .unwrap();
        let rows: Vec<_> = t.rows.iter().map(|r| (r.component.as_str(), r.percent.as_str(), r.amount.as_str())).collect();
        assert_eq!(rows, [("TVA", "8.5", "85")], "La Réunion's rate: {:?}", t.warnings);
    }

    #[test]
    fn ca3_from_the_books() {
        let (_f, mut slot) = fr_company("fr-ca3", "IDF");
        let client = party(&mut slot, "Client SARL", "Sundry Debtors", Some("FR40303265045"), "FR");
        let berlin = party(&mut slot, "Berlin GmbH", "Sundry Debtors", Some("DE136695976"), "DE");
        let fournisseur = party(&mut slot, "Fournisseur SA", "Sundry Creditors", Some("FR83404833048"), "FR");
        let wien = party(&mut slot, "Wien GmbH", "Sundry Creditors", Some("ATU13585627"), "AT");
        let ventes = ledger(&mut slot, "Ventes", "Sales Accounts", "");
        let conseil = ledger(&mut slot, "Conseil", "Sales Accounts", "");
        let achats = ledger(&mut slot, "Achats", "Purchase Accounts", "");
        let logiciel = ledger(&mut slot, "Logiciel", "Indirect Expenses", "");
        let collectee = ledger(&mut slot, "TVA collectée", "Duties & Taxes", "");
        let deductible = ledger(&mut slot, "TVA déductible", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &collectee, "TVA", TaxSide::Output).unwrap();
        set_ledger_tax(&mut slot, &deductible, "TVA", TaxSide::Input).unwrap();
        let u = save_unit(&mut slot, "", "U", "", 0).unwrap().id;
        let ordinateur = item(&mut slot, "Ordinateur", &u, "STANDARD");
        let livre = item(&mut slot, "Livre", &u, "REDUCED");
        let line = |ledger: &str, side: DrCr, amount: &str, item_id: &str, qty: &str, rate: &str| {
            let mut e = entry(ledger, side, amount);
            e.items = items(item_id, qty, rate);
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap().id;
        let mut l = line(&ventes, DrCr::Cr, "2040", &ordinateur, "2", "1000");
        l.items.extend(items(&livre, "1", "40"));
        post(&mut slot, "Sales", "2026-03-02", vec![entry(&client, DrCr::Dr, "2442.20"), l, entry(&collectee, DrCr::Cr, "402.20")]);
        post(&mut slot, "CreditNote", "2026-03-10", vec![
            entry(&client, DrCr::Cr, "1200"),
            line(&ventes, DrCr::Dr, "1000", &ordinateur, "1", "1000"),
            entry(&collectee, DrCr::Dr, "200"),
        ]);
        post(&mut slot, "Sales", "2026-03-11", vec![entry(&berlin, DrCr::Dr, "3000"), line(&ventes, DrCr::Cr, "3000", &ordinateur, "3", "1000")]);
        post(&mut slot, "Sales", "2026-03-12", vec![entry(&berlin, DrCr::Dr, "500"), entry(&conseil, DrCr::Cr, "500")]);
        post(&mut slot, "Purchase", "2026-03-05", vec![
            entry(&fournisseur, DrCr::Cr, "1200"),
            line(&achats, DrCr::Dr, "1000", &ordinateur, "1", "1000"),
            entry(&deductible, DrCr::Dr, "200"),
        ]);
        post(&mut slot, "Purchase", "2026-03-06", vec![entry(&wien, DrCr::Cr, "200"), entry(&logiciel, DrCr::Dr, "200")]);
        post(&mut slot, "Purchase", "2026-03-07", vec![entry(&wien, DrCr::Cr, "800"), line(&achats, DrCr::Dr, "800", &ordinateur, "1", "800")]);
        let cancelled = post(&mut slot, "Sales", "2026-03-20", vec![
            entry(&client, DrCr::Dr, "120"),
            line(&ventes, DrCr::Cr, "100", &ordinateur, "1", "100"),
            entry(&collectee, DrCr::Cr, "20"),
        ]);
        cancel_voucher(&mut slot, &cancelled).unwrap();

        let r = ca3_fr(&slot, "2026-03-01", "2026-03-31").unwrap();
        assert_eq!((r.tax_id.as_deref(), r.territory), (Some("FR44732829320"), Territory::Continent));
        let c = &r.ret;
        assert_eq!((c.a1, c.b5, c.f2, c.e2), (dec!(2040), dec!(1000), dec!(3000), dec!(500)));
        assert_eq!((c.a3, c.b2), (dec!(200), dec!(800)));
        // 08: 2000 + 200 + 800 = 3000 → 600; 09: 40 → 2.20 → 2.
        assert_eq!((c.l08.base, c.l08.tax, c.l09.base, c.l09.tax), (dec!(3000), dec!(600), dec!(40), dec!(2)));
        assert_eq!((c.l16, c.l17, c.l20, c.l21, c.l23), (dec!(602), dec!(160), dec!(400), dec!(200), dec!(600)));
        assert_eq!((c.td, c.l28, c.l32), (dec!(2), dec!(2), dec!(2)));
        assert!(!c.warnings.iter().any(|w| w.contains("rates on file")), "{:?}", c.warnings);
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!((j["l32"].as_str(), j["territory"].as_str()), (Some("2"), Some("Continent")));
        assert!(ca3_fr(&slot, "2026-03-31", "2026-03-01").unwrap_err().contains("before it starts"));
    }

    #[test]
    fn france_only_and_no_ca3_in_mayotte() {
        let (_f, slot) = indian_company("fr-india");
        assert!(ca3_fr(&slot, "2026-03-01", "2026-03-31").unwrap_err().contains("France"));
        let (_g, slot) = fr_company("fr-mayotte", "976");
        assert!(ca3_fr(&slot, "2026-03-01", "2026-03-31").unwrap_err().contains("Mayotte"));
    }
}

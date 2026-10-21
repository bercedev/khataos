//! The Flat Rate Scheme (VAT Notice 733) and the Cash Accounting Scheme
//! (VAT Notice 731), on top of the documents of [`crate::returns`].
//!
//! Sources (gov.uk, checked 21 Oct 2026 [PRIMARY]; see
//! `docs/research/uk-compliance.md` §1.8):
//! - Notice 733 §§3.1, 3.5 (join at £150,000 expected taxable turnover
//!   excluding VAT; leave above £230,000 income including VAT), §4.4 and
//!   §4.6 (limited cost business: 16.5%, relevant goods below 2% of flat
//!   rate turnover or £1,000 a year, £250 a quarter), §4.7 (1% off in the
//!   first year of VAT registration), §6.2 (flat rate turnover: all
//!   supplies including VAT, zero-rated and exempt ones included), §6.4 and
//!   §6.5 (reverse charges and postponed import VAT stay outside the flat
//!   rate), §9.1 (cash-based turnover), §15.2 (a single purchase of capital
//!   expenditure goods of £2,000 or more including VAT is reclaimed in
//!   Box 4); the sector table from "VAT Flat Rate Scheme: how much you pay";
//!   Notice 700/12 §4.1 (the boxes under the scheme);
//! - Notice 731 §§2.1, 2.5 (join at £1.35m, leave above £1.6m), §§4.4 to
//!   4.7 (VAT on the dates payments are received and made; Boxes 6 and 7 on
//!   payments too), §5.7 and §8 (payments go to invoices in the order they
//!   were issued or received; a part payment carries the invoice's VAT in
//!   proportion), §§2.7, 2.8, 5.5 (imports, acquisitions and the domestic
//!   reverse charge stay on the normal rules; not with the Flat Rate
//!   Scheme); Notice 700/12 §4.2.
//!
//! Simplifications, each also in `docs/uk-vat.md`:
//! - A "single purchase" of capital goods is taken to be one purchase line
//!   posted to a fixed-asset ledger [UNCERTAIN: several lines of one
//!   invoice for one asset should be added up by hand].
//! - The limited cost test counts the stock-item lines of purchases (VAT
//!   included, capital lines excluded); Notice 733 §4.6 also excludes food
//!   and drink for staff, vehicles and fuel, and goods for resale in some
//!   trades, which the books cannot tell apart: the user can force the
//!   answer ([`LimitedCost`]).
//! - Under the scheme, reverse-charge VAT goes in Box 1 and, as for imported
//!   services (§6.4), is reclaimed in Box 4; for the domestic reverse charge
//!   the reclaim is [UNCERTAIN] (a warning says so). Import VAT (paid at the
//!   border or postponed) is not reclaimed, except on capital goods, which
//!   the user must add [UNCERTAIN for postponed VAT].
//! - Box 8 is part of the flat rate turnover here, so it is not added to
//!   Box 6 a second time [UNCERTAIN].

use std::collections::BTreeMap;

use chrono::{Datelike, Months, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::returns::{DocKind, DocLine, InwardDoc, OutwardDoc, Period, VatReturn, assemble, label, parts};
use crate::vat::Mechanism;

const fn pct(units: i64, scale: u32) -> Decimal {
    Decimal::from_parts(units as u32, 0, 0, false, scale)
}

/// The limited cost business rate (Notice 733 §4.4).
pub const LIMITED_COST_PERCENT: Decimal = pct(165, 1);
/// Taken off the flat rate in the first year of VAT registration (§4.7).
pub const FIRST_YEAR_DISCOUNT: Decimal = pct(1, 0);
/// A single capital purchase of at least this, VAT included, is reclaimed
/// in Box 4 (§15.2).
pub const CAPITAL_GOODS_MIN: Decimal = pct(2000, 0);
/// Relevant goods below this share (%) of flat rate turnover make a
/// limited cost business (§4.4).
pub const LIMITED_COST_SHARE_PERCENT: Decimal = pct(2, 0);
/// …or below this a year, pro rata for shorter periods (£250 a quarter).
pub const LIMITED_COST_YEARLY: Decimal = pct(1000, 0);
/// Join the Flat Rate Scheme with expected taxable turnover up to this,
/// VAT excluded (§3.1).
pub const FLAT_RATE_JOIN_LIMIT: Decimal = pct(150_000, 0);
/// Leave it when income for the year, VAT included, is above this (§3.5).
pub const FLAT_RATE_LEAVE_LIMIT: Decimal = pct(230_000, 0);
/// Join the Cash Accounting Scheme with expected taxable supplies up to
/// this (Notice 731 §2.1).
pub const CASH_JOIN_LIMIT: Decimal = pct(1_350_000, 0);
/// Leave it when taxable supplies in 12 months are above this (§2.5).
pub const CASH_LEAVE_LIMIT: Decimal = pct(1_600_000, 0);

/// A Flat Rate Scheme sector and its percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sector {
    pub name: &'static str,
    pub percent: Decimal,
}

const fn s(name: &'static str, units: i64, scale: u32) -> Sector {
    Sector { name, percent: pct(units, scale) }
}

/// The flat rates in force (gov.uk "VAT Flat Rate Scheme: how much you
/// pay", checked 21 Oct 2026 [PRIMARY]; catering, hotels and pubs from
/// 1 Apr 2022). Rates change: the company stores its own percentage, and
/// this list only fills the settings dialog.
pub const SECTORS: &[Sector] = &[
    s("Accountancy or book-keeping", 145, 1),
    s("Advertising", 11, 0),
    s("Agricultural services", 11, 0),
    s("Any other activity not listed elsewhere", 12, 0),
    s("Architect, civil and structural engineer or surveyor", 145, 1),
    s("Boarding or care of animals", 12, 0),
    s("Business services not listed elsewhere", 12, 0),
    s("Catering services including restaurants and takeaways", 125, 1),
    s("Computer and IT consultancy or data processing", 145, 1),
    s("Computer repair services", 105, 1),
    s("Entertainment or journalism", 125, 1),
    s("Estate agency or property management services", 12, 0),
    s("Farming or agriculture not listed elsewhere", 65, 1),
    s("Film, radio, television or video production", 13, 0),
    s("Financial services", 135, 1),
    s("Forestry or fishing", 105, 1),
    s("General building or construction services", 95, 1),
    s("Hairdressing or other beauty treatment services", 13, 0),
    s("Hiring or renting goods", 95, 1),
    s("Hotel or accommodation", 105, 1),
    s("Investigation or security", 12, 0),
    s("Labour-only building or construction services", 145, 1),
    s("Laundry or dry-cleaning services", 12, 0),
    s("Lawyer or legal services", 145, 1),
    s("Library, archive, museum or other cultural activity", 95, 1),
    s("Management consultancy", 14, 0),
    s("Manufacturing fabricated metal products", 105, 1),
    s("Manufacturing food", 9, 0),
    s("Manufacturing not listed elsewhere", 95, 1),
    s("Manufacturing yarn, textiles or clothing", 9, 0),
    s("Membership organisation", 8, 0),
    s("Mining or quarrying", 10, 0),
    s("Packaging", 9, 0),
    s("Photography", 11, 0),
    s("Post offices", 5, 0),
    s("Printing", 85, 1),
    s("Publishing", 11, 0),
    s("Pubs", 65, 1),
    s("Real estate activity not listed elsewhere", 14, 0),
    s("Repairing personal or household goods", 10, 0),
    s("Repairing vehicles", 85, 1),
    s("Retailing food, confectionery, tobacco, newspapers or children's clothing", 4, 0),
    s("Retailing pharmaceuticals, medical goods, cosmetics or toiletries", 8, 0),
    s("Retailing not listed elsewhere", 75, 1),
    s("Retailing vehicles or fuel", 65, 1),
    s("Secretarial services", 13, 0),
    s("Social work", 11, 0),
    s("Sport or recreation", 85, 1),
    s("Transport or storage, including couriers, freight, removals and taxis", 10, 0),
    s("Travel agency", 105, 1),
    s("Veterinary medicine", 11, 0),
    s("Wholesaling agricultural products", 8, 0),
    s("Wholesaling food", 75, 1),
    s("Wholesaling not listed elsewhere", 85, 1),
];

/// Whether the business is a limited cost business this period.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitedCost {
    /// Worked out from the purchases (see the module notes).
    #[default]
    Auto,
    /// Always 16.5%.
    Yes,
    /// Never: the sector rate.
    No,
}

/// The company's Flat Rate Scheme settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatRate {
    /// The sector, as on HMRC's list (for the record).
    pub sector: String,
    /// The sector's flat rate (%), as HMRC confirmed it.
    pub percent: Decimal,
    /// The VAT registration date: 1% comes off the rate for supplies in the
    /// first 12 months from it (§4.7). `None`: no reduction.
    #[serde(default)]
    pub registered_on: Option<NaiveDate>,
    #[serde(default)]
    pub limited_cost: LimitedCost,
    /// Cash-based turnover (§9): the rate applies to payments received.
    #[serde(default)]
    pub cash_turnover: bool,
}

/// The flat rate turnover taxed at one percentage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RateSegment {
    pub percent: Decimal,
    /// VAT-inclusive turnover.
    pub turnover: Decimal,
    pub vat: Decimal,
}

/// How the Flat Rate Scheme figures were reached, for the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlatRateDetails {
    pub sector: String,
    pub sector_percent: Decimal,
    /// The 16.5% rate applied.
    pub limited_cost: bool,
    /// Goods bought, VAT included (the test's figure; see the module notes).
    pub relevant_goods: Decimal,
    /// The test's limit: the higher of 2% of turnover and £1,000 a year pro
    /// rata.
    pub limited_cost_limit: Decimal,
    /// Flat rate turnover, VAT included.
    pub turnover: Decimal,
    pub segments: Vec<RateSegment>,
    /// Flat rate × turnover, before the reverse charges and import VAT.
    pub flat_rate_vat: Decimal,
    /// Capital goods of £2,000 or more reclaimed in Box 4.
    pub capital_value: Decimal,
    pub capital_vat: Decimal,
    pub cash_turnover: bool,
}

/// Whole calendar months the period touches (at least 1).
fn months(period: Period) -> u32 {
    let a = period.from.year() * 12 + period.from.month0() as i32;
    let b = period.to.year() * 12 + period.to.month0() as i32;
    (b - a + 1).max(1) as u32
}

/// The 9-box return under the Flat Rate Scheme (Notice 700/12 §4.1):
/// - Box 1: the flat rate × flat rate turnover (VAT included; 1% less in
///   the first year; 16.5% for a limited cost business), plus reverse-charge
///   VAT and postponed import VAT, which stay outside the flat rate;
/// - Box 4: VAT on single capital purchases of £2,000 or more (VAT
///   included), plus the reverse-charge VAT;
/// - Box 6: flat rate turnover, VAT included, plus the value of supplies
///   outside the scheme (reverse-charge sales and imported services);
/// - Box 7: those capital purchases, reverse-charge purchases and imports,
///   VAT excluded; Box 9.
pub fn flat_rate_return(outward: &[OutwardDoc], inward: &[InwardDoc], period: Period, frs: &FlatRate) -> (VatReturn, FlatRateDetails) {
    let p = parts(outward, inward, period);
    let mut warnings = p.warnings.clone();
    let turnover: Decimal = p.sales_gross.iter().map(|(_, g)| *g).sum();
    let limit = (turnover * LIMITED_COST_SHARE_PERCENT / Decimal::ONE_HUNDRED)
        .max(LIMITED_COST_YEARLY * Decimal::from(months(period)) / Decimal::from(12))
        .round_dp(2);
    let limited_cost = match frs.limited_cost {
        LimitedCost::Yes => true,
        LimitedCost::No => false,
        LimitedCost::Auto => p.goods_gross < limit,
    };
    if frs.limited_cost == LimitedCost::Auto {
        warnings.push(format!(
            "Limited cost business test: goods bought {} against a limit of {}, so the {} rate applies; the books cannot leave out food, vehicles and fuel or capital items bought as stock, so check it (Notice 733 §4.6) and set the answer in the scheme settings if it is wrong",
            p.goods_gross.round_dp(2).normalize(),
            limit.normalize(),
            if limited_cost { "16.5% limited cost" } else { "sector" }
        ));
    }
    let base = if limited_cost { LIMITED_COST_PERCENT } else { frs.percent };
    let first_year_end = frs.registered_on.and_then(|r| r.checked_add_months(Months::new(12)));
    let mut by_rate: BTreeMap<Decimal, Decimal> = BTreeMap::new();
    for (date, gross) in &p.sales_gross {
        let discounted = first_year_end.is_some_and(|end| *date < end) && frs.registered_on.is_some_and(|r| *date >= r);
        let rate = if discounted { (base - FIRST_YEAR_DISCOUNT).max(Decimal::ZERO) } else { base };
        *by_rate.entry(rate).or_default() += *gross;
    }
    let segments: Vec<RateSegment> = by_rate
        .into_iter()
        .rev()
        .map(|(percent, turnover)| RateSegment { percent, turnover, vat: (turnover * percent / Decimal::ONE_HUNDRED).round_dp(2) })
        .collect();
    let flat_rate_vat: Decimal = segments.iter().map(|s| s.vat).sum();
    let (mut capital_value, mut capital_vat) = (Decimal::ZERO, Decimal::ZERO);
    for (value, vat) in &p.capital {
        if (value + vat).abs() >= CAPITAL_GOODS_MIN {
            capital_value += value;
            capital_vat += vat;
        }
    }
    if !p.drc_vat.is_zero() {
        warnings.push("Domestic reverse charge under the Flat Rate Scheme: its VAT is in Box 1 (outside the flat rate) and reclaimed in Box 4 as for imported services; check that reclaim with HMRC's construction reverse charge guide".to_owned());
    }
    if !p.import_vat.is_zero() {
        warnings.push(format!(
            "Import VAT of {} paid at the border is not reclaimed under the Flat Rate Scheme (only on capital goods of £2,000 or more: add those to Box 4 by hand)",
            p.import_vat.normalize()
        ));
    }
    if !p.pva_vat.is_zero() {
        warnings.push(format!(
            "Postponed import VAT of {} is in Box 1 after the flat rate (Notice 733 §6.5) and not reclaimed in Box 4, except on capital goods of £2,000 or more (add those by hand)",
            p.pva_vat.normalize()
        ));
    }
    if months(period) >= 12 && turnover > FLAT_RATE_LEAVE_LIMIT {
        warnings.push(format!(
            "Flat rate turnover is {} for the year, above £230,000: the business must leave the Flat Rate Scheme (Notice 733 §3.5)",
            turnover.normalize()
        ));
    }
    let box1 = flat_rate_vat + p.rc_vat + p.drc_vat + p.pva_vat;
    let box4 = capital_vat + p.rc_vat + p.drc_vat;
    let box6 = turnover + p.outside_sales_value + p.rc_value;
    let box7 = capital_value + p.rc_value + p.drc_value + p.import_value + p.pva_value + p.box9;
    let ret = assemble(period, box1, box4, box6, box7, p.box8, p.box9, warnings);
    let details = FlatRateDetails {
        sector: frs.sector.clone(),
        sector_percent: frs.percent,
        limited_cost,
        relevant_goods: p.goods_gross,
        limited_cost_limit: limit,
        turnover,
        segments,
        flat_rate_vat,
        capital_value,
        capital_vat,
        cash_turnover: frs.cash_turnover,
    };
    (ret, details)
}

/// Money received or paid against one invoice (or a purchase invoice),
/// VAT included. Negative: a refund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Payment {
    /// The [`OutwardDoc::id`] or [`InwardDoc::id`] it pays.
    pub doc_id: String,
    pub date: NaiveDate,
    pub amount: Decimal,
}

/// A credit note (or the supplier's credit note, a purchase return) set
/// against an invoice: it reduces what is left to pay, and is not counted on
/// its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Credit {
    pub invoice_id: String,
    pub credit_id: String,
}

fn gross(lines: &[DocLine]) -> Decimal {
    lines.iter().map(|l| l.taxable_value + if l.treatment.taxed() { l.vat } else { Decimal::ZERO }).sum()
}

fn scaled(lines: &[DocLine], f: Decimal) -> Vec<DocLine> {
    lines
        .iter()
        .map(|l| DocLine { taxable_value: (l.taxable_value * f).round_dp(2), vat: (l.vat * f).round_dp(2), ..l.clone() })
        .collect()
}

/// One side's documents on the cash basis.
struct Side<'a, D> {
    docs: &'a [D],
}

trait Doc: Clone {
    fn id(&self) -> &str;
    fn kind(&self) -> DocKind;
    fn date(&self) -> NaiveDate;
    fn number(&self) -> &str;
    fn cancelled(&self) -> bool;
    fn lines(&self) -> &[DocLine];
    fn with(&self, kind: DocKind, date: NaiveDate, number: String, lines: Vec<DocLine>) -> Self;
    fn ni_eu(&self) -> bool;
}

macro_rules! doc_impl {
    ($t:ty) => {
        impl Doc for $t {
            fn id(&self) -> &str {
                &self.id
            }
            fn kind(&self) -> DocKind {
                self.kind
            }
            fn date(&self) -> NaiveDate {
                self.date
            }
            fn number(&self) -> &str {
                &self.number
            }
            fn cancelled(&self) -> bool {
                self.cancelled
            }
            fn lines(&self) -> &[DocLine] {
                &self.lines
            }
            fn with(&self, kind: DocKind, date: NaiveDate, number: String, lines: Vec<DocLine>) -> Self {
                Self { kind, date, number, lines, ..self.clone() }
            }
            fn ni_eu(&self) -> bool {
                self.ni_eu_goods
            }
        }
    };
}
doc_impl!(OutwardDoc);
doc_impl!(InwardDoc);

impl<D: Doc> Side<'_, D> {
    fn cash(&self, payments: &[Payment], credits: &[Credit], period: Period, warnings: &mut Vec<String>) -> Vec<D> {
        let mut out = Vec::new();
        let by_id: BTreeMap<&str, &D> = self.docs.iter().filter(|d| !d.cancelled()).map(|d| (d.id(), d)).collect();
        let credited: BTreeMap<&str, Vec<&D>> = credits.iter().fold(BTreeMap::new(), |mut m, c| {
            if let Some(cn) = by_id.get(c.credit_id.as_str()) {
                m.entry(c.invoice_id.as_str()).or_insert_with(Vec::new).push(*cn);
            }
            m
        });
        let linked: std::collections::BTreeSet<&str> = credits.iter().map(|c| c.credit_id.as_str()).collect();
        let normal = |l: &&DocLine| l.mechanism == Mechanism::Normal;
        // Imports, acquisitions and reverse charges stay on the normal
        // rules (Notice 731 §§2.7, 5.5): their lines on the document date.
        for d in self.docs.iter().filter(|d| !d.cancelled() && period.contains(d.date())) {
            let special: Vec<DocLine> = d.lines().iter().filter(|l| l.mechanism != Mechanism::Normal).cloned().collect();
            if !special.is_empty() {
                out.push(d.with(d.kind(), d.date(), d.number().to_owned(), special));
            }
            if d.ni_eu() {
                warnings.push(format!(
                    "{} {}: goods trade between Northern Ireland and the EU (Boxes 8 and 9) should be on the value of supplies made, not payments; check those boxes",
                    label(d.kind()),
                    d.number()
                ));
            }
            if d.kind() == DocKind::CreditNote && !linked.contains(d.id()) && d.lines().iter().any(|l| l.mechanism == Mechanism::Normal) {
                warnings.push(format!(
                    "{} {} is not set against an invoice; it is counted on its own date",
                    label(d.kind()),
                    d.number()
                ));
                let lines: Vec<DocLine> = d.lines().iter().filter(normal).cloned().collect();
                out.push(d.with(d.kind(), d.date(), d.number().to_owned(), lines));
            }
        }
        // Payments: the invoice's normal lines, less its credit notes, in
        // proportion to the payment.
        let mut paid: BTreeMap<&str, Decimal> = BTreeMap::new();
        let mut sorted: Vec<&Payment> = payments.iter().collect();
        sorted.sort_by_key(|p| p.date);
        for p in sorted {
            let Some(d) = by_id.get(p.doc_id.as_str()) else { continue };
            let mut lines: Vec<DocLine> = d.lines().iter().filter(normal).cloned().collect();
            for cn in credited.get(d.id()).into_iter().flatten() {
                lines.extend(cn.lines().iter().filter(normal).map(|l| DocLine {
                    taxable_value: -l.taxable_value,
                    vat: -l.vat,
                    ..l.clone()
                }));
            }
            let total = gross(&lines);
            let before = paid.get(d.id()).copied().unwrap_or_default();
            let mut amount = p.amount;
            if total > Decimal::ZERO && before + amount > total {
                if period.contains(p.date) {
                    warnings.push(format!(
                        "{} {} is paid {} more than it is for; the excess is left out",
                        label(d.kind()),
                        d.number(),
                        (before + amount - total).normalize()
                    ));
                }
                amount = (total - before).max(Decimal::ZERO);
            }
            *paid.entry(d.id()).or_default() += amount;
            if !period.contains(p.date) || amount.is_zero() {
                continue;
            }
            if total.is_zero() {
                warnings.push(format!("{} {} has nothing to pay; its payment is left out", label(d.kind()), d.number()));
                continue;
            }
            let f = amount / total;
            out.push(d.with(DocKind::Invoice, p.date, format!("{} (paid {})", d.number(), p.date), scaled(&lines, f)));
        }
        out
    }
}

/// The documents of a period on the cash basis (Notice 731): each payment
/// dated in the period becomes a document dated on the payment, carrying
/// its invoice's lines (less the credit notes set against it) in the
/// proportion paid. Reverse-charge and import lines stay on their document
/// date; a credit note not set against an invoice counts on its own date.
/// Feed the result to [`crate::returns::vat_return`] (or, for cash-based
/// turnover, the outward side to [`flat_rate_return`]).
pub fn cash_basis(
    outward: &[OutwardDoc],
    inward: &[InwardDoc],
    payments: &[Payment],
    credits: &[Credit],
    period: Period,
) -> (Vec<OutwardDoc>, Vec<InwardDoc>, Vec<String>) {
    let mut warnings = Vec::new();
    let o = Side { docs: outward }.cash(payments, credits, period, &mut warnings);
    let i = Side { docs: inward }.cash(payments, credits, period, &mut warnings);
    (o, i, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::returns::vat_return;
    use crate::vat::Treatment;
    use rust_decimal_macros::dec;

    const VRN: &str = "GB562235945";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn q2() -> Period {
        Period { from: d(2026, 4, 1), to: d(2026, 6, 30) }
    }

    fn line(treatment: Treatment, value: Decimal, vat: Decimal) -> DocLine {
        DocLine { treatment, goods: false, taxable_value: value, vat, mechanism: Mechanism::Normal, capital: false }
    }

    fn std(value: Decimal) -> DocLine {
        line(Treatment::Standard, value, value * dec!(0.2))
    }

    fn sale(id: &str, kind: DocKind, date: NaiveDate, lines: Vec<DocLine>) -> OutwardDoc {
        OutwardDoc { id: id.into(), kind, number: id.into(), date, party_name: "C".into(), ni_eu_goods: false, cancelled: false, lines }
    }

    fn purchase(id: &str, date: NaiveDate, lines: Vec<DocLine>) -> InwardDoc {
        InwardDoc {
            id: id.into(),
            kind: DocKind::Invoice,
            number: id.into(),
            date,
            party_name: "S".into(),
            party_vrn: Some(VRN.into()),
            ni_eu_goods: false,
            cancelled: false,
            lines,
        }
    }

    fn frs(percent: Decimal) -> FlatRate {
        FlatRate { sector: "Computer and IT consultancy or data processing".into(), percent, registered_on: None, limited_cost: LimitedCost::No, cash_turnover: false }
    }

    #[test]
    fn sector_table_holds_the_published_rates() {
        assert_eq!(SECTORS.len(), 54);
        let rate = |n: &str| SECTORS.iter().find(|s| s.name == n).unwrap().percent;
        assert_eq!(rate("Accountancy or book-keeping"), dec!(14.5));
        assert_eq!(rate("Retailing food, confectionery, tobacco, newspapers or children's clothing"), dec!(4));
        assert_eq!(rate("Pubs"), dec!(6.5));
        assert_eq!(LIMITED_COST_PERCENT, dec!(16.5));
        assert!(SECTORS.iter().all(|s| s.percent > dec!(0) && s.percent < dec!(16.5)));
    }

    #[test]
    fn flat_rate_applies_to_vat_inclusive_turnover_of_every_supply() {
        let outward = [
            sale("1", DocKind::Invoice, d(2026, 4, 10), vec![std(dec!(10000)), line(Treatment::ZeroRated, dec!(500), dec!(0)), line(Treatment::Exempt, dec!(500), dec!(0))]),
            sale("CN", DocKind::CreditNote, d(2026, 5, 1), vec![std(dec!(1000))]),
        ];
        let inward = [purchase("P", d(2026, 4, 2), vec![std(dec!(3000))])];
        let (r, x) = flat_rate_return(&outward, &inward, q2(), &frs(dec!(14.5)));
        // Turnover: 12000 + 500 + 500 − 1200 = 11800.
        assert_eq!(x.turnover, dec!(11800));
        assert_eq!(x.flat_rate_vat, dec!(1711.00));
        assert_eq!((r.vat_due_sales, r.vat_reclaimed, r.net_vat_due), (dec!(1711.00), dec!(0), dec!(1711.00)), "no input tax on ordinary purchases");
        assert_eq!((r.total_sales_ex_vat, r.total_purchases_ex_vat), (dec!(11800), dec!(0)), "Box 6 is VAT-inclusive; Box 7 empty");
        assert!(!x.limited_cost);
    }

    #[test]
    fn first_year_discount_stops_on_the_anniversary() {
        let outward = [
            sale("A", DocKind::Invoice, d(2026, 4, 14), vec![std(dec!(1000))]),
            sale("B", DocKind::Invoice, d(2026, 4, 15), vec![std(dec!(1000))]),
        ];
        let mut f = frs(dec!(14.5));
        f.registered_on = Some(d(2025, 4, 15));
        let (r, x) = flat_rate_return(&outward, &[], q2(), &f);
        assert_eq!(x.segments, vec![
            RateSegment { percent: dec!(14.5), turnover: dec!(1200), vat: dec!(174.00) },
            RateSegment { percent: dec!(13.5), turnover: dec!(1200), vat: dec!(162.00) },
        ]);
        assert_eq!(r.vat_due_sales, dec!(336.00));
    }

    #[test]
    fn limited_cost_business_test_and_override() {
        let outward = [sale("1", DocKind::Invoice, d(2026, 4, 10), vec![std(dec!(10000))])];
        // £200 of goods (VAT included) < max(2% of 12000 = 240, £250 a quarter).
        let goods = DocLine { goods: true, ..std(dec!(166.67)) };
        let inward = [purchase("P", d(2026, 4, 2), vec![goods])];
        let mut f = frs(dec!(14.5));
        f.limited_cost = LimitedCost::Auto;
        let (r, x) = flat_rate_return(&outward, &inward, q2(), &f);
        assert!(x.limited_cost, "{x:?}");
        assert_eq!(x.limited_cost_limit, dec!(250.00));
        assert_eq!(r.vat_due_sales, dec!(1980.00), "16.5% of 12000");
        assert!(r.warnings.iter().any(|w| w.contains("Limited cost business test")));
        f.limited_cost = LimitedCost::No;
        assert_eq!(flat_rate_return(&outward, &inward, q2(), &f).0.vat_due_sales, dec!(1740.00));
        f.limited_cost = LimitedCost::Yes;
        f.registered_on = Some(d(2026, 1, 1));
        assert_eq!(flat_rate_return(&outward, &inward, q2(), &f).0.vat_due_sales, dec!(1860.00), "15.5% in the first year");
    }

    #[test]
    fn capital_goods_of_2000_or_more_are_reclaimed() {
        let laptop = DocLine { capital: true, ..std(dec!(1666.67)) }; // 2000.00 with VAT
        let phone = DocLine { capital: true, ..std(dec!(500)) };
        let inward = [purchase("P", d(2026, 4, 2), vec![laptop, phone])];
        let (r, x) = flat_rate_return(&[], &inward, q2(), &frs(dec!(12)));
        assert_eq!((x.capital_value, x.capital_vat), (dec!(1666.67), dec!(333.334)));
        assert_eq!((r.vat_reclaimed, r.total_purchases_ex_vat), (dec!(333.33), dec!(1666)));
    }

    #[test]
    fn reverse_charges_and_postponed_vat_stay_outside_the_flat_rate() {
        let rc = DocLine { mechanism: Mechanism::ReverseCharge, ..std(dec!(1000)) };
        let pva = DocLine { mechanism: Mechanism::PostponedImport, ..line(Treatment::Standard, dec!(0), dec!(300)) };
        let inward = [purchase("SaaS", d(2026, 4, 2), vec![rc]), purchase("MPIVS", d(2026, 5, 2), vec![pva])];
        let outward = [sale("1", DocKind::Invoice, d(2026, 4, 10), vec![std(dec!(1000))])];
        let (r, x) = flat_rate_return(&outward, &inward, q2(), &frs(dec!(10)));
        assert_eq!(x.flat_rate_vat, dec!(120.00));
        assert_eq!(r.vat_due_sales, dec!(620.00), "120 + 200 reverse charge + 300 postponed");
        assert_eq!(r.vat_reclaimed, dec!(200.00), "the reverse charge only");
        assert_eq!((r.total_sales_ex_vat, r.total_purchases_ex_vat), (dec!(2200), dec!(1000)));
        assert!(r.warnings.iter().any(|w| w.contains("Postponed import VAT")), "{:?}", r.warnings);
    }

    #[test]
    fn cash_accounting_counts_payments_in_proportion() {
        // Notice 731 §8.1: £780 of a £1,200 invoice with £200 VAT is 65%: £130.
        let inv = sale("INV-1", DocKind::Invoice, d(2026, 3, 20), vec![std(dec!(1000))]);
        let mixed = sale("INV-2", DocKind::Invoice, d(2026, 4, 2), vec![std(dec!(2000)), line(Treatment::ZeroRated, dec!(1000), dec!(0))]);
        let unpaid = sale("INV-3", DocKind::Invoice, d(2026, 5, 2), vec![std(dec!(500))]);
        let payments = [
            Payment { doc_id: "INV-1".into(), date: d(2026, 4, 5), amount: dec!(780) },
            Payment { doc_id: "INV-2".into(), date: d(2026, 3, 30), amount: dec!(1870) },
            // §8.2: the rest of a £3,400 invoice, 45%: £180 VAT.
            Payment { doc_id: "INV-2".into(), date: d(2026, 4, 30), amount: dec!(1530) },
        ];
        let (o, i, w) = cash_basis(&[inv, mixed, unpaid], &[], &payments, &[], q2());
        assert!(i.is_empty() && w.is_empty(), "{w:?}");
        let r = vat_return(&o, &[], q2());
        assert_eq!(r.vat_due_sales, dec!(310.00), "130 + 180; the unpaid invoice is not due");
        assert_eq!(r.total_sales_ex_vat, dec!(2000), "Box 6 on payments too");
    }

    #[test]
    fn cash_accounting_nets_credit_notes_and_keeps_reverse_charges_on_invoice_dates() {
        let inv = sale("INV", DocKind::Invoice, d(2026, 4, 1), vec![std(dec!(1000))]);
        let cn = sale("CN", DocKind::CreditNote, d(2026, 4, 3), vec![std(dec!(100))]);
        let loose = sale("CN2", DocKind::CreditNote, d(2026, 4, 4), vec![std(dec!(50))]);
        let drc = DocLine { mechanism: Mechanism::DomesticReverseCharge, ..std(dec!(400)) };
        let bill = purchase("BILL", d(2026, 4, 8), vec![std(dec!(300)), drc]);
        let payments = [
            Payment { doc_id: "INV".into(), date: d(2026, 4, 9), amount: dec!(1080) },
            Payment { doc_id: "INV".into(), date: d(2026, 4, 10), amount: dec!(5) },
        ];
        let credits = [Credit { invoice_id: "INV".into(), credit_id: "CN".into() }];
        let (o, i, w) = cash_basis(&[inv, cn, loose], &[bill], &payments, &credits, q2());
        let r = vat_return(&o, &i, q2());
        // Sales: 180 paid (1200 − 120 in full) − 10 on the loose credit note.
        // Purchases: the unpaid bill's normal VAT is not reclaimed; its
        // reverse charge is (Boxes 1 and 4, on the invoice date).
        assert_eq!((r.vat_due_sales, r.vat_reclaimed), (dec!(250.00), dec!(80.00)));
        assert!(w.iter().any(|x| x.contains("CN2") && x.contains("not set against")), "{w:?}");
        assert!(w.iter().any(|x| x.contains("paid 5 more")), "{w:?}");
    }
}

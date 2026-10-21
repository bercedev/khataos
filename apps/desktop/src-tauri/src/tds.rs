//! TDS and TCS (India) from the books: the suggested deduction or
//! collection while entering a voucher, the payable reports, the data for
//! the quarterly statements (Forms 140, 143, 144) and their official text
//! files, the deductor's details, and a salary TDS estimate. Rates,
//! thresholds and due dates live in `khata_in::{tds, tcs, salary}`, the
//! file layout in `khata_in::tds_file`; see `docs/india-tds.md` for what is
//! and is not covered.
//!
//! How vouchers are read:
//! - **Deductee (party)**: the first line on a party ledger (Sundry Debtors
//!   or Creditors), or on a ledger with a PAN or a deductee type (a
//!   partner's capital account, a landlord under Loans).
//! - **Amount paid or credited**: the net debit of the voucher's other
//!   lines whose ledger has a nature of payment, or, for an expense line
//!   without one, the party's nature. Cash, bank, duty and party lines never
//!   count.
//! - **TDS deducted**: the net credit to TDS payable ledgers (duty ledgers
//!   tagged with a nature, or "all"). TDS on an "all" ledger goes to the
//!   voucher's largest nature.
//! - **Challan payments**: a voucher that debits a TDS payable ledger and
//!   has no nature line.
//!
//! An advance paid before the bill (TDS at "the earlier of credit or
//! payment") is not detected: tag the advance's line with the nature too,
//! or deduct on the bill.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use khata_core::chart::{LedgerTcs, NonResidentTerms, TcsTerms};
use khata_core::{Amount, Book, GroupId, Ledger, LedgerId, LedgerTds, TdsTerms, UserRole, VoucherId, VoucherKind};
use khata_in::pan::{self, Pan};
use khata_in::tds::{self as rules, ALL_NATURES, LowerCertificate, NATURES, Quarter, Statement, TdsInput};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize};

use crate::api::{
    ApiResult, Slot, amt, ancestors, err, group_is_party, ledger_dto, now, open_mut, open_ref, os_user, parse_amount,
    parse_date, parse_uuid,
};

const NOT_INDIA: &str = "TDS is for companies in India (pick India as the company's country)";

fn india(book: &Book) -> ApiResult<()> {
    if book.country() == Some("IN") { Ok(()) } else { Err(NOT_INDIA.into()) }
}

/// `deserialize_with` for "absent keeps, null clears" fields: present
/// values (null included) come back wrapped in `Some`.
pub fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

// ------------------------------------------------------------ natures

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NatureDto {
    pub code: String,
    pub name: String,
    /// 1961 Act section ("194C").
    pub old_section: String,
    /// 2025 Act provision ("s.393(1)") and its table row ("Sl. No. 6(i)").
    pub new_section: String,
    pub table: String,
    /// 4-digit statement codes from tax year 2026-27.
    pub tax_code_individual_huf: String,
    pub tax_code_others: String,
    /// The rate is the user's (non-residents): enter it on the party.
    pub user_rate: bool,
    /// Percent.
    pub rate_individual_huf: String,
    pub rate_others: String,
    /// The limit in words ("over ₹50,000 in the year").
    pub threshold: String,
    pub statement: Statement,
    pub condition: Option<String>,
    pub caution: Option<String>,
}

/// Natures of payment for pickers, in the pack's order.
pub fn tds_natures() -> Vec<NatureDto> {
    NATURES
        .iter()
        .map(|n| NatureDto {
            code: n.code.into(),
            name: n.name.into(),
            old_section: n.old_section.into(),
            new_section: n.new_section.into(),
            table: n.table.into(),
            tax_code_individual_huf: n.tax_code_individual_huf.into(),
            tax_code_others: n.tax_code_others.into(),
            user_rate: n.user_rate,
            rate_individual_huf: amt(n.rate_individual_huf),
            rate_others: amt(n.rate_others),
            threshold: n.threshold_text(),
            statement: n.statement,
            condition: n.condition.map(str::to_owned),
            caution: n.caution.map(str::to_owned),
        })
        .collect()
}

fn nature_name(code: &str) -> String {
    if code == ALL_NATURES {
        return "All natures".into();
    }
    rules::nature(code).map_or_else(|| code.to_owned(), |n| n.name.to_owned())
}

fn section(code: &str, date: NaiveDate) -> String {
    rules::nature(code).map(|n| n.section(date)).unwrap_or_default()
}

// ------------------------------------------------------- ledger settings

/// The deductee's PAN: its own, else the one inside its GSTIN.
pub fn party_pan(l: &Ledger) -> Option<Pan> {
    l.pan
        .as_deref()
        .and_then(|p| pan::validate(p).ok())
        .or_else(|| l.tax_id.as_deref().and_then(pan::from_gstin))
}

fn terms(l: &Ledger) -> Option<&TdsTerms> {
    match &l.tds {
        Some(LedgerTds::Deductible(t)) => Some(t),
        _ => None,
    }
}

fn payable_for(l: &Ledger) -> Option<&str> {
    match &l.tds {
        Some(LedgerTds::Payable { payable_for }) => Some(payable_for),
        _ => None,
    }
}

fn under_duties(book: &Book, l: &Ledger) -> bool {
    ancestors(book, Some(l.group)).any(|g| g.id == GroupId::default_named("Duties & Taxes"))
}

/// A ledger that receives payments subject to TDS.
fn is_deductee(book: &Book, l: &Ledger) -> bool {
    group_is_party(book, l.group) || l.pan.is_some() || terms(l).is_some_and(|t| t.deductee.is_some())
}

/// The nature a line on `l` is for: its own, else the deductee's.
fn line_nature(l: &Ledger, party: Option<&Ledger>) -> Option<String> {
    terms(l)
        .and_then(|t| t.nature.clone())
        .or_else(|| party.and_then(terms).and_then(|t| t.nature.clone()))
}

/// Check and normalise PAN and TDS settings (codes lower-case, PAN upper).
fn checked_tds(book: &Book, l: &Ledger, pan: Option<String>, tds: Option<LedgerTds>) -> ApiResult<(Option<String>, Option<LedgerTds>)> {
    let pan = match pan.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        None => None,
        Some(p) => {
            let p = pan::validate(p).map_err(err)?;
            let gstin = l.tax_id.as_deref().filter(|_| l.tax_id_override.is_none());
            if let Some(g) = gstin.filter(|g| !khata_in::gstin::embeds_pan(g, &p.value)) {
                return Err(format!("the PAN {} is not the one in the GSTIN {g}", p.value));
            }
            Some(p.value)
        }
    };
    let known = |c: &str| rules::nature(c).map(|n| n.code.to_owned()).ok_or_else(|| format!("{c:?} is not a TDS nature of payment"));
    let tds = match tds {
        None => None,
        Some(LedgerTds::Payable { payable_for }) => {
            let code = payable_for.trim().to_ascii_lowercase();
            let code = if code == ALL_NATURES { code } else { known(&code)? };
            if !under_duties(book, l) {
                return Err("a TDS payable ledger must be under Duties & Taxes".into());
            }
            Some(LedgerTds::Payable { payable_for: code })
        }
        Some(LedgerTds::Deductible(mut t)) => {
            t.nature = t.nature.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(known).transpose()?;
            t.certificate = t.certificate.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty());
            t.non_resident = t.non_resident.map(checked_non_resident).transpose()?;
            if t.nature.is_none() && t.deductee.is_none() {
                return Err("pick a nature of payment or a deductee type".into());
            }
            if under_duties(book, l) {
                return Err("a ledger under Duties & Taxes can hold TDS payable, not have TDS deducted".into());
            }
            Some(LedgerTds::Deductible(t))
        }
    };
    Ok((pan, tds))
}

/// Normalise a non-resident's terms; codes must be in the statement's lists.
fn checked_non_resident(mut n: NonResidentTerms) -> ApiResult<NonResidentTerms> {
    let clean = |s: Option<String>| s.map(|x| x.trim().to_owned()).filter(|x| !x.is_empty());
    n.country = clean(n.country);
    n.remittance = clean(n.remittance);
    n.foreign_tin = clean(n.foreign_tin);
    if let Some(c) = &n.country {
        if khata_in::tds_file::country(c).is_none() {
            return Err(format!("{c:?} is not a country code of the Form 144 list"));
        }
    }
    if let Some(r) = &n.remittance {
        if !khata_in::tds_file::REMITTANCES.iter().any(|x| x.0 == r) {
            return Err(format!("{r:?} is not a nature-of-remittance code of the Form 144 list"));
        }
    }
    if n.rate.is_sign_negative() || n.rate > Decimal::ONE_HUNDRED {
        return Err("a TDS rate is 0 to 100 percent".into());
    }
    Ok(n)
}

/// Apply `LedgerInput`'s `pan` (absent keeps, empty clears) and `tds`
/// (absent keeps, null clears) to a ledger being altered. Called after the
/// group and GSTIN are set, so they are checked against the new ones.
pub fn apply_ledger_input(book: &Book, l: &mut Ledger, pan: Option<&str>, tds: Option<&Option<LedgerTds>>) -> ApiResult<()> {
    if pan.is_none() && tds.is_none() {
        return Ok(());
    }
    let wants = pan.is_some_and(|p| !p.trim().is_empty()) || tds.is_some_and(Option::is_some);
    if wants {
        india(book)?;
    }
    let pan = pan.map_or_else(|| l.pan.clone(), |p| Some(p.to_owned()));
    let tds = tds.map_or_else(|| l.tds.clone(), Clone::clone);
    (l.pan, l.tds) = checked_tds(book, l, pan, tds)?;
    Ok(())
}

/// Set (replace) a ledger's PAN and TDS settings; `None` clears each.
pub fn set_ledger_tds(slot: &mut Slot, id: &str, pan: Option<String>, tds: Option<LedgerTds>) -> ApiResult<crate::api::LedgerDto> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let id = LedgerId(parse_uuid(id, "ledger")?);
    let mut l = book.ledger(id).ok_or("unknown ledger")?.clone();
    if pan.as_deref().is_some_and(|p| !p.trim().is_empty()) || tds.is_some() {
        india(book)?;
    }
    (l.pan, l.tds) = checked_tds(book, &l, pan, tds)?;
    book.alter_ledger(now(), &os_user(), l).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(ledger_dto(book, book.ledger(id).expect("just altered"), &book.balances(None)))
}

// ------------------------------------------------------- reading vouchers

#[derive(Debug, Clone)]
struct Deduction {
    voucher: VoucherId,
    kind: VoucherKind,
    number: String,
    date: NaiveDate,
    party: Option<LedgerId>,
    nature: String,
    /// Amount paid or credited.
    base: Amount,
    tds: Amount,
}

#[derive(Debug, Clone)]
struct Challan {
    voucher: VoucherId,
    kind: VoucherKind,
    number: String,
    date: NaiveDate,
    ledger: LedgerId,
    payable_for: String,
    amount: Amount,
}

#[derive(Debug, Default)]
struct Scan {
    deductions: Vec<Deduction>,
    challans: Vec<Challan>,
    warnings: Vec<String>,
}

fn kind_label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Contra => "Contra",
        VoucherKind::Payment => "Payment",
        VoucherKind::Receipt => "Receipt",
        VoucherKind::Journal => "Journal",
        VoucherKind::Sales => "Sales",
        VoucherKind::Purchase => "Purchase",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        VoucherKind::StockJournal => "Stock Journal",
    }
}

/// Deductions and challan payments in vouchers dated `from..=to`.
fn scan(book: &Book, from: NaiveDate, to: NaiveDate) -> Scan {
    let mut out = Scan::default();
    for v in book.active_vouchers().filter(|v| from <= v.date && v.date <= to) {
        let name = format!("{} {}", kind_label(v.kind), v.number);
        let party_at = v.entries.iter().position(|e| book.ledger(e.ledger).is_some_and(|l| is_deductee(book, l)));
        let party = party_at.and_then(|i| book.ledger(v.entries[i].ledger));
        let mut bases: BTreeMap<String, Amount> = BTreeMap::new();
        // (ledger, payable_for, net credit)
        let mut payable: Vec<(LedgerId, String, Amount)> = Vec::new();
        for (i, e) in v.entries.iter().enumerate() {
            let Some(l) = book.ledger(e.ledger) else { continue };
            if let Some(p) = payable_for(l) {
                match payable.iter_mut().find(|x| x.0 == l.id) {
                    Some(x) => x.2 -= e.signed(),
                    None => payable.push((l.id, p.to_owned(), -e.signed())),
                }
                continue;
            }
            if Some(i) == party_at || book.is_cash_or_bank(l.id) || under_duties(book, l) || is_deductee(book, l) {
                continue;
            }
            if let Some(n) = line_nature(l, party) {
                *bases.entry(n).or_default() += e.signed();
            }
        }
        bases.retain(|_, b| !b.is_zero());
        payable.retain(|p| !p.2.is_zero());
        if bases.is_empty() && payable.is_empty() {
            continue;
        }
        let row = |nature: String, base: Amount, tds: Amount| Deduction {
            voucher: v.id,
            kind: v.kind,
            number: v.number.clone(),
            date: v.date,
            party: party.map(|p| p.id),
            nature,
            base,
            tds,
        };
        if bases.is_empty() {
            for (ledger, payable_for, net_cr) in payable {
                if net_cr.is_sign_negative() {
                    out.challans.push(Challan {
                        voucher: v.id,
                        kind: v.kind,
                        number: v.number.clone(),
                        date: v.date,
                        ledger,
                        payable_for,
                        amount: -net_cr,
                    });
                } else {
                    out.warnings.push(format!(
                        "{name}: TDS is credited but no line has a nature of payment; tag the expense ledger"
                    ));
                    out.deductions.push(row(payable_for, Amount::ZERO, net_cr));
                }
            }
            continue;
        }
        let largest = bases.iter().max_by_key(|(_, b)| b.abs()).map(|(n, _)| n.clone()).expect("not empty");
        let mut tds: BTreeMap<String, Amount> = BTreeMap::new();
        for (_, payable_for, net_cr) in &payable {
            let n = if payable_for == ALL_NATURES { largest.clone() } else { payable_for.clone() };
            *tds.entry(n).or_default() += *net_cr;
        }
        for n in tds.keys().filter(|n| !bases.contains_key(*n)) {
            out.warnings.push(format!("{name}: TDS for {} but no line has that nature of payment", nature_name(n)));
        }
        if party.is_none() && !tds.is_empty() {
            out.warnings.push(format!("{name}: TDS is deducted but no line is a party (deductee) ledger"));
        }
        let natures: Vec<String> = bases.keys().chain(tds.keys()).cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        for n in natures {
            let base = bases.get(&n).copied().unwrap_or_default();
            out.deductions.push(row(n.clone(), base, tds.get(&n).copied().unwrap_or_default()));
        }
    }
    out
}

// ------------------------------------------------------------ tds_for

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TdsSuggestion {
    /// Whether TDS is due on this amount (false: none, see `explanation`).
    pub applies: bool,
    /// TDS to deduct, rounded to the rupee.
    pub amount: String,
    pub nature: Option<String>,
    pub nature_name: Option<String>,
    pub section: Option<String>,
    /// Percent.
    pub rate: Option<String>,
    pub rate_basis: Option<rules::RateBasis>,
    /// The amount the rate is applied to (the year's total when a limit is crossed).
    pub base: String,
    pub threshold: Option<rules::ThresholdHit>,
    pub explanation: String,
    /// Paid or credited to the party for this nature earlier in the year, and TDS on it.
    pub ytd_amount: String,
    pub ytd_tds: String,
    pub pan: Option<String>,
    pub deductee: Option<khata_core::DeducteeKind>,
    /// Where to credit the TDS; missing means a ledger must be created.
    pub payable_ledger_id: Option<String>,
    pub payable_ledger_name: Option<String>,
    /// Conditions, uncertainties and missing set-up.
    pub warnings: Vec<String>,
}

fn nothing(explanation: String, warnings: Vec<String>) -> TdsSuggestion {
    TdsSuggestion {
        applies: false,
        amount: "0".into(),
        nature: None,
        nature_name: None,
        section: None,
        rate: None,
        rate_basis: None,
        base: "0".into(),
        threshold: None,
        explanation,
        ytd_amount: "0".into(),
        ytd_tds: "0".into(),
        pan: None,
        deductee: None,
        payable_ledger_id: None,
        payable_ledger_name: None,
        warnings,
    }
}

/// The TDS payable ledger for a nature: one tagged for it, else one for all.
fn payable_ledger<'a>(book: &'a Book, nature: &str) -> Option<&'a Ledger> {
    book.ledgers()
        .find(|l| payable_for(l) == Some(nature))
        .or_else(|| book.ledgers().find(|l| payable_for(l) == Some(ALL_NATURES)))
}

/// Suggested TDS for a voucher being entered: `amount` paid or credited
/// to `party_id` through `expense_ledger_id` on `date`. `voucher_id` (the
/// voucher being altered) is left out of the year's totals.
pub fn tds_for(
    slot: &Slot,
    date: &str,
    party_id: &str,
    expense_ledger_id: &str,
    amount: &str,
    voucher_id: Option<&str>,
) -> ApiResult<TdsSuggestion> {
    let book = open_ref(slot)?;
    india(book)?;
    let date = parse_date(date)?;
    let amount = parse_amount(amount)?.abs();
    let party = match party_id.trim() {
        "" => None,
        id => Some(book.ledger(LedgerId(parse_uuid(id, "party ledger")?)).ok_or("unknown party ledger")?),
    };
    let expense = match expense_ledger_id.trim() {
        "" => None,
        id => Some(book.ledger(LedgerId(parse_uuid(id, "ledger")?)).ok_or("unknown ledger")?),
    };
    let exclude = voucher_id.filter(|v| !v.trim().is_empty()).map(|v| parse_uuid(v, "voucher")).transpose()?.map(VoucherId);
    let nature = expense.and_then(|e| line_nature(e, party)).or_else(|| party.and_then(terms).and_then(|t| t.nature.clone()));
    let Some(nature) = nature else {
        let what = [expense, party].iter().flatten().map(|l| l.name.as_str()).collect::<Vec<_>>().join(" or ");
        return Ok(nothing(format!("No nature of payment on {}: no TDS suggested.", if what.is_empty() { "these ledgers" } else { what.as_str() }), Vec::new()));
    };
    let mut warnings = Vec::new();
    let pan = party.and_then(party_pan);
    if party.is_none() {
        warnings.push("No party: the year's earlier payments cannot be added up, and the PAN is unknown.".into());
    }
    let deductee = party.and_then(terms).and_then(|t| t.deductee).or(pan.as_ref().map(|p| p.holder));
    if let (Some(t), Some(p)) = (party.and_then(terms).and_then(|t| t.deductee), &pan) {
        if t != p.holder {
            warnings.push(format!("The deductee type ({t:?}) differs from the PAN's ({:?}); the ledger's type is used.", p.holder));
        }
    }
    let certificate = party.and_then(terms).and_then(|t| {
        t.lower_rate.map(|rate| LowerCertificate { rate, number: t.certificate.clone(), valid_from: t.valid_from, valid_to: t.valid_to })
    });
    let fy = khata_compliance::FinancialYear::containing(date, khata_in::FY_START_MONTH);
    let (mut ytd_amount, mut ytd_tds) = (Amount::ZERO, Amount::ZERO);
    if let Some(p) = party {
        for d in scan(book, fy.start, date).deductions {
            if d.party == Some(p.id) && d.nature == nature && Some(d.voucher) != exclude {
                ytd_amount += d.base;
                ytd_tds += d.tds;
            }
        }
    }
    let user_rate = party.and_then(terms).and_then(|t| t.non_resident.as_ref()).map(|n| n.rate);
    let r = match rules::compute(&TdsInput {
        nature: &nature,
        deductee,
        pan: pan.is_some(),
        date,
        amount,
        ytd_amount: ytd_amount.max(Amount::ZERO),
        ytd_tds: ytd_tds.max(Amount::ZERO),
        certificate,
        user_rate,
    }) {
        Err(e @ rules::TdsError::RateRequired(_)) => {
            warnings.push(rules::NON_RESIDENT_REMINDER.into());
            return Ok(TdsSuggestion { nature_name: Some(nature_name(&nature)), nature: Some(nature), ..nothing(format!("{}.", capitalise(&e.to_string())), warnings) });
        }
        r => r.map_err(err)?,
    };
    warnings.extend(r.cautions.iter().cloned());
    let ledger = payable_ledger(book, &nature);
    if ledger.is_none() && !r.tds.is_zero() {
        warnings.push(format!(
            "Create a ledger under Duties & Taxes marked as TDS payable for {} (or for all natures) to credit the TDS to.",
            nature_name(&nature)
        ));
    }
    Ok(TdsSuggestion {
        applies: !r.tds.is_zero(),
        amount: amt(r.tds),
        nature_name: Some(nature_name(&nature)),
        nature: Some(nature),
        section: Some(r.section.clone()),
        rate: Some(amt(r.rate)),
        rate_basis: Some(r.rate_basis),
        base: amt(r.base),
        threshold: Some(r.threshold),
        explanation: r.explanation.clone(),
        ytd_amount: amt(ytd_amount),
        ytd_tds: amt(ytd_tds),
        pan: pan.map(|p| p.value),
        deductee,
        payable_ledger_id: ledger.map(|l| l.id.0.to_string()),
        payable_ledger_name: ledger.map(|l| l.name.clone()),
        warnings,
    })
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

// ------------------------------------------------------------ TDS payable

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeductionRow {
    pub voucher_id: String,
    pub kind: VoucherKind,
    pub number: String,
    pub date: String,
    pub party_id: Option<String>,
    pub party_name: String,
    pub pan: Option<String>,
    pub nature: String,
    pub nature_name: String,
    pub section: String,
    /// Amount paid or credited.
    pub amount: String,
    pub tds: String,
    /// When the TDS must be paid to the government.
    pub deposit_due: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChallanRow {
    pub voucher_id: String,
    pub kind: VoucherKind,
    pub number: String,
    pub date: String,
    pub ledger_id: String,
    pub ledger_name: String,
    /// Nature code, or "all".
    pub nature: String,
    pub amount: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MonthDue {
    /// "2026-05".
    pub month: String,
    pub deducted: String,
    pub due: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NatureTotals {
    /// Nature code, or "all" for payments to an all-natures ledger.
    pub nature: String,
    pub name: String,
    pub section: String,
    pub paid_credited: String,
    pub deducted: String,
    /// Challan payments in the period.
    pub deposited: String,
    /// Closing balance (as at `to`) of the ledgers tagged for this nature;
    /// `None` when its TDS sits in an all-natures ledger.
    pub outstanding: Option<String>,
    pub by_month: Vec<MonthDue>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PayableLedgerRow {
    pub id: String,
    pub name: String,
    pub payable_for: String,
    /// Owed before the period (credit positive).
    pub opening: String,
    /// Owed at the end of the period (credit positive).
    pub closing: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TdsPayableDto {
    pub from: String,
    pub to: String,
    pub natures: Vec<NatureTotals>,
    pub rows: Vec<DeductionRow>,
    pub challans: Vec<ChallanRow>,
    pub ledgers: Vec<PayableLedgerRow>,
    pub paid_credited: String,
    pub deducted: String,
    pub deposited: String,
    pub warnings: Vec<String>,
}

fn ledger_name(book: &Book, id: Option<LedgerId>) -> String {
    id.and_then(|i| book.ledger(i)).map(|l| l.name.clone()).unwrap_or_default()
}

fn deduction_row(book: &Book, d: &Deduction) -> DeductionRow {
    let party = d.party.and_then(|p| book.ledger(p));
    DeductionRow {
        voucher_id: d.voucher.0.to_string(),
        kind: d.kind,
        number: d.number.clone(),
        date: d.date.to_string(),
        party_id: d.party.map(|p| p.0.to_string()),
        party_name: party.map(|p| p.name.clone()).unwrap_or_else(|| "(no party)".into()),
        pan: party.and_then(party_pan).map(|p| p.value),
        nature: d.nature.clone(),
        nature_name: nature_name(&d.nature),
        section: section(&d.nature, d.date),
        amount: amt(d.base),
        tds: amt(d.tds),
        deposit_due: rules::deposit_due(d.date).to_string(),
    }
}

fn challan_row(book: &Book, c: &Challan) -> ChallanRow {
    ChallanRow {
        voucher_id: c.voucher.0.to_string(),
        kind: c.kind,
        number: c.number.clone(),
        date: c.date.to_string(),
        ledger_id: c.ledger.0.to_string(),
        ledger_name: ledger_name(book, Some(c.ledger)),
        nature: c.payable_for.clone(),
        amount: amt(c.amount),
    }
}

/// Position of a nature in the pack's list ("all" and unknown codes last).
fn order(code: &str) -> usize {
    NATURES.iter().position(|n| n.code == code).unwrap_or(NATURES.len())
}

/// Deductions and challan payments in a period, by nature, with what is
/// still owed and when each month's TDS is due.
pub fn tds_payable(slot: &Slot, from: &str, to: &str) -> ApiResult<TdsPayableDto> {
    let book = open_ref(slot)?;
    india(book)?;
    let (from_d, to_d) = (parse_date(from)?, parse_date(to)?);
    if from_d > to_d {
        return Err("the period ends before it starts".into());
    }
    let s = scan(book, from_d, to_d);
    let closing = book.balances(Some(to_d));
    let opening = from_d.pred_opt().map(|d| book.balances(Some(d))).unwrap_or_default();
    let owed = |m: &BTreeMap<LedgerId, Amount>, l: &Ledger| -m.get(&l.id).copied().unwrap_or(l.opening);
    let mut ledgers: Vec<PayableLedgerRow> = book
        .ledgers()
        .filter_map(|l| {
            payable_for(l).map(|p| PayableLedgerRow {
                id: l.id.0.to_string(),
                name: l.name.clone(),
                payable_for: p.to_owned(),
                opening: amt(if from_d.pred_opt().is_some() { owed(&opening, l) } else { Amount::ZERO }),
                closing: amt(owed(&closing, l)),
            })
        })
        .collect();
    ledgers.sort_by_key(|l| (order(&l.payable_for), l.name.to_lowercase()));

    let mut codes: Vec<String> = s.deductions.iter().map(|d| d.nature.clone()).chain(s.challans.iter().map(|c| c.payable_for.clone())).collect();
    codes.sort_by_key(|c| (order(c), c.clone()));
    codes.dedup();
    let natures = codes
        .into_iter()
        .map(|code| {
            let mine: Vec<&Deduction> = s.deductions.iter().filter(|d| d.nature == code).collect();
            let mut months: BTreeMap<(i32, u32), Amount> = BTreeMap::new();
            for d in &mine {
                *months.entry((d.date.year(), d.date.month())).or_default() += d.tds;
            }
            let tagged: Vec<&Ledger> = book.ledgers().filter(|l| payable_for(l) == Some(code.as_str())).collect();
            NatureTotals {
                name: nature_name(&code),
                section: section(&code, to_d),
                paid_credited: amt(mine.iter().map(|d| d.base).sum()),
                deducted: amt(mine.iter().map(|d| d.tds).sum()),
                deposited: amt(s.challans.iter().filter(|c| c.payable_for == code).map(|c| c.amount).sum()),
                outstanding: (!tagged.is_empty()).then(|| amt(tagged.iter().map(|l| owed(&closing, l)).sum())),
                by_month: months
                    .into_iter()
                    .filter(|(_, a)| !a.is_zero())
                    .map(|((y, m), a)| MonthDue {
                        month: format!("{y}-{m:02}"),
                        deducted: amt(a),
                        due: NaiveDate::from_ymd_opt(y, m, 1).map(rules::deposit_due).map(|d| d.to_string()).unwrap_or_default(),
                    })
                    .collect(),
                nature: code,
            }
        })
        .collect();
    let mut warnings = s.warnings;
    if ledgers.is_empty() {
        warnings.push("No ledger is marked as TDS payable: deductions and challans cannot be found.".into());
    }
    warnings.push(
        "Due dates for paying TDS (the 7th of the next month; 30 April for March) are the old Rule 30's; the new rules' \
         dates are not verified."
            .into(),
    );
    Ok(TdsPayableDto {
        from: from.into(),
        to: to.into(),
        paid_credited: amt(s.deductions.iter().map(|d| d.base).sum()),
        deducted: amt(s.deductions.iter().map(|d| d.tds).sum()),
        deposited: amt(s.challans.iter().map(|c| c.amount).sum()),
        rows: s.deductions.iter().map(|d| deduction_row(book, d)).collect(),
        challans: s.challans.iter().map(|c| challan_row(book, c)).collect(),
        natures,
        ledgers,
        warnings,
    })
}

// ------------------------------------------------------------ quarterly

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatementRow {
    pub serial: usize,
    pub party_id: Option<String>,
    pub party_name: String,
    /// Empty when the deductee has no PAN on record.
    pub pan: Option<String>,
    pub deductee: Option<khata_core::DeducteeKind>,
    pub nature: String,
    pub nature_name: String,
    /// Old section ("194C"), the section code statements have used.
    pub section: String,
    /// The 4-digit tax code statements use from tax year 2026-27.
    pub tax_code: String,
    pub date: String,
    pub amount: String,
    pub tds: String,
    /// TDS as a percent of the amount (two decimals).
    pub rate: String,
    /// Why the rate is not the normal one: no PAN, a certificate.
    pub reason: Option<String>,
    pub voucher_id: String,
    pub kind: VoucherKind,
    pub number: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TdsQuarterlyDto {
    /// "2026-27".
    pub year: String,
    pub quarter: u32,
    pub from: String,
    pub to: String,
    /// "Form 140" (2026-27 onward) or "Form 26Q".
    pub form: String,
    pub act: rules::Act,
    /// `None` where the research has no due date (before 2026-27).
    pub due: Option<String>,
    pub deductor_name: String,
    pub deductor_gstin: Option<String>,
    pub deductor_pan: Option<String>,
    pub rows: Vec<StatementRow>,
    pub challans: Vec<ChallanRow>,
    pub amount: String,
    pub tds: String,
    pub deposited: String,
    pub warnings: Vec<String>,
    /// What this data lacks compared to a filed statement.
    pub gaps: Vec<String>,
}

const GAPS: &[&str] = &[
    "The CSV is a working list. From tax year 2026-27 the statement text file (Protean format v1.2) can be saved from \
     this report once the deductor's details (TAN, responsible person) and each challan's BSR code, serial number and \
     deposit date are entered; validate it with the official File Validation Utility before filing.",
    "Challans are matched to rows by nature (or an all-natures ledger) in date order; the FVU checks them against the \
     challan status (.csi) file, which KhataOS does not read.",
    "Payments to non-residents are on Form 144 (its own report); salary (Form 138, ex-24Q) is not covered beyond the \
     salary TDS estimate.",
    "Correction statements and the Form 141 challan-cum-statement are not covered.",
];

/// The deductee-wise data for the quarterly statement of resident
/// non-salary TDS. `year` is the year the financial year starts in.
pub fn tds_quarterly(slot: &Slot, year: i32, quarter: u32) -> ApiResult<TdsQuarterlyDto> {
    let book = open_ref(slot)?;
    india(book)?;
    let q = Quarter { year, q: quarter };
    let (from, to) = q.range().ok_or("the quarter must be 1 to 4")?;
    let s = scan(book, from, to);
    let mut warnings = s.warnings;
    let mut rows = Vec::new();
    for d in &s.deductions {
        let Some(n) = rules::nature(&d.nature) else {
            if d.nature != ALL_NATURES {
                warnings.push(format!("{} {}: unknown nature {:?}", kind_label(d.kind), d.number, d.nature));
            }
            continue;
        };
        if n.statement == Statement::NonResident {
            continue;
        }
        if n.statement != Statement::Quarterly {
            if !d.tds.is_zero() {
                warnings.push(format!(
                    "{} {}: {} is reported on the challan-cum-statement, not here",
                    kind_label(d.kind),
                    d.number,
                    n.name
                ));
            }
            continue;
        }
        let party = d.party.and_then(|p| book.ledger(p));
        let pan = party.and_then(party_pan);
        let cert = party.and_then(terms).filter(|t| t.lower_rate.is_some()).filter(|t| {
            t.valid_from.is_none_or(|f| f <= d.date) && t.valid_to.is_none_or(|x| d.date <= x)
        });
        let reason = if pan.is_none() {
            Some("No PAN: higher rate".to_owned())
        } else {
            cert.map(|t| format!("Lower deduction certificate {}", t.certificate.as_deref().unwrap_or("")).trim_end().to_owned())
        };
        if d.tds.is_zero() && cert.is_none() {
            continue;
        }
        if party.is_none() {
            warnings.push(format!("{} {}: TDS without a deductee ledger", kind_label(d.kind), d.number));
        } else if pan.is_none() {
            warnings.push(format!("{}: no PAN on record", ledger_name(book, d.party)));
        }
        let rate = if d.base.is_zero() {
            Decimal::ZERO
        } else {
            (d.tds * Decimal::ONE_HUNDRED / d.base).round_dp(2)
        };
        let deductee = party.and_then(terms).and_then(|t| t.deductee).or(pan.as_ref().map(|p| p.holder));
        rows.push(StatementRow {
            serial: rows.len() + 1,
            party_id: d.party.map(|p| p.0.to_string()),
            party_name: party.map(|p| p.name.clone()).unwrap_or_else(|| "(no party)".into()),
            deductee,
            pan: pan.map(|p| p.value),
            nature: n.code.into(),
            nature_name: n.name.into(),
            section: n.old_section.into(),
            tax_code: n.tax_code(deductee).into(),
            date: d.date.to_string(),
            amount: amt(d.base),
            tds: amt(d.tds),
            rate: format!("{rate:.2}"),
            reason,
            voucher_id: d.voucher.0.to_string(),
            kind: d.kind,
            number: d.number.clone(),
        });
    }
    warnings.sort();
    warnings.dedup();
    let gstin = book.profile().and_then(|p| p.tax_id.clone());
    let challans: Vec<ChallanRow> = s.challans.iter().map(|c| challan_row(book, c)).collect();
    let sum = |f: fn(&StatementRow) -> &str| amt(rows.iter().map(|r| parse_amount(f(r)).unwrap_or_default()).sum());
    Ok(TdsQuarterlyDto {
        year: q.year_label(),
        quarter,
        from: from.to_string(),
        to: to.to_string(),
        form: q.form().into(),
        act: rules::Act::for_date(from),
        due: q.statement_due().map(|d| d.to_string()),
        deductor_name: book.name.clone(),
        deductor_pan: gstin.as_deref().and_then(pan::from_gstin).map(|p| p.value),
        deductor_gstin: gstin,
        amount: sum(|r| &r.amount),
        tds: sum(|r| &r.tds),
        deposited: amt(s.challans.iter().map(|c| c.amount).sum()),
        rows,
        challans,
        warnings,
        gaps: GAPS.iter().map(|g| (*g).to_owned()).collect(),
    })
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() }
}

fn csv_line(fields: &[&str]) -> String {
    let mut line = fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(",");
    line.push_str("\r\n");
    line
}

/// The quarterly statement data as CSV: a header block, one line per
/// deductee row, then the challans. Not the FVU text format.
pub fn tds_quarterly_csv(slot: &Slot, year: i32, quarter: u32) -> ApiResult<String> {
    let d = tds_quarterly(slot, year, quarter)?;
    let mut out = String::new();
    out += &csv_line(&[&format!("{} data, Q{} {} (not an FVU file)", d.form, d.quarter, d.year)]);
    out += &csv_line(&["Deductor", &d.deductor_name, "PAN", d.deductor_pan.as_deref().unwrap_or(""), "GSTIN", d.deductor_gstin.as_deref().unwrap_or("")]);
    out += &csv_line(&["Period", &d.from, &d.to, "Due", d.due.as_deref().unwrap_or("not known")]);
    out += "\r\n";
    out += &csv_line(&[
        "Sr", "Deductee", "PAN", "Deductee type", "Section", "Nature", "Date paid/credited", "Amount paid/credited", "TDS",
        "Rate %", "Reason", "Voucher type", "Voucher no.",
    ]);
    for r in &d.rows {
        let kind = r.deductee.map(|k| format!("{k:?}")).unwrap_or_default();
        out += &csv_line(&[
            &r.serial.to_string(),
            &r.party_name,
            r.pan.as_deref().unwrap_or(""),
            &kind,
            &r.section,
            &r.nature_name,
            &r.date,
            &r.amount,
            &r.tds,
            &r.rate,
            r.reason.as_deref().unwrap_or(""),
            kind_label(r.kind),
            &r.number,
        ]);
    }
    out += &csv_line(&["Total", "", "", "", "", "", "", &d.amount, &d.tds]);
    out += "\r\n";
    out += &csv_line(&["Challans (payments from the books)"]);
    out += &csv_line(&["Date", "Ledger", "Nature", "Amount", "Voucher type", "Voucher no."]);
    for c in &d.challans {
        out += &csv_line(&[&c.date, &c.ledger_name, &nature_name(&c.nature), &c.amount, kind_label(c.kind), &c.number]);
    }
    Ok(out)
}

// ============================================================== TCS
//
// How vouchers are read for TCS (sales and receipts):
// - **Collectee (party)**: the first line on a party ledger, or on a ledger
//   with a PAN or TCS terms.
// - **Amount received or debited**: the net credit of the voucher's other
//   lines whose ledger has a TCS nature (or, for an income line without
//   one, the party's nature), plus the net credit to GST output ledgers in
//   the same voucher, put on the largest nature: the Act taxes the "amount
//   payable by the buyer", read here as including GST [UNCERTAIN].
// - **TCS collected**: the net credit to TCS payable ledgers.
// - **Challan payments**: a voucher that debits a TCS payable ledger and has
//   no nature line.

use khata_in::tcs::{self as tcs_rules, NATURES as TCS_NATURES};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TcsNatureDto {
    pub code: String,
    pub name: String,
    /// "s.394(1) [Table: Sl. No. 4] (ex-206C(1))".
    pub section: String,
    pub old_section: String,
    pub tax_code: String,
    /// Percent.
    pub rate: String,
    /// Percent without a PAN.
    pub no_pan_rate: String,
    pub threshold: String,
    pub collector: String,
    pub declaration_allowed: bool,
    pub condition: Option<String>,
    pub caution: Option<String>,
}

/// TCS natures of collection for pickers.
pub fn tcs_natures() -> Vec<TcsNatureDto> {
    TCS_NATURES
        .iter()
        .map(|n| TcsNatureDto {
            code: n.code.into(),
            name: n.name.into(),
            section: n.section(),
            old_section: n.old_section.into(),
            tax_code: n.tax_code.into(),
            rate: amt(n.rate),
            no_pan_rate: amt(tcs_rules::no_pan_rate(n.rate)),
            threshold: n.threshold_text(),
            collector: n.collector.into(),
            declaration_allowed: n.declaration_allowed,
            condition: n.condition.map(str::to_owned),
            caution: n.caution.map(str::to_owned),
        })
        .collect()
}

fn tcs_nature_name(code: &str) -> String {
    if code == tcs_rules::ALL_NATURES {
        return "All natures".into();
    }
    tcs_rules::nature(code).map_or_else(|| code.to_owned(), |n| n.name.to_owned())
}

fn tcs_terms(l: &Ledger) -> Option<&TcsTerms> {
    match &l.tcs {
        Some(LedgerTcs::Collectible(t)) => Some(t),
        _ => None,
    }
}

fn tcs_payable_for(l: &Ledger) -> Option<&str> {
    match &l.tcs {
        Some(LedgerTcs::Payable { payable_for }) => Some(payable_for),
        _ => None,
    }
}

/// A ledger that pays amounts subject to TCS (a buyer, licensee or lessee).
fn is_collectee(book: &Book, l: &Ledger) -> bool {
    group_is_party(book, l.group) || l.pan.is_some() || tcs_terms(l).is_some_and(|t| t.nature.is_none())
}

fn tcs_line_nature(l: &Ledger, party: Option<&Ledger>) -> Option<String> {
    tcs_terms(l).and_then(|t| t.nature.clone()).or_else(|| party.and_then(tcs_terms).and_then(|t| t.nature.clone()))
}

/// The collectee type: the TCS terms' own, else the TDS terms', else the PAN's.
fn collectee_kind(l: &Ledger) -> Option<khata_core::DeducteeKind> {
    tcs_terms(l).and_then(|t| t.collectee).or_else(|| terms(l).and_then(|t| t.deductee)).or_else(|| party_pan(l).map(|p| p.holder))
}

fn checked_tcs(book: &Book, l: &Ledger, tcs: Option<LedgerTcs>) -> ApiResult<Option<LedgerTcs>> {
    let known = |c: &str| tcs_rules::nature(c).map(|n| n.code.to_owned()).ok_or_else(|| format!("{c:?} is not a TCS nature of collection"));
    Ok(match tcs {
        None => None,
        Some(LedgerTcs::Payable { payable_for }) => {
            let code = payable_for.trim().to_ascii_lowercase();
            let code = if code == tcs_rules::ALL_NATURES { code } else { known(&code)? };
            if !under_duties(book, l) {
                return Err("a TCS payable ledger must be under Duties & Taxes".into());
            }
            Some(LedgerTcs::Payable { payable_for: code })
        }
        Some(LedgerTcs::Collectible(mut t)) => {
            t.nature = t.nature.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(known).transpose()?;
            t.certificate = t.certificate.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty());
            if t.nature.is_none() && t.collectee.is_none() && !t.declaration && !t.excluded && !t.non_resident {
                return Err("pick a nature of collection or the buyer's terms".into());
            }
            if under_duties(book, l) {
                return Err("a ledger under Duties & Taxes can hold TCS payable, not have TCS collected".into());
            }
            if !t.non_resident {
                t.permanent_establishment = false;
            }
            Some(LedgerTcs::Collectible(t))
        }
    })
}

/// A ledger's TCS settings.
pub fn ledger_tcs(slot: &Slot, id: &str) -> ApiResult<Option<LedgerTcs>> {
    let book = open_ref(slot)?;
    let id = LedgerId(parse_uuid(id, "ledger")?);
    Ok(book.ledger(id).ok_or("unknown ledger")?.tcs.clone())
}

/// Set (replace) a ledger's TCS settings; `None` clears them.
pub fn set_ledger_tcs(slot: &mut Slot, id: &str, tcs: Option<LedgerTcs>) -> ApiResult<Option<LedgerTcs>> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let id = LedgerId(parse_uuid(id, "ledger")?);
    let mut l = book.ledger(id).ok_or("unknown ledger")?.clone();
    if tcs.is_some() {
        india(book)?;
    }
    l.tcs = checked_tcs(book, &l, tcs)?;
    book.alter_ledger(now(), &os_user(), l).map_err(err)?;
    store.save(book).map_err(err)?;
    Ok(book.ledger(id).expect("just altered").tcs.clone())
}

fn is_gst_output(l: &Ledger) -> bool {
    l.tax.as_ref().is_some_and(|t| t.side == khata_core::TaxSide::Output)
}

/// Collections and challan payments in vouchers dated `from..=to`.
fn tcs_scan(book: &Book, from: NaiveDate, to: NaiveDate) -> Scan {
    let mut out = Scan::default();
    for v in book.active_vouchers().filter(|v| from <= v.date && v.date <= to) {
        let name = format!("{} {}", kind_label(v.kind), v.number);
        let party_at = v.entries.iter().position(|e| book.ledger(e.ledger).is_some_and(|l| is_collectee(book, l)));
        let party = party_at.and_then(|i| book.ledger(v.entries[i].ledger));
        let mut bases: BTreeMap<String, Amount> = BTreeMap::new();
        let mut gst = Amount::ZERO;
        let mut payable: Vec<(LedgerId, String, Amount)> = Vec::new();
        for (i, e) in v.entries.iter().enumerate() {
            let Some(l) = book.ledger(e.ledger) else { continue };
            if let Some(p) = tcs_payable_for(l) {
                match payable.iter_mut().find(|x| x.0 == l.id) {
                    Some(x) => x.2 -= e.signed(),
                    None => payable.push((l.id, p.to_owned(), -e.signed())),
                }
                continue;
            }
            if is_gst_output(l) {
                gst -= e.signed();
                continue;
            }
            if Some(i) == party_at || book.is_cash_or_bank(l.id) || under_duties(book, l) || is_collectee(book, l) {
                continue;
            }
            if let Some(n) = tcs_line_nature(l, party) {
                *bases.entry(n).or_default() -= e.signed();
            }
        }
        bases.retain(|_, b| !b.is_zero());
        payable.retain(|p| !p.2.is_zero());
        if bases.is_empty() && payable.is_empty() {
            continue;
        }
        let row = |nature: String, base: Amount, tcs: Amount| Deduction {
            voucher: v.id,
            kind: v.kind,
            number: v.number.clone(),
            date: v.date,
            party: party.map(|p| p.id),
            nature,
            base,
            tds: tcs,
        };
        if bases.is_empty() {
            for (ledger, payable_for, net_cr) in payable {
                if net_cr.is_sign_negative() {
                    out.challans.push(Challan { voucher: v.id, kind: v.kind, number: v.number.clone(), date: v.date, ledger, payable_for, amount: -net_cr });
                } else {
                    out.warnings.push(format!("{name}: TCS is credited but no line has a nature of collection; tag the sales ledger"));
                    out.deductions.push(row(payable_for, Amount::ZERO, net_cr));
                }
            }
            continue;
        }
        let largest = bases.iter().max_by_key(|(_, b)| b.abs()).map(|(n, _)| n.clone()).expect("not empty");
        *bases.get_mut(&largest).expect("present") += gst;
        let mut collected: BTreeMap<String, Amount> = BTreeMap::new();
        for (_, payable_for, net_cr) in &payable {
            let n = if payable_for == tcs_rules::ALL_NATURES { largest.clone() } else { payable_for.clone() };
            *collected.entry(n).or_default() += *net_cr;
        }
        // The TCS itself is part of what the buyer pays but not of the base.
        for n in collected.keys().filter(|n| !bases.contains_key(*n)) {
            out.warnings.push(format!("{name}: TCS for {} but no line has that nature of collection", tcs_nature_name(n)));
        }
        if party.is_none() && !collected.is_empty() {
            out.warnings.push(format!("{name}: TCS is collected but no line is a party (collectee) ledger"));
        }
        let natures: std::collections::BTreeSet<String> = bases.keys().chain(collected.keys()).cloned().collect();
        for n in natures {
            out.deductions.push(row(n.clone(), bases.get(&n).copied().unwrap_or_default(), collected.get(&n).copied().unwrap_or_default()));
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TcsSuggestion {
    pub applies: bool,
    /// TCS to collect, rounded to the rupee.
    pub amount: String,
    pub nature: Option<String>,
    pub nature_name: Option<String>,
    pub section: Option<String>,
    pub tax_code: Option<String>,
    pub rate: Option<String>,
    pub basis: Option<tcs_rules::TcsBasis>,
    pub base: String,
    pub explanation: String,
    pub pan: Option<String>,
    pub collectee: Option<khata_core::DeducteeKind>,
    /// Where to credit the TCS; missing means a ledger must be created.
    pub payable_ledger_id: Option<String>,
    pub payable_ledger_name: Option<String>,
    pub warnings: Vec<String>,
}

fn tcs_payable_ledger<'a>(book: &'a Book, nature: &str) -> Option<&'a Ledger> {
    book.ledgers()
        .find(|l| tcs_payable_for(l) == Some(nature))
        .or_else(|| book.ledgers().find(|l| tcs_payable_for(l) == Some(tcs_rules::ALL_NATURES)))
}

/// Suggested TCS for a Sales or Receipt voucher being entered: `amount`
/// received from or debited to `party_id` through `sales_ledger_id` on
/// `date` (the amount the buyer pays; see the module notes on GST).
pub fn tcs_for(slot: &Slot, date: &str, party_id: &str, sales_ledger_id: &str, amount: &str) -> ApiResult<TcsSuggestion> {
    let book = open_ref(slot)?;
    india(book)?;
    let date = parse_date(date)?;
    let amount = parse_amount(amount)?.abs();
    let find = |id: &str, what: &str| -> ApiResult<Option<&Ledger>> {
        match id.trim() {
            "" => Ok(None),
            id => Ok(Some(book.ledger(LedgerId(parse_uuid(id, what)?)).ok_or_else(|| format!("unknown {what}"))?)),
        }
    };
    let party = find(party_id, "party ledger")?;
    let sales = find(sales_ledger_id, "ledger")?;
    let empty = |explanation: String, warnings: Vec<String>| TcsSuggestion {
        applies: false,
        amount: "0".into(),
        nature: None,
        nature_name: None,
        section: None,
        tax_code: None,
        rate: None,
        basis: None,
        base: "0".into(),
        explanation,
        pan: None,
        collectee: None,
        payable_ledger_id: None,
        payable_ledger_name: None,
        warnings,
    };
    let nature = sales.and_then(|s| tcs_line_nature(s, party)).or_else(|| party.and_then(tcs_terms).and_then(|t| t.nature.clone()));
    let Some(nature) = nature else {
        return Ok(empty("No nature of collection on these ledgers: no TCS suggested.".into(), Vec::new()));
    };
    let mut warnings = Vec::new();
    if party.is_none() {
        warnings.push("No party: the buyer's PAN and declaration are unknown.".into());
    }
    let pan = party.and_then(party_pan);
    let t = party.and_then(tcs_terms);
    let certificate = t.and_then(|t| {
        t.lower_rate.map(|rate| LowerCertificate { rate, number: t.certificate.clone(), valid_from: t.valid_from, valid_to: t.valid_to })
    });
    let r = tcs_rules::compute(&tcs_rules::TcsInput {
        nature: &nature,
        collectee: party.and_then(collectee_kind),
        pan: pan.is_some(),
        date,
        amount,
        declaration: t.is_some_and(|t| t.declaration),
        excluded: t.is_some_and(|t| t.excluded),
        non_resident: t.is_some_and(|t| t.non_resident),
        permanent_establishment: t.is_some_and(|t| t.permanent_establishment),
        certificate,
    })
    .map_err(err)?;
    warnings.extend(r.cautions.iter().cloned());
    let ledger = tcs_payable_ledger(book, &nature);
    if ledger.is_none() && !r.tcs.is_zero() {
        warnings.push(format!(
            "Create a ledger under Duties & Taxes marked as TCS payable for {} (or for all natures) to credit the TCS to.",
            tcs_nature_name(&nature)
        ));
    }
    Ok(TcsSuggestion {
        applies: !r.tcs.is_zero(),
        amount: amt(r.tcs),
        nature_name: Some(tcs_nature_name(&nature)),
        nature: Some(nature),
        section: Some(r.section.clone()),
        tax_code: Some(r.tax_code.into()),
        rate: Some(amt(r.rate)),
        basis: Some(r.basis),
        base: amt(r.base),
        explanation: r.explanation.clone(),
        pan: pan.map(|p| p.value),
        collectee: party.and_then(collectee_kind),
        payable_ledger_id: ledger.map(|l| l.id.0.to_string()),
        payable_ledger_name: ledger.map(|l| l.name.clone()),
        warnings,
    })
}

fn tcs_order(code: &str) -> usize {
    TCS_NATURES.iter().position(|n| n.code == code).unwrap_or(TCS_NATURES.len())
}

/// Collections and challan payments in a period, by nature, with what is
/// still owed and when each month's TCS is due. Same shape as the TDS
/// payable report ("paid / credited" is the amount received or debited,
/// "deducted" the TCS collected).
pub fn tcs_payable(slot: &Slot, from: &str, to: &str) -> ApiResult<TdsPayableDto> {
    let book = open_ref(slot)?;
    india(book)?;
    let (from_d, to_d) = (parse_date(from)?, parse_date(to)?);
    if from_d > to_d {
        return Err("the period ends before it starts".into());
    }
    let s = tcs_scan(book, from_d, to_d);
    let closing = book.balances(Some(to_d));
    let opening = from_d.pred_opt().map(|d| book.balances(Some(d))).unwrap_or_default();
    let owed = |m: &BTreeMap<LedgerId, Amount>, l: &Ledger| -m.get(&l.id).copied().unwrap_or(l.opening);
    let mut ledgers: Vec<PayableLedgerRow> = book
        .ledgers()
        .filter_map(|l| {
            tcs_payable_for(l).map(|p| PayableLedgerRow {
                id: l.id.0.to_string(),
                name: l.name.clone(),
                payable_for: p.to_owned(),
                opening: amt(owed(&opening, l)),
                closing: amt(owed(&closing, l)),
            })
        })
        .collect();
    ledgers.sort_by_key(|l| (tcs_order(&l.payable_for), l.name.to_lowercase()));
    let mut codes: Vec<String> = s.deductions.iter().map(|d| d.nature.clone()).chain(s.challans.iter().map(|c| c.payable_for.clone())).collect();
    codes.sort_by_key(|c| (tcs_order(c), c.clone()));
    codes.dedup();
    let section_of = |code: &str| tcs_rules::nature(code).map(|n| n.section()).unwrap_or_default();
    let natures = codes
        .into_iter()
        .map(|code| {
            let mine: Vec<&Deduction> = s.deductions.iter().filter(|d| d.nature == code).collect();
            let mut months: BTreeMap<(i32, u32), Amount> = BTreeMap::new();
            for d in &mine {
                *months.entry((d.date.year(), d.date.month())).or_default() += d.tds;
            }
            let tagged: Vec<&Ledger> = book.ledgers().filter(|l| tcs_payable_for(l) == Some(code.as_str())).collect();
            NatureTotals {
                name: tcs_nature_name(&code),
                section: section_of(&code),
                paid_credited: amt(mine.iter().map(|d| d.base).sum()),
                deducted: amt(mine.iter().map(|d| d.tds).sum()),
                deposited: amt(s.challans.iter().filter(|c| c.payable_for == code).map(|c| c.amount).sum()),
                outstanding: (!tagged.is_empty()).then(|| amt(tagged.iter().map(|l| owed(&closing, l)).sum())),
                by_month: months
                    .into_iter()
                    .filter(|(_, a)| !a.is_zero())
                    .map(|((y, m), a)| MonthDue {
                        month: format!("{y}-{m:02}"),
                        deducted: amt(a),
                        due: NaiveDate::from_ymd_opt(y, m, 1).map(tcs_rules::deposit_due).map(|d| d.to_string()).unwrap_or_default(),
                    })
                    .collect(),
                nature: code,
            }
        })
        .collect();
    let mut warnings = s.warnings;
    if ledgers.is_empty() {
        warnings.push("No ledger is marked as TCS payable: collections and challans cannot be found.".into());
    }
    warnings.push("TCS is due within seven days from the end of the month (old Rule 37CA; the 2026 rules' date is from secondary sources).".into());
    let rows = s
        .deductions
        .iter()
        .map(|d| DeductionRow {
            nature_name: tcs_nature_name(&d.nature),
            section: section_of(&d.nature),
            deposit_due: tcs_rules::deposit_due(d.date).to_string(),
            ..deduction_row(book, d)
        })
        .collect();
    Ok(TdsPayableDto {
        from: from.into(),
        to: to.into(),
        paid_credited: amt(s.deductions.iter().map(|d| d.base).sum()),
        deducted: amt(s.deductions.iter().map(|d| d.tds).sum()),
        deposited: amt(s.challans.iter().map(|c| c.amount).sum()),
        rows,
        challans: s.challans.iter().map(|c| challan_row(book, c)).collect(),
        natures,
        ledgers,
        warnings,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TcsStatementRow {
    pub serial: usize,
    pub party_id: Option<String>,
    pub party_name: String,
    pub pan: Option<String>,
    pub collectee: Option<khata_core::DeducteeKind>,
    pub nature: String,
    pub nature_name: String,
    /// 4-digit collection code.
    pub tax_code: String,
    pub date: String,
    /// Amount received or debited.
    pub amount: String,
    pub tcs: String,
    /// Percent, two decimals.
    pub rate: String,
    /// Form 143 reason flag: A certificate, B declaration, C no PAN.
    pub remark: Option<String>,
    pub reason: Option<String>,
    pub non_resident: bool,
    pub voucher_id: String,
    pub kind: VoucherKind,
    pub number: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TcsQuarterlyDto {
    pub year: String,
    pub quarter: u32,
    pub from: String,
    pub to: String,
    /// "Form 143" (2026-27 onward) or "Form 27EQ".
    pub form: String,
    pub due: Option<String>,
    pub deductor_name: String,
    pub deductor_pan: Option<String>,
    pub rows: Vec<TcsStatementRow>,
    pub challans: Vec<ChallanRow>,
    pub amount: String,
    pub tcs: String,
    pub deposited: String,
    pub warnings: Vec<String>,
}

fn tcs_reason(r: Option<char>) -> Option<String> {
    r.map(|c| match c {
        'A' => "Lower collection certificate".to_owned(),
        'B' => "Buyer's declaration (s.394(2))".to_owned(),
        'C' => "No PAN: higher rate".to_owned(),
        x => x.to_string(),
    })
}

/// The collectee-wise data for the quarterly TCS statement.
pub fn tcs_quarterly(slot: &Slot, year: i32, quarter: u32) -> ApiResult<TcsQuarterlyDto> {
    let book = open_ref(slot)?;
    india(book)?;
    let q = Quarter { year, q: quarter };
    let (from, to) = q.range().ok_or("the quarter must be 1 to 4")?;
    let s = tcs_scan(book, from, to);
    let mut warnings = s.warnings;
    let mut rows: Vec<TcsStatementRow> = Vec::new();
    for d in &s.deductions {
        let Some(n) = tcs_rules::nature(&d.nature) else {
            if d.nature != tcs_rules::ALL_NATURES {
                warnings.push(format!("{} {}: unknown nature {:?}", kind_label(d.kind), d.number, d.nature));
            }
            continue;
        };
        let party = d.party.and_then(|p| book.ledger(p));
        let pan = party.and_then(party_pan);
        let t = party.and_then(tcs_terms);
        let cert_valid = t.filter(|t| t.lower_rate.is_some()).is_some_and(|t| {
            t.valid_from.is_none_or(|f| f <= d.date) && t.valid_to.is_none_or(|x| d.date <= x)
        });
        let remark = if t.is_some_and(|t| t.declaration && !t.non_resident) && pan.is_some() && d.tds.is_zero() && n.declaration_allowed {
            Some('B')
        } else if pan.is_none() && !(t.is_some_and(|t| t.non_resident && !t.permanent_establishment)) {
            Some('C')
        } else if cert_valid && pan.is_some() {
            Some('A')
        } else {
            None
        };
        if d.tds.is_zero() && remark.is_none() {
            continue;
        }
        if t.is_some_and(|t| t.excluded) {
            continue;
        }
        if party.is_none() {
            warnings.push(format!("{} {}: TCS without a collectee ledger", kind_label(d.kind), d.number));
        } else if pan.is_none() {
            warnings.push(format!("{}: no PAN on record", ledger_name(book, d.party)));
        }
        let rate = if d.base.is_zero() { Decimal::ZERO } else { (d.tds * Decimal::ONE_HUNDRED / d.base).round_dp(2) };
        rows.push(TcsStatementRow {
            serial: rows.len() + 1,
            party_id: d.party.map(|p| p.0.to_string()),
            party_name: party.map(|p| p.name.clone()).unwrap_or_else(|| "(no party)".into()),
            pan: pan.map(|p| p.value),
            collectee: party.and_then(collectee_kind),
            nature: n.code.into(),
            nature_name: n.name.into(),
            tax_code: n.tax_code.into(),
            date: d.date.to_string(),
            amount: amt(d.base),
            tcs: amt(d.tds),
            rate: format!("{rate:.2}"),
            remark: remark.map(|c| c.to_string()),
            reason: tcs_reason(remark),
            non_resident: t.is_some_and(|t| t.non_resident),
            voucher_id: d.voucher.0.to_string(),
            kind: d.kind,
            number: d.number.clone(),
        });
    }
    warnings.sort();
    warnings.dedup();
    let gstin = book.profile().and_then(|p| p.tax_id.clone());
    let sum = |f: fn(&TcsStatementRow) -> &str| amt(rows.iter().map(|r| parse_amount(f(r)).unwrap_or_default()).sum());
    Ok(TcsQuarterlyDto {
        year: q.year_label(),
        quarter,
        from: from.to_string(),
        to: to.to_string(),
        form: tcs_rules::statement_form(q).into(),
        due: q.statement_due().map(|d| d.to_string()),
        deductor_name: book.name.clone(),
        deductor_pan: gstin.as_deref().and_then(pan::from_gstin).map(|p| p.value),
        amount: sum(|r| &r.amount),
        tcs: sum(|r| &r.tcs),
        deposited: amt(s.challans.iter().map(|c| c.amount).sum()),
        challans: s.challans.iter().map(|c| challan_row(book, c)).collect(),
        rows,
        warnings,
    })
}

// ============================================== non-residents (Form 144)

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NonResidentRowDto {
    pub serial: usize,
    pub party_id: Option<String>,
    pub party_name: String,
    pub pan: Option<String>,
    pub deductee: Option<khata_core::DeducteeKind>,
    pub tax_code: String,
    pub date: String,
    pub amount: String,
    pub tds: String,
    pub rate: String,
    /// The rate on the party (the user's), percent.
    pub user_rate: Option<String>,
    /// The rate is the treaty's (Form 144 "B") rather than the Act's ("A").
    pub treaty: bool,
    pub country: Option<String>,
    pub country_name: Option<String>,
    pub remittance: Option<String>,
    pub foreign_tin: Option<String>,
    pub remark: Option<String>,
    pub voucher_id: String,
    pub kind: VoucherKind,
    pub number: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NonResidentQuarterlyDto {
    pub year: String,
    pub quarter: u32,
    pub from: String,
    pub to: String,
    /// "Form 144" (2026-27 onward) or "Form 27Q".
    pub form: String,
    pub due: Option<String>,
    pub rows: Vec<NonResidentRowDto>,
    pub challans: Vec<ChallanRow>,
    pub amount: String,
    pub tds: String,
    pub warnings: Vec<String>,
    /// Always shown: rates are the user's, and the Form 145/146 reminder.
    pub reminders: Vec<String>,
}

fn nr_terms(l: &Ledger) -> Option<&NonResidentTerms> {
    terms(l).and_then(|t| t.non_resident.as_ref())
}

/// Payments to non-residents in a quarter, for Form 144 (ex-27Q).
pub fn tds_quarterly_nr(slot: &Slot, year: i32, quarter: u32) -> ApiResult<NonResidentQuarterlyDto> {
    let book = open_ref(slot)?;
    india(book)?;
    let q = Quarter { year, q: quarter };
    let (from, to) = q.range().ok_or("the quarter must be 1 to 4")?;
    let s = scan(book, from, to);
    let mut warnings = Vec::new();
    let mut rows: Vec<NonResidentRowDto> = Vec::new();
    for d in s.deductions.iter().filter(|d| rules::nature(&d.nature).is_some_and(|n| n.statement == Statement::NonResident)) {
        let n = rules::nature(&d.nature).expect("filtered");
        let party = d.party.and_then(|p| book.ledger(p));
        let pan = party.and_then(party_pan);
        let nr = party.and_then(nr_terms);
        if d.tds.is_zero() {
            continue;
        }
        let what = format!("{} {}", kind_label(d.kind), d.number);
        match (party, nr) {
            (None, _) => warnings.push(format!("{what}: TDS without a deductee ledger")),
            (Some(p), None) => warnings.push(format!("{}: enter the non-resident's rate, country and nature of remittance", p.name)),
            (Some(p), Some(t)) => {
                if t.country.is_none() {
                    warnings.push(format!("{}: the country of residence is missing", p.name));
                }
                if t.remittance.is_none() {
                    warnings.push(format!("{}: the nature of remittance is missing", p.name));
                }
            }
        }
        let rate = if d.base.is_zero() { Decimal::ZERO } else { (d.tds * Decimal::ONE_HUNDRED / d.base).round_dp(2) };
        let deductee = party.and_then(terms).and_then(|t| t.deductee).or(pan.as_ref().map(|p| p.holder));
        rows.push(NonResidentRowDto {
            serial: rows.len() + 1,
            party_id: d.party.map(|p| p.0.to_string()),
            party_name: party.map(|p| p.name.clone()).unwrap_or_else(|| "(no party)".into()),
            remark: pan.is_none().then(|| "C".to_owned()),
            pan: pan.map(|p| p.value),
            deductee,
            tax_code: n.tax_code(deductee).into(),
            date: d.date.to_string(),
            amount: amt(d.base),
            tds: amt(d.tds),
            rate: format!("{rate:.2}"),
            user_rate: nr.map(|t| amt(t.rate)),
            treaty: nr.is_some_and(|t| t.treaty),
            country_name: nr.and_then(|t| t.country.as_deref()).and_then(khata_in::tds_file::country).map(str::to_owned),
            country: nr.and_then(|t| t.country.clone()),
            remittance: nr.and_then(|t| t.remittance.clone()),
            foreign_tin: nr.and_then(|t| t.foreign_tin.clone()),
            voucher_id: d.voucher.0.to_string(),
            kind: d.kind,
            number: d.number.clone(),
        });
    }
    warnings.extend(s.warnings);
    warnings.sort();
    warnings.dedup();
    let natures: Vec<&str> = NATURES.iter().filter(|n| n.statement == Statement::NonResident).map(|n| n.code).collect();
    let challans: Vec<ChallanRow> = s.challans.iter().filter(|c| natures.contains(&c.payable_for.as_str())).map(|c| challan_row(book, c)).collect();
    let sum = |f: fn(&NonResidentRowDto) -> &str| amt(rows.iter().map(|r| parse_amount(f(r)).unwrap_or_default()).sum());
    Ok(NonResidentQuarterlyDto {
        year: q.year_label(),
        quarter,
        from: from.to_string(),
        to: to.to_string(),
        form: if q.year >= rules::NEW_ACT_FROM.year() { "Form 144".into() } else { "Form 27Q".into() },
        due: q.statement_due().map(|d| d.to_string()),
        amount: sum(|r| &r.amount),
        tds: sum(|r| &r.tds),
        rows,
        challans,
        warnings,
        reminders: vec![
            rules::NON_RESIDENT_REMINDER.to_owned(),
            "Challans paid from an all-natures TDS ledger are listed on Form 140 only; keep a TDS payable ledger for \
             non-residents so their challans can be matched here."
                .to_owned(),
        ],
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodeName {
    pub code: String,
    pub name: String,
}

/// Form 144 country codes (Annexure 10) and natures of remittance (Annexure 9).
pub fn tds_nr_codes() -> (Vec<CodeName>, Vec<CodeName>) {
    let list = |l: &[(&str, &str)]| l.iter().map(|(c, n)| CodeName { code: (*c).into(), name: (*n).into() }).collect();
    (list(khata_in::tds_file::COUNTRIES), list(khata_in::tds_file::REMITTANCES))
}

// ============================================ deductor and challan settings

use khata_in::tds_file::{self as file, Form};

// The TAN, the deductor and the challans' bank details are the company's
// shared records (synced, `docs/sync-settings.md`); the file sequence is a
// counter of the files this computer made, a device-local setting.
const TAN_KEY: &str = "tds.tan";
const DEDUCTOR_KEY: &str = "tds.deductor";
const SEQUENCE_KEY: &str = "tds.file_sequence";
const CHALLAN_PREFIX: &str = "tds.challan.";

fn store_ref(slot: &Slot) -> ApiResult<&khata_store::Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

/// A shared record of the company, as JSON.
fn read_setting<T: for<'de> Deserialize<'de>>(store: &khata_store::Store, key: &str) -> ApiResult<Option<T>> {
    match store.record(key).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map(Some).map_err(|e| format!("setting {key} is damaged: {e}")),
        None => Ok(None),
    }
}

fn write_setting<T: Serialize>(store: &khata_store::Store, key: &str, value: &T) -> ApiResult<()> {
    store.set_record(key, Some(&serde_json::to_string(value).map_err(err)?), &crate::api::os_user()).map(|_| ()).map_err(err)
}

/// The deductor's and responsible person's details for the statements.
/// Shared records of the company (TAN under `tds.tan`), not in the books,
/// until the company profile holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeductorSettings {
    pub tan: String,
    /// Empty: the PAN inside the company GSTIN.
    pub pan: String,
    /// Annexure 4 code: K company, F firm, Q individual / HUF, P AOP, T trust,
    /// B BOI, J artificial juridical person, M branch of a company.
    pub deductor_type: String,
    /// Empty: the company address.
    pub address: String,
    /// GST state code ("27"); empty: the company's.
    pub state: String,
    pub pin: String,
    pub email: String,
    /// Calling code, "91" when empty.
    pub phone_country: String,
    pub phone: String,
    pub responsible_name: String,
    pub responsible_designation: String,
    pub responsible_pan: String,
    /// Empty: the deductor's address, state and PIN.
    pub responsible_address: String,
    pub responsible_state: String,
    pub responsible_pin: String,
    pub responsible_email: String,
    pub responsible_phone_country: String,
    pub responsible_phone: String,
    /// Country of the responsible person, "INDIA" when empty.
    pub responsible_country: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeductorDto {
    pub stored: DeductorSettings,
    /// What the statement files will use (defaults filled in).
    pub effective: file::Deductor,
    pub responsible: file::Responsible,
    /// Deductor types for the picker.
    pub deductor_types: Vec<CodeName>,
    /// What is still missing or wrong for a statement file.
    pub problems: Vec<String>,
}

fn or<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.trim().is_empty() { b } else { a.trim() }
}

fn effective_deductor(book: &Book, s: &DeductorSettings) -> (file::Deductor, file::Responsible, Vec<String>) {
    let mut problems = Vec::new();
    let profile = book.profile();
    let gstin = profile.and_then(|p| p.tax_id.clone());
    let region = profile.and_then(|p| p.tax_region.clone()).or_else(|| gstin.as_deref().and_then(|g| g.get(..2)).map(str::to_owned));
    let company_address = profile.map(|p| p.address.clone()).unwrap_or_default();
    let gst_pan = gstin.as_deref().and_then(pan::from_gstin).map(|p| p.value).unwrap_or_default();
    let address_text = or(&s.address, &company_address).to_owned();
    let lines = |text: &str, who: &str, problems: &mut Vec<String>| {
        file::address_lines(text).unwrap_or_else(|| {
            problems.push(format!("{who}'s address does not fit five lines of 25 characters; shorten it"));
            Vec::new()
        })
    };
    let state_of = |gst: &str| file::state_from_gst(gst).unwrap_or_default().to_owned();
    let d_state = state_of(or(&s.state, region.as_deref().unwrap_or("")));
    let deductor = file::Deductor {
        tan: s.tan.trim().to_uppercase(),
        pan: or(&s.pan, &gst_pan).to_uppercase(),
        name: book.name.clone(),
        address: lines(&address_text, "The deductor", &mut problems),
        state: d_state.clone(),
        pin: s.pin.trim().to_owned(),
        email: s.email.trim().to_owned(),
        phone_country: or(&s.phone_country, "91").to_owned(),
        phone: s.phone.trim().to_owned(),
        deductor_type: s.deductor_type.trim().to_uppercase(),
        gstin: gstin.clone(),
    };
    let r_address = or(&s.responsible_address, &address_text).to_owned();
    let responsible = file::Responsible {
        name: s.responsible_name.trim().to_owned(),
        designation: s.responsible_designation.trim().to_owned(),
        pan: s.responsible_pan.trim().to_uppercase(),
        address: lines(&r_address, "The responsible person", &mut problems),
        state: if s.responsible_state.trim().is_empty() { d_state } else { state_of(&s.responsible_state) },
        pin: or(&s.responsible_pin, &s.pin).to_owned(),
        email: or(&s.responsible_email, &s.email).to_owned(),
        country: or(&s.responsible_country, "INDIA").to_uppercase(),
        phone_country: or(&s.responsible_phone_country, or(&s.phone_country, "91")).to_owned(),
        phone: or(&s.responsible_phone, &s.phone).to_owned(),
    };
    (deductor, responsible, problems)
}

fn deductor_dto(book: &Book, stored: DeductorSettings) -> DeductorDto {
    let (effective, responsible, mut problems) = effective_deductor(book, &stored);
    // Reuse the file's checks on a one-row dummy statement for the header fields.
    let probe = file::Statement {
        form: Form::F140,
        quarter: Quarter { year: rules::NEW_ACT_FROM.year(), q: 1 },
        created: rules::NEW_ACT_FROM,
        sequence: 1,
        deductor: effective.clone(),
        responsible: responsible.clone(),
        previous_token: None,
        challans: Vec::new(),
    };
    if let Err(all) = file::render(&probe) {
        problems.extend(all.into_iter().filter(|p| !p.contains("challan") && !p.contains("at least one")));
    }
    DeductorDto {
        stored,
        effective,
        responsible,
        deductor_types: file::DEDUCTOR_TYPES.iter().map(|(c, n)| CodeName { code: (*c).into(), name: (*n).into() }).collect(),
        problems,
    }
}

pub fn tds_deductor(slot: &Slot) -> ApiResult<DeductorDto> {
    let book = open_ref(slot)?;
    india(book)?;
    let store = store_ref(slot)?;
    let mut stored: DeductorSettings = read_setting(store, DEDUCTOR_KEY)?.unwrap_or_default();
    stored.tan = store.record(TAN_KEY).map_err(err)?.unwrap_or_default();
    Ok(deductor_dto(book, stored))
}

/// Save the deductor's details (owner or accountant). The TAN is checked;
/// other fields are checked when a file is made (and listed in `problems`).
pub fn set_tds_deductor(slot: &mut Slot, settings: DeductorSettings) -> ApiResult<DeductorDto> {
    {
        let (store, book) = open_mut(slot)?;
        crate::api::require(book, UserRole::Accountant)?;
        india(book)?;
        let tan = settings.tan.trim().to_uppercase();
        let b = tan.as_bytes();
        let tan_ok = tan.is_empty()
            || (b.len() == 10 && b[..4].iter().all(u8::is_ascii_uppercase) && b[4..9].iter().all(u8::is_ascii_digit) && b[9].is_ascii_uppercase());
        if !tan_ok {
            return Err("a TAN is 4 letters, 5 digits and a letter (like MUMA12345B)".into());
        }
        store.set_record(TAN_KEY, (!tan.is_empty()).then_some(tan.as_str()), &crate::api::os_user()).map_err(err)?;
        let rest = DeductorSettings { tan: String::new(), ..settings };
        write_setting(store, DEDUCTOR_KEY, &rest)?;
    }
    tds_deductor(slot)
}

/// What the bank's challan says, which the books do not hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChallanDetails {
    /// 7-digit BSR code.
    pub bsr: String,
    /// Challan serial number (up to 5 digits).
    pub serial: String,
    /// Date of deposit as the bank records it; empty: the voucher's date.
    pub date: String,
    /// Parts of the voucher's amount that are not tax (whole rupees).
    pub interest: String,
    pub fee: String,
    pub others: String,
    /// 200 (payable by taxpayer, the default) or 400.
    pub minor_head: String,
}

pub fn tds_challan(slot: &Slot, voucher_id: &str) -> ApiResult<Option<ChallanDetails>> {
    open_ref(slot)?;
    let id = parse_uuid(voucher_id, "voucher")?;
    read_setting(store_ref(slot)?, &format!("{CHALLAN_PREFIX}{id}"))
}

/// Save (or, with `None`, clear) a challan voucher's bank details.
pub fn set_tds_challan(slot: &mut Slot, voucher_id: &str, details: Option<ChallanDetails>) -> ApiResult<Option<ChallanDetails>> {
    let (store, book) = open_mut(slot)?;
    crate::api::require(book, UserRole::Accountant)?;
    let id = parse_uuid(voucher_id, "voucher")?;
    book.voucher(VoucherId(id)).ok_or("unknown voucher")?;
    let key = format!("{CHALLAN_PREFIX}{id}");
    let Some(d) = details else {
        store.set_record(&key, None, &crate::api::os_user()).map_err(err)?;
        return Ok(None);
    };
    let d = ChallanDetails {
        bsr: d.bsr.trim().to_owned(),
        serial: d.serial.trim().to_owned(),
        date: d.date.trim().to_owned(),
        interest: d.interest.trim().to_owned(),
        fee: d.fee.trim().to_owned(),
        others: d.others.trim().to_owned(),
        minor_head: or(&d.minor_head, "200").to_owned(),
    };
    if !(d.bsr.len() == 7 && d.bsr.bytes().all(|b| b.is_ascii_digit())) {
        return Err("the BSR code has 7 digits".into());
    }
    if !((1..=5).contains(&d.serial.len()) && d.serial.bytes().all(|b| b.is_ascii_digit())) {
        return Err("the challan serial number is 1 to 5 digits".into());
    }
    if !d.date.is_empty() {
        parse_date(&d.date)?;
    }
    for (what, v) in [("interest", &d.interest), ("fee", &d.fee), ("others", &d.others)] {
        if !v.is_empty() && !parse_amount(v).is_ok_and(|a| a.fract().is_zero() && !a.is_sign_negative()) {
            return Err(format!("the challan's {what} is whole rupees"));
        }
    }
    if !["200", "400"].contains(&d.minor_head.as_str()) {
        return Err("the minor head is 200 or 400".into());
    }
    write_setting(store, &key, &d)?;
    Ok(Some(d))
}

// ============================================== the statement text file

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatementFileDto {
    /// "Form 140 (ex-26Q)".
    pub form: String,
    /// "140RQ1-2026-27.txt".
    pub file_name: String,
    /// The file; `None` when `problems` is not empty.
    pub text: Option<String>,
    pub problems: Vec<String>,
    /// Always shown: validate with the FVU before filing.
    pub notes: Vec<String>,
}

struct Pending {
    voucher: VoucherId,
    date: NaiveDate,
    payable_for: String,
    amount: Amount,
}

/// Rows of one form as the file module wants them, before challans.
struct FileRow {
    nature: String,
    row: file::Row,
}

fn whole_amount(s: &str) -> ApiResult<Amount> {
    if s.trim().is_empty() { Ok(Amount::ZERO) } else { parse_amount(s) }
}

/// The official statement text file for a quarter: `form` is "140", "143"
/// or "144"; `previous_token` the 15-digit token of the previous regular
/// statement of that form, when one was filed. Problems (missing TAN,
/// challan details, PANs…) come back in `problems` instead of a file.
pub fn tds_statement_file(slot: &mut Slot, form: &str, year: i32, quarter: u32, previous_token: Option<&str>) -> ApiResult<StatementFileDto> {
    let form = match form.trim() {
        "140" => Form::F140,
        "143" => Form::F143,
        "144" => Form::F144,
        f => return Err(format!("{f:?} is not a statement form (140, 143 or 144)")),
    };
    let book = open_ref(slot)?;
    india(book)?;
    let store = store_ref(slot)?;
    let q = Quarter { year, q: quarter };
    let (from, to) = q.range().ok_or("the quarter must be 1 to 4")?;
    let mut problems = Vec::new();
    let mut settings: DeductorSettings = read_setting(store, DEDUCTOR_KEY)?.unwrap_or_default();
    settings.tan = store.record(TAN_KEY).map_err(err)?.unwrap_or_default();
    let (deductor, responsible, p) = effective_deductor(book, &settings);
    problems.extend(p);
    let s = if form == Form::F143 { tcs_scan(book, from, to) } else { scan(book, from, to) };
    // Rows of this form, oldest first.
    let mut rows: Vec<FileRow> = Vec::new();
    match form {
        Form::F140 => {
            let data = tds_quarterly(slot, year, quarter)?;
            for r in &data.rows {
                let party = r.party_id.as_deref().and_then(|p| parse_uuid(p, "party").ok()).and_then(|id| book.ledger(LedgerId(id)));
                let cert = party.and_then(terms).and_then(|t| t.certificate.clone());
                let remark = match r.reason.as_deref() {
                    Some(x) if x.starts_with("No PAN") => Some('C'),
                    Some(x) if x.starts_with("Lower") => Some('A'),
                    _ => None,
                };
                rows.push(FileRow {
                    nature: r.nature.clone(),
                    row: file_row(r.pan.clone(), &r.party_name, &r.tax_code, &r.date, &r.amount, &r.tds, r.deductee, remark, cert.filter(|_| remark == Some('A'))),
                });
            }
        }
        Form::F143 => {
            let data = tcs_quarterly(slot, year, quarter)?;
            for r in &data.rows {
                let party = r.party_id.as_deref().and_then(|p| parse_uuid(p, "party").ok()).and_then(|id| book.ledger(LedgerId(id)));
                let t = party.and_then(tcs_terms);
                let remark = r.remark.as_deref().and_then(|x| x.chars().next());
                let mut row = file_row(r.pan.clone(), &r.party_name, &r.tax_code, &r.date, &r.amount, &r.tcs, r.collectee, remark, t.and_then(|t| t.certificate.clone()).filter(|_| remark == Some('A')));
                row.non_resident = t.is_some_and(|t| t.non_resident);
                row.permanent_establishment = t.is_some_and(|t| t.permanent_establishment);
                rows.push(FileRow { nature: r.nature.clone(), row });
            }
        }
        Form::F144 => {
            let data = tds_quarterly_nr(slot, year, quarter)?;
            for r in &data.rows {
                let party = r.party_id.as_deref().and_then(|p| parse_uuid(p, "party").ok()).and_then(|id| book.ledger(LedgerId(id)));
                let mailing = party.and_then(|p| p.mailing.clone()).unwrap_or_default();
                let remark = r.remark.as_deref().and_then(|x| x.chars().next());
                let mut row = file_row(r.pan.clone(), &r.party_name, &r.tax_code, &r.date, &r.amount, &r.tds, r.deductee, remark, None);
                row.nr = Some(file::NonResidentRow {
                    remittance: r.remittance.clone(),
                    country: r.country.clone().unwrap_or_default(),
                    treaty: r.treaty,
                    grossed_up: false,
                    address: mailing.address.replace('\n', ", "),
                    email: mailing.email,
                    phone: mailing.phone.chars().filter(char::is_ascii_digit).collect(),
                    tin: r.foreign_tin.clone().unwrap_or_default(),
                });
                rows.push(FileRow { nature: "non_resident".into(), row });
            }
        }
    }
    // Challans of this form with their bank details.
    let all = if form == Form::F143 { tcs_rules::ALL_NATURES } else { ALL_NATURES };
    let in_form = |payable_for: &str| match form {
        Form::F143 => true,
        Form::F140 => payable_for == ALL_NATURES || rules::nature(payable_for).is_some_and(|n| n.statement == Statement::Quarterly),
        Form::F144 => rules::nature(payable_for).is_some_and(|n| n.statement == Statement::NonResident),
    };
    let mut pending: Vec<Pending> = s
        .challans
        .iter()
        .filter(|c| in_form(&c.payable_for))
        .map(|c| Pending { voucher: c.voucher, date: c.date, payable_for: c.payable_for.clone(), amount: c.amount })
        .collect();
    // Deposits for the quarter's last month are often made after it ends.
    let late = if form == Form::F143 { tcs_rules::deposit_due(to) } else { rules::deposit_due(to) };
    if let Some(next) = to.succ_opt() {
        let after = if form == Form::F143 { tcs_scan(book, next, late) } else { scan(book, next, late) };
        pending.extend(after.challans.iter().filter(|c| in_form(&c.payable_for)).map(|c| Pending {
            voucher: c.voucher,
            date: c.date,
            payable_for: c.payable_for.clone(),
            amount: c.amount,
        }));
    }
    pending.sort_by_key(|p| p.date);
    let mut challans: Vec<(String, file::Challan, Amount)> = Vec::new();
    for p in &pending {
        let v = book.voucher(p.voucher).map(|v| format!("{} {}", kind_label(v.kind), v.number)).unwrap_or_default();
        let Some(d) = read_setting::<ChallanDetails>(store, &format!("{CHALLAN_PREFIX}{}", p.voucher.0))? else {
            problems.push(format!("Challan {v} ({}): enter its BSR code, serial number and deposit date", p.date));
            continue;
        };
        let (interest, fee, others) = (whole_amount(&d.interest)?, whole_amount(&d.fee)?, whole_amount(&d.others)?);
        let tax = p.amount - interest - fee - others;
        if tax.is_sign_negative() || !tax.fract().is_zero() {
            problems.push(format!("Challan {v}: the tax part ({}) must be whole rupees and not negative", amt(tax)));
            continue;
        }
        let date = if d.date.is_empty() { p.date } else { parse_date(&d.date)? };
        challans.push((
            p.payable_for.clone(),
            file::Challan { tax, interest, fee, others, bsr: d.bsr, serial: d.serial, date, minor_head: or(&d.minor_head, "200").to_owned(), rows: Vec::new() },
            tax,
        ));
    }
    // Allocate rows to challans: same nature (or the all-natures ledger), in date order.
    for r in rows {
        let fits = |c: &(String, file::Challan, Amount)| (c.0 == r.nature || c.0 == all) && c.2 >= r.row.tax;
        match challans.iter_mut().find(|c| fits(c)) {
            Some(c) => {
                c.2 -= r.row.tax;
                c.1.rows.push(r.row);
            }
            None => problems.push(format!(
                "{} ({}): no challan with ₹{} left for its {} of ₹{}",
                r.row.name,
                r.row.date,
                if r.row.tax.is_zero() { "room".to_owned() } else { amt(r.row.tax) },
                if form == Form::F143 { "TCS" } else { "TDS" },
                amt(r.row.tax)
            )),
        }
    }
    let used: Vec<file::Challan> = challans.into_iter().map(|c| c.1).filter(|c| !c.rows.is_empty()).collect();
    let created = crate::api::today();
    let sequence = store.setting(SEQUENCE_KEY).map_err(err)?.and_then(|s| s.parse::<u32>().ok()).unwrap_or(0) + 1;
    let statement = file::Statement {
        form,
        quarter: q,
        created,
        sequence,
        deductor,
        responsible,
        previous_token: previous_token.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned),
        challans: used,
    };
    let rendered = file::render(&statement);
    let text = match rendered {
        Ok(t) if problems.is_empty() => Some(t),
        Ok(_) => None,
        Err(all) => {
            problems.extend(all);
            None
        }
    };
    problems.dedup();
    if text.is_some() {
        if let Some((store, _)) = slot.as_mut() {
            store.set_setting(SEQUENCE_KEY, Some(&sequence.to_string())).map_err(err)?;
        }
    }
    Ok(StatementFileDto {
        form: form.label(),
        file_name: format!("{}RQ{}-{}.txt", form.number(), quarter, q.year_label()),
        text,
        problems,
        notes: vec![
            format!(
                "Written to Protean's file format v{} for tax year 2026-27 onward. Validate it with the official File \
                 Validation Utility (with the challan status .csi file) before filing; KhataOS does not run the FVU.",
                file::FORMAT_VERSION
            ),
            "Regular statement only (no corrections). Optional fields are left empty: Form 121 numbers, the Form 145 \
             acknowledgement, government deductor fields."
                .into(),
            "Collectees and non-resident deductees are reported as not opting out of the s.202(1) regime, and amounts \
             as not grossed up; change them in the RPU if that is wrong."
                .into(),
        ],
    })
}

#[allow(clippy::too_many_arguments)]
fn file_row(
    pan: Option<String>,
    name: &str,
    code: &str,
    date: &str,
    amount: &str,
    tax: &str,
    kind: Option<khata_core::DeducteeKind>,
    remark: Option<char>,
    certificate: Option<String>,
) -> file::Row {
    let date = parse_date(date).unwrap_or(rules::NEW_ACT_FROM);
    let amount = parse_amount(amount).unwrap_or_default();
    let tax = parse_amount(tax).unwrap_or_default();
    let rate = if amount.is_zero() { Decimal::ZERO } else { (tax * Decimal::ONE_HUNDRED / amount).round_dp(4) };
    file::Row {
        pan,
        name: name.to_owned(),
        code: code.to_owned(),
        date,
        amount,
        tax,
        deducted_on: (!tax.is_zero()).then_some(date),
        rate,
        remark,
        certificate,
        kind,
        opting_out: false,
        non_resident: false,
        permanent_establishment: false,
        nr: None,
    }
}

/// Save a statement text file (`.txt` only).
pub fn write_statement_file(path: &str, contents: &str) -> ApiResult<()> {
    let p = std::path::Path::new(path);
    if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
        return Err("save the statement as a .txt file".into());
    }
    std::fs::write(p, contents).map_err(|e| format!("cannot write {path}: {e}"))
}

// ============================================================ salary

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SalaryEstimateInput {
    /// The tax year by its first year (2026); 0: the year of today.
    pub tax_year: i32,
    pub salary: String,
    pub other_salary: String,
    pub other_income: String,
    pub tds_so_far: String,
    /// 0: the months left from today, this month included.
    pub months_remaining: u32,
    pub non_resident: bool,
}

/// The salary TDS estimate (new regime). Needs no open company.
pub fn salary_estimate(input: &SalaryEstimateInput) -> ApiResult<khata_in::salary::SalaryEstimate> {
    let today = crate::api::today();
    let fy = khata_compliance::FinancialYear::containing(today, khata_in::FY_START_MONTH);
    let tax_year = if input.tax_year == 0 { fy.start.year() } else { input.tax_year };
    let months = if input.months_remaining != 0 {
        input.months_remaining
    } else {
        let m = today.month();
        if m >= 4 { 12 - (m - 4) } else { 4 - m }
    };
    let num = |s: &str| if s.trim().is_empty() { Ok(Amount::ZERO) } else { parse_amount(s) };
    khata_in::salary::estimate(&khata_in::salary::SalaryInput {
        tax_year,
        salary: num(&input.salary)?,
        other_salary: num(&input.other_salary)?,
        other_income: num(&input.other_income)?,
        tds_so_far: num(&input.tds_so_far)?,
        months_remaining: months,
        resident: !input.non_resident,
    })
    .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher};
    use crate::api::{create_party_ledger, post_voucher};
    use crate::masters::{LedgerInput, alter_ledger, get_ledger};
    use khata_core::{DeducteeKind, DrCr};
    use rust_decimal_macros::dec;

    fn deductible(nature: Option<&str>, deductee: Option<DeducteeKind>) -> Option<LedgerTds> {
        Some(LedgerTds::Deductible(TdsTerms { nature: nature.map(str::to_owned), deductee, ..TdsTerms::default() }))
    }

    fn payable(code: &str) -> Option<LedgerTds> {
        Some(LedgerTds::Payable { payable_for: code.into() })
    }

    struct Books {
        slot: Slot,
        contractor: String,
        no_pan: String,
        labour: String,
        fees: String,
        tds_contract: String,
        tds_all: String,
        bank: String,
    }

    fn books(name: &str) -> (crate::api::tests::TempFile, Books) {
        let (f, mut slot) = indian_company(name);
        let creditors = group_id(&slot, "Sundry Creditors");
        let contractor = create_party_ledger(&mut slot, "Mehta Builders", &creditors, "", None, None, None).unwrap().ledger.id;
        let no_pan = ledger(&mut slot, "Ramesh Plumber", "Sundry Creditors", "");
        let labour = ledger(&mut slot, "Contract Labour", "Indirect Expenses", "");
        let fees = ledger(&mut slot, "Audit Fees", "Indirect Expenses", "");
        let tds_contract = ledger(&mut slot, "TDS Payable - Contractors", "Duties & Taxes", "");
        let tds_all = ledger(&mut slot, "TDS Payable", "Duties & Taxes", "");
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "");
        set_ledger_tds(&mut slot, &contractor, Some("aapfu0939f".into()), deductible(None, Some(DeducteeKind::Firm))).unwrap();
        set_ledger_tds(&mut slot, &labour, None, deductible(Some("Contract"), None)).unwrap();
        set_ledger_tds(&mut slot, &fees, None, deductible(Some("professional"), None)).unwrap();
        set_ledger_tds(&mut slot, &tds_contract, None, payable("contract")).unwrap();
        set_ledger_tds(&mut slot, &tds_all, None, payable("ALL")).unwrap();
        (f, Books { slot, contractor, no_pan, labour, fees, tds_contract, tds_all, bank })
    }

    #[test]
    fn natures_list_for_pickers() {
        let n = tds_natures();
        assert_eq!(n.len(), NATURES.len());
        let c = n.iter().find(|n| n.code == "contract").unwrap();
        assert_eq!((c.old_section.as_str(), c.rate_individual_huf.as_str(), c.rate_others.as_str()), ("194C", "1", "2"));
        assert!(c.threshold.contains("30,000") && c.threshold.contains("1,00,000"), "{}", c.threshold);
    }

    #[test]
    fn ledger_settings_are_checked() {
        let (_f, b) = books("tds-settings");
        let mut slot = b.slot;
        let l = get_ledger(&slot, &b.contractor).unwrap();
        assert_eq!((l.pan.as_deref(), &l.tds), (Some("AAPFU0939F"), &deductible(None, Some(DeducteeKind::Firm))));
        assert_eq!(get_ledger(&slot, &b.labour).unwrap().tds, deductible(Some("contract"), None), "codes are normalised");
        assert_eq!(get_ledger(&slot, &b.tds_all).unwrap().tds, payable("all"));
        assert!(set_ledger_tds(&mut slot, &b.no_pan, Some("AAPXU0939F".into()), None).unwrap_err().contains("4th character"));
        assert!(set_ledger_tds(&mut slot, &b.labour, None, deductible(Some("salary"), None)).unwrap_err().contains("not a TDS nature"));
        assert!(set_ledger_tds(&mut slot, &b.labour, None, payable("contract")).unwrap_err().contains("Duties & Taxes"));
        assert!(set_ledger_tds(&mut slot, &b.tds_all, None, deductible(Some("contract"), None)).unwrap_err().contains("hold TDS payable"));
        // A PAN must match the GSTIN.
        let creditors = group_id(&slot, "Sundry Creditors");
        let reg = create_party_ledger(&mut slot, "KA Supplier", &creditors, "", None, Some("29AAGCB7383J1Z4"), None).unwrap().ledger.id;
        assert!(set_ledger_tds(&mut slot, &reg, Some("AAPFU0939F".into()), None).unwrap_err().contains("not the one in the GSTIN"));
        set_ledger_tds(&mut slot, &reg, Some("AAGCB7383J".into()), None).unwrap();

        // Through the ledger form: absent keeps, empty / null clear.
        let mut input = LedgerInput {
            name: "Mehta Builders".into(),
            group_id: creditors,
            opening: String::new(),
            credit_days: None,
            tax_id: None,
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: None,
            tax_class: None,
            pan: None,
            tds: None,
        };
        let kept = alter_ledger(&mut slot, &b.contractor, &input).unwrap().ledger;
        assert_eq!((kept.pan.as_deref(), kept.tds.is_some()), (Some("AAPFU0939F"), true));
        input.pan = Some(String::new());
        input.tds = Some(None);
        let cleared = alter_ledger(&mut slot, &b.contractor, &input).unwrap().ledger;
        assert_eq!((cleared.pan, cleared.tds), (None, None));
        let json: LedgerInput = serde_json::from_value(serde_json::json!({
            "name": "x", "group_id": "y", "pan": "AAPFU0939F", "tds": { "kind": "Deductible", "deductee": "Company", "lower_rate": "1.5" }
        }))
        .unwrap();
        assert_eq!(json.tds, Some(Some(LedgerTds::Deductible(TdsTerms { deductee: Some(DeducteeKind::Company), lower_rate: Some(dec!(1.5)), ..TdsTerms::default() }))));
        let absent: LedgerInput = serde_json::from_value(serde_json::json!({ "name": "x", "group_id": "y", "tds": null })).unwrap();
        assert_eq!(absent.tds, Some(None));
        let absent: LedgerInput = serde_json::from_value(serde_json::json!({ "name": "x", "group_id": "y" })).unwrap();
        assert_eq!(absent.tds, None);
    }

    #[test]
    fn suggestion_follows_the_year_to_date() {
        let (_f, b) = books("tds-for");
        let mut slot = b.slot;
        // First bill under both limits: nothing.
        let s = tds_for(&slot, "2026-05-10", &b.contractor, &b.labour, "25000", None).unwrap();
        assert!(!s.applies, "{s:?}");
        assert_eq!((s.nature.as_deref(), s.deductee), (Some("contract"), Some(DeducteeKind::Firm)));
        post_voucher(&mut slot, voucher("Journal", "2026-05-10", vec![
            entry(&b.labour, DrCr::Dr, "25000"),
            entry(&b.contractor, DrCr::Cr, "25000"),
        ]))
        .unwrap();
        // Over the single limit: 2% for a firm, credited to the contractors' ledger.
        let s = tds_for(&slot, "2026-06-10", &b.contractor, &b.labour, "40000", None).unwrap();
        assert_eq!((s.amount.as_str(), s.base.as_str(), s.threshold), ("800", "40000", Some(rules::ThresholdHit::Single)));
        assert_eq!((s.ytd_amount.as_str(), s.payable_ledger_id.as_deref()), ("25000", Some(b.tds_contract.as_str())));
        assert_eq!(s.section.as_deref(), Some("s.393(1) (ex-194C)"));
        let second = post_voucher(&mut slot, voucher("Journal", "2026-06-10", vec![
            entry(&b.labour, DrCr::Dr, "40000"),
            entry(&b.contractor, DrCr::Cr, "39200"),
            entry(&b.tds_contract, DrCr::Cr, "800"),
        ]))
        .unwrap();
        // Crossing 1 lakh: TDS on the year's 1,05,000 less 800 already deducted.
        let s = tds_for(&slot, "2026-07-01", &b.contractor, &b.labour, "40000", None).unwrap();
        assert_eq!((s.amount.as_str(), s.base.as_str(), s.threshold), ("1300", "105000", Some(rules::ThresholdHit::AggregateCrossed)));
        assert_eq!(s.ytd_tds, "800");
        // Altering the second voucher leaves it out of the totals.
        let s = tds_for(&slot, "2026-07-01", &b.contractor, &b.labour, "40000", Some(&second.id)).unwrap();
        assert_eq!((s.ytd_amount.as_str(), s.amount.as_str()), ("25000", "800"));
        // Next year starts afresh.
        let s = tds_for(&slot, "2027-04-02", &b.contractor, &b.labour, "25000", None).unwrap();
        assert!(!s.applies && s.ytd_amount == "0");
        // No PAN: 20%; the nature comes from the expense ledger; TDS goes to the all-natures ledger.
        let s = tds_for(&slot, "2026-06-10", &b.no_pan, &b.fees, "60000", None).unwrap();
        assert_eq!((s.amount.as_str(), s.rate.as_deref(), s.rate_basis), ("12000", Some("20"), Some(rules::RateBasis::NoPan)));
        assert_eq!(s.payable_ledger_id.as_deref(), Some(b.tds_all.as_str()));
        // A lower-deduction certificate on the party.
        let cert = Some(LedgerTds::Deductible(TdsTerms {
            deductee: Some(DeducteeKind::Firm),
            lower_rate: Some(dec!(0.5)),
            certificate: Some("LDC9".into()),
            valid_from: NaiveDate::from_ymd_opt(2026, 4, 1),
            valid_to: NaiveDate::from_ymd_opt(2027, 12, 31),
            ..TdsTerms::default()
        }));
        set_ledger_tds(&mut slot, &b.contractor, Some("AAPFU0939F".into()), cert).unwrap();
        let s = tds_for(&slot, "2026-08-01", &b.contractor, &b.labour, "40000", None).unwrap();
        assert_eq!((s.amount.as_str(), s.rate_basis), ("0", Some(rules::RateBasis::LowerCertificate)), "0.5% of 1,05,000 is less than the 800 already deducted");
        let s = tds_for(&slot, "2027-04-10", &b.contractor, &b.labour, "40000", None).unwrap();
        assert_eq!((s.amount.as_str(), s.rate_basis), ("200", Some(rules::RateBasis::LowerCertificate)));
        // No nature anywhere: nothing suggested.
        let s = tds_for(&slot, "2026-08-01", &b.contractor, &b.bank, "40000", None).unwrap();
        assert!(!s.applies && s.explanation.contains("No nature of payment"), "{}", s.explanation);
        assert!(tds_for(&slot, "2025-03-01", &b.contractor, &b.labour, "40000", None).unwrap_err().contains("before 1 Apr 2025"));
    }

    #[test]
    fn payable_report_and_quarterly_statement() {
        let (_f, b) = books("tds-payable");
        let mut slot = b.slot;
        let post = |slot: &mut Slot, kind: &str, date: &str, e| post_voucher(slot, voucher(kind, date, e)).unwrap();
        post(&mut slot, "Journal", "2026-05-10", vec![
            entry(&b.labour, DrCr::Dr, "40000"),
            entry(&b.contractor, DrCr::Cr, "39200"),
            entry(&b.tds_contract, DrCr::Cr, "800"),
        ]);
        post(&mut slot, "Journal", "2026-05-20", vec![
            entry(&b.fees, DrCr::Dr, "60000"),
            entry(&b.no_pan, DrCr::Cr, "48000"),
            entry(&b.tds_all, DrCr::Cr, "12000"),
        ]);
        // Below the limits: counts as paid, no TDS.
        post(&mut slot, "Journal", "2026-06-01", vec![entry(&b.labour, DrCr::Dr, "5000"), entry(&b.contractor, DrCr::Cr, "5000")]);
        // Challans in June.
        post(&mut slot, "Payment", "2026-06-05", vec![entry(&b.tds_contract, DrCr::Dr, "800"), entry(&b.bank, DrCr::Cr, "800")]);
        post(&mut slot, "Payment", "2026-06-06", vec![entry(&b.tds_all, DrCr::Dr, "10000"), entry(&b.bank, DrCr::Cr, "10000")]);
        // Next quarter.
        post(&mut slot, "Journal", "2026-07-15", vec![
            entry(&b.labour, DrCr::Dr, "35000"),
            entry(&b.contractor, DrCr::Cr, "34300"),
            entry(&b.tds_contract, DrCr::Cr, "700"),
        ]);

        let r = tds_payable(&slot, "2026-04-01", "2026-06-30").unwrap();
        let natures: Vec<_> = r.natures.iter().map(|n| (n.nature.as_str(), n.paid_credited.as_str(), n.deducted.as_str(), n.deposited.as_str())).collect();
        assert_eq!(natures, [("contract", "45000", "800", "800"), ("professional", "60000", "12000", "0"), ("all", "0", "0", "10000")]);
        let contract = &r.natures[0];
        assert_eq!(contract.outstanding.as_deref(), Some("0"));
        assert_eq!(contract.by_month, [MonthDue { month: "2026-05".into(), deducted: "800".into(), due: "2026-06-07".into() }]);
        assert_eq!(r.natures[1].outstanding, None, "held in the all-natures ledger");
        let all = r.ledgers.iter().find(|l| l.payable_for == "all").unwrap();
        assert_eq!((all.opening.as_str(), all.closing.as_str()), ("0", "2000"));
        assert_eq!((r.deducted.as_str(), r.deposited.as_str(), r.rows.len(), r.challans.len()), ("12800", "10800", 3, 2));
        let row = r.rows.iter().find(|x| x.nature == "professional").unwrap();
        assert_eq!((row.party_name.as_str(), row.pan.as_deref(), row.section.as_str()), ("Ramesh Plumber", None, "s.393(1) (ex-194J)"));

        let q = tds_quarterly(&slot, 2026, 1).unwrap();
        assert_eq!((q.form.as_str(), q.due.as_deref(), q.year.as_str()), ("Form 140", Some("2026-07-31"), "2026-27"));
        let rows: Vec<_> = q.rows.iter().map(|r| (r.party_name.as_str(), r.section.as_str(), r.amount.as_str(), r.tds.as_str(), r.rate.as_str())).collect();
        assert_eq!(rows, [("Mehta Builders", "194C", "40000", "800", "2.00"), ("Ramesh Plumber", "194J", "60000", "12000", "20.00")]);
        assert_eq!(q.rows[1].reason.as_deref(), Some("No PAN: higher rate"));
        assert_eq!((q.tds.as_str(), q.deposited.as_str()), ("12800", "10800"));
        assert!(q.warnings.iter().any(|w| w.contains("Ramesh Plumber: no PAN")), "{:?}", q.warnings);
        assert!(q.gaps.iter().any(|g| g.contains("FVU")));
        let csv = tds_quarterly_csv(&slot, 2026, 1).unwrap();
        assert!(csv.starts_with("\"Form 140 data, Q1 2026-27 (not an FVU file)\"\r\n"), "{csv}");
        assert!(csv.contains("1,Mehta Builders,AAPFU0939F,Firm,194C,Payments to contractors and sub-contractors,2026-05-10,40000,800,2.00,,Journal,"), "{csv}");
        assert!(csv.contains("Total,,,,,,,100000,12800"), "{csv}");
        assert_eq!(tds_quarterly(&slot, 2026, 2).unwrap().rows.len(), 1);
        assert!(tds_quarterly(&slot, 2026, 5).is_err());
    }

    #[test]
    fn only_for_indian_companies() {
        let (_f, mut slot) = crate::api::tests::uae_company("tds-uae");
        let id = ledger(&mut slot, "Supplier", "Sundry Creditors", "");
        assert_eq!(set_ledger_tds(&mut slot, &id, Some("AAPFU0939F".into()), None).unwrap_err(), NOT_INDIA);
        assert_eq!(tds_payable(&slot, "2026-04-01", "2026-06-30").unwrap_err(), NOT_INDIA);
        set_ledger_tds(&mut slot, &id, None, None).unwrap();
    }

    fn collectible(nature: Option<&str>) -> Option<LedgerTcs> {
        Some(LedgerTcs::Collectible(TcsTerms { nature: nature.map(str::to_owned), ..TcsTerms::default() }))
    }

    #[test]
    fn tcs_settings_suggestion_and_reports() {
        let (_f, mut slot) = indian_company("tcs");
        let debtors = group_id(&slot, "Sundry Debtors");
        let kabadi = create_party_ledger(&mut slot, "Kabadi Traders", &debtors, "", None, None, None).unwrap().ledger.id;
        let factory = ledger(&mut slot, "Steel Works Ltd", "Sundry Debtors", "");
        let scrap = ledger(&mut slot, "Scrap Sales", "Sales Accounts", "");
        let tcs_ledger = ledger(&mut slot, "TCS Payable", "Duties & Taxes", "");
        let cgst = ledger(&mut slot, "Output CGST", "Duties & Taxes", "");
        let sgst = ledger(&mut slot, "Output SGST", "Duties & Taxes", "");
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "");
        crate::tax::set_ledger_tax(&mut slot, &cgst, "CGST", khata_core::TaxSide::Output).unwrap();
        crate::tax::set_ledger_tax(&mut slot, &sgst, "SGST", khata_core::TaxSide::Output).unwrap();
        set_ledger_tds(&mut slot, &kabadi, Some("AAPFU0939F".into()), None).unwrap();
        set_ledger_tds(&mut slot, &factory, Some("AAGCB7383J".into()), None).unwrap();
        assert_eq!(set_ledger_tcs(&mut slot, &scrap, collectible(Some(" Scrap "))).unwrap(), collectible(Some("scrap")));
        set_ledger_tcs(&mut slot, &tcs_ledger, Some(LedgerTcs::Payable { payable_for: "ALL".into() })).unwrap();
        let declared = Some(LedgerTcs::Collectible(TcsTerms { declaration: true, ..TcsTerms::default() }));
        set_ledger_tcs(&mut slot, &factory, declared.clone()).unwrap();
        assert_eq!(ledger_tcs(&slot, &factory).unwrap(), declared);
        // Shape checks.
        assert!(set_ledger_tcs(&mut slot, &scrap, Some(LedgerTcs::Payable { payable_for: "scrap".into() })).unwrap_err().contains("Duties & Taxes"));
        assert!(set_ledger_tcs(&mut slot, &scrap, collectible(Some("goods"))).unwrap_err().contains("not a TCS nature"));
        assert!(set_ledger_tcs(&mut slot, &tcs_ledger, collectible(Some("scrap"))).unwrap_err().contains("hold TCS payable"));
        assert_eq!(tcs_natures().iter().find(|n| n.code == "scrap").map(|n| (n.rate.as_str(), n.no_pan_rate.as_str(), n.tax_code.as_str())), Some(("2", "5", "1073")));

        // Suggestion: 2% of what the buyer pays (GST included).
        let s = tcs_for(&slot, "2026-05-10", &kabadi, &scrap, "118000").unwrap();
        assert_eq!((s.applies, s.amount.as_str(), s.tax_code.as_deref(), s.payable_ledger_id.as_deref()), (true, "2360", Some("1073"), Some(tcs_ledger.as_str())));
        let s = tcs_for(&slot, "2026-05-10", &factory, &scrap, "50000").unwrap();
        assert_eq!((s.applies, s.basis), (false, Some(tcs_rules::TcsBasis::Declaration)));
        assert!(tcs_for(&slot, "2026-03-31", &kabadi, &scrap, "1000").unwrap_err().contains("before 1 Apr 2026"));
        assert!(!tcs_for(&slot, "2026-05-10", &kabadi, &bank, "1000").unwrap().applies);

        let post = |slot: &mut Slot, kind: &str, date: &str, e| post_voucher(slot, voucher(kind, date, e)).unwrap();
        post(&mut slot, "Sales", "2026-05-10", vec![
            entry(&kabadi, DrCr::Dr, "120360"),
            entry(&scrap, DrCr::Cr, "100000"),
            entry(&cgst, DrCr::Cr, "9000"),
            entry(&sgst, DrCr::Cr, "9000"),
            entry(&tcs_ledger, DrCr::Cr, "2360"),
        ]);
        post(&mut slot, "Sales", "2026-06-02", vec![entry(&factory, DrCr::Dr, "50000"), entry(&scrap, DrCr::Cr, "50000")]);
        post(&mut slot, "Payment", "2026-06-05", vec![entry(&tcs_ledger, DrCr::Dr, "2360"), entry(&bank, DrCr::Cr, "2360")]);

        let r = tcs_payable(&slot, "2026-04-01", "2026-06-30").unwrap();
        let n: Vec<_> = r.natures.iter().map(|n| (n.nature.as_str(), n.paid_credited.as_str(), n.deducted.as_str(), n.deposited.as_str())).collect();
        assert_eq!(n, [("scrap", "168000", "2360", "0"), ("all", "0", "0", "2360")]);
        assert_eq!(r.natures[0].by_month[0], MonthDue { month: "2026-05".into(), deducted: "2360".into(), due: "2026-06-07".into() });
        assert_eq!((r.ledgers[0].closing.as_str(), r.rows[0].section.as_str()), ("0", "s.394(1) [Table: Sl. No. 4] (ex-206C(1))"));
        // The TDS reports do not pick up sales.
        assert!(tds_payable(&slot, "2026-04-01", "2026-06-30").unwrap().rows.is_empty());

        let q = tcs_quarterly(&slot, 2026, 1).unwrap();
        assert_eq!((q.form.as_str(), q.due.as_deref(), q.tcs.as_str()), ("Form 143", Some("2026-07-31"), "2360"));
        let rows: Vec<_> = q.rows.iter().map(|r| (r.party_name.as_str(), r.tax_code.as_str(), r.amount.as_str(), r.tcs.as_str(), r.rate.as_str(), r.remark.as_deref())).collect();
        assert_eq!(rows, [("Kabadi Traders", "1073", "118000", "2360", "2.00", None), ("Steel Works Ltd", "1073", "50000", "0", "0.00", Some("B"))]);
        assert_eq!(q.rows[0].collectee, Some(DeducteeKind::Firm), "from the PAN");
    }

    fn deductor_settings() -> DeductorSettings {
        DeductorSettings {
            tan: "muma12345b".into(),
            deductor_type: "K".into(),
            pin: "400069".into(),
            email: "accounts@demo.example".into(),
            phone: "9820012345".into(),
            responsible_name: "Ravi Mehta".into(),
            responsible_designation: "Director".into(),
            responsible_pan: "ABCPM1234K".into(),
            ..DeductorSettings::default()
        }
    }

    #[test]
    fn deductor_settings_challans_and_the_form_140_file() {
        let (_f, b) = books("tds-file");
        let mut slot = b.slot;
        crate::tax::set_company_tax(&mut slot, "27AAGCB7383J1Z4", "27").unwrap_or_else(|_| {
            crate::tax::set_company_tax(&mut slot, "29AAGCB7383J1Z4", "29").unwrap()
        });
        crate::tax::set_company_address(&mut slot, "12 Market Road, Andheri East, Mumbai").unwrap();
        let post = |slot: &mut Slot, kind: &str, date: &str, e| post_voucher(slot, voucher(kind, date, e)).unwrap();
        post(&mut slot, "Journal", "2026-05-10", vec![
            entry(&b.labour, DrCr::Dr, "40000"),
            entry(&b.contractor, DrCr::Cr, "39200"),
            entry(&b.tds_contract, DrCr::Cr, "800"),
        ]);
        let challan = post(&mut slot, "Payment", "2026-06-05", vec![entry(&b.tds_contract, DrCr::Dr, "800"), entry(&b.bank, DrCr::Cr, "800")]);

        // Nothing set up: the file lists what is missing.
        let d = tds_deductor(&slot).unwrap();
        assert!(d.problems.iter().any(|p| p.contains("TAN")), "{:?}", d.problems);
        let f = tds_statement_file(&mut slot, "140", 2026, 1, None).unwrap();
        assert!(f.text.is_none());
        assert!(f.problems.iter().any(|p| p.contains("BSR code")), "{:?}", f.problems);

        assert!(set_tds_deductor(&mut slot, DeductorSettings { tan: "MUM12345B".into(), ..deductor_settings() }).unwrap_err().contains("TAN"));
        let d = set_tds_deductor(&mut slot, deductor_settings()).unwrap();
        assert!(d.problems.is_empty(), "{:?}", d.problems);
        assert_eq!((d.stored.tan.as_str(), d.effective.state.as_str(), d.effective.pan.as_str()), ("MUMA12345B", file::state_from_gst(&d.effective.gstin.clone().unwrap()[..2]).unwrap(), "AAGCB7383J"));
        assert_eq!(d.effective.address, ["12 Market Road", "Andheri East, Mumbai"]);
        assert_eq!(d.responsible.address, d.effective.address, "the responsible person defaults to the deductor's address");

        assert!(set_tds_challan(&mut slot, &challan.id, Some(ChallanDetails { bsr: "123".into(), serial: "1".into(), ..ChallanDetails::default() })).unwrap_err().contains("7 digits"));
        let details = ChallanDetails { bsr: "0510308".into(), serial: "00042".into(), date: "2026-06-06".into(), ..ChallanDetails::default() };
        assert_eq!(set_tds_challan(&mut slot, &challan.id, Some(details.clone())).unwrap().unwrap().minor_head, "200");
        assert_eq!(tds_challan(&slot, &challan.id).unwrap().map(|c| c.bsr), Some("0510308".into()));

        let f = tds_statement_file(&mut slot, "140", 2026, 1, None).unwrap();
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        let text = f.text.unwrap();
        let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("1^FH^NS1^R^") && lines[0].contains("^MUMA12345B^1^KhataOS^"), "{}", lines[0]);
        assert!(lines[2].starts_with("3^CD^1^1^1^N^^800.00^0.00^0.00^0.00^800.00^C^^0510308^^00042^^06062026^"), "{}", lines[2]);
        assert!(lines[3].starts_with("4^DD^1^1^1^O^^AAPFU0939F^Mehta Builders^^^^^^1024^^^Y^10052026^40000.00^"), "{}", lines[3]);
        assert_eq!(f.file_name, "140RQ1-2026-27.txt");
        // The sequence number moves on with each file.
        let again = tds_statement_file(&mut slot, "140", 2026, 1, None).unwrap().text.unwrap();
        assert!(again.starts_with("1^FH^NS1^R^") && again.split('^').nth(5) == Some("2"));
        // A challan without its details stops the file.
        set_tds_challan(&mut slot, &challan.id, None).unwrap();
        assert!(tds_statement_file(&mut slot, "140", 2026, 1, None).unwrap().text.is_none());
        assert!(tds_statement_file(&mut slot, "26Q", 2026, 1, None).is_err());
        assert!(write_statement_file("/tmp/x.csv", "").unwrap_err().contains(".txt"));
    }

    #[test]
    fn non_residents_get_the_users_rate_and_form_144() {
        let (_f, b) = books("tds-nr");
        let mut slot = b.slot;
        let creditors = group_id(&slot, "Sundry Creditors");
        let acme = create_party_ledger(&mut slot, "Acme Inc", &creditors, "", None, None, None).unwrap().ledger.id;
        let consult = ledger(&mut slot, "Foreign Consultancy", "Indirect Expenses", "");
        let tds_nr = ledger(&mut slot, "TDS Payable - Non-residents", "Duties & Taxes", "");
        set_ledger_tds(&mut slot, &consult, None, deductible(Some("non_resident"), None)).unwrap();
        set_ledger_tds(&mut slot, &tds_nr, None, payable("non_resident")).unwrap();
        // No rate yet: the suggestion asks for one.
        set_ledger_tds(&mut slot, &acme, None, deductible(None, Some(DeducteeKind::Company))).unwrap();
        let s = tds_for(&slot, "2026-05-15", &acme, &consult, "100000", None).unwrap();
        assert!(!s.applies && s.explanation.contains("Enter the rate") && s.warnings.iter().any(|w| w.contains("Form 145")), "{s:?}");
        let nr = |rate: Decimal, country: &str| {
            Some(LedgerTds::Deductible(TdsTerms {
                deductee: Some(DeducteeKind::Company),
                non_resident: Some(NonResidentTerms { rate, treaty: true, country: Some(country.into()), remittance: Some("21".into()), foreign_tin: Some(" 12-3456789 ".into()) }),
                ..TdsTerms::default()
            }))
        };
        assert!(set_ledger_tds(&mut slot, &acme, None, nr(dec!(10), "999")).unwrap_err().contains("country code"));
        set_ledger_tds(&mut slot, &acme, Some("AAGCB7383J".into()), nr(dec!(10), "267")).unwrap();
        let s = tds_for(&slot, "2026-05-15", &acme, &consult, "100000", None).unwrap();
        assert_eq!((s.amount.as_str(), s.rate.as_deref(), s.section.as_deref()), ("10000", Some("10"), Some("s.393(2) (ex-195)")));
        assert!(s.warnings.iter().any(|w| w.contains("Form 146")));

        let post = |slot: &mut Slot, kind: &str, date: &str, e| post_voucher(slot, voucher(kind, date, e)).unwrap();
        post(&mut slot, "Journal", "2026-05-15", vec![entry(&consult, DrCr::Dr, "100000"), entry(&acme, DrCr::Cr, "90000"), entry(&tds_nr, DrCr::Cr, "10000")]);
        let challan = post(&mut slot, "Payment", "2026-06-07", vec![entry(&tds_nr, DrCr::Dr, "10000"), entry(&b.bank, DrCr::Cr, "10000")]);
        let q = tds_quarterly_nr(&slot, 2026, 1).unwrap();
        assert_eq!(q.form, "Form 144");
        let r = &q.rows[0];
        assert_eq!((r.tax_code.as_str(), r.rate.as_str(), r.treaty, r.country_name.as_deref(), r.foreign_tin.as_deref()), ("1057", "10.00", true, Some("UNITED STATES OF AMERICA"), Some("12-3456789")));
        assert_eq!(q.challans.len(), 1);
        assert!(tds_quarterly(&slot, 2026, 1).unwrap().rows.is_empty(), "not on Form 140");

        // No GSTIN: the deductor's PAN is entered.
        let d = set_tds_deductor(&mut slot, DeductorSettings { address: "1 Park Street, Kolkata".into(), state: "19".into(), ..deductor_settings() }).unwrap();
        assert!(d.problems.iter().any(|p| p.contains("deductor PAN")), "{:?}", d.problems);
        let d = set_tds_deductor(&mut slot, DeductorSettings { pan: "aapfu0939f".into(), address: "1 Park Street, Kolkata".into(), state: "19".into(), ..deductor_settings() }).unwrap();
        assert_eq!((d.effective.pan.as_str(), d.effective.state.as_str()), ("AAPFU0939F", "32"));
        set_tds_challan(&mut slot, &challan.id, Some(ChallanDetails { bsr: "0510308".into(), serial: "7".into(), ..ChallanDetails::default() })).unwrap();
        // Below 20% the payee's foreign address, email and phone are needed.
        let f = tds_statement_file(&mut slot, "144", 2026, 1, None).unwrap();
        assert!(f.problems.iter().any(|p| p.contains("address in the country of residence")), "{:?}", f.problems);
        let mut input = crate::masters::LedgerInput {
            name: "Acme Inc".into(),
            group_id: creditors,
            opening: String::new(),
            credit_days: None,
            tax_id: None,
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: Some(khata_core::Mailing { address: "1 Main St, Austin TX".into(), country: Some("US".into()), phone: "+1 512 555 0100".into(), email: "ap@acme.example".into(), ..Default::default() }),
            tax_class: None,
            pan: None,
            tds: None,
        };
        crate::masters::alter_ledger(&mut slot, &acme, &input).unwrap();
        let f = tds_statement_file(&mut slot, "144", 2026, 1, None).unwrap();
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        let dd = f.text.unwrap().split("\r\n").nth(3).unwrap().to_owned();
        assert!(dd.starts_with("4^DD^1^1^1^O^^AAGCB7383J^Acme Inc^^^1^^N^1057^21^267^Y^15052026^100000.00^N^"), "{dd}");
        assert!(dd.contains("^10.0000^^B^") && dd.contains("^1 Main St, Austin TX^ap@acme.example^15125550100^^12-3456789^"), "{dd}");
        input.mailing = None;
        let _ = input;
    }

    #[test]
    fn salary_estimate_command() {
        let e = salary_estimate(&SalaryEstimateInput { tax_year: 2026, salary: "2075000".into(), months_remaining: 12, ..SalaryEstimateInput::default() }).unwrap();
        assert_eq!((e.annual_tax, e.monthly), (dec!(208_000), dec!(17_333)));
        assert!(salary_estimate(&SalaryEstimateInput { tax_year: 2025, salary: "1".into(), months_remaining: 1, ..SalaryEstimateInput::default() }).unwrap_err().contains("2025-26"));
        let defaulted = salary_estimate(&SalaryEstimateInput { tax_year: 2026, salary: "100".into(), ..SalaryEstimateInput::default() }).unwrap();
        assert!((1..=12).contains(&defaulted.months_remaining));
    }
}

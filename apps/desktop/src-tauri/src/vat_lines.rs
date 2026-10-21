//! A voucher split into return lines, shared by the UK and Singapore
//! returns (`vat_gb.rs`, `gst_sg.rs`; `vat_ae.rs` keeps its own copy of the
//! same rules). The party line carries the total; item lines (goods) and
//! other ledgers on the opposite side (services) are the value (a ledger
//! named "Round off" excepted); ledgers whose type of tax is the country's
//! component give the tax.
//!
//! **Mechanisms without core changes.** A line's class may carry a prefix
//! ("RC:STANDARD", "IMPORT:STANDARD", "DRC:STANDARD"): the country's
//! [`Rules::classify`] splits it into a mechanism and a rate class. Lines
//! whose mechanism is self-accounted (reverse charge) carry the tax worked
//! out at their rate class: the supplier charged none. A tax ledger may
//! carry a class too ("IMPORT", "IMPORT:PVA"): its postings are import tax
//! paid at customs or postponed, reported from the ledger
//! ([`Split::ledger_tax`]) and kept out of the voucher's own tax.

use chrono::NaiveDate;
use khata_core::{Book, DrCr, TaxSide, Voucher, VoucherKind};
use rust_decimal::{Decimal, RoundingStrategy};

/// What a country's return needs to know about classes and treatments.
pub(crate) trait Rules {
    type Treatment: Copy + PartialEq;
    type Mechanism: Copy + PartialEq + Default;
    /// "VAT", "GST".
    const COMPONENT: &'static str;
    /// The mechanism of a class, and the treatment and rate of its rate
    /// class on a date when a rate is on file.
    fn classify(&self, class: Option<&str>, date: NaiveDate) -> (Self::Mechanism, Option<(Self::Treatment, Decimal)>);
    /// The treatment of a line with no class or rate on file.
    fn standard(&self) -> Self::Treatment;
    /// Zero-rated: what an untaxed export becomes.
    fn zero_rated(&self) -> Self::Treatment;
    /// The treatment carries tax (standard, reduced).
    fn taxed(&self, t: Self::Treatment) -> bool;
    /// The business works out and accounts for this mechanism's tax itself
    /// (reverse charge) on a purchase.
    fn self_accounted(&self, m: Self::Mechanism) -> bool;
    /// The mechanism a tax ledger's class marks (import tax), if any.
    fn ledger_mechanism(&self, class: &str) -> Option<Self::Mechanism>;
}

/// One line of a voucher.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line<T, M> {
    /// Signed: negative when on the party's side.
    pub value: Decimal,
    /// A stock item rather than a ledger.
    pub goods: bool,
    /// Posted to a ledger under Fixed Assets (capital expenditure).
    pub capital: bool,
    pub mechanism: M,
    pub treatment: T,
    /// The tax: the voucher's own tax spread by value, or the self-accounted
    /// tax at the rate class.
    pub tax: Decimal,
}

/// A posting to a tax ledger whose class marks a mechanism.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LedgerTax<M> {
    pub mechanism: M,
    pub side: TaxSide,
    /// Debits less credits.
    pub debit: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Split<T, M> {
    pub lines: Vec<Line<T, M>>,
    pub ledger_tax: Vec<LedgerTax<M>>,
}

fn label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Sales",
        VoucherKind::Purchase => "Purchase",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        VoucherKind::Journal => "Journal",
        VoucherKind::Payment => "Payment",
        VoucherKind::Receipt => "Receipt",
        _ => "Voucher",
    }
}

/// The voucher's name in warnings: "Purchase 7".
pub(crate) fn voucher_name(v: &Voucher) -> String {
    format!("{} {}", label(v.kind), v.number)
}

fn is_capital(book: &Book, ledger: khata_core::LedgerId) -> bool {
    let fixed = khata_core::GroupId::default_named("Fixed Assets");
    crate::api::ancestors(book, book.ledger(ledger).map(|l| l.group)).any(|g| g.id == fixed)
}

/// A line and the treatment and rate of its class, when known.
type Raw<R> = (Line<<R as Rules>::Treatment, <R as Rules>::Mechanism>, Option<(<R as Rules>::Treatment, Decimal)>);

/// Postings to tax ledgers whose class marks a mechanism, on any voucher.
pub(crate) fn ledger_tax<R: Rules>(rules: &R, book: &Book, v: &Voucher) -> Vec<LedgerTax<R::Mechanism>> {
    let mut out = Vec::new();
    for e in &v.entries {
        let Some(l) = book.ledger(e.ledger) else { continue };
        let Some(t) = l.tax.as_ref().filter(|t| t.component.eq_ignore_ascii_case(R::COMPONENT)) else { continue };
        if let Some(m) = l.tax_class.as_deref().and_then(|c| rules.ledger_mechanism(c)) {
            out.push(LedgerTax { mechanism: m, side: t.side, debit: if e.side == DrCr::Dr { e.amount } else { -e.amount } });
        }
    }
    out
}

/// Split a Sales, Purchase, Credit Note or Debit Note voucher. `inward`:
/// a purchase or purchase return (self-accounted tax is worked out only
/// there). `export`: a sale to a party abroad; with no tax, its taxed
/// normal lines become zero-rated.
pub(crate) fn split<R: Rules>(
    rules: &R,
    book: &Book,
    v: &Voucher,
    party: Option<usize>,
    inward: bool,
    export: bool,
    warnings: &mut Vec<String>,
) -> Split<R::Treatment, R::Mechanism> {
    let name = voucher_name(v);
    let party_side = party.map(|i| v.entries[i].side);
    let normal = R::Mechanism::default();
    // (line, known treatment and rate)
    let mut raw: Vec<Raw<R>> = Vec::new();
    let mut tax_total = Decimal::ZERO;
    // Output tax a purchase posts (credits less debits): the self-accounted reverse charge.
    let mut self_output = Decimal::ZERO;
    let ledger_tax = ledger_tax(rules, book, v);
    let mut push = |value: Decimal, goods: bool, capital: bool, class: Option<&str>| {
        let (mechanism, known) = rules.classify(class, v.date);
        let treatment = known.map_or(rules.standard(), |(t, _)| t);
        raw.push((Line { value, goods, capital, mechanism, treatment, tax: Decimal::ZERO }, known));
    };
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.as_str()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if !t.component.eq_ignore_ascii_case(R::COMPONENT) {
                warnings.push(format!(
                    "{name}: {ledger_name} collects {}, which is not {}; it is left out",
                    t.component,
                    R::COMPONENT
                ));
            } else if ledger.and_then(|l| l.tax_class.as_deref()).and_then(|c| rules.ledger_mechanism(c)).is_none() {
                tax_total += signed;
                if t.side == TaxSide::Output {
                    self_output += if e.side == DrCr::Cr { e.amount } else { -e.amount };
                }
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let class = book.stock_item(l.item).and_then(|i| i.tax_class.clone());
                push(if on_party_side { -l.amount } else { l.amount }, true, false, class.as_deref());
            }
        } else if ledger_name.to_lowercase().contains("round") {
            // Round-off is part of the invoice value, not of the supply.
        } else if on_party_side {
            warnings.push(format!("{name}: {ledger_name} is on the party's side; it is not deducted from the value"));
        } else {
            let class = ledger.and_then(|l| l.tax_class.clone());
            push(signed, false, is_capital(book, e.ledger), class.as_deref());
        }
    }
    if raw.is_empty() {
        if !tax_total.is_zero() {
            warnings.push(format!("{name} has {} but no lines to put it on; it is left out", R::COMPONENT));
        }
        return Split { lines: Vec::new(), ledger_tax };
    }

    // Self-accounted lines: tax at the rate class.
    let mut self_tax = Decimal::ZERO;
    for (l, known) in raw.iter_mut() {
        if l.mechanism == normal || !rules.self_accounted(l.mechanism) {
            continue;
        }
        match known {
            Some((t, r)) if inward && rules.taxed(*t) => {
                l.tax = (l.value * *r / Decimal::ONE_HUNDRED).round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
                self_tax += l.tax;
            }
            Some(_) => {}
            None => warnings.push(format!(
                "{name}: a reverse-charge line has no rate on file for its class; its {} is taken as 0",
                R::COMPONENT
            )),
        }
    }
    if inward && (self_tax - self_output).abs() > Decimal::ONE {
        warnings.push(format!(
            "{name}: the reverse charge worked out is {} {}, but the voucher posts {} to output {} (book Dr input, Cr output for it)",
            R::COMPONENT,
            self_tax.normalize(),
            self_output.normalize(),
            R::COMPONENT
        ));
    }

    // The voucher's own tax goes on its taxed lines that are not self-accounted.
    let carries = |l: &Line<R::Treatment, R::Mechanism>| l.mechanism == normal || !rules.self_accounted(l.mechanism);
    if !tax_total.is_zero() && !raw.iter().any(|(l, _)| carries(l) && rules.taxed(l.treatment)) {
        if raw.iter().any(|(l, _)| carries(l)) {
            warnings.push(format!(
                "{name} has {} but only zero-rated, exempt or out-of-scope lines; it is reported as standard-rated",
                R::COMPONENT
            ));
            for (l, _) in raw.iter_mut().filter(|(l, _)| carries(l)) {
                l.treatment = rules.standard();
            }
        } else {
            warnings.push(format!("{name} posts {} {} beyond its reverse charge; it is left out", R::COMPONENT, tax_total.normalize()));
            return Split { lines: raw.into_iter().map(|(l, _)| l).collect(), ledger_tax };
        }
    }
    if export && tax_total.is_zero() {
        for (l, _) in raw.iter_mut().filter(|(l, _)| l.mechanism == normal && rules.taxed(l.treatment)) {
            l.treatment = rules.zero_rated();
        }
        return Split { lines: raw.into_iter().map(|(l, _)| l).collect(), ledger_tax };
    }
    let taxed: Vec<usize> = (0..raw.len()).filter(|&i| carries(&raw[i].0) && rules.taxed(raw[i].0.treatment)).collect();
    if tax_total.is_zero() && raw.iter().any(|(l, k)| l.mechanism == normal && k.is_none() && rules.taxed(l.treatment) && !l.value.is_zero()) {
        warnings.push(format!(
            "{name} has no {} and lines without a tax class or rate; they are reported as standard-rated (give them a ZERO or EXEMPT class if they are not)",
            R::COMPONENT
        ));
    }
    // Check the tax against the rates on file when every taxed line has one.
    if !taxed.is_empty() && taxed.iter().all(|&i| raw[i].1.is_some()) {
        let expected: Decimal =
            taxed.iter().map(|&i| raw[i].0.value * raw[i].1.map_or(Decimal::ZERO, |(_, r)| r)).sum::<Decimal>() / Decimal::ONE_HUNDRED;
        if (expected - tax_total).abs() > Decimal::ONE {
            warnings.push(format!(
                "{name}: {} booked is {}, but the rates on file give {}",
                R::COMPONENT,
                tax_total.normalize(),
                expected.round_dp(2).normalize()
            ));
        }
    }
    // Spread the tax by value; the last taxed line takes what is left.
    let weight: Decimal = taxed.iter().map(|&i| raw[i].0.value).sum();
    let last = taxed.iter().rev().copied().find(|&i| !raw[i].0.value.is_zero());
    let mut left = tax_total;
    for &i in &taxed {
        let share = match last {
            Some(k) if i == k => left,
            Some(_) if !weight.is_zero() => (tax_total * raw[i].0.value / weight).round_dp(2),
            _ => Decimal::ZERO,
        };
        left -= share;
        raw[i].0.tax = share;
    }
    if last.is_none() && !tax_total.is_zero() {
        warnings.push(format!("{name}: its lines add up to zero, so its {} is left out", R::COMPONENT));
    }
    Split { lines: raw.into_iter().map(|(l, _)| l).collect(), ledger_tax }
}

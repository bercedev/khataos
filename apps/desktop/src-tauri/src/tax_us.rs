//! United States: the sales tax liability report and the Form 1099 vendor
//! list from the books. The rules live in `khata_us`; this module turns
//! vouchers into its neutral documents.
//!
//! **Sales tax report.** Sales → invoice, Credit Note → credit note. The
//! jurisdiction is the customer's: a party whose mailing country is not
//! the US is abroad (an export); otherwise its address region (`CA` or a
//! company-defined `CA-LA`), else the company's own state. Lines split as
//! in `vat_gb.rs`: items and service ledgers on the opposite side are the
//! lines; ledgers whose type of tax is `STATE`, `COUNTY`, `CITY` or
//! `DISTRICT` give the tax collected, spread over the taxable lines by
//! value. A line's taxability comes from its tax class and the rate keys
//! on file for the voucher date (`khata_us::sales_tax::rate_components`).
//!
//! **Invoices.** `tax.rs invoice_tax` needs the company's region (its
//! state) because the US pack has regions; the party's region (its
//! mailing region) is the place of supply, so a customer in `CA-LA` gets
//! the `CA` state rate plus the `CA-LA/…` local components.
//!
//! **Exemption certificates.** A customer's resale and exemption
//! certificates (state, number, type, dates) are a shared record of the
//! company under `us.exempt.<ledger id>` (JSON): outside the books, synced
//! with co-workers and the accountant like them (`docs/sync-settings.md`),
//! until the books have a certificate master. In the sales tax report, a sale whose customer
//! has a certificate in force for the sale's state on its date counts as
//! exempt ([`khata_us::exemptions::covering`]).
//!
//! **Use tax.** Purchase → purchase, Debit Note → purchase return. The
//! vendor's state is its address region (state part), its country when
//! not the US. Stock-item lines are bought for resale; other lines (an
//! expense or asset ledger) take that ledger's tax class at the company's
//! own jurisdiction's rates. A purchase with any sales tax component
//! booked on it is taken as taxed by the vendor.
//!
//! **Form 1099.** Payment vouchers: every debit to a party under Sundry
//! Creditors is a payment to that payee (1099-NEC nonemployee
//! compensation; KhataOS cannot tell rents or royalties apart yet). The
//! payee's TIN is its ledger's tax ID.

use khata_compliance::RateSchedule;
use khata_core::{Book, DrCr, GroupId, LedgerId, UserRole, Voucher, VoucherKind};
use khata_store::Store;
use khata_us::exemptions::{self, Certificate, CertificateReport, CustomerCertificates};
use khata_us::form1099::{self, Form1099Report, Payment, PaymentKind};
use khata_us::returns::{self, DocKind, Period, SaleDoc, SaleLine, SalesTaxReport};
use khata_us::sales_tax::{self, Taxability};
use khata_us::use_tax::{self, PurchaseDoc, PurchaseLine, UseTaxReport};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::api::{ApiResult, Slot, ancestors, err, group_is_party, open_ref, parse_date, parse_uuid};

const CERT_PREFIX: &str = "us.exempt.";

/// The tax components a US duty ledger can collect.
pub const COMPONENTS: [&str; 4] = [sales_tax::STATE, sales_tax::COUNTY, sales_tax::CITY, sales_tax::DISTRICT];

fn us_only(book: &Book) -> ApiResult<()> {
    if book.country() != Some(khata_us::COUNTRY.code) {
        return Err("the US sales tax report and 1099 list are for companies in the United States".into());
    }
    Ok(())
}

fn rates(book: &Book) -> ApiResult<RateSchedule> {
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

fn label(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Sales",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::Payment => "Payment",
        _ => "Voucher",
    }
}

/// The party line: a debtor/creditor, else the cash or bank line on the
/// party's side, else the first line on that side.
fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = if matches!(v.kind, VoucherKind::Sales | VoucherKind::DebitNote) { DrCr::Dr } else { DrCr::Cr };
    let is_party = |e: &khata_core::Entry| book.ledger(e.ledger).is_some_and(|l| group_is_party(book, l.group));
    v.entries
        .iter()
        .position(is_party)
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

/// One voucher's lines (positive for the sale), with the tax collected.
fn sale_lines(
    book: &Book,
    v: &Voucher,
    party: Option<usize>,
    juris: Option<&sales_tax::Jurisdiction>,
    rates: &RateSchedule,
    warnings: &mut Vec<String>,
) -> Vec<SaleLine> {
    let name = format!("{} {}", label(v.kind), v.number);
    let party_side = party.map(|i| v.entries[i].side);
    let mut raw: Vec<(Decimal, Option<String>)> = Vec::new();
    let mut tax = Decimal::ZERO;
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let signed = if on_party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.as_str()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if COMPONENTS.iter().any(|c| t.component.eq_ignore_ascii_case(c)) {
                tax += signed;
            } else {
                warnings.push(format!("{name}: {ledger_name} collects {}, which is not a sales tax component; it is left out", t.component));
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let class = book.stock_item(l.item).and_then(|i| i.tax_class.clone());
                raw.push((if on_party_side { -l.amount } else { l.amount }, class));
            }
        } else if ledger_name.to_lowercase().contains("round") {
            // Round-off is not part of the sale.
        } else if on_party_side {
            warnings.push(format!("{name}: {ledger_name} is on the party's side; it is not deducted from the sales"));
        } else {
            raw.push((signed, ledger.and_then(|l| l.tax_class.clone())));
        }
    }
    let mut out: Vec<SaleLine> = raw
        .iter()
        .map(|(value, class)| {
            let taxability = match juris {
                None => Taxability::Exempt,
                Some(j) => sales_tax::rate_components(j, class.as_deref().unwrap_or_default(), v.date, rates).0,
            };
            SaleLine { taxability, value: *value, tax: Decimal::ZERO }
        })
        .collect();
    if tax.is_zero() {
        return out;
    }
    let taxable: Vec<usize> = (0..out.len()).filter(|&i| out[i].taxability == Taxability::Taxable).collect();
    let weight: Decimal = taxable.iter().map(|&i| out[i].value).sum();
    if taxable.is_empty() || weight.is_zero() {
        warnings.push(format!("{name} has sales tax {} but no taxable lines on file; it is reported as tax collected", tax.normalize()));
        out.push(SaleLine { taxability: Taxability::Taxable, value: Decimal::ZERO, tax });
        return out;
    }
    let mut left = tax;
    for (n, &i) in taxable.iter().enumerate() {
        let share = if n + 1 == taxable.len() { left } else { (tax * out[i].value / weight).round_dp(2) };
        left -= share;
        out[i].tax = share;
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesTaxReportUsDto {
    /// The company's EIN as stored.
    pub ein: Option<String>,
    /// The company's state.
    pub state: Option<String>,
    #[serde(flatten)]
    pub report: SalesTaxReport,
}

/// The sales tax liability report for a period, by jurisdiction and state.
pub fn sales_tax_report_us(slot: &Slot, from: &str, to: &str) -> ApiResult<SalesTaxReportUsDto> {
    let book = open_ref(slot)?;
    us_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let rates = rates(book)?;
    let pack: &dyn khata_compliance::CompliancePack = &khata_us::UnitedStates;
    let home = profile.tax_region.clone();
    let mut warnings = Vec::new();
    let mut docs = Vec::new();
    let year_start = chrono::NaiveDate::from_ymd_opt(chrono::Datelike::year(&period.to), 1, 1).expect("1 January");
    let first = period.from.min(year_start);
    let store = store_ref(slot)?;
    let mut certs_of: std::collections::HashMap<LedgerId, Vec<Certificate>> = std::collections::HashMap::new();
    let mut vouchers: Vec<&Voucher> = book
        .vouchers()
        .filter(|v| matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote) && first <= v.date && v.date <= period.to)
        .collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let party = party_line(book, v);
        let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
        let country = ledger.and_then(|l| l.mailing.as_ref()).and_then(|m| m.country.as_deref()).filter(|c| !c.trim().is_empty());
        let abroad = country.is_some_and(|c| !c.trim().eq_ignore_ascii_case("US"));
        let code = if abroad { None } else { ledger.and_then(|l| crate::tax::party_region(pack, l)).or_else(|| home.clone()) };
        let mut voucher_warnings = Vec::new();
        let juris = match code.as_deref().map(sales_tax::parse_jurisdiction) {
            Some(Ok(j)) => Some(j),
            Some(Err(e)) => {
                voucher_warnings.push(format!("{} {}: {e}", label(v.kind), v.number));
                None
            }
            None if abroad => None,
            None => {
                voucher_warnings.push(format!(
                    "{} {}: the customer has no state and the company's state is not set; it is left out",
                    label(v.kind),
                    v.number
                ));
                continue;
            }
        };
        let lines = sale_lines(book, v, party, juris.as_ref(), &rates, &mut voucher_warnings);
        if period.contains(v.date) {
            warnings.extend(voucher_warnings);
        }
        let certificate = match (ledger, juris.as_ref()) {
            (Some(l), Some(j)) if !abroad => {
                let certs = match certs_of.entry(l.id) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => e.insert(read_certificates(store, l.id)?),
                };
                exemptions::covering(certs, &j.state, v.date).map(|c| c.number.clone())
            }
            _ => None,
        };
        docs.push(SaleDoc {
            id: v.id.0.to_string(),
            kind: if v.kind == VoucherKind::Sales { DocKind::Invoice } else { DocKind::CreditNote },
            number: v.number.clone(),
            date: v.date,
            party_name: ledger.map(|l| l.name.clone()).unwrap_or_default(),
            jurisdiction: if abroad { None } else { juris.map(|j| j.code()) },
            cancelled: v.cancelled,
            lines,
            certificate,
        });
    }
    let mut report = returns::sales_tax_report(&docs, period);
    report.warnings.splice(0..0, warnings);
    Ok(SalesTaxReportUsDto { ein: profile.tax_id.clone(), state: home, report })
}

fn store_ref(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

// ------------------------------------------------------------ certificates

fn cert_key(ledger: LedgerId) -> String {
    format!("{CERT_PREFIX}{}", ledger.0)
}

fn read_certificates(store: &Store, ledger: LedgerId) -> ApiResult<Vec<Certificate>> {
    match store.record(&cert_key(ledger)).map_err(err)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("the stored certificates are damaged: {e}")),
        None => Ok(Vec::new()),
    }
}

fn customer(book: &Book, ledger_id: &str) -> ApiResult<LedgerId> {
    let id = LedgerId(parse_uuid(ledger_id, "ledger")?);
    let l = book.ledger(id).ok_or("unknown ledger")?;
    if !group_is_party(book, l.group) {
        return Err(format!("{} is not a party ledger", l.name));
    }
    Ok(id)
}

/// A customer's resale and exemption certificates.
pub fn us_certificates(slot: &Slot, ledger_id: &str) -> ApiResult<Vec<Certificate>> {
    let book = open_ref(slot)?;
    us_only(book)?;
    read_certificates(store_ref(slot)?, customer(book, ledger_id)?)
}

/// Replace a customer's certificates (an empty list removes them).
pub fn set_us_certificates(slot: &mut Slot, ledger_id: &str, certificates: &[Certificate]) -> ApiResult<Vec<Certificate>> {
    let book = open_ref(slot)?;
    us_only(book)?;
    crate::api::require(book, UserRole::Accountant)?;
    // A company shared as Viewer is read-only here too.
    crate::lan::check_writable(store_ref(slot)?)?;
    let id = customer(book, ledger_id)?;
    let clean = exemptions::validate(certificates)?;
    let json = (!clean.is_empty()).then(|| serde_json::to_string(&clean)).transpose().map_err(err)?;
    store_ref(slot)?.set_record(&cert_key(id), json.as_deref(), &crate::api::os_user()).map_err(err)?;
    Ok(clean)
}

/// The certificate that exempts a sale to `ledger_id` in `jurisdiction`
/// ("CA" or "CA-LA") on `date`, for the invoice screen (see
/// `docs/us-sales-tax.md`, wiring).
pub fn exempting_certificate(slot: &Slot, ledger_id: &str, jurisdiction: &str, date: chrono::NaiveDate) -> ApiResult<Option<Certificate>> {
    let book = open_ref(slot)?;
    us_only(book)?;
    let juris = sales_tax::parse_jurisdiction(jurisdiction).map_err(err)?;
    let certs = read_certificates(store_ref(slot)?, LedgerId(parse_uuid(ledger_id, "ledger")?))?;
    Ok(exemptions::covering(&certs, &juris.state, date).cloned())
}

/// Every certificate on file, expired and expiring first.
pub fn certificate_report_us(slot: &Slot, as_of: &str, within_days: i64) -> ApiResult<CertificateReport> {
    let book = open_ref(slot)?;
    us_only(book)?;
    if !(0..=3650).contains(&within_days) {
        return Err("the warning window is 0 to 3650 days".into());
    }
    let as_of = parse_date(as_of)?;
    let mut customers = Vec::new();
    for (key, json) in store_ref(slot)?.records(CERT_PREFIX).map_err(err)? {
        let Some(id) = key.strip_prefix(CERT_PREFIX) else { continue };
        let Ok(uuid) = parse_uuid(id, "ledger") else { continue };
        let Some(l) = book.ledger(LedgerId(uuid)) else { continue };
        let certificates: Vec<Certificate> = serde_json::from_str(&json).map_err(|e| format!("the stored certificates of {} are damaged: {e}", l.name))?;
        customers.push(CustomerCertificates { customer_id: id.to_owned(), customer_name: l.name.clone(), certificates });
    }
    Ok(exemptions::report(&customers, as_of, within_days))
}

// ------------------------------------------------------------ use tax

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UseTaxReportUsDto {
    pub ein: Option<String>,
    pub state: Option<String>,
    #[serde(flatten)]
    pub report: UseTaxReport,
}

/// One purchase's lines: (lines, sales tax booked).
fn purchase_lines(book: &Book, v: &Voucher, party: Option<usize>, warnings: &mut Vec<String>) -> (Vec<PurchaseLine>, Decimal) {
    let name = format!("{} {}", if v.kind == VoucherKind::Purchase { "Purchase" } else { "Debit Note" }, v.number);
    let party_side = party.map(|i| v.entries[i].side);
    let mut lines = Vec::new();
    let mut tax = Decimal::ZERO;
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        let on_party_side = Some(e.side) == party_side;
        let ledger = book.ledger(e.ledger);
        let ledger_name = ledger.map(|l| l.name.as_str()).unwrap_or_default();
        if let Some(t) = ledger.and_then(|l| l.tax.as_ref()) {
            if COMPONENTS.iter().any(|c| t.component.eq_ignore_ascii_case(c)) {
                tax += if on_party_side { -e.amount } else { e.amount };
            }
            continue;
        }
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                lines.push(PurchaseLine {
                    description: item.map(|i| i.name.clone()).unwrap_or_default(),
                    class: item.and_then(|i| i.tax_class.clone()),
                    value: if on_party_side { -l.amount } else { l.amount },
                    for_resale: true,
                });
            }
        } else if ledger_name.to_lowercase().contains("round") {
        } else if on_party_side {
            warnings.push(format!("{name}: {ledger_name} is on the vendor's side; it is not deducted"));
        } else {
            lines.push(PurchaseLine {
                description: ledger_name.to_owned(),
                class: ledger.and_then(|l| l.tax_class.clone()),
                value: e.amount,
                for_resale: false,
            });
        }
    }
    (lines, tax)
}

const USE_JURISDICTION_KEY: &str = "us.use_tax.jurisdiction";

/// Use tax to self-assess on untaxed purchases from vendors outside the
/// company's state, at the rates of the jurisdiction where the goods are
/// used: `jurisdiction` ("CA-LA", in the company's state; remembered in
/// a shared record of the company), else the last one given, else the
/// company's state alone.
pub fn use_tax_report_us(slot: &Slot, from: &str, to: &str, jurisdiction: Option<&str>) -> ApiResult<UseTaxReportUsDto> {
    let book = open_ref(slot)?;
    us_only(book)?;
    let profile = book.profile().ok_or("pick the company's country first")?;
    let period = Period { from: parse_date(from)?, to: parse_date(to)? };
    if period.from > period.to {
        return Err("the period ends before it starts".into());
    }
    let state = profile.tax_region.clone().ok_or("set the company's state first (Company → Tax registration)")?;
    let store = store_ref(slot)?;
    let given = jurisdiction.map(str::trim).filter(|j| !j.is_empty());
    let home_code = match given {
        Some(j) => j.to_ascii_uppercase(),
        None => store.record(USE_JURISDICTION_KEY).map_err(err)?.unwrap_or_else(|| state.clone()),
    };
    let home = sales_tax::parse_jurisdiction(&home_code).map_err(err)?;
    if !home.state.eq_ignore_ascii_case(&state) {
        return Err(format!("{home_code} is not in the company's state {state}"));
    }
    // Remembered for the company, unless this copy is read-only (a viewer's).
    if given.is_some() && crate::lan::check_writable(store).is_ok() && store.record(USE_JURISDICTION_KEY).map_err(err)? != Some(home.code()) {
        store.set_record(USE_JURISDICTION_KEY, Some(&home.code()), &crate::api::os_user()).map_err(err)?;
    }
    let home_code = home.code();
    let rates = rates(book)?;
    let mut warnings = Vec::new();
    let mut docs = Vec::new();
    let mut vouchers: Vec<&Voucher> =
        book.vouchers().filter(|v| matches!(v.kind, VoucherKind::Purchase | VoucherKind::DebitNote) && period.contains(v.date)).collect();
    vouchers.sort_by(|a, b| (a.date, &a.number).cmp(&(b.date, &b.number)));
    for v in vouchers {
        let party = party_line(book, v);
        let ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
        let mailing = ledger.and_then(|l| l.mailing.as_ref());
        let country = mailing.and_then(|m| m.country.as_deref()).map(str::trim).filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("US"));
        let state = mailing
            .and_then(|m| m.region.as_deref())
            .filter(|r| !r.trim().is_empty())
            .and_then(|r| sales_tax::parse_jurisdiction(r).ok())
            .map(|j| j.state);
        let (lines, tax_charged) = purchase_lines(book, v, party, &mut warnings);
        docs.push(PurchaseDoc {
            id: v.id.0.to_string(),
            kind: if v.kind == VoucherKind::Purchase { DocKind::Invoice } else { DocKind::CreditNote },
            number: v.number.clone(),
            date: v.date,
            vendor_name: ledger.map(|l| l.name.clone()).unwrap_or_default(),
            vendor_state: if country.is_some() { None } else { state },
            vendor_country: country.map(|c| c.to_ascii_uppercase()),
            cancelled: v.cancelled,
            tax_charged,
            lines,
        });
    }
    let mut report = use_tax::use_tax_report(&docs, &home, &rates, period);
    report.warnings.splice(0..0, warnings);
    Ok(UseTaxReportUsDto { ein: profile.tax_id.clone(), state: Some(home_code), report })
}

fn is_creditor(book: &Book, group: GroupId) -> bool {
    let creditors = GroupId::default_named("Sundry Creditors");
    ancestors(book, Some(group)).any(|g| g.id == creditors)
}

/// The Form 1099 vendor list for a calendar year.
pub fn form1099_us(slot: &Slot, year: i32) -> ApiResult<Form1099Report> {
    let book = open_ref(slot)?;
    us_only(book)?;
    if !(2000..=2100).contains(&year) {
        return Err(format!("{year} is not a year to report"));
    }
    let mut payments = Vec::new();
    for v in book.vouchers().filter(|v| v.kind == VoucherKind::Payment && chrono::Datelike::year(&v.date) == year) {
        for e in v.entries.iter().filter(|e| e.side == DrCr::Dr) {
            let Some(l) = book.ledger(e.ledger).filter(|l| is_creditor(book, l.group)) else { continue };
            payments.push(Payment {
                party_id: l.id.0.to_string(),
                party_name: l.name.clone(),
                tin: l.tax_id.clone(),
                date: v.date,
                kind: PaymentKind::Nonemployee,
                amount: e.amount,
                cancelled: v.cancelled,
            });
        }
    }
    let mut r = form1099::form1099_report(&payments, year);
    for row in &r.rows {
        if row.tin.as_deref().is_some_and(khata_us::ein::unassigned_prefix) {
            r.warnings.push(format!("{}: EIN {} has a prefix the IRS no longer assigns; check it", row.party_name, row.tin.as_deref().unwrap_or_default()));
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, ledger, voucher};
    use crate::api::{StockLineInput, cancel_voucher, check_tax_id, create_company, create_party_ledger, post_voucher, set_country};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{InvoiceTaxInput, InvoiceTaxLine, invoice_tax, set_company_tax, set_ledger_tax, set_tax_rate};
    use khata_core::TaxSide;
    use rust_decimal_macros::dec;

    fn us_company(name: &str) -> (TempFile, Slot) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Inc").unwrap();
        set_country(&mut slot, Some(&f.0), "US").unwrap();
        (f, slot)
    }

    fn set_region(slot: &mut Slot, id: &str, name: &str, group: &str, region: &str, country: &str) {
        let input: crate::masters::LedgerInput = serde_json::from_value(serde_json::json!({
            "name": name,
            "group_id": group_id(slot, group),
            "mailing": {
                "region": (!region.is_empty()).then_some(region),
                "country": (!country.is_empty()).then_some(country),
            },
        }))
        .unwrap();
        crate::masters::alter_ledger(slot, id, &input).unwrap();
    }

    fn rates(slot: &mut Slot) {
        for (class, pct) in [("CA", "7.25"), ("CA-LA/COUNTY", "0.25"), ("CA-LA/CITY", "0.75"), ("CA-LA/DISTRICT", "1.25"), ("CA:FOOD", "0")] {
            set_tax_rate(slot, class, "2020-01-01", pct).unwrap();
        }
    }

    #[test]
    fn a_us_company_gets_per_jurisdiction_components_on_invoices() {
        let (_f, mut slot) = us_company("us-invoice");
        let ok = check_tax_id(&slot, "123456789").unwrap();
        assert_eq!((ok.value.as_str(), ok.error), ("12-3456789", None));
        assert!(set_company_tax(&mut slot, "12-3456789", "ZZ").is_err());
        let tax = set_company_tax(&mut slot, "12-3456789", "CA").unwrap();
        assert_eq!((tax.label.as_str(), tax.region_name.as_deref(), tax.regions.len()), ("EIN", Some("California"), 51));
        rates(&mut slot);
        for c in COMPONENTS {
            let id = ledger(&mut slot, &format!("Sales Tax {c}"), "Duties & Taxes", "");
            set_ledger_tax(&mut slot, &id, c, TaxSide::Output).unwrap();
        }
        let debtors = group_id(&slot, "Sundry Debtors");
        let la = create_party_ledger(&mut slot, "LA Store", &debtors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &la, "LA Store", "Sundry Debtors", "CA-LA", "US");
        let ny = create_party_ledger(&mut slot, "NY Store", &debtors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &ny, "NY Store", "Sundry Debtors", "NY", "");
        let ask = |party: &str, class: &str| InvoiceTaxInput {
            kind: VoucherKind::Sales,
            date: "2026-05-01".into(),
            party_id: party.into(),
            lines: vec![InvoiceTaxLine { item_id: String::new(), ledger_id: String::new(), tax_class: class.into(), taxable_value: "100".into() }],
        };
        let t = invoice_tax(&slot, &ask(&la, "TAXABLE")).unwrap();
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.place_of_supply.as_deref(), Some("CA-LA"));
        let rows: Vec<_> = t.rows.iter().map(|r| (r.component.as_str(), r.amount.as_str())).collect();
        assert_eq!(rows, [("CITY", "0.75"), ("COUNTY", "0.25"), ("DISTRICT", "1.25"), ("STATE", "7.25")]);
        assert!(invoice_tax(&slot, &ask(&la, "FOOD")).unwrap().rows.is_empty(), "exempt in California");
        assert!(invoice_tax(&slot, &ask(&ny, "TAXABLE")).unwrap().rows.is_empty(), "no New York rate: not collected");
    }

    #[test]
    fn sales_tax_report_and_1099_list_from_the_books() {
        let (_f, mut slot) = us_company("us-report");
        set_company_tax(&mut slot, "12-3456789", "CA").unwrap();
        rates(&mut slot);
        let output = ledger(&mut slot, "Sales Tax Payable", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "STATE", TaxSide::Output).unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let creditors = group_id(&slot, "Sundry Creditors");
        let la = create_party_ledger(&mut slot, "LA Store", &debtors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &la, "LA Store", "Sundry Debtors", "CA-LA", "");
        let walk_in = create_party_ledger(&mut slot, "Walk-in", &debtors, "", None, None, None).unwrap().ledger.id;
        let ny = create_party_ledger(&mut slot, "NY Store", &debtors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &ny, "NY Store", "Sundry Debtors", "NY", "");
        let toronto = create_party_ledger(&mut slot, "Toronto Ltd", &debtors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &toronto, "Toronto Ltd", "Sundry Debtors", "", "CA");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let item = |slot: &mut Slot, name: &str, class: &str| {
            let input: StockItemInput = serde_json::from_value(serde_json::json!({ "name": name, "unit_id": nos, "tax_class": class })).unwrap();
            save_stock_item(slot, &input).unwrap().id
        };
        let laptop = item(&mut slot, "Laptop", "TAXABLE");
        let bread = item(&mut slot, "Bread", "FOOD");
        let line = |amount: &str, item_id: &str| {
            let mut e = entry(&sales, DrCr::Cr, amount);
            e.items = vec![StockLineInput {
                item_id: item_id.into(),
                qty: "1".into(),
                rate: amount.into(),
                amount: String::new(),
                godown_id: String::new(),
                batch: String::new(),
            }];
            e
        };
        let post = |slot: &mut Slot, kind: &str, date: &str, entries| post_voucher(slot, voucher(kind, date, entries)).unwrap();
        post(&mut slot, "Sales", "2026-04-02", vec![entry(&la, DrCr::Dr, "1095"), line("1000", &laptop), entry(&output, DrCr::Cr, "95")]);
        post(&mut slot, "Sales", "2026-04-03", vec![entry(&walk_in, DrCr::Dr, "200"), line("200", &bread)]);
        post(&mut slot, "Sales", "2026-04-04", vec![entry(&ny, DrCr::Dr, "500"), line("500", &laptop)]);
        post(&mut slot, "Sales", "2026-04-05", vec![entry(&toronto, DrCr::Dr, "300"), line("300", &laptop)]);
        post(&mut slot, "CreditNote", "2026-04-09", vec![
            entry(&la, DrCr::Cr, "109.50"),
            { let mut e = line("100", &laptop); e.side = DrCr::Dr; e },
            entry(&output, DrCr::Dr, "9.50"),
        ]);
        let gone = post(&mut slot, "Sales", "2026-04-10", vec![entry(&la, DrCr::Dr, "1095"), line("1000", &laptop), entry(&output, DrCr::Cr, "95")]);
        cancel_voucher(&mut slot, &gone.id).unwrap();

        let r = sales_tax_report_us(&slot, "2026-04-01", "2026-06-30").unwrap();
        assert_eq!((r.ein.as_deref(), r.state.as_deref()), (Some("12-3456789"), Some("CA")));
        let j: Vec<_> = r.report.jurisdictions.iter().map(|j| (j.jurisdiction.as_str(), j.totals.gross_sales, j.totals.exempt_sales, j.totals.taxable_sales, j.totals.tax_collected)).collect();
        assert_eq!(j, [
            ("CA", dec!(200), dec!(200), dec!(0), dec!(0)),
            ("CA-LA", dec!(900), dec!(0), dec!(900), dec!(85.5)),
            ("NY", dec!(500), dec!(0), dec!(0), dec!(0)),
        ]);
        assert_eq!(r.report.export_sales, dec!(300));
        assert_eq!(r.report.states[0].nexus.as_ref().map(|n| n.met), Some(false));
        assert!(r.report.warnings.iter().any(|w| w.starts_with("New York")), "{:?}", r.report.warnings);
        assert!(sales_tax_report_us(&slot, "2026-05-01", "2026-04-01").is_err());

        // Payments to vendors.
        let bank = ledger(&mut slot, "Bank", "Bank Accounts", "");
        let designer = create_party_ledger(&mut slot, "Jane Designer", &creditors, "", None, Some("123-45-6789"), None).unwrap().ledger.id;
        let printer = create_party_ledger(&mut slot, "Print Co", &creditors, "", None, None, None).unwrap().ledger.id;
        post(&mut slot, "Payment", "2026-02-01", vec![entry(&designer, DrCr::Dr, "1500"), entry(&bank, DrCr::Cr, "1500")]);
        post(&mut slot, "Payment", "2026-03-01", vec![entry(&designer, DrCr::Dr, "600"), entry(&bank, DrCr::Cr, "600")]);
        post(&mut slot, "Payment", "2026-03-02", vec![entry(&printer, DrCr::Dr, "700"), entry(&bank, DrCr::Cr, "700")]);
        post(&mut slot, "Payment", "2025-03-02", vec![entry(&printer, DrCr::Dr, "700"), entry(&bank, DrCr::Cr, "700")]);
        let f = form1099_us(&slot, 2026).unwrap();
        let rows: Vec<_> = f.rows.iter().map(|r| (r.party_name.as_str(), r.total, r.tin.as_deref())).collect();
        assert_eq!(rows, [("Jane Designer", dec!(2100), Some("123-45-6789"))]);
        let f25 = form1099_us(&slot, 2025).unwrap();
        assert_eq!((f25.rows.len(), f25.threshold), (1, dec!(600)));
        assert!(f25.warnings.iter().any(|w| w.contains("Print Co: no TIN")));
    }

    #[test]
    fn certificates_exempt_sales_and_expire() {
        let (_f, mut slot) = us_company("us-certs");
        set_company_tax(&mut slot, "12-3456789", "CA").unwrap();
        rates(&mut slot);
        let output = ledger(&mut slot, "Sales Tax Payable", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "STATE", TaxSide::Output).unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let reseller = create_party_ledger(&mut slot, "Reseller Inc", &debtors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &reseller, "Reseller Inc", "Sundry Debtors", "CA-LA", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let cert = |state: &str, number: &str, issued: &str, expires: Option<&str>| Certificate {
            state: state.into(),
            number: number.into(),
            kind: exemptions::CertificateKind::Resale,
            issued: Some(parse_date(issued).unwrap()),
            expires: expires.map(|e| parse_date(e).unwrap()),
            note: String::new(),
        };
        assert!(set_us_certificates(&mut slot, &sales, &[]).is_err(), "only party ledgers");
        assert!(set_us_certificates(&mut slot, &reseller, &[cert("ZZ", "1", "2026-01-01", None)]).is_err());
        let saved = set_us_certificates(&mut slot, &reseller, &[cert("ca", "SR-AA-123", "2026-04-01", Some("2026-04-30")), cert("NV", "NV-9", "2026-01-01", Some("2026-07-10"))]).unwrap();
        assert_eq!(saved[0].state, "CA");
        assert_eq!(us_certificates(&slot, &reseller).unwrap().len(), 2);
        let post = |slot: &mut Slot, date: &str| {
            post_voucher(slot, voucher("Sales", date, vec![entry(&reseller, DrCr::Dr, "1000"), entry(&sales, DrCr::Cr, "1000")])).unwrap();
        };
        post(&mut slot, "2026-04-10"); // covered
        post(&mut slot, "2026-05-10"); // the certificate expired
        let r = sales_tax_report_us(&slot, "2026-04-01", "2026-06-30").unwrap();
        let la = &r.report.jurisdictions[0].totals;
        assert_eq!((la.gross_sales, la.certificate_exempt_sales, la.exempt_sales, la.taxable_sales), (dec!(2000), dec!(1000), dec!(1000), dec!(1000)));
        assert_eq!(
            exempting_certificate(&slot, &reseller, "CA-LA", parse_date("2026-04-15").unwrap()).unwrap().map(|c| c.number),
            Some("SR-AA-123".into())
        );
        let report = certificate_report_us(&slot, "2026-06-30", 30).unwrap();
        let rows: Vec<_> = report.rows.iter().map(|x| (x.certificate.number.as_str(), x.status, x.customer_name.as_str())).collect();
        assert_eq!(rows, [("SR-AA-123", exemptions::CertificateStatus::Expired, "Reseller Inc"), ("NV-9", exemptions::CertificateStatus::Expiring, "Reseller Inc")]);
        assert!(set_us_certificates(&mut slot, &reseller, &[]).unwrap().is_empty());
        assert!(certificate_report_us(&slot, "2026-06-30", 30).unwrap().rows.is_empty());
    }

    #[test]
    fn use_tax_on_untaxed_out_of_state_purchases() {
        let (_f, mut slot) = us_company("us-use-tax");
        set_company_tax(&mut slot, "12-3456789", "CA").unwrap();
        rates(&mut slot);
        let creditors = group_id(&slot, "Sundry Creditors");
        let oregon = create_party_ledger(&mut slot, "Portland Office Supply", &creditors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &oregon, "Portland Office Supply", "Sundry Creditors", "OR", "");
        let local = create_party_ledger(&mut slot, "LA Wholesale", &creditors, "", None, None, None).unwrap().ledger.id;
        set_region(&mut slot, &local, "LA Wholesale", "Sundry Creditors", "CA-LA", "");
        let supplies = ledger(&mut slot, "Office Supplies", "Indirect Expenses", "");
        let purchases = ledger(&mut slot, "Purchases", "Purchase Accounts", "");
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap().id;
        let input: StockItemInput = serde_json::from_value(serde_json::json!({ "name": "Widget", "unit_id": nos, "tax_class": "TAXABLE" })).unwrap();
        let widget = save_stock_item(&mut slot, &input).unwrap().id;
        let mut stock = entry(&purchases, DrCr::Dr, "300");
        stock.items = vec![StockLineInput { item_id: widget, qty: "3".into(), rate: "100".into(), amount: String::new(), godown_id: String::new(), batch: String::new() }];
        post_voucher(&mut slot, voucher("Purchase", "2026-05-02", vec![entry(&oregon, DrCr::Cr, "1300"), entry(&supplies, DrCr::Dr, "1000"), stock])).unwrap();
        post_voucher(&mut slot, voucher("Purchase", "2026-05-03", vec![entry(&local, DrCr::Cr, "200"), entry(&supplies, DrCr::Dr, "200")])).unwrap();
        assert!(use_tax_report_us(&slot, "2026-04-01", "2026-06-30", Some("NV")).is_err(), "the company's state only");
        assert_eq!(use_tax_report_us(&slot, "2026-04-01", "2026-06-30", None).unwrap().report.use_tax, dec!(72.5), "state rate only");
        use_tax_report_us(&slot, "2026-04-01", "2026-06-30", Some("ca-la")).unwrap();
        let r = use_tax_report_us(&slot, "2026-04-01", "2026-06-30", None).unwrap();
        assert_eq!(r.report.jurisdiction, "CA-LA", "remembered");
        let rows: Vec<_> = r.report.rows.iter().map(|x| (x.vendor_name.as_str(), x.vendor_location.as_str(), x.taxable_value, x.exempt_value, x.use_tax)).collect();
        assert_eq!(rows, [("Portland Office Supply", "OR", dec!(1000), dec!(300), dec!(95))], "9.5% on the supplies; the widgets are for resale");
        assert_eq!((r.report.in_state_untaxed, r.report.use_tax), (1, dec!(95)));
        assert!(use_tax_report_us(&slot, "2026-06-30", "2026-04-01", None).is_err());
    }

    #[test]
    fn us_reports_need_a_us_company() {
        let f = TempFile::new("us-only");
        let mut slot = None;
        create_company(&mut slot, &f.0, "No Country Ltd").unwrap();
        assert!(sales_tax_report_us(&slot, "2026-01-01", "2026-01-31").is_err());
        assert!(form1099_us(&slot, 2026).is_err());
        assert!(use_tax_report_us(&slot, "2026-01-01", "2026-01-31", None).is_err());
        assert!(certificate_report_us(&slot, "2026-01-01", 30).is_err());
    }
}

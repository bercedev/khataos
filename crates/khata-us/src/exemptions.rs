//! Resale and exemption certificates a customer gives the seller.
//!
//! A sale is taxable unless the seller holds proof that it is not: a
//! resale certificate (the buyer will resell the goods), or an exemption
//! certificate (an exempt organisation, a government, a direct-pay permit,
//! a manufacturing or agricultural use…). Each state has its own forms;
//! the Streamlined Sales Tax states accept the SST Certificate of
//! Exemption (form F0003), and the MTC Uniform Sales & Use Tax Certificate
//! is accepted by some others (`docs/research/us-compliance.md` §11)
//! [MEDIUM].
//!
//! What KhataOS keeps for each customer: one record per certificate with
//! the **state** it is for, its **number** (the buyer's permit or
//! exemption number), its **type**, the date it was given and its
//! **expiry**. Expiry rules differ by state: some certificates do not
//! expire while sales recur (SST blanket certificates when no more than 12
//! months pass between sales), some expire on a date (Florida's annual
//! resale certificate), some need renewing every few years
//! [MEDIUM/UNCERTAIN by state]. KhataOS does not know the rule: the user
//! enters the expiry date the state gives, or none.
//!
//! A certificate makes a sale in **its state** exempt when it is in force
//! on the sale's date ([`covering`]). Whether the certificate is
//! complete, given in good faith and for goods it covers is the seller's
//! judgement; the SST rules relieve a seller who obtains a fully completed
//! certificate within 90 days after the sale [MEDIUM].

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::states;

/// What a certificate is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CertificateKind {
    /// For resale.
    Resale,
    /// An exempt organisation (charity, religious, educational…).
    ExemptOrganization,
    /// A government purchaser.
    Government,
    /// Anything else (direct pay, manufacturing, agricultural…).
    Other,
}

impl CertificateKind {
    pub fn label(self) -> &'static str {
        match self {
            CertificateKind::Resale => "resale",
            CertificateKind::ExemptOrganization => "exempt organization",
            CertificateKind::Government => "government",
            CertificateKind::Other => "other exemption",
        }
    }
}

/// One certificate on file for a customer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Certificate {
    /// USPS state code ("CA").
    pub state: String,
    /// The buyer's permit, registration or exemption number.
    pub number: String,
    pub kind: CertificateKind,
    /// When the buyer gave it; `None` when not recorded.
    #[serde(default)]
    pub issued: Option<NaiveDate>,
    /// The last day it is good for; `None`: no expiry date.
    #[serde(default)]
    pub expires: Option<NaiveDate>,
    #[serde(default)]
    pub note: String,
}

impl Certificate {
    /// In force on a date: given by then (when recorded) and not expired.
    pub fn in_force(&self, on: NaiveDate) -> bool {
        self.issued.is_none_or(|i| i <= on) && self.expires.is_none_or(|e| on <= e)
    }
}

/// Longest certificate number kept.
pub const MAX_NUMBER_LEN: usize = 40;

/// Clean and check a certificate list (states known, numbers present).
pub fn validate(certs: &[Certificate]) -> Result<Vec<Certificate>, String> {
    let mut out = Vec::with_capacity(certs.len());
    for c in certs {
        let state = c.state.trim().to_ascii_uppercase();
        let st = states::state(&state).ok_or_else(|| format!("{:?} is not a US state code", c.state))?;
        let number = c.number.trim().to_owned();
        if number.is_empty() {
            return Err(format!("the {} certificate needs its number", st.name));
        }
        if number.chars().count() > MAX_NUMBER_LEN || number.chars().any(char::is_control) {
            return Err(format!("{number:?}: at most {MAX_NUMBER_LEN} characters"));
        }
        if let (Some(i), Some(e)) = (c.issued, c.expires)
            && e < i
        {
            return Err(format!("{number}: it expires before it was given"));
        }
        if out.iter().any(|o: &Certificate| o.state == state && o.number.eq_ignore_ascii_case(&number)) {
            return Err(format!("{number} ({state}) is listed twice"));
        }
        out.push(Certificate { state, number, kind: c.kind, issued: c.issued, expires: c.expires, note: c.note.trim().to_owned() });
    }
    Ok(out)
}

/// The certificate that makes a sale in `state` on `on` exempt, if any.
pub fn covering<'a>(certs: &'a [Certificate], state: &str, on: NaiveDate) -> Option<&'a Certificate> {
    certs.iter().filter(|c| c.state.eq_ignore_ascii_case(state) && c.in_force(on)).max_by_key(|c| c.expires.unwrap_or(NaiveDate::MAX))
}

/// A certificate's state of play on a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum CertificateStatus {
    Expired,
    /// Expires within the warning window.
    Expiring,
    Valid,
    /// No expiry date on file.
    NoExpiry,
    /// Given after the report date.
    NotYetValid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CertificateRow {
    pub customer_id: String,
    pub customer_name: String,
    #[serde(flatten)]
    pub certificate: Certificate,
    pub state_name: String,
    pub status: CertificateStatus,
    /// Days from the report date to the expiry (negative when expired).
    pub days_left: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CertificateReport {
    pub as_of: NaiveDate,
    pub within_days: i64,
    /// Expired first, then expiring soonest, then the rest.
    pub rows: Vec<CertificateRow>,
    pub expired: usize,
    pub expiring: usize,
}

/// One customer's certificates, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerCertificates {
    pub customer_id: String,
    pub customer_name: String,
    pub certificates: Vec<Certificate>,
}

/// Every certificate on file with its status on `as_of`; `within_days`
/// is the warning window for "expiring".
pub fn report(customers: &[CustomerCertificates], as_of: NaiveDate, within_days: i64) -> CertificateReport {
    let mut rows = Vec::new();
    for c in customers {
        for cert in &c.certificates {
            let days_left = cert.expires.map(|e| (e - as_of).num_days());
            let status = match (cert.issued, days_left) {
                (Some(i), _) if i > as_of => CertificateStatus::NotYetValid,
                (_, Some(d)) if d < 0 => CertificateStatus::Expired,
                (_, Some(d)) if d <= within_days => CertificateStatus::Expiring,
                (_, Some(_)) => CertificateStatus::Valid,
                (_, None) => CertificateStatus::NoExpiry,
            };
            rows.push(CertificateRow {
                customer_id: c.customer_id.clone(),
                customer_name: c.customer_name.clone(),
                state_name: states::state(&cert.state).map(|s| s.name.to_owned()).unwrap_or_default(),
                certificate: cert.clone(),
                status,
                days_left,
            });
        }
    }
    rows.sort_by(|a, b| {
        (a.status, a.days_left.unwrap_or(i64::MAX), &a.customer_name, &a.certificate.state).cmp(&(b.status, b.days_left.unwrap_or(i64::MAX), &b.customer_name, &b.certificate.state))
    });
    let expired = rows.iter().filter(|r| r.status == CertificateStatus::Expired).count();
    let expiring = rows.iter().filter(|r| r.status == CertificateStatus::Expiring).count();
    CertificateReport { as_of, within_days, rows, expired, expiring }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn cert(state: &str, number: &str, issued: Option<NaiveDate>, expires: Option<NaiveDate>) -> Certificate {
        Certificate { state: state.into(), number: number.into(), kind: CertificateKind::Resale, issued, expires, note: String::new() }
    }

    #[test]
    fn validation_and_cover() {
        let certs = validate(&[cert(" ca ", " SR-1 ", Some(d(2026, 1, 1)), Some(d(2026, 12, 31))), cert("TX", "17", None, None)]).unwrap();
        assert_eq!((certs[0].state.as_str(), certs[0].number.as_str()), ("CA", "SR-1"));
        assert!(validate(&[cert("ZZ", "1", None, None)]).is_err());
        assert!(validate(&[cert("CA", " ", None, None)]).is_err());
        assert!(validate(&[cert("CA", "1", Some(d(2026, 2, 1)), Some(d(2026, 1, 1)))]).is_err());
        assert!(validate(&[cert("CA", "1", None, None), cert("ca", "1", None, None)]).is_err());
        assert_eq!(covering(&certs, "CA", d(2026, 6, 1)).map(|c| c.number.as_str()), Some("SR-1"));
        assert!(covering(&certs, "CA", d(2025, 12, 31)).is_none(), "before it was given");
        assert!(covering(&certs, "CA", d(2027, 1, 1)).is_none(), "expired");
        assert!(covering(&certs, "NY", d(2026, 6, 1)).is_none(), "another state");
        assert!(covering(&certs, "tx", d(2030, 1, 1)).is_some(), "no expiry");
    }

    #[test]
    fn expiring_report() {
        let customers = vec![
            CustomerCertificates {
                customer_id: "1".into(),
                customer_name: "Acme".into(),
                certificates: vec![cert("CA", "A", None, Some(d(2026, 5, 1))), cert("NV", "B", None, Some(d(2026, 7, 15))), cert("TX", "C", None, None)],
            },
            CustomerCertificates { customer_id: "2".into(), customer_name: "Beta".into(), certificates: vec![cert("FL", "D", None, Some(d(2027, 12, 31))), cert("WA", "E", Some(d(2026, 9, 1)), None)] },
        ];
        let r = report(&customers, d(2026, 6, 30), 30);
        let rows: Vec<_> = r.rows.iter().map(|x| (x.certificate.number.as_str(), x.status, x.days_left)).collect();
        assert_eq!(rows, [
            ("A", CertificateStatus::Expired, Some(-60)),
            ("B", CertificateStatus::Expiring, Some(15)),
            ("D", CertificateStatus::Valid, Some(549)),
            ("C", CertificateStatus::NoExpiry, None),
            ("E", CertificateStatus::NotYetValid, None),
        ]);
        assert_eq!((r.expired, r.expiring), (1, 1));
        assert_eq!(r.rows[1].state_name, "Nevada");
    }
}

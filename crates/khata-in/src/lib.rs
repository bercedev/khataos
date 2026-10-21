//! India compliance pack.
//!
//! Implements the M1 items of `docs/research/india-compliance.md`:
//! - GSTIN validation: format, state code, embedded PAN, check character
//!   ([`gstin`]);
//! - place of supply and the CGST + SGST/UTGST vs IGST split, with SEZ
//!   supplies and exports always inter-State ([`gst`]);
//! - invoice numbers under CGST Rule 46(b): at most 16 characters of
//!   `[A-Za-z0-9/-]`, unique per financial year;
//! - April to March financial years (Companies Act s.2(41); CGST Rules 46, 53);
//! - GSTR-1 sections and a GSTR-3B summary from outward and inward
//!   documents, with a draft GSTR-1 JSON export ([`returns`]).
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`] (GST 2.0, Notif. 9/2025-CT(R), changed most rates from
//! 22 Sep 2025, and older documents keep the older rate).

pub mod calendar;
pub mod einvoice;
pub mod ewaybill;
pub mod gst;
pub mod gstin;
pub mod gstr2b;
pub mod irp;
pub mod pan;
pub mod rcm;
pub mod returns;
pub mod salary;
pub mod schedule3;
pub mod schedule3_notes;
pub mod states;
pub mod tcs;
pub mod tds;
pub mod tds_file;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "IN", name: "India", dialing_code: "+91" };
pub const CURRENCY: Currency = Currency { code: "INR", symbol: "₹", minor_units: 2 };

/// Indian financial years start on 1 April.
pub const FY_START_MONTH: u32 = 4;

/// The India pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct India;

impl CompliancePack for India {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// States and union territories where supplies are made (current codes
    /// only; legacy codes still validate inside GSTINs).
    fn regions(&self) -> Vec<Region> {
        states::STATES
            .iter()
            .filter(|s| !s.legacy && s.kind != states::Kind::CentreJurisdiction)
            .map(|s| Region { code: s.code, name: s.name })
            .collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "GSTIN"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        gstin::validate(id)
    }

    fn validate_invoice_number(
        &self,
        number: &str,
        date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        gst::validate_invoice_number(number, self.financial_year(date), issued)
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        gst::line_tax(line, rates)
    }
    fn calendar_defaults(&self) -> Option<khata_compliance::CalendarDefaults> {
        Some(calendar::defaults())
    }

    fn due_dates(&self, company: &khata_compliance::CalendarCompany, from: NaiveDate, to: NaiveDate) -> Vec<khata_compliance::DueDate> {
        calendar::due_dates(company, from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use khata_compliance::{PlaceOfSupply, Supply};
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// Everything an app needs goes through the trait object.
    #[test]
    fn works_as_a_trait_object() {
        let pack: &dyn CompliancePack = &India;
        assert_eq!((pack.country().code, pack.currency().code, pack.tax_id_label()), ("IN", "INR", "GSTIN"));
        let fy = pack.financial_year(d(2027, 1, 15));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2026, 4, 1), d(2027, 3, 31), "2026-27".into()));
        assert_eq!(pack.validate_tax_id("27aapfu0939f1zv").unwrap().value, "27AAPFU0939F1ZV");
        let issued = [IssuedDocument { number: "7", date: d(2026, 4, 2) }];
        assert!(pack.validate_invoice_number("7", d(2027, 3, 31), &issued).is_err());
        assert!(pack.validate_invoice_number("7", d(2027, 4, 1), &issued).is_ok(), "a new year may reuse it");

        let mut rates = RateSchedule::default();
        rates.set("9983", d(2017, 7, 1), dec!(18)).unwrap();
        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "998311".into(),
            taxable_value: dec!(50000),
            supply: Supply {
                supplier_region: "27".into(),
                place_of_supply: PlaceOfSupply::Region("24".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        assert_eq!(pack.line_tax(&line, &rates).unwrap().total(), dec!(9000));
    }

    #[test]
    fn regions_are_current_places_of_supply() {
        let regions = India.regions();
        assert_eq!(regions.len(), 37);
        assert!(regions.iter().any(|r| r.code == "37" && r.name == "Andhra Pradesh"));
        assert!(!regions.iter().any(|r| ["25", "28", "99"].contains(&r.code)));
    }
}

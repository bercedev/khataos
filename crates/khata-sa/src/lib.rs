//! Kingdom of Saudi Arabia compliance pack.
//!
//! Implements the KSA items of `docs/research/mena-compliance.md` §5 and §6
//! (see also `docs/ksa-zatca.md`):
//! - VAT number validation: 15 digits, first and last "3", VAT-group flag
//!   at digit 11; no checksum, because none is published ([`vat_number`]);
//! - the thirteen administrative regions, with ISO 3166-2:SA codes
//!   ([`regions`]);
//! - VAT on a line: one "VAT" component at the class's rate (15% standard
//!   from 1 Jul 2020, 5% before; 0% zero-rated, exempt and out-of-scope
//!   classes), exports zero-rated, half-up rounding ([`vat`]);
//! - tax invoice numbers: unique across all years, at most 35 characters
//!   of `[A-Za-z0-9/-]` (a KhataOS choice; see
//!   [`vat::MAX_INVOICE_NUMBER_LEN`]);
//! - January to December years by default;
//! - ZATCA e-invoices: UBL 2.1 XML, the invoice hash, the ICV/PIH chain and
//!   the QR code ([`zatca`], [`qr`]), with the XAdES cryptographic stamp
//!   (ECDSA secp256k1 over the hash, [`sign`], [`xades`]);
//! - onboarding: the device's PKCS#10 CSR with ZATCA's fields ([`csr`]);
//! - the VAT return from outward and inward documents ([`returns`]), with
//!   imports paid at customs (field 8) and the reverse charge (field 9)
//!   marked on tax classes ([`vat::Mechanism`]).
//!
//! The CSID and clearance/reporting API calls (network) live in the desktop
//! app (`zatca_api.rs`). Not modelled yet: prepayments, deferred
//! import VAT (IR Art 44), the margin scheme, citizen healthcare/education, input-tax
//! blocking, and Zakat.
//!
//! Rates are not in this crate: they come from an effective-dated
//! [`RateSchedule`]; [`vat::default_rates`] is a starting list.

pub mod calendar;
pub mod csr;
pub mod qr;
pub mod regions;
pub mod returns;
pub mod sign;
pub mod vat;
pub mod vat_number;
pub mod xades;
pub mod zatca;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "SA", name: "Saudi Arabia", dialing_code: "+966" };

/// Saudi riyal, 100 halalas. The symbol is the ISO code: the research
/// doesn't cover the currency sign, and VAT amounts must be stated in SAR
/// (research §5).
pub const CURRENCY: Currency = Currency { code: "SAR", symbol: "SAR", minor_units: 2 };

/// Books default to Gregorian calendar years. The research fixes no VAT
/// year (returns are monthly or quarterly, §5); a Zakat year may be
/// Gregorian or Hijri, which only changes the Zakat rate's day count (§7,
/// §8 point 11). KhataOS keeps the books in Gregorian dates; Hijri dates are
/// for display only, and the e-invoice is always Gregorian (BR-KSA-04 and
/// related rules, research §6.2). A company can still change its books'
/// first month.
pub const FY_START_MONTH: u32 = 1;

/// The Saudi pack. It holds no company data, so one value serves every
/// company.
#[derive(Debug, Clone, Copy, Default)]
pub struct Saudi;

impl CompliancePack for Saudi {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The thirteen regions, in ISO code order.
    fn regions(&self) -> Vec<Region> {
        regions::REGIONS.iter().map(|r| Region { code: r.code, name: r.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "VAT number"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        vat_number::validate(id)
    }

    /// Unique among every number already issued in the series, in any year.
    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        vat::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        vat::line_tax(line, rates)
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
        let pack: &dyn CompliancePack = &Saudi;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("SA", "Saudi Arabia", "+966"));
        assert_eq!((pack.currency().code, pack.currency().minor_units, pack.tax_id_label()), ("SAR", 2, "VAT number"));
        let fy = pack.financial_year(d(2027, 1, 15));
        assert_eq!((fy.start, fy.end, fy.label()), (d(2027, 1, 1), d(2027, 12, 31), "2027".into()));
        let vat = pack.validate_tax_id("300-0755-8870-0003").unwrap();
        assert_eq!((vat.value.as_str(), vat.entity_id, vat.region), ("300075588700003", None, None));
        assert!(pack.validate_tax_id("100075588700003").is_err());

        let issued = [IssuedDocument { number: "7", date: d(2026, 4, 2) }];
        assert_eq!(
            pack.validate_invoice_number("7", d(2027, 1, 1), &issued),
            Err(InvoiceNumberError::Duplicate { number: "7".into(), year: "2026".into() }),
            "no yearly reset"
        );

        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "STANDARD".into(),
            taxable_value: dec!(1999.90),
            supply: Supply {
                supplier_region: "SA-01".into(),
                place_of_supply: PlaceOfSupply::Region("SA-02".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        let t = pack.line_tax(&line, &vat::default_rates()).unwrap();
        assert_eq!((t.components.len(), t.components[0].tax, t.total()), (1, "VAT", dec!(299.99)));
    }

    #[test]
    fn regions_are_the_thirteen_iso_regions() {
        let regions = Saudi.regions();
        assert_eq!(regions.len(), 13);
        assert_eq!(regions[0], Region { code: "SA-01", name: "Riyadh" });
        assert!(regions.contains(&Region { code: "SA-14", name: "Asir" }));
    }

    #[test]
    fn calendar_years_by_default() {
        let fy = Saudi.financial_year(d(2028, 2, 29));
        assert_eq!((fy.start, fy.end), (d(2028, 1, 1), d(2028, 12, 31)));
    }
}

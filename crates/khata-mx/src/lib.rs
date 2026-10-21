//! Mexico compliance pack.
//!
//! Implements the items of `docs/research/mexico-compliance.md` (see also
//! `docs/mexico-cfdi.md`):
//! - the 32 federal entities as regions, ISO 3166-2:MX codes ([`states`]);
//! - RFC validation with the SAT check digit ([`rfc`]);
//! - IVA on a line: 16% general, 8% border-region stimulus, 0% and exempt,
//!   exports at 0% ([`iva`]);
//! - calendar years: the ejercicio fiscal is the calendar year (CFF
//!   art. 11) ([`FY_START_MONTH`]);
//! - CFDI 4.0 XML (Anexo 20) from a neutral invoice model, its cadena
//!   original and the sello with the issuer's CSD ([`cfdi`]), with ISR and
//!   IVA withholdings from the customer's `RET:` class ([`retenciones`]);
//! - the Recibo Electrónico de Pago: CFDI de tipo P with the Complemento
//!   de Pagos 2.0 for PPD invoices ([`pagos`]);
//! - the PAC's answer (TimbreFiscalDigital 1.1), the printed QR text and
//!   the cancellation rules ([`timbre`]). The PAC connection itself is in
//!   the desktop app (`pac_client.rs`): a CFDI is only valid once a PAC
//!   certifies it;
//! - the monthly IVA summary, with withholdings ([`returns`]).
//!
//! Not modelled: IEPS, the factura global's InformacionGlobal node,
//! foreign-currency payments, DIOT and Contabilidad electrónica (Anexo 24).

pub mod calendar;
pub mod cfdi;
pub mod iva;
pub mod pagos;
pub mod retenciones;
pub mod returns;
pub mod rfc;
pub mod states;
pub mod timbre;

use chrono::NaiveDate;
use khata_compliance::{
    CompliancePack, Country, Currency, FinancialYear, InvoiceNumberError, IssuedDocument, LineTax, RateSchedule,
    Region, TaxError, TaxIdDetails, TaxIdError, TaxLine,
};

pub const COUNTRY: Country = Country { code: "MX", name: "Mexico", dialing_code: "+52" };

/// Mexican peso, 100 centavos.
pub const CURRENCY: Currency = Currency { code: "MXN", symbol: "$", minor_units: 2 };

/// The ejercicio fiscal coincides with the calendar year (CFF art. 11),
/// so books run January to December.
pub const FY_START_MONTH: u32 = 1;

/// The Mexico pack. It holds no company data.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mexico;

impl CompliancePack for Mexico {
    fn country(&self) -> Country {
        COUNTRY
    }

    fn currency(&self) -> Currency {
        CURRENCY
    }

    /// The 32 federal entities.
    fn regions(&self) -> Vec<Region> {
        states::ENTITIES.iter().map(|e| Region { code: e.code, name: e.name }).collect()
    }

    fn financial_year(&self, date: NaiveDate) -> FinancialYear {
        FinancialYear::containing(date, FY_START_MONTH)
    }

    fn tax_id_label(&self) -> &'static str {
        "RFC"
    }

    fn validate_tax_id(&self, id: &str) -> Result<TaxIdDetails, TaxIdError> {
        rfc::validate(id)
    }

    fn validate_invoice_number(
        &self,
        number: &str,
        _date: NaiveDate,
        issued: &[IssuedDocument<'_>],
    ) -> Result<(), InvoiceNumberError> {
        iva::validate_invoice_number(number, issued, |d| self.financial_year(d))
    }

    fn line_tax(&self, line: &TaxLine, rates: &RateSchedule) -> Result<LineTax, TaxError> {
        iva::line_tax(line, rates)
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

    #[test]
    fn works_as_a_trait_object() {
        let pack: &dyn CompliancePack = &Mexico;
        assert_eq!((pack.country().code, pack.country().name, pack.country().dialing_code), ("MX", "Mexico", "+52"));
        assert_eq!((pack.currency().code, pack.currency().minor_units), ("MXN", 2));
        assert_eq!((pack.tax_id_label(), pack.regions().len()), ("RFC", 32));
        let fy = pack.financial_year(d(2026, 10, 20));
        assert_eq!((fy.start, fy.end), (d(2026, 1, 1), d(2026, 12, 31)));
        assert_eq!(pack.validate_tax_id("eku9003173c9").unwrap().value, "EKU9003173C9");
        assert!(pack.validate_tax_id("EKU9003173C1").is_err());
        let line = TaxLine {
            date: d(2026, 6, 1),
            tax_class: "STANDARD".into(),
            taxable_value: dec!(100),
            supply: Supply {
                supplier_region: "CMX".into(),
                place_of_supply: PlaceOfSupply::Region("JAL".into()),
                special_zone: false,
                without_payment: false,
            },
        };
        assert_eq!(pack.line_tax(&line, &iva::default_rates()).unwrap().total(), dec!(16));
        assert_eq!(pack.validate_invoice_number("A-1", d(2026, 1, 1), &[]), Ok(()));
    }
}

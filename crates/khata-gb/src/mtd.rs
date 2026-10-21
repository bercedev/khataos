//! The Making Tax Digital (MTD) VAT API body for submitting a return.
//!
//! HMRC Developer Hub, VAT (MTD) API 1.0, "Submit VAT return for period":
//! `POST /organisations/vat/{vrn}/returns` with a `VATReturnDeclaration`
//! JSON body (`docs/research/uk-compliance.md` §1.6 [PRIMARY: the API's
//! OpenAPI file]). This module only **renders** that body from a
//! [`VatReturn`]; nothing is sent. Field rules from the specification:
//!
//! | Field | Box | Rule |
//! |---|---|---|
//! | `periodKey` | — | 4 characters, from the obligations endpoint; letters and digits, "occasionally" `#` |
//! | `vatDueSales` | 1 | 2 decimal places, ±9,999,999,999,999.99 |
//! | `vatDueAcquisitions` | 2 | same |
//! | `totalVatDue` | 3 | same; must equal Box 1 + Box 2 (`VAT_TOTAL_VALUE`) |
//! | `vatReclaimedCurrPeriod` | 4 | same |
//! | `netVatDue` | 5 | **0 to 99,999,999,999.99**: the absolute difference of Boxes 3 and 4; HMRC works out whether it is payable or repayable (`VAT_NET_VALUE`) |
//! | `totalValueSalesExVAT` … `totalAcquisitionsExVAT` | 6–9 | "2 zeroed decimal places" (whole pounds), ±9,999,999,999,999 |
//! | `finalised` | — | must be `true` to be accepted (403 `NOT_FINALISED`) |
//!
//! `finalised` is the user's legal declaration that the return is true and
//! complete. KhataOS does not submit returns, so the desktop app renders it
//! `false`; whatever submits the file must ask for the declaration and set
//! it. Submitting also needs, as future work: OAuth 2.0 (authorisation-code
//! flow, scope `write:vat`), the `Accept: application/vnd.hmrc.1.0+json`
//! header and HMRC's fraud-prevention headers (`Gov-Client-*`,
//! `Gov-Vendor-*`; for a desktop app connecting directly,
//! `Gov-Client-Connection-Method: DESKTOP_APP_DIRECT`), which are required
//! by law.
//!
//! Amounts are written as exact JSON numbers from decimals (never through
//! floating point): pence as two decimals ("105.50"), whole pounds without
//! decimals ("300").

use rust_decimal::Decimal;
use serde::Serialize;
use thiserror::Error;

use crate::returns::VatReturn;

/// Largest Boxes 1 to 4 (and smallest, negated).
pub const MAX_VAT: Decimal = Decimal::from_parts(2_764_472_319, 232_830, 0, false, 2); // 9_999_999_999_999.99
/// Largest Box 5.
pub const MAX_NET: Decimal = Decimal::from_parts(1_316_134_911, 2_328, 0, false, 2); // 99_999_999_999.99
/// Largest Boxes 6 to 9 (and smallest, negated).
pub const MAX_VALUE: Decimal = Decimal::from_parts(1_316_134_911, 2_328, 0, false, 0); // 9_999_999_999_999

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MtdError {
    #[error("the period key must be the 4 characters HMRC gives for the period (letters, digits or #), not {0:?}")]
    PeriodKey(String),
    #[error("{field} is {value}, outside what the MTD API accepts")]
    OutOfRange { field: &'static str, value: String },
}

/// The `VATReturnDeclaration` body. Field names are the API's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MtdVatReturn {
    pub period_key: String,
    pub vat_due_sales: Decimal,
    pub vat_due_acquisitions: Decimal,
    pub total_vat_due: Decimal,
    pub vat_reclaimed_curr_period: Decimal,
    pub net_vat_due: Decimal,
    #[serde(rename = "totalValueSalesExVAT")]
    pub total_value_sales_ex_vat: Decimal,
    #[serde(rename = "totalValuePurchasesExVAT")]
    pub total_value_purchases_ex_vat: Decimal,
    #[serde(rename = "totalValueGoodsSuppliedExVAT")]
    pub total_value_goods_supplied_ex_vat: Decimal,
    #[serde(rename = "totalAcquisitionsExVAT")]
    pub total_acquisitions_ex_vat: Decimal,
    pub finalised: bool,
}

/// Whether a period key has the published shape: 4 characters, letters,
/// digits or `#` ("18A1", "#001").
pub fn valid_period_key(key: &str) -> bool {
    key.chars().count() == 4 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '#')
}

fn check(field: &'static str, value: Decimal, min: Decimal, max: Decimal) -> Result<Decimal, MtdError> {
    if value < min || value > max {
        return Err(MtdError::OutOfRange { field, value: value.normalize().to_string() });
    }
    Ok(value)
}

impl MtdVatReturn {
    /// The body for `ret` and the period key HMRC gave for its period.
    /// Box 3 is re-derived from Boxes 1 and 2 and Box 5 from Boxes 3 and 4,
    /// so HMRC's `VAT_TOTAL_VALUE` and `VAT_NET_VALUE` checks always pass.
    pub fn from_return(ret: &VatReturn, period_key: &str, finalised: bool) -> Result<Self, MtdError> {
        let period_key = period_key.trim();
        if !valid_period_key(period_key) {
            return Err(MtdError::PeriodKey(period_key.to_owned()));
        }
        let money = |field, v: Decimal| check(field, v.round_dp(2), -MAX_VAT, MAX_VAT);
        let pounds = |field, v: Decimal| check(field, v.trunc(), -MAX_VALUE, MAX_VALUE);
        let vat_due_sales = money("vatDueSales (Box 1)", ret.vat_due_sales)?;
        let vat_due_acquisitions = money("vatDueAcquisitions (Box 2)", ret.vat_due_acquisitions)?;
        let total_vat_due = money("totalVatDue (Box 3)", vat_due_sales + vat_due_acquisitions)?;
        let vat_reclaimed_curr_period = money("vatReclaimedCurrPeriod (Box 4)", ret.vat_reclaimed)?;
        let net_vat_due =
            check("netVatDue (Box 5)", (total_vat_due - vat_reclaimed_curr_period).abs(), Decimal::ZERO, MAX_NET)?;
        Ok(Self {
            period_key: period_key.to_owned(),
            vat_due_sales,
            vat_due_acquisitions,
            total_vat_due,
            vat_reclaimed_curr_period,
            net_vat_due,
            total_value_sales_ex_vat: pounds("totalValueSalesExVAT (Box 6)", ret.total_sales_ex_vat)?,
            total_value_purchases_ex_vat: pounds("totalValuePurchasesExVAT (Box 7)", ret.total_purchases_ex_vat)?,
            total_value_goods_supplied_ex_vat: pounds(
                "totalValueGoodsSuppliedExVAT (Box 8)",
                ret.goods_supplied_to_eu,
            )?,
            total_acquisitions_ex_vat: pounds("totalAcquisitionsExVAT (Box 9)", ret.goods_acquired_from_eu)?,
            finalised,
        })
    }

    /// The JSON body, pretty-printed, with amounts as exact JSON numbers.
    pub fn to_json(&self) -> String {
        let pence = |v: Decimal| format!("{:.2}", v.round_dp(2));
        let pounds = |v: Decimal| v.trunc().normalize().to_string();
        // The period key is 4 characters of [A-Za-z0-9#]: nothing to escape.
        format!(
            "{{\n  \"periodKey\": \"{}\",\n  \"vatDueSales\": {},\n  \"vatDueAcquisitions\": {},\n  \"totalVatDue\": {},\n  \"vatReclaimedCurrPeriod\": {},\n  \"netVatDue\": {},\n  \"totalValueSalesExVAT\": {},\n  \"totalValuePurchasesExVAT\": {},\n  \"totalValueGoodsSuppliedExVAT\": {},\n  \"totalAcquisitionsExVAT\": {},\n  \"finalised\": {}\n}}\n",
            self.period_key,
            pence(self.vat_due_sales),
            pence(self.vat_due_acquisitions),
            pence(self.total_vat_due),
            pence(self.vat_reclaimed_curr_period),
            pence(self.net_vat_due),
            pounds(self.total_value_sales_ex_vat),
            pounds(self.total_value_purchases_ex_vat),
            pounds(self.total_value_goods_supplied_ex_vat),
            pounds(self.total_acquisitions_ex_vat),
            self.finalised,
        )
    }
}

/// The MTD JSON body for a return: [`MtdVatReturn::from_return`] then
/// [`MtdVatReturn::to_json`].
pub fn vat_return_json(ret: &VatReturn, period_key: &str, finalised: bool) -> Result<String, MtdError> {
    MtdVatReturn::from_return(ret, period_key, finalised).map(|m| m.to_json())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::returns::Period;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn ret(b1: Decimal, b4: Decimal, b6: Decimal, b7: Decimal) -> VatReturn {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        VatReturn {
            period: Period { from: d(4, 1), to: d(6, 30) },
            vat_due_sales: b1,
            vat_due_acquisitions: dec!(0),
            total_vat_due: b1,
            vat_reclaimed: b4,
            net_vat_due: b1 - b4,
            total_sales_ex_vat: b6,
            total_purchases_ex_vat: b7,
            goods_supplied_to_eu: dec!(0),
            goods_acquired_from_eu: dec!(0),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn constants_match_the_specification() {
        assert_eq!(MAX_VAT, dec!(9999999999999.99));
        assert_eq!(MAX_NET, dec!(99999999999.99));
        assert_eq!(MAX_VALUE, dec!(9999999999999));
    }

    #[test]
    fn renders_the_declaration_with_exact_numbers() {
        let json = vat_return_json(&ret(dec!(1870), dec!(1000.5), dec!(12600), dec!(5900)), "26A1", false).unwrap();
        assert_eq!(
            json,
            "{\n  \"periodKey\": \"26A1\",\n  \"vatDueSales\": 1870.00,\n  \"vatDueAcquisitions\": 0.00,\n  \"totalVatDue\": 1870.00,\n  \"vatReclaimedCurrPeriod\": 1000.50,\n  \"netVatDue\": 869.50,\n  \"totalValueSalesExVAT\": 12600,\n  \"totalValuePurchasesExVAT\": 5900,\n  \"totalValueGoodsSuppliedExVAT\": 0,\n  \"totalAcquisitionsExVAT\": 0,\n  \"finalised\": false\n}\n"
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["netVatDue"], serde_json::json!(869.5), "valid JSON with numbers, not strings");
        assert_eq!(v["finalised"], false);
    }

    #[test]
    fn a_repayment_is_sent_as_a_positive_net_figure() {
        let m = MtdVatReturn::from_return(&ret(dec!(20), dec!(200), dec!(100), dec!(1000)), "#001", true).unwrap();
        assert_eq!((m.total_vat_due, m.vat_reclaimed_curr_period, m.net_vat_due), (dec!(20), dec!(200), dec!(180)));
        assert!(m.finalised);
        let v = serde_json::to_value(&m).unwrap();
        assert!(v.get("totalValueSalesExVAT").is_some() && v.get("vatReclaimedCurrPeriod").is_some(), "{v}");
    }

    #[test]
    fn boxes_6_to_9_are_whole_pounds_even_if_given_pence() {
        let m = MtdVatReturn::from_return(&ret(dec!(0), dec!(0), dec!(99.99), dec!(-5.5)), "18A1", false).unwrap();
        assert_eq!((m.total_value_sales_ex_vat, m.total_value_purchases_ex_vat), (dec!(99), dec!(-5)));
        assert!(m.to_json().contains("\"totalValuePurchasesExVAT\": -5,"));
    }

    #[test]
    fn period_keys_and_ranges_are_checked() {
        let r = ret(dec!(1), dec!(0), dec!(5), dec!(0));
        for bad in ["", "18A", "18A12", "18-1", "18 1", "١٨A1"] {
            assert_eq!(MtdVatReturn::from_return(&r, bad, false), Err(MtdError::PeriodKey(bad.trim().into())), "{bad:?}");
        }
        assert!(MtdVatReturn::from_return(&r, " 18AD ", false).is_ok(), "surrounding space is trimmed");
        let big = ret(dec!(100000000000), dec!(0), dec!(0), dec!(0));
        assert_eq!(
            MtdVatReturn::from_return(&big, "18A1", false),
            Err(MtdError::OutOfRange { field: "netVatDue (Box 5)", value: "100000000000".into() })
        );
        let huge = ret(dec!(0), dec!(0), dec!(10000000000000), dec!(0));
        assert!(matches!(MtdVatReturn::from_return(&huge, "18A1", false), Err(MtdError::OutOfRange { .. })));
    }
}

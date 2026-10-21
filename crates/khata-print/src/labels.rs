//! Arabic for the invoice labels that invoice designs add (ship-to, bank
//! details, export details, currency…), shared by the HTML and PDF
//! writers. The original labels of the bilingual invoice are in
//! [`crate::ArabicLabels`]. Like those, these should be checked by an
//! Arabic-reading tax adviser (`docs/invoice-design.md`).

/// The Arabic of an English label, when there is one.
pub(crate) fn arabic(en: &str) -> Option<&'static str> {
    Some(match en {
        "Bill to" => "فاتورة إلى",
        "Ship to" => "عنوان الشحن",
        "Address" => "العنوان",
        "Bank details" => "البيانات المصرفية",
        "Account name" => "اسم الحساب",
        "Bank" => "البنك",
        "Branch" => "الفرع",
        "Account number" => "رقم الحساب",
        "IBAN" => "رقم الآيبان",
        "SWIFT / BIC" => "رمز السويفت",
        "Terms and conditions" => "الشروط والأحكام",
        "Declaration" => "إقرار",
        "Export details" => "بيانات التصدير",
        "Port of loading" => "ميناء الشحن",
        "Port of discharge" => "ميناء التفريغ",
        "Incoterms" => "شروط التسليم (إنكوترمز)",
        "Country of destination" => "بلد الوجهة",
        "Shipping bill no." => "رقم بيان التصدير",
        "Shipping bill date" => "تاريخ بيان التصدير",
        "Currency" => "العملة",
        "Exchange rate" => "سعر الصرف",
        "Total in" => "الإجمالي بعملة",
        "Tax in" => "الضريبة بعملة",
        "Authorised signatory" => "المفوض بالتوقيع",
        "Amount in words" => "المبلغ كتابة",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_new_label_has_arabic() {
        for en in ["Ship to", "Bank details", "Port of loading", "Exchange rate", "Terms and conditions", "Declaration"] {
            assert!(super::arabic(en).is_some(), "{en}");
        }
        assert_eq!(super::arabic("IFSC"), None, "local bank codes stay as they are");
    }
}

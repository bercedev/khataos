//! Printing a saved voucher as a tax invoice, credit note or debit note
//! (or a plain voucher for the other types).

use khata_core::format::Grouping;
use khata_core::words::{WordsStyle, amount_words};
use khata_core::{Amount, Book, DrCr, GroupId, Voucher, VoucherId, VoucherKind};
use khata_print::{InvoiceDoc, InvoiceLine, PrintLanguage};

use crate::api::{ApiResult, Slot, open_ref, pack, parse_uuid};

/// `amount` in words as this company's documents write it: its country's
/// convention (`khata_core::words`), or Arabic tafqit when `arabic`.
/// `currency` defaults to the company's base currency.
pub(crate) fn words(book: &Book, amount: Amount, currency: Option<&str>, arabic: bool) -> String {
    let style = if arabic { WordsStyle::Arabic } else { WordsStyle::for_country(book.country()) };
    words_in(amount, currency.or_else(|| pack(book).map(|p| p.currency().code)).unwrap_or_default(), style)
}

fn words_in(amount: Amount, currency: &str, style: WordsStyle) -> String {
    amount_words(amount, currency, style).trim().to_owned()
}

/// The default printed title of a voucher type (`invoice_design` applies
/// the company's choice and the country's rule).
pub(crate) fn title(kind: VoucherKind) -> &'static str {
    match kind {
        VoucherKind::Sales => "Tax Invoice",
        VoucherKind::Purchase => "Purchase Voucher",
        VoucherKind::CreditNote => "Credit Note",
        VoucherKind::DebitNote => "Debit Note",
        VoucherKind::Payment => "Payment Voucher",
        VoucherKind::Receipt => "Receipt Voucher",
        VoucherKind::Contra => "Contra Voucher",
        VoucherKind::Journal => "Journal Voucher",
        VoucherKind::StockJournal => "Stock Journal",
    }
}

fn is_party(book: &Book, ledger: khata_core::LedgerId) -> bool {
    let parties = [GroupId::default_named("Sundry Debtors"), GroupId::default_named("Sundry Creditors")];
    crate::api::ancestors(book, book.ledger(ledger).map(|l| l.group)).any(|g| parties.contains(&g.id))
}

/// The party a trading voucher is with: a debtor/creditor line, else the
/// cash or bank line on the party's side.
pub(crate) fn party_line(book: &Book, v: &Voucher) -> Option<usize> {
    let party_side = match v.kind {
        VoucherKind::Sales | VoucherKind::DebitNote => DrCr::Dr,
        _ => DrCr::Cr,
    };
    v.entries
        .iter()
        .position(|e| is_party(book, e.ledger))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side && book.is_cash_or_bank(e.ledger)))
        .or_else(|| v.entries.iter().position(|e| e.side == party_side))
}

pub fn invoice_doc(book: &Book, v: &Voucher) -> InvoiceDoc {
    let pack = pack(book);
    let profile = book.profile();
    let label = pack.map(|p| p.tax_id_label().to_owned());
    let region_name = |code: &str| {
        pack.and_then(|p| p.regions().into_iter().find(|r| r.code == code)).map(|r| format!("{} ({code})", r.name))
    };
    let party = party_line(book, v);
    let party_ledger = party.and_then(|i| book.ledger(v.entries[i].ledger));
    let party_region = party_ledger.zip(pack).and_then(|(l, p)| crate::tax::party_region(p, l));
    // The party's side carries the total; every other line is printed.
    let party_side = party.map(|i| v.entries[i].side);
    let mut lines = Vec::new();
    let mut taxes = Vec::new();
    let mut others = Vec::new();
    // A UK sale under the domestic reverse charge (a "DRC:" class on an item or service).
    let drc = |class: Option<&str>| {
        class.is_some_and(|c| khata_gb::vat::mechanism(c).0 == khata_gb::vat::Mechanism::DomesticReverseCharge)
    };
    let mut reverse_charge = false;
    for (i, e) in v.entries.iter().enumerate() {
        if Some(i) == party {
            continue;
        }
        // Lines on the party's side reduce the total (a discount on a sale).
        let signed = if Some(e.side) == party_side { -e.amount } else { e.amount };
        let ledger = book.ledger(e.ledger);
        let name = ledger.map(|l| l.name.clone()).unwrap_or_default();
        if !e.items.is_empty() {
            for l in &e.items {
                let item = book.stock_item(l.item);
                reverse_charge |= drc(item.and_then(|i| i.tax_class.as_deref()));
                let unit = item.and_then(|i| book.unit(i.unit)).map(|u| u.symbol.clone()).unwrap_or_default();
                lines.push(InvoiceLine {
                    description: item.map(|i| i.name.clone()).unwrap_or_default(),
                    // Show the HSN/SAC itself, without the reverse-charge / blocked-credit markers.
                    tax_class: crate::gst::class_flags(item.and_then(|i| i.tax_class.as_deref())).0,
                    qty: Some(format!("{} {unit}", l.qty.normalize()).trim().to_owned()),
                    rate: Some(l.rate),
                    amount: l.amount,
                });
            }
        } else if ledger.is_some_and(|l| l.tax.is_some()) {
            taxes.push((name, signed));
        } else if lines.is_empty() || Some(e.side) == party_side {
            // Before any item: a service line; on the party's side: an adjustment.
            if Some(e.side) == party_side {
                others.push((name, signed));
            } else {
                reverse_charge |= drc(ledger.and_then(|l| l.tax_class.as_deref()));
                lines.push(InvoiceLine { description: name, tax_class: None, qty: None, rate: None, amount: e.amount });
            }
        } else {
            others.push((name, signed));
        }
    }
    let total = party.map_or_else(|| v.totals().0, |i| v.entries[i].amount);
    let mut narration = v.narration.clone();
    // Notice 735 §7.6.1: the invoice must say the customer accounts for the VAT.
    if reverse_charge
        && book.country() == Some("GB")
        && matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote | VoucherKind::DebitNote)
    {
        let note = "Reverse charge: Customer to pay the VAT to HMRC.";
        narration = if narration.trim().is_empty() { note.to_owned() } else { format!("{} {note}", narration.trim_end()) };
    }
    let mut doc = InvoiceDoc {
        title: title(v.kind).to_owned(),
        number: v.number.clone(),
        date: v.date,
        seller_tax: label.clone().zip(profile.and_then(|p| p.tax_id.clone())),
        seller_region: profile.and_then(|p| p.tax_region.as_deref()).and_then(region_name),
        buyer_name: party_ledger.map(|l| l.name.clone()).unwrap_or_default(),
        buyer_tax: label.zip(party_ledger.and_then(|l| l.tax_id.clone())),
        place_of_supply: party_region
            .or_else(|| profile.and_then(|p| p.tax_region.clone()))
            .and_then(|c| region_name(&c)),
        lines,
        taxes,
        others,
        total,
        total_in_words: words(book, total, None, false),
        narration,
        cancelled: v.cancelled,
        // Filled for prints by `invoice_design::prepare` (ship-to, export,
        // statutory notes, blocks) and by the multi-currency code (`currency`).
        ..Default::default()
    };
    // A party kept in a foreign currency: the invoice in that currency, with the rate and base totals.
    crate::forex::invoice_currency(book, v, &mut doc);
    doc
}

/// Print-ready HTML for a saved voucher, in English.
pub fn voucher_html(slot: &Slot, id: &str) -> ApiResult<String> {
    voucher_html_in(slot, id, PrintLanguage::English, None)
}

/// Print-ready HTML for a saved voucher in `lang`, in the company's invoice
/// design (`invoice_design`). KSA and UAE invoices and notes print
/// bilingually whatever the language; any other voucher prints right to
/// left in Arabic when `lang` is Arabic. `hijri` is the voucher date in the
/// Hijri calendar, formatted by the frontend (Rust has no Hijri calendar);
/// it prints under the date on bilingual and Arabic prints.
pub fn voucher_html_in(slot: &Slot, id: &str, lang: PrintLanguage, hijri: Option<&str>) -> ApiResult<String> {
    crate::invoice_design::voucher_html(slot, id, lang, hijri)
}

/// Print-ready HTML for a cheque from a saved Payment out of a bank
/// ledger: payee = the ledger paid, amount = what left the bank.
pub fn cheque_html(slot: &Slot, id: &str, account_payee: bool) -> ApiResult<String> {
    let book = open_ref(slot)?;
    let v = book.voucher(VoucherId(parse_uuid(id, "voucher")?)).ok_or("unknown voucher")?;
    if v.kind != VoucherKind::Payment || v.cancelled {
        return Err("cheques are printed from a saved, uncancelled Payment voucher".into());
    }
    let bank = v
        .entries
        .iter()
        .find(|e| e.side == DrCr::Cr && book.is_cash_or_bank(e.ledger) && !is_cash(book, e.ledger))
        .ok_or("this payment wasn't made from a bank ledger")?;
    let payee = v
        .entries
        .iter()
        .filter(|e| e.side == DrCr::Dr)
        .max_by_key(|e| e.amount)
        .and_then(|e| book.ledger(e.ledger))
        .map(|l| l.name.clone())
        .unwrap_or_default();
    let grouping = if book.country() == Some("IN") { Grouping::Indian } else { Grouping::Western };
    // The bank's convention: its country (a bank abroad has one on its
    // mailing details), else the company's; a foreign-currency account's
    // cheque is in that currency.
    let bank_ledger = book.ledger(bank.ledger);
    let country = bank_ledger
        .and_then(|l| l.mailing.as_ref())
        .and_then(|m| m.country.as_deref())
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .or(book.country());
    let fx = bank.forex.as_ref().filter(|f| bank_ledger.and_then(|l| l.currency.as_deref()) == Some(f.currency.as_str()));
    let (amount, currency) = match fx {
        Some(f) => (f.amount, Some(f.currency.as_str())),
        None => (bank.amount, pack(book).map(|p| p.currency().code)),
    };
    let cheque = khata_print::ChequeDoc {
        payee,
        date: v.date,
        amount,
        amount_in_words: words_in(amount, currency.unwrap_or_default(), WordsStyle::for_country(country)),
        account_payee,
    };
    Ok(khata_print::cheque_html(&cheque, &khata_print::ChequeLayout::default(), grouping))
}

fn is_cash(book: &Book, ledger: khata_core::LedgerId) -> bool {
    let cash = GroupId::default_named("Cash-in-Hand");
    crate::api::ancestors(book, book.ledger(ledger).map(|l| l.group)).any(|g| g.id == cash)
}

#[cfg(test)]
mod tests {
    use khata_core::DrCr;

    use super::*;
    use crate::api::tests::{entry, group_id, indian_company, ledger, voucher};
    use crate::api::{StockLineInput, create_party_ledger, post_voucher};
    use crate::masters::{StockItemInput, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax};

    #[test]
    fn a_sales_voucher_prints_as_a_tax_invoice() {
        let (_f, mut slot) = indian_company("invoice-print");
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_company_address(&mut slot, "12 MG Road\nPune 411001").unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Bengaluru <Co>", &debtors, "", None, Some("29AAGCB7383J1Z4"), None)
            .unwrap()
            .ledger
            .id;
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let igst = ledger(&mut slot, "Output IGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &igst, "IGST", khata_core::TaxSide::Output).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap();
        let laptop = save_stock_item(&mut slot, &StockItemInput {
            id: String::new(),
            name: "Laptop".into(),
            group_id: String::new(),
            unit_id: nos.id,
            opening_qty: "5".into(),
            opening_rate: "40000".into(),
            opening_value: String::new(),
            tax_class: "847130".into(),
            costing: None,
        })
        .unwrap();
        let mut items = entry(&sales, DrCr::Cr, "100000");
        items.items = vec![StockLineInput {
            item_id: laptop.id,
            qty: "2".into(),
            rate: "50000".into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }];
        let posted = post_voucher(
            &mut slot,
            voucher("Sales", "2026-05-02", vec![entry(&buyer, DrCr::Dr, "118000"), items, entry(&igst, DrCr::Cr, "18000")]),
        )
        .unwrap();
        let html = voucher_html(&slot, &posted.id).unwrap();
        for needle in [
            "Tax Invoice",
            "26-27/1",
            "GSTIN: 27AAPFU0939F1ZV",
            "12 MG Road",
            "Bengaluru &lt;Co&gt;",
            "29AAGCB7383J1Z4",
            "Karnataka (29)",
            "847130",
            "2 Nos",
            "50,000.00",
            "Output IGST",
            "1,18,000.00",
            "Rupees One Lakh Eighteen Thousand Only",
        ] {
            assert!(html.contains(needle), "missing {needle:?}");
        }
    }

    #[test]
    fn a_bank_payment_prints_as_a_cheque() {
        let (_f, mut slot) = indian_company("cheque-print");
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "100000");
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "5000");
        let rent = ledger(&mut slot, "Landlord <Mehta>", "Sundry Creditors", "");
        let paid = post_voucher(&mut slot, voucher("Payment", "2026-05-07", vec![entry(&rent, DrCr::Dr, "25500"), entry(&bank, DrCr::Cr, "25500")])).unwrap();
        let html = cheque_html(&slot, &paid.id, true).unwrap();
        for needle in ["Landlord &lt;Mehta&gt;", "25,500.00/-", "Rupees Twenty-Five Thousand Five Hundred Only", "A/c Payee", ">0<", ">7<", ">5<"] {
            assert!(html.contains(needle), "missing {needle:?}");
        }
        let by_cash = post_voucher(&mut slot, voucher("Payment", "2026-05-07", vec![entry(&rent, DrCr::Dr, "100"), entry(&cash, DrCr::Cr, "100")])).unwrap();
        assert!(cheque_html(&slot, &by_cash.id, false).unwrap_err().contains("bank"));
    }

    #[test]
    fn a_plain_voucher_prints_in_arabic_when_asked() {
        let (_f, mut slot) = indian_company("voucher-print-ar");
        let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "100000");
        let rent = ledger(&mut slot, "Landlord <Mehta>", "Sundry Creditors", "");
        let paid = post_voucher(&mut slot, voucher("Payment", "2026-05-07", vec![entry(&rent, DrCr::Dr, "25500"), entry(&bank, DrCr::Cr, "25500")])).unwrap();
        let html = voucher_html_in(&slot, &paid.id, PrintLanguage::Arabic, Some("20 ذو القعدة 1447 هـ")).unwrap();
        for needle in ["<html lang=\"ar\" dir=\"rtl\">", "<h2>سند صرف</h2>", "<bdi>Landlord &lt;Mehta&gt;</bdi>", "7-مايو-2026", "20 ذو القعدة 1447 هـ", "25,500.00", "المفوض بالتوقيع", "فقط خمسة وعشرون ألفاً وخمسمائة روبية هندية لا غير"] {
            assert!(html.contains(needle), "missing {needle:?}");
        }
        assert_eq!(voucher_html_in(&slot, &paid.id, PrintLanguage::English, None).unwrap(), voucher_html(&slot, &paid.id).unwrap());
    }

    #[test]
    fn a_gulf_invoice_stays_bilingual_in_arabic_and_takes_the_hijri_date() {
        let (_f, mut slot) = crate::api::tests::uae_company("invoice-print-ae");
        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Dubai Mart", &debtors, "", None, None, None).unwrap().ledger.id;
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let sold = post_voucher(&mut slot, voucher("Sales", "2026-05-02", vec![entry(&buyer, DrCr::Dr, "1050"), entry(&sales, DrCr::Cr, "1050")])).unwrap();
        let html = voucher_html_in(&slot, &sold.id, PrintLanguage::Arabic, Some("15 ذو القعدة 1447 هـ")).unwrap();
        assert!(html.contains("Tax Invoice</span> / <span lang=\"ar\" dir=\"rtl\">فاتورة ضريبية</span>"), "bilingual, as KSA and UAE invoices are");
        assert!(html.contains("<div>2-May-2026</div><div class=\"hijri\"><span lang=\"ar\" dir=\"rtl\">15 ذو القعدة 1447 هـ</span></div>"));
        // A UAE payment is not an invoice: Arabic only.
        let bank = ledger(&mut slot, "ENBD", "Bank Accounts", "5000");
        let paid = post_voucher(&mut slot, voucher("Payment", "2026-05-03", vec![entry(&sales, DrCr::Dr, "10"), entry(&bank, DrCr::Cr, "10")])).unwrap();
        assert!(voucher_html_in(&slot, &paid.id, PrintLanguage::Arabic, None).unwrap().contains("<h2>سند صرف</h2>"));
    }
}

# Multi-currency (foreign currency)

A business keeps its books in one **base currency** (the country pack's: INR,
AED, SAR, GBP, SGD…) and raises invoices to customers abroad, or takes bills
from suppliers abroad, in USD, EUR, AED and so on. This page covers how other
software models that, what the accounting standards and tax laws require,
and how KhataOS does it.

Nothing here is legal advice. Tags follow the research notes:
**[PRIMARY]** checked against the official text, **[SECONDARY]** reported by
professional sources, **[UNCERTAIN]** not confirmed. The sources behind the
tags are in `docs/research/mena-compliance.md`, `uk-compliance.md`,
`singapore-compliance.md`, `einvoicing-peppol.md` and `tally-gap-accounting.md`
(rows M17, V12, V13).

Code: `crates/khata-core/src/forex.rs` (the model, settlement and
revaluation), `voucher.rs`, `chart.rs` and `profile.rs` (the new optional
fields), `reports.rs` (both currencies in statements and outstandings),
`apps/desktop/src-tauri/src/forex.rs` (commands, voucher input, the invoice
print), `apps/desktop/src/forex-api.ts`, and the voucher, ledger, bills and
reports screens.

## 1. How others model it

Described from public help pages, in our own words.

**TallyPrime** (help.tallysolutions.com, "Multi-currency in TallyPrime"):

- A **currency master**: a symbol, a formal name, an ISO code, the number of
  decimal places, the word for the fractional part, and display options
  (symbol before or after, a space between). The base currency is set when
  the company is created.
- **Rates of exchange** are kept by date for each currency: a standard rate
  and, separately, the selling and buying rates actually used on vouchers.
  The last rate used is offered on the next voucher.
- A **ledger can be in a foreign currency** (parties and bank accounts, in
  practice). Its amounts are entered and shown in that currency, with the
  base equivalent beside them.
- On a **voucher**, a line in a foreign currency is entered as "amount @ rate
  = base amount"; the voucher balances in the base currency.
- **Bill-wise details keep the foreign amount**, so outstandings show what
  is open in the currency the bill was raised in.
- **Forex gain or loss**: when a foreign bill is settled at a rate other
  than the one it was raised at, the difference is the realised gain or
  loss. Users book it to a gain/loss ledger, in the settlement voucher or
  with an adjustment journal, and Tally reports what is still unadjusted.
  Year-end revaluation of open foreign balances (unrealised gain or loss) is
  a journal the user posts.

**Zoho Books, QuickBooks Online, Xero** follow the same shape: a currency
per contact and bank account, a rate per transaction (defaulting to a
stored or fetched rate), realised gain/loss posted automatically on payment
to an "Exchange Gain or Loss" account, and an optional period-end
revaluation (Xero's "currency revaluation", QuickBooks' "home currency
adjustment") posted as a journal.

## 2. Accounting standards

**IAS 21 / Ind AS 21** (The Effects of Changes in Foreign Exchange Rates),
for entities on IFRS or Ind AS:

- A foreign-currency transaction is recorded at the **spot rate on the date
  of the transaction** (an average rate for a period is allowed when rates
  do not move much).
- At each reporting date, **monetary items** (receivables, payables, foreign
  bank balances, loans) are translated at the **closing rate**; non-monetary
  items at historical cost stay at the transaction-date rate.
- **Exchange differences** on settlement, or on translating monetary items
  at a rate different from the one first recorded, go to **profit or loss**
  in the period they arise.

**AS 11** (Indian GAAP, for companies not on Ind AS) says the same for these
purposes: record at the transaction-date rate, report monetary items at the
closing rate, and recognise differences as income or expense in the period.
(AS 11's options for long-term foreign-currency monetary items, para 46/46A,
are not modelled.)

So KhataOS needs: the rate on each line (transaction date), the realised
difference on settlement, and the unrealised difference at a closing rate.

## 3. Tax-law points that change the design

| Where | Rule | Design consequence |
|---|---|---|
| India | GST value of a supply is in INR. CGST Rules, **Rule 34**: for **goods**, the rate of exchange is the one **notified by the Board (CBIC) under section 14 of the Customs Act** for the date of the time of supply; for **services**, the rate under generally accepted accounting principles on that date [SECONDARY]. An export invoice may be in foreign currency; GSTR-1 and the e-invoice report INR values. | Tax is worked out on the **base value** of each line (already how `tax.rs` works). The user enters the customs-notified rate on export invoices of goods (it is the voucher's rate); KhataOS does not fetch it. The print shows the tax and total in INR. |
| UAE | VAT Law **Art 69**: amounts in another currency are converted to AED at the **Central Bank (CBUAE)** rate [PRIMARY]. Executive Regulation **Art 59(1)**: a tax invoice in another currency shows the tax payable in AED and the exchange rate used (item (k)); the gross amount payable in AED [SECONDARY]. PINT AE: `TaxCurrencyCode` AED, the exchange rate (BTAE-04) and AED totals [PRIMARY]. | Print: rate, base total and tax in AED. Rate source: the user's (CBUAE rate). Whether every **line** must also show AED (Art 59(1)(h) as summarised) is [UNCERTAIN]. |
| KSA | VAT Implementing Regulations **Art 53**: the VAT amount on a tax invoice must be in **SAR** whatever the invoice currency [SECONDARY]. ZATCA XML: `TaxCurrencyCode` (BT-6) must be SAR and a second `TaxTotal` carries the VAT in SAR (BR-KSA-68) [PRIMARY]. Rate source (SAMA) [UNCERTAIN]. | Print: tax in SAR. `khata-sa/src/zatca.rs` writes SAR as both document and tax currency, so the app refuses a foreign-currency voucher before writing ZATCA XML (`forex::refuse_foreign_einvoice`); a USD document needs follow-up work (§6). |
| UK | VAT Regulations 1995 **reg 14(1)(l)**: the total VAT on a VAT invoice must be **expressed in sterling** [PRIMARY]. HMRC accepts the UK market selling rate at the time of supply, or its published period rates (VAT Notice 700) [SECONDARY]. | Print: VAT in GBP. |
| Singapore | An invoice may be in a foreign currency, but the **total excluding GST, the GST and the total including GST** must also be shown in **SGD**, at exchange rates IRAS accepts for GST (MAS or bank rates, used consistently; others with approval) [PRIMARY for the SGD totals]. PINT SG: `TaxCurrencyCode` SGD, a second `TaxTotal` and the `sgdtotal-*` document references (BR-113, BR-100 to 104) [PRIMARY]. | Print: total and GST in SGD (total excluding GST = the two's difference). PINT SG foreign currency is follow-up: the app refuses a foreign-currency voucher before writing PINT SG or PINT AE (`forex::refuse_foreign_einvoice`), and `khata-en16931/src/pint.rs` also refuses non-SGD invoices (BR-113-GST-SG). |
| EN 16931 / Peppol | **BT-5** is the invoice (document) currency; **BT-6** the VAT accounting currency. If BT-6 is given, **BT-111** (total VAT in the accounting currency) must be too (BR-53); in UBL a second `cac:TaxTotal` holds only `cbc:TaxAmount` in BT-6. | `khata-en16931` does not model BT-6/BT-111 yet (`model.rs` header), so the app refuses foreign-currency vouchers for EN 16931, XRechnung and Factur-X: follow-up. |

## 4. The KhataOS model

### Principles

1. **The books balance in the base currency.** A voucher line's `amount` is
   still the base amount and is what is posted. The foreign amount and the
   rate are extra data on the line.
2. **The audit trail is the storage format.** Every new field is optional,
   `#[serde(default, skip_serializing_if = …)]`, so a record without foreign
   data serialises exactly as before. The hash of an audit entry covers the
   JSON of `before`/`after` as stored (`audit.rs`, `entry_hash`); old
   entries are never re-serialised for hashing, and old records read back
   and written again produce the same JSON, so the next Alter's `before`
   check still matches.
3. **Country rules stay in packs.** The base currency and its decimals come
   from `CompliancePack::currency()`. The core knows ISO codes and
   arithmetic, nothing about which country asks for what.
4. **No outbound calls.** Rates are typed in and remembered. A
   `RateSource` trait (`apps/desktop/src-tauri/src/forex.rs`) is the hook for
   an online source; one must ask the user first and store its rates with
   `fx_set_rate`, so they are in the trail like any other change.

### Data

| Where | Field | Meaning |
|---|---|---|
| `forex::Currency` | `code`, `symbol`, `name`, `decimals` | The currency master: 43 built-in ISO 4217 currencies (`forex::builtin`), ISO decimals (JPY 0, KWD/BHD/OMR/JOD 3). |
| `Profile.currencies` | code → `Currency` | The company's own currencies or changes to built-in ones (`Book::save_currency`). |
| `Profile.fx_rates` | code → date → rate | The company's rate table (`Book::set_fx_rate`): base units per unit. |
| `Ledger.currency` | `Option<String>` | A party, cash or bank ledger kept in a foreign currency. A line on it may only carry that currency; base-only lines (gain/loss, revaluation) are allowed. Changing it is refused once it has lines in another currency. |
| `Entry.forex` | `Option<Forex { currency, amount, rate }>` | The line's foreign amount and rate. |
| `BillAllocation.forex_amount` | `Option<Amount>` | The allocation in the line's currency. On a line with `forex`, all or none of the bills have one, and they add up to it. |

**Remembered rates.** `Book::fx_rate(code, date)` returns the latest rate on
or before the date, from the rate table or from voucher lines in that
currency (the last one entered on the day); on the same date the table
wins. The voucher screen offers it as the default.

**Checks** (in `Book::apply_entry`, so they hold on replay, sync and the
server): ISO-shaped codes; positive foreign amounts and rates; bills' foreign
amounts all-or-none and adding up (`VoucherError::Forex`); a line's currency
matching its ledger's; a ledger not switched to a currency its lines don't
have; profile currencies and rates well formed (`BookError::Currency`). The
core does **not** require `amount == foreign × rate`: a settlement line is
booked at the bill's carrying amount, and a balancing line may differ from
the product by rounding.

### Realised gain or loss (settlement)

`forex::settle(book, &mut voucher, gain_loss_ledger, base_decimals)`, called
by the desktop on every post and alteration (`forex::settle_voucher`):

1. For each Agst Ref allocation with a foreign amount, on a bill first raised
   with a foreign amount in the same currency, the **carrying amount** is
   the bill's open base amount in proportion to the foreign amount settled
   (all of it when the bill is settled in full, so it closes exactly).
2. The allocation and its line are set to the carrying amount. The line
   keeps the voucher's rate, as typed.
3. The difference from what the line was entered at (foreign × the
   voucher's rate) goes to the **Forex Gain/Loss** ledger as one base-only
   line in the same voucher: a credit for a gain, a debit for a loss. It is
   merged with any base-only line on that ledger, so settling twice changes
   nothing more.

This is what Zoho, QuickBooks and Xero do, and what Tally users do by hand
in the receipt or payment: the party's bill closes in both currencies and
the gain or loss is in the same voucher. The desktop creates a ledger named
"Forex Gain/Loss" under Indirect Expenses the first time a gain or loss
arises (a company can rename its own; KhataOS looks it up by that name).

Example: invoice EXP-1, $1,000 at 80 = ₹80,000. Received $400 at 78.50 =
₹31,400: the party is credited ₹32,000 (the carrying amount), Forex Gain/Loss
is debited ₹600 (a loss), the bank is debited ₹31,400. EXP-1 stays open at
$600 / ₹48,000. The rest at 81 = ₹48,600 closes it with a ₹600 gain.

When altering a settlement, the voucher screen leaves the old gain line out
(it is marked `forex_gain` in `get_voucher`) and shows the foreign lines at
the voucher's rate; saving works the gain out again.

### Unrealised gain or loss (revaluation)

`forex::revaluation(book, as_at, rates, base_decimals)` restates the foreign
balance of every ledger kept in a foreign currency at a closing rate (given,
or the remembered rate on that date) and returns the difference per ledger.
`forex::revaluation_journal` turns that into a Journal against Forex
Gain/Loss. Nothing is posted by itself: the desktop's `fx_revaluation`
command shows the suggestion, and `fx_post_revaluation` posts it when the
user says so. Common practice is to reverse it on the first day of the next
period; KhataOS does not reverse automatically. The revaluation line is
base-only and not bill-wise, so in outstandings it appears as On Account.

## 5. Desktop

**Ledger master.** Party, cash and bank ledgers have a **Currency** field
(blank: the company's own). It is saved with `fx_set_ledger_currency`.

**Voucher entry** (`screens/voucher.ts`). When a ledger on the voucher is
kept in a foreign currency, a **Rate of exchange** field ("1 USD = … INR")
appears right after that ledger, filled with the last rate used. Amounts on
that ledger's lines are typed in its currency, and the base amount follows
live (shown under the line). Other lines stay in the base currency, as in
Tally. An item invoice to a foreign party shows the party's foreign total.
One foreign currency per voucher. On saving:

- foreign lines send their foreign amount and the rate;
- on Sales, Purchase, Credit Note and Debit Note every other line sends its
  foreign equivalent (base ÷ rate), and a derived party line (item invoice)
  the sum of them, so the invoice adds up in the party's currency;
- bills' foreign amounts are shared out by base amount (`entry_forex`);
- a bill picked in Agst Ref defaults to its foreign pending amount at the
  voucher's rate.

**Reports.** Ledger Vouchers adds an "In USD" column (opening, every row,
closing) for a ledger kept in a foreign currency. Outstandings add a
"Pending (foreign)" column when any bill has a foreign amount; the Agst Ref
list shows it too.

**Invoice print.** `forex::invoice_currency` (called from
`invoice::invoice_doc`) restates a Sales/Credit/Debit Note to a party kept
in a foreign currency in that currency: lines and taxes from the lines'
foreign amounts (or at the rate), the total is the party's foreign amount,
amounts in words in the currency, and the `CurrencySection` carries the
rate, the base total and, in India, UAE, KSA, UK and Singapore, the tax in
the base currency.

**Commands** (`forex::cmd`): `fx_list_currencies`, `fx_save_currency`,
`fx_set_ledger_currency`, `fx_rate`, `fx_set_rate`, `fx_revaluation`,
`fx_post_revaluation`.

## 6. Not done yet

- **ZATCA XML in a foreign currency**: `DocumentCurrencyCode` USD with
  amounts in USD, `TaxCurrencyCode` SAR, and the second `TaxTotal` with the
  VAT in SAR (BR-KSA-68). Today `khata-sa/src/zatca.rs` writes SAR for both,
  so the app refuses a foreign-currency voucher for ZATCA
  (`forex::refuse_foreign_einvoice` in `zatca.rs`).
- **EN 16931 / Peppol** (`khata-en16931`): BT-6 and BT-111 (a second
  `TaxTotal`), PINT AE's exchange rate (BTAE-04) and AED totals, PINT SG's
  SGD `TaxTotal` and `sgdtotal-*` references. Until then the app refuses
  foreign-currency vouchers for PINT SG, PINT AE, EN 16931 / Factur-X and
  XRechnung (`forex::refuse_foreign_einvoice`).
- **Mexico CFDI and Spain VeriFactu** don't refuse foreign-currency
  vouchers; whether their output is right for one hasn't been checked.
- **India IRP e-invoice and e-way bill**: values stay in INR (correct); the
  export details' foreign currency code (`ExpDtls.ForCur`) is not filled.
- **Tally import** of currencies, ledger currencies and forex voucher lines.
- **Opening balances in a foreign currency** (a foreign amount on a ledger's
  opening balance and opening bills).
- **Advances in a foreign currency** settled against a later invoice (the
  advance's rate as the carrying rate).
- A **Rates of Exchange screen** and a **Revaluation screen** in the desktop
  (the commands exist; the period-end flow is through the API for now).
- Separate buying and selling rates (Tally keeps both); KhataOS keeps one
  rate per currency per date plus whatever each voucher used.

## 7. Open questions

- India Rule 34: confirm the current text for services (GAAP rate) and that
  the customs-notified rate for goods is the export rate (CBIC publishes
  separate import and export rates). [UNCERTAIN]
- UAE ER Art 59(1)(h) and (j): whether every line, and the gross total, must
  be shown in AED, or only the tax (item (k)). [UNCERTAIN]
- KSA: the exchange-rate source (SAMA) and whether the total, not only the
  VAT, must be in SAR. [UNCERTAIN]
- Singapore: the print shows the total and the GST in SGD; the total
  excluding GST is their difference. Confirm a print helper shows all three.
- Revaluation journals: whether to offer an automatic reversal on the first
  day of the next period.

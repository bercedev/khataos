# Netherlands btw pack (`khata-nl`)

KhataOS can keep books for a Dutch business. It checks the
btw-identificatienummer and charges 21%, 9% or 0% btw. It handles "btw
verlegd" (reverse charge), checks invoice numbers, and works out the
aangifte omzetbelasting from the books.

Nothing is sent to the Belastingdienst. Nothing here is tax advice: check
the figures with your accountant before you file.

Code:
- `crates/khata-nl` holds the rules: `btw_id.rs`, `provincies.rs`,
  `btw.rs`, `returns.rs`.
- `apps/desktop/src-tauri/src/vat_nl.rs` turns vouchers into aangifte
  documents and provides the `aangifte_nl(from, to)` command. It reads
  vouchers with the shared reader in `vat_fr.rs`.
- `apps/desktop/src/screens/vat_nl.ts` provides `AangifteReport`, in Go To
  (Alt+G) as **Aangifte omzetbelasting (Netherlands VAT return)** for Dutch
  companies.

Sources and open questions are in
[`docs/research/netherlands-compliance.md`](research/netherlands-compliance.md).
Tags:
- **[PRIMARY]**: read in the Wet OB 1968, the AWR or the Belastingdienst's
  Toelichting.
- **[SECONDARY]**: from python-stdnum or other non-official sources.
- **[UNCERTAIN]**: not confirmed.

## Setting up a Dutch company

1. Pick **Netherlands** as the country. The books use EUR and **calendar
   years**. The boekjaar is the calendar year unless the statutes say
   otherwise (BW 2:10a).
2. In **Company → Company details** (Alt+K), enter the **btw-id** and pick the
   **province**: `DR`, `FL`, `FR`, `GE`, `GR`, `LI`, `NB`, `NH`, `OV`, `UT`,
   `ZE`, `ZH`. These are ISO 3166-2:NL without "NL-". The province never
   changes the rate. Bonaire, Sint Eustatius and Saba are outside the Wet
   OB (they have the ABB) and count as abroad.
3. Add the rate list (also in `khata_nl::btw::default_rates`):

   | Class | Rate | From | Notes |
   |---|---|---|---|
   | `STANDARD` | 21% | 1 Oct 2012 | 19% from 2001 in the list [PRIMARY] |
   | `REDUCED` | 9% | 1 Jan 2019 | 6% before [PRIMARY] |
   | `ZERO` | 0% | | Tabel II |
   | `EXEMPT` | 0% | | art. 11 vrijstellingen, and the KOR |
   | `REVERSE` | 0% | | "btw verlegd" (art. 12 lid 2–3, art. 24ba) |

   Two 2026 changes [PRIMARY]:
   - **Logies** (hotels and holiday homes) moved from 9% to 21% on 1 Jan
     2026. Give those items a `STANDARD` class, or a class such as
     `LOGIES` with 9% then 21% from 2026-01-01.
   - Culture, media and sport stay at 9%: the planned increase was
     withdrawn.

## Tax identifiers

The btw-id is `NL`, nine digits, `B` and two digits. A number is accepted
when either check passes, as python-stdnum does:
- **MOD 97-10** (the btw-id given to sole traders since 2020): ISO 7064
  over the whole string, letters read as numbers (N = 23, L = 21, B = 11).
  The remainder must be 1.
- **Elfproef** (the RSIN-based omzetbelastingnummer of legal entities): 9·d1
  + 8·d2 + … + 2·d8 − d9 ≡ 0 (mod 11).

Both algorithms are [SECONDARY]. The Belastingdienst publishes neither, but
all known real numbers pass. Test vectors:
- elfproef: NL004495445B01;
- MOD 97: NL002455799B11, NL000099998B57;
- invalid: NL002455799B12, NL004495446B01.

The digits after B cannot be 00. Other EU VAT identifiers are accepted on
their shape only.

## Invoices

- **Numbers**: "een opeenvolgend nummer, met één of meer reeksen, waardoor
  de factuur eenduidig wordt geïdentificeerd" (art. 35a lid 1 b)
  [PRIMARY].
  - The pack requires a number to be unique across all years, at most 35
    characters of `[A-Za-z0-9/-]`.
  - KhataOS's gap-free series per voucher type make the numbers
    "opeenvolgend". Whether gaps would breach the law is [UNCERTAIN].
- A REVERSE invoice must say **"btw verlegd"** and show the customer's
  btw-id; an exempt one must indicate the exemption. The btw amount must
  be in EUR.
- Records are kept for 7 years (art. 52 AWR).

## Aangifte omzetbelasting

The desktop's `aangifte_nl(from, to)` reads Sales and Credit Notes as
outward and Purchases and Debit Notes as inward. Tax ledgers of type "BTW"
(or "VAT") give the btw. `khata_nl::returns::aangifte` turns them into
rubrieken:

| Rubriek | Label | What goes in it |
|---|---|---|
| 1a / 1b | Leveringen/diensten belast met hoog / laag tarief | 21% / 9% sales: omzet and btw |
| 1c | … overige tarieven, behalve 0% | Other rates (an old 19% or 6% on a credit note) |
| 1d | Privégebruik | not worked out (0) |
| 1e | … belast met 0% of niet bij u belast | 0% sales and "btw verlegd" sales to Dutch buyers |
| 2a | … waarbij de btw naar u is verlegd | REVERSE purchases from Dutch suppliers, btw self-assessed |
| 3a | Leveringen naar landen buiten de EU (uitvoer) | Goods exported |
| 3b | Leveringen naar of diensten in landen binnen de EU | Goods to EU buyers with a VAT id, services to EU businesses (the ICP lines) |
| 3c | Installatie/afstandsverkopen binnen de EU | not worked out (0) |
| 4a / 4b | Leveringen/diensten uit landen buiten / binnen de EU | Purchases from abroad without btw, self-assessed |
| 5a | Verschuldigde btw | Sum of the btw column |
| 5b | Voorbelasting | Input btw plus the self-assessed btw of 2a, 4a and 4b, all taken as deductible |
| 5c, 5g | Subtotaal, totaal | 5a − 5b; negative means to reclaim |

How the figures are shaped:
- **Exempt supplies** are not on the aangifte [PRIMARY].
- **Services to customers outside the EU** are not on it either.
- **Rounding** [PRIMARY]: "Rond alle bedragen af op hele euro's. Dit mag u
  in uw voordeel doen." Omzet and btw due are rounded down, voorbelasting
  up.
- **5d (KOR), 5e and 5f (estimates)** stay at 0. Whether the live form
  still has them is [UNCERTAIN]; 5g = 5c either way.
- **Filing**: quarterly by default, due by the last day of the next month.
  File through Mijn Belastingdienst Zakelijk; SBR / Digipoort filing is
  future work.

Not worked out: privégebruik, the OSS, import VAT and the art. 23 licence,
partial deduction, suppleties, and the ICP-opgaaf (the 3b lines by
customer).

## KOR (kleineondernemersregeling)

A company with turnover of at most €20,000 in both the current and the
previous calendar year can choose the KOR. Under it, the company:
- charges no btw;
- deducts none;
- files no aangifte.

Give its lines the `EXEMPT` class. The 2025 changes (no 3-year minimum,
the EU-KOR) are in the research. The pack does not track the threshold.

## E-invoicing

There is no Dutch B2B mandate yet. The cabinet plans domestic B2B
e-invoicing from 1 Jul 2030; it is not law yet. The shared
`khata-en16931` crate can write Peppol BIS Billing 3.0, but the app offers
no Peppol BIS or NLCIUS export yet: a saved Dutch invoice's Export menu
(Ctrl+E) has **Factur-X / ZUGFeRD PDF** only. In Peppol, the Dutch rules
NL-R-001 to 009 require a KVK or OIN for Dutch suppliers.

## Next steps

- SBR / Digipoort filing, the ICP-opgaaf, suppleties.
- A KOR flag and turnover tracking; KVK number on the profile and invoices.

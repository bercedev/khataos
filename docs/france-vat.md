# France TVA pack (`khata-fr`)

KhataOS can keep books for a French business. It checks the numéro de
TVA intracommunautaire, the SIREN and the SIRET. It charges TVA at 20%,
10%, 5.5% or 2.1%, and at the DOM rates in Guadeloupe, Martinique and La
Réunion. It handles autoliquidation, checks invoice numbers, and works out
the CA3 (déclaration 3310-CA3) from the books.

Nothing is sent to impots.gouv.fr, and nothing goes to a plateforme agréée.
Nothing here is tax advice: check the figures with your expert-comptable
before you file.

Code:
- `crates/khata-fr` holds the rules: `siren.rs`, `regions.rs`, `tva.rs`,
  `returns.rs`.
- `apps/desktop/src-tauri/src/vat_fr.rs` turns vouchers into CA3 documents
  and provides the `ca3_fr(from, to)` command. It also holds the voucher
  reader shared with the Netherlands and Spain (`vat_fr::eu`).
- `apps/desktop/src/screens/vat_fr.ts` provides `Ca3Report`, in Go To
  (Alt+G) as **CA3 (France VAT return)** for French companies (see
  [Where it is in the app](#where-it-is-in-the-app)).

Sources and open questions are in
[`docs/research/france-compliance.md`](research/france-compliance.md).
Tags:
- **[PRIMARY]**: read in BOFiP, an impots.gouv.fr form or notice, or the
  DGFiP's e-invoicing specifications.
- **[SECONDARY]**: reported by other sources (service-public, python-stdnum,
  Wikipedia).
- **[UNCERTAIN]**: not confirmed.

## Setting up a French company

1. Pick **France** as the country. The books use EUR and **calendar
   years**. The exercice comptable is any 12 months; change the books'
   first month if yours differs.
2. In **Company → Company details** (Alt+K), enter the **numéro de TVA** and pick
   the **région**. Région codes are the ISO 3166-2:FR codes without
   "FR-":
   - metropolitan: `ARA`, `BFC`, `BRE`, `CVL`, `20R` (Corse), `GES`,
     `HDF`, `IDF`, `NOR`, `NAQ`, `OCC`, `PDL`, `PAC`;
   - overseas: `971` Guadeloupe, `972` Martinique, `973` Guyane, `974` La
     Réunion, `976` Mayotte.

   Régions, not départements, are the address regions, because they are
   what changes the TVA (see the next section). The old codes FR-GP …
   FR-YT were withdrawn from ISO 3166-2 in 2021 [SECONDARY].
3. Add the rate list (also in `khata_fr::tva::default_rates`):

   | Class | Rate | From | Notes |
   |---|---|---|---|
   | `STANDARD` | 20% | 1 Jan 2014 | 19.6% from 2012 in the list [rates PRIMARY, dates SECONDARY] |
   | `INTERMEDIATE` | 10% | 1 Jan 2014 | 7% in 2012–2013 |
   | `REDUCED` | 5.5% | | food, energy-saving works, books … |
   | `SPECIAL` | 2.1% | | reimbursable medicines, press … |
   | `DOM-STANDARD` | 8.5% | | Guadeloupe, Martinique, La Réunion (art. 296 CGI) |
   | `DOM-REDUCED` | 2.1% | | the same |
   | `ZERO`, `EXEMPT`, `REVERSE` | 0% | | `EXEMPT…`: exonéré (art. 261 ff.); `REVERSE…`: autoliquidation |

   A class starting with `EXEMPT` is exempt and one starting with `REVERSE`
   is autoliquidated. Any other class at 0% is zero-rated; any positive
   rate is taxed.

   The 2025 and 2026 finance laws changed no rate [PRIMARY: the 2026 CA3
   form]. The franchise en base thresholds are unchanged for 2026: €85,000
   for goods and €37,500 for services, with majoré limits of €93,500 and
   €41,250. The single €25,000 threshold was abandoned [PRIMARY:
   service-public F21746].

## Territories

`tva::line_tax` picks the rate from the supplier's région:

| Supplier in | Rate used |
|---|---|
| Metropolitan France, Corse included | The class's rate. Corse's own rates (art. 297: 13%, 10%, 2.1%, 0.9%) are classes you add (`CORSE-13` …) |
| Guadeloupe, Martinique, La Réunion | The `DOM-` class first: `STANDARD` becomes `DOM-STANDARD`, 8.5%. A class with no `DOM-` rate is an error, never a silent fallback to the metropolitan rate |
| Guyane, Mayotte | No TVA: "provisoirement pas applicable" (art. 294-1) [PRIMARY: BOFiP] |
| Metropolitan France ↔ overseas, or between two overseas régions | 0%: an export or import (art. 294-2). Between two DOM is [UNCERTAIN] |
| Abroad | 0% (intra-EU supply, export or not taxable in France) |

Monaco counts as France: a party there is not abroad.

## Tax identifiers

- **Numéro de TVA**: `FR`, a two-character key and the SIREN.
  - The numeric key is `(12 + 3 × (SIREN mod 97)) mod 97`
    [SECONDARY: python-stdnum; no official page states the formula].
  - Keys with letters (alphabet without I and O) are checked with
    python-stdnum's rules [SECONDARY, UNCERTAIN].
  - A body starting `000` is a Monaco number. The SIREN check is skipped
    for it.
  - Test vectors: FR40303265045, FR44732829320, FRK7399859412,
    FR4Z123456782, FR32000000330 (Monaco).
- **SIREN**: nine digits with a Luhn check. **SIRET**: the SIREN plus a
  five-digit NIC, Luhn over all 14 digits. For La Poste (SIREN 356000000),
  the digit sum must be a multiple of 5 instead [SECONDARY].
  - The pack accepts a SIREN or SIRET as a party's tax id when the party
    has no TVA number. The SIREN is the `entity_id`.
  - The SIREN is also the e-invoicing address in the PPF annuaire.
- **Other EU VAT identifiers** are accepted on their shape only.

A pass shows that a number is well formed. Only VIES can say that it is
registered.

## Invoices

- **Numbers** are "unique, basé sur une séquence chronologique et
  continue" (BOI-TVA-DECLA-30-20-20-10 §70) [PRIMARY].
  - Several series are allowed; KhataOS numbers each voucher type in its
    own gap-free series.
  - The pack also requires a number to be unique across all years, at
    most 35 characters of `[A-Za-z0-9/-]`. 35 matches the e-invoicing rule
    G1.42 (number ≤ 35, unique per year and SIREN).
- **Mentions**: the printed and e-invoice must carry the fixed notes:
  - "Autoliquidation" on a REVERSE line;
  - "TVA non applicable - article 293 B du CGI" under the franchise.

  Since the reform, invoices also carry four new mentions: the customer's
  SIREN, the operation category (goods, services or both), the option
  "TVA sur les débits", and the delivery address. When these apply by
  issuing wave is [UNCERTAIN] (see research §3).

  The invoice layouts belong to the print and e-invoice code, not this pack.

## E-invoicing (facturation électronique)

The reform's dates [PRIMARY: DGFiP FAQ, 1 Sep 2026]:
- **Receiving**: every business must receive e-invoices through a
  plateforme agréée (PA, formerly PDP) from **1 Sep 2026**.
- **Issuing and e-reporting** (B2C, international operations, payment
  data): large companies and ETI from **1 Sep 2026**; PME and
  micro-entreprises from **1 Sep 2027**.

The PPF runs the annuaire and collects the data; it is no longer a free
invoicing platform.

Accepted formats are UBL 2.1, UN/CEFACT CII and **Factur-X**, all on EN
16931 with the French rules (AFNOR XP Z12-012). KhataOS's French
e-invoices come from the shared `khata-en16931` crate (its Factur-X and
CII writers), not from this pack. Connecting to a PA and e-reporting are
future work.

## CA3

`khata_fr::returns::ca3(outward, inward, period, territory)` works out the
CA3 from a neutral document model. The desktop's `ca3_fr(from, to)` feeds
it as follows:
- outward: Sales and Credit Notes; inward: Purchases and Debit Notes;
- tax ledgers of type "TVA" (or "VAT");
- the company's région picks the rate lines: Continent, Corse or DOM.
  There is no CA3 in Guyane or Mayotte.

| Line | What goes in it |
|---|---|
| A1 | Taxable sales (HT) |
| A2 | Purchases autoliquidated inside France (the buyer's side of a REVERSE purchase) |
| A3 | Services bought from abroad (EU or not), autoliquidated (art. 283-2) |
| B2 | Intra-EU acquisitions of goods |
| B5 | The company's credit notes on taxable sales (a positive figure); their TVA goes in 21 |
| E1 | Exports outside the EU (and to or from the overseas régions) |
| E2 | Exempt and 0% supplies, services to customers abroad, the seller's side of an autoliquidation |
| F2 | Intra-EU supplies of goods to a buyer with a VAT id |
| F8 | Credit notes on non-taxable operations |
| 08 / 09 / 9B / T6 | 20% / 5.5% / 10% / 2.1% in continental France: base and tax |
| TC / T3 / T4 / T5 | Corse 13% / 10% / 2.1% / 0.9% (with 08 and 09 for 20% and 5.5%) |
| 10 / 11 / T1 / T2 | DOM 8.5% / 2.1% / 1.75% / 1.05% |
| 13 | Anciens taux (19.6%, 7%) |
| 15 | TVA antérieurement déduite à reverser (suppliers' credit notes) |
| 16 / 17 | Total TVA brute; dont TVA sur acquisitions intracommunautaires |
| 20 / 21 / 23 | Deductible TVA: purchases plus the self-assessed TVA; the TVA of the company's credit notes; total |
| 25 / 27 or TD / 28 | Crédit de TVA and crédit à reporter, or TVA due and TVA nette due |
| 32 | Total à payer |

Rules from the notice 3310-NOT-CA3-SD [PRIMARY]:
- Every box is rounded to the nearest euro (0.50 counts as 1). A rate
  line's tax is its rounded base × rate; KhataOS warns when the TVA booked
  differs by more than €1.
- **No negative amounts**: credit notes go to the régularisation lines (B5,
  F8, 15, 21). A box that still comes out negative gets a warning.
- The self-assessed TVA (A2, A3, B2) is in its rate line and again in line
  20.
- Filing is monthly (quarterly below €4,000 of TVA a year) and online only
  (EFI or EDI). The screen's figures are for keying into EFI. EDI is
  future work.

Not worked out (left at zero):
- imports (A4, I1–I6), B4, fixed assets (19), the credit carried forward
  from the last return (22), refund claims (26), taxes assimilées (29) and
  the energy excise lines;
- TVA sur les encaissements timing, partial deduction, the CA12 and the
  DEB / DES.

All input TVA is taken as deductible.

## Where it is in the app

- **CA3:** Go To (Alt+G) → **CA3 (France VAT return)**, shown only for
  French companies (`launcher.ts`). F2 changes the period; Ctrl+P prints and
  Ctrl+E saves a CSV.
- **Factur-X:** on a saved Sales invoice or Credit Note, Ctrl+E (Export) →
  **Factur-X / ZUGFeRD PDF** (`einvoice.ts` `exportVoucher`, built by
  `peppol.rs` `facturx_pdf` and `khata_en16931::facturx`). It is the
  EN 16931 profile, laid out as plain text rather than the invoice design,
  and it has not been run through a PDF/A validator. The four new invoice
  mentions are not printed yet.

## Next steps

- Company profile fields for the SIREN, the TVA-on-debits option and
  the franchise flag; the four new invoice mentions on print.
- Factur-X with the French CTC-FR profile; a PA connection; e-reporting.
- EDI-TVA filing, the CA12, the credit carried forward from the last
  return, the DEB and DES, and the FEC export.

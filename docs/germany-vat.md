# Germany USt pack (`khata-de`)

KhataOS can keep books for a German business: it checks the USt-IdNr and
the Steuernummer, charges 19%, 7% or 0% USt on invoices (with the 16% / 5%
of the second half of 2020), handles the §13b reverse charge, checks
invoice numbers, works out the UStVA (Umsatzsteuer-Voranmeldung) from the
books, and writes XRechnung 3.0 invoices (UBL or CII) through the shared
EN 16931 crate ([`docs/en16931.md`](en16931.md)). Nothing is sent to ELSTER
or to a buyer. Nothing here is tax advice: check the figures with your
Steuerberater before you file.

Code: `crates/khata-de` (the rules: `ustid.rs`, `steuernummer.rs`,
`laender.rs`, `ust.rs`, `returns.rs`, `xrechnung.rs`, `leitweg.rs`),
`apps/desktop/src-tauri/src/vat_de.rs` (vouchers to UStVA documents and
XRechnung invoices; the `ustva(from, to)` and
`xrechnung_xml(voucher_id, syntax)` commands) and
`apps/desktop/src/screens/vat_de.ts` (`UstvaReport`, `exportXRechnung`);
see [Where it is in the app](#where-it-is-in-the-app). Sources and
open questions are in [`docs/research/germany-compliance.md`](research/germany-compliance.md).
Tags: **[PRIMARY]** read in the law, a BMF letter, ELSTER or KoSIT
material; **[SECONDARY]** reported by other sources; **[UNCERTAIN]** not
confirmed.

## Setting up a German company

1. Pick **Germany** as the country. The books use EUR (2 decimals, "€")
   and **calendar years**: the Wirtschaftsjahr is the calendar year unless
   a business in the Handelsregister chooses another (§4a Abs. 1 EStG).
2. In **Company → Company details** (Alt+K), enter the **USt-IdNr** (or the
   Steuernummer, when the company has no USt-IdNr) and pick the
   **Bundesland** (ISO 3166-2:DE without "DE-": `BW`, `BY`, `BE`, `BB`,
   `HB`, `HH`, `HE`, `MV`, `NI`, `NW`, `RP`, `SL`, `SN`, `ST`, `SH`, `TH`).
   The Land never changes the rate. A 13-digit Steuernummer gives its own
   Land.
3. Add the rate list (also in `khata_de::ust::default_rates`):

   | Class | Rate | From | Notes |
   |---|---|---|---|
   | `STANDARD` | 19% | 1 Jan 2007 | 16% from 1 Jul to 31 Dec 2020 (§28 Abs. 1 UStG) [PRIMARY] |
   | `REDUCED` | 7% | 1 Jan 2007 | 5% from 1 Jul to 31 Dec 2020 (§28 Abs. 2) [PRIMARY]; restaurant food is 7% from 1 Jan 2026 (§12 Abs. 2 Nr. 15) [PRIMARY] |
   | `ZERO` | 0% | | taxable at 0% (§12 Abs. 3, photovoltaics) |
   | `EXEMPT` | 0% | | tax-free under §4 (Nr. 8 ff.) |
   | `REVERSE` | 0% | | §13b: the recipient owes the tax |

   Names matter: a class starting with `EXEMPT` is tax-free, `REVERSE` is
   reverse-charged, `REDUCED` is reduced; any other 0% class is zero-rated.
4. Put the contact and bank details on extra lines of the **company
   address** (until the profile has fields for them); XRechnung needs them:

   ```
   Hauptstraße 1
   10115 Berlin
   Tel.: +49 30 123456
   E-Mail: rechnung@muster.de
   Kontakt: Erika Muster
   IBAN: DE02 1203 0000 0000 2020 51
   BIC: BYLADEM1001
   ```

## Tax identifiers

- **USt-IdNr**: `DE` and nine digits; the ninth is an ISO 7064 MOD 11,10
  check digit over the first eight [SECONDARY: Wikipedia de,
  python-stdnum; ELSTER publishes the same algorithm for the
  Steuer-IdNr, PRIMARY]. The prefix may be left out. A number starting
  with 0 is refused, as python-stdnum does [UNCERTAIN: no official text].
  A pass shows the number is well formed; only the BZSt (eVatR) or VIES
  say it is valid.
- **Steuernummer**: the printed form (10 or 11 digits with the state's
  grouping) or ELSTER's 13-digit form `FFFF0BBBUUUUP` (NRW
  `FFFF0BBBBUUUP`). The 13-digit form is checked fully: state prefix,
  the 0 in position 5, the Bezirk rules and the state's check digit (11er,
  NRW, modified 11er for Rheinland-Pfalz, 2er for BW/HE/SH), from ELSTER's
  *Prüfung der Steuer- und Steueridentifikationsnummer* (Stand
  02.09.2026) [PRIMARY]. Berlin's choice of method A or B per Finanzamt is
  not implemented: either is accepted. A printed number is accepted when
  it fits some state's form and check digit (the Land is given only when
  exactly one state fits); `steuernummer::to_elster(id, land)` converts.
- **Other EU VAT identifiers** (for EU customers and suppliers) are
  accepted on their shape only: an EU prefix (`EL` for Greece) and 2 to 12
  letters or digits.
- **Leitweg-ID** (the Buyer reference of a public buyer):
  `leitweg::validate` checks the shape and the MOD 97-10 check number
  (Format-Spezifikation 2.0.2) [PRIMARY].

## Invoices

- Invoice and credit-note numbers must be given **once** (§14 Abs. 4
  Nr. 4: "eine fortlaufende Nummer … die … einmalig vergeben wird")
  [PRIMARY]: KhataOS refuses a number used before in the series in any
  year (case-insensitive). At most 35 characters of letters, digits, `-`
  and `/`: a KhataOS choice, not law.
- USt is one "USt" component per line, rounded to the cent, half away from
  zero (no rounding rule in the UStG [UNCERTAIN]). A REVERSE line is 0%
  whatever its rate on file; a supply abroad is 0%.
- Duty ledgers for USt use the tax type **USt** (`VAT` is also read).

## UStVA

`ustva(from, to)` works out the Kennzahlen of form USt 1 A (2025 and 2026
forms, BMF letters of 9 Dec 2024 and 29 Dec 2025) [PRIMARY] for a month or
a quarter. Bases are whole euros, taxes euros and cents, as the form asks
[PRIMARY]; cents are dropped (toward zero) rather than rounded
[UNCERTAIN: the form does not say]. The tax on Kz 81 and 86 is base ×
rate, as the form computes it; a warning appears when the USt booked
differs by more than €1.

| Kz | What | From |
|---|---|---|
| 81 / 86 | taxable at 19% / 7% (base; tax computed) | Sales and Credit Notes |
| 35 / 36 | other rates (16% / 5% of 2H 2020, corrections): base and tax booked | |
| 87 | taxable at 0% (§12 Abs. 3) | `ZERO` class |
| 41 | tax-free intra-EU supplies of goods to buyers with a VAT id | goods to an EU party with a VAT id, no USt |
| 43 | other tax-free supplies with input-tax deduction (exports) | goods to a party outside the EU, no USt |
| 48 | tax-free without input-tax deduction | `EXEMPT` class |
| 60 | supplies where the recipient owes the tax (§13b) | `REVERSE` class |
| 21 | services taxable in another EU state (§18b) | services to an EU party with a VAT id, no USt |
| 45 | other supplies not taxable in Germany | services to a party outside the EU |
| 89 / 93 | intra-EU acquisitions at 19% / 7% (tax computed) | goods from an EU supplier, no USt |
| 46 / 47 | services from EU businesses (§13b Abs. 1) | services from an EU supplier, no USt |
| 84 / 85 | other §13b supplies received | `REVERSE` purchases, services from outside the EU |
| 66 | input tax from invoices | USt booked on Purchases and Debit Notes |
| 61 / 67 | input tax on acquisitions / on §13b | the self-assessed tax, deducted again |
| 83 | remaining prepayment (positive) or refund (negative) | output tax − input tax |

Where a party is comes from its ledger's mailing country (empty or `DE`:
Germany). A sale or purchase with USt booked is always domestic.
Reverse-charge and acquisition tax is self-assessed at the class's rate,
or the STANDARD rate when the class is 0%.

Not worked out: Kz 42 (triangular trade), 44/49 (new vehicles), 73/74,
76/80, 95/98, 94/96, 90/91, 62 (import VAT: goods bought from outside the
EU get a warning), 59, 63, 64, 65, 69, 39 (Sondervorauszahlung), 37, 10,
22, 26, 29, 70 and 500; Istversteuerung; partial input-tax deduction;
§15a; the Zusammenfassende Meldung; the annual return.

**Filing** [PRIMARY, §18 UStG]: quarterly by default; monthly when the
previous year's tax was more than €9,000; exemption possible up to €2,000;
due on the 10th day after the period (one month later with
Dauerfristverlängerung). Only electronically through ELSTER: third-party
software uses ERiC, a free C library that needs a Hersteller-ID. KhataOS
does not submit (see [Next steps](#next-steps)).

## XRechnung

`xrechnung_xml(voucher_id, syntax)` maps a saved **Sales** (380) or
**Credit Note** (381) voucher to the EN 16931 model
(`khata_de::xrechnung::to_en16931`) and writes it as **UBL 2.1**
(`"ubl"`) or **UN/CEFACT CII D16B** (`"cii"`) with the XRechnung 3.0
CustomizationID `urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0`
[PRIMARY: KoSIT Schematron v2.6.0]. The answer carries the XML, a file
name, `errors` (broken EN 16931 or BR-DE rules: not a valid XRechnung
until fixed) and `warnings`. Save it with `write_gst_json` (it accepts
`.xml`).

- **Seller**: the company name, its USt-IdNr (BT-31) or Steuernummer
  (BT-32), and the address block above (BT-35 ff., contact BG-6, e-mail
  as electronic address BT-34, IBAN/BIC as SEPA credit transfer 58).
  Without an IBAN the payment means is "1" (not defined), with a warning.
- **Buyer**: the party ledger's name, VAT id (a Steuernummer is not put
  in BT-48), mailing address and post code, country, and e-mail (BT-49).
- **Buyer reference (BT-10, required by BR-DE-15)**: a line
  `Leitweg-ID: …` (or `Buyer reference:`, `Käuferreferenz:`,
  `Ihre Referenz:`) in the voucher narration; the other narration lines
  become invoice notes (BT-22). A reference shaped like a Leitweg-ID with
  a wrong check number gets a warning. (BMF 15 Oct 2025: a missing BT-10
  is "umsatzsteuerlich unbeachtlich" [PRIMARY], but the XRechnung rule
  still fails.)
- **Dates**: the supply date (BT-72) is the invoice date (the books have no
  separate Leistungsdatum yet; a warning says so); the due date (BT-9)
  from the party's bill or credit days.
- **Categories and notes** (§14 Abs. 4 Nr. 8, §14a): 19%/7% → S; `ZERO` →
  Z; `EXEMPT` → E with "Steuerfreie Leistung nach § 4 UStG" (a warning
  asks to name the provision); `REVERSE` → AE with
  "Steuerschuldnerschaft des Leistungsempfängers" (the wording of §14a
  Abs. 5) [PRIMARY]; intra-EU goods → K ("Steuerfreie
  innergemeinschaftliche Lieferung", VATEX-EU-IC, deliver-to country =
  buyer's); exports → G (VATEX-EU-G); services to an EU business → AE
  (§14a Abs. 1); other supplies abroad → O. Each note is also an invoice
  note. A Kleinunternehmer (§19) invoice would be E with the §19 note
  (`Seller::kleinunternehmer`), but the profile has no such flag yet.
  Kleinunternehmer invoices are exempt from the e-invoice obligation
  (§34a UStDV) [PRIMARY].
- **Units**: stock units map to UN/ECE Rec 20 codes (`Stk`/`Nos` → H87,
  `kg` → KGM, `Std` → HUR…); others become C62 with a warning.
- **Discounts** (ledgers on the party's side) become document allowances
  in the first line's category.

**The e-invoice obligation** (Wachstumschancengesetz, §14 and §27 Abs. 38
UStG; BMF letters of 15 Oct 2024 and 15 Oct 2025) [PRIMARY]: every
business must be able to **receive** e-invoices from 1 Jan 2025; for
domestic B2B supplies, paper or other electronic invoices remain allowed
in 2025 and 2026, in 2027 only for issuers with prior-year turnover up to
€800,000 (and EDI with consent), and from 2028 every domestic B2B invoice
must be an e-invoice (EN 16931: XRechnung, or ZUGFeRD from 2.0.1 except
MINIMUM and BASIC-WL). Exceptions: invoices up to €250 (§33 UStDV),
tickets, Kleinunternehmer, §4 Nr. 8–29 supplies, B2C. **Retention**:
invoices 8 years (§14b UStG), books 10 years (§147 AO, after the
Bürokratieentlastungsgesetz IV); keep the XML as received (GoBD as amended
14 Jul 2025) [PRIMARY]. KhataOS keeps the books; it does not archive
received XML files yet.

### ZUGFeRD / Factur-X

A ZUGFeRD invoice is a PDF/A-3 with the CII XML embedded (`factur-x.xml`,
or `xrechnung.xml` for the XRECHNUNG profile), AFRelationship
"Alternative", and XMP metadata in the `fx` namespace
(`urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#`: DocumentType,
DocumentFileName, Version, ConformanceLevel) [SECONDARY: Mustang]. The
current version is ZUGFeRD 2.5.2 / Factur-X 1.09.2 (Aug 2026) [PRIMARY:
ferd-net.de]. `khata_en16931::cii::write` already produces the XML for the
EN 16931 (`urn:cen.eu:en16931:2017`) and XRECHNUNG profiles.
`khata_en16931::facturx` wraps the EN 16931 XML in a PDF/A-3b container
with the `fx` XMP metadata, and `peppol.rs` `facturx_pdf` builds it from a
voucher: Ctrl+E (Export) on a saved Sales invoice or Credit Note →
**Factur-X / ZUGFeRD PDF**. Still open: only the EN 16931 profile is
written (not XRECHNUNG inside the PDF), the visible page is plain text
rather than the invoice design, and the file has not been run through a
PDF/A validator.

## Where it is in the app

- **UStVA:** Go To (Alt+G) → **UStVA (Germany VAT return)**, shown only for
  German companies. F2 changes the period; Ctrl+P prints and Ctrl+E saves
  a CSV.
- **XRechnung:** on a saved Sales invoice or Credit Note, Ctrl+E (Export) →
  **XRechnung (UBL or CII)**, which asks for the syntax and saves the XML.
- **Factur-X:** the same menu → **Factur-X / ZUGFeRD PDF** (above).

## Next steps

- Company profile fields for contact, e-mail, IBAN/BIC, Handelsregister
  number (BT-30, needed by BR-CO-26 when the company has only a
  Steuernummer) and the Kleinunternehmer flag; a Leistungsdatum per
  voucher; a buyer-reference field per party (Leitweg-ID).
- ELSTER submission through ERiC (licence and redistribution for an
  open-source app are open questions in the research).
- Zusammenfassende Meldung (Kz 41 and 21 by buyer VAT id).
- The XRECHNUNG profile inside ZUGFeRD, and the invoice design on its
  page; receiving and archiving e-invoices (XML import, GoBD).
- Run the KoSIT validator on generated samples in CI.

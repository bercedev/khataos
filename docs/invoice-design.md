# Invoice design and PDF export

A company can choose how its invoices look. It can also save any voucher
straight to a PDF file, on every platform KhataOS runs on: Windows, macOS,
Linux, Android and iOS. Whatever the design, the content the country's law
requires is always printed.

- Code:
  - `crates/khata-print/src/design.rs`: the design, `Look` and `Required`.
  - `crates/khata-print/src/images.rs`: logo and signature checks, and the
    SVG sanitiser.
  - `crates/khata-print/src/lib.rs`: the HTML templates (`invoice_html_styled`).
  - `crates/khata-print/src/pdf/`: the PDF writer.
  - `apps/desktop/src-tauri/src/invoice_design.rs`: storage, the country
    rules, `prepare` and the commands.
  - `apps/desktop/src/screens/invoice_design.ts`: the design screen, the
    ship-to and export form, and PDF export.
  - `apps/desktop/src/invoice-api.ts`: the frontend's calls.
- Opened from: Company (Alt+K) → **Invoice design**, or Go To (Alt+G) →
  "Invoice design".

## What a company can set

| Setting | Values | Notes |
|---|---|---|
| Template | Classic, Modern, Compact | A style only. The fields and their order are the same in all three. **Classic** is the print KhataOS always had. **Modern** puts the logo and company at the start, prints the title large in the accent colour and fills the table header. **Compact** uses smaller type and tighter rows, for A5 paper and long invoices. |
| Accent colour | `#rrggbb` | Used for the title, the rules and the section headings, and to fill the Modern table header. Anything that isn't a hex colour is refused when saving, and prints as the default (`#111111`). |
| Font | System, Arial/Helvetica, Serif (Georgia/Times), Monospace (Courier) | Safe system fonts only; nothing is downloaded. Arabic always falls back to the Arabic fonts the prints already used. PDFs always use their embedded font (see below). |
| Paper | A4, Letter (US), A5 | |
| B2C invoices on 80 mm receipts | yes / no | When on, a sale to a buyer with no tax ID, or a KSA simplified invoice, prints on an 80 mm receipt. The receipt is one column with smaller type. As a PDF it is a single page as long as the receipt. B2B invoices always use the paper above. |
| Logo | PNG, JPEG or SVG, 300 KB or less | See [Images](#images). |
| HSN/SAC (tax class) column, quantity and rate columns, amount in words, "Authorised signatory" | on / off | The country can keep some of these on; see [Country rules](#country-rules). |
| Signature image | PNG, JPEG or SVG | Printed above "Authorised signatory". |
| Bank details | account name, bank, branch, account number, IBAN, local code, SWIFT/BIC | The labels follow the country (`BankLabels::for_country`): India "IFSC", UK "Sort code", US "Routing number (ABA)", Singapore "Bank / branch code", Mexico "CLABE" for the account number. IBAN countries (AE, SA, DE, FR, NL, ES) have no local code field. Empty fields are not printed. |
| Terms and conditions, declaration, footer note | text, up to 2,000 characters each | Terms, the declaration and the bank details print on invoices and notes, not on payments or journals. The footer note prints on every voucher. |
| LUT ARN (India) | text | Printed with the export endorsement on supplies under a Letter of Undertaking. A voucher can give its own. |
| Titles | per voucher type | Only where the law allows a choice; see below. |

The screen shows a live preview of the unsaved settings beside the form. It
uses the latest sale, or a sample invoice if the company has none yet, and
it can also show the B2C receipt. The screen is fully keyboard-operable:
- Enter moves on, and accepts on the last field.
- Up and Shift+Tab go back.
- Left, Right and Space change a choice; a letter jumps to the choice that
  starts with it.
- Enter on Logo or Signature opens a menu (Choose a file / Remove).
- PageDown and PageUp scroll the preview.
- Ctrl+A saves.
- Esc leaves, asking first if anything changed.

Only the company's owner can change the design (`UserRole::Owner`, as for
the company details).

## Country rules

The design can never remove what a country requires. That is enforced in
two places.

1. **`Required`** (khata-print) overrides the toggles. `invoice_design::required`
   sets it for Sales, Credit Notes and Debit Notes:

   | Kept on | Countries | Why |
   |---|---|---|
   | Tax class / HSN/SAC column | IN, AE, SA, GB, DE, FR, NL, ES | India Rule 46(g) (HSN). Elsewhere, the VAT rate or category of each line: UAE ER Art 59(1)(h), KSA IR Art 53, UK Notice 700/21 §4.3, EU VAT Directive Art 226(9). |
   | Quantity and unit price | IN, AE, SA, GB, DE, FR, NL, ES, MX | India Rule 46(i); UAE ER Art 59(1)(h); KSA IR Art 53; UK Notice 700/21; EU Art 226(6) and (8); Mexico's CFDI. |
   | "Authorised signatory" | IN | Rule 46(q): a signature on a paper invoice. |

   In the US and Singapore every toggle works. On payments, receipts,
   journals and purchases every toggle works everywhere.

2. **Content outside the design.** Everything below is part of the document
   (`InvoiceDoc`), not the design, so no template can drop it.
   - **India**:
     - the GSTIN, the state and the place of supply;
     - HSN codes;
     - the buyer's GSTIN and address;
     - the IRN block with the signed QR (spliced into the HTML; a
       `PrintBlock` in the PDF);
     - the Rule 46 export/SEZ endorsement.
   - **Saudi Arabia**:
     - the bilingual layout (`InvoiceLayout::Bilingual`);
     - both VAT numbers;
     - the ZATCA QR (`doc.seller_qr`);
     - the ZATCA title: "Tax Invoice" or "Simplified Tax Invoice", with the
       Arabic.
   - **UAE**:
     - the bilingual layout;
     - the TRN;
     - "Tax Invoice" (ER Art 59(1)(a)) and "Tax Credit Note" (ER Art 60).
   - **UK**:
     - the VAT number;
     - the domestic reverse charge wording ("Reverse charge: Customer to pay
       the VAT to HMRC.", Notice 735 §7.6.1).
   - **Mexico**: the PAC block (folio fiscal, TFD data, seals, QR, "Este
     documento es una representación impresa de un CFDI.").
   - **Statutory notes** (`InvoiceDoc::legal_notes`) print in every design
     and can't be switched off.

**Titles** (`invoice_design::title_rule`):

| Country | Voucher | Rule |
|---|---|---|
| SA | Sales, Credit Note, Debit Note | Fixed. The ZATCA title comes from the e-invoice, with the Arabic (IR Art 53). |
| AE | Sales | Fixed: "Tax Invoice" (ER Art 59(1)(a)). |
| AE | Credit Note | Fixed: "Tax Credit Note" (ER Art 60). |
| SG | Sales | Fixed: "Tax Invoice". |
| IN | Sales | One of "Tax Invoice" (Rule 46), "Bill of Supply" (Rule 49, for exempt supplies or composition dealers) or "Invoice-cum-bill of supply" (Rule 46A). |
| IN | Credit Note, Debit Note | Fixed (Rule 53). |
| Others, and every other voucher type | | Free text. The default is the existing title ("Tax Invoice", "Payment Voucher"…). |

A title the rule doesn't allow is refused when saving, with the reason. A
stored title that no longer fits the rule is ignored when printing.

Tests: `invoice_design::tests` posts real vouchers for each launch country
and prints them in every template and paper, and on the B2C receipt. Every
optional toggle is off, so the tests prove what the law keeps. Each test
checks the HTML print and the PDF's text (and the QR codes drawn):
- `india_keeps_gstin_place_of_supply_hsn_and_the_irn_in_every_design`
- `india_exports_print_the_rule_46_endorsement_lut_arn_and_shipping_data`
- `india_titles_are_the_rules_choices_only`
- `ksa_keeps_the_bilingual_layout_vat_numbers_and_the_zatca_qr_in_every_design`
- `ksa_simplified_invoices_go_on_the_receipt_with_their_arabic_and_qr`
- `gulf_exports_print_the_zero_rating_in_english_and_arabic`
- `uae_keeps_the_trn_and_the_tax_invoice_title_in_every_design`
- `uk_keeps_the_vat_number_and_the_reverse_charge_wording_in_every_design`
- `mexico_keeps_the_pac_block_in_every_design`

`khata-print` also has `design_html_tests`, which checks every template ×
paper × layout, and `pdf::tests`.

## Export invoices, ship-to and foreign currency

Per voucher, Ctrl+E → **Ship-to and export details** sets the following
(`VoucherPrintDetails`):
- a ship-to name and address;
- "Export": blank means decided by the party's mailing country;
- port of loading, port of discharge, Incoterms, country of destination,
  and the shipping bill (export declaration) number and date;
- for India, whether the export is under LUT or with IGST (blank means
  decided by whether the voucher charges IGST), and an optional LUT ARN.

These details are print-only: **the accounting entries don't change.** The
voucher type has no place for extra fields, and the core and the audit
format were not changed. The details are a shared record of the company,
under `invoice.voucher.<uuid>`, and sync with the books.

The bill-to address is the party's mailing address (street, postal code,
and the country if it is abroad).

Statutory notes (`invoice_design::legal_notes`, Sales only):
- **India, export or supply to an SEZ:** the endorsement Rule 46 prescribes
  (first proviso), either "SUPPLY MEANT FOR EXPORT/SUPPLY TO SEZ UNIT OR SEZ
  DEVELOPER FOR AUTHORISED OPERATIONS ON PAYMENT OF INTEGRATED TAX" or
  "…UNDER BOND OR LETTER OF UNDERTAKING WITHOUT PAYMENT OF INTEGRATED TAX".
  Under LUT, "LUT ARN: …" follows. This is the statutory text, not a vendor
  template.
- **UAE and KSA export:** "Zero-rated supply: export" with the Arabic
  (توريد خاضع للضريبة بنسبة الصفر: تصدير). [UNCERTAIN: have an
  Arabic-reading adviser check the wording.]
- **UK:** "Zero-rated export (0% VAT)".
- **Singapore:** "Zero-rated supply: export (0% GST)".

UK exports of services are often outside the scope of UK VAT rather than
zero-rated. Set "Export: No" on those and say so in the narration.

**Foreign currency.** `InvoiceDoc::currency` (`CurrencySection`: code, base
code, rate, base-currency total, optional base-currency tax) is filled by
the multi-currency code (`forex::invoice_currency`), which also restates
the lines, taxes, total and amount in words in the party's currency. When
it is set, every design prints "Amounts in USD at 1 USD = 83.25 INR" and
then the figures the country asks for in its own currency
(`CurrencySection::base_figures`, chosen by the base currency):

| Base currency | Figures |
|---|---|
| SGD | Total excluding GST (base total − base tax), GST and total including GST, in SGD |
| AED, SAR, GBP | VAT and total in the base currency (UAE ER Art 59(1)(k), KSA, UK) |
| INR | GST and total in INR |
| others | total in the base currency |

Test: `foreign_currency_invoices_keep_the_base_currency_figures_each_country_asks_for`
(SG, IN, AE and GB, every template and paper, HTML and PDF).

The structured e-invoices (ZATCA, PINT, XRechnung, Factur-X) refuse
foreign-currency vouchers for now (`forex::refuse_foreign_einvoice`); the
plain PDF export works for them.


## PDF export

- **Ctrl+E on any saved, uncancelled voucher → PDF.** In India, Saudi
  Arabia, the UAE, Singapore, Germany, Spain, France, the Netherlands and
  Mexico, the option sits beside the e-invoice formats. Elsewhere it is the
  export menu.
- **Alt+E on the Day Book** offers:
  - one PDF of the period's Sales, Credit Notes and Debit Notes, each
    starting on a new page (up to 500);
  - a PDF of the voucher under the cursor.
- Files are saved through the save dialog and `write_binary_file`, the path
  the Factur-X export already uses. On phones that writes to the picked
  `content://` or `file://` URI (`peppol::write_binary_uri`, which uses
  `phone_files`).

### The approach, and why

The options were:

1. **HTML to PDF in pure Rust.** This needs an HTML and CSS layout engine.
   None is small enough, and none handles the prints' CSS (grid, `:has`,
   bidi).
2. **The webview's own print-to-PDF.** This means a different native API on
   each platform:
   - WebView2 `PrintToPdf` on Windows;
   - `WKWebView` print operations on macOS;
   - `UIPrintPageRenderer` on iOS;
   - WebKitGTK `WebKitPrintOperation` on Linux;
   - `PrintDocumentAdapter` on Android, which needs Java glue because its
     callbacks are package-private.

   That is five native implementations, plus a hidden window to render in,
   and phones can't even print today.
3. **A small layout engine on a hand-written PDF writer**, as khata-en16931's
   Factur-X writer already does (hand-written objects, `flate2` streams).

**KhataOS uses option 3** (`khata-print/src/pdf/`). It is pure Rust, so the
same code and the same output run on Windows, macOS, Linux, Android and
iOS, with no native code. Its dependencies are all pure Rust: `flate2`
(already in the workspace), `png` (already in the app through Tauri), and
for text the crates in [Scripts and fonts](#scripts-and-fonts).

The Factur-X writer itself could not be reused as it is: it is text-only,
monospace and WinAnsi. Its approach was reused instead:
- hand-written objects, a cross-reference table, deterministic output and
  compressed streams;
- embedded fonts, subset per file, drawn through Type0 fonts with
  Identity-H codes and a ToUnicode map, so text can be copied and searched;
- OpenType shaping and the Unicode Bidirectional Algorithm for every
  script, and a right-to-left page for Arabic (below);
- PNG (with alpha) and JPEG images;
- text wrapping, and tables split across pages with the header repeated.

The PDF prints the same content, in the same order, as the HTML. Both come
from the same `InvoiceDoc`, `Look` and `prepare`.

Limits:
- **Characters no font has.** A character that neither the bundled fonts,
  the `fonts` folder nor the system has prints as "?" (it still copies as
  itself). In practice: CJK on a Linux install without CJK fonts, emoji
  (colour fonts can't be embedded), and rare scripts.
- **Font choice.** The design's font applies to printing only; PDFs always
  use the fonts below.
- **Bold.** Fonts are Regular only; bold is drawn by stroking the outline,
  as before.
- **PDF/A.** The PDF is not PDF/A. Factur-X keeps its own writer for that.

### Scripts and fonts

Text goes through three steps (`pdf/shape.rs`):

1. **Bidi.** `unicode-bidi` (UAX #9 with isolates) orders each line. A
   line's direction is its first strong letter's, else the page's. The
   Arabic half of every "English / Arabic" label is an RLI…PDI isolate, as
   the HTML's `dir="rtl"` spans are, so "VAT number / الرقم الضريبي:
   300…" keeps the colon and number after the Arabic. Numbers inside
   Arabic text keep their digits' order, and amounts stay whole. Text in
   any other line follows UAX #9: in "Invoice فاتورة 123" the number
   belongs to the Arabic word, as in a browser.
2. **Fonts by script.** `unicode-script` splits each run by script; spaces,
   digits and punctuation join their neighbours. Each character takes:
   - the bundled font of its script;
   - else DejaVu Sans;
   - else any bundled font that has it;
   - else a font from the app's `fonts` folder;
   - else a system font.
3. **Shaping.** `rustybuzz` (a port of HarfBuzz) shapes each piece:
   - Indic conjuncts, matra reordering and reph;
   - Arabic joining, lam-alef and mark placement (Noto Sans Arabic's
     required ligatures);
   - Hebrew;
   - kerning;
   - mirrored brackets in right-to-left runs.

**Copy and search.** Each glyph's ToUnicode text is its share of its
cluster: the cluster's glyphs, in reading order, spell the cluster. So श्री
(two glyphs) copies as श्री and कि (i-matra glyph first) as कि. A glyph
shared by different clusters gets a code for each. Glyphs that stand for no
text, such as Noto Sans Arabic's separate dots or a mark, need two things,
since readers would otherwise put a space or the code itself into the text:
- they are drawn after the line's other glyphs;
- their codes are control or invisible characters.

Text extraction was checked with poppler (`pdftotext`) and Apple PDFKit:
every sample script reads back as typed, Arabic in reading order. MuPDF
reads the text right too, but adds those control characters. The tests
read the PDF back through its ToUnicode maps (`pdf::tests::text_round_trips_through_to_unicode`).

**Bundled fonts** (`crates/khata-print/assets/`), Regular weights,
unhinted:

| Font | Scripts | Licence |
|---|---|---|
| DejaVu Sans 2.37 | Latin, Greek, Cyrillic, Hebrew, Armenian, Georgian, symbols, ₹ € £ | Bitstream Vera terms, DejaVu changes public domain (`DejaVu-LICENSE.txt`) |
| Noto Sans Devanagari 2.007 | Hindi, Marathi, Sanskrit, Nepali | SIL OFL 1.1 (`noto/OFL.txt`) |
| Noto Sans Gujarati 2.106 | Gujarati | SIL OFL 1.1 |
| Noto Sans Tamil 2.004 | Tamil | SIL OFL 1.1 |
| Noto Sans Telugu 2.005 | Telugu | SIL OFL 1.1 |
| Noto Sans Kannada 2.006 | Kannada | SIL OFL 1.1 |
| Noto Sans Malayalam 2.104 | Malayalam | SIL OFL 1.1 |
| Noto Sans Bengali 3.011 | Bengali, Assamese | SIL OFL 1.1 |
| Noto Sans Gurmukhi 2.004 | Punjabi | SIL OFL 1.1 |
| Noto Sans Oriya 2.007 | Odia | SIL OFL 1.1 |
| Noto Sans Arabic 2.013 | Arabic, Persian, Urdu | SIL OFL 1.1 |

The Noto fonts are the static instances from
`notofonts/notofonts.github.io`. Noto Sans Arabic was picked over Noto Naskh
Arabic: it sits better beside DejaVu Sans, and its line height is closer
to it.

**Size.** `build.rs` deflates the fonts into the binary, and each font is
inflated the first time a PDF needs it.
- On disk, the fonts are 1.92 MB raw: DejaVu Sans 0.76 MB, the ten Noto
  fonts 1.16 MB.
- In the binary they are 0.89 MB. Before, DejaVu Sans alone went in raw at
  0.76 MB, so the fonts add 0.13 MB.
- The macOS release binary of the app grew from 32.49 MB to 34.37 MB
  (+1.88 MB, +5.8%). That is the fonts, plus the code of the shaping,
  bidi, subsetting and font-finding crates. The build was unstripped and
  without LTO; other work landed at the same time.

A PDF embeds only the glyphs it uses. An English A4 invoice is about 20 KB,
and a bilingual one about 30 KB.

**CJK.** Noto Sans CJK is about 16 MB a weight, so it is not bundled.
Instead, CJK uses the system's fonts, found by family name with `fontdb`
the first time a character needs one:

| Platform | Fonts | Where |
|---|---|---|
| Windows | Microsoft YaHei, Yu Gothic, Malgun Gothic | `%WINDIR%\Fonts` |
| macOS | PingFang, Hiragino, Apple SD Gothic Neo | `/System/Library/Fonts` and its asset folders |
| iOS | Hiragino Kaku Gothic, Apple SD Gothic Neo | `/System/Library/Fonts/Core` |
| Android | Noto Sans CJK | `/system/fonts`, read directly (fontdb has no Android folders) |
| Linux | Noto Sans CJK, WenQuanYi, Droid Sans Fallback | the fontconfig folders |

How the CJK font is chosen:
- Han takes a Chinese font first, or a Japanese one on a line with kana,
  or a Korean one on a line with Hangul.
- Other scripts the bundle lacks (Thai, Sinhala, Khmer…) take a system
  family named after the script.
- System fonts are used only where the bundled fonts have no glyph, so
  every script they cover prints the same on every machine.

The iOS fonts are plain files in the simulator's runtime. PingFang is not
a file on iOS. That a device's sandbox lets the app read those files is
expected but not yet checked on a device. If the read fails, CJK prints
"?" there, unless the folder below has a font.

Where the system has no CJK font, the user can copy one (Noto Sans CJK, an
OFL font) into the app's data folder, under `fonts/`
(`khata_print::pdf::add_font_dir`). The app never downloads a font by
itself. A font pack download, offered with the user's consent, would only
need to fill that folder.

**Subsets.** Fonts are subset with `subsetter`, then embedded by outline
type:

| Outlines | Embedded as | Codes |
|---|---|---|
| TrueType | CIDFontType2, `/FontFile2` | One per glyph and text, mapped to glyphs by a `/CIDToGIDMap` stream |
| CFF (the system CJK fonts) | CIDFontType0, `/FontFile3 /CIDFontType0C` | The subset's glyph ids |

Collections (`.ttc`) are read face by face, not whole. Colour-bitmap fonts
and CFF2 fonts are skipped. `khata_print::pdf::set_system_fonts(false)`
turns system fonts off, so PDFs come out the same on every machine.

**Crates added** (exact versions, all pure Rust):

| Crate | Version | Licence | Use |
|---|---|---|---|
| `rustybuzz` | 0.20.1 (Nov 2024) | MIT | OpenType shaping |
| `unicode-bidi` | 0.3.18 | MIT OR Apache-2.0 | UAX #9 |
| `unicode-script` | 0.5.8 | MIT OR Apache-2.0 | Script itemisation |
| `subsetter` | 0.2.6, no default features | MIT OR Apache-2.0 | Subsetting TrueType and CFF |
| `fontdb` | 0.24.0 | MIT | Finding system fonts by family |

`rustybuzz` pulls in `ttf-parser` (MIT OR Apache-2.0) and small Unicode
tables (MIT OR Apache-2.0). Its successor `harfrust` (MIT, from the
HarfBuzz project) was considered. `rustybuzz` 0.20.1 is the version asked
for, and it has been stable since 2024.

### Right-to-left PDFs

With the Arabic print language, the PDF page runs right to left
(`PdfLabels::Arabic`). The page is laid out as usual, then mirrored:
- the logo, the boxes and the table columns change sides;
- the QR codes move to the left but are never mirrored themselves;
- every cell, numbers included, is right-aligned;
- labels are in Arabic (`ArabicLabels`, `labels.rs`) with Arabic month
  names;
- names and values from the books are FSI…PDI isolates, as the HTML's
  `<bdi>` is;
- with Arabic words for the total, those print alone.

Digits stay Latin, as in the app (`docs/i18n.md`).
`PdfLabels::Arabic { arabic_indic_digits: true }` prints amounts,
quantities, dates and page numbers in Arabic-Indic digits (١٬١٥٠٫٠٠).
Identifiers keep their digits: tax numbers, invoice numbers and bank
accounts.

The KSA and UAE bilingual invoice stays left to right, with each label in
English and Arabic.

**Hijri dates.** The caller formats them, since Rust has no Hijri calendar.
- A single voucher's PDF takes one, as before.
- Batch PDFs (`export_vouchers_pdf`) take `hijri`, a map from ISO date to
  Hijri date. With "Show Hijri dates" on, the Day Book fills it for every
  day of the period.

### Visual check

`KHATA_PDF_SAMPLES=<dir> cargo test -p khata-print pdf_samples` writes
sample PDFs to `<dir>`:
- every template and paper;
- `bilingual.pdf`;
- `arabic-*.pdf`: A4 and receipt, with Latin and with Arabic-Indic digits;
- `scripts.pdf`: a line in each Indian script, CJK and mixed bidi.

Render them with `pdftoppm -r 150 -png <file>.pdf <name>` to look at them.

## Storage

| What | Where | Key |
|---|---|---|
| The design | the company file's shared `records`, as JSON | `invoice.design` |
| Logo, signature | `records`, JSON `{kind, data (base64), raster}` | `invoice.logo`, `invoice.signature` |
| Per-voucher ship-to and export data | `records` | `invoice.voucher.<voucher uuid>` |

**The decision.** Invoice designs are the company's shared records, like
`vat_gb.schemes` and the IRP records (`docs/sync-settings.md`).

- **Backups carry them.** The company is one SQLite file, so a copy or
  backup of it carries the design and the images. The images are small: at
  most 300 KB each.
- **They are kept out of the audit trail.** The trail is the hash-chained
  record of the books. A design is not accounting data, and the trail has no
  non-accounting record type. Putting it in the company profile (an audit
  entity) would add every logo change, base64 and all, to the chain. It
  would also change what every co-worker replays.
- **The audit format is unchanged.** Records are not audit entries; a test
  checks that saving a design adds none.
- **Synced, and in `.khatapkg` files.** The design, the images and the
  per-voucher details reach co-workers and the accountant on every path the
  books take: the office server, a self-hosted server, the relay (sealed
  with the company key) and accountant files. The last change wins. Only an
  Owner changes the design; a company shared as Viewer is read-only. A sync
  rebase takes the records along to the rebuilt file. With a server older
  than protocol 2 they wait until it is updated.

**Images.** PNG and JPEG are checked by their headers; a PNG is decoded once
and must be at most 4,000 pixels each way. SVGs are rebuilt from an allow
list of elements and attributes. Anything else is refused:
- `<script>`, `<foreignObject>`, `<image>` and `<a>`;
- animations;
- `on*` handlers (these are dropped);
- links that aren't `#local`;
- `url()` that isn't `#local`;
- DOCTYPE and entities;
- `@import` in style sheets.

Images only ever show through `<img src="data:…">`, where an SVG runs no
script and loads nothing, so the sanitiser is a second line of defence. The
PDF writer draws no SVG. When an SVG is added, the design screen draws it
on a canvas (as an `<img>`) and stores a PNG copy for PDFs
(`set_invoice_image_raster`).

## Follow-ups

- A consented download of a CJK font pack into the `fonts` folder, for
  machines without CJK fonts. The PDF side is ready; it needs only a
  setting and the download.
- Check on an iOS device that the system CJK fonts can be read.
- A setting for Arabic-Indic digits in Arabic PDFs; the engine supports
  them.
- Unregistered companies: titles such as "Invoice" or "Bill" in place of
  "Tax Invoice", where the law forbids an unregistered supplier to issue a
  tax invoice (India, Singapore).
- The Arabic of the new labels (`labels.rs`) and of the zero-rating note
  should be reviewed by an Arabic-reading tax adviser.

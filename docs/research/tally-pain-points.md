# TallyPrime / Tally.ERP 9: user pain points

Research date: 2026-10-19. Scope: India first, then UAE/KSA. This file feeds the KhataOS product requirements.

## Method and source caveats (read first)

- **Reddit could not be accessed.** `old.reddit.com` answers every unauthenticated request with a 302 redirect to `/login/?reason=lor2`. `www.reddit.com/*.json` returns 403. The WebSearch tool refuses `reddit.com` as a domain. As a result, **no claim in this file comes from Reddit.** A person should do a logged-in pass on r/CharteredAccountants, r/IndiaTax, r/IndianStartups and r/developersIndia, and fill in the gaps marked `[U]`.
- **G2, Capterra, TrustRadius, GetApp, Software Advice, Quora and MouthShut all return HTTP 403** to direct fetches. Claims from those sites come from search-engine summaries of the pages and are marked `[S]`.
- **CAclubindia was scraped directly.** I collected 473 thread titles from the *Tally* sub-forum (forum_id=25) and the *GST > Software* sub-forum (forum_id=117), plus 600 titles from *Experts > Info Technology* (cat_id=9). I then read the key threads. The raw title lists were temporary working files and are not in this repository. Many of these threads date from the ERP 9 era (2009–2018). The recent threads (2022–2025) are flagged as such below.
- Other user forums read directly: TechEnclave (Discourse JSON), Hacker News (Algolia API), Techjockey reviews, Trustpilot, and the CodeWeavers forum.
- **Tally's own help pages were used for hard facts** such as prices, rules and error messages. **Vendor blogs** (tallyatcloud, accountune, aiaccountant and others) sell competing or adjacent products, so they are flagged `(vendor)`.

**About this file.** It is a planning input, not a review of TallyPrime and not a claim about it. Prices, plans and features are as Tally Solutions published them on the research date and may have changed; check tallysolutions.com for current details. Quotes are the opinions of the users and sites linked next to them, not statements by the KhataOS project. "Tally", "TallyPrime" and "Tally.ERP 9" are trademarks of Tally Solutions Pvt. Ltd.; KhataOS is not affiliated with Tally Solutions.

**Evidence tags:**
- `[V]` the page was fetched and read directly.
- `[S]` taken from a search-engine summary only.
- `[U]` unverified: plausible, but no first-hand user complaint was found.

**"How often mentioned"** is a qualitative call (High / Med / Low) across the sources I could reach. It is backed by the CAclubindia title counts where those exist.

---

## Ranked table

| # | Pain point | Sev (1-5) | How often mentioned | Who's affected | Evidence links |
|---|---|---|---|---|---|
| 1 | Annual TSS lock-in: GST, e-invoice, banking, remote and mobile features, upgrades and backup all need a paid TSS on top of the licence | 5 | High | SMB owners, CA firms | [TCO page](https://tallysolutions.com/business-guides/tallyprime-total-cost-of-ownership-india/) [V], [TSS FAQ](https://tallysolutions.com/tally/faqs-on-tally-software-services-tss) [V], [Techjockey reviews](https://www.techjockey.com/reviews/tally-erp-9) [V] |
| 2 | Remote and multi-user access is LAN-bound: needs RDP + TVU fees, a paid Cloud Access add-on, or a same-subnet VPN | 5 | High | Multi-branch SMBs, CAs, owners away from office | [TechEnclave remote office](https://techenclave.com/t/two-remote-office-single-tally-server/252434) [V], [TVU FAQ](https://help.tallysolutions.com/tally-virtual-user-tvu-faq/) [V], [TechEnclave TVU PSA](https://techenclave.com/t/psa-tally-tvu-aka-tally-virtual-user-licenses/250506) [V] |
| 3 | Data corruption, repair that deletes data, and backups users must remember to take | 5 | High (forums) | Everyone, especially single-PC SMBs | [Repair FAQ](https://help.tallysolutions.com/repairing-company-data-faq/) [S], [Migration/corruption FAQ](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/data-migration-data-corruption-faq/) [V], [CAclubindia: FILE CORRUPT](https://www.caclubindia.com/forum/file-corrupt-39438.asp) [V] |
| 4 | CA–client data exchange: emailing or pen-driving backups, release mismatches, no live shared ledger | 4 | High | CAs, article assistants, clients | [Tally FAQ on 1800 files](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/data-migration-data-corruption-faq/) [V], [tallyatcloud (vendor, CA author)](https://www.tallyatcloud.com/article/modern-method-used-by-ca-firms-to-access-client-tally-data-without-pendrive-or-teamviewer/930/0/1) [V], [ICAI AI use case](https://ai.icai.org/usecases_details.php?id=183) [V] |
| 5 | Getting data in and out (Excel import, integrations, APIs) is hard. Native Excel import only arrived in Release 4.0 | 4 | **Highest in CAclubindia** (33 of 473 titles) | Accountants, data-entry staff, developers | [CAclubindia large Excel import](https://www.caclubindia.com/forum/how-to-import-large-data-from-excel-to-tally-468664.asp) [V], [tally-database-loader README](https://github.com/dhananjay1405/tally-database-loader) [V], [Tally Excel import](https://tallysolutions.com/apac/tally/import-data-from-excel-to-tallyprime/) [S] |
| 6 | One-way, multi-step version migration (7.2 → ERP 9 → Prime), with data and TDL breakage | 4 | Med | Long-time ERP 9 users | [Migration FAQ](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/data-migration-data-corruption-faq/) [V], [ERP 9 TDL in Prime](https://help.tallysolutions.com/working-of-tally-erp-9-customisations-with-tallyprime/) [V], [CAclubindia migration](https://www.caclubindia.com/forum/tally-9-data-migration-problem-22994.asp) [V] |
| 7 | GST workflow gaps: GSTR-2B reconciliation pushed out to Excel and third-party tools, e-invoice rounding rejections, GSTR-9/9C only "prepared" | 4 | Med-High | CAs, GST practitioners | [CAclubindia e-invoice rounding](https://www.caclubindia.com/forum/round-off-gst-amount-and-e-invoice-way-bill-rejection-in-tally-prime-593693.asp) [V], [ICAI 2B use case](https://ai.icai.org/usecases_details.php?id=183) [V], [GSTR-9C help](https://help.tallysolutions.com/file-gstr-9c-tally/) [S] |
| 8 | Performance with large data and over networks: slow voucher saves, data splitting by year | 4 | Med | Mid-size traders, multi-user offices | [CodeWeavers forum](https://codeweavers.com/compatibility/crossover/forum/tally-9?msg=306572) [S], [Tally perf checklist](https://help.tallysolutions.com/checklist-for-troubleshooting-performance-issues-of-tallyprime/) [V], [Techjockey reviews](https://www.techjockey.com/reviews/tally-erp-9) [V] |
| 9 | Windows-only: no Mac or Linux build; workarounds are Parallels or paid Cloud Access | 3 | Med | Mac-using founders, CAs, startups | [Tally FAQ](https://tallysolutions.com/tally-prime-faq-guide/) [S], [Parallels guide](https://gauravtiwari.org/tally-on-mac/) [S], [TechEnclave Linux](https://techenclave.com/t/whats-the-1-pain-point-which-made-you-to-switch-to-linux-for-whatever-you-use-pc/407302) [V] |
| 10 | Mobile access is view-only reports in a browser, gated by TSS. Local, no-cloud mobile access doesn't exist | 3 | Med | Owners, sales staff | [CAclubindia iOS LAN](https://www.caclubindia.com/forum/mobile-app-with-tally-prime-lan-access-605988.asp) [V], [Tally mobile page](https://tallysolutions.com/tally/tallyprime-mobile-access-business-reports/) [S], [TVU FAQ](https://help.tallysolutions.com/tally-virtual-user-tvu-faq/) [V] |
| 11 | Customisation and report formats depend on TDL plus a partner; TDL breaks on upgrades | 3 | Med | SMBs with custom invoices or MIS, partners | [ERP 9 TDL in Prime](https://help.tallysolutions.com/working-of-tally-erp-9-customisations-with-tallyprime/) [V], [Tally integrations page](https://tallysolutions.com/business-guides/tallyprime-integrations-without-any-custom-development/) [S], [Capterra UK](https://www.capterra.co.uk/reviews/127762/tally-erp-9) [S] |
| 12 | Audit trail (MCA edit log, 1 Apr 2023): two product variants, a log that cannot be pruned, a forced upgrade | 3 | Low (user voice) / High (compliance) | Companies under the Companies Act, their auditors | [Edit Log FAQ](https://help.tallysolutions.com/edit-log-in-tallyprime-faq/) [V], [TaxGuru](https://taxguru.in/company-law/demystifying-audit-trail-edit-log.html) [S] |
| 13 | Bank reconciliation: live feeds for only 4 banks, TSS needed; imported statements don't create vouchers | 3 | Low-Med | Accountants | [Tally banking guide](https://tallysolutions.com/business-guides/tallyprime-banking-integration-auto-bank-feeds-auto-brs-payments/) [S], [CAclubindia auto BRS](https://www.caclubindia.com/forum/auto-bank-reconciliation-not-working-153596.asp) [title only] |
| 14 | TDS/TCS: return formats lag new rules; FVU validation is done outside Tally | 3 | Low | CAs, payroll | [TDS FAQ](https://help.tallysolutions.com/tds-faq-tally/) [S], [CAclubindia TDS](https://www.caclubindia.com/forum/tally-tds-not-calculating-right-170913.asp) [V] |
| 15 | Lost admin or TallyVault passwords, and weak user/role controls | 2 | Med (10 forum titles) | Small offices with staff turnover | [CAclubindia password recovery](https://www.caclubindia.com/forum/tally-password-recovery-242609.asp) [title], [Experts: break TallyVault](https://www.caclubindia.com/experts/break-tally-vault-password-2390169.asp) [title] |
| 16 | UAE/KSA: e-invoicing ASP status unclear, FTA submission needs a connector | 3 (for roadmap) | Low | GCC SMBs | [ClearTax UAE](https://www.cleartax.com/ae/uae-e-invoicing-tally-integration) [S], [Tally UAE ASP](https://tallysolutions.com/mena/uae-vat/tallyprime-peppol-certified-provider-for-uae-einvoicing/) [S] |
| 17 | Multi-currency: forex gain/loss is a manual journal; balance mismatches | 2 | Low | Exporters, importers, GCC | [CAclubindia exchange rate](https://www.caclubindia.com/forum/tally-9-exchange-rate-trouble--157673.asp) [V], [Tally multi-currency](https://tallysolutions.com/business-guides/multi-currency-in-tally-erp-9/) [S] |
| 18 | UI: dated look and steep learning curve, but loved for keyboard speed; ERP 9 → Prime shortcut changes | 2 | Med (mixed: also a top strength) | New staff, students, power users | [HN 22082990](https://news.ycombinator.com/item?id=22082990) [V], [HN 26099093](https://news.ycombinator.com/item?id=26099093) [V], [SoftwareSuggest](https://www.softwaresuggest.com/tally-erp-9) [V] |

---

## Details and KhataOS answers

### 1. TSS lock-in and total cost (Sev 5)

**Prices**
- The licence is ₹22,500 for Silver (single PC) and ₹67,500 for Gold (multi-user LAN), plus 18% GST. Both include one year of TSS. [TCO page](https://tallysolutions.com/business-guides/tallyprime-total-cost-of-ownership-india/) [V]
- TSS renewal from year 2 costs ₹4,500/yr for Silver, ₹13,500/yr for Gold and ₹6,750/yr for the Auditor Edition. [same page] [V]
- Rental runs ₹750/month for Silver and ₹2,250/month for Gold. [Buy page](https://tallysolutions.com/buy-tally/) [V]

**What stops without TSS**
- Without TSS you lose "remote access, software upgrades, connected services". [TSS FAQ](https://tallysolutions.com/tally/faqs-on-tally-software-services-tss) [V]
- If TSS lapses, statutory updates and connected GST, banking and WhatsApp features stop. TallyDrive backup, Bharat Connect, Amazon import and the new invoice templates all need an active TSS. [TCO page] [V]
- Online e-invoicing does not work without TSS; only offline JSON export does. [e-Invoice FAQ](https://help.tallysolutions.com/e-invoice-faq/) [S]
- Upgrading to TallyPrime 3.0 needs a valid TSS. A search snippet from a Tally help page says users whose TSS expired before 12 Jun 2023 cannot upgrade. `[U]`: the fetched [migration page](https://help.tallysolutions.com/migrate-from-older-tp-versions-to-latest/) does not state this.

**What users say**
- Techjockey reviewers: "PRICE IS HIGH" (accountant, Apr 2023); "license renewed every year may not be a feasible option for small businesses" (Oct 2020); "High upfront cost" (Jul 2024). [Techjockey](https://www.techjockey.com/reviews/tally-erp-9) [V]
- Capterra UK reviewers call it "pretty expensive". [Capterra UK](https://www.capterra.co.uk/reviews/127762/tally-erp-9) [S]
- A single-user Silver owner asks whether repeatedly surrendering and re-activating the licence to move between an office PC and a home laptop "will get me into trouble". [CAclubindia, Jul 2020](https://www.caclubindia.com/forum/how-to-use-tally-erp-9-silver-single-user-in-both-my-laptop-and-pc--551062.asp) [V]
- Some forum replies blame a "pirated version" for errors. [CAclubindia](https://www.caclubindia.com/forum/file-corrupt-39438.asp) [V] Whether price is the reason is `[U]`.

**KhataOS answer:** Open-source core with no licence key, no per-seat or per-machine activation, and no "subscription to stay compliant". Statutory updates (GST, TDS, VAT schema changes) ship as free releases. Optional paid services, such as hosted sync and managed e-invoice/GSP relay, sit outside the core and never block reading or exporting your own books. The software never stops working if you stop paying.

### 2. Remote and multi-user access (Sev 5)

**What users say**
- "Tally is restricted to work on single vlan and it does not have a cloud native solution yet." A 2021 thread on running two offices off one Tally server. [TechEnclave](https://techenclave.com/t/two-remote-office-single-tally-server/252434) [V]
- In the same thread, an L2 VPN made Tally "abysmally slow for normal use". In Oct 2026, a reply says it "works over VPN but need same subnet both sides". [same thread] [V]
- A TechEnclave PSA calls the TVU (virtual user) policy for RDP "a money grab from Tally". It took effect 1 May 2021. [TechEnclave PSA](https://techenclave.com/t/psa-tally-tvu-aka-tally-virtual-user-licenses/250506) [V]
- Even a small 9–15 PC office is advised to move "tally on cloud with AWS". [TechEnclave 2024](https://techenclave.com/t/office-data-file-sharing-cpu-with-9-15-computers/267369) [V]

**Official rules**
- TVU packs are needed for access over RDP, Citrix or cloud. They cost ₹125/user/month, billed annually.
- Silver includes 1 TVU and Gold includes 10.
- Remote Access and Reports-in-Browser need an active TSS.
- Source for all three: [TVU FAQ](https://help.tallysolutions.com/tally-virtual-user-tvu-faq/) [V]
- Tally.NET remote access needs the company connected to Tally.NET and per-user remote rights. [Remote access help](https://help.tallysolutions.com/remote-access-tally/) [V]
- TallyPrime Cloud Access is a separate add-on from ₹450–750 per user for each period. [TCO page] [V]

**KhataOS answer:** Local-first with built-in sync. Each device holds a full local copy (SQLite/CRDT or an event log) and syncs peer-to-peer on the LAN, or through an optional relay (self-hosted or ours) over the internet. No VLAN, VPN, RDP or virtual-user tax. Works offline and merges when back online. Role-based access applies per user, not per seat.

### 3. Data corruption, repair and backup (Sev 5)

**Official documentation**
- Tally's repair recovers what it can, and "data that can't be recovered is removed". [Repair FAQ](https://help.tallysolutions.com/repairing-company-data-faq/) [S]
- Tally documents many corruption error messages:
  - "Error Code 0" when TranMgr.900 is 1 KB
  - "Diskette one is damaged" for a bad backup
  - "Record Insertion Failure"
  - tstate.tsf errors when third-party backup software locks files
  - Repair needs free disk space of 2x the data size
  - Malware-corrupted data "may be unrecoverable"
  - Source: [Migration/corruption FAQ](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/data-migration-data-corruption-faq/) [V]
- "Memory Access Violation" can come from corrupt data, printer drivers, or antivirus locking `.tsf`/`.1800` files. [MAV FAQ](https://help.tallysolutions.com/memory-access-violation-faq/) [S]

**What users say**
- Forum threads cover: power cut without UPS, OS failure, confusing folder copies, corrupted backups. [CAclubindia 2009](https://www.caclubindia.com/forum/why-data-gets-corrupted-in-tally-23174.asp) [V]
- "EXITING TALLY … TR01313.500 … VERY URGENT." [CAclubindia 2009](https://www.caclubindia.com/forum/file-corrupt-39438.asp) [V]
- 14 of the 473 Tally sub-forum titles are about corruption, recovery, restore or backup, for example ["tally data recovery"](https://www.caclubindia.com/forum/tally-data-recovery-27738.asp) and ["Restore in tally"](https://www.caclubindia.com/forum/restore-in-tally-150949.asp). [titles]
- "Automatic backup option needed as files are gets lost if system gets corrupted." [Techjockey, Jan 2021](https://www.techjockey.com/reviews/tally-erp-9) [V]
- Data can go missing after a power failure even though the company still opens. [Bison KB (partner)](https://knowledgebase.bison.co.in/view_article.php?id=1791) [S]
- Automatic cloud backup (TallyDrive) exists but needs TSS. [TCO page] [V]

**KhataOS answer:**
- An append-only, journaled ledger: SQLite in WAL mode, or event-sourced, with checksummed pages. A power cut can never leave a half-written voucher.
- Continuous local snapshots, plus optional encrypted off-site backup to the user's own drive, S3 or Google Drive, at no extra licence cost.
- `khata verify` runs an integrity check and `khata restore --at <time>` does point-in-time restore. Every synced device is also a full backup.
- No proprietary binary format. Data is always exportable to open formats (SQLite, CSV, JSON).

### 4. CA–client data exchange (Sev 4)

**What users and vendors say**
- CA firms historically rely on pendrives, email or WhatsApp backups and TeamViewer. The problems: "multiple versions of the same data", "version confusion", missing transactions, and confidentiality risk. [tallyatcloud, CA author, Jan 2026 (vendor)](https://www.tallyatcloud.com/article/modern-method-used-by-ca-firms-to-access-client-tally-data-without-pendrive-or-teamviewer/930/0/1) [V]

**Release mismatch between client and CA**
- Opening data in a higher release converts it (`.1800` files). The lower release then can't open it: "Restore a backup taken before sharing and use compatible versions." [Tally FAQ](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/data-migration-data-corruption-faq/) [V]
- In practice, a CA on the latest release can lock a client out of their own data. `[U]` as a frequency claim.

**Other points**
- CAs need a separate Auditor Edition (TSS ₹6,750/yr). [TCO page] [V]
- ICAI members get a discount through a Tally–ICAI initiative. [ICAI CMP](https://www.icai.org/post/19535) [S]
- Clients are wary of the cloud: "Clients are hesitant to upload sensitive financial data to third-party cloud SaaS platforms." [ICAI AI use case](https://ai.icai.org/usecases_details.php?id=183) [V] This strongly supports local-first.

**KhataOS answer:**
- "Invite your CA": the client grants a scoped, revocable, end-to-end-encrypted sync share, either for a period range or read-only.
- The CA's adjustments come back as *proposed journal entries* that the client approves, all recorded in the audit trail.
- The data format is versioned and backward/forward compatible. No "opened in newer version, now broken".
- Air-gapped CAs get a signed `.khata` bundle (encrypted, diffable) instead of a raw backup.
- One CA workspace lists all clients.

### 5. Data import/export and integration (Sev 4)

**What users say**
- Excel import is the single biggest theme in the CAclubindia Tally sub-forum: 33 of 473 titles. Examples include ["How to import large data from excel"](https://www.caclubindia.com/forum/how-to-import-large-data-from-excel-to-tally-468664.asp) and ["Excel to Tally FREE version"](https://www.caclubindia.com/forum/display.asp?forum_id=25&cat_id=9). [V]
- The Experts section repeats the pattern ("Import of data in tally erp9", "excel to tally"). [Experts IT list](https://www.caclubindia.com/experts/browse.asp?cat_id=9) [V]
- The forum answers to the large-import question: "go for customisation of tally software", or third-party add-ins. [CAclubindia](https://www.caclubindia.com/forum/how-to-import-large-data-from-excel-to-tally-468664.asp) [V]
- Exporting is a pain too: several threads ask how to turn Tally's "Dr/Cr" export into +/- signed numbers in Excel. [titles]

**Official and developer evidence**
- Native mapped Excel import arrived only with TallyPrime 4.0 (2023/24). [Tally](https://tallysolutions.com/apac/tally/import-data-from-excel-to-tallyprime/) [S]
- Developer pain, from the most-used open-source extractor:
  - Tally must be running with the company active.
  - Tally "sometimes returning stale data after running for days" (needs a restart).
  - Batches over 10,000 records "can cause exports to get stuck".
  - Windows only.
  - Source: [tally-database-loader](https://github.com/dhananjay1405/tally-database-loader) [V]

**KhataOS answer:**
- First-class CSV/XLSX import with saved column mappings, dry-run preview and row-level errors.
- Importers for Tally XML/TallyPrime backups, Busy and Zoho.
- A documented local HTTP/JSON API and CLI, plus the open SQLite schema for direct BI reads (Power BI, Metabase, Sheets).
- Signed amounts by default in exports.

### 6. Version upgrade and migration (Sev 4)

**Official documentation**
- Data from older versions cannot go straight to Prime; it must pass through Tally.ERP 9 first. [Tally help](https://help.tallysolutions.com/migrate-from-older-tp-versions-to-latest/) [V]
- Once migrated, data works only in the latest release. Saving in an older one crashes the app. Forcibly stopping a migration "might damage the data".
- Migration can lose edit-log details for masters or vouchers with exceptions, and increases data size.
- Other documented migration errors: "Bad unit name", wrong stock values, and opening BRS entries older than 6 months deleted.
- Source for the three points above: [Migration FAQ](https://help.tallysolutions.com/tally-prime/data-management-tally-prime/data-migration-data-corruption-faq/) [V]
- ERP 9 TDLs break in Prime: "existing toolbar buttons shall fail to load", and masters need rework. [Tally dev ref](https://help.tallysolutions.com/working-of-tally-erp-9-customisations-with-tallyprime/) [V]

**What users say**
- Users report migrated companies with no ledgers, "Failed to Post to: https://127.0.0.1:9090", and "loading failed" (2009–2016). [CAclubindia](https://www.caclubindia.com/forum/tally-9-data-migration-problem-22994.asp) [V]
- Another migration error report: [CAclubindia](https://www.caclubindia.com/forum/error-message-in-data-migration-139029.asp) [title]
- Searches found no first-hand complaints specific to the ERP 9 → Prime move; Reddit was blocked. `[U]`

**KhataOS answer:**
- Schema migrations are automatic, reversible and tested, with a pre-migration snapshot taken every time.
- Old clients can still read newer files read-only.
- A one-click Tally importer reads ERP 9/Prime data through the XML export, with a reconciliation report: trial balance, voucher counts and closing stock compared side by side.
- Plugins target a stable, versioned API, so upgrades don't break customisations.

### 7. GST compliance workflow gaps (Sev 4)

**GSTR-2B reconciliation**
- CAs build VBA and ChatGPT tools because the Tally purchase register and the portal 2B Excel have different headers, number formats and date formats. Small typos and rounding cause missed ITC. [ICAI use case 183](https://ai.icai.org/usecases_details.php?id=183) [V]
- Another ICAI use case covers the same problem. [ICAI use case 84](https://ai.icai.org/usecases_details.php?id=84) [S]
- Recent CAclubindia Software-forum titles: "Tired converting GSTR-1, 2B & 3B to Clean Excel?" and "GST Reconciliation even if Invoice Number Mismatch!". [titles, forum 117](https://www.caclubindia.com/forum/display.asp?forum_id=117&cat_id=54)
- Tally auto-downloads 2A/2B only from Release 5.0, as a TSS "connected" feature. [Tally](https://tallysolutions.com/gst/gstr-2b-reconciliation-mismatches-how-to-fix/) [S]

**e-invoice and e-way bill**
- Rounding CGST and SGST separately makes the portal reject invoices: "GST Tax amount mismatched and its Rejected". [CAclubindia, Sep 2022](https://www.caclubindia.com/forum/round-off-gst-amount-and-e-invoice-way-bill-rejection-in-tally-prime-593693.asp) [V]
- A duplicate-IRN error is documented. [Tally](https://help.tallysolutions.com/duplicate-irn-error/) [S]

**GSTR-9/9C**
- TallyPrime gives a "GST Annual Computation" report. Filing happens in the portal's offline utility. [Tally help](https://help.tallysolutions.com/file-gstr-9c-tally/) [S]
- Older ERP 9 did not support GSTR-9 at all. [Tally FAQ](https://help.tallysolutions.com/article/favourites/internal/faq-gst-annual-returns.htm) [S]
- No first-hand 9/9C complaints were found. `[U]`

**KhataOS answer:**
- A built-in 2B/2A/IMS reconciliation engine with fuzzy invoice-number matching, tolerance rules and ITC-at-risk reports. It runs locally, so the privacy concern goes away.
- e-invoice JSON built with a portal-identical rounding model and validated before upload.
- GSTR-1/3B/9/9C working papers drilling down to vouchers, and portal-format JSON exports.
- GSP connectivity as an optional plug-in, never required to produce returns.

### 8. Performance with large data and over networks (Sev 4)

**What users say**
- 5 companies (1.1 GB), 6 users, an i7 with 32 GB RAM and NVMe: "saving a voucher takes at least 45 to 90 seconds". [CodeWeavers forum](https://codeweavers.com/compatibility/crossover/forum/tally-9?msg=306572) [S]
- "Speed goes down in client computers." [Techjockey, Aug 2023](https://www.techjockey.com/reviews/tally-erp-9) [V]
- From a CFO: "big task to download reports and vouchers when the data is in excess." [same] [V]

**Official guidance**
- Requests from clients are processed one at a time, ping must stay under 5 ms, and the data folder must be excluded from antivirus. [Tally perf checklist](https://help.tallysolutions.com/checklist-for-troubleshooting-performance-issues-of-tallyprime/) [V]
- Load time worsens as data grows. [Bison KB](https://knowledgebase.bison.co.in/view_article.php?id=762) [S]
- Splitting company data by year is the standard workaround. Examples: ["split company data!"](https://www.caclubindia.com/forum/split-company-data--39606.asp) [title] and [Tally blog](https://tallysolutions.com/smart-moves/expert-insights/can-your-accounting-software-handle-high-transaction-volumes-an-expert-performance-guide/) [S]

**KhataOS answer:**
- An indexed relational store with materialised balances, so reports run in O(log n) and don't replay vouchers.
- Benchmark target: 1M vouchers with trial balance in under 1 s.
- No "split company" needed: years are views over a single book.
- Clients read their local replica, so the network never sits in the voucher-save path.

### 9. Windows-only: no Mac or Linux (Sev 3)

- The desktop app is Windows-only; Mac, Chromebook and other users are pointed to Cloud Access. [Tally FAQ](https://tallysolutions.com/tally-prime-faq-guide/) [S]
- Parallels works but may not cover every TDL, bank connector, DSC token or printer driver. [Gaurav Tiwari](https://gauravtiwari.org/tally-on-mac/) [S]
- "All the softwares I use run only on windows like MS office, tally…", cited as the reason not to switch to Linux. [TechEnclave, Dec 2025](https://techenclave.com/t/whats-the-1-pain-point-which-made-you-to-switch-to-linux-for-whatever-you-use-pc/407302) [V]

**KhataOS answer:** Native builds for Windows, macOS (Apple Silicon) and Linux, from one Rust core (the repo already uses Cargo). Small binaries that run on old office PCs. DSC/USB token signing goes through PKCS#11 on all three platforms.

### 10. Mobile access (Sev 3)

- An owner wants an iOS app that reads Tally over the LAN "without syncing it into the cloud". The suggested Biz App was rejected because "data syncs to the cloud". [CAclubindia, Mar–May 2024](https://www.caclubindia.com/forum/mobile-app-with-tally-prime-lan-access-605988.asp) [V]
- Official mobile access is reports in a browser, which needs TSS. [Tally](https://tallysolutions.com/tally/tallyprime-mobile-access-business-reports/) [S], [TVU FAQ](https://help.tallysolutions.com/tally-virtual-user-tvu-faq/) [V]
- It is view-only, with no entry creation. [fitgap (third-party)](https://us.fitgap.com/products/tallyprime) [S]
- SoftwareSuggest's sentiment summary flags "limited cloud/mobile access". [SoftwareSuggest](https://www.softwaresuggest.com/tally-erp-9) [V]

**KhataOS answer:** Android and iOS apps that are full peers in the sync mesh. They read and write locally and sync over the LAN or an encrypted relay, with no mandatory cloud. A trimmed UI for owners covers dashboards, receivables and approving CA entries. Sales staff can create invoices and receipts offline.

### 11. Customisation through TDL and partners (Sev 3)

- Capterra reviewers report "very limited customization for larger scale industries". [Capterra UK](https://www.capterra.co.uk/reviews/127762/tally-erp-9) [S]
- "It has only one type of Invoice format." [Techjockey retail owner, Jun 2023](https://www.techjockey.com/reviews/tally-erp-9) [V]
- 10 of 473 CAclubindia Tally titles are TDL requests (running balance, address book, detailed balance sheet), and there is a "TDL Codes available for Sale" market. [CAclubindia](https://www.caclubindia.com/forum/tdl-codes-available-for-sale-243675.asp) [title]
- Tally's own marketing says freelance TDL scripts "often crash during a Tally version upgrade". [Tally](https://tallysolutions.com/business-guides/tallyprime-integrations-without-any-custom-development/) [S]
- New invoice templates need Release 7.1 and TSS. [TCO page] [V]

**KhataOS answer:**
- Invoice and report templates in HTML/CSS (Tera/Handlebars) that users can edit, with a visual designer.
- Report builder: pick ledgers or groups, columns, period comparisons, then save and share.
- A plugin SDK (WASM, sandboxed) against a versioned API, so customisations survive upgrades.
- A community template gallery in the repo.

### 12. Audit trail / MCA edit log (Sev 3)

**The rule**
- Rule 3(1) of the Companies (Accounts) Rules applies from 1 Apr 2023. Software must log every change and the log must not be disableable. [TaxGuru](https://taxguru.in/company-law/demystifying-audit-trail-edit-log.html) [S], [Tally](https://tallysolutions.com/accounting/audit-trail-rule-in-accounting-software-from-1st-april-2023/) [S]

**How Tally handles it**
- Edit Log exists only from Release 2.1, so ERP 9 users had to upgrade, which needs TSS.
- The "TallyPrime Edit Log" variant cannot be disabled and its log "cannot be removed or deleted". In regular Release 2.1 it is optional.
- Data grows "marginally", and voucher save is slower.
- Source for the three points above: [Edit Log FAQ](https://help.tallysolutions.com/edit-log-in-tallyprime-faq/) [V]
- Renumbering vouchers creates one log entry per renumbered voucher. [Tally help](https://help.tallysolutions.com/?p=155414) [S]
- No first-hand user complaints about the edit log were found (Reddit was blocked). `[U]`

**KhataOS answer:** Immutable, hash-chained change log at the storage layer for every master and voucher: who, when, before and after, which device. It is always on and tamper-evident, with an auditor's verification report ("log intact for FY"). It is cheap because it is the event log itself. Renumbering is a single logged operation.

### 13. Bank reconciliation and bank feeds (Sev 3)

- Live Connected Banking works for only Axis, Kotak, SBI and ICICI. Other banks need statement file import (Excel, MT940 or CSV). Auto-reconciliation needs Release 6.0+ and TSS. [Tally banking guide](https://tallysolutions.com/business-guides/tallyprime-banking-integration-auto-bank-feeds-auto-brs-payments/) [S]
- Matching only links statement lines to vouchers that already exist; it does not create vouchers from the statement. [invoicedataextraction (vendor)](https://invoicedataextraction.com/blog/import-bank-statements-into-tallyprime) [S]
- Forum titles: ["Auto Bank Reconciliation not working"](https://www.caclubindia.com/forum/auto-bank-reconciliation-not-working-153596.asp) and ["Automatic bank reco in tally"](https://www.caclubindia.com/forum/automatic-bank-reco-in-tally-272438.asp). [titles]

**KhataOS answer:** A statement importer for every major Indian, UAE and KSA bank format (CSV, XLS, PDF, MT940, CAMT.053). It runs rule-based and learned matching, creates vouchers from unmatched lines ("bank rules"), handles split matches, and produces a BRS report. Optional Account Aggregator / bank API feeds are plug-ins, not a paywall.

### 14. TDS/TCS (Sev 3)

- Tally's TDS FAQ lists items "not supported yet" under the new Income-tax Act 2025 formats, such as error T_FV_6455 for 10-character certificate numbers. FVU validation is done outside Tally. [TDS FAQ](https://help.tallysolutions.com/tds-faq-tally/) [S], [Filing help](https://help.tallysolutions.com/tds-filing-returns-tally/) [S]
- Payroll TDS goes wrong from setup errors (HRA declarations), and Form 16/16A challan questions go unanswered. [CAclubindia 2011](https://www.caclubindia.com/forum/tally-tds-not-calculating-right-170913.asp) [V]
- Low volume of complaints. `[U]` for any current-release issues.

**KhataOS answer:** A TDS/TCS engine with section masters, thresholds (194Q and others), lower-deduction certificates, and challan matching to the OLTAS/CSI file. It generates 24Q/26Q/27Q/27EQ files and runs the FVU locally as a bundled step. Rule tables are data files, so a rule change is a data update, not a paid upgrade.

### 15. Passwords, user access and security (Sev 2)

- 10 of 473 CAclubindia Tally titles are password recovery: ["Tally password recovery"](https://www.caclubindia.com/forum/tally-password-recovery-242609.asp) and ["Lost your Tally Password?"](https://www.caclubindia.com/forum/lost-your-tally-password--4514.asp). The Experts section adds "Break tally vault password". [titles]
- Another thread asks for help with users and security in Tally. [CAclubindia](https://www.caclubindia.com/forum/hepl-needed-on-users-and-security-issues-in-tally-222849.asp) [title]
- A vendor assessment calls Tally's access controls "somewhat limited". [fitgap](https://us.fitgap.com/products/tallyprime) [S]

**KhataOS answer:** Owner recovery key (printed or exported at setup) and multi-admin recovery. Per-user roles scoped by company, voucher type, ledger group and date lock, plus OS keychain integration. Encryption at rest with a key the user controls.

### 16. UAE VAT / KSA ZATCA (Sev 3 for roadmap; low evidence)

**UAE**
- Tally claims EmaraTax-integrated VAT 201 filing in TallyPrime 7.1. [TechAfricaNews, Jun 2026](https://techafricanews.com/2026/06/30/tally-solutions-launches-tallyprime-7-1-with-uae-e-invoicing-and-emaratax-integration/) [S]
- Tally's own pages conflict on whether it is an accredited Peppol ASP or still awaiting accreditation. [Tally MENA](https://tallysolutions.com/mena/uae-vat/tallyprime-peppol-certified-provider-for-uae-einvoicing/) [S]
- ClearTax says TallyPrime "does not natively submit invoices to the FTA; you need an ASP connector". [ClearTax AE (competitor)](https://www.cleartax.com/ae/uae-e-invoicing-tally-integration) [S]

**KSA**
- Tally claims Phase 1 and Phase 2 (FATOORA) support, but a Sep 2023 page said Phase 2 was still to come. [Tally KSA](https://tallysolutions.com/mena/features/e-invoicing/) [S]

No GCC user complaints were found. `[U]`

**KhataOS answer:** A jurisdiction-pack architecture: the India GST pack ships first; UAE VAT/CT and KSA VAT/ZATCA are separate packs. Each pack holds tax codes, return forms, e-invoice formats (Peppol PINT-AE, ZATCA UBL + cryptographic stamp) and Arabic/RTL bilingual invoices. ASP/Fatoora connectors are pluggable. Check the UAE MoF ASP list and the ZATCA SDK before committing to dates.

### 17. Multi-currency (Sev 2)

- Year-end forex totals didn't match primary amounts; the thread was unresolved in 2011. Another user says Tally care gave "alternative ways to handle the issue". [CAclubindia](https://www.caclubindia.com/forum/tally-9-exchange-rate-trouble--157673.asp) [V]
- Unadjusted forex gain/loss stays on the Balance Sheet until someone passes a manual journal. [Tally guide](https://tallysolutions.com/business-guides/multi-currency-in-tally-erp-9/) [S], [CAclubindia](https://www.caclubindia.com/forum/unadjusted-forex-gain-loss-in-tally-17124.asp) [title]

**KhataOS answer:** Per-transaction currency with daily rate tables (RBI/CBUAE/SAMA import), automatic realised and unrealised revaluation entries at period close, and reports in any currency. Rates are stored as exact decimals, with no float rounding.

### 18. UI learnability vs shortcut power (Sev 2, double-edged)

**Praise for the keyboard**
- "Massive popularity of… tally is because of mouse-less handling of the whole interface." [HN](https://news.ycombinator.com/item?id=26099093) [V]
- "Tally… has an interface from 1995. That's how they remained productive." [HN](https://news.ycombinator.com/item?id=22082990) [V]
- Shops "use ERPs (Tally) with 90s UI that dont need mouse". [HN 2025](https://news.ycombinator.com/item?id=44795076) [V]

**Complaints**
- SoftwareSuggest's sentiment summary cites an "outdated UI". [SoftwareSuggest](https://www.softwaresuggest.com/tally-erp-9) [V]
- There is "a learning curve for individuals lacking prior experience". [SoftwareReviews](https://cdn1.softwarereviews.com/products/tallyprime) [S]
- Ease of use is still a top-rated strength (G2 4.5/5, 277 reviews). [G2 compare](https://www.g2.com/compare/oracle-peoplesoft-vs-tally) [S]
- The move from ERP 9 to Prime changed shortcuts, for example Alt+F1 became Ctrl+F3, and Go To/Switch To replaced menu walking. [Tally shortcuts](https://help.tallysolutions.com/keyboard-shortcuts/keyboard-shortcuts-tally-prime) [S]
- Retraining complaints are `[U]`.

**KhataOS answer:** Keyboard-first by design: every action reachable without a mouse, and a command palette. An optional "Tally keymap" profile (F4–F9 voucher keys, Alt+C, Ctrl+A and so on) lets trained staff move over with zero retraining; see `docs/research/tally-shortcuts.md`. Pair it with a modern discoverable UI (inline hints, search-anything) so new staff learn quickly.

---

## Cross-cutting takeaways for KhataOS

1. **The moat is the subscription gate, not the features.** Most "connected" pain (GST, e-invoice, banking, remote, mobile, backup, upgrades) traces back to TSS. "Free statutory updates, forever" is the headline.
2. **Local-first is a selling point, not a compromise.** CAs and clients distrust third-party cloud ([ICAI](https://ai.icai.org/usecases_details.php?id=183)) and want mobile access without cloud sync ([CAclubindia](https://www.caclubindia.com/forum/mobile-app-with-tally-prime-lan-access-605988.asp)). Sync without a mandatory cloud answers both.
3. **The Tally importer and Tally keymap are adoption-critical.** Migration fear and shortcut muscle memory are the switching costs.
4. **Reconciliation (2B, bank, CA adjustments) is where accountants lose time.** Build it as a first-class engine, not a report.

## Open gaps / to verify

- A logged-in Reddit pass, to confirm frequency for #1, #2, #4, #6 and #9 and find ERP 9 → Prime and edit-log complaints.
- Direct reads of the G2, Capterra and TrustRadius cons sections (blocked with 403 here).
- The "TSS expired before 12 Jun 2023 cannot upgrade to 3.0" claim (search snippet only).
- Current UAE ASP accreditation status (MoF list) and ZATCA Phase 2 wave obligations.
- A Cloud Access price unit (per user per month vs per period). The TCO page is ambiguous.

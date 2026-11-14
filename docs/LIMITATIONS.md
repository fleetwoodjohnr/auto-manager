# Feature completion and limitations

This report covers version 0.1.0 as of 2026-11-14. Status: **Done** works and was verified as shown; **Partial** works with the stated gap; **Not included** is missing, with the reason. Verification codes:

- **U**: unit and integration tests in `cargo test` (226 tests).
- **E2E**: the real app driven through WebDriver (`e2e/`).
- **L**: live tests against the real services (2026-11-14).
- **M**: manual check of screenshots.

## Platform, data and updates

| Spec item | Status | Notes / verification |
|---|---|---|
| Tauri 2 desktop app: React + TypeScript, Tailwind, Rust, SQLite | Done | E2E on Linux x64 (Fedora 44, WebKitGTK 2.54) |
| Windows x64/ARM64, macOS Intel/Apple silicon, Linux ARM64 | Partial | Release CI builds all six natively and runs `--self-test` on each, but **those builds haven't been run yet**: they need the GitHub repository. Only Linux x64 was built and run here |
| Packages: NSIS, DMG, AppImage, DEB, RPM | Partial | AppImage, DEB and RPM built locally on Linux x64 ([TEST_REPORT](TEST_REPORT.md)). Windows and macOS packages come from CI |
| Flatpak | Not included | Optional in the spec. It would need its own manifest and Flathub's update route; documented in RELEASING.md |
| Simple Suite design: tokens, typography, navigation, dialogs, settings, icon | Done | Tokens from Simple Setup; petrol accent; icon in the family template. M; WCAG 2.2 A/AA audit with axe in both themes (E2E) |
| Light/dark/system theme, persisted | Done | E2E |
| SQLite: migrations, indexes, foreign keys, transactions | Done | U: every upgrade path, FK check, refuse newer schema |
| Separate manufacturer, user and enrichment data | Done | One row per source; the user's corrections are separate rows. U, E2E |
| Offline use | Done | Saved lookups keep working, with a *stale* marker on network failure. U (offline fallback), E2E (Workflow A reopened with no provider) |
| Backup: manual, scheduled, restore, integrity, documents | Done | Zip with SHA-256 manifest; verified restore; daily automatic backups (14 kept). U (round trip, tamper, truncation, zip-slip) |
| CSV/JSON export | Done | Inventory CSV/XLSX; full JSON; reports CSV/XLSX. U |
| Optional encrypted sync | Partial | **Encrypted bundles** move a complete copy between computers (Argon2id + ChaCha20-Poly1305). U: export/import between two data folders, wrong passphrase, tampering. **Two-way merge sync isn't implemented**; its design is in ARCHITECTURE.md |
| GitHub Releases updates: semver, notes, download, verify, install, restart, channels, no downgrades, data kept | Done (Linux AppImage); Partial (other formats) | **Workflow E** passed locally with real AppImages: interrupted download refused, wrong signature refused, good update installed, backup made, restarted as the new version with data intact (E2E). Stable/beta channel selection and DEB/RPM/NSIS/DMG installs use the Tauri updater's supported routes, but **haven't been run** until real releases exist (checklist in RELEASING.md) |
| Release CI: lint, tests, audit, packaging, signing, tags, changelog, manifests | Done (written); not yet run | `ci.yml`, `release.yml`. The release script verifies every signature and requires all 10 platform entries (U: `release-manifest.test.mjs`) |
| Code signing (Apple notarization, Windows) | Partial | Wired to optional secrets. Without certificates, Gatekeeper and SmartScreen warn on first run |
| Language setting | Not included | English only. A language picker with one choice would be a dead control; the setting shows "English" with an explanation |
| No telemetry, local by default, HTTPS, input validation, safe imports | Done | See SECURITY.md. U (XXE refusal, zip-slip, formula injection, non-HTTPS refusal, file sniffing) |
| OS keyring for API credentials | Not included (not needed) | No built-in provider needs a credential. The pattern to use when one does is documented in PROVIDERS.md |

## Vehicle lookup and specifications

| Spec item | Status | Notes / verification |
|---|---|---|
| 17-character VIN with check-digit validation; I/O/Q rejected | Done | A wrong check digit is a warning, as it should be for non-North-American VINs. U, E2E |
| Partial VIN | Done | `*` wildcards; not saved. U (fixture), L |
| Year/make/model/trim; older and non-standard vehicles | Done | Makes and models suggested from vPIC; free entry offline. E2E (smoke), U |
| Saved vehicles | Done | E2E |
| Barcode/QR VIN entry | Partial | USB scanners work (they type into the field). **Camera-based VIN or barcode recognition isn't included**: WebKitGTK has no BarcodeDetector, and camera access in desktop webviews is inconsistent across platforms |
| CSV bulk VIN import | Done | Batches of 50 (vPIC's documented limit, sent as POST, the only method it accepts), one batch at a time. U (mocked POST), L |
| Every reliably identified attribute | Done | All vPIC values are kept; known ones are mapped to labelled fields. Unknown fields show "Not available from free sources". Nothing is inferred. U, E2E |
| Horsepower/torque "when verified" | Partial | Horsepower only where vPIC has it (VIN-decoded). No free source of torque |
| Dimensions, capacities, maintenance specs | Partial | Dimensions and curb weight from vPIC Canadian specs; engine and fuel economy from the EPA (model-level, the person picks the configuration). Fluids, capacities, plugs, battery, tire pressures and service intervals have **no free authoritative source**. They're entered from the owner's manual with the page cited |
| Sources and confidence on every value; edits kept apart from originals | Done | E2E (Workflow A), U |
| Conflict detection | Done | Disagreeing providers are flagged, ignoring wording differences such as Gasoline vs Regular Gasoline or 4x2 vs FWD. U |

## Providers

| Spec item | Status | Notes |
|---|---|---|
| NHTSA vPIC | Done | U, L, E2E |
| NHTSA Recalls | Done | **Model-level**, clearly labelled "potentially applicable", with the per-VIN result recorded by the person after checking nhtsa.gov. U, L, E2E |
| NHTSA manufacturer communications (TSBs) | Not included | NHTSA publishes them only as a bulk download, with no per-vehicle query (checked 2026-11-14: the open-data endpoint is non-tabular). The Recalls & Safety tab links to nhtsa.gov; imported TSB PDFs are supported |
| NHTSA Safety Ratings, vPIC Canadian specs, EPA fuel economy | Done | Additional free sources. U, L |
| OEM, licensed specification, aftermarket catalog and supplier APIs | Not included | Commercial agreements needed; nothing to test against. No placeholder adapters. Extension points and imports documented in PROVIDERS.md |
| ACES/PIES | Done (import) | PIES creates and updates parts. ACES needs the user's VCdb export (licensed reference data). U (hand-written ACES 4.2 / PIES 7.2-structured samples, not files from a real catalog) |
| Priorities, retries, rate limits, caching, attribution, freshness | Done | U (429 retry, `Retry-After`, give-up, no retry on 4xx, offline, cache, stale) |
| Plugin system | Partial | Providers are separate modules behind a small documented contract. Runtime-loadable plugins aren't included |

## Parts, fitment and inventory

| Spec item | Status | Notes |
|---|---|---|
| Hierarchical taxonomy covering all listed systems | Done | 19 systems with components; user-added subcategories |
| Search by VIN, YMM, name, OEM/aftermarket/SKU/barcode/interchange number, brand, category | Done | Normalized number matching. U, E2E |
| Part records (numbers, specs, install info, warranty, photos, compatible vehicles, supplier, prices, stock) | Done | Photos and diagrams as attachments |
| Licensed exploded-view diagrams | Not included | Needs a licensed source; your own diagrams can be attached or imported as PDFs |
| Fit statuses (Confirmed, Conditional, Possible, Not compatible, Unknown), qualifiers, exclusions, supersession, traceable records, manual verification | Done | Never a fit from make and model alone. U (fitment matrix), E2E (Workflow B) |
| Cross-references, alternatives, related products, comparison | Partial | Supersession and interchange: Done. "Commonly replaced components" and a side-by-side product comparison aren't included |
| Saved parts lists, kits/assemblies | Partial | Parts lists: Done (E2E). Kits/assemblies aren't modelled |
| Inventory records and all core operations; ledger; transactions | Done | Receive, issue, transfer, reserve, release, consume, returns, adjustments, counts, damage, undo. U (negative stock, concurrent reservations from 8 threads, rollback, consume-once, stale edits, duplicate SKU), E2E |
| Quantities | Partial | **Whole units only.** Bulk or partial quantities (e.g. oil by the litre) aren't supported; stock bottles or drums instead |
| Valuation method | Partial | Moving weighted average or last purchase cost. No FIFO/LIFO layers |
| Parts in Stock for a vehicle: confidence, alternatives, missing details, reserve, estimate, open, restock | Done | E2E (Workflow B) |
| Barcode/QR: USB scanners, manual entry, printable labels | Done | Code 128 or QR labels. Camera scanning not included (see above) |
| Bulk import/export with mapping, validation, duplicates | Done | CSV/XLSX/XLS/ODS. U |
| Purchasing: suppliers, numbers, POs, expected deliveries, receiving, cost history, preferred, comparison, backorders | Done | U (suggestions, partial receipt, over-receipt refused, locked sent orders) |

## Pricing, service, maintenance

| Spec item | Status | Notes |
|---|---|---|
| Profiles; fixed, markup and margin; category, brand and item rules; quantity breaks; rounding; minimum prices; tax; explained prices; decimal maths | Done | U ($50 at 40% markup → $70; 40% margin → $83.33; precedence; rounding; precision), E2E |
| Customer discounts and customer profiles, labor rates | Done | Customer discount and profile applied to new estimates; labor rate per profile |
| Estimates, jobs, invoices, PDF | Done | PDF via the print dialog's Save as PDF / Print to File; there is no separate PDF writer. U (totals, tax, never double-deducted, rollback), E2E |
| Repair-shop workflow, steps 1–11 | Done | Completing a job consumes stock once and adds it to maintenance history |
| Maintenance records, intervals, due items | Done | Intervals are the person's, from their manual. **No manufacturer schedules are shipped**: none are freely available. U |
| Vehicle garage, customer vehicles, photos, documents | Done | |
| Recalls: campaign, component, description, consequence, remedy, date, link | Done | |
| OBD-II code reference | Done | 9,533 generic codes (OBDex, CC0). Causes labelled "possible, not a diagnosis". Manufacturer-specific codes not included |
| OBD-II connectivity (optional) | Partial | Read-only ELM327 over serial ports: stored and pending codes, MIL, freeze frame, 12 live values, VIN. **Tested against a scripted adapter only, not real hardware.** Bluetooth works when the OS exposes the adapter as a serial port. No guided diagnostics |

## Manuals and documentation

| Spec item | Status | Notes |
|---|---|---|
| Import PDFs; associate with vehicles; library | Done | E2E |
| Viewer: zoom, search, contents, bookmarks, page navigation, print, saved page references, notes | Done | Search jumps to matching pages but doesn't highlight the words on the page. Printing prints the current page; to print a whole manual, use your PDF reader. E2E |
| Automatic manual discovery | Partial | Verified links to official owner-manual sites for 12 makes (link-checked 2026-11-14). Sites that refused an automated check aren't listed. Nothing is scraped or downloaded |
| Where Is It? | Done | From the vehicle's documents, saved component locations and part installation notes only; says so when nothing is recorded. E2E (Workflow C) |
| Scanned (image-only) PDFs | Partial | Viewable, not searchable (no OCR) |

## Interface

| Spec item | Status | Notes |
|---|---|---|
| Dashboard with all listed sections and shortcuts | Done | Sections can be shown or hidden. They can't be reordered by dragging |
| Global search: autocomplete, typos, recent, filters, routing, Ctrl/⌘ K | Done | Saved searches on the Inventory and Parts lists. U, E2E |
| Tables: search, sort, filter, virtualized, right-click menus, keyboard | Done | Tested with the E2E data; not benchmarked with very large catalogs |
| Undo | Partial | Stock movements (once each, from the toast or the item's history). Other edits are confirmed instead |
| Resizable panels | Partial | The document viewer's sidebar |
| Keyboard shortcuts | Partial | Ctrl/⌘ K, Alt+1–0, arrows and Enter in tables and search. No full shortcut map |
| Unit conversion | Done | Miles/km, lb/kg, gal/L, qt/L, psi/kPa, hp/kW, lb·ft/N·m, in/mm, for display |
| Reports with CSV, XLSX and PDF | Done | From stored records; inventory turnover uses today's value (no history of daily values). U (every report runs) |
| Data Quality Center | Done | U |

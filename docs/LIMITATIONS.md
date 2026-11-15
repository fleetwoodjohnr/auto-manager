# Feature completion and limitations

This report covers version 0.1.0 as of 2026-11-14. Status: **Done** works and was verified as shown; **Partial** works with the stated gap; **Not included** is missing, with the reason. Verification codes:

- **U**: unit and integration tests in `cargo test`.
- **E2E**: the real app driven through WebDriver (`e2e/`).
- **CI**: GitHub Actions on Linux x64/ARM64, Windows x64/ARM64 and macOS Intel/Apple silicon.
- **L**: live tests against the real services (2026-11-14).
- **M**: manual check of screenshots.

## What's left

| Gap | Why |
|---|---|
| OBD-II with a real adapter | Tested end to end through a real serial port against a stand-in ELM327 (CR line endings, echo, chunked and multi-frame replies), in the app and in CI. Not yet confirmed with physical adapters, whose quirks vary by make and clone |
| Per-VIN open-recall status | No public API exists. NHTSA's VIN check is an interactive web page only; the app links to it and records your answer |
| Free torque, fluid and maintenance databases | None exists. Auto Manager **finds these values in your own manuals** and saves them with the page cited (below) |
| Windows and macOS code signing | Needs paid certificates; first-run Gatekeeper/SmartScreen warnings until then. The update signature check is separate and always on |
| Camera scanning on Windows/macOS | Uses each platform's webview permission prompt; only Linux (WebKitGTK) was exercised here. Choosing a photo works everywhere |

## Platform, data and updates

| Spec item | Status | Notes / verification |
|---|---|---|
| Tauri 2 desktop app: React + TypeScript, Tailwind, Rust, SQLite | Done | E2E on Linux x64 (Fedora 44, WebKitGTK 2.54) |
| Windows x64/ARM64, macOS Intel/Apple silicon, Linux ARM64 | Done (built and tested) | CI builds and runs the full test suite natively on all six. The release workflow also builds the installers and runs `--self-test` on each |
| Packages: NSIS, DMG, AppImage, DEB, RPM | Done (Linux packages verified here) | AppImage, DEB and RPM built and checked locally ([TEST_REPORT](TEST_REPORT.md)). Windows and macOS installers come from the release workflow |
| Flatpak | Not included | Optional in the spec. Needs its own manifest and Flathub's update route; documented in RELEASING.md |
| Simple Suite design: tokens, typography, navigation, dialogs, settings, icon | Done | M; WCAG 2.2 A/AA audit with axe in both themes (E2E) |
| Light/dark/system theme, persisted | Done | E2E |
| **Languages** | Done | English, Spanish and French for the whole interface and the core's fixed messages. Chosen in Settings → General (default: the system language). A unit test fails if any text lacks a translation or loses a `{0}` slot; E2E renders every page in Spanish and French. Data from NHTSA/EPA stays in English as published, and core messages that carry values (e.g. "Only 3 available") stay in English |
| SQLite: migrations, indexes, foreign keys, transactions | Done | U: every upgrade path, FK check, refuse newer schema; migration 005 tested on a version-4 database with stock |
| Separate manufacturer, user and enrichment data | Done | One row per source; the user's corrections are separate rows. U, E2E |
| Offline use | Done | Saved lookups keep working, with a *stale* marker on network failure. U, E2E |
| Backup: manual, scheduled, restore, integrity, documents | Done | Zip with SHA-256 manifest; verified restore; daily automatic backups (14 kept). U |
| CSV/JSON export | Done | Inventory CSV/XLSX; full JSON (quantities in units); reports CSV/XLSX. U |
| **Two-way sync between computers** | Done | Through a shared folder (Syncthing, Dropbox, OneDrive, network share, USB stick). Each computer publishes its full state, encrypted (Argon2id + ChaCha20-Poly1305); changes merge per record (latest change wins, deletions kept). Repairs: references to deleted records, the same VIN/SKU/document added twice (merged into one), clashing job/invoice/order numbers (later one renumbered), stock recomputed from the ledger with one reported correction when two computers took the same stock, a job completed on both deducted once. U (7 two-computer scenarios; the double-deduction test was confirmed to fail when its safeguard is removed); E2E (two app instances through one folder). Device-local settings (theme, language, update channel) and the audit history aren't synced |
| Encrypted bundles (move everything) | Done | Still available for a one-way copy |
| GitHub Releases updates: semver, notes, download, verify, install, restart, channels, no downgrades, data kept | Done (Linux AppImage); Partial (other formats) | **Workflow E** passed locally with real AppImages (E2E). Stable/beta selection and DEB/RPM/NSIS/DMG installs use the Tauri updater's supported routes, but **haven't been run** until real releases exist (checklist in RELEASING.md) |
| Release CI | Done | `release.yml` published [v0.1.0](https://github.com/fleetwoodjohnr/auto-manager/releases/tag/v0.1.0) from all six native runners (each ran `--self-test`): 10 signed update packages, 2 DMGs, `latest.json`, `SHA256SUMS`. The manifest script verifies every signature (U) |
| No telemetry, local by default, HTTPS, input validation, safe imports | Done | See SECURITY.md |
| OS keyring for API credentials | Not included (not needed) | No built-in provider needs a credential |

## Vehicle lookup and specifications

| Spec item | Status | Notes / verification |
|---|---|---|
| 17-character VIN with check-digit validation; I/O/Q rejected | Done | U, E2E |
| Partial VIN | Done | U, L |
| Year/make/model/trim; older and non-standard vehicles | Done | U, E2E |
| **Barcode/QR and photo VIN entry** | Done | **Scan…** next to the VIN field: live camera or a photo. Door-jamb barcodes (Code 39/128, Data Matrix, QR) are decoded with rxing; printed or stamped VINs are read with bundled text recognition (ocrs). A VIN is only offered when its check digit is right, or one common misreading (S/5, B/8, Z/2, G/6) away, which is labelled. U (Code 39 label, rendered VIN-plate photo, false-positive guards); E2E (camera opens and frames are read via WebKit's mock camera; the bundled models read a VIN photo in the running app). Inventory's Scan tab reads part barcodes the same way. About 1–2 s per picture |
| CSV bulk VIN import | Done | U, L |
| Every reliably identified attribute | Done | U, E2E |
| Horsepower/torque and maintenance specifications | Done (from your manuals) | Horsepower from vPIC where it has it. **Torque, fluid capacities, oil grade, tire pressures, wheel-nut torque, brake fluid, battery group and spark-plug gap are found in your imported manuals** (Full Specifications → *Find values in your manuals*), shown with their page and context, and saved as "From a manual" with the page cited only when you choose. U (a capacities page; no engine torque taken from a drain-plug figure; nothing from unrelated text) |
| Dimensions | Done | vPIC Canadian specs and EPA, model-level |
| Sources and confidence; conflict detection | Done | U, E2E |

## Providers

| Spec item | Status | Notes |
|---|---|---|
| NHTSA vPIC, Recalls, Safety Ratings, Canadian specs, EPA | Done | U, L, E2E |
| **NHTSA manufacturer communications (TSBs)** | Done | Per model year/make/model from `api.nhtsa.gov/vehicles/byYmmt`, searchable on Recalls & Safety, with the public bulletin PDFs (static.nhtsa.gov) addable to the library. Model-level, labelled so. U (123 bulletins for a 2019 Sentra; document links only from static.nhtsa.gov), L (list → documents → PDF download) |
| Per-VIN recall status | Not available | See *What's left* |
| OEM, licensed specification, aftermarket catalog and supplier APIs | Not included | Commercial agreements needed |
| ACES/PIES | Done (import) | ACES needs your VCdb export |

## Parts, fitment and inventory

| Spec item | Status | Notes |
|---|---|---|
| Taxonomy, search, part records, fit statuses, supersession | Done | U, E2E |
| Inventory operations, ledger, transactions | Done | U (including 8 concurrent writers), E2E |
| **Quantities** | Done | Items can be **sold in parts of a unit** (oil by the litre, hose by the foot) with a unit of measure; stored as exact thousandths so the no-negative-stock and no-over-reservation checks stay exact. Whole-unit items still refuse 1.5. Applies to stock, reservations, counts, purchase orders, part lists, job lines, imports and reports. U (4.5 L received, 0.25 issued, 1.75 reserved, all exact; average cost exact; existing stock migrated) |
| Valuation method | Partial | Moving weighted average or last purchase cost. No FIFO/LIFO layers |
| Barcode/QR: USB scanners, camera, photos, printable labels | Done | See scanning above |
| Bulk import/export | Done | CSV/XLSX/XLS/ODS, with an optional unit column. U |
| Purchasing | Done | U |
| Photos and documents on records | Done | Vehicles, parts, items and jobs; files checked by content and stored by hash. U, E2E |

## Pricing, service, maintenance

| Spec item | Status | Notes |
|---|---|---|
| Pricing engine with explanations | Done | U, E2E |
| Estimates, jobs, invoices, PDF | Done | PDF via the print dialog |
| Maintenance records, intervals, due items | Done | **Intervals can be found in your imported manuals** ("every 5,000 miles or 6 months"; km converted) and added with the page cited. No generic schedules are shipped. U |
| OBD-II code reference | Done | 9,533 generic codes (OBDex, CC0) |
| OBD-II connectivity | Done (simulated adapter); physical adapters not yet tried | Read-only ELM327 over serial ports; any port path can be typed (e.g. a Bluetooth `/dev/rfcomm0` the system doesn't list). U and E2E (Linux) against `crates/obd/tests/elm327_sim.py` on a pseudo-terminal (macOS pseudo-terminals can't take serial settings, so macOS and Windows weren't exercised): VIN, stored codes, warning light, live values, freeze frame, unsupported data left out, never a clear-codes request. This test found and fixed a bug real adapters would have hit (replies end lines with a bare CR) |

## Manuals and interface

| Spec item | Status | Notes |
|---|---|---|
| Library, viewer, Where Is It? | Done | E2E |
| Automatic manual discovery | Partial | Verified links to official owner-manual sites for 12 makes; service bulletins from NHTSA |
| Scanned (image-only) PDFs | Partial | Viewable, not searchable (no OCR of whole manuals) |
| Dashboard, global search, saved searches, tables, units, reports, Data Quality Center | Done | U, E2E |
| Undo | Partial | Stock movements; other edits are confirmed instead |
| Keyboard shortcuts | Partial | Ctrl/⌘ K, Alt+1–0, arrows and Enter in tables and search |

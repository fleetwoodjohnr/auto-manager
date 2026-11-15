# Changelog

All notable changes are recorded here. Versions follow [Semantic Versioning](https://semver.org). The release workflow publishes the section for each version as its release notes and in-app update notes.

## [Unreleased]

- **QuickBooks import.** Settings → QuickBooks imports a QuickBooks Online or Desktop export (products and services, customers, vendors). You see what will be added, updated or skipped before anything is saved. Stock items get their QuickBooks price, cost, quantity, reorder point and preferred vendor. Importing again later updates the same records instead of adding copies; stock and cost only follow QuickBooks if you tick it. QuickBooks itself is never changed.
- **QuickBooks Online connection (built, switched on once Intuit approves).** Connect to QuickBooks: sign in in your browser, pick the company, and **Bring in from QuickBooks…** reads the same data straight from QuickBooks Online with the same review. The sign-in is kept in your system's keyring. Until a release carries Intuit's production keys, the screen says it isn't available yet and the export import does the same job.

## [0.1.1] - 2026-11-14

- **Updates work like the other Simple Suite apps.** Auto Manager checks GitHub at start-up and once a day (when the setting is on) and shows a notice when a new version is out. **Review update** opens Settings → Updates, where **Install and restart** downloads it, checks its signature, backs up your data and restarts. Nothing installs until you choose to. The beta channel and automatic downloads were removed. Installing waits until unsaved changes are saved or discarded.
- **Every installer is now tested on its own platform before a release.** Each package is installed the way you would install it (the Windows installer, the macOS disk image, the .deb on a clean Ubuntu, the .rpm on a clean Fedora, the AppImage), started, updated through the real updater (a tampered package must be refused) and uninstalled.

## [0.1.0] - 2026-11-14

First release.

- **Vehicle lookup.** NHTSA vPIC VIN decoding with check-digit validation, partial VINs, bulk CSV/Excel decoding, and identification without a VIN. Every value shows its source and confidence. Values no source provides are marked unavailable, never guessed. Your corrections are stored beside the originals. Offline copies of everything already looked up.
- **More vehicle data.** NHTSA recalls (by model, with a per-VIN result you record), NHTSA 5-Star ratings, vPIC Canadian dimensions and EPA engine and fuel-economy data (model-level, you pick the matching configuration).
- **Parts and fitment.** Hierarchical catalog. OEM, aftermarket, barcode, interchange and supplier numbers. Fitment records with conditions, and confirmed, conditional, possible, not compatible and unknown statuses. Supersession and interchange. Spreadsheet, ACES (with your VCdb export) and PIES imports.
- **Inventory.** Ledger-based stock with locations and bins, reservations, counts, damage quarantine, returns, undo, weighted-average or last-cost valuation, imports with column mapping, labels, and a USB barcode scan field.
- **Pricing and service.** Pricing profiles with explained prices. Estimates, jobs and invoices, with parts consumed from stock exactly once. Customers. Printable estimates and invoices.
- **Maintenance.** Intervals from your manual, service history, and what's due.
- **Manuals.** PDF library and viewer with search, contents, bookmarks, notes and printing. "Where Is It?" component lookup. Verified official owner-manual links.
- **Diagnostics.** OBD-II code reference (OBDex, CC0). Read-only ELM327 scanner.
- **Business.** Suppliers, prices and comparison, reorder assistant, purchase orders and receiving, reports with CSV/Excel/PDF output, Data Quality Center.
- **Scanning.** Read a VIN with the camera or from a photo (door-jamb barcodes and printed plates, check-digit verified), and part barcodes in Inventory.
- **Service bulletins.** NHTSA manufacturer communications for each model, searchable, with their PDFs.
- **From your manuals.** Torque, fluid capacities, tire pressures and other specifications, and service intervals, found in your imported manuals and saved with the page cited.
- **Fractional quantities.** Items sold by the litre, foot or kilo, with units.
- **Sync between computers.** Two-way, encrypted, through a shared folder; conflicts in stock and invoice numbers are corrected and reported.
- **Languages.** Spanish and French.
- **Attachments and saved searches.** Photos and PDFs (receipts, invoices, diagrams) on vehicles, parts, items and jobs. Saved filters on the Inventory and Parts lists.
- **Data.** Daily automatic backups, verified restore, encrypted transfer bundles, JSON export.
- **Updates.** Signed updates from GitHub Releases, with stable and beta channels.

# Auto Manager

Find a vehicle → understand its specifications → find its parts and manuals → check inventory → manage pricing and service.

Auto Manager is a free, open-source (MIT) desktop app for Windows, macOS and Linux. Decode a VIN with NHTSA's free vPIC service and see every specification it returns, with where each value came from and what isn't known. Then:

- match the vehicle against your parts' fitment records;
- see what's in stock for it, and reserve it or add it to an estimate;
- open the manuals you've imported, and find components in them ("Where Is It?");
- run estimates, service jobs and invoices with pricing rules that explain themselves.

It's local-first: no account, no telemetry, and your data stays on your computer. Online lookups are saved, so anything you've looked up keeps working offline.

It's part of the Simple Suite family (Simple Cell, Simple Docs, Simple Viz, Simple Setup) and shares their design.

## What it does

| Area | Highlights |
|---|---|
| Vehicle Lookup | 17-character VIN with check-digit validation, partial VINs (`*`), **scanning the VIN with the camera or from a photo** (door-jamb barcodes and printed plates), CSV/Excel bulk decode (50 per request), identification without a VIN for older or non-US vehicles |
| Specifications | Every vPIC value, grouped, with its source and confidence. "Not available from free sources" where nothing is known. Your corrections are kept beside the original values. Dimensions (vPIC Canadian specifications) and engine and fuel economy (EPA), marked model-level |
| Parts & fitment | Hierarchical catalog with OEM, aftermarket, barcode and interchange numbers. Fitment records with conditions. Fit statuses: **Confirmed**, **Conditional**, **Possible**, **Not compatible**, **Unknown**. Never a fit from make and model alone. Supersession, interchange, and ACES/PIES and spreadsheet import |
| Inventory | Whole or fractional quantities with units (oil by the litre). Movement ledger (receive, issue, transfer, adjust, count, damage, returns) with database-enforced no-negative stock. Reservations that are never deducted twice. Undo. Multiple locations and bins. Weighted-average or last cost. CSV/Excel import with column mapping and duplicate detection. Labels (Code 128 or QR) |
| Pricing | Profiles with fixed, markup or margin rules by category, brand, item and quantity, plus minimum prices, rounding and tax. Every price shows how it was calculated. Decimal arithmetic throughout |
| Service | Estimates → approved → in progress → completed (stock consumed) → invoiced. Customers, labor, fees, discounts, printable estimates and invoices (Save as PDF) |
| Maintenance | Service intervals and specifications (torque, capacities, tire pressures…) found in your own manuals with the page cited, service history, what's due across the garage |
| Manuals | PDF library with a built-in viewer: zoom, search, contents, bookmarks, notes, print. Linked to vehicles, full-text searchable, with "Where Is It?" for components. Verified links to official owner-manual sites |
| Safety | NHTSA recall campaigns by model (clearly marked as *potentially* applicable), a per-VIN result you record, the model's technical service bulletins with their PDFs, and NHTSA 5-Star ratings |
| Diagnostics | Generic OBD-II code reference (OBDex, CC0). Read-only ELM327 scanner: codes, freeze frame, live data, VIN. Never clears codes |
| Purchasing | Suppliers, supplier part numbers and prices, price comparison, a reorder assistant, purchase orders, receiving, backorders, cost history |
| Reports | Inventory value, stock status, movement, usage, slow movers, turnover, revenue and margins, estimate conversion, supplier spending, vehicle history and cost. Print to PDF, CSV or Excel. Data Quality Center |
| Attachments | Photos and PDFs (receipts, invoices, diagrams) on vehicles, parts, inventory items and jobs |
| Data | Automatic daily backups, verified restore, **two-way encrypted sync between computers through a shared folder**, full JSON export |
| Languages | English, Spanish and French |
| Updates | Signed updates from GitHub Releases, stable or beta channel, checks and downloads only with your permission |

The full list of what's done and what isn't, and how each part was verified, is in **[docs/LIMITATIONS.md](docs/LIMITATIONS.md)**.

## Install

Download the file for your system from the [latest release](https://github.com/fleetwoodjohnr/auto-manager/releases/latest):

| System | File | Updates |
|---|---|---|
| Windows 10/11 (x64, ARM64) | `…-setup.exe` (per-user install, no admin needed) | In-app |
| macOS 11+ (Apple silicon, Intel) | `.dmg` | In-app |
| Linux (x64, ARM64) | `.AppImage` (any distribution) | In-app |
| Debian, Ubuntu, Mint | `.deb` | In-app (asks for your password to run `dpkg`) |
| Fedora, RHEL, openSUSE | `.rpm` | In-app (asks for your password to run `rpm`) |

Check a download against `SHA256SUMS` on the release page.

Unless the release was built with signing certificates, Windows SmartScreen and macOS Gatekeeper may warn the first time you open it. On Windows, choose **More info → Run anyway**. On macOS, choose **System Settings → Privacy & Security → Open Anyway**.

## Where your data lives

Everything is in one folder:

- Linux: `~/.local/share/dev.automanager.desktop`
- macOS: `~/Library/Application Support/dev.automanager.desktop`
- Windows: `%APPDATA%\dev.automanager.desktop`

The folder holds `auto-manager.db` (SQLite), `documents/` (imported PDFs), `attachments/`, `backups/` and `logs/`. **Settings → Data and backups** shows the exact path. Set `AUTO_MANAGER_DATA_DIR` to use another folder (for a portable install, or for testing).

## Documentation

- [User guide](docs/USER_GUIDE.md): the everyday workflows
- [Data providers](docs/PROVIDERS.md): what each source supplies, its terms, and how to add one
- [Architecture](docs/ARCHITECTURE.md) and [database](docs/DATABASE.md)
- [Development](docs/DEVELOPMENT.md): build, test, the end-to-end suite
- [Releasing](docs/RELEASING.md): signing keys, GitHub secrets, channels, and update behaviour per package type
- [Security](SECURITY.md)
- [Feature completion and limitations](docs/LIMITATIONS.md), and the [test report](docs/TEST_REPORT.md)

## License

MIT. See [LICENSE](LICENSE). Vehicle data from NHTSA and the EPA is U.S. Government work. Trouble-code descriptions come from [OBDex](https://github.com/foerbsnavi/OBDex) (data CC0 1.0).

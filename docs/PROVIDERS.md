# Data providers

Auto Manager only shows vehicle data that some source actually supplied. Every value is stored with its source, its confidence and the date it was fetched. When two sources disagree, the one with the lower priority number wins (**Settings → Vehicle data**), and the disagreement is flagged in the vehicle's specifications and in the Data Quality Center. Your own entries always win, and the original source values stay on record.

None of the built-in providers needs an account or an API key.

## Built in

| Provider | Supplies | How specific | Terms | Verified |
|---|---|---|---|---|
| **NHTSA vPIC**: `DecodeVinValuesExtended`, `DecodeVINValuesBatch` (POST, ≤ 50 VINs), `GetMakesForVehicleType`, `GetModelsForMakeYear` | Make, model, year, trim, series, body, vehicle class, doors, plant, engine (configuration, displacement, cylinders, fuel, engine model/code, HP where NHTSA has it), transmission style and speeds, drive type, axles, brake system, wheelbase, track width, curb weight, GVWR class, seats, wheel sizes, safety equipment, plus NHTSA's own error text | **VIN-decoded**: what the manufacturer reported to NHTSA for that VIN pattern. Missing means NHTSA has no data, not that the feature is absent | U.S. Government work, public domain; free, no key; rate-controlled automatically by NHTSA | Live test `live_vpic_decode_and_lists`, recorded fixtures, E2E |
| **NHTSA Recalls**: `api.nhtsa.gov/recalls/recallsByVehicle` | Campaign number, component, summary, consequence, remedy, notes, report date, park-it / park-outside / over-the-air flags | **Model-level.** Campaigns for that model year, make and model. NHTSA's public API doesn't say whether a campaign is open for one VIN. The app links to nhtsa.gov/recalls for that check and stores the answer you record | Public domain; free | Live test, fixtures, E2E |
| **NHTSA 5-Star Safety Ratings**: `api.nhtsa.gov/SafetyRatings` | Overall, frontal, side and rollover ratings, rollover risk, ESC/FCW/LDW availability, complaint, recall and investigation counts | Model variant (e.g. "4 DR FWD") | Public domain; free | Live test, fixtures |
| **NHTSA Manufacturer Communications**: `api.nhtsa.gov/vehicles/byYmmt?data=manufacturerCommunications`, `safetyIssues/byNhtsaId` | Technical service bulletins and other notices manufacturers sent NHTSA: number, date, components, summary, and the bulletin PDFs on static.nhtsa.gov (downloaded only when you choose *Add to library*) | **Model-level**: a bulletin may cover only some configurations | Public domain; free | Live test, fixtures, E2E |
| **vPIC Canadian Vehicle Specifications** | Overall length, width, height, wheelbase, curb weight, track widths, weight distribution | **Model-level**: you pick the matching body style | Public domain; free | Live test, fixtures |
| **EPA fueleconomy.gov** web services | Engine displacement and cylinders, transmission, drive, fuel, city/highway/combined MPG, EV range, size class | **Model-level**: you pick the matching configuration | Public domain; free | Live test, fixtures |
| **OBDex**: generic OBD-II codes (bundled) | 9,533 SAE-range code titles, descriptions, possible causes with likelihood, symptoms, related codes, references | Generic codes; manufacturer-specific codes need licensed data | Data **CC0 1.0** ([LICENSE-DATA](https://github.com/foerbsnavi/OBDex/blob/main/LICENSE-DATA), checked 2026-11-14). English fields only are bundled | Unit test on the bundled data |
| **Official owner-manual portals** (links only) | A link to the manufacturer's owner-information site, which opens in your browser | Make | Each manufacturer's terms. Auto Manager never downloads, copies or redistributes manuals | `scripts/check-doc-links.sh`. Only links that answered HTTP 200 on 2026-11-14 are included |

Answers are cached in the database (`api_cache`). A saved answer is used without a request until it expires (Settings → Vehicle data → "Refresh saved answers after"). If the network fails, an expired copy is used and marked **stale**.

## Your own data

| Source | Format | Notes |
|---|---|---|
| Manual entry | Forms | Stored as "Entered by you" or "From a manual (page …)" |
| Inventory spreadsheet | CSV, XLSX, XLS, ODS | Column mapping, row-by-row preview, duplicate detection (by SKU), one transaction |
| Fitment spreadsheet | CSV/XLSX with `part, kind, year_from, year_to, make, model, position, quantity, notes`; any other column is a condition | Optionally marked as from an authoritative catalog (makes records *verified*) |
| **PIES** XML (Auto Care Association product data) | `Item` → name/description (DES/SHO, EXT/MKT/FAB), brand, part number, GTIN, attributes, prices, interchange numbers | Creates or updates catalog parts (matched by part number and brand) |
| **ACES** XML (Auto Care Association fitment) | `App` → BaseVehicle (or Years/Make/Model ids), VCdb attributes, part number, quantity, position, notes | **Needs your VCdb export.** ACES identifies vehicles by VCdb ids, which are licensed and not included. Supply a vehicle map (BaseVehicleID, Year, Make, Model) exported from your licensed VCdb, and optionally an attribute map (`attribute,id,key,value`, e.g. `EngineBase,1234,engine_displacement_l,2.0`). Unresolvable applications are reported and skipped. Unmapped attributes become conditions no vehicle meets yet (listed in the Data Quality Center). Delete actions aren't applied automatically |
| Supplier price list | CSV/XLSX with SKU or supplier part number, and cost | Updates supplier prices; unmatched lines are listed |
| **QuickBooks export** (Online or Desktop) | Excel/CSV: Products and Services or Item List, Customer list, Vendor list | Recognized from the headers (report title lines and footers passed over). Inventory items become stock items with their price, cost, quantity, reorder point and vendor. Linked, so importing again updates the same records |
| **QuickBooks Online** (live) | Intuit's Accounting API, minor version 75: `Item`, `Customer`, `Vendor`, `CompanyInfo` queries, 1,000 per page | OAuth 2.0 sign-in in your browser (scope `com.intuit.quickbooks.accounting`; Intuit has no read-only scope, but Auto Manager only ever sends GET requests to the API). Needs the Intuit app's keys in the build (see SECURITY.md) and, for real companies, Intuit's production approval. Tokens: refresh token in the OS keyring, rotated on every refresh; access token in memory only |
| PDF documents | PDF up to 512 MB | Copied into the library by content hash; text indexed for search |

## Read from your own documents

| What | How | Notes |
|---|---|---|
| Torque, fluid capacities, oil grade, tire pressures, wheel-nut torque, brake fluid, battery group, spark-plug gap | *Find values in your manuals* on Full Specifications | Searched only in the searchable text of documents linked to the vehicle; each value is shown with its page and context and saved as "From a manual" with the page cited, only when you choose it |
| Service intervals | *Find intervals in your manuals* on Maintenance | "every N miles (or km) or M months" lines, added with the page cited when you choose them |
| VINs and barcodes in photos | *Scan…* | Barcodes decoded with rxing (Apache-2.0). Printed VINs read with ocrs text-recognition models (code MIT/Apache-2.0; **model weights CC BY-SA 4.0**, trained on Google's HierText dataset), fetched at build time by `scripts/fetch-ocr-models.mjs` with pinned SHA-256s and shipped in the app's resources with their license |

## Not included (and why)

| Kind of source | Status |
|---|---|
| OEM manufacturer databases (build data, OEM part catalogs) | Not public. They need commercial agreements, which the project doesn't have and can't test |
| Licensed specification databases (fluids, capacities, torque) | Commercial, per-seat licensing. Maintenance specifications are therefore filled from your manuals, with the page cited |
| Aftermarket catalog and supplier APIs (pricing, availability) | Per-supplier contracts. The supplier price-list import covers the common case |
| Per-VIN open-recall status | Only the manufacturer and nhtsa.gov/recalls (one VIN at a time, interactive) provide it; no public API exists for all makes (checked 2026-11-14) |
| Manufacturer-specific trouble codes | Licensed data |

No adapters are shipped for these, so nothing in the app pretends to use them.

## Adding a provider

Providers are plain modules in `crates/core/src/providers/`. Each one has:

1. a **parser** from the provider's response to Auto Manager types (`DecodedValue { key, value, unit, source_ref }` for specifications), unit-tested against a recorded response in `crates/core/tests/fixtures/`;
2. a **fetch** function that goes through `Core::cached_text` (or `cached_json_accept`). That gives HTTPS-only requests, timeouts, per-host throttling, retries with backoff on 429/5xx honouring `Retry-After`, the offline cache and stale fallback;
3. a row in `data_sources` (a migration) with its name, priority, URL and license;
4. a **live test** in `crates/core/tests/live.rs` (ignored by default);
5. a base-URL entry in `providers::Endpoints` so E2E tests can point it at `e2e/fixture-server.mjs`.

Store specification values with `vehicles::store_values(conn, vehicle_id, source_id, confidence, values)`. That replaces the provider's earlier values and leaves the user's own untouched.

Providers that need credentials should store them with the operating-system keyring (`keyring-core` with the per-OS stores, as in Simple Connect's `security` module), never in the database or settings. No current provider needs one, so the keyring code isn't included yet.

## Live check

```sh
AUTO_MANAGER_LIVE=1 cargo test -p auto-manager-core --test live -- --ignored --test-threads=1
```

The results of each run are recorded in [TEST_REPORT.md](TEST_REPORT.md).

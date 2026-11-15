# Database

SQLite (bundled, via rusqlite), one file: `auto-manager.db` in the data folder.

The connection is configured with WAL journaling, `synchronous=FULL`, foreign keys on and a 5 s busy timeout. All tables are `STRICT`. IDs are UUIDv7 text. Money and decimals are TEXT parsed by `rust_decimal`. Times are UTC ISO-8601; dates are `YYYY-MM-DD`.

## Migrations

Migrations live in `crates/core/migrations/`, and `PRAGMA user_version` is the schema version.

- `001_foundation.sql`: settings, audit, sources, cache, vehicles and specifications, catalog and fitment, inventory, pricing, jobs, lists, manuals, attachments, search.
- `002_intelligence.sql`: recalls and recall status, supersession, interchange, maintenance, trouble codes.
- `003_business.sql`: supplier prices, purchase orders, saved searches.
- `004_advanced.sql`: OBD sessions.
- `005_quantities_sync.sql`: the service-bulletin source; quantities in thousandths of a unit (stock, movements, reservations, part lists, order lines, minimum and reorder levels) and `inventory_items.fractional`; the sync tables.
- `006_external_links.sql`: `external_links (system, entity, external_id) → local_id`, which links imported QuickBooks records (`qbo:<company id>` for QuickBooks Online, `qb-file` for exports) to the inventory item, customer or supplier they became. Synced between computers, so a refresh on either one updates instead of copying. The QuickBooks Online sign-in itself is never in the database: the refresh token is in the OS keyring and the connection details in `quickbooks.json` in the data folder (this computer only).

How they run:

- Each migration runs in its own transaction, followed by a foreign-key check.
- Before an existing database is upgraded, a `pre-migration-v{N}` backup is written.
- A database from a newer version is refused, with a message to update the app.
- Shipped migrations are never edited.

## Main tables

| Table | What it holds |
|---|---|
| `vehicles` | Identity (cached from effective specs), owner, mileage, purchase, notes, `saved` (garage) vs recent lookup |
| `vehicle_specifications` | One row per (vehicle, key, source): value, unit, confidence (`vin_decoded`, `model_level`, `user_entered`, `manual_ref`), reference, fetched_at |
| `vehicle_effective_specs` (view) | The winning value per key, by `data_sources.priority` (user = 0) |
| `vehicle_configuration` (view) | The effective attributes fitment matches against |
| `data_sources` | Providers, imports and the user, with priority and license |
| `api_cache` | Provider answers by URL, with fetch and expiry times |
| `part_categories` | Hierarchical system → component taxonomy |
| `parts`, `part_identifiers` | Catalog parts; numbers by kind with a normalized form for matching |
| `part_fitments`, `fitment_qualifiers` | Fits/excludes, year range, make and model, plus conditions; verification and source |
| `part_supersessions`, `part_interchanges` | Replacement chains; interchangeable pairs |
| `inventory_items` | Stocked products (SKU, condition, last and average cost, fixed price, profile, min/reorder, supplier, version for optimistic locking) |
| `inventory_locations`, `stock_levels` | Locations (including quarantine); per-location on hand and reserved with `CHECK` constraints |
| `inventory_movements` | The ledger: signed quantity, kind, cost, transfer group, reference, reversal link (each movement can be reversed once) |
| `stock_reservations` | Active, released or consumed holds, linked to a job, line or vehicle |
| `pricing_profiles`, `pricing_rules` | Tax, rounding, labor rate; rules by scope, method, priority, minimum quantity and price |
| `customers`, `service_jobs`, `service_lines` | Customers; estimates, jobs and invoices with tax terms copied in; lines with price explanations and stock links |
| `supplier_parts`, `purchase_orders`, `po_lines` | Supplier numbers and costs; orders and received quantities (`CHECK received <= ordered`) |
| `maintenance_intervals`, `maintenance_records` | Schedules entered from manuals; service history (records from completed jobs are added automatically) |
| `recalls`, `recall_status` | Campaigns by year/make/model; the per-VIN result the user recorded |
| `manuals`, `manual_vehicle_links`, `manual_pages` (FTS5), `manual_annotations` | Documents, what they apply to, page text, bookmarks, notes and component locations |
| `dtc_codes` | Bundled OBD-II code reference |
| `obd_sessions`, `obd_readings` | Stored scans |
| `search_docs`, `search_index` (FTS5 trigram) | Global search, kept current by triggers |
| `settings`, `audit_events`, `part_lists`, `part_list_items`, `attachments`, `saved_searches` | |
| `sync_log`, `sync_clock`, `sync_aliases` | Two-way sync (device-local, never synced themselves): the version of every synced row or deletion, the hybrid logical clock and the trigger switch, and rows merged into another |

**Quantities** are INTEGER thousandths of a unit (`4500` = 4.5), so the `CHECK`s and sums stay exact; the app shows and exports them in units (`crates/core/src/qty.rs`).

## Backups

`VACUUM INTO` gives a consistent snapshot. The snapshot is zipped with `documents/` and `attachments/` and a manifest of SHA-256 checksums.

Backups are written:

- automatically once a day (the newest 14 are kept);
- before every migration, update and restore;
- on demand, from Settings.

A restore extracts into a scratch folder. It checks checksums, paths, `integrity_check`, `foreign_key_check` and the schema version, and upgrades the copy, before swapping it in. The previous files are kept as `previous-<time>/`.

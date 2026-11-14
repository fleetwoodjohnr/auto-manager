-- Auto Manager schema v1: foundation.
-- Append-only: never edit a migration that has shipped; add a new one.
-- IDs are UUIDv7 text. Money and other decimals are TEXT parsed by rust_decimal. Times are UTC ISO-8601.

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL            -- JSON
) STRICT;

CREATE TABLE audit_events (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  entity    TEXT NOT NULL,
  entity_id TEXT,
  action    TEXT NOT NULL,
  summary   TEXT NOT NULL,
  detail    TEXT                 -- JSON
) STRICT;
CREATE INDEX audit_events_entity ON audit_events(entity, entity_id);

-- Where a value came from. priority: lower wins when two sources disagree; the user is 0.
CREATE TABLE data_sources (
  id       TEXT PRIMARY KEY,
  name     TEXT NOT NULL,
  kind     TEXT NOT NULL CHECK (kind IN ('user', 'provider', 'import')),
  priority INTEGER NOT NULL,
  url      TEXT,
  license  TEXT,
  enabled  INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
INSERT INTO data_sources (id, name, kind, priority, url, license) VALUES
  ('user', 'Entered by you', 'user', 0, NULL, NULL),
  ('nhtsa_vpic', 'NHTSA vPIC', 'provider', 10, 'https://vpic.nhtsa.dot.gov/api/', 'U.S. Government work, public domain');

CREATE TABLE api_cache (
  key        TEXT PRIMARY KEY,   -- request URL without credentials
  source_id  TEXT NOT NULL,
  body       TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
) STRICT;

CREATE TABLE pricing_profiles (
  id                 TEXT PRIMARY KEY,
  name               TEXT NOT NULL UNIQUE COLLATE NOCASE,
  kind               TEXT NOT NULL DEFAULT 'custom' CHECK (kind IN ('retail', 'wholesale', 'internal', 'fleet', 'customer', 'custom')),
  is_default         INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  tax_rate_pct       TEXT NOT NULL DEFAULT '0',
  prices_include_tax INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0, 1)),
  rounding_increment TEXT NOT NULL DEFAULT '0.01',
  rounding_mode      TEXT NOT NULL DEFAULT 'nearest' CHECK (rounding_mode IN ('nearest', 'up', 'down', 'ending')),
  rounding_ending    TEXT,       -- for 'ending': e.g. '0.99'
  labor_rate         TEXT,
  notes              TEXT NOT NULL DEFAULT '',
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX pricing_profiles_one_default ON pricing_profiles(is_default) WHERE is_default = 1;
-- A profile with no rules: the user sets their own markup or margin (nothing is assumed).
INSERT INTO pricing_profiles (id, name, kind, is_default) VALUES ('pp-default-retail', 'Default Retail', 'retail', 1);

CREATE TABLE pricing_rules (
  id          TEXT PRIMARY KEY,
  profile_id  TEXT NOT NULL REFERENCES pricing_profiles(id) ON DELETE CASCADE,
  priority    INTEGER NOT NULL DEFAULT 100,
  scope       TEXT NOT NULL CHECK (scope IN ('all', 'category', 'brand', 'item')),
  scope_value TEXT,              -- category id, brand name, or inventory item id
  method      TEXT NOT NULL CHECK (method IN ('fixed', 'markup', 'margin')),
  value       TEXT NOT NULL,     -- price for 'fixed', percent for 'markup'/'margin'
  min_qty     INTEGER NOT NULL DEFAULT 1 CHECK (min_qty >= 1),
  min_price   TEXT,
  enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  note        TEXT NOT NULL DEFAULT '',
  CHECK ((scope = 'all') = (scope_value IS NULL))
) STRICT;
CREATE INDEX pricing_rules_profile ON pricing_rules(profile_id, priority);

CREATE TABLE customers (
  id                 TEXT PRIMARY KEY,
  name               TEXT NOT NULL,
  phone              TEXT NOT NULL DEFAULT '',
  email              TEXT NOT NULL DEFAULT '',
  address            TEXT NOT NULL DEFAULT '',
  notes              TEXT NOT NULL DEFAULT '',
  pricing_profile_id TEXT REFERENCES pricing_profiles(id) ON DELETE SET NULL,
  discount_pct       TEXT,
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- year/make/model/trim/engine are a cache of the effective specifications (refreshed in code)
-- so lists and search stay cheap. saved = 0 means "looked up recently", 1 = in the garage.
CREATE TABLE vehicles (
  id             TEXT PRIMARY KEY,
  vin            TEXT,
  year           INTEGER,
  make           TEXT NOT NULL DEFAULT '',
  model          TEXT NOT NULL DEFAULT '',
  trim           TEXT NOT NULL DEFAULT '',
  engine         TEXT NOT NULL DEFAULT '',
  transmission   TEXT NOT NULL DEFAULT '',
  nickname       TEXT NOT NULL DEFAULT '',
  color          TEXT NOT NULL DEFAULT '',
  plate          TEXT NOT NULL DEFAULT '',
  mileage        INTEGER CHECK (mileage IS NULL OR mileage >= 0),
  mileage_unit   TEXT NOT NULL DEFAULT 'mi' CHECK (mileage_unit IN ('mi', 'km')),
  purchase_date  TEXT,
  purchase_price TEXT,
  notes          TEXT NOT NULL DEFAULT '',
  customer_id    TEXT REFERENCES customers(id) ON DELETE SET NULL,
  saved          INTEGER NOT NULL DEFAULT 0 CHECK (saved IN (0, 1)),
  decode_note    TEXT NOT NULL DEFAULT '',   -- provider's own error/warning text, as returned
  last_viewed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX vehicles_vin ON vehicles(vin) WHERE vin IS NOT NULL;
CREATE INDEX vehicles_recent ON vehicles(saved, last_viewed_at);
CREATE INDEX vehicles_customer ON vehicles(customer_id);

-- One row per (vehicle, key, source). User corrections are their own rows (source 'user'),
-- so a provider's original value is never overwritten.
CREATE TABLE vehicle_specifications (
  id         INTEGER PRIMARY KEY,
  vehicle_id TEXT NOT NULL REFERENCES vehicles(id) ON DELETE CASCADE,
  key        TEXT NOT NULL,
  value      TEXT NOT NULL,
  unit       TEXT,
  source_id  TEXT NOT NULL REFERENCES data_sources(id),
  confidence TEXT NOT NULL CHECK (confidence IN ('vin_decoded', 'model_level', 'user_entered', 'manual_ref')),
  source_ref TEXT,               -- provider variable name, or "manual <id> p.<n>"
  fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  UNIQUE (vehicle_id, key, source_id)
) STRICT;

-- The value that wins for each key: the lowest source priority (the user's own entry first).
CREATE VIEW vehicle_effective_specs AS
SELECT s.vehicle_id, s.key, s.value, s.unit, s.source_id, s.confidence, s.source_ref, s.fetched_at
FROM vehicle_specifications s JOIN data_sources d ON d.id = s.source_id
WHERE NOT EXISTS (
  SELECT 1 FROM vehicle_specifications s2 JOIN data_sources d2 ON d2.id = s2.source_id
  WHERE s2.vehicle_id = s.vehicle_id AND s2.key = s.key
    AND (d2.priority < d.priority OR (d2.priority = d.priority AND s2.source_id < s.source_id))
);

-- The attributes fitment records are matched against.
CREATE VIEW vehicle_configuration AS
SELECT v.id AS vehicle_id, v.year, v.make, v.model,
  MAX(CASE WHEN e.key = 'trim' THEN e.value END) AS trim,
  MAX(CASE WHEN e.key = 'series' THEN e.value END) AS series,
  MAX(CASE WHEN e.key = 'body_class' THEN e.value END) AS body_class,
  MAX(CASE WHEN e.key = 'drive_type' THEN e.value END) AS drive_type,
  MAX(CASE WHEN e.key = 'engine_displacement_l' THEN e.value END) AS engine_displacement_l,
  MAX(CASE WHEN e.key = 'engine_cylinders' THEN e.value END) AS engine_cylinders,
  MAX(CASE WHEN e.key = 'engine_code' THEN e.value END) AS engine_code,
  MAX(CASE WHEN e.key = 'fuel_type' THEN e.value END) AS fuel_type,
  MAX(CASE WHEN e.key = 'transmission_style' THEN e.value END) AS transmission_style
FROM vehicles v LEFT JOIN vehicle_effective_specs e ON e.vehicle_id = v.id
GROUP BY v.id;

-- Hierarchical system → component taxonomy (structure only, no product claims).
CREATE TABLE part_categories (
  id        TEXT PRIMARY KEY,
  parent_id TEXT REFERENCES part_categories(id) ON DELETE CASCADE,
  name      TEXT NOT NULL,
  sort      INTEGER NOT NULL DEFAULT 0,
  UNIQUE (parent_id, name)
) STRICT;

CREATE TABLE parts (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  category_id   TEXT REFERENCES part_categories(id) ON DELETE SET NULL,
  brand         TEXT NOT NULL DEFAULT '',
  manufacturer  TEXT NOT NULL DEFAULT '',
  description   TEXT NOT NULL DEFAULT '',
  specs         TEXT NOT NULL DEFAULT '{}',  -- JSON object of technical attributes
  install_notes TEXT NOT NULL DEFAULT '',
  warranty      TEXT NOT NULL DEFAULT '',
  source_id     TEXT NOT NULL DEFAULT 'user' REFERENCES data_sources(id),
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX parts_category ON parts(category_id);
CREATE INDEX parts_brand ON parts(brand COLLATE NOCASE);

-- value_norm: uppercase letters and digits only, so "12345-ABC" matches "12345abc".
CREATE TABLE part_identifiers (
  id         INTEGER PRIMARY KEY,
  part_id    TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('oem', 'aftermarket', 'barcode', 'interchange', 'supplier')),
  value      TEXT NOT NULL,
  value_norm TEXT NOT NULL,
  brand      TEXT NOT NULL DEFAULT '',
  UNIQUE (part_id, kind, value_norm)
) STRICT;
CREATE INDEX part_identifiers_norm ON part_identifiers(value_norm);

-- A fitment record: "this part fits / does not fit year range + make + model", narrowed by
-- qualifiers. verified_by is set when the source is authoritative or a person checked it.
CREATE TABLE part_fitments (
  id          TEXT PRIMARY KEY,
  part_id     TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL DEFAULT 'fits' CHECK (kind IN ('fits', 'excludes')),
  year_from   INTEGER NOT NULL,
  year_to     INTEGER NOT NULL,
  make        TEXT NOT NULL COLLATE NOCASE,
  model       TEXT NOT NULL COLLATE NOCASE,
  position    TEXT NOT NULL DEFAULT '',
  quantity    INTEGER,
  notes       TEXT NOT NULL DEFAULT '',
  source_id   TEXT NOT NULL DEFAULT 'user' REFERENCES data_sources(id),
  source_ref  TEXT,
  verified_by TEXT CHECK (verified_by IN ('source', 'user')),
  verified_at TEXT,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CHECK (year_from <= year_to)
) STRICT;
CREATE INDEX part_fitments_vehicle ON part_fitments(make, model, year_from, year_to);
CREATE INDEX part_fitments_part ON part_fitments(part_id);

-- key is a vehicle specification key (engine_displacement_l, drive_type, trim, ...) or a
-- free qualifier (brake_package, production_from, ...) the vehicle must supply to be matched.
CREATE TABLE fitment_qualifiers (
  id         INTEGER PRIMARY KEY,
  fitment_id TEXT NOT NULL REFERENCES part_fitments(id) ON DELETE CASCADE,
  key        TEXT NOT NULL,
  value      TEXT NOT NULL,
  UNIQUE (fitment_id, key)
) STRICT;

CREATE TABLE suppliers (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  contact        TEXT NOT NULL DEFAULT '',
  phone          TEXT NOT NULL DEFAULT '',
  email          TEXT NOT NULL DEFAULT '',
  website        TEXT NOT NULL DEFAULT '',
  account_no     TEXT NOT NULL DEFAULT '',
  lead_time_days INTEGER,
  preferred      INTEGER NOT NULL DEFAULT 0 CHECK (preferred IN (0, 1)),
  notes          TEXT NOT NULL DEFAULT '',
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE inventory_locations (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  kind       TEXT NOT NULL DEFAULT 'warehouse' CHECK (kind IN ('warehouse', 'damaged')),
  is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  archived   INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX inventory_locations_one_default ON inventory_locations(is_default) WHERE is_default = 1;
INSERT INTO inventory_locations (id, name, kind, is_default) VALUES
  ('loc-main', 'Main', 'warehouse', 1),
  ('loc-damaged', 'Damaged / quarantine', 'damaged', 0);

-- A stocked product. Name, brand and category come from the catalog part it stocks.
CREATE TABLE inventory_items (
  id                 TEXT PRIMARY KEY,
  part_id            TEXT NOT NULL REFERENCES parts(id) ON DELETE RESTRICT,
  sku                TEXT NOT NULL UNIQUE COLLATE NOCASE,
  condition          TEXT NOT NULL DEFAULT 'new' CHECK (condition IN ('new', 'used', 'refurbished')),
  unit               TEXT NOT NULL DEFAULT 'each',
  cost               TEXT NOT NULL DEFAULT '0',   -- last purchase cost per unit
  avg_cost           TEXT NOT NULL DEFAULT '0',   -- moving weighted-average cost per unit
  price_override     TEXT,                        -- fixed selling price, beats every pricing rule
  pricing_profile_id TEXT REFERENCES pricing_profiles(id) ON DELETE SET NULL,
  min_stock          INTEGER NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
  reorder_qty        INTEGER NOT NULL DEFAULT 0 CHECK (reorder_qty >= 0),
  supplier_id        TEXT REFERENCES suppliers(id) ON DELETE SET NULL,
  warranty           TEXT NOT NULL DEFAULT '',
  notes              TEXT NOT NULL DEFAULT '',
  archived           INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
  version            INTEGER NOT NULL DEFAULT 1,  -- optimistic locking for edit forms
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX inventory_items_part ON inventory_items(part_id);

-- Quantities per location. The CHECKs make negative or over-reserved stock impossible,
-- whatever the application code does.
CREATE TABLE stock_levels (
  item_id     TEXT NOT NULL REFERENCES inventory_items(id) ON DELETE CASCADE,
  location_id TEXT NOT NULL REFERENCES inventory_locations(id),
  bin         TEXT NOT NULL DEFAULT '',
  on_hand     INTEGER NOT NULL DEFAULT 0 CHECK (on_hand >= 0),
  reserved    INTEGER NOT NULL DEFAULT 0 CHECK (reserved >= 0),
  CHECK (reserved <= on_hand),
  PRIMARY KEY (item_id, location_id)
) STRICT;

CREATE TABLE service_jobs (
  id                 TEXT PRIMARY KEY,
  number             INTEGER NOT NULL UNIQUE,
  status             TEXT NOT NULL DEFAULT 'estimate'
                     CHECK (status IN ('estimate', 'approved', 'in_progress', 'completed', 'invoiced', 'cancelled')),
  customer_id        TEXT REFERENCES customers(id) ON DELETE SET NULL,
  vehicle_id         TEXT REFERENCES vehicles(id) ON DELETE SET NULL,
  title              TEXT NOT NULL DEFAULT '',
  complaint          TEXT NOT NULL DEFAULT '',
  notes              TEXT NOT NULL DEFAULT '',
  pricing_profile_id TEXT REFERENCES pricing_profiles(id) ON DELETE SET NULL,
  mileage            INTEGER,
  technician         TEXT NOT NULL DEFAULT '',
  discount_pct       TEXT NOT NULL DEFAULT '0',
  -- Tax terms copied from the pricing profile, so an invoice never changes afterwards.
  tax_rate_pct       TEXT NOT NULL DEFAULT '0',
  prices_include_tax INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0, 1)),
  invoice_number     INTEGER UNIQUE,
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  approved_at        TEXT,
  completed_at       TEXT,
  invoiced_at        TEXT
) STRICT;
CREATE INDEX service_jobs_vehicle ON service_jobs(vehicle_id);
CREATE INDEX service_jobs_status ON service_jobs(status);

CREATE TABLE service_lines (
  id                   TEXT PRIMARY KEY,
  job_id               TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
  sort                 INTEGER NOT NULL DEFAULT 0,
  kind                 TEXT NOT NULL CHECK (kind IN ('part', 'labor', 'fee')),
  item_id              TEXT REFERENCES inventory_items(id) ON DELETE SET NULL,
  part_id              TEXT REFERENCES parts(id) ON DELETE SET NULL,
  description          TEXT NOT NULL,
  qty                  TEXT NOT NULL,          -- whole units for parts, hours for labor
  unit_price           TEXT NOT NULL,
  unit_cost            TEXT NOT NULL DEFAULT '0',
  taxable              INTEGER NOT NULL DEFAULT 1 CHECK (taxable IN (0, 1)),
  price_explanation    TEXT,                   -- JSON snapshot of how the price was calculated
  reservation_id       TEXT,
  consumed_movement_id TEXT
) STRICT;
CREATE INDEX service_lines_job ON service_lines(job_id, sort);

CREATE TABLE stock_reservations (
  id          TEXT PRIMARY KEY,
  item_id     TEXT NOT NULL REFERENCES inventory_items(id) ON DELETE CASCADE,
  location_id TEXT NOT NULL REFERENCES inventory_locations(id),
  qty         INTEGER NOT NULL CHECK (qty > 0),
  status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'released', 'consumed')),
  job_id      TEXT REFERENCES service_jobs(id) ON DELETE SET NULL,
  line_id     TEXT,
  vehicle_id  TEXT REFERENCES vehicles(id) ON DELETE SET NULL,
  note        TEXT NOT NULL DEFAULT '',
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  closed_at   TEXT
) STRICT;
CREATE INDEX stock_reservations_item ON stock_reservations(item_id, status);
CREATE INDEX stock_reservations_job ON stock_reservations(job_id);

-- The ledger. qty is the signed change to on_hand at location_id. Reservations change
-- `reserved` only and are recorded in stock_reservations.
CREATE TABLE inventory_movements (
  id             TEXT PRIMARY KEY,
  item_id        TEXT NOT NULL REFERENCES inventory_items(id) ON DELETE CASCADE,
  location_id    TEXT NOT NULL REFERENCES inventory_locations(id),
  kind           TEXT NOT NULL CHECK (kind IN ('receive', 'issue', 'transfer_out', 'transfer_in', 'adjust', 'count',
                                               'damage', 'return_in', 'return_out', 'consume', 'reverse')),
  qty            INTEGER NOT NULL CHECK (qty <> 0),
  unit_cost      TEXT,
  group_id       TEXT,           -- pairs both halves of a transfer
  ref_kind       TEXT,           -- 'job', 'po', 'count', ...
  ref_id         TEXT,
  reason         TEXT NOT NULL DEFAULT '',
  reverses_id    TEXT REFERENCES inventory_movements(id),
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX inventory_movements_item ON inventory_movements(item_id, created_at);
CREATE INDEX inventory_movements_time ON inventory_movements(created_at);
CREATE UNIQUE INDEX inventory_movements_one_reversal ON inventory_movements(reverses_id) WHERE reverses_id IS NOT NULL;

CREATE TABLE part_lists (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  vehicle_id TEXT REFERENCES vehicles(id) ON DELETE SET NULL,
  notes      TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE part_list_items (
  id       INTEGER PRIMARY KEY,
  list_id  TEXT NOT NULL REFERENCES part_lists(id) ON DELETE CASCADE,
  part_id  TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  qty      INTEGER NOT NULL DEFAULT 1 CHECK (qty > 0),
  note     TEXT NOT NULL DEFAULT '',
  added_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  UNIQUE (list_id, part_id)
) STRICT;

CREATE TABLE manuals (
  id           TEXT PRIMARY KEY,
  title        TEXT NOT NULL,
  kind         TEXT NOT NULL DEFAULT 'other' CHECK (kind IN ('owner', 'maintenance', 'service', 'repair', 'tsb', 'wiring',
                 'fuse', 'fluids', 'procedure', 'component_location', 'parts_diagram', 'troubleshooting', 'installation', 'other')),
  publisher    TEXT NOT NULL DEFAULT '',
  source_url   TEXT NOT NULL DEFAULT '',
  license_note TEXT NOT NULL DEFAULT '',
  file_sha256  TEXT NOT NULL UNIQUE,
  file_name    TEXT NOT NULL,
  file_size    INTEGER NOT NULL,
  page_count   INTEGER,
  indexed      INTEGER NOT NULL DEFAULT 0 CHECK (indexed IN (0, 1)),
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A manual belongs to one saved vehicle, or to a make/model/year range.
CREATE TABLE manual_vehicle_links (
  id         INTEGER PRIMARY KEY,
  manual_id  TEXT NOT NULL REFERENCES manuals(id) ON DELETE CASCADE,
  vehicle_id TEXT REFERENCES vehicles(id) ON DELETE CASCADE,
  make       TEXT COLLATE NOCASE,
  model      TEXT COLLATE NOCASE,
  year_from  INTEGER,
  year_to    INTEGER,
  CHECK (vehicle_id IS NOT NULL OR make IS NOT NULL)
) STRICT;
CREATE INDEX manual_vehicle_links_manual ON manual_vehicle_links(manual_id);
CREATE INDEX manual_vehicle_links_vehicle ON manual_vehicle_links(vehicle_id);

-- Text of each page, extracted on import, for search inside manuals and "Where Is It?".
CREATE VIRTUAL TABLE manual_pages USING fts5(manual_id UNINDEXED, page UNINDEXED, text, tokenize = 'porter unicode61');

CREATE TABLE manual_annotations (
  id         TEXT PRIMARY KEY,
  manual_id  TEXT NOT NULL REFERENCES manuals(id) ON DELETE CASCADE,
  page       INTEGER NOT NULL CHECK (page >= 1),
  kind       TEXT NOT NULL CHECK (kind IN ('bookmark', 'note', 'component')),
  label      TEXT NOT NULL,
  body       TEXT NOT NULL DEFAULT '',
  vehicle_id TEXT REFERENCES vehicles(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX manual_annotations_manual ON manual_annotations(manual_id, page);

CREATE TABLE attachments (
  id         TEXT PRIMARY KEY,
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('vehicle', 'part', 'item', 'job', 'maintenance', 'customer')),
  owner_id   TEXT NOT NULL,
  sha256     TEXT NOT NULL,
  file_name  TEXT NOT NULL,
  mime       TEXT NOT NULL,
  size       INTEGER NOT NULL,
  caption    TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX attachments_owner ON attachments(owner_kind, owner_id);

-- Global search. search_docs is the content (one row per entity, kept current by triggers);
-- search_index is the trigram FTS5 index over it.
CREATE TABLE search_docs (
  id     INTEGER PRIMARY KEY,
  kind   TEXT NOT NULL,
  ref_id TEXT NOT NULL,
  title  TEXT NOT NULL,
  body   TEXT NOT NULL,
  UNIQUE (kind, ref_id)
) STRICT;
CREATE VIRTUAL TABLE search_index USING fts5(title, body, content = 'search_docs', content_rowid = 'id', tokenize = 'trigram');
CREATE TRIGGER search_docs_ai AFTER INSERT ON search_docs BEGIN
  INSERT INTO search_index(rowid, title, body) VALUES (new.id, new.title, new.body);
END;
CREATE TRIGGER search_docs_ad AFTER DELETE ON search_docs BEGIN
  INSERT INTO search_index(search_index, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
END;
CREATE TRIGGER search_docs_au AFTER UPDATE ON search_docs BEGIN
  INSERT INTO search_index(search_index, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
  INSERT INTO search_index(rowid, title, body) VALUES (new.id, new.title, new.body);
END;

CREATE TRIGGER vehicles_search_ai AFTER INSERT ON vehicles BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('vehicle', new.id,
    trim(coalesce(new.year, '') || ' ' || new.make || ' ' || new.model || ' ' || new.trim),
    concat_ws(' ', new.vin, new.nickname, new.plate, new.color, new.engine))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER vehicles_search_au AFTER UPDATE ON vehicles BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('vehicle', new.id,
    trim(coalesce(new.year, '') || ' ' || new.make || ' ' || new.model || ' ' || new.trim),
    concat_ws(' ', new.vin, new.nickname, new.plate, new.color, new.engine))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER vehicles_search_ad AFTER DELETE ON vehicles BEGIN
  DELETE FROM search_docs WHERE kind = 'vehicle' AND ref_id = old.id;
END;

CREATE TRIGGER parts_search_ai AFTER INSERT ON parts BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('part', new.id, new.name,
    concat_ws(' ', new.brand, new.manufacturer, new.description))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER parts_search_au AFTER UPDATE ON parts BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('part', new.id, new.name,
    concat_ws(' ', new.brand, new.manufacturer, new.description,
      (SELECT group_concat(value, ' ') FROM part_identifiers WHERE part_id = new.id)))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
  UPDATE search_docs SET title = new.name
  WHERE kind = 'item' AND ref_id IN (SELECT id FROM inventory_items WHERE part_id = new.id) AND title <> new.name;
END;
CREATE TRIGGER parts_search_ad AFTER DELETE ON parts BEGIN
  DELETE FROM search_docs WHERE kind = 'part' AND ref_id = old.id;
END;
-- Identifiers change the part's search text; touching updated_at re-runs parts_search_au.
CREATE TRIGGER part_identifiers_ai AFTER INSERT ON part_identifiers BEGIN
  UPDATE parts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = new.part_id;
END;
CREATE TRIGGER part_identifiers_ad AFTER DELETE ON part_identifiers BEGIN
  UPDATE parts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = old.part_id;
END;

CREATE TRIGGER inventory_items_search_ai AFTER INSERT ON inventory_items BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('item', new.id,
    (SELECT name FROM parts WHERE id = new.part_id), concat_ws(' ', new.sku, new.notes))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER inventory_items_search_au AFTER UPDATE ON inventory_items BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('item', new.id,
    (SELECT name FROM parts WHERE id = new.part_id), concat_ws(' ', new.sku, new.notes))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER inventory_items_search_ad AFTER DELETE ON inventory_items BEGIN
  DELETE FROM search_docs WHERE kind = 'item' AND ref_id = old.id;
END;

CREATE TRIGGER manuals_search_ai AFTER INSERT ON manuals BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('manual', new.id, new.title, concat_ws(' ', new.publisher, new.kind, new.file_name))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER manuals_search_au AFTER UPDATE ON manuals BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('manual', new.id, new.title, concat_ws(' ', new.publisher, new.kind, new.file_name))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER manuals_search_ad AFTER DELETE ON manuals BEGIN
  DELETE FROM search_docs WHERE kind = 'manual' AND ref_id = old.id;
  DELETE FROM manual_pages WHERE manual_id = old.id;
END;

CREATE TRIGGER suppliers_search_ai AFTER INSERT ON suppliers BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('supplier', new.id, new.name, concat_ws(' ', new.contact, new.phone, new.email, new.account_no))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER suppliers_search_au AFTER UPDATE ON suppliers BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('supplier', new.id, new.name, concat_ws(' ', new.contact, new.phone, new.email, new.account_no))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER suppliers_search_ad AFTER DELETE ON suppliers BEGIN
  DELETE FROM search_docs WHERE kind = 'supplier' AND ref_id = old.id;
END;

CREATE TRIGGER customers_search_ai AFTER INSERT ON customers BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('customer', new.id, new.name, concat_ws(' ', new.phone, new.email))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER customers_search_au AFTER UPDATE ON customers BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('customer', new.id, new.name, concat_ws(' ', new.phone, new.email))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER customers_search_ad AFTER DELETE ON customers BEGIN
  DELETE FROM search_docs WHERE kind = 'customer' AND ref_id = old.id;
END;

CREATE TRIGGER service_jobs_search_ai AFTER INSERT ON service_jobs BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('job', new.id, '#' || new.number || ' ' || new.title, new.complaint)
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER service_jobs_search_au AFTER UPDATE ON service_jobs BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('job', new.id, '#' || new.number || ' ' || new.title, new.complaint)
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER service_jobs_search_ad AFTER DELETE ON service_jobs BEGIN
  DELETE FROM search_docs WHERE kind = 'job' AND ref_id = old.id;
END;

-- System → component taxonomy.
INSERT INTO part_categories (id, parent_id, name, sort) VALUES
  ('engine', NULL, 'Engine', 1),
  ('engine.gaskets', 'engine', 'Gaskets and seals', 1),
  ('engine.timing', 'engine', 'Timing belts, chains and components', 2),
  ('engine.belts', 'engine', 'Drive belts and tensioners', 3),
  ('engine.mounts', 'engine', 'Engine mounts', 4),
  ('engine.internal', 'engine', 'Internal components', 5),
  ('engine.oil_filter', 'engine', 'Oil filters', 6),
  ('engine.air_filter', 'engine', 'Air filters and intake', 7),
  ('fuel', NULL, 'Fuel delivery', 2),
  ('fuel.pump', 'fuel', 'Fuel pumps', 1),
  ('fuel.filter', 'fuel', 'Fuel filters', 2),
  ('fuel.injectors', 'fuel', 'Fuel injectors', 3),
  ('cooling', NULL, 'Cooling', 3),
  ('cooling.radiator', 'cooling', 'Radiators', 1),
  ('cooling.water_pump', 'cooling', 'Water pumps', 2),
  ('cooling.thermostat', 'cooling', 'Thermostats', 3),
  ('cooling.hoses', 'cooling', 'Hoses', 4),
  ('exhaust', NULL, 'Exhaust', 4),
  ('exhaust.catalytic', 'exhaust', 'Catalytic converters', 1),
  ('exhaust.muffler', 'exhaust', 'Mufflers and pipes', 2),
  ('exhaust.o2', 'exhaust', 'Oxygen sensors', 3),
  ('drivetrain', NULL, 'Transmission and drivetrain', 5),
  ('drivetrain.clutch', 'drivetrain', 'Clutch', 1),
  ('drivetrain.axles', 'drivetrain', 'Axles and CV joints', 2),
  ('drivetrain.transmission', 'drivetrain', 'Transmission parts', 3),
  ('drivetrain.differential', 'drivetrain', 'Differential', 4),
  ('suspension', NULL, 'Suspension', 6),
  ('suspension.shocks', 'suspension', 'Shocks and struts', 1),
  ('suspension.control_arms', 'suspension', 'Control arms and ball joints', 2),
  ('suspension.springs', 'suspension', 'Springs', 3),
  ('suspension.bushings', 'suspension', 'Bushings and sway bar links', 4),
  ('steering', NULL, 'Steering', 7),
  ('steering.rack', 'steering', 'Steering racks and gearboxes', 1),
  ('steering.tie_rods', 'steering', 'Tie rods', 2),
  ('steering.pump', 'steering', 'Power steering pumps', 3),
  ('brakes', NULL, 'Braking', 8),
  ('brakes.pads', 'brakes', 'Brake pads', 1),
  ('brakes.rotors', 'brakes', 'Rotors', 2),
  ('brakes.calipers', 'brakes', 'Calipers', 3),
  ('brakes.shoes_drums', 'brakes', 'Shoes and drums', 4),
  ('brakes.lines', 'brakes', 'Brake lines and hoses', 5),
  ('electrical', NULL, 'Electrical', 9),
  ('electrical.lighting', 'electrical', 'Lighting and bulbs', 1),
  ('electrical.switches', 'electrical', 'Switches and relays', 2),
  ('electrical.wiring', 'electrical', 'Wiring and fuses', 3),
  ('hvac', NULL, 'HVAC', 10),
  ('hvac.compressor', 'hvac', 'A/C compressors', 1),
  ('hvac.cabin_filter', 'hvac', 'Cabin air filters', 2),
  ('hvac.blower', 'hvac', 'Blower motors', 3),
  ('ignition', NULL, 'Ignition', 11),
  ('ignition.spark_plugs', 'ignition', 'Spark plugs', 1),
  ('ignition.coils', 'ignition', 'Ignition coils', 2),
  ('charging', NULL, 'Batteries and charging', 12),
  ('charging.battery', 'charging', 'Batteries', 1),
  ('charging.alternator', 'charging', 'Alternators', 2),
  ('charging.starter', 'charging', 'Starters', 3),
  ('interior', NULL, 'Interior', 13),
  ('exterior', NULL, 'Exterior', 14),
  ('exterior.wipers', 'exterior', 'Wiper blades', 1),
  ('exterior.mirrors', 'exterior', 'Mirrors', 2),
  ('wheels', NULL, 'Wheels and tires', 15),
  ('wheels.tires', 'wheels', 'Tires', 1),
  ('wheels.wheels', 'wheels', 'Wheels', 2),
  ('wheels.bearings', 'wheels', 'Wheel bearings and hubs', 3),
  ('wheels.tpms', 'wheels', 'TPMS sensors', 4),
  ('body', NULL, 'Body panels', 16),
  ('sensors', NULL, 'Sensors and modules', 17),
  ('sensors.engine', 'sensors', 'Engine sensors', 1),
  ('sensors.modules', 'sensors', 'Control modules', 2),
  ('fluids', NULL, 'Fluids and maintenance products', 18),
  ('fluids.oil', 'fluids', 'Engine oil', 1),
  ('fluids.coolant', 'fluids', 'Coolant', 2),
  ('fluids.transmission', 'fluids', 'Transmission fluid', 3),
  ('fluids.brake', 'fluids', 'Brake fluid', 4),
  ('fluids.other', 'fluids', 'Other fluids and chemicals', 5),
  ('ev', NULL, 'Hybrid and electric vehicle', 19),
  ('ev.battery', 'ev', 'High-voltage battery components', 1),
  ('ev.charging', 'ev', 'Charging equipment', 2),
  ('ev.inverter', 'ev', 'Inverters and converters', 3);

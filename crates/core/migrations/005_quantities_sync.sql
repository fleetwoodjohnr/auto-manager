-- 005: service bulletins source, fractional quantities, two-way sync.

INSERT INTO data_sources (id, name, kind, priority, url, license) VALUES
  ('nhtsa_bulletins', 'NHTSA Manufacturer Communications', 'provider', 20, 'https://api.nhtsa.gov/vehicles/byYmmt', 'U.S. Government work, public domain');

-- Quantities are now stored in thousandths of a unit, so items sold by the litre, foot or kilo
-- can hold 4.5 or 0.25 while the CHECKs below stay exact integer comparisons. Items are whole
-- units unless `fractional` is set.
ALTER TABLE inventory_items ADD COLUMN fractional INTEGER NOT NULL DEFAULT 0 CHECK (fractional IN (0, 1));
UPDATE stock_levels SET on_hand = on_hand * 1000, reserved = reserved * 1000;
UPDATE inventory_items SET min_stock = min_stock * 1000, reorder_qty = reorder_qty * 1000;
UPDATE inventory_movements SET qty = qty * 1000;
UPDATE stock_reservations SET qty = qty * 1000;
UPDATE part_list_items SET qty = qty * 1000;
UPDATE po_lines SET qty_ordered = qty_ordered * 1000, qty_received = qty_received * 1000;

-- Two-way sync (see folder_sync.rs). Device-local: never synced themselves.
-- The version of every synced row (or deletion) this computer knows. device '' = this computer.
CREATE TABLE sync_log (
  tbl     TEXT NOT NULL,
  key     TEXT NOT NULL,
  hlc     INTEGER NOT NULL,
  device  TEXT NOT NULL DEFAULT '',
  deleted INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
  PRIMARY KEY (tbl, key)
) STRICT, WITHOUT ROWID;
CREATE INDEX sync_log_device ON sync_log(device, hlc);

-- The hybrid logical clock (ms), and the switch the triggers check while applying other
-- computers' changes.
CREATE TABLE sync_clock (
  id       INTEGER PRIMARY KEY CHECK (id = 1),
  hlc      INTEGER NOT NULL DEFAULT 0,
  applying INTEGER NOT NULL DEFAULT 0 CHECK (applying IN (0, 1)),
  seeded   INTEGER NOT NULL DEFAULT 0 CHECK (seeded IN (0, 1))
) STRICT;
INSERT INTO sync_clock (id) VALUES (1);

-- Rows merged into another (the same VIN or SKU added on two computers).
CREATE TABLE sync_aliases (
  tbl     TEXT NOT NULL,
  from_id TEXT NOT NULL,
  to_id   TEXT NOT NULL,
  PRIMARY KEY (tbl, from_id)
) STRICT, WITHOUT ROWID;

-- Auto Manager schema v3: purchasing, supplier prices, saved searches.

-- What a supplier charges for an item, under their own part number.
CREATE TABLE supplier_parts (
  supplier_id    TEXT NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  item_id        TEXT NOT NULL REFERENCES inventory_items(id) ON DELETE CASCADE,
  supplier_sku   TEXT NOT NULL DEFAULT '',
  cost           TEXT,
  lead_time_days INTEGER,
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  PRIMARY KEY (supplier_id, item_id)
) STRICT;

CREATE TABLE purchase_orders (
  id            TEXT PRIMARY KEY,
  number        INTEGER NOT NULL UNIQUE,
  supplier_id   TEXT NOT NULL REFERENCES suppliers(id),
  status        TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'ordered', 'partial', 'received', 'cancelled')),
  expected_date TEXT,
  notes         TEXT NOT NULL DEFAULT '',
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  ordered_at    TEXT,
  closed_at     TEXT
) STRICT;
CREATE INDEX purchase_orders_status ON purchase_orders(status);

CREATE TABLE po_lines (
  id           TEXT PRIMARY KEY,
  po_id        TEXT NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
  item_id      TEXT NOT NULL REFERENCES inventory_items(id),
  qty_ordered  INTEGER NOT NULL CHECK (qty_ordered > 0),
  qty_received INTEGER NOT NULL DEFAULT 0 CHECK (qty_received >= 0),
  unit_cost    TEXT NOT NULL,
  sort         INTEGER NOT NULL DEFAULT 0,
  CHECK (qty_received <= qty_ordered),
  UNIQUE (po_id, item_id)
) STRICT;

CREATE TABLE saved_searches (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  page       TEXT NOT NULL,          -- which screen the filters belong to
  filters    TEXT NOT NULL,          -- JSON
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TRIGGER purchase_orders_search_ai AFTER INSERT ON purchase_orders BEGIN
  INSERT INTO search_docs (kind, ref_id, title, body) VALUES ('po', new.id, 'PO ' || new.number, (SELECT name FROM suppliers WHERE id = new.supplier_id))
  ON CONFLICT (kind, ref_id) DO UPDATE SET title = excluded.title, body = excluded.body;
END;
CREATE TRIGGER purchase_orders_search_ad AFTER DELETE ON purchase_orders BEGIN
  DELETE FROM search_docs WHERE kind = 'po' AND ref_id = old.id;
END;

-- 006: links to records in other systems (QuickBooks), so importing again updates the same
-- records instead of adding copies. Synced between computers (folder_sync.rs).
CREATE TABLE external_links (
  -- "qbo:<company id>" for a QuickBooks Online connection, "qb-file" for QuickBooks exports
  system      TEXT NOT NULL,
  -- "item", "customer" or "vendor"
  entity      TEXT NOT NULL,
  external_id TEXT NOT NULL,
  -- inventory_items.id, customers.id or suppliers.id
  local_id    TEXT NOT NULL,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  PRIMARY KEY (system, entity, external_id)
) STRICT;
CREATE INDEX external_links_local ON external_links(local_id);

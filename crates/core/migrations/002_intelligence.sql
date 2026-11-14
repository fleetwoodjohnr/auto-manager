-- Auto Manager schema v2: automotive intelligence (recalls, maintenance, interchange, DTCs).

INSERT INTO data_sources (id, name, kind, priority, url, license) VALUES
  ('nhtsa_recalls', 'NHTSA Recalls', 'provider', 20, 'https://api.nhtsa.gov/recalls/', 'U.S. Government work, public domain'),
  ('nhtsa_ratings', 'NHTSA 5-Star Safety Ratings', 'provider', 20, 'https://api.nhtsa.gov/SafetyRatings/', 'U.S. Government work, public domain'),
  ('vpic_canada', 'vPIC Canadian Vehicle Specifications', 'provider', 30, 'https://vpic.nhtsa.dot.gov/api/', 'U.S. Government work, public domain'),
  ('epa_fueleconomy', 'EPA fueleconomy.gov', 'provider', 30, 'https://www.fueleconomy.gov/feg/ws/', 'U.S. Government work, public domain'),
  ('obdex', 'OBDex generic OBD-II codes', 'import', 40, 'https://github.com/foerbsnavi/OBDex', 'CC0 1.0 (data)');

-- Campaigns NHTSA lists for a model year/make/model. These are potentially applicable to a
-- vehicle; whether a campaign is outstanding for one VIN is only known from a per-VIN check
-- (nhtsa.gov/recalls or the manufacturer), recorded in recall_status.
CREATE TABLE recalls (
  vehicle_key  TEXT NOT NULL,      -- "YEAR|MAKE|MODEL" upper case
  campaign     TEXT NOT NULL,
  manufacturer TEXT NOT NULL DEFAULT '',
  component    TEXT NOT NULL DEFAULT '',
  summary      TEXT NOT NULL DEFAULT '',
  consequence  TEXT NOT NULL DEFAULT '',
  remedy       TEXT NOT NULL DEFAULT '',
  notes        TEXT NOT NULL DEFAULT '',
  report_date  TEXT,               -- YYYY-MM-DD
  park_it      INTEGER NOT NULL DEFAULT 0,
  park_outside INTEGER NOT NULL DEFAULT 0,
  over_the_air INTEGER NOT NULL DEFAULT 0,
  fetched_at   TEXT NOT NULL,
  PRIMARY KEY (vehicle_key, campaign)
) STRICT;

CREATE TABLE recall_status (
  vehicle_id TEXT NOT NULL REFERENCES vehicles(id) ON DELETE CASCADE,
  campaign   TEXT NOT NULL,
  status     TEXT NOT NULL CHECK (status IN ('outstanding', 'completed', 'not_applicable')),
  note       TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  PRIMARY KEY (vehicle_id, campaign)
) STRICT;

-- old_part is replaced by new_part (the newer number should be used).
CREATE TABLE part_supersessions (
  old_part_id TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  new_part_id TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  note        TEXT NOT NULL DEFAULT '',
  source_id   TEXT NOT NULL DEFAULT 'user' REFERENCES data_sources(id),
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  PRIMARY KEY (old_part_id, new_part_id),
  CHECK (old_part_id <> new_part_id)
) STRICT;

-- Two parts that can be used in place of each other (stored once, part_a < part_b).
CREATE TABLE part_interchanges (
  part_a     TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  part_b     TEXT NOT NULL REFERENCES parts(id) ON DELETE CASCADE,
  note       TEXT NOT NULL DEFAULT '',
  source_id  TEXT NOT NULL DEFAULT 'user' REFERENCES data_sources(id),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  PRIMARY KEY (part_a, part_b),
  CHECK (part_a < part_b)
) STRICT;

-- Service intervals for a vehicle, as the person records them (ideally from the owner's
-- manual, cited in source_ref). Auto Manager ships no generic schedule.
CREATE TABLE maintenance_intervals (
  id           TEXT PRIMARY KEY,
  vehicle_id   TEXT NOT NULL REFERENCES vehicles(id) ON DELETE CASCADE,
  service      TEXT NOT NULL,
  every_miles  INTEGER CHECK (every_miles IS NULL OR every_miles > 0),
  every_months INTEGER CHECK (every_months IS NULL OR every_months > 0),
  source_ref   TEXT NOT NULL DEFAULT '',
  notes        TEXT NOT NULL DEFAULT '',
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CHECK (every_miles IS NOT NULL OR every_months IS NOT NULL)
) STRICT;
CREATE INDEX maintenance_intervals_vehicle ON maintenance_intervals(vehicle_id);

CREATE TABLE maintenance_records (
  id          TEXT PRIMARY KEY,
  vehicle_id  TEXT NOT NULL REFERENCES vehicles(id) ON DELETE CASCADE,
  interval_id TEXT REFERENCES maintenance_intervals(id) ON DELETE SET NULL,
  job_id      TEXT REFERENCES service_jobs(id) ON DELETE SET NULL,
  date        TEXT NOT NULL,         -- YYYY-MM-DD
  mileage     INTEGER CHECK (mileage IS NULL OR mileage >= 0),
  service     TEXT NOT NULL,
  parts_used  TEXT NOT NULL DEFAULT '',
  labor       TEXT NOT NULL DEFAULT '',
  cost        TEXT,
  provider    TEXT NOT NULL DEFAULT '',
  notes       TEXT NOT NULL DEFAULT '',
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX maintenance_records_vehicle ON maintenance_records(vehicle_id, date);

-- Generic OBD-II trouble codes (loaded from bundled CC0 data on first use).
CREATE TABLE dtc_codes (
  code        TEXT PRIMARY KEY,
  category    TEXT NOT NULL,
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  detail      TEXT NOT NULL DEFAULT '{}',  -- JSON: causes, symptoms, related, sources, mil
  source_id   TEXT NOT NULL DEFAULT 'obdex' REFERENCES data_sources(id)
) STRICT;

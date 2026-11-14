-- Auto Manager schema v4: read-only OBD-II sessions.

CREATE TABLE obd_sessions (
  id         TEXT PRIMARY KEY,
  vehicle_id TEXT REFERENCES vehicles(id) ON DELETE SET NULL,
  adapter    TEXT NOT NULL,          -- e.g. "ELM327 v1.5 on /dev/ttyUSB0"
  protocol   TEXT NOT NULL DEFAULT '',
  vin_read   TEXT,
  started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE obd_readings (
  id         INTEGER PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES obd_sessions(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('stored_dtc', 'pending_dtc', 'pid', 'freeze_frame', 'readiness')),
  code       TEXT NOT NULL,          -- DTC or PID name
  value      TEXT NOT NULL DEFAULT '',
  unit       TEXT NOT NULL DEFAULT '',
  at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE INDEX obd_readings_session ON obd_readings(session_id);

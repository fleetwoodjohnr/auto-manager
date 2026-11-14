-- 005: service bulletins source, fractional quantities, two-way sync.

INSERT INTO data_sources (id, name, kind, priority, url, license) VALUES
  ('nhtsa_bulletins', 'NHTSA Manufacturer Communications', 'provider', 20, 'https://api.nhtsa.gov/vehicles/byYmmt', 'U.S. Government work, public domain');

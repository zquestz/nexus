-- Unicode-folded tracker-name key for case-insensitive uniqueness and lookup.
-- (Address keeps its own COLLATE NOCASE + (address, port) endpoint index.)
ALTER TABLE trackers ADD COLUMN name_lower TEXT NOT NULL DEFAULT '';
UPDATE trackers SET name_lower = lower(name);
CREATE UNIQUE INDEX idx_trackers_name_lower ON trackers(name_lower);

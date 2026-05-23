-- Unicode-folded group-name key for case-insensitive uniqueness and lookup.
ALTER TABLE groups ADD COLUMN name_lower TEXT NOT NULL DEFAULT '';
UPDATE groups SET name_lower = lower(name);
CREATE UNIQUE INDEX idx_groups_name_lower ON groups(name_lower);

-- Unicode-folded channel-name key for case-insensitive uniqueness and lookup.
ALTER TABLE channel_settings ADD COLUMN name_lower TEXT NOT NULL DEFAULT '';
UPDATE channel_settings SET name_lower = lower(name);
CREATE UNIQUE INDEX idx_channel_settings_name_lower ON channel_settings(name_lower);

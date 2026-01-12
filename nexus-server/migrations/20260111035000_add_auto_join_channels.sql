-- Add auto_join_channels config (space-separated list of channel names)
-- These channels are automatically joined by users on login
-- Default to #nexus for backward compatibility (same as persistent_channels default)
INSERT INTO config (key, value) VALUES ('auto_join_channels', '#nexus');
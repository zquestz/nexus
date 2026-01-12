-- Add persistent_channels config (space-separated list of channel names)
-- Default to #nexus for a sensible out-of-box experience
INSERT INTO config (key, value) VALUES ('persistent_channels', '#nexus');

-- Create channel_settings table for persistent channel data
-- Only persistent channels have their settings stored here
CREATE TABLE IF NOT EXISTS channel_settings (
    name TEXT PRIMARY KEY,
    topic TEXT NOT NULL DEFAULT '',
    topic_set_by TEXT NOT NULL DEFAULT '',
    secret INTEGER NOT NULL DEFAULT 0
);

-- Initialize settings for the default #nexus channel
INSERT INTO channel_settings (name, topic, topic_set_by, secret) VALUES ('#nexus', '', '', 0);
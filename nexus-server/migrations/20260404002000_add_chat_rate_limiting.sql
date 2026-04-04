-- Add default flood protection config values
INSERT OR IGNORE INTO config (key, value) VALUES ('chat_burst_limit', '5');
INSERT OR IGNORE INTO config (key, value) VALUES ('chat_rate_limit', '20');
-- Add file_reindex_interval to config
-- Default is 5 minutes, 0 to disable
INSERT INTO config (key, value) VALUES ('file_reindex_interval', '5');

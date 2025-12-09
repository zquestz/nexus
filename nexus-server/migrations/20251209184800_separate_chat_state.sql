-- Separate chat state from server configuration
-- 1. Create new chat_state table for topic storage
-- 2. Rename server_config to config
-- 3. Migrate topic data to chat_state
-- 4. Remove topic keys from config

-- Create chat_state table
CREATE TABLE IF NOT EXISTS chat_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Migrate topic data from server_config to chat_state
INSERT INTO chat_state (key, value)
SELECT key, value FROM server_config WHERE key IN ('topic', 'topic_set_by');

-- Delete topic keys from server_config
DELETE FROM server_config WHERE key IN ('topic', 'topic_set_by');

-- Rename server_config to config
ALTER TABLE server_config RENAME TO config;
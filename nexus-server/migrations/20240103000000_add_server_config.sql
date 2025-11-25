-- Add server configuration table for persistent settings
-- This table stores key-value pairs for server-wide configuration

CREATE TABLE IF NOT EXISTS server_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Insert default topic (empty - no topic set by default)
INSERT INTO server_config (key, value) 
VALUES ('topic', '');
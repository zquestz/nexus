-- Create ip_bans table for IP-based bans
CREATE TABLE IF NOT EXISTS ip_bans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ip_address TEXT NOT NULL UNIQUE,
    nickname TEXT,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);

-- Index for faster IP lookups (used during pre-TLS ban check)
CREATE INDEX IF NOT EXISTS idx_ip_bans_ip_address ON ip_bans(ip_address);

-- Index for nickname lookups (used by /unban when unbanning by nickname)
CREATE INDEX IF NOT EXISTS idx_ip_bans_nickname ON ip_bans(nickname);

-- Index for expiry filtering (used by cleanup and active ban queries)
CREATE INDEX IF NOT EXISTS idx_ip_bans_expires_at ON ip_bans(expires_at);
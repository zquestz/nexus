-- Create ip_trusted table for trusted IPs (bypass ban checks)
CREATE TABLE IF NOT EXISTS ip_trusted (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ip_address TEXT NOT NULL UNIQUE,
    nickname TEXT,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);

-- Index for faster IP lookups (used during pre-TLS trust check)
CREATE INDEX IF NOT EXISTS idx_ip_trusted_ip_address ON ip_trusted(ip_address);

-- Index for nickname lookups (used by /untrust when untrusting by nickname)
CREATE INDEX IF NOT EXISTS idx_ip_trusted_nickname ON ip_trusted(nickname);

-- Index for expiry filtering (used by cleanup and active trust queries)
CREATE INDEX IF NOT EXISTS idx_ip_trusted_expires_at ON ip_trusted(expires_at);
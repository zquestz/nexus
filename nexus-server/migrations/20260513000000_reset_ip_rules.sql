-- Replace ip_bans and ip_trusted with canonical-form schema:
-- - ip_address is the natural primary key (drop the unused autoincrement id)
-- - COLLATE NOCASE so case-folded uniqueness is enforced at the SQL layer
-- - Indexes on nickname and expires_at retained
-- Dev-preview only; no data preserved.

DROP TABLE IF EXISTS ip_bans;
CREATE TABLE ip_bans (
    ip_address TEXT NOT NULL PRIMARY KEY COLLATE NOCASE,
    nickname TEXT,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_ip_bans_nickname ON ip_bans(nickname);
CREATE INDEX IF NOT EXISTS idx_ip_bans_expires_at ON ip_bans(expires_at);

DROP TABLE IF EXISTS ip_trusted;
CREATE TABLE ip_trusted (
    ip_address TEXT NOT NULL PRIMARY KEY COLLATE NOCASE,
    nickname TEXT,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_ip_trusted_nickname ON ip_trusted(nickname);
CREATE INDEX IF NOT EXISTS idx_ip_trusted_expires_at ON ip_trusted(expires_at);

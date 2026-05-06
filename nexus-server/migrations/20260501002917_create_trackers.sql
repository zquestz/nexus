-- Tracker configuration rows. Runtime status lives in memory.
CREATE TABLE IF NOT EXISTS trackers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    address TEXT NOT NULL,
    port INTEGER NOT NULL,
    fingerprint TEXT,
    password TEXT,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_trackers_endpoint ON trackers(LOWER(address), port);
CREATE UNIQUE INDEX IF NOT EXISTS idx_trackers_name_lower ON trackers(LOWER(name));

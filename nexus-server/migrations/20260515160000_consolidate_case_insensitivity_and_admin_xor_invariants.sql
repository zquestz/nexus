-- Consolidate case-insensitivity (COLLATE NOCASE); drop chat_state; admin XOR group + admin XOR shared CHECKs.

-- Admin rows must satisfy both XOR invariants before restore: no group, not shared.
UPDATE users SET group_id = NULL, is_shared = 0 WHERE is_admin = 1;
DELETE FROM user_permissions
WHERE user_id IN (SELECT id FROM users WHERE is_admin = 1);

CREATE TEMP TABLE _users_data AS SELECT * FROM users;
CREATE TEMP TABLE _user_permissions_data AS SELECT * FROM user_permissions;
CREATE TEMP TABLE _news_data AS SELECT * FROM news;
CREATE TEMP TABLE _groups_data AS SELECT * FROM groups;
CREATE TEMP TABLE _group_permissions_data AS SELECT * FROM group_permissions;
CREATE TEMP TABLE _trackers_data AS SELECT * FROM trackers;
CREATE TEMP TABLE _channel_settings_data AS SELECT * FROM channel_settings;
CREATE TEMP TABLE _sqlite_sequence_backup AS
    SELECT name, seq FROM sqlite_sequence
    WHERE name IN ('users', 'groups', 'trackers', 'news');

-- Children first, then parents.
DROP TABLE user_permissions;
DROP TABLE news;
DROP TABLE group_permissions;
DROP TABLE users;
DROP TABLE groups;
DROP TABLE trackers;
DROP TABLE channel_settings;
DROP TABLE chat_state;

-- Recreate parent-first.
CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    is_shared BOOLEAN NOT NULL DEFAULT 0,
    bandwidth_weight INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    is_admin BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    is_shared BOOLEAN NOT NULL DEFAULT 0,
    group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
    bandwidth_weight INTEGER,
    CHECK (NOT (is_admin = 1 AND group_id IS NOT NULL)),
    CHECK (NOT (is_admin = 1 AND is_shared = 1))
);

CREATE TABLE user_permissions (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    override_type TEXT NOT NULL DEFAULT 'grant',
    PRIMARY KEY (user_id, permission)
);

CREATE TABLE news (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    body TEXT,
    image TEXT,
    author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT,
    CHECK (body IS NOT NULL OR image IS NOT NULL)
);
CREATE INDEX idx_news_author_id ON news(author_id);
CREATE INDEX idx_news_created_at ON news(created_at);

CREATE TABLE group_permissions (
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    PRIMARY KEY (group_id, permission)
);

CREATE TABLE trackers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    address TEXT NOT NULL COLLATE NOCASE,
    port INTEGER NOT NULL,
    fingerprint TEXT,
    password TEXT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_trackers_endpoint ON trackers(address, port);

CREATE TABLE channel_settings (
    name TEXT PRIMARY KEY COLLATE NOCASE,
    topic TEXT NOT NULL DEFAULT '',
    topic_set_by TEXT NOT NULL DEFAULT '',
    secret BOOLEAN NOT NULL DEFAULT 0
);

-- Restore parent-first.
INSERT INTO groups SELECT * FROM _groups_data;
INSERT INTO users SELECT * FROM _users_data;
INSERT INTO user_permissions SELECT * FROM _user_permissions_data;
INSERT INTO news SELECT * FROM _news_data;
INSERT INTO group_permissions SELECT * FROM _group_permissions_data;
INSERT INTO trackers SELECT * FROM _trackers_data;
INSERT INTO channel_settings SELECT * FROM _channel_settings_data;

-- Restore original AUTOINCREMENT high-water marks (data INSERTs only seed max(id)).
DELETE FROM sqlite_sequence WHERE name IN ('users', 'groups', 'trackers', 'news');
INSERT INTO sqlite_sequence SELECT name, seq FROM _sqlite_sequence_backup;

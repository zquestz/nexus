-- Harden the 0.9 schema after the dev-preview 0.8.x cycle.
--
-- This rebuild keeps existing data but makes folded-name columns first-class,
-- normalizes news timestamps to integer epoch seconds, preserves long-lived
-- news rows when authors are deleted, and adds DB-level checks for closed
-- boolean/enum/range domains.

CREATE TEMP TABLE _schema09_config_data AS
    SELECT key, value FROM config;

CREATE TEMP TABLE _schema09_groups_data AS
    SELECT id, name, is_shared, bandwidth_weight, name_lower FROM groups;

CREATE TEMP TABLE _schema09_users_data AS
    SELECT id, username, password_hash, is_admin, created_at, enabled,
           is_shared, group_id, bandwidth_weight, username_lower
    FROM users;

CREATE TEMP TABLE _schema09_user_permissions_data AS
    SELECT user_id, permission, override_type FROM user_permissions;

CREATE TEMP TABLE _schema09_news_data AS
    SELECT id, body, image, author_id, created_at, updated_at FROM news;

CREATE TEMP TABLE _schema09_group_permissions_data AS
    SELECT group_id, permission FROM group_permissions;

CREATE TEMP TABLE _schema09_trackers_data AS
    SELECT id, address, port, fingerprint, password, name, enabled,
           created_at, updated_at, name_lower
    FROM trackers;

CREATE TEMP TABLE _schema09_channel_settings_data AS
    SELECT name, topic, topic_set_by, secret, name_lower FROM channel_settings;

CREATE TEMP TABLE _schema09_ip_bans_data AS
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at,
           nickname_lower
    FROM ip_bans;

CREATE TEMP TABLE _schema09_ip_trusted_data AS
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at,
           nickname_lower
    FROM ip_trusted;

CREATE TEMP TABLE _schema09_sqlite_sequence_backup AS
    SELECT name, seq FROM sqlite_sequence
    WHERE name IN ('users', 'groups', 'trackers', 'news');

-- Drop children before parents.
DROP TABLE user_permissions;
DROP TABLE news;
DROP TABLE group_permissions;
DROP TABLE users;
DROP TABLE groups;
DROP TABLE trackers;
DROP TABLE channel_settings;
DROP TABLE ip_bans;
DROP TABLE ip_trusted;
DROP TABLE config;

CREATE TABLE config (
    key TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name_lower TEXT NOT NULL UNIQUE,
    is_shared BOOLEAN NOT NULL DEFAULT 0 CHECK (is_shared IN (0, 1)),
    bandwidth_weight INTEGER NOT NULL DEFAULT 1 CHECK (bandwidth_weight BETWEEN 1 AND 65535)
);

CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    username_lower TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    is_admin BOOLEAN NOT NULL DEFAULT 0 CHECK (is_admin IN (0, 1)),
    created_at INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    is_shared BOOLEAN NOT NULL DEFAULT 0 CHECK (is_shared IN (0, 1)),
    group_id INTEGER REFERENCES groups(id) ON DELETE RESTRICT,
    bandwidth_weight INTEGER CHECK (
        bandwidth_weight IS NULL OR bandwidth_weight BETWEEN 1 AND 65535
    ),
    CHECK (NOT (is_admin = 1 AND group_id IS NOT NULL)),
    CHECK (NOT (is_admin = 1 AND is_shared = 1))
);

CREATE TABLE user_permissions (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    override_type TEXT NOT NULL DEFAULT 'grant'
        CHECK (override_type IN ('grant', 'revoke')),
    PRIMARY KEY (user_id, permission)
);

CREATE TABLE news (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    body TEXT,
    image TEXT,
    author_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK (
        (body IS NOT NULL AND body != '')
        OR (image IS NOT NULL AND image != '')
    )
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
    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    fingerprint TEXT,
    password TEXT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name_lower TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_trackers_endpoint ON trackers(address, port);

CREATE TABLE channel_settings (
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name_lower TEXT NOT NULL UNIQUE,
    topic TEXT NOT NULL DEFAULT '',
    topic_set_by TEXT NOT NULL,
    secret BOOLEAN NOT NULL DEFAULT 0 CHECK (secret IN (0, 1))
);

CREATE TABLE ip_bans (
    ip_address TEXT NOT NULL PRIMARY KEY COLLATE NOCASE,
    nickname TEXT,
    nickname_lower TEXT,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
CREATE INDEX idx_ip_bans_expires_at ON ip_bans(expires_at);
CREATE INDEX idx_ip_bans_nickname_lower ON ip_bans(nickname_lower);

CREATE TABLE ip_trusted (
    ip_address TEXT NOT NULL PRIMARY KEY COLLATE NOCASE,
    nickname TEXT,
    nickname_lower TEXT,
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
CREATE INDEX idx_ip_trusted_expires_at ON ip_trusted(expires_at);
CREATE INDEX idx_ip_trusted_nickname_lower ON ip_trusted(nickname_lower);

INSERT INTO config (key, value)
SELECT key, value FROM _schema09_config_data;

INSERT INTO groups (id, name, name_lower, is_shared, bandwidth_weight)
SELECT id, name, name_lower, is_shared, bandwidth_weight FROM _schema09_groups_data;

INSERT INTO users (
    id, username, username_lower, password_hash, is_admin, created_at, enabled,
    is_shared, group_id, bandwidth_weight
)
SELECT id, username, username_lower, password_hash, is_admin, created_at,
       enabled, is_shared, group_id, bandwidth_weight
FROM _schema09_users_data;

INSERT INTO user_permissions (user_id, permission, override_type)
SELECT user_id, permission, override_type FROM _schema09_user_permissions_data;

INSERT INTO news (id, body, image, author_id, created_at, updated_at)
SELECT n.id,
       n.body,
       n.image,
       CASE WHEN u.id IS NULL THEN NULL ELSE n.author_id END,
       CAST(strftime('%s', n.created_at) AS INTEGER),
       COALESCE(
           CAST(strftime('%s', n.updated_at) AS INTEGER),
           CAST(strftime('%s', n.created_at) AS INTEGER)
       )
FROM _schema09_news_data n
LEFT JOIN users u ON u.id = n.author_id;

INSERT INTO group_permissions (group_id, permission)
SELECT group_id, permission FROM _schema09_group_permissions_data;

INSERT INTO trackers (
    id, address, port, fingerprint, password, name, name_lower, enabled,
    created_at, updated_at
)
SELECT id, address, port, fingerprint, password, name, name_lower, enabled,
       created_at, updated_at
FROM _schema09_trackers_data;

INSERT INTO channel_settings (name, name_lower, topic, topic_set_by, secret)
SELECT name, name_lower, topic, topic_set_by, secret FROM _schema09_channel_settings_data;

INSERT INTO ip_bans (
    ip_address, nickname, nickname_lower, reason, created_by, created_at,
    expires_at
)
SELECT ip_address, nickname, nickname_lower, reason, created_by, created_at,
       expires_at
FROM _schema09_ip_bans_data;

INSERT INTO ip_trusted (
    ip_address, nickname, nickname_lower, reason, created_by, created_at,
    expires_at
)
SELECT ip_address, nickname, nickname_lower, reason, created_by, created_at,
       expires_at
FROM _schema09_ip_trusted_data;

DELETE FROM sqlite_sequence WHERE name IN ('users', 'groups', 'trackers', 'news');
INSERT INTO sqlite_sequence SELECT name, seq FROM _schema09_sqlite_sequence_backup;

DROP TABLE _schema09_config_data;
DROP TABLE _schema09_groups_data;
DROP TABLE _schema09_users_data;
DROP TABLE _schema09_user_permissions_data;
DROP TABLE _schema09_news_data;
DROP TABLE _schema09_group_permissions_data;
DROP TABLE _schema09_trackers_data;
DROP TABLE _schema09_channel_settings_data;
DROP TABLE _schema09_ip_bans_data;
DROP TABLE _schema09_ip_trusted_data;
DROP TABLE _schema09_sqlite_sequence_backup;

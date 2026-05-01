-- Trackers the server publishes registration entries to.
--
-- Each row represents one tracker connection target. The publisher task
-- maintains one long-lived TLS connection per `enabled = 1` row and
-- refreshes the registration on the tracker-supplied interval.
--
-- This table holds *configuration* only — the admin's persistent intent
-- to publish to a given tracker. Connection status, last successful
-- refresh, last error, pending-but-unaccepted fingerprint observations,
-- and the tracker-supplied refresh interval all live in memory in the
-- publisher task manager and are surfaced to the admin UI via runtime
-- fields on the admin protocol response. After a server restart, the
-- publisher task reconnects and recreates that runtime state from the
-- next round of attempts; persisting it would only narrow a few-second
-- post-restart visibility gap that doesn't matter operationally.
CREATE TABLE IF NOT EXISTS trackers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Hostname or IP literal, no brackets, no port. Validation rules
    -- mirror `nexus_common::validators::validate_public_address`.
    address TEXT NOT NULL,
    -- TCP port (typically 7510). The BBS server connects to trackers
    -- via raw TCP only; tracker WebSocket support exists for browser
    -- clients, not for server-to-tracker publishing.
    port INTEGER NOT NULL,
    -- TOFU-pinned cert fingerprint in canonical form. NULL on first
    -- connect (publisher accepts whatever the tracker presents and
    -- writes it here); subsequent connects must match byte-for-byte
    -- or the publisher refuses to register and surfaces the observed
    -- value to the admin for manual accept.
    fingerprint TEXT,
    -- Registration password. NULL if the tracker is open. Stored
    -- plaintext because the protocol must send it verbatim; the
    -- database file is mode 0600 (same posture as TLS keys and
    -- bookmark passwords on the client side).
    password TEXT,
    -- Admin-supplied label used as the primary identifier in the
    -- admin UI. Required and capped at MAX_TRACKER_NAME_LENGTH (256
    -- bytes) by the protocol layer. Admins who don't want a friendly
    -- label can paste the address here.
    name TEXT NOT NULL,
    -- 1 = publisher actively maintains the connection. 0 = configured
    -- but paused; admin can disable temporarily without losing the
    -- pinned fingerprint or stored password. Publisher drops the
    -- connection on transition to 0 and reconnects on transition to 1.
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- A given (address, port) target can be configured at most once.
-- Duplicate rows would mean the publisher fires two parallel
-- connections to the same tracker, which the tracker's per-IP entry
-- cap would reject at registration time anyway.
CREATE UNIQUE INDEX idx_trackers_endpoint ON trackers(address, port);

-- Case-insensitive uniqueness on the admin-supplied label, matching
-- the pattern users/groups use for their human-name columns. Forces
-- distinct names in the admin UI list (admins who don't care about
-- naming can paste the address, which is itself unique). Doubles as
-- the index supporting the LOWER(name) sort in SQL_SELECT_ALL_TRACKERS.
CREATE UNIQUE INDEX idx_trackers_name_lower ON trackers(LOWER(name));

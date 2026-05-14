//! SQL query constants for database operations
//!
//! This module contains all SQL queries used by the database layer.
//! Each query is documented with its parameters and special behaviors.

// ========================================================================
// Configuration Query Operations
// ========================================================================

/// Get a configuration value by key
///
/// **Parameters:**
/// 1. `key: &str` - Configuration key to look up
///
/// **Returns:** `(value: String)`
pub const SQL_GET_CONFIG: &str = "SELECT value FROM config WHERE key = ?";

/// Set a configuration value (update existing key)
///
/// **Parameters:**
/// 1. `value: &str` - New configuration value
/// 2. `key: &str` - Configuration key to update
///
/// **Note:** Only updates existing keys; does not insert new ones.
pub const SQL_SET_CONFIG: &str = "UPDATE config SET value = ? WHERE key = ?";

/// Get all configuration key-value pairs
///
/// **Returns:** `(key: String, value: String)` for each row
pub const SQL_GET_ALL_CONFIG: &str = "SELECT key, value FROM config";

/// Get the three config fields needed by the tracker task to build
/// `TrackerServerRegister` payloads.
///
/// **Parameters:** None
///
/// **Returns:** `(key: String, value: String)` for each row that exists,
/// for the keys `server_name`, `server_description`, and `public_address`.
/// Missing keys are simply absent from the result set; the caller maps
/// remaining keys to defaults / `None`.
///
/// **Note:** Single-query bundle (rather than three separate
/// `SQL_GET_CONFIG` calls) — small efficiency win on the tracker task's
/// per-refresh hot path.
pub const SQL_GET_TRACKER_CONFIG_FIELDS: &str = "SELECT key, value FROM config WHERE key IN ('server_name', 'server_description', 'public_address')";

// ========================================================================
// User Query Operations
// ========================================================================

/// Guest account username constant
///
/// The guest account is a special shared account that allows passwordless login.
/// This username is reserved and cannot be used for other accounts.
pub const GUEST_USERNAME: &str = "guest";

/// Count non-guest users in the database
///
/// **Parameters:** None
///
/// **Returns:** `(count: i64)` - Total number of non-guest users
///
/// **Note:** Used in `create_first_user_if_none_exist()` to check if any real users exist.
/// The guest account is excluded so the first non-guest user becomes admin.
pub const SQL_COUNT_NON_GUEST_USERS: &str =
    "SELECT COUNT(*) FROM users WHERE LOWER(username) != 'guest'";

/// Select user by username (case-insensitive lookup)
///
/// **Parameters:**
/// 1. `username: &str` - Username to search for
///
/// **Returns:** `(id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id: Option<i64>)`
///
/// **Note:** Uses `LOWER()` for case-insensitive matching while preserving
/// the original case in the returned username.
pub const SQL_SELECT_USER_BY_USERNAME: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id FROM users WHERE LOWER(username) = LOWER(?)";

/// Get the guest account's `enabled` flag.
///
/// **Parameters:** None
///
/// **Returns:** `(enabled: bool)` — single row from the `users` table
/// matching the guest account, or no row if the guest account is
/// missing (catastrophic; bootstrap migration ensures it exists).
///
/// **Note:** Used by the tracker task to populate
/// `TrackerServerRegister.allows_guest`.
pub const SQL_GET_GUEST_ENABLED: &str = "SELECT enabled FROM users WHERE LOWER(username) = 'guest'";

/// Select all users (for user management listing)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id: Option<i64>)`
///
/// **Note:** Used by `/list all` command for user management.
/// Results are sorted alphabetically by username (case-insensitive).
pub const SQL_SELECT_ALL_USERS: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id FROM users ORDER BY LOWER(username)";

/// Check if a username exists (case-insensitive)
///
/// **Parameters:**
/// 1. `username: &str` - Username to check
///
/// **Returns:** `(count: i64)` - 1 if exists, 0 if not
///
/// **Note:** Used to check if a shared account nickname collides with an existing username.
pub const SQL_CHECK_USERNAME_EXISTS: &str =
    "SELECT COUNT(*) FROM users WHERE LOWER(username) = LOWER(?)";

// ========================================================================
// Permission Query Operations
// ========================================================================

/// Select all permissions for a user with override type
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Returns:** Multiple rows of `(permission: String, override_type: String)`
///
/// **Note:** The `override_type` is either `'grant'` or `'revoke'`.
pub const SQL_SELECT_PERMISSIONS: &str =
    "SELECT permission, override_type FROM user_permissions WHERE user_id = ?";

/// Select only grant overrides for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Returns:** Multiple rows of `(permission: String)`
pub const SQL_SELECT_GRANT_PERMISSIONS: &str =
    "SELECT permission FROM user_permissions WHERE user_id = ? AND override_type = 'grant'";

/// Select only revoke overrides for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Returns:** Multiple rows of `(permission: String)`
pub const SQL_SELECT_REVOKE_PERMISSIONS: &str =
    "SELECT permission FROM user_permissions WHERE user_id = ? AND override_type = 'revoke'";

/// Delete only revoke overrides for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
pub const SQL_DELETE_REVOKE_PERMISSIONS: &str =
    "DELETE FROM user_permissions WHERE user_id = ? AND override_type = 'revoke'";

/// Delete a specific grant override for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
/// 2. `permission: &str` - Permission name (snake_case)
pub const SQL_DELETE_GRANT_PERMISSION: &str =
    "DELETE FROM user_permissions WHERE user_id = ? AND permission = ? AND override_type = 'grant'";

/// Select a user's group_id
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Returns:** `(group_id: Option<i64>)`
///
/// **Note:** Lightweight query used by `get_user_permissions()` to check
/// if group-based permission resolution is needed.
pub const SQL_SELECT_USER_GROUP_ID: &str = "SELECT group_id FROM users WHERE id = ?";

/// Delete all permissions for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Note:** Used when replacing permissions or promoting user to admin.
pub const SQL_DELETE_PERMISSIONS: &str = "DELETE FROM user_permissions WHERE user_id = ?";

/// Insert a permission for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
/// 2. `permission: &str` - Permission name (snake_case)
pub const SQL_INSERT_PERMISSION: &str =
    "INSERT INTO user_permissions (user_id, permission) VALUES (?, ?)";

/// Insert a permission for a user with override type
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
/// 2. `permission: &str` - Permission name (snake_case)
/// 3. `override_type: &str` - Either `'grant'` or `'revoke'`
pub const SQL_INSERT_PERMISSION_OVERRIDE: &str =
    "INSERT INTO user_permissions (user_id, permission, override_type) VALUES (?, ?, ?)";

// ========================================================================
// User Mutation Operations
// ========================================================================

/// Insert a new user
///
/// **Parameters:**
/// 1. `username: &str` - Username
/// 2. `password_hash: &str` - Hashed password
/// 3. `is_admin: bool` - Admin status
/// 4. `is_shared: bool` - Shared account status
/// 5. `enabled: bool` - Enabled status
/// 6. `created_at: i64` - Unix timestamp
/// 7. `group_id: Option<i64>` - Optional group assignment
///
/// **Returns:** `last_insert_rowid()` - The new user's ID
pub const SQL_INSERT_USER: &str = "INSERT INTO users (username, password_hash, is_admin, is_shared, enabled, created_at, group_id) VALUES (?, ?, ?, ?, ?, ?, ?)";

/// Update user with atomic protection for last admin/enabled admin
///
/// **Parameters:**
/// 1. `username: &str` - New username
/// 2. `password_hash: &str` - New password hash
/// 3. `is_admin: bool` - New admin status
/// 4. `enabled: bool` - New enabled status
/// 5. `user_id: i64` - User ID to update
/// 6. `enabled: bool` - (Duplicate) Final enabled status for protection check
/// 7. `is_admin: bool` - (Duplicate) Final admin status for protection check
///
/// **Note:** `is_shared` is not updated - it is immutable once set at creation.
///
/// **Atomic Protection:**
/// - Prevents disabling the last enabled admin
/// - Prevents demoting the last admin
/// - Uses compound WHERE clauses with subqueries to check counts atomically
/// - Returns 0 rows affected if blocked by protection
///
/// **TOCTOU Prevention:** All checks happen in a single SQL statement,
/// preventing race conditions where multiple simultaneous updates could
/// leave the system with zero enabled admins or zero admins.
pub const SQL_UPDATE_USER: &str = "UPDATE users
    SET username = ?, password_hash = ?, is_admin = ?, enabled = ?
    WHERE id = ?
    AND (
        -- Enabled protection: allow enabling, allow non-admin disable, allow if multiple enabled admins
        ? = 1
        OR is_admin = 0
        OR (SELECT COUNT(*) FROM users WHERE is_admin = 1 AND enabled = 1) > 1
    )
    AND (
        -- is_admin protection: allow promoting, allow if currently non-admin, allow if multiple admins
        ? = 1
        OR is_admin = 0
        OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
    )";

/// Update a user's group assignment
///
/// **Parameters:**
/// 1. `group_id: Option<i64>` - New group ID (NULL to remove)
/// 2. `user_id: i64` - User ID
pub const SQL_UPDATE_USER_GROUP: &str = "UPDATE users SET group_id = ? WHERE id = ?";

/// Delete user with atomic protection for last admin
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID to delete
///
/// **Atomic Protection:**
/// - Prevents deleting the last admin user
/// - Uses subquery to check admin count atomically
/// - Returns 0 rows affected if blocked by protection
///
/// **TOCTOU Prevention:** The admin count check and deletion happen in a
/// single SQL statement, preventing race conditions where two admins could
/// simultaneously delete each other, leaving zero admins.
///
/// **Cascade:** Foreign key constraints automatically delete associated
/// permissions when the user is deleted.
pub const SQL_DELETE_USER_ATOMIC: &str = "DELETE FROM users
     WHERE id = ?
     AND (
         is_admin = 0
         OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
     )";

// ========================================================================
// News Query Operations
// ========================================================================

/// Select all news items ordered by creation time (newest first)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(id, body, image, author_id, author_username, author_is_admin, created_at, updated_at)`
///
/// **Note:** Joins with users table to get author information.
/// Results are sorted by created_at descending (newest first).
pub const SQL_SELECT_ALL_NEWS: &str = "
    SELECT 
        n.id,
        n.body,
        n.image,
        n.author_id,
        u.username as author_username,
        u.is_admin as author_is_admin,
        n.created_at,
        n.updated_at
    FROM news n
    JOIN users u ON n.author_id = u.id
    ORDER BY n.created_at DESC";

/// Select a single news item by ID
///
/// **Parameters:**
/// 1. `id: i64` - News item ID
///
/// **Returns:** `(id, body, image, author_id, author_username, author_is_admin, created_at, updated_at)`
///
/// **Note:** Joins with users table to get author information.
pub const SQL_SELECT_NEWS_BY_ID: &str = "
    SELECT 
        n.id,
        n.body,
        n.image,
        n.author_id,
        u.username as author_username,
        u.is_admin as author_is_admin,
        n.created_at,
        n.updated_at
    FROM news n
    JOIN users u ON n.author_id = u.id
    WHERE n.id = ?";

/// Insert a new news item
///
/// **Parameters:**
/// 1. `body: Option<&str>` - Markdown body text (nullable)
/// 2. `image: Option<&str>` - Image data URI (nullable)
/// 3. `author_id: i64` - Author's user ID
/// 4. `created_at: &str` - ISO 8601 timestamp
///
/// **Returns:** `last_insert_rowid()` - The new news item's ID
///
/// **Note:** At least one of body or image must be non-null (enforced by CHECK constraint).
pub const SQL_INSERT_NEWS: &str = "
    INSERT INTO news (body, image, author_id, created_at)
    VALUES (?, ?, ?, ?)";

/// Update a news item
///
/// **Parameters:**
/// 1. `body: Option<&str>` - New markdown body text (nullable)
/// 2. `image: Option<&str>` - New image data URI (nullable)
/// 3. `updated_at: &str` - ISO 8601 timestamp
/// 4. `id: i64` - News item ID
///
/// **Note:** At least one of body or image must be non-null (enforced by CHECK constraint).
pub const SQL_UPDATE_NEWS: &str = "
    UPDATE news
    SET body = ?, image = ?, updated_at = ?
    WHERE id = ?";

/// Delete a news item
///
/// **Parameters:**
/// 1. `id: i64` - News item ID
pub const SQL_DELETE_NEWS: &str = "DELETE FROM news WHERE id = ?";

// ========================================================================
// Tracker Query Operations
// ========================================================================

/// Select all tracker rows ordered by name (case-insensitive).
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(id, address, port, fingerprint,
/// password, name, enabled, created_at, updated_at)`.
///
/// **Notes:**
/// - `port` is stored as INTEGER in SQLite but always fits in `u16`
///   because protocol-layer validation rejects out-of-range ports
///   before insert.
/// - The `LOWER(name)` order matches the case-insensitive uniqueness
///   on `name`; no tiebreaker is needed because two rows can't have
///   the same `LOWER(name)`.
pub const SQL_SELECT_ALL_TRACKERS: &str = "
    SELECT id, address, port, fingerprint, password, name, enabled,
           created_at, updated_at
    FROM trackers
    ORDER BY LOWER(name)";

/// Select a single tracker by id.
///
/// **Parameters:**
/// 1. `id: i64` - Tracker row id
///
/// **Returns:** `(id, address, port, fingerprint, password, name, enabled,
/// created_at, updated_at)`
pub const SQL_SELECT_TRACKER_BY_ID: &str = "
    SELECT id, address, port, fingerprint, password, name, enabled,
           created_at, updated_at
    FROM trackers
    WHERE id = ?";

/// Insert a tracker row.
///
/// **Parameters:**
/// 1. `address: &str` - Hostname or IP literal
/// 2. `port: i64` - TCP port (typically 7510)
/// 3. `fingerprint: Option<&str>` - TOFU pin; `None` on first connect
/// 4. `password: Option<&str>` - Registration password (`None` if open)
/// 5. `name: &str` - Admin label (required, non-empty)
/// 6. `enabled: bool` - Active flag
/// 7. `created_at: i64` - Unix epoch seconds
/// 8. `updated_at: i64` - Unix epoch seconds (typically equals
///    `created_at` on insert)
/// 9. `cap: i64` - Maximum tracker rows allowed; the insert is a
///    no-op (0 rows affected) if the table already has `>= cap` rows.
///
/// **Returns:** `last_insert_rowid()` on success, or 0 rows affected
/// if the cap was reached. Caller distinguishes via `rows_affected()`.
///
/// **Note:** Insert fails with a UNIQUE-constraint violation if
/// another row with the same `(address, port)` exists. The cap check
/// runs atomically with the insert via `INSERT … SELECT … WHERE`, so
/// concurrent admin requests at `count == cap - 1` cannot both
/// succeed (matching the admin-protection pattern used elsewhere in
/// this file).
pub const SQL_INSERT_TRACKER: &str = "
    INSERT INTO trackers (address, port, fingerprint, password, name, enabled,
                          created_at, updated_at)
    SELECT ?, ?, ?, ?, ?, ?, ?, ?
    WHERE (SELECT COUNT(*) FROM trackers) < ?";

/// Replace a tracker row's mutable fields by id (full-row update).
///
/// **Parameters:**
/// 1. `address: &str`
/// 2. `port: i64`
/// 3. `fingerprint: Option<&str>`
/// 4. `password: Option<&str>`
/// 5. `name: &str`
/// 6. `enabled: bool`
/// 7. `updated_at: i64`
/// 8. `id: i64`
///
/// **Note:** `created_at` is preserved. Update fails with a UNIQUE-
/// constraint violation if changing `(address, port)` collides with
/// another row.
pub const SQL_UPDATE_TRACKER: &str = "
    UPDATE trackers
    SET address = ?, port = ?, fingerprint = ?, password = ?, name = ?,
        enabled = ?, updated_at = ?
    WHERE id = ?";

/// Narrow update: replace only the fingerprint and the `updated_at`
/// timestamp. Used by the tracker task on TOFU first-connect (when
/// the row's fingerprint was `NULL`) and on admin-accept after a
/// fingerprint mismatch — both paths only need to update the pin
/// without touching other fields.
///
/// **Parameters:**
/// 1. `fingerprint: &str` - Newly accepted canonical fingerprint
/// 2. `updated_at: i64` - Unix epoch seconds
/// 3. `id: i64`
pub const SQL_UPDATE_TRACKER_FINGERPRINT: &str = "
    UPDATE trackers
    SET fingerprint = ?, updated_at = ?
    WHERE id = ?";

/// Delete a tracker row by id.
///
/// **Parameters:**
/// 1. `id: i64`
pub const SQL_DELETE_TRACKER: &str = "DELETE FROM trackers WHERE id = ?";

// ========================================================================
// IP Ban Query Operations
// ========================================================================

/// Insert or update an IP ban (upsert)
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to ban
/// 2. `nickname: Option<&str>` - Nickname annotation (if banned by nickname)
/// 3. `reason: Option<&str>` - Reason for the ban
/// 4. `created_by: &str` - Username of admin who created the ban
/// 5. `created_at: i64` - Unix timestamp when ban was created
/// 6. `expires_at: Option<i64>` - Unix timestamp when ban expires (None = permanent)
///
/// **Note:** Uses `ON CONFLICT` to update all fields if IP already exists.
pub const SQL_UPSERT_BAN: &str = "
    INSERT INTO ip_bans (ip_address, nickname, reason, created_by, created_at, expires_at)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(ip_address) DO UPDATE SET
        nickname = excluded.nickname,
        reason = excluded.reason,
        created_by = excluded.created_by,
        created_at = excluded.created_at,
        expires_at = excluded.expires_at";

/// Select a ban by IP address (regardless of expiry status)
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to look up
///
/// **Returns:** `(ip_address, nickname, reason, created_by, created_at, expires_at)`
///
/// **Note:** Returns the ban even if expired. Used internally after upsert.
pub const SQL_SELECT_BAN_BY_IP_UNFILTERED: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_bans
    WHERE ip_address = ?";

/// Delete a ban by IP address
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to unban
pub const SQL_DELETE_BAN_BY_IP: &str = "DELETE FROM ip_bans WHERE ip_address = ?";

/// Select all IP addresses with a given nickname annotation
///
/// **Parameters:**
/// 1. `nickname: &str` - Nickname to search for
///
/// **Returns:** Multiple rows of `(ip_address: String)`
pub const SQL_SELECT_IPS_BY_NICKNAME: &str = "
    SELECT ip_address FROM ip_bans WHERE nickname = ?";

/// Delete all bans with a given nickname annotation
///
/// **Parameters:**
/// 1. `nickname: &str` - Nickname to delete bans for
pub const SQL_DELETE_BANS_BY_NICKNAME: &str = "DELETE FROM ip_bans WHERE nickname = ?";

/// Count bans with a given nickname annotation
///
/// **Parameters:**
/// 1. `nickname: &str` - Nickname to count bans for
///
/// **Returns:** `(count: i64)`
pub const SQL_COUNT_BANS_BY_NICKNAME: &str = "SELECT COUNT(*) FROM ip_bans WHERE nickname = ?";

/// Select all active (non-expired) bans
///
/// **Parameters:**
/// 1. `now: i64` - Current Unix timestamp
///
/// **Returns:** Multiple rows of `(ip_address, nickname, reason, created_by, created_at, expires_at)`
///
/// **Note:** Results are sorted by creation time (newest first).
pub const SQL_SELECT_ACTIVE_BANS: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_bans
    WHERE expires_at IS NULL OR expires_at > ?
    ORDER BY created_at DESC";

/// Delete all expired bans
///
/// **Parameters:**
/// 1. `now: i64` - Current Unix timestamp
///
/// **Note:** Only deletes bans with a non-null expires_at that is <= now.
/// Called on server startup to clean up stale entries.
pub const SQL_DELETE_EXPIRED_BANS: &str = "
    DELETE FROM ip_bans
    WHERE expires_at IS NOT NULL AND expires_at <= ?";

// =============================================================================
// IP Trusted
// =============================================================================

/// Insert or update a trusted IP entry (upsert)
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address or CIDR range
/// 2. `nickname: Option<&str>` - Nickname annotation (if trusted by nickname)
/// 3. `reason: Option<&str>` - Reason for trusting
/// 4. `created_by: &str` - Username of admin who created the trust
/// 5. `created_at: i64` - Unix timestamp of creation
/// 6. `expires_at: Option<i64>` - Unix timestamp when trust expires (NULL = permanent)
pub const SQL_UPSERT_TRUST: &str = "
    INSERT INTO ip_trusted (ip_address, nickname, reason, created_by, created_at, expires_at)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(ip_address) DO UPDATE SET
        nickname = excluded.nickname,
        reason = excluded.reason,
        created_by = excluded.created_by,
        created_at = excluded.created_at,
        expires_at = excluded.expires_at";

/// Select a trusted IP entry by IP address (regardless of expiry status)
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to look up
///
/// **Returns:** `(ip_address, nickname, reason, created_by, created_at, expires_at)`
pub const SQL_SELECT_TRUST_BY_IP_UNFILTERED: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_trusted
    WHERE ip_address = ?";

/// Delete a trusted IP entry by IP address
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to untrust
pub const SQL_DELETE_TRUST_BY_IP: &str = "DELETE FROM ip_trusted WHERE ip_address = ?";

/// Select all IP addresses with a given nickname annotation (trusted)
///
/// **Parameters:**
/// 1. `nickname: &str` - Nickname to look up
pub const SQL_SELECT_TRUSTED_IPS_BY_NICKNAME: &str = "
    SELECT ip_address FROM ip_trusted WHERE nickname = ?";

/// Delete all trusted IP entries with a given nickname annotation
///
/// **Parameters:**
/// 1. `nickname: &str` - Nickname to delete trusts for
pub const SQL_DELETE_TRUSTS_BY_NICKNAME: &str = "DELETE FROM ip_trusted WHERE nickname = ?";

/// Count trusted IP entries with a given nickname annotation
///
/// **Returns:** `(count: i64)`
pub const SQL_COUNT_TRUSTS_BY_NICKNAME: &str = "SELECT COUNT(*) FROM ip_trusted WHERE nickname = ?";

/// Select all active (non-expired) trusted IP entries
///
/// **Parameters:**
/// 1. `now: i64` - Current Unix timestamp
///
/// **Returns:** `(ip_address, nickname, reason, created_by, created_at, expires_at)`
/// Results are sorted by creation time (newest first).
pub const SQL_SELECT_ACTIVE_TRUSTS: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_trusted
    WHERE expires_at IS NULL OR expires_at > ?
    ORDER BY created_at DESC";

/// Delete all expired trusted IP entries
///
/// **Parameters:**
/// 1. `now: i64` - Current Unix timestamp
///
/// **Note:** Only deletes trusts with a non-null expires_at that is <= now.
/// Called on server startup to clean up stale entries.
pub const SQL_DELETE_EXPIRED_TRUSTS: &str = "
    DELETE FROM ip_trusted
    WHERE expires_at IS NOT NULL AND expires_at <= ?";

// ========================================================================
// Group Query Operations
// ========================================================================

/// Select all groups ordered by name (case-insensitive)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(id: i64, name: String, is_shared: bool)`
///
/// **Note:** Member count and permissions are fetched separately per group.
pub const SQL_SELECT_ALL_GROUPS: &str =
    "SELECT id, name, is_shared FROM groups ORDER BY LOWER(name)";

/// Select a group by ID
///
/// **Parameters:**
/// 1. `id: i64` - Group ID
///
/// **Returns:** `(id: i64, name: String, is_shared: bool)`
pub const SQL_SELECT_GROUP_BY_ID: &str = "SELECT id, name, is_shared FROM groups WHERE id = ?";

/// Select a group by name (case-insensitive)
///
/// **Parameters:**
/// 1. `name: &str` - Group name
///
/// **Returns:** `(id: i64, name: String, is_shared: bool)`
///
/// **Note:** Uses `LOWER()` for case-insensitive matching while preserving
/// the original case in the returned name.
pub const SQL_SELECT_GROUP_BY_NAME: &str =
    "SELECT id, name, is_shared FROM groups WHERE LOWER(name) = LOWER(?)";

/// Count users assigned to a group
///
/// **Parameters:**
/// 1. `group_id: i64` - Group ID
///
/// **Returns:** `(count: i64)`
pub const SQL_COUNT_GROUP_MEMBERS: &str = "SELECT COUNT(*) FROM users WHERE group_id = ?";

/// Count users assigned to each group (bulk)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(group_id: i64, count: i64)`
pub const SQL_COUNT_ALL_GROUP_MEMBERS: &str =
    "SELECT group_id, COUNT(*) FROM users WHERE group_id IS NOT NULL GROUP BY group_id";

/// Select all permissions for a group
///
/// **Parameters:**
/// 1. `group_id: i64` - Group ID
///
/// **Returns:** Multiple rows of `(permission: String)`
pub const SQL_SELECT_GROUP_PERMISSIONS: &str =
    "SELECT permission FROM group_permissions WHERE group_id = ? ORDER BY permission";

/// Select all permissions for all groups (bulk)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(group_id: i64, permission: String)`
pub const SQL_SELECT_ALL_GROUP_PERMISSIONS: &str =
    "SELECT group_id, permission FROM group_permissions ORDER BY group_id, permission";

/// Delete all permissions for a group
///
/// **Parameters:**
/// 1. `group_id: i64` - Group ID
///
/// **Note:** Used when replacing group permissions during an update.
pub const SQL_DELETE_GROUP_PERMISSIONS: &str = "DELETE FROM group_permissions WHERE group_id = ?";

/// Insert a permission for a group
///
/// **Parameters:**
/// 1. `group_id: i64` - Group ID
/// 2. `permission: &str` - Permission name (snake_case)
pub const SQL_INSERT_GROUP_PERMISSION: &str =
    "INSERT INTO group_permissions (group_id, permission) VALUES (?, ?)";

/// Insert a new group
///
/// **Parameters:**
/// 1. `name: &str` - Group name
/// 2. `is_shared: bool` - Whether this is a shared account group
///
/// **Returns:** `last_insert_rowid()` - The new group's ID
pub const SQL_INSERT_GROUP: &str = "INSERT INTO groups (name, is_shared) VALUES (?, ?)";

/// Update a group's name and shared status
///
/// **Parameters:**
/// 1. `name: &str` - New group name
/// 2. `is_shared: bool` - New shared status
/// 3. `id: i64` - Group ID to update
/// 4. `is_shared: bool` - (Duplicate) New shared status for atomic shared-toggle check
/// 5. `id: i64` - (Duplicate) Group ID for member-count subquery
///
/// **Atomic Protection:**
/// - Prevents toggling `is_shared` while the group has assigned members
/// - When `is_shared` is unchanged, the `is_shared = ?` condition passes and
///   the member-count subquery is never evaluated
/// - When `is_shared` IS changing, the update only proceeds if the group has
///   zero members, preventing a TOCTOU race between the handler's pre-check
///   and the actual update
/// - Returns 0 rows affected if blocked by protection
pub const SQL_UPDATE_GROUP: &str = "UPDATE groups SET name = ?, is_shared = ?
    WHERE id = ?
    AND (
        is_shared = ?
        OR (SELECT COUNT(*) FROM users WHERE group_id = ?) = 0
    )";

/// Atomically delete a group by ID, only if it has no assigned members
///
/// **Parameters:**
/// 1. `id: i64` - Group ID to delete
/// 2. `id: i64` - Group ID (repeated for subquery bind)
///
/// Returns rows_affected = 1 on success, 0 if group not found OR has members.
/// Caller should distinguish those cases with a follow-up SELECT if needed.
pub const SQL_DELETE_GROUP: &str =
    "DELETE FROM groups WHERE id = ? AND (SELECT COUNT(*) FROM users WHERE group_id = ?) = 0";

// ========================================================================
// Channel Settings Query Operations
// ========================================================================

/// Select channel settings by name (case-insensitive)
///
/// **Parameters:**
/// 1. `name: &str` - Channel name
///
/// **Returns:** `(name: String, topic: String, topic_set_by: String, secret: i32)`
///
/// **Note:** Uses `LOWER()` for case-insensitive matching while preserving
/// the original case in the returned name.
pub const SQL_SELECT_CHANNEL_SETTINGS: &str =
    "SELECT name, topic, topic_set_by, secret FROM channel_settings WHERE LOWER(name) = LOWER(?)";

/// Select all channel settings
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(name: String, topic: String, topic_set_by: String, secret: i32)`
pub const SQL_SELECT_ALL_CHANNEL_SETTINGS: &str =
    "SELECT name, topic, topic_set_by, secret FROM channel_settings";

/// Insert or update channel settings (upsert)
///
/// **Parameters:**
/// 1. `name: &str` - Channel name
/// 2. `topic: &str` - Channel topic
/// 3. `topic_set_by: &str` - Username of who set the topic
/// 4. `secret: i32` - Whether the channel is secret (0 or 1)
///
/// **Note:** Uses SQLite `ON CONFLICT` for upsert semantics — creates if
/// the channel doesn't exist, updates if it does.
pub const SQL_UPSERT_CHANNEL_SETTINGS: &str = "INSERT INTO channel_settings (name, topic, topic_set_by, secret) VALUES (?, ?, ?, ?) ON CONFLICT(name) DO UPDATE SET topic = excluded.topic, topic_set_by = excluded.topic_set_by, secret = excluded.secret";

/// Update topic for a channel (case-insensitive name match)
///
/// **Parameters:**
/// 1. `topic: &str` - New topic text
/// 2. `topic_set_by: &str` - Username of who set the topic
/// 3. `name: &str` - Channel name
pub const SQL_UPDATE_CHANNEL_TOPIC: &str =
    "UPDATE channel_settings SET topic = ?, topic_set_by = ? WHERE LOWER(name) = LOWER(?)";

/// Update secret flag for a channel (case-insensitive name match)
///
/// **Parameters:**
/// 1. `secret: i32` - Whether the channel is secret (0 or 1)
/// 2. `name: &str` - Channel name
pub const SQL_UPDATE_CHANNEL_SECRET: &str =
    "UPDATE channel_settings SET secret = ? WHERE LOWER(name) = LOWER(?)";

/// Delete channel settings (case-insensitive name match)
///
/// **Parameters:**
/// 1. `name: &str` - Channel name
pub const SQL_DELETE_CHANNEL_SETTINGS: &str =
    "DELETE FROM channel_settings WHERE LOWER(name) = LOWER(?)";

// ========================================================================
// Test-Only Query Operations
// ========================================================================

/// Select user by ID
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID to look up
///
/// **Returns:** `(id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id: Option<i64>)`
///
/// Select a user by ID
pub const SQL_SELECT_USER_BY_ID: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id FROM users WHERE id = ?";

/// Check if user is admin
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID to check
///
/// **Returns:** `(is_admin: bool)`
///
/// **Note:** Only used in tests. Production code uses cached permissions.
#[cfg(test)]
pub const SQL_CHECK_IS_ADMIN: &str = "SELECT is_admin FROM users WHERE id = ?";

/// Select a ban by IP address (only if not expired)
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to look up
/// 2. `now: i64` - Current Unix timestamp
///
/// **Returns:** `(ip_address, nickname, reason, created_by, created_at, expires_at)`
///
/// **Note:** Only returns bans that are either permanent (expires_at IS NULL)
/// or not yet expired (expires_at > now).
/// Used in tests only — production code uses the in-memory BanCache.
#[cfg(test)]
pub const SQL_SELECT_BAN_BY_IP: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_bans
    WHERE ip_address = ?
    AND (expires_at IS NULL OR expires_at > ?)";

/// Select a trusted IP entry by IP address (only if not expired)
///
/// **Parameters:**
/// 1. `ip_address: &str` - IP address to look up
/// 2. `now: i64` - Current Unix timestamp
///
/// **Returns:** `(ip_address, nickname, reason, created_by, created_at, expires_at)`
///
/// **Note:** Used in tests only — production code uses the in-memory TrustCache.
#[cfg(test)]
pub const SQL_SELECT_TRUST_BY_IP: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_trusted
    WHERE ip_address = ?
    AND (expires_at IS NULL OR expires_at > ?)";

/// Count admin users in the database
///
/// **Parameters:** None
///
/// **Returns:** `(count: i64)` - Number of admin users
///
/// **Note:** Used in test utilities to verify admin count in race condition tests.
#[cfg(test)]
pub const SQL_COUNT_ADMINS: &str = "SELECT COUNT(*) FROM users WHERE is_admin = 1";

/// Check if channel settings exist (case-insensitive name match)
///
/// **Parameters:**
/// 1. `name: &str` - Channel name
///
/// **Returns:** `(count: i32)`
///
/// **Note:** Used in tests only — production code does not need this check.
#[cfg(test)]
pub const SQL_COUNT_CHANNEL_SETTINGS: &str =
    "SELECT COUNT(*) FROM channel_settings WHERE LOWER(name) = LOWER(?)";

/// Count all user_permissions rows for a given user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Note:** Used in tests to verify permission insertion and cascade deletion.
#[cfg(test)]
pub const SQL_COUNT_USER_PERMISSIONS: &str =
    "SELECT COUNT(*) FROM user_permissions WHERE user_id = ?";

/// Count all users in the database
///
/// **Note:** Used in tests to verify user creation and first-user logic.
#[cfg(test)]
pub const SQL_COUNT_USERS: &str = "SELECT COUNT(*) FROM users";

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

// ========================================================================
// Chat State Query Operations
// ========================================================================

/// Get a chat state value by key
///
/// **Parameters:**
/// 1. `key: &str` - Chat state key to look up
///
/// **Returns:** `(value: String)`
pub const SQL_GET_CHAT_STATE: &str = "SELECT value FROM chat_state WHERE key = ?";

/// Set a chat state value (insert or replace)
///
/// **Parameters:**
/// 1. `key: &str` - Chat state key
/// 2. `value: &str` - Chat state value
///
/// **Note:** Uses `INSERT OR REPLACE` to upsert the value.
pub const SQL_SET_CHAT_STATE: &str = "INSERT OR REPLACE INTO chat_state (key, value) VALUES (?, ?)";

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
/// **Returns:** `(id, username, password_hash, is_admin, is_shared, enabled, created_at)`
///
/// **Note:** Uses `LOWER()` for case-insensitive matching while preserving
/// the original case in the returned username.
pub const SQL_SELECT_USER_BY_USERNAME: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at FROM users WHERE LOWER(username) = LOWER(?)";

/// Select user by ID
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID to look up
///
/// **Returns:** `(id, username, password_hash, is_admin, is_shared, enabled, created_at)`
///
/// Note: Only used in tests. Production code looks up users by username.
#[cfg(test)]
pub const SQL_SELECT_USER_BY_ID: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at FROM users WHERE id = ?";

/// Select all users (for user management listing)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(id, username, password_hash, is_admin, is_shared, enabled, created_at)`
///
/// **Note:** Used by `/list all` command for user management.
/// Results are sorted alphabetically by username (case-insensitive).
pub const SQL_SELECT_ALL_USERS: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at FROM users ORDER BY LOWER(username)";

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

/// Check if user is admin
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID to check
///
/// **Returns:** `(is_admin: bool)`
///
/// Note: Only used in tests. Production code uses cached permissions.
#[cfg(test)]
pub const SQL_CHECK_IS_ADMIN: &str = "SELECT is_admin FROM users WHERE id = ?";

// ========================================================================
// Permission Query Operations
// ========================================================================

/// Count permissions for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
/// 2. `permission: &str` - Permission name (snake_case)
///
/// **Returns:** `(count: i64)` - Number of matching permissions (0 or 1)
///
/// Note: Only used in tests. Production code uses cached permissions.
#[cfg(test)]
pub const SQL_COUNT_PERMISSION: &str =
    "SELECT COUNT(*) FROM user_permissions WHERE user_id = ? AND permission = ?";

/// Select all permissions for a user
///
/// **Parameters:**
/// 1. `user_id: i64` - User ID
///
/// **Returns:** Multiple rows of `(permission: String)`
pub const SQL_SELECT_PERMISSIONS: &str =
    "SELECT permission FROM user_permissions WHERE user_id = ?";

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
///
/// **Returns:** `last_insert_rowid()` - The new user's ID
pub const SQL_INSERT_USER: &str = "INSERT INTO users (username, password_hash, is_admin, is_shared, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?)";

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

/// Select all news items ordered by creation time (oldest first)
///
/// **Parameters:** None
///
/// **Returns:** Multiple rows of `(id, body, image, author_id, author_username, author_is_admin, created_at, updated_at)`
///
/// **Note:** Joins with users table to get author information.
/// Results are sorted by created_at ascending (oldest first).
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
    ORDER BY n.created_at ASC";

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

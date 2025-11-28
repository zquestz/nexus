//! SQL query constants for user operations

/// Select user by username (case-insensitive)
pub const SQL_SELECT_USER_BY_USERNAME: &str = "SELECT id, username, password_hash, is_admin, enabled, created_at FROM users WHERE LOWER(username) = LOWER(?)";

/// Select user by ID
pub const SQL_SELECT_USER_BY_ID: &str =
    "SELECT id, username, password_hash, is_admin, enabled, created_at FROM users WHERE id = ?";

/// Check if user is admin
pub const SQL_CHECK_IS_ADMIN: &str = "SELECT is_admin FROM users WHERE id = ?";

/// Count permissions for a user
pub const SQL_COUNT_PERMISSION: &str =
    "SELECT COUNT(*) FROM user_permissions WHERE user_id = ? AND permission = ?";

/// Select all permissions for a user
pub const SQL_SELECT_PERMISSIONS: &str =
    "SELECT permission FROM user_permissions WHERE user_id = ?";

/// Delete all permissions for a user
pub const SQL_DELETE_PERMISSIONS: &str = "DELETE FROM user_permissions WHERE user_id = ?";

/// Insert a permission for a user
pub const SQL_INSERT_PERMISSION: &str =
    "INSERT INTO user_permissions (user_id, permission) VALUES (?, ?)";

/// Insert a new user
pub const SQL_INSERT_USER: &str = "INSERT INTO users (username, password_hash, is_admin, enabled, created_at) VALUES (?, ?, ?, ?, ?)";

/// Update user with atomic protection for last admin/enabled admin
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
pub const SQL_DELETE_USER_ATOMIC: &str = "DELETE FROM users
     WHERE id = ?
     AND (
         is_admin = 0
         OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
     )";

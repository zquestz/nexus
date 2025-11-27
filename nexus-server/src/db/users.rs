//! User account database operations

use super::permissions::{Permission, Permissions};
use sqlx::SqlitePool;

// SQL query constants
const SQL_SELECT_USER_BY_USERNAME: &str = "SELECT id, username, password_hash, is_admin, enabled, created_at FROM users WHERE LOWER(username) = LOWER(?)";
const SQL_SELECT_USER_BY_ID: &str =
    "SELECT id, username, password_hash, is_admin, enabled, created_at FROM users WHERE id = ?";
const SQL_CHECK_IS_ADMIN: &str = "SELECT is_admin FROM users WHERE id = ?";
const SQL_COUNT_PERMISSION: &str =
    "SELECT COUNT(*) FROM user_permissions WHERE user_id = ? AND permission = ?";
const SQL_SELECT_PERMISSIONS: &str = "SELECT permission FROM user_permissions WHERE user_id = ?";
const SQL_DELETE_PERMISSIONS: &str = "DELETE FROM user_permissions WHERE user_id = ?";
const SQL_INSERT_PERMISSION: &str =
    "INSERT INTO user_permissions (user_id, permission) VALUES (?, ?)";
const SQL_INSERT_USER: &str = "INSERT INTO users (username, password_hash, is_admin, enabled, created_at) VALUES (?, ?, ?, ?, ?)";
const SQL_UPDATE_USER: &str = "UPDATE users 
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

const SQL_DELETE_USER_ATOMIC: &str = "DELETE FROM users
     WHERE id = ?
     AND (
         is_admin = 0
         OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
     )";

/// User account stored in database
#[derive(Debug, Clone)]
pub struct UserAccount {
    pub id: i64,
    pub username: String,
    pub hashed_password: String,
    pub is_admin: bool,
    pub enabled: bool,
    pub created_at: i64,
}

/// Database operations for user accounts
#[derive(Clone)]
pub struct UserDb {
    pool: SqlitePool,
}

impl UserDb {
    // ========================================================================
    // Constructor
    // ========================================================================

    /// Create a new UserDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a reference to the connection pool (for testing)
    #[cfg(test)]
    #[allow(dead_code)] // Used in bin tests
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ========================================================================
    // Query Methods - User Lookup
    // ========================================================================

    /// Get a user by ID
    pub async fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserAccount>, sqlx::Error> {
        let user: Option<(i64, String, String, bool, bool, i64)> =
            sqlx::query_as(SQL_SELECT_USER_BY_ID)
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(user.map(
            |(id, username, hashed_password, is_admin, enabled, created_at)| UserAccount {
                id,
                username,
                hashed_password,
                is_admin,
                enabled,
                created_at,
            },
        ))
    }

    /// Get a user by username (case-insensitive lookup)
    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserAccount>, sqlx::Error> {
        let user: Option<(i64, String, String, bool, bool, i64)> =
            sqlx::query_as(SQL_SELECT_USER_BY_USERNAME)
                .bind(username)
                .fetch_optional(&self.pool)
                .await?;

        Ok(user.map(
            |(id, username, hashed_password, is_admin, enabled, created_at)| UserAccount {
                id,
                username,
                hashed_password,
                is_admin,
                enabled,
                created_at,
            },
        ))
    }

    // ========================================================================
    // Permission Methods
    // ========================================================================

    /// Get all permissions for a user
    pub async fn get_user_permissions(&self, user_id: i64) -> Result<Permissions, sqlx::Error> {
        let perm_rows: Vec<(String,)> = sqlx::query_as(SQL_SELECT_PERMISSIONS)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;

        let mut permissions = Permissions::new();
        for (perm_str,) in perm_rows {
            if let Some(perm) = Permission::parse(&perm_str) {
                permissions.permissions.insert(perm);
            }
        }

        Ok(permissions)
    }

    /// Check if user has a specific permission (with admin override)
    pub async fn has_permission(
        &self,
        user_id: i64,
        permission: Permission,
    ) -> Result<bool, sqlx::Error> {
        // Check if user is admin (admins have all permissions)
        let is_admin: Option<(bool,)> = sqlx::query_as(SQL_CHECK_IS_ADMIN)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

        // User doesn't exist
        let Some((is_admin,)) = is_admin else {
            return Ok(false);
        };

        if is_admin {
            return Ok(true);
        }

        let count: (i64,) = sqlx::query_as(SQL_COUNT_PERMISSION)
            .bind(user_id)
            .bind(permission.as_str())
            .fetch_one(&self.pool)
            .await?;

        Ok(count.0 > 0)
    }

    /// Set permissions for a user (replaces all existing permissions)
    ///
    /// This operation is atomic - uses a transaction to ensure either all permissions
    /// are updated or none are (prevents partial permission states on errors).
    pub async fn set_permissions(
        &self,
        user_id: i64,
        permissions: &Permissions,
    ) -> Result<(), sqlx::Error> {
        // Use a transaction to make this atomic
        let mut tx = self.pool.begin().await?;

        // Delete existing permissions
        sqlx::query(SQL_DELETE_PERMISSIONS)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        // Insert new permissions
        for perm in permissions.to_vec() {
            sqlx::query(SQL_INSERT_PERMISSION)
                .bind(user_id)
                .bind(perm.as_str())
                .execute(&mut *tx)
                .await?;
        }

        // Commit the transaction
        tx.commit().await?;

        Ok(())
    }

    // ========================================================================
    // Mutation Methods - Create/Update/Delete
    // ========================================================================

    /// Create a new user account with permissions
    pub async fn create_user(
        &self,
        username: &str,
        hashed_password: &str,
        is_admin: bool,
        enabled: bool,
        permissions: &Permissions,
    ) -> Result<UserAccount, sqlx::Error> {
        let created_at = chrono::Utc::now().timestamp();

        let result = sqlx::query(SQL_INSERT_USER)
            .bind(username)
            .bind(hashed_password)
            .bind(is_admin)
            .bind(enabled)
            .bind(created_at)
            .execute(&self.pool)
            .await?;

        let user_id = result.last_insert_rowid();

        // Only set permissions for non-admin users
        // Admins automatically get all permissions via has_permission()
        if !is_admin {
            self.set_permissions(user_id, permissions).await?;
        }

        Ok(UserAccount {
            id: user_id,
            username: username.to_string(),
            hashed_password: hashed_password.to_string(),
            is_admin,
            enabled,
            created_at,
        })
    }

    /// Atomically create the first user as admin if no users exist
    ///
    /// This method uses a transaction to check if any users exist and create
    /// the first user atomically, preventing race conditions where multiple
    /// simultaneous logins could all become admin.
    ///
    /// Returns:
    /// - Ok(Some(account)) - First user created successfully as admin
    /// - Ok(None) - Users already exist, did not create
    /// - Err(e) - Database error
    pub async fn create_first_user_if_none_exist(
        &self,
        username: &str,
        hashed_password: &str,
    ) -> Result<Option<UserAccount>, sqlx::Error> {
        // Start a transaction for atomicity
        let mut tx = self.pool.begin().await?;

        // Check if any users exist (within transaction)
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;

        if count.0 > 0 {
            // Users exist - rollback and return None
            tx.rollback().await?;
            return Ok(None);
        }

        // No users exist - create first user as admin
        let created_at = chrono::Utc::now().timestamp();

        let result = sqlx::query(SQL_INSERT_USER)
            .bind(username)
            .bind(hashed_password)
            .bind(true) // is_admin = true
            .bind(true) // enabled = true
            .bind(created_at)
            .execute(&mut *tx)
            .await?;

        let user_id = result.last_insert_rowid();

        // Commit the transaction
        tx.commit().await?;

        Ok(Some(UserAccount {
            id: user_id,
            username: username.to_string(),
            hashed_password: hashed_password.to_string(),
            is_admin: true,
            enabled: true,
            created_at,
        }))
    }

    /// Delete a user account
    /// Returns Ok(true) if user was deleted, Ok(false) if user didn't exist or deletion was blocked
    ///
    /// This operation is atomic and prevents deleting the last admin via a SQL constraint.
    /// If the target user is an admin and they are the last admin, the deletion will not occur.
    pub async fn delete_user(&self, user_id: i64) -> Result<bool, sqlx::Error> {
        // Atomic deletion: only delete if user is non-admin OR if they're not the last admin
        // This prevents race conditions when multiple admins try to delete each other simultaneously
        let result = sqlx::query(SQL_DELETE_USER_ATOMIC)
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Update a user account
    /// Returns Ok(true) if user was updated, Ok(false) if user didn't exist or update was blocked
    ///
    /// # Atomic Protection
    ///
    /// This method uses atomic SQL to prevent race conditions in two scenarios:
    ///
    /// 1. **Last Admin Demotion**: Prevents demoting the last admin (atomic SQL)
    /// 2. **Last Enabled Admin Disable**: Prevents disabling the last enabled admin (atomic SQL)
    ///
    /// The SQL UPDATE includes WHERE clauses with compound conditions:
    /// ```sql
    /// WHERE id = ?
    /// AND (
    ///     ? = 1                                -- Allow if enabling (final_enabled = true)
    ///     OR is_admin = 0                      -- Allow if target is not admin
    ///     OR (COUNT enabled admins) > 1        -- Allow if multiple enabled admins exist
    /// )
    /// AND (
    ///     ? = 1                                -- Allow if promoting (final_is_admin = true)
    ///     OR is_admin = 0                      -- Allow if target is currently not admin
    ///     OR (COUNT admins) > 1                -- Allow if multiple admins exist
    /// )
    /// ```
    ///
    /// This prevents TOCTOU (Time-Of-Check-To-Time-Of-Use) vulnerabilities where two admins
    /// could simultaneously disable or demote each other, leaving zero enabled/any admins.
    ///
    /// # Return Value
    ///
    /// - `Ok(true)`: Update succeeded
    /// - `Ok(false)`: Update blocked by protection or user not found
    ///
    /// # Note
    ///
    /// When `Ok(false)` is returned, the caller must distinguish between:
    /// - User not found
    /// - Last admin demotion attempt
    /// - Last enabled admin disable attempt
    /// - Duplicate username conflict
    pub async fn update_user(
        &self,
        username: &str,
        requested_username: Option<&str>,
        requested_password_hash: Option<&str>,
        requested_is_admin: Option<bool>,
        requested_enabled: Option<bool>,
        requested_permissions: Option<&Permissions>,
    ) -> Result<bool, sqlx::Error> {
        // First, get the user to update
        let user = match self.get_user_by_username(username).await? {
            Some(u) => u,
            None => return Ok(false),
        };

        // Check if new username already exists (and it's not the same user)
        if let Some(new_name) = requested_username
            && new_name != username
            && self.get_user_by_username(new_name).await?.is_some()
        {
            // Username already taken
            return Ok(false);
        }

        // Build the final values for each field
        let final_username = requested_username.unwrap_or(username);
        let final_password = requested_password_hash.unwrap_or(&user.hashed_password);
        let final_is_admin = requested_is_admin.unwrap_or(user.is_admin);
        let final_enabled = requested_enabled.unwrap_or(user.enabled);

        // Execute update with atomic last-admin protection
        // The SQL includes conditions to prevent:
        // 1. Disabling the last enabled admin
        // 2. Demoting the last admin
        let result = sqlx::query(SQL_UPDATE_USER)
            .bind(final_username)
            .bind(final_password)
            .bind(final_is_admin)
            .bind(final_enabled)
            .bind(user.id)
            .bind(final_enabled) // Final enabled status for the "enabling" check
            .bind(final_is_admin) // Final admin status for the "promoting" check
            .execute(&self.pool)
            .await?;

        // Check if the update was blocked (0 rows affected means constraints prevented update)
        if result.rows_affected() == 0 {
            // Update was blocked - could be last admin protection or user not found
            // Check if user still exists to distinguish between the cases
            if self.get_user_by_username(username).await?.is_some() {
                // User exists but update was blocked - must be last admin protection
                return Ok(false);
            }
            // User doesn't exist
            return Ok(false);
        }

        // Update permissions if provided
        if let Some(perms) = requested_permissions {
            // Only set permissions for non-admin users
            if !final_is_admin {
                self.set_permissions(user.id, perms).await?;
            } else {
                // Clear permissions for admin users (they get all automatically)
                sqlx::query(SQL_DELETE_PERMISSIONS)
                    .bind(user.id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::*;

    // ========================================================================
    // Database Operations Tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_user_by_username() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user
        let created = db
            .create_user("alice", "hash123", false, true, &Permissions::new())
            .await
            .unwrap();

        // Retrieve user
        let retrieved = db.get_user_by_username("alice").await.unwrap().unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.username, "alice");
        assert_eq!(retrieved.hashed_password, "hash123");
        assert!(!retrieved.is_admin);
    }

    #[tokio::test]
    async fn test_get_user_by_username_not_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let result = db.get_user_by_username("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_user_by_username_case_insensitive() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user with specific casing
        db.create_user("Alice", "hash123", false, true, &Permissions::new())
            .await
            .unwrap();

        // All case variations should match (case-insensitive lookup)
        let user1 = db.get_user_by_username("Alice").await.unwrap().unwrap();
        let user2 = db.get_user_by_username("alice").await.unwrap().unwrap();
        let user3 = db.get_user_by_username("ALICE").await.unwrap().unwrap();

        // All should return the same user
        assert_eq!(user1.id, user2.id);
        assert_eq!(user1.id, user3.id);

        // But the stored username should preserve original case
        assert_eq!(user1.username, "Alice");
        assert_eq!(user2.username, "Alice");
        assert_eq!(user3.username, "Alice");

        // Cannot create another user with different casing
        let result = db
            .create_user("alice", "hash456", false, true, &Permissions::new())
            .await;
        assert!(result.is_err()); // Should fail due to unique constraint
    }

    #[tokio::test]
    async fn test_get_user_by_id() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user
        let created = db
            .create_user("alice", "hash123", false, true, &Permissions::new())
            .await
            .unwrap();

        // Retrieve by ID
        let retrieved = db.get_user_by_id(created.id).await.unwrap().unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.username, "alice");
    }

    #[tokio::test]
    async fn test_get_user_by_id_not_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let result = db.get_user_by_id(99999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_create_user_with_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user with specific permissions
        use std::collections::HashSet;
        let mut perms = Permissions::new();
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::UserList);
            set.insert(Permission::ChatSend);
            set
        };

        let user = db
            .create_user("alice", "hash123", false, true, &perms)
            .await
            .unwrap();

        // Verify permissions were stored in database
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_permissions WHERE user_id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count, 2, "Should have 2 permissions stored");
    }

    #[tokio::test]
    async fn test_create_admin_does_not_store_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create admin with permissions object (should be ignored)
        let mut perms = Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::UserList);
            set
        };

        let admin = db
            .create_user("admin", "hash123", true, true, &perms)
            .await
            .unwrap();

        // Verify NO permissions stored in database (admin gets all automatically)
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_permissions WHERE user_id = ?")
                .bind(admin.id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count, 0, "Admin should have no stored permissions");
    }

    // ========================================================================
    // Permission Tests
    // ========================================================================

    #[tokio::test]
    async fn test_admin_has_all_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create admin (no permissions stored in DB)
        let admin = db
            .create_user("admin", "hash", true, true, &Permissions::new())
            .await
            .unwrap();

        // Admin should have all permissions
        assert!(
            db.has_permission(admin.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::UserInfo)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::ChatSend)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::ChatReceive)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::UserDelete)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_non_admin_respects_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create non-admin with specific permissions
        let mut perms = Permissions::new();
        // Access internal field for testing
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::UserList);
            set.insert(Permission::ChatReceive);
            set
        };

        let user = db
            .create_user("bob", "hash", false, true, &perms)
            .await
            .unwrap();

        // Should have granted permissions
        assert!(
            db.has_permission(user.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(user.id, Permission::ChatReceive)
                .await
                .unwrap()
        );

        // Should NOT have other permissions
        assert!(
            !db.has_permission(user.id, Permission::ChatSend)
                .await
                .unwrap()
        );
        assert!(
            !db.has_permission(user.id, Permission::UserDelete)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_has_permission_nonexistent_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Non-existent user should have no permissions
        let result = db
            .has_permission(99999, Permission::UserList)
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_set_permissions_replaces_existing() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user with initial permissions
        let mut initial_perms = Permissions::new();
        use std::collections::HashSet;
        initial_perms.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::UserList);
            set.insert(Permission::ChatSend);
            set
        };

        let user = db
            .create_user("alice", "hash", false, true, &initial_perms)
            .await
            .unwrap();

        // Verify initial permissions
        assert!(
            db.has_permission(user.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(user.id, Permission::ChatSend)
                .await
                .unwrap()
        );

        // Set new permissions (should replace, not merge)
        let mut new_perms = Permissions::new();
        new_perms.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::ChatReceive);
            set
        };

        db.set_permissions(user.id, &new_perms).await.unwrap();

        // Should have new permission
        assert!(
            db.has_permission(user.id, Permission::ChatReceive)
                .await
                .unwrap()
        );

        // Should NOT have old permissions
        assert!(
            !db.has_permission(user.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            !db.has_permission(user.id, Permission::ChatSend)
                .await
                .unwrap()
        );
    }

    // ========================================================================
    // User Deletion Tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_user_cascades_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create admin first
        db.create_user("admin", "hash0", true, true, &Permissions::new())
            .await
            .unwrap();

        // Create user with permissions
        let mut perms = Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::UserList);
            set.insert(Permission::ChatSend);
            set
        };

        let user = db
            .create_user("bob", "hash", false, true, &perms)
            .await
            .unwrap();

        // Verify permissions exist
        let (perm_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_permissions WHERE user_id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(perm_count, 2);

        // Delete user
        let deleted = db.delete_user(user.id).await.unwrap();
        assert!(deleted, "User should be deleted");

        // Permissions should be cascaded
        let (perm_count_after,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_permissions WHERE user_id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            perm_count_after, 0,
            "Permissions should be deleted via CASCADE"
        );
    }

    #[tokio::test]
    async fn test_delete_nonexistent_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Try to delete non-existent user
        let deleted = db.delete_user(99999).await.unwrap();
        assert!(!deleted, "Should return false for non-existent user");
    }

    #[tokio::test]
    async fn test_cannot_delete_last_admin() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create single admin
        let admin = db
            .create_user("admin", "hash", true, true, &Permissions::new())
            .await
            .unwrap();

        // Verify only one admin exists
        assert_eq!(count_admins(&pool).await, 1);

        // Try to delete the last admin
        let deleted = db.delete_user(admin.id).await.unwrap();
        assert!(!deleted, "Should not delete the last admin");

        // Admin should still exist
        assert!(db.get_user_by_id(admin.id).await.unwrap().is_some());
        assert_eq!(count_admins(&pool).await, 1);
    }

    #[tokio::test]
    async fn test_can_delete_admin_when_multiple_exist() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create two admins
        let admin1 = db
            .create_user("admin1", "hash1", true, true, &Permissions::new())
            .await
            .unwrap();
        let admin2 = db
            .create_user("admin2", "hash2", true, true, &Permissions::new())
            .await
            .unwrap();

        // Verify two admins exist
        assert_eq!(count_admins(&pool).await, 2);

        // Delete one admin (should succeed)
        let deleted = db.delete_user(admin1.id).await.unwrap();
        assert!(deleted, "Should delete admin when multiple exist");

        // Verify one admin remains
        assert_eq!(count_admins(&pool).await, 1);
        assert!(db.get_user_by_id(admin2.id).await.unwrap().is_some());
        assert!(db.get_user_by_id(admin1.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_can_delete_non_admin_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create admin (so system has an admin)
        db.create_user("admin", "hash0", true, true, &Permissions::new())
            .await
            .unwrap();

        // Create regular user
        let user = db
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Delete regular user (should succeed)
        let deleted = db.delete_user(user.id).await.unwrap();
        assert!(deleted, "Should delete non-admin user");

        // User should be gone
        assert!(db.get_user_by_id(user.id).await.unwrap().is_none());
    }

    // ========================================================================
    // Race Condition Tests
    // ========================================================================

    #[tokio::test]
    async fn test_concurrent_admin_deletion_race_condition() {
        let pool = create_test_db().await;
        let db1 = UserDb::new(pool.clone());
        let db2 = UserDb::new(pool.clone());

        // Create exactly 2 admins
        let admin1 = db1
            .create_user("admin1", "hash1", true, true, &Permissions::new())
            .await
            .unwrap();
        let admin2 = db1
            .create_user("admin2", "hash2", true, true, &Permissions::new())
            .await
            .unwrap();

        assert_eq!(count_admins(&pool).await, 2);

        // Try to delete both admins simultaneously
        let (result1, result2) =
            tokio::join!(db1.delete_user(admin2.id), db2.delete_user(admin1.id));

        let deleted1 = result1.unwrap();
        let deleted2 = result2.unwrap();

        // At least one deletion should fail (both can't succeed)
        assert!(
            !deleted1 || !deleted2,
            "Both admins should not be deleted! Race condition protection failed."
        );

        // At least one admin should still exist
        let remaining_admins = count_admins(&pool).await;
        assert!(
            remaining_admins >= 1,
            "No admins left! Race condition protection failed. Count: {}",
            remaining_admins
        );
    }

    #[tokio::test]
    async fn test_concurrent_permission_updates() {
        let pool = create_test_db().await;
        let db1 = UserDb::new(pool.clone());
        let db2 = UserDb::new(pool.clone());

        // Create admin first
        db1.create_user("admin", "hash0", true, true, &Permissions::new())
            .await
            .unwrap();

        // Create user
        let user = db1
            .create_user("bob", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Set different permissions concurrently
        use std::collections::HashSet;
        let mut perms1 = Permissions::new();
        perms1.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::UserList);
            set
        };

        let mut perms2 = Permissions::new();
        perms2.permissions = {
            let mut set = HashSet::new();
            set.insert(Permission::ChatSend);
            set
        };

        let (result1, result2) = tokio::join!(
            db1.set_permissions(user.id, &perms1),
            db2.set_permissions(user.id, &perms2)
        );

        // Both operations should succeed
        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // User should have one of the permission sets (last write wins)
        let has_userlist = db1
            .has_permission(user.id, Permission::UserList)
            .await
            .unwrap();
        let has_chatsend = db1
            .has_permission(user.id, Permission::ChatSend)
            .await
            .unwrap();

        // Should have one or the other (XOR), but not both
        assert!(
            has_userlist ^ has_chatsend,
            "User should have one permission set, not both or neither"
        );
    }
}

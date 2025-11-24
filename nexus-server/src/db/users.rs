//! User account database operations

use super::permissions::{Permission, Permissions};
use sqlx::SqlitePool;

/// User account stored in database
#[derive(Debug, Clone)]
pub struct UserAccount {
    pub id: i64,
    pub username: String,
    pub hashed_password: String,
    pub is_admin: bool,
    pub created_at: i64,
}

/// Database operations for user accounts
#[derive(Clone)]
pub struct UserDb {
    pool: SqlitePool,
}

impl UserDb {
    /// Create a new UserDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Check if any users exist in the database
    pub async fn has_any_users(&self) -> Result<bool, sqlx::Error> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0 > 0)
    }

    /// Get a user by username
    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserAccount>, sqlx::Error> {
        let user: Option<(i64, String, String, bool, i64)> = sqlx::query_as(
            "SELECT id, username, password_hash, is_admin, created_at FROM users WHERE username = ?"
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user.map(
            |(id, username, hashed_password, is_admin, created_at)| UserAccount {
                id,
                username,
                hashed_password,
                is_admin,
                created_at,
            },
        ))
    }

    /// Check if user has a specific permission (with admin override)
    pub async fn has_permission(
        &self,
        user_id: i64,
        permission: Permission,
    ) -> Result<bool, sqlx::Error> {
        // Check if user is admin (admins have all permissions)
        let is_admin: Option<(bool,)> = sqlx::query_as("SELECT is_admin FROM users WHERE id = ?")
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

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_permissions WHERE user_id = ? AND permission = ?",
        )
        .bind(user_id)
        .bind(permission.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0 > 0)
    }

    /// Get all permissions for a user
    pub async fn get_user_permissions(
        &self,
        user_id: i64,
    ) -> Result<Permissions, sqlx::Error> {
        let perm_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT permission FROM user_permissions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut permissions = Permissions::new();
        for (perm_str,) in perm_rows {
            if let Some(perm) = Permission::from_str(&perm_str) {
                permissions.permissions.insert(perm);
            }
        }

        Ok(permissions)
    }

    /// Set permissions for a user (replaces all existing permissions)
    pub async fn set_permissions(
        &self,
        user_id: i64,
        permissions: &Permissions,
    ) -> Result<(), sqlx::Error> {
        // Delete existing permissions
        sqlx::query("DELETE FROM user_permissions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        // Insert new permissions
        for perm in permissions.to_vec() {
            sqlx::query("INSERT INTO user_permissions (user_id, permission) VALUES (?, ?)")
                .bind(user_id)
                .bind(perm.as_str())
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    /// Create a new user account with permissions
    pub async fn create_user(
        &self,
        username: &str,
        hashed_password: &str,
        is_admin: bool,
        permissions: &Permissions,
    ) -> Result<UserAccount, sqlx::Error> {
        let created_at = chrono::Utc::now().timestamp();

        let result = sqlx::query(
            "INSERT INTO users (username, password_hash, is_admin, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(username)
        .bind(hashed_password)
        .bind(is_admin)
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
            created_at,
        })
    }

    /// Get a user by ID
    pub async fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserAccount>, sqlx::Error> {
        let user: Option<(i64, String, String, bool, i64)> = sqlx::query_as(
            "SELECT id, username, password_hash, is_admin, created_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user.map(
            |(id, username, hashed_password, is_admin, created_at)| UserAccount {
                id,
                username,
                hashed_password,
                is_admin,
                created_at,
            },
        ))
    }

    /// Delete a user account
    /// Returns Ok(true) if user was deleted, Ok(false) if user didn't exist or deletion was blocked
    ///
    /// This operation is atomic and prevents deleting the last admin via a SQL constraint.
    /// If the target user is an admin and they are the last admin, the deletion will not occur.
    pub async fn delete_user(&self, user_id: i64) -> Result<bool, sqlx::Error> {
        // Atomic deletion: only delete if user is non-admin OR if they're not the last admin
        // This prevents race conditions when multiple admins try to delete each other simultaneously
        let result = sqlx::query(
            "DELETE FROM users
             WHERE id = ?
             AND (
                 is_admin = 0
                 OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
             )",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an in-memory test database with migrations
    async fn create_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory database");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    /// Count the number of admin users in the database
    async fn count_admins(pool: &SqlitePool) -> i64 {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE is_admin = 1")
            .fetch_one(pool)
            .await
            .unwrap();
        count
    }

    // ========================================================================
    // Database Operations Tests
    // ========================================================================

    #[tokio::test]
    async fn test_has_any_users() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Initially no users
        assert!(!db.has_any_users().await.unwrap());

        // Create a user
        db.create_user("alice", "hash", false, &Permissions::new())
            .await
            .unwrap();

        // Now should have users
        assert!(db.has_any_users().await.unwrap());
    }

    #[tokio::test]
    async fn test_get_user_by_username() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user
        let created = db
            .create_user("alice", "hash123", false, &Permissions::new())
            .await
            .unwrap();

        // Retrieve user
        let retrieved = db.get_user_by_username("alice").await.unwrap().unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.username, "alice");
        assert_eq!(retrieved.hashed_password, "hash123");
        assert_eq!(retrieved.is_admin, false);
    }

    #[tokio::test]
    async fn test_get_user_by_username_not_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let result = db.get_user_by_username("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_user_by_username_case_sensitive() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user with specific casing
        db.create_user("Alice", "hash123", false, &Permissions::new())
            .await
            .unwrap();

        // Exact match should work
        assert!(db.get_user_by_username("Alice").await.unwrap().is_some());

        // Different case should not match (case-sensitive)
        assert!(db.get_user_by_username("alice").await.unwrap().is_none());
        assert!(db.get_user_by_username("ALICE").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_user_by_id() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create user
        let created = db
            .create_user("alice", "hash123", false, &Permissions::new())
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
            .create_user("alice", "hash123", false, &perms)
            .await
            .unwrap();

        // Verify permissions were stored in database
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_permissions WHERE user_id = ?",
        )
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
            .create_user("admin", "hash123", true, &perms)
            .await
            .unwrap();

        // Verify NO permissions stored in database (admin gets all automatically)
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_permissions WHERE user_id = ?",
        )
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
            .create_user("admin", "hash", true, &Permissions::new())
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
            .create_user("bob", "hash", false, &perms)
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
        let result = db.has_permission(99999, Permission::UserList).await.unwrap();
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
            .create_user("alice", "hash", false, &initial_perms)
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
        db.create_user("admin", "hash0", true, &Permissions::new())
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
            .create_user("bob", "hash", false, &perms)
            .await
            .unwrap();

        // Verify permissions exist
        let (perm_count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_permissions WHERE user_id = ?",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(perm_count, 2);

        // Delete user
        let deleted = db.delete_user(user.id).await.unwrap();
        assert!(deleted, "User should be deleted");

        // Permissions should be cascaded
        let (perm_count_after,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM user_permissions WHERE user_id = ?",
        )
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(perm_count_after, 0, "Permissions should be deleted via CASCADE");
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
            .create_user("admin", "hash", true, &Permissions::new())
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
            .create_user("admin1", "hash1", true, &Permissions::new())
            .await
            .unwrap();
        let admin2 = db
            .create_user("admin2", "hash2", true, &Permissions::new())
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
        db.create_user("admin", "hash0", true, &Permissions::new())
            .await
            .unwrap();

        // Create regular user
        let user = db
            .create_user("bob", "hash", false, &Permissions::new())
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
            .create_user("admin1", "hash1", true, &Permissions::new())
            .await
            .unwrap();
        let admin2 = db1
            .create_user("admin2", "hash2", true, &Permissions::new())
            .await
            .unwrap();

        assert_eq!(count_admins(&pool).await, 2);

        // Try to delete both admins simultaneously
        let (result1, result2) = tokio::join!(
            db1.delete_user(admin2.id),
            db2.delete_user(admin1.id)
        );

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
        db1.create_user("admin", "hash0", true, &Permissions::new())
            .await
            .unwrap();

        // Create user
        let user = db1
            .create_user("bob", "hash", false, &Permissions::new())
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

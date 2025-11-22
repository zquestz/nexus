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

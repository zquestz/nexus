//! User account database operations

use sqlx::SqlitePool;
use std::collections::HashSet;

/// Permission types for user actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Permission to list connected users
    ListUsers,
    /// Permission to send chat messages
    SendChat,
    /// Permission to receive chat messages
    ReceiveChat,
}

impl Permission {
    /// Convert permission to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::ListUsers => "list_users",
            Permission::SendChat => "send_chat",
            Permission::ReceiveChat => "receive_chat",
        }
    }

    /// Parse permission from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "list_users" => Some(Permission::ListUsers),
            "send_chat" => Some(Permission::SendChat),
            "receive_chat" => Some(Permission::ReceiveChat),
            _ => None,
        }
    }

    /// Get all available permissions
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::ListUsers,
            Permission::SendChat,
            Permission::ReceiveChat,
        ]
    }
}

/// Set of permissions for a user
#[derive(Debug, Clone)]
pub struct Permissions {
    permissions: HashSet<Permission>,
}

impl Permissions {
    /// Create a new empty permission set
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    /// Create a permission set with all permissions (for admins)
    pub fn all() -> Self {
        Self {
            permissions: Permission::all().into_iter().collect(),
        }
    }

    /// Create permissions from a list
    pub fn from_vec(perms: Vec<Permission>) -> Self {
        Self {
            permissions: perms.into_iter().collect(),
        }
    }

    /// Check if user has a specific permission
    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// Add a permission
    pub fn add(&mut self, permission: Permission) {
        self.permissions.insert(permission);
    }

    /// Remove a permission
    pub fn remove(&mut self, permission: Permission) {
        self.permissions.remove(&permission);
    }

    /// Get all permissions as a vec
    pub fn to_vec(&self) -> Vec<Permission> {
        self.permissions.iter().copied().collect()
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::new()
    }
}

/// User account stored in database
#[derive(Debug, Clone)]
pub struct UserAccount {
    pub id: i64,
    pub username: String,
    pub hashed_password: String,
    pub is_admin: bool,
    pub created_at: i64,
}

impl UserAccount {
    /// Check if user has a specific permission
    /// Note: This only checks the is_admin flag. Use UserDb::has_permission for full permission checking.
    pub fn is_admin(&self) -> bool {
        self.is_admin
    }
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

    /// Get permissions for a user
    pub async fn get_permissions(&self, user_id: i64) -> Result<Permissions, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT permission FROM user_permissions WHERE user_id = ?"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let perms: Vec<Permission> = rows
            .into_iter()
            .filter_map(|(perm_str,)| Permission::from_str(&perm_str))
            .collect();

        Ok(Permissions::from_vec(perms))
    }

    /// Check if user has a specific permission (with admin override)
    pub async fn has_permission(&self, user_id: i64, permission: Permission) -> Result<bool, sqlx::Error> {
        // Check if user is admin (admins have all permissions)
        let is_admin: Option<(bool,)> = sqlx::query_as(
            "SELECT is_admin FROM users WHERE id = ?"
        )
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
            "SELECT COUNT(*) FROM user_permissions WHERE user_id = ? AND permission = ?"
        )
        .bind(user_id)
        .bind(permission.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0 > 0)
    }

    /// Set permissions for a user (replaces all existing permissions)
    pub async fn set_permissions(&self, user_id: i64, permissions: &Permissions) -> Result<(), sqlx::Error> {
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

        // Set permissions in separate table
        self.set_permissions(user_id, permissions).await?;

        Ok(UserAccount {
            id: user_id,
            username: username.to_string(),
            hashed_password: hashed_password.to_string(),
            is_admin,
            created_at,
        })
    }
}
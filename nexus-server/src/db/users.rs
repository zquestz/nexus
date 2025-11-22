//! User account database operations

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

    /// Create a new user account
    pub async fn create_user(
        &self,
        username: &str,
        hashed_password: &str,
        is_admin: bool,
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

        Ok(UserAccount {
            id: result.last_insert_rowid(),
            username: username.to_string(),
            hashed_password: hashed_password.to_string(),
            is_admin,
            created_at,
        })
    }
}

//! Database module for persistent storage

pub mod chat;
pub mod config;
pub mod news;
pub mod password;
pub mod permissions;
pub mod sql;
pub mod users;

#[cfg(test)]
pub mod testing;

pub use chat::ChatDb;
pub use config::ConfigDb;
pub use news::NewsDb;
pub use password::{hash_password, verify_password};
pub use permissions::{Permission, Permissions};
pub use users::UserDb;

use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use crate::constants::*;

/// Combined database access for all database operations
#[derive(Clone)]
pub struct Database {
    pub users: UserDb,
    pub config: ConfigDb,
    pub chat: ChatDb,
    pub news: NewsDb,
}

impl Database {
    /// Create a new Database instance from a connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            users: UserDb::new(pool.clone()),
            config: ConfigDb::new(pool.clone()),
            chat: ChatDb::new(pool.clone()),
            news: NewsDb::new(pool),
        }
    }
}

/// Get the default database path for the platform
///
/// Returns the platform-specific path where the database file should be stored:
/// - **Linux**: `~/.local/share/nexusd/nexus.db`
/// - **macOS**: `~/Library/Application Support/nexusd/nexus.db`
/// - **Windows**: `%APPDATA%\nexusd\nexus.db`
///
/// # Errors
///
/// Returns an error if the platform's data directory cannot be determined.
/// This is rare but can happen on unsupported or misconfigured systems.
pub fn default_database_path() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir().ok_or_else(|| ERR_NO_DATA_DIR.to_string())?;
    Ok(data_dir.join(DATA_DIR_NAME).join(DATABASE_FILENAME))
}

/// Initialize the database connection pool and run migrations
pub async fn init_db(database_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    // Create parent directories if they don't exist
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            eprintln!("{}{}", ERR_CREATE_DB_DIR, e);
            sqlx::Error::Io(e)
        })?;
    }

    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());

    // Create connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_DB_CONNECTIONS)
        .connect(&database_url)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

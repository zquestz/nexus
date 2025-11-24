//! Database module for persistent storage

pub mod password;
pub mod permissions;
pub mod users;

#[cfg(test)]
pub mod testing;

pub use password::{hash_password, verify_password};
pub use permissions::{Permission, Permissions};
pub use users::UserDb;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::{Path, PathBuf};

/// Maximum number of concurrent database connections in the pool
const MAX_DB_CONNECTIONS: u32 = 5;

/// Get the default database path for the platform
pub fn default_database_path() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| "Unable to determine data directory for your platform".to_string())?;
    Ok(data_dir.join("nexusd").join("nexus.db"))
}

/// Initialize the database connection pool and run migrations
pub async fn init_db(database_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    // Create parent directories if they don't exist
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            eprintln!("Failed to create directory: {}", e);
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

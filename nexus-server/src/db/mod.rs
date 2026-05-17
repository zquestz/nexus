//! Database module for persistent storage

pub mod bans;
pub mod channels;
pub mod config;
pub mod groups;
pub mod news;
pub mod password;
pub mod permissions;
pub mod sql;
pub mod trackers;
pub mod trusts;
pub mod users;
mod util;

#[cfg(test)]
pub mod testing;

pub use bans::BanDb;
pub use channels::ChannelDb;
pub use config::ConfigDb;
pub use groups::{GroupDb, UpdateGroupResult};
pub use news::NewsDb;
pub use password::{hash_password_async, verify_password_async};
// Sync helpers retained for tests (cheap fast-hash path) and the cached test
// hash helper. Production code in async contexts must use the `_async` variants.
#[allow(unused_imports)]
pub use password::{hash_password, verify_password};
pub use permissions::{Permission, Permissions};
// Tracker re-exports are dead until chunk 4 (handlers consume them).
#[allow(unused_imports)]
pub use trackers::{
    CreateTrackerParams, TrackerDb, TrackerDbError, TrackerRecord, UpdateTrackerParams,
    is_transient_db_error,
};
pub use trusts::TrustDb;
pub use users::{CreateUserParams, UpdateUserParams, UpdateUserResult, UserDb};

use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use crate::constants::*;

/// Combined database access for all database operations
#[derive(Clone)]
pub struct Database {
    pub users: UserDb,
    pub config: ConfigDb,
    pub news: NewsDb,
    pub bans: BanDb,
    pub trusts: TrustDb,
    pub channels: ChannelDb,
    pub groups: GroupDb,
    pub trackers: TrackerDb,
}

impl Database {
    /// Create a new Database instance from a connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            users: UserDb::new(pool.clone()),
            config: ConfigDb::new(pool.clone()),
            news: NewsDb::new(pool.clone()),
            bans: BanDb::new(pool.clone()),
            trusts: TrustDb::new(pool.clone()),
            channels: ChannelDb::new(pool.clone()),
            groups: GroupDb::new(pool.clone()),
            trackers: TrackerDb::new(pool),
        }
    }

    /// Bundle the per-table fields the tracker task needs to build
    /// `TrackerServerRegister` payloads on each refresh.
    ///
    /// Composes [`ConfigDb::get_tracker_fields`] (server name, description,
    /// public address — sourced from the `config` table) with
    /// [`UserDb::guest_enabled`] (sourced from the `users` table). Two
    /// queries internally; one method to call from the tracker task.
    ///
    /// # Propagation contract
    ///
    /// This per-refresh DB read is the **only** path by which an
    /// admin's `ServerInfoUpdate` (changes to `server_name`,
    /// `description`, `public_address`, or guest-enabled state)
    /// reaches a tracker entry. The tracker task must NOT cache the
    /// result anywhere — caller-side, in `TrackerContext`, or in
    /// the manager — or admin updates will silently fail to propagate
    /// until the tracker task is restarted. See
    /// `handlers/server_info_update.rs` for the write half.
    ///
    /// # Errors
    ///
    /// Returns `sqlx::Error` if either underlying query fails.
    pub async fn tracker_registration_fields(
        &self,
    ) -> Result<TrackerRegistrationFields, sqlx::Error> {
        let config = self.config.get_tracker_fields().await?;
        let allows_guest = self.users.guest_enabled().await?;
        Ok(TrackerRegistrationFields {
            server_name: config.server_name,
            description: config.description,
            public_address: config.public_address,
            allows_guest,
        })
    }
}

/// Fields the tracker task needs to populate `TrackerServerRegister`
/// payloads. Bundled from [`ConfigDb::get_tracker_fields`] and
/// [`UserDb::guest_enabled`] via [`Database::tracker_registration_fields`].
pub struct TrackerRegistrationFields {
    pub server_name: String,
    pub description: Option<String>,
    pub public_address: Option<String>,
    pub allows_guest: bool,
}

/// Get the database file path under the given server data directory
/// (`<data_dir>/nexus.db`).
pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DATABASE_FILENAME)
}

/// Initialize the database connection pool and run migrations.
///
/// The parent directory of `database_path` must already exist; the caller
/// is responsible for ensuring the data directory is created (typically
/// via `ensure_data_dir` in `main.rs`).
pub async fn init_db(database_path: &Path) -> Result<SqlitePool, sqlx::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_path_under_data_dir() {
        let data = Path::new("/var/lib/nexusd");
        assert_eq!(database_path(data), Path::new("/var/lib/nexusd/nexus.db"));
    }
}

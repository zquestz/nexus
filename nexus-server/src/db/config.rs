//! Server configuration database operations

use super::sql::SQL_GET_CONFIG;
use crate::constants::{
    CONFIG_KEY_MAX_CONNECTIONS_PER_IP, CONFIG_KEY_SERVER_DESCRIPTION, CONFIG_KEY_SERVER_NAME,
};
use sqlx::SqlitePool;
use std::io;

/// Database interface for server configuration
#[derive(Clone)]
pub struct ConfigDb {
    pool: SqlitePool,
}

impl ConfigDb {
    /// Create a new ConfigDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get the maximum connections allowed per IP address
    pub async fn get_max_connections_per_ip(&self) -> usize {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
            .fetch_one(&self.pool)
            .await
            .expect("max_connections_per_ip config missing")
            .parse()
            .expect("max_connections_per_ip config invalid")
    }

    /// Get the server name
    pub async fn get_server_name(&self) -> io::Result<String> {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_SERVER_NAME)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))
    }

    /// Get the server description
    pub async fn get_server_description(&self) -> io::Result<String> {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_SERVER_DESCRIPTION)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;

    #[tokio::test]
    async fn test_get_max_connections_per_ip_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to 5
        let limit = config_db.get_max_connections_per_ip().await;
        assert_eq!(limit, 5);
    }

    #[tokio::test]
    async fn test_get_server_name_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to "Nexus BBS"
        let name = config_db.get_server_name().await.unwrap();
        assert_eq!(name, "Nexus BBS");
    }

    #[tokio::test]
    async fn test_get_server_description_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to empty string
        let description = config_db.get_server_description().await.unwrap();
        assert_eq!(description, "");
    }
}

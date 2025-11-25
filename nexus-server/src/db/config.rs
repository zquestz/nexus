//! Server configuration database operations

use sqlx::SqlitePool;
use std::io;

/// SQL query constants
const SQL_GET_CONFIG: &str = "SELECT value FROM server_config WHERE key = ?";
const SQL_SET_CONFIG: &str = "INSERT OR REPLACE INTO server_config (key, value) VALUES (?, ?)";

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

    /// Get the current server topic
    pub async fn get_topic(&self) -> io::Result<String> {
        let result = sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind("topic")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(result)
    }

    /// Set the server topic
    pub async fn set_topic(&self, topic: &str) -> io::Result<()> {
        sqlx::query(SQL_SET_CONFIG)
            .bind("topic")
            .bind(topic)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;

    #[tokio::test]
    async fn test_get_topic_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let topic = config_db.get_topic().await.unwrap();
        assert_eq!(topic, "");
    }

    #[tokio::test]
    async fn test_set_and_get_topic() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let new_topic = "Server maintenance tonight at 10pm";
        config_db.set_topic(new_topic).await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved, new_topic);
    }

    #[tokio::test]
    async fn test_set_topic_overwrites_previous() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_topic("First topic").await.unwrap();
        config_db.set_topic("Second topic").await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved, "Second topic");
    }

    #[tokio::test]
    async fn test_set_empty_topic() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_topic("").await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved, "");
    }
}

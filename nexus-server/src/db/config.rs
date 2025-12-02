//! Server configuration database operations

use super::sql::*;
use crate::constants::*;
use sqlx::SqlitePool;
use std::io;

/// Chat topic with the username who set it
#[derive(Debug, Clone, Default)]
pub struct ChatTopic {
    /// The topic text (empty string if no topic)
    pub topic: String,
    /// Username who set the topic (empty string if never set or cleared)
    pub set_by: String,
}

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

    /// Get the current server topic with the username who set it
    pub async fn get_topic(&self) -> io::Result<ChatTopic> {
        let topic = sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_TOPIC)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        let set_by = sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_TOPIC_SET_BY)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(ChatTopic { topic, set_by })
    }

    /// Set the server topic and record who set it
    pub async fn set_topic(&self, topic: &str, set_by: &str) -> io::Result<()> {
        sqlx::query(SQL_SET_CONFIG)
            .bind(CONFIG_KEY_TOPIC)
            .bind(topic)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        sqlx::query(SQL_SET_CONFIG)
            .bind(CONFIG_KEY_TOPIC_SET_BY)
            .bind(set_by)
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

        let chat_topic = config_db.get_topic().await.unwrap();
        assert_eq!(chat_topic.topic, "");
        assert_eq!(chat_topic.set_by, "");
    }

    #[tokio::test]
    async fn test_set_and_get_topic() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let new_topic = "Server maintenance tonight at 10pm";
        config_db.set_topic(new_topic, "admin").await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, new_topic);
        assert_eq!(retrieved.set_by, "admin");
    }

    #[tokio::test]
    async fn test_set_topic_overwrites_previous() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_topic("First topic", "alice").await.unwrap();
        config_db.set_topic("Second topic", "bob").await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, "Second topic");
        assert_eq!(retrieved.set_by, "bob");
    }

    #[tokio::test]
    async fn test_clear_topic() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_topic("Some topic", "alice")
            .await
            .unwrap();
        config_db.set_topic("", "bob").await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, "");
        assert_eq!(retrieved.set_by, "bob");
    }
}
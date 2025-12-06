//! Server configuration database operations

use super::sql::*;
use crate::constants::{
    CONFIG_KEY_MAX_CONNECTIONS_PER_IP, CONFIG_KEY_SERVER_DESCRIPTION, CONFIG_KEY_SERVER_NAME,
    CONFIG_KEY_TOPIC, CONFIG_KEY_TOPIC_SET_BY,
};
use nexus_common::validators;
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
        // Validate topic format (failsafe - handlers should also validate)
        // If this fails, it indicates a bug or attack bypassing handler validation
        if let Err(e) = validators::validate_chat_topic(topic) {
            return Err(io::Error::other(format!("{:?}", e)));
        }

        // Validate username format (failsafe - handlers should also validate)
        if let Err(e) = validators::validate_username(set_by) {
            return Err(io::Error::other(format!("{:?}", e)));
        }

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

        config_db.set_topic("Some topic", "alice").await.unwrap();
        config_db.set_topic("", "bob").await.unwrap();

        let retrieved = config_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, "");
        assert_eq!(retrieved.set_by, "bob");
    }

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

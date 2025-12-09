//! Chat state database operations

use super::sql::{SQL_GET_CHAT_STATE, SQL_SET_CHAT_STATE};
use crate::constants::{CHAT_STATE_KEY_TOPIC, CHAT_STATE_KEY_TOPIC_SET_BY};
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

/// Database interface for chat state
#[derive(Clone)]
pub struct ChatDb {
    pool: SqlitePool,
}

impl ChatDb {
    /// Create a new ChatDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get the current chat topic with the username who set it
    pub async fn get_topic(&self) -> io::Result<ChatTopic> {
        let topic = sqlx::query_scalar::<_, String>(SQL_GET_CHAT_STATE)
            .bind(CHAT_STATE_KEY_TOPIC)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        let set_by = sqlx::query_scalar::<_, String>(SQL_GET_CHAT_STATE)
            .bind(CHAT_STATE_KEY_TOPIC_SET_BY)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(ChatTopic { topic, set_by })
    }

    /// Set the chat topic and record who set it
    pub async fn set_topic(&self, topic: &str, set_by: &str) -> io::Result<()> {
        // Validate topic format (failsafe - handlers should also validate)
        // If this fails, it indicates a bug or attack bypassing handler validation
        if let Err(e) = validators::validate_chat_topic(topic) {
            return Err(io::Error::other(format!("{e:?}")));
        }

        // Validate username format (failsafe - handlers should also validate)
        if let Err(e) = validators::validate_username(set_by) {
            return Err(io::Error::other(format!("{e:?}")));
        }

        sqlx::query(SQL_SET_CHAT_STATE)
            .bind(CHAT_STATE_KEY_TOPIC)
            .bind(topic)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        sqlx::query(SQL_SET_CHAT_STATE)
            .bind(CHAT_STATE_KEY_TOPIC_SET_BY)
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
        let chat_db = ChatDb::new(pool);

        let chat_topic = chat_db.get_topic().await.unwrap();
        assert_eq!(chat_topic.topic, "");
        assert_eq!(chat_topic.set_by, "");
    }

    #[tokio::test]
    async fn test_set_and_get_topic() {
        let pool = create_test_db().await;
        let chat_db = ChatDb::new(pool);

        let new_topic = "Server maintenance tonight at 10pm";
        chat_db.set_topic(new_topic, "admin").await.unwrap();

        let retrieved = chat_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, new_topic);
        assert_eq!(retrieved.set_by, "admin");
    }

    #[tokio::test]
    async fn test_set_topic_overwrites_previous() {
        let pool = create_test_db().await;
        let chat_db = ChatDb::new(pool);

        chat_db.set_topic("First topic", "alice").await.unwrap();
        chat_db.set_topic("Second topic", "bob").await.unwrap();

        let retrieved = chat_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, "Second topic");
        assert_eq!(retrieved.set_by, "bob");
    }

    #[tokio::test]
    async fn test_clear_topic() {
        let pool = create_test_db().await;
        let chat_db = ChatDb::new(pool);

        chat_db.set_topic("Some topic", "alice").await.unwrap();
        chat_db.set_topic("", "bob").await.unwrap();

        let retrieved = chat_db.get_topic().await.unwrap();
        assert_eq!(retrieved.topic, "");
        assert_eq!(retrieved.set_by, "bob");
    }
}

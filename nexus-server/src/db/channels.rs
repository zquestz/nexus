//! Channel settings database operations
//!
//! Handles persistence of channel settings for persistent channels.
//! Ephemeral channels do not have their settings stored in the database.

use std::io;

use sqlx::SqlitePool;

use crate::db::sql;

/// Channel settings from database
#[derive(Debug, Clone)]
pub struct ChannelSettings {
    pub name: String,
    pub topic: String,
    pub topic_set_by: String,
    pub secret: bool,
}

/// Database interface for channel settings
#[derive(Clone)]
pub struct ChannelDb {
    pool: SqlitePool,
}

impl ChannelDb {
    /// Create a new ChannelDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get settings for a channel
    ///
    /// Returns None if the channel doesn't exist in the database.
    pub async fn get_channel_settings(&self, name: &str) -> io::Result<Option<ChannelSettings>> {
        let result =
            sqlx::query_as::<_, (String, String, String, bool)>(sql::SQL_SELECT_CHANNEL_SETTINGS)
                .bind(name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(
            result.map(|(name, topic, topic_set_by, secret)| ChannelSettings {
                name,
                topic,
                topic_set_by,
                secret,
            }),
        )
    }

    /// Get all channel settings
    ///
    /// Returns settings for all persistent channels.
    pub async fn get_all_channel_settings(&self) -> io::Result<Vec<ChannelSettings>> {
        let results = sqlx::query_as::<_, (String, String, String, bool)>(
            sql::SQL_SELECT_ALL_CHANNEL_SETTINGS,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|(name, topic, topic_set_by, secret)| ChannelSettings {
                name,
                topic,
                topic_set_by,
                secret,
            })
            .collect())
    }

    /// Create or update channel settings
    ///
    /// Uses upsert semantics - creates if doesn't exist, updates if it does.
    pub async fn upsert_channel_settings(&self, settings: &ChannelSettings) -> io::Result<()> {
        sqlx::query(sql::SQL_UPSERT_CHANNEL_SETTINGS)
            .bind(&settings.name)
            .bind(&settings.topic)
            .bind(&settings.topic_set_by)
            .bind(settings.secret)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Update only the topic for a channel
    pub async fn set_topic(&self, name: &str, topic: &str, set_by: &str) -> io::Result<()> {
        sqlx::query(sql::SQL_UPDATE_CHANNEL_TOPIC)
            .bind(topic)
            .bind(set_by)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Update only the secret flag for a channel
    pub async fn set_secret(&self, name: &str, secret: bool) -> io::Result<()> {
        sqlx::query(sql::SQL_UPDATE_CHANNEL_SECRET)
            .bind(secret)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Delete channel settings
    ///
    /// Used when a channel is removed from the persistent channels list.
    pub async fn delete_channel_settings(&self, name: &str) -> io::Result<()> {
        sqlx::query(sql::SQL_DELETE_CHANNEL_SETTINGS)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Check if a channel has settings in the database
    #[cfg(test)]
    pub async fn channel_exists(&self, name: &str) -> io::Result<bool> {
        let count: i32 = sqlx::query_scalar(sql::SQL_COUNT_CHANNEL_SETTINGS)
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;
    use nexus_common::validators::DEFAULT_CHANNEL;

    #[tokio::test]
    async fn test_get_channel_settings_not_found() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        let result = db.get_channel_settings("#nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_channel_settings_default_channel() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        let result = db.get_channel_settings(DEFAULT_CHANNEL).await.unwrap();
        assert!(result.is_some());
        let settings = result.unwrap();
        assert_eq!(settings.name, DEFAULT_CHANNEL);
        assert_eq!(settings.topic, "");
        assert_eq!(settings.topic_set_by, "");
        assert!(!settings.secret);
    }

    #[tokio::test]
    async fn test_get_channel_settings_case_insensitive() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        // Use uppercase version of default channel
        let result = db
            .get_channel_settings(&DEFAULT_CHANNEL.to_uppercase())
            .await
            .unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_upsert_channel_settings_create() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        let settings = ChannelSettings {
            name: "#general".to_string(),
            topic: "General chat".to_string(),
            topic_set_by: "admin".to_string(),
            secret: false,
        };

        db.upsert_channel_settings(&settings).await.unwrap();

        let result = db.get_channel_settings("#general").await.unwrap().unwrap();
        assert_eq!(result.name, "#general");
        assert_eq!(result.topic, "General chat");
        assert_eq!(result.topic_set_by, "admin");
        assert!(!result.secret);
    }

    #[tokio::test]
    async fn test_upsert_channel_settings_update() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        // Update the default #nexus channel
        let settings = ChannelSettings {
            name: DEFAULT_CHANNEL.to_string(),
            topic: "Welcome to Nexus!".to_string(),
            topic_set_by: "admin".to_string(),
            secret: true,
        };

        db.upsert_channel_settings(&settings).await.unwrap();

        let result = db
            .get_channel_settings(DEFAULT_CHANNEL)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.topic, "Welcome to Nexus!");
        assert_eq!(result.topic_set_by, "admin");
        assert!(result.secret);
    }

    #[tokio::test]
    async fn test_set_topic() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        db.set_topic(DEFAULT_CHANNEL, "New topic", "moderator")
            .await
            .unwrap();

        let result = db
            .get_channel_settings(DEFAULT_CHANNEL)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.topic, "New topic");
        assert_eq!(result.topic_set_by, "moderator");
    }

    #[tokio::test]
    async fn test_set_secret() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        db.set_secret(DEFAULT_CHANNEL, true).await.unwrap();

        let result = db
            .get_channel_settings(DEFAULT_CHANNEL)
            .await
            .unwrap()
            .unwrap();
        assert!(result.secret);

        db.set_secret(DEFAULT_CHANNEL, false).await.unwrap();

        let result = db
            .get_channel_settings(DEFAULT_CHANNEL)
            .await
            .unwrap()
            .unwrap();
        assert!(!result.secret);
    }

    #[tokio::test]
    async fn test_delete_channel_settings() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        // Create a channel first
        let settings = ChannelSettings {
            name: "#deleteme".to_string(),
            topic: "".to_string(),
            topic_set_by: "".to_string(),
            secret: false,
        };
        db.upsert_channel_settings(&settings).await.unwrap();

        // Verify it exists
        assert!(db.channel_exists("#deleteme").await.unwrap());

        // Delete it
        db.delete_channel_settings("#deleteme").await.unwrap();

        // Verify it's gone
        assert!(!db.channel_exists("#deleteme").await.unwrap());
    }

    #[tokio::test]
    async fn test_get_all_channel_settings() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        // Add another channel
        let settings = ChannelSettings {
            name: "#general".to_string(),
            topic: "General".to_string(),
            topic_set_by: "admin".to_string(),
            secret: false,
        };
        db.upsert_channel_settings(&settings).await.unwrap();

        let all = db.get_all_channel_settings().await.unwrap();
        assert_eq!(all.len(), 2); // #nexus (from migration) + #general

        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&DEFAULT_CHANNEL));
        assert!(names.contains(&"#general"));
    }

    #[tokio::test]
    async fn test_channel_exists() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        assert!(db.channel_exists(DEFAULT_CHANNEL).await.unwrap());
        assert!(
            db.channel_exists(&DEFAULT_CHANNEL.to_uppercase())
                .await
                .unwrap()
        ); // case-insensitive
        assert!(!db.channel_exists("#nonexistent").await.unwrap());
    }
}

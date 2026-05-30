//! Channel settings database operations
//!
//! Handles persistence of channel settings for persistent channels.
//! Ephemeral channels do not have their settings stored in the database.

use std::io;

use nexus_common::names::fold_name;
use sqlx::SqlitePool;

use crate::db::sql;

#[derive(Debug, Clone)]
pub struct ChannelSettings {
    pub name: String,
    pub topic: String,
    pub topic_set_by: String,
    pub secret: bool,
}

#[derive(Clone)]
pub struct ChannelDb {
    pool: SqlitePool,
}

impl ChannelDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_channel_settings(&self, name: &str) -> io::Result<Option<ChannelSettings>> {
        let result =
            sqlx::query_as::<_, (String, String, String, bool)>(sql::SQL_SELECT_CHANNEL_SETTINGS)
                .bind(fold_name(name))
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

    pub async fn get_channel_settings_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        name: &str,
    ) -> io::Result<Option<ChannelSettings>> {
        let result =
            sqlx::query_as::<_, (String, String, String, bool)>(sql::SQL_SELECT_CHANNEL_SETTINGS)
                .bind(fold_name(name))
                .fetch_optional(&mut **tx)
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

    pub async fn get_all_channel_settings_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> io::Result<Vec<ChannelSettings>> {
        let results = sqlx::query_as::<_, (String, String, String, bool)>(
            sql::SQL_SELECT_ALL_CHANNEL_SETTINGS,
        )
        .fetch_all(&mut **tx)
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

    pub async fn upsert_channel_settings(&self, settings: &ChannelSettings) -> io::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;
        Self::upsert_channel_settings_in_tx(&mut tx, settings).await?;
        tx.commit()
            .await
            .map_err(|e| io::Error::other(e.to_string()))
    }

    pub async fn upsert_channel_settings_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        settings: &ChannelSettings,
    ) -> io::Result<()> {
        sqlx::query(sql::SQL_UPSERT_CHANNEL_SETTINGS)
            .bind(&settings.name)
            .bind(fold_name(&settings.name))
            .bind(&settings.topic)
            .bind(&settings.topic_set_by)
            .bind(settings.secret)
            .execute(&mut **tx)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    pub async fn set_topic(&self, name: &str, topic: &str, set_by: &str) -> io::Result<()> {
        sqlx::query(sql::SQL_UPDATE_CHANNEL_TOPIC)
            .bind(topic)
            .bind(set_by)
            .bind(fold_name(name))
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    pub async fn set_secret(&self, name: &str, secret: bool) -> io::Result<()> {
        sqlx::query(sql::SQL_UPDATE_CHANNEL_SECRET)
            .bind(secret)
            .bind(fold_name(name))
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    pub async fn delete_channel_settings(&self, name: &str) -> io::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;
        Self::delete_channel_settings_in_tx(&mut tx, name).await?;
        tx.commit()
            .await
            .map_err(|e| io::Error::other(e.to_string()))
    }

    pub async fn delete_channel_settings_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        name: &str,
    ) -> io::Result<()> {
        sqlx::query(sql::SQL_DELETE_CHANNEL_SETTINGS)
            .bind(fold_name(name))
            .execute(&mut **tx)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
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
    async fn test_default_channel_settings_lookup_case_insensitive() {
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
    async fn test_channel_settings_unicode_case_insensitive() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        // Non-ASCII case pair (uppercase É folds to é) collapses only via the
        // Unicode-folded `name_lower`; the old ASCII-only COLLATE NOCASE kept
        // them distinct.
        db.upsert_channel_settings(&ChannelSettings {
            name: "#Café".to_string(),
            topic: "first".to_string(),
            topic_set_by: "admin".to_string(),
            secret: false,
        })
        .await
        .unwrap();

        // Lookup with arbitrary case finds the row; stored name keeps its case.
        let found = db.get_channel_settings("#café").await.unwrap().unwrap();
        assert_eq!(found.name, "#Café");
        assert_eq!(found.topic, "first");
        assert!(db.get_channel_settings("#CAFÉ").await.unwrap().is_some());

        let before = db.get_all_channel_settings().await.unwrap().len();

        // Re-upsert with different case resolves on name_lower → updates the same
        // row rather than inserting a second one.
        db.upsert_channel_settings(&ChannelSettings {
            name: "#café".to_string(),
            topic: "second".to_string(),
            topic_set_by: "admin".to_string(),
            secret: false,
        })
        .await
        .unwrap();

        let after = db.get_all_channel_settings().await.unwrap();
        assert_eq!(after.len(), before, "should update in place, not insert");
        let updated = db.get_channel_settings("#CAFÉ").await.unwrap().unwrap();
        assert_eq!(updated.topic, "second");
        // ON CONFLICT does not touch `name`, so the original stored case survives.
        assert_eq!(updated.name, "#Café");
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
        assert!(
            db.get_channel_settings("#deleteme")
                .await
                .unwrap()
                .is_some()
        );

        // Delete it
        db.delete_channel_settings("#deleteme").await.unwrap();

        // Verify it's gone
        assert!(
            db.get_channel_settings("#deleteme")
                .await
                .unwrap()
                .is_none()
        );
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
    async fn test_get_channel_settings_case_insensitive() {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);

        assert!(
            db.get_channel_settings(DEFAULT_CHANNEL)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            db.get_channel_settings(&DEFAULT_CHANNEL.to_uppercase())
                .await
                .unwrap()
                .is_some()
        ); // case-insensitive
        assert!(
            db.get_channel_settings("#nonexistent")
                .await
                .unwrap()
                .is_none()
        );
    }
}

//! Server configuration database operations

use std::io;

use nexus_common::validators::{
    ChannelListError, ServerDescriptionError, ServerImageError, ServerNameError,
    validate_auto_join_channels, validate_persistent_channels, validate_server_description,
    validate_server_image, validate_server_name,
};
use sqlx::SqlitePool;

use super::sql::{SQL_GET_CONFIG, SQL_SET_CONFIG};
use crate::constants::{
    CONFIG_KEY_AUTO_JOIN_CHANNELS, CONFIG_KEY_FILE_REINDEX_INTERVAL,
    CONFIG_KEY_MAX_CONNECTIONS_PER_IP, CONFIG_KEY_MAX_TRANSFERS_PER_IP,
    CONFIG_KEY_PERSISTENT_CHANNELS, CONFIG_KEY_SERVER_DESCRIPTION, CONFIG_KEY_SERVER_IMAGE,
    CONFIG_KEY_SERVER_NAME, DEFAULT_AUTO_JOIN_CHANNELS, DEFAULT_FILE_REINDEX_INTERVAL,
    DEFAULT_MAX_CONNECTIONS_PER_IP, DEFAULT_MAX_TRANSFERS_PER_IP, DEFAULT_PERSISTENT_CHANNELS,
    DEFAULT_SERVER_DESCRIPTION, DEFAULT_SERVER_IMAGE, DEFAULT_SERVER_NAME,
    ERR_SERVER_DESC_INVALID_CHARS, ERR_SERVER_DESC_NEWLINES, ERR_SERVER_DESC_TOO_LONG,
    ERR_SERVER_IMAGE_INVALID_FORMAT, ERR_SERVER_IMAGE_TOO_LARGE, ERR_SERVER_IMAGE_UNSUPPORTED_TYPE,
    ERR_SERVER_NAME_EMPTY, ERR_SERVER_NAME_INVALID_CHARS, ERR_SERVER_NAME_NEWLINES,
    ERR_SERVER_NAME_TOO_LONG,
};

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
    ///
    /// Returns the configured value, or 5 (the default) if not found or invalid.
    pub async fn get_max_connections_per_ip(&self) -> usize {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_CONNECTIONS_PER_IP)
    }

    /// Set the maximum connections allowed per IP address
    ///
    /// A value of 0 means unlimited connections are allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn set_max_connections_per_ip(&self, value: u32) -> io::Result<()> {
        sqlx::query(SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the maximum file transfer connections allowed per IP address
    ///
    /// Returns the configured value, or 3 (the default) if not found or invalid.
    pub async fn get_max_transfers_per_ip(&self) -> usize {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_MAX_TRANSFERS_PER_IP)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_TRANSFERS_PER_IP)
    }

    /// Set the maximum file transfer connections allowed per IP address
    ///
    /// A value of 0 means unlimited transfers are allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn set_max_transfers_per_ip(&self, value: u32) -> io::Result<()> {
        sqlx::query(SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_MAX_TRANSFERS_PER_IP)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the server name
    ///
    /// Returns the configured value, or "Nexus BBS" (the default) if not found.
    pub async fn get_server_name(&self) -> String {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_SERVER_NAME)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|_| DEFAULT_SERVER_NAME.to_string())
    }

    /// Set the server name
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or if the database update fails.
    pub async fn set_server_name(&self, name: &str) -> io::Result<()> {
        // Defense-in-depth validation
        if let Err(e) = validate_server_name(name) {
            let msg = match e {
                ServerNameError::Empty => ERR_SERVER_NAME_EMPTY,
                ServerNameError::TooLong => ERR_SERVER_NAME_TOO_LONG,
                ServerNameError::ContainsNewlines => ERR_SERVER_NAME_NEWLINES,
                ServerNameError::InvalidCharacters => ERR_SERVER_NAME_INVALID_CHARS,
            };
            return Err(io::Error::other(msg));
        }

        sqlx::query(SQL_SET_CONFIG)
            .bind(name)
            .bind(CONFIG_KEY_SERVER_NAME)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the server description
    ///
    /// Returns the configured value, or "" (empty string, the default) if not found.
    pub async fn get_server_description(&self) -> String {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_SERVER_DESCRIPTION)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|_| DEFAULT_SERVER_DESCRIPTION.to_string())
    }

    /// Set the server description
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or if the database update fails.
    pub async fn set_server_description(&self, description: &str) -> io::Result<()> {
        // Defense-in-depth validation
        if let Err(e) = validate_server_description(description) {
            let msg = match e {
                ServerDescriptionError::TooLong => ERR_SERVER_DESC_TOO_LONG,
                ServerDescriptionError::ContainsNewlines => ERR_SERVER_DESC_NEWLINES,
                ServerDescriptionError::InvalidCharacters => ERR_SERVER_DESC_INVALID_CHARS,
            };
            return Err(io::Error::other(msg));
        }

        sqlx::query(SQL_SET_CONFIG)
            .bind(description)
            .bind(CONFIG_KEY_SERVER_DESCRIPTION)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the server image
    ///
    /// Returns the configured value, or "" (empty string, the default) if not found.
    pub async fn get_server_image(&self) -> String {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_SERVER_IMAGE)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|_| DEFAULT_SERVER_IMAGE.to_string())
    }

    /// Set the server image
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or if the database update fails.
    /// An empty string is allowed to clear the image.
    pub async fn set_server_image(&self, image: &str) -> io::Result<()> {
        // Defense-in-depth validation (empty string is allowed to clear image)
        if !image.is_empty()
            && let Err(e) = validate_server_image(image)
        {
            let msg = match e {
                ServerImageError::TooLarge => ERR_SERVER_IMAGE_TOO_LARGE,
                ServerImageError::InvalidFormat => ERR_SERVER_IMAGE_INVALID_FORMAT,
                ServerImageError::UnsupportedType => ERR_SERVER_IMAGE_UNSUPPORTED_TYPE,
            };
            return Err(io::Error::other(msg));
        }

        sqlx::query(SQL_SET_CONFIG)
            .bind(image)
            .bind(CONFIG_KEY_SERVER_IMAGE)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the file reindex interval in minutes
    ///
    /// Returns the configured value, or 5 (the default) if not found or invalid.
    /// A value of 0 means automatic reindexing is disabled.
    pub async fn get_file_reindex_interval(&self) -> u32 {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_FILE_REINDEX_INTERVAL)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_FILE_REINDEX_INTERVAL)
    }

    /// Set the file reindex interval in minutes
    ///
    /// A value of 0 disables automatic reindexing.
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn set_file_reindex_interval(&self, value: u32) -> io::Result<()> {
        sqlx::query(SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_FILE_REINDEX_INTERVAL)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the persistent channels list
    ///
    /// Returns a space-separated string of channel names that survive restart.
    /// Returns the default channel from `DEFAULT_PERSISTENT_CHANNELS` if not configured.
    pub async fn get_persistent_channels(&self) -> String {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_PERSISTENT_CHANNELS)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|_| DEFAULT_PERSISTENT_CHANNELS.to_string())
    }

    /// Set the persistent channels list
    ///
    /// Value should be a space-separated string of channel names (e.g., "#general #support").
    /// These channels survive restart and can't be deleted when empty.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the database update fails.
    pub async fn set_persistent_channels(&self, value: &str) -> io::Result<()> {
        // Defense-in-depth validation
        if let Err(e) = validate_persistent_channels(value) {
            let msg = match e {
                ChannelListError::TooLong => "Persistent channels list is too long",
                ChannelListError::InvalidCharacters => {
                    "Persistent channels list contains invalid characters"
                }
                ChannelListError::ContainsNewlines => {
                    "Persistent channels list contains newlines"
                }
            };
            return Err(io::Error::other(msg));
        }

        sqlx::query(SQL_SET_CONFIG)
            .bind(value)
            .bind(CONFIG_KEY_PERSISTENT_CHANNELS)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the auto-join channels list
    ///
    /// Returns a space-separated string of channel names that users auto-join on login.
    /// Returns the default channel from `DEFAULT_AUTO_JOIN_CHANNELS` if not configured.
    pub async fn get_auto_join_channels(&self) -> String {
        sqlx::query_scalar::<_, String>(SQL_GET_CONFIG)
            .bind(CONFIG_KEY_AUTO_JOIN_CHANNELS)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|_| DEFAULT_AUTO_JOIN_CHANNELS.to_string())
    }

    /// Set the auto-join channels list
    ///
    /// Value should be a space-separated string of channel names (e.g., "#nexus #welcome").
    /// These channels are automatically joined by users on login.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the database update fails.
    pub async fn set_auto_join_channels(&self, value: &str) -> io::Result<()> {
        // Defense-in-depth validation
        if let Err(e) = validate_auto_join_channels(value) {
            let msg = match e {
                ChannelListError::TooLong => "Auto-join channels list is too long",
                ChannelListError::InvalidCharacters => {
                    "Auto-join channels list contains invalid characters"
                }
                ChannelListError::ContainsNewlines => "Auto-join channels list contains newlines",
            };
            return Err(io::Error::other(msg));
        }

        sqlx::query(SQL_SET_CONFIG)
            .bind(value)
            .bind(CONFIG_KEY_AUTO_JOIN_CHANNELS)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Parse channel list string into a list of channel names
    ///
    /// Handles space-separated values.
    /// Returns an empty Vec if the input is empty.
    pub fn parse_channel_list(value: &str) -> Vec<String> {
        value.split_whitespace().map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;
    use nexus_common::validators;

    #[tokio::test]
    async fn test_get_max_connections_per_ip_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to 5
        let limit = config_db.get_max_connections_per_ip().await;
        assert_eq!(limit, 5);
    }

    #[tokio::test]
    async fn test_set_max_connections_per_ip() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Set to new value
        config_db.set_max_connections_per_ip(10).await.unwrap();
        let limit = config_db.get_max_connections_per_ip().await;
        assert_eq!(limit, 10);
    }

    #[tokio::test]
    async fn test_set_max_connections_per_ip_zero_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // 0 means unlimited
        config_db.set_max_connections_per_ip(0).await.unwrap();
        let limit = config_db.get_max_connections_per_ip().await;
        assert_eq!(limit, 0);
    }

    #[tokio::test]
    async fn test_get_max_transfers_per_ip_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to 3
        let limit = config_db.get_max_transfers_per_ip().await;
        assert_eq!(limit, 3);
    }

    #[tokio::test]
    async fn test_set_max_transfers_per_ip() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Set to new value
        config_db.set_max_transfers_per_ip(5).await.unwrap();
        let limit = config_db.get_max_transfers_per_ip().await;
        assert_eq!(limit, 5);
    }

    #[tokio::test]
    async fn test_set_max_transfers_per_ip_zero_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // 0 means unlimited
        config_db.set_max_transfers_per_ip(0).await.unwrap();
        let limit = config_db.get_max_transfers_per_ip().await;
        assert_eq!(limit, 0);
    }

    #[tokio::test]
    async fn test_get_server_name_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to "Nexus BBS"
        let name = config_db.get_server_name().await;
        assert_eq!(name, "Nexus BBS");
    }

    #[tokio::test]
    async fn test_set_server_name() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_server_name("My Server").await.unwrap();
        let name = config_db.get_server_name().await;
        assert_eq!(name, "My Server");
    }

    #[tokio::test]
    async fn test_set_server_name_empty_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db.set_server_name("").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_set_server_name_too_long_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let long_name = "a".repeat(validators::MAX_SERVER_NAME_LENGTH + 1);
        let result = config_db.set_server_name(&long_name).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[tokio::test]
    async fn test_get_server_description_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to empty string
        let description = config_db.get_server_description().await;
        assert_eq!(description, "");
    }

    #[tokio::test]
    async fn test_set_server_description() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_server_description("Welcome to the server!")
            .await
            .unwrap();
        let description = config_db.get_server_description().await;
        assert_eq!(description, "Welcome to the server!");
    }

    #[tokio::test]
    async fn test_set_server_description_empty_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // First set to something
        config_db
            .set_server_description("Initial description")
            .await
            .unwrap();

        // Then clear it
        config_db.set_server_description("").await.unwrap();
        let description = config_db.get_server_description().await;
        assert_eq!(description, "");
    }

    #[tokio::test]
    async fn test_set_server_description_too_long_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let long_desc = "a".repeat(validators::MAX_SERVER_DESCRIPTION_LENGTH + 1);
        let result = config_db.set_server_description(&long_desc).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    // =========================================================================
    // Server Image Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_server_image_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to empty string
        let image = config_db.get_server_image().await;
        assert_eq!(image, "");
    }

    #[tokio::test]
    async fn test_set_server_image() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let image = "data:image/png;base64,iVBORw0KGgo=";
        config_db.set_server_image(image).await.unwrap();
        let result = config_db.get_server_image().await;
        assert_eq!(result, image);
    }

    #[tokio::test]
    async fn test_set_server_image_empty_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // First set to something
        config_db
            .set_server_image("data:image/png;base64,iVBORw0KGgo=")
            .await
            .unwrap();

        // Then clear it
        config_db.set_server_image("").await.unwrap();
        let image = config_db.get_server_image().await;
        assert_eq!(image, "");
    }

    #[tokio::test]
    async fn test_set_server_image_invalid_format_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db.set_server_image("not a data uri").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid format"));
    }

    #[tokio::test]
    async fn test_set_server_image_unsupported_type_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db
            .set_server_image("data:image/gif;base64,R0lGODlh")
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported type"));
    }

    #[tokio::test]
    async fn test_set_server_image_too_large_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Create an image that exceeds the limit
        let prefix = "data:image/png;base64,";
        let padding = "A".repeat(validators::MAX_SERVER_IMAGE_DATA_URI_LENGTH);
        let large_image = format!("{}{}", prefix, padding);

        let result = config_db.set_server_image(&large_image).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    // =========================================================================
    // File Reindex Interval Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_file_reindex_interval_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to 5 minutes
        let interval = config_db.get_file_reindex_interval().await;
        assert_eq!(interval, 5);
    }

    #[tokio::test]
    async fn test_set_file_reindex_interval() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Set to new value
        config_db.set_file_reindex_interval(10).await.unwrap();
        let interval = config_db.get_file_reindex_interval().await;
        assert_eq!(interval, 10);
    }

    #[tokio::test]
    async fn test_set_file_reindex_interval_zero_disables() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // 0 disables automatic reindexing
        config_db.set_file_reindex_interval(0).await.unwrap();
        let interval = config_db.get_file_reindex_interval().await;
        assert_eq!(interval, 0);
    }
}

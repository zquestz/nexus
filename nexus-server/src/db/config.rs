//! Server configuration database operations

use std::collections::HashMap;
use std::io;

use nexus_common::validators::{
    ChannelListError, PasswordStrength, ServerDescriptionError, ServerImageError, ServerNameError,
    validate_auto_join_channels, validate_persistent_channels, validate_server_description,
    validate_server_image, validate_server_name,
};
use sqlx::SqlitePool;

use crate::constants::{
    CONFIG_KEY_AUTO_JOIN_CHANNELS, CONFIG_KEY_CHAT_BURST_LIMIT, CONFIG_KEY_CHAT_RATE_LIMIT,
    CONFIG_KEY_FILE_REINDEX_INTERVAL, CONFIG_KEY_MAX_CONNECTIONS_PER_IP,
    CONFIG_KEY_MAX_TRANSFERS_PER_IP, CONFIG_KEY_MIN_PASSWORD_STRENGTH,
    CONFIG_KEY_PERSISTENT_CHANNELS, CONFIG_KEY_SERVER_DESCRIPTION, CONFIG_KEY_SERVER_IMAGE,
    CONFIG_KEY_SERVER_NAME, DEFAULT_AUTO_JOIN_CHANNELS, DEFAULT_CHAT_BURST_LIMIT,
    DEFAULT_CHAT_RATE_LIMIT, DEFAULT_FILE_REINDEX_INTERVAL, DEFAULT_MAX_CONNECTIONS_PER_IP,
    DEFAULT_MAX_TRANSFERS_PER_IP, DEFAULT_MIN_PASSWORD_STRENGTH, DEFAULT_PERSISTENT_CHANNELS,
    DEFAULT_SERVER_DESCRIPTION, DEFAULT_SERVER_IMAGE, DEFAULT_SERVER_NAME,
    ERR_SERVER_DESC_INVALID_CHARS, ERR_SERVER_DESC_NEWLINES, ERR_SERVER_DESC_TOO_LONG,
    ERR_SERVER_IMAGE_INVALID_FORMAT, ERR_SERVER_IMAGE_TOO_LARGE, ERR_SERVER_IMAGE_UNSUPPORTED_TYPE,
    ERR_SERVER_NAME_EMPTY, ERR_SERVER_NAME_INVALID_CHARS, ERR_SERVER_NAME_NEWLINES,
    ERR_SERVER_NAME_TOO_LONG,
};
use crate::db::sql;

/// All server configuration values, fetched in a single query.
///
/// Used by handlers that need multiple config values (e.g., building `ServerInfo`).
/// Individual getters remain for cases where only one value is needed.
pub struct ServerConfig {
    pub server_name: String,
    pub server_description: String,
    pub server_image: String,
    pub max_connections_per_ip: u32,
    pub max_transfers_per_ip: u32,
    pub file_reindex_interval: u32,
    pub persistent_channels: String,
    pub auto_join_channels: String,
    pub min_password_strength: PasswordStrength,
    pub chat_burst_limit: u32,
    pub chat_rate_limit: u32,
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

    /// Get all server configuration values in a single query.
    ///
    /// Returns a `ServerConfig` with all values, using defaults for any missing keys.
    pub async fn get_all(&self) -> ServerConfig {
        let rows: Vec<(String, String)> = sqlx::query_as(sql::SQL_GET_ALL_CONFIG)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let mut map: HashMap<String, String> = rows.into_iter().collect();

        ServerConfig {
            server_name: map
                .remove(CONFIG_KEY_SERVER_NAME)
                .unwrap_or_else(|| DEFAULT_SERVER_NAME.to_string()),
            server_description: map
                .remove(CONFIG_KEY_SERVER_DESCRIPTION)
                .unwrap_or_else(|| DEFAULT_SERVER_DESCRIPTION.to_string()),
            server_image: map
                .remove(CONFIG_KEY_SERVER_IMAGE)
                .unwrap_or_else(|| DEFAULT_SERVER_IMAGE.to_string()),
            max_connections_per_ip: map
                .remove(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MAX_CONNECTIONS_PER_IP as u32),
            max_transfers_per_ip: map
                .remove(CONFIG_KEY_MAX_TRANSFERS_PER_IP)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MAX_TRANSFERS_PER_IP as u32),
            file_reindex_interval: map
                .remove(CONFIG_KEY_FILE_REINDEX_INTERVAL)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_FILE_REINDEX_INTERVAL),
            persistent_channels: map
                .remove(CONFIG_KEY_PERSISTENT_CHANNELS)
                .unwrap_or_else(|| DEFAULT_PERSISTENT_CHANNELS.to_string()),
            auto_join_channels: map
                .remove(CONFIG_KEY_AUTO_JOIN_CHANNELS)
                .unwrap_or_else(|| DEFAULT_AUTO_JOIN_CHANNELS.to_string()),
            min_password_strength: map
                .remove(CONFIG_KEY_MIN_PASSWORD_STRENGTH)
                .and_then(|v| v.parse::<u8>().ok())
                .map(PasswordStrength::from)
                .unwrap_or(DEFAULT_MIN_PASSWORD_STRENGTH),
            chat_burst_limit: map
                .remove(CONFIG_KEY_CHAT_BURST_LIMIT)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_CHAT_BURST_LIMIT),
            chat_rate_limit: map
                .remove(CONFIG_KEY_CHAT_RATE_LIMIT)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_CHAT_RATE_LIMIT),
        }
    }

    /// Get the maximum connections allowed per IP address
    ///
    /// Returns the configured value, or 5 (the default) if not found or invalid.
    pub async fn get_max_connections_per_ip(&self) -> usize {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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
        sqlx::query(sql::SQL_SET_CONFIG)
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
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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
        sqlx::query(sql::SQL_SET_CONFIG)
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
    #[cfg(test)]
    pub async fn get_server_name(&self) -> String {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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

        sqlx::query(sql::SQL_SET_CONFIG)
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
    #[cfg(test)]
    pub async fn get_server_description(&self) -> String {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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

        sqlx::query(sql::SQL_SET_CONFIG)
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
    #[cfg(test)]
    pub async fn get_server_image(&self) -> String {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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

        sqlx::query(sql::SQL_SET_CONFIG)
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
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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
        sqlx::query(sql::SQL_SET_CONFIG)
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
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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
                ChannelListError::ContainsNewlines => "Persistent channels list contains newlines",
            };
            return Err(io::Error::other(msg));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
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
    #[cfg(test)]
    pub async fn get_auto_join_channels(&self) -> String {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
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

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value)
            .bind(CONFIG_KEY_AUTO_JOIN_CHANNELS)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the minimum password strength requirement
    ///
    /// Returns the configured value, or `Good` (the default) if not found or invalid.
    pub async fn get_min_password_strength(&self) -> PasswordStrength {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_MIN_PASSWORD_STRENGTH)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .map(PasswordStrength::from)
            .unwrap_or(DEFAULT_MIN_PASSWORD_STRENGTH)
    }

    /// Set the minimum password strength requirement
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn set_min_password_strength(&self, value: PasswordStrength) -> io::Result<()> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.score().to_string())
            .bind(CONFIG_KEY_MIN_PASSWORD_STRENGTH)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get the chat burst limit (max messages in a burst)
    ///
    /// A value of 0 means no burst allowance (capacity is 1).
    /// Defaults to 5 if not configured.
    pub async fn get_chat_burst_limit(&self) -> u32 {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_CHAT_BURST_LIMIT)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CHAT_BURST_LIMIT)
    }

    /// Set the chat burst limit
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn set_chat_burst_limit(&self, value: u32) -> io::Result<()> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_CHAT_BURST_LIMIT)
            .execute(&self.pool)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(())
    }

    /// Get the chat rate limit (messages per minute)
    ///
    /// A value of 0 means flood protection is disabled.
    /// Defaults to 20 if not configured.
    pub async fn get_chat_rate_limit(&self) -> u32 {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_CHAT_RATE_LIMIT)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CHAT_RATE_LIMIT)
    }

    /// Set the chat rate limit
    ///
    /// # Errors
    ///
    /// Returns an error if the database update fails.
    pub async fn set_chat_rate_limit(&self, value: u32) -> io::Result<()> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_CHAT_RATE_LIMIT)
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

    // =========================================================================
    // Min Password Strength Tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_min_password_strength_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let strength = config_db.get_min_password_strength().await;
        assert_eq!(strength, validators::PasswordStrength::Good);
    }

    #[tokio::test]
    async fn test_set_min_password_strength() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_min_password_strength(validators::PasswordStrength::Strong)
            .await
            .unwrap();
        let strength = config_db.get_min_password_strength().await;
        assert_eq!(strength, validators::PasswordStrength::Strong);
    }

    #[tokio::test]
    async fn test_set_min_password_strength_weak() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_min_password_strength(validators::PasswordStrength::Weak)
            .await
            .unwrap();
        let strength = config_db.get_min_password_strength().await;
        assert_eq!(strength, validators::PasswordStrength::Weak);
    }

    #[tokio::test]
    async fn test_set_min_password_strength_excellent() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_min_password_strength(validators::PasswordStrength::Excellent)
            .await
            .unwrap();
        let strength = config_db.get_min_password_strength().await;
        assert_eq!(strength, validators::PasswordStrength::Excellent);
    }

    #[tokio::test]
    async fn test_get_chat_burst_limit_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        let limit = config_db.get_chat_burst_limit().await;
        assert_eq!(limit, 5);
    }

    #[tokio::test]
    async fn test_set_chat_burst_limit() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_chat_burst_limit(10).await.unwrap();
        let limit = config_db.get_chat_burst_limit().await;
        assert_eq!(limit, 10);
    }

    #[tokio::test]
    async fn test_set_chat_burst_limit_zero() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_chat_burst_limit(0).await.unwrap();
        let limit = config_db.get_chat_burst_limit().await;
        assert_eq!(limit, 0);
    }

    #[tokio::test]
    async fn test_get_chat_rate_limit_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        let limit = config_db.get_chat_rate_limit().await;
        assert_eq!(limit, 20);
    }

    #[tokio::test]
    async fn test_set_chat_rate_limit() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_chat_rate_limit(60).await.unwrap();
        let limit = config_db.get_chat_rate_limit().await;
        assert_eq!(limit, 60);
    }

    #[tokio::test]
    async fn test_set_chat_rate_limit_zero() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_chat_rate_limit(0).await.unwrap();
        let limit = config_db.get_chat_rate_limit().await;
        assert_eq!(limit, 0);
    }

    #[tokio::test]
    async fn test_get_all_config_defaults() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let config = config_db.get_all().await;
        assert_eq!(config.max_connections_per_ip, 5);
        assert_eq!(config.max_transfers_per_ip, 3);
        assert_eq!(config.chat_burst_limit, 5);
        assert_eq!(config.chat_rate_limit, 20);
    }

    #[tokio::test]
    async fn test_get_all_config_after_updates() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_chat_burst_limit(10).await.unwrap();
        config_db.set_chat_rate_limit(60).await.unwrap();
        config_db.set_max_connections_per_ip(20).await.unwrap();

        let config = config_db.get_all().await;
        assert_eq!(config.chat_burst_limit, 10);
        assert_eq!(config.chat_rate_limit, 60);
        assert_eq!(config.max_connections_per_ip, 20);
        // Unchanged values should still be defaults
        assert_eq!(config.max_transfers_per_ip, 3);
    }
}

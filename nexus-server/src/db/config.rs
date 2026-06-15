//! Server configuration database operations

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use nexus_common::validators::{
    BandwidthChunkSizeError, ChannelListError, DEFAULT_BANDWIDTH_CHUNK_SIZE,
    MAX_BANDWIDTH_CHUNK_SIZE, MIN_BANDWIDTH_CHUNK_SIZE, PasswordStrength, PublicAddressError,
    ServerDescriptionError, ServerImageError, ServerNameError, validate_auto_join_channels,
    validate_bandwidth_chunk_size, validate_persistent_channels, validate_public_address,
    validate_server_description, validate_server_image, validate_server_name,
};
use sqlx::SqlitePool;

use crate::constants::{
    CONFIG_KEY_AUTO_JOIN_CHANNELS, CONFIG_KEY_CHAT_BURST_LIMIT, CONFIG_KEY_CHAT_RATE_LIMIT,
    CONFIG_KEY_FILE_REINDEX_INTERVAL, CONFIG_KEY_MAX_CONNECTIONS_PER_IP,
    CONFIG_KEY_MAX_OUTBOUND_RATE, CONFIG_KEY_MAX_TRANSFERS_PER_IP,
    CONFIG_KEY_MIN_PASSWORD_STRENGTH, CONFIG_KEY_PERSISTENT_CHANNELS, CONFIG_KEY_PUBLIC_ADDRESS,
    CONFIG_KEY_SCHEDULER_CHUNK_SIZE, CONFIG_KEY_SERVER_DESCRIPTION, CONFIG_KEY_SERVER_IMAGE,
    CONFIG_KEY_SERVER_NAME, DEFAULT_AUTO_JOIN_CHANNELS, DEFAULT_CHAT_BURST_LIMIT,
    DEFAULT_CHAT_RATE_LIMIT, DEFAULT_FILE_REINDEX_INTERVAL, DEFAULT_MAX_CONNECTIONS_PER_IP,
    DEFAULT_MAX_OUTBOUND_RATE, DEFAULT_MAX_TRANSFERS_PER_IP, DEFAULT_MIN_PASSWORD_STRENGTH,
    DEFAULT_PERSISTENT_CHANNELS, DEFAULT_PUBLIC_ADDRESS, DEFAULT_SERVER_DESCRIPTION,
    DEFAULT_SERVER_IMAGE, DEFAULT_SERVER_NAME, ERR_AUTO_JOIN_CHANNELS_INVALID_CHARS,
    ERR_AUTO_JOIN_CHANNELS_NEWLINES, ERR_AUTO_JOIN_CHANNELS_TOO_LONG,
    ERR_PERSISTENT_CHANNELS_INVALID_CHARS, ERR_PERSISTENT_CHANNELS_NEWLINES,
    ERR_PERSISTENT_CHANNELS_TOO_LONG, ERR_PUBLIC_ADDRESS_CONTAINS_BRACKETS,
    ERR_PUBLIC_ADDRESS_CONTAINS_PATH, ERR_PUBLIC_ADDRESS_CONTAINS_PORT,
    ERR_PUBLIC_ADDRESS_CONTAINS_SCHEME, ERR_PUBLIC_ADDRESS_CONTAINS_USERINFO,
    ERR_PUBLIC_ADDRESS_CONTAINS_WHITESPACE, ERR_PUBLIC_ADDRESS_CONTAINS_ZONE_ID,
    ERR_PUBLIC_ADDRESS_INVALID_FORMAT, ERR_PUBLIC_ADDRESS_TOO_LONG,
    ERR_SCHEDULER_CHUNK_SIZE_TOO_LARGE, ERR_SCHEDULER_CHUNK_SIZE_TOO_SMALL,
    ERR_SERVER_DESC_INVALID_CHARS, ERR_SERVER_DESC_NEWLINES, ERR_SERVER_DESC_TOO_LONG,
    ERR_SERVER_IMAGE_INVALID_FORMAT, ERR_SERVER_IMAGE_TOO_LARGE, ERR_SERVER_IMAGE_UNDECODABLE,
    ERR_SERVER_IMAGE_UNSUPPORTED_TYPE, ERR_SERVER_IMAGE_VALIDATE_TASK_FAILED,
    ERR_SERVER_NAME_EMPTY, ERR_SERVER_NAME_INVALID_CHARS, ERR_SERVER_NAME_NEWLINES,
    ERR_SERVER_NAME_TOO_LONG,
};
use crate::db::sql;

/// Configuration fields the tracker task needs to build
/// `TrackerServerRegister` payloads. Subset of [`ServerConfig`] —
/// fetched via a dedicated single-query method to avoid pulling
/// unrelated config rows on the tracker task's per-refresh hot path.
///
/// `description` and `public_address` are `Option<String>` because
/// empty values are normalized to `None` here so the caller doesn't
/// need to special-case empty strings.
pub struct TrackerConfigFields {
    pub server_name: String,
    pub description: Option<String>,
    pub public_address: Option<String>,
}

/// All server configuration values, fetched in a single query.
///
/// Used by handlers that need multiple config values (e.g., building `ServerInfo`).
/// Individual getters remain for cases where only one value is needed.
pub struct ServerConfig {
    pub server_name: String,
    pub server_description: String,
    pub server_image: String,
    pub public_address: String,
    pub max_connections_per_ip: u32,
    pub max_transfers_per_ip: u32,
    pub file_reindex_interval: u32,
    pub persistent_channels: String,
    pub auto_join_channels: String,
    pub min_password_strength: PasswordStrength,
    pub chat_burst_limit: u32,
    pub chat_rate_limit: u32,
    /// Maximum outbound bandwidth in bytes/sec (0 = unlimited).
    pub max_outbound_rate: u64,
    /// WF2Q+ scheduler chunk size in bytes.
    pub scheduler_chunk_size: u32,
}

#[derive(Clone)]
pub struct ConfigDb {
    pool: SqlitePool,
}

#[derive(Debug)]
pub enum ConfigDbError {
    Db(sqlx::Error),
    Validation(String),
}

impl fmt::Display for ConfigDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigDbError::Db(err) => write!(f, "{err}"),
            ConfigDbError::Validation(msg) => f.write_str(msg),
        }
    }
}

impl Error for ConfigDbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigDbError::Db(err) => Some(err),
            ConfigDbError::Validation(_) => None,
        }
    }
}

impl From<sqlx::Error> for ConfigDbError {
    fn from(err: sqlx::Error) -> Self {
        ConfigDbError::Db(err)
    }
}

impl ConfigDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// All server configuration values in a single query, using
    /// defaults for any missing keys.
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
            public_address: map
                .remove(CONFIG_KEY_PUBLIC_ADDRESS)
                .unwrap_or_else(|| DEFAULT_PUBLIC_ADDRESS.to_string()),
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
            max_outbound_rate: map
                .remove(CONFIG_KEY_MAX_OUTBOUND_RATE)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MAX_OUTBOUND_RATE),
            scheduler_chunk_size: map
                .remove(CONFIG_KEY_SCHEDULER_CHUNK_SIZE)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_BANDWIDTH_CHUNK_SIZE),
        }
    }

    pub async fn set_max_outbound_rate_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u64,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_MAX_OUTBOUND_RATE)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn set_scheduler_chunk_size_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u32,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates).
        if let Err(e) = validate_bandwidth_chunk_size(value) {
            let msg = match e {
                BandwidthChunkSizeError::TooSmall => {
                    format!("{ERR_SCHEDULER_CHUNK_SIZE_TOO_SMALL} {MIN_BANDWIDTH_CHUNK_SIZE}")
                }
                BandwidthChunkSizeError::TooLarge => {
                    format!("{ERR_SCHEDULER_CHUNK_SIZE_TOO_LARGE} {MAX_BANDWIDTH_CHUNK_SIZE}")
                }
            };
            return Err(ConfigDbError::Validation(msg));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_SCHEDULER_CHUNK_SIZE)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Fetch the three config fields the tracker task needs for
    /// `TrackerServerRegister` payloads: server name, description,
    /// public address. Single query against the `config` table
    /// (rather than three separate `SQL_GET_CONFIG` calls); this is
    /// the tracker task's per-refresh hot path and we want to keep it
    /// efficient as the config table grows over time.
    ///
    /// Empty `description` and `public_address` values are normalized
    /// to `None` here so the protocol-level message construction
    /// doesn't need to special-case empty strings.
    pub async fn get_tracker_fields(&self) -> Result<TrackerConfigFields, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(sql::SQL_GET_TRACKER_CONFIG_FIELDS)
            .fetch_all(&self.pool)
            .await?;

        let mut map: HashMap<String, String> = rows.into_iter().collect();

        Ok(TrackerConfigFields {
            server_name: map
                .remove(CONFIG_KEY_SERVER_NAME)
                .unwrap_or_else(|| DEFAULT_SERVER_NAME.to_string()),
            description: map
                .remove(CONFIG_KEY_SERVER_DESCRIPTION)
                .filter(|s| !s.is_empty()),
            public_address: map
                .remove(CONFIG_KEY_PUBLIC_ADDRESS)
                .filter(|s| !s.is_empty()),
        })
    }

    pub async fn get_max_connections_per_ip(&self) -> usize {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_CONNECTIONS_PER_IP)
    }

    pub async fn set_max_connections_per_ip_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u32,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn get_max_transfers_per_ip(&self) -> usize {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_MAX_TRANSFERS_PER_IP)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_TRANSFERS_PER_IP)
    }

    pub async fn set_max_transfers_per_ip_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u32,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_MAX_TRANSFERS_PER_IP)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn set_server_name_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        name: &str,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates).
        if let Err(e) = validate_server_name(name) {
            let msg = match e {
                ServerNameError::Empty => ERR_SERVER_NAME_EMPTY,
                ServerNameError::TooLong => ERR_SERVER_NAME_TOO_LONG,
                ServerNameError::ContainsNewlines => ERR_SERVER_NAME_NEWLINES,
                ServerNameError::InvalidCharacters => ERR_SERVER_NAME_INVALID_CHARS,
            };
            return Err(ConfigDbError::Validation(msg.to_string()));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(name)
            .bind(CONFIG_KEY_SERVER_NAME)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn set_server_description_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        description: &str,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates).
        if let Err(e) = validate_server_description(description) {
            let msg = match e {
                ServerDescriptionError::TooLong => ERR_SERVER_DESC_TOO_LONG,
                ServerDescriptionError::ContainsNewlines => ERR_SERVER_DESC_NEWLINES,
                ServerDescriptionError::InvalidCharacters => ERR_SERVER_DESC_INVALID_CHARS,
            };
            return Err(ConfigDbError::Validation(msg.to_string()));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(description)
            .bind(CONFIG_KEY_SERVER_DESCRIPTION)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn set_server_image_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        image: &str,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates); empty string clears the image.
        if !image.is_empty() {
            validate_server_image_for_db(image).await?;
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(image)
            .bind(CONFIG_KEY_SERVER_IMAGE)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn set_public_address_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: &str,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates); empty string clears the address.
        if let Err(e) = validate_public_address(value) {
            let msg = match e {
                PublicAddressError::TooLong => ERR_PUBLIC_ADDRESS_TOO_LONG,
                PublicAddressError::ContainsScheme => ERR_PUBLIC_ADDRESS_CONTAINS_SCHEME,
                PublicAddressError::ContainsBrackets => ERR_PUBLIC_ADDRESS_CONTAINS_BRACKETS,
                PublicAddressError::ContainsPath => ERR_PUBLIC_ADDRESS_CONTAINS_PATH,
                PublicAddressError::ContainsUserinfo => ERR_PUBLIC_ADDRESS_CONTAINS_USERINFO,
                PublicAddressError::ContainsWhitespace => ERR_PUBLIC_ADDRESS_CONTAINS_WHITESPACE,
                PublicAddressError::ContainsPort => ERR_PUBLIC_ADDRESS_CONTAINS_PORT,
                PublicAddressError::ContainsZoneId => ERR_PUBLIC_ADDRESS_CONTAINS_ZONE_ID,
                PublicAddressError::InvalidFormat => ERR_PUBLIC_ADDRESS_INVALID_FORMAT,
            };
            return Err(ConfigDbError::Validation(msg.to_string()));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value)
            .bind(CONFIG_KEY_PUBLIC_ADDRESS)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

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

    pub async fn set_file_reindex_interval_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u32,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_FILE_REINDEX_INTERVAL)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    /// Space-separated string of channel names that survive restart.
    pub async fn get_persistent_channels(&self) -> String {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_PERSISTENT_CHANNELS)
            .fetch_one(&self.pool)
            .await
            .unwrap_or_else(|_| DEFAULT_PERSISTENT_CHANNELS.to_string())
    }

    pub async fn set_persistent_channels_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: &str,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates).
        if let Err(e) = validate_persistent_channels(value) {
            let msg = match e {
                ChannelListError::TooLong => ERR_PERSISTENT_CHANNELS_TOO_LONG,
                ChannelListError::InvalidCharacters => ERR_PERSISTENT_CHANNELS_INVALID_CHARS,
                ChannelListError::ContainsNewlines => ERR_PERSISTENT_CHANNELS_NEWLINES,
            };
            return Err(ConfigDbError::Validation(msg.to_string()));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value)
            .bind(CONFIG_KEY_PERSISTENT_CHANNELS)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    pub async fn set_auto_join_channels_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: &str,
    ) -> Result<(), ConfigDbError> {
        // Defense-in-depth (handler also validates).
        if let Err(e) = validate_auto_join_channels(value) {
            let msg = match e {
                ChannelListError::TooLong => ERR_AUTO_JOIN_CHANNELS_TOO_LONG,
                ChannelListError::InvalidCharacters => ERR_AUTO_JOIN_CHANNELS_INVALID_CHARS,
                ChannelListError::ContainsNewlines => ERR_AUTO_JOIN_CHANNELS_NEWLINES,
            };
            return Err(ConfigDbError::Validation(msg.to_string()));
        }

        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value)
            .bind(CONFIG_KEY_AUTO_JOIN_CHANNELS)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

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

    pub async fn set_min_password_strength_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: PasswordStrength,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.score().to_string())
            .bind(CONFIG_KEY_MIN_PASSWORD_STRENGTH)
            .execute(&mut **tx)
            .await?;

        Ok(())
    }

    /// A value of 0 means no burst allowance (capacity is 1).
    pub async fn get_chat_burst_limit(&self) -> u32 {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_CHAT_BURST_LIMIT)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CHAT_BURST_LIMIT)
    }

    pub async fn set_chat_burst_limit_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u32,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_CHAT_BURST_LIMIT)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Messages per minute; a value of 0 disables flood protection.
    pub async fn get_chat_rate_limit(&self) -> u32 {
        sqlx::query_scalar::<_, String>(sql::SQL_GET_CONFIG)
            .bind(CONFIG_KEY_CHAT_RATE_LIMIT)
            .fetch_one(&self.pool)
            .await
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CHAT_RATE_LIMIT)
    }

    pub async fn set_chat_rate_limit_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        value: u32,
    ) -> Result<(), ConfigDbError> {
        sqlx::query(sql::SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_CHAT_RATE_LIMIT)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub fn parse_channel_list(value: &str) -> Vec<String> {
        value.split_whitespace().map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
impl ConfigDb {
    async fn begin_test_tx(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, ConfigDbError> {
        Ok(self.pool.begin().await?)
    }

    pub async fn set_max_outbound_rate(&self, value: u64) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_max_outbound_rate_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_scheduler_chunk_size(&self, value: u32) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_scheduler_chunk_size_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    /// A value of 0 means unlimited connections are allowed.
    pub async fn set_max_connections_per_ip(&self, value: u32) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_max_connections_per_ip_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    /// A value of 0 means unlimited transfers are allowed.
    pub async fn set_max_transfers_per_ip(&self, value: u32) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_max_transfers_per_ip_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_server_name(&self, name: &str) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_server_name_in_tx(&mut tx, name).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_server_description(&self, description: &str) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_server_description_in_tx(&mut tx, description).await?;
        Ok(tx.commit().await?)
    }

    /// An empty string is allowed to clear the image.
    pub async fn set_server_image(&self, image: &str) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_server_image_in_tx(&mut tx, image).await?;
        Ok(tx.commit().await?)
    }

    /// An empty string is allowed to clear the advertised address.
    pub async fn set_public_address(&self, value: &str) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_public_address_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    /// A value of 0 disables automatic reindexing.
    pub async fn set_file_reindex_interval(&self, value: u32) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_file_reindex_interval_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    /// These channels survive restart and can't be deleted when empty.
    pub async fn set_persistent_channels(&self, value: &str) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_persistent_channels_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_auto_join_channels(&self, value: &str) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_auto_join_channels_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_min_password_strength(
        &self,
        value: PasswordStrength,
    ) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_min_password_strength_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_chat_burst_limit(&self, value: u32) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_chat_burst_limit_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }

    pub async fn set_chat_rate_limit(&self, value: u32) -> Result<(), ConfigDbError> {
        let mut tx = self.begin_test_tx().await?;
        Self::set_chat_rate_limit_in_tx(&mut tx, value).await?;
        Ok(tx.commit().await?)
    }
}

async fn validate_server_image_for_db(image: &str) -> Result<(), ConfigDbError> {
    let owned = image.to_string();
    match tokio::task::spawn_blocking(move || validate_server_image(&owned)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(ConfigDbError::Validation(
            server_image_error_message(e).to_string(),
        )),
        Err(e) => Err(ConfigDbError::Validation(format!(
            "{ERR_SERVER_IMAGE_VALIDATE_TASK_FAILED}: {e}"
        ))),
    }
}

fn server_image_error_message(error: ServerImageError) -> &'static str {
    match error {
        ServerImageError::TooLarge => ERR_SERVER_IMAGE_TOO_LARGE,
        ServerImageError::InvalidFormat => ERR_SERVER_IMAGE_INVALID_FORMAT,
        ServerImageError::UnsupportedType => ERR_SERVER_IMAGE_UNSUPPORTED_TYPE,
        ServerImageError::Undecodable => ERR_SERVER_IMAGE_UNDECODABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;
    use nexus_common::validators;

    const VALID_PNG_DATA_URI: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    fn expect_config_validation_error(result: Result<(), ConfigDbError>, expected_fragment: &str) {
        match result {
            Err(ConfigDbError::Validation(msg)) => {
                assert!(
                    msg.contains(expected_fragment),
                    "validation error {msg:?} should contain {expected_fragment:?}"
                );
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn config_setters_preserve_sqlx_errors() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool.clone());
        pool.close().await;

        assert!(matches!(
            config_db.set_max_connections_per_ip(10).await,
            Err(ConfigDbError::Db(_))
        ));
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
    async fn test_set_max_connections_per_ip() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_max_connections_per_ip(10).await.unwrap();
        let limit = config_db.get_max_connections_per_ip().await;
        assert_eq!(limit, 10);
    }

    #[tokio::test]
    async fn test_set_max_connections_per_ip_zero_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

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

        config_db.set_max_transfers_per_ip(5).await.unwrap();
        let limit = config_db.get_max_transfers_per_ip().await;
        assert_eq!(limit, 5);
    }

    #[tokio::test]
    async fn test_set_max_transfers_per_ip_zero_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_max_transfers_per_ip(0).await.unwrap();
        let limit = config_db.get_max_transfers_per_ip().await;
        assert_eq!(limit, 0);
    }

    #[tokio::test]
    async fn test_get_server_name_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to "Nexus BBS"
        let name = config_db.get_all().await.server_name;
        assert_eq!(name, "Nexus BBS");
    }

    #[tokio::test]
    async fn test_set_server_name() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_server_name("My Server").await.unwrap();
        let name = config_db.get_all().await.server_name;
        assert_eq!(name, "My Server");
    }

    #[tokio::test]
    async fn test_set_server_name_empty_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db.set_server_name("").await;
        expect_config_validation_error(result, "cannot be empty");
    }

    #[tokio::test]
    async fn test_set_server_name_too_long_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let long_name = "a".repeat(validators::MAX_SERVER_NAME_LENGTH + 1);
        let result = config_db.set_server_name(&long_name).await;
        expect_config_validation_error(result, "too long");
    }

    #[tokio::test]
    async fn test_get_server_description_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to empty string
        let description = config_db.get_all().await.server_description;
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
        let description = config_db.get_all().await.server_description;
        assert_eq!(description, "Welcome to the server!");
    }

    #[tokio::test]
    async fn test_set_server_description_empty_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_server_description("Initial description")
            .await
            .unwrap();

        config_db.set_server_description("").await.unwrap();
        let description = config_db.get_all().await.server_description;
        assert_eq!(description, "");
    }

    #[tokio::test]
    async fn test_set_server_description_too_long_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let long_desc = "a".repeat(validators::MAX_SERVER_DESCRIPTION_LENGTH + 1);
        let result = config_db.set_server_description(&long_desc).await;
        expect_config_validation_error(result, "too long");
    }

    #[tokio::test]
    async fn test_get_server_image_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to empty string
        let image = config_db.get_all().await.server_image;
        assert_eq!(image, "");
    }

    #[tokio::test]
    async fn test_set_server_image() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let image = VALID_PNG_DATA_URI;
        config_db.set_server_image(image).await.unwrap();
        let result = config_db.get_all().await.server_image;
        assert_eq!(result, image);
    }

    #[tokio::test]
    async fn test_set_server_image_empty_allowed() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db
            .set_server_image(VALID_PNG_DATA_URI)
            .await
            .unwrap();

        config_db.set_server_image("").await.unwrap();
        let image = config_db.get_all().await.server_image;
        assert_eq!(image, "");
    }

    #[tokio::test]
    async fn test_set_server_image_invalid_format_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db.set_server_image("not a data uri").await;
        expect_config_validation_error(result, "invalid format");
    }

    #[tokio::test]
    async fn test_set_server_image_unsupported_type_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db
            .set_server_image("data:image/gif;base64,R0lGODlh")
            .await;
        expect_config_validation_error(result, "unsupported type");
    }

    #[tokio::test]
    async fn test_set_server_image_too_large_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let prefix = "data:image/png;base64,";
        let padding = "A".repeat(validators::MAX_SERVER_IMAGE_DATA_URI_LENGTH);
        let large_image = format!("{}{}", prefix, padding);

        let result = config_db.set_server_image(&large_image).await;
        expect_config_validation_error(result, "too large");
    }

    #[tokio::test]
    async fn test_get_public_address_default() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        let value = config_db.get_all().await.public_address;
        assert_eq!(value, "");
    }

    #[tokio::test]
    async fn test_set_public_address_hostname() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db
            .set_public_address("bbs.example.com")
            .await
            .unwrap();
        assert_eq!(config_db.get_all().await.public_address, "bbs.example.com");
    }

    #[tokio::test]
    async fn test_set_public_address_ipv4() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_public_address("203.0.113.5").await.unwrap();
        assert_eq!(config_db.get_all().await.public_address, "203.0.113.5");
    }

    #[tokio::test]
    async fn test_set_public_address_ipv6() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_public_address("2001:db8::1").await.unwrap();
        assert_eq!(config_db.get_all().await.public_address, "2001:db8::1");
    }

    #[tokio::test]
    async fn test_set_public_address_unicode_preserved() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db.set_public_address("münchen.de").await.unwrap();
        // Option C: stored as-typed, not normalized
        assert_eq!(config_db.get_all().await.public_address, "münchen.de");
    }

    #[tokio::test]
    async fn test_set_public_address_empty_clears() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db
            .set_public_address("bbs.example.com")
            .await
            .unwrap();
        config_db.set_public_address("").await.unwrap();
        assert_eq!(config_db.get_all().await.public_address, "");
    }

    #[tokio::test]
    async fn test_set_public_address_rejects_scheme() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        let result = config_db.set_public_address("nexus://example.com").await;
        expect_config_validation_error(result, "scheme");
    }

    #[tokio::test]
    async fn test_set_public_address_rejects_port() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        let result = config_db.set_public_address("example.com:7500").await;
        expect_config_validation_error(result, "port");
    }

    #[tokio::test]
    async fn test_set_public_address_rejects_brackets() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        let result = config_db.set_public_address("[::1]").await;
        expect_config_validation_error(result, "brackets");
    }

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

        config_db.set_file_reindex_interval(10).await.unwrap();
        let interval = config_db.get_file_reindex_interval().await;
        assert_eq!(interval, 10);
    }

    #[tokio::test]
    async fn test_set_file_reindex_interval_zero_disables() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        config_db.set_file_reindex_interval(0).await.unwrap();
        let interval = config_db.get_file_reindex_interval().await;
        assert_eq!(interval, 0);
    }

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
    async fn test_set_persistent_channels() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);
        config_db
            .set_persistent_channels("#general #support")
            .await
            .unwrap();
        let value = config_db.get_persistent_channels().await;
        assert_eq!(value, "#general #support");
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

    #[tokio::test]
    async fn test_set_max_outbound_rate_round_trips() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to 0 (unlimited).
        assert_eq!(config_db.get_all().await.max_outbound_rate, 0);

        config_db.set_max_outbound_rate(125_000_000).await.unwrap();
        assert_eq!(config_db.get_all().await.max_outbound_rate, 125_000_000);

        config_db.set_max_outbound_rate(0).await.unwrap();
        assert_eq!(config_db.get_all().await.max_outbound_rate, 0);
    }

    #[tokio::test]
    async fn test_set_scheduler_chunk_size_round_trips() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        // Migration sets default to 8192.
        assert_eq!(config_db.get_all().await.scheduler_chunk_size, 8192);

        config_db.set_scheduler_chunk_size(4096).await.unwrap();
        assert_eq!(config_db.get_all().await.scheduler_chunk_size, 4096);

        config_db.set_scheduler_chunk_size(65536).await.unwrap();
        assert_eq!(config_db.get_all().await.scheduler_chunk_size, 65536);
    }

    #[tokio::test]
    async fn test_set_scheduler_chunk_size_rejects_too_small() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        expect_config_validation_error(
            config_db
                .set_scheduler_chunk_size(MIN_BANDWIDTH_CHUNK_SIZE - 1)
                .await,
            &MIN_BANDWIDTH_CHUNK_SIZE.to_string(),
        );

        // Stored value should remain the default (no write happened).
        assert_eq!(config_db.get_all().await.scheduler_chunk_size, 8192);
    }

    #[tokio::test]
    async fn test_set_scheduler_chunk_size_rejects_too_large() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        expect_config_validation_error(
            config_db
                .set_scheduler_chunk_size(MAX_BANDWIDTH_CHUNK_SIZE + 1)
                .await,
            &MAX_BANDWIDTH_CHUNK_SIZE.to_string(),
        );

        // Stored value should remain the default (no write happened).
        assert_eq!(config_db.get_all().await.scheduler_chunk_size, 8192);
    }

    #[tokio::test]
    async fn test_get_all_includes_bandwidth_defaults() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let config = config_db.get_all().await;
        assert_eq!(config.max_outbound_rate, 0);
        assert_eq!(config.scheduler_chunk_size, 8192);
    }
}

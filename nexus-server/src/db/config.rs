//! Server configuration database operations

use nexus_common::validators::{
    ServerDescriptionError, ServerImageError, ServerNameError, validate_server_description,
    validate_server_image, validate_server_name,
};

use super::sql::{SQL_GET_CONFIG, SQL_SET_CONFIG};
use crate::constants::{
    CONFIG_KEY_MAX_CONNECTIONS_PER_IP, CONFIG_KEY_SERVER_DESCRIPTION, CONFIG_KEY_SERVER_IMAGE,
    CONFIG_KEY_SERVER_NAME, DEFAULT_MAX_CONNECTIONS_PER_IP, DEFAULT_SERVER_DESCRIPTION,
    DEFAULT_SERVER_IMAGE, DEFAULT_SERVER_NAME, ERR_MAX_CONNECTIONS_ZERO,
    ERR_SERVER_DESC_INVALID_CHARS, ERR_SERVER_DESC_NEWLINES, ERR_SERVER_DESC_TOO_LONG,
    ERR_SERVER_IMAGE_INVALID_FORMAT, ERR_SERVER_IMAGE_TOO_LARGE, ERR_SERVER_IMAGE_UNSUPPORTED_TYPE,
    ERR_SERVER_NAME_EMPTY, ERR_SERVER_NAME_INVALID_CHARS, ERR_SERVER_NAME_NEWLINES,
    ERR_SERVER_NAME_TOO_LONG,
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
    /// # Errors
    ///
    /// Returns an error if the value is zero or if the database update fails.
    pub async fn set_max_connections_per_ip(&self, value: u32) -> io::Result<()> {
        if value == 0 {
            return Err(io::Error::other(ERR_MAX_CONNECTIONS_ZERO));
        }

        sqlx::query(SQL_SET_CONFIG)
            .bind(value.to_string())
            .bind(CONFIG_KEY_MAX_CONNECTIONS_PER_IP)
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
    async fn test_set_max_connections_per_ip_zero_fails() {
        let pool = create_test_db().await;
        let config_db = ConfigDb::new(pool);

        let result = config_db.set_max_connections_per_ip(0).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must be greater than 0")
        );
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
}

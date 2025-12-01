//! Server bookmark configuration

mod theme;

use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

use crate::i18n::{t, t_args};
use crate::types::ServerBookmark;

pub use theme::ThemePreference;

// =============================================================================
// Constants
// =============================================================================

/// File permissions for config file on Unix (owner read/write only)
#[cfg(unix)]
const CONFIG_FILE_MODE: u32 = 0o600;

// =============================================================================
// Config Struct
// =============================================================================

/// Application configuration containing server bookmarks and theme preference
///
/// The config is persisted to disk as JSON in the platform-specific
/// configuration directory (e.g., ~/.config/nexus/config.json on Linux).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// Server bookmarks for quick connect
    pub bookmarks: Vec<ServerBookmark>,
    /// UI theme preference (light or dark mode)
    #[serde(default)]
    pub theme: ThemePreference,
    /// Application-wide locale setting (e.g., "en", "zh-CN")
    #[serde(default = "default_config_locale")]
    pub locale: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bookmarks: Vec::new(),
            theme: ThemePreference::default(),
            locale: default_config_locale(),
        }
    }
}

/// Default locale value for Config (used by serde)
fn default_config_locale() -> String {
    crate::i18n::DEFAULT_LOCALE.to_string()
}

// =============================================================================
// Config Implementation
// =============================================================================

impl Config {
    // -------------------------------------------------------------------------
    // Persistence
    // -------------------------------------------------------------------------

    /// Get the platform-specific config file path
    ///
    /// Returns None if the config directory cannot be determined.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("nexus").join("config.json"))
    }

    /// Load config from disk, or return default if not found
    ///
    /// Returns a default config if:
    /// - Config directory cannot be determined
    /// - Config file doesn't exist
    /// - Config file cannot be read
    /// - Config file contains invalid JSON
    pub fn load() -> Self {
        if let Some(path) = Self::config_path()
            && path.exists()
            && let Ok(contents) = fs::read_to_string(&path)
            && let Ok(config) = serde_json::from_str(&contents)
        {
            return config;
        }
        Self::default()
    }

    /// Save config to disk with restrictive permissions
    ///
    /// Creates the config directory if it doesn't exist.
    /// On Unix systems, sets file permissions to 0o600 (owner read/write only)
    /// to protect saved passwords.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or_else(|| t("err-could-not-determine-config-dir"))?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                t_args("err-failed-create-config-dir", &[("error", &e.to_string())])
            })?;
        }

        // Serialize config to pretty JSON
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| t_args("err-failed-serialize-config", &[("error", &e.to_string())]))?;

        // Write to disk
        fs::write(&path, json)
            .map_err(|e| t_args("err-failed-write-config", &[("error", &e.to_string())]))?;

        // Set restrictive permissions on Unix (owner read/write only)
        #[cfg(unix)]
        Self::set_config_permissions(&path)?;

        Ok(())
    }

    /// Set config file permissions to owner read/write only on Unix systems
    #[cfg(unix)]
    fn set_config_permissions(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path).map_err(|e| {
            t_args(
                "err-failed-read-config-metadata",
                &[("error", &e.to_string())],
            )
        })?;
        let mut perms = metadata.permissions();
        perms.set_mode(CONFIG_FILE_MODE);

        fs::set_permissions(path, perms).map_err(|e| {
            t_args(
                "err-failed-set-config-permissions",
                &[("error", &e.to_string())],
            )
        })?;

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Bookmark Management
    // -------------------------------------------------------------------------

    /// Add a new bookmark to the configuration
    pub fn add_bookmark(&mut self, bookmark: ServerBookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Delete a bookmark at the given index
    ///
    /// Does nothing if the index is out of bounds.
    pub fn delete_bookmark(&mut self, index: usize) {
        if index < self.bookmarks.len() {
            self.bookmarks.remove(index);
        }
    }

    /// Get a bookmark by index
    ///
    /// Returns None if the index is out of bounds.
    pub fn get_bookmark(&self, index: usize) -> Option<&ServerBookmark> {
        self.bookmarks.get(index)
    }

    /// Update an existing bookmark at the given index
    ///
    /// Does nothing if the index is out of bounds.
    pub fn update_bookmark(&mut self, index: usize, bookmark: ServerBookmark) {
        if index < self.bookmarks.len() {
            self.bookmarks[index] = bookmark;
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a bookmark with just a name
    fn bookmark(name: &str) -> ServerBookmark {
        ServerBookmark {
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.bookmarks.len(), 0);
        assert_eq!(config.theme.0, iced::Theme::Dark);
        assert_eq!(config.locale, crate::i18n::DEFAULT_LOCALE);
    }

    #[test]
    fn test_add_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Test Server");
    }

    #[test]
    fn test_add_multiple_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));
        config.add_bookmark(bookmark("Server 2"));

        assert_eq!(config.bookmarks.len(), 2);
        assert_eq!(config.bookmarks[0].name, "Server 1");
        assert_eq!(config.bookmarks[1].name, "Server 2");
    }

    #[test]
    fn test_config_path_format() {
        if let Some(path) = Config::config_path() {
            assert!(
                path.ends_with("nexus/config.json"),
                "Config path should end with nexus/config.json, got: {:?}",
                path
            );
        }
    }

    #[test]
    fn test_delete_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));
        config.add_bookmark(bookmark("Server 2"));

        config.delete_bookmark(0);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark_out_of_bounds() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.delete_bookmark(10);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    #[test]
    fn test_get_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        let result = config.get_bookmark(0);
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test Server"));
    }

    #[test]
    fn test_get_bookmark_out_of_bounds() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert!(config.get_bookmark(5).is_none());
    }

    #[test]
    fn test_update_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Original"));

        config.update_bookmark(
            0,
            ServerBookmark {
                name: "Updated".to_string(),
                address: "200::2".to_string(),
                port: "8000".to_string(),
                auto_connect: true,
                ..Default::default()
            },
        );

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Updated");
        assert_eq!(config.bookmarks[0].address, "200::2");
        assert_eq!(config.bookmarks[0].port, "8000");
        assert!(config.bookmarks[0].auto_connect);
    }

    #[test]
    fn test_update_bookmark_out_of_bounds() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.update_bookmark(5, bookmark("Should Not Appear"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }
}

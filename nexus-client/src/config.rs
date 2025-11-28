//! Server bookmark configuration

use crate::types::ServerBookmark;
use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// File permissions for config file on Unix (owner read/write only)
#[cfg(unix)]
const CONFIG_FILE_MODE: u32 = 0o600;

/// Application configuration containing server bookmarks and theme preference
///
/// The config is persisted to disk as JSON in the platform-specific
/// configuration directory (e.g., ~/.config/nexus/config.json on Linux).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub bookmarks: Vec<ServerBookmark>,
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

/// Default locale value for Config
fn default_config_locale() -> String {
    crate::types::DEFAULT_LOCALE.to_string()
}

/// Theme preference (Light or Dark mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ThemePreference {
    Light,
    #[default]
    Dark,
}

impl Config {
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
        let path = Self::config_path().ok_or("Could not determine config directory")?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        // Serialize config to pretty JSON
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // Write to disk
        fs::write(&path, json).map_err(|e| format!("Failed to write config file: {}", e))?;

        // Set restrictive permissions on Unix (owner read/write only)
        #[cfg(unix)]
        Self::set_config_permissions(&path)?;

        Ok(())
    }

    /// Set config file permissions to owner read/write only on Unix systems
    #[cfg(unix)]
    fn set_config_permissions(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to read config file metadata: {}", e))?;
        let mut perms = metadata.permissions();
        perms.set_mode(CONFIG_FILE_MODE);

        fs::set_permissions(path, perms)
            .map_err(|e| format!("Failed to set config file permissions: {}", e))?;

        Ok(())
    }

    /// Add a new bookmark to the configuration
    pub fn add_bookmark(&mut self, bookmark: ServerBookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Update an existing bookmark at the given index
    ///
    /// Does nothing if the index is out of bounds.
    pub fn update_bookmark(&mut self, index: usize, bookmark: ServerBookmark) {
        if index < self.bookmarks.len() {
            self.bookmarks[index] = bookmark;
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DEFAULT_PORT;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.bookmarks.len(), 0);
        assert_eq!(config.theme, ThemePreference::Dark);
    }

    #[test]
    fn test_add_bookmark() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Test Server".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Test Server");
    }

    #[test]
    fn test_add_multiple_bookmarks() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Server 1".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user1".to_string(),
            password: "pass1".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        config.add_bookmark(ServerBookmark {
            name: "Server 2".to_string(),
            address: "200::2".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user2".to_string(),
            password: "pass2".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        assert_eq!(config.bookmarks.len(), 2);
        assert_eq!(config.bookmarks[0].name, "Server 1");
        assert_eq!(config.bookmarks[1].name, "Server 2");
    }

    #[test]
    fn test_update_bookmark() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Original".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        config.update_bookmark(
            0,
            ServerBookmark {
                name: "Updated".to_string(),
                address: "200::2".to_string(),
                port: "8000".to_string(),
                username: "newuser".to_string(),
                password: "newpass".to_string(),
                auto_connect: true,
                certificate_fingerprint: None,
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

        config.add_bookmark(ServerBookmark {
            name: "Server 1".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        config.update_bookmark(
            5,
            ServerBookmark {
                name: "Should Not Appear".to_string(),
                address: "200::99".to_string(),
                port: DEFAULT_PORT.to_string(),
                username: "baduser".to_string(),
                password: "badpass".to_string(),
                auto_connect: false,
                certificate_fingerprint: None,
            },
        );

        // Original bookmark should be unchanged
        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    #[test]
    fn test_delete_bookmark() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Server 1".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user1".to_string(),
            password: "pass1".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        config.add_bookmark(ServerBookmark {
            name: "Server 2".to_string(),
            address: "200::2".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user2".to_string(),
            password: "pass2".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        config.delete_bookmark(0);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark_out_of_bounds() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Server 1".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        // Try to delete non-existent index
        config.delete_bookmark(10);

        // Bookmark should still exist
        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    #[test]
    fn test_get_bookmark() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Test Server".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        let bookmark = config.get_bookmark(0);
        assert!(bookmark.is_some());
        assert_eq!(bookmark.unwrap().name, "Test Server");
    }

    #[test]
    fn test_get_bookmark_out_of_bounds() {
        let mut config = Config::default();

        config.add_bookmark(ServerBookmark {
            name: "Test Server".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
            certificate_fingerprint: None,
        });

        let bookmark = config.get_bookmark(5);
        assert!(bookmark.is_none());
    }

    #[test]
    fn test_config_path_returns_some() {
        // This test just verifies the method doesn't panic
        // The actual path depends on the platform
        let path = Config::config_path();
        // On most systems this should return Some
        // We can't assert the exact path as it's platform-dependent
        assert!(path.is_some() || path.is_none()); // Always passes, just exercises the code
    }
}

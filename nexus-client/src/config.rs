//! Server bookmark configuration

use crate::types::ServerBookmark;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub bookmarks: Vec<ServerBookmark>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bookmarks: Vec::new(),
        }
    }
}

impl Config {
    /// Get the config file path
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("nexus").join("config.json"))
    }

    /// Load config from disk, or create default if not exists
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str(&contents) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    /// Save config to disk
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or("Could not determine config directory")?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        fs::write(&path, json).map_err(|e| format!("Failed to write config file: {}", e))?;

        // Set restrictive permissions on Unix (owner read/write only)
        #[cfg(unix)]
        Self::set_config_permissions(&path)?;

        Ok(())
    }

    /// Set config file permissions to 0o600 (owner read/write only) on Unix systems
    #[cfg(unix)]
    fn set_config_permissions(path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to read config file metadata: {}", e))?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o600);

        fs::set_permissions(path, perms)
            .map_err(|e| format!("Failed to set config file permissions: {}", e))?;

        Ok(())
    }

    /// Add a bookmark
    pub fn add_bookmark(&mut self, bookmark: ServerBookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Update a bookmark at index
    pub fn update_bookmark(&mut self, index: usize, bookmark: ServerBookmark) {
        if index < self.bookmarks.len() {
            self.bookmarks[index] = bookmark;
        }
    }

    /// Delete a bookmark at index
    pub fn delete_bookmark(&mut self, index: usize) {
        if index < self.bookmarks.len() {
            self.bookmarks.remove(index);
        }
    }

    /// Get a bookmark by index
    pub fn get_bookmark(&self, index: usize) -> Option<&ServerBookmark> {
        self.bookmarks.get(index)
    }
}

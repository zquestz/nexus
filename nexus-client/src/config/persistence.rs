//! Configuration persistence (load/save)

use std::fs;
use std::path::PathBuf;

use crate::constants::{APP_DIR_NAME, CONFIG_FILE_NAME};
use crate::i18n::{t, t_args};

use super::Config;

impl Config {
    /// Get the platform-specific config file path
    ///
    /// Returns None if the config directory cannot be determined.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join(APP_DIR_NAME).join(CONFIG_FILE_NAME))
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

        // Create parent directory (owner-only) if it doesn't exist
        if let Some(parent) = path.parent() {
            crate::secure_file::create_dir_owner_only(parent).map_err(|e| {
                t_args("err-failed-create-config-dir", &[("error", &e.to_string())])
            })?;
        }

        // Serialize config to pretty JSON
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| t_args("err-failed-serialize-config", &[("error", &e.to_string())]))?;

        // Atomic owner-only replacement — config holds saved passwords.
        nexus_common::secure_file::write_atomic(&path, json.as_bytes())
            .map_err(|e| t_args("err-failed-write-config", &[("error", &e.to_string())]))?;

        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}

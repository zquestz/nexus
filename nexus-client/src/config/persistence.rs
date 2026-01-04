//! Configuration persistence (load/save)

use std::fs;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

use crate::constants::{APP_DIR_NAME, CONFIG_FILE_NAME};
use crate::i18n::{t, t_args};

use super::Config;

/// File permissions for config file on Unix (owner read/write only)
#[cfg(unix)]
const CONFIG_FILE_MODE: u32 = 0o600;

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

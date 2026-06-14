//! Configuration persistence (load/save)

use std::fs;
use std::path::{Path, PathBuf};

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
    /// - Config file contains invalid JSON (after moving it aside)
    pub fn load() -> Self {
        Self::config_path()
            .as_deref()
            .map(Self::load_from_path)
            .unwrap_or_default()
    }

    /// Save config to disk with restrictive permissions
    ///
    /// Creates the config directory if it doesn't exist.
    /// On Unix systems, sets file permissions to 0o600 (owner read/write only)
    /// to protect saved passwords.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or_else(|| t("err-could-not-determine-config-dir"))?;
        self.save_to_path(&path)
    }

    fn load_from_path(path: &Path) -> Self {
        let Ok(contents) = fs::read_to_string(path) else {
            return Self::default();
        };

        match serde_json::from_str(&contents) {
            Ok(config) => config,
            Err(_) => {
                let _ = preserve_corrupt_config(path);
                Self::default()
            }
        }
    }

    fn save_to_path(&self, path: &Path) -> Result<(), String> {
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
        nexus_common::secure_file::write_atomic(path, json.as_bytes())
            .map_err(|e| t_args("err-failed-write-config", &[("error", &e.to_string())]))?;

        Ok(())
    }
}

fn preserve_corrupt_config(path: &Path) -> std::io::Result<PathBuf> {
    let backup = next_corrupt_backup_path(path);
    match fs::rename(path, &backup) {
        Ok(()) => Ok(backup),
        Err(_) => {
            fs::copy(path, &backup)?;
            Ok(backup)
        }
    }
}

fn next_corrupt_backup_path(path: &Path) -> PathBuf {
    let first = path.with_extension("json.corrupt");
    if !first.exists() {
        return first;
    }

    for index in 1.. {
        let candidate = path.with_extension(format!("json.corrupt.{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    // The unbounded search above should not exhaust in practice; if a future
    // compiler ever treats it as fallible, fall back to the first backup name
    // rather than panicking during config recovery.
    first
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

    #[test]
    fn corrupt_config_is_moved_aside_before_defaults_are_returned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        fs::write(&path, b"{ invalid json").expect("write corrupt config");

        let config = Config::load_from_path(&path);

        assert_eq!(config.bookmarks.len(), 0);
        assert!(!path.exists());
        assert_eq!(
            fs::read(path.with_extension("json.corrupt")).expect("read backup"),
            b"{ invalid json"
        );
    }

    #[test]
    fn corrupt_config_backup_does_not_overwrite_existing_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        fs::write(&path, b"{ invalid json").expect("write corrupt config");
        fs::write(path.with_extension("json.corrupt"), b"older backup").expect("write backup");

        let _ = Config::load_from_path(&path);

        assert_eq!(
            fs::read(path.with_extension("json.corrupt")).expect("read first backup"),
            b"older backup"
        );
        assert_eq!(
            fs::read(path.with_extension("json.corrupt.1")).expect("read second backup"),
            b"{ invalid json"
        );
    }

    #[test]
    fn save_after_corrupt_load_leaves_backup_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        fs::write(&path, b"{ invalid json").expect("write corrupt config");

        let config = Config::load_from_path(&path);
        config.save_to_path(&path).expect("save default config");

        assert!(path.exists());
        assert_eq!(
            fs::read(path.with_extension("json.corrupt")).expect("read backup"),
            b"{ invalid json"
        );
        let saved = fs::read_to_string(&path).expect("read saved config");
        serde_json::from_str::<Config>(&saved).expect("saved config should parse");
    }

    #[test]
    fn semantically_invalid_config_is_moved_aside() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        let config = r#"{
            "bookmarks": [{
                "name": "example",
                "address": "example.com",
                "port": 70000,
                "username": "alice",
                "password": "secret"
            }]
        }"#;
        fs::write(&path, config).expect("write invalid config");

        let loaded = Config::load_from_path(&path);

        assert_eq!(loaded.bookmarks.len(), 0);
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.corrupt")).expect("read backup"),
            config
        );
    }

    #[test]
    fn missing_config_returns_default_without_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");

        let config = Config::load_from_path(&path);

        assert_eq!(config.bookmarks.len(), 0);
        assert!(!path.exists());
        assert!(!path.with_extension("json.corrupt").exists());
    }
}

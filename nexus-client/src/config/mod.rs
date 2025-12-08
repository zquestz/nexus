//! Application configuration
//!
//! Configuration is split into:
//! - `Settings` - User preferences (theme, font size, notifications)
//! - `bookmarks` - Server bookmarks for quick connect

mod bookmarks;
mod persistence;
pub mod settings;
pub mod theme;

use crate::types::ServerBookmark;
use settings::Settings;

// =============================================================================
// Config
// =============================================================================

/// Application configuration containing settings and server bookmarks
///
/// Persisted to disk as JSON in the platform-specific configuration directory
/// (e.g., ~/.config/nexus/config.json on Linux).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Config {
    /// User preferences
    #[serde(default)]
    pub settings: Settings,

    /// Server bookmarks for quick connect
    #[serde(default)]
    pub bookmarks: Vec<ServerBookmark>,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        // Only test Config-level defaults; Settings defaults are tested in settings.rs
        assert_eq!(config.bookmarks.len(), 0);
    }
}

//! Application configuration
//!
//! Configuration is split into:
//! - `Settings` - User preferences (theme, font size, notifications)
//! - `bookmarks` - Server bookmarks for quick connect
//! - `client_trackers` - Trackers the user queries for server discovery

pub mod audio;
mod bookmarks;
pub mod events;
mod persistence;
pub mod settings;
pub mod theme;
mod trackers;

use crate::types::{ClientTracker, ServerBookmark};
pub(crate) use bookmarks::canonical_bookmark_host;
use settings::Settings;

// =============================================================================
// Config
// =============================================================================

/// Application configuration containing settings, server bookmarks, and
/// the user's tracker discovery list.
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

    /// User-managed list of trackers to query for server discovery.
    /// Each entry carries its own TOFU pin and optional registration
    /// password. See [`crate::types::ClientTracker`] for the field-level
    /// invariants.
    #[serde(default)]
    pub client_trackers: Vec<ClientTracker>,
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
        assert_eq!(config.client_trackers.len(), 0);
    }
}

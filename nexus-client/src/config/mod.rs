//! Application configuration
//!
//! Configuration is split into:
//! - `Settings` - User preferences (theme, font size, notifications)
//! - `bookmarks` - Server bookmarks for quick connect

mod bookmarks;
mod persistence;
mod settings;
mod theme;

use crate::types::ServerBookmark;

pub use settings::{CHAT_FONT_SIZE_MAX, CHAT_FONT_SIZE_MIN, CHAT_FONT_SIZES, Settings};
pub use theme::ThemePreference;

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
    use crate::i18n::DEFAULT_LOCALE;
    use settings::CHAT_FONT_SIZE_DEFAULT;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.bookmarks.len(), 0);
        assert_eq!(config.settings.theme.0, iced::Theme::Dark);
        assert_eq!(config.settings.locale, DEFAULT_LOCALE);
        assert_eq!(config.settings.chat_font_size, CHAT_FONT_SIZE_DEFAULT);
        assert!(config.settings.show_connection_notifications);
    }
}

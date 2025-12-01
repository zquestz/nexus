//! User preference settings

use crate::i18n::DEFAULT_LOCALE;

use super::ThemePreference;

// =============================================================================
// Constants
// =============================================================================

/// Minimum allowed chat font size
pub const CHAT_FONT_SIZE_MIN: u8 = 9;

/// Maximum allowed chat font size
pub const CHAT_FONT_SIZE_MAX: u8 = 16;

/// Default chat font size
pub const CHAT_FONT_SIZE_DEFAULT: u8 = 13;

/// All valid chat font sizes for the picker
pub const CHAT_FONT_SIZES: &[u8] = &[9, 10, 11, 12, 13, 14, 15, 16];

// =============================================================================
// Settings
// =============================================================================

/// User preferences for the application
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// UI theme preference
    #[serde(default)]
    pub theme: ThemePreference,

    /// Application-wide locale setting (e.g., "en", "zh-CN")
    #[serde(default = "default_locale")]
    pub locale: String,

    /// Font size for chat messages (9-16)
    #[serde(default = "default_chat_font_size")]
    pub chat_font_size: u8,

    /// Show user connect/disconnect notifications in chat
    #[serde(default = "default_true")]
    pub show_connection_notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            locale: default_locale(),
            chat_font_size: default_chat_font_size(),
            show_connection_notifications: default_true(),
        }
    }
}

// =============================================================================
// Default Functions (for serde)
// =============================================================================

fn default_locale() -> String {
    DEFAULT_LOCALE.to_string()
}

fn default_chat_font_size() -> u8 {
    CHAT_FONT_SIZE_DEFAULT
}

fn default_true() -> bool {
    true
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.theme.0, iced::Theme::Dark);
        assert_eq!(settings.locale, DEFAULT_LOCALE);
        assert_eq!(settings.chat_font_size, CHAT_FONT_SIZE_DEFAULT);
        assert!(settings.show_connection_notifications);
    }

    #[test]
    fn test_chat_font_sizes_array() {
        assert_eq!(CHAT_FONT_SIZES.len(), 8);
        assert_eq!(CHAT_FONT_SIZES[0], CHAT_FONT_SIZE_MIN);
        assert_eq!(CHAT_FONT_SIZES[7], CHAT_FONT_SIZE_MAX);
    }
}

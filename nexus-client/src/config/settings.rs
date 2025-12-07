//! User preference settings

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

    /// Font size for chat messages (9-16)
    #[serde(default = "default_chat_font_size")]
    pub chat_font_size: u8,

    /// Show user connect/disconnect notifications in chat
    #[serde(default = "default_true")]
    pub show_connection_notifications: bool,

    /// Show timestamps in chat messages
    #[serde(default = "default_true")]
    pub show_timestamps: bool,

    /// Use 24-hour time format (false = 12-hour with AM/PM)
    #[serde(default)]
    pub use_24_hour_time: bool,

    /// Show seconds in timestamps
    #[serde(default = "default_true")]
    pub show_seconds: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            chat_font_size: default_chat_font_size(),
            show_connection_notifications: default_true(),
            show_timestamps: default_true(),
            use_24_hour_time: false,
            show_seconds: default_true(),
        }
    }
}

// =============================================================================
// Default Functions (for serde)
// =============================================================================

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
        assert_eq!(settings.theme, ThemePreference::default());
        assert_eq!(settings.chat_font_size, CHAT_FONT_SIZE_DEFAULT);
        assert!(settings.show_connection_notifications);
        assert!(settings.show_timestamps);
        assert!(!settings.use_24_hour_time);
        assert!(settings.show_seconds);
    }

    #[test]
    fn test_chat_font_sizes_array() {
        assert_eq!(CHAT_FONT_SIZES.len(), 8);
        assert_eq!(CHAT_FONT_SIZES[0], CHAT_FONT_SIZE_MIN);
        assert_eq!(CHAT_FONT_SIZES[7], CHAT_FONT_SIZE_MAX);
    }

    #[test]
    fn test_settings_serialization_roundtrip() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).expect("serialize");
        let deserialized: Settings = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(settings.theme.0, deserialized.theme.0);
        assert_eq!(settings.chat_font_size, deserialized.chat_font_size);
        assert_eq!(
            settings.show_connection_notifications,
            deserialized.show_connection_notifications
        );
        assert_eq!(settings.show_timestamps, deserialized.show_timestamps);
        assert_eq!(settings.use_24_hour_time, deserialized.use_24_hour_time);
        assert_eq!(settings.show_seconds, deserialized.show_seconds);
    }
}

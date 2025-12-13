//! User preference settings

use crate::style::{WINDOW_HEIGHT, WINDOW_WIDTH};

use super::theme::ThemePreference;

// =============================================================================
// Constants
// =============================================================================

/// Maximum avatar size in bytes (128KB)
pub const AVATAR_MAX_SIZE: usize = 128 * 1024;

/// Maximum server image size in bytes (512KB)
pub const SERVER_IMAGE_MAX_SIZE: usize = 512 * 1024;

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
#[derive(Clone, serde::Serialize, serde::Deserialize)]
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

    /// User avatar as data URI (e.g., "data:image/png;base64,...")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,

    /// Window width in pixels
    #[serde(default = "default_window_width")]
    pub window_width: f32,

    /// Window height in pixels
    #[serde(default = "default_window_height")]
    pub window_height: f32,

    /// Window X position (None = system default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_x: Option<i32>,

    /// Window Y position (None = system default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_y: Option<i32>,
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
            avatar: None,
            window_width: default_window_width(),
            window_height: default_window_height(),
            window_x: None,
            window_y: None,
        }
    }
}

// =============================================================================
// Default Functions (for serde)
// =============================================================================

// Manual Debug implementation to avoid printing large avatar data URIs
impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("theme", &self.theme)
            .field("chat_font_size", &self.chat_font_size)
            .field(
                "show_connection_notifications",
                &self.show_connection_notifications,
            )
            .field("show_timestamps", &self.show_timestamps)
            .field("use_24_hour_time", &self.use_24_hour_time)
            .field("show_seconds", &self.show_seconds)
            .field(
                "avatar",
                &self.avatar.as_ref().map(|a| format!("<{} bytes>", a.len())),
            )
            .finish()
    }
}

fn default_chat_font_size() -> u8 {
    CHAT_FONT_SIZE_DEFAULT
}

fn default_true() -> bool {
    true
}

fn default_window_width() -> f32 {
    WINDOW_WIDTH
}

fn default_window_height() -> f32 {
    WINDOW_HEIGHT
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
        assert!(settings.avatar.is_none());
        assert_eq!(settings.window_width, WINDOW_WIDTH);
        assert_eq!(settings.window_height, WINDOW_HEIGHT);
        assert!(settings.window_x.is_none());
        assert!(settings.window_y.is_none());
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
        assert_eq!(settings.avatar, deserialized.avatar);
    }

    #[test]
    fn test_settings_with_avatar_serialization_roundtrip() {
        let settings = Settings {
            avatar: Some("data:image/png;base64,abc123".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&settings).expect("serialize");
        let deserialized: Settings = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(settings.avatar, deserialized.avatar);
    }
}

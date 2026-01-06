//! User preference settings

use crate::style::{WINDOW_HEIGHT, WINDOW_WIDTH};

use super::events::EventSettings;
use super::theme::ThemePreference;

// =============================================================================
// Proxy Settings
// =============================================================================

/// Default SOCKS5 proxy address (localhost for Tor)
pub const DEFAULT_PROXY_ADDRESS: &str = "127.0.0.1";

/// Default SOCKS5 proxy port (Tor default)
pub const DEFAULT_PROXY_PORT: u16 = 9050;

/// SOCKS5 proxy configuration
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ProxySettings {
    /// Whether to use a proxy
    #[serde(default)]
    pub enabled: bool,

    /// Proxy server address (hostname or IP)
    #[serde(default)]
    pub address: String,

    /// Proxy server port (default: 9050 for Tor)
    #[serde(default = "default_proxy_port")]
    pub port: u16,

    /// Optional username for authentication
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// Optional password for authentication
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl std::fmt::Debug for ProxySettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxySettings")
            .field("enabled", &self.enabled)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            address: DEFAULT_PROXY_ADDRESS.to_string(),
            port: DEFAULT_PROXY_PORT,
            username: None,
            password: None,
        }
    }
}

fn default_proxy_port() -> u16 {
    DEFAULT_PROXY_PORT
}

// =============================================================================
// Constants
// =============================================================================

/// Maximum avatar size in bytes (128KB)
pub const AVATAR_MAX_SIZE: usize = 128 * 1024;

/// Default value for queue_transfers setting
pub const DEFAULT_QUEUE_TRANSFERS: bool = false;

/// Default download limit per server (0 = unlimited)
pub const DEFAULT_DOWNLOAD_LIMIT: u8 = 2;

/// Default upload limit per server (0 = unlimited)
pub const DEFAULT_UPLOAD_LIMIT: u8 = 2;

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

    /// Download location for file transfers
    /// Defaults to system downloads directory if not set
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_path: Option<String>,

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

    /// Show hidden files (dotfiles) in file browser
    #[serde(default)]
    pub show_hidden_files: bool,

    /// Global toggle for desktop notifications
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,

    /// User avatar as data URI (e.g., "data:image/png;base64,...")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,

    /// Default nickname for shared account connections
    ///
    /// When connecting to a server with a shared account, this nickname will be used
    /// unless the bookmark specifies its own nickname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

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

    /// SOCKS5 proxy settings
    #[serde(default)]
    pub proxy: ProxySettings,

    /// Whether to queue transfers (limit concurrent transfers per server)
    #[serde(default = "default_queue_transfers", alias = "queue_downloads")]
    pub queue_transfers: bool,

    /// Maximum concurrent downloads per server (0 = unlimited)
    #[serde(default = "default_download_limit", alias = "max_concurrent_transfers")]
    pub download_limit: u8,

    /// Maximum concurrent uploads per server (0 = unlimited)
    #[serde(default = "default_upload_limit")]
    pub upload_limit: u8,

    /// Event notification settings
    #[serde(default)]
    pub event_settings: EventSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            download_path: None,
            chat_font_size: default_chat_font_size(),
            show_connection_notifications: default_true(),
            show_timestamps: default_true(),
            use_24_hour_time: false,
            show_seconds: default_true(),
            show_hidden_files: false,
            notifications_enabled: default_true(),
            avatar: None,
            nickname: None,
            window_width: default_window_width(),
            window_height: default_window_height(),
            window_x: None,
            window_y: None,
            proxy: ProxySettings::default(),
            queue_transfers: default_queue_transfers(),
            download_limit: default_download_limit(),
            upload_limit: default_upload_limit(),
            event_settings: EventSettings::default(),
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
            .field("download_path", &self.download_path)
            .field("chat_font_size", &self.chat_font_size)
            .field(
                "show_connection_notifications",
                &self.show_connection_notifications,
            )
            .field("show_timestamps", &self.show_timestamps)
            .field("use_24_hour_time", &self.use_24_hour_time)
            .field("show_seconds", &self.show_seconds)
            .field("show_hidden_files", &self.show_hidden_files)
            .field(
                "avatar",
                &self.avatar.as_ref().map(|a| format!("<{} bytes>", a.len())),
            )
            .field("nickname", &self.nickname)
            .field("proxy", &self.proxy)
            .finish()
    }
}

/// Get the default download directory path
///
/// Returns the system downloads directory, or None if it cannot be determined.
pub fn default_download_path() -> Option<String> {
    dirs::download_dir().map(|p| p.to_string_lossy().into_owned())
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

fn default_queue_transfers() -> bool {
    DEFAULT_QUEUE_TRANSFERS
}

fn default_download_limit() -> u8 {
    DEFAULT_DOWNLOAD_LIMIT
}

fn default_upload_limit() -> u8 {
    DEFAULT_UPLOAD_LIMIT
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
        assert!(settings.download_path.is_none());
        assert_eq!(settings.chat_font_size, CHAT_FONT_SIZE_DEFAULT);
        assert!(settings.show_connection_notifications);
        assert!(settings.show_timestamps);
        assert!(!settings.use_24_hour_time);
        assert!(settings.show_seconds);
        assert!(settings.avatar.is_none());
        assert!(settings.nickname.is_none());
        assert_eq!(settings.window_width, WINDOW_WIDTH);
        assert_eq!(settings.window_height, WINDOW_HEIGHT);
        assert!(settings.window_x.is_none());
        assert!(settings.window_y.is_none());
        assert!(!settings.queue_transfers);
        assert_eq!(settings.download_limit, DEFAULT_DOWNLOAD_LIMIT);
        assert_eq!(settings.upload_limit, DEFAULT_UPLOAD_LIMIT);
    }

    #[test]
    fn test_default_download_path() {
        // Just verify it doesn't panic - actual path depends on system
        let _path = default_download_path();
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

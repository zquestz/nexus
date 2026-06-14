//! User preference settings

use serde::Deserialize;

use crate::style::{WINDOW_HEIGHT, WINDOW_WIDTH};

use super::audio::AudioSettings;
use super::events::{EventSettings, EventType};
use super::theme::ThemePreference;

// =============================================================================
// Sound Settings
// =============================================================================

/// Default sound volume (0.0 - 1.0)
pub const DEFAULT_SOUND_VOLUME: f32 = 0.75;

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

    /// Allow voice to bypass the proxy (direct UDP). Off by default —
    /// when on, voice exposes the user's real IP to the server.
    #[serde(default)]
    pub allow_voice_bypass: bool,
}

impl std::fmt::Debug for ProxySettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxySettings")
            .field("enabled", &self.enabled)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("allow_voice_bypass", &self.allow_voice_bypass)
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
            allow_voice_bypass: false,
        }
    }
}

fn default_proxy_port() -> u16 {
    DEFAULT_PROXY_PORT
}

// =============================================================================
// Auto-Away Timeout
// =============================================================================

/// Auto-away timeout duration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AutoAwayTimeout {
    /// Disabled - no auto-away
    #[default]
    Off,
    /// 5 minutes
    Min5,
    /// 10 minutes
    Min10,
    /// 15 minutes
    Min15,
    /// 30 minutes
    Min30,
}

impl AutoAwayTimeout {
    /// All timeout options for the picker
    pub const ALL: &'static [AutoAwayTimeout] = &[
        AutoAwayTimeout::Off,
        AutoAwayTimeout::Min5,
        AutoAwayTimeout::Min10,
        AutoAwayTimeout::Min15,
        AutoAwayTimeout::Min30,
    ];

    /// Get the translation key for this timeout
    pub fn translation_key(self) -> &'static str {
        match self {
            AutoAwayTimeout::Off => "auto-away-off",
            AutoAwayTimeout::Min5 => "auto-away-5min",
            AutoAwayTimeout::Min10 => "auto-away-10min",
            AutoAwayTimeout::Min15 => "auto-away-15min",
            AutoAwayTimeout::Min30 => "auto-away-30min",
        }
    }

    /// Get the duration, or None if disabled
    pub fn as_duration(self) -> Option<std::time::Duration> {
        match self {
            AutoAwayTimeout::Off => None,
            AutoAwayTimeout::Min5 => Some(std::time::Duration::from_secs(5 * 60)),
            AutoAwayTimeout::Min10 => Some(std::time::Duration::from_secs(10 * 60)),
            AutoAwayTimeout::Min15 => Some(std::time::Duration::from_secs(15 * 60)),
            AutoAwayTimeout::Min30 => Some(std::time::Duration::from_secs(30 * 60)),
        }
    }

    /// Whether auto-away is enabled
    pub fn is_enabled(self) -> bool {
        self != AutoAwayTimeout::Off
    }
}

impl std::fmt::Display for AutoAwayTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
}

/// Default auto-away message
pub const DEFAULT_AUTO_AWAY_MESSAGE: &str = "AFK";

// =============================================================================
// Chat History Retention
// =============================================================================

/// Chat history retention policy for user message conversations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ChatHistoryRetention {
    /// Keep history forever (no automatic deletion)
    #[default]
    Forever,
    /// Keep messages for 30 days
    Days30,
    /// Keep messages for 14 days
    Days14,
    /// Keep messages for 7 days
    Days7,
    /// Don't save history at all
    Disabled,
}

impl ChatHistoryRetention {
    /// All retention options for the picker
    pub const ALL: &'static [ChatHistoryRetention] = &[
        ChatHistoryRetention::Forever,
        ChatHistoryRetention::Days30,
        ChatHistoryRetention::Days14,
        ChatHistoryRetention::Days7,
        ChatHistoryRetention::Disabled,
    ];

    /// Get the number of days for this retention policy, or None for Forever/Disabled
    pub fn days(&self) -> Option<u32> {
        match self {
            ChatHistoryRetention::Forever => None,
            ChatHistoryRetention::Days30 => Some(30),
            ChatHistoryRetention::Days14 => Some(14),
            ChatHistoryRetention::Days7 => Some(7),
            ChatHistoryRetention::Disabled => None,
        }
    }

    /// Whether history saving is enabled
    pub fn is_enabled(&self) -> bool {
        !matches!(self, ChatHistoryRetention::Disabled)
    }

    /// Get the translation key for this retention option
    pub fn translation_key(&self) -> &'static str {
        match self {
            ChatHistoryRetention::Forever => "chat-history-forever",
            ChatHistoryRetention::Days30 => "chat-history-30-days",
            ChatHistoryRetention::Days14 => "chat-history-14-days",
            ChatHistoryRetention::Days7 => "chat-history-7-days",
            ChatHistoryRetention::Disabled => "chat-history-disabled",
        }
    }
}

impl std::fmt::Display for ChatHistoryRetention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
}

/// GPU rendering backend selection (only used when hardware_rendering is enabled)
///
/// All variants exist on all platforms so that configs are portable across OSes.
/// The `ALL` array is platform-gated to control which options appear in the picker.
/// Use `is_available()` to check if a backend is valid on the current platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum GpuBackend {
    /// Let wgpu choose the best backend automatically
    #[default]
    Auto,
    /// DirectX 12 backend (Windows only)
    DX12,
    /// Metal backend (macOS/iOS only)
    Metal,
    /// OpenGL/GLES backend
    OpenGL,
    /// Vulkan backend
    Vulkan,
}

impl GpuBackend {
    /// All backend options for the current platform (alphabetical after Auto)
    #[cfg(target_os = "linux")]
    pub const ALL: &'static [GpuBackend] =
        &[GpuBackend::Auto, GpuBackend::OpenGL, GpuBackend::Vulkan];

    #[cfg(target_os = "windows")]
    pub const ALL: &'static [GpuBackend] = &[
        GpuBackend::Auto,
        GpuBackend::DX12,
        GpuBackend::OpenGL,
        GpuBackend::Vulkan,
    ];

    #[cfg(target_os = "macos")]
    pub const ALL: &'static [GpuBackend] = &[
        GpuBackend::Auto,
        GpuBackend::Metal,
        GpuBackend::OpenGL,
        GpuBackend::Vulkan,
    ];

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    pub const ALL: &'static [GpuBackend] =
        &[GpuBackend::Auto, GpuBackend::OpenGL, GpuBackend::Vulkan];

    /// Get the WGPU_BACKEND env var value for this backend, or None for Auto
    pub fn env_value(&self) -> Option<&'static str> {
        match self {
            GpuBackend::Auto => None,
            GpuBackend::DX12 => Some("dx12"),
            GpuBackend::Metal => Some("metal"),
            GpuBackend::OpenGL => Some("gl"),
            GpuBackend::Vulkan => Some("vulkan"),
        }
    }

    /// Whether this backend is available on the current platform
    pub fn is_available(&self) -> bool {
        match self {
            GpuBackend::Auto | GpuBackend::OpenGL | GpuBackend::Vulkan => true,
            #[cfg(target_os = "macos")]
            GpuBackend::Metal => true,
            #[cfg(not(target_os = "macos"))]
            GpuBackend::Metal => false,
            #[cfg(target_os = "windows")]
            GpuBackend::DX12 => true,
            #[cfg(not(target_os = "windows"))]
            GpuBackend::DX12 => false,
        }
    }

    /// Get the translation key for this backend option
    pub fn translation_key(&self) -> &'static str {
        match self {
            GpuBackend::Auto => "gpu-backend-auto",
            GpuBackend::DX12 => "gpu-backend-dx12",
            GpuBackend::Metal => "gpu-backend-metal",
            GpuBackend::OpenGL => "gpu-backend-opengl",
            GpuBackend::Vulkan => "gpu-backend-vulkan",
        }
    }
}

impl std::fmt::Display for GpuBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
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

/// Default maximum scrollback lines per chat tab
pub const DEFAULT_MAX_SCROLLBACK: usize = 5000;

/// Minimum allowed chat font size
pub const CHAT_FONT_SIZE_MIN: u8 = 9;

/// Maximum allowed chat font size
pub const CHAT_FONT_SIZE_MAX: u8 = 16;

/// Minimum sound volume
pub const SOUND_VOLUME_MIN: f32 = 0.0;

/// Maximum sound volume
pub const SOUND_VOLUME_MAX: f32 = 1.0;

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

    /// Show user connect/disconnect events in chat
    #[serde(default = "default_true")]
    pub show_connection_events: bool,

    /// Show channel join/leave events in chat
    #[serde(default = "default_true")]
    pub show_join_leave_events: bool,

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

    /// Global toggle for sound notifications
    #[serde(default = "default_true")]
    pub sound_enabled: bool,

    /// Master volume for sounds (0.0 - 1.0)
    #[serde(default = "default_sound_volume")]
    pub sound_volume: f32,

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

    /// Last selected event type in Settings > Events tab
    #[serde(default, deserialize_with = "deserialize_event_type_or_default")]
    pub selected_event_type: EventType,

    /// Maximum scrollback lines per chat tab (0 = unlimited)
    #[serde(default = "default_max_scrollback")]
    pub max_scrollback: usize,

    /// Chat history retention policy for user message conversations
    #[serde(default)]
    pub chat_history_retention: ChatHistoryRetention,

    /// Audio settings for voice chat
    #[serde(default)]
    pub audio: AudioSettings,

    /// Show system tray icon (Windows/Linux only)
    #[serde(default)]
    pub show_tray_icon: bool,

    /// Minimize to tray instead of closing (Windows/Linux only)
    #[serde(default)]
    pub minimize_to_tray: bool,

    /// Enable hardware (GPU) rendering instead of software rendering (requires restart)
    #[serde(default)]
    pub hardware_rendering: bool,

    /// GPU rendering backend (requires restart, only used when hardware_rendering is enabled)
    #[serde(default)]
    pub gpu_backend: GpuBackend,

    /// Auto-away timeout (how long before automatically setting away)
    #[serde(default)]
    pub auto_away_timeout: AutoAwayTimeout,

    /// Auto-away status message (sent when auto-away triggers)
    #[serde(default = "default_auto_away_message")]
    pub auto_away_message: String,
}

/// Default value for max_scrollback setting
const fn default_max_scrollback() -> usize {
    DEFAULT_MAX_SCROLLBACK
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::default(),
            download_path: None,
            chat_font_size: default_chat_font_size(),
            show_connection_events: default_true(),
            show_join_leave_events: default_true(),
            show_timestamps: default_true(),
            use_24_hour_time: false,
            show_seconds: default_true(),
            show_hidden_files: false,
            notifications_enabled: default_true(),
            sound_enabled: default_true(),
            sound_volume: default_sound_volume(),
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
            selected_event_type: EventType::default(),
            max_scrollback: default_max_scrollback(),
            chat_history_retention: ChatHistoryRetention::default(),
            audio: AudioSettings::default(),
            show_tray_icon: false,
            minimize_to_tray: false,
            hardware_rendering: false,
            gpu_backend: GpuBackend::default(),
            auto_away_timeout: AutoAwayTimeout::default(),
            auto_away_message: default_auto_away_message(),
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
            .field("show_connection_events", &self.show_connection_events)
            .field("show_join_leave_events", &self.show_join_leave_events)
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
            .field("max_scrollback", &self.max_scrollback)
            .field("chat_history_retention", &self.chat_history_retention)
            .field("audio", &self.audio)
            .field("hardware_rendering", &self.hardware_rendering)
            .field("gpu_backend", &self.gpu_backend)
            .field("auto_away_timeout", &self.auto_away_timeout)
            .field("auto_away_message", &self.auto_away_message)
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

fn default_sound_volume() -> f32 {
    DEFAULT_SOUND_VOLUME
}

fn default_auto_away_message() -> String {
    DEFAULT_AUTO_AWAY_MESSAGE.to_string()
}

fn deserialize_event_type_or_default<'de, D>(deserializer: D) -> Result<EventType, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
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
        assert!(settings.show_connection_events);
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
            settings.show_connection_events,
            deserialized.show_connection_events
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

    #[test]
    fn test_unknown_selected_event_type_defaults() {
        let json = r#"{
            "selected_event_type": "future_event"
        }"#;

        let settings: Settings = serde_json::from_str(json).expect("deserialize");

        assert_eq!(settings.selected_event_type, EventType::Broadcast);
    }
}

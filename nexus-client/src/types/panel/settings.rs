//! Settings panel form state

use crate::config::Config;
use crate::config::events::EventType;

use crate::avatar::generate_identicon;
use crate::image::{CachedImage, decode_data_uri_square};
use crate::style::AVATAR_MAX_CACHE_SIZE;
use nexus_common::validators::ImageDecodeProfile;

// =============================================================================
// Settings Tab
// =============================================================================

/// Settings panel tab identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// General settings (theme, avatar, nickname)
    #[default]
    General,
    /// Chat settings (font size, timestamps, notifications)
    Chat,
    /// Network settings (proxy configuration)
    Network,
    /// Files settings (download location)
    Files,
    /// Event notification settings
    Events,
    /// Audio settings for voice chat
    Audio,
}

// =============================================================================
// Settings Form State
// =============================================================================

/// Settings panel form state
///
/// Stores a snapshot of the configuration when the settings panel is opened,
/// allowing the user to cancel and restore the original settings.
#[derive(Clone)]
pub struct SettingsFormState {
    /// Currently active settings tab
    pub active_tab: SettingsTab,
    /// Original config snapshot to restore on cancel
    pub original_config: Config,
    /// Error message to display (e.g., avatar load failure)
    pub error: Option<String>,
    /// Cached avatar for stable rendering (decoded from config.settings.avatar)
    pub cached_avatar: Option<CachedImage>,
    /// Default avatar for settings preview when no custom avatar is set
    pub default_avatar: CachedImage,
    /// Currently selected event type in Events tab
    pub selected_event_type: EventType,
    /// Whether PTT key capture mode is active
    pub ptt_capturing: bool,
    /// Whether microphone test is active
    pub mic_testing: bool,
    /// Current microphone input level (0.0 - 1.0)
    pub mic_level: f32,
    /// Microphone test error message (e.g., device initialization failure)
    pub mic_error: Option<String>,
    /// Cached output audio devices (populated once when settings opens)
    pub output_devices: Vec<crate::voice::audio::AudioDevice>,
    /// Cached input audio devices (populated once when settings opens)
    pub input_devices: Vec<crate::voice::audio::AudioDevice>,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for SettingsFormState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsFormState")
            .field("original_config", &self.original_config)
            .field("error", &self.error)
            .field(
                "cached_avatar",
                &self.cached_avatar.as_ref().map(|_| "<cached>"),
            )
            .field("default_avatar", &"<cached>")
            .field("selected_event_type", &self.selected_event_type)
            .field("ptt_capturing", &self.ptt_capturing)
            .field("mic_testing", &self.mic_testing)
            .field("mic_level", &self.mic_level)
            .field("mic_error", &self.mic_error)
            .field("output_devices", &self.output_devices.len())
            .field("input_devices", &self.input_devices.len())
            .finish()
    }
}

impl SettingsFormState {
    /// Create a new settings form state with a snapshot of the current config
    ///
    /// The `last_tab` parameter restores the previously selected tab when reopening the panel.
    /// The `last_event_type` parameter restores the previously selected event type in the Events tab.
    pub fn new(config: &Config, last_tab: SettingsTab, last_event_type: EventType) -> Self {
        // Cache audio device lists once when settings opens (avoids ALSA spam on every frame)
        let output_devices = crate::voice::audio::list_output_devices();
        let input_devices = crate::voice::audio::list_input_devices();

        Self::with_devices(
            config,
            last_tab,
            last_event_type,
            output_devices,
            input_devices,
        )
    }

    /// [`Self::new`] with the audio device lists injected instead of scanned.
    ///
    /// Unit tests MUST use this (with empty lists): device enumeration goes
    /// through cpal — WASAPI/COM on Windows — and two tests hitting that
    /// native first-init concurrently on libtest worker threads intermittently
    /// crashed the whole test process with STATUS_ACCESS_VIOLATION on Windows
    /// CI. Tests have no business enumerating hardware anyway.
    pub fn with_devices(
        config: &Config,
        last_tab: SettingsTab,
        last_event_type: EventType,
        output_devices: Vec<crate::voice::audio::AudioDevice>,
        input_devices: Vec<crate::voice::audio::AudioDevice>,
    ) -> Self {
        // Decode avatar from config if present
        let cached_avatar = config.settings.avatar.as_ref().and_then(|data_uri| {
            decode_data_uri_square(data_uri, AVATAR_MAX_CACHE_SIZE, ImageDecodeProfile::Avatar)
        });
        // Generate default avatar for settings preview
        let default_avatar = generate_identicon("default");

        Self {
            active_tab: last_tab,
            original_config: config.clone(),
            error: None,
            cached_avatar,
            default_avatar,
            selected_event_type: last_event_type,
            ptt_capturing: false,
            mic_testing: false,
            mic_level: 0.0,
            mic_error: None,
            output_devices,
            input_devices,
        }
    }
}

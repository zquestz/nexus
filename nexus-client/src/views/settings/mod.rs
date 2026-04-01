//! Settings panel view
//!
//! Each tab is in its own sub-module for maintainability.

mod audio;
mod chat;
mod events;
mod files;
mod general;
mod network;

use iced::widget::{Column, Space, button, button as btn, container, row, scrollable};
use iced::{Center, Element, Fill, Theme};
use iced_aw::TabLabel;
use iced_aw::Tabs;
use nexus_common::voice::VoiceQuality;

use crate::config::audio::{PttMode, PttReleaseDelay};
use crate::config::events::{EventSettings, EventType};
use crate::config::settings::{ChatHistoryRetention, ProxySettings};
use crate::i18n::t;
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TAB_LABEL_PADDING, TEXT_SIZE, content_background_style, error_text_style,
    panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::{Message, SettingsFormState, SettingsTab};
use crate::voice::audio::AudioDevice;

use super::chat::TimestampSettings;

/// Data needed to render the audio settings tab
pub struct AudioTabData<'a> {
    /// Available output audio devices
    pub output_devices: &'a [AudioDevice],
    /// Currently selected output device
    pub selected_output_device: AudioDevice,
    /// Available input audio devices
    pub input_devices: &'a [AudioDevice],
    /// Currently selected input device
    pub selected_input_device: AudioDevice,
    /// Current voice quality setting
    pub voice_quality: VoiceQuality,
    /// Current PTT key binding
    pub ptt_key: &'a str,
    /// Whether PTT key capture mode is active
    pub ptt_capturing: bool,
    /// Current PTT mode (hold or toggle)
    pub ptt_mode: PttMode,
    /// Current PTT release delay
    pub ptt_release_delay: PttReleaseDelay,
    /// Whether microphone test is active
    pub mic_testing: bool,
    /// Microphone level from settings mic test (0.0 - 1.0)
    /// Separate from voice bar mic_level so voice PTT doesn't bleed into the settings meter.
    pub settings_mic_level: f32,
    /// Microphone test error message
    pub mic_error: Option<&'a str>,
    /// Noise suppression level
    pub noise_suppression_level: crate::config::audio::NoiseSuppressionLevel,
    /// Echo cancellation enabled
    pub echo_cancellation: bool,
    /// Automatic gain control enabled
    pub agc: bool,
    /// Transient suppression (keyboard noise reduction) enabled
    pub transient_suppression: bool,
    /// Microphone boost level
    pub mic_boost: crate::config::audio::MicBoost,
    /// Current theme (for VU meter rendering)
    pub theme: Theme,
}

/// All data needed to render the settings panel
pub struct SettingsViewData<'a> {
    /// Current theme
    pub current_theme: Theme,
    /// Whether connection events are shown in chat
    pub show_connection_events: bool,
    /// Whether channel join/leave events are shown in chat
    pub show_join_leave_events: bool,
    /// Chat history retention setting
    pub chat_history_retention: ChatHistoryRetention,
    /// Max scrollback messages per tab
    pub max_scrollback: usize,
    /// Chat font size
    pub chat_font_size: u8,
    /// Timestamp display settings
    pub timestamp_settings: TimestampSettings,
    /// Settings form state (present when panel is open)
    pub settings_form: Option<&'a SettingsFormState>,
    /// Default nickname for shared accounts
    pub nickname: &'a str,
    /// Proxy settings
    pub proxy: &'a ProxySettings,
    /// Download path override (None = system default)
    pub download_path: Option<&'a str>,
    /// Whether transfer queuing is enabled
    pub queue_transfers: bool,
    /// Max concurrent downloads per server (0 = unlimited)
    pub download_limit: u8,
    /// Max concurrent uploads per server (0 = unlimited)
    pub upload_limit: u8,
    /// Event notification settings
    pub event_settings: &'a EventSettings,
    /// Currently selected event type in Events tab
    pub selected_event_type: EventType,
    /// Global notifications enabled
    pub notifications_enabled: bool,
    /// Global sound enabled
    pub sound_enabled: bool,
    /// Master sound volume (0.0 - 1.0)
    pub sound_volume: f32,
    /// Available output audio devices
    pub output_devices: &'a [AudioDevice],
    /// Currently selected output device
    pub selected_output_device: AudioDevice,
    /// Available input audio devices
    pub input_devices: &'a [AudioDevice],
    /// Currently selected input device
    pub selected_input_device: AudioDevice,
    /// Current voice quality setting
    pub voice_quality: VoiceQuality,
    /// Current PTT key binding
    pub ptt_key: &'a str,
    /// Whether PTT key capture mode is active
    pub ptt_capturing: bool,
    /// Current PTT mode (hold or toggle)
    pub ptt_mode: PttMode,
    /// Current PTT release delay
    pub ptt_release_delay: PttReleaseDelay,
    /// Whether microphone test is active
    pub mic_testing: bool,
    /// Microphone level from settings mic test (0.0 - 1.0)
    pub settings_mic_level: f32,
    /// Microphone test error message
    pub mic_error: Option<&'a str>,
    /// Noise suppression level
    pub noise_suppression_level: crate::config::audio::NoiseSuppressionLevel,
    /// Echo cancellation enabled
    pub echo_cancellation: bool,
    /// Automatic gain control enabled
    pub agc: bool,
    /// Transient suppression (keyboard noise reduction) enabled
    pub transient_suppression: bool,
    /// Microphone boost level
    pub mic_boost: crate::config::audio::MicBoost,
    /// Whether to show tray icon setting (Windows/Linux only)
    pub show_tray_icon: bool,
    /// Whether to minimize to tray setting (Windows/Linux only)
    pub minimize_to_tray: bool,
    /// Whether hardware (GPU) rendering is enabled
    pub hardware_rendering: bool,
    /// GPU rendering backend selection
    pub gpu_backend: crate::config::settings::GpuBackend,
    /// Auto-away timeout setting
    pub auto_away_timeout: crate::config::settings::AutoAwayTimeout,
    /// Auto-away message
    pub auto_away_message: &'a str,
}

// ============================================================================
// Settings View
// ============================================================================

/// Render the settings panel with tabbed layout
///
/// Shows application settings organized into tabs:
/// - General: Theme, avatar, nickname
/// - Chat: Font size, timestamps, notifications
/// - Files: Download location
/// - Network: Proxy configuration
/// - Events: Notification, toast, and sound settings per event
/// - Audio: Voice chat devices and push-to-talk settings
///
/// Cancel restores original settings, Save persists changes.
pub fn settings_view<'a>(data: SettingsViewData<'a>) -> Element<'a, Message> {
    // Extract state from settings form (only present when panel is open)
    let (avatar, default_avatar, error, active_tab): (
        Option<&CachedImage>,
        Option<&CachedImage>,
        Option<&str>,
        SettingsTab,
    ) = data
        .settings_form
        .map(|f| {
            (
                f.cached_avatar.as_ref(),
                Some(&f.default_avatar),
                f.error.as_deref(),
                f.active_tab,
            )
        })
        .unwrap_or((None, None, None, SettingsTab::General));

    // Build tab content
    let theme = data.current_theme.clone();
    let general_content = general::general_tab_content(general::GeneralTabData {
        current_theme: data.current_theme,
        avatar,
        default_avatar,
        nickname: data.nickname,
        show_tray_icon: data.show_tray_icon,
        minimize_to_tray: data.minimize_to_tray,
        hardware_rendering: data.hardware_rendering,
        gpu_backend: data.gpu_backend,
    });
    let chat_content = chat::chat_tab_content(chat::ChatTabData {
        chat_history_retention: data.chat_history_retention,
        max_scrollback: data.max_scrollback,
        chat_font_size: data.chat_font_size,
        show_connection_events: data.show_connection_events,
        show_join_leave_events: data.show_join_leave_events,
        timestamp_settings: data.timestamp_settings,
        auto_away_timeout: data.auto_away_timeout,
        auto_away_message: data.auto_away_message,
    });
    let network_content = network::network_tab_content(data.proxy);

    let files_content = files::files_tab_content(
        data.download_path,
        data.queue_transfers,
        data.download_limit,
        data.upload_limit,
    );

    let events_content = events::events_tab_content(
        data.event_settings,
        data.selected_event_type,
        data.notifications_enabled,
        data.sound_enabled,
        data.sound_volume,
    );

    let audio_content = audio::audio_tab_content(AudioTabData {
        output_devices: data.output_devices,
        selected_output_device: data.selected_output_device.clone(),
        input_devices: data.input_devices,
        selected_input_device: data.selected_input_device.clone(),
        voice_quality: data.voice_quality,
        ptt_key: data.ptt_key,
        ptt_capturing: data.ptt_capturing,
        ptt_mode: data.ptt_mode,
        ptt_release_delay: data.ptt_release_delay,
        mic_testing: data.mic_testing,
        settings_mic_level: data.settings_mic_level,
        mic_error: data.mic_error,
        noise_suppression_level: data.noise_suppression_level,
        echo_cancellation: data.echo_cancellation,
        agc: data.agc,
        transient_suppression: data.transient_suppression,
        mic_boost: data.mic_boost,
        theme,
    });

    // Create tabs widget with compact styling
    let tabs = Tabs::new(Message::SettingsTabSelected)
        .push(
            SettingsTab::General,
            TabLabel::Text(t("tab-general")),
            general_content,
        )
        .push(
            SettingsTab::Chat,
            TabLabel::Text(t("tab-chat")),
            chat_content,
        )
        .push(
            SettingsTab::Files,
            TabLabel::Text(t("tab-files")),
            files_content,
        )
        .push(
            SettingsTab::Network,
            TabLabel::Text(t("tab-network")),
            network_content,
        )
        .push(
            SettingsTab::Events,
            TabLabel::Text(t("settings-tab-events")),
            events_content,
        )
        .push(
            SettingsTab::Audio,
            TabLabel::Text(t("tab-audio")),
            audio_content,
        )
        .set_active_tab(&active_tab)
        .tab_bar_position(iced_aw::TabBarPosition::Top)
        .text_size(TEXT_SIZE)
        .tab_label_padding(TAB_LABEL_PADDING);

    // Buttons row
    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::CancelSettings)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::SaveSettings)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    // Build the form
    let mut content_items: Vec<Element<'_, Message>> = Vec::new();

    content_items.push(panel_title(t("title-settings")).into());

    // Show error if present
    if let Some(error) = error {
        content_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        content_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        content_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    }

    content_items.push(tabs.into());
    content_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    content_items.push(buttons.into());

    let content = Column::with_children(content_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    // Scrollable, top-aligned, with background
    let scrollable_content = scrollable(container(content).width(Fill).center_x(Fill)).width(Fill);

    container(scrollable_content)
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}

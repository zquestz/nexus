//! Settings panel view

use iced::widget::button as btn;
use iced::widget::{
    Column, Id, Space, button, checkbox, container, pick_list, row, scrollable, slider, text_input,
};
use iced::{Center, Element, Fill, Theme};
use iced_aw::NumberInput;
use iced_aw::TabLabel;
use iced_aw::Tabs;

use super::chat::TimestampSettings;
use crate::config::events::{EventSettings, EventType, NotificationContent, SoundChoice};
use crate::config::settings::{
    CHAT_FONT_SIZES, ProxySettings, SOUND_VOLUME_MAX, SOUND_VOLUME_MIN, default_download_path,
};
use crate::config::theme::all_themes;
use crate::i18n::t;
use crate::image::CachedImage;
use crate::style::SPACER_SIZE_LARGE;
use crate::style::{
    AVATAR_PREVIEW_SIZE, BUTTON_PADDING, CHECKBOX_INDENT, CONTENT_MAX_WIDTH, CONTENT_PADDING,
    ELEMENT_SPACING, INPUT_PADDING, PATH_DISPLAY_PADDING, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TAB_LABEL_PADDING, TEXT_SIZE, content_background_style, error_text_style, panel_title,
    shaped_text, shaped_text_wrapped,
};
use crate::types::{InputId, Message, SettingsFormState, SettingsTab};

// ============================================================================
// Settings View Data
// ============================================================================

/// Data needed to render the settings panel
pub struct SettingsViewData<'a> {
    /// Current theme for styling
    pub current_theme: Theme,
    /// Show user connect/disconnect events in chat
    pub show_connection_events: bool,
    /// Show channel join/leave events in chat
    pub show_join_leave_events: bool,
    /// Font size for chat messages
    pub chat_font_size: u8,
    /// Timestamp display settings
    pub timestamp_settings: TimestampSettings,
    /// Settings form state (present when panel is open)
    pub settings_form: Option<&'a SettingsFormState>,
    /// Default nickname for shared accounts
    pub nickname: &'a str,
    /// SOCKS5 proxy settings
    pub proxy: &'a ProxySettings,
    /// Download path for file transfers
    pub download_path: Option<&'a str>,
    /// Whether to queue transfers (limit concurrent transfers per server)
    pub queue_transfers: bool,
    /// Maximum concurrent downloads per server (0 = unlimited)
    pub download_limit: u8,
    /// Maximum concurrent uploads per server (0 = unlimited)
    pub upload_limit: u8,
    /// Event notification settings
    pub event_settings: &'a EventSettings,
    /// Currently selected event type in the Events tab
    pub selected_event_type: EventType,
    /// Global toggle for desktop notifications
    pub notifications_enabled: bool,
    /// Global toggle for sound notifications
    pub sound_enabled: bool,
    /// Master volume for sounds (0.0 - 1.0)
    pub sound_volume: f32,
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
    let general_content =
        general_tab_content(data.current_theme, avatar, default_avatar, data.nickname);
    let chat_content = chat_tab_content(
        data.chat_font_size,
        data.show_connection_events,
        data.show_join_leave_events,
        data.timestamp_settings,
    );
    let network_content = network_tab_content(data.proxy);

    let files_content = files_tab_content(
        data.download_path,
        data.queue_transfers,
        data.download_limit,
        data.upload_limit,
    );

    let events_content = events_tab_content(
        data.event_settings,
        data.selected_event_type,
        data.notifications_enabled,
        data.sound_enabled,
        data.sound_volume,
    );

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

// ============================================================================
// Tab Content Builders
// ============================================================================

/// Build the General tab content (theme, avatar, nickname)
fn general_tab_content<'a>(
    current_theme: Theme,
    avatar: Option<&'a crate::image::CachedImage>,
    default_avatar: Option<&'a crate::image::CachedImage>,
    nickname: &'a str,
) -> Element<'a, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Theme picker row
    let theme_label = shaped_text(t("label-theme")).size(TEXT_SIZE);
    let theme_picker =
        pick_list(all_themes(), Some(current_theme), Message::ThemeSelected).text_size(TEXT_SIZE);
    let theme_row = row![theme_label, theme_picker]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(theme_row.into());

    // Avatar section
    let avatar_preview: Element<'_, Message> = if let Some(av) = avatar {
        av.render(AVATAR_PREVIEW_SIZE)
    } else if let Some(default) = default_avatar {
        default.render(AVATAR_PREVIEW_SIZE)
    } else {
        Space::new()
            .width(AVATAR_PREVIEW_SIZE)
            .height(AVATAR_PREVIEW_SIZE)
            .into()
    };

    let pick_avatar_button = button(shaped_text(t("button-choose-avatar")).size(TEXT_SIZE))
        .on_press(Message::PickAvatarPressed)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let clear_avatar_button = if avatar.is_some() {
        button(shaped_text(t("button-clear-avatar")).size(TEXT_SIZE))
            .on_press(Message::ClearAvatarPressed)
            .padding(BUTTON_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("button-clear-avatar")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::secondary)
    };

    let avatar_buttons = row![pick_avatar_button, clear_avatar_button].spacing(ELEMENT_SPACING);
    let avatar_row = row![avatar_preview, avatar_buttons]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(avatar_row.into());

    // Nickname input (placeholder guides the user)
    let nickname_input = text_input(&t("placeholder-nickname-optional"), nickname)
        .on_input(Message::SettingsNicknameChanged)
        .on_submit(Message::SaveSettings)
        .id(Id::from(InputId::SettingsNickname))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);
    items.push(nickname_input.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

/// Build the Chat tab content (font size, notifications, timestamps)
fn chat_tab_content(
    chat_font_size: u8,
    show_connection_events: bool,
    show_join_leave_events: bool,
    timestamp_settings: TimestampSettings,
) -> Element<'static, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Chat font size picker row
    let font_size_label = shaped_text(t("label-chat-font-size")).size(TEXT_SIZE);
    let font_size_picker = pick_list(
        CHAT_FONT_SIZES,
        Some(chat_font_size),
        Message::ChatFontSizeSelected,
    )
    .text_size(TEXT_SIZE);
    let font_size_row = row![font_size_label, font_size_picker]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(font_size_row.into());

    // Connection events checkbox
    let connection_events_checkbox = checkbox(show_connection_events)
        .label(t("label-show-connection-events"))
        .on_toggle(Message::ConnectionNotificationsToggled)
        .text_size(TEXT_SIZE);
    items.push(connection_events_checkbox.into());

    // Channel join/leave events checkbox
    let join_leave_events_checkbox = checkbox(show_join_leave_events)
        .label(t("label-show-channel-events"))
        .on_toggle(Message::ChannelNotificationsToggled)
        .text_size(TEXT_SIZE);
    items.push(join_leave_events_checkbox.into());

    // Timestamp settings
    let timestamps_checkbox = checkbox(timestamp_settings.show_timestamps)
        .label(t("label-show-timestamps"))
        .on_toggle(Message::ShowTimestampsToggled)
        .text_size(TEXT_SIZE);
    items.push(timestamps_checkbox.into());

    // 24-hour time checkbox (disabled if timestamps are hidden, indented)
    let time_format_checkbox = if timestamp_settings.show_timestamps {
        checkbox(timestamp_settings.use_24_hour_time)
            .label(t("label-use-24-hour-time"))
            .on_toggle(Message::Use24HourTimeToggled)
            .text_size(TEXT_SIZE)
    } else {
        checkbox(timestamp_settings.use_24_hour_time)
            .label(t("label-use-24-hour-time"))
            .text_size(TEXT_SIZE)
    };
    let time_format_row = row![Space::new().width(CHECKBOX_INDENT), time_format_checkbox];
    items.push(time_format_row.into());

    // Show seconds checkbox (disabled if timestamps are hidden, indented)
    let seconds_checkbox = if timestamp_settings.show_timestamps {
        checkbox(timestamp_settings.show_seconds)
            .label(t("label-show-seconds"))
            .on_toggle(Message::ShowSecondsToggled)
            .text_size(TEXT_SIZE)
    } else {
        checkbox(timestamp_settings.show_seconds)
            .label(t("label-show-seconds"))
            .text_size(TEXT_SIZE)
    };
    let seconds_row = row![Space::new().width(CHECKBOX_INDENT), seconds_checkbox];
    items.push(seconds_row.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

/// Build the Files tab content (download location, transfer queue settings)
fn files_tab_content(
    download_path: Option<&str>,
    queue_transfers: bool,
    download_limit: u8,
    upload_limit: u8,
) -> Element<'static, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Download location label
    let download_label = shaped_text(t("label-download-location")).size(TEXT_SIZE);
    items.push(download_label.into());

    // Get current download path (from live config or system default)
    // Only call default_download_path() if no path is configured
    let path_display = match download_path {
        Some(path) => path.to_string(),
        None => default_download_path().unwrap_or_else(|| t("placeholder-download-location")),
    };

    // Path display (read-only style, no left padding to align with label) with Browse button
    let path_text = shaped_text(path_display).size(TEXT_SIZE);
    let path_container = container(path_text)
        .padding(PATH_DISPLAY_PADDING)
        .width(Fill);

    let browse_button = button(shaped_text(t("button-browse")).size(TEXT_SIZE))
        .on_press(Message::BrowseDownloadPathPressed)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let path_row = row![path_container, browse_button]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(path_row.into());

    // Spacer before queue settings
    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Queue transfers checkbox
    let queue_checkbox = checkbox(queue_transfers)
        .label(t("label-queue-transfers"))
        .on_toggle(Message::QueueTransfersToggled)
        .text_size(TEXT_SIZE);
    items.push(queue_checkbox.into());

    // Download limit (disabled when queue_transfers is off)
    let download_limit_label = shaped_text(t("label-download-limit")).size(TEXT_SIZE);
    let download_limit_input: Element<'_, Message> = if queue_transfers {
        NumberInput::new(&download_limit, 0..=u8::MAX, Message::DownloadLimitChanged)
            .padding(INPUT_PADDING)
            .into()
    } else {
        NumberInput::new(&download_limit, 0..=u8::MAX, Message::DownloadLimitChanged)
            .on_input_maybe(None::<fn(u8) -> Message>)
            .padding(INPUT_PADDING)
            .into()
    };
    let download_limit_row = row![download_limit_label, download_limit_input]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(download_limit_row.into());

    // Upload limit (disabled when queue_transfers is off)
    let upload_limit_label = shaped_text(t("label-upload-limit")).size(TEXT_SIZE);
    let upload_limit_input: Element<'_, Message> = if queue_transfers {
        NumberInput::new(&upload_limit, 0..=u8::MAX, Message::UploadLimitChanged)
            .padding(INPUT_PADDING)
            .into()
    } else {
        NumberInput::new(&upload_limit, 0..=u8::MAX, Message::UploadLimitChanged)
            .on_input_maybe(None::<fn(u8) -> Message>)
            .padding(INPUT_PADDING)
            .into()
    };
    let upload_limit_row = row![upload_limit_label, upload_limit_input]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(upload_limit_row.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

/// Build the Network tab content (proxy configuration)
fn network_tab_content(proxy: &ProxySettings) -> Element<'_, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Proxy enabled checkbox
    let proxy_enabled_checkbox = checkbox(proxy.enabled)
        .label(t("label-use-socks5-proxy"))
        .on_toggle(Message::ProxyEnabledToggled)
        .text_size(TEXT_SIZE);
    items.push(proxy_enabled_checkbox.into());

    // Proxy address input (disabled when proxy is disabled)
    let proxy_address_input = if proxy.enabled {
        text_input(&t("placeholder-proxy-address"), &proxy.address)
            .on_input(Message::ProxyAddressChanged)
            .on_submit(Message::SaveSettings)
            .id(Id::from(InputId::ProxyAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    } else {
        text_input(&t("placeholder-proxy-address"), &proxy.address)
            .id(Id::from(InputId::ProxyAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    };
    items.push(proxy_address_input.into());

    // Proxy port input with label (disabled when proxy is disabled)
    let proxy_port_label = shaped_text(t("label-proxy-port")).size(TEXT_SIZE);
    let proxy_port_input: Element<'_, Message> = if proxy.enabled {
        NumberInput::new(&proxy.port, 1..=65535, Message::ProxyPortChanged)
            .id(Id::from(InputId::ProxyPort))
            .padding(INPUT_PADDING)
            .into()
    } else {
        NumberInput::new(&proxy.port, 1..=65535, Message::ProxyPortChanged)
            .on_input_maybe(None::<fn(u16) -> Message>)
            .id(Id::from(InputId::ProxyPort))
            .padding(INPUT_PADDING)
            .into()
    };
    let proxy_port_row = row![proxy_port_label, proxy_port_input]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    items.push(proxy_port_row.into());

    // Proxy username input (optional, disabled when proxy is disabled)
    let proxy_username_value = proxy.username.as_deref().unwrap_or("");
    let proxy_username_input = if proxy.enabled {
        text_input(&t("placeholder-proxy-username"), proxy_username_value)
            .on_input(Message::ProxyUsernameChanged)
            .on_submit(Message::SaveSettings)
            .id(Id::from(InputId::ProxyUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    } else {
        text_input(&t("placeholder-proxy-username"), proxy_username_value)
            .id(Id::from(InputId::ProxyUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    };
    items.push(proxy_username_input.into());

    // Proxy password input (optional, disabled when proxy is disabled)
    let proxy_password_value = proxy.password.as_deref().unwrap_or("");
    let proxy_password_input = if proxy.enabled {
        text_input(&t("placeholder-proxy-password"), proxy_password_value)
            .on_input(Message::ProxyPasswordChanged)
            .on_submit(Message::SaveSettings)
            .id(Id::from(InputId::ProxyPassword))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .secure(true)
    } else {
        text_input(&t("placeholder-proxy-password"), proxy_password_value)
            .id(Id::from(InputId::ProxyPassword))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .secure(true)
    };
    items.push(proxy_password_input.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

// ============================================================================
// Events Tab
// ============================================================================

/// Build the events tab content
fn events_tab_content<'a>(
    event_settings: &'a EventSettings,
    selected_event_type: EventType,
    notifications_enabled: bool,
    sound_enabled: bool,
    sound_volume: f32,
) -> Element<'a, Message> {
    let mut items: Vec<Element<'_, Message>> = Vec::new();

    // Space between tab bar and first content
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Global toggles for notifications and sound
    let notifications_checkbox = checkbox(notifications_enabled)
        .label(t("settings-notifications-enabled"))
        .on_toggle(Message::ToggleNotificationsEnabled)
        .text_size(TEXT_SIZE)
        .spacing(ELEMENT_SPACING);

    let sound_checkbox = checkbox(sound_enabled)
        .label(t("settings-sound-enabled"))
        .on_toggle(Message::ToggleSoundEnabled)
        .text_size(TEXT_SIZE)
        .spacing(ELEMENT_SPACING);

    let global_toggles_row = row![
        notifications_checkbox,
        Space::new().width(SPACER_SIZE_LARGE),
        sound_checkbox,
    ]
    .align_y(iced::Alignment::Center);
    items.push(global_toggles_row.into());

    items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Volume slider with label and percentage
    let volume_percent = (sound_volume * 100.0).round() as u8;
    let volume_label = shaped_text(t("settings-sound-volume")).size(TEXT_SIZE);
    let volume_value = shaped_text(format!("{}%", volume_percent)).size(TEXT_SIZE);

    // Create the slider (always interactive - disable is handled at handler level)
    let volume_slider = slider(
        SOUND_VOLUME_MIN..=SOUND_VOLUME_MAX,
        sound_volume,
        Message::SoundVolumeChanged,
    )
    .step(0.01);

    let volume_row = row![
        volume_label,
        Space::new().width(ELEMENT_SPACING),
        volume_slider,
        Space::new().width(ELEMENT_SPACING),
        volume_value,
    ]
    .spacing(ELEMENT_SPACING)
    .align_y(iced::Alignment::Center);

    items.push(volume_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Event type picker with label on same row
    let event_label = shaped_text(t("event-settings-event")).size(TEXT_SIZE);
    let event_types: Vec<EventType> = EventType::all().to_vec();
    let event_picker = pick_list(
        event_types,
        Some(selected_event_type),
        Message::EventTypeSelected,
    )
    .text_size(TEXT_SIZE);

    let event_row = row![
        event_label,
        Space::new().width(ELEMENT_SPACING),
        event_picker,
    ]
    .align_y(iced::Alignment::Center);
    items.push(event_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Get config for selected event
    let event_config = event_settings.get(selected_event_type);

    // Show notification checkbox - disabled when global notifications are off
    let show_notification_checkbox = if notifications_enabled {
        checkbox(event_config.show_notification)
            .label(t("event-settings-show-notification"))
            .on_toggle(Message::EventShowNotificationToggled)
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    } else {
        checkbox(event_config.show_notification)
            .label(t("event-settings-show-notification"))
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    };
    items.push(show_notification_checkbox.into());

    // Content level picker and test button on same row
    let content_enabled = notifications_enabled && event_config.show_notification;
    let content_levels: Vec<NotificationContent> = NotificationContent::all().to_vec();
    let content_picker = if content_enabled {
        pick_list(
            content_levels,
            Some(event_config.notification_content),
            Message::EventNotificationContentSelected,
        )
        .text_size(TEXT_SIZE)
    } else {
        pick_list(
            content_levels,
            Some(event_config.notification_content),
            |_| Message::EventNotificationContentSelected(event_config.notification_content),
        )
        .text_size(TEXT_SIZE)
    };

    let test_notification_button = if notifications_enabled && event_config.show_notification {
        button(shaped_text(t("settings-notification-test")).size(TEXT_SIZE))
            .on_press(Message::TestNotification)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("settings-notification-test")).size(TEXT_SIZE))
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    };

    let notification_row = row![
        content_picker,
        Space::new().width(ELEMENT_SPACING),
        test_notification_button,
    ]
    .align_y(iced::Alignment::Center);
    items.push(notification_row.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Play sound checkbox with Always Play inline
    let play_sound_enabled = sound_enabled;
    let always_play_enabled = sound_enabled && event_config.play_sound;

    let play_sound_checkbox = if play_sound_enabled {
        checkbox(event_config.play_sound)
            .label(t("settings-sound-play"))
            .on_toggle(Message::EventPlaySoundToggled)
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    } else {
        checkbox(event_config.play_sound)
            .label(t("settings-sound-play"))
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    };

    let always_play_checkbox = if always_play_enabled {
        checkbox(event_config.always_play_sound)
            .label(t("settings-sound-always-play"))
            .on_toggle(Message::EventAlwaysPlaySoundToggled)
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    } else {
        checkbox(event_config.always_play_sound)
            .label(t("settings-sound-always-play"))
            .text_size(TEXT_SIZE)
            .spacing(ELEMENT_SPACING)
    };

    let sound_checkboxes_row = row![
        play_sound_checkbox,
        Space::new().width(SPACER_SIZE_LARGE),
        always_play_checkbox,
    ]
    .align_y(iced::Alignment::Center);
    items.push(sound_checkboxes_row.into());

    // Sound picker and test button on same row
    let sound_picker_enabled = sound_enabled && event_config.play_sound;
    let sound_choices: Vec<SoundChoice> = SoundChoice::all().to_vec();
    let sound_picker = if sound_picker_enabled {
        pick_list(
            sound_choices,
            Some(event_config.sound.clone()),
            Message::EventSoundSelected,
        )
        .text_size(TEXT_SIZE)
    } else {
        pick_list(sound_choices, Some(event_config.sound.clone()), |_| {
            Message::EventSoundSelected(event_config.sound.clone())
        })
        .text_size(TEXT_SIZE)
    };

    let test_sound_button = if sound_enabled && event_config.play_sound {
        button(shaped_text(t("settings-sound-test")).size(TEXT_SIZE))
            .on_press(Message::TestSound)
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("settings-sound-test")).size(TEXT_SIZE))
            .padding(INPUT_PADDING)
            .style(btn::secondary)
    };

    let sound_row = row![
        sound_picker,
        Space::new().width(ELEMENT_SPACING),
        test_sound_button,
    ]
    .align_y(iced::Alignment::Center);
    items.push(sound_row.into());

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

//! Settings panel view

use super::chat::TimestampSettings;
use crate::config::settings::{CHAT_FONT_SIZES, ProxySettings, default_download_path};
use crate::config::theme::all_themes;
use crate::i18n::t;
use crate::image::CachedImage;
use crate::style::{
    AVATAR_PREVIEW_SIZE, BUTTON_PADDING, CHECKBOX_INDENT, ELEMENT_SPACING, FORM_MAX_WIDTH,
    FORM_PADDING, INPUT_PADDING, PATH_DISPLAY_PADDING, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TAB_LABEL_PADDING, TEXT_SIZE, TITLE_ROW_HEIGHT_WITH_ACTION, TITLE_SIZE,
    content_background_style, error_text_style, shaped_text, shaped_text_wrapped,
};
use crate::types::{InputId, Message, SettingsFormState, SettingsTab};
use iced::widget::button as btn;
use iced::widget::{
    Column, Id, Space, button, checkbox, container, pick_list, row, scrollable, text_input,
};
use iced::{Center, Element, Fill, Theme};
use iced_aw::NumberInput;
use iced_aw::TabLabel;
use iced_aw::Tabs;

// ============================================================================
// Settings View Data
// ============================================================================

/// Data needed to render the settings panel
pub struct SettingsViewData<'a> {
    /// Current theme for styling
    pub current_theme: Theme,
    /// Show user connect/disconnect notifications in chat
    pub show_connection_notifications: bool,
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
        data.show_connection_notifications,
        data.timestamp_settings,
    );
    let network_content = network_tab_content(data.proxy);

    let files_content = files_tab_content(data.download_path);

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

    content_items.push(
        container(
            shaped_text(t("title-settings"))
                .size(TITLE_SIZE)
                .width(Fill)
                .align_x(Center),
        )
        // Title row height matches news/users panels (which have action buttons)
        .height(TITLE_ROW_HEIGHT_WITH_ACTION)
        .align_y(Center)
        .into(),
    );

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
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

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
    show_connection_notifications: bool,
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

    // Connection notifications checkbox
    let notifications_checkbox = checkbox(show_connection_notifications)
        .label(t("label-show-connection-notifications"))
        .on_toggle(Message::ConnectionNotificationsToggled)
        .text_size(TEXT_SIZE);
    items.push(notifications_checkbox.into());

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

/// Build the Files tab content (download location)
fn files_tab_content(download_path: Option<&str>) -> Element<'static, Message> {
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

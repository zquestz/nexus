//! Settings panel view

use super::chat::TimestampSettings;
use crate::config::{CHAT_FONT_SIZES, all_themes};
use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, SPACER_SIZE_MEDIUM, TEXT_SIZE,
    TITLE_SIZE, content_background_style, shaped_text,
};
use crate::types::Message;
use iced::widget::button as btn;
use iced::widget::{Space, button, checkbox, column, container, pick_list, row};
use iced::{Center, Element, Fill, Theme};

// ============================================================================
// Settings View
// ============================================================================

/// Render the settings panel
///
/// Shows application settings that can be modified and saved to disk.
/// Cancel restores original settings, Save persists changes.
pub fn settings_view(
    current_theme: Theme,
    show_connection_notifications: bool,
    chat_font_size: u8,
    timestamp_settings: TimestampSettings,
) -> Element<'static, Message> {
    let title = shaped_text(t("title-settings"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    // Theme picker row
    let theme_label = shaped_text(t("label-theme")).size(TEXT_SIZE);
    let theme_picker =
        pick_list(all_themes(), Some(current_theme), Message::ThemeSelected).text_size(TEXT_SIZE);
    let theme_row = row![theme_label, theme_picker]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);

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

    // Connection notifications checkbox
    let notifications_checkbox = checkbox(
        t("label-show-connection-notifications"),
        show_connection_notifications,
    )
    .on_toggle(Message::ConnectionNotificationsToggled)
    .text_size(TEXT_SIZE);

    // Timestamp settings
    let timestamps_checkbox = checkbox(
        t("label-show-timestamps"),
        timestamp_settings.show_timestamps,
    )
    .on_toggle(Message::ShowTimestampsToggled)
    .text_size(TEXT_SIZE);

    // 24-hour time checkbox (disabled if timestamps are hidden)
    let time_format_checkbox = if timestamp_settings.show_timestamps {
        checkbox(
            t("label-use-24-hour-time"),
            timestamp_settings.use_24_hour_time,
        )
        .on_toggle(Message::Use24HourTimeToggled)
        .text_size(TEXT_SIZE)
    } else {
        checkbox(
            t("label-use-24-hour-time"),
            timestamp_settings.use_24_hour_time,
        )
        .text_size(TEXT_SIZE)
    };

    // Show seconds checkbox (disabled if timestamps are hidden)
    let seconds_checkbox = if timestamp_settings.show_timestamps {
        checkbox(t("label-show-seconds"), timestamp_settings.show_seconds)
            .on_toggle(Message::ShowSecondsToggled)
            .text_size(TEXT_SIZE)
    } else {
        checkbox(t("label-show-seconds"), timestamp_settings.show_seconds).text_size(TEXT_SIZE)
    };

    // Indent the dependent timestamp options
    let time_format_row = row![Space::with_width(20), time_format_checkbox];
    let seconds_row = row![Space::with_width(20), seconds_checkbox];

    let buttons = row![
        Space::with_width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::CancelSettings)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::SaveSettings)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    let form = column![
        title,
        Space::with_height(SPACER_SIZE_MEDIUM),
        theme_row,
        font_size_row,
        notifications_checkbox,
        timestamps_checkbox,
        time_format_row,
        seconds_row,
        Space::with_height(SPACER_SIZE_MEDIUM),
        buttons,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    container(form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}

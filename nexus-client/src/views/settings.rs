//! Settings panel view

use super::chat::TimestampSettings;
use super::layout::scrollable_panel;
use crate::config::settings::CHAT_FONT_SIZES;
use crate::config::theme::all_themes;
use crate::i18n::t;
use crate::style::{
    AVATAR_PREVIEW_SIZE, BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING,
    INPUT_PADDING, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, SUBHEADING_SIZE, TEXT_SIZE, TITLE_SIZE,
    error_text_style, shaped_text, shaped_text_wrapped, subheading_text_style,
};
use crate::types::{Message, SettingsFormState};
use iced::widget::button as btn;
use iced::widget::{Column, Space, button, checkbox, pick_list, row, text_input};
use iced::{Center, Element, Fill, Theme};

// ============================================================================
// Settings View
// ============================================================================

/// Render the settings panel
///
/// Shows application settings that can be modified and saved to disk.
/// Cancel restores original settings, Save persists changes.
pub fn settings_view<'a>(
    current_theme: Theme,
    show_connection_notifications: bool,
    chat_font_size: u8,
    timestamp_settings: TimestampSettings,
    settings_form: Option<&'a SettingsFormState>,
    nickname: &'a str,
) -> Element<'a, Message> {
    // Extract avatar state from settings form (only present when panel is open)
    let (avatar, default_avatar, error) = settings_form
        .map(|f| {
            (
                f.cached_avatar.as_ref(),
                Some(&f.default_avatar),
                f.error.as_deref(),
            )
        })
        .unwrap_or((None, None, None));

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
    let notifications_checkbox = checkbox(show_connection_notifications)
        .label(t("label-show-connection-notifications"))
        .on_toggle(Message::ConnectionNotificationsToggled)
        .text_size(TEXT_SIZE);

    // Timestamp settings
    let timestamps_checkbox = checkbox(timestamp_settings.show_timestamps)
        .label(t("label-show-timestamps"))
        .on_toggle(Message::ShowTimestampsToggled)
        .text_size(TEXT_SIZE);

    // 24-hour time checkbox (disabled if timestamps are hidden)
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

    // Show seconds checkbox (disabled if timestamps are hidden)
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

    // Indent the dependent timestamp options
    let time_format_row = row![Space::new().width(20), time_format_checkbox];
    let seconds_row = row![Space::new().width(20), seconds_checkbox];

    // Nickname input
    let nickname_input = text_input(&t("placeholder-nickname-optional"), nickname)
        .on_input(Message::SettingsNicknameChanged)
        .id(iced::widget::Id::from(
            crate::types::InputId::SettingsNickname,
        ))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

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

    let buttons: iced::widget::Row<'_, Message> = row![
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

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = error {
        form_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    // Appearance subheading
    let appearance_heading = shaped_text(t("label-appearance"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);

    // Chat subheading
    let chat_heading = shaped_text(t("label-chat-options"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);

    // Identity subheading (avatar + nickname)
    let identity_heading = shaped_text(t("label-identity"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);

    form_items.extend([
        appearance_heading.into(),
        theme_row.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        identity_heading.into(),
        avatar_row.into(),
        nickname_input.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        chat_heading.into(),
        font_size_row.into(),
        notifications_checkbox.into(),
        timestamps_checkbox.into(),
        time_format_row.into(),
        seconds_row.into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        buttons.into(),
    ]);

    let form = Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

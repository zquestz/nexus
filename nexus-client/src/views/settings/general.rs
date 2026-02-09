//! General settings tab (theme, avatar, nickname, tray settings)

use iced::widget::button as btn;
#[cfg(not(target_os = "macos"))]
use iced::widget::checkbox;
use iced::widget::{Column, Id, Space, button, pick_list, row, text_input};
use iced::{Center, Element, Fill};

use crate::config::theme::all_themes;
use crate::i18n::t;
use crate::image::CachedImage;
use crate::style::{
    AVATAR_PREVIEW_SIZE, BUTTON_PADDING, ELEMENT_SPACING, INPUT_PADDING, SPACER_SIZE_MEDIUM,
    TEXT_SIZE, shaped_text,
};
#[cfg(not(target_os = "macos"))]
use crate::style::{CHECKBOX_INDENT, SPACER_SIZE_SMALL};
use crate::types::{InputId, Message};
use iced::Theme;

/// Build the General tab content (theme, avatar, nickname, tray settings)
pub(super) fn general_tab_content<'a>(
    current_theme: Theme,
    avatar: Option<&'a CachedImage>,
    default_avatar: Option<&'a CachedImage>,
    nickname: &'a str,
    show_tray_icon: bool,
    minimize_to_tray: bool,
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

    // System tray settings (Windows/Linux only)
    #[cfg(not(target_os = "macos"))]
    {
        // Add some spacing before tray section
        items.push(Space::new().height(SPACER_SIZE_SMALL).into());

        // Show tray icon checkbox
        let tray_icon_checkbox = checkbox(show_tray_icon)
            .label(t("settings-show-tray-icon"))
            .on_toggle(Message::ShowTrayIconToggled)
            .text_size(TEXT_SIZE);
        items.push(tray_icon_checkbox.into());

        // Minimize to tray checkbox (only enabled when tray icon is shown)
        let minimize_checkbox = checkbox(minimize_to_tray)
            .label(t("settings-minimize-to-tray"))
            .text_size(TEXT_SIZE);
        let minimize_checkbox = if show_tray_icon {
            minimize_checkbox.on_toggle(Message::MinimizeToTrayToggled)
        } else {
            minimize_checkbox
        };
        let minimize_row = row![Space::new().width(CHECKBOX_INDENT), minimize_checkbox];
        items.push(minimize_row.into());
    }

    // Suppress unused variable warnings on macOS
    #[cfg(target_os = "macos")]
    {
        let _ = show_tray_icon;
        let _ = minimize_to_tray;
    }

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

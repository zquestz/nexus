//! User info panel view

use std::collections::HashMap;

use super::layout::scrollable_panel;
use crate::avatar::generate_identicon;
use crate::handlers::network::constants::DATETIME_FORMAT;
use crate::handlers::network::helpers::format_duration;
use crate::i18n::{t, t_args};
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, SPACER_SIZE_MEDIUM, TEXT_SIZE,
    TITLE_SIZE, USER_INFO_AVATAR_SIZE, USER_INFO_AVATAR_SPACING, chat, shaped_text,
};
use crate::types::Message;
use iced::widget::button as btn;
use iced::widget::{Space, button, column, row};
use iced::{Center, Color, Element, Fill, Theme};
use nexus_common::protocol::UserInfoDetailed;

use super::constants::PERMISSION_USER_EDIT;

/// Render the user info panel
///
/// Displays user information received from the server.
/// Shows loading state, error state, or user details depending on data.
pub fn user_info_view<'a>(
    data: &Option<Result<UserInfoDetailed, String>>,
    theme: Theme,
    is_admin: bool,
    permissions: &[String],
    current_username: &str,
    avatar_cache: &'a HashMap<String, CachedImage>,
) -> Element<'a, Message> {
    let has_edit_permission = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_EDIT);

    let mut content = column![].spacing(ELEMENT_SPACING);

    match data {
        None => {
            // Loading state - show centered loading text
            let loading = shaped_text(t("user-info-loading"))
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center);
            content = content.push(loading);
        }
        Some(Err(error)) => {
            // Error state
            let error_text = shaped_text(error.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center);
            content = content.push(error_text);
        }
        Some(Ok(user)) => {
            // User info display with avatar + username header
            content = build_user_info_content(content, user, &theme, avatar_cache);
        }
    }

    // Build buttons row - spacer, then Edit (if permitted) + Close on the right
    let mut buttons = row![Space::new().width(Fill)].spacing(ELEMENT_SPACING);

    // Add edit button if user has permission, data loaded, and not viewing self
    if has_edit_permission
        && let Some(Ok(user)) = data
        && user.username.to_lowercase() != current_username.to_lowercase()
    {
        buttons = buttons.push(
            button(shaped_text(t("button-edit")).size(TEXT_SIZE))
                .on_press(Message::ToggleEditUser(Some(user.username.clone())))
                .padding(BUTTON_PADDING)
                .style(btn::secondary),
        );
    }

    // Close button (primary)
    buttons = buttons.push(
        button(shaped_text(t("button-close")).size(TEXT_SIZE))
            .on_press(Message::CloseUserInfo)
            .padding(BUTTON_PADDING),
    );

    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));
    content = content.push(buttons);

    let form = content.padding(FORM_PADDING).max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

/// Create a label: value row for the user info panel
fn info_row<'a>(
    label: String,
    value: String,
    color: Option<Color>,
) -> iced::widget::Row<'a, Message> {
    let value_text = shaped_text(value).size(TEXT_SIZE);
    let value_text = if let Some(c) = color {
        value_text.color(c)
    } else {
        value_text
    };
    row![
        shaped_text(label).size(TEXT_SIZE),
        Space::new().width(ELEMENT_SPACING),
        value_text,
    ]
    .align_y(Center)
}

/// Build the user info content rows
fn build_user_info_content<'a>(
    mut content: iced::widget::Column<'a, Message>,
    user: &UserInfoDetailed,
    theme: &Theme,
    avatar_cache: &'a HashMap<String, CachedImage>,
) -> iced::widget::Column<'a, Message> {
    // Header row: Avatar + Username (title-sized, red for admins)
    let is_admin = user.is_admin.unwrap_or(false);

    let avatar_element: Element<'_, Message> =
        if let Some(cached_avatar) = avatar_cache.get(&user.username) {
            cached_avatar.render(USER_INFO_AVATAR_SIZE)
        } else {
            // Fallback: generate identicon (shouldn't happen if cache is properly populated)
            generate_identicon(&user.username).render(USER_INFO_AVATAR_SIZE)
        };

    let username_text = if is_admin {
        shaped_text(&user.username)
            .size(TITLE_SIZE)
            .color(chat::admin(theme))
    } else {
        shaped_text(&user.username).size(TITLE_SIZE)
    };

    let header_row = row![avatar_element, username_text]
        .spacing(USER_INFO_AVATAR_SPACING)
        .align_y(Center);

    content = content.push(header_row);
    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Role (only shown if is_admin field is present)
    if user.is_admin.is_some() {
        let role_value = if is_admin {
            t("user-info-role-admin")
        } else {
            t("user-info-role-user")
        };
        content = content.push(info_row(t("user-info-role"), role_value, None));
    }

    // Session duration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time should be after UNIX epoch")
        .as_secs() as i64;
    let session_duration_secs = now.saturating_sub(user.login_time) as u64;
    let duration_str = format_duration(session_duration_secs);

    let session_count = user.session_ids.len();
    let connected_value = if session_count == 1 {
        t_args("user-info-connected-value", &[("duration", &duration_str)])
    } else {
        t_args(
            "user-info-connected-value-sessions",
            &[
                ("duration", &duration_str),
                ("count", &session_count.to_string()),
            ],
        )
    };
    content = content.push(info_row(t("user-info-connected"), connected_value, None));

    // Features
    let features_value = if user.features.is_empty() {
        t("user-info-features-none")
    } else {
        t_args(
            "user-info-features-value",
            &[("features", &user.features.join(", "))],
        )
    };
    content = content.push(info_row(t("user-info-features"), features_value, None));

    // Locale
    content = content.push(info_row(t("user-info-locale"), user.locale.clone(), None));

    // IP Addresses (only shown if field is present - admin viewers only)
    if let Some(addresses) = &user.addresses
        && !addresses.is_empty()
    {
        if addresses.len() == 1 {
            content = content.push(info_row(t("user-info-address"), addresses[0].clone(), None));
        } else {
            // Multiple addresses - show label then list
            content = content.push(info_row(t("user-info-addresses"), String::new(), None));
            for addr in addresses {
                content = content.push(info_row(String::from("  "), addr.clone(), None));
            }
        }
    }

    // Account created
    let created = chrono::DateTime::from_timestamp(user.created_at, 0)
        .map(|dt| dt.format(DATETIME_FORMAT).to_string())
        .unwrap_or_else(|| t("user-info-unknown"));
    content = content.push(info_row(t("user-info-created"), created, None));

    content
}

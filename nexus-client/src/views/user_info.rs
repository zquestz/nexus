//! User info panel view

use std::collections::HashMap;

use iced::widget::button as btn;
use iced::widget::{Id, Space, button, column, row, text_input};
use iced::{Center, Color, Element, Fill, Theme};
use nexus_common::protocol::UserInfoDetailed;

use super::constants::PERMISSION_USER_EDIT;
use super::layout::scrollable_panel;
use crate::avatar::generate_identicon;
use crate::handlers::network::constants::DATETIME_FORMAT;
use crate::handlers::network::helpers::format_duration;
use crate::i18n::{t, t_args};
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, USER_INFO_AVATAR_SIZE, USER_INFO_AVATAR_SPACING,
    chat, error_text_style, panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::{InputId, Message, PasswordChangeState, ServerConnection};

/// Render the user info panel
///
/// Displays user information received from the server.
/// Shows loading state, error state, or user details depending on data.
pub fn user_info_view<'a>(conn: &'a ServerConnection, theme: Theme) -> Element<'a, Message> {
    let has_edit_permission = conn.has_permission(PERMISSION_USER_EDIT);
    let data = &conn.user_info_data;
    let current_username = &conn.connection_info.username;
    let avatar_cache = &conn.avatar_cache;

    let mut content = column![].spacing(ELEMENT_SPACING);

    // Check if viewing self (for Change Password button)
    let viewing_self = data
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|user| user.username.to_lowercase() == current_username.to_lowercase());

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

    // Build buttons row - spacer, then buttons on the right
    let mut buttons = row![Space::new().width(Fill)].spacing(ELEMENT_SPACING);

    // Add Change Password button if viewing self, data loaded, and NOT a shared account
    // (shared account users cannot change their password)
    let is_shared_user = data
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|user| user.is_shared);

    if viewing_self && !is_shared_user {
        buttons = buttons.push(
            button(shaped_text(t("button-change-password")).size(TEXT_SIZE))
                .on_press(Message::ChangePasswordPressed)
                .padding(BUTTON_PADDING)
                .style(btn::secondary),
        );
    }

    // Add edit button if user has permission, data loaded, and not viewing self
    if has_edit_permission
        && let Some(Ok(user)) = data
        && !viewing_self
    {
        buttons = buttons.push(
            button(shaped_text(t("button-edit")).size(TEXT_SIZE))
                .on_press(Message::UserManagementEditClicked(user.username.clone()))
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

    let form = content
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

/// Render the password change panel
pub fn password_change_view(state: Option<&PasswordChangeState>) -> Element<'_, Message> {
    let Some(state) = state else {
        // Should not happen, but handle gracefully
        return column![].into();
    };

    let mut content = column![].spacing(ELEMENT_SPACING);

    // Title (centered)
    content = content.push(panel_title(t("title-change-password")));

    // Error message (if any) - shown under title, centered
    if let Some(error) = &state.error {
        content = content.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style),
        );
        content = content.push(Space::new().height(SPACER_SIZE_SMALL));
    } else {
        content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));
    }

    // Current password field
    let current_password_input =
        text_input(&t("placeholder-current-password"), &state.current_password)
            .id(Id::from(InputId::ChangePasswordCurrent))
            .on_input(Message::ChangePasswordCurrentChanged)
            .on_submit(Message::ChangePasswordSavePressed)
            .secure(true)
            .padding(BUTTON_PADDING)
            .width(Fill);
    content = content.push(current_password_input);

    // New password field
    let new_password_input = text_input(&t("placeholder-new-password"), &state.new_password)
        .id(Id::from(InputId::ChangePasswordNew))
        .on_input(Message::ChangePasswordNewChanged)
        .on_submit(Message::ChangePasswordSavePressed)
        .secure(true)
        .padding(BUTTON_PADDING)
        .width(Fill);
    content = content.push(new_password_input);

    // Confirm password field
    let confirm_password_input =
        text_input(&t("placeholder-confirm-password"), &state.confirm_password)
            .id(Id::from(InputId::ChangePasswordConfirm))
            .on_input(Message::ChangePasswordConfirmChanged)
            .on_submit(Message::ChangePasswordSavePressed)
            .secure(true)
            .padding(BUTTON_PADDING)
            .width(Fill);
    content = content.push(confirm_password_input);

    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Check if all fields are filled for enabling Save button
    let can_save = !state.current_password.is_empty()
        && !state.new_password.is_empty()
        && !state.confirm_password.is_empty();

    // Buttons row
    let save_button = button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING);
    let save_button = if can_save {
        save_button.on_press(Message::ChangePasswordSavePressed)
    } else {
        save_button
    };

    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::ChangePasswordCancelPressed)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        save_button,
    ]
    .spacing(ELEMENT_SPACING);

    content = content.push(buttons);

    let form = content
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

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
    // Header: Avatar + Nickname (always populated; equals username for regular accounts)
    // For shared accounts, also show the account name below
    let is_admin = user.is_admin.unwrap_or(false);
    let is_shared = user.is_shared;

    // Nickname is always populated (equals username for regular accounts)
    let nickname = &user.nickname;

    // Avatar cache is keyed by nickname (always populated; equals username for regular accounts)
    let avatar_element: Element<'_, Message> =
        if let Some(cached_avatar) = avatar_cache.get(nickname) {
            cached_avatar.render(USER_INFO_AVATAR_SIZE)
        } else {
            // Fallback: generate identicon (shouldn't happen if cache is properly populated)
            generate_identicon(nickname).render(USER_INFO_AVATAR_SIZE)
        };

    // Build nickname display with 💤 if away
    let nickname_display = if user.is_away {
        format!("{} 💤", nickname)
    } else {
        nickname.clone()
    };

    // Apply color: admin = red, shared = muted, regular = default
    let nickname_text = if is_admin {
        shaped_text(nickname_display)
            .size(TITLE_SIZE)
            .color(chat::admin(theme))
    } else if is_shared {
        shaped_text(nickname_display)
            .size(TITLE_SIZE)
            .color(chat::shared(theme))
    } else {
        shaped_text(nickname_display).size(TITLE_SIZE)
    };

    let header_row = row![avatar_element, nickname_text]
        .spacing(USER_INFO_AVATAR_SPACING)
        .align_y(Center);

    content = content.push(header_row);
    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Role (only shown if is_admin field is present)
    if user.is_admin.is_some() {
        let is_guest = user.username.to_lowercase() == "guest";
        let role_value = if is_admin {
            t("user-info-role-admin")
        } else if is_guest {
            t("user-info-role-guest")
        } else if is_shared {
            t("user-info-role-shared")
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

    // Features (sorted alphabetically for consistent display)
    let features_value = if user.features.is_empty() {
        t("user-info-features-none")
    } else {
        let mut sorted_features = user.features.clone();
        sorted_features.sort();
        t_args(
            "user-info-features-value",
            &[("features", &sorted_features.join(", "))],
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

    // Status (if set) - away is shown via 💤 in header
    if let Some(status) = &user.status {
        content = content.push(info_row(t("user-info-status"), status.clone(), None));
    }

    // Account created
    let created = chrono::DateTime::from_timestamp(user.created_at, 0)
        .map(|dt| dt.format(DATETIME_FORMAT).to_string())
        .unwrap_or_else(|| t("user-info-unknown"));
    content = content.push(info_row(t("user-info-created"), created, None));

    content
}

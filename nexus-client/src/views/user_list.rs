//! User list panel (right sidebar)
//!
//! Shows contextual user list based on the active chat tab:
//! - Console tab: All online users
//! - Channel tab: Only channel members
//! - User message tab: You + the other user (or just you if they're offline)

use iced::widget::{Column, Row, Space, button, column, container, row, scrollable, tooltip};
use iced::{Center, Color, Element, Fill, Theme};

use super::constants::{
    PERMISSION_BAN_CREATE, PERMISSION_USER_INFO, PERMISSION_USER_KICK, PERMISSION_USER_MESSAGE,
};
use crate::avatar::generate_identicon;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    CONTENT_PADDING, ICON_BUTTON_PADDING, INPUT_PADDING, NO_SPACING, SCROLLBAR_PADDING,
    SEPARATOR_HEIGHT, SIDEBAR_ACTION_ICON_SIZE, TOOLBAR_CONTAINER_PADDING,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    USER_LIST_AVATAR_SIZE, USER_LIST_AVATAR_SPACING, USER_LIST_ITEM_SPACING, USER_LIST_PANEL_WIDTH,
    USER_LIST_SMALL_TEXT_SIZE, USER_LIST_SPACING, USER_LIST_TEXT_SIZE, USER_LIST_TITLE_SIZE,
    alternating_row_style, chat, disabled_icon_button_style, icon_button_with_hover_style,
    muted_text_style, shaped_text, sidebar_panel_style, tooltip_container_style, ui,
    user_list_item_button_style, user_toolbar_separator_style,
};
use crate::types::ActivePanel;
use crate::types::{ChatTab, Message, ServerConnection, UserInfo};

// ============================================================================
// Helper Functions
// ============================================================================

/// Create an icon container with consistent sizing and alignment
fn icon_container(icon: iced::widget::Text<'_>) -> iced::widget::Container<'_, Message> {
    container(icon.size(SIDEBAR_ACTION_ICON_SIZE))
        .width(SIDEBAR_ACTION_ICON_SIZE)
        .height(SIDEBAR_ACTION_ICON_SIZE)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
}

/// Create an enabled icon button with hover effect
fn enabled_icon_button<'a>(
    icon: iced::widget::Container<'a, Message>,
    message: Message,
    hover_color: Color,
    normal_color: Color,
) -> button::Button<'a, Message> {
    button(icon)
        .on_press(message)
        .padding(ICON_BUTTON_PADDING)
        .style(icon_button_with_hover_style(hover_color, normal_color))
}

/// Create a disabled icon button (greyed out)
fn disabled_icon_button(icon: iced::widget::Container<'_, Message>) -> button::Button<'_, Message> {
    button(icon)
        .padding(ICON_BUTTON_PADDING)
        .style(disabled_icon_button_style)
}

/// Wrap a button in a tooltip
fn with_tooltip<'a>(
    btn: button::Button<'a, Message>,
    tooltip_text: String,
) -> tooltip::Tooltip<'a, Message> {
    tooltip(
        btn,
        container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING)
}

/// Create a horizontal separator line (primary color)
fn toolbar_separator<'a>() -> iced::widget::Container<'a, Message> {
    container(Space::new().width(Fill).height(SEPARATOR_HEIGHT))
        .width(Fill)
        .height(SEPARATOR_HEIGHT)
        .style(user_toolbar_separator_style)
}

// ============================================================================
// User Filtering
// ============================================================================

/// Get the list of users to display based on the active view
///
/// Returns a filtered and sorted list of users:
/// - Panel open (Files, News, etc.): All online users
/// - Console: All online users
/// - Channel: Only channel members (matched by nickname, case-insensitive)
/// - User message: You + the other user (if they're online)
fn get_contextual_users(conn: &ServerConnection) -> Vec<&UserInfo> {
    // When a panel is open, always show all users
    if conn.active_panel != ActivePanel::None {
        return conn.online_users.iter().collect();
    }

    let current_nickname = conn
        .online_users
        .iter()
        .find(|u| u.session_ids.contains(&conn.session_id))
        .map(|u| u.nickname.as_str())
        .unwrap_or(&conn.connection_info.username);

    match &conn.active_chat_tab {
        ChatTab::Console => {
            // Show all online users
            conn.online_users.iter().collect()
        }
        ChatTab::Channel(channel_name) => {
            // Show only channel members
            if let Some(channel_state) = conn.get_channel_state(channel_name) {
                // Filter online_users to only those who are members of this channel
                conn.online_users
                    .iter()
                    .filter(|user| {
                        channel_state
                            .members
                            .iter()
                            .any(|m| m.eq_ignore_ascii_case(&user.nickname))
                    })
                    .collect()
            } else {
                // Channel not found, show empty list
                Vec::new()
            }
        }
        ChatTab::UserMessage(other_nickname) => {
            // Show you + the other user (if they're online)
            let other_lower = other_nickname.to_lowercase();
            let current_lower = current_nickname.to_lowercase();

            conn.online_users
                .iter()
                .filter(|user| {
                    let nickname_lower = user.nickname.to_lowercase();
                    nickname_lower == current_lower || nickname_lower == other_lower
                })
                .collect()
        }
    }
}

/// Get the title for the user list based on the active view
fn get_user_list_title(conn: &ServerConnection) -> String {
    // When a panel is open, show generic "Users" title
    if conn.active_panel != ActivePanel::None {
        return t("title-users");
    }

    match &conn.active_chat_tab {
        ChatTab::Console => t("title-users"),
        ChatTab::Channel(_) | ChatTab::UserMessage(_) => t("title-channel-members"),
    }
}

// ============================================================================
// User Toolbar
// ============================================================================

/// Create action toolbar for an expanded user
///
/// - `nickname`: The user's nickname (always populated; equals username for regular accounts).
///   Used for all actions: info, PM, kick - server looks up by nickname.
fn create_user_toolbar<'a>(
    nickname: &'a str,
    current_nickname: &'a str,
    target_is_admin: bool,
    conn: &ServerConnection,
    theme: &Theme,
) -> Row<'a, Message> {
    let nickname_owned = nickname.to_string();
    let is_self = nickname == current_nickname;

    // Check permissions
    let has_user_info_permission = conn.has_permission(PERMISSION_USER_INFO);
    let has_user_message_permission = conn.has_permission(PERMISSION_USER_MESSAGE);
    let has_disconnect_permission =
        conn.has_permission(PERMISSION_USER_KICK) || conn.has_permission(PERMISSION_BAN_CREATE);

    // Build toolbar row
    let mut toolbar_row = row![].spacing(NO_SPACING).width(Fill);

    // Info button (always show, disabled if no permission)
    let info_icon = icon_container(icon::info());
    let primary_color = theme.palette().primary;
    let icon_color = ui::icon_color(theme);
    let danger_color = theme.palette().danger;

    let info_button = if has_user_info_permission {
        let nickname_for_info = nickname_owned.clone();
        enabled_icon_button(
            info_icon,
            Message::UserInfoIconClicked(nickname_for_info),
            primary_color,
            icon_color,
        )
    } else {
        disabled_icon_button(info_icon)
    };
    toolbar_row = toolbar_row.push(with_tooltip(info_button, t("tooltip-info")));

    // Message button (only show if not self)
    if !is_self {
        let message_icon = icon_container(icon::message());
        let message_button = if has_user_message_permission {
            let nickname_for_message = nickname_owned.clone();
            enabled_icon_button(
                message_icon,
                Message::UserMessageIconClicked(nickname_for_message),
                primary_color,
                icon_color,
            )
        } else {
            disabled_icon_button(message_icon)
        };
        toolbar_row = toolbar_row.push(with_tooltip(message_button, t("tooltip-message")));
    }

    // Disconnect button (if not self, has kick or ban permission, and target is not admin)
    if !is_self && has_disconnect_permission && !target_is_admin {
        // Add spacer to push disconnect button to the right
        toolbar_row = toolbar_row.push(Space::new().width(Fill).height(SEPARATOR_HEIGHT));

        let disconnect_icon = icon_container(icon::kick());
        let disconnect_button = enabled_icon_button(
            disconnect_icon,
            Message::DisconnectIconClicked(nickname_owned),
            danger_color,
            icon_color,
        );
        toolbar_row = toolbar_row.push(with_tooltip(disconnect_button, t("tooltip-disconnect")));
    }

    toolbar_row
}

// ============================================================================
// User List Panel
// ============================================================================

/// Displays online users as clickable buttons with expandable action toolbars
///
/// Shows a contextual list of users based on the active chat tab:
/// - Console tab: All online users
/// - Channel tab: Only channel members
/// - User message tab: You + the other user
///
/// Clicking a username expands it to show an action toolbar underneath.
/// Only one user can be expanded at a time.
/// Admin users are shown in red (using the chat admin color).
///
/// Note: This panel is only shown when the user has `user_list` permission.
/// Permission checking is done at the layout level.
///
/// Build tooltip text for a user, including away/status information
fn build_user_tooltip(nickname: &str, is_away: bool, status: Option<&str>) -> String {
    match (is_away, status) {
        (true, Some(status_msg)) => format!("{} 💤\n{}", nickname, status_msg),
        (true, None) => format!("{} 💤", nickname),
        (false, Some(status_msg)) => format!("{}\n{}", nickname, status_msg),
        (false, None) => nickname.to_string(),
    }
}

pub fn user_list_panel<'a>(conn: &'a ServerConnection, theme: &Theme) -> Element<'a, Message> {
    // Get current user's nickname for self-detection
    // Use session_id to find our entry (important for shared accounts where
    // multiple users may have the same username but different nicknames)
    let current_nickname = conn
        .online_users
        .iter()
        .find(|u| u.session_ids.contains(&conn.session_id))
        .map(|u| u.nickname.as_str())
        .unwrap_or(&conn.connection_info.username);

    // Get contextual title based on active tab
    let title = shaped_text(get_user_list_title(conn))
        .size(USER_LIST_TITLE_SIZE)
        .style(muted_text_style);

    // Get contextual user list based on active tab
    let users = get_contextual_users(conn);

    let mut users_column = Column::new().spacing(USER_LIST_ITEM_SPACING);

    if users.is_empty() {
        // Show appropriate empty message based on tab type
        let empty_message = match &conn.active_chat_tab {
            ChatTab::Console => t("empty-no-users"),
            ChatTab::Channel(_) => t("empty-no-channel-members"),
            ChatTab::UserMessage(_) => t("empty-no-users"),
        };
        users_column = users_column.push(
            shaped_text(empty_message)
                .size(USER_LIST_SMALL_TEXT_SIZE)
                .style(muted_text_style),
        );
    } else {
        for (index, user) in users.iter().enumerate() {
            let is_expanded = conn.expanded_user.as_deref() == Some(user.nickname.as_str());
            let is_even = index % 2 == 0;

            // Username button with avatar
            let user_is_admin = user.is_admin;
            let user_is_shared = user.is_shared;
            let nickname_clone = user.nickname.clone();
            let nickname = &user.nickname;

            // Get cached avatar (should already be populated by handlers)
            // Avatar cache is keyed by nickname (always populated; equals username for regular accounts)
            let avatar_element: Element<'_, Message> =
                if let Some(cached_avatar) = conn.avatar_cache.get(nickname) {
                    cached_avatar.render(USER_LIST_AVATAR_SIZE)
                } else {
                    // Fallback: generate identicon if not in cache (shouldn't happen normally)
                    // Use nickname for identicon to match the cached key
                    generate_identicon(nickname).render(USER_LIST_AVATAR_SIZE)
                };

            // Row with avatar and nickname (always populated; equals username for regular accounts)
            // Apply appropriate color: admin = red, shared = muted, regular = default
            let nickname_text = if user_is_admin {
                shaped_text(nickname)
                    .size(USER_LIST_TEXT_SIZE)
                    .color(chat::admin(theme))
            } else if user_is_shared {
                shaped_text(nickname)
                    .size(USER_LIST_TEXT_SIZE)
                    .color(chat::shared(theme))
            } else {
                shaped_text(nickname).size(USER_LIST_TEXT_SIZE)
            };

            let user_row = row![avatar_element, nickname_text,]
                .spacing(USER_LIST_AVATAR_SPACING)
                .align_y(Center);

            let user_button = button(container(user_row).width(Fill))
                .on_press(Message::UserListItemClicked(nickname_clone))
                .width(Fill)
                .padding(INPUT_PADDING)
                .style(user_list_item_button_style(
                    user_is_admin,
                    chat::admin(theme),
                ));

            // Tooltip: show nickname with away/status if set
            let tooltip_text = build_user_tooltip(nickname, user.is_away, user.status.as_deref());

            // Wrap button in tooltip showing full name (useful when truncated)
            let user_button_with_tooltip = tooltip(
                user_button,
                container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Left,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING);

            // Create item column (username + optional toolbar)
            let mut item_column = Column::new().spacing(NO_SPACING);

            // Username button with tooltip
            item_column = item_column.push(user_button_with_tooltip);

            // Add toolbar if expanded
            if is_expanded {
                // Primary color separator line
                item_column = item_column.push(toolbar_separator());

                // Toolbar
                let toolbar = create_user_toolbar(
                    &user.nickname,
                    current_nickname,
                    user.is_admin,
                    conn,
                    theme,
                );
                let toolbar_row = container(toolbar)
                    .width(Fill)
                    .padding(TOOLBAR_CONTAINER_PADDING);
                item_column = item_column.push(toolbar_row);
            }

            // Wrap entire item (username + toolbar) in container with alternating background
            let item_container = container(item_column)
                .width(Fill)
                .style(alternating_row_style(is_even));

            users_column = users_column.push(item_container);
        }
    }

    // Add right padding to make room for scrollbar
    let users_column = container(users_column)
        .padding(iced::Padding {
            top: 0.0,
            right: SCROLLBAR_PADDING,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Fill);

    let panel = column![title, scrollable(users_column).height(Fill),]
        .spacing(USER_LIST_SPACING)
        .padding(iced::Padding {
            top: CONTENT_PADDING,
            right: CONTENT_PADDING - SCROLLBAR_PADDING,
            bottom: CONTENT_PADDING,
            left: CONTENT_PADDING,
        })
        .width(USER_LIST_PANEL_WIDTH);

    container(panel)
        .height(Fill)
        .style(sidebar_panel_style)
        .into()
}

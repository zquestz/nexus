//! User list panel (right sidebar)

use super::constants::{PERMISSION_USER_INFO, PERMISSION_USER_KICK, PERMISSION_USER_MESSAGE};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BORDER_WIDTH, FORM_PADDING, ICON_BUTTON_PADDING, INPUT_PADDING, NO_SPACING, SEPARATOR_HEIGHT,
    SIDEBAR_ACTION_ICON_SIZE, TOOLBAR_CONTAINER_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, USER_LIST_ITEM_SPACING, USER_LIST_PANEL_WIDTH,
    USER_LIST_SMALL_TEXT_SIZE, USER_LIST_SPACING, USER_LIST_TEXT_SIZE, USER_LIST_TITLE_SIZE,
    admin_user_text_color, alt_row_color, button_text_color, disconnect_icon_color,
    disconnect_icon_hover_color, interactive_hover_color, primary_scrollbar_style,
    section_title_color, shaped_text, sidebar_background, sidebar_border, sidebar_empty_color,
    sidebar_icon_color, sidebar_icon_disabled_color, sidebar_icon_hover_color,
    tooltip_container_style,
};
use crate::types::{Message, ServerConnection};
use iced::widget::{Column, Row, Space, button, column, container, row, scrollable, tooltip};
use iced::{Background, Border, Element, Fill};

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
    hover_color: impl Fn(&iced::Theme) -> iced::Color + 'static,
    normal_color: impl Fn(&iced::Theme) -> iced::Color + 'static,
) -> button::Button<'a, Message> {
    button(icon)
        .on_press(message)
        .padding(ICON_BUTTON_PADDING)
        .style(move |theme, status| button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => hover_color(theme),
                _ => normal_color(theme),
            },
            border: Border::default(),
            shadow: iced::Shadow::default(),
        })
}

/// Create a disabled icon button (greyed out)
fn disabled_icon_button(icon: iced::widget::Container<'_, Message>) -> button::Button<'_, Message> {
    button(icon)
        .padding(ICON_BUTTON_PADDING)
        .style(|_theme, _status| button::Style {
            background: None,
            text_color: sidebar_icon_disabled_color(),
            border: Border::default(),
            shadow: iced::Shadow::default(),
        })
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

/// Create a horizontal separator line (soft blue)
fn toolbar_separator<'a>() -> iced::widget::Container<'a, Message> {
    container(Space::new(Fill, SEPARATOR_HEIGHT))
        .width(Fill)
        .height(SEPARATOR_HEIGHT)
        .style(|_theme| container::Style {
            background: Some(Background::Color(interactive_hover_color())),
            ..Default::default()
        })
}

// ============================================================================
// User Toolbar
// ============================================================================

/// Create action toolbar for an expanded user
fn create_user_toolbar<'a>(
    username: &'a str,
    current_username: &'a str,
    target_is_admin: bool,
    current_user_is_admin: bool,
    permissions: &[String],
) -> Row<'a, Message> {
    let username_owned = username.to_string();
    let is_self = username == current_username;

    // Check permissions (admins have all permissions)
    let has_user_info_permission =
        current_user_is_admin || permissions.iter().any(|p| p == PERMISSION_USER_INFO);
    let has_user_message_permission =
        current_user_is_admin || permissions.iter().any(|p| p == PERMISSION_USER_MESSAGE);
    let has_user_kick_permission =
        current_user_is_admin || permissions.iter().any(|p| p == PERMISSION_USER_KICK);

    // Build toolbar row
    let mut toolbar_row = row![].spacing(NO_SPACING).width(Fill);

    // Info button (always show, disabled if no permission)
    let info_icon = icon_container(icon::info());
    let info_button = if has_user_info_permission {
        let username_for_info = username_owned.clone();
        enabled_icon_button(
            info_icon,
            Message::UserInfoIconClicked(username_for_info),
            sidebar_icon_hover_color,
            sidebar_icon_color,
        )
    } else {
        disabled_icon_button(info_icon)
    };
    toolbar_row = toolbar_row.push(with_tooltip(info_button, t("tooltip-info")));

    // Message button (only show if not self)
    if !is_self {
        let message_icon = icon_container(icon::message());
        let message_button = if has_user_message_permission {
            let username_for_message = username_owned.clone();
            enabled_icon_button(
                message_icon,
                Message::UserMessageIconClicked(username_for_message),
                sidebar_icon_hover_color,
                sidebar_icon_color,
            )
        } else {
            disabled_icon_button(message_icon)
        };
        toolbar_row = toolbar_row.push(with_tooltip(message_button, t("tooltip-message")));
    }

    // Kick button (if not self, has permission, and target is not admin)
    if !is_self && has_user_kick_permission && !target_is_admin {
        // Add spacer to push kick button to the right
        toolbar_row = toolbar_row.push(Space::new(Fill, SEPARATOR_HEIGHT));

        let kick_icon = icon_container(icon::kick());
        let kick_button = enabled_icon_button(
            kick_icon,
            Message::UserKickIconClicked(username_owned),
            disconnect_icon_hover_color,
            disconnect_icon_color,
        );
        toolbar_row = toolbar_row.push(with_tooltip(kick_button, t("tooltip-kick")));
    }

    toolbar_row
}

// ============================================================================
// User List Panel
// ============================================================================

/// Displays online users as clickable buttons with expandable action toolbars
///
/// Shows a list of currently connected users. Clicking a username expands it
/// to show an action toolbar underneath. Only one user can be expanded at a time.
/// Admin users are shown in red.
///
/// Note: This panel is only shown when the user has `user_list` permission.
/// Permission checking is done at the layout level.
pub fn user_list_panel(conn: &ServerConnection) -> Element<'_, Message> {
    let current_username = &conn.username;
    let is_admin = conn.is_admin;
    let permissions = &conn.permissions;

    let title = shaped_text(t("title-users"))
        .size(USER_LIST_TITLE_SIZE)
        .style(|theme| iced::widget::text::Style {
            color: Some(section_title_color(theme)),
        });

    let mut users_column = Column::new().spacing(USER_LIST_ITEM_SPACING);

    if conn.online_users.is_empty() {
        users_column = users_column.push(
            shaped_text(t("empty-no-users"))
                .size(USER_LIST_SMALL_TEXT_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(sidebar_empty_color(theme)),
                }),
        );
    } else {
        for (index, user) in conn.online_users.iter().enumerate() {
            let is_expanded = conn.expanded_user.as_ref() == Some(&user.username);
            let is_even = index % 2 == 0;

            // Username button
            let user_is_admin = user.is_admin;
            let username_clone = user.username.clone();

            let user_button = button(
                container(shaped_text(&user.username).size(USER_LIST_TEXT_SIZE))
                    .width(Fill)
                    .align_x(iced::alignment::Horizontal::Left),
            )
            .on_press(Message::UserListItemClicked(username_clone))
            .width(Fill)
            .padding(INPUT_PADDING)
            .style(move |theme, status| button::Style {
                background: None,
                text_color: match status {
                    button::Status::Hovered => interactive_hover_color(),
                    _ => {
                        if user_is_admin {
                            admin_user_text_color(theme)
                        } else {
                            button_text_color(theme)
                        }
                    }
                },
                border: Border::default(),
                shadow: iced::Shadow::default(),
            });

            // Create item column (username + optional toolbar)
            let mut item_column = Column::new().spacing(NO_SPACING);

            // Username button
            item_column = item_column.push(user_button);

            // Add toolbar if expanded
            if is_expanded {
                // Soft blue separator line
                item_column = item_column.push(toolbar_separator());

                // Toolbar
                let toolbar = create_user_toolbar(
                    &user.username,
                    current_username,
                    user.is_admin,
                    is_admin,
                    permissions,
                );
                let toolbar_row = container(toolbar)
                    .width(Fill)
                    .padding(TOOLBAR_CONTAINER_PADDING);
                item_column = item_column.push(toolbar_row);
            }

            // Wrap entire item (username + toolbar) in container with alternating background
            let item_container =
                container(item_column)
                    .width(Fill)
                    .style(move |theme| container::Style {
                        background: if is_even {
                            Some(Background::Color(alt_row_color(theme)))
                        } else {
                            None
                        },
                        ..Default::default()
                    });

            users_column = users_column.push(item_container);
        }
    }

    let panel = column![
        title,
        scrollable(users_column)
            .height(Fill)
            .style(primary_scrollbar_style()),
    ]
    .spacing(USER_LIST_SPACING)
    .padding(FORM_PADDING)
    .width(USER_LIST_PANEL_WIDTH);

    container(panel)
        .height(Fill)
        .style(|theme| container::Style {
            background: Some(Background::Color(sidebar_background(theme))),
            border: Border {
                color: sidebar_border(theme),
                width: BORDER_WIDTH,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

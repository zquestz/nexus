//! User list panel (right sidebar)

use super::style::{
    BORDER_WIDTH, FORM_PADDING, ICON_BUTTON_PADDING_HORIZONTAL, ICON_BUTTON_PADDING_VERTICAL,
    INPUT_PADDING, TOOLBAR_CONTAINER_PADDING_HORIZONTAL, TOOLTIP_BACKGROUND_COLOR,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    USER_LIST_ITEM_SPACING, USER_LIST_PANEL_WIDTH, USER_LIST_SMALL_TEXT_SIZE, USER_LIST_SPACING,
    USER_LIST_TEXT_SIZE, USER_LIST_TITLE_SIZE, admin_user_text_color, alt_row_color,
    button_text_color, disconnect_icon_color, disconnect_icon_hover_color, empty_state_color,
    interactive_hover_color, primary_scrollbar_style, section_title_color, sidebar_background,
    sidebar_border, sidebar_icon_color, sidebar_icon_hover_color, tooltip_border,
    tooltip_text_color,
};
use super::colors::SIDEBAR_ICON_DISABLED;
use crate::icon;
use crate::types::{Message, ServerConnection};
use iced::widget::{
    Column, Row, button, column, container, rich_text, row, scrollable, span, text, tooltip,
};
use iced::{Background, Border, Element, Fill};

/// Icon size for user action toolbar
const ICON_SIZE: f32 = 18.0;

/// Spacing between toolbar icons
const TOOLBAR_SPACING: f32 = 0.0;

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
    let title = text("Users")
        .size(USER_LIST_TITLE_SIZE)
        .style(|theme| iced::widget::text::Style {
            color: Some(section_title_color(theme)),
        });

    let mut users_column = Column::new().spacing(USER_LIST_ITEM_SPACING);

    if conn.online_users.is_empty() {
        users_column = users_column.push(
            text("No users online")
                .size(USER_LIST_SMALL_TEXT_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(empty_state_color(theme)),
                }),
        );
    } else {
        for (index, user) in conn.online_users.iter().enumerate() {
            let is_expanded = conn.expanded_user.as_ref() == Some(&user.username);
            let is_even = index % 2 == 0;

            // Username button with optional underline when expanded
            let username_text = if is_expanded {
                rich_text![span(&user.username).underline(true)]
            } else {
                rich_text![span(&user.username)]
            };
            let user_is_admin = user.is_admin;
            let username_clone = user.username.clone();

            let user_button = button(username_text.size(USER_LIST_TEXT_SIZE))
                .on_press(Message::UserListItemClicked(username_clone))
                .width(Fill)
                .padding(if is_expanded {
                    iced::Padding {
                        top: INPUT_PADDING as f32,
                        right: INPUT_PADDING as f32,
                        bottom: 0.0, // No bottom padding when expanded (toolbar appears below)
                        left: INPUT_PADDING as f32,
                    }
                } else {
                    INPUT_PADDING.into()
                })
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
            let mut item_column = Column::new().spacing(0);

            // Username button
            item_column = item_column.push(user_button);

            // Add toolbar if expanded
            if is_expanded {
                // Toolbar
                let toolbar = create_user_toolbar(
                    &user.username,
                    current_username,
                    user.is_admin,
                    is_admin,
                    permissions,
                );
                let toolbar_row = container(toolbar).width(Fill).padding(iced::Padding {
                    top: 0.0, // Flush with username button above
                    right: TOOLBAR_CONTAINER_PADDING_HORIZONTAL as f32,
                    bottom: 0.0, // Flush with bottom of item
                    left: TOOLBAR_CONTAINER_PADDING_HORIZONTAL as f32,
                });
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

/// Create action toolbar for an expanded user
fn create_user_toolbar<'a>(
    username: &'a str,
    current_username: &'a str,
    target_is_admin: bool,
    current_user_is_admin: bool,
    permissions: &[String],
) -> Row<'a, Message> {
    let username_clone = username.to_string();
    let is_self = username == current_username;

    // Check permissions (admins have all permissions)
    let has_message_permission =
        current_user_is_admin || permissions.contains(&"user_message".to_string());
    let has_kick_permission =
        current_user_is_admin || permissions.contains(&"user_kick".to_string());

    // Square button size
    let button_size = ICON_SIZE;

    // Info icon button (square)
    let info_icon = container(icon::info().size(ICON_SIZE))
        .width(button_size)
        .height(button_size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
    let info_button = tooltip(
        button(info_icon)
            .on_press(Message::UserInfoIconClicked(username_clone.clone()))
            .padding(iced::Padding {
                top: ICON_BUTTON_PADDING_VERTICAL as f32,
                right: ICON_BUTTON_PADDING_HORIZONTAL as f32,
                bottom: ICON_BUTTON_PADDING_VERTICAL as f32,
                left: ICON_BUTTON_PADDING_HORIZONTAL as f32,
            })
            .style(|theme, status| button::Style {
                background: None,
                text_color: match status {
                    button::Status::Hovered => sidebar_icon_hover_color(theme),
                    _ => sidebar_icon_color(theme),
                },
                border: Border::default(),
                shadow: iced::Shadow::default(),
            }),
        container(text("Info").size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(|theme| container::Style {
                background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                text_color: Some(tooltip_text_color(theme)),
                border: tooltip_border(),
                ..Default::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    // Message icon button (square) - TODO: Implement private messaging
    let message_icon = container(icon::message().size(ICON_SIZE))
        .width(button_size)
        .height(button_size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
    let message_button = if has_message_permission {
        // Enabled button
        tooltip(
            button(message_icon)
                .on_press(Message::UserMessageIconClicked(username_clone.clone()))
                .padding(iced::Padding {
                    top: ICON_BUTTON_PADDING_VERTICAL as f32,
                    right: ICON_BUTTON_PADDING_HORIZONTAL as f32,
                    bottom: ICON_BUTTON_PADDING_VERTICAL as f32,
                    left: ICON_BUTTON_PADDING_HORIZONTAL as f32,
                })
                .style(|theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => sidebar_icon_hover_color(theme),
                        _ => sidebar_icon_color(theme),
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                }),
            container(text("Message").size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(|theme| container::Style {
                    background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                    text_color: Some(tooltip_text_color(theme)),
                    border: tooltip_border(),
                    ..Default::default()
                }),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
    } else {
        // Disabled button (no permission)
        tooltip(
            button(message_icon)
                .padding(iced::Padding {
                    top: ICON_BUTTON_PADDING_VERTICAL as f32,
                    right: ICON_BUTTON_PADDING_HORIZONTAL as f32,
                    bottom: ICON_BUTTON_PADDING_VERTICAL as f32,
                    left: ICON_BUTTON_PADDING_HORIZONTAL as f32,
                })
                .style(|_theme, _status| button::Style {
                    background: None,
                    text_color: SIDEBAR_ICON_DISABLED,
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                }),
            container(text("Message").size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(|theme| container::Style {
                    background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                    text_color: Some(tooltip_text_color(theme)),
                    border: tooltip_border(),
                    ..Default::default()
                }),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
    };

    // Kick icon button (square) - TODO: Implement kick/disconnect
    let kick_icon = container(icon::kick().size(ICON_SIZE))
        .width(button_size)
        .height(button_size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
    let kick_button = tooltip(
        button(kick_icon)
            .on_press(Message::UserKickIconClicked(username_clone))
            .padding(iced::Padding {
                top: ICON_BUTTON_PADDING_VERTICAL as f32,
                right: ICON_BUTTON_PADDING_HORIZONTAL as f32,
                bottom: ICON_BUTTON_PADDING_VERTICAL as f32,
                left: ICON_BUTTON_PADDING_HORIZONTAL as f32,
            })
            .style(|theme, status| button::Style {
                background: None,
                text_color: match status {
                    button::Status::Hovered => disconnect_icon_hover_color(theme),
                    _ => disconnect_icon_color(theme),
                },
                border: Border::default(),
                shadow: iced::Shadow::default(),
            }),
        container(text("Kick").size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(|theme| container::Style {
                background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                text_color: Some(tooltip_text_color(theme)),
                border: tooltip_border(),
                ..Default::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    // Build toolbar based on permissions and whether viewing self
    let mut toolbar_row = row![].spacing(TOOLBAR_SPACING).width(Fill);

    // Message button (always show, disabled if no permission)
    toolbar_row = toolbar_row.push(message_button);

    // Info button (always show)
    toolbar_row = toolbar_row.push(info_button);

    // Kick button (if not self, has permission, and target is not admin)
    if !is_self && has_kick_permission && !target_is_admin {
        toolbar_row = toolbar_row.push(container(text("")).width(Fill)); // Spacer
        toolbar_row = toolbar_row.push(kick_button);
    }

    toolbar_row
}

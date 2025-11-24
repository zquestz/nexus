//! User list panel (right sidebar)

use crate::types::{Message, ServerConnection};
use iced::widget::{Column, button, column, container, scrollable, text};
use iced::{Element, Fill};

/// Displays online users as clickable buttons
pub fn user_list_panel<'a>(conn: &'a ServerConnection) -> Element<'a, Message> {
    // Check if user has permission to view user list
    let can_view_users = conn.is_admin || conn.permissions.contains(&"user_list".to_string());

    let title = text("Users").size(16);

    let mut users_column = Column::new().spacing(3).padding(5);

    if !can_view_users {
        // No permission to view user list
        users_column = users_column.push(
            text("No permission to view users")
                .size(11)
                .color([0.7, 0.7, 0.7]),
        );
    } else if conn.online_users.is_empty() {
        users_column = users_column.push(text("No users online").size(11).color([0.5, 0.5, 0.5]));
    } else {
        for user in &conn.online_users {
            // Bold text for admins
            let username_text = if user.is_admin {
                text(&user.username).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::default()
                })
            } else {
                text(&user.username)
            };

            users_column = users_column.push(
                button(username_text.size(12))
                    .on_press(Message::RequestUserInfo(user.session_id))
                    .width(Fill)
                    .padding(6),
            );
        }
    }

    let panel = column![title, scrollable(users_column).height(Fill),]
        .spacing(8)
        .padding(10)
        .width(180);

    container(panel)
        .height(Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb(
                0.12, 0.12, 0.12,
            ))),
            border: iced::Border {
                color: iced::Color::from_rgb(0.2, 0.2, 0.2),
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

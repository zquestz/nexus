//! User list panel (right sidebar)

use crate::types::{Message, UserInfo};
use iced::widget::{button, column, container, scrollable, text, Column};
use iced::{Element, Fill};

/// Displays online users as clickable buttons
pub fn user_list_panel<'a>(users: &'a [UserInfo], _bookmark_index: usize) -> Element<'a, Message> {
    let title = text("Users").size(16);

    let mut users_column = Column::new().spacing(3).padding(5);

    if users.is_empty() {
        users_column = users_column.push(text("No users online").size(11).color([0.5, 0.5, 0.5]));
    } else {
        for user in users {
            users_column = users_column.push(
                button(text(&user.username).size(12))
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
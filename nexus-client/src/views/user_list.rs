//! User list panel (right sidebar)

use super::style::{
    BORDER_WIDTH, EMPTY_STATE_COLOR, FORM_PADDING, INPUT_PADDING, SERVER_LIST_BACKGROUND_COLOR,
    SERVER_LIST_BORDER_COLOR, USER_LIST_ITEM_SPACING,
    USER_LIST_PANEL_WIDTH, USER_LIST_SMALL_TEXT_SIZE, USER_LIST_SPACING, USER_LIST_TEXT_SIZE,
    USER_LIST_TITLE_SIZE, BOOKMARK_ROW_ALT_COLOR, BOOKMARK_BUTTON_HOVER_COLOR,
};
use crate::types::{Message, ServerConnection};
use iced::widget::{button, column, container, scrollable, text, Column};
use iced::{Background, Border, Color, Element, Fill};

/// Displays online users as clickable buttons
///
/// Shows a list of currently connected users. Each username is clickable to
/// request detailed user information. Admin users are shown in bold.
///
/// Note: This panel is only shown when the user has `user_list` permission.
/// Permission checking is done at the layout level.
pub fn user_list_panel<'a>(conn: &'a ServerConnection) -> Element<'a, Message> {
    let title = text("Users").size(USER_LIST_TITLE_SIZE);

    let mut users_column = Column::new()
        .spacing(USER_LIST_ITEM_SPACING);

    if conn.online_users.is_empty() {
        users_column = users_column.push(
            text("No users online")
                .size(USER_LIST_SMALL_TEXT_SIZE)
                .color(EMPTY_STATE_COLOR),
        );
    } else {
        for (index, user) in conn.online_users.iter().enumerate() {
            // Bold text for admins
            let username_text = if user.is_admin {
                text(&user.username).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::default()
                })
            } else {
                text(&user.username)
            };

            // Transparent button with hover effect
            let user_button = button(username_text.size(USER_LIST_TEXT_SIZE))
                .on_press(Message::RequestUserInfo(user.username.clone()))
                .width(Fill)
                .padding(INPUT_PADDING)
                .style(|_theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => BOOKMARK_BUTTON_HOVER_COLOR,
                        _ => Color::WHITE,
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                });

            // Alternating row backgrounds
            let is_even = index % 2 == 0;
            let row_container = container(user_button)
                .width(Fill)
                .style(move |_theme| container::Style {
                    background: if is_even {
                        Some(Background::Color(BOOKMARK_ROW_ALT_COLOR))
                    } else {
                        None
                    },
                    ..Default::default()
                });

            users_column = users_column.push(row_container);
        }
    }

    let panel = column![title, scrollable(users_column).height(Fill),]
        .spacing(USER_LIST_SPACING)
        .padding(FORM_PADDING)
        .width(USER_LIST_PANEL_WIDTH);

    container(panel)
        .height(Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(SERVER_LIST_BACKGROUND_COLOR)),
            border: Border {
                color: SERVER_LIST_BORDER_COLOR,
                width: BORDER_WIDTH,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

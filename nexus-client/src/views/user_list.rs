//! User list panel (right sidebar)

use super::style::{
    BORDER_WIDTH, FORM_PADDING, INPUT_PADDING, USER_LIST_ITEM_SPACING, USER_LIST_PANEL_WIDTH,
    USER_LIST_SMALL_TEXT_SIZE, USER_LIST_SPACING, USER_LIST_TEXT_SIZE, USER_LIST_TITLE_SIZE,
    admin_user_text_color, alt_row_color, button_text_color, empty_state_color,
    interactive_hover_color, section_title_color, sidebar_background, sidebar_border,
};
use crate::types::{Message, ServerConnection};
use iced::widget::{Column, button, column, container, scrollable, text};
use iced::{Background, Border, Element, Fill};

/// Displays online users as clickable buttons
///
/// Shows a list of currently connected users. Each username is clickable to
/// request detailed user information. Admin users are shown in red.
///
/// Note: This panel is only shown when the user has `user_list` permission.
/// Permission checking is done at the layout level.
pub fn user_list_panel<'a>(conn: &'a ServerConnection) -> Element<'a, Message> {
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
            // Username text
            let username_text = text(&user.username);

            // Transparent button with hover effect
            let user_is_admin = user.is_admin;
            let user_button = button(username_text.size(USER_LIST_TEXT_SIZE))
                .on_press(Message::RequestUserInfo(user.username.clone()))
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

            // Alternating row backgrounds
            let is_even = index % 2 == 0;
            let row_container =
                container(user_button)
                    .width(Fill)
                    .style(move |theme| container::Style {
                        background: if is_even {
                            Some(Background::Color(alt_row_color(theme)))
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

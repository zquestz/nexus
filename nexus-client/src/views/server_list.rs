//! Server list panel (left sidebar)

use crate::types::{Message, ServerBookmark, ServerConnection};
use iced::widget::{button, column, container, row, scrollable, text, Column};
use iced::{Element, Fill};
use std::collections::HashMap;

/// Displays connected servers and saved bookmarks
pub fn server_list_panel<'a>(
    bookmarks: &'a [ServerBookmark],
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
) -> Element<'a, Message> {
    let mut main_column = Column::new().spacing(0);

    // === CONNECTED SERVERS SECTION ===
    let connected_title = text("Connected").size(14).color([0.7, 0.7, 0.7]);
    let mut connected_column = Column::new().spacing(3).padding(5);

    if connections.is_empty() {
        connected_column =
            connected_column.push(text("No connections").size(11).color([0.4, 0.4, 0.4]));
    } else {
        // Sort connections by connection_id for consistent ordering
        let mut conn_list: Vec<_> = connections.iter().collect();
        conn_list.sort_by_key(|(id, _)| **id);

        for (conn_id, conn) in conn_list {
            let is_active = active_connection == Some(*conn_id);

            let display = conn.display_name.clone();
            let mut btn = button(text(display).size(13)).width(Fill).padding(6);

            if is_active {
                btn = btn.style(|theme, status| button::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgb(
                        0.3, 0.4, 0.5,
                    ))),
                    text_color: iced::Color::WHITE,
                    ..button::primary(theme, status)
                });
            }

            btn = btn
                .on_press(Message::SwitchToConnection(*conn_id))
                .height(32);

            // Add disconnect button with matching height (square)
            let disconnect_btn = button(
                container(text("X").size(11))
                    .center_x(32)
                    .center_y(32)
            )
                .on_press(Message::DisconnectFromServer(*conn_id))
                .width(32)
                .height(32)
                .padding(0);

            let server_row = row![btn, disconnect_btn]
                .spacing(5)
                .align_y(iced::alignment::Vertical::Center);

            connected_column = connected_column.push(server_row);
        }
    }

    let connected_section = column![connected_title, connected_column,]
        .spacing(5)
        .padding(10);

    main_column = main_column.push(connected_section);

    // Separator line
    let separator = container(text(""))
        .width(Fill)
        .height(1)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb(
                0.3, 0.3, 0.3,
            ))),
            ..Default::default()
        });
    main_column = main_column.push(separator);

    // === BOOKMARKS SECTION ===
    let bookmarks_title = text("Bookmarks").size(14).color([0.7, 0.7, 0.7]);
    let mut bookmarks_column = Column::new().spacing(3).padding(5);

    if bookmarks.is_empty() {
        bookmarks_column =
            bookmarks_column.push(text("No bookmarks").size(11).color([0.4, 0.4, 0.4]));
    } else {
        for (index, bookmark) in bookmarks.iter().enumerate() {
            // Check if this bookmark is currently connected
            let connected_to_bookmark = connections
                .values()
                .any(|conn| conn.bookmark_index == Some(index));

            let btn = button(text(&bookmark.name).size(13))
                .width(Fill)
                .height(32)
                .padding(6)
                .on_press(if connected_to_bookmark {
                    // Find the connection_id for this bookmark and switch to it
                    let conn_id = connections
                        .iter()
                        .find(|(_, conn)| conn.bookmark_index == Some(index))
                        .map(|(id, _)| *id)
                        .unwrap_or(0);
                    Message::SwitchToConnection(conn_id)
                } else {
                    Message::ConnectToBookmark(index)
                });

            // Add action buttons with matching height (square)
            let edit_btn = button(
                container(text("Edit").size(11))
                    .center_x(32)
                    .center_y(32)
            )
                .on_press(Message::ShowEditBookmark(index))
                .width(32)
                .height(32)
                .padding(0);
                
            let delete_btn = button(
                container(text("Del").size(11))
                    .center_x(32)
                    .center_y(32)
            )
                .on_press(Message::DeleteBookmark(index))
                .width(32)
                .height(32)
                .padding(0);

            let bookmark_row = row![btn, edit_btn, delete_btn]
                .spacing(3)
                .align_y(iced::alignment::Vertical::Center);

            bookmarks_column = bookmarks_column.push(bookmark_row);
        }
    }

    let add_btn = button(text("+ Add Bookmark").size(12))
        .on_press(Message::ShowAddBookmark)
        .padding(8)
        .width(Fill);

    let bookmarks_section = column![
        bookmarks_title,
        scrollable(bookmarks_column).height(Fill),
        add_btn,
    ]
    .spacing(5)
    .padding(10);

    main_column = main_column.push(bookmarks_section);

    // Wrap in container with styling
    container(main_column)
        .height(Fill)
        .width(220)
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
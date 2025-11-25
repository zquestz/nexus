//! Server list panel (left sidebar)

use super::style::{
    ACTIVE_CONNECTION_COLOR, BORDER_WIDTH, EMPTY_STATE_COLOR, FORM_PADDING, INPUT_PADDING,
    PANEL_SPACING, SECTION_TITLE_COLOR, SECTION_TITLE_SIZE, SEPARATOR_COLOR, SEPARATOR_HEIGHT,
    SERVER_LIST_BACKGROUND_COLOR, SERVER_LIST_BORDER_COLOR, SERVER_LIST_BUTTON_HEIGHT,
    SERVER_LIST_BUTTON_SIZE, SERVER_LIST_ICON_BUTTON_SIZE, SERVER_LIST_ITEM_SPACING,
    SERVER_LIST_PANEL_WIDTH, SERVER_LIST_SECTION_SPACING, SERVER_LIST_SMALL_TEXT_SIZE,
    SERVER_LIST_TEXT_SIZE, SMALL_PADDING,
};
use crate::types::{Message, ServerBookmark, ServerConnection};
use iced::widget::{button, column, container, row, scrollable, text, Column};
use iced::{alignment, Background, Border, Color, Element, Fill};
use std::collections::HashMap;

/// Displays connected servers and saved bookmarks
///
/// Shows two sections: connected servers (top) and bookmarks (bottom).
/// Connected servers can be switched between or disconnected. Bookmarks can be
/// connected to, edited, or deleted. A separator divides the two sections.
pub fn server_list_panel<'a>(
    bookmarks: &'a [ServerBookmark],
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
) -> Element<'a, Message> {
    let mut main_column = Column::new().spacing(PANEL_SPACING);

    // === CONNECTED SERVERS SECTION ===
    let connected_title = text("Connected")
        .size(SECTION_TITLE_SIZE)
        .color(SECTION_TITLE_COLOR);
    let mut connected_column = Column::new()
        .spacing(SERVER_LIST_ITEM_SPACING)
        .padding(SMALL_PADDING);

    if connections.is_empty() {
        connected_column = connected_column.push(
            text("No connections")
                .size(SERVER_LIST_SMALL_TEXT_SIZE)
                .color(EMPTY_STATE_COLOR),
        );
    } else {
        // Sort connections by connection_id for consistent ordering
        let mut conn_list: Vec<_> = connections.iter().collect();
        conn_list.sort_by_key(|(id, _)| **id);

        for (conn_id, conn) in conn_list {
            let is_active = active_connection == Some(*conn_id);

            let mut btn = button(text(&conn.display_name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING);

            if is_active {
                btn = btn.style(|theme, status| button::Style {
                    background: Some(Background::Color(ACTIVE_CONNECTION_COLOR)),
                    text_color: Color::WHITE,
                    ..button::primary(theme, status)
                });
            }

            btn = btn.on_press(Message::SwitchToConnection(*conn_id));

            // Disconnect button (square icon button)
            let disconnect_btn = icon_button("X", Message::DisconnectFromServer(*conn_id));

            let server_row = row![btn, disconnect_btn]
                .spacing(SERVER_LIST_ITEM_SPACING)
                .align_y(alignment::Vertical::Center);

            connected_column = connected_column.push(server_row);
        }
    }

    let connected_section = column![connected_title, connected_column,]
        .spacing(SERVER_LIST_SECTION_SPACING)
        .padding(FORM_PADDING);

    main_column = main_column.push(connected_section);

    // Separator line
    let separator = container(text(""))
        .width(Fill)
        .height(SEPARATOR_HEIGHT)
        .style(|_theme| container::Style {
            background: Some(Background::Color(SEPARATOR_COLOR)),
            ..Default::default()
        });
    main_column = main_column.push(separator);

    // === BOOKMARKS SECTION ===
    let bookmarks_title = text("Bookmarks")
        .size(SECTION_TITLE_SIZE)
        .color(SECTION_TITLE_COLOR);
    let mut bookmarks_column = Column::new()
        .spacing(SERVER_LIST_ITEM_SPACING)
        .padding(SMALL_PADDING);

    if bookmarks.is_empty() {
        bookmarks_column = bookmarks_column.push(
            text("No bookmarks")
                .size(SERVER_LIST_SMALL_TEXT_SIZE)
                .color(EMPTY_STATE_COLOR),
        );
    } else {
        for (index, bookmark) in bookmarks.iter().enumerate() {
            // Determine message based on whether bookmark is currently connected
            let bookmark_message = if let Some(conn_id) = connections
                .iter()
                .find(|(_, conn)| conn.bookmark_index == Some(index))
                .map(|(id, _)| *id)
            {
                // Bookmark is connected - switch to it
                Message::SwitchToConnection(conn_id)
            } else {
                // Not connected - connect to it
                Message::ConnectToBookmark(index)
            };

            let btn = button(text(&bookmark.name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING)
                .on_press(bookmark_message);

            // Action button (square icon button)
            let edit_btn = icon_button("Edit", Message::ShowEditBookmark(index));

            let bookmark_row = row![btn, edit_btn]
                .spacing(SERVER_LIST_ITEM_SPACING)
                .align_y(alignment::Vertical::Center);

            bookmarks_column = bookmarks_column.push(bookmark_row);
        }
    }

    let add_btn = button(text("+ Add Bookmark").size(SERVER_LIST_BUTTON_SIZE))
        .on_press(Message::ShowAddBookmark)
        .padding(INPUT_PADDING)
        .width(Fill);

    let bookmarks_section = column![
        bookmarks_title,
        scrollable(bookmarks_column).height(Fill),
        add_btn,
    ]
    .spacing(SERVER_LIST_SECTION_SPACING)
    .padding(FORM_PADDING);

    main_column = main_column.push(bookmarks_section);

    // Wrap in container with styling
    container(main_column)
        .height(Fill)
        .width(SERVER_LIST_PANEL_WIDTH)
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

/// Helper function to create square icon buttons
fn icon_button<'a>(label: &'a str, message: Message) -> button::Button<'a, Message> {
    button(
        container(text(label).size(SERVER_LIST_SMALL_TEXT_SIZE))
            .center_x(SERVER_LIST_ICON_BUTTON_SIZE)
            .center_y(SERVER_LIST_ICON_BUTTON_SIZE),
    )
    .on_press(message)
    .width(SERVER_LIST_ICON_BUTTON_SIZE)
    .height(SERVER_LIST_ICON_BUTTON_SIZE)
    .padding(0)
}
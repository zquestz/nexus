//! Server list panel (left sidebar)

use super::style::{
    BORDER_WIDTH, EMPTY_STATE_COLOR, FORM_PADDING, INPUT_PADDING,
    PANEL_SPACING, SECTION_TITLE_COLOR, SECTION_TITLE_SIZE, SEPARATOR_COLOR, SEPARATOR_HEIGHT,
    SERVER_LIST_BACKGROUND_COLOR, SERVER_LIST_BORDER_COLOR, SERVER_LIST_BUTTON_HEIGHT,
    SERVER_LIST_BUTTON_SIZE, SERVER_LIST_DISCONNECT_ICON_SIZE,
    SERVER_LIST_ITEM_SPACING, SERVER_LIST_PANEL_WIDTH,
    SERVER_LIST_SECTION_SPACING, SERVER_LIST_SMALL_TEXT_SIZE, SERVER_LIST_TEXT_SIZE,
    TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, TOOLTIP_BACKGROUND_PADDING,
    TOOLTIP_BACKGROUND_COLOR, NO_SPACING,
    DISCONNECT_ICON_COLOR, DISCONNECT_ICON_HOVER_COLOR, EDIT_ICON_COLOR, EDIT_ICON_HOVER_COLOR,
    BOOKMARK_ROW_ALT_COLOR, BOOKMARK_BUTTON_HOVER_COLOR,
};
use crate::icon;
use crate::types::{Message, ServerBookmark, ServerConnection};
use iced::widget::{button, column, container, row, scrollable, text, tooltip, Column};
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
        .spacing(SERVER_LIST_ITEM_SPACING);

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

        for (index, (conn_id, conn)) in conn_list.iter().enumerate() {
            let is_active = active_connection == Some(**conn_id);

            // Transparent button with hover effect and blue text for active
            let btn = button(text(&conn.display_name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING)
                .on_press(Message::SwitchToConnection(**conn_id))
                .style(move |_theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => BOOKMARK_BUTTON_HOVER_COLOR,
                        _ if is_active => BOOKMARK_BUTTON_HOVER_COLOR,
                        _ => Color::WHITE,
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                });

            // Disconnect button (transparent icon button with hover effect)
            let disconnect_btn = tooltip(
                transparent_icon_button(icon::logout(), Message::DisconnectFromServer(**conn_id)),
                container(text("Disconnect").size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                        ..Default::default()
                    }),
                tooltip::Position::Right,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING);

            let server_row = row![btn, disconnect_btn]
                .spacing(NO_SPACING)
                .align_y(alignment::Vertical::Center);

            // Alternating row backgrounds
            let is_even = index % 2 == 0;
            let row_container = container(server_row)
                .width(Fill)
                .style(move |_theme| container::Style {
                    background: if is_even {
                        Some(Background::Color(BOOKMARK_ROW_ALT_COLOR))
                    } else {
                        None
                    },
                    ..Default::default()
                });

            connected_column = connected_column.push(row_container);
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
        .spacing(SERVER_LIST_ITEM_SPACING);

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

            // Transparent button with hover effect
            let btn = button(text(&bookmark.name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING)
                .on_press(bookmark_message)
                .style(|_theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => BOOKMARK_BUTTON_HOVER_COLOR,
                        _ => Color::WHITE,
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                });

            // Action button (transparent icon button with hover effect)
            let edit_btn = tooltip(
                transparent_edit_button(icon::cog(), Message::ShowEditBookmark(index)),
                container(text("Edit").size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                        ..Default::default()
                    }),
                tooltip::Position::Right,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING);

            let bookmark_row = row![btn, edit_btn]
                .spacing(NO_SPACING)
                .align_y(alignment::Vertical::Center);

            // Alternating row backgrounds
            let is_even = index % 2 == 0;
            let row_container = container(bookmark_row)
                .width(Fill)
                .style(move |_theme| container::Style {
                    background: if is_even {
                        Some(Background::Color(BOOKMARK_ROW_ALT_COLOR))
                    } else {
                        None
                    },
                    ..Default::default()
                });

            bookmarks_column = bookmarks_column.push(row_container);
        }
    }

    let add_btn = button(text("Add Bookmark").size(SERVER_LIST_BUTTON_SIZE))
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



/// Helper function to create transparent icon buttons with hover color change
fn transparent_icon_button<'a>(
    icon: iced::widget::Text<'a>,
    message: Message,
) -> button::Button<'a, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(|_theme, status| button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => DISCONNECT_ICON_HOVER_COLOR,
                _ => DISCONNECT_ICON_COLOR,
            },
            border: Border::default(),
            shadow: iced::Shadow::default(),
        })
}

/// Helper function to create transparent edit/cog buttons with hover color change
fn transparent_edit_button<'a>(
    icon: iced::widget::Text<'a>,
    message: Message,
) -> button::Button<'a, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(|_theme, status| button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => EDIT_ICON_HOVER_COLOR,
                _ => EDIT_ICON_COLOR,
            },
            border: Border::default(),
            shadow: iced::Shadow::default(),
        })
}
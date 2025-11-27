//! Server list panel (left sidebar)

use super::style::{
    BORDER_WIDTH, FORM_PADDING, INPUT_PADDING, NO_SPACING, PANEL_SPACING, SECTION_TITLE_SIZE,
    SEPARATOR_HEIGHT, SERVER_LIST_BUTTON_HEIGHT, SERVER_LIST_BUTTON_SIZE,
    SERVER_LIST_DISCONNECT_ICON_SIZE, SERVER_LIST_ITEM_SPACING, SERVER_LIST_PANEL_WIDTH,
    SERVER_LIST_SECTION_SPACING, SERVER_LIST_SMALL_TEXT_SIZE, SERVER_LIST_TEXT_SIZE,
    TOOLTIP_BACKGROUND_COLOR, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING,
    TOOLTIP_TEXT_SIZE, alt_row_color, button_text_color, disconnect_icon_color,
    disconnect_icon_hover_color, edit_icon_color, edit_icon_hover_color, empty_state_color,
    interactive_hover_color, primary_button_style, primary_scrollbar_style, section_title_color,
    separator_color, sidebar_background, sidebar_border, tooltip_border, tooltip_text_color,
};
use crate::icon;
use crate::types::{Message, ServerBookmark, ServerConnection};
use iced::widget::{Column, button, column, container, row, scrollable, text, tooltip};
use iced::{Background, Border, Element, Fill, alignment};
use std::collections::HashMap;

// UI text constants
const TITLE_CONNECTED: &str = "Connected";
const TITLE_BOOKMARKS: &str = "Bookmarks";
const EMPTY_NO_CONNECTIONS: &str = "No connections";
const EMPTY_NO_BOOKMARKS: &str = "No bookmarks";
const TOOLTIP_DISCONNECT: &str = "Disconnect";
const TOOLTIP_EDIT: &str = "Edit";
const BUTTON_ADD_BOOKMARK: &str = "Add Bookmark";

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
    let connected_title =
        text(TITLE_CONNECTED)
            .size(SECTION_TITLE_SIZE)
            .style(|theme| iced::widget::text::Style {
                color: Some(section_title_color(theme)),
            });
    let mut connected_column = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

    if connections.is_empty() {
        connected_column = connected_column.push(
            text(EMPTY_NO_CONNECTIONS)
                .size(SERVER_LIST_SMALL_TEXT_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(empty_state_color(theme)),
                }),
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
                .style(move |theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => interactive_hover_color(),
                        _ if is_active => interactive_hover_color(),
                        _ => button_text_color(theme),
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                });

            // Disconnect button (transparent icon button with hover effect)
            let disconnect_btn = tooltip(
                transparent_icon_button(icon::logout(), Message::DisconnectFromServer(**conn_id)),
                container(text(TOOLTIP_DISCONNECT).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(|theme| container::Style {
                        background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                        text_color: Some(tooltip_text_color(theme)),
                        border: tooltip_border(),
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
            let row_container =
                container(server_row)
                    .width(Fill)
                    .style(move |theme| container::Style {
                        background: if is_even {
                            Some(Background::Color(alt_row_color(theme)))
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
        .style(|theme| container::Style {
            background: Some(Background::Color(separator_color(theme))),
            ..Default::default()
        });
    main_column = main_column.push(separator);

    // === BOOKMARKS SECTION ===
    let bookmarks_title =
        text(TITLE_BOOKMARKS)
            .size(SECTION_TITLE_SIZE)
            .style(|theme| iced::widget::text::Style {
                color: Some(section_title_color(theme)),
            });
    let mut bookmarks_column = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

    if bookmarks.is_empty() {
        bookmarks_column = bookmarks_column.push(
            text(EMPTY_NO_BOOKMARKS)
                .size(SERVER_LIST_SMALL_TEXT_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(empty_state_color(theme)),
                }),
        );
    } else {
        for (index, bookmark) in bookmarks.iter().enumerate() {
            // Check if this bookmark is currently connected
            let is_connected = connections
                .iter()
                .any(|(_, conn)| conn.bookmark_index == Some(index));

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
                .style(move |theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => interactive_hover_color(),
                        _ if is_connected => interactive_hover_color(),
                        _ => button_text_color(theme),
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                });

            // Action button (transparent icon button with hover effect)
            let edit_btn = tooltip(
                transparent_edit_button(icon::cog(), Message::ShowEditBookmark(index)),
                container(text(TOOLTIP_EDIT).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(|theme| container::Style {
                        background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                        text_color: Some(tooltip_text_color(theme)),
                        border: tooltip_border(),
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
            let row_container =
                container(bookmark_row)
                    .width(Fill)
                    .style(move |theme| container::Style {
                        background: if is_even {
                            Some(Background::Color(alt_row_color(theme)))
                        } else {
                            None
                        },
                        ..Default::default()
                    });

            bookmarks_column = bookmarks_column.push(row_container);
        }
    }

    let add_btn = button(text(BUTTON_ADD_BOOKMARK).size(SERVER_LIST_BUTTON_SIZE))
        .on_press(Message::ShowAddBookmark)
        .padding(INPUT_PADDING)
        .style(primary_button_style())
        .width(Fill);

    let bookmarks_section = column![
        bookmarks_title,
        scrollable(bookmarks_column)
            .height(Fill)
            .style(primary_scrollbar_style()),
        add_btn,
    ]
    .spacing(SERVER_LIST_SECTION_SPACING)
    .padding(FORM_PADDING);

    main_column = main_column.push(bookmarks_section);

    // Wrap in container with styling
    container(main_column)
        .height(Fill)
        .width(SERVER_LIST_PANEL_WIDTH)
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

/// Helper function to create transparent icon buttons with hover color change
fn transparent_icon_button(
    icon: iced::widget::Text<'_>,
    message: Message,
) -> button::Button<'_, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(|theme, status| button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => disconnect_icon_hover_color(theme),
                _ => disconnect_icon_color(theme),
            },
            border: Border::default(),
            shadow: iced::Shadow::default(),
        })
}

/// Helper function to create transparent edit/cog buttons with hover color change
fn transparent_edit_button(
    icon: iced::widget::Text<'_>,
    message: Message,
) -> button::Button<'_, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(|theme, status| button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => edit_icon_hover_color(theme),
                _ => edit_icon_color(theme),
            },
            border: Border::default(),
            shadow: iced::Shadow::default(),
        })
}

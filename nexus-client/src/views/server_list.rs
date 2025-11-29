//! Server list panel (left sidebar)

use super::style::{
    BORDER_WIDTH, FORM_PADDING, ICON_BUTTON_PADDING_HORIZONTAL, ICON_BUTTON_PADDING_VERTICAL,
    INPUT_PADDING, NO_SPACING, PANEL_SPACING, SECTION_TITLE_SIZE, SEPARATOR_HEIGHT,
    SERVER_LIST_BUTTON_HEIGHT, SERVER_LIST_DISCONNECT_ICON_SIZE, SERVER_LIST_ITEM_SPACING,
    SERVER_LIST_PANEL_WIDTH, SERVER_LIST_SECTION_SPACING, SERVER_LIST_SMALL_TEXT_SIZE,
    SERVER_LIST_TEXT_SIZE, TOOLTIP_BACKGROUND_COLOR, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, alt_row_color, bookmark_error_color, button_text_color,
    disconnect_icon_color, disconnect_icon_hover_color, edit_icon_color, edit_icon_hover_color,
    empty_state_color, interactive_hover_color, primary_scrollbar_style, section_title_color,
    separator_color, shaped_text, sidebar_background, sidebar_border, sidebar_icon_color,
    sidebar_icon_hover_color, tooltip_border, tooltip_text_color,
};
use crate::i18n::t;
use crate::icon;
use crate::types::{Message, ServerBookmark, ServerConnection};
use iced::widget::{Column, button, column, container, row, scrollable, tooltip};
use iced::{Background, Border, Element, Fill, alignment};
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
    bookmark_errors: &'a HashMap<usize, String>,
) -> Element<'a, Message> {
    let mut main_column = Column::new().spacing(PANEL_SPACING);

    // === CONNECTED SERVERS SECTION ===
    let connected_title = shaped_text(t("title-connected"))
        .size(SECTION_TITLE_SIZE)
        .style(|theme| iced::widget::text::Style {
            color: Some(section_title_color(theme)),
        });
    let mut connected_column = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

    if connections.is_empty() {
        connected_column = connected_column.push(
            shaped_text(t("empty-no-connections"))
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
            let btn = button(shaped_text(&conn.display_name).size(SERVER_LIST_TEXT_SIZE))
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
                container(shaped_text(t("tooltip-disconnect")).size(TOOLTIP_TEXT_SIZE))
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
    let separator = container(shaped_text(""))
        .width(Fill)
        .height(SEPARATOR_HEIGHT)
        .style(|theme| container::Style {
            background: Some(Background::Color(separator_color(theme))),
            ..Default::default()
        });
    main_column = main_column.push(separator);

    // === BOOKMARKS SECTION ===
    let bookmarks_title = shaped_text(t("title-bookmarks"))
        .size(SECTION_TITLE_SIZE)
        .style(|theme| iced::widget::text::Style {
            color: Some(section_title_color(theme)),
        });
    let mut bookmarks_column = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

    if bookmarks.is_empty() {
        bookmarks_column = bookmarks_column.push(
            shaped_text(t("empty-no-bookmarks"))
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

            // Check if this bookmark has an error
            let has_error = bookmark_errors.contains_key(&index);

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
            // Show in red if there's an error, blue if connected, normal otherwise
            let btn = button(shaped_text(&bookmark.name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING)
                .on_press(bookmark_message)
                .style(move |theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => interactive_hover_color(),
                        _ if has_error => bookmark_error_color(),
                        _ if is_connected => interactive_hover_color(),
                        _ => button_text_color(theme),
                    },
                    border: Border::default(),
                    shadow: iced::Shadow::default(),
                });

            // Action button (transparent icon button with hover effect)
            let edit_btn = tooltip(
                transparent_edit_button(icon::cog(), Message::ShowEditBookmark(index)),
                container(shaped_text(t("tooltip-edit")).size(TOOLTIP_TEXT_SIZE))
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

    // Icon size matching user toolbar
    let icon_size: f32 = 18.0;
    let add_icon = container(icon::bookmark().size(icon_size))
        .width(icon_size)
        .height(icon_size)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center);

    let add_btn = tooltip(
        button(add_icon)
            .on_press(Message::ShowAddBookmark)
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
        container(shaped_text(t("tooltip-add-bookmark")).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(|theme| container::Style {
                background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                text_color: Some(tooltip_text_color(theme)),
                border: tooltip_border(),
                ..Default::default()
            }),
        tooltip::Position::Top,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    let bookmarks_section = column![
        bookmarks_title,
        scrollable(bookmarks_column)
            .height(Fill)
            .style(primary_scrollbar_style()),
        Element::from(add_btn),
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

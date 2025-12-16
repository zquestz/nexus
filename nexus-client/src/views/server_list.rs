//! Server list panel (left sidebar)

use crate::i18n::t;
use crate::icon;
use crate::style::{
    FORM_PADDING, ICON_BUTTON_PADDING, INPUT_PADDING, NO_SPACING, PANEL_SPACING, SCROLLBAR_PADDING,
    SECTION_TITLE_SIZE, SEPARATOR_HEIGHT, SERVER_LIST_BUTTON_HEIGHT,
    SERVER_LIST_DISCONNECT_ICON_SIZE, SERVER_LIST_ITEM_SPACING, SERVER_LIST_PANEL_WIDTH,
    SERVER_LIST_SECTION_SPACING, SERVER_LIST_SMALL_TEXT_SIZE, SERVER_LIST_TEXT_SIZE,
    SIDEBAR_ACTION_ICON_SIZE, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING,
    TOOLTIP_TEXT_SIZE, alternating_row_style, danger_icon_button_style, list_item_button_style,
    muted_text_style, separator_style, shaped_text, sidebar_panel_style, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{Message, ServerBookmark, ServerConnection};
use iced::widget::{Column, Space, button, column, container, row, scrollable, tooltip};
use iced::{Element, Fill, alignment};
use std::collections::HashMap;

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create transparent icon buttons with hover color change (disconnect style)
fn transparent_icon_button(
    icon: iced::widget::Text<'_>,
    message: Message,
) -> button::Button<'_, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(danger_icon_button_style)
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
        .style(transparent_icon_button_style)
}

/// Create a horizontal separator line
fn separator<'a>() -> Element<'a, Message> {
    container(Space::new().width(Fill).height(SEPARATOR_HEIGHT))
        .width(Fill)
        .height(SEPARATOR_HEIGHT)
        .style(separator_style)
        .into()
}

// ============================================================================
// Connected Servers Section
// ============================================================================

/// Build the connected servers section
fn connected_servers_section<'a>(
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
) -> Column<'a, Message> {
    let connected_title = shaped_text(t("title-connected"))
        .size(SECTION_TITLE_SIZE)
        .style(muted_text_style);

    let mut connected_column = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

    if connections.is_empty() {
        connected_column = connected_column.push(
            shaped_text(t("empty-no-connections"))
                .size(SERVER_LIST_SMALL_TEXT_SIZE)
                .style(muted_text_style),
        );
    } else {
        // Sort connections by connection_id for consistent ordering
        let mut conn_list: Vec<_> = connections.iter().collect();
        conn_list.sort_by_key(|(id, _)| **id);

        for (index, (conn_id, conn)) in conn_list.iter().enumerate() {
            let is_active = active_connection == Some(**conn_id);

            // Transparent button with hover effect and primary color for active
            let btn = button(shaped_text(&conn.display_name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING)
                .on_press(Message::SwitchToConnection(**conn_id))
                .style(list_item_button_style(is_active, false));

            // Disconnect button (transparent icon button with hover effect)
            let disconnect_btn = tooltip(
                transparent_icon_button(icon::logout(), Message::DisconnectFromServer(**conn_id)),
                container(shaped_text(t("tooltip-disconnect")).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
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
                .style(alternating_row_style(is_even));

            connected_column = connected_column.push(row_container);
        }
    }

    column![connected_title, connected_column]
        .spacing(SERVER_LIST_SECTION_SPACING)
        .padding(FORM_PADDING)
}

// ============================================================================
// Bookmarks Section
// ============================================================================

/// Build the bookmarks section
fn bookmarks_section<'a>(
    bookmarks: &'a [ServerBookmark],
    connections: &'a HashMap<usize, ServerConnection>,
    bookmark_errors: &'a HashMap<usize, String>,
) -> Column<'a, Message> {
    let bookmarks_title = shaped_text(t("title-bookmarks"))
        .size(SECTION_TITLE_SIZE)
        .style(muted_text_style);

    let mut bookmarks_column = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

    if bookmarks.is_empty() {
        bookmarks_column = bookmarks_column.push(
            shaped_text(t("empty-no-bookmarks"))
                .size(SERVER_LIST_SMALL_TEXT_SIZE)
                .style(muted_text_style),
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
            // Show in danger color if there's an error, primary color if connected, normal otherwise
            let btn = button(shaped_text(&bookmark.name).size(SERVER_LIST_TEXT_SIZE))
                .width(Fill)
                .height(SERVER_LIST_BUTTON_HEIGHT)
                .padding(INPUT_PADDING)
                .on_press(bookmark_message)
                .style(list_item_button_style(is_connected, has_error));

            // Action button (transparent icon button with hover effect)
            let edit_btn = tooltip(
                transparent_edit_button(icon::cog(), Message::ShowEditBookmark(index)),
                container(shaped_text(t("tooltip-edit")).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
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
                .style(alternating_row_style(is_even));

            bookmarks_column = bookmarks_column.push(row_container);
        }
    }

    // Add right padding to make room for scrollbar
    let bookmarks_column = container(bookmarks_column)
        .padding(iced::Padding {
            top: 0.0,
            right: SCROLLBAR_PADDING,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Fill);

    // Add bookmark button
    let add_icon = container(icon::bookmark().size(SIDEBAR_ACTION_ICON_SIZE))
        .width(SIDEBAR_ACTION_ICON_SIZE)
        .height(SIDEBAR_ACTION_ICON_SIZE)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center);

    let add_btn = tooltip(
        button(add_icon)
            .on_press(Message::ShowAddBookmark)
            .padding(ICON_BUTTON_PADDING)
            .style(transparent_icon_button_style),
        container(shaped_text(t("tooltip-add-bookmark")).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Top,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    column![
        bookmarks_title,
        scrollable(bookmarks_column).height(Fill),
        Element::from(add_btn),
    ]
    .spacing(SERVER_LIST_SECTION_SPACING)
    .padding(iced::Padding {
        top: FORM_PADDING,
        right: FORM_PADDING - SCROLLBAR_PADDING,
        bottom: FORM_PADDING,
        left: FORM_PADDING,
    })
}

// ============================================================================
// Main Panel View
// ============================================================================

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
    let main_column = column![
        connected_servers_section(connections, active_connection),
        separator(),
        bookmarks_section(bookmarks, connections, bookmark_errors),
    ]
    .spacing(PANEL_SPACING);

    // Wrap in container with styling
    container(main_column)
        .height(Fill)
        .width(SERVER_LIST_PANEL_WIDTH)
        .style(sidebar_panel_style)
        .into()
}

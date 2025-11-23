//! Main application layout

use crate::types::{
    BookmarkEditMode, Message, ServerBookmark, ServerConnection, UserManagementState,
};
use iced::widget::{button, column, container, row, text};
use iced::{Center, Element, Fill};
use std::collections::HashMap;

use super::{
    admin::admin_view, bookmark::bookmark_edit_view, chat::chat_view,
    connection::connection_form_view, server_list::server_list_panel,
    user_list::user_list_panel,
};

/// Main application layout with toolbar and three-panel layout
pub fn main_layout<'a>(
    bookmarks: &'a [ServerBookmark],
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
    server_name: &'a str,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
    bookmark_edit_mode: &'a BookmarkEditMode,
    bookmark_name: &'a str,
    bookmark_address: &'a str,
    bookmark_port: &'a str,
    bookmark_username: &'a str,
    bookmark_password: &'a str,
    bookmark_auto_connect: bool,
    message_input: &'a str,
    user_management: &'a UserManagementState,
    show_bookmarks: bool,
    show_user_list: bool,
    show_add_user: bool,
    show_delete_user: bool,
) -> Element<'a, Message> {
    // Top toolbar
    let toolbar = build_toolbar(show_bookmarks, show_user_list, active_connection.is_some());

    // Left panel: Server list
    let server_list = server_list_panel(bookmarks, connections, active_connection);

    // Middle panel: Main content (bookmark editor, connection form, or active server view)
    let main_content = if *bookmark_edit_mode != BookmarkEditMode::None {
        bookmark_edit_view(
            bookmark_edit_mode,
            bookmark_name,
            bookmark_address,
            bookmark_port,
            bookmark_username,
            bookmark_password,
            bookmark_auto_connect,
        )
    } else if let Some(conn_id) = active_connection {
        if let Some(conn) = connections.get(&conn_id) {
            server_content_view(
                conn,
                message_input,
                user_management,
                show_add_user,
                show_delete_user,
            )
        } else {
            empty_content_view()
        }
    } else {
        connection_form_view(
            server_name,
            server_address,
            port,
            username,
            password,
            connection_error,
        )
    };

    // Right panel: User list (show if enabled, display empty state if no active connection)
    let user_list = if show_user_list {
        if let Some(conn_id) = active_connection {
            if let Some(conn) = connections.get(&conn_id) {
                user_list_panel(&conn.online_users, conn_id)
            } else {
                user_list_panel(&[], 0) // Empty user list
            }
        } else {
            user_list_panel(&[], 0) // Empty user list
        }
    } else {
        container(text("")).width(0).into()
    };

    // Main layout: Toolbar at top, then three-column layout
    let main_row = if show_bookmarks {
        row![server_list, main_content, user_list,].spacing(0)
    } else {
        row![main_content, user_list,].spacing(0)
    };

    let layout = column![toolbar, main_row,].spacing(0);

    container(layout).width(Fill).height(Fill).into()
}

/// Build the top toolbar with toggle buttons
fn build_toolbar(show_bookmarks: bool, show_user_list: bool, is_connected: bool) -> Element<'static, Message> {
    // Need to capture these for the closures
    let show_bookmarks_copy = show_bookmarks;
    let show_user_list_copy = show_user_list;

    let toolbar = container(
        row![
            text("Nexus BBS").size(16),
            button(text(if show_bookmarks { "[<]" } else { "[>]" }).size(12))
                .on_press(Message::ToggleBookmarks)
                .padding(8)
                .style(move |theme, status| {
                    let mut style = button::primary(theme, status);
                    if !show_bookmarks_copy {
                        style.background = Some(iced::Background::Color(iced::Color::from_rgb(
                            0.4, 0.4, 0.4,
                        )));
                    }
                    style
                }),
            button(text(if show_user_list { "[>]" } else { "[<]" }).size(12))
                .on_press(Message::ToggleUserList)
                .padding(8)
                .style(move |theme, status| {
                    let mut style = button::primary(theme, status);
                    if !show_user_list_copy {
                        style.background = Some(iced::Background::Color(iced::Color::from_rgb(
                            0.4, 0.4, 0.4,
                        )));
                    }
                    style
                }),
            if is_connected {
                button(text("User Create").size(12))
                    .on_press(Message::ToggleAddUser)
                    .padding(8)
            } else {
                button(text("User Create").size(12)).padding(8)
            },
            if is_connected {
                button(text("User Delete").size(12))
                    .on_press(Message::ToggleDeleteUser)
                    .padding(8)
            } else {
                button(text("User Delete").size(12)).padding(8)
            },
        ]
        .spacing(10)
        .padding(8)
        .align_y(Center),
    )
    .width(Fill)
    .style(|_theme| container::Style {
        background: Some(iced::Background::Color(iced::Color::from_rgb(
            0.15, 0.15, 0.15,
        ))),
        ..Default::default()
    });

    toolbar.into()
}

/// Dispatches to admin or chat view based on active panels
fn server_content_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    user_management: &'a UserManagementState,
    show_add_user: bool,
    show_delete_user: bool,
) -> Element<'a, Message> {
    // If any admin panel is open, show admin view, otherwise show chat
    if show_add_user || show_delete_user {
        admin_view(conn, user_management, show_add_user, show_delete_user)
    } else {
        chat_view(conn, message_input, show_add_user, show_delete_user)
    }
}

/// Empty content view when no server is selected
fn empty_content_view<'a>() -> Element<'a, Message> {
    container(
        text("Select a server from the list")
            .size(16)
            .color([0.5, 0.5, 0.5]),
    )
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .into()
}
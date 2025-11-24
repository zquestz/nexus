//! Main application layout and toolbar

use crate::types::{
    BookmarkEditMode, Message, ServerBookmark, ServerConnection, UserManagementState,
};
use iced::widget::{button, column, container, row, text};
use iced::{Center, Element, Fill};
use std::collections::HashMap;

use super::{
    bookmark::bookmark_edit_view, broadcast::broadcast_view, chat::chat_view,
    connection::connection_form_view, server_list::server_list_panel, user_list::user_list_panel,
    users::users_view,
};

/// Main application layout with toolbar and three-panel layout
pub fn main_layout<'a>(
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
    bookmarks: &'a [ServerBookmark],
    bookmark_edit_mode: &'a BookmarkEditMode,
    server_name: &'a str,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
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
    show_edit_user: bool,
    show_broadcast: bool,
) -> Element<'a, Message> {
    // Get permissions and admin status from active connection
    let (is_admin, permissions) = active_connection
        .and_then(|id| connections.get(&id))
        .map(|conn| (conn.is_admin, conn.permissions.as_slice()))
        .unwrap_or((false, &[]));

    // Top toolbar
    let toolbar = build_toolbar(
        show_bookmarks,
        show_user_list,
        show_broadcast,
        active_connection.is_some(),
        is_admin,
        permissions,
    );

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
                show_edit_user,
                show_broadcast,
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

    // Right panel: User list (only when connected)
    let user_list = if let Some(conn_id) = active_connection {
        if show_user_list {
            if let Some(conn) = connections.get(&conn_id) {
                user_list_panel(conn)
            } else {
                container(text("")).width(0).into()
            }
        } else {
            container(text("")).width(0).into()
        }
    } else {
        container(text("")).width(0).into()
    };

    // Three-panel layout
    let content = if show_bookmarks {
        row![server_list, main_content, user_list]
            .spacing(0)
            .height(Fill)
    } else {
        row![main_content, user_list].spacing(0).height(Fill)
    };

    column![toolbar, content].into()
}

/// Build the top toolbar with buttons and toggles
fn build_toolbar(
    show_bookmarks: bool,
    show_user_list: bool,
    show_broadcast: bool,
    is_connected: bool,
    is_admin: bool,
    permissions: &[String],
) -> Element<'static, Message> {
    // Need to capture these for the closures
    let show_bookmarks_copy = show_bookmarks;
    let show_user_list_copy = show_user_list;
    let show_broadcast_copy = show_broadcast;

    // Check permissions
    let has_broadcast = is_admin || permissions.contains(&"user_broadcast".to_string());
    let has_user_create = is_admin || permissions.contains(&"user_create".to_string());
    let has_user_edit = is_admin || permissions.contains(&"user_edit".to_string());

    let toolbar = container(
        row![
            // Title
            text("Nexus BBS").size(16),
            // Broadcast button
            if is_connected && has_broadcast {
                button(text("Broadcast").size(12))
                    .on_press(Message::ToggleBroadcast)
                    .padding(8)
                    .style(move |theme, status| {
                        let mut style = button::primary(theme, status);
                        if show_broadcast_copy {
                            style.background = Some(iced::Background::Color(
                                iced::Color::from_rgb(0.3, 0.5, 0.7),
                            ));
                        }
                        style
                    })
            } else {
                button(text("Broadcast").size(12)).padding(8)
            },
            // User Create button
            if is_connected && has_user_create {
                button(text("User Create").size(12))
                    .on_press(Message::ToggleAddUser)
                    .padding(8)
            } else {
                button(text("User Create").size(12)).padding(8)
            },
            // User Edit button (replaces User Delete - delete is now inside edit form)
            if is_connected && has_user_edit {
                button(text("User Edit").size(12))
                    .on_press(Message::ToggleEditUser)
                    .padding(8)
            } else {
                button(text("User Edit").size(12)).padding(8)
            },
            // Spacer to push collapse buttons to the right
            container(text("")).width(Fill),
            // Left collapse button (bookmarks)
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
            // Right collapse button (user list)
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
    show_edit_user: bool,
    show_broadcast: bool,
) -> Element<'a, Message> {
    // If broadcast panel is open, show broadcast view
    if show_broadcast {
        broadcast_view(conn)
    } else if show_add_user || show_edit_user {
        // If any user management panel is open, show users view
        users_view(conn, user_management, show_add_user, show_edit_user)
    } else {
        // Otherwise show chat
        chat_view(conn, message_input)
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

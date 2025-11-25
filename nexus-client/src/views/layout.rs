//! Main application layout and toolbar

use super::style::{
    BOOKMARK_BUTTON_HOVER_COLOR, EMPTY_VIEW_SIZE, EMPTY_VIEW_TEXT_COLOR,
    PANEL_SPACING, TOOLBAR_BACKGROUND_COLOR, TOOLBAR_BUTTON_PADDING, TOOLBAR_BUTTON_SIZE,
    TOOLBAR_ICON_SIZE, TOOLBAR_PADDING_HORIZONTAL, TOOLBAR_PADDING_VERTICAL, TOOLBAR_SPACING, TOOLBAR_ICON_SPACING, TOOLBAR_TITLE_SIZE, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_BACKGROUND_COLOR,
    TOOLBAR_ICON_COLOR, TOOLBAR_ICON_HOVER_COLOR, TOOLBAR_ICON_DISABLED_COLOR,
};
use crate::icon;
use crate::types::{
    BookmarkEditMode, Message, ServerBookmark, ServerConnection, UserManagementState,
};
use iced::widget::{button, column, container, row, text, tooltip};
use iced::{Background, Border, Center, Color, Element, Fill};
use std::collections::HashMap;

use super::{
    bookmark::bookmark_edit_view, broadcast::broadcast_view, chat::chat_view,
    connection::connection_form_view, server_list::server_list_panel, user_list::user_list_panel,
    users::users_view,
};

/// Helper function to create an invisible/hidden panel
fn hidden_panel<'a>() -> Element<'a, Message> {
    container(text("")).width(0).into()
}

// Permission constants
const PERMISSION_USER_BROADCAST: &str = "user_broadcast";
const PERMISSION_USER_CREATE: &str = "user_create";
const PERMISSION_USER_EDIT: &str = "user_edit";
const PERMISSION_USER_LIST: &str = "user_list";

/// Main application layout with toolbar and three-panel layout
///
/// Displays the top toolbar with action buttons, and a multi-panel layout
/// containing the server list (left), main content area (center), and user
/// list (right). Panels can be toggled on/off via toolbar buttons.
///
/// The main content area shows different views based on application state:
/// - Bookmark editor when editing/adding bookmarks
/// - Connection form when no server is connected
/// - Server content (chat/user management/broadcast) when connected
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
    is_connecting: bool,
    bookmark_name: &'a str,
    bookmark_address: &'a str,
    bookmark_port: &'a str,
    bookmark_username: &'a str,
    bookmark_password: &'a str,
    bookmark_auto_connect: bool,
    bookmark_error: &'a Option<String>,
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

    // Check if user has permission to view user list
    let can_view_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

    // Top toolbar
    let toolbar = build_toolbar(
        show_bookmarks,
        show_user_list,
        show_broadcast,
        active_connection.is_some(),
        is_admin,
        permissions,
        can_view_user_list,
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
            bookmark_error,
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
            is_connecting,
        )
    };

    // Right panel: User list (only when connected, visible, and user has permission)
    let user_list = if show_user_list && can_view_user_list {
        active_connection
            .and_then(|conn_id| connections.get(&conn_id))
            .map(|conn| user_list_panel(conn))
            .unwrap_or_else(hidden_panel)
    } else {
        hidden_panel()
    };

    // Three-panel layout with conditional panels
    let content = if show_bookmarks {
        row![server_list, main_content, user_list]
            .spacing(PANEL_SPACING)
            .height(Fill)
    } else {
        row![main_content, user_list]
            .spacing(PANEL_SPACING)
            .height(Fill)
    };

    column![toolbar, content].into()
}

/// Build the top toolbar with buttons and toggles
///
/// Shows application title, action buttons (Broadcast, User Create, User Edit),
/// and panel toggle buttons. Buttons are enabled/disabled based on connection
/// state and user permissions.
fn build_toolbar(
    show_bookmarks: bool,
    show_user_list: bool,
    show_broadcast: bool,
    is_connected: bool,
    is_admin: bool,
    permissions: &[String],
    can_view_user_list: bool,
) -> Element<'static, Message> {
    // Need to capture this for the closures
    let show_broadcast_copy = show_broadcast;

    // Check permissions (avoid string allocations)
    let has_broadcast = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_BROADCAST);
    let has_user_create = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_CREATE);
    let has_user_edit = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_EDIT);

    let toolbar = container(
        row![
            // Title
            text("Nexus BBS").size(TOOLBAR_TITLE_SIZE),
            // Main icon group (Broadcast, User Create, User Edit)
            row![
            // Broadcast button
            if is_connected && has_broadcast {
                tooltip(
                    button(icon::megaphone().size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ToggleBroadcast)
                        .style(move |_theme, status| {
                            if show_broadcast_copy {
                                // Active state - blue background
                                button::Style {
                                    background: Some(Background::Color(BOOKMARK_BUTTON_HOVER_COLOR)),
                                    text_color: Color::WHITE,
                                    border: Border::default(),
                                    shadow: iced::Shadow::default(),
                                }
                            } else {
                                // Default state - transparent with hover
                                button::Style {
                                    background: None,
                                    text_color: match status {
                                        button::Status::Hovered => TOOLBAR_ICON_HOVER_COLOR,
                                        _ => TOOLBAR_ICON_COLOR,
                                    },
                                    border: Border::default(),
                                    shadow: iced::Shadow::default(),
                                }
                            }
                        }),
                    container(text("Broadcast").size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
            } else {
                tooltip(
                    button(icon::megaphone().size(TOOLBAR_ICON_SIZE))
                        .style(|_theme, _status| button::Style {
                            background: None,
                            text_color: TOOLBAR_ICON_DISABLED_COLOR,
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    container(text("Broadcast").size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
            },
            // User Create button
            if is_connected && has_user_create {
                tooltip(
                    button(icon::user_plus().size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ToggleAddUser)
                        .style(|_theme, status| button::Style {
                            background: None,
                            text_color: match status {
                                button::Status::Hovered => TOOLBAR_ICON_HOVER_COLOR,
                                _ => TOOLBAR_ICON_COLOR,
                            },
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    container(text("User Create").size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
            } else {
                tooltip(
                    button(icon::user_plus().size(TOOLBAR_ICON_SIZE))
                        .style(|_theme, _status| button::Style {
                            background: None,
                            text_color: TOOLBAR_ICON_DISABLED_COLOR,
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    container(text("User Create").size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
            },
            // User Edit button
            if is_connected && has_user_edit {
                tooltip(
                    button(icon::users().size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ToggleEditUser)
                        .style(|_theme, status| button::Style {
                            background: None,
                            text_color: match status {
                                button::Status::Hovered => TOOLBAR_ICON_HOVER_COLOR,
                                _ => TOOLBAR_ICON_COLOR,
                            },
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    container(text("User Edit").size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
            } else {
                tooltip(
                    button(icon::users().size(TOOLBAR_ICON_SIZE))
                        .style(|_theme, _status| button::Style {
                            background: None,
                            text_color: TOOLBAR_ICON_DISABLED_COLOR,
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    container(text("User Edit").size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
            },
            ].spacing(TOOLBAR_ICON_SPACING),
            // Spacer to push collapse buttons to the right
            container(text("")).width(Fill),
            // Collapse buttons group
            row![
                // Left collapse button (bookmarks)
                tooltip(
                    button(if show_bookmarks { icon::collapse_left() } else { icon::expand_right() }.size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ToggleBookmarks)
                        .style(|_theme, status| button::Style {
                            background: None,
                            text_color: match status {
                                button::Status::Hovered => TOOLBAR_ICON_HOVER_COLOR,
                                _ => TOOLBAR_ICON_COLOR,
                            },
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                    container(text(if show_bookmarks { "Hide Bookmarks" } else { "Show Bookmarks" }).size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING),
                // Right collapse button (user list)
                if can_view_user_list {
                    tooltip(
                        button(if show_user_list { icon::expand_right() } else { icon::collapse_left() }.size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleUserList)
                            .style(|_theme, status| button::Style {
                                background: None,
                                text_color: match status {
                                    button::Status::Hovered => TOOLBAR_ICON_HOVER_COLOR,
                                    _ => TOOLBAR_ICON_COLOR,
                                },
                                border: Border::default(),
                                shadow: iced::Shadow::default(),
                            }),
                        container(text(if show_user_list { "Hide User List" } else { "Show User List" }).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(|_theme| container::Style {
                                background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                                ..Default::default()
                            }),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(if show_user_list { icon::expand_right() } else { icon::collapse_left() }.size(TOOLBAR_ICON_SIZE))
                            .style(|_theme, _status| button::Style {
                                background: None,
                                text_color: TOOLBAR_ICON_DISABLED_COLOR,
                                border: Border::default(),
                                shadow: iced::Shadow::default(),
                            }),
                        container(text("User List").size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(|_theme| container::Style {
                                background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                                ..Default::default()
                            }),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
            ].spacing(TOOLBAR_ICON_SPACING),
        ]
        .spacing(TOOLBAR_SPACING)
        .padding([TOOLBAR_PADDING_VERTICAL, TOOLBAR_PADDING_HORIZONTAL])
        .align_y(Center),
    )
    .width(Fill)
    .style(|_theme| container::Style {
        background: Some(Background::Color(TOOLBAR_BACKGROUND_COLOR)),
        ..Default::default()
    });

    toolbar.into()
}

/// Dispatches to appropriate content view based on active panels
///
/// Shows broadcast view, user management view, or chat view depending on
/// which panels are currently open.
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
///
/// Displays a centered message prompting the user to select a server.
fn empty_content_view<'a>() -> Element<'a, Message> {
    container(
        text("Select a server from the list")
            .size(EMPTY_VIEW_SIZE)
            .color(EMPTY_VIEW_TEXT_COLOR),
    )
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .into()
}

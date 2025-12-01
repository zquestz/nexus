//! Main application layout and toolbar

use super::constants::{
    PERMISSION_USER_BROADCAST, PERMISSION_USER_CREATE, PERMISSION_USER_EDIT, PERMISSION_USER_LIST,
};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BORDER_WIDTH, EMPTY_VIEW_SIZE, PANEL_SPACING, TOOLBAR_ICON_SIZE, TOOLBAR_ICON_SPACING,
    TOOLBAR_PADDING_HORIZONTAL, TOOLBAR_PADDING_VERTICAL, TOOLBAR_SPACING, TOOLBAR_TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    disabled_icon_button_style, muted_text_style, separator_style, shaped_text,
    toolbar_background_style, toolbar_button_style, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{
    ActivePanel, BookmarkEditMode, Message, ServerConnection, ToolbarState, UserManagementState,
    ViewConfig,
};
use iced::widget::{button, column, container, row, stack, tooltip, Space};
use iced::{Center, Element, Fill};

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a horizontal separator line
fn separator<'a>() -> Element<'a, Message> {
    container(Space::new(Fill, BORDER_WIDTH))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(separator_style)
        .into()
}

use super::{
    bookmark::bookmark_edit_view, broadcast::broadcast_view, chat::chat_view,
    connection::connection_form_view, server_list::server_list_panel, user_list::user_list_panel,
    users::users_view,
};

/// Helper function to create an invisible/hidden panel
fn hidden_panel<'a>() -> Element<'a, Message> {
    container(shaped_text("")).width(0).into()
}

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
pub fn main_layout<'a>(config: ViewConfig<'a>) -> Element<'a, Message> {
    // Get permissions and admin status from active connection
    let (is_admin, permissions) = config
        .active_connection
        .and_then(|id| config.connections.get(&id))
        .map(|conn| (conn.is_admin, conn.permissions.as_slice()))
        .unwrap_or((false, &[]));

    // Check if user has permission to view user list
    let can_view_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

    // Top toolbar
    let toolbar = build_toolbar(ToolbarState {
        show_bookmarks: config.ui_state.show_bookmarks,
        show_user_list: config.ui_state.show_user_list,
        active_panel: config.ui_state.active_panel,
        is_connected: config.active_connection.is_some(),
        is_admin,
        permissions,
        can_view_user_list,
    });

    // Left panel: Server list
    let server_list = server_list_panel(
        config.bookmarks,
        config.connections,
        config.active_connection,
        config.bookmark_errors,
    );

    // Middle panel: Main content (bookmark editor, connection form, or active server view)
    // Wrapped with separators for consistent appearance
    let main_content: Element<'_, Message> = {
        let content = if config.bookmark_edit.mode != BookmarkEditMode::None {
            bookmark_edit_view(config.bookmark_edit)
        } else if let Some(conn_id) = config.active_connection
            && let Some(conn) = config.connections.get(&conn_id)
            && let Some(user_mgmt) = config.user_management
        {
            server_content_view(
                conn,
                config.message_input,
                user_mgmt,
                config.ui_state.active_panel,
                config.theme.clone(),
            )
        } else if config.active_connection.is_some() {
            // Connection exists but couldn't get all required state
            empty_content_view()
        } else {
            connection_form_view(config.connection_form)
        };

        column![separator(), content, separator()]
            .width(Fill)
            .height(Fill)
            .into()
    };

    // Right panel: User list (only when connected, visible, and user has permission)
    let user_list = if config.ui_state.show_user_list && can_view_user_list {
        config
            .active_connection
            .and_then(|conn_id| config.connections.get(&conn_id))
            .map(|conn| user_list_panel(conn))
            .unwrap_or_else(hidden_panel)
    } else {
        hidden_panel()
    };

    // Three-panel layout with conditional panels
    let content = if config.ui_state.show_bookmarks {
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
fn build_toolbar(state: ToolbarState) -> Element<'static, Message> {
    // Need to capture this for the closures
    let active_panel = state.active_panel;

    // Check permissions (avoid string allocations)
    let has_broadcast = state.is_admin
        || state
            .permissions
            .iter()
            .any(|p| p == PERMISSION_USER_BROADCAST);
    let has_user_create = state.is_admin
        || state
            .permissions
            .iter()
            .any(|p| p == PERMISSION_USER_CREATE);
    let has_user_edit =
        state.is_admin || state.permissions.iter().any(|p| p == PERMISSION_USER_EDIT);

    let toolbar = container(
        row![
            // Title
            shaped_text(t("title-nexus-bbs")).size(TOOLBAR_TITLE_SIZE),
            // Main icon group (Chat, Broadcast, User Create, User Edit)
            row![
                // Chat button - always visible when connected
                if state.is_connected {
                    tooltip(
                        button(icon::chat().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ShowChatView)
                            .style(toolbar_button_style(active_panel == ActivePanel::None)),
                        container(shaped_text(t("tooltip-chat")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::chat().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-chat")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
                // Broadcast button
                if state.is_connected && has_broadcast {
                    tooltip(
                        button(icon::megaphone().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleBroadcast)
                            .style(toolbar_button_style(active_panel == ActivePanel::Broadcast)),
                        container(shaped_text(t("tooltip-broadcast")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::megaphone().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-broadcast")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
                // User Create button
                if state.is_connected && has_user_create {
                    tooltip(
                        button(icon::user_plus().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleAddUser)
                            .style(toolbar_button_style(active_panel == ActivePanel::AddUser)),
                        container(shaped_text(t("tooltip-user-create")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::user_plus().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-user-create")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
                // User Edit button
                if state.is_connected && has_user_edit {
                    tooltip(
                        button(icon::users().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleEditUser)
                            .style(toolbar_button_style(active_panel == ActivePanel::EditUser)),
                        container(shaped_text(t("tooltip-user-edit")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::users().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-user-edit")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
            ]
            .spacing(TOOLBAR_ICON_SPACING),
            // Spacer to push collapse buttons to the right
            container(shaped_text("")).width(Fill),
            // Collapse buttons group (with theme toggle)
            row![
                // Theme toggle button
                tooltip(
                    button(icon::sun().size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ToggleTheme)
                        .style(transparent_icon_button_style),
                    container(shaped_text(t("tooltip-toggle-theme")).size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(tooltip_container_style),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING),
                // Left collapse button (bookmarks)
                tooltip(
                    button(
                        if state.show_bookmarks {
                            icon::collapse_left()
                        } else {
                            icon::expand_right()
                        }
                        .size(TOOLBAR_ICON_SIZE)
                    )
                    .on_press(Message::ToggleBookmarks)
                    .style(transparent_icon_button_style),
                    container(
                        shaped_text(if state.show_bookmarks {
                            t("tooltip-hide-bookmarks")
                        } else {
                            t("tooltip-show-bookmarks")
                        })
                        .size(TOOLTIP_TEXT_SIZE)
                    )
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING),
                // Right collapse button (user list)
                if state.can_view_user_list {
                    tooltip(
                        button(
                            if state.show_user_list {
                                icon::expand_right()
                            } else {
                                icon::collapse_left()
                            }
                            .size(TOOLBAR_ICON_SIZE),
                        )
                        .on_press(Message::ToggleUserList)
                        .style(transparent_icon_button_style),
                        container(
                            shaped_text(if state.show_user_list {
                                t("tooltip-hide-user-list")
                            } else {
                                t("tooltip-show-user-list")
                            })
                            .size(TOOLTIP_TEXT_SIZE),
                        )
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(
                            if state.show_user_list {
                                icon::expand_right()
                            } else {
                                icon::collapse_left()
                            }
                            .size(TOOLBAR_ICON_SIZE),
                        )
                        .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-show-user-list")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
            ]
            .spacing(TOOLBAR_ICON_SPACING),
        ]
        .spacing(TOOLBAR_SPACING)
        .padding([TOOLBAR_PADDING_VERTICAL, TOOLBAR_PADDING_HORIZONTAL])
        .align_y(Center),
    )
    .width(Fill)
    .style(toolbar_background_style);

    toolbar.into()
}

/// Dispatches to appropriate content view based on active panels
///
/// Always renders chat view at the bottom layer to preserve scroll position,
/// then overlays broadcast or user management panels on top when active.
fn server_content_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    user_management: &'a UserManagementState,
    active_panel: ActivePanel,
    theme: iced::Theme,
) -> Element<'a, Message> {
    // Always render chat view as the base layer to preserve scroll position
    let chat = chat_view(conn, message_input, theme);

    // Overlay panels on top when active
    match active_panel {
        ActivePanel::Broadcast => stack![chat, broadcast_view(conn)]
            .width(Fill)
            .height(Fill)
            .into(),
        ActivePanel::AddUser | ActivePanel::EditUser => {
            stack![chat, users_view(conn, user_management, active_panel)]
                .width(Fill)
                .height(Fill)
                .into()
        }
        ActivePanel::None => chat,
    }
}

/// Empty content view when no server is selected
///
/// Displays a centered message prompting the user to select a server.
fn empty_content_view<'a>() -> Element<'a, Message> {
    container(
        shaped_text(t("empty-select-server"))
            .size(EMPTY_VIEW_SIZE)
            .style(muted_text_style),
    )
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .into()
}

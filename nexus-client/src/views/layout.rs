//! Main application layout and toolbar

use super::constants::{
    PERMISSION_NEWS_LIST, PERMISSION_USER_BROADCAST, PERMISSION_USER_CREATE,
    PERMISSION_USER_DELETE, PERMISSION_USER_EDIT, PERMISSION_USER_LIST,
};
use super::news::news_view;
use super::server_info::{ServerInfoData, server_info_view};
use super::user_info::{password_change_view, user_info_view};
use crate::config::settings::ProxySettings;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BORDER_WIDTH, EMPTY_VIEW_SIZE, PANEL_SPACING, TOOLBAR_ICON_SIZE, TOOLBAR_ICON_SPACING,
    TOOLBAR_PADDING_HORIZONTAL, TOOLBAR_PADDING_VERTICAL, TOOLBAR_SPACING, TOOLBAR_TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    content_background_style, disabled_icon_button_style, modal_overlay_style, muted_text_style,
    separator_style, shaped_text, toolbar_background_style, toolbar_button_style,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{
    ActivePanel, BookmarkEditMode, Message, ServerConnection, SettingsFormState, ToolbarState,
    UserManagementState, ViewConfig,
};
use iced::widget::{
    Column, Space, button, column, container, row, scrollable, stack, text_editor, tooltip,
};
use iced::{Center, Element, Fill};

// ============================================================================
// Server Content Context
// ============================================================================

/// Context for rendering server content view
struct ServerContentContext<'a> {
    /// Active server connection
    conn: &'a ServerConnection,
    /// Current message input text
    message_input: &'a str,
    /// User management panel state
    user_management: &'a UserManagementState,
    /// Currently active panel
    active_panel: ActivePanel,
    /// Current theme
    theme: iced::Theme,
    /// Whether to show connection notifications
    show_connection_notifications: bool,
    /// Chat font size
    chat_font_size: u8,
    /// Timestamp display settings
    timestamp_settings: TimestampSettings,
    /// Settings form state (when settings panel is open)
    settings_form: Option<&'a SettingsFormState>,
    /// News body editor content
    news_body_content: Option<&'a text_editor::Content>,
    /// Default nickname for shared accounts
    nickname: &'a str,
    /// SOCKS5 proxy settings
    proxy: &'a ProxySettings,
    /// Download path for file transfers
    download_path: Option<&'a str>,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a horizontal separator line
fn separator<'a>() -> Element<'a, Message> {
    container(Space::new().width(Fill).height(BORDER_WIDTH))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(separator_style)
        .into()
}

/// Wrap a form column in a scrollable, centered container with background styling.
///
/// This is the standard wrapper for all panel views (About, Settings, Server Info,
/// User Info, Broadcast, Add/Edit User). It provides:
/// - Vertical scrolling when content exceeds window height
/// - Horizontal and vertical centering of the form (when content fits)
/// - Consistent background styling
pub fn scrollable_panel(form: Column<'_, Message>) -> Element<'_, Message> {
    let scrollable_form = scrollable(container(form).width(Fill).center_x(Fill))
        .width(Fill)
        .height(iced::Length::Shrink);

    container(scrollable_form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}

/// Wrap a form column in a scrollable, centered container with modal overlay styling.
///
/// This is the wrapper for modal dialogs (e.g., fingerprint mismatch). It provides:
/// - Vertical scrolling when content exceeds window height
/// - Horizontal and vertical centering of the form (when content fits)
/// - Semi-transparent overlay background
pub fn scrollable_modal(form: Column<'_, Message>) -> Element<'_, Message> {
    let scrollable_form = scrollable(container(form).width(Fill).center_x(Fill))
        .width(Fill)
        .height(iced::Length::Shrink);

    container(scrollable_form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(modal_overlay_style)
        .into()
}

use super::{
    about::about_view,
    bookmark::bookmark_edit_view,
    broadcast::broadcast_view,
    chat::{TimestampSettings, chat_view},
    connection::connection_form_view,
    server_list::server_list_panel,
    settings::{SettingsViewData, settings_view},
    user_list::user_list_panel,
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
    let can_view_user_list = config
        .active_connection
        .and_then(|id| config.connections.get(&id))
        .is_some_and(|conn| conn.has_permission(PERMISSION_USER_LIST));

    // Get server name from active connection
    let server_name = config
        .active_connection
        .and_then(|id| config.connections.get(&id))
        .and_then(|conn| conn.server_name.as_deref());

    // Top toolbar
    let toolbar = build_toolbar(ToolbarState {
        show_bookmarks: config.ui_state.show_bookmarks,
        show_user_list: config.ui_state.show_user_list,
        active_panel: config.active_panel,
        is_connected: config.active_connection.is_some(),
        is_admin,
        permissions,
        can_view_user_list,
        server_name,
    });

    // Left panel: Server list (use hidden_panel when not visible to preserve layout stability)
    let server_list = if config.ui_state.show_bookmarks {
        server_list_panel(
            config.bookmarks,
            config.connections,
            config.active_connection,
            config.bookmark_errors,
        )
    } else {
        hidden_panel()
    };

    // Middle panel: Main content (bookmark editor, connection form, or active server view)
    // Wrapped with separators for consistent appearance
    let main_content: Element<'_, Message> = {
        let content = if config.bookmark_edit.mode != BookmarkEditMode::None {
            bookmark_edit_view(config.bookmark_edit)
        } else if let Some(conn_id) = config.active_connection
            && let Some(conn) = config.connections.get(&conn_id)
            && let Some(user_mgmt) = config.user_management
        {
            server_content_view(ServerContentContext {
                conn,
                message_input: config.message_input,
                user_management: user_mgmt,
                active_panel: config.active_panel,
                theme: config.theme.clone(),
                show_connection_notifications: config.show_connection_notifications,
                chat_font_size: config.chat_font_size,
                timestamp_settings: TimestampSettings {
                    show_timestamps: config.show_timestamps,
                    use_24_hour_time: config.use_24_hour_time,
                    show_seconds: config.show_seconds,
                },
                settings_form: config.settings_form,
                news_body_content: config.news_body_content,
                nickname: config.nickname,
                proxy: config.proxy,
                download_path: config.download_path,
            })
        } else if config.active_connection.is_some() {
            // Connection exists but couldn't get all required state
            empty_content_view()
        } else {
            // Not connected - show connection form, with Settings/About overlay if active
            let conn_form = connection_form_view(config.connection_form);
            match config.active_panel {
                ActivePanel::Settings => stack![
                    conn_form,
                    settings_view(SettingsViewData {
                        current_theme: config.theme.clone(),
                        show_connection_notifications: config.show_connection_notifications,
                        chat_font_size: config.chat_font_size,
                        timestamp_settings: TimestampSettings {
                            show_timestamps: config.show_timestamps,
                            use_24_hour_time: config.use_24_hour_time,
                            show_seconds: config.show_seconds,
                        },
                        settings_form: config.settings_form,
                        nickname: config.nickname,
                        proxy: config.proxy,
                        download_path: config.download_path,
                    })
                ]
                .width(Fill)
                .height(Fill)
                .into(),
                ActivePanel::About => stack![conn_form, about_view(config.theme.clone())]
                    .width(Fill)
                    .height(Fill)
                    .into(),
                _ => conn_form,
            }
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
            .map(|conn| user_list_panel(conn, &config.theme))
            .unwrap_or_else(hidden_panel)
    } else {
        hidden_panel()
    };

    // Three-panel layout (always same structure to preserve scroll state)
    let content = row![server_list, main_content, user_list]
        .spacing(PANEL_SPACING)
        .height(Fill);

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

    // Check permissions
    let has_broadcast = state.has_permission(PERMISSION_USER_BROADCAST);
    let has_news = state.has_permission(PERMISSION_NEWS_LIST);
    let has_user_management = state.has_any_permission(&[
        PERMISSION_USER_CREATE,
        PERMISSION_USER_EDIT,
        PERMISSION_USER_DELETE,
    ]);

    let toolbar = container(
        row![
            // Title: server name when connected, "Nexus BBS" otherwise
            shaped_text(state.toolbar_title()).size(TOOLBAR_TITLE_SIZE),
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
                // News button
                if state.is_connected && has_news {
                    tooltip(
                        button(icon::newspaper().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleNews)
                            .style(toolbar_button_style(active_panel == ActivePanel::News)),
                        container(shaped_text(t("tooltip-news")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::newspaper().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-news")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
                // User Management button
                if state.is_connected && has_user_management {
                    tooltip(
                        button(icon::users().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleUserManagement)
                            .style(toolbar_button_style(
                                active_panel == ActivePanel::UserManagement,
                            )),
                        container(shaped_text(t("tooltip-manage-users")).size(TOOLTIP_TEXT_SIZE))
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
                        container(shaped_text(t("tooltip-manage-users")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
                // Server Info button
                if state.is_connected {
                    tooltip(
                        button(icon::server().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ShowServerInfo)
                            .style(toolbar_button_style(
                                active_panel == ActivePanel::ServerInfo,
                            )),
                        container(shaped_text(t("tooltip-server-info")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::server().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-server-info")).size(TOOLTIP_TEXT_SIZE))
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
                // About button
                tooltip(
                    button(icon::info_circled().size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ShowAbout)
                        .style(toolbar_button_style(active_panel == ActivePanel::About)),
                    container(shaped_text(t("tooltip-about")).size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(tooltip_container_style),
                    tooltip::Position::Bottom,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING),
                // Settings button
                tooltip(
                    button(icon::cog().size(TOOLBAR_ICON_SIZE))
                        .on_press(Message::ToggleSettings)
                        .style(toolbar_button_style(active_panel == ActivePanel::Settings)),
                    container(shaped_text(t("tooltip-settings")).size(TOOLTIP_TEXT_SIZE))
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
fn server_content_view<'a>(ctx: ServerContentContext<'a>) -> Element<'a, Message> {
    // Always render chat view as the base layer to preserve scroll position
    let chat = chat_view(
        ctx.conn,
        ctx.message_input,
        ctx.theme.clone(),
        ctx.chat_font_size,
        ctx.timestamp_settings,
    );

    // Overlay panels on top when active
    match ctx.active_panel {
        ActivePanel::About => stack![chat, about_view(ctx.theme)]
            .width(Fill)
            .height(Fill)
            .into(),
        ActivePanel::Broadcast => stack![chat, broadcast_view(ctx.conn)]
            .width(Fill)
            .height(Fill)
            .into(),
        ActivePanel::UserManagement => {
            stack![chat, users_view(ctx.conn, ctx.user_management, &ctx.theme)]
                .width(Fill)
                .height(Fill)
                .into()
        }
        ActivePanel::Settings => stack![
            chat,
            settings_view(SettingsViewData {
                current_theme: ctx.theme.clone(),
                show_connection_notifications: ctx.show_connection_notifications,
                chat_font_size: ctx.chat_font_size,
                timestamp_settings: ctx.timestamp_settings,
                settings_form: ctx.settings_form,
                nickname: ctx.nickname,
                proxy: ctx.proxy,
                download_path: ctx.download_path,
            })
        ]
        .width(Fill)
        .height(Fill)
        .into(),
        ActivePanel::ServerInfo => {
            let data = ServerInfoData {
                name: ctx.conn.server_name.clone(),
                description: ctx.conn.server_description.clone(),
                version: ctx.conn.server_version.clone(),
                max_connections_per_ip: ctx.conn.max_connections_per_ip,
                cached_server_image: ctx.conn.cached_server_image.as_ref(),
                is_admin: ctx.conn.is_admin,
                edit_state: ctx.conn.server_info_edit.as_ref(),
            };
            stack![chat, server_info_view(&data)]
                .width(Fill)
                .height(Fill)
                .into()
        }
        ActivePanel::UserInfo => stack![chat, user_info_view(ctx.conn, ctx.theme)]
            .width(Fill)
            .height(Fill)
            .into(),
        ActivePanel::ChangePassword => stack![
            chat,
            password_change_view(ctx.conn.password_change_state.as_ref())
        ]
        .width(Fill)
        .height(Fill)
        .into(),
        ActivePanel::News => stack![
            chat,
            news_view(
                ctx.conn,
                &ctx.conn.news_management,
                &ctx.theme,
                ctx.news_body_content,
            )
        ]
        .width(Fill)
        .height(Fill)
        .into(),
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

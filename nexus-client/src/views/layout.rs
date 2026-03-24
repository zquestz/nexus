//! Main application layout and toolbar

use iced::widget::{
    Column, Space, button, column, container, row, scrollable, stack, text_editor, tooltip,
};
use iced::{Center, Element, Fill};
use nexus_common::voice::VoiceQuality;

use crate::config::audio::{PttMode, PttReleaseDelay};
use crate::voice::audio::AudioDevice;

use super::connection_monitor::connection_monitor_view;
use super::constants::{
    PERMISSION_CONNECTION_MONITOR, PERMISSION_FILE_COPY, PERMISSION_FILE_CREATE_DIR,
    PERMISSION_FILE_DELETE, PERMISSION_FILE_DOWNLOAD, PERMISSION_FILE_INFO, PERMISSION_FILE_LIST,
    PERMISSION_FILE_MOVE, PERMISSION_FILE_RENAME, PERMISSION_FILE_ROOT, PERMISSION_FILE_SEARCH,
    PERMISSION_FILE_UPLOAD, PERMISSION_NEWS_LIST, PERMISSION_USER_BROADCAST,
    PERMISSION_USER_CREATE, PERMISSION_USER_DELETE, PERMISSION_USER_EDIT, PERMISSION_USER_LIST,
};
use super::disconnect_dialog::disconnect_dialog_view;
use super::files::{FilePermissions, files_view};
use super::news::news_view;
use super::server_info::{ServerInfoData, server_info_view};
use super::transfers::transfers_view;
use super::user_info::{password_change_view, user_info_view};
use crate::config::events::EventSettings;
use crate::config::settings::ProxySettings;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BADGE_FONT_SIZE, BADGE_HEIGHT, BADGE_PADDING_HORIZONTAL, BADGE_SIZE, BORDER_WIDTH,
    EMPTY_VIEW_SIZE, PANEL_SPACING, TOOLBAR_ICON_SIZE, TOOLBAR_ICON_SPACING,
    TOOLBAR_PADDING_HORIZONTAL, TOOLBAR_PADDING_VERTICAL, TOOLBAR_SPACING, TOOLBAR_TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, badge_style,
    content_background_style, disabled_icon_button_style, modal_overlay_style, muted_text_style,
    separator_style, shaped_text, toolbar_background_style, toolbar_button_style,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{
    ActivePanel, BookmarkEditMode, Message, ServerConnection, SettingsFormState, ToolbarState,
    UserManagementState, ViewConfig,
};

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
    /// Whether to show connection events
    show_connection_events: bool,
    /// Whether to show channel join/leave events
    show_join_leave_events: bool,
    /// Chat history retention policy for user message conversations
    chat_history_retention: crate::config::settings::ChatHistoryRetention,
    /// Maximum scrollback lines per chat tab (0 = unlimited)
    max_scrollback: usize,
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
    /// Whether to show hidden files
    show_hidden: bool,
    /// Transfer manager for file downloads/uploads
    transfer_manager: &'a crate::transfers::TransferManager,
    /// Whether to queue transfers (limit concurrent transfers per server)
    queue_transfers: bool,
    /// Maximum concurrent downloads per server (0 = unlimited)
    download_limit: u8,
    /// Maximum concurrent uploads per server (0 = unlimited)
    pub upload_limit: u8,
    /// Whether to show the drag-and-drop overlay
    pub show_drop_overlay: bool,
    /// Event notification settings
    pub event_settings: &'a EventSettings,
    /// Global toggle for desktop notifications
    pub notifications_enabled: bool,
    /// Global toggle for sound notifications
    pub sound_enabled: bool,
    /// Master volume for sounds (0.0 - 1.0)
    pub sound_volume: f32,
    /// Voice target for the current tab (channel or nickname)
    pub voice_target: Option<String>,
    // ==================== Audio Settings ====================
    /// Available output devices (borrowed from SettingsFormState cache)
    pub output_devices: &'a [AudioDevice],
    /// Selected output device
    pub selected_output_device: AudioDevice,
    /// Available input devices (borrowed from SettingsFormState cache)
    pub input_devices: &'a [AudioDevice],
    /// Selected input device
    pub selected_input_device: AudioDevice,
    /// Voice quality setting
    pub voice_quality: VoiceQuality,
    /// Push-to-talk key binding
    pub ptt_key: &'a str,
    /// Whether PTT key capture is active
    pub ptt_capturing: bool,
    /// Push-to-talk mode
    pub ptt_mode: PttMode,
    /// Push-to-talk release delay
    pub ptt_release_delay: PttReleaseDelay,
    /// Whether microphone test is active
    pub mic_testing: bool,
    /// Current microphone input level (0.0 - 1.0) — used by voice bar
    pub mic_level: f32,
    /// Microphone level from settings mic test (0.0 - 1.0)
    pub settings_mic_level: f32,
    /// Error message from microphone test (e.g., device not found)
    pub mic_error: Option<&'a str>,
    /// Noise suppression level
    pub noise_suppression_level: crate::config::audio::NoiseSuppressionLevel,
    /// Enable echo cancellation
    pub echo_cancellation: bool,
    /// Enable automatic gain control
    pub agc: bool,
    /// Enable transient suppression (keyboard/click noise reduction)
    pub transient_suppression: bool,
    /// Microphone boost level
    pub mic_boost: crate::config::audio::MicBoost,
    /// Whether local user is currently transmitting (PTT active)
    pub is_local_speaking: bool,
    /// Whether local user has deafened (muted all incoming voice audio)
    pub is_deafened: bool,
    // ==================== System Tray (Windows/Linux only) ====================
    /// Show system tray icon
    pub show_tray_icon: bool,
    /// Minimize to tray instead of closing
    pub minimize_to_tray: bool,
    // ==================== Rendering ====================
    /// Whether hardware (GPU) rendering is enabled
    pub hardware_rendering: bool,
    /// GPU rendering backend selection
    pub gpu_backend: crate::config::settings::GpuBackend,
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

    // Count active + queued transfers for badge
    let transfer_count =
        config.transfer_manager.active().count() + config.transfer_manager.queued().count();

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
        transfer_count,
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
                show_connection_events: config.show_connection_events,
                show_join_leave_events: config.show_join_leave_events,
                chat_history_retention: config.chat_history_retention,
                max_scrollback: config.max_scrollback,
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
                show_hidden: config.show_hidden,
                transfer_manager: config.transfer_manager,
                queue_transfers: config.queue_transfers,
                download_limit: config.download_limit,
                upload_limit: config.upload_limit,
                show_drop_overlay: config.show_drop_overlay,
                event_settings: config.event_settings,
                notifications_enabled: config.notifications_enabled,
                sound_enabled: config.sound_enabled,
                sound_volume: config.sound_volume,
                voice_target: config.voice_target.clone(),
                output_devices: config.output_devices,
                selected_output_device: config.selected_output_device.clone(),
                input_devices: config.input_devices,
                selected_input_device: config.selected_input_device.clone(),
                voice_quality: config.voice_quality,
                ptt_key: config.ptt_key,
                ptt_capturing: config.ptt_capturing,
                ptt_mode: config.ptt_mode,
                ptt_release_delay: config.ptt_release_delay,
                mic_testing: config.mic_testing,
                mic_level: config.mic_level,
                settings_mic_level: config.settings_mic_level,
                mic_error: config.mic_error,
                noise_suppression_level: config.noise_suppression_level,
                echo_cancellation: config.echo_cancellation,
                agc: config.agc,
                transient_suppression: config.transient_suppression,
                mic_boost: config.mic_boost,
                is_local_speaking: config.is_local_speaking,
                is_deafened: config.is_deafened,
                show_tray_icon: config.show_tray_icon,
                minimize_to_tray: config.minimize_to_tray,
                hardware_rendering: config.hardware_rendering,
                gpu_backend: config.gpu_backend,
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
                        show_connection_events: config.show_connection_events,
                        show_join_leave_events: config.show_join_leave_events,
                        chat_history_retention: config.chat_history_retention,
                        max_scrollback: config.max_scrollback,
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
                        queue_transfers: config.queue_transfers,
                        download_limit: config.download_limit,
                        upload_limit: config.upload_limit,
                        event_settings: config.event_settings,
                        selected_event_type: config
                            .settings_form
                            .map(|f| f.selected_event_type)
                            .unwrap_or_default(),
                        notifications_enabled: config.notifications_enabled,
                        sound_enabled: config.sound_enabled,
                        sound_volume: config.sound_volume,
                        output_devices: config.output_devices,
                        selected_output_device: config.selected_output_device.clone(),
                        input_devices: config.input_devices,
                        selected_input_device: config.selected_input_device.clone(),
                        voice_quality: config.voice_quality,
                        ptt_key: config.ptt_key,
                        ptt_capturing: config.ptt_capturing,
                        ptt_mode: config.ptt_mode,
                        ptt_release_delay: config.ptt_release_delay,
                        mic_testing: config.mic_testing,
                        settings_mic_level: config.settings_mic_level,
                        mic_error: config.mic_error,
                        noise_suppression_level: config.noise_suppression_level,
                        echo_cancellation: config.echo_cancellation,
                        agc: config.agc,
                        transient_suppression: config.transient_suppression,
                        mic_boost: config.mic_boost,
                        show_tray_icon: config.show_tray_icon,
                        minimize_to_tray: config.minimize_to_tray,
                        hardware_rendering: config.hardware_rendering,
                        gpu_backend: config.gpu_backend,
                    })
                ]
                .width(Fill)
                .height(Fill)
                .into(),
                ActivePanel::About => stack![conn_form, about_view(config.theme.clone())]
                    .width(Fill)
                    .height(Fill)
                    .into(),
                ActivePanel::Transfers => {
                    stack![conn_form, transfers_view(config.transfer_manager)]
                        .width(Fill)
                        .height(Fill)
                        .into()
                }
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
    let has_files = state.has_permission(PERMISSION_FILE_LIST);
    let has_user_management = state.has_any_permission(&[
        PERMISSION_USER_CREATE,
        PERMISSION_USER_EDIT,
        PERMISSION_USER_DELETE,
    ]);
    let has_connection_monitor = state.has_permission(PERMISSION_CONNECTION_MONITOR);

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
                // Files button
                if state.is_connected && has_files {
                    tooltip(
                        button(icon::folder().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleFiles)
                            .style(toolbar_button_style(active_panel == ActivePanel::Files)),
                        container(shaped_text(t("tooltip-files")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::folder().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(shaped_text(t("tooltip-files")).size(TOOLTIP_TEXT_SIZE))
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
                        container(shaped_text(t("title-user-management")).size(TOOLTIP_TEXT_SIZE))
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
                        container(shaped_text(t("title-user-management")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
                // Connection Monitor button
                if state.is_connected && has_connection_monitor {
                    tooltip(
                        button(icon::desktop().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleConnectionMonitor)
                            .style(toolbar_button_style(
                                active_panel == ActivePanel::ConnectionMonitor,
                            )),
                        container(
                            shaped_text(t("tooltip-connection-monitor")).size(TOOLTIP_TEXT_SIZE),
                        )
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::desktop().size(TOOLBAR_ICON_SIZE))
                            .style(disabled_icon_button_style),
                        container(
                            shaped_text(t("tooltip-connection-monitor")).size(TOOLTIP_TEXT_SIZE),
                        )
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
                // Transfers button (global - always enabled, with badge for active/queued count)
                if state.transfer_count > 0 {
                    tooltip(
                        stack![
                            button(icon::exchange().size(TOOLBAR_ICON_SIZE))
                                .on_press(Message::ToggleTransfers)
                                .style(toolbar_button_style(
                                    active_panel == ActivePanel::Transfers
                                )),
                            // Badge overlay positioned at top-right
                            container({
                                // Calculate badge width: circular for 1 digit, pill for 2+
                                let count_str = state.transfer_count.to_string();
                                let badge_width = if count_str.len() == 1 {
                                    BADGE_SIZE // Circular for single digit
                                } else {
                                    // Approximate width: font size * 0.6 per digit + padding
                                    (count_str.len() as f32 * BADGE_FONT_SIZE * 0.6)
                                        + (BADGE_PADDING_HORIZONTAL * 2.0)
                                };
                                container(shaped_text(count_str).size(BADGE_FONT_SIZE))
                                    .height(BADGE_HEIGHT)
                                    .width(badge_width)
                                    .align_x(Center)
                                    .align_y(Center)
                                    .style(badge_style)
                            })
                            .width(Fill)
                            .align_x(iced::alignment::Horizontal::Right)
                        ],
                        container(shaped_text(t("tooltip-transfers")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                } else {
                    tooltip(
                        button(icon::exchange().size(TOOLBAR_ICON_SIZE))
                            .on_press(Message::ToggleTransfers)
                            .style(toolbar_button_style(active_panel == ActivePanel::Transfers)),
                        container(shaped_text(t("tooltip-transfers")).size(TOOLTIP_TEXT_SIZE))
                            .padding(TOOLTIP_BACKGROUND_PADDING)
                            .style(tooltip_container_style),
                        tooltip::Position::Bottom,
                    )
                    .gap(TOOLTIP_GAP)
                    .padding(TOOLTIP_PADDING)
                },
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
        ctx.voice_target.clone(),
        ctx.is_local_speaking,
        ctx.is_deafened,
        ctx.mic_level,
    );

    // Build the main content based on active panel
    let main_content: Element<'a, Message> = match ctx.active_panel {
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
                show_connection_events: ctx.show_connection_events,
                show_join_leave_events: ctx.show_join_leave_events,
                chat_history_retention: ctx.chat_history_retention,
                max_scrollback: ctx.max_scrollback,
                chat_font_size: ctx.chat_font_size,
                timestamp_settings: ctx.timestamp_settings,
                settings_form: ctx.settings_form,
                nickname: ctx.nickname,
                proxy: ctx.proxy,
                download_path: ctx.download_path,
                queue_transfers: ctx.queue_transfers,
                download_limit: ctx.download_limit,
                upload_limit: ctx.upload_limit,
                event_settings: ctx.event_settings,
                selected_event_type: ctx
                    .settings_form
                    .map(|f| f.selected_event_type)
                    .unwrap_or_default(),
                notifications_enabled: ctx.notifications_enabled,
                sound_enabled: ctx.sound_enabled,
                sound_volume: ctx.sound_volume,
                output_devices: ctx.output_devices,
                selected_output_device: ctx.selected_output_device.clone(),
                input_devices: ctx.input_devices,
                selected_input_device: ctx.selected_input_device.clone(),
                voice_quality: ctx.voice_quality,
                ptt_key: ctx.ptt_key,
                ptt_capturing: ctx.ptt_capturing,
                ptt_mode: ctx.ptt_mode,
                ptt_release_delay: ctx.ptt_release_delay,
                mic_testing: ctx.mic_testing,
                settings_mic_level: ctx.settings_mic_level,
                mic_error: ctx.mic_error,
                noise_suppression_level: ctx.noise_suppression_level,
                echo_cancellation: ctx.echo_cancellation,
                agc: ctx.agc,
                transient_suppression: ctx.transient_suppression,
                mic_boost: ctx.mic_boost,
                show_tray_icon: ctx.show_tray_icon,
                minimize_to_tray: ctx.minimize_to_tray,
                hardware_rendering: ctx.hardware_rendering,
                gpu_backend: ctx.gpu_backend,
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
                max_transfers_per_ip: ctx.conn.max_transfers_per_ip,
                file_reindex_interval: ctx.conn.file_reindex_interval,
                persistent_channels: ctx.conn.persistent_channels.clone(),
                auto_join_channels: ctx.conn.auto_join_channels.clone(),
                cached_server_image: ctx.conn.cached_server_image.as_ref(),
                is_admin: ctx.conn.is_admin,
                active_tab: ctx.conn.server_info_tab,
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
        ActivePanel::Files => {
            let perms = FilePermissions {
                file_root: ctx.conn.has_permission(PERMISSION_FILE_ROOT),
                file_create_dir: ctx.conn.has_permission(PERMISSION_FILE_CREATE_DIR),
                file_info: ctx.conn.has_permission(PERMISSION_FILE_INFO),
                file_delete: ctx.conn.has_permission(PERMISSION_FILE_DELETE),
                file_rename: ctx.conn.has_permission(PERMISSION_FILE_RENAME),
                file_move: ctx.conn.has_permission(PERMISSION_FILE_MOVE),
                file_copy: ctx.conn.has_permission(PERMISSION_FILE_COPY),
                file_download: ctx.conn.has_permission(PERMISSION_FILE_DOWNLOAD),
                file_upload: ctx.conn.has_permission(PERMISSION_FILE_UPLOAD),
                file_search: ctx.conn.has_permission(PERMISSION_FILE_SEARCH),
            };
            stack![
                chat,
                files_view(
                    &ctx.conn.files_management,
                    perms,
                    ctx.show_hidden,
                    ctx.show_drop_overlay
                )
            ]
            .width(Fill)
            .height(Fill)
            .into()
        }
        ActivePanel::Transfers => stack![chat, transfers_view(ctx.transfer_manager)]
            .width(Fill)
            .height(Fill)
            .into(),
        ActivePanel::ConnectionMonitor => stack![
            chat,
            connection_monitor_view(ctx.conn, &ctx.conn.connection_monitor, ctx.theme.clone())
        ]
        .width(Fill)
        .height(Fill)
        .into(),
        ActivePanel::None => chat,
    };

    // If disconnect dialog is open, overlay it on top of everything
    if let Some(ref dialog_state) = ctx.conn.disconnect_dialog {
        stack![main_content, disconnect_dialog_view(ctx.conn, dialog_state)]
            .width(Fill)
            .height(Fill)
            .into()
    } else {
        main_content
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

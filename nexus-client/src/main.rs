//! Nexus BBS Client - GUI Application
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod avatar;
mod commands;
mod config;
mod fonts;
mod handlers;
mod i18n;
mod icon;
mod image;
mod network;
mod style;
mod types;
mod views;

use std::collections::{HashMap, HashSet, VecDeque};

use iced::widget::{Id, operation, text_editor};
use iced::{Element, Subscription, Task, Theme};

use style::{WINDOW_HEIGHT_MIN, WINDOW_TITLE, WINDOW_WIDTH_MIN};
use types::{
    BookmarkEditState, ConnectionFormState, FingerprintMismatch, InputId, Message,
    ServerConnection, SettingsFormState, SettingsTab, UiState, ViewConfig,
};

/// Application entry point
///
/// Configures the Iced application with window settings, fonts, and theme,
/// then starts the event loop.
pub fn main() -> iced::Result {
    // Load config early to get saved window position/size
    let config = config::Config::load();
    let window_size = iced::Size::new(config.settings.window_width, config.settings.window_height);
    let window_position = match (config.settings.window_x, config.settings.window_y) {
        (Some(x), Some(y)) => {
            iced::window::Position::Specific(iced::Point::new(x as f32, y as f32))
        }
        _ => iced::window::Position::default(),
    };

    iced::application(NexusApp::new, NexusApp::update, NexusApp::view)
        .title(WINDOW_TITLE)
        .theme(NexusApp::theme)
        .subscription(NexusApp::subscription)
        .window(iced::window::Settings {
            size: window_size,
            min_size: Some(iced::Size::new(WINDOW_WIDTH_MIN, WINDOW_HEIGHT_MIN)),
            position: window_position,
            exit_on_close_request: false,
            ..Default::default()
        })
        .font(fonts::SAUCECODE_PRO_MONO)
        .font(fonts::SAUCECODE_PRO_MONO_BOLD)
        .font(fonts::SAUCECODE_PRO_MONO_ITALIC)
        .font(fonts::SAUCECODE_PRO_MONO_BOLD_ITALIC)
        .font(icon::FONT)
        .font(iced_aw::ICED_AW_FONT_BYTES)
        .run()
}

/// Main application state for the Nexus BBS client
struct NexusApp {
    // -------------------------------------------------------------------------
    // Persistence
    // -------------------------------------------------------------------------
    /// Application configuration and server bookmarks
    config: config::Config,

    // -------------------------------------------------------------------------
    // Connections
    // -------------------------------------------------------------------------
    /// Active server connections by connection_id
    connections: HashMap<usize, ServerConnection>,
    /// Currently displayed connection
    active_connection: Option<usize>,
    /// Counter for generating unique connection IDs
    next_connection_id: usize,
    /// Set of bookmark indices currently connecting (prevents duplicate attempts)
    connecting_bookmarks: HashSet<usize>,

    // -------------------------------------------------------------------------
    // Forms
    // -------------------------------------------------------------------------
    /// Connection form inputs and state
    connection_form: ConnectionFormState,
    /// State for bookmark add/edit dialog
    bookmark_edit: BookmarkEditState,
    /// Currently focused input field
    focused_field: InputId,

    // -------------------------------------------------------------------------
    // UI State
    // -------------------------------------------------------------------------
    /// UI panel visibility toggles
    ui_state: UiState,
    /// Settings panel form state (present when settings panel is open)
    settings_form: Option<SettingsFormState>,
    /// Active settings tab (persisted across panel opens)
    settings_tab: SettingsTab,

    // -------------------------------------------------------------------------
    // Async / Transient
    // -------------------------------------------------------------------------
    /// Certificate fingerprint mismatch queue (for handling multiple mismatches)
    fingerprint_mismatch_queue: VecDeque<FingerprintMismatch>,
    /// Transient per-bookmark connection errors (not persisted to disk)
    bookmark_errors: HashMap<usize, String>,

    // -------------------------------------------------------------------------
    // Text Editor State (not Clone, stored outside ServerConnection)
    // -------------------------------------------------------------------------
    /// News body editor content, keyed by connection_id (used for both create and edit)
    news_body_content: HashMap<usize, text_editor::Content>,
}

impl Default for NexusApp {
    fn default() -> Self {
        let config = config::Config::load();
        Self {
            // Persistence
            config,
            // Connections
            connections: HashMap::new(),
            active_connection: None,
            next_connection_id: 0,
            connecting_bookmarks: HashSet::new(),
            // Forms
            connection_form: ConnectionFormState::default(),
            bookmark_edit: BookmarkEditState::default(),
            focused_field: InputId::ServerName,
            // UI State
            ui_state: UiState::default(),
            settings_form: None,
            settings_tab: SettingsTab::default(),
            // Async / Transient
            fingerprint_mismatch_queue: VecDeque::new(),
            bookmark_errors: HashMap::new(),
            // Text Editor State
            news_body_content: HashMap::new(),
        }
    }
}

impl NexusApp {
    /// Initialize the application with default state and auto-connect tasks
    ///
    /// Called once at startup to set up initial state and generate tasks for
    /// focusing the input field and auto-connecting to bookmarks.
    fn new() -> (Self, Task<Message>) {
        let app = Self::default();

        // Generate auto-connect tasks for bookmarks
        let auto_connect_tasks = autostart::generate_auto_connect_tasks(&app.config);

        // Combine focus task with auto-connect tasks
        let mut tasks = vec![operation::focus(Id::from(InputId::ServerName))];
        tasks.extend(auto_connect_tasks);

        (app, Task::batch(tasks))
    }

    /// Process a message and update application state
    ///
    /// Central message dispatcher that routes messages to their handlers.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Keyboard and window events
            Message::Event(event) => self.handle_keyboard_event(event),
            Message::NextChatTab => self.handle_next_chat_tab(),
            Message::PrevChatTab => self.handle_prev_chat_tab(),
            Message::TabPressed => self.handle_tab_navigation(),
            Message::WindowCloseRequested(id) => {
                // Query window size and position, then save and close
                iced::window::size(id).then(move |size| {
                    iced::window::position(id).map(move |point| Message::WindowSaveAndClose {
                        id,
                        width: size.width,
                        height: size.height,
                        x: point.map(|p| p.x as i32),
                        y: point.map(|p| p.y as i32),
                    })
                })
            }
            Message::WindowSaveAndClose {
                id,
                width,
                height,
                x,
                y,
            } => {
                // Save window dimensions and position
                self.config.settings.window_width = width;
                self.config.settings.window_height = height;
                self.config.settings.window_x = x;
                self.config.settings.window_y = y;
                let _ = self.config.save();
                iced::window::close(id)
            }

            // Connection management
            Message::ConnectPressed => self.handle_connect_pressed(),
            Message::ConnectToBookmark(index) => self.handle_connect_to_bookmark(index),
            Message::DisconnectFromServer(connection_id) => {
                self.handle_disconnect_from_server(connection_id)
            }
            Message::SwitchToConnection(connection_id) => {
                self.handle_switch_to_connection(connection_id)
            }

            // Connection form fields
            Message::AddBookmarkToggled(enabled) => self.handle_add_bookmark_toggled(enabled),
            Message::PasswordChanged(password) => self.handle_password_changed(password),
            Message::PortChanged(port) => self.handle_port_changed(port),
            Message::ServerAddressChanged(addr) => self.handle_server_address_changed(addr),
            Message::ServerNameChanged(name) => self.handle_server_name_changed(name),
            Message::UsernameChanged(username) => self.handle_username_changed(username),
            Message::NicknameChanged(nickname) => self.handle_nickname_changed(nickname),
            Message::ConnectionFormTabPressed => self.handle_connection_form_tab_pressed(),
            Message::ConnectionFormFocusResult(
                name,
                address,
                port,
                username,
                password,
                nickname,
            ) => self.handle_connection_form_focus_result(
                name, address, port, username, password, nickname,
            ),

            // Bookmark management
            Message::BookmarkAddressChanged(addr) => self.handle_bookmark_address_changed(addr),
            Message::BookmarkAutoConnectToggled(enabled) => {
                self.handle_bookmark_auto_connect_toggled(enabled)
            }
            Message::BookmarkNameChanged(name) => self.handle_bookmark_name_changed(name),
            Message::BookmarkPasswordChanged(password) => {
                self.handle_bookmark_password_changed(password)
            }
            Message::BookmarkPortChanged(port) => self.handle_bookmark_port_changed(port),
            Message::BookmarkUsernameChanged(username) => {
                self.handle_bookmark_username_changed(username)
            }
            Message::BookmarkNicknameChanged(nickname) => {
                self.handle_bookmark_nickname_changed(nickname)
            }
            Message::CancelBookmarkEdit => self.handle_cancel_bookmark_edit(),
            Message::DeleteBookmark(index) => self.handle_delete_bookmark(index),
            Message::SaveBookmark => self.handle_save_bookmark(),
            Message::ShowAddBookmark => self.handle_show_add_bookmark(),
            Message::ShowEditBookmark(index) => self.handle_show_edit_bookmark(index),
            Message::BookmarkEditTabPressed => self.handle_bookmark_edit_tab_pressed(),
            Message::BookmarkEditFocusResult(name, address, port, username, password, nickname) => {
                self.handle_bookmark_edit_focus_result(
                    name, address, port, username, password, nickname,
                )
            }

            // Certificate fingerprint
            Message::AcceptNewFingerprint => self.handle_accept_new_fingerprint(),
            Message::CancelFingerprintMismatch => self.handle_cancel_fingerprint_mismatch(),

            // Chat
            Message::ChatInputChanged(input) => self.handle_message_input_changed(input),
            Message::ChatTabComplete => self.handle_chat_tab_complete(),
            Message::ChatScrolled(viewport) => self.handle_chat_scrolled(viewport),
            Message::CloseUserMessageTab(nickname) => self.handle_close_user_message_tab(nickname),
            Message::SendMessagePressed => self.handle_send_message_pressed(),
            Message::SwitchChatTab(tab) => self.handle_switch_chat_tab(tab),

            // User list interactions
            Message::UserInfoIconClicked(nickname) => self.handle_user_info_icon_clicked(nickname),
            Message::UserKickIconClicked(nickname) => self.handle_user_kick_icon_clicked(nickname),
            Message::UserListItemClicked(nickname) => self.handle_user_list_item_clicked(nickname),
            Message::UserMessageIconClicked(nickname) => {
                self.handle_user_message_icon_clicked(nickname)
            }

            // User management
            Message::CancelUserManagement => self.handle_cancel_user_management(),
            Message::ToggleUserManagement => self.handle_toggle_user_management(),
            Message::UserManagementShowCreate => self.handle_user_management_show_create(),
            Message::UserManagementUsernameChanged(username) => {
                self.handle_user_management_username_changed(username)
            }
            Message::UserManagementPasswordChanged(password) => {
                self.handle_user_management_password_changed(password)
            }
            Message::UserManagementIsAdminToggled(is_admin) => {
                self.handle_user_management_is_admin_toggled(is_admin)
            }
            Message::UserManagementIsSharedToggled(is_shared) => {
                self.handle_user_management_is_shared_toggled(is_shared)
            }
            Message::UserManagementEnabledToggled(enabled) => {
                self.handle_user_management_enabled_toggled(enabled)
            }
            Message::UserManagementPermissionToggled(permission, enabled) => {
                self.handle_user_management_permission_toggled(permission, enabled)
            }
            Message::UserManagementCreatePressed => self.handle_user_management_create_pressed(),
            Message::UserManagementEditClicked(username) => {
                self.handle_user_management_edit_clicked(username)
            }
            Message::UserManagementDeleteClicked(username) => {
                self.handle_user_management_delete_clicked(username)
            }
            Message::UserManagementConfirmDelete => self.handle_user_management_confirm_delete(),
            Message::UserManagementCancelDelete => self.handle_user_management_cancel_delete(),
            Message::UserManagementEditUsernameChanged(username) => {
                self.handle_user_management_edit_username_changed(username)
            }
            Message::UserManagementEditPasswordChanged(password) => {
                self.handle_user_management_edit_password_changed(password)
            }
            Message::UserManagementEditIsAdminToggled(is_admin) => {
                self.handle_user_management_edit_is_admin_toggled(is_admin)
            }
            Message::UserManagementEditEnabledToggled(enabled) => {
                self.handle_user_management_edit_enabled_toggled(enabled)
            }
            Message::UserManagementEditPermissionToggled(permission, enabled) => {
                self.handle_user_management_edit_permission_toggled(permission, enabled)
            }
            Message::UserManagementUpdatePressed => self.handle_user_management_update_pressed(),
            Message::ValidateUserManagementCreate => self.handle_validate_user_management_create(),
            Message::ValidateUserManagementEdit => self.handle_validate_user_management_edit(),
            Message::UserManagementCreateTabPressed => {
                self.handle_user_management_create_tab_pressed()
            }
            Message::UserManagementCreateFocusResult(username, password) => {
                self.handle_user_management_create_focus_result(username, password)
            }
            Message::UserManagementEditTabPressed => self.handle_user_management_edit_tab_pressed(),
            Message::UserManagementEditFocusResult(username, password) => {
                self.handle_user_management_edit_focus_result(username, password)
            }

            // Broadcast
            Message::BroadcastMessageChanged(input) => self.handle_broadcast_message_changed(input),
            Message::CancelBroadcast => self.handle_cancel_broadcast(),
            Message::SendBroadcastPressed => self.handle_send_broadcast_pressed(),
            Message::ValidateBroadcast => self.handle_validate_broadcast(),

            // UI toggles
            Message::ShowChatView => self.handle_show_chat_view(),
            Message::ToggleBookmarks => self.handle_toggle_bookmarks(),
            Message::ToggleBroadcast => self.handle_toggle_broadcast(),
            Message::ToggleUserList => self.handle_toggle_user_list(),

            // Settings
            Message::CancelSettings => self.handle_cancel_settings(),
            Message::ChatFontSizeSelected(size) => self.handle_chat_font_size_selected(size),
            Message::ClearAvatarPressed => self.handle_clear_avatar_pressed(),
            Message::ConnectionNotificationsToggled(enabled) => {
                self.handle_connection_notifications_toggled(enabled)
            }
            Message::AvatarLoaded(result) => self.handle_avatar_loaded(result),
            Message::PickAvatarPressed => self.handle_pick_avatar_pressed(),
            Message::SaveSettings => self.handle_save_settings(),
            Message::ShowSecondsToggled(enabled) => self.handle_show_seconds_toggled(enabled),
            Message::ShowTimestampsToggled(enabled) => self.handle_show_timestamps_toggled(enabled),
            Message::SettingsTabSelected(tab) => self.handle_settings_tab_selected(tab),
            Message::ThemeSelected(theme) => self.handle_theme_selected(theme),
            Message::SettingsNicknameChanged(nickname) => {
                self.handle_settings_nickname_changed(nickname)
            }
            Message::ToggleSettings => self.handle_toggle_settings(),
            Message::Use24HourTimeToggled(enabled) => self.handle_use_24_hour_time_toggled(enabled),
            Message::ProxyEnabledToggled(enabled) => self.handle_proxy_enabled_toggled(enabled),
            Message::ProxyAddressChanged(address) => self.handle_proxy_address_changed(address),
            Message::ProxyPortChanged(port) => self.handle_proxy_port_changed(port),
            Message::ProxyUsernameChanged(username) => self.handle_proxy_username_changed(username),
            Message::ProxyPasswordChanged(password) => self.handle_proxy_password_changed(password),
            Message::SettingsTabPressed => self.handle_settings_tab_pressed(),
            Message::SettingsNetworkFocusResult(address, port, username, password) => {
                self.handle_settings_network_focus_result(address, port, username, password)
            }
            Message::BrowseDownloadPathPressed => self.handle_browse_download_path_pressed(),
            Message::DownloadPathSelected(path) => self.handle_download_path_selected(path),

            // About
            Message::CloseAbout => self.handle_close_about(),
            Message::OpenUrl(url) => self.handle_open_url(url),
            Message::ShowAbout => self.handle_show_about(),

            // Server info
            Message::CancelEditServerInfo => self.handle_cancel_edit_server_info(),
            Message::ClearServerImagePressed => self.handle_clear_server_image_pressed(),
            Message::CloseServerInfo => self.handle_close_server_info(),
            Message::EditServerInfoDescriptionChanged(description) => {
                self.handle_edit_server_info_description_changed(description)
            }
            Message::EditServerInfoImageLoaded(result) => {
                self.handle_edit_server_info_image_loaded(result)
            }
            Message::EditServerInfoMaxConnectionsChanged(max_connections) => {
                self.handle_edit_server_info_max_connections_changed(max_connections)
            }
            Message::EditServerInfoNameChanged(name) => {
                self.handle_edit_server_info_name_changed(name)
            }
            Message::EditServerInfoPressed => self.handle_edit_server_info_pressed(),
            Message::PickServerImagePressed => self.handle_pick_server_image_pressed(),
            Message::ShowServerInfo => self.handle_show_server_info(),
            Message::UpdateServerInfoPressed => self.handle_update_server_info_pressed(),
            Message::ServerInfoEditTabPressed => self.handle_server_info_edit_tab_pressed(),
            Message::ServerInfoEditFocusResult(name, description) => {
                self.handle_server_info_edit_focus_result(name, description)
            }

            // User info
            Message::CloseUserInfo => self.handle_close_user_info(),

            // Password change
            Message::ChangePasswordPressed => self.handle_change_password_pressed(),
            Message::ChangePasswordCurrentChanged(value) => {
                self.handle_change_password_current_changed(value)
            }
            Message::ChangePasswordNewChanged(value) => {
                self.handle_change_password_new_changed(value)
            }
            Message::ChangePasswordConfirmChanged(value) => {
                self.handle_change_password_confirm_changed(value)
            }
            Message::ChangePasswordCancelPressed => self.handle_change_password_cancel_pressed(),
            Message::ChangePasswordSavePressed => self.handle_change_password_save_pressed(),
            Message::ChangePasswordTabPressed => self.handle_change_password_tab_pressed(),
            Message::ChangePasswordFocusResult(current, new, confirm) => {
                self.handle_change_password_focus_result(current, new, confirm)
            }

            // Network events (async results)
            Message::BookmarkConnectionResult {
                result,
                bookmark_index,
                display_name,
            } => self.handle_bookmark_connection_result(result, bookmark_index, display_name),
            Message::ConnectionResult(result) => self.handle_connection_result(result),
            Message::NetworkError(connection_id, error) => {
                self.handle_network_error(connection_id, error)
            }
            Message::ServerMessageReceived(connection_id, message_id, msg) => {
                self.handle_server_message_received(connection_id, message_id, msg)
            }

            // News management
            Message::ToggleNews => self.handle_toggle_news(),
            Message::CancelNews => self.handle_cancel_news(),
            Message::NewsShowCreate => self.handle_news_show_create(),
            Message::NewsEditClicked(id) => self.handle_news_edit_clicked(id),
            Message::NewsDeleteClicked(id) => self.handle_news_delete_clicked(id),
            Message::NewsConfirmDelete => self.handle_news_confirm_delete(),
            Message::NewsCancelDelete => self.handle_news_cancel_delete(),
            Message::NewsBodyAction(action) => self.handle_news_body_action(action),
            Message::NewsPickImagePressed => self.handle_news_pick_image_pressed(),
            Message::NewsImageLoaded(result) => self.handle_news_image_loaded(result),
            Message::NewsClearImagePressed => self.handle_news_clear_image_pressed(),
            Message::NewsSubmitPressed => self.handle_news_submit_pressed(),

            // Files panel
            Message::ToggleFiles => self.handle_toggle_files(),
            Message::CancelFiles => self.handle_cancel_files(),
            Message::FileNavigate(path) => self.handle_file_navigate(path),
            Message::FileNavigateUp => self.handle_file_navigate_up(),
            Message::FileNavigateHome => self.handle_file_navigate_home(),
            Message::FileRefresh => self.handle_file_refresh(),
        }
    }

    /// Set up subscriptions for keyboard events, window events, and network streams
    ///
    /// Creates subscriptions for keyboard events, window resize/move/close events,
    /// and network message streams for each active connection.
    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            // Keyboard and general events
            iced::event::listen().map(Message::Event),
            // Window close requests (we handle saving before exit)
            iced::window::close_requests().map(Message::WindowCloseRequested),
        ];

        // Subscribe to all active connections
        for conn in self.connections.values() {
            subscriptions.push(Subscription::run_with(
                conn.connection_id,
                network::network_stream,
            ));
        }

        Subscription::batch(subscriptions)
    }

    /// Render the current application state to the UI
    ///
    /// Delegates to `views::main_layout()` for all rendering logic.
    fn view(&self) -> Element<'_, Message> {
        // Get current connection state
        let active_conn = self
            .active_connection
            .and_then(|id| self.connections.get(&id));
        let message_input = active_conn.map(|c| c.message_input.as_str()).unwrap_or("");
        let user_management = active_conn.map(|c| &c.user_management);

        // Get news body content for current connection
        let news_body_content = self
            .active_connection
            .and_then(|id| self.news_body_content.get(&id));

        // Build view configuration
        let config = ViewConfig {
            theme: self.theme(),
            show_connection_notifications: self.config.settings.show_connection_notifications,
            chat_font_size: self.config.settings.chat_font_size,
            show_timestamps: self.config.settings.show_timestamps,
            use_24_hour_time: self.config.settings.use_24_hour_time,
            show_seconds: self.config.settings.show_seconds,
            settings_form: self.settings_form.as_ref(),
            connections: &self.connections,
            active_connection: self.active_connection,
            bookmarks: &self.config.bookmarks,
            bookmark_errors: &self.bookmark_errors,
            connection_form: &self.connection_form,
            bookmark_edit: &self.bookmark_edit,
            message_input,
            nickname: self.config.settings.nickname.as_deref().unwrap_or(""),
            user_management,
            ui_state: &self.ui_state,
            active_panel: self.active_panel(),
            news_body_content,
            proxy: &self.config.settings.proxy,
            download_path: self.config.settings.download_path.as_deref(),
        };

        let main_view = views::main_layout(config);

        // Overlay fingerprint mismatch dialog if present (show first in queue)
        if let Some(mismatch) = self.fingerprint_mismatch_queue.front() {
            return views::fingerprint_mismatch_dialog(mismatch);
        }

        main_view
    }

    /// Get the current theme based on configuration
    fn theme(&self) -> Theme {
        self.config.settings.theme.to_iced_theme()
    }
}

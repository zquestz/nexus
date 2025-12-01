//! Nexus BBS Client - GUI Application

mod autostart;
mod config;
mod fonts;
mod handlers;
mod i18n;
mod icon;
mod network;
mod style;
mod types;
mod views;

use std::collections::{HashMap, HashSet, VecDeque};

use iced::widget::text_input;
use iced::{Element, Subscription, Task, Theme};

use style::{WINDOW_HEIGHT, WINDOW_HEIGHT_MIN, WINDOW_TITLE, WINDOW_WIDTH, WINDOW_WIDTH_MIN};
use types::{
    BookmarkEditState, ConnectionFormState, FingerprintMismatch, InputId, Message,
    ServerConnection, SettingsFormState, UiState, ViewConfig,
};

/// Application entry point
///
/// Configures the Iced application with window settings, fonts, and theme,
/// then starts the event loop.
pub fn main() -> iced::Result {
    iced::application(WINDOW_TITLE, NexusApp::update, NexusApp::view)
        .theme(NexusApp::theme)
        .subscription(NexusApp::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            min_size: Some(iced::Size::new(WINDOW_WIDTH_MIN, WINDOW_HEIGHT_MIN)),
            ..Default::default()
        })
        .font(fonts::SAUCECODE_PRO_MONO)
        .font(fonts::SAUCECODE_PRO_MONO_BOLD)
        .font(fonts::SAUCECODE_PRO_MONO_ITALIC)
        .font(fonts::SAUCECODE_PRO_MONO_BOLD_ITALIC)
        .font(icon::FONT)
        .run_with(NexusApp::new)
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

    // -------------------------------------------------------------------------
    // Async / Transient
    // -------------------------------------------------------------------------
    /// Certificate fingerprint mismatch queue (for handling multiple mismatches)
    fingerprint_mismatch_queue: VecDeque<FingerprintMismatch>,
    /// Transient per-bookmark connection errors (not persisted to disk)
    bookmark_errors: HashMap<usize, String>,
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
            // Async / Transient
            fingerprint_mismatch_queue: VecDeque::new(),
            bookmark_errors: HashMap::new(),
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
        let mut tasks = vec![text_input::focus(text_input::Id::from(InputId::ServerName))];
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
            Message::TabPressed => self.handle_tab_navigation(),

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
            Message::CancelBookmarkEdit => self.handle_cancel_bookmark_edit(),
            Message::DeleteBookmark(index) => self.handle_delete_bookmark(index),
            Message::SaveBookmark => self.handle_save_bookmark(),
            Message::ShowAddBookmark => self.handle_show_add_bookmark(),
            Message::ShowEditBookmark(index) => self.handle_show_edit_bookmark(index),

            // Certificate fingerprint
            Message::AcceptNewFingerprint => self.handle_accept_new_fingerprint(),
            Message::CancelFingerprintMismatch => self.handle_cancel_fingerprint_mismatch(),

            // Chat
            Message::ChatInputChanged(input) => self.handle_message_input_changed(input),
            Message::ChatScrolled(viewport) => self.handle_chat_scrolled(viewport),
            Message::CloseUserMessageTab(username) => self.handle_close_user_message_tab(username),
            Message::SendMessagePressed => self.handle_send_message_pressed(),
            Message::SwitchChatTab(tab) => self.handle_switch_chat_tab(tab),

            // User list interactions
            Message::UserInfoIconClicked(username) => self.handle_user_info_icon_clicked(username),
            Message::UserKickIconClicked(username) => self.handle_user_kick_icon_clicked(username),
            Message::UserListItemClicked(username) => self.handle_user_list_item_clicked(username),
            Message::UserMessageIconClicked(username) => {
                self.handle_user_message_icon_clicked(username)
            }

            // User management - Add user
            Message::AdminEnabledToggled(enabled) => self.handle_admin_enabled_toggled(enabled),
            Message::AdminIsAdminToggled(is_admin) => self.handle_admin_is_admin_toggled(is_admin),
            Message::AdminPasswordChanged(password) => self.handle_admin_password_changed(password),
            Message::AdminPermissionToggled(permission, enabled) => {
                self.handle_admin_permission_toggled(permission, enabled)
            }
            Message::AdminUsernameChanged(username) => self.handle_admin_username_changed(username),
            Message::CancelAddUser => self.handle_cancel_add_user(),
            Message::CreateUserPressed => self.handle_create_user_pressed(),
            Message::DeleteUserPressed(username) => self.handle_delete_user_pressed(username),
            Message::ValidateCreateUser => self.handle_validate_create_user(),

            // User management - Edit user
            Message::CancelEditUser => self.handle_cancel_edit_user(),
            Message::EditEnabledToggled(enabled) => self.handle_edit_enabled_toggled(enabled),
            Message::EditIsAdminToggled(is_admin) => self.handle_edit_is_admin_toggled(is_admin),
            Message::EditNewPasswordChanged(new_password) => {
                self.handle_edit_new_password_changed(new_password)
            }
            Message::EditNewUsernameChanged(new_username) => {
                self.handle_edit_new_username_changed(new_username)
            }
            Message::EditPermissionToggled(permission, enabled) => {
                self.handle_edit_permission_toggled(permission, enabled)
            }
            Message::EditUsernameChanged(username) => self.handle_edit_username_changed(username),
            Message::EditUserPressed => self.handle_edit_user_pressed(),
            Message::UpdateUserPressed => self.handle_update_user_pressed(),
            Message::ValidateEditUser => self.handle_validate_edit_user(),

            // Broadcast
            Message::BroadcastMessageChanged(input) => self.handle_broadcast_message_changed(input),
            Message::CancelBroadcast => self.handle_cancel_broadcast(),
            Message::SendBroadcastPressed => self.handle_send_broadcast_pressed(),
            Message::ValidateBroadcast => self.handle_validate_broadcast(),

            // UI toggles
            Message::ShowChatView => self.handle_show_chat_view(),
            Message::ToggleAddUser => self.handle_toggle_add_user(),
            Message::ToggleBookmarks => self.handle_toggle_bookmarks(),
            Message::ToggleBroadcast => self.handle_toggle_broadcast(),
            Message::ToggleEditUser => self.handle_toggle_edit_user(),
            Message::ToggleUserList => self.handle_toggle_user_list(),

            // Settings
            Message::CancelSettings => self.handle_cancel_settings(),
            Message::ChatFontSizeSelected(size) => self.handle_chat_font_size_selected(size),
            Message::ConnectionNotificationsToggled(enabled) => {
                self.handle_connection_notifications_toggled(enabled)
            }
            Message::SaveSettings => self.handle_save_settings(),
            Message::ThemeSelected(theme) => self.handle_theme_selected(theme),
            Message::ToggleSettings => self.handle_toggle_settings(),

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
            Message::ServerMessageReceived(connection_id, msg) => {
                self.handle_server_message_received(connection_id, msg)
            }
        }
    }

    /// Set up subscriptions for keyboard events and network streams
    ///
    /// Creates subscriptions for keyboard events and network message streams
    /// for each active connection.
    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![iced::event::listen().map(Message::Event)];

        // Subscribe to all active connections
        for conn in self.connections.values() {
            subscriptions.push(Subscription::run_with_id(
                conn.connection_id,
                network::network_stream(conn.connection_id),
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

        // Build view configuration
        let config = ViewConfig {
            theme: self.theme(),
            show_connection_notifications: self.config.settings.show_connection_notifications,
            chat_font_size: self.config.settings.chat_font_size,
            connections: &self.connections,
            active_connection: self.active_connection,
            bookmarks: &self.config.bookmarks,
            bookmark_errors: &self.bookmark_errors,
            connection_form: &self.connection_form,
            bookmark_edit: &self.bookmark_edit,
            message_input,
            user_management,
            ui_state: &self.ui_state,
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

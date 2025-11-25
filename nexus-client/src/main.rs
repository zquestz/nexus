//! Nexus BBS Client - GUI Application

mod autostart;
mod config;
mod handlers;
mod icon;
mod network;
mod types;
mod views;

use iced::widget::text_input;
use iced::{Element, Subscription, Task, Theme};

use std::collections::HashMap;
use types::{
    BookmarkEditState, ConnectionFormState, DEFAULT_PORT, InputId, Message, ServerConnection,
    UiState, UserManagementState,
};
use config::ThemePreference;

/// Default window width
const WINDOW_WIDTH: f32 = 1200.0;

/// Default window height
const WINDOW_HEIGHT: f32 = 700.0;

/// Minimum window width
const MIN_WINDOW_WIDTH: f32 = 800.0;

/// Minimum window height
const MIN_WINDOW_HEIGHT: f32 = 500.0;

pub fn main() -> iced::Result {
    iced::application("Nexus BBS", NexusApp::update, NexusApp::view)
        .theme(NexusApp::theme)
        .subscription(NexusApp::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            min_size: Some(iced::Size::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)),
            ..Default::default()
        })
        .font(icon::FONT)
        .run_with(NexusApp::new)
}

/// Main application state for the Nexus BBS client
struct NexusApp {
    /// Application configuration and server bookmarks
    config: config::Config,
    /// State for bookmark add/edit dialog
    bookmark_edit: BookmarkEditState,
    /// Active server connections by connection_id
    connections: HashMap<usize, ServerConnection>,
    /// Currently displayed connection
    active_connection: Option<usize>,
    /// Counter for generating unique connection IDs
    next_connection_id: usize,
    /// Connection form inputs and state
    connection_form: ConnectionFormState,
    /// Currently focused input field
    focused_field: InputId,
    /// UI panel visibility toggles
    ui_state: UiState,
    /// Default user management state when no connection active
    default_user_mgmt: UserManagementState,
}

impl Default for NexusApp {
    fn default() -> Self {
        let config = config::Config::load();
        Self {
            config,
            bookmark_edit: BookmarkEditState::default(),
            connections: HashMap::new(),
            active_connection: None,
            next_connection_id: 0,
            connection_form: ConnectionFormState {
                port: DEFAULT_PORT.to_string(),
                ..Default::default()
            },
            focused_field: InputId::ServerName,
            ui_state: UiState {
                show_bookmarks: true,
                show_user_list: true,
                show_add_user: false,
                show_edit_user: false,
                show_broadcast: false,
            },
            default_user_mgmt: UserManagementState::default(),
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
            // Keyboard navigation
            Message::TabPressed => self.handle_tab_navigation(),
            Message::Event(event) => self.handle_keyboard_event(event),

            // Bookmark operations
            Message::ConnectToBookmark(index) => self.handle_connect_to_bookmark(index),
            Message::ShowAddBookmark => self.handle_show_add_bookmark(),
            Message::ShowEditBookmark(index) => self.handle_show_edit_bookmark(index),
            Message::CancelBookmarkEdit => self.handle_cancel_bookmark_edit(),
            Message::BookmarkNameChanged(name) => self.handle_bookmark_name_changed(name),
            Message::BookmarkAddressChanged(addr) => self.handle_bookmark_address_changed(addr),
            Message::BookmarkPortChanged(port) => self.handle_bookmark_port_changed(port),
            Message::BookmarkUsernameChanged(username) => {
                self.handle_bookmark_username_changed(username)
            }
            Message::BookmarkPasswordChanged(password) => {
                self.handle_bookmark_password_changed(password)
            }
            Message::BookmarkAutoConnectToggled(enabled) => {
                self.handle_bookmark_auto_connect_toggled(enabled)
            }
            Message::SaveBookmark => self.handle_save_bookmark(),
            Message::DeleteBookmark(index) => self.handle_delete_bookmark(index),

            // Connection screen events
            Message::ServerNameChanged(name) => self.handle_server_name_changed(name),
            Message::ServerAddressChanged(addr) => self.handle_server_address_changed(addr),
            Message::PortChanged(port) => self.handle_port_changed(port),
            Message::UsernameChanged(username) => self.handle_username_changed(username),
            Message::PasswordChanged(password) => self.handle_password_changed(password),
            Message::ConnectPressed => self.handle_connect_pressed(),

            // Chat operations
            Message::MessageInputChanged(input) => self.handle_message_input_changed(input),
            Message::SendMessagePressed => self.handle_send_message_pressed(),
            Message::RequestUserInfo(session_id) => self.handle_request_user_info(session_id),
            Message::DisconnectFromServer(connection_id) => {
                self.handle_disconnect_from_server(connection_id)
            }
            Message::SwitchToConnection(connection_id) => {
                self.handle_switch_to_connection(connection_id)
            }

            // User management (create user)
            Message::AdminUsernameChanged(username) => self.handle_admin_username_changed(username),
            Message::AdminPasswordChanged(password) => self.handle_admin_password_changed(password),
            Message::AdminIsAdminToggled(is_admin) => self.handle_admin_is_admin_toggled(is_admin),
            Message::AdminPermissionToggled(permission, enabled) => {
                self.handle_admin_permission_toggled(permission, enabled)
            }
            Message::CreateUserPressed => self.handle_create_user_pressed(),
            Message::DeleteUserPressed(username) => self.handle_delete_user_pressed(username),

            // User edit
            Message::EditUsernameChanged(username) => self.handle_edit_username_changed(username),
            Message::EditUserPressed => self.handle_edit_user_pressed(),
            Message::EditNewUsernameChanged(new_username) => {
                self.handle_edit_new_username_changed(new_username)
            }
            Message::EditNewPasswordChanged(new_password) => {
                self.handle_edit_new_password_changed(new_password)
            }
            Message::EditIsAdminToggled(is_admin) => self.handle_edit_is_admin_toggled(is_admin),
            Message::EditPermissionToggled(permission, enabled) => {
                self.handle_edit_permission_toggled(permission, enabled)
            }
            Message::UpdateUserPressed => self.handle_update_user_pressed(),
            Message::CancelEditUser => self.handle_cancel_edit_user(),

            // Broadcast
            Message::BroadcastMessageChanged(input) => self.handle_broadcast_message_changed(input),
            Message::SendBroadcastPressed => self.handle_send_broadcast_pressed(),

            // UI toggles
            Message::ToggleBookmarks => self.handle_toggle_bookmarks(),
            Message::ToggleUserList => self.handle_toggle_user_list(),
            Message::ToggleAddUser => self.handle_toggle_add_user(),
            Message::ToggleEditUser => self.handle_toggle_edit_user(),
            Message::ToggleBroadcast => self.handle_toggle_broadcast(),
            Message::ToggleTheme => self.handle_toggle_theme(),

            // Network events
            Message::ConnectionResult(result) => self.handle_connection_result(result),
            Message::BookmarkConnectionResult {
                result,
                bookmark_index,
                display_name,
            } => self.handle_bookmark_connection_result(result, bookmark_index, display_name),
            Message::ServerMessageReceived(connection_id, msg) => {
                self.handle_server_message_received(connection_id, msg)
            }
            Message::NetworkError(connection_id, error) => {
                self.handle_network_error(connection_id, error)
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
    fn view(&self) -> Element<Message> {
        // Get current connection state or use defaults
        let (message_input, user_management) = self
            .active_connection
            .and_then(|id| self.connections.get(&id))
            .map(|c| (c.message_input.as_str(), &c.user_management))
            .unwrap_or(("", &self.default_user_mgmt));

        views::main_layout(
            &self.connections,
            self.active_connection,
            &self.config.bookmarks,
            &self.bookmark_edit.mode,
            &self.connection_form.server_name,
            &self.connection_form.server_address,
            &self.connection_form.port,
            &self.connection_form.username,
            &self.connection_form.password,
            &self.connection_form.error,
            self.connection_form.is_connecting,
            &self.bookmark_edit.bookmark.name,
            &self.bookmark_edit.bookmark.address,
            &self.bookmark_edit.bookmark.port,
            &self.bookmark_edit.bookmark.username,
            &self.bookmark_edit.bookmark.password,
            self.bookmark_edit.bookmark.auto_connect,
            &self.bookmark_edit.error,
            message_input,
            user_management,
            self.ui_state.show_bookmarks,
            self.ui_state.show_user_list,
            self.ui_state.show_add_user,
            self.ui_state.show_edit_user,
            self.ui_state.show_broadcast,
        )
    }

    /// Get the current theme based on configuration
    fn theme(&self) -> Theme {
        match self.config.theme {
            ThemePreference::Light => Theme::Light,
            ThemePreference::Dark => Theme::Dark,
        }
    }
}

//! Nexus BBS Client - GUI Application

mod autostart;
mod config;
mod handlers;
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

pub fn main() -> iced::Result {
    iced::application("Nexus BBS", NexusApp::update, NexusApp::view)
        .theme(|_| Theme::Dark)
        .subscription(NexusApp::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1200.0, 700.0),
            min_size: Some(iced::Size::new(800.0, 500.0)),
            ..Default::default()
        })
        .run_with(NexusApp::new)
}

/// Main application state for the Nexus BBS client
///
/// This struct contains all state needed to run the client, including
/// configuration, active connections, UI state, and form inputs.
///
/// The state is organized into logical groups using dedicated state structs
/// to improve organization and reduce the number of top-level fields.
struct NexusApp {
    // Configuration and bookmarks
    /// Application configuration and saved bookmarks
    config: config::Config,
    /// Current bookmark being edited (if any)
    bookmark_edit: BookmarkEditState,

    // Multi-server connections
    /// Active server connections mapped by connection_id
    connections: HashMap<usize, ServerConnection>,
    /// Currently active/displayed connection (if any)
    active_connection: Option<usize>,
    /// Counter for generating unique connection IDs
    next_connection_id: usize,

    // Connection form state
    /// Connection form inputs and state
    connection_form: ConnectionFormState,
    /// Currently focused input field
    focused_field: InputId,

    // UI state
    /// UI panel visibility toggles
    ui_state: UiState,

    // Default state for views when no connection is active
    default_user_mgmt: UserManagementState,
}

impl Default for NexusApp {
    fn default() -> Self {
        let config = config::Config::load();
        Self {
            config,
            bookmark_edit: BookmarkEditState {
                port: DEFAULT_PORT.to_string(),
                ..Default::default()
            },
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
                show_delete_user: false,
                show_broadcast: false,
            },
            default_user_mgmt: UserManagementState::default(),
        }
    }
}

impl NexusApp {
    /// Initialize the application with default state and auto-connect tasks
    ///
    /// This method is called once at startup. It creates the default application
    /// state and generates tasks for:
    /// - Focusing the server name input field
    /// - Auto-connecting to bookmarks with `auto_connect` enabled
    ///
    /// # Returns
    ///
    /// A tuple of (app state, batched initialization tasks)
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
    /// This is the central message dispatcher that routes all messages to their
    /// appropriate handlers. The handlers are implemented in separate modules
    /// under `handlers/` for better organization.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to process
    ///
    /// # Returns
    ///
    /// A `Task` that may trigger additional messages (for async operations)
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
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
            Message::MessageInputChanged(input) => self.handle_message_input_changed(input),
            Message::SendMessagePressed => self.handle_send_message_pressed(),
            Message::RequestUserInfo(session_id) => self.handle_request_user_info(session_id),
            Message::DisconnectFromServer(connection_id) => {
                self.handle_disconnect_from_server(connection_id)
            }
            Message::SwitchToConnection(connection_id) => {
                self.handle_switch_to_connection(connection_id)
            }

            // User management
            Message::AdminUsernameChanged(username) => self.handle_admin_username_changed(username),
            Message::AdminPasswordChanged(password) => self.handle_admin_password_changed(password),
            Message::AdminIsAdminToggled(is_admin) => self.handle_admin_is_admin_toggled(is_admin),
            Message::AdminPermissionToggled(permission, enabled) => {
                self.handle_admin_permission_toggled(permission, enabled)
            }
            Message::CreateUserPressed => self.handle_create_user_pressed(),
            Message::DeleteUserPressed(username) => self.handle_delete_user_pressed(username),
            Message::DeleteUsernameChanged(username) => {
                self.handle_delete_username_changed(username)
            }

            // Broadcast
            Message::BroadcastMessageChanged(input) => {
                self.handle_broadcast_message_changed(input)
            }
            Message::SendBroadcastPressed => self.handle_send_broadcast_pressed(),

            // UI toggles
            Message::ToggleBookmarks => self.handle_toggle_bookmarks(),
            Message::ToggleUserList => self.handle_toggle_user_list(),
            Message::ToggleAddUser => self.handle_toggle_add_user(),
            Message::ToggleDeleteUser => self.handle_toggle_delete_user(),
            Message::ToggleBroadcast => self.handle_toggle_broadcast(),

            // Network events
            Message::ConnectionResult(result) => self.handle_connection_result(result),
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
    /// This method creates subscriptions for:
    /// - Keyboard events (Tab, Enter, Escape)
    /// - Network message streams for each active connection
    ///
    /// Subscriptions are dynamically created/destroyed as connections are
    /// added or removed.
    ///
    /// # Returns
    ///
    /// A batched subscription of all active subscriptions
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
    /// This method delegates to `views::main_layout()` which handles all
    /// UI rendering logic. The view function takes a snapshot of the current
    /// state and renders it to Iced Elements.
    ///
    /// # Returns
    ///
    /// The root UI element containing the entire application interface
    fn view(&self) -> Element<Message> {
        // Get current connection state or use defaults
        let (message_input, user_management) = self
            .active_connection
            .and_then(|id| self.connections.get(&id))
            .map(|c| (c.message_input.as_str(), &c.user_management))
            .unwrap_or(("", &self.default_user_mgmt));

        views::main_layout(
            &self.config.bookmarks,
            &self.connections,
            self.active_connection,
            &self.connection_form.server_name,
            &self.connection_form.server_address,
            &self.connection_form.port,
            &self.connection_form.username,
            &self.connection_form.password,
            &self.connection_form.error,
            &self.bookmark_edit.mode,
            &self.bookmark_edit.name,
            &self.bookmark_edit.address,
            &self.bookmark_edit.port,
            &self.bookmark_edit.username,
            &self.bookmark_edit.password,
            self.bookmark_edit.auto_connect,
            message_input,
            user_management,
            self.ui_state.show_bookmarks,
            self.ui_state.show_user_list,
            self.ui_state.show_add_user,
            self.ui_state.show_delete_user,
            self.ui_state.show_broadcast,
        )
    }
}

//! Nexus BBS Client - GUI Application

mod config;
mod icons;
mod network;
mod types;
mod views;

use chrono::Local;
use iced::keyboard::{self, key};
use iced::widget::{scrollable, text_input};
use iced::{Element, Event, Subscription, Task, Theme};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use std::collections::HashMap;
use types::{BookmarkEditMode, ChatMessage, InputId, Message, ScrollableId, ServerBookmark, ServerConnection, UserInfo};

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

struct NexusApp {
    // Configuration and bookmarks
    config: config::Config,
    bookmark_edit_mode: BookmarkEditMode,
    bookmark_name: String,
    bookmark_address: String,
    bookmark_port: String,
    bookmark_username: String,
    bookmark_password: String,

    // Multi-server connections
    connections: HashMap<usize, ServerConnection>, // connection_id -> connection
    active_connection: Option<usize>, // which connection_id is currently displayed
    next_connection_id: usize, // counter for generating connection IDs
    
    // Connection screen inputs (for connecting to new servers)
    server_name: String,
    server_address: String,
    port: String,
    username: String,
    password: String,
    connection_error: Option<String>,
    focused_field: InputId,

    // Message input (per active server)
    message_input: String,

    // Admin panel state (per active server)
    admin_username: String,
    admin_password: String,
    admin_is_admin: bool,
    admin_permissions: Vec<(String, bool)>, // (permission_name, is_enabled)
    delete_username: String,

    // UI state
    show_bookmarks: bool,
    show_userlist: bool,
    show_add_user: bool,
    show_delete_user: bool,
}

impl Default for NexusApp {
    fn default() -> Self {
        let config = config::Config::load();
        Self {
            config,
            bookmark_edit_mode: BookmarkEditMode::None,
            bookmark_name: String::new(),
            bookmark_address: String::new(),
            bookmark_port: String::from("7500"),
            bookmark_username: String::new(),
            bookmark_password: String::new(),
            connections: HashMap::new(),
            active_connection: None,
            next_connection_id: 0,
            server_name: String::new(),
            server_address: String::new(),
            port: String::from("7500"),
            username: String::new(),
            password: String::new(),
            connection_error: None,
            focused_field: InputId::ServerName,
            message_input: String::new(),
            admin_username: String::new(),
            admin_password: String::new(),
            admin_is_admin: false,
            admin_permissions: vec![
                ("user_list".to_string(), false),
                ("user_info".to_string(), false),
                ("chat_send".to_string(), false),
                ("chat_receive".to_string(), false),
                ("user_create".to_string(), false),
                ("user_delete".to_string(), false),
            ],
            delete_username: String::new(),
            show_bookmarks: true,
            show_userlist: true,
            show_add_user: false,
            show_delete_user: false,
        }
    }
}

impl NexusApp {
    fn new() -> (Self, Task<Message>) {
        (
            Self::default(),
            text_input::focus(text_input::Id::from(InputId::ServerName)),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Handle keyboard events for Tab navigation
            Message::TabPressed => {
                if self.bookmark_edit_mode != BookmarkEditMode::None {
                    // On bookmark edit screen, cycle through fields
                    let next_field = match self.focused_field {
                        InputId::BookmarkName => InputId::BookmarkAddress,
                        InputId::BookmarkAddress => InputId::BookmarkPort,
                        InputId::BookmarkPort => InputId::BookmarkUsername,
                        InputId::BookmarkUsername => InputId::BookmarkPassword,
                        InputId::BookmarkPassword => InputId::BookmarkName,
                        _ => InputId::BookmarkName,
                    };
                    self.focused_field = next_field.clone();
                    return text_input::focus(text_input::Id::from(next_field));
                } else if self.show_add_user {
                    // On add user screen, cycle through fields
                    let next_field = match self.focused_field {
                        InputId::AdminUsername => InputId::AdminPassword,
                        InputId::AdminPassword => InputId::AdminUsername,
                        _ => InputId::AdminUsername,
                    };
                    self.focused_field = next_field.clone();
                    return text_input::focus(text_input::Id::from(next_field));
                } else if self.show_delete_user {
                    // Delete user screen only has one field, so focus stays
                    self.focused_field = InputId::DeleteUsername;
                    return text_input::focus(text_input::Id::from(InputId::DeleteUsername));
                } else if self.active_connection.is_none() {
                    // On connection screen, cycle through fields
                    let next_field = match self.focused_field {
                        InputId::ServerName => InputId::ServerAddress,
                        InputId::ServerAddress => InputId::Port,
                        InputId::Port => InputId::Username,
                        InputId::Username => InputId::Password,
                        InputId::Password => InputId::ServerName,
                        _ => InputId::ServerName,
                    };
                    self.focused_field = next_field.clone();
                    return text_input::focus(text_input::Id::from(next_field));
                }
                Task::none()
            }

            Message::Event(event) => {
                if let Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Tab),
                    modifiers,
                    ..
                }) = event
                {
                    if !modifiers.shift() {
                        return self.update(Message::TabPressed);
                    }
                }
                // Handle Enter key
                if let Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Enter),
                    ..
                }) = event
                {
                    if self.bookmark_edit_mode != BookmarkEditMode::None {
                        // On bookmark edit screen, try to save
                        let can_save = !self.bookmark_name.trim().is_empty() 
                            && !self.bookmark_address.trim().is_empty() 
                            && !self.bookmark_port.trim().is_empty();
                        if can_save {
                            return self.update(Message::SaveBookmark);
                        }
                    } else if self.show_add_user {
                        // On add user screen, try to create user
                        let can_create = !self.admin_username.trim().is_empty() 
                            && !self.admin_password.trim().is_empty();
                        if can_create {
                            return self.update(Message::CreateUserPressed);
                        }
                    } else if self.show_delete_user {
                        // On delete user screen, try to delete user
                        let can_delete = !self.delete_username.trim().is_empty();
                        if can_delete {
                            return self.update(Message::DeleteUserPressed(self.delete_username.clone()));
                        }
                    } else if self.active_connection.is_none() {
                        // On connection screen, try to connect
                        let can_connect = !self.server_address.trim().is_empty() 
                            && !self.port.trim().is_empty() 
                            && !self.username.trim().is_empty();
                        if can_connect {
                            return self.update(Message::ConnectPressed);
                        }
                    }
                }
                // Handle Escape key
                if let Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Escape),
                    ..
                }) = event
                {
                    if self.bookmark_edit_mode != BookmarkEditMode::None {
                        // Cancel bookmark edit
                        return self.update(Message::CancelBookmarkEdit);
                    } else if self.show_add_user || self.show_delete_user {
                        // Cancel add/delete user screens
                        if self.show_add_user {
                            return self.update(Message::ToggleAddUser);
                        }
                        if self.show_delete_user {
                            return self.update(Message::ToggleDeleteUser);
                        }
                    }
                }
                Task::none()
            }

            // Connection screen
            // Server bookmark events
            Message::ConnectToBookmark(index) => {
                // Auto-fill connection form from bookmark
                if let Some(bookmark) = self.config.get_bookmark(index) {
                    self.server_name = bookmark.name.clone();
                    self.server_address = bookmark.address.clone();
                    self.port = bookmark.port.clone();
                    self.username = bookmark.username.clone();
                    self.password = bookmark.password.clone();
                    
                    // Auto-connect
                    return self.update(Message::ConnectPressed);
                }
                Task::none()
            }
            Message::ShowAddBookmark => {
                self.bookmark_edit_mode = BookmarkEditMode::Add;
                self.bookmark_name.clear();
                self.bookmark_address.clear();
                self.bookmark_port = String::from("7500");
                self.bookmark_username.clear();
                self.bookmark_password.clear();
                self.focused_field = InputId::BookmarkName;
                text_input::focus(text_input::Id::from(InputId::BookmarkName))
            }
            Message::ShowEditBookmark(index) => {
                if let Some(bookmark) = self.config.get_bookmark(index) {
                    self.bookmark_edit_mode = BookmarkEditMode::Edit(index);
                    self.bookmark_name = bookmark.name.clone();
                    self.bookmark_address = bookmark.address.clone();
                    self.bookmark_port = bookmark.port.clone();
                    self.bookmark_username = bookmark.username.clone();
                    self.bookmark_password = bookmark.password.clone();
                    self.focused_field = InputId::BookmarkName;
                    return text_input::focus(text_input::Id::from(InputId::BookmarkName));
                }
                Task::none()
            }
            Message::CancelBookmarkEdit => {
                self.bookmark_edit_mode = BookmarkEditMode::None;
                Task::none()
            }
            Message::BookmarkNameChanged(name) => {
                self.bookmark_name = name;
                self.focused_field = InputId::BookmarkName;
                Task::none()
            }
            Message::BookmarkAddressChanged(addr) => {
                self.bookmark_address = addr;
                self.focused_field = InputId::BookmarkAddress;
                Task::none()
            }
            Message::BookmarkPortChanged(port) => {
                self.bookmark_port = port;
                self.focused_field = InputId::BookmarkPort;
                Task::none()
            }
            Message::BookmarkUsernameChanged(username) => {
                self.bookmark_username = username;
                self.focused_field = InputId::BookmarkUsername;
                Task::none()
            }
            Message::BookmarkPasswordChanged(password) => {
                self.bookmark_password = password;
                self.focused_field = InputId::BookmarkPassword;
                Task::none()
            }
            Message::SaveBookmark => {
                let bookmark = ServerBookmark {
                    name: self.bookmark_name.clone(),
                    address: self.bookmark_address.clone(),
                    port: self.bookmark_port.clone(),
                    username: self.bookmark_username.clone(),
                    password: self.bookmark_password.clone(),
                };

                match self.bookmark_edit_mode {
                    BookmarkEditMode::Add => {
                        self.config.add_bookmark(bookmark);
                    }
                    BookmarkEditMode::Edit(index) => {
                        self.config.update_bookmark(index, bookmark);
                    }
                    BookmarkEditMode::None => {}
                }

                let _ = self.config.save();
                self.bookmark_edit_mode = BookmarkEditMode::None;
                Task::none()
            }
            Message::DeleteBookmark(index) => {
                self.config.delete_bookmark(index);
                let _ = self.config.save();
                Task::none()
            }

            // Connection screen events
            Message::ServerNameChanged(name) => {
                self.server_name = name;
                self.focused_field = InputId::ServerName;
                Task::none()
            }
            Message::ServerAddressChanged(addr) => {
                self.server_address = addr;
                self.focused_field = InputId::ServerAddress;
                Task::none()
            }
            Message::PortChanged(port) => {
                self.port = port;
                self.focused_field = InputId::Port;
                Task::none()
            }
            Message::UsernameChanged(username) => {
                self.username = username;
                self.focused_field = InputId::Username;
                Task::none()
            }
            Message::PasswordChanged(password) => {
                self.password = password;
                self.focused_field = InputId::Password;
                Task::none()
            }
            Message::ConnectPressed => {
                self.connection_error = None;

                let server_address = self.server_address.clone();
                let port = self.port.clone();
                let username = self.username.clone();
                let password = self.password.clone();
                let connection_id = self.next_connection_id;
                self.next_connection_id += 1;

                Task::perform(
                    async move {
                        let mut result = network::connect_to_server(server_address.clone(), port.clone(), username, password).await;
                        // Override connection_id with our pre-assigned one
                        if let Ok(ref mut conn) = result {
                            conn.connection_id = connection_id;
                        }
                        result
                    },
                    Message::ConnectionResult,
                )
            }

            // Main app events
            Message::MessageInputChanged(input) => {
                self.message_input = input;
                Task::none()
            }
            Message::SendMessagePressed => {
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
                        if !self.message_input.trim().is_empty() {
                            let msg = ClientMessage::ChatSend {
                                message: self.message_input.clone(),
                            };
                            let _ = conn.tx.send(msg);
                            self.message_input.clear();
                        }
                    }
                }
                Task::none()
            }

            Message::RequestUserInfo(session_id) => {
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let _ = conn.tx.send(ClientMessage::UserInfo { session_id });
                    }
                }
                Task::none()
            }
            Message::DisconnectFromServer(connection_id) => {
                if let Some(conn) = self.connections.remove(&connection_id) {
                    // Signal the network task to shutdown
                    let shutdown_arc = conn.shutdown_handle.clone();
                    tokio::spawn(async move {
                        let mut guard = shutdown_arc.lock().await;
                        if let Some(shutdown) = guard.take() {
                            shutdown.shutdown();
                        }
                    });
                    
                    // Clean up the receiver from the global registry
                    let conn_id = conn.connection_id;
                    let registry = network::NETWORK_RECEIVERS.clone();
                    tokio::spawn(async move {
                        let mut receivers = registry.lock().await;
                        receivers.remove(&conn_id);
                    });
                    
                    // If this was the active connection, clear active
                    if self.active_connection == Some(connection_id) {
                        self.active_connection = None;
                    }
                }
                Task::none()
            }
            
            Message::SwitchToConnection(connection_id) => {
                if self.connections.contains_key(&connection_id) {
                    self.active_connection = Some(connection_id);
                }
                Task::none()
            }

            // Admin panel
            Message::AdminUsernameChanged(username) => {
                self.admin_username = username;
                self.focused_field = InputId::AdminUsername;
                Task::none()
            }
            Message::AdminPasswordChanged(password) => {
                self.admin_password = password;
                self.focused_field = InputId::AdminPassword;
                Task::none()
            }
            Message::AdminIsAdminToggled(is_admin) => {
                self.admin_is_admin = is_admin;
                Task::none()
            }
            Message::AdminPermissionToggled(permission, enabled) => {
                if let Some(perm) = self.admin_permissions.iter_mut().find(|(p, _)| p == &permission) {
                    perm.1 = enabled;
                }
                Task::none()
            }
            Message::CreateUserPressed => {
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let permissions: Vec<String> = self.admin_permissions
                            .iter()
                            .filter(|(_, enabled)| *enabled)
                            .map(|(name, _)| name.clone())
                            .collect();

                        let msg = ClientMessage::UserCreate {
                            username: self.admin_username.clone(),
                            password: self.admin_password.clone(),
                            is_admin: self.admin_is_admin,
                            permissions,
                        };
                        let _ = conn.tx.send(msg);

                        // Clear the form and close the panel
                        self.admin_username.clear();
                        self.admin_password.clear();
                        self.admin_is_admin = false;
                        for (_, enabled) in &mut self.admin_permissions {
                            *enabled = false;
                        }
                        self.show_add_user = false;
                    }
                }
                Task::none()
            }
            Message::DeleteUserPressed(username) => {
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let msg = ClientMessage::UserDelete { username };
                        let _ = conn.tx.send(msg);
                        // Clear the form and close the panel
                        self.delete_username.clear();
                        self.show_delete_user = false;
                    }
                }
                Task::none()
            }
            Message::DeleteUsernameChanged(username) => {
                self.delete_username = username;
                self.focused_field = InputId::DeleteUsername;
                Task::none()
            }

            // UI toggles
            Message::ToggleBookmarks => {
                self.show_bookmarks = !self.show_bookmarks;
                Task::none()
            }
            Message::ToggleUserlist => {
                self.show_userlist = !self.show_userlist;
                Task::none()
            }
            Message::ToggleAddUser => {
                // Toggle Add User, and turn off Delete User
                self.show_add_user = !self.show_add_user;
                if self.show_add_user {
                    self.show_delete_user = false;
                    // Clear form and set focus
                    self.admin_username.clear();
                    self.admin_password.clear();
                    self.admin_is_admin = false;
                    for (_, enabled) in &mut self.admin_permissions {
                        *enabled = false;
                    }
                    self.focused_field = InputId::AdminUsername;
                    return text_input::focus(text_input::Id::from(InputId::AdminUsername));
                }
                Task::none()
            }
            Message::ToggleDeleteUser => {
                // Toggle Delete User, and turn off Add User
                self.show_delete_user = !self.show_delete_user;
                if self.show_delete_user {
                    self.show_add_user = false;
                    // Clear form and set focus
                    self.delete_username.clear();
                    self.focused_field = InputId::DeleteUsername;
                    return text_input::focus(text_input::Id::from(InputId::DeleteUsername));
                }
                Task::none()
            }

            // Network events
            Message::ConnectionResult(result) => match result {
                Ok(conn) => {
                    self.connection_error = None;
                    
                    // Find if this connection matches a bookmark
                    let bookmark_index = self.config.bookmarks
                        .iter()
                        .position(|b| {
                            b.address == self.server_address && 
                            b.port == self.port &&
                            b.username == self.username
                        });

                    let session_id = conn.session_id.parse().unwrap_or(0);
                    let username = self.username.clone();
                    let connection_id = conn.connection_id;
                    
                    // Create display name
                    let display_name = if !self.server_name.trim().is_empty() {
                        self.server_name.clone()
                    } else if let Some(idx) = bookmark_index {
                        self.config.bookmarks[idx].name.clone()
                    } else {
                        format!("{}:{}", self.server_address, self.port)
                    };
                    
                    // Request initial user list
                    let _ = conn.tx.send(ClientMessage::UserList);
                    
                    // Create server connection
                    let server_conn = ServerConnection {
                        bookmark_index,
                        session_id,
                        username,
                        display_name,
                        chat_messages: Vec::new(),
                        online_users: Vec::new(),
                        tx: conn.tx,
                        shutdown_handle: conn.shutdown.unwrap(),
                        connection_id,
                    };
                    
                    // Add to connections and make it active
                    self.connections.insert(connection_id, server_conn);
                    self.active_connection = Some(connection_id);
                    
                    // Clear connection form
                    self.server_name.clear();
                    self.server_address.clear();
                    self.port = String::from("7500");
                    self.username.clear();
                    self.password.clear();

                    Task::none()
                }
                Err(error) => {
                    self.connection_error = Some(error);
                    Task::none()
                }
            },
            Message::ServerMessageReceived(connection_id, msg) => {
                if self.connections.contains_key(&connection_id) {
                    self.handle_server_message(connection_id, msg)
                } else {
                    Task::none()
                }
            }
            Message::NetworkError(connection_id, error) => {
                // Connection has closed or errored - remove it from the list
                if let Some(conn) = self.connections.remove(&connection_id) {
                    // Clean up the receiver from the global registry
                    let registry = network::NETWORK_RECEIVERS.clone();
                    tokio::spawn(async move {
                        let mut receivers = registry.lock().await;
                        receivers.remove(&connection_id);
                    });
                    
                    // Signal the network task to shutdown
                    let shutdown_arc = conn.shutdown_handle.clone();
                    tokio::spawn(async move {
                        let mut guard = shutdown_arc.lock().await;
                        if let Some(shutdown) = guard.take() {
                            shutdown.shutdown();
                        }
                    });
                    
                    // If this was the active connection, clear it and show error
                    if self.active_connection == Some(connection_id) {
                        self.active_connection = None;
                        self.connection_error = Some(format!("Disconnected: {}", error));
                    }
                }
                Task::none()
            }
        }
    }

    fn handle_server_message(&mut self, connection_id: usize, msg: ServerMessage) -> Task<Message> {
        let conn = match self.connections.get_mut(&connection_id) {
            Some(c) => c,
            None => return Task::none(),
        };
        
        let is_active = self.active_connection == Some(connection_id);
        
        match msg {
            ServerMessage::ChatMessage {
                session_id,
                username,
                message,
            } => {
                conn.chat_messages.push(ChatMessage {
                    session_id,
                    username,
                    message,
                    timestamp: Local::now(),
                });
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserConnected { user } => {
                conn.online_users.push(UserInfo {
                    session_id: user.session_id,
                    username: user.username.clone(),
                });
                conn.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "System".to_string(),
                    message: format!("{} connected", user.username),
                    timestamp: Local::now(),
                });
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserDisconnected {
                session_id,
                username,
            } => {
                conn.online_users.retain(|u| u.session_id != session_id);
                conn.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "System".to_string(),
                    message: format!("{} disconnected", username),
                    timestamp: Local::now(),
                });
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserListResponse { users } => {
                conn.online_users = users
                    .into_iter()
                    .map(|u| UserInfo {
                        session_id: u.session_id,
                        username: u.username,
                    })
                    .collect();
                Task::none()
            }
            ServerMessage::UserInfoResponse { user, error } => {
                if let Some(err) = error {
                    conn.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Info".to_string(),
                        message: format!("Error: {}", err),
                        timestamp: Local::now(),
                    });
                } else if let Some(user) = user {
                    conn.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Info".to_string(),
                        message: format!(
                            "User {} (session {}): created {}, features: {:?}",
                            user.username, user.session_id, user.created_at, user.features
                        ),
                        timestamp: Local::now(),
                    });
                }
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserCreateResponse { success, error } => {
                if success {
                    conn.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "System".to_string(),
                        message: "User created successfully".to_string(),
                        timestamp: Local::now(),
                    });
                } else {
                    conn.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Error".to_string(),
                        message: format!("Failed to create user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    });
                }
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::UserDeleteResponse { success, error } => {
                if success {
                    conn.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "System".to_string(),
                        message: "User deleted successfully".to_string(),
                        timestamp: Local::now(),
                    });
                } else {
                    conn.chat_messages.push(ChatMessage {
                        session_id: 0,
                        username: "Error".to_string(),
                        message: format!("Failed to delete user: {}", error.unwrap_or_default()),
                        timestamp: Local::now(),
                    });
                }
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            ServerMessage::Error { message, .. } => {
                conn.chat_messages.push(ChatMessage {
                    session_id: 0,
                    username: "Error".to_string(),
                    message,
                    timestamp: Local::now(),
                });
                if is_active {
                    scrollable::snap_to(
                        ScrollableId::ChatMessages.into(),
                        scrollable::RelativeOffset::END,
                    )
                } else {
                    Task::none()
                }
            }
            _ => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![iced::event::listen().map(Message::Event)];
        
        // Subscribe to all active connections
        for conn in self.connections.values() {
            subscriptions.push(
                Subscription::run_with_id(
                    conn.connection_id,
                    network::network_stream(conn.connection_id)
                )
            );
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<Message> {
        views::main_layout(
            &self.config.bookmarks,
            &self.connections,
            self.active_connection,
            &self.server_name,
            &self.server_address,
            &self.port,
            &self.username,
            &self.password,
            &self.connection_error,
            &self.bookmark_edit_mode,
            &self.bookmark_name,
            &self.bookmark_address,
            &self.bookmark_port,
            &self.bookmark_username,
            &self.bookmark_password,
            &self.message_input,
            &self.admin_username,
            &self.admin_password,
            self.admin_is_admin,
            &self.admin_permissions,
            &self.delete_username,
            self.show_bookmarks,
            self.show_userlist,
            self.show_add_user,
            self.show_delete_user,
        )
    }
}

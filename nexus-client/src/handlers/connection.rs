//! Connection and chat message handlers

use crate::commands::{self, ParseResult};
use crate::i18n::{get_locale, t};
use crate::types::{ActivePanel, ChatMessage, ChatTab, InputId, Message, ScrollableId};
use crate::views::constants::{
    PERMISSION_USER_BROADCAST, PERMISSION_USER_CREATE, PERMISSION_USER_EDIT,
};
use crate::{NexusApp, network};
use iced::Task;
use iced::widget::{scrollable, text_input};
use nexus_common::protocol::ClientMessage;

/// Maximum length for chat messages
const MAX_CHAT_LENGTH: usize = 1024;

/// Threshold for considering scroll position "at bottom" (0.0 = top, 1.0 = bottom)
const SCROLL_BOTTOM_THRESHOLD: f32 = 0.99;

impl NexusApp {
    // ==================== Connection Form Fields ====================

    /// Handle add bookmark checkbox toggle
    pub fn handle_add_bookmark_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.connection_form.add_bookmark = enabled;
        Task::none()
    }

    /// Handle password field change
    pub fn handle_password_changed(&mut self, password: String) -> Task<Message> {
        self.connection_form.password = password;
        self.connection_form.error = None;
        self.focused_field = InputId::Password;
        Task::none()
    }

    /// Handle port field change
    pub fn handle_port_changed(&mut self, port: String) -> Task<Message> {
        self.connection_form.port = port;
        self.connection_form.error = None;
        self.focused_field = InputId::Port;
        Task::none()
    }

    /// Handle server address field change
    pub fn handle_server_address_changed(&mut self, addr: String) -> Task<Message> {
        self.connection_form.server_address = addr;
        self.connection_form.error = None;
        self.focused_field = InputId::ServerAddress;
        Task::none()
    }

    /// Handle server name field change
    pub fn handle_server_name_changed(&mut self, name: String) -> Task<Message> {
        self.connection_form.server_name = name;
        self.connection_form.error = None;
        self.focused_field = InputId::ServerName;
        Task::none()
    }

    /// Handle username field change
    pub fn handle_username_changed(&mut self, username: String) -> Task<Message> {
        self.connection_form.username = username;
        self.connection_form.error = None;
        self.focused_field = InputId::Username;
        Task::none()
    }

    // ==================== Connection Actions ====================

    /// Handle connect button press
    pub fn handle_connect_pressed(&mut self) -> Task<Message> {
        if self.connection_form.is_connecting {
            return Task::none();
        }

        self.connection_form.error = None;

        let port: u16 = match self.connection_form.port.parse() {
            Ok(p) => p,
            Err(_) => {
                self.connection_form.error = Some(t("err-port-invalid"));
                return Task::none();
            }
        };

        self.connection_form.is_connecting = true;

        let server_address = self.connection_form.server_address.clone();
        let username = self.connection_form.username.clone();
        let password = self.connection_form.password.clone();
        let locale = get_locale().to_string();
        let connection_id = self.next_connection_id;
        self.next_connection_id += 1;

        Task::perform(
            async move {
                network::connect_to_server(
                    server_address,
                    port,
                    username,
                    password,
                    locale,
                    connection_id,
                )
                .await
            },
            Message::ConnectionResult,
        )
    }

    /// Disconnect from a server and clean up resources
    pub fn handle_disconnect_from_server(&mut self, connection_id: usize) -> Task<Message> {
        if let Some(conn) = self.connections.remove(&connection_id) {
            let shutdown_arc = conn.shutdown_handle.clone();
            tokio::spawn(async move {
                let mut guard = shutdown_arc.lock().await;
                if let Some(shutdown) = guard.take() {
                    shutdown.shutdown();
                }
            });

            let conn_id = conn.connection_id;
            let registry = network::NETWORK_RECEIVERS.clone();
            tokio::spawn(async move {
                let mut receivers = registry.lock().await;
                receivers.remove(&conn_id);
            });

            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
            }
        }
        Task::none()
    }

    /// Switch active view to a different connection
    pub fn handle_switch_to_connection(&mut self, connection_id: usize) -> Task<Message> {
        if let Some(conn) = self.connections.get(&connection_id) {
            self.active_connection = Some(connection_id);

            let has_broadcast = conn.is_admin
                || conn
                    .permissions
                    .iter()
                    .any(|p| p == PERMISSION_USER_BROADCAST);
            let has_user_create =
                conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_CREATE);
            let has_user_edit =
                conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_EDIT);

            match self.ui_state.active_panel {
                ActivePanel::Broadcast if !has_broadcast => {
                    self.ui_state.active_panel = ActivePanel::None;
                }
                ActivePanel::AddUser if !has_user_create => {
                    self.ui_state.active_panel = ActivePanel::None;
                }
                ActivePanel::EditUser if !has_user_edit => {
                    self.ui_state.active_panel = ActivePanel::None;
                }
                _ => {}
            }

            return self.handle_show_chat_view();
        }
        Task::none()
    }

    // ==================== Chat Helpers ====================

    /// Scroll chat and focus input if chat view is visible (no panel overlay).
    ///
    /// Use this for background events (e.g., incoming messages) that shouldn't
    /// close panels or steal focus from panel input fields.
    pub fn scroll_chat_if_visible(&self) -> Task<Message> {
        // Don't scroll or steal focus if a panel is open
        if self.ui_state.active_panel != ActivePanel::None {
            return Task::none();
        }

        let scroll_state = self
            .active_connection
            .and_then(|id| self.connections.get(&id))
            .map(|conn| {
                conn.scroll_states
                    .get(&conn.active_chat_tab)
                    .copied()
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let scroll_offset = if scroll_state.auto_scroll {
            scrollable::RelativeOffset::END
        } else {
            scrollable::RelativeOffset {
                x: 0.0,
                y: scroll_state.offset,
            }
        };

        Task::batch([
            scrollable::snap_to(ScrollableId::ChatMessages.into(), scroll_offset),
            text_input::focus(text_input::Id::from(InputId::ChatInput)),
        ])
    }

    // ==================== Chat Handlers ====================

    /// Handle chat scroll position change
    pub fn handle_chat_scrolled(
        &mut self,
        viewport: iced::widget::scrollable::Viewport,
    ) -> Task<Message> {
        // Only track scroll when chat view is active (no panel overlay)
        if self.ui_state.active_panel != ActivePanel::None {
            return Task::none();
        }

        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(valid_offset) = Self::get_valid_scroll_offset(&viewport)
        {
            let tab = conn.active_chat_tab.clone();
            let scroll_state = conn.scroll_states.entry(tab).or_default();
            scroll_state.offset = valid_offset;
            scroll_state.auto_scroll = valid_offset >= SCROLL_BOTTOM_THRESHOLD;
        }
        Task::none()
    }

    /// Extract a valid scroll offset from a viewport, if applicable.
    ///
    /// Returns `None` when content fits in viewport (nothing to scroll).
    /// Spurious scroll events from panel transitions are handled separately
    /// via the panel check in `handle_chat_scrolled`.
    fn get_valid_scroll_offset(viewport: &iced::widget::scrollable::Viewport) -> Option<f32> {
        let bounds = viewport.bounds();
        let content_bounds = viewport.content_bounds();

        // Content fits in viewport - nothing to scroll, ignore event
        if content_bounds.height <= bounds.height {
            return None;
        }

        Some(viewport.relative_offset().y)
    }

    /// Close a user message tab
    pub fn handle_close_user_message_tab(&mut self, username: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_messages.remove(&username);

            let tab = ChatTab::UserMessage(username);
            conn.unread_tabs.remove(&tab);
            conn.scroll_states.remove(&tab);

            if conn.active_chat_tab == tab {
                conn.active_chat_tab = ChatTab::Server;
                return self.handle_show_chat_view();
            }
        }
        Task::none()
    }

    /// Handle chat message input change
    pub fn handle_message_input_changed(&mut self, input: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.message_input = input;
        }
        self.focused_field = InputId::ChatInput;
        Task::none()
    }

    /// Handle send chat message button press
    ///
    /// This method intercepts the input and checks for commands:
    /// - `/command` - Execute a client-side command
    /// - `//text` - Escape sequence, sends `/text` as a regular message
    /// - Regular text - Send as chat or private message
    pub fn handle_send_message_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get(&conn_id)
        {
            let input = conn.message_input.clone();

            // Parse input for commands
            match commands::parse_input(&input) {
                ParseResult::Empty => {
                    return Task::none();
                }
                ParseResult::Command(command) => {
                    // Clear input and execute command
                    if let Some(conn) = self.connections.get_mut(&conn_id) {
                        conn.message_input.clear();
                    }
                    return commands::execute_command(self, conn_id, command);
                }
                ParseResult::Message(message) => {
                    // Continue with normal message sending
                    if message.len() > MAX_CHAT_LENGTH {
                        let error_msg = format!(
                            "{} ({} characters, max {})",
                            t("err-message-too-long"),
                            message.len(),
                            MAX_CHAT_LENGTH
                        );
                        return self.add_chat_error(conn_id, error_msg);
                    }

                    // Re-borrow conn after potential mutable borrow above
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let msg = match &conn.active_chat_tab {
                            ChatTab::Server => ClientMessage::ChatSend { message },
                            ChatTab::UserMessage(username) => ClientMessage::UserMessage {
                                to_username: username.clone(),
                                message,
                            },
                        };

                        if let Err(e) = conn.tx.send(msg) {
                            let error_msg = format!("{}: {}", t("err-send-failed"), e);
                            return self.add_chat_error(conn_id, error_msg);
                        }
                    }

                    if let Some(conn) = self.connections.get_mut(&conn_id) {
                        conn.message_input.clear();
                    }
                }
            }
        }
        Task::none()
    }

    /// Switch to a different chat tab (Server or UserMessage)
    pub fn handle_switch_chat_tab(&mut self, tab: ChatTab) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.unread_tabs.remove(&tab);
            conn.active_chat_tab = tab;

            return self.handle_show_chat_view();
        }
        Task::none()
    }

    // ==================== Private Helpers ====================

    /// Add an error message to the chat
    fn add_chat_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::error(message))
    }
}

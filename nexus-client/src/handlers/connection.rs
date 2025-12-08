//! Connection and chat message handlers

use crate::commands::{self, ParseResult};
use crate::i18n::{get_locale, t, t_args};
use crate::types::{ActivePanel, ChatMessage, ChatTab, InputId, Message, ScrollableId};
use crate::views::constants::{PERMISSION_CHAT_SEND, PERMISSION_USER_MESSAGE};
use crate::{NexusApp, network};
use iced::Task;
use iced::widget::{Id, operation, scrollable};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, MessageError};

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
        let Some(conn) = self.connections.get(&connection_id) else {
            return Task::none();
        };

        // Copy permission data before mutable borrow
        let is_admin = conn.is_admin;
        let permissions = conn.permissions.clone();

        self.active_connection = Some(connection_id);

        // Close any panel the new connection doesn't have permission for
        self.close_panels_without_permission(is_admin, &permissions);

        self.handle_show_chat_view()
    }

    // ==================== Chat Helpers ====================

    /// Scroll chat if chat view is visible (no panel overlay).
    ///
    /// Use this for background events (e.g., incoming messages) that shouldn't
    /// close panels or steal focus from panel input fields.
    ///
    /// If `focus` is true, also focuses the chat input field.
    pub fn scroll_chat_if_visible(&self, focus: bool) -> Task<Message> {
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

        if focus {
            Task::batch([
                operation::snap_to(ScrollableId::ChatMessages, scroll_offset),
                operation::focus(Id::from(InputId::ChatInput)),
            ])
        } else {
            operation::snap_to(ScrollableId::ChatMessages, scroll_offset)
        }
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
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        let input = conn.message_input.clone();

        // Parse input for commands
        match commands::parse_input(&input) {
            ParseResult::Empty => Task::none(),
            ParseResult::Command(command) => {
                // Clear input and execute command
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.message_input.clear();
                }
                commands::execute_command(self, conn_id, command)
            }
            ParseResult::Message(message) => {
                // Check permission before sending
                let has_permission = match &conn.active_chat_tab {
                    ChatTab::Server => {
                        conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_CHAT_SEND)
                    }
                    ChatTab::UserMessage(_) => {
                        conn.is_admin
                            || conn
                                .permissions
                                .iter()
                                .any(|p| p == PERMISSION_USER_MESSAGE)
                    }
                };

                if !has_permission {
                    return self.add_chat_error(conn_id, t("err-no-chat-permission"));
                }

                // Validate message content using shared validators
                if let Err(e) = validators::validate_message(&message) {
                    let error_msg = match e {
                        MessageError::Empty => t("err-message-empty"),
                        MessageError::TooLong => t_args(
                            "err-message-too-long",
                            &[
                                ("length", &message.len().to_string()),
                                ("max", &validators::MAX_MESSAGE_LENGTH.to_string()),
                            ],
                        ),
                        MessageError::ContainsNewlines => t("err-message-contains-newlines"),
                        MessageError::InvalidCharacters => t("err-message-invalid-characters"),
                    };
                    return self.add_chat_error(conn_id, error_msg);
                }

                // Re-borrow conn after potential mutable borrow above
                let Some(conn) = self.connections.get(&conn_id) else {
                    return Task::none();
                };

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

                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.message_input.clear();
                }

                Task::none()
            }
        }
    }

    /// Switch to a different chat tab (Server or UserMessage)
    pub fn handle_switch_chat_tab(&mut self, tab: ChatTab) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.unread_tabs.remove(&tab);
        conn.active_chat_tab = tab;

        self.handle_show_chat_view()
    }

    // ==================== Private Helpers ====================

    /// Add an error message to the chat
    fn add_chat_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::error(message))
    }
}

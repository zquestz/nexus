//! Broadcast message handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, MessageError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, ChatMessage, InputId, Message};

impl NexusApp {
    // ==================== Panel Actions ====================

    /// Cancel/close broadcast panel
    pub fn handle_cancel_broadcast(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.broadcast_error = None;
        }

        self.handle_show_chat_view()
    }

    /// Handle showing chat view - closes all panels, scrolls to position, and focuses input.
    ///
    /// This is the single source of truth for returning to the chat view.
    /// All code paths that close panels and return to chat should use this method.
    pub fn handle_show_chat_view(&mut self) -> Task<Message> {
        self.set_active_panel(ActivePanel::None);
        self.scroll_chat_if_visible(true)
    }

    /// Show broadcast panel (does nothing if already shown)
    pub fn handle_toggle_broadcast(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::Broadcast {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::Broadcast);
        operation::focus(Id::from(InputId::BroadcastMessage))
    }

    // ==================== Form Handlers ====================

    /// Handle broadcast message input change
    pub fn handle_broadcast_message_changed(&mut self, input: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.broadcast_message = input;
        self.focused_field = InputId::BroadcastMessage;
        Task::none()
    }

    /// Handle validation of broadcast form (called on Enter when message empty)
    pub fn handle_validate_broadcast(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if let Err(MessageError::Empty) = validators::validate_message(&conn.broadcast_message) {
            conn.broadcast_error = Some(t("err-message-required"));
        }
        Task::none()
    }

    // ==================== Send Action ====================

    /// Handle send broadcast button press
    pub fn handle_send_broadcast_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        let message = conn.broadcast_message.trim().to_string();

        // Validate message content using shared validators
        if let Err(e) = validators::validate_message(&message) {
            let error_msg = match e {
                MessageError::Empty => return Task::none(),
                MessageError::TooLong => t_args(
                    "err-broadcast-too-long",
                    &[
                        ("length", &message.len().to_string()),
                        ("max", &validators::MAX_MESSAGE_LENGTH.to_string()),
                    ],
                ),
                MessageError::ContainsNewlines => t("err-message-contains-newlines"),
                MessageError::InvalidCharacters => t("err-message-invalid-characters"),
            };
            return self.add_broadcast_error(conn_id, error_msg);
        }

        let msg = ClientMessage::UserBroadcast { message };

        if let Err(e) = conn.send(msg) {
            let error_msg = format!("{}: {}", t("err-broadcast-send-failed"), e);
            return self.add_broadcast_error(conn_id, error_msg);
        }

        if let Some(conn) = self.connections.get_mut(&conn_id) {
            conn.broadcast_message.clear();
        }

        self.handle_show_chat_view()
    }

    // ==================== Private Helpers ====================

    /// Add an error message to the chat for broadcast errors
    /// Add a broadcast-specific error to chat
    fn add_broadcast_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::error(message))
    }
}

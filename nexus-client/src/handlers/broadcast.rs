//! Broadcast message handlers

use crate::NexusApp;
use crate::handlers::network::msg_username_error;
use crate::i18n::t;
use crate::types::{ChatMessage, ChatTab, InputId, Message, ScrollableId};
use chrono::Local;
use iced::Task;
use iced::widget::{scrollable, text_input};
use nexus_common::protocol::ClientMessage;

// Constants
const MAX_BROADCAST_LENGTH: usize = 1024;

impl NexusApp {
    /// Handle broadcast message input change
    pub fn handle_broadcast_message_changed(&mut self, input: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.broadcast_message = input;
        }
        self.focused_field = InputId::BroadcastMessage;
        Task::none()
    }

    /// Handle send broadcast button press
    pub fn handle_send_broadcast_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get(&conn_id)
        {
            let message = conn.broadcast_message.trim();

            // Validate message is not empty
            if message.is_empty() {
                return Task::none();
            }

            // Validate message length
            if message.len() > MAX_BROADCAST_LENGTH {
                let error_msg = format!(
                    "{} ({} characters, max {})",
                    t("err-broadcast-too-long"),
                    message.len(),
                    MAX_BROADCAST_LENGTH
                );
                return self.add_broadcast_error(conn_id, error_msg);
            }

            let msg = ClientMessage::UserBroadcast {
                message: message.to_string(),
            };

            // Send message and handle errors
            if let Err(e) = conn.tx.send(msg) {
                let error_msg = format!("{}: {}", t("err-broadcast-send-failed"), e);
                return self.add_broadcast_error(conn_id, error_msg);
            }

            // Clear message after successful send
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.broadcast_message.clear();
            }

            // Close broadcast panel and return focus to chat
            self.ui_state.show_broadcast = false;
            return text_input::focus(text_input::Id::from(InputId::ChatInput));
        }
        Task::none()
    }

    /// Handle toggle broadcast panel
    pub fn handle_toggle_broadcast(&mut self) -> Task<Message> {
        self.ui_state.show_broadcast = !self.ui_state.show_broadcast;

        // Close other admin panels when opening broadcast
        if self.ui_state.show_broadcast {
            self.ui_state.show_add_user = false;
            self.ui_state.show_edit_user = false;

            // Focus broadcast input when opening
            text_input::focus(text_input::Id::from(InputId::BroadcastMessage))
        } else {
            // Clear error when closing panel
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get_mut(&conn_id)
            {
                conn.broadcast_error = None;
            }

            // Return focus to chat when closing
            text_input::focus(text_input::Id::from(InputId::ChatInput))
        }
    }

    /// Handle showing chat view - closes all panels, switches to Server tab, and focuses chat input
    pub fn handle_show_chat_view(&mut self) -> Task<Message> {
        // Close all panels
        self.close_all_panels();

        // Check if we should auto-scroll
        let should_scroll = self
            .active_connection
            .and_then(|id| self.connections.get(&id))
            .is_some_and(|conn| conn.chat_auto_scroll);

        // Switch to Server tab, conditionally scroll, and focus chat input
        if should_scroll {
            Task::batch([
                Task::done(Message::SwitchChatTab(ChatTab::Server)),
                scrollable::snap_to(
                    ScrollableId::ChatMessages.into(),
                    scrollable::RelativeOffset::END,
                ),
                text_input::focus(text_input::Id::from(InputId::ChatInput)),
            ])
        } else {
            Task::batch([
                Task::done(Message::SwitchChatTab(ChatTab::Server)),
                text_input::focus(text_input::Id::from(InputId::ChatInput)),
            ])
        }
    }

    /// Handle validation of broadcast form (called on Enter when message empty)
    pub fn handle_validate_broadcast(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && conn.broadcast_message.trim().is_empty()
        {
            conn.broadcast_error = Some(t("err-message-required"));
        }
        Task::none()
    }

    /// Add an error message to the chat for broadcast errors and auto-scroll
    fn add_broadcast_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        self.add_chat_message(
            connection_id,
            ChatMessage {
                username: msg_username_error(),
                message,
                timestamp: Local::now(),
            },
        )
    }
}

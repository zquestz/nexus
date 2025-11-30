//! Broadcast message handlers

use crate::i18n::t;
use crate::types::{ActivePanel, ChatMessage, Message};
use crate::NexusApp;
use chrono::Local;
use iced::Task;

impl NexusApp {
    /// Handle incoming server broadcast message
    pub fn handle_server_broadcast(
        &mut self,
        connection_id: usize,
        username: String,
        message: String,
    ) -> Task<Message> {
        self.add_chat_message(
            connection_id,
            ChatMessage {
                username: format!("{} {}", t("msg-username-broadcast-prefix"), username),
                message,
                timestamp: Local::now(),
            },
        )
    }

    /// Handle user broadcast response (success/failure of sending a broadcast)
    pub fn handle_user_broadcast_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if success {
            // Close broadcast panel on success
            self.ui_state.active_panel = ActivePanel::None;
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.broadcast_error = None;
            }
            Task::none()
        } else {
            // On error, keep panel open and show error in form
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.broadcast_error = Some(error.unwrap_or_default());
            }
            Task::none()
        }
    }
}
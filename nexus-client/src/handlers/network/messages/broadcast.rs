//! Broadcast message handlers

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ActivePanel, ChatMessage, Message};
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
            ChatMessage::new(
                format!("{} {}", t("msg-username-broadcast-prefix"), username),
                message,
            ),
        )
    }

    /// Handle user broadcast response (success/failure of sending a broadcast)
    pub fn handle_user_broadcast_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            if success {
                self.ui_state.active_panel = ActivePanel::None;
                conn.broadcast_error = None;
            } else {
                conn.broadcast_error = Some(error.unwrap_or_default());
            }
        }
        Task::none()
    }
}

//! User kick response handler

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;

impl NexusApp {
    /// Handle user kick response
    pub fn handle_user_kick_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        username: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            // Show success message with username if available
            if let Some(ref name) = username {
                ChatMessage::info(t_args(
                    "msg-user-kicked-success-name",
                    &[("username", name)],
                ))
            } else {
                ChatMessage::info(t("msg-user-kicked-success"))
            }
        } else {
            ChatMessage::error(t_args(
                "err-failed-send-message",
                &[("error", &error.unwrap_or_default())],
            ))
        };
        self.add_chat_message(connection_id, message)
    }
}

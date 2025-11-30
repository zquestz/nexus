//! User kick response handler

use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use crate::NexusApp;
use chrono::Local;
use iced::Task;

impl NexusApp {
    /// Handle user kick response
    pub fn handle_user_kick_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            ChatMessage {
                username: t("msg-username-system"),
                message: t("msg-user-kicked-success"),
                timestamp: Local::now(),
            }
        } else {
            ChatMessage {
                username: t("msg-username-error"),
                message: t_args(
                    "err-failed-send-message",
                    &[("error", &error.unwrap_or_default())],
                ),
                timestamp: Local::now(),
            }
        };
        self.add_chat_message(connection_id, message)
    }
}
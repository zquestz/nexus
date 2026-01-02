//! User kick response handler

use iced::Task;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle user kick response
    pub fn handle_user_kick_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        nickname: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            // Show success message with nickname if available
            if let Some(ref name) = nickname {
                ChatMessage::info(t_args(
                    "msg-user-kicked-success-name",
                    &[("nickname", name)],
                ))
            } else {
                ChatMessage::info(t("msg-user-kicked-success"))
            }
        } else {
            // Show the server's error message directly
            ChatMessage::error(error.unwrap_or_default())
        };
        self.add_chat_message(connection_id, message)
    }
}

//! Chat message handlers

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;

impl NexusApp {
    /// Handle incoming chat message
    pub fn handle_chat_message(
        &mut self,
        connection_id: usize,
        username: String,
        message: String,
    ) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::new(username, message))
    }

    /// Handle chat topic change notification
    pub fn handle_chat_topic(
        &mut self,
        connection_id: usize,
        topic: String,
        username: String,
    ) -> Task<Message> {
        let message = if topic.is_empty() {
            t_args("msg-topic-cleared", &[("username", &username)])
        } else {
            t_args(
                "msg-topic-set",
                &[("username", &username), ("topic", &topic)],
            )
        };
        self.add_chat_message(
            connection_id,
            ChatMessage::new(t("msg-username-info"), message),
        )
    }

    /// Handle chat topic update response
    pub fn handle_chat_topic_update_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            ChatMessage::new(t("msg-username-system"), t("msg-topic-updated"))
        } else {
            ChatMessage::new(
                t("msg-username-error"),
                t_args(
                    "err-failed-update-topic",
                    &[("error", &error.unwrap_or_default())],
                ),
            )
        };
        self.add_chat_message(connection_id, message)
    }
}

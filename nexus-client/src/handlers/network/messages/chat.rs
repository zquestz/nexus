//! Chat message handlers

use iced::Task;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle incoming chat message
    pub fn handle_chat_message(
        &mut self,
        connection_id: usize,
        nickname: String,
        message: String,
        is_admin: bool,
        is_shared: bool,
    ) -> Task<Message> {
        let chat_message = ChatMessage::with_timestamp_and_status(
            nickname,
            message,
            chrono::Local::now(),
            is_admin,
            is_shared,
        );
        self.add_chat_message(connection_id, chat_message)
    }

    /// Handle chat topic change notification
    pub fn handle_chat_topic(
        &mut self,
        connection_id: usize,
        topic: String,
        username: String,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Build message first using references (before moving values)
        let message = if topic.is_empty() {
            t_args("msg-topic-cleared", &[("username", &username)])
        } else {
            t_args(
                "msg-topic-set",
                &[("username", &username), ("topic", &topic)],
            )
        };

        // Store values by moving (no clones needed)
        conn.chat_topic = if topic.is_empty() { None } else { Some(topic) };
        conn.chat_topic_set_by = if username.is_empty() {
            None
        } else {
            Some(username)
        };

        self.add_chat_message(connection_id, ChatMessage::system(message))
    }

    /// Handle chat topic update response
    pub fn handle_chat_topic_update_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            ChatMessage::info(t("msg-topic-updated"))
        } else {
            ChatMessage::error(t_args(
                "err-failed-update-topic",
                &[("error", &error.unwrap_or_default())],
            ))
        };
        self.add_chat_message(connection_id, message)
    }
}

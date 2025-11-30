//! Chat message handlers

use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use crate::NexusApp;
use chrono::Local;
use iced::Task;

impl NexusApp {
    /// Handle incoming chat message
    pub fn handle_chat_message(
        &mut self,
        connection_id: usize,
        username: String,
        message: String,
    ) -> Task<Message> {
        self.add_chat_message(
            connection_id,
            ChatMessage {
                username,
                message,
                timestamp: Local::now(),
            },
        )
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
            ChatMessage {
                username: t("msg-username-info"),
                message,
                timestamp: Local::now(),
            },
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
            ChatMessage {
                username: t("msg-username-system"),
                message: t("msg-topic-updated"),
                timestamp: Local::now(),
            }
        } else {
            ChatMessage {
                username: t("msg-username-error"),
                message: t_args(
                    "err-failed-update-topic",
                    &[("error", &error.unwrap_or_default())],
                ),
                timestamp: Local::now(),
            }
        };
        self.add_chat_message(connection_id, message)
    }
}
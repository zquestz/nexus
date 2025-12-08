//! Chat utility functions for network handlers

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message};
use iced::Task;

impl NexusApp {
    /// Add chat message and auto-scroll if this is the active connection
    pub fn add_chat_message(
        &mut self,
        connection_id: usize,
        mut message: ChatMessage,
    ) -> Task<Message> {
        // Set timestamp if not already set
        if message.timestamp.is_none() {
            message.timestamp = Some(chrono::Local::now());
        }

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        conn.chat_messages.push(message);

        // Mark Server tab as unread if not currently viewing it
        if conn.active_chat_tab != ChatTab::Server {
            conn.unread_tabs.insert(ChatTab::Server);
        }

        if self.active_connection == Some(connection_id) {
            return self.scroll_chat_if_visible(true);
        }

        Task::none()
    }

    /// Add chat topic message if present and not empty
    pub fn add_topic_message(
        &mut self,
        connection_id: usize,
        chat_topic: Option<String>,
        chat_topic_set_by: Option<String>,
    ) {
        if let Some(topic) = chat_topic
            && !topic.is_empty()
        {
            let message = match chat_topic_set_by {
                Some(ref username) if !username.is_empty() => t_args(
                    "msg-topic-set",
                    &[("username", username), ("topic", &topic)],
                ),
                _ => t_args("msg-topic-display", &[("topic", &topic)]),
            };
            let _ = self.add_chat_message(connection_id, ChatMessage::system(message));
        }
    }
}

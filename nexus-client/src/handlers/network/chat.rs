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

        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.chat_messages.push(message);

            // Mark Server tab as unread if not currently viewing it
            if conn.active_chat_tab != ChatTab::Server {
                conn.unread_tabs.insert(ChatTab::Server);
            }

            if self.active_connection == Some(connection_id) {
                return self.scroll_chat_if_visible();
            }
        }
        Task::none()
    }

    /// Add chat topic message if present and not empty
    pub fn add_topic_message(&mut self, connection_id: usize, chat_topic: Option<String>) {
        if let Some(topic) = chat_topic
            && !topic.is_empty()
        {
            let _ = self.add_chat_message(
                connection_id,
                ChatMessage::info(t_args("msg-topic-display", &[("topic", &topic)])),
            );
        }
    }
}

//! Chat utility functions for network handlers

use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message, ScrollableId};
use crate::NexusApp;
use chrono::Local;
use iced::widget::scrollable;
use iced::Task;

impl NexusApp {
    /// Add chat message and auto-scroll if this is the active connection
    pub fn add_chat_message(
        &mut self,
        connection_id: usize,
        message: ChatMessage,
    ) -> Task<Message> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.chat_messages.push(message);

            // Mark Server tab as unread if not currently viewing it
            if conn.active_chat_tab != ChatTab::Server {
                conn.unread_tabs.insert(ChatTab::Server);
            }

            if self.active_connection == Some(connection_id) && conn.chat_auto_scroll {
                return scrollable::snap_to(
                    ScrollableId::ChatMessages.into(),
                    scrollable::RelativeOffset::END,
                );
            }
        }
        Task::none()
    }

    /// Add chat topic message if present and not empty
    pub fn add_topic_message(
        &mut self,
        connection_id: usize,
        chat_topic: Option<String>,
    ) {
        if let Some(topic) = chat_topic
            && !topic.is_empty()
        {
            let _ = self.add_chat_message(
                connection_id,
                ChatMessage {
                    username: t("msg-username-info"),
                    message: t_args("msg-topic-display", &[("topic", &topic)]),
                    timestamp: Local::now(),
                },
            );
        }
    }
}
//! User message (private message) handlers

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message, ScrollableId};
use chrono::Local;
use iced::Task;
use iced::widget::scrollable;

impl NexusApp {
    /// Handle incoming private message
    pub fn handle_user_message(
        &mut self,
        connection_id: usize,
        from_username: String,
        to_username: String,
        message: String,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Determine which user we're chatting with (the other person)
        let other_user = if from_username == conn.username {
            to_username
        } else {
            from_username.clone()
        };

        // Add message to PM tab history (creates entry if doesn't exist)
        let chat_msg = ChatMessage::with_timestamp(from_username, message, Local::now());
        conn.user_messages
            .entry(other_user.clone())
            .or_default()
            .push(chat_msg);

        // Mark as unread if not currently viewing this tab
        let pm_tab = ChatTab::UserMessage(other_user);
        if conn.active_chat_tab != pm_tab {
            conn.unread_tabs.insert(pm_tab);
        } else if conn.chat_auto_scroll {
            // Auto-scroll if viewing this tab and at bottom
            return scrollable::snap_to(
                ScrollableId::ChatMessages.into(),
                scrollable::RelativeOffset::END,
            );
        }

        Task::none()
    }

    /// Handle user message response (success/failure of sending a PM)
    pub fn handle_user_message_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if success {
            return Task::none();
        }

        self.add_chat_message(
            connection_id,
            ChatMessage::error(t_args(
                "err-failed-send-message",
                &[("error", &error.unwrap_or_default())],
            )),
        )
    }
}

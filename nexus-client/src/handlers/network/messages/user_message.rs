//! User message (private message) handlers

use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message, ScrollableId};
use crate::NexusApp;
use chrono::Local;
use iced::widget::scrollable;
use iced::Task;

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
            // We sent this message, so we're chatting with to_username
            to_username
        } else {
            // We received this message, so we're chatting with from_username
            from_username.clone()
        };

        // Create PM tab entry if it doesn't exist
        if !conn.user_messages.contains_key(&other_user) {
            conn.user_messages.insert(other_user.clone(), Vec::new());
        }

        // Add message to PM tab history
        let chat_msg = ChatMessage {
            username: from_username,
            message,
            timestamp: Local::now(),
        };

        if let Some(messages) = conn.user_messages.get_mut(&other_user) {
            messages.push(chat_msg);
        }

        // Mark as unread if not currently viewing this tab
        let pm_tab = ChatTab::UserMessage(other_user.clone());
        if conn.active_chat_tab != pm_tab {
            conn.unread_tabs.insert(pm_tab);
        }

        // Auto-scroll if viewing this tab and at bottom
        if conn.active_chat_tab == ChatTab::UserMessage(other_user) && conn.chat_auto_scroll {
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
        // Only show error messages - success is obvious from the PM tab
        if !success {
            let message = ChatMessage {
                username: t("msg-username-error"),
                message: t_args(
                    "err-failed-send-message",
                    &[("error", &error.unwrap_or_default())],
                ),
                timestamp: Local::now(),
            };
            self.add_chat_message(connection_id, message)
        } else {
            Task::none()
        }
    }
}
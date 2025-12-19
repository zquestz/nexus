//! User message handlers

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message, ResponseRouting};
use chrono::Local;
use iced::Task;
use nexus_common::framing::MessageId;

impl NexusApp {
    /// Handle incoming private message
    pub fn handle_user_message(
        &mut self,
        connection_id: usize,
        from_username: String,
        from_admin: bool,
        to_username: String,
        message: String,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Get current user's display name for comparison
        // For shared accounts, this is the nickname; for regular accounts, this is the username
        let current_display_name = conn
            .online_users
            .iter()
            .find(|u| u.username == conn.username)
            .map(|u| u.display_name())
            .unwrap_or(&conn.username);

        // Determine which user we're chatting with (the other person)
        // Compare against display name since from_username is the sender's display name
        let other_user = if from_username == current_display_name {
            to_username
        } else {
            from_username.clone()
        };

        // Look up is_shared status from online_users (from_username is display name)
        let is_shared = conn
            .online_users
            .iter()
            .find(|u| u.display_name() == from_username)
            .map(|u| u.is_shared)
            .unwrap_or(false);

        // Add message to PM tab history (creates entry if doesn't exist)
        let chat_msg = ChatMessage::with_timestamp_and_status(
            from_username,
            message,
            Local::now(),
            from_admin,
            is_shared,
        );
        conn.user_messages
            .entry(other_user.clone())
            .or_default()
            .push(chat_msg);

        // Mark as unread if not currently viewing this tab
        let pm_tab = ChatTab::UserMessage(other_user);
        if conn.active_chat_tab != pm_tab {
            conn.unread_tabs.insert(pm_tab);
            Task::none()
        } else {
            self.scroll_chat_if_visible(true)
        }
    }

    /// Handle user message response (success/failure of sending a message)
    pub fn handle_user_message_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Check if this response corresponds to a tracked request
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        if success {
            // Switch to tab if this was a /msg command
            if let Some(ResponseRouting::OpenMessageTab(username)) = routing {
                return Task::done(Message::SwitchChatTab(ChatTab::UserMessage(username)));
            }
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

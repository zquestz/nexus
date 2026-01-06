//! User message handlers

use chrono::Local;
use iced::Task;
use nexus_common::framing::MessageId;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message, ResponseRouting};

impl NexusApp {
    /// Handle incoming private message
    pub fn handle_user_message(
        &mut self,
        connection_id: usize,
        from_nickname: String,
        from_admin: bool,
        to_nickname: String,
        message: String,
    ) -> Task<Message> {
        // First pass: get info we need for notification (immutable borrow)
        let notification_info = {
            let Some(conn) = self.connections.get(&connection_id) else {
                return Task::none();
            };

            // Get current user's nickname for comparison
            let current_nickname = conn
                .online_users
                .iter()
                .find(|u| u.session_ids.contains(&conn.session_id))
                .map(|u| u.nickname.clone())
                .unwrap_or_else(|| conn.connection_info.username.clone());

            // Check if this is a message from someone else
            let should_notify = from_nickname != current_nickname;

            // Determine which user we're chatting with (the other person)
            let other_user = if from_nickname == current_nickname {
                to_nickname.clone()
            } else {
                from_nickname.clone()
            };

            // Look up is_shared status
            let is_shared = conn
                .online_users
                .iter()
                .find(|u| u.nickname == from_nickname)
                .map(|u| u.is_shared)
                .unwrap_or(false);

            (should_notify, other_user, is_shared)
        };

        let (should_notify, other_user, is_shared) = notification_info;

        // Emit notification event (only for messages from others)
        if should_notify {
            emit_event(
                self,
                EventType::UserMessage,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_username(&from_nickname)
                    .with_message(&message),
            );
        }

        // Second pass: mutate state
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Add message to PM tab history (creates entry if doesn't exist)
        let chat_msg = ChatMessage::with_timestamp_and_status(
            from_nickname,
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
            if let Some(ResponseRouting::OpenMessageTab(nickname)) = routing {
                return Task::done(Message::SwitchChatTab(ChatTab::UserMessage(nickname)));
            }
            return Task::none();
        }

        // Build error message
        let error_msg = ChatMessage::error(t_args(
            "err-failed-send-message",
            &[("error", &error.unwrap_or_default())],
        ));

        // Route error to the appropriate tab
        match routing {
            Some(ResponseRouting::OpenMessageTab(nickname))
            | Some(ResponseRouting::ShowErrorInMessageTab(nickname)) => {
                let Some(conn) = self.connections.get_mut(&connection_id) else {
                    return Task::none();
                };

                // Only add to PM tab if it still exists (user didn't close it)
                if let Some(messages) = conn.user_messages.get_mut(&nickname) {
                    messages.push(error_msg);

                    // Scroll to bottom if we're viewing this tab
                    let pm_tab = ChatTab::UserMessage(nickname);
                    if conn.active_chat_tab == pm_tab {
                        return self.scroll_chat_if_visible(true);
                    }

                    Task::none()
                } else {
                    // Tab was closed, fall back to server chat
                    self.add_chat_message(connection_id, error_msg)
                }
            }
            _ => {
                // Default: add to server chat
                self.add_chat_message(connection_id, error_msg)
            }
        }
    }
}

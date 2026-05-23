//! User message handlers

use chrono::{Local, TimeZone};
use iced::Task;
use nexus_common::framing::MessageId;
use nexus_common::names::fold_name;
use nexus_common::protocol::{ChatAction, ServerMessage};

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message, ResponseRouting};

/// Parameters for handling an incoming user message
pub struct UserMessageParams {
    pub connection_id: usize,
    pub from_nickname: String,
    pub from_admin: bool,
    pub from_shared: bool,
    pub to_nickname: String,
    pub message: String,
    pub action: ChatAction,
    pub timestamp: u64,
}

impl NexusApp {
    /// Handle incoming user message
    pub fn handle_user_message(&mut self, params: UserMessageParams) -> Task<Message> {
        let UserMessageParams {
            connection_id,
            from_nickname,
            from_admin,
            from_shared,
            to_nickname,
            message,
            action,
            timestamp,
        } = params;

        // First pass: get info we need for notification and history (immutable borrow)
        let (should_notify, other_nickname) = {
            let Some(conn) = self.connections.get(&connection_id) else {
                return Task::none();
            };

            // Use server-confirmed nickname
            let current_nickname = conn.nickname.clone();

            // Check if this is a message from someone else (case-insensitive)
            let is_from_self = fold_name(&from_nickname) == fold_name(&current_nickname);
            let should_notify = !is_from_self;

            // Determine the other party's nickname
            // If we sent the message, the "other" is the recipient (to_nickname)
            // If someone else sent it, the "other" is the sender (from_nickname)
            let other_nickname = if is_from_self {
                to_nickname.clone()
            } else {
                from_nickname.clone()
            };

            (should_notify, other_nickname)
        };

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

        // Save to history (keyed by nickname)
        if let Some(base_dir) = self.connection_history_keys.get(&connection_id)
            && let Some(history_manager) = self.history_managers.get_mut(base_dir)
        {
            let server_msg = ServerMessage::UserMessage {
                from_nickname: from_nickname.clone(),
                from_admin,
                from_shared,
                to_nickname: to_nickname.clone(),
                message: message.clone(),
                action,
                timestamp,
            };
            // Silently ignore save failures - history is non-critical
            let _ = history_manager.add_message(&other_nickname, server_msg);
        }

        // Second pass: mutate state
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Add message to user message tab history (creates entry if doesn't exist)
        // Use server timestamp if available, otherwise fall back to local time
        let datetime = if timestamp > 0 {
            Local
                .timestamp_opt(timestamp as i64, 0)
                .single()
                .unwrap_or_else(Local::now)
        } else {
            Local::now()
        };
        let chat_msg = ChatMessage::with_timestamp_and_status(
            from_nickname,
            message,
            datetime,
            from_admin,
            from_shared,
            action,
        );
        conn.user_messages
            .entry(other_nickname.clone())
            .or_default()
            .push(chat_msg);

        // Add to user_message_tabs if not already present (creates the tab in UI)
        if !conn.user_message_tabs.contains(&other_nickname) {
            conn.user_message_tabs.push(other_nickname.clone());
        }

        // Mark as unread if not currently viewing this tab
        let pm_tab = ChatTab::UserMessage(other_nickname);
        if conn.active_chat_tab != pm_tab {
            conn.unread_tabs.insert(pm_tab);

            // Update tray icon state (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            self.update_tray_state();

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
        is_away: Option<bool>,
        status: Option<String>,
    ) -> Task<Message> {
        // Check if this response corresponds to a tracked request
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        if success {
            // Get the nickname for showing away notice
            let nickname_for_away = match &routing {
                Some(ResponseRouting::OpenMessageTab(nickname)) => Some(nickname.clone()),
                Some(ResponseRouting::ShowErrorInMessageTab(nickname)) => Some(nickname.clone()),
                _ => None,
            };

            // Show away notice if recipient is away
            if let Some(true) = is_away
                && let Some(nickname) = &nickname_for_away
            {
                let away_msg = if let Some(status_msg) = &status {
                    ChatMessage::info(t_args(
                        "msg-user-is-away-status",
                        &[
                            ("nickname", nickname.as_str()),
                            ("status", status_msg.as_str()),
                        ],
                    ))
                } else {
                    ChatMessage::info(t_args(
                        "msg-user-is-away",
                        &[("nickname", nickname.as_str())],
                    ))
                };

                // Add the away notice to the user message tab
                if let Some(conn) = self.connections.get_mut(&connection_id)
                    && let Some(messages) = conn.user_messages.get_mut(nickname)
                {
                    messages.push(away_msg);
                }
            }

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

                // Only add to user message tab if it still exists (user didn't close it)
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
                    self.add_active_tab_message(connection_id, error_msg)
                }
            }
            _ => {
                // Default: add to server chat
                self.add_active_tab_message(connection_id, error_msg)
            }
        }
    }
}

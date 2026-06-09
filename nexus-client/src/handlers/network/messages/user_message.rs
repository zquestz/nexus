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
    pub timestamp: i64,
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
        let mut history_error = None;
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
            if let Err(e) = history_manager.add_message(&other_nickname, server_msg) {
                history_error = Some(t_args(
                    "err-chat-history-save",
                    &[("nickname", &other_nickname), ("error", &e.to_string())],
                ));
            }
        }

        // Second pass: mutate state
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Add message to user message tab history (creates entry if doesn't exist)
        // Use server timestamp if available, otherwise fall back to local time
        let datetime = if timestamp > 0 {
            Local
                .timestamp_opt(timestamp, 0)
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
        // Resolve to the canonical DM tab (folded identity), then append under it.
        let key = conn.resolve_user_message_tab(&other_nickname);
        conn.user_messages
            .entry(key.clone())
            .or_default()
            .push(chat_msg);

        // Mark as unread if not currently viewing this tab
        let pm_tab = ChatTab::UserMessage(key);
        let display_task = if conn.active_chat_tab != pm_tab {
            conn.unread_tabs.insert(pm_tab);

            // Update tray icon state (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            self.update_tray_state();

            Task::none()
        } else {
            self.scroll_chat_if_visible(true)
        };

        if let Some(error) = history_error {
            Task::batch([
                display_task,
                self.add_background_error_message(connection_id, error),
            ])
        } else {
            display_task
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
            // For /msg, resolve (create) the canonical tab up front so the away notice
            // lands in it and we can switch to it. Uses the server-confirmed display name
            // from the online user list (correct casing regardless of what the user
            // typed); falls back to an existing tab's key, then the typed nickname when
            // the recipient isn't currently visible (offline, no user_list, etc.).
            let opened_tab_key = if let Some(ResponseRouting::OpenMessageTab(nickname)) = &routing {
                self.connections.get_mut(&connection_id).map(|conn| {
                    let canonical = conn
                        .online_users
                        .iter()
                        .find(|u| fold_name(&u.nickname) == fold_name(nickname))
                        .map(|u| u.nickname.clone())
                        .or_else(|| conn.user_message_tab_key(nickname))
                        .unwrap_or_else(|| nickname.clone());
                    conn.resolve_user_message_tab(&canonical)
                })
            } else {
                None
            };

            // Show away notice if recipient is away.
            if let Some(true) = is_away {
                let nickname_for_away = match &routing {
                    Some(ResponseRouting::OpenMessageTab(nickname))
                    | Some(ResponseRouting::ShowErrorInMessageTab(nickname)) => {
                        Some(nickname.clone())
                    }
                    _ => None,
                };
                if let Some(nickname) = &nickname_for_away {
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

                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        // /msg: the tab was just resolved above. ShowError: the tab is
                        // already open; don't re-create a closed one.
                        if let Some(key) = &opened_tab_key {
                            conn.user_messages
                                .entry(key.clone())
                                .or_default()
                                .push(away_msg);
                        } else if let Some(messages) = conn.user_messages_for_mut(nickname) {
                            messages.push(away_msg);
                        }
                    }
                }
            }

            // Switch to the tab if this was a /msg command.
            if let Some(key) = opened_tab_key {
                return Task::done(Message::SwitchChatTab(ChatTab::UserMessage(key)));
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

                // Only add to user message tab if it still exists (user didn't close it).
                // Folded lookup so a case-different reply still lands in the tab.
                if let Some(messages) = conn.user_messages_for_mut(&nickname) {
                    messages.push(error_msg);

                    // Scroll to bottom if we're viewing this tab
                    if conn.is_active_user_message_tab(&nickname) {
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

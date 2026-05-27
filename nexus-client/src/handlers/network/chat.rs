//! Chat utility functions for network handlers

use iced::Task;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message};

/// Truncate a message list to respect max_scrollback setting.
/// Removes oldest messages (from the front) when limit is exceeded.
/// A limit of 0 means unlimited.
fn truncate_scrollback(messages: &mut Vec<ChatMessage>, max_scrollback: usize) {
    if max_scrollback > 0 && messages.len() > max_scrollback {
        let excess = messages.len() - max_scrollback;
        messages.drain(0..excess);
    }
}

impl NexusApp {
    /// Add a message to the user's current active tab and auto-scroll
    ///
    /// Used for user-initiated actions like command output and errors.
    /// The message goes to wherever the user is currently looking (Console, Channel, or PM).
    pub fn add_active_tab_message(
        &mut self,
        connection_id: usize,
        message: ChatMessage,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get(&connection_id) else {
            return Task::none();
        };

        match &conn.active_chat_tab {
            ChatTab::Console => self.add_console_message(connection_id, message),
            ChatTab::Channel(channel) => {
                let channel = channel.clone();
                self.add_channel_message(connection_id, &channel, message)
            }
            ChatTab::UserMessage(nickname) => {
                let nickname = nickname.clone();
                self.add_user_message(connection_id, &nickname, message)
            }
        }
    }

    /// Add a message to the console and auto-scroll if this is the active connection
    ///
    /// Used for server-initiated events like broadcasts, permission changes, and
    /// user connect/disconnect notifications. These go to the Console tab.
    pub fn add_console_message(
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

        conn.console_messages.push(message);
        truncate_scrollback(
            &mut conn.console_messages,
            self.config.settings.max_scrollback,
        );

        // Mark Console tab as unread if not currently viewing it
        if conn.active_chat_tab != ChatTab::Console {
            conn.unread_tabs.insert(ChatTab::Console);
        }

        if self.active_connection == Some(connection_id) {
            return self.scroll_chat_if_visible(true);
        }

        Task::none()
    }

    /// Add a message to a user message tab and auto-scroll if viewing that tab
    ///
    /// Used for command output in user message tabs.
    pub fn add_user_message(
        &mut self,
        connection_id: usize,
        nickname: &str,
        mut message: ChatMessage,
    ) -> Task<Message> {
        // Set timestamp if not already set
        if message.timestamp.is_none() {
            message.timestamp = Some(chrono::Local::now());
        }

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Resolve to the canonical DM tab by folded identity (creates it, or relabels
        // a case-variant tab to this case), then append under that key.
        let key = conn.resolve_user_message_tab(nickname);
        let user_msgs = conn.user_messages.entry(key.clone()).or_default();
        user_msgs.push(message);
        truncate_scrollback(user_msgs, self.config.settings.max_scrollback);

        // Mark user message tab as unread if not currently viewing it
        let pm_tab = ChatTab::UserMessage(key);
        if conn.active_chat_tab != pm_tab {
            conn.unread_tabs.insert(pm_tab);
        }

        if self.active_connection == Some(connection_id) {
            return self.scroll_chat_if_visible(true);
        }

        Task::none()
    }

    /// Add a message to a specific channel and auto-scroll if viewing that channel
    ///
    /// Used for chat messages received in channels.
    pub fn add_channel_message(
        &mut self,
        connection_id: usize,
        channel: &str,
        mut message: ChatMessage,
    ) -> Task<Message> {
        // Set timestamp if not already set
        if message.timestamp.is_none() {
            message.timestamp = Some(chrono::Local::now());
        }

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Channel membership is session-based, and the server guarantees that a session
        // only receives channel messages for channels it has joined.
        //
        // So: do NOT auto-create channel state/tabs here. If we don't recognize the channel,
        // ignore the message (it likely arrived after a leave or during a tab close race).
        if conn.get_channel_state(channel).is_none() {
            return Task::none();
        }

        if let Some(channel_state) = conn.get_channel_state_mut(channel) {
            channel_state.messages.push(message);
            truncate_scrollback(
                &mut channel_state.messages,
                self.config.settings.max_scrollback,
            );
        }

        // Mark channel tab as unread if not currently viewing it
        let channel_tab = ChatTab::Channel(conn.get_channel_display_name(channel));
        if conn.active_chat_tab != channel_tab {
            conn.unread_tabs.insert(channel_tab);
        }

        if self.active_connection == Some(connection_id) {
            return self.scroll_chat_if_visible(true);
        }

        Task::none()
    }

    /// Add chat topic message to a channel if present and not empty
    pub fn add_topic_message(
        &mut self,
        connection_id: usize,
        channel: &str,
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
            let _ = self.add_channel_message(connection_id, channel, ChatMessage::system(message));
        }
    }
}

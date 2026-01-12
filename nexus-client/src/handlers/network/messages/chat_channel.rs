//! Channel-related message handlers
//!
//! Handles server messages for multi-channel chat:
//! - ChatJoinResponse - Response to /join command
//! - ChatLeaveResponse - Response to /leave command
//! - ChatSecretResponse - Response to /secret command
//! - ChatUserJoined - Notification when another user joins a channel
//! - ChatUserLeft - Notification when another user leaves a channel
//! - ChatListResponse - Response to /channels command

use iced::Task;
use nexus_common::protocol::ChannelInfo;

use nexus_common::framing::MessageId;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::{t, t_args};
use crate::types::{ChannelState, ChatMessage, ChatTab, Message, ResponseRouting};

/// Data from a ChatJoinResponse message
pub struct ChatJoinResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub channel: Option<String>,
    pub topic: Option<String>,
    pub topic_set_by: Option<String>,
    pub secret: Option<bool>,
    pub members: Option<Vec<String>>,
}

impl NexusApp {
    /// Handle response to ChatJoin request (user explicitly joined a channel)
    ///
    /// On success: Create channel tab, show topic as first message, set focus to new channel
    /// On error: Show error in console
    pub fn handle_chat_join_response(
        &mut self,
        connection_id: usize,
        data: ChatJoinResponseData,
    ) -> Task<Message> {
        if !data.success {
            let error_msg = data.error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-join-channel", &[("error", &error_msg)])),
            );
        }

        let Some(channel_name) = data.channel else {
            return Task::none();
        };

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if we're already in this channel (shouldn't happen, but handle gracefully)
        if let Some(channel_state) = conn.get_channel_state_mut(&channel_name) {
            // Update existing channel state with fresh data
            channel_state.topic = data.topic.clone();
            channel_state.topic_set_by = data.topic_set_by.clone();
            channel_state.secret = data.secret.unwrap_or(false);
            channel_state.members = data.members.clone().unwrap_or_default();
        } else {
            // Create new channel state
            let channel_state = ChannelState::new(
                data.topic.clone(),
                data.topic_set_by.clone(),
                data.secret.unwrap_or(false),
                data.members.unwrap_or_default(),
            );
            let channel_lower = channel_name.to_lowercase();
            conn.channels.insert(channel_lower, channel_state);
            conn.channel_tabs.push(channel_name.clone());
        }

        // Set active tab to the newly joined channel
        conn.active_chat_tab = ChatTab::Channel(channel_name.clone());

        // Clear unread marker since we're now viewing this tab
        conn.unread_tabs
            .remove(&ChatTab::Channel(channel_name.clone()));

        // Add topic message if present
        self.add_topic_message(connection_id, &channel_name, data.topic, data.topic_set_by);

        // Add secret indicator if channel is secret
        if data.secret.unwrap_or(false) {
            let _ = self.add_channel_message(
                connection_id,
                &channel_name,
                ChatMessage::system(t("msg-channel-is-secret")),
            );
        }

        // Focus chat input
        self.scroll_chat_if_visible(true)
    }

    /// Handle response to ChatLeave request
    ///
    /// On success: Remove channel tab and data, move focus to previous tab
    /// On error: Show error in console
    pub fn handle_chat_leave_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        channel: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Clear pending leave state
        conn.pending_channel_leave = None;

        if !success {
            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-leave-channel", &[("error", &error_msg)])),
            );
        }

        let Some(channel_name) = channel else {
            return Task::none();
        };

        self.remove_channel_tab(connection_id, &channel_name)
    }

    /// Handle ChatUserJoined - notification when another user joins a channel you're in
    ///
    /// Adds member to channel's member list and optionally shows a system message.
    pub fn handle_chat_user_joined(
        &mut self,
        connection_id: usize,
        channel: String,
        nickname: String,
        _is_admin: bool,
        _is_shared: bool,
    ) -> Task<Message> {
        // Emit event for notifications
        emit_event(
            self,
            EventType::ChatJoin,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_username(&nickname)
                .with_channel(&channel),
        );

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Add member to channel state
        if let Some(channel_state) = conn.get_channel_state_mut(&channel) {
            channel_state.add_member(nickname.clone());
        }

        // Add system message to the channel if notifications are enabled
        if self.config.settings.show_channel_notifications {
            let message = ChatMessage::system(t_args("msg-chat-join", &[("nickname", &nickname)]));
            self.add_channel_message(connection_id, &channel, message)
        } else {
            Task::none()
        }
    }

    /// Handle ChatUserLeft - notification when another user leaves a channel you're in
    ///
    /// Removes member from channel's member list and optionally shows a system message.
    pub fn handle_chat_user_left(
        &mut self,
        connection_id: usize,
        channel: String,
        nickname: String,
    ) -> Task<Message> {
        // Emit event for notifications
        emit_event(
            self,
            EventType::ChatLeave,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_username(&nickname)
                .with_channel(&channel),
        );

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Remove member from channel state
        if let Some(channel_state) = conn.get_channel_state_mut(&channel) {
            channel_state.remove_member(&nickname);
        }

        // Add system message to the channel if notifications are enabled
        if self.config.settings.show_channel_notifications {
            let message = ChatMessage::system(t_args("msg-chat-leave", &[("nickname", &nickname)]));
            self.add_channel_message(connection_id, &channel, message)
        } else {
            Task::none()
        }
    }

    /// Handle ChatListResponse - response to /channels command
    ///
    /// Displays the list of available channels in the console.
    pub fn handle_chat_list_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        channels: Option<Vec<ChannelInfo>>,
    ) -> Task<Message> {
        if !success {
            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-list-channels", &[("error", &error_msg)])),
            );
        }

        let channels = channels.unwrap_or_default();

        if channels.is_empty() {
            return self
                .add_active_tab_message(connection_id, ChatMessage::info(t("msg-no-channels")));
        }

        // Build channel list output
        let mut output = String::new();
        output.push_str(&t("msg-channel-list-header"));
        output.push('\n');

        for channel in &channels {
            // Format: #channel (N members) - Topic
            // Or: #channel (N members) [secret] - Topic
            let member_text = t_args(
                "msg-channel-member-count",
                &[("count", &channel.member_count.to_string())],
            );

            let secret_marker = if channel.secret {
                format!(" [{}]", t("channel-secret"))
            } else {
                String::new()
            };

            let topic_text = channel
                .topic
                .as_ref()
                .map(|t| format!(" - {}", t))
                .unwrap_or_default();

            output.push_str(&format!(
                "  {}{} ({}){}",
                channel.name, secret_marker, member_text, topic_text
            ));
            output.push('\n');
        }

        // Remove trailing newline
        output.pop();

        self.add_active_tab_message(connection_id, ChatMessage::info(output))
    }

    /// Handle response to ChatSecret request (toggle secret mode)
    ///
    /// On success: Update local channel state and show confirmation message
    /// On error: Show error in console
    ///
    /// Uses message_id to look up the pending request which contains the channel
    /// name and new secret value.
    pub fn handle_chat_secret_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Look up the pending request to get channel and secret value
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let pending = conn.pending_requests.remove(&message_id);
        let Some(ResponseRouting::SecretResult { channel, secret }) = pending else {
            // No pending request found - just show error if any
            if !success {
                let error_msg = error.unwrap_or_else(|| t("err-unknown"));
                return self.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
            }
            return Task::none();
        };

        if !success {
            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
        }

        // Update local channel state
        if let Some(channel_state) = conn.get_channel_state_mut(&channel) {
            channel_state.secret = secret;
        }

        // Show confirmation message in the channel
        let message = if secret {
            t("msg-secret-enabled")
        } else {
            t("msg-secret-disabled")
        };

        self.add_channel_message(connection_id, &channel, ChatMessage::info(message))
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    /// Remove a channel tab and its data, moving focus if necessary
    fn remove_channel_tab(&mut self, connection_id: usize, channel: &str) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let channel_lower = channel.to_lowercase();

        // Find the index of this channel in the tabs list
        let tab_index = conn
            .channel_tabs
            .iter()
            .position(|c| c.to_lowercase() == channel_lower);

        // Determine if we need to change focus
        let was_active = matches!(&conn.active_chat_tab, ChatTab::Channel(c) if c.to_lowercase() == channel_lower);

        // Remove channel data
        conn.channels.remove(&channel_lower);

        // Remove from tabs list and unread set
        if let Some(idx) = tab_index {
            let removed_name = conn.channel_tabs.remove(idx);
            conn.unread_tabs.remove(&ChatTab::Channel(removed_name));
        }

        // Remove scroll state for this tab
        // Find the actual tab name for the scroll state key
        conn.scroll_states.retain(
            |tab, _| !matches!(tab, ChatTab::Channel(c) if c.to_lowercase() == channel_lower),
        );

        // If this was the active tab, move to previous tab
        if was_active {
            let new_tab = if let Some(idx) = tab_index {
                // Try previous channel tab, or Console if no channels left
                if idx > 0 && idx <= conn.channel_tabs.len() {
                    ChatTab::Channel(conn.channel_tabs[idx - 1].clone())
                } else if !conn.channel_tabs.is_empty() {
                    ChatTab::Channel(conn.channel_tabs[0].clone())
                } else {
                    ChatTab::Console
                }
            } else {
                ChatTab::Console
            };

            conn.active_chat_tab = new_tab;
        }

        if self.active_connection == Some(connection_id) {
            // Focus chat input if this was the active tab
            self.scroll_chat_if_visible(was_active)
        } else {
            Task::none()
        }
    }
}

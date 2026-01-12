//! /leave command implementation - leave a channel

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, ChannelError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message};

/// Execute the /leave command
///
/// Leaves the current channel or a specified channel.
/// Usage: /leave [#channel]
///
/// If no channel is specified, leaves the currently active channel.
/// Cannot leave from Console or User Message tabs without specifying a channel.
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    _invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    // Determine which channel to leave
    let channel = if args.is_empty() {
        // /leave without argument - leave current channel
        match &conn.active_chat_tab {
            ChatTab::Channel(ch) => ch.clone(),
            ChatTab::Console | ChatTab::UserMessage(_) => {
                return app.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t("err-leave-no-channel")),
                );
            }
        }
    } else {
        // /leave #channel - leave specified channel
        let channel = &args[0];

        // Validate channel name
        if let Err(e) = validators::validate_channel(channel) {
            let error_msg = match e {
                ChannelError::Empty => t("err-channel-empty"),
                ChannelError::TooShort => t("err-channel-too-short"),
                ChannelError::TooLong => t_args(
                    "err-channel-too-long",
                    &[("max", &validators::MAX_CHANNEL_LENGTH.to_string())],
                ),
                ChannelError::MissingPrefix => t("err-channel-missing-prefix"),
                ChannelError::InvalidCharacters => t("err-channel-invalid-characters"),
            };
            return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
        }

        channel.clone()
    };

    // Check if we're actually in this channel
    let channel_lower = channel.to_lowercase();
    if !conn.channels.contains_key(&channel_lower) {
        return app.add_active_tab_message(
            connection_id,
            ChatMessage::error(t_args("err-not-in-channel", &[("channel", &channel)])),
        );
    }

    // Check if we already have a pending leave request for this channel
    if conn
        .pending_channel_leave
        .as_ref()
        .is_some_and(|c| c.to_lowercase() == channel_lower)
    {
        return app.add_active_tab_message(
            connection_id,
            ChatMessage::error(t("err-leave-already-pending")),
        );
    }

    // Set pending leave state
    conn.pending_channel_leave = Some(channel.clone());

    // Send ChatLeave message to server
    let msg = ClientMessage::ChatLeave {
        channel: channel.clone(),
    };

    if let Err(e) = conn.send(msg) {
        // Clear pending state on send error
        if let Some(conn) = app.connections.get_mut(&connection_id) {
            conn.pending_channel_leave = None;
        }
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

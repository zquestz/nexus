//! /join command implementation - join or create a channel

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, ChannelError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

/// Execute the /join command
///
/// Joins an existing channel or creates a new one if it doesn't exist.
/// Usage: /join #channel
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /join requires a channel name
    if args.is_empty() {
        let error_msg = t_args("cmd-join-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

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

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    // Send ChatJoin message to server
    let msg = ClientMessage::ChatJoin {
        channel: channel.clone(),
    };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

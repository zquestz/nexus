//! /leave command implementation - leave a channel

use iced::Task;
use nexus_common::names::fold_name;
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
    invoked_name: &str,
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
                    ChatMessage::error(t_args(
                        "err-leave-no-channel",
                        &[("command", invoked_name)],
                    )),
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
    let channel_lower = fold_name(&channel);
    if !conn.channels.contains_key(&channel_lower) {
        return app.add_active_tab_message(
            connection_id,
            ChatMessage::error(t_args("err-not-in-channel", &[("channel", &channel)])),
        );
    }

    app.send_chat_leave_once(connection_id, channel)
}

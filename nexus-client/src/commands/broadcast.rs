//! /broadcast command implementation - send broadcast to all users

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, MessageError};

/// Execute the /broadcast command
///
/// Sends a broadcast message to all connected users.
/// Usage: /broadcast <message>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    if args.is_empty() {
        let error_msg = t_args("cmd-broadcast-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let message = args.join(" ");

    // Validate message content
    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => t("err-message-empty"),
            MessageError::TooLong => t_args(
                "err-message-too-long",
                &[
                    ("length", &message.len().to_string()),
                    ("max", &validators::MAX_MESSAGE_LENGTH.to_string()),
                ],
            ),
            MessageError::ContainsNewlines => t("err-message-contains-newlines"),
            MessageError::InvalidCharacters => t("err-message-invalid-characters"),
        };
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let msg = ClientMessage::UserBroadcast { message };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

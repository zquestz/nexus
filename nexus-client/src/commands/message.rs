//! /message command implementation - send messages to users

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;

/// Execute the /message command
///
/// Sends a message to a user. If a message tab for that user already exists,
/// switches to it.
/// Usage: /message <username> <message>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // Need at least username and one word of message
    if args.len() < 2 {
        let error_msg = t_args("cmd-message-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let username = &args[0];
    let message = args[1..].join(" ");

    // Get connection and send message
    if let Some(conn) = app.connections.get(&connection_id) {
        let msg = ClientMessage::UserMessage {
            to_username: username.clone(),
            message,
        };

        if let Err(e) = conn.tx.send(msg) {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
    }

    // Mark that we want to switch to this user's tab on successful delivery
    if let Some(conn) = app.connections.get_mut(&connection_id) {
        conn.pending_message_tab = Some(username.clone());
    }

    Task::none()
}

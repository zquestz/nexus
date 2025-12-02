//! /broadcast command implementation - send broadcast to all users

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;

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
    let msg = ClientMessage::UserBroadcast { message };

    if let Err(e) = conn.tx.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}
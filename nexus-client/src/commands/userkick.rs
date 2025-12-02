//! /kick command implementation - kick users from server

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;

/// Execute the /kick command
///
/// Kicks a user from the server (disconnects them).
/// Usage: /kick <username>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /kick takes exactly 1 argument (username)
    if args.len() != 1 {
        let error_msg = t_args("cmd-kick-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let username = &args[0];
    let msg = ClientMessage::UserKick {
        username: username.clone(),
    };

    if let Err(e) = conn.tx.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}
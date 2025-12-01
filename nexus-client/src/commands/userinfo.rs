//! /userinfo command implementation - request user info from server

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;

/// Execute the /userinfo command
///
/// Requests information about a user from the server.
/// Usage: /userinfo <username>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    if args.is_empty() {
        let error_msg = t_args("cmd-userinfo-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let username = &args[0];

    if let Some(conn) = app.connections.get(&connection_id) {
        let msg = ClientMessage::UserInfo {
            username: username.clone(),
        };

        if let Err(e) = conn.tx.send(msg) {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
    }

    Task::none()
}

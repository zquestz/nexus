//! /userinfo command implementation - request user info from server

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, UsernameError};

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
    // /info takes exactly 1 argument (username)
    if args.len() != 1 {
        let error_msg = t_args("cmd-userinfo-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let username = &args[0];

    // Validate username
    if let Err(e) = validators::validate_username(username) {
        let error_msg = match e {
            UsernameError::Empty => t("err-username-empty"),
            UsernameError::TooLong => t_args(
                "err-username-too-long",
                &[("max", &validators::MAX_USERNAME_LENGTH.to_string())],
            ),
            UsernameError::InvalidCharacters => t("err-username-invalid"),
        };
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let msg = ClientMessage::UserInfo {
        username: username.clone(),
    };

    if let Err(e) = conn.tx.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

//! /kick command implementation - kick users from server

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, NicknameError};

/// Execute the /kick command
///
/// Kicks a user from the server (disconnects them).
/// Usage: /kick <nickname>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /kick takes exactly 1 argument (nickname)
    if args.len() != 1 {
        let error_msg = t_args("cmd-kick-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let nickname = &args[0];

    // Validate nickname
    if let Err(e) = validators::validate_nickname(nickname) {
        let error_msg = match e {
            NicknameError::Empty => t("err-nickname-empty"),
            NicknameError::TooLong => t_args(
                "err-nickname-too-long",
                &[("max", &validators::MAX_NICKNAME_LENGTH.to_string())],
            ),
            NicknameError::InvalidCharacters => t("err-nickname-invalid"),
        };
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let msg = ClientMessage::UserKick {
        nickname: nickname.clone(),
    };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

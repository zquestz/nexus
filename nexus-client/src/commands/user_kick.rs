//! /kick command implementation - kick users from server

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, NicknameError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

/// Execute the /kick command
///
/// Kicks a user from the server (disconnects them).
/// Usage: /kick <nickname> [reason]
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /kick takes 1 or more arguments (nickname + optional reason)
    if args.is_empty() {
        let error_msg = t_args("cmd-kick-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
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
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    // Join remaining args as reason (if any)
    let reason = if args.len() > 1 {
        Some(args[1..].join(" "))
    } else {
        None
    };

    let msg = ClientMessage::UserKick {
        nickname: nickname.clone(),
        reason,
    };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

//! /userinfo command implementation - request user info from server

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, NicknameError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};

/// Execute the /userinfo command
///
/// Requests information about a user from the server.
/// Usage: /userinfo <nickname>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /info takes exactly 1 argument (nickname)
    if args.len() != 1 {
        let error_msg = t_args("cmd-userinfo-usage", &[("command", invoked_name)]);
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

    let msg = ClientMessage::UserInfo {
        nickname: nickname.clone(),
    };

    let message_id = match conn.send(msg) {
        Ok(id) => id,
        Err(e) => {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
    };

    // Track this request so the response goes to chat, not the panel
    if let Some(conn) = app.connections.get_mut(&connection_id) {
        conn.pending_requests
            .track(message_id, ResponseRouting::DisplayUserInfoInChat);
    }

    Task::none()
}

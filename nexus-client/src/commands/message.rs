//! /message command implementation - send messages to users

use iced::Task;
use nexus_common::protocol::{ChatAction, ClientMessage};
use nexus_common::validators::{self, MessageError, NicknameError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};

/// Execute the /message command
///
/// Sends a message to a user. If a message tab for that user already exists,
/// switches to it.
/// Usage: /message <nickname> <message>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // Need at least nickname and one word of message
    if args.len() < 2 {
        let error_msg = t_args("cmd-message-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let nickname = &args[0];
    let message = args[1..].join(" ");

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
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    let msg = ClientMessage::UserMessage {
        to_nickname: nickname.clone(),
        message,
        action: ChatAction::Normal,
    };

    let message_id = match conn.send(msg) {
        Ok(id) => id,
        Err(e) => {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
        }
    };

    // Track this request so we can switch to the user's tab on successful delivery
    if let Some(conn) = app.connections.get_mut(&connection_id) {
        conn.pending_requests.track(
            message_id,
            ResponseRouting::OpenMessageTab(nickname.clone()),
        );
    }

    Task::none()
}

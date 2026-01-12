//! /me command implementation - send action messages

use iced::Task;
use nexus_common::protocol::{ChatAction, ClientMessage};
use nexus_common::validators::{self, MessageError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message};

/// Execute the /me command
///
/// Sends an action message to the current chat (server chat or PM tab).
///
/// Usage: /me <action>
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // Require at least one argument
    if args.is_empty() {
        let error_msg = t_args("cmd-me-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    // Join all arguments as the action message
    let message = args.join(" ");

    // Validate the message
    if let Err(e) = validators::validate_message(&message) {
        let error_msg = match e {
            MessageError::Empty => t("err-message-empty"),
            MessageError::TooLong => t_args(
                "err-chat-too-long",
                &[("max", &validators::MAX_MESSAGE_LENGTH.to_string())],
            ),
            MessageError::ContainsNewlines => t("err-message-contains-newlines"),
            MessageError::InvalidCharacters => t("err-message-invalid-characters"),
        };
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    // Get the connection
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    // Send to the appropriate target based on active chat tab
    match &conn.active_chat_tab {
        ChatTab::Console => {
            // Can't send /me to console - need to be in a channel or PM
            let error_msg = t("err-me-no-target");
            return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
        }
        ChatTab::Channel(channel) => {
            // Send to channel chat
            let msg = ClientMessage::ChatSend {
                message,
                action: ChatAction::Me,
                channel: channel.clone(),
            };
            if let Err(e) = conn.send(msg) {
                return app.add_active_tab_message(connection_id, ChatMessage::error(e));
            }
        }
        ChatTab::UserMessage(nickname) => {
            // Send as PM to the user
            let msg = ClientMessage::UserMessage {
                to_nickname: nickname.clone(),
                message,
                action: ChatAction::Me,
            };
            if let Err(e) = conn.send(msg) {
                return app.add_active_tab_message(connection_id, ChatMessage::error(e));
            }
        }
    }

    Task::none()
}

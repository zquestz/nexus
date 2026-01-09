//! /away command implementation - set away status

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, StatusError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};

/// Execute the /away command
///
/// Sets the user as away, optionally with a status message.
///
/// Usage: /away [message]
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    _invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // Get the connection
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    // Build optional message from arguments
    let message = if args.is_empty() {
        None
    } else {
        let msg = args.join(" ");

        // Validate the message
        if let Err(e) = validators::validate_status(&msg) {
            let error_msg = match e {
                StatusError::TooLong => t_args(
                    "err-status-too-long",
                    &[("max", &validators::MAX_STATUS_LENGTH.to_string())],
                ),
                StatusError::ContainsNewlines => t("err-status-contains-newlines"),
                StatusError::InvalidCharacters => t("err-status-invalid-characters"),
            };
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }

        Some(msg)
    };

    // Send the away message
    let client_msg = ClientMessage::UserAway {
        message: message.clone(),
    };
    match conn.send(client_msg) {
        Ok(message_id) => {
            // Track the request so we can display the status message in the response
            conn.pending_requests
                .track(message_id, ResponseRouting::AwayResult(message));
        }
        Err(e) => {
            return app.add_chat_message(connection_id, ChatMessage::error(e));
        }
    }

    Task::none()
}

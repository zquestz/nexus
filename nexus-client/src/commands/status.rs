//! /status command implementation - set or clear status message

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, StatusError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};

/// Execute the /status command
///
/// Sets or clears a status message without changing away status.
///
/// Usage:
/// - /status <message> - Set status message
/// - /status - Clear status message
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

    // Determine status message (None to clear, Some to set)
    let status = if args.is_empty() {
        None
    } else {
        let message = args.join(" ");

        // Validate the status message
        if let Err(e) = validators::validate_status(&message) {
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

        Some(message)
    };

    // Send the status message
    let msg = ClientMessage::UserStatus {
        status: status.clone(),
    };
    match conn.send(msg) {
        Ok(message_id) => {
            // Track the request so we can display the status message in the response
            conn.pending_requests
                .track(message_id, ResponseRouting::StatusResult(status));
        }
        Err(e) => {
            return app.add_chat_message(connection_id, ChatMessage::error(e));
        }
    }

    Task::none()
}

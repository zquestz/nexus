//! /back command implementation - clear away status

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};

/// Execute the /back command
///
/// Clears the user's away status and status message.
///
/// Usage: /back
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /back takes no arguments
    if !args.is_empty() {
        let error_msg = t_args("cmd-back-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    // Get the connection
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    // Send the back message
    let msg = ClientMessage::UserBack;
    match conn.send(msg) {
        Ok(message_id) => {
            conn.pending_requests
                .track(message_id, ResponseRouting::BackResult);
        }
        Err(e) => {
            return app.add_chat_message(connection_id, ChatMessage::error(e));
        }
    }

    Task::none()
}

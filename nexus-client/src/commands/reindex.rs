//! /reindex command implementation - trigger file index rebuild

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};

/// Execute the /reindex command
///
/// Triggers a file index rebuild on the server.
/// Requires `file_reindex` permission.
/// Usage: /reindex
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /reindex takes no arguments
    if !args.is_empty() {
        let error_msg = t_args("cmd-reindex-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let msg = ClientMessage::FileReindex;

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

//! /ping command implementation - measure latency to server

use std::time::Instant;

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};

/// Execute the /ping command
///
/// Sends a ping to the server and measures the round-trip time.
///
/// Usage:
/// - /ping - Measure latency to server
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    _invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // No arguments expected
    if !args.is_empty() {
        return app.add_active_tab_message(connection_id, ChatMessage::error(t("cmd-ping-usage")));
    }

    // Get the connection
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    // Send the ping
    let msg = ClientMessage::Ping;
    match conn.send(msg) {
        Ok(message_id) => {
            // Track the request with send time for latency calculation
            conn.pending_requests
                .track(message_id, ResponseRouting::PingResult(Instant::now()));
        }
        Err(e) => {
            return app.add_active_tab_message(connection_id, ChatMessage::error(e));
        }
    }

    Task::none()
}

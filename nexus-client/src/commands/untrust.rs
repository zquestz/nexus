//! /untrust command implementation - remove trusted IPs

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};

/// Execute the /untrust command
///
/// Removes a trusted IP by IP address, CIDR range, or nickname annotation.
///
/// Usage: /untrust <target>
///
/// Examples:
///   /untrust alice              - Untrust by nickname annotation
///   /untrust 192.168.1.100      - Untrust single IP
///   /untrust 192.168.1.0/24     - Untrust CIDR range (also removes contained IPs)
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /untrust takes exactly 1 argument (target)
    if args.len() != 1 {
        let error_msg = t_args("cmd-untrust-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let target = &args[0];

    let msg = ClientMessage::TrustDelete {
        target: target.clone(),
    };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

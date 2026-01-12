//! /ban command implementation - ban users by IP, CIDR range, or nickname

use iced::Task;
use nexus_common::protocol::ClientMessage;

use super::duration::is_duration_format;
use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};

/// Execute the /ban command
///
/// Bans a user by IP address, CIDR range, or online nickname.
///
/// Usage: /ban <target> [duration] [reason]
///
/// Examples:
///   /ban Spammer                     - permanent ban, no reason
///   /ban Spammer 1h                  - 1 hour ban, no reason
///   /ban Spammer 0 flooding chat     - permanent ban with reason
///   /ban Spammer 1h flooding chat    - 1 hour ban with reason
///   /ban 192.168.1.100               - ban single IP
///   /ban 192.168.1.0/24 7d           - ban CIDR range for 7 days
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /ban requires at least 1 argument (target)
    if args.is_empty() {
        let error_msg = t_args("cmd-ban-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let target = args[0].clone();

    // Parse optional duration and reason
    // Duration format: "10m", "4h", "7d", "0" (permanent)
    // If first remaining arg looks like a duration, use it; rest is reason
    let (duration, reason) = if args.len() > 1 {
        let potential_duration = &args[1];
        if is_duration_format(potential_duration) {
            // "0" means permanent with reason following
            let dur = if potential_duration == "0" {
                None
            } else {
                Some(potential_duration.clone())
            };
            let reason = if args.len() > 2 {
                Some(args[2..].join(" "))
            } else {
                None
            };
            (dur, reason)
        } else {
            // No duration, rest is reason
            (None, Some(args[1..].join(" ")))
        }
    } else {
        (None, None)
    };

    let msg = ClientMessage::BanCreate {
        target,
        duration,
        reason,
    };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

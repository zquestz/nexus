//! /trust command implementation - trust users by IP, CIDR range, or nickname

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, TargetError, TrustReasonError};

use super::duration::is_duration_format;
use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

/// Execute the /trust command
///
/// Trusts a user by IP address, CIDR range, or online nickname.
/// Trusted IPs bypass the ban list.
///
/// Usage: /trust <target> [duration] [reason]
///
/// Examples:
///   /trust alice                     - permanent trust, no reason
///   /trust alice 30d                 - 30 day trust, no reason
///   /trust alice 0 office network    - permanent trust with reason
///   /trust alice 30d contractor      - 30 day trust with reason
///   /trust 192.168.1.100             - trust single IP
///   /trust 192.168.1.0/24            - trust CIDR range permanently
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /trust requires at least 1 argument (target)
    if args.is_empty() {
        let error_msg = t_args("cmd-trust-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let target = args[0].clone();

    // Validate target length
    if let Err(e) = validators::validate_target(&target) {
        let error_msg = match e {
            TargetError::Empty => t("err-target-empty"),
            TargetError::TooLong => t_args(
                "err-target-too-long",
                &[("max", &validators::MAX_TARGET_LENGTH.to_string())],
            ),
        };
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    // Parse optional duration and reason
    // Duration format: "10m", "4h", "7d", "0" (permanent)
    // If first remaining arg looks like a duration, use it; rest is reason
    let (duration, reason) = if args.len() > 1 {
        let potential_duration = &args[1];
        if is_duration_format(potential_duration) {
            // Validate duration length
            if validators::validate_duration(potential_duration).is_err() {
                let error_msg = t_args(
                    "err-duration-too-long",
                    &[("max", &validators::MAX_DURATION_LENGTH.to_string())],
                );
                return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
            }
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

    if let Some(ref r) = reason
        && let Err(e) = validators::validate_trust_reason(r)
    {
        let error_msg = match e {
            TrustReasonError::TooLong => t_args(
                "err-trust-reason-too-long",
                &[("max", &validators::MAX_TRUST_REASON_LENGTH.to_string())],
            ),
            TrustReasonError::InvalidCharacters => t("err-trust-reason-invalid-characters"),
        };
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    let msg = ClientMessage::TrustCreate {
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

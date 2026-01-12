//! /secret command implementation - view or set channel secret mode

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message, PendingRequests, ResponseRouting};
use crate::views::constants::PERMISSION_CHAT_SECRET;

/// Execute the /secret command
///
/// View or set secret mode on the current channel.
/// Secret channels are hidden from /channels list for non-members.
///
/// Usage:
///   /secret      - Show current secret mode state
///   /secret on   - Enable secret mode
///   /secret off  - Disable secret mode
///
/// Viewing requires no permission. Changing requires chat_secret permission.
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /secret only works on channel tabs, not console or PM
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let channel = match &conn.active_chat_tab {
        ChatTab::Channel(ch) => ch.clone(),
        ChatTab::Console | ChatTab::UserMessage(_) => {
            return app.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-secret-no-channel")),
            );
        }
    };

    // Get current secret state
    let channel_lower = channel.to_lowercase();
    let current_secret = conn
        .channels
        .get(&channel_lower)
        .map(|ch| ch.secret)
        .unwrap_or(false);

    // If no args, show current state
    if args.is_empty() {
        let message = if current_secret {
            t("msg-secret-status-on")
        } else {
            t("msg-secret-status-off")
        };
        return app.add_active_tab_message(connection_id, ChatMessage::info(message));
    }

    // Parse on/off argument
    let on_keyword = t("cmd-secret-arg-on").to_lowercase();
    let off_keyword = t("cmd-secret-arg-off").to_lowercase();
    let arg = args[0].to_lowercase();

    let new_secret = if arg == on_keyword {
        true
    } else if arg == off_keyword {
        false
    } else {
        // Invalid argument - show usage
        let error_msg = t_args("cmd-secret-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    };

    // Check chat_secret permission (only needed for changing, not viewing)
    if !conn.is_admin && !conn.permissions.iter().any(|p| p == PERMISSION_CHAT_SECRET) {
        return app.add_active_tab_message(
            connection_id,
            ChatMessage::error(t("err-secret-permission-denied")),
        );
    }

    // Don't send if no change
    if new_secret == current_secret {
        let message = if current_secret {
            t("msg-secret-already-on")
        } else {
            t("msg-secret-already-off")
        };
        return app.add_active_tab_message(connection_id, ChatMessage::info(message));
    }

    // Send ChatSecret message to server
    let msg = ClientMessage::ChatSecret {
        channel: channel.clone(),
        secret: new_secret,
    };

    match conn.send(msg) {
        Ok(message_id) => {
            // Track the pending request for response routing
            // Need to get mutable access again after send
            if let Some(conn) = app.connections.get_mut(&connection_id) {
                conn.pending_requests.track(
                    message_id,
                    ResponseRouting::SecretResult {
                        channel,
                        secret: new_secret,
                    },
                );
            }
        }
        Err(e) => {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
        }
    }

    Task::none()
}

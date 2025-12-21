//! /list command implementation - display connected users

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, PendingRequests, ResponseRouting};
use crate::views::constants::{PERMISSION_USER_DELETE, PERMISSION_USER_EDIT};
use iced::Task;
use nexus_common::protocol::ClientMessage;

/// Execute the /list command
///
/// Displays the currently connected users from the cached user list.
/// Usage: /list [all]
///
/// The `all` argument requires user_edit OR user_delete permission and
/// sends a request to the server to get all users (including offline).
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // Check for optional "all" argument (translated)
    let all_keyword = t("cmd-list-arg-all");
    let request_all = if args.is_empty() {
        false
    } else if args.len() == 1 && args[0].to_lowercase() == all_keyword.to_lowercase() {
        true
    } else {
        let error_msg = t_args("cmd-list-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    };

    // If requesting all users, check permissions and send server request
    if request_all {
        let Some(conn) = app.connections.get(&connection_id) else {
            return Task::none();
        };

        // Check if user has user_edit OR user_delete permission
        let has_edit = conn.has_permission(PERMISSION_USER_EDIT);
        let has_delete = conn.has_permission(PERMISSION_USER_DELETE);

        if !has_edit && !has_delete {
            let error_msg = t("cmd-list-all-no-permission");
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }

        // Send request to server for all users
        let msg = ClientMessage::UserList { all: true };
        let message_id = match conn.send(msg) {
            Ok(id) => id,
            Err(e) => {
                let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
                return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
            }
        };

        // Track this request so the response handler knows to display results in chat
        let conn = app
            .connections
            .get_mut(&connection_id)
            .expect("connection exists");
        conn.pending_requests
            .track(message_id, ResponseRouting::DisplayListInChat);

        return Task::none();
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    // Default behavior: show cached online users
    if conn.online_users.is_empty() {
        return app.add_chat_message(connection_id, ChatMessage::info(t("cmd-list-empty")));
    }

    // Build IRC-style user list: @admin user1 user2
    // nickname is always the display name (== username for regular accounts)
    let user_count = conn.online_users.len();
    let user_list: String = conn
        .online_users
        .iter()
        .map(|user| {
            if user.is_admin {
                format!("@{}", user.nickname)
            } else {
                user.nickname.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Format: "Users online: @alice bob charlie (3 users)"
    let message = t_args(
        "cmd-list-output",
        &[("users", &user_list), ("count", &user_count.to_string())],
    );

    app.add_chat_message(connection_id, ChatMessage::info(message))
}

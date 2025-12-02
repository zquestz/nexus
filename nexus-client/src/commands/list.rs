//! /list command implementation - display connected users

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Execute the /list command
///
/// Displays the currently connected users from the cached user list.
/// Usage: /list
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /list takes no arguments
    if !args.is_empty() {
        let error_msg = t_args("cmd-list-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    if conn.online_users.is_empty() {
        return app.add_chat_message(connection_id, ChatMessage::info(t("cmd-list-empty")));
    }

    // Build IRC-style user list: @admin user1 user2
    let user_count = conn.online_users.len();
    let user_list: String = conn
        .online_users
        .iter()
        .map(|user| {
            if user.is_admin {
                format!("@{}", user.username)
            } else {
                user.username.clone()
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
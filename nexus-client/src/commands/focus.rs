//! /focus command implementation - switch focus to a chat tab

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message};
use iced::Task;

/// Execute the /focus command
///
/// Switches focus to a chat tab.
/// Usage:
/// - `/focus` or `/f` - Switch to server chat
/// - `/focus <nickname>` or `/f <nickname>` - Switch to (or open) a user's PM tab
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /focus takes 0 or 1 argument
    if args.len() > 1 {
        let error_msg = t_args("cmd-focus-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    // No args = focus server tab
    if args.is_empty() {
        return Task::done(Message::SwitchChatTab(ChatTab::Server));
    }

    let target = &args[0];
    let target_lower = target.to_lowercase();

    // Check if target matches a PM tab (case-insensitive)
    let matching_user = conn
        .user_messages
        .keys()
        .find(|nickname| nickname.to_lowercase() == target_lower)
        .cloned();

    if let Some(nickname) = matching_user {
        // Tab exists, switch to it
        return Task::done(Message::SwitchChatTab(ChatTab::UserMessage(nickname)));
    }

    // Check if target matches an online user (case-insensitive)
    // nickname is always the display name (== username for regular accounts)
    let online_user = conn
        .online_users
        .iter()
        .find(|user| user.nickname.to_lowercase() == target_lower)
        .map(|user| user.nickname.clone());

    if let Some(nickname) = online_user {
        // User is online, open/switch to their PM tab
        return Task::done(Message::UserMessageIconClicked(nickname));
    }

    // User not found
    let error_msg = t_args("cmd-focus-not-found", &[("name", target.as_str())]);
    app.add_chat_message(connection_id, ChatMessage::error(error_msg))
}

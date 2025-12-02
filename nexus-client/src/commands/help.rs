//! /help command implementation

use crate::NexusApp;
use crate::commands::command_list_for_user;
use crate::i18n::t;
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Execute the /help command
///
/// Displays a list of available commands (filtered by user permissions).
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    _invoked_name: &str,
    _args: &[String],
) -> Task<Message> {
    // Get user's permissions for filtering
    let (is_admin, permissions) = app
        .connections
        .get(&connection_id)
        .map(|conn| (conn.is_admin, conn.permissions.clone()))
        .unwrap_or((false, Vec::new()));

    let mut tasks = Vec::new();

    // Header
    tasks.push(app.add_chat_message(connection_id, ChatMessage::info(t("cmd-help-header"))));

    // List available commands (filtered by permission)
    for cmd in command_list_for_user(is_admin, &permissions) {
        let aliases = if cmd.aliases.is_empty() {
            String::new()
        } else {
            format!(" ({})", cmd.aliases.join(", "))
        };

        let description = t(cmd.description_key);
        let line = format!("  /{}{} - {}", cmd.name, aliases, description);
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));
    }

    // Footer with escape hint
    tasks.push(app.add_chat_message(connection_id, ChatMessage::info(t("cmd-help-escape-hint"))));

    Task::batch(tasks)
}
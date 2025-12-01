//! /help command implementation

use crate::NexusApp;
use crate::commands::command_list;
use crate::i18n::t;
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Execute the /help command
///
/// Displays a list of all available commands with their descriptions.
/// Optionally accepts a command name as an argument to show detailed help.
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    _invoked_name: &str,
    _args: &[String],
) -> Task<Message> {
    let mut tasks = Vec::new();

    // Header
    tasks.push(app.add_chat_message(connection_id, ChatMessage::info(t("cmd-help-header"))));

    // List all available commands
    for cmd in command_list() {
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

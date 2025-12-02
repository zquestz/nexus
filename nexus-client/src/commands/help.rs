//! /help command implementation

use crate::NexusApp;
use crate::commands::{command_list_for_user, get_command_info};
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Execute the /help command
///
/// Usage:
/// - `/help` - Show all available commands
/// - `/help <command>` - Show usage for a specific command
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // Get user's permissions for filtering
    let (is_admin, permissions) = app
        .connections
        .get(&connection_id)
        .map(|conn| (conn.is_admin, conn.permissions.clone()))
        .unwrap_or((false, Vec::new()));

    // If a command name is provided, show help for that specific command
    if args.len() == 1 {
        return show_command_help(app, connection_id, &args[0], is_admin, &permissions);
    }

    // Too many arguments - show usage
    if args.len() > 1 {
        let error_msg = t_args("cmd-help-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    // Show all commands
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

/// Show help for a specific command
fn show_command_help(
    app: &mut NexusApp,
    connection_id: usize,
    command_name: &str,
    is_admin: bool,
    permissions: &[String],
) -> Task<Message> {
    // Look up the command
    let Some(cmd) = get_command_info(command_name) else {
        let error_msg = t_args("cmd-unknown", &[("command", command_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    };

    // Check if user has permission to use this command
    let has_permission = cmd.permissions.is_empty()
        || is_admin
        || cmd
            .permissions
            .iter()
            .any(|req| permissions.iter().any(|p| p == *req));

    if !has_permission {
        let error_msg = t_args("cmd-unknown", &[("command", command_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let mut tasks = Vec::new();

    // Show description
    let description = t(cmd.description_key);
    tasks.push(app.add_chat_message(
        connection_id,
        ChatMessage::info(format!("/{} - {}", cmd.name, description)),
    ));

    // Show aliases if any
    if !cmd.aliases.is_empty() {
        let aliases_line = format!("  Aliases: {}", cmd.aliases.join(", "));
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(aliases_line)));
    }

    // Show usage
    let usage = t_args(cmd.usage_key, &[("command", cmd.name)]);
    tasks.push(app.add_chat_message(connection_id, ChatMessage::info(format!("  {}", usage))));

    Task::batch(tasks)
}
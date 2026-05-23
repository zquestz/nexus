//! /clear command implementation - clear chat history for current tab

use iced::Task;
use nexus_common::names::fold_name;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, ChatTab, Message};

/// Execute the /clear command
///
/// Clears the chat history for the currently active tab.
/// Usage: /clear
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /clear takes no arguments
    if !args.is_empty() {
        let error_msg = t_args("cmd-clear-usage", &[("command", invoked_name)]);
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    // Get active tab for clearing
    let active_tab = {
        let Some(conn) = app.connections.get(&connection_id) else {
            return Task::none();
        };
        conn.active_chat_tab.clone()
    };

    // Clear in-memory messages
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    match &active_tab {
        ChatTab::Console => {
            conn.console_messages.clear();
        }
        ChatTab::Channel(channel) => {
            let channel_lower = fold_name(channel);
            if let Some(channel_state) = conn.channels.get_mut(&channel_lower) {
                channel_state.messages.clear();
            }
        }
        ChatTab::UserMessage(nickname) => {
            if let Some(messages) = conn.user_messages.get_mut(nickname) {
                messages.clear();
            }
        }
    }

    // Clear history file for user message tabs (keyed by nickname)
    // Silently ignore failures - history is non-critical
    if let ChatTab::UserMessage(nickname) = &active_tab
        && let Some(base_dir) = app.connection_history_keys.get(&connection_id)
        && let Some(history_manager) = app.history_managers.get_mut(base_dir)
    {
        let _ = history_manager.clear_conversation(nickname);
    }

    Task::none()
}

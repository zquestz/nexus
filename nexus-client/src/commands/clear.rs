//! /clear command implementation - clear chat history for current tab

use iced::Task;

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
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    match &conn.active_chat_tab {
        ChatTab::Server => {
            conn.chat_messages.clear();
        }
        ChatTab::UserMessage(nickname) => {
            if let Some(messages) = conn.user_messages.get_mut(nickname) {
                messages.clear();
            }
        }
    }

    Task::none()
}

//! /clear command implementation - clear chat history for current tab

use crate::NexusApp;
use crate::types::{ChatTab, Message};
use iced::Task;

/// Execute the /clear command
///
/// Clears the chat history for the currently active tab.
/// Usage: /clear
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    _invoked_name: &str,
    _args: &[String],
) -> Task<Message> {
    let Some(conn) = app.connections.get_mut(&connection_id) else {
        return Task::none();
    };

    match &conn.active_chat_tab {
        ChatTab::Server => {
            conn.chat_messages.clear();
        }
        ChatTab::UserMessage(username) => {
            if let Some(messages) = conn.user_messages.get_mut(username) {
                messages.clear();
            }
        }
    }

    Task::none()
}
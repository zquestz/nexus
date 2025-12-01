//! /topic command implementation - view and manage chat topic

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;
use nexus_common::protocol::ClientMessage;

/// Execute the /topic command
///
/// Subcommands:
/// - `/topic` - Show current topic
/// - `/topic set <topic>` - Set the topic
/// - `/topic clear` - Clear the topic
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    if args.is_empty() {
        // /topic - show current topic
        return show_topic(app, connection_id);
    }

    match args[0].to_lowercase().as_str() {
        "set" => {
            if args.len() < 2 {
                let error_msg = t_args("cmd-topic-set-usage", &[("command", invoked_name)]);
                return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
            }
            let topic = args[1..].join(" ");
            set_topic(app, connection_id, topic)
        }
        "clear" => set_topic(app, connection_id, String::new()),
        _ => {
            // Unknown subcommand - show usage
            let error_msg = t_args("cmd-topic-usage", &[("command", invoked_name)]);
            app.add_chat_message(connection_id, ChatMessage::error(error_msg))
        }
    }
}

/// Show the current topic
fn show_topic(app: &mut NexusApp, connection_id: usize) -> Task<Message> {
    if let Some(conn) = app.connections.get(&connection_id) {
        let topic = conn.chat_topic.as_deref().unwrap_or("");
        let message = if topic.is_empty() {
            ChatMessage::info(t("cmd-topic-none"))
        } else {
            ChatMessage::info(t_args("msg-topic-display", &[("topic", topic)]))
        };
        return app.add_chat_message(connection_id, message);
    }
    Task::none()
}

/// Set or clear the topic
fn set_topic(app: &mut NexusApp, connection_id: usize, topic: String) -> Task<Message> {
    if let Some(conn) = app.connections.get(&connection_id) {
        let msg = ClientMessage::ChatTopicUpdate { topic };

        if let Err(e) = conn.tx.send(msg) {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
    }
    Task::none()
}

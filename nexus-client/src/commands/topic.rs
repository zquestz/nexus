//! /topic command implementation - view and manage chat topic

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use crate::views::constants::PERMISSION_CHAT_TOPIC_EDIT;
use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, ChatTopicError};

/// Execute the /topic command
///
/// Subcommands:
/// - `/topic` - Show current topic (requires chat_topic permission)
/// - `/topic set <topic>` - Set the topic (requires chat_topic_edit permission)
/// - `/topic clear` - Clear the topic (requires chat_topic_edit permission)
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

    // Get translated subcommand keywords
    let set_keyword = t("cmd-topic-arg-set").to_lowercase();
    let clear_keyword = t("cmd-topic-arg-clear").to_lowercase();
    let arg = args[0].to_lowercase();

    if arg == set_keyword {
        // Check chat_topic_edit permission
        if !has_topic_edit_permission(app, connection_id) {
            return app.add_chat_message(
                connection_id,
                ChatMessage::error(t("cmd-topic-permission-denied")),
            );
        }

        if args.len() < 2 {
            let error_msg = t_args("cmd-topic-set-usage", &[("command", invoked_name)]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
        let topic = args[1..].join(" ");

        // Validate topic content
        if let Err(e) = validators::validate_chat_topic(&topic) {
            let error_msg = match e {
                ChatTopicError::TooLong => t_args(
                    "err-topic-too-long",
                    &[
                        ("length", &topic.len().to_string()),
                        ("max", &validators::MAX_CHAT_TOPIC_LENGTH.to_string()),
                    ],
                ),
                ChatTopicError::ContainsNewlines => t("err-message-contains-newlines"),
                ChatTopicError::InvalidCharacters => t("err-message-invalid-characters"),
            };
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }

        set_topic(app, connection_id, topic)
    } else if arg == clear_keyword {
        // /topic clear takes no additional arguments
        if args.len() > 1 {
            let error_msg = t_args("cmd-topic-usage", &[("command", invoked_name)]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }

        // Check chat_topic_edit permission
        if !has_topic_edit_permission(app, connection_id) {
            return app.add_chat_message(
                connection_id,
                ChatMessage::error(t("cmd-topic-permission-denied")),
            );
        }

        set_topic(app, connection_id, String::new())
    } else {
        // Unknown subcommand - show usage
        let error_msg = t_args("cmd-topic-usage", &[("command", invoked_name)]);
        app.add_chat_message(connection_id, ChatMessage::error(error_msg))
    }
}

/// Check if user has chat_topic_edit permission
fn has_topic_edit_permission(app: &NexusApp, connection_id: usize) -> bool {
    app.connections.get(&connection_id).is_some_and(|conn| {
        conn.is_admin
            || conn
                .permissions
                .iter()
                .any(|p| p == PERMISSION_CHAT_TOPIC_EDIT)
    })
}

/// Show the current topic
fn show_topic(app: &mut NexusApp, connection_id: usize) -> Task<Message> {
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let topic = conn.chat_topic.as_deref().unwrap_or("");
    let set_by = conn.chat_topic_set_by.as_deref().unwrap_or("");

    let message = if topic.is_empty() {
        ChatMessage::info(t("cmd-topic-none"))
    } else if !set_by.is_empty() {
        ChatMessage::info(t_args(
            "msg-topic-set",
            &[("username", set_by), ("topic", topic)],
        ))
    } else {
        ChatMessage::info(t_args("msg-topic-display", &[("topic", topic)]))
    };

    app.add_chat_message(connection_id, message)
}

/// Set or clear the topic
fn set_topic(app: &mut NexusApp, connection_id: usize, topic: String) -> Task<Message> {
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let msg = ClientMessage::ChatTopicUpdate { topic };

    if let Err(e) = conn.send(msg) {
        let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    Task::none()
}

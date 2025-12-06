//! /serverinfo command implementation - display server information

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Execute the /serverinfo command
///
/// Displays server information received during login.
/// Usage: /serverinfo
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    // /serverinfo takes no arguments
    if !args.is_empty() {
        let error_msg = t_args("cmd-serverinfo-usage", &[("command", invoked_name)]);
        return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
    }

    // Extract data from connection first to avoid borrow issues
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let server_name = conn.server_name.clone();
    let server_description = conn.server_description.clone();
    let server_version = conn.server_version.clone();
    let chat_topic = conn.chat_topic.clone();
    let chat_topic_set_by = conn.chat_topic_set_by.clone();
    let max_connections_per_ip = conn.max_connections_per_ip;

    // Now build all the messages
    let mut tasks = Vec::new();

    // Header
    tasks.push(app.add_chat_message(connection_id, ChatMessage::info(t("cmd-serverinfo-header"))));

    // Server name
    if let Some(name) = server_name {
        let line = t_args("cmd-serverinfo-name", &[("name", &name)]);
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));
    }

    // Server description (only if non-empty)
    if let Some(description) = server_description
        && !description.is_empty()
    {
        let line = t_args(
            "cmd-serverinfo-description",
            &[("description", &description)],
        );
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));
    }

    // Server version
    if let Some(version) = server_version {
        let line = t_args("cmd-serverinfo-version", &[("version", &version)]);
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));
    }

    // Chat topic (only if set)
    if let Some(topic) = chat_topic
        && !topic.is_empty()
    {
        let line = t_args("cmd-serverinfo-topic", &[("topic", &topic)]);
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));

        // Topic set by
        if let Some(set_by) = chat_topic_set_by
            && !set_by.is_empty()
        {
            let line = t_args("cmd-serverinfo-topic-set-by", &[("username", &set_by)]);
            tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));
        }
    }

    // Max connections per IP (admin only)
    if let Some(max_conn) = max_connections_per_ip {
        let line = t_args(
            "cmd-serverinfo-max-connections",
            &[("count", &max_conn.to_string())],
        );
        tasks.push(app.add_chat_message(connection_id, ChatMessage::info(line)));
    }

    Task::batch(tasks)
}

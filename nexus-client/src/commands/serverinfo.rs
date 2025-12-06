//! /serverinfo command implementation - display server information

use chrono::Local;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Indentation for server info display lines (matching user info style)
const INFO_INDENT: &str = "  ";

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

    // Build multi-line output similar to user info
    let mut lines = Vec::new();

    // Header - [server]
    lines.push(t("cmd-serverinfo-header"));

    // Server name
    if let Some(name) = server_name {
        let label = t("label-server-name").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {name}"));
    }

    // Server description (only if non-empty)
    if let Some(description) = server_description
        && !description.is_empty()
    {
        let label = t("label-server-description").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {description}"));
    }

    // Server version
    if let Some(version) = server_version {
        let label = t("label-server-version").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {version}"));
    }

    // Chat topic (only if set)
    if let Some(topic) = chat_topic
        && !topic.is_empty()
    {
        let label = t("label-chat-topic").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {topic}"));

        // Topic set by
        if let Some(set_by) = chat_topic_set_by
            && !set_by.is_empty()
        {
            let label = t("label-chat-topic-set-by").to_lowercase();
            lines.push(format!("{INFO_INDENT}{label} {set_by}"));
        }
    }

    // Max connections per IP (admin only)
    if let Some(max_conn) = max_connections_per_ip {
        let label = t("label-max-connections-per-ip").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {max_conn}"));
    }

    // End line
    lines.push(format!("{INFO_INDENT}{}", t("cmd-serverinfo-end")));

    // Add each line as a separate chat message with shared timestamp
    let timestamp = Local::now();
    let mut task = Task::none();
    for line in lines {
        task = app.add_chat_message(
            connection_id,
            ChatMessage::info_with_timestamp(line, timestamp),
        );
    }
    // Last add_chat_message will handle auto-scroll
    task
}

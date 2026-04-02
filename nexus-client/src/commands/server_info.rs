//! /serverinfo command implementation - display server information

use chrono::Local;
use iced::Task;

use crate::NexusApp;
use crate::i18n::{log_level_translation_key, strength_translation_key, t, t_args};
use crate::types::{ChatMessage, Message};

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
        return app.add_active_tab_message(connection_id, ChatMessage::error(error_msg));
    }

    // Extract data from connection first to avoid borrow issues
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let server_name = conn.server_name.clone();
    let server_description = conn.server_description.clone();
    let server_version = conn.server_version.clone();
    let max_connections_per_ip = conn.max_connections_per_ip;
    let max_transfers_per_ip = conn.max_transfers_per_ip;
    let min_password_strength = conn.min_password_strength;
    let log_level = conn.log_level.clone();

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

    // General fields in alphabetical order, gated by availability

    // Log level
    if let Some(level) = log_level {
        let label = t("label-log-level").to_lowercase();
        let value = t(log_level_translation_key(&level)).to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {value}"));
    }

    // Max connections per IP
    if let Some(max_conn) = max_connections_per_ip {
        let label = t("label-max-connections-per-ip").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {max_conn}"));
    }

    // Max transfers per IP
    if let Some(max_xfer) = max_transfers_per_ip {
        let label = t("label-max-transfers-per-ip").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {max_xfer}"));
    }

    // Min password strength
    {
        let label = t("label-min-password-strength").to_lowercase();
        let value = t(strength_translation_key(min_password_strength)).to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {value}"));
    }

    // Version
    if let Some(version) = server_version {
        let label = t("label-server-version").to_lowercase();
        lines.push(format!("{INFO_INDENT}{label} {version}"));
    }

    // End line
    lines.push(format!("{INFO_INDENT}{}", t("cmd-serverinfo-end")));

    // Add each line as a separate chat message with shared timestamp
    let timestamp = Local::now();
    let mut task = Task::none();
    for line in lines {
        task = app.add_active_tab_message(
            connection_id,
            ChatMessage::info_with_timestamp(line, timestamp),
        );
    }
    // Last add_active_tab_message will handle auto-scroll
    task
}

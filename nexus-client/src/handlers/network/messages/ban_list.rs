//! Ban list response handler

use iced::Task;

use super::time_format::{TimeFormatContext, format_remaining_time};
use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle ban list response
    pub fn handle_ban_list_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        bans: Option<Vec<nexus_common::protocol::BanInfo>>,
    ) -> Task<Message> {
        if !success {
            let message = ChatMessage::error(error.unwrap_or_default());
            return self.add_chat_message(connection_id, message);
        }

        let bans = bans.unwrap_or_default();

        if bans.is_empty() {
            let message = ChatMessage::info(t("msg-ban-list-empty"));
            return self.add_chat_message(connection_id, message);
        }

        // Build the ban list output
        let mut tasks = Vec::new();

        // Header
        tasks.push(
            self.add_chat_message(connection_id, ChatMessage::info(t("msg-ban-list-header"))),
        );

        // Each ban entry
        for ban in bans {
            let entry = format_ban_entry(&ban);
            tasks.push(self.add_chat_message(connection_id, ChatMessage::info(entry)));
        }

        Task::batch(tasks)
    }
}

/// Format a single ban entry for display
fn format_ban_entry(ban: &nexus_common::protocol::BanInfo) -> String {
    let mut parts = Vec::new();

    // IP/CIDR with optional nickname annotation
    if let Some(ref nickname) = ban.nickname {
        parts.push(format!("  {} ({})", ban.ip_address, nickname));
    } else {
        parts.push(format!("  {}", ban.ip_address));
    }

    // Created by
    parts.push(format!("- {}", ban.created_by));

    // Duration remaining
    if let Some(expires_at) = ban.expires_at {
        let remaining = format_remaining_time(expires_at, TimeFormatContext::Ban);
        parts.push(t_args("msg-ban-remaining", &[("time", &remaining)]));
    } else {
        parts.push(t("msg-ban-permanent"));
    }

    // Reason (if any)
    if let Some(ref reason) = ban.reason {
        parts.push(format!("- \"{}\"", reason));
    }

    parts.join(" ")
}

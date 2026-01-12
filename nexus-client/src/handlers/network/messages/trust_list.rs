//! Trust list response handler

use iced::Task;

use super::time_format::{TimeFormatContext, format_remaining_time};
use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle trust list response
    pub fn handle_trust_list_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        entries: Option<Vec<nexus_common::protocol::TrustInfo>>,
    ) -> Task<Message> {
        if !success {
            let message = ChatMessage::error(error.unwrap_or_default());
            return self.add_active_tab_message(connection_id, message);
        }

        let entries = entries.unwrap_or_default();

        if entries.is_empty() {
            let message = ChatMessage::info(t("msg-trust-list-empty"));
            return self.add_active_tab_message(connection_id, message);
        }

        // Build the trust list output
        let mut tasks = Vec::new();

        // Header
        tasks.push(
            self.add_active_tab_message(
                connection_id,
                ChatMessage::info(t("msg-trust-list-header")),
            ),
        );

        // Each trust entry
        for entry in entries {
            let formatted = format_trust_entry(&entry);
            tasks.push(self.add_active_tab_message(connection_id, ChatMessage::info(formatted)));
        }

        Task::batch(tasks)
    }
}

/// Format a single trust entry for display
fn format_trust_entry(entry: &nexus_common::protocol::TrustInfo) -> String {
    let mut parts = Vec::new();

    // IP/CIDR with optional nickname annotation
    if let Some(ref nickname) = entry.nickname {
        parts.push(format!("  {} ({})", entry.ip_address, nickname));
    } else {
        parts.push(format!("  {}", entry.ip_address));
    }

    // Created by
    parts.push(format!("- {}", entry.created_by));

    // Duration remaining
    if let Some(expires_at) = entry.expires_at {
        let remaining = format_remaining_time(expires_at, TimeFormatContext::Trust);
        parts.push(t_args("msg-trust-remaining", &[("time", &remaining)]));
    } else {
        parts.push(t("msg-trust-permanent"));
    }

    // Reason (if any)
    if let Some(ref reason) = entry.reason {
        parts.push(format!("- \"{}\"", reason));
    }

    parts.join(" ")
}

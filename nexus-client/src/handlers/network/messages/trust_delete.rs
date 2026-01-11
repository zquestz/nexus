//! Trust delete response handler

use iced::Task;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle trust delete response
    pub fn handle_trust_delete_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        ips: Option<Vec<String>>,
        nickname: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            // Build success message based on what was untrusted
            let ips = ips.unwrap_or_default();
            if ips.len() == 1 {
                // Single IP untrusted
                if let Some(ref nick) = nickname {
                    ChatMessage::info(t_args(
                        "msg-untrusted-ip-nickname",
                        &[("ip", &ips[0]), ("nickname", nick)],
                    ))
                } else {
                    ChatMessage::info(t_args("msg-untrusted-ip", &[("ip", &ips[0])]))
                }
            } else if !ips.is_empty() {
                // Multiple IPs untrusted
                if let Some(ref nick) = nickname {
                    ChatMessage::info(t_args(
                        "msg-untrusted-ips-nickname",
                        &[("count", &ips.len().to_string()), ("nickname", nick)],
                    ))
                } else {
                    ChatMessage::info(t_args(
                        "msg-untrusted-ips",
                        &[("count", &ips.len().to_string())],
                    ))
                }
            } else {
                // No IPs returned, generic success
                ChatMessage::info(t("msg-untrusted-success"))
            }
        } else {
            // Show the server's error message directly
            ChatMessage::error(error.unwrap_or_default())
        };

        self.add_chat_message(connection_id, message)
    }
}

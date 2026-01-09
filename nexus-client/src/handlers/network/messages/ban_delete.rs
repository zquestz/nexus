//! Ban delete response handler

use iced::Task;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle ban delete response
    pub fn handle_ban_delete_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        ips: Option<Vec<String>>,
        nickname: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            // Build success message based on what was unbanned
            let ips = ips.unwrap_or_default();
            if ips.len() == 1 {
                // Single IP unbanned
                if let Some(ref nick) = nickname {
                    ChatMessage::info(t_args(
                        "msg-unbanned-ip-nickname",
                        &[("ip", &ips[0]), ("nickname", nick)],
                    ))
                } else {
                    ChatMessage::info(t_args("msg-unbanned-ip", &[("ip", &ips[0])]))
                }
            } else if !ips.is_empty() {
                // Multiple IPs unbanned
                if let Some(ref nick) = nickname {
                    ChatMessage::info(t_args(
                        "msg-unbanned-ips-nickname",
                        &[("count", &ips.len().to_string()), ("nickname", nick)],
                    ))
                } else {
                    ChatMessage::info(t_args(
                        "msg-unbanned-ips",
                        &[("count", &ips.len().to_string())],
                    ))
                }
            } else {
                // No IPs returned, generic success
                ChatMessage::info(t("msg-unbanned-success"))
            }
        } else {
            // Show the server's error message directly
            ChatMessage::error(error.unwrap_or_default())
        };

        self.add_chat_message(connection_id, message)
    }
}
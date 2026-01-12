//! Trust create response handler

use iced::Task;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle trust create response
    pub fn handle_trust_create_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        ips: Option<Vec<String>>,
        nickname: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            // Show success message with IPs and optional nickname
            match (ips, nickname) {
                (Some(ip_list), Some(nick)) if ip_list.len() == 1 => {
                    // Single IP trusted by nickname
                    ChatMessage::info(t_args(
                        "msg-trusted-ip-nickname",
                        &[("ip", &ip_list[0]), ("nickname", &nick)],
                    ))
                }
                (Some(ip_list), Some(nick)) if ip_list.len() > 1 => {
                    // Multiple IPs trusted by nickname
                    ChatMessage::info(t_args(
                        "msg-trusted-ips-nickname",
                        &[("count", &ip_list.len().to_string()), ("nickname", &nick)],
                    ))
                }
                (Some(ip_list), None) if ip_list.len() == 1 => {
                    // Single IP trusted directly
                    ChatMessage::info(t_args("msg-trusted-ip", &[("ip", &ip_list[0])]))
                }
                (Some(ip_list), None) if ip_list.len() > 1 => {
                    // Multiple IPs trusted (CIDR range)
                    ChatMessage::info(t_args(
                        "msg-trusted-ips",
                        &[("count", &ip_list.len().to_string())],
                    ))
                }
                _ => {
                    // Fallback: generic success
                    ChatMessage::info(t("msg-trust-created"))
                }
            }
        } else {
            // Show the server's error message directly
            ChatMessage::error(error.unwrap_or_default())
        };
        self.add_active_tab_message(connection_id, message)
    }
}

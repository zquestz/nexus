//! Ban delete response handler

use iced::Task;

use super::ip_actions::{BAN_DELETE_KEYS, ip_action_message};
use crate::NexusApp;
use crate::types::Message;

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
        let message = ip_action_message(&BAN_DELETE_KEYS, success, error, ips, nickname);
        self.add_active_tab_message(connection_id, message)
    }
}

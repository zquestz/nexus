//! Trust delete response handler

use iced::Task;

use super::ip_actions::{TRUST_DELETE_KEYS, ip_action_message};
use crate::NexusApp;
use crate::types::Message;

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
        let message = ip_action_message(&TRUST_DELETE_KEYS, success, error, ips, nickname);
        self.add_active_tab_message(connection_id, message)
    }
}

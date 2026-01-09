//! Away/back/status response handlers

use iced::Task;
use nexus_common::framing::MessageId;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, ResponseRouting};

impl NexusApp {
    /// Handle response to UserAway request
    pub fn handle_user_away_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Get the tracked request to retrieve the status message we sent
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        if success {
            // Check if we had a status message from the tracked request
            let msg = match routing {
                Some(ResponseRouting::AwayResult(Some(status))) => {
                    t_args("msg-now-away-status", &[("status", &status)])
                }
                _ => t("msg-now-away"),
            };
            self.add_chat_message(connection_id, ChatMessage::info(msg))
        } else {
            let error_msg = error.unwrap_or_default();
            self.add_chat_message(connection_id, ChatMessage::error(error_msg))
        }
    }

    /// Handle response to UserBack request
    pub fn handle_user_back_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Remove tracking (we don't need the data, just cleanup)
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.pending_requests.remove(&message_id);
        }

        if success {
            self.add_chat_message(connection_id, ChatMessage::info(t("msg-welcome-back")))
        } else {
            let error_msg = error.unwrap_or_default();
            self.add_chat_message(connection_id, ChatMessage::error(error_msg))
        }
    }

    /// Handle response to UserStatus request
    pub fn handle_user_status_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Get the tracked request to retrieve the status message we sent
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        if success {
            // Check if we had a status message from the tracked request
            let msg = match routing {
                Some(ResponseRouting::StatusResult(Some(status))) => {
                    t_args("msg-status-set", &[("status", &status)])
                }
                _ => t("msg-status-cleared"),
            };
            self.add_chat_message(connection_id, ChatMessage::info(msg))
        } else {
            let error_msg = error.unwrap_or_default();
            self.add_chat_message(connection_id, ChatMessage::error(error_msg))
        }
    }
}

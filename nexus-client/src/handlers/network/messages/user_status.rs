//! Away/back/status response handlers

use iced::Task;
use nexus_common::framing::MessageId;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message, ResponseRouting};

impl NexusApp {
    /// Handle response to UserAway request (manual or auto-away)
    pub fn handle_user_away_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Get the tracked request to retrieve the status message and routing type
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        let is_auto = matches!(routing, Some(ResponseRouting::AutoAwayResult(_)));

        if success {
            // Update connection state
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.is_away = true;
                if is_auto {
                    conn.is_auto_away = true;
                }
            }

            // Chat output is the same for manual and auto-away
            let msg = match &routing {
                Some(ResponseRouting::AwayResult(Some(status)))
                | Some(ResponseRouting::AutoAwayResult(Some(status))) => {
                    t_args("msg-now-away-status", &[("status", status)])
                }
                _ => t("msg-now-away"),
            };
            self.add_active_tab_message(connection_id, ChatMessage::info(msg))
        } else {
            // On error: no state change (auto-away will retry on next tick)
            let error_msg = error.unwrap_or_default();
            self.add_active_tab_message(connection_id, ChatMessage::error(error_msg))
        }
    }

    /// Handle response to UserBack request (manual or auto-back)
    pub fn handle_user_back_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Get the tracked request to determine routing type
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        let is_auto = matches!(routing, Some(ResponseRouting::AutoBackResult));

        if success {
            // Update connection state
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.is_away = false;
                conn.is_auto_away = false;
            }

            self.add_active_tab_message(connection_id, ChatMessage::info(t("msg-welcome-back")))
        } else if is_auto {
            // Auto-back failed: set is_auto_away back to true so next activity retries
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.is_auto_away = true;
            }
            Task::none()
        } else {
            let error_msg = error.unwrap_or_default();
            self.add_active_tab_message(connection_id, ChatMessage::error(error_msg))
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
            self.add_active_tab_message(connection_id, ChatMessage::info(msg))
        } else {
            let error_msg = error.unwrap_or_default();
            self.add_active_tab_message(connection_id, ChatMessage::error(error_msg))
        }
    }
}

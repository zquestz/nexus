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
            // Update connection state. A manual away replaces an auto-away, so
            // participating no longer clears it; only /back does.
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.is_away = true;
                conn.is_auto_away = is_auto;
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
            // Auto-back failed: leave the away state as is. The request is no
            // longer pending, so the next mark_back retries.
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
            // The server clears away when a status is set, so mirror it. A stale
            // is_auto_away would make the next mark_back send a UserBack that
            // wipes the new status.
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.is_away = false;
                conn.is_auto_away = false;
            }

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

#[cfg(test)]
mod tests {
    use nexus_common::protocol::ClientMessage;

    use super::*;
    use crate::testing::support::test_connection_with_receiver;
    use crate::types::PendingRequests;

    #[test]
    fn manual_away_replaces_auto_away() {
        let mut app = NexusApp::default();
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.is_away = true;
        conn.is_auto_away = true;
        let message_id = MessageId::new();
        conn.pending_requests
            .track(message_id, ResponseRouting::AwayResult(None));
        app.connections.insert(1, conn);

        let _ = app.handle_user_away_response(1, message_id, true, None);

        // Participating no longer clears the away; only /back does
        let conn = app.connections.get_mut(&1).expect("connection 1 exists");
        assert!(conn.is_away);
        assert!(!conn.is_auto_away);
        conn.mark_back();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn setting_a_status_clears_auto_away() {
        let mut app = NexusApp::default();
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.is_away = true;
        conn.is_auto_away = true;
        let message_id = MessageId::new();
        conn.pending_requests.track(
            message_id,
            ResponseRouting::StatusResult(Some("In a meeting".to_string())),
        );
        app.connections.insert(1, conn);

        let _ = app.handle_user_status_response(1, message_id, true, None);

        // The server cleared away along with setting the status, so chatting
        // later must not send a UserBack that would wipe it
        let conn = app.connections.get_mut(&1).expect("connection 1 exists");
        assert!(!conn.is_away);
        assert!(!conn.is_auto_away);
        conn.mark_back();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn failed_auto_back_retries_on_the_next_participation() {
        let mut app = NexusApp::default();
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.is_away = true;
        conn.is_auto_away = true;
        conn.mark_back();
        let message_id = match rx.try_recv() {
            Ok((message_id, ClientMessage::UserBack)) => message_id,
            other => panic!("expected UserBack, got {other:?}"),
        };
        app.connections.insert(1, conn);

        let _ = app.handle_user_back_response(1, message_id, false, None);

        let conn = app.connections.get_mut(&1).expect("connection 1 exists");
        assert!(conn.is_auto_away);
        conn.mark_back();
        assert!(matches!(rx.try_recv(), Ok((_, ClientMessage::UserBack))));
    }
}

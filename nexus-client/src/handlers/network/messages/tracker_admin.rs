//! Tracker administration response handlers.
//!
//! Mirrors the User / Group admin response handler shape. Each tracker
//! response is dispatched here from the message router; the handlers
//! consult `pending_requests` to decide whether the response was triggered
//! from the tracker management panel (in which case it updates panel
//! state) or from a stray request (in which case it surfaces in chat).

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::framing::MessageId;
use nexus_common::protocol::{ClientMessage, TrackerInfo};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    ChatMessage, InputId, Message, PendingRequests, ResponseRouting, TrackerEditInit,
    TrackerManagementMode,
};

impl NexusApp {
    /// Handle `TrackerListResponse` — full tracker list refresh.
    pub fn handle_tracker_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        trackers: Vec<TrackerInfo>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let routing = conn.pending_requests.remove(&message_id);

        // Only consume responses tied to our refresh routing — stray
        // responses (e.g. arriving after the panel closed) are ignored.
        if !matches!(
            routing,
            Some(ResponseRouting::PopulateTrackerManagementList)
        ) {
            return Task::none();
        }

        if success {
            conn.tracker_management.all_trackers = Some(Ok(trackers));
            conn.tracker_management.list_error = None;
        } else {
            let msg = error.unwrap_or_default();
            conn.tracker_management.all_trackers = Some(Err(msg));
            // Also clear the panel-level banner: the body's red Err state
            // already conveys the failure, and a stale list_error from a
            // previous TrackerEdit fetch failure would render simultaneously
            // with the body banner — confusing two-banner UX.
            conn.tracker_management.list_error = None;
        }

        Task::none()
    }

    /// Handle `TrackerAddResponse` — Add subview submission result.
    pub fn handle_tracker_add_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Build success toast (with name if available)
            let toast = if let Some(ref n) = name {
                t_args("toast-tracker-added-name", &[("name", n)])
            } else {
                t("toast-tracker-added")
            };

            if matches!(routing, Some(ResponseRouting::TrackerManagementAddResult)) {
                conn.tracker_management.reset_to_list();
                let refresh = self.refresh_tracker_management_list_for(connection_id);
                return Task::batch([Task::done(Message::ShowToast(toast)), refresh]);
            }

            return Task::done(Message::ShowToast(toast));
        }

        // Failure: show in form banner if from the panel; otherwise chat.
        let err_msg = error.unwrap_or_default();
        if matches!(routing, Some(ResponseRouting::TrackerManagementAddResult)) {
            conn.tracker_management.form_error = Some(err_msg);
            conn.tracker_management.is_submitting = false;
            return Task::none();
        }

        self.add_active_tab_message(connection_id, ChatMessage::error(err_msg))
    }

    /// Handle `TrackerEditResponse` — payload for the Edit subview after
    /// the user clicks Edit on a row. On success populate the form; on
    /// error surface in the panel-level banner (the form never opens).
    pub fn handle_tracker_edit_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        tracker: Option<TrackerInfo>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let routing = conn.pending_requests.remove(&message_id);

        if !matches!(
            routing,
            Some(ResponseRouting::PopulateTrackerManagementEdit)
        ) {
            // Stray response — ignore.
            return Task::none();
        }

        if success {
            if let Some(info) = tracker {
                conn.tracker_management.enter_edit_mode(TrackerEditInit {
                    id: info.id,
                    name: info.name,
                    address: info.address,
                    port: info.port,
                    fingerprint: info.fingerprint,
                    password: info.password,
                    enabled: info.enabled,
                    last_error: info.last_error,
                });
                self.focused_field = InputId::EditTrackerName;
                return operation::focus(Id::from(InputId::EditTrackerName));
            } else {
                // Success but no payload — treat as fetch failure so the
                // user sees something in the banner instead of an empty form.
                conn.tracker_management.list_error = Some(t("err-tracker-edit-no-payload"));
            }
        } else {
            conn.tracker_management.list_error = Some(error.unwrap_or_default());
        }

        Task::none()
    }

    /// Handle `TrackerUpdateResponse` — Edit subview submission result.
    pub fn handle_tracker_update_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let routing = conn.pending_requests.remove(&message_id);

        if success {
            let toast = if let Some(ref n) = name {
                t_args("toast-tracker-updated-name", &[("name", n)])
            } else {
                t("toast-tracker-updated")
            };

            if matches!(
                routing,
                Some(ResponseRouting::TrackerManagementUpdateResult)
            ) {
                conn.tracker_management.reset_to_list();
                let refresh = self.refresh_tracker_management_list_for(connection_id);
                return Task::batch([Task::done(Message::ShowToast(toast)), refresh]);
            }

            return Task::done(Message::ShowToast(toast));
        }

        let err_msg = error.unwrap_or_default();
        if matches!(
            routing,
            Some(ResponseRouting::TrackerManagementUpdateResult)
        ) {
            conn.tracker_management.form_error = Some(err_msg);
            conn.tracker_management.is_submitting = false;
            return Task::none();
        }

        self.add_active_tab_message(connection_id, ChatMessage::error(err_msg))
    }

    /// Handle `TrackerRemoveResponse` — Remove confirmation result.
    pub fn handle_tracker_remove_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let routing = conn.pending_requests.remove(&message_id);

        if success {
            let toast = if let Some(ref n) = name {
                t_args("toast-tracker-removed-name", &[("name", n)])
            } else {
                t("toast-tracker-removed")
            };

            if matches!(
                routing,
                Some(ResponseRouting::TrackerManagementRemoveResult)
            ) {
                conn.tracker_management.mode = TrackerManagementMode::List;
                conn.tracker_management.remove_error = None;
                conn.tracker_management.is_remove_submitting = false;
                let refresh = self.refresh_tracker_management_list_for(connection_id);
                return Task::batch([Task::done(Message::ShowToast(toast)), refresh]);
            }

            return Task::done(Message::ShowToast(toast));
        }

        let err_msg = error.unwrap_or_default();
        if matches!(
            routing,
            Some(ResponseRouting::TrackerManagementRemoveResult)
        ) {
            conn.tracker_management.remove_error = Some(err_msg);
            conn.tracker_management.is_remove_submitting = false;
            return Task::none();
        }

        self.add_active_tab_message(connection_id, ChatMessage::error(err_msg))
    }

    /// Handle `TrackerAcceptFingerprintResponse` — promote the
    /// Stage-1-observed fingerprint.
    pub fn handle_tracker_accept_fingerprint_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let routing = conn.pending_requests.remove(&message_id);

        if success {
            let toast = if let Some(ref n) = name {
                t_args("toast-tracker-fingerprint-accepted-name", &[("name", n)])
            } else {
                t("toast-tracker-fingerprint-accepted")
            };

            if matches!(
                routing,
                Some(ResponseRouting::TrackerManagementAcceptFingerprintResult)
            ) {
                conn.tracker_management.mode = TrackerManagementMode::List;
                conn.tracker_management.accept_fingerprint_error = None;
                conn.tracker_management.is_accept_fingerprint_submitting = false;
                let refresh = self.refresh_tracker_management_list_for(connection_id);
                return Task::batch([Task::done(Message::ShowToast(toast)), refresh]);
            }

            return Task::done(Message::ShowToast(toast));
        }

        let err_msg = error.unwrap_or_default();
        if matches!(
            routing,
            Some(ResponseRouting::TrackerManagementAcceptFingerprintResult)
        ) {
            conn.tracker_management.accept_fingerprint_error = Some(err_msg);
            conn.tracker_management.is_accept_fingerprint_submitting = false;
            return Task::none();
        }

        self.add_active_tab_message(connection_id, ChatMessage::error(err_msg))
    }

    // ====================================================================
    // Helpers
    // ====================================================================

    /// Reissue a `TrackerList` request after a mutation succeeded so the
    /// table reflects the new state. Mirrors `refresh_user_management_list_for`.
    fn refresh_tracker_management_list_for(&mut self, connection_id: usize) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Only refresh if the panel is still showing Server Info — avoid
        // burning a request on a panel the user has already closed.
        if conn.active_panel != crate::types::ActivePanel::ServerInfo {
            return Task::none();
        }

        conn.tracker_management.all_trackers = None;

        match conn.send(ClientMessage::TrackerList) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateTrackerManagementList);
            }
            Err(e) => {
                conn.tracker_management.all_trackers =
                    Some(Err(format!("{}: {}", t("err-send-failed"), e)));
            }
        }

        Task::none()
    }
}

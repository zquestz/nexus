//! Tracker management handlers (Trackers tab inside the Server Info panel).
//!
//! Mirrors the user/group management handler shape: panel toggles,
//! sub-view transitions, form input handlers, validate-then-submit on
//! Enter, and is_submitting / is_remove_submitting / is_accept_submitting
//! double-submit guards.

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{
    self, MAX_PASSWORD_LENGTH, MAX_TRACKER_NAME_LENGTH, PublicAddressError, TrackerAddressError,
    TrackerNameError,
};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    InputId, Message, PendingRequests, ResponseRouting, TrackerManagementMode,
    TrackerManagementSortColumn,
};

impl NexusApp {
    // ====================================================================
    // List view actions
    // ====================================================================

    /// Show the Add Tracker subview (toolbar [+] click).
    pub fn handle_tracker_management_show_add(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };
        conn.tracker_management.enter_add_mode();
        self.focused_field = InputId::AddTrackerName;
        operation::focus(Id::from(InputId::AddTrackerName))
    }

    /// Refresh the tracker list (toolbar refresh button).
    pub fn handle_tracker_management_refresh(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.tracker_management.all_trackers = None;
        conn.tracker_management.list_error = None;

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

    /// Handle Edit row click — refetch the tracker via `TrackerEdit`
    /// rather than reusing the cached row. Mirrors User Management.
    pub fn handle_tracker_management_show_edit(&mut self, id: i64) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match conn.send(ClientMessage::TrackerEdit { id }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateTrackerManagementEdit);
            }
            Err(e) => {
                conn.tracker_management.list_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle Remove row click — open the confirmation modal.
    pub fn handle_tracker_management_show_remove(
        &mut self,
        id: i64,
        name: String,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };
        conn.tracker_management.enter_confirm_remove_mode(id, name);
        Task::none()
    }

    /// Handle "Accept Fingerprint" row click — open the accept dialog
    /// pre-populated from the row's `pending_fingerprint`.
    pub fn handle_tracker_management_show_accept_fingerprint(&mut self, id: i64) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Look up the tracker from the cached list. We don't refetch here
        // because the row's pending_fingerprint is already in our cache
        // (it arrived with the most recent TrackerListResponse) and the
        // accept dialog needs that exact value.
        let Some(Ok(trackers)) = conn.tracker_management.all_trackers.as_ref() else {
            return Task::none();
        };
        let Some(tracker) = trackers.iter().find(|t| t.id == id) else {
            return Task::none();
        };
        let Some(received) = tracker.pending_fingerprint.clone() else {
            // The menu item is gated on this being Some, but races happen:
            // another admin (or another session of the same admin) may have
            // accepted, edited, or otherwise cleared the pending observation
            // between when the menu opened and when the click fired. Surface
            // a soft toast so the click doesn't appear to be a no-op.
            return Task::done(Message::ShowToast(t("toast-tracker-fingerprint-stale")));
        };

        let name = tracker.name.clone();
        let address = tracker.address.clone();
        let port = tracker.port;
        let expected = tracker.fingerprint.clone();

        conn.tracker_management
            .enter_accept_fingerprint_mode(id, name, address, port, expected, received);
        Task::none()
    }

    /// Handle Cancel from Add subview, Edit subview, Remove modal, or
    /// Accept Fingerprint dialog — return to list view.
    pub fn handle_cancel_tracker_management(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };
        conn.tracker_management.reset_to_list();
        Task::none()
    }

    /// Sort column header click — toggle direction or switch column.
    pub fn handle_tracker_management_sort_changed(
        &mut self,
        column: TrackerManagementSortColumn,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            if conn.tracker_management.sort_column == column {
                conn.tracker_management.sort_ascending = !conn.tracker_management.sort_ascending;
            } else {
                conn.tracker_management.sort_column = column;
                conn.tracker_management.sort_ascending = true;
            }
        }
        Task::none()
    }

    // ====================================================================
    // Add form
    // ====================================================================

    pub fn handle_add_tracker_name_changed(&mut self, name: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.add_name = name;
        }
        self.focused_field = InputId::AddTrackerName;
        Task::none()
    }

    pub fn handle_add_tracker_address_changed(&mut self, address: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.add_address = address;
        }
        self.focused_field = InputId::AddTrackerAddress;
        Task::none()
    }

    pub fn handle_add_tracker_port_changed(&mut self, port: u16) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.add_port = port;
        }
        Task::none()
    }

    pub fn handle_add_tracker_fingerprint_changed(&mut self, value: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.add_fingerprint = value;
        }
        self.focused_field = InputId::AddTrackerFingerprint;
        Task::none()
    }

    pub fn handle_add_tracker_password_changed(&mut self, value: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.add_password = value;
        }
        self.focused_field = InputId::AddTrackerPassword;
        Task::none()
    }

    pub fn handle_add_tracker_enabled_toggled(&mut self, enabled: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.add_enabled = enabled;
        }
        Task::none()
    }

    /// Add form submit.
    pub fn handle_add_tracker_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.tracker_management.is_submitting {
            return Task::none();
        }

        // Snapshot the form values
        let name = conn.tracker_management.add_name.clone();
        let address = conn.tracker_management.add_address.clone();
        let port = conn.tracker_management.add_port;
        let fingerprint = conn.tracker_management.add_fingerprint.clone();
        let password = conn.tracker_management.add_password.clone();
        let enabled = conn.tracker_management.add_enabled;

        // Validate — same checks as Edit, surfaced through helper.
        if let Err(err_msg) = validate_tracker_form(&name, &address, &fingerprint, &password) {
            conn.tracker_management.form_error = Some(err_msg);
            return Task::none();
        }

        let fingerprint_opt = if fingerprint.trim().is_empty() {
            None
        } else {
            Some(fingerprint.trim().to_string())
        };
        let password_opt = if password.is_empty() {
            None
        } else {
            Some(password)
        };

        let msg = ClientMessage::TrackerAdd {
            address,
            port,
            fingerprint: fingerprint_opt,
            password: password_opt,
            name,
            enabled,
        };

        conn.tracker_management.form_error = None;
        conn.tracker_management.is_submitting = true;

        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::TrackerManagementAddResult);
            }
            Err(e) => {
                conn.tracker_management.is_submitting = false;
                conn.tracker_management.form_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Validate Add form on Enter (placeholder validation for missing fields).
    pub fn handle_validate_add_tracker(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            let name = &conn.tracker_management.add_name;
            let address = &conn.tracker_management.add_address;
            let fingerprint = &conn.tracker_management.add_fingerprint;
            let password = &conn.tracker_management.add_password;
            if let Err(msg) = validate_tracker_form(name, address, fingerprint, password) {
                conn.tracker_management.form_error = Some(msg);
            }
        }
        Task::none()
    }

    // ====================================================================
    // Edit form
    // ====================================================================

    pub fn handle_edit_tracker_name_changed(&mut self, value: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit { name, .. } = &mut conn.tracker_management.mode
        {
            *name = value;
        }
        self.focused_field = InputId::EditTrackerName;
        Task::none()
    }

    pub fn handle_edit_tracker_address_changed(&mut self, value: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit { address, .. } = &mut conn.tracker_management.mode
        {
            *address = value;
        }
        self.focused_field = InputId::EditTrackerAddress;
        Task::none()
    }

    pub fn handle_edit_tracker_port_changed(&mut self, value: u16) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit { port, .. } = &mut conn.tracker_management.mode
        {
            *port = value;
        }
        Task::none()
    }

    pub fn handle_edit_tracker_fingerprint_changed(&mut self, value: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit { fingerprint, .. } =
                &mut conn.tracker_management.mode
        {
            *fingerprint = value;
        }
        self.focused_field = InputId::EditTrackerFingerprint;
        Task::none()
    }

    pub fn handle_edit_tracker_password_changed(&mut self, value: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit { password, .. } = &mut conn.tracker_management.mode
        {
            *password = value;
        }
        self.focused_field = InputId::EditTrackerPassword;
        Task::none()
    }

    pub fn handle_edit_tracker_enabled_toggled(&mut self, value: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit { enabled, .. } = &mut conn.tracker_management.mode
        {
            *enabled = value;
        }
        Task::none()
    }

    /// Edit form submit.
    pub fn handle_edit_tracker_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.tracker_management.is_submitting {
            return Task::none();
        }

        // Snapshot Edit-mode payload.
        let (id, name, address, port, fingerprint, password, enabled) =
            match &conn.tracker_management.mode {
                TrackerManagementMode::Edit {
                    id,
                    name,
                    address,
                    port,
                    fingerprint,
                    password,
                    enabled,
                    ..
                } => (
                    *id,
                    name.clone(),
                    address.clone(),
                    *port,
                    fingerprint.clone(),
                    password.clone(),
                    *enabled,
                ),
                _ => return Task::none(),
            };

        if let Err(err_msg) = validate_tracker_form(&name, &address, &fingerprint, &password) {
            conn.tracker_management.form_error = Some(err_msg);
            return Task::none();
        }

        let fingerprint_opt = if fingerprint.trim().is_empty() {
            None
        } else {
            Some(fingerprint.trim().to_string())
        };
        let password_opt = if password.is_empty() {
            None
        } else {
            Some(password)
        };

        let msg = ClientMessage::TrackerUpdate {
            id,
            address,
            port,
            fingerprint: fingerprint_opt,
            password: password_opt,
            name,
            enabled,
        };

        conn.tracker_management.form_error = None;
        conn.tracker_management.is_submitting = true;

        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::TrackerManagementUpdateResult);
            }
            Err(e) => {
                conn.tracker_management.is_submitting = false;
                conn.tracker_management.form_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Validate Edit form on Enter.
    pub fn handle_validate_edit_tracker(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let TrackerManagementMode::Edit {
                name,
                address,
                fingerprint,
                password,
                ..
            } = &conn.tracker_management.mode
        {
            let name = name.clone();
            let address = address.clone();
            let fingerprint = fingerprint.clone();
            let password = password.clone();
            if let Err(msg) = validate_tracker_form(&name, &address, &fingerprint, &password) {
                conn.tracker_management.form_error = Some(msg);
            }
        }
        Task::none()
    }

    // ====================================================================
    // Remove confirm modal
    // ====================================================================

    pub fn handle_remove_tracker_confirm(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.tracker_management.is_remove_submitting {
            return Task::none();
        }

        let id = match &conn.tracker_management.mode {
            TrackerManagementMode::ConfirmRemove { id, .. } => *id,
            _ => return Task::none(),
        };

        conn.tracker_management.remove_error = None;
        conn.tracker_management.is_remove_submitting = true;

        match conn.send(ClientMessage::TrackerRemove { id }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::TrackerManagementRemoveResult);
            }
            Err(e) => {
                conn.tracker_management.is_remove_submitting = false;
                conn.tracker_management.remove_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    pub fn handle_remove_tracker_cancel(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.mode = TrackerManagementMode::List;
            conn.tracker_management.remove_error = None;
            conn.tracker_management.is_remove_submitting = false;
        }
        Task::none()
    }

    // ====================================================================
    // Accept Fingerprint dialog
    // ====================================================================

    pub fn handle_accept_fingerprint_confirm(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.tracker_management.is_accept_fingerprint_submitting {
            return Task::none();
        }

        let id = match &conn.tracker_management.mode {
            TrackerManagementMode::AcceptFingerprint { id, .. } => *id,
            _ => return Task::none(),
        };

        conn.tracker_management.accept_fingerprint_error = None;
        conn.tracker_management.is_accept_fingerprint_submitting = true;

        match conn.send(ClientMessage::TrackerAcceptFingerprint { id }) {
            Ok(message_id) => {
                conn.pending_requests.track(
                    message_id,
                    ResponseRouting::TrackerManagementAcceptFingerprintResult,
                );
            }
            Err(e) => {
                conn.tracker_management.is_accept_fingerprint_submitting = false;
                conn.tracker_management.accept_fingerprint_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    pub fn handle_accept_fingerprint_cancel(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.mode = TrackerManagementMode::List;
            conn.tracker_management.accept_fingerprint_error = None;
            conn.tracker_management.is_accept_fingerprint_submitting = false;
        }
        Task::none()
    }

    // ====================================================================
    // Identity-block fingerprint copy
    // ====================================================================

    /// Copy the server's certificate fingerprint to the clipboard and
    /// surface a toast.
    pub fn handle_copy_server_fingerprint(&mut self, fingerprint: String) -> Task<Message> {
        let toast = t("toast-fingerprint-copied");
        iced::clipboard::write(fingerprint).chain(Task::done(Message::ShowToast(toast)))
    }

    // ====================================================================
    // Tab navigation (focus chain) for Add / Edit tracker forms
    // ====================================================================
    //
    // Chain: Name → Address → Fingerprint → Password → wrap to Name.
    // Port is intentionally excluded because `NumberInput` consumes
    // the Tab key internally (see CLAUDE.md "UI Quirks") — including
    // it would break tab navigation. The `enabled` checkbox is also
    // excluded since checkboxes don't participate in text-input focus.
    // Click into Port directly with the mouse.
    //
    // We track the focus directly via `focused_field` and advance to
    // the next field on Tab — same pattern as the User Management Tab
    // handlers, avoiding async `is_focused` race conditions.

    /// Handle Tab pressed in the Add Tracker form.
    pub fn handle_add_tracker_tab_pressed(&mut self) -> Task<Message> {
        let next_field = match self.focused_field {
            InputId::AddTrackerName => InputId::AddTrackerAddress,
            InputId::AddTrackerAddress => InputId::AddTrackerFingerprint,
            InputId::AddTrackerFingerprint => InputId::AddTrackerPassword,
            InputId::AddTrackerPassword => InputId::AddTrackerName,
            _ => InputId::AddTrackerName,
        };
        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
    }

    /// Handle Tab pressed in the Edit Tracker form.
    pub fn handle_edit_tracker_tab_pressed(&mut self) -> Task<Message> {
        let next_field = match self.focused_field {
            InputId::EditTrackerName => InputId::EditTrackerAddress,
            InputId::EditTrackerAddress => InputId::EditTrackerFingerprint,
            InputId::EditTrackerFingerprint => InputId::EditTrackerPassword,
            InputId::EditTrackerPassword => InputId::EditTrackerName,
            _ => InputId::EditTrackerName,
        };
        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
    }
}

// ========================================================================
// Form validation helpers
// ========================================================================

/// Validate the shared subset of the tracker Add/Edit form. Returns the
/// translated error message on failure.
fn validate_tracker_form(
    name: &str,
    address: &str,
    fingerprint: &str,
    password: &str,
) -> Result<(), String> {
    if let Err(e) = validators::validate_tracker_name(name) {
        return Err(match e {
            TrackerNameError::Empty => t("err-tracker-name-empty"),
            TrackerNameError::TooLong => t_args(
                "err-tracker-name-too-long",
                &[("max", &MAX_TRACKER_NAME_LENGTH.to_string())],
            ),
            TrackerNameError::ContainsNewlines => t("err-tracker-name-contains-newlines"),
            TrackerNameError::InvalidCharacters => t("err-tracker-name-invalid-characters"),
        });
    }

    if let Err(e) = validators::validate_tracker_address(address) {
        return Err(match e {
            TrackerAddressError::Empty => t("err-tracker-address-empty"),
            TrackerAddressError::Invalid(inner) => match inner {
                PublicAddressError::TooLong => t_args(
                    "err-tracker-address-too-long",
                    &[("max", &validators::MAX_PUBLIC_ADDRESS_LENGTH.to_string())],
                ),
                PublicAddressError::ContainsScheme => t("err-tracker-address-contains-scheme"),
                PublicAddressError::ContainsBrackets => t("err-tracker-address-contains-brackets"),
                PublicAddressError::ContainsPath => t("err-tracker-address-contains-path"),
                PublicAddressError::ContainsUserinfo => t("err-tracker-address-contains-userinfo"),
                PublicAddressError::ContainsWhitespace => {
                    t("err-tracker-address-contains-whitespace")
                }
                PublicAddressError::ContainsPort => t("err-tracker-address-contains-port"),
                PublicAddressError::ContainsZoneId => t("err-tracker-address-contains-zone-id"),
                PublicAddressError::InvalidFormat => t("err-tracker-address-invalid-format"),
            },
        });
    }

    let fingerprint_trimmed = fingerprint.trim();
    if !fingerprint_trimmed.is_empty() && !is_canonical_fingerprint(fingerprint_trimmed) {
        return Err(t("err-tracker-fingerprint-invalid"));
    }

    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(t_args(
            "err-tracker-password-too-long",
            &[("max", &MAX_PASSWORD_LENGTH.to_string())],
        ));
    }

    Ok(())
}

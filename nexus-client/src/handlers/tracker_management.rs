//! Tracker management handlers (Trackers tab inside the Server Info panel).
//!
//! Mirrors the user/group management handler shape: panel toggles,
//! sub-view transitions, form input handlers, validate-then-submit on
//! Enter, and is_submitting / is_remove_submitting / is_accept_submitting
//! double-submit guards.

use iced::Task;
use iced::widget::Id;
use nexus_common::names::fold_name;
use nexus_common::protocol::{ClientMessage, TrackerInfo};

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
        self.focus_field(InputId::AddTrackerName)
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
        let existing = conn
            .tracker_management
            .all_trackers
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        // Validate — same checks as Edit, surfaced through helper.
        if let Err(err_msg) = validate_tracker_form(
            &name,
            &address,
            port,
            &fingerprint,
            &password,
            existing,
            None,
        ) {
            conn.tracker_management.form_error = Some(err_msg);
            return Task::none();
        }

        let fingerprint_opt = super::tracker_browser::optional_string(&fingerprint);
        let password_opt = super::tracker_browser::optional_string(&password);

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
            let port = conn.tracker_management.add_port;
            let fingerprint = &conn.tracker_management.add_fingerprint;
            let password = &conn.tracker_management.add_password;
            let existing = conn
                .tracker_management
                .all_trackers
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if let Err(msg) =
                validate_tracker_form(name, address, port, fingerprint, password, existing, None)
            {
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
        let (id, name, address, port, fingerprint, password) = match &conn.tracker_management.mode {
            TrackerManagementMode::Edit {
                id,
                name,
                address,
                port,
                fingerprint,
                password,
                ..
            } => (
                *id,
                name.clone(),
                address.clone(),
                *port,
                fingerprint.clone(),
                password.clone(),
            ),
            _ => return Task::none(),
        };

        let existing = conn
            .tracker_management
            .all_trackers
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if let Err(err_msg) = validate_tracker_form(
            &name,
            &address,
            port,
            &fingerprint,
            &password,
            existing,
            Some(id),
        ) {
            conn.tracker_management.form_error = Some(err_msg);
            return Task::none();
        }

        let Some(fields) = conn.tracker_management.mode.tracker_update_fields() else {
            return Task::none();
        };

        let msg = ClientMessage::TrackerUpdate {
            id: fields.id,
            address: fields.address,
            port: fields.port,
            fingerprint: fields.fingerprint,
            password: fields.password,
            name: fields.name,
            enabled: fields.enabled,
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
                id,
                name,
                address,
                port,
                fingerprint,
                password,
                ..
            } = &conn.tracker_management.mode
        {
            let id = *id;
            let port = *port;
            let name = name.clone();
            let address = address.clone();
            let fingerprint = fingerprint.clone();
            let password = password.clone();
            let existing = conn
                .tracker_management
                .all_trackers
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if let Err(msg) = validate_tracker_form(
                &name,
                &address,
                port,
                &fingerprint,
                &password,
                existing,
                Some(id),
            ) {
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
    // Port is intentionally excluded because `iced_aw::NumberInput`
    // consumes the Tab key internally — including it would break
    // tab navigation. The `enabled` checkbox is also excluded since
    // checkboxes don't participate in text-input focus. Click into
    // Port directly with the mouse.
    //
    // We track the focus directly via `focused_field` and advance to
    // the next field on Tab — same pattern as the User Management Tab
    // handlers, avoiding async `is_focused` race conditions.

    /// Handle Tab pressed in the Add Tracker form.
    pub fn handle_add_tracker_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::AddTrackerTabResolved)
    }

    pub fn handle_add_tracker_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        // Fingerprint is conventionally last across both tracker
        // forms (BBS-admin + client discovery).
        const CYCLE: &[InputId] = &[
            InputId::AddTrackerName,
            InputId::AddTrackerAddress,
            InputId::AddTrackerPassword,
            InputId::AddTrackerFingerprint,
        ];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focus_field(next)
    }

    /// Handle Tab pressed in the Edit Tracker form.
    pub fn handle_edit_tracker_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::EditTrackerTabResolved)
    }

    pub fn handle_edit_tracker_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        const CYCLE: &[InputId] = &[
            InputId::EditTrackerName,
            InputId::EditTrackerAddress,
            InputId::EditTrackerPassword,
            InputId::EditTrackerFingerprint,
        ];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focus_field(next)
    }
}

// ========================================================================
// Form validation helpers
// ========================================================================

/// Validate the shared subset of the tracker Add/Edit form. Returns the
/// translated error message on failure.
///
/// Field-shape checks run first; on success the new entry is then deduped
/// against `existing` using Unicode-aware case folding (`to_lowercase`).
/// This catches IDN collisions client-side that the server's
/// `LOWER(address)` / `LOWER(name)` unique indexes miss — SQLite's
/// `LOWER` is ASCII-only, so `MÜNCHEN.de` and `münchen.de` would
/// otherwise both pass the index. `excluding_id` is `Some(id)` for Edit
/// mode so a row doesn't trip on itself.
fn validate_tracker_form(
    name: &str,
    address: &str,
    port: u16,
    fingerprint: &str,
    password: &str,
    existing: &[TrackerInfo],
    excluding_id: Option<i64>,
) -> Result<(), String> {
    super::tracker_form_errors::translate_tracker_field_errors(
        name,
        address,
        fingerprint,
        password,
    )?;

    let name_key = fold_name(name);
    let address_key = address.to_lowercase();
    for entry in existing {
        if Some(entry.id) == excluding_id {
            continue;
        }
        if fold_name(&entry.name) == name_key {
            return Err(t_args("err-tracker-name-duplicate", &[("name", name)]));
        }
        if entry.address.to_lowercase() == address_key && entry.port == port {
            return Err(t("err-tracker-address-duplicate"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use nexus_common::framing::MessageId;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams};

    const VALID_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
        AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

    fn tracker(id: i64, name: &str, address: &str, port: u16) -> TrackerInfo {
        TrackerInfo {
            id,
            address: address.to_string(),
            port,
            fingerprint: None,
            password: None,
            name: name.to_string(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
            connected: false,
            last_connected_at: None,
            last_attempted_at: None,
            last_error: None,
            last_error_kind: None,
            pending_fingerprint: None,
            refresh_interval: None,
        }
    }

    fn test_connection_with_receiver(
        connection_id: usize,
    ) -> (
        ServerConnection,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: "me".to_string(),
            },
            display_name: String::new(),
            connection_id,
            is_admin: true,
            permissions: Vec::new(),
            features: Vec::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            cached_server_image: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            max_connections_per_ip: None,
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Weak,
            log_level: None,
            scheduler_chunk_size: None,
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        });
        (conn, rx)
    }

    #[test]
    fn dedup_rejects_duplicate_name_case_insensitive() {
        let existing = vec![tracker(1, "Public", "a.example.com", 7510)];
        let err = validate_tracker_form("public", "b.example.com", 7510, "", "", &existing, None)
            .expect_err("should reject duplicate name");
        assert_eq!(
            err,
            t_args("err-tracker-name-duplicate", &[("name", "public")])
        );
    }

    #[test]
    fn dedup_rejects_duplicate_address_port_case_insensitive() {
        let existing = vec![tracker(1, "First", "tracker.example.com", 7510)];
        let err = validate_tracker_form(
            "Second",
            "Tracker.Example.COM",
            7510,
            "",
            "",
            &existing,
            None,
        )
        .expect_err("should reject duplicate address+port");
        assert_eq!(err, t("err-tracker-address-duplicate"));
    }

    #[test]
    fn dedup_allows_same_address_different_port() {
        let existing = vec![tracker(1, "First", "tracker.example.com", 7510)];
        assert!(
            validate_tracker_form(
                "Second",
                "tracker.example.com",
                7520,
                "",
                "",
                &existing,
                None,
            )
            .is_ok()
        );
    }

    #[test]
    fn dedup_allows_different_address_same_port() {
        let existing = vec![tracker(1, "First", "alpha.example.com", 7510)];
        assert!(
            validate_tracker_form("Second", "beta.example.com", 7510, "", "", &existing, None,)
                .is_ok()
        );
    }

    #[test]
    fn dedup_rejects_idn_address_unicode_case_fold() {
        // SQLite's `LOWER()` is ASCII-only and would treat these as
        // distinct. Client-side `to_lowercase()` is Unicode-aware and
        // catches the IDN collision the server's index misses.
        let existing = vec![tracker(1, "First", "MÜNCHEN.de", 7510)];
        let err = validate_tracker_form("Second", "münchen.de", 7510, "", "", &existing, None)
            .expect_err("should reject IDN duplicate");
        assert_eq!(err, t("err-tracker-address-duplicate"));
    }

    #[test]
    fn dedup_excludes_self_on_edit() {
        let existing = vec![
            tracker(1, "Public", "tracker.example.com", 7510),
            tracker(2, "Other", "other.example.com", 7510),
        ];
        // Editing row id=1 to the same name+address+port must not
        // trip on its own existing record.
        assert!(
            validate_tracker_form(
                "Public",
                "tracker.example.com",
                7510,
                "",
                "",
                &existing,
                Some(1),
            )
            .is_ok()
        );
    }

    #[test]
    fn edit_mode_has_no_effective_update_changes_initially() {
        let mut state = crate::types::TrackerManagementState::default();
        state.enter_edit_mode(crate::types::TrackerEditInit {
            id: 7,
            name: "Public Tracker".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: Some(VALID_FINGERPRINT.to_string()),
            password: Some("secret".to_string()),
            enabled: true,
            last_error: None,
        });

        assert!(!state.mode.has_effective_tracker_update_changes());
    }

    #[test]
    fn edit_mode_detects_effective_update_changes() {
        let mut state = crate::types::TrackerManagementState::default();
        state.enter_edit_mode(crate::types::TrackerEditInit {
            id: 7,
            name: "Public Tracker".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: Some(VALID_FINGERPRINT.to_string()),
            password: Some("secret".to_string()),
            enabled: true,
            last_error: None,
        });
        if let TrackerManagementMode::Edit { enabled, .. } = &mut state.mode {
            *enabled = false;
        }

        assert!(state.mode.has_effective_tracker_update_changes());
    }

    #[test]
    fn edit_mode_detects_padded_fingerprint_and_password_changes() {
        let mut state = crate::types::TrackerManagementState::default();
        state.enter_edit_mode(crate::types::TrackerEditInit {
            id: 7,
            name: "Public Tracker".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: Some(VALID_FINGERPRINT.to_string()),
            password: Some("secret".to_string()),
            enabled: true,
            last_error: None,
        });
        if let TrackerManagementMode::Edit {
            fingerprint,
            password,
            ..
        } = &mut state.mode
        {
            *fingerprint = format!("  {VALID_FINGERPRINT}  ");
            *password = "  secret  ".to_string();
        }

        assert!(state.mode.has_effective_tracker_update_changes());
    }

    #[test]
    fn update_submit_sends_whitespace_password_verbatim() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.tracker_management.all_trackers = Some(Ok(vec![tracker(
            7,
            "Public Tracker",
            "tracker.example.com",
            7510,
        )]));
        conn.tracker_management.mode = TrackerManagementMode::Edit {
            id: 7,
            original_name: "Public Tracker".to_string(),
            original_address: "tracker.example.com".to_string(),
            original_port: 7510,
            original_fingerprint: String::new(),
            original_password: String::new(),
            original_enabled: true,
            last_error: None,
            name: "Public Tracker".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: String::new(),
            password: "   ".to_string(),
            enabled: true,
        };
        app.connections.insert(1, conn);

        let _ = app.handle_edit_tracker_submit();

        match rx.try_recv() {
            Ok((_, ClientMessage::TrackerUpdate { password, .. })) => {
                assert_eq!(password.as_deref(), Some("   "));
            }
            other => panic!("expected TrackerUpdate, got {other:?}"),
        }
    }

    #[test]
    fn update_submit_does_not_rewrite_legacy_padded_tracker_password() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.tracker_management.all_trackers = Some(Ok(vec![tracker(
            7,
            "Public Tracker",
            "tracker.example.com",
            7510,
        )]));
        conn.tracker_management.mode = TrackerManagementMode::Edit {
            id: 7,
            original_name: "Public Tracker".to_string(),
            original_address: "tracker.example.com".to_string(),
            original_port: 7510,
            original_fingerprint: String::new(),
            original_password: " secret ".to_string(),
            original_enabled: true,
            last_error: None,
            name: "Public Tracker".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: String::new(),
            password: " secret ".to_string(),
            enabled: false,
        };
        app.connections.insert(1, conn);

        let _ = app.handle_edit_tracker_submit();

        match rx.try_recv() {
            Ok((
                _,
                ClientMessage::TrackerUpdate {
                    password, enabled, ..
                },
            )) => {
                assert!(password.is_none());
                assert_eq!(enabled, Some(false));
            }
            other => panic!("expected TrackerUpdate, got {other:?}"),
        }
    }

    #[test]
    fn update_submit_rejects_whitespace_fingerprint() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.tracker_management.all_trackers = Some(Ok(vec![tracker(
            7,
            "Public Tracker",
            "tracker.example.com",
            7510,
        )]));
        conn.tracker_management.mode = TrackerManagementMode::Edit {
            id: 7,
            original_name: "Public Tracker".to_string(),
            original_address: "tracker.example.com".to_string(),
            original_port: 7510,
            original_fingerprint: String::new(),
            original_password: String::new(),
            original_enabled: true,
            last_error: None,
            name: "Public Tracker".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: format!("  {VALID_FINGERPRINT}  "),
            password: String::new(),
            enabled: true,
        };
        app.connections.insert(1, conn);

        let _ = app.handle_edit_tracker_submit();

        // A fingerprint with surrounding whitespace is non-canonical — the form
        // rejects it; no TrackerUpdate is sent and the form surfaces the error.
        assert!(rx.try_recv().is_err());
        assert!(
            app.connections
                .get(&1)
                .expect("connection")
                .tracker_management
                .form_error
                .is_some()
        );
    }

    #[test]
    fn update_submit_sends_only_changed_fields() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.tracker_management.all_trackers = Some(Ok(vec![tracker(
            7,
            "Public Tracker",
            "tracker.example.com",
            7510,
        )]));
        conn.tracker_management.mode = TrackerManagementMode::Edit {
            id: 7,
            original_name: "Public Tracker".to_string(),
            original_address: "tracker.example.com".to_string(),
            original_port: 7510,
            original_fingerprint: String::new(),
            original_password: String::new(),
            original_enabled: true,
            last_error: None,
            name: "Public Tracker 2".to_string(),
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: String::new(),
            password: "  secret  ".to_string(),
            enabled: true,
        };
        app.connections.insert(1, conn);

        let _ = app.handle_edit_tracker_submit();

        match rx.try_recv() {
            Ok((
                _,
                ClientMessage::TrackerUpdate {
                    name,
                    password,
                    address,
                    port,
                    fingerprint,
                    enabled,
                    ..
                },
            )) => {
                assert_eq!(name.as_deref(), Some("Public Tracker 2"));
                assert_eq!(password.as_deref(), Some("  secret  "));
                assert!(address.is_none());
                assert!(port.is_none());
                assert!(fingerprint.is_none());
                assert!(enabled.is_none());
            }
            other => panic!("expected TrackerUpdate, got {other:?}"),
        }
    }
}

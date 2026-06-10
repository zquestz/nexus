//! Group management handlers
//!
//! Handles all group management UI interactions within the Groups tab
//! of the User Management panel, including group CRUD operations and
//! the group dropdown in user create/edit forms.

use iced::Task;
use iced::widget::Id;
use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, GroupNameError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    GroupManagementMode, GroupManagementSortColumn, InputId, Message, PendingRequests,
    ResponseRouting, UserManagementMode, UserManagementTab,
};

impl NexusApp {
    // ==================== Tab & Group Selection ====================

    /// Handle tab selection within the User Management panel (Users / Groups)
    ///
    /// When switching to the Groups tab, fetches the group list from the server
    /// if it hasn't been loaded yet.
    pub fn handle_user_management_tab_selected(&mut self, tab: UserManagementTab) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.active_tab = tab;

        // When switching to Groups tab, fetch group list if not already loaded
        if tab == UserManagementTab::Groups && conn.user_management.available_groups.is_none() {
            match conn.send(ClientMessage::GroupList) {
                Ok(message_id) => {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::PopulateGroupManagementList);
                }
                Err(e) => {
                    conn.user_management.available_groups =
                        Some(Err(format!("{}: {}", t("err-send-failed"), e)));
                }
            }
        }

        Task::none()
    }

    /// Handle group dropdown selection in the user create form
    ///
    /// When a group is selected or changed, applies the formula:
    ///   new_enabled = in_new_group || (was_checked && !in_old_group)
    ///
    /// This ensures:
    /// - New group permissions are checked (inherited)
    /// - Old group's inherited permissions are cleared
    /// - User overrides (permissions not from the old group) are preserved
    pub fn handle_user_management_group_selected(
        &mut self,
        group_id: Option<i64>,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Look up old group permissions before overwriting
            let old_group_permissions: Vec<String> =
                if let Some(old_gid) = conn.user_management.create_group_id {
                    conn.user_management
                        .loaded_groups()
                        .and_then(|groups| groups.iter().find(|g| g.id == old_gid))
                        .map(|g| g.permissions.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };

            // Look up new group permissions
            let new_group_permissions: Vec<String> = if let Some(gid) = group_id {
                conn.user_management
                    .loaded_groups()
                    .and_then(|groups| groups.iter().find(|g| g.id == gid))
                    .map(|g| g.permissions.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // Apply formula: new_enabled = in_new_group || (was_checked && !in_old_group)
            for (perm_name, enabled) in &mut conn.user_management.permissions {
                let in_new_group = new_group_permissions.contains(perm_name);
                let in_old_group = old_group_permissions.contains(perm_name);
                *enabled = in_new_group || (*enabled && !in_old_group);
            }

            conn.user_management.create_group_id = group_id;
        }
        Task::none()
    }

    /// Handle group dropdown selection in the user edit form
    ///
    /// When a group is selected or changed, applies the formula:
    ///   new_enabled = in_new_group || (was_checked && !in_old_group)
    ///
    /// This ensures:
    /// - New group permissions are checked (inherited)
    /// - Old group's inherited permissions are cleared
    /// - User overrides (permissions not from the old group) are preserved
    pub fn handle_user_management_edit_group_selected(
        &mut self,
        group_id: Option<i64>,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Look up new group permissions
        let new_group_permissions: Vec<String> = if let Some(gid) = group_id {
            conn.user_management
                .loaded_groups()
                .and_then(|groups| groups.iter().find(|g| g.id == gid))
                .map(|g| g.permissions.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if let UserManagementMode::Edit {
            group_id: ref mut edit_group_id,
            group_permissions: ref mut edit_group_perms,
            revoked_permissions: ref mut edit_revoked,
            permissions: ref mut edit_perms,
            ..
        } = conn.user_management.mode
        {
            // Capture old group permissions before modifying
            let old_group_permissions = edit_group_perms.clone();

            // Apply formula: new_enabled = in_new_group || (was_checked && !in_old_group)
            for (perm_name, enabled) in edit_perms.iter_mut() {
                let in_new_group = new_group_permissions.contains(perm_name);
                let in_old_group = old_group_permissions.contains(perm_name);
                *enabled = in_new_group || (*enabled && !in_old_group);
            }

            *edit_group_id = group_id;
            *edit_revoked = Vec::new();
            *edit_group_perms = new_group_permissions;
        }

        Task::none()
    }

    // ==================== Group Management Panel Navigation ====================

    /// Handle cancel in the group management sub-panel
    ///
    /// In Create/Edit/ConfirmDelete mode: returns to List mode.
    /// In List mode: does nothing (CancelUserManagement handles closing the panel).
    pub fn handle_cancel_group_management(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match &conn.user_management.group_management.mode {
            GroupManagementMode::Create
            | GroupManagementMode::Edit { .. }
            | GroupManagementMode::ConfirmDelete { .. } => {
                // Escape from any sub-mode dismisses the form / dialog
                // and returns to the Groups list. Mirrors User
                // ConfirmDelete's reset behaviour at the parent
                // `handle_cancel_user_management`.
                conn.user_management.group_management.reset_to_list();
            }
            GroupManagementMode::List => {
                // List: parent panel handles close
            }
        }

        Task::none()
    }

    // ==================== Group List View Actions ====================

    /// Show the create group form
    pub fn handle_group_management_show_create(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.group_management.enter_create_mode();
        self.focus_field(InputId::CreateGroupName)
    }

    /// Handle edit button click on a group in the list
    ///
    /// Requests group details from the server, then transitions to edit mode.
    pub fn handle_group_management_edit_clicked(
        &mut self,
        id: i64,
        _name: String,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Request group details from server
        match conn.send(ClientMessage::GroupEdit { id }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateGroupManagementEdit);
            }
            Err(e) => {
                conn.user_management.set_group_list_error(format!(
                    "{}: {}",
                    t("err-send-failed"),
                    e
                ));
            }
        }

        Task::none()
    }

    /// Handle delete button click on a group in the list
    ///
    /// Shows the delete confirmation modal.
    pub fn handle_group_management_delete_clicked(
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

        conn.user_management
            .group_management
            .enter_confirm_delete_mode(id, name);
        Task::none()
    }

    /// Handle confirm delete button in the group delete confirmation modal
    ///
    /// Sends the delete request and keeps the dialog open until a response arrives.
    pub fn handle_group_management_confirm_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Prevent double-submit
        if conn.user_management.group_management.is_delete_submitting {
            return Task::none();
        }

        let id = match &conn.user_management.group_management.mode {
            GroupManagementMode::ConfirmDelete { id, .. } => *id,
            _ => return Task::none(),
        };

        // Clear any previous error before sending
        conn.user_management.group_management.delete_error = None;

        // Send delete request (keep dialog open until response)
        conn.user_management.group_management.is_delete_submitting = true;
        match conn.send(ClientMessage::GroupDelete { id }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::GroupManagementDeleteResult);
            }
            Err(e) => {
                // Show send error in the delete dialog
                conn.user_management.group_management.is_delete_submitting = false;
                conn.user_management.group_management.delete_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle cancel delete button in the group delete confirmation modal
    pub fn handle_group_management_cancel_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Clear error and return to list
        conn.user_management.group_management.delete_error = None;
        conn.user_management.group_management.mode = GroupManagementMode::List;
        Task::none()
    }

    // ==================== Create Form Handlers ====================

    /// Handle group name field change in create form
    pub fn handle_group_management_name_changed(&mut self, name: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.group_management.name = name;
        }
        Task::none()
    }

    /// Handle bandwidth weight field change in create form
    pub fn handle_group_management_bandwidth_weight_changed(
        &mut self,
        weight: u16,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.group_management.bandwidth_weight = weight;
        }
        Task::none()
    }

    /// Handle is_shared checkbox toggle in create form
    ///
    /// When toggled ON, disables forbidden permissions (only allows shared account permissions).
    pub fn handle_group_management_is_shared_toggled(&mut self, is_shared: bool) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.group_management.is_shared = is_shared;

            if is_shared {
                // Disable forbidden permissions (only allow shared account permissions)
                for (perm_name, enabled) in &mut conn.user_management.group_management.permissions {
                    if !is_shared_account_permission(perm_name) {
                        *enabled = false;
                    }
                }
            }
        }
        Task::none()
    }

    /// Handle permission checkbox toggle in create form
    pub fn handle_group_management_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(perm) = conn
                .user_management
                .group_management
                .permissions
                .iter_mut()
                .find(|(p, _)| p == &permission)
        {
            perm.1 = enabled;
        }
        Task::none()
    }

    /// Validate create group form (called on Enter when form incomplete)
    pub fn handle_validate_group_management_create(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            let name = &conn.user_management.group_management.name;
            if let Err(e) = validators::validate_group_name(name) {
                conn.user_management.group_management.create_error = Some(match e {
                    GroupNameError::Empty => t("err-group-name-empty"),
                    GroupNameError::TooLong => t_args(
                        "err-group-name-too-long",
                        &[("max", &validators::MAX_GROUP_NAME_LENGTH.to_string())],
                    ),
                    GroupNameError::InvalidCharacters => t("err-group-name-invalid"),
                });
            }
        }
        Task::none()
    }

    /// Validate edit group form (called on Enter when form incomplete)
    pub fn handle_validate_group_management_edit(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let GroupManagementMode::Edit { new_name, .. } =
                &conn.user_management.group_management.mode
            && let Err(e) = validators::validate_group_name(new_name)
        {
            conn.user_management.group_management.edit_error = Some(match e {
                GroupNameError::Empty => t("err-group-name-empty"),
                GroupNameError::TooLong => t_args(
                    "err-group-name-too-long",
                    &[("max", &validators::MAX_GROUP_NAME_LENGTH.to_string())],
                ),
                GroupNameError::InvalidCharacters => t("err-group-name-invalid"),
            });
        }
        Task::none()
    }

    /// Handle create group button press
    ///
    /// Validates the group name, filters permissions by delegation,
    /// and sends the `GroupCreate` message.
    pub fn handle_group_management_create_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Prevent double-submit
        if conn.user_management.group_management.is_submitting {
            return Task::none();
        }

        // Validate group name
        let name = &conn.user_management.group_management.name;
        if let Err(e) = validators::validate_group_name(name) {
            conn.user_management.group_management.create_error = Some(match e {
                GroupNameError::Empty => t("err-group-name-empty"),
                GroupNameError::TooLong => t_args(
                    "err-group-name-too-long",
                    &[("max", &validators::MAX_GROUP_NAME_LENGTH.to_string())],
                ),
                GroupNameError::InvalidCharacters => t("err-group-name-invalid"),
            });
            return Task::none();
        }

        // Only send permissions that the current user has (non-admin delegation)
        let permissions: Vec<String> = conn
            .user_management
            .group_management
            .permissions
            .iter()
            .filter(|(perm_name, enabled)| *enabled && conn.has_permission(perm_name))
            .map(|(name, _)| name.clone())
            .collect();

        let msg = ClientMessage::GroupCreate {
            name: conn.user_management.group_management.name.clone(),
            is_shared: conn.user_management.group_management.is_shared,
            permissions,
            bandwidth_weight: conn.user_management.group_management.bandwidth_weight,
        };

        // Clear any previous error on new submission
        conn.user_management.group_management.create_error = None;

        // Send message and track for response routing
        conn.user_management.group_management.is_submitting = true;
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::GroupManagementCreateResult);
            }
            Err(e) => {
                conn.user_management.group_management.is_submitting = false;
                conn.user_management.group_management.create_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    // ==================== Edit Form Handlers ====================

    /// Handle group name field change in edit form
    pub fn handle_group_management_edit_name_changed(&mut self, name: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let GroupManagementMode::Edit {
                new_name: ref mut nn,
                ..
            } = conn.user_management.group_management.mode
        {
            *nn = name;
        }
        Task::none()
    }

    /// Handle bandwidth weight field change in edit form
    pub fn handle_group_management_edit_bandwidth_weight_changed(
        &mut self,
        weight: u16,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let GroupManagementMode::Edit {
                bandwidth_weight: ref mut bw,
                ..
            } = conn.user_management.group_management.mode
        {
            *bw = weight;
        }
        Task::none()
    }

    /// Handle is_shared checkbox toggle in edit form
    ///
    /// Only allowed when `member_count == 0`. When toggled ON, disables
    /// forbidden permissions (only allows shared account permissions).
    pub fn handle_group_management_edit_is_shared_toggled(
        &mut self,
        is_shared: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let GroupManagementMode::Edit {
                is_shared: ref mut edit_is_shared,
                member_count,
                permissions: ref mut edit_perms,
                ..
            } = conn.user_management.group_management.mode
        {
            // Only allow toggling is_shared when group has no members
            if member_count == 0 {
                *edit_is_shared = is_shared;

                if is_shared {
                    // Disable forbidden permissions (only allow shared account permissions)
                    for (perm_name, enabled) in edit_perms.iter_mut() {
                        if !is_shared_account_permission(perm_name) {
                            *enabled = false;
                        }
                    }
                }
            }
        }
        Task::none()
    }

    /// Handle permission checkbox toggle in edit form
    pub fn handle_group_management_edit_permission_toggled(
        &mut self,
        permission: String,
        enabled: bool,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let GroupManagementMode::Edit {
                permissions: ref mut perms,
                ..
            } = conn.user_management.group_management.mode
            && let Some(perm) = perms.iter_mut().find(|(p, _)| p == &permission)
        {
            perm.1 = enabled;
        }
        Task::none()
    }

    /// Handle update group button press
    ///
    /// Validates the group form, filters permissions by delegation, and sends
    /// `GroupUpdate`, omitting name and bandwidth weight when unchanged.
    pub fn handle_group_management_update_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Prevent double-submit
        if conn.user_management.group_management.is_submitting {
            return Task::none();
        }
        let new_name = match &conn.user_management.group_management.mode {
            GroupManagementMode::Edit { new_name, .. } => new_name.clone(),
            _ => return Task::none(),
        };

        // Validate new group name
        if let Err(e) = validators::validate_group_name(&new_name) {
            conn.user_management.group_management.edit_error = Some(match e {
                GroupNameError::Empty => t("err-group-name-empty"),
                GroupNameError::TooLong => t_args(
                    "err-group-name-too-long",
                    &[("max", &validators::MAX_GROUP_NAME_LENGTH.to_string())],
                ),
                GroupNameError::InvalidCharacters => t("err-group-name-invalid"),
            });
            return Task::none();
        }

        let requester_is_admin = conn.is_admin;
        let requester_permissions = conn.permissions.clone();
        let Some(fields) = conn
            .user_management
            .group_management
            .mode
            .group_update_fields(|permission| {
                requester_is_admin || requester_permissions.iter().any(|p| p == permission)
            })
        else {
            return Task::none();
        };

        let msg = ClientMessage::GroupUpdate {
            id: fields.id,
            name: fields.name,
            is_shared: fields.is_shared,
            permissions: fields.permissions,
            bandwidth_weight: fields.bandwidth_weight,
        };

        // Clear any previous error on new submission
        conn.user_management.group_management.edit_error = None;

        // Send message and track for response routing
        conn.user_management.group_management.is_submitting = true;
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::GroupManagementUpdateResult);
            }
            Err(e) => {
                conn.user_management.group_management.is_submitting = false;
                conn.user_management.group_management.edit_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle sort column click in the Groups table
    ///
    /// If clicking the same column, toggles direction. If clicking a different column,
    /// sets it as the active column with ascending direction.
    pub fn handle_group_management_sort_by(
        &mut self,
        column: GroupManagementSortColumn,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            if conn.user_management.group_management.sort_column == column {
                conn.user_management.group_management.sort_ascending =
                    !conn.user_management.group_management.sort_ascending;
            } else {
                conn.user_management.group_management.sort_column = column;
                conn.user_management.group_management.sort_ascending = true;
            }
        }
        Task::none()
    }

    // ==================== Tab Navigation ====================

    /// Tab in Group Create form. Single field, but Tab still queries
    /// iced for the focused widget so click-and-Tab without typing
    /// still routes back to the Name input rather than relying on a
    /// stale tracker.
    pub fn handle_group_management_create_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::GroupManagementCreateTabResolved)
    }

    pub fn handle_group_management_create_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        // NumberInput fields (CreateGroupBandwidthWeight) consume Tab
        // internally and are reachable only via mouse — excluded from the
        // cycle so tabbing doesn't dead-end inside them.
        const CYCLE: &[InputId] = &[InputId::CreateGroupName];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focus_field(next)
    }

    /// Tab in Group Edit form. Same shape as Group Create.
    pub fn handle_group_management_edit_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::GroupManagementEditTabResolved)
    }

    pub fn handle_group_management_edit_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        // NumberInput fields (EditGroupBandwidthWeight) consume Tab
        // internally — see create form comment above.
        const CYCLE: &[InputId] = &[InputId::EditGroupName];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focus_field(next)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use nexus_common::framing::MessageId;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams};

    fn test_connection_with_receiver(
        connection_id: usize,
    ) -> (
        ServerConnection,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: Some(1),
            nickname: "admin".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "admin".to_string(),
                password: String::new(),
                nickname: "admin".to_string(),
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
    fn update_group_does_not_send_unchanged_edit() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.user_management.group_management.enter_edit_mode(
            7,
            "Staff".to_string(),
            false,
            0,
            vec!["chat_send".to_string()],
            1,
        );
        app.connections.insert(1, conn);

        let _ = app.handle_group_management_update_pressed();

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn update_group_name_change_sends_only_name() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.user_management.group_management.enter_edit_mode(
            7,
            "Staff".to_string(),
            false,
            0,
            vec!["chat_send".to_string()],
            1,
        );
        if let GroupManagementMode::Edit { new_name, .. } =
            &mut conn.user_management.group_management.mode
        {
            *new_name = "Staff2".to_string();
        }
        app.connections.insert(1, conn);

        let _ = app.handle_group_management_update_pressed();

        match rx.try_recv() {
            Ok((
                _,
                ClientMessage::GroupUpdate {
                    name,
                    is_shared,
                    permissions,
                    bandwidth_weight,
                    ..
                },
            )) => {
                assert_eq!(name.as_deref(), Some("Staff2"));
                assert!(is_shared.is_none());
                assert!(permissions.is_none());
                assert!(bandwidth_weight.is_none());
            }
            other => panic!("expected GroupUpdate, got {other:?}"),
        }
    }
}

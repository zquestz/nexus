//! Group management handlers
//!
//! Handles all group management UI interactions within the Groups tab
//! of the User Management panel, including group CRUD operations and
//! the group dropdown in user create/edit forms.

use iced::Task;
use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, GroupNameError};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    GroupManagementMode, GroupManagementSortColumn, Message, PendingRequests, ResponseRouting,
    UserManagementMode, UserManagementTab,
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
                    conn.user_management.group_management.list_error =
                        Some(format!("{}: {}", t("err-send-failed"), e));
                }
            }
        }

        Task::none()
    }

    /// Handle group dropdown selection in the user create form
    ///
    /// Sets the `create_group_id` on the user management state.
    pub fn handle_user_management_group_selected(
        &mut self,
        group_id: Option<i64>,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_management.create_group_id = group_id;
        }
        Task::none()
    }

    /// Handle group dropdown selection in the user edit form
    ///
    /// When a new group is selected:
    /// - Look up the group in `available_groups`
    /// - Update `group_id`, `group_permissions`, clear `revoked_permissions`
    /// - For each permission: if it's in the new group's permissions, set checked=true;
    ///   otherwise keep the existing checked state (existing grants become overrides)
    ///
    /// When None is selected (remove group):
    /// - Set `group_id = None`, clear `group_permissions` and `revoked_permissions`
    /// - Keep all permission checked states as-is (they become individual permissions)
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

        // Look up group permissions if a group was selected
        let new_group_permissions: Vec<String> = if let Some(gid) = group_id {
            conn.user_management
                .available_groups
                .as_ref()
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
            *edit_group_id = group_id;
            *edit_revoked = Vec::new();

            if group_id.is_some() {
                // Group selected: mark group permissions as checked,
                // keep existing state for non-group permissions
                for (perm_name, enabled) in edit_perms.iter_mut() {
                    if new_group_permissions.contains(perm_name) {
                        *enabled = true;
                    }
                    // Otherwise keep existing checked state (becomes an override)
                }
                *edit_group_perms = new_group_permissions;
            } else {
                // Group removed: keep all permission states as-is,
                // they become individual permissions
                *edit_group_perms = Vec::new();
            }
        }

        Task::none()
    }

    // ==================== Group Management Panel Navigation ====================

    /// Handle cancel in the group management sub-panel
    ///
    /// In Create/Edit mode: returns to List mode.
    /// In List mode: does nothing (CancelUserManagement handles closing the panel).
    pub fn handle_cancel_group_management(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match &conn.user_management.group_management.mode {
            GroupManagementMode::Create | GroupManagementMode::Edit { .. } => {
                conn.user_management.group_management.reset_to_list();
            }
            GroupManagementMode::List | GroupManagementMode::ConfirmDelete { .. } => {
                // List: do nothing (parent panel handles close)
                // ConfirmDelete: modal handles its own cancel
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
        Task::none()
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
                conn.user_management.group_management.list_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
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
    /// Validates the group name, computes changed fields, filters permissions
    /// by delegation, and sends the `GroupUpdate` message with only changed fields.
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

        let (id, original_name, new_name, is_shared, permissions) =
            match &conn.user_management.group_management.mode {
                GroupManagementMode::Edit {
                    id,
                    original_name,
                    new_name,
                    is_shared,
                    permissions,
                    ..
                } => (
                    *id,
                    original_name.clone(),
                    new_name.clone(),
                    *is_shared,
                    permissions.clone(),
                ),
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

        // Only send name if it changed
        let requested_name = if new_name != original_name {
            Some(new_name)
        } else {
            None
        };

        // Only send permissions that the current user has (non-admin delegation)
        let requested_permissions: Vec<String> = permissions
            .iter()
            .filter(|(perm_name, perm_enabled)| *perm_enabled && conn.has_permission(perm_name))
            .map(|(name, _)| name.clone())
            .collect();

        // Only send is_shared when it could have been toggled (member_count == 0).
        // When member_count > 0, the UI disables the checkbox so the value is unchanged.
        let requested_is_shared = match &conn.user_management.group_management.mode {
            GroupManagementMode::Edit { member_count, .. } => {
                if *member_count > 0 {
                    None
                } else {
                    Some(is_shared)
                }
            }
            _ => None,
        };

        let msg = ClientMessage::GroupUpdate {
            id,
            name: requested_name,
            is_shared: requested_is_shared,
            permissions: Some(requested_permissions),
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
}

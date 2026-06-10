//! Group management panel state

use nexus_common::ALL_PERMISSIONS;

use super::users::DEFAULT_USER_PERMISSIONS;

// =============================================================================
// Group Management State
// =============================================================================

/// Group management panel mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GroupManagementMode {
    /// Showing list of all groups
    #[default]
    List,
    /// Creating a new group
    Create,
    /// Editing an existing group
    Edit {
        /// Group database ID
        id: i64,
        /// Original group name (for display)
        original_name: String,
        /// New group name (editable field)
        new_name: String,
        /// Whether this group is for shared accounts
        is_shared: bool,
        /// Original shared-account flag at time of edit.
        original_is_shared: bool,
        /// Number of members (determines if is_shared can be toggled)
        member_count: u32,
        /// Group permissions (editable)
        permissions: Vec<(String, bool)>,
        /// Original permissions at time of edit.
        original_permissions: Vec<(String, bool)>,
        /// Bandwidth weight (subject to the delegation rule on the server:
        /// non-admins can set it only at or below their own resolved
        /// weight; admins bypass).
        bandwidth_weight: u16,
        /// Original bandwidth weight as loaded from the server. Used to
        /// detect whether the user changed the value so the update message
        /// only carries the field when it differs.
        original_bandwidth_weight: u16,
    },
    /// Confirming deletion of a group
    ConfirmDelete {
        /// Group ID to delete
        id: i64,
        /// Group name (for display in confirmation dialog)
        name: String,
    },
}

/// Sparse `GroupUpdate` payload computed from an edit baseline.
pub struct GroupUpdateFields {
    pub id: i64,
    pub name: Option<String>,
    pub is_shared: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub bandwidth_weight: Option<u16>,
}

impl GroupManagementMode {
    /// Returns true when the edit form contains at least one field the
    /// client would send in a `GroupUpdate`.
    pub fn has_effective_group_update_changes(&self) -> bool {
        let Self::Edit {
            original_name,
            new_name,
            is_shared,
            original_is_shared,
            member_count,
            permissions,
            original_permissions,
            bandwidth_weight,
            original_bandwidth_weight,
            ..
        } = self
        else {
            return false;
        };

        new_name != original_name
            || (member_count == &0 && is_shared != original_is_shared)
            || permissions != original_permissions
            || bandwidth_weight != original_bandwidth_weight
    }

    /// Build a sparse update payload. The caller supplies permission
    /// delegation rules so panel state stays independent from connections.
    pub fn group_update_fields<F>(
        &self,
        mut can_delegate_permission: F,
    ) -> Option<GroupUpdateFields>
    where
        F: FnMut(&str) -> bool,
    {
        let Self::Edit {
            id,
            original_name,
            new_name,
            is_shared,
            original_is_shared,
            member_count,
            permissions,
            original_permissions,
            bandwidth_weight,
            original_bandwidth_weight,
            ..
        } = self
        else {
            return None;
        };

        let fields = GroupUpdateFields {
            id: *id,
            name: (new_name != original_name).then_some(new_name.clone()),
            is_shared: (*member_count == 0 && is_shared != original_is_shared)
                .then_some(*is_shared),
            permissions: (permissions != original_permissions).then(|| {
                permissions
                    .iter()
                    .filter(|(perm_name, enabled)| *enabled && can_delegate_permission(perm_name))
                    .map(|(name, _)| name.clone())
                    .collect()
            }),
            bandwidth_weight: (bandwidth_weight != original_bandwidth_weight)
                .then_some(*bandwidth_weight),
        };

        (fields.name.is_some()
            || fields.is_shared.is_some()
            || fields.permissions.is_some()
            || fields.bandwidth_weight.is_some())
        .then_some(fields)
    }
}

/// Sort column for the Groups table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum GroupManagementSortColumn {
    /// Sort by group name (default)
    #[default]
    Name,
    /// Sort by member count
    Members,
}

/// Group management state (per-connection, inside UserManagementState)
#[derive(Debug, Clone)]
pub struct GroupManagementState {
    /// Current mode (list, create, edit, confirm delete)
    pub mode: GroupManagementMode,
    /// Group name for create form
    pub name: String,
    /// Is shared flag for create form
    pub is_shared: bool,
    /// Permissions for create form
    pub permissions: Vec<(String, bool)>,
    /// Bandwidth weight for create form (default 1).
    pub bandwidth_weight: u16,
    /// Error message for create form
    pub create_error: Option<String>,
    /// Error message for edit form
    pub edit_error: Option<String>,
    /// Panel-level action error from the group side (e.g., `GroupEdit` fetch
    /// failed when clicking Edit on a row). Displayed as a banner above the
    /// tabs in the user-management panel — see the parent
    /// `UserManagementState.list_error` field for the symmetric user-side
    /// error and the mutual-exclusion invariant. Write via
    /// `UserManagementState::set_group_list_error` to maintain the invariant.
    pub list_error: Option<String>,
    /// Error message for delete confirmation
    pub delete_error: Option<String>,
    /// Current sort column for the groups table
    pub sort_column: GroupManagementSortColumn,
    /// Whether sorting is ascending (true) or descending (false)
    pub sort_ascending: bool,
    /// Whether a create or update request is in flight (shared; only one mode active at a time)
    pub is_submitting: bool,
    /// Whether a delete request is in flight (separate; delete modal can overlap with list mode)
    pub is_delete_submitting: bool,
}

impl Default for GroupManagementState {
    fn default() -> Self {
        Self {
            mode: GroupManagementMode::List,
            name: String::new(),
            is_shared: false,
            permissions: ALL_PERMISSIONS
                .iter()
                .map(|s| (s.to_string(), DEFAULT_USER_PERMISSIONS.contains(s)))
                .collect(),
            bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
            create_error: None,
            edit_error: None,
            list_error: None,
            delete_error: None,
            sort_column: GroupManagementSortColumn::default(),
            sort_ascending: true,
            is_submitting: false,
            is_delete_submitting: false,
        }
    }
}

impl GroupManagementState {
    /// Reset to list mode and clear all form state
    pub fn reset_to_list(&mut self) {
        self.mode = GroupManagementMode::List;
        self.clear_create_form();
        self.edit_error = None;
        self.list_error = None;
        self.is_submitting = false;
        self.is_delete_submitting = false;
    }

    /// Clear the create form fields
    pub fn clear_create_form(&mut self) {
        self.name.clear();
        self.is_shared = false;
        for (perm_name, enabled) in &mut self.permissions {
            *enabled = DEFAULT_USER_PERMISSIONS.contains(&perm_name.as_str());
        }
        self.bandwidth_weight = nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT;
        self.create_error = None;
        self.is_submitting = false;
    }

    /// Enter create mode
    pub fn enter_create_mode(&mut self) {
        self.clear_create_form();
        self.mode = GroupManagementMode::Create;
    }

    /// Enter edit mode for a group (with pre-populated values from server)
    pub fn enter_edit_mode(
        &mut self,
        id: i64,
        name: String,
        is_shared: bool,
        member_count: u32,
        permissions: Vec<String>,
        bandwidth_weight: u16,
    ) {
        let mut perm_map: Vec<(String, bool)> = ALL_PERMISSIONS
            .iter()
            .map(|s| (s.to_string(), false))
            .collect();

        for (perm_name, perm_enabled) in &mut perm_map {
            *perm_enabled = permissions.contains(perm_name);
        }

        self.mode = GroupManagementMode::Edit {
            id,
            original_name: name.clone(),
            new_name: name,
            is_shared,
            original_is_shared: is_shared,
            member_count,
            permissions: perm_map.clone(),
            original_permissions: perm_map,
            bandwidth_weight,
            original_bandwidth_weight: bandwidth_weight,
        };
        self.edit_error = None;
    }

    /// Enter confirm delete mode for a group
    pub fn enter_confirm_delete_mode(&mut self, id: i64, name: String) {
        self.mode = GroupManagementMode::ConfirmDelete { id, name };
        self.delete_error = None;
        self.is_delete_submitting = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_state() -> GroupManagementState {
        let mut state = GroupManagementState::default();
        state.enter_edit_mode(
            7,
            "Staff".to_string(),
            false,
            0,
            vec!["chat_send".to_string()],
            nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
        );
        state
    }

    #[test]
    fn edit_mode_has_no_effective_update_changes_initially() {
        let state = edit_state();

        assert!(!state.mode.has_effective_group_update_changes());
    }

    #[test]
    fn edit_mode_detects_effective_update_changes() {
        let mut state = edit_state();
        if let GroupManagementMode::Edit { new_name, .. } = &mut state.mode {
            *new_name = "Staff 2".to_string();
        }

        assert!(state.mode.has_effective_group_update_changes());
    }

    #[test]
    fn edit_mode_toggle_back_is_not_an_effective_update() {
        let mut state = edit_state();
        if let GroupManagementMode::Edit { permissions, .. } = &mut state.mode
            && let Some((_, enabled)) = permissions
                .iter_mut()
                .find(|(permission, _)| permission == "chat_send")
        {
            *enabled = false;
            *enabled = true;
        }

        assert!(!state.mode.has_effective_group_update_changes());
    }
}

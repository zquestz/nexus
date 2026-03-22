//! Group management panel state

use nexus_common::ALL_PERMISSIONS;

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
        /// Number of members (determines if is_shared can be toggled)
        member_count: u32,
        /// Group permissions (editable)
        permissions: Vec<(String, bool)>,
    },
    /// Confirming deletion of a group
    ConfirmDelete {
        /// Group ID to delete
        id: i64,
        /// Group name (for display in confirmation dialog)
        name: String,
    },
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
    /// Error message for create form
    pub create_error: Option<String>,
    /// Error message for edit form
    pub edit_error: Option<String>,
    /// Error message for list view
    pub list_error: Option<String>,
    /// Error message for delete confirmation
    pub delete_error: Option<String>,
}

impl Default for GroupManagementState {
    fn default() -> Self {
        Self {
            mode: GroupManagementMode::List,
            name: String::new(),
            is_shared: false,
            permissions: ALL_PERMISSIONS
                .iter()
                .map(|s| (s.to_string(), false))
                .collect(),
            create_error: None,
            edit_error: None,
            list_error: None,
            delete_error: None,
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
    }

    /// Clear the create form fields
    pub fn clear_create_form(&mut self) {
        self.name.clear();
        self.is_shared = false;
        for (_perm_name, enabled) in &mut self.permissions {
            *enabled = false;
        }
        self.create_error = None;
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
            member_count,
            permissions: perm_map,
        };
        self.edit_error = None;
    }

    /// Enter confirm delete mode for a group
    pub fn enter_confirm_delete_mode(&mut self, id: i64, name: String) {
        self.mode = GroupManagementMode::ConfirmDelete { id, name };
        self.delete_error = None;
    }
}

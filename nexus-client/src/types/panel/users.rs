//! User management panel state

use nexus_common::ALL_PERMISSIONS;
use nexus_common::names::fold_name;
use nexus_common::protocol::{GroupInfo, UserInfo};
use nexus_common::validators::resolve_bandwidth_weight;

use super::super::ActivePanel;
use super::groups::GroupManagementState;

// =============================================================================
// User Management Tab
// =============================================================================

/// Tab selection for User Management panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum UserManagementTab {
    /// Users tab (default)
    #[default]
    Users,
    /// Groups tab
    Groups,
}

/// Sort column for the Users table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum UserManagementSortColumn {
    /// Sort by username (default)
    #[default]
    Username,
    /// Sort by group name
    Group,
}

// =============================================================================
// User Management State
// =============================================================================

/// Default permissions for new users
///
/// These permissions are enabled by default when creating a new user:
/// - `chat_receive`: Receive chat messages
/// - `chat_send`: Send chat messages
/// - `chat_topic`: View chat topic
/// - `file_info`: View file information
/// - `file_list`: Browse files and directories
/// - `news_list`: View news posts
/// - `user_info`: View user information
/// - `user_list`: View connected users list
/// - `user_message`: Send user messages
pub(crate) const DEFAULT_USER_PERMISSIONS: &[&str] = &[
    "chat_receive",
    "chat_send",
    "chat_topic",
    "file_info",
    "file_list",
    "news_list",
    "user_info",
    "user_list",
    "user_message",
];

/// User management panel mode
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UserManagementMode {
    /// Showing list of all users
    #[default]
    List,
    /// Creating a new user
    Create,
    /// Editing an existing user
    Edit {
        /// Database user ID (for the UserUpdate request)
        id: i64,
        /// Original username (for the UserUpdate request)
        original_username: String,
        /// New username (editable field, pre-filled with original)
        new_username: String,
        /// New password (optional, empty = don't change)
        new_password: String,
        /// Is admin flag (editable)
        is_admin: bool,
        /// Original admin flag at time of edit.
        original_is_admin: bool,
        /// Is shared account flag (immutable - display only)
        is_shared: bool,
        /// Enabled flag (editable)
        enabled: bool,
        /// Original enabled flag at time of edit.
        original_enabled: bool,
        /// Permissions (editable) — effective permissions (checked = on for user)
        permissions: Vec<(String, bool)>,
        /// Original effective permissions at time of edit.
        original_permissions: Vec<(String, bool)>,
        /// Original group ID at time of edit (for detecting whether remove_group is needed)
        original_group_id: Option<i64>,
        /// Assigned group ID (None = no group)
        group_id: Option<i64>,
        /// Group's base permissions (for computing inherited vs override styling).
        /// Empty when user has no group.
        group_permissions: Vec<String>,
        /// Permissions explicitly revoked from the group for this user.
        /// Empty when user has no group.
        revoked_permissions: Vec<String>,
        /// Bandwidth weight override (editable). `None` = inherit from group,
        /// `Some(w)` = individual override.
        bandwidth_weight_override: Option<u16>,
        /// "Inherit from group" checkbox state. Drives the override
        /// vs inherit-on-submit decision.
        bandwidth_weight_inherit: bool,
        /// Original bandwidth weight override from the server (used to
        /// diff against the form value so the update message only carries
        /// the field when it changed).
        original_bandwidth_weight_override: Option<u16>,
    },
    /// Confirming deletion of a user
    ConfirmDelete {
        /// Database user ID to delete
        id: i64,
        /// Username to delete (for display in confirmation dialog)
        username: String,
    },
}

/// Sparse `UserUpdate` payload computed from an edit baseline.
pub struct UserUpdateFields {
    pub id: i64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    pub enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub group_id: Option<i64>,
    pub remove_group: Option<bool>,
    pub revokes: Option<Vec<String>>,
    pub bandwidth_weight: Option<u16>,
    pub inherit_bandwidth_weight: Option<bool>,
}

impl UserManagementMode {
    /// Returns true when the edit form contains at least one field the
    /// client would send in a `UserUpdate`.
    pub fn has_effective_user_update_changes(
        &self,
        requester_username: &str,
        requester_is_admin: bool,
    ) -> bool {
        let Self::Edit {
            original_username,
            new_username,
            new_password,
            is_admin,
            original_is_admin,
            enabled,
            original_enabled,
            permissions,
            original_permissions,
            original_group_id,
            group_id,
            bandwidth_weight_override,
            bandwidth_weight_inherit,
            original_bandwidth_weight_override,
            ..
        } = self
        else {
            return false;
        };

        let is_self_edit = fold_name(original_username) == fold_name(requester_username);
        let identity_and_bandwidth_allowed = !is_self_edit || requester_is_admin;
        let original_bandwidth_inherit = original_bandwidth_weight_override.is_none();

        (identity_and_bandwidth_allowed && new_username != original_username)
            || (!is_self_edit && !new_password.is_empty())
            || (!is_self_edit && requester_is_admin && is_admin != original_is_admin)
            || (!is_self_edit && requester_is_admin && enabled != original_enabled)
            || (!is_self_edit && permissions != original_permissions)
            || (!is_self_edit && group_id != original_group_id)
            || (identity_and_bandwidth_allowed
                && (*bandwidth_weight_inherit != original_bandwidth_inherit
                    || (!*bandwidth_weight_inherit
                        && bandwidth_weight_override != original_bandwidth_weight_override)))
    }

    /// Build a sparse update payload. Permission delegation and group-weight
    /// lookup are supplied by the caller so panel state stays connection-free.
    pub fn user_update_fields<CanPerm, GroupWeight>(
        &self,
        requester_username: &str,
        requester_is_admin: bool,
        mut can_delegate_permission: CanPerm,
        mut group_weight_for: GroupWeight,
    ) -> Option<UserUpdateFields>
    where
        CanPerm: FnMut(&str) -> bool,
        GroupWeight: FnMut(i64) -> Option<u16>,
    {
        if !self.has_effective_user_update_changes(requester_username, requester_is_admin) {
            return None;
        }

        let Self::Edit {
            id,
            original_username,
            new_username,
            new_password,
            is_admin,
            original_is_admin,
            enabled,
            original_enabled,
            permissions,
            original_permissions,
            original_group_id,
            group_id,
            group_permissions,
            bandwidth_weight_override,
            bandwidth_weight_inherit,
            original_bandwidth_weight_override,
            ..
        } = self
        else {
            return None;
        };

        let is_self_edit = fold_name(original_username) == fold_name(requester_username);
        let permissions_changed = permissions != original_permissions;
        let group_changed = group_id != original_group_id;
        let send_permission_state = !is_self_edit
            && (permissions_changed || group_changed || (*original_is_admin && !*is_admin));

        let requested_permissions = if send_permission_state {
            Some(
                permissions
                    .iter()
                    .filter(|(perm_name, enabled)| *enabled && can_delegate_permission(perm_name))
                    .map(|(name, _)| name.clone())
                    .collect(),
            )
        } else {
            None
        };

        let (requested_group_id, remove_group, revokes) =
            if is_self_edit || (!group_changed && !send_permission_state) {
                (None, None, None)
            } else if let Some(gid) = group_id {
                let revoke_list: Vec<String> = group_permissions
                    .iter()
                    .filter(|gp| can_delegate_permission(gp))
                    .filter(|gp| {
                        permissions
                            .iter()
                            .any(|(p, enabled)| p.as_str() == gp.as_str() && !*enabled)
                    })
                    .cloned()
                    .collect();
                let revokes = if revoke_list.is_empty() {
                    None
                } else {
                    Some(revoke_list)
                };
                (group_changed.then_some(*gid), None, revokes)
            } else {
                let remove = if group_changed && original_group_id.is_some() {
                    Some(true)
                } else {
                    None
                };
                (None, remove, None)
            };

        let original_inherit = original_bandwidth_weight_override.is_none();
        let bandwidth_changed = *bandwidth_weight_inherit != original_inherit
            || (!*bandwidth_weight_inherit
                && bandwidth_weight_override != original_bandwidth_weight_override);
        let (bandwidth_weight, inherit_bandwidth_weight) = if !bandwidth_changed {
            (None, None)
        } else if *bandwidth_weight_inherit {
            (None, Some(true))
        } else {
            let group_weight = group_id.and_then(&mut group_weight_for);
            let baseline = resolve_bandwidth_weight(None, group_weight, *is_admin);
            let pinned = bandwidth_weight_override.unwrap_or(baseline);
            (Some(pinned), Some(false))
        };

        Some(UserUpdateFields {
            id: *id,
            username: (new_username != original_username).then_some(new_username.clone()),
            password: (!is_self_edit && !new_password.is_empty()).then_some(new_password.clone()),
            is_admin: (requester_is_admin && !is_self_edit && is_admin != original_is_admin)
                .then_some(*is_admin),
            enabled: (requester_is_admin && !is_self_edit && enabled != original_enabled)
                .then_some(*enabled),
            permissions: requested_permissions,
            group_id: requested_group_id,
            remove_group,
            revokes,
            bandwidth_weight,
            inherit_bandwidth_weight,
        })
    }
}

/// User management panel state (per-connection)
#[derive(Clone)]
pub struct UserManagementState {
    /// Currently active tab (Users or Groups)
    pub active_tab: UserManagementTab,
    /// Current user management mode (list, create, edit, confirm delete)
    pub mode: UserManagementMode,
    /// All users from database (None = not loaded, Some(Ok) = loaded, Some(Err) = error)
    pub all_users: Option<Result<Vec<UserInfo>, String>>,
    /// Available groups for dropdown and group list view.
    /// `None` = list fetch in flight, `Some(Ok(_))` = loaded (from `GroupListResponse`
    /// or as a snapshot inside `UserEditResponse`), `Some(Err(_))` = list fetch failed.
    pub available_groups: Option<Result<Vec<GroupInfo>, String>>,
    /// Group management state (for Groups tab)
    pub group_management: GroupManagementState,
    /// Panel to return to after edit (e.g., UserInfo if edit was triggered from there)
    pub return_to_panel: Option<ActivePanel>,
    /// Username for create user form
    pub username: String,
    /// Password for create user form
    pub password: String,
    /// Admin flag for create user form
    pub is_admin: bool,
    /// Shared account flag for create user form
    pub is_shared: bool,
    /// Enabled flag for create user form
    pub enabled: bool,
    /// Permissions for create user form
    pub permissions: Vec<(String, bool)>,
    /// Group ID for create user form (None = no group)
    pub create_group_id: Option<i64>,
    /// Bandwidth weight override for create user form. `None` means
    /// "inherit from group" (and `inherit_bandwidth_weight: true` is sent);
    /// `Some(w)` means an individual override is set.
    pub bandwidth_weight_override: Option<u16>,
    /// "Inherit from group" checkbox state for the create user form.
    /// When `true`, the bandwidth-weight NumberInput is disabled and the
    /// submit sends `inherit_bandwidth_weight: Some(true)`.
    pub bandwidth_weight_inherit: bool,
    /// Error message for create user form
    pub create_error: Option<String>,
    /// Error message for edit user form
    pub edit_error: Option<String>,
    /// Whether the edit form has been invalidated by a newer server-side update.
    pub edit_stale: bool,
    /// Panel-level action error from the user side (e.g., `UserEdit` fetch
    /// failed when clicking Edit on a row). Displayed as a banner above the
    /// tabs in the user-management panel. Mutually exclusive with
    /// `group_management.list_error` — write via [`set_user_list_error`] /
    /// [`set_group_list_error`] to maintain the invariant so the banner
    /// only ever shows one error at a time.
    ///
    /// [`set_user_list_error`]: UserManagementState::set_user_list_error
    /// [`set_group_list_error`]: UserManagementState::set_group_list_error
    pub list_error: Option<String>,
    /// Error message for delete confirmation dialog
    pub delete_error: Option<String>,
    /// Whether a create or update request is in flight (prevents double-submit)
    pub is_submitting: bool,
    /// Whether a delete request is in flight (prevents double-submit)
    pub is_delete_submitting: bool,
    /// Current sort column for the Users table
    pub sort_column: UserManagementSortColumn,
    /// Whether the sort is ascending
    pub sort_ascending: bool,
}

impl Default for UserManagementState {
    fn default() -> Self {
        Self {
            active_tab: UserManagementTab::Users,
            mode: UserManagementMode::List,
            all_users: None,
            available_groups: None,
            group_management: GroupManagementState::default(),
            return_to_panel: None,
            username: String::new(),
            password: String::new(),
            is_admin: false,
            is_shared: false,
            enabled: true, // Default to enabled
            permissions: ALL_PERMISSIONS
                .iter()
                .map(|s| (s.to_string(), DEFAULT_USER_PERMISSIONS.contains(s)))
                .collect(),
            create_group_id: None,
            bandwidth_weight_override: None,
            bandwidth_weight_inherit: true,
            create_error: None,
            edit_error: None,
            edit_stale: false,
            list_error: None,
            delete_error: None,
            is_submitting: false,
            is_delete_submitting: false,
            sort_column: UserManagementSortColumn::default(),
            sort_ascending: true,
        }
    }
}

impl std::fmt::Debug for UserManagementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserManagementState")
            .field("active_tab", &self.active_tab)
            .field("mode", &self.mode)
            .field("all_users", &self.all_users)
            .field(
                "available_groups",
                &self
                    .available_groups
                    .as_ref()
                    .map(|r| r.as_ref().map(|g| g.len())),
            )
            .field("group_management", &self.group_management)
            .field("return_to_panel", &self.return_to_panel)
            .field("username", &self.username)
            .field("password", &nexus_common::REDACTED)
            .field("is_admin", &self.is_admin)
            .field("is_shared", &self.is_shared)
            .field("enabled", &self.enabled)
            .field("permissions", &self.permissions)
            .field("create_group_id", &self.create_group_id)
            .field("bandwidth_weight_override", &self.bandwidth_weight_override)
            .field("bandwidth_weight_inherit", &self.bandwidth_weight_inherit)
            .field("create_error", &self.create_error)
            .field("edit_error", &self.edit_error)
            .field("edit_stale", &self.edit_stale)
            .field("list_error", &self.list_error)
            .field("delete_error", &self.delete_error)
            .field("is_submitting", &self.is_submitting)
            .field("is_delete_submitting", &self.is_delete_submitting)
            .field("sort_column", &self.sort_column)
            .field("sort_ascending", &self.sort_ascending)
            .finish()
    }
}

/// Initial state for [`UserManagementState::enter_edit_mode`]. Bundled
/// so the call site doesn't need 9 positional args.
pub struct UserEditInit {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub is_shared: bool,
    pub enabled: bool,
    /// Effective (resolved) permission set for this user.
    pub permissions: Vec<String>,
    /// Assigned group (if any).
    pub group_id: Option<i64>,
    /// Base permissions of the assigned group (empty if no group).
    pub group_permissions: Vec<String>,
    /// Permissions explicitly revoked from the group for this user.
    pub revoked_permissions: Vec<String>,
    /// Per-user bandwidth weight override from the server. `None` means
    /// the user inherits from group / server default.
    pub bandwidth_weight: Option<u16>,
}

impl UserManagementState {
    /// Reset to list mode and clear all form state
    pub fn reset_to_list(&mut self) {
        self.mode = UserManagementMode::List;
        self.clear_create_form();
        self.edit_error = None;
        self.edit_stale = false;
        self.list_error = None;
        self.return_to_panel = None;
        self.is_submitting = false;
        self.is_delete_submitting = false;
    }

    /// Clear the create user form fields
    pub fn clear_create_form(&mut self) {
        self.username.clear();
        self.password.clear();
        self.is_admin = false;
        self.is_shared = false;
        self.enabled = true; // Reset to default enabled
        for (perm_name, enabled) in &mut self.permissions {
            *enabled = DEFAULT_USER_PERMISSIONS.contains(&perm_name.as_str());
        }
        self.create_group_id = None;
        self.bandwidth_weight_override = None;
        self.bandwidth_weight_inherit = true;
        self.create_error = None;
        self.is_submitting = false;
    }

    /// Enter create mode
    pub fn enter_create_mode(&mut self) {
        self.clear_create_form();
        self.edit_stale = false;
        self.mode = UserManagementMode::Create;
    }

    /// Enter edit mode for a user (with pre-populated values from server).
    ///
    /// See [`UserEditInit`] for the field-by-field meaning.
    pub fn enter_edit_mode(&mut self, init: UserEditInit) {
        let UserEditInit {
            id,
            username,
            is_admin,
            is_shared,
            enabled,
            permissions,
            group_id,
            group_permissions,
            revoked_permissions,
            bandwidth_weight,
        } = init;
        // Convert permissions Vec<String> to Vec<(String, bool)>
        let mut perm_map: Vec<(String, bool)> = ALL_PERMISSIONS
            .iter()
            .map(|s| (s.to_string(), false))
            .collect();

        // Mark permissions that the user has (effective set)
        for (perm_name, perm_enabled) in &mut perm_map {
            *perm_enabled = permissions.contains(perm_name);
        }

        // Inherit when no individual override is set on the server.
        let inherit = bandwidth_weight.is_none();

        self.mode = UserManagementMode::Edit {
            id,
            original_username: username.clone(),
            new_username: username,
            new_password: String::new(),
            is_admin,
            original_is_admin: is_admin,
            is_shared,
            enabled,
            original_enabled: enabled,
            permissions: perm_map.clone(),
            original_permissions: perm_map,
            original_group_id: group_id,
            group_id,
            group_permissions,
            revoked_permissions,
            bandwidth_weight_override: bandwidth_weight,
            bandwidth_weight_inherit: inherit,
            original_bandwidth_weight_override: bandwidth_weight,
        };
        self.edit_error = None;
        self.edit_stale = false;
        // Start every edit with a clean submit flag (matches enter_create_mode /
        // enter_confirm_delete_mode), so a stuck flag can never disable Save.
        self.is_submitting = false;
    }

    /// Enter confirm delete mode for a user
    pub fn enter_confirm_delete_mode(&mut self, id: i64, username: String) {
        self.mode = UserManagementMode::ConfirmDelete { id, username };
        self.edit_stale = false;
        self.delete_error = None;
        self.is_delete_submitting = false;
    }

    /// Mark the active edit form stale if it is editing the updated user.
    pub fn mark_edit_stale_if_user_id(&mut self, updated_user_id: i64) -> bool {
        let is_editing_updated_user = matches!(
            &self.mode,
            UserManagementMode::Edit { id, .. } if *id == updated_user_id
        );
        if is_editing_updated_user {
            self.edit_stale = true;
            self.is_submitting = false;
        }
        is_editing_updated_user
    }

    /// Set the user-side panel error and clear any pending group-side
    /// error. The two `list_error` fields are mutually exclusive so the
    /// panel banner only ever shows one error at a time.
    pub fn set_user_list_error(&mut self, message: String) {
        self.list_error = Some(message);
        self.group_management.list_error = None;
    }

    /// Set the group-side panel error and clear any pending user-side
    /// error. See [`set_user_list_error`](Self::set_user_list_error) for
    /// the mutual-exclusion rule.
    pub fn set_group_list_error(&mut self, message: String) {
        self.group_management.list_error = Some(message);
        self.list_error = None;
    }

    /// Returns the successfully loaded group list, or `None` if the
    /// fetch is in flight or has failed. Use this anywhere that needs
    /// to read the cached groups without caring about the loading or
    /// error state.
    pub fn loaded_groups(&self) -> Option<&[GroupInfo]> {
        self.available_groups
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_init(id: i64, username: &str) -> UserEditInit {
        UserEditInit {
            id,
            username: username.to_string(),
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: Vec::new(),
            group_id: None,
            group_permissions: Vec::new(),
            revoked_permissions: Vec::new(),
            bandwidth_weight: None,
        }
    }

    fn edit_mode_mut(state: &mut UserManagementState) -> &mut UserManagementMode {
        &mut state.mode
    }

    #[test]
    fn edit_mode_has_no_effective_update_changes_initially() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));

        assert!(!state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_detects_effective_update_changes() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        if let UserManagementMode::Edit { new_username, .. } = edit_mode_mut(&mut state) {
            *new_username = "alice2".to_string();
        }

        assert!(state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_detects_whitespace_only_password_change() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        if let UserManagementMode::Edit { new_password, .. } = edit_mode_mut(&mut state) {
            *new_password = "   ".to_string();
        }

        // A whitespace-only password is a real change (only an exactly-empty
        // field means "no change"); the strength validator judges it on submit.
        assert!(state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_ignores_hidden_bandwidth_override_while_inheriting() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        if let UserManagementMode::Edit {
            bandwidth_weight_override,
            bandwidth_weight_inherit,
            ..
        } = edit_mode_mut(&mut state)
        {
            *bandwidth_weight_override = Some(25);
            *bandwidth_weight_inherit = true;
        }

        assert!(!state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_detects_bandwidth_inherit_toggle() {
        let mut init = edit_init(42, "alice");
        init.bandwidth_weight = Some(10);

        let mut state = UserManagementState::default();
        state.enter_edit_mode(init);
        if let UserManagementMode::Edit {
            bandwidth_weight_override,
            bandwidth_weight_inherit,
            ..
        } = edit_mode_mut(&mut state)
        {
            *bandwidth_weight_override = None;
            *bandwidth_weight_inherit = true;
        }

        assert!(state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_detects_permission_changes() {
        let mut init = edit_init(42, "alice");
        init.permissions = vec!["chat_send".to_string()];

        let mut state = UserManagementState::default();
        state.enter_edit_mode(init);
        if let UserManagementMode::Edit { permissions, .. } = edit_mode_mut(&mut state)
            && let Some((_, enabled)) = permissions
                .iter_mut()
                .find(|(permission, _)| permission == "chat_send")
        {
            *enabled = false;
        }

        assert!(state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_detects_group_changes() {
        let mut init = edit_init(42, "alice");
        init.group_id = Some(1);

        let mut state = UserManagementState::default();
        state.enter_edit_mode(init);
        if let UserManagementMode::Edit { group_id, .. } = edit_mode_mut(&mut state) {
            *group_id = Some(2);
        }

        assert!(state.mode.has_effective_user_update_changes("admin", true));
    }

    #[test]
    fn edit_mode_ignores_self_edit_fields_that_submit_strips() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        if let UserManagementMode::Edit { enabled, .. } = edit_mode_mut(&mut state) {
            *enabled = false;
        }

        assert!(!state.mode.has_effective_user_update_changes("alice", true));
    }

    #[test]
    fn user_update_fields_strip_self_edit_password() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        if let UserManagementMode::Edit {
            new_username,
            new_password,
            ..
        } = edit_mode_mut(&mut state)
        {
            *new_username = "alice2".to_string();
            *new_password = "new-password".to_string();
        }

        let fields = state
            .mode
            .user_update_fields("alice", true, |_| false, |_| None)
            .expect("admin self-rename is an effective update");

        assert_eq!(fields.username.as_deref(), Some("alice2"));
        assert!(fields.password.is_none());
    }

    #[test]
    fn mark_edit_stale_if_user_id_invalidates_active_edit_only() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        state.edit_error = Some("username is required".to_string());
        state.is_submitting = true;

        assert!(!state.mark_edit_stale_if_user_id(7));
        assert!(!state.edit_stale);
        assert!(state.is_submitting);

        assert!(state.mark_edit_stale_if_user_id(42));
        assert!(state.edit_stale);
        assert!(!state.is_submitting);
        assert_eq!(state.edit_error.as_deref(), Some("username is required"));
    }

    #[test]
    fn mode_transitions_clear_edit_stale() {
        let mut state = UserManagementState::default();
        state.enter_edit_mode(edit_init(42, "alice"));
        assert!(state.mark_edit_stale_if_user_id(42));

        state.enter_edit_mode(edit_init(42, "alice"));
        assert!(!state.edit_stale);
        assert!(state.mark_edit_stale_if_user_id(42));

        state.enter_create_mode();
        assert!(!state.edit_stale);
        assert!(!state.mark_edit_stale_if_user_id(42));

        state.enter_edit_mode(edit_init(42, "alice"));
        assert!(state.mark_edit_stale_if_user_id(42));
        state.reset_to_list();
        assert!(!state.edit_stale);
        assert!(!state.mark_edit_stale_if_user_id(42));
    }
}

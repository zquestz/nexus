//! User management panel state

use nexus_common::ALL_PERMISSIONS;
use nexus_common::protocol::{GroupInfo, UserInfo};

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
        /// Is shared account flag (immutable - display only)
        is_shared: bool,
        /// Enabled flag (editable)
        enabled: bool,
        /// Permissions (editable) — effective permissions (checked = on for user)
        permissions: Vec<(String, bool)>,
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
            .field("password", &"[REDACTED]")
            .field("is_admin", &self.is_admin)
            .field("is_shared", &self.is_shared)
            .field("enabled", &self.enabled)
            .field("permissions", &self.permissions)
            .field("create_group_id", &self.create_group_id)
            .field("bandwidth_weight_override", &self.bandwidth_weight_override)
            .field("bandwidth_weight_inherit", &self.bandwidth_weight_inherit)
            .field("create_error", &self.create_error)
            .field("edit_error", &self.edit_error)
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
            is_shared,
            enabled,
            permissions: perm_map,
            original_group_id: group_id,
            group_id,
            group_permissions,
            revoked_permissions,
            bandwidth_weight_override: bandwidth_weight,
            bandwidth_weight_inherit: inherit,
            original_bandwidth_weight_override: bandwidth_weight,
        };
        self.edit_error = None;
        // Start every edit with a clean submit flag (matches enter_create_mode /
        // enter_confirm_delete_mode), so a stuck flag can never disable Save.
        self.is_submitting = false;
    }

    /// Enter confirm delete mode for a user
    pub fn enter_confirm_delete_mode(&mut self, id: i64, username: String) {
        self.mode = UserManagementMode::ConfirmDelete { id, username };
        self.delete_error = None;
        self.is_delete_submitting = false;
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

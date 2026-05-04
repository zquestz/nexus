//! Tracker management panel state (Trackers tab inside the Server Info panel).
//!
//! Mirrors the User / Group management state shape: a single `mode` field
//! describes whether the tab body is showing the list, the Add subview, the
//! Edit subview, the Remove confirmation modal, or the Accept Fingerprint
//! modal. The list cache, sort settings, and panel-level action error live
//! alongside the mode so the panel-level banner survives mode transitions.

use nexus_common::DEFAULT_TRACKER_PORT;
use nexus_common::protocol::TrackerInfo;

// =============================================================================
// Sort Column
// =============================================================================

/// Sort column for the Trackers table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum TrackerManagementSortColumn {
    /// Sort by tracker name (default).
    #[default]
    Name,
    /// Sort by address.
    Address,
    /// Sort by status (connected → pending → disconnected).
    Status,
}

// =============================================================================
// Tracker Management Mode
// =============================================================================

/// Current mode of the Trackers tab.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TrackerManagementMode {
    /// Showing the sortable tracker list.
    #[default]
    List,
    /// Adding a new tracker (full-panel form).
    Add,
    /// Editing an existing tracker (full-panel form).
    Edit {
        /// Database tracker ID being edited.
        id: i64,
        /// Original name (immutable, used for the form subtitle).
        original_name: String,
        /// Most recent operational error reported by the server, surfaced
        /// in the form banner so admins see context while editing.
        last_error: Option<String>,
        /// Editable name field.
        name: String,
        /// Editable address field.
        address: String,
        /// Editable port field.
        port: u16,
        /// Editable fingerprint field (empty = unpinned).
        fingerprint: String,
        /// Editable password field (empty = open tracker).
        password: String,
        /// Editable enabled flag.
        enabled: bool,
    },
    /// Confirming removal of a tracker.
    ConfirmRemove {
        /// Database tracker ID to remove.
        id: i64,
        /// Tracker name (for display in the confirmation dialog).
        name: String,
    },
    /// Reviewing a pending Stage-1 fingerprint observation. Shown when the
    /// admin clicks "Accept Fingerprint" on a row with
    /// `pending_fingerprint.is_some()`.
    AcceptFingerprint {
        /// Database tracker ID.
        id: i64,
        /// Tracker name for the dialog identity line.
        name: String,
        /// Address for the dialog identity line.
        address: String,
        /// Port for the dialog identity line.
        port: u16,
        /// Currently pinned fingerprint (`None` if unpinned at the time the
        /// observation arrived). Rendered as the "expected" half.
        expected: Option<String>,
        /// Newly observed fingerprint (the value to be promoted).
        received: String,
    },
}

// =============================================================================
// Tracker Management State
// =============================================================================

/// Per-connection state for the Trackers tab.
///
/// The Add-form fields live as flat scalars on the struct rather than as
/// payload on the `Add` mode variant — this keeps the borrow shape simple
/// for handlers that mutate one field at a time (matching the User Create
/// form's layout in `UserManagementState`). The fields are reset to
/// defaults each time `enter_add_mode()` runs, so a Cancel-and-reopen
/// starts the form fresh; any user input is discarded by the cancel.
#[derive(Debug, Clone)]
pub struct TrackerManagementState {
    /// Current mode (list, add, edit, confirm remove, accept fingerprint).
    pub mode: TrackerManagementMode,
    /// All trackers from the server. `None` while a fetch is in flight,
    /// `Some(Ok(_))` on successful load, `Some(Err(_))` after a failed
    /// fetch (so the Refresh button re-enables for retry).
    pub all_trackers: Option<Result<Vec<TrackerInfo>, String>>,
    /// Panel-level action error displayed as a banner above the tabs.
    /// Used for non-form errors: `TrackerEdit` fetch failure (form can't
    /// open) and `TrackerList` fetch send failure when refreshing.
    pub list_error: Option<String>,
    /// Form error for the Add/Edit subview.
    pub form_error: Option<String>,
    /// Error displayed in the Remove confirmation modal.
    pub remove_error: Option<String>,
    /// Error displayed in the Accept Fingerprint dialog.
    pub accept_fingerprint_error: Option<String>,
    /// Whether an Add or Edit submit is in flight (prevents double-submit).
    pub is_submitting: bool,
    /// Whether a Remove submit is in flight (separate so the modal can
    /// overlap with editing).
    pub is_remove_submitting: bool,
    /// Whether an Accept Fingerprint submit is in flight.
    pub is_accept_fingerprint_submitting: bool,
    /// Current sort column for the trackers table.
    pub sort_column: TrackerManagementSortColumn,
    /// Whether the sort is ascending (true) or descending (false).
    pub sort_ascending: bool,

    // ---- Add-form fields (persist across reopen) ----
    /// Add form: tracker name field.
    pub add_name: String,
    /// Add form: address field.
    pub add_address: String,
    /// Add form: port field.
    pub add_port: u16,
    /// Add form: fingerprint field (empty = unpinned).
    pub add_fingerprint: String,
    /// Add form: registration password field (empty = open tracker).
    pub add_password: String,
    /// Add form: enabled flag (default true).
    pub add_enabled: bool,
}

impl Default for TrackerManagementState {
    fn default() -> Self {
        Self {
            mode: TrackerManagementMode::List,
            all_trackers: None,
            list_error: None,
            form_error: None,
            remove_error: None,
            accept_fingerprint_error: None,
            is_submitting: false,
            is_remove_submitting: false,
            is_accept_fingerprint_submitting: false,
            sort_column: TrackerManagementSortColumn::default(),
            sort_ascending: true,
            add_name: String::new(),
            add_address: String::new(),
            add_port: DEFAULT_TRACKER_PORT,
            add_fingerprint: String::new(),
            add_password: String::new(),
            add_enabled: true,
        }
    }
}

/// Initial values for [`TrackerManagementState::enter_edit_mode`]. Bundled
/// to avoid a long positional argument list at the call site.
pub struct TrackerEditInit {
    pub id: i64,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub enabled: bool,
    pub last_error: Option<String>,
}

impl TrackerManagementState {
    /// Reset the panel to the list view and clear all form/modal state.
    pub fn reset_to_list(&mut self) {
        self.mode = TrackerManagementMode::List;
        self.form_error = None;
        self.remove_error = None;
        self.accept_fingerprint_error = None;
        self.list_error = None;
        self.is_submitting = false;
        self.is_remove_submitting = false;
        self.is_accept_fingerprint_submitting = false;
    }

    /// Clear the Add form fields back to defaults.
    pub fn clear_add_form(&mut self) {
        self.add_name.clear();
        self.add_address.clear();
        self.add_port = DEFAULT_TRACKER_PORT;
        self.add_fingerprint.clear();
        self.add_password.clear();
        self.add_enabled = true;
        self.form_error = None;
        self.is_submitting = false;
    }

    /// Enter the Add subview, clearing any prior add-form state.
    pub fn enter_add_mode(&mut self) {
        self.clear_add_form();
        self.mode = TrackerManagementMode::Add;
    }

    /// Enter the Edit subview pre-populated from a `TrackerEditResponse`.
    pub fn enter_edit_mode(&mut self, init: TrackerEditInit) {
        self.mode = TrackerManagementMode::Edit {
            id: init.id,
            original_name: init.name.clone(),
            last_error: init.last_error,
            name: init.name,
            address: init.address,
            port: init.port,
            fingerprint: init.fingerprint.unwrap_or_default(),
            password: init.password.unwrap_or_default(),
            enabled: init.enabled,
        };
        self.form_error = None;
        self.is_submitting = false;
    }

    /// Enter the Remove confirmation modal.
    pub fn enter_confirm_remove_mode(&mut self, id: i64, name: String) {
        self.mode = TrackerManagementMode::ConfirmRemove { id, name };
        self.remove_error = None;
        self.is_remove_submitting = false;
    }

    /// Enter the Accept Fingerprint dialog with the values from the
    /// affected tracker row.
    pub fn enter_accept_fingerprint_mode(
        &mut self,
        id: i64,
        name: String,
        address: String,
        port: u16,
        expected: Option<String>,
        received: String,
    ) {
        self.mode = TrackerManagementMode::AcceptFingerprint {
            id,
            name,
            address,
            port,
            expected,
            received,
        };
        self.accept_fingerprint_error = None;
        self.is_accept_fingerprint_submitting = false;
    }

    /// Returns the cached tracker list on success, or `None` while loading
    /// or after a failed fetch.
    #[allow(dead_code)] // Public accessor reserved for future call sites.
    pub fn loaded_trackers(&self) -> Option<&[TrackerInfo]> {
        self.all_trackers
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(Vec::as_slice)
    }
}

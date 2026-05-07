//! Tracker discovery panel state.
//!
//! Lives globally on `NexusApp` (not per-connection) — the discovery
//! panel is always visible regardless of connection state, and every
//! user manages their own client tracker list.
//!
//! State shape mirrors the User/Group/Server-side-Tracker management
//! panels in spirit: a single `mode` describes whether the body shows
//! the list, an Add/Edit form, the Remove confirmation, or the
//! Stage-1 AcceptFingerprint dialog. The list-area cache and
//! sort settings live alongside the mode so they survive mode
//! transitions and panel close/reopen within the session.

use std::collections::HashMap;

use nexus_common::DEFAULT_TRACKER_PORT;
use nexus_common::tracker_protocol::ServerEntry;
use uuid::Uuid;

// =============================================================================
// Sort Column
// =============================================================================

/// Sort column for the discovered-servers table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum TrackerBrowserSortColumn {
    /// Sort by server name (default — matches wire order from the tracker).
    #[default]
    Name,
    /// Sort by description.
    Description,
    /// Sort by reported user count.
    Users,
}

// =============================================================================
// Cache
// =============================================================================

/// Per-tracker fetch state. Survives mode transitions and panel
/// close/reopen within the session; cleared on app exit (in-memory only).
///
/// `entries` is the most recent successful fetch and is preserved across
/// failed refreshes — that keeps the table visible even when a refresh
/// errors out. `error` is the most recent fetch error (already
/// localized); cleared on next successful fetch. `is_fetching` is true
/// while a query is in flight; gates Refresh-button enablement so
/// repeated clicks don't pile up parallel queries.
#[derive(Debug, Clone, Default)]
pub struct TrackerCacheEntry {
    /// Last successful list of advertised servers from this tracker.
    pub entries: Option<Vec<ServerEntry>>,
    /// Last fetch error (already localized, ready to render).
    pub error: Option<String>,
    /// Whether a query is currently in flight for this tracker.
    pub is_fetching: bool,
}

// =============================================================================
// Mode
// =============================================================================

/// Current mode of the tracker discovery panel.
#[derive(Clone, PartialEq, Default)]
pub enum TrackerBrowserMode {
    /// Showing the toolbar + search + sortable server list.
    #[default]
    List,
    /// Adding a new client tracker (full-panel form).
    Add,
    /// Editing an existing client tracker (full-panel form).
    Edit {
        /// Stable client-tracker id being edited (the row's `Uuid`).
        id: Uuid,
        /// Display name captured at form-open; immutable for the life
        /// of the Edit session. Used for the form subtitle so the
        /// user always sees which row they came in to edit, even
        /// after they've started changing `name`.
        original_name: String,
        /// Editable name field. May diverge from `original_name` as
        /// the user types; the form submit replaces the row's name
        /// with this value.
        name: String,
        /// Editable address field.
        address: String,
        /// Editable port field.
        port: u16,
        /// Editable registration password (empty = open tracker).
        password: String,
        /// Editable fingerprint pin (empty = unpinned).
        fingerprint: String,
    },
    /// Confirming removal of a configured tracker.
    ConfirmRemove {
        /// Tracker id to remove on confirmation.
        id: Uuid,
        /// Display name shown in the modal copy.
        name: String,
    },
    /// Stage-1 fingerprint mismatch reviewing dialog. Shown when a
    /// query observes a fingerprint that differs from the row's pin.
    /// Step 6 wires this; the variant is structurally part of the
    /// enum now so handlers and views can pattern-match exhaustively.
    #[allow(dead_code)] // Constructed by the network layer in step 6.
    AcceptFingerprint {
        /// Tracker id whose pin will be promoted on accept.
        id: Uuid,
        /// Display name for the dialog identity line.
        name: String,
        /// Address for the dialog identity line.
        address: String,
        /// Port for the dialog identity line.
        port: u16,
        /// Currently pinned fingerprint (`None` if unpinned at the
        /// time the observation arrived). Rendered as the "expected"
        /// half.
        expected: Option<String>,
        /// Newly observed fingerprint (the value to be promoted).
        received: String,
    },
}

impl std::fmt::Debug for TrackerBrowserMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List => f.write_str("List"),
            Self::Add => f.write_str("Add"),
            Self::Edit {
                id,
                original_name,
                name,
                address,
                port,
                password: _,
                fingerprint,
            } => f
                .debug_struct("Edit")
                .field("id", id)
                .field("original_name", original_name)
                .field("name", name)
                .field("address", address)
                .field("port", port)
                .field("password", &"[REDACTED]")
                .field("fingerprint", fingerprint)
                .finish(),
            Self::ConfirmRemove { id, name } => f
                .debug_struct("ConfirmRemove")
                .field("id", id)
                .field("name", name)
                .finish(),
            Self::AcceptFingerprint {
                id,
                name,
                address,
                port,
                expected,
                received,
            } => f
                .debug_struct("AcceptFingerprint")
                .field("id", id)
                .field("name", name)
                .field("address", address)
                .field("port", port)
                .field("expected", expected)
                .field("received", received)
                .finish(),
        }
    }
}

// =============================================================================
// State
// =============================================================================

/// Bundled initial values for [`TrackerBrowserState::enter_edit_mode`].
/// Matches the shape used by the BBS-admin tracker panel for consistency.
pub struct TrackerBrowserEditInit {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub password: Option<String>,
    pub fingerprint: Option<String>,
}

/// Top-level state for the tracker discovery panel.
///
/// Add-form fields live as flat scalars on the struct rather than as
/// payload on the `Add` mode variant — this keeps the borrow shape
/// simple for handlers that mutate one field at a time. They reset
/// to defaults each time `enter_add_mode()` runs, so a Cancel + reopen
/// starts the form fresh.
#[derive(Clone)]
pub struct TrackerBrowserState {
    /// Current mode (list / add / edit / confirm remove / accept fingerprint).
    pub mode: TrackerBrowserMode,
    /// Currently selected tracker (drives the dropdown + which cache
    /// entry the list area renders). `None` until the user makes a
    /// selection (or step 5+ auto-selects the alphabetically-first
    /// configured tracker on panel open).
    pub selected_tracker: Option<Uuid>,
    /// Per-tracker fetch cache, keyed by `ClientTracker.id`.
    pub cache: HashMap<Uuid, TrackerCacheEntry>,
    /// Live-filter input for the search row. Resets when the selected
    /// tracker changes (step 5 wiring).
    pub search_input: String,
    /// Active sort column for the server table.
    pub sort_column: TrackerBrowserSortColumn,
    /// Whether sort is ascending (true) or descending (false).
    pub sort_ascending: bool,

    /// Form-level error displayed inside Add/Edit subviews.
    pub form_error: Option<String>,
    /// Error displayed inside the Remove confirmation modal.
    pub remove_error: Option<String>,
    /// Error displayed inside the AcceptFingerprint modal. Read by the
    /// AcceptFingerprint subview that lands with the network layer;
    /// write-only for now.
    pub accept_fingerprint_error: Option<String>,
    /// Whether an Add/Edit submit is in flight (prevents double-submit).
    pub is_submitting: bool,
    /// Whether a Remove submit is in flight (separate so Remove can
    /// overlap with editing on different rows).
    pub is_remove_submitting: bool,
    /// Whether an AcceptFingerprint submit is in flight. Gated on the
    /// AcceptFingerprint dialog landing with the network layer;
    /// write-only for now.
    pub is_accept_fingerprint_submitting: bool,

    // ---- Add-form fields (persist across reopen of Add mode) ----
    /// Add form: tracker display name field.
    pub add_name: String,
    /// Add form: tracker hostname/IP field.
    pub add_address: String,
    /// Add form: tracker port field.
    pub add_port: u16,
    /// Add form: registration password field (empty = open tracker).
    pub add_password: String,
    /// Add form: fingerprint pin field (empty = unpinned).
    pub add_fingerprint: String,
}

impl Default for TrackerBrowserState {
    fn default() -> Self {
        Self {
            mode: TrackerBrowserMode::List,
            selected_tracker: None,
            cache: HashMap::new(),
            search_input: String::new(),
            sort_column: TrackerBrowserSortColumn::default(),
            sort_ascending: true,
            form_error: None,
            remove_error: None,
            accept_fingerprint_error: None,
            is_submitting: false,
            is_remove_submitting: false,
            is_accept_fingerprint_submitting: false,
            add_name: String::new(),
            add_address: String::new(),
            add_port: DEFAULT_TRACKER_PORT,
            add_password: String::new(),
            add_fingerprint: String::new(),
        }
    }
}

impl std::fmt::Debug for TrackerBrowserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackerBrowserState")
            .field("mode", &self.mode)
            .field("selected_tracker", &self.selected_tracker)
            .field("cache", &self.cache)
            .field("search_input", &self.search_input)
            .field("sort_column", &self.sort_column)
            .field("sort_ascending", &self.sort_ascending)
            .field("form_error", &self.form_error)
            .field("remove_error", &self.remove_error)
            .field("accept_fingerprint_error", &self.accept_fingerprint_error)
            .field("is_submitting", &self.is_submitting)
            .field("is_remove_submitting", &self.is_remove_submitting)
            .field(
                "is_accept_fingerprint_submitting",
                &self.is_accept_fingerprint_submitting,
            )
            .field("add_name", &self.add_name)
            .field("add_address", &self.add_address)
            .field("add_port", &self.add_port)
            .field("add_password", &"[REDACTED]")
            .field("add_fingerprint", &self.add_fingerprint)
            .finish()
    }
}

impl TrackerBrowserState {
    /// Reset to the list view and clear all form/modal state. Used by
    /// every Cancel path.
    pub fn reset_to_list(&mut self) {
        self.mode = TrackerBrowserMode::List;
        self.form_error = None;
        self.remove_error = None;
        self.accept_fingerprint_error = None;
        self.is_submitting = false;
        self.is_remove_submitting = false;
        self.is_accept_fingerprint_submitting = false;
    }

    /// Clear the Add form fields back to defaults.
    pub fn clear_add_form(&mut self) {
        self.add_name.clear();
        self.add_address.clear();
        self.add_port = DEFAULT_TRACKER_PORT;
        self.add_password.clear();
        self.add_fingerprint.clear();
        self.form_error = None;
        self.is_submitting = false;
    }

    /// Enter the Add subview, clearing any prior add-form state.
    pub fn enter_add_mode(&mut self) {
        self.clear_add_form();
        self.mode = TrackerBrowserMode::Add;
    }

    /// Enter the Edit subview pre-populated from the configured row.
    pub fn enter_edit_mode(&mut self, init: TrackerBrowserEditInit) {
        self.mode = TrackerBrowserMode::Edit {
            id: init.id,
            original_name: init.name.clone(),
            name: init.name,
            address: init.address,
            port: init.port,
            password: init.password.unwrap_or_default(),
            fingerprint: init.fingerprint.unwrap_or_default(),
        };
        self.form_error = None;
        self.is_submitting = false;
    }

    /// Enter the Remove confirmation modal.
    pub fn enter_confirm_remove_mode(&mut self, id: Uuid, name: String) {
        self.mode = TrackerBrowserMode::ConfirmRemove { id, name };
        self.remove_error = None;
        self.is_remove_submitting = false;
    }

    /// Enter the AcceptFingerprint modal with the values the network
    /// layer captured at Stage-1 mismatch time.
    #[allow(dead_code)] // Wired up by the network layer in step 6.
    pub fn enter_accept_fingerprint_mode(
        &mut self,
        id: Uuid,
        name: String,
        address: String,
        port: u16,
        expected: Option<String>,
        received: String,
    ) {
        self.mode = TrackerBrowserMode::AcceptFingerprint {
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
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_is_list_mode() {
        let s = TrackerBrowserState::default();
        assert_eq!(s.mode, TrackerBrowserMode::List);
        assert!(s.cache.is_empty());
        assert!(s.selected_tracker.is_none());
        assert_eq!(s.add_port, DEFAULT_TRACKER_PORT);
    }

    #[test]
    fn test_enter_add_mode_clears_form() {
        let mut s = TrackerBrowserState {
            add_name: "stale".to_string(),
            add_password: "stale-pw".to_string(),
            form_error: Some("stale error".to_string()),
            ..TrackerBrowserState::default()
        };

        s.enter_add_mode();
        assert_eq!(s.mode, TrackerBrowserMode::Add);
        assert!(s.add_name.is_empty());
        assert!(s.add_password.is_empty());
        assert!(s.form_error.is_none());
        assert_eq!(s.add_port, DEFAULT_TRACKER_PORT);
    }

    #[test]
    fn test_reset_to_list_clears_modes_and_errors() {
        let mut s = TrackerBrowserState::default();
        s.enter_confirm_remove_mode(Uuid::new_v4(), "T".to_string());
        s.remove_error = Some("oops".to_string());
        s.is_remove_submitting = true;

        s.reset_to_list();
        assert_eq!(s.mode, TrackerBrowserMode::List);
        assert!(s.remove_error.is_none());
        assert!(!s.is_remove_submitting);
    }

    #[test]
    fn test_enter_edit_mode_populates_form() {
        let id = Uuid::new_v4();
        let init = TrackerBrowserEditInit {
            id,
            name: "T".to_string(),
            address: "tracker.example".to_string(),
            port: 7510,
            password: Some("invite".to_string()),
            fingerprint: None,
        };
        let mut s = TrackerBrowserState::default();
        s.enter_edit_mode(init);

        match &s.mode {
            TrackerBrowserMode::Edit {
                id: edit_id,
                name,
                password,
                fingerprint,
                ..
            } => {
                assert_eq!(*edit_id, id);
                assert_eq!(name, "T");
                assert_eq!(password, "invite");
                assert!(fingerprint.is_empty(), "None pin → empty form field");
            }
            other => panic!("expected Edit mode, got {:?}", other),
        }
    }

    #[test]
    fn test_enter_accept_fingerprint_mode_carries_received_pin() {
        let id = Uuid::new_v4();
        let mut s = TrackerBrowserState::default();
        s.enter_accept_fingerprint_mode(
            id,
            "T".to_string(),
            "tracker.example".to_string(),
            7510,
            Some("OLD:PIN".to_string()),
            "NEW:PIN".to_string(),
        );

        match &s.mode {
            TrackerBrowserMode::AcceptFingerprint {
                expected, received, ..
            } => {
                assert_eq!(expected.as_deref(), Some("OLD:PIN"));
                assert_eq!(received, "NEW:PIN");
            }
            other => panic!("expected AcceptFingerprint mode, got {:?}", other),
        }
    }
}

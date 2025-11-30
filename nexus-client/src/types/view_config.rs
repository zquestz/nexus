//! View configuration struct for passing state to view rendering

use crate::types::{
    ActivePanel, BookmarkEditState, ConnectionFormState, ServerBookmark, ServerConnection, UiState,
    UserManagementState,
};
use iced::Theme;
use std::collections::HashMap;

/// Configuration struct for view rendering
///
/// Holds all the state needed to render the main layout. Uses references to
/// sub-structs for cleaner organization and simpler construction.
pub struct ViewConfig<'a> {
    /// Theme for views that need concrete colors (e.g., rich_text spans)
    pub theme: Theme,

    /// Active server connections by connection_id
    pub connections: &'a HashMap<usize, ServerConnection>,

    /// Currently displayed connection
    pub active_connection: Option<usize>,

    /// Server bookmarks from config
    pub bookmarks: &'a [ServerBookmark],

    /// Per-bookmark connection errors (transient)
    pub bookmark_errors: &'a HashMap<usize, String>,

    /// Connection form state
    pub connection_form: &'a ConnectionFormState,

    /// Bookmark add/edit dialog state
    pub bookmark_edit: &'a BookmarkEditState,

    /// Chat message input (from active connection or empty)
    pub message_input: &'a str,

    /// User management state (only present when connected)
    pub user_management: Option<&'a UserManagementState>,

    /// UI panel visibility state
    pub ui_state: &'a UiState,
}

/// Toolbar state configuration
///
/// Groups all toolbar-related state to simplify passing to build_toolbar.
pub struct ToolbarState<'a> {
    pub show_bookmarks: bool,
    pub show_user_list: bool,
    pub active_panel: ActivePanel,
    pub is_connected: bool,
    pub is_admin: bool,
    pub permissions: &'a [String],
    pub can_view_user_list: bool,
}

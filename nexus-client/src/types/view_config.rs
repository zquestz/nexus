//! View configuration struct for passing state to view rendering

use crate::types::{
    ActivePanel, BookmarkEditState, ConnectionFormState, ServerBookmark, ServerConnection,
    SettingsFormState, UiState, UserManagementState,
};
use iced::Theme;
use std::collections::HashMap;

/// Configuration struct for view rendering
///
/// Holds all the state needed to render the main layout. Uses references to
/// sub-structs for cleaner organization and simpler construction.
pub struct ViewConfig<'a> {
    /// Current theme for styling
    pub theme: Theme,

    /// Show user connect/disconnect notifications in chat
    pub show_connection_notifications: bool,

    /// Font size for chat messages
    pub chat_font_size: u8,

    /// Show timestamps in chat messages
    pub show_timestamps: bool,

    /// Use 24-hour time format (false = 12-hour with AM/PM)
    pub use_24_hour_time: bool,

    /// Show seconds in timestamps
    pub show_seconds: bool,

    /// Settings form state (present when settings panel is open)
    pub settings_form: Option<&'a SettingsFormState>,

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

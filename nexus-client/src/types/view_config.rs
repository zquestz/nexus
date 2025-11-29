//! View configuration struct for passing state to view rendering

use crate::types::{BookmarkEditMode, ServerBookmark, ServerConnection, UserManagementState};
use std::collections::HashMap;

/// Configuration struct for view rendering
///
/// Holds all the state needed to render the main layout, replacing the previous
/// 25-parameter function signature with a single, well-organized struct.
pub struct ViewConfig<'a> {
    // Connection state
    pub connections: &'a HashMap<usize, ServerConnection>,
    pub active_connection: Option<usize>,

    // Bookmarks
    pub bookmarks: &'a [ServerBookmark],
    pub bookmark_edit_mode: &'a BookmarkEditMode,

    // Connection form
    pub server_name: &'a str,
    pub server_address: &'a str,
    pub port: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub connection_error: &'a Option<String>,
    pub is_connecting: bool,
    pub add_bookmark: bool,

    // Bookmark edit form
    pub bookmark_name: &'a str,
    pub bookmark_address: &'a str,
    pub bookmark_port: &'a str,
    pub bookmark_username: &'a str,
    pub bookmark_password: &'a str,
    pub bookmark_auto_connect: bool,
    pub bookmark_error: &'a Option<String>,

    // Active connection state
    pub message_input: &'a str,
    pub user_management: &'a UserManagementState,

    // UI panel visibility
    pub show_bookmarks: bool,
    pub show_user_list: bool,
    pub show_add_user: bool,
    pub show_edit_user: bool,
    pub show_broadcast: bool,
}

/// Bookmark form data for add/edit dialogs
///
/// Groups all bookmark form fields to simplify passing them to bookmark_edit_view.
pub struct BookmarkFormData<'a> {
    pub mode: &'a BookmarkEditMode,
    pub name: &'a str,
    pub address: &'a str,
    pub port: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub auto_connect: bool,
    pub error: &'a Option<String>,
}

impl<'a> ViewConfig<'a> {
    /// Extract bookmark form data from the config
    pub fn bookmark_form_data(&self) -> BookmarkFormData<'a> {
        BookmarkFormData {
            mode: self.bookmark_edit_mode,
            name: self.bookmark_name,
            address: self.bookmark_address,
            port: self.bookmark_port,
            username: self.bookmark_username,
            password: self.bookmark_password,
            auto_connect: self.bookmark_auto_connect,
            error: self.bookmark_error,
        }
    }
}

/// Toolbar state configuration
///
/// Groups all toolbar-related state to simplify passing to build_toolbar.
pub struct ToolbarState<'a> {
    pub show_bookmarks: bool,
    pub show_user_list: bool,
    pub show_broadcast: bool,
    pub show_add_user: bool,
    pub show_edit_user: bool,
    pub is_connected: bool,
    pub is_admin: bool,
    pub permissions: &'a [String],
    pub can_view_user_list: bool,
}

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

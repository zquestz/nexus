//! Server bookmark types

use super::DEFAULT_PORT;

/// Server bookmark configuration
///
/// Stores connection details for a server that can be saved and reused.
/// Supports optional username/password for quick connect and auto-connect flag.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerBookmark {
    /// Display name for the bookmark
    pub name: String,
    /// Server IPv6 address
    pub address: String,
    /// Server port number
    pub port: String,
    /// Optional username for quick connect
    pub username: String,
    /// Optional password for quick connect
    pub password: String,
    /// Whether to auto-connect on startup
    #[serde(default)]
    pub auto_connect: bool,
}

impl Default for ServerBookmark {
    fn default() -> Self {
        Self {
            name: String::new(),
            address: String::new(),
            port: DEFAULT_PORT.to_string(),
            username: String::new(),
            password: String::new(),
            auto_connect: false,
        }
    }
}

/// State for bookmark editing dialog
///
/// Wraps a ServerBookmark with an editing mode to track whether
/// we're adding a new bookmark or editing an existing one.
#[derive(Debug, Clone)]
pub struct BookmarkEditState {
    /// Current editing mode (None, Add, or Edit)
    pub mode: BookmarkEditMode,
    /// The bookmark being edited
    pub bookmark: ServerBookmark,
    /// Error message for bookmark operations
    pub error: Option<String>,
}

impl Default for BookmarkEditState {
    fn default() -> Self {
        Self {
            mode: BookmarkEditMode::None,
            bookmark: ServerBookmark::default(),
            error: None,
        }
    }
}

/// Bookmark editing mode
///
/// Tracks whether we're adding a new bookmark or editing an existing one.
#[derive(Debug, Clone, PartialEq)]
pub enum BookmarkEditMode {
    None,
    Add,
    /// Editing bookmark at this index
    Edit(usize),
}

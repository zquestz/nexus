//! Server bookmark types

use super::DEFAULT_PORT;

/// Server bookmark configuration
///
/// Stores connection details for a server that can be saved and reused.
/// Supports optional username/password for quick connect and auto-connect flag.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerBookmark {
    pub name: String,
    pub address: String,
    pub port: String,
    /// Optional username for quick connect (can be empty)
    pub username: String,
    /// Optional password for quick connect (can be empty)
    pub password: String,
    #[serde(default)]
    pub auto_connect: bool,
}

/// State for bookmark editing dialog
///
/// Manages temporary state while adding or editing a bookmark.
/// Keeps track of which bookmark is being edited (if any).
#[derive(Debug, Clone)]
pub struct BookmarkEditState {
    pub mode: BookmarkEditMode,
    pub name: String,
    pub address: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub auto_connect: bool,
}

impl Default for BookmarkEditState {
    fn default() -> Self {
        Self {
            mode: BookmarkEditMode::None,
            name: String::new(),
            address: String::new(),
            port: DEFAULT_PORT.to_string(),
            username: String::new(),
            password: String::new(),
            auto_connect: false,
        }
    }
}

impl BookmarkEditState {
    /// Clear all fields and reset to default state
    pub fn clear(&mut self) {
        self.mode = BookmarkEditMode::None;
        self.name.clear();
        self.address.clear();
        self.port = DEFAULT_PORT.to_string();
        self.username.clear();
        self.password.clear();
        self.auto_connect = false;
    }

    /// Load bookmark data into edit state
    pub fn load_from_bookmark(&mut self, mode: BookmarkEditMode, bookmark: &ServerBookmark) {
        self.mode = mode;
        self.name = bookmark.name.clone();
        self.address = bookmark.address.clone();
        self.port = bookmark.port.clone();
        self.username = bookmark.username.clone();
        self.password = bookmark.password.clone();
        self.auto_connect = bookmark.auto_connect;
    }

    /// Convert edit state to a bookmark
    pub fn to_bookmark(&self) -> ServerBookmark {
        ServerBookmark {
            name: self.name.clone(),
            address: self.address.clone(),
            port: self.port.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            auto_connect: self.auto_connect,
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

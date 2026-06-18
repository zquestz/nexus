//! Server bookmark types

use nexus_common::DEFAULT_PORT;
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::fingerprint::deserialize_normalized_fingerprint;

/// Deserialize port from either a number or a string (for backward compatibility)
fn deserialize_port<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Unexpected, Visitor};

    struct PortVisitor;

    impl<'de> Visitor<'de> for PortVisitor {
        type Value = u16;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a port number as integer or string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<u16, E>
        where
            E: de::Error,
        {
            u16::try_from(value)
                .map_err(|_| de::Error::invalid_value(Unexpected::Unsigned(value), &self))
        }

        fn visit_str<E>(self, value: &str) -> Result<u16, E>
        where
            E: de::Error,
        {
            value
                .parse()
                .map_err(|_| de::Error::invalid_value(Unexpected::Str(value), &self))
        }
    }

    deserializer.deserialize_any(PortVisitor)
}

/// Server bookmark configuration
///
/// Stores connection details for a server that can be saved and reused.
/// Supports optional username/password for quick connect and auto-connect flag.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerBookmark {
    /// Unique identifier for this bookmark
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Display name for the bookmark
    pub name: String,
    /// Server address (IPv4 or IPv6)
    pub address: String,
    /// Server port number
    #[serde(deserialize_with = "deserialize_port")]
    pub port: u16,
    /// Optional username for quick connect
    pub username: String,
    /// Optional password for quick connect
    pub password: String,
    /// Optional nickname for shared account logins
    #[serde(default)]
    pub nickname: String,
    /// Whether to auto-connect on startup
    #[serde(default)]
    pub auto_connect: bool,
    /// Certificate fingerprint (SHA-256) for Trust On First Use.
    ///
    /// Deserialized via [`deserialize_normalized_fingerprint`] so an empty
    /// value from a hand-edited config collapses to `None`.
    #[serde(default, deserialize_with = "deserialize_normalized_fingerprint")]
    pub certificate_fingerprint: Option<String>,
}

impl Default for ServerBookmark {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            address: String::new(),
            port: DEFAULT_PORT,
            username: String::new(),
            password: String::new(),
            nickname: String::new(),
            auto_connect: false,
            certificate_fingerprint: None,
        }
    }
}

impl std::fmt::Debug for ServerBookmark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerBookmark")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("nickname", &self.nickname)
            .field("auto_connect", &self.auto_connect)
            .field("certificate_fingerprint", &self.certificate_fingerprint)
            .finish()
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
    /// Whether the edit form has been invalidated by an external bookmark update.
    pub edit_stale: bool,
    /// Whether a save operation is in progress (double-submit prevention)
    pub is_submitting: bool,
    /// Whether the delete confirmation modal is shown
    pub confirm_delete: bool,
}

impl Default for BookmarkEditState {
    fn default() -> Self {
        Self {
            mode: BookmarkEditMode::None,
            bookmark: ServerBookmark::default(),
            error: None,
            edit_stale: false,
            is_submitting: false,
            confirm_delete: false,
        }
    }
}

impl BookmarkEditState {
    /// Mark the active edit form stale if it is editing the updated bookmark.
    pub fn mark_edit_stale_if_bookmark_id(&mut self, updated_bookmark_id: Uuid) -> bool {
        let is_editing_updated_bookmark = matches!(
            &self.mode,
            BookmarkEditMode::Edit(id) if *id == updated_bookmark_id
        );
        if is_editing_updated_bookmark {
            self.edit_stale = true;
            self.is_submitting = false;
        }
        is_editing_updated_bookmark
    }
}

/// Bookmark editing mode
///
/// Tracks whether we're adding a new bookmark or editing an existing one.
#[derive(Debug, Clone, PartialEq)]
pub enum BookmarkEditMode {
    None,
    Add,
    /// Editing bookmark with this ID
    Edit(Uuid),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_edit_stale_if_bookmark_id_invalidates_active_edit_only() {
        let edited_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        let mut state = BookmarkEditState {
            mode: BookmarkEditMode::Edit(edited_id),
            is_submitting: true,
            ..BookmarkEditState::default()
        };

        assert!(!state.mark_edit_stale_if_bookmark_id(other_id));
        assert!(!state.edit_stale);
        assert!(state.is_submitting);

        assert!(state.mark_edit_stale_if_bookmark_id(edited_id));
        assert!(state.edit_stale);
        assert!(!state.is_submitting);
    }

    #[test]
    fn mark_edit_stale_if_bookmark_id_ignores_add_and_none_modes() {
        let id = Uuid::new_v4();
        let mut state = BookmarkEditState {
            mode: BookmarkEditMode::Add,
            ..BookmarkEditState::default()
        };

        assert!(!state.mark_edit_stale_if_bookmark_id(id));
        assert!(!state.edit_stale);

        state.mode = BookmarkEditMode::None;
        assert!(!state.mark_edit_stale_if_bookmark_id(id));
        assert!(!state.edit_stale);
    }
}

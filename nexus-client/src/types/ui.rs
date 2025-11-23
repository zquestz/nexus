//! UI state and widget identifier types

use iced::widget::{scrollable, text_input};

/// UI visibility state for toggleable panels
///
/// Tracks which optional UI panels are currently visible.
#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub show_bookmarks: bool,
    pub show_user_list: bool,
    pub show_add_user: bool,
    pub show_delete_user: bool,
}

/// Text input IDs for focus management
///
/// Type-safe identifiers for text input widgets. Used with Iced's
/// focus system to programmatically set focus to specific inputs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputId {
    ServerName,
    ServerAddress,
    Port,
    Username,
    Password,
    BookmarkName,
    BookmarkAddress,
    BookmarkPort,
    BookmarkUsername,
    BookmarkPassword,
    AdminUsername,
    AdminPassword,
    DeleteUsername,
    ChatInput,
}

impl From<InputId> for text_input::Id {
    fn from(id: InputId) -> Self {
        text_input::Id::new(format!("{:?}", id))
    }
}

/// Scrollable area IDs
///
/// Type-safe identifiers for scrollable widgets. Used to programmatically
/// control scroll position (e.g., auto-scroll to bottom on new messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollableId {
    ChatMessages,
}

impl From<ScrollableId> for scrollable::Id {
    fn from(id: ScrollableId) -> Self {
        scrollable::Id::new(format!("{:?}", id))
    }
}

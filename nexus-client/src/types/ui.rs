//! UI state and widget identifier types

use iced::widget::{scrollable, text_input};

/// UI visibility state for toggleable panels
#[derive(Debug, Clone, Default)]
pub struct UiState {
    /// Show bookmarks sidebar
    pub show_bookmarks: bool,
    /// Show user list sidebar
    pub show_user_list: bool,
    /// Show Add User panel
    pub show_add_user: bool,
    /// Show Edit User panel
    pub show_edit_user: bool,
    /// Show Broadcast panel
    pub show_broadcast: bool,
}

/// Text input IDs for focus management
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputId {
    /// Connection form: Server name input
    ServerName,
    /// Connection form: Server address input
    ServerAddress,
    /// Connection form: Port input
    Port,
    /// Connection form: Username input
    Username,
    /// Connection form: Password input
    Password,
    /// Bookmark editor: Name input
    BookmarkName,
    /// Bookmark editor: Address input
    BookmarkAddress,
    /// Bookmark editor: Port input
    BookmarkPort,
    /// Bookmark editor: Username input
    BookmarkUsername,
    /// Bookmark editor: Password input
    BookmarkPassword,
    /// Admin panel: Username input
    AdminUsername,
    /// Admin panel: Password input
    AdminPassword,
    /// Edit user panel: Username input (stage 1)
    EditUsername,
    /// Edit user panel: New username input (stage 2)
    EditNewUsername,
    /// Edit user panel: New password input (stage 2)
    EditNewPassword,
    /// Broadcast panel: Message input
    BroadcastMessage,
    /// Chat: Message input
    ChatInput,
}

impl From<InputId> for text_input::Id {
    fn from(id: InputId) -> Self {
        text_input::Id::new(format!("{:?}", id))
    }
}

/// Scrollable area IDs for scroll position control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollableId {
    /// Chat messages scrollable area
    ChatMessages,
}

impl From<ScrollableId> for scrollable::Id {
    fn from(id: ScrollableId) -> Self {
        scrollable::Id::new(format!("{:?}", id))
    }
}

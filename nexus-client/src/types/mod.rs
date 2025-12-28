//! Type definitions for the Nexus client

mod bookmark;
pub mod connection;
mod display;
mod form;
mod message;
mod pending;
mod ui;
mod view_config;

// Re-export types for convenience
pub use bookmark::{BookmarkEditMode, BookmarkEditState, ServerBookmark};
pub use connection::{
    NetworkConnection, ServerConnection, ServerConnectionParams, TabCompletionState,
};
pub use display::{ChatMessage, ChatTab, MessageType, ScrollState, UserInfo};
pub use form::{
    ClipboardItem, ClipboardOperation, ConnectionFormState, FileSortColumn, FileTab,
    FilesManagementState, NewsManagementMode, NewsManagementState, PasswordChangeState,
    PendingOverwrite, ServerInfoEditState, SettingsFormState, SettingsTab, TabId,
    UserManagementMode, UserManagementState,
};
pub use message::Message;
pub use nexus_common::protocol::ServerMessage;
pub use pending::{PendingRequests, ResponseRouting};
pub use ui::{
    ActivePanel, FingerprintMismatch, FingerprintMismatchDetails, InputId, ScrollableId, UiState,
};
pub use view_config::{ToolbarState, ViewConfig};

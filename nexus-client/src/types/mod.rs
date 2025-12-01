//! Type definitions for the Nexus client

mod bookmark;
mod connection;
mod display;
mod form;
mod message;
mod ui;
mod view_config;

// Re-export types for convenience
pub use bookmark::{BookmarkEditMode, BookmarkEditState, ServerBookmark};
pub use connection::{NetworkConnection, ServerConnection};
pub use display::{ChatMessage, ChatTab, MessageType, ScrollState, UserInfo};
pub use form::{ConnectionFormState, UserEditState, UserManagementState};
pub use message::Message;
pub use nexus_common::protocol::ServerMessage;
pub use ui::{
    ActivePanel, FingerprintMismatch, FingerprintMismatchDetails, InputId, ScrollableId, UiState,
};
pub use view_config::{ToolbarState, ViewConfig};

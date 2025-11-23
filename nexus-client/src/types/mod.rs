//! Type definitions for the Nexus client

// Module declarations
mod bookmark;
mod connection;
mod display;
mod form;
mod message;
mod ui;

// Re-export commonly used protocol types
pub use nexus_common::protocol::ServerMessage;

// Re-export all types from submodules
pub use bookmark::{BookmarkEditMode, BookmarkEditState, ServerBookmark};
pub use connection::{NetworkConnection, ServerConnection};
pub use display::{ChatMessage, UserInfo};
pub use form::{ConnectionFormState, UserManagementState};
pub use message::Message;
pub use ui::{InputId, ScrollableId, UiState};

/// Default Nexus BBS port
pub const DEFAULT_PORT: &str = "7500";

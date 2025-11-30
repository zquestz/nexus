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
pub use display::{ChatMessage, ChatTab, UserInfo};
pub use form::{ConnectionFormState, UserEditState, UserManagementState};
pub use message::Message;
pub use nexus_common::protocol::ServerMessage;
pub use ui::{ActivePanel, FingerprintMismatch, InputId, ScrollableId, UiState};
pub use view_config::{ToolbarState, ViewConfig};

/// Default Nexus BBS port
pub const DEFAULT_PORT: &str = "7500";

/// Default locale for connections
pub const DEFAULT_LOCALE: &str = "en";

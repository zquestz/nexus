//! Type definitions for the Nexus client

mod bookmark;
mod connection;
mod display;
mod form;
mod message;
mod ui;
mod view_config;

pub use bookmark::{BookmarkEditMode, BookmarkEditState, ServerBookmark};
pub use connection::{NetworkConnection, ServerConnection};
pub use display::{ChatMessage, UserInfo};
pub use form::{ConnectionFormState, UserEditState, UserManagementState};
pub use message::Message;
pub use nexus_common::protocol::ServerMessage;
pub use ui::{InputId, ScrollableId, UiState};
pub use view_config::ViewConfig;

/// Default Nexus BBS port
pub const DEFAULT_PORT: &str = "7500";

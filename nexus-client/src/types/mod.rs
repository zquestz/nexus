//! Type definitions for the Nexus client

mod bookmark;
mod channel;
pub mod connection;
mod display;
mod message;
mod panel;
mod pending;
mod ui;
mod view_config;
mod voice;

// Re-export types for convenience
pub use bookmark::{BookmarkEditMode, BookmarkEditState, ServerBookmark};
pub use channel::ChannelState;
pub use connection::{
    ConnectionInfo, NetworkConnection, ServerConnection, ServerConnectionParams, TabCompletionState,
};
pub use display::{ChatMessage, ChatTab, MessageType, ScrollState, UserInfo};
pub use message::Message;
pub use nexus_common::protocol::ServerMessage;
pub use panel::{
    BanDuration, ClipboardItem, ClipboardOperation, ConnectionFormState,
    ConnectionMonitorSortColumn, ConnectionMonitorState, ConnectionMonitorTab, DisconnectAction,
    DisconnectDialogState, FileSortColumn, FileTab, FilesManagementState, GroupManagementMode,
    GroupManagementSortColumn, NewsManagementMode, NewsManagementState, PasswordChangeState,
    PendingOverwrite, ServerInfoEditState, ServerInfoParams, ServerInfoTab, SettingsFormState,
    SettingsTab, TabId, TrackerEditInit, TrackerManagementMode, TrackerManagementSortColumn,
    TrackerManagementState, TransferSortColumn, UserEditInit, UserManagementMode,
    UserManagementSortColumn, UserManagementState, UserManagementTab,
};
pub use pending::{PendingRequests, ResponseRouting};
pub use ui::{ActivePanel, FingerprintMismatch, InputId, ReconnectAction, ScrollableId, UiState};
pub use view_config::{ToolbarState, ViewConfig};
pub use voice::VoiceState;

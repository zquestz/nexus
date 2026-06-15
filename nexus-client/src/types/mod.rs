//! Type definitions for the Nexus client

mod bookmark;
mod channel;
pub mod connection;
mod display;
mod fingerprint;
mod message;
mod panel;
mod pending;
mod tracker;
mod ui;
mod view_config;
mod voice;

// Re-export types for convenience
pub use bookmark::{BookmarkEditMode, BookmarkEditState, ServerBookmark};
pub use channel::ChannelState;
pub use connection::{
    ConnectionInfo, NetworkConnection, PendingDisconnectError, ServerConnection,
    ServerConnectionParams, TabCompletionState,
};
pub use display::{ChatMessage, ChatTab, MessageType, ScrollState, UserInfo};
pub use fingerprint::normalize_certificate_fingerprint;
pub use message::Message;
pub use nexus_common::protocol::ServerMessage;
pub use panel::{
    BanDuration, ClipboardItem, ClipboardOperation, ConnectionFormState,
    ConnectionMonitorSortColumn, ConnectionMonitorState, ConnectionMonitorTab, DisconnectAction,
    DisconnectDialogState, FileSortColumn, FileTab, FilesManagementState, GroupManagementMode,
    GroupManagementSortColumn, NewsManagementMode, NewsManagementState, PasswordChangeState,
    PendingOverwrite, ServerInfoEditState, ServerInfoParams, ServerInfoTab, SettingsFormState,
    SettingsTab, TabId, TrackerBrowserEditInit, TrackerBrowserMode, TrackerBrowserSortColumn,
    TrackerBrowserState, TrackerCacheEntry, TrackerEditInit, TrackerManagementMode,
    TrackerManagementSortColumn, TrackerManagementState, TransferSortColumn, UserEditInit,
    UserManagementMode, UserManagementSortColumn, UserManagementState, UserManagementTab,
    format_bytes_per_sec_as_mbps,
};
pub use pending::{PendingRequests, ResponseRouting};
pub use tracker::ClientTracker;
pub use ui::{
    ActivePanel, FingerprintMismatch, InputId, ManualConnectionIntent, ReconnectAction,
    ScrollableId, UiState,
};
pub use view_config::{ToolbarState, ViewConfig};
pub use voice::VoiceState;

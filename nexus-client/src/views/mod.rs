//! UI view rendering components

mod about;
mod bookmark;
mod broadcast;
mod chat;
mod connection;
mod connection_monitor;
pub(crate) mod constants;
mod disconnect_dialog;
pub(crate) mod files;
mod fingerprint;
mod fingerprint_interception;
pub(crate) mod groups;
pub(crate) mod helpers;
mod layout;
mod news;
pub(crate) mod password_strength;
mod server_info;
mod server_list;
mod settings;
mod tracker_browser;
mod trackers;
pub(crate) mod transfers;
mod user_info;
mod user_list;
mod users;
pub(crate) mod voice;

// Re-export the main layout function and fingerprint dialogs (public API)
pub use fingerprint::fingerprint_mismatch_dialog;
pub use fingerprint_interception::fingerprint_interception_dialog;
pub use layout::main_layout;

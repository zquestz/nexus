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
pub(crate) mod groups;
mod layout;
mod news;
mod server_info;
mod server_list;
mod settings;
pub(crate) mod transfers;
mod user_info;
mod user_list;
mod users;
pub(crate) mod voice;

// Re-export the main layout function and fingerprint dialog (public API)
pub use fingerprint::fingerprint_mismatch_dialog;
pub use layout::main_layout;
